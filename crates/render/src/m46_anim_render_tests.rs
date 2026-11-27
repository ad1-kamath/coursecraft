//! M46 phase 2: the renderer draws every part of an animation sample (move, x/y scale,
//! rotation, opacity, tint, rectangular clip and shaped masks), and every registry effect ends
//! exactly at the object's resting look.

use coursecraft_course::{AnimEffect, Course, EFFECTS};
use coursecraft_logic::{ConvexPoly, Mask, ObjectSample, Tint};

use crate::compose::{Hit, apply_sample_to_items, compose_authoring};
use crate::media::{MediaFetch, MediaSource};
use crate::offscreen::{
    RenderMode, RenderOptions, RgbaImage, render_items_rgba, render_slide_rgba,
};

struct NoMedia;
impl MediaSource for NoMedia {
    fn get(&self, _src: &str) -> MediaFetch {
        MediaFetch::Missing
    }
}

const RED: [u8; 4] = [255, 0, 0, 255];
const WHITE: [u8; 4] = [255, 255, 255, 255];

/// A 1280×720 slide with a 200×200 red square centred at (640, 360), with an optional
/// entrance.
fn course(entrance: Option<serde_json::Value>) -> Course {
    let mut obj = serde_json::json!({
        "id": "box",
        "kind": { "type": "shape", "shape": "rectangle", "fill": "#FF0000" },
        "frame": { "x": 540, "y": 260, "w": 200, "h": 200 }
    });
    if let Some(e) = entrance {
        obj["entrance"] = e;
    }
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "m46-render",
        "title": "M46 render",
        "slide_size": { "w": 1280, "h": 720 },
        "scenes": [{
            "id": "s1",
            "title": "Scene",
            "slides": [{
                "id": "slide1",
                "title": "Slide",
                "background": "#FFFFFF",
                "duration_ms": 3000,
                "objects": [obj]
            }]
        }]
    });
    Course::from_json(&json.to_string()).expect("valid course")
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

/// Render the square with `sample` applied, at 1280 px wide (1 slide px = 1 image px).
fn render_with(sample: &ObjectSample) -> RgbaImage {
    let c = course(None);
    let slide = c.slide("slide1").expect("slide");
    let mut items = compose_authoring(&c, slide, None);
    let at = items
        .iter()
        .position(|i| i.hit == Hit::Object("box".into()))
        .expect("box item");
    apply_sample_to_items(&mut items[at..=at], sample);
    render_items_rgba(&c, slide, 1280, items, &NoMedia, None).expect("render")
}

fn player_at(c: &Course, ms: u32) -> RgbaImage {
    let opts = RenderOptions {
        width: 1280,
        mode: RenderMode::Player,
        layers: Vec::new(),
        at_ms: Some(ms),
    };
    render_slide_rgba(c, "slide1", &opts, &NoMedia).expect("render")
}

#[test]
fn identity_sample_draws_the_resting_square() {
    let img = render_with(&ObjectSample::IDENTITY);
    assert_eq!(px(&img, 640, 360), RED);
    assert_eq!(px(&img, 545, 265), RED);
    assert_eq!(px(&img, 530, 360), WHITE);
}

#[test]
fn rotation_turns_about_the_centre() {
    let img = render_with(&ObjectSample {
        rotation_deg: 45.0,
        ..ObjectSample::IDENTITY
    });
    // The corner of the resting square is outside the diamond…
    assert_eq!(px(&img, 548, 268), WHITE);
    // …and the diamond's top point (≈141 px above the centre) is where nothing was before.
    assert_eq!(px(&img, 640, 230), RED);
    assert_eq!(px(&img, 640, 360), RED);
}

#[test]
fn scale_is_per_axis() {
    let img = render_with(&ObjectSample {
        scale: (0.5, 1.0),
        ..ObjectSample::IDENTITY
    });
    assert_eq!(px(&img, 640 + 40, 360), RED);
    assert_eq!(px(&img, 640 + 60, 360), WHITE);
    assert_eq!(px(&img, 640, 360 + 90), RED);
    let tiny = render_with(&ObjectSample {
        scale: (0.0, 1.0),
        ..ObjectSample::IDENTITY
    });
    assert_eq!(px(&tiny, 640, 360), WHITE);
}

#[test]
fn tint_recolours_the_object() {
    let img = render_with(&ObjectSample {
        tint: Some(Tint {
            rgb: [0, 0, 255],
            amount: 1.0,
        }),
        ..ObjectSample::IDENTITY
    });
    assert_eq!(px(&img, 640, 360), [0, 0, 255, 255]);
    let half = render_with(&ObjectSample {
        tint: Some(Tint {
            rgb: [0, 0, 255],
            amount: 0.5,
        }),
        ..ObjectSample::IDENTITY
    });
    let [r, g, b, _] = px(&half, 640, 360);
    assert!(
        r.abs_diff(128) <= 2 && g == 0 && b.abs_diff(128) <= 2,
        "{r} {g} {b}"
    );
}

#[test]
fn masks_reveal_only_inside_their_polygons() {
    // Left half, as a rectangle polygon.
    let left = render_with(&ObjectSample {
        mask: Some(Mask::new(vec![ConvexPoly::rect(0.0, 0.0, 0.5, 1.0)])),
        ..ObjectSample::IDENTITY
    });
    assert_eq!(px(&left, 600, 360), RED);
    assert_eq!(px(&left, 680, 360), WHITE);

    // A diamond: its corners are hidden, its centre shows.
    let diamond = render_with(&ObjectSample {
        mask: Some(Mask::new(vec![ConvexPoly(vec![
            (0.5, 0.0),
            (1.0, 0.5),
            (0.5, 1.0),
            (0.0, 0.5),
        ])])),
        ..ObjectSample::IDENTITY
    });
    assert_eq!(px(&diamond, 640, 360), RED);
    assert_eq!(px(&diamond, 548, 268), WHITE);
    assert_eq!(px(&diamond, 732, 452), WHITE);

    // Two bars with a gap between them.
    let bars = render_with(&ObjectSample {
        mask: Some(Mask::new(vec![
            ConvexPoly::rect(0.0, 0.0, 1.0, 0.25),
            ConvexPoly::rect(0.0, 0.5, 1.0, 0.75),
        ])),
        ..ObjectSample::IDENTITY
    });
    assert_eq!(px(&bars, 640, 260 + 25), RED);
    assert_eq!(px(&bars, 640, 260 + 75), WHITE);
    assert_eq!(px(&bars, 640, 260 + 125), RED);
    assert_eq!(px(&bars, 640, 260 + 175), WHITE);

    // An empty mask hides everything.
    let none = render_with(&ObjectSample {
        mask: Some(Mask::new(Vec::new())),
        ..ObjectSample::IDENTITY
    });
    assert_eq!(px(&none, 640, 360), WHITE);
}

#[test]
fn mask_turns_with_the_object() {
    // Left half of the frame, then a quarter turn clockwise: the top half shows.
    let img = render_with(&ObjectSample {
        rotation_deg: 90.0,
        mask: Some(Mask::new(vec![ConvexPoly::rect(0.0, 0.0, 0.5, 1.0)])),
        ..ObjectSample::IDENTITY
    });
    assert_eq!(px(&img, 640, 300), RED);
    assert_eq!(px(&img, 640, 420), WHITE);
}

#[test]
fn hostile_samples_do_not_break_the_render() {
    for sample in [
        ObjectSample {
            scale: (f32::NAN, f32::INFINITY),
            rotation_deg: f32::NAN,
            offset: (f32::NAN, 1e30),
            ..ObjectSample::IDENTITY
        },
        ObjectSample {
            mask: Some(Mask::new(vec![ConvexPoly(vec![
                (f32::NAN, 0.0),
                (1.0, 1.0),
                (0.0, 1.0),
            ])])),
            tint: Some(Tint {
                rgb: [0, 0, 0],
                amount: f32::NAN,
            }),
            ..ObjectSample::IDENTITY
        },
    ] {
        let img = render_with(&sample.clone().sanitized());
        assert_eq!(img.width, 1280);
        let raw = render_with(&sample);
        assert_eq!(raw.width, 1280);
    }
}

/// Every registry effect, in every direction it takes, starts hidden or away, is somewhere in
/// between at the midpoint, and ends pixel-identical to the object without an animation.
#[test]
fn every_effect_ends_at_the_resting_look() {
    let rest = player_at(&course(None), 2000);
    assert_eq!(px(&rest, 640, 360), RED);
    for def in EFFECTS {
        let dirs: Vec<Option<coursecraft_course::Direction>> = match def.direction() {
            Some((_, allowed)) => allowed.iter().copied().map(Some).collect(),
            None => vec![None],
        };
        for dir in dirs {
            let effect = match dir {
                Some(d) => AnimEffect::with_from(def.id, d),
                None => AnimEffect::new(def.id),
            };
            let entrance = serde_json::json!({
                "effect": serde_json::to_value(&effect).expect("effect json"),
                "duration_ms": 1000,
                "easing": "linear"
            });
            let c = course(Some(entrance));
            let start = player_at(&c, 0);
            assert_ne!(
                px(&start, 640, 360),
                RED,
                "`{}` {dir:?} is already in place at the start",
                def.id
            );
            let mid = player_at(&c, 500);
            assert_eq!(mid.width, 1280);
            let end = player_at(&c, 2000);
            assert!(
                end.pixels == rest.pixels,
                "`{}` {dir:?} does not end at the resting look",
                def.id
            );
        }
    }
}

/// By paragraph: half way through a two-paragraph fade, the first paragraph shows and the
/// second doesn't yet.
#[test]
fn by_paragraph_reveals_the_first_paragraph_first() {
    let json = serde_json::json!({
        "schema_version": 1, "id": "m46-seq", "title": "Seq",
        "slide_size": { "w": 1280, "h": 720 },
        "scenes": [{ "id": "s1", "title": "Scene", "slides": [{
            "id": "slide1", "title": "Slide", "background": "#FFFFFF", "duration_ms": 3000,
            "objects": [{
                "id": "t",
                "kind": { "type": "text", "text": "MMMMMMMM\nMMMMMMMM", "style": { "size": 40.0, "color": "#000000" } },
                "frame": { "x": 100, "y": 100, "w": 600, "h": 200 },
                "entrance": { "effect": { "type": "fade" }, "duration_ms": 1000, "easing": "linear", "sequence": "by_paragraph" }
            }]
        }] }]
    });
    let c = Course::from_json(&json.to_string()).expect("valid course");
    let dark_rows = |img: &RgbaImage, y0: u32, y1: u32| {
        (y0..y1)
            .filter(|&y| (100..700).any(|x| px(img, x, y)[0] < 128))
            .count()
    };
    let half = player_at(&c, 480);
    let end = player_at(&c, 2000);
    // Find the two lines at rest, then check the half-way frame line by line.
    let rows: Vec<u32> = (100..300)
        .filter(|&y| (100..700).any(|x| px(&end, x, y)[0] < 128))
        .collect();
    assert!(rows.len() > 10, "text drawn at rest");
    let mid = (rows[0] + rows[rows.len() - 1]) / 2;
    assert!(dark_rows(&half, 100, mid) > 5, "first paragraph shows");
    assert_eq!(dark_rows(&half, mid, 300), 0, "second paragraph not yet");
}

#[test]
fn rectangular_clip_turns_with_the_object() {
    // Left half of the frame, then a quarter turn clockwise: the top half shows. A clip
    // that stayed axis-aligned on the screen would still show the left half.
    let img = render_with(&ObjectSample {
        rotation_deg: 90.0,
        clip: Some(coursecraft_logic::ClipFrac {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 0.5,
            max_y: 1.0,
        }),
        ..ObjectSample::IDENTITY
    });
    assert_eq!(px(&img, 600, 300), RED);
    assert_eq!(px(&img, 600, 420), WHITE);
    assert_eq!(px(&img, 680, 420), WHITE);
}
