//! M49 step s2: group animations render as one unit (shared frame for scale/zoom/wipe,
//! unified offscreen opacity without darker overlap, and child animations inside the group).

use coursecraft_course::Course;

use crate::compose::{Hit, compose};
use crate::media::{MediaFetch, MediaSource};
use crate::offscreen::{RenderMode, RenderOptions, RgbaImage, render_slide_rgba};

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

fn render_at(c: &Course, ms: u32) -> RgbaImage {
    let opts = RenderOptions {
        width: 1280,
        mode: RenderMode::Player,
        layers: Vec::new(),
        at_ms: Some(ms),
    };
    render_slide_rgba(c, "slide1", &opts, &NoMedia).expect("render")
}

#[test]
fn m49_s2_group_zoom_scales_around_group_centre() {
    // Two shapes: left [200..400, 260..460], right [600..800, 260..460].
    // Combined group box: [200..800, 260..460], centre = (500, 360).
    // At 50% of linear zoom entrance (scale = 0.5):
    // Group scales around (500, 360), so:
    // - left shape moves to [350..450, 310..410]
    // - right shape moves to [550..650, 310..410]
    // Whereas per-child zoom would keep left centred at (300, 360) -> [250..350].
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "m49-zoom",
        "title": "Group Zoom",
        "slide_size": { "w": 1280, "h": 720 },
        "scenes": [{
            "id": "s1",
            "title": "Scene",
            "slides": [{
                "id": "slide1",
                "title": "Slide",
                "background": "#FFFFFF",
                "duration_ms": 3000,
                "groups": [{
                    "id": "g1",
                    "entrance": { "effect": { "type": "zoom" }, "duration_ms": 1000, "easing": "linear" }
                }],
                "objects": [
                    { "id": "left", "group": "g1", "kind": { "type": "shape", "shape": "rectangle", "fill": "#FF0000" }, "frame": { "x": 200, "y": 260, "w": 200, "h": 200 } },
                    { "id": "right", "group": "g1", "kind": { "type": "shape", "shape": "rectangle", "fill": "#0000FF" }, "frame": { "x": 600, "y": 260, "w": 200, "h": 200 } }
                ]
            }]
        }]
    });
    let c = Course::from_json(&json.to_string()).expect("course");
    let img = render_at(&c, 500);

    // Individual resting centres (300, 360) and (700, 360) must be white (moved inward!).
    assert_eq!(px(&img, 300, 360), [255, 255, 255, 255]);
    assert_eq!(px(&img, 700, 360), [255, 255, 255, 255]);

    // Scaled positions (400, 360) and (600, 360) must be tinted red/blue (blended with white at 50% opacity).
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
}

#[test]
fn m49_s2_group_fade_has_no_darker_overlap() {
    // Two overlapping opaque red rectangles in a group with 1000 ms linear fade.
    // At t=500 ms (opacity = 0.5), the overlap region must have the exact same RGB as
    // each non-overlapping region (no darker double-blended overlap).
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "m49-fade",
        "title": "Group Fade",
        "slide_size": { "w": 1280, "h": 720 },
        "scenes": [{
            "id": "s1",
            "title": "Scene",
            "slides": [{
                "id": "slide1",
                "title": "Slide",
                "background": "#FFFFFF",
                "duration_ms": 3000,
                "groups": [{
                    "id": "g1",
                    "entrance": { "effect": { "type": "fade" }, "duration_ms": 1000, "easing": "linear" }
                }],
                "objects": [
                    { "id": "a", "group": "g1", "kind": { "type": "shape", "shape": "rectangle", "fill": "#FF0000" }, "frame": { "x": 200, "y": 200, "w": 300, "h": 200 } },
                    { "id": "b", "group": "g1", "kind": { "type": "shape", "shape": "rectangle", "fill": "#FF0000" }, "frame": { "x": 350, "y": 250, "w": 300, "h": 200 } }
                ]
            }]
        }]
    });
    let c = Course::from_json(&json.to_string()).expect("course");
    let img = render_at(&c, 500);

    let only_a = px(&img, 250, 220);
    let overlap = px(&img, 420, 320);
    let only_b = px(&img, 580, 400);

    // Must be faded (~[255, 128, 128]), not full red or white.
    assert!(
        only_a[1] > 100 && only_a[1] < 160,
        "expected ~50% faded red, got {only_a:?}"
    );
    for ch in 0..3 {
        assert!(
            only_a[ch].abs_diff(overlap[ch]) <= 2,
            "overlap differs from only_a at channel {ch}: {only_a:?} vs {overlap:?}"
        );
        assert!(
            only_b[ch].abs_diff(overlap[ch]) <= 2,
            "overlap differs from only_b at channel {ch}: {only_b:?} vs {overlap:?}"
        );
    }
}

#[test]
fn m49_s2_group_wipe_reveals_across_group_box() {
    // Two side-by-side shapes: [200..400] and [400..600].
    // Left-to-right wipe across the group box [200..600] at 50% reveals [200..400] (left shape)
    // and completely hides [400..600] (right shape).
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "m49-wipe",
        "title": "Group Wipe",
        "slide_size": { "w": 1280, "h": 720 },
        "scenes": [{
            "id": "s1",
            "title": "Scene",
            "slides": [{
                "id": "slide1",
                "title": "Slide",
                "background": "#FFFFFF",
                "duration_ms": 3000,
                "groups": [{
                    "id": "g1",
                    "entrance": { "effect": { "type": "wipe", "from": "left" }, "duration_ms": 1000, "easing": "linear" }
                }],
                "objects": [
                    { "id": "left", "group": "g1", "kind": { "type": "shape", "shape": "rectangle", "fill": "#FF0000" }, "frame": { "x": 200, "y": 200, "w": 200, "h": 200 } },
                    { "id": "right", "group": "g1", "kind": { "type": "shape", "shape": "rectangle", "fill": "#0000FF" }, "frame": { "x": 400, "y": 200, "w": 200, "h": 200 } }
                ]
            }]
        }]
    });
    let c = Course::from_json(&json.to_string()).expect("course");
    let img = render_at(&c, 500);

    // Left shape is fully revealed (even its right half at x=350).
    assert_eq!(px(&img, 350, 300), [255, 0, 0, 255]);
    // Right shape is still hidden (white background).
    assert_eq!(px(&img, 450, 300), [255, 255, 255, 255]);
}

#[test]
fn m49_s2_child_fly_in_composes_inside_group_fade() {
    // Child "a" has its own fly_in from top (1000 ms); group "g1" has a fade (1000 ms).
    // At t=500 ms, child "a" is halfway down from off-slide-top toward y=300 (so it is above y=300)
    // AND faded to 50% opacity by the group.
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "m49-child-in-group",
        "title": "Child In Group",
        "slide_size": { "w": 1280, "h": 720 },
        "scenes": [{
            "id": "s1",
            "title": "Scene",
            "slides": [{
                "id": "slide1",
                "title": "Slide",
                "background": "#FFFFFF",
                "duration_ms": 3000,
                "groups": [{
                    "id": "g1",
                    "entrance": { "effect": { "type": "fade" }, "duration_ms": 1000, "easing": "linear" }
                }],
                "objects": [
                    {
                        "id": "a",
                        "group": "g1",
                        "kind": { "type": "shape", "shape": "rectangle", "fill": "#FF0000" },
                        "frame": { "x": 200, "y": 300, "w": 200, "h": 200 },
                        "entrance": { "effect": { "type": "fly_in", "from": "top" }, "duration_ms": 1000, "easing": "linear" }
                    },
                    {
                        "id": "b",
                        "group": "g1",
                        "kind": { "type": "shape", "shape": "rectangle", "fill": "#0000FF" },
                        "frame": { "x": 600, "y": 300, "w": 200, "h": 200 }
                    }
                ]
            }]
        }]
    });
    let c = Course::from_json(&json.to_string()).expect("course");

    let mut player = coursecraft_logic::Player::new(&c).expect("player");
    player.handle(&coursecraft_logic::Input::Start);
    player.handle(&coursecraft_logic::Input::Tick { ms: 500 });
    let items = compose(&player);
    let item_a = items
        .iter()
        .find(|it| it.hit == Hit::Object("a".into()))
        .expect("item a");
    // Child fly-in moved it above its resting y=300.
    assert!(
        item_a.rect.min.y < 250.0,
        "expected child fly-in y < 250, got {:?}",
        item_a.rect
    );
    let ga = item_a.group_anim.as_ref().expect("group anim");
    assert!((ga.sample.opacity - 0.5).abs() < 0.05);

    let img = render_at(&c, 500);
    // At the translated child position (x=300, y=item_a.rect.center().y), colour should be ~50% faded red.
    let cy = item_a.rect.center().y.round() as u32;
    let p = px(&img, 300, cy);
    assert!(
        p[0] > 240 && p[1] > 100 && p[1] < 160,
        "expected ~50% faded red at (300,{cy}), got {p:?}"
    );
}

#[test]
fn m49_s2_legacy_per_child_animations_unchanged() {
    // Plain authoring group (no Group record in `groups`) with per-child fade entrances:
    // every DrawItem has `group_anim: None` and overlapping children blend individually.
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "m49-legacy",
        "title": "Legacy",
        "slide_size": { "w": 1280, "h": 720 },
        "scenes": [{
            "id": "s1",
            "title": "Scene",
            "slides": [{
                "id": "slide1",
                "title": "Slide",
                "background": "#FFFFFF",
                "duration_ms": 3000,
                "objects": [
                    { "id": "a", "group": "g_plain", "kind": { "type": "shape", "shape": "rectangle", "fill": "#FF0000" }, "frame": { "x": 200, "y": 200, "w": 300, "h": 200 }, "entrance": { "effect": { "type": "fade" }, "duration_ms": 1000, "easing": "linear" } },
                    { "id": "b", "group": "g_plain", "kind": { "type": "shape", "shape": "rectangle", "fill": "#FF0000" }, "frame": { "x": 350, "y": 250, "w": 300, "h": 200 }, "entrance": { "effect": { "type": "fade" }, "duration_ms": 1000, "easing": "linear" } }
                ]
            }]
        }]
    });
    let c = Course::from_json(&json.to_string()).expect("course");
    let mut player = coursecraft_logic::Player::new(&c).expect("player");
    player.handle(&coursecraft_logic::Input::Start);
    player.handle(&coursecraft_logic::Input::Tick { ms: 500 });
    let items = compose(&player);
    assert!(items.iter().all(|it| it.group_anim.is_none()));
}
