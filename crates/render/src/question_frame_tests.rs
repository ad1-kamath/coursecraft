//! A question's optional `frame` holds its prompt and answer rows. Without one the layout is
//! the original full-width grid.

use coursecraft_course::{Course, Size};
use coursecraft_logic::{Input, Player};
use egui::{Rect, pos2, vec2};

use crate::compose::{DrawItem, Hit, Visual, compose, compose_authoring, question_prompt_rect_in};
use crate::layout::SlideTransform;

fn course(frame: Option<serde_json::Value>) -> Course {
    let mut question = serde_json::json!({
        "kind": "multiple_choice",
        "prompt": "Which one?",
        "choices": [
            { "id": "a", "text": "Alpha", "correct": true },
            { "id": "b", "text": "Beta", "correct": false },
            { "id": "c", "text": "Gamma", "correct": false },
            { "id": "d", "text": "Delta", "correct": false }
        ]
    });
    if let Some(frame) = frame {
        question["frame"] = frame;
    }
    let json = serde_json::json!({
        "schema_version": 1, "id": "qf", "title": "Question frame",
        "slide_size": { "w": 1280, "h": 720 },
        "scenes": [{ "id": "s1", "title": "Scene", "slides": [{
            "id": "slide1", "title": "Slide", "duration_ms": 5000,
            "objects": [],
            "question": question
        }] }]
    });
    Course::from_json(&json.to_string()).expect("valid course")
}

fn framed() -> Course {
    course(Some(
        serde_json::json!({ "x": 700, "y": 100, "w": 400, "h": 400 }),
    ))
}

fn frame_rect() -> Rect {
    Rect::from_min_size(pos2(700.0, 100.0), vec2(400.0, 400.0))
}

fn question_items(items: &[DrawItem]) -> Vec<&DrawItem> {
    items
        .iter()
        .filter(|i| matches!(i.visual, Visual::Prompt { .. } | Visual::ChoiceRow { .. }))
        .collect()
}

fn started(course: &Course) -> Player<'_> {
    let mut player = Player::new(course).expect("player");
    player.handle(&Input::Start);
    player
}

#[test]
fn a_question_without_a_frame_keeps_the_original_grid() {
    let c = course(None);
    let player = started(&c);
    let items = compose(&player);
    let rects: Vec<Rect> = question_items(&items).iter().map(|i| i.rect).collect();
    assert_eq!(rects.len(), 5);
    assert_eq!(
        rects[0],
        Rect::from_min_size(pos2(80.0, 60.0), vec2(1120.0, 80.0))
    );
    for (i, rect) in rects[1..].iter().enumerate() {
        assert_eq!(
            *rect,
            Rect::from_min_size(pos2(80.0, 160.0 + 72.0 * i as f32), vec2(900.0, 56.0))
        );
    }
}

#[test]
fn a_narrow_frame_keeps_the_prompt_and_every_row_inside_it() {
    let c = framed();
    let slide = c.slide("slide1").expect("slide");
    let player = started(&c);
    for (name, items) in [
        ("player", compose(&player)),
        ("authoring", compose_authoring(&c, slide, None)),
    ] {
        let found = question_items(&items);
        assert_eq!(found.len(), 5, "{name}: prompt and four rows");
        for item in found {
            let r = item.rect;
            assert!(
                r.min.x >= 700.0 - 0.01
                    && r.max.x <= 1100.0 + 0.01
                    && r.min.y >= 100.0 - 0.01
                    && r.max.y <= 500.0 + 0.01,
                "{name}: {r:?} is inside {:?}",
                frame_rect()
            );
        }
    }
    // The prompt rect the editor and the accessibility tree use is the drawn prompt.
    let items = compose(&player);
    let prompt = items
        .iter()
        .find(|i| matches!(i.visual, Visual::Prompt { .. }))
        .expect("prompt");
    let frame = c
        .slide("slide1")
        .and_then(|s| s.question.as_ref())
        .and_then(|q| q.frame);
    assert_eq!(
        prompt.rect,
        question_prompt_rect_in(1280.0, 720.0, frame.as_ref())
    );
}

#[test]
fn a_bad_frame_falls_back_to_the_original_grid() {
    let c = course(Some(
        serde_json::json!({ "x": 10, "y": 10, "w": 0, "h": 50 }),
    ));
    let player = started(&c);
    let items = compose(&player);
    let found = question_items(&items);
    assert_eq!(found[0].rect.min, pos2(80.0, 60.0));
}

#[test]
fn clicking_a_drawn_row_selects_that_answer() {
    let c = framed();
    let player = started(&c);
    let items = compose(&player);
    let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 720.0));
    let t = SlideTransform::fit(screen, Size { w: 1280, h: 720 }).expect("fit");
    let click = |at: egui::Pos2| {
        let mut pointer = crate::view::PointerState::default();
        let events = vec![
            egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::default(),
            },
            egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::default(),
            },
        ];
        pointer.handle_events(&events, &t, &items, &|_| false)
    };
    for id in ["a", "b", "c", "d"] {
        let row = items
            .iter()
            .find(|i| i.hit == Hit::Choice(id.to_owned()))
            .expect("row")
            .rect;
        assert_eq!(
            click(row.center()),
            vec![Input::SelectChoice {
                choice: id.to_owned()
            }],
            "a click in the middle of row {id}"
        );
    }
    // Where the rows used to be (the old full-width grid) there is nothing to click now.
    assert!(click(pos2(200.0, 190.0)).is_empty());
}
