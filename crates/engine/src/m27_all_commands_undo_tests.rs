//! M27 Step D-4 (D5.1): Exhaustive undo/redo round-trip sweep across every
//! `mutates == true` command in `REGISTRY`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashSet;

use coursecraft_course::{
    Action, BackgroundAudio, BankDraw, Choice, Color, Course, Cue, DragItem, Event, FontFace,
    FontStyle, GlossaryEntry, Group, Guide, HotspotRegion, HotspotShape, Layer, MatchPair,
    MotionPath, NamedTextStyle, Object, ObjectKind, Outline, PaletteColor, Path, PathPoint, Point,
    Question, QuestionBank, QuestionKind, Rect, Resource, ResourceTarget, Scene, Scoring, Severity,
    ShapeKind, Slide, SlideCue, StateDef, StateExtra, StateExtraKind, TableBorders, TableCell,
    TableRow, TextStyle, Timing, Trigger, Value, VariableDef,
};
use serde_json::{Value as Json, json};

use crate::{Session, registry};

const TRANSLATION_XLIFF: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<xliff version="1.2" xmlns="urn:oasis:names:tc:xliff:document:1.2">
  <file original="course.json" datatype="plaintext" source-language="en" target-language="fr">
    <body>
      <trans-unit id="course/title" xml:space="preserve"><source>M27 Sweep</source><target>Balayage M27</target></trans-unit>
      <trans-unit id="slide/slide1/title" xml:space="preserve"><source>Slide 1</source><target>Diapositive 1</target></trans-unit>
    </body>
  </file>
</xliff>"#;

const CSV_QUESTIONS: &str = include_str!("../../../examples/questions.csv");

fn base_object(id: &str, kind: ObjectKind, frame: Rect) -> Object {
    Object {
        id: id.into(),
        name: id.into(),
        kind,
        frame,
        timing: Timing::default(),
        states: Vec::new(),
        initial_state: "normal".into(),
        entrance: None,
        exit: None,
        motion_paths: Vec::new(),
        emphasis: Vec::new(),
        focus_order: None,
        decorative: false,
        description: None,
        group: None,
        panel: None,
        rotation: 0.0,
        flip_h: false,
        flip_v: false,
        opacity: 1.0,
        shadow: None,
        glow: None,
        drag: None,
        locked: false,
        editor_hidden: false,
    }
}

fn make_text_obj(id: &str, text: &str, x: f32, y: f32) -> Object {
    base_object(
        id,
        ObjectKind::Text {
            text: text.into(),
            style: TextStyle::default(),
            runs: Vec::new(),
            paragraphs: Vec::new(),
            autofit: Default::default(),
            text_style: None,
            outline: None,
            point_text: false,
        },
        Rect {
            x,
            y,
            w: 200.0,
            h: 50.0,
        },
    )
}

fn plain_run(text: &str) -> coursecraft_course::TextRun {
    coursecraft_course::TextRun {
        text: text.into(),
        bold: None,
        italic: None,
        underline: None,
        strikethrough: None,
        baseline: None,
        letter_spacing: None,
        size: None,
        color: None,
        link: None,
        font: None,
        highlight: None,
        shadow: None,
    }
}

fn make_shape_obj(id: &str, x: f32, y: f32, w: f32, h: f32, fill: &str) -> Object {
    base_object(
        id,
        ObjectKind::Shape {
            shape: coursecraft_course::ShapeKind::Rectangle,
            fill: Color(fill.into()),
            fill_style: None,
            outline: None,
            text: None,
            adjust: Vec::new(),
        },
        Rect { x, y, w, h },
    )
}

fn make_empty_slide(id: &str, title: &str) -> Slide {
    Slide {
        id: id.into(),
        title: title.into(),
        duration_ms: 5000,
        background: None,
        objects: Vec::new(),
        layers: Vec::new(),
        groups: Vec::new(),
        triggers: Vec::new(),
        cue_points: Vec::new(),
        transition: None,
        question: None,
        draw: None,
        notes: String::new(),
        master: None,
        revisit: coursecraft_course::RevisitMode::Auto,
        player: None,
    }
}

fn make_empty_question(kind: QuestionKind, prompt: &str) -> Question {
    Question {
        kind,
        prompt: prompt.into(),
        choices: Vec::new(),
        points: 10,
        scoring: Scoring::default(),
        attempts: 1,
        shuffle: false,
        correct_layer: None,
        incorrect_layer: None,
        try_again_layer: None,
        accepted: Vec::new(),
        case_sensitive: false,
        pairs: Vec::new(),
        hotspot_image: None,
        regions: Vec::new(),
        markers: 1,
        drag_items: Vec::new(),
        drop_targets: Vec::new(),
        snap_to_center: true,
        return_if_wrong: false,
        one_per_target: true,
        correct_when: None,
        frame: None,
    }
}

fn make_table_obj(id: &str) -> Object {
    let cell = |t: &str| TableCell {
        text: t.into(),
        style: TextStyle::default(),
        runs: Vec::new(),
        paragraphs: Vec::new(),
        fill: None,
        valign: Default::default(),
        col_span: 1,
        row_span: 1,
    };
    let mut r0c0 = cell("H0");
    r0c0.col_span = 2;
    let r0c1 = cell("");
    base_object(
        id,
        ObjectKind::Table {
            columns: vec![1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0],
            rows: vec![
                TableRow {
                    height: 1.0 / 3.0,
                    cells: vec![r0c0, r0c1, cell("H2")],
                },
                TableRow {
                    height: 1.0 / 3.0,
                    cells: vec![cell("A1"), cell("B1"), cell("C1")],
                },
                TableRow {
                    height: 1.0 / 3.0,
                    cells: vec![cell("A2"), cell("B2"), cell("C2")],
                },
            ],
            borders: TableBorders::default(),
            header_row: false,
        },
        Rect {
            x: 50.0,
            y: 300.0,
            w: 300.0,
            h: 120.0,
        },
    )
}

/// Builds a valid fixture course containing every object kind, layer, master,
/// template, bank, draw, question kind, table, path, video cue, variable,
/// resource, glossary, font, text style, palette, and guide.
fn fixture_session() -> Session {
    let mut s = Session::blank("M27 Sweep");
    let c = &mut s.course;

    c.language = Some("en".into());
    if let Some(slide) = c
        .scenes
        .first_mut()
        .and_then(|scene| scene.slides.first_mut())
    {
        slide.player = Some(coursecraft_course::SlidePlayerOverrides {
            show_menu: Some(false),
            ..Default::default()
        });
    }
    c.background_audio = Some(BackgroundAudio {
        src: "audio/bg.mp3".into(),
        volume: 0.5,
        looping: true,
    });
    c.theme.palette = vec![
        PaletteColor {
            id: "pal1".into(),
            name: "Primary".into(),
            color: Color("#112233".into()),
        },
        PaletteColor {
            id: "pal2".into(),
            name: "Secondary".into(),
            color: Color("#445566".into()),
        },
    ];
    c.text_styles = vec![NamedTextStyle {
        id: "tstyle1".into(),
        name: "Heading".into(),
        style: TextStyle {
            size: 24.0,
            bold: true,
            color: Color("#112233".into()),
            ..TextStyle::default()
        },
        paragraphs: None,
    }];
    c.fonts = vec![FontFace {
        src: "fonts/custom.ttf".into(),
        family: "CustomFont".into(),
        style: FontStyle::Regular,
    }];
    c.variables = vec![
        VariableDef {
            name: "var1".into(),
            value: Value::Number(1.0),
        },
        VariableDef {
            name: "var_text".into(),
            value: Value::Text(String::new()),
        },
        VariableDef {
            name: "var_unused".into(),
            value: Value::Bool(false),
        },
    ];
    c.resources = vec![
        Resource {
            id: "res1".into(),
            title: "Guide".into(),
            target: ResourceTarget::Url {
                url: "https://example.com/1".into(),
            },
        },
        Resource {
            id: "res2".into(),
            title: "Ref".into(),
            target: ResourceTarget::Url {
                url: "https://example.com/2".into(),
            },
        },
    ];
    c.glossary = vec![
        GlossaryEntry {
            id: "gloss1".into(),
            term: "SCORM".into(),
            definition: "Standard".into(),
        },
        GlossaryEntry {
            id: "gloss2".into(),
            term: "xAPI".into(),
            definition: "Experience API".into(),
        },
        GlossaryEntry {
            id: "gloss3".into(),
            term: "AICC".into(),
            definition: "Aviation Industry CBT Committee".into(),
        },
    ];
    c.editor.guides = vec![
        Guide {
            vertical: true,
            at: 100.0,
        },
        Guide {
            vertical: false,
            at: 200.0,
        },
    ];
    c.masters = vec![make_empty_slide("master1", "Master 1")];
    c.templates = vec![make_empty_slide("tpl1", "Template 1")];

    let mut bank_q = make_empty_slide("bank_q1", "Bank Question 1");
    let mut bq = make_empty_question(QuestionKind::TrueFalse, "Is Rust safe?");
    bq.choices = vec![
        Choice {
            id: "t".into(),
            text: "True".into(),
            correct: true,
            points: 0,
        },
        Choice {
            id: "f".into(),
            text: "False".into(),
            correct: false,
            points: 0,
        },
    ];
    bank_q.question = Some(bq);
    let mut bank_q2 = make_empty_slide("bank_q2", "Bank Question 2");
    bank_q2.question = bank_q.question.clone();
    let mut bank_u1 = make_empty_slide("bank_u1", "Unused Bank Question");
    bank_u1.question = bank_q.question.clone();
    c.banks = vec![
        QuestionBank {
            id: "bank1".into(),
            title: "Bank 1".into(),
            slides: vec![bank_q, bank_q2],
        },
        QuestionBank {
            id: "bank_unused".into(),
            title: "Unused Bank".into(),
            slides: vec![bank_u1],
        },
    ];

    // Populate slide1 with all object kinds, layers, cues, triggers, states, motion paths.
    let mut obj_shape = make_shape_obj("obj_shape", 40.0, 40.0, 120.0, 80.0, "#112233");
    obj_shape.states = vec![
        StateDef {
            name: "custom1".into(),
            fill: Some(Color("#223344".into())),
            extras: vec![
                StateExtra {
                    id: "extra1".into(),
                    name: "Badge".into(),
                    kind: StateExtraKind::Shape {
                        shape: ShapeKind::Ellipse,
                        fill: Color("#22AA44".into()),
                        fill_style: None,
                        outline: None,
                        text: None,
                        adjust: vec![],
                    },
                    frame: Rect {
                        x: -6.0,
                        y: -6.0,
                        w: 16.0,
                        h: 16.0,
                    },
                    rotation: 0.0,
                    flip_h: false,
                    flip_v: false,
                    opacity: 1.0,
                    shadow: None,
                    glow: None,
                },
                StateExtra {
                    id: "extra2".into(),
                    name: "Tag".into(),
                    kind: StateExtraKind::Text {
                        text: "New".into(),
                        style: Default::default(),
                        runs: vec![],
                        paragraphs: vec![],
                        autofit: Default::default(),
                        outline: None,
                        point_text: false,
                    },
                    frame: Rect {
                        x: 0.0,
                        y: 12.0,
                        w: 40.0,
                        h: 18.0,
                    },
                    rotation: 0.0,
                    flip_h: false,
                    flip_v: false,
                    opacity: 1.0,
                    shadow: None,
                    glow: None,
                },
            ],
            ..Default::default()
        },
        StateDef {
            name: "custom2".into(),
            fill: Some(Color("#334455".into())),
            ..Default::default()
        },
    ];
    obj_shape.motion_paths = vec![MotionPath {
        id: "mpath1".into(),
        points: vec![Point { x: 0.0, y: 0.0 }, Point { x: 100.0, y: 0.0 }],
        duration_ms: 1000,
        easing: Default::default(),
        relative: true,
    }];
    obj_shape.emphasis = vec![coursecraft_course::Emphasis::new("pulse", 0)];

    let obj_shape2 = make_shape_obj("obj_shape2", 200.0, 40.0, 100.0, 60.0, "#445566");
    let obj_shape3 = make_shape_obj("obj_shape3", 360.0, 40.0, 100.0, 60.0, "#445566");

    let mut obj_g1 = make_shape_obj("obj_g1", 500.0, 40.0, 60.0, 60.0, "#AABBCC");
    obj_g1.group = Some("grp1".into());
    let mut obj_g2 = make_shape_obj("obj_g2", 580.0, 40.0, 60.0, 60.0, "#AABBCC");
    obj_g2.group = Some("grp1".into());

    let mut obj_ig1 = make_shape_obj("obj_ig1", 500.0, 120.0, 60.0, 40.0, "#1D4ED8");
    obj_ig1.group = Some("grp_btn".into());
    let mut obj_ig2 = make_text_obj("obj_ig2", "Go", 505.0, 125.0);
    obj_ig2.group = Some("grp_btn".into());

    let mut obj_text = make_text_obj("obj_text", "Hello world", 40.0, 140.0);
    // An italic run, so text.clear_formatting has something to clear.
    if let ObjectKind::Text { runs, .. } = &mut obj_text.kind {
        runs.push(coursecraft_course::TextRun {
            italic: Some(true),
            ..plain_run("Hello world")
        });
    }

    let obj_pic = base_object(
        "obj_pic",
        ObjectKind::Image {
            src: "media/pic.png".into(),
            alt: "Pic".into(),
            crop: None,
            outline: None,
            shape: None,
        },
        Rect {
            x: 40.0,
            y: 210.0,
            w: 120.0,
            h: 80.0,
        },
    );

    let obj_video = base_object(
        "obj_video",
        ObjectKind::Video {
            src: "media/clip.mp4".into(),
            autoplay: false,
            captions: None,
            volume: 1.0,
            trim: None,
            cues: vec![Cue {
                id: "vcue1".into(),
                at_ms: 1000.0,
                name: "Cue 1".into(),
                pause: false,
            }],
            poster: None,
        },
        Rect {
            x: 200.0,
            y: 210.0,
            w: 160.0,
            h: 90.0,
        },
    );

    let obj_lottie = base_object(
        "obj_lottie",
        ObjectKind::Lottie {
            src: "media/anim.json".into(),
            loop_playback: true,
            autoplay: true,
            speed: 1.0,
        },
        Rect {
            x: 400.0,
            y: 210.0,
            w: 100.0,
            h: 100.0,
        },
    );

    let obj_web = base_object(
        "obj_web",
        ObjectKind::Web {
            source: coursecraft_course::WebSource::Url {
                url: "https://example.com".into(),
            },
        },
        Rect {
            x: 520.0,
            y: 210.0,
            w: 120.0,
            h: 90.0,
        },
    );

    let obj_input = base_object(
        "obj_input",
        ObjectKind::TextEntry {
            variable: "var_text".into(),
            placeholder: "Type".into(),
            multiline: false,
            max_chars: 0,
        },
        Rect {
            x: 660.0,
            y: 210.0,
            w: 140.0,
            h: 40.0,
        },
    );

    let obj_marker = base_object(
        "obj_marker",
        ObjectKind::Marker {
            title: "Info".into(),
            body: "Body".into(),
            runs: Vec::new(),
            paragraphs: Vec::new(),
            icon: Default::default(),
            color: Some(Color("#112233".into())),
        },
        Rect {
            x: 660.0,
            y: 270.0,
            w: 32.0,
            h: 32.0,
        },
    );

    let obj_scroll = base_object(
        "obj_scroll",
        ObjectKind::ScrollPanel {
            content_height: 400.0,
            bar: Default::default(),
        },
        Rect {
            x: 720.0,
            y: 270.0,
            w: 150.0,
            h: 150.0,
        },
    );

    let obj_timer = base_object(
        "obj_timer",
        ObjectKind::Timer {
            duration_ms: 10_000,
            format: Default::default(),
            autostart: false,
            style: TextStyle::default(),
        },
        Rect {
            x: 40.0,
            y: 440.0,
            w: 100.0,
            h: 40.0,
        },
    );

    let obj_meter = base_object(
        "obj_meter",
        ObjectKind::ProgressMeter {
            variable: "var1".into(),
            min: 0.0,
            max: 100.0,
            style: Default::default(),
            show_value: true,
        },
        Rect {
            x: 160.0,
            y: 440.0,
            w: 160.0,
            h: 24.0,
        },
    );

    let obj_path = base_object(
        "obj_path",
        ObjectKind::Path {
            path: Path {
                points: vec![PathPoint { x: 0.0, y: 0.0 }, PathPoint { x: 1.0, y: 1.0 }],
                style: Default::default(),
                closed: false,
                subpaths: Vec::new(),
                fill: None,
                fill_style: None,
                outline: Outline {
                    color: Color("#112233".into()),
                    width: 2.0,
                    dash: Default::default(),
                    dash_pattern: None,
                    join: Default::default(),
                    miter_limit: None,
                },
                start_end: Default::default(),
                end_end: Default::default(),
                connector: None,
                start_glue: None,
                end_glue: None,
                text: None,
            },
        },
        Rect {
            x: 340.0,
            y: 440.0,
            w: 100.0,
            h: 50.0,
        },
    );

    let obj_table = make_table_obj("obj_table");

    let s1 = &mut c.scenes[0].slides[0];
    s1.objects = vec![
        obj_shape, obj_shape2, obj_shape3, obj_g1, obj_g2, obj_text, obj_pic, obj_video,
        obj_lottie, obj_web, obj_input, obj_marker, obj_scroll, obj_timer, obj_meter, obj_path,
        obj_table, obj_ig1, obj_ig2,
    ];
    s1.groups = vec![Group {
        id: "grp_btn".into(),
        name: "Go".into(),
        interactive: true,
        initial_state: "normal".into(),
        focus_order: None,
        description: None,
        entrance: None,
        exit: None,
        emphasis: Vec::new(),
    }];
    s1.layers = vec![
        Layer {
            id: "layer1".into(),
            name: "Layer 1".into(),
            duration_ms: 3000,
            hide_other_layers: true,
            pause_base_timeline: false,
            prevent_base_clicks: false,
            hide_base_objects: false,
            hide_when_timeline_ends: false,
            allow_seek: Default::default(),
            revisit: Default::default(),
            objects: Vec::new(),
            triggers: Vec::new(),
            cue_points: Vec::new(),
            groups: Vec::new(),
        },
        Layer {
            id: "layer2".into(),
            name: "Layer 2".into(),
            duration_ms: 3000,
            hide_other_layers: true,
            pause_base_timeline: false,
            prevent_base_clicks: false,
            hide_base_objects: false,
            hide_when_timeline_ends: false,
            allow_seek: Default::default(),
            revisit: Default::default(),
            objects: Vec::new(),
            triggers: Vec::new(),
            cue_points: Vec::new(),
            groups: Vec::new(),
        },
    ];
    s1.cue_points = vec![SlideCue {
        id: "scue1".into(),
        name: "Slide Cue 1".into(),
        at_ms: 1500,
    }];
    s1.triggers = vec![
        Trigger {
            when: Event::Click {
                object: "obj_shape".into(),
            },
            condition: None,
            action: Action::ShowLayer {
                layer: "layer1".into(),
            },
            disabled: false,
        },
        Trigger {
            when: Event::Click {
                object: "obj_shape2".into(),
            },
            condition: None,
            action: Action::ShowLayer {
                layer: "layer2".into(),
            },
            disabled: false,
        },
    ];

    // Additional slides in scene1 for slide.*, question.*, draw.*
    let slide2 = make_empty_slide("slide2", "Slide 2");

    let mut slide_q_mc = make_empty_slide("slide_q_mc", "MC Question");
    let mut q_mc = make_empty_question(QuestionKind::MultipleChoice, "Pick A");
    q_mc.choices = vec![
        Choice {
            id: "c1".into(),
            text: "A".into(),
            correct: true,
            points: 0,
        },
        Choice {
            id: "c2".into(),
            text: "B".into(),
            correct: false,
            points: 0,
        },
    ];
    slide_q_mc.question = Some(q_mc);

    let mut slide_q_fib = make_empty_slide("slide_q_fib", "Fill In Blank");
    let mut q_fib = make_empty_question(QuestionKind::FillIn, "Language?");
    q_fib.accepted = vec!["rust".into(), "Rust".into()];
    slide_q_fib.question = Some(q_fib);

    let mut slide_q_match = make_empty_slide("slide_q_match", "Matching");
    let mut q_match = make_empty_question(QuestionKind::Matching, "Match");
    q_match.pairs = vec![
        MatchPair {
            id: "p1".into(),
            left: "L1".into(),
            right: "R1".into(),
        },
        MatchPair {
            id: "p2".into(),
            left: "L2".into(),
            right: "R2".into(),
        },
        MatchPair {
            id: "p3".into(),
            left: "L3".into(),
            right: "R3".into(),
        },
    ];
    slide_q_match.question = Some(q_match);

    let mut slide_q_seq = make_empty_slide("slide_q_seq", "Sequence");
    let mut q_seq = make_empty_question(QuestionKind::Sequence, "Order");
    q_seq.choices = vec![
        Choice {
            id: "sq1".into(),
            text: "First".into(),
            correct: true,
            points: 0,
        },
        Choice {
            id: "sq2".into(),
            text: "Second".into(),
            correct: true,
            points: 0,
        },
        Choice {
            id: "sq3".into(),
            text: "Third".into(),
            correct: true,
            points: 0,
        },
    ];
    slide_q_seq.question = Some(q_seq);

    let mut slide_q_hotspot = make_empty_slide("slide_q_hotspot", "Hotspot");
    slide_q_hotspot.objects = vec![
        base_object(
            "hs_img",
            ObjectKind::Image {
                src: "media/hotspot.png".into(),
                alt: "Hotspot".into(),
                crop: None,
                outline: None,
                shape: None,
            },
            Rect {
                x: 0.0,
                y: 0.0,
                w: 400.0,
                h: 300.0,
            },
        ),
        base_object(
            "hs_img2",
            ObjectKind::Image {
                src: "media/hotspot2.png".into(),
                alt: "Hotspot 2".into(),
                crop: None,
                outline: None,
                shape: None,
            },
            Rect {
                x: 0.0,
                y: 0.0,
                w: 400.0,
                h: 300.0,
            },
        ),
    ];
    let mut q_hotspot = make_empty_question(QuestionKind::Hotspot, "Click target");
    q_hotspot.hotspot_image = Some("hs_img".into());
    q_hotspot.regions = vec![
        HotspotRegion {
            id: "hr1".into(),
            shape: HotspotShape::Rect,
            x: 10.0,
            y: 10.0,
            w: 100.0,
            h: 100.0,
            correct: true,
        },
        HotspotRegion {
            id: "hr2".into(),
            shape: HotspotShape::Ellipse,
            x: 200.0,
            y: 10.0,
            w: 100.0,
            h: 100.0,
            correct: false,
        },
    ];
    slide_q_hotspot.question = Some(q_hotspot);

    let mut slide_q_dnd = make_empty_slide("slide_q_dnd", "Drag and Drop");
    slide_q_dnd.objects = vec![
        make_shape_obj("dnd_item1", 40.0, 40.0, 80.0, 40.0, "#112233"),
        make_shape_obj("dnd_item2", 40.0, 100.0, 80.0, 40.0, "#112233"),
        make_shape_obj("dnd_item3", 40.0, 160.0, 80.0, 40.0, "#112233"),
        make_shape_obj("dnd_target1", 300.0, 40.0, 120.0, 60.0, "#445566"),
        make_shape_obj("dnd_target2", 300.0, 120.0, 120.0, 60.0, "#445566"),
        make_shape_obj("dnd_target3", 300.0, 200.0, 120.0, 60.0, "#445566"),
    ];
    let mut q_dnd = make_empty_question(QuestionKind::DragDrop, "Drag");
    q_dnd.drag_items = vec![
        DragItem {
            item: "dnd_item1".into(),
            target: "dnd_target1".into(),
        },
        DragItem {
            item: "dnd_item2".into(),
            target: "dnd_target2".into(),
        },
    ];
    q_dnd.drop_targets = vec!["dnd_target1".into(), "dnd_target2".into()];
    slide_q_dnd.question = Some(q_dnd);

    let mut slide_draw = make_empty_slide("slide_draw", "Bank Draw");
    slide_draw.draw = Some(BankDraw {
        bank: "bank1".into(),
        count: 1,
    });

    c.scenes[0].slides.extend([
        slide2,
        slide_q_mc,
        slide_q_fib,
        slide_q_match,
        slide_q_seq,
        slide_q_hotspot,
        slide_q_dnd,
        slide_draw,
    ]);

    c.scenes.push(Scene {
        id: "scene2".into(),
        title: "Scene 2".into(),
        slides: vec![make_empty_slide("scene2_s1", "Scene 2 Slide 1")],
    });

    s
}

fn validation_errors(course: &Course) -> Vec<String> {
    course
        .validate()
        .into_iter()
        .filter(|i| i.severity == Severity::Error)
        .map(|i| format!("{}: {}", i.path, i.message))
        .collect()
}

fn command_cases() -> Vec<(&'static str, Json)> {
    let pasted_obj = serde_json::to_value(make_shape_obj(
        "pasted_src",
        20.0,
        20.0,
        80.0,
        40.0,
        "#112233",
    ))
    .unwrap();
    let pasted_slide = serde_json::to_value(make_empty_slide("pasted_slide", "Pasted")).unwrap();

    vec![
        ("course.update", json!({ "title": "Updated Title" })),
        (
            "course.set_slide_size",
            json!({ "w": 1920, "h": 1080, "mode": "scale" }),
        ),
        ("course.set_title", json!({ "title": "New Title" })),
        (
            "course.set_player",
            json!({ "show_nav": false, "navigation": "restricted" }),
        ),
        ("course.clear_slide_player_overrides", json!({})),
        ("course.set_quiz", json!({ "passing_percent": 75 })),
        (
            "course.set_publish_settings",
            json!({ "lms_status": "passed_failed" }),
        ),
        (
            "course.set_tracking",
            json!({ "slides_viewed": { "mode": "at_least", "count": 2 }, "quiz": { "slide": null } }),
        ),
        (
            "course.set_background_audio",
            json!({ "src": "audio/new.mp3", "volume": 0.25, "loop": false }),
        ),
        (
            "theme.set_palette",
            json!({ "colors": [{ "id": "pal1", "name": "Brand", "color": "#ABCDEF" }] }),
        ),
        (
            "theme.set_palette_color",
            json!({ "id": "pal1", "color": "#FF00AA", "recolor": true }),
        ),
        (
            "course.recolor",
            json!({ "from": "#112233", "to": "#998877" }),
        ),
        (
            "text_style.create",
            json!({ "name": "Body Style", "style": { "size": 16.0 } }),
        ),
        (
            "text_style.update",
            json!({ "id": "tstyle1", "name": "Subheading", "style": { "size": 20.0 } }),
        ),
        ("text_style.delete", json!({ "id": "tstyle1" })),
        (
            "text_style.apply",
            json!({ "id": "tstyle1", "slide": "slide1", "objects": ["obj_text"] }),
        ),
        ("course.add_finish_button", json!({})),
        (
            "course.set_theme",
            json!({ "accent": "#FF5500", "font": "Georgia" }),
        ),
        (
            "course.add_font",
            json!({ "src": "fonts/bold.ttf", "family": "CustomFont", "style": "bold" }),
        ),
        (
            "course.remove_font",
            json!({ "family": "CustomFont", "style": "regular" }),
        ),
        ("course.import_xliff", json!({ "xliff": TRANSLATION_XLIFF })),
        ("scene.add", json!({ "title": "New Scene" })),
        (
            "scene.update",
            json!({ "scene": "scene2", "title": "Renamed Scene 2" }),
        ),
        ("scene.delete", json!({ "scene": "scene2" })),
        (
            "slide.add",
            json!({ "scene": "scene1", "after": "slide1", "title": "Inserted Slide" }),
        ),
        (
            "slide.update",
            json!({ "slide": "slide1", "title": "Updated Slide 1", "notes": "Note" }),
        ),
        ("slide.delete", json!({ "slide": "slide2" })),
        (
            "slide.move",
            json!({ "slide": "slide2", "scene": "scene2", "index": 0 }),
        ),
        (
            "slide.duplicate",
            json!({ "slide": "slide1", "after": "slide1" }),
        ),
        (
            "slide.paste",
            json!({ "scene": "scene1", "after": "slide1", "slide": pasted_slide }),
        ),
        (
            "slide.set_duration",
            json!({ "slide": "slide1", "ms": 8000 }),
        ),
        (
            "slide.set_background",
            json!({ "slide": "slide1", "background": "#EFEFEF" }),
        ),
        (
            "slide.set_transition",
            json!({ "slide": "slide1", "transition": { "type": "fade", "duration_ms": 400 } }),
        ),
        (
            "slide.set_transition_all",
            json!({ "transition": { "type": "fade", "duration_ms": 400 } }),
        ),
        (
            "slide.set_player_overrides",
            json!({ "slide": "slide1", "show_nav": false }),
        ),
        ("master.add", json!({ "title": "Master 2" })),
        ("master.delete", json!({ "master": "master1" })),
        (
            "slide.set_master",
            json!({ "slides": ["slide1"], "master": "master1" }),
        ),
        ("bank.create", json!({ "title": "Bank 2" })),
        (
            "bank.update",
            json!({ "bank": "bank1", "title": "Renamed Bank" }),
        ),
        ("bank.delete", json!({ "bank": "bank_unused" })),
        (
            "bank.add_question",
            json!({ "bank": "bank1", "title": "Extra Q" }),
        ),
        (
            "bank.import_csv",
            json!({ "csv": CSV_QUESTIONS, "bank": "CSV Bank" }),
        ),
        (
            "bank.move_slide",
            json!({ "slide": "slide_q_mc", "bank": "bank1", "index": 0 }),
        ),
        ("bank.remove_slide", json!({ "slide": "bank_q2" })),
        (
            "draw.insert",
            json!({ "bank": "bank1", "count": 1, "scene": "scene1" }),
        ),
        ("draw.update", json!({ "slide": "slide_draw", "count": 2 })),
        (
            "slide.add_from_layout",
            json!({ "layout": "title_content", "after": "slide1", "master": "master1" }),
        ),
        (
            "course.import_outline",
            json!({
                "outline": {
                    "scenes": [{
                        "title": "Outline Scene",
                        "slides": [{ "title": "Outline Slide", "body": ["Point A"] }]
                    }]
                }
            }),
        ),
        (
            "template.save",
            json!({ "slide": "slide1", "title": "Saved Template" }),
        ),
        (
            "template.add_slide",
            json!({ "template": "tpl1", "after": "slide1" }),
        ),
        ("template.delete", json!({ "template": "tpl1" })),
        ("layer.add", json!({ "slide": "slide1", "name": "Layer 3" })),
        (
            "layer.update",
            json!({ "slide": "slide1", "layer": "layer1", "name": "Feedback", "duration_ms": 4000 }),
        ),
        (
            "layer.delete",
            json!({ "slide": "slide1", "layer": "layer2" }),
        ),
        (
            "layer.move",
            json!({ "slide": "slide1", "layer": "layer2", "to": 0 }),
        ),
        (
            "layer.duplicate",
            json!({ "slide": "slide1", "layer": "layer1" }),
        ),
        (
            "object.add",
            json!({
                "slide": "slide1",
                "kind": { "type": "shape", "shape": "ellipse", "fill": "#123456" },
                "frame": { "x": 10.0, "y": 10.0, "w": 80.0, "h": 80.0 }
            }),
        ),
        (
            "object.update",
            json!({ "slide": "slide1", "object": "obj_shape", "name": "Renamed Shape" }),
        ),
        (
            "object.reorder",
            json!({ "slide": "slide1", "object": "obj_shape", "index": 2 }),
        ),
        (
            "object.align",
            json!({ "slide": "slide1", "objects": ["obj_shape", "obj_shape2"], "edge": "bottom" }),
        ),
        (
            "object.distribute",
            json!({ "slide": "slide1", "objects": ["obj_shape", "obj_shape2", "obj_shape3"], "axis": "horizontal" }),
        ),
        (
            "object.match_size",
            json!({ "slide": "slide1", "objects": ["obj_shape", "obj_shape2"], "dimension": "both" }),
        ),
        (
            "object.group",
            json!({ "slide": "slide1", "objects": ["obj_shape", "obj_shape2"] }),
        ),
        (
            "object.ungroup",
            json!({ "slide": "slide1", "objects": ["obj_g1", "obj_g2"] }),
        ),
        (
            "group.set_interactive",
            json!({ "slide": "slide1", "group": "grp1", "interactive": true, "name": "Card button" }),
        ),
        (
            "group.update",
            json!({ "slide": "slide1", "group": "grp_btn", "name": "Start button", "focus_order": 5, "description": "Starts course" }),
        ),
        (
            "object.duplicate",
            json!({ "slide": "slide1", "object": "obj_shape" }),
        ),
        (
            "object.paste",
            json!({ "slide": "slide1", "objects": [pasted_obj] }),
        ),
        (
            "object.set_frame",
            json!({ "slide": "slide1", "object": "obj_shape", "frame": { "x": 99.0, "y": 88.0, "w": 140.0, "h": 90.0 } }),
        ),
        (
            "object.set_rotation",
            json!({ "slide": "slide1", "object": "obj_shape", "degrees": 45.0 }),
        ),
        (
            "object.flip",
            json!({ "slide": "slide1", "object": "obj_shape", "axis": "horizontal" }),
        ),
        (
            "object.lock",
            json!({ "slide": "slide1", "object": "obj_shape", "locked": true }),
        ),
        (
            "object.hide_in_editor",
            json!({ "slide": "slide1", "object": "obj_shape", "hidden": true }),
        ),
        (
            "view.set",
            json!({ "show_grid": true, "snap_to_grid": true }),
        ),
        ("view.guides.add", json!({ "vertical": true, "at": 320.0 })),
        ("view.guides.move", json!({ "index": 0, "at": 150.0 })),
        ("view.guides.remove", json!({ "index": 0 })),
        (
            "object.set_opacity",
            json!({ "slide": "slide1", "object": "obj_shape", "opacity": 0.5 }),
        ),
        (
            "object.set_outline_join",
            json!({ "slide": "slide1", "object": "obj_path", "join": "round" }),
        ),
        (
            "object.set_fill_style",
            json!({ "slide": "slide1", "object": "obj_shape", "fill_style": { "type": "linear", "angle": 90.0, "stops": [{ "position": 0.0, "color": "#FF0000" }, { "position": 1.0, "color": "#0000FF" }] } }),
        ),
        (
            "object.set_shadow",
            json!({ "slide": "slide1", "object": "obj_shape", "shadow": { "offset_x": 4.0, "offset_y": 4.0, "blur": 8.0, "color": "#00000080" } }),
        ),
        (
            "object.set_glow",
            json!({ "slide": "slide1", "object": "obj_shape", "glow": { "size": 6.0, "color": "#FFFF0080" } }),
        ),
        (
            "object.apply_style",
            json!({ "slide": "slide1", "source_slide": "slide1", "source": "obj_shape", "objects": ["obj_shape2"] }),
        ),
        (
            "object.set_drag",
            json!({ "slide": "slide1", "object": "obj_shape", "drag": { "targets": ["obj_shape2"], "snap": true, "return_on_miss": true } }),
        ),
        (
            "object.set_lottie",
            json!({ "slide": "slide1", "object": "obj_lottie", "loop": false, "speed": 1.5 }),
        ),
        (
            "object.set_web_source",
            json!({ "slide": "slide1", "object": "obj_web", "source": { "type": "url", "url": "https://example.org" } }),
        ),
        (
            "object.set_input",
            json!({ "slide": "slide1", "object": "obj_input", "patch": { "placeholder": "Enter value" } }),
        ),
        (
            "object.set_marker",
            json!({ "slide": "slide1", "object": "obj_marker", "patch": { "title": "Updated Marker" } }),
        ),
        (
            "object.set_panel",
            json!({ "slide": "slide1", "object": "obj_shape", "panel": "obj_scroll" }),
        ),
        (
            "object.set_scroll_panel",
            json!({ "slide": "slide1", "object": "obj_scroll", "patch": { "content_height": 600.0 } }),
        ),
        (
            "object.set_timer",
            json!({ "slide": "slide1", "object": "obj_timer", "patch": { "duration_ms": 20000, "autostart": true } }),
        ),
        (
            "object.set_meter",
            json!({ "slide": "slide1", "object": "obj_meter", "patch": { "max": 200.0 } }),
        ),
        (
            "object.set_path",
            json!({ "slide": "slide1", "object": "obj_path", "points": [{ "x": 0.0, "y": 0.0 }, { "x": 0.5, "y": 1.0 }] }),
        ),
        (
            "object.set_crop",
            json!({ "slide": "slide1", "object": "obj_pic", "crop": { "x": 0.1, "y": 0.1, "w": 0.8, "h": 0.8 } }),
        ),
        (
            "object.set_captions",
            json!({ "slide": "slide1", "object": "obj_video", "captions": "media/clip.vtt" }),
        ),
        (
            "object.set_media",
            json!({ "slide": "slide1", "object": "obj_video", "volume": 0.6 }),
        ),
        (
            "video.add_cue",
            json!({ "slide": "slide1", "object": "obj_video", "at_ms": 2500, "name": "Cue 2" }),
        ),
        (
            "video.set_cue",
            json!({ "slide": "slide1", "object": "obj_video", "cue": "vcue1", "patch": { "name": "Renamed Cue" } }),
        ),
        (
            "video.remove_cue",
            json!({ "slide": "slide1", "object": "obj_video", "cue": "vcue1" }),
        ),
        (
            "video.add_question_cue",
            json!({ "slide": "slide1", "object": "obj_video", "at_ms": 2000, "name": "Q Cue" }),
        ),
        (
            "slide.add_cue",
            json!({ "slide": "slide1", "at_ms": 2500, "name": "Slide Cue 2" }),
        ),
        (
            "slide.set_cue",
            json!({ "slide": "slide1", "cue": "scue1", "patch": { "name": "Renamed Slide Cue" } }),
        ),
        (
            "slide.remove_cue",
            json!({ "slide": "slide1", "cue": "scue1" }),
        ),
        (
            "object.add_state",
            json!({ "slide": "slide1", "object": "obj_shape", "state": { "name": "hover", "fill": "#FFAA00" } }),
        ),
        (
            "object.update_state",
            json!({ "slide": "slide1", "object": "obj_shape", "state": { "name": "custom1", "fill": "#00AAFF" } }),
        ),
        (
            "object.patch_state",
            json!({ "slide": "slide1", "object": "obj_shape", "name": "custom1", "patch": { "glow": { "color": "#2F6FEB", "size": 8.0 }, "opacity": 0.85 } }),
        ),
        (
            "object.delete_state",
            json!({ "slide": "slide1", "object": "obj_shape", "name": "custom1" }),
        ),
        (
            "object.duplicate_state",
            json!({ "slide": "slide1", "object": "obj_shape", "name": "custom1" }),
        ),
        (
            "object.rename_state",
            json!({ "slide": "slide1", "object": "obj_shape", "from": "custom1", "to": "custom1_renamed" }),
        ),
        (
            "object.reorder_state",
            json!({ "slide": "slide1", "object": "obj_shape", "name": "custom2", "index": 0 }),
        ),
        (
            "object.state_add_extra",
            json!({
                "slide": "slide1",
                "object": "obj_shape",
                "state": "custom1",
                "extra": {
                    "kind": { "type": "shape", "shape": "rectangle", "fill": "#FFAA00" },
                    "frame": { "x": 4.0, "y": 4.0, "w": 20.0, "h": 20.0 }
                }
            }),
        ),
        (
            "object.state_update_extra",
            json!({
                "slide": "slide1",
                "object": "obj_shape",
                "state": "custom1",
                "extra": "extra1",
                "patch": { "name": "Renamed Badge", "opacity": 0.5 }
            }),
        ),
        (
            "object.state_delete_extra",
            json!({
                "slide": "slide1",
                "object": "obj_shape",
                "state": "custom1",
                "extra": "extra1"
            }),
        ),
        (
            "object.state_reorder_extra",
            json!({
                "slide": "slide1",
                "object": "obj_shape",
                "state": "custom1",
                "extra": "extra2",
                "index": 0
            }),
        ),
        (
            "object.delete",
            json!({ "slide": "slide1", "object": "obj_shape3" }),
        ),
        (
            "object.set_animation",
            json!({ "slide": "slide1", "object": "obj_shape", "which": "entrance", "animation": { "effect": { "type": "fade" }, "duration_ms": 500 } }),
        ),
        (
            "object.add_emphasis",
            json!({ "slide": "slide1", "object": "obj_shape2", "effect": "shake", "start_ms": 500 }),
        ),
        (
            "object.set_emphasis",
            json!({ "slide": "slide1", "object": "obj_shape", "index": 0, "emphasis": { "effect": { "type": "spin" }, "start_ms": 200, "duration_ms": 800 } }),
        ),
        (
            "object.remove_emphasis",
            json!({ "slide": "slide1", "object": "obj_shape", "index": 0 }),
        ),
        (
            "object.copy_animation",
            json!({ "slide": "slide1", "from": "obj_shape", "objects": ["obj_shape2", "obj_shape3"] }),
        ),
        (
            "object.add_motion_path",
            json!({ "slide": "slide1", "object": "obj_shape2", "duration_ms": 1200 }),
        ),
        (
            "object.update_motion_path",
            json!({ "slide": "slide1", "object": "obj_shape", "path": "mpath1", "duration_ms": 1500 }),
        ),
        (
            "object.remove_motion_path",
            json!({ "slide": "slide1", "object": "obj_shape", "path": "mpath1" }),
        ),
        (
            "text.set",
            json!({ "slide": "slide1", "object": "obj_text", "text": "Updated text" }),
        ),
        (
            "text.style_range",
            json!({ "slide": "slide1", "object": "obj_text", "start": 0, "end": 5, "bold": true }),
        ),
        (
            "text.clear_formatting",
            json!({ "slide": "slide1", "object": "obj_text", "start": 0, "end": 0 }),
        ),
        (
            "text.change_case",
            json!({ "slide": "slide1", "object": "obj_text", "start": 0, "end": 0, "case": "upper" }),
        ),
        (
            "text.set_paragraph",
            json!({ "slide": "slide1", "object": "obj_text", "align": "center" }),
        ),
        (
            "text.set_paragraphs",
            json!({ "slide": "slide1", "object": "obj_text", "list": "bullet" }),
        ),
        (
            "text.set_link",
            json!({ "slide": "slide1", "object": "obj_text", "link": { "type": "url", "url": "https://example.com" } }),
        ),
        (
            "object.set_autofit",
            json!({ "slide": "slide1", "object": "obj_text", "autofit": "shrink" }),
        ),
        (
            "table.set_cell",
            json!({ "slide": "slide1", "object": "obj_table", "row": 1, "column": 1, "text": "Edited" }),
        ),
        (
            "table.insert_row",
            json!({ "slide": "slide1", "object": "obj_table", "index": 2 }),
        ),
        (
            "table.delete_row",
            json!({ "slide": "slide1", "object": "obj_table", "index": 2 }),
        ),
        (
            "table.insert_column",
            json!({ "slide": "slide1", "object": "obj_table", "index": 2 }),
        ),
        (
            "table.delete_column",
            json!({ "slide": "slide1", "object": "obj_table", "index": 2 }),
        ),
        (
            "table.resize",
            json!({ "slide": "slide1", "object": "obj_table", "columns": [0.4, 0.3, 0.3] }),
        ),
        (
            "table.merge",
            json!({ "slide": "slide1", "object": "obj_table", "row": 1, "column": 0, "rows": 2, "columns": 2 }),
        ),
        (
            "table.split",
            json!({ "slide": "slide1", "object": "obj_table", "row": 0, "column": 0 }),
        ),
        (
            "table.set_borders",
            json!({ "slide": "slide1", "object": "obj_table", "outer": { "color": "#000000", "width": 2.0 } }),
        ),
        (
            "table.set_header_row",
            json!({ "slide": "slide1", "object": "obj_table", "header_row": true }),
        ),
        (
            "trigger.add",
            json!({
                "slide": "slide1",
                "trigger": serde_json::to_value(Trigger {
                    when: Event::TimelineStarts { layer: None },
                    condition: None,
                    action: Action::ShowLayer { layer: "layer1".into() },
                    disabled: false,
                }).unwrap()
            }),
        ),
        (
            "trigger.update",
            json!({
                "slide": "slide1",
                "index": 0,
                "trigger": serde_json::to_value(Trigger {
                    when: Event::Click { object: "obj_shape".into() },
                    condition: None,
                    action: Action::HideLayer { layer: Some("layer1".into()) },
                    disabled: false,
                }).unwrap()
            }),
        ),
        (
            "trigger.move",
            json!({ "slide": "slide1", "from": 0, "to": 1 }),
        ),
        ("trigger.delete", json!({ "slide": "slide1", "index": 0 })),
        (
            "resource.add",
            json!({ "title": "Doc", "target": { "kind": "url", "url": "https://example.com/doc" } }),
        ),
        (
            "resource.update",
            json!({ "id": "res1", "title": "Updated Guide" }),
        ),
        ("resource.remove", json!({ "id": "res1" })),
        ("resource.move", json!({ "id": "res2", "to": 0 })),
        (
            "glossary.add",
            json!({ "term": "LTI", "definition": "Learning Tools Interoperability" }),
        ),
        (
            "glossary.update",
            json!({ "id": "gloss1", "definition": "Sharable Content Object Reference Model" }),
        ),
        ("glossary.remove", json!({ "id": "gloss1" })),
        ("glossary.move", json!({ "id": "gloss2", "to": 0 })),
        ("glossary.sort", json!({})),
        (
            "variable.add",
            json!({ "name": "score_flag", "value": false }),
        ),
        ("variable.set_value", json!({ "name": "var1", "value": 42 })),
        ("variable.delete", json!({ "name": "var_unused" })),
        (
            "interaction.insert",
            json!({ "slide": "slide2", "kind": "tabs" }),
        ),
        ("question.insert", json!({ "slide": "slide2" })),
        ("results.insert", json!({ "after": "slide1" })),
        ("certificate.insert", json!({ "after": "slide1" })),
        (
            "question.add_submit_button",
            json!({ "slide": "slide_q_mc" }),
        ),
        (
            "question.add_feedback_layers",
            json!({ "slide": "slide_q_mc" }),
        ),
        (
            "question.add_try_again_layer",
            json!({ "slide": "slide_q_mc" }),
        ),
        (
            "question.set",
            json!({
                "slide": "slide2",
                "question": {
                    "kind": "true_false",
                    "prompt": "True or false?",
                    "choices": [
                        { "id": "t", "text": "True", "correct": true },
                        { "id": "f", "text": "False", "correct": false }
                    ],
                    "points": 5
                }
            }),
        ),
        ("question.delete", json!({ "slide": "slide_q_mc" })),
        (
            "question.set_kind",
            json!({ "slide": "slide_q_mc", "kind": "multiple_response" }),
        ),
        (
            "question.add_accepted",
            json!({ "slide": "slide_q_fib", "text": "RUST" }),
        ),
        (
            "question.remove_accepted",
            json!({ "slide": "slide_q_fib", "index": 1 }),
        ),
        (
            "question.update_accepted",
            json!({ "slide": "slide_q_fib", "index": 0, "text": "cargo" }),
        ),
        (
            "question.set_case_sensitive",
            json!({ "slide": "slide_q_fib", "case_sensitive": true }),
        ),
        (
            "question.add_pair",
            json!({ "slide": "slide_q_match", "left": "L3", "right": "R3" }),
        ),
        (
            "question.remove_pair",
            json!({ "slide": "slide_q_match", "id": "p2" }),
        ),
        (
            "question.update_pair",
            json!({ "slide": "slide_q_match", "id": "p1", "left": "Left 1" }),
        ),
        (
            "question.reorder_pair",
            json!({ "slide": "slide_q_match", "id": "p1", "to": 1 }),
        ),
        (
            "question.reorder_sequence",
            json!({ "slide": "slide_q_seq", "id": "sq1", "to": 2 }),
        ),
        (
            "question.set_hotspot_image",
            json!({ "slide": "slide_q_hotspot", "image": "hs_img2" }),
        ),
        (
            "question.add_region",
            json!({ "slide": "slide_q_hotspot", "x": 50.0, "y": 50.0, "w": 60.0, "h": 60.0, "correct": false }),
        ),
        (
            "question.update_region",
            json!({ "slide": "slide_q_hotspot", "id": "hr1", "w": 150.0 }),
        ),
        (
            "question.remove_region",
            json!({ "slide": "slide_q_hotspot", "id": "hr2" }),
        ),
        (
            "question.set_markers",
            json!({ "slide": "slide_q_hotspot", "markers": 3 }),
        ),
        (
            "question.add_drag_item",
            json!({ "slide": "slide_q_dnd", "item": "dnd_item3", "target": "dnd_target1" }),
        ),
        (
            "question.update_drag_item",
            json!({ "slide": "slide_q_dnd", "item": "dnd_item1", "target": "dnd_target2" }),
        ),
        (
            "question.remove_drag_item",
            json!({ "slide": "slide_q_dnd", "item": "dnd_item2" }),
        ),
        (
            "question.set_drop_targets",
            json!({ "slide": "slide_q_dnd", "targets": ["dnd_target1", "dnd_target2", "dnd_target3"] }),
        ),
        (
            "question.set_drag_options",
            json!({ "slide": "slide_q_dnd", "snap_to_center": false }),
        ),
        (
            "course.replace",
            json!({ "query": "Hello", "replacement": "Hi" }),
        ),
    ]
}

#[test]
fn d5_1_fixture_course_is_valid() {
    let s = fixture_session();
    let errs = validation_errors(&s.course);
    assert!(errs.is_empty(), "fixture course invalid: {errs:#?}");
}

#[test]
fn d5_1_all_mutating_commands_undo_redo_round_trip() {
    let cases = command_cases();

    let registry_mutating: Vec<&'static str> = registry()
        .iter()
        .filter(|c| c.mutates)
        .map(|c| c.id)
        .collect();

    let mut seen = HashSet::new();
    for &(id, _) in &cases {
        assert!(
            seen.insert(id),
            "duplicate command entry in command_cases(): {id}"
        );
        assert!(
            registry_mutating.contains(&id),
            "command_cases() entry {id} is not a mutates==true command in REGISTRY"
        );
    }

    let missing: Vec<&'static str> = registry_mutating
        .iter()
        .copied()
        .filter(|id| !seen.contains(id))
        .collect();
    assert!(
        missing.is_empty(),
        "REGISTRY mutating commands missing from D5.1 test table ({}): {missing:?}",
        missing.len()
    );

    let mut failures = Vec::new();
    for (id, params) in &cases {
        let mut s = fixture_session();
        let before = s.course.clone();

        let res = s.execute(id, params);
        if let Err(e) = res {
            failures.push(format!("{id}: failed to execute: {e:?}"));
            continue;
        }

        let after = s.course.clone();
        if after == before {
            failures.push(format!("{id}: did not mutate the course"));
            continue;
        }

        let errs = validation_errors(&s.course);
        if !errs.is_empty() {
            failures.push(format!("{id}: validation errors: {errs:?}"));
            continue;
        }

        if let Err(e) = s.execute("edit.undo", &json!({})) {
            failures.push(format!("{id}: first edit.undo failed: {e:?}"));
            continue;
        }
        if s.course != before {
            failures.push(format!("{id}: first edit.undo did not restore before"));
            continue;
        }

        if let Err(e) = s.execute("edit.redo", &json!({})) {
            failures.push(format!("{id}: edit.redo failed: {e:?}"));
            continue;
        }
        if s.course != after {
            failures.push(format!("{id}: edit.redo did not restore after"));
            continue;
        }

        if let Err(e) = s.execute("edit.undo", &json!({})) {
            failures.push(format!("{id}: second edit.undo failed: {e:?}"));
            continue;
        }
        if s.course != before {
            failures.push(format!("{id}: second edit.undo did not restore before"));
        }
    }

    assert!(
        failures.is_empty(),
        "D5.1 failures ({}):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn set_slide_size_fill_undo_redo_and_cli_docs_sync() {
    // A-05: course.set_slide_size ("fill") undo/redo + docs/cli.md sync.
    let cli_md = include_str!("../../../docs/cli.md");
    assert!(
        cli_md.contains(r#""mode": "scale"|"fill"|"keep""#),
        "docs/cli.md must document course.set_slide_size with scale|fill|keep"
    );

    let mut s = Session::blank("Slide Size Fill Undo");
    let before = s.course.clone();
    s.execute(
        "course.set_slide_size",
        &json!({ "w": 1000, "h": 1000, "mode": "fill" }),
    )
    .expect("course.set_slide_size fill");
    let after = s.course.clone();
    assert_ne!(after, before);
    assert!(validation_errors(&after).is_empty());

    s.execute("edit.undo", &json!({})).expect("undo");
    assert_eq!(s.course, before);
    s.execute("edit.redo", &json!({})).expect("redo");
    assert_eq!(s.course, after);
    s.execute("edit.undo", &json!({})).expect("second undo");
    assert_eq!(s.course, before);
}

/// DF-01: Differential byte-identical JSON and undo/redo metadata oracle across
/// every `mutates == true` command in `REGISTRY`.
#[test]
fn df_01_all_mutating_commands_byte_identical_delta_oracle() {
    let cases = command_cases();

    let registry_mutating: Vec<&'static str> = registry()
        .iter()
        .filter(|c| c.mutates)
        .map(|c| c.id)
        .collect();
    let seen: HashSet<&'static str> = cases.iter().map(|&(id, _)| id).collect();
    let missing: Vec<&'static str> = registry_mutating
        .iter()
        .copied()
        .filter(|id| !seen.contains(id))
        .collect();
    assert!(
        missing.is_empty(),
        "REGISTRY mutating commands missing from DF-01 ({}): {missing:?}",
        missing.len()
    );

    let mut failures = Vec::new();
    for &(id, ref params) in &cases {
        let label = registry()
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.label)
            .expect("command registered");

        let mut s = fixture_session();
        let before = s.course.clone();
        let before_json = before.to_json().expect("before json");
        let base_rev = s.revision;

        if let Err(e) = s.execute(id, params) {
            failures.push(format!("{id}: execute failed: {e:?}"));
            continue;
        }
        let after = s.course.clone();
        let after_json = after.to_json().expect("after json");
        if after == before || after_json == before_json {
            failures.push(format!("{id}: command did not change course JSON"));
            continue;
        }
        if s.undo_names() != vec![label] || !s.redo_names().is_empty() || s.revision != base_rev + 1
        {
            failures.push(format!(
                "{id}: post-execute oracle mismatch: undo={:?} redo={:?} rev={}",
                s.undo_names(),
                s.redo_names(),
                s.revision
            ));
            continue;
        }

        // Explicit CourseDelta::diff + apply byte-identical roundtrip check.
        match crate::delta::CourseDelta::diff(before.clone(), &after) {
            None => {
                failures.push(format!("{id}: CourseDelta::diff returned None"));
                continue;
            }
            Some(mut delta) => {
                let mut check = after.clone();
                if let Err(e) = delta.apply(&mut check) {
                    failures.push(format!("{id}: delta undo apply failed: {e:?}"));
                    continue;
                }
                let undone_json = check.to_json().expect("delta undo json");
                if check != before || undone_json != before_json {
                    failures.push(format!("{id}: delta undo JSON mismatch"));
                    continue;
                }
                if let Err(e) = delta.apply(&mut check) {
                    failures.push(format!("{id}: delta redo apply failed: {e:?}"));
                    continue;
                }
                let redone_json = check.to_json().expect("delta redo json");
                if check != after || redone_json != after_json {
                    failures.push(format!("{id}: delta redo JSON mismatch"));
                    continue;
                }
            }
        }

        // First edit.undo
        if let Err(e) = s.execute("edit.undo", &json!({})) {
            failures.push(format!("{id}: first edit.undo failed: {e:?}"));
            continue;
        }
        let undo1_json = s.course.to_json().expect("undo1 json");
        if s.course != before
            || undo1_json != before_json
            || !s.undo_names().is_empty()
            || s.redo_names() != vec![label]
            || s.revision != base_rev + 2
        {
            failures.push(format!(
                "{id}: first edit.undo oracle mismatch (json_eq={}, undo={:?}, redo={:?}, rev={})",
                undo1_json == before_json,
                s.undo_names(),
                s.redo_names(),
                s.revision
            ));
            continue;
        }

        // edit.redo
        if let Err(e) = s.execute("edit.redo", &json!({})) {
            failures.push(format!("{id}: edit.redo failed: {e:?}"));
            continue;
        }
        let redo_json = s.course.to_json().expect("redo json");
        if s.course != after
            || redo_json != after_json
            || s.undo_names() != vec![label]
            || !s.redo_names().is_empty()
            || s.revision != base_rev + 3
        {
            failures.push(format!(
                "{id}: edit.redo oracle mismatch (json_eq={}, undo={:?}, redo={:?}, rev={})",
                redo_json == after_json,
                s.undo_names(),
                s.redo_names(),
                s.revision
            ));
            continue;
        }

        // Second edit.undo
        if let Err(e) = s.execute("edit.undo", &json!({})) {
            failures.push(format!("{id}: second edit.undo failed: {e:?}"));
            continue;
        }
        let undo2_json = s.course.to_json().expect("undo2 json");
        if s.course != before
            || undo2_json != before_json
            || !s.undo_names().is_empty()
            || s.redo_names() != vec![label]
            || s.revision != base_rev + 4
        {
            failures.push(format!(
                "{id}: second edit.undo oracle mismatch (json_eq={}, undo={:?}, redo={:?}, rev={})",
                undo2_json == before_json,
                s.undo_names(),
                s.redo_names(),
                s.revision
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "DF-01 failures ({}):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// SC-01: Every annotated `SingleSlide` and `CourseHeader` command in `REGISTRY`
/// must mutate only within its declared scope (enforced by the `#[cfg(test)]`
/// scope-isolation verifier and shadow differential oracle), perform zero
/// full-course clones, and round-trip undo/redo with byte-identical JSON.
#[test]
fn sc_01_all_annotated_commands_isolated_and_zero_full_clones() {
    use crate::delta::UndoScope;

    let mut annotated_count = 0usize;
    for cmd in registry() {
        let scope = cmd.scope();
        if scope != UndoScope::FullCourse {
            annotated_count += 1;
            assert!(
                cmd.mutates,
                "command {} is annotated {:?} but has mutates == false",
                cmd.id, scope
            );
        }
    }
    assert!(
        annotated_count >= 130,
        "expected at least 130 commands annotated as SingleSlide or CourseHeader, found {annotated_count}"
    );

    let cases = command_cases();
    let mut failures = Vec::new();

    for &(id, ref params) in &cases {
        let cmd = registry()
            .iter()
            .find(|c| c.id == id)
            .expect("command registered");
        let scope = cmd.scope();

        let mut s = fixture_session();
        let before = s.course.clone();
        let before_json = before.to_json().expect("before json");

        #[cfg(debug_assertions)]
        crate::test_counts::reset();

        if let Err(e) = s.execute(id, params) {
            failures.push(format!("{id} ({scope:?}): execute failed: {e:?}"));
            continue;
        }

        #[cfg(debug_assertions)]
        if scope != UndoScope::FullCourse {
            let clones = crate::test_counts::full_course_clones();
            if clones != 0 {
                failures.push(format!(
                    "{id} ({scope:?}): expected 0 full-course clones, got {clones}"
                ));
                continue;
            }
        }

        let after = s.course.clone();
        let after_json = after.to_json().expect("after json");
        if after == before || after_json == before_json {
            failures.push(format!("{id} ({scope:?}): command did not mutate course"));
            continue;
        }

        if let Err(e) = s.execute("edit.undo", &json!({})) {
            failures.push(format!("{id} ({scope:?}): edit.undo failed: {e:?}"));
            continue;
        }
        let undo_json = s.course.to_json().expect("undo json");
        if s.course != before || undo_json != before_json {
            failures.push(format!(
                "{id} ({scope:?}): edit.undo did not restore byte-identical JSON"
            ));
            continue;
        }

        if let Err(e) = s.execute("edit.redo", &json!({})) {
            failures.push(format!("{id} ({scope:?}): edit.redo failed: {e:?}"));
            continue;
        }
        let redo_json = s.course.to_json().expect("redo json");
        if s.course != after || redo_json != after_json {
            failures.push(format!(
                "{id} ({scope:?}): edit.redo did not restore byte-identical JSON"
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "SC-01 failures ({}):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// U-05 (`M34-07`): Scoped single-slide validation agrees 100% with full
/// `Course::validate()` across every mutating command in `REGISTRY` and across
/// hostile invalid inputs (bad frames, NaN frames, invalid jump targets,
/// invalid colors, master restrictions, bank slides, bad triggers, bad draws).
#[test]
fn u05_scoped_validation_matches_full_validation_across_all_commands() {
    use crate::delta::UndoScope;

    // Every command whitelisted for scoped slide validation must be a mutating
    // SingleSlide command, and non-mutating commands must keep FullCourse scope
    // without slide_validation.
    for cmd in registry() {
        if cmd.can_use_slide_validation() {
            assert!(
                cmd.mutates,
                "can_use_slide_validation({}) must have mutates == true",
                cmd.id
            );
            assert_eq!(
                cmd.scope(),
                UndoScope::SingleSlide,
                "can_use_slide_validation({}) must have UndoScope::SingleSlide",
                cmd.id
            );
        }
        if !cmd.mutates {
            assert_eq!(
                cmd.scope(),
                UndoScope::FullCourse,
                "non-mutating command {} must have UndoScope::FullCourse",
                cmd.id
            );
            assert!(
                !cmd.can_use_slide_validation(),
                "non-mutating command {} must not enable slide_validation",
                cmd.id
            );
        }
    }

    let cases = command_cases();
    let mut failures = Vec::new();

    // 1. Differential parity across every mutating command in REGISTRY.
    for &(id, ref params) in &cases {
        let mut s_scoped = fixture_session();
        let mut s_full = fixture_session();
        let _ = s_scoped.validation_issues();
        let _ = s_full.validation_issues();

        let res_scoped = s_scoped.execute(id, params);
        let res_full = crate::with_full_validation_forced(|| s_full.execute(id, params));

        if res_scoped.is_ok() != res_full.is_ok() {
            failures.push(format!(
                "valid {id}: accept/reject mismatch: scoped={res_scoped:?} full={res_full:?}"
            ));
            continue;
        }
        if res_scoped != res_full {
            failures.push(format!(
                "valid {id}: result mismatch: scoped={res_scoped:?} full={res_full:?}"
            ));
            continue;
        }
        if s_scoped.course != s_full.course {
            failures.push(format!("valid {id}: resulting course diverged"));
            continue;
        }
        if res_scoped.is_ok() {
            let full_errs = validation_errors(&s_scoped.course);
            if !full_errs.is_empty() {
                failures.push(format!(
                    "valid {id}: scoped validation accepted invalid course: {full_errs:?}"
                ));
            }
        }
    }

    // 2. Hostile invalid inputs across scoped and fallback commands.
    let hostile_cases: Vec<(&str, &str, Json)> = vec![
        (
            "negative_frame_width",
            "object.set_frame",
            json!({ "slide": "slide1", "object": "obj_shape", "frame": { "x": 10.0, "y": 10.0, "w": -5.0, "h": 40.0 } }),
        ),
        (
            "oversized_frame_height",
            "object.set_frame",
            json!({ "slide": "slide1", "object": "obj_shape", "frame": { "x": 10.0, "y": 10.0, "w": 40.0, "h": 200_000.0 } }),
        ),
        (
            "nan_frame_null_in_json",
            "object.set_frame",
            json!({ "slide": "slide1", "object": "obj_shape", "frame": { "x": f64::NAN, "y": 10.0, "w": 40.0, "h": 40.0 } }),
        ),
        (
            "scroll_panel_frame_taller_than_content",
            "object.set_frame",
            json!({ "slide": "slide1", "object": "obj_scroll", "frame": { "x": 720.0, "y": 270.0, "w": 150.0, "h": 900.0 } }),
        ),
        (
            "invalid_slide_background_color",
            "slide.set_background",
            json!({ "slide": "slide1", "background": "#ZZZZZZ" }),
        ),
        (
            "invalid_object_fill_color",
            "object.update",
            json!({ "slide": "slide1", "object": "obj_shape", "kind": { "type": "shape", "shape": "rectangle", "fill": "not-hex" } }),
        ),
        (
            "invalid_object_timing_inverted",
            "object.update",
            json!({ "slide": "slide1", "object": "obj_shape", "timing": { "start_ms": 2000, "end_ms": 1000 } }),
        ),
        (
            "invalid_slide_transition_zero_ms",
            "slide.set_transition",
            json!({ "slide": "slide1", "transition": { "type": "fade", "duration_ms": 0 } }),
        ),
        (
            "invalid_player_override_missing_var",
            "slide.set_player_overrides",
            json!({ "slide": "slide1", "next_requires": { "kind": "variable", "name": "no_such_var" } }),
        ),
        (
            "invalid_player_override_non_bool_var",
            "slide.set_player_overrides",
            json!({ "slide": "slide1", "next_requires": { "kind": "variable", "name": "var1" } }),
        ),
        (
            "invalid_fill_style_color",
            "object.set_fill_style",
            json!({ "slide": "slide1", "object": "obj_shape", "fill_style": { "type": "linear", "angle": 90.0, "stops": [{ "position": 0.0, "color": "bad" }, { "position": 1.0, "color": "#0000FF" }] } }),
        ),
        (
            "invalid_shadow_color",
            "object.set_shadow",
            json!({ "slide": "slide1", "object": "obj_shape", "shadow": { "offset_x": 4.0, "offset_y": 4.0, "blur": 8.0, "color": "bad" } }),
        ),
        (
            "invalid_glow_color",
            "object.set_glow",
            json!({ "slide": "slide1", "object": "obj_shape", "glow": { "size": 6.0, "color": "bad" } }),
        ),
        (
            "invalid_crop_past_bounds",
            "object.set_crop",
            json!({ "slide": "slide1", "object": "obj_pic", "crop": { "x": 0.8, "y": 0.1, "w": 0.5, "h": 0.5 } }),
        ),
        (
            "invalid_link_missing_slide",
            "text.set_link",
            json!({ "slide": "slide1", "object": "obj_text", "link": { "type": "slide", "slide": "missing_slide" } }),
        ),
        (
            "invalid_link_javascript_url",
            "text.set_link",
            json!({ "slide": "slide1", "object": "obj_text", "link": { "type": "url", "url": "javascript:alert(1)" } }),
        ),
        (
            "invalid_drag_target_missing",
            "object.set_drag",
            json!({ "slide": "slide1", "object": "obj_shape", "drag": { "targets": ["ghost_target"], "snap": true, "return_on_miss": true } }),
        ),
        (
            "invalid_panel_non_scroll",
            "object.set_panel",
            json!({ "slide": "slide1", "object": "obj_shape", "panel": "obj_shape2" }),
        ),
        (
            "invalid_state_fill_color",
            "object.add_state",
            json!({ "slide": "slide1", "object": "obj_shape", "state": { "name": "bad_state", "fill": "not-hex" } }),
        ),
        (
            "invalid_master_video_object",
            "object.add",
            json!({
                "slide": "master1",
                "kind": { "type": "video", "src": "media/clip.mp4" },
                "frame": { "x": 10.0, "y": 10.0, "w": 160.0, "h": 90.0 }
            }),
        ),
        (
            "invalid_bank_slide_bad_color",
            "object.add",
            json!({
                "slide": "bank_q1",
                "kind": { "type": "shape", "shape": "rectangle", "fill": "nope" },
                "frame": { "x": 10.0, "y": 10.0, "w": 80.0, "h": 40.0 }
            }),
        ),
        (
            "invalid_trigger_jump_target",
            "trigger.add",
            json!({
                "slide": "slide1",
                "trigger": {
                    "when": { "event": "click", "object": "obj_shape" },
                    "action": { "action": "jump_to_slide", "target": { "id": "missing_slide" } }
                }
            }),
        ),
        (
            "invalid_draw_count_exceeds_bank",
            "draw.update",
            json!({ "slide": "slide_draw", "count": 99 }),
        ),
        (
            "invalid_motion_path_coords",
            "object.update_motion_path",
            json!({
                "slide": "slide1",
                "object": "obj_shape",
                "path": "mpath1",
                "points": [{ "x": 0.0, "y": 0.0 }, { "x": 500_000.0, "y": 0.0 }]
            }),
        ),
    ];

    for (label, id, params) in &hostile_cases {
        let mut s_scoped = fixture_session();
        let mut s_full = fixture_session();
        let before = s_scoped.course.clone();
        let _ = s_scoped.validation_issues();
        let _ = s_full.validation_issues();

        let res_scoped = s_scoped.execute(id, params);
        let res_full = crate::with_full_validation_forced(|| s_full.execute(id, params));

        if res_scoped.is_ok() {
            failures.push(format!(
                "hostile {label} ({id}): expected Err, got Ok({res_scoped:?})"
            ));
            continue;
        }
        if res_scoped != res_full {
            failures.push(format!(
                "hostile {label} ({id}): error mismatch: scoped={res_scoped:?} full={res_full:?}"
            ));
            continue;
        }
        if s_scoped.course != before || s_full.course != before {
            failures.push(format!(
                "hostile {label} ({id}): failed command did not roll back course"
            ));
        }
    }

    // 3. Direct NaN frame check: validate_slide_errors agrees with first_error / validate.
    let mut nan_course = fixture_session().course;
    nan_course.scenes[0].slides[0].objects[0].frame.w = f32::NAN;
    let scoped_nan =
        nan_course.validate_slide_errors(&nan_course.scenes[0].slides[0], "/scenes/0/slides/0");
    let full_nan = nan_course.first_error();
    assert!(
        scoped_nan.is_some(),
        "NaN frame must be rejected by scoped validation"
    );
    assert_eq!(
        scoped_nan, full_nan,
        "NaN frame error must match full validation"
    );

    assert!(
        failures.is_empty(),
        "U-05 scoped validation parity failures ({}):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn pt_07_point_text_single_step_undo() {
    let mut s = Session::blank("Point Text Undo");
    assert_eq!(s.undo_len(), 0);

    // 1. Insert point text = 1 undo step.
    s.execute(
        "object.add",
        &json!({
            "slide": "slide1",
            "id": "pt1",
            "kind": { "type": "text", "text": "Next", "point_text": true },
            "frame": { "x": 100.0, "y": 100.0, "w": 300.0, "h": 60.0 }
        }),
    )
    .expect("add point text");
    assert_eq!(s.undo_len(), 1);
    let after_add = s.course.clone();
    let initial_frame = s
        .course
        .slide("slide1")
        .unwrap()
        .object("pt1")
        .unwrap()
        .0
        .frame;
    assert!(initial_frame.w < 120.0);

    // 2. text.set (text + refitted frame) = 1 undo step.
    s.execute(
        "text.set",
        &json!({
            "slide": "slide1",
            "object": "pt1",
            "text": "Continue to Module 2"
        }),
    )
    .expect("text.set");
    assert_eq!(s.undo_len(), 2);
    let after_edit = s.course.clone();
    let edited_frame = s
        .course
        .slide("slide1")
        .unwrap()
        .object("pt1")
        .unwrap()
        .0
        .frame;
    assert!(edited_frame.w > initial_frame.w * 2.0);

    // 3. Width resize conversion = 1 undo step.
    let resized_w = edited_frame.w + 80.0;
    s.execute(
        "object.set_frame",
        &json!({
            "slide": "slide1",
            "object": "pt1",
            "frame": {
                "x": edited_frame.x,
                "y": edited_frame.y,
                "w": resized_w,
                "h": edited_frame.h
            }
        }),
    )
    .expect("resize width");
    assert_eq!(s.undo_len(), 3);
    let after_convert = s.course.clone();
    match &s
        .course
        .slide("slide1")
        .unwrap()
        .object("pt1")
        .unwrap()
        .0
        .kind
    {
        ObjectKind::Text {
            point_text, style, ..
        } => {
            assert!(!*point_text);
            assert!(style.wrap);
        }
        other => panic!("expected text, got {other:?}"),
    }

    // Undo resize conversion in 1 step -> restores point_text = true, wrap = false, old width.
    s.execute("edit.undo", &json!({})).expect("undo resize");
    assert_eq!(s.course, after_edit);
    let (obj_after_undo_resize, _) = s.course.slide("slide1").unwrap().object("pt1").unwrap();
    assert_eq!(obj_after_undo_resize.frame, edited_frame);
    assert!(matches!(
        &obj_after_undo_resize.kind,
        ObjectKind::Text { point_text: true, style, .. } if !style.wrap
    ));

    // Undo text.set in 1 step -> restores both old text AND old fitted frame.
    s.execute("edit.undo", &json!({})).expect("undo text.set");
    assert_eq!(s.course, after_add);
    let (obj_after_undo_edit, _) = s.course.slide("slide1").unwrap().object("pt1").unwrap();
    assert_eq!(obj_after_undo_edit.frame, initial_frame);
    assert!(matches!(
        &obj_after_undo_edit.kind,
        ObjectKind::Text { text, point_text: true, style, .. }
            if text == "Next" && !style.wrap
    ));

    // Undo insert in 1 step -> object removed.
    s.execute("edit.undo", &json!({})).expect("undo add");
    assert!(s.course.slide("slide1").unwrap().object("pt1").is_none());

    // Redo all 3 steps restores exact states.
    s.execute("edit.redo", &json!({})).expect("redo add");
    assert_eq!(s.course, after_add);
    s.execute("edit.redo", &json!({})).expect("redo text.set");
    assert_eq!(s.course, after_edit);
    s.execute("edit.redo", &json!({})).expect("redo resize");
    assert_eq!(s.course, after_convert);
}

fn assert_one_step_undo_redo(s: &mut Session, cmd: &str, params: &Json, keep_applied: bool) {
    let before = s.course.clone();
    let undo_before = s.undo_len();
    s.execute(cmd, params).expect(cmd);
    assert_eq!(s.undo_len(), undo_before + 1, "{cmd} must be one undo step");
    let after = s.course.clone();
    assert_ne!(after, before, "{cmd} must mutate course");
    s.execute("edit.undo", &json!({})).expect("undo");
    assert_eq!(s.course, before, "{cmd} undo must restore before");
    s.execute("edit.redo", &json!({})).expect("redo");
    assert_eq!(s.course, after, "{cmd} redo must restore after");
    if !keep_applied {
        s.execute("edit.undo", &json!({})).expect("revert");
        assert_eq!(s.course, before);
    }
}

#[test]
fn gb_12_interactive_group_commands_single_step_undo() {
    let mut s = Session::blank("Interactive Group Undo");
    s.execute(
        "object.add",
        &json!({
            "slide": "slide1",
            "id": "bg",
            "kind": { "type": "shape", "shape": "rectangle", "fill": "#1D4ED8" },
            "frame": { "x": 60.0, "y": 60.0, "w": 160.0, "h": 48.0 }
        }),
    )
    .expect("add bg");
    s.execute(
        "object.add",
        &json!({
            "slide": "slide1",
            "id": "lbl",
            "kind": { "type": "text", "text": "Submit" },
            "frame": { "x": 80.0, "y": 70.0, "w": 100.0, "h": 28.0 }
        }),
    )
    .expect("add lbl");
    let gid = s
        .execute(
            "object.group",
            &json!({ "slide": "slide1", "objects": ["bg", "lbl"] }),
        )
        .expect("group")["group"]
        .as_str()
        .unwrap()
        .to_owned();

    // 1. group.set_interactive ON, group.update, and group.set_interactive OFF.
    assert_one_step_undo_redo(
        &mut s,
        "group.set_interactive",
        &json!({ "slide": "slide1", "group": gid, "interactive": true }),
        true,
    );
    assert_one_step_undo_redo(
        &mut s,
        "group.update",
        &json!({
            "slide": "slide1",
            "group": gid,
            "name": "Submit answers",
            "initial_state": "disabled",
            "focus_order": 2,
            "description": "Submits the form"
        }),
        true,
    );
    assert_one_step_undo_redo(
        &mut s,
        "group.set_interactive",
        &json!({ "slide": "slide1", "group": gid, "interactive": false }),
        false,
    );

    // Add a group trigger to verify 1-step undo of duplicate, ungroup, and delete.
    s.execute(
        "trigger.add",
        &json!({
            "slide": "slide1",
            "trigger": {
                "when": { "event": "click", "object": gid },
                "action": { "action": "pause_timeline" }
            }
        }),
    )
    .expect("add group trigger");

    assert_one_step_undo_redo(
        &mut s,
        "object.duplicate",
        &json!({ "slide": "slide1", "objects": ["bg", "lbl"] }),
        false,
    );
    assert_one_step_undo_redo(
        &mut s,
        "object.ungroup",
        &json!({ "slide": "slide1", "objects": ["bg", "lbl"] }),
        false,
    );
    assert_one_step_undo_redo(
        &mut s,
        "object.delete",
        &json!({ "slide": "slide1", "object": "bg" }),
        true,
    );
    assert_one_step_undo_redo(
        &mut s,
        "object.delete",
        &json!({ "slide": "slide1", "object": "lbl" }),
        true,
    );
    assert!(s.course.slide("slide1").unwrap().groups.is_empty());
    assert!(s.course.slide("slide1").unwrap().triggers.is_empty());
}
