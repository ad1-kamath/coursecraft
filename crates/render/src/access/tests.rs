use std::collections::BTreeSet;

use coursecraft_course::Course;
use coursecraft_logic::{Input, Player};
use serde_json::{Value, json};

use super::{AccessNode, AccessRole, AccessTree, announcement, build_access_tree};
use crate::PlayerFrame;
use crate::compose::compose;
use crate::focus::FocusTarget;

const SHOWCASE_JSON: &str = include_str!("../../../../examples/showcase/course.json");

fn course(slides: Value) -> Course {
    let text = json!({
        "schema_version": 1,
        "id": "access-test",
        "title": "Access Test",
        "scenes": [{ "id": "sc", "title": "Scene", "slides": slides }]
    })
    .to_string();
    Course::from_json(&text).expect("valid course")
}

fn player(course: &Course) -> Player<'_> {
    let mut player = Player::new(course).expect("valid player");
    player.handle(&Input::Start);
    player
}

fn tree_for(player: &Player<'_>, show_nav: bool, show_menu: bool) -> AccessTree {
    let mut frame = PlayerFrame::new();
    let drawer = frame.drawer_open();
    frame
        .focus_mut()
        .rebuild(player, show_nav, show_menu, drawer);
    let items = compose(player);
    build_access_tree(player, &frame, &items, show_nav, show_menu)
}

fn text_object(id: &str, text: &str) -> Value {
    json!({
        "id": id,
        "kind": { "type": "text", "text": text },
        "frame": { "x": 0.0, "y": 0.0, "w": 200.0, "h": 40.0 }
    })
}

fn button_object(id: &str, label: &str) -> Value {
    json!({
        "id": id,
        "kind": { "type": "button", "label": label },
        "frame": { "x": 0.0, "y": 0.0, "w": 100.0, "h": 40.0 }
    })
}

fn table_object(id: &str) -> Value {
    json!({
        "id": id,
        "name": "Scores",
        "kind": {
            "type": "table",
            "columns": [0.5, 0.5],
            "rows": [
                {
                    "height": 0.4,
                    "cells": [
                        { "text": "Name" },
                        { "text": "Score" }
                    ]
                },
                {
                    "height": 0.6,
                    "cells": [
                        { "text": "Ada" },
                        { "text": "10" }
                    ]
                }
            ],
            "header_row": true
        },
        "frame": { "x": 0.0, "y": 0.0, "w": 400.0, "h": 200.0 }
    })
}

fn shape_object(id: &str, text: &str) -> Value {
    json!({
        "id": id,
        "kind": {
            "type": "shape",
            "shape": "rectangle",
            "fill": "#D0D7DE",
            "text": { "text": text }
        },
        "frame": { "x": 0.0, "y": 0.0, "w": 200.0, "h": 80.0 }
    })
}

fn targets(tree: &AccessTree) -> Vec<FocusTarget> {
    tree.nodes
        .iter()
        .filter_map(|node| node.target.clone())
        .collect()
}

fn find(tree: &AccessTree, predicate: impl Fn(&AccessNode) -> bool) -> Option<&AccessNode> {
    tree.nodes.iter().find(|node| predicate(node))
}

#[test]
fn every_showcase_slide_interactive_order_matches_the_focus_manager() {
    let course = Course::from_json(SHOWCASE_JSON).expect("valid showcase");
    let boxed: &'static Course = Box::leak(Box::new(course.clone()));
    let mut player = Player::new(boxed).expect("valid player");
    player.handle(&Input::Start);
    let slide_ids: Vec<String> = boxed.slides().map(|s| s.id.clone()).collect();
    assert!(!slide_ids.is_empty());

    for slide_id in slide_ids {
        player.handle(&Input::GoTo {
            slide: slide_id.clone(),
        });
        let mut frame = PlayerFrame::new();
        let drawer = frame.drawer_open();
        frame.focus_mut().rebuild(&player, true, true, drawer);
        let items = compose(&player);
        let tree = build_access_tree(&player, &frame, &items, true, true);

        assert_eq!(
            targets(&tree),
            frame.focus().list().to_vec(),
            "interactive order on slide {slide_id}"
        );
        assert_eq!(
            tree.nodes.first().map(|n| n.role),
            Some(AccessRole::Heading),
            "slide {slide_id} starts with its title"
        );
        let mut keys = BTreeSet::new();
        for node in &tree.nodes {
            assert!(keys.insert(node.key.clone()), "duplicate key {}", node.key);
        }
    }
}

#[test]
fn text_links_are_announced_as_links_after_their_object() {
    let course = course(json!([{
        "id": "s1",
        "title": "Links",
        "objects": [{
            "id": "link-text",
            "kind": {
                "type": "text",
                "text": "Read more",
                "runs": [{
                    "text": "Read more",
                    "link": { "type": "url", "url": "https://example.com" }
                }]
            },
            "frame": { "x": 0.0, "y": 0.0, "w": 200.0, "h": 40.0 }
        }]
    }]));
    let player = player(&course);
    let tree = tree_for(&player, false, false);
    let link_index = tree
        .nodes
        .iter()
        .position(|node| node.role == AccessRole::Link)
        .expect("link node");
    assert!(
        tree.nodes[..link_index]
            .iter()
            .any(|node| node.role == AccessRole::Text),
        "the link is read after its text"
    );
    let node = &tree.nodes[link_index];
    assert_eq!(node.label, "https://example.com");
    assert!(matches!(
        node.target,
        Some(FocusTarget::Link {
            ref object,
            link: coursecraft_course::Link::Url { .. },
            ..
        }) if object == "link-text"
    ));
}

#[test]
fn transport_controls_have_roles_labels_and_values() {
    let course = Course::from_json(SHOWCASE_JSON).expect("valid showcase");
    let player = player(&course);
    let tree = tree_for(&player, true, true);
    let chrome = |button: crate::focus::ChromeButton| {
        tree.nodes
            .iter()
            .find(|node| node.target == Some(FocusTarget::Chrome(button.clone())))
    };

    let play = chrome(crate::focus::ChromeButton::PlayPause).expect("play/pause node");
    assert_eq!(play.role, AccessRole::Button);
    assert_eq!(play.label, "Pause");

    let seek = chrome(crate::focus::ChromeButton::Seek).expect("seek node");
    assert_eq!(seek.role, AccessRole::Slider);
    assert_eq!(seek.label, "Slide position");
    assert!(seek.value.as_deref().is_some_and(|v| v.contains(" / ")));

    let mute = chrome(crate::focus::ChromeButton::Mute).expect("mute node");
    assert_eq!(mute.label, "Mute");

    let volume = chrome(crate::focus::ChromeButton::Volume).expect("volume node");
    assert_eq!(volume.role, AccessRole::Slider);
    assert_eq!(volume.label, "Volume");
    assert!(volume.value.as_deref().is_some_and(|v| v.ends_with('%')));
}

#[test]
fn decorative_hidden_and_variable_text_are_handled() {
    let mut decorative = text_object("deco", "Not read");
    decorative["decorative"] = json!(true);
    let mut hidden = text_object("hidden", "Hidden text");
    hidden["initial_state"] = json!("hidden");
    let course = course(json!([{
        "id": "s1",
        "title": "Reading",
        "objects": [
            decorative,
            hidden,
            text_object("live", "Hello %name%"),
            text_object("hostile", "<script>alert(1)</script>")
        ]
    }]));
    let player = player(&course);
    let tree = tree_for(&player, false, false);
    let labels: Vec<&str> = tree.nodes.iter().map(|n| n.label.as_str()).collect();
    assert!(
        labels.contains(&"Hello %name%"),
        "unresolved variable kept plain"
    );
    assert!(
        labels.contains(&"<script>alert(1)</script>"),
        "hostile text is not markup"
    );
    assert!(
        !labels.contains(&"Not read"),
        "decorative object is left out"
    );
    assert!(
        !labels.contains(&"Hidden text"),
        "hidden object is left out"
    );
}

#[test]
fn variable_text_is_resolved() {
    let text = json!({
        "schema_version": 1,
        "id": "access-var",
        "title": "Variables",
        "variables": [{ "name": "name", "value": "Ada" }],
        "scenes": [{ "id": "sc", "title": "Scene", "slides": [{
            "id": "s1",
            "title": "Reading",
            "objects": [text_object("live", "Hello %name%")]
        }]}]
    })
    .to_string();
    let course = Course::from_json(&text).expect("valid course");
    let player = player(&course);
    let tree = tree_for(&player, false, false);
    assert!(
        tree.nodes.iter().any(|n| n.label == "Hello Ada"),
        "variable substituted"
    );
}

#[test]
fn label_fallback_order_is_text_then_description_then_name_then_id() {
    let course = course(json!([{
        "id": "s1",
        "title": "Labels",
        "objects": [
            {
                "id": "btn-text",
                "name": "Name",
                "description": "Description",
                "kind": { "type": "button", "label": "Text" },
                "frame": { "x": 0.0, "y": 0.0, "w": 10.0, "h": 10.0 }
            },
            {
                "id": "btn-description",
                "name": "Name",
                "description": "Description",
                "kind": { "type": "shape", "shape": "rectangle", "fill": "#ffffff" },
                "frame": { "x": 0.0, "y": 0.0, "w": 10.0, "h": 10.0 }
            },
            {
                "id": "btn-name",
                "name": "Name",
                "kind": { "type": "shape", "shape": "rectangle", "fill": "#ffffff" },
                "frame": { "x": 0.0, "y": 0.0, "w": 10.0, "h": 10.0 }
            },
            {
                "id": "btn-id",
                "kind": { "type": "shape", "shape": "rectangle", "fill": "#ffffff" },
                "frame": { "x": 0.0, "y": 0.0, "w": 10.0, "h": 10.0 }
            }
        ],
        "triggers": [
            { "action": { "action": "jump_to_slide", "target": "next" }, "when": { "event": "click", "object": "btn-description" } },
            { "action": { "action": "jump_to_slide", "target": "next" }, "when": { "event": "click", "object": "btn-name" } },
            { "action": { "action": "jump_to_slide", "target": "next" }, "when": { "event": "click", "object": "btn-id" } }
        ]
    }]));
    let player = player(&course);
    let tree = tree_for(&player, false, false);
    let label = |id: &str| {
        find(
            &tree,
            |n| matches!(&n.target, Some(FocusTarget::Object(hit)) if hit == id),
        )
        .map(|n| n.label.as_str())
    };
    assert_eq!(label("btn-text"), Some("Text"));
    assert_eq!(label("btn-description"), Some("Description"));
    assert_eq!(label("btn-name"), Some("Name"));
    assert_eq!(label("btn-id"), Some("btn-id"));
}

#[test]
fn choice_roles_follow_the_question_kind() {
    let course = course(json!([{
        "id": "s1",
        "title": "Choice",
        "objects": [],
        "question": {
            "kind": "multiple_choice",
            "prompt": "Pick",
            "choices": [
                { "id": "a", "text": "A", "correct": true },
                { "id": "b", "text": "B" }
            ]
        }
    }]));
    let player = player(&course);
    let tree = tree_for(&player, false, false);
    let roles: Vec<AccessRole> = tree
        .nodes
        .iter()
        .filter(|n| matches!(n.target, Some(FocusTarget::Choice(_))))
        .map(|n| n.role)
        .collect();
    assert_eq!(roles, vec![AccessRole::Radio, AccessRole::Radio]);
    assert!(
        tree.nodes
            .iter()
            .any(|n| n.label == "Pick" && n.role == AccessRole::Text)
    );
}

#[test]
fn multiple_response_choices_are_checkboxes() {
    let course = course(json!([{
        "id": "s1",
        "title": "Choice",
        "objects": [],
        "question": {
            "kind": "multiple_response",
            "prompt": "Pick",
            "choices": [
                { "id": "a", "text": "A", "correct": true },
                { "id": "b", "text": "B" }
            ]
        }
    }]));
    let player = player(&course);
    let tree = tree_for(&player, false, false);
    let roles: Vec<AccessRole> = tree
        .nodes
        .iter()
        .filter(|n| matches!(n.target, Some(FocusTarget::Choice(_))))
        .map(|n| n.role)
        .collect();
    assert_eq!(roles, vec![AccessRole::Checkbox, AccessRole::Checkbox]);
}

#[test]
fn true_false_choices_are_radios() {
    let course = course(json!([{
        "id": "s1",
        "title": "Choice",
        "objects": [],
        "question": {
            "kind": "true_false",
            "prompt": "True?",
            "choices": [
                { "id": "true", "text": "True", "correct": true },
                { "id": "false", "text": "False" }
            ]
        }
    }]));
    let player = player(&course);
    let tree = tree_for(&player, false, false);
    assert!(
        tree.nodes
            .iter()
            .filter(|n| matches!(n.target, Some(FocusTarget::Choice(_))))
            .all(|n| n.role == AccessRole::Radio)
    );
}

#[test]
fn essay_fill_in_is_a_text_area_and_short_answer_a_text_box() {
    let essay_course = course(json!([{
        "id": "s1",
        "title": "Essay",
        "objects": [],
        "question": {
            "kind": "essay",
            "prompt": "Tell us more",
            "choices": []
        }
    }]));
    let essay_player = player(&essay_course);
    let tree = tree_for(&essay_player, false, false);
    let node = tree
        .nodes
        .iter()
        .find(|n| n.target == Some(FocusTarget::FillIn))
        .expect("essay node");
    assert_eq!(node.role, AccessRole::TextArea);

    let short_course = course(json!([{
        "id": "s1",
        "title": "Short",
        "objects": [],
        "question": {
            "kind": "short_answer",
            "prompt": "Your name",
            "choices": []
        }
    }]));
    let short_player = player(&short_course);
    let tree = tree_for(&short_player, false, false);
    let node = tree
        .nodes
        .iter()
        .find(|n| n.target == Some(FocusTarget::FillIn))
        .expect("short answer node");
    assert_eq!(node.role, AccessRole::TextBox);
}

#[test]
fn fill_in_is_a_text_box_with_the_prompt_and_value() {
    let course = course(json!([{
        "id": "s1",
        "title": "Fill",
        "objects": [],
        "question": {
            "kind": "fill_in",
            "prompt": "Your name",
            "choices": [],
            "accepted": ["Ada"]
        }
    }]));
    let mut player = player(&course);
    player.handle(&Input::TextEntered {
        text: "Ada".to_owned(),
    });
    let tree = tree_for(&player, false, false);
    let node = find(&tree, |n| n.role == AccessRole::TextBox).expect("text box");
    assert_eq!(node.label, "Your name");
    assert_eq!(node.value.as_deref(), Some("Ada"));
}

#[test]
fn matching_rows_are_selects_with_options() {
    let course = course(json!([{
        "id": "s1",
        "title": "Match",
        "objects": [],
        "question": {
            "kind": "matching",
            "prompt": "Match",
            "choices": [],
            "pairs": [
                { "id": "a", "left": "Left A", "right": "Right A" },
                { "id": "b", "left": "Left B", "right": "Right B" }
            ]
        }
    }]));
    let mut player = player(&course);
    player.handle(&Input::MatchSelected {
        left: "a".to_owned(),
        right: Some("b".to_owned()),
    });
    let tree = tree_for(&player, false, false);
    let node = find(&tree, |n| n.role == AccessRole::Select).expect("select");
    assert_eq!(node.label, "Left A");
    assert_eq!(node.value.as_deref(), Some("b"));
    assert_eq!(node.options.first().map(|o| o.0.as_str()), Some(""));
    assert!(
        node.options
            .iter()
            .any(|(value, label)| value == "b" && label == "Right B")
    );
}

#[test]
fn hotspot_is_a_slider_with_a_hint_and_marker_count() {
    let course = course(json!([{
        "id": "s1",
        "title": "Hotspot",
        "objects": [{
            "id": "img",
            "kind": { "type": "image", "src": "a.png", "alt": "Floor plan" },
            "frame": { "x": 0.0, "y": 0.0, "w": 100.0, "h": 100.0 }
        }],
        "question": {
            "kind": "hotspot",
            "prompt": "Place it",
            "choices": [],
            "hotspot_image": "img",
            "regions": [{ "id": "r", "shape": "rect", "x": 0.0, "y": 0.0, "w": 10.0, "h": 10.0, "correct": true }]
        }
    }]));
    let mut player = player(&course);
    player.handle(&Input::HotspotClicked { x: 5.0, y: 5.0 });
    let tree = tree_for(&player, false, false);
    let node = find(&tree, |n| n.role == AccessRole::Slider).expect("slider");
    assert_eq!(node.label, "Place a marker on the image");
    assert_eq!(node.value.as_deref(), Some("1 placed"));
    assert_eq!(
        node.hint.as_deref(),
        Some("Use the arrow keys, then press Enter.")
    );
}

#[test]
fn sequence_controls_say_direction_and_position() {
    let course = course(json!([{
        "id": "s1",
        "title": "Sequence",
        "objects": [],
        "question": {
            "kind": "sequence",
            "prompt": "Order",
            "choices": [
                { "id": "one", "text": "One" },
                { "id": "two", "text": "Two" },
                { "id": "three", "text": "Three" }
            ]
        }
    }]));
    let player = player(&course);
    let tree = tree_for(&player, false, false);
    let moves: Vec<&str> = tree
        .nodes
        .iter()
        .filter(|n| n.role == AccessRole::Button && n.label.starts_with("Move "))
        .map(|n| n.label.as_str())
        .collect();
    assert!(moves.iter().any(|l| l.ends_with("down, position 1 of 3")));
    assert!(moves.iter().any(|l| l.ends_with("up, position 2 of 3")));
}

#[test]
fn drag_items_are_pressed_buttons_with_a_hint() {
    let course = course(json!([{
        "id": "s1",
        "title": "Drag",
        "objects": [
            {
                "id": "item",
                "kind": { "type": "text", "text": "Apple" },
                "frame": { "x": 0.0, "y": 0.0, "w": 100.0, "h": 40.0 }
            },
            {
                "id": "target",
                "kind": { "type": "text", "text": "Fruit" },
                "frame": { "x": 200.0, "y": 0.0, "w": 100.0, "h": 40.0 }
            }
        ],
        "question": {
            "kind": "drag_drop",
            "prompt": "Drag",
            "choices": [],
            "drag_items": [{ "item": "item", "target": "target" }],
            "drop_targets": ["target"]
        }
    }]));
    let mut player = player(&course);
    player.handle(&Input::DragDropped {
        item: "item".to_owned(),
        target: Some("target".to_owned()),
        x: 200.0,
        y: 0.0,
    });
    let tree = tree_for(&player, false, false);
    let node = find(
        &tree,
        |n| matches!(&n.target, Some(FocusTarget::DragItem(id)) if id == "item"),
    )
    .expect("drag item");
    assert_eq!(node.role, AccessRole::Button);
    assert_eq!(node.pressed, Some(false));
    assert!(node.label.ends_with(", on Fruit"));
    assert!(
        node.hint
            .as_deref()
            .is_some_and(|h| h.contains("Press Space"))
    );
}

#[test]
fn an_image_without_alt_or_description_stays_with_an_empty_label() {
    let course = course(json!([{
        "id": "s1",
        "title": "Image",
        "objects": [{
            "id": "img",
            "kind": { "type": "image", "src": "a.png" },
            "frame": { "x": 0.0, "y": 0.0, "w": 10.0, "h": 10.0 }
        }]
    }]));
    let player = player(&course);
    let tree = tree_for(&player, false, false);
    let node = find(&tree, |n| n.role == AccessRole::Image).expect("image");
    assert_eq!(node.label, "");
}

#[test]
fn announcement_for_a_slide_change_names_the_position_and_title() {
    let course = course(json!([
        { "id": "s1", "title": "One", "objects": [text_object("a", "A")] },
        { "id": "s2", "title": "Two", "objects": [text_object("b", "B")] }
    ]));
    let mut player = player(&course);
    let first = tree_for(&player, false, false);
    assert_eq!(
        announcement(None, &first, &player).as_deref(),
        Some("Slide 1 of 2: One")
    );
    player.handle(&Input::Next);
    let second = tree_for(&player, false, false);
    assert_eq!(
        announcement(Some(&first), &second, &player).as_deref(),
        Some("Slide 2 of 2: Two")
    );
}

#[test]
fn announcement_uses_the_drawn_total_and_position() {
    fn question_slide(id: &str) -> Value {
        json!({
            "id": id,
            "title": id,
            "question": {
                "kind": "multiple_choice",
                "prompt": "Pick one",
                "choices": [
                    { "id": "a", "text": "A" },
                    { "id": "b", "text": "B", "correct": true }
                ],
                "attempts": 1
            },
            "triggers": [{
                "action": { "action": "submit_question" },
                "when": { "event": "submit" }
            }]
        })
    }
    let bank_slides: Vec<Value> = (1..=5).map(|i| question_slide(&format!("bq{i}"))).collect();
    let text = json!({
        "schema_version": 1,
        "id": "draw-access",
        "title": "Draw Access",
        "scenes": [{ "id": "sc", "title": "Scene", "slides": [
            { "id": "s-intro", "title": "Intro" },
            { "id": "s-draw", "title": "Draw", "draw": { "bank": "bank1", "count": 2 } },
            { "id": "s-end", "title": "End" }
        ]}],
        "banks": [{ "id": "bank1", "title": "Bank One", "slides": bank_slides }]
    })
    .to_string();
    let course = Course::from_json(&text).expect("valid course");
    let mut player = player(&course);
    // Drawn total is 4 (intro + 2 drawn + end), not the 3 scene slides or 8 total.
    let first = tree_for(&player, false, false);
    assert_eq!(
        announcement(None, &first, &player).as_deref(),
        Some("Slide 1 of 4: Intro")
    );
    player.handle(&Input::Next);
    let drawn_title = player.current_slide().unwrap().title.clone();
    let second = tree_for(&player, false, false);
    assert_eq!(
        announcement(Some(&first), &second, &player).as_deref(),
        Some(format!("Slide 2 of 4: {drawn_title}").as_str())
    );
}

#[test]
fn announcement_reads_a_newly_visible_layer() {
    let course = course(json!([{
        "id": "s1",
        "title": "Layer",
        "objects": [button_object("open", "Open")],
        "layers": [{
            "id": "feedback",
            "name": "Feedback",
            "objects": [text_object("msg", "Well done")]
        }],
        "triggers": [{
            "action": { "action": "show_layer", "layer": "feedback" },
            "when": { "event": "click", "object": "open" }
        }]
    }]));
    let mut player = player(&course);
    let before = tree_for(&player, false, false);
    player.handle(&Input::Click {
        object: "open".to_owned(),
    });
    let after = tree_for(&player, false, false);
    assert_eq!(
        announcement(Some(&before), &after, &player).as_deref(),
        Some("Well done")
    );
}

fn drag_node(label: &str, pressed: Option<bool>) -> AccessNode {
    AccessNode {
        key: "drag:s1:item".to_owned(),
        role: AccessRole::Button,
        label: label.to_owned(),
        value: None,
        options: Vec::new(),
        checked: None,
        pressed,
        expanded: None,
        hint: None,
        target: Some(FocusTarget::DragItem("item".to_owned())),
        rect: egui::Rect::NOTHING,
        screen: false,
    }
}

fn drag_tree(node: AccessNode) -> AccessTree {
    AccessTree {
        title: "Drag".to_owned(),
        nodes: vec![node],
    }
}

#[test]
fn announcement_for_drag_pick_up_hover_and_drop() {
    let course = course(json!([{
        "id": "s1",
        "title": "Drag",
        "objects": [
            { "id": "item", "kind": { "type": "text", "text": "Apple" }, "frame": { "x": 0.0, "y": 0.0, "w": 10.0, "h": 10.0 } },
            { "id": "target", "kind": { "type": "text", "text": "Fruit" }, "frame": { "x": 20.0, "y": 0.0, "w": 10.0, "h": 10.0 } }
        ],
        "question": {
            "kind": "drag_drop",
            "prompt": "Drag",
            "choices": [],
            "drag_items": [{ "item": "item", "target": "target" }],
            "drop_targets": ["target"]
        }
    }]));
    let player = player(&course);
    let idle = drag_tree(drag_node("Apple", Some(false)));
    let picked = drag_tree(drag_node("Apple, on Fruit", Some(true)));
    let moved = drag_tree(drag_node("Apple, on Basket", Some(true)));
    let dropped = drag_tree(drag_node("Apple, on Basket", Some(false)));
    assert_eq!(
        announcement(Some(&idle), &picked, &player).as_deref(),
        Some("Picked up Apple")
    );
    assert_eq!(
        announcement(Some(&picked), &moved, &player).as_deref(),
        Some("Apple over Basket")
    );
    assert_eq!(
        announcement(Some(&moved), &dropped, &player).as_deref(),
        Some("Apple dropped on Basket")
    );
}

#[test]
fn announcement_is_none_when_nothing_changed() {
    let course = course(json!([{
        "id": "s1",
        "title": "Still",
        "objects": [text_object("a", "A")]
    }]));
    let player = player(&course);
    let tree = tree_for(&player, false, false);
    assert_eq!(announcement(Some(&tree), &tree, &player), None);
}

fn tree_with_notice(player: &Player<'_>, notice: Option<&str>) -> (PlayerFrame, AccessTree) {
    let mut frame = PlayerFrame::new();
    frame.notice = notice.map(str::to_owned);
    let showing = frame.showing_notice().is_some();
    frame.focus_mut().set_notice_visible(showing);
    let drawer = frame.drawer_open();
    frame.focus_mut().rebuild(player, false, false, drawer);
    let items = compose(player);
    let tree = build_access_tree(player, &frame, &items, false, false);
    (frame, tree)
}

#[test]
fn the_notice_is_read_first_with_its_dismiss_button() {
    let course = course(json!([{
        "id": "s1",
        "title": "One",
        "objects": [button_object("a", "A")]
    }]));
    let player = player(&course);
    let (_frame, tree) = tree_with_notice(&player, Some("Progress is not saved"));
    let notice = tree.nodes.first().expect("notice node");
    assert_eq!(notice.key, "notice");
    assert_eq!(notice.label, "Progress is not saved");
    assert_eq!(
        targets(&tree).first(),
        Some(&FocusTarget::Chrome(
            crate::focus::ChromeButton::DismissNotice
        )),
        "the dismiss button is first in the focus order"
    );
    assert!(
        find(&tree, |n| matches!(
            &n.target,
            Some(FocusTarget::Chrome(
                crate::focus::ChromeButton::DismissNotice
            ))
        ),)
        .is_some_and(|n| n.label == "Dismiss" && n.role == AccessRole::Button),
        "the close button is labelled for screen readers"
    );

    // Without a notice nothing is added.
    let (_frame, plain) = tree_with_notice(&player, None);
    assert!(!plain.nodes.iter().any(|n| n.key == "notice"));
    assert_eq!(
        targets(&plain).first(),
        Some(&FocusTarget::Object("a".into()))
    );
}

#[test]
fn the_notice_is_announced_once_when_it_appears() {
    let course = course(json!([{
        "id": "s1",
        "title": "One",
        "objects": [text_object("a", "A")]
    }]));
    let player = player(&course);
    let (_frame, before) = tree_with_notice(&player, None);
    let (_frame, after) = tree_with_notice(&player, Some("Progress is not saved"));
    assert_eq!(
        announcement(Some(&before), &after, &player).as_deref(),
        Some("Progress is not saved"),
        "the notice text is announced when the banner appears"
    );
    // It is not announced again on later frames.
    assert_eq!(announcement(Some(&after), &after, &player), None);
}

#[test]
fn a_chrome_override_shows_in_the_notice_banner() {
    let mut course = course(json!([{
        "id": "s1",
        "title": "One",
        "objects": [text_object("a", "A")]
    }]));
    for (key, _) in coursecraft_course::CHROME_STRINGS {
        course.strings.insert((*key).to_owned(), format!("[{key}]"));
    }
    let player = player(&course);
    let (_frame, tree) = tree_with_notice(
        &player,
        Some(player.course().chrome_string("reportingLost")),
    );
    let notice = tree
        .nodes
        .iter()
        .find(|n| n.key == "notice")
        .expect("notice");
    assert_eq!(notice.label, "[reportingLost]");
    assert!(
        find(&tree, |n| matches!(
            &n.target,
            Some(FocusTarget::Chrome(
                crate::focus::ChromeButton::DismissNotice
            ))
        ),)
        .is_some_and(|n| n.label == "[dismissNotice]"),
        "the dismiss button uses the override too"
    );
}

#[test]
fn answer_required_announcement_text_is_stable() {
    assert_eq!(
        super::ANSWER_REQUIRED_ANNOUNCEMENT,
        "Please answer the question"
    );
}

fn chrome_overrides() -> Course {
    let mut course = course(json!([{
        "id": "s1",
        "title": "Slide",
        "objects": [
            text_object("a", "Apple"),
            text_object("b", "Basket"),
            {
                "id": "item",
                "kind": { "type": "text", "text": "Apple" },
                "frame": { "x": 0.0, "y": 0.0, "w": 10.0, "h": 10.0 }
            }
        ],
        "question": {
            "kind": "drag_drop",
            "prompt": "Drag",
            "choices": [],
            "drag_items": [{ "item": "item", "target": "b" }],
            "drop_targets": ["b"]
        }
    }]));
    for (key, _) in coursecraft_course::CHROME_STRINGS {
        course.strings.insert((*key).to_owned(), format!("[{key}]"));
    }
    course
}

#[test]
fn chrome_overrides_reach_access_labels_and_hints() {
    let plain = chrome_overrides();
    let overridden = chrome_overrides();
    let plain_player = player(&plain);
    let player = player(&overridden);
    let plain_tree = tree_for(&plain_player, true, true);
    let tree = tree_for(&player, true, true);
    let node_text = |tree: &AccessTree| -> String {
        tree.nodes
            .iter()
            .flat_map(|node| {
                std::iter::once(node.label.clone())
                    .chain(node.hint.clone())
                    .chain(node.options.iter().map(|(_, label)| label.clone()))
                    .chain(node.value.clone())
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let before = node_text(&plain_tree);
    let after = node_text(&tree);
    for (key, default) in coursecraft_course::CHROME_STRINGS {
        if !before.contains(default) {
            continue;
        }
        let marker = format!("[{key}]");
        assert!(
            after.contains(&marker),
            "chrome key {key} did not replace {default:?} in the access tree"
        );
        assert!(
            !after.contains(default),
            "chrome key {key} still shows {default:?}"
        );
    }
}

#[test]
fn chrome_overrides_change_the_slide_announcement() {
    let course = chrome_overrides();
    let player = player(&course);
    let tree = tree_for(&player, false, false);
    let text = announcement(None, &tree, &player).expect("announcement");
    assert_eq!(text, "[slideAnnouncement]");
}

#[test]
fn no_learner_facing_english_default_remains_with_overrides() {
    let mut course = Course::from_json(SHOWCASE_JSON).expect("valid showcase");
    for (key, _) in coursecraft_course::CHROME_STRINGS {
        course.strings.insert((*key).to_owned(), format!("[{key}]"));
    }
    let boxed: &'static Course = Box::leak(Box::new(course));
    let mut player = Player::new(boxed).expect("valid player");
    player.handle(&Input::Start);
    let slide_ids: Vec<String> = boxed.slides().map(|slide| slide.id.clone()).collect();
    // Only defaults that are unmistakably player chrome, so a slide that
    // legitimately says "up" or "down" is not a false failure.
    let defaults: Vec<&str> = coursecraft_course::CHROME_STRINGS
        .iter()
        .map(|(_, text)| *text)
        .filter(|text| text.len() >= 5)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    for slide_id in slide_ids {
        player.handle(&Input::GoTo {
            slide: slide_id.clone(),
        });
        let mut frame = PlayerFrame::new();
        let drawer = frame.drawer_open();
        frame.focus_mut().rebuild(&player, true, true, drawer);
        let items = compose(&player);
        let tree = build_access_tree(&player, &frame, &items, true, true);
        for node in &tree.nodes {
            for default in &defaults {
                assert!(
                    !node.label.contains(default),
                    "slide {slide_id} node {} leaked {default:?}",
                    node.key
                );
                if let Some(hint) = &node.hint {
                    assert!(
                        !hint.contains(default),
                        "slide {slide_id} node {} hint leaked {default:?}",
                        node.key
                    );
                }
            }
        }
    }
}

#[test]
fn master_text_and_alt_text_are_read_before_the_slides_own() {
    let text = json!({
        "schema_version": 1,
        "id": "access-master",
        "title": "Access Master",
        "scenes": [{
            "id": "sc",
            "title": "Scene",
            "slides": [{
                "id": "s1",
                "title": "Slide",
                "master": "m1",
                "objects": [text_object("own", "Slide text")]
            }]
        }],
        "masters": [{
            "id": "m1",
            "title": "Master",
            "objects": [
                text_object("mtext", "Master text"),
                {
                    "id": "mimg",
                    "kind": { "type": "image", "src": "x.png", "alt": "Master alt" },
                    "frame": { "x": 0.0, "y": 50.0, "w": 100.0, "h": 40.0 }
                },
                {
                    "id": "mdeco",
                    "kind": { "type": "text", "text": "Decorative" },
                    "frame": { "x": 0.0, "y": 100.0, "w": 100.0, "h": 40.0 },
                    "decorative": true
                }
            ]
        }]
    })
    .to_string();
    let course = Course::from_json(&text).expect("valid course");
    let player = player(&course);
    let tree = tree_for(&player, false, false);
    let labels: Vec<&str> = tree.nodes.iter().map(|node| node.label.as_str()).collect();
    let master_text = labels.iter().position(|label| *label == "Master text");
    let own_text = labels.iter().position(|label| *label == "Slide text");
    assert!(master_text.is_some() && own_text.is_some(), "{labels:?}");
    assert!(master_text < own_text, "{labels:?}");
    assert!(labels.contains(&"Master alt"));
    assert!(!labels.contains(&"Decorative"));
    assert!(
        tree.nodes
            .iter()
            .filter(|node| node.key.starts_with("master:"))
            .all(|node| node.target.is_none())
    );
}

#[test]
fn base_objects_and_question_leave_the_tree_when_a_layer_hides_the_base() {
    let course = course(json!([{
        "id": "s1",
        "title": "Slide 1",
        "duration_ms": 5000,
        "objects": [
            button_object("btn-show", "Show"),
            text_object("base-text", "Base text")
        ],
        "question": {
            "kind": "multiple_choice",
            "prompt": "Pick one",
            "choices": [
                { "id": "a", "text": "A", "correct": true },
                { "id": "b", "text": "B" }
            ]
        },
        "layers": [{
            "id": "modal",
            "name": "Modal",
            "duration_ms": 5000,
            "hide_other_layers": false,
            "hide_base_objects": true,
            "objects": [text_object("modal-text", "On the layer")],
            "triggers": []
        }],
        "triggers": [{
            "when": { "event": "click", "object": "btn-show" },
            "action": { "action": "show_layer", "layer": "modal" }
        }]
    }]));
    let mut player = player(&course);
    player.handle(&Input::Click {
        object: "btn-show".to_owned(),
    });
    assert!(player.base_objects_hidden());
    let tree = tree_for(&player, true, true);
    assert!(
        !tree
            .nodes
            .iter()
            .any(|node| node.key.starts_with("object:s1:")),
        "base content leaves the tree"
    );
    assert!(
        !tree.nodes.iter().any(|node| node.key == "prompt:s1"),
        "the base question leaves the tree"
    );
    assert!(
        tree.nodes
            .iter()
            .any(|node| node.key == "layer:s1:modal:modal-text"),
        "layer content stays"
    );
}

#[test]
fn shape_text_is_the_accessible_name_and_description_wins() {
    let with_text = course(json!([{
        "id": "s1",
        "title": "Slide",
        "objects": [shape_object("shape", "Inside the shape")]
    }]));
    let first_player = player(&with_text);
    let tree = tree_for(&first_player, false, false);
    assert!(
        tree.nodes
            .iter()
            .any(|node| node.key == "object:s1:shape" && node.label == "Inside the shape"),
        "{:?}",
        tree.nodes
    );

    let described_course = course(json!([{
        "id": "s1",
        "title": "Slide",
        "objects": [{
            "id": "shape",
            "description": "Described shape",
            "kind": {
                "type": "shape",
                "shape": "rectangle",
                "fill": "#D0D7DE",
                "text": { "text": "Inside the shape" }
            },
            "frame": { "x": 0.0, "y": 0.0, "w": 200.0, "h": 80.0 }
        }]
    }]));
    let described_player = player(&described_course);
    let tree = tree_for(&described_player, false, false);
    assert!(
        tree.nodes
            .iter()
            .any(|node| node.key == "object:s1:shape" && node.label == "Described shape"),
        "{:?}",
        tree.nodes
    );
}

#[test]
fn table_content_is_a_table_with_rows_and_header_cells() {
    let course = course(json!([{
        "id": "s1",
        "title": "Slide",
        "objects": [table_object("scores")]
    }]));
    let player = player(&course);
    let tree = tree_for(&player, false, false);
    let table = find(&tree, |node| node.role == AccessRole::Table).expect("table node");
    assert_eq!(table.label, "Scores");
    let rows: Vec<&AccessNode> = tree
        .nodes
        .iter()
        .filter(|node| node.role == AccessRole::Row)
        .collect();
    assert_eq!(rows.len(), 2, "{:?}", tree.nodes);
    let headers: Vec<&str> = tree
        .nodes
        .iter()
        .filter(|node| node.role == AccessRole::ColumnHeader)
        .map(|node| node.label.as_str())
        .collect();
    assert_eq!(headers, vec!["Name", "Score"]);
    let cells: Vec<&str> = tree
        .nodes
        .iter()
        .filter(|node| node.role == AccessRole::Cell)
        .map(|node| node.label.as_str())
        .collect();
    assert_eq!(cells, vec!["Ada", "10"]);
}

fn input_access_course(kind: Value, variable: Value) -> Course {
    Course::from_json(
        &json!({
            "schema_version": 1,
            "id": "input-access",
            "title": "Input Access",
            "variables": [variable],
            "scenes": [{ "id": "sc", "title": "Scene", "slides": [{
                "id": "s1",
                "title": "Slide",
                "objects": [{
                    "id": "input",
                    "name": "My input",
                    "kind": kind,
                    "frame": { "x": 0.0, "y": 0.0, "w": 200.0, "h": 60.0 }
                }]
            }] }]
        })
        .to_string(),
    )
    .expect("valid input course")
}

#[test]
fn marker_is_a_button_with_expanded_state_and_reads_its_card() {
    let course = course(json!([{
        "id": "s1",
        "title": "Slide",
        "objects": [{
            "id": "marker",
            "name": "Tip marker",
            "kind": { "type": "marker", "title": "Tip", "body": "Body text" },
            "frame": { "x": 0.0, "y": 0.0, "w": 40.0, "h": 40.0 }
        }]
    }]));
    let mut player = player(&course);
    let tree = tree_for(&player, false, false);
    let node = find(&tree, |n| {
        n.target == Some(FocusTarget::Object("marker".into()))
    })
    .expect("marker node");
    assert_eq!(node.role, AccessRole::Button);
    assert_eq!(node.label, "Tip");
    assert_eq!(node.expanded, Some(false));
    assert!(
        !tree.nodes.iter().any(|n| n.key.starts_with("marker-card:")),
        "a closed marker exposes no card text"
    );

    player.handle(&Input::MarkerToggled {
        object: "marker".into(),
    });
    let tree = tree_for(&player, false, false);
    let node = find(&tree, |n| {
        n.target == Some(FocusTarget::Object("marker".into()))
    })
    .expect("marker node");
    assert_eq!(node.expanded, Some(true));
    let card = find(&tree, |n| n.key.starts_with("marker-card:")).expect("open marker card text");
    assert_eq!(card.role, AccessRole::Text);
    assert_eq!(card.label, "Tip\nBody text");
}

#[test]
fn a_child_scrolled_out_of_a_panel_is_not_announced_until_it_is_in_view() {
    let course = course(json!([{
        "id": "s1",
        "title": "Slide",
        "objects": [
            { "id": "panel",
              "kind": { "type": "scroll_panel", "content_height": 600.0 },
              "frame": { "x": 100.0, "y": 100.0, "w": 300.0, "h": 200.0 } },
            { "id": "child", "panel": "panel", "name": "Child",
              "kind": { "type": "button", "label": "Child" },
              "frame": { "x": 120.0, "y": 500.0, "w": 100.0, "h": 50.0 } }
        ]
    }]));
    let mut player = player(&course);
    let tree = tree_for(&player, false, false);
    assert!(
        !tree
            .nodes
            .iter()
            .any(|n| n.target == Some(FocusTarget::Object("child".into()))),
        "a child wholly outside the viewport is not announced"
    );

    player.handle(&Input::PanelScrolled {
        object: "panel".into(),
        offset: 400.0,
    });
    let tree = tree_for(&player, false, false);
    assert!(
        tree.nodes
            .iter()
            .any(|n| n.target == Some(FocusTarget::Object("child".into()))),
        "the scrolled-in child is announced"
    );
}

#[test]
fn text_entry_is_a_text_box_with_its_variable_value() {
    let course = input_access_course(
        json!({ "type": "text_entry", "variable": "name", "placeholder": "Your name", "max_chars": 20 }),
        json!({ "name": "name", "value": "Ada" }),
    );
    let entry_player = player(&course);
    let tree = tree_for(&entry_player, false, false);
    let node = find(&tree, |n| {
        n.target == Some(FocusTarget::Object("input".into()))
    })
    .expect("input node");
    assert_eq!(node.role, AccessRole::TextBox);
    assert_eq!(node.label, "My input");
    assert_eq!(node.value.as_deref(), Some("Ada"));

    let course = input_access_course(
        json!({ "type": "text_entry", "variable": "name", "multiline": true }),
        json!({ "name": "name", "value": "a\nb" }),
    );
    let multiline = player(&course);
    let tree = tree_for(&multiline, false, false);
    let node = find(&tree, |n| {
        n.target == Some(FocusTarget::Object("input".into()))
    })
    .expect("input node");
    assert_eq!(node.role, AccessRole::TextArea);
}

#[test]
fn sliders_and_dials_report_their_range() {
    for kind in [
        json!({ "type": "slider", "variable": "n", "min": 0, "max": 10, "step": 1 }),
        json!({ "type": "dial", "variable": "n", "min": 0, "max": 10, "step": 1 }),
    ] {
        let course = input_access_course(kind, json!({ "name": "n", "value": 4 }));
        let tree = tree_for(&player(&course), false, false);
        let node = find(&tree, |n| {
            n.target == Some(FocusTarget::Object("input".into()))
        })
        .expect("input node");
        assert_eq!(node.role, AccessRole::Slider);
        assert_eq!(node.value.as_deref(), Some("4"));
        assert_eq!(
            node.options,
            vec![
                ("min".to_owned(), "0".to_owned()),
                ("max".to_owned(), "10".to_owned()),
                ("now".to_owned(), "4".to_owned())
            ]
        );
    }
}

#[test]
fn checkbox_and_radio_group_announce_state_and_options() {
    let course = input_access_course(
        json!({ "type": "checkbox", "variable": "on", "label": "Agree" }),
        json!({ "name": "on", "value": true }),
    );
    let tree = tree_for(&player(&course), false, false);
    let node = find(&tree, |n| {
        n.target == Some(FocusTarget::Object("input".into()))
    })
    .expect("checkbox node");
    assert_eq!(node.role, AccessRole::Checkbox);
    assert_eq!(node.checked, Some(true));

    let course = input_access_course(
        json!({ "type": "radio_group", "variable": "choice", "options": [
            { "label": "A", "value": "a" },
            { "label": "B", "value": "b" }
        ] }),
        json!({ "name": "choice", "value": "b" }),
    );
    let tree = tree_for(&player(&course), false, false);
    let node = find(&tree, |n| {
        n.target == Some(FocusTarget::Object("input".into()))
    })
    .expect("radio node");
    assert_eq!(node.role, AccessRole::RadioGroup);
    assert_eq!(node.value.as_deref(), Some("b"));
    assert_eq!(
        node.options,
        vec![
            ("a".to_owned(), "A".to_owned()),
            ("b".to_owned(), "B".to_owned())
        ]
    );
}

#[test]
fn lightbox_is_a_modal_dialog_labelled_by_its_slide_title() {
    let course = course(json!([
        {
            "id": "base",
            "title": "Base",
            "objects": [
                { "id": "open", "kind": { "type": "button", "label": "Open" }, "frame": { "x": 0.0, "y": 0.0, "w": 100.0, "h": 40.0 } }
            ],
            "triggers": [{
                "action": { "action": "lightbox_slide", "slide": "box" },
                "when": { "event": "click", "object": "open" }
            }]
        },
        {
            "id": "box",
            "title": "Box Title",
            "objects": [
                { "id": "box-btn", "kind": { "type": "button", "label": "Box" }, "frame": { "x": 0.0, "y": 0.0, "w": 100.0, "h": 40.0 } }
            ]
        }
    ]));
    let mut player = player(&course);
    player.handle(&Input::Click {
        object: "open".into(),
    });
    let tree = tree_for(&player, true, true);
    let dialog = tree
        .nodes
        .iter()
        .find(|node| node.role == AccessRole::Dialog)
        .expect("lightbox dialog node");
    assert_eq!(dialog.label, "Box Title");
    assert!(
        tree.nodes.iter().any(|node| node.label == "Close lightbox"),
        "close button is read inside the dialog"
    );
}

#[test]
fn draggable_objects_are_buttons_with_a_drag_hint() {
    let course = course(json!([{
        "id": "s1",
        "title": "Slide",
        "objects": [
            { "id": "card", "name": "Card",
              "kind": { "type": "shape", "shape": "rectangle", "fill": "#112233" },
              "frame": { "x": 0.0, "y": 0.0, "w": 40.0, "h": 40.0 },
              "drag": { "targets": ["bin"] } },
            { "id": "bin", "name": "Bin",
              "kind": { "type": "shape", "shape": "rectangle", "fill": "#334455" },
              "frame": { "x": 200.0, "y": 0.0, "w": 80.0, "h": 80.0 } }
        ]
    }]));
    let player = player(&course);
    let tree = tree_for(&player, false, false);
    let node = tree
        .nodes
        .iter()
        .find(|node| node.key.starts_with("drag:"))
        .expect("draggable node uses the drag key so the live region sees it");
    assert_eq!(node.role, AccessRole::Button);
    assert_eq!(node.label, "Card");
    assert_eq!(node.pressed, Some(false));
    assert_eq!(node.hint.as_deref(), Some(course.chrome_string("dragHint")));
}

#[test]
fn timer_and_progress_meter_are_announced_with_their_values() {
    let text = json!({
        "schema_version": 1,
        "id": "access-timer",
        "title": "Timer access",
        "variables": [{ "name": "n", "value": 50 }],
        "scenes": [{ "id": "sc", "title": "Scene", "slides": [{
            "id": "s1",
            "title": "Slide",
            "duration_ms": 300000,
            "objects": [
                { "id": "t1", "name": "Countdown",
                  "kind": { "type": "timer", "duration_ms": 90000 },
                  "frame": { "x": 0.0, "y": 0.0, "w": 200.0, "h": 60.0 } },
                { "id": "m1", "name": "Progress",
                  "kind": { "type": "progress_meter", "variable": "n", "min": 0, "max": 100 },
                  "frame": { "x": 0.0, "y": 80.0, "w": 200.0, "h": 40.0 } }
            ]
        }] }]
    })
    .to_string();
    let course = Course::from_json(&text).expect("valid course");
    let mut player = player(&course);
    // 30 seconds in: 60 seconds left on the timer.
    player.handle(&Input::Tick { ms: 30000 });
    let tree = tree_for(&player, false, false);

    let timer = tree
        .nodes
        .iter()
        .find(|node| node.role == AccessRole::Timer)
        .expect("timer node");
    assert_eq!(timer.label, "Countdown");
    assert_eq!(timer.value.as_deref(), Some("01:00"));

    let meter = tree
        .nodes
        .iter()
        .find(|node| node.role == AccessRole::ProgressBar)
        .expect("progress bar node");
    assert_eq!(meter.label, "Progress");
    assert_eq!(meter.value.as_deref(), Some("50%"));
    assert_eq!(
        meter.options,
        vec![
            ("min".to_owned(), "0".to_owned()),
            ("max".to_owned(), "100".to_owned()),
            ("now".to_owned(), "50".to_owned()),
        ]
    );
}

#[test]
fn a_timer_is_announced_at_each_minute_and_in_the_last_ten_seconds_only() {
    let text = json!({
        "schema_version": 1,
        "id": "access-timer-announce",
        "title": "Timer announce",
        "scenes": [{ "id": "sc", "title": "Scene", "slides": [{
            "id": "s1",
            "title": "Slide",
            "duration_ms": 300000,
            "objects": [
                { "id": "t1", "name": "Countdown",
                  "kind": { "type": "timer", "duration_ms": 130000 },
                  "frame": { "x": 0.0, "y": 0.0, "w": 200.0, "h": 60.0 } }
            ]
        }] }]
    })
    .to_string();
    let course = Course::from_json(&text).expect("valid course");
    let mut player = player(&course);
    let mut previous = tree_for(&player, false, false);
    // Tick in one-second steps and record which values get announced.
    let mut announced = Vec::new();
    for _ in 0..125 {
        player.handle(&Input::Tick { ms: 1000 });
        let tree = tree_for(&player, false, false);
        if let Some(text) = announcement(Some(&previous), &tree, &player) {
            // Skip the slide announcement for the first tree.
            if text.contains("Countdown") {
                announced.push(text);
            }
        }
        previous = tree;
    }
    // 130s down to 5s: whole minutes are 120, 60 and the last 10 seconds
    // 10..5 -- never one announcement per tick.
    assert_eq!(
        announced,
        vec![
            "Countdown: 02:00".to_owned(),
            "Countdown: 01:00".to_owned(),
            "Countdown: 00:10".to_owned(),
            "Countdown: 00:09".to_owned(),
            "Countdown: 00:08".to_owned(),
            "Countdown: 00:07".to_owned(),
            "Countdown: 00:06".to_owned(),
            "Countdown: 00:05".to_owned(),
        ]
    );
}
