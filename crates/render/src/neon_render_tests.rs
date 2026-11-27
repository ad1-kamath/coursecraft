//! Neon 0.6.1 render fixes N37 (a)-(e): path hit tests, curved fills with subpaths, dashed
//! table borders, links on text that is not on yet, and the box that links and text effects use.

use coursecraft_course::Course;

use crate::compose::{Hit, compose_authoring, item_hit_test, item_link_at};
use crate::media::{MediaFetch, MediaSource};
use crate::offscreen::{RenderMode, RenderOptions, RgbaImage, render_slide_rgba};

struct NoMedia;
impl MediaSource for NoMedia {
    fn get(&self, _src: &str) -> MediaFetch {
        MediaFetch::Missing
    }
}

fn course_with(object: serde_json::Value) -> Course {
    let json = serde_json::json!({
        "schema_version": 1, "id": "neon", "title": "Neon",
        "slide_size": { "w": 1280, "h": 720 },
        "scenes": [{ "id": "sc", "title": "S", "slides": [{
            "id": "sl", "title": "One", "background": "#FFFFFF", "duration_ms": 5000,
            "objects": [object]
        }] }]
    });
    Course::from_json(&json.to_string()).expect("course")
}

fn render(course: &Course) -> RgbaImage {
    let opts = RenderOptions {
        width: 1280,
        mode: RenderMode::Authoring,
        layers: Vec::new(),
        at_ms: None,
    };
    render_slide_rgba(course, "sl", &opts, &NoMedia).expect("render")
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

fn item_of(course: &Course, id: &str) -> crate::compose::DrawItem {
    let slide = course.slide("sl").expect("slide");
    compose_authoring(course, slide, None)
        .into_iter()
        .find(|item| item.hit == Hit::Object(id.into()))
        .expect("item")
}

fn path_object(extra: serde_json::Value) -> serde_json::Value {
    let mut kind = serde_json::json!({
        "type": "path",
        "points": [
            { "x": 0.0, "y": 0.0 }, { "x": 1.0, "y": 0.0 },
            { "x": 1.0, "y": 1.0 }, { "x": 0.0, "y": 1.0 }
        ],
        "closed": true,
        "outline": { "color": "#000000", "width": 1.0 }
    });
    if let (Some(target), Some(extra)) = (kind.as_object_mut(), extra.as_object()) {
        for (key, value) in extra {
            target.insert(key.clone(), value.clone());
        }
    }
    serde_json::json!({
        "id": "p1", "kind": kind,
        "frame": { "x": 100, "y": 100, "w": 400, "h": 400 }
    })
}

#[test]
fn n37a_a_gradient_filled_path_hits_inside() {
    let gradient = serde_json::json!({ "fill_style": {
        "type": "linear", "angle": 0.0,
        "stops": [{ "color": "#FF0000", "position": 0.0 }, { "color": "#0000FF", "position": 1.0 }]
    } });
    let item = item_of(&course_with(path_object(gradient)), "p1");
    assert!(item_hit_test(&item, egui::pos2(300.0, 300.0)));
    let bare = item_of(&course_with(path_object(serde_json::json!({}))), "p1");
    assert!(
        !item_hit_test(&bare, egui::pos2(300.0, 300.0)),
        "no fill, no inside hit"
    );
}

#[test]
fn n37a_subpaths_cut_holes_and_take_hits() {
    let extra = serde_json::json!({
        "fill": "#2266CC",
        "subpaths": [{
            "points": [
                { "x": 0.3, "y": 0.3 }, { "x": 0.7, "y": 0.3 },
                { "x": 0.7, "y": 0.7 }, { "x": 0.3, "y": 0.7 }
            ],
            "closed": true
        }]
    });
    let item = item_of(&course_with(path_object(extra)), "p1");
    // Inside the ring: a hit. Inside the hole, away from its edge: none.
    assert!(item_hit_test(&item, egui::pos2(120.0, 300.0)));
    assert!(!item_hit_test(&item, egui::pos2(300.0, 300.0)));
    // The hole's edge is a line like any other.
    assert!(item_hit_test(&item, egui::pos2(220.0, 300.0)));
}

#[test]
fn n37b_curved_fill_with_a_subpath_follows_the_curve() {
    let curved = |with_subpath: bool| {
        let mut extra = serde_json::json!({
            "style": "curved", "fill": "#FF0000",
            "points": [
                { "x": 0.1, "y": 0.1 }, { "x": 0.9, "y": 0.1 },
                { "x": 0.9, "y": 0.9 }, { "x": 0.1, "y": 0.9 }
            ]
        });
        if with_subpath && let Some(map) = extra.as_object_mut() {
            map.insert(
                "subpaths".into(),
                serde_json::json!([{
                    "points": [{ "x": 0.0, "y": 0.0 }, { "x": 0.02, "y": 0.0 }, { "x": 0.0, "y": 0.02 }],
                    "closed": true
                }]),
            );
        }
        render(&course_with(path_object(extra)))
    };
    let plain = curved(false);
    let with_sub = curved(true);
    // The tiny subpath sits in the corner; the rest of the fill must match the plain path.
    let mut differing = 0;
    for y in (100..500).step_by(3) {
        for x in (110..500).step_by(3) {
            if px(&plain, x, y) != px(&with_sub, x, y) {
                differing += 1;
            }
        }
    }
    assert_eq!(
        differing, 0,
        "the fill follows the same curve as the stroke"
    );
}

#[test]
fn n37c_table_border_dash_shows_gaps() {
    let table = |dash: &str| {
        course_with(serde_json::json!({
            "id": "t1",
            "kind": {
                "type": "table", "columns": [1.0],
                "rows": [{ "height": 1.0, "cells": [{ "text": "" }] }],
                "borders": { "outer": { "color": "#000000", "width": 6, "dash": dash } }
            },
            "frame": { "x": 100, "y": 100, "w": 600, "h": 200 }
        }))
    };
    let dark_along_top = |course: &Course| {
        let img = render(course);
        (110..690).filter(|&x| px(&img, x, 102)[0] < 100).count()
    };
    let solid = dark_along_top(&table("solid"));
    let dashed = dark_along_top(&table("dash"));
    assert!(solid > 560, "solid border is unbroken: {solid}");
    assert!(
        dashed < solid - 100,
        "dashed border has gaps: {dashed} of {solid}"
    );
    assert!(dashed > 100, "dashed border still draws: {dashed}");
}

#[test]
fn n37e_links_follow_insets_and_wrap() {
    let object = |insets: bool| {
        let mut style = serde_json::json!({ "size": 24, "wrap": true });
        if insets && let Some(map) = style.as_object_mut() {
            map.insert(
                "insets".into(),
                serde_json::json!({ "left": 100.0, "top": 0.0, "right": 0.0, "bottom": 0.0 }),
            );
        }
        serde_json::json!({
            "id": "t1",
            "kind": { "type": "text", "text": "Link", "style": style,
                      "runs": [{ "text": "Link", "link": { "type": "url", "url": "https://example.com" } }] },
            "frame": { "x": 100, "y": 100, "w": 400, "h": 100 }
        })
    };
    let ctx = egui::Context::default();
    let mut out = Vec::new();
    for insets in [false, true] {
        let course = course_with(object(insets));
        let mut items = vec![item_of(&course, "t1")];
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            crate::text_layout::fill_item_links(ui.ctx(), &mut items);
        });
        output.textures_delta.clear();
        output.drop_without_applying_deltas();
        let item = items.remove(0);
        let rect = item.links.first().map(|link| link.rect);
        out.push((item, rect));
    }
    let (plain, plain_rect) = &out[0];
    let (inset, inset_rect) = &out[1];
    let (plain_rect, inset_rect) = (plain_rect.expect("link"), inset_rect.expect("link"));
    // 100 px of inset on a 400 px frame moves the link a quarter of the frame to the right.
    assert!(
        inset_rect.min.x - plain_rect.min.x > 0.2,
        "{plain_rect:?} vs {inset_rect:?}"
    );
    let at = |item: &crate::compose::DrawItem, x: f32| {
        item_link_at(
            item,
            egui::pos2(item.rect.min.x + x, item.rect.min.y + 14.0),
        )
        .is_some()
    };
    assert!(at(plain, 10.0));
    assert!(
        !at(inset, 10.0),
        "the inset text is not under the left edge"
    );
    assert!(at(inset, 110.0));
}
