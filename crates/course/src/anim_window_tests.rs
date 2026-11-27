//! Validation warnings for animations that the object's time window cuts off.

use crate::{AnimEffect, Animation, Course, Easing, Emphasis, EmphasisEffect, Issue, Severity};

const HELLO: &str = include_str!("../../../examples/hello-course/course.json");

fn late_issues(course: &Course) -> Vec<Issue> {
    course
        .validate()
        .into_iter()
        .filter(|i| i.message.contains("\"late\""))
        .collect()
}

#[test]
fn animation_window_warnings_name_the_slide_and_object() {
    let mut course = Course::from_json(HELLO).expect("hello course");
    let slide = &mut course.scenes[0].slides[0];
    slide.duration_ms = 2000;
    let slide_id = slide.id.clone();
    let mut o = slide.objects[0].clone();
    o.id = "late".to_owned();
    o.timing.start_ms = 1800;
    o.timing.end_ms = None;
    o.entrance = Some(Animation {
        effect: AnimEffect {
            id: "fade".to_owned(),
            ..AnimEffect::default()
        },
        duration_ms: 1000,
        easing: Easing::default(),
        sequence: Default::default(),
    });
    o.exit = None;
    o.emphasis = vec![Emphasis {
        effect: EmphasisEffect {
            id: "pulse".to_owned(),
        },
        start_ms: 0,
        duration_ms: 500,
        easing: Easing::default(),
        repeat: 1,
    }];
    slide.objects.push(o);
    let issues = late_issues(&course);
    let paths: Vec<&str> = issues.iter().map(|i| i.path.as_str()).collect();
    assert!(
        issues.iter().all(|i| i.severity == Severity::Warning),
        "{issues:#?}"
    );
    assert!(
        paths.iter().any(|p| p.ends_with("/entrance")),
        "{issues:#?}"
    );
    assert!(
        paths.iter().any(|p| p.ends_with("/emphasis/0")),
        "{issues:#?}"
    );
    assert!(
        issues
            .iter()
            .all(|i| i.message.contains(&format!("{slide_id:?}"))),
        "{issues:#?}"
    );
    // An object that fits warns about nothing.
    let o = course.scenes[0].slides[0].objects.last_mut().unwrap();
    o.timing.start_ms = 0;
    o.entrance.as_mut().unwrap().duration_ms = 500;
    o.emphasis[0].start_ms = 600;
    assert!(late_issues(&course).is_empty(), "no warning expected");
}

#[test]
fn exit_longer_than_the_window_warns() {
    let mut course = Course::from_json(HELLO).expect("hello course");
    let slide = &mut course.scenes[0].slides[0];
    slide.duration_ms = 5000;
    let mut o = slide.objects[0].clone();
    o.id = "late".to_owned();
    o.timing.start_ms = 1000;
    o.timing.end_ms = Some(1400);
    o.entrance = None;
    o.emphasis.clear();
    o.exit = Some(Animation {
        effect: AnimEffect {
            id: "fade".to_owned(),
            ..AnimEffect::default()
        },
        duration_ms: 500,
        easing: Easing::default(),
        sequence: Default::default(),
    });
    slide.objects.push(o);
    let issues = late_issues(&course);
    assert!(
        issues.iter().any(|i| i.path.ends_with("/exit")),
        "{issues:#?}"
    );
}
