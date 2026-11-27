//! M39 package T on the ribbon: Clear Formatting, Text Shadow, Change Case, Highlight,
//! Numbering ▾ and Text Direction ▾, and the Text panel rows.

use crate::ribbon::spec::{self, Action, TextOp};
use crate::test_harness::TestHarness;
use crate::text_format::{self, Change};
use coursecraft_course::{CaseChange, Course, ListKind, NumberScheme, ObjectKind, TextDirection};

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

struct Title {
    text: String,
    runs: Vec<coursecraft_course::TextRun>,
    style: coursecraft_course::TextStyle,
    paragraphs: Vec<coursecraft_course::Paragraph>,
}

fn title(h: &TestHarness) -> Title {
    let slide = h.editor.session.course.slide("s-intro").expect("slide");
    let (obj, _) = slide.object("title").expect("title");
    match &obj.kind {
        ObjectKind::Text {
            text,
            runs,
            style,
            paragraphs,
            ..
        } => Title {
            text: text.clone(),
            runs: runs.clone(),
            style: style.clone(),
            paragraphs: paragraphs.clone(),
        },
        _ => panic!("title is text"),
    }
}

fn undo(h: &mut TestHarness) {
    h.editor.run("edit.undo", serde_json::json!({}));
}

fn state(h: &TestHarness) -> text_format::TextState {
    text_format::state(&h.editor).unwrap_or_default()
}

fn edit_title(h: &mut TestHarness, buffer: &str, range: Option<(usize, usize)>, caret: usize) {
    h.editor.ui.inline_edit = Some(crate::canvas::InlineEdit {
        slide: "s-intro".into(),
        object: "title".into(),
        buffer: buffer.into(),
        original: buffer.into(),
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
fn the_font_and_paragraph_controls_of_package_t_are_wired() {
    let home = spec::tab("home").expect("home tab");
    let find = |id: &str| {
        home.groups
            .iter()
            .flat_map(|g| g.items.iter())
            .find(|i| i.id == id)
            .unwrap_or_else(|| panic!("{id}"))
    };
    assert_eq!(
        find("font.clear").action,
        Action::Text(TextOp::ClearFormatting)
    );
    assert_eq!(find("font.shadow").action, Action::Text(TextOp::Shadow));
    assert_eq!(find("font.case").action, Action::Text(TextOp::Case));
    assert_eq!(
        find("font.highlight").action,
        Action::Text(TextOp::Highlight)
    );
    assert_eq!(
        find("paragraph.numbering").action,
        Action::Text(TextOp::Numbering)
    );
    assert!(find("paragraph.numbering").kind.is_split());
    assert_eq!(
        find("paragraph.direction").action,
        Action::Text(TextOp::Direction)
    );
    for id in [
        "font.clear",
        "font.shadow",
        "font.case",
        "font.highlight",
        "paragraph.numbering",
        "paragraph.direction",
    ] {
        assert!(
            !matches!(find(id).action, Action::NotYet(_)),
            "{id} is still disabled"
        );
    }
    // Numbering toggles on the main part; the menu sits behind the ▾.
    assert!(TextOp::Numbering.has_menu() && !TextOp::Numbering.main_opens());
    assert!(TextOp::Case.main_opens());
    // Highlight and Font Color apply their bar colour on the main part; the menu is behind the ▾.
    assert!(TextOp::Highlight.has_menu() && !TextOp::Highlight.main_opens());
    assert!(TextOp::Color.has_menu() && !TextOp::Color.main_opens());
}

#[test]
fn shadow_toggles_on_and_off_in_one_undo_step_each() {
    let mut h = harness();
    assert_eq!(state(&h).shadow, Some(false));
    let before = h.editor.session.revision;
    assert!(text_format::apply(&mut h.editor, &Change::Shadow));
    assert!(h.editor.session.revision > before);
    assert_eq!(state(&h).shadow, Some(true), "shows on");
    assert!(title(&h).runs.iter().all(|r| r.shadow == Some(true)));
    assert!(text_format::apply(&mut h.editor, &Change::Shadow));
    assert_eq!(state(&h).shadow, Some(false));
    assert!(title(&h).runs.iter().all(|r| r.shadow.is_none()));
    undo(&mut h);
    assert_eq!(state(&h).shadow, Some(true), "one step undone");
}

#[test]
fn shadow_on_part_of_the_text_shows_mixed() {
    let mut h = harness();
    edit_title(&mut h, "Workplace Safety Basics", Some((0, 9)), 0);
    assert!(text_format::apply(&mut h.editor, &Change::Shadow));
    let runs = title(&h).runs;
    assert_eq!(runs[0].text, "Workplace");
    assert_eq!(runs[0].shadow, Some(true));
    // The whole text now has a mix.
    h.editor.ui.inline_edit = None;
    h.editor.ui.inline_edit_selection = None;
    h.idle_frame();
    assert_eq!(state(&h).shadow, None, "mixed shows as not on");
    assert!(
        !crate::ribbon::availability(&h.editor, Action::Text(TextOp::Shadow), None).is_err(),
        "still enabled"
    );
}

#[test]
fn highlight_sets_a_colour_and_no_color_clears_it() {
    let mut h = harness();
    assert!(text_format::apply(
        &mut h.editor,
        &Change::Highlight(Some("#FFFF00".into()))
    ));
    assert_eq!(state(&h).highlight.as_deref(), Some("#FFFF00"));
    assert!(
        title(&h)
            .runs
            .iter()
            .all(|r| r.highlight.as_ref().map(|c| c.0.as_str()) == Some("#FFFF00"))
    );
    assert!(text_format::apply(&mut h.editor, &Change::Highlight(None)));
    assert_eq!(state(&h).highlight, None);
    assert!(title(&h).runs.iter().all(|r| r.highlight.is_none()));
    undo(&mut h);
    assert_eq!(state(&h).highlight.as_deref(), Some("#FFFF00"));
}

#[test]
fn a_highlight_picked_at_the_caret_waits_for_typing() {
    let mut h = harness();
    edit_title(&mut h, "Workplace Safety Basics", None, 4);
    let before = h.editor.session.revision;
    assert!(text_format::apply(
        &mut h.editor,
        &Change::Highlight(Some("#00FF00".into()))
    ));
    assert_eq!(h.editor.session.revision, before, "nothing run yet");
    let pending = h
        .editor
        .ui
        .properties
        .pending_caret_style
        .as_ref()
        .expect("pending");
    assert_eq!(
        pending
            .patch
            .highlight
            .as_ref()
            .and_then(|c| c.as_ref())
            .map(|c| c.0.as_str()),
        Some("#00FF00")
    );
    assert_eq!(state(&h).highlight.as_deref(), Some("#00FF00"));
}

#[test]
fn clear_formatting_removes_run_formatting_in_one_undo_step() {
    let mut h = harness();
    assert!(text_format::apply(&mut h.editor, &Change::Italic));
    assert!(text_format::apply(&mut h.editor, &Change::Shadow));
    assert!(text_format::apply(
        &mut h.editor,
        &Change::Highlight(Some("#FFFF00".into()))
    ));
    assert!(!title(&h).runs.is_empty());
    let before = h.editor.session.revision;
    assert!(text_format::apply(&mut h.editor, &Change::ClearFormatting));
    assert_eq!(h.editor.session.revision, before + 1);
    assert!(title(&h).runs.is_empty(), "plain text again");
    undo(&mut h);
    assert!(!title(&h).runs.is_empty(), "one step brings it back");
}

#[test]
fn clear_formatting_of_a_selection_leaves_the_rest() {
    let mut h = harness();
    assert!(text_format::apply(&mut h.editor, &Change::Italic));
    edit_title(&mut h, "Workplace Safety Basics", Some((0, 9)), 0);
    assert!(text_format::apply(&mut h.editor, &Change::ClearFormatting));
    let runs = title(&h).runs;
    assert_eq!(runs[0].text, "Workplace");
    assert!(runs[0].italic.is_none());
    assert_eq!(runs[1].italic, Some(true));
}

#[test]
fn change_case_changes_the_whole_text_or_the_selection() {
    let mut h = harness();
    assert!(text_format::apply(
        &mut h.editor,
        &Change::Case(CaseChange::Upper)
    ));
    assert_eq!(title(&h).text, "WORKPLACE SAFETY BASICS");
    undo(&mut h);
    assert_eq!(title(&h).text, "Workplace Safety Basics");

    edit_title(&mut h, "Workplace Safety Basics", Some((10, 16)), 10);
    assert!(text_format::apply(
        &mut h.editor,
        &Change::Case(CaseChange::Lower)
    ));
    assert_eq!(title(&h).text, "Workplace safety Basics");
    assert_eq!(
        h.editor.ui.inline_edit.as_ref().map(|e| e.buffer.as_str()),
        Some("Workplace safety Basics"),
        "the edit buffer follows the new letters"
    );
}

#[test]
fn change_case_at_a_caret_does_nothing() {
    let mut h = harness();
    edit_title(&mut h, "Workplace Safety Basics", None, 4);
    let before = h.editor.session.revision;
    assert!(!text_format::apply(
        &mut h.editor,
        &Change::Case(CaseChange::Upper)
    ));
    assert_eq!(h.editor.session.revision, before);
}

#[test]
fn numbering_schemes_and_start_at_apply_to_the_paragraphs() {
    let mut h = harness();
    assert!(text_format::apply(
        &mut h.editor,
        &Change::NumberScheme(NumberScheme::RomanUcPeriod)
    ));
    let t = title(&h);
    assert_eq!(t.paragraphs[0].list, ListKind::Number);
    assert_eq!(
        t.paragraphs[0].number_scheme,
        Some(NumberScheme::RomanUcPeriod)
    );
    assert_eq!(state(&h).number_scheme, Some(NumberScheme::RomanUcPeriod));
    assert!(text_format::apply(&mut h.editor, &Change::StartAt(4)));
    assert_eq!(title(&h).paragraphs[0].start_at, Some(4));
    assert_eq!(state(&h).start_at, Some(4));
    undo(&mut h);
    assert_eq!(title(&h).paragraphs[0].start_at, None);
}

#[test]
fn the_numbering_main_button_still_toggles_the_list() {
    let mut h = harness();
    let at = h
        .find_icon_last(coursecraft_render::Icon::ListNumbers)
        .expect("the Numbering button");
    h.click_at(at);
    assert_eq!(title(&h).paragraphs[0].list, ListKind::Number);
    h.click_at(at);
    assert!(
        title(&h)
            .paragraphs
            .first()
            .is_none_or(|p| p.list == ListKind::None)
    );
}

#[test]
fn the_numbering_arrow_lists_the_seven_schemes_and_start_at() {
    let mut h = harness();
    let at = h
        .find_icon_last(coursecraft_render::Icon::ListNumbers)
        .expect("the Numbering button");
    // The ▾ strip is right of the icon.
    h.click_at(egui::pos2(at.x + 18.0, at.y));
    h.idle_frame();
    for (_, sample) in crate::ribbon::NUMBER_SCHEMES {
        assert!(h.find_text(sample).is_some(), "{sample} is listed");
    }
    assert!(h.find_text("Start at").is_some());
    let roman = h.find_text("I. II. III.").expect("roman upper");
    h.click_at(roman);
    let t = title(&h);
    assert_eq!(t.paragraphs[0].list, ListKind::Number);
    assert_eq!(
        t.paragraphs[0].number_scheme,
        Some(NumberScheme::RomanUcPeriod)
    );
}

#[test]
fn text_direction_menu_offers_horizontal_and_two_rotations() {
    let mut h = harness();
    let label = h.find_text("Text Direction").expect("the control");
    h.click_at(label);
    h.idle_frame();
    assert!(h.find_text("Horizontal").is_some());
    assert!(h.find_text("Rotate all text 270°").is_some());
    assert!(h.find_text("Stacked").is_none(), "stacked is not offered");
    let ninety = h.find_text("Rotate all text 90°").expect("90°");
    h.click_at(ninety);
    assert_eq!(title(&h).style.direction, TextDirection::Vertical);
    assert_eq!(state(&h).direction, Some(TextDirection::Vertical));
    assert!(text_format::apply(
        &mut h.editor,
        &Change::Direction(TextDirection::Vertical270)
    ));
    assert_eq!(title(&h).style.direction, TextDirection::Vertical270);
    assert!(text_format::apply(
        &mut h.editor,
        &Change::Direction(TextDirection::Horizontal)
    ));
    assert_eq!(title(&h).style.direction, TextDirection::Horizontal);
}

#[test]
fn the_case_menu_lists_the_five_choices() {
    let mut h = harness();
    let at = h
        .find_icon_last(coursecraft_render::Icon::TextAa)
        .expect("the Change Case button");
    h.click_at(at);
    h.idle_frame();
    for label in [
        "Sentence case",
        "lowercase",
        "UPPERCASE",
        "Capitalize Each Word",
        "tOGGLE cASE",
    ] {
        assert!(h.find_text(label).is_some(), "{label}");
    }
    let upper = h.find_text("UPPERCASE").expect("upper");
    h.click_at(upper);
    assert_eq!(title(&h).text, "WORKPLACE SAFETY BASICS");
}

#[test]
fn the_highlight_menu_offers_colours_and_no_color() {
    let mut h = harness();
    let arrow = h
        .last_widgets
        .iter()
        .find(|w| w.label.as_deref() == Some("Highlight options"))
        .map(|w| w.bounds.center())
        .expect("the Highlight ▾");
    h.click_at(arrow);
    h.idle_frame();
    assert!(
        h.find_text("Highlight colours").is_some(),
        "the ▾ opens the Highlight menu"
    );
    assert!(h.find_text("Theme colours").is_some());
    assert!(h.find_text("No Color").is_some());
    let none = h.find_text("No Color").expect("No Color");
    h.click_at(none);
    assert_eq!(state(&h).highlight, None);
}

#[test]
fn the_text_panel_has_shadow_and_highlight_rows_and_no_disabled_highlight() {
    let mut h = harness();
    crate::panels::open_at(
        &mut h.editor,
        crate::dock::Panel::Text,
        Some(crate::panels::Section::Font),
    );
    h.idle_frame();
    h.idle_frame();
    assert!(h.find_text_containing("Highlight").is_some());
    assert!(h.find_text("Sh").is_some(), "the shadow toggle");
    assert!(h.find_text("No Color").is_some(), "the highlight reset");
    assert!(
        h.find_text_containing("text highlight colour").is_none(),
        "the Not available yet row is gone"
    );
}
