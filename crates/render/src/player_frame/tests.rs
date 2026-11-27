use super::{FrameOutput, PlayerFrame};
use crate::focus::{ChromeButton, FocusTarget};
use crate::view::SlideView;
use coursecraft_course::Course;
use coursecraft_logic::{Input, Player};
use egui::{Event, Pos2, Rect, Vec2, pos2, vec2};

const HELLO_JSON: &str = include_str!("../../../../examples/hello-course/course.json");

struct Harness {
    ctx: egui::Context,
    frame: PlayerFrame,
    view: SlideView,
    size: Vec2,
    time: f64,
    last_shapes: Vec<egui::epaint::ClippedShape>,
}

impl Harness {
    fn new(width: f32, height: f32) -> Self {
        let ctx = egui::Context::default();
        crate::install_fonts(&ctx);
        Self {
            ctx,
            frame: PlayerFrame::new(),
            view: SlideView::new(),
            size: vec2(width, height),
            time: 0.0,
            last_shapes: Vec::new(),
        }
    }

    fn run(&mut self, player: &Player<'_>, events: Vec<Event>) -> FrameOutput {
        self.time += 0.1;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), self.size)),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        let mut output = FrameOutput::default();
        let frame = &mut self.frame;
        let view = &mut self.view;
        let full = self.ctx.run_ui(raw, |ui| {
            output = frame.show(ui, player, view, None);
        });
        self.last_shapes = full.shapes.clone();
        full.drop_without_applying_deltas();
        output
    }
}

fn key(key: egui::Key) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::default(),
    }
}

fn click(pos: Pos2) -> Vec<Event> {
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

fn hello() -> (Course, Player<'static>) {
    let course = Course::from_json(HELLO_JSON).expect("valid course");
    let boxed: &'static Course = Box::leak(Box::new(course.clone()));
    let mut player = Player::new(boxed).expect("valid player");
    player.handle(&Input::Start);
    (course, player)
}

#[test]
fn frame_paints_slide_and_strip_without_input() {
    let (_course, player) = hello();
    let mut harness = Harness::new(1280.0, 720.0);
    let output = harness.run(&player, Vec::new());
    assert!(output.inputs.is_empty());
    assert!(output.slide_rect.is_positive());
    assert!(harness.frame.strip_rect().is_some());
}

#[test]
fn hostile_size_is_ignored() {
    let (_course, player) = hello();
    let mut harness = Harness::new(0.0, 0.0);
    let output = harness.run(&player, Vec::new());
    assert!(output.inputs.is_empty());
    assert_eq!(output.slide_rect, Rect::NOTHING);
    assert!(harness.frame.strip_rect().is_none());
}

#[test]
fn navigation_is_ignored_before_start() {
    let course = Course::from_json(HELLO_JSON).expect("valid course");
    let player = Player::new(&course).expect("valid player");
    assert!(!player.state().started);

    let mut harness = Harness::new(1280.0, 720.0);
    let output = harness.run(&player, Vec::new());
    assert!(output.inputs.is_empty());
    let strip = harness.frame.strip_rect().expect("strip layout");
    let next = pos2(strip.right() - 26.0, strip.center().y);
    let output = harness.run(&player, click(next));
    assert!(
        output.inputs.is_empty(),
        "next must be disabled before start"
    );
    let output = harness.run(&player, vec![key(egui::Key::ArrowRight)]);
    assert!(output.inputs.is_empty(), "arrow nav must wait for start");
}

#[test]
fn arrow_keys_fire_key_triggers_when_navigation_and_focus_do_not_use_them() {
    let mut course = Course::from_json(HELLO_JSON).expect("valid course");
    let slide = course.slide_mut("s-intro").expect("intro");
    for key in ["arrow_up", "escape"] {
        slide.triggers.push(coursecraft_course::Trigger {
            action: coursecraft_course::Action::SetVariable {
                variable: "tabs_seen".into(),
                op: coursecraft_course::VarOp::Assign,
                value: Some(coursecraft_course::Value::Number(9.0).into()),
            },
            when: coursecraft_course::Event::KeyPressed {
                key: key.into(),
                object: None,
            },
            condition: None,
            disabled: false,
        });
    }
    let boxed: &'static Course = Box::leak(Box::new(course));
    let mut player = Player::new(boxed).expect("valid player");
    player.handle(&Input::Start);
    let mut harness = Harness::new(1280.0, 720.0);
    harness.run(&player, Vec::new());
    let output = harness.run(&player, vec![key(egui::Key::ArrowUp)]);
    assert_eq!(
        output.inputs,
        vec![Input::KeyPressed {
            key: "arrow_up".into()
        }]
    );
    let output = harness.run(&player, vec![key(egui::Key::Escape)]);
    assert_eq!(
        output.inputs,
        vec![Input::KeyPressed {
            key: "escape".into()
        }]
    );
}

#[test]
fn arrow_keys_emit_previous_and_next_when_allowed() {
    let (course, mut player) = hello();
    let mut harness = Harness::new(1280.0, 720.0);
    assert!(harness.run(&player, Vec::new()).inputs.is_empty());

    let output = harness.run(&player, vec![key(egui::Key::ArrowLeft)]);
    assert!(
        !output.inputs.contains(&Input::Previous),
        "previous is disabled on the first slide"
    );

    let output = harness.run(&player, vec![key(egui::Key::ArrowRight)]);
    assert_eq!(output.inputs, vec![Input::Next]);

    let total = course.slides().count();
    for _ in 1..total {
        player.handle(&Input::Next);
    }
    let output = harness.run(&player, vec![key(egui::Key::ArrowRight)]);
    assert!(
        !output.inputs.contains(&Input::Next),
        "next is disabled on the last slide"
    );
    let output = harness.run(&player, vec![key(egui::Key::ArrowLeft)]);
    assert_eq!(output.inputs, vec![Input::Previous]);
}

#[test]
fn strip_buttons_emit_navigation_inputs() {
    let (course, mut player) = hello();
    let mut harness = Harness::new(1280.0, 720.0);
    harness.run(&player, Vec::new());
    let next = harness
        .frame
        .chrome_rect(&ChromeButton::Next)
        .expect("next button")
        .center();

    let output = harness.run(&player, click(next));
    assert_eq!(output.inputs, vec![Input::Next]);

    let total = course.slides().count();
    for _ in 1..total {
        player.handle(&Input::Next);
    }
    harness.run(&player, Vec::new());
    let previous = harness
        .frame
        .chrome_rect(&ChromeButton::Previous)
        .expect("previous button")
        .center();
    let output = harness.run(&player, click(previous));
    assert_eq!(output.inputs, vec![Input::Previous]);
}

#[test]
fn outline_opens_with_m_and_closes_with_escape() {
    let (_course, player) = hello();
    let mut harness = Harness::new(1280.0, 720.0);
    harness.run(&player, Vec::new());
    assert!(!harness.frame.drawer_open());

    let output = harness.run(&player, vec![key(egui::Key::M)]);
    assert!(output.inputs.is_empty());
    assert!(harness.frame.drawer_open());
    harness.run(&player, Vec::new());
    assert!(harness.frame.drawer_rect().is_some());

    let output = harness.run(&player, vec![key(egui::Key::Escape)]);
    assert!(output.inputs.is_empty());
    assert!(!harness.frame.drawer_open());
}

#[test]
fn disabled_player_flags_ignore_keyboard_and_buttons() {
    let mut course = Course::from_json(HELLO_JSON).expect("valid course");
    course.player.show_menu = false;
    course.player.show_nav = false;
    let boxed: &'static Course = Box::leak(Box::new(course));
    let mut player = Player::new(boxed).expect("valid player");
    player.handle(&Input::Start);

    let mut harness = Harness::new(1280.0, 720.0);
    harness.run(&player, Vec::new());
    let output = harness.run(&player, vec![key(egui::Key::M)]);
    // With the menu off the key is free for a slide's `key_pressed` trigger.
    assert!(
        output
            .inputs
            .iter()
            .all(|input| matches!(input, Input::KeyPressed { .. })),
        "{:?}",
        output.inputs
    );
    assert!(!harness.frame.drawer_open());
    let output = harness.run(&player, vec![key(egui::Key::ArrowRight)]);
    assert!(
        !output
            .inputs
            .iter()
            .any(|input| matches!(input, Input::Next | Input::Previous)),
        "navigation is off, but key triggers may still hear the arrow"
    );
}

#[test]
fn idle_strip_hides_and_reappears_on_activity() {
    let (_course, player) = hello();
    let mut harness = Harness::new(1280.0, 720.0);
    harness.frame.overlay_strip = true;
    harness.run(&player, Vec::new());

    harness.frame.hide_strip();
    let output = harness.run(&player, Vec::new());
    assert!(output.inputs.is_empty());
    assert!(
        harness.frame.strip_rect().is_none(),
        "an idle strip must not be painted or cover the slide"
    );

    let output = harness.run(&player, vec![Event::PointerMoved(pos2(400.0, 300.0))]);
    assert!(output.inputs.is_empty());
    assert!(
        harness.frame.strip_rect().is_some(),
        "pointer movement brings the strip back"
    );
}

#[test]
fn reduced_motion_keeps_the_strip_visible() {
    let (_course, player) = hello();
    let mut harness = Harness::new(1280.0, 720.0);
    harness.frame.overlay_strip = true;
    harness.frame.reduced_motion = true;
    harness.frame.hide_strip();
    harness.run(&player, Vec::new());
    assert!(
        harness.frame.strip_rect().is_some(),
        "reduced motion must always show the strip"
    );
}

#[test]
fn clicking_the_scrim_closes_the_drawer_and_sends_no_input() {
    let (_course, player) = hello();
    let mut harness = Harness::new(1280.0, 720.0);
    harness.run(&player, Vec::new());
    harness.run(&player, vec![key(egui::Key::M)]);
    harness.run(&player, Vec::new());
    harness.run(&player, Vec::new());
    assert!(harness.frame.drawer_open());
    let drawer = harness.frame.drawer_rect().expect("drawer is painted");
    let scrim_point = pos2(drawer.right() + 200.0, 300.0);

    let output = harness.run(&player, click(scrim_point));
    assert!(output.inputs.is_empty(), "the scrim must not send input");
    assert!(!harness.frame.drawer_open(), "the scrim click closes it");
}

#[test]
fn drawer_close_button_closes_without_input() {
    let (_course, player) = hello();
    let mut harness = Harness::new(1280.0, 720.0);
    harness.run(&player, Vec::new());
    harness.run(&player, vec![key(egui::Key::M)]);
    harness.run(&player, Vec::new());
    harness.run(&player, Vec::new());
    let drawer = harness.frame.drawer_rect().expect("drawer is painted");
    let close = pos2(drawer.right() - 16.0 - 16.0, drawer.top() + 16.0 + 16.0);
    let output = harness.run(&player, click(close));
    assert!(output.inputs.is_empty());
    assert!(!harness.frame.drawer_open());
}

#[test]
fn ended_state_hides_the_strip_and_ignores_strip_input() {
    let (_course, player) = hello();
    let mut harness = Harness::new(1280.0, 720.0);
    harness.run(&player, Vec::new());
    assert!(harness.frame.strip_rect().is_some());
    harness.frame.ended = true;
    let output = harness.run(&player, Vec::new());
    assert!(output.inputs.is_empty());
    assert!(
        harness.frame.strip_rect().is_none(),
        "the ended state hides the control strip"
    );
}

#[test]
fn ended_card_contains_its_text_and_meets_the_minimum_width() {
    let (_course, player) = hello();
    let mut harness = Harness::new(1280.0, 720.0);
    harness.frame.ended = true;
    harness.run(&player, Vec::new());

    let card = harness.frame.ended_card_rect().expect("ended card");
    let (heading, small) = harness.frame.ended_text_rects().expect("ended text");
    assert!(
        card.width() >= 280.0,
        "completion card must be at least 280 wide, got {}",
        card.width()
    );
    assert!(
        card.contains_rect(heading),
        "heading {heading:?} must sit inside the card {card:?}"
    );
    assert!(
        card.contains_rect(small),
        "hint {small:?} must sit inside the card {card:?}"
    );
}

const CAPTIONS_JSON: &str = r#"{
  "schema_version": 1,
  "id": "captions",
  "title": "Captions",
  "slide_size": { "w": 960, "h": 540 },
  "scenes": [{
    "id": "sc",
    "title": "Scene",
    "slides": [
      {
        "id": "s1",
        "title": "One",
        "duration_ms": 10000,
        "objects": [{
          "id": "audio1", "name": "Audio",
          "kind": { "type": "audio", "src": "a.wav", "autoplay": false, "captions": "a.vtt" },
          "frame": { "x": 0.0, "y": 0.0, "w": 48.0, "h": 48.0 }
        }]
      },
      {
        "id": "s2",
        "title": "Two",
        "duration_ms": 10000,
        "objects": [{
          "id": "audio2", "name": "Audio",
          "kind": { "type": "audio", "src": "b.wav", "autoplay": false, "captions": "b.vtt" },
          "frame": { "x": 0.0, "y": 0.0, "w": 48.0, "h": 48.0 }
        }]
      }
    ]
  }]
}"#;

fn captions() -> (Course, Player<'static>) {
    let course = Course::from_json(CAPTIONS_JSON).expect("valid course");
    let boxed: &'static Course = Box::leak(Box::new(course.clone()));
    let mut player = Player::new(boxed).expect("valid player");
    player.handle(&Input::Start);
    (course, player)
}

fn captions_button(frame: &PlayerFrame) -> Option<Rect> {
    frame.chrome_rect(&crate::focus::ChromeButton::Captions)
}

impl Harness {
    fn run_with_text_field(
        &mut self,
        player: &Player<'_>,
        events: Vec<Event>,
        text: &mut String,
    ) -> FrameOutput {
        self.time += 0.1;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), self.size)),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        let mut output = FrameOutput::default();
        let frame = &mut self.frame;
        let view = &mut self.view;
        let full = self.ctx.run_ui(raw, |ui| {
            let id = egui::Id::unique("captions-test-text-field");
            let response = ui.add(egui::TextEdit::singleline(text).id(id));
            response.request_focus();
            output = frame.show(ui, player, view, None);
        });
        full.drop_without_applying_deltas();
        output
    }
}

#[test]
fn cc_button_only_appears_when_the_slide_has_captions() {
    let (_course, player) = hello();
    let mut plain = Harness::new(1280.0, 720.0);
    plain.run(&player, Vec::new());
    assert!(captions_button(&plain.frame).is_none());

    let (_course, player) = captions();
    let mut harness = Harness::new(1280.0, 720.0);
    harness.run(&player, Vec::new());
    let button = captions_button(&harness.frame).expect("CC button");
    let strip = harness.frame.strip_rect().expect("strip");
    assert!(strip.contains_rect(button));
}

#[test]
fn cc_button_click_toggles_captions() {
    let (_course, player) = captions();
    let mut harness = Harness::new(1280.0, 720.0);
    harness.run(&player, Vec::new());
    assert!(!harness.frame.captions_on());
    let button = captions_button(&harness.frame).expect("CC button");
    harness.run(&player, click(button.center()));
    assert!(harness.frame.captions_on());
    harness.run(&player, click(button.center()));
    assert!(!harness.frame.captions_on());
}

#[test]
fn c_key_toggles_captions_and_persists_across_slides() {
    let (_course, mut player) = captions();
    let mut harness = Harness::new(1280.0, 720.0);
    harness.run(&player, Vec::new());
    harness.run(&player, vec![key(egui::Key::C)]);
    assert!(harness.frame.captions_on());

    player.handle(&Input::Next);
    harness.run(&player, Vec::new());
    assert_eq!(player.current_slide().map(|s| s.id.as_str()), Some("s2"));
    assert!(harness.frame.captions_on(), "the preference persists");

    harness.run(&player, vec![key(egui::Key::C)]);
    assert!(!harness.frame.captions_on());
}

#[test]
fn c_key_is_ignored_while_a_text_field_has_focus() {
    let (_course, player) = captions();
    let mut harness = Harness::new(1280.0, 720.0);
    let mut text = String::new();
    harness.run_with_text_field(&player, Vec::new(), &mut text);
    harness.run_with_text_field(&player, Vec::new(), &mut text);
    harness.run_with_text_field(&player, vec![key(egui::Key::C)], &mut text);
    assert!(!harness.frame.captions_on());
}

fn ended_text_widths(course: Course) -> (f32, f32) {
    let boxed: &'static Course = Box::leak(Box::new(course));
    let mut player = Player::new(boxed).expect("valid player");
    player.handle(&Input::Start);
    let mut harness = Harness::new(1280.0, 720.0);
    harness.frame.ended = true;
    harness.run(&player, Vec::new());
    let (heading, small) = harness.frame.ended_text_rects().expect("ended card text");
    (heading.width(), small.width())
}

#[test]
fn chrome_overrides_reach_the_ended_card() {
    let default = Course::from_json(HELLO_JSON).expect("valid course");
    let (default_heading, default_small) = ended_text_widths(default);
    let mut translated = Course::from_json(HELLO_JSON).expect("valid course");
    translated.strings.insert(
        "courseComplete".to_owned(),
        "COURSE COMPLETE TRANSLATED HEADING".to_owned(),
    );
    translated.strings.insert(
        "closeWindow".to_owned(),
        "CLOSE WINDOW TRANSLATED HINT TEXT".to_owned(),
    );
    let (heading, small) = ended_text_widths(translated);
    assert!(heading > default_heading, "heading used the default text");
    assert!(small > default_small, "hint used the default text");
}

// ---------------------------------------------------------------- notice tests

#[test]
fn no_notice_leaves_everything_as_it_was() {
    let (_course, player) = hello();
    let mut harness = Harness::new(1280.0, 720.0);
    let output = harness.run(&player, Vec::new());
    assert!(harness.frame.notice_rect().is_none());
    assert!(
        harness
            .frame
            .chrome_rect(&crate::focus::ChromeButton::DismissNotice)
            .is_none()
    );
    assert!(
        !harness
            .frame
            .focus()
            .list()
            .contains(&FocusTarget::Chrome(ChromeButton::DismissNotice))
    );
    assert!(output.slide_rect.is_positive());
}

#[test]
fn the_notice_banner_draws_above_the_slide_and_dismisses_for_the_session() {
    let (_course, player) = hello();
    let mut plain = Harness::new(1280.0, 720.0);
    let plain_out = plain.run(&player, Vec::new());

    let mut harness = Harness::new(1280.0, 720.0);
    harness.frame.notice = Some("Progress is not saved".to_owned());
    let output = harness.run(&player, Vec::new());
    let banner = harness
        .frame
        .notice_rect()
        .expect("the notice banner is painted");
    assert_eq!(banner.left(), 0.0, "the banner spans the player width");
    assert_eq!(banner.right(), 1280.0);
    assert_eq!(
        banner.top(),
        0.0,
        "the banner sits at the top of the player"
    );
    assert!(
        output.slide_rect == plain_out.slide_rect,
        "the banner must not push the layout"
    );

    // It is first in the keyboard focus order, with a dismiss button on screen.
    assert_eq!(
        harness.frame.focus().list().first(),
        Some(&FocusTarget::Chrome(ChromeButton::DismissNotice))
    );
    let close = harness
        .frame
        .chrome_rect(&crate::focus::ChromeButton::DismissNotice)
        .expect("dismiss button laid out");
    assert!(banner.contains_rect(close));

    // Dismissing hides it for the session, even though the notice is still set.
    let output = harness.run(&player, click(close.center()));
    assert!(output.inputs.is_empty(), "dismissing sends no input");
    harness.run(&player, Vec::new());
    assert!(
        harness.frame.notice_rect().is_none(),
        "the notice stays hidden for the session"
    );
    assert!(
        harness
            .frame
            .chrome_rect(&crate::focus::ChromeButton::DismissNotice)
            .is_none()
    );
}

#[test]
fn the_notice_banner_wraps_at_narrow_widths() {
    let (_course, player) = hello();
    let text = "Your progress isn't being saved. To keep your results, close this and \
        reopen the course from your LMS.";

    let mut wide = Harness::new(1280.0, 720.0);
    wide.frame.notice = Some(text.to_owned());
    wide.run(&player, Vec::new());
    let wide_banner = wide.frame.notice_rect().expect("wide banner");

    let mut narrow = Harness::new(360.0, 720.0);
    narrow.frame.notice = Some(text.to_owned());
    narrow.run(&player, Vec::new());
    let narrow_banner = narrow.frame.notice_rect().expect("narrow banner");

    assert!(
        narrow_banner.height() > wide_banner.height(),
        "the notice must wrap onto more lines when narrow: {} vs {}",
        narrow_banner.height(),
        wide_banner.height()
    );
}

// ---------------------------------------------------------------- player controls

fn control(frame: &PlayerFrame, button: ChromeButton) -> Rect {
    frame.chrome_rect(&button).expect("control was laid out")
}

fn press(pos: Pos2) -> Vec<Event> {
    vec![Event::PointerMoved(pos), pointer(pos, true)]
}

fn release(pos: Pos2) -> Vec<Event> {
    vec![pointer(pos, false)]
}

fn pointer(pos: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::default(),
    }
}

fn apply_inputs(player: &mut Player<'_>, inputs: &[Input]) {
    for input in inputs {
        player.handle(input);
    }
}

fn controls(frame: &PlayerFrame) -> Vec<(ChromeButton, Rect)> {
    [
        ChromeButton::PlayPause,
        ChromeButton::Replay,
        ChromeButton::Seek,
        ChromeButton::Mute,
        ChromeButton::Volume,
        ChromeButton::Captions,
        ChromeButton::Outline,
        ChromeButton::Previous,
        ChromeButton::Next,
    ]
    .into_iter()
    .filter_map(|button| frame.chrome_rect(&button).map(|rect| (button, rect)))
    .collect()
}

#[test]
fn play_and_pause_send_the_runtime_inputs_and_keep_the_strip_visible() {
    let (_course, mut player) = hello();
    let mut harness = Harness::new(1280.0, 720.0);
    harness.frame.overlay_strip = true;
    harness.run(&player, Vec::new());
    let play = control(&harness.frame, ChromeButton::PlayPause).center();

    let output = harness.run(&player, click(play));
    assert_eq!(output.inputs, vec![Input::PlayerPause]);
    apply_inputs(&mut player, &output.inputs);
    harness.frame.hide_strip();
    harness.run(&player, Vec::new());
    assert!(
        harness.frame.strip_rect().is_some(),
        "the strip stays visible while paused"
    );

    let output = harness.run(&player, click(play));
    assert_eq!(output.inputs, vec![Input::PlayerResume]);
    apply_inputs(&mut player, &output.inputs);
    assert!(!player.player_paused());
}

#[test]
fn replay_seeks_to_zero_and_resumes_a_paused_player() {
    let (_course, mut player) = hello();
    let mut harness = Harness::new(1280.0, 720.0);
    harness.run(&player, Vec::new());
    let replay = control(&harness.frame, ChromeButton::Replay).center();

    let output = harness.run(&player, click(replay));
    assert_eq!(output.inputs, vec![Input::PlayerSeek { ms: 0 }]);

    player.handle(&Input::PlayerPause);
    harness.run(&player, Vec::new());
    let replay = control(&harness.frame, ChromeButton::Replay).center();
    let output = harness.run(&player, click(replay));
    assert_eq!(
        output.inputs,
        vec![Input::PlayerResume, Input::PlayerSeek { ms: 0 }]
    );
}

#[test]
fn the_seek_bar_sends_once_on_release_and_tracks_the_drag() {
    let (_course, mut player) = hello();
    let mut harness = Harness::new(1280.0, 720.0);
    harness.run(&player, Vec::new());
    let seek = control(&harness.frame, ChromeButton::Seek);
    let from = pos2(seek.left() + seek.width() * 0.25, seek.center().y);
    let to = pos2(seek.left() + seek.width() * 0.75, seek.center().y);

    let output = harness.run(&player, press(from));
    assert!(output.inputs.is_empty(), "a press must not seek yet");
    let output = harness.run(&player, vec![Event::PointerMoved(to)]);
    assert!(output.inputs.is_empty(), "dragging must not seek yet");
    let output = harness.run(&player, release(to));
    assert_eq!(output.inputs.len(), 1, "one seek on release");
    match output.inputs.first() {
        Some(Input::PlayerSeek { ms }) => {
            assert!(*ms > 2_500 && *ms <= 3_200, "seek landed at {ms} ms");
        }
        other => panic!("expected a seek, got {other:?}"),
    }
    apply_inputs(&mut player, &output.inputs);
    assert!(player.state().base.ms > 2_500);
}

#[test]
fn the_seek_bar_keyboard_steps_and_jumps() {
    let (_course, mut player) = hello();
    let mut harness = Harness::new(1280.0, 720.0);
    harness.run(&player, Vec::new());
    harness
        .frame
        .focus_mut()
        .external(crate::focus::ExternalFocus::Focused(FocusTarget::Chrome(
            ChromeButton::Seek,
        )));
    harness.run(&player, Vec::new());

    let output = harness.run(&player, vec![key(egui::Key::ArrowRight)]);
    assert_eq!(output.inputs, vec![Input::PlayerSeek { ms: 1_000 }]);
    apply_inputs(&mut player, &output.inputs);
    let output = harness.run(&player, vec![key(egui::Key::End)]);
    assert_eq!(output.inputs, vec![Input::PlayerSeek { ms: 4_000 }]);
    apply_inputs(&mut player, &output.inputs);
    let output = harness.run(&player, vec![key(egui::Key::Home)]);
    assert_eq!(output.inputs, vec![Input::PlayerSeek { ms: 0 }]);
}

#[test]
fn a_disabled_seek_bar_sends_nothing() {
    let json = r#"{
      "schema_version": 1,
      "id": "no-duration",
      "title": "No duration",
      "scenes": [{ "id": "sc", "title": "Scene", "slides": [{
        "id": "s1", "title": "One", "duration_ms": 0,
        "objects": [{ "id": "t", "kind": { "type": "text", "text": "Hi" },
          "frame": { "x": 0.0, "y": 0.0, "w": 100.0, "h": 40.0 } }]
      }] }]
    }"#;
    let course = Course::from_json(json).expect("valid course");
    let boxed: &'static Course = Box::leak(Box::new(course));
    let mut player = Player::new(boxed).expect("valid player");
    player.handle(&Input::Start);
    let mut harness = Harness::new(1280.0, 720.0);
    harness.run(&player, Vec::new());
    let seek = control(&harness.frame, ChromeButton::Seek);
    let output = harness.run(&player, click(seek.center()));
    assert!(
        output.inputs.is_empty(),
        "a zero-duration slide cannot seek"
    );
    let output = harness.run(&player, vec![key(egui::Key::ArrowRight)]);
    assert!(
        !output
            .inputs
            .iter()
            .any(|input| matches!(input, Input::PlayerSeek { .. })),
        "arrow keys on a disabled seek bar send no seek"
    );
}

#[test]
fn mute_and_volume_reach_the_media_driver() {
    let (_course, player) = hello();
    let backend = coursecraft_playback::NullBackend::new();
    let mut driver = crate::MediaDriver::new().with_backend(Box::new(backend.clone()));
    let mut harness = Harness::new(1280.0, 720.0);
    harness.run(&player, Vec::new());
    let mute = control(&harness.frame, ChromeButton::Mute).center();
    let output = harness.run(&player, click(mute));
    assert!(output.inputs.is_empty());
    assert_eq!(output.volume, Some(0.0));
    if let Some(volume) = output.volume {
        driver.set_volume(volume);
    }
    assert_eq!(driver.volume(), 0.0);
    assert_eq!(backend.volume(), 0.0);

    let volume = control(&harness.frame, ChromeButton::Volume);
    let output = harness.run(
        &player,
        click(pos2(
            volume.left() + volume.width() * 0.4,
            volume.center().y,
        )),
    );
    assert!(output.volume.is_some_and(|v| (v - 0.4).abs() < 0.1));
    if let Some(volume) = output.volume {
        driver.set_volume(volume);
    }
    assert!(driver.volume() > 0.3 && driver.volume() < 0.5);
}

fn text_in_rect(shapes: &[egui::epaint::ClippedShape], rect: Rect) -> Option<String> {
    fn walk(shape: &egui::epaint::Shape, rect: Rect) -> Option<String> {
        match shape {
            egui::epaint::Shape::Text(t) => {
                let center = t.galley.rect.translate(t.pos.to_vec2()).center();
                if rect.contains(center) {
                    Some(t.galley.job.text.clone())
                } else {
                    None
                }
            }
            egui::epaint::Shape::Vec(items) => items.iter().find_map(|s| walk(s, rect)),
            _ => None,
        }
    }
    shapes.iter().find_map(|cs| walk(&cs.shape, rect))
}

#[test]
fn play_pause_shows_play_icon_at_end_and_replays_same_slide() {
    let (_course, mut player) = hello();
    let initial_slide = player.state().slide;
    let duration = player.seek_info().duration_ms;
    let mut harness = Harness::new(1280.0, 720.0);

    // Playing mid-slide: shows Pause icon.
    harness.run(&player, Vec::new());
    let play_rect = control(&harness.frame, ChromeButton::PlayPause);
    assert_eq!(
        text_in_rect(&harness.last_shapes, play_rect).as_deref(),
        Some(crate::Icon::Pause.codepoint().to_string().as_str())
    );

    // Paused mid-slide: shows Play icon, clicking resumes.
    player.handle(&Input::PlayerPause);
    harness.run(&player, Vec::new());
    let play_rect = control(&harness.frame, ChromeButton::PlayPause);
    assert_eq!(
        text_in_rect(&harness.last_shapes, play_rect).as_deref(),
        Some(crate::Icon::Play.codepoint().to_string().as_str())
    );
    let output = harness.run(&player, click(play_rect.center()));
    assert_eq!(output.inputs, vec![Input::PlayerResume]);
    apply_inputs(&mut player, &output.inputs);

    // Drive timeline to the end of the slide.
    player.handle(&Input::Tick { ms: duration });
    assert!(player.state().base.ended);
    harness.run(&player, Vec::new());

    let play_rect = control(&harness.frame, ChromeButton::PlayPause);
    let replay_rect = control(&harness.frame, ChromeButton::Replay);
    assert_eq!(
        text_in_rect(&harness.last_shapes, replay_rect).as_deref(),
        Some(
            crate::Icon::ArrowCounterClockwise
                .codepoint()
                .to_string()
                .as_str()
        )
    );
    assert_eq!(
        text_in_rect(&harness.last_shapes, play_rect).as_deref(),
        Some(crate::Icon::Play.codepoint().to_string().as_str()),
        "Play/Pause button at end of slide must show Play icon, not Replay"
    );

    // Clicking Play at the end replays the current slide from 0 and never advances.
    let output = harness.run(&player, click(play_rect.center()));
    assert_eq!(output.inputs, vec![Input::PlayerSeek { ms: 0 }]);
    apply_inputs(&mut player, &output.inputs);
    assert_eq!(player.state().slide, initial_slide);
    assert_eq!(player.seek_info().elapsed_ms, 0);
    assert!(!player.state().base.ended);

    harness.run(&player, Vec::new());
    let play_rect = control(&harness.frame, ChromeButton::PlayPause);
    assert_eq!(
        text_in_rect(&harness.last_shapes, play_rect).as_deref(),
        Some(crate::Icon::Pause.codepoint().to_string().as_str())
    );

    // Paused at the end of the slide: still shows Play and replays + resumes.
    player.handle(&Input::Tick { ms: duration });
    player.handle(&Input::PlayerPause);
    harness.run(&player, Vec::new());
    let play_rect = control(&harness.frame, ChromeButton::PlayPause);
    assert_eq!(
        text_in_rect(&harness.last_shapes, play_rect).as_deref(),
        Some(crate::Icon::Play.codepoint().to_string().as_str())
    );
    let output = harness.run(&player, click(play_rect.center()));
    assert_eq!(
        output.inputs,
        vec![Input::PlayerResume, Input::PlayerSeek { ms: 0 }]
    );
    apply_inputs(&mut player, &output.inputs);
    assert_eq!(player.state().slide, initial_slide);
    assert_eq!(player.seek_info().elapsed_ms, 0);
    assert!(!player.player_paused());
    assert!(!player.state().base.ended);
}

#[test]
fn the_main_button_acts_as_replay_at_the_end_of_a_slide() {
    let (_course, mut player) = hello();
    player.handle(&Input::PlayerSeek { ms: 4_000 });
    let mut harness = Harness::new(1280.0, 720.0);
    harness.run(&player, Vec::new());
    let play = control(&harness.frame, ChromeButton::PlayPause).center();
    let output = harness.run(&player, click(play));
    assert_eq!(output.inputs, vec![Input::PlayerSeek { ms: 0 }]);
}

#[test]
fn the_focus_order_matches_the_strip_left_to_right() {
    let (_course, player) = hello();
    let mut harness = Harness::new(1280.0, 720.0);
    harness.run(&player, Vec::new());
    let list = harness.frame.focus().list();
    let position = |button: ChromeButton| {
        list.iter()
            .position(|target| target == &FocusTarget::Chrome(button.clone()))
            .unwrap_or(usize::MAX)
    };
    assert!(position(ChromeButton::PlayPause) < position(ChromeButton::Replay));
    assert!(position(ChromeButton::Replay) < position(ChromeButton::Seek));
    assert!(position(ChromeButton::Seek) < position(ChromeButton::Mute));
    assert!(position(ChromeButton::Mute) < position(ChromeButton::Volume));
    assert!(position(ChromeButton::Volume) < position(ChromeButton::Outline));
    assert!(position(ChromeButton::Outline) < position(ChromeButton::Next));
}

#[test]
fn the_strip_fits_without_overlaps_from_320_to_1280() {
    let (_course, player) = hello();
    for width in [320.0, 390.0, 820.0, 1280.0] {
        let mut harness = Harness::new(width, 720.0);
        harness.run(&player, Vec::new());
        let strip = harness.frame.strip_rect().expect("strip");
        assert!(strip.width() <= width - 2.0 * super::STRIP_MARGIN + 0.5);
        let rects = controls(&harness.frame);
        for (button, rect) in &rects {
            assert!(
                strip.contains_rect(*rect),
                "{button:?} at {rect:?} falls outside {strip:?} at width {width}"
            );
            assert!(
                rect.height() >= 44.0,
                "{button:?} hit target is only {} pt tall at width {width}",
                rect.height()
            );
        }
        for (index, (a, ra)) in rects.iter().enumerate() {
            for (b, rb) in rects.iter().skip(index + 1) {
                assert!(
                    !ra.intersects(*rb),
                    "{a:?} overlaps {b:?} at width {width}: {ra:?} / {rb:?}"
                );
            }
        }
    }
}

#[test]
fn space_on_the_transport_controls_pauses_and_mutes() {
    let (_course, player) = hello();
    let mut harness = Harness::new(1280.0, 720.0);
    harness.run(&player, Vec::new());

    harness
        .frame
        .focus_mut()
        .external(crate::focus::ExternalFocus::Focused(FocusTarget::Chrome(
            ChromeButton::PlayPause,
        )));
    harness.run(&player, Vec::new());
    let output = harness.run(&player, vec![key(egui::Key::Space)]);
    assert_eq!(output.inputs, vec![Input::PlayerPause]);

    harness
        .frame
        .focus_mut()
        .external(crate::focus::ExternalFocus::Focused(FocusTarget::Chrome(
            ChromeButton::Mute,
        )));
    harness.run(&player, Vec::new());
    let output = harness.run(&player, vec![key(egui::Key::Space)]);
    assert_eq!(output.volume, Some(0.0));
}

#[test]
fn show_seekbar_gates_the_seek_bar_and_its_time_label() {
    let mut course = Course::from_json(HELLO_JSON).expect("valid course");
    course.player.show_seekbar = false;
    let boxed: &'static Course = Box::leak(Box::new(course));
    let mut player = Player::new(boxed).expect("valid player");
    player.handle(&Input::Start);
    let mut harness = Harness::new(1280.0, 720.0);
    harness.run(&player, Vec::new());
    assert!(
        harness.frame.chrome_rect(&ChromeButton::Seek).is_none(),
        "show_seekbar off removes the seek bar"
    );
    assert!(
        !harness
            .frame
            .focus()
            .list()
            .contains(&FocusTarget::Chrome(ChromeButton::Seek)),
        "the hidden seek bar leaves the focus order"
    );
    assert!(
        harness.frame.chrome_visibility().play && harness.frame.chrome_visibility().volume,
        "play and volume do not depend on show_seekbar"
    );
}

#[test]
fn narrow_widths_drop_controls_in_the_documented_order() {
    let (_course, player) = captions();
    let visible = |width: f32| {
        let mut harness = Harness::new(width, 720.0);
        harness.run(&player, Vec::new());
        harness.frame.chrome_visibility()
    };
    let narrow = visible(320.0);
    assert!(narrow.play && narrow.seek && narrow.next);
    assert!(
        !narrow.replay,
        "Replay drops at 320 with the caption button"
    );
    assert!(!narrow.volume_slider, "the volume slider drops at 320");

    let phone = visible(390.0);
    assert!(phone.play && phone.seek && phone.next);
    assert!(phone.replay, "Replay fits again at 390");
    assert!(!phone.volume_slider, "the volume slider drops before 390");

    let desktop = visible(1280.0);
    assert!(desktop.replay && desktop.volume_slider && desktop.next);
}

#[test]
fn next_is_disabled_while_the_slide_requirement_is_unmet() {
    let mut course = Course::from_json(HELLO_JSON).expect("valid course");
    course.player.next_requires = coursecraft_course::NextRequirement::TimelineEnds;
    let boxed: &'static Course = Box::leak(Box::new(course));
    let mut player = Player::new(boxed).expect("valid player");
    player.handle(&Input::Start);
    assert!(!player.can_next());

    let mut harness = Harness::new(1280.0, 720.0);
    harness.run(&player, Vec::new());
    let next = harness
        .frame
        .chrome_rect(&ChromeButton::Next)
        .expect("next button")
        .center();
    let output = harness.run(&player, click(next));
    assert!(
        !output.inputs.contains(&Input::Next),
        "locked Next is inert"
    );
    let output = harness.run(&player, vec![key(egui::Key::ArrowRight)]);
    assert!(!output.inputs.contains(&Input::Next));

    // Once the slide finishes, the same button works.
    player.handle(&Input::Tick { ms: 60_000 });
    assert!(player.can_next());
    harness.run(&player, Vec::new());
    let output = harness.run(&player, click(next));
    assert_eq!(output.inputs, vec![Input::Next]);
}

#[test]
fn locked_menu_rows_send_nothing() {
    let mut course = Course::from_json(HELLO_JSON).expect("valid course");
    course.player.navigation = coursecraft_course::NavMode::Locked;
    let boxed: &'static Course = Box::leak(Box::new(course));
    let mut player = Player::new(boxed).expect("valid player");
    player.handle(&Input::Start);

    let mut harness = Harness::new(1280.0, 720.0);
    harness.run(&player, Vec::new());
    harness.run(&player, vec![key(egui::Key::M)]);
    harness.run(&player, Vec::new());
    harness.run(&player, Vec::new());
    let second = boxed.slides().nth(1).expect("second slide").id.clone();
    let row = harness
        .frame
        .chrome_rect(&ChromeButton::DrawerRow(second))
        .expect("drawer row")
        .center();
    let output = harness.run(&player, click(row));
    assert!(
        !output
            .inputs
            .iter()
            .any(|input| matches!(input, Input::GoTo { .. })),
        "locked rows are inert"
    );
}

#[test]
fn speed_popup_picks_a_rate() {
    let (_course, player) = hello();
    let mut harness = Harness::new(1280.0, 720.0);
    harness.frame.can_speed = true;
    harness.run(&player, Vec::new());
    harness.run(&player, Vec::new());
    let speed = harness
        .frame
        .chrome_rect(&ChromeButton::Speed)
        .expect("speed button")
        .center();
    let output = harness.run(&player, click(speed));
    assert!(output.inputs.is_empty(), "opening the list sends nothing");
    harness.run(&player, Vec::new());
    // The list is laid out above the button, slowest nearest the strip. The
    // 2x row is the sixth, so it is the highest.
    let top = pos2(speed.x, speed.y - 24.0 - 5.0 * 30.0 - 12.0);
    let output = harness.run(&player, click(top));
    assert_eq!(output.inputs, vec![Input::SetSpeed { rate: 2.0 }]);
}

#[test]
fn speed_button_needs_the_host_flag_and_the_setting() {
    let (_course, player) = hello();
    let mut harness = Harness::new(1280.0, 720.0);
    harness.run(&player, Vec::new());
    assert!(
        harness.frame.chrome_rect(&ChromeButton::Speed).is_none(),
        "hidden when the backend cannot change rate"
    );
    harness.frame.can_speed = true;
    harness.run(&player, Vec::new());
    assert!(harness.frame.chrome_rect(&ChromeButton::Speed).is_some());

    let mut course = Course::from_json(HELLO_JSON).expect("valid course");
    course.player.show_speed = false;
    course.player.show_fullscreen = false;
    let boxed: &'static Course = Box::leak(Box::new(course));
    let mut off = Player::new(boxed).expect("valid player");
    off.handle(&Input::Start);
    harness.frame.can_fullscreen = true;
    harness.run(&off, Vec::new());
    assert!(harness.frame.chrome_rect(&ChromeButton::Speed).is_none());
    assert!(
        harness
            .frame
            .chrome_rect(&ChromeButton::Fullscreen)
            .is_none()
    );
}

#[test]
fn fullscreen_button_reports_a_toggle_request() {
    let (_course, player) = hello();
    let mut harness = Harness::new(1280.0, 720.0);
    harness.run(&player, Vec::new());
    assert!(
        harness
            .frame
            .chrome_rect(&ChromeButton::Fullscreen)
            .is_none(),
        "hidden unless the host can go full screen"
    );
    harness.frame.can_fullscreen = true;
    harness.run(&player, Vec::new());
    let button = harness
        .frame
        .chrome_rect(&ChromeButton::Fullscreen)
        .expect("full screen button")
        .center();
    let output = harness.run(&player, click(button));
    assert!(output.fullscreen_toggle);
    let output = harness.run(&player, Vec::new());
    assert!(!output.fullscreen_toggle);
}

fn course_with_drawer_content(transcript: bool) -> &'static Course {
    let mut course = Course::from_json(HELLO_JSON).expect("valid course");
    course.resources = vec![coursecraft_course::Resource {
        id: "guide".into(),
        title: "Guide".into(),
        target: coursecraft_course::ResourceTarget::Url {
            url: "https://example.com/guide".into(),
        },
    }];
    course.glossary = ["Zebra", "Apple", "Avocado"]
        .iter()
        .enumerate()
        .map(|(i, term)| coursecraft_course::GlossaryEntry {
            id: format!("g{i}"),
            term: (*term).into(),
            definition: format!("About {term}."),
        })
        .collect();
    course.player.show_transcript = transcript;
    course.slide_mut("s-intro").expect("intro").notes = "Intro narration.".into();
    course.slide_mut("s-tabs").expect("tabs").notes = "Tabs narration.".into();
    Box::leak(Box::new(course))
}

fn open_drawer(harness: &mut Harness, player: &Player<'_>) {
    harness.run(player, Vec::new());
    harness.run(player, vec![key(egui::Key::M)]);
    for _ in 0..4 {
        harness.run(player, Vec::new());
    }
}

#[test]
fn drawer_has_no_tabs_when_there_is_nothing_besides_the_menu() {
    let (_course, player) = hello();
    let mut harness = Harness::new(1280.0, 720.0);
    open_drawer(&mut harness, &player);
    for tab in [
        crate::drawer::DrawerTab::Menu,
        crate::drawer::DrawerTab::Resources,
        crate::drawer::DrawerTab::Glossary,
        crate::drawer::DrawerTab::Transcript,
    ] {
        assert!(
            harness
                .frame
                .chrome_rect(&ChromeButton::DrawerTab(tab))
                .is_none(),
            "{tab:?}"
        );
    }
}

#[test]
fn drawer_tabs_follow_content_and_settings() {
    use crate::drawer::{DrawerTab, available_tabs};
    let course = course_with_drawer_content(false);
    assert_eq!(
        available_tabs(course),
        vec![DrawerTab::Menu, DrawerTab::Resources, DrawerTab::Glossary]
    );
    let with = course_with_drawer_content(true);
    assert!(available_tabs(with).contains(&DrawerTab::Transcript));
}

#[test]
fn a_resource_click_asks_the_runtime_to_open_it() {
    let course = course_with_drawer_content(false);
    let mut player = Player::new(course).expect("valid player");
    player.handle(&Input::Start);
    let mut harness = Harness::new(1280.0, 720.0);
    open_drawer(&mut harness, &player);
    let tab = harness
        .frame
        .chrome_rect(&ChromeButton::DrawerTab(
            crate::drawer::DrawerTab::Resources,
        ))
        .expect("resources tab")
        .center();
    harness.run(&player, click(tab));
    for _ in 0..2 {
        harness.run(&player, Vec::new());
    }
    let row = harness
        .frame
        .chrome_rect(&ChromeButton::ResourceRow("guide".into()))
        .expect("resource row")
        .center();
    let output = harness.run(&player, click(row));
    assert_eq!(
        output.inputs,
        vec![Input::OpenResource {
            resource: "guide".into()
        }]
    );
    let effects = player.handle(&output.inputs[0]);
    assert_eq!(
        effects,
        vec![coursecraft_logic::Effect::OpenUrl {
            url: "https://example.com/guide".into()
        }]
    );
}

#[test]
fn glossary_lists_terms_a_to_z_and_filters_by_the_search_text() {
    use crate::drawer::visible_glossary;
    let course = course_with_drawer_content(false);
    let terms: Vec<&str> = visible_glossary(course, "")
        .iter()
        .map(|entry| entry.term.as_str())
        .collect();
    assert_eq!(terms, vec!["Apple", "Avocado", "Zebra"]);
    let filtered: Vec<&str> = visible_glossary(course, " av ")
        .iter()
        .map(|entry| entry.term.as_str())
        .collect();
    assert_eq!(filtered, vec!["Avocado"]);
    assert!(visible_glossary(course, "nothing").is_empty());

    let mut player = Player::new(course).expect("valid player");
    player.handle(&Input::Start);
    let mut harness = Harness::new(1280.0, 720.0);
    open_drawer(&mut harness, &player);
    let tab = harness
        .frame
        .chrome_rect(&ChromeButton::DrawerTab(crate::drawer::DrawerTab::Glossary))
        .expect("glossary tab")
        .center();
    harness.run(&player, click(tab));
    for _ in 0..2 {
        harness.run(&player, Vec::new());
    }
    assert!(
        harness
            .frame
            .chrome_rect(&ChromeButton::GlossaryTerm("g0".into()))
            .is_some()
    );
    let term = harness
        .frame
        .chrome_rect(&ChromeButton::GlossaryTerm("g1".into()))
        .expect("Apple row")
        .center();
    harness.run(&player, click(term));
    harness.run(&player, Vec::new());
    assert_eq!(harness.frame.drawer_view().selected.as_deref(), Some("g1"));
}

#[test]
fn the_transcript_shows_the_current_slides_notes_and_follows_the_slide() {
    let course = course_with_drawer_content(true);
    let mut player = Player::new(course).expect("valid player");
    player.handle(&Input::Start);
    let mut harness = Harness::new(1280.0, 720.0);
    open_drawer(&mut harness, &player);
    let tab = harness
        .frame
        .chrome_rect(&ChromeButton::DrawerTab(
            crate::drawer::DrawerTab::Transcript,
        ))
        .expect("transcript tab")
        .center();
    harness.run(&player, click(tab));
    for _ in 0..2 {
        harness.run(&player, Vec::new());
    }
    assert!(
        harness
            .frame
            .chrome_rect(&ChromeButton::TranscriptText)
            .is_some()
    );
    let notes = |harness: &Harness, player: &Player<'_>| {
        let frame = &harness.frame;
        let items = crate::compose::compose(player);
        crate::access::build_access_tree(player, frame, &items, true, true)
            .nodes
            .into_iter()
            .find(|node| node.key == "chrome:transcript")
            .map(|node| node.label)
    };
    assert_eq!(
        notes(&harness, &player).as_deref(),
        Some("Intro narration.")
    );
    player.handle(&Input::Next);
    harness.run(&player, Vec::new());
    assert_eq!(notes(&harness, &player).as_deref(), Some("Tabs narration."));
}

#[test]
fn drawer_items_have_tab_link_and_list_item_roles() {
    use crate::access::AccessRole;
    let course = course_with_drawer_content(false);
    let mut player = Player::new(course).expect("valid player");
    player.handle(&Input::Start);
    let mut harness = Harness::new(1280.0, 720.0);
    open_drawer(&mut harness, &player);
    let role_of = |harness: &Harness, key: &str| {
        let items = crate::compose::compose(&player);
        crate::access::build_access_tree(&player, &harness.frame, &items, true, true)
            .nodes
            .into_iter()
            .find(|node| node.key == key)
            .map(|node| node.role)
    };
    assert_eq!(
        role_of(&harness, "chrome:drawer-tab:tabResources"),
        Some(AccessRole::Tab)
    );
    let tab = harness
        .frame
        .chrome_rect(&ChromeButton::DrawerTab(
            crate::drawer::DrawerTab::Resources,
        ))
        .expect("resources tab")
        .center();
    harness.run(&player, click(tab));
    for _ in 0..2 {
        harness.run(&player, Vec::new());
    }
    assert_eq!(
        role_of(&harness, "chrome:resource:guide"),
        Some(AccessRole::Link)
    );
    let tab = harness
        .frame
        .chrome_rect(&ChromeButton::DrawerTab(crate::drawer::DrawerTab::Glossary))
        .expect("glossary tab")
        .center();
    harness.run(&player, click(tab));
    for _ in 0..2 {
        harness.run(&player, Vec::new());
    }
    assert_eq!(
        role_of(&harness, "chrome:glossary-term:g0"),
        Some(AccessRole::ListItem)
    );
}

struct LogoSource(std::sync::Arc<[u8]>);

impl crate::media::MediaSource for LogoSource {
    fn get(&self, src: &str) -> crate::media::MediaFetch {
        if src == "logo.png" {
            crate::media::MediaFetch::Ready(self.0.clone())
        } else {
            crate::media::MediaFetch::Missing
        }
    }
}

fn run_with_logo(harness: &mut Harness, player: &Player<'_>, cache: &mut crate::media::MediaCache) {
    // 100 x 56 pixels, so it is scaled down to 28 pt high and ~50 pt wide.
    let pixels = vec![200u8; 100 * 56 * 4];
    let png = coursecraft_media::encode_png_rgba(100, 56, &pixels).expect("png");
    let source = LogoSource(png.into());
    harness.time += 0.1;
    let raw = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), harness.size)),
        time: Some(harness.time),
        ..Default::default()
    };
    let frame = &mut harness.frame;
    let view = &mut harness.view;
    let full = harness.ctx.run_ui(raw, |ui| {
        frame.show(
            ui,
            player,
            view,
            Some((&mut *cache, &source as &dyn crate::media::MediaSource)),
        );
    });
    full.drop_without_applying_deltas();
}

#[test]
fn the_logo_is_drawn_at_the_left_of_the_strip_and_the_title_can_be_hidden() {
    let mut course = Course::from_json(HELLO_JSON).expect("valid course");
    course.player.logo = Some("logo.png".into());
    course.player.show_title = false;
    let boxed: &'static Course = Box::leak(Box::new(course));
    let mut player = Player::new(boxed).expect("valid player");
    player.handle(&Input::Start);
    let mut harness = Harness::new(1280.0, 720.0);
    let mut cache = crate::media::MediaCache::new();
    for _ in 0..3 {
        run_with_logo(&mut harness, &player, &mut cache);
    }
    let logo = harness.frame.logo_rect().expect("logo drawn");
    let strip = harness.frame.strip_rect().expect("strip");
    assert!(logo.height() <= 28.0 + 0.01, "{logo:?}");
    assert!((logo.width() / logo.height() - 100.0 / 56.0).abs() < 0.05);
    let play = harness
        .frame
        .chrome_rect(&ChromeButton::PlayPause)
        .expect("play");
    assert!(logo.right() <= play.left() && logo.left() >= strip.left());
    // With no title shown, the strip is no wider than without one.
    let mut plain = Course::from_json(HELLO_JSON).expect("valid course");
    plain.player.show_title = true;
    let plain: &'static Course = Box::leak(Box::new(plain));
    let mut titled = Player::new(plain).expect("valid player");
    titled.handle(&Input::Start);
    let mut other = Harness::new(1280.0, 720.0);
    other.run(&titled, Vec::new());
    assert!(
        other.frame.chrome_rect(&ChromeButton::PlayPause).is_some()
            && other.frame.logo_rect().is_none()
    );
}

#[test]
fn volume_and_captions_buttons_are_absent_when_their_settings_are_off() {
    let mut course = Course::from_json(HELLO_JSON).expect("valid course");
    course.player.show_volume = false;
    course.player.show_captions = false;
    let boxed: &'static Course = Box::leak(Box::new(course));
    let mut player = Player::new(boxed).expect("valid player");
    player.handle(&Input::Start);
    let mut harness = Harness::new(1280.0, 720.0);
    harness.run(&player, Vec::new());
    assert!(harness.frame.chrome_rect(&ChromeButton::Mute).is_none());
    assert!(harness.frame.chrome_rect(&ChromeButton::Volume).is_none());
    assert!(harness.frame.chrome_rect(&ChromeButton::Captions).is_none());
    let (_, normal) = hello();
    harness.run(&normal, Vec::new());
    assert!(harness.frame.chrome_rect(&ChromeButton::Mute).is_some());
}

#[test]
fn a_slide_override_hides_the_navigation_buttons_on_that_slide() {
    let mut course = Course::from_json(HELLO_JSON).expect("valid course");
    course.slide_mut("s-intro").expect("intro").player =
        Some(coursecraft_course::SlidePlayerOverrides {
            show_nav: Some(false),
            ..Default::default()
        });
    let boxed: &'static Course = Box::leak(Box::new(course));
    let mut player = Player::new(boxed).expect("valid player");
    player.handle(&Input::Start);
    let mut harness = Harness::new(1280.0, 720.0);
    harness.run(&player, Vec::new());
    assert!(harness.frame.chrome_rect(&ChromeButton::Next).is_none());
    player.handle(&Input::Jump {
        slide: "s-tabs".into(),
    });
    harness.run(&player, Vec::new());
    assert!(harness.frame.chrome_rect(&ChromeButton::Next).is_some());
}

const HEALTHY_JSON: &str = include_str!("../../../../examples/healthy-habits/course.json");

fn healthy() -> Player<'static> {
    let course = Course::from_json(HEALTHY_JSON).expect("valid course");
    let boxed: &'static Course = Box::leak(Box::new(course));
    let mut player = Player::new(boxed).expect("valid player");
    player.handle(&Input::Start);
    player
}

fn wheel(delta_y: f32) -> Event {
    Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: vec2(0.0, delta_y),
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Unknown,
        modifiers: egui::Modifiers::default(),
    }
}

#[test]
fn scrolled_drawer_rows_never_report_a_place_over_the_tabs() {
    let player = healthy();
    let mut harness = Harness::new(1280.0, 720.0);
    open_drawer(&mut harness, &player);
    let drawer = harness.frame.drawer_rect().expect("drawer is painted");
    let tabs = harness
        .frame
        .chrome_rect(&ChromeButton::DrawerTab(crate::drawer::DrawerTab::Menu))
        .expect("the sample course has drawer tabs");
    let over = drawer.center();
    for _ in 0..8 {
        harness.run(&player, vec![Event::PointerMoved(over), wheel(-300.0)]);
    }
    harness.run(&player, Vec::new());
    let rows: Vec<Rect> = player
        .play_order()
        .iter()
        .filter_map(|s| {
            harness
                .frame
                .chrome_rect(&ChromeButton::DrawerRow(s.id.clone()))
        })
        .collect();
    assert!(!rows.is_empty());
    assert!(
        rows.iter().any(|r| r.area() == 0.0),
        "the long outline should have scrolled rows out of view"
    );
    for row in rows.iter().filter(|r| r.area() > 0.0) {
        assert!(
            row.top() >= tabs.bottom() - 0.5,
            "row {row:?} reaches over the tabs {tabs:?}"
        );
    }
}

#[test]
fn drawer_reports_its_settled_place_and_ignores_row_clicks_while_sliding() {
    let (_course, player) = hello();
    let mut harness = Harness::new(1280.0, 720.0);
    harness.run(&player, Vec::new());
    harness.run(&player, vec![key(egui::Key::M)]);
    let sliding_drawer = harness.frame.drawer_rect().expect("drawer is painted");
    let sliding_close = harness
        .frame
        .chrome_rect(&ChromeButton::DrawerClose)
        .expect("close button");
    let target = player
        .play_order()
        .get(1)
        .map(|s| s.id.clone())
        .expect("a second slide");
    let row = harness
        .frame
        .chrome_rect(&ChromeButton::DrawerRow(target.clone()))
        .expect("row rect");
    // Still sliding: a click on the row's reported place does nothing.
    let output = harness.run(&player, click(row.center()));
    assert!(output.inputs.is_empty(), "{:?}", output.inputs);
    for _ in 0..4 {
        harness.run(&player, Vec::new());
    }
    let settled_drawer = harness.frame.drawer_rect().expect("drawer is painted");
    let settled_close = harness
        .frame
        .chrome_rect(&ChromeButton::DrawerClose)
        .expect("close button");
    assert!((sliding_drawer.left() - settled_drawer.left()).abs() < 0.5);
    assert!((sliding_close.center().x - settled_close.center().x).abs() < 0.5);
    let settled_row = harness
        .frame
        .chrome_rect(&ChromeButton::DrawerRow(target.clone()))
        .expect("row rect");
    assert!((settled_row.center().x - row.center().x).abs() < 0.5);
    // Settled: the same click goes to the slide.
    let output = harness.run(&player, click(settled_row.center()));
    assert!(
        output
            .inputs
            .iter()
            .any(|i| matches!(i, Input::GoTo { slide } if *slide == target)),
        "{:?}",
        output.inputs
    );
}

#[test]
fn start_over_button_in_menu_sets_restart_when_enabled() {
    let (_course, player) = hello();
    let mut harness = Harness::new(1280.0, 720.0);
    harness.frame.open_drawer();
    harness.run(&player, Vec::new());
    assert!(
        harness
            .frame
            .chrome_rect(&ChromeButton::StartOver)
            .is_none(),
        "hidden when can_restart is false"
    );

    harness.frame.can_restart = true;
    harness.run(&player, Vec::new());
    let btn = harness
        .frame
        .chrome_rect(&ChromeButton::StartOver)
        .expect("start over button");
    let output = harness.run(&player, click(btn.center()));
    assert!(output.restart, "clicking Start over requests restart");
    assert!(!harness.frame.drawer_open(), "drawer closes on Start over");
}

#[test]
fn a15_play_at_end_wait_for_click_vs_auto_advance_off() {
    // 1. Slide with no auto-advance (waits for click)
    let (_course, mut player) = hello();
    let initial_slide = player.state().slide;
    let dur = player.seek_info().duration_ms;
    player.handle(&Input::Tick { ms: dur });
    assert!(player.state().base.ended);

    let mut harness = Harness::new(1280.0, 720.0);
    harness.run(&player, Vec::new());
    let play_rect = control(&harness.frame, ChromeButton::PlayPause);
    assert_eq!(
        text_in_rect(&harness.last_shapes, play_rect).as_deref(),
        Some(crate::Icon::Play.codepoint().to_string().as_str())
    );
    let out = harness.run(&player, click(play_rect.center()));
    assert_eq!(out.inputs, vec![Input::PlayerSeek { ms: 0 }]);
    apply_inputs(&mut player, &out.inputs);
    assert_eq!(player.state().slide, initial_slide);
    assert_eq!(player.seek_info().elapsed_ms, 0);

    // 2. Slide with explicit auto-advance trigger disabled / no timeline_ends jump
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "a15-wait",
        "title": "A15 Wait",
        "scenes": [{
            "id": "sc1",
            "title": "Scene",
            "slides": [
                { "id": "s1", "title": "S1", "duration_ms": 2000, "objects": [] },
                { "id": "s2", "title": "S2", "duration_ms": 2000, "objects": [] }
            ]
        }]
    })
    .to_string();
    let course = Course::from_json(&json).unwrap();
    let mut p2 = Player::new(&course).unwrap();
    p2.handle(&Input::Start);
    p2.handle(&Input::Tick { ms: 2000 });
    assert_eq!(p2.state().slide, 0);
    assert!(p2.state().base.ended);
    let mut h2 = Harness::new(1280.0, 720.0);
    h2.run(&p2, Vec::new());
    let pr2 = control(&h2.frame, ChromeButton::PlayPause);
    let out2 = h2.run(&p2, click(pr2.center()));
    assert_eq!(out2.inputs, vec![Input::PlayerSeek { ms: 0 }]);
    apply_inputs(&mut p2, &out2.inputs);
    assert_eq!(p2.state().slide, 0);
    assert_eq!(p2.seek_info().elapsed_ms, 0);
}

#[test]
fn a15_play_at_end_last_slide_of_course() {
    let (course, mut player) = hello();
    let last_idx = course.slides().count() - 1;
    let last_id = course.slides().last().unwrap().id.clone();
    player.handle(&Input::GoTo { slide: last_id });
    assert_eq!(player.state().slide, last_idx);
    let dur = player.seek_info().duration_ms;
    player.handle(&Input::Tick { ms: dur });
    assert!(player.state().base.ended);

    let mut harness = Harness::new(1280.0, 720.0);
    harness.run(&player, Vec::new());
    let play_rect = control(&harness.frame, ChromeButton::PlayPause);
    assert_eq!(
        text_in_rect(&harness.last_shapes, play_rect).as_deref(),
        Some(crate::Icon::Play.codepoint().to_string().as_str())
    );
    let out = harness.run(&player, click(play_rect.center()));
    assert_eq!(out.inputs, vec![Input::PlayerSeek { ms: 0 }]);
    apply_inputs(&mut player, &out.inputs);
    assert_eq!(player.state().slide, last_idx);
    assert_eq!(player.seek_info().elapsed_ms, 0);
    assert!(!player.state().base.ended);
}

#[test]
fn a15_play_after_seek_past_end() {
    let (_course, mut player) = hello();
    let initial_slide = player.state().slide;
    let dur = player.seek_info().duration_ms;
    player.handle(&Input::PlayerSeek {
        ms: u64::from(dur).saturating_add(5_000),
    });
    assert!(player.seek_info().elapsed_ms >= dur);

    let mut harness = Harness::new(1280.0, 720.0);
    harness.run(&player, Vec::new());
    let play_rect = control(&harness.frame, ChromeButton::PlayPause);
    assert_eq!(
        text_in_rect(&harness.last_shapes, play_rect).as_deref(),
        Some(crate::Icon::Play.codepoint().to_string().as_str())
    );
    let out = harness.run(&player, click(play_rect.center()));
    assert_eq!(out.inputs, vec![Input::PlayerSeek { ms: 0 }]);
    apply_inputs(&mut player, &out.inputs);
    assert_eq!(player.state().slide, initial_slide);
    assert_eq!(player.seek_info().elapsed_ms, 0);
}

#[test]
fn a15_play_at_end_with_slide_layer_open() {
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "a15-layer",
        "title": "A15 Layer",
        "scenes": [{
            "id": "sc1",
            "title": "Scene",
            "slides": [{
                "id": "s1",
                "title": "S1",
                "duration_ms": 2000,
                "objects": [],
                "layers": [{
                    "id": "l1",
                    "name": "L1",
                    "duration_ms": 1500,
                    "pause_base_timeline": false,
                    "objects": []
                }],
                "triggers": [{
                    "when": { "event": "timeline_starts" },
                    "action": { "action": "show_layer", "layer": "l1" }
                }]
            }]
        }]
    })
    .to_string();
    let course = Course::from_json(&json).unwrap();
    let mut player = Player::new(&course).unwrap();
    player.handle(&Input::Start);
    assert!(player.state().visible_layers.contains(&"l1".to_owned()));
    player.handle(&Input::Tick { ms: 2000 });
    assert!(player.state().base.ended);

    let mut harness = Harness::new(1280.0, 720.0);
    harness.run(&player, Vec::new());
    let play_rect = control(&harness.frame, ChromeButton::PlayPause);
    assert_eq!(
        text_in_rect(&harness.last_shapes, play_rect).as_deref(),
        Some(crate::Icon::Play.codepoint().to_string().as_str())
    );
    let out = harness.run(&player, click(play_rect.center()));
    assert_eq!(out.inputs, vec![Input::PlayerSeek { ms: 0 }]);
    apply_inputs(&mut player, &out.inputs);
    assert_eq!(player.state().slide, 0);
    assert_eq!(player.seek_info().elapsed_ms, 0);
}

#[test]
fn a15_keyboard_space_and_enter_at_end_replay_same_slide() {
    for key in [egui::Key::Space, egui::Key::Enter] {
        let json = serde_json::json!({
            "schema_version": 1,
            "id": "a15-keys",
            "title": "A15 Keys",
            "scenes": [{
                "id": "sc1",
                "title": "Scene",
                "slides": [{ "id": "s1", "title": "S1", "duration_ms": 2000, "objects": [] }]
            }]
        })
        .to_string();
        let course = Course::from_json(&json).unwrap();
        let mut player = Player::new(&course).unwrap();
        player.handle(&Input::Start);
        let initial_slide = player.state().slide;
        let dur = player.seek_info().duration_ms;
        player.handle(&Input::Tick { ms: dur });
        assert!(player.state().base.ended);

        let mut harness = Harness::new(1280.0, 720.0);
        harness.run(&player, Vec::new());
        // First press turns on the keyboard focus ring on PlayPause.
        let _ = harness.run(
            &player,
            vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        let out = harness.run(
            &player,
            vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert_eq!(
            out.inputs,
            vec![Input::PlayerSeek { ms: 0 }],
            "key {key:?} at end of slide must replay from 0"
        );
        apply_inputs(&mut player, &out.inputs);
        assert_eq!(player.state().slide, initial_slide);
        assert_eq!(player.seek_info().elapsed_ms, 0);
    }
}

#[test]
fn a15_separate_replay_button_still_works_at_end() {
    let (_course, mut player) = hello();
    let initial_slide = player.state().slide;
    let dur = player.seek_info().duration_ms;
    player.handle(&Input::Tick { ms: dur });
    assert!(player.state().base.ended);

    let mut harness = Harness::new(1280.0, 720.0);
    harness.run(&player, Vec::new());
    let replay_rect = control(&harness.frame, ChromeButton::Replay);
    let out = harness.run(&player, click(replay_rect.center()));
    assert_eq!(out.inputs, vec![Input::PlayerSeek { ms: 0 }]);
    apply_inputs(&mut player, &out.inputs);
    assert_eq!(player.state().slide, initial_slide);
    assert_eq!(player.seek_info().elapsed_ms, 0);
}

struct AreaHarness {
    ctx: egui::Context,
    frame: PlayerFrame,
    view: SlideView,
    screen_size: Vec2,
    player_area: Rect,
    time: f64,
}

impl AreaHarness {
    fn new(screen_size: Vec2, player_area: Rect) -> Self {
        let ctx = egui::Context::default();
        crate::install_fonts(&ctx);
        Self {
            ctx,
            frame: PlayerFrame::new(),
            view: SlideView::new(),
            screen_size,
            player_area,
            time: 0.1,
        }
    }

    fn run(
        &mut self,
        player: &Player<'_>,
        dt: f64,
        events: Vec<Event>,
    ) -> (FrameOutput, Vec<egui::epaint::ClippedShape>) {
        self.time += dt;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), self.screen_size)),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        let mut output = FrameOutput::default();
        let frame = &mut self.frame;
        let view = &mut self.view;
        let area = self.player_area;
        let full = self.ctx.run_ui(raw, |ui| {
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(area));
            child.set_clip_rect(area);
            output = frame.show(&mut child, player, view, None);
        });
        let shapes = full.shapes.clone();
        full.drop_without_applying_deltas();
        (output, shapes)
    }
}

fn assert_shapes_within(shapes: &[egui::epaint::ClippedShape], bounds: Rect, phase: &str) {
    let drawer_bg = egui::Color32::from_rgb(0x2F, 0x2F, 0x33);
    let mut saw_drawer_bg = false;
    for clipped in shapes {
        assert!(
            clipped.clip_rect.left() >= bounds.left() - 0.5
                && clipped.clip_rect.right() <= bounds.right() + 0.5
                && clipped.clip_rect.top() >= bounds.top() - 0.5
                && clipped.clip_rect.bottom() <= bounds.bottom() + 0.5,
            "{phase}: shape clip_rect {:?} escaped player area {:?}",
            clipped.clip_rect,
            bounds
        );
        if let egui::epaint::Shape::Rect(rect_shape) = &clipped.shape
            && rect_shape.fill == drawer_bg
        {
            saw_drawer_bg = true;
            assert!(
                rect_shape.rect.left() >= bounds.left() - 0.5
                    && rect_shape.rect.right() <= bounds.right() + 0.5,
                "{phase}: drawer background rect {:?} escaped player area {:?}",
                rect_shape.rect,
                bounds
            );
        }
    }
    assert!(
        saw_drawer_bg,
        "{phase}: expected drawer background to be painted"
    );
}

#[test]
fn drawer_clips_to_player_area_at_offset_rect_during_entire_animation() {
    let mut course = Course::from_json(HELLO_JSON).expect("valid course");
    course.resources.push(coursecraft_course::Resource {
        id: "res-1".to_owned(),
        title: "Course Guide".to_owned(),
        target: coursecraft_course::ResourceTarget::Url {
            url: "https://example.com".to_owned(),
        },
    });
    course.glossary.push(coursecraft_course::GlossaryEntry {
        id: "gl-1".to_owned(),
        term: "SCORM".to_owned(),
        definition: "Sharable Content Object Reference Model".to_owned(),
    });
    course.player.show_menu = true;
    course.player.show_transcript = true;
    let boxed: &'static Course = Box::leak(Box::new(course));
    let mut player = Player::new(boxed).expect("valid player");
    player.handle(&Input::Start);

    for (screen, area) in [
        (
            vec2(1280.0, 900.0),
            Rect::from_min_size(pos2(180.0, 60.0), vec2(920.0, 580.0)),
        ),
        (
            vec2(1600.0, 1000.0),
            Rect::from_min_size(pos2(320.0, 120.0), vec2(960.0, 640.0)),
        ),
        (
            vec2(1000.0, 700.0),
            Rect::from_min_size(pos2(240.0, 40.0), vec2(520.0, 420.0)),
        ),
    ] {
        for tab in [
            crate::drawer::DrawerTab::Menu,
            crate::drawer::DrawerTab::Resources,
            crate::drawer::DrawerTab::Glossary,
            crate::drawer::DrawerTab::Transcript,
        ] {
            let mut area_harness = AreaHarness::new(screen, area);

            // Initialize frame.
            area_harness.run(&player, 0.0, Vec::new());

            // First animation frame (dt = 0.02s -> progress ~ 0.077).
            area_harness.run(&player, 0.02, vec![key(egui::Key::M)]);
            let (_, shapes_first) = area_harness.run(&player, 0.02, Vec::new());
            assert_shapes_within(
                &shapes_first,
                area,
                &format!("first frame {tab:?} {area:?}"),
            );

            // Middle of animation (dt = 0.09s -> progress ~ 0.50).
            let (_, shapes_mid) = area_harness.run(&player, 0.09, Vec::new());
            assert_shapes_within(&shapes_mid, area, &format!("mid frame {tab:?} {area:?}"));

            // End of animation (settled).
            let (_, shapes_end) = area_harness.run(&player, 0.25, Vec::new());
            assert_shapes_within(&shapes_end, area, &format!("end frame {tab:?} {area:?}"));
            let settled = area_harness
                .frame
                .drawer_rect()
                .expect("drawer rect at end");
            assert!(
                (settled.left() - area.left()).abs() < 0.5,
                "settled drawer left {} should equal area left {}",
                settled.left(),
                area.left()
            );

            // Switch tab while open and verify it stays clipped.
            if tab != crate::drawer::DrawerTab::Menu {
                let tab_rect = area_harness
                    .frame
                    .chrome_rect(&ChromeButton::DrawerTab(tab))
                    .expect("tab rect");
                let (_, shapes_tab) = area_harness.run(&player, 0.05, click(tab_rect.center()));
                assert_shapes_within(&shapes_tab, area, &format!("tab {tab:?} {area:?}"));
            }
        }
    }
}

#[test]
fn drawer_clips_to_player_area_at_letterboxed_and_fullscreen_sizes() {
    let (_course, player) = hello();
    for size in [
        vec2(1600.0, 720.0), // wider than 16:9 slide aspect
        vec2(800.0, 900.0),  // taller than 16:9 slide aspect
        vec2(1280.0, 720.0), // fullscreen 16:9
    ] {
        let area = Rect::from_min_size(pos2(0.0, 0.0), size);
        let mut area_harness = AreaHarness::new(size, area);

        area_harness.run(&player, 0.0, Vec::new());
        let (_, shapes_first) = area_harness.run(&player, 0.02, vec![key(egui::Key::M)]);
        assert_shapes_within(&shapes_first, area, &format!("first frame {size:?}"));

        let (_, shapes_mid) = area_harness.run(&player, 0.10, Vec::new());
        assert_shapes_within(&shapes_mid, area, &format!("mid frame {size:?}"));

        let (_, shapes_end) = area_harness.run(&player, 0.25, Vec::new());
        assert_shapes_within(&shapes_end, area, &format!("end frame {size:?}"));
        let settled = area_harness.frame.drawer_rect().expect("drawer rect");
        assert!((settled.left() - area.left()).abs() < 0.5);
    }
}

#[test]
fn reserved_strip_never_overlaps_slide_across_aspects_and_sizes() {
    use coursecraft_course::{ObjectKind, Rect as CourseRect, Size};

    let aspects = [
        ("16:9", Size { w: 1280, h: 720 }),
        ("4:3", Size { w: 1024, h: 768 }),
        ("portrait", Size { w: 720, h: 1280 }),
    ];

    for (label, slide_size) in aspects {
        let mut course = Course::from_json(HELLO_JSON).expect("valid course");
        course.slide_size = slide_size;
        let mut video_obj = course.scenes[0].slides[0].objects[0].clone();
        video_obj.id = "vid-1".to_owned();
        video_obj.name = "Video".to_owned();
        video_obj.frame = CourseRect {
            x: 0.0,
            y: (slide_size.h as f32) - 180.0,
            w: 320.0,
            h: 180.0,
        };
        video_obj.kind = ObjectKind::Video {
            src: "clip.mp4".to_owned(),
            poster: None,
            autoplay: true,
            captions: Some("clip.vtt".to_owned()),
            volume: 1.0,
            trim: None,
            cues: Vec::new(),
        };
        course.scenes[0].slides[0].objects.push(video_obj);
        let boxed: &'static Course = Box::leak(Box::new(course));
        let mut player = Player::new(boxed).expect("valid player");
        player.handle(&Input::Start);

        let expected_aspect = slide_size.w as f32 / slide_size.h as f32;
        let cases: &[(&str, Vec2, bool)] = &[
            ("wider", vec2(1600.0, 500.0), false),
            ("taller", vec2(420.0, 900.0), false),
            (
                "exact",
                vec2(slide_size.w as f32, slide_size.h as f32),
                false,
            ),
            ("tiny", vec2(240.0, 160.0), false),
            ("fullscreen", vec2(1280.0, 720.0), true),
        ];

        for &(case_name, size, fullscreen) in cases {
            let mut harness = Harness::new(size.x, size.y);
            harness.frame.can_fullscreen = true;
            harness.frame.fullscreen = fullscreen;
            harness.frame.captions_on = true;
            harness
                .frame
                .set_caption_frame(Some(crate::media_driver::CaptionFrame {
                    object: "vid-1".to_owned(),
                    kind: coursecraft_playback::MediaKind::Audio,
                    text: "Sample caption line".to_owned(),
                    rect: None,
                    dom: false,
                }));

            // Advance past 2.5 s idle to verify default reserved strip does not auto-hide.
            harness.time = 0.0;
            harness.run(&player, Vec::new());
            harness.time = 4.0;
            let output = harness.run(&player, Vec::new());

            let player_rect = Rect::from_min_size(pos2(0.0, 0.0), size);
            let bar_rect = harness
                .frame
                .strip_rect()
                .unwrap_or_else(|| panic!("{label}/{case_name}: strip_rect missing"));
            let slide_rect = output.slide_rect;

            assert!(
                slide_rect.is_positive(),
                "{label}/{case_name}: slide_rect must be positive, got {slide_rect:?}"
            );
            assert!(
                slide_rect.left() >= player_rect.left() - 0.5
                    && slide_rect.right() <= player_rect.right() + 0.5
                    && slide_rect.top() >= player_rect.top() - 0.5
                    && slide_rect.bottom() <= player_rect.bottom() + 0.5,
                "{label}/{case_name}: slide_rect {slide_rect:?} escaped {player_rect:?}"
            );
            assert!(
                !slide_rect.intersects(bar_rect),
                "{label}/{case_name}: slide_rect {slide_rect:?} intersects bar_rect {bar_rect:?}"
            );
            assert!(
                slide_rect.bottom() < bar_rect.top(),
                "{label}/{case_name}: slide bottom {} must be above bar top {}",
                slide_rect.bottom(),
                bar_rect.top()
            );

            let actual_aspect = slide_rect.width() / slide_rect.height();
            assert!(
                (actual_aspect - expected_aspect).abs() < 0.02,
                "{label}/{case_name}: aspect {actual_aspect} != {expected_aspect}"
            );

            let transform = harness.view.last_transform().expect("transform");
            let vid_item = harness
                .view
                .last_items()
                .iter()
                .find(|item| matches!(item.hit, crate::Hit::Object(ref id) if id == "vid-1"))
                .expect("video item");
            let vid_screen = transform.to_screen_rect(vid_item.rect);
            assert!(
                vid_screen.left() >= slide_rect.left() - 0.5
                    && vid_screen.right() <= slide_rect.right() + 0.5
                    && vid_screen.top() >= slide_rect.top() - 0.5
                    && vid_screen.bottom() <= slide_rect.bottom() + 0.5,
                "{label}/{case_name}: video rect {vid_screen:?} escaped slide_rect {slide_rect:?}"
            );
            assert!(
                !vid_screen.intersects(bar_rect),
                "{label}/{case_name}: video rect {vid_screen:?} intersects bar_rect {bar_rect:?}"
            );
        }
    }
}

#[test]
fn rendered_bottom_edge_textbox_is_fully_visible_above_bar() {
    let mut course = Course::from_json(HELLO_JSON).expect("valid course");
    let mut box_obj = course.scenes[0].slides[0].objects[0].clone();
    box_obj.id = "bottom-box".to_owned();
    box_obj.name = "Bottom Box".to_owned();
    box_obj.frame = coursecraft_course::Rect {
        x: 240.0,
        y: 640.0,
        w: 800.0,
        h: 80.0,
    };
    box_obj.kind = coursecraft_course::ObjectKind::Shape {
        shape: coursecraft_course::ShapeKind::Rectangle,
        fill: coursecraft_course::Color("#E02020".to_owned()),
        fill_style: None,
        outline: None,
        text: None,
        adjust: Vec::new(),
    };
    course.scenes[0].slides[0].objects.push(box_obj);
    let boxed: &'static Course = Box::leak(Box::new(course));
    let mut player = Player::new(boxed).expect("valid player");
    player.handle(&Input::Start);

    struct EmptyMedia;
    impl crate::media::MediaSource for EmptyMedia {
        fn get(&self, _src: &str) -> crate::media::MediaFetch {
            crate::media::MediaFetch::Missing
        }
    }

    let mut frame = PlayerFrame::new();
    frame.reduced_motion = true;
    let mut view = SlideView::new();
    view.set_reduced_motion(true);
    let mut cache = crate::media::MediaCache::new();
    let media = EmptyMedia;
    let img = crate::offscreen::render_ui(
        1280,
        720,
        1.0,
        |_ctx| {},
        |ctx| {
            ctx.root_ui(|ui| {
                frame.show(ui, &player, &mut view, Some((&mut cache, &media)));
            });
        },
    )
    .expect("render");

    let slide_rect = frame.slide_rect().expect("slide_rect");
    let bar_rect = frame.strip_rect().expect("strip_rect");
    assert!(slide_rect.bottom() < bar_rect.top());

    // Sample near the very bottom edge of the bottom-edge box inside slide_rect.
    let sample_x = slide_rect.center().x as u32;
    let sample_y = (slide_rect.bottom() - 4.0) as u32;
    let idx = ((sample_y as usize) * (img.width as usize) + sample_x as usize) * 4;
    let (r, g, b) = (img.pixels[idx], img.pixels[idx + 1], img.pixels[idx + 2]);
    assert!(
        r > 180 && g < 80 && b < 80,
        "bottom-edge box should be fully visible above the bar, got ({r}, {g}, {b}) at ({sample_x}, {sample_y})"
    );
}

#[test]
fn overlay_strip_explicit_opt_in_auto_hides_and_uses_full_area() {
    let (_course, player) = hello();
    let mut harness = Harness::new(1280.0, 720.0);
    harness.frame.overlay_strip = true;
    let out = harness.run(&player, Vec::new());
    assert_eq!(
        out.slide_rect,
        Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 720.0))
    );
    harness.frame.hide_strip();
    harness.run(&player, Vec::new());
    assert!(harness.frame.strip_rect().is_none());
}

fn find_tooltip_popup_rects(shapes: &[egui::epaint::ClippedShape]) -> Vec<Rect> {
    fn collect(shape: &egui::epaint::Shape, out: &mut Vec<Rect>) {
        match shape {
            egui::epaint::Shape::Rect(r) => {
                if r.fill != egui::Color32::TRANSPARENT {
                    out.push(r.rect);
                }
            }
            egui::epaint::Shape::Vec(v) => {
                for s in v {
                    collect(s, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    for clipped in shapes {
        collect(&clipped.shape, &mut out);
    }
    out
}

fn has_painted_text(shapes: &[egui::epaint::ClippedShape], needle: &str) -> bool {
    fn walk(shape: &egui::epaint::Shape, needle: &str) -> bool {
        match shape {
            egui::epaint::Shape::Text(t) => t.galley.text().contains(needle),
            egui::epaint::Shape::Vec(v) => v.iter().any(|s| walk(s, needle)),
            _ => false,
        }
    }
    shapes.iter().any(|c| walk(&c.shape, needle))
}

#[test]
fn speed_popup_clamps_to_player_area_and_closes_on_escape_regardless_of_pointer() {
    let mut course = Course::from_json(HELLO_JSON).expect("valid course");
    // Hide fullscreen and keep 1 slide so speed sits right at the right edge of the strip.
    course.player.show_fullscreen = false;
    course.scenes[0].slides.truncate(1);
    let boxed: &'static Course = Box::leak(Box::new(course));
    let mut player = Player::new(boxed).expect("valid player");
    player.handle(&Input::Start);

    // Narrow inset player area (520x600 at (100, 50)) so an unclamped centered
    // 72pt speed popup would spill past the right edge of `full`.
    let full = Rect::from_min_size(pos2(100.0, 50.0), vec2(520.0, 600.0));
    let mut harness = Harness::new(800.0, 800.0);
    harness.frame.can_speed = true;
    let run_inset = |h: &mut Harness, events: Vec<Event>| {
        h.time += 0.1;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), h.size)),
            time: Some(h.time),
            events,
            ..Default::default()
        };
        let frame = &mut h.frame;
        let view = &mut h.view;
        let full_out = h.ctx.run_ui(raw, |ui| {
            ui.scope_builder(egui::UiBuilder::new().max_rect(full), |ui| {
                frame.show(ui, &player, view, None);
            });
        });
        h.last_shapes = full_out.shapes.clone();
        full_out.drop_without_applying_deltas();
    };

    run_inset(&mut harness, Vec::new());
    run_inset(&mut harness, Vec::new());
    let speed_rect = harness
        .frame
        .chrome_rect(&ChromeButton::Speed)
        .expect("speed button");
    run_inset(&mut harness, click(speed_rect.center()));
    run_inset(&mut harness, Vec::new());
    assert!(
        has_painted_text(&harness.last_shapes, "2×"),
        "speed popup should be open"
    );

    // Check every painted rect on the speed popup (the 72pt-wide popup background)
    // stays completely inside `full`.
    let popup_rects: Vec<Rect> = find_tooltip_popup_rects(&harness.last_shapes)
        .into_iter()
        .filter(|r| (r.width() - 72.0).abs() < 0.5 && r.height() > 100.0)
        .collect();
    assert!(!popup_rects.is_empty(), "expected speed popup rect");
    for r in &popup_rects {
        assert!(
            full.contains_rect(*r),
            "speed popup {r:?} must stay inside player area {full:?}"
        );
    }

    // Hovering INSIDE the speed popup and pressing Escape must close it (SUS-23).
    let inside_popup = popup_rects[0].center();
    run_inset(
        &mut harness,
        vec![Event::PointerMoved(inside_popup), key(egui::Key::Escape)],
    );
    run_inset(&mut harness, Vec::new());
    assert!(
        !has_painted_text(&harness.last_shapes, "2×"),
        "Escape while hovering speed popup must close it"
    );

    // Reopen and press Escape with NO pointer position (keyboard-only, SUS-23).
    run_inset(&mut harness, click(speed_rect.center()));
    run_inset(&mut harness, Vec::new());
    assert!(has_painted_text(&harness.last_shapes, "2×"));
    run_inset(
        &mut harness,
        vec![Event::PointerGone, key(egui::Key::Escape)],
    );
    run_inset(&mut harness, Vec::new());
    assert!(
        !has_painted_text(&harness.last_shapes, "2×"),
        "Escape with no pointer position must close speed popup"
    );
}

#[test]
fn fullscreen_button_tooltip_reflects_fullscreen_state() {
    let (_course, player) = hello();
    let mut harness = Harness::new(1280.0, 720.0);
    harness
        .ctx
        .all_styles_mut(|s| s.interaction.tooltip_delay = 0.0);
    harness.frame.can_fullscreen = true;
    harness.run(&player, Vec::new());

    let fs_center = harness
        .frame
        .chrome_rect(&ChromeButton::Fullscreen)
        .expect("fullscreen button")
        .center();

    // Windowed: tooltip should read "Full screen".
    harness.run(&player, vec![Event::PointerMoved(fs_center)]);
    harness.time += 1.0;
    harness.run(&player, vec![Event::PointerMoved(fs_center)]);
    assert!(
        has_painted_text(&harness.last_shapes, "Full screen"),
        "expected 'Full screen' tooltip when not fullscreen"
    );
    assert!(
        !has_painted_text(&harness.last_shapes, "Exit full screen"),
        "unexpected 'Exit full screen' tooltip when not fullscreen"
    );

    // Fullscreen: tooltip should read "Exit full screen".
    harness.frame.fullscreen = true;
    harness.time += 1.0;
    harness.run(&player, vec![Event::PointerMoved(fs_center)]);
    assert!(
        has_painted_text(&harness.last_shapes, "Exit full screen"),
        "expected 'Exit full screen' tooltip when fullscreen"
    );
}

fn collect_text_shapes_in_rect(
    shapes: &[egui::epaint::ClippedShape],
    container: Rect,
) -> Vec<(String, Rect, egui::Color32)> {
    fn walk(
        shape: &egui::epaint::Shape,
        clip: Rect,
        container: Rect,
        out: &mut Vec<(String, Rect, egui::Color32)>,
    ) {
        match shape {
            egui::epaint::Shape::Text(t) => {
                let r = t.visual_bounding_rect();
                if r.is_positive()
                    && clip.intersects(container)
                    && container.intersects(r)
                    && clip.right() <= container.right() + 2.0
                {
                    let color = if t.override_text_color.is_some() {
                        t.override_text_color.unwrap_or(egui::Color32::WHITE)
                    } else if let Some(sec) = t.galley.job.sections.first() {
                        sec.format.color
                    } else {
                        egui::Color32::WHITE
                    };
                    out.push((t.galley.text().to_string(), r, color));
                }
            }
            egui::epaint::Shape::Vec(v) => {
                for s in v {
                    walk(s, clip, container, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    for clipped in shapes {
        walk(&clipped.shape, clipped.clip_rect, container, &mut out);
    }
    out
}

#[test]
fn t14_1_responsive_player_strip_across_six_browser_sizes() {
    let (_course, player) = hello();
    let sizes = [
        (320.0, 568.0),
        (390.0, 844.0),
        (844.0, 390.0),
        (768.0, 1024.0),
        (1280.0, 720.0),
        (1920.0, 1080.0),
    ];
    for (w, h) in sizes {
        let mut harness = Harness::new(w, h);
        harness.frame.can_fullscreen = true;
        harness.frame.can_speed = true;
        harness.run(&player, Vec::new());

        let strip = harness
            .frame
            .strip_rect()
            .unwrap_or_else(|| panic!("strip rect missing at {w}x{h}"));
        let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(w, h));
        assert!(
            screen.expand(1.5).contains_rect(strip),
            "strip {strip:?} overflows viewport {w}x{h}"
        );

        let play = harness
            .frame
            .chrome_rect(&ChromeButton::PlayPause)
            .unwrap_or_else(|| panic!("play button missing at {w}x{h}"));
        assert!(
            play.width() >= 36.0 && play.height() >= 36.0,
            "play button {play:?} < 36x36 at {w}x{h}"
        );
        let next = harness
            .frame
            .chrome_rect(&ChromeButton::Next)
            .unwrap_or_else(|| panic!("next button missing at {w}x{h}"));
        assert!(
            next.width() >= 36.0 && next.height() >= 36.0,
            "next button {next:?} < 36x36 at {w}x{h}"
        );

        // No chrome buttons on the strip may overlap each other.
        let strip_buttons = controls(&harness.frame);
        let strip_buttons: Vec<(ChromeButton, Rect)> = strip_buttons
            .into_iter()
            .filter(|(_, r)| strip.intersects(*r) && r.width() > 0.0 && r.height() > 0.0)
            .collect();
        for i in 0..strip_buttons.len() {
            for j in (i + 1)..strip_buttons.len() {
                let inter = strip_buttons[i].1.intersect(strip_buttons[j].1);
                assert!(
                    !inter.is_positive() || (inter.width() <= 1.0 && inter.height() <= 1.0),
                    "strip controls {:?} and {:?} overlap at {w}x{h}: {inter:?}",
                    strip_buttons[i].0,
                    strip_buttons[j].0
                );
            }
        }
    }
}

#[test]
fn t14_3_drawer_long_titles_tabs_resources_and_glossary_truncate() {
    let mut course = Course::from_json(HELLO_JSON).expect("valid hello course");
    let long_100 = "VeryLongTitleForTestingDrawerTruncationAndNoOverlapWithIconsInThe320pxDrawerPanel_0123456789_012345";
    course.title = long_100.to_string();
    course.scenes[0].title = long_100.to_string();
    course.scenes[0].slides[0].title = long_100.to_string();
    course.resources.push(coursecraft_course::Resource {
        id: "res1".into(),
        title: long_100.to_string(),
        target: coursecraft_course::ResourceTarget::Url {
            url: "https://example.com".into(),
        },
    });
    course.glossary.push(coursecraft_course::GlossaryEntry {
        id: "gl1".into(),
        term: long_100.to_string(),
        definition: "Definition of long glossary term.".into(),
    });
    course.player.show_menu = true;
    course.player.show_transcript = true;
    course.scenes[0].slides[0].notes = "Speaker transcript text.".into();

    let mut player = Player::new(&course).expect("valid player");
    player.handle(&Input::Start);
    let mut harness = Harness::new(1280.0, 720.0);
    open_drawer(&mut harness, &player);

    let drawer = harness.frame.drawer_rect().expect("drawer rect");

    // 1. Menu tab: slide title must fit within drawer bounds (must not overflow right edge)
    let texts = collect_text_shapes_in_rect(&harness.last_shapes, drawer);
    for (text, rect, _) in &texts {
        assert!(
            rect.right() <= drawer.right() + 1.0,
            "drawer text '{text}' overflows drawer right edge: rect={rect:?} drawer={drawer:?}"
        );
    }

    // 2. Resources tab: resource title must not overlap ArrowSquareOut icon (drawer.right() - 28)
    let res_tab = harness
        .frame
        .chrome_rect(&ChromeButton::DrawerTab(super::DrawerTab::Resources))
        .expect("resources tab");
    harness.run(
        &player,
        vec![
            Event::PointerButton {
                pos: res_tab.center(),
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
            Event::PointerButton {
                pos: res_tab.center(),
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            },
        ],
    );
    harness.run(&player, Vec::new());
    let res_texts = collect_text_shapes_in_rect(&harness.last_shapes, drawer);
    for (text, rect, _) in &res_texts {
        if text.starts_with("VeryLong") {
            assert!(
                rect.right() <= drawer.right() - 32.0,
                "resource title '{text}' overlaps external icon: rect={rect:?} drawer={drawer:?}"
            );
        }
    }

    // 3. Glossary tab: glossary term must fit within drawer bounds
    let gl_tab = harness
        .frame
        .chrome_rect(&ChromeButton::DrawerTab(super::DrawerTab::Glossary))
        .expect("glossary tab");
    harness.run(
        &player,
        vec![
            Event::PointerButton {
                pos: gl_tab.center(),
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
            Event::PointerButton {
                pos: gl_tab.center(),
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            },
        ],
    );
    harness.run(&player, Vec::new());
    let gl_texts = collect_text_shapes_in_rect(&harness.last_shapes, drawer);
    for (text, rect, _) in &gl_texts {
        assert!(
            rect.right() <= drawer.right() + 1.0,
            "glossary text '{text}' overflows drawer right edge: rect={rect:?} drawer={drawer:?}"
        );
    }
}

#[test]
fn t14_6_player_chrome_contrast_under_bright_pastel_and_dark_themes() {
    let themes = [
        ("default", "#1B1B1B", "#F2F2F5", "#0A6CFF"),
        ("light", "#FFFFFF", "#1D1D1F", "#0A6CFF"),
        ("bright_yellow", "#1B1B1B", "#F2F2F5", "#FFD60A"),
    ];

    for (name, bar_hex, text_hex, accent_hex) in themes {
        let mut course = Course::from_json(HELLO_JSON).expect("valid hello course");
        course.theme.player.bar = coursecraft_course::Color(bar_hex.into());
        course.theme.player.bar_text = coursecraft_course::Color(text_hex.into());
        course.theme.accent = coursecraft_course::Color(accent_hex.into());
        course.player.show_menu = true;
        course.scenes[0].slides[0].duration_ms = 0; // triggers disabled seekbar time label
        course.resources.push(coursecraft_course::Resource {
            id: "r1".into(),
            title: "Guide".into(),
            target: coursecraft_course::ResourceTarget::Url {
                url: "https://example.com".into(),
            },
        });

        let mut player = Player::new(&course).expect("valid player");
        player.handle(&Input::Start);
        let mut harness = Harness::new(1280.0, 720.0);
        harness.frame.captions_on = true;
        open_drawer(&mut harness, &player);

        let accent = super::parse_color(&course.theme.accent).unwrap();
        let bar = super::parse_color(&course.theme.player.bar).unwrap();

        // 1. Selected drawer tab text contrast against `accent`
        let menu_tab = harness
            .frame
            .chrome_rect(&ChromeButton::DrawerTab(super::DrawerTab::Menu))
            .expect("menu tab");
        let tab_texts = collect_text_shapes_in_rect(&harness.last_shapes, menu_tab);
        for (label, _, fg) in &tab_texts {
            let cr = crate::design::contrast_ratio(*fg, accent);
            assert!(
                cr >= 4.5,
                "[{name}] selected drawer tab '{label}' fg={fg:?} on accent={accent:?} has contrast {cr:.2} < 4.5"
            );
        }

        // 2. Captions active icon contrast against `accent` (when present)
        if let Some(cap_rect) = harness.frame.chrome_rect(&ChromeButton::Captions) {
            let cap_texts = collect_text_shapes_in_rect(&harness.last_shapes, cap_rect);
            for (_, _, fg) in &cap_texts {
                let cr = crate::design::contrast_ratio(*fg, accent);
                assert!(
                    cr >= 3.0,
                    "[{name}] active captions icon fg={fg:?} on accent={accent:?} has contrast {cr:.2} < 3.0"
                );
            }
        }

        // 3. Disabled seekbar time label contrast against `bar`
        let strip = harness.frame.strip_rect().expect("strip");
        let strip_texts = collect_text_shapes_in_rect(&harness.last_shapes, strip);
        for (label, _, fg) in &strip_texts {
            if label.contains(':') {
                let [fr, fg_c, fb, fa] = fg.to_srgba_unmultiplied();
                let a = f32::from(fa) / 255.0;
                let blended = egui::Color32::from_rgb(
                    (f32::from(fr) * a + f32::from(bar.r()) * (1.0 - a)).round() as u8,
                    (f32::from(fg_c) * a + f32::from(bar.g()) * (1.0 - a)).round() as u8,
                    (f32::from(fb) * a + f32::from(bar.b()) * (1.0 - a)).round() as u8,
                );
                let cr = crate::design::contrast_ratio(blended, bar);
                assert!(
                    cr >= 3.0,
                    "[{name}] disabled time '{label}' blended={blended:?} on bar={bar:?} has contrast {cr:.2} < 3.0"
                );
            }
        }
    }
}

#[test]
fn t14_7_banners_captions_and_lightbox_close_never_collide() {
    let (_course, player) = hello();
    for (w, h) in [(390.0, 844.0), (1280.0, 720.0)] {
        let mut harness = Harness::new(w, h);
        harness.frame.captions_on = true;
        harness
            .frame
            .set_caption_frame(Some(crate::media_driver::CaptionFrame {
                object: "vid-1".to_owned(),
                kind: coursecraft_playback::MediaKind::Audio,
                text: "Sample audio caption text for overlap testing.".into(),
                rect: None,
                dom: false,
            }));
        harness.frame.notice = Some("Session notice message".into());
        harness.run(&player, Vec::new());

        let strip = harness.frame.strip_rect().expect("strip");
        let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(w, h));
        // Captions and notice must stay inside screen and above strip
        let texts = collect_text_shapes_in_rect(&harness.last_shapes, screen);
        let caption_rect = texts
            .iter()
            .find(|(t, _, _)| t.contains("Sample audio caption"))
            .map(|(_, r, _)| *r);
        let notice_rect = texts
            .iter()
            .find(|(t, _, _)| t.contains("Session notice"))
            .map(|(_, r, _)| *r);
        if let Some(cr) = caption_rect {
            assert!(
                cr.bottom() <= strip.top() + 1.0,
                "caption {cr:?} overlaps strip {strip:?} at {w}x{h}"
            );
        }
        if let (Some(cr), Some(nr)) = (caption_rect, notice_rect) {
            assert!(
                !cr.intersects(nr),
                "caption {cr:?} overlaps notice {nr:?} at {w}x{h}"
            );
        }
    }
}

#[test]
fn bar_off_uses_full_frame_without_strip_and_ignores_arrow_keys_and_menu() {
    let mut course = Course::from_json(HELLO_JSON).expect("valid course");
    course.player.show_bar = false;
    let boxed: &'static Course = Box::leak(Box::new(course));
    let mut player = Player::new(boxed).expect("valid player");
    player.handle(&Input::Start);

    let mut harness = Harness::new(1280.0, 720.0);
    harness.run(&player, Vec::new());

    assert!(
        harness.frame.strip_rect().is_none(),
        "bar off must not draw or reserve a strip"
    );
    let full = Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 720.0));
    let expected_slide = super::slide_rect(full, &player);
    assert_eq!(harness.frame.slide_rect(), Some(expected_slide));

    let output = harness.run(&player, vec![key(egui::Key::ArrowRight)]);
    assert!(
        !output
            .inputs
            .iter()
            .any(|input| matches!(input, Input::Next | Input::Previous)),
        "arrow keys must not navigate when bar is off"
    );
    let output = harness.run(&player, vec![key(egui::Key::M)]);
    // With the menu off the key is free for a slide's `key_pressed` trigger.
    assert!(
        output
            .inputs
            .iter()
            .all(|input| matches!(input, Input::KeyPressed { .. })),
        "{:?}",
        output.inputs
    );
    assert!(!harness.frame.drawer_open());

    // The runtime refuses the learner's Next too, so no host can bypass the bar.
    let first_id = player.current_slide().expect("slide").id.clone();
    player.handle(&Input::Next);
    assert_eq!(player.current_slide().expect("slide").id, first_id);
    // Author navigation still moves.
    player.handle(&Input::Jump {
        slide: "s-tabs".into(),
    });
    assert_ne!(player.current_slide().expect("slide").id, first_id);
}

#[test]
fn slide_show_bar_override_wins_both_ways() {
    let mut course = Course::from_json(HELLO_JSON).expect("valid course");
    course.player.show_bar = true;
    course.slide_mut("s-intro").expect("intro").player =
        Some(coursecraft_course::SlidePlayerOverrides {
            show_bar: Some(false),
            ..Default::default()
        });
    let boxed: &'static Course = Box::leak(Box::new(course));
    let mut player = Player::new(boxed).expect("valid player");
    player.handle(&Input::Start);

    let mut harness = Harness::new(1280.0, 720.0);
    harness.run(&player, Vec::new());
    assert!(harness.frame.strip_rect().is_none());

    player.handle(&Input::Jump {
        slide: "s-tabs".into(),
    });
    harness.run(&player, Vec::new());
    assert!(harness.frame.strip_rect().is_some());

    // Now test course show_bar = false with per-slide show_bar = Some(true).
    let mut course2 = Course::from_json(HELLO_JSON).expect("valid course");
    course2.player.show_bar = false;
    course2.slide_mut("s-intro").expect("intro").player =
        Some(coursecraft_course::SlidePlayerOverrides {
            show_bar: Some(true),
            ..Default::default()
        });
    let boxed2: &'static Course = Box::leak(Box::new(course2));
    let mut player2 = Player::new(boxed2).expect("valid player");
    player2.handle(&Input::Start);

    let mut harness2 = Harness::new(1280.0, 720.0);
    harness2.run(&player2, Vec::new());
    assert!(harness2.frame.strip_rect().is_some());

    player2.handle(&Input::Jump {
        slide: "s-tabs".into(),
    });
    harness2.run(&player2, Vec::new());
    assert!(harness2.frame.strip_rect().is_none());
}

fn unstarted() -> Player<'static> {
    let course = Course::from_json(HELLO_JSON).expect("valid course");
    let boxed: &'static Course = Box::leak(Box::new(course));
    Player::new(boxed).expect("valid player")
}

#[test]
fn start_gate_hides_the_strip_and_centres_its_button() {
    let player = unstarted();
    let mut harness = Harness::new(1280.0, 720.0);
    harness.frame.awaiting_start = true;
    let output = harness.run(&player, Vec::new());
    assert!(!output.start);
    assert!(output.inputs.is_empty());
    assert!(
        harness.frame.strip_rect().is_none(),
        "the start gate hides the control strip"
    );
    let button = harness.frame.start_button_rect().expect("start button");
    let slide = harness.frame.slide_rect().expect("slide rect");
    assert!(slide.contains_rect(button));
    assert!((button.center().x - slide.center().x).abs() < 1.0);
}

#[test]
fn clicking_the_start_button_reports_start() {
    let player = unstarted();
    let mut harness = Harness::new(1280.0, 720.0);
    harness.frame.awaiting_start = true;
    harness.run(&player, Vec::new());
    let button = harness.frame.start_button_rect().expect("start button");
    let output = harness.run(&player, click(button.center()));
    assert!(output.start, "a click on Play starts the course");
    assert!(output.inputs.is_empty());
}

#[test]
fn enter_or_space_on_the_gate_reports_start() {
    for k in [egui::Key::Enter, egui::Key::Space] {
        let player = unstarted();
        let mut harness = Harness::new(1280.0, 720.0);
        harness.frame.awaiting_start = true;
        harness.run(&player, Vec::new());
        let output = harness.run(&player, vec![key(k)]);
        assert!(output.start, "{k:?} starts the course");
    }
}

#[test]
fn clicks_outside_the_start_button_do_nothing() {
    let player = unstarted();
    let mut harness = Harness::new(1280.0, 720.0);
    harness.frame.awaiting_start = true;
    harness.run(&player, Vec::new());
    let slide = harness.frame.slide_rect().expect("slide rect");
    let output = harness.run(&player, click(slide.min + vec2(20.0, 20.0)));
    assert!(!output.start);
    assert!(output.inputs.is_empty());
}

#[test]
fn no_gate_means_no_start_button() {
    let (_course, player) = hello();
    let mut harness = Harness::new(1280.0, 720.0);
    let output = harness.run(&player, vec![key(egui::Key::Enter)]);
    assert!(!output.start);
    assert!(harness.frame.start_button_rect().is_none());
}

#[test]
fn b09_start_gate_blocks_keyboard_shortcuts() {
    let player = unstarted();
    let mut harness = Harness::new(1280.0, 720.0);
    harness.frame.awaiting_start = true;
    harness.run(&player, Vec::new());

    for k in [
        egui::Key::C,
        egui::Key::M,
        egui::Key::ArrowRight,
        egui::Key::ArrowLeft,
    ] {
        let output = harness.run(&player, vec![key(k)]);
        assert!(!output.start, "{k:?} must not start the course");
        assert!(
            output.inputs.is_empty(),
            "{k:?} must not emit inputs behind the start gate: {:?}",
            output.inputs
        );
        assert!(
            !harness.frame.captions_on,
            "{k:?} must not toggle captions behind the start gate"
        );
        assert!(
            !harness.frame.drawer_open,
            "{k:?} must not open the drawer behind the start gate"
        );
    }
}

#[test]
fn b10_start_gate_wraps_long_label_on_narrow_screen() {
    let mut course = Course::from_json(HELLO_JSON).expect("valid course");
    course.strings.insert(
        "startCourse".to_owned(),
        "Start this interactive training module now".to_owned(),
    );
    let player = Player::new(&course).expect("player");
    let mut harness = Harness::new(320.0, 568.0);
    harness.frame.awaiting_start = true;
    harness.run(&player, Vec::new());

    let slide = harness.frame.slide_rect().expect("slide rect");
    let button = harness.frame.start_button_rect().expect("start button");
    assert!(
        slide.contains_rect(button),
        "start button {button:?} must stay completely inside slide {slide:?}"
    );
}

#[test]
fn m31_11_slider_value_handles_inverted_and_nan_bounds() {
    let normal = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(100.0, 20.0));
    let _ = super::slider_value(egui::Rect::NOTHING, 10.0, 100.0, 0.0, 0.0);
    let v = super::slider_value(normal, 50.0, 100.0, 10.0, 4.0);
    assert!(v.is_finite());
    let nan_bound = super::slider_value(normal, 50.0, f32::NAN, 10.0, 4.0);
    assert!(nan_bound.is_finite());
}

// ---- Neon player verification (N6, N18, N29, N30, N31, N34, N40) ----

fn neon_course(slides: serde_json::Value, variables: serde_json::Value) -> &'static Course {
    let course = Course::from_json(
        &serde_json::json!({
            "schema_version": 1,
            "id": "neon-frame",
            "title": "Neon Frame",
            "variables": variables,
            "quiz": { "passing_percent": 50 },
            "scenes": [{ "id": "sc", "title": "Scene", "slides": slides }]
        })
        .to_string(),
    )
    .expect("valid course");
    Box::leak(Box::new(course))
}

fn neon_player(course: &'static Course) -> Player<'static> {
    let mut player = Player::new(course).expect("valid player");
    player.handle(&Input::Start);
    player
}

#[test]
fn n6_tab_out_of_a_text_entry_commits_the_typed_text() {
    let course = neon_course(
        serde_json::json!([{ "id": "s1", "title": "S", "objects": [
            { "id": "entry", "kind": { "type": "text_entry", "variable": "name" },
              "frame": { "x": 100.0, "y": 100.0, "w": 300.0, "h": 40.0 } },
            { "id": "other", "kind": { "type": "button", "label": "Other" },
              "frame": { "x": 100.0, "y": 300.0, "w": 100.0, "h": 40.0 } }
        ] }]),
        serde_json::json!([{ "name": "name", "value": "" }]),
    );
    let mut player = neon_player(course);
    let mut harness = Harness::new(1280.0, 720.0);
    harness.run(&player, Vec::new());
    harness.run(&player, Vec::new());
    let t = harness.view.last_transform().expect("transform");
    let centre = t.to_screen_pos(pos2(250.0, 120.0));
    harness.run(&player, click(centre));
    harness.run(&player, vec![Event::Text("Alice".to_owned())]);
    harness.run(&player, Vec::new());
    let mut seen = Vec::new();
    for _ in 0..3 {
        let output = harness.run(&player, vec![key(egui::Key::Tab)]);
        seen.extend(output.inputs);
        harness.run(&player, Vec::new());
    }
    let committed = seen.iter().any(|input| {
        matches!(input, Input::SetInputValue { object, value }
            if object == "entry" && *value == coursecraft_course::Value::Text("Alice".into()))
    });
    assert!(committed, "Tab lost the typed text: {seen:?}");
    for input in &seen {
        player.handle(input);
    }
    assert_eq!(
        player.variable("name"),
        Some(&coursecraft_course::Value::Text("Alice".into()))
    );
}

#[test]
fn n18_slide_show_bar_false_refuses_navigation_keys_and_menu() {
    let mut course = Course::from_json(HELLO_JSON).expect("valid course");
    course.player.show_bar = true;
    course.slide_mut("s-intro").expect("intro").player =
        Some(coursecraft_course::SlidePlayerOverrides {
            show_bar: Some(false),
            ..Default::default()
        });
    let boxed: &'static Course = Box::leak(Box::new(course));
    let player = neon_player(boxed);
    assert!(!player.effective_player().show_bar);

    let mut harness = Harness::new(1280.0, 720.0);
    harness.run(&player, Vec::new());
    assert!(harness.frame.strip_rect().is_none());
    let output = harness.run(&player, vec![key(egui::Key::ArrowRight)]);
    assert!(
        !output
            .inputs
            .iter()
            .any(|i| matches!(i, Input::Next | Input::Previous)),
        "{:?}",
        output.inputs
    );
    harness.run(&player, vec![key(egui::Key::M)]);
    assert!(!harness.frame.drawer_open());
    assert!(
        harness
            .frame
            .chrome_rect(&ChromeButton::Next)
            .is_none_or(|r| !r.is_positive())
    );
}

const REVIEW_SLIDES: &str = r#"[{
    "id": "q1", "title": "Q1", "duration_ms": 1000,
    "objects": [
        { "id": "btn-submit", "kind": { "type": "button", "label": "Submit" }, "frame": { "x": 1000, "y": 600, "w": 200, "h": 60 } },
        { "id": "btn-review", "kind": { "type": "button", "label": "Review" }, "frame": { "x": 1000, "y": 500, "w": 200, "h": 60 } }
    ],
    "question": { "kind": "true_false", "prompt": "One", "points": 10,
        "choices": [ { "id": "true", "text": "True", "correct": true }, { "id": "false", "text": "False" } ] },
    "triggers": [
        { "action": { "action": "submit_question" }, "when": { "event": "click", "object": "btn-submit" } },
        { "action": { "action": "review_quiz" }, "when": { "event": "click", "object": "btn-review" } }
    ]
}]"#;

#[test]
fn n29_next_stays_enabled_on_the_last_answered_question_in_review() {
    let slides: serde_json::Value = serde_json::from_str(REVIEW_SLIDES).expect("json");
    let mut player = neon_player(neon_course(slides, serde_json::json!([])));
    player.handle(&Input::SelectChoice {
        choice: "true".to_owned(),
    });
    for object in ["btn-submit", "btn-review"] {
        player.handle(&Input::Click {
            object: object.to_owned(),
        });
    }
    assert!(player.is_reviewing());
    let mut harness = Harness::new(1280.0, 720.0);
    harness.run(&player, Vec::new());
    harness.run(&player, Vec::new());
    assert!(
        harness.frame.chrome_rect(&ChromeButton::Next).is_some(),
        "the Next button joins the strip so it can exit the review"
    );
    let output = harness.run(&player, vec![key(egui::Key::ArrowRight)]);
    assert!(
        output.inputs.contains(&Input::Next),
        "Next must exit the review: {:?}",
        output.inputs
    );
}

#[test]
fn n30_the_ended_card_takes_no_keys_and_no_slide_input() {
    let (_course, player) = hello();
    let mut harness = Harness::new(1280.0, 720.0);
    harness.run(&player, Vec::new());
    harness.frame.ended = true;
    harness.run(&player, Vec::new());
    let output = harness.run(&player, vec![key(egui::Key::M)]);
    assert!(output.inputs.is_empty());
    assert!(
        !harness.frame.drawer_open(),
        "M opened the drawer over the card"
    );
    let slide = harness.frame.slide_rect().expect("slide rect");
    let output = harness.run(&player, click(slide.center()));
    assert!(output.inputs.is_empty(), "{:?}", output.inputs);
}

#[test]
fn n31_unused_m_and_c_keys_still_reach_key_triggers() {
    let course = neon_course(
        serde_json::json!([{ "id": "s1", "title": "S",
            "player": { "show_menu": false },
            "objects": [],
            "triggers": [
                { "when": { "event": "key_pressed", "key": "m" },
                  "action": { "action": "set_variable", "variable": "v", "value": true } },
                { "when": { "event": "key_pressed", "key": "c" },
                  "action": { "action": "set_variable", "variable": "v", "value": true } }
            ] }]),
        serde_json::json!([{ "name": "v", "value": false }]),
    );
    let player = neon_player(course);
    let mut harness = Harness::new(1280.0, 720.0);
    harness.run(&player, Vec::new());
    for name in ["m", "c"] {
        let key_code = if name == "m" {
            egui::Key::M
        } else {
            egui::Key::C
        };
        let output = harness.run(&player, vec![key(key_code)]);
        assert!(
            output.inputs.contains(&Input::KeyPressed {
                key: name.to_owned()
            }),
            "{name} did not reach its trigger: {:?}",
            output.inputs
        );
    }
    assert!(!harness.frame.drawer_open());
    assert!(
        !harness.frame.captions_on(),
        "C toggled captions with none to show"
    );
}

#[test]
fn n34_a_click_on_the_speed_popup_does_not_reach_the_slide() {
    let course = neon_course(
        serde_json::json!([{ "id": "s1", "title": "S", "objects": [
            { "id": "big", "kind": { "type": "button", "label": "Big" },
              "frame": { "x": 0.0, "y": 0.0, "w": 1280.0, "h": 720.0 } }
        ] }]),
        serde_json::json!([]),
    );
    let player = neon_player(course);
    let mut harness = Harness::new(1280.0, 720.0);
    harness.frame.can_speed = true;
    harness.run(&player, Vec::new());
    harness.run(&player, Vec::new());
    let speed = harness
        .frame
        .chrome_rect(&ChromeButton::Speed)
        .expect("speed button")
        .center();
    harness.run(&player, click(speed));
    harness.run(&player, Vec::new());
    let top = pos2(speed.x, speed.y - 24.0 - 5.0 * 30.0 - 12.0);
    let output = harness.run(&player, click(top));
    assert!(
        output.inputs.contains(&Input::SetSpeed { rate: 2.0 }),
        "{:?}",
        output.inputs
    );
    assert!(
        !output
            .inputs
            .iter()
            .any(|i| matches!(i, Input::Click { .. })),
        "the press also clicked the slide: {:?}",
        output.inputs
    );
}

#[test]
fn n40_a_new_notice_shows_even_after_an_earlier_one_was_dismissed() {
    let (_course, player) = hello();
    let mut harness = Harness::new(1280.0, 720.0);
    harness.frame.post_notice("First".to_owned());
    harness.run(&player, Vec::new());
    harness.run(&player, Vec::new());
    let close = harness
        .frame
        .chrome_rect(&ChromeButton::DismissNotice)
        .expect("dismiss button");
    harness.run(&player, click(close.center()));
    harness.run(&player, Vec::new());
    assert!(harness.frame.showing_notice().is_none());
    harness.frame.post_notice("Print blocked".to_owned());
    assert_eq!(harness.frame.showing_notice(), Some("Print blocked"));
}

// ---- Neon accessibility verification (N113, N115, N116, N119) ----

fn apply(player: &mut Player<'static>, inputs: &[Input]) {
    for input in inputs {
        player.handle(input);
    }
}

#[test]
fn n113_pointer_click_lightbox_returns_focus_to_the_clicked_opener() {
    let course = neon_course(
        serde_json::json!([
            { "id": "s1", "title": "S",
              "objects": [
                { "id": "first", "kind": { "type": "button", "label": "First" },
                  "frame": { "x": 100.0, "y": 100.0, "w": 100.0, "h": 40.0 } },
                { "id": "open", "kind": { "type": "button", "label": "Open" },
                  "frame": { "x": 100.0, "y": 300.0, "w": 100.0, "h": 40.0 } }
              ],
              "triggers": [{
                "action": { "action": "lightbox_slide", "slide": "box" },
                "when": { "event": "click", "object": "open" }
              }] },
            { "id": "box", "title": "Box", "objects": [
                { "id": "box-btn", "kind": { "type": "button", "label": "Box" },
                  "frame": { "x": 10.0, "y": 10.0, "w": 100.0, "h": 40.0 } }
            ] }
        ]),
        serde_json::json!([]),
    );
    let mut player = neon_player(course);
    let mut harness = Harness::new(1280.0, 720.0);
    harness.run(&player, Vec::new());
    harness.run(&player, Vec::new());
    let t = harness.view.last_transform().expect("transform");
    let centre = t.to_screen_pos(pos2(150.0, 320.0));
    let output = harness.run(&player, click(centre));
    apply(&mut player, &output.inputs);
    assert!(player.lightbox_open(), "click opened the lightbox");
    for _ in 0..3 {
        let output = harness.run(&player, Vec::new());
        apply(&mut player, &output.inputs);
    }
    player.handle(&Input::CloseLightbox);
    for _ in 0..3 {
        let output = harness.run(&player, Vec::new());
        apply(&mut player, &output.inputs);
    }
    assert_eq!(
        harness.frame.focus().focused(),
        Some(&FocusTarget::Object("open".into())),
        "focus returns to the object the pointer clicked"
    );
}

#[test]
fn n115_restart_keeps_the_dom_focus_owner_flag() {
    let mut frame = PlayerFrame::new();
    frame.focus_mut().set_dom_focus_owner(true);
    frame.restart();
    assert!(frame.focus().dom_focus_owner());
}

#[test]
fn n116_leaving_keyboard_mode_clears_the_runtime_focus() {
    let course = neon_course(
        serde_json::json!([{ "id": "s1", "title": "S", "objects": [
            { "id": "obj1", "kind": { "type": "button", "label": "One" },
              "frame": { "x": 100.0, "y": 100.0, "w": 100.0, "h": 40.0 } }
        ] }]),
        serde_json::json!([]),
    );
    let mut player = neon_player(course);
    let mut harness = Harness::new(1280.0, 720.0);
    harness.run(&player, Vec::new());
    for _ in 0..3 {
        let output = harness.run(&player, vec![key(egui::Key::Tab)]);
        apply(&mut player, &output.inputs);
        if player.state().focused_object.is_some() {
            break;
        }
    }
    assert_eq!(player.state().focused_object.as_deref(), Some("obj1"));
    // A pointer press on empty canvas ends keyboard mode.
    let t = harness.view.last_transform().expect("transform");
    let output = harness.run(&player, click(t.to_screen_pos(pos2(900.0, 600.0))));
    assert!(
        output
            .inputs
            .contains(&Input::FocusChanged { object: None }),
        "runtime was never told focus left: {:?}",
        output.inputs
    );
}

#[test]
fn n119_exit_full_screen_tooltip_comes_from_the_chrome_strings() {
    let mut course = Course::from_json(HELLO_JSON).expect("valid course");
    course
        .strings
        .insert("exitFullScreen".to_owned(), "Verlassen".to_owned());
    let boxed: &'static Course = Box::leak(Box::new(course));
    let player = neon_player(boxed);
    let mut harness = Harness::new(1280.0, 720.0);
    harness
        .ctx
        .all_styles_mut(|s| s.interaction.tooltip_delay = 0.0);
    harness.frame.can_fullscreen = true;
    harness.frame.fullscreen = true;
    harness.run(&player, Vec::new());
    let rect = harness
        .frame
        .chrome_rect(&ChromeButton::Fullscreen)
        .expect("fullscreen button");
    harness.run(&player, vec![Event::PointerMoved(rect.center())]);
    harness.time += 1.0;
    harness.run(&player, vec![Event::PointerMoved(rect.center())]);
    assert!(
        has_painted_text(&harness.last_shapes, "Verlassen"),
        "tooltip stays English"
    );
}
