//! Step u15 regression tests: Status Bar, Check Course, Welcome, Coach Marks, Shortcuts & Report (`Area 12`).

use crate::test_harness::{TestHarness, capture_layers, check_contrast, check_nested_scrolls};
use crate::{AuditFindingSummary, ValidationIssueSummary};
use coursecraft_course::Course;
use coursecraft_render::{Icon, ThemeMode, palette};
use egui::{Key, Modifiers, Rect, pos2, vec2};

fn base_course() -> Course {
    let raw = serde_json::json!({
        "schema_version": 1,
        "id": "c-u15",
        "title": "Step u15 Course",
        "slide_size": { "w": 1280, "h": 720 },
        "masters": [{ "id": "m1", "title": "Default Master", "objects": [] }],
        "scenes": [{
            "id": "scene1",
            "title": "Main",
            "slides": [{
                "id": "s1",
                "title": "Slide 1",
                "duration_ms": 60000,
                "objects": [
                    {
                        "id": "t1",
                        "name": "Text 1",
                        "frame": { "x": 100.0, "y": 100.0, "w": 300.0, "h": 80.0 },
                        "kind": { "type": "text", "text": "Welcome to CourseCraft" }
                    }
                ],
                "layers": [],
                "triggers": []
            }]
        }]
    });
    serde_json::from_value(raw).expect("valid course json")
}

fn capture_frame_layers(
    h: &mut TestHarness,
) -> Vec<(egui::LayerId, Vec<egui::epaint::ClippedShape>)> {
    let raw_input = egui::RawInput {
        screen_rect: Some(h.screen_rect),
        ..Default::default()
    };
    let mut layers = Vec::new();
    let output = h.ctx.run_ui(raw_input, |ui| {
        let _ = h.editor.show(ui);
        layers = capture_layers(ui.ctx());
    });
    output.drop_without_applying_deltas();
    layers
}

fn find_rect_at(shape: &egui::Shape, pos: egui::Pos2) -> Option<Rect> {
    match shape {
        egui::Shape::Rect(r)
            if r.rect.contains(pos)
                && r.rect.height() >= 18.0
                && r.rect.height() <= 30.0
                && r.rect.width() < 250.0 =>
        {
            Some(r.rect)
        }
        egui::Shape::Vec(v) => v.iter().find_map(|s| find_rect_at(s, pos)),
        _ => None,
    }
}

fn foreground_layer_containing(
    ctx: &egui::Context,
    layers: &[(egui::LayerId, Vec<egui::epaint::ClippedShape>)],
    text_substr: &str,
) -> Option<Rect> {
    fn has_text(shape: &egui::epaint::Shape, sub: &str) -> bool {
        match shape {
            egui::epaint::Shape::Text(t) => t.galley.text().contains(sub),
            egui::epaint::Shape::Vec(v) => v.iter().any(|s| has_text(s, sub)),
            _ => false,
        }
    }
    layers.iter().find_map(|(layer, shapes)| {
        if layer.order != egui::Order::Foreground {
            return None;
        }
        if !shapes.iter().any(|c| has_text(&c.shape, text_substr)) {
            return None;
        }
        if let Some(area_rect) = ctx.memory(|m| m.area_rect(layer.id))
            && area_rect.is_positive()
        {
            return Some(area_rect);
        }
        let mut rect = Rect::NOTHING;
        for c in shapes {
            rect = rect.union(c.shape.visual_bounding_rect());
        }
        (!rect.is_negative() && rect != Rect::NOTHING).then_some(rect)
    })
}

fn widget_rect_containing(h: &TestHarness, pos: egui::Pos2) -> Option<Rect> {
    h.last_widgets
        .iter()
        .filter(|w| w.bounds.contains(pos))
        .min_by(|a, b| {
            let area_a = a.bounds.width() * a.bounds.height();
            let area_b = b.bounds.width() * b.bounds.height();
            area_a.total_cmp(&area_b)
        })
        .map(|w| w.bounds)
}

#[test]
fn t12_1_status_bar_validation_chip_hit_target_and_long_title_truncation() {
    let mut h =
        TestHarness::new_with_config(base_course(), (1024.0, 600.0), ThemeMode::Dark, 1.5, true);
    let long_title = "Very_Long_Course_Filename_With_Many_Words_".repeat(3);
    assert!(long_title.len() >= 120);
    h.editor.title_info = Some((long_title, true));
    h.editor.ui.last_error = Some(("Save failed".to_owned(), 10.0));
    h.editor.ui.status_message = Some(("Copied".to_owned(), 10.0));
    h.editor.ui.current_time = 10.0;
    h.editor.automation_port = Some(9876);
    h.idle_frame();
    h.idle_frame();
    let _ = h.layout_report(2);

    // Validation pill hit target >= 24 pt and stays inside screen.
    let chip_pos = h
        .find_text_containing("error")
        .expect("validation summary chip");
    // Measure painted pill background rect in status bar.
    let layers = capture_frame_layers(&mut h);
    let mut chip_rect = None;
    for (_, shapes) in &layers {
        for c in shapes {
            if let Some(r) = find_rect_at(&c.shape, chip_pos) {
                chip_rect = Some(r);
            }
        }
    }
    let chip_rect = chip_rect.expect("validation chip rect");
    assert!(
        chip_rect.height() >= 24.0 - 0.5,
        "validation pill height must be >= 24 pt, got {}",
        chip_rect.height()
    );
    assert!(
        h.screen_rect.contains_rect(chip_rect),
        "validation chip {chip_rect:?} must fit inside screen {:?}",
        h.screen_rect
    );

    // Error button must also stay inside screen (not pushed off by 120-char filename).
    let err_pos = h
        .find_text("Save failed")
        .expect("transient error button in status bar");
    assert!(
        err_pos.x < h.screen_rect.max.x - 10.0,
        "error button {err_pos:?} must not clip off right edge of {:?}",
        h.screen_rect
    );
}

#[test]
fn t12_2_check_course_and_error_history_single_scroll_wrap_and_esc() {
    let mut h = TestHarness::new_with_size(base_course(), (1024.0, 600.0));
    h.editor.ui.audit_dirty = false;
    h.editor.ui.validation_issues = (0..15)
        .map(|i| ValidationIssueSummary {
            severity: if i % 2 == 0 {
                "error".to_owned()
            } else {
                "warning".to_owned()
            },
            path: format!("scenes/0/slides/0/objects/{i}"),
            message: format!(
                "Validation issue #{i}: very long description explaining what is missing on this slide object"
            ),
        })
        .collect();
    h.editor.ui.audit_findings = (0..15)
        .map(|i| AuditFindingSummary {
            severity: "warning".to_owned(),
            criterion: "1.1.1".to_owned(),
            slide: Some("s1".to_owned()),
            object: Some(format!("obj_{i}")),
            check: "alt-text".to_owned(),
            message: format!(
                "Accessibility finding #{i}: missing descriptive alternative text for screen reader users"
            ),
            fix: "Add concise alt text in the Format inspector.".to_owned(),
        })
        .collect();
    h.editor.ui.show_validation_window = true;
    h.idle_frame();
    h.idle_frame();

    let layers = capture_layers(&h.ctx);
    let scrolls = crate::scroll_audit::last_recorded_scrolls(&h.ctx);
    let nested = check_nested_scrolls(&layers, &scrolls);
    assert!(
        nested.is_empty(),
        "Check Course must have zero nested ScrollAreas, got: {nested:?}"
    );

    let report = h.layout_report(2);
    let clipped: Vec<_> = report
        .findings
        .iter()
        .filter(|f| {
            f.contains("Check course")
                || f.contains("Validation issue")
                || f.contains("Accessibility finding")
        })
        .collect();
    assert!(
        clipped.is_empty(),
        "Check Course long descriptions must wrap cleanly without clipping: {clipped:?}"
    );

    h.press(Key::Escape, Modifiers::NONE);
    h.idle_frame();
    assert!(
        !h.editor.ui.show_validation_window,
        "Esc must close Check Course"
    );

    // Error History with 20 errors
    h.editor.ui.error_history = (0..20)
        .map(|i| format!("Error #{i}: failed to load external asset from disk"))
        .collect();
    h.editor.ui.show_error_history_window = true;
    h.idle_frame();
    h.idle_frame();

    let layers_err = capture_layers(&h.ctx);
    let scrolls_err = crate::scroll_audit::last_recorded_scrolls(&h.ctx);
    let nested_err = check_nested_scrolls(&layers_err, &scrolls_err);
    assert!(
        nested_err.is_empty(),
        "Error History must have zero nested ScrollAreas, got: {nested_err:?}"
    );

    h.press(Key::Escape, Modifiers::NONE);
    h.idle_frame();
    assert!(
        !h.editor.ui.show_error_history_window,
        "Esc must close Error History"
    );
}

#[test]
fn t12_3_welcome_screen_geometry_and_recent_courses_at_1024x600_scale_1_5() {
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        for (size, scale) in [
            ((1024.0, 600.0), 1.0),
            ((1024.0, 600.0), 1.5),
            ((1280.0, 800.0), 1.0),
            ((1920.0, 1080.0), 1.5),
        ] {
            let mut h = TestHarness::new_with_config(base_course(), size, mode, scale, true);
            h.editor.recent_files = (0..10)
                .map(|i| {
                    format!(
                        "/Users/author/Documents/Very_Long_Project_Folder_Name_{i}/Subfolder/My_Very_Long_Course_Title_Number_{i}.ccraft"
                    )
                })
                .collect();
            h.editor.ui.show_welcome = true;
            h.editor.ui.has_loaded_course = true;
            h.idle_frame();
            h.idle_frame();

            assert!(
                h.find_text("Back to course").is_some(),
                "Back to course button must be visible ({size:?}, {scale}, {mode:?})"
            );

            let report = h.layout_report(2);
            assert!(
                report.findings.is_empty(),
                "Welcome screen layout findings at {size:?} scale {scale} {mode:?}: {:?}",
                report.findings
            );
        }
    }
}

#[test]
fn t12_4_welcome_example_popup_clamping_and_template_contrast() {
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        let mut h = TestHarness::new_with_theme(base_course(), (1024.0, 600.0), mode);
        h.editor.ui.show_welcome = true;
        h.idle_frame();
        h.idle_frame();

        let layers = capture_layers(&h.ctx);
        let p = palette(&h.ctx);
        let contrast = check_contrast(&layers, &[], &p, &[]);
        assert!(
            contrast.is_empty(),
            "Welcome screen contrast failures in {mode:?}: {contrast:?}"
        );

        let pos = h
            .find_text("Explore an example")
            .expect("Explore an example tile");
        h.click_at(pos);
        h.idle_frame();

        let hc_pos = h
            .find_text("Workplace Safety Basics")
            .expect("example popup menu should open");
        assert!(
            h.screen_rect.contains(hc_pos),
            "Example popup item {hc_pos:?} must be inside screen {:?}",
            h.screen_rect
        );

        h.press(Key::Escape, Modifiers::NONE);
        h.idle_frame();
        assert!(
            h.editor.ui.show_welcome,
            "Esc must close example popup without dismissing Welcome screen"
        );
    }
}

#[test]
fn t12_5_coach_marks_anchor_to_live_panels_clamp_and_esc() {
    let mut h =
        TestHarness::new_with_config(base_course(), (1024.0, 600.0), ThemeMode::Dark, 1.5, true);
    h.idle_frame();

    // Resize left navigator to 320 px (or max) and right inspector to 400 px (or max).
    let left0 = h.left_panel_rect();
    h.drag_by(
        pos2(left0.max.x, left0.center().y),
        vec2(100.0, 0.0),
        4,
        Modifiers::NONE,
    );
    let right0 = h.right_panel_rect();
    h.drag_by(
        pos2(right0.min.x, right0.center().y),
        vec2(-100.0, 0.0),
        4,
        Modifiers::NONE,
    );
    h.idle_frame();

    h.editor.set_coach_marks_done(false);
    h.editor.ui.current_coach_mark = Some(1);
    h.idle_frame();
    h.idle_frame();
    let _ = h.layout_report(2);

    // Step 1: check Next button >= 26 pt and popover inside screen
    let next_pos = h.find_text("Next").expect("coach mark 1 Next button");
    let next_rect = widget_rect_containing(&h, next_pos).expect("Next button rect");
    assert!(
        next_rect.height() >= 26.0 - 0.5,
        "Coach Next button height must be >= 26 pt, got {}",
        next_rect.height()
    );
    let layers1 = capture_frame_layers(&mut h);
    let coach1_rect =
        foreground_layer_containing(&h.ctx, &layers1, "1 of 3").expect("coach mark 1 layer");
    assert!(
        h.screen_rect.contains_rect(coach1_rect),
        "Coach mark 1 {coach1_rect:?} must fit inside screen {:?}",
        h.screen_rect
    );

    // Advance to step 2
    h.click_at(next_pos);
    h.idle_frame();
    assert_eq!(h.editor.ui.current_coach_mark, Some(2));
    let layers2 = capture_frame_layers(&mut h);
    let coach2_rect =
        foreground_layer_containing(&h.ctx, &layers2, "2 of 3").expect("coach mark 2 layer");
    assert!(
        h.screen_rect.contains_rect(coach2_rect),
        "Coach mark 2 {coach2_rect:?} must fit inside screen {:?}",
        h.screen_rect
    );

    // Advance to step 3
    let next2 = h.find_text("Next").expect("coach mark 2 Next button");
    h.click_at(next2);
    h.idle_frame();
    assert_eq!(h.editor.ui.current_coach_mark, Some(3));
    let layers3 = capture_frame_layers(&mut h);
    let coach3_rect =
        foreground_layer_containing(&h.ctx, &layers3, "3 of 3").expect("coach mark 3 layer");
    assert!(
        h.screen_rect.contains_rect(coach3_rect),
        "Coach mark 3 {coach3_rect:?} must fit inside screen {:?}",
        h.screen_rect
    );

    // Esc must dismiss coach marks
    h.press(Key::Escape, Modifiers::NONE);
    h.idle_frame();
    assert!(h.editor.coach_marks_done(), "Esc must dismiss coach marks");
}

#[test]
fn t12_6_shortcuts_modal_backdrop_close_tooltip_and_bounds() {
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        for size in [(1024.0, 600.0), (1440.0, 900.0)] {
            let mut h = TestHarness::new_with_theme(base_course(), size, mode);
            h.editor.ui.show_shortcuts_dialog = true;
            h.idle_frame();
            h.idle_frame();
            let _ = h.layout_report(2);

            let layers = capture_frame_layers(&mut h);
            let dlg_rect = foreground_layer_containing(&h.ctx, &layers, "Keyboard shortcuts")
                .expect("shortcuts dialog layer");
            assert!(
                dlg_rect.min.y >= h.top_bars_height() - 0.5,
                "Shortcuts dialog {dlg_rect:?} must stay below top bars ({})",
                h.top_bars_height()
            );
            assert!(
                h.screen_rect.contains_rect(dlg_rect),
                "Shortcuts dialog {dlg_rect:?} must fit inside screen {:?}",
                h.screen_rect
            );

            let close_pos = h
                .find_text_in(&Icon::X.codepoint().to_string(), dlg_rect)
                .expect("Shortcuts close icon button");
            let close_rect = widget_rect_containing(&h, close_pos).expect("close button rect");
            assert!(
                close_rect.width() >= 26.0 - 0.5 && close_rect.height() >= 26.0 - 0.5,
                "Shortcuts close button must be >= 26x26, got {close_rect:?}"
            );
            let tip = h.probe_tooltip(close_pos);
            assert_eq!(
                tip.as_deref(),
                Some("Close (Esc)"),
                "Shortcuts close button must have 'Close (Esc)' tooltip"
            );

            let backdrop_findings = h.check_modals_and_backdrops(false);
            assert!(
                backdrop_findings.is_empty(),
                "Shortcuts modal backdrop findings at {size:?} {mode:?}: {backdrop_findings:?}"
            );
        }
    }
}

#[test]
fn t12_7_crash_recovery_and_report_problem_disabled_details_and_esc() {
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        let mut h = TestHarness::new_with_theme(base_course(), (1024.0, 600.0), mode);
        h.editor.set_crash_note(Some("panic in test".into()));
        h.idle_frame();
        h.idle_frame();

        // Reviewer note #4: keep platform-standard "Don't ask again"
        assert!(
            h.find_text("Don't ask again").is_some(),
            "Crash prompt keeps platform-standard 'Don't ask again'"
        );

        h.press(Key::Escape, Modifiers::NONE);
        h.idle_frame();
        assert!(
            !h.editor.ui.report.crash_prompt,
            "Esc must close crash recovery prompt"
        );

        // Open Report a Problem
        h.editor.open_report();
        h.editor.ui.report.include_details = false;
        h.idle_frame();
        h.idle_frame();

        // Disabled technical details TextEdit must explain why on hover
        let details_pos = h
            .find_text_containing("CourseCraft ")
            .expect("disabled technical details text");
        let tip = h.probe_tooltip(details_pos);
        assert!(
            tip.is_some(),
            "Disabled technical details preview must explain why on hover"
        );

        h.press(Key::Escape, Modifiers::NONE);
        h.idle_frame();
        assert!(
            !h.editor.ui.report.open,
            "Esc must close Report a Problem dialog"
        );
    }
}

#[test]
fn t12_8_pptx_import_report_single_scroll_and_pinned_dismiss_footer() {
    let mut h = TestHarness::new_with_size(base_course(), (1024.0, 600.0));
    let long_report = (1..=80)
        .map(|i| format!("Slide {i}: imported text and shape objects"))
        .collect::<Vec<_>>()
        .join("\n");
    h.editor.ui.pptx_import_report = Some(long_report);
    h.idle_frame();
    h.idle_frame();

    let layers = capture_layers(&h.ctx);
    let scrolls = crate::scroll_audit::last_recorded_scrolls(&h.ctx);
    let nested = check_nested_scrolls(&layers, &scrolls);
    assert!(
        nested.is_empty(),
        "PPTX Import Report must have zero nested ScrollAreas, got: {nested:?}"
    );

    let dismiss_pos = h
        .find_text("Dismiss")
        .expect("Dismiss button must be visible without scrolling");
    assert!(
        h.screen_rect.contains(dismiss_pos),
        "Dismiss button {dismiss_pos:?} must be on screen"
    );

    h.click_at(dismiss_pos);
    h.idle_frame();
    assert!(
        h.editor.ui.pptx_import_report.is_none(),
        "Clicking Dismiss must close PPTX Import Report"
    );
}

/// L-03: Status, report, and PowerPoint import-report dialogs paint sentence-case titles.
#[test]
fn status_and_report_dialogs_paint_sentence_case_titles() {
    let mut h = TestHarness::new_with_size(base_course(), (1280.0, 800.0));

    h.editor.ui.show_validation_window = true;
    h.idle_frame();
    assert!(
        h.find_text("Check course").is_some(),
        "expected 'Check course'"
    );
    assert!(h.find_text("Check Course").is_none());
    h.editor.ui.show_validation_window = false;
    h.idle_frame();

    h.editor.ui.error_history = vec!["Sample error".to_owned()];
    h.editor.ui.show_error_history_window = true;
    h.idle_frame();
    assert!(
        h.find_text("Error history").is_some(),
        "expected 'Error history'"
    );
    assert!(h.find_text("Error History").is_none());
    h.editor.ui.show_error_history_window = false;
    h.idle_frame();

    h.editor.ui.pptx_import_report = Some("Imported 3 slides.".to_owned());
    h.idle_frame();
    assert!(
        h.find_text("PowerPoint import report").is_some(),
        "expected 'PowerPoint import report'"
    );
    h.editor.ui.pptx_import_report = None;
    h.idle_frame();

    h.editor.open_report();
    h.idle_frame();
    assert!(
        h.find_text("Report a problem").is_some(),
        "expected 'Report a problem'"
    );
    assert!(
        h.find_text("Copy to clipboard").is_some(),
        "expected 'Copy to clipboard'"
    );
}
