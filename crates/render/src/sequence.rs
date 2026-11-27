//! M46 phase 7: entrances and exits by paragraph or by letter.
//!
//! A text item is split into one copy per part. Each copy keeps the whole text, so the layout
//! is the same, but only its own part's characters are visible (the others are drawn
//! transparent). The first copy also keeps the shape's fill, outline, shadow and glow, so the
//! shape comes with the first paragraph. Each copy then gets its own animation sample from
//! `coursecraft_logic::sample_timed_parts`.

use coursecraft_course::Sequence;
use coursecraft_logic::{MAX_SEQUENCE_PARTS, ObjectSample};

use crate::compose::{DrawItem, ResolvedTextRun, Visual, apply_sample_to_items};

fn runs_of(item: &DrawItem) -> Option<&Vec<ResolvedTextRun>> {
    match &item.visual {
        Visual::Text { runs, .. } => Some(runs),
        Visual::Shape {
            text: Some(text), ..
        } => Some(&text.runs),
        _ => None,
    }
}

/// The part index of every character of `runs`, in order, and how many parts there are.
fn units(runs: &[ResolvedTextRun], seq: Sequence) -> (Vec<usize>, usize) {
    let text: String = runs.iter().map(|r| r.text.as_str()).collect();
    match seq {
        Sequence::AsOne => (vec![0; text.chars().count()], 1),
        Sequence::ByParagraph => {
            // Paragraphs with only white space join the paragraph before them.
            let mut out = Vec::with_capacity(text.len());
            let mut part = 0usize;
            let mut seen = false;
            for para in text.split('\n') {
                let has_text = para.chars().any(|c| !c.is_whitespace());
                if has_text && seen {
                    part += 1;
                }
                seen |= has_text;
                out.extend(std::iter::repeat_n(part, para.chars().count()));
                out.push(part); // the '\n' (one too many at the end; trimmed below)
            }
            out.truncate(text.chars().count());
            (out, if seen { part + 1 } else { 1 })
        }
        Sequence::ByLetter => {
            let letters = text.chars().filter(|c| !c.is_whitespace()).count().max(1);
            let groups = letters.min(MAX_SEQUENCE_PARTS);
            let mut out = Vec::with_capacity(text.len());
            let mut seen = 0usize;
            for c in text.chars() {
                out.push((seen.min(letters - 1)) * groups / letters);
                if !c.is_whitespace() {
                    seen += 1;
                }
            }
            (out, groups)
        }
    }
}

/// How many parts `item` plays in with `seq` (1 when it has no text to split).
#[must_use]
pub fn part_count(item: &DrawItem, seq: Sequence) -> usize {
    runs_of(item).map_or(1, |runs| units(runs, seq).1)
}

/// `runs` with every character outside part `k` made transparent.
fn only_part(runs: &[ResolvedTextRun], unit: &[usize], k: usize) -> Vec<ResolvedTextRun> {
    let mut out = Vec::with_capacity(runs.len());
    let mut i = 0usize;
    for run in runs {
        let mut cur: Option<(bool, String)> = None;
        for c in run.text.chars() {
            let mine = unit.get(i).is_none_or(|&u| u == k);
            i += 1;
            match &mut cur {
                Some((m, s)) if *m == mine => s.push(c),
                _ => {
                    if let Some((m, s)) = cur.take() {
                        out.push(piece(run, s, m));
                    }
                    cur = Some((mine, c.to_string()));
                }
            }
        }
        if let Some((m, s)) = cur {
            out.push(piece(run, s, m));
        }
    }
    out
}

fn piece(run: &ResolvedTextRun, text: String, visible: bool) -> ResolvedTextRun {
    let mut r = run.clone();
    r.text = text;
    if !visible {
        r.color = egui::Color32::TRANSPARENT;
        r.underline = false;
        r.strikethrough = false;
        r.highlight = None;
        r.shadow = false;
        // Text that is not on yet is not a link yet.
        r.link = None;
    }
    r
}

/// `item` split into its parts, or `None` when it has fewer than two.
#[must_use]
pub fn split_parts(item: &DrawItem, seq: Sequence) -> Option<Vec<DrawItem>> {
    let runs = runs_of(item)?;
    let (unit, n) = units(runs, seq);
    if n < 2 {
        return None;
    }
    let parts = (0..n)
        .map(|k| {
            let mut part = item.clone();
            let kept = only_part(runs, &unit, k);
            match &mut part.visual {
                Visual::Text { runs, outline, .. } => {
                    *runs = kept;
                    if k > 0 {
                        *outline = None;
                    }
                }
                Visual::Shape {
                    text: Some(text),
                    fill,
                    fill_style,
                    outline,
                    hover_fill,
                    down_fill,
                    ..
                } => {
                    text.runs = kept;
                    if k > 0 {
                        *fill = egui::Color32::TRANSPARENT;
                        *fill_style = None;
                        *outline = None;
                        *hover_fill = None;
                        *down_fill = None;
                    }
                }
                _ => {}
            }
            if k > 0 {
                part.shadow = None;
                part.glow = None;
            }
            part
        })
        .collect();
    Some(parts)
}

/// Apply an object's animation to its items (`items[0]` the object, the rest its state
/// extras). With a `sequence`, the object is split into parts and `parts(n)` gives each one's
/// sample; extras follow the first part. Otherwise, or when the text doesn't split, `whole`
/// applies to everything.
pub fn apply_object_samples(
    items: &mut Vec<DrawItem>,
    seq: Option<Sequence>,
    whole: &ObjectSample,
    parts: impl FnOnce(usize) -> Option<Vec<ObjectSample>>,
) {
    let split = seq.and_then(|seq| {
        let first = items.first()?;
        let pieces = split_parts(first, seq)?;
        let samples = parts(pieces.len())?;
        (samples.len() == pieces.len()).then_some((pieces, samples))
    });
    let Some((mut pieces, samples)) = split else {
        apply_sample_to_items(items, whole);
        return;
    };
    // The first part and the state extras move together (extras about the part's centre).
    let mut group: Vec<DrawItem> = std::iter::once(pieces.remove(0))
        .chain(items.drain(1..))
        .collect();
    let mut samples = samples.into_iter();
    if let Some(s) = samples.next() {
        apply_sample_to_items(&mut group, &s);
    }
    for (piece, sample) in pieces.iter_mut().zip(samples) {
        apply_sample_to_items(std::slice::from_mut(piece), &sample);
    }
    group.extend(pieces);
    *items = group;
}

/// The parts that are not invisible, for counting what a frame shows (tests).
#[cfg(test)]
fn visible_text(item: &DrawItem) -> String {
    runs_of(item)
        .map(|runs| {
            runs.iter()
                .filter(|r| r.color.a() > 0)
                .map(|r| r.text.as_str())
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_item(text: &str) -> DrawItem {
        let json = serde_json::json!({
            "schema_version": 1, "id": "s", "title": "S",
            "slide_size": { "w": 1000, "h": 600 },
            "scenes": [{ "id": "sc", "title": "S", "slides": [{
                "id": "sl", "title": "One", "duration_ms": 5000, "objects": [
                    { "id": "t", "kind": { "type": "text", "text": text },
                      "frame": { "x": 100, "y": 100, "w": 400, "h": 200 } }
                ] }] }]
        });
        let c = coursecraft_course::Course::from_json(&json.to_string()).expect("course");
        let slide = c.slide("sl").expect("slide");
        crate::compose_authoring(&c, slide, None)
            .into_iter()
            .find(|i| i.hit == crate::Hit::Object("t".into()))
            .expect("text item")
    }

    #[test]
    fn paragraphs_split_into_parts_that_add_up_to_the_text() {
        let item = text_item("First line\n\nSecond line\nThird");
        assert_eq!(part_count(&item, Sequence::ByParagraph), 3);
        let parts = split_parts(&item, Sequence::ByParagraph).expect("parts");
        let shown: Vec<String> = parts.iter().map(visible_text).collect();
        assert_eq!(shown[0].trim(), "First line");
        assert_eq!(shown[1].trim(), "Second line");
        assert_eq!(shown[2].trim(), "Third");
        // Every part keeps the full text for layout.
        for p in &parts {
            let all: String = runs_of(p)
                .expect("runs")
                .iter()
                .map(|r| r.text.as_str())
                .collect();
            assert_eq!(all, "First line\n\nSecond line\nThird");
        }
        assert!(split_parts(&item, Sequence::AsOne).is_none());
    }

    #[test]
    fn parts_keep_the_link_only_on_their_own_text() {
        let mut item = text_item("First\nSecond");
        if let Visual::Text { runs, .. } = &mut item.visual {
            for run in runs.iter_mut() {
                run.link = Some(coursecraft_course::Link::Url {
                    url: "https://example.com".into(),
                });
            }
        }
        let parts = split_parts(&item, Sequence::ByParagraph).expect("parts");
        for part in &parts {
            for run in runs_of(part).expect("runs") {
                assert_eq!(
                    run.link.is_some(),
                    run.color.a() > 0,
                    "run {:?}: a link only where the text shows",
                    run.text
                );
            }
        }
    }

    #[test]
    fn letters_split_one_per_letter_up_to_the_cap() {
        let item = text_item("Hi you");
        assert_eq!(part_count(&item, Sequence::ByLetter), 5);
        let parts = split_parts(&item, Sequence::ByLetter).expect("parts");
        assert_eq!(visible_text(&parts[2]).trim(), "y");
        let long = text_item(&"a".repeat(1000));
        assert_eq!(part_count(&long, Sequence::ByLetter), MAX_SEQUENCE_PARTS);
        let one = text_item("Single paragraph");
        assert!(split_parts(&one, Sequence::ByParagraph).is_none());
    }

    #[test]
    fn samples_go_to_their_parts_and_extras_follow_the_first() {
        let item = text_item("A\nB");
        let mut items = vec![item];
        let hidden = ObjectSample {
            opacity: 0.0,
            ..ObjectSample::IDENTITY
        };
        apply_object_samples(
            &mut items,
            Some(Sequence::ByParagraph),
            &ObjectSample::IDENTITY,
            |n| {
                Some(
                    vec![ObjectSample::IDENTITY, hidden.clone()]
                        .into_iter()
                        .take(n)
                        .collect(),
                )
            },
        );
        assert_eq!(items.len(), 2);
        assert!((items[0].opacity - 1.0).abs() < 1e-6);
        assert!(items[1].opacity.abs() < 1e-6);
    }
}
