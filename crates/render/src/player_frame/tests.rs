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
                value: Some(coursecraft_course::Value::Number(9.0)),
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
    assert!(output.inputs.is_empty());
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
