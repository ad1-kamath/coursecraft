//! Answer review marks, the review banner and its accessible reading.

use coursecraft_course::Course;
use coursecraft_logic::{Input, Player};
use egui::{Event, Pos2, Rect, Vec2, pos2, vec2};
use serde_json::{Value, json};

use crate::PlayerFrame;
use crate::access::{AccessRole, AccessTree, announcement, build_access_tree};
use crate::compose::{Hit, ReviewMark, Visual, compose};
use crate::focus::{ChromeButton, FocusTarget};
use crate::player_frame::review_banner_text;
use crate::view::SlideView;

fn course_with_slides(slides: Vec<Value>) -> Course {
    let text = json!({
        "schema_version": 1,
        "id": "review-test",
        "title": "Review Test",
        "quiz": { "passing_percent": 50 },
        "scenes": [{ "id": "sc", "title": "Scene", "slides": slides }]
    })
    .to_string();
    Course::from_json(&text).expect("valid review course")
}

fn question_slide(id: &str, question: Value) -> Value {
    json!({
        "id": id,
        "title": id,
        "duration_ms": 1000,
        "objects": [
            { "id": "btn-submit", "kind": { "type": "button", "label": "Submit" }, "frame": { "x": 1000, "y": 600, "w": 200, "h": 60 } },
            { "id": "btn-review", "kind": { "type": "button", "label": "Review" }, "frame": { "x": 1000, "y": 500, "w": 200, "h": 60 } }
        ],
        "question": question,
        "triggers": [
            { "action": { "action": "submit_question" }, "when": { "event": "click", "object": "btn-submit" } },
            { "action": { "action": "review_quiz" }, "when": { "event": "click", "object": "btn-review" } }
        ]
    })
}

fn with_objects(mut slide: Value, objects: Vec<Value>) -> Value {
    if let Some(existing) = slide.get_mut("objects").and_then(Value::as_array_mut) {
        existing.extend(objects);
    }
    slide
}

fn box_object(id: &str, x: f32, y: f32, w: f32, h: f32) -> Value {
    json!({
        "id": id,
        "kind": { "type": "shape", "shape": "rectangle", "fill": "#D0D7DE" },
        "frame": { "x": x, "y": y, "w": w, "h": h }
    })
}

fn image_object(id: &str, x: f32, y: f32, w: f32, h: f32) -> Value {
    json!({
        "id": id,
        "kind": { "type": "image", "src": "hotspot.png", "alt": "" },
        "frame": { "x": x, "y": y, "w": w, "h": h }
    })
}

fn review_player(slides: Vec<Value>) -> Player<'static> {
    let course = course_with_slides(slides);
    let leaked: &'static Course = Box::leak(Box::new(course));
    let mut player = Player::new(leaked).expect("valid player");
    player.handle(&Input::Start);
    player
}

fn click(object: &str) -> Input {
    Input::Click {
        object: object.to_owned(),
    }
}

fn enter_review(player: &mut Player<'_>) {
    player.handle(&click("btn-submit"));
    player.handle(&click("btn-review"));
    assert!(player.is_reviewing(), "review should be showing");
}

fn choice_mark(items: &[crate::DrawItem], text: &str) -> Option<ReviewMark> {
    items.iter().find_map(|item| match &item.visual {
        Visual::ChoiceRow {
            text: row, mark, ..
        } if row == text => *mark,
        _ => None,
    })
}

fn choice_hit(items: &[crate::DrawItem], text: &str) -> Option<Hit> {
    items
        .iter()
        .find(|item| matches!(&item.visual, Visual::ChoiceRow { text: row, .. } if row == text))
        .map(|item| item.hit.clone())
}

fn multiple_response() -> Value {
    json!({
        "kind": "multiple_response",
        "prompt": "Pick the right answers",
        "points": 10,
        "choices": [
            { "id": "a", "text": "Right A", "correct": true },
            { "id": "b", "text": "Wrong B" },
            { "id": "c", "text": "Right C", "correct": true }
        ]
    })
}

#[test]
fn choice_marks_a_right_wrong_and_missed_answer() {
    let mut player = review_player(vec![question_slide("q1", multiple_response())]);
    player.handle(&Input::SelectChoice {
        choice: "a".to_owned(),
    });
    player.handle(&Input::SelectChoice {
        choice: "b".to_owned(),
    });
    enter_review(&mut player);

    let items = compose(&player);
    assert_eq!(choice_mark(&items, "Right A"), Some(ReviewMark::Correct));
    assert_eq!(choice_mark(&items, "Wrong B"), Some(ReviewMark::Incorrect));
    assert_eq!(
        choice_mark(&items, "Right C"),
        Some(ReviewMark::MissedCorrect)
    );
    assert_eq!(
        choice_hit(&items, "Right A"),
        Some(Hit::QuestionChrome("a".to_owned()))
    );
}

#[test]
fn a_wrong_fill_in_shows_the_correct_answer_row() {
    let mut player = review_player(vec![question_slide(
        "q1",
        json!({
            "kind": "fill_in",
            "prompt": "Capital of France?",
            "choices": [],
            "accepted": ["Paris", "paris"]
        }),
    )]);
    player.handle(&Input::TextEntered {
        text: "London".to_owned(),
    });
    enter_review(&mut player);

    let items = compose(&player);
    let field = items
        .iter()
        .find(|item| matches!(item.visual, Visual::FillIn { .. }))
        .expect("fill-in field");
    let Visual::FillIn { mark, .. } = &field.visual else {
        panic!("expected fill-in");
    };
    assert_eq!(*mark, Some(ReviewMark::Incorrect));
    assert!(
        items.iter().any(|item| matches!(
            &item.visual,
            Visual::CorrectAnswer { text } if text == "Correct answer: Paris"
        )),
        "the accepted answer is written under the field"
    );
}

#[test]
fn sequence_rows_carry_their_marks_and_the_buttons_are_gone() {
    let mut player = review_player(vec![question_slide(
        "q1",
        json!({
            "kind": "sequence",
            "prompt": "Put these in order",
            "choices": [
                { "id": "a", "text": "A" },
                { "id": "b", "text": "B" },
                { "id": "c", "text": "C" }
            ]
        }),
    )]);
    let order_after_submit = player.sequence_order().to_vec();
    enter_review(&mut player);
    let items = compose(&player);
    let rows: Vec<&crate::DrawItem> = items
        .iter()
        .filter(|item| matches!(item.visual, Visual::SequenceRow { .. }))
        .collect();
    assert_eq!(rows.len(), 3);
    let choices = [("a", "A"), ("b", "B"), ("c", "C")];
    for (i, row) in rows.iter().enumerate() {
        let Visual::SequenceRow { text, mark } = &row.visual else {
            continue;
        };
        let expected = if order_after_submit.get(i).map(String::as_str) == Some(choices[i].0) {
            ReviewMark::Correct
        } else {
            ReviewMark::Incorrect
        };
        assert_eq!(*mark, Some(expected), "row {i} ({text})");
    }
    assert!(
        !items
            .iter()
            .any(|item| matches!(item.visual, Visual::SequenceButton { .. })),
        "sequence buttons are not composed while reviewing"
    );
}

#[test]
fn matching_marks_rows_and_shows_the_right_answer_under_a_wrong_one() {
    let mut player = review_player(vec![question_slide(
        "q1",
        json!({
            "kind": "matching",
            "prompt": "Match",
            "choices": [],
            "pairs": [
                { "id": "p1", "left": "Sun", "right": "Star" },
                { "id": "p2", "left": "Moon", "right": "Satellite" }
            ]
        }),
    )]);
    player.handle(&Input::MatchSelected {
        left: "p1".to_owned(),
        right: Some("p1".to_owned()),
    });
    player.handle(&Input::MatchSelected {
        left: "p2".to_owned(),
        right: Some("p1".to_owned()),
    });
    enter_review(&mut player);

    let items = compose(&player);
    let marks: Vec<Option<ReviewMark>> = items
        .iter()
        .filter_map(|item| match &item.visual {
            Visual::MatchRow { id, mark, .. } => Some((id.as_str(), *mark)),
            _ => None,
        })
        .map(|(id, mark)| match id {
            "p1" => {
                assert_eq!(mark, Some(ReviewMark::Correct));
                mark
            }
            "p2" => {
                assert_eq!(mark, Some(ReviewMark::Incorrect));
                mark
            }
            _ => mark,
        })
        .collect();
    assert_eq!(marks.len(), 2);
    assert!(
        items.iter().any(|item| matches!(
            &item.visual,
            Visual::CorrectAnswer { text } if text == "Correct answer: Satellite"
        )),
        "the pair's right text is written under the wrong row"
    );
}

#[test]
fn hotspot_correct_regions_are_outlined() {
    let slide = with_objects(
        question_slide(
            "q1",
            json!({
                "kind": "hotspot",
                "prompt": "Click the right spot",
                "choices": [],
                "hotspot_image": "img",
                "regions": [
                    { "id": "r1", "shape": "rect", "x": 100.0, "y": 160.0, "w": 100.0, "h": 80.0, "correct": true },
                    { "id": "r2", "shape": "ellipse", "x": 320.0, "y": 160.0, "w": 90.0, "h": 90.0, "correct": false },
                    { "id": "r3", "shape": "ellipse", "x": 500.0, "y": 160.0, "w": 90.0, "h": 90.0, "correct": true }
                ]
            }),
        ),
        vec![image_object("img", 80.0, 140.0, 600.0, 300.0)],
    );
    let mut player = review_player(vec![slide]);
    player.handle(&Input::HotspotClicked { x: 120.0, y: 180.0 });
    enter_review(&mut player);

    let items = compose(&player);
    let outlines: Vec<bool> = items
        .iter()
        .filter_map(|item| match item.visual {
            Visual::RegionOutline { ellipse } => Some(ellipse),
            _ => None,
        })
        .collect();
    assert_eq!(outlines, vec![false, true], "only correct regions");
}

#[test]
fn drag_items_carry_their_own_marks() {
    let slide = with_objects(
        question_slide(
            "q1",
            json!({
                "kind": "drag_drop",
                "prompt": "Drop the items",
                "choices": [],
                "drag_items": [
                    { "item": "item-a", "target": "target-1" },
                    { "item": "item-b", "target": "target-2" }
                ],
                "drop_targets": ["target-1", "target-2"],
                "one_per_target": false
            }),
        ),
        vec![
            box_object("item-a", 100.0, 300.0, 100.0, 60.0),
            box_object("item-b", 240.0, 300.0, 100.0, 60.0),
            box_object("target-1", 100.0, 500.0, 160.0, 80.0),
            box_object("target-2", 320.0, 500.0, 160.0, 80.0),
        ],
    );
    let mut player = review_player(vec![slide]);
    player.handle(&Input::DragDropped {
        item: "item-a".to_owned(),
        target: Some("target-1".to_owned()),
        x: 0.0,
        y: 0.0,
    });
    player.handle(&Input::DragDropped {
        item: "item-b".to_owned(),
        target: None,
        x: 0.0,
        y: 0.0,
    });
    enter_review(&mut player);

    let items = compose(&player);
    let mark = |id: &str| {
        items
            .iter()
            .find(|item| item.hit == Hit::QuestionChrome(id.to_owned()))
            .map(|item| item.mark)
    };
    assert_eq!(mark("item-a"), Some(Some(ReviewMark::Correct)));
    assert_eq!(mark("item-b"), Some(Some(ReviewMark::Incorrect)));
}

#[test]
fn likert_choice_rows_never_get_marks() {
    let mut player = review_player(vec![question_slide(
        "q1",
        json!({
            "kind": "likert",
            "prompt": "How was it?",
            "choices": [
                { "id": "low", "text": "Poor" },
                { "id": "high", "text": "Great" }
            ]
        }),
    )]);
    player.handle(&Input::SelectChoice {
        choice: "high".to_owned(),
    });
    enter_review(&mut player);

    let items = compose(&player);
    assert!(
        items
            .iter()
            .all(|item| !matches!(item.visual, Visual::ChoiceRow { mark: Some(_), .. })),
        "survey choices show no mark"
    );
}

#[test]
fn the_banner_names_question_two_of_three_with_points() {
    let mut player = review_player(vec![
        question_slide(
            "q1",
            json!({
                "kind": "true_false",
                "prompt": "One",
                "points": 10,
                "choices": [
                    { "id": "true", "text": "True", "correct": true },
                    { "id": "false", "text": "False" }
                ]
            }),
        ),
        question_slide(
            "q2",
            json!({
                "kind": "true_false",
                "prompt": "Two",
                "points": 10,
                "choices": [
                    { "id": "true", "text": "True", "correct": true },
                    { "id": "false", "text": "False" }
                ]
            }),
        ),
        question_slide(
            "q3",
            json!({
                "kind": "true_false",
                "prompt": "Three",
                "points": 10,
                "choices": [
                    { "id": "true", "text": "True", "correct": true },
                    { "id": "false", "text": "False" }
                ]
            }),
        ),
    ]);
    for _ in 0..3 {
        player.handle(&Input::SelectChoice {
            choice: "true".to_owned(),
        });
        player.handle(&click("btn-submit"));
        if !player.is_reviewing() && player.current_slide().map(|s| s.id.as_str()) != Some("q3") {
            player.handle(&Input::Next);
        }
    }
    player.handle(&click("btn-review"));
    assert!(player.is_reviewing());
    player.handle(&Input::Next);
    assert_eq!(player.current_slide().map(|s| s.id.as_str()), Some("q2"));
    assert_eq!(
        review_banner_text(&player).as_deref(),
        Some("Reviewing answers · Question 2 of 3 · Correct · 10 of 10 points")
    );
}

struct Harness {
    ctx: egui::Context,
    frame: PlayerFrame,
    view: SlideView,
    size: Vec2,
    time: f64,
}

impl Harness {
    fn new(width: f32, height: f32) -> Self {
        Self {
            ctx: egui::Context::default(),
            frame: PlayerFrame::new(),
            view: SlideView::new(),
            size: vec2(width, height),
            time: 0.0,
        }
    }

    fn run(&mut self, player: &Player<'_>, events: Vec<Event>) -> crate::FrameOutput {
        self.time += 0.1;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), self.size)),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        let mut output = crate::FrameOutput::default();
        let frame = &mut self.frame;
        let view = &mut self.view;
        let full = self.ctx.run_ui(raw, |ui| {
            output = frame.show(ui, player, view, None);
        });
        full.drop_without_applying_deltas();
        output
    }
}

fn click_event(pos: Pos2) -> Vec<Event> {
    vec![
        Event::PointerMoved(pos),
        Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        },
        Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::default(),
        },
    ]
}

#[test]
fn the_exit_review_button_sends_the_runtime_input() {
    let mut player = review_player(vec![question_slide("q1", multiple_response())]);
    player.handle(&Input::SelectChoice {
        choice: "a".to_owned(),
    });
    enter_review(&mut player);

    let mut harness = Harness::new(1280.0, 720.0);
    harness.run(&player, Vec::new());
    let rect = harness
        .frame
        .chrome_rect(&ChromeButton::ExitReview)
        .expect("exit review button");
    let output = harness.run(&player, click_event(rect.center()));
    assert!(
        output.inputs.contains(&Input::ExitReview),
        "{:?}",
        output.inputs
    );
}

fn tree_for(player: &Player<'_>, frame: &mut PlayerFrame) -> AccessTree {
    let drawer = frame.drawer_open();
    frame.focus_mut().rebuild(player, false, false, drawer);
    let items = compose(player);
    build_access_tree(player, frame, &items, false, false)
}

#[test]
fn the_access_tree_carries_marks_and_announces_the_banner() {
    let mut player = review_player(vec![
        question_slide("q1", multiple_response()),
        question_slide("q2", multiple_response()),
    ]);
    player.handle(&Input::SelectChoice {
        choice: "a".to_owned(),
    });
    player.handle(&Input::SelectChoice {
        choice: "b".to_owned(),
    });
    player.handle(&click("btn-submit"));
    player.handle(&Input::Next);
    player.handle(&Input::SelectChoice {
        choice: "a".to_owned(),
    });
    player.handle(&click("btn-submit"));
    player.handle(&Input::Jump {
        slide: "q1".to_owned(),
    });

    let mut frame = PlayerFrame::new();
    let before = tree_for(&player, &mut frame);
    player.handle(&click("btn-review"));
    assert!(player.is_reviewing());
    let current = tree_for(&player, &mut frame);

    let value = |id: &str| {
        current
            .nodes
            .iter()
            .find(|node| node.target == Some(FocusTarget::Choice(id.to_owned())))
            .and_then(|node| node.value.clone())
    };
    assert_eq!(value("a").as_deref(), Some("Correct"));
    assert_eq!(value("b").as_deref(), Some("Incorrect"));
    assert_eq!(value("c").as_deref(), Some("Correct answer"));

    let banner = review_banner_text(&player).expect("review banner text");
    assert_eq!(announcement(Some(&before), &current, &player), Some(banner));

    player.handle(&Input::Next);
    assert_eq!(player.current_slide().map(|s| s.id.as_str()), Some("q2"));
    let next = tree_for(&player, &mut frame);
    let next_banner = review_banner_text(&player).expect("review banner text");
    assert_eq!(
        announcement(Some(&current), &next, &player),
        Some(next_banner)
    );
    assert!(
        next.nodes
            .iter()
            .any(|node| node.key == "review" && node.role == AccessRole::Text),
        "the review bar is a text node"
    );
}
