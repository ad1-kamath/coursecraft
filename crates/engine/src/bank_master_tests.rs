//! Question-bank slides take a master like scene slides do: `slide.set_master`
//! sets and clears it, `bank.move_slide` keeps it, `master.delete` clears it, and
//! validation warns when a bank slide names a master that does not exist.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::Session;
use coursecraft_course::Severity;
use serde_json::{Value as Json, json};

fn ok(s: &mut Session, id: &str, p: Json) -> Json {
    s.execute(id, &p)
        .unwrap_or_else(|e| panic!("{id} failed: {e}"))
}

/// A course with a bank, a master, and a question slide `q` that uses the
/// master and has moved into the bank.
fn setup() -> (Session, String, String) {
    let mut s = Session::blank("Title");
    let bank = ok(&mut s, "bank.create", json!({ "title": "Bank" }))["id"]
        .as_str()
        .expect("bank id")
        .to_owned();
    let master = ok(&mut s, "master.add", json!({ "title": "Look" }))["id"]
        .as_str()
        .expect("master id")
        .to_owned();
    ok(&mut s, "slide.add", json!({ "id": "q" }));
    ok(&mut s, "question.insert", json!({ "slide": "q" }));
    ok(
        &mut s,
        "slide.set_master",
        json!({ "slides": ["q"], "master": master }),
    );
    ok(
        &mut s,
        "bank.move_slide",
        json!({ "slide": "q", "bank": bank }),
    );
    (s, bank, master)
}

fn bank_master(s: &Session) -> Option<String> {
    s.course
        .bank_slide("q")
        .and_then(|slide| slide.master.clone())
}

#[test]
fn set_master_sets_and_clears_a_bank_slide_master() {
    let (mut s, _, master) = setup();
    ok(
        &mut s,
        "slide.set_master",
        json!({ "slides": ["q"], "master": null }),
    );
    assert_eq!(bank_master(&s), None);
    ok(
        &mut s,
        "slide.set_master",
        json!({ "slides": ["q"], "master": master }),
    );
    assert_eq!(bank_master(&s), Some(master.clone()));
    ok(&mut s, "slide.set_master", json!({ "slides": ["q"] }));
    assert_eq!(bank_master(&s), None);
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(bank_master(&s), Some(master));
}

#[test]
fn moving_into_a_bank_keeps_the_master() {
    let (s, bank, master) = setup();
    assert!(s.course.bank(&bank).is_some_and(|b| b.slides.len() == 1));
    assert_eq!(bank_master(&s), Some(master));
}

#[test]
fn deleting_a_master_clears_it_on_bank_slides_and_undo_restores_it() {
    let (mut s, _, master) = setup();
    ok(&mut s, "master.delete", json!({ "master": master }));
    assert_eq!(bank_master(&s), None);
    assert!(
        s.course
            .validate()
            .iter()
            .all(|issue| !issue.path.starts_with("/banks/")),
        "no dangling master left"
    );
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(bank_master(&s), Some(master));
}

#[test]
fn validation_warns_only_about_a_missing_bank_slide_master() {
    let (mut s, _, _) = setup();
    let bank_issues = |s: &Session| {
        s.course
            .validate()
            .into_iter()
            .filter(|issue| issue.path.starts_with("/banks/0/slides/0"))
            .collect::<Vec<_>>()
    };
    assert!(bank_issues(&s).is_empty(), "{:?}", bank_issues(&s));
    s.course.banks[0].slides[0].master = Some("ghost".to_owned());
    let issues = bank_issues(&s);
    assert!(
        issues
            .iter()
            .any(|issue| issue.severity == Severity::Warning
                && issue.path == "/banks/0/slides/0/master"
                && issue.message.contains("ghost")),
        "{issues:?}"
    );
}
