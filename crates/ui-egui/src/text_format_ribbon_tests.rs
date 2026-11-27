//! Home › Font and Home › Paragraph on the ribbon, through `text_format`.

use crate::ribbon::spec::{self, Action, TextOp};
use crate::test_harness::TestHarness;
use crate::text_format::{self, Change, Scope};
use coursecraft_course::{Align, Course, ListKind, ObjectKind};

const HELLO_COURSE: &str = include_str!("../../../examples/hello-course/course.json");

fn harness() -> TestHarness {
    let course = Course::from_json(HELLO_COURSE).expect("hello course parses");
    let mut h = TestHarness::new_with_size(course, (1600.0, 900.0));
    h.editor.ui.ribbon.visible = true;
    h.editor.selection = crate::Selection::new("s-intro");
    h.editor.selection.object = Some("title".into());
    h.idle_frame();
    h
}

fn title(
    h: &TestHarness,
) -> (
    coursecraft_course::TextStyle,
    Vec<coursecraft_course::TextRun>,
) {
    let slide = h.editor.session.course.slide("s-intro").expect("slide");
    let (obj, _) = slide.object("title").expect("title");
    match &obj.kind {
        ObjectKind::Text { style, runs, .. } => (style.clone(), runs.clone()),
        _ => panic!("title is text"),
    }
}

fn edit_title(h: &mut TestHarness, buffer: &str, range: Option<(usize, usize)>, caret: usize) {
    let original = "Workplace Safety Basics".to_owned();
    h.editor.ui.inline_edit = Some(crate::canvas::InlineEdit {
        slide: "s-intro".into(),
        object: "title".into(),
        buffer: buffer.into(),
        original,
        is_button: false,
        is_shape: false,
        cell: None,
        just_entered: false,
        live_runs: Vec::new(),
        live_paragraphs: Vec::new(),
        last_revision: 0,
        state_name: None,
        extra_id: None,
    });
    h.editor.ui.inline_edit_selection = Some(("s-intro".into(), "title".into(), range, caret));
}

#[test]
fn every_font_and_paragraph_control_with_a_feature_is_wired() {
    let home = spec::tab("home").expect("home tab");
    for id in ["font", "paragraph"] {
        let group = home.groups.iter().find(|g| g.id == id).expect("group");
        for item in group.items {
            assert!(
                !matches!(item.action, Action::NotYet(spec::WIRING_FORMAT)),
                "{} still waits for wiring",
                item.id
            );
        }
    }
    let bold = home
        .groups
        .iter()
        .flat_map(|g| g.items.iter())
        .find(|i| i.id == "font.bold")
        .expect("bold");
    assert_eq!(bold.action, Action::Text(TextOp::Bold));
}

#[test]
fn a_selected_text_box_is_formatted_whole_in_one_undo_step() {
    let mut h = harness();
    assert_eq!(
        text_format::targets(&h.editor).map(|t| t[0].scope),
        Ok(Scope::Whole)
    );
    assert_eq!(
        text_format::state(&h.editor).and_then(|s| s.bold),
        Some(true)
    );
    assert!(text_format::apply(&mut h.editor, &Change::Bold));
    let now = |h: &TestHarness| text_format::state(&h.editor).unwrap_or_default();
    assert_eq!(now(&h).bold, Some(false), "bold toggles off");
    assert!(text_format::apply(&mut h.editor, &Change::Grow));
    assert_eq!(now(&h).size, Some(54.0), "48 grows to 54");
    assert!(text_format::apply(
        &mut h.editor,
        &Change::Align(Align::Center)
    ));
    assert_eq!(title(&h).0.align, Align::Center);
    h.editor.run("edit.undo", serde_json::json!({}));
    assert_eq!(title(&h).0.align, Align::Left, "one step undone");
    assert_eq!(now(&h).size, Some(54.0));
}

#[test]
fn the_ribbon_shows_the_state_and_says_why_when_it_cannot_act() {
    let mut h = harness();
    assert_eq!(
        h.editor.ui.ribbon.text.as_ref().map(|s| s.bold),
        Ok(Some(true))
    );
    h.editor.selection = crate::Selection::new("s-intro");
    h.idle_frame();
    let reason = crate::ribbon::availability(&h.editor, Action::Text(TextOp::Bold), None);
    assert_eq!(
        reason,
        Err("Select a text box, or a shape that holds text".to_owned())
    );
    h.editor.selection.object = Some("btn-start".into());
    h.idle_frame();
    assert!(
        crate::ribbon::availability(&h.editor, Action::Text(TextOp::Bold), None).is_err(),
        "a button has no text box"
    );
}

#[test]
fn selected_characters_being_edited_are_formatted_alone() {
    let mut h = harness();
    edit_title(&mut h, "Workplace Safety Basics", Some((0, 9)), 0);
    assert!(text_format::apply(&mut h.editor, &Change::Italic));
    let (style, runs) = title(&h);
    assert!(!style.italic, "the object's style is unchanged");
    let first = runs.first().expect("runs");
    assert_eq!(first.text, "Workplace");
    assert_eq!(first.italic, Some(true));
    assert!(runs.iter().skip(1).all(|r| r.italic != Some(true)));
}

#[test]
fn typed_text_is_committed_with_the_change() {
    let mut h = harness();
    edit_title(&mut h, "Safety", Some((0, 6)), 0);
    assert!(text_format::apply(&mut h.editor, &Change::Underline));
    let (_, runs) = title(&h);
    let text: String = runs.iter().map(|r| r.text.as_str()).collect();
    assert_eq!(text, "Safety");
    assert!(runs.iter().all(|r| r.underline == Some(true)));
    assert_eq!(
        h.editor
            .ui
            .inline_edit
            .as_ref()
            .map(|e| e.original.as_str()),
        Some("Safety")
    );
}

#[test]
fn a_style_picked_at_the_caret_waits_for_typing() {
    let mut h = harness();
    edit_title(&mut h, "Workplace Safety Basics", None, 4);
    let before = h.editor.session.revision;
    assert!(text_format::apply(&mut h.editor, &Change::Strikethrough));
    assert_eq!(h.editor.session.revision, before, "nothing run yet");
    let pending = h
        .editor
        .ui
        .properties
        .pending_caret_style
        .as_ref()
        .expect("pending");
    assert_eq!(pending.cursor, 4);
    assert_eq!(pending.patch.strikethrough, Some(true));
    assert_eq!(
        text_format::state(&h.editor).and_then(|s| s.strikethrough),
        Some(true),
        "the toggle shows on"
    );
}

#[test]
fn bullets_toggle_and_indent_steps_on_the_paragraphs() {
    let mut h = harness();
    assert!(text_format::apply(
        &mut h.editor,
        &Change::List(ListKind::Bullet)
    ));
    assert_eq!(
        text_format::state(&h.editor).and_then(|s| s.list),
        Some(ListKind::Bullet)
    );
    assert!(text_format::apply(&mut h.editor, &Change::Indent(1)));
    assert_eq!(text_format::state(&h.editor).and_then(|s| s.level), Some(1));
    assert!(text_format::apply(
        &mut h.editor,
        &Change::List(ListKind::Bullet)
    ));
    assert_eq!(
        text_format::state(&h.editor).and_then(|s| s.list),
        Some(ListKind::None),
        "a second click turns bullets off"
    );
}

#[test]
fn several_selected_text_boxes_change_together() {
    let mut h = harness();
    // A second text box on the same slide.
    h.editor
        .run(
            "object.add",
            serde_json::json!({
                "id": "note",
                "slide": "s-intro",
                "kind": { "type": "text", "text": "Note" },
                "frame": { "x": 80, "y": 300, "w": 400, "h": 60 }
            }),
        )
        .expect("added");
    h.editor.selection.set_many(["title", "note"]);
    h.idle_frame();
    assert_eq!(text_format::targets(&h.editor).map(|t| t.len()), Ok(2));
    assert!(text_format::apply(
        &mut h.editor,
        &Change::Font("Georgia".into())
    ));
    assert_eq!(
        text_format::state(&h.editor).and_then(|s| s.font),
        Some("Georgia".to_owned())
    );
}

#[test]
fn clicking_bold_on_the_ribbon_keeps_the_text_being_edited_open() {
    let mut h = harness();
    edit_title(&mut h, "Workplace Safety Basics", Some((10, 16)), 10);
    h.idle_frame();
    h.editor.ui.inline_edit_selection =
        Some(("s-intro".into(), "title".into(), Some((10, 16)), 10));
    let bold = h
        .find_icon(coursecraft_render::Icon::TextB)
        .expect("the ribbon's Bold");
    assert!(h.editor.ui.ribbon.rect.contains(bold));
    h.click_at(bold);
    assert!(h.editor.ui.inline_edit.is_some(), "still editing");
}

#[test]
fn the_ribbon_font_box_opens_the_inspectors_font_picker() {
    let mut h = harness();
    let font = text_format::state(&h.editor)
        .and_then(|s| s.font)
        .expect("the title has one font");
    let ribbon = h.editor.ui.ribbon.rect;
    let combo = h
        .find_text_in(&crate::font_picker::displayed_font(&font), ribbon)
        .expect("the Font box");
    h.click_at(combo);
    h.idle_frame();
    assert!(
        h.find_text("Search fonts").is_some(),
        "the picker's search field"
    );
    assert!(
        h.find_text("Add font…").is_some(),
        "the picker's Add font… button"
    );
}

#[test]
fn the_ribbon_font_color_opens_the_full_colour_picker() {
    let mut h = harness();
    // The main part applies the colour in its bar; the ▾ opens the picker.
    let arrow = h
        .last_widgets
        .iter()
        .find(|w| w.label.as_deref() == Some("Font Color options"))
        .map(|w| w.bounds.center())
        .expect("the ribbon's Font Color ▾");
    h.click_at(arrow);
    h.idle_frame();
    assert!(
        h.find_text_containing("Eyedropper").is_some(),
        "the picker's eyedropper"
    );
    assert!(h.find_text("Theme").is_some(), "the theme swatches");
}

#[test]
fn ribbon_font_box_shows_inter_for_the_legacy_default_font() {
    let mut h = harness();
    let slide = h.editor.session.course.slide("s-intro").expect("slide");
    let (obj, _) = slide.object("title").expect("title");
    let ObjectKind::Text { style, .. } = &obj.kind else {
        panic!("title is text");
    };
    assert!(
        style.font.eq_ignore_ascii_case("Open Sans"),
        "the hello course title uses the legacy default font"
    );
    h.idle_frame();
    assert_eq!(
        crate::ribbon::combo_text(&h.editor, Action::Text(TextOp::Font)),
        "Inter",
        "the ribbon Font box agrees with the inspector"
    );
}
