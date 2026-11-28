use coursecraft_course::{AnimEffect, Animation, Course, Easing, Emphasis};
use coursecraft_render::{Hit, SlideTransform};
use egui::pos2;

use crate::anim_preview::{PreviewState, Slot};
use crate::canvas::{canvas_fit_rect, canvas_rect, timeline_animated_items};
use crate::ribbon::gallery;
use crate::ribbon::spec::Gallery;
use crate::test_harness::TestHarness;

fn two_group_course() -> Course {
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "m49-ui",
        "title": "M49 UI",
        "slide_size": { "w": 1280, "h": 720 },
        "scenes": [{
            "id": "sc1",
            "title": "Scene 1",
            "slides": [{
                "id": "sl1",
                "title": "Slide 1",
                "duration_ms": 5000,
                "objects": [
                    {
                        "id": "a",
                        "group": "g1",
                        "kind": { "type": "shape", "shape": "rectangle", "fill": "#2F6FEB" },
                        "frame": { "x": 100, "y": 100, "w": 150, "h": 100 }
                    },
                    {
                        "id": "b",
                        "group": "g1",
                        "kind": { "type": "shape", "shape": "rectangle", "fill": "#D2522B" },
                        "frame": { "x": 200, "y": 150, "w": 150, "h": 100 }
                    },
                    {
                        "id": "c",
                        "group": "g2",
                        "kind": { "type": "shape", "shape": "rectangle", "fill": "#3B8A3E" },
                        "frame": { "x": 700, "y": 200, "w": 160, "h": 120 }
                    },
                    {
                        "id": "d",
                        "group": "g2",
                        "kind": { "type": "shape", "shape": "rectangle", "fill": "#7A3E9D" },
                        "frame": { "x": 800, "y": 260, "w": 160, "h": 120 }
                    }
                ],
                "groups": [
                    { "id": "g1", "name": "Group 1" },
                    { "id": "g2", "name": "Group 2" }
                ]
            }]
        }]
    });
    Course::from_json(&json.to_string()).expect("course")
}

fn setup() -> (TestHarness, SlideTransform) {
    let mut h = TestHarness::new(two_group_course());
    h.editor.selection.slide = "sl1".into();
    h.editor.ui.snap_to_guides = false;
    h.idle_frame();
    let t = SlideTransform::fit(
        canvas_fit_rect(canvas_rect(h.central_panel_rect())),
        h.editor.session.course.slide_size,
    )
    .expect("transform");
    (h, t)
}

#[test]
fn m49_s3_ribbon_entrance_on_group_sets_group_not_children_and_undoes() {
    let (mut h, _t) = setup();
    h.editor.selection.set_many(["a", "b"]);
    assert_eq!(
        h.editor.selected_group_for_animation().as_deref(),
        Some("g1")
    );

    // Index 2 in EFFECTS + None is Fade (index 1 in EFFECTS = fade, +1 for None = 2).
    gallery::apply(&mut h.editor, Gallery::Entrance, 2);

    let slide = h.editor.session.course.slide("sl1").expect("slide");
    let group = slide.group("g1").expect("g1").0;
    assert!(group.entrance.is_some(), "group should carry entrance");
    assert!(slide.object("a").expect("a").0.entrance.is_none());
    assert!(slide.object("b").expect("b").0.entrance.is_none());

    let _ = h.editor.session.undo();
    let slide = h.editor.session.course.slide("sl1").expect("slide");
    assert!(slide.group("g1").expect("g1").0.entrance.is_none());

    let _ = h.editor.session.redo();
    let slide = h.editor.session.course.slide("sl1").expect("slide");
    assert!(slide.group("g1").expect("g1").0.entrance.is_some());
}

fn click_inspector_tab(h: &mut TestHarness, tab: &str) {
    let right_x = h.right_panel_rect().min.x;
    let pos = h
        .find_texts(tab)
        .into_iter()
        .filter(|p| p.y < 80.0 && p.x >= right_x)
        .min_by(|a, b| a.y.total_cmp(&b.y))
        .unwrap_or_else(|| panic!("{tab} tab"));
    h.click_at(pos);
    h.idle_frame();
}

#[test]
fn m49_s3_animate_inspector_edits_group_and_focused_child_edits_child() {
    let (mut h, _t) = setup();
    click_inspector_tab(&mut h, "Animate");
    h.editor.selection.set_many(["a", "b"]);
    h.idle_frame();

    assert_eq!(
        h.editor.selected_group_for_animation().as_deref(),
        Some("g1")
    );
    // Set entrance on group via gallery, then verify inspector syncs from group.
    gallery::apply(&mut h.editor, Gallery::Entrance, 2);
    h.idle_frame();
    assert_eq!(h.editor.ui.properties.entrance_effect, "fade");

    // Focus a single child inside the group: selected_group_for_animation becomes None.
    h.editor.ui.group_focus = Some("a".into());
    h.editor.selection.set_many(["a"]);
    h.idle_frame();
    assert_eq!(h.editor.selected_group_for_animation(), None);
    assert_eq!(h.editor.ui.properties.entrance_effect, "none");

    gallery::apply(&mut h.editor, Gallery::Entrance, 3);
    let slide = h.editor.session.course.slide("sl1").expect("slide");
    assert!(slide.object("a").expect("a").0.entrance.is_some());
    assert!(slide.object("b").expect("b").0.entrance.is_none());
    assert_eq!(
        slide
            .group("g1")
            .expect("g1")
            .0
            .entrance
            .as_ref()
            .map(|e| e.effect.id.as_str()),
        Some("fade")
    );
}

#[test]
fn m49_s3_animation_painter_copies_between_groups() {
    let (mut h, t) = setup();
    h.editor.selection.set_many(["a", "b"]);
    gallery::apply(&mut h.editor, Gallery::Entrance, 2);
    h.editor.ui.anim_preview.stop();
    h.idle_frame();

    assert!(h.editor.can_arm_animation_painter());
    h.editor.arm_animation_painter(false);
    h.idle_frame();
    assert!(h.editor.animation_painter_armed());

    // Click on member `c` of group `g2` on the canvas.
    h.click_at(t.to_screen_pos(pos2(760.0, 250.0)));
    h.idle_frame();

    let slide = h.editor.session.course.slide("sl1").expect("slide");
    let g2 = slide.group("g2").expect("g2").0;
    assert_eq!(
        g2.entrance.as_ref().map(|e| e.effect.id.as_str()),
        Some("fade")
    );
    assert!(slide.object("c").expect("c").0.entrance.is_none());
    assert!(slide.object("d").expect("d").0.entrance.is_none());
    assert!(!h.editor.animation_painter_armed());
}

#[test]
fn m49_s3_anim_preview_and_timeline_play_emit_group_anim() {
    let (mut h, _t) = setup();
    h.editor.selection.set_many(["a", "b"]);
    let anim = Animation {
        effect: AnimEffect::new("zoom"),
        duration_ms: 1000,
        easing: Easing::Linear,
        sequence: Default::default(),
    };
    h.editor.run(
        "object.set_animation",
        serde_json::json!({
            "slide": "sl1",
            "object": "g1",
            "which": "entrance",
            "animation": anim,
        }),
    );

    let slide = h.editor.session.course.slide("sl1").expect("slide");
    let expected_frame = slide.group_frame("g1", None).expect("group frame");
    let expected_rect = egui::Rect::from_min_size(
        pos2(expected_frame.x, expected_frame.y),
        egui::vec2(expected_frame.w, expected_frame.h),
    );
    let base = coursecraft_render::compose_authoring(&h.editor.session.course, slide, None);

    // 1. Canvas animation preview (`PreviewState::items`)
    let mut pv = PreviewState::default();
    let ids = vec!["a".to_owned(), "b".to_owned()];
    pv.play_group("sl1", ids.clone(), "g1".to_owned(), Slot::Entrance, anim);
    let _ = pv.items(&h.editor.session.course, ("sl1", &ids), &base, 0.0, 1);
    let out = pv
        .items(&h.editor.session.course, ("sl1", &ids), &base, 0.65, 2)
        .expect("preview items at 50%");
    for oid in ["a", "b"] {
        let item = out
            .iter()
            .find(|i| i.hit == Hit::Object(oid.into()))
            .expect("member item");
        let ga = item.group_anim.as_ref().expect("group_anim on preview");
        assert_eq!(ga.id, "g1");
        assert_eq!(ga.frame, expected_rect);
    }

    // 2. Timeline play (`timeline_animated_items`)
    let played = timeline_animated_items(&base, slide, h.editor.session.course.slide_size, 500);
    for oid in ["a", "b"] {
        let item = played
            .iter()
            .find(|i| i.hit == Hit::Object(oid.into()))
            .expect("member item in timeline play");
        let ga = item.group_anim.as_ref().expect("group_anim on timeline");
        assert_eq!(ga.id, "g1");
        assert_eq!(ga.frame, expected_rect);
    }
}

#[test]
fn m49_s3_timeline_group_row_renders_with_group_animation() {
    let (mut h, _t) = setup();
    let anim = Animation {
        effect: AnimEffect::new("fade"),
        duration_ms: 500,
        easing: Easing::Linear,
        sequence: Default::default(),
    };
    h.editor.run(
        "object.set_animation",
        serde_json::json!({
            "slide": "sl1",
            "object": "g1",
            "which": "entrance",
            "animation": anim,
        }),
    );
    h.editor.run(
        "object.set_animation",
        serde_json::json!({
            "slide": "sl1",
            "object": "g1",
            "which": "exit",
            "animation": anim,
        }),
    );
    h.editor.run(
        "object.add_emphasis",
        serde_json::json!({
            "slide": "sl1",
            "object": "g1",
            "effect": Emphasis::new("pulse", 1000).effect.id,
            "start_ms": 1000,
        }),
    );

    h.idle_frame();
    assert!(
        h.editor.ui.timeline.group_bar_rects.contains_key("g1"),
        "timeline should render group row and record g1 bar rect"
    );
}
