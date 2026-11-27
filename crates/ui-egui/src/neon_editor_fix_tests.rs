//! Tests for the editor fixes of the 0.6.1 Neon bug hunt (`N84` to `N109`).

use crate::test_harness::TestHarness;
use coursecraft_course::Course;
use serde_json::json;

fn text_course() -> Course {
    let raw = json!({
        "schema_version": 1,
        "id": "c-neon",
        "title": "Neon",
        "slide_size": { "w": 1280, "h": 720 },
        "theme": { "accent": "#1166CC", "text": "#222222", "background": "#F5F5F5", "font": "Inter" },
        "scenes": [{
            "id": "scene1",
            "title": "Main",
            "slides": [{
                "id": "s1",
                "title": "Slide 1",
                "objects": [{
                    "id": "t1",
                    "frame": { "x": 100.0, "y": 100.0, "w": 400.0, "h": 120.0 },
                    "kind": { "type": "text", "text": "Hello world", "style": { "font": "Inter", "size": 20.0, "color": "#222222" } },
                    "states": [{ "name": "hover" }]
                }]
            }]
        }]
    });
    serde_json::from_value(raw).expect("valid course")
}

/// `N84`: the Format inspector's text commands patch the edited state, never the base object.
#[test]
fn n84_text_commands_route_into_the_edited_state() {
    use crate::state_text_route::{StateTextView, patch_for};
    let mut h = TestHarness::new(text_course());
    let (style, text) = {
        let (obj, _) = h
            .editor
            .session
            .course
            .slide("s1")
            .unwrap()
            .object("t1")
            .unwrap();
        match &obj.kind {
            coursecraft_course::ObjectKind::Text { style, text, .. } => {
                (style.clone(), text.clone())
            }
            _ => panic!("text object"),
        }
    };
    let actions = [
        (
            "text.style_range",
            json!({ "slide": "s1", "object": "t1", "start": 0, "end": 0, "bold": true }),
        ),
        (
            "text.set_paragraph",
            json!({ "slide": "s1", "object": "t1", "align": "center" }),
        ),
        (
            "text.set_paragraphs",
            json!({ "slide": "s1", "object": "t1", "list": "bullet" }),
        ),
        (
            "text.set_link",
            json!({ "slide": "s1", "object": "t1", "link": { "type": "url", "url": "https://example.com" } }),
        ),
        (
            "object.set_autofit",
            json!({ "slide": "s1", "object": "t1", "autofit": "shrink" }),
        ),
    ];
    for action in actions {
        let hover = h
            .editor
            .session
            .course
            .slide("s1")
            .unwrap()
            .object("t1")
            .unwrap()
            .0
            .states[0]
            .clone();
        let view = StateTextView {
            style: &style,
            runs: hover.runs.as_deref().unwrap_or(&[]),
            paragraphs: hover.paragraphs.as_deref().unwrap_or(&[]),
            text: &text,
        };
        let patch = patch_for(&action, &view).expect("routed");
        let ran = h.editor.run(
            "object.patch_state",
            json!({ "slide": "s1", "object": "t1", "name": "hover", "patch": patch }),
        );
        assert!(
            ran.is_some(),
            "{} patch must run: {:?}",
            action.0,
            h.editor.ui.last_error
        );
    }
    let (obj, _) = h
        .editor
        .session
        .course
        .slide("s1")
        .unwrap()
        .object("t1")
        .unwrap();
    let coursecraft_course::ObjectKind::Text {
        style,
        runs,
        paragraphs,
        autofit,
        ..
    } = &obj.kind
    else {
        panic!("text object")
    };
    assert!(
        !style.bold && runs.is_empty() && paragraphs.is_empty(),
        "base object untouched"
    );
    assert_ne!(*autofit, coursecraft_course::Autofit::Shrink);
    let hover = &obj.states[0];
    assert!(
        hover
            .runs
            .as_ref()
            .is_some_and(|r| r.iter().any(|run| run.bold == Some(true)))
    );
    assert_eq!(
        hover.text_style.as_ref().map(|s| s.align),
        Some(coursecraft_course::Align::Center)
    );
    assert!(hover.paragraphs.as_ref().is_some_and(|p| !p.is_empty()));
    assert_eq!(hover.autofit, Some(coursecraft_course::Autofit::Shrink));
}

/// `N85`: Ctrl+B inside an inline edit in Edit-State Mode styles the state, not the base object.
#[test]
fn n85_inline_bold_in_edit_state_mode_patches_the_state() {
    use egui::pos2;
    use egui::text::{CCursor, CCursorRange};
    let mut h = TestHarness::new(text_course());
    h.editor.selection.set_many(["t1".to_owned()]);
    h.editor.enter_edit_state("s1", "t1", "hover");
    h.idle_frame();
    let slide_rect = h.editor.ui.last_slide_rect.expect("slide rect");
    h.double_click_at(pos2(
        slide_rect.min.x + slide_rect.width() * (300.0 / 1280.0),
        slide_rect.min.y + slide_rect.height() * (150.0 / 720.0),
    ));
    let edit = h.editor.ui.inline_edit.clone().expect("inline edit opened");
    assert_eq!(edit.state_name.as_deref(), Some("hover"));
    let range = CCursorRange::two(CCursor::new(0), CCursor::new(5));
    crate::canvas::apply_inline_edit_style_shortcut(
        &mut h.editor,
        &edit,
        &edit.buffer,
        Some(range),
        true,
        false,
        false,
    );
    let (obj, _) = h
        .editor
        .session
        .course
        .slide("s1")
        .unwrap()
        .object("t1")
        .unwrap();
    let coursecraft_course::ObjectKind::Text { runs, .. } = &obj.kind else {
        panic!("text object")
    };
    assert!(runs.is_empty(), "base runs must stay untouched");
    let hover = &obj.states[0];
    assert!(
        hover
            .runs
            .as_ref()
            .is_some_and(|r| r.iter().any(|run| run.bold == Some(true))),
        "the hover state must hold the bold run"
    );
    assert!(
        h.editor
            .ui
            .inline_edit
            .as_ref()
            .is_some_and(|e| e.live_runs.iter().any(|run| run.bold == Some(true)))
    );
}

/// `N87`: an outline imported from the Start page leaves the Start page.
#[test]
fn n87_outline_import_from_start_page_leaves_the_start_page() {
    let mut h = TestHarness::new(text_course());
    h.editor.ui.show_welcome = true;
    h.editor.ui.has_loaded_course = false;
    h.editor.ui.file_page.open = true;
    crate::outline::open(&mut h.editor);
    h.editor.ui.outline_text = "# Course\n## Intro\n- One\n".to_owned();
    assert!(crate::outline::add(&mut h.editor));
    assert!(!h.editor.ui.show_welcome, "the Start page must close");
    assert!(!h.editor.ui.file_page.open);
    assert!(h.editor.ui.has_loaded_course);
}

/// `N98`: asking for a bottom tab that is current but closed brings it back.
#[test]
fn n98_requesting_a_closed_current_bottom_tab_reopens_it() {
    use crate::dock::{self, Panel};
    let mut h = TestHarness::new(text_course());
    h.idle_frame();
    h.editor.ui.dock.enabled = true;
    dock::request_bottom_tab(&mut h.editor, crate::BottomTab::Question);
    h.idle_frame();
    dock::set_open(&mut h.editor, Panel::Question, false);
    assert!(!dock::is_open(&h.editor, Panel::Question));
    h.idle_frame();
    // The tab names are unchanged, so the per-frame follower alone would do nothing.
    dock::request_bottom_tab(&mut h.editor, crate::BottomTab::Question);
    h.idle_frame();
    assert!(dock::is_open(&h.editor, Panel::Question));
}

/// `N89`: Delete behind Publish, and the Publish shortcut on the Start page, change nothing.
#[test]
fn n89_keys_do_not_leak_through_windows_and_the_start_page() {
    use egui::{Key, Modifiers};
    let mut h = TestHarness::new(text_course());
    h.editor.selection.set_many(["t1".to_owned()]);
    h.idle_frame();
    h.editor.ui.publish.open = true;
    h.press(Key::Delete, Modifiers::NONE);
    h.idle_frame();
    assert!(
        h.editor
            .session
            .course
            .slide("s1")
            .unwrap()
            .object("t1")
            .is_some(),
        "Delete behind Publish must not delete the selection"
    );
    h.editor.ui.publish.open = false;
    h.editor.ui.show_welcome = true;
    h.editor.file_ops_available = true;
    h.press(Key::P, Modifiers::COMMAND | Modifiers::SHIFT);
    h.idle_frame();
    assert!(
        !h.editor.ui.publish.open,
        "Ctrl+Shift+P must not open Publish on the Start page"
    );
}

/// `N94`: the Button Tools controls never open Shape Style, which only styles shapes.
#[test]
fn n94_button_format_controls_do_not_open_shape_style() {
    use crate::ribbon::spec::{Action, UiAction, all_items};
    let fill = all_items()
        .find(|item| item.id == "button_fmt.fill")
        .expect("Button Fill");
    assert!(matches!(fill.action, Action::Ui(UiAction::FormatInspector)));
    let border = all_items()
        .find(|item| item.id == "button_fmt.border")
        .expect("Button Border");
    assert!(matches!(border.action, Action::NotYet(_)));
}

/// `N95`: controls that cannot work in Master View or Edit-State Mode are disabled there.
#[test]
fn n95_controls_are_disabled_where_they_cannot_work() {
    use crate::ribbon::spec::UiAction;
    let mut h = TestHarness::new(text_course());
    h.editor.selection.set_many(["t1".to_owned()]);
    assert!(crate::ribbon::ui_available(&h.editor, UiAction::Table).is_ok());
    assert!(crate::ribbon::ui_available(&h.editor, UiAction::RecordNarration).is_ok());
    h.editor.enter_master_view();
    for action in [
        UiAction::Table,
        UiAction::RecordNarration,
        UiAction::NarrationFromText,
    ] {
        assert!(
            crate::ribbon::ui_available(&h.editor, action).is_err(),
            "{action:?} must be disabled in Master View"
        );
    }
    h.editor.leave_master_view();
    h.editor.selection.set_many(["t1".to_owned()]);
    h.editor.enter_edit_state("s1", "t1", "hover");
    for action in [
        UiAction::Arrange,
        UiAction::RecordNarration,
        UiAction::NarrationFromText,
    ] {
        assert!(
            crate::ribbon::ui_available(&h.editor, action).is_err(),
            "{action:?} must be disabled in Edit-State Mode"
        );
    }
}

/// `N97`: the ribbon's Duplicate copies every selected object.
#[test]
fn n97_ribbon_duplicate_copies_every_selected_object() {
    let course = serde_json::from_value(json!({
        "schema_version": 1, "id": "c", "title": "C",
        "slide_size": { "w": 1280, "h": 720 },
        "scenes": [{ "id": "sc", "title": "S", "slides": [{
            "id": "s1", "title": "One",
            "objects": [
                { "id": "a", "frame": { "x": 10.0, "y": 10.0, "w": 100.0, "h": 50.0 },
                  "kind": { "type": "shape", "shape": "rectangle", "fill": "#336699" } },
                { "id": "b", "frame": { "x": 200.0, "y": 10.0, "w": 100.0, "h": 50.0 },
                  "kind": { "type": "shape", "shape": "rectangle", "fill": "#336699" } }
            ]
        }] }]
    }))
    .expect("course");
    let mut h = TestHarness::new(course);
    h.editor
        .selection
        .set_many(["a".to_owned(), "b".to_owned()]);
    crate::ribbon::run_command(&mut h.editor, "object.duplicate");
    let count = h.editor.session.course.slide("s1").unwrap().objects.len();
    assert_eq!(count, 4, "both selected objects must be duplicated");
}

/// `N99`: a text box that mixes fonts, sizes and colours reports them as mixed.
#[test]
fn n99_mixed_font_size_and_color_read_as_mixed() {
    let course = serde_json::from_value(json!({
        "schema_version": 1, "id": "c", "title": "C",
        "slide_size": { "w": 1280, "h": 720 },
        "scenes": [{ "id": "sc", "title": "S", "slides": [{
            "id": "s1", "title": "One",
            "objects": [{
                "id": "t1", "frame": { "x": 10.0, "y": 10.0, "w": 400.0, "h": 100.0 },
                "kind": { "type": "text", "text": "AABB",
                  "style": { "font": "Inter", "size": 20.0, "color": "#222222" },
                  "runs": [
                    { "text": "AA", "font": "Georgia", "size": 32.0, "color": "#FF0000" },
                    { "text": "BB", "font": "Inter", "size": 12.0, "color": "#0000FF" }
                  ] }
            }]
        }] }]
    }))
    .expect("course");
    let mut h = TestHarness::new(course);
    h.editor.selection.set_many(["t1".to_owned()]);
    let state = crate::text_format::state(&h.editor).expect("text state");
    assert_eq!(state.font, None);
    assert_eq!(state.size, None);
    assert_eq!(state.color, None);
}

/// `N100`: leaving the Numbering "Start at" field untouched changes nothing.
#[test]
fn n100_untouched_start_at_changes_nothing() {
    assert!(crate::ribbon::start_at_change(false, 1).is_none());
    assert_eq!(
        crate::ribbon::start_at_change(true, 5),
        Some(crate::text_format::Change::StartAt(5))
    );
}

/// `N101`: switching the variable or the target object leaves no invalid value behind.
#[test]
fn n101_dependent_trigger_pickers_repair_stale_values() {
    use crate::triggers::{ActionTag, TriggerFormState};
    use coursecraft_course::VarOp;
    let course: Course = serde_json::from_value(json!({
        "schema_version": 1, "id": "c", "title": "C",
        "slide_size": { "w": 1280, "h": 720 },
        "variables": [
            { "name": "flag", "value": true },
            { "name": "word", "value": "hi" }
        ],
        "scenes": [{ "id": "sc", "title": "S", "slides": [{
            "id": "s1", "title": "One",
            "objects": [
                { "id": "a", "frame": { "x": 10.0, "y": 10.0, "w": 100.0, "h": 50.0 },
                  "kind": { "type": "shape", "shape": "rectangle", "fill": "#336699" },
                  "motion_paths": [ { "id": "pa", "points": [ { "x": 0.0, "y": 0.0 }, { "x": 10.0, "y": 10.0 } ], "duration_ms": 1000 } ] },
                { "id": "b", "frame": { "x": 200.0, "y": 10.0, "w": 100.0, "h": 50.0 },
                  "kind": { "type": "shape", "shape": "rectangle", "fill": "#336699" } }
            ]
        }] }]
    }))
    .expect("course");
    let slide = course.slide("s1").unwrap().clone();
    let mut form = TriggerFormState::new_for_owner(&slide, None, &course);
    form.action_tag = ActionTag::SetVariable;
    form.set_var_name = "word".to_owned();
    form.set_var_op = VarOp::Toggle;
    form.action_tag = ActionTag::MoveOnPath;
    form.move_path_object = "b".to_owned();
    form.move_path_id = "pa".to_owned();
    let ctx = egui::Context::default();
    ctx.run_ui(egui::RawInput::default(), |ui| {
        crate::triggers::render_action_fields(&mut form, ui, &slide, &course);
    })
    .drop_without_applying_deltas();
    assert_eq!(form.move_path_id, "", "object b has no path pa");
    form.action_tag = ActionTag::SetVariable;
    ctx.run_ui(egui::RawInput::default(), |ui| {
        crate::triggers::render_action_fields(&mut form, ui, &slide, &course);
    })
    .drop_without_applying_deltas();
    assert_eq!(
        form.set_var_op,
        VarOp::Assign,
        "Toggle does not fit a text variable"
    );
}

/// `N102`: a drag item needs a different object to land on, and Fill-in has no shuffle.
#[test]
fn n102_question_panel_drag_pair_and_fill_in_shuffle() {
    let one = vec!["a".to_owned()];
    assert_eq!(crate::question::new_drag_pair(&one, &one), None);
    let two = vec!["a".to_owned(), "b".to_owned()];
    assert_eq!(
        crate::question::new_drag_pair(&one, &two),
        Some(("a".to_owned(), "b".to_owned()))
    );

    let json = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/hello-course/course.json"
    ))
    .expect("hello course");
    let course = Course::from_json(&json).expect("valid course");
    let mut h = TestHarness::new(course);
    let sid = h.editor.session.course.scenes[0].slides[0].id.clone();
    let _ = h.editor.session.execute(
        "question.insert",
        &json!({ "slide": sid, "kind": "fill_in" }),
    );
    h.editor.selection.slide = sid;
    h.editor.selection.object = None;
    h.idle_frame();
    h.idle_frame();
    let panel = h.right_panel_rect();
    for _ in 0..8 {
        assert!(
            h.find_text_in("Shuffle choices", panel).is_none(),
            "a fill-in question has no choices to shuffle"
        );
        h.scroll_over(panel.center(), -200.0);
    }
}

/// `N103`: discarded Theme dialog drafts do not come back, and N109: its labels use `tl!`.
#[test]
fn n103_theme_dialog_drops_drafts_and_translates_labels() {
    let mut h = TestHarness::new(text_course());
    h.editor.set_language("pseudo");
    h.editor.ui.show_theme_window = true;
    h.idle_frame();
    h.idle_frame();
    for text in ["Colors", "Player chrome", "Reset to defaults", "Close"] {
        assert!(
            h.find_text(&format!("⟦{text}⟧")).is_some(),
            "{text:?} is not drawn through tl! in the Theme dialog"
        );
    }
    let course_accent = h.editor.session.course.theme.accent.0.clone();
    h.editor.ui.theme_dialog.accent = "#ABCDEF".to_owned();
    let close = h.find_text("⟦Close⟧").expect("Close button");
    h.click_at(close);
    h.idle_frame();
    assert!(!h.editor.ui.show_theme_window);
    h.editor.ui.show_theme_window = true;
    h.idle_frame();
    assert_eq!(
        h.editor.ui.theme_dialog.accent, course_accent,
        "the discarded draft must not reappear"
    );
}

/// `N109`: the character spacing preset in use is the one the ribbon highlights.
#[test]
fn n109_text_state_reports_the_character_spacing() {
    let mut h = TestHarness::new(text_course());
    h.editor.selection.set_many(["t1".to_owned()]);
    assert_eq!(
        crate::text_format::state(&h.editor).and_then(|s| s.letter_spacing),
        Some(0.0)
    );
    assert!(h.editor.run(
        "text.style_range",
        json!({ "slide": "s1", "object": "t1", "start": 0, "end": 0, "letter_spacing": 3.0 }),
    )
    .is_some());
    assert_eq!(
        crate::text_format::state(&h.editor).and_then(|s| s.letter_spacing),
        Some(3.0)
    );
}

/// `N109`: the status bar names what the armed Animation Painter copies.
#[test]
fn n109_status_bar_names_the_animation_painter() {
    use crate::format_painter::PainterKind;
    assert!(crate::status::painter_hint(PainterKind::Animation, false).contains("animations"));
    assert!(crate::status::painter_hint(PainterKind::Animation, true).contains("animations"));
    assert!(crate::status::painter_hint(PainterKind::Format, false).contains("format"));
}

/// `N105`: F2 on a question bank slide does not leave a rename that nothing can finish.
#[test]
fn n105_f2_on_a_bank_slide_starts_no_rename() {
    use egui::{Key, Modifiers};
    let json = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/hello-course/course.json"
    ))
    .expect("hello course");
    let course = Course::from_json(&json).expect("valid course");
    let mut h = TestHarness::new(course);
    let bank = h
        .editor
        .run("bank.create", json!({ "title": "Pool" }))
        .expect("bank.create")["id"]
        .as_str()
        .expect("id")
        .to_owned();
    let added = h
        .editor
        .run(
            "bank.add_question",
            json!({ "bank": bank, "kind": "multiple_choice", "prompt": "Q1" }),
        )
        .expect("bank.add_question");
    let slide_id = added["id"].as_str().expect("slide id").to_owned();
    h.editor.selection.slide = slide_id;
    h.editor.ui.last_active_area = crate::ActiveArea::Navigator;
    h.idle_frame();
    h.press(Key::F2, Modifiers::NONE);
    h.idle_frame();
    assert!(
        h.editor.ui.slide_rename.is_none(),
        "no rename field exists for a bank slide"
    );
}

/// `N106`: a recent file's name and path are shown as they are, not looked up.
#[test]
fn n106_recent_courses_are_not_translated() {
    let mut h = TestHarness::new_with_size(text_course(), (1600.0, 900.0));
    h.editor.ui.ribbon.visible = true;
    h.editor.file_ops_available = true;
    h.editor.set_language("pseudo");
    h.editor.recent_files = vec!["C:/courses/Save.ccraft".to_owned()];
    h.editor.ui.file_page.open = true;
    h.editor.ui.file_page.section = crate::file_page::FileSection::Recent;
    h.idle_frame();
    h.idle_frame();
    assert!(
        h.find_text_containing("Save.ccraft").is_some(),
        "the recent file is listed"
    );
    assert!(
        h.find_text_containing("⟦Save.ccraft").is_none()
            && h.find_text_containing("⟦C:/courses").is_none(),
        "a file name and its path never go through the lookup"
    );
}

/// `N108`: a layer eye turned on for one slide is cleared when the slide changes, even with
/// the Slide Layers panel closed.
#[test]
fn n108_layer_eyes_clear_on_slide_change_without_the_layers_panel() {
    let json = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/hello-course/course.json"
    ))
    .expect("hello course");
    let course = Course::from_json(&json).expect("valid course");
    let mut h = TestHarness::new(course);
    let slides: Vec<String> = h
        .editor
        .session
        .course
        .slides()
        .map(|s| s.id.clone())
        .collect();
    assert!(slides.len() >= 2);
    h.editor.ui.bottom_tab = crate::BottomTab::States;
    crate::dock::set_open(&mut h.editor, crate::dock::Panel::Layers, false);
    h.editor.selection.slide = slides[0].clone();
    h.idle_frame();
    h.editor
        .ui
        .layers
        .shown_layers
        .insert("some-layer".to_owned());
    h.editor.selection.slide = slides[1].clone();
    h.idle_frame();
    assert!(
        h.editor.ui.layers.shown_layers.is_empty(),
        "the eyes belong to one slide"
    );
}
