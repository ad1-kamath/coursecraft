//! Editor frame-time measurements on a 300-slide course. Ignored by default:
//! timings only mean something in a release build. Run with
//! `cargo test --release -p coursecraft-ui-egui perf_tests -- --ignored --nocapture`.
//! Budgets: idle frame at most 16 ms, interaction frame at most 33 ms.

use std::time::{Duration, Instant};

use crate::test_harness::TestHarness;
use coursecraft_course::Course;
use egui::{Event, Key, Modifiers, RawInput, pos2};

const SLIDES: usize = 300;

/// The same shape as the big-course speed test (300 slides in 10 scenes). The
/// pictures name files the editor does not have, which stubs the media out.
fn big_course() -> Course {
    let mut scenes = Vec::new();
    for scene in 0..10 {
        let mut slides = Vec::new();
        for n in scene * 30..(scene + 1) * 30 {
            let target = if n + 1 < SLIDES {
                "\"next\"".to_string()
            } else {
                "{\"id\":\"s-0\"}".to_string()
            };
            let extra = if n % 10 == 0 {
                format!(
                    r#","layers":[{{"id":"lay{n}","name":"Hint","objects":[{{"id":"lt{n}","kind":{{"type":"text","text":"A hint."}},"frame":{{"x":300,"y":200,"w":500,"h":100}}}}]}}],
                    "question":{{"kind":"multiple_choice","prompt":"Pick","choices":[{{"id":"a","text":"Wrong"}},{{"id":"b","text":"Right","correct":true}}],"points":10,"attempts":2}}"#
                )
            } else {
                String::new()
            };
            slides.push(format!(
                r##"{{"id":"s-{n}","title":"Slide {n}","duration_ms":3000,"objects":[
                {{"id":"t{n}","kind":{{"type":"text","text":"Slide {n} title","style":{{"size":44,"bold":true}}}},"frame":{{"x":60,"y":40,"w":1100,"h":80}}}},
                {{"id":"a{n}","kind":{{"type":"text","text":"First paragraph of body text."}},"frame":{{"x":60,"y":140,"w":500,"h":160}}}},
                {{"id":"b{n}","kind":{{"type":"text","text":"Second paragraph of body text."}},"frame":{{"x":60,"y":320,"w":500,"h":160}}}},
                {{"id":"p{n}","kind":{{"type":"image","src":"img/p{n}.png","alt":"Test picture"}},"frame":{{"x":600,"y":140,"w":560,"h":340}}}},
                {{"id":"h{n}","kind":{{"type":"shape","shape":"rounded_rectangle","fill":"#2E8B57"}},"frame":{{"x":60,"y":520,"w":200,"h":80}}}},
                {{"id":"btn{n}","kind":{{"type":"button","label":"Next"}},"frame":{{"x":1000,"y":600,"w":200,"h":60}}}}],
                "triggers":[{{"action":{{"action":"jump_to_slide","target":{target}}},"when":{{"event":"click","object":"btn{n}"}}}}]{extra}}}"##
            ));
        }
        scenes.push(format!(
            r#"{{"id":"sc{scene}","title":"Scene {scene}","slides":[{}]}}"#,
            slides.join(",")
        ));
    }
    let json = format!(
        r#"{{"schema_version":1,"id":"big","title":"Big","slide_size":{{"w":1280,"h":720}},"scenes":[{}]}}"#,
        scenes.join(",")
    );
    Course::from_json(&json).expect("the generated big course parses")
}

/// One frame with events, timed through layout and tessellation.
fn timed(h: &mut TestHarness, events: Vec<Event>) -> Duration {
    let raw = RawInput {
        screen_rect: Some(h.screen_rect),
        events,
        ..Default::default()
    };
    let t = Instant::now();
    let output = h.ctx.run_ui(raw, |ui| {
        let _ = h.editor.show(ui);
    });
    let _meshes = h
        .ctx
        .tessellate(output.shapes.clone(), output.pixels_per_point);
    let took = t.elapsed();
    output.drop_without_applying_deltas();
    took
}

fn report(name: &str, budget_ms: f64, samples: &[Duration]) -> f64 {
    let ms: Vec<f64> = samples.iter().map(|d| d.as_secs_f64() * 1000.0).collect();
    let avg = ms.iter().sum::<f64>() / ms.len().max(1) as f64;
    let worst = ms.iter().copied().fold(0.0, f64::max);
    println!("| {name:<26} | avg {avg:>7.2} ms | worst {worst:>7.2} ms | budget {budget_ms} ms");
    worst
}

#[test]
#[ignore = "timing test: run in release with --ignored --nocapture"]
fn editor_frame_times_on_a_300_slide_course() {
    let mut h = TestHarness::new(big_course());
    h.editor.selection.slide = "s-0".into();
    for _ in 0..5 {
        h.idle_frame();
    }

    let idle: Vec<_> = (0..60).map(|_| timed(&mut h, Vec::new())).collect();
    report("idle", 16.0, &idle);

    let select: Vec<_> = (0..60)
        .map(|i| {
            h.editor.selection.slide = format!("s-{}", 150 + i % 2);
            timed(&mut h, Vec::new())
        })
        .collect();
    report("select slide 150", 33.0, &select);

    let list_pos = h
        .find_text_containing("Slide 1")
        .unwrap_or(pos2(90.0, 400.0));
    h.frame(vec![Event::PointerMoved(list_pos)]);
    let scroll: Vec<_> = (0..60)
        .map(|i| {
            let dy = if i % 2 == 0 { -240.0 } else { 240.0 };
            timed(
                &mut h,
                vec![Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, dy),
                    phase: egui::TouchPhase::Move,
                    source: egui::MouseWheelSource::Wheel,
                    modifiers: Modifiers::NONE,
                }],
            )
        })
        .collect();
    report("scroll slide list", 33.0, &scroll);

    // Typing: focus the button label field in Properties, then type a letter a frame.
    h.editor.selection.slide = "s-150".into();
    h.editor.selection.object = Some("btn150".into());
    for _ in 0..3 {
        h.idle_frame();
    }
    let rect = h.editor.ui.properties.btn_label_rect;
    assert!(rect.is_some(), "the button label field should be laid out");
    if let Some(rect) = rect {
        h.click_at(rect.center());
    }
    let typing: Vec<_> = (0..60)
        .map(|_| {
            timed(
                &mut h,
                vec![
                    Event::Text("x".into()),
                    Event::Key {
                        key: Key::X,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: Modifiers::NONE,
                    },
                ],
            )
        })
        .collect();
    report("typing in a text field", 33.0, &typing);
}
