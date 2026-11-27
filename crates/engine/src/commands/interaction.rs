//! Built-in interaction template insertion commands.

use coursecraft_course::{
    Action, Align, Autofit, CmpOp, Color, Condition, DashStyle, Event, Layer, Object, ObjectKind,
    Outline, Rect, RevisitMode, SeekMode, ShapeKind, ShapeText, Slide, StateDef, TextStyle, Theme,
    Timing, Trigger, VAlign, Value, VarOp, VariableDef,
};
use serde_json::{Value as Json, json};

use super::{bad, fresh_explored_variable, fresh_id, keep_valid, req, slide_mut};
use crate::{EngineError, Result, Session};

/// A plain text object for an inserted interaction.
pub(super) fn interaction_text(
    id: String,
    name: String,
    text: String,
    frame: Rect,
    theme: &Theme,
) -> Object {
    Object {
        id,
        name,
        kind: ObjectKind::Text {
            text,
            style: TextStyle {
                size: 24.0,
                color: theme.text.clone(),
                ..TextStyle::default()
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
        entrance: None,
        exit: None,
        motion_paths: Vec::new(),
        emphasis: Vec::new(),
        states: Vec::new(),
        initial_state: "normal".into(),
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

/// A button with a `selected` state in the theme accent.
pub(super) fn interaction_button(
    id: String,
    name: String,
    label: String,
    frame: Rect,
    selected: bool,
    theme: &Theme,
) -> Object {
    // Button labels are white, so the unselected fill must stay dark enough to read.
    let normal = tint(theme.text.clone(), 0x88);
    Object {
        id,
        name,
        kind: ObjectKind::Button {
            label,
            fill: normal,
            fill_style: None,
        },
        frame,
        timing: Timing::default(),
        entrance: None,
        exit: None,
        motion_paths: Vec::new(),
        emphasis: Vec::new(),
        states: vec![StateDef {
            name: "selected".into(),
            fill: Some(theme.accent.clone()),
            ..Default::default()
        }],
        initial_state: if selected {
            "selected".into()
        } else {
            "normal".into()
        },
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

/// A theme colour with its alpha replaced, so normal states stay readable on
/// any background without inventing a new palette.
fn tint(color: Color, alpha: u8) -> Color {
    let hex = color.0.strip_prefix('#').unwrap_or("");
    let first_six: String = hex.chars().take(6).collect();
    if first_six.len() == 6 && first_six.chars().all(|c| c.is_ascii_hexdigit()) {
        Color(format!("#{first_six}{alpha:02X}"))
    } else {
        color
    }
}

pub(super) fn interaction_trigger(action: Action, when: Event) -> Trigger {
    Trigger {
        action,
        when,
        condition: None,
        disabled: false,
    }
}

/// A trigger that only fires when `condition` holds.
fn interaction_conditional_trigger(action: Action, when: Event, condition: Condition) -> Trigger {
    Trigger {
        action,
        when,
        condition: Some(condition),
        disabled: false,
    }
}

/// A shape with optional centred text and a caller-supplied set of states.
#[allow(clippy::too_many_arguments)]
fn interaction_shape(
    id: String,
    name: String,
    shape: ShapeKind,
    fill: Color,
    outline: Option<Outline>,
    text: Option<(String, f32, bool)>,
    states: Vec<StateDef>,
    frame: Rect,
    theme: &Theme,
) -> Object {
    Object {
        id,
        name,
        kind: ObjectKind::Shape {
            shape,
            fill,
            fill_style: None,
            outline,
            text: text.map(|(text, size, bold)| ShapeText {
                text,
                style: TextStyle {
                    font: theme.font.clone(),
                    size,
                    color: theme.text.clone(),
                    bold,
                    align: Align::Center,
                    valign: VAlign::Middle,
                    ..TextStyle::default()
                },
                runs: Vec::new(),
                paragraphs: Vec::new(),
                autofit: Autofit::None,
            }),
            adjust: Vec::new(),
        },
        frame,
        timing: Timing::default(),
        entrance: None,
        exit: None,
        motion_paths: Vec::new(),
        emphasis: Vec::new(),
        states,
        initial_state: "normal".into(),
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

/// A button with no selected state, so a click never leaves it highlighted.
fn interaction_plain_button(
    id: String,
    name: String,
    label: String,
    frame: Rect,
    theme: &Theme,
) -> Object {
    Object {
        id,
        name,
        kind: ObjectKind::Button {
            label,
            fill: theme.accent.clone(),
            fill_style: None,
        },
        frame,
        timing: Timing::default(),
        entrance: None,
        exit: None,
        motion_paths: Vec::new(),
        emphasis: Vec::new(),
        states: Vec::new(),
        initial_state: "normal".into(),
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

/// A callout panel with a title and one line of short, replaceable detail.
fn interaction_panel_objects(
    slide: &Slide,
    prefix: &str,
    label: &str,
    frame: Rect,
    theme: &Theme,
) -> Vec<Object> {
    let panel_id = fresh_id(prefix, slide.local_ids());
    let title_id = fresh_id(&format!("{prefix}_title"), slide.local_ids());
    let detail_id = fresh_id(&format!("{prefix}_detail"), slide.local_ids());
    let panel = interaction_shape(
        panel_id,
        format!("{label} panel"),
        ShapeKind::RoundedRectangle,
        tint(theme.accent.clone(), 0x22),
        Some(Outline {
            color: theme.accent.clone(),
            width: 2.0,
            dash: DashStyle::Solid,
            dash_pattern: None,
            join: Default::default(),
            miter_limit: None,
        }),
        None,
        Vec::new(),
        frame,
        theme,
    );
    let mut title = interaction_text(
        title_id,
        format!("{label} title"),
        label.to_owned(),
        Rect {
            x: frame.x + 16.0,
            y: frame.y + 10.0,
            w: (frame.w - 32.0).max(20.0),
            h: 36.0,
        },
        theme,
    );
    if let ObjectKind::Text { style, .. } = &mut title.kind {
        style.size = 26.0;
        style.bold = true;
    }
    let mut detail = interaction_text(
        detail_id,
        format!("{label} detail"),
        "Add the detail here.".to_owned(),
        Rect {
            x: frame.x + 16.0,
            y: frame.y + 52.0,
            w: (frame.w - 32.0).max(20.0),
            h: (frame.h - 64.0).max(24.0),
        },
        theme,
    );
    if let ObjectKind::Text { style, .. } = &mut detail.kind {
        style.size = 20.0;
    }
    vec![panel, title, detail]
}

/// A layer whose callout replaces the other callouts when shown.
fn interaction_callout_layer(
    id: String,
    name: String,
    objects: Vec<Object>,
    triggers: Vec<Trigger>,
) -> Layer {
    Layer {
        id,
        name,
        duration_ms: 5000,
        objects,
        triggers,
        hide_other_layers: true,
        pause_base_timeline: false,
        prevent_base_clicks: false,
        hide_base_objects: false,
        hide_when_timeline_ends: false,
        allow_seek: SeekMode::Auto,
        revisit: RevisitMode::Auto,
        cue_points: Vec::new(),
        groups: Vec::new(),
    }
}

struct InteractionCtx<'a> {
    slide: &'a mut Slide,
    theme: &'a Theme,
    w: f32,
    h: f32,
    margin: f32,
    top: f32,
    gap: f32,
    base_ids: Vec<String>,
    layer_ids: Vec<String>,
    all_ids: Vec<String>,
}

pub(super) fn interaction_insert(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "interaction.insert";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    if s.course.play_slide(&sid).is_none() {
        return Err(EngineError::NotFound(format!(
            "slide {sid:?} is not a playable slide"
        )));
    }
    let kind: String = req(C, p, "kind")?;
    let theme = s.course.theme.clone();
    let size = s.course.slide_size;
    let (w, h) = (size.w as f32, size.h as f32);
    if !w.is_finite() || !h.is_finite() || w <= 0.0 || h <= 0.0 {
        return Err(bad(C, "slide size is invalid"));
    }

    // Variables are course-wide, so create them before borrowing the slide.
    let mut flip_variables: Vec<String> = Vec::new();
    if kind == "flip_cards" {
        for _ in 0..3 {
            let name = fresh_id(
                "flip_card",
                s.course.variables.iter().map(|v| v.name.as_str()),
            );
            s.course.variables.push(VariableDef {
                name: name.clone(),
                value: Value::Bool(false),
            });
            flip_variables.push(name);
        }
    }
    let step_variable = if kind == "process_steps" {
        let name = fresh_id(
            "process_step",
            s.course.variables.iter().map(|v| v.name.as_str()),
        );
        s.course.variables.push(VariableDef {
            name: name.clone(),
            value: Value::Number(1.0),
        });
        Some(name)
    } else {
        None
    };
    // The explored counter comes first, then one flag per tile so a second click
    // on a revealed tile cannot count it twice.
    let mut reveal_variables: Vec<String> = Vec::new();
    if kind == "reveal_grid" {
        let explored = fresh_explored_variable(&s.course, "grid");
        s.course.variables.push(VariableDef {
            name: explored.clone(),
            value: Value::Number(0.0),
        });
        reveal_variables.push(explored);
        for _ in 0..6 {
            let name = fresh_id(
                "grid_tile",
                s.course.variables.iter().map(|v| v.name.as_str()),
            );
            s.course.variables.push(VariableDef {
                name: name.clone(),
                value: Value::Bool(false),
            });
            reveal_variables.push(name);
        }
    }

    let slide = slide_mut(s, &sid)?;
    let mut ctx = InteractionCtx {
        slide,
        theme: &theme,
        w,
        h,
        // The floors keep a normal slide readable; the caps keep a tiny slide
        // (a side of a few pixels) from getting negative widths.
        margin: (w * 0.06).clamp(20.0, 120.0).min(w * 0.2),
        top: (h * 0.12).max(20.0).min(h * 0.2),
        gap: (w * 0.012).clamp(6.0, 20.0).min(w * 0.02),
        base_ids: Vec::new(),
        layer_ids: Vec::new(),
        all_ids: Vec::new(),
    };

    match kind.as_str() {
        "tabs" => insert_tabs(&mut ctx),
        "accordion" => insert_accordion(&mut ctx),
        "flip_cards" => insert_flip_cards(&mut ctx, &flip_variables),
        "hotspot_reveal" => insert_hotspot_reveal(&mut ctx),
        "process_steps" => {
            let Some(variable) = step_variable.as_deref() else {
                return Err(bad(C, "process_steps needs a step variable"));
            };
            insert_process_steps(&mut ctx, variable);
        }
        "timeline" => insert_timeline(&mut ctx),
        "reveal_grid" => {
            if reveal_variables.is_empty() {
                return Err(bad(C, "reveal_grid needs an explored variable"));
            }
            insert_reveal_grid(&mut ctx, &reveal_variables);
        }
        "scenario_choice" => insert_scenario_choice(&mut ctx),
        _ => {
            return Err(bad(
                C,
                "kind must be \"tabs\", \"accordion\", \"flip_cards\", \"hotspot_reveal\", \"process_steps\", \"timeline\", \"reveal_grid\" or \"scenario_choice\"",
            ));
        }
    }

    let InteractionCtx {
        base_ids,
        layer_ids,
        all_ids,
        ..
    } = ctx;

    keep_valid(s, was_valid)?;
    let mut variables = flip_variables;
    variables.extend(step_variable);
    variables.extend(reveal_variables);
    Ok(json!({
        "objects": base_ids,
        "layers": layer_ids,
        "all_objects": all_ids,
        "variables": variables,
    }))
}

fn insert_tabs(ctx: &mut InteractionCtx<'_>) {
    let InteractionCtx {
        slide,
        theme,
        w,
        h,
        margin,
        top,
        gap,
        base_ids,
        layer_ids,
        all_ids,
    } = ctx;
    let (w, h, margin, top, gap) = (*w, *h, *margin, *top, *gap);
    let theme = *theme;
    let count = 3usize;
    let tab_h = (h * 0.08).clamp(36.0, 64.0);
    let tab_w = ((w - 2.0 * margin - gap * (count as f32 - 1.0)) / count as f32).max(20.0);
    let content_y = top + tab_h + gap;
    let content_h = (h - content_y - top).max(40.0);
    let mut tab_ids = Vec::new();
    for i in 0..count {
        let x = margin + (tab_w + gap) * i as f32;
        let id = fresh_id("tab_button", slide.local_ids());
        tab_ids.push(id.clone());
        let button = interaction_button(
            id.clone(),
            format!("Tab {} button", i + 1),
            format!("Tab {}", i + 1),
            Rect {
                x,
                y: top,
                w: tab_w,
                h: tab_h,
            },
            i == 0,
            theme,
        );
        slide.objects.push(button);
        all_ids.push(id.clone());
        base_ids.push(id);
    }
    for i in 0..count {
        let layer_id = fresh_id("tab_layer", slide.local_ids());
        layer_ids.push(layer_id.clone());
        let text_id = fresh_id("tab_content", slide.local_ids());
        let text = interaction_text(
            text_id.clone(),
            format!("Tab {} content", i + 1),
            format!("Content for Tab {}", i + 1),
            Rect {
                x: margin,
                y: content_y,
                w: (w - 2.0 * margin).max(20.0),
                h: content_h,
            },
            theme,
        );
        all_ids.push(text_id.clone());
        slide.layers.push(Layer {
            id: layer_id.clone(),
            name: format!("Tab {} content", i + 1),
            duration_ms: 5000,
            objects: vec![text],
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
    }
    // Clicking a tab shows its layer and moves the selected state.
    for i in 0..count {
        let when = Event::Click {
            object: tab_ids.get(i).cloned().unwrap_or_default(),
        };
        if let Some(layer) = layer_ids.get(i) {
            slide.triggers.push(interaction_trigger(
                Action::ShowLayer {
                    layer: layer.clone(),
                },
                when.clone(),
            ));
        }
        for (j, id) in tab_ids.iter().enumerate() {
            slide.triggers.push(interaction_trigger(
                Action::ChangeState {
                    object: id.clone(),
                    state: if j == i { "selected" } else { "normal" }.to_owned(),
                },
                when.clone(),
            ));
        }
    }
    if let Some(first) = layer_ids.first() {
        slide.triggers.push(interaction_trigger(
            Action::ShowLayer {
                layer: first.clone(),
            },
            Event::TimelineStarts { layer: None },
        ));
    }
}

fn insert_accordion(ctx: &mut InteractionCtx<'_>) {
    let InteractionCtx {
        slide,
        theme,
        w,
        h,
        margin,
        top,
        gap,
        base_ids,
        layer_ids,
        all_ids,
    } = ctx;
    let (w, h, margin, top, gap) = (*w, *h, *margin, *top, *gap);
    let theme = *theme;
    let count = 3usize;
    let header_h = (h * 0.08).clamp(36.0, 64.0);
    let header_w = (w - 2.0 * margin).max(20.0);
    let content_h = (h * 0.18).clamp(70.0, 160.0);
    let base_headers: Vec<Rect> = (0..count)
        .map(|i| Rect {
            x: margin,
            y: top + (header_h + gap) * i as f32,
            w: header_w,
            h: header_h,
        })
        .collect();
    for i in 0..count {
        layer_ids.push(fresh_id(
            &format!("accordion_layer{}", i + 1),
            slide.local_ids(),
        ));
    }
    let mut header_ids = Vec::new();
    for (i, frame) in base_headers.iter().copied().enumerate() {
        let id = fresh_id("accordion_header", slide.local_ids());
        header_ids.push(id.clone());
        slide.objects.push(interaction_button(
            id.clone(),
            format!("Header {} button", i + 1),
            format!("Section {}", i + 1),
            frame,
            false,
            theme,
        ));
        all_ids.push(id.clone());
        base_ids.push(id);
    }
    for open in 0..count {
        let Some(layer_id) = layer_ids.get(open).cloned() else {
            continue;
        };
        let mut layer = Layer {
            id: layer_id.clone(),
            name: format!("Section {} open", open + 1),
            duration_ms: 5000,
            objects: Vec::new(),
            triggers: Vec::new(),
            hide_other_layers: true,
            pause_base_timeline: false,
            prevent_base_clicks: true,
            hide_base_objects: true,
            hide_when_timeline_ends: false,
            allow_seek: SeekMode::Auto,
            revisit: RevisitMode::Auto,
            cue_points: Vec::new(),
            groups: Vec::new(),
        };
        let mut layer_headers = Vec::new();
        for (i, base) in base_headers.iter().copied().enumerate() {
            let shift = if i > open { content_h + gap } else { 0.0 };
            let frame = Rect {
                y: base.y + shift,
                ..base
            };
            let id = fresh_id(
                &format!("accordion_h{}_{}", open + 1, i + 1),
                slide.local_ids(),
            );
            layer_headers.push((id.clone(), i, frame));
            layer.objects.push(interaction_button(
                id.clone(),
                format!("Header {} button (section {} open)", i + 1, open + 1),
                format!("Section {}", i + 1),
                frame,
                false,
                theme,
            ));
            all_ids.push(id);
        }
        let content_id = fresh_id("accordion_content", slide.local_ids());
        layer.objects.push(interaction_text(
            content_id.clone(),
            format!("Header {} content", open + 1),
            format!("Content for Section {}", open + 1),
            Rect {
                x: margin,
                y: base_headers
                    .get(open)
                    .map_or(top, |frame| frame.y + header_h + gap * 0.5),
                w: header_w,
                h: content_h,
            },
            theme,
        ));
        all_ids.push(content_id);
        for (id, i, _) in &layer_headers {
            let when = Event::Click { object: id.clone() };
            if *i == open {
                layer
                    .triggers
                    .push(interaction_trigger(Action::HideLayer { layer: None }, when));
            } else if let Some(target) = layer_ids.get(*i) {
                layer.triggers.push(interaction_trigger(
                    Action::ShowLayer {
                        layer: target.clone(),
                    },
                    when,
                ));
            }
        }
        slide.layers.push(layer);
    }
    for (i, id) in header_ids.iter().enumerate() {
        if let Some(layer) = layer_ids.get(i) {
            slide.triggers.push(interaction_trigger(
                Action::ShowLayer {
                    layer: layer.clone(),
                },
                Event::Click { object: id.clone() },
            ));
        }
    }
}

fn insert_flip_cards(ctx: &mut InteractionCtx<'_>, flip_variables: &[String]) {
    let InteractionCtx {
        slide,
        theme,
        w,
        h,
        margin,
        top,
        gap,
        base_ids,
        all_ids,
        ..
    } = ctx;
    let (w, h, margin, top, gap) = (*w, *h, *margin, *top, *gap);
    let theme = *theme;
    let count = 3usize;
    let card_h = (h * 0.36).clamp(120.0, 260.0);
    let card_w = ((w - 2.0 * margin - gap * (count as f32 - 1.0)) / count as f32).max(20.0);
    let card_y = top + ((h - top * 2.0 - card_h).max(0.0)) * 0.5;
    let mut card_ids = Vec::new();
    for i in 0..count {
        let id = fresh_id("flip_card", slide.local_ids());
        card_ids.push(id.clone());
        // The front is a plain card with an accent outline and the
        // back an accent tint, so the one text colour (the theme's)
        // reads on both sides whatever the accent is.
        let card = interaction_shape(
            id.clone(),
            format!("Card {}", i + 1),
            ShapeKind::RoundedRectangle,
            theme.background.clone(),
            Some(Outline {
                color: theme.accent.clone(),
                width: 2.0,
                dash: Default::default(),
                dash_pattern: None,
                join: Default::default(),
                miter_limit: None,
            }),
            Some(("Card title".to_owned(), 26.0, true)),
            vec![StateDef {
                name: "Back".to_owned(),
                fill: Some(tint(theme.accent.clone(), 0x55)),
                text: Some("Add the detail here.".to_owned()),
                ..Default::default()
            }],
            Rect {
                x: margin + (card_w + gap) * i as f32,
                y: card_y,
                w: card_w,
                h: card_h,
            },
            theme,
        );
        slide.objects.push(card);
        all_ids.push(id.clone());
        base_ids.push(id);
    }
    for (card_id, variable) in card_ids.iter().zip(flip_variables.iter()) {
        let when = Event::Click {
            object: card_id.clone(),
        };
        // Triggers for one event run in order, so a pair of conditions
        // on the same state would both fire. Toggle a boolean instead
        // and let variable_changes set the visible state.
        slide.triggers.push(interaction_trigger(
            Action::SetVariable {
                variable: variable.clone(),
                op: VarOp::Toggle,
                value: None,
            },
            when,
        ));
        for (value, state) in [(true, "Back"), (false, "normal")] {
            slide.triggers.push(interaction_conditional_trigger(
                Action::ChangeState {
                    object: card_id.clone(),
                    state: state.to_owned(),
                },
                Event::VariableChanges {
                    variable: variable.clone(),
                },
                Condition::Compare {
                    variable: variable.clone(),
                    op: CmpOp::Eq,
                    value: Value::Bool(value).into(),
                },
            ));
        }
    }
}

fn insert_hotspot_reveal(ctx: &mut InteractionCtx<'_>) {
    let InteractionCtx {
        slide,
        theme,
        w,
        h,
        margin,
        top,
        base_ids,
        layer_ids,
        all_ids,
        ..
    } = ctx;
    let (w, h, margin, top) = (*w, *h, *margin, *top);
    let theme = *theme;
    let picture = Rect {
        x: margin,
        y: top,
        w: (w - 2.0 * margin).max(20.0),
        h: (h - top - margin).max(40.0),
    };
    let placeholder_id = fresh_id("hotspot_picture", slide.local_ids());
    slide.objects.push(interaction_shape(
        placeholder_id.clone(),
        "Picture placeholder".to_owned(),
        ShapeKind::RoundedRectangle,
        tint(theme.accent.clone(), 0x22),
        Some(Outline {
            color: theme.accent.clone(),
            width: 2.0,
            dash: DashStyle::Solid,
            dash_pattern: None,
            join: Default::default(),
            miter_limit: None,
        }),
        Some(("Replace with your picture".to_owned(), 24.0, false)),
        Vec::new(),
        picture,
        theme,
    ));
    base_ids.push(placeholder_id.clone());
    all_ids.push(placeholder_id);

    let marker_d = (w * 0.055).clamp(36.0, 60.0);
    let spots = [(0.28_f32, 0.38_f32), (0.72, 0.34), (0.50, 0.74)];
    let callout = Rect {
        x: picture.x + picture.w * 0.40,
        y: picture.y + picture.h * 0.16,
        w: (picture.w * 0.48).max(120.0),
        h: (picture.h * 0.48).max(110.0),
    };
    for (i, (px, py)) in spots.iter().copied().enumerate() {
        let marker_id = fresh_id("hotspot_marker", slide.local_ids());
        let marker = interaction_shape(
            marker_id.clone(),
            format!("Hotspot {} marker", i + 1),
            ShapeKind::Ellipse,
            theme.accent.clone(),
            None,
            Some(((i + 1).to_string(), 22.0, true)),
            vec![StateDef {
                name: "Visited".to_owned(),
                fill: Some(tint(theme.accent.clone(), 0x99)),
                ..Default::default()
            }],
            Rect {
                x: picture.x + picture.w * px - marker_d * 0.5,
                y: picture.y + picture.h * py - marker_d * 0.5,
                w: marker_d,
                h: marker_d,
            },
            theme,
        );
        slide.objects.push(marker);
        base_ids.push(marker_id.clone());
        all_ids.push(marker_id.clone());

        let layer_id = fresh_id("hotspot_layer", slide.local_ids());
        layer_ids.push(layer_id.clone());
        let mut objects = interaction_panel_objects(
            slide,
            &format!("hotspot_callout{}", i + 1),
            &format!("Hotspot {}", i + 1),
            callout,
            theme,
        );
        let close_id = fresh_id("hotspot_close", slide.local_ids());
        objects.push(interaction_plain_button(
            close_id.clone(),
            format!("Close hotspot {}", i + 1),
            "×".to_owned(),
            Rect {
                x: callout.x + callout.w - 44.0,
                y: callout.y + 8.0,
                w: 36.0,
                h: 36.0,
            },
            theme,
        ));
        all_ids.extend(objects.iter().map(|o| o.id.clone()));
        let layer_triggers = vec![interaction_trigger(
            Action::HideLayer { layer: None },
            Event::Click { object: close_id },
        )];
        slide.layers.push(interaction_callout_layer(
            layer_id.clone(),
            format!("Hotspot {}", i + 1),
            objects,
            layer_triggers,
        ));

        let when = Event::Click {
            object: marker_id.clone(),
        };
        slide.triggers.push(interaction_trigger(
            Action::ShowLayer { layer: layer_id },
            when.clone(),
        ));
        slide.triggers.push(interaction_trigger(
            Action::ChangeState {
                object: marker_id,
                state: "Visited".to_owned(),
            },
            when,
        ));
    }
}

fn insert_process_steps(ctx: &mut InteractionCtx<'_>, step_variable: &str) {
    let InteractionCtx {
        slide,
        theme,
        w,
        h,
        margin,
        top,
        gap,
        base_ids,
        layer_ids,
        all_ids,
    } = ctx;
    let (w, h, margin, top, gap) = (*w, *h, *margin, *top, *gap);
    let theme = *theme;
    let count = 4usize;
    let circle_d = (w * 0.07).clamp(40.0, 76.0);
    let row_y = top;
    let step_w = (w - 2.0 * margin) / count as f32;
    let centres: Vec<f32> = (0..count)
        .map(|i| margin + step_w * (i as f32 + 0.5))
        .collect();
    for (i, pair) in centres.windows(2).enumerate() {
        let [left, right] = *pair else {
            continue;
        };
        let id = fresh_id("step_line", slide.local_ids());
        slide.objects.push(interaction_shape(
            id.clone(),
            format!("Step {} connector", i + 1),
            ShapeKind::LineHorizontal,
            theme.accent.clone(),
            None,
            None,
            Vec::new(),
            Rect {
                x: left + circle_d * 0.5,
                y: row_y + circle_d * 0.5,
                w: (right - left - circle_d).max(1.0),
                h: 2.0,
            },
            theme,
        ));
        all_ids.push(id.clone());
        base_ids.push(id);
    }
    let mut circle_ids = Vec::new();
    for (i, &centre) in centres.iter().enumerate() {
        let circle_id = fresh_id("step_circle", slide.local_ids());
        circle_ids.push(circle_id.clone());
        slide.objects.push(interaction_shape(
            circle_id.clone(),
            format!("Step {} circle", i + 1),
            ShapeKind::Ellipse,
            tint(theme.accent.clone(), 0x99),
            None,
            Some(((i + 1).to_string(), 24.0, true)),
            vec![StateDef {
                name: "Selected".to_owned(),
                fill: Some(theme.accent.clone()),
                ..Default::default()
            }],
            Rect {
                x: centre - circle_d * 0.5,
                y: row_y,
                w: circle_d,
                h: circle_d,
            },
            theme,
        ));
        all_ids.push(circle_id.clone());
        base_ids.push(circle_id.clone());

        let label_id = fresh_id("step_label", slide.local_ids());
        let mut label = interaction_text(
            label_id.clone(),
            format!("Step {} label", i + 1),
            format!("Step {}", i + 1),
            Rect {
                x: centre - step_w * 0.5,
                y: row_y + circle_d + 6.0,
                w: step_w,
                h: 30.0,
            },
            theme,
        );
        // The box is centred on its circle, so the text is too.
        if let ObjectKind::Text { style, .. } = &mut label.kind {
            style.align = Align::Center;
        }
        slide.objects.push(label);
        all_ids.push(label_id.clone());
        base_ids.push(label_id);
    }

    let button_y = (h - margin - 44.0).max(row_y + circle_d + 60.0);
    let button_w = 110.0;
    let back_id = fresh_id("step_back", slide.local_ids());
    let next_id = fresh_id("step_next", slide.local_ids());
    slide.objects.push(interaction_plain_button(
        back_id.clone(),
        "Back".to_owned(),
        "Back".to_owned(),
        Rect {
            x: margin,
            y: button_y,
            w: button_w,
            h: 44.0,
        },
        theme,
    ));
    slide.objects.push(interaction_plain_button(
        next_id.clone(),
        "Next".to_owned(),
        "Next".to_owned(),
        Rect {
            x: (w - margin - button_w).max(margin + button_w + gap),
            y: button_y,
            w: button_w,
            h: 44.0,
        },
        theme,
    ));
    all_ids.push(back_id.clone());
    all_ids.push(next_id.clone());
    base_ids.push(back_id.clone());
    base_ids.push(next_id.clone());

    let panel = Rect {
        x: margin,
        y: row_y + circle_d + 48.0,
        w: (w - 2.0 * margin).max(20.0),
        h: (button_y - row_y - circle_d - 56.0).max(80.0),
    };
    for i in 0..count {
        let layer_id = fresh_id("step_layer", slide.local_ids());
        layer_ids.push(layer_id.clone());
        let objects = interaction_panel_objects(
            slide,
            &format!("step_panel{}", i + 1),
            &format!("Step {}", i + 1),
            panel,
            theme,
        );
        all_ids.extend(objects.iter().map(|o| o.id.clone()));
        slide.layers.push(interaction_callout_layer(
            layer_id,
            format!("Step {}", i + 1),
            objects,
            Vec::new(),
        ));
    }

    for (i, circle_id) in circle_ids.iter().enumerate() {
        let step = (i + 1) as f64;
        slide.triggers.push(interaction_trigger(
            Action::SetVariable {
                variable: step_variable.to_owned(),
                op: VarOp::Assign,
                value: Some(Value::Number(step).into()),
            },
            Event::Click {
                object: circle_id.clone(),
            },
        ));
    }
    slide.triggers.push(interaction_conditional_trigger(
        Action::SetVariable {
            variable: step_variable.to_owned(),
            op: VarOp::Add,
            value: Some(Value::Number(1.0).into()),
        },
        Event::Click { object: next_id },
        Condition::Compare {
            variable: step_variable.to_owned(),
            op: CmpOp::Lt,
            value: Value::Number(count as f64).into(),
        },
    ));
    slide.triggers.push(interaction_conditional_trigger(
        Action::SetVariable {
            variable: step_variable.to_owned(),
            op: VarOp::Subtract,
            value: Some(Value::Number(1.0).into()),
        },
        Event::Click { object: back_id },
        Condition::Compare {
            variable: step_variable.to_owned(),
            op: CmpOp::Gt,
            value: Value::Number(1.0).into(),
        },
    ));
    for i in 0..count {
        let step = (i + 1) as f64;
        let condition = Condition::Compare {
            variable: step_variable.to_owned(),
            op: CmpOp::Eq,
            value: Value::Number(step).into(),
        };
        if let Some(layer_id) = layer_ids.get(i) {
            slide.triggers.push(interaction_conditional_trigger(
                Action::ShowLayer {
                    layer: layer_id.clone(),
                },
                Event::VariableChanges {
                    variable: step_variable.to_owned(),
                },
                condition.clone(),
            ));
        }
        for (j, circle_id) in circle_ids.iter().enumerate() {
            slide.triggers.push(interaction_conditional_trigger(
                Action::ChangeState {
                    object: circle_id.clone(),
                    state: if j == i { "Selected" } else { "normal" }.to_owned(),
                },
                Event::VariableChanges {
                    variable: step_variable.to_owned(),
                },
                condition.clone(),
            ));
        }
    }
    if let Some(first_layer) = layer_ids.first()
        && let Some(first_circle) = circle_ids.first()
    {
        // Revisiting the slide starts again at step 1, so the variable,
        // the shown layer and the selected circle always agree.
        slide.triggers.push(interaction_trigger(
            Action::SetVariable {
                variable: step_variable.to_owned(),
                op: VarOp::Assign,
                value: Some(Value::Number(1.0).into()),
            },
            Event::TimelineStarts { layer: None },
        ));
        slide.triggers.push(interaction_trigger(
            Action::ShowLayer {
                layer: first_layer.clone(),
            },
            Event::TimelineStarts { layer: None },
        ));
        slide.triggers.push(interaction_trigger(
            Action::ChangeState {
                object: first_circle.clone(),
                state: "Selected".to_owned(),
            },
            Event::TimelineStarts { layer: None },
        ));
    }
}

fn insert_timeline(ctx: &mut InteractionCtx<'_>) {
    let InteractionCtx {
        slide,
        theme,
        w,
        h,
        margin,
        top,
        base_ids,
        layer_ids,
        all_ids,
        ..
    } = ctx;
    let (w, h, margin, top) = (*w, *h, *margin, *top);
    let theme = *theme;
    let count = 5usize;
    let point_d = (w * 0.05).clamp(28.0, 52.0);
    let line_y = top + 40.0;
    let step = (w - 2.0 * margin) / (count as f32 - 1.0);
    let centres: Vec<f32> = (0..count).map(|i| margin + step * i as f32).collect();

    let line_id = fresh_id("timeline_line", slide.local_ids());
    slide.objects.push(interaction_shape(
        line_id.clone(),
        "Timeline line".to_owned(),
        ShapeKind::LineHorizontal,
        theme.accent.clone(),
        None,
        None,
        Vec::new(),
        Rect {
            x: margin,
            y: line_y,
            w: (w - 2.0 * margin).max(1.0),
            h: 2.0,
        },
        theme,
    ));
    all_ids.push(line_id.clone());
    base_ids.push(line_id);

    let mut point_ids = Vec::new();
    for (i, &centre) in centres.iter().enumerate() {
        let point_id = fresh_id("timeline_point", slide.local_ids());
        point_ids.push(point_id.clone());
        slide.objects.push(interaction_shape(
            point_id.clone(),
            format!("Point {}", i + 1),
            ShapeKind::Ellipse,
            tint(theme.accent.clone(), 0x99),
            None,
            None,
            vec![StateDef {
                name: "Visited".to_owned(),
                fill: Some(theme.accent.clone()),
                ..Default::default()
            }],
            Rect {
                x: centre - point_d * 0.5,
                y: line_y - point_d * 0.5,
                w: point_d,
                h: point_d,
            },
            theme,
        ));
        all_ids.push(point_id.clone());
        base_ids.push(point_id.clone());

        let year_id = fresh_id("timeline_year", slide.local_ids());
        let mut year = interaction_text(
            year_id.clone(),
            format!("Point {} year", i + 1),
            format!("{}", 2020 + i as u32),
            Rect {
                x: centre - step * 0.5,
                y: top - 8.0,
                w: step,
                h: 30.0,
            },
            theme,
        );
        if let ObjectKind::Text { style, .. } = &mut year.kind {
            style.size = 18.0;
            style.align = Align::Center;
        }
        slide.objects.push(year);
        all_ids.push(year_id.clone());
        base_ids.push(year_id);

        let title_id = fresh_id("timeline_title", slide.local_ids());
        let mut title = interaction_text(
            title_id.clone(),
            format!("Point {} title", i + 1),
            format!("Title {}", i + 1),
            Rect {
                x: centre - step * 0.5,
                y: line_y + point_d * 0.5 + 4.0,
                w: step,
                h: 30.0,
            },
            theme,
        );
        if let ObjectKind::Text { style, .. } = &mut title.kind {
            style.size = 18.0;
            style.align = Align::Center;
        }
        slide.objects.push(title);
        all_ids.push(title_id.clone());
        base_ids.push(title_id);
    }

    let panel_y = line_y + point_d * 0.5 + 40.0;
    let panel = Rect {
        x: margin,
        y: panel_y,
        w: (w - 2.0 * margin).max(20.0),
        h: (h - panel_y - margin).max(80.0),
    };
    for (i, point_id) in point_ids.iter().enumerate() {
        let layer_id = fresh_id("timeline_layer", slide.local_ids());
        layer_ids.push(layer_id.clone());
        let objects = interaction_panel_objects(
            slide,
            &format!("timeline_panel{}", i + 1),
            &format!("Point {}", i + 1),
            panel,
            theme,
        );
        all_ids.extend(objects.iter().map(|o| o.id.clone()));
        slide.layers.push(interaction_callout_layer(
            layer_id.clone(),
            format!("Point {}", i + 1),
            objects,
            Vec::new(),
        ));

        let when = Event::Click {
            object: point_id.clone(),
        };
        slide.triggers.push(interaction_trigger(
            Action::ShowLayer { layer: layer_id },
            when.clone(),
        ));
        slide.triggers.push(interaction_trigger(
            Action::ChangeState {
                object: point_id.clone(),
                state: "Visited".to_owned(),
            },
            when,
        ));
    }
    // No layer is shown on slide start; each visit is remembered.
}

fn insert_reveal_grid(ctx: &mut InteractionCtx<'_>, reveal_variables: &[String]) {
    let InteractionCtx {
        slide,
        theme,
        w,
        h,
        margin,
        top,
        gap,
        base_ids,
        all_ids,
        ..
    } = ctx;
    let (w, h, margin, top, gap) = (*w, *h, *margin, *top, *gap);
    let theme = *theme;
    let Some((explored, tile_vars)) = reveal_variables.split_first() else {
        return;
    };
    let cols = 3usize;
    let rows = 2usize;
    let count = cols * rows;
    let counter_h = 32.0;
    let grid_w = (w - 2.0 * margin).max(20.0);
    let grid_h = (h - top - margin - counter_h - gap).max(120.0);
    let tile_w = (grid_w - gap * (cols as f32 - 1.0)) / cols as f32;
    let tile_h = (grid_h - gap * (rows as f32 - 1.0)) / rows as f32;

    let mut tile_ids = Vec::new();
    for i in 0..count {
        let id = fresh_id("grid_tile_shape", slide.local_ids());
        tile_ids.push(id.clone());
        slide.objects.push(interaction_shape(
            id.clone(),
            format!("Tile {}", i + 1),
            ShapeKind::RoundedRectangle,
            tint(theme.accent.clone(), 0x22),
            Some(Outline {
                color: theme.accent.clone(),
                width: 2.0,
                dash: DashStyle::Solid,
                dash_pattern: None,
                join: Default::default(),
                miter_limit: None,
            }),
            Some(("Tap to reveal".to_owned(), 18.0, false)),
            vec![StateDef {
                name: "Revealed".to_owned(),
                fill: Some(tint(theme.accent.clone(), 0x77)),
                text: Some(format!("Detail {}", i + 1)),
                ..Default::default()
            }],
            Rect {
                x: margin + (tile_w + gap) * (i % cols) as f32,
                y: top + (tile_h + gap) * (i / cols) as f32,
                w: tile_w,
                h: tile_h,
            },
            theme,
        ));
        all_ids.push(id.clone());
        base_ids.push(id);
    }

    let counter_id = fresh_id("grid_counter", slide.local_ids());
    let mut counter = interaction_text(
        counter_id.clone(),
        "Explored counter".to_owned(),
        format!("%{explored}% of {count} explored"),
        Rect {
            x: margin,
            y: top + grid_h + gap,
            w: grid_w,
            h: counter_h,
        },
        theme,
    );
    if let ObjectKind::Text { style, .. } = &mut counter.kind {
        style.align = Align::Center;
        style.size = 20.0;
    }
    slide.objects.push(counter);
    all_ids.push(counter_id.clone());
    base_ids.push(counter_id);

    for (tile_id, tile_var) in tile_ids.iter().zip(tile_vars.iter()) {
        // A click only flips the flag when the tile is still normal. The
        // reveal and the count then follow the flag's change, so a second
        // click on a revealed tile changes nothing.
        slide.triggers.push(interaction_conditional_trigger(
            Action::SetVariable {
                variable: tile_var.clone(),
                op: VarOp::Assign,
                value: Some(Value::Bool(true).into()),
            },
            Event::Click {
                object: tile_id.clone(),
            },
            Condition::Compare {
                variable: tile_var.clone(),
                op: CmpOp::Eq,
                value: Value::Bool(false).into(),
            },
        ));
        let changed = Event::VariableChanges {
            variable: tile_var.clone(),
        };
        let revealed = Condition::Compare {
            variable: tile_var.clone(),
            op: CmpOp::Eq,
            value: Value::Bool(true).into(),
        };
        slide.triggers.push(interaction_conditional_trigger(
            Action::ChangeState {
                object: tile_id.clone(),
                state: "Revealed".to_owned(),
            },
            changed.clone(),
            revealed.clone(),
        ));
        slide.triggers.push(interaction_conditional_trigger(
            Action::SetVariable {
                variable: explored.clone(),
                op: VarOp::Add,
                value: Some(Value::Number(1.0).into()),
            },
            changed,
            revealed,
        ));
        // Revisiting the slide resets both the flag and the visible state.
        slide.triggers.push(interaction_trigger(
            Action::SetVariable {
                variable: tile_var.clone(),
                op: VarOp::Assign,
                value: Some(Value::Bool(false).into()),
            },
            Event::TimelineStarts { layer: None },
        ));
        slide.triggers.push(interaction_trigger(
            Action::ChangeState {
                object: tile_id.clone(),
                state: "normal".to_owned(),
            },
            Event::TimelineStarts { layer: None },
        ));
    }
    slide.triggers.push(interaction_trigger(
        Action::SetVariable {
            variable: explored.clone(),
            op: VarOp::Assign,
            value: Some(Value::Number(0.0).into()),
        },
        Event::TimelineStarts { layer: None },
    ));
}

fn insert_scenario_choice(ctx: &mut InteractionCtx<'_>) {
    let InteractionCtx {
        slide,
        theme,
        w,
        h,
        margin,
        top,
        gap,
        base_ids,
        layer_ids,
        all_ids,
    } = ctx;
    let (w, h, margin, top, gap) = (*w, *h, *margin, *top, *gap);
    let theme = *theme;
    let situation = Rect {
        x: margin,
        y: top,
        w: (w - 2.0 * margin).max(20.0),
        h: (h * 0.22).clamp(90.0, 180.0),
    };
    for (id, name, fill, text, size, bold) in [
        (
            fresh_id("scenario_panel", slide.local_ids()),
            "Situation panel".to_owned(),
            tint(theme.accent.clone(), 0x22),
            None,
            0.0,
            false,
        ),
        (
            fresh_id("scenario_title", slide.local_ids()),
            "Situation title".to_owned(),
            theme.text.clone(),
            Some("What would you do?".to_owned()),
            26.0,
            true,
        ),
        (
            fresh_id("scenario_body", slide.local_ids()),
            "Situation sentence".to_owned(),
            theme.text.clone(),
            Some("Describe the situation here.".to_owned()),
            20.0,
            false,
        ),
    ] {
        all_ids.push(id.clone());
        base_ids.push(id.clone());
        if text.is_none() {
            slide.objects.push(interaction_shape(
                id,
                name,
                ShapeKind::RoundedRectangle,
                fill,
                Some(Outline {
                    color: theme.accent.clone(),
                    width: 2.0,
                    dash: DashStyle::Solid,
                    dash_pattern: None,
                    join: Default::default(),
                    miter_limit: None,
                }),
                None,
                Vec::new(),
                situation,
                theme,
            ));
        } else {
            slide.objects.push(interaction_text(
                id,
                name,
                text.unwrap_or_default(),
                Rect {
                    x: situation.x + 16.0,
                    y: situation.y + if bold { 10.0 } else { 52.0 },
                    w: (situation.w - 32.0).max(20.0),
                    h: 36.0,
                },
                theme,
            ));
            if let Some(object) = slide.objects.last_mut()
                && let ObjectKind::Text { style, .. } = &mut object.kind
            {
                style.size = size;
                style.bold = bold;
            }
        }
    }

    let choice_h = (h * 0.08).clamp(36.0, 56.0);
    let choice_y = situation.y + situation.h + gap;
    let mut choice_ids = Vec::new();
    for i in 0..3 {
        let id = fresh_id("scenario_choice", slide.local_ids());
        choice_ids.push(id.clone());
        slide.objects.push(interaction_plain_button(
            id.clone(),
            format!("Choice {}", i + 1),
            format!("Choice {}", i + 1),
            Rect {
                x: situation.x,
                y: choice_y + (choice_h + gap) * i as f32,
                w: situation.w,
                h: choice_h,
            },
            theme,
        ));
        all_ids.push(id.clone());
        base_ids.push(id);
    }

    let panel = Rect {
        x: situation.x + situation.w * 0.10,
        y: choice_y,
        w: (situation.w * 0.80).max(20.0),
        h: (h - choice_y - margin).max(160.0),
    };
    let feedback = [
        (
            "Think again",
            "That is a reasonable start, but there is a better option. Consider the suggested answer and try again.",
        ),
        (
            "Good choice",
            "This is the suggested best answer because it balances the learner's needs with the goal of the situation.",
        ),
        (
            "Think again",
            "Not quite. Think about what the situation is asking, then try a different approach.",
        ),
    ];
    for (i, (heading, body)) in feedback.into_iter().enumerate() {
        let layer_id = fresh_id("scenario_layer", slide.local_ids());
        layer_ids.push(layer_id.clone());
        let heading_id = fresh_id("scenario_feedback_title", slide.local_ids());
        let body_id = fresh_id("scenario_feedback_text", slide.local_ids());
        let mut heading_object = interaction_text(
            heading_id.clone(),
            format!("Choice {} heading", i + 1),
            heading.to_owned(),
            Rect {
                x: panel.x + 16.0,
                y: panel.y + 12.0,
                w: (panel.w - 32.0).max(20.0),
                h: 36.0,
            },
            theme,
        );
        if let ObjectKind::Text { style, .. } = &mut heading_object.kind {
            style.size = 26.0;
            style.bold = true;
        }
        let body_object = interaction_text(
            body_id.clone(),
            format!("Choice {} text", i + 1),
            body.to_owned(),
            Rect {
                x: panel.x + 16.0,
                y: panel.y + 56.0,
                w: (panel.w - 32.0).max(20.0),
                h: (panel.h - 120.0).max(24.0),
            },
            theme,
        );
        let mut objects = vec![
            interaction_shape(
                fresh_id("scenario_feedback_panel", slide.local_ids()),
                format!("Choice {} panel", i + 1),
                ShapeKind::RoundedRectangle,
                tint(theme.accent.clone(), 0x22),
                Some(Outline {
                    color: theme.accent.clone(),
                    width: 2.0,
                    dash: DashStyle::Solid,
                    dash_pattern: None,
                    join: Default::default(),
                    miter_limit: None,
                }),
                None,
                Vec::new(),
                panel,
                theme,
            ),
            heading_object,
            body_object,
        ];
        let layer_triggers = if i == 1 {
            Vec::new()
        } else {
            let retry_id = fresh_id("scenario_retry", slide.local_ids());
            objects.push(interaction_plain_button(
                retry_id.clone(),
                format!("Try again {}", i + 1),
                "Try again".to_owned(),
                Rect {
                    x: panel.x + 16.0,
                    y: panel.y + panel.h - 52.0,
                    w: 120.0,
                    h: 40.0,
                },
                theme,
            ));
            vec![interaction_trigger(
                Action::HideLayer { layer: None },
                Event::Click { object: retry_id },
            )]
        };
        all_ids.extend(objects.iter().map(|o| o.id.clone()));
        slide.layers.push(interaction_callout_layer(
            layer_id.clone(),
            format!("Choice {}", i + 1),
            objects,
            layer_triggers,
        ));
        if let Some(choice_id) = choice_ids.get(i) {
            slide.triggers.push(interaction_trigger(
                Action::ShowLayer { layer: layer_id },
                Event::Click {
                    object: choice_id.clone(),
                },
            ));
        }
    }
    // No question and no score: the best answer is marked by its text only.
}
