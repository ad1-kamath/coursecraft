//! The shared string walker (`coursecraft_course::course_strings`) and the
//! XLIFF export must agree on every author-visible string. If they drift,
//! find and replace would miss a string that translation covers, or the
//! reverse.
//!
//! Known exceptions, all written down here:
//! - Chrome strings (`chrome/*` units) are the player's own text, not course
//!   content, so the walker leaves them out on purpose.
//! - XLIFF does not export the layers or the question of a master. The walker
//!   does, because find and replace must reach them.

use coursecraft_course::{Course, course_strings};
use coursecraft_format::xliff::{export_xliff, parse_xliff};
use serde_json::{Value, json};

const SHOWCASE: &str = include_str!("../../../examples/showcase/course.json");

/// The source text of every XLIFF unit that is course content, sorted.
fn xliff_texts(course: &Course) -> Vec<String> {
    let xml = export_xliff(course, "fr").expect("export");
    let doc = parse_xliff(xml.as_bytes()).expect("parse");
    let mut texts: Vec<String> = doc
        .units
        .iter()
        .filter(|unit| !unit.id.starts_with("chrome/"))
        .map(|unit| unit.source.text())
        .collect();
    texts.sort();
    texts
}

/// The text of every walked string, sorted.
fn walker_texts(course: &Course) -> Vec<String> {
    let mut texts: Vec<String> = course_strings(course)
        .into_iter()
        .map(|item| item.text)
        .collect();
    texts.sort();
    texts
}

/// The showcase course with one master added. The master has a title, notes,
/// one object, and optionally a layer with one text object (which XLIFF skips).
fn showcase_with_master(with_layer: bool) -> Course {
    let mut doc: Value = serde_json::from_str(SHOWCASE).expect("showcase json");
    let mut master = json!({
        "id": "master_parity",
        "title": "Parity master title",
        "notes": "Parity master notes",
        "objects": [{
            "id": "master_parity_text",
            "name": "Master text",
            "kind": { "type": "text", "text": "Parity master object text" },
            "frame": { "x": 10.0, "y": 10.0, "w": 200.0, "h": 40.0 }
        }]
    });
    if let Some(object) = master.as_object_mut().filter(|_| with_layer) {
        object.insert(
            "layers".to_owned(),
            json!([{
                "id": "master_parity_layer",
                "name": "Master layer",
                "objects": [{
                    "id": "master_parity_layer_text",
                    "kind": { "type": "text", "text": "Parity master layer text" },
                    "frame": { "x": 10.0, "y": 60.0, "w": 200.0, "h": 40.0 }
                }]
            }]),
        );
    }
    let Some(root) = doc.as_object_mut() else {
        panic!("showcase is not an object");
    };
    root.insert("masters".to_owned(), json!([master]));
    serde_json::from_value(doc).expect("course with master")
}

#[test]
fn walker_and_xliff_agree_on_the_showcase_course() {
    let course = Course::from_json(SHOWCASE).expect("showcase");
    let xliff = xliff_texts(&course);
    let walked = walker_texts(&course);
    assert!(!xliff.is_empty(), "the showcase has translatable text");
    assert_eq!(walked, xliff, "walker and XLIFF disagree");
}

#[test]
fn walker_and_xliff_agree_on_a_course_with_a_master() {
    let course = showcase_with_master(false);
    let xliff = xliff_texts(&course);
    let walked = walker_texts(&course);
    assert!(xliff.iter().any(|text| text == "Parity master object text"));
    assert!(xliff.iter().any(|text| text == "Parity master title"));
    assert!(xliff.iter().any(|text| text == "Parity master notes"));
    assert_eq!(walked, xliff, "walker and XLIFF disagree");
}

#[test]
fn master_layers_are_walked_but_not_exported_to_xliff() {
    // Documented exception: XLIFF skips master layers, the walker does not.
    let course = showcase_with_master(true);
    let xliff = xliff_texts(&course);
    let walked = walker_texts(&course);
    assert!(walked.iter().any(|text| text == "Parity master layer text"));
    assert!(!xliff.iter().any(|text| text == "Parity master layer text"));
    assert_eq!(walked.len(), xliff.len() + 1);
}

#[test]
fn every_walked_location_is_unique() {
    let course = showcase_with_master(true);
    let mut seen = std::collections::BTreeSet::new();
    for item in course_strings(&course) {
        assert!(
            seen.insert(item.location.clone()),
            "duplicate location for {}",
            item.label
        );
    }
}
