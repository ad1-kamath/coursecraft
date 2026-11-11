use coursecraft_course::{
    CourseOutline, MAX_CHOICES, ObjectKind, OutlineQuestion, outline_from_markdown,
};
use serde_json::{Value as Json, json};

use crate::{EngineError, Session};

fn ok(s: &mut Session, id: &str, p: Json) -> Json {
    match s.execute(id, &p) {
        Ok(v) => v,
        Err(e) => panic!("{id} failed: {e}"),
    }
}

const OUTLINE: &str = "# Fire Safety\n\
## Welcome\n\
- Keep exits clear\n\
- Know the alarm sound\n\
> Point at the nearest exit.\n\
## One point\n\
Just the one line.\n\
## Quiz\n\
? What do you do when the alarm sounds?\n\
[ ] Finish your coffee\n\
[x] Leave by the nearest exit\n\
[ ] Wait for an email.\n\
# Wrap Up\n\
## Thanks\n";

fn parsed(text: &str) -> CourseOutline {
    match outline_from_markdown(text) {
        Ok(o) => o,
        Err(p) => panic!("outline should parse: {p:?}"),
    }
}

fn text_of(slide: &coursecraft_course::Slide, name: &str) -> Option<String> {
    slide.objects.iter().find_map(|o| match &o.kind {
        ObjectKind::Text { text, .. } if o.name == name => Some(text.clone()),
        _ => None,
    })
}

fn errors(s: &Session) -> Vec<String> {
    s.course
        .validate()
        .into_iter()
        .filter(|i| i.severity == coursecraft_course::Severity::Error)
        .map(|i| format!("{}: {}", i.path, i.message))
        .collect()
}

#[test]
fn import_appends_scenes_and_slides_and_reports_counts() {
    let mut s = Session::blank("Blank");
    let outline = parsed(OUTLINE);
    let r = ok(
        &mut s,
        "course.import_outline",
        json!({ "outline": outline, "after": "slide1" }),
    );
    assert_eq!(r["scenes"], 2);
    assert_eq!(r["slides"], 4);
    let first = r["first_slide"].as_str().unwrap_or_default().to_owned();
    // Nothing is deleted: the blank course's own slide is still first.
    let ids: Vec<&str> = s.course.slides().map(|sl| sl.id.as_str()).collect();
    assert_eq!(ids.first().copied(), Some("slide1"));
    assert_eq!(ids.len(), 5);
    assert_eq!(ids.get(1).copied(), Some(first.as_str()));
    assert_eq!(errors(&s), Vec::<String>::new());
}

#[test]
fn imported_slides_carry_title_body_notes_and_question() {
    let mut s = Session::blank("Blank");
    let outline = parsed(OUTLINE);
    ok(
        &mut s,
        "course.import_outline",
        json!({ "outline": outline, "after": "slide1" }),
    );
    let slides: Vec<_> = s.course.slides().skip(1).cloned().collect();
    let welcome = &slides[0];
    assert_eq!(welcome.title, "Welcome");
    assert_eq!(text_of(welcome, "Title").as_deref(), Some("Welcome"));
    assert_eq!(
        text_of(welcome, "Text").as_deref(),
        Some("• Keep exits clear\n• Know the alarm sound")
    );
    assert_eq!(welcome.notes, "Point at the nearest exit.");

    let one = &slides[1];
    assert_eq!(text_of(one, "Title").as_deref(), Some("One point"));
    // One body line gets no bullet and still uses the content box.
    assert_eq!(text_of(one, "Text").as_deref(), Some("Just the one line."));

    let quiz = &slides[2];
    let q = quiz.question.as_ref().expect("question");
    assert_eq!(q.prompt, "What do you do when the alarm sounds?");
    assert_eq!(q.choices.len(), 3);
    assert_eq!(q.choices.iter().filter(|c| c.correct).count(), 1);
    // The submit button and feedback layers that `question.insert` builds are kept.
    assert!(quiz.objects.iter().any(|o| o.id.starts_with("button")));
    assert!(quiz.layers.iter().any(|l| l.name == "Correct"));
    assert!(quiz.layers.iter().any(|l| l.name == "Incorrect"));
    assert!(
        quiz.triggers
            .iter()
            .any(|t| matches!(t.action, coursecraft_course::Action::SubmitQuestion))
    );

    let thanks = &slides[3];
    assert_eq!(thanks.title, "Thanks");
    // No body: the title layout, one text object, no content box.
    assert_eq!(text_of(thanks, "Title").as_deref(), Some("Thanks"));
    assert!(text_of(thanks, "Text").is_none());
}

#[test]
fn import_is_one_undo_step_that_restores_exactly() {
    let mut s = Session::blank("Blank");
    let outline = parsed(OUTLINE);
    let before = s.course.clone();
    let revision = s.revision;
    ok(
        &mut s,
        "course.import_outline",
        json!({ "outline": outline, "after": "slide1" }),
    );
    assert_ne!(s.course, before);
    assert_eq!(s.revision, revision + 1);
    ok(&mut s, "edit.undo", json!({}));
    assert_eq!(s.course, before);
}

#[test]
fn a_failing_outline_changes_nothing() {
    let mut s = Session::blank("T");
    ok(&mut s, "slide.add", json!({ "title": "Second" }));
    let before = s.course.clone();
    let revision = s.revision;
    // Two correct choices: the outline is refused before anything is written.
    let bad = CourseOutline {
        title: None,
        scenes: vec![coursecraft_course::OutlineScene {
            title: "S".into(),
            slides: vec![coursecraft_course::OutlineSlide {
                title: "Q".into(),
                question: Some(OutlineQuestion {
                    prompt: "Pick".into(),
                    choices: vec![
                        coursecraft_course::OutlineChoice {
                            text: "a".into(),
                            correct: true,
                        },
                        coursecraft_course::OutlineChoice {
                            text: "b".into(),
                            correct: true,
                        },
                    ],
                }),
                ..coursecraft_course::OutlineSlide::default()
            }],
        }],
    };
    let err = s.execute("course.import_outline", &json!({ "outline": bad }));
    match err {
        Err(EngineError::Invalid(m)) => assert!(m.contains("one correct"), "{m}"),
        other => panic!("expected Invalid, got {other:?}"),
    }
    assert_eq!(s.course, before);
    assert_eq!(s.revision, revision);
    assert!(s.can_undo(), "the earlier slide add is still undoable");
}

#[test]
fn empty_outline_is_refused_without_an_undo_step() {
    let mut s = Session::blank("T");
    let before = s.course.clone();
    let revision = s.revision;
    let empty = CourseOutline::default();
    match s.execute("course.import_outline", &json!({ "outline": empty })) {
        Err(EngineError::Invalid(message)) => assert!(message.contains("no scenes"), "{message}"),
        other => panic!("expected Invalid, got {other:?}"),
    }
    assert_eq!(s.course, before);
    assert_eq!(s.revision, revision);
    assert!(!s.can_undo());
}

#[test]
fn an_outline_with_scenes_but_no_slides_is_a_no_op() {
    let mut s = Session::blank("T");
    let before = s.course.clone();
    let revision = s.revision;
    let empty = CourseOutline {
        title: None,
        scenes: vec![coursecraft_course::OutlineScene {
            title: "S".into(),
            slides: Vec::new(),
        }],
    };
    let r = ok(&mut s, "course.import_outline", json!({ "outline": empty }));
    assert_eq!(r["slides"], 0);
    assert_eq!(s.course, before);
    assert_eq!(s.revision, revision);
    assert!(!s.can_undo());
}

#[test]
fn unknown_after_slide_is_refused() {
    let mut s = Session::blank("T");
    let before = s.course.clone();
    let err = s.execute(
        "course.import_outline",
        &json!({ "outline": parsed(OUTLINE), "after": "nope" }),
    );
    assert!(matches!(err, Err(EngineError::NotFound(_))), "{err:?}");
    assert_eq!(s.course, before);
}

#[test]
fn imported_frames_stay_inside_the_slide() {
    let mut s = Session::blank("Blank");
    ok(
        &mut s,
        "course.import_outline",
        json!({ "outline": parsed(OUTLINE), "after": "slide1" }),
    );
    let w = s.course.slide_size.w as f32;
    let h = s.course.slide_size.h as f32;
    for slide in s.course.slides() {
        for object in &slide.objects {
            let f = object.frame;
            assert!(
                f.x >= 0.0 && f.y >= 0.0 && f.x + f.w <= w && f.y + f.h <= h,
                "{} on {} is outside the slide: {f:?}",
                object.id,
                slide.id
            );
        }
    }
}

#[test]
fn new_slides_take_the_master_of_the_slide_they_follow() {
    let mut s = Session::blank("Blank");
    let master = Session::blank("M").course.slides().next().cloned();
    let mut master = match master {
        Some(m) => m,
        None => panic!("blank course has a slide"),
    };
    master.id = "brand".into();
    master.title = "Brand".into();
    s.course.masters.push(master);
    ok(
        &mut s,
        "slide.set_master",
        json!({ "slides": ["slide1"], "master": "brand" }),
    );
    ok(
        &mut s,
        "course.import_outline",
        json!({ "outline": parsed(OUTLINE), "after": "slide1" }),
    );
    // Inserted right after slide1, so the first new slide follows a master slide.
    let after: Vec<_> = s.course.slides().skip(1).take(4).cloned().collect();
    assert_eq!(
        after.first().and_then(|sl| sl.master.clone()),
        Some("brand".to_owned())
    );
}

#[test]
fn without_after_new_slides_follow_the_last_slide_master() {
    let mut s = Session::blank("Blank");
    let master = Session::blank("M").course.slides().next().cloned();
    let mut master = match master {
        Some(m) => m,
        None => panic!("blank course has a slide"),
    };
    master.id = "brand".into();
    master.title = "Brand".into();
    s.course.masters.push(master);
    let second = ok(&mut s, "slide.add", json!({ "title": "Second" }));
    let second = second["id"].as_str().unwrap_or_default().to_owned();
    ok(
        &mut s,
        "slide.set_master",
        json!({ "slides": [second], "master": "brand" }),
    );
    ok(
        &mut s,
        "course.import_outline",
        json!({ "outline": parsed(OUTLINE) }),
    );
    // Appended after the last slide, so the first new slide follows it.
    let after: Vec<_> = s.course.slides().skip(2).take(4).cloned().collect();
    assert_eq!(
        after.first().and_then(|sl| sl.master.clone()),
        Some("brand".to_owned())
    );
}

#[test]
fn master_slides_are_left_alone() {
    let mut s = Session::blank("Blank");
    let master = s.course.slides().next().cloned();
    if let Some(mut m) = master {
        m.id = "brand".into();
        s.course.masters.push(m);
    }
    let masters = s.course.masters.clone();
    ok(
        &mut s,
        "course.import_outline",
        json!({ "outline": parsed(OUTLINE), "after": "slide1" }),
    );
    assert_eq!(s.course.masters, masters);
}

#[test]
fn importing_a_valid_outline_into_an_invalid_course_never_makes_things_worse() {
    // `keep_valid` only guards valid courses; an invalid one must still import
    // without panicking and keep its own errors.
    let mut s = Session::blank("Blank");
    s.course.title.clear();
    let before = errors(&s);
    ok(
        &mut s,
        "course.import_outline",
        json!({ "outline": parsed(OUTLINE), "after": "slide1" }),
    );
    assert_eq!(s.course.slides().count(), 5);
    let after = errors(&s);
    assert!(
        after.len() <= before.len(),
        "import added validation errors: before {before:#?}, after {after:#?}"
    );
}

#[test]
fn question_choices_get_short_unique_ids() {
    let mut s = Session::blank("Blank");
    let choices: Vec<coursecraft_course::OutlineChoice> = (0..MAX_CHOICES)
        .map(|i| coursecraft_course::OutlineChoice {
            text: format!("c{i}"),
            correct: i == 0,
        })
        .collect();
    let outline = CourseOutline {
        title: None,
        scenes: vec![coursecraft_course::OutlineScene {
            title: "S".into(),
            slides: vec![coursecraft_course::OutlineSlide {
                title: "Q".into(),
                question: Some(OutlineQuestion {
                    prompt: "Pick".into(),
                    choices,
                }),
                ..coursecraft_course::OutlineSlide::default()
            }],
        }],
    };
    ok(
        &mut s,
        "course.import_outline",
        json!({ "outline": outline, "after": null }),
    );
    let q = s
        .course
        .slides()
        .find_map(|sl| sl.question.clone())
        .expect("question");
    let mut ids: Vec<&str> = q.choices.iter().map(|c| c.id.as_str()).collect();
    let len = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), len);
}

#[test]
fn importing_through_json_round_trips() {
    let mut s = Session::blank("Blank");
    let outline = parsed(OUTLINE);
    let value = match serde_json::to_value(&outline) {
        Ok(v) => v,
        Err(e) => panic!("serialise: {e}"),
    };
    ok(&mut s, "course.import_outline", json!({ "outline": value }));
    assert_eq!(s.course.slides().count(), 5);
    let errors = s
        .course
        .validate()
        .into_iter()
        .filter(|i| i.severity == coursecraft_course::Severity::Error)
        .collect::<Vec<_>>();
    assert!(errors.is_empty(), "{errors:#?}");
}

#[test]
fn a_question_slide_ignores_its_body() {
    let outline = CourseOutline {
        title: None,
        scenes: vec![coursecraft_course::OutlineScene {
            title: "S".into(),
            slides: vec![coursecraft_course::OutlineSlide {
                title: "Q".into(),
                body: vec!["not shown".into(), "also not shown".into()],
                question: Some(OutlineQuestion {
                    prompt: "Pick".into(),
                    choices: vec![
                        coursecraft_course::OutlineChoice {
                            text: "a".into(),
                            correct: true,
                        },
                        coursecraft_course::OutlineChoice {
                            text: "b".into(),
                            correct: false,
                        },
                    ],
                }),
                ..coursecraft_course::OutlineSlide::default()
            }],
        }],
    };
    let mut s = Session::blank("Blank");
    ok(
        &mut s,
        "course.import_outline",
        json!({ "outline": outline, "after": "slide1" }),
    );
    let slide = s
        .course
        .slides()
        .find(|sl| sl.question.is_some())
        .expect("imported question slide");
    assert!(
        slide.objects.iter().all(
            |o| !matches!(&o.kind, ObjectKind::Text { text, .. } if text.contains("not shown"))
        ),
        "body text on a question slide is ignored"
    );
}
