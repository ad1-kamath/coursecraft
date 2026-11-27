//! M39 package T: `TextRun.highlight` and `TextRun.shadow`, and the helpers behind
//! Clear Formatting and Change Case.

use crate::text::{
    CaseChange, apply_style_to_range, change_case_in_range, clear_formatting_in_range,
    normalize_runs,
};
use crate::{Color, Course, Link, Severity, TextRun, TextRunPatch};

fn run(text: &str) -> TextRun {
    serde_json::from_value(serde_json::json!({ "text": text })).expect("run")
}

fn course_with_runs(runs: serde_json::Value) -> Course {
    let raw = serde_json::json!({
        "schema_version": 1,
        "id": "c-m39-t",
        "title": "M39 T",
        "slide_size": { "w": 1280, "h": 720 },
        "scenes": [{
            "id": "scene-1",
            "title": "Scene 1",
            "slides": [{
                "id": "s1",
                "title": "Slide 1",
                "objects": [{
                    "id": "t1",
                    "kind": { "type": "text", "text": "ab", "runs": runs },
                    "frame": { "x": 0, "y": 0, "w": 100, "h": 40 }
                }]
            }]
        }]
    });
    serde_json::from_value(raw).expect("course")
}

#[test]
fn highlight_and_shadow_round_trip_and_are_omitted_when_none() {
    let plain = run("x");
    let json = serde_json::to_value(&plain).expect("json");
    assert!(json.get("highlight").is_none() && json.get("shadow").is_none());

    let r: TextRun = serde_json::from_value(
        serde_json::json!({ "text": "x", "highlight": "#FFFF00", "shadow": true }),
    )
    .expect("run");
    assert_eq!(r.highlight, Some(Color("#FFFF00".into())));
    assert_eq!(r.shadow, Some(true));
    assert!(!r.is_plain());
    let back: TextRun =
        serde_json::from_value(serde_json::to_value(&r).expect("json")).expect("back");
    assert_eq!(back, r);
}

#[test]
fn an_old_run_without_the_fields_loads_unchanged() {
    let r: TextRun =
        serde_json::from_str(r##"{"text":"old","bold":true,"color":"#112233"}"##).expect("run");
    assert!(r.highlight.is_none() && r.shadow.is_none());
    let json = serde_json::to_string(&r).expect("json");
    assert!(!json.contains("highlight") && !json.contains("shadow"));
}

#[test]
fn normalize_merges_only_runs_with_the_same_highlight_and_shadow() {
    let mut a = run("a");
    a.highlight = Some(Color("#FFFF00".into()));
    let mut b = run("b");
    b.highlight = Some(Color("#FFFF00".into()));
    let c = run("c");
    let mut d = run("d");
    d.shadow = Some(true);
    let merged = normalize_runs(&[a, b, c, d]);
    assert_eq!(merged.len(), 3);
    assert_eq!(merged[0].text, "ab");
    assert_eq!(merged[1].text, "c");
    assert_eq!(merged[2].text, "d");
}

#[test]
fn the_patch_sets_and_clears_highlight() {
    let on = TextRunPatch {
        highlight: Some(Some(Color("#00FF00".into()))),
        shadow: Some(true),
        ..TextRunPatch::default()
    };
    let runs = apply_style_to_range(&[], "hello", (1, 3), &on);
    assert_eq!(runs.len(), 3);
    assert_eq!(runs[1].text, "el");
    assert_eq!(runs[1].highlight, Some(Color("#00FF00".into())));
    assert_eq!(runs[1].shadow, Some(true));
    let off = TextRunPatch {
        highlight: Some(None),
        shadow: Some(false),
        ..TextRunPatch::default()
    };
    let cleared = crate::apply_style_to_range_with_base(
        &runs,
        "hello",
        (0, 0),
        &off,
        Some(&crate::TextStyle::default()),
    );
    assert!(
        cleared.is_empty(),
        "no overrides left means plain text: {cleared:?}"
    );
}

#[test]
fn typing_inside_a_highlighted_run_continues_the_highlight() {
    let mut a = run("ab");
    a.highlight = Some(Color("#FFFF00".into()));
    a.shadow = Some(true);
    let edited = crate::apply_text_edit(&[a], "ab", "axb");
    assert_eq!(edited.len(), 1);
    assert_eq!(edited[0].text, "axb");
    assert_eq!(edited[0].shadow, Some(true));
}

#[test]
fn validation_rejects_a_bad_highlight_colour() {
    let good = course_with_runs(serde_json::json!([
        { "text": "ab", "highlight": "#FFFF00", "shadow": true }
    ]));
    assert!(
        good.validate()
            .iter()
            .all(|issue| issue.severity != Severity::Error),
        "{:?}",
        good.validate()
    );
    let bad = course_with_runs(serde_json::json!([
        { "text": "ab", "highlight": "yellow" }
    ]));
    assert!(
        bad.validate()
            .iter()
            .any(|issue| issue.severity == Severity::Error && issue.path.contains("highlight"))
    );
}

#[test]
fn clear_formatting_keeps_links_and_text() {
    let mut a = run("ab");
    a.bold = Some(true);
    a.highlight = Some(Color("#FFFF00".into()));
    let mut b = run("cd");
    b.link = Some(Link::Url {
        url: "https://example.com".into(),
    });
    b.shadow = Some(true);
    let cleared = clear_formatting_in_range(&[a, b], "abcd", (0, 0));
    assert_eq!(cleared.len(), 2);
    assert!(cleared[0].is_plain());
    assert_eq!(cleared[0].text, "ab");
    assert!(cleared[1].link.is_some() && cleared[1].shadow.is_none());
    // Nothing left but plain text: no runs.
    assert!(clear_formatting_in_range(&cleared[..1], "ab", (0, 0)).is_empty());
}

#[test]
fn change_case_handles_titles_sentences_and_apostrophes() {
    let (text, _) = change_case_in_range(&[], "don't STOP. it's 3rd", (0, 0), CaseChange::Title);
    assert_eq!(text, "Don't Stop. It's 3rd");
    let (text, _) = change_case_in_range(&[], "hELLO. WHO? me", (0, 0), CaseChange::Sentence);
    assert_eq!(text, "Hello. Who? Me");
    let (text, _) = change_case_in_range(&[], "aBc", (0, 0), CaseChange::Toggle);
    assert_eq!(text, "AbC");
}

#[test]
fn change_case_of_a_range_still_knows_the_word_start_before_it() {
    // Title case on "llo" inside "hello" must not capitalise the "l".
    let (text, _) = change_case_in_range(&[], "hello", (2, 5), CaseChange::Title);
    assert_eq!(text, "hello");
}

#[test]
fn applying_a_named_style_with_clear_overrides_also_clears_highlight_and_shadow() {
    let mut kind: crate::ObjectKind = serde_json::from_value(serde_json::json!({
        "type": "text",
        "text": "abc",
        "runs": [{
            "text": "abc", "bold": true, "highlight": "#FFFF00", "shadow": true,
            "link": { "type": "url", "url": "https://example.com" }
        }]
    }))
    .expect("kind");
    let named: crate::NamedTextStyle = serde_json::from_value(serde_json::json!({
        "id": "n1", "name": "N", "style": {}
    }))
    .expect("named");
    assert!(crate::apply_named_style(&mut kind, &named, true));
    let crate::ObjectKind::Text { runs, .. } = &kind else {
        panic!("text");
    };
    assert_eq!(runs.len(), 1, "the link keeps its run");
    assert_eq!(runs[0].bold, None);
    assert_eq!(runs[0].highlight, None);
    assert_eq!(runs[0].shadow, None);
    assert!(runs[0].link.is_some());
}

#[test]
fn title_case_keeps_a_curly_apostrophe_inside_the_word() {
    let (text, _) = change_case_in_range(&[], "don’t STOP, it’s ok", (0, 0), CaseChange::Title);
    assert_eq!(text, "Don’t Stop, It’s Ok");
    let (text, _) = change_case_in_range(&[], "donʼt", (0, 0), CaseChange::Title);
    assert_eq!(text, "Donʼt");
}

#[test]
fn sentence_case_does_not_start_a_sentence_after_an_abbreviation() {
    let (text, _) = change_case_in_range(
        &[],
        "USE TOOLS, e.g. foo. i.e. bar. next",
        (0, 0),
        CaseChange::Sentence,
    );
    assert_eq!(text, "Use tools, e.g. foo. I.e. bar. Next");
    let (text, _) =
        change_case_in_range(&[], "version 1.5 is out. ok", (0, 0), CaseChange::Sentence);
    assert_eq!(text, "Version 1.5 is out. Ok");
}
