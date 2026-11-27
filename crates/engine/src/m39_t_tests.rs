//! M39 package T: text highlight and shadow, Clear Formatting, Change Case, numbering
//! schemes, and the format painter carrying highlight and shadow.

use coursecraft_course::{Link, ListKind, NumberScheme, ObjectKind, TextRun};
use serde_json::{Value as Json, json};

use crate::Session;

fn ok(s: &mut Session, id: &str, p: Json) -> Json {
    s.execute(id, &p)
        .unwrap_or_else(|e| panic!("{id} failed: {e}"))
}

fn add_text(s: &mut Session, kind: Json) -> String {
    ok(
        s,
        "object.add",
        json!({ "slide": "slide1", "kind": kind, "frame": { "x": 10, "y": 10, "w": 300, "h": 60 } }),
    )["id"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

fn text_of(s: &Session, id: &str) -> (String, Vec<TextRun>) {
    let obj = s
        .course
        .slide("slide1")
        .and_then(|sl| sl.object(id))
        .unwrap();
    match &obj.0.kind {
        ObjectKind::Text { text, runs, .. } => (text.clone(), runs.clone()),
        other => panic!("{other:?}"),
    }
}

#[test]
fn style_range_sets_and_clears_highlight_and_shadow_in_one_undo_step() {
    let mut s = Session::blank("T");
    let id = add_text(&mut s, json!({ "type": "text", "text": "Hello world" }));
    let before = s.undo_len();
    ok(
        &mut s,
        "text.style_range",
        json!({ "slide": "slide1", "object": id, "start": 0, "end": 5,
                "highlight": "#FFFF00", "shadow": true }),
    );
    assert_eq!(s.undo_len(), before + 1);
    let (_, runs) = text_of(&s, &id);
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].text, "Hello");
    assert_eq!(
        runs[0].highlight.as_ref().map(|c| c.0.as_str()),
        Some("#FFFF00")
    );
    assert_eq!(runs[0].shadow, Some(true));
    assert!(runs[1].is_plain());

    // `null` removes the highlight; shadow false collapses back to plain text.
    ok(
        &mut s,
        "text.style_range",
        json!({ "slide": "slide1", "object": id, "start": 0, "end": 5,
                "highlight": null, "shadow": false }),
    );
    assert!(text_of(&s, &id).1.is_empty());
    s.undo().unwrap();
    assert_eq!(text_of(&s, &id).1.len(), 2);
    s.undo().unwrap();
    assert!(text_of(&s, &id).1.is_empty());
}

#[test]
fn style_range_rejects_a_bad_highlight_colour() {
    let mut s = Session::blank("T");
    let id = add_text(&mut s, json!({ "type": "text", "text": "Hello" }));
    let err = s.execute(
        "text.style_range",
        &json!({ "slide": "slide1", "object": id, "start": 0, "end": 0, "highlight": "yellow" }),
    );
    assert!(err.is_err());
    let err = s.execute(
        "text.style_range",
        &json!({ "slide": "slide1", "object": id, "start": 0, "end": 0, "highlight": 5 }),
    );
    assert!(err.is_err());
}

#[test]
fn clear_formatting_removes_overrides_but_keeps_links() {
    let mut s = Session::blank("T");
    let id = add_text(
        &mut s,
        json!({ "type": "text", "text": "abcdef", "runs": [
            { "text": "abc", "bold": true, "highlight": "#FFFF00", "shadow": true, "size": 40.0 },
            { "text": "def", "italic": true, "link": { "type": "url", "url": "https://example.com" },
              "color": "#FF0000" } ] }),
    );
    // A range covering the middle: only those characters are cleared.
    ok(
        &mut s,
        "text.clear_formatting",
        json!({ "slide": "slide1", "object": id, "start": 1, "end": 5 }),
    );
    let (_, runs) = text_of(&s, &id);
    let joined: String = runs.iter().map(|r| r.text.as_str()).collect();
    assert_eq!(joined, "abcdef");
    assert_eq!(runs[0].text, "a");
    assert_eq!(runs[0].bold, Some(true));
    assert_eq!(runs[1].text, "bc");
    assert!(runs[1].is_plain());
    assert_eq!(runs[2].text, "de");
    assert!(matches!(runs[2].link, Some(Link::Url { .. })));
    assert!(runs[2].italic.is_none() && runs[2].color.is_none());
    assert_eq!(runs[3].text, "f");
    assert_eq!(runs[3].italic, Some(true));

    // 0..0 is the whole text; the link survives, everything else goes.
    let before = s.undo_len();
    ok(
        &mut s,
        "text.clear_formatting",
        json!({ "slide": "slide1", "object": id, "start": 0, "end": 0 }),
    );
    assert_eq!(s.undo_len(), before + 1);
    let (_, runs) = text_of(&s, &id);
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].text, "abc");
    assert!(runs[0].is_plain());
    assert_eq!(runs[1].text, "def");
    assert!(matches!(runs[1].link, Some(Link::Url { .. })));
    assert!(runs[1].italic.is_none() && runs[1].color.is_none());
    s.undo().unwrap();
    assert_eq!(text_of(&s, &id).1.len(), 4);
}

#[test]
fn clear_formatting_without_a_link_returns_plain_text() {
    let mut s = Session::blank("T");
    let id = add_text(
        &mut s,
        json!({ "type": "text", "text": "ab", "runs": [{ "text": "ab", "bold": true }] }),
    );
    ok(
        &mut s,
        "text.clear_formatting",
        json!({ "slide": "slide1", "object": id, "start": 0, "end": 0 }),
    );
    assert!(text_of(&s, &id).1.is_empty());
}

fn case_of(s: &mut Session, id: &str, case: &str, range: (usize, usize)) -> String {
    ok(
        s,
        "text.change_case",
        json!({ "slide": "slide1", "object": id, "start": range.0, "end": range.1, "case": case }),
    );
    text_of(s, id).0
}

#[test]
fn change_case_modes() {
    let mut s = Session::blank("T");
    let id = add_text(
        &mut s,
        json!({ "type": "text", "text": "hello WORLD. second sentence" }),
    );
    assert_eq!(
        case_of(&mut s, &id, "upper", (0, 0)),
        "HELLO WORLD. SECOND SENTENCE"
    );
    assert_eq!(
        case_of(&mut s, &id, "lower", (0, 0)),
        "hello world. second sentence"
    );
    assert_eq!(
        case_of(&mut s, &id, "title", (0, 0)),
        "Hello World. Second Sentence"
    );
    assert_eq!(
        case_of(&mut s, &id, "sentence", (0, 0)),
        "Hello world. Second sentence"
    );
    assert_eq!(
        case_of(&mut s, &id, "toggle", (0, 0)),
        "hELLO WORLD. sECOND SENTENCE"
    );
    // A range only changes its own characters.
    case_of(&mut s, &id, "lower", (0, 0));
    assert_eq!(
        case_of(&mut s, &id, "upper", (0, 5)),
        "HELLO world. second sentence"
    );
    assert!(
        s.execute(
            "text.change_case",
            &json!({ "slide": "slide1", "object": id, "start": 0, "end": 0, "case": "weird" })
        )
        .is_err()
    );
}

#[test]
fn change_case_keeps_runs_and_paragraph_breaks_and_undoes() {
    let mut s = Session::blank("T");
    let id = add_text(
        &mut s,
        json!({ "type": "text", "text": "one two\nthree", "runs": [
            { "text": "one ", "bold": true }, { "text": "two\nthr", "italic": true },
            { "text": "ee" } ],
            "paragraphs": [{ "list": "bullet" }, {}] }),
    );
    let before = s.undo_len();
    ok(
        &mut s,
        "text.change_case",
        json!({ "slide": "slide1", "object": id, "start": 0, "end": 0, "case": "upper" }),
    );
    assert_eq!(s.undo_len(), before + 1);
    let (text, runs) = text_of(&s, &id);
    assert_eq!(text, "ONE TWO\nTHREE");
    assert_eq!(runs.len(), 3);
    assert_eq!(runs[0].text, "ONE ");
    assert_eq!(runs[0].bold, Some(true));
    assert_eq!(runs[1].text, "TWO\nTHR");
    assert_eq!(runs[1].italic, Some(true));
    assert_eq!(runs[2].text, "EE");
    let obj = s.course.slide("slide1").unwrap().object(&id).unwrap().0;
    if let ObjectKind::Text { paragraphs, .. } = &obj.kind {
        assert_eq!(paragraphs[0].list, ListKind::Bullet);
    }
    s.undo().unwrap();
    assert_eq!(text_of(&s, &id).0, "one two\nthree");
}

#[test]
fn change_case_maps_length_changing_characters_per_run() {
    let mut s = Session::blank("T");
    let id = add_text(
        &mut s,
        json!({ "type": "text", "text": "straße!", "runs": [
            { "text": "stra", "bold": true }, { "text": "ße", "italic": true },
            { "text": "!" } ] }),
    );
    let text = case_of(&mut s, &id, "upper", (0, 0));
    assert_eq!(text, "STRASSE!");
    let (_, runs) = text_of(&s, &id);
    assert_eq!(runs.len(), 3);
    assert_eq!(runs[0].text, "STRA");
    assert_eq!(runs[0].bold, Some(true));
    assert_eq!(runs[1].text, "SSE");
    assert_eq!(runs[1].italic, Some(true));
    assert_eq!(runs[2].text, "!");
    let joined: String = runs.iter().map(|r| r.text.as_str()).collect();
    assert_eq!(joined, text);
}

#[test]
fn change_case_refits_point_text() {
    let mut s = Session::blank("T");
    let id = add_text(
        &mut s,
        json!({ "type": "text", "text": "iiiiiiii", "point_text": true }),
    );
    let width = |s: &Session| {
        s.course
            .slide("slide1")
            .unwrap()
            .object(&id)
            .unwrap()
            .0
            .frame
            .w
    };
    ok(
        &mut s,
        "text.style_range",
        json!({ "slide": "slide1", "object": id, "start": 0, "end": 0, "size": 20.0 }),
    );
    let narrow = width(&s);
    case_of(&mut s, &id, "upper", (0, 0));
    let wide = width(&s);
    assert!(
        wide >= narrow,
        "upper case is not narrower: {narrow} {wide}"
    );
}

#[test]
fn set_paragraphs_takes_a_number_scheme_and_start_at() {
    let mut s = Session::blank("T");
    let id = add_text(&mut s, json!({ "type": "text", "text": "a\nb" }));
    ok(
        &mut s,
        "text.set_paragraphs",
        json!({ "slide": "slide1", "object": id, "list": "number",
                "number_scheme": "roman_uc_period", "start_at": 4 }),
    );
    let obj = s.course.slide("slide1").unwrap().object(&id).unwrap().0;
    let ObjectKind::Text { paragraphs, .. } = &obj.kind else {
        panic!("text")
    };
    assert!(paragraphs.iter().all(|p| p.list == ListKind::Number
        && p.number_scheme == Some(NumberScheme::RomanUcPeriod)
        && p.start_at == Some(4)));
    ok(
        &mut s,
        "text.set_paragraphs",
        json!({ "slide": "slide1", "object": id, "number_scheme": null, "start_at": null }),
    );
    let obj = s.course.slide("slide1").unwrap().object(&id).unwrap().0;
    let ObjectKind::Text { paragraphs, .. } = &obj.kind else {
        panic!("text")
    };
    assert!(
        paragraphs
            .iter()
            .all(|p| p.number_scheme.is_none() && p.start_at.is_none())
    );
    for bad in [json!(0), json!(100_000), json!("x")] {
        assert!(
            s.execute(
                "text.set_paragraphs",
                &json!({ "slide": "slide1", "object": id, "start_at": bad })
            )
            .is_err()
        );
    }
    assert!(
        s.execute(
            "text.set_paragraphs",
            &json!({ "slide": "slide1", "object": id, "number_scheme": "nope" })
        )
        .is_err()
    );
}

#[test]
fn format_painter_carries_highlight_and_shadow() {
    let mut s = Session::blank("T");
    let source = add_text(
        &mut s,
        json!({ "type": "text", "text": "Source", "runs": [
            { "text": "Source", "highlight": "#00FF00", "shadow": true }] }),
    );
    let target = add_text(
        &mut s,
        json!({ "type": "text", "text": "Target", "runs": [
            { "text": "Tar", "highlight": "#FF0000" }, { "text": "get", "bold": true }] }),
    );
    ok(
        &mut s,
        "object.apply_style",
        json!({ "slide": "slide1", "source_slide": "slide1", "source": source,
                "objects": [target] }),
    );
    let (text, runs) = text_of(&s, &target);
    assert_eq!(text, "Target");
    assert_eq!(runs.len(), 1, "one uniform run");
    assert_eq!(
        runs[0].highlight.as_ref().map(|c| c.0.as_str()),
        Some("#00FF00")
    );
    assert_eq!(runs[0].shadow, Some(true));
    assert!(runs[0].bold.is_none());

    // A source without them clears the target's.
    let plain = add_text(&mut s, json!({ "type": "text", "text": "Plain" }));
    ok(
        &mut s,
        "object.apply_style",
        json!({ "slide": "slide1", "source_slide": "slide1", "source": plain,
                "objects": [target] }),
    );
    assert!(text_of(&s, &target).1.iter().all(TextRun::is_plain));
}

#[test]
fn highlight_and_shadow_survive_find_and_replace() {
    let mut s = Session::blank("T");
    let id = add_text(
        &mut s,
        json!({ "type": "text", "text": "cat dog", "runs": [
            { "text": "cat", "highlight": "#FFFF00", "shadow": true }, { "text": " dog" }] }),
    );
    ok(
        &mut s,
        "course.replace",
        json!({ "query": "cat", "replacement": "bird" }),
    );
    let (text, runs) = text_of(&s, &id);
    assert_eq!(text, "bird dog");
    assert_eq!(runs[0].text, "bird");
    assert_eq!(runs[0].shadow, Some(true));
    assert!(runs[0].highlight.is_some());
}

#[test]
fn an_empty_or_out_of_range_selection_changes_nothing_but_0_0_means_all() {
    let mut s = Session::blank("T");
    let id = add_text(
        &mut s,
        json!({ "type": "text", "text": "hello", "runs": [{ "text": "hel", "bold": true }, { "text": "lo" }] }),
    );
    assert_eq!(case_of(&mut s, &id, "upper", (2, 2)), "hello");
    assert_eq!(case_of(&mut s, &id, "upper", (10, 20)), "hello");
    assert_eq!(case_of(&mut s, &id, "upper", (5, 5)), "hello");
    ok(
        &mut s,
        "text.clear_formatting",
        json!({ "slide": "slide1", "object": id, "start": 2, "end": 2 }),
    );
    assert_eq!(text_of(&s, &id).1.first().and_then(|r| r.bold), Some(true));
    assert_eq!(case_of(&mut s, &id, "upper", (0, 0)), "HELLO");
    assert_eq!(case_of(&mut s, &id, "lower", (3, 99)), "HELlo");
}
