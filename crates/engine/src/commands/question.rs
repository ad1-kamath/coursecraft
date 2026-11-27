//! Question authoring, feedback layers, and question-kind commands.

use std::collections::BTreeSet;

use coursecraft_course::{
    Action, Align, Autofit, Choice, CmpOp, Color, Condition, Course, DragItem, Event,
    HotspotRegion, HotspotShape, Layer, MatchPair, Object, ObjectKind, Question, QuestionKind,
    Rect, RevisitMode, Scoring, SeekMode, ShapeKind, Size, Slide, Theme, Timing, Trigger, VAlign,
    Value,
};
use serde_json::{Value as Json, json};

use super::{bad, fresh_id, keep_valid, opt, req, results_button, slide_mut, text};
use crate::{EngineError, Result, Session};

/// The default question `question.insert` starts from, matching the editor's old
/// `insert_question` defaults.
pub(super) fn default_multiple_choice_question() -> Question {
    Question {
        kind: QuestionKind::MultipleChoice,
        prompt: "Enter question prompt here".to_owned(),
        choices: vec![
            Choice {
                id: "a".to_owned(),
                text: "Choice 1".to_owned(),
                correct: true,
                points: 0,
            },
            Choice {
                id: "b".to_owned(),
                text: "Choice 2".to_owned(),
                correct: false,
                points: 0,
            },
        ],
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

/// Add the placeholder objects a hotspot or drag-and-drop question needs so the
/// course stays valid before the author picks real content in the panel.
pub(super) fn ensure_question_support(slide: &mut Slide, kind: QuestionKind, theme: &Theme) {
    match kind {
        QuestionKind::Hotspot => {
            let has_image = slide
                .objects
                .iter()
                .any(|object| matches!(object.kind, ObjectKind::Image { .. }));
            if !has_image {
                let id = fresh_id("image", slide.local_ids());
                slide.objects.push(Object {
                    id,
                    name: "Hotspot image".to_owned(),
                    kind: ObjectKind::Image {
                        src: "placeholder.png".to_owned(),
                        alt: "Choose an image for the hotspot".to_owned(),
                        crop: None,
                        outline: None,
                        shape: None,
                    },
                    frame: Rect {
                        x: 240.0,
                        y: 180.0,
                        w: 800.0,
                        h: 360.0,
                    },
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
                });
            }
        }
        QuestionKind::DragDrop => {
            let has_named =
                |slide: &Slide, name: &str| slide.objects.iter().any(|object| object.name == name);
            if !has_named(slide, "Drag me") {
                let id = fresh_id("button", slide.local_ids());
                slide.objects.push(Object {
                    id,
                    name: "Drag me".to_owned(),
                    kind: ObjectKind::Button {
                        label: "Drag me".to_owned(),
                        fill: theme.accent.clone(),
                        fill_style: None,
                    },
                    frame: Rect {
                        x: 200.0,
                        y: 240.0,
                        w: 200.0,
                        h: 100.0,
                    },
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
                });
            }
            if !has_named(slide, "Drop target") {
                let target_frame = Rect {
                    x: 560.0,
                    y: 210.0,
                    w: 280.0,
                    h: 160.0,
                };
                let id = fresh_id("shape", slide.local_ids());
                slide.objects.push(Object {
                    id,
                    name: "Drop target".to_owned(),
                    kind: ObjectKind::Shape {
                        shape: ShapeKind::RoundedRectangle,
                        fill: theme.accent.clone(),
                        fill_style: None,
                        outline: None,
                        text: None,
                        adjust: Vec::new(),
                    },
                    frame: target_frame,
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
                });
                let label_id = fresh_id("text", slide.local_ids());
                slide.objects.push(Object {
                    id: label_id,
                    name: "Drop target label".to_owned(),
                    kind: ObjectKind::Text {
                        text: "Drop here".to_owned(),
                        style: coursecraft_course::TextStyle {
                            size: 18.0,
                            bold: true,
                            color: Color("#FFFFFF".to_owned()),
                            align: Align::Center,
                            valign: VAlign::Middle,
                            ..coursecraft_course::TextStyle::default()
                        },
                        runs: Vec::new(),
                        paragraphs: Vec::new(),
                        autofit: Autofit::None,
                        text_style: None,
                        outline: None,
                        point_text: false,
                    },
                    frame: Rect {
                        x: target_frame.x + 40.0,
                        y: target_frame.y + target_frame.h * 0.5 - 20.0,
                        w: target_frame.w - 80.0,
                        h: 40.0,
                    },
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
                });
            }
        }
        _ => {}
    }
}

fn submit_button_frame(slide_size: Size) -> Rect {
    const MARGIN: f32 = 40.0;
    const W: f32 = 160.0;
    const H: f32 = 48.0;
    let max_x = (slide_size.w as f32 - MARGIN - W).max(0.0);
    let max_y = (slide_size.h as f32 - MARGIN - H).max(0.0);
    Rect {
        x: max_x,
        y: max_y,
        w: W,
        h: H,
    }
}

fn frames_overlap(a: Rect, b: Rect) -> bool {
    let finite = |r: Rect| r.x.is_finite() && r.y.is_finite() && r.w.is_finite() && r.h.is_finite();
    finite(a)
        && finite(b)
        && a.w > 0.0
        && a.h > 0.0
        && b.w > 0.0
        && b.h > 0.0
        && a.x < b.x + b.w
        && b.x < a.x + a.w
        && a.y < b.y + b.h
        && b.y < a.y + a.h
}

/// Bottom-right Finish button, shifted left of any button it would cover.
fn finish_button_frame(slide: &Slide, slide_size: Size) -> Rect {
    const MARGIN: f32 = 40.0;
    const W: f32 = 160.0;
    const H: f32 = 48.0;
    const GAP: f32 = 12.0;
    let mut x = (slide_size.w as f32 - MARGIN - W).max(0.0);
    let y = (slide_size.h as f32 - MARGIN - H).max(0.0);
    for _ in 0..64 {
        let candidate = Rect { x, y, w: W, h: H };
        let covering = slide
            .objects
            .iter()
            .filter(|object| matches!(object.kind, ObjectKind::Button { .. }))
            .map(|object| object.frame)
            .find(|frame| frames_overlap(candidate, *frame));
        let Some(frame) = covering else {
            break;
        };
        x = (frame.x - GAP - W).max(MARGIN);
    }
    Rect { x, y, w: W, h: H }
}

pub(super) fn add_finish_button(slide: &mut Slide, theme: &Theme, slide_size: Size) -> String {
    let id = fresh_id("button", slide.local_ids());
    slide.objects.push(Object {
        id: id.clone(),
        name: "Finish".to_owned(),
        kind: ObjectKind::Button {
            label: "Finish".to_owned(),
            fill: theme.accent.clone(),
            fill_style: None,
        },
        frame: finish_button_frame(slide, slide_size),
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
    });
    slide.triggers.push(Trigger {
        action: Action::SubmitResults,
        when: Event::Click { object: id.clone() },
        condition: None,
        disabled: false,
    });
    slide.triggers.push(Trigger {
        action: Action::ExitCourse,
        when: Event::Click { object: id.clone() },
        condition: None,
        disabled: false,
    });
    id
}

/// Remove submit-event `submit_question` triggers, which no player control can
/// ever fire, and add a Submit button plus a click trigger that grades the question.
pub(super) fn add_submit_button(slide: &mut Slide, theme: &Theme, slide_size: Size) -> String {
    let is_dead_submit = |trigger: &Trigger| {
        trigger.action == Action::SubmitQuestion && trigger.when == Event::Submit
    };
    slide.triggers.retain(|trigger| !is_dead_submit(trigger));
    for layer in &mut slide.layers {
        layer.triggers.retain(|trigger| !is_dead_submit(trigger));
    }

    let id = fresh_id("button", slide.local_ids());
    slide.objects.push(Object {
        id: id.clone(),
        name: "Submit".to_owned(),
        kind: ObjectKind::Button {
            label: "Submit".to_owned(),
            fill: theme.accent.clone(),
            fill_style: None,
        },
        frame: submit_button_frame(slide_size),
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
    });
    slide.triggers.push(Trigger {
        action: Action::SubmitQuestion,
        when: Event::Click { object: id.clone() },
        condition: None,
        disabled: false,
    });
    id
}

/// Where a feedback message can sit without covering the question's response
/// rows. Falls back above the prompt when a tall question leaves no room below.
fn feedback_text_frame(slide_size: Size, question: Option<&Question>) -> Rect {
    const MARGIN: f32 = 24.0;
    const ROW_START: f32 = 160.0;
    const ROW_H: f32 = 56.0;
    const ROW_GAP: f32 = 16.0;
    const MIN_H: f32 = 28.0;
    const MIN_GAP: f32 = 4.0;
    let w = (slide_size.w as f32 - 160.0).clamp(0.0, 900.0);
    let slide_h = slide_size.h as f32;
    let below = question.map_or(ROW_START - 20.0, |q| {
        let rows = match q.kind {
            QuestionKind::MultipleChoice
            | QuestionKind::MultipleResponse
            | QuestionKind::TrueFalse
            | QuestionKind::Sequence
            | QuestionKind::Likert => q.choices.len(),
            QuestionKind::FillIn
            | QuestionKind::Matching
            | QuestionKind::ShortAnswer
            | QuestionKind::Essay => q.pairs.len().max(1),
            QuestionKind::Hotspot | QuestionKind::DragDrop => 0,
            QuestionKind::Freeform => 0,
        };
        if rows == 0 {
            ROW_START - 20.0
        } else {
            let available = (slide_h - ROW_START - MARGIN).max(0.0);
            let (row_h, stride) = if (rows as f32) * (ROW_H + ROW_GAP) <= available {
                (ROW_H, ROW_H + ROW_GAP)
            } else {
                let min_stride = MIN_H + MIN_GAP;
                if !available.is_finite() || (rows as f32) * min_stride > available {
                    (MIN_H, min_stride)
                } else {
                    let stride = available / rows as f32;
                    let h = (stride - MIN_GAP).clamp(MIN_H, ROW_H);
                    (h, h + (stride - h).max(0.0))
                }
            };
            ROW_START + stride * (rows.saturating_sub(1)) as f32 + row_h
        }
    });
    let h = 60.0;
    let y = below + 16.0;
    if y + h <= slide_h - MARGIN {
        Rect { x: 80.0, y, w, h }
    } else {
        Rect {
            x: 80.0,
            y: 10.0,
            w,
            h: 40.0,
        }
    }
}

/// Add the two hidden feedback layers and point the question at them.
pub(super) fn add_feedback_layers(
    slide: &mut Slide,
    theme: &Theme,
    slide_size: Size,
) -> (String, String) {
    let frame = feedback_text_frame(slide_size, slide.question.as_ref());
    let make_layer = |slide: &mut Slide, name: &str, text: &str| {
        let layer_id = fresh_id(name, slide.local_ids());
        let text_id = fresh_id("feedback_text", slide.local_ids());
        slide.layers.push(Layer {
            id: layer_id.clone(),
            name: name.to_owned(),
            duration_ms: 5000,
            objects: vec![Object {
                id: text_id,
                name: format!("{name} feedback"),
                kind: ObjectKind::Text {
                    text: text.to_owned(),
                    style: coursecraft_course::TextStyle {
                        size: 28.0,
                        bold: true,
                        color: theme.text.clone(),
                        ..coursecraft_course::TextStyle::default()
                    },
                    runs: Vec::new(),
                    paragraphs: Vec::new(),
                    autofit: Autofit::None,
                    text_style: None,
                    outline: None,
                    point_text: false,
                },
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
            }],
            triggers: Vec::new(),
            hide_other_layers: true,
            pause_base_timeline: false,
            prevent_base_clicks: false,
            hide_base_objects: false,
            hide_when_timeline_ends: false,
            allow_seek: SeekMode::Auto,
            revisit: RevisitMode::Auto,
            cue_points: Vec::new(),
            groups: Vec::new(),
        });
        layer_id
    };
    let correct = make_layer(slide, "Correct", "Correct!");
    let incorrect = make_layer(slide, "Incorrect", "Not quite.");
    if let Some(question) = slide.question.as_mut() {
        question.correct_layer = Some(correct.clone());
        question.incorrect_layer = Some(incorrect.clone());
    }
    (correct, incorrect)
}

/// Add a hidden Try Again layer with a message and a "Try Again" button wired
/// to hide the layer, and point the question's `try_again_layer` at it.
fn add_try_again_layer(slide: &mut Slide, theme: &Theme, slide_size: Size) -> String {
    let text_frame = feedback_text_frame(slide_size, slide.question.as_ref());
    let button_w = 160.0_f32.min(text_frame.w.max(80.0));
    let button_h = 44.0;
    let slide_h = slide_size.h.max(1) as f32;
    let button_y = (text_frame.y + text_frame.h + 12.0).min((slide_h - button_h - 16.0).max(0.0));
    let button_frame = Rect {
        x: text_frame.x,
        y: button_y,
        w: button_w,
        h: button_h,
    };
    let layer_id = fresh_id("try_again", slide.local_ids());
    let text_id = fresh_id("try_again_text", slide.local_ids());
    let button_id = fresh_id("try_again_button", slide.local_ids());
    let objects = vec![
        Object {
            id: text_id,
            name: "Try Again feedback".to_owned(),
            kind: ObjectKind::Text {
                text: "Not quite — give it another try.".to_owned(),
                style: coursecraft_course::TextStyle {
                    size: 28.0,
                    bold: true,
                    color: theme.text.clone(),
                    ..coursecraft_course::TextStyle::default()
                },
                runs: Vec::new(),
                paragraphs: Vec::new(),
                autofit: Autofit::None,
                text_style: None,
                outline: None,
                point_text: false,
            },
            frame: text_frame,
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
        },
        results_button(
            button_id.clone(),
            "Try Again",
            theme.accent.clone(),
            button_frame,
        ),
    ];
    let triggers = vec![Trigger {
        when: Event::Click { object: button_id },
        action: Action::HideLayer { layer: None },
        condition: None,
        disabled: false,
    }];
    slide.layers.push(Layer {
        id: layer_id.clone(),
        name: "Try Again".to_owned(),
        duration_ms: 5000,
        objects,
        triggers,
        hide_other_layers: true,
        pause_base_timeline: false,
        prevent_base_clicks: true,
        hide_base_objects: false,
        hide_when_timeline_ends: false,
        allow_seek: SeekMode::Auto,
        revisit: RevisitMode::Auto,
        cue_points: Vec::new(),
        groups: Vec::new(),
    });
    if let Some(question) = slide.question.as_mut() {
        question.try_again_layer = Some(layer_id.clone());
    }
    layer_id
}

pub(super) fn question_add_feedback_layers(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "question.add_feedback_layers";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let theme = s.course.theme.clone();
    let slide_size = s.course.slide_size;
    let slide = slide_mut(s, &sid)?;
    if slide.question.is_none() {
        return Err(EngineError::NotFound(format!(
            "slide {sid:?} has no question"
        )));
    }
    let (correct, incorrect) = add_feedback_layers(slide, &theme, slide_size);
    keep_valid(s, was_valid)?;
    Ok(json!({ "correct": correct, "incorrect": incorrect }))
}

pub(super) fn question_add_try_again_layer(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "question.add_try_again_layer";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let theme = s.course.theme.clone();
    let slide_size = s.course.slide_size;
    let slide = slide_mut(s, &sid)?;
    if slide.question.is_none() {
        return Err(EngineError::NotFound(format!(
            "slide {sid:?} has no question"
        )));
    }
    let layer = add_try_again_layer(slide, &theme, slide_size);
    keep_valid(s, was_valid)?;
    Ok(json!({ "layer": layer }))
}

/// The condition a new freeform question starts with: the first user variable
/// (if any) compared with zero, else the built-in score percent.
fn freeform_default_condition(course: &Course) -> Condition {
    match course.variables.first() {
        Some(v) => match &v.value {
            Value::Bool(_) => Condition::Compare {
                variable: v.name.clone(),
                op: CmpOp::Eq,
                value: Value::Bool(true).into(),
            },
            Value::Number(_) => Condition::Compare {
                variable: v.name.clone(),
                op: CmpOp::Ge,
                value: Value::Number(0.0).into(),
            },
            Value::Text(_) => Condition::Compare {
                variable: v.name.clone(),
                op: CmpOp::Eq,
                value: Value::Text(String::new()).into(),
            },
        },
        None => Condition::Compare {
            variable: "Course.ScorePercent".to_owned(),
            op: CmpOp::Ge,
            value: Value::Number(0.0).into(),
        },
    }
}

pub(super) fn question_insert(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "question.insert";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let kind: QuestionKind = opt(C, p, "kind")?.unwrap_or(QuestionKind::MultipleChoice);
    let freeform_default = freeform_default_condition(&s.course);
    let theme = s.course.theme.clone();
    let slide_size = s.course.slide_size;

    let slide = slide_mut(s, &sid)?;
    if slide.question.is_some() {
        return Err(EngineError::Invalid(format!(
            "slide {sid:?} already has a question"
        )));
    }
    ensure_question_support(slide, kind, &theme);
    let mut question = default_multiple_choice_question();
    apply_question_kind(&slide.objects, &mut question, kind)?;
    if kind == QuestionKind::Freeform {
        question.correct_when = Some(freeform_default);
    }
    slide.question = Some(question);
    add_feedback_layers(slide, &theme, slide_size);
    let button = add_submit_button(slide, &theme, slide_size);
    keep_valid(s, was_valid)?;
    Ok(json!({ "button": button }))
}

pub(super) fn question_add_submit_button(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "question.add_submit_button";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let theme = s.course.theme.clone();
    let slide_size = s.course.slide_size;
    let slide = slide_mut(s, &sid)?;
    if slide.question.is_none() {
        return Err(EngineError::NotFound(format!(
            "slide {sid:?} has no question"
        )));
    }
    let button = add_submit_button(slide, &theme, slide_size);
    keep_valid(s, was_valid)?;
    Ok(json!({ "button": button }))
}

pub(super) fn question_set(s: &mut Session, p: &Json) -> Result<Json> {
    let was_valid = s.is_playable();
    let sid: String = req("question.set", p, "slide")?;
    let question: Question = req("question.set", p, "question")?;
    let slide = slide_mut(s, &sid)?;
    slide.question = Some(question);
    let graded = slide
        .triggers
        .iter()
        .chain(slide.layers.iter().flat_map(|l| l.triggers.iter()))
        .any(|t| t.action == Action::SubmitQuestion);
    if !graded {
        slide.triggers.push(Trigger {
            action: Action::SubmitQuestion,
            when: Event::Submit,
            condition: None,
            disabled: false,
        });
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn question_of<'a>(s: &'a mut Session, sid: &str) -> Result<&'a mut Question> {
    let slide = slide_mut(s, sid)?;
    slide
        .question
        .as_mut()
        .ok_or_else(|| EngineError::NotFound(format!("slide {sid:?} has no question")))
}

fn require_kind(q: &Question, kind: QuestionKind, cmd: &str) -> Result<()> {
    if q.kind == kind {
        Ok(())
    } else {
        Err(bad(cmd, format!("{cmd} does not apply to this question")))
    }
}

fn index_of(cmd: &str, p: &Json, len: usize) -> Result<usize> {
    let raw: u64 = req(cmd, p, "index")?;
    let idx = usize::try_from(raw).map_err(|_| bad(cmd, "`index` is out of range"))?;
    if idx >= len {
        return Err(bad(cmd, "`index` is out of range"));
    }
    Ok(idx)
}

fn dest_index(cmd: &str, p: &Json, len: usize) -> Result<usize> {
    let raw: u64 = req(cmd, p, "to")?;
    let idx = usize::try_from(raw).map_err(|_| bad(cmd, "`to` is out of range"))?;
    if idx >= len {
        return Err(bad(cmd, "`to` is out of range"));
    }
    Ok(idx)
}

fn fresh_choice_id(choices: &[Choice]) -> String {
    for ch in 'a'..='z' {
        let id = ch.to_string();
        if !choices.iter().any(|c| c.id == id) {
            return id;
        }
    }
    fresh_id("choice_", choices.iter().map(|c| c.id.as_str()))
}

fn push_choice(choices: &mut Vec<Choice>, label: &str, correct: bool) {
    let n = choices.len().saturating_add(1);
    let id = fresh_choice_id(choices);
    choices.push(Choice {
        id,
        text: format!("{label} {n}"),
        correct,
        points: 0,
    });
}

fn base_object<'a>(slide: &'a Slide, id: &str) -> Option<&'a Object> {
    match slide.object(id) {
        Some((object, None)) => Some(object),
        _ => None,
    }
}

fn clear_kind_payload(q: &mut Question) {
    q.accepted.clear();
    q.pairs.clear();
    q.case_sensitive = false;
    q.hotspot_image = None;
    q.regions.clear();
    q.markers = 1;
    q.drag_items.clear();
    q.drop_targets.clear();
    q.snap_to_center = true;
    q.return_if_wrong = false;
    q.one_per_target = true;
    q.correct_when = None;
}

fn keep_one_correct(choices: &mut [Choice]) {
    let mut seen = false;
    for c in choices.iter_mut() {
        if c.correct && !seen {
            seen = true;
        } else {
            c.correct = false;
        }
    }
    if !seen && let Some(first) = choices.first_mut() {
        first.correct = true;
    }
}

fn keep_some_correct(choices: &mut [Choice]) {
    if choices.iter().any(|c| c.correct) {
        return;
    }
    if let Some(first) = choices.first_mut() {
        first.correct = true;
    }
}

pub(super) fn apply_question_kind(
    base_objects: &[Object],
    q: &mut Question,
    kind: QuestionKind,
) -> Result<()> {
    if q.kind == kind {
        return Ok(());
    }
    match kind {
        QuestionKind::TrueFalse => {
            q.choices = vec![
                Choice {
                    id: "true".into(),
                    text: "True".into(),
                    correct: true,
                    points: 0,
                },
                Choice {
                    id: "false".into(),
                    text: "False".into(),
                    correct: false,
                    points: 0,
                },
            ];
            clear_kind_payload(q);
        }
        QuestionKind::FillIn => {
            q.choices.clear();
            clear_kind_payload(q);
            q.accepted = vec!["answer".into()];
            q.shuffle = false;
        }
        QuestionKind::Sequence => {
            for c in &mut q.choices {
                c.correct = false;
            }
            clear_kind_payload(q);
            q.shuffle = false;
            while q.choices.len() < 2 {
                push_choice(&mut q.choices, "Item", false);
            }
            if q.choices.len() > 20 {
                q.choices.truncate(20);
            }
        }
        QuestionKind::Matching => {
            q.choices.clear();
            clear_kind_payload(q);
            q.shuffle = false;
            q.pairs = vec![
                MatchPair {
                    id: "p1".into(),
                    left: "Left 1".into(),
                    right: "Right 1".into(),
                },
                MatchPair {
                    id: "p2".into(),
                    left: "Left 2".into(),
                    right: "Right 2".into(),
                },
            ];
        }
        QuestionKind::MultipleChoice => {
            clear_kind_payload(q);
            while q.choices.len() < 2 {
                let correct = !q.choices.iter().any(|c| c.correct);
                push_choice(&mut q.choices, "Choice", correct);
            }
            keep_one_correct(&mut q.choices);
        }
        QuestionKind::MultipleResponse => {
            clear_kind_payload(q);
            while q.choices.len() < 2 {
                let correct = !q.choices.iter().any(|c| c.correct);
                push_choice(&mut q.choices, "Choice", correct);
            }
            keep_some_correct(&mut q.choices);
        }
        QuestionKind::Hotspot => {
            let img = base_objects
                .iter()
                .find(|o| matches!(o.kind, ObjectKind::Image { .. }));
            let Some(img) = img else {
                return Err(EngineError::Invalid(
                    "hotspot requires at least one base-layer image on the slide".into(),
                ));
            };
            q.choices.clear();
            clear_kind_payload(q);
            let cx = img.frame.x + img.frame.w * 0.5;
            let cy = img.frame.y + img.frame.h * 0.5;
            let rx = (cx - 60.0).max(0.0);
            let ry = (cy - 40.0).max(0.0);
            q.hotspot_image = Some(img.id.clone());
            q.regions = vec![HotspotRegion {
                id: "r1".into(),
                shape: HotspotShape::Rect,
                x: rx,
                y: ry,
                w: 120.0,
                h: 80.0,
                correct: true,
            }];
            q.markers = 1;
            q.shuffle = false;
        }
        QuestionKind::DragDrop => {
            if base_objects.len() < 2 {
                return Err(EngineError::Invalid(
                    "drag_drop requires at least two base-layer objects on the slide".into(),
                ));
            }
            let by_name = |name: &str| {
                base_objects
                    .iter()
                    .find(|o| o.name == name)
                    .map(|o| o.id.clone())
            };
            let item_id = by_name("Drag me")
                .or_else(|| base_objects.first().map(|o| o.id.clone()))
                .ok_or_else(|| EngineError::Invalid("drag_drop requires a drag item".into()))?;
            let target_id = by_name("Drop target")
                .or_else(|| {
                    base_objects
                        .iter()
                        .map(|o| o.id.clone())
                        .find(|id| id != &item_id)
                })
                .ok_or_else(|| EngineError::Invalid("drag_drop requires a drop target".into()))?;
            q.choices.clear();
            clear_kind_payload(q);
            q.drag_items = vec![DragItem {
                item: item_id,
                target: target_id.clone(),
            }];
            q.drop_targets = vec![target_id];
            q.snap_to_center = true;
            q.return_if_wrong = false;
            q.one_per_target = true;
            q.shuffle = false;
        }
        QuestionKind::Likert => {
            q.choices = vec![
                Choice {
                    id: "1".into(),
                    text: "Strongly disagree".into(),
                    correct: false,
                    points: 0,
                },
                Choice {
                    id: "2".into(),
                    text: "Disagree".into(),
                    correct: false,
                    points: 0,
                },
                Choice {
                    id: "3".into(),
                    text: "Neutral".into(),
                    correct: false,
                    points: 0,
                },
                Choice {
                    id: "4".into(),
                    text: "Agree".into(),
                    correct: false,
                    points: 0,
                },
                Choice {
                    id: "5".into(),
                    text: "Strongly agree".into(),
                    correct: false,
                    points: 0,
                },
            ];
            clear_kind_payload(q);
            q.shuffle = false;
        }
        QuestionKind::ShortAnswer | QuestionKind::Essay => {
            q.choices.clear();
            clear_kind_payload(q);
            q.shuffle = false;
        }
        QuestionKind::Freeform => {
            q.choices.clear();
            clear_kind_payload(q);
            q.shuffle = false;
            q.correct_when = Some(Condition::Compare {
                variable: "Course.ScorePercent".into(),
                op: CmpOp::Ge,
                value: Value::Number(0.0).into(),
            });
        }
    }
    q.kind = kind;
    // Reset scoring if it is no longer allowed for the new kind.
    match q.scoring {
        Scoring::PerAnswer
            if !matches!(
                kind,
                QuestionKind::MultipleChoice
                    | QuestionKind::MultipleResponse
                    | QuestionKind::TrueFalse
            ) =>
        {
            q.scoring = Scoring::AllOrNothing;
        }
        Scoring::Partial
            if !matches!(
                kind,
                QuestionKind::MultipleResponse
                    | QuestionKind::Sequence
                    | QuestionKind::Matching
                    | QuestionKind::Hotspot
                    | QuestionKind::DragDrop
            ) =>
        {
            q.scoring = Scoring::AllOrNothing;
        }
        _ => {}
    }
    Ok(())
}

pub(super) fn move_to<T>(items: &mut Vec<T>, from: usize, to: usize) {
    if from >= items.len() || to >= items.len() {
        return;
    }
    let item = items.remove(from);
    let dest = to.min(items.len());
    items.insert(dest, item);
}

pub(super) fn question_set_kind(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "question.set_kind";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let kind: QuestionKind = req(C, p, "kind")?;
    let freeform_default = freeform_default_condition(&s.course);
    let slide = slide_mut(s, &sid)?;
    let q = slide
        .question
        .as_mut()
        .ok_or_else(|| EngineError::NotFound(format!("slide {sid:?} has no question")))?;
    apply_question_kind(&slide.objects, q, kind)?;
    if kind == QuestionKind::Freeform {
        q.correct_when = Some(freeform_default);
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn question_add_accepted(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "question.add_accepted";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let value = match opt::<String>(C, p, "text")? {
        Some(t) => text(C, t, "text")?,
        None => "answer".to_owned(),
    };
    {
        let q = question_of(s, &sid)?;
        require_kind(q, QuestionKind::FillIn, C)?;
        q.accepted.push(value);
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn question_remove_accepted(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "question.remove_accepted";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    {
        let q = question_of(s, &sid)?;
        require_kind(q, QuestionKind::FillIn, C)?;
        let idx = index_of(C, p, q.accepted.len())?;
        q.accepted.remove(idx);
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn question_update_accepted(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "question.update_accepted";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let value = text(C, req(C, p, "text")?, "text")?;
    {
        let q = question_of(s, &sid)?;
        require_kind(q, QuestionKind::FillIn, C)?;
        let idx = index_of(C, p, q.accepted.len())?;
        if let Some(slot) = q.accepted.get_mut(idx) {
            *slot = value;
        }
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn question_set_case_sensitive(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "question.set_case_sensitive";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let flag: bool = req(C, p, "case_sensitive")?;
    {
        let q = question_of(s, &sid)?;
        require_kind(q, QuestionKind::FillIn, C)?;
        q.case_sensitive = flag;
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn question_add_pair(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "question.add_pair";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let left = match opt::<String>(C, p, "left")? {
        Some(v) => text(C, v, "left")?,
        None => "Left".to_owned(),
    };
    let right = match opt::<String>(C, p, "right")? {
        Some(v) => text(C, v, "right")?,
        None => "Right".to_owned(),
    };
    let id = {
        let q = question_of(s, &sid)?;
        require_kind(q, QuestionKind::Matching, C)?;
        let id = fresh_id("p", q.pairs.iter().map(|pair| pair.id.as_str()));
        q.pairs.push(MatchPair {
            id: id.clone(),
            left,
            right,
        });
        id
    };
    keep_valid(s, was_valid)?;
    Ok(json!({ "id": id }))
}

pub(super) fn question_remove_pair(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "question.remove_pair";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let id: String = req(C, p, "id")?;
    {
        let q = question_of(s, &sid)?;
        require_kind(q, QuestionKind::Matching, C)?;
        let Some(pos) = q.pairs.iter().position(|pair| pair.id == id) else {
            return Err(EngineError::NotFound(format!("pair {id:?} does not exist")));
        };
        q.pairs.remove(pos);
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn question_update_pair(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "question.update_pair";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let id: String = req(C, p, "id")?;
    let left: Option<String> = opt(C, p, "left")?;
    let right: Option<String> = opt(C, p, "right")?;
    if left.is_none() && right.is_none() {
        return Err(bad(C, "missing `left` or `right`"));
    }
    let left = left.map(|v| text(C, v, "left")).transpose()?;
    let right = right.map(|v| text(C, v, "right")).transpose()?;
    {
        let q = question_of(s, &sid)?;
        require_kind(q, QuestionKind::Matching, C)?;
        let Some(pair) = q.pairs.iter_mut().find(|pair| pair.id == id) else {
            return Err(EngineError::NotFound(format!("pair {id:?} does not exist")));
        };
        if let Some(v) = left {
            pair.left = v;
        }
        if let Some(v) = right {
            pair.right = v;
        }
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn question_reorder_pair(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "question.reorder_pair";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let id: String = req(C, p, "id")?;
    {
        let q = question_of(s, &sid)?;
        require_kind(q, QuestionKind::Matching, C)?;
        let to = dest_index(C, p, q.pairs.len())?;
        let Some(from) = q.pairs.iter().position(|pair| pair.id == id) else {
            return Err(EngineError::NotFound(format!("pair {id:?} does not exist")));
        };
        move_to(&mut q.pairs, from, to);
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn question_reorder_sequence(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "question.reorder_sequence";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let id: String = req(C, p, "id")?;
    {
        let q = question_of(s, &sid)?;
        require_kind(q, QuestionKind::Sequence, C)?;
        let to = dest_index(C, p, q.choices.len())?;
        let Some(from) = q.choices.iter().position(|c| c.id == id) else {
            return Err(EngineError::NotFound(format!(
                "choice {id:?} does not exist"
            )));
        };
        move_to(&mut q.choices, from, to);
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn question_set_hotspot_image(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "question.set_hotspot_image";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let image: String = req(C, p, "image")?;
    let slide = slide_mut(s, &sid)?;
    let obj = base_object(slide, &image)
        .ok_or_else(|| bad(C, format!("image {image:?} must name a base-layer object")))?;
    if !matches!(obj.kind, ObjectKind::Image { .. }) {
        return Err(bad(C, format!("object {image:?} is not an image")));
    }
    let q = slide
        .question
        .as_mut()
        .ok_or_else(|| EngineError::NotFound(format!("slide {sid:?} has no question")))?;
    require_kind(q, QuestionKind::Hotspot, C)?;
    q.hotspot_image = Some(image);
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn question_add_region(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "question.add_region";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let shape = opt::<HotspotShape>(C, p, "shape")?.unwrap_or(HotspotShape::Rect);
    let slide_size = s.course.slide_size;
    let slide = slide_mut(s, &sid)?;
    let (default_x, default_y) = if let Some(q) = &slide.question
        && let Some(ref img_id) = q.hotspot_image
        && let Some(img) = base_object(slide, img_id)
    {
        let cx = img.frame.x + img.frame.w * 0.5;
        let cy = img.frame.y + img.frame.h * 0.5;
        ((cx - 60.0).max(0.0), (cy - 40.0).max(0.0))
    } else {
        (
            ((slide_size.w as f32) * 0.5 - 60.0).max(0.0),
            ((slide_size.h as f32) * 0.5 - 40.0).max(0.0),
        )
    };
    let x = opt::<f32>(C, p, "x")?.unwrap_or(default_x);
    let y = opt::<f32>(C, p, "y")?.unwrap_or(default_y);
    let w = opt::<f32>(C, p, "w")?.unwrap_or(120.0);
    let h = opt::<f32>(C, p, "h")?.unwrap_or(80.0);

    if !x.is_finite()
        || !y.is_finite()
        || !w.is_finite()
        || !h.is_finite()
        || x < -100_000.0
        || x > 100_000.0
        || y < -100_000.0
        || y > 100_000.0
        || w <= 0.0
        || w > 100_000.0
        || h <= 0.0
        || h > 100_000.0
    {
        return Err(bad(C, "invalid coordinates or dimensions"));
    }

    let q = slide
        .question
        .as_mut()
        .ok_or_else(|| EngineError::NotFound(format!("slide {sid:?} has no question")))?;
    require_kind(q, QuestionKind::Hotspot, C)?;
    if q.regions.len() >= 20 {
        return Err(bad(C, "cannot exceed 20 regions"));
    }
    let correct = opt::<bool>(C, p, "correct")?
        .unwrap_or(q.regions.is_empty() || !q.regions.iter().any(|r| r.correct));
    let id = fresh_id("r", q.regions.iter().map(|r| r.id.as_str()));
    q.regions.push(HotspotRegion {
        id: id.clone(),
        shape,
        x,
        y,
        w,
        h,
        correct,
    });
    keep_valid(s, was_valid)?;
    Ok(json!({ "id": id }))
}

pub(super) fn question_update_region(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "question.update_region";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let id: String = req(C, p, "id")?;
    let shape: Option<HotspotShape> = opt(C, p, "shape")?;
    let x: Option<f32> = opt(C, p, "x")?;
    let y: Option<f32> = opt(C, p, "y")?;
    let w: Option<f32> = opt(C, p, "w")?;
    let h: Option<f32> = opt(C, p, "h")?;
    let correct: Option<bool> = opt(C, p, "correct")?;

    for v in [&x, &y].into_iter().flatten() {
        if !v.is_finite() || *v < -100_000.0 || *v > 100_000.0 {
            return Err(bad(C, "coordinate out of range or not finite"));
        }
    }
    for v in [&w, &h].into_iter().flatten() {
        if !v.is_finite() || *v <= 0.0 || *v > 100_000.0 {
            return Err(bad(C, "dimension must be > 0 and <= 100000"));
        }
    }

    let q = question_of(s, &sid)?;
    require_kind(q, QuestionKind::Hotspot, C)?;
    let Some(pos) = q.regions.iter().position(|r| r.id == id) else {
        return Err(EngineError::NotFound(format!(
            "region {id:?} does not exist"
        )));
    };
    if correct == Some(false)
        && q.regions[pos].correct
        && q.regions.iter().filter(|r| r.correct).count() <= 1
    {
        return Err(bad(C, "hotspot needs at least one correct region"));
    }
    let region = &mut q.regions[pos];
    if let Some(sh) = shape {
        region.shape = sh;
    }
    if let Some(v) = x {
        region.x = v;
    }
    if let Some(v) = y {
        region.y = v;
    }
    if let Some(v) = w {
        region.w = v;
    }
    if let Some(v) = h {
        region.h = v;
    }
    if let Some(c) = correct {
        region.correct = c;
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn question_remove_region(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "question.remove_region";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let id: String = req(C, p, "id")?;
    let q = question_of(s, &sid)?;
    require_kind(q, QuestionKind::Hotspot, C)?;
    if q.regions.len() <= 1 {
        return Err(bad(C, "hotspot needs at least one region"));
    }
    let Some(pos) = q.regions.iter().position(|r| r.id == id) else {
        return Err(EngineError::NotFound(format!(
            "region {id:?} does not exist"
        )));
    };
    if q.regions[pos].correct && q.regions.iter().filter(|r| r.correct).count() <= 1 {
        return Err(bad(C, "cannot remove the only correct region"));
    }
    q.regions.remove(pos);
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn question_set_markers(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "question.set_markers";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let markers: u32 = req(C, p, "markers")?;
    if !(1..=10).contains(&markers) {
        return Err(bad(C, "markers must be between 1 and 10"));
    }
    let q = question_of(s, &sid)?;
    require_kind(q, QuestionKind::Hotspot, C)?;
    q.markers = markers;
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn question_add_drag_item(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "question.add_drag_item";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let item: String = req(C, p, "item")?;
    let target: String = req(C, p, "target")?;
    if item == target {
        return Err(bad(C, "drag item cannot target itself"));
    }
    let slide = slide_mut(s, &sid)?;
    if base_object(slide, &item).is_none() {
        return Err(bad(C, format!("item {item:?} is not a base-layer object")));
    }
    if base_object(slide, &target).is_none() {
        return Err(bad(
            C,
            format!("target {target:?} is not a base-layer object"),
        ));
    }
    let q = slide
        .question
        .as_mut()
        .ok_or_else(|| EngineError::NotFound(format!("slide {sid:?} has no question")))?;
    require_kind(q, QuestionKind::DragDrop, C)?;
    if q.drag_items.len() >= 20 {
        return Err(bad(C, "cannot exceed 20 drag items"));
    }
    if q.drag_items.iter().any(|d| d.item == item) {
        return Err(bad(C, format!("drag item {item:?} already exists")));
    }
    if q.drag_items.iter().any(|d| d.item == target) {
        return Err(bad(C, format!("target {target:?} is a drag item")));
    }
    if q.drop_targets.contains(&item) {
        return Err(bad(C, format!("item {item:?} is already a drop target")));
    }
    if !q.drop_targets.contains(&target) {
        if q.drop_targets.len() >= 20 {
            return Err(bad(C, "cannot exceed 20 drop targets"));
        }
        q.drop_targets.push(target.clone());
    }
    q.drag_items.push(DragItem { item, target });
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn question_update_drag_item(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "question.update_drag_item";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let item: String = req(C, p, "item")?;
    let target: String = req(C, p, "target")?;
    if item == target {
        return Err(bad(C, "drag item cannot target itself"));
    }
    let slide = slide_mut(s, &sid)?;
    if base_object(slide, &target).is_none() {
        return Err(bad(
            C,
            format!("target {target:?} is not a base-layer object"),
        ));
    }
    let q = slide
        .question
        .as_mut()
        .ok_or_else(|| EngineError::NotFound(format!("slide {sid:?} has no question")))?;
    require_kind(q, QuestionKind::DragDrop, C)?;
    if q.drag_items.iter().any(|d| d.item == target) {
        return Err(bad(C, format!("target {target:?} is a drag item")));
    }
    let Some(drag) = q.drag_items.iter_mut().find(|d| d.item == item) else {
        return Err(EngineError::NotFound(format!(
            "drag item {item:?} does not exist"
        )));
    };
    drag.target = target.clone();
    if !q.drop_targets.contains(&target) {
        if q.drop_targets.len() >= 20 {
            return Err(bad(C, "cannot exceed 20 drop targets"));
        }
        q.drop_targets.push(target);
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn question_remove_drag_item(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "question.remove_drag_item";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let item: String = req(C, p, "item")?;
    let q = question_of(s, &sid)?;
    require_kind(q, QuestionKind::DragDrop, C)?;
    if q.drag_items.len() <= 1 {
        return Err(bad(C, "drag_drop needs at least one drag item"));
    }
    let Some(pos) = q.drag_items.iter().position(|d| d.item == item) else {
        return Err(EngineError::NotFound(format!(
            "drag item {item:?} does not exist"
        )));
    };
    q.drag_items.remove(pos);
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn question_set_drop_targets(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "question.set_drop_targets";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let targets: Vec<String> = req(C, p, "targets")?;
    if targets.is_empty() || targets.len() > 20 {
        return Err(bad(C, "drop_targets must have 1 to 20 targets"));
    }
    let mut set = std::collections::BTreeSet::new();
    for t in &targets {
        if !set.insert(t.as_str()) {
            return Err(bad(C, format!("duplicate drop target {t:?}")));
        }
    }
    let slide = slide_mut(s, &sid)?;
    for t in &targets {
        if base_object(slide, t).is_none() {
            return Err(bad(
                C,
                format!("drop target {t:?} is not a base-layer object"),
            ));
        }
    }
    let q = slide
        .question
        .as_mut()
        .ok_or_else(|| EngineError::NotFound(format!("slide {sid:?} has no question")))?;
    require_kind(q, QuestionKind::DragDrop, C)?;
    for d in &q.drag_items {
        if targets.contains(&d.item) {
            return Err(bad(
                C,
                format!(
                    "object {:?} cannot be both drag item and drop target",
                    d.item
                ),
            ));
        }
        if !targets.contains(&d.target) {
            return Err(bad(
                C,
                format!(
                    "target {:?} used by drag item {:?} must be in drop_targets",
                    d.target, d.item
                ),
            ));
        }
    }
    q.drop_targets = targets;
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn question_set_drag_options(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "question.set_drag_options";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let snap_to_center: Option<bool> = opt(C, p, "snap_to_center")?;
    let return_if_wrong: Option<bool> = opt(C, p, "return_if_wrong")?;
    let one_per_target: Option<bool> = opt(C, p, "one_per_target")?;

    let q = question_of(s, &sid)?;
    require_kind(q, QuestionKind::DragDrop, C)?;
    if let Some(v) = snap_to_center {
        q.snap_to_center = v;
    }
    if let Some(v) = return_if_wrong {
        q.return_if_wrong = v;
    }
    if let Some(v) = one_per_target {
        q.one_per_target = v;
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

/// True when a trigger only makes sense on a slide that has a question.
///
/// Validation rejects `submit_question` on a slide without a question, and a
/// `submit` event can never fire once the question is gone.
fn trigger_needs_question(t: &Trigger) -> bool {
    t.action == Action::SubmitQuestion || t.when == Event::Submit
}

fn condition_references_object(c: &Condition, object: &str) -> bool {
    match c {
        Condition::All(list) | Condition::Any(list) => {
            list.iter().any(|c| condition_references_object(c, object))
        }
        Condition::Not(inner) => condition_references_object(inner, object),
        Condition::Compare {
            variable, value, ..
        } => [Some(variable.as_str()), value.as_variable()]
            .into_iter()
            .flatten()
            .any(|name| coursecraft_course::timer_id_for_remaining_variable(name) == Some(object)),
        Condition::ObjectState { object: target, .. } => target == object,
    }
}

/// Every object a trigger names in its event, action or condition.
fn trigger_references_object(t: &Trigger, object: &str) -> bool {
    let in_event = match &t.when {
        Event::Click { object: target }
        | Event::HoverStart { object: target }
        | Event::HoverEnd { object: target }
        | Event::MediaEnds { object: target }
        | Event::MarkerOpened { object: target }
        | Event::MarkerClosed { object: target }
        | Event::CueReached { object: target, .. }
        | Event::StateChanges { object: target, .. }
        | Event::AnimationCompletes { object: target, .. }
        | Event::TimerEnds { timer: target } => target == object,
        Event::KeyPressed {
            object: Some(target),
            ..
        } => target == object,
        Event::DroppedOn {
            object: first,
            target: second,
        }
        | Event::ObjectsIntersect {
            a: first,
            b: second,
        }
        | Event::ObjectsStopIntersecting {
            a: first,
            b: second,
        } => first == object || second == object,
        Event::VariableChanges { variable } => {
            coursecraft_course::timer_id_for_remaining_variable(variable) == Some(object)
        }
        _ => false,
    };
    let in_action = match &t.action {
        Action::ChangeState { object: target, .. }
        | Action::PlayMedia { object: target }
        | Action::PauseMedia { object: target }
        | Action::StopMedia { object: target }
        | Action::StartTimer { timer: target }
        | Action::PauseTimer { timer: target }
        | Action::ResetTimer { timer: target }
        | Action::MoveOnPath { object: target, .. } => target == object,
        _ => false,
    };
    in_event
        || in_action
        || t.condition
            .as_ref()
            .is_some_and(|c| condition_references_object(c, object))
}

/// The object id a trigger's event fires from, when the event has one.
fn event_object(event: &Event) -> Option<&str> {
    match event {
        Event::Click { object }
        | Event::HoverStart { object }
        | Event::HoverEnd { object }
        | Event::MediaEnds { object }
        | Event::StateChanges { object, .. }
        | Event::AnimationCompletes { object, .. } => Some(object),
        _ => None,
    }
}

/// Remove a question and everything that cannot exist without it, in one edit:
/// submit triggers, dead submit events, and Submit buttons that have nothing
/// else to do. Layers, feedback layers and all other objects stay.
pub(super) fn question_delete(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "question.delete";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let slide = slide_mut(s, &sid)?;
    if slide.question.is_none() {
        return Err(EngineError::NotFound(format!(
            "slide {sid:?} has no question"
        )));
    }
    slide.question = None;

    // Objects named by submit_question triggers may have been there only to
    // submit. Remember them before the triggers go.
    let mut submit_objects: BTreeSet<String> = BTreeSet::new();
    for trigger in slide
        .triggers
        .iter()
        .chain(slide.layers.iter().flat_map(|layer| layer.triggers.iter()))
    {
        if trigger.action == Action::SubmitQuestion
            && let Some(object) = event_object(&trigger.when)
        {
            submit_objects.insert(object.to_owned());
        }
    }

    slide.triggers.retain(|t| !trigger_needs_question(t));
    for layer in &mut slide.layers {
        layer.triggers.retain(|t| !trigger_needs_question(t));
    }

    // A button that had a submit_question trigger and now has no trigger (and
    // is not named by any other trigger) was only there to submit.
    let mut lonely_buttons: BTreeSet<String> = BTreeSet::new();
    let all_objects = slide
        .objects
        .iter()
        .chain(slide.layers.iter().flat_map(|layer| layer.objects.iter()));
    for object in all_objects {
        if !submit_objects.contains(&object.id) || !matches!(object.kind, ObjectKind::Button { .. })
        {
            continue;
        }
        let referenced = slide
            .triggers
            .iter()
            .chain(slide.layers.iter().flat_map(|layer| layer.triggers.iter()));
        if !referenced
            .into_iter()
            .any(|t| trigger_references_object(t, &object.id))
        {
            lonely_buttons.insert(object.id.clone());
        }
    }
    slide
        .objects
        .retain(|object| !lonely_buttons.contains(&object.id));
    for layer in &mut slide.layers {
        layer
            .objects
            .retain(|object| !lonely_buttons.contains(&object.id));
    }

    // Feedback layers made for this question, if nobody edited them or points at them.
    let unused_feedback: Vec<String> = slide
        .layers
        .iter()
        .filter(|layer| is_untouched_feedback_layer(layer))
        .map(|layer| layer.id.clone())
        .filter(|id| {
            !slide
                .triggers
                .iter()
                .chain(slide.layers.iter().flat_map(|layer| layer.triggers.iter()))
                .any(|t| {
                    matches!(&t.action, Action::ShowLayer { layer } if layer == id)
                        || matches!(&t.action, Action::HideLayer { layer: Some(layer) } if layer == id)
                        || matches!(
                            &t.when,
                            Event::TimelineStarts { layer: Some(layer) }
                                | Event::TimelineEnds { layer: Some(layer) }
                                | Event::TimelineReaches { layer: Some(layer), .. }
                            if layer == id
                        )
                })
        })
        .collect();
    slide
        .layers
        .retain(|layer| !unused_feedback.contains(&layer.id));

    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

/// A `Correct` or `Incorrect` layer still exactly as `add_feedback_layers` made it.
fn is_untouched_feedback_layer(layer: &Layer) -> bool {
    let expected = match layer.name.as_str() {
        "Correct" => "Correct!",
        "Incorrect" => "Not quite.",
        _ => return false,
    };
    layer.triggers.is_empty()
        && matches!(
            layer.objects.as_slice(),
            [Object { kind: ObjectKind::Text { text, .. }, .. }] if text == expected
        )
}
