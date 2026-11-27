use coursecraft_course::Course;

use crate::media::{MediaFetch, MediaSource};
use crate::offscreen::{RenderOptions, RgbaImage, render_slide_rgba};

struct NoMedia;
impl MediaSource for NoMedia {
    fn get(&self, _src: &str) -> MediaFetch {
        MediaFetch::Missing
    }
}

fn render(course: &Course) -> RgbaImage {
    render_slide_rgba(
        course,
        "slide-1",
        &RenderOptions {
            width: 1280,
            ..Default::default()
        },
        &NoMedia,
    )
    .expect("render slide")
}

fn pixel(img: &RgbaImage, x: u32, y: u32) -> [u8; 4] {
    let idx = ((y * img.width + x) * 4) as usize;
    [
        img.pixels[idx],
        img.pixels[idx + 1],
        img.pixels[idx + 2],
        img.pixels[idx + 3],
    ]
}

fn is_pure_white(px: [u8; 4]) -> bool {
    px == [255, 255, 255, 255]
}

fn has_red_tint(px: [u8; 4]) -> bool {
    px[0] > px[1].saturating_add(10) && px[0] > px[2].saturating_add(10)
}

fn count_red_pixels_in(img: &RgbaImage, x0: u32, y0: u32, x1: u32, y1: u32) -> usize {
    (y0..=y1)
        .flat_map(|y| (x0..=x1).map(move |x| pixel(img, x, y)))
        .filter(|&px| has_red_tint(px))
        .count()
}

fn all_white(img: &RgbaImage) -> bool {
    img.pixels
        .as_chunks::<4>()
        .0
        .iter()
        .all(|&c| c == [255, 255, 255, 255])
}

fn make_rect_course(
    fill: &str,
    outline: Option<(&str, f32)>,
    shadow: Option<serde_json::Value>,
    glow: Option<serde_json::Value>,
) -> Course {
    let mut kind = serde_json::json!({
        "type": "shape",
        "shape": "rectangle",
        "fill": fill,
    });
    if let Some((color, width)) = outline {
        kind["outline"] = serde_json::json!({ "color": color, "width": width });
    }
    let mut obj = serde_json::json!({
        "id": "obj-1",
        "frame": { "x": 200, "y": 200, "w": 200, "h": 120 },
        "kind": kind,
    });
    if let Some(s) = shadow {
        obj["shadow"] = s;
    }
    if let Some(g) = glow {
        obj["glow"] = g;
    }
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "m48-s4",
        "title": "M48 S4",
        "slide_size": { "w": 1280, "h": 720 },
        "scenes": [{
            "id": "sc1",
            "title": "S",
            "slides": [{
                "id": "slide-1",
                "title": "S1",
                "background": "#FFFFFF",
                "duration_ms": 5000,
                "objects": [obj]
            }]
        }]
    })
    .to_string();
    Course::from_json(&json).expect("valid course json")
}

fn red_shadow() -> serde_json::Value {
    serde_json::json!({
        "color": "#FF0000FF",
        "offset_x": 16.0,
        "offset_y": 16.0,
        "blur": 4.0
    })
}

fn red_glow() -> serde_json::Value {
    serde_json::json!({
        "color": "#FF0000FF",
        "size": 12.0
    })
}

#[test]
fn m48_s4_01_rect_shadow_fill_on_fills_silhouette() {
    let course = make_rect_course("#2F6FEB", None, Some(red_shadow()), None);
    let img = render(&course);
    // Shadow appears outside bottom-right edge (rect is 200..400 x 200..320, offset +16,+16),
    // including the filled silhouette region between the rect edge (400) and offset edge (416).
    let shadow_count = count_red_pixels_in(&img, 402, 220, 420, 340);
    assert!(
        shadow_count > 0,
        "expected red shadow pixels outside right edge"
    );
    assert!(
        has_red_tint(pixel(&img, 406, 260)),
        "expected filled shadow silhouette at (406, 260), got {:?}",
        pixel(&img, 406, 260)
    );
    // Interior centre is filled with #2F6FEB.
    let center = pixel(&img, 300, 260);
    assert!(
        center[2] > center[0] && center[2] > 200,
        "expected blue fill at center, got {center:?}"
    );
}

#[test]
fn m48_s4_02_rect_shadow_fill_off_outline_on_follows_outline_only() {
    let course = make_rect_course(
        "#00000000",
        Some(("#111111", 4.0)),
        Some(red_shadow()),
        None,
    );
    let img = render(&course);
    // Shadow pixels appear near the offset outline edges (e.g. around x=416, y=220..336
    // and along the offset left/top strokes inside the transparent rect at x=216 / y=216).
    let shadow_count = count_red_pixels_in(&img, 412, 220, 422, 338);
    assert!(
        shadow_count > 0,
        "expected red outline shadow pixels near offset right edge"
    );
    assert!(
        count_red_pixels_in(&img, 212, 240, 220, 300) > 0,
        "expected red outline shadow pixels along offset left edge"
    );
    // Gap between rect right edge (400) and offset right outline (416) stays pure white.
    assert!(
        is_pure_white(pixel(&img, 406, 260)),
        "expected pure white gap at (406, 260) for outline-only shadow, got {:?}",
        pixel(&img, 406, 260)
    );
    // Interior centre remains pure white (slide background).
    assert!(
        is_pure_white(pixel(&img, 300, 260)),
        "expected pure white interior at (300, 260), got {:?}",
        pixel(&img, 300, 260)
    );
}

#[test]
fn m48_s4_03_rect_shadow_fill_off_outline_off_draws_no_shadow() {
    let course = make_rect_course("#00000000", None, Some(red_shadow()), None);
    let img = render(&course);
    assert!(
        all_white(&img),
        "expected entire slide to remain pure white"
    );
}

#[test]
fn m48_s4_04_rect_glow_fill_on_draws_glow() {
    let course = make_rect_course("#2F6FEB", None, None, Some(red_glow()));
    let img = render(&course);
    let glow_count = count_red_pixels_in(&img, 401, 200, 415, 320);
    assert!(
        glow_count > 0,
        "expected red glow pixels outside right edge"
    );
}

#[test]
fn m48_s4_05_rect_glow_fill_off_outline_on_follows_outline_only() {
    let course = make_rect_course("#00000000", Some(("#111111", 4.0)), None, Some(red_glow()));
    let img = render(&course);
    let outside_glow = count_red_pixels_in(&img, 403, 210, 415, 310);
    let inside_glow = count_red_pixels_in(&img, 385, 210, 397, 310);
    assert!(
        outside_glow > 0,
        "expected red glow pixels just outside outline"
    );
    assert!(
        inside_glow > 0,
        "expected red glow pixels just inside outline"
    );
    assert!(
        is_pure_white(pixel(&img, 300, 260)),
        "expected pure white interior at (300, 260), got {:?}",
        pixel(&img, 300, 260)
    );
}

#[test]
fn m48_s4_06_rect_glow_fill_off_outline_off_draws_no_glow() {
    let course = make_rect_course("#00000000", None, None, Some(red_glow()));
    let img = render(&course);
    assert!(
        all_white(&img),
        "expected entire slide to remain pure white"
    );
}

#[test]
fn m48_s4_07_turning_fill_back_on_restores_shadow_without_changing_shadow_field() {
    let baseline = render(&make_rect_course("#2F6FEB", None, Some(red_shadow()), None));
    let mut course = make_rect_course("#00000000", None, Some(red_shadow()), None);
    let hidden = render(&course);
    assert!(all_white(&hidden));

    // Turn fill back on without touching shadow.
    if let coursecraft_course::ObjectKind::Shape { fill, .. } =
        &mut course.scenes[0].slides[0].objects[0].kind
    {
        *fill = coursecraft_course::Color("#2F6FEB".to_owned());
    }
    let restored = render(&course);
    assert_eq!(restored.pixels, baseline.pixels);
}

#[test]
fn m48_s4_08_non_rect_shape_button_and_path_nofill_effects() {
    // (a) Oval with no fill + outline casts outline-only shadow with white centre.
    let oval_json = serde_json::json!({
        "schema_version": 1,
        "id": "m48-s4-oval",
        "title": "Oval",
        "slide_size": { "w": 1280, "h": 720 },
        "scenes": [{
            "id": "sc1",
            "title": "S",
            "slides": [{
                "id": "slide-1",
                "title": "S1",
                "background": "#FFFFFF",
                "duration_ms": 5000,
                "objects": [{
                    "id": "oval-1",
                    "frame": { "x": 200, "y": 200, "w": 200, "h": 120 },
                    "shadow": red_shadow(),
                    "kind": {
                        "type": "shape",
                        "shape": "ellipse",
                        "fill": "#00000000",
                        "outline": { "color": "#111111", "width": 4.0 }
                    }
                }]
            }]
        }]
    })
    .to_string();
    let oval_img = render(&Course::from_json(&oval_json).expect("oval course"));
    assert!(
        count_red_pixels_in(&oval_img, 404, 240, 422, 300) > 0,
        "expected red shadow pixels near offset oval right edge"
    );
    assert!(
        is_pure_white(pixel(&oval_img, 300, 260)),
        "expected pure white oval interior"
    );

    // (b) Button with transparent fill casts no box shadow/glow.
    let btn_json = serde_json::json!({
        "schema_version": 1,
        "id": "m48-s4-btn",
        "title": "Button",
        "slide_size": { "w": 1280, "h": 720 },
        "scenes": [{
            "id": "sc1",
            "title": "S",
            "slides": [{
                "id": "slide-1",
                "title": "S1",
                "background": "#FFFFFF",
                "duration_ms": 5000,
                "objects": [{
                    "id": "btn-1",
                    "frame": { "x": 200, "y": 200, "w": 200, "h": 80 },
                    "shadow": red_shadow(),
                    "glow": red_glow(),
                    "kind": {
                        "type": "button",
                        "label": "",
                        "fill": "#00000000"
                    }
                }]
            }]
        }]
    })
    .to_string();
    let btn_img = render(&Course::from_json(&btn_json).expect("button course"));
    assert!(
        all_white(&btn_img),
        "expected no red shadow/glow for transparent button"
    );
    // A filled button still casts its shadow/glow.
    let btn_filled_json = btn_json.replace("\"#00000000\"", "\"#2F6FEB\"");
    let btn_filled_img = render(&Course::from_json(&btn_filled_json).expect("filled button"));
    assert!(
        count_red_pixels_in(&btn_filled_img, 402, 210, 420, 295) > 0,
        "expected filled button to keep its shadow/glow"
    );

    // (c) Closed path with no fill + outline casts outline-only shadow and glow with white centre,
    //     and with no fill + no outline casts neither shadow nor glow.
    let path_outline_json = serde_json::json!({
        "schema_version": 1,
        "id": "m48-s4-path1",
        "title": "Path1",
        "slide_size": { "w": 1280, "h": 720 },
        "scenes": [{
            "id": "sc1",
            "title": "S",
            "slides": [{
                "id": "slide-1",
                "title": "S1",
                "background": "#FFFFFF",
                "duration_ms": 5000,
                "objects": [{
                    "id": "path-1",
                    "frame": { "x": 200, "y": 200, "w": 200, "h": 120 },
                    "shadow": red_shadow(),
                    "kind": {
                        "type": "path",
                        "points": [
                            { "x": 0.0, "y": 0.0 },
                            { "x": 1.0, "y": 0.0 },
                            { "x": 1.0, "y": 1.0 },
                            { "x": 0.0, "y": 1.0 }
                        ],
                        "closed": true,
                        "fill": "#00000000",
                        "outline": { "color": "#111111", "width": 4.0 }
                    }
                }]
            }]
        }]
    })
    .to_string();
    let path_outline_img =
        render(&Course::from_json(&path_outline_json).expect("path outline course"));
    assert!(
        count_red_pixels_in(&path_outline_img, 412, 220, 422, 338) > 0,
        "expected red shadow pixels near offset path outline"
    );
    assert!(
        is_pure_white(pixel(&path_outline_img, 300, 260)),
        "expected pure white path interior"
    );

    let path_glow_json = serde_json::json!({
        "schema_version": 1,
        "id": "m48-s4-path-glow",
        "title": "PathGlow",
        "slide_size": { "w": 1280, "h": 720 },
        "scenes": [{
            "id": "sc1",
            "title": "S",
            "slides": [{
                "id": "slide-1",
                "title": "S1",
                "background": "#FFFFFF",
                "duration_ms": 5000,
                "objects": [{
                    "id": "path-1",
                    "frame": { "x": 200, "y": 200, "w": 200, "h": 120 },
                    "glow": red_glow(),
                    "kind": {
                        "type": "path",
                        "points": [
                            { "x": 0.0, "y": 0.0 },
                            { "x": 1.0, "y": 0.0 },
                            { "x": 1.0, "y": 1.0 },
                            { "x": 0.0, "y": 1.0 }
                        ],
                        "closed": true,
                        "fill": "#00000000",
                        "outline": { "color": "#111111", "width": 4.0 }
                    }
                }]
            }]
        }]
    })
    .to_string();
    let path_glow_img = render(&Course::from_json(&path_glow_json).expect("path glow course"));
    assert!(
        count_red_pixels_in(&path_glow_img, 403, 210, 415, 310) > 0,
        "expected red glow pixels outside path outline"
    );
    assert!(
        count_red_pixels_in(&path_glow_img, 385, 210, 397, 310) > 0,
        "expected red glow pixels inside path outline"
    );
    assert!(
        is_pure_white(pixel(&path_glow_img, 300, 260)),
        "expected pure white path interior under outline-only glow"
    );

    let path_empty_json = serde_json::json!({
        "schema_version": 1,
        "id": "m48-s4-path2",
        "title": "Path2",
        "slide_size": { "w": 1280, "h": 720 },
        "scenes": [{
            "id": "sc1",
            "title": "S",
            "slides": [{
                "id": "slide-1",
                "title": "S1",
                "background": "#FFFFFF",
                "duration_ms": 5000,
                "objects": [{
                    "id": "path-1",
                    "frame": { "x": 200, "y": 200, "w": 200, "h": 120 },
                    "shadow": red_shadow(),
                    "glow": red_glow(),
                    "kind": {
                        "type": "path",
                        "points": [
                            { "x": 0.0, "y": 0.0 },
                            { "x": 1.0, "y": 0.0 },
                            { "x": 1.0, "y": 1.0 },
                            { "x": 0.0, "y": 1.0 }
                        ],
                        "closed": true,
                        "fill": "#00000000",
                        "outline": { "color": "#00000000", "width": 0.0 }
                    }
                }]
            }]
        }]
    })
    .to_string();
    let path_empty_img = render(&Course::from_json(&path_empty_json).expect("path empty course"));
    assert!(
        all_white(&path_empty_img),
        "expected no shadow or glow when path has neither fill nor outline"
    );

    // (d) Image with shadow is unaffected (still draws its shadow).
    let img_json = serde_json::json!({
        "schema_version": 1,
        "id": "m48-s4-img",
        "title": "Image",
        "slide_size": { "w": 1280, "h": 720 },
        "scenes": [{
            "id": "sc1",
            "title": "S",
            "slides": [{
                "id": "slide-1",
                "title": "S1",
                "background": "#FFFFFF",
                "duration_ms": 5000,
                "objects": [{
                    "id": "img-1",
                    "frame": { "x": 200, "y": 200, "w": 200, "h": 120 },
                    "shadow": red_shadow(),
                    "kind": {
                        "type": "image",
                        "src": "media/missing.png"
                    }
                }]
            }]
        }]
    })
    .to_string();
    let img_out = render(&Course::from_json(&img_json).expect("image course"));
    assert!(
        count_red_pixels_in(&img_out, 402, 220, 420, 340) > 0,
        "expected image shadow to remain unaffected"
    );
}
