//! Step s1 regression tests: ribbon split buttons two-part layout and hover (issue #17).

use crate::test_harness::TestHarness;
use coursecraft_course::Course;
use egui::{Event, Key, Modifiers, Rect, Shape};

const HELLO_COURSE: &str = include_str!("../../../examples/hello-course/course.json");

fn harness_with_ribbon() -> TestHarness {
    let course = Course::from_json(HELLO_COURSE).expect("hello course parses");
    let mut h = TestHarness::new_with_size(course, (1600.0, 900.0));
    h.editor.ui.ribbon.visible = true;
    h.idle_frame();
    h
}

fn find_widget_bounds(h: &TestHarness, label: &str) -> Option<Rect> {
    h.last_widgets
        .iter()
        .find(|w| w.label.as_deref() == Some(label))
        .map(|w| w.bounds)
}

fn find_ribbon_widget_bounds(h: &TestHarness, label: &str) -> Option<Rect> {
    let top_h = h.top_bars_height();
    h.last_widgets
        .iter()
        .find(|w| w.label.as_deref() == Some(label) && w.bounds.center().y <= top_h)
        .map(|w| w.bounds)
}

fn select_first_object(h: &mut TestHarness) {
    let slide_id = h.editor.selection.slide.clone();
    let obj_id = h
        .editor
        .session
        .course
        .slide(&slide_id)
        .and_then(|s| s.objects.first())
        .map(|o| o.id.clone())
        .expect("hello course has an object on the first slide");
    h.editor.selection.set_many([obj_id]);
    h.idle_frame();
}

#[test]
fn m48_s1_01_preview_bottom_half_opens_menu() {
    let mut h = harness_with_ribbon();
    let label_pos = h.find_text_last("Preview").expect("Preview label");
    let arrow_rect = find_ribbon_widget_bounds(&h, "Preview options").expect("Preview options");
    assert!(
        arrow_rect.contains(label_pos),
        "Preview label {label_pos:?} should be inside Preview options {arrow_rect:?}"
    );
    h.click_at(label_pos);
    h.idle_frame();
    assert!(
        h.editor.preview.is_none(),
        "clicking Preview label must open menu, not start preview"
    );
    assert!(
        h.find_text("Preview this slide").is_some(),
        "Preview menu should show 'Preview this slide'"
    );
    assert!(
        h.find_text("Preview whole course").is_some(),
        "Preview menu should show 'Preview whole course'"
    );
}

#[test]
fn m48_s1_02_preview_top_half_runs_preview() {
    let mut h = harness_with_ribbon();
    let top_rect = find_ribbon_widget_bounds(&h, "Preview").expect("Preview top half");
    h.click_at(top_rect.center());
    h.idle_frame();
    assert!(
        h.editor.preview.is_some(),
        "clicking Preview top half should start preview"
    );
}

#[test]
fn m48_s1_03_other_large_split_two_parts() {
    let mut h = harness_with_ribbon();
    let before = h.editor.session.course.slides().count();

    let label_pos = h.find_text("New\nSlide").expect("New Slide label");
    let bot_rect = find_ribbon_widget_bounds(&h, "New Slide options").expect("New Slide options");
    assert!(
        bot_rect.contains(label_pos),
        "New Slide label {label_pos:?} should be inside bottom half {bot_rect:?}"
    );
    h.click_at(label_pos);
    h.idle_frame();
    assert_eq!(
        h.editor.session.course.slides().count(),
        before,
        "clicking New Slide bottom half must open menu without adding a slide"
    );
    assert!(
        h.find_text("Blank").is_some(),
        "New Slide menu should be open"
    );

    h.press(Key::Escape, Modifiers::NONE);
    h.idle_frame();

    let top_rect = find_ribbon_widget_bounds(&h, "New Slide").expect("New Slide top half");
    h.click_at(top_rect.center());
    h.idle_frame();
    assert_eq!(
        h.editor.session.course.slides().count(),
        before + 1,
        "clicking New Slide top half should add a slide"
    );
}

#[test]
fn m48_s1_04_small_split_widened_arrow_and_two_parts() {
    let mut h = harness_with_ribbon();
    select_first_object(&mut h);

    let copy_main = find_ribbon_widget_bounds(&h, "Copy").expect("Copy");
    let copy_arrow = find_ribbon_widget_bounds(&h, "Copy options").expect("Copy options");
    assert!(
        copy_arrow.width() >= 18.0,
        "SmallSplit arrow width should be >= 18 px, got {}",
        copy_arrow.width()
    );
    assert!(
        copy_main.max.x <= copy_arrow.min.x + 0.5,
        "SmallSplit halves should not overlap: {copy_main:?} vs {copy_arrow:?}"
    );

    let font_color_arrow =
        find_widget_bounds(&h, "Font Color options").expect("Font Color options");
    assert!(
        font_color_arrow.width() >= 18.0,
        "ToggleSplit arrow width should be >= 18 px, got {}",
        font_color_arrow.width()
    );

    // Clicking the Copy main half runs its main action without opening the menu.
    assert!(
        h.editor.ui.clipboard.is_none(),
        "clipboard should start empty before clicking Copy"
    );
    let dup_before = h.find_texts("Duplicate").len();
    h.click_at(copy_main.center());
    h.idle_frame();
    assert!(
        h.editor.ui.clipboard.is_some(),
        "clicking Copy main part should copy the selected object to the clipboard"
    );
    assert_eq!(
        h.find_texts("Duplicate").len(),
        dup_before,
        "clicking Copy main part should not open its menu"
    );

    // Clicking the Copy arrow strip opens its menu.
    h.click_at(copy_arrow.center());
    h.idle_frame();
    assert_eq!(
        h.find_texts("Duplicate").len(),
        dup_before + 1,
        "clicking Copy arrow should open its menu"
    );
}

#[test]
fn m48_s1_05_hover_fill_covers_only_hovered_part() {
    let mut h = harness_with_ribbon();
    select_first_object(&mut h);

    let p = coursecraft_render::palette(&h.ctx);
    let top_rect = find_ribbon_widget_bounds(&h, "Preview").expect("Preview top half");
    let bot_rect = find_ribbon_widget_bounds(&h, "Preview options").expect("Preview bottom half");
    assert!(
        top_rect.max.y <= bot_rect.min.y + 0.5,
        "LargeSplit halves must not overlap: {top_rect:?} vs {bot_rect:?}"
    );

    let whole_preview = top_rect.union(bot_rect);
    let has_h_divider = |shapes: &[egui::epaint::ClippedShape]| {
        shapes.iter().any(|cs| match &cs.shape {
            Shape::LineSegment { points, .. } => {
                (points[0].y - top_rect.max.y).abs() <= 1.5
                    && (points[1].y - top_rect.max.y).abs() <= 1.5
                    && points[0].x >= top_rect.min.x - 1.0
                    && points[1].x <= top_rect.max.x + 1.0
            }
            _ => false,
        })
    };

    // Hover top half of Preview.
    let shapes = h.frame_shapes(vec![Event::PointerMoved(top_rect.center())]);
    let top_hover_rects: Vec<Rect> = shapes
        .iter()
        .filter_map(|cs| match &cs.shape {
            Shape::Rect(r) if r.fill == p.hover && whole_preview.intersects(r.rect) => Some(r.rect),
            _ => None,
        })
        .collect();
    assert!(
        !top_hover_rects.is_empty(),
        "hovering Preview top half should paint a hover rect"
    );
    for r in &top_hover_rects {
        assert!(
            r.max.y <= top_rect.max.y + 1.0 && r.height() < 40.0,
            "top hover rect {r:?} must stay in top half {top_rect:?}"
        );
    }
    assert!(
        has_h_divider(&shapes),
        "hovering Preview top half should draw a horizontal divider between top and bottom halves"
    );

    // Hover bottom half of Preview.
    let shapes = h.frame_shapes(vec![Event::PointerMoved(bot_rect.center())]);
    let bot_hover_rects: Vec<Rect> = shapes
        .iter()
        .filter_map(|cs| match &cs.shape {
            Shape::Rect(r) if r.fill == p.hover && whole_preview.intersects(r.rect) => Some(r.rect),
            _ => None,
        })
        .collect();
    assert!(
        !bot_hover_rects.is_empty(),
        "hovering Preview bottom half should paint a hover rect"
    );
    for r in &bot_hover_rects {
        assert!(
            r.min.y >= bot_rect.min.y - 1.0 && r.height() < 40.0,
            "bottom hover rect {r:?} must stay in bottom half {bot_rect:?}"
        );
    }
    assert!(
        has_h_divider(&shapes),
        "hovering Preview bottom half should draw a horizontal divider between top and bottom halves"
    );

    // Hover main and arrow halves of a SmallSplit button ("Copy").
    let copy_main = find_ribbon_widget_bounds(&h, "Copy").expect("Copy");
    let copy_arrow = find_ribbon_widget_bounds(&h, "Copy options").expect("Copy options");
    let whole_copy = copy_main.union(copy_arrow);
    let has_v_divider = |shapes: &[egui::epaint::ClippedShape]| {
        shapes.iter().any(|cs| match &cs.shape {
            Shape::LineSegment { points, .. } => {
                (points[0].x - copy_main.max.x).abs() <= 1.5
                    && (points[1].x - copy_main.max.x).abs() <= 1.5
                    && points[0].y >= copy_main.min.y - 1.0
                    && points[1].y <= copy_main.max.y + 1.0
            }
            _ => false,
        })
    };
    let shapes = h.frame_shapes(vec![Event::PointerMoved(copy_main.center())]);
    let main_hover: Vec<Rect> = shapes
        .iter()
        .filter_map(|cs| match &cs.shape {
            Shape::Rect(r) if r.fill == p.hover && whole_copy.intersects(r.rect) => Some(r.rect),
            _ => None,
        })
        .collect();
    assert!(!main_hover.is_empty());
    for r in &main_hover {
        assert!(
            r.max.x <= copy_main.max.x + 1.0,
            "small split main hover {r:?} must not overlap arrow {copy_arrow:?}"
        );
    }
    assert!(
        has_v_divider(&shapes),
        "hovering small split main half should draw a vertical divider"
    );

    let shapes = h.frame_shapes(vec![Event::PointerMoved(copy_arrow.center())]);
    let arrow_hover: Vec<Rect> = shapes
        .iter()
        .filter_map(|cs| match &cs.shape {
            Shape::Rect(r) if r.fill == p.hover && whole_copy.intersects(r.rect) => Some(r.rect),
            _ => None,
        })
        .collect();
    assert!(!arrow_hover.is_empty());
    for r in &arrow_hover {
        assert!(
            r.min.x >= copy_arrow.min.x - 1.0,
            "small split arrow hover {r:?} must not overlap main {copy_main:?}"
        );
    }
    assert!(
        has_v_divider(&shapes),
        "hovering small split arrow half should draw a vertical divider"
    );
}

#[test]
fn m48_s1_06_accessibility_alt_down_and_non_split_unchanged() {
    let mut h = harness_with_ribbon();
    assert!(find_ribbon_widget_bounds(&h, "Preview").is_some());
    assert!(find_ribbon_widget_bounds(&h, "Preview options").is_some());

    // Non-split Large button ("Publish") has only one WidgetInfo and still runs from both its
    // icon and its label.
    assert!(
        find_widget_bounds(&h, "Publish options").is_none(),
        "non-split Publish button must not have an options widget"
    );
    let publish_rect = find_ribbon_widget_bounds(&h, "Publish").expect("Publish button");
    let publish_label = h.find_text("Publish").expect("Publish label");
    h.click_at(publish_label);
    h.idle_frame();
    assert!(
        h.editor.ui.publish.open,
        "clicking non-split Publish label should open Publish dialog"
    );
    h.press(Key::Escape, Modifiers::NONE);
    h.idle_frame();
    assert!(
        !h.editor.ui.publish.open,
        "Escape should close Publish dialog"
    );

    let publish_icon = egui::pos2(publish_rect.center().x, publish_rect.min.y + 18.0);
    h.click_at(publish_icon);
    h.idle_frame();
    assert!(
        h.editor.ui.publish.open,
        "clicking non-split Publish icon should open Publish dialog"
    );
    h.press(Key::Escape, Modifiers::NONE);
    h.idle_frame();

    // Focus Preview via Tab and open its menu with Alt+ArrowDown.
    let preview_id = h
        .last_widgets
        .iter()
        .find(|w| {
            w.label.as_deref() == Some("Preview") && w.bounds.center().y <= h.top_bars_height()
        })
        .map(|w| w.id)
        .expect("Preview widget");
    let mut focused = false;
    for _ in 0..80 {
        h.press(Key::Tab, Modifiers::NONE);
        h.idle_frame();
        if h.ctx
            .memory(|m| m.focused().is_some_and(|id| id.value() == preview_id.0))
        {
            focused = true;
            break;
        }
    }
    assert!(focused, "Tab should be able to focus the Preview button");

    h.press(Key::ArrowDown, Modifiers::ALT);
    h.idle_frame();
    assert!(
        h.find_text("Preview this slide").is_some(),
        "Alt+ArrowDown on focused Preview should open its menu"
    );
}
