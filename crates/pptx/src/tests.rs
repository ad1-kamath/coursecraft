//! Comprehensive unit and integration tests for coursecraft-pptx.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::Write as _;

use coursecraft_course::{Align, ShapeKind, VAlign};

use crate::{Limits, PptxError, SkipKind, import_pptx, import_pptx_with_limits};

/// A tiny valid SVG used by the svgBlip preference test.
const TINY_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16"><rect width="16" height="16" fill="#ff0000"/></svg>"##;

/// 1x1 red PNG pixel bytes valid for decoding by coursecraft-media.
const TINY_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG magic
    0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, // IHDR
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, // 1x1
    0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53, // 8-bit RGB
    0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, // IDAT
    0x54, 0x08, 0xD7, 0x63, 0xF8, 0xCF, 0xC0, 0x00, // compressed data
    0x00, 0x03, 0x01, 0x01, 0x00, 0x18, 0xDD, 0x8D, // data & CRC
    0xB0, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, // IEND
    0x44, 0xAE, 0x42, 0x60, 0x82,
];

/// Helper to build in-memory .pptx zip fixtures with 16:9 slide size.
fn fixture_pptx(slides: &[&str], extra: &[(&str, &[u8])]) -> Vec<u8> {
    fixture_pptx_with_size(slides, 9_144_000, 5_143_500, extra)
}

/// Helper to build in-memory .pptx zip fixtures with custom slide size (cx, cy in EMU).
fn fixture_pptx_with_size(slides: &[&str], cx: u32, cy: u32, extra: &[(&str, &[u8])]) -> Vec<u8> {
    let mut buffer = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buffer);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);

        // 1. ppt/presentation.xml
        let mut sld_ids = String::new();
        for (i, _) in slides.iter().enumerate() {
            let rid = format!("rId{}", i + 1);
            let id = 256 + i;
            sld_ids.push_str(&format!(r#"<p:sldId id="{id}" r:id="{rid}"/>"#));
        }

        let pres_xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
            <p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
                            xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
                            xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
                <p:sldSz cx="{cx}" cy="{cy}"/>
                <p:sldIdLst>{sld_ids}</p:sldIdLst>
            </p:presentation>"#
        );
        zip.start_file("ppt/presentation.xml", options).unwrap();
        zip.write_all(pres_xml.as_bytes()).unwrap();

        // 2. ppt/_rels/presentation.xml.rels
        let mut pres_rels = String::new();
        for (i, _) in slides.iter().enumerate() {
            let rid = format!("rId{}", i + 1);
            let sld_num = i + 1;
            pres_rels.push_str(&format!(
                r#"<Relationship Id="{rid}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide{sld_num}.xml"/>"#
            ));
        }
        let pres_rels_xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
            <Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
                {pres_rels}
            </Relationships>"#
        );
        zip.start_file("ppt/_rels/presentation.xml.rels", options)
            .unwrap();
        zip.write_all(pres_rels_xml.as_bytes()).unwrap();

        // 3. Slide XML files
        for (i, slide_content) in slides.iter().enumerate() {
            let sld_num = i + 1;
            let slide_path = format!("ppt/slides/slide{sld_num}.xml");
            zip.start_file(&slide_path, options).unwrap();
            zip.write_all(slide_content.as_bytes()).unwrap();
        }

        // 4. Extra parts (e.g. media, rels, notes)
        for (path, bytes) in extra {
            zip.start_file(*path, options).unwrap();
            zip.write_all(bytes).unwrap();
        }

        zip.finish().unwrap();
    }
    buffer.into_inner()
}

#[test]
fn text_paragraphs_links_and_autofit_import() {
    let slide1 = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
    <p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
           xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
           xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
        <p:cSld>
            <p:spTree>
                <p:sp>
                    <p:nvSpPr><p:cNvPr id="2" name="Body"/></p:nvSpPr>
                    <p:spPr>
                        <a:xfrm>
                            <a:off x="914400" y="914400"/>
                            <a:ext cx="5486400" cy="2743200"/>
                        </a:xfrm>
                        <a:prstGeom prst="rect"><a:avLst/></a:prstGeom>
                    </p:spPr>
                    <p:txBody>
                        <a:bodyPr><a:normAutofit/></a:bodyPr>
                        <a:p>
                            <a:pPr lvl="0">
                                <a:lnSpc><a:spcPct val="150000"/></a:lnSpc>
                                <a:spcBef><a:spcPts val="600"/></a:spcBef>
                                <a:spcAft><a:spcPts val="300"/></a:spcAft>
                                <a:buChar char="•"/>
                            </a:pPr>
                            <a:r><a:t>Bullet</a:t></a:r>
                        </a:p>
                        <a:p>
                            <a:pPr lvl="1"><a:buAutoNum type="arabicPeriod"/></a:pPr>
                            <a:r>
                                <a:rPr>
                                    <a:hlinkClick r:id="rId2" action="ppaction://hlinksldjump"/>
                                </a:rPr>
                                <a:t>Jump</a:t>
                            </a:r>
                        </a:p>
                        <a:p>
                            <a:pPr lvl="1"><a:buNone/></a:pPr>
                            <a:r>
                                <a:rPr><a:hlinkClick r:id="rId1"/></a:rPr>
                                <a:t>Visit</a:t>
                            </a:r>
                        </a:p>
                    </p:txBody>
                </p:sp>
            </p:spTree>
        </p:cSld>
    </p:sld>"#;
    let slide2 = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
    <p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
           xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
        <p:cSld><p:spTree>
            <p:sp>
                <p:nvSpPr><p:cNvPr id="3" name="Second"/></p:nvSpPr>
                <p:spPr>
                    <a:xfrm><a:off x="0" y="0"/><a:ext cx="1000000" cy="1000000"/></a:xfrm>
                    <a:prstGeom prst="rect"><a:avLst/></a:prstGeom>
                </p:spPr>
                <p:txBody><a:bodyPr/><a:p><a:r><a:t>Second</a:t></a:r></a:p></p:txBody>
            </p:sp>
        </p:spTree></p:cSld>
    </p:sld>"#;
    let rels = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
    <Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
        <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://example.com" TargetMode="External"/>
        <Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slide2.xml"/>
    </Relationships>"#;
    let bytes = fixture_pptx(
        &[slide1, slide2],
        &[("ppt/slides/_rels/slide1.xml.rels", rels)],
    );
    let result = import_pptx(&bytes, "Paragraphs").expect("import");
    let slide = &result.course.scenes[0].slides[0];
    let (object, _) = slide.object("text1").expect("imported text box");
    let coursecraft_course::ObjectKind::Text {
        paragraphs,
        autofit,
        runs,
        ..
    } = &object.kind
    else {
        panic!("expected a text object");
    };
    assert_eq!(*autofit, coursecraft_course::Autofit::Shrink);
    assert_eq!(paragraphs.len(), 3);
    assert_eq!(paragraphs[0].list, coursecraft_course::ListKind::Bullet);
    assert_eq!(paragraphs[0].level, 0);
    assert!((paragraphs[0].line_spacing - 1.5).abs() < 0.01);
    assert!(paragraphs[0].space_before > 0.0);
    assert!(paragraphs[0].space_after > 0.0);
    assert_eq!(paragraphs[1].list, coursecraft_course::ListKind::Number);
    assert_eq!(paragraphs[1].level, 1);
    assert_eq!(paragraphs[2].list, coursecraft_course::ListKind::None);
    assert_eq!(paragraphs[2].level, 1);

    let jump = runs
        .iter()
        .find(|run| run.text.contains("Jump"))
        .and_then(|run| run.link.clone());
    assert_eq!(
        jump,
        Some(coursecraft_course::Link::Slide {
            slide: "slide2".into()
        })
    );
    let visit = runs
        .iter()
        .find(|run| run.text.contains("Visit"))
        .and_then(|run| run.link.clone());
    assert_eq!(
        visit,
        Some(coursecraft_course::Link::Url {
            url: "https://example.com".into()
        })
    );
}

#[test]
fn test_slide_order_and_titles() {
    let slide1 = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
    <p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
           xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
        <p:cSld>
            <p:spTree>
                <p:sp>
                    <p:nvSpPr>
                        <p:cNvPr id="2" name="Title 1"/>
                        <p:nvPr><p:ph type="title"/></p:nvPr>
                    </p:nvSpPr>
                    <p:spPr>
                        <a:xfrm>
                            <a:off x="914400" y="914400"/>
                            <a:ext cx="7315200" cy="914400"/>
                        </a:xfrm>
                        <a:prstGeom prst="rect"><a:avLst/></a:prstGeom>
                    </p:spPr>
                    <p:txBody>
                        <a:bodyPr/>
                        <a:p>
                            <a:r><a:t>  Welcome to CourseCraft  </a:t></a:r>
                        </a:p>
                    </p:txBody>
                </p:sp>
            </p:spTree>
        </p:cSld>
    </p:sld>"#;

    let slide2 = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
    <p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
           xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
        <p:cSld>
            <p:spTree>
                <p:sp>
                    <p:nvSpPr>
                        <p:cNvPr id="3" name="Box 1"/>
                    </p:nvSpPr>
                    <p:spPr>
                        <a:xfrm>
                            <a:off x="914400" y="914400"/>
                            <a:ext cx="2000000" cy="1000000"/>
                        </a:xfrm>
                        <a:prstGeom prst="rect"><a:avLst/></a:prstGeom>
                    </p:spPr>
                    <p:txBody>
                        <a:bodyPr/>
                        <a:p>
                            <a:r><a:t>Content without title placeholder</a:t></a:r>
                        </a:p>
                    </p:txBody>
                </p:sp>
            </p:spTree>
        </p:cSld>
    </p:sld>"#;

    let bytes = fixture_pptx(&[slide1, slide2], &[]);
    let res = import_pptx(&bytes, "My Course").expect("import success");

    assert_eq!(res.course.title, "My Course");
    assert_eq!(res.course.scenes.len(), 1);
    let scene = &res.course.scenes[0];
    assert_eq!(scene.title, "Imported");
    assert_eq!(scene.slides.len(), 2);

    assert_eq!(scene.slides[0].id, "slide1");
    assert_eq!(scene.slides[0].title, "Welcome to CourseCraft");

    assert_eq!(scene.slides[1].id, "slide2");
    assert_eq!(scene.slides[1].title, "Slide 2");
    assert!(res.course.is_playable(), "{:?}", res.course.validate());
}

#[test]
fn test_size_scaling_with_letterbox_for_4_3() {
    // 4:3 size in EMU: 9,144,000 x 6,858,000
    let slide = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
    <p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
           xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
        <p:cSld>
            <p:spTree>
                <p:sp>
                    <p:nvSpPr><p:cNvPr id="2" name="Box"/></p:nvSpPr>
                    <p:spPr>
                        <a:xfrm>
                            <a:off x="0" y="0"/>
                            <a:ext cx="9144000" cy="6858000"/>
                        </a:xfrm>
                        <a:prstGeom prst="rect"/>
                    </p:spPr>
                    <p:txBody><a:bodyPr/><a:p><a:r><a:t>Full 4:3 slide</a:t></a:r></a:p></p:txBody>
                </p:sp>
            </p:spTree>
        </p:cSld>
    </p:sld>"#;

    let bytes = fixture_pptx_with_size(&[slide], 9_144_000, 6_858_000, &[]);
    let res = import_pptx(&bytes, "4:3 Deck").expect("import success");

    let slide = &res.course.scenes[0].slides[0];
    let obj = &slide.objects[0];

    // Scale = 720 / 6,858,000. Scaled w = 960 px. Offset x = (1280 - 960) / 2 = 160 px.
    assert!((obj.frame.x - 160.0).abs() < 1.0);
    assert!((obj.frame.y - 0.0).abs() < 1.0);
    assert!((obj.frame.w - 960.0).abs() < 1.0);
    assert!((obj.frame.h - 720.0).abs() < 1.0);

    // Should have recorded aspect ratio letterbox note
    assert!(
        res.report
            .items
            .iter()
            .any(|i| i.kind == SkipKind::AspectRatioMismatch)
    );
    assert!(res.course.is_playable(), "{:?}", res.course.validate());
}

#[test]
fn test_text_runs_and_align_valign() {
    let slide = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
    <p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
           xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
        <p:cSld>
            <p:spTree>
                <p:sp>
                    <p:nvSpPr><p:cNvPr id="2" name="Rich Text"/></p:nvSpPr>
                    <p:spPr>
                        <a:xfrm>
                            <a:off x="100000" y="100000"/>
                            <a:ext cx="4000000" cy="2000000"/>
                        </a:xfrm>
                        <a:prstGeom prst="rect"/>
                    </p:spPr>
                    <p:txBody>
                        <a:bodyPr anchor="ctr"/>
                        <a:p>
                            <a:pPr algn="ctr"/>
                            <a:r>
                                <a:rPr b="1" i="0" u="sng" sz="2400">
                                    <a:solidFill><a:srgbClr val="FF0000"/></a:solidFill>
                                </a:rPr>
                                <a:t>Bold Red</a:t>
                            </a:r>
                            <a:r>
                                <a:rPr b="0" i="1" u="none" sz="1800">
                                    <a:solidFill><a:srgbClr val="00FF00"/></a:solidFill>
                                </a:rPr>
                                <a:t> Italic Green</a:t>
                            </a:r>
                        </a:p>
                        <a:p>
                            <a:r><a:t>Second paragraph</a:t></a:r>
                        </a:p>
                    </p:txBody>
                </p:sp>
            </p:spTree>
        </p:cSld>
    </p:sld>"#;

    let bytes = fixture_pptx(&[slide], &[]);
    let res = import_pptx(&bytes, "Text Deck").expect("import success");

    let slide = &res.course.scenes[0].slides[0];
    let obj = &slide.objects[0];

    match &obj.kind {
        coursecraft_course::ObjectKind::Text {
            text, style, runs, ..
        } => {
            assert_eq!(style.align, Align::Center);
            assert_eq!(style.valign, VAlign::Middle);
            assert_eq!(runs.len(), 3);

            assert_eq!(runs[0].text, "Bold Red");
            assert_eq!(runs[0].bold, Some(true));
            assert_eq!(runs[0].italic, Some(false));
            assert_eq!(runs[0].underline, Some(true));
            assert_eq!(
                runs[0].color,
                Some(coursecraft_course::Color("#FF0000".into()))
            );

            assert_eq!(runs[1].text, " Italic Green\n");
            assert_eq!(runs[1].bold, Some(false));
            assert_eq!(runs[1].italic, Some(true));
            assert_eq!(runs[1].underline, Some(false));
            assert_eq!(
                runs[1].color,
                Some(coursecraft_course::Color("#00FF00".into()))
            );

            assert_eq!(runs[2].text, "Second paragraph");

            let concat: String = runs.iter().map(|r| r.text.as_str()).collect();
            assert_eq!(*text, concat);
        }
        _ => panic!("expected text object"),
    }
    assert!(res.course.is_playable(), "{:?}", res.course.validate());
}

#[test]
fn test_basic_shapes_fills() {
    let slide = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
    <p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
           xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
        <p:cSld>
            <p:spTree>
                <!-- Rectangle with solid fill -->
                <p:sp>
                    <p:nvSpPr><p:cNvPr id="2" name="Rect Shape"/></p:nvSpPr>
                    <p:spPr>
                        <a:xfrm><a:off x="100" y="100"/><a:ext cx="1000" cy="1000"/></a:xfrm>
                        <a:prstGeom prst="rect"/>
                        <a:solidFill><a:srgbClr val="112233"/></a:solidFill>
                    </p:spPr>
                </p:sp>
                <!-- Rounded rectangle with solid fill -->
                <p:sp>
                    <p:nvSpPr><p:cNvPr id="3" name="Round Rect Shape"/></p:nvSpPr>
                    <p:spPr>
                        <a:xfrm><a:off x="200" y="200"/><a:ext cx="1000" cy="1000"/></a:xfrm>
                        <a:prstGeom prst="roundRect"/>
                        <a:solidFill><a:srgbClr val="445566"/></a:solidFill>
                    </p:spPr>
                </p:sp>
                <!-- Ellipse with solid fill -->
                <p:sp>
                    <p:nvSpPr><p:cNvPr id="4" name="Ellipse Shape"/></p:nvSpPr>
                    <p:spPr>
                        <a:xfrm><a:off x="300" y="300"/><a:ext cx="1000" cy="1000"/></a:xfrm>
                        <a:prstGeom prst="ellipse"/>
                        <a:solidFill><a:srgbClr val="778899"/></a:solidFill>
                    </p:spPr>
                </p:sp>
            </p:spTree>
        </p:cSld>
    </p:sld>"#;

    let bytes = fixture_pptx(&[slide], &[]);
    let res = import_pptx(&bytes, "Shapes").expect("import success");

    let slide = &res.course.scenes[0].slides[0];
    assert_eq!(slide.objects.len(), 3);

    match &slide.objects[0].kind {
        coursecraft_course::ObjectKind::Shape { shape, fill, .. } => {
            assert_eq!(*shape, ShapeKind::Rectangle);
            assert_eq!(fill.0, "#112233");
        }
        _ => panic!("expected rectangle"),
    }
    match &slide.objects[1].kind {
        coursecraft_course::ObjectKind::Shape { shape, fill, .. } => {
            assert_eq!(*shape, ShapeKind::RoundedRectangle);
            assert_eq!(fill.0, "#445566");
        }
        _ => panic!("expected round rect"),
    }
    match &slide.objects[2].kind {
        coursecraft_course::ObjectKind::Shape { shape, fill, .. } => {
            assert_eq!(*shape, ShapeKind::Ellipse);
            assert_eq!(fill.0, "#778899");
        }
        _ => panic!("expected ellipse"),
    }
    assert!(res.course.is_playable(), "{:?}", res.course.validate());
}

#[test]
fn test_shape_with_text_keeps_it_inside_the_shape() {
    let slide = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
    <p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
           xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
        <p:cSld>
            <p:spTree>
                <p:sp>
                    <p:nvSpPr><p:cNvPr id="2" name="Pill Button"/></p:nvSpPr>
                    <p:spPr>
                        <a:xfrm><a:off x="500000" y="500000"/><a:ext cx="2000000" cy="800000"/></a:xfrm>
                        <a:prstGeom prst="roundRect"/>
                        <a:solidFill><a:srgbClr val="0066CC"/></a:solidFill>
                    </p:spPr>
                    <p:txBody>
                        <a:bodyPr anchor="ctr"/>
                        <a:p>
                            <a:pPr algn="ctr"/>
                            <a:r><a:rPr sz="2400"><a:solidFill><a:srgbClr val="FFFFFF"/></a:solidFill></a:rPr><a:t>Click Me</a:t></a:r>
                        </a:p>
                    </p:txBody>
                </p:sp>
            </p:spTree>
        </p:cSld>
    </p:sld>"#;

    let bytes = fixture_pptx(&[slide], &[]);
    let res = import_pptx(&bytes, "Button").expect("import success");

    let slide = &res.course.scenes[0].slides[0];
    assert_eq!(slide.objects.len(), 1, "text lives inside the shape");
    assert_eq!(slide.objects[0].id, "shape1");
    match &slide.objects[0].kind {
        coursecraft_course::ObjectKind::Shape {
            shape, fill, text, ..
        } => {
            assert_eq!(*shape, ShapeKind::RoundedRectangle);
            assert_eq!(fill.0, "#0066CC");
            let text = text.as_ref().expect("shape text");
            assert_eq!(text.text, "Click Me");
            assert_eq!(text.style.align, Align::Center);
            assert_eq!(text.style.valign, VAlign::Middle);
            assert_eq!(
                text.runs[0].color.as_ref().map(|c| c.0.as_str()),
                Some("#FFFFFF")
            );
        }
        _ => panic!("expected shape"),
    }
    assert!(res.course.is_playable(), "{:?}", res.course.validate());
}

#[test]
fn test_shape_outline_no_fill_and_dash_import() {
    let slide = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
    <p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
           xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
        <p:cSld>
            <p:spTree>
                <p:sp>
                    <p:nvSpPr><p:cNvPr id="2" name="Outlined"/></p:nvSpPr>
                    <p:spPr>
                        <a:xfrm><a:off x="100" y="100"/><a:ext cx="1000" cy="1000"/></a:xfrm>
                        <a:prstGeom prst="rect"/>
                        <a:solidFill><a:srgbClr val="112233"/></a:solidFill>
                        <a:ln w="19050">
                            <a:solidFill><a:srgbClr val="FF0000"/></a:solidFill>
                            <a:prstDash val="dash"/>
                        </a:ln>
                    </p:spPr>
                </p:sp>
                <p:sp>
                    <p:nvSpPr><p:cNvPr id="3" name="No fill"/></p:nvSpPr>
                    <p:spPr>
                        <a:xfrm><a:off x="200" y="200"/><a:ext cx="1000" cy="1000"/></a:xfrm>
                        <a:prstGeom prst="ellipse"/>
                        <a:noFill/>
                        <a:ln w="9525"><a:noFill/></a:ln>
                    </p:spPr>
                </p:sp>
            </p:spTree>
        </p:cSld>
    </p:sld>"#;

    let bytes = fixture_pptx(&[slide], &[]);
    let res = import_pptx(&bytes, "Outlines").expect("import success");
    let slide = &res.course.scenes[0].slides[0];
    assert_eq!(slide.objects.len(), 2);
    match &slide.objects[0].kind {
        coursecraft_course::ObjectKind::Shape { outline, .. } => {
            let outline = outline.as_ref().expect("outline");
            assert_eq!(outline.color.0, "#FF0000");
            assert!((outline.width - 2.667).abs() < 0.01, "{}", outline.width);
            assert_eq!(outline.dash, coursecraft_course::DashStyle::Dash);
        }
        _ => panic!("expected shape"),
    }
    match &slide.objects[1].kind {
        coursecraft_course::ObjectKind::Shape { fill, outline, .. } => {
            assert_eq!(fill.0, "#00000000");
            assert!(outline.is_none());
        }
        _ => panic!("expected shape"),
    }
    assert!(res.course.is_playable(), "{:?}", res.course.validate());
}

#[test]
fn test_picture_copied_with_alt_text() {
    let slide = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
    <p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
           xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
           xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
        <p:cSld>
            <p:spTree>
                <p:pic>
                    <p:nvPicPr>
                        <p:cNvPr id="4" name="Logo Photo" descr="Company Logo"/>
                        <p:cNvPicPr/>
                        <p:nvPr/>
                    </p:nvPicPr>
                    <p:blipFill>
                        <a:blip r:embed="rIdPic1"/>
                    </p:blipFill>
                    <p:spPr>
                        <a:xfrm><a:off x="100000" y="100000"/><a:ext cx="1000000" cy="1000000"/></a:xfrm>
                    </p:spPr>
                </p:pic>
            </p:spTree>
        </p:cSld>
    </p:sld>"#;

    let slide_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
    <Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
        <Relationship Id="rIdPic1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/image1.png"/>
    </Relationships>"#;

    let extra: &[(&str, &[u8])] = &[
        ("ppt/slides/_rels/slide1.xml.rels", slide_rels.as_bytes()),
        ("ppt/media/image1.png", TINY_PNG),
    ];

    let bytes = fixture_pptx(&[slide], extra);
    let res = import_pptx(&bytes, "Pic Deck").expect("import success");

    assert_eq!(res.media.len(), 1);
    assert_eq!(res.media[0].0, "image1.png");
    assert_eq!(res.media[0].1, TINY_PNG);

    let slide = &res.course.scenes[0].slides[0];
    assert_eq!(slide.objects.len(), 1);
    assert_eq!(slide.objects[0].id, "image1");

    match &slide.objects[0].kind {
        coursecraft_course::ObjectKind::Image { src, alt, .. } => {
            assert_eq!(src, "image1.png");
            assert_eq!(alt, "Company Logo");
        }
        _ => panic!("expected image"),
    }
    assert!(res.course.is_playable(), "{:?}", res.course.validate());
}

#[test]
fn test_picture_prefers_svg_blip_over_png_fallback() {
    let slide = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
    <p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
           xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
           xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
           xmlns:asvg="http://schemas.microsoft.com/office/drawing/2016/SVG/main">
        <p:cSld>
            <p:spTree>
                <p:pic>
                    <p:nvPicPr>
                        <p:cNvPr id="4" name="Vector Logo" descr="Logo"/>
                        <p:cNvPicPr/>
                        <p:nvPr/>
                    </p:nvPicPr>
                    <p:blipFill>
                        <a:blip r:embed="rIdPng">
                            <a:extLst>
                                <a:ext uri="{96DAC541-7B7A-43D3-8B79-37D633B846F1}">
                                    <asvg:svgBlip r:embed="rIdSvg"/>
                                </a:ext>
                            </a:extLst>
                        </a:blip>
                    </p:blipFill>
                    <p:spPr>
                        <a:xfrm><a:off x="100000" y="100000"/><a:ext cx="1000000" cy="1000000"/></a:xfrm>
                    </p:spPr>
                </p:pic>
            </p:spTree>
        </p:cSld>
    </p:sld>"#;

    let slide_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
    <Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
        <Relationship Id="rIdPng" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/image1.png"/>
        <Relationship Id="rIdSvg" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/image1.svg"/>
    </Relationships>"#;

    let extra: &[(&str, &[u8])] = &[
        ("ppt/slides/_rels/slide1.xml.rels", slide_rels.as_bytes()),
        ("ppt/media/image1.png", TINY_PNG),
        ("ppt/media/image1.svg", TINY_SVG),
    ];

    let bytes = fixture_pptx(&[slide], extra);
    let res = import_pptx(&bytes, "Vector Deck").expect("import success");

    assert_eq!(res.media.len(), 1);
    assert_eq!(res.media[0].0, "image1.svg");
    assert_eq!(res.media[0].1, TINY_SVG);
    match &res.course.scenes[0].slides[0].objects[0].kind {
        coursecraft_course::ObjectKind::Image { src, .. } => assert_eq!(src, "image1.svg"),
        _ => panic!("expected image"),
    }
}

#[test]
fn test_picture_src_rect_becomes_a_crop() {
    let slide = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
    <p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
           xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
           xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
        <p:cSld>
            <p:spTree>
                <p:pic>
                    <p:nvPicPr>
                        <p:cNvPr id="4" name="Cropped Photo" descr="Cropped"/>
                        <p:cNvPicPr/>
                        <p:nvPr/>
                    </p:nvPicPr>
                    <p:blipFill>
                        <a:blip r:embed="rIdPic1"/>
                        <a:srcRect l="10000" t="5000" r="20000" b="15000"/>
                    </p:blipFill>
                    <p:spPr>
                        <a:xfrm><a:off x="100000" y="100000"/><a:ext cx="1000000" cy="1000000"/></a:xfrm>
                    </p:spPr>
                </p:pic>
            </p:spTree>
        </p:cSld>
    </p:sld>"#;

    let slide_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
    <Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
        <Relationship Id="rIdPic1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/image1.png"/>
    </Relationships>"#;
    let extra: &[(&str, &[u8])] = &[
        ("ppt/slides/_rels/slide1.xml.rels", slide_rels.as_bytes()),
        ("ppt/media/image1.png", TINY_PNG),
    ];

    let bytes = fixture_pptx(&[slide], extra);
    let res = import_pptx(&bytes, "Cropped Deck").expect("import success");
    let object = &res.course.scenes[0].slides[0].objects[0];
    match &object.kind {
        coursecraft_course::ObjectKind::Image { crop, .. } => {
            let crop = crop.expect("srcRect becomes a crop");
            assert!((crop.x - 0.1).abs() < 1e-6, "{crop:?}");
            assert!((crop.y - 0.05).abs() < 1e-6, "{crop:?}");
            assert!((crop.w - 0.7).abs() < 1e-6, "{crop:?}");
            assert!((crop.h - 0.8).abs() < 1e-6, "{crop:?}");
        }
        _ => panic!("expected image"),
    }
    assert!(res.course.is_playable(), "{:?}", res.course.validate());

    // A srcRect that would leave no picture is ignored rather than imported
    // as an invalid crop.
    let broken = slide.replace(
        r#"<a:srcRect l="10000" t="5000" r="20000" b="15000"/>"#,
        r#"<a:srcRect l="60000" r="60000"/>"#,
    );
    let bytes = fixture_pptx(&[&broken], extra);
    let res = import_pptx(&bytes, "Broken Crop").expect("import success");
    match &res.course.scenes[0].slides[0].objects[0].kind {
        coursecraft_course::ObjectKind::Image { crop, .. } => assert_eq!(*crop, None),
        _ => panic!("expected image"),
    }
}

#[test]
fn test_undecodable_picture_skipped() {
    let slide = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
    <p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
           xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
           xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
        <p:cSld>
            <p:spTree>
                <p:pic>
                    <p:nvPicPr>
                        <p:cNvPr id="4" name="Corrupt" descr="Bad"/>
                        <p:cNvPicPr/><p:nvPr/>
                    </p:nvPicPr>
                    <p:blipFill><a:blip r:embed="rIdBad"/></p:blipFill>
                    <p:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="100" cy="100"/></a:xfrm></p:spPr>
                </p:pic>
            </p:spTree>
        </p:cSld>
    </p:sld>"#;

    let slide_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
    <Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
        <Relationship Id="rIdBad" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/corrupt.wmf"/>
    </Relationships>"#;

    let extra: &[(&str, &[u8])] = &[
        ("ppt/slides/_rels/slide1.xml.rels", slide_rels.as_bytes()),
        ("ppt/media/corrupt.wmf", b"NOT A VALID IMAGE"),
    ];

    let bytes = fixture_pptx(&[slide], extra);
    let res = import_pptx(&bytes, "Corrupt Pic").expect("import success");

    let slide = &res.course.scenes[0].slides[0];
    assert!(slide.objects.is_empty());
    assert!(res.media.is_empty());
    assert!(
        res.report
            .items
            .iter()
            .any(|i| i.kind == SkipKind::UndecodablePicture)
    );
}

#[test]
fn test_group_flattening_with_choff_chext() {
    let slide = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
    <p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
           xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
        <p:cSld>
            <p:spTree>
                <p:grpSp>
                    <p:grpSpPr>
                        <a:xfrm>
                            <a:off x="1000000" y="1000000"/>
                            <a:ext cx="2000000" cy="2000000"/>
                            <a:chOff x="0" y="0"/>
                            <a:chExt cx="1000000" cy="1000000"/>
                        </a:xfrm>
                    </p:grpSpPr>
                    <p:sp>
                        <p:nvSpPr><p:cNvPr id="2" name="Child"/></p:nvSpPr>
                        <p:spPr>
                            <a:xfrm>
                                <a:off x="100000" y="100000"/>
                                <a:ext cx="200000" cy="200000"/>
                            </a:xfrm>
                            <a:prstGeom prst="rect"/>
                            <a:solidFill><a:srgbClr val="AABBCC"/></a:solidFill>
                        </p:spPr>
                    </p:sp>
                </p:grpSp>
            </p:spTree>
        </p:cSld>
    </p:sld>"#;

    let bytes = fixture_pptx(&[slide], &[]);
    let res = import_pptx(&bytes, "Group").expect("import success");

    let slide = &res.course.scenes[0].slides[0];
    assert_eq!(slide.objects.len(), 1);

    // Group scales 2x. Child off was (100000, 100000), scaled = (200000, 200000).
    // Plus group offset (1000000, 1000000) = (1200000, 1200000).
    // Child ext was (200000, 200000), scaled = (400000, 400000).
    // In slide px: 1200000 * scale. Scale = 1280 / 9144000 = 0.0001399825.
    let expected_x = (1_200_000.0 * (1280.0 / 9_144_000.0)) as f32;
    let expected_w = (400_000.0 * (1280.0 / 9_144_000.0)) as f32;

    let obj = &slide.objects[0];
    assert!((obj.frame.x - expected_x).abs() < 1.0);
    assert!((obj.frame.w - expected_w).abs() < 1.0);
    assert!(res.course.is_playable(), "{:?}", res.course.validate());
}

#[test]
fn test_notes_slide_extracted() {
    let slide = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
    <p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
           xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
        <p:cSld><p:spTree/></p:cSld>
    </p:sld>"#;

    let slide_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
    <Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
        <Relationship Id="rIdNotes" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesSlide" Target="../notesSlides/notesSlide1.xml"/>
    </Relationships>"#;

    let notes_slide = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
    <p:notes xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
             xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
        <p:cSld>
            <p:spTree>
                <p:sp>
                    <p:nvSpPr>
                        <p:cNvPr id="2" name="Slide Image"/>
                        <p:nvPr><p:ph type="sldImg"/></p:nvPr>
                    </p:nvSpPr>
                    <p:spPr/>
                </p:sp>
                <p:sp>
                    <p:nvSpPr>
                        <p:cNvPr id="3" name="Notes Placeholder"/>
                        <p:nvPr><p:ph type="body"/></p:nvPr>
                    </p:nvSpPr>
                    <p:spPr/>
                    <p:txBody>
                        <a:bodyPr/>
                        <a:p><a:r><a:t>Speaker notes for slide 1.</a:t></a:r></a:p>
                        <a:p><a:r><a:t>Remember to pause here.</a:t></a:r></a:p>
                    </p:txBody>
                </p:sp>
            </p:spTree>
        </p:cSld>
    </p:notes>"#;

    let extra: &[(&str, &[u8])] = &[
        ("ppt/slides/_rels/slide1.xml.rels", slide_rels.as_bytes()),
        ("ppt/notesSlides/notesSlide1.xml", notes_slide.as_bytes()),
    ];

    let bytes = fixture_pptx(&[slide], extra);
    let res = import_pptx(&bytes, "Notes Deck").expect("import success");

    let slide = &res.course.scenes[0].slides[0];
    assert_eq!(
        slide.notes,
        "Speaker notes for slide 1.\nRemember to pause here."
    );
    assert!(res.course.is_playable(), "{:?}", res.course.validate());
}

#[test]
fn table_graphic_frame_imports_grid_spans_header_and_borders() {
    let slide = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
    <p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
           xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
        <p:cSld><p:spTree>
            <p:graphicFrame>
                <p:nvGraphicFramePr><p:cNvPr id="7" name="Scores"/></p:nvGraphicFramePr>
                <p:xfrm>
                    <a:off x="914400" y="914400"/>
                    <a:ext cx="5486400" cy="2743200"/>
                </p:xfrm>
                <a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/table">
                    <a:tbl>
                        <a:tblPr firstRow="1"/>
                        <a:tblGrid>
                            <a:gridCol w="1828800"/>
                            <a:gridCol w="1828800"/>
                            <a:gridCol w="1828800"/>
                        </a:tblGrid>
                        <a:tr h="914400">
                            <a:tc><a:txBody><a:bodyPr/><a:p><a:r><a:t>Name</a:t></a:r></a:p></a:txBody>
                                <a:tcPr><a:solidFill><a:srgbClr val="2F6FEB"/></a:solidFill>
                                    <a:lnT w="12700"><a:solidFill><a:srgbClr val="111111"/></a:solidFill></a:lnT>
                                    <a:lnB w="12700"><a:solidFill><a:srgbClr val="111111"/></a:solidFill></a:lnB>
                                </a:tcPr>
                            </a:tc>
                            <a:tc><a:txBody><a:bodyPr/><a:p><a:r><a:t>Score</a:t></a:r></a:p></a:txBody></a:tc>
                            <a:tc><a:txBody><a:bodyPr/><a:p><a:r><a:t>Note</a:t></a:r></a:p></a:txBody></a:tc>
                        </a:tr>
                        <a:tr h="914400">
                            <a:tc><a:txBody><a:bodyPr/><a:p><a:r><a:t>Ada</a:t></a:r></a:p></a:txBody></a:tc>
                            <a:tc gridSpan="2" rowSpan="2">
                                <a:txBody><a:bodyPr/><a:p><a:r><a:t>Merged</a:t></a:r></a:p></a:txBody>
                            </a:tc>
                            <a:tc hMerge="1"/>
                        </a:tr>
                        <a:tr h="914400">
                            <a:tc><a:txBody><a:bodyPr/><a:p><a:r><a:t>Grace</a:t></a:r></a:p></a:txBody></a:tc>
                            <a:tc vMerge="1"/>
                            <a:tc vMerge="1"/>
                        </a:tr>
                    </a:tbl>
                </a:graphicData></a:graphic>
            </p:graphicFrame>
        </p:spTree></p:cSld>
    </p:sld>"#;

    let bytes = fixture_pptx(&[slide], &[]);
    let result = import_pptx(&bytes, "Table Deck").expect("import success");
    let slide = &result.course.scenes[0].slides[0];
    let (object, _) = slide.object("table1").expect("imported table");
    let coursecraft_course::ObjectKind::Table {
        columns,
        rows,
        borders,
        header_row,
    } = &object.kind
    else {
        panic!("expected a table");
    };
    assert_eq!(columns.len(), 3);
    assert_eq!(rows.len(), 3);
    assert!(header_row);
    assert_eq!(rows[0].cells[0].text, "Name");
    assert_eq!(rows[0].cells[1].text, "Score");
    assert_eq!(rows[1].cells[0].text, "Ada");
    assert_eq!(rows[1].cells[1].text, "Merged");
    assert_eq!(rows[1].cells[1].row_span, 2);
    assert_eq!(rows[1].cells[1].col_span, 2);
    assert!(rows[1].cells[2].text.is_empty(), "covered cell stays empty");
    assert_eq!(rows[2].cells[0].text, "Grace");
    assert!(
        rows[0].cells[0]
            .fill
            .as_ref()
            .is_some_and(|fill| fill.0 == "#2F6FEB"),
        "solid cell fill imported"
    );
    let inner = borders.inner.as_ref().expect("inner border");
    assert_eq!(inner.color.0, "#111111");
    assert!(inner.width >= 1.0);
    assert!(
        !result
            .report
            .items
            .iter()
            .any(|item| item.kind == SkipKind::Table),
        "tables are imported, not skipped"
    );
    assert!(
        result.course.is_playable(),
        "{:?}",
        result.course.validate()
    );
}

#[test]
fn test_all_skip_kinds_recorded_in_report() {
    let slide = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
    <p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
           xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
           xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
        <p:transition><p:fade/></p:transition>
        <p:timing><p:tnLst/></p:timing>
        <p:cSld>
            <p:spTree>
                <!-- 1. Connector -->
                <p:cxnSp><p:nvCxnSpPr><p:cNvPr id="2" name="Line"/></p:nvCxnSpPr></p:cxnSp>

                <!-- 2. GraphicFrame: Table -->
                <p:graphicFrame>
                    <p:nvGraphicFramePr><p:cNvPr id="3" name="Table 1"/></p:nvGraphicFramePr>
                    <a:graphic><a:graphicData><a:tbl/></a:graphicData></a:graphic>
                </p:graphicFrame>

                <!-- 3. GraphicFrame: Chart -->
                <p:graphicFrame>
                    <p:nvGraphicFramePr><p:cNvPr id="4" name="Chart 1"/></p:nvGraphicFramePr>
                    <a:graphic><a:graphicData><c:chart xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart"/></a:graphicData></a:graphic>
                </p:graphicFrame>

                <!-- 4. GraphicFrame: SmartArt -->
                <p:graphicFrame>
                    <p:nvGraphicFramePr><p:cNvPr id="5" name="Diagram"/></p:nvGraphicFramePr>
                    <a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/diagram"/></a:graphic>
                </p:graphicFrame>

                <!-- 5. GraphicFrame: OLE Object -->
                <p:graphicFrame>
                    <p:nvGraphicFramePr><p:cNvPr id="6" name="OLE"/></p:nvGraphicFramePr>
                    <p:oleObj/>
                </p:graphicFrame>

                <!-- 6. Custom geometry -->
                <p:sp>
                    <p:nvSpPr><p:cNvPr id="7" name="Custom"/></p:nvSpPr>
                    <p:spPr>
                        <a:xfrm><a:off x="0" y="0"/><a:ext cx="100" cy="100"/></a:xfrm>
                        <a:custGeom/>
                    </p:spPr>
                </p:sp>

                <!-- 7. Scheme colour fill -->
                <p:sp>
                    <p:nvSpPr><p:cNvPr id="8" name="Scheme Color"/></p:nvSpPr>
                    <p:spPr>
                        <a:xfrm><a:off x="0" y="0"/><a:ext cx="100" cy="100"/></a:xfrm>
                        <a:prstGeom prst="rect"/>
                        <a:solidFill><a:schemeClr val="accent1"/></a:solidFill>
                    </p:spPr>
                </p:sp>

                <!-- 8. Unsupported fill (gradient) -->
                <p:sp>
                    <p:nvSpPr><p:cNvPr id="9" name="Grad Fill"/></p:nvSpPr>
                    <p:spPr>
                        <a:xfrm><a:off x="0" y="0"/><a:ext cx="100" cy="100"/></a:xfrm>
                        <a:prstGeom prst="rect"/>
                        <a:gradFill/>
                    </p:spPr>
                </p:sp>

                <!-- 9. Empty placeholder without text -->
                <p:sp>
                    <p:nvSpPr>
                        <p:cNvPr id="10" name="Empty PH"/>
                        <p:nvPr><p:ph type="body"/></p:nvPr>
                    </p:nvSpPr>
                    <p:spPr>
                        <a:xfrm><a:off x="0" y="0"/><a:ext cx="100" cy="100"/></a:xfrm>
                        <a:prstGeom prst="rect"/>
                    </p:spPr>
                    <p:txBody><a:bodyPr/><a:p/></p:txBody>
                </p:sp>

                <!-- 10. Rotation or flip -->
                <p:sp>
                    <p:nvSpPr><p:cNvPr id="11" name="Rotated"/></p:nvSpPr>
                    <p:spPr>
                        <a:xfrm rot="5400000"><a:off x="0" y="0"/><a:ext cx="100" cy="100"/></a:xfrm>
                        <a:prstGeom prst="rect"/>
                    </p:spPr>
                    <p:txBody><a:bodyPr/><a:p><a:r><a:t>Rotated</a:t></a:r></a:p></p:txBody>
                </p:sp>

                <!-- 11. Media in pic -->
                <p:pic>
                    <p:nvPicPr>
                        <p:cNvPr id="12" name="Video"/>
                        <p:cNvPicPr/>
                        <p:nvPr><a:videoFile/></p:nvPr>
                    </p:nvPicPr>
                    <p:blipFill><a:blip r:embed="rIdV"/></p:blipFill>
                    <p:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="100" cy="100"/></a:xfrm></p:spPr>
                </p:pic>
            </p:spTree>
        </p:cSld>
    </p:sld>"#;

    let bytes = fixture_pptx(&[slide], &[]);
    let res = import_pptx(&bytes, "Skips").expect("import success");

    let kinds: Vec<SkipKind> = res.report.items.iter().map(|i| i.kind).collect();

    assert!(kinds.contains(&SkipKind::Transition));
    assert!(kinds.contains(&SkipKind::Animation));
    assert!(kinds.contains(&SkipKind::Connector));
    assert!(kinds.contains(&SkipKind::Table));
    assert!(kinds.contains(&SkipKind::Chart));
    assert!(kinds.contains(&SkipKind::SmartArt));
    assert!(kinds.contains(&SkipKind::OleObject));
    assert!(kinds.contains(&SkipKind::CustomGeometry));
    assert!(kinds.contains(&SkipKind::SchemeColor));
    assert!(kinds.contains(&SkipKind::UnsupportedFill));
    assert!(kinds.contains(&SkipKind::EmptyPlaceholder));
    assert!(kinds.contains(&SkipKind::Media));

    let window_summary = res.report.summary_window_text();
    assert!(window_summary.starts_with("Imported 1 slide. Skipped:"));
    assert!(res.course.is_playable(), "{:?}", res.course.validate());
}

#[test]
fn test_hidden_slide_skipped() {
    let slide1 = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
    <p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
           xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
        <p:cSld><p:spTree/></p:cSld>
    </p:sld>"#;

    let slide2 = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
    <p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
           xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
           show="0">
        <p:cSld><p:spTree/></p:cSld>
    </p:sld>"#;

    let bytes = fixture_pptx(&[slide1, slide2], &[]);
    let res = import_pptx(&bytes, "Hidden Test").expect("import success");

    assert_eq!(res.course.scenes[0].slides.len(), 1);
    assert_eq!(res.course.scenes[0].slides[0].id, "slide1");
    assert!(
        res.report
            .items
            .iter()
            .any(|i| i.kind == SkipKind::HiddenSlide && i.slide == 2)
    );
    assert!(res.course.is_playable(), "{:?}", res.course.validate());
}

#[test]
fn test_huge_negative_nonnumeric_emus_skipped_without_panic() {
    let slide = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
    <p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
           xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
        <p:cSld>
            <p:spTree>
                <!-- Negative EMU -->
                <p:sp>
                    <p:nvSpPr><p:cNvPr id="2" name="Neg"/></p:nvSpPr>
                    <p:spPr>
                        <a:xfrm><a:off x="-500" y="100"/><a:ext cx="1000" cy="1000"/></a:xfrm>
                        <a:prstGeom prst="rect"/>
                    </p:spPr>
                </p:sp>
                <!-- Non-numeric EMU -->
                <p:sp>
                    <p:nvSpPr><p:cNvPr id="3" name="NonNum"/></p:nvSpPr>
                    <p:spPr>
                        <a:xfrm><a:off x="100" y="xyz"/><a:ext cx="1000" cy="1000"/></a:xfrm>
                        <a:prstGeom prst="rect"/>
                    </p:spPr>
                </p:sp>
                <!-- Huge EMU -->
                <p:sp>
                    <p:nvSpPr><p:cNvPr id="4" name="Huge"/></p:nvSpPr>
                    <p:spPr>
                        <a:xfrm><a:off x="0" y="0"/><a:ext cx="999999999999999999" cy="1000"/></a:xfrm>
                        <a:prstGeom prst="rect"/>
                    </p:spPr>
                </p:sp>
                <!-- Valid shape -->
                <p:sp>
                    <p:nvSpPr><p:cNvPr id="5" name="Valid"/></p:nvSpPr>
                    <p:spPr>
                        <a:xfrm><a:off x="100" y="100"/><a:ext cx="1000" cy="1000"/></a:xfrm>
                        <a:prstGeom prst="rect"/>
                        <a:solidFill><a:srgbClr val="00AA00"/></a:solidFill>
                    </p:spPr>
                </p:sp>
            </p:spTree>
        </p:cSld>
    </p:sld>"#;

    let bytes = fixture_pptx(&[slide], &[]);
    let res = import_pptx(&bytes, "Hostile EMU").expect("import success");

    let slide = &res.course.scenes[0].slides[0];
    // A negative position is legal: that shape sits partly off the slide.
    assert_eq!(slide.objects.len(), 2);
    assert_eq!(slide.objects[0].name, "Neg");
    assert!(slide.objects[0].frame.x < 0.0);
    assert_eq!(slide.objects[1].name, "Valid");
    assert!(
        res.report
            .items
            .iter()
            .any(|i| i.kind == SkipKind::InvalidGeometry)
    );
    assert!(res.course.is_playable(), "{:?}", res.course.validate());
}

#[test]
fn test_errors_for_corrupt_zip_malformed_xml_missing_pres_and_limits() {
    // 1. Not a zip
    let err = import_pptx(b"not a zip file at all", "Fail").unwrap_err();
    assert!(matches!(err, PptxError::NotAZip(_)));

    // 2. Missing presentation.xml
    let mut buffer = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buffer);
        let opt = zip::write::SimpleFileOptions::default();
        zip.start_file("dummy.txt", opt).unwrap();
        zip.write_all(b"hello").unwrap();
        zip.finish().unwrap();
    }
    let err = import_pptx(&buffer.into_inner(), "No Pres").unwrap_err();
    assert_eq!(err, PptxError::MissingPresentationXml);

    // 3. Malformed XML in slide
    let bad_xml = r#"<p:sld><unclosed>"#;
    let bytes = fixture_pptx(&[bad_xml], &[]);
    let err = import_pptx(&bytes, "Bad XML").unwrap_err();
    assert!(matches!(err, PptxError::MalformedXml { .. }));

    // 4. Exceeds max entries limit
    let slide = r#"<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cSld><p:spTree/></p:cSld></p:sld>"#;
    let bytes = fixture_pptx(&[slide], &[]);
    let limits = Limits {
        max_entries: 1, // Only 1 entry allowed, fixture has at least 3
        ..Default::default()
    };
    let err = import_pptx_with_limits(&bytes, "Too Many Entries", limits).unwrap_err();
    assert_eq!(err, PptxError::TooManyEntries);

    // 5. Exceeds max XML depth limit
    let deep_xml = r#"<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><a><b><c><d><e/></d></c></b></a></p:sld>"#;
    let bytes = fixture_pptx(&[deep_xml], &[]);
    let limits = Limits {
        max_xml_depth: 3,
        ..Default::default()
    };
    let err = import_pptx_with_limits(&bytes, "Deep XML", limits).unwrap_err();
    assert!(matches!(err, PptxError::XmlDepthExceeded(_)));
}

#[test]
fn test_package_escaping_path_rejected() {
    let mut buffer = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buffer);
        let opt = zip::write::SimpleFileOptions::default();
        zip.start_file("../outside.xml", opt).unwrap();
        zip.write_all(b"evil").unwrap();
        zip.finish().unwrap();
    }
    let err = import_pptx(&buffer.into_inner(), "Escape").unwrap_err();
    assert!(matches!(err, PptxError::UnsafePath(_)));
}

#[test]
fn test_always_validates() {
    let slide = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
    <p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
           xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
        <p:cSld>
            <p:spTree>
                <p:sp>
                    <p:nvSpPr><p:cNvPr id="2" name="Title"/><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr>
                    <p:spPr>
                        <a:xfrm><a:off x="914400" y="914400"/><a:ext cx="5000000" cy="800000"/></a:xfrm>
                        <a:prstGeom prst="rect"/>
                    </p:spPr>
                    <p:txBody>
                        <a:bodyPr/>
                        <a:p><a:r><a:t>Valid Slide</a:t></a:r></a:p>
                    </p:txBody>
                </p:sp>
            </p:spTree>
        </p:cSld>
    </p:sld>"#;

    let bytes = fixture_pptx(&[slide], &[]);
    let res = import_pptx(&bytes, "Valid Deck").expect("import success");
    assert!(res.course.is_playable(), "{:?}", res.course.validate());
}

// ---------------------------------------------------------------------------
// Placeholder geometry inheritance and run sizes
// ---------------------------------------------------------------------------

const NS: &str = r#"xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main""#;

/// A `<p:sp>` placeholder with empty `spPr` (inherited geometry).
fn ph_sp(ph_attrs: &str, text: &str) -> String {
    format!(
        r#"<p:sp><p:nvSpPr><p:cNvPr id="2" name="ph"/><p:nvPr><p:ph {ph_attrs}/></p:nvPr></p:nvSpPr><p:spPr/><p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:t>{text}</a:t></a:r></a:p></p:txBody></p:sp>"#
    )
}

fn slide_xml(body: &str) -> String {
    format!(r#"<p:sld {NS}><p:cSld><p:spTree>{body}</p:spTree></p:cSld></p:sld>"#)
}

/// A layout/master placeholder shape carrying geometry.
fn geo_sp(ph_attrs: &str, x: u32, y: u32, w: u32, h: u32) -> String {
    format!(
        r#"<p:sp><p:nvSpPr><p:cNvPr id="3" name="g"/><p:nvPr><p:ph {ph_attrs}/></p:nvPr></p:nvSpPr><p:spPr><a:xfrm><a:off x="{x}" y="{y}"/><a:ext cx="{w}" cy="{h}"/></a:xfrm></p:spPr></p:sp>"#
    )
}

fn rels(entries: &[(&str, &str)]) -> String {
    let mut out = String::from(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#,
    );
    for (i, (kind, target)) in entries.iter().enumerate() {
        out.push_str(&format!(
            r#"<Relationship Id="rId{}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/{kind}" Target="{target}"/>"#,
            i + 1
        ));
    }
    out.push_str("</Relationships>");
    out
}

#[test]
fn test_placeholders_inherit_geometry_from_layout_and_master() {
    // 12192000 x 6858000 EMU => 1280 x 720 at scale 1280/12192000.
    let slide = slide_xml(&format!(
        "{}{}{}",
        ph_sp(r#"type="title""#, "My Title"),
        ph_sp(r#"idx="1""#, "Body text"),
        ph_sp(r#"type="ftr" idx="11""#, "Footer text"),
    ));
    let layout = slide_xml(&format!(
        "{}{}",
        geo_sp(r#"type="title""#, 1_219_200, 609_600, 6_096_000, 1_219_200),
        geo_sp(r#"idx="1""#, 1_219_200, 2_438_400, 9_144_000, 3_048_000),
    ));
    // Master only provides the footer; layout lacks it.
    let master = slide_xml(&geo_sp(
        r#"type="ftr" idx="11""#,
        0,
        6_096_000,
        12_192_000,
        762_000,
    ));
    let slide_rels = rels(&[("slideLayout", "../slideLayouts/slideLayout1.xml")]);
    let layout_rels = rels(&[("slideMaster", "../slideMasters/slideMaster1.xml")]);
    let bytes = fixture_pptx_with_size(
        &[&slide],
        12_192_000,
        6_858_000,
        &[
            ("ppt/slides/_rels/slide1.xml.rels", slide_rels.as_bytes()),
            ("ppt/slideLayouts/slideLayout1.xml", layout.as_bytes()),
            (
                "ppt/slideLayouts/_rels/slideLayout1.xml.rels",
                layout_rels.as_bytes(),
            ),
            ("ppt/slideMasters/slideMaster1.xml", master.as_bytes()),
        ],
    );
    let res = import_pptx(&bytes, "Deck").expect("import");
    let s = &res.course.scenes[0].slides[0];
    assert_eq!(s.title, "My Title");
    assert_eq!(s.objects.len(), 3);
    let k = 1280.0 / 12_192_000.0_f32;
    let f = |i: usize| s.objects[i].frame;
    assert!((f(0).x - 1_219_200.0 * k).abs() < 0.5);
    assert!((f(0).y - 609_600.0 * k).abs() < 0.5);
    assert!((f(0).w - 6_096_000.0 * k).abs() < 0.5);
    assert!((f(1).y - 2_438_400.0 * k).abs() < 0.5);
    assert!((f(1).w - 9_144_000.0 * k).abs() < 0.5);
    assert!((f(2).y - 6_096_000.0 * k).abs() < 0.5);
    assert!((f(2).w - 1280.0).abs() < 0.5);
    // Unsized placeholder text falls back to the Office defaults: 44 pt for the title.
    match &s.objects[0].kind {
        coursecraft_course::ObjectKind::Text { style, .. } => {
            assert!(
                (style.size - 44.0 * 4.0 / 3.0).abs() < 0.01,
                "{}",
                style.size
            );
        }
        other => panic!("expected text, got {other:?}"),
    }
    assert!(res.course.is_playable(), "{:?}", res.course.validate());
}

#[test]
fn test_ctr_title_matches_layout_title_and_subtitle_by_idx() {
    let slide = slide_xml(&format!(
        "{}{}",
        ph_sp(r#"type="ctrTitle""#, "Big"),
        ph_sp(r#"type="subTitle" idx="1""#, "Sub"),
    ));
    let layout = slide_xml(&format!(
        "{}{}",
        geo_sp(r#"type="ctrTitle""#, 100_000, 200_000, 3_000_000, 1_000_000),
        geo_sp(
            r#"type="subTitle" idx="1""#,
            100_000,
            1_500_000,
            3_000_000,
            500_000
        ),
    ));
    let slide_rels = rels(&[("slideLayout", "../slideLayouts/slideLayout1.xml")]);
    let bytes = fixture_pptx_with_size(
        &[&slide],
        12_192_000,
        6_858_000,
        &[
            ("ppt/slides/_rels/slide1.xml.rels", slide_rels.as_bytes()),
            ("ppt/slideLayouts/slideLayout1.xml", layout.as_bytes()),
        ],
    );
    let res = import_pptx(&bytes, "Deck").expect("import");
    let s = &res.course.scenes[0].slides[0];
    assert_eq!(s.title, "Big");
    assert_eq!(s.objects.len(), 2);
}

#[test]
fn test_unresolvable_placeholder_keeps_title_and_reports() {
    let slide = slide_xml(&format!(
        "{}{}",
        ph_sp(r#"type="title""#, "Still The Title"),
        ph_sp(r#"idx="1""#, "Orphan body"),
    ));
    // No rels, no layout at all.
    let bytes = fixture_pptx(&[&slide], &[]);
    let res = import_pptx(&bytes, "Deck").expect("import");
    let s = &res.course.scenes[0].slides[0];
    assert_eq!(s.title, "Still The Title");
    assert!(s.objects.is_empty());
    assert!(
        res.report
            .items
            .iter()
            .any(|i| i.note.contains("Placeholder without position skipped"))
    );
    assert!(res.course.is_playable(), "{:?}", res.course.validate());
}

#[test]
fn test_empty_placeholder_still_skipped_with_inherited_geometry() {
    let slide = slide_xml(
        r#"<p:sp><p:nvSpPr><p:cNvPr id="2" name="e"/><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr><p:spPr/><p:txBody><a:bodyPr/><a:p/></p:txBody></p:sp>"#,
    );
    let bytes = fixture_pptx(&[&slide], &[]);
    let res = import_pptx(&bytes, "Deck").expect("import");
    let s = &res.course.scenes[0].slides[0];
    assert!(s.objects.is_empty());
    assert_eq!(s.title, "Slide 1");
}

fn sized_run_slide() -> String {
    slide_xml(
        r#"<p:sp><p:nvSpPr><p:cNvPr id="2" name="T"/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="3000000" cy="1000000"/></a:xfrm></p:spPr><p:txBody><a:bodyPr/><a:p><a:r><a:rPr sz="2800"/><a:t>Big</a:t></a:r><a:r><a:t>Plain</a:t></a:r></a:p></p:txBody></p:sp>"#,
    )
}

fn run_sizes(bytes: &[u8]) -> (Option<f32>, Option<f32>, f32) {
    let res = import_pptx(bytes, "Deck").expect("import");
    let s = &res.course.scenes[0].slides[0];
    match &s.objects[0].kind {
        coursecraft_course::ObjectKind::Text { runs, style, .. } => {
            (runs[0].size, runs[1].size, style.size)
        }
        other => panic!("expected text, got {other:?}"),
    }
}

#[test]
fn test_run_size_exact_on_16_9_and_4_3() {
    let slide = sized_run_slide();
    // 13.333in wide 16:9 deck: 96 px per inch, scale factor 1.0.
    let bytes = fixture_pptx_with_size(&[&slide], 12_192_000, 6_858_000, &[]);
    let (a, b, style) = run_sizes(&bytes);
    assert!((a.unwrap() - 37.333).abs() < 0.01);
    assert_eq!(b, None);
    assert!((style - 37.333).abs() < 0.01);

    // 10x7.5in 4:3 deck letterboxed into 720 px height: also 96 px per inch.
    let bytes = fixture_pptx_with_size(&[&slide], 9_144_000, 6_858_000, &[]);
    let (a, b, _) = run_sizes(&bytes);
    assert!((a.unwrap() - 37.333).abs() < 0.01);
    assert_eq!(b, None);

    // 10in wide 16:9 deck: 128 px per inch => 28pt = 49.78 px.
    let bytes = fixture_pptx(&[&slide], &[]);
    let (a, _, _) = run_sizes(&bytes);
    assert!((a.unwrap() - 49.778).abs() < 0.01);
}

#[test]
fn test_hostile_layout_and_master_references_do_not_panic() {
    let slide = slide_xml(&ph_sp(r#"type="title""#, "T"));
    // Layout rel escapes the package; another points to a missing part.
    for target in [
        "../../../../etc/passwd",
        "../slideLayouts/missing.xml",
        "http://example.com/layout.xml",
        "/ppt/slideLayouts/slideLayout1.xml",
    ] {
        let slide_rels = rels(&[("slideLayout", target)]);
        let bytes = fixture_pptx(
            &[&slide],
            &[("ppt/slides/_rels/slide1.xml.rels", slide_rels.as_bytes())],
        );
        let res = import_pptx(&bytes, "Deck").expect("import");
        let s = &res.course.scenes[0].slides[0];
        assert_eq!(s.title, "T");
        assert!(s.objects.is_empty());
        assert!(
            res.report
                .items
                .iter()
                .any(|i| i.note.contains("without position"))
        );
    }

    // Circular: layout's master rel points back to the layout; master points
    // to the layout too. No geometry anywhere -> terminates with a report.
    let slide_rels = rels(&[("slideLayout", "../slideLayouts/slideLayout1.xml")]);
    let layout_rels = rels(&[("slideMaster", "slideLayout1.xml")]);
    let layout = slide_xml("");
    let bytes = fixture_pptx(
        &[&slide],
        &[
            ("ppt/slides/_rels/slide1.xml.rels", slide_rels.as_bytes()),
            ("ppt/slideLayouts/slideLayout1.xml", layout.as_bytes()),
            (
                "ppt/slideLayouts/_rels/slideLayout1.xml.rels",
                layout_rels.as_bytes(),
            ),
        ],
    );
    let res = import_pptx(&bytes, "Deck").expect("import");
    assert_eq!(res.course.scenes[0].slides[0].title, "T");
    assert!(res.course.scenes[0].slides[0].objects.is_empty());

    // Malformed layout XML must not fail the import.
    let slide_rels = rels(&[("slideLayout", "../slideLayouts/slideLayout1.xml")]);
    let bytes = fixture_pptx(
        &[&slide],
        &[
            ("ppt/slides/_rels/slide1.xml.rels", slide_rels.as_bytes()),
            ("ppt/slideLayouts/slideLayout1.xml", b"<p:sldLayout><oops"),
        ],
    );
    let res = import_pptx(&bytes, "Deck").expect("import");
    assert_eq!(res.course.scenes[0].slides[0].title, "T");
}

#[test]
fn test_unaligned_shape_text_centres_but_text_box_stays_left() {
    let slide = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
    <p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
           xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
        <p:cSld>
            <p:spTree>
                <p:sp>
                    <p:nvSpPr><p:cNvPr id="2" name="Box"/><p:cNvSpPr/></p:nvSpPr>
                    <p:spPr>
                        <a:xfrm><a:off x="0" y="0"/><a:ext cx="2000000" cy="800000"/></a:xfrm>
                        <a:prstGeom prst="rect"/>
                    </p:spPr>
                    <p:txBody><a:bodyPr/><a:p><a:r><a:t>Shape</a:t></a:r></a:p></p:txBody>
                </p:sp>
                <p:sp>
                    <p:nvSpPr><p:cNvPr id="3" name="TextBox"/><p:cNvSpPr txBox="1"/></p:nvSpPr>
                    <p:spPr>
                        <a:xfrm><a:off x="0" y="1000000"/><a:ext cx="2000000" cy="800000"/></a:xfrm>
                        <a:prstGeom prst="rect"/>
                    </p:spPr>
                    <p:txBody><a:bodyPr/><a:p><a:r><a:t>Box text</a:t></a:r></a:p></p:txBody>
                </p:sp>
            </p:spTree>
        </p:cSld>
    </p:sld>"#;
    let bytes = fixture_pptx(&[slide], &[]);
    let res = import_pptx(&bytes, "Align").expect("import success");
    let aligns: Vec<_> = res.course.scenes[0].slides[0]
        .objects
        .iter()
        .filter_map(|o| match &o.kind {
            coursecraft_course::ObjectKind::Text { style, .. } => Some(style.align),
            _ => None,
        })
        .collect();
    assert_eq!(
        aligns,
        vec![
            coursecraft_course::Align::Center,
            coursecraft_course::Align::Left
        ]
    );
}

#[test]
fn preset_adjustment_guides_map_to_coursecraft_values() {
    use std::f32::consts::{PI, TAU};

    fn mapped(preset: &str, kind: ShapeKind, gds: &str) -> Vec<f32> {
        let xml = format!("<root><a:avLst>{gds}</a:avLst></root>");
        let node = crate::parse_xml_tree(&xml, "test", 20).expect("parse fragment");
        let av = node.find_descendant("avLst");
        crate::map_adjust(kind, preset, av)
    }
    fn close(got: f32, expected: f32) {
        assert!(
            (got - expected).abs() <= 1e-4,
            "got {got}, expected {expected}"
        );
    }
    let one = |preset, kind, value| {
        mapped(
            preset,
            kind,
            &format!("<a:gd name=\"adj\" fmla=\"val {value}\"/>"),
        )
    };
    let two = |preset, kind, a, b| {
        mapped(
            preset,
            kind,
            &format!(
                "<a:gd name=\"adj1\" fmla=\"val {a}\"/><a:gd name=\"adj2\" fmla=\"val {b}\"/>"
            ),
        )
    };

    assert_eq!(
        one("roundRect", ShapeKind::RoundedRectangle, 25_000),
        vec![0.25]
    );
    assert_eq!(
        one("parallelogram", ShapeKind::Parallelogram, 40_000),
        vec![0.4]
    );
    assert_eq!(
        two("rightArrow", ShapeKind::ArrowRight, 60_000, 40_000),
        vec![0.6, 0.2]
    );
    assert_eq!(
        one("homePlate", ShapeKind::ArrowPentagon, 30_000),
        vec![0.3]
    );
    assert_eq!(one("chevron", ShapeKind::ArrowChevron, 40_000), vec![0.4]);
    assert_eq!(one("donut", ShapeKind::Donut, 30_000), vec![0.3]);
    assert_eq!(one("frame", ShapeKind::Frame, 20_000), vec![0.2]);
    assert_eq!(
        one("leftBracket", ShapeKind::LeftBracket, 40_000),
        vec![0.4]
    );
    assert_eq!(one("star5", ShapeKind::Star5, 30_000), vec![0.6]);

    let wedge = two(
        "wedgeRectCallout",
        ShapeKind::CalloutRectangle,
        -10_000,
        50_000,
    );
    close(wedge[0], 0.4);
    close(wedge[1], 1.0);

    let pie = two("pie", ShapeKind::Pie, 0, 9_000_000);
    close(pie[0], 0.0);
    close(pie[1], PI * 5.0 / 6.0);
    assert_eq!(pie[1].signum(), 1.0);

    let block = mapped(
        "blockArc",
        ShapeKind::BlockArc,
        "<a:gd name=\"adj1\" fmla=\"val 10800000\"/><a:gd name=\"adj2\" fmla=\"val 0\"/><a:gd name=\"adj3\" fmla=\"val 30000\"/>",
    );
    close(block[0], 0.3);
    close(block[1], PI);
    close(block[2], TAU);

    let line = mapped(
        "callout2",
        ShapeKind::CalloutLineBent,
        "<a:gd name=\"adj1\" fmla=\"val -10000\"/><a:gd name=\"adj2\" fmla=\"val 60000\"/><a:gd name=\"adj3\" fmla=\"val -10000\"/><a:gd name=\"adj4\" fmla=\"val 90000\"/>",
    );
    close(line[0], 0.4);
    close(line[1], 1.1);
    close(line[2], 0.4);
    close(line[3], 1.4);

    assert!(one("noSuchPreset", ShapeKind::RoundedRectangle, 25_000).is_empty());
    assert!(mapped("roundRect", ShapeKind::RoundedRectangle, "").is_empty());
}

#[test]
fn connectors_import_as_paths_with_style_ends_and_flips() {
    let slide = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
    <p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
           xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
        <p:cSld>
            <p:spTree>
                <p:cxnSp>
                    <p:nvCxnSpPr><p:cNvPr id="2" name="Straight"/></p:nvCxnSpPr>
                    <p:spPr>
                        <a:xfrm><a:off x="100000" y="100000"/><a:ext cx="1000000" cy="800000"/></a:xfrm>
                        <a:prstGeom prst="straightConnector1"/>
                        <a:ln w="25400">
                            <a:solidFill><a:srgbClr val="FF0000"/></a:solidFill>
                            <a:headEnd type="triangle"/>
                            <a:tailEnd type="diamond"/>
                        </a:ln>
                    </p:spPr>
                </p:cxnSp>
                <p:cxnSp>
                    <p:nvCxnSpPr><p:cNvPr id="3" name="Elbow flip H"/></p:nvCxnSpPr>
                    <p:spPr>
                        <a:xfrm flipH="1"><a:off x="1200000" y="100000"/><a:ext cx="1000000" cy="800000"/></a:xfrm>
                        <a:prstGeom prst="bentConnector3"/>
                    </p:spPr>
                </p:cxnSp>
                <p:cxnSp>
                    <p:nvCxnSpPr><p:cNvPr id="4" name="Curved flip V"/></p:nvCxnSpPr>
                    <p:spPr>
                        <a:xfrm flipV="1"><a:off x="2300000" y="100000"/><a:ext cx="1000000" cy="800000"/></a:xfrm>
                        <a:prstGeom prst="curvedConnector2"/>
                    </p:spPr>
                </p:cxnSp>
                <p:cxnSp>
                    <p:nvCxnSpPr><p:cNvPr id="5" name="Both flips"/></p:nvCxnSpPr>
                    <p:spPr>
                        <a:xfrm flipH="1" flipV="1"><a:off x="3400000" y="100000"/><a:ext cx="1000000" cy="800000"/></a:xfrm>
                        <a:prstGeom prst="straightConnector1"/>
                    </p:spPr>
                </p:cxnSp>
            </p:spTree>
        </p:cSld>
    </p:sld>"#;
    let bytes = fixture_pptx(&[slide], &[]);
    let res = import_pptx(&bytes, "Connectors").expect("import success");
    let slide = &res.course.scenes[0].slides[0];
    assert_eq!(slide.objects.len(), 4);
    let mut paths = Vec::new();
    for object in &slide.objects {
        match &object.kind {
            coursecraft_course::ObjectKind::Path { path } => paths.push(path),
            other => panic!("expected path, got {other:?}"),
        }
    }
    assert_eq!(
        paths[0].connector,
        Some(coursecraft_course::ConnectorStyle::Straight)
    );
    assert_eq!(
        paths[1].connector,
        Some(coursecraft_course::ConnectorStyle::Elbow)
    );
    assert_eq!(
        paths[2].connector,
        Some(coursecraft_course::ConnectorStyle::Curved)
    );
    assert_eq!(paths[0].start_end, coursecraft_course::LineEnd::Arrow);
    assert_eq!(paths[0].end_end, coursecraft_course::LineEnd::Diamond);
    assert_eq!(paths[0].outline.color.0, "#FF0000");
    assert!(paths[0].outline.width > 0.0 && paths[0].outline.width <= 50.0);

    let corners = |path: &coursecraft_course::Path| {
        let first = path.points.first().copied().unwrap_or_default();
        let last = path.points.last().copied().unwrap_or_default();
        ((first.x, first.y), (last.x, last.y))
    };
    assert_eq!(corners(paths[0]), ((0.0, 0.0), (1.0, 1.0)));
    assert_eq!(corners(paths[1]), ((1.0, 0.0), (0.0, 1.0)));
    assert_eq!(corners(paths[2]), ((0.0, 1.0), (1.0, 0.0)));
    assert_eq!(corners(paths[3]), ((1.0, 1.0), (0.0, 0.0)));
    assert!(res.course.is_playable(), "{:?}", res.course.validate());
}

#[test]
fn connector_attachments_import_as_glue_to_same_slide_shapes() {
    let slide = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
    <p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
           xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
        <p:cSld>
            <p:spTree>
                <p:sp>
                    <p:nvSpPr><p:cNvPr id="2" name="Left box"/></p:nvSpPr>
                    <p:spPr>
                        <a:xfrm><a:off x="100000" y="100000"/><a:ext cx="1000000" cy="800000"/></a:xfrm>
                        <a:prstGeom prst="rect"/>
                        <a:solidFill><a:srgbClr val="D9D9D9"/></a:solidFill>
                    </p:spPr>
                </p:sp>
                <p:sp>
                    <p:nvSpPr><p:cNvPr id="3" name="Right box"/></p:nvSpPr>
                    <p:spPr>
                        <a:xfrm><a:off x="3000000" y="100000"/><a:ext cx="1000000" cy="800000"/></a:xfrm>
                        <a:prstGeom prst="rect"/>
                        <a:solidFill><a:srgbClr val="D9D9D9"/></a:solidFill>
                    </p:spPr>
                </p:sp>
                <p:cxnSp>
                    <p:nvCxnSpPr><p:cNvPr id="4" name="Joined"/></p:nvCxnSpPr>
                    <p:spPr>
                        <a:xfrm><a:off x="1100000" y="500000"/><a:ext cx="1900000" cy="1"/></a:xfrm>
                        <a:prstGeom prst="straightConnector1"/>
                    </p:spPr>
                    <p:stCxn id="2" idx="3"/>
                    <p:endCxn id="3" idx="1"/>
                </p:cxnSp>
            </p:spTree>
        </p:cSld>
    </p:sld>"#;
    let bytes = fixture_pptx(&[slide], &[]);
    let res = import_pptx(&bytes, "Glue").expect("import success");
    let slide = &res.course.scenes[0].slides[0];
    assert_eq!(slide.objects.len(), 3);
    let left = slide.objects[0].id.clone();
    let right = slide.objects[1].id.clone();
    let coursecraft_course::ObjectKind::Path { path } = &slide.objects[2].kind else {
        panic!("expected a connector path");
    };
    let start = path.start_glue.as_ref().expect("start glue");
    let end = path.end_glue.as_ref().expect("end glue");
    assert_eq!(start.object, left);
    assert_eq!(start.site(), 1, "idx 3 is the right side");
    assert_eq!(end.object, right);
    assert_eq!(end.site(), 3, "idx 1 is the left side");
    assert_eq!(path.points.len(), 2, "import keeps the original ends");
    assert!(res.course.is_playable(), "{:?}", res.course.validate());
}

#[test]
fn connector_to_an_ellipse_uses_the_nearest_site() {
    let slide = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
    <p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
           xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
        <p:cSld>
            <p:spTree>
                <p:sp>
                    <p:nvSpPr><p:cNvPr id="2" name="Oval"/></p:nvSpPr>
                    <p:spPr>
                        <a:xfrm><a:off x="1000000" y="1000000"/><a:ext cx="1000000" cy="800000"/></a:xfrm>
                        <a:prstGeom prst="ellipse"/>
                        <a:solidFill><a:srgbClr val="D9D9D9"/></a:solidFill>
                    </p:spPr>
                </p:sp>
                <p:cxnSp>
                    <p:nvCxnSpPr><p:cNvPr id="3" name="To oval"/></p:nvCxnSpPr>
                    <p:spPr>
                        <a:xfrm><a:off x="100000" y="1000000"/><a:ext cx="900000" cy="1"/></a:xfrm>
                        <a:prstGeom prst="straightConnector1"/>
                    </p:spPr>
                    <p:stCxn id="2" idx="1"/>
                </p:cxnSp>
            </p:spTree>
        </p:cSld>
    </p:sld>"#;
    let bytes = fixture_pptx(&[slide], &[]);
    let res = import_pptx(&bytes, "Oval glue").expect("import success");
    let slide = &res.course.scenes[0].slides[0];
    let coursecraft_course::ObjectKind::Path { path } = &slide.objects[1].kind else {
        panic!("expected a connector path");
    };
    let glue = path.start_glue.as_ref().expect("start glue");
    assert_eq!(glue.object, slide.objects[0].id);
    assert_eq!(
        glue.site(),
        3,
        "the imported end is nearest the left side of the ellipse"
    );
}

#[test]
fn preset_line_sp_imports_as_a_two_point_path() {
    let slide = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
    <p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
           xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
        <p:cSld>
            <p:spTree>
                <p:sp>
                    <p:nvSpPr><p:cNvPr id="2" name="Drawn line"/></p:nvSpPr>
                    <p:spPr>
                        <a:xfrm flipV="1"><a:off x="100000" y="100000"/><a:ext cx="1000000" cy="600000"/></a:xfrm>
                        <a:prstGeom prst="line"/>
                        <a:ln w="38100"><a:solidFill><a:srgbClr val="008000"/></a:solidFill></a:ln>
                    </p:spPr>
                </p:sp>
            </p:spTree>
        </p:cSld>
    </p:sld>"#;
    let bytes = fixture_pptx(&[slide], &[]);
    let res = import_pptx(&bytes, "Line").expect("import success");
    let object = &res.course.scenes[0].slides[0].objects[0];
    let coursecraft_course::ObjectKind::Path { path } = &object.kind else {
        panic!("expected a path");
    };
    assert_eq!(path.points.len(), 2);
    assert_eq!(path.connector, None);
    let first = path.points.first().copied().unwrap_or_default();
    let last = path.points.last().copied().unwrap_or_default();
    assert_eq!((first.x, first.y), (0.0, 1.0));
    assert_eq!((last.x, last.y), (1.0, 0.0));
    assert_eq!(path.outline.color.0, "#008000");
    assert!(res.course.is_playable(), "{:?}", res.course.validate());
}

#[test]
fn custom_geometry_imports_first_subpath_and_reports_the_rest() {
    let slide = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
    <p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
           xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
        <p:cSld>
            <p:spTree>
                <p:sp>
                    <p:nvSpPr><p:cNvPr id="2" name="Custom"/></p:nvSpPr>
                    <p:spPr>
                        <a:xfrm><a:off x="100000" y="100000"/><a:ext cx="1000000" cy="1000000"/></a:xfrm>
                        <a:custGeom>
                            <a:pathLst>
                                <a:path w="1000" h="1000">
                                    <a:moveTo><a:pt x="0" y="0"/></a:moveTo>
                                    <a:lnTo><a:pt x="1000" y="0"/></a:lnTo>
                                    <a:cubicBezTo>
                                        <a:pt x="1000" y="500"/>
                                        <a:pt x="500" y="1000"/>
                                        <a:pt x="0" y="1000"/>
                                    </a:cubicBezTo>
                                    <a:quadBezTo>
                                        <a:pt x="100" y="800"/>
                                        <a:pt x="200" y="600"/>
                                    </a:quadBezTo>
                                    <a:arcTo wR="100" hR="100" stAng="0" swAng="5400000"/>
                                    <a:close/>
                                </a:path>
                                <a:path w="1000" h="1000">
                                    <a:moveTo><a:pt x="0" y="0"/></a:moveTo>
                                    <a:lnTo><a:pt x="1000" y="1000"/></a:lnTo>
                                </a:path>
                            </a:pathLst>
                        </a:custGeom>
                        <a:ln w="19050"><a:solidFill><a:srgbClr val="2F6FEB"/></a:solidFill></a:ln>
                    </p:spPr>
                </p:sp>
            </p:spTree>
        </p:cSld>
    </p:sld>"#;
    let bytes = fixture_pptx(&[slide], &[]);
    let res = import_pptx(&bytes, "Custom").expect("import success");
    let object = &res.course.scenes[0].slides[0].objects[0];
    let coursecraft_course::ObjectKind::Path { path } = &object.kind else {
        panic!("expected a path");
    };
    assert!(path.closed);
    assert!(path.points.len() > 3);
    assert!(
        path.points
            .iter()
            .all(coursecraft_course::PathPoint::is_valid)
    );
    assert_eq!(path.outline.color.0, "#2F6FEB");
    assert!(res.report.items.iter().any(
        |item| item.kind == SkipKind::CustomGeometry && item.note.contains("several subpaths")
    ));
    assert!(res.course.is_playable(), "{:?}", res.course.validate());
}

#[test]
fn test_negative_size_still_skipped() {
    let slide = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
    <p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
           xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
        <p:cSld><p:spTree><p:sp>
            <p:nvSpPr><p:cNvPr id="2" name="Bad"/></p:nvSpPr>
            <p:spPr>
                <a:xfrm><a:off x="0" y="0"/><a:ext cx="-1000" cy="1000"/></a:xfrm>
                <a:prstGeom prst="rect"/>
            </p:spPr>
        </p:sp></p:spTree></p:cSld>
    </p:sld>"#;
    let bytes = fixture_pptx(&[slide], &[]);
    let res = import_pptx(&bytes, "Negative size").expect("import success");
    assert!(res.course.scenes[0].slides[0].objects.is_empty());
}

#[test]
fn test_empty_line_element_draws_no_outline() {
    let slide = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
    <p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
           xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
        <p:cSld><p:spTree><p:sp>
            <p:nvSpPr><p:cNvPr id="2" name="Text"/></p:nvSpPr>
            <p:spPr>
                <a:xfrm><a:off x="100" y="100"/><a:ext cx="1000" cy="1000"/></a:xfrm>
                <a:prstGeom prst="rect"/>
                <a:noFill/>
                <a:ln></a:ln>
            </p:spPr>
            <p:txBody><a:bodyPr/><a:p><a:r><a:t>Hello</a:t></a:r></a:p></p:txBody>
        </p:sp></p:spTree></p:cSld>
    </p:sld>"#;
    let bytes = fixture_pptx(&[slide], &[]);
    let res = import_pptx(&bytes, "Empty line").expect("import success");
    match &res.course.scenes[0].slides[0].objects[0].kind {
        coursecraft_course::ObjectKind::Shape { outline, .. } => assert!(outline.is_none()),
        _ => panic!("expected shape"),
    }
}

#[test]
fn test_absolute_relationship_targets_resolve_from_package_root() {
    assert_eq!(
        crate::resolve_target("ppt", "/ppt/slides/slide1.xml").as_deref(),
        Some("ppt/slides/slide1.xml")
    );
    assert_eq!(
        crate::resolve_target("ppt/slides", "../media/a.png").as_deref(),
        Some("ppt/media/a.png")
    );
    assert_eq!(crate::resolve_target("ppt", "/../evil.xml"), None);
    assert_eq!(crate::resolve_target("ppt", "../../evil.xml"), None);
}

#[test]
fn test_fill_alpha_is_kept() {
    let slide = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
    <p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
           xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
        <p:cSld><p:spTree><p:sp>
            <p:nvSpPr><p:cNvPr id="2" name="Clear"/></p:nvSpPr>
            <p:spPr>
                <a:xfrm><a:off x="100" y="100"/><a:ext cx="1000" cy="1000"/></a:xfrm>
                <a:prstGeom prst="rect"/>
                <a:solidFill><a:srgbClr val="000000"><a:alpha val="0"/></a:srgbClr></a:solidFill>
            </p:spPr>
        </p:sp><p:sp>
            <p:nvSpPr><p:cNvPr id="3" name="Half"/></p:nvSpPr>
            <p:spPr>
                <a:xfrm><a:off x="100" y="100"/><a:ext cx="1000" cy="1000"/></a:xfrm>
                <a:prstGeom prst="rect"/>
                <a:solidFill><a:srgbClr val="FF0000"><a:alpha val="50000"/></a:srgbClr></a:solidFill>
            </p:spPr>
        </p:sp></p:spTree></p:cSld>
    </p:sld>"#;
    let bytes = fixture_pptx(&[slide], &[]);
    let res = import_pptx(&bytes, "Alpha").expect("import success");
    let fills: Vec<String> = res.course.scenes[0].slides[0]
        .objects
        .iter()
        .filter_map(|object| match &object.kind {
            coursecraft_course::ObjectKind::Shape { fill, .. } => Some(fill.0.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(fills, ["#00000000", "#FF000080"]);
    assert!(res.course.is_playable(), "{:?}", res.course.validate());
}

#[test]
fn rotation_and_flips_import_on_shapes_and_group_children() {
    let slide = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
    <p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
           xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
        <p:cSld><p:spTree>
            <p:sp>
                <p:nvSpPr><p:cNvPr id="2" name="Rotated"/></p:nvSpPr>
                <p:spPr>
                    <a:xfrm rot="2700000"><a:off x="1000000" y="500000"/><a:ext cx="1000000" cy="500000"/></a:xfrm>
                    <a:prstGeom prst="rect"/>
                    <a:solidFill><a:srgbClr val="FF0000"/></a:solidFill>
                </p:spPr>
            </p:sp>
            <p:sp>
                <p:nvSpPr><p:cNvPr id="3" name="Flipped"/></p:nvSpPr>
                <p:spPr>
                    <a:xfrm flipH="1"><a:off x="3000000" y="500000"/><a:ext cx="1000000" cy="500000"/></a:xfrm>
                    <a:prstGeom prst="rect"/>
                    <a:solidFill><a:srgbClr val="00FF00"/></a:solidFill>
                </p:spPr>
            </p:sp>
            <p:grpSp>
                <p:nvGrpSpPr><p:cNvPr id="4" name="Group"/></p:nvGrpSpPr>
                <p:grpSpPr>
                    <a:xfrm rot="5400000"><a:off x="1000000" y="1000000"/><a:ext cx="2000000" cy="2000000"/>
                        <a:chOff x="1000000" y="1000000"/><a:chExt cx="2000000" cy="2000000"/></a:xfrm>
                </p:grpSpPr>
                <p:sp>
                    <p:nvSpPr><p:cNvPr id="5" name="GChild"/></p:nvSpPr>
                    <p:spPr>
                        <a:xfrm><a:off x="1000000" y="1500000"/><a:ext cx="500000" cy="500000"/></a:xfrm>
                        <a:prstGeom prst="rect"/>
                        <a:solidFill><a:srgbClr val="0000FF"/></a:solidFill>
                    </p:spPr>
                </p:sp>
            </p:grpSp>
        </p:spTree></p:cSld>
    </p:sld>"#;
    let bytes = fixture_pptx(&[slide], &[]);
    let res = import_pptx(&bytes, "Rotation").expect("import success");
    let objects = &res.course.scenes[0].slides[0].objects;

    let rotated = objects
        .iter()
        .find(|object| object.name == "Rotated")
        .expect("rotated shape");
    assert!(
        (rotated.rotation - 45.0).abs() < 0.01,
        "{}",
        rotated.rotation
    );

    let flipped = objects
        .iter()
        .find(|object| object.name == "Flipped")
        .expect("flipped shape");
    assert!(flipped.flip_h);
    assert!(!flipped.flip_v);

    let child = objects
        .iter()
        .find(|object| object.name == "GChild")
        .expect("group child");
    assert!((child.rotation - 90.0).abs() < 0.01, "{}", child.rotation);
    let centre = (
        child.frame.x + child.frame.w * 0.5,
        child.frame.y + child.frame.h * 0.5,
    );
    assert!(
        (centre.0 - 315.0).abs() < 2.0 && (centre.1 - 175.0).abs() < 2.0,
        "rotated group child centre {centre:?}"
    );
    assert!(
        !res.report
            .items
            .iter()
            .any(|item| item.kind == SkipKind::RotationOrFlip),
        "rotation and flips are imported, not skipped"
    );
}

#[test]
fn gradient_picture_fill_shadow_and_glow_import() {
    let slide = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
    <p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
           xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
           xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
        <p:cSld>
            <p:spTree>
                <p:sp>
                    <p:nvSpPr><p:cNvPr id="2" name="Gradient"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr>
                    <p:spPr>
                        <a:xfrm><a:off x="500000" y="500000"/><a:ext cx="3000000" cy="2000000"/></a:xfrm>
                        <a:prstGeom prst="rect"><a:avLst/></a:prstGeom>
                        <a:gradFill>
                            <a:gsLst>
                                <a:gs pos="0"><a:srgbClr val="FF0000"/></a:gs>
                                <a:gs pos="100000"><a:srgbClr val="0000FF"><a:alpha val="50000"/></a:srgbClr></a:gs>
                            </a:gsLst>
                            <a:lin ang="5400000" scaled="1"/>
                        </a:gradFill>
                        <a:effectLst>
                            <a:outerShdw blurRad="127000" dist="254000" dir="2700000" algn="tl" rotWithShape="0">
                                <a:srgbClr val="000000"><a:alpha val="40000"/></a:srgbClr>
                            </a:outerShdw>
                            <a:glow rad="63500">
                                <a:srgbClr val="00FF00"><a:alpha val="60000"/></a:srgbClr>
                            </a:glow>
                        </a:effectLst>
                    </p:spPr>
                    <p:txBody><a:bodyPr/><a:lstStyle/><a:p/></p:txBody>
                </p:sp>
                <p:sp>
                    <p:nvSpPr><p:cNvPr id="3" name="Picture fill"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr>
                    <p:spPr>
                        <a:xfrm><a:off x="4000000" y="500000"/><a:ext cx="2000000" cy="2000000"/></a:xfrm>
                        <a:prstGeom prst="ellipse"><a:avLst/></a:prstGeom>
                        <a:blipFill>
                            <a:blip r:embed="rIdPic1"/>
                            <a:tile tx="0" ty="0" sx="100000" sy="100000"/>
                        </a:blipFill>
                    </p:spPr>
                    <p:txBody><a:bodyPr/><a:lstStyle/><a:p/></p:txBody>
                </p:sp>
            </p:spTree>
        </p:cSld>
    </p:sld>"#;

    let slide_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
    <Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
        <Relationship Id="rIdPic1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/image1.png"/>
    </Relationships>"#;
    let extra: &[(&str, &[u8])] = &[
        ("ppt/slides/_rels/slide1.xml.rels", slide_rels.as_bytes()),
        ("ppt/media/image1.png", TINY_PNG),
    ];

    let bytes = fixture_pptx(&[slide], extra);
    let res = import_pptx(&bytes, "Effects Deck").expect("import success");
    assert_eq!(res.media.len(), 1, "picture fill imports its image");
    assert!(res.course.is_playable(), "{:?}", res.course.validate());

    let objects = &res.course.scenes[0].slides[0].objects;
    let gradient = objects
        .iter()
        .find(|object| object.name == "Gradient")
        .expect("gradient shape");
    match &gradient.kind {
        coursecraft_course::ObjectKind::Shape {
            fill_style: Some(coursecraft_course::FillStyle::Linear { angle, stops }),
            ..
        } => {
            assert!((*angle - 90.0).abs() < 0.01, "angle {angle}");
            assert_eq!(stops.len(), 2);
            assert_eq!(stops[1].color.0, "#0000FF80");
        }
        other => panic!("expected a linear fill, got {other:?}"),
    }
    let shadow = gradient.shadow.as_ref().expect("outer shadow");
    assert!(shadow.offset_x > 0.0 && shadow.offset_y > 0.0, "{shadow:?}");
    assert!(shadow.blur > 0.0, "{shadow:?}");
    let glow = gradient.glow.as_ref().expect("glow");
    assert!(glow.size > 0.0, "{glow:?}");

    let pictured = objects
        .iter()
        .find(|object| object.name == "Picture fill")
        .expect("picture fill shape");
    match &pictured.kind {
        coursecraft_course::ObjectKind::Shape {
            fill_style: Some(coursecraft_course::FillStyle::Picture { src, mode }),
            ..
        } => {
            assert_eq!(src, "image1.png");
            assert_eq!(*mode, coursecraft_course::PictureFillMode::Tile);
        }
        other => panic!("expected a picture fill, got {other:?}"),
    }
}

#[test]
fn import_report_lists_deck_fonts_missing_from_the_course() {
    let slide = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
    <p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
           xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
        <p:cSld>
            <p:spTree>
                <p:sp>
                    <p:nvSpPr><p:cNvPr id="2" name="Title"/></p:nvSpPr>
                    <p:spPr>
                        <a:xfrm><a:off x="914400" y="914400"/><a:ext cx="5486400" cy="914400"/></a:xfrm>
                        <a:prstGeom prst="rect"><a:avLst/></a:prstGeom>
                    </p:spPr>
                    <p:txBody>
                        <a:bodyPr/>
                        <a:p><a:r><a:rPr><a:latin typeface="Lato"/></a:rPr><a:t>Hello</a:t></a:r></a:p>
                    </p:txBody>
                </p:sp>
            </p:spTree>
        </p:cSld>
    </p:sld>"#;
    let theme = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
    <a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
        <a:themeElements>
            <a:fontScheme name="Deck">
                <a:majorFont><a:latin typeface="Georgia"/><a:ea typeface=""/></a:majorFont>
                <a:minorFont><a:latin typeface="Lato"/><a:ea typeface=""/></a:minorFont>
            </a:fontScheme>
        </a:themeElements>
    </a:theme>"#;
    let bytes = fixture_pptx(&[slide], &[("ppt/theme/theme1.xml", theme.as_bytes())]);
    let imported =
        crate::import_pptx_with_limits(&bytes, "Fonts", Limits::default()).expect("import");

    assert!(
        imported.report.missing_fonts.iter().any(|f| f == "Lato"),
        "{:?}",
        imported.report.missing_fonts
    );
    assert!(
        imported.report.missing_fonts.iter().any(|f| f == "Georgia"),
        "{:?}",
        imported.report.missing_fonts
    );
    let summary = imported.report.summary_window_text();
    assert!(
        summary.contains("Add these fonts to keep the look:") && summary.contains("Lato"),
        "{summary}"
    );

    // Imported text keeps the deck font name so adding it later renders.
    let title = &imported.course.scenes[0].slides[0].objects[0];
    match &title.kind {
        coursecraft_course::ObjectKind::Text { runs, .. } => {
            assert_eq!(runs[0].font.as_deref(), Some("Lato"));
        }
        other => panic!("expected text, got {other:?}"),
    }
}
