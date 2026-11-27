//! EMF and WMF clip art imported through Insert › Picture and the media library.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::PlacementTool;
use crate::test_harness::TestHarness;
use coursecraft_course::Course;
use craft_metafile::write::{self, Op};
use kurbo::Shape as _;

fn hello() -> Course {
    let json = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/hello-course/course.json"
    ))
    .expect("hello course");
    Course::from_json(&json).expect("valid course")
}

/// A 150 × 75 pt red square on transparent, as EMF or WMF.
fn clip_art(wmf: bool) -> Vec<u8> {
    let square = kurbo::Rect::new(10.0, 10.0, 60.0, 60.0).to_path(0.1);
    let ops = [Op::Fill {
        path: square,
        even_odd: false,
        rgb: [255, 0, 0],
    }];
    let region = kurbo::Rect::new(0.0, 0.0, 150.0, 75.0);
    if wmf {
        write::wmf(&ops, region).expect("WMF").bytes
    } else {
        write::emf(&ops, region, "test", "clip").expect("EMF")
    }
}

#[test]
fn inserting_emf_and_wmf_stores_svg_and_arms_the_image_tool() {
    for (name, wmf) in [("logo.emf", false), ("Logo.WMF", true)] {
        let mut harness = TestHarness::new_with_size(hello(), (1280.0, 800.0));
        let src = harness
            .editor
            .import_image(name, &clip_art(wmf))
            .expect("clip art imports");
        assert!(src.ends_with(".svg"), "{src}");
        let bytes = harness.editor.media.get(&src).expect("stored").clone();
        assert!(bytes.starts_with(b"<svg"));
        match &harness.editor.ui.placement_tool {
            Some(PlacementTool::Image {
                src: tool_src,
                natural_size,
            }) => {
                assert_eq!(tool_src, &src);
                // 150 pt is 200 CSS px.
                assert!((natural_size.0 - 200.0).abs() < 1.0, "{natural_size:?}");
                assert!((natural_size.1 - 100.0).abs() < 1.0, "{natural_size:?}");
            }
            other => panic!("expected the Image tool, got {other:?}"),
        }
    }
}

#[test]
fn clip_art_without_its_extension_is_still_recognised() {
    let mut harness = TestHarness::new_with_size(hello(), (1280.0, 800.0));
    let src = harness
        .editor
        .import_image("pasted", &clip_art(false))
        .expect("clip art imports");
    assert!(src.ends_with(".svg"), "{src}");
}

#[test]
fn a_broken_metafile_is_refused_with_a_message() {
    let mut harness = TestHarness::new_with_size(hello(), (1280.0, 800.0));
    assert!(
        harness
            .editor
            .import_image("bad.emf", b"not clip art")
            .is_none()
    );
    let last = harness.editor.ui.error_history.last().expect("error noted");
    assert!(last.contains("bad.emf"), "{last}");
}

#[test]
fn media_library_adds_clip_art_as_svg() {
    let mut harness = TestHarness::new_with_size(hello(), (1280.0, 800.0));
    let src = harness
        .editor
        .add_media_bytes("chart.emf", &clip_art(false))
        .expect("added");
    assert!(src.ends_with(".svg"), "{src}");
    assert!(harness.editor.ui.placement_tool.is_none());
    // Replacing that SVG with clip art keeps it an SVG.
    assert!(harness.editor.replace_media_bytes(&src, &clip_art(true)));
    assert!(harness.editor.media.get(&src).unwrap().starts_with(b"<svg"));
}
