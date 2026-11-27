//! M39 Package P: paste options (destination theme, source formatting, text only, Paste
//! Special) and the Focus Order dialog.

use crate::clipboard::{self, Clipboard, ClipboardKind, PasteMode, SourceTheme};
use crate::focus_order::FocusOrderState;
use crate::ribbon::spec::{Action, UiAction};
use crate::test_harness::TestHarness;
use coursecraft_course::{Course, Object, ObjectKind};
use serde_json::json;

const ACCENT: &str = "#2563EB";
const TEXT: &str = "#111111";
const BACKGROUND: &str = "#FFFFFF";

fn text_object(id: &str, text: &str, font: &str, color: &str) -> serde_json::Value {
    json!({
        "id": id,
        "name": id,
        "kind": {
            "type": "text",
            "text": text,
            "style": { "font": font, "color": color },
            "runs": [{ "text": text, "color": ACCENT, "font": font }]
        },
        "frame": { "x": 100.0, "y": 100.0, "w": 200.0, "h": 60.0 }
    })
}

fn button(id: &str, order: Option<u32>) -> serde_json::Value {
    let mut value = json!({
        "id": id,
        "name": id,
        "kind": { "type": "button", "label": id },
        "frame": { "x": 0.0, "y": 0.0, "w": 100.0, "h": 40.0 }
    });
    if let Some(order) = order {
        value["focus_order"] = json!(order);
    }
    value
}

fn course_with(objects: Vec<serde_json::Value>) -> Course {
    serde_json::from_value(json!({
        "schema_version": 1,
        "id": "m39-p",
        "title": "M39 P",
        "slide_size": { "w": 1280, "h": 720 },
        "theme": {
            "accent": ACCENT, "background": BACKGROUND, "text": TEXT, "font": "Inter"
        },
        "scenes": [{
            "id": "sc",
            "title": "Scene",
            "slides": [
                { "id": "s1", "title": "One", "objects": objects },
                { "id": "s2", "title": "Two" }
            ]
        }]
    }))
    .expect("course")
}

fn harness() -> TestHarness {
    let mut h = TestHarness::new(course_with(vec![
        text_object("t1", "First", "Inter", ACCENT),
        text_object("t2", "Second", "Inter", TEXT),
    ]));
    h.idle_frame();
    h
}

fn copy_both(h: &mut TestHarness) {
    h.editor.selection.slide = "s1".into();
    h.editor
        .selection
        .set_many(["t1".to_owned(), "t2".to_owned()]);
    h.editor.ui.last_active_area = crate::ActiveArea::Canvas;
    h.copy();
}

fn set_destination_theme(h: &mut TestHarness) {
    h.editor
        .run(
            "course.set_theme",
            json!({ "font": "Lato", "accent": "#DC2626", "text": "#222222" }),
        )
        .expect("set theme");
}

fn pasted_on(h: &TestHarness, slide: &str) -> Vec<Object> {
    h.editor
        .session
        .course
        .slide(slide)
        .map(|s| s.objects.clone())
        .unwrap_or_default()
}

fn style_of(object: &Object) -> (String, String, String, String) {
    let ObjectKind::Text { style, runs, .. } = &object.kind else {
        panic!("not a text object");
    };
    let run = runs.first().expect("run");
    (
        style.font.clone(),
        style.color.0.clone(),
        run.font.clone().unwrap_or_default(),
        run.color.as_ref().map(|c| c.0.clone()).unwrap_or_default(),
    )
}

// ---------------------------------------------------------------------------
// 1. The clipboard payload carries the source theme
// ---------------------------------------------------------------------------

#[test]
fn copy_writes_the_source_theme_and_it_round_trips() {
    let mut h = harness();
    copy_both(&mut h);
    let clip = h.editor.ui.clipboard.clone().expect("clipboard");
    let theme = clip.theme.clone().expect("theme in payload");
    assert_eq!(theme.font, "Inter");
    assert_eq!(theme.accent, ACCENT);
    assert_eq!(theme.text, TEXT);
    assert_eq!(theme.background, BACKGROUND);
    let parsed = Clipboard::from_text(&clip.to_text()).expect("parses");
    assert_eq!(parsed.theme, Some(theme));
}

#[test]
fn an_old_payload_without_a_theme_still_pastes() {
    let mut h = harness();
    let payload = json!({
        "coursecraft_clipboard": 1,
        "kind": "objects",
        "source_slide": "s1",
        "objects": [text_object("old", "Old", "Inter", ACCENT)]
    })
    .to_string();
    let parsed = Clipboard::from_text(&payload).expect("old payload parses");
    assert_eq!(parsed.theme, None);
    assert_eq!(parsed.kind, ClipboardKind::Objects);
    h.editor.selection.slide = "s2".into();
    set_destination_theme(&mut h);
    h.paste_text(&payload);
    let objects = pasted_on(&h, "s2");
    assert_eq!(objects.len(), 1);
    // No source theme: nothing is mapped, the object pastes as copied.
    let (font, color, ..) = style_of(&objects[0]);
    assert_eq!(font, "Inter");
    assert_eq!(color, ACCENT);
}

#[test]
fn a_broken_theme_field_does_not_block_the_paste() {
    let payload = json!({
        "coursecraft_clipboard": 1,
        "kind": "objects",
        "theme": { "font": 7 },
        "objects": [text_object("o", "x", "Inter", ACCENT)]
    })
    .to_string();
    let parsed = Clipboard::from_text(&payload).expect("parses");
    assert_eq!(parsed.theme, None);
}

// ---------------------------------------------------------------------------
// 2. Use Destination Theme
// ---------------------------------------------------------------------------

#[test]
fn default_paste_follows_the_destination_theme() {
    let mut h = harness();
    copy_both(&mut h);
    set_destination_theme(&mut h);
    h.editor.selection.slide = "s2".into();
    let before = h.editor.session.undo_len();
    h.paste_text(&h.editor.ui.clipboard_text.clone().expect("text"));
    assert_eq!(h.editor.session.undo_len(), before + 1, "one undo step");
    let objects = pasted_on(&h, "s2");
    assert_eq!(objects.len(), 2);
    let (font, color, run_font, run_color) = style_of(&objects[0]);
    assert_eq!(
        font, "Lato",
        "the source theme font becomes the destination font"
    );
    assert_eq!(
        color, "#DC2626",
        "the source accent becomes the destination accent"
    );
    assert_eq!(run_font, "Lato");
    assert_eq!(run_color, "#DC2626");
    let (_, color2, ..) = style_of(&objects[1]);
    assert_eq!(
        color2, "#222222",
        "the source text colour becomes the destination's"
    );
}

#[test]
fn destination_theme_leaves_other_values_alone() {
    let mut h = harness();
    h.editor.selection.slide = "s1".into();
    h.editor.selection.set_many(["t1".to_owned()]);
    h.editor.ui.last_active_area = crate::ActiveArea::Canvas;
    let mut clip = clipboard::copy_objects(&h.editor).expect("copy").0;
    // A font and a colour that are not the source theme's stay as they are,
    // and a text that happens to look like a colour is never touched.
    clip.objects = vec![
        serde_json::from_value(text_object("x", ACCENT, "Georgia", "#00FF00")).expect("object"),
    ];
    set_destination_theme(&mut h);
    h.editor.selection.slide = "s2".into();
    clipboard::paste_with(&mut h.editor, clip, PasteMode::DestinationTheme);
    let objects = pasted_on(&h, "s2");
    let ObjectKind::Text { text, .. } = &objects[0].kind else {
        panic!("text");
    };
    assert_eq!(text, ACCENT, "author text is not a colour");
    let (font, color, run_font, run_color) = style_of(&objects[0]);
    assert_eq!(font, "Georgia");
    assert_eq!(color, "#00FF00");
    assert_eq!(run_font, "Georgia");
    assert_eq!(run_color, "#DC2626", "the run colour was the source accent");
}

#[test]
fn equal_themes_change_nothing() {
    let h = harness();
    let theme = SourceTheme::of(&h.editor.session.course);
    let objects: Vec<Object> =
        vec![serde_json::from_value(text_object("a", "A", "Inter", ACCENT)).expect("object")];
    assert_eq!(
        clipboard::adapt_to_theme(&objects, Some(&theme), &theme),
        objects
    );
    assert_eq!(clipboard::adapt_to_theme(&objects, None, &theme), objects);
}

#[test]
fn a_colour_keeps_its_alpha_when_mapped() {
    let source = SourceTheme {
        font: "Inter".into(),
        heading_font: None,
        accent: ACCENT.into(),
        text: TEXT.into(),
        background: BACKGROUND.into(),
    };
    let dest = SourceTheme {
        accent: "#DC2626".into(),
        ..source.clone()
    };
    let objects: Vec<Object> =
        vec![serde_json::from_value(text_object("a", "A", "Inter", "#2563EB80")).expect("object")];
    let mapped = clipboard::adapt_to_theme(&objects, Some(&source), &dest);
    let (_, color, ..) = style_of(&mapped[0]);
    assert_eq!(color, "#DC262680");
}

#[test]
fn a_heading_font_maps_to_the_body_font_when_the_destination_has_no_heading_font() {
    let source = SourceTheme {
        font: "Inter".into(),
        heading_font: Some("Lora".into()),
        accent: ACCENT.into(),
        text: TEXT.into(),
        background: BACKGROUND.into(),
    };
    let dest = SourceTheme {
        font: "Roboto".into(),
        heading_font: None,
        ..source.clone()
    };
    let objects: Vec<Object> =
        vec![serde_json::from_value(text_object("a", "A", "Lora", ACCENT)).expect("object")];
    let mapped = clipboard::adapt_to_theme(&objects, Some(&source), &dest);
    let (font, _, run_font, _) = style_of(&mapped[0]);
    assert_eq!(font, "Roboto");
    assert_eq!(run_font, "Roboto");
}

// ---------------------------------------------------------------------------
// 3. Keep Source Formatting
// ---------------------------------------------------------------------------

#[test]
fn keep_source_formatting_pastes_exactly_as_copied() {
    let mut h = harness();
    copy_both(&mut h);
    set_destination_theme(&mut h);
    h.editor.selection.slide = "s2".into();
    let before = h.editor.session.undo_len();
    clipboard::paste_private_with(&mut h.editor, PasteMode::KeepSource);
    assert_eq!(h.editor.session.undo_len(), before + 1);
    let objects = pasted_on(&h, "s2");
    let (font, color, run_font, run_color) = style_of(&objects[0]);
    assert_eq!(font, "Inter");
    assert_eq!(color, ACCENT);
    assert_eq!(run_font, "Inter");
    assert_eq!(run_color, ACCENT);
}

#[test]
fn the_menu_choice_waits_for_the_system_clipboard() {
    let mut h = harness();
    copy_both(&mut h);
    set_destination_theme(&mut h);
    h.editor.selection.slide = "s2".into();
    let ctx = h.ctx.clone();
    clipboard::paste_now_with(&mut h.editor, &ctx, PasteMode::KeepSource);
    assert_eq!(
        h.editor.ui.pending_paste_mode,
        Some(PasteMode::KeepSource),
        "desktop: the mode waits for the paste event"
    );
    h.paste_text(&h.editor.ui.clipboard_text.clone().expect("text"));
    assert_eq!(h.editor.ui.pending_paste_mode, None);
    let (font, ..) = style_of(&pasted_on(&h, "s2")[0]);
    assert_eq!(font, "Inter");
    // The next plain Ctrl+V is back to the destination theme.
    h.editor.selection.slide = "s2".into();
    h.paste_text(&h.editor.ui.clipboard_text.clone().expect("text"));
    let objects = pasted_on(&h, "s2");
    let (font, ..) = style_of(&objects[objects.len() - 1]);
    assert_eq!(font, "Lato");
}

// ---------------------------------------------------------------------------
// 4. Keep Text Only
// ---------------------------------------------------------------------------

#[test]
fn keep_text_only_makes_one_plain_text_box_in_the_body_font() {
    let mut h = harness();
    copy_both(&mut h);
    set_destination_theme(&mut h);
    h.editor.selection.slide = "s2".into();
    let before = h.editor.session.undo_len();
    clipboard::paste_private_with(&mut h.editor, PasteMode::TextOnly);
    assert_eq!(h.editor.session.undo_len(), before + 1, "one undo step");
    let objects = pasted_on(&h, "s2");
    assert_eq!(objects.len(), 1, "one new text box");
    let ObjectKind::Text {
        text, style, runs, ..
    } = &objects[0].kind
    else {
        panic!("text");
    };
    assert_eq!(text, "First\nSecond", "one paragraph per copied object");
    assert!(runs.is_empty(), "plain: no run formatting");
    assert_eq!(style.font, "Lato", "the theme body font");
    assert_eq!(
        h.editor.selection.object.as_deref(),
        Some(objects[0].id.as_str())
    );
    h.editor.session.undo().expect("undo");
    assert!(pasted_on(&h, "s2").is_empty());
}

#[test]
fn keep_text_only_of_objects_without_text_says_so() {
    let mut h = TestHarness::new(course_with(vec![button("b1", None)]));
    h.idle_frame();
    h.editor.selection.slide = "s1".into();
    h.editor.selection.set_many(["b1".to_owned()]);
    h.editor.ui.last_active_area = crate::ActiveArea::Canvas;
    h.copy();
    h.editor.selection.slide = "s2".into();
    let before = h.editor.session.undo_len();
    clipboard::paste_private_with(&mut h.editor, PasteMode::TextOnly);
    assert_eq!(h.editor.session.undo_len(), before, "nothing was pasted");
    assert!(pasted_on(&h, "s2").is_empty());
}

#[test]
fn an_unanswered_menu_choice_does_not_reach_a_later_ctrl_v() {
    let mut h = harness();
    copy_both(&mut h);
    set_destination_theme(&mut h);
    h.editor.selection.slide = "s2".into();
    let ctx = h.ctx.clone();
    // Keep Text Only, but the system clipboard never answers (no text on it).
    clipboard::paste_now_with(&mut h.editor, &ctx, PasteMode::TextOnly);
    for _ in 0..4 {
        h.idle_frame();
    }
    assert_eq!(
        h.editor.ui.pending_paste_mode, None,
        "the stale mode expired"
    );
    h.paste_text(&h.editor.ui.clipboard_text.clone().expect("text"));
    let (font, ..) = style_of(&pasted_on(&h, "s2")[0]);
    assert_eq!(font, "Lato", "objects paste with the destination theme");
}

#[test]
fn the_main_paste_button_clears_an_older_menu_mode() {
    let mut h = harness();
    h.editor.ui.pending_paste_mode = Some(PasteMode::TextOnly);
    let ctx = h.ctx.clone();
    crate::menu::paste_now(&mut h.editor, &ctx);
    assert_eq!(h.editor.ui.pending_paste_mode, None);
}

#[test]
fn keep_text_only_of_plain_os_text_is_the_normal_plain_paste() {
    let mut h = harness();
    h.editor.selection.slide = "s2".into();
    h.editor.ui.pending_paste_mode = Some(PasteMode::TextOnly);
    h.paste_text("Just words");
    let objects = pasted_on(&h, "s2");
    assert_eq!(objects.len(), 1);
    let ObjectKind::Text { text, .. } = &objects[0].kind else {
        panic!("text");
    };
    assert_eq!(text, "Just words");
}

// ---------------------------------------------------------------------------
// 5. Paste Special
// ---------------------------------------------------------------------------

#[test]
fn paste_menu_items_are_wired() {
    let action_of = |id: &str| {
        crate::ribbon::spec::all_items()
            .find(|item| item.id == id)
            .unwrap_or_else(|| panic!("{id} in the Paste menu"))
            .action
    };
    assert_eq!(action_of("paste.destination"), Action::Ui(UiAction::Paste));
    assert_eq!(
        action_of("paste.keep_source"),
        Action::Ui(UiAction::PasteKeepSource)
    );
    assert_eq!(
        action_of("paste.text_only"),
        Action::Ui(UiAction::PasteTextOnly)
    );
    assert_eq!(
        action_of("paste.special"),
        Action::Ui(UiAction::PasteSpecial)
    );
    assert_eq!(
        action_of("slide.focus_order"),
        Action::Ui(UiAction::FocusOrder)
    );
}

#[test]
fn paste_special_disables_formats_that_are_not_on_the_clipboard() {
    let mut h = TestHarness::new_with_config(
        course_with(vec![text_object("t1", "First", "Inter", ACCENT)]),
        (1280.0, 900.0),
        coursecraft_render::ThemeMode::Dark,
        1.0,
        false,
    );
    h.idle_frame();
    let formats = clipboard::paste_formats(&h.editor);
    assert!(formats.iter().all(|(_, available)| available.is_err()));
    h.editor.selection.slide = "s1".into();
    h.editor.selection.set_many(["t1".to_owned()]);
    h.editor.ui.last_active_area = crate::ActiveArea::Canvas;
    h.copy();
    let formats = clipboard::paste_formats(&h.editor);
    assert!(formats[0].1.is_ok(), "objects");
    assert!(formats[1].1.is_err(), "slides");
    assert_eq!(
        formats.len(),
        2,
        "plain text is a formatting choice, not a format"
    );
}

#[test]
fn the_paste_menu_marks_use_destination_theme_as_the_default() {
    assert!(crate::ribbon::is_default_entry("paste.destination"));
    assert!(!crate::ribbon::is_default_entry("paste.keep_source"));
    assert!(!crate::ribbon::is_default_entry("paste.text_only"));
}

#[test]
fn paste_special_lists_text_only_once() {
    let mut h = harness();
    copy_both(&mut h);
    h.editor.open_paste_special();
    h.idle_frame();
    h.idle_frame();
    assert!(h.find_text("Plain text").is_none());
    assert!(h.find_text("Keep Text Only").is_some());
}

#[test]
fn paste_special_window_pastes_the_chosen_option_and_blocks_global_keys() {
    let mut h = harness();
    copy_both(&mut h);
    set_destination_theme(&mut h);
    h.editor.selection.slide = "s2".into();
    h.editor.open_paste_special();
    assert!(h.editor.ui.paste_special.open);
    assert_eq!(
        h.editor.ui.paste_special.format,
        clipboard::PasteFormat::Objects
    );
    h.idle_frame();
    h.idle_frame();
    assert!(
        crate::dialog::any_open(&h.ctx),
        "the window counts as a dialog"
    );
    assert!(h.editor.modal_dialog_open());
    assert!(h.find_text("Paste Special").is_some());
    assert!(h.find_text("Keep Source Formatting").is_some());
    h.editor.ui.paste_special.mode = PasteMode::KeepSource;
    let ok = h.find_text("OK").expect("OK button");
    h.click_at(ok);
    h.idle_frame();
    assert!(!h.editor.ui.paste_special.open, "OK closes the window");
    assert_eq!(h.editor.ui.pending_paste_mode, Some(PasteMode::KeepSource));
    h.paste_text(&h.editor.ui.clipboard_text.clone().expect("text"));
    let (font, ..) = style_of(&pasted_on(&h, "s2")[0]);
    assert_eq!(font, "Inter", "Keep Source Formatting was chosen");
}

#[test]
fn escape_closes_paste_special() {
    let mut h = harness();
    copy_both(&mut h);
    h.editor.open_paste_special();
    h.idle_frame();
    h.idle_frame();
    h.press(egui::Key::Escape, egui::Modifiers::NONE);
    h.idle_frame();
    assert!(!h.editor.ui.paste_special.open);
}

// ---------------------------------------------------------------------------
// 6. Focus Order
// ---------------------------------------------------------------------------

fn focus_harness() -> TestHarness {
    let mut h = TestHarness::new(course_with(vec![
        button("a", None),
        button("b", Some(1)),
        button("c", None),
        json!({
            "id": "plain",
            "kind": { "type": "shape", "shape": "rectangle", "fill": "#000000" },
            "frame": { "x": 0.0, "y": 0.0, "w": 10.0, "h": 10.0 }
        }),
    ]));
    h.editor.selection.slide = "s1".into();
    h.idle_frame();
    h
}

fn order_of(h: &TestHarness) -> Vec<(String, Option<u32>)> {
    let slide = h.editor.session.course.slide("s1").expect("slide");
    slide
        .objects
        .iter()
        .map(|o| (o.id.clone(), o.focus_order))
        .collect()
}

fn ids(h: &TestHarness) -> Vec<String> {
    h.editor
        .ui
        .focus_order
        .items
        .iter()
        .map(|item| item.id.clone())
        .collect()
}

#[test]
fn focus_order_lists_what_can_take_focus_in_the_current_order() {
    let mut h = focus_harness();
    h.editor.open_focus_order();
    assert!(h.editor.ui.focus_order.open);
    assert_eq!(
        ids(&h),
        ["b", "a", "c"],
        "author order first, then reading order"
    );
    assert!(!h.editor.ui.focus_order.dirty);
}

#[test]
fn focus_order_move_up_down_and_drag_reorder() {
    let mut h = focus_harness();
    h.editor.open_focus_order();
    let state: &mut FocusOrderState = &mut h.editor.ui.focus_order;
    assert!(state.move_down(0));
    assert_eq!(
        state
            .items
            .iter()
            .map(|i| i.id.as_str())
            .collect::<Vec<_>>(),
        ["a", "b", "c"]
    );
    assert!(state.move_up(2));
    assert_eq!(
        state
            .items
            .iter()
            .map(|i| i.id.as_str())
            .collect::<Vec<_>>(),
        ["a", "c", "b"]
    );
    assert!(!state.move_up(0), "the first row cannot move up");
    assert!(!state.move_down(2), "the last row cannot move down");
    assert!(state.move_to(2, 0), "a drag moves a row to another place");
    assert_eq!(
        state
            .items
            .iter()
            .map(|i| i.id.as_str())
            .collect::<Vec<_>>(),
        ["b", "a", "c"]
    );
    assert!(state.dirty);
}

#[test]
fn focus_order_ok_writes_one_to_n_in_one_undo_step() {
    let mut h = focus_harness();
    h.editor.open_focus_order();
    h.editor.ui.focus_order.move_to(2, 0);
    assert_eq!(ids(&h), ["c", "b", "a"]);
    let before = h.editor.session.undo_len();
    assert!(h.editor.apply_focus_order(), "{:?}", h.editor.ui.last_error);
    assert_eq!(h.editor.session.undo_len(), before + 1, "one undo step");
    assert_eq!(
        order_of(&h),
        [
            ("a".to_owned(), Some(3)),
            ("b".to_owned(), Some(2)),
            ("c".to_owned(), Some(1)),
            ("plain".to_owned(), None),
        ]
    );
    // The shared order logic now agrees with what the dialog wrote.
    let slide = h.editor.session.course.slide("s1").expect("slide");
    let got: Vec<String> = coursecraft_render::focus_order_candidates(slide)
        .into_iter()
        .map(|c| c.id)
        .collect();
    assert_eq!(got, ["c", "b", "a"]);
    h.editor.session.undo().expect("undo");
    assert_eq!(
        order_of(&h),
        [
            ("a".to_owned(), None),
            ("b".to_owned(), Some(1)),
            ("c".to_owned(), None),
            ("plain".to_owned(), None),
        ]
    );
}

#[test]
fn focus_order_reset_clears_every_number() {
    let mut h = focus_harness();
    h.editor.open_focus_order();
    let slide = h.editor.session.course.slide("s1").cloned().expect("slide");
    h.editor.ui.focus_order.reset_to_default(&slide);
    assert_eq!(ids(&h), ["a", "b", "c"], "reading order, no author numbers");
    let before = h.editor.session.undo_len();
    assert!(h.editor.apply_focus_order());
    assert_eq!(h.editor.session.undo_len(), before + 1);
    assert!(order_of(&h).iter().all(|(_, order)| order.is_none()));
}

#[test]
fn focus_order_handles_interactive_groups_and_layers() {
    let mut course = course_with(vec![
        button("a", None),
        {
            let mut m1 = button("m1", None);
            m1["group"] = json!("g1");
            m1
        },
        {
            let mut m2 = button("m2", None);
            m2["group"] = json!("g1");
            m2
        },
    ]);
    let mut value = serde_json::to_value(&course).expect("json");
    value["scenes"][0]["slides"][0]["groups"] =
        json!([{ "id": "g1", "name": "Group", "interactive": true }]);
    value["scenes"][0]["slides"][0]["layers"] =
        json!([{ "id": "l1", "name": "Layer", "objects": [button("lb", None)] }]);
    course = serde_json::from_value(value).expect("course");
    let mut h = TestHarness::new(course);
    h.editor.selection.slide = "s1".into();
    h.idle_frame();
    h.editor.open_focus_order();
    assert_eq!(
        ids(&h),
        ["a", "g1", "lb"],
        "the group stands for its members"
    );
    let kinds: Vec<bool> = h
        .editor
        .ui
        .focus_order
        .items
        .iter()
        .map(|item| item.is_group)
        .collect();
    assert_eq!(kinds, [false, true, false]);
    assert!(
        h.editor.ui.focus_order.items[2].label.contains("Layer"),
        "a layer object shows its layer"
    );
    h.editor.ui.focus_order.move_to(1, 0);
    assert!(h.editor.apply_focus_order());
    let slide = h.editor.session.course.slide("s1").expect("slide");
    assert_eq!(slide.group("g1").and_then(|(g, _)| g.focus_order), Some(1));
    assert_eq!(slide.object("a").and_then(|(o, _)| o.focus_order), Some(2));
    assert_eq!(slide.object("lb").and_then(|(o, _)| o.focus_order), Some(3));
}

#[test]
fn focus_order_window_draws_and_ok_needs_a_change() {
    let mut h = focus_harness();
    h.editor.open_focus_order();
    h.idle_frame();
    h.idle_frame();
    assert!(crate::dialog::any_open(&h.ctx));
    assert!(h.editor.modal_dialog_open());
    assert!(h.find_text("Focus Order").is_some());
    assert!(h.find_text("Move Up").is_some());
    assert!(h.find_text("Move Down").is_some());
    assert!(h.find_text("Reset to default order").is_some());
    // OK is disabled until something changes: pressing it writes nothing.
    let before = h.editor.session.undo_len();
    let ok = h.find_text("OK").expect("OK");
    h.click_at(ok);
    h.idle_frame();
    assert_eq!(h.editor.session.undo_len(), before);
    assert!(h.editor.ui.focus_order.open);
    // After a change, OK writes and closes.
    h.editor.ui.focus_order.move_down(0);
    h.idle_frame();
    let ok = h.find_text("OK").expect("OK");
    h.click_at(ok);
    h.idle_frame();
    assert_eq!(h.editor.session.undo_len(), before + 1);
    assert!(!h.editor.ui.focus_order.open);
}

#[test]
fn escape_closes_focus_order_without_writing() {
    let mut h = focus_harness();
    h.editor.open_focus_order();
    h.idle_frame();
    h.idle_frame();
    h.editor.ui.focus_order.move_down(0);
    let before = h.editor.session.undo_len();
    h.press(egui::Key::Escape, egui::Modifiers::NONE);
    h.idle_frame();
    assert!(!h.editor.ui.focus_order.open);
    assert_eq!(h.editor.session.undo_len(), before);
}

#[test]
fn focus_order_is_not_available_in_masters_or_without_a_slide() {
    let mut h = focus_harness();
    h.editor.ui.master_view = true;
    h.editor.open_focus_order();
    assert!(!h.editor.ui.focus_order.open);
    h.editor.ui.master_view = false;
    h.editor.selection.slide = "missing".into();
    h.editor.open_focus_order();
    assert!(!h.editor.ui.focus_order.open);
}

#[test]
fn clipboard_text_round_trip_keeps_slides_payloads_working() {
    let mut h = harness();
    h.editor.selection.slide = "s1".into();
    h.editor.ui.last_active_area = crate::ActiveArea::Navigator;
    h.copy();
    let clip = h.editor.ui.clipboard.clone().expect("slide clipboard");
    assert_eq!(clip.kind, ClipboardKind::Slides);
    assert_eq!(clip.theme, None);
    let before = h.editor.session.course.slides().count();
    h.paste_text(&clip.to_text());
    assert_eq!(h.editor.session.course.slides().count(), before + 1);
}
