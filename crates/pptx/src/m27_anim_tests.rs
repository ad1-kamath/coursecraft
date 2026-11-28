//! M27 Step C-4 tests: PowerPoint animation, motion-path, and timing import (C1.1–C1.13).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::Write as _;

use coursecraft_course::{
    Action, AnimEffect, Animation, Direction, Easing, Event, Object, Point, Slide,
};
use coursecraft_logic::{Input, Player};

use crate::timing::apply_slide_timing;
use crate::{
    ImportReport, Limits, PptxError, SkipKind, SlideLayout, SlideTracker, import_pptx,
    import_pptx_with_limits, parse_xml_tree,
};

const STD_16_9_CX: u64 = 12_192_000;
const STD_16_9_CY: u64 = 6_858_000;

fn build_pptx_with_slide(sp_tree_inner: &str, timing_xml: &str) -> Vec<u8> {
    let slide_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
       xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
       xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
  <p:cSld>
    <p:spTree>
      <p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>
      <p:grpSpPr/>
      {sp_tree_inner}
    </p:spTree>
  </p:cSld>
  {timing_xml}
</p:sld>"#
    );

    let mut buffer = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buffer);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);

        let pres_xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
  <p:sldIdLst><p:sldId id="256" r:id="rId1"/></p:sldIdLst>
  <p:sldSz cx="{STD_16_9_CX}" cy="{STD_16_9_CY}"/>
</p:presentation>"#
        );
        zip.start_file("ppt/presentation.xml", options).unwrap();
        zip.write_all(pres_xml.as_bytes()).unwrap();

        let pres_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide1.xml"/>
</Relationships>"#;
        zip.start_file("ppt/_rels/presentation.xml.rels", options)
            .unwrap();
        zip.write_all(pres_rels.as_bytes()).unwrap();

        zip.start_file("ppt/slides/slide1.xml", options).unwrap();
        zip.write_all(slide_xml.as_bytes()).unwrap();

        zip.finish().unwrap();
    }
    buffer.into_inner()
}

fn rect_shape_xml(spid: u32, name: &str, x_emu: u64, y_emu: u64) -> String {
    format!(
        r#"<p:sp>
  <p:nvSpPr><p:cNvPr id="{spid}" name="{name}"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr>
  <p:spPr>
    <a:xfrm><a:off x="{x_emu}" y="{y_emu}"/><a:ext cx="914400" cy="914400"/></a:xfrm>
    <a:prstGeom prst="rect"/>
    <a:solidFill><a:srgbClr val="336699"/></a:solidFill>
  </p:spPr>
</p:sp>"#
    )
}

fn import_and_validate(
    sp_tree_inner: &str,
    timing_xml: &str,
) -> (coursecraft_course::Course, ImportReport) {
    let bytes = build_pptx_with_slide(sp_tree_inner, timing_xml);
    let imported = import_pptx(&bytes, "Anim Test").expect("import_pptx");
    let issues = imported.course.validate();
    let errors: Vec<_> = issues
        .iter()
        .filter(|i| i.severity == coursecraft_course::Severity::Error)
        .collect();
    assert!(errors.is_empty(), "validation errors: {errors:?}");
    (imported.course, imported.report)
}

fn obj_by_name<'a>(slide: &'a Slide, name: &str) -> &'a Object {
    slide
        .objects
        .iter()
        .find(|o| o.name == name)
        .unwrap_or_else(|| panic!("missing object named {name:?}"))
}

fn anim_notes(report: &ImportReport) -> Vec<&str> {
    report
        .items
        .iter()
        .filter(|i| i.kind == SkipKind::Animation)
        .map(|i| i.note.as_str())
        .collect()
}

#[test]
fn c1_1_all_five_mapped_entrance_effects_and_directions() {
    let shapes: String = (2..=12)
        .map(|id| rect_shape_xml(id, &format!("Shape {id}"), u64::from(id) * 500_000, 914_400))
        .collect();

    let timing = r#"
    <p:timing>
      <p:tnLst>
        <p:par><p:cTn id="1" nodeType="tmRoot"><p:childTnLst>
          <p:seq><p:cTn id="2" nodeType="mainSeq"><p:childTnLst>
            <p:par><p:cTn id="3"><p:childTnLst>
              <p:par><p:cTn id="10" presetID="10" presetClass="entr" presetSubtype="0" nodeType="withEffect" dur="500"><p:childTnLst><p:set><p:cBhvr><p:tgtEl><p:spTgt spid="2"/></p:tgtEl></p:cBhvr></p:set></p:childTnLst></p:cTn></p:par>
              <p:par><p:cTn id="11" presetID="2" presetClass="entr" presetSubtype="1" nodeType="withEffect" dur="400"><p:childTnLst><p:set><p:cBhvr><p:tgtEl><p:spTgt spid="3"/></p:tgtEl></p:cBhvr></p:set></p:childTnLst></p:cTn></p:par>
              <p:par><p:cTn id="12" presetID="2" presetClass="entr" presetSubtype="2" nodeType="withEffect" dur="400"><p:childTnLst><p:set><p:cBhvr><p:tgtEl><p:spTgt spid="4"/></p:tgtEl></p:cBhvr></p:set></p:childTnLst></p:cTn></p:par>
              <p:par><p:cTn id="13" presetID="2" presetClass="entr" presetSubtype="4" nodeType="withEffect" dur="400"><p:childTnLst><p:set><p:cBhvr><p:tgtEl><p:spTgt spid="5"/></p:tgtEl></p:cBhvr></p:set></p:childTnLst></p:cTn></p:par>
              <p:par><p:cTn id="14" presetID="2" presetClass="entr" presetSubtype="8" nodeType="withEffect" dur="400"><p:childTnLst><p:set><p:cBhvr><p:tgtEl><p:spTgt spid="6"/></p:tgtEl></p:cBhvr></p:set></p:childTnLst></p:cTn></p:par>
              <p:par><p:cTn id="15" presetID="22" presetClass="entr" presetSubtype="1" nodeType="withEffect" dur="600"><p:childTnLst><p:set><p:cBhvr><p:tgtEl><p:spTgt spid="7"/></p:tgtEl></p:cBhvr></p:set></p:childTnLst></p:cTn></p:par>
              <p:par><p:cTn id="16" presetID="22" presetClass="entr" presetSubtype="2" nodeType="withEffect" dur="600"><p:childTnLst><p:set><p:cBhvr><p:tgtEl><p:spTgt spid="8"/></p:tgtEl></p:cBhvr></p:set></p:childTnLst></p:cTn></p:par>
              <p:par><p:cTn id="17" presetID="22" presetClass="entr" presetSubtype="4" nodeType="withEffect" dur="600"><p:childTnLst><p:set><p:cBhvr><p:tgtEl><p:spTgt spid="9"/></p:tgtEl></p:cBhvr></p:set></p:childTnLst></p:cTn></p:par>
              <p:par><p:cTn id="18" presetID="22" presetClass="entr" presetSubtype="8" nodeType="withEffect" dur="600"><p:childTnLst><p:set><p:cBhvr><p:tgtEl><p:spTgt spid="10"/></p:tgtEl></p:cBhvr></p:set></p:childTnLst></p:cTn></p:par>
              <p:par><p:cTn id="19" presetID="23" presetClass="entr" presetSubtype="0" nodeType="withEffect" dur="700"><p:childTnLst><p:set><p:cBhvr><p:tgtEl><p:spTgt spid="11"/></p:tgtEl></p:cBhvr></p:set></p:childTnLst></p:cTn></p:par>
              <p:par><p:cTn id="20" presetID="42" presetClass="entr" presetSubtype="4" nodeType="withEffect" dur="550"><p:childTnLst><p:set><p:cBhvr><p:tgtEl><p:spTgt spid="12"/></p:tgtEl></p:cBhvr></p:set></p:childTnLst></p:cTn></p:par>
            </p:childTnLst></p:cTn></p:par>
          </p:childTnLst></p:cTn></p:seq>
        </p:childTnLst></p:cTn></p:par>
      </p:tnLst>
    </p:timing>"#;

    let (course, report) = import_and_validate(&shapes, timing);
    assert!(
        anim_notes(&report).is_empty(),
        "unexpected animation skip notes: {:?}",
        anim_notes(&report)
    );

    let slide = &course.scenes[0].slides[0];
    let expected: [(&str, AnimEffect, u32); 11] = [
        ("Shape 2", AnimEffect::new("fade"), 500),
        (
            "Shape 3",
            AnimEffect::with_from("fly_in", Direction::Top),
            400,
        ),
        (
            "Shape 4",
            AnimEffect::with_from("fly_in", Direction::Right),
            400,
        ),
        (
            "Shape 5",
            AnimEffect::with_from("fly_in", Direction::Bottom),
            400,
        ),
        (
            "Shape 6",
            AnimEffect::with_from("fly_in", Direction::Left),
            400,
        ),
        (
            "Shape 7",
            AnimEffect::with_from("wipe", Direction::Top),
            600,
        ),
        (
            "Shape 8",
            AnimEffect::with_from("wipe", Direction::Right),
            600,
        ),
        (
            "Shape 9",
            AnimEffect::with_from("wipe", Direction::Bottom),
            600,
        ),
        (
            "Shape 10",
            AnimEffect::with_from("wipe", Direction::Left),
            600,
        ),
        ("Shape 11", AnimEffect::new("zoom"), 700),
        (
            "Shape 12",
            AnimEffect::with_from("float", Direction::Bottom),
            550,
        ),
    ];

    for (name, effect, dur) in expected {
        let obj = obj_by_name(slide, name);
        assert_eq!(
            obj.entrance,
            Some(Animation {
                effect,
                duration_ms: dur,
                easing: Easing::EaseOut,
                sequence: Default::default(),
            }),
            "mismatch on {name}"
        );
    }
}

#[test]
fn c1_2_appear_and_disappear_visibility_with_player() {
    let shapes = rect_shape_xml(2, "ToggleBox", 914_400, 914_400);
    let timing = r#"
    <p:timing>
      <p:tnLst>
        <p:par><p:cTn id="1" nodeType="tmRoot"><p:childTnLst>
          <p:seq><p:cTn id="2" nodeType="mainSeq"><p:childTnLst>
            <p:par><p:cTn id="3"><p:childTnLst>
              <p:par><p:cTn id="4" presetID="1" presetClass="entr" nodeType="withEffect" dur="1">
                <p:stCondLst><p:cond delay="1000"/></p:stCondLst>
                <p:childTnLst><p:set><p:cBhvr><p:cTn id="5" dur="1"/><p:tgtEl><p:spTgt spid="2"/></p:tgtEl></p:cBhvr></p:set></p:childTnLst>
              </p:cTn></p:par>
              <p:par><p:cTn id="6" presetID="1" presetClass="exit" nodeType="withEffect" dur="1">
                <p:stCondLst><p:cond delay="3000"/></p:stCondLst>
                <p:childTnLst><p:set><p:cBhvr><p:cTn id="7" dur="1"/><p:tgtEl><p:spTgt spid="2"/></p:tgtEl></p:cBhvr></p:set></p:childTnLst>
              </p:cTn></p:par>
            </p:childTnLst></p:cTn></p:par>
          </p:childTnLst></p:cTn></p:seq>
        </p:childTnLst></p:cTn></p:par>
      </p:tnLst>
    </p:timing>"#;

    let (course, _report) = import_and_validate(&shapes, timing);
    let slide = &course.scenes[0].slides[0];
    let obj = obj_by_name(slide, "ToggleBox");
    assert_eq!(obj.timing.start_ms, 1000);
    assert_eq!(obj.entrance, None);
    assert_eq!(obj.timing.end_ms, Some(3000));
    assert_eq!(obj.exit, None);

    let mut player = Player::new(&course).expect("valid player");
    player.handle(&Input::Start);
    assert!(
        player.sample_object(&obj.id).is_none(),
        "hidden before 1000ms"
    );

    player.handle(&Input::Tick { ms: 900 });
    assert!(
        player.sample_object(&obj.id).is_none(),
        "still hidden at 900ms"
    );

    player.handle(&Input::Tick { ms: 200 });
    assert!(player.sample_object(&obj.id).is_some(), "visible at 1100ms");

    player.handle(&Input::Tick { ms: 1900 });
    assert!(player.sample_object(&obj.id).is_none(), "hidden at 3000ms");
}

#[test]
fn c1_3_all_five_mapped_exit_effects_and_player_sampling() {
    let shapes: String = (2..=6)
        .map(|id| rect_shape_xml(id, &format!("Exit {id}"), u64::from(id) * 914_400, 914_400))
        .collect();

    let timing = r#"
    <p:timing>
      <p:tnLst>
        <p:par><p:cTn id="1" nodeType="tmRoot"><p:childTnLst>
          <p:seq><p:cTn id="2" nodeType="mainSeq"><p:childTnLst>
            <p:par><p:cTn id="3"><p:childTnLst>
              <p:par><p:cTn id="10" presetID="10" presetClass="exit" presetSubtype="0" nodeType="withEffect" dur="500"><p:stCondLst><p:cond delay="1000"/></p:stCondLst><p:childTnLst><p:set><p:cBhvr><p:tgtEl><p:spTgt spid="2"/></p:tgtEl></p:cBhvr></p:set></p:childTnLst></p:cTn></p:par>
              <p:par><p:cTn id="11" presetID="2" presetClass="exit" presetSubtype="2" nodeType="withEffect" dur="400"><p:stCondLst><p:cond delay="1000"/></p:stCondLst><p:childTnLst><p:set><p:cBhvr><p:tgtEl><p:spTgt spid="3"/></p:tgtEl></p:cBhvr></p:set></p:childTnLst></p:cTn></p:par>
              <p:par><p:cTn id="12" presetID="22" presetClass="exit" presetSubtype="8" nodeType="withEffect" dur="600"><p:stCondLst><p:cond delay="1000"/></p:stCondLst><p:childTnLst><p:set><p:cBhvr><p:tgtEl><p:spTgt spid="4"/></p:tgtEl></p:cBhvr></p:set></p:childTnLst></p:cTn></p:par>
              <p:par><p:cTn id="13" presetID="23" presetClass="exit" presetSubtype="0" nodeType="withEffect" dur="500"><p:stCondLst><p:cond delay="1000"/></p:stCondLst><p:childTnLst><p:set><p:cBhvr><p:tgtEl><p:spTgt spid="5"/></p:tgtEl></p:cBhvr></p:set></p:childTnLst></p:cTn></p:par>
              <p:par><p:cTn id="14" presetID="47" presetClass="exit" presetSubtype="0" nodeType="withEffect" dur="500"><p:stCondLst><p:cond delay="1000"/></p:stCondLst><p:childTnLst><p:set><p:cBhvr><p:tgtEl><p:spTgt spid="6"/></p:tgtEl></p:cBhvr></p:set></p:childTnLst></p:cTn></p:par>
            </p:childTnLst></p:cTn></p:par>
          </p:childTnLst></p:cTn></p:seq>
        </p:childTnLst></p:cTn></p:par>
      </p:tnLst>
    </p:timing>"#;

    let (course, _report) = import_and_validate(&shapes, timing);
    let slide = &course.scenes[0].slides[0];
    let expected: [(&str, AnimEffect, u32); 5] = [
        ("Exit 2", AnimEffect::new("fade"), 500),
        (
            "Exit 3",
            AnimEffect::with_from("fly_in", Direction::Right),
            400,
        ),
        (
            "Exit 4",
            AnimEffect::with_from("wipe", Direction::Left),
            600,
        ),
        ("Exit 5", AnimEffect::new("zoom"), 500),
        (
            "Exit 6",
            AnimEffect::with_from("float", Direction::Top),
            500,
        ),
    ];

    for (name, effect, dur) in expected {
        let obj = obj_by_name(slide, name);
        assert_eq!(obj.timing.end_ms, Some(1000 + dur));
        assert_eq!(
            obj.exit,
            Some(Animation {
                effect,
                duration_ms: dur,
                easing: Easing::EaseOut,
                sequence: Default::default(),
            })
        );
        assert!(slide.duration_ms >= 1000 + dur);
    }

    let mut player = Player::new(&course).expect("valid player");
    player.handle(&Input::Start);
    let fade_obj = obj_by_name(slide, "Exit 2");
    player.handle(&Input::Tick { ms: 1250 });
    let mid = player
        .sample_object(&fade_obj.id)
        .expect("mid-exit visible");
    assert!(
        mid.opacity > 0.0 && mid.opacity < 1.0,
        "opacity={}",
        mid.opacity
    );

    player.handle(&Input::Tick { ms: 300 });
    assert!(
        player.sample_object(&fade_obj.id).is_none(),
        "hidden after end_ms"
    );
}

#[test]
fn c1_4_with_after_click_scheduling_and_delays() {
    let shapes: String = (2..=5)
        .map(|id| rect_shape_xml(id, &format!("Step {id}"), u64::from(id) * 914_400, 914_400))
        .collect();

    let timing = r#"
    <p:timing>
      <p:tnLst>
        <p:par><p:cTn id="1" nodeType="tmRoot"><p:childTnLst>
          <p:seq><p:cTn id="2" nodeType="mainSeq"><p:childTnLst>
            <p:par><p:cTn id="3"><p:childTnLst>
              <p:par><p:cTn id="10" presetID="10" presetClass="entr" nodeType="withEffect" dur="500"><p:stCondLst><p:cond delay="0"/></p:stCondLst><p:childTnLst><p:set><p:cBhvr><p:tgtEl><p:spTgt spid="2"/></p:tgtEl></p:cBhvr></p:set></p:childTnLst></p:cTn></p:par>
              <p:par><p:cTn id="11" presetID="10" presetClass="entr" nodeType="withEffect" dur="400"><p:stCondLst><p:cond delay="200"/></p:stCondLst><p:childTnLst><p:set><p:cBhvr><p:tgtEl><p:spTgt spid="3"/></p:tgtEl></p:cBhvr></p:set></p:childTnLst></p:cTn></p:par>
              <p:par><p:cTn id="12" presetID="10" presetClass="entr" nodeType="afterEffect" dur="600"><p:stCondLst><p:cond delay="100"/></p:stCondLst><p:childTnLst><p:set><p:cBhvr><p:tgtEl><p:spTgt spid="4"/></p:tgtEl></p:cBhvr></p:set></p:childTnLst></p:cTn></p:par>
              <p:par><p:cTn id="13" presetID="10" presetClass="entr" nodeType="clickEffect" dur="500"><p:stCondLst><p:cond delay="0"/></p:stCondLst><p:childTnLst><p:set><p:cBhvr><p:tgtEl><p:spTgt spid="5"/></p:tgtEl></p:cBhvr></p:set></p:childTnLst></p:cTn></p:par>
            </p:childTnLst></p:cTn></p:par>
          </p:childTnLst></p:cTn></p:seq>
        </p:childTnLst></p:cTn></p:par>
      </p:tnLst>
    </p:timing>"#;

    let (course, report) = import_and_validate(&shapes, timing);
    let slide = &course.scenes[0].slides[0];
    assert_eq!(obj_by_name(slide, "Step 2").timing.start_ms, 0);
    assert_eq!(obj_by_name(slide, "Step 3").timing.start_ms, 200);
    assert_eq!(obj_by_name(slide, "Step 4").timing.start_ms, 700);
    assert_eq!(obj_by_name(slide, "Step 5").timing.start_ms, 1800);
    assert!(slide.duration_ms >= 2800);
    assert!(anim_notes(&report).iter().any(|n| n.contains("On-click")));
}

#[test]
fn c1_5_emphasis_effects_skipped_cleanly_with_diagnostics() {
    let shapes = format!(
        "{}{}{}{}",
        rect_shape_xml(2, "SpinBox", 914_400, 914_400),
        rect_shape_xml(3, "PulseBox", 2_000_000, 914_400),
        rect_shape_xml(4, "GrowBox", 3_000_000, 914_400),
        rect_shape_xml(5, "Follower", 4_000_000, 914_400),
    );

    let timing = r#"
    <p:timing>
      <p:tnLst>
        <p:par><p:cTn id="1" nodeType="tmRoot"><p:childTnLst>
          <p:seq><p:cTn id="2" nodeType="mainSeq"><p:childTnLst>
            <p:par><p:cTn id="3"><p:childTnLst>
              <p:par><p:cTn id="10" presetID="8" presetClass="emph" nodeType="withEffect" dur="400"><p:childTnLst><p:animRot><p:cBhvr><p:tgtEl><p:spTgt spid="2"/></p:tgtEl></p:cBhvr></p:animRot></p:childTnLst></p:cTn></p:par>
              <p:par><p:cTn id="11" presetID="14" presetClass="emph" nodeType="afterEffect" dur="500"><p:childTnLst><p:anim><p:cBhvr><p:tgtEl><p:spTgt spid="3"/></p:tgtEl></p:cBhvr></p:anim></p:childTnLst></p:cTn></p:par>
              <p:par><p:cTn id="12" presetID="6" presetClass="emph" nodeType="afterEffect" dur="300"><p:childTnLst><p:animScale><p:cBhvr><p:tgtEl><p:spTgt spid="4"/></p:tgtEl></p:cBhvr></p:animScale></p:childTnLst></p:cTn></p:par>
              <p:par><p:cTn id="13" presetID="10" presetClass="entr" nodeType="afterEffect" dur="400"><p:childTnLst><p:set><p:cBhvr><p:tgtEl><p:spTgt spid="5"/></p:tgtEl></p:cBhvr></p:set></p:childTnLst></p:cTn></p:par>
            </p:childTnLst></p:cTn></p:par>
          </p:childTnLst></p:cTn></p:seq>
        </p:childTnLst></p:cTn></p:par>
      </p:tnLst>
    </p:timing>"#;

    let (course, report) = import_and_validate(&shapes, timing);
    let slide = &course.scenes[0].slides[0];
    for name in ["SpinBox", "PulseBox", "GrowBox"] {
        let obj = obj_by_name(slide, name);
        assert_eq!(obj.entrance, None);
        assert_eq!(obj.exit, None);
        assert_eq!(obj.timing.start_ms, 0);
    }
    assert_eq!(obj_by_name(slide, "Follower").timing.start_ms, 1200);

    let notes = anim_notes(&report);
    assert_eq!(notes.len(), 3);
    assert!(
        notes
            .iter()
            .any(|n| n.contains("Spin") && n.contains("SpinBox"))
    );
    assert!(
        notes
            .iter()
            .any(|n| n.contains("Pulse") && n.contains("PulseBox"))
    );
    assert!(
        notes
            .iter()
            .any(|n| n.contains("Grow/Shrink") && n.contains("GrowBox"))
    );
}

#[test]
fn c1_6_motion_path_commands_triggers_and_player_playback() {
    let shapes = format!(
        "{}{}",
        rect_shape_xml(2, "MoverStart", 914_400, 914_400),
        rect_shape_xml(3, "MoverDelayed", 2_000_000, 914_400),
    );
    let timing = r#"
    <p:timing>
      <p:tnLst>
        <p:par><p:cTn id="1" nodeType="tmRoot"><p:childTnLst>
          <p:seq><p:cTn id="2" nodeType="mainSeq"><p:childTnLst>
            <p:par><p:cTn id="3"><p:childTnLst>
              <p:par><p:cTn id="10" presetID="0" presetClass="path" nodeType="withEffect" dur="1200">
                <p:childTnLst>
                  <p:animMotion path="M 0 0 L 0.25 0 C 0.25 0.1 0.1 0.2 0 0.2 E" origin="layout" pathEditMode="relative">
                    <p:cBhvr><p:cTn id="11" dur="1200"/><p:tgtEl><p:spTgt spid="2"/></p:tgtEl></p:cBhvr>
                  </p:animMotion>
                </p:childTnLst>
              </p:cTn></p:par>
              <p:par><p:cTn id="12" presetID="0" presetClass="path" nodeType="withEffect" dur="1200">
                <p:stCondLst><p:cond delay="1500"/></p:stCondLst>
                <p:childTnLst>
                  <p:animMotion path="M 0 0 L 0.25 0 C 0.25 0.1 0.1 0.2 0 0.2 E" origin="layout" pathEditMode="relative">
                    <p:cBhvr><p:cTn id="13" dur="1200"/><p:tgtEl><p:spTgt spid="3"/></p:tgtEl></p:cBhvr>
                  </p:animMotion>
                </p:childTnLst>
              </p:cTn></p:par>
            </p:childTnLst></p:cTn></p:par>
          </p:childTnLst></p:cTn></p:seq>
        </p:childTnLst></p:cTn></p:par>
      </p:tnLst>
    </p:timing>"#;

    let (course, _report) = import_and_validate(&shapes, timing);
    let slide = &course.scenes[0].slides[0];
    let m1 = obj_by_name(slide, "MoverStart");
    let m2 = obj_by_name(slide, "MoverDelayed");

    assert_eq!(m1.motion_paths.len(), 1);
    let mp1 = &m1.motion_paths[0];
    assert!((2..=100).contains(&mp1.points.len()));
    assert_eq!(mp1.points[1], Point { x: 320.0, y: 0.0 });
    assert!((mp1.points.last().unwrap().x - 0.0).abs() < 0.01);
    assert!((mp1.points.last().unwrap().y - 144.0).abs() < 0.01);

    assert!(slide.triggers.iter().any(|t| {
        t.when == Event::TimelineStarts { layer: None }
            && t.action
                == Action::MoveOnPath {
                    object: m1.id.clone(),
                    path: mp1.id.clone(),
                }
    }));
    assert!(slide.triggers.iter().any(|t| {
        t.when
            == Event::TimelineReaches {
                ms: 1500,
                layer: None,
                cue: None,
            }
            && matches!(&t.action, Action::MoveOnPath { object, .. } if object == &m2.id)
    }));

    let mut player = Player::new(&course).expect("valid player");
    player.handle(&Input::Start);
    player.handle(&Input::Tick { ms: 1200 });
    let s1 = player.sample_object(&m1.id).expect("m1 visible");
    assert!((s1.offset.0 - 0.0).abs() < 0.5);
    assert!((s1.offset.1 - 144.0).abs() < 0.5);

    player.handle(&Input::Tick { ms: 300 });
    player.handle(&Input::Tick { ms: 1200 });
    let s2 = player.sample_object(&m2.id).expect("m2 visible");
    assert!((s2.offset.0 - 0.0).abs() < 0.5);
    assert!((s2.offset.1 - 144.0).abs() < 0.5);
}

#[test]
fn c1_7_interactive_click_sequence_with_player() {
    let shapes = format!(
        "{}{}{}",
        rect_shape_xml(10, "BtnA", 914_400, 914_400),
        rect_shape_xml(11, "MoverB", 2_000_000, 914_400),
        rect_shape_xml(12, "RevealC", 3_000_000, 914_400),
    );
    let timing = r#"
    <p:timing>
      <p:tnLst>
        <p:par><p:cTn id="1" nodeType="tmRoot"><p:childTnLst>
          <p:seq><p:cTn id="2" nodeType="interactiveSeq">
            <p:stCondLst><p:cond evt="onClick"><p:tgtEl><p:spTgt spid="10"/></p:tgtEl></p:cond></p:stCondLst>
            <p:childTnLst>
              <p:par><p:cTn id="3"><p:childTnLst>
                <p:par><p:cTn id="4" presetID="0" presetClass="path" dur="600">
                  <p:childTnLst>
                    <p:animMotion path="M 0 0 L 0.25 0 E" origin="layout" pathEditMode="relative">
                      <p:cBhvr><p:cTn id="5" dur="600"/><p:tgtEl><p:spTgt spid="11"/></p:tgtEl></p:cBhvr>
                    </p:animMotion>
                  </p:childTnLst>
                </p:cTn></p:par>
                <p:par><p:cTn id="6" presetID="10" presetClass="entr" dur="400">
                  <p:childTnLst><p:set><p:cBhvr><p:tgtEl><p:spTgt spid="12"/></p:tgtEl></p:cBhvr></p:set></p:childTnLst>
                </p:cTn></p:par>
              </p:childTnLst></p:cTn></p:par>
            </p:childTnLst>
          </p:cTn></p:seq>
        </p:childTnLst></p:cTn></p:par>
      </p:tnLst>
    </p:timing>"#;

    let (course, _report) = import_and_validate(&shapes, timing);
    let slide = &course.scenes[0].slides[0];
    let btn_a = obj_by_name(slide, "BtnA");
    let mover_b = obj_by_name(slide, "MoverB");
    let reveal_c = obj_by_name(slide, "RevealC");

    assert_eq!(reveal_c.initial_state, "hidden");
    assert!(slide.triggers.iter().any(|t| {
        t.when
            == Event::Click {
                object: btn_a.id.clone(),
            }
            && t.action
                == Action::MoveOnPath {
                    object: mover_b.id.clone(),
                    path: "pptx_path_1".to_owned(),
                }
    }));
    assert!(slide.triggers.iter().any(|t| {
        t.when
            == Event::Click {
                object: btn_a.id.clone(),
            }
            && t.action
                == Action::ChangeState {
                    object: reveal_c.id.clone(),
                    state: "normal".to_owned(),
                }
    }));

    let mut player = Player::new(&course).expect("valid player");
    player.handle(&Input::Start);
    assert_eq!(player.object_state(&reveal_c.id), Some("hidden"));

    player.handle(&Input::Click {
        object: btn_a.id.clone(),
    });
    assert_eq!(player.object_state(&reveal_c.id), Some("normal"));

    player.handle(&Input::Tick { ms: 600 });
    let sb = player.sample_object(&mover_b.id).expect("mover_b visible");
    assert!((sb.offset.0 - 320.0).abs() < 0.5);
}

#[test]
fn c1_8_group_and_connector_targeting() {
    let shapes = r#"
    <p:grpSp>
      <p:nvGrpSpPr><p:cNvPr id="100" name="Group 1"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>
      <p:grpSpPr/>
      <p:sp><p:nvSpPr><p:cNvPr id="101" name="G1"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="914400" cy="914400"/></a:xfrm><a:prstGeom prst="rect"/></p:spPr></p:sp>
      <p:sp><p:nvSpPr><p:cNvPr id="102" name="G2"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x="914400" y="0"/><a:ext cx="914400" cy="914400"/></a:xfrm><a:prstGeom prst="rect"/></p:spPr></p:sp>
      <p:sp><p:nvSpPr><p:cNvPr id="103" name="G3"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x="1828800" y="0"/><a:ext cx="914400" cy="914400"/></a:xfrm><a:prstGeom prst="rect"/></p:spPr></p:sp>
    </p:grpSp>
    <p:cxnSp>
      <p:nvCxnSpPr><p:cNvPr id="200" name="Conn1"/><p:cNvCxnSpPr/><p:nvPr/></p:nvCxnSpPr>
      <p:spPr><a:xfrm><a:off x="0" y="1828800"/><a:ext cx="914400" cy="0"/></a:xfrm><a:prstGeom prst="line"/><a:ln><a:solidFill><a:srgbClr val="000000"/></a:solidFill></a:ln></p:spPr>
    </p:cxnSp>"#;

    let timing = r#"
    <p:timing>
      <p:tnLst>
        <p:par><p:cTn id="1" nodeType="tmRoot"><p:childTnLst>
          <p:seq><p:cTn id="2" nodeType="mainSeq"><p:childTnLst>
            <p:par><p:cTn id="3"><p:childTnLst>
              <p:par><p:cTn id="10" presetID="10" presetClass="entr" nodeType="withEffect" dur="500"><p:stCondLst><p:cond delay="250"/></p:stCondLst><p:childTnLst><p:set><p:cBhvr><p:tgtEl><p:spTgt spid="100"/></p:tgtEl></p:cBhvr></p:set></p:childTnLst></p:cTn></p:par>
              <p:par><p:cTn id="11" presetID="22" presetClass="entr" presetSubtype="8" nodeType="withEffect" dur="400"><p:childTnLst><p:set><p:cBhvr><p:tgtEl><p:spTgt spid="200"/></p:tgtEl></p:cBhvr></p:set></p:childTnLst></p:cTn></p:par>
            </p:childTnLst></p:cTn></p:par>
          </p:childTnLst></p:cTn></p:seq>
        </p:childTnLst></p:cTn></p:par>
      </p:tnLst>
    </p:timing>"#;

    let (course, _report) = import_and_validate(shapes, timing);
    let slide = &course.scenes[0].slides[0];
    let gid = obj_by_name(slide, "G1")
        .group
        .as_deref()
        .expect("G1 belongs to imported group");
    for name in ["G1", "G2", "G3"] {
        let obj = obj_by_name(slide, name);
        assert_eq!(obj.group.as_deref(), Some(gid));
        assert_eq!(obj.timing.start_ms, 250);
        assert_eq!(obj.entrance, None);
    }
    let grp = slide
        .groups
        .iter()
        .find(|g| g.id == gid)
        .expect("group record");
    assert_eq!(
        grp.entrance,
        Some(Animation {
            effect: AnimEffect::new("fade"),
            duration_ms: 500,
            easing: Easing::EaseOut,
            sequence: Default::default(),
        })
    );
    let conn = obj_by_name(slide, "Conn1");
    assert_eq!(
        conn.entrance,
        Some(Animation {
            effect: AnimEffect::with_from("wipe", Direction::Left),
            duration_ms: 400,
            easing: Easing::EaseOut,
            sequence: Default::default(),
        })
    );
}

#[test]
fn c1_9_easing_and_repeat_count_handling() {
    let shapes: String = (2..=5)
        .map(|id| rect_shape_xml(id, &format!("Ease {id}"), u64::from(id) * 914_400, 914_400))
        .collect();

    let timing = r#"
    <p:timing>
      <p:tnLst>
        <p:par><p:cTn id="1" nodeType="tmRoot"><p:childTnLst>
          <p:seq><p:cTn id="2" nodeType="mainSeq"><p:childTnLst>
            <p:par><p:cTn id="3"><p:childTnLst>
              <p:par><p:cTn id="10" presetID="10" presetClass="entr" nodeType="withEffect" dur="400"><p:childTnLst><p:animEffect><p:cBhvr><p:cTn id="11" dur="400" accel="0" decel="0"/><p:tgtEl><p:spTgt spid="2"/></p:tgtEl></p:cBhvr></p:animEffect></p:childTnLst></p:cTn></p:par>
              <p:par><p:cTn id="12" presetID="10" presetClass="entr" nodeType="withEffect" dur="400" repeatCount="3000"><p:childTnLst><p:animEffect><p:cBhvr><p:cTn id="13" dur="400" accel="50000" decel="0"/><p:tgtEl><p:spTgt spid="3"/></p:tgtEl></p:cBhvr></p:animEffect></p:childTnLst></p:cTn></p:par>
              <p:par><p:cTn id="14" presetID="10" presetClass="entr" nodeType="withEffect" dur="400" repeatCount="indefinite"><p:childTnLst><p:animEffect><p:cBhvr><p:cTn id="15" dur="400" accel="0" decel="50000"/><p:tgtEl><p:spTgt spid="4"/></p:tgtEl></p:cBhvr></p:animEffect></p:childTnLst></p:cTn></p:par>
              <p:par><p:cTn id="16" presetID="10" presetClass="entr" nodeType="withEffect" dur="400"><p:childTnLst><p:animEffect><p:cBhvr><p:cTn id="17" dur="400" accel="50000" decel="50000"/><p:tgtEl><p:spTgt spid="5"/></p:tgtEl></p:cBhvr></p:animEffect></p:childTnLst></p:cTn></p:par>
            </p:childTnLst></p:cTn></p:par>
          </p:childTnLst></p:cTn></p:seq>
        </p:childTnLst></p:cTn></p:par>
      </p:tnLst>
    </p:timing>"#;

    let (course, report) = import_and_validate(&shapes, timing);
    let slide = &course.scenes[0].slides[0];
    assert_eq!(
        obj_by_name(slide, "Ease 2")
            .entrance
            .as_ref()
            .unwrap()
            .easing,
        Easing::Linear
    );
    assert_eq!(
        obj_by_name(slide, "Ease 3")
            .entrance
            .as_ref()
            .unwrap()
            .easing,
        Easing::EaseIn
    );
    assert_eq!(
        obj_by_name(slide, "Ease 4")
            .entrance
            .as_ref()
            .unwrap()
            .easing,
        Easing::EaseOut
    );
    assert_eq!(
        obj_by_name(slide, "Ease 5")
            .entrance
            .as_ref()
            .unwrap()
            .easing,
        Easing::EaseInOut
    );

    let notes = anim_notes(&report);
    assert!(notes.iter().any(|n| n.contains("repeatCount=3000")));
    assert!(notes.iter().any(|n| n.contains("repeatCount=indefinite")));
}

#[test]
fn c1_10_presets_map_to_effects_and_unknown_falls_back() {
    let shapes = format!(
        "{}{}{}",
        rect_shape_xml(2, "BounceBox", 914_400, 914_400),
        rect_shape_xml(3, "ExoticBox", 2_000_000, 914_400),
        rect_shape_xml(4, "DiagFly", 3_000_000, 914_400),
    );
    let timing = r#"
    <p:timing>
      <p:tnLst>
        <p:par><p:cTn id="1" nodeType="tmRoot"><p:childTnLst>
          <p:seq><p:cTn id="2" nodeType="mainSeq"><p:childTnLst>
            <p:par><p:cTn id="3"><p:childTnLst>
              <p:par><p:cTn id="10" presetID="26" presetClass="entr" nodeType="withEffect" dur="500"><p:stCondLst><p:cond delay="200"/></p:stCondLst><p:childTnLst><p:set><p:cBhvr><p:tgtEl><p:spTgt spid="2"/></p:tgtEl></p:cBhvr></p:set></p:childTnLst></p:cTn></p:par>
              <p:par><p:cTn id="11" presetID="99" presetClass="entr" nodeType="withEffect" dur="500"><p:stCondLst><p:cond delay="400"/></p:stCondLst><p:childTnLst><p:set><p:cBhvr><p:tgtEl><p:spTgt spid="3"/></p:tgtEl></p:cBhvr></p:set></p:childTnLst></p:cTn></p:par>
              <p:par><p:cTn id="12" presetID="2" presetClass="entr" presetSubtype="6" nodeType="withEffect" dur="500"><p:childTnLst><p:set><p:cBhvr><p:tgtEl><p:spTgt spid="4"/></p:tgtEl></p:cBhvr></p:set></p:childTnLst></p:cTn></p:par>
            </p:childTnLst></p:cTn></p:par>
          </p:childTnLst></p:cTn></p:seq>
        </p:childTnLst></p:cTn></p:par>
      </p:tnLst>
    </p:timing>"#;

    let (course, report) = import_and_validate(&shapes, timing);
    let slide = &course.scenes[0].slides[0];

    let bounce = obj_by_name(slide, "BounceBox");
    assert_eq!(bounce.timing.start_ms, 200);
    assert_eq!(
        bounce.entrance.as_ref().unwrap().effect,
        AnimEffect::new("bounce"),
        "Bounce (26) imports as itself since M46"
    );

    let exotic = obj_by_name(slide, "ExoticBox");
    assert_eq!(exotic.timing.start_ms, 400);
    assert_eq!(
        exotic.entrance.as_ref().unwrap().effect,
        AnimEffect::new("fade")
    );

    let diag = obj_by_name(slide, "DiagFly");
    assert_eq!(
        diag.entrance.as_ref().unwrap().effect,
        AnimEffect::with_from("fly_in", Direction::BottomRight),
        "Fly In keeps its corner since M46"
    );

    let notes = anim_notes(&report);
    assert!(notes.iter().any(|n| n.contains("id=99")));
    assert!(!notes.iter().any(|n| n.contains("id=26")));
    assert!(!notes.iter().any(|n| n.contains("Diagonal")));
}

#[test]
fn c1_11_combined_entrance_motion_path_and_exit_with_player() {
    let shapes = rect_shape_xml(2, "ComboBox", 914_400, 914_400);
    let timing = r#"
    <p:timing>
      <p:tnLst>
        <p:par><p:cTn id="1" nodeType="tmRoot"><p:childTnLst>
          <p:seq><p:cTn id="2" nodeType="mainSeq"><p:childTnLst>
            <p:par><p:cTn id="3"><p:childTnLst>
              <p:par><p:cTn id="10" presetID="10" presetClass="entr" nodeType="withEffect" dur="500">
                <p:stCondLst><p:cond delay="500"/></p:stCondLst>
                <p:childTnLst><p:set><p:cBhvr><p:tgtEl><p:spTgt spid="2"/></p:tgtEl></p:cBhvr></p:set></p:childTnLst>
              </p:cTn></p:par>
              <p:par><p:cTn id="11" presetID="0" presetClass="path" nodeType="afterEffect" dur="1000">
                <p:stCondLst><p:cond delay="200"/></p:stCondLst>
                <p:childTnLst>
                  <p:animMotion path="M 0 0 L 0.25 0 E" origin="layout" pathEditMode="relative">
                    <p:cBhvr><p:cTn id="12" dur="1000"/><p:tgtEl><p:spTgt spid="2"/></p:tgtEl></p:cBhvr>
                  </p:animMotion>
                </p:childTnLst>
              </p:cTn></p:par>
              <p:par><p:cTn id="13" presetID="10" presetClass="exit" nodeType="afterEffect" dur="500">
                <p:stCondLst><p:cond delay="800"/></p:stCondLst>
                <p:childTnLst><p:set><p:cBhvr><p:tgtEl><p:spTgt spid="2"/></p:tgtEl></p:cBhvr></p:set></p:childTnLst>
              </p:cTn></p:par>
            </p:childTnLst></p:cTn></p:par>
          </p:childTnLst></p:cTn></p:seq>
        </p:childTnLst></p:cTn></p:par>
      </p:tnLst>
    </p:timing>"#;

    let (course, _report) = import_and_validate(&shapes, timing);
    let slide = &course.scenes[0].slides[0];
    let obj = obj_by_name(slide, "ComboBox");
    assert_eq!(obj.timing.start_ms, 500);
    assert_eq!(obj.timing.end_ms, Some(3500));
    assert!(obj.entrance.is_some());
    assert_eq!(obj.motion_paths.len(), 1);
    assert!(obj.exit.is_some());

    let mut player = Player::new(&course).expect("valid player");
    player.handle(&Input::Start);
    player.handle(&Input::Tick { ms: 200 });
    assert!(player.sample_object(&obj.id).is_none(), "before entrance");

    player.handle(&Input::Tick { ms: 900 });
    let s1 = player
        .sample_object(&obj.id)
        .expect("visible after entrance");
    assert!((s1.offset.0 - 0.0).abs() < 0.5);

    player.handle(&Input::Tick { ms: 100 });
    player.handle(&Input::Tick { ms: 1000 });
    let s2 = player.sample_object(&obj.id).expect("visible after motion");
    assert!((s2.offset.0 - 320.0).abs() < 0.5);

    player.handle(&Input::Tick { ms: 1300 });
    assert!(player.sample_object(&obj.id).is_none(), "hidden after exit");
}

#[test]
fn c1_12_overlapping_exit_before_start_and_duplicate_entrance() {
    let shapes = rect_shape_xml(2, "ConflictBox", 914_400, 914_400);
    let timing = r#"
    <p:timing>
      <p:tnLst>
        <p:par><p:cTn id="1" nodeType="tmRoot"><p:childTnLst>
          <p:seq><p:cTn id="2" nodeType="mainSeq"><p:childTnLst>
            <p:par><p:cTn id="3"><p:childTnLst>
              <p:par><p:cTn id="10" presetID="10" presetClass="entr" nodeType="withEffect" dur="400"><p:stCondLst><p:cond delay="1000"/></p:stCondLst><p:childTnLst><p:set><p:cBhvr><p:tgtEl><p:spTgt spid="2"/></p:tgtEl></p:cBhvr></p:set></p:childTnLst></p:cTn></p:par>
              <p:par><p:cTn id="11" presetID="23" presetClass="entr" nodeType="withEffect" dur="400"><p:stCondLst><p:cond delay="1200"/></p:stCondLst><p:childTnLst><p:set><p:cBhvr><p:tgtEl><p:spTgt spid="2"/></p:tgtEl></p:cBhvr></p:set></p:childTnLst></p:cTn></p:par>
              <p:par><p:cTn id="12" presetID="10" presetClass="exit" nodeType="withEffect" dur="200"><p:stCondLst><p:cond delay="100"/></p:stCondLst><p:childTnLst><p:set><p:cBhvr><p:tgtEl><p:spTgt spid="2"/></p:tgtEl></p:cBhvr></p:set></p:childTnLst></p:cTn></p:par>
            </p:childTnLst></p:cTn></p:par>
          </p:childTnLst></p:cTn></p:seq>
        </p:childTnLst></p:cTn></p:par>
      </p:tnLst>
    </p:timing>"#;

    let (course, report) = import_and_validate(&shapes, timing);
    let slide = &course.scenes[0].slides[0];
    let obj = obj_by_name(slide, "ConflictBox");
    assert_eq!(
        obj.entrance.as_ref().unwrap().effect,
        AnimEffect::new("fade")
    );
    assert_eq!(obj.timing.start_ms, 1000);
    assert!(obj.timing.end_ms.is_some_and(|e| e > obj.timing.start_ms));
    assert!(
        anim_notes(&report)
            .iter()
            .any(|n| n.contains("Duplicate entrance"))
    );
}

#[test]
fn c1_13_hostile_and_malformed_timing_resilience() {
    let shapes = format!(
        "{}{}",
        rect_shape_xml(2, "HostileBox", 914_400, 914_400),
        rect_shape_xml(3, "LongPathBox", 2_000_000, 914_400),
    );

    let mut huge_path = String::from("M 0 0");
    for i in 1..=5000 {
        huge_path.push_str(&format!(" L {:.4} 0", f64::from(i) / 10_000.0));
    }
    huge_path.push_str(" E");

    let timing = format!(
        r#"
    <p:timing>
      <p:tnLst>
        <p:par><p:cTn id="1" nodeType="tmRoot"><p:childTnLst>
          <p:seq><p:cTn id="2" nodeType="mainSeq"><p:childTnLst>
            <p:par><p:cTn id="3"><p:childTnLst>
              <p:par><p:cTn id="10" presetID="10" presetClass="entr" nodeType="withEffect" dur="999999999999">
                <p:stCondLst><p:cond delay="-500"/><p:cond delay="NaN"/></p:stCondLst>
                <p:childTnLst><p:set><p:cBhvr><p:tgtEl><p:spTgt spid="2"/></p:tgtEl></p:cBhvr></p:set></p:childTnLst>
              </p:cTn></p:par>
              <p:par><p:cTn id="11" presetID="10" presetClass="entr" nodeType="withEffect" dur="500">
                <p:childTnLst><p:set><p:cBhvr><p:tgtEl><p:spTgt spid="99999"/></p:tgtEl></p:cBhvr></p:set></p:childTnLst>
              </p:cTn></p:par>
              <p:par><p:cTn id="12" presetID="0" presetClass="path" nodeType="withEffect" dur="1000">
                <p:childTnLst><p:animMotion path="{huge_path}"><p:cBhvr><p:tgtEl><p:spTgt spid="3"/></p:tgtEl></p:cBhvr></p:animMotion></p:childTnLst>
              </p:cTn></p:par>
              <p:par><p:cTn id="13" presetID="0" presetClass="path" nodeType="withEffect" dur="1000">
                <p:childTnLst><p:animMotion path="M foo bar"><p:cBhvr><p:tgtEl><p:spTgt spid="2"/></p:tgtEl></p:cBhvr></p:animMotion></p:childTnLst>
              </p:cTn></p:par>
            </p:childTnLst></p:cTn></p:par>
          </p:childTnLst></p:cTn></p:seq>
        </p:childTnLst></p:cTn></p:par>
      </p:tnLst>
    </p:timing>"#
    );

    let (course, report) = import_and_validate(&shapes, &timing);
    let slide = &course.scenes[0].slides[0];
    let hostile = obj_by_name(slide, "HostileBox");
    assert_eq!(hostile.timing.start_ms, 0);
    assert_eq!(hostile.entrance.as_ref().unwrap().duration_ms, 60_000);
    assert!(hostile.motion_paths.is_empty());

    let long_path = obj_by_name(slide, "LongPathBox");
    assert_eq!(long_path.motion_paths.len(), 1);
    assert!(long_path.motion_paths[0].points.len() <= 100);

    let notes = anim_notes(&report);
    assert!(notes.iter().any(|n| n.contains("99999")));
    assert!(notes.iter().any(|n| n.contains("Malformed")));

    // Deeply nested <p:par> (>64 levels): import_pptx rejects excessive XML depth safely,
    // and direct apply_slide_timing enforces MAX_TIMING_DEPTH = 64 without panicking.
    let mut deep_timing = String::from(
        r#"<p:timing xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:tnLst><p:par><p:cTn id="1" nodeType="tmRoot"><p:childTnLst><p:seq><p:cTn id="2" nodeType="mainSeq"><p:childTnLst>"#,
    );
    for i in 0..70 {
        deep_timing.push_str(&format!(r#"<p:par><p:cTn id="{}"><p:childTnLst>"#, i + 10));
    }
    deep_timing.push_str(
        r#"<p:par><p:cTn id="999" presetID="10" presetClass="entr" dur="500"><p:childTnLst><p:set><p:cBhvr><p:tgtEl><p:spTgt spid="2"/></p:tgtEl></p:cBhvr></p:set></p:childTnLst></p:cTn></p:par>"#,
    );
    for _ in 0..70 {
        deep_timing.push_str("</p:childTnLst></p:cTn></p:par>");
    }
    deep_timing.push_str(
        "</p:childTnLst></p:cTn></p:seq></p:childTnLst></p:cTn></p:par></p:tnLst></p:timing>",
    );

    let deep_bytes = build_pptx_with_slide(&shapes, &deep_timing);
    let err = import_pptx(&deep_bytes, "Deep").expect_err("depth > 64 rejected");
    assert!(matches!(err, PptxError::XmlDepthExceeded(_)));

    let raised = Limits {
        max_xml_depth: 256,
        ..Limits::default()
    };
    let imported_deep =
        import_pptx_with_limits(&deep_bytes, "Deep", raised).expect("raised xml depth");
    assert!(
        imported_deep
            .report
            .items
            .iter()
            .any(|i| i.kind == SkipKind::Animation && i.note.contains("depth"))
    );

    let tree = parse_xml_tree(&deep_timing, "slide1.xml", 256).expect("parse deep timing");
    let mut objects = vec![hostile.clone()];
    let mut groups = Vec::new();
    let mut triggers = Vec::new();
    let mut tracker = SlideTracker::new(1);
    tracker
        .pptx_ids
        .insert("2".to_owned(), vec![hostile.id.clone()]);
    let layout =
        SlideLayout::from_emu(f64::from(STD_16_9_CX as u32), f64::from(STD_16_9_CY as u32));
    apply_slide_timing(
        &tree,
        &layout,
        &mut objects,
        &mut groups,
        &mut triggers,
        &mut tracker,
    );
    assert!(
        tracker
            .items
            .iter()
            .any(|i| i.kind == SkipKind::Animation && i.note.contains("depth"))
    );
}

#[test]
fn m49_s1_pptx_group_entrance_lands_on_group_record() {
    let sp_tree_inner = r#"
        <p:grpSp>
          <p:nvGrpSpPr><p:cNvPr id="10" name="Card Group"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>
          <p:grpSpPr>
            <a:xfrm><a:off x="0" y="0"/><a:ext cx="1828800" cy="914400"/>
              <a:chOff x="0" y="0"/><a:chExt cx="1828800" cy="914400"/></a:xfrm>
          </p:grpSpPr>
          <p:sp>
            <p:nvSpPr><p:cNvPr id="11" name="Box A"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr>
            <p:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="914400" cy="914400"/></a:xfrm>
              <a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr>
          </p:sp>
          <p:sp>
            <p:nvSpPr><p:cNvPr id="12" name="Box B"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr>
            <p:spPr><a:xfrm><a:off x="914400" y="0"/><a:ext cx="914400" cy="914400"/></a:xfrm>
              <a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr>
          </p:sp>
        </p:grpSp>"#;
    let timing_xml = r#"
      <p:timing>
        <p:tnLst><p:par><p:cTn id="1" nodeType="tmRoot"><p:childTnLst>
          <p:seq><p:cTn id="2" nodeType="mainSeq"><p:childTnLst>
            <p:par><p:cTn id="3" presetID="10" presetClass="entr" nodeType="withEffect" dur="500">
              <p:stCondLst><p:cond delay="250"/></p:stCondLst>
              <p:childTnLst><p:set><p:cBhvr><p:tgtEl><p:spTgt spid="10"/></p:tgtEl></p:cBhvr></p:set></p:childTnLst>
            </p:cTn></p:par>
          </p:childTnLst></p:cTn></p:seq>
        </p:childTnLst></p:cTn></p:par></p:tnLst>
      </p:timing>"#;

    let bytes = build_pptx_with_slide(sp_tree_inner, timing_xml);
    let imported = import_pptx(&bytes, "GroupAnim").expect("import pptx");
    let slide = &imported.course.scenes[0].slides[0];
    assert_eq!(slide.objects.len(), 2);
    let gid = slide.objects[0]
        .group
        .as_deref()
        .expect("child 0 has group id");
    assert_eq!(slide.objects[1].group.as_deref(), Some(gid));
    assert_eq!(slide.objects[0].entrance, None);
    assert_eq!(slide.objects[1].entrance, None);
    assert_eq!(slide.objects[0].timing.start_ms, 250);
    assert_eq!(slide.objects[1].timing.start_ms, 250);
    let grp = slide
        .groups
        .iter()
        .find(|g| g.id == gid)
        .expect("group record created");
    assert!(!grp.interactive);
    assert_eq!(
        grp.entrance.as_ref().map(|a| a.effect.id.as_str()),
        Some("fade")
    );
    assert_eq!(grp.entrance.as_ref().map(|a| a.duration_ms), Some(500));
}

#[test]
fn m49_s1_pptx_group_appear_omits_empty_record_and_sanitizes_hostile_spid() {
    let sp_tree_inner = r#"
        <p:grpSp>
          <p:nvGrpSpPr><p:cNvPr id="bad id!@#" name="Hostile Group"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>
          <p:grpSpPr>
            <a:xfrm><a:off x="0" y="0"/><a:ext cx="1828800" cy="914400"/>
              <a:chOff x="0" y="0"/><a:chExt cx="1828800" cy="914400"/></a:xfrm>
          </p:grpSpPr>
          <p:sp>
            <p:nvSpPr><p:cNvPr id="21" name="A"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr>
            <p:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="914400" cy="914400"/></a:xfrm>
              <a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr>
          </p:sp>
          <p:sp>
            <p:nvSpPr><p:cNvPr id="22" name="B"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr>
            <p:spPr><a:xfrm><a:off x="914400" y="0"/><a:ext cx="914400" cy="914400"/></a:xfrm>
              <a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr>
          </p:sp>
        </p:grpSp>
        <p:grpSp>
          <p:nvGrpSpPr><p:cNvPr id="30" name="Appear Group"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>
          <p:grpSpPr>
            <a:xfrm><a:off x="0" y="1000000"/><a:ext cx="1828800" cy="914400"/>
              <a:chOff x="0" y="0"/><a:chExt cx="1828800" cy="914400"/></a:xfrm>
          </p:grpSpPr>
          <p:sp>
            <p:nvSpPr><p:cNvPr id="31" name="C"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr>
            <p:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="914400" cy="914400"/></a:xfrm>
              <a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr>
          </p:sp>
          <p:sp>
            <p:nvSpPr><p:cNvPr id="32" name="D"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr>
            <p:spPr><a:xfrm><a:off x="914400" y="0"/><a:ext cx="914400" cy="914400"/></a:xfrm>
              <a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr>
          </p:sp>
        </p:grpSp>"#;
    let timing_xml = r#"
      <p:timing>
        <p:tnLst><p:par><p:cTn id="1" nodeType="tmRoot"><p:childTnLst>
          <p:seq><p:cTn id="2" nodeType="mainSeq"><p:childTnLst>
            <p:par><p:cTn id="3" presetID="10" presetClass="entr" nodeType="withEffect" dur="500">
              <p:stCondLst><p:cond delay="100"/></p:stCondLst>
              <p:childTnLst><p:set><p:cBhvr><p:tgtEl><p:spTgt spid="bad id!@#"/></p:tgtEl></p:cBhvr></p:set></p:childTnLst>
            </p:cTn></p:par>
            <p:par><p:cTn id="4" presetID="1" presetClass="entr" nodeType="withEffect" dur="1">
              <p:stCondLst><p:cond delay="300"/></p:stCondLst>
              <p:childTnLst><p:set><p:cBhvr><p:tgtEl><p:spTgt spid="30"/></p:tgtEl></p:cBhvr></p:set></p:childTnLst>
            </p:cTn></p:par>
          </p:childTnLst></p:cTn></p:seq>
        </p:childTnLst></p:cTn></p:par></p:tnLst>
      </p:timing>"#;

    let (course, _report) = import_and_validate(sp_tree_inner, timing_xml);
    let slide = &course.scenes[0].slides[0];
    // Only the faded group gets a Group record; Appear (presetID=1) leaves no empty record.
    assert_eq!(slide.groups.len(), 1);
    assert_eq!(slide.groups[0].id, "pptx_grp_badid");
    assert_eq!(obj_by_name(slide, "C").timing.start_ms, 300);
    assert_eq!(obj_by_name(slide, "D").timing.start_ms, 300);
}
