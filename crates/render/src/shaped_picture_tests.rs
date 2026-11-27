//! Pictures inside shapes: a shaped image honours its crop, and a picture fill can cover its
//! shape (`fill`) instead of being stretched.

use coursecraft_course::Course;
use egui::{Rect, pos2};

use crate::media::{MediaFetch, MediaSource};
use crate::offscreen::{RenderMode, RenderOptions, RgbaImage, encode_png, render_slide_rgba};
use crate::paint::{FULL_UV, picture_uv_window};

const RED: [u8; 4] = [220, 30, 30, 255];
const GREEN: [u8; 4] = [30, 200, 30, 255];
const BLUE: [u8; 4] = [30, 30, 220, 255];

/// A 40x20 picture whose colour depends on the column only.
struct Columns(fn(u32) -> [u8; 4]);

impl MediaSource for Columns {
    fn get(&self, src: &str) -> MediaFetch {
        if src != "media/pic.png" {
            return MediaFetch::Missing;
        }
        let (w, h) = (40u32, 20u32);
        let mut pixels = Vec::new();
        for _ in 0..h {
            for x in 0..w {
                pixels.extend_from_slice(&(self.0)(x));
            }
        }
        match encode_png(&RgbaImage {
            width: w,
            height: h,
            pixels,
        }) {
            Ok(bytes) => MediaFetch::Ready(std::sync::Arc::from(bytes)),
            Err(_) => MediaFetch::Missing,
        }
    }
}

fn left_red_right_blue(x: u32) -> [u8; 4] {
    if x < 20 { RED } else { BLUE }
}

fn centre_stripe(x: u32) -> [u8; 4] {
    if (16..24).contains(&x) { GREEN } else { RED }
}

fn course(object: serde_json::Value) -> Course {
    let json = serde_json::json!({
        "schema_version": 1, "id": "sp", "title": "Shaped pictures",
        "slide_size": { "w": 1280, "h": 720 },
        "scenes": [{ "id": "s1", "title": "Scene", "slides": [{
            "id": "slide1", "title": "Slide", "background": "#FFFFFF", "duration_ms": 3000,
            "objects": [object]
        }] }]
    });
    Course::from_json(&json.to_string()).expect("valid course")
}

fn render(c: &Course, media: &Columns) -> RgbaImage {
    let opts = RenderOptions {
        width: 1280,
        mode: RenderMode::Player,
        layers: Vec::new(),
        at_ms: Some(1000),
    };
    render_slide_rgba(c, "slide1", &opts, media).expect("render")
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

fn near(a: [u8; 4], b: [u8; 4]) -> bool {
    (0..3).all(|i| a[i].abs_diff(b[i]) < 40)
}

fn image(shape: Option<&str>, crop: Option<serde_json::Value>) -> Course {
    let mut kind = serde_json::json!({ "type": "image", "src": "media/pic.png" });
    if let Some(shape) = shape {
        kind["shape"] = serde_json::json!(shape);
    }
    if let Some(crop) = crop {
        kind["crop"] = crop;
    }
    course(serde_json::json!({
        "id": "img", "kind": kind,
        "frame": { "x": 100, "y": 100, "w": 200, "h": 200 }
    }))
}

/// Points well inside a 200x200 ellipse at (100, 100).
const INSIDE: [(u32, u32); 3] = [(150, 200), (200, 200), (250, 200)];

#[test]
fn a_shaped_image_honours_its_crop() {
    let left_half = serde_json::json!({ "x": 0.0, "y": 0.0, "w": 0.45, "h": 1.0 });
    let media = Columns(left_red_right_blue);
    let plain = render(&image(None, Some(left_half.clone())), &media);
    let shaped = render(&image(Some("ellipse"), Some(left_half)), &media);
    for (x, y) in INSIDE {
        assert!(near(px(&plain, x, y), RED), "plain crop is red at {x},{y}");
        assert!(
            near(px(&shaped, x, y), px(&plain, x, y)),
            "shaped crop matches the plain crop at {x},{y}: {:?} vs {:?}",
            px(&shaped, x, y),
            px(&plain, x, y)
        );
    }
    // The outline of the shape still cuts the corners.
    assert!(near(px(&shaped, 104, 104), [255, 255, 255, 255]));
}

#[test]
fn a_shaped_image_without_a_crop_still_shows_the_whole_picture() {
    let media = Columns(left_red_right_blue);
    let shaped = render(&image(Some("ellipse"), None), &media);
    assert!(near(px(&shaped, 150, 200), RED));
    assert!(near(px(&shaped, 250, 200), BLUE));
}

fn shape_with_picture(mode: &str) -> Course {
    course(serde_json::json!({
        "id": "shp",
        "kind": { "type": "shape", "shape": "ellipse", "fill": "#000000",
                  "fill_style": { "type": "picture", "src": "media/pic.png", "mode": mode } },
        "frame": { "x": 100, "y": 100, "w": 200, "h": 200 }
    }))
}

#[test]
fn fill_mode_covers_the_shape_and_keeps_the_picture_proportions() {
    let media = Columns(centre_stripe);
    let stretched = render(&shape_with_picture("stretch"), &media);
    let covered = render(&shape_with_picture("fill"), &media);
    // The stripe is the middle fifth of a 2:1 picture. Stretched, it keeps that width in the
    // square shape. Covered, the square shows the middle half of the picture, so the stripe
    // is wider (30 % to 70 % of the width).
    assert!(near(px(&stretched, 170, 200), RED));
    assert!(near(px(&covered, 170, 200), GREEN));
    assert!(near(px(&stretched, 200, 200), GREEN));
    assert!(near(px(&covered, 200, 200), GREEN));
    assert!(near(px(&covered, 150, 200), RED));
    assert!(near(px(&covered, 250, 200), RED));
}

#[test]
fn the_default_mode_stays_stretch() {
    let json = serde_json::json!({ "type": "picture", "src": "a.png" });
    let style: coursecraft_course::FillStyle = serde_json::from_value(json).expect("style");
    match style {
        coursecraft_course::FillStyle::Picture { mode, .. } => {
            assert_eq!(mode, coursecraft_course::PictureFillMode::Stretch);
        }
        other => panic!("not a picture: {other:?}"),
    }
    let fill: coursecraft_course::FillStyle = serde_json::from_value(
        serde_json::json!({ "type": "picture", "src": "a.png", "mode": "fill" }),
    )
    .expect("fill");
    assert_eq!(
        serde_json::to_value(&fill).expect("json")["mode"],
        serde_json::json!("fill")
    );
}

#[test]
fn the_cover_window_is_centred_and_matches_the_shape_proportions() {
    // A 2:1 picture in a square keeps the middle half of its width.
    let w = picture_uv_window(FULL_UV, (40.0, 20.0), (100.0, 100.0), true);
    assert!((w.min.x - 0.25).abs() < 1e-5 && (w.max.x - 0.75).abs() < 1e-5);
    assert!(w.min.y == 0.0 && w.max.y == 1.0);
    // A tall shape from a wide picture keeps even less of the width.
    let w = picture_uv_window(FULL_UV, (40.0, 20.0), (50.0, 100.0), true);
    assert!((w.width() - 0.25).abs() < 1e-5);
    // A wide shape from a square picture keeps the middle of its height.
    let w = picture_uv_window(FULL_UV, (20.0, 20.0), (100.0, 50.0), true);
    assert!((w.height() - 0.5).abs() < 1e-5 && (w.center().y - 0.5).abs() < 1e-5);
    // Without cover, or with an unusable size, the window is unchanged.
    let crop = Rect::from_min_max(pos2(0.1, 0.2), pos2(0.6, 0.9));
    assert_eq!(
        picture_uv_window(crop, (40.0, 20.0), (100.0, 100.0), false),
        crop
    );
    assert_eq!(
        picture_uv_window(crop, (40.0, 0.0), (100.0, 100.0), true),
        crop
    );
}
