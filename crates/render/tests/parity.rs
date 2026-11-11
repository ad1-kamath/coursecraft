//! Integration parity tests for `coursecraft-render` (M1 step 4).
//!
//! Verifies that:
//! 1. Direct simulation matches `hello-course.pass.effects.jsonl` exactly.
//! 2. Headless UI click-through matches `hello-course.pass.effects.jsonl` line by line.
//!    This is repeated at several screen sizes (letterboxed and pillarboxed) and in both
//!    three-frame and single-frame pointer delivery.
//! 3. UI clicks on disabled objects are dispatched by the renderer but ignored by the runtime.

use coursecraft_course::Course;
use coursecraft_logic::{Input, Player};
use coursecraft_render::{
    FocusTarget, FrameOutput, Hit, PlayerFrame, SlideTransform, SlideView, compose,
};
use egui::{CentralPanel, Context, Event, PointerButton, Pos2, RawInput, Rect, pos2, vec2};

const COURSE_JSON: &str = include_str!("../../../examples/hello-course/course.json");
const INPUTS_JSON: &str = include_str!("../../../examples/hello-course/pass.inputs.json");
const GOLDEN_JSONL: &str = include_str!("fixtures/hello-course.pass.effects.jsonl");
const SHOWCASE_JSON: &str = include_str!("../../../examples/showcase/course.json");
const SHOWCASE_INPUTS_JSON: &str = include_str!("../../../examples/showcase/pass.inputs.json");

const SCREEN_SIZES: [(f32, f32); 4] = [
    (640.0, 400.0),
    (1280.0, 800.0),
    (1000.0, 1000.0),
    (2560.0, 1080.0),
];
const DEFAULT_SIZE: (f32, f32) = (1280.0, 800.0);

/// How a click is delivered to egui.
#[derive(Debug, Clone, Copy)]
enum Mode {
    /// Move, press and release in three separate frames.
    Frames,
    /// Move, press and release as events of one frame.
    SingleFrame,
}

/// Everything needed to drive a `SlideView` headlessly.
struct Harness<'c> {
    ctx: Context,
    view: SlideView,
    player: Player<'c>,
    screen_rect: Rect,
    panel_rect: Rect,
    effects: Vec<String>,
    /// Names the screen size and mode in panic messages.
    label: String,
}

impl<'c> Harness<'c> {
    fn new(course: &'c Course, (w, h): (f32, f32), label: &str) -> Self {
        let ctx = Context::default();
        let screen_rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(w, h));
        let panel_rect = probe_panel_rect(&ctx, screen_rect);
        Self {
            ctx,
            view: SlideView::default(),
            player: Player::new(course).expect("valid player"),
            screen_rect,
            panel_rect,
            effects: Vec::new(),
            label: label.to_owned(),
        }
    }

    fn record(&mut self, input: &Input) {
        for effect in self.player.handle(input) {
            self.effects
                .push(serde_json::to_string(&effect).expect("serialize effect"));
        }
    }

    /// Run one UI frame and return the inputs the view produced, without handling them.
    fn frame_raw(&mut self, events: Vec<Event>) -> Vec<Input> {
        let raw_input = RawInput {
            screen_rect: Some(self.screen_rect),
            events,
            ..Default::default()
        };
        let mut returned = Vec::new();
        let output = self.ctx.run_ui(raw_input, |ui| {
            CentralPanel::default().show(ui, |ui| {
                returned = self.view.show(ui, &self.player, None);
            });
        });
        output.drop_without_applying_deltas();
        returned
    }

    /// Run one UI frame and feed the produced inputs to the player.
    fn frame(&mut self, events: Vec<Event>) {
        for input in self.frame_raw(events) {
            self.record(&input);
        }
    }

    fn idle_frame(&mut self) {
        self.frame(Vec::new());
    }

    /// Feed a direct input (`Start` / `Tick`) and then run an idle frame.
    fn feed(&mut self, input: &Input) {
        self.record(input);
        self.idle_frame();
    }

    /// Screen position of the centre of the composed item with this hit.
    fn hit_pos(&self, hit: &Hit) -> Pos2 {
        let items = compose(&self.player);
        let item = items
            .iter()
            .find(|i| &i.hit == hit)
            .unwrap_or_else(|| panic!("[{}] {hit:?} not found in compose items", self.label));
        let transform = SlideTransform::fit(self.panel_rect, self.player.course().slide_size)
            .unwrap_or_else(|| panic!("[{}] SlideTransform::fit failed", self.label));
        transform.to_screen_rect(item.rect).center()
    }

    /// Click the slide item with this hit through the UI.
    fn click_hit(&mut self, hit: &Hit, mode: Mode) {
        let pos = self.hit_pos(hit);
        let events = [
            Event::PointerMoved(pos),
            button(pos, true),
            button(pos, false),
        ];
        match mode {
            Mode::Frames => {
                for event in events {
                    self.frame(vec![event]);
                }
            }
            Mode::SingleFrame => self.frame(events.to_vec()),
        }
    }
}

fn button(pos: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    }
}

fn key_event(key: egui::Key, shift: bool) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers {
            shift,
            ..Default::default()
        },
    }
}

/// Drive a full `PlayerFrame` with keyboard events only.
struct KeyHarness<'c> {
    ctx: Context,
    frame: PlayerFrame,
    view: SlideView,
    player: Player<'c>,
    screen_rect: Rect,
    effects: Vec<String>,
    time: f64,
}

impl<'c> KeyHarness<'c> {
    fn new(course: &'c Course, (w, h): (f32, f32)) -> Self {
        Self {
            ctx: Context::default(),
            frame: PlayerFrame::new(),
            view: SlideView::default(),
            player: Player::new(course).expect("valid player"),
            screen_rect: Rect::from_min_size(pos2(0.0, 0.0), vec2(w, h)),
            effects: Vec::new(),
            time: 0.0,
        }
    }

    fn record(&mut self, input: &Input) {
        for effect in self.player.handle(input) {
            self.effects
                .push(serde_json::to_string(&effect).expect("serialize effect"));
        }
    }

    fn frame(&mut self, events: Vec<Event>) {
        self.time += 0.1;
        let raw = RawInput {
            screen_rect: Some(self.screen_rect),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        let mut output = FrameOutput::default();
        {
            let frame = &mut self.frame;
            let view = &mut self.view;
            let player = &self.player;
            let ctx = &self.ctx;
            let full = ctx.run_ui(raw, |ui| {
                output = frame.show(ui, player, view, None);
            });
            full.drop_without_applying_deltas();
        }
        for input in output.inputs {
            self.record(&input);
        }
    }

    fn settle(&mut self) {
        self.frame(Vec::new());
    }

    fn feed(&mut self, input: &Input) {
        self.record(input);
        self.settle();
    }

    fn press(&mut self, key: egui::Key) {
        self.frame(vec![key_event(key, false)]);
    }

    fn type_text(&mut self, text: &str) {
        self.frame(vec![Event::Text(text.to_owned())]);
    }

    fn tab_until(&mut self, target: &FocusTarget) {
        // The first key only reveals the ring.
        if !self.frame.focus().keyboard_mode() {
            self.press(egui::Key::Tab);
        }
        for _ in 0..200 {
            if self.frame.focus().focused() == Some(target) {
                return;
            }
            self.press(egui::Key::Tab);
        }
        panic!(
            "focus never reached {target:?}; now at {:?}",
            self.frame.focus().focused()
        );
    }
}

fn golden_lines() -> Vec<&'static str> {
    GOLDEN_JSONL
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect()
}

fn assert_effects_match_golden(label: &str, actual: &[String], expected: &[&str]) {
    let max_len = actual.len().max(expected.len());
    for i in 0..max_len {
        let actual_line = actual.get(i).map(String::as_str);
        let expected_line = expected.get(i).copied();
        if actual_line != expected_line {
            panic!(
                "[{label}] effects mismatch at index {i}:\n  actual:   {actual_line:?}\n  expected: {expected_line:?}"
            );
        }
    }
}

fn probe_panel_rect(ctx: &Context, screen_rect: Rect) -> Rect {
    let mut panel_rect = Rect::ZERO;
    let output = ctx.run_ui(
        RawInput {
            screen_rect: Some(screen_rect),
            ..Default::default()
        },
        |ui| {
            CentralPanel::default().show(ui, |ui| {
                panel_rect = ui.available_rect_before_wrap();
            });
        },
    );
    output.drop_without_applying_deltas();
    panel_rect
}

fn scripted_inputs() -> Vec<Input> {
    serde_json::from_str(INPUTS_JSON).expect("valid inputs")
}

/// Replay the scripted inputs through the UI and return the collected effects.
fn run_click_through(course: &Course, size: (f32, f32), mode: Mode, label: &str) -> Vec<String> {
    let mut h = Harness::new(course, size, label);
    for input in &scripted_inputs() {
        match input {
            Input::Start | Input::Tick { .. } => h.feed(input),
            Input::Click { object } => h.click_hit(&Hit::Object(object.clone()), mode),
            Input::SelectChoice { choice } => h.click_hit(&Hit::Choice(choice.clone()), mode),
            other => panic!("unexpected scripted input variant: {other:?}"),
        }
    }
    h.effects
}

#[test]
fn simulate_matches_golden() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut player = Player::new(&course).expect("valid player");

    let mut actual_lines = Vec::new();
    for input in &scripted_inputs() {
        for effect in player.handle(input) {
            actual_lines.push(serde_json::to_string(&effect).expect("serialize effect"));
        }
    }

    assert_effects_match_golden("direct simulation", &actual_lines, &golden_lines());
}

#[test]
fn ui_click_through_matches_golden_at_all_sizes_and_modes() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let expected = golden_lines();
    for (w, h) in SCREEN_SIZES {
        for mode in [Mode::Frames, Mode::SingleFrame] {
            let label = format!("{w}x{h} {mode:?}");
            let actual = run_click_through(&course, (w, h), mode, &label);
            assert_effects_match_golden(&label, &actual, &expected);
        }
    }
}

#[test]
fn ui_ignores_clicks_on_disabled_next_until_tabs_seen() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut h = Harness::new(&course, DEFAULT_SIZE, "disabled-next");
    let current = |h: &Harness| h.player.current_slide().map(|s| s.id.clone());

    h.feed(&Input::Start);
    // 1000 ms tick to make btn-start visible
    h.feed(&Input::Tick { ms: 1000 });

    h.click_hit(&Hit::Object("btn-start".to_owned()), Mode::Frames);
    assert_eq!(
        current(&h).as_deref(),
        Some("s-tabs"),
        "expected to be on s-tabs after clicking btn-start"
    );

    // Click btn-next through the UI while it is in initial disabled state. The final
    // release frame is not handled by the player so the view's output can be inspected.
    let next = Hit::Object("btn-next".to_owned());
    let pos_next = h.hit_pos(&next);
    h.frame(vec![Event::PointerMoved(pos_next)]);
    h.frame(vec![button(pos_next, true)]);
    let returned_inputs = h.frame_raw(vec![button(pos_next, false)]);

    assert_eq!(
        returned_inputs,
        vec![Input::Click {
            object: "btn-next".to_owned()
        }],
        "view must still return Click btn-next even when disabled"
    );

    let next_click_effects = h.player.handle(&Input::Click {
        object: "btn-next".to_owned(),
    });
    assert!(
        next_click_effects.is_empty(),
        "player must produce no effects for click on disabled btn-next, got: {next_click_effects:?}"
    );
    assert_eq!(
        current(&h).as_deref(),
        Some("s-tabs"),
        "slide must remain s-tabs"
    );

    // Click tab1, tab2, tab3 through the UI to unlock btn-next.
    for tab_id in ["tab1", "tab2", "tab3"] {
        h.click_hit(&Hit::Object(tab_id.to_owned()), Mode::SingleFrame);
    }

    // Now btn-next should be normal. Clicking it advances to s-quiz.
    h.click_hit(&next, Mode::SingleFrame);
    assert_eq!(
        current(&h).as_deref(),
        Some("s-quiz"),
        "slide must advance to s-quiz once all tabs have been seen"
    );
}

fn scripted_showcase_inputs() -> Vec<Input> {
    serde_json::from_str(SHOWCASE_INPUTS_JSON).expect("valid inputs")
}

fn keyboard_hello_effects(course: &Course) -> Vec<String> {
    let mut h = KeyHarness::new(course, DEFAULT_SIZE);
    h.feed(&Input::Start);
    h.feed(&Input::Tick { ms: 1000 });
    h.settle();

    h.tab_until(&FocusTarget::Object("btn-start".into()));
    h.press(egui::Key::Enter);
    h.settle();

    for tab in ["tab1", "tab2", "tab3"] {
        h.tab_until(&FocusTarget::Object(tab.into()));
        h.press(egui::Key::Enter);
    }
    h.tab_until(&FocusTarget::Object("btn-next".into()));
    h.press(egui::Key::Enter);
    h.settle();

    h.tab_until(&FocusTarget::Choice("b".into()));
    h.press(egui::Key::Enter);
    h.tab_until(&FocusTarget::Object("btn-submit".into()));
    h.press(egui::Key::Enter);
    h.settle();

    h.tab_until(&FocusTarget::Object("btn-continue-ok".into()));
    h.press(egui::Key::Enter);
    h.settle();
    h.effects
}

#[test]
fn ui_keyboard_through_hello_matches_golden() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let actual = keyboard_hello_effects(&course);
    assert_effects_match_golden("keyboard", &actual, &golden_lines());
}

fn sort_sequence(h: &mut KeyHarness<'_>, authored: &[&str]) {
    for (position, expected) in authored.iter().enumerate() {
        for _ in 0..40 {
            let current = h.player.sequence_order().to_vec();
            if current.get(position).map(String::as_str) == Some(*expected) {
                break;
            }
            let from = current
                .iter()
                .position(|id| id == expected)
                .unwrap_or_else(|| panic!("{expected} is not in the sequence"));
            let up = from > position;
            h.tab_until(&FocusTarget::SequenceMove {
                choice: (*expected).to_owned(),
                up,
            });
            h.press(egui::Key::Enter);
            h.settle();
        }
    }
    let expected: Vec<String> = authored.iter().map(|id| (*id).to_owned()).collect();
    assert_eq!(
        h.player.sequence_order(),
        expected.as_slice(),
        "keyboard sequence sorting did not reach the authored order"
    );
}

fn keyboard_showcase_effects(course: &Course) -> Vec<String> {
    let mut h = KeyHarness::new(course, DEFAULT_SIZE);
    h.feed(&Input::Start);
    h.feed(&Input::Tick { ms: 1000 });
    h.settle();

    // Welcome.
    h.tab_until(&FocusTarget::Object("btn-welcome-start".into()));
    h.press(egui::Key::Enter);
    h.settle();

    // Media.
    h.tab_until(&FocusTarget::Object("btn-play-video".into()));
    h.press(egui::Key::Enter);
    h.tab_until(&FocusTarget::Object("btn-play-audio".into()));
    h.press(egui::Key::Enter);
    h.feed(&Input::MediaEnded {
        object: "obj-media-video".into(),
    });
    h.tab_until(&FocusTarget::Object("btn-next-media".into()));
    h.press(egui::Key::Enter);
    h.settle();

    // Layers, states and motion.
    for id in ["tab-arch", "tab-engine", "tab-scorm", "btn-animate"] {
        h.tab_until(&FocusTarget::Object(id.into()));
        h.press(egui::Key::Enter);
    }
    h.feed(&Input::Tick { ms: 1000 });
    h.tab_until(&FocusTarget::Object("btn-next-layers".into()));
    h.press(egui::Key::Enter);
    h.settle();

    // Multiple choice.
    h.tab_until(&FocusTarget::Choice("mc-b".into()));
    h.press(egui::Key::Enter);
    h.tab_until(&FocusTarget::Object("btn-submit-mc".into()));
    h.press(egui::Key::Enter);
    h.settle();
    h.tab_until(&FocusTarget::Object("btn-ok-mc".into()));
    h.press(egui::Key::Enter);
    h.settle();

    // Multiple response.
    for id in ["mr-png", "mr-mp4", "mr-wav"] {
        h.tab_until(&FocusTarget::Choice(id.into()));
        h.press(egui::Key::Enter);
    }
    h.tab_until(&FocusTarget::Object("btn-submit-mr".into()));
    h.press(egui::Key::Enter);
    h.settle();
    h.tab_until(&FocusTarget::Object("btn-ok-mr".into()));
    h.press(egui::Key::Enter);
    h.settle();

    // True or false.
    h.tab_until(&FocusTarget::Choice("true".into()));
    h.press(egui::Key::Enter);
    h.tab_until(&FocusTarget::Object("btn-submit-tf".into()));
    h.press(egui::Key::Enter);
    h.settle();
    h.tab_until(&FocusTarget::Object("btn-ok-tf".into()));
    h.press(egui::Key::Enter);
    h.settle();

    // Fill-in: focus the field, type, then Tab out without losing the answer.
    h.tab_until(&FocusTarget::FillIn);
    h.press(egui::Key::Enter);
    h.type_text("Rust");
    assert_eq!(h.player.text_entry(), "Rust");
    h.press(egui::Key::Tab);
    assert_ne!(h.frame.focus().focused(), Some(&FocusTarget::FillIn));
    assert_eq!(
        h.player.text_entry(),
        "Rust",
        "leaving the fill-in field with Tab keeps the typed text"
    );
    h.tab_until(&FocusTarget::Object("btn-submit-fi".into()));
    h.press(egui::Key::Enter);
    h.settle();
    h.tab_until(&FocusTarget::Object("btn-ok-fi".into()));
    h.press(egui::Key::Enter);
    h.settle();

    // Sequence.
    sort_sequence(&mut h, &["seq-spec", "seq-logic", "seq-verify"]);
    h.tab_until(&FocusTarget::Object("btn-submit-seq".into()));
    h.press(egui::Key::Enter);
    h.settle();
    h.tab_until(&FocusTarget::Object("btn-ok-seq".into()));
    h.press(egui::Key::Enter);
    h.settle();

    // Matching.
    for pair in ["p-scorm", "p-logic", "p-publish"] {
        h.tab_until(&FocusTarget::MatchRow(pair.into()));
        for _ in 0..10 {
            if h.player.match_for(pair) == Some(pair) {
                break;
            }
            h.press(egui::Key::ArrowDown);
        }
        assert_eq!(
            h.player.match_for(pair),
            Some(pair),
            "{pair} was not matched"
        );
    }
    h.tab_until(&FocusTarget::Object("btn-submit-mat".into()));
    h.press(egui::Key::Enter);
    h.settle();
    h.tab_until(&FocusTarget::Object("btn-ok-mat".into()));
    h.press(egui::Key::Enter);
    h.settle();

    // Hotspot: the cursor starts at the image centre; move it into the red zone.
    h.tab_until(&FocusTarget::Hotspot);
    for _ in 0..8 {
        h.press(egui::Key::ArrowLeft);
    }
    for _ in 0..4 {
        h.press(egui::Key::ArrowUp);
    }
    h.press(egui::Key::Enter);
    h.tab_until(&FocusTarget::Object("btn-submit-hs".into()));
    h.press(egui::Key::Enter);
    h.settle();
    h.tab_until(&FocusTarget::Object("btn-ok-hs".into()));
    h.press(egui::Key::Enter);
    h.settle();

    // Drag and drop: pick each item up, cycle to its target, and drop.
    for (item, downs) in [("item-scorm", 0), ("item-logic", 1), ("item-engine", 2)] {
        h.tab_until(&FocusTarget::DragItem(item.into()));
        h.press(egui::Key::Enter);
        for _ in 0..downs {
            h.press(egui::Key::ArrowDown);
        }
        h.press(egui::Key::Enter);
        h.settle();
    }
    h.tab_until(&FocusTarget::Object("btn-submit-dd".into()));
    h.press(egui::Key::Enter);
    h.settle();
    h.tab_until(&FocusTarget::Object("btn-ok-dd".into()));
    h.press(egui::Key::Enter);
    h.settle();

    h.effects
}

fn graded_effects(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter(|line| {
            line.contains("\"effect\":\"question_answered\"")
                || line.contains("\"effect\":\"score\"")
                || line.contains("\"effect\":\"completion\"")
        })
        .cloned()
        .collect()
}

#[test]
fn ui_keyboard_through_showcase_matches_simulate_questions_and_score() {
    let course = Course::from_json(SHOWCASE_JSON).expect("valid course");
    let actual = graded_effects(&keyboard_showcase_effects(&course));

    let mut simulate = Player::new(&course).expect("valid player");
    let mut expected_all = Vec::new();
    for input in &scripted_showcase_inputs() {
        for effect in simulate.handle(input) {
            expected_all.push(serde_json::to_string(&effect).expect("serialize effect"));
        }
    }
    let expected = graded_effects(&expected_all);

    assert_eq!(actual, expected);
    let answered: Vec<&String> = actual
        .iter()
        .filter(|line| line.contains("\"effect\":\"question_answered\""))
        .collect();
    assert_eq!(answered.len(), 8, "all eight question kinds were answered");
    for line in answered {
        assert!(
            line.contains("\"correct\":true"),
            "keyboard answer was not correct: {line}"
        );
    }
    assert!(
        actual
            .iter()
            .any(|line| line.contains("\"effect\":\"score\"")),
        "the keyboard run reports a score"
    );
    assert!(
        actual
            .iter()
            .any(|line| line.contains("\"effect\":\"completion\"")),
        "the keyboard run reports completion"
    );
}

#[test]
fn arrows_still_navigate_when_no_slide_control_is_focused() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut h = KeyHarness::new(&course, DEFAULT_SIZE);
    h.feed(&Input::Start);
    h.feed(&Input::Tick { ms: 1000 });
    h.settle();

    h.press(egui::Key::ArrowRight);
    assert_eq!(
        h.player.current_slide().map(|s| s.id.as_str()),
        Some("s-tabs"),
        "right arrow navigates when an ordinary object has focus"
    );
    h.settle();
    h.press(egui::Key::ArrowLeft);
    assert_eq!(
        h.player.current_slide().map(|s| s.id.as_str()),
        Some("s-intro"),
        "left arrow navigates back"
    );
}

#[test]
fn fill_in_keeps_typed_text_when_focus_leaves_with_tab() {
    let course = Course::from_json(
        &serde_json::json!({
            "schema_version": 1,
            "id": "fill-in-focus",
            "title": "Fill in",
            "scenes": [{
                "id": "sc",
                "title": "Scene",
                "slides": [{
                    "id": "s1",
                    "title": "Answer",
                    "question": {
                        "kind": "fill_in",
                        "prompt": "Language?",
                        "choices": [],
                        "accepted": ["Rust"]
                    }
                }]
            }]
        })
        .to_string(),
    )
    .expect("valid course");
    let mut h = KeyHarness::new(&course, DEFAULT_SIZE);
    h.feed(&Input::Start);
    h.settle();

    h.tab_until(&FocusTarget::FillIn);
    h.press(egui::Key::Enter);
    h.type_text("Rust");
    assert_eq!(h.player.text_entry(), "Rust");

    h.press(egui::Key::Tab);
    assert_ne!(h.frame.focus().focused(), Some(&FocusTarget::FillIn));
    assert_eq!(
        h.player.text_entry(),
        "Rust",
        "Tab must leave the field without clearing it"
    );
}

#[test]
fn the_first_key_only_reveals_the_ring() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut h = KeyHarness::new(&course, DEFAULT_SIZE);
    h.feed(&Input::Start);
    h.feed(&Input::Tick { ms: 1000 });
    h.settle();
    let first = h.frame.focus().focused().cloned();
    let slide = h.player.current_slide().map(|s| s.id.clone());
    assert!(!h.frame.focus().keyboard_mode());

    // Enter with no ring showing must not press something the learner can't see.
    h.press(egui::Key::Enter);
    assert!(h.frame.focus().keyboard_mode());
    assert_eq!(h.frame.focus().focused().cloned(), first);
    assert_eq!(h.player.current_slide().map(|s| s.id.clone()), slide);

    // Now the ring shows, and the same key acts.
    h.press(egui::Key::Enter);
    h.settle();
    assert_ne!(h.player.current_slide().map(|s| s.id.clone()), slide);
}

#[test]
fn the_first_tab_lands_on_the_first_item_instead_of_skipping_it() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut h = KeyHarness::new(&course, DEFAULT_SIZE);
    h.feed(&Input::Start);
    h.feed(&Input::Tick { ms: 1000 });
    h.settle();
    let first = h.frame.focus().focused().cloned();
    h.press(egui::Key::Tab);
    assert_eq!(h.frame.focus().focused().cloned(), first);
    h.press(egui::Key::Tab);
    assert_ne!(h.frame.focus().focused().cloned(), first);
}

#[test]
fn a_pointer_press_releases_the_hover_the_keyboard_held() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut h = KeyHarness::new(&course, DEFAULT_SIZE);
    h.feed(&Input::Start);
    h.feed(&Input::Tick { ms: 1000 });
    h.settle();
    h.press(egui::Key::Tab);
    assert!(h.frame.focus().hovered_object().is_some());
    h.frame(vec![Event::PointerButton {
        pos: egui::pos2(1.0, 1.0),
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    }]);
    assert!(!h.frame.focus().keyboard_mode());
    assert!(h.frame.focus().hovered_object().is_none());
}

#[test]
fn nothing_takes_focus_keys_once_the_course_has_ended() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut h = KeyHarness::new(&course, DEFAULT_SIZE);
    h.feed(&Input::Start);
    h.feed(&Input::Tick { ms: 1000 });
    h.settle();
    h.frame.ended = true;
    let slide = h.player.current_slide().map(|s| s.id.clone());
    h.press(egui::Key::Tab);
    h.press(egui::Key::Enter);
    h.press(egui::Key::Enter);
    assert!(!h.frame.focus().keyboard_mode());
    assert_eq!(h.player.current_slide().map(|s| s.id.clone()), slide);
}

#[test]
fn every_tab_counts_when_several_arrive_in_one_frame() {
    let course = Course::from_json(COURSE_JSON).expect("valid course");
    let mut h = KeyHarness::new(&course, DEFAULT_SIZE);
    h.feed(&Input::Start);
    h.feed(&Input::Tick { ms: 1000 });
    h.settle();
    h.press(egui::Key::Tab);
    let start = h.frame.focus().focused().cloned();
    let list = h.frame.focus().list().to_vec();
    let index = list
        .iter()
        .position(|target| Some(target) == start.as_ref())
        .expect("focus is in the list");
    let tab = || key_event(egui::Key::Tab, false);
    h.frame(vec![tab(), tab()]);
    let expected = list.get((index + 2) % list.len()).cloned();
    assert_eq!(h.frame.focus().focused().cloned(), expected);
}
