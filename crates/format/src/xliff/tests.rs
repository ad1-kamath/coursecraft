use coursecraft_course::{Course, ObjectKind, TextRun};

use super::*;

const SHOWCASE_JSON: &str = include_str!("../../../../examples/showcase/course.json");

fn showcase() -> Course {
    Course::from_json(SHOWCASE_JSON).expect("valid showcase")
}

fn export_doc(course: &Course, target: &str) -> XliffDocument {
    let text = export_xliff(course, target).expect("export");
    parse_xliff(text.as_bytes()).expect("parse exported xliff")
}

fn copy_targets(doc: &XliffDocument) -> XliffDocument {
    let mut out = doc.clone();
    for unit in &mut out.units {
        unit.target = Some(unit.source.clone());
    }
    out
}

fn pseudo(text: &str) -> String {
    format!("[xx]{text}[/xx]")
}

fn pseudo_doc(doc: &XliffDocument) -> XliffDocument {
    let mut out = doc.clone();
    for unit in &mut out.units {
        unit.target = Some(ParsedText {
            segments: unit
                .source
                .segments
                .iter()
                .map(|segment| TextSegment {
                    text: pseudo(&segment.text),
                    run: segment.run,
                })
                .collect(),
        });
    }
    out
}

fn run_course() -> Course {
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "runs",
        "title": "Runs",
        "scenes": [{
            "id": "sc",
            "title": "Scene",
            "slides": [{
                "id": "s1",
                "title": "Slide",
                "objects": [{
                    "id": "t1",
                    "kind": {
                        "type": "text",
                        "text": "Hello world",
                        "runs": [
                            { "text": "Hello ", "bold": true },
                            { "text": "world", "italic": true }
                        ]
                    },
                    "frame": { "x": 0.0, "y": 0.0, "w": 200.0, "h": 50.0 }
                }]
            }]
        }]
    });
    Course::from_json(&json.to_string()).expect("valid runs course")
}

fn table_course() -> Course {
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "table",
        "title": "Table",
        "scenes": [{
            "id": "sc",
            "title": "Scene",
            "slides": [{
                "id": "s1",
                "title": "Slide",
                "objects": [{
                    "id": "table1",
                    "kind": {
                        "type": "table",
                        "columns": [0.5, 0.5],
                        "rows": [{
                            "height": 1.0,
                            "cells": [
                                { "text": "First cell" },
                                { "text": "Second cell" }
                            ]
                        }]
                    },
                    "frame": { "x": 0.0, "y": 0.0, "w": 200.0, "h": 80.0 }
                }]
            }]
        }]
    });
    Course::from_json(&json.to_string()).expect("valid table course")
}

#[test]
fn xliff_exports_and_imports_every_table_cell() {
    let course = table_course();
    let doc = export_doc(&course, "fr");
    let ids: Vec<&str> = doc.units.iter().map(|unit| unit.id.as_str()).collect();
    assert!(
        ids.contains(&"slide/s1/object/table1/table/0/0/text"),
        "{ids:?}"
    );
    assert!(
        ids.contains(&"slide/s1/object/table1/table/0/1/text"),
        "{ids:?}"
    );

    let translated_doc = pseudo_doc(&doc);
    let (translated, report) = apply_xliff(&course, &translated_doc);
    assert_eq!(report.applied, doc.units.len());
    let ObjectKind::Table { rows, .. } = &translated.scenes[0].slides[0].objects[0].kind else {
        panic!("table");
    };
    assert_eq!(rows[0].cells[0].text, "[xx]First cell[/xx]");
    assert_eq!(rows[0].cells[1].text, "[xx]Second cell[/xx]");
}

#[test]
fn round_trip_source_to_target_changes_only_the_language() {
    let course = showcase();
    let doc = export_doc(&course, "fr");
    let copied = copy_targets(&doc);
    let (translated, report) = apply_xliff(&course, &copied);
    assert_eq!(report.applied, doc.units.len());
    assert!(report.source_changed.is_empty());
    assert!(report.unknown_ids.is_empty());
    assert!(report.placeholder_mismatch.is_empty());
    let mut expected = course.clone();
    expected.language = Some("fr".to_owned());
    assert_eq!(translated, expected);
}

#[test]
fn export_uses_the_documented_stable_ids() {
    let doc = export_doc(&showcase(), "fr");
    let ids: Vec<&str> = doc.units.iter().map(|unit| unit.id.as_str()).collect();
    for expected in [
        "course/title",
        "scene/sc-tour/title",
        "slide/s-welcome/title",
        "slide/s-welcome/object/obj-welcome-title/text",
        "slide/s-welcome/object/btn-welcome-start/label",
        "slide/s-q-mc/question/prompt",
        "slide/s-q-mc/question/choice/mc-a",
        "chrome/courseComplete",
        "chrome/answerRequired",
    ] {
        assert!(ids.contains(&expected), "missing {expected} in {ids:?}");
    }
}

#[test]
fn runs_export_as_groups_and_keep_their_formatting() {
    let course = run_course();
    let text = export_xliff(&course, "fr").expect("export");
    assert!(text.contains(r#"<trans-unit id="slide/s1/object/t1/text" xml:space="preserve">"#));
    assert!(text.contains(r#"<g id="r0">Hello </g>"#));
    assert!(text.contains(r#"<g id="r1">world</g>"#));

    let doc = parse_xliff(text.as_bytes()).expect("parse");
    let unit = doc
        .units
        .iter()
        .find(|unit| unit.id == "slide/s1/object/t1/text")
        .expect("run unit");
    assert_eq!(unit.source.segments.len(), 2);
    assert_eq!(unit.source.text(), "Hello world");
}

#[test]
fn reordered_groups_keep_each_runs_formatting() {
    let course = run_course();
    let mut doc = export_doc(&course, "fr");
    let unit = doc
        .units
        .iter_mut()
        .find(|unit| unit.id == "slide/s1/object/t1/text")
        .expect("run unit");
    unit.target = Some(ParsedText {
        segments: vec![
            TextSegment {
                text: "world".to_owned(),
                run: Some(1),
            },
            TextSegment {
                text: "Hello ".to_owned(),
                run: Some(0),
            },
        ],
    });
    let (translated, report) = apply_xliff(&course, &doc);
    assert_eq!(report.applied, 1);
    let object = translated
        .slide("s1")
        .and_then(|slide| slide.objects.first())
        .expect("object");
    let ObjectKind::Text { text, runs, .. } = &object.kind else {
        panic!("expected text object");
    };
    assert_eq!(text, "worldHello ");
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].text, "world");
    assert_eq!(runs[0].italic, Some(true));
    assert_eq!(runs[1].text, "Hello ");
    assert_eq!(runs[1].bold, Some(true));
}

#[test]
fn paragraphs_and_links_survive_xliff_round_trip() {
    let mut course = run_course();
    let object = course.scenes[0].slides[0]
        .objects
        .first_mut()
        .expect("text object");
    let ObjectKind::Text {
        paragraphs, runs, ..
    } = &mut object.kind
    else {
        panic!("not text");
    };
    *paragraphs = vec![coursecraft_course::Paragraph {
        list: coursecraft_course::ListKind::Bullet,
        level: 1,
        line_spacing: 1.5,
        space_before: 4.0,
        space_after: 6.0,
    }];
    let expected_paragraphs = paragraphs.clone();
    if let Some(run) = runs.first_mut() {
        run.link = Some(coursecraft_course::Link::Url {
            url: "https://example.com".into(),
        });
    }

    let doc = export_doc(&course, "fr");
    let copied = copy_targets(&doc);
    let (translated, report) = apply_xliff(&course, &copied);
    assert!(report.applied > 0);
    let object = translated.scenes[0].slides[0]
        .objects
        .first()
        .expect("text object");
    let ObjectKind::Text {
        paragraphs, runs, ..
    } = &object.kind
    else {
        panic!("not text");
    };
    assert_eq!(paragraphs, &expected_paragraphs);
    assert_eq!(
        runs.first().and_then(|run| run.link.clone()),
        Some(coursecraft_course::Link::Url {
            url: "https://example.com".into()
        })
    );
}

#[test]
fn pseudo_translation_changes_every_exported_string_and_validates() {
    let course = showcase();
    let original = export_doc(&course, "fr");
    let translated_doc = pseudo_doc(&original);
    let (translated, report) = apply_xliff(&course, &translated_doc);
    assert_eq!(report.applied, original.units.len());
    assert!(
        translated
            .validate()
            .iter()
            .all(|issue| issue.severity != coursecraft_course::Severity::Error)
    );
    let after = export_doc(&translated, "fr");
    for before in &original.units {
        let Some(changed) = after.units.iter().find(|unit| unit.id == before.id) else {
            panic!("unit {} disappeared", before.id);
        };
        assert_ne!(
            before.source.text(),
            changed.source.text(),
            "unit {} was not translated",
            before.id
        );
    }
}

#[test]
fn missing_target_is_reported() {
    let course = showcase();
    let original = export_doc(&course, "fr");
    let mut doc = original.clone();
    doc.units.truncate(1);
    doc.units[0].target = None;
    let (_, report) = apply_xliff(&course, &doc);
    assert_eq!(report.missing_target, vec![doc.units[0].id.clone()]);
    assert_eq!(report.applied, 0);
}

#[test]
fn empty_target_counts_as_missing() {
    let course = showcase();
    let original = export_doc(&course, "fr");
    let mut doc = original.clone();
    doc.units.truncate(1);
    doc.units[0].target = Some(ParsedText::default());
    let (_, report) = apply_xliff(&course, &doc);
    assert_eq!(report.missing_target, vec![doc.units[0].id.clone()]);
}

#[test]
fn changed_source_is_not_applied() {
    let course = showcase();
    let original = export_doc(&course, "fr");
    let mut doc = original.clone();
    doc.units.truncate(1);
    doc.units[0].source = ParsedText {
        segments: vec![TextSegment {
            text: "Someone edited the source".to_owned(),
            run: None,
        }],
    };
    doc.units[0].target = Some(ParsedText {
        segments: vec![TextSegment {
            text: "Traduction".to_owned(),
            run: None,
        }],
    });
    let (_, report) = apply_xliff(&course, &doc);
    assert_eq!(report.source_changed, vec![doc.units[0].id.clone()]);
    assert_eq!(report.applied, 0);
}

#[test]
fn unknown_id_is_reported() {
    let course = showcase();
    let doc = XliffDocument {
        source_language: "en".to_owned(),
        target_language: "fr".to_owned(),
        units: vec![XliffUnit {
            id: "slide/nope/title".to_owned(),
            source: ParsedText {
                segments: vec![TextSegment {
                    text: "Nope".to_owned(),
                    run: None,
                }],
            },
            target: Some(ParsedText {
                segments: vec![TextSegment {
                    text: "Introuvable".to_owned(),
                    run: None,
                }],
            }),
        }],
    };
    let (_, report) = apply_xliff(&course, &doc);
    assert_eq!(report.unknown_ids, vec!["slide/nope/title"]);
}

#[test]
fn lost_percent_reference_is_refused() {
    let course = showcase();
    let doc = XliffDocument {
        source_language: "en".to_owned(),
        target_language: "fr".to_owned(),
        units: vec![XliffUnit {
            id: "course/title".to_owned(),
            source: ParsedText {
                segments: vec![TextSegment {
                    text: "Hello %learner%".to_owned(),
                    run: None,
                }],
            },
            target: Some(ParsedText {
                segments: vec![TextSegment {
                    text: "Bonjour".to_owned(),
                    run: None,
                }],
            }),
        }],
    };
    // Make the source equal to the course only after adding a reference, so the
    // test exercises the placeholder rule rather than source_changed.
    let mut course = course;
    course.title = "Hello %learner%".to_owned();
    let (_, report) = apply_xliff(&course, &doc);
    assert_eq!(report.placeholder_mismatch, vec!["course/title"]);
    assert_eq!(report.applied, 0);
}

#[test]
fn lost_chrome_brace_placeholder_is_refused() {
    let mut course = showcase();
    course.strings.clear();
    let source = course.chrome_string("markersPlaced").to_owned();
    let doc = XliffDocument {
        source_language: "en".to_owned(),
        target_language: "fr".to_owned(),
        units: vec![XliffUnit {
            id: "chrome/markersPlaced".to_owned(),
            source: ParsedText {
                segments: vec![TextSegment {
                    text: source,
                    run: None,
                }],
            },
            target: Some(ParsedText {
                segments: vec![TextSegment {
                    text: "placé".to_owned(),
                    run: None,
                }],
            }),
        }],
    };
    let (_, report) = apply_xliff(&course, &doc);
    assert_eq!(report.placeholder_mismatch, vec!["chrome/markersPlaced"]);
}

#[test]
fn unsupported_script_is_applied_and_reported() {
    let course = showcase();
    let doc = XliffDocument {
        source_language: "en".to_owned(),
        target_language: "ja".to_owned(),
        units: vec![XliffUnit {
            id: "course/title".to_owned(),
            source: ParsedText {
                segments: vec![TextSegment {
                    text: course.title.clone(),
                    run: None,
                }],
            },
            target: Some(ParsedText {
                segments: vec![TextSegment {
                    text: "コース".to_owned(),
                    run: None,
                }],
            }),
        }],
    };
    let (translated, report) = apply_xliff(&course, &doc);
    assert_eq!(report.applied, 1);
    assert_eq!(report.unsupported_script, vec!["course/title"]);
    assert_eq!(translated.title, "コース");
    assert!(translated.language.as_deref() == Some("ja"));
}

#[test]
fn chrome_defaults_round_trip_without_adding_overrides() {
    let course = showcase();
    let doc = copy_targets(&export_doc(&course, "fr"));
    let (translated, _) = apply_xliff(&course, &doc);
    assert!(translated.strings.is_empty());
}

fn document_with(body: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<xliff version="1.2" xmlns="urn:oasis:names:tc:xliff:document:1.2">
  <file original="course.json" datatype="plaintext" source-language="en" target-language="fr">
    <body>{body}</body>
  </file>
</xliff>"#
    )
}

#[test]
fn doctype_entity_bomb_is_refused() {
    let xml = r#"<?xml version="1.0"?>
<!DOCTYPE lolz [
  <!ENTITY lol "lol">
  <!ENTITY lol2 "&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;">
]>
<xliff version="1.2"><file source-language="en" target-language="fr"><body/></file></xliff>"#;
    assert_eq!(parse_xliff(xml.as_bytes()), Err(XliffError::Doctype));
}

#[test]
fn unknown_entity_is_refused() {
    let xml = document_with(
        r#"<trans-unit id="course/title"><source>Hi</source><target>&bogus;</target></trans-unit>"#,
    );
    assert!(matches!(
        parse_xliff(xml.as_bytes()),
        Err(XliffError::Xml(_))
    ));
}

#[test]
fn oversized_file_is_refused() {
    let bytes = vec![b' '; MAX_XLIFF_BYTES + 1];
    assert_eq!(parse_xliff(&bytes), Err(XliffError::TooLarge));
}

#[test]
fn too_many_units_are_refused() {
    let mut body = String::new();
    for i in 0..=MAX_UNITS {
        body.push_str(&format!(
            r#"<trans-unit id="u{i}"><source>x</source></trans-unit>"#
        ));
    }
    let xml = document_with(&body);
    assert_eq!(parse_xliff(xml.as_bytes()), Err(XliffError::TooManyUnits));
}

#[test]
fn too_deep_nesting_is_refused() {
    let mut body = String::new();
    for _ in 0..MAX_DEPTH {
        body.push_str("<g>");
    }
    body.push('x');
    for _ in 0..MAX_DEPTH {
        body.push_str("</g>");
    }
    let xml = document_with(&format!(
        r#"<trans-unit id="course/title"><source>{body}</source></trans-unit>"#
    ));
    assert_eq!(parse_xliff(xml.as_bytes()), Err(XliffError::TooDeep));
}

#[test]
fn oversized_unit_is_refused() {
    let huge = "x".repeat(MAX_UNIT_BYTES + 1);
    let xml = document_with(&format!(
        r#"<trans-unit id="course/title"><source>{huge}</source></trans-unit>"#
    ));
    assert_eq!(parse_xliff(xml.as_bytes()), Err(XliffError::UnitTooLarge));
}

#[test]
fn truncated_xml_is_refused() {
    let text = export_xliff(&showcase(), "fr").expect("export");
    let truncated = text.get(..text.len() / 2).unwrap_or_default();
    assert!(matches!(
        parse_xliff(truncated.as_bytes()),
        Err(XliffError::Xml(_))
    ));
}

#[test]
fn invalid_utf8_is_refused() {
    assert!(matches!(
        parse_xliff(&[0xFF, 0xFE, 0x00]),
        Err(XliffError::InvalidUtf8(_))
    ));
}

#[test]
fn wrong_version_is_refused() {
    let xml = document_with("").replace("version=\"1.2\"", "version=\"2.0\"");
    assert_eq!(
        parse_xliff(xml.as_bytes()),
        Err(XliffError::UnsupportedVersion("2.0".to_owned()))
    );
}

#[test]
fn target_with_markup_keeps_the_text() {
    let xml = document_with(
        r#"<trans-unit id="course/title"><source>Hi</source><target>Bonjour <b>monde</b></target></trans-unit>"#,
    );
    let doc = parse_xliff(xml.as_bytes()).expect("parse");
    assert_eq!(
        doc.units[0].target.as_ref().map(ParsedText::text),
        Some("Bonjour monde".to_owned())
    );
}

#[test]
fn raw_cdata_end_in_text_is_refused() {
    let xml = document_with(
        r#"<trans-unit id="course/title"><source>Hi</source><target>bad ]]&gt; text</target></trans-unit>"#,
    );
    // An escaped `]]>` is valid character data; a raw one is not. Both must
    // parse without panicking, and the escaped one keeps its text.
    let doc = parse_xliff(xml.as_bytes()).expect("escaped cdata end is text");
    assert_eq!(
        doc.units[0].target.as_ref().map(ParsedText::text),
        Some("bad ]]> text".to_owned())
    );

    let raw = document_with(
        r#"<trans-unit id="course/title"><source>Hi</source><target>bad ]]> text</target></trans-unit>"#,
    );
    assert!(parse_xliff(raw.as_bytes()).is_err());
}

#[test]
fn control_characters_are_refused() {
    let xml = document_with(
        "<trans-unit id=\"course/title\"><source>Hi</source><target>bad\u{1}text</target></trans-unit>",
    );
    assert!(parse_xliff(xml.as_bytes()).is_err());
}

fn prefixed_targets(course: &Course) -> std::collections::BTreeMap<String, String> {
    translation_units(course, "fr")
        .into_iter()
        .map(|unit| (unit.id, format!("[fr] {}", unit.source)))
        .collect()
}

#[test]
fn fill_xliff_then_import_round_trips_the_showcase() {
    let course = showcase();
    let before = translation_units(&course, "fr");
    assert_eq!(before.len(), export_doc(&course, "fr").units.len());
    let xml = fill_xliff(&course, "fr", &prefixed_targets(&course)).expect("fill");
    let doc = parse_xliff(xml.as_bytes()).expect("parse filled");
    let (translated, report) = apply_xliff(&course, &doc);

    assert_eq!(report.applied, before.len());
    assert!(report.missing_target.is_empty());
    assert!(report.source_changed.is_empty());
    assert!(report.unknown_ids.is_empty());
    assert!(report.placeholder_mismatch.is_empty());
    assert_eq!(translated.language.as_deref(), Some("fr"));
    assert!(
        translated
            .validate()
            .iter()
            .all(|issue| issue.severity != coursecraft_course::Severity::Error)
    );

    let after = translation_units(&translated, "fr");
    assert_eq!(before.len(), after.len());
    for (old, new) in before.iter().zip(&after) {
        assert_eq!(old.id, new.id, "ids stay stable");
        assert_ne!(old.source, new.source, "{} was not translated", old.id);
    }
}

#[test]
fn fill_xliff_keeps_master_text_rich_runs_and_references() {
    let mut course = run_course();
    course.title = "Hello %learner%".to_owned();
    let mut master = course.scenes[0].slides[0].clone();
    master.id = "m1".to_owned();
    master.title = "Master title".to_owned();
    master.triggers.clear();
    master.layers.clear();
    master.question = None;
    if let Some(object) = master.objects.first_mut() {
        object.id = "master-text".to_owned();
        object.kind = ObjectKind::Text {
            text: "Master body".to_owned(),
            style: Default::default(),
            runs: vec![
                TextRun {
                    text: "Master ".to_owned(),
                    bold: Some(true),
                    italic: None,
                    underline: None,
                    size: None,
                    color: None,
                    link: None,
                    font: None,
                    strikethrough: None,
                    baseline: None,
                    letter_spacing: None,
                },
                TextRun {
                    text: "body".to_owned(),
                    bold: None,
                    italic: Some(true),
                    underline: None,
                    size: None,
                    color: None,
                    link: None,
                    font: None,
                    strikethrough: None,
                    baseline: None,
                    letter_spacing: None,
                },
            ],
            paragraphs: Vec::new(),
            autofit: coursecraft_course::Autofit::None,
            text_style: None,
        };
    }
    course.masters.push(master);

    let before = translation_units(&course, "fr");
    assert!(before.iter().any(|unit| unit.id == "course/title"));
    assert!(
        before
            .iter()
            .any(|unit| unit.id == "master/m1/object/master-text/text")
    );
    assert!(
        before
            .iter()
            .any(|unit| unit.id == "slide/s1/object/t1/text")
    );
    let targets = prefixed_targets(&course);
    let xml = fill_xliff(&course, "fr", &targets).expect("fill");
    let doc = parse_xliff(xml.as_bytes()).expect("parse filled");
    let (translated, report) = apply_xliff(&course, &doc);
    assert_eq!(report.applied, before.len());
    assert!(report.placeholder_mismatch.is_empty());

    assert!(translated.title.starts_with("[fr] Hello %learner%"));
    let run_unit = doc
        .units
        .iter()
        .find(|unit| unit.id == "slide/s1/object/t1/text")
        .expect("run unit");
    let target = run_unit.target.as_ref().expect("target");
    assert_eq!(target.text(), "[fr] Hello world");
    assert!(target.segments.iter().any(|segment| segment.run == Some(0)));
    assert!(target.segments.iter().any(|segment| segment.run == Some(1)));

    let slide_object = translated
        .slide("s1")
        .and_then(|slide| slide.objects.first())
        .expect("object");
    let ObjectKind::Text { text, runs, .. } = &slide_object.kind else {
        panic!("expected text object");
    };
    assert_eq!(text, "[fr] Hello world");
    let run = |index: usize| runs.get(index).expect("run");
    assert_eq!(run(1).text, "Hello ");
    assert_eq!(run(1).bold, Some(true));
    assert_eq!(run(2).text, "world");
    assert_eq!(run(2).italic, Some(true));

    let master_object = translated.masters[0]
        .object("master-text")
        .map(|(object, _)| object)
        .expect("master object");
    let ObjectKind::Text { text, runs, .. } = &master_object.kind else {
        panic!("expected master text object");
    };
    assert_eq!(text, "[fr] Master body");
    assert_eq!(runs.get(1).map(|run| run.bold), Some(Some(true)));
    assert_eq!(runs.get(2).map(|run| run.italic), Some(Some(true)));
}

#[test]
fn fill_xliff_round_trips_ampersands_and_angle_brackets() {
    let mut course = run_course();
    course.title = "A & B < C".to_owned();
    let units = translation_units(&course, "fr");
    let title = units
        .iter()
        .find(|unit| unit.id == "course/title")
        .expect("title unit");
    assert_eq!(title.source, "A &amp; B &lt; C");

    let xml = fill_xliff(&course, "fr", &prefixed_targets(&course)).expect("fill");
    let doc = parse_xliff(xml.as_bytes()).expect("parse filled");
    let (translated, report) = apply_xliff(&course, &doc);
    assert!(report.placeholder_mismatch.is_empty());
    assert_eq!(translated.title, "[fr] A & B < C");
}

#[test]
fn a_missing_target_is_left_untranslated() {
    let course = showcase();
    let before = translation_units(&course, "fr");
    let mut targets = prefixed_targets(&course);
    let missing = before.first().map(|unit| unit.id.clone()).expect("unit");
    targets.remove(&missing);
    let xml = fill_xliff(&course, "fr", &targets).expect("fill");
    let doc = parse_xliff(xml.as_bytes()).expect("parse filled");
    let (translated, report) = apply_xliff(&course, &doc);

    assert_eq!(report.missing_target, vec![missing.clone()]);
    assert_eq!(report.applied, before.len().saturating_sub(1));
    let old = before.first().expect("unit").source.clone();
    let unchanged = translation_units(&translated, "fr")
        .into_iter()
        .find(|unit| unit.id == missing)
        .map(|unit| unit.source);
    assert_eq!(unchanged, Some(old));
}

#[test]
fn masters_round_trip_through_xliff_and_templates_are_not_exported() {
    let mut course = run_course();
    let mut master = course.scenes[0].slides[0].clone();
    master.id = "m1".to_owned();
    master.title = "Master title".to_owned();
    master.triggers.clear();
    master.layers.clear();
    master.question = None;
    if let Some(object) = master.objects.first_mut() {
        object.id = "master-text".to_owned();
        object.kind = ObjectKind::Text {
            text: "Master body".to_owned(),
            style: Default::default(),
            runs: Vec::new(),
            paragraphs: Vec::new(),
            autofit: coursecraft_course::Autofit::None,
            text_style: None,
        };
    }
    course.masters.push(master);

    let mut template = course.scenes[0].slides[0].clone();
    template.id = "template1".to_owned();
    template.title = "Template title".to_owned();
    template.triggers.clear();
    template.layers.clear();
    template.question = None;
    if let Some(object) = template.objects.first_mut() {
        object.kind = ObjectKind::Text {
            text: "Template body".to_owned(),
            style: Default::default(),
            runs: Vec::new(),
            paragraphs: Vec::new(),
            autofit: coursecraft_course::Autofit::None,
            text_style: None,
        };
    }
    course.templates.push(template);

    let doc = export_doc(&course, "fr");
    assert!(doc.units.iter().any(|unit| unit.id == "master/m1/title"));
    assert!(
        doc.units
            .iter()
            .any(|unit| unit.id == "master/m1/object/master-text/text")
    );
    assert!(doc.units.iter().all(|unit| !unit.id.contains("template1")));

    let translated_doc = pseudo_doc(&doc);
    let (translated, report) = apply_xliff(&course, &translated_doc);
    assert_eq!(report.applied, doc.units.len());
    assert!(translated.masters[0].title.starts_with("[xx]"));
    match &translated.masters[0].objects[0].kind {
        ObjectKind::Text { text, .. } => assert!(text.starts_with("[xx]")),
        other => panic!("unexpected master object {other:?}"),
    }
    assert_eq!(translated.templates[0].title, "Template title");
}

#[test]
fn bank_slide_text_exports_and_imports() {
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "bank-xliff",
        "title": "Bank Xliff",
        "scenes": [{
            "id": "sc",
            "title": "Scene",
            "slides": [{
                "id": "s-draw",
                "title": "Draw",
                "draw": { "bank": "bank1", "count": 1 }
            }]
        }],
        "banks": [{
            "id": "bank1",
            "title": "Bank One",
            "slides": [{
                "id": "bq1",
                "title": "Bank question",
                "question": {
                    "kind": "multiple_choice",
                    "prompt": "Bank prompt",
                    "choices": [
                        { "id": "a", "text": "Choice A" },
                        { "id": "b", "text": "Choice B", "correct": true }
                    ],
                    "attempts": 1
                },
                "triggers": [{
                    "action": { "action": "submit_question" },
                    "when": { "event": "submit" }
                }]
            }]
        }]
    });
    let course = Course::from_json(&json.to_string()).expect("valid bank course");
    let doc = export_doc(&course, "fr");
    for id in [
        "slide/bq1/title",
        "slide/bq1/question/prompt",
        "slide/bq1/question/choice/a",
    ] {
        assert!(doc.units.iter().any(|unit| unit.id == id), "{id} exported");
    }
    let translated_doc = pseudo_doc(&doc);
    let (translated, report) = apply_xliff(&course, &translated_doc);
    assert_eq!(report.applied, doc.units.len());
    assert!(translated.banks[0].slides[0].title.starts_with("[xx]"));
    assert!(
        translated.banks[0].slides[0]
            .question
            .as_ref()
            .unwrap()
            .prompt
            .starts_with("[xx]")
    );
}

#[test]
fn shape_text_exports_and_imports_like_text_object_text() {
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "shapes",
        "title": "Shapes",
        "scenes": [{
            "id": "sc",
            "title": "Scene",
            "slides": [{
                "id": "s1",
                "title": "Slide",
                "objects": [{
                    "id": "shape1",
                    "kind": {
                        "type": "shape",
                        "shape": "rectangle",
                        "fill": "#D0D7DE",
                        "text": {
                            "text": "Hello %name%",
                            "runs": [
                                { "text": "Hello ", "bold": true },
                                { "text": "%name%", "italic": true }
                            ]
                        }
                    },
                    "frame": { "x": 0.0, "y": 0.0, "w": 200.0, "h": 50.0 }
                }]
            }]
        }]
    });
    let course = Course::from_json(&json.to_string()).expect("valid shape course");
    let doc = export_doc(&course, "fr");
    assert!(
        doc.units
            .iter()
            .any(|unit| unit.id == "slide/s1/object/shape1/text")
    );
    let translated_doc = pseudo_doc(&doc);
    let (translated, report) = apply_xliff(&course, &translated_doc);
    assert_eq!(report.applied, doc.units.len());
    match &translated.scenes[0].slides[0].objects[0].kind {
        ObjectKind::Shape {
            text: Some(text), ..
        } => {
            assert!(text.text.starts_with("[xx]"));
            assert!(text.runs.iter().any(|run| run.bold == Some(true)));
        }
        other => panic!("unexpected shape {other:?}"),
    }
}
