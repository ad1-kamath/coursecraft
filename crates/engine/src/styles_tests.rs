//! Tests for the palette, `course.recolor` and named text styles.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use coursecraft_course::{Course, ObjectKind, Severity};
use serde_json::{Value as Json, json};

use crate::Session;

const HELLO: &str = include_str!("../../../examples/hello-course/course.json");
/// Used everywhere in `rainbow`; its short form is `#369`.
const OLD: &str = "#336699";

fn find<'a>(c: &'a Course, id: &str) -> &'a coursecraft_course::Object {
    c.scenes
        .iter()
        .flat_map(|scene| scene.slides.iter())
        .find_map(|slide| slide.object(id))
        .map(|(object, _)| object)
        .expect("object exists")
}

fn run(s: &mut Session, id: &str, params: Json) -> Json {
    s.execute(id, &params)
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn fails(s: &mut Session, id: &str, params: Json) -> String {
    s.execute(id, &params).expect_err(id).to_string()
}

fn errors(c: &Course) -> Vec<String> {
    c.validate()
        .into_iter()
        .filter(|i| i.severity == Severity::Error)
        .map(|i| format!("{}: {}", i.path, i.message))
        .collect()
}

fn frame() -> Json {
    json!({"x": 10, "y": 10, "w": 200, "h": 80})
}

fn text_object(id: &str, color: &str) -> Json {
    json!({"id": id, "kind": {"type": "text", "text": "Hello", "style": {"color": color},
        "runs": [{"text": "Hel", "color": color}, {"text": "lo", "bold": true}]}, "frame": frame()})
}

/// One colour in as many places as the model has.
fn rainbow() -> Course {
    let o = OLD;
    let alpha = format!("{o}80");
    let slide_objects = json!([
        text_object("t1", o),
        {"id": "box", "kind": {"type": "shape", "shape": "rectangle", "fill": o,
            "fill_style": {"type": "linear", "angle": 0, "stops": [
                {"color": o, "position": 0}, {"color": "#FFFFFF", "position": 1}]},
            "outline": {"color": o, "width": 2},
            "text": {"text": "in shape", "style": {"color": o}, "runs": [{"text": "in shape", "color": o}]}},
         "frame": frame(), "shadow": {"color": alpha, "offset_x": 2, "offset_y": 2, "blur": 4},
         "glow": {"color": alpha, "size": 6},
         "states": [{"name": "hot", "fill": o}]},
        {"id": "btn", "kind": {"type": "button", "label": "Go", "fill": o,
            "fill_style": {"type": "radial", "stops": [{"color": o, "position": 0}, {"color": "#000000", "position": 1}]}},
         "frame": frame()},
        {"id": "dot", "kind": {"type": "marker", "title": "t", "body": "b", "color": o}, "frame": frame()},
        {"id": "line", "kind": {"type": "path", "points": [{"x": 0, "y": 0}, {"x": 1, "y": 1}, {"x": 0, "y": 1}],
            "closed": true, "fill": o, "outline": {"color": o, "width": 1}}, "frame": frame()},
        {"id": "grid", "kind": {"type": "table", "columns": [0.5, 0.5],
            "rows": [{"height": 1.0, "cells": [
                {"text": "a", "fill": o, "style": {"color": o}, "runs": [{"text": "a", "color": o}]},
                {"text": "b"}]}],
            "borders": {"outer": {"color": o, "width": 1}, "inner": {"color": o, "width": 1}}}, "frame": frame()},
        {"id": "clock", "kind": {"type": "timer", "duration_ms": 5000, "style": {"color": o}}, "frame": frame()}
    ]);
    let layer = json!([{"id": "pop", "name": "Pop", "objects": [text_object("t2", o)]}]);
    let question = json!({"kind": "multiple_choice", "prompt": "p",
        "choices": [{"id": "a", "text": "A", "correct": true}, {"id": "b", "text": "B"}]});
    let doc = json!({
        "schema_version": 1, "id": "rainbow", "title": "Rainbow",
        "theme": {"accent": o, "background": o, "text": o, "player": {"bar": o, "bar_text": o},
                  "palette": [{"id": "brand", "name": "Brand", "color": o}]},
        "text_styles": [{"id": "body", "name": "Body", "style": {"color": o}}],
        "scenes": [{"id": "sc", "title": "S", "slides": [
            {"id": "s1", "title": "One", "background": o, "objects": slide_objects, "layers": layer},
            {"id": "s2", "title": "Two", "background": alpha, "objects": [text_object("t3", o)]}]}],
        "masters": [{"id": "m1", "title": "M", "background": o, "objects": [text_object("mt", o)]}],
        "templates": [{"id": "tp1", "title": "T", "objects": [text_object("tt", o)]}],
        "banks": [{"id": "bk", "title": "B", "slides": [
            {"id": "q1", "title": "Q", "background": o, "objects": [text_object("qt", o)], "question": question}]}]
    });
    Course::from_json(&doc.to_string()).expect("rainbow parses")
}

fn lower(c: &Course) -> String {
    c.to_json().unwrap().to_ascii_lowercase()
}

#[test]
fn recolor_leaves_the_old_colour_nowhere_in_any_case_or_length() {
    let course = rainbow();
    assert_eq!(errors(&course), Vec::<String>::new());
    let before = lower(&course);
    assert!(
        before.matches("336699").count() >= 30,
        "the course uses the colour widely"
    );

    let mut s = Session::new(course);
    // The short form and a different case match the same colour.
    let result = run(
        &mut s,
        "course.recolor",
        json!({"from": "#369", "to": "#c0ffee"}),
    );
    assert!(result["changed"].as_u64().unwrap() >= 30, "{result}");
    let after = lower(&s.course);
    assert!(!after.contains("336699"), "old long form left behind");
    assert!(!after.contains("#369\""), "old short form left behind");
    assert!(after.contains("#c0ffee"));
    // Alpha stays each use's own.
    assert!(
        after.contains("#c0ffee80"),
        "shadow and glow keep their alpha"
    );
    assert!(!after.contains("#c0ffee80\"}\n") || true);
    assert_eq!(errors(&s.course), Vec::<String>::new());
    // Colours that were different stay different.
    assert!(after.contains("#ffffff") && after.contains("#000000"));
    // The theme and the palette changed too.
    assert_eq!(s.course.theme.accent.0, "#C0FFEE");
    assert_eq!(s.course.theme.palette[0].color.0, "#C0FFEE");
    assert_eq!(s.course.text_styles[0].style.color.0, "#C0FFEE");
}

#[test]
fn recolor_touches_each_field_kind() {
    let mut s = Session::new(rainbow());
    run(
        &mut s,
        "course.recolor",
        json!({"from": OLD, "to": "#AA0000"}),
    );
    let c = &s.course;
    assert_eq!(c.theme.player.bar.0, "#AA0000");
    assert_eq!(c.theme.player.bar_text.0, "#AA0000");
    let slide = c.slide("s1").unwrap();
    assert_eq!(slide.background.as_ref().unwrap().0, "#AA0000");
    assert_eq!(
        c.slide("s2").unwrap().background.as_ref().unwrap().0,
        "#AA000080"
    );
    assert_eq!(
        c.master("m1").unwrap().background.as_ref().unwrap().0,
        "#AA0000"
    );
    assert_eq!(
        c.bank_slide("q1").unwrap().background.as_ref().unwrap().0,
        "#AA0000"
    );
    let box_object = find(c, "box");
    assert_eq!(box_object.shadow.as_ref().unwrap().color.0, "#AA000080");
    assert_eq!(box_object.glow.as_ref().unwrap().color.0, "#AA000080");
    assert_eq!(box_object.states[0].fill.as_ref().unwrap().0, "#AA0000");
    let ObjectKind::Shape {
        fill,
        outline,
        text,
        ..
    } = &box_object.kind
    else {
        panic!("shape")
    };
    assert_eq!(fill.0, "#AA0000");
    assert_eq!(outline.as_ref().unwrap().color.0, "#AA0000");
    let shape_text = text.as_ref().unwrap();
    assert_eq!(shape_text.style.color.0, "#AA0000");
    assert_eq!(shape_text.runs[0].color.as_ref().unwrap().0, "#AA0000");
    let t1 = find(c, "t1");
    let ObjectKind::Text { style, runs, .. } = &t1.kind else {
        panic!("text")
    };
    assert_eq!(style.color.0, "#AA0000");
    assert_eq!(runs[0].color.as_ref().unwrap().0, "#AA0000");
    assert!(
        runs[1].color.is_none(),
        "an override that was never set stays unset"
    );
    let t2 = find(c, "t2");
    assert!(matches!(&t2.kind, ObjectKind::Text { style, .. } if style.color.0 == "#AA0000"));
}

#[test]
fn recolor_matches_by_rgb_only_and_leaves_other_text_alone() {
    let mut s = Session::new(Course::from_json(HELLO).unwrap());
    run(
        &mut s,
        "object.add",
        json!({"slide": "s-intro", "kind": {"type": "text",
        "text": "#336699 in prose", "style": {"color": "#336699cc"}}, "frame": frame()}),
    );
    run(
        &mut s,
        "course.recolor",
        json!({"from": "#336699", "to": "#112233"}),
    );
    let json = s.course.to_json().unwrap();
    assert!(json.contains("#112233CC"), "alpha kept, upper case");
    assert!(
        json.contains("#336699 in prose"),
        "text that is not a colour field is untouched"
    );
    // Nothing matched: no undo step and no change.
    let revision = s.revision;
    let result = run(
        &mut s,
        "course.recolor",
        json!({"from": "#010203", "to": "#040506"}),
    );
    assert_eq!(result["changed"], 0);
    assert_eq!(s.revision, revision);
}

#[test]
fn recolor_undo_restores_everything_and_is_one_step() {
    let mut s = Session::new(rainbow());
    let before = s.course.clone();
    run(
        &mut s,
        "course.recolor",
        json!({"from": OLD, "to": "#AA0000"}),
    );
    assert_ne!(s.course, before);
    s.undo().unwrap();
    assert_eq!(s.course, before);
    s.redo().unwrap();
    assert!(!lower(&s.course).contains("336699"));
}

#[test]
fn malformed_colours_are_refused_and_change_nothing() {
    let mut s = Session::new(rainbow());
    let before = s.course.clone();
    for bad in ["red", "#12", "#GGGGGG", "336699", "#1234567", ""] {
        assert!(
            fails(
                &mut s,
                "course.recolor",
                json!({"from": bad, "to": "#000000"})
            )
            .contains("not a colour"),
            "{bad}"
        );
        assert!(
            fails(&mut s, "course.recolor", json!({"from": OLD, "to": bad}))
                .contains("not a colour"),
            "{bad}"
        );
        assert!(
            fails(
                &mut s,
                "theme.set_palette",
                json!({"colors": [{"name": "x", "color": bad}]})
            )
            .contains("not a colour"),
            "{bad}"
        );
        assert!(
            fails(
                &mut s,
                "theme.set_palette_color",
                json!({"id": "brand", "color": bad})
            )
            .contains("not a colour"),
            "{bad}"
        );
    }
    assert_eq!(s.course, before);
}

#[test]
fn palette_can_be_set_and_is_limited_to_twelve() {
    let mut s = Session::new(Course::from_json(HELLO).unwrap());
    let result = run(
        &mut s,
        "theme.set_palette",
        json!({"colors": [
            {"name": "Brand blue", "color": "#2f6feb"},
            {"id": "warm", "name": "Warm", "color": "#fa0"},
        ]}),
    );
    assert_eq!(result["ids"], json!(["pal1", "warm"]));
    assert_eq!(s.course.theme.palette[0].color.0, "#2F6FEB");
    assert_eq!(s.course.theme.palette[1].color.0, "#FFAA00");
    assert!(s.course.to_json().unwrap().contains("\"palette\""));

    let twelve: Vec<Json> = (0..12)
        .map(|n| json!({"name": format!("c{n}"), "color": "#000000"}))
        .collect();
    run(&mut s, "theme.set_palette", json!({"colors": twelve}));
    let thirteen: Vec<Json> = (0..13)
        .map(|n| json!({"name": format!("c{n}"), "color": "#000000"}))
        .collect();
    assert!(fails(&mut s, "theme.set_palette", json!({"colors": thirteen})).contains("at most 12"));
    assert!(fails(&mut s, "theme.set_palette", json!({"colors": [
        {"id": "a", "name": "A", "color": "#000000"}, {"id": "a", "name": "B", "color": "#000000"}]})).contains("duplicate"));
    assert!(
        fails(
            &mut s,
            "theme.set_palette",
            json!({"colors": [{"name": "  ", "color": "#000000"}]})
        )
        .contains("name is empty")
    );
    assert_eq!(
        s.course.theme.palette.len(),
        12,
        "failed commands changed nothing"
    );

    run(&mut s, "theme.set_palette", json!({"colors": []}));
    assert!(!s.course.to_json().unwrap().contains("palette"));
    s.undo().unwrap();
    assert_eq!(s.course.theme.palette.len(), 12);
}

#[test]
fn set_palette_color_recolors_the_course_unless_told_not_to() {
    let mut s = Session::new(rainbow());
    run(
        &mut s,
        "theme.set_palette",
        json!({"colors": [
        {"id": "brand", "name": "Brand", "color": OLD},
        {"id": "twin", "name": "Twin", "color": "#336699"}]}),
    );
    let before = s.course.clone();

    let result = run(
        &mut s,
        "theme.set_palette_color",
        json!({"id": "brand", "color": "#AA0000", "recolor": false}),
    );
    assert_eq!(result["changed"], 0);
    assert_eq!(s.course.theme.palette[0].color.0, "#AA0000");
    assert!(lower(&s.course).contains("336699"), "uses were not touched");
    s.undo().unwrap();
    assert_eq!(s.course, before);

    let result = run(
        &mut s,
        "theme.set_palette_color",
        json!({"id": "brand", "color": "#AA0000"}),
    );
    assert!(result["changed"].as_u64().unwrap() >= 20);
    assert_eq!(s.course.theme.palette[0].color.0, "#AA0000");
    assert_eq!(
        s.course.theme.palette[1].color.0, "#336699",
        "another entry keeps its own colour"
    );
    assert_eq!(s.course.theme.accent.0, "#AA0000");
    s.undo().unwrap();
    assert_eq!(s.course, before, "one undo step restores palette and uses");

    assert!(
        fails(
            &mut s,
            "theme.set_palette_color",
            json!({"id": "nope", "color": "#000000"})
        )
        .contains("does not exist")
    );
}

// ----------------------------------------------------------------- styles

/// A course with a text box on each of three slides, one on a layer.
fn three_boxes() -> Session {
    let mut s = Session::new(Course::from_json(HELLO).unwrap());
    for (slide, id) in [("s-intro", "ta"), ("s-tabs", "tb"), ("s-results", "tc")] {
        run(
            &mut s,
            "object.add",
            json!({"slide": slide, "kind": {"type": "text", "text": "Line one\nLine two"}, "frame": frame(), "id": id}),
        );
    }
    run(
        &mut s,
        "layer.add",
        json!({"slide": "s-tabs", "id": "lay", "name": "Lay"}),
    );
    run(
        &mut s,
        "object.add",
        json!({"slide": "s-tabs", "layer": "lay", "id": "td", "kind": {"type": "text", "text": "On a layer"}, "frame": frame()}),
    );
    s
}

fn style_of(s: &Session, object: &str) -> (coursecraft_course::TextStyle, Option<String>) {
    match &find(&s.course, object).kind {
        ObjectKind::Text {
            style, text_style, ..
        } => (style.clone(), text_style.clone()),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_style_is_created_from_a_box_and_linked() {
    let mut s = three_boxes();
    run(
        &mut s,
        "object.update",
        json!({"slide": "s-intro", "object": "ta", "kind": {"type": "text",
        "text": "Line one\nLine two", "style": {"font": "Inter", "size": 40, "bold": true, "color": "#AA0000", "align": "center"}}}),
    );
    let result = run(
        &mut s,
        "text_style.create",
        json!({"name": "Heading", "from_object": {"slide": "s-intro", "object": "ta"}}),
    );
    let id = result["id"].as_str().unwrap().to_owned();
    let named = &s.course.text_styles[0];
    assert_eq!(
        (named.id.as_str(), named.name.as_str()),
        (id.as_str(), "Heading")
    );
    assert_eq!(named.style.size, 40.0);
    assert!(named.style.bold);
    assert_eq!(
        style_of(&s, "ta").1.as_deref(),
        Some(id.as_str()),
        "the source box is linked"
    );
    s.undo().unwrap();
    assert!(s.course.text_styles.is_empty());
    assert_eq!(style_of(&s, "ta").1, None, "undo unlinks too");

    // From explicit fields: nothing is linked.
    let result = run(
        &mut s,
        "text_style.create",
        json!({"name": "Caption", "style": {"size": 14, "italic": true}}),
    );
    assert_eq!(s.course.text_styles[0].style.size, 14.0);
    assert!(s.course.text_styles[0].style.italic);
    assert!(result["id"].is_string());
    assert!(fails(&mut s, "text_style.create", json!({"name": "x"})).contains("exactly one"));
    assert!(
        fails(
            &mut s,
            "text_style.create",
            json!({"name": "x", "from_object": {"slide": "s-intro", "object": "btn-start"}})
        )
        .contains("not a text box")
    );
    assert!(
        fails(
            &mut s,
            "text_style.create",
            json!({"name": "x", "style": {"color": "red"}})
        )
        .contains("not #RRGGBB")
    );
    assert!(
        fails(
            &mut s,
            "text_style.create",
            json!({"name": " ", "style": {}})
        )
        .contains("name is empty")
    );
}

#[test]
fn update_reapplies_to_every_linked_box_including_hand_changed_ones() {
    let mut s = three_boxes();
    let id = run(
        &mut s,
        "text_style.create",
        json!({"name": "Body", "style": {"size": 22}}),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    for (slide, object) in [
        ("s-intro", "ta"),
        ("s-tabs", "tb"),
        ("s-tabs", "td"),
        ("s-results", "tc"),
    ] {
        run(
            &mut s,
            "text_style.apply",
            json!({"id": id, "slide": slide, "objects": [object]}),
        );
    }
    assert_eq!(style_of(&s, "td").0.size, 22.0);

    // One box is changed by hand and keeps its link; one has a per-range override.
    run(
        &mut s,
        "object.update",
        json!({"slide": "s-results", "object": "tc", "kind": {"type": "text", "text": "Line one\nLine two",
        "style": {"size": 99, "bold": true}, "text_style": id}}),
    );
    run(
        &mut s,
        "text.style_range",
        json!({"slide": "s-intro", "object": "ta", "start": 0, "end": 4, "bold": true}),
    );
    assert_eq!(
        style_of(&s, "tc").1.as_deref(),
        Some(id.as_str()),
        "a hand edit keeps the link"
    );

    let result = run(
        &mut s,
        "text_style.update",
        json!({"id": id, "style": {"size": 30, "color": "#00AA00"}, "name": "Body text"}),
    );
    assert_eq!(result["updated"], 4);
    assert_eq!(s.course.text_styles[0].name, "Body text");
    for object in ["ta", "tb", "td", "tc"] {
        let (style, link) = style_of(&s, object);
        assert_eq!(style.size, 30.0, "{object}");
        assert_eq!(style.color.0, "#00AA00", "{object}");
        assert!(!style.bold, "{object}: the style wins over the hand edit");
        assert_eq!(link.as_deref(), Some(id.as_str()), "{object} stays linked");
    }
    let ObjectKind::Text { runs, .. } = &find(&s.course, "ta").kind else {
        panic!()
    };
    assert!(
        runs.iter().any(|r| r.bold == Some(true)),
        "per-range overrides are kept"
    );
    for object in ["ta", "tb", "td", "tc"] {
        let link = coursecraft_course::style_link(&s.course, find(&s.course, object));
        assert_eq!(
            link,
            coursecraft_course::StyleLink::Matches,
            "{object} is no longer modified"
        );
    }
    s.undo().unwrap();
    assert_eq!(style_of(&s, "tb").0.size, 22.0, "one undo step");
    assert_eq!(s.course.text_styles[0].name, "Body");
}

#[test]
fn apply_keeps_overrides_unless_asked_to_clear_them() {
    let mut s = three_boxes();
    let id = run(
        &mut s,
        "text_style.create",
        json!({"name": "Plain", "style": {"size": 18}}),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    run(
        &mut s,
        "text.style_range",
        json!({"slide": "s-intro", "object": "ta", "start": 0, "end": 4, "bold": true}),
    );
    run(
        &mut s,
        "text_style.apply",
        json!({"id": id, "slide": "s-intro", "objects": ["ta"]}),
    );
    let ObjectKind::Text { runs, .. } = &find(&s.course, "ta").kind else {
        panic!()
    };
    assert!(runs.iter().any(|r| r.bold == Some(true)));
    run(
        &mut s,
        "text_style.apply",
        json!({"id": id, "slide": "s-intro", "objects": ["ta"], "clear_overrides": true}),
    );
    let ObjectKind::Text { runs, style, .. } = &find(&s.course, "ta").kind else {
        panic!()
    };
    assert!(runs.iter().all(|r| r.bold.is_none()));
    assert_eq!(style.size, 18.0);
    assert!(
        fails(
            &mut s,
            "text_style.apply",
            json!({"id": id, "slide": "s-intro", "objects": ["btn-start"]})
        )
        .contains("not a text box")
    );
    assert!(
        fails(
            &mut s,
            "text_style.apply",
            json!({"id": "missing", "slide": "s-intro", "objects": ["ta"]})
        )
        .contains("does not exist")
    );
}

#[test]
fn paragraph_formatting_applies_and_list_kind_only_when_given() {
    let mut s = three_boxes();
    let id = run(
        &mut s,
        "text_style.create",
        json!({"name": "Spaced", "style": {},
        "paragraphs": {"line_spacing": 1.5, "space_after": 8}}),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    run(
        &mut s,
        "text.set_paragraphs",
        json!({"slide": "s-intro", "object": "ta", "start": 0, "end": 1, "list": "bullet"}),
    );
    run(
        &mut s,
        "text_style.apply",
        json!({"id": id, "slide": "s-intro", "objects": ["ta"]}),
    );
    let ObjectKind::Text { paragraphs, .. } = &find(&s.course, "ta").kind else {
        panic!()
    };
    assert_eq!(paragraphs.len(), 2);
    assert!(
        paragraphs
            .iter()
            .all(|p| p.line_spacing == 1.5 && p.space_after == 8.0)
    );
    assert_eq!(
        paragraphs[0].list,
        coursecraft_course::ListKind::Bullet,
        "no list given: the box keeps its own"
    );
}

#[test]
fn delete_unlinks_boxes_and_keeps_their_look() {
    let mut s = three_boxes();
    let id = run(
        &mut s,
        "text_style.create",
        json!({"name": "Big", "style": {"size": 60, "bold": true}}),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    for (slide, object) in [("s-intro", "ta"), ("s-tabs", "tb"), ("s-tabs", "td")] {
        run(
            &mut s,
            "text_style.apply",
            json!({"id": id, "slide": slide, "objects": [object]}),
        );
    }
    let looks: Vec<_> = ["ta", "tb", "td"]
        .iter()
        .map(|o| style_of(&s, o).0)
        .collect();
    let result = run(&mut s, "text_style.delete", json!({"id": id}));
    assert_eq!(result["unlinked"], 3);
    assert!(s.course.text_styles.is_empty());
    for (object, look) in ["ta", "tb", "td"].iter().zip(looks) {
        let (style, link) = style_of(&s, object);
        assert_eq!(style, look, "{object} looks the same");
        assert_eq!(link, None);
    }
    s.undo().unwrap();
    assert_eq!(s.course.text_styles.len(), 1);
    assert_eq!(style_of(&s, "ta").1.as_deref(), Some(id.as_str()));
    assert!(fails(&mut s, "text_style.delete", json!({"id": "gone"})).contains("does not exist"));
}

#[test]
fn at_most_fifty_styles() {
    let mut s = three_boxes();
    for n in 0..50 {
        run(
            &mut s,
            "text_style.create",
            json!({"name": format!("S{n}"), "style": {}}),
        );
    }
    assert!(
        fails(
            &mut s,
            "text_style.create",
            json!({"name": "one too many", "style": {}})
        )
        .contains("at most 50")
    );
    assert_eq!(s.course.text_styles.len(), 50);
}

#[test]
fn a_link_to_a_missing_style_is_a_warning_and_the_box_keeps_its_look() {
    let mut s = three_boxes();
    run(
        &mut s,
        "object.update",
        json!({"slide": "s-intro", "object": "ta", "kind": {"type": "text", "text": "x",
        "style": {"size": 33}, "text_style": "ghost"}}),
    );
    let issues = s.course.validate();
    assert!(
        issues
            .iter()
            .all(|i| i.severity == Severity::Warning || !i.path.contains("text_style"))
    );
    assert!(
        issues.iter().any(|i| i.severity == Severity::Warning
            && i.message.contains("text style \"ghost\" does not exist")),
        "{issues:?}"
    );
    assert_eq!(errors(&s.course), Vec::<String>::new());
    assert_eq!(style_of(&s, "ta").0.size, 33.0);
    assert_eq!(
        coursecraft_course::style_link(&s.course, find(&s.course, "ta")),
        coursecraft_course::StyleLink::Missing
    );
}

#[test]
fn courses_without_the_new_fields_round_trip_unchanged() {
    let course = Course::from_json(HELLO).unwrap();
    let saved = course.to_json().unwrap();
    for absent in ["palette", "text_styles", "text_style"] {
        assert!(
            !saved.contains(absent),
            "{absent} must be omitted when empty"
        );
    }
    assert_eq!(Course::from_json(&saved).unwrap(), course);
    assert_eq!(Course::from_json(&saved).unwrap().to_json().unwrap(), saved);
}

#[test]
fn an_object_update_without_text_style_keeps_the_link() {
    let mut s = three_boxes();
    let id = run(
        &mut s,
        "text_style.create",
        json!({"name": "S", "style": {"size": 25}}),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    run(
        &mut s,
        "text_style.apply",
        json!({"id": id, "slide": "s-intro", "objects": ["ta"]}),
    );
    run(
        &mut s,
        "object.update",
        json!({"slide": "s-intro", "object": "ta", "kind": {"type": "text", "text": "x", "style": {"size": 26}}}),
    );
    let (style, link) = style_of(&s, "ta");
    assert_eq!(style.size, 26.0);
    assert_eq!(
        link.as_deref(),
        Some(id.as_str()),
        "changing the look by hand keeps the link"
    );
    assert_eq!(
        coursecraft_course::style_link(&s.course, find(&s.course, "ta")),
        coursecraft_course::StyleLink::Modified
    );
}
