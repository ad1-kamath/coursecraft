//! M49 step s4: slide master objects and groups animate on each slide's base
//! timeline while staying non-interactive, unfocusable, and event-free.

use coursecraft_course::Course;
use coursecraft_logic::{Input, Player};

use crate::compose::{AnimLook, Hit, compose};
use crate::focus::focus_order_candidates;
use crate::media::{MediaFetch, MediaSource};
use crate::offscreen::{RenderMode, RenderOptions, RgbaImage, render_slide_rgba};
use crate::view::SlideView;

struct NoMedia;
impl MediaSource for NoMedia {
    fn get(&self, _src: &str) -> MediaFetch {
        MediaFetch::Missing
    }
}

fn px(img: &RgbaImage, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * img.width + x) * 4) as usize;
    [
        img.pixels[i],
        img.pixels[i + 1],
        img.pixels[i + 2],
        img.pixels[i + 3],
    ]
}

fn render_slide_at(c: &Course, slide_id: &str, ms: u32) -> RgbaImage {
    let opts = RenderOptions {
        width: 1280,
        mode: RenderMode::Player,
        layers: Vec::new(),
        at_ms: Some(ms),
    };
    render_slide_rgba(c, slide_id, &opts, &NoMedia).expect("render")
}

#[test]
fn m49_s4_master_fade_renders_invisible_at_0_visible_at_600_across_two_slides() {
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "m49-master-fade",
        "title": "Master Fade",
        "slide_size": { "w": 1280, "h": 720 },
        "masters": [{
            "id": "m1",
            "title": "Master",
            "background": "#FFFFFF",
            "duration_ms": 5000,
            "objects": [{
                "id": "m_box",
                "kind": { "type": "shape", "shape": "rectangle", "fill": "#FF0000" },
                "frame": { "x": 100, "y": 100, "w": 200, "h": 200 },
                "entrance": { "effect": { "type": "fade" }, "duration_ms": 500, "easing": "linear" }
            }]
        }],
        "scenes": [{
            "id": "sc1",
            "title": "Scene",
            "slides": [
                { "id": "slide1", "title": "Slide 1", "master": "m1", "duration_ms": 3000 },
                { "id": "slide2", "title": "Slide 2", "master": "m1", "duration_ms": 3000 }
            ]
        }]
    });
    let c = Course::from_json(&json.to_string()).expect("course");

    for slide_id in ["slide1", "slide2"] {
        let img0 = render_slide_at(&c, slide_id, 0);
        assert_eq!(
            px(&img0, 200, 200),
            [255, 255, 255, 255],
            "{slide_id}: master box should be invisible at t=0"
        );

        let img600 = render_slide_at(&c, slide_id, 600);
        assert_eq!(
            px(&img600, 200, 200),
            [255, 0, 0, 255],
            "{slide_id}: master box should be fully visible at t=600"
        );
    }

    // Master object is excluded from focus_order_candidates on slide1.
    let focus = focus_order_candidates(&c.scenes[0].slides[0]);
    assert!(
        !focus.iter().any(|f| f.id == "m_box"),
        "master object must not appear in focus_order_candidates"
    );
}

#[test]
fn m49_s4_master_exit_hides_object_after_exit() {
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "m49-master-exit-render",
        "title": "Master Exit",
        "slide_size": { "w": 1280, "h": 720 },
        "masters": [{
            "id": "m1",
            "title": "Master",
            "background": "#FFFFFF",
            "duration_ms": 5000,
            "objects": [{
                "id": "m_box",
                "kind": { "type": "shape", "shape": "rectangle", "fill": "#0000FF" },
                "frame": { "x": 100, "y": 100, "w": 200, "h": 200 },
                "timing": { "start_ms": 0, "end_ms": 1500 },
                "exit": { "effect": { "type": "fade" }, "duration_ms": 500, "easing": "linear" }
            }]
        }],
        "scenes": [{
            "id": "sc1",
            "title": "Scene",
            "slides": [
                { "id": "slide1", "title": "Slide 1", "master": "m1", "duration_ms": 3000 }
            ]
        }]
    });
    let c = Course::from_json(&json.to_string()).expect("course");

    let img_before = render_slide_at(&c, "slide1", 500);
    assert_eq!(px(&img_before, 200, 200), [0, 0, 255, 255]);

    let img_after = render_slide_at(&c, "slide1", 1600);
    assert_eq!(px(&img_after, 200, 200), [255, 255, 255, 255]);
}

#[test]
fn m49_s4_master_group_animates_as_one_unit() {
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "m49-master-group",
        "title": "Master Group",
        "slide_size": { "w": 1280, "h": 720 },
        "masters": [{
            "id": "m1",
            "title": "Master",
            "background": "#FFFFFF",
            "duration_ms": 5000,
            "groups": [{
                "id": "mg1",
                "entrance": { "effect": { "type": "zoom" }, "duration_ms": 1000, "easing": "linear" }
            }],
            "objects": [
                { "id": "m_left", "group": "mg1", "kind": { "type": "shape", "shape": "rectangle", "fill": "#FF0000" }, "frame": { "x": 200, "y": 260, "w": 200, "h": 200 } },
                { "id": "m_right", "group": "mg1", "kind": { "type": "shape", "shape": "rectangle", "fill": "#0000FF" }, "frame": { "x": 600, "y": 260, "w": 200, "h": 200 } }
            ]
        }],
        "scenes": [{
            "id": "sc1",
            "title": "Scene",
            "slides": [
                { "id": "slide1", "title": "Slide 1", "master": "m1", "duration_ms": 3000 }
            ]
        }]
    });
    let c = Course::from_json(&json.to_string()).expect("course");

    // At t=500ms (50% zoom): group scales around (500, 360), so resting centres (300,360) and
    // (700,360) are white while scaled positions (400,360) and (600,360) are tinted red/blue.
    let img = render_slide_at(&c, "slide1", 500);
    assert_eq!(px(&img, 300, 360), [255, 255, 255, 255]);
    assert_eq!(px(&img, 700, 360), [255, 255, 255, 255]);
    let left_p = px(&img, 400, 360);
    assert!(
        left_p[0] > 240 && left_p[1] < 200 && left_p[2] < 200,
        "expected reddish at (400,360), got {left_p:?}"
    );
    let right_p = px(&img, 600, 360);
    assert!(
        right_p[2] > 240 && right_p[0] < 200 && right_p[1] < 200,
        "expected bluish at (600,360), got {right_p:?}"
    );

    // Every master DrawItem carries Hit::None and a non-colliding `master:` GroupAnim id.
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    p.handle(&Input::Tick { ms: 500 });
    let items = compose(&p);
    assert_eq!(items.len(), 2);
    for it in &items {
        assert_eq!(it.hit, Hit::None);
        let ga = it.group_anim.as_ref().expect("master group_anim");
        assert_eq!(ga.id, "master:mg1");
    }
}

#[test]
fn m49_s4_legacy_master_without_animations_renders_identically() {
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "m49-master-legacy",
        "title": "Legacy Master",
        "slide_size": { "w": 1280, "h": 720 },
        "masters": [{
            "id": "m1",
            "title": "Master",
            "background": "#FFFFFF",
            "duration_ms": 5000,
            "objects": [{
                "id": "m_box",
                "kind": { "type": "shape", "shape": "rectangle", "fill": "#00AA00" },
                "frame": { "x": 100, "y": 100, "w": 200, "h": 200 }
            }]
        }],
        "scenes": [{
            "id": "sc1",
            "title": "Scene",
            "slides": [
                { "id": "slide1", "title": "Slide 1", "master": "m1", "duration_ms": 3000 }
            ]
        }]
    });
    let c = Course::from_json(&json.to_string()).expect("course");
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);

    let items = compose(&p);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].hit, Hit::None);
    assert_eq!(items[0].anim, AnimLook::IDENTITY);
    assert!(items[0].group_anim.is_none());

    let img = render_slide_at(&c, "slide1", 0);
    assert_eq!(px(&img, 200, 200), [0, 170, 0, 255]);
}

#[test]
fn m49_s4_slide_transition_with_animated_master_renders_cleanly() {
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "m49-master-transition",
        "title": "Master Transition",
        "slide_size": { "w": 800, "h": 600 },
        "masters": [{
            "id": "m1",
            "title": "Master",
            "background": "#FFFFFF",
            "duration_ms": 5000,
            "objects": [{
                "id": "m_box",
                "kind": { "type": "shape", "shape": "rectangle", "fill": "#FF0000" },
                "frame": { "x": 50, "y": 50, "w": 100, "h": 100 },
                "entrance": { "effect": { "type": "fade" }, "duration_ms": 400, "easing": "linear" }
            }]
        }],
        "scenes": [{
            "id": "sc1",
            "title": "Scene",
            "slides": [
                { "id": "s1", "title": "Slide 1", "master": "m1", "duration_ms": 3000 },
                {
                    "id": "s2",
                    "title": "Slide 2",
                    "master": "m1",
                    "duration_ms": 3000,
                    "transition": { "type": "push", "from": "right", "duration_ms": 500 }
                }
            ]
        }]
    });
    let c = Course::from_json(&json.to_string()).expect("course");
    let mut player = Player::new(&c).expect("player");
    player.handle(&Input::Start);
    player.handle(&Input::Tick { ms: 500 });

    let ctx = egui::Context::default();
    let mut view = SlideView::new();
    let run_frame = |view: &mut SlideView, player: &Player<'_>, time: f64| {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            time: Some(time),
            ..Default::default()
        };
        let mut out = ctx.run_ui(raw, |ui| {
            let _ = view.show(ui, player, None);
        });
        out.textures_delta.clear();
    };

    run_frame(&mut view, &player, 0.0);
    player.handle(&Input::Jump { slide: "s2".into() });
    run_frame(&mut view, &player, 0.1);
    assert!(view.transition_active());

    player.handle(&Input::Tick { ms: 600 });
    run_frame(&mut view, &player, 0.8);
    assert!(!view.transition_active());
    assert_eq!(view.last_items().len(), 1);
    assert_eq!(view.last_items()[0].hit, Hit::None);
    assert!((view.last_items()[0].opacity - 1.0).abs() < 0.01);
}
