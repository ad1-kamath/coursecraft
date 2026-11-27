//! `render --layer` shows only the named layers on top of the base layer.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;
use std::process::ExitCode;

use coursecraft_ui_egui::coursecraft_media::image;

use crate::{Cli, Cmd, run};

/// A slide whose own timeline-start trigger shows the `failed` layer, as a results slide does.
fn course_json() -> String {
    serde_json::json!({
        "schema_version": 1,
        "id": "layers",
        "title": "Layers",
        "slide_size": { "w": 400, "h": 200 },
        "scenes": [{ "id": "sc", "title": "S", "slides": [{
            "id": "results",
            "title": "Results",
            "duration_ms": 1000,
            "objects": [],
            "layers": [
                {
                    "id": "passed", "name": "Passed", "duration_ms": 1000,
                    "objects": [{
                        "id": "p", "kind": { "type": "shape", "shape": "rectangle", "fill": "#FF0000" },
                        "frame": { "x": 0.0, "y": 0.0, "w": 100.0, "h": 100.0 }
                    }]
                },
                {
                    "id": "failed", "name": "Failed", "duration_ms": 1000,
                    "objects": [{
                        "id": "f", "kind": { "type": "shape", "shape": "rectangle", "fill": "#0000FF" },
                        "frame": { "x": 200.0, "y": 0.0, "w": 100.0, "h": 100.0 }
                    }]
                }
            ],
            "triggers": [{
                "action": { "action": "show_layer", "layer": "failed" },
                "when": { "event": "timeline_starts" }
            }]
        }] }]
    })
    .to_string()
}

fn render_pixels(name: &str, layers: &[&str]) -> (image::RgbaImage, PathBuf) {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/agy-scratch")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let course = dir.join("course.json");
    std::fs::write(&course, course_json()).expect("write course");
    let out = dir.join("out.png");
    let code = run(Cli {
        cmd: Cmd::Render {
            course,
            slide: "results".to_owned(),
            width: 400,
            mode: "player".to_owned(),
            layer: layers.iter().map(|l| (*l).to_owned()).collect(),
            at_ms: None,
            out: out.clone(),
        },
    })
    .expect("render");
    assert_eq!(code, ExitCode::SUCCESS);
    let img = image::open(&out).expect("decode png").to_rgba8();
    (img, out)
}

fn is_red(p: &image::Rgba<u8>) -> bool {
    p.0[0] > 200 && p.0[1] < 60 && p.0[2] < 60
}

fn is_blue(p: &image::Rgba<u8>) -> bool {
    p.0[2] > 200 && p.0[0] < 60 && p.0[1] < 60
}

#[test]
fn render_layer_shows_only_the_named_layer() {
    let (img, _) = render_pixels("render-layer-only", &["passed"]);
    assert!(
        is_red(img.get_pixel(50, 50)),
        "the named layer is drawn: {:?}",
        img.get_pixel(50, 50)
    );
    assert!(
        !is_blue(img.get_pixel(250, 50)),
        "a layer the slide's own trigger opened is not drawn: {:?}",
        img.get_pixel(250, 50)
    );
}

#[test]
fn render_without_layer_still_shows_triggered_layers() {
    let (img, _) = render_pixels("render-layer-none", &[]);
    assert!(
        is_blue(img.get_pixel(250, 50)),
        "{:?}",
        img.get_pixel(250, 50)
    );
}
