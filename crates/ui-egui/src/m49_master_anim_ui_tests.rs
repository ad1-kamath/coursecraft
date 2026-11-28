use coursecraft_course::{AnimEffect, Animation, Course, Easing};
use coursecraft_render::Hit;

use crate::anim_preview::{PreviewState, Slot};
use crate::canvas::timeline_animated_items;
use crate::dock::{self, Panel};
use crate::properties::InspectorTab;
use crate::ribbon::gallery;
use crate::ribbon::spec::{Gallery, UiAction};
use crate::ribbon::ui_available;
use crate::test_harness::TestHarness;

fn master_course() -> Course {
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "m49-master-ui",
        "title": "M49 Master UI",
        "slide_size": { "w": 1280, "h": 720 },
        "masters": [{
            "id": "m1",
            "title": "Main Master",
            "duration_ms": 5000,
            "objects": [
                {
                    "id": "m_rect",
                    "name": "Master Banner",
                    "kind": { "type": "shape", "shape": "rectangle", "fill": "#2F6FEB" },
                    "frame": { "x": 100, "y": 80, "w": 300, "h": 120 }
                }
            ]
        }],
        "scenes": [{
            "id": "sc1",
            "title": "Scene 1",
            "slides": [{
                "id": "sl1",
                "title": "Slide 1",
                "master": "m1",
                "duration_ms": 5000,
                "objects": [
                    {
                        "id": "s_rect",
                        "name": "Slide Box",
                        "kind": { "type": "shape", "shape": "rectangle", "fill": "#D2522B" },
                        "frame": { "x": 500, "y": 240, "w": 200, "h": 140 }
                    }
                ]
            }]
        }]
    });
    Course::from_json(&json.to_string()).expect("course")
}

#[test]
fn m49_s5_animations_tab_enabled_in_master_view_and_sets_animation() {
    let mut h = TestHarness::new(master_course());
    h.editor.toggle_master_view();
    h.editor.selection.slide = "m1".into();
    h.editor.selection.object = Some("m_rect".into());
    h.editor.selection.also.clear();
    h.idle_frame();

    assert!(
        gallery::available(&h.editor, Gallery::Entrance).is_ok(),
        "Animations entrance gallery should be enabled in Master View"
    );
    assert!(
        ui_available(&h.editor, UiAction::AnimationPainter).is_ok(),
        "Animation Painter ribbon control should be enabled in Master View"
    );
    assert!(
        ui_available(&h.editor, UiAction::FormatPainter).is_err(),
        "Format Painter ribbon control should stay disabled in Master View"
    );
    assert!(
        ui_available(&h.editor, UiAction::AddMotionPath).is_err(),
        "Add Motion Path ribbon control should stay disabled in Master View"
    );

    // Index 2 in EFFECTS + None is Fade.
    gallery::apply(&mut h.editor, Gallery::Entrance, 2);

    let master = h.editor.session.course.master("m1").expect("master");
    let obj = master.object("m_rect").expect("m_rect").0;
    assert!(
        obj.entrance.is_some(),
        "entrance animation should be stored on master object"
    );

    let _ = h.editor.session.undo();
    let master = h.editor.session.course.master("m1").expect("master");
    assert!(
        master
            .object("m_rect")
            .expect("m_rect")
            .0
            .entrance
            .is_none(),
        "undo should clear master entrance animation"
    );

    let _ = h.editor.session.redo();
    let master = h.editor.session.course.master("m1").expect("master");
    assert!(
        master
            .object("m_rect")
            .expect("m_rect")
            .0
            .entrance
            .is_some(),
        "redo should restore master entrance animation"
    );
}

#[test]
fn m49_s5_animate_inspector_works_in_master_view_without_transition_or_motion_path() {
    let mut h = TestHarness::new(master_course());
    h.editor.toggle_master_view();
    h.editor.selection.slide = "m1".into();
    h.editor.selection.object = Some("m_rect".into());
    h.editor.selection.also.clear();
    h.editor.ui.inspector_tab = InspectorTab::Animate;
    h.idle_frame();

    assert!(h.find_text("Entrance animation").is_some());
    assert!(h.find_text("Emphasis").is_some());
    assert!(h.find_text("Exit animation").is_some());
    assert!(
        h.find_text("Transition").is_none(),
        "masters should not show Transition section in Animate inspector"
    );
    assert!(
        h.find_text("Motion paths").is_none(),
        "masters should not show Motion paths section in Animate inspector"
    );
    assert!(h.find_text("+ Add motion path").is_none());
}

#[test]
fn m49_s5_timeline_is_visible_in_master_view_and_shows_master_objects() {
    // 1. Fixed (non-docked) layout
    let mut fixed = TestHarness::new_with_config(
        master_course(),
        (1280.0, 800.0),
        coursecraft_render::ThemeMode::Dark,
        1.0,
        true,
    );
    fixed.editor.ui.dock.enabled = false;
    fixed.editor.ui.bottom_tab = crate::BottomTab::Triggers;
    fixed.editor.toggle_master_view();
    fixed.editor.selection.slide = "m1".into();
    fixed.idle_frame();

    let timeline_rect = fixed
        .editor
        .ui
        .timeline
        .panel_rect
        .expect("timeline panel should be visible in fixed Master View");
    assert!(
        fixed.find_text("Master Banner").is_some(),
        "master object should appear in timeline"
    );
    assert!(fixed.find_text_in("States", timeline_rect).is_none());
    assert!(fixed.find_text_in("Triggers", timeline_rect).is_none());
    assert!(fixed.find_text_in("Variables", timeline_rect).is_none());
    assert!(fixed.find_text_in("Question", timeline_rect).is_none());

    // 2. Docked layout
    let mut docked = TestHarness::new_with_config(
        master_course(),
        (1280.0, 800.0),
        coursecraft_render::ThemeMode::Dark,
        1.0,
        true,
    );
    docked.editor.ui.dock.enabled = true;
    docked.editor.toggle_master_view();
    docked.editor.selection.slide = "m1".into();
    docked.idle_frame();

    assert!(
        docked.editor.ui.timeline.panel_rect.is_some(),
        "timeline panel should be visible in docked Master View"
    );
    assert!(docked.find_text("Master Banner").is_some());

    dock::set_open(&mut docked.editor, Panel::Layers, true);
    docked.idle_frame();
    assert!(docked.find_text("Slide masters have no layers.").is_some());
}

#[test]
fn m49_s5_anim_preview_and_timeline_play_work_in_master_view() {
    let mut h = TestHarness::new(master_course());
    let fade = Animation {
        effect: AnimEffect::new("fade"),
        duration_ms: 1000,
        easing: Easing::Linear,
        sequence: Default::default(),
    };
    h.editor.run(
        "object.set_animation",
        serde_json::json!({
            "slide": "m1",
            "object": "m_rect",
            "which": "entrance",
            "animation": fade,
        }),
    );

    h.editor.toggle_master_view();
    h.editor.selection.slide = "m1".into();
    h.editor.selection.object = Some("m_rect".into());
    h.editor.selection.also.clear();
    h.idle_frame();

    // 1. Canvas anim_preview in Master View
    let master = h.editor.session.course.master("m1").expect("master");
    let base = coursecraft_render::compose_authoring_layers_opts(
        &h.editor.session.course,
        master,
        None,
        &[],
        false,
        true,
    );
    let mut preview = PreviewState::default();
    preview.play("m1", vec!["m_rect".to_owned()], Slot::Entrance, fade);
    let ids = vec!["m_rect".to_owned()];
    let _ = preview.items(&h.editor.session.course, ("m1", &ids), &base, 10.0, 1);
    let mid = preview
        .items(&h.editor.session.course, ("m1", &ids), &base, 10.65, 2)
        .expect("preview should produce animated items in Master View");
    let preview_item = mid
        .iter()
        .find(|i| i.hit == Hit::Object("m_rect".into()))
        .expect("m_rect item");
    assert!(
        (preview_item.opacity - 0.5).abs() < 0.05,
        "expected ~0.5 opacity mid-fade in preview, got {}",
        preview_item.opacity
    );

    // 2. Timeline play in Master View
    let played_master =
        timeline_animated_items(&base, master, None, h.editor.session.course.slide_size, 500);
    let played_item = played_master
        .iter()
        .find(|i| i.hit == Hit::Object("m_rect".into()))
        .expect("m_rect in timeline play");
    assert!(
        (played_item.opacity - 0.5).abs() < 0.05,
        "expected ~0.5 opacity mid-fade in Master View timeline play, got {}",
        played_item.opacity
    );

    // 3. Timeline play on a regular slide that uses the master
    let slide = h.editor.session.course.slide("sl1").expect("sl1");
    let slide_base = coursecraft_render::compose_authoring_layers_opts(
        &h.editor.session.course,
        slide,
        None,
        &[],
        false,
        true,
    );
    let played_slide = timeline_animated_items(
        &slide_base,
        slide,
        Some(master),
        h.editor.session.course.slide_size,
        500,
    );
    let master_item_on_slide = played_slide
        .iter()
        .find(|i| i.hit == Hit::None)
        .expect("master item on regular slide");
    assert!(
        (master_item_on_slide.opacity - 0.5).abs() < 0.05,
        "expected ~0.5 opacity on master item during regular slide timeline play, got {}",
        master_item_on_slide.opacity
    );
}

#[test]
fn m49_s5_animation_painter_and_master_group_in_master_view_and_prefix_safety() {
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "m49-master-extra",
        "title": "M49 Master Extra",
        "slide_size": { "w": 1280, "h": 720 },
        "masters": [{
            "id": "m1",
            "title": "Main Master",
            "duration_ms": 5000,
            "objects": [
                {
                    "id": "m_a",
                    "name": "Master A",
                    "kind": { "type": "shape", "shape": "rectangle", "fill": "#2F6FEB" },
                    "frame": { "x": 100, "y": 80, "w": 200, "h": 100 },
                    "group": "mg1"
                },
                {
                    "id": "m_b",
                    "name": "Master B",
                    "kind": { "type": "shape", "shape": "rectangle", "fill": "#22A06B" },
                    "frame": { "x": 320, "y": 80, "w": 200, "h": 100 },
                    "group": "mg1"
                },
                {
                    "id": "m_c",
                    "name": "Master C",
                    "kind": { "type": "shape", "shape": "rectangle", "fill": "#D2522B" },
                    "frame": { "x": 600, "y": 80, "w": 200, "h": 100 }
                }
            ]
        }],
        "scenes": [{
            "id": "sc1",
            "title": "Scene 1",
            "slides": [{
                "id": "sl1",
                "title": "Slide 1",
                "master": "m1",
                "duration_ms": 5000,
                "objects": [
                    {
                        "id": "s_obj",
                        "kind": { "type": "shape", "shape": "rectangle", "fill": "#555555" },
                        "frame": { "x": 10, "y": 10, "w": 50, "h": 50 }
                    }
                ],
                "layers": [
                    {
                        "id": "l_bg",
                        "name": "Background Layer",
                        "duration_ms": 5000,
                        "objects": [
                            {
                                "id": "l_same_frame",
                                "kind": { "type": "shape", "shape": "rectangle", "fill": "#999999" },
                                "frame": { "x": 100, "y": 80, "w": 200, "h": 100 }
                            }
                        ]
                    },
                    {
                        "id": "l_fg",
                        "name": "Foreground Layer",
                        "duration_ms": 5000,
                        "objects": []
                    }
                ]
            }]
        }]
    });
    let course = Course::from_json(&json.to_string()).expect("course");
    let mut h = TestHarness::new(course);

    // Format Painter armed on a regular slide is cleared when entering Master View.
    h.editor.selection.slide = "sl1".into();
    h.editor.selection.set_many(["s_obj"]);
    h.editor.arm_format_painter(false);
    assert!(h.editor.ui.format_painter.is_some());
    h.editor.enter_master_view();
    assert!(
        h.editor.ui.format_painter.is_none(),
        "entering Master View should disarm Format Painter"
    );

    // Animate inspector on a master group shows Group animation and hides Transition.
    h.editor.selection.slide = "m1".into();
    h.editor.selection.set_many(["m_a", "m_b"]);
    h.editor.ui.inspector_tab = InspectorTab::Animate;
    h.idle_frame();
    assert!(h.find_text("Group animation").is_some());
    assert!(h.find_text("Transition").is_none());
    assert!(h.find_text("Motion paths").is_none());

    // Animation Painter copies an animation from one master object to another in Master View.
    let fade = Animation {
        effect: AnimEffect::new("fade"),
        duration_ms: 1000,
        easing: Easing::Linear,
        sequence: Default::default(),
    };
    h.editor.run(
        "object.set_animation",
        serde_json::json!({
            "slide": "m1",
            "object": "m_c",
            "which": "entrance",
            "animation": fade,
        }),
    );
    h.editor.selection.set_many(["m_c"]);
    h.editor.arm_animation_painter(false);
    assert!(h.editor.animation_painter_armed());
    h.editor.apply_format_painter(&["m_a".into()]);
    let master = h.editor.session.course.master("m1").expect("master");
    assert!(
        master.object("m_a").expect("m_a").0.entrance.is_some(),
        "Animation Painter should copy entrance onto target master object"
    );

    // Master group animation emits a `master:` GroupAnim during regular slide timeline play.
    h.editor.run(
        "object.set_animation",
        serde_json::json!({
            "slide": "m1",
            "object": "mg1",
            "which": "entrance",
            "animation": fade,
        }),
    );
    let slide = h.editor.session.course.slide("sl1").expect("sl1");
    let master = h.editor.session.course.master("m1").expect("m1");
    let slide_base = coursecraft_render::compose_authoring_layers_opts(
        &h.editor.session.course,
        slide,
        None,
        &[],
        false,
        true,
    );
    let played = timeline_animated_items(
        &slide_base,
        slide,
        Some(master),
        h.editor.session.course.slide_size,
        500,
    );
    let group_items: Vec<_> = played
        .iter()
        .filter(|i| {
            i.hit == Hit::None
                && i.group_anim
                    .as_ref()
                    .is_some_and(|ga| ga.id == "master:mg1")
        })
        .collect();
    assert_eq!(
        group_items.len(),
        2,
        "both master group members should carry `master:mg1` GroupAnim on regular slide play"
    );

    // When base/master is hidden (`show_base = false`), an unfocused layer item with the same
    // frame as a master object must not be matched against master objects.
    let hidden_base = coursecraft_render::compose_authoring_layers_opts(
        &h.editor.session.course,
        slide,
        Some("l_fg"),
        &["l_bg".to_owned()],
        true,
        false,
    );
    let played_hidden = timeline_animated_items(
        &hidden_base,
        slide,
        None,
        h.editor.session.course.slide_size,
        0,
    );
    assert_eq!(
        played_hidden.len(),
        1,
        "unfocused layer item should remain untouched when base/master is hidden"
    );
    assert_eq!(played_hidden[0].opacity, 1.0);
}
