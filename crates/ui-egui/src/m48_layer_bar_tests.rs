//! Headless UI tests for M48 step 3: the dedicated canvas layer bar above
//! the scroll area is always visible, wraps cleanly, and never overlaps the
//! slide or rulers.

use crate::ZoomLevel;
use crate::test_harness::TestHarness;
use coursecraft_course::{BaseSlideView, Course};
use egui::epaint::{ClippedShape, Shape};
use egui::{Pos2, Rect};

fn hello() -> Course {
    let json = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/hello-course/course.json"
    ))
    .expect("hello course");
    Course::from_json(&json).expect("valid course")
}

fn find_text_rect_in(shapes: &[ClippedShape], label: &str, in_rect: Rect) -> Option<(Rect, Rect)> {
    fn walk(shape: &Shape, label: &str, in_rect: Rect) -> Option<Rect> {
        match shape {
            Shape::Text(t) if t.galley.text() == label => {
                let rect = t.visual_bounding_rect();
                if in_rect.contains(rect.center()) {
                    Some(rect)
                } else {
                    None
                }
            }
            Shape::Vec(v) => v.iter().find_map(|s| walk(s, label, in_rect)),
            _ => None,
        }
    }
    shapes
        .iter()
        .find_map(|cs| walk(&cs.shape, label, in_rect).map(|r| (r, cs.clip_rect)))
}

fn find_plus_rect_in(shapes: &[ClippedShape], in_rect: Rect) -> Option<(Rect, Rect)> {
    let plus = coursecraft_render::Icon::Plus.codepoint().to_string();
    find_text_rect_in(shapes, &plus, in_rect)
}

fn add_layers_and_select_first(h: &mut TestHarness, names: &[&str]) {
    let slide_id = h.editor.selection.slide.clone();
    for name in names {
        h.editor
            .session
            .execute(
                "layer.add",
                &serde_json::json!({ "slide": slide_id, "name": name }),
            )
            .expect("layer.add");
    }
    let first_id = h
        .editor
        .session
        .course
        .slide(&slide_id)
        .expect("slide")
        .layers
        .first()
        .expect("at least one layer")
        .id
        .clone();
    h.editor.selection.layer = Some(first_id);
    h.editor.selection.repair(&h.editor.session.course);
    h.idle_frame();
    h.idle_frame();
}

#[test]
fn m48_s3_01_all_pills_outside_slide_and_inside_clip_across_zooms() {
    let mut h = TestHarness::new(hello());
    h.editor.selection.slide = "s-intro".into();
    add_layers_and_select_first(&mut h, &["Correct", "Incorrect", "Extra"]);
    assert_eq!(
        h.editor.session.course.editor.base_slide_view,
        BaseSlideView::Dim
    );

    let central = h.central_panel_rect();
    let expected_labels = [
        "Base",
        "About",
        "Correct",
        "Incorrect",
        "Extra",
        "Base slide:",
        "Show",
        "Dim",
        "Hide",
    ];

    for zoom in [
        ZoomLevel::Fit,
        ZoomLevel::Percent50,
        ZoomLevel::Percent100,
        ZoomLevel::Percent200,
    ] {
        h.editor.ui.zoom = zoom;
        h.idle_frame();
        h.idle_frame();

        let slide_rect = h.editor.ui.last_slide_rect.expect("slide rect");
        let canvas_viewport = crate::canvas::canvas_rect(central);
        let visible_slide = slide_rect.intersect(canvas_viewport);
        let shapes = h.frame_shapes(vec![]);

        for label in expected_labels {
            let (rect, clip) = find_text_rect_in(&shapes, label, central)
                .unwrap_or_else(|| panic!("missing pill {label:?} at zoom {zoom:?}"));
            assert!(
                central.contains_rect(rect),
                "pill {label:?} rect {rect:?} outside central {central:?} at {zoom:?}"
            );
            assert!(
                clip.contains_rect(rect),
                "pill {label:?} rect {rect:?} clipped by {clip:?} at {zoom:?}"
            );
            assert!(
                rect.max.y <= canvas_viewport.min.y,
                "pill {label:?} max.y {} is not above canvas viewport min.y {} at {zoom:?}",
                rect.max.y,
                canvas_viewport.min.y
            );
            assert!(
                !rect.intersects(visible_slide),
                "pill {label:?} rect {rect:?} intersects visible slide {visible_slide:?} at {zoom:?}"
            );
            assert!(
                rect.max.y <= visible_slide.min.y,
                "pill {label:?} max.y {} is not above visible slide min.y {} at {zoom:?}",
                rect.max.y,
                visible_slide.min.y
            );
        }

        let (plus_rect, plus_clip) =
            find_plus_rect_in(&shapes, central).expect("Add layer plus icon");
        assert!(central.contains_rect(plus_rect));
        assert!(plus_clip.contains_rect(plus_rect));
        assert!(plus_rect.max.y <= canvas_viewport.min.y);
        assert!(!plus_rect.intersects(visible_slide));
        assert!(plus_rect.max.y <= visible_slide.min.y);
    }
}

#[test]
fn m48_s3_02_narrow_window_wrapping_bar_never_overlaps_slide_and_hide_clicks() {
    let mut h = TestHarness::new_with_size(hello(), (760.0, 700.0));
    h.editor.selection.slide = "s-intro".into();
    add_layers_and_select_first(
        &mut h,
        &[
            "Feedback Correct Layer",
            "Feedback Incorrect Layer",
            "Additional Hint Layer",
        ],
    );

    let central = h.central_panel_rect();
    let slide_rect = h.editor.ui.last_slide_rect.expect("slide rect");
    let shapes = h.frame_shapes(vec![]);

    let mut min_y = f32::MAX;
    let mut max_y = f32::MIN;
    for label in [
        "Base",
        "About",
        "Feedback Correct Layer",
        "Feedback Incorrect Layer",
        "Additional Hint Layer",
        "Base slide:",
        "Show",
        "Dim",
        "Hide",
    ] {
        let (rect, clip) = find_text_rect_in(&shapes, label, central)
            .unwrap_or_else(|| panic!("missing wrapped pill {label:?}"));
        assert!(central.contains_rect(rect), "{label:?} outside central");
        assert!(clip.contains_rect(rect), "{label:?} clipped");
        assert!(
            !rect.intersects(slide_rect),
            "{label:?} {rect:?} overlaps slide {slide_rect:?}"
        );
        assert!(rect.max.y <= slide_rect.min.y);
        min_y = min_y.min(rect.min.y);
        max_y = max_y.max(rect.max.y);
    }
    assert!(
        max_y - min_y > 24.0,
        "narrow window bar should wrap onto 2+ rows: min_y={min_y}, max_y={max_y}"
    );

    let hide_pos = h.find_text_in("Hide", central).expect("Hide button");
    h.click_at(hide_pos);
    h.idle_frame();
    assert_eq!(
        h.editor.session.course.editor.base_slide_view,
        BaseSlideView::Hide
    );
}

#[test]
fn m48_s3_03_rulers_and_zoom_scroll_do_not_overlap_or_jump_bar() {
    let mut h = TestHarness::new(hello());
    h.editor.selection.slide = "s-intro".into();
    h.editor.session.course.editor.show_rulers = true;
    add_layers_and_select_first(&mut h, &["Correct", "Incorrect", "Extra"]);

    let central = h.central_panel_rect();
    let shapes = h.frame_shapes(vec![]);
    let (base_rect, _) = find_text_rect_in(&shapes, "Base", central).expect("Base pill");
    let (dim_rect, _) = find_text_rect_in(&shapes, "Dim", central).expect("Dim pill");

    // The top ruler starts at central.min.y + LAYER_BAR_MIN_H (the bar's height);
    // verify the bar pills sit strictly above the ruler strip.
    let bar_bottom = central.min.y + crate::canvas::LAYER_BAR_MIN_H;
    let top_ruler = crate::rulers::top_strip(Rect::from_min_max(
        Pos2::new(central.min.x, bar_bottom),
        central.max,
    ));
    assert!(
        base_rect.max.y <= top_ruler.min.y,
        "Base pill {base_rect:?} overlaps top ruler {top_ruler:?}"
    );
    assert!(
        dim_rect.max.y <= top_ruler.min.y,
        "Dim pill {dim_rect:?} overlaps top ruler {top_ruler:?}"
    );

    let base_y = base_rect.center().y;
    let dim_y = dim_rect.center().y;

    for zoom in [
        ZoomLevel::Percent50,
        ZoomLevel::Percent100,
        ZoomLevel::Percent200,
    ] {
        h.editor.ui.zoom = zoom;
        h.idle_frame();
        h.idle_frame();
        let slide_center = h.editor.ui.last_slide_rect.expect("slide rect").center();
        h.scroll_over(slide_center, -180.0);
        h.idle_frame();

        let after_base_y = h
            .find_text_in("Base", central)
            .expect("Base after zoom/scroll")
            .y;
        let after_dim_y = h
            .find_text_in("Dim", central)
            .expect("Dim after zoom/scroll")
            .y;
        assert!(
            (after_base_y - base_y).abs() < 0.5,
            "Base pill y jumped from {base_y} to {after_base_y} at {zoom:?}"
        );
        assert!(
            (after_dim_y - dim_y).abs() < 0.5,
            "Dim pill y jumped from {dim_y} to {after_dim_y} at {zoom:?}"
        );
    }
}

#[test]
fn m48_s3_04_master_view_hides_layer_bar_cleanly() {
    let mut h = TestHarness::new(hello());
    h.editor.selection.slide = "s-intro".into();
    add_layers_and_select_first(&mut h, &["Correct"]);

    h.editor.enter_master_view();
    h.idle_frame();
    h.idle_frame();

    let central = h.central_panel_rect();
    assert!(h.find_text_in("Base", central).is_none());
    assert!(h.find_text_in("Correct", central).is_none());
    assert!(h.find_text_in("Base slide:", central).is_none());

    let slide_rect = h.editor.ui.last_slide_rect.expect("master slide rect");
    assert!(
        central.contains_rect(slide_rect),
        "master slide {slide_rect:?} should render inside central {central:?}"
    );
}
