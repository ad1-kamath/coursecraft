//! UX fixes found by hand in 0.6.1, group A: theme font windows, font lists, Symbol window,
//! Focus Order, Design › Fonts, inspector tabs and the ribbon colour bars.

use crate::test_harness::TestHarness;
use crate::theme_dialog::open_theme_fonts_dialog;
use coursecraft_course::Course;
use egui::{Key, Modifiers, Popup};

fn course() -> Course {
    Course::from_json(include_str!("../../../examples/hello-course/course.json"))
        .expect("hello course")
}

/// A harness with the Theme window and its Create New Theme Fonts child open.
fn theme_with_fonts_child() -> TestHarness {
    let mut h = TestHarness::new_with_size(course(), (1456.0, 820.0));
    h.editor.ui.show_theme_window = true;
    h.idle_frame();
    let button = h.find_text("Theme fonts…").expect("Theme fonts… button");
    h.click_at(button);
    h.idle_frame();
    h.idle_frame();
    assert!(h.editor.ui.theme_dialog.fonts_open, "the child is open");
    h
}

/// Open the Heading font list of the child window.
fn open_heading_list(h: &mut TestHarness) -> egui::Pos2 {
    let label = h.find_text("Heading font").expect("Heading font row");
    let combo = egui::pos2(label.x + 220.0, label.y);
    h.click_at(combo);
    h.idle_frame();
    assert!(Popup::is_any_open(&h.ctx), "the font list is open");
    combo
}

#[test]
fn escape_closes_only_the_font_list_then_the_child_then_the_theme_window() {
    let mut h = theme_with_fonts_child();
    open_heading_list(&mut h);

    h.press(Key::Escape, Modifiers::NONE);
    h.idle_frame();
    assert!(!Popup::is_any_open(&h.ctx), "Escape closes the list");
    assert!(h.editor.ui.show_theme_window, "the Theme window stays");
    assert!(h.editor.ui.theme_dialog.fonts_open, "the child stays");

    h.press(Key::Escape, Modifiers::NONE);
    h.idle_frame();
    assert!(!h.editor.ui.theme_dialog.fonts_open, "the child closes");
    assert!(h.editor.ui.show_theme_window, "the Theme window stays");

    h.press(Key::Escape, Modifiers::NONE);
    h.idle_frame();
    assert!(!h.editor.ui.show_theme_window, "the Theme window closes");
}

#[test]
fn closing_the_theme_window_closes_the_fonts_window_it_opened() {
    let mut h = theme_with_fonts_child();
    h.editor.ui.show_theme_window = false;
    h.idle_frame();
    assert!(
        !h.editor.ui.theme_dialog.fonts_open,
        "the child never outlives its parent"
    );
}

#[test]
fn a_fonts_window_opened_from_the_design_tab_does_not_need_the_theme_window() {
    let mut h = TestHarness::new_with_size(course(), (1456.0, 820.0));
    open_theme_fonts_dialog(&mut h.editor);
    h.idle_frame();
    h.idle_frame();
    assert!(h.editor.ui.theme_dialog.fonts_open);
    assert!(!h.editor.ui.show_theme_window);
}

#[test]
fn the_theme_font_list_opens_under_its_combo_box() {
    let mut h = theme_with_fonts_child();
    let combo = open_heading_list(&mut h);
    let search = h.find_text("Search fonts").expect("the list's search");
    assert!(
        (search.x - combo.x).abs() < 400.0 && search.y > combo.y && search.y < combo.y + 120.0,
        "the list sits just below the combo: combo {combo:?}, list {search:?}"
    );
}

#[test]
fn the_symbol_window_keeps_its_top_edge_when_its_height_changes() {
    let mut h = TestHarness::new_with_size(course(), (1456.0, 820.0));
    h.editor.ui.insert_text.symbol_open = true;
    h.idle_frame();
    h.idle_frame();
    let top = |h: &TestHarness| {
        h.editor
            .ui
            .insert_text
            .symbol_rect
            .expect("the window")
            .min
            .y
    };
    let before = top(&h);
    // A shorter group, then a recently used row: both change the content height.
    h.editor.ui.insert_text.symbol_group = 3;
    h.idle_frame();
    h.idle_frame();
    assert!(
        (top(&h) - before).abs() < 0.5,
        "tab switch moved the window"
    );
    h.editor.ui.insert_text.recent.push('©');
    h.idle_frame();
    h.idle_frame();
    assert!(
        (top(&h) - before).abs() < 0.5,
        "Recently used moved the window"
    );
}

fn button(id: &str, order: Option<u32>) -> serde_json::Value {
    let mut value = serde_json::json!({
        "id": id,
        "name": id,
        "kind": { "type": "button", "label": id },
        "frame": { "x": 0.0, "y": 0.0, "w": 100.0, "h": 40.0 }
    });
    if let Some(order) = order {
        value["focus_order"] = serde_json::json!(order);
    }
    value
}

/// Slide s1: base buttons a and b, layer About with button lb, and a second slide s2.
fn layered_course(lb_order: Option<u32>) -> Course {
    serde_json::from_value(serde_json::json!({
        "schema_version": 1,
        "id": "ux-a8",
        "title": "UX A8",
        "slide_size": { "w": 1280, "h": 720 },
        "scenes": [{
            "id": "sc",
            "title": "Scene",
            "slides": [
                {
                    "id": "s1",
                    "title": "One",
                    "objects": [button("a", None), button("b", None)],
                    "layers": [{ "id": "l1", "name": "About", "objects": [button("lb", lb_order)] }]
                },
                { "id": "s2", "title": "Two", "objects": [button("c", None)] }
            ]
        }]
    }))
    .expect("course")
}

fn focus_ids(h: &TestHarness) -> Vec<String> {
    h.editor
        .ui
        .focus_order
        .items
        .iter()
        .map(|i| i.id.clone())
        .collect()
}

#[test]
fn focus_order_lists_the_base_layer_first_and_keeps_layers_in_their_own_section() {
    // The layer button has an author number that sorts it first in the player's list.
    let mut h = TestHarness::new(layered_course(Some(1)));
    h.editor.selection.slide = "s1".into();
    h.idle_frame();
    h.editor.open_focus_order();
    assert_eq!(focus_ids(&h), ["a", "b", "lb"], "base first, then About");
    let state = &mut h.editor.ui.focus_order;
    assert!(
        !state.move_to(2, 0),
        "a layer item never joins the base list"
    );
    assert!(!state.move_to(0, 2), "a base item never joins a layer");
    assert!(!state.can_move_up(2) && !state.can_move_down(1));
    assert!(state.can_move_down(0) && state.can_move_up(1));
    assert!(state.move_down(0));
    assert_eq!(focus_ids(&h), ["b", "a", "lb"]);
    assert!(h.editor.apply_focus_order());
    let slide = h.editor.session.course.slide("s1").expect("slide");
    let order = |id: &str| slide.object(id).and_then(|(o, _)| o.focus_order);
    assert_eq!(
        (order("b"), order("a"), order("lb")),
        (Some(1), Some(2), Some(3))
    );
}

#[test]
fn focus_order_window_draws_a_heading_for_the_base_and_each_layer() {
    let mut h = TestHarness::new(layered_course(None));
    h.editor.selection.slide = "s1".into();
    h.idle_frame();
    h.editor.open_focus_order();
    h.idle_frame();
    h.idle_frame();
    assert!(h.find_text("Base layer").is_some());
    assert!(h.find_text("About").is_some());
}

#[test]
fn focus_order_closes_when_the_selected_slide_changes() {
    let mut h = TestHarness::new(layered_course(None));
    h.editor.selection.slide = "s1".into();
    h.idle_frame();
    h.editor.open_focus_order();
    h.idle_frame();
    assert!(h.editor.ui.focus_order.open);
    h.editor.selection = crate::Selection::new("s2");
    h.idle_frame();
    assert!(!h.editor.ui.focus_order.open, "the list belonged to s1");
}

#[test]
fn focus_order_closes_when_the_course_changes_under_it() {
    let mut h = TestHarness::new(layered_course(None));
    h.editor.selection.slide = "s1".into();
    h.idle_frame();
    h.editor.open_focus_order();
    h.editor.ui.focus_order.move_down(0);
    h.idle_frame();
    assert!(h.editor.ui.focus_order.open);
    h.editor
        .run(
            "object.delete",
            serde_json::json!({ "slide": "s1", "object": "b" }),
        )
        .expect("delete b");
    h.idle_frame();
    assert!(
        !h.editor.ui.focus_order.open,
        "OK would fail as a whole on the deleted row"
    );
    assert!(
        h.editor
            .session
            .course
            .slide("s1")
            .is_some_and(|s| s.object("a").is_some()),
        "nothing else was written"
    );
}

#[test]
fn design_fonts_main_button_opens_the_theme_fonts_window_directly() {
    let item = crate::ribbon::spec::all_items()
        .find(|i| i.id == "design.fonts")
        .expect("design.fonts");
    assert_eq!(
        item.action,
        crate::ribbon::spec::Action::Ui(crate::ribbon::spec::UiAction::ThemeFonts)
    );
    let mut h = TestHarness::new_with_size(course(), (1600.0, 900.0));
    h.editor.ui.ribbon.visible = true;
    h.editor.ui.ribbon.tab = "design";
    h.idle_frame();
    h.idle_frame();
    let fonts = h
        .find_icon(coursecraft_render::Icon::TextAa)
        .expect("the Fonts button");
    h.click_at(fonts);
    h.idle_frame();
    h.idle_frame();
    assert!(
        h.editor.ui.theme_dialog.fonts_open,
        "the font settings open directly"
    );
    assert!(
        !h.editor.ui.show_theme_window,
        "not the whole Theme window with Colours first"
    );
}

/// A harness with the inspector column `column` points wide and Shape Style opened.
fn narrow_inspector(column: f32) -> TestHarness {
    use craft_ui::docking::Node;
    use craft_ui::layout::SplitSize;
    let mut h = TestHarness::new_with_size(course(), (1456.0, 820.0));
    h.editor.ui.dock.enabled = true;
    let mut layout = crate::dock::default_layout();
    if let Some(Node::Split { second, .. }) = layout.root.as_mut()
        && let Node::Split { size, .. } = second.as_mut()
    {
        *size = SplitSize::FixedSecond(column);
    }
    h.editor.ui.dock.workspace = craft_ui::docking::Workspace::new(layout);
    crate::panels::open_at(&mut h.editor, crate::dock::Panel::ShapeStyle, None);
    h.idle_frame();
    h.idle_frame();
    h
}

/// The texts of `shapes` that were cut with an ellipsis.
fn elided_texts(shape: &egui::Shape, out: &mut Vec<String>) {
    match shape {
        egui::Shape::Text(t) if t.galley.elided => out.push(t.galley.text().to_owned()),
        egui::Shape::Vec(v) => v.iter().for_each(|s| elided_texts(s, out)),
        _ => {}
    }
}

#[test]
fn inspector_tab_names_are_never_cut_to_three_letters() {
    // Tabs that do not fit at their natural width shrink to a common width. A name cut to
    // "Ani…" is unreadable, so the tabs that do not fit go to the overflow menu instead.
    for column in (200..=420).step_by(10) {
        let mut h = narrow_inspector(column as f32);
        let mut cut = Vec::new();
        for clipped in h.frame_shapes(Vec::new()) {
            elided_texts(&clipped.shape, &mut cut);
        }
        cut.retain(|t| ["Format", "Animate", "Slide", "Course"].contains(&t.as_str()));
        assert!(cut.is_empty(), "column {column}: tab names cut: {cut:?}");
    }
}

/// The hello course with the title selected and the ribbon open.
fn ribbon_harness() -> TestHarness {
    let mut h = TestHarness::new_with_size(course(), (1600.0, 900.0));
    h.editor.ui.ribbon.visible = true;
    h.editor.selection = crate::Selection::new("s-intro");
    h.editor.selection.object = Some("title".into());
    h.idle_frame();
    h.idle_frame();
    h
}

/// The thin filled bars of one colour painted in the ribbon.
fn colour_bars(h: &mut TestHarness, color: egui::Color32) -> Vec<egui::Rect> {
    fn walk(shape: &egui::Shape, color: egui::Color32, out: &mut Vec<egui::Rect>) {
        match shape {
            egui::Shape::Rect(r) if r.fill == color && r.rect.height() <= 4.0 => out.push(r.rect),
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, color, out)),
            _ => {}
        }
    }
    let ribbon = h.editor.ui.ribbon.rect;
    let mut found = Vec::new();
    for clipped in h.frame_shapes(Vec::new()) {
        walk(&clipped.shape, color, &mut found);
    }
    found.retain(|r| ribbon.contains(r.center()));
    found
}

#[test]
fn font_color_and_highlight_show_a_colour_bar_under_the_icon() {
    let mut h = ribbon_harness();
    // Highlight starts yellow; Font Color starts at the theme accent.
    assert_eq!(
        colour_bars(&mut h, egui::Color32::from_rgb(255, 255, 0)).len(),
        1
    );
    let accent = coursecraft_render::parse_color(&h.editor.session.course.theme.accent)
        .expect("accent colour");
    assert_eq!(colour_bars(&mut h, accent).len(), 1);
    // The bar follows the last colour picked.
    h.editor.ui.ribbon.last_highlight = Some("#FF00FF".to_owned());
    h.idle_frame();
    assert_eq!(
        colour_bars(&mut h, egui::Color32::from_rgb(255, 0, 255)).len(),
        1
    );
    assert!(colour_bars(&mut h, egui::Color32::from_rgb(255, 255, 0)).is_empty());
}

#[test]
fn clicking_the_main_part_applies_the_bar_colour_and_the_arrow_opens_the_list() {
    let mut h = ribbon_harness();
    let widget = |h: &TestHarness, name: &str| {
        h.last_widgets
            .iter()
            .find(|w| w.label.as_deref() == Some(name))
            .map(|w| w.bounds.center())
            .unwrap_or_else(|| panic!("no {name} control"))
    };
    let brush = widget(&h, "Highlight");
    h.click_at(brush);
    h.idle_frame();
    assert_eq!(
        crate::text_format::state(&h.editor).and_then(|s| s.highlight),
        Some("#FFFF00".to_owned()),
        "the main part applies the colour in the bar"
    );
    assert!(
        h.find_text("No Color").is_none(),
        "the main part opens no list"
    );
    // The ▾ strip lists the colours.
    let arrow = widget(&h, "Highlight options");
    h.click_at(arrow);
    h.idle_frame();
    assert!(
        h.find_text("No Color").is_some(),
        "the arrow opens the list"
    );
}

#[test]
fn scale_base_is_kept_only_while_the_size_is_the_one_it_was_taken_for() {
    use crate::size_panel::scale_base;
    let stored = ("a".to_owned(), 100.0, 50.0, 100.0, 50.0);
    assert_eq!(scale_base(Some(&stored), "a", (100.0, 50.0)), (100.0, 50.0));
    // A drag on the canvas or an undo changed the size: the base is the new size.
    assert_eq!(
        scale_base(Some(&stored), "a", (200.0, 100.0)),
        (200.0, 100.0)
    );
    // Another object, or nothing stored.
    assert_eq!(scale_base(Some(&stored), "b", (100.0, 50.0)), (100.0, 50.0));
    assert_eq!(scale_base(None, "a", (30.0, 20.0)), (30.0, 20.0));
    // The Scale field set 200 x 100 itself and kept the base: still 100 x 50.
    let after_scale = ("a".to_owned(), 100.0, 50.0, 200.0, 100.0);
    assert_eq!(
        scale_base(Some(&after_scale), "a", (200.0, 100.0)),
        (100.0, 50.0)
    );
}

#[test]
fn reselecting_an_object_resized_elsewhere_reads_one_hundred_percent() {
    let mut h = TestHarness::new_with_size(course(), (1600.0, 900.0));
    h.editor.selection = crate::Selection::new("s-intro");
    h.editor.selection.object = Some("title".into());
    crate::panels::open_at(
        &mut h.editor,
        crate::dock::Panel::SizePosition,
        Some(crate::panels::Section::Size),
    );
    h.idle_frame();
    h.idle_frame();
    let width = |h: &TestHarness| {
        h.editor
            .session
            .course
            .slide("s-intro")
            .and_then(|s| s.object("title"))
            .map(|(o, _)| o.frame)
            .expect("title")
    };
    let frame = width(&h);
    let widened = coursecraft_course::Rect {
        w: frame.w * 2.0,
        ..frame
    };
    h.editor
        .run(
            "object.set_frame",
            serde_json::json!({ "slide": "s-intro", "object": "title", "frame": widened }),
        )
        .expect("resize");
    h.idle_frame();
    h.editor.selection.object = None;
    h.idle_frame();
    h.editor.selection.object = Some("title".into());
    h.idle_frame();
    h.idle_frame();
    let texts = h.layout_report(1).texts;
    assert!(
        !texts.iter().any(|t| t == "200"),
        "Scale width must not read 200 % against the old base: {texts:?}"
    );
}

/// The hello course with the title and the Start button selected.
fn two_selected() -> TestHarness {
    let mut h = TestHarness::new_with_size(course(), (1600.0, 900.0));
    h.editor.selection = crate::Selection::new("s-intro");
    h.editor.selection.object = Some("title".into());
    h.editor.selection.toggle("btn-start");
    h.idle_frame();
    h.idle_frame();
    assert_eq!(h.editor.selection.ids().len(), 2);
    h
}

fn object_of(h: &TestHarness, id: &str) -> coursecraft_course::Object {
    h.editor
        .session
        .course
        .slide("s-intro")
        .and_then(|s| s.object(id))
        .map(|(o, _)| o.clone())
        .expect("object")
}

#[test]
fn a_gallery_entrance_on_several_objects_is_one_undo_step() {
    use crate::ribbon::gallery;
    use crate::ribbon::spec::Gallery;
    let mut h = two_selected();
    let before = h.editor.session.undo_len();
    gallery::apply(&mut h.editor, Gallery::Entrance, 1);
    assert!(
        object_of(&h, "title").entrance.is_some(),
        "{:?}",
        h.editor.ui.last_error
    );
    assert!(object_of(&h, "btn-start").entrance.is_some());
    assert_eq!(h.editor.session.undo_len(), before + 1, "one step");
    h.editor.run("edit.undo", serde_json::json!({}));
    assert!(object_of(&h, "title").entrance.is_none(), "one Undo");
    assert!(
        object_of(&h, "btn-start").entrance.is_none(),
        "reverts both"
    );
}

#[test]
fn a_gallery_emphasis_on_several_objects_is_one_undo_step() {
    use crate::ribbon::gallery;
    use crate::ribbon::spec::Gallery;
    let mut h = two_selected();
    let before = h.editor.session.undo_len();
    gallery::apply(&mut h.editor, Gallery::Emphasis, 0);
    assert_eq!(object_of(&h, "title").emphasis.len(), 1);
    assert_eq!(object_of(&h, "btn-start").emphasis.len(), 1);
    assert_eq!(h.editor.session.undo_len(), before + 1, "one step");
}

#[test]
fn changing_the_sequence_of_several_objects_is_one_undo_step() {
    use crate::ribbon::gallery;
    use crate::ribbon::spec::Gallery;
    use crate::ribbon::spec::Motion;
    use coursecraft_course::Sequence;
    let mut h = two_selected();
    gallery::apply(&mut h.editor, Gallery::Entrance, 1);
    let before = h.editor.session.undo_len();
    gallery::set_sequence(&mut h.editor, Motion::Entrance, Sequence::ByParagraph);
    let sequence = |h: &TestHarness, id: &str| object_of(h, id).entrance.map(|a| a.sequence);
    assert_eq!(sequence(&h, "title"), Some(Sequence::ByParagraph));
    assert_eq!(sequence(&h, "btn-start"), Some(Sequence::ByParagraph));
    assert_eq!(h.editor.session.undo_len(), before + 1, "one step");
}

#[test]
fn picking_another_gallery_effect_keeps_the_sequence() {
    use crate::ribbon::gallery;
    use crate::ribbon::spec::Gallery;
    use crate::ribbon::spec::Motion;
    use coursecraft_course::Sequence;
    let mut h = two_selected();
    gallery::apply(&mut h.editor, Gallery::Entrance, 1);
    gallery::set_sequence(&mut h.editor, Motion::Entrance, Sequence::ByLetter);
    let first = object_of(&h, "title").entrance.expect("entrance").effect.id;
    gallery::apply(&mut h.editor, Gallery::Entrance, 2);
    let after = object_of(&h, "title").entrance.expect("entrance");
    assert_ne!(after.effect.id, first, "the effect changed");
    assert_eq!(after.sequence, Sequence::ByLetter, "Sequence is kept");
    // Exit uses the same rule.
    gallery::apply(&mut h.editor, Gallery::Exit, 1);
    gallery::set_sequence(&mut h.editor, Motion::Exit, Sequence::ByParagraph);
    gallery::apply(&mut h.editor, Gallery::Exit, 2);
    let exit = object_of(&h, "title").exit.expect("exit");
    assert_eq!(exit.sequence, Sequence::ByParagraph);
}
