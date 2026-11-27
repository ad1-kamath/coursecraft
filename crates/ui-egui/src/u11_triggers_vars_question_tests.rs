//! Step u11 regression tests: Triggers, Trigger Editor, Variables & Question Editors (`Area 7` & `Area 8`).

use crate::test_harness::{TestHarness, capture_layers, check_contrast, check_nested_scrolls};
use coursecraft_course::{Course, QuestionKind};
use coursecraft_render::ThemeMode;
use egui::{Key, Modifiers};

fn base_course_with_objects_and_vars() -> Course {
    let long_obj =
        "Extraordinarily Long Object Name That Exceeds Normal Combo Widths 1234567890 ABCDEF";
    let raw = serde_json::json!({
        "schema_version": 1,
        "id": "c-u11",
        "title": "Step u11 Course",
        "slide_size": { "w": 1280, "h": 720 },
        "masters": [{ "id": "m1", "title": "Default Master", "objects": [] }],
        "variables": [
            { "name": "score", "value": 10.0 },
            { "name": "flag", "value": false },
            { "name": "learner_name", "value": "Alex" }
        ],
        "scenes": [{
            "id": "scene1",
            "title": "Main",
            "slides": [{
                "id": "s1",
                "title": "Slide 1",
                "duration_ms": 60000,
                "objects": [
                    {
                        "id": "obj1",
                        "name": long_obj,
                        "frame": { "x": 100, "y": 100, "w": 240, "h": 120 },
                        "kind": { "type": "shape", "shape": "rectangle", "fill": "#2563EB" }
                    },
                    {
                        "id": "obj2",
                        "name": "Target Two",
                        "frame": { "x": 400, "y": 100, "w": 200, "h": 100 },
                        "kind": { "type": "shape", "shape": "rectangle", "fill": "#10B981" }
                    }
                ],
                "layers": [
                    { "id": "layer1", "name": "Correct", "duration_ms": 5000, "objects": [] }
                ],
                "triggers": []
            }]
        }]
    });
    serde_json::from_value(raw).expect("valid course json")
}

#[test]
fn t7_1_triggers_empty_state_and_broken_reference_contrast() {
    let mut h = TestHarness::new_with_config(
        base_course_with_objects_and_vars(),
        (1280.0, 800.0),
        ThemeMode::Dark,
        1.0,
        true,
    );
    h.editor.ui.bottom_tab = crate::BottomTab::Triggers;
    let report = h.layout_report(1);
    assert!(
        report.painted("No triggers on this slide yet"),
        "Empty Triggers tab must explain what triggers do"
    );
    assert_eq!(
        h.find_texts("+ Add trigger").len(),
        1,
        "Empty Triggers tab must show exactly one + Add trigger button"
    );

    // 15 triggers including a broken target warning in both themes.
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        let mut course = base_course_with_objects_and_vars();
        let triggers: Vec<coursecraft_course::Trigger> = (0..15)
            .map(|i| {
                let layer = if i == 0 {
                    "missing_layer".to_owned()
                } else {
                    "layer1".to_owned()
                };
                coursecraft_course::Trigger {
                    action: coursecraft_course::Action::ShowLayer { layer },
                    when: coursecraft_course::Event::Click {
                        object: "obj2".to_owned(),
                    },
                    condition: None,
                    disabled: false,
                }
            })
            .collect();
        course.scenes[0].slides[0].triggers = triggers;

        let mut h = TestHarness::new_with_config(course, (1280.0, 800.0), mode, 1.0, true);
        h.editor.ui.bottom_tab = crate::BottomTab::Triggers;
        let _shapes = h.frame_shapes(vec![]);
        let layers = capture_layers(&h.ctx);
        let p = coursecraft_render::palette(&h.ctx);
        let contrast_issues = check_contrast(&layers, &[], &p, &h.last_widgets);
        let trigger_contrast: Vec<_> = contrast_issues
            .into_iter()
            .filter(|msg| msg.contains("needs attention") || msg.contains("missing_obj"))
            .collect();
        assert!(
            trigger_contrast.is_empty(),
            "Broken trigger warning failed contrast in {mode:?}: {trigger_contrast:?}"
        );
    }
}

#[test]
fn t7_2_trigger_editor_tall_combo_width_and_twelve_item_height() {
    let mut h = TestHarness::new_with_size(base_course_with_objects_and_vars(), (1280.0, 800.0));
    h.editor.ui.bottom_tab = crate::BottomTab::Triggers;
    h.idle_frame();
    let add_pos = h.find_text("+ Add trigger").expect("+ Add trigger button");
    h.click_at(add_pos);
    h.idle_frame();
    assert!(
        h.editor.ui.trigger_form.is_some(),
        "Trigger editor should open"
    );

    // Closed Action combo must have reasonable width (>= 120 pt), and its popup must fit >= 12 items.
    let action_pos = h
        .find_text("Jump to slide")
        .expect("Action combo closed label");
    let action_node = h
        .last_widgets
        .iter()
        .find(|n| n.bounds.contains(action_pos))
        .expect("Action combo widget node");
    assert!(
        action_node.bounds.width() >= 120.0,
        "Closed tall_combo must be >= 120 pt wide, got {}",
        action_node.bounds.width()
    );

    // Open Action combo (20+ actions) and verify popup height fits >= 12 items (~>= 240 pt).
    let audit = h.audit_combo("Jump to slide");
    assert!(
        audit.is_empty(),
        "Action tall_combo audit failed: {audit:?}"
    );
    h.click_at(action_pos);
    h.idle_frame();
    h.idle_frame();
    let layers = capture_layers(&h.ctx);
    let fg_shapes: Vec<_> = layers
        .iter()
        .filter(|(id, _)| id.order == egui::Order::Foreground)
        .flat_map(|(_, s)| s.iter())
        .collect();
    if let Some(popup_rect) = fg_shapes
        .iter()
        .map(|s| s.clip_rect)
        .reduce(|a, b| a.union(b))
    {
        assert!(
            popup_rect.height() >= 240.0,
            "Action tall_combo popup must fit >= 12 items (>= 240 pt), got {}",
            popup_rect.height()
        );
    }
}

#[test]
fn t7_3_trigger_editor_javascript_action_has_zero_nested_scroll() {
    let mut h = TestHarness::new_with_size(base_course_with_objects_and_vars(), (1024.0, 600.0));
    h.editor.ui.bottom_tab = crate::BottomTab::Triggers;
    h.idle_frame();
    let add_pos = h.find_text("+ Add trigger").expect("+ Add trigger button");
    h.click_at(add_pos);
    h.idle_frame();

    let form = h
        .editor
        .ui
        .trigger_form
        .as_mut()
        .expect("trigger form open");
    form.action_tag = crate::triggers::ActionTag::RunJavaScript;
    h.idle_frame();

    let layers = capture_layers(&h.ctx);
    let scrolls = crate::scroll_audit::recorded_scrolls(&h.ctx);
    let nested = check_nested_scrolls(&layers, &scrolls);
    assert!(
        nested.is_empty(),
        "Trigger editor Run JavaScript must have zero nested scroll areas: {nested:?}"
    );
}

#[test]
fn t7_4_trigger_editor_escape_closes_open_combo_before_dialog() {
    let mut h = TestHarness::new_with_size(base_course_with_objects_and_vars(), (1280.0, 800.0));
    h.editor.ui.bottom_tab = crate::BottomTab::Triggers;
    h.idle_frame();
    let add_pos = h.find_text("+ Add trigger").expect("+ Add trigger button");
    h.click_at(add_pos);
    h.idle_frame();

    let action_pos = h.find_text("Jump to slide").expect("Action combo");
    h.click_at(action_pos);
    h.idle_frame();
    assert!(
        egui::Popup::is_any_open(&h.ctx),
        "Action combo popup must be open"
    );

    // First Esc closes only the combo.
    h.press(Key::Escape, Modifiers::NONE);
    h.idle_frame();
    assert!(
        !egui::Popup::is_any_open(&h.ctx),
        "First Esc must close the combo popup"
    );
    assert!(
        h.editor.ui.trigger_form.is_some(),
        "First Esc must keep Trigger Editor open"
    );

    // Second Esc closes the Trigger Editor.
    h.press(Key::Escape, Modifiers::NONE);
    h.idle_frame();
    assert!(
        h.editor.ui.trigger_form.is_none(),
        "Second Esc must close Trigger Editor"
    );
}

#[test]
fn t7_5_trigger_editor_event_ms_unit_colon_free_conditions_and_cmd_enter_undo() {
    let mut h = TestHarness::new_with_size(base_course_with_objects_and_vars(), (1280.0, 800.0));
    h.editor.ui.bottom_tab = crate::BottomTab::Triggers;
    h.idle_frame();
    let add_pos = h.find_text("+ Add trigger").expect("+ Add trigger button");
    h.click_at(add_pos);
    h.idle_frame();

    {
        let form = h
            .editor
            .ui
            .trigger_form
            .as_mut()
            .expect("trigger form open");
        form.event_tag = crate::triggers::EventKindTag::TimelineReaches;
        form.event_ms = 2500;
        form.condition_enabled = true;
        form.condition_root = crate::triggers::ConditionNode::All(vec![]);
    }
    let report = h.layout_report(1);

    assert!(
        report.painted(" ms"),
        "event_ms DragValue must display ' ms' unit suffix"
    );
    assert!(
        !report.painted("the following:"),
        "Compound condition header must not have a trailing colon"
    );
    assert!(
        report.painted("the following"),
        "Compound condition header should say 'the following'"
    );

    let before = h.editor.session.course.clone();
    h.press(Key::Enter, Modifiers::COMMAND);
    h.idle_frame();
    assert!(
        h.editor.ui.trigger_form.is_none(),
        "Cmd/Ctrl+Enter should commit and close Trigger Editor"
    );
    assert_ne!(h.editor.session.course, before);
    h.press(Key::Z, Modifiers::COMMAND);
    h.idle_frame();
    assert_eq!(
        h.editor.session.course, before,
        "Single Cmd/Ctrl+Z must undo the trigger commit"
    );
}

#[test]
fn t7_6_trigger_row_disabled_actions_explain_why_on_hover() {
    let mut course = base_course_with_objects_and_vars();
    course.scenes[0].slides[0].triggers = vec![
        coursecraft_course::Trigger {
            action: coursecraft_course::Action::ShowLayer {
                layer: "layer1".to_owned(),
            },
            when: coursecraft_course::Event::Click {
                object: "obj1".to_owned(),
            },
            condition: None,
            disabled: false,
        },
        coursecraft_course::Trigger {
            action: coursecraft_course::Action::HideLayer {
                layer: Some("layer1".to_owned()),
            },
            when: coursecraft_course::Event::Click {
                object: "obj2".to_owned(),
            },
            condition: None,
            disabled: false,
        },
    ];

    let mut h = TestHarness::new_with_size(course, (1280.0, 800.0));
    h.editor.ui.bottom_tab = crate::BottomTab::Triggers;
    h.idle_frame();

    // Click first row to reveal its action buttons.
    let first_row = h.find_text("1.").expect("first trigger row");
    h.click_at(first_row);
    h.idle_frame();

    // Move Up on the first row must be disabled and explain why on hover.
    let up_node = h
        .last_widgets
        .iter()
        .find(|n| n.label.as_deref() == Some("Already at the top"))
        .expect("Disabled Move up button on first row");
    assert!(up_node.disabled, "Move up on first row must be disabled");
    let up_center = up_node.bounds.center();
    h.hover_at(up_center);
    h.idle_frame();
    let up_tip = h
        .probe_tooltip(up_center)
        .expect("Disabled Move up must show tooltip");
    assert!(
        up_tip.to_ascii_lowercase().contains("top")
            || up_tip.to_ascii_lowercase().contains("first")
            || up_tip.to_ascii_lowercase().contains("already"),
        "Disabled Move up tooltip should explain why it is disabled, got '{up_tip}'"
    );

    // Open Trigger Editor while row is active, then open row `...` menu and hover disabled Duplicate.
    h.double_click_at(first_row);
    h.idle_frame();
    assert!(h.editor.ui.trigger_form.is_some());

    let more_pos = h
        .find_icon(coursecraft_render::Icon::DotsThree)
        .expect("More trigger actions button");
    h.click_at(more_pos);
    h.idle_frame();
    h.idle_frame();
    let dup_pos = h.find_text_last("Duplicate").expect("Duplicate menu item");
    h.hover_at(dup_pos);
    let dup_tip = h
        .probe_tooltip(dup_pos)
        .expect("Disabled Duplicate must show hover explanation");
    assert!(
        dup_tip.len() >= 8,
        "Disabled Duplicate tooltip must explain why, got '{dup_tip}'"
    );
}

#[test]
fn t7_7_variables_tab_combo_width_disabled_add_tooltip_and_long_list() {
    let mut course = base_course_with_objects_and_vars();
    course.variables.clear();

    let mut h = TestHarness::new_with_size(course, (1280.0, 800.0));
    h.editor.ui.bottom_tab = crate::BottomTab::Variables;
    let report = h.layout_report(1);

    // Empty state guidance
    assert!(
        report.painted("No variables yet"),
        "Empty Variables tab must show guidance"
    );

    // Type combo must have explicit width (>= 120 pt)
    let type_pos = h.find_text("Number").expect("var_new_type_combo");
    let type_node = h
        .last_widgets
        .iter()
        .find(|n| n.bounds.contains(type_pos))
        .expect("var_new_type_combo widget node");
    assert!(
        type_node.bounds.width() >= 120.0,
        "var_new_type_combo must be >= 120 pt wide, got {}",
        type_node.bounds.width()
    );

    // Add 25 long-named variables and check disabled + Add tooltip for empty and duplicate name.
    for i in 0..25 {
        let name = format!("long_variable_name_{i:02}_abcdefghijklmnopqrstuvwxyz_1234567890");
        h.editor.run(
            "variable.add",
            serde_json::json!({ "name": name, "value": i as f64 }),
        );
    }
    h.idle_frame();

    // Empty name -> + Add disabled with tooltip
    let add_pos = h.find_text("+ Add").expect("+ Add button");
    let tip_empty = h
        .probe_tooltip(add_pos)
        .expect("Disabled + Add with empty name must show tooltip");
    assert!(
        tip_empty.contains("Enter a unique variable name"),
        "Expected 'Enter a unique variable name' tooltip, got '{tip_empty}'"
    );

    // Duplicate name -> + Add also disabled with tooltip
    h.editor.ui.variables.new_name =
        "long_variable_name_00_abcdefghijklmnopqrstuvwxyz_1234567890".to_owned();
    h.idle_frame();
    let tip_dup = h
        .probe_tooltip(add_pos)
        .expect("Disabled + Add with duplicate name must show tooltip");
    assert!(
        tip_dup.contains("Enter a unique variable name"),
        "Expected 'Enter a unique variable name' tooltip for duplicate, got '{tip_dup}'"
    );

    // Single scroll area
    let layers = capture_layers(&h.ctx);
    let scrolls = crate::scroll_audit::recorded_scrolls(&h.ctx);
    let nested = check_nested_scrolls(&layers, &scrolls);
    assert!(
        nested.is_empty(),
        "Variables tab must have 1 scroll area: {nested:?}"
    );
}

#[test]
fn t7_8_variable_default_value_editing_and_undo() {
    let mut h = TestHarness::new_with_size(base_course_with_objects_and_vars(), (1280.0, 800.0));
    h.editor.ui.bottom_tab = crate::BottomTab::Variables;
    h.idle_frame();

    // Edit Number default via typing
    let num_pos = h.find_text("10").expect("score DragValue");
    let before_num = h.editor.session.course.clone();
    h.set_field(num_pos, "42");
    h.idle_frame();
    assert_ne!(h.editor.session.course, before_num);
    h.press(Key::Z, Modifiers::COMMAND);
    h.idle_frame();
    assert_eq!(h.editor.session.course, before_num, "Undo number edit");

    // Edit Bool default
    let before_bool = h.editor.session.course.clone();
    let flag_label = h.find_text("flag").expect("flag label");
    let bool_cb = h
        .last_widgets
        .iter()
        .filter(|n| n.role == egui::accesskit::Role::CheckBox)
        .min_by_key(|n| ((n.bounds.center().y - flag_label.y).abs() * 10.0) as i32)
        .expect("flag checkbox");
    h.click_at(bool_cb.bounds.center());
    h.idle_frame();
    assert_ne!(h.editor.session.course, before_bool);
    h.press(Key::Z, Modifiers::COMMAND);
    h.idle_frame();
    assert_eq!(h.editor.session.course, before_bool, "Undo bool edit");
}

#[test]
fn t8_1_all_twelve_question_kinds_and_results_at_1024x600_both_themes() {
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        for &kind in &crate::question::ALL_QUESTION_KINDS {
            let mut h = TestHarness::new_with_config(
                base_course_with_objects_and_vars(),
                (1024.0, 600.0),
                mode,
                1.0,
                true,
            );
            crate::toolbar::insert_question(&mut h.editor, kind);
            h.editor.ui.bottom_tab = crate::BottomTab::Question;
            let report = h.layout_report(3);
            let bad: Vec<_> = report
                .findings
                .iter()
                .filter(|i| {
                    i.starts_with("clipped")
                        || i.starts_with("overlap")
                        || i.starts_with("off-window")
                })
                .collect();
            assert!(
                bad.is_empty(),
                "Question kind {kind:?} in {mode:?} at 1024x600 had layout issues: {bad:?}"
            );
        }

        // Results slide
        let mut h = TestHarness::new_with_config(
            base_course_with_objects_and_vars(),
            (1024.0, 600.0),
            mode,
            1.0,
            true,
        );
        crate::toolbar::insert_results_slide(&mut h.editor);
        h.editor.ui.bottom_tab = crate::BottomTab::Question;
        let report = h.layout_report(3);
        let bad: Vec<_> = report
            .findings
            .iter()
            .filter(|i| {
                i.starts_with("clipped") || i.starts_with("overlap") || i.starts_with("off-window")
            })
            .collect();
        assert!(
            bad.is_empty(),
            "Results slide in {mode:?} at 1024x600 had layout issues: {bad:?}"
        );
    }
}

#[test]
fn t8_2_clean_room_terminology_across_question_types() {
    assert!(
        crate::question::question_kind_name(QuestionKind::Freeform).contains("Custom question"),
        "Freeform must be labelled 'Custom question'"
    );
    assert!(
        !crate::question::question_kind_name(QuestionKind::Freeform)
            .to_ascii_lowercase()
            .contains("freeform"),
        "Must never paint 'Freeform' in UI"
    );

    let mut h = TestHarness::new_with_size(base_course_with_objects_and_vars(), (1280.0, 800.0));
    crate::toolbar::insert_question(&mut h.editor, QuestionKind::Freeform);
    h.editor.ui.bottom_tab = crate::BottomTab::Question;
    let report = h.layout_report(1);
    assert!(report.painted("Custom question"));
    assert!(!report.painted("Freeform"));
}

#[test]
fn t8_3_non_question_slide_shows_empty_state_guidance_and_action() {
    let mut h = TestHarness::new_with_size(base_course_with_objects_and_vars(), (1280.0, 800.0));
    h.editor.ui.bottom_tab = crate::BottomTab::Question;
    let report = h.layout_report(1);
    assert!(
        report.painted("This slide has no question"),
        "Non-question slide must show clear explanation"
    );
    assert!(
        h.find_text("Add question")
            .or_else(|| h.find_text("Question"))
            .is_some(),
        "Non-question slide must offer a single quiet/primary action to add a question"
    );
}

#[test]
fn t8_4_choice_delete_disabled_at_minimum_count_with_tooltip() {
    let mut h = TestHarness::new_with_size(base_course_with_objects_and_vars(), (1280.0, 800.0));
    crate::toolbar::insert_question(&mut h.editor, QuestionKind::MultipleChoice);
    h.editor.ui.bottom_tab = crate::BottomTab::Question;
    h.idle_frame();

    // Trim choices down to 2 (minimum for Multiple Choice).
    while h.editor.ui.question.choices.len() > 2 {
        h.editor.ui.question.choices.pop();
    }
    h.idle_frame();

    // Hover Delete choice button — must be disabled and explain minimum choices.
    let trash_pos = h
        .find_icon(coursecraft_render::Icon::Trash)
        .expect("Remove choice trash icon");
    let trash_node = h
        .last_widgets
        .iter()
        .find(|n| n.bounds.contains(trash_pos))
        .expect("Remove choice widget node");
    assert!(
        trash_node.disabled,
        "Remove choice must be disabled at minimum count (2 choices)"
    );
    let tip = h
        .probe_tooltip(trash_pos)
        .expect("Disabled Remove choice must show tooltip");
    assert!(
        tip.to_ascii_lowercase().contains("at least two")
            || tip.to_ascii_lowercase().contains("at least 2")
            || tip.to_ascii_lowercase().contains("minimum"),
        "Disabled Remove choice tooltip should explain minimum choices, got '{tip}'"
    );
}

#[test]
fn t8_5_drag_drop_combos_fit_long_names_and_twelve_item_popup() {
    let mut course = base_course_with_objects_and_vars();
    let mut raw: serde_json::Value = serde_json::to_value(&course).expect("serialize");
    let mut objs = Vec::new();
    for i in 0..16 {
        objs.push(serde_json::json!({
            "id": format!("dd_{i}"),
            "name": format!("Long_Drag_Or_Drop_Object_Name_{i:02}_1234567890_ABCDEFGHIJKLMNOPQRSTUVWXYZ"),
            "frame": { "x": 50 + (i * 20), "y": 50, "w": 80, "h": 40 },
            "kind": { "type": "shape", "shape": "rectangle", "fill": "#2563EB" }
        }));
    }
    raw["scenes"][0]["slides"][0]["objects"] = serde_json::Value::Array(objs);
    course = serde_json::from_value(raw).expect("deserialize");
    course.scenes[0].slides[0].question = Some(coursecraft_course::Question {
        kind: QuestionKind::DragDrop,
        prompt: "Match each item".to_owned(),
        choices: vec![],
        points: 10,
        scoring: coursecraft_course::Scoring::AllOrNothing,
        attempts: 1,
        shuffle: false,
        correct_layer: None,
        incorrect_layer: None,
        try_again_layer: None,
        accepted: vec![],
        case_sensitive: false,
        pairs: vec![],
        hotspot_image: None,
        regions: vec![],
        markers: 1,
        drag_items: vec![coursecraft_course::DragItem {
            item: "dd_0".to_owned(),
            target: "dd_1".to_owned(),
        }],
        drop_targets: (1..16).map(|i| format!("dd_{i}")).collect(),
        snap_to_center: true,
        return_if_wrong: false,
        one_per_target: false,
        correct_when: None,
        frame: None,
    });

    let mut h = TestHarness::new_with_size(course, (1280.0, 800.0));
    h.editor.ui.bottom_tab = crate::BottomTab::Question;
    h.idle_frame();
    if h.find_text_containing("Long_Drag_Or_Drop_").is_none() {
        let scroll_pos = h.editor.ui.question.scroll_inner_rect.center();
        h.scroll_over(scroll_pos, -240.0);
        h.idle_frame();
    }

    // Open the first drag target combo and verify popup height >= 240 pt for 12+ items.
    let target_pos = h
        .find_text_containing("Long_Drag_Or_Drop_Object_Name_01")
        .expect("target combo");
    let audit = h.audit_popup(|h| {
        h.click_at(target_pos);
    });
    assert!(
        audit.is_empty(),
        "DragDrop target combo audit failed: {audit:?}"
    );
}

#[test]
fn t8_6_question_points_and_attempts_display_units_and_undo() {
    let mut h = TestHarness::new_with_size(base_course_with_objects_and_vars(), (1280.0, 800.0));
    crate::toolbar::insert_question(&mut h.editor, QuestionKind::MultipleChoice);
    h.editor.ui.bottom_tab = crate::BottomTab::Question;
    let report = h.layout_report(1);

    assert!(
        report.painted(" pts"),
        "Points DragValue must display ' pts' unit suffix"
    );

    let pts_pos = h.find_text_containing(" pts").expect("Points DragValue");
    let before = h.editor.session.course.clone();
    h.set_field(pts_pos, "25");
    h.idle_frame();
    assert_ne!(h.editor.session.course, before);
    h.press(Key::Z, Modifiers::COMMAND);
    h.idle_frame();
    assert_eq!(h.editor.session.course, before, "Undo points edit");
}

#[test]
fn t8_7_question_feedback_single_scroll_and_contrast_in_both_themes() {
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        let mut h = TestHarness::new_with_config(
            base_course_with_objects_and_vars(),
            (1024.0, 600.0),
            mode,
            1.0,
            true,
        );
        crate::toolbar::insert_question(&mut h.editor, QuestionKind::MultipleChoice);
        h.editor.ui.bottom_tab = crate::BottomTab::Question;
        h.idle_frame();

        let layers = capture_layers(&h.ctx);
        let scrolls = crate::scroll_audit::recorded_scrolls(&h.ctx);
        let nested = check_nested_scrolls(&layers, &scrolls);
        assert!(
            nested.is_empty(),
            "Question tab in {mode:?} must have 1 scroll area: {nested:?}"
        );
        let p = coursecraft_render::palette(&h.ctx);
        let contrast = check_contrast(&layers, &[], &p, &h.last_widgets);
        assert!(
            contrast.is_empty(),
            "Question tab in {mode:?} failed contrast: {contrast:?}"
        );
    }
}

#[test]
fn t8_8_question_edits_undo_atomically() {
    let mut h = TestHarness::new_with_size(base_course_with_objects_and_vars(), (1280.0, 800.0));
    crate::toolbar::insert_question(&mut h.editor, QuestionKind::MultipleChoice);
    h.editor.ui.bottom_tab = crate::BottomTab::Question;
    h.idle_frame();

    let before = h.editor.session.course.clone();
    let shuffle_pos = h.find_text("Shuffle choices").expect("Shuffle choices");
    h.click_at(shuffle_pos);
    h.idle_frame();
    assert_ne!(h.editor.session.course, before);

    h.press(Key::Z, Modifiers::COMMAND);
    h.idle_frame();
    assert_eq!(
        h.editor.session.course, before,
        "Single Cmd/Ctrl+Z must undo question edit"
    );
}
