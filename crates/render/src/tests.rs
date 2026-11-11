use coursecraft_course::{Color, Course, Rect as ModelRect, Size};
use coursecraft_logic::{Input, Player};
use egui::{Color32, Rect, pos2, vec2};

use crate::color::parse_color;
use crate::compose::{
    DrawItem, Hit, Visual, compose, compose_authoring, compose_authoring_layers, compose_object,
    is_visible_at, validate_frame,
};
use crate::layout::SlideTransform;
use crate::ticker::Ticker;

const HELLO_JSON: &str = include_str!("../../../examples/hello-course/course.json");

/// An egui context with its fonts built, for layout-only tests.
fn text_layout_context() -> egui::Context {
    let ctx = egui::Context::default();
    let output = ctx.run_ui(egui::RawInput::default(), |_ui| {});
    output.drop_without_applying_deltas();
    ctx
}

#[test]
fn large_ellipse_uses_enough_segments() {
    assert!(
        crate::paint::ellipse_segment_count(600.0) >= 128,
        "a 600 px ellipse must use at least 128 segments"
    );
    assert_eq!(
        crate::paint::ellipse_segment_count(10.0),
        32,
        "small ellipses keep the minimum segment count"
    );
    assert_eq!(
        crate::paint::ellipse_segment_count(6000.0),
        256,
        "huge ellipses stay bounded"
    );
}

// ---------------------------------------------------------------- color tests

#[test]
fn color_parsing_valid() {
    assert_eq!(
        parse_color(&Color("#2F6FEB".to_owned())),
        Some(Color32::from_rgb(0x2F, 0x6F, 0xEB))
    );
    assert_eq!(
        parse_color(&Color("#000000".to_owned())),
        Some(Color32::from_rgb(0, 0, 0))
    );
    assert_eq!(
        parse_color(&Color("#FFFFFF".to_owned())),
        Some(Color32::from_rgb(255, 255, 255))
    );
    assert_eq!(
        parse_color(&Color("#FFFFFFF0".to_owned())),
        Some(Color32::from_rgba_unmultiplied(255, 255, 255, 0xF0))
    );
    assert_eq!(
        parse_color(&Color("#11223344".to_owned())),
        Some(Color32::from_rgba_unmultiplied(0x11, 0x22, 0x33, 0x44))
    );
}

#[test]
fn color_parsing_invalid() {
    // Short hex
    assert_eq!(parse_color(&Color("#FFF".to_owned())), None);
    assert_eq!(parse_color(&Color("#12345".to_owned())), None);

    // Bad hex characters
    assert_eq!(parse_color(&Color("#GGGGGG".to_owned())), None);
    assert_eq!(parse_color(&Color("#12345Z".to_owned())), None);
    assert_eq!(parse_color(&Color("#1234567G".to_owned())), None);

    // Missing #
    assert_eq!(parse_color(&Color("2F6FEB".to_owned())), None);
    assert_eq!(parse_color(&Color("FFFFFFF0".to_owned())), None);

    // Empty or bare #
    assert_eq!(parse_color(&Color(String::new())), None);
    assert_eq!(parse_color(&Color("#".to_owned())), None);

    // Non-ascii and too long
    assert_eq!(parse_color(&Color("#🦀1234".to_owned())), None);
    assert_eq!(parse_color(&Color("#123456789".to_owned())), None);
    assert_eq!(parse_color(&Color("#1234567".to_owned())), None);
}

// ---------------------------------------------------------------- layout tests

#[test]
fn transform_pillarbox_and_letterbox() {
    let slide = Size { w: 1280, h: 720 }; // 16:9

    // Pillarbox: available is wider (2:1) -> vertical fit, horizontal bars
    let available_wide = Rect::from_min_size(pos2(0.0, 0.0), vec2(1600.0, 800.0));
    let t_wide = SlideTransform::fit(available_wide, slide).expect("fit wide");
    assert!((t_wide.scale() - (800.0 / 720.0)).abs() < 1e-3);
    let r_wide = t_wide.slide_rect();
    assert!((r_wide.height() - 800.0).abs() < 1e-3);
    assert!((r_wide.min.y - 0.0).abs() < 1e-3);
    // Centred horizontally
    let expected_w = 1280.0 * (800.0 / 720.0);
    assert!((r_wide.width() - expected_w).abs() < 1e-2);
    let expected_bar = (1600.0 - expected_w) * 0.5;
    assert!((r_wide.min.x - expected_bar).abs() < 1e-2);

    // Letterbox: available is taller (5:4) -> horizontal fit, vertical bars
    let available_tall = Rect::from_min_size(pos2(0.0, 0.0), vec2(1000.0, 800.0));
    let t_tall = SlideTransform::fit(available_tall, slide).expect("fit tall");
    assert!((t_tall.scale() - (1000.0 / 1280.0)).abs() < 1e-3);
    let r_tall = t_tall.slide_rect();
    assert!((r_tall.width() - 1000.0).abs() < 1e-3);
    assert!((r_tall.min.x - 0.0).abs() < 1e-3);
    // Centred vertically
    let expected_h = 720.0 * (1000.0 / 1280.0);
    assert!((r_tall.height() - expected_h).abs() < 1e-2);
    let expected_top_bar = (800.0 - expected_h) * 0.5;
    assert!((r_tall.min.y - expected_top_bar).abs() < 1e-2);

    // Exact fit: scale = 1.0, fills available
    let available_exact = Rect::from_min_size(pos2(50.0, 50.0), vec2(1280.0, 720.0));
    let t_exact = SlideTransform::fit(available_exact, slide).expect("fit exact");
    assert!((t_exact.scale() - 1.0).abs() < 1e-3);
    assert_eq!(t_exact.slide_rect(), available_exact);
}

#[test]
fn transform_round_tripping() {
    let slide = Size { w: 1280, h: 720 };
    let available = Rect::from_min_size(pos2(100.0, 50.0), vec2(1600.0, 900.0));
    let t = SlideTransform::fit(available, slide).expect("fit");

    for p in [
        pos2(0.0, 0.0),
        pos2(1280.0, 720.0),
        pos2(640.0, 360.0),
        pos2(123.4, 567.8),
    ] {
        let screen = t.to_screen_pos(p);
        let back = t.to_slide_pos(screen);
        assert!((back.x - p.x).abs() < 1e-4, "roundtrip x mismatch at {p:?}");
        assert!((back.y - p.y).abs() < 1e-4, "roundtrip y mismatch at {p:?}");
    }

    let slide_r = Rect::from_min_size(pos2(80.0, 120.0), vec2(200.0, 60.0));
    let screen_r = t.to_screen_rect(slide_r);
    assert_eq!(screen_r.min, t.to_screen_pos(slide_r.min));
    assert_eq!(screen_r.max, t.to_screen_pos(slide_r.max));
}

#[test]
fn transform_zero_and_nan_sizes_give_none() {
    let valid_available = Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 720.0));
    let valid_slide = Size { w: 1280, h: 720 };

    // Zero slide sizes
    assert_eq!(
        SlideTransform::fit(valid_available, Size { w: 0, h: 720 }),
        None
    );
    assert_eq!(
        SlideTransform::fit(valid_available, Size { w: 1280, h: 0 }),
        None
    );
    assert_eq!(
        SlideTransform::fit(valid_available, Size { w: 0, h: 0 }),
        None
    );

    // Empty or negative available rects
    assert_eq!(
        SlideTransform::fit(
            Rect::from_min_size(pos2(0.0, 0.0), vec2(0.0, 720.0)),
            valid_slide
        ),
        None
    );
    assert_eq!(
        SlideTransform::fit(
            Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 0.0)),
            valid_slide
        ),
        None
    );
    assert_eq!(
        SlideTransform::fit(
            Rect::from_min_size(pos2(0.0, 0.0), vec2(-10.0, 720.0)),
            valid_slide
        ),
        None
    );

    // Non-finite available rects
    assert_eq!(
        SlideTransform::fit(
            Rect::from_min_size(pos2(f32::NAN, 0.0), vec2(1280.0, 720.0)),
            valid_slide
        ),
        None
    );
    assert_eq!(
        SlideTransform::fit(
            Rect::from_min_size(pos2(0.0, 0.0), vec2(f32::INFINITY, 720.0)),
            valid_slide
        ),
        None
    );
}

// ---------------------------------------------------------------- compose tests

#[test]
fn frame_validation_drops_hostile_frames() {
    // Non-finite
    assert_eq!(
        validate_frame(&ModelRect {
            x: f32::NAN,
            y: 0.0,
            w: 100.0,
            h: 50.0
        }),
        None
    );
    assert_eq!(
        validate_frame(&ModelRect {
            x: 0.0,
            y: f32::INFINITY,
            w: 100.0,
            h: 50.0
        }),
        None
    );
    assert_eq!(
        validate_frame(&ModelRect {
            x: 0.0,
            y: 0.0,
            w: f32::NAN,
            h: 50.0
        }),
        None
    );
    assert_eq!(
        validate_frame(&ModelRect {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: f32::NEG_INFINITY
        }),
        None
    );

    // Negative width / height
    assert_eq!(
        validate_frame(&ModelRect {
            x: 0.0,
            y: 0.0,
            w: -1.0,
            h: 50.0
        }),
        None
    );
    assert_eq!(
        validate_frame(&ModelRect {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: -0.01
        }),
        None
    );

    // Overflow
    assert_eq!(
        validate_frame(&ModelRect {
            x: f32::MAX,
            y: 0.0,
            w: f32::MAX,
            h: 50.0
        }),
        None
    );

    // Valid frame
    let valid = validate_frame(&ModelRect {
        x: 80.0,
        y: 120.0,
        w: 200.0,
        h: 60.0,
    });
    assert_eq!(
        valid,
        Some(Rect::from_min_size(pos2(80.0, 120.0), vec2(200.0, 60.0)))
    );

    // Zero width and height is non-negative and valid
    assert_eq!(
        validate_frame(&ModelRect {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0
        }),
        Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(0.0, 0.0)))
    );
}

#[test]
fn timing_visibility() {
    let t_open = coursecraft_course::Timing {
        start_ms: 1000,
        end_ms: None,
    };
    assert!(!is_visible_at(&t_open, 0));
    assert!(!is_visible_at(&t_open, 999));
    assert!(is_visible_at(&t_open, 1000));
    assert!(is_visible_at(&t_open, 5000));

    let t_window = coursecraft_course::Timing {
        start_ms: 1000,
        end_ms: Some(3000),
    };
    assert!(!is_visible_at(&t_window, 999));
    assert!(is_visible_at(&t_window, 1000));
    assert!(is_visible_at(&t_window, 2999));
    assert!(!is_visible_at(&t_window, 3000));
    assert!(!is_visible_at(&t_window, 4000));
}

fn has_object(items: &[DrawItem], id: &str) -> bool {
    items
        .iter()
        .any(|item| item.hit == Hit::Object(id.to_owned()))
}

#[test]
fn compose_hello_course_timing_and_layers() {
    let course = Course::from_json(HELLO_JSON).expect("valid course");
    let mut player = Player::new(&course).expect("valid player");

    // Before Start (ms = 0): btn-start is timed for start_ms: 1000, so absent
    let items = compose(&player);
    assert!(has_object(&items, "title"));
    assert!(has_object(&items, "btn-about"));
    assert!(!has_object(&items, "btn-start"));

    // Feed Start
    player.handle(&Input::Start);

    // Tick to 500 ms: btn-start is still absent
    player.handle(&Input::Tick { ms: 500 });
    let items = compose(&player);
    assert!(!has_object(&items, "btn-start"));

    // Tick another 500 ms: base clock reaches 1000 ms, btn-start appears
    player.handle(&Input::Tick { ms: 500 });
    let items = compose(&player);
    assert!(has_object(&items, "btn-start"));

    // Click btn-about: shows layer "about"
    player.handle(&Input::Click {
        object: "btn-about".to_owned(),
    });
    assert_eq!(player.state().visible_layers, vec!["about"]);

    let items = compose(&player);
    // Base objects come first: "title", "btn-start", "btn-about"
    // Layer objects come after: "about-panel", "about-text", "btn-close"
    assert!(has_object(&items, "about-panel"));
    assert!(has_object(&items, "about-text"));
    assert!(has_object(&items, "btn-close"));

    let idx_btn_about = items
        .iter()
        .position(|i| i.hit == Hit::Object("btn-about".to_owned()))
        .expect("btn-about present");
    let idx_about_panel = items
        .iter()
        .position(|i| i.hit == Hit::Object("about-panel".to_owned()))
        .expect("about-panel present");
    let idx_btn_close = items
        .iter()
        .position(|i| i.hit == Hit::Object("btn-close".to_owned()))
        .expect("btn-close present");

    assert!(
        idx_btn_about < idx_about_panel,
        "base objects must come before layer objects"
    );
    assert!(
        idx_about_panel < idx_btn_close,
        "layer objects must preserve bottom-to-top order"
    );
}

#[test]
fn compose_state_fill_overrides_on_tabs() {
    let course = Course::from_json(HELLO_JSON).expect("valid course");
    let mut player = Player::new(&course).expect("valid player");

    player.handle(&Input::Start);
    player.handle(&Input::Tick { ms: 1000 });
    // Click btn-start to navigate to s-tabs
    player.handle(&Input::Click {
        object: "btn-start".to_owned(),
    });
    assert_eq!(
        player.current_slide().map(|s| s.id.as_str()),
        Some("s-tabs")
    );

    // Initially tab1 has normal fill (#2F6FEB by default for buttons)
    let items = compose(&player);
    let tab1_initial = items
        .iter()
        .find(|i| i.hit == Hit::Object("tab1".to_owned()))
        .expect("tab1 item");
    assert_eq!(
        tab1_initial.visual,
        Visual::Button {
            label: "Slips".to_owned(),
            fill: Color32::from_rgb(0x2F, 0x6F, 0xEB),
            fill_style: None,
            hover_fill: None,
            down_fill: None,
            disabled: false,
        }
    );

    // Clicking tab1 marks it visited in runtime, triggering state override visited -> fill: #2E8B57
    player.handle(&Input::Click {
        object: "tab1".to_owned(),
    });
    assert_eq!(player.object_state("tab1"), Some("visited"));

    let items_after = compose(&player);
    let tab1_visited = items_after
        .iter()
        .find(|i| i.hit == Hit::Object("tab1".to_owned()))
        .expect("tab1 item after click");
    assert_eq!(
        tab1_visited.visual,
        Visual::Button {
            label: "Slips".to_owned(),
            fill: Color32::from_rgb(0x2E, 0x8B, 0x57),
            fill_style: None,
            hover_fill: None,
            down_fill: None,
            disabled: false,
        }
    );
}

#[test]
fn compose_question_and_choices_selection() {
    let course = Course::from_json(HELLO_JSON).expect("valid course");
    let mut player = Player::new(&course).expect("valid player");

    player.handle(&Input::Start);
    // Jump straight to s-quiz
    player.handle(&Input::GoTo {
        slide: "s-quiz".to_owned(),
    });
    assert_eq!(
        player.current_slide().map(|s| s.id.as_str()),
        Some("s-quiz")
    );

    let items = compose(&player);

    // Base button btn-submit is present
    assert!(has_object(&items, "btn-submit"));

    // Prompt has Hit::None
    let prompt_item = items
        .iter()
        .find(|i| matches!(i.visual, Visual::Prompt { .. }))
        .expect("prompt item");
    assert_eq!(prompt_item.hit, Hit::None);
    assert_eq!(
        prompt_item.visual,
        Visual::Prompt {
            text: "You see a spill in the corridor. What do you do first?".to_owned()
        }
    );

    // Choice items exist with selected: false
    let choice_b = items
        .iter()
        .find(|i| i.hit == Hit::Choice("b".to_owned()))
        .expect("choice b");
    assert_eq!(
        choice_b.visual,
        Visual::ChoiceRow {
            text: "Clean it or mark it, then report it".to_owned(),
            multiple: false,
            selected: false,
            mark: None,
        }
    );

    // Select choice b
    player.handle(&Input::SelectChoice {
        choice: "b".to_owned(),
    });
    let items_after = compose(&player);

    let choice_a = items_after
        .iter()
        .find(|i| i.hit == Hit::Choice("a".to_owned()))
        .expect("choice a");
    assert_eq!(
        choice_a.visual,
        Visual::ChoiceRow {
            text: "Walk around it".to_owned(),
            multiple: false,
            selected: false,
            mark: None,
        }
    );

    let choice_b_after = items_after
        .iter()
        .find(|i| i.hit == Hit::Choice("b".to_owned()))
        .expect("choice b after selection");
    assert_eq!(
        choice_b_after.visual,
        Visual::ChoiceRow {
            text: "Clean it or mark it, then report it".to_owned(),
            multiple: false,
            selected: true,
            mark: None,
        }
    );
}

// ---------------------------------------------------------------- ticker tests

#[test]
fn ticker_accumulates_and_advances() {
    let mut ticker = Ticker::new();

    // 16.6 ms step -> emits 16 ms, carries 0.6 ms
    let t1 = ticker.advance(0.0166);
    assert_eq!(t1, Some(Input::Tick { ms: 16 }));

    // Another 16.6 ms step -> 16.6 + 0.6 = 17.2 ms -> emits 17 ms, carries 0.2 ms
    let t2 = ticker.advance(0.0166);
    assert_eq!(t2, Some(Input::Tick { ms: 17 }));
}

#[test]
fn ticker_sub_millisecond_steps() {
    let mut ticker = Ticker::new();

    // 0.3 ms -> None
    assert_eq!(ticker.advance(0.0003), None);
    // 0.4 ms -> sum 0.7 ms -> None
    assert_eq!(ticker.advance(0.0004), None);
    // 0.4 ms -> sum 1.1 ms -> Tick { ms: 1 }
    assert_eq!(ticker.advance(0.0004), Some(Input::Tick { ms: 1 }));
}

#[test]
fn ticker_ignores_invalid_dt() {
    let mut ticker = Ticker::new();

    assert_eq!(ticker.advance(-0.016), None);
    assert_eq!(ticker.advance(0.0), None);
    assert_eq!(ticker.advance(f32::NAN), None);
    assert_eq!(ticker.advance(f32::INFINITY), None);
    assert_eq!(ticker.advance(f32::NEG_INFINITY), None);

    // Clean advance still works after bad inputs
    assert_eq!(ticker.advance(0.020), Some(Input::Tick { ms: 20 }));
}

#[test]
fn ticker_caps_at_1000_ms() {
    let mut ticker = Ticker::new();

    // 2.5 seconds (2500 ms) is capped at 1000 ms
    assert_eq!(ticker.advance(2.5), Some(Input::Tick { ms: 1000 }));

    // After capping, next normal step starts fresh without excess remainder
    assert_eq!(ticker.advance(0.016), Some(Input::Tick { ms: 16 }));
}

#[test]
fn ticker_long_session_has_no_drift() {
    let mut ticker = Ticker::new();
    let mut total: u64 = 0;
    for _ in 0..100_000 {
        if let Some(Input::Tick { ms }) = ticker.advance(1.0 / 60.0) {
            total += u64::from(ms);
        }
    }
    let expected: u64 = 100_000 * 1000 / 60;
    assert!(
        total.abs_diff(expected) <= 1,
        "total {total} vs expected {expected}"
    );
}

// ---------------------------------------------------------------- hit_test & pointer tests

use crate::paint::{Highlight, paint};
use crate::view::{PointerState, SlideView, hit_test};
use coursecraft_course::ShapeKind;

fn make_dummy_item(hit: Hit, rect: Rect) -> DrawItem {
    DrawItem {
        hit,
        rect,
        visual: Visual::Shape {
            shape: ShapeKind::Rectangle,
            adjust: Vec::new(),
            fill: Color32::from_rgb(100, 100, 100),
            fill_style: None,
            outline: None,
            text: None,
            hover_fill: None,
            down_fill: None,
            disabled: false,
        },
        dimmed: false,
        opacity: 1.0,
        clip: None,
        rotation: 0.0,
        flip_h: false,
        flip_v: false,
        shadow: None,
        glow: None,
        media_time_ms: None,
        links: Vec::new(),
        show_table_overflow: false,
        clip_rect: None,
        mark: None,
    }
}

#[test]
fn hit_test_topmost_wins() {
    let item_bottom = make_dummy_item(
        Hit::Object("bottom".to_owned()),
        Rect::from_min_size(pos2(10.0, 10.0), vec2(50.0, 50.0)),
    );
    let item_top = make_dummy_item(
        Hit::Object("top".to_owned()),
        Rect::from_min_size(pos2(10.0, 10.0), vec2(50.0, 50.0)),
    );
    let items = vec![item_bottom, item_top];

    let hit = hit_test(&items, pos2(20.0, 20.0));
    assert_eq!(hit, Some(Hit::Object("top".to_owned())));
}

#[test]
fn hit_test_none_is_ignored() {
    let item_bottom = make_dummy_item(
        Hit::Object("bottom".to_owned()),
        Rect::from_min_size(pos2(10.0, 10.0), vec2(50.0, 50.0)),
    );
    let item_none = make_dummy_item(
        Hit::None,
        Rect::from_min_size(pos2(10.0, 10.0), vec2(50.0, 50.0)),
    );
    let items = vec![item_bottom, item_none];

    let hit = hit_test(&items, pos2(20.0, 20.0));
    assert_eq!(hit, Some(Hit::Object("bottom".to_owned())));
}

#[test]
fn hit_test_points_outside_ignored() {
    let item = make_dummy_item(
        Hit::Object("btn".to_owned()),
        Rect::from_min_size(pos2(10.0, 10.0), vec2(50.0, 50.0)),
    );
    let items = vec![item];

    assert_eq!(hit_test(&items, pos2(5.0, 5.0)), None);
    assert_eq!(hit_test(&items, pos2(100.0, 100.0)), None);
    assert_eq!(hit_test(&items, pos2(f32::NAN, 20.0)), None);
    assert_eq!(hit_test(&items, pos2(20.0, f32::INFINITY)), None);
}

#[test]
fn pointer_press_and_release_same_object_emits_click() {
    let slide_size = Size { w: 1280, h: 720 };
    let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 720.0));
    let t = SlideTransform::fit(screen, slide_size).expect("fit");

    let item = make_dummy_item(
        Hit::Object("btn".to_owned()),
        Rect::from_min_size(pos2(100.0, 100.0), vec2(200.0, 60.0)),
    );
    let items = vec![item];

    let mut state = PointerState::default();
    let pos = pos2(150.0, 130.0);

    let events = vec![
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        },
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::default(),
        },
    ];

    let inputs = state.handle_events(&events, &t, &items, &|_| false);
    assert_eq!(
        inputs,
        vec![Input::Click {
            object: "btn".to_owned()
        }]
    );
    assert_eq!(state.pressed, None);
}

#[test]
fn pointer_press_on_a_and_release_on_b_emits_nothing() {
    let slide_size = Size { w: 1280, h: 720 };
    let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 720.0));
    let t = SlideTransform::fit(screen, slide_size).expect("fit");

    let item_a = make_dummy_item(
        Hit::Object("btn-a".to_owned()),
        Rect::from_min_size(pos2(100.0, 100.0), vec2(100.0, 60.0)),
    );
    let item_b = make_dummy_item(
        Hit::Object("btn-b".to_owned()),
        Rect::from_min_size(pos2(300.0, 100.0), vec2(100.0, 60.0)),
    );
    let items = vec![item_a, item_b];

    let mut state = PointerState::default();
    let pos_a = pos2(150.0, 130.0);
    let pos_b = pos2(350.0, 130.0);

    let events = vec![
        egui::Event::PointerButton {
            pos: pos_a,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        },
        egui::Event::PointerButton {
            pos: pos_b,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::default(),
        },
    ];

    let inputs = state.handle_events(&events, &t, &items, &|_| false);
    assert!(inputs.is_empty(), "expected no inputs, got {inputs:?}");
    assert_eq!(state.pressed, None);
}

#[test]
fn pointer_release_over_choice_emits_select_choice() {
    let slide_size = Size { w: 1280, h: 720 };
    let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 720.0));
    let t = SlideTransform::fit(screen, slide_size).expect("fit");

    let item_choice = make_dummy_item(
        Hit::Choice("choice-1".to_owned()),
        Rect::from_min_size(pos2(80.0, 160.0), vec2(400.0, 56.0)),
    );
    let items = vec![item_choice];

    let mut state = PointerState::default();
    let pos = pos2(120.0, 180.0);

    let events = vec![
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        },
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::default(),
        },
    ];

    let inputs = state.handle_events(&events, &t, &items, &|_| false);
    assert_eq!(
        inputs,
        vec![Input::SelectChoice {
            choice: "choice-1".to_owned()
        }]
    );
    assert_eq!(state.pressed, None);
}

#[test]
fn pointer_moving_between_objects_emits_hover_end_then_start() {
    let slide_size = Size { w: 1280, h: 720 };
    let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 720.0));
    let t = SlideTransform::fit(screen, slide_size).expect("fit");

    let item_a = make_dummy_item(
        Hit::Object("btn-a".to_owned()),
        Rect::from_min_size(pos2(100.0, 100.0), vec2(100.0, 60.0)),
    );
    let item_b = make_dummy_item(
        Hit::Object("btn-b".to_owned()),
        Rect::from_min_size(pos2(300.0, 100.0), vec2(100.0, 60.0)),
    );
    let items = vec![item_a, item_b];

    let mut state = PointerState::default();
    let pos_a = pos2(150.0, 130.0);
    let pos_b = pos2(350.0, 130.0);

    let mut inputs = Vec::new();
    state.handle_event(
        &egui::Event::PointerMoved(pos_a),
        &t,
        &items,
        &|_| false,
        &mut inputs,
    );
    assert_eq!(
        inputs,
        vec![Input::HoverStart {
            object: "btn-a".to_owned()
        }]
    );
    assert_eq!(state.hovered, Some(Hit::Object("btn-a".to_owned())));

    inputs.clear();
    state.handle_event(
        &egui::Event::PointerMoved(pos_b),
        &t,
        &items,
        &|_| false,
        &mut inputs,
    );
    assert_eq!(
        inputs,
        vec![
            Input::HoverEnd {
                object: "btn-a".to_owned()
            },
            Input::HoverStart {
                object: "btn-b".to_owned()
            },
        ]
    );
    assert_eq!(state.hovered, Some(Hit::Object("btn-b".to_owned())));
}

#[test]
fn pointer_moving_over_choice_emits_no_hover() {
    let slide_size = Size { w: 1280, h: 720 };
    let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 720.0));
    let t = SlideTransform::fit(screen, slide_size).expect("fit");

    let item_choice = make_dummy_item(
        Hit::Choice("choice-1".to_owned()),
        Rect::from_min_size(pos2(80.0, 160.0), vec2(400.0, 56.0)),
    );
    let items = vec![item_choice];

    let mut state = PointerState::default();
    let pos = pos2(120.0, 180.0);

    let mut inputs = Vec::new();
    state.handle_event(
        &egui::Event::PointerMoved(pos),
        &t,
        &items,
        &|_| false,
        &mut inputs,
    );
    assert!(inputs.is_empty(), "choices should not hover");
    assert_eq!(state.hovered, None);

    // If an object was previously hovered, moving onto a choice ends hover for that object without starting hover on the choice
    state.hovered = Some(Hit::Object("btn-prev".to_owned()));
    inputs.clear();
    state.handle_event(
        &egui::Event::PointerMoved(pos),
        &t,
        &items,
        &|_| false,
        &mut inputs,
    );
    assert_eq!(
        inputs,
        vec![Input::HoverEnd {
            object: "btn-prev".to_owned()
        }]
    );
    assert_eq!(state.hovered, None);
}

#[test]
fn pointer_prune_uncomposed_drops_silently_on_slide_change() {
    let mut state = PointerState {
        hovered: Some(Hit::Object("old-btn".to_owned())),
        pressed: Some(Hit::Object("old-btn".to_owned())),
        ..PointerState::default()
    };

    let new_item = make_dummy_item(
        Hit::Object("new-btn".to_owned()),
        Rect::from_min_size(pos2(10.0, 10.0), vec2(50.0, 50.0)),
    );
    let new_items = vec![new_item];

    state.prune_uncomposed(&new_items);
    assert_eq!(state.hovered, None);
    assert_eq!(state.pressed, None);
}

#[test]
fn compose_with_hover_down_states_and_disabled() {
    let course_json = r##"{
      "schema_version": 1,
      "id": "course-state-test",
      "title": "State Test",
      "slide_size": { "w": 1280, "h": 720 },
      "scenes": [
        {
          "id": "scene-1",
          "title": "Scene 1",
          "slides": [
            {
              "id": "slide-1",
              "title": "Slide 1",
              "objects": [
                {
                  "id": "btn-styled",
                  "kind": { "type": "button", "label": "Click me", "fill": "#2F6FEB" },
                  "frame": { "x": 100, "y": 100, "w": 200, "h": 60 },
                  "states": [
                    { "name": "hover", "fill": "#334455" },
                    { "name": "down", "fill": "#667788" },
                    { "name": "disabled", "fill": "#888888" }
                  ]
                },
                {
                  "id": "shape-disabled",
                  "kind": { "type": "shape", "shape": "rectangle", "fill": "#112233" },
                  "frame": { "x": 400, "y": 100, "w": 100, "h": 100 },
                  "initial_state": "disabled",
                  "states": [
                    { "name": "disabled" },
                    { "name": "hover", "fill": "#AABBCC" }
                  ]
                }
              ]
            }
          ]
        }
      ]
    }"##;

    let course = Course::from_json(course_json).expect("valid test course");
    let player = Player::new(&course).expect("valid player");

    let items = compose(&player);

    let btn_item = items
        .iter()
        .find(|i| i.hit == Hit::Object("btn-styled".to_owned()))
        .expect("btn-styled");
    assert_eq!(
        btn_item.visual,
        Visual::Button {
            label: "Click me".to_owned(),
            fill: Color32::from_rgb(0x2F, 0x6F, 0xEB),
            fill_style: None,
            hover_fill: Some(Color32::from_rgb(0x33, 0x44, 0x55)),
            down_fill: Some(Color32::from_rgb(0x66, 0x77, 0x88)),
            disabled: false,
        }
    );

    let shape_item = items
        .iter()
        .find(|i| i.hit == Hit::Object("shape-disabled".to_owned()))
        .expect("shape-disabled");
    assert_eq!(
        shape_item.visual,
        Visual::Shape {
            shape: ShapeKind::Rectangle,
            adjust: Vec::new(),
            fill: Color32::from_rgb(0x11, 0x22, 0x33),
            fill_style: None,
            outline: None,
            text: None,
            hover_fill: Some(Color32::from_rgb(0xAA, 0xBB, 0xCC)),
            down_fill: None,
            disabled: true,
        }
    );
}

#[test]
fn headless_smoke_test_click_btn_about() {
    let course = Course::from_json(HELLO_JSON).expect("valid course");
    let mut player = Player::new(&course).expect("valid player");
    player.handle(&Input::Start);

    let mut view = SlideView::default();
    let ctx = egui::Context::default();

    // Find the btn-about item
    let items = compose(&player);
    let about_item = items
        .iter()
        .find(|i| i.hit == Hit::Object("btn-about".to_owned()))
        .expect("btn-about composed");

    let screen_rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 720.0));
    let t = SlideTransform::fit(screen_rect, player.course().slide_size).expect("fit");
    let screen_btn_rect = t.to_screen_rect(about_item.rect);
    let btn_center = screen_btn_rect.center();

    let raw_input = egui::RawInput {
        screen_rect: Some(screen_rect),
        events: vec![
            egui::Event::PointerMoved(btn_center),
            egui::Event::PointerButton {
                pos: btn_center,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::default(),
            },
            egui::Event::PointerButton {
                pos: btn_center,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::default(),
            },
        ],
        ..Default::default()
    };

    let mut returned_inputs = Vec::new();
    let output = ctx.run_ui(raw_input, |ui| {
        egui::CentralPanel::default().show(ui, |ui| {
            returned_inputs = view.show(ui, &player, None);
        });
    });
    output.drop_without_applying_deltas();

    assert!(
        returned_inputs.contains(&Input::Click {
            object: "btn-about".to_owned()
        }),
        "expected returned inputs to contain Click btn-about, got: {returned_inputs:?}"
    );
}

#[test]
fn paint_smoke_and_highlight() {
    let course = Course::from_json(HELLO_JSON).expect("valid course");
    let mut player = Player::new(&course).expect("valid player");
    player.handle(&Input::Start);

    let items = compose(&player);
    let ctx = egui::Context::default();
    let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 720.0));
    let t = SlideTransform::fit(screen, player.course().slide_size).expect("fit");

    let raw_input = egui::RawInput::default();
    let output = ctx.run_ui(raw_input, |ui| {
        let painter = ui.painter();
        let highlight = Highlight {
            hovered: Some(Hit::Object("btn-about".to_owned())),
            pressed: Some(Hit::Object("btn-start".to_owned())),
        };
        paint(painter, &t, &items, highlight, None, None, None);
    });
    output.drop_without_applying_deltas();
}

#[test]
fn essay_paint_puts_ink_on_more_than_one_line() {
    use crate::offscreen::render_ui;
    use crate::paint::{Highlight, paint};

    let course = course_with_question(serde_json::json!({
        "kind": "essay",
        "prompt": "Tell us more",
        "choices": []
    }));
    let mut player = Player::new(&course).expect("player");
    player.handle(&Input::Start);
    player.handle(&Input::TextEntered {
        text: "word ".repeat(80),
    });
    let items = compose(&player);
    let field = items
        .iter()
        .find(|item| matches!(item.visual, Visual::FillIn { .. }))
        .expect("essay fill-in");
    let rect = field.rect;
    let img = render_ui(
        1280,
        720,
        1.0,
        |_ctx| {},
        |ctx| {
            ctx.root_ui(|ui| {
                let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 720.0));
                let t = SlideTransform::fit(screen, course.slide_size).expect("fit");
                let painter = ui.painter();
                paint(painter, &t, &items, Highlight::default(), None, None, None);
            });
        },
    )
    .expect("essay render");

    let dark_in_band = |y0: u32, y1: u32| -> bool {
        for y in y0..y1 {
            for x in (rect.min.x as u32)..(rect.max.x as u32) {
                let index = ((y as usize) * (img.width as usize) + x as usize) * 4;
                let (r, g, b) = (
                    img.pixels.get(index).copied().unwrap_or(255),
                    img.pixels.get(index + 1).copied().unwrap_or(255),
                    img.pixels.get(index + 2).copied().unwrap_or(255),
                );
                if r < 128 && g < 128 && b < 128 {
                    return true;
                }
            }
        }
        false
    };
    let top = rect.min.y as u32;
    assert!(
        dark_in_band(top + 8, top + 40),
        "first line of the essay box must have ink"
    );
    assert!(
        dark_in_band(top + 40, top + 80),
        "second line of the essay box must have ink"
    );
}

#[test]
fn hit_test_uses_the_exact_rotated_rectangle() {
    let mut item = make_dummy_item(
        Hit::Object("rotated".to_owned()),
        Rect::from_min_size(pos2(100.0, 100.0), vec2(100.0, 100.0)),
    );
    item.rotation = 45.0;

    // Inside the rotated frame but outside the unrotated one.
    assert!(
        crate::compose::item_hit_test(&item, pos2(150.0, 85.0)),
        "a point in the rotated frame's top corner must hit"
    );
    assert!(
        !item.rect.contains(pos2(150.0, 85.0)),
        "the unrotated frame must miss that point"
    );

    // Inside the unrotated frame but outside the rotated one.
    assert!(
        item.rect.contains(pos2(105.0, 105.0)),
        "the unrotated frame contains its corner"
    );
    assert!(
        !crate::compose::item_hit_test(&item, pos2(105.0, 105.0)),
        "the rotated frame must miss its old corner"
    );

    let bounds = crate::compose::item_bounds(&item);
    assert!(bounds.width() > item.rect.width());
    assert!(bounds.height() > item.rect.height());
}

#[test]
fn paint_button_hover_down_and_disabled_fills() {
    let base_fill = Color32::from_rgb(47, 111, 235);
    let item_btn = DrawItem {
        hit: Hit::Object("btn-1".to_owned()),
        rect: Rect::from_min_size(pos2(100.0, 100.0), vec2(200.0, 60.0)),
        visual: Visual::Button {
            label: "Test".to_owned(),
            fill: base_fill,
            fill_style: None,
            hover_fill: None,
            down_fill: None,
            disabled: false,
        },
        dimmed: false,
        opacity: 1.0,
        clip: None,
        rotation: 0.0,
        flip_h: false,
        flip_v: false,
        shadow: None,
        glow: None,
        media_time_ms: None,
        links: Vec::new(),
        show_table_overflow: false,
        clip_rect: None,
        mark: None,
    };
    let item_btn_disabled = DrawItem {
        hit: Hit::Object("btn-dis".to_owned()),
        rect: Rect::from_min_size(pos2(100.0, 200.0), vec2(200.0, 60.0)),
        visual: Visual::Button {
            label: "Disabled".to_owned(),
            fill: base_fill,
            fill_style: None,
            hover_fill: None,
            down_fill: None,
            disabled: true,
        },
        dimmed: false,
        opacity: 1.0,
        clip: None,
        rotation: 0.0,
        flip_h: false,
        flip_v: false,
        shadow: None,
        glow: None,
        media_time_ms: None,
        links: Vec::new(),
        show_table_overflow: false,
        clip_rect: None,
        mark: None,
    };

    let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 720.0));
    let t = SlideTransform::fit(screen, Size { w: 1280, h: 720 }).expect("fit");
    let ctx = egui::Context::default();

    // 1. Normal: fill is base_fill
    let output1 = ctx.run_ui(egui::RawInput::default(), |ui| {
        let painter = ui.painter();
        paint(
            painter,
            &t,
            std::slice::from_ref(&item_btn),
            Highlight::default(),
            None,
            None,
            None,
        );
        let mut found_fill = None;
        painter.for_each_shape(|clipped| {
            if let egui::Shape::Rect(ref r) = clipped.shape
                && r.rect == item_btn.rect
            {
                found_fill = Some(r.fill);
            }
        });
        assert_eq!(found_fill, Some(base_fill));
    });
    output1.drop_without_applying_deltas();

    // 2. Hovered: fill lightened by 12%
    let output2 = ctx.run_ui(egui::RawInput::default(), |ui| {
        let painter = ui.painter();
        paint(
            painter,
            &t,
            std::slice::from_ref(&item_btn),
            Highlight {
                hovered: Some(Hit::Object("btn-1".to_owned())),
                pressed: None,
            },
            None,
            None,
            None,
        );
        let mut found_fill = None;
        painter.for_each_shape(|clipped| {
            if let egui::Shape::Rect(ref r) = clipped.shape
                && r.rect == item_btn.rect
            {
                found_fill = Some(r.fill);
            }
        });
        assert_eq!(found_fill, Some(Color32::from_rgb(72, 128, 237)));
    });
    output2.drop_without_applying_deltas();

    // 3. Down (pressed): fill darkened by 15%
    let output3 = ctx.run_ui(egui::RawInput::default(), |ui| {
        let painter = ui.painter();
        paint(
            painter,
            &t,
            std::slice::from_ref(&item_btn),
            Highlight {
                hovered: None,
                pressed: Some(Hit::Object("btn-1".to_owned())),
            },
            None,
            None,
            None,
        );
        let mut found_fill = None;
        painter.for_each_shape(|clipped| {
            if let egui::Shape::Rect(ref r) = clipped.shape
                && r.rect == item_btn.rect
            {
                found_fill = Some(r.fill);
            }
        });
        assert_eq!(found_fill, Some(Color32::from_rgb(40, 94, 200)));
    });
    output3.drop_without_applying_deltas();

    // 4. Disabled: fill remains base_fill even if hovered and pressed
    let output4 = ctx.run_ui(egui::RawInput::default(), |ui| {
        let painter = ui.painter();
        paint(
            painter,
            &t,
            std::slice::from_ref(&item_btn_disabled),
            Highlight {
                hovered: Some(Hit::Object("btn-dis".to_owned())),
                pressed: Some(Hit::Object("btn-dis".to_owned())),
            },
            None,
            None,
            None,
        );
        let mut found_fill = None;
        painter.for_each_shape(|clipped| {
            if let egui::Shape::Rect(ref r) = clipped.shape
                && r.rect == item_btn_disabled.rect
            {
                found_fill = Some(r.fill);
            }
        });
        assert_eq!(found_fill, Some(base_fill));
    });
    output4.drop_without_applying_deltas();
}

#[test]
fn view_show_with_zero_or_hostile_size_returns_no_inputs() {
    let course = Course::from_json(HELLO_JSON).expect("valid course");
    let mut player = Player::new(&course).expect("valid player");
    player.handle(&Input::Start);

    let mut view = SlideView::default();
    let ctx = egui::Context::default();

    let raw_input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(0.0, 0.0))),
        ..Default::default()
    };
    let output = ctx.run_ui(raw_input, |ui| {
        let inputs = view.show(ui, &player, None);
        assert!(inputs.is_empty());
    });
    output.drop_without_applying_deltas();
}

#[test]
fn pointer_covered_press_and_release_emits_no_click() {
    let slide_size = Size { w: 1280, h: 720 };
    let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 720.0));
    let t = SlideTransform::fit(screen, slide_size).expect("fit");
    let items = vec![make_dummy_item(
        Hit::Object("btn".to_owned()),
        Rect::from_min_size(pos2(100.0, 100.0), vec2(200.0, 60.0)),
    )];

    let mut state = PointerState::default();
    let pos = pos2(150.0, 130.0);
    let button = |pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::default(),
    };
    let events = vec![button(true), button(false)];

    let inputs = state.handle_events(&events, &t, &items, &|_| true);
    assert!(inputs.is_empty(), "got {inputs:?}");
    assert_eq!(state.pressed, None);
}

#[test]
fn pointer_covered_move_over_hovered_object_emits_hover_end() {
    let slide_size = Size { w: 1280, h: 720 };
    let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 720.0));
    let t = SlideTransform::fit(screen, slide_size).expect("fit");
    let items = vec![make_dummy_item(
        Hit::Object("btn".to_owned()),
        Rect::from_min_size(pos2(100.0, 100.0), vec2(200.0, 60.0)),
    )];

    let mut state = PointerState::default();
    let pos = pos2(150.0, 130.0);
    let mut inputs = Vec::new();
    state.handle_event(
        &egui::Event::PointerMoved(pos),
        &t,
        &items,
        &|_| false,
        &mut inputs,
    );
    assert_eq!(state.hovered, Some(Hit::Object("btn".to_owned())));

    inputs.clear();
    state.handle_event(
        &egui::Event::PointerMoved(pos),
        &t,
        &items,
        &|_| true,
        &mut inputs,
    );
    assert_eq!(
        inputs,
        vec![Input::HoverEnd {
            object: "btn".to_owned()
        }]
    );
    assert_eq!(state.hovered, None);
}

// ---------------------------------------------------------------- chrome tests

use crate::chrome::{format_ms, nav_limits, progress_fraction};

#[test]
fn format_ms_formats_times_correctly() {
    assert_eq!(format_ms(0), "0:00");
    assert_eq!(format_ms(500), "0:00");
    assert_eq!(format_ms(999), "0:00");
    assert_eq!(format_ms(1000), "0:01");
    assert_eq!(format_ms(5000), "0:05");
    assert_eq!(format_ms(59000), "0:59");
    assert_eq!(format_ms(60000), "1:00");
    assert_eq!(format_ms(65000), "1:05");
    assert_eq!(format_ms(125000), "2:05");
    assert_eq!(format_ms(3599000), "59:59");
    assert_eq!(format_ms(3600000), "60:00");
}

#[test]
fn progress_fraction_guards_zero_duration_and_clamps() {
    assert_eq!(progress_fraction(0, 0), 0.0);
    assert_eq!(progress_fraction(1000, 0), 0.0);
    assert_eq!(progress_fraction(0, 5000), 0.0);
    assert_eq!(progress_fraction(2500, 5000), 0.5);
    assert_eq!(progress_fraction(5000, 5000), 1.0);
    assert_eq!(progress_fraction(6000, 5000), 1.0);
}

#[test]
fn nav_limits_detects_first_and_last_slides() {
    // Empty
    assert_eq!(nav_limits(std::iter::empty::<&str>(), "a"), (true, true));

    // Single slide
    assert_eq!(nav_limits(["intro"], "intro"), (true, true));

    // Two slides
    assert_eq!(nav_limits(["intro", "outro"], "intro"), (true, false));
    assert_eq!(nav_limits(["intro", "outro"], "outro"), (false, true));

    // Three slides
    let slides = ["intro", "tabs", "quiz"];
    assert_eq!(nav_limits(slides, "intro"), (true, false));
    assert_eq!(nav_limits(slides, "tabs"), (false, false));
    assert_eq!(nav_limits(slides, "quiz"), (false, true));

    // Unknown slide
    assert_eq!(nav_limits(slides, "missing"), (true, true));
}

// ---------------------------------------------------------------- authoring compose tests

#[test]
fn compose_draws_reverse_shown_layers_in_authored_order() {
    let mut course = Course::from_json(HELLO_JSON).expect("valid course");
    if let Some(slide) = course
        .scenes
        .get_mut(0)
        .and_then(|scene| scene.slides.get_mut(1))
    {
        for layer in &mut slide.layers {
            layer.hide_other_layers = false;
        }
    }
    let mut player = Player::new(&course).expect("valid player");
    player.handle(&Input::Start);
    player.handle(&Input::Tick { ms: 1000 });
    player.handle(&Input::Click {
        object: "btn-start".to_owned(),
    });
    player.handle(&Input::Click {
        object: "tab3".to_owned(),
    });
    player.handle(&Input::Click {
        object: "tab1".to_owned(),
    });
    let items = compose(&player);
    let back = items
        .iter()
        .position(|item| item.hit == Hit::Object("l1-text".to_owned()))
        .expect("l1 text");
    let front = items
        .iter()
        .position(|item| item.hit == Hit::Object("l3-text".to_owned()))
        .expect("l3 text");
    assert!(back < front, "draw order follows slide.layers");
}

#[test]
fn compose_authoring_s_intro_ignores_timing_and_has_three_items() {
    let course = Course::from_json(HELLO_JSON).expect("valid course");
    let slide = course.slide("s-intro").expect("s-intro exists");
    let items = compose_authoring(&course, slide, None);
    assert_eq!(items.len(), 3);
    assert!(
        items
            .iter()
            .any(|i| i.hit == Hit::Object("btn-start".to_owned()))
    );
    assert!(items.iter().all(|i| !i.dimmed));
}

#[test]
fn compose_authoring_s_intro_with_focus_layer_dims_base_items() {
    let course = Course::from_json(HELLO_JSON).expect("valid course");
    let slide = course.slide("s-intro").expect("s-intro exists");
    let items = compose_authoring(&course, slide, Some("about"));
    assert_eq!(items.len(), 6);
    assert!(items[0..3].iter().all(|i| i.dimmed));
    assert!(items[3..6].iter().all(|i| !i.dimmed));
}

#[test]
fn compose_authoring_extra_layers_draw_in_authored_order_without_hits() {
    let course = Course::from_json(HELLO_JSON).expect("valid course");
    let slide = course.slide("s-tabs").expect("s-tabs exists");
    let extras = vec!["l1".to_owned(), "l3".to_owned()];
    let items = compose_authoring_layers(&course, slide, Some("l2"), &extras, true);

    let layer_text = |id: &str| {
        items
            .iter()
            .position(|item| match &item.visual {
                Visual::Text { runs, .. } => runs.iter().any(|run| run.text.contains(id)),
                _ => false,
            })
            .expect("layer text")
    };
    assert!(layer_text("Wipe spills") < layer_text("Know your two nearest exits"));
    assert!(layer_text("Know your two nearest exits") < layer_text("Never overload a socket"));

    for item in &items {
        let focused = matches!(&item.visual, Visual::Text { runs, .. }
            if runs.iter().any(|run| run.text.contains("Know your two nearest exits")));
        if focused {
            assert!(!item.dimmed);
            assert!(matches!(item.hit, Hit::Object(_)));
        } else if matches!(&item.visual, Visual::Text { .. }) {
            assert!(item.dimmed, "extra layers are dimmed");
            assert_eq!(item.hit, Hit::None, "extra layers take no hits");
        }
    }
}

#[test]
fn compose_authoring_dim_off_leaves_base_undimmed() {
    let course = Course::from_json(HELLO_JSON).expect("valid course");
    let slide = course.slide("s-intro").expect("s-intro exists");
    let items = compose_authoring_layers(&course, slide, Some("about"), &[], false);
    let base = items
        .iter()
        .find(|item| matches!(&item.visual, Visual::Button { label, .. } if label == "Start"))
        .expect("base Start");
    assert!(!base.dimmed, "Dim off leaves base objects undimmed");
}

#[test]
fn compose_authoring_s_tabs_btn_next_uses_initial_disabled_fill() {
    let course = Course::from_json(HELLO_JSON).expect("valid course");
    let slide = course.slide("s-tabs").expect("s-tabs exists");
    let items = compose_authoring(&course, slide, None);
    let next = items
        .iter()
        .find(|i| i.hit == Hit::Object("btn-next".to_owned()))
        .expect("btn-next found");
    match &next.visual {
        Visual::Button { fill, disabled, .. } => {
            assert!(*disabled);
            assert_eq!(*fill, Color32::from_rgb(0xB0, 0xB0, 0xB0));
        }
        other => panic!("expected Button visual, got {other:?}"),
    }
}

#[test]
fn compose_authoring_s_quiz_prompt_and_choices_none_selected() {
    let course = Course::from_json(HELLO_JSON).expect("valid course");
    let slide = course.slide("s-quiz").expect("s-quiz exists");
    let items = compose_authoring(&course, slide, None);
    let prompt_count = items
        .iter()
        .filter(|i| matches!(i.visual, Visual::Prompt { .. }))
        .count();
    let choice_items: Vec<_> = items
        .iter()
        .filter_map(|i| match &i.visual {
            Visual::ChoiceRow { selected, .. } => Some(*selected),
            _ => None,
        })
        .collect();
    assert_eq!(prompt_count, 1);
    assert_eq!(choice_items.len(), 3);
    assert!(choice_items.iter().all(|selected| !*selected));
}

#[test]
fn compose_authoring_unknown_focus_layer_is_ignored() {
    let course = Course::from_json(HELLO_JSON).expect("valid course");
    let slide = course.slide("s-intro").expect("s-intro exists");
    let items_none = compose_authoring(&course, slide, None);
    let items_unknown = compose_authoring(&course, slide, Some("nonexistent_layer"));
    assert_eq!(items_none, items_unknown);
}

#[test]
fn compose_authoring_invalid_course_still_composes() {
    let mut course = Course::from_json(HELLO_JSON).expect("valid course");
    if let Some(slide) = course.scenes.get_mut(0).and_then(|s| s.slides.get_mut(0)) {
        slide.triggers.push(coursecraft_course::Trigger {
            action: coursecraft_course::Action::ShowLayer {
                layer: "missing_layer_id".to_owned(),
            },
            when: coursecraft_course::Event::Click {
                object: "btn-start".to_owned(),
            },
            condition: None,
            disabled: false,
        });
    }
    assert!(Player::new(&course).is_err());
    let slide = course.slide("s-intro").expect("slide exists");
    let items = compose_authoring(&course, slide, None);
    assert_eq!(items.len(), 3);
}

#[test]
fn compose_player_items_never_dimmed() {
    let course = Course::from_json(HELLO_JSON).expect("valid course");
    let mut player = Player::new(&course).expect("valid player");
    player.handle(&Input::Start);
    let items = compose(&player);
    assert!(!items.is_empty());
    assert!(items.iter().all(|i| !i.dimmed));
}

#[test]
fn compose_authoring_hidden_initial_state_is_dimmed() {
    let mut course = Course::from_json(HELLO_JSON).expect("valid course");
    if let Some(slide) = course.scenes.get_mut(0).and_then(|s| s.slides.get_mut(0))
        && let Some(obj) = slide.objects.get_mut(0)
    {
        obj.initial_state = "hidden".to_owned();
    }
    let slide = course.slide("s-intro").expect("slide exists");
    let items = compose_authoring(&course, slide, None);
    assert_eq!(items.len(), 3);
    assert!(items[0].dimmed);
    assert!(!items[1].dimmed);
    assert!(!items[2].dimmed);
}

#[test]
fn compose_object_hidden_state_resolves_item() {
    let course = Course::from_json(HELLO_JSON).expect("valid course");
    let slide = course.slide("s-intro").expect("s-intro exists");
    let obj = slide.objects.first().expect("first object");
    let item = compose_object(obj, "hidden").expect("resolves item");
    assert_eq!(item.hit, Hit::Object(obj.id.clone()));
    assert!(!item.dimmed);
}

#[test]
fn paint_smoke_dimmed_item_does_not_panic() {
    let base_fill = Color32::from_rgb(47, 111, 235);
    let dimmed_item = DrawItem {
        hit: Hit::Object("btn-dimmed".to_owned()),
        rect: Rect::from_min_size(pos2(100.0, 100.0), vec2(200.0, 60.0)),
        visual: Visual::Button {
            label: "Dimmed Button".to_owned(),
            fill: base_fill,
            fill_style: None,
            hover_fill: None,
            down_fill: None,
            disabled: false,
        },
        dimmed: true,
        opacity: 1.0,
        clip: None,
        rotation: 0.0,
        flip_h: false,
        flip_v: false,
        shadow: None,
        glow: None,
        media_time_ms: None,
        links: Vec::new(),
        show_table_overflow: false,
        clip_rect: None,
        mark: None,
    };
    let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 720.0));
    let t = SlideTransform::fit(screen, Size { w: 1280, h: 720 }).expect("fit");
    let ctx = egui::Context::default();
    let output = ctx.run_ui(egui::RawInput::default(), |ui| {
        let painter = ui.painter();
        paint(
            painter,
            &t,
            std::slice::from_ref(&dimmed_item),
            Highlight::default(),
            None,
            None,
            None,
        );
        let mut found_fill = None;
        painter.for_each_shape(|clipped| {
            if let egui::Shape::Rect(ref r) = clipped.shape
                && r.rect == dimmed_item.rect
            {
                found_fill = Some(r.fill);
            }
        });
        assert_eq!(found_fill, Some(base_fill.gamma_multiply(0.35)));
    });
    output.drop_without_applying_deltas();
}

struct FakeMediaSource {
    files: std::collections::HashMap<String, std::sync::Arc<[u8]>>,
    queries: std::sync::atomic::AtomicUsize,
}

impl FakeMediaSource {
    fn new() -> Self {
        Self {
            files: std::collections::HashMap::new(),
            queries: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    fn add(&mut self, src: &str, bytes: Vec<u8>) {
        self.files
            .insert(src.to_string(), std::sync::Arc::from(bytes));
    }
}

impl crate::media::MediaSource for FakeMediaSource {
    fn get(&self, src: &str) -> crate::media::MediaFetch {
        self.queries
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        match self.files.get(src) {
            Some(bytes) => crate::media::MediaFetch::Ready(std::sync::Arc::clone(bytes)),
            None => crate::media::MediaFetch::Missing,
        }
    }
}

fn make_2x2_png() -> Vec<u8> {
    use coursecraft_media::image::{ImageBuffer, ImageFormat, Rgba};
    use std::io::Cursor;
    let mut img = ImageBuffer::<Rgba<u8>, Vec<u8>>::new(2, 2);
    img.put_pixel(0, 0, Rgba([255, 0, 0, 255]));
    img.put_pixel(1, 0, Rgba([0, 255, 0, 255]));
    img.put_pixel(0, 1, Rgba([0, 0, 255, 255]));
    img.put_pixel(1, 1, Rgba([255, 255, 255, 255]));
    let mut bytes = Vec::new();
    img.write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
        .expect("PNG write");
    bytes
}

#[test]
fn image_state_src_overrides_base_src_keeping_alt() {
    let course_json = r##"{
      "schema_version": 1,
      "id": "course-img-state",
      "title": "Image State Test",
      "slide_size": { "w": 1280, "h": 720 },
      "scenes": [{
        "id": "scene-1",
        "title": "Scene 1",
        "slides": [{
          "id": "slide-1",
          "title": "Slide 1",
          "objects": [{
            "id": "img1",
            "name": "Photo",
            "kind": { "type": "image", "src": "a.png", "alt": "A photo" },
            "frame": { "x": 50, "y": 50, "w": 400, "h": 300 },
            "states": [{ "name": "visited", "src": "b.png" }]
          }]
        }]
      }]
    }"##;
    let course = Course::from_json(course_json).expect("valid course");
    let obj = course
        .slide("slide-1")
        .expect("slide")
        .objects
        .first()
        .expect("object");
    let visual = |state: &str| compose_object(obj, state).expect("item").visual;
    assert_eq!(
        visual("visited"),
        Visual::Image {
            crop: None,
            src: "b.png".into(),
            alt: "A photo".into()
        }
    );
    for state in ["normal", "undefined-state"] {
        assert_eq!(
            visual(state),
            Visual::Image {
                crop: None,
                src: "a.png".into(),
                alt: "A photo".into()
            }
        );
    }
}

#[test]
fn compose_produces_visual_image() {
    let course_json = r##"{
      "schema_version": 1,
      "id": "course-img",
      "title": "Image Test",
      "slide_size": { "w": 1280, "h": 720 },
      "scenes": [
        {
          "id": "scene-1",
          "title": "Scene 1",
          "slides": [
            {
              "id": "slide-1",
              "title": "Slide 1",
              "objects": [
                {
                  "id": "img1",
                  "name": "Photo",
                  "kind": {
                    "type": "image",
                    "src": "picture.png",
                    "alt": "A mountain view"
                  },
                  "frame": { "x": 50, "y": 50, "w": 400, "h": 300 }
                }
              ]
            }
          ]
        }
      ]
    }"##;

    let course = Course::from_json(course_json).expect("valid course");
    let player = Player::new(&course).expect("player new");
    let items = compose(&player);
    let image_item = items
        .iter()
        .find(|item| item.hit == Hit::Object("img1".to_string()))
        .expect("image item should be composed");

    assert_eq!(
        image_item.visual,
        Visual::Image {
            crop: None,
            src: "picture.png".to_string(),
            alt: "A mountain view".to_string(),
        }
    );
}

#[test]
fn an_uncropped_image_is_aspect_fit_and_centred_in_its_frame() {
    // A 2:1 picture in a 4:1 frame keeps its height and gets bars at the sides.
    let wide_frame = Rect::from_min_size(pos2(10.0, 20.0), vec2(400.0, 100.0));
    assert_eq!(
        crate::paint::image_dest_rect(wide_frame, (200.0, 100.0), None),
        Some(Rect::from_min_max(pos2(110.0, 20.0), pos2(310.0, 120.0)))
    );

    // The same picture in a tall 1:4 frame keeps its width and is centred
    // vertically.
    let tall_frame = Rect::from_min_size(pos2(10.0, 20.0), vec2(100.0, 400.0));
    assert_eq!(
        crate::paint::image_dest_rect(tall_frame, (200.0, 100.0), None),
        Some(Rect::from_min_max(pos2(10.0, 195.0), pos2(110.0, 245.0)))
    );
}

#[test]
fn a_crop_fills_the_frame() {
    let frame = Rect::from_min_size(pos2(10.0, 20.0), vec2(400.0, 100.0));
    let crop = Rect::from_min_max(pos2(0.25, 0.1), pos2(0.75, 0.9));
    assert_eq!(
        crate::paint::image_dest_rect(frame, (200.0, 100.0), Some(crop)),
        Some(frame)
    );
}

#[test]
fn a_full_crop_composes_exactly_like_no_crop() {
    let json = |crop: &str| {
        format!(
            r##"{{
              "schema_version": 1,
              "id": "course-crop",
              "title": "Crop",
              "slide_size": {{ "w": 1280, "h": 720 }},
              "scenes": [{{
                "id": "scene-1",
                "title": "Scene",
                "slides": [{{
                  "id": "slide-1",
                  "title": "Slide",
                  "objects": [{{
                    "id": "img1",
                    "kind": {{ "type": "image", "src": "picture.png", "alt": "A view"{crop} }},
                    "frame": {{ "x": 50, "y": 50, "w": 400, "h": 300 }}
                  }}]
                }}]
              }}]
            }}"##
        )
    };
    let visual = |text: String| {
        let course = Course::from_json(&text).expect("valid course");
        compose_object(
            course
                .slide("slide-1")
                .expect("slide")
                .objects
                .first()
                .expect("image"),
            "normal",
        )
        .expect("item")
        .visual
    };
    let none = visual(json(""));
    let full = visual(json(r#", "crop": { "x": 0, "y": 0, "w": 1, "h": 1 }"#));
    assert_eq!(none, full);
    assert_eq!(
        full,
        Visual::Image {
            crop: None,
            src: "picture.png".into(),
            alt: "A view".into(),
        }
    );
}

#[test]
fn a_crop_passes_through_to_the_visual() {
    let text = r##"{
      "schema_version": 1,
      "id": "course-crop",
      "title": "Crop",
      "slide_size": { "w": 1280, "h": 720 },
      "scenes": [{
        "id": "scene-1",
        "title": "Scene",
        "slides": [{
          "id": "slide-1",
          "title": "Slide",
          "objects": [{
            "id": "img1",
            "kind": {
              "type": "image",
              "src": "picture.png",
              "alt": "A view",
              "crop": { "x": 0.25, "y": 0.1, "w": 0.5, "h": 0.8 }
            },
            "frame": { "x": 50, "y": 50, "w": 400, "h": 300 }
          }]
        }]
      }]
    }"##;
    let course = Course::from_json(text).expect("valid course");
    let visual = compose_object(
        course
            .slide("slide-1")
            .expect("slide")
            .objects
            .first()
            .expect("image"),
        "normal",
    )
    .expect("item")
    .visual;
    assert_eq!(
        visual,
        Visual::Image {
            crop: Some(Rect::from_min_max(
                pos2(0.25, 0.1),
                pos2(0.25 + 0.5, 0.1 + 0.8)
            )),
            src: "picture.png".into(),
            alt: "A view".into(),
        }
    );
}

#[test]
fn a_hostile_crop_is_ignored_by_the_composer() {
    for crop in [
        ModelRect {
            x: -0.5,
            y: 0.0,
            w: 1.0,
            h: 1.0,
        },
        ModelRect {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 1.0,
        },
        ModelRect {
            x: 0.5,
            y: 0.0,
            w: 0.75,
            h: 1.0,
        },
        ModelRect {
            x: f32::NAN,
            y: 0.0,
            w: 0.5,
            h: 0.5,
        },
    ] {
        assert!(
            crate::compose::valid_crop_rect(&coursecraft_course::CropRect {
                x: crop.x,
                y: crop.y,
                w: crop.w,
                h: crop.h,
            })
            .is_none(),
            "hostile crop was accepted: {crop:?}"
        );
    }
}

#[test]
fn painting_registers_texture_caches_failure_and_retain_drops() {
    use std::collections::HashSet;

    let png_bytes = make_2x2_png();
    let mut source = FakeMediaSource::new();
    source.add("photo.png", png_bytes);

    let mut cache = crate::media::MediaCache::new();

    let items = vec![
        DrawItem {
            hit: Hit::Object("img-good".to_string()),
            rect: Rect::from_min_size(pos2(10.0, 10.0), vec2(100.0, 100.0)),
            visual: Visual::Image {
                crop: None,
                src: "photo.png".to_string(),
                alt: "Good Image".to_string(),
            },
            dimmed: false,
            opacity: 1.0,
            clip: None,
            rotation: 0.0,
            flip_h: false,
            flip_v: false,
            shadow: None,
            glow: None,
            media_time_ms: None,
            links: Vec::new(),
            show_table_overflow: false,
            clip_rect: None,
            mark: None,
        },
        DrawItem {
            hit: Hit::Object("img-missing".to_string()),
            rect: Rect::from_min_size(pos2(120.0, 10.0), vec2(100.0, 100.0)),
            visual: Visual::Image {
                crop: None,
                src: "missing.png".to_string(),
                alt: "Missing Image".to_string(),
            },
            dimmed: false,
            opacity: 1.0,
            clip: None,
            rotation: 0.0,
            flip_h: false,
            flip_v: false,
            shadow: None,
            glow: None,
            media_time_ms: None,
            links: Vec::new(),
            show_table_overflow: false,
            clip_rect: None,
            mark: None,
        },
    ];

    let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 720.0));
    let t = SlideTransform::fit(screen, Size { w: 1280, h: 720 }).expect("fit");
    let ctx = egui::Context::default();
    let mut recorded_handle_id = None;

    // 1. First paint: good.png loads and registers texture, missing.png fails
    let output1 = ctx.run_ui(egui::RawInput::default(), |ui| {
        let painter = ui.painter();
        paint(
            painter,
            &t,
            &items,
            Highlight::default(),
            Some((&mut cache, &source)),
            None,
            None,
        );

        let entry = cache.get_entry("photo.png").expect("photo.png entry");
        let handle_id = match entry {
            crate::media::MediaEntry::Texture(handle, size) => {
                assert_eq!(*size, [2, 2]);
                handle.id()
            }
            _ => panic!("expected Texture entry"),
        };

        let missing_entry = cache.get_entry("missing.png").expect("missing.png entry");
        assert!(matches!(missing_entry, crate::media::MediaEntry::Failed(_)));

        recorded_handle_id = Some(handle_id);
    });
    output1.drop_without_applying_deltas();

    let handle_id = recorded_handle_id.expect("handle id recorded");

    let queries_after_first = source.queries.load(std::sync::atomic::Ordering::SeqCst);
    assert_eq!(queries_after_first, 2);

    // 2. Second paint: failure and texture are cached, no new queries to source
    let output2 = ctx.run_ui(egui::RawInput::default(), |ui| {
        let painter = ui.painter();
        paint(
            painter,
            &t,
            &items,
            Highlight::default(),
            Some((&mut cache, &source)),
            None,
            None,
        );
    });
    output2.drop_without_applying_deltas();

    let queries_after_second = source.queries.load(std::sync::atomic::Ordering::SeqCst);
    assert_eq!(queries_after_second, 2); // Unchanged!

    // Check texture is alive in texture manager
    assert!(ctx.tex_manager().read().meta(handle_id).is_some());

    // 3. Retain drops unused textures
    let empty_set: HashSet<String> = HashSet::new();
    cache.retain(&empty_set);
    assert!(cache.get_entry("photo.png").is_none());
    assert!(cache.get_entry("missing.png").is_none());
    assert!(ctx.tex_manager().read().meta(handle_id).is_none());
}

#[test]
fn compose_audio_and_video() {
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "c1",
        "title": "Course",
        "scenes": [{
            "id": "sc1",
            "title": "Scene",
            "slides": [{
                "id": "s1",
                "title": "Slide 1",
                "duration_ms": 5000,
                "objects": [
                    {
                        "id": "vid1",
                        "kind": { "type": "video", "src": "sample.mp4", "autoplay": false },
                        "frame": { "x": 100.0, "y": 100.0, "w": 640.0, "h": 360.0 },
                        "timing": { "start_ms": 0 }
                    },
                    {
                        "id": "aud1",
                        "kind": { "type": "audio", "src": "music.mp3", "autoplay": false },
                        "frame": { "x": 50.0, "y": 50.0, "w": 48.0, "h": 48.0 },
                        "timing": { "start_ms": 0 }
                    }
                ]
            }]
        }]
    });

    let course = Course::from_json(&json.to_string()).unwrap();
    let player = coursecraft_logic::Player::new(&course).unwrap();

    // In play time compose: video is composed, audio is invisible
    let items = compose(&player);
    assert!(
        items.iter().any(
            |item| matches!(item.visual, Visual::Video { ref src, .. } if src == "sample.mp4")
        )
    );
    assert!(
        !items
            .iter()
            .any(|item| matches!(item.visual, Visual::Audio { .. }))
    );

    // In authoring compose: both video and audio are composed (audio as speaker icon box)
    let slide = course.slides().next().unwrap();
    let auth_items = compose_authoring(&course, slide, None);
    assert!(
        auth_items.iter().any(
            |item| matches!(item.visual, Visual::Video { ref src, .. } if src == "sample.mp4")
        )
    );
    assert!(
        auth_items
            .iter()
            .any(|item| matches!(item.visual, Visual::Audio { ref src } if src == "music.mp3"))
    );
}

// ---------------------------------------------------------------- media driver tests

mod media_driver {
    use std::cell::Cell;
    use std::sync::Arc;

    use coursecraft_course::{BACKGROUND_AUDIO_ID, Course};
    use coursecraft_logic::{Effect, Input, MediaOp, Player};
    use coursecraft_playback::{MediaBackend, NullBackend};

    use crate::compose::{Hit, compose};
    use crate::media::{MediaFetch, MediaSource};
    use crate::media_driver::{MediaDriver, collect_video_ids, find_media_target};

    /// Two slides that both own an audio object `audio1`, with different sources.
    const REPEAT_IDS_JSON: &str = r#"{
      "schema_version": 1,
      "id": "c",
      "title": "Repeat ids",
      "slide_size": { "w": 960, "h": 540 },
      "scenes": [{
        "id": "sc",
        "title": "Scene",
        "slides": [
          {
            "id": "s1",
            "title": "One",
            "duration_ms": 5000,
            "objects": [{
              "id": "audio1", "name": "A",
              "kind": { "type": "audio", "src": "one.wav", "autoplay": true },
              "frame": { "x": 0.0, "y": 0.0, "w": 48.0, "h": 48.0 }
            }],
            "triggers": [{
              "when": { "event": "media_ends", "object": "audio1" },
              "action": { "action": "jump_to_slide", "target": { "id": "s2" } }
            }]
          },
          {
            "id": "s2",
            "title": "Two",
            "duration_ms": 5000,
            "objects": [{
              "id": "audio1", "name": "A",
              "kind": { "type": "audio", "src": "two.wav", "autoplay": true },
              "frame": { "x": 0.0, "y": 0.0, "w": 48.0, "h": 48.0 }
            }]
          }
        ]
      }]
    }"#;

    struct AlwaysReady;

    impl MediaSource for AlwaysReady {
        fn get(&self, _src: &str) -> MediaFetch {
            MediaFetch::Ready(Arc::from(vec![0u8; 4]))
        }
    }

    /// Pending until `ready` is set.
    struct Gate {
        ready: Cell<bool>,
    }

    impl MediaSource for Gate {
        fn get(&self, _src: &str) -> MediaFetch {
            if self.ready.get() {
                MediaFetch::Ready(Arc::from(vec![0u8; 4]))
            } else {
                MediaFetch::Pending
            }
        }
    }

    fn media_effect(object: &str, op: MediaOp) -> Effect {
        Effect::Media {
            object: object.to_owned(),
            op,
        }
    }

    fn seek_effect(object: &str, ms: u64) -> Effect {
        Effect::MediaSeek {
            object: object.to_owned(),
            ms,
        }
    }

    #[test]
    fn lottie_media_effects_do_not_reach_the_audio_video_backend() {
        let course = Course::from_json(
            r#"{
              "schema_version": 1,
              "id": "lottie-driver",
              "title": "Lottie driver",
              "scenes": [{
                "id": "sc",
                "title": "Scene",
                "slides": [{
                  "id": "s1",
                  "title": "One",
                  "duration_ms": 5000,
                  "objects": [{
                    "id": "lot1",
                    "kind": { "type": "lottie", "src": "anim.json", "autoplay": true },
                    "frame": { "x": 0.0, "y": 0.0, "w": 100.0, "h": 100.0 }
                  }]
                }]
              }]
            }"#,
        )
        .expect("valid course");
        let mut player = Player::new(&course).expect("player");
        let backend = NullBackend::new();
        let mut driver = MediaDriver::new().with_backend(Box::new(backend.clone()));
        let effects = player.handle(&Input::Start);
        assert!(has_media_play(&effects));
        let followups = driver.apply(&player, &effects, Some(&AlwaysReady));
        assert!(
            followups.is_empty(),
            "a Lottie play must not be reported as missing media"
        );
        assert!(
            !backend
                .recorded_calls()
                .iter()
                .any(|call| call.starts_with("play:")),
            "a Lottie animation has no audio/video backend track"
        );
    }

    fn has_media_play(effects: &[Effect]) -> bool {
        effects.iter().any(|effect| {
            matches!(
                effect,
                Effect::Media {
                    op: MediaOp::Play,
                    ..
                }
            )
        })
    }

    #[test]
    fn the_driver_plays_course_background_audio_with_its_volume() {
        let course = Course::from_json(
            r#"{
              "schema_version": 1,
              "id": "background",
              "title": "Background",
              "background_audio": { "src": "music.mp3", "volume": 0.35 },
              "scenes": [{
                "id": "sc",
                "title": "Scene",
                "slides": [{ "id": "s1", "title": "One", "duration_ms": 5000 }]
              }]
            }"#,
        )
        .expect("valid course");
        let mut player = Player::new(&course).expect("player");
        assert_eq!(
            find_media_target(&player, BACKGROUND_AUDIO_ID).map(|t| t.0),
            Some("music.mp3".to_owned())
        );
        let backend = NullBackend::new();
        let mut driver = MediaDriver::new().with_backend(Box::new(backend.clone()));

        let effects = player.handle(&Input::Start);
        assert!(
            effects.contains(&media_effect(BACKGROUND_AUDIO_ID, MediaOp::Play)),
            "{effects:?}"
        );
        assert!(
            driver
                .apply(&player, &effects, Some(&AlwaysReady))
                .is_empty()
        );

        let calls = backend.recorded_calls();
        assert!(
            calls.contains(&format!("set_object_volume:{BACKGROUND_AUDIO_ID}:0.35")),
            "{calls:?}"
        );
        assert!(
            calls.contains(&format!("play:{BACKGROUND_AUDIO_ID}:music.mp3")),
            "{calls:?}"
        );
    }

    #[test]
    fn media_target_is_looked_up_on_the_current_slide_only() {
        let course = Course::from_json(REPEAT_IDS_JSON).expect("valid course");
        let s1 = course.slide("s1").expect("s1");
        let mut player = Player::new(&course).expect("player");
        assert_eq!(
            find_media_target(&player, "audio1").map(|t| t.0),
            Some("one.wav".to_owned())
        );
        assert!(find_media_target(&player, "missing").is_none());
        assert!(collect_video_ids(s1).is_empty());

        player.handle(&Input::Start);
        player.handle(&Input::Jump { slide: "s2".into() });
        assert_eq!(
            find_media_target(&player, "audio1").map(|t| t.0),
            Some("two.wav".to_owned())
        );
    }

    #[test]
    fn repeated_object_ids_play_the_current_slides_source() {
        let course = Course::from_json(REPEAT_IDS_JSON).expect("valid course");
        let mut player = Player::new(&course).expect("player");
        let backend = NullBackend::new();
        let mut driver = MediaDriver::new().with_backend(Box::new(backend.clone()));

        let effects = player.handle(&Input::Start);
        assert!(
            driver
                .apply(&player, &effects, Some(&AlwaysReady))
                .is_empty()
        );
        assert!(
            backend
                .recorded_calls()
                .contains(&"play:audio1:one.wav".to_owned())
        );

        // Slide 1's audio ends, the trigger jumps to slide 2 whose `audio1` autoplays.
        backend.mark_ended("audio1");
        for input in driver.poll(Some(&AlwaysReady)) {
            let effects = player.handle(&input);
            driver.apply(&player, &effects, Some(&AlwaysReady));
        }
        assert_eq!(player.current_slide().map(|s| s.id.as_str()), Some("s2"));
        let calls = backend.recorded_calls();
        assert!(
            calls.contains(&"play:audio1:two.wav".to_owned()),
            "{calls:?}"
        );
        assert!(!calls.contains(&"play:audio1:one.wav:two.wav".to_owned()));
    }

    #[test]
    fn pause_drops_a_play_that_is_still_fetching() {
        let course = Course::from_json(REPEAT_IDS_JSON).expect("valid course");
        let mut player = Player::new(&course).expect("player");
        let backend = NullBackend::new();
        let mut driver = MediaDriver::new().with_backend(Box::new(backend.clone()));
        let gate = Gate {
            ready: Cell::new(false),
        };
        player.handle(&Input::Start);

        driver.apply(
            &player,
            &[media_effect("audio1", MediaOp::Play)],
            Some(&gate),
        );
        assert_eq!(driver.pending_len(), 1);
        driver.apply(
            &player,
            &[media_effect("audio1", MediaOp::Pause)],
            Some(&gate),
        );
        assert_eq!(driver.pending_len(), 0);

        gate.ready.set(true);
        assert!(driver.poll(Some(&gate)).is_empty());
        assert!(!backend.is_playing("audio1"));
        assert!(
            !backend
                .recorded_calls()
                .iter()
                .any(|c| c.starts_with("play:") || c.starts_with("resume:"))
        );

        // Without the pause the same play starts once the bytes arrive.
        gate.ready.set(false);
        driver.apply(
            &player,
            &[media_effect("audio1", MediaOp::Play)],
            Some(&gate),
        );
        gate.ready.set(true);
        driver.poll(Some(&gate));
        assert!(backend.is_playing("audio1"));
    }

    #[test]
    fn seek_reaches_live_media_and_waits_for_fetching_media() {
        let course = Course::from_json(REPEAT_IDS_JSON).expect("valid course");
        let mut player = Player::new(&course).expect("player");
        let backend = NullBackend::new();
        let mut driver = MediaDriver::new().with_backend(Box::new(backend.clone()));
        let effects = player.handle(&Input::Start);
        driver.apply(&player, &effects, Some(&AlwaysReady));
        assert!(backend.is_playing("audio1"));

        driver.apply(&player, &[seek_effect("audio1", 1250)], Some(&AlwaysReady));
        assert_eq!(backend.position_ms("audio1"), Some(1250));
        assert!(
            backend
                .recorded_calls()
                .contains(&"seek:audio1:1250".to_owned())
        );

        // Stop it, then start again while its bytes are still fetching. The
        // seek that arrives now must apply only once the play starts.
        driver.apply(&player, &[media_effect("audio1", MediaOp::Stop)], None);
        let gate = Gate {
            ready: Cell::new(false),
        };
        driver.apply(
            &player,
            &[
                media_effect("audio1", MediaOp::Play),
                seek_effect("audio1", 900),
            ],
            Some(&gate),
        );
        assert_eq!(driver.pending_len(), 1);
        assert!(!backend.is_playing("audio1"));

        gate.ready.set(true);
        assert!(driver.poll(Some(&gate)).is_empty());
        assert!(backend.is_playing("audio1"));
        assert_eq!(backend.position_ms("audio1"), Some(900));
        let calls = backend.recorded_calls();
        let play = calls
            .iter()
            .rposition(|c| c.starts_with("play:audio1"))
            .expect("play recorded");
        let seek = calls
            .iter()
            .rposition(|c| c == "seek:audio1:900")
            .expect("seek recorded");
        assert!(play < seek, "seek must follow the play: {calls:?}");
    }

    #[test]
    fn volume_reaches_the_backend_and_is_guarded() {
        let backend = NullBackend::new();
        let mut driver = MediaDriver::new().with_backend(Box::new(backend.clone()));
        driver.set_volume(0.4);
        assert_eq!(driver.volume(), 0.4);
        assert_eq!(backend.volume(), 0.4);

        driver.set_volume(f32::NAN);
        assert_eq!(driver.volume(), 0.0);
        assert_eq!(backend.volume(), 0.0);

        driver.set_volume(f32::INFINITY);
        assert_eq!(driver.volume(), 1.0);
        assert_eq!(backend.volume(), 1.0);
    }

    /// A trimmed, half-volume audio object plus an untrimmed one, both autoplay.
    const TRIMMED_AUDIO_JSON: &str = r#"{
      "schema_version": 1,
      "id": "trimmed-audio",
      "title": "Trimmed audio",
      "slide_size": { "w": 960, "h": 540 },
      "scenes": [{
        "id": "sc",
        "title": "Scene",
        "slides": [{
          "id": "s1",
          "title": "One",
          "duration_ms": 10000,
          "objects": [
            {
              "id": "trimmed",
              "kind": {
                "type": "audio",
                "src": "cut.wav",
                "autoplay": true,
                "volume": 0.5,
                "trim": { "start_ms": 1000, "end_ms": 3000 }
              },
              "frame": { "x": 0.0, "y": 0.0, "w": 48.0, "h": 48.0 }
            },
            {
              "id": "plain",
              "kind": { "type": "audio", "src": "full.wav", "autoplay": true },
              "frame": { "x": 0.0, "y": 60.0, "w": 48.0, "h": 48.0 }
            }
          ]
        }]
      }]
    }"#;

    #[test]
    fn start_applies_a_half_volume_object_before_play() {
        let course = Course::from_json(TRIMMED_AUDIO_JSON).expect("valid course");
        let mut player = Player::new(&course).expect("player");
        let backend = NullBackend::new();
        let mut driver = MediaDriver::new().with_backend(Box::new(backend.clone()));
        let effects = player.handle(&Input::Start);
        driver.apply(&player, &effects, Some(&AlwaysReady));
        let calls = backend.recorded_calls();
        let volume = calls
            .iter()
            .position(|call| call == "set_object_volume:trimmed:0.5")
            .expect("object volume recorded");
        let play = calls
            .iter()
            .position(|call| call.starts_with("play:trimmed:"))
            .expect("play recorded");
        assert!(volume < play, "volume must be set before play: {calls:?}");
        assert_eq!(backend.object_volume("trimmed"), Some(0.5));
        assert_eq!(backend.object_volume("plain"), Some(1.0));
    }

    #[test]
    fn media_time_inputs_report_trimmed_audio_but_not_untrimmed_audio() {
        let course = Course::from_json(TRIMMED_AUDIO_JSON).expect("valid course");
        let mut player = Player::new(&course).expect("player");
        let backend = NullBackend::new();
        let mut driver = MediaDriver::new().with_backend(Box::new(backend.clone()));
        let effects = player.handle(&Input::Start);
        driver.apply(&player, &effects, Some(&AlwaysReady));
        backend.set_position_ms("trimmed", 1500);
        backend.set_position_ms("plain", 700);
        let inputs = driver.media_time_inputs(&player, 500);
        assert_eq!(
            inputs,
            vec![Input::MediaTime {
                object: "trimmed".to_owned(),
                at_ms: 1500.0,
            }],
            "only the object with a trim end is reported"
        );
    }

    #[test]
    fn missing_media_reports_ended() {
        struct Missing;
        impl MediaSource for Missing {
            fn get(&self, _src: &str) -> MediaFetch {
                MediaFetch::Missing
            }
        }
        let course = Course::from_json(REPEAT_IDS_JSON).expect("valid course");
        let mut player = Player::new(&course).expect("player");
        let mut driver = MediaDriver::new().with_backend(Box::new(NullBackend::new()));
        player.handle(&Input::Start);
        let followups = driver.apply(
            &player,
            &[media_effect("audio1", MediaOp::Play)],
            Some(&Missing),
        );
        assert_eq!(
            followups,
            vec![Input::MediaEnded {
                object: "audio1".to_owned()
            }]
        );
        let unknown = driver.apply(
            &player,
            &[media_effect("nope", MediaOp::Play)],
            Some(&Missing),
        );
        assert_eq!(unknown.len(), 1);
    }

    const TIMED_OBJECT_JSON: &str = r##"{
      "schema_version": 1,
      "id": "timed",
      "title": "Timed",
      "slide_size": { "w": 960, "h": 540 },
      "scenes": [{
        "id": "sc",
        "title": "Scene",
        "slides": [{
          "id": "s1",
          "title": "One",
          "duration_ms": 5000,
          "objects": [{
            "id": "box",
            "kind": { "type": "shape", "shape": "rectangle", "fill": "#2F6FEB" },
            "frame": { "x": 100.0, "y": 100.0, "w": 200.0, "h": 100.0 },
            "timing": { "start_ms": 1000, "end_ms": 4000 },
            "exit": { "effect": { "type": "fade" }, "duration_ms": 1000 }
          }]
        }]
      }]
    }"##;

    fn box_is_composed(player: &Player<'_>) -> bool {
        compose(player)
            .iter()
            .any(|item| matches!(&item.hit, Hit::Object(id) if id == "box"))
    }

    #[test]
    fn seek_moves_visibility_and_exit_on_the_next_composition() {
        let course = Course::from_json(TIMED_OBJECT_JSON).expect("valid course");
        let mut player = Player::new(&course).expect("player");
        player.handle(&Input::Start);
        assert!(!box_is_composed(&player));

        player.handle(&Input::PlayerSeek { ms: 1500 });
        assert!(box_is_composed(&player));

        // Back before its start: the next composition drops it again.
        player.handle(&Input::PlayerSeek { ms: 500 });
        assert!(!box_is_composed(&player));

        player.handle(&Input::PlayerSeek { ms: 3500 });
        let mid = player
            .sample_object("box")
            .expect("inside the window")
            .opacity;
        player.handle(&Input::PlayerSeek { ms: 3900 });
        let late = player
            .sample_object("box")
            .expect("exit still running")
            .opacity;
        assert!(late < mid, "exit animation follows the seek");

        player.handle(&Input::PlayerSeek { ms: 4000 });
        assert!(player.sample_object("box").is_none());
        assert!(!box_is_composed(&player));
    }
}

#[test]
fn installed_fonts_families_exist_and_bold_face_is_wider() {
    let ctx = egui::Context::default();
    crate::install_fonts(&ctx);

    let mut w_reg = 0.0;
    let mut w_bold = 0.0;

    let output = ctx.run_ui(egui::RawInput::default(), |ui| {
        let families = ui.ctx().fonts(|f| f.families());
        assert!(families.contains(&egui::FontFamily::Name("inter".into())));
        assert!(families.contains(&egui::FontFamily::Name("inter-bold".into())));
        assert!(families.contains(&egui::FontFamily::Name("inter-italic".into())));
        assert!(families.contains(&egui::FontFamily::Name("inter-bold-italic".into())));

        let text = "Sphinx of black quartz, judge my vow";
        let reg_id = egui::FontId::new(24.0, egui::FontFamily::Name("inter".into()));
        let bold_id = egui::FontId::new(24.0, egui::FontFamily::Name("inter-bold".into()));

        let g_reg = ui
            .painter()
            .layout(text.to_owned(), reg_id, egui::Color32::BLACK, 1000.0);
        let g_bold = ui
            .painter()
            .layout(text.to_owned(), bold_id, egui::Color32::BLACK, 1000.0);
        w_reg = g_reg.size().x;
        w_bold = g_bold.size().x;
    });
    output.drop_without_applying_deltas();

    assert!(
        w_bold > w_reg,
        "expected bold font ({w_bold}) to be wider than regular font ({w_reg})"
    );
}

#[test]
fn paint_without_install_fonts_falls_back_without_panic() {
    let ctx = egui::Context::default();
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 720.0));
    let t = SlideTransform::fit(screen, coursecraft_course::Size { w: 1280, h: 720 }).unwrap();

    let items = vec![
        DrawItem {
            hit: Hit::None,
            rect: egui::Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(200.0, 50.0)),
            visual: Visual::Text {
                runs: vec![crate::compose::ResolvedTextRun {
                    text: "Fallback test".to_owned(),
                    size: 20.0,
                    color: egui::Color32::BLACK,
                    bold: true,
                    italic: true,
                    underline: false,
                    link: None,
                    font: None,
                    strikethrough: false,
                    baseline: coursecraft_course::Baseline::Normal,
                    letter_spacing: 0.0,
                }],
                align: coursecraft_course::Align::Left,
                valign: coursecraft_course::VAlign::Top,
                paragraphs: Vec::new(),
                autofit: coursecraft_course::Autofit::None,
            },
            dimmed: false,
            opacity: 1.0,
            clip: None,
            rotation: 0.0,
            flip_h: false,
            flip_v: false,
            shadow: None,
            glow: None,
            media_time_ms: None,
            links: Vec::new(),
            show_table_overflow: false,
            clip_rect: None,
            mark: None,
        },
        DrawItem {
            hit: Hit::None,
            rect: egui::Rect::from_min_size(egui::pos2(10.0, 70.0), egui::vec2(200.0, 50.0)),
            visual: Visual::Button {
                label: "Btn".to_owned(),
                fill: egui::Color32::BLUE,
                fill_style: None,
                hover_fill: None,
                down_fill: None,
                disabled: false,
            },
            dimmed: false,
            opacity: 1.0,
            clip: None,
            rotation: 0.0,
            flip_h: false,
            flip_v: false,
            shadow: None,
            glow: None,
            media_time_ms: None,
            links: Vec::new(),
            show_table_overflow: false,
            clip_rect: None,
            mark: None,
        },
        DrawItem {
            hit: Hit::None,
            rect: egui::Rect::from_min_size(egui::pos2(10.0, 130.0), egui::vec2(300.0, 60.0)),
            visual: Visual::Prompt {
                text: "Prompt text".to_owned(),
            },
            dimmed: false,
            opacity: 1.0,
            clip: None,
            rotation: 0.0,
            flip_h: false,
            flip_v: false,
            shadow: None,
            glow: None,
            media_time_ms: None,
            links: Vec::new(),
            show_table_overflow: false,
            clip_rect: None,
            mark: None,
        },
        DrawItem {
            hit: Hit::None,
            rect: egui::Rect::from_min_size(egui::pos2(10.0, 200.0), egui::vec2(300.0, 40.0)),
            visual: Visual::ChoiceRow {
                text: "Choice text".to_owned(),
                multiple: false,
                selected: true,
                mark: None,
            },
            dimmed: false,
            opacity: 1.0,
            clip: None,
            rotation: 0.0,
            flip_h: false,
            flip_v: false,
            shadow: None,
            glow: None,
            media_time_ms: None,
            links: Vec::new(),
            show_table_overflow: false,
            clip_rect: None,
            mark: None,
        },
    ];

    let output = ctx.run_ui(egui::RawInput::default(), |ui| {
        let painter = ui.painter();
        paint(painter, &t, &items, Highlight::default(), None, None, None);
    });
    output.drop_without_applying_deltas();
}

#[test]
fn compose_player_replaces_vars_and_authoring_keeps_placeholders() {
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "c_vars",
        "title": "Course with %vars%",
        "variables": [
            { "name": "user_name", "value": "Alice" }
        ],
        "scenes": [{
            "id": "sc1",
            "title": "Scene 1",
            "slides": [{
                "id": "s1",
                "title": "Slide 1",
                "duration_ms": 5000,
                "objects": [
                    {
                        "id": "txt1",
                        "kind": { "type": "text", "text": "Hello %user_name% on slide %Course.SlideNumber%", "style": { "size": 20.0 } },
                        "frame": { "x": 50.0, "y": 50.0, "w": 300.0, "h": 50.0 }
                    },
                    {
                        "id": "btn1",
                        "kind": { "type": "button", "label": "Click %user_name%" },
                        "frame": { "x": 50.0, "y": 120.0, "w": 200.0, "h": 50.0 }
                    }
                ],
                "question": {
                    "kind": "multiple_choice",
                    "prompt": "Question for %user_name%?",
                    "choices": [
                        { "id": "c1", "text": "Choice %user_name%", "correct": true },
                        { "id": "c2", "text": "Other", "correct": false }
                    ]
                }
            }]
        }]
    });

    let course = Course::from_json(&json.to_string()).unwrap();
    let mut player = Player::new(&course).unwrap();
    player.handle(&Input::Start);

    // Player compose should replace variables
    let items = compose(&player);
    let txt_item = items
        .iter()
        .find(|i| matches!(i.hit, Hit::Object(ref id) if id == "txt1"))
        .unwrap();
    match &txt_item.visual {
        Visual::Text { runs, .. } => assert_eq!(runs[0].text, "Hello Alice on slide 1"),
        _ => panic!("expected Visual::Text"),
    }

    let btn_item = items
        .iter()
        .find(|i| matches!(i.hit, Hit::Object(ref id) if id == "btn1"))
        .unwrap();
    match &btn_item.visual {
        Visual::Button { label, .. } => assert_eq!(label, "Click Alice"),
        _ => panic!("expected Visual::Button"),
    }

    let prompt_item = items
        .iter()
        .find(|i| matches!(i.visual, Visual::Prompt { .. }))
        .unwrap();
    match &prompt_item.visual {
        Visual::Prompt { text } => assert_eq!(text, "Question for Alice?"),
        _ => panic!("expected Visual::Prompt"),
    }

    let choice_item = items
        .iter()
        .find(|i| matches!(i.hit, Hit::Choice(ref id) if id == "c1"))
        .unwrap();
    match &choice_item.visual {
        Visual::ChoiceRow { text, .. } => assert_eq!(text, "Choice Alice"),
        _ => panic!("expected Visual::ChoiceRow"),
    }

    // Authoring compose should keep placeholders
    let slide = course.slides().next().unwrap();
    let auth_items = compose_authoring(&course, slide, None);

    let auth_txt = auth_items
        .iter()
        .find(|i| matches!(i.hit, Hit::Object(ref id) if id == "txt1"))
        .unwrap();
    match &auth_txt.visual {
        Visual::Text { runs, .. } => {
            assert_eq!(
                runs[0].text,
                "Hello %user_name% on slide %Course.SlideNumber%"
            )
        }
        _ => panic!("expected Visual::Text"),
    }

    let auth_btn = auth_items
        .iter()
        .find(|i| matches!(i.hit, Hit::Object(ref id) if id == "btn1"))
        .unwrap();
    match &auth_btn.visual {
        Visual::Button { label, .. } => assert_eq!(label, "Click %user_name%"),
        _ => panic!("expected Visual::Button"),
    }

    let auth_prompt = auth_items
        .iter()
        .find(|i| matches!(i.visual, Visual::Prompt { .. }))
        .unwrap();
    match &auth_prompt.visual {
        Visual::Prompt { text } => assert_eq!(text, "Question for %user_name%?"),
        _ => panic!("expected Visual::Prompt"),
    }
}

#[test]
fn text_layout_bullets_hanging_indent_and_wrapping() {
    use coursecraft_course::{Autofit, ListKind, Paragraph, VAlign};

    let ctx = text_layout_context();
    let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(120.0, 200.0));
    let runs = vec![crate::compose::ResolvedTextRun {
        text: "A long bullet line that wraps at least once in this narrow box".into(),
        size: 16.0,
        color: Color32::BLACK,
        bold: false,
        italic: false,
        underline: false,
        link: None,
        font: None,
        strikethrough: false,
        baseline: coursecraft_course::Baseline::Normal,
        letter_spacing: 0.0,
    }];
    let laid = crate::text_layout::layout_text(
        &ctx,
        rect,
        1.0,
        &runs,
        &[Paragraph {
            list: ListKind::Bullet,
            ..Paragraph::default()
        }],
        coursecraft_course::Align::Left,
        VAlign::Top,
        Autofit::None,
    );
    let paragraph = laid.paragraphs.first().expect("paragraph");
    assert!((paragraph.pos.x - crate::text_layout::INDENT_STEP).abs() < 0.01);
    let marker = paragraph.marker.as_ref().expect("bullet marker");
    assert!(marker.galley.text().contains('•'));
    assert!(marker.pos.x < paragraph.pos.x);
    assert!(
        paragraph.galley.rows.len() > 1,
        "the narrow box should wrap the line"
    );
    // Every wrapped row starts at the text indent, not under the marker.
    for row in &paragraph.galley.rows {
        assert!(paragraph.pos.x + row.pos.x >= paragraph.pos.x - 0.01);
    }
}

#[test]
fn text_layout_numbering_restarts_and_formats_by_level() {
    use coursecraft_course::{Autofit, ListKind, Paragraph, VAlign};

    let ctx = text_layout_context();
    let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 400.0));
    let runs = vec![crate::compose::ResolvedTextRun {
        text: "one\ntwo\nthree\nfour".into(),
        size: 16.0,
        color: Color32::BLACK,
        bold: false,
        italic: false,
        underline: false,
        link: None,
        font: None,
        strikethrough: false,
        baseline: coursecraft_course::Baseline::Normal,
        letter_spacing: 0.0,
    }];
    let paragraphs = vec![
        Paragraph {
            list: ListKind::Number,
            level: 1,
            ..Paragraph::default()
        },
        Paragraph {
            list: ListKind::Number,
            level: 1,
            ..Paragraph::default()
        },
        Paragraph::default(),
        Paragraph {
            list: ListKind::Number,
            level: 1,
            ..Paragraph::default()
        },
    ];
    let laid = crate::text_layout::layout_text(
        &ctx,
        rect,
        1.0,
        &runs,
        &paragraphs,
        coursecraft_course::Align::Left,
        VAlign::Top,
        Autofit::None,
    );
    let markers: Vec<Option<String>> = laid
        .paragraphs
        .iter()
        .map(|paragraph| {
            paragraph
                .marker
                .as_ref()
                .map(|marker| marker.galley.text().to_owned())
        })
        .collect();
    assert_eq!(
        markers,
        vec![
            Some("a.".into()),
            Some("b.".into()),
            None,
            Some("a.".into())
        ]
    );

    let level_runs = vec![crate::compose::ResolvedTextRun {
        text: "one\ntwo\nthree".into(),
        size: 16.0,
        color: Color32::BLACK,
        bold: false,
        italic: false,
        underline: false,
        link: None,
        font: None,
        strikethrough: false,
        baseline: coursecraft_course::Baseline::Normal,
        letter_spacing: 0.0,
    }];
    let levels = vec![
        Paragraph {
            list: ListKind::Number,
            level: 0,
            ..Paragraph::default()
        },
        Paragraph {
            list: ListKind::Number,
            level: 1,
            ..Paragraph::default()
        },
        Paragraph {
            list: ListKind::Number,
            level: 2,
            ..Paragraph::default()
        },
    ];
    let laid = crate::text_layout::layout_text(
        &ctx,
        rect,
        1.0,
        &level_runs,
        &levels,
        coursecraft_course::Align::Left,
        VAlign::Top,
        Autofit::None,
    );
    let markers: Vec<String> = laid
        .paragraphs
        .iter()
        .filter_map(|paragraph| {
            paragraph
                .marker
                .as_ref()
                .map(|m| m.galley.text().to_owned())
        })
        .collect();
    assert_eq!(markers, vec!["1.", "a.", "i."]);
}

#[test]
fn text_layout_line_spacing_changes_height() {
    use coursecraft_course::{Autofit, Paragraph, VAlign};

    let ctx = text_layout_context();
    let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 400.0));
    let runs = vec![crate::compose::ResolvedTextRun {
        text: "one\ntwo\nthree".into(),
        size: 20.0,
        color: Color32::BLACK,
        bold: false,
        italic: false,
        underline: false,
        link: None,
        font: None,
        strikethrough: false,
        baseline: coursecraft_course::Baseline::Normal,
        letter_spacing: 0.0,
    }];
    let single = crate::text_layout::layout_text(
        &ctx,
        rect,
        1.0,
        &runs,
        &[],
        coursecraft_course::Align::Left,
        VAlign::Top,
        Autofit::None,
    );
    let paragraphs: Vec<Paragraph> = (0..3)
        .map(|_| Paragraph {
            line_spacing: 2.0,
            ..Paragraph::default()
        })
        .collect();
    let double = crate::text_layout::layout_text(
        &ctx,
        rect,
        1.0,
        &runs,
        &paragraphs,
        coursecraft_course::Align::Left,
        VAlign::Top,
        Autofit::None,
    );
    assert!(
        double.height > single.height * 1.5,
        "double spacing should roughly double the height: {} vs {}",
        double.height,
        single.height
    );
}

#[test]
fn text_layout_shrink_stops_at_forty_percent_and_fits() {
    use coursecraft_course::{Autofit, VAlign};

    let ctx = text_layout_context();
    let narrow = Rect::from_min_size(pos2(0.0, 0.0), vec2(160.0, 110.0));
    let runs = vec![crate::compose::ResolvedTextRun {
        text: "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight".into(),
        size: 24.0,
        color: Color32::BLACK,
        bold: false,
        italic: false,
        underline: false,
        link: None,
        font: None,
        strikethrough: false,
        baseline: coursecraft_course::Baseline::Normal,
        letter_spacing: 0.0,
    }];
    let full = crate::text_layout::layout_text(
        &ctx,
        narrow,
        1.0,
        &runs,
        &[],
        coursecraft_course::Align::Left,
        VAlign::Top,
        Autofit::None,
    );
    assert!(
        !full.fits(narrow),
        "the test text must overflow at full size"
    );

    let shrunk = crate::text_layout::layout_text(
        &ctx,
        narrow,
        1.0,
        &runs,
        &[],
        coursecraft_course::Align::Left,
        VAlign::Top,
        Autofit::Shrink,
    );
    assert!(shrunk.shrink >= crate::text_layout::AUTOFIT_MIN_SCALE - 0.001);
    assert!(shrunk.shrink <= 1.0);
    assert!(shrunk.fits(narrow), "shrink must make the text fit");

    let roomy = Rect::from_min_size(pos2(0.0, 0.0), vec2(600.0, 600.0));
    let none = crate::text_layout::layout_text(
        &ctx,
        roomy,
        1.0,
        &runs,
        &[],
        coursecraft_course::Align::Left,
        VAlign::Top,
        Autofit::Shrink,
    );
    assert!((none.shrink - 1.0).abs() < 0.001);
}

#[test]
fn keyboard_focus_reaches_text_links() {
    use coursecraft_course::{Autofit, Link, ObjectKind, TextRun, TextStyle};

    let mut course = Course::from_json(HELLO_JSON).expect("hello course");
    let title = course.scenes[0].slides[0]
        .objects
        .iter_mut()
        .find(|object| object.id == "title")
        .expect("title");
    title.kind = ObjectKind::Text {
        text: "Go".into(),
        style: TextStyle::default(),
        runs: vec![TextRun {
            text: "Go".into(),
            bold: None,
            italic: None,
            underline: None,
            size: None,
            color: None,
            link: Some(Link::Url {
                url: "https://example.com".into(),
            }),
            font: None,
            strikethrough: None,
            baseline: None,
            letter_spacing: None,
        }],
        paragraphs: Vec::new(),
        autofit: Autofit::None,
        text_style: None,
    };
    let mut player = Player::new(&course).expect("player");
    player.handle(&Input::Start);
    let list = crate::focus::build_list(&player, true, true, false);
    assert!(
        list.iter().any(|target| matches!(
            target,
            crate::focus::FocusTarget::Link { object, .. } if object == "title"
        )),
        "the title link must be in the keyboard focus order: {list:?}"
    );
}

#[test]
fn runs_produce_layout_sections_with_format() {
    let runs = vec![
        crate::compose::ResolvedTextRun {
            text: "Bold part ".into(),
            size: 20.0,
            color: Color32::RED,
            bold: true,
            italic: false,
            underline: false,
            link: None,
            font: None,
            strikethrough: false,
            baseline: coursecraft_course::Baseline::Normal,
            letter_spacing: 0.0,
        },
        crate::compose::ResolvedTextRun {
            text: "italic underline part ".into(),
            size: 20.0,
            color: Color32::GREEN,
            bold: false,
            italic: true,
            underline: true,
            link: None,
            font: None,
            strikethrough: false,
            baseline: coursecraft_course::Baseline::Normal,
            letter_spacing: 0.0,
        },
        crate::compose::ResolvedTextRun {
            text: "large part".into(),
            size: 32.0,
            color: Color32::BLUE,
            bold: false,
            italic: false,
            underline: false,
            link: None,
            font: None,
            strikethrough: false,
            baseline: coursecraft_course::Baseline::Normal,
            letter_spacing: 0.0,
        },
    ];

    let ctx = egui::Context::default();
    let output = ctx.run_ui(egui::RawInput::default(), |ui| {
        let mut job = egui::text::LayoutJob::default();
        for run in &runs {
            let font_id = crate::font::slide_font(ui.ctx(), run.bold, run.italic, run.size);
            let underline_stroke = if run.underline {
                egui::Stroke::new(1.0, run.color)
            } else {
                egui::Stroke::NONE
            };
            job.append(
                &run.text,
                0.0,
                egui::text::TextFormat {
                    font_id,
                    color: run.color,
                    underline: underline_stroke,
                    ..Default::default()
                },
            );
        }

        assert_eq!(job.sections.len(), 3);
        assert_eq!(job.sections[0].format.color, Color32::RED);
        assert_eq!(job.sections[0].format.underline, egui::Stroke::NONE);

        assert_eq!(job.sections[1].format.color, Color32::GREEN);
        assert_ne!(job.sections[1].format.underline, egui::Stroke::NONE);

        assert_eq!(job.sections[2].format.color, Color32::BLUE);
        assert_eq!(job.sections[2].format.font_id.size, 32.0);
    });
    output.drop_without_applying_deltas();
}

#[test]
fn align_and_valign_positions_on_known_frame() {
    let screen_rect = Rect::from_min_size(pos2(100.0, 100.0), vec2(400.0, 200.0));
    let galley_h = 50.0;
    let diff = (screen_rect.height() - galley_h).max(0.0);

    for (align, expected_x) in [
        (coursecraft_course::Align::Left, 100.0),
        (coursecraft_course::Align::Center, 300.0),
        (coursecraft_course::Align::Right, 500.0),
    ] {
        let x = match align {
            coursecraft_course::Align::Left => screen_rect.left(),
            coursecraft_course::Align::Center => screen_rect.center().x,
            coursecraft_course::Align::Right => screen_rect.right(),
        };
        assert_eq!(x, expected_x);
    }

    for (valign, expected_y) in [
        (coursecraft_course::VAlign::Top, 100.0),
        (coursecraft_course::VAlign::Middle, 100.0 + diff * 0.5),
        (coursecraft_course::VAlign::Bottom, 100.0 + diff),
    ] {
        let y_offset = match valign {
            coursecraft_course::VAlign::Top => 0.0,
            coursecraft_course::VAlign::Middle => diff * 0.5,
            coursecraft_course::VAlign::Bottom => diff,
        };
        let y = screen_rect.top() + y_offset;
        assert_eq!(y, expected_y);
    }
}

#[test]
fn plain_text_renders_as_one_run() {
    let obj = coursecraft_course::Object {
        id: "t1".into(),
        name: "".into(),
        kind: coursecraft_course::ObjectKind::Text {
            text: "Simple text".into(),
            style: coursecraft_course::TextStyle::default(),
            runs: Vec::new(),
            paragraphs: Vec::new(),
            autofit: coursecraft_course::Autofit::None,
            text_style: None,
        },
        frame: coursecraft_course::Rect {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 50.0,
        },
        timing: coursecraft_course::Timing::default(),
        entrance: None,
        exit: None,
        motion_paths: Vec::new(),
        states: Vec::new(),
        initial_state: "normal".into(),
        focus_order: None,
        decorative: false,
        description: None,
        group: None,
        panel: None,
        rotation: 0.0,
        flip_h: false,
        flip_v: false,
        opacity: 1.0,
        shadow: None,
        glow: None,
        drag: None,
    };
    let item = compose_object(&obj, "normal").expect("composed item");
    match item.visual {
        Visual::Text { runs, .. } => {
            assert_eq!(runs.len(), 1);
            assert_eq!(runs[0].text, "Simple text");
        }
        _ => panic!("expected Visual::Text"),
    }
}

#[test]
fn per_run_var_substitution_and_split_reference() {
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "c_runs_vars",
        "title": "Per run vars",
        "variables": [
            { "name": "user_name", "value": "Bob" }
        ],
        "scenes": [{
            "id": "sc1",
            "title": "Scene",
            "slides": [{
                "id": "s1",
                "title": "Slide 1",
                "objects": [
                    {
                        "id": "txt_runs",
                        "frame": { "x": 0, "y": 0, "w": 300, "h": 50 },
                        "kind": {
                            "type": "text",
                            "text": "Hello %user_name%! %user_name% again. %user_name%",
                            "runs": [
                                { "text": "Hello %user_name%! " },
                                { "text": "Score: %Course.ScorePercent%%% " },
                                { "text": "%user_" },
                                { "text": "name%" }
                            ]
                        }
                    }
                ]
            }]
        }]
    });

    let course = Course::from_json(&json.to_string()).unwrap();
    let mut player = Player::new(&course).unwrap();
    player.handle(&Input::Start);

    let items = compose(&player);
    let item = items
        .iter()
        .find(|i| matches!(i.hit, Hit::Object(ref id) if id == "txt_runs"))
        .unwrap();
    match &item.visual {
        Visual::Text { runs, .. } => {
            assert_eq!(runs.len(), 4);
            assert_eq!(runs[0].text, "Hello Bob! ");
            assert_eq!(runs[1].text, "Score: 100% ");
            // Split reference across runs 2 and 3 should NOT be substituted
            assert_eq!(runs[2].text, "%user_");
            assert_eq!(runs[3].text, "name%");
        }
        _ => panic!("expected Visual::Text"),
    }
}

#[test]
fn default_theme_renders_same_colours_as_constants() {
    assert_eq!(
        crate::compose::DEFAULT_BUTTON_COLOR,
        Color32::from_rgb(0x2F, 0x6F, 0xEB)
    );
    assert_eq!(
        crate::compose::DEFAULT_TEXT_COLOR,
        Color32::from_rgb(0x1F, 0x1F, 0x1F)
    );
    assert_eq!(
        crate::chrome::DEFAULT_BAR_COLOR,
        Color32::from_rgb(0x1B, 0x1B, 0x1B)
    );
    assert_eq!(
        crate::chrome::DEFAULT_BAR_TEXT_COLOR,
        Color32::from_rgb(0xFF, 0xFF, 0xFF)
    );
    assert_eq!(crate::paint::DEFAULT_BACKGROUND_COLOR, Color32::WHITE);
}

#[test]
fn slide_background_override() {
    let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 720.0));
    let t = SlideTransform::fit(screen, Size { w: 1280, h: 720 }).expect("fit");
    let ctx = egui::Context::default();

    let custom_bg = coursecraft_course::Color("#FF8800".into());
    let mut recorded_bg = None;

    let output = ctx.run_ui(egui::RawInput::default(), |ui| {
        let painter = ui.painter();
        paint(
            painter,
            &t,
            &[],
            Highlight::default(),
            None,
            None,
            Some(&custom_bg),
        );
        painter.for_each_shape(|clipped| {
            if let egui::Shape::Rect(ref r) = clipped.shape
                && r.rect == t.slide_rect()
            {
                recorded_bg = Some(r.fill);
            }
        });
    });
    output.drop_without_applying_deltas();
    assert_eq!(recorded_bg, Some(Color32::from_rgb(0xFF, 0x88, 0x00)));
}

// ---------------------------------------------------------------- transition & animation tests

#[test]
fn transition_sampling_fade_and_push() {
    use coursecraft_course::{Direction, Transition, TransitionEffect};
    use coursecraft_logic::animation::sample_transition;

    let slide_size = Size { w: 1000, h: 500 };

    // Fade
    let fade = Transition {
        effect: TransitionEffect::Fade,
        duration_ms: 500,
    };
    let s0 = sample_transition(&fade, 0.0, slide_size);
    assert_eq!(s0.prev_alpha, 1.0);
    assert_eq!(s0.next_alpha, 0.0);
    assert_eq!(s0.prev_offset, (0.0, 0.0));
    assert_eq!(s0.next_offset, (0.0, 0.0));

    let s_mid = sample_transition(&fade, 0.5, slide_size);
    assert!((s_mid.prev_alpha - 0.5).abs() < 1e-4);
    assert!((s_mid.next_alpha - 0.5).abs() < 1e-4);

    let s1 = sample_transition(&fade, 1.0, slide_size);
    assert_eq!(s1.prev_alpha, 0.0);
    assert_eq!(s1.next_alpha, 1.0);

    // Push from Left: enters from Left (moves from -w to 0), prev moves from 0 to +w
    let push_left = Transition {
        effect: TransitionEffect::Push {
            from: Direction::Left,
        },
        duration_ms: 500,
    };
    let p0 = sample_transition(&push_left, 0.0, slide_size);
    assert_eq!(p0.prev_offset, (0.0, 0.0));
    assert_eq!(p0.next_offset, (-1000.0, 0.0));

    let p_mid = sample_transition(&push_left, 0.5, slide_size);
    assert_eq!(p_mid.prev_offset, (500.0, 0.0));
    assert_eq!(p_mid.next_offset, (-500.0, 0.0));

    let p1 = sample_transition(&push_left, 1.0, slide_size);
    assert_eq!(p1.prev_offset, (1000.0, 0.0));
    assert_eq!(p1.next_offset, (0.0, 0.0));

    // Push from Top: enters from Top (moves from -h to 0), prev moves from 0 to +h
    let push_top = Transition {
        effect: TransitionEffect::Push {
            from: Direction::Top,
        },
        duration_ms: 500,
    };
    let pt0 = sample_transition(&push_top, 0.0, slide_size);
    assert_eq!(pt0.prev_offset, (0.0, 0.0));
    assert_eq!(pt0.next_offset, (0.0, -500.0));

    let pt_mid = sample_transition(&push_top, 0.5, slide_size);
    assert_eq!(pt_mid.prev_offset, (0.0, 250.0));
    assert_eq!(pt_mid.next_offset, (0.0, -250.0));

    let pt1 = sample_transition(&push_top, 1.0, slide_size);
    assert_eq!(pt1.prev_offset, (0.0, 500.0));
    assert_eq!(pt1.next_offset, (0.0, 0.0));
}

#[test]
fn transition_sampling_cover() {
    use coursecraft_course::{Direction, Transition, TransitionEffect};
    use coursecraft_logic::animation::sample_transition;

    let slide_size = Size { w: 1000, h: 500 };

    // Cover from Right: enters from Right (+w to 0), prev is static at (0, 0)
    let cover_right = Transition {
        effect: TransitionEffect::Cover {
            from: Direction::Right,
        },
        duration_ms: 500,
    };
    let c0 = sample_transition(&cover_right, 0.0, slide_size);
    assert_eq!(c0.prev_offset, (0.0, 0.0));
    assert_eq!(c0.next_offset, (1000.0, 0.0));

    let c_mid = sample_transition(&cover_right, 0.5, slide_size);
    assert_eq!(c_mid.prev_offset, (0.0, 0.0));
    assert_eq!(c_mid.next_offset, (500.0, 0.0));

    let c1 = sample_transition(&cover_right, 1.0, slide_size);
    assert_eq!(c1.prev_offset, (0.0, 0.0));
    assert_eq!(c1.next_offset, (0.0, 0.0));
}

#[test]
fn compose_with_animated_object_scales_and_translates() {
    let mut course = Course::from_json(HELLO_JSON).expect("valid course");
    let slide = course.slide_mut("s-intro").expect("intro slide");
    // Attach an entrance animation to start button
    let btn = slide
        .objects
        .iter_mut()
        .find(|o| o.id == "btn-start")
        .expect("btn");
    btn.entrance = Some(coursecraft_course::Animation {
        effect: coursecraft_course::AnimEffect::Fade,
        duration_ms: 1000,
        easing: coursecraft_course::Easing::Linear,
    });
    btn.timing.start_ms = 0;

    let mut player = Player::new(&course).expect("valid player");
    player.handle(&Input::Start);
    // At t = 0 ms, entrance progress = 0.0, opacity = 0.0 -> skipped in compose!
    let items_0 = compose(&player);
    assert!(
        !items_0
            .iter()
            .any(|it| it.hit == Hit::Object("btn-start".into()))
    );

    // At t = 500 ms, opacity = 0.5
    player.handle(&Input::Tick { ms: 500 });
    let items_500 = compose(&player);
    let it = items_500
        .iter()
        .find(|it| it.hit == Hit::Object("btn-start".into()))
        .expect("btn present");
    assert!((it.opacity - 0.5).abs() < 1e-4);

    // At t = 1000 ms, opacity = 1.0
    player.handle(&Input::Tick { ms: 500 });
    let items_1000 = compose(&player);
    let it = items_1000
        .iter()
        .find(|it| it.hit == Hit::Object("btn-start".into()))
        .expect("btn present");
    assert!((it.opacity - 1.0).abs() < 1e-4);
}

fn course_with_question(question: serde_json::Value) -> Course {
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "c_q",
        "title": "Questions",
        "variables": [{ "name": "user_name", "value": "Alice" }],
        "scenes": [{
            "id": "sc",
            "title": "Scene",
            "slides": [{
                "id": "s1",
                "title": "Slide 1",
                "duration_ms": 1000,
                "question": question
            }]
        }]
    });
    Course::from_json(&json.to_string()).expect("question course")
}

fn sequence_row_texts(items: &[DrawItem]) -> Vec<&str> {
    items
        .iter()
        .filter_map(|item| match &item.visual {
            Visual::SequenceRow { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

#[test]
fn compose_true_false_and_fill_in() {
    use crate::compose::FILL_IN_PLACEHOLDER;

    let tf = course_with_question(serde_json::json!({
        "kind": "true_false",
        "prompt": "The sky is up",
        "choices": [
            { "id": "true", "text": "True", "correct": true },
            { "id": "false", "text": "False", "correct": false }
        ]
    }));
    let mut player = Player::new(&tf).expect("player");
    player.handle(&Input::Start);
    let items = compose(&player);
    let radios: Vec<_> = items
        .iter()
        .filter(|item| {
            matches!(
                item.visual,
                Visual::ChoiceRow {
                    multiple: false,
                    ..
                }
            )
        })
        .collect();
    assert_eq!(radios.len(), 2);
    assert!(radios.iter().all(|item| matches!(item.hit, Hit::Choice(_))));
    assert!((radios[0].rect.height() - 56.0).abs() < 0.1);
    player.handle(&Input::SelectChoice {
        choice: "true".into(),
    });
    let items = compose(&player);
    let selected = items
        .iter()
        .find(|item| item.hit == Hit::Choice("true".into()));
    match &selected.expect("true").visual {
        Visual::ChoiceRow {
            selected: true,
            multiple: false,
            text,
            ..
        } => assert_eq!(text, "True"),
        other => panic!("expected a selected true row, got {other:?}"),
    }

    let fill = course_with_question(serde_json::json!({
        "kind": "fill_in",
        "prompt": "Name %user_name%",
        "choices": [],
        "accepted": ["Alice"]
    }));
    let mut player = Player::new(&fill).expect("player");
    player.handle(&Input::Start);
    let items = compose(&player);
    let field = items
        .iter()
        .find(|item| matches!(item.visual, Visual::FillIn { .. }))
        .expect("fill-in");
    assert!(matches!(field.hit, Hit::QuestionChrome(_)));
    match &field.visual {
        Visual::FillIn {
            text,
            placeholder,
            multiline: false,
            ..
        } => {
            assert_eq!(text, "");
            assert_eq!(placeholder, FILL_IN_PLACEHOLDER);
        }
        other => panic!("unexpected visual {other:?}"),
    }
    let prompt = items
        .iter()
        .find(|item| matches!(item.visual, Visual::Prompt { .. }))
        .expect("prompt");
    assert_eq!(
        prompt.visual,
        Visual::Prompt {
            text: "Name Alice".into()
        }
    );
    player.handle(&Input::TextEntered {
        text: "%user_name%".into(),
    });
    let items = compose(&player);
    match &items
        .iter()
        .find(|item| matches!(item.visual, Visual::FillIn { .. }))
        .expect("fill-in")
        .visual
    {
        Visual::FillIn { text, .. } => assert_eq!(text, "%user_name%"),
        other => panic!("unexpected visual {other:?}"),
    }
}

#[test]
fn freeform_composes_the_prompt_only_when_it_has_text() {
    let empty = course_with_question(serde_json::json!({
        "kind": "freeform",
        "prompt": "",
        "choices": [],
        "correct_when": { "compare": { "variable": "user_name", "op": "eq", "value": "Alice" } }
    }));
    let mut player = Player::new(&empty).expect("player");
    player.handle(&Input::Start);
    let items = compose(&player);
    assert!(
        !items
            .iter()
            .any(|item| matches!(item.visual, Visual::Prompt { .. })),
        "empty freeform prompts draw nothing"
    );
    assert!(
        !items.iter().any(|item| matches!(
            item.visual,
            Visual::ChoiceRow { .. } | Visual::FillIn { .. }
        )),
        "freeform draws no question rows"
    );

    let filled = course_with_question(serde_json::json!({
        "kind": "freeform",
        "prompt": "Show your working",
        "choices": [],
        "correct_when": { "compare": { "variable": "user_name", "op": "eq", "value": "Alice" } }
    }));
    let mut player = Player::new(&filled).expect("player");
    player.handle(&Input::Start);
    let items = compose(&player);
    let prompt = items
        .iter()
        .find(|item| matches!(item.visual, Visual::Prompt { .. }))
        .expect("prompt");
    assert_eq!(
        prompt.visual,
        Visual::Prompt {
            text: "Show your working".into()
        }
    );
}

#[test]
fn essay_fill_in_is_multiline_four_rows_and_stays_on_the_slide() {
    let essay = course_with_question(serde_json::json!({
        "kind": "essay",
        "prompt": "Tell us more",
        "choices": []
    }));
    let mut player = Player::new(&essay).expect("player");
    player.handle(&Input::Start);
    let items = compose(&player);
    let field = items
        .iter()
        .find(|item| matches!(item.visual, Visual::FillIn { .. }))
        .expect("essay fill-in");
    let Visual::FillIn { multiline, .. } = &field.visual else {
        panic!("unexpected visual {:?}", field.visual);
    };
    assert!(*multiline, "essay fill-in must be multiline");
    let slide_h = essay.slide_size.h as f32;
    assert!(
        field.rect.bottom() <= slide_h - 24.0 + 0.01,
        "essay bottom {} exceeds slide limit {}",
        field.rect.bottom(),
        slide_h - 24.0
    );

    let short = course_with_question(serde_json::json!({
        "kind": "short_answer",
        "prompt": "Your name",
        "choices": []
    }));
    let mut player = Player::new(&short).expect("player");
    player.handle(&Input::Start);
    let items = compose(&player);
    let field = items
        .iter()
        .find(|item| matches!(item.visual, Visual::FillIn { .. }))
        .expect("short-answer fill-in");
    let Visual::FillIn { multiline, .. } = &field.visual else {
        panic!("unexpected visual {:?}", field.visual);
    };
    assert!(!*multiline, "short answer must stay single-line");
    let normal_h = field.rect.height();
    assert!(
        (field.rect.height() - 56.0).abs() < 0.1,
        "short answer keeps normal row height, got {normal_h}"
    );

    let essay_rect_h = {
        let items = {
            let course = course_with_question(serde_json::json!({
                "kind": "essay",
                "prompt": "Tell us more",
                "choices": []
            }));
            let mut player = Player::new(&course).expect("player");
            player.handle(&Input::Start);
            compose(&player)
        };
        items
            .iter()
            .find(|item| matches!(item.visual, Visual::FillIn { .. }))
            .expect("essay fill-in")
            .rect
            .height()
    };
    assert!(
        (essay_rect_h - normal_h * 4.0).abs() < 0.1,
        "essay is four rows tall: {essay_rect_h} vs {normal_h}"
    );
}

#[test]
fn compose_sequence_starts_shuffled_and_long_lists_shrink() {
    let sequence = course_with_question(serde_json::json!({
        "kind": "sequence",
        "prompt": "Order these",
        "choices": [
            { "id": "a", "text": "First %user_name%", "correct": false },
            { "id": "b", "text": "Second", "correct": false }
        ]
    }));
    let mut player = Player::new(&sequence).expect("player");
    player.handle(&Input::Start);
    assert_eq!(
        player.sequence_order(),
        &["b".to_owned(), "a".to_owned()][..]
    );
    let items = compose(&player);
    assert_eq!(sequence_row_texts(&items), vec!["Second", "First Alice"]);
    let ups: Vec<_> = items
        .iter()
        .filter(|item| matches!(item.visual, Visual::SequenceButton { up: true, .. }))
        .collect();
    assert_eq!(ups.len(), 2);
    assert!(matches!(ups[0].hit, Hit::QuestionChrome(_)));
    assert!(matches!(
        &ups[1].hit,
        Hit::SequenceMove { choice, to: 0 } if choice == "a"
    ));
    let slide = sequence.slides().next().expect("slide");
    let authored = compose_authoring(&sequence, slide, None);
    assert_eq!(
        sequence_row_texts(&authored),
        vec!["First %user_name%", "Second"]
    );

    let wide = course_with_question(serde_json::json!({
        "kind": "sequence",
        "prompt": "Many",
        "choices": (0..20).map(|i| serde_json::json!({
            "id": format!("i{i:02}"),
            "text": format!("Item {i}"),
            "correct": false
        })).collect::<Vec<_>>()
    }));
    let mut player = Player::new(&wide).expect("player");
    player.handle(&Input::Start);
    let items = compose(&player);
    let rows: Vec<_> = items
        .iter()
        .filter(|item| matches!(item.visual, Visual::SequenceRow { .. }))
        .collect();
    assert_eq!(rows.len(), 20);
    assert!((rows[0].rect.height() - 28.0).abs() < 0.1);
    assert!(rows[0].rect.min.y < 720.0);
    assert!(rows[19].rect.min.y > 720.0);
}

#[test]
fn compose_matching_substitutes_pair_text() {
    use crate::compose::MATCH_PLACEHOLDER;

    let matching = course_with_question(serde_json::json!({
        "kind": "matching",
        "prompt": "Match for %user_name%",
        "choices": [],
        "pairs": [
            { "id": "p1", "left": "Left %user_name%", "right": "Right %user_name%" },
            { "id": "p2", "left": "Other", "right": "Thing" }
        ]
    }));
    let mut player = Player::new(&matching).expect("player");
    player.handle(&Input::Start);
    let items = compose(&player);
    let rows: Vec<_> = items
        .iter()
        .filter(|item| matches!(item.visual, Visual::MatchRow { .. }))
        .collect();
    assert_eq!(rows.len(), 2);
    match &rows[0].visual {
        Visual::MatchRow {
            id, left, value, ..
        } => {
            assert_eq!(id, "p1");
            assert_eq!(left, "Left Alice");
            assert_eq!(value, MATCH_PLACEHOLDER);
        }
        other => panic!("unexpected visual {other:?}"),
    }
    player.handle(&Input::MatchSelected {
        left: "p1".into(),
        right: Some("p1".into()),
    });
    let items = compose(&player);
    match &items
        .iter()
        .find(|item| matches!(item.visual, Visual::MatchRow { ref id, .. } if id == "p1"))
        .expect("p1")
        .visual
    {
        Visual::MatchRow { value, .. } => assert_eq!(value, "Right Alice"),
        other => panic!("unexpected visual {other:?}"),
    }
    let slide = matching.slides().next().expect("slide");
    let authored = compose_authoring(&matching, slide, None);
    match &authored
        .iter()
        .find(|item| matches!(item.visual, Visual::MatchRow { ref id, .. } if id == "p1"))
        .expect("authored p1")
        .visual
    {
        Visual::MatchRow { left, value, .. } => {
            assert_eq!(left, "Left %user_name%");
            assert_eq!(value, MATCH_PLACEHOLDER);
        }
        other => panic!("unexpected visual {other:?}"),
    }
}

#[test]
fn pointer_release_on_sequence_up_emits_sequence_moved() {
    use crate::view::PointerState;

    let course = course_with_question(serde_json::json!({
        "kind": "sequence",
        "prompt": "Order",
        "choices": [
            { "id": "a", "text": "A", "correct": false },
            { "id": "b", "text": "B", "correct": false },
            { "id": "c", "text": "C", "correct": false }
        ]
    }));
    let mut player = Player::new(&course).expect("player");
    player.handle(&Input::Start);
    let order = player.sequence_order().to_vec();
    assert_ne!(order, vec!["a".to_owned(), "b".to_owned(), "c".to_owned()]);
    let items = compose(&player);
    let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 720.0));
    let t = SlideTransform::fit(screen, Size { w: 1280, h: 720 }).expect("fit");

    let disabled = items
        .iter()
        .find(|item| {
            matches!(&item.hit, Hit::QuestionChrome(key) if key == &format!("seq-up-{}", order[0]))
        })
        .expect("disabled up");
    let enabled = items
        .iter()
        .find(
            |item| matches!(&item.hit, Hit::SequenceMove { choice, to: 0 } if choice == &order[1]),
        )
        .expect("enabled up");

    let mut state = PointerState::default();
    let disabled_pos = t.to_screen_rect(disabled.rect).center();
    let none = state.handle_events(
        &[
            egui::Event::PointerButton {
                pos: disabled_pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::default(),
            },
            egui::Event::PointerButton {
                pos: disabled_pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::default(),
            },
        ],
        &t,
        &items,
        &|_| false,
    );
    assert!(none.is_empty(), "disabled up emitted {none:?}");

    let pos = t.to_screen_rect(enabled.rect).center();
    let inputs = state.handle_events(
        &[
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::default(),
            },
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::default(),
            },
        ],
        &t,
        &items,
        &|_| false,
    );
    assert_eq!(
        inputs,
        vec![Input::SequenceMoved {
            choice: order[1].clone(),
            to: 0,
        }]
    );
}

#[test]
fn widget_painting_blanks_fill_in_and_match_value_only() {
    let matching = course_with_question(serde_json::json!({
        "kind": "matching",
        "prompt": "Match",
        "choices": [],
        "pairs": [
            { "id": "p1", "left": "Left", "right": "Right" },
            { "id": "p2", "left": "Other", "right": "Thing" }
        ]
    }));
    let mut player = Player::new(&matching).expect("player");
    player.handle(&Input::Start);
    let items = compose(&player);
    let painted = crate::view::without_widget_text(&items);
    let mut rows = 0;
    for item in painted.iter() {
        if let Visual::MatchRow { left, value, .. } = &item.visual {
            rows += 1;
            assert!(value.is_empty(), "value box text must come from the combo");
            assert!(!left.is_empty(), "left label stays painted");
        }
    }
    assert_eq!(rows, 2);
    // The composed (editor-style) items still carry the placeholder.
    assert!(items.iter().any(
        |i| matches!(&i.visual, Visual::MatchRow { value, .. } if value == crate::compose::MATCH_PLACEHOLDER)
    ));
}

#[test]
fn compose_and_paint_hotspot_markers() {
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "c_hotspot",
        "title": "Hotspot",
        "scenes": [{
            "id": "sc",
            "title": "Scene",
            "slides": [{
                "id": "s1",
                "title": "Slide 1",
                "duration_ms": 1000,
                "objects": [{
                    "id": "img1",
                    "kind": { "type": "image", "src": "photo.png" },
                    "frame": { "x": 100.0, "y": 100.0, "w": 400.0, "h": 300.0 },
                    "timing": { "start_ms": 0 }
                }],
                "question": {
                    "kind": "hotspot",
                    "prompt": "Find it",
                    "choices": [],
                    "hotspot_image": "img1",
                    "regions": [{
                        "id": "r1",
                        "shape": "rect",
                        "x": 120.0,
                        "y": 120.0,
                        "w": 50.0,
                        "h": 50.0,
                        "correct": true
                    }],
                    "markers": 2
                }
            }]
        }]
    });
    let course = Course::from_json(&json.to_string()).expect("valid course");
    let mut player = Player::new(&course).expect("player");
    player.handle(&Input::Start);
    player.handle(&Input::HotspotClicked { x: 150.0, y: 150.0 });
    player.handle(&Input::HotspotClicked { x: 200.0, y: 200.0 });

    let items = compose(&player);
    let marker_items: Vec<_> = items
        .iter()
        .filter(|it| matches!(it.visual, Visual::HotspotMarker))
        .collect();
    assert_eq!(marker_items.len(), 2);
    assert_eq!(marker_items[0].rect.center(), egui::pos2(150.0, 150.0));
    assert_eq!(marker_items[1].rect.center(), egui::pos2(200.0, 200.0));

    let t = SlideTransform::fit(
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0)),
        course.slide_size,
    )
    .expect("transform");
    let ctx = egui::Context::default();
    let raw_input = egui::RawInput::default();
    let output = ctx.run_ui(raw_input, |ui| {
        let painter = ui.painter();
        paint(painter, &t, &items, Highlight::default(), None, None, None);
    });
    output.drop_without_applying_deltas();
}

#[test]
fn pointer_sequence_drag_dropped_with_target() {
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "c_dd",
        "title": "Drag Drop",
        "scenes": [{
            "id": "sc",
            "title": "Scene",
            "slides": [{
                "id": "s1",
                "title": "Slide 1",
                "duration_ms": 1000,
                "objects": [
                    {
                        "id": "item1",
                        "kind": { "type": "shape", "shape": "rectangle", "fill": "#ff0000" },
                        "frame": { "x": 100.0, "y": 100.0, "w": 80.0, "h": 60.0 },
                        "timing": { "start_ms": 0 }
                    },
                    {
                        "id": "target1",
                        "kind": { "type": "shape", "shape": "rectangle", "fill": "#00ff00" },
                        "frame": { "x": 300.0, "y": 100.0, "w": 100.0, "h": 100.0 },
                        "timing": { "start_ms": 0 }
                    },
                    {
                        "id": "btn_normal",
                        "kind": { "type": "button", "label": "Next" },
                        "frame": { "x": 500.0, "y": 500.0, "w": 100.0, "h": 40.0 },
                        "timing": { "start_ms": 0 }
                    }
                ],
                "question": {
                    "kind": "drag_drop",
                    "prompt": "Match",
                    "choices": [],
                    "drag_items": [{ "item": "item1", "target": "target1" }],
                    "drop_targets": ["target1"]
                }
            }]
        }]
    });
    let course = Course::from_json(&json.to_string()).expect("valid course");
    let mut player = Player::new(&course).expect("player");
    player.handle(&Input::Start);

    let items = compose(&player);
    let t = SlideTransform::fit(
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 720.0)),
        course.slide_size,
    )
    .expect("transform");

    let item_rect = items
        .iter()
        .find(|it| it.hit == Hit::DragItem("item1".into()))
        .expect("item")
        .rect;
    let target_rect = items
        .iter()
        .find(|it| it.hit == Hit::Object("target1".into()))
        .expect("target")
        .rect;
    let btn_rect = items
        .iter()
        .find(|it| it.hit == Hit::Object("btn_normal".into()))
        .expect("button")
        .rect;

    let item_screen = t.to_screen_rect(item_rect).center();
    let target_screen = t.to_screen_rect(target_rect).center();
    let outside_screen = egui::pos2(2000.0, 2000.0);
    let btn_screen = t.to_screen_rect(btn_rect).center();

    // 1. Drag item1 onto target1
    let mut state = PointerState {
        drop_targets: vec!["target1".into()],
        ..PointerState::default()
    };
    let events = vec![
        egui::Event::PointerButton {
            pos: item_screen,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        },
        egui::Event::PointerMoved(target_screen),
        egui::Event::PointerButton {
            pos: target_screen,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::default(),
        },
    ];
    let inputs = state.handle_events(&events, &t, &items, &|_| false);
    assert_eq!(inputs.len(), 1);
    match &inputs[0] {
        Input::DragDropped { item, target, .. } => {
            assert_eq!(item, "item1");
            assert_eq!(target.as_deref(), Some("target1"));
        }
        other => panic!("expected DragDropped, got {other:?}"),
    }

    // 2. Drag item1 outside slide -> target: None
    let mut state2 = PointerState {
        drop_targets: vec!["target1".into()],
        ..PointerState::default()
    };
    let events2 = vec![
        egui::Event::PointerButton {
            pos: item_screen,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        },
        egui::Event::PointerMoved(outside_screen),
        egui::Event::PointerButton {
            pos: outside_screen,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::default(),
        },
    ];
    let inputs2 = state2.handle_events(&events2, &t, &items, &|_| false);
    assert_eq!(inputs2.len(), 1);
    match &inputs2[0] {
        Input::DragDropped { item, target, .. } => {
            assert_eq!(item, "item1");
            assert_eq!(*target, None);
        }
        other => panic!("expected DragDropped with None target, got {other:?}"),
    }

    // 3. Normal button click on drag-drop slide still produces Click
    let mut state3 = PointerState {
        drop_targets: vec!["target1".into()],
        ..PointerState::default()
    };
    let events3 = vec![
        egui::Event::PointerButton {
            pos: btn_screen,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        },
        egui::Event::PointerButton {
            pos: btn_screen,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::default(),
        },
    ];
    let inputs3 = state3.handle_events(&events3, &t, &items, &|_| false);
    assert_eq!(
        inputs3,
        vec![Input::Click {
            object: "btn_normal".into()
        }]
    );

    // 4. Escape during drag cancels it
    let mut state4 = PointerState {
        drop_targets: vec!["target1".into()],
        ..PointerState::default()
    };
    let events4 = vec![
        egui::Event::PointerButton {
            pos: item_screen,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        },
        egui::Event::PointerMoved(target_screen),
        egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::default(),
        },
        egui::Event::PointerButton {
            pos: target_screen,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::default(),
        },
    ];
    let inputs4 = state4.handle_events(&events4, &t, &items, &|_| false);
    assert!(
        inputs4.is_empty(),
        "Escape must cancel drag, got {inputs4:?}"
    );
}

#[test]
fn hotspot_click_emits_hotspot_clicked() {
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "c_hs",
        "title": "Hotspot",
        "scenes": [{
            "id": "sc",
            "title": "Scene",
            "slides": [{
                "id": "s1",
                "title": "Slide 1",
                "duration_ms": 1000,
                "objects": [{
                    "id": "bg_img",
                    "kind": { "type": "image", "src": "map.png" },
                    "frame": { "x": 100.0, "y": 100.0, "w": 400.0, "h": 300.0 },
                    "timing": { "start_ms": 0 }
                }],
                "question": {
                    "kind": "hotspot",
                    "prompt": "Click somewhere",
                    "choices": [],
                    "hotspot_image": "bg_img",
                    "regions": [{
                        "id": "r1",
                        "shape": "rect",
                        "x": 120.0,
                        "y": 120.0,
                        "w": 50.0,
                        "h": 50.0,
                        "correct": true
                    }],
                    "markers": 1
                }
            }]
        }]
    });
    let course = Course::from_json(&json.to_string()).expect("valid course");
    let mut player = Player::new(&course).expect("player");
    player.handle(&Input::Start);

    let items = compose(&player);
    let t = SlideTransform::fit(
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 720.0)),
        course.slide_size,
    )
    .expect("transform");

    let img_item = items
        .iter()
        .find(|it| it.hit == Hit::HotspotImage)
        .expect("hotspot image");
    let click_pos = t.to_screen_rect(img_item.rect).center();

    let mut state = PointerState::default();
    let events = vec![
        egui::Event::PointerButton {
            pos: click_pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        },
        egui::Event::PointerButton {
            pos: click_pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::default(),
        },
    ];
    let inputs = state.handle_events(&events, &t, &items, &|_| false);
    assert_eq!(inputs.len(), 1);
    match &inputs[0] {
        Input::HotspotClicked { x, y } => {
            assert!((x - 300.0).abs() < 1e-3);
            assert!((y - 250.0).abs() < 1e-3);
        }
        other => panic!("expected HotspotClicked, got {other:?}"),
    }
}

#[test]
fn drag_items_compose_above_drop_targets() {
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "c_dd_order",
        "title": "Drag order",
        "scenes": [{
            "id": "sc",
            "title": "Scene",
            "slides": [{
                "id": "s1",
                "title": "Slide 1",
                "duration_ms": 1000,
                "objects": [
                    { "id": "item", "kind": { "type": "button", "label": "Cat" },
                      "frame": { "x": 0.0, "y": 0.0, "w": 100.0, "h": 50.0 } },
                    { "id": "target", "kind": { "type": "shape", "shape": "rectangle", "fill": "#DDDDDD" },
                      "frame": { "x": 300.0, "y": 0.0, "w": 200.0, "h": 100.0 } },
                    { "id": "target-label", "kind": { "type": "text", "text": "Target" },
                      "frame": { "x": 310.0, "y": 30.0, "w": 180.0, "h": 30.0 } }
                ],
                "question": {
                    "kind": "drag_drop",
                    "prompt": "Drag",
                    "choices": [],
                    "drag_items": [{ "item": "item", "target": "target" }],
                    "drop_targets": ["target"]
                }
            }]
        }]
    });
    let course = Course::from_json(&json.to_string()).expect("valid course");
    let mut player = Player::new(&course).expect("player");
    player.handle(&Input::Start);
    player.handle(&Input::DragDropped {
        item: "item".into(),
        target: Some("target".into()),
        x: 350.0,
        y: 20.0,
    });
    let items = compose(&player);
    let pos = |id: &str| {
        items
            .iter()
            .position(|it| matches!(&it.hit, Hit::Object(o) | Hit::DragItem(o) if o == id))
            .expect("composed")
    };
    // The moved label is out of the way above the target's frame and takes no
    // hits, so it is only identifiable as the Hit::None text item.
    let label_pos = items
        .iter()
        .position(|it| it.hit == Hit::None && matches!(it.visual, Visual::Text { .. }))
        .expect("moved target label");
    assert!(
        pos("item") > pos("target"),
        "drag item must paint above its target"
    );
    assert!(
        pos("item") > label_pos,
        "the dropped item must paint above the moved target label"
    );
}

fn drag_label_course(target_y: f32) -> Course {
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "c_dd_label",
        "title": "Drag label",
        "scenes": [{
            "id": "sc",
            "title": "Scene",
            "slides": [{
                "id": "s1",
                "title": "Slide 1",
                "duration_ms": 1000,
                "objects": [
                    { "id": "item", "kind": { "type": "text", "text": "Cat" },
                      "frame": { "x": 100.0, "y": 300.0, "w": 120.0, "h": 60.0 } },
                    { "id": "target", "kind": { "type": "shape", "shape": "rectangle", "fill": "#DDDDDD" },
                      "frame": { "x": 400.0, "y": target_y, "w": 240.0, "h": 120.0 } },
                    { "id": "target-label", "kind": { "type": "text", "text": "Animals" },
                      "frame": { "x": 410.0, "y": target_y + 30.0, "w": 220.0, "h": 40.0 } },
                    { "id": "spare", "kind": { "type": "shape", "shape": "rectangle", "fill": "#EEEEEE" },
                      "frame": { "x": 750.0, "y": 420.0, "w": 240.0, "h": 120.0 } },
                    { "id": "spare-label", "kind": { "type": "text", "text": "Empty" },
                      "frame": { "x": 760.0, "y": 450.0, "w": 220.0, "h": 40.0 } }
                ],
                "question": {
                    "kind": "drag_drop",
                    "prompt": "Drag",
                    "choices": [],
                    "drag_items": [{ "item": "item", "target": "target" }],
                    "drop_targets": ["target", "spare"]
                }
            }]
        }]
    });
    Course::from_json(&json.to_string()).expect("valid course")
}

#[test]
fn an_empty_target_keeps_its_label_where_the_author_put_it() {
    let course = drag_label_course(200.0);
    let mut player = Player::new(&course).expect("player");
    player.handle(&Input::Start);
    let items = compose(&player);
    let label = items
        .iter()
        .find(|it| it.hit == Hit::Object("target-label".into()))
        .expect("label composed in place");
    assert_eq!(
        label.rect,
        Rect::from_min_size(pos2(410.0, 230.0), vec2(220.0, 40.0))
    );
}

#[test]
fn a_dropped_item_leaves_the_target_label_readable() {
    let course = drag_label_course(200.0);
    let mut player = Player::new(&course).expect("player");
    player.handle(&Input::Start);
    player.handle(&Input::DragDropped {
        item: "item".into(),
        target: Some("target".into()),
        x: 460.0,
        y: 230.0,
    });

    let items = compose(&player);
    let item_rect = items
        .iter()
        .find(|it| it.hit == Hit::DragItem("item".into()))
        .expect("dropped item")
        .rect;
    let target_rect = items
        .iter()
        .find(|it| it.hit == Hit::Object("target".into()))
        .expect("target")
        .rect;
    // The moved label takes no hits, so it is found as the Hit::None text item.
    let moved = items
        .iter()
        .find(|it| it.hit == Hit::None && matches!(it.visual, Visual::Text { .. }))
        .expect("moved target label");

    assert!(
        !moved.rect.intersects(item_rect),
        "the dropped item's label {item_rect:?} must not be covered by the target label {moved:?}"
    );
    assert!(
        moved.rect.bottom() <= target_rect.top(),
        "the label {moved:?} must sit above the target's frame {target_rect:?}"
    );
    let slide = Size { w: 1280, h: 720 };
    assert!(moved.rect.left() >= 0.0 && moved.rect.right() <= slide.w as f32);
    assert!(moved.rect.top() >= 0.0 && moved.rect.bottom() <= slide.h as f32);

    // The empty target's label stays where the author put it.
    let spare = items
        .iter()
        .find(|it| it.hit == Hit::Object("spare-label".into()))
        .expect("empty target's label");
    assert_eq!(
        spare.rect,
        Rect::from_min_size(pos2(760.0, 450.0), vec2(220.0, 40.0)),
        "an empty target's label must not move"
    );
}

#[test]
fn a_target_at_the_top_of_the_slide_puts_its_label_below() {
    let course = drag_label_course(0.0);
    let mut player = Player::new(&course).expect("player");
    player.handle(&Input::Start);
    player.handle(&Input::DragDropped {
        item: "item".into(),
        target: Some("target".into()),
        x: 460.0,
        y: 30.0,
    });

    let items = compose(&player);
    let target_rect = items
        .iter()
        .find(|it| it.hit == Hit::Object("target".into()))
        .expect("target")
        .rect;
    let item_rect = items
        .iter()
        .find(|it| it.hit == Hit::DragItem("item".into()))
        .expect("dropped item")
        .rect;
    let moved = items
        .iter()
        .find(|it| it.hit == Hit::None && matches!(it.visual, Visual::Text { .. }))
        .expect("moved target label");

    assert!(
        moved.rect.top() >= target_rect.bottom(),
        "with no room above, the label {moved:?} must sit below the target {target_rect:?}"
    );
    assert!(
        !moved.rect.intersects(item_rect),
        "the dropped item's label must not be covered by the target label"
    );
    assert!(moved.rect.top() >= 0.0 && moved.rect.bottom() <= 720.0);
}

mod offscreen_tests {
    use crate::media::MediaFetch;
    use crate::offscreen::{
        RenderError, RenderMode, RenderOptions, RgbaImage, encode_png, render_slide_rgba,
    };
    use coursecraft_course::Course;
    use coursecraft_logic::{HostInfo, Input, Player};

    struct DummyMedia;
    impl crate::media::MediaSource for DummyMedia {
        fn get(&self, _src: &str) -> MediaFetch {
            MediaFetch::Missing
        }
    }

    struct TestImageMediaSource {
        png_bytes: std::sync::Arc<[u8]>,
    }
    impl crate::media::MediaSource for TestImageMediaSource {
        fn get(&self, src: &str) -> MediaFetch {
            if src == "test.png" {
                MediaFetch::Ready(self.png_bytes.clone())
            } else {
                MediaFetch::Missing
            }
        }
    }

    struct LottieMediaSource {
        json: std::sync::Arc<[u8]>,
    }
    impl crate::media::MediaSource for LottieMediaSource {
        fn get(&self, src: &str) -> MediaFetch {
            if src == "tiny.json" {
                MediaFetch::Ready(self.json.clone())
            } else {
                MediaFetch::Missing
            }
        }
    }

    fn tiny_lottie_json() -> String {
        r#"{
          "v":"5.5.7","fr":30,"ip":0,"op":30,"w":100,"h":100,"nm":"tiny","ddd":0,
          "assets":[],
          "layers":[{
            "ddd":0,"ind":1,"ty":4,"nm":"Rect","sr":1,
            "ks":{
              "a":{"a":0,"k":[0,0]},
              "p":{"a":1,"k":[
                {"t":0,"s":[20,50],"e":[80,50],
                 "i":{"x":[0.5],"y":[0.5]},"o":{"x":[0.5],"y":[0.5]}},
                {"t":30,"s":[80,50]}
              ]},
              "s":{"a":0,"k":[100,100]},
              "r":{"a":0,"k":0},
              "o":{"a":0,"k":100}
            },
            "ao":0,
            "shapes":[{"ty":"gr","it":[
              {"ty":"rc","d":1,"s":{"a":0,"k":[20,20]},
               "p":{"a":0,"k":[0,0]},"r":{"a":0,"k":0}},
              {"ty":"fl","c":{"a":0,"k":[1,0,0,1]},"o":{"a":0,"k":100},"r":1},
              {"ty":"tr","p":{"a":0,"k":[0,0]},"a":{"a":0,"k":[0,0]},
               "s":{"a":0,"k":[100,100]},"r":{"a":0,"k":0},"o":{"a":0,"k":100}}
            ]}],
            "ip":0,"op":30,"st":0,"bm":0
          }]
        }"#
        .to_string()
    }

    fn pixel_at(img: &RgbaImage, x: u32, y: u32) -> [u8; 4] {
        let idx = ((y * img.width + x) * 4) as usize;
        [
            img.pixels[idx],
            img.pixels[idx + 1],
            img.pixels[idx + 2],
            img.pixels[idx + 3],
        ]
    }

    fn make_course(slide_json: serde_json::Value) -> Course {
        let json = serde_json::json!({
            "schema_version": 1,
            "id": "offscreen-course-test",
            "title": "Offscreen Test",
            "slide_size": { "w": 1280, "h": 720 },
            "scenes": [{
                "id": "s1",
                "title": "Scene 1",
                "slides": [slide_json]
            }]
        });
        Course::from_json(&json.to_string()).expect("valid course")
    }

    #[test]
    fn callout_tail_outside_the_frame_is_not_clipped() {
        let course = make_course(serde_json::json!({
            "id": "slide1",
            "title": "Callout tail",
            "background": "#FFFFFF",
            "objects": [{
                "id": "callout1",
                "kind": {
                    "type": "shape",
                    "shape": "callout_rectangle",
                    "fill": "#FF0000",
                    "adjust": [0.5, 1.8]
                },
                "frame": { "x": 200, "y": 200, "w": 200, "h": 160 }
            }]
        }));
        let opts = RenderOptions {
            width: 1280,
            mode: RenderMode::Authoring,
            layers: Vec::new(),
            at_ms: None,
        };
        let img = render_slide_rgba(&course, "slide1", &opts, &DummyMedia).expect("render slide");
        assert_eq!(pixel_at(&img, 300, 450), [255, 0, 0, 255]);
        assert_eq!(pixel_at(&img, 300, 500), [255, 255, 255, 255]);
    }

    #[test]
    fn lottie_frame_zero_and_mid_animation_differ_where_the_rectangle_moved() {
        let course = make_course(serde_json::json!({
            "id": "slide-lottie",
            "title": "Lottie",
            "background": "#FFFFFF",
            "objects": [{
                "id": "anim1",
                "kind": { "type": "lottie", "src": "tiny.json" },
                "frame": { "x": 400, "y": 200, "w": 400, "h": 400 }
            }]
        }));
        let media = LottieMediaSource {
            json: tiny_lottie_json().into_bytes().into(),
        };
        let start = RenderOptions {
            width: 640,
            mode: RenderMode::Player,
            layers: Vec::new(),
            at_ms: Some(0),
        };
        let middle = RenderOptions {
            at_ms: Some(500),
            ..start.clone()
        };
        let frame0 = render_slide_rgba(&course, "slide-lottie", &start, &media).expect("frame 0");
        let frame15 =
            render_slide_rgba(&course, "slide-lottie", &middle, &media).expect("frame 15");
        let frame0_pixels = frame0.pixels.as_chunks::<4>().0;
        let frame15_pixels = frame15.pixels.as_chunks::<4>().0;
        let differing = frame0_pixels
            .iter()
            .zip(frame15_pixels.iter())
            .filter(|(a, b)| a != b)
            .count();
        assert!(
            differing > 100,
            "frame 0 and frame 15 should differ where the rectangle moved, got {differing} pixels"
        );
    }

    #[test]
    fn rotated_rectangle_draws_outside_its_unrotated_frame() {
        let course = make_course(serde_json::json!({
            "id": "slide1",
            "title": "Rotated Rect",
            "background": "#FFFFFF",
            "objects": [{
                "id": "rect1",
                "kind": { "type": "shape", "shape": "rectangle", "fill": "#FF0000" },
                "frame": { "x": 540, "y": 260, "w": 200, "h": 200 },
                "rotation": 45
            }]
        }));
        let img = render_slide_rgba(&course, "slide1", &RenderOptions::default(), &DummyMedia)
            .expect("render slide");
        // Above the unrotated frame's top edge, but inside the rotated square.
        assert_eq!(pixel_at(&img, 640, 250), [255, 0, 0, 255]);
        // The unrotated top-left corner is now outside the rotated square.
        assert_eq!(pixel_at(&img, 545, 265), [255, 255, 255, 255]);
    }

    #[test]
    fn flipped_image_mirrors_its_pixels() {
        let pixels = vec![255, 0, 0, 255, 0, 0, 255, 255];
        let png = coursecraft_media::encode_png_rgba(2, 1, &pixels).expect("encode png");
        let media = TestImageMediaSource {
            png_bytes: png.into(),
        };
        let course = make_course(serde_json::json!({
            "id": "slide1",
            "title": "Flipped Image",
            "background": "#FFFFFF",
            "objects": [{
                "id": "image1",
                "kind": { "type": "image", "src": "test.png", "alt": "" },
                "frame": { "x": 200, "y": 200, "w": 200, "h": 100 },
                "flip_h": true
            }]
        }));
        let img = render_slide_rgba(&course, "slide1", &RenderOptions::default(), &media)
            .expect("render slide");
        // The source is red on the left, blue on the right; a horizontal flip
        // swaps them. Bilinear sampling can leave a couple of units of the
        // other channel at a half's edge, so compare the dominant channel.
        let left = pixel_at(&img, 250, 250);
        assert!(left[2] > 200 && left[0] < 30, "left is blue: {left:?}");
        let right = pixel_at(&img, 350, 250);
        assert!(right[0] > 200 && right[2] < 30, "right is red: {right:?}");
    }

    #[test]
    fn solid_red_rectangle_inside_and_outside_pixels() {
        let course = make_course(serde_json::json!({
            "id": "slide1",
            "title": "Red Rect",
            "background": "#FFFFFF",
            "objects": [{
                "id": "rect1",
                "kind": { "type": "shape", "shape": "rectangle", "fill": "#FF0000" },
                "frame": { "x": 200, "y": 200, "w": 200, "h": 200 }
            }]
        }));

        let opts = RenderOptions {
            width: 1280,
            mode: RenderMode::Authoring,
            layers: Vec::new(),
            at_ms: None,
        };

        let img = render_slide_rgba(&course, "slide1", &opts, &DummyMedia).expect("render slide");
        assert_eq!(img.width, 1280);
        assert_eq!(img.height, 720);

        // Pixel inside (300, 300) is red
        let inside = pixel_at(&img, 300, 300);
        assert_eq!(inside, [255, 0, 0, 255]);

        // Pixel outside (50, 50) is background white
        let outside = pixel_at(&img, 50, 50);
        assert_eq!(outside, [255, 255, 255, 255]);
    }

    #[test]
    fn translucent_rectangle_has_no_seam_on_its_diagonal() {
        // Odd sizes put pixel centres exactly on the shared diagonal of the
        // rectangle's two triangles; each must be blended once, not twice.
        let course = make_course(serde_json::json!({
            "id": "slide1",
            "title": "Alpha Rect",
            "background": "#FFFFFF",
            "objects": [{
                "id": "rect1",
                "kind": { "type": "shape", "shape": "rectangle", "fill": "#00000080" },
                "frame": { "x": 100, "y": 100, "w": 601, "h": 397 }
            }]
        }));
        let img = render_slide_rgba(&course, "slide1", &RenderOptions::default(), &DummyMedia)
            .expect("render slide");
        let expected = pixel_at(&img, 400, 300);
        for y in 110..490 {
            for x in 110..690 {
                assert_eq!(pixel_at(&img, x, y), expected, "seam at ({x}, {y})");
            }
        }
    }

    #[test]
    fn text_object_rasterises_dark_pixels_and_none_outside() {
        let course = make_course(serde_json::json!({
            "id": "slide_txt",
            "title": "Text Slide",
            "background": "#FFFFFF",
            "objects": [{
                "id": "txt1",
                "kind": {
                    "type": "text",
                    "text": "Hello World",
                    "style": { "color": "#000000", "size": 32 }
                },
                "frame": { "x": 100, "y": 100, "w": 400, "h": 100 }
            }]
        }));

        let opts = RenderOptions {
            width: 1280,
            mode: RenderMode::Authoring,
            layers: Vec::new(),
            at_ms: None,
        };

        let img = render_slide_rgba(&course, "slide_txt", &opts, &DummyMedia).expect("render text");

        // Inside the text frame, some pixels must be dark glyph pixels
        let mut found_dark = false;
        for y in 100..200 {
            for x in 100..500 {
                let p = pixel_at(&img, x, y);
                if p[0] < 128 && p[1] < 128 && p[2] < 128 {
                    found_dark = true;
                    break;
                }
            }
            if found_dark {
                break;
            }
        }
        assert!(
            found_dark,
            "expected font atlas glyph pixels inside text box"
        );

        // Far outside the text frame (e.g. 50, 50), all pixels are pure background white
        let outside = pixel_at(&img, 50, 50);
        assert_eq!(outside, [255, 255, 255, 255]);
    }

    #[test]
    fn image_object_with_2x2_png_renders_colors() {
        let png_2x2 = coursecraft_media::encode_png_rgba(
            2,
            2,
            &[
                255, 0, 0, 255, // top-left red
                0, 255, 0, 255, // top-right green
                0, 0, 255, 255, // bottom-left blue
                255, 255, 0, 255, // bottom-right yellow
            ],
        )
        .expect("encode 2x2 png");

        let media = TestImageMediaSource {
            png_bytes: png_2x2.into(),
        };

        let course = make_course(serde_json::json!({
            "id": "slide_img",
            "title": "Image Slide",
            "background": "#FFFFFF",
            "objects": [{
                "id": "img1",
                "kind": { "type": "image", "src": "test.png", "alt": "test" },
                "frame": { "x": 100, "y": 100, "w": 200, "h": 200 }
            }]
        }));

        let opts = RenderOptions {
            width: 1280,
            mode: RenderMode::Authoring,
            layers: Vec::new(),
            at_ms: None,
        };

        let img = render_slide_rgba(&course, "slide_img", &opts, &media).expect("render image");

        // Inside the image area (around 120, 120 - near top-left of the 200x200 image), it should be predominantly red
        let p_top_left = pixel_at(&img, 120, 120);
        assert!(
            p_top_left[0] > 180 && p_top_left[2] < 100,
            "top-left should be red-ish: {p_top_left:?}"
        );

        // Near bottom-left (120, 280), it should be predominantly blue
        let p_bot_left = pixel_at(&img, 120, 280);
        assert!(
            p_bot_left[2] > 180 && p_bot_left[0] < 100,
            "bottom-left should be blue-ish: {p_bot_left:?}"
        );
    }

    #[test]
    fn svg_image_rasterises_offscreen_at_the_export_size() {
        struct SvgMedia {
            bytes: std::sync::Arc<[u8]>,
        }
        impl crate::media::MediaSource for SvgMedia {
            fn get(&self, src: &str) -> MediaFetch {
                if src == "test.svg" {
                    MediaFetch::Ready(self.bytes.clone())
                } else {
                    MediaFetch::Missing
                }
            }
        }

        let svg: std::sync::Arc<[u8]> = std::sync::Arc::from(
            &br##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16"><rect width="16" height="16" fill="#00cc00"/></svg>"##[..],
        );
        let media = SvgMedia { bytes: svg };
        let course = make_course(serde_json::json!({
            "id": "slide_svg",
            "title": "SVG Slide",
            "background": "#FFFFFF",
            "objects": [{
                "id": "svg1",
                "kind": { "type": "image", "src": "test.svg", "alt": "vector" },
                "frame": { "x": 100, "y": 100, "w": 200, "h": 200 }
            }]
        }));
        let opts = RenderOptions {
            width: 1280,
            mode: RenderMode::Authoring,
            layers: Vec::new(),
            at_ms: None,
        };
        let img = render_slide_rgba(&course, "slide_svg", &opts, &media).expect("render SVG");
        let inside = pixel_at(&img, 200, 200);
        assert!(
            inside[1] > 150 && inside[0] < 100,
            "SVG rectangle should paint green: {inside:?}"
        );
        let outside = pixel_at(&img, 50, 50);
        assert_eq!(outside, [255, 255, 255, 255]);
    }

    #[test]
    fn width_of_640_halves_coordinates() {
        let course = make_course(serde_json::json!({
            "id": "slide_scale",
            "title": "Scaled Slide",
            "background": "#FFFFFF",
            "objects": [{
                "id": "rect1",
                "kind": { "type": "shape", "shape": "rectangle", "fill": "#FF0000" },
                "frame": { "x": 200, "y": 200, "w": 200, "h": 200 }
            }]
        }));

        let opts = RenderOptions {
            width: 640,
            mode: RenderMode::Authoring,
            layers: Vec::new(),
            at_ms: None,
        };

        let img =
            render_slide_rgba(&course, "slide_scale", &opts, &DummyMedia).expect("render scaled");
        assert_eq!(img.width, 640);
        assert_eq!(img.height, 360);

        // 300, 300 halved is 150, 150
        let inside = pixel_at(&img, 150, 150);
        assert_eq!(inside, [255, 0, 0, 255]);

        // 50, 50 halved is 25, 25
        let outside = pixel_at(&img, 25, 25);
        assert_eq!(outside, [255, 255, 255, 255]);
    }

    #[test]
    fn empty_and_missing_slide_id_returns_error() {
        let course = make_course(serde_json::json!({
            "id": "s1",
            "title": "Slide 1"
        }));

        let opts = RenderOptions::default();
        let res_missing = render_slide_rgba(&course, "does_not_exist", &opts, &DummyMedia);
        assert!(matches!(res_missing, Err(RenderError::SlideNotFound(_))));

        let res_empty = render_slide_rgba(&course, "", &opts, &DummyMedia);
        assert!(matches!(res_empty, Err(RenderError::SlideNotFound(_))));
    }

    #[test]
    fn hostile_course_does_not_panic() {
        // Course with degenerate or huge frames, huge text
        let course = make_course(serde_json::json!({
            "id": "hostile_slide",
            "title": "Hostile Slide",
            "objects": [
                {
                    "id": "nan_rect",
                    "kind": { "type": "shape", "shape": "rectangle", "fill": "#FF0000" },
                    "frame": { "x": 0, "y": 0, "w": 1e9, "h": 1e9 }
                },
                {
                    "id": "huge_text",
                    "kind": {
                        "type": "text",
                        "text": "Huge Text",
                        "style": { "size": 99999 }
                    },
                    "frame": { "x": 0, "y": 0, "w": 500, "h": 500 }
                }
            ]
        }));

        let opts = RenderOptions::default();
        let _ = render_slide_rgba(&course, "hostile_slide", &opts, &DummyMedia);
    }

    #[test]
    fn layers_param_composes_layer_on_top() {
        let course = make_course(serde_json::json!({
            "id": "slide_layer",
            "title": "Layer Slide",
            "background": "#FFFFFF",
            "layers": [{
                "id": "layer_blue",
                "name": "Blue Layer",
                "objects": [{
                    "id": "blue_rect",
                    "kind": { "type": "shape", "shape": "rectangle", "fill": "#0000FF" },
                    "frame": { "x": 100, "y": 100, "w": 200, "h": 200 }
                }]
            }]
        }));

        // Without layer: pixel at (200, 200) is background white
        let opts_no_layer = RenderOptions::default();
        let img_base = render_slide_rgba(&course, "slide_layer", &opts_no_layer, &DummyMedia)
            .expect("render base");
        assert_eq!(pixel_at(&img_base, 200, 200), [255, 255, 255, 255]);

        // With layer: pixel at (200, 200) is blue
        let opts_with_layer = RenderOptions {
            layers: vec!["layer_blue".to_string()],
            ..Default::default()
        };
        let img_layer = render_slide_rgba(&course, "slide_layer", &opts_with_layer, &DummyMedia)
            .expect("render with layer");
        assert_eq!(pixel_at(&img_layer, 200, 200), [0, 0, 255, 255]);
    }

    fn pixel_hash(pixels: &[u8]) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        for &byte in pixels {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash
    }

    fn variable_course() -> Course {
        make_course(serde_json::json!({
            "id": "slide1",
            "title": "Certificate",
            "background": "#FFFFFF",
            "objects": [{
                "id": "name",
                "kind": {
                    "type": "text",
                    "text": "%Learner.Name%",
                    "style": { "size": 48, "color": "#000000", "align": "center" }
                },
                "frame": { "x": 100, "y": 200, "w": 800, "h": 200 }
            }]
        }))
    }

    #[test]
    fn player_render_substitutes_variables_and_differs_from_authoring() {
        let course = variable_course();
        let authored = render_slide_rgba(&course, "slide1", &RenderOptions::default(), &DummyMedia)
            .expect("authoring render");
        let mut player = Player::new(&course).expect("valid player");
        player.set_host_info(HostInfo {
            learner_name: Some("Ada Lovelace".to_owned()),
            date: None,
        });
        player.handle(&Input::Start);
        player.handle(&Input::GoTo {
            slide: "slide1".to_owned(),
        });
        let played =
            crate::render_player_rgba(&player, 640, &[], &DummyMedia, None).expect("player render");
        assert_ne!(
            authored.pixels, played.pixels,
            "the substituted learner name must change the drawn pixels"
        );
    }

    #[test]
    fn player_render_leaves_excluded_objects_at_the_background() {
        let course = make_course(serde_json::json!({
            "id": "slide1",
            "title": "Certificate",
            "background": "#FFFFFF",
            "objects": [{
                "id": "print",
                "kind": { "type": "shape", "shape": "rectangle", "fill": "#FF0000" },
                "frame": { "x": 100, "y": 100, "w": 200, "h": 200 }
            }]
        }));
        let mut player = Player::new(&course).expect("valid player");
        player.handle(&Input::Start);
        player.handle(&Input::GoTo {
            slide: "slide1".to_owned(),
        });
        let img = crate::render_player_rgba(&player, 640, &["print".to_owned()], &DummyMedia, None)
            .expect("player render");
        let x = 200 * img.width / 1280;
        let y = 200 * img.height / 720;
        assert_eq!(pixel_at(&img, x, y), [255, 255, 255, 255]);
    }

    #[test]
    fn player_mode_render_matches_the_fixture_hash() {
        let course = make_course(serde_json::json!({
            "id": "slide1",
            "title": "Player fixture",
            "background": "#F4F1EA",
            "duration_ms": 60000,
            "objects": [{
                "id": "title",
                "kind": {
                    "type": "text",
                    "text": "Player fixture",
                    "style": { "size": 48, "color": "#333333" }
                },
                "frame": { "x": 80, "y": 80, "w": 600, "h": 100 }
            }, {
                "id": "clock",
                "kind": { "type": "timer", "duration_ms": 90000 },
                "frame": { "x": 800, "y": 80, "w": 200, "h": 60 }
            }]
        }));
        let opts = RenderOptions {
            width: 320,
            mode: RenderMode::Player,
            layers: Vec::new(),
            at_ms: None,
        };
        let img = render_slide_rgba(&course, "slide1", &opts, &DummyMedia).expect("render slide");
        assert_eq!(pixel_hash(&img.pixels), 13_610_865_306_066_260_794);
    }

    #[test]
    fn encode_png_produces_valid_png_bytes() {
        let course = make_course(serde_json::json!({
            "id": "s1",
            "title": "Slide"
        }));
        let img = render_slide_rgba(&course, "s1", &RenderOptions::default(), &DummyMedia)
            .expect("render");
        let png = encode_png(&img).expect("encode png");
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
    }

    fn gradient_slide(fill_style: serde_json::Value, rotation: f32) -> Course {
        make_course(serde_json::json!({
            "id": "slide1",
            "title": "Gradient",
            "background": "#FFFFFF",
            "objects": [{
                "id": "shape1",
                "kind": {
                    "type": "shape",
                    "shape": "rectangle",
                    "fill": "#FF0000",
                    "fill_style": fill_style
                },
                "frame": { "x": 200, "y": 200, "w": 200, "h": 200 },
                "rotation": rotation
            }]
        }))
    }

    fn linear_stops() -> serde_json::Value {
        serde_json::json!([
            { "color": "#FF0000", "position": 0.0 },
            { "color": "#0000FF", "position": 1.0 }
        ])
    }

    #[test]
    fn linear_gradient_is_red_on_the_left_and_blue_on_the_right() {
        let course = gradient_slide(
            serde_json::json!({ "type": "linear", "angle": 0, "stops": linear_stops() }),
            0.0,
        );
        let img = render_slide_rgba(&course, "slide1", &RenderOptions::default(), &DummyMedia)
            .expect("render gradient");
        let left = pixel_at(&img, 220, 300);
        let right = pixel_at(&img, 380, 300);
        assert!(left[0] > 200 && left[2] < 60, "left is red: {left:?}");
        assert!(right[2] > 200 && right[0] < 60, "right is blue: {right:?}");
    }

    #[test]
    fn radial_gradient_is_red_at_the_centre_and_blue_at_the_edge() {
        let course = gradient_slide(
            serde_json::json!({
                "type": "radial",
                "center": [0.5, 0.5],
                "stops": linear_stops()
            }),
            0.0,
        );
        let img = render_slide_rgba(&course, "slide1", &RenderOptions::default(), &DummyMedia)
            .expect("render gradient");
        let centre = pixel_at(&img, 300, 300);
        let edge = pixel_at(&img, 397, 397);
        assert!(centre[0] > 200 && centre[2] < 60, "centre red: {centre:?}");
        assert!(edge[2] > 200 && edge[0] < 60, "edge blue: {edge:?}");
    }

    #[test]
    fn half_opacity_red_over_white_is_pink() {
        let course = make_course(serde_json::json!({
            "id": "slide1",
            "title": "Opacity",
            "background": "#FFFFFF",
            "objects": [{
                "id": "rect1",
                "kind": { "type": "shape", "shape": "rectangle", "fill": "#FF0000" },
                "frame": { "x": 200, "y": 200, "w": 200, "h": 200 },
                "opacity": 0.5
            }]
        }));
        let img = render_slide_rgba(&course, "slide1", &RenderOptions::default(), &DummyMedia)
            .expect("render opacity");
        let pixel = pixel_at(&img, 300, 300);
        assert!(pixel[0] > 240, "red channel stays high: {pixel:?}");
        assert!(
            (100..=160).contains(&pixel[1]) && (100..=160).contains(&pixel[2]),
            "white shows through: {pixel:?}"
        );
    }

    #[test]
    fn shadow_paints_outside_the_object_frame() {
        let course = make_course(serde_json::json!({
            "id": "slide1",
            "title": "Shadow",
            "background": "#FFFFFF",
            "objects": [{
                "id": "rect1",
                "kind": { "type": "shape", "shape": "rectangle", "fill": "#FF0000" },
                "frame": { "x": 200, "y": 200, "w": 100, "h": 100 },
                "shadow": { "color": "#000000", "offset_x": 20, "offset_y": 0, "blur": 0 }
            }]
        }));
        let img = render_slide_rgba(&course, "slide1", &RenderOptions::default(), &DummyMedia)
            .expect("render shadow");
        let shadow = pixel_at(&img, 310, 250);
        assert!(
            shadow[0] < 30 && shadow[1] < 30 && shadow[2] < 30,
            "shadow pixel is dark: {shadow:?}"
        );
    }

    #[test]
    fn rotated_gradient_shape_still_draws_its_gradient() {
        let course = gradient_slide(
            serde_json::json!({ "type": "linear", "angle": 0, "stops": linear_stops() }),
            45.0,
        );
        let mut rotated = course;
        if let Some(object) = rotated.scenes[0].slides[0].objects.first_mut() {
            object.frame.x = 540.0;
            object.frame.y = 260.0;
        }
        let img = render_slide_rgba(&rotated, "slide1", &RenderOptions::default(), &DummyMedia)
            .expect("render rotated gradient");
        let inside_rotated = pixel_at(&img, 640, 250);
        assert!(
            inside_rotated[0] > 100 || inside_rotated[2] > 100,
            "rotated gradient fills the corner: {inside_rotated:?}"
        );
        let outside = pixel_at(&img, 545, 265);
        assert_eq!(outside, [255, 255, 255, 255]);
    }

    #[test]
    fn picture_filled_ellipse_leaves_frame_corners_transparent() {
        let pixels = vec![
            255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255,
        ];
        let png = coursecraft_media::encode_png_rgba(2, 2, &pixels).expect("encode png");
        let media = TestImageMediaSource {
            png_bytes: png.into(),
        };
        let course = make_course(serde_json::json!({
            "id": "slide1",
            "title": "Picture fill",
            "background": "#FFFFFF",
            "objects": [{
                "id": "shape1",
                "kind": {
                    "type": "shape",
                    "shape": "ellipse",
                    "fill": "#D9D9D9",
                    "fill_style": { "type": "picture", "src": "test.png", "mode": "stretch" }
                },
                "frame": { "x": 540, "y": 260, "w": 200, "h": 200 }
            }]
        }));
        let img = render_slide_rgba(&course, "slide1", &RenderOptions::default(), &media)
            .expect("render picture fill");
        let centre = pixel_at(&img, 640, 360);
        assert!(
            centre[0] > 200 && centre[1] < 50,
            "centre is the picture: {centre:?}"
        );
        let corner = pixel_at(&img, 545, 265);
        assert_eq!(corner, [255, 255, 255, 255], "corner is untouched");
    }

    #[test]
    fn web_object_draws_a_placeholder_card() {
        let course = make_course(serde_json::json!({
            "id": "slide1",
            "title": "Web",
            "background": "#FFFFFF",
            "objects": [{
                "id": "web1",
                "kind": {
                    "type": "web",
                    "source": { "type": "url", "url": "https://example.com/lesson" }
                },
                "frame": { "x": 440, "y": 240, "w": 400, "h": 240 }
            }]
        }));
        let img = render_slide_rgba(&course, "slide1", &RenderOptions::default(), &DummyMedia)
            .expect("render web placeholder");
        let outside = pixel_at(&img, 50, 50);
        assert_eq!(outside, [255, 255, 255, 255], "slide behind is white");
        let inside = pixel_at(&img, 640, 260);
        assert_ne!(
            inside, outside,
            "the placeholder card must paint over the slide"
        );
        assert!(
            inside[0] < 100 && inside[1] < 100 && inside[2] < 100,
            "the card uses the dark design-token surface, got {inside:?}"
        );
    }

    #[test]
    fn marker_dot_uses_its_colour() {
        let course = make_course(serde_json::json!({
            "id": "slide1",
            "title": "Marker",
            "background": "#FFFFFF",
            "objects": [{
                "id": "marker",
                "kind": { "type": "marker", "title": "Info", "color": "#FF0000" },
                "frame": { "x": 100, "y": 100, "w": 100, "h": 100 }
            }]
        }));
        let img = render_slide_rgba(&course, "slide1", &RenderOptions::default(), &DummyMedia)
            .expect("render marker");
        let dot = pixel_at(&img, 115, 150);
        assert_eq!(dot, [255, 0, 0, 255], "the dot uses its stored colour");
        let outside = pixel_at(&img, 50, 50);
        assert_eq!(outside, [255, 255, 255, 255]);
    }

    #[test]
    fn panel_clips_a_child_outside_its_viewport() {
        let course = make_course(serde_json::json!({
            "id": "slide1",
            "title": "Panel",
            "background": "#FFFFFF",
            "objects": [
                { "id": "panel",
                  "kind": { "type": "scroll_panel", "content_height": 600.0 },
                  "frame": { "x": 100, "y": 100, "w": 300, "h": 200 } },
                { "id": "inside", "panel": "panel",
                  "kind": { "type": "button", "label": "A", "fill": "#FF0000" },
                  "frame": { "x": 120, "y": 120, "w": 100, "h": 50 } },
                { "id": "outside", "panel": "panel",
                  "kind": { "type": "button", "label": "B", "fill": "#0000FF" },
                  "frame": { "x": 120, "y": 500, "w": 100, "h": 50 } }
            ]
        }));
        let opts = RenderOptions {
            mode: RenderMode::Player,
            ..RenderOptions::default()
        };
        let img = render_slide_rgba(&course, "slide1", &opts, &DummyMedia).expect("render panel");
        assert_eq!(
            pixel_at(&img, 140, 140),
            [255, 0, 0, 255],
            "the child inside the viewport is drawn"
        );
        assert_eq!(
            pixel_at(&img, 140, 520),
            [255, 255, 255, 255],
            "a child below the viewport is clipped away"
        );
    }

    #[test]
    fn panel_draws_a_scrollbar_thumb_only_when_the_bar_is_visible() {
        let make = |bar: &str| {
            make_course(serde_json::json!({
                "id": "slide1",
                "title": "Panel",
                "background": "#FFFFFF",
                "objects": [{
                    "id": "panel",
                    "kind": { "type": "scroll_panel", "content_height": 600.0, "bar": bar },
                    "frame": { "x": 100, "y": 100, "w": 300, "h": 200 }
                }]
            }))
        };
        let opts = RenderOptions {
            mode: RenderMode::Player,
            ..RenderOptions::default()
        };
        let visible = render_slide_rgba(&make("visible"), "slide1", &opts, &DummyMedia)
            .expect("render visible bar");
        let thumb = pixel_at(&visible, 393, 130);
        assert_ne!(thumb, [255, 255, 255, 255], "the thumb is painted");

        let hidden = render_slide_rgba(&make("hidden"), "slide1", &opts, &DummyMedia)
            .expect("render hidden bar");
        assert_eq!(
            pixel_at(&hidden, 393, 130),
            [255, 255, 255, 255],
            "no thumb when the bar is hidden"
        );
    }
}

#[cfg(test)]
mod caption_tests {
    use std::cell::Cell;
    use std::sync::Arc;

    use coursecraft_course::Course;
    use coursecraft_logic::{Input, Player};
    use coursecraft_playback::{MediaBackend, MediaKind, NullBackend};

    use crate::media::{MediaFetch, MediaSource};
    use crate::media_driver::{CaptionStore, MediaDriver};
    use crate::view::SlideView;

    const COURSE: &str = r#"{
      "schema_version": 1,
      "id": "captions",
      "title": "Captions",
      "slide_size": { "w": 960, "h": 540 },
      "scenes": [{
        "id": "sc",
        "title": "Scene",
        "slides": [{
          "id": "s1",
          "title": "One",
          "duration_ms": 10000,
          "objects": [
            { "id": "audio1", "name": "Audio",
              "kind": { "type": "audio", "src": "a.wav", "autoplay": false, "captions": "a.vtt" },
              "frame": { "x": 0.0, "y": 0.0, "w": 48.0, "h": 48.0 } },
            { "id": "video1", "name": "Video",
              "kind": { "type": "video", "src": "v.mp4", "autoplay": false, "captions": "v.vtt" },
              "frame": { "x": 80.0, "y": 60.0, "w": 480.0, "h": 270.0 } }
          ]
        }]
      }]
    }"#;

    const AUDIO_VTT: &str =
        "WEBVTT\n\n00:00:01.000 --> 00:00:02.000\nhello\n\n00:00:03.000 --> 00:00:04.000\nbye\n";
    const VIDEO_VTT: &str = "WEBVTT\n\n00:00:01.000 --> 00:00:02.000\nvideo words\n";

    struct CaptionSource;

    impl MediaSource for CaptionSource {
        fn get(&self, src: &str) -> MediaFetch {
            match src {
                "a.vtt" => MediaFetch::Ready(Arc::from(AUDIO_VTT.as_bytes())),
                "v.vtt" => MediaFetch::Ready(Arc::from(VIDEO_VTT.as_bytes())),
                _ => MediaFetch::Ready(Arc::from(&b"media"[..])),
            }
        }
    }

    fn setup(
        audio_autoplay: bool,
        video_autoplay: bool,
    ) -> (Player<'static>, MediaDriver, NullBackend) {
        let mut course = Course::from_json(COURSE).expect("course");
        for object in &mut course.scenes[0].slides[0].objects {
            match &mut object.kind {
                coursecraft_course::ObjectKind::Audio { autoplay, .. } => {
                    *autoplay = audio_autoplay;
                }
                coursecraft_course::ObjectKind::Video { autoplay, .. } => {
                    *autoplay = video_autoplay;
                }
                _ => {}
            }
        }
        let course: &'static Course = Box::leak(Box::new(course));
        let mut player = Player::new(course).expect("player");
        let effects = player.handle(&Input::Start);
        let backend = NullBackend::new();
        let mut driver = MediaDriver::new().with_backend(Box::new(backend.clone()));
        driver.apply(&player, &effects, Some(&CaptionSource));
        (player, driver, backend)
    }

    #[test]
    fn caption_text_follows_the_backend_position() {
        let (player, mut driver, backend) = setup(true, false);
        let view = SlideView::new();

        backend.set_position_ms("audio1", 500);
        assert!(
            driver
                .caption_frame(&player, Some(&CaptionSource), &view)
                .is_none()
        );

        backend.set_position_ms("audio1", 1_500);
        let frame = driver
            .caption_frame(&player, Some(&CaptionSource), &view)
            .expect("caption");
        assert_eq!(frame.object, "audio1");
        assert_eq!(frame.kind, MediaKind::Audio);
        assert_eq!(frame.text, "hello");

        backend.set_position_ms("audio1", 3_500);
        let frame = driver
            .caption_frame(&player, Some(&CaptionSource), &view)
            .expect("caption");
        assert_eq!(frame.text, "bye");
    }

    #[test]
    fn captions_follow_a_seek() {
        let (mut player, mut driver, backend) = setup(true, false);
        let view = SlideView::new();

        let effects = player.handle(&Input::PlayerSeek { ms: 3_500 });
        driver.apply(&player, &effects, Some(&CaptionSource));
        assert_eq!(backend.position_ms("audio1"), Some(3_500));
        let frame = driver
            .caption_frame(&player, Some(&CaptionSource), &view)
            .expect("caption after seek");
        assert_eq!(frame.text, "bye");

        let effects = player.handle(&Input::PlayerSeek { ms: 1_500 });
        driver.apply(&player, &effects, Some(&CaptionSource));
        let frame = driver
            .caption_frame(&player, Some(&CaptionSource), &view)
            .expect("caption after seeking back");
        assert_eq!(frame.text, "hello");
    }

    #[test]
    fn captions_clear_when_the_object_stops() {
        let (mut player, mut driver, backend) = setup(true, false);
        let view = SlideView::new();
        backend.set_position_ms("audio1", 1_500);
        assert!(
            driver
                .caption_frame(&player, Some(&CaptionSource), &view)
                .is_some()
        );

        let effects = player.handle(&Input::MediaEnded {
            object: "audio1".to_owned(),
        });
        driver.apply(&player, &effects, Some(&CaptionSource));
        assert!(
            driver
                .caption_frame(&player, Some(&CaptionSource), &view)
                .is_none()
        );
    }

    #[test]
    fn video_wins_when_both_are_playing() {
        let (player, mut driver, backend) = setup(true, true);
        let view = SlideView::new();
        backend.set_position_ms("audio1", 1_500);
        backend.set_position_ms("video1", 1_500);

        let frame = driver
            .caption_frame(&player, Some(&CaptionSource), &view)
            .expect("caption");
        assert_eq!(frame.object, "video1");
        assert_eq!(frame.kind, MediaKind::Video);
        assert_eq!(frame.text, "video words");
    }

    #[test]
    fn caption_store_logs_and_fetches_a_bad_file_once() {
        struct Bad(Cell<usize>);
        impl MediaSource for Bad {
            fn get(&self, _src: &str) -> MediaFetch {
                self.0.set(self.0.get().saturating_add(1));
                MediaFetch::Ready(Arc::from(&b"not a caption file"[..]))
            }
        }
        let source = Bad(Cell::new(0));
        let mut store = CaptionStore::new();
        assert_eq!(store.text_at("bad.vtt", Some(&source), 0), None);
        assert_eq!(store.text_at("bad.vtt", Some(&source), 1_000), None);
        assert_eq!(source.0.get(), 1);
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn pending_caption_files_are_retried_next_frame() {
        struct Gate {
            ready: Cell<bool>,
            calls: Cell<usize>,
        }
        impl MediaSource for Gate {
            fn get(&self, _src: &str) -> MediaFetch {
                self.calls.set(self.calls.get().saturating_add(1));
                if self.ready.get() {
                    MediaFetch::Ready(Arc::from(AUDIO_VTT.as_bytes()))
                } else {
                    MediaFetch::Pending
                }
            }
        }
        let source = Gate {
            ready: Cell::new(false),
            calls: Cell::new(0),
        };
        let mut store = CaptionStore::new();
        assert_eq!(store.text_at("a.vtt", Some(&source), 1_500), None);
        assert!(store.is_empty());
        source.ready.set(true);
        assert_eq!(
            store.text_at("a.vtt", Some(&source), 1_500),
            Some("hello".to_owned())
        );
        assert_eq!(source.calls.get(), 2);
    }
}

#[test]
fn course_and_render_contrast_agree() {
    let pairs = [
        ("#000000", "#FFFFFF"),
        ("#1F1F1F", "#FFFFFF"),
        ("#2563EB", "#FFFFFF"),
        ("#16A34A", "#F8FAFC"),
        ("#DC2626", "#FEF2F2"),
        ("#767676", "#FFFFFF"),
        ("#949494", "#FFFFFF"),
    ];
    for (foreground, background) in pairs {
        let fg = Color(foreground.to_owned());
        let bg = Color(background.to_owned());
        let course_fg = coursecraft_course::parse_color(&fg).expect("course colour");
        let course_bg = coursecraft_course::parse_color(&bg).expect("course colour");
        let ours = coursecraft_course::contrast_ratio(course_fg, course_bg);

        let render_fg = parse_color(&fg).expect("render colour");
        let render_bg = parse_color(&bg).expect("render colour");
        let theirs = f64::from(crate::design::contrast_ratio(render_fg, render_bg));

        assert!(
            (ours - theirs).abs() < 1e-4,
            "{foreground} on {background}: course {ours:.6}, render {theirs:.6}"
        );
    }
}

#[test]
fn bundled_inter_has_glyphs_for_latin_greek_and_cyrillic_only() {
    let ctx = egui::Context::default();
    crate::install_fonts(&ctx);
    let raw = || egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(100.0, 100.0),
        )),
        ..Default::default()
    };
    ctx.run_ui(raw(), |_ui| {}).drop_without_applying_deltas();
    let font = egui::FontId::new(20.0, egui::FontFamily::Name("inter".into()));
    for c in ['A', 'é', 'Ω', 'Ж'] {
        assert!(
            ctx.fonts_mut(|fonts| fonts.has_glyph(&font, c)),
            "Inter should cover {c:?}"
        );
    }
    for c in ['漢', 'ع', 'א', 'ह', 'ก'] {
        assert!(
            !ctx.fonts_mut(|fonts| fonts.has_glyph(&font, c)),
            "Inter should not cover {c:?}"
        );
    }
}

// ---------------------------------------------------------------- masters

fn course_with_master() -> Course {
    let mut course = Course::from_json(HELLO_JSON).expect("hello course");
    let mut master = course.scenes[0].slides[0].clone();
    master.id = "master1".to_owned();
    master.title = "Master".to_owned();
    master.triggers.clear();
    master.layers.clear();
    master.question = None;
    master.background = Some(Color("#123456".to_owned()));
    let mut shape = master.objects[0].clone();
    shape.id = "master-shape".to_owned();
    shape.kind = coursecraft_course::ObjectKind::Shape {
        shape: coursecraft_course::ShapeKind::Rectangle,
        fill: Color("#FF0000".to_owned()),
        fill_style: None,
        outline: None,
        text: None,
        adjust: Vec::new(),
    };
    master.objects = vec![shape];
    course.masters.push(master);
    course.scenes[0].slides[0].master = Some("master1".to_owned());
    course
}

#[test]
fn master_objects_draw_under_slide_objects_and_never_take_a_hit() {
    let course = course_with_master();
    let slide = course.scenes[0].slides[0].clone();
    let items = compose_authoring(&course, &slide, None);
    assert!(items.len() > 1);
    assert_eq!(items[0].hit, Hit::None);
    assert!(matches!(items[0].visual, Visual::Shape { .. }));
    assert!(
        items
            .iter()
            .all(|item| item.hit != Hit::Object("master-shape".to_owned()))
    );
    let first_slide_object = &slide.objects[0];
    assert_eq!(items[1].hit, Hit::Object(first_slide_object.id.clone()));
}

#[test]
fn master_objects_dim_with_base_objects_when_a_layer_is_focused() {
    let course = course_with_master();
    let slide = course.scenes[0].slides[0].clone();
    let items = compose_authoring(&course, &slide, Some("about"));
    assert!(items[0].dimmed);
}

#[test]
fn a_hidden_master_object_is_not_drawn() {
    let mut course = course_with_master();
    course.masters[0].objects[0].initial_state = "hidden".to_owned();
    let slide = course.scenes[0].slides[0].clone();
    let with_hidden = compose_authoring(&course, &slide, None);

    let mut plain = course.clone();
    plain.scenes[0].slides[0].master = None;
    let without_master = compose_authoring(&plain, &slide, None);
    assert_eq!(with_hidden.len(), without_master.len());
    assert_eq!(
        with_hidden.first().map(|item| item.hit.clone()),
        without_master.first().map(|item| item.hit.clone())
    );
}

#[test]
fn player_composition_draws_the_master_without_a_hit() {
    let course = course_with_master();
    let mut player = Player::new(&course).expect("valid");
    player.handle(&Input::Start);
    player.handle(&Input::GoTo {
        slide: "s-intro".to_owned(),
    });
    let items = compose(&player);
    assert!(!items.is_empty());
    assert_eq!(items[0].hit, Hit::None);
    assert!(matches!(items[0].visual, Visual::Shape { .. }));
}

#[test]
fn background_prefers_slide_then_master_then_theme() {
    let mut course = course_with_master();
    let slide = course.scenes[0].slides[0].clone();
    assert_eq!(course.slide_background(&slide).0, "#123456");
    course.scenes[0].slides[0].background = Some(Color("#ABCDEF".to_owned()));
    let slide = course.scenes[0].slides[0].clone();
    assert_eq!(course.slide_background(&slide).0, "#ABCDEF");
    course.scenes[0].slides[0].background = None;
    course.scenes[0].slides[0].master = None;
    let slide = course.scenes[0].slides[0].clone();
    assert_eq!(course.slide_background(&slide).0, course.theme.background.0);
}

// ---------------------------------------------------------------- layer options

#[test]
fn compose_hides_base_objects_while_a_layer_asks() {
    let mut course = Course::from_json(HELLO_JSON).expect("valid course");
    if let Some(layer) = course
        .slide_mut("s-intro")
        .and_then(|slide| slide.layers.first_mut())
    {
        layer.hide_base_objects = true;
    }
    let mut player = Player::new(&course).expect("player");
    player.handle(&Input::Start);
    player.handle(&Input::Click {
        object: "btn-about".to_owned(),
    });
    assert!(player.base_objects_hidden());

    let items = compose(&player);
    for base in ["title", "btn-start", "btn-about"] {
        assert!(
            !items
                .iter()
                .any(|item| item.hit == Hit::Object(base.to_owned())),
            "{base} stays out of the composition"
        );
    }
    assert!(
        items
            .iter()
            .any(|item| item.hit == Hit::Object("about-panel".to_owned())),
        "layer objects still draw"
    );
}

#[test]
fn compose_hides_base_question_and_keeps_master_items() {
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "base-hidden",
        "title": "Base Hidden",
        "scenes": [{
            "id": "scene1",
            "title": "Scene 1",
            "slides": [{
                "id": "s1",
                "title": "Slide 1",
                "duration_ms": 5000,
                "master": "m1",
                "objects": [
                    {
                        "id": "btn_show",
                        "kind": { "type": "button", "label": "Show" },
                        "frame": { "x": 0.0, "y": 0.0, "w": 100.0, "h": 50.0 }
                    }
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
                    "objects": [{
                        "id": "modal-text",
                        "kind": { "type": "text", "text": "On the layer" },
                        "frame": { "x": 100.0, "y": 100.0, "w": 300.0, "h": 80.0 }
                    }]
                }],
                "triggers": [{
                    "when": { "event": "click", "object": "btn_show" },
                    "action": { "action": "show_layer", "layer": "modal" }
                }]
            }]
        }],
        "masters": [{
            "id": "m1",
            "title": "Master",
            "duration_ms": 5000,
            "objects": [{
                "id": "master-text",
                "kind": { "type": "text", "text": "Master text" },
                "frame": { "x": 0.0, "y": 600.0, "w": 300.0, "h": 50.0 }
            }]
        }]
    });
    let course = Course::from_json(&json.to_string()).expect("valid course");
    let mut player = Player::new(&course).expect("player");
    player.handle(&Input::Start);
    player.handle(&Input::Click {
        object: "btn_show".to_owned(),
    });
    assert!(player.base_objects_hidden());

    let items = compose(&player);
    assert!(!items.iter().any(|item| matches!(
        item.visual,
        Visual::Prompt { .. } | Visual::ChoiceRow { .. }
    )));
    assert!(
        !items
            .iter()
            .any(|item| item.hit == Hit::Object("btn_show".to_owned()))
    );
    assert!(items.iter().any(|item| matches!(
        &item.visual,
        Visual::Text { runs, .. } if runs.iter().any(|run| run.text == "Master text")
    )));
    assert!(
        items
            .iter()
            .any(|item| item.hit == Hit::Object("modal-text".to_owned()))
    );
}

#[test]
fn compose_authoring_omits_base_objects_when_the_focused_layer_hides_them() {
    let mut course = Course::from_json(HELLO_JSON).expect("valid course");
    if let Some(layer) = course
        .slide_mut("s-intro")
        .and_then(|slide| slide.layers.first_mut())
    {
        layer.hide_base_objects = true;
    }
    let slide = course.slide("s-intro").expect("s-intro");
    let items = compose_authoring(&course, slide, Some("about"));
    assert_eq!(items.len(), 3, "only the focused layer's objects draw");
    assert!(
        items.iter().all(|item| !item.dimmed),
        "Dim has nothing to dim"
    );
    assert!(items.iter().all(|item| matches!(
        &item.hit,
        Hit::Object(id) if id == "about-panel" || id == "about-text" || id == "btn-close"
    )));
}

#[test]
fn a_diagonal_path_hits_its_line_not_its_bounding_box() {
    let mut course = Course::from_json(HELLO_JSON).expect("valid course");
    let slide = &mut course.scenes[0].slides[0];
    slide.objects.push(coursecraft_course::Object {
        id: "diagonal".into(),
        name: "Diagonal".into(),
        kind: coursecraft_course::ObjectKind::Path {
            path: coursecraft_course::Path {
                points: vec![
                    coursecraft_course::PathPoint { x: 0.0, y: 0.0 },
                    coursecraft_course::PathPoint { x: 1.0, y: 1.0 },
                ],
                style: coursecraft_course::PathStyle::Straight,
                closed: false,
                fill: None,
                outline: coursecraft_course::Outline {
                    color: Color("#1F1F1F".into()),
                    width: 2.0,
                    dash: coursecraft_course::DashStyle::Solid,
                },
                start_end: coursecraft_course::LineEnd::None,
                end_end: coursecraft_course::LineEnd::None,
                connector: None,
                start_glue: None,
                end_glue: None,
            },
        },
        frame: ModelRect {
            x: 100.0,
            y: 100.0,
            w: 400.0,
            h: 400.0,
        },
        timing: coursecraft_course::Timing::default(),
        states: Vec::new(),
        initial_state: "normal".into(),
        entrance: None,
        exit: None,
        motion_paths: Vec::new(),
        focus_order: None,
        decorative: false,
        description: None,
        group: None,
        panel: None,
        rotation: 0.0,
        flip_h: false,
        flip_v: false,
        opacity: 1.0,
        shadow: None,
        glow: None,
        drag: None,
    });
    let player = Player::new(&course).expect("valid player");
    let items = compose(&player);

    // On the line.
    assert_eq!(
        crate::hit_test(&items, pos2(300.0, 300.0)),
        Some(Hit::Object("diagonal".into()))
    );
    // Inside the frame but far from the diagonal line: it must not steal the
    // click from whatever is behind it.
    assert_ne!(
        crate::hit_test(&items, pos2(460.0, 140.0)),
        Some(Hit::Object("diagonal".into()))
    );
}

#[test]
fn a_closed_filled_path_hits_inside_its_outline() {
    let object = coursecraft_course::Object {
        id: "box".into(),
        name: "Box".into(),
        kind: coursecraft_course::ObjectKind::Path {
            path: coursecraft_course::Path {
                points: vec![
                    coursecraft_course::PathPoint { x: 0.5, y: 0.0 },
                    coursecraft_course::PathPoint { x: 1.0, y: 0.5 },
                    coursecraft_course::PathPoint { x: 0.5, y: 1.0 },
                    coursecraft_course::PathPoint { x: 0.0, y: 0.5 },
                ],
                style: coursecraft_course::PathStyle::Straight,
                closed: true,
                fill: Some(Color("#2F6FEB".into())),
                outline: coursecraft_course::Outline {
                    color: Color("#1F1F1F".into()),
                    width: 2.0,
                    dash: coursecraft_course::DashStyle::Solid,
                },
                start_end: coursecraft_course::LineEnd::None,
                end_end: coursecraft_course::LineEnd::None,
                connector: None,
                start_glue: None,
                end_glue: None,
            },
        },
        frame: ModelRect {
            x: 100.0,
            y: 100.0,
            w: 200.0,
            h: 200.0,
        },
        timing: coursecraft_course::Timing::default(),
        states: Vec::new(),
        initial_state: "normal".into(),
        entrance: None,
        exit: None,
        motion_paths: Vec::new(),
        focus_order: None,
        decorative: false,
        description: None,
        group: None,
        panel: None,
        rotation: 0.0,
        flip_h: false,
        flip_v: false,
        opacity: 1.0,
        shadow: None,
        glow: None,
        drag: None,
    };
    let item = compose_object(&object, "normal").expect("composed path");
    assert!(crate::compose::item_hit_test(&item, pos2(200.0, 200.0)));
    assert!(!crate::compose::item_hit_test(&item, pos2(104.0, 104.0)));
}

#[test]
fn slide_font_resolves_exact_face_then_regular_then_inter() {
    use coursecraft_course::{FontFace, FontStyle};
    use egui::FontFamily;

    struct FontMedia;
    impl crate::media::MediaSource for FontMedia {
        fn get(&self, src: &str) -> crate::media::MediaFetch {
            if src == "fonts/loud.ttf" {
                crate::media::MediaFetch::Ready(std::sync::Arc::from(LOUD_FONT))
            } else {
                crate::media::MediaFetch::Missing
            }
        }
    }

    let mut course = Course::from_json(HELLO_JSON).expect("course");
    course.fonts.push(FontFace {
        family: "Loud".into(),
        style: FontStyle::Regular,
        src: "fonts/loud.ttf".into(),
    });
    let ctx = egui::Context::default();
    crate::install_course_fonts(&ctx, &course, &FontMedia);
    // Fonts become available at the start of the next pass.
    ctx.run_ui(egui::RawInput::default(), |_ui| {})
        .drop_without_applying_deltas();

    let exact = crate::slide_font_named(&ctx, Some("Loud"), false, false, 20.0);
    assert_eq!(
        exact.family,
        crate::course_face_family("Loud", FontStyle::Regular)
    );

    // No bold face: the family's regular face is used.
    let bold = crate::slide_font_named(&ctx, Some("Loud"), true, false, 20.0);
    assert_eq!(
        bold.family,
        crate::course_face_family("Loud", FontStyle::Regular)
    );

    let italic = crate::slide_font_named(&ctx, Some("Loud"), false, true, 20.0);
    assert_eq!(
        italic.family,
        crate::course_face_family("Loud", FontStyle::Regular)
    );

    let unknown = crate::slide_font_named(&ctx, Some("Missing"), false, false, 20.0);
    assert_eq!(unknown.family, FontFamily::Name("inter".into()));

    let inter_bold = crate::slide_font_named(&ctx, Some("Inter"), true, false, 20.0);
    assert_eq!(inter_bold.family, FontFamily::Name("inter-bold".into()));
}

const LOUD_FONT: &[u8] = include_bytes!("../fonts/Inter-Bold.ttf");

fn course_with_font_text(font: &str, fonts: Vec<coursecraft_course::FontFace>) -> Course {
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "font-render-test",
        "title": "Font render test",
        "slide_size": { "w": 640, "h": 360 },
        "fonts": fonts,
        "scenes": [{
            "id": "s1",
            "title": "Scene",
            "slides": [{
                "id": "slide1",
                "title": "Slide",
                "background": "#FFFFFF",
                "objects": [{
                    "id": "text1",
                    "kind": {
                        "type": "text",
                        "text": "WAVE HAND",
                        "style": { "font": font, "size": 90, "color": "#000000" }
                    },
                    "frame": { "x": 20, "y": 40, "w": 600, "h": 240 }
                }]
            }]
        }]
    });
    Course::from_json(&json.to_string()).expect("valid course")
}

#[test]
fn offscreen_render_differs_between_inter_and_a_course_font() {
    struct FontMedia;
    impl crate::media::MediaSource for FontMedia {
        fn get(&self, src: &str) -> crate::media::MediaFetch {
            if src == "fonts/loud.ttf" {
                crate::media::MediaFetch::Ready(std::sync::Arc::from(LOUD_FONT))
            } else {
                crate::media::MediaFetch::Missing
            }
        }
    }

    let inter_course = course_with_font_text("Inter", Vec::new());
    let loud_course = course_with_font_text(
        "Loud",
        vec![coursecraft_course::FontFace {
            family: "Loud".into(),
            style: coursecraft_course::FontStyle::Regular,
            src: "fonts/loud.ttf".into(),
        }],
    );
    let opts = crate::RenderOptions {
        width: 640,
        ..crate::RenderOptions::default()
    };
    let inter =
        crate::render_slide_rgba(&inter_course, "slide1", &opts, &FontMedia).expect("render inter");
    let loud = crate::render_slide_rgba(&loud_course, "slide1", &opts, &FontMedia)
        .expect("render course font");
    assert_ne!(
        inter.pixels, loud.pixels,
        "the course font must change the rendered pixels"
    );
}

fn table_course(header_row: bool) -> Course {
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "table-course",
        "title": "Table",
        "slide_size": { "w": 1280, "h": 720 },
        "scenes": [{
            "id": "scene1",
            "title": "Scene 1",
            "slides": [{
                "id": "slide1",
                "title": "Slide 1",
                "duration_ms": 5000,
                "objects": [{
                    "id": "table1",
                    "kind": {
                        "type": "table",
                        "columns": [0.5, 0.5],
                        "rows": [
                            {
                                "height": 0.5,
                                "cells": [
                                    { "text": "Header", "fill": "#FF0000" },
                                    { "text": "Second" }
                                ]
                            },
                            {
                                "height": 0.5,
                                "cells": [
                                    { "text": "A" },
                                    { "text": "B" }
                                ]
                            }
                        ],
                        "borders": {
                            "outer": { "color": "#000000", "width": 4 },
                            "inner": { "color": "#000000", "width": 4 }
                        },
                        "header_row": header_row
                    },
                    "frame": { "x": 100, "y": 100, "w": 400, "h": 300 }
                }]
            }]
        }]
    });
    Course::from_json(&json.to_string()).expect("valid table course")
}

#[test]
fn table_renders_cell_fill_and_inner_border_offscreen() {
    struct NoMedia;
    impl crate::media::MediaSource for NoMedia {
        fn get(&self, _src: &str) -> crate::media::MediaFetch {
            crate::media::MediaFetch::Missing
        }
    }
    let course = table_course(false);
    let image = crate::render_slide_rgba(
        &course,
        "slide1",
        &crate::RenderOptions::default(),
        &NoMedia,
    )
    .expect("render table");
    let pixel = |x: u32, y: u32| -> [u8; 4] {
        let index = ((y * image.width + x) * 4) as usize;
        [
            image.pixels[index],
            image.pixels[index + 1],
            image.pixels[index + 2],
            image.pixels[index + 3],
        ]
    };
    assert_eq!(pixel(150, 150), [255, 0, 0, 255], "cell fill");
    assert_eq!(pixel(350, 350), [255, 255, 255, 255], "unfilled cell");
    let border = pixel(300, 350);
    assert!(
        border[0] < 80 && border[1] < 80 && border[2] < 80,
        "inner border pixel was {border:?}"
    );
}

#[test]
fn table_merged_cell_fill_covers_its_span_offscreen() {
    struct NoMedia;
    impl crate::media::MediaSource for NoMedia {
        fn get(&self, _src: &str) -> crate::media::MediaFetch {
            crate::media::MediaFetch::Missing
        }
    }
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "span-course",
        "title": "Span",
        "slide_size": { "w": 1280, "h": 720 },
        "scenes": [{
            "id": "scene1",
            "title": "Scene 1",
            "slides": [{
                "id": "slide1",
                "title": "Slide 1",
                "objects": [{
                    "id": "table1",
                    "kind": {
                        "type": "table",
                        "columns": [0.33333334, 0.33333334, 0.33333334],
                        "rows": [
                            {
                                "height": 0.5,
                                "cells": [
                                    { "text": "Wide", "col_span": 2, "fill": "#00FF00" },
                                    { "text": "" },
                                    { "text": "Right" }
                                ]
                            },
                            {
                                "height": 0.5,
                                "cells": [
                                    { "text": "A" },
                                    { "text": "B" },
                                    { "text": "C" }
                                ]
                            }
                        ]
                    },
                    "frame": { "x": 100, "y": 100, "w": 600, "h": 200 }
                }]
            }]
        }]
    });
    let course = Course::from_json(&json.to_string()).expect("valid span course");
    let image = crate::render_slide_rgba(
        &course,
        "slide1",
        &crate::RenderOptions::default(),
        &NoMedia,
    )
    .expect("render span table");
    let pixel = |x: u32, y: u32| -> [u8; 4] {
        let index = ((y * image.width + x) * 4) as usize;
        [
            image.pixels[index],
            image.pixels[index + 1],
            image.pixels[index + 2],
            image.pixels[index + 3],
        ]
    };
    assert_eq!(pixel(150, 150), [0, 255, 0, 255], "first span column");
    assert_eq!(pixel(350, 150), [0, 255, 0, 255], "second span column");
    assert_eq!(pixel(550, 150), [255, 255, 255, 255], "cell after the span");
}

#[test]
fn table_header_row_bolds_text_and_tints_the_cell() {
    let course = table_course(true);
    let slide = course.play_slide("slide1").expect("slide");
    let (object, _) = slide.object("table1").expect("table");
    let item = crate::compose_object_with_theme(object, "normal", Some(&course.theme))
        .expect("composed table");
    let Visual::Table(table) = item.visual else {
        panic!("expected a table");
    };
    let header = &table.rows[0].cells[0];
    assert!(
        header.text.runs.iter().all(|run| run.bold),
        "header cell text is bold"
    );
    assert_eq!(header.fill, Some(Color32::from_rgb(0xFF, 0, 0)));
    let plain = &table.rows[0].cells[1];
    assert!(plain.text.runs.iter().all(|run| run.bold));
    assert!(
        plain.fill.is_some_and(|fill| fill != Color32::WHITE),
        "a header cell without a fill gets the accent wash"
    );
    let body = &table.rows[1].cells[0];
    assert!(body.text.runs.iter().all(|run| !run.bold));
    assert_eq!(body.fill, None);
}

fn input_render_course(kind: serde_json::Value, variable: serde_json::Value) -> Course {
    Course::from_json(
        &serde_json::json!({
            "schema_version": 1,
            "id": "input-render",
            "title": "Input Render",
            "theme": { "accent": "#2F6FEB", "text": "#1F1F1F" },
            "variables": [variable],
            "scenes": [{ "id": "sc", "title": "Scene", "slides": [{
                "id": "s1",
                "title": "Slide",
                "objects": [{
                    "id": "input",
                    "kind": kind,
                    "frame": { "x": 100.0, "y": 100.0, "w": 200.0, "h": 100.0 }
                }]
            }] }]
        })
        .to_string(),
    )
    .expect("valid input course")
}

#[test]
fn panel_children_move_with_offset_and_clip_hit_testing() {
    let course = Course::from_json(
        &serde_json::json!({
            "schema_version": 1,
            "id": "panel-hit",
            "title": "Panel Hit",
            "scenes": [{ "id": "sc", "title": "Scene", "slides": [{
                "id": "s1",
                "title": "One",
                "objects": [
                    { "id": "panel", "name": "Panel",
                      "kind": { "type": "scroll_panel", "content_height": 600.0 },
                      "frame": { "x": 100.0, "y": 100.0, "w": 300.0, "h": 200.0 } },
                    { "id": "a", "name": "A", "panel": "panel",
                      "kind": { "type": "button", "label": "A", "fill": "#2F6FEB" },
                      "frame": { "x": 120.0, "y": 120.0, "w": 100.0, "h": 50.0 } },
                    { "id": "b", "name": "B", "panel": "panel",
                      "kind": { "type": "button", "label": "B", "fill": "#2F6FEB" },
                      "frame": { "x": 120.0, "y": 500.0, "w": 100.0, "h": 50.0 } }
                ]
            }] }]
        })
        .to_string(),
    )
    .expect("panel course");
    let mut player = Player::new(&course).expect("playable");
    player.handle(&Input::Start);

    let items = compose(&player);
    let panel = items
        .iter()
        .find(|item| item.hit == Hit::Object("panel".into()))
        .expect("panel item");
    assert!(matches!(
        panel.visual,
        Visual::ScrollPanel {
            content_height,
            thumb: Some(_),
            ..
        } if (content_height - 600.0).abs() < 1e-6
    ));
    let a = items
        .iter()
        .find(|item| item.hit == Hit::Object("a".into()))
        .expect("a item");
    assert!((a.rect.top() - 120.0).abs() < 1e-6);
    assert_eq!(a.clip_rect, Some(panel.rect));

    assert_eq!(
        crate::view::hit_test(&items, pos2(140.0, 140.0)),
        Some(Hit::Object("a".into()))
    );
    // A child wholly outside the viewport cannot be clicked even though its
    // authored frame is at the pointer.
    assert_eq!(crate::view::hit_test(&items, pos2(140.0, 520.0)), None);

    player.handle(&Input::PanelScrolled {
        object: "panel".into(),
        offset: 400.0,
    });
    let items = compose(&player);
    let a = items
        .iter()
        .find(|item| item.hit == Hit::Object("a".into()))
        .expect("a item");
    let b = items
        .iter()
        .find(|item| item.hit == Hit::Object("b".into()))
        .expect("b item");
    assert!((a.rect.top() + 280.0).abs() < 1e-6);
    assert!((b.rect.top() - 100.0).abs() < 1e-6);
    // The scrolled-in child is clickable; the scrolled-out one is not.
    assert_eq!(
        crate::view::hit_test(&items, pos2(140.0, 140.0)),
        Some(Hit::Object("b".into()))
    );
}

#[test]
fn input_visuals_read_the_variable_and_a_set_variable_moves_a_slider() {
    let course = input_render_course(
        serde_json::json!({ "type": "slider", "variable": "n", "min": 0, "max": 100, "step": 10 }),
        serde_json::json!({ "name": "n", "value": 20 }),
    );
    let mut player = Player::new(&course).unwrap();
    player.handle(&Input::Start);
    let items = compose(&player);
    let item = items
        .iter()
        .find(|item| item.hit == Hit::Object("input".into()))
        .expect("slider item");
    assert!(matches!(
        item.visual,
        Visual::Slider { value, .. } if (value - 20.0).abs() < 1e-9
    ));

    // A set_variable input moves the displayed value.
    player.handle(&Input::SetVariable {
        variable: "n".into(),
        value: coursecraft_course::Value::Number(70.0),
    });
    let items = compose(&player);
    let item = items
        .iter()
        .find(|item| item.hit == Hit::Object("input".into()))
        .expect("slider item");
    assert!(matches!(
        item.visual,
        Visual::Slider { value, .. } if (value - 70.0).abs() < 1e-9
    ));
}

#[test]
fn input_visuals_show_text_checkbox_and_radio_state() {
    let mut course = input_render_course(
        serde_json::json!({ "type": "radio_group", "variable": "choice", "options": [
            { "label": "A", "value": "a" },
            { "label": "B", "value": "b" }
        ] }),
        serde_json::json!({ "name": "choice", "value": "b" }),
    );
    let player = Player::new(&course).unwrap();
    let items = compose(&player);
    let item = items
        .iter()
        .find(|item| item.hit == Hit::Object("input".into()))
        .expect("radio item");
    assert!(matches!(
        &item.visual,
        Visual::RadioGroup { options }
            if options.get(1).is_some_and(|(_, value, selected)| value == "b" && *selected)
    ));

    course.slide_mut("s1").unwrap().objects[0].kind = coursecraft_course::ObjectKind::Checkbox {
        variable: "choice".into(),
        label: "On".into(),
    };
    // Use a real bool variable for the checkbox composition.
    course.variables[0].value = coursecraft_course::Value::Bool(false);
    let player = Player::new(&course).unwrap();
    let items = compose(&player);
    let item = items
        .iter()
        .find(|item| item.hit == Hit::Object("input".into()))
        .expect("checkbox item");
    assert!(matches!(
        item.visual,
        Visual::Checkbox { checked: false, .. }
    ));
}

#[test]
fn slider_fill_length_matches_the_value() {
    let rect = Rect::from_min_size(pos2(100.0, 100.0), vec2(200.0, 20.0));
    let track = crate::paint::slider_track_rect(rect, coursecraft_course::Orientation::Horizontal);
    let fill =
        crate::paint::slider_fill_rect(rect, coursecraft_course::Orientation::Horizontal, 0.25);
    assert!((fill.width() - track.width() * 0.25).abs() < 1e-4);
    assert!((fill.left() - track.left()).abs() < 1e-4);
    assert!((crate::paint::value_fraction(25.0, 0.0, 100.0) - 0.25).abs() < 1e-6);
}

#[test]
fn paint_slider_checkbox_and_radio_use_the_accent() {
    let accent = Color32::from_rgb(0x2F, 0x6F, 0xEB);
    let make = |visual: Visual, rect: Rect| DrawItem {
        hit: Hit::Object("input".into()),
        rect,
        visual,
        dimmed: false,
        opacity: 1.0,
        clip: None,
        rotation: 0.0,
        flip_h: false,
        flip_v: false,
        shadow: None,
        glow: None,
        media_time_ms: None,
        links: Vec::new(),
        show_table_overflow: false,
        clip_rect: None,
        mark: None,
    };
    let slider = make(
        Visual::Slider {
            value: 50.0,
            min: 0.0,
            max: 100.0,
            step: 1.0,
            orientation: coursecraft_course::Orientation::Horizontal,
        },
        Rect::from_min_size(pos2(100.0, 100.0), vec2(200.0, 20.0)),
    );
    let checkbox = make(
        Visual::Checkbox {
            checked: true,
            label: "On".into(),
        },
        Rect::from_min_size(pos2(100.0, 200.0), vec2(200.0, 40.0)),
    );
    let radio = make(
        Visual::RadioGroup {
            options: vec![
                ("A".into(), "a".into(), true),
                ("B".into(), "b".into(), false),
            ],
        },
        Rect::from_min_size(pos2(100.0, 300.0), vec2(200.0, 60.0)),
    );
    let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 720.0));
    let t = SlideTransform::fit(screen, Size { w: 1280, h: 720 }).expect("fit");
    let theme = coursecraft_course::Theme {
        accent: coursecraft_course::Color("#2F6FEB".into()),
        ..Default::default()
    };
    let items = vec![slider, checkbox, radio];
    let ctx = egui::Context::default();
    let output = ctx.run_ui(egui::RawInput::default(), |ui| {
        let painter = ui.painter();
        paint(
            painter,
            &t,
            &items,
            Highlight::default(),
            None,
            Some(&theme),
            None,
        );
        let mut accent_rects = 0;
        let mut accent_circles = 0;
        painter.for_each_shape(|clipped| match &clipped.shape {
            egui::Shape::Rect(r) if r.fill == accent => accent_rects += 1,
            egui::Shape::Circle(c) if c.fill == accent => accent_circles += 1,
            _ => {}
        });
        assert!(accent_rects >= 2, "slider fill and checked box are accent");
        assert!(accent_circles >= 2, "thumb and selected radio are accent");
    });
    output.drop_without_applying_deltas();
}

#[test]
fn pointer_drag_on_slider_and_dial_emits_snapped_values() {
    let mut course = input_render_course(
        serde_json::json!({ "type": "slider", "variable": "n", "min": 0, "max": 100, "step": 10 }),
        serde_json::json!({ "name": "n", "value": 0 }),
    );
    let mut player = Player::new(&course).unwrap();
    player.handle(&Input::Start);
    let items = compose(&player);
    let item = items
        .iter()
        .find(|item| item.hit == Hit::Object("input".into()))
        .expect("slider item");
    let rect = item.rect;
    let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 720.0));
    let t = SlideTransform::fit(screen, player.course().slide_size).expect("fit");
    let mut state = PointerState::default();
    let events = vec![
        egui::Event::PointerButton {
            pos: t.to_screen_pos(pos2(rect.left() + rect.width() * 0.75, rect.center().y)),
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
        egui::Event::PointerButton {
            pos: t.to_screen_pos(pos2(rect.left() + rect.width() * 0.75, rect.center().y)),
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        },
    ];
    let inputs = state.handle_events(&events, &t, &items, &|_| false);
    assert_eq!(
        inputs,
        vec![Input::SetInputValue {
            object: "input".into(),
            value: coursecraft_course::Value::Number(80.0),
        }]
    );

    course.slide_mut("s1").unwrap().objects[0].kind = coursecraft_course::ObjectKind::Dial {
        variable: "n".into(),
        min: 0.0,
        max: 100.0,
        step: 10.0,
        start_angle: -135.0,
        end_angle: 135.0,
    };
    let player = Player::new(&course).unwrap();
    let items = compose(&player);
    let rect = items
        .iter()
        .find(|item| item.hit == Hit::Object("input".into()))
        .expect("dial item")
        .rect;
    let mut state = PointerState::default();
    let press = pos2(rect.center().x, rect.top());
    let events = vec![
        egui::Event::PointerButton {
            pos: t.to_screen_pos(press),
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
        egui::Event::PointerButton {
            pos: t.to_screen_pos(press),
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        },
    ];
    let inputs = state.handle_events(&events, &t, &items, &|_| false);
    assert_eq!(
        inputs,
        vec![Input::SetInputValue {
            object: "input".into(),
            value: coursecraft_course::Value::Number(50.0),
        }]
    );
}

// ---------------------------------------------------------------- lightbox

fn lightbox_render_course() -> Course {
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "lightbox-render",
        "title": "Lightbox Render",
        "scenes": [{
            "id": "sc",
            "title": "Scene",
            "slides": [
                {
                    "id": "base",
                    "title": "Base",
                    "background": "#FFFFFF",
                    "objects": [
                        { "id": "open", "kind": { "type": "button", "label": "Open" }, "frame": { "x": 20, "y": 20, "w": 120, "h": 40 } },
                        { "id": "base-only", "kind": { "type": "button", "label": "Base only" }, "frame": { "x": 20, "y": 80, "w": 120, "h": 40 } }
                    ],
                    "triggers": [
                        { "action": { "action": "lightbox_slide", "slide": "box" }, "when": { "event": "click", "object": "open" } }
                    ]
                },
                {
                    "id": "box",
                    "title": "Box Title",
                    "background": "#FFFFFF",
                    "objects": [
                        { "id": "box-btn", "kind": { "type": "button", "label": "Box" }, "frame": { "x": 500, "y": 300, "w": 200, "h": 80 } }
                    ]
                }
            ]
        }]
    });
    Course::from_json(&json.to_string()).expect("valid lightbox render course")
}

#[test]
fn lightbox_maps_clicks_through_the_scaled_box_and_ignores_the_base() {
    let course = lightbox_render_course();
    let mut player = Player::new(&course).expect("player");
    player.handle(&Input::Start);
    player.handle(&Input::Click {
        object: "open".into(),
    });
    assert!(player.lightbox_open());
    let items = compose(&player);
    let target = items
        .iter()
        .find(|item| item.hit == Hit::Object("box-btn".to_owned()))
        .expect("box button");
    let base = compose_authoring(&course, course.slide("base").expect("base slide"), None)
        .into_iter()
        .find(|item| item.hit == Hit::Object("base-only".to_owned()))
        .expect("base button");

    let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 720.0));
    let t = SlideTransform::fit(screen, course.slide_size).expect("fit");
    let box_rect =
        crate::view::lightbox_box_rect(t.slide_rect(), course.slide_size).expect("box rect");
    let box_t = SlideTransform::fit(box_rect, course.slide_size).expect("box fit");
    let inside = box_t.to_screen_pos(target.rect.center());
    let base_point = t.to_screen_pos(base.rect.center());

    let mut view = SlideView::new();
    let press = |pos| {
        vec![
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            },
        ]
    };
    let mut inputs = Vec::new();
    let ctx = egui::Context::default();
    let output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(screen),
            events: press(inside),
            ..Default::default()
        },
        |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                inputs = view.show(ui, &player, None);
            });
        },
    );
    output.drop_without_applying_deltas();
    assert!(
        inputs.contains(&Input::Click {
            object: "box-btn".into()
        }),
        "expected a click inside the scaled box, got {inputs:?}"
    );

    let mut inputs = Vec::new();
    let output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(screen),
            events: press(base_point),
            ..Default::default()
        },
        |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                inputs = view.show(ui, &player, None);
            });
        },
    );
    output.drop_without_applying_deltas();
    assert!(
        !inputs
            .iter()
            .any(|input| matches!(input, Input::Click { .. } | Input::LinkClicked { .. })),
        "the base slide must take no clicks while a lightbox is open, got {inputs:?}"
    );
}

#[test]
fn lightbox_draws_a_dark_backdrop_over_the_base() {
    use crate::media::{MediaCache, MediaFetch, MediaSource};
    use crate::offscreen::render_ui;
    use crate::player_frame::PlayerFrame;

    struct NoMedia;
    impl MediaSource for NoMedia {
        fn get(&self, _src: &str) -> MediaFetch {
            MediaFetch::Missing
        }
    }

    let course = lightbox_render_course();
    let mut player = Player::new(&course).expect("player");
    player.handle(&Input::Start);
    let mut frame = PlayerFrame::new();
    frame.reduced_motion = true;
    let mut view = SlideView::new();
    view.set_reduced_motion(true);
    let mut cache = MediaCache::new();
    let media = NoMedia;
    let size = (1280u32, 720u32);

    // Paint the base once so the lightbox has a frozen frame underneath.
    let first = render_ui(
        size.0,
        size.1,
        1.0,
        |_ctx| {},
        |ctx| {
            ctx.root_ui(|ui| {
                frame.show(ui, &player, &mut view, Some((&mut cache, &media)));
            });
        },
    )
    .expect("base render");

    player.handle(&Input::Click {
        object: "open".into(),
    });
    let second = render_ui(
        size.0,
        size.1,
        1.0,
        |_ctx| {},
        |ctx| {
            ctx.root_ui(|ui| {
                frame.show(ui, &player, &mut view, Some((&mut cache, &media)));
            });
        },
    )
    .expect("lightbox render");

    let pixel = |img: &crate::offscreen::RgbaImage, x: u32, y: u32| -> (u8, u8, u8) {
        let index = ((y as usize) * (img.width as usize) + x as usize) * 4;
        (
            img.pixels.get(index).copied().unwrap_or(0),
            img.pixels.get(index + 1).copied().unwrap_or(0),
            img.pixels.get(index + 2).copied().unwrap_or(0),
        )
    };
    let base_corner = pixel(&first, 10, 10);
    let dimmed_corner = pixel(&second, 10, 10);
    let centre = pixel(&second, 200, 100);
    assert!(
        base_corner.0 > 200,
        "base slide corner starts light: {base_corner:?}"
    );
    assert!(
        dimmed_corner.0 < base_corner.0.saturating_sub(80),
        "backdrop dims the base: was {base_corner:?}, now {dimmed_corner:?}"
    );
    assert!(centre.0 > 200, "the boxed slide stays bright: {centre:?}");
}

fn draggable_view() -> (&'static Course, Player<'static>) {
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "draggable-view",
        "title": "Draggable View",
        "scenes": [{
            "id": "sc",
            "title": "Scene",
            "slides": [{
                "id": "s1",
                "title": "Slide",
                "objects": [
                    { "id": "card", "kind": { "type": "shape", "shape": "rectangle", "fill": "#112233" },
                      "frame": { "x": 100.0, "y": 100.0, "w": 80.0, "h": 60.0 },
                      "drag": { "targets": ["bin"] } },
                    { "id": "bin", "kind": { "type": "shape", "shape": "rectangle", "fill": "#334455" },
                      "frame": { "x": 400.0, "y": 300.0, "w": 100.0, "h": 100.0 } }
                ]
            }]
        }]
    });
    let course = Course::from_json(&json.to_string()).expect("valid course");
    let course: &'static Course = Box::leak(Box::new(course));
    let mut player = Player::new(course).expect("player");
    player.handle(&Input::Start);
    (course, player)
}

fn draggable_pointer_state() -> PointerState {
    PointerState {
        draggable_specs: vec![("card".into(), vec!["bin".into()])],
        slide_bounds: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1280.0, 720.0),
        )),
        ..PointerState::default()
    }
}

#[test]
fn a_draggable_object_needs_more_than_3_px_before_it_drags() {
    let (course, player) = draggable_view();
    let items = compose(&player);
    let t = SlideTransform::fit(
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 720.0)),
        course.slide_size,
    )
    .expect("transform");
    let card = items
        .iter()
        .find(|item| item.hit == Hit::Object("card".into()))
        .expect("card")
        .rect;
    let start = t.to_screen_rect(card).center();

    let press = egui::Event::PointerButton {
        pos: start,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    };
    let release = |pos| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::default(),
    };

    // A press with no movement stays an ordinary click.
    let mut state = draggable_pointer_state();
    let inputs = state.handle_events(&[press.clone(), release(start)], &t, &items, &|_| false);
    assert_eq!(
        inputs,
        vec![Input::Click {
            object: "card".into()
        }]
    );

    // Two pixels is still a click.
    let mut state = draggable_pointer_state();
    let two = start + egui::vec2(2.0, 0.0);
    let inputs = state.handle_events(
        &[press.clone(), egui::Event::PointerMoved(two), release(two)],
        &t,
        &items,
        &|_| false,
    );
    assert_eq!(
        inputs,
        vec![Input::Click {
            object: "card".into()
        }]
    );

    // Four pixels starts a drag and reports the moved top-left.
    let mut state = draggable_pointer_state();
    let four = start + egui::vec2(4.0, 0.0);
    let inputs = state.handle_events(
        &[
            press.clone(),
            egui::Event::PointerMoved(four),
            release(four),
        ],
        &t,
        &items,
        &|_| false,
    );
    assert_eq!(
        inputs,
        vec![Input::ObjectDropped {
            object: "card".into(),
            x: 104.0,
            y: 100.0,
        }]
    );
}

#[test]
fn a_draggable_object_is_clamped_inside_the_slide() {
    let (course, player) = draggable_view();
    let items = compose(&player);
    let t = SlideTransform::fit(
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 720.0)),
        course.slide_size,
    )
    .expect("transform");
    let card = items
        .iter()
        .find(|item| item.hit == Hit::Object("card".into()))
        .expect("card")
        .rect;
    let start = t.to_screen_rect(card).center();
    let far = egui::pos2(5000.0, 5000.0);
    let mut state = draggable_pointer_state();
    let inputs = state.handle_events(
        &[
            egui::Event::PointerButton {
                pos: start,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::default(),
            },
            egui::Event::PointerMoved(far),
            egui::Event::PointerButton {
                pos: far,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::default(),
            },
        ],
        &t,
        &items,
        &|_| false,
    );
    assert_eq!(
        inputs,
        vec![Input::ObjectDropped {
            object: "card".into(),
            x: 1200.0,
            y: 660.0,
        }]
    );
}

fn timer_meter_render_course(kind: serde_json::Value, variable: serde_json::Value) -> Course {
    Course::from_json(
        &serde_json::json!({
            "schema_version": 1,
            "id": "timer-meter-render",
            "title": "Timer Meter Render",
            "theme": { "accent": "#2F6FEB", "text": "#1F1F1F" },
            "variables": [variable],
            "scenes": [{ "id": "sc", "title": "Scene", "slides": [{
                "id": "s1",
                "title": "Slide",
                "duration_ms": 60000,
                "objects": [{
                    "id": "obj",
                    "kind": kind,
                    "frame": { "x": 100.0, "y": 100.0, "w": 200.0, "h": 100.0 }
                }]
            }] }]
        })
        .to_string(),
    )
    .expect("valid timer/meter course")
}

#[test]
fn timer_composes_its_formatted_remaining_time() {
    let course = timer_meter_render_course(
        serde_json::json!({ "type": "timer", "duration_ms": 90000 }),
        serde_json::json!({ "name": "unused", "value": 0 }),
    );
    let mut player = Player::new(&course).expect("playable");
    player.handle(&Input::Start);
    player.handle(&Input::Tick { ms: 30000 });
    let items = compose(&player);
    let item = items
        .iter()
        .find(|item| item.hit == Hit::Object("obj".into()))
        .expect("timer item");
    let Visual::Timer { runs, .. } = &item.visual else {
        panic!("timer visual");
    };
    let text: String = runs.iter().map(|run| run.text.as_str()).collect();
    assert_eq!(text, "01:00", "90s minus 30s of ticks");
    // The canvas shows the full duration and does not run.
    let slide = course.slide("s1").expect("slide");
    let authored = compose_authoring(&course, slide, None);
    let item = authored
        .iter()
        .find(|item| item.hit == Hit::Object("obj".into()))
        .expect("authored timer item");
    let Visual::Timer { runs, .. } = &item.visual else {
        panic!("timer visual");
    };
    let text: String = runs.iter().map(|run| run.text.as_str()).collect();
    assert_eq!(text, "01:30");
}

#[test]
fn hidden_timer_draws_nothing_in_the_player_but_a_placeholder_on_the_canvas() {
    let course = timer_meter_render_course(
        serde_json::json!({ "type": "timer", "duration_ms": 90000, "format": "hidden" }),
        serde_json::json!({ "name": "unused", "value": 0 }),
    );
    let mut player = Player::new(&course).expect("playable");
    player.handle(&Input::Start);
    player.handle(&Input::Tick { ms: 1000 });
    let items = compose(&player);
    let item = items
        .iter()
        .find(|item| item.hit == Hit::Object("obj".into()))
        .expect("timer item");
    let Visual::Timer { runs, .. } = &item.visual else {
        panic!("timer visual");
    };
    assert!(runs.iter().all(|run| run.text.is_empty()));

    // Painted in the player it produces no glyph shapes; on the canvas it
    // paints a placeholder rect and a label.
    let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 720.0));
    let t = SlideTransform::fit(screen, course.slide_size).expect("fit");
    let theme = course.theme.clone();
    let ctx = egui::Context::default();
    let output = ctx.run_ui(egui::RawInput::default(), |ui| {
        let painter = ui.painter();
        paint(
            painter,
            &t,
            &items,
            Highlight::default(),
            None,
            Some(&theme),
            None,
        );
        let mut galleys = 0;
        painter.for_each_shape(|clipped| {
            if matches!(clipped.shape, egui::Shape::Text(_)) {
                galleys += 1;
            }
        });
        assert_eq!(galleys, 0, "a hidden timer draws nothing in the player");
    });
    output.drop_without_applying_deltas();
}

#[test]
fn progress_meter_bar_fill_matches_the_variable_value() {
    let course = timer_meter_render_course(
        serde_json::json!({ "type": "progress_meter", "variable": "n", "min": 0, "max": 100, "show_value": true }),
        serde_json::json!({ "name": "n", "value": 25 }),
    );
    let player = Player::new(&course).expect("playable");
    let items = compose(&player);
    let item = items
        .iter()
        .find(|item| item.hit == Hit::Object("obj".into()))
        .expect("meter item");
    let Visual::ProgressMeter {
        fraction,
        percent,
        show_value,
        ..
    } = item.visual
    else {
        panic!("meter visual");
    };
    assert!((fraction - 0.25).abs() < 1e-6);
    assert_eq!(percent, 25);
    assert!(show_value);
    // The bar fill is a quarter of the track at a known value.
    let rect = Rect::from_min_size(pos2(100.0, 100.0), vec2(200.0, 20.0));
    let fill = crate::paint::progress_bar_fill_rect(rect, fraction);
    assert!((fill.width() - rect.width() * 0.25).abs() < 1e-4);
    assert!(
        (crate::paint::progress_ring_point(pos2(0.0, 0.0), 10.0, 0.25) - pos2(10.0, 0.0)).length()
            < 1e-4
    );
}

#[test]
fn progress_meter_clamps_out_of_range_values() {
    let course = timer_meter_render_course(
        serde_json::json!({ "type": "progress_meter", "variable": "n", "min": 10, "max": 20 }),
        serde_json::json!({ "name": "n", "value": 99 }),
    );
    let player = Player::new(&course).expect("playable");
    let items = compose(&player);
    let item = items
        .iter()
        .find(|item| item.hit == Hit::Object("obj".into()))
        .expect("meter item");
    let Visual::ProgressMeter { fraction, .. } = item.visual else {
        panic!("meter visual");
    };
    assert!((fraction - 1.0).abs() < 1e-6);
}

#[test]
fn bar_and_ring_meters_paint_accent_over_the_track_token() {
    let accent = Color32::from_rgb(0x2F, 0x6F, 0xEB);
    let make = |kind: serde_json::Value| {
        let course =
            timer_meter_render_course(kind, serde_json::json!({ "name": "n", "value": 50 }));
        let player = Player::new(&course).expect("playable");
        compose(&player)
    };
    let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 720.0));
    for items in [
        make(
            serde_json::json!({ "type": "progress_meter", "variable": "n", "min": 0, "max": 100, "style": "bar" }),
        ),
        make(
            serde_json::json!({ "type": "progress_meter", "variable": "n", "min": 0, "max": 100, "style": "ring" }),
        ),
    ] {
        let t = SlideTransform::fit(screen, Size { w: 1280, h: 720 }).expect("fit");
        let theme = coursecraft_course::Theme {
            accent: coursecraft_course::Color("#2F6FEB".into()),
            ..Default::default()
        };
        let ctx = egui::Context::default();
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let painter = ui.painter();
            paint(
                painter,
                &t,
                &items,
                Highlight::default(),
                None,
                Some(&theme),
                None,
            );
            let mut accent_shapes = 0;
            painter.for_each_shape(|clipped| match &clipped.shape {
                egui::Shape::Rect(r) if r.fill == accent => accent_shapes += 1,
                egui::Shape::Path(path)
                    if path.stroke.color == egui::epaint::ColorMode::Solid(accent) =>
                {
                    accent_shapes += 1;
                }
                _ => {}
            });
            assert!(accent_shapes > 0, "the meter fill uses the theme accent");
        });
        output.drop_without_applying_deltas();
    }
}

#[test]
fn desktop_cue_simulation_uses_the_same_media_time_input_as_the_web() {
    use coursecraft_logic::{Effect, MediaOp};
    use coursecraft_playback::NullBackend;

    use crate::media_driver::MediaDriver;

    const CUE_JSON: &str = r#"{
      "schema_version": 1,
      "id": "cue-preview",
      "title": "Cue preview",
      "slide_size": { "w": 960, "h": 540 },
      "scenes": [{
        "id": "sc",
        "title": "Scene",
        "slides": [{
          "id": "s1",
          "title": "One",
          "duration_ms": 5000,
          "objects": [{
            "id": "vid",
            "kind": {
              "type": "video",
              "src": "clip.mp4",
              "autoplay": true,
              "cues": [
                { "id": "c1", "at_ms": 1000, "name": "One" },
                { "id": "c2", "at_ms": 2000, "name": "Two" }
              ]
            },
            "frame": { "x": 0, "y": 0, "w": 320, "h": 180 }
          }]
        }]
      }]
    }"#;
    let course = Course::from_json(CUE_JSON).expect("valid course");
    let backend = NullBackend::new();
    let mut driver = MediaDriver::new().with_backend(Box::new(backend.clone()));

    let mut preview = Player::new(&course).expect("preview");
    let mut web = Player::new(&course).expect("web");
    let start = preview.handle(&Input::Start);
    driver.apply(&preview, &start, None);
    web.handle(&Input::Start);
    assert!(
        !start.iter().any(|effect| matches!(
            effect,
            Effect::Media {
                op: MediaOp::Pause,
                ..
            }
        )),
        "desktop simulation must not fake a pause at start"
    );

    let mut seen = Vec::new();
    for _ in 0..6 {
        let inputs = driver.media_time_inputs(&preview, 500);
        assert!(!inputs.is_empty(), "the simulated clock keeps reporting");
        for input in inputs {
            let at_ms = match &input {
                Input::MediaTime { object, at_ms } if object == "vid" => *at_ms,
                other => panic!("unexpected input {other:?}"),
            };
            let web_effects = web.handle(&Input::MediaTime {
                object: "vid".into(),
                at_ms,
            });
            let preview_effects = preview.handle(&input);
            assert_eq!(web_effects, preview_effects);
            for effect in &preview_effects {
                if let Effect::CueReached { cue, .. } = effect {
                    seen.push(cue.clone());
                }
            }
        }
    }
    assert_eq!(seen, vec!["c1".to_owned(), "c2".to_owned()]);
    assert!(driver.poll(None).is_empty(), "simulated video never ends");
}

#[test]
fn video_frame_draws_as_texture_instead_of_poster() {
    use coursecraft_playback::VideoFrame;
    use std::sync::Arc;

    use crate::compose::{DrawItem, Hit, Visual, validate_frame};
    use crate::layout::SlideTransform;
    use crate::media::{MediaCache, MediaFetch, MediaSource};
    use crate::paint::{Highlight, paint};

    struct VideoSource {
        frame: Option<Arc<VideoFrame>>,
    }
    impl MediaSource for VideoSource {
        fn get(&self, _src: &str) -> MediaFetch {
            MediaFetch::Missing
        }
        fn video_frame(&self, object: &str) -> Option<Arc<VideoFrame>> {
            if object == "vid1" {
                self.frame.as_ref().map(Arc::clone)
            } else {
                None
            }
        }
    }

    fn red_frame() -> Arc<VideoFrame> {
        let mut rgba = Vec::with_capacity(16 * 16 * 4);
        for _ in 0..16 * 16 {
            rgba.extend_from_slice(&[255, 0, 0, 255]);
        }
        Arc::new(VideoFrame {
            pts_ms: 0,
            width: 16,
            height: 16,
            rgba,
        })
    }

    fn video_item() -> DrawItem {
        DrawItem {
            hit: Hit::Object("vid1".to_owned()),
            rect: egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(320.0, 180.0)),
            visual: Visual::Video {
                src: "clip.mp4".to_owned(),
                poster: None,
            },
            dimmed: false,
            opacity: 1.0,
            clip: None,
            rotation: 0.0,
            flip_h: false,
            flip_v: false,
            shadow: None,
            glow: None,
            media_time_ms: None,
            links: Vec::new(),
            show_table_overflow: false,
            clip_rect: None,
            mark: None,
        }
    }

    let frame_rect = validate_frame(&coursecraft_course::Rect {
        x: 0.0,
        y: 0.0,
        w: 320.0,
        h: 180.0,
    });
    assert!(frame_rect.is_some(), "frame validates");

    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640.0, 360.0));
    let transform =
        SlideTransform::fit(screen, coursecraft_course::Size { w: 640, h: 360 }).expect("fit");

    // With a frame: an image shape is drawn.
    let ctx = egui::Context::default();
    let mut cache = MediaCache::new();
    let source = VideoSource {
        frame: Some(red_frame()),
    };
    let item = video_item();
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        let painter = ui.painter();
        paint(
            painter,
            &transform,
            std::slice::from_ref(&item),
            Highlight::default(),
            Some((&mut cache, &source as &dyn MediaSource)),
            None,
            None,
        );
        let mut images = 0;
        painter.for_each_shape(|clipped| {
            if let egui::Shape::Mesh(mesh) = &clipped.shape
                && mesh.texture_id != egui::TextureId::default()
            {
                images += 1;
            }
        });
        assert_eq!(images, 1, "video frame draws as a texture");
    });
    output.textures_delta.clear();
    output.drop_without_applying_deltas();

    // Missing frame: the poster is drawn instead (no image shape).
    let ctx2 = egui::Context::default();
    let mut cache2 = MediaCache::new();
    let empty = VideoSource { frame: None };
    let mut output2 = ctx2.run_ui(egui::RawInput::default(), |ui| {
        let painter = ui.painter();
        paint(
            painter,
            &transform,
            std::slice::from_ref(&item),
            Highlight::default(),
            Some((&mut cache2, &empty as &dyn MediaSource)),
            None,
            None,
        );
        let mut images = 0;
        painter.for_each_shape(|clipped| {
            if let egui::Shape::Mesh(mesh) = &clipped.shape
                && mesh.texture_id != egui::TextureId::default()
            {
                images += 1;
            }
        });
        assert_eq!(images, 0, "missing frame falls back to the poster");
    });
    output2.textures_delta.clear();
    output2.drop_without_applying_deltas();

    // Texture is updated in place for a new frame (same texture id).
    let ctx3 = egui::Context::default();
    let mut cache3 = MediaCache::new();
    let first = red_frame();
    let second = {
        let mut rgba = Vec::with_capacity(16 * 16 * 4);
        for _ in 0..16 * 16 {
            rgba.extend_from_slice(&[0, 255, 0, 255]);
        }
        Arc::new(VideoFrame {
            pts_ms: 33,
            width: 16,
            height: 16,
            rgba,
        })
    };
    let mut output3 = ctx3.run_ui(egui::RawInput::default(), |ui| {
        let view1 = cache3
            .video_texture(ui.ctx(), "vid1", &first)
            .expect("first texture");
        let id1 = match view1 {
            crate::media::MediaView::Ready { texture, .. } => texture.id(),
            _ => panic!("expected Ready"),
        };
        let view2 = cache3
            .video_texture(ui.ctx(), "vid1", &second)
            .expect("second texture");
        let id2 = match view2 {
            crate::media::MediaView::Ready { texture, .. } => texture.id(),
            _ => panic!("expected Ready"),
        };
        assert_eq!(id1, id2, "texture updated in place, not reallocated");
    });
    output3.textures_delta.clear();
    output3.drop_without_applying_deltas();
}

#[test]
fn desktop_fallback_when_decoding_fails_still_drives_cues() {
    use coursecraft_logic::{Effect, MediaOp};
    use coursecraft_playback::NullBackend;

    use crate::media_driver::MediaDriver;

    const CUE_JSON: &str = r#"{
      "schema_version": 1,
      "id": "cue-fallback",
      "title": "Cue fallback",
      "slide_size": { "w": 960, "h": 540 },
      "scenes": [{
        "id": "sc",
        "title": "Scene",
        "slides": [{
          "id": "s1",
          "title": "One",
          "duration_ms": 5000,
          "objects": [{
            "id": "vid",
            "kind": {
              "type": "video",
              "src": "missing.mp4",
              "autoplay": true,
              "cues": [
                { "id": "c1", "at_ms": 1000, "name": "One" }
              ]
            },
            "frame": { "x": 0, "y": 0, "w": 320, "h": 180 }
          }]
        }]
      }]
    }"#;
    let course = Course::from_json(CUE_JSON).expect("valid course");
    let mut player = Player::new(&course).expect("player");
    let backend = NullBackend::new();
    let mut driver = MediaDriver::new().with_backend(Box::new(backend.clone()));

    let start = player.handle(&Input::Start);
    assert!(
        !start.iter().any(|effect| matches!(
            effect,
            Effect::Media {
                op: MediaOp::Pause,
                ..
            }
        )),
        "fallback must not fake a pause at start"
    );
    let followups = driver.apply(&player, &start, None);
    assert!(
        followups.is_empty(),
        "fallback still drives cues instead of ending: {followups:?}"
    );
    let mut seen = Vec::new();
    for _ in 0..4 {
        let inputs = driver.media_time_inputs(&player, 500);
        assert!(!inputs.is_empty(), "the fallback clock keeps reporting");
        for input in inputs {
            let at_ms = match &input {
                Input::MediaTime { object, at_ms } if object == "vid" => *at_ms,
                other => panic!("unexpected input {other:?}"),
            };
            let _ = at_ms;
            let effects = player.handle(&input);
            for effect in &effects {
                if let Effect::CueReached { cue, .. } = effect {
                    seen.push(cue.clone());
                }
            }
        }
        // Even if the backend reports the file undecodable, the fallback
        // suppresses the ended while simulating.
        backend.mark_ended("vid");
        assert!(
            driver.poll(None).is_empty(),
            "fallback video never ends via decode failure"
        );
    }
    assert_eq!(seen, vec!["c1".to_owned()]);
}

#[test]
fn compose_authoring_draw_slide_shows_the_draw_card_text() {
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "draw-card",
        "title": "Draw Card",
        "slide_size": { "w": 1280, "h": 720 },
        "scenes": [{
            "id": "sc",
            "title": "Scene",
            "slides": [{ "id": "s-draw", "title": "Draw", "draw": { "bank": "bank1", "count": 1 } }]
        }],
        "banks": [{
            "id": "bank1",
            "title": "Bank One",
            "slides": [
                {
                    "id": "bq1",
                    "title": "Q1",
                    "question": {
                        "kind": "multiple_choice",
                        "prompt": "One?",
                        "choices": [
                            { "id": "a", "text": "A" },
                            { "id": "b", "text": "B", "correct": true }
                        ],
                        "attempts": 1
                    },
                    "triggers": [{ "action": { "action": "submit_question" }, "when": { "event": "submit" } }]
                },
                {
                    "id": "bq2",
                    "title": "Q2",
                    "question": {
                        "kind": "multiple_choice",
                        "prompt": "Two?",
                        "choices": [
                            { "id": "a", "text": "A" },
                            { "id": "b", "text": "B", "correct": true }
                        ],
                        "attempts": 1
                    },
                    "triggers": [{ "action": { "action": "submit_question" }, "when": { "event": "submit" } }]
                }
            ]
        }]
    });
    let course = Course::from_json(&json.to_string()).expect("valid course");
    let slide = course.slide("s-draw").expect("draw slide");
    let items = compose_authoring(&course, slide, None);
    let card = items
        .iter()
        .find(|item| matches!(item.visual, Visual::DrawCard { .. }))
        .expect("draw card item");
    assert_eq!(
        card.visual,
        Visual::DrawCard {
            text: "Draws 1 of 2 questions from Bank One".to_owned()
        }
    );
    // Centred on the slide.
    assert_eq!(card.rect.center().x, 640.0);
    assert_eq!(card.rect.center().y, 360.0);
    assert_eq!(card.hit, Hit::None);
}

fn effect_run(text: &str) -> crate::compose::ResolvedTextRun {
    crate::compose::ResolvedTextRun {
        text: text.into(),
        size: 20.0,
        color: Color32::BLACK,
        bold: false,
        italic: false,
        underline: false,
        link: None,
        font: None,
        strikethrough: false,
        baseline: coursecraft_course::Baseline::Normal,
        letter_spacing: 0.0,
    }
}

fn lay_out_one(
    ctx: &egui::Context,
    run: crate::compose::ResolvedTextRun,
) -> crate::text_layout::LaidParagraph {
    let laid = crate::text_layout::layout_text(
        ctx,
        Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 200.0)),
        1.0,
        &[run],
        &[],
        coursecraft_course::Align::Left,
        coursecraft_course::VAlign::Top,
        coursecraft_course::Autofit::None,
    );
    laid.paragraphs.into_iter().next().expect("paragraph")
}

#[test]
fn text_effects_reach_the_layout() {
    let ctx = text_layout_context();
    let plain = lay_out_one(&ctx, effect_run("Hello"));

    let mut struck = effect_run("Hello");
    struck.strikethrough = true;
    let struck = lay_out_one(&ctx, struck);
    assert!(
        struck
            .galley
            .job
            .sections
            .iter()
            .any(|s| s.format.strikethrough != egui::Stroke::NONE),
        "strikethrough is part of the layout job"
    );
    assert!(
        plain
            .galley
            .job
            .sections
            .iter()
            .all(|s| s.format.strikethrough == egui::Stroke::NONE)
    );

    let mut spaced = effect_run("Hello");
    spaced.letter_spacing = 5.0;
    let spaced = lay_out_one(&ctx, spaced);
    assert!(spaced.galley.size().x > plain.galley.size().x + 15.0);

    let mut sup = effect_run("Hello");
    sup.baseline = coursecraft_course::Baseline::Superscript;
    let sup = lay_out_one(&ctx, sup);
    assert!(
        sup.galley.size().x < plain.galley.size().x * 0.8,
        "a superscript draws smaller"
    );
    let font = &sup.galley.job.sections[0].format.font_id;
    assert!((font.size - 20.0 * crate::text_layout::SCRIPT_SCALE).abs() < 0.01);
    assert_eq!(sup.galley.job.sections[0].format.valign, egui::Align::TOP);
}
