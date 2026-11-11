use super::*;
use crate::{Event, ObjectKind};

const HELLO: &str = include_str!("../../../../examples/hello-course/course.json");

fn hello() -> Course {
    Course::from_json(HELLO).unwrap()
}

fn sentences(changes: &[Change]) -> Vec<String> {
    changes.iter().map(Change::sentence).collect()
}

fn slide_mut<'a>(course: &'a mut Course, id: &str) -> &'a mut Slide {
    course.slide_mut(id).unwrap()
}

fn object_mut<'a>(course: &'a mut Course, slide: &str, id: &str) -> &'a mut Object {
    let slide = slide_mut(course, slide);
    slide
        .objects
        .iter_mut()
        .chain(slide.layers.iter_mut().flat_map(|l| l.objects.iter_mut()))
        .find(|o| o.id == id)
        .unwrap()
}

#[test]
fn identical_courses_have_no_changes() {
    assert!(diff(&hello(), &hello()).is_empty());
}

#[test]
fn text_change_is_worded_with_old_and_new() {
    let mut b = hello();
    if let ObjectKind::Text { text, .. } = &mut object_mut(&mut b, "s-intro", "title").kind {
        *text = "Hi there".into();
    }
    let changes = diff(&hello(), &b);
    assert_eq!(changes.len(), 1, "{changes:#?}");
    assert_eq!(changes[0].kind, ChangeKind::Changed);
    assert_eq!(
        changes[0].sentence(),
        "Slide \"Welcome\": text of \"title\" changed from \"Workplace Safety Basics\" to \"Hi there\""
    );
    assert_eq!(
        changes[0].before.as_deref(),
        Some("Workplace Safety Basics")
    );
    assert_eq!(changes[0].after.as_deref(), Some("Hi there"));
    assert_eq!(
        changes[0].path,
        "scene \"Main\" › slide \"Welcome\" › object \"title\""
    );
}

#[test]
fn long_text_is_not_quoted() {
    let mut b = hello();
    if let ObjectKind::Text { text, .. } = &mut object_mut(&mut b, "s-intro", "title").kind {
        *text = "x".repeat(200);
    }
    let changes = diff(&hello(), &b);
    assert_eq!(changes[0].summary, "text of \"title\" changed");
}

#[test]
fn course_settings_report_which_setting() {
    let mut b = hello();
    b.title = "New title".into();
    b.theme.accent = crate::Color("#FF0000".into());
    b.player.show_menu = false;
    b.slide_size.w = 1920;
    b.quiz = None;
    let lines = sentences(&diff(&hello(), &b));
    assert!(
        lines.contains(
            &"Course: title changed from \"Workplace Safety Basics\" to \"New title\"".to_owned()
        ),
        "{lines:#?}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l.contains("theme accent changed from \"#2F6FEB\" to \"#FF0000\""))
    );
    assert!(
        lines
            .iter()
            .any(|l| l.contains("player show menu changed from true to false"))
    );
    assert!(
        lines
            .iter()
            .any(|l| l.contains("slide size w changed from 1280 to 1920"))
    );
    assert!(lines.iter().any(|l| l.contains("quiz cleared")));
}

#[test]
fn moving_a_slide_within_a_scene_is_a_move() {
    let mut b = hello();
    let slide = b.scenes[0].slides.remove(3);
    b.scenes[0].slides.insert(0, slide);
    let changes = diff(&hello(), &b);
    assert_eq!(changes.len(), 1, "{changes:#?}");
    assert_eq!(changes[0].kind, ChangeKind::Moved);
    assert_eq!(
        changes[0].sentence(),
        "Slide \"Results\": moved to position 1 in scene \"Main\""
    );
}

#[test]
fn removing_the_first_slide_does_not_move_the_rest() {
    let mut b = hello();
    b.scenes[0].slides.remove(0);
    let changes = diff(&hello(), &b);
    assert_eq!(changes.len(), 1, "{changes:#?}");
    assert_eq!(changes[0].kind, ChangeKind::Removed);
}

#[test]
fn moving_a_slide_to_another_scene_is_a_move() {
    let mut b = hello();
    let slide = b.scenes[0].slides.pop().unwrap();
    b.scenes.push(Scene {
        id: "sc-end".into(),
        title: "End".into(),
        slides: vec![slide],
    });
    let lines = sentences(&diff(&hello(), &b));
    assert!(
        lines.contains(&"Scene \"End\": scene \"End\" added".to_owned()),
        "{lines:#?}"
    );
    assert!(
        lines.contains(&"Slide \"Results\": moved from scene \"Main\" to scene \"End\"".to_owned()),
        "{lines:#?}"
    );
    assert!(
        !lines
            .iter()
            .any(|l| l.contains("removed") || l.contains("slide \"Results\" added"))
    );
}

#[test]
fn scenes_and_slides_added_removed_renamed() {
    let mut b = hello();
    b.scenes[0].title = "Primary".into();
    b.scenes[0].slides[1].title = "Hazards".into();
    let mut extra = b.scenes[0].slides[0].clone();
    extra.id = "s-new".into();
    extra.title = "Bonus".into();
    extra.objects.clear();
    extra.layers.clear();
    extra.triggers.clear();
    b.scenes[0].slides.push(extra);
    b.scenes[0].slides.remove(2);
    let changes = diff(&hello(), &b);
    let kinds: Vec<(ChangeKind, String)> = changes
        .iter()
        .map(|c| (c.kind, c.summary.clone()))
        .collect();
    assert!(
        kinds.contains(&(
            ChangeKind::Renamed,
            "renamed from \"Main\" to \"Primary\"".into()
        )),
        "{kinds:#?}"
    );
    assert!(kinds.contains(&(
        ChangeKind::Renamed,
        "renamed from \"Three hazards\" to \"Hazards\"".into()
    )));
    assert!(
        kinds
            .iter()
            .any(|(k, s)| *k == ChangeKind::Added && s.starts_with("slide \"Bonus\" added"))
    );
    assert!(kinds.iter().any(|(k, s)| *k == ChangeKind::Removed
        && s.starts_with("slide \"Check your knowledge\" removed")));
}

#[test]
fn objects_added_removed_and_moved_between_layers() {
    let mut b = hello();
    let s = slide_mut(&mut b, "s-intro");
    let removed = s.objects.remove(2);
    s.layers[0].objects.push(removed);
    let mut added = s.objects[0].clone();
    added.id = "extra".into();
    added.name = "Extra".into();
    s.objects.push(added);
    let lines = sentences(&diff(&hello(), &b));
    assert!(lines.iter().any(|l| l == "Slide \"Welcome\": object \"btn-about\" moved from the base layer to layer \"About\""), "{lines:#?}");
    assert!(
        lines
            .iter()
            .any(|l| l == "Slide \"Welcome\": object \"Extra\" (text) added"),
        "{lines:#?}"
    );
    let mut c = hello();
    slide_mut(&mut c, "s-intro").objects.remove(0);
    let lines = sentences(&diff(&hello(), &c));
    assert_eq!(
        lines,
        vec!["Slide \"Welcome\": object \"title\" (text) removed"]
    );
}

#[test]
fn object_changes_name_the_part() {
    let mut b = hello();
    {
        let o = object_mut(&mut b, "s-intro", "btn-start");
        o.frame.x = 99.0;
        o.frame.w = 250.0;
        o.description = Some("Starts the course".into());
        o.opacity = 0.5;
        o.name = "Start".into();
        o.states.push(crate::StateDef {
            name: "hover".into(),
            fill: None,
            text: None,
            src: None,
        });
        o.timing.start_ms = 2000;
        if let ObjectKind::Button { fill, .. } = &mut o.kind {
            *fill = crate::Color("#00FF00".into());
        }
    }
    let lines = sentences(&diff(&hello(), &b));
    let has = |needle: &str| lines.iter().any(|l| l.contains(needle));
    assert!(has("renamed from \"btn-start\""), "{lines:#?}");
    assert!(
        has("position of \"Start\" changed from (80, 520) to (99, 520)"),
        "{lines:#?}"
    );
    assert!(
        has("size of \"Start\" changed from 200 × 60 to 250 × 60"),
        "{lines:#?}"
    );
    assert!(
        has("accessibility text of \"Start\" set to \"Starts the course\""),
        "{lines:#?}"
    );
    assert!(has("opacity of \"Start\" set to 0.5"), "{lines:#?}");
    assert!(has("states of \"Start\" set to"), "{lines:#?}");
    assert!(
        has("timing start ms of \"Start\" changed from 1000 to 2000"),
        "{lines:#?}"
    );
    assert!(
        has("fill of \"Start\" changed from \"#2F6FEB\" to \"#00FF00\"")
            || has("fill of \"Start\""),
        "{lines:#?}"
    );
}

#[test]
fn media_source_change_is_media() {
    let mut a = hello();
    let template = slide_mut(&mut a, "s-results").objects[0].clone();
    slide_mut(&mut a, "s-results").objects.push(Object {
        kind: ObjectKind::Image {
            src: "a.png".into(),
            alt: String::new(),
            crop: None,
        },
        id: "pic".into(),
        ..template
    });
    let mut b = a.clone();
    if let ObjectKind::Image { src, .. } = &mut slide_mut(&mut b, "s-results").objects[1].kind {
        *src = "b.png".into();
    }
    let lines = sentences(&diff(&a, &b));
    assert_eq!(lines.len(), 1, "{lines:#?}");
    assert!(
        lines[0].contains("media of") && lines[0].contains("from \"a.png\" to \"b.png\""),
        "{lines:#?}"
    );
}

#[test]
fn triggers_added_removed_and_changed() {
    let mut b = hello();
    let s = slide_mut(&mut b, "s-intro");
    let removed = s.triggers.remove(0);
    let mut edited = s.triggers[0].clone();
    edited.action = crate::Action::ShowLayer {
        layer: "other".into(),
    };
    s.triggers[0] = edited;
    let mut added = removed.clone();
    added.when = Event::TimelineStarts { layer: None };
    s.triggers.push(added);
    s.layers[0].triggers.clear();
    let changes = diff(&hello(), &b);
    let lines = sentences(&changes);
    assert!(
        lines
            .iter()
            .any(|l| l.contains("action changed from \"show layer about\" to \"show layer other\"")),
        "{lines:#?}"
    );
    assert!(
        lines.iter().any(|l| l.contains(
            "trigger \"when click object btn-start, jump to slide target next\" removed"
        )),
        "{lines:#?}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l.contains("removed from layer \"About\"")),
        "{lines:#?}"
    );
    assert!(changes.iter().any(|c| c.kind == ChangeKind::Added));
}

#[test]
fn variables_banks_resources_glossary() {
    let mut b = hello();
    b.variables[0].value = crate::Value::Number(5.0);
    b.variables.push(crate::VariableDef {
        name: "extra".into(),
        value: crate::Value::Bool(true),
    });
    b.resources.push(crate::Resource {
        id: "r1".into(),
        title: "Handbook".into(),
        target: crate::ResourceTarget::Url {
            url: "https://example.com".into(),
        },
    });
    b.glossary.push(crate::GlossaryEntry {
        id: "g1".into(),
        term: "PPE".into(),
        definition: "Gear".into(),
    });
    b.banks.push(crate::QuestionBank {
        id: "bank1".into(),
        title: "Pool".into(),
        slides: vec![],
    });
    let lines = sentences(&diff(&hello(), &b));
    let has = |needle: &str| lines.iter().any(|l| l.contains(needle));
    assert!(
        has("variable \"tabs_seen\" value changed from 0.0 to 5.0")
            || has("variable \"tabs_seen\" value changed from 0 to 5"),
        "{lines:#?}"
    );
    assert!(has("variable \"extra\" added"));
    assert!(has("resource \"Handbook\" added"));
    assert!(has("glossary entry \"PPE\" added"));
    assert!(has("question bank \"Pool\" added"));
    let reverse = sentences(&diff(&b, &hello()));
    assert!(
        reverse
            .iter()
            .any(|l| l.contains("variable \"extra\" removed"))
    );
    assert!(
        reverse
            .iter()
            .any(|l| l.contains("question bank \"Pool\" removed"))
    );
}

#[test]
fn media_files_added_removed_and_changed() {
    let a: MediaHashes = [
        ("a.png".to_owned(), 1),
        ("b.png".to_owned(), 2),
        ("c.png".to_owned(), 3),
    ]
    .into();
    let b: MediaHashes = [
        ("b.png".to_owned(), 2),
        ("c.png".to_owned(), 9),
        ("d.png".to_owned(), 4),
    ]
    .into();
    let changes = diff_with_media(&hello(), &hello(), Some((&a, &b)));
    let lines = sentences(&changes);
    assert_eq!(lines.len(), 3, "{lines:#?}");
    assert!(
        lines
            .iter()
            .any(|l| l == "Media: media file \"a.png\" removed")
    );
    assert!(
        lines
            .iter()
            .any(|l| l == "Media: media file \"c.png\" has different content")
    );
    assert!(
        lines
            .iter()
            .any(|l| l == "Media: media file \"d.png\" added")
    );
    assert!(diff_with_media(&hello(), &hello(), None).is_empty());
}

#[test]
fn question_changes_are_listed() {
    let mut b = hello();
    slide_mut(&mut b, "s-quiz")
        .question
        .as_mut()
        .unwrap()
        .prompt = "New prompt".into();
    let lines = sentences(&diff(&hello(), &b));
    assert_eq!(lines.len(), 1);
    assert!(
        lines[0].contains("question prompt changed from"),
        "{lines:#?}"
    );
}

#[test]
fn changes_serialise_to_json() {
    let mut b = hello();
    b.title = "T".into();
    let json = serde_json::to_value(diff(&hello(), &b)).unwrap();
    let first = &json[0];
    assert_eq!(first["kind"], "changed");
    assert_eq!(first["before"], "Workplace Safety Basics");
    assert_eq!(first["after"], "T");
    assert!(first["path"].is_string());
}

#[test]
fn reordering_objects_is_a_stacking_move() {
    let mut b = hello();
    let s = slide_mut(&mut b, "s-intro");
    let last = s.objects.pop().unwrap();
    s.objects.insert(0, last);
    let lines = sentences(&diff(&hello(), &b));
    assert_eq!(lines.len(), 1, "{lines:#?}");
    assert!(lines[0].contains("moved in stacking order"));
}
