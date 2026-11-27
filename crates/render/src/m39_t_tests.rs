//! M39 package T: text highlight and shadow in the layout, the offscreen render, the
//! animation sequence (by paragraph and by letter) and the mesh pass (scale, rotation, clip).

use coursecraft_course::{Align, Autofit, Course, VAlign};
use coursecraft_logic::{ConvexPoly, Mask, ObjectSample};

use crate::compose::{Hit, ResolvedTextRun, apply_sample_to_items, compose_authoring};
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

fn run(text: &str) -> ResolvedTextRun {
    ResolvedTextRun {
        text: text.to_owned(),
        size: 20.0,
        color: egui::Color32::BLACK,
        bold: false,
        italic: false,
        underline: false,
        strikethrough: false,
        baseline: coursecraft_course::Baseline::Normal,
        letter_spacing: 0.0,
        font: None,
        link: None,
        highlight: None,
        shadow: false,
    }
}

fn lay_out(ctx: &egui::Context, runs: &[ResolvedTextRun]) -> crate::text_layout::LaidText {
    crate::text_layout::layout_text(
        ctx,
        egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 200.0)),
        1.0,
        runs,
        &[],
        Align::Left,
        VAlign::Top,
        Autofit::None,
    )
}

const YELLOW: egui::Color32 = egui::Color32::from_rgb(255, 255, 0);

#[test]
fn highlight_rects_cover_only_the_highlighted_letters() {
    let ctx = egui::Context::default();
    let mut marked = run("mark");
    marked.highlight = Some(YELLOW);
    let runs = [run("ab "), marked, run(" cd")];
    let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
        let laid = lay_out(ui.ctx(), &runs);
        let paragraph = &laid.paragraphs[0];
        let rects = crate::text_layout::highlight_rects(paragraph, paragraph.pos);
        assert_eq!(rects.len(), 1, "one continuous rectangle");
        let (rect, color) = rects[0];
        assert_eq!(color, YELLOW);
        let row = &paragraph.galley.rows[0];
        let first = &row.glyphs[3];
        let last = &row.glyphs[6];
        assert!((rect.left() - (paragraph.pos.x + row.pos.x + first.pos.x)).abs() < 0.5);
        assert!(
            (rect.right() - (paragraph.pos.x + row.pos.x + last.pos.x + last.advance_width)).abs()
                < 0.5
        );
        assert!((rect.height() - row.size.y).abs() < 0.5);
        assert!(paragraph.shadow.is_none(), "no run has a shadow");
    });
    out.textures_delta.clear();
}

#[test]
fn a_highlight_that_wraps_gets_one_rectangle_per_line() {
    let ctx = egui::Context::default();
    let mut long = run("word word word word word word word word word word word word");
    long.highlight = Some(YELLOW);
    long.size = 30.0;
    let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
        let laid = crate::text_layout::layout_text(
            ui.ctx(),
            egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(200.0, 400.0)),
            1.0,
            std::slice::from_ref(&long),
            &[],
            Align::Left,
            VAlign::Top,
            Autofit::None,
        );
        let paragraph = &laid.paragraphs[0];
        let lines = paragraph.galley.rows.len();
        assert!(lines > 2, "the text wraps: {lines}");
        let rects = crate::text_layout::highlight_rects(paragraph, paragraph.pos);
        assert_eq!(rects.len(), lines);
    });
    out.textures_delta.clear();
}

#[test]
fn a_shadow_run_gets_an_offset_shadow_galley_in_black_at_45_percent() {
    let ctx = egui::Context::default();
    let mut shadowed = run("shadow");
    shadowed.shadow = true;
    shadowed.size = 100.0;
    let runs = [run("plain "), shadowed];
    let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
        let laid = lay_out(ui.ctx(), &runs);
        let paragraph = &laid.paragraphs[0];
        let (galley, offset) = paragraph.shadow.as_ref().expect("shadow galley");
        assert!((offset.x - 1.5).abs() < 0.01, "1.5% of 100: {offset:?}",);
        assert_eq!(offset.x, offset.y);
        assert_eq!(galley.text(), paragraph.galley.text());
        let colours: Vec<egui::Color32> = galley
            .job
            .sections
            .iter()
            .map(|section| section.format.color)
            .collect();
        assert_eq!(
            colours[0],
            egui::Color32::TRANSPARENT,
            "plain run draws no shadow"
        );
        assert_eq!(
            colours[1],
            egui::Color32::from_rgba_unmultiplied(0, 0, 0, 115),
            "black at 45% of 255"
        );
    });
    out.textures_delta.clear();

    // Small text still moves its shadow at least one pixel.
    let mut tiny = run("tiny");
    tiny.shadow = true;
    let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
        let laid = lay_out(ui.ctx(), std::slice::from_ref(&tiny));
        let (_, offset) = laid.paragraphs[0].shadow.clone().expect("shadow");
        assert!(offset.x >= 1.0);
    });
    out.textures_delta.clear();
}

#[test]
fn the_layout_cache_tells_highlight_and_shadow_apart() {
    let ctx = egui::Context::default();
    let plain = run("cached");
    let mut marked = run("cached");
    marked.highlight = Some(YELLOW);
    let mut shadowed = run("cached");
    shadowed.shadow = true;
    let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
        let a = lay_out(ui.ctx(), std::slice::from_ref(&plain));
        let b = lay_out(ui.ctx(), std::slice::from_ref(&marked));
        let c = lay_out(ui.ctx(), std::slice::from_ref(&shadowed));
        let a2 = lay_out(ui.ctx(), std::slice::from_ref(&plain));
        assert!(a.paragraphs[0].highlights.is_empty());
        assert_eq!(b.paragraphs[0].highlights.len(), 1);
        assert!(c.paragraphs[0].shadow.is_some());
        assert!(a2.paragraphs[0].highlights.is_empty() && a2.paragraphs[0].shadow.is_none());
    });
    out.textures_delta.clear();
}

#[test]
fn the_inline_edit_job_carries_the_highlight_as_a_background() {
    let ctx = egui::Context::default();
    let mut marked = run("ab");
    marked.highlight = Some(YELLOW);
    let runs = [run("x"), marked];
    let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
        let job = crate::text_layout::inline_edit_layout_job(
            ui.ctx(),
            1.0,
            1.0,
            &runs,
            &[],
            Align::Left,
            400.0,
            true,
        );
        assert_eq!(
            job.sections[0].format.background,
            egui::Color32::TRANSPARENT
        );
        assert_eq!(job.sections[1].format.background, YELLOW);
    });
    out.textures_delta.clear();
}

// --- Offscreen renders -------------------------------------------------------------------

fn course_with_text(text: &str, runs: serde_json::Value, extra: serde_json::Value) -> Course {
    let mut object = serde_json::json!({
        "id": "t",
        "kind": { "type": "text", "text": text, "runs": runs,
                  "style": { "size": 60.0, "color": "#000000" } },
        "frame": { "x": 100, "y": 100, "w": 800, "h": 300 }
    });
    if let (Some(to), Some(from)) = (object.as_object_mut(), extra.as_object()) {
        to.extend(from.clone());
    }
    let json = serde_json::json!({
        "schema_version": 1, "id": "m39-t", "title": "M39 T",
        "slide_size": { "w": 1280, "h": 720 },
        "scenes": [{ "id": "s1", "title": "Scene", "slides": [{
            "id": "slide1", "title": "Slide", "background": "#FFFFFF", "duration_ms": 3000,
            "objects": [object]
        }] }]
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

/// Strongly yellow: red and green high, blue clearly lower (also at half opacity).
fn is_yellow(p: [u8; 4]) -> bool {
    p[0] > 230 && p[1] > 230 && p[2] < 200
}

/// The bounding box `(x0, y0, x1, y1)` of the yellow pixels, if any.
fn yellow_box(img: &RgbaImage) -> Option<(u32, u32, u32, u32)> {
    let mut found: Option<(u32, u32, u32, u32)> = None;
    for y in 0..img.height {
        for x in 0..img.width {
            if is_yellow(px(img, x, y)) {
                found = Some(match found {
                    None => (x, y, x, y),
                    Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x), y1.max(y)),
                });
            }
        }
    }
    found
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

fn count_non_white(img: &RgbaImage) -> usize {
    (0..img.height)
        .flat_map(|y| (0..img.width).map(move |x| (x, y)))
        .filter(|&(x, y)| px(img, x, y)[0] < 250)
        .count()
}

#[test]
fn offscreen_render_draws_the_highlight_behind_the_marked_words_only() {
    let c = course_with_text(
        "MM MM",
        serde_json::json!([{ "text": "MM " }, { "text": "MM", "highlight": "#FFFF00" }]),
        serde_json::json!({}),
    );
    let img = player_at(&c, 1000);
    let (x0, y0, x1, y1) = yellow_box(&img).expect("a yellow highlight");
    // The first word has no highlight: the yellow starts to the right of the frame's left edge.
    assert!(x0 > 100 + 40, "highlight starts after the first word: {x0}");
    assert!(x1 < 900 && y0 >= 90 && y1 <= 200);
    // The glyphs are drawn over it.
    let dark = (y0..=y1)
        .flat_map(|y| (x0..=x1).map(move |x| (x, y)))
        .filter(|&(x, y)| px(&img, x, y)[0] < 80)
        .count();
    assert!(dark > 50, "black glyph pixels over the highlight: {dark}");
}

#[test]
fn offscreen_render_draws_a_shadow_that_a_plain_run_does_not() {
    let plain = course_with_text("MMMM", serde_json::json!([]), serde_json::json!({}));
    let shadowed = course_with_text(
        "MMMM",
        serde_json::json!([{ "text": "MMMM", "shadow": true }]),
        serde_json::json!({}),
    );
    let a = count_non_white(&player_at(&plain, 1000));
    let b = count_non_white(&player_at(&shadowed, 1000));
    assert!(a > 100, "text is drawn");
    assert!(b > a + 20, "the shadow adds pixels: {a} -> {b}");
}

// --- Animation sequence ------------------------------------------------------------------

fn sequence_course(sequence: &str, text: &str, runs: serde_json::Value) -> Course {
    course_with_text(
        text,
        runs,
        serde_json::json!({
            "entrance": { "effect": { "type": "fade" }, "duration_ms": 1000,
                          "easing": "linear", "sequence": sequence }
        }),
    )
}

#[test]
fn by_paragraph_highlight_follows_its_paragraph() {
    let c = sequence_course(
        "by_paragraph",
        "MMMM\nMMMM",
        serde_json::json!([
            { "text": "MMMM\n", "highlight": "#FFFF00" },
            { "text": "MMMM", "highlight": "#FFFF00" }
        ]),
    );
    let end = player_at(&c, 2500);
    let (_, ey0, _, ey1) = yellow_box(&end).expect("both paragraphs highlighted at rest");
    let mid = (ey0 + ey1) / 2;
    let half = player_at(&c, 480);
    let (_, hy0, _, hy1) = yellow_box(&half).expect("the first paragraph's highlight shows");
    assert!(hy0 <= ey0 + 2);
    assert!(
        hy1 <= mid + 1 && ey1 > hy1 + 20,
        "the second paragraph's highlight is not there yet: {hy1} {mid} {ey1}"
    );
}

#[test]
fn by_letter_highlight_grows_with_the_visible_letters() {
    let c = sequence_course(
        "by_letter",
        "MMMMMMMM",
        serde_json::json!([{ "text": "MMMMMMMM", "highlight": "#FFFF00" }]),
    );
    let end = player_at(&c, 2500);
    let (ex0, _, ex1, _) = yellow_box(&end).expect("highlight at rest");
    let early = player_at(&c, 150);
    match yellow_box(&early) {
        None => {}
        Some((x0, _, x1, _)) => {
            assert!(x0.abs_diff(ex0) < 4, "starts at the first letter");
            assert!(
                x1 + 20 < ex1,
                "only the first letters have theirs: {x1} {ex1}"
            );
        }
    }
    let later = player_at(&c, 600);
    let (_, _, lx1, _) = yellow_box(&later).expect("more letters in");
    assert!(lx1 > ex0 + 40);
}

#[test]
fn by_letter_shadow_follows_its_letters() {
    let c = sequence_course(
        "by_letter",
        "MMMMMMMM",
        serde_json::json!([{ "text": "MMMMMMMM", "shadow": true }]),
    );
    let early = count_non_white(&player_at(&c, 150));
    let end = count_non_white(&player_at(&c, 2500));
    assert!(
        end > early + 100,
        "more letters, more shadow: {early} {end}"
    );
}

// --- %variables% inside a by-letter or by-paragraph sequence ----------------------------------

fn with_variable(c: Course, name: &str, value: &str) -> Course {
    let mut json: serde_json::Value = serde_json::from_str(&c.to_json().expect("json")).expect("v");
    json["variables"] = serde_json::json!([{ "name": name, "value": value }]);
    Course::from_json(&json.to_string()).expect("course")
}

#[test]
fn sequenced_text_fills_in_variables_before_it_splits() {
    for sequence in ["by_letter", "by_paragraph"] {
        let literal = sequence_course(sequence, "MMMM WWWW", serde_json::json!([]));
        let token = with_variable(
            sequence_course(sequence, "%who%", serde_json::json!([])),
            "who",
            "MMMM WWWW",
        );
        let want = count_non_white(&player_at(&literal, 2500));
        let got = count_non_white(&player_at(&token, 2500));
        assert!(want > 100, "text is drawn");
        assert!(
            got.abs_diff(want) < want / 20,
            "{sequence}: the value shows, not the raw token: {want} vs {got}"
        );
        // Partway through, the filled-in text is still being revealed.
        let early = count_non_white(&player_at(&token, 150));
        assert!(early < got, "{sequence}: still revealing: {early} < {got}");
    }
}

// --- Mesh pass: scale, rotation and clip -------------------------------------------------

fn render_with(c: &Course, sample: &ObjectSample) -> RgbaImage {
    let slide = c.slide("slide1").expect("slide");
    let mut items = compose_authoring(c, slide, None);
    let at = items
        .iter()
        .position(|i| i.hit == Hit::Object("t".into()))
        .expect("text item");
    apply_sample_to_items(&mut items[at..=at], sample);
    render_items_rgba(c, slide, 1280, items, &NoMedia, None).expect("render")
}

fn highlighted() -> Course {
    course_with_text(
        "MMMMMM",
        serde_json::json!([{ "text": "MMMMMM", "highlight": "#FFFF00", "shadow": true }]),
        serde_json::json!({}),
    )
}

#[test]
fn the_mesh_pass_scales_the_highlight_with_the_text() {
    let c = highlighted();
    let rest = render_with(&c, &ObjectSample::IDENTITY);
    let (rx0, _, rx1, _) = yellow_box(&rest).expect("highlight at rest");
    let half = render_with(
        &c,
        &ObjectSample {
            scale: (0.5, 1.0),
            ..ObjectSample::IDENTITY
        },
    );
    let (hx0, _, hx1, _) = yellow_box(&half).expect("highlight when scaled");
    let rest_w = f64::from(rx1 - rx0);
    let half_w = f64::from(hx1 - hx0);
    assert!(
        (half_w - rest_w * 0.5).abs() < 6.0,
        "width halves: {rest_w} -> {half_w}"
    );
}

#[test]
fn the_mesh_pass_turns_the_highlight_with_the_text() {
    let c = highlighted();
    let rest = render_with(&c, &ObjectSample::IDENTITY);
    let (rx0, ry0, rx1, ry1) = yellow_box(&rest).expect("highlight at rest");
    assert!(rx1 - rx0 > (ry1 - ry0) * 2, "a wide band at rest");
    let turned = render_with(
        &c,
        &ObjectSample {
            rotation_deg: 90.0,
            ..ObjectSample::IDENTITY
        },
    );
    let (tx0, ty0, tx1, ty1) = yellow_box(&turned).expect("highlight when turned");
    assert!(
        ty1 - ty0 > (tx1 - tx0) * 2,
        "a tall band after a quarter turn: {tx0},{ty0} {tx1},{ty1}"
    );
}

#[test]
fn the_mesh_pass_clips_the_highlight_and_the_shadow() {
    let c = highlighted();
    let rest = render_with(&c, &ObjectSample::IDENTITY);
    let (rx0, _, rx1, _) = yellow_box(&rest).expect("highlight at rest");
    let middle = f64::from(rx0 + rx1) / 2.0;
    // Frame is x 100..900: clip to its left half (100..500).
    let clipped = render_with(
        &c,
        &ObjectSample {
            mask: Some(Mask::new(vec![ConvexPoly::rect(0.0, 0.0, 0.5, 1.0)])),
            ..ObjectSample::IDENTITY
        },
    );
    let (_, _, cx1, _) = yellow_box(&clipped).expect("the left part still shows");
    assert!(
        f64::from(cx1) <= 500.5,
        "nothing right of the clip edge: {cx1} (rest middle {middle})"
    );
    // The right half of the frame holds no ink at all: neither highlight nor shadow.
    let ink_right = (100..300)
        .flat_map(|y| (505..900).map(move |x| (x, y)))
        .filter(|&(x, y)| px(&clipped, x, y)[2] < 250)
        .count();
    assert_eq!(ink_right, 0);
}
