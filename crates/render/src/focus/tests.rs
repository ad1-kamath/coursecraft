use coursecraft_course::Course;
use coursecraft_logic::{Input, Player};
use serde_json::{Value, json};

use super::{
    ChromeButton, ExternalFocus, FocusManager, FocusTarget, build_list, focus_order_candidates,
};

fn course(slides: Value) -> Course {
    let text = json!({
        "schema_version": 1,
        "id": "focus-test",
        "title": "Focus Test",
        "scenes": [{ "id": "sc", "title": "Scene", "slides": slides }]
    })
    .to_string();
    Course::from_json(&text).expect("valid course")
}

fn button(id: &str) -> Value {
    json!({
        "id": id,
        "kind": { "type": "button", "label": id },
        "frame": { "x": 0.0, "y": 0.0, "w": 100.0, "h": 40.0 }
    })
}

fn player(course: &Course) -> Player<'_> {
    let mut player = Player::new(course).expect("valid player");
    player.handle(&Input::Start);
    player
}

fn objects(ids: &[&str]) -> Vec<FocusTarget> {
    ids.iter()
        .map(|id| FocusTarget::Object((*id).to_owned()))
        .collect()
}

/// The transport chrome that follows slide content once the course started.
fn transport() -> Vec<FocusTarget> {
    vec![
        FocusTarget::Chrome(ChromeButton::PlayPause),
        FocusTarget::Chrome(ChromeButton::Replay),
        FocusTarget::Chrome(ChromeButton::Seek),
        FocusTarget::Chrome(ChromeButton::Mute),
        FocusTarget::Chrome(ChromeButton::Volume),
    ]
}

fn with_transport(mut list: Vec<FocusTarget>) -> Vec<FocusTarget> {
    list.extend(transport());
    list
}

#[test]
fn focus_order_objects_come_first_and_ties_keep_default_order() {
    let mut a = button("btn-a");
    a["focus_order"] = json!(1);
    let mut c = button("btn-c");
    c["focus_order"] = json!(1);
    let mut d = button("btn-d");
    d["focus_order"] = json!(2);
    let course = course(json!([{
        "id": "s1",
        "title": "Slide",
        "objects": [a, button("btn-b"), c, d]
    }]));
    let player = player(&course);
    assert_eq!(
        build_list(&player, false, false, false),
        with_transport(objects(&["btn-a", "btn-c", "btn-d", "btn-b"]))
    );
}

#[test]
fn decorative_objects_are_never_focusable() {
    let mut hidden = button("btn-decorative");
    hidden["decorative"] = json!(true);
    let course = course(json!([{
        "id": "s1",
        "title": "Slide",
        "objects": [hidden, button("btn-real")]
    }]));
    let player = player(&course);
    assert_eq!(
        build_list(&player, false, false, false),
        with_transport(objects(&["btn-real"]))
    );
}

#[test]
fn locked_question_drops_its_controls() {
    let course = course(json!([{
        "id": "s1",
        "title": "Question",
        "objects": [button("btn-submit")],
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
    }]));
    let mut player = player(&course);
    assert_eq!(
        build_list(&player, false, false, false),
        with_transport(vec![
            FocusTarget::Choice("a".into()),
            FocusTarget::Choice("b".into()),
            FocusTarget::Object("btn-submit".into()),
        ])
    );
    player.handle(&Input::SelectChoice { choice: "b".into() });
    player.handle(&Input::Submit);
    assert!(player.question_locked());
    assert_eq!(
        build_list(&player, false, false, false),
        with_transport(objects(&["btn-submit"]))
    );
}

#[test]
fn prevent_base_clicks_removes_base_objects() {
    let course = course(json!([{
        "id": "s1",
        "title": "Layers",
        "objects": [
            button("btn-base"),
            {
                "id": "btn-open",
                "kind": { "type": "button", "label": "Open" },
                "frame": { "x": 0.0, "y": 0.0, "w": 100.0, "h": 40.0 }
            }
        ],
        "layers": [{
            "id": "l1",
            "name": "Layer",
            "prevent_base_clicks": true,
            "objects": [button("btn-layer")]
        }],
        "triggers": [{
            "action": { "action": "show_layer", "layer": "l1" },
            "when": { "event": "click", "object": "btn-open" }
        }]
    }]));
    let mut player = player(&course);
    assert_eq!(
        build_list(&player, false, false, false),
        with_transport(objects(&["btn-base", "btn-open"]))
    );
    player.handle(&Input::Click {
        object: "btn-open".into(),
    });
    assert_eq!(
        build_list(&player, false, false, false),
        with_transport(objects(&["btn-layer"]))
    );
}

#[test]
fn slide_change_resets_focus_to_the_first_item() {
    let course = course(json!([
        { "id": "s1", "title": "One", "objects": [button("btn-one")] },
        { "id": "s2", "title": "Two", "objects": [button("btn-two")] }
    ]));
    let mut player = player(&course);
    let mut focus = FocusManager::new();
    focus.rebuild(&player, false, false, false);
    assert_eq!(
        focus.focused(),
        Some(&FocusTarget::Object("btn-one".into()))
    );

    player.handle(&Input::Next);
    focus.rebuild(&player, false, false, false);
    assert_eq!(
        focus.focused(),
        Some(&FocusTarget::Object("btn-two".into()))
    );
}

#[test]
fn layer_shown_focuses_its_first_item_and_hidden_falls_back() {
    let course = course(json!([{
        "id": "s1",
        "title": "Layers",
        "objects": [button("btn-base")],
        "layers": [{
            "id": "l1",
            "name": "Layer",
            "objects": [button("btn-layer")]
        }],
        "triggers": [
            {
                "action": { "action": "show_layer", "layer": "l1" },
                "when": { "event": "click", "object": "btn-base" }
            },
            {
                "action": { "action": "hide_layer", "layer": "l1" },
                "when": { "event": "click", "object": "btn-layer" }
            }
        ]
    }]));
    let mut player = player(&course);
    let mut focus = FocusManager::new();
    focus.rebuild(&player, false, false, false);
    assert_eq!(
        focus.focused(),
        Some(&FocusTarget::Object("btn-base".into()))
    );

    player.handle(&Input::Click {
        object: "btn-base".into(),
    });
    focus.rebuild(&player, false, false, false);
    assert_eq!(
        focus.focused(),
        Some(&FocusTarget::Object("btn-layer".into()))
    );

    player.handle(&Input::Click {
        object: "btn-layer".into(),
    });
    focus.rebuild(&player, false, false, false);
    assert_eq!(
        focus.focused(),
        Some(&FocusTarget::Object("btn-base".into()))
    );
}

#[test]
fn sequence_controls_only_offer_moves_that_can_happen() {
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
    let mut player = player(&course);
    let list = build_list(&player, false, false, false);
    assert_eq!(
        list.len(),
        9,
        "four question controls plus transport chrome"
    );
    let first = player
        .sequence_order()
        .first()
        .cloned()
        .expect("first item");
    assert_eq!(
        list.first(),
        Some(&FocusTarget::SequenceMove {
            choice: first.clone(),
            up: false
        })
    );

    let mut focus = FocusManager::new();
    focus.rebuild(&player, false, false, false);
    // The learner pressed the control, so focus no longer tracks the first item.
    focus.auto_first = false;
    let last = player.sequence_order().len().saturating_sub(1);
    player.handle(&Input::SequenceMoved {
        choice: first.clone(),
        to: last,
    });
    focus.rebuild(&player, false, false, false);
    assert_eq!(
        focus.focused(),
        Some(&FocusTarget::SequenceMove {
            choice: first,
            up: true
        }),
        "the down move is gone, so focus keeps the same choice on its other direction"
    );
}

#[test]
fn drawer_open_traps_focus_and_escape_returns_to_outline() {
    let course = course(json!([
        { "id": "s1", "title": "One", "objects": [button("btn-one")] },
        { "id": "s2", "title": "Two", "objects": [button("btn-two")] }
    ]));
    let player = player(&course);
    let mut focus = FocusManager::new();
    focus.rebuild(&player, true, true, false);
    assert_eq!(
        focus.focused(),
        Some(&FocusTarget::Object("btn-one".into()))
    );

    focus.rebuild(&player, true, true, true);
    assert_eq!(
        focus.list(),
        &[
            FocusTarget::Chrome(ChromeButton::DrawerClose),
            FocusTarget::Chrome(ChromeButton::DrawerRow("s1".into())),
            FocusTarget::Chrome(ChromeButton::DrawerRow("s2".into())),
        ]
    );
    assert_eq!(
        focus.focused(),
        Some(&FocusTarget::Chrome(ChromeButton::DrawerClose))
    );

    focus.rebuild(&player, true, true, false);
    assert_eq!(
        focus.focused(),
        Some(&FocusTarget::Chrome(ChromeButton::Outline))
    );
}

fn update(
    ctx: &egui::Context,
    focus: &mut FocusManager,
    player: &Player<'_>,
    events: Vec<egui::Event>,
) -> Vec<Input> {
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(800.0, 450.0),
        )),
        events,
        ..Default::default()
    };
    let mut inputs = Vec::new();
    let mut drawer = false;
    let full = ctx.run_ui(raw, |ui| {
        inputs = focus.update(ui, player, true, false, &mut drawer, None);
    });
    full.drop_without_applying_deltas();
    inputs
}

fn real_key(key: egui::Key) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

fn focus_on(
    ctx: &egui::Context,
    focus: &mut FocusManager,
    player: &Player<'_>,
    target: FocusTarget,
) {
    focus.external(ExternalFocus::Focused(target));
    let _ = update(ctx, focus, player, Vec::new());
}

fn same_as_key(course: &Course, target: FocusTarget, key: egui::Key) {
    let player = player(course);
    let ctx = egui::Context::default();
    let mut real = FocusManager::new();
    focus_on(&ctx, &mut real, &player, target.clone());
    let real_inputs = update(&ctx, &mut real, &player, vec![real_key(key)]);

    let ctx = egui::Context::default();
    let mut external = FocusManager::new();
    focus_on(&ctx, &mut external, &player, target);
    external.external(ExternalFocus::Key { key, shift: false });
    let external_inputs = update(&ctx, &mut external, &player, Vec::new());

    assert_eq!(real_inputs, external_inputs, "key {key:?}");
    assert_eq!(real.focused(), external.focused());
    assert_eq!(real.keyboard_mode(), external.keyboard_mode());
}

#[test]
fn external_object_focus_and_activation_match_the_keyboard() {
    let course = course(json!([{
        "id": "s1",
        "title": "Slide",
        "objects": [button("btn-a"), button("btn-b")]
    }]));
    let player = player(&course);
    let ctx = egui::Context::default();
    let mut focus = FocusManager::new();
    let mut drawer = false;

    focus_on(
        &ctx,
        &mut focus,
        &player,
        FocusTarget::Object("btn-b".into()),
    );
    assert_eq!(focus.focused(), Some(&FocusTarget::Object("btn-b".into())));
    assert!(focus.keyboard_mode());

    focus.external(ExternalFocus::Activated(FocusTarget::Object(
        "btn-b".into(),
    )));
    let mut emitted = Vec::new();
    focus.process_external(&player, &mut drawer, &mut emitted);
    assert_eq!(
        emitted,
        vec![Input::Click {
            object: "btn-b".into()
        }]
    );
}

#[test]
fn external_choice_activation_matches_enter() {
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
    let mut focus = FocusManager::new();
    let mut drawer = false;
    focus.rebuild(&player, false, false, false);
    focus.external(ExternalFocus::Activated(FocusTarget::Choice("b".into())));
    let mut emitted = Vec::new();
    focus.process_external(&player, &mut drawer, &mut emitted);
    assert_eq!(emitted, vec![Input::SelectChoice { choice: "b".into() }]);
}

#[test]
fn external_text_and_match_emit_the_same_inputs() {
    let course = course(json!([{
        "id": "s1",
        "title": "Question",
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
    let player = player(&course);
    let mut focus = FocusManager::new();
    let mut drawer = false;
    focus.rebuild(&player, false, false, false);
    focus.external(ExternalFocus::Text("typed".into()));
    focus.external(ExternalFocus::Match {
        left: "a".into(),
        right: Some("b".into()),
    });
    let mut emitted = Vec::new();
    focus.process_external(&player, &mut drawer, &mut emitted);
    assert_eq!(
        emitted,
        vec![
            Input::TextEntered {
                text: "typed".into()
            },
            Input::MatchSelected {
                left: "a".into(),
                right: Some("b".into())
            },
        ]
    );
}

#[test]
fn external_blur_turns_keyboard_mode_off() {
    let course = course(json!([{
        "id": "s1",
        "title": "Slide",
        "objects": [button("btn-a")]
    }]));
    let player = player(&course);
    let mut focus = FocusManager::new();
    let mut drawer = false;
    focus.rebuild(&player, false, false, false);
    focus.external(ExternalFocus::Focused(FocusTarget::Object("btn-a".into())));
    let mut emitted = Vec::new();
    focus.process_external(&player, &mut drawer, &mut emitted);
    assert!(focus.keyboard_mode());
    focus.external(ExternalFocus::Blurred);
    focus.process_external(&player, &mut drawer, &mut emitted);
    assert!(!focus.keyboard_mode());
}

#[test]
fn external_enter_matches_the_keyboard_on_an_object() {
    let course = course(json!([{
        "id": "s1",
        "title": "Slide",
        "objects": [button("btn-a")]
    }]));
    same_as_key(
        &course,
        FocusTarget::Object("btn-a".into()),
        egui::Key::Enter,
    );
}

#[test]
fn external_space_matches_the_keyboard_on_a_choice() {
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
    same_as_key(&course, FocusTarget::Choice("a".into()), egui::Key::Space);
}

#[test]
fn external_arrow_matches_the_keyboard_on_a_matching_row() {
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
    same_as_key(
        &course,
        FocusTarget::MatchRow("a".into()),
        egui::Key::ArrowDown,
    );
}

#[test]
fn external_enter_matches_the_keyboard_on_a_sequence_move() {
    let course = course(json!([{
        "id": "s1",
        "title": "Sequence",
        "objects": [],
        "question": {
            "kind": "sequence",
            "prompt": "Order",
            "choices": [
                { "id": "one", "text": "One" },
                { "id": "two", "text": "Two" }
            ]
        }
    }]));
    let player = player(&course);
    let first = player.sequence_order().first().cloned().expect("first");
    same_as_key(
        &course,
        FocusTarget::SequenceMove {
            choice: first,
            up: false,
        },
        egui::Key::Enter,
    );
}

#[test]
fn external_space_starts_a_keyboard_drag() {
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
    let ctx = egui::Context::default();
    let mut focus = FocusManager::new();
    focus_on(
        &ctx,
        &mut focus,
        &player,
        FocusTarget::DragItem("item".into()),
    );
    focus.external(ExternalFocus::Key {
        key: egui::Key::Space,
        shift: false,
    });
    let _ = update(&ctx, &mut focus, &player, Vec::new());
    assert_eq!(focus.drag_visual().map(|d| d.item), Some("item".to_owned()));
}

#[test]
fn a_focused_draggable_object_is_picked_up_and_dropped_with_the_keyboard() {
    let course = course(json!([{
        "id": "s1",
        "title": "Slide",
        "objects": [
            { "id": "card", "kind": { "type": "shape", "shape": "rectangle", "fill": "#112233" },
              "frame": { "x": 100.0, "y": 100.0, "w": 80.0, "h": 60.0 },
              "drag": { "targets": ["bin", "other"] } },
            { "id": "bin", "kind": { "type": "shape", "shape": "rectangle", "fill": "#334455" },
              "frame": { "x": 400.0, "y": 300.0, "w": 100.0, "h": 100.0 } },
            { "id": "other", "kind": { "type": "shape", "shape": "rectangle", "fill": "#556677" },
              "frame": { "x": 400.0, "y": 500.0, "w": 100.0, "h": 100.0 } }
        ]
    }]));
    let player = player(&course);
    let ctx = egui::Context::default();
    let mut focus = FocusManager::new();
    focus_on(
        &ctx,
        &mut focus,
        &player,
        FocusTarget::Object("card".into()),
    );
    assert_eq!(
        focus.focused(),
        Some(&FocusTarget::Object("card".into())),
        "the draggable object is in the focus list"
    );

    let inputs = update(&ctx, &mut focus, &player, vec![real_key(egui::Key::Space)]);
    assert!(inputs.is_empty(), "picking up sends nothing yet");
    assert_eq!(focus.drag_visual().map(|d| d.item), Some("card".to_owned()));
    assert_eq!(
        focus.drag_visual().and_then(|d| d.target),
        Some("bin".to_owned()),
        "the first listed target is the initial one"
    );

    let inputs = update(&ctx, &mut focus, &player, vec![real_key(egui::Key::Space)]);
    assert_eq!(
        inputs,
        vec![Input::ObjectDropped {
            object: "card".into(),
            x: 410.0,
            y: 320.0,
        }]
    );
    assert!(focus.drag_visual().is_none());

    // Arrows cycle through the listed targets.
    let _ = update(&ctx, &mut focus, &player, vec![real_key(egui::Key::Space)]);
    let _ = update(
        &ctx,
        &mut focus,
        &player,
        vec![real_key(egui::Key::ArrowRight)],
    );
    assert_eq!(
        focus.drag_visual().and_then(|d| d.target),
        Some("other".to_owned())
    );
    let inputs = update(&ctx, &mut focus, &player, vec![real_key(egui::Key::Space)]);
    assert_eq!(
        inputs,
        vec![Input::ObjectDropped {
            object: "card".into(),
            x: 410.0,
            y: 520.0,
        }]
    );

    // Escape cancels a pick-up and sends nothing.
    let _ = update(&ctx, &mut focus, &player, vec![real_key(egui::Key::Space)]);
    assert!(focus.drag_visual().is_some());
    let inputs = update(&ctx, &mut focus, &player, vec![real_key(egui::Key::Escape)]);
    assert!(inputs.is_empty());
    assert!(focus.drag_visual().is_none());
}

#[test]
fn the_notice_button_leads_the_list_even_with_the_drawer_open() {
    let course = course(json!([
        { "id": "s1", "title": "One", "objects": [button("btn-one")] },
        { "id": "s2", "title": "Two", "objects": [button("btn-two")] }
    ]));
    let player = player(&course);
    let mut focus = FocusManager::new();
    focus.set_notice_visible(true);

    focus.rebuild(&player, true, true, false);
    assert_eq!(
        focus.list().first(),
        Some(&FocusTarget::Chrome(ChromeButton::DismissNotice)),
        "the dismiss button is first while a notice is showing"
    );

    focus.rebuild(&player, true, true, true);
    assert_eq!(
        focus.list(),
        &[
            FocusTarget::Chrome(ChromeButton::DismissNotice),
            FocusTarget::Chrome(ChromeButton::DrawerClose),
            FocusTarget::Chrome(ChromeButton::DrawerRow("s1".into())),
            FocusTarget::Chrome(ChromeButton::DrawerRow("s2".into())),
        ]
    );

    focus.set_notice_visible(false);
    focus.rebuild(&player, true, true, false);
    assert!(
        !focus
            .list()
            .contains(&FocusTarget::Chrome(ChromeButton::DismissNotice))
    );
}

#[test]
fn enter_on_the_notice_button_dismisses_it() {
    let course = course(json!([{ "id": "s1", "title": "One", "objects": [button("btn-one")] }]));
    let player = player(&course);
    let ctx = egui::Context::default();
    let mut focus = FocusManager::new();
    focus.set_notice_visible(true);
    focus_on(
        &ctx,
        &mut focus,
        &player,
        FocusTarget::Chrome(ChromeButton::DismissNotice),
    );

    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(800.0, 450.0),
        )),
        events: vec![real_key(egui::Key::Enter)],
        ..Default::default()
    };
    let mut dismissed = false;
    let full = ctx.run_ui(raw, |ui| {
        let mut drawer = false;
        let mut captions_on = false;
        let inputs = focus.update_with_captions(
            ui,
            &player,
            super::ChromeOptions {
                show_nav: true,
                show_menu: true,
            },
            &mut drawer,
            None,
            &mut captions_on,
            &mut dismissed,
        );
        assert!(inputs.is_empty(), "dismissing sends no runtime input");
    });
    full.drop_without_applying_deltas();
    assert!(
        dismissed,
        "Enter on the dismiss button dismisses the notice"
    );
}

fn input_course(kind: Value, variable: Value) -> Course {
    Course::from_json(
        &json!({
            "schema_version": 1,
            "id": "input-focus",
            "title": "Input Focus",
            "variables": [variable],
            "scenes": [{ "id": "sc", "title": "Scene", "slides": [{
                "id": "s1",
                "title": "Slide",
                "objects": [{
                    "id": "input",
                    "kind": kind,
                    "frame": { "x": 0.0, "y": 0.0, "w": 200.0, "h": 40.0 }
                }]
            }] }]
        })
        .to_string(),
    )
    .expect("valid input course")
}

fn shift_key(key: egui::Key) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::SHIFT,
    }
}

#[test]
fn marker_enter_toggles_and_escape_closes() {
    let course = course(json!([{
        "id": "s1",
        "title": "Slide",
        "objects": [{
            "id": "marker",
            "kind": { "type": "marker", "title": "Info" },
            "frame": { "x": 0.0, "y": 0.0, "w": 40.0, "h": 40.0 }
        }]
    }]));
    let mut player = player(&course);
    let ctx = egui::Context::default();
    let mut focus = FocusManager::new();
    focus_on(
        &ctx,
        &mut focus,
        &player,
        FocusTarget::Object("marker".into()),
    );
    let inputs = update(&ctx, &mut focus, &player, vec![real_key(egui::Key::Enter)]);
    assert_eq!(
        inputs,
        vec![Input::MarkerToggled {
            object: "marker".into()
        }]
    );
    for input in inputs {
        player.handle(&input);
    }
    assert_eq!(player.marker_open(), Some("marker"));
    assert_eq!(
        update(&ctx, &mut focus, &player, vec![real_key(egui::Key::Escape)]),
        vec![Input::MarkersClosed]
    );
}

#[test]
fn focusing_a_partly_hidden_child_scrolls_its_panel() {
    let course = course(json!([{
        "id": "s1",
        "title": "Slide",
        "objects": [
            { "id": "panel",
              "kind": { "type": "scroll_panel", "content_height": 600.0 },
              "frame": { "x": 100.0, "y": 100.0, "w": 300.0, "h": 200.0 } },
            { "id": "child", "panel": "panel",
              "kind": { "type": "button", "label": "Child" },
              "frame": { "x": 120.0, "y": 280.0, "w": 100.0, "h": 50.0 } }
        ]
    }]));
    let mut player = player(&course);
    let ctx = egui::Context::default();
    let mut focus = FocusManager::new();
    focus_on(
        &ctx,
        &mut focus,
        &player,
        FocusTarget::Object("child".into()),
    );
    let inputs = update(&ctx, &mut focus, &player, Vec::new());
    let scroll = inputs
        .iter()
        .find_map(|input| match input {
            Input::PanelScrolled { object, offset } if object == "panel" => Some(*offset),
            _ => None,
        })
        .expect("the panel scrolls the focused child into view");
    assert!((scroll - 30.0).abs() < 1e-4, "{scroll}");
    for input in inputs {
        player.handle(&input);
    }
    assert!((player.panel_offset("panel") - 30.0).abs() < 1e-4);
}

#[test]
fn slider_arrow_keys_step_and_home_end_jump() {
    let course = input_course(
        json!({ "type": "slider", "variable": "n", "min": 0, "max": 10, "step": 2 }),
        json!({ "name": "n", "value": 0 }),
    );
    let mut player = player(&course);
    let ctx = egui::Context::default();
    let mut focus = FocusManager::new();
    focus_on(
        &ctx,
        &mut focus,
        &player,
        FocusTarget::Object("input".into()),
    );
    let step = update(
        &ctx,
        &mut focus,
        &player,
        vec![real_key(egui::Key::ArrowRight)],
    );
    assert_eq!(
        step,
        vec![Input::SetInputValue {
            object: "input".into(),
            value: coursecraft_course::Value::Number(2.0),
        }]
    );
    for input in step {
        player.handle(&input);
    }
    let ten = update(
        &ctx,
        &mut focus,
        &player,
        vec![shift_key(egui::Key::ArrowRight)],
    );
    assert_eq!(
        ten,
        vec![Input::SetInputValue {
            object: "input".into(),
            value: coursecraft_course::Value::Number(10.0),
        }]
    );
    for input in ten {
        player.handle(&input);
    }
    let home = update(&ctx, &mut focus, &player, vec![real_key(egui::Key::Home)]);
    assert_eq!(
        home,
        vec![Input::SetInputValue {
            object: "input".into(),
            value: coursecraft_course::Value::Number(0.0),
        }]
    );
    for input in home {
        player.handle(&input);
    }
    assert_eq!(
        update(&ctx, &mut focus, &player, vec![real_key(egui::Key::End)]),
        vec![Input::SetInputValue {
            object: "input".into(),
            value: coursecraft_course::Value::Number(10.0),
        }]
    );
}

#[test]
fn checkbox_space_toggles_and_radio_arrows_move_the_selection() {
    let course = input_course(
        json!({ "type": "checkbox", "variable": "on", "label": "On" }),
        json!({ "name": "on", "value": false }),
    );
    let checkbox_player = player(&course);
    let ctx = egui::Context::default();
    let mut focus = FocusManager::new();
    focus_on(
        &ctx,
        &mut focus,
        &checkbox_player,
        FocusTarget::Object("input".into()),
    );
    assert_eq!(
        update(
            &ctx,
            &mut focus,
            &checkbox_player,
            vec![real_key(egui::Key::Space)]
        ),
        vec![Input::SetInputValue {
            object: "input".into(),
            value: coursecraft_course::Value::Bool(true),
        }]
    );

    let course = input_course(
        json!({ "type": "radio_group", "variable": "choice", "options": [
            { "label": "A", "value": "a" },
            { "label": "B", "value": "b" }
        ] }),
        json!({ "name": "choice", "value": "a" }),
    );
    let mut radio_player = player(&course);
    let ctx = egui::Context::default();
    let mut focus = FocusManager::new();
    focus_on(
        &ctx,
        &mut focus,
        &radio_player,
        FocusTarget::Object("input".into()),
    );
    let down = update(
        &ctx,
        &mut focus,
        &radio_player,
        vec![real_key(egui::Key::ArrowDown)],
    );
    assert_eq!(
        down,
        vec![Input::SetInputValue {
            object: "input".into(),
            value: coursecraft_course::Value::Text("b".into()),
        }]
    );
    for input in down {
        radio_player.handle(&input);
    }
    assert_eq!(
        update(
            &ctx,
            &mut focus,
            &radio_player,
            vec![real_key(egui::Key::ArrowRight)],
        ),
        vec![Input::SetInputValue {
            object: "input".into(),
            value: coursecraft_course::Value::Text("a".into()),
        }]
    );
}

#[test]
fn enter_in_an_essay_reaches_the_text_box_and_keeps_focus() {
    let course = course(json!([{
        "id": "s1",
        "title": "Essay",
        "objects": [],
        "question": {
            "kind": "essay",
            "prompt": "Tell us more",
            "choices": []
        }
    }]));
    let player = player(&course);
    let ctx = egui::Context::default();
    let mut focus = FocusManager::new();
    focus_on(&ctx, &mut focus, &player, FocusTarget::FillIn);

    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(800.0, 450.0),
        )),
        events: vec![real_key(egui::Key::Enter)],
        ..Default::default()
    };
    let mut drawer = false;
    let mut enter_left = false;
    let full = ctx.run_ui(raw, |ui| {
        let inputs = focus.update(ui, &player, true, false, &mut drawer, None);
        assert!(inputs.is_empty(), "Enter must not submit: {inputs:?}");
        enter_left = ui.input(|i| i.key_pressed(egui::Key::Enter));
    });
    full.drop_without_applying_deltas();
    assert!(
        enter_left,
        "Enter was swallowed before the essay box saw it"
    );
    assert_eq!(focus.focused(), Some(&FocusTarget::FillIn));
}

#[test]
fn enter_on_a_text_entry_stays_with_whichever_field_holds_the_text() {
    let course = input_course(
        json!({ "type": "text_entry", "variable": "t" }),
        json!({ "name": "t", "value": "old" }),
    );
    let player = player(&course);
    let ctx = egui::Context::default();
    let mut focus = FocusManager::new();
    let target = FocusTarget::Object("input".into());
    focus_on(&ctx, &mut focus, &player, target);

    // The live text box owns the text: Enter must still reach it.
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(800.0, 450.0),
        )),
        events: vec![real_key(egui::Key::Enter)],
        ..Default::default()
    };
    let mut drawer = false;
    let mut enter_left = false;
    let full = ctx.run_ui(raw, |ui| {
        let inputs = focus.update(ui, &player, true, false, &mut drawer, None);
        assert!(inputs.is_empty(), "{inputs:?}");
        enter_left = ui.input(|i| i.key_pressed(egui::Key::Enter));
    });
    full.drop_without_applying_deltas();
    assert!(enter_left, "Enter was swallowed before the text box saw it");

    // The screen-reader field filled the buffer: Enter commits it.
    focus.external(ExternalFocus::Text("new".into()));
    let _ = update(&ctx, &mut focus, &player, Vec::new());
    let inputs = update(&ctx, &mut focus, &player, vec![real_key(egui::Key::Enter)]);
    assert_eq!(
        inputs,
        vec![Input::SetInputValue {
            object: "input".into(),
            value: coursecraft_course::Value::Text("new".into()),
        }]
    );
}

fn lightbox_course() -> Course {
    course(json!([{
        "id": "base",
        "title": "Base",
        "objects": [
            { "id": "open", "kind": { "type": "button", "label": "Open" }, "frame": { "x": 0.0, "y": 0.0, "w": 100.0, "h": 40.0 } }
        ],
        "triggers": [{
            "action": { "action": "lightbox_slide", "slide": "box" },
            "when": { "event": "click", "object": "open" }
        }]
    }, {
        "id": "box",
        "title": "Box",
        "objects": [
            { "id": "box-btn", "kind": { "type": "button", "label": "Box" }, "frame": { "x": 0.0, "y": 0.0, "w": 100.0, "h": 40.0 } }
        ]
    }]))
}

#[test]
fn lightbox_traps_focus_and_returns_it_to_the_opener() {
    let course = lightbox_course();
    let mut player = player(&course);
    let mut manager = FocusManager::new();
    manager.rebuild(&player, false, false, false);
    manager.focused = Some(FocusTarget::Object("open".into()));

    player.handle(&Input::Click {
        object: "open".into(),
    });
    manager.rebuild(&player, false, false, false);
    assert!(
        manager
            .list()
            .contains(&FocusTarget::Chrome(ChromeButton::CloseLightbox)),
        "close button is inside the trap"
    );
    assert!(
        !manager
            .list()
            .iter()
            .any(|target| matches!(target, FocusTarget::Chrome(other) if other != &ChromeButton::CloseLightbox)),
        "no chrome outside the box: {:?}",
        manager.list()
    );
    assert_eq!(
        manager.focused(),
        Some(&FocusTarget::Object("box-btn".into()))
    );

    player.handle(&Input::CloseLightbox);
    manager.rebuild(&player, false, false, false);
    assert_eq!(
        manager.focused(),
        Some(&FocusTarget::Object("open".into())),
        "focus returns to the object that opened the box"
    );
}

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

#[test]
fn draw_outline_and_navigation_use_the_drawn_play_order() {
    let bank_slides: Vec<Value> = (1..=5).map(|i| question_slide(&format!("bq{i}"))).collect();
    let text = json!({
        "schema_version": 1,
        "id": "draw-focus",
        "title": "Draw Focus",
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
    let ids: Vec<String> = player.play_order().iter().map(|s| s.id.clone()).collect();
    assert_eq!(ids.len(), 4, "intro + 2 drawn + end: {ids:?}");
    assert_eq!(ids[0], "s-intro");
    assert_eq!(ids[3], "s-end");
    assert!(ids.iter().all(|id| id != "s-draw"));

    // The outline drawer lists the drawn questions, not the draw slide.
    let list = build_list(&player, true, true, true);
    let rows: Vec<String> = list
        .iter()
        .filter_map(|target| match target {
            FocusTarget::Chrome(ChromeButton::DrawerRow(id)) => Some(id.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(rows, ids);

    // Next goes through the drawn questions and stops at the drawn total.
    player.handle(&Input::Next);
    assert_eq!(player.current_slide().unwrap().id, ids[1]);
    let list = build_list(&player, true, false, false);
    assert!(
        list.iter()
            .any(|target| matches!(target, FocusTarget::Chrome(ChromeButton::Next))),
        "next is available on the first drawn question: {list:?}"
    );
    player.handle(&Input::Next);
    assert_eq!(player.current_slide().unwrap().id, ids[2]);
    player.handle(&Input::Next);
    assert_eq!(player.current_slide().unwrap().id, "s-end");
    let list = build_list(&player, true, false, false);
    assert!(
        !list
            .iter()
            .any(|target| matches!(target, FocusTarget::Chrome(ChromeButton::Next))),
        "next is gone on the last slide: {list:?}"
    );
}

#[test]
fn m31_11_clamp_point_handles_inverted_rect_and_nan() {
    let _ = super::clamp_point(egui::Rect::NOTHING, 10.0, 20.0);
    let inverted = egui::Rect::from_min_max(egui::pos2(100.0, 100.0), egui::pos2(10.0, 20.0));
    let (x, y) = super::clamp_point(inverted, 50.0, 50.0);
    assert!(x.is_finite() && y.is_finite());
    let _ = super::clamp_point(egui::Rect::NOTHING, f32::NAN, f32::INFINITY);
}

#[test]
fn gb_08_interactive_group_tab_activate_and_union_focus_ring() {
    let course = course(json!([{
        "id": "s1",
        "title": "Grouped Button Focus",
        "groups": [{
            "id": "g-btn",
            "name": "Next Card",
            "interactive": true
        }],
        "objects": [
            {
                "id": "btn-before",
                "kind": { "type": "button", "label": "Before" },
                "frame": { "x": 20.0, "y": 20.0, "w": 80.0, "h": 40.0 }
            },
            {
                "id": "m-bg",
                "group": "g-btn",
                "kind": { "type": "shape", "shape": "rectangle", "fill": "#224488" },
                "frame": { "x": 100.0, "y": 100.0, "w": 120.0, "h": 60.0 },
                "states": [{
                    "name": "hover",
                    "fill": "#11AA22",
                    "extras": [{
                        "id": "extra-badge",
                        "frame": { "x": -50.0, "y": -50.0, "w": 300.0, "h": 300.0 },
                        "kind": { "type": "shape", "shape": "rectangle", "fill": "#FF0000" }
                    }]
                }]
            },
            {
                "id": "m-icon",
                "group": "g-btn",
                "kind": { "type": "shape", "shape": "ellipse", "fill": "#FFFFFF" },
                "frame": { "x": 240.0, "y": 110.0, "w": 40.0, "h": 40.0 }
            },
            {
                "id": "m-label",
                "group": "g-btn",
                "kind": { "type": "text", "text": "Continue" },
                "frame": { "x": 120.0, "y": 170.0, "w": 140.0, "h": 30.0 }
            },
            {
                "id": "btn-after",
                "kind": { "type": "button", "label": "After" },
                "frame": { "x": 400.0, "y": 20.0, "w": 80.0, "h": 40.0 }
            }
        ]
    }]));
    let player = player(&course);

    // Focus list visits g-btn ONCE and never visits individual members.
    let list = build_list(&player, false, false, false);
    assert_eq!(
        list,
        with_transport(objects(&["btn-before", "g-btn", "btn-after"]))
    );
    for member in ["m-bg", "m-icon", "m-label"] {
        assert!(!list.contains(&FocusTarget::Object(member.to_owned())));
    }

    let ctx = egui::Context::default();
    let mut focus = FocusManager::new();

    // First Tab reveals the ring on btn-before; second Tab moves to g-btn.
    let _ = update(&ctx, &mut focus, &player, vec![real_key(egui::Key::Tab)]);
    assert_eq!(
        focus.focused(),
        Some(&FocusTarget::Object("btn-before".into()))
    );

    let hover_inputs = update(&ctx, &mut focus, &player, vec![real_key(egui::Key::Tab)]);
    assert_eq!(focus.focused(), Some(&FocusTarget::Object("g-btn".into())));
    assert_eq!(focus.hovered_object(), Some("g-btn"));
    assert!(
        hover_inputs.contains(&Input::HoverStart {
            object: "g-btn".into()
        }),
        "tabbing onto interactive group must start keyboard hover: {hover_inputs:?}"
    );

    // Enter emits Click { object: "g-btn" } once.
    let enter_inputs = update(&ctx, &mut focus, &player, vec![real_key(egui::Key::Enter)]);
    assert_eq!(
        enter_inputs,
        vec![Input::Click {
            object: "g-btn".into()
        }]
    );

    // Space emits Click { object: "g-btn" } once.
    let space_inputs = update(&ctx, &mut focus, &player, vec![real_key(egui::Key::Space)]);
    assert_eq!(
        space_inputs,
        vec![Input::Click {
            object: "g-btn".into()
        }]
    );

    // Painting with keyboard hover active (which includes the oversized state extra on m-bg)
    // must draw the ring over the union of all PRIMARY members only: (100, 100) to (280, 200).
    let items = crate::compose::compose_with_pointer(&player, Some("g-btn"), None);
    let screen = egui::Rect::from_min_size(
        egui::Pos2::ZERO,
        egui::vec2(course.slide_size.w as f32, course.slide_size.h as f32),
    );
    let transform = crate::layout::SlideTransform::fit(screen, course.slide_size).expect("fit");
    let raw = egui::RawInput {
        screen_rect: Some(screen),
        ..Default::default()
    };
    let full = ctx.run_ui(raw, |ui| {
        focus.paint(ui, &player, &items, Some(&transform));
    });
    full.drop_without_applying_deltas();

    let ring = focus.ring_rect().expect("union focus ring painted");
    let expected = transform.to_screen_rect(egui::Rect::from_min_max(
        egui::pos2(100.0, 100.0),
        egui::pos2(280.0, 200.0),
    ));
    assert!(
        (ring.min.x - expected.min.x).abs() < 0.5
            && (ring.min.y - expected.min.y).abs() < 0.5
            && (ring.max.x - expected.max.x).abs() < 0.5
            && (ring.max.y - expected.max.y).abs() < 0.5,
        "expected union ring {expected:?}, got {ring:?}"
    );
    assert!(
        ring.width() > 140.0,
        "union ring must be wider than any single member"
    );
    assert!(
        ring.height() > 60.0,
        "union ring must be taller than any single member"
    );

    // Next Tab moves straight to btn-after (never stops on members).
    let _ = update(&ctx, &mut focus, &player, vec![real_key(egui::Key::Tab)]);
    assert_eq!(
        focus.focused(),
        Some(&FocusTarget::Object("btn-after".into()))
    );
}

#[test]
fn focus_order_candidates_match_the_player_focus_list() {
    let mut late = button("late");
    late["focus_order"] = json!(1);
    let mut early = button("early");
    early["focus_order"] = json!(1);
    let mut decorative = button("deco");
    decorative["decorative"] = json!(true);
    let c = course(json!([{
        "id": "s1",
        "title": "S",
        "objects": [button("a"), late, button("b"), decorative, early]
    }]));
    let slide = c.slide("s1").expect("slide");
    let got: Vec<String> = focus_order_candidates(slide)
        .into_iter()
        .map(|candidate| candidate.id)
        .collect();
    assert_eq!(got, ["late", "early", "a", "b"]);
    let list = build_list(&player(&c), false, false, false);
    let from_player: Vec<String> = list
        .into_iter()
        .filter_map(|target| match target {
            FocusTarget::Object(id) => Some(id),
            _ => None,
        })
        .collect();
    assert_eq!(got, from_player);
}

#[test]
fn focus_order_candidates_list_layers_after_the_base_and_skip_plain_shapes() {
    let c = course(json!([{
        "id": "s1",
        "title": "S",
        "objects": [
            button("base"),
            {
                "id": "plain",
                "kind": { "type": "shape", "shape": "rectangle", "fill": "#000000" },
                "frame": { "x": 0.0, "y": 0.0, "w": 10.0, "h": 10.0 }
            }
        ],
        "layers": [{ "id": "l1", "name": "L", "objects": [button("inlayer")] }]
    }]));
    let slide = c.slide("s1").expect("slide");
    let got = focus_order_candidates(slide);
    assert_eq!(got.len(), 2);
    assert_eq!(got[0].id, "base");
    assert_eq!(got[0].layer, None);
    assert_eq!(got[1].id, "inlayer");
    assert_eq!(got[1].layer.as_deref(), Some("l1"));
}

// ---- Neon accessibility verification (N117, N118) ----

fn slide_with_one_text(runs: Value) -> Course {
    course(json!([{
        "id": "s1",
        "title": "S",
        "objects": [{
            "id": "txt",
            "kind": { "type": "text", "text": "x", "runs": runs },
            "frame": { "x": 10.0, "y": 10.0, "w": 120.0, "h": 300.0 }
        }]
    }]))
}

#[test]
fn n117_focus_ring_wraps_the_parent_not_its_state_badge() {
    let c = course(json!([{
        "id": "s1",
        "title": "S",
        "objects": [{
            "id": "btn",
            "initial_state": "done",
            "kind": { "type": "button", "label": "Go" },
            "frame": { "x": 100.0, "y": 200.0, "w": 160.0, "h": 48.0 },
            "states": [{ "name": "done", "extras": [{
                "id": "badge",
                "frame": { "x": 144.0, "y": -8.0, "w": 24.0, "h": 24.0 },
                "kind": { "type": "shape", "shape": "ellipse", "fill": "#22C55E" }
            }] }]
        }]
    }]));
    let player = player(&c);
    let items = crate::compose::compose(&player);
    assert!(items.iter().any(|i| i.is_state_extra), "badge is composed");
    let rect =
        super::target_rect(&player, &FocusTarget::Object("btn".into()), &items).expect("ring rect");
    assert!(
        rect.width() > 100.0,
        "ring must wrap the button, not the 24px badge: {rect:?}"
    );
}

#[test]
fn n117_second_link_ring_skips_the_lines_of_a_wrapped_first_link() {
    let long = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi ";
    let c = slide_with_one_text(json!([
        { "text": long, "link": { "type": "url", "url": "https://a.invalid" } },
        { "text": "second", "link": { "type": "url", "url": "https://b.invalid" } }
    ]));
    let player = player(&c);
    let ctx = egui::Context::default();
    crate::install_fonts(&ctx);
    let mut items = crate::compose::compose(&player);
    let full = ctx.run_ui(egui::RawInput::default(), |_ui| {
        crate::text_layout::fill_item_links(&ctx, &mut items);
    });
    full.drop_without_applying_deltas();
    let link = |index: usize| FocusTarget::Link {
        object: "txt".into(),
        index,
        link: coursecraft_course::Link::Url { url: String::new() },
    };
    let first = super::target_rect(&player, &link(0), &items).expect("first link");
    let second = super::target_rect(&player, &link(1), &items).expect("second link");
    assert!(
        first.height() > second.height() * 1.5,
        "link 0 wraps over several lines, link 1 is one line: {first:?} {second:?}"
    );
    assert!(second.min.y >= first.min.y, "{first:?} {second:?}");
}

fn press(
    ctx: &egui::Context,
    focus: &mut FocusManager,
    player: &Player<'_>,
    key: egui::Key,
) -> Vec<Input> {
    update(ctx, focus, player, vec![real_key(key)])
}

#[test]
fn n118_space_reaches_key_triggers_when_nothing_is_focusable() {
    let c = course(json!([{ "id": "s1", "title": "S", "objects": [] }]));
    let player = player(&c);
    let ctx = egui::Context::default();
    let mut focus = FocusManager::new();
    focus.set_chrome_visibility(super::ChromeVisibility::none());
    let first = press(&ctx, &mut focus, &player, egui::Key::Space);
    assert!(
        first.contains(&Input::KeyPressed {
            key: "space".into()
        }),
        "first Space was swallowed: {first:?}"
    );
    let second = press(&ctx, &mut focus, &player, egui::Key::Space);
    assert!(
        second.contains(&Input::KeyPressed {
            key: "space".into()
        }),
        "second Space was swallowed: {second:?}"
    );
}

#[test]
fn n118_slide_key_triggers_do_not_fire_behind_the_open_drawer() {
    let c = course(json!([{ "id": "s1", "title": "S", "objects": [button("b")] }]));
    let player = player(&c);
    let ctx = egui::Context::default();
    let mut focus = FocusManager::new();
    let mut drawer = true;
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(800.0, 450.0),
        )),
        events: vec![real_key(egui::Key::A)],
        ..Default::default()
    };
    let mut inputs = Vec::new();
    let full = ctx.run_ui(raw, |ui| {
        inputs = focus.update(ui, &player, true, true, &mut drawer, None);
    });
    full.drop_without_applying_deltas();
    assert!(
        !inputs.contains(&Input::KeyPressed { key: "a".into() }),
        "key trigger fired behind the modal drawer: {inputs:?}"
    );
}
