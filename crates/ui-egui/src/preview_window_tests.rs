//! Headless tests for the desktop preview window: backdrop dimming,
//! edge/corner resizing with min/max clamps, session size persistence,
//! full screen toggle + Esc order, and slide scaling + variables panel fit.

use crate::preview::{
    PREVIEW_BACKDROP_COLOR, PREVIEW_MARGIN, PREVIEW_MIN_HEIGHT, PREVIEW_MIN_WIDTH,
};
use crate::test_harness::TestHarness;
use coursecraft_course::Course;
use egui::{Color32, Key, Modifiers, Rect, pos2, vec2};

fn test_course() -> Course {
    let json = r#"{
        "schema_version": 1,
        "id": "preview-window-test",
        "title": "Preview Window Test",
        "slide_size": { "w": 1280, "h": 720 },
        "variables": [
            { "name": "score", "value": 42 },
            { "name": "answered", "value": false }
        ],
        "scenes": [
            {
                "id": "sc-1",
                "title": "Main",
                "slides": [
                    {
                        "id": "s-1",
                        "title": "Slide 1",
                        "duration_ms": 5000,
                        "objects": [
                            {
                                "id": "txt-1",
                                "kind": { "type": "text", "text": "Score: %score%" },
                                "frame": { "x": 80, "y": 80, "w": 400, "h": 60 }
                            }
                        ],
                        "triggers": []
                    }
                ]
            }
        ]
    }"#;
    Course::from_json(json).expect("valid course")
}

fn open_preview(harness: &mut TestHarness) -> Rect {
    harness.editor.open_preview();
    for _ in 0..3 {
        harness.idle_frame();
    }
    harness
        .editor
        .preview
        .as_ref()
        .and_then(|p| p.window_rect)
        .expect("preview window_rect")
}

fn current_rect(harness: &TestHarness) -> Rect {
    harness
        .editor
        .preview
        .as_ref()
        .and_then(|p| p.window_rect)
        .expect("preview window_rect")
}

fn find_rect_with_fill(shape: &egui::epaint::Shape, fill: Color32) -> Option<Rect> {
    match shape {
        egui::epaint::Shape::Rect(r)
            if r.fill == fill && r.rect.width() >= 1279.0 && r.rect.height() >= 799.0 =>
        {
            Some(r.rect)
        }
        egui::epaint::Shape::Vec(v) => v.iter().find_map(|s| find_rect_with_fill(s, fill)),
        _ => None,
    }
}

#[test]
fn preview_backdrop_dims_to_80_percent_black_in_dark_and_light_themes() {
    for pref in [
        coursecraft_render::ThemePref::Dark,
        coursecraft_render::ThemePref::Light,
    ] {
        let mut harness = TestHarness::new_with_size(test_course(), (1280.0, 800.0));
        harness.editor.set_theme_pref(pref);
        open_preview(&mut harness);
        let shapes = harness.frame_shapes(Vec::new());
        let expected_alpha = PREVIEW_BACKDROP_COLOR.a();
        assert!(
            (200..=210).contains(&expected_alpha),
            "backdrop alpha should be ~80% (204), got {expected_alpha}"
        );
        let backdrop_rect = shapes.iter().find_map(|clipped| {
            find_rect_with_fill(&clipped.shape, Color32::from_black_alpha(expected_alpha))
        });
        assert!(
            backdrop_rect.is_some(),
            "theme {pref:?}: expected full-screen ~80% black backdrop rect"
        );
    }
}

#[test]
fn preview_window_resizes_from_all_edges_and_corners_with_min_max_clamps() {
    let mut harness = TestHarness::new_with_size(test_course(), (1440.0, 900.0));
    let initial = open_preview(&mut harness);

    // Drag each of the 4 edges outward by 40px and verify only that edge moves.
    harness.drag_by(
        pos2(initial.min.x, initial.center().y),
        vec2(-40.0, 0.0),
        4,
        Modifiers::NONE,
    );
    let after_left = current_rect(&harness);
    assert!(
        (after_left.min.x - (initial.min.x - 40.0)).abs() < 2.0,
        "left edge: {after_left:?}"
    );
    assert!((after_left.max.x - initial.max.x).abs() < 0.5);

    harness.drag_by(
        pos2(after_left.max.x, after_left.center().y),
        vec2(40.0, 0.0),
        4,
        Modifiers::NONE,
    );
    let after_right = current_rect(&harness);
    assert!(
        (after_right.max.x - (after_left.max.x + 40.0)).abs() < 2.0,
        "right edge: {after_right:?}"
    );
    assert!((after_right.min.x - after_left.min.x).abs() < 0.5);

    harness.drag_by(
        pos2(after_right.center().x, after_right.min.y),
        vec2(0.0, -30.0),
        4,
        Modifiers::NONE,
    );
    let after_top = current_rect(&harness);
    assert!(
        (after_top.min.y - (after_right.min.y - 30.0)).abs() < 2.0,
        "top edge: {after_top:?}"
    );
    assert!((after_top.max.y - after_right.max.y).abs() < 0.5);

    harness.drag_by(
        pos2(after_top.center().x, after_top.max.y),
        vec2(0.0, 30.0),
        4,
        Modifiers::NONE,
    );
    let after_bottom = current_rect(&harness);
    assert!(
        (after_bottom.max.y - (after_top.max.y + 30.0)).abs() < 2.0,
        "bottom edge: {after_bottom:?}"
    );
    assert!((after_bottom.min.y - after_top.min.y).abs() < 0.5);

    // Drag each of the 4 corners inward by 20px and verify both adjacent edges move.
    harness.drag_by(
        pos2(after_bottom.min.x, after_bottom.min.y),
        vec2(20.0, 20.0),
        4,
        Modifiers::NONE,
    );
    let after_tl = current_rect(&harness);
    assert!((after_tl.min.x - (after_bottom.min.x + 20.0)).abs() < 2.0);
    assert!((after_tl.min.y - (after_bottom.min.y + 20.0)).abs() < 2.0);

    harness.drag_by(
        pos2(after_tl.max.x, after_tl.min.y),
        vec2(-20.0, 20.0),
        4,
        Modifiers::NONE,
    );
    let after_tr = current_rect(&harness);
    assert!((after_tr.max.x - (after_tl.max.x - 20.0)).abs() < 2.0);
    assert!((after_tr.min.y - (after_tl.min.y + 20.0)).abs() < 2.0);

    harness.drag_by(
        pos2(after_tr.min.x, after_tr.max.y),
        vec2(20.0, -20.0),
        4,
        Modifiers::NONE,
    );
    let after_bl = current_rect(&harness);
    assert!((after_bl.min.x - (after_tr.min.x + 20.0)).abs() < 2.0);
    assert!((after_bl.max.y - (after_tr.max.y - 20.0)).abs() < 2.0);

    harness.drag_by(
        pos2(after_bl.max.x, after_bl.max.y),
        vec2(-20.0, -20.0),
        4,
        Modifiers::NONE,
    );
    let after_br = current_rect(&harness);
    assert!((after_br.max.x - (after_bl.max.x - 20.0)).abs() < 2.0);
    assert!((after_br.max.y - (after_bl.max.y - 20.0)).abs() < 2.0);

    // Drag far outward to test max bounds clamp (screen shrunk by PREVIEW_MARGIN).
    harness.drag_by(
        pos2(after_br.min.x, after_br.min.y),
        vec2(-500.0, -500.0),
        5,
        Modifiers::NONE,
    );
    let r1 = current_rect(&harness);
    harness.drag_by(
        pos2(r1.max.x, r1.max.y),
        vec2(500.0, 500.0),
        5,
        Modifiers::NONE,
    );
    let maxed = current_rect(&harness);
    assert!(
        (maxed.min.x - PREVIEW_MARGIN).abs() < 1.0,
        "maxed={maxed:?}"
    );
    assert!(
        (maxed.min.y - PREVIEW_MARGIN).abs() < 1.0,
        "maxed={maxed:?}"
    );
    assert!(
        (maxed.max.x - (1440.0 - PREVIEW_MARGIN)).abs() < 1.0,
        "maxed={maxed:?}"
    );
    assert!(
        (maxed.max.y - (900.0 - PREVIEW_MARGIN)).abs() < 1.0,
        "maxed={maxed:?}"
    );

    // Drag far inward to test min size clamp.
    harness.drag_by(
        pos2(maxed.max.x, maxed.max.y),
        vec2(-1200.0, -800.0),
        6,
        Modifiers::NONE,
    );
    let minned = current_rect(&harness);
    assert!(
        (minned.width() - PREVIEW_MIN_WIDTH).abs() < 1.0,
        "minned={minned:?}"
    );
    assert!(
        (minned.height() - PREVIEW_MIN_HEIGHT).abs() < 1.0,
        "minned={minned:?}"
    );

    // Resize to a custom size, close preview, reopen, and verify size is remembered.
    harness.drag_by(
        pos2(minned.max.x, minned.max.y),
        vec2(220.0, 140.0),
        4,
        Modifiers::NONE,
    );
    let custom = current_rect(&harness);
    harness.press(Key::Escape, Modifiers::NONE);
    harness.idle_frame();
    assert!(harness.editor.preview.is_none());

    let reopened = open_preview(&mut harness);
    assert!(
        (reopened.width() - custom.width()).abs() < 1.0
            && (reopened.height() - custom.height()).abs() < 1.0,
        "remembered size mismatch: custom={:?} reopened={:?}",
        custom.size(),
        reopened.size()
    );
}

#[test]
fn preview_fullscreen_toggle_and_esc_order() {
    let mut harness = TestHarness::new_with_size(test_course(), (1280.0, 800.0));
    let initial = open_preview(&mut harness);

    // Toggle full screen via F11 while preview has focus.
    harness.press(Key::F11, Modifiers::NONE);
    harness.idle_frame();
    assert!(
        harness
            .editor
            .preview
            .as_ref()
            .is_some_and(|p| p.fullscreen)
    );
    let fs_rect = current_rect(&harness);
    assert!((fs_rect.width() - 1280.0).abs() < 1.0 && (fs_rect.height() - 800.0).abs() < 1.0);

    // First Esc leaves full screen, does not close preview.
    harness.press(Key::Escape, Modifiers::NONE);
    harness.idle_frame();
    assert!(harness.editor.preview.is_some());
    assert!(
        !harness
            .editor
            .preview
            .as_ref()
            .is_some_and(|p| p.fullscreen)
    );
    let restored = current_rect(&harness);
    assert!((restored.width() - initial.width()).abs() < 1.0);

    // Toggle full screen via Cmd+Ctrl+F.
    harness.press(Key::F, Modifiers::MAC_CMD | Modifiers::CTRL);
    harness.idle_frame();
    assert!(
        harness
            .editor
            .preview
            .as_ref()
            .is_some_and(|p| p.fullscreen)
    );

    // Clicking near the edge while full screen does not close preview.
    harness.click_at(pos2(5.0, 5.0));
    harness.idle_frame();
    assert!(harness.editor.preview.is_some());

    // First Esc exits full screen; second Esc closes preview.
    harness.press(Key::Escape, Modifiers::NONE);
    harness.idle_frame();
    assert!(harness.editor.preview.is_some());
    assert!(
        !harness
            .editor
            .preview
            .as_ref()
            .is_some_and(|p| p.fullscreen)
    );

    harness.press(Key::Escape, Modifiers::NONE);
    harness.idle_frame();
    assert!(harness.editor.preview.is_none());

    // Reopen and toggle full screen via the title-bar button.
    open_preview(&mut harness);
    let fs_btn = harness
        .find_icon(coursecraft_render::Icon::CornersOut)
        .expect("Full screen button in preview title bar");
    harness.click_at(fs_btn);
    harness.idle_frame();
    assert!(
        harness
            .editor
            .preview
            .as_ref()
            .is_some_and(|p| p.fullscreen)
    );
    let exit_btn = harness
        .find_icon(coursecraft_render::Icon::CornersIn)
        .expect("Exit full screen button in preview title bar");
    harness.click_at(exit_btn);
    harness.idle_frame();
    assert!(
        !harness
            .editor
            .preview
            .as_ref()
            .is_some_and(|p| p.fullscreen)
    );
}

#[test]
fn preview_slide_scales_and_variables_panel_fits_at_three_sizes() {
    let mut harness = TestHarness::new_with_size(test_course(), (1440.0, 900.0));
    open_preview(&mut harness);
    harness.press(Key::F3, Modifiers::NONE);
    for _ in 0..3 {
        harness.idle_frame();
    }

    // 1. Default size (~960x640)
    let default_rect = current_rect(&harness);
    let default_slide = harness
        .editor
        .preview
        .as_ref()
        .expect("preview")
        .frame
        .slide_rect()
        .expect("default slide_rect");

    // 2. Minimum size (480x320)
    harness.drag_by(
        pos2(default_rect.max.x, default_rect.max.y),
        vec2(-600.0, -400.0),
        6,
        Modifiers::NONE,
    );
    harness.idle_frame();
    let min_rect = current_rect(&harness);
    let min_slide = harness
        .editor
        .preview
        .as_ref()
        .expect("preview")
        .frame
        .slide_rect()
        .expect("min slide_rect");
    let score_pos_min = harness.find_text("score").expect("score in min preview");
    assert!(
        min_rect.contains(score_pos_min),
        "variables panel must stay inside min preview"
    );

    // 3. Full screen (1440x900)
    harness.press(Key::F11, Modifiers::NONE);
    for _ in 0..2 {
        harness.idle_frame();
    }
    let fs_rect = current_rect(&harness);
    let fs_slide = harness
        .editor
        .preview
        .as_ref()
        .expect("preview")
        .frame
        .slide_rect()
        .expect("fs slide_rect");
    let score_pos_fs = harness.find_text("score").expect("score in fs preview");
    assert!(
        fs_rect.contains(score_pos_fs),
        "variables panel must stay inside fs preview"
    );

    // Slide scales monotonically and preserves 16:9 aspect ratio at all 3 sizes.
    assert!(min_slide.width() < default_slide.width());
    assert!(default_slide.width() < fs_slide.width());
    for s in [min_slide, default_slide, fs_slide] {
        let aspect = s.width() / s.height();
        assert!(
            (aspect - 16.0 / 9.0).abs() < 0.02,
            "expected 16:9 aspect, got {aspect}"
        );
    }
}

#[test]
fn preview_player_strip_fullscreen_button_toggles_preview_fullscreen() {
    let mut harness = TestHarness::new_with_size(test_course(), (1280.0, 800.0));
    open_preview(&mut harness);
    let fs_rect = harness
        .editor
        .preview
        .as_ref()
        .expect("preview")
        .frame
        .chrome_rect(&coursecraft_render::ChromeButton::Fullscreen)
        .expect("player strip fullscreen button should be enabled in preview");

    harness.click_at(fs_rect.center());
    harness.idle_frame();
    assert!(
        harness
            .editor
            .preview
            .as_ref()
            .is_some_and(|p| p.fullscreen),
        "clicking player strip fullscreen button must enter fullscreen"
    );

    let fs_rect2 = harness
        .editor
        .preview
        .as_ref()
        .expect("preview")
        .frame
        .chrome_rect(&coursecraft_render::ChromeButton::Fullscreen)
        .expect("player strip fullscreen button");
    harness.click_at(fs_rect2.center());
    harness.idle_frame();
    assert!(
        harness
            .editor
            .preview
            .as_ref()
            .is_some_and(|p| !p.fullscreen),
        "clicking player strip fullscreen button again must exit fullscreen"
    );
}

#[test]
fn preview_escape_hierarchy_closes_speed_popup_drawer_lightbox_before_preview() {
    let mut course = test_course();
    // Add a second slide and a button on slide 1 that opens slide 2 as a lightbox.
    let mut slide2 = course.scenes[0].slides[0].clone();
    slide2.id = "s2".into();
    slide2.title = "Lightbox Slide".into();
    course.scenes[0].slides.push(slide2);
    course.scenes[0].slides[0]
        .triggers
        .push(coursecraft_course::Trigger {
            when: coursecraft_course::Event::Click {
                object: "txt-1".into(),
            },
            action: coursecraft_course::Action::LightboxSlide { slide: "s2".into() },
            condition: None,
            disabled: false,
        });

    let mut harness = TestHarness::new_with_size(course, (1280.0, 800.0));
    harness.editor.open_preview_at(None);
    for _ in 0..3 {
        harness.idle_frame();
    }

    // 1. Speed popup open -> Esc closes speed popup, preview stays open.
    let speed_rect = harness
        .editor
        .preview
        .as_ref()
        .expect("preview")
        .frame
        .chrome_rect(&coursecraft_render::ChromeButton::Speed)
        .expect("speed button");
    harness.click_at(speed_rect.center());
    harness.idle_frame();
    assert!(harness.find_text("2×").is_some(), "speed popup should open");
    harness.press(Key::Escape, Modifiers::NONE);
    harness.idle_frame();
    assert!(
        harness.editor.preview.is_some(),
        "Esc must close speed popup without closing preview"
    );
    assert!(
        harness.find_text("2×").is_none(),
        "speed popup should close"
    );

    // 2. Drawer open -> Esc closes drawer, preview stays open.
    let outline_rect = harness
        .editor
        .preview
        .as_ref()
        .expect("preview")
        .frame
        .chrome_rect(&coursecraft_render::ChromeButton::Outline)
        .expect("outline button");
    harness.click_at(outline_rect.center());
    harness.idle_frame();
    assert!(
        harness
            .editor
            .preview
            .as_ref()
            .is_some_and(|p| p.frame.drawer_open())
    );
    harness.press(Key::Escape, Modifiers::NONE);
    harness.idle_frame();
    assert!(
        harness.editor.preview.is_some(),
        "Esc must close drawer without closing preview"
    );
    assert!(
        harness
            .editor
            .preview
            .as_ref()
            .is_some_and(|p| !p.frame.drawer_open())
    );
    // Let the drawer close animation finish so the scrim no longer covers the slide.
    for _ in 0..20 {
        harness.idle_frame();
    }

    // 3. Lightbox open -> Esc closes lightbox, preview stays open.
    let s_rect = harness
        .editor
        .preview
        .as_ref()
        .and_then(|p| p.frame.slide_rect())
        .expect("slide_rect");
    let txt_pos = s_rect.min
        + egui::vec2(
            s_rect.width() * (280.0 / 1280.0),
            s_rect.height() * (110.0 / 720.0),
        );
    harness.click_at(txt_pos);
    harness.idle_frame();
    assert!(
        harness.editor.preview.as_ref().is_some_and(|p| p
            .frame
            .chrome_rect(&coursecraft_render::ChromeButton::CloseLightbox)
            .is_some()),
        "lightbox should be open"
    );
    harness.press(Key::Escape, Modifiers::NONE);
    harness.idle_frame();
    assert!(
        harness.editor.preview.is_some(),
        "Esc must close lightbox without closing preview"
    );
    assert!(
        harness.editor.preview.as_ref().is_some_and(|p| p
            .frame
            .chrome_rect(&coursecraft_render::ChromeButton::CloseLightbox)
            .is_none()),
        "lightbox should close on Esc"
    );

    // 4. Clean slide -> Esc closes preview.
    harness.press(Key::Escape, Modifiers::NONE);
    harness.idle_frame();
    assert!(
        harness.editor.preview.is_none(),
        "Esc on clean slide closes preview"
    );
}

fn shape_has_text(shape: &egui::epaint::Shape, needle: &str) -> bool {
    match shape {
        egui::epaint::Shape::Text(t) => t.galley.text() == needle,
        egui::epaint::Shape::Vec(v) => v.iter().any(|s| shape_has_text(s, needle)),
        _ => false,
    }
}

fn preview_match_combo_centers(harness: &TestHarness) -> Vec<egui::Pos2> {
    let preview = harness.editor.preview.as_ref().expect("preview open");
    let t = preview.view.last_transform().expect("preview transform");
    let scale = t.scale();
    preview
        .view
        .last_items()
        .iter()
        .filter_map(|item| {
            let coursecraft_render::Visual::MatchRow { .. } = &item.visual else {
                return None;
            };
            let row = t.to_screen_rect(item.rect);
            let combo_w = (row.width() * 0.46).max(1.0);
            let combo_h = (row.height() - 12.0 * scale).max(1.0);
            let combo_rect = Rect::from_min_size(
                pos2(row.max.x - combo_w - 8.0 * scale, row.min.y + 6.0 * scale),
                vec2(combo_w, combo_h),
            );
            Some(combo_rect.center())
        })
        .collect()
}

fn pick_matching_option_in_preview(
    harness: &mut TestHarness,
    combo_pos: egui::Pos2,
    prompt: &str,
    option_text: &str,
) {
    harness.click_at(combo_pos);
    harness.idle_frame();
    let open_shapes = harness.frame_shapes(vec![]);
    assert!(
        egui::Popup::is_any_open(&harness.ctx),
        "clicking matching dropdown at {combo_pos:?} must open popup"
    );
    let opt_idx = open_shapes
        .iter()
        .rposition(|c| shape_has_text(&c.shape, option_text));
    let prompt_idx = open_shapes
        .iter()
        .position(|c| shape_has_text(&c.shape, prompt));
    assert!(
        opt_idx.is_some() && opt_idx > prompt_idx,
        "matching dropdown popup ({option_text:?}, shape {opt_idx:?}) must paint above preview slide ({prompt:?}, shape {prompt_idx:?})"
    );
    let opt_pos = harness
        .find_text_last(option_text)
        .expect("matching option text in open popup");
    let top_layer = harness.ctx.layer_id_at(opt_pos);
    assert_ne!(
        top_layer,
        Some(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::unique("preview_window")
        )),
        "matching popup at {opt_pos:?} must be above preview_window in hit-test order"
    );
    harness.click_at(opt_pos);
    harness.idle_frame();
}

#[test]
fn matching_dropdown_opens_above_preview_handles_escape_and_reopens() {
    let json = include_str!("../../../examples/feature-tour/course.json");
    let course = Course::from_json(json).expect("valid feature-tour course");
    let mut harness = TestHarness::new_with_size(course, (1280.0, 800.0));
    harness.editor.open_preview_at(Some("q-match"));
    for _ in 0..5 {
        harness.idle_frame();
    }

    let prompt = "Match each feature to what it does.";
    let chooses = harness.find_texts(coursecraft_render::compose::MATCH_PLACEHOLDER);
    assert_eq!(
        chooses.len(),
        3,
        "expected 3 matching Choose… controls in preview"
    );

    // 1. Open row 2's dropdown; ArrowDown must not silently mutate row 1.
    harness.click_at(chooses[1]);
    harness.idle_frame();
    assert!(egui::Popup::is_any_open(&harness.ctx));
    harness.press(Key::ArrowDown, Modifiers::NONE);
    harness.idle_frame();
    let p1_after_arrow = harness
        .editor
        .preview
        .as_ref()
        .and_then(|p| p.state.as_ref())
        .and_then(|s| s.matches.get("p1").cloned());
    assert_eq!(
        p1_after_arrow, None,
        "ArrowDown while row 2's dropdown is open must not mutate row 1"
    );

    // 2. Pressing Escape while a matching dropdown is open closes ONLY the dropdown, not Preview.
    harness.press(Key::Escape, Modifiers::NONE);
    harness.idle_frame();
    assert!(
        harness.editor.preview.is_some(),
        "Escape while matching dropdown is open must not close preview"
    );
    assert!(
        !egui::Popup::is_any_open(&harness.ctx),
        "Escape must close the matching dropdown"
    );

    // 3. Clicking dimmed backdrop while a dropdown is open closes ONLY the dropdown, not Preview.
    harness.click_at(chooses[0]);
    harness.idle_frame();
    assert!(egui::Popup::is_any_open(&harness.ctx));
    harness.click_at(pos2(5.0, 5.0));
    harness.idle_frame();
    assert!(
        harness.editor.preview.is_some(),
        "clicking backdrop while matching dropdown is open must not close preview"
    );
    assert!(
        !egui::Popup::is_any_open(&harness.ctx),
        "clicking backdrop must close the matching dropdown"
    );

    // 4. Reopen row 1 (testing second open after close) and pick all 3 pairs.
    pick_matching_option_in_preview(
        &mut harness,
        chooses[0],
        prompt,
        "Runs an action when something happens",
    );
    pick_matching_option_in_preview(
        &mut harness,
        chooses[1],
        prompt,
        "Changes how an object looks",
    );
    pick_matching_option_in_preview(
        &mut harness,
        chooses[2],
        prompt,
        "Opens a pop-up on a picture",
    );
    let matches = harness
        .editor
        .preview
        .as_ref()
        .and_then(|p| p.state.as_ref())
        .map(|s| s.matches.clone())
        .unwrap_or_default();
    assert_eq!(matches.get("p1").map(String::as_str), Some("p1"));
    assert_eq!(matches.get("p2").map(String::as_str), Some("p2"));
    assert_eq!(matches.get("p3").map(String::as_str), Some("p3"));

    // 5. Fullscreen Esc hierarchy: dropdown -> fullscreen -> preview.
    harness.press(Key::F11, Modifiers::NONE);
    harness.idle_frame();
    assert!(
        harness
            .editor
            .preview
            .as_ref()
            .is_some_and(|p| p.fullscreen)
    );
    let row1_fs = harness
        .find_text("Runs an action when something happens")
        .expect("selected p1 combo in fullscreen");
    harness.click_at(row1_fs);
    harness.idle_frame();
    assert!(egui::Popup::is_any_open(&harness.ctx));
    harness.press(Key::Escape, Modifiers::NONE);
    harness.idle_frame();
    assert!(
        harness
            .editor
            .preview
            .as_ref()
            .is_some_and(|p| p.fullscreen),
        "first Esc closes matching dropdown, keeping fullscreen"
    );
    assert!(!egui::Popup::is_any_open(&harness.ctx));
    harness.press(Key::Escape, Modifiers::NONE);
    harness.idle_frame();
    assert!(
        harness
            .editor
            .preview
            .as_ref()
            .is_some_and(|p| !p.fullscreen),
        "second Esc leaves fullscreen"
    );
    harness.press(Key::Escape, Modifiers::NONE);
    harness.idle_frame();
    assert!(harness.editor.preview.is_none(), "third Esc closes preview");
}

#[test]
fn matching_dropdown_works_across_all_examples_and_freshly_inserted_question() {
    let cases = [
        (
            "showcase",
            include_str!("../../../examples/showcase/course.json"),
            "s-q-mat",
            "p-scorm",
            "SCORM 1.2 data model and manifest",
        ),
        (
            "healthy-habits",
            include_str!("../../../examples/healthy-habits/course.json"),
            "q6",
            "p1",
            "Start of the day",
        ),
        (
            "reference-course",
            include_str!("../../../examples/reference-course/course.json"),
            "s12-q-match",
            "m1",
            "R1",
        ),
    ];

    for (name, json, slide_id, pair_id, right_text) in cases {
        let course = Course::from_json(json).expect("valid example course");
        let prompt = course
            .slide(slide_id)
            .and_then(|s| s.question.as_ref())
            .map(|q| q.prompt.clone())
            .expect("matching slide prompt");
        let mut harness = TestHarness::new_with_size(course, (1280.0, 800.0));
        harness.editor.open_preview_at(Some(slide_id));
        for _ in 0..5 {
            harness.idle_frame();
        }
        let chooses = preview_match_combo_centers(&harness);
        assert!(
            !chooses.is_empty(),
            "{name}: expected matching Choose… controls in preview"
        );
        pick_matching_option_in_preview(&mut harness, chooses[0], &prompt, right_text);
        let picked = harness
            .editor
            .preview
            .as_ref()
            .and_then(|p| p.state.as_ref())
            .and_then(|s| s.matches.get(pair_id).cloned());
        assert_eq!(
            picked.as_deref(),
            Some(pair_id),
            "{name}: clicking option in preview dropdown must select {pair_id}"
        );
    }

    // Freshly inserted matching question via the editor toolbar command.
    let mut harness = TestHarness::new_with_size(test_course(), (1280.0, 800.0));
    harness.editor.selection.slide = "s-1".to_owned();
    harness.idle_frame();
    crate::toolbar::insert_question(
        &mut harness.editor,
        coursecraft_course::QuestionKind::Matching,
    );
    harness.idle_frame();
    let new_slide_id = harness.editor.selection.slide.clone();
    let (prompt, first_pair_id, first_right) = {
        let q = harness
            .editor
            .session
            .course
            .slide(&new_slide_id)
            .and_then(|s| s.question.as_ref())
            .expect("inserted matching question");
        (
            q.prompt.clone(),
            q.pairs[0].id.clone(),
            q.pairs[0].right.clone(),
        )
    };
    harness.editor.open_preview_at(Some(&new_slide_id));
    for _ in 0..5 {
        harness.idle_frame();
    }
    let chooses = preview_match_combo_centers(&harness);
    assert!(
        !chooses.is_empty(),
        "freshly inserted matching question must show Choose… controls in preview"
    );
    pick_matching_option_in_preview(&mut harness, chooses[0], &prompt, &first_right);
    let picked = harness
        .editor
        .preview
        .as_ref()
        .and_then(|p| p.state.as_ref())
        .and_then(|s| s.matches.get(&first_pair_id).cloned());
    assert_eq!(
        picked.as_deref(),
        Some(first_pair_id.as_str()),
        "freshly inserted matching question dropdown must select the clicked option"
    );
}

fn advance_and_settle_preview(h: &mut TestHarness) {
    for _ in 0..40 {
        h.advance_time(0.1);
        h.idle_frame();
    }
}

fn preview_slide_pt(h: &TestHarness, sx: f32, sy: f32) -> egui::Pos2 {
    h.editor
        .preview
        .as_ref()
        .and_then(|p| p.view.last_transform())
        .expect("preview transform")
        .to_screen_pos(pos2(sx, sy))
}

fn preview_current_slide(h: &TestHarness) -> String {
    let p = h.editor.preview.as_ref().expect("preview");
    let idx = p.state.as_ref().expect("state").slide;
    p.course
        .slides()
        .nth(idx)
        .map(|s| s.id.clone())
        .unwrap_or_default()
}

fn preview_has_layer(h: &TestHarness, layer: &str) -> bool {
    h.editor
        .preview
        .as_ref()
        .and_then(|p| p.state.as_ref())
        .is_some_and(|s| s.visible_layers.iter().any(|l| l == layer))
}

fn walk_feature_tour_stops(h: &mut TestHarness) {
    use coursecraft_course::Value;

    // Slide 1: tour (Welcome)
    advance_and_settle_preview(h);
    assert_eq!(preview_current_slide(h), "tour");
    h.click_at(preview_slide_pt(h, 188.0, 584.0));
    advance_and_settle_preview(h);
    assert_eq!(preview_current_slide(h), "media");

    // Player chrome drawer (Menu & Transcript) on media slide.
    let outline_rect = h
        .editor
        .preview
        .as_ref()
        .unwrap()
        .frame
        .chrome_rect(&coursecraft_render::ChromeButton::Outline)
        .expect("outline button");
    h.click_at(outline_rect.center());
    for _ in 0..10 {
        h.advance_time(0.05);
        h.idle_frame();
    }
    assert!(h.editor.preview.as_ref().unwrap().frame.drawer_open());
    assert!(h.find_text("Transcript").is_some());
    h.press(Key::Escape, Modifiers::NONE);
    for _ in 0..20 {
        h.advance_time(0.05);
        h.idle_frame();
    }
    assert!(!h.editor.preview.as_ref().unwrap().frame.drawer_open());

    // Slide 2: media (Video and sound)
    h.click_at(preview_slide_pt(h, 850.0, 474.0));
    h.idle_frame();
    assert!(h.editor.preview.as_ref().is_some_and(|p| {
        p.log
            .iter()
            .any(|l| l.contains("md-video") && l.contains("play"))
    }));
    h.click_at(preview_slide_pt(h, 1040.0, 474.0));
    h.idle_frame();
    assert!(h.editor.preview.as_ref().is_some_and(|p| {
        p.log
            .iter()
            .any(|l| l.contains("md-chime") && l.contains("play"))
    }));
    h.click_at(preview_slide_pt(h, 1102.0, 624.0));
    advance_and_settle_preview(h);
    assert_eq!(preview_current_slide(h), "interact");

    // Slide 3: interact (Layers, states, variables)
    for (cx, expected_layer, expected_count) in [
        (632.0, "about-layers", 1.0),
        (856.0, "about-states", 2.0),
        (1080.0, "about-variables", 3.0),
    ] {
        h.click_at(preview_slide_pt(h, cx, 262.0));
        h.advance_time(0.5);
        h.idle_frame();
        h.idle_frame();
        assert!(preview_has_layer(h, expected_layer));
        let explored = h
            .editor
            .preview
            .as_ref()
            .unwrap()
            .state
            .as_ref()
            .unwrap()
            .variables
            .get("explored")
            .cloned();
        assert_eq!(explored, Some(Value::Number(expected_count)));
    }
    let counter_state = h
        .editor
        .preview
        .as_ref()
        .unwrap()
        .state
        .as_ref()
        .unwrap()
        .object_states
        .get("interact")
        .and_then(|m| m.get("ia-counter"))
        .cloned();
    assert_eq!(counter_state.as_deref(), Some("done"));
    h.click_at(preview_slide_pt(h, 1102.0, 624.0));
    advance_and_settle_preview(h);
    assert_eq!(preview_current_slide(h), "motion");

    // Slide 4: motion (Entrances, paths, transitions)
    h.click_at(preview_slide_pt(h, 970.0, 472.0));
    h.idle_frame();
    assert!(
        h.editor
            .preview
            .as_ref()
            .unwrap()
            .state
            .as_ref()
            .unwrap()
            .running_paths
            .contains_key("mo-star")
    );
    h.click_at(preview_slide_pt(h, 1124.0, 472.0));
    advance_and_settle_preview(h);
    assert_eq!(preview_current_slide(h), "motion");
    h.click_at(preview_slide_pt(h, 1102.0, 624.0));
    advance_and_settle_preview(h);
    assert_eq!(preview_current_slide(h), "campus");

    // Slide 5: campus (Markers on a picture)
    for (id, mx, my, title) in [
        ("cp-m1", 176.0, 235.0, "Library"),
        ("cp-m2", 625.0, 247.0, "Cafe"),
        ("cp-m3", 183.0, 437.0, "Science lab"),
        ("cp-m4", 578.0, 429.0, "Park"),
    ] {
        h.click_at(preview_slide_pt(h, mx, my));
        h.idle_frame();
        assert_eq!(
            h.editor
                .preview
                .as_ref()
                .unwrap()
                .state
                .as_ref()
                .unwrap()
                .marker_open
                .as_deref(),
            Some(id)
        );
        assert!(h.find_text(title).is_some());
        h.click_at(preview_slide_pt(h, 20.0, 20.0));
        h.idle_frame();
        assert!(
            h.editor
                .preview
                .as_ref()
                .unwrap()
                .state
                .as_ref()
                .unwrap()
                .marker_open
                .is_none()
        );
    }
    h.click_at(preview_slide_pt(h, 1092.0, 624.0));
    advance_and_settle_preview(h);
    assert_eq!(preview_current_slide(h), "q-mc");
}

fn submit_and_continue_feature_tour_quiz(
    h: &mut TestHarness,
    layer: &str,
    button: &str,
    next: &str,
) {
    h.click_at(preview_slide_pt(h, 178.0, 612.0));
    advance_and_settle_preview(h);
    assert!(preview_has_layer(h, layer), "expected layer {layer}");
    let btn = h.find_text(button).expect("feedback button");
    h.click_at(btn);
    advance_and_settle_preview(h);
    assert_eq!(preview_current_slide(h), next);
}

fn walk_feature_tour_quiz_and_results(h: &mut TestHarness) {
    // Slide 6: q-mc (Multiple choice) — wrong attempt + Try again + correct attempt
    let mc_wrong = h.find_text("A new scene").expect("MC wrong choice");
    h.click_at(mc_wrong);
    h.idle_frame();
    submit_and_continue_feature_tour_quiz(h, "q-mc-incorrect", "Try again", "q-mc");
    let mc_a = h.find_text("A layer").expect("MC choice A layer");
    h.click_at(mc_a);
    h.idle_frame();
    submit_and_continue_feature_tour_quiz(h, "q-mc-correct", "Continue  →", "q-tf");

    // Slide 7: q-tf (True/False)
    let tf_true = h.find_text("True").expect("TF True");
    h.click_at(tf_true);
    h.idle_frame();
    submit_and_continue_feature_tour_quiz(h, "q-tf-correct", "Continue  →", "q-fill");

    // Slide 8: q-fill (Fill-in)
    let fill_pos = h
        .find_text("Type your answer")
        .expect("fill-in placeholder");
    h.click_at(fill_pos);
    h.idle_frame();
    h.frame(vec![egui::Event::Text("variable".to_owned())]);
    h.idle_frame();
    assert_eq!(
        h.editor
            .preview
            .as_ref()
            .unwrap()
            .state
            .as_ref()
            .unwrap()
            .text_entry,
        "variable"
    );
    submit_and_continue_feature_tour_quiz(h, "q-fill-correct", "Continue  →", "q-match");

    // Slide 9: q-match (Matching) — mouse dropdown interaction for all 3 pairs
    let match_prompt = "Match each feature to what it does.";
    let chooses = preview_match_combo_centers(h);
    assert_eq!(chooses.len(), 3, "q-match must show 3 Choose… dropdowns");
    pick_matching_option_in_preview(
        h,
        chooses[0],
        match_prompt,
        "Runs an action when something happens",
    );
    pick_matching_option_in_preview(h, chooses[1], match_prompt, "Changes how an object looks");
    pick_matching_option_in_preview(h, chooses[2], match_prompt, "Opens a pop-up on a picture");
    submit_and_continue_feature_tour_quiz(h, "q-match-correct", "Continue  →", "q-map");

    // Slide 10: q-map (Hotspot)
    h.click_at(preview_slide_pt(h, 230.0, 220.0));
    h.idle_frame();
    assert_eq!(
        h.editor
            .preview
            .as_ref()
            .unwrap()
            .state
            .as_ref()
            .unwrap()
            .hotspot_markers
            .len(),
        1
    );
    submit_and_continue_feature_tour_quiz(h, "q-map-correct", "Continue  →", "q-sort");

    // Slide 11: q-sort (Drag and drop)
    for (sx, sy, tx, ty) in [
        (184.0, 224.0, 288.0, 425.0),
        (392.0, 224.0, 288.0, 425.0),
        (600.0, 224.0, 708.0, 425.0),
        (808.0, 224.0, 708.0, 425.0),
    ] {
        let from = preview_slide_pt(h, sx, sy);
        let to = preview_slide_pt(h, tx, ty);
        h.drag_by(from, to - from, 4, Modifiers::NONE);
        h.idle_frame();
    }
    submit_and_continue_feature_tour_quiz(h, "q-sort-correct", "See result  →", "results");

    // Slide 12: results (6/6 = 100% -> passed)
    assert!(preview_has_layer(h, "passed"));
    assert!(
        h.find_text("100%").is_some(),
        "results slide must show 100% after answering all 6 questions correctly"
    );
    let finish = h.find_text("Finish  ✓").expect("results Finish");
    h.click_at(finish);
    h.idle_frame();
    assert!(h.editor.preview.as_ref().unwrap().ended);
}

#[test]
fn feature_tour_all_slides_and_interactions_in_preview() {
    let json = include_str!("../../../examples/feature-tour/course.json");
    let course = Course::from_json(json).expect("valid feature-tour course");
    let mut h = TestHarness::new_with_size(course, (1280.0, 800.0));
    h.editor.open_preview_at(None);
    walk_feature_tour_stops(&mut h);
    walk_feature_tour_quiz_and_results(&mut h);
}
