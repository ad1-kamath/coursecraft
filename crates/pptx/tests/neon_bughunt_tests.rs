//! Bug-hunt proof tests for `coursecraft-pptx` (0.6.1 Neon review).
//!
//! Each test failed before its fix and now guards against a regression.

use std::io::Write as _;

use coursecraft_pptx::{Limits, import_pptx, import_pptx_with_limits};

fn build_pptx(slides: &[&str], extra: &[(&str, &[u8])]) -> Vec<u8> {
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);

        let mut sld_ids = String::new();
        let mut sld_rels = String::new();
        for (i, _) in slides.iter().enumerate() {
            let rid = format!("rId{}", i + 1);
            let id = 256 + i;
            let n = i + 1;
            sld_ids.push_str(&format!(r#"<p:sldId id="{id}" r:id="{rid}"/>"#));
            sld_rels.push_str(&format!(
                r#"<Relationship Id="{rid}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide{n}.xml"/>"#
            ));
        }

        let pres = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
            <p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
                            xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
                            xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
                <p:sldSz cx="9144000" cy="5143500"/>
                <p:sldIdLst>{sld_ids}</p:sldIdLst>
            </p:presentation>"#
        );
        zip.start_file("ppt/presentation.xml", opts).expect("zip");
        zip.write_all(pres.as_bytes()).expect("write");

        let rels = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
            <Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
                {sld_rels}
            </Relationships>"#
        );
        zip.start_file("ppt/_rels/presentation.xml.rels", opts)
            .expect("zip");
        zip.write_all(rels.as_bytes()).expect("write");

        for (i, slide) in slides.iter().enumerate() {
            zip.start_file(format!("ppt/slides/slide{}.xml", i + 1), opts)
                .expect("zip");
            zip.write_all(slide.as_bytes()).expect("write");
        }
        for (path, bytes) in extra {
            zip.start_file(*path, opts).expect("zip");
            zip.write_all(bytes).expect("write");
        }
        zip.finish().expect("finish");
    }
    buf.into_inner()
}

/// `parse_hex_rgb` checks `hex.len() == 6` (byte length) and slices `&hex[0..2]`,
/// panicking on any 6-byte non-ASCII UTF-8 string such as `"中中"`.
#[test]
fn pptx_non_ascii_6_byte_hex_color_panics() {
    let slide = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
    <p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
           xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
      <p:cSld>
        <p:spTree>
          <p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>
          <p:grpSpPr/>
          <p:sp>
            <p:nvSpPr><p:cNvPr id="2" name="Rect"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr>
            <p:spPr>
              <a:xfrm><a:off x="0" y="0"/><a:ext cx="914400" cy="914400"/></a:xfrm>
              <a:prstGeom prst="rect"><a:avLst/></a:prstGeom>
              <a:solidFill><a:srgbClr val="中中"/></a:solidFill>
            </p:spPr>
          </p:sp>
        </p:spTree>
      </p:cSld>
    </p:sld>"#;
    let bytes = build_pptx(&[slide], &[]);

    // Hostile input rule: never crash/panic.
    let res = import_pptx(&bytes, "Bad Color");
    assert!(res.is_ok() || res.is_err());
}

/// `tokenize_motion_path` splits on `'e'`/`'E'` even inside scientific notation
/// numbers like `1.5e-5`, prematurely ending the path and dropping remaining segments.
#[test]
fn pptx_motion_path_scientific_notation_truncated() {
    let slide = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
    <p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
           xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
      <p:cSld>
        <p:spTree>
          <p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>
          <p:grpSpPr/>
          <p:sp>
            <p:nvSpPr><p:cNvPr id="2" name="Box"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr>
            <p:spPr>
              <a:xfrm><a:off x="0" y="0"/><a:ext cx="914400" cy="914400"/></a:xfrm>
              <a:prstGeom prst="rect"><a:avLst/></a:prstGeom>
              <a:solidFill><a:srgbClr val="2266CC"/></a:solidFill>
            </p:spPr>
          </p:sp>
        </p:spTree>
      </p:cSld>
      <p:timing>
        <p:tnLst>
          <p:par>
            <p:cTn id="1" dur="indefinite" nodeType="tmRoot">
              <p:childTnLst>
                <p:seq concurrent="1" nextAc="seek">
                  <p:cTn id="2" dur="indefinite" nodeType="mainSeq">
                    <p:childTnLst>
                      <p:par>
                        <p:cTn id="3" fill="hold">
                          <p:stCondLst><p:cond delay="0"/></p:stCondLst>
                          <p:childTnLst>
                            <p:par>
                              <p:cTn id="4" presetID="0" presetClass="path" presetSubtype="0" fill="hold">
                                <p:stCondLst><p:cond delay="0"/></p:stCondLst>
                                <p:childTnLst>
                                  <p:animMotion origin="layout" path="M 0 0 L 1.5e-5 0.5 E" pathEditMode="relative">
                                    <p:cBhvr>
                                      <p:cTn id="5" dur="1000" fill="hold"/>
                                      <p:tgtEl><p:spTgt spid="2"/></p:tgtEl>
                                    </p:cBhvr>
                                  </p:animMotion>
                                </p:childTnLst>
                              </p:cTn>
                            </p:par>
                          </p:childTnLst>
                        </p:cTn>
                      </p:par>
                    </p:childTnLst>
                  </p:cTn>
                </p:seq>
              </p:childTnLst>
            </p:cTn>
          </p:par>
        </p:tnLst>
      </p:timing>
    </p:sld>"#;
    let bytes = build_pptx(&[slide], &[]);
    let imported = import_pptx(&bytes, "Sci Path").expect("import_pptx");
    let obj = &imported.course.scenes[0].slides[0].objects[0];
    assert_eq!(
        obj.motion_paths.len(),
        1,
        "motion path with scientific-notation coordinate 1.5e-5 should be parsed instead of truncated"
    );
}

/// N52: shape ids are numbered per XML part, so a master shape with id 2 must
/// not receive the animation that targets id 2 on the slide.
#[test]
fn pptx_animation_target_id_does_not_leak_to_master_shapes() {
    let shape = |name: &str| {
        format!(
            r#"<p:sp>
            <p:nvSpPr><p:cNvPr id="2" name="{name}"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr>
            <p:spPr>
              <a:xfrm><a:off x="0" y="0"/><a:ext cx="914400" cy="914400"/></a:xfrm>
              <a:prstGeom prst="rect"><a:avLst/></a:prstGeom>
              <a:solidFill><a:srgbClr val="2266CC"/></a:solidFill>
            </p:spPr>
          </p:sp>"#
        )
    };
    let tree = |name: &str| {
        format!(
            r#"<p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>{}"#,
            shape(name)
        )
    };
    let master = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
        <p:sldMaster xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
           xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
          <p:cSld><p:spTree>{}</p:spTree></p:cSld>
        </p:sldMaster>"#,
        tree("MasterBox")
    );
    let slide = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
    <p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
           xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
      <p:cSld><p:spTree>{}</p:spTree></p:cSld>
      <p:timing><p:tnLst><p:par><p:cTn id="1" dur="indefinite" nodeType="tmRoot"><p:childTnLst>
        <p:seq concurrent="1" nextAc="seek"><p:cTn id="2" dur="indefinite" nodeType="mainSeq"><p:childTnLst>
          <p:par><p:cTn id="3" fill="hold"><p:stCondLst><p:cond delay="0"/></p:stCondLst><p:childTnLst>
            <p:par><p:cTn id="4" presetID="0" presetClass="path" presetSubtype="0" fill="hold">
              <p:stCondLst><p:cond delay="0"/></p:stCondLst><p:childTnLst>
                <p:animMotion origin="layout" path="M 0 0 L 0.2 0.5 E" pathEditMode="relative">
                  <p:cBhvr><p:cTn id="5" dur="1000" fill="hold"/><p:tgtEl><p:spTgt spid="2"/></p:tgtEl></p:cBhvr>
                </p:animMotion>
              </p:childTnLst></p:cTn></p:par>
          </p:childTnLst></p:cTn></p:par>
        </p:childTnLst></p:cTn></p:seq>
      </p:childTnLst></p:cTn></p:par></p:tnLst></p:timing>
    </p:sld>"#,
        tree("SlideBox")
    );
    let bytes = build_pptx(
        &[&slide],
        &[("ppt/slideMasters/slideMaster1.xml", master.as_bytes())],
    );
    let imported = import_pptx(&bytes, "Ids").expect("import_pptx");
    let objects = &imported.course.scenes[0].slides[0].objects;
    let animated: Vec<&str> = objects
        .iter()
        .filter(|o| !o.motion_paths.is_empty())
        .map(|o| o.name.as_str())
        .collect();
    assert_eq!(objects.len(), 2, "master and slide shape are both imported");
    assert_eq!(animated, vec!["SlideBox"], "only the slide shape animates");
}

/// Custom geometry `<a:pathLst>` with many `<a:path>` elements inside one
/// `<p:sp>` bypasses `Limits::max_shapes_per_slide`.
#[test]
fn pptx_cust_geom_bypasses_max_shapes_per_slide() {
    let mut paths_xml = String::new();
    for _ in 0..20 {
        paths_xml.push_str(
            r#"<a:path w="100" h="100"><a:moveTo><a:pt x="0" y="0"/></a:moveTo><a:lnTo><a:pt x="100" y="100"/></a:lnTo></a:path>"#,
        );
    }
    let slide = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
        <p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
               xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
          <p:cSld>
            <p:spTree>
              <p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>
              <p:grpSpPr/>
              <p:sp>
                <p:nvSpPr><p:cNvPr id="2" name="MultiPath"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr>
                <p:spPr>
                  <a:xfrm><a:off x="0" y="0"/><a:ext cx="914400" cy="914400"/></a:xfrm>
                  <a:custGeom><a:pathLst>{paths_xml}</a:pathLst></a:custGeom>
                  <a:ln w="12700"><a:solidFill><a:srgbClr val="000000"/></a:solidFill></a:ln>
                </p:spPr>
              </p:sp>
            </p:spTree>
          </p:cSld>
        </p:sld>"#
    );
    let bytes = build_pptx(&[&slide], &[]);
    let limits = Limits {
        max_shapes_per_slide: 5,
        ..Limits::default()
    };
    let imported = import_pptx_with_limits(&bytes, "Cap", limits).expect("import");
    let count = imported.course.scenes[0].slides[0].objects.len();
    assert!(
        count <= 5,
        "expected at most max_shapes_per_slide (5) objects, but got {count}"
    );
}

/// N80: a table with thousands of columns and rows is skipped with a report
/// note, quickly, and never builds millions of cells.
#[test]
fn pptx_huge_table_is_skipped_not_allocated() {
    let cols = "<a:gridCol w=\"100\"/>".repeat(5000);
    let rows = "<a:tr h=\"100\"/>".repeat(5000);
    let slide = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
        <p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
               xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
          <p:cSld><p:spTree>
            <p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>
            <p:grpSpPr/>
            <p:graphicFrame>
              <p:nvGraphicFramePr><p:cNvPr id="2" name="T"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr>
              <p:xfrm><a:off x="0" y="0"/><a:ext cx="914400" cy="914400"/></p:xfrm>
              <a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/table">
                <a:tbl><a:tblPr/><a:tblGrid>{cols}</a:tblGrid>{rows}</a:tbl>
              </a:graphicData></a:graphic>
            </p:graphicFrame>
          </p:spTree></p:cSld>
        </p:sld>"#
    );
    let bytes = build_pptx(&[&slide], &[]);
    let started = std::time::Instant::now();
    let imported = import_pptx(&bytes, "Huge table").expect("import");
    assert!(started.elapsed() < std::time::Duration::from_secs(10));
    assert!(
        imported.course.scenes[0].slides[0].objects.is_empty(),
        "table must be skipped"
    );
    assert!(
        imported
            .report
            .items
            .iter()
            .any(|i| i.note.contains("too large")),
        "{:?}",
        imported.report.items
    );
}
