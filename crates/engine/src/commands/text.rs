//! Rich text, paragraph, hyperlink, and autofit commands.

use coursecraft_course::{
    Align, Autofit, Baseline, CaseChange, Color, Link, ListKind, ObjectKind, Paragraph, Rect,
    ShapeText, TextDirection, TextRun, TextRunPatch, TextStyle, VAlign, align_paragraphs,
    apply_link_to_range, apply_style_to_range_with_base, apply_text_edit, is_valid_link_url,
    paragraphs_for, parse_color, relative_luminance,
};
use coursecraft_media::text_metrics::{
    MeasuredRun, POINT_TEXT_MAX_DIM, POINT_TEXT_MIN_W, ParagraphMetrics, TextBoxInsets,
    grow_text_box_height, measure_unwrapped_text, measure_wrapped_text_height, point_text_box_size,
};
use serde_json::Value as Json;

use super::{bad, find_object_mut, keep_valid, opt, req, slide_mut, text, validate_color};
use crate::{EngineError, Result, Session};

/// Superscript and subscript text scales to this fraction of its font size,
/// matching `coursecraft_render::text_layout::SCRIPT_SCALE`.
const SCRIPT_SCALE: f32 = 0.65;
/// Indent per paragraph list/nesting level in slide pixels, matching
/// `coursecraft_render::text_layout::INDENT_STEP`.
const LIST_INDENT_STEP: f32 = 24.0;

/// Parse an optional `"measured_size": { "w": f32, "h": f32 }` hint from command
/// parameters. Returns `Some((w, h))` only when both dimensions are finite and
/// strictly positive, clamped into `1.0..=POINT_TEXT_MAX_DIM`.
pub(super) fn parse_measured_size(p: &Json) -> Option<(f32, f32)> {
    let obj = p.get("measured_size")?.as_object()?;
    let w = obj.get("w")?.as_f64()?;
    let h = obj.get("h")?.as_f64()?;
    if !w.is_finite() || !h.is_finite() || w <= 0.0 || h <= 0.0 {
        return None;
    }
    let max = f64::from(POINT_TEXT_MAX_DIM);
    #[allow(clippy::cast_possible_truncation)]
    let w = (w.clamp(1.0, max) as f32).clamp(1.0, POINT_TEXT_MAX_DIM);
    #[allow(clippy::cast_possible_truncation)]
    let h = (h.clamp(1.0, max) as f32).clamp(1.0, POINT_TEXT_MAX_DIM);
    Some((w, h))
}

fn script_scale_for(baseline: Baseline) -> f32 {
    match baseline {
        Baseline::Normal => 1.0,
        Baseline::Superscript | Baseline::Subscript => SCRIPT_SCALE,
    }
}

fn build_measured_runs<'a>(
    text: &'a str,
    runs: &'a [TextRun],
    style: &TextStyle,
) -> Vec<MeasuredRun<'a>> {
    if runs.is_empty() {
        vec![MeasuredRun {
            text,
            size: style.size,
            bold: style.bold,
            italic: style.italic,
            letter_spacing: style.letter_spacing,
            script_scale: script_scale_for(style.baseline),
        }]
    } else {
        runs.iter()
            .map(|run| MeasuredRun {
                text: &run.text,
                size: run.size.unwrap_or(style.size),
                bold: run.bold.unwrap_or(style.bold),
                italic: run.italic.unwrap_or(style.italic),
                letter_spacing: run.letter_spacing.unwrap_or(style.letter_spacing),
                script_scale: script_scale_for(run.baseline.unwrap_or(style.baseline)),
            })
            .collect()
    }
}

fn build_paragraph_metrics(paragraphs: &[Paragraph]) -> Vec<ParagraphMetrics> {
    paragraphs
        .iter()
        .map(|p| {
            let level = f32::from(p.level.min(4));
            let list_indent = if p.list == ListKind::None {
                level * LIST_INDENT_STEP
            } else {
                (level + 1.0) * LIST_INDENT_STEP
            };
            ParagraphMetrics {
                line_spacing: p.line_spacing,
                space_before: p.space_before,
                space_after: p.space_after,
                list_indent,
            }
        })
        .collect()
}

fn box_insets(style: &TextStyle) -> TextBoxInsets {
    style
        .insets
        .map_or(TextBoxInsets::ZERO, |ins| TextBoxInsets {
            left: ins.left,
            top: ins.top,
            right: ins.right,
            bottom: ins.bottom,
        })
}

/// Fit `frame` to unwrapped point text content using `measured_override` when
/// supplied and valid, or `coursecraft-media`'s headless `skrifa` metrics
/// otherwise. Keeps `(frame.x, frame.y)` fixed when `anchor_top_left` is true
/// and preserves `style.align` / `style.valign` anchors on subsequent edits.
pub(super) fn fit_point_text_frame(
    frame: &mut Rect,
    text: &str,
    runs: &[TextRun],
    style: &TextStyle,
    paragraphs: &[Paragraph],
    measured_override: Option<(f32, f32)>,
    anchor_top_left: bool,
) {
    let (new_w, new_h) = match measured_override {
        Some((w, h)) if w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0 => (
            w.ceil().clamp(POINT_TEXT_MIN_W, POINT_TEXT_MAX_DIM),
            h.ceil().clamp(1.0, POINT_TEXT_MAX_DIM),
        ),
        _ => {
            let measured_runs = build_measured_runs(text, runs, style);
            let para_metrics = build_paragraph_metrics(paragraphs);
            let insets = box_insets(style);
            let vertical = !matches!(style.direction, TextDirection::Horizontal);
            let (content_w, content_h) =
                measure_unwrapped_text(&measured_runs, &para_metrics, style.size);
            point_text_box_size(content_w, content_h, insets, vertical)
        }
    };

    let old_x = if frame.x.is_finite() { frame.x } else { 0.0 };
    let old_y = if frame.y.is_finite() { frame.y } else { 0.0 };
    let old_w = if frame.w.is_finite() && frame.w > 0.0 {
        frame.w
    } else {
        new_w
    };
    let old_h = if frame.h.is_finite() && frame.h > 0.0 {
        frame.h
    } else {
        new_h
    };

    let (next_x, next_y) = if anchor_top_left {
        (old_x, old_y)
    } else {
        let x = match style.align {
            Align::Left | Align::Justify => old_x,
            Align::Center => old_x + (old_w - new_w) * 0.5,
            Align::Right => old_x + (old_w - new_w),
        };
        let y = match style.valign {
            VAlign::Top => old_y,
            VAlign::Middle => old_y + (old_h - new_h) * 0.5,
            VAlign::Bottom => old_y + (old_h - new_h),
        };
        (x, y)
    };

    frame.x = if next_x.is_finite() {
        next_x.round()
    } else {
        0.0
    };
    frame.y = if next_y.is_finite() {
        next_y.round()
    } else {
        0.0
    };
    frame.w = new_w.round().clamp(1.0, POINT_TEXT_MAX_DIM);
    frame.h = new_h.round().clamp(1.0, POINT_TEXT_MAX_DIM);
}

/// Fit `frame.h` (keeping `frame.x` and `frame.w` unchanged) to wrapped text
/// content for `Autofit::Grow`, using `measured_override` when supplied and
/// valid or `coursecraft-media`'s headless `skrifa` metrics otherwise.
/// Keeps `frame.y` fixed when `anchor_top_left` is true and preserves
/// `style.valign` anchoring on subsequent edits.
pub(super) fn fit_grow_text_frame(
    frame: &mut Rect,
    text: &str,
    runs: &[TextRun],
    style: &TextStyle,
    paragraphs: &[Paragraph],
    measured_override: Option<(f32, f32)>,
    anchor_top_left: bool,
) {
    let new_h = match measured_override {
        Some((_, h)) if h.is_finite() && h > 0.0 => h.ceil().clamp(1.0, POINT_TEXT_MAX_DIM),
        _ => {
            let measured_runs = build_measured_runs(text, runs, style);
            let para_metrics = build_paragraph_metrics(paragraphs);
            let insets = box_insets(style);
            let box_w = if frame.w.is_finite() && frame.w > 0.0 {
                frame.w.clamp(1.0, POINT_TEXT_MAX_DIM)
            } else {
                POINT_TEXT_MIN_W
            };
            let left_ins = if insets.left.is_finite() {
                insets.left.max(0.0).min(box_w * 0.45)
            } else {
                0.0
            };
            let right_ins = if insets.right.is_finite() {
                insets.right.max(0.0).min(box_w * 0.45)
            } else {
                0.0
            };
            let inner_w = (box_w - left_ins - right_ins).max(1.0);
            let content_h = if style.wrap {
                measure_wrapped_text_height(&measured_runs, &para_metrics, style.size, inner_w)
            } else {
                measure_unwrapped_text(&measured_runs, &para_metrics, style.size).1
            };
            grow_text_box_height(content_h, insets)
        }
    };

    let old_y = if frame.y.is_finite() { frame.y } else { 0.0 };
    let old_h = if frame.h.is_finite() && frame.h > 0.0 {
        frame.h
    } else {
        new_h
    };
    let next_y = if anchor_top_left {
        old_y
    } else {
        match style.valign {
            VAlign::Top => old_y,
            VAlign::Middle => old_y + (old_h - new_h) * 0.5,
            VAlign::Bottom => old_y + (old_h - new_h),
        }
    };

    frame.y = if next_y.is_finite() {
        next_y.round()
    } else {
        0.0
    };
    frame.h = new_h.round().clamp(1.0, POINT_TEXT_MAX_DIM);
}

/// Default shape text style: a contrasting colour on the shape's fill, centred
/// both ways. Transparent fills read as the slide background, so use dark text.
fn shape_text_default_style(fill: &Color) -> TextStyle {
    let dark = Color("#1F1F1F".to_owned());
    let light = Color("#FFFFFF".to_owned());
    let color = match parse_color(fill) {
        Some(fill) if fill.a > 0 => {
            let luminance = relative_luminance(fill);
            if luminance < 0.45 { light } else { dark }
        }
        _ => dark,
    };
    TextStyle {
        color,
        align: Align::Center,
        valign: VAlign::Middle,
        ..TextStyle::default()
    }
}

/// Mutable parts of an object that carries text. A shape with no text yet gets
/// a default style when a text command touches it.
struct TextParts<'a> {
    text: &'a mut String,
    runs: &'a mut Vec<TextRun>,
    style: &'a mut TextStyle,
    paragraphs: &'a mut Vec<Paragraph>,
    autofit: Option<&'a mut Autofit>,
    /// A table cell keeps vertical alignment beside its style.
    valign: Option<&'a mut VAlign>,
}

fn text_parts(kind: &mut ObjectKind) -> Option<TextParts<'_>> {
    match kind {
        ObjectKind::Text {
            text,
            runs,
            style,
            paragraphs,
            autofit,
            ..
        } => Some(TextParts {
            text,
            runs,
            style,
            paragraphs,
            autofit: Some(autofit),
            valign: None,
        }),
        ObjectKind::Shape { fill, text, .. } => {
            if text.is_none() {
                *text = Some(ShapeText {
                    text: String::new(),
                    style: shape_text_default_style(fill),
                    runs: Vec::new(),
                    paragraphs: Vec::new(),
                    autofit: Autofit::None,
                });
            }
            let shape = text.as_mut()?;
            Some(TextParts {
                text: &mut shape.text,
                runs: &mut shape.runs,
                style: &mut shape.style,
                paragraphs: &mut shape.paragraphs,
                autofit: Some(&mut shape.autofit),
                valign: None,
            })
        }
        _ => None,
    }
}

/// Text parts of an object, or of the table cell named by `row` and `column`.
fn text_parts_at<'a>(
    kind: &'a mut ObjectKind,
    p: &Json,
    cmd: &str,
) -> Result<Option<TextParts<'a>>> {
    if let ObjectKind::Table { rows, .. } = kind {
        let row: usize =
            opt(cmd, p, "row")?.ok_or_else(|| bad(cmd, "a table cell needs `row` and `column`"))?;
        let column: usize = opt(cmd, p, "column")?
            .ok_or_else(|| bad(cmd, "a table cell needs `row` and `column`"))?;
        let cell = rows
            .get_mut(row)
            .and_then(|line| line.cells.get_mut(column))
            .ok_or_else(|| EngineError::NotFound(format!("no table cell {row},{column}")))?;
        return Ok(Some(TextParts {
            text: &mut cell.text,
            runs: &mut cell.runs,
            style: &mut cell.style,
            paragraphs: &mut cell.paragraphs,
            autofit: None,
            valign: Some(&mut cell.valign),
        }));
    }
    Ok(text_parts(kind))
}

pub(super) fn text_set(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "text.set";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let new_text_raw: String = req(C, p, "text")?;
    let new_text = text(C, new_text_raw, "text")?;

    let explicit_runs: Option<Vec<coursecraft_course::TextRun>> = opt(C, p, "runs")?;
    let explicit_paragraphs: Option<Vec<coursecraft_course::Paragraph>> = opt(C, p, "paragraphs")?;

    let sl = slide_mut(s, &sid)?;
    let obj = find_object_mut(sl, &oid)?;
    let Some(parts) = text_parts_at(&mut obj.kind, p, C)? else {
        return Err(bad(C, format!("object {oid:?} is not a text object")));
    };
    // Diff against what is displayed: the runs when there are any.
    let old_plain: String = if parts.runs.is_empty() {
        parts.text.clone()
    } else {
        parts.runs.iter().map(|r| r.text.as_str()).collect()
    };
    let new_runs = match explicit_runs {
        Some(runs) if runs.iter().map(|r| r.text.as_str()).collect::<String>() == new_text => {
            coursecraft_course::normalize_runs(&runs)
        }
        _ => apply_text_edit(parts.runs, &old_plain, &new_text),
    };
    if new_runs.len() > 1000 {
        return Err(bad(C, "exceeded 1000 text runs"));
    }
    let new_paragraphs = match explicit_paragraphs {
        Some(paragraphs) => paragraphs,
        None => align_paragraphs(&old_plain, parts.paragraphs, &new_text),
    };
    *parts.runs = new_runs;
    *parts.paragraphs = new_paragraphs;
    *parts.text = new_text;
    if let ObjectKind::Text {
        text,
        style,
        runs,
        paragraphs,
        autofit,
        point_text,
        ..
    } = &obj.kind
    {
        if *point_text {
            fit_point_text_frame(
                &mut obj.frame,
                text,
                runs,
                style,
                paragraphs,
                parse_measured_size(p),
                false,
            );
        } else if *autofit == Autofit::Grow {
            fit_grow_text_frame(
                &mut obj.frame,
                text,
                runs,
                style,
                paragraphs,
                parse_measured_size(p),
                false,
            );
        }
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn text_style_range(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "text.style_range";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let start: usize = req(C, p, "start")?;
    let end: usize = req(C, p, "end")?;
    let bold: Option<bool> = opt(C, p, "bold")?;
    let italic: Option<bool> = opt(C, p, "italic")?;
    let underline: Option<bool> = opt(C, p, "underline")?;
    let strikethrough: Option<bool> = opt(C, p, "strikethrough")?;
    let baseline: Option<coursecraft_course::Baseline> = opt(C, p, "baseline")?;
    let letter_spacing: Option<f32> = opt(C, p, "letter_spacing")?;
    if letter_spacing.is_some_and(|v| !v.is_finite()) {
        return Err(bad(C, "letter_spacing must be finite"));
    }
    let size: Option<f32> = opt(C, p, "size")?;
    let color_str: Option<String> = opt(C, p, "color")?;
    let font: Option<String> = opt(C, p, "font")?;
    let shadow: Option<bool> = opt(C, p, "shadow")?;
    // `highlight` is a colour, or `null` to remove the highlight. Absent leaves it alone.
    let highlight: Option<Option<Color>> = match p.get("highlight") {
        None => None,
        Some(Json::Null) => Some(None),
        Some(value) => {
            let text: String = serde_json::from_value(value.clone())
                .map_err(|e| bad(C, format!("`highlight`: {e}")))?;
            let color = Color(text);
            validate_color(C, &color)?;
            Some(Some(color))
        }
    };

    if let Some(sz) = size
        && (!sz.is_finite() || sz <= 0.0)
    {
        return Err(bad(C, "size must be a finite positive number"));
    }
    let color = if let Some(cs) = color_str {
        let col = Color(cs);
        validate_color(C, &col)?;
        Some(col)
    } else {
        None
    };

    let sl = slide_mut(s, &sid)?;
    let obj = find_object_mut(sl, &oid)?;
    let Some(parts) = text_parts_at(&mut obj.kind, p, C)? else {
        return Err(bad(C, format!("object {oid:?} is not a text object")));
    };
    let (range_start, range_end) = clamped_range(start, end, parts.text.chars().count());

    let patch = TextRunPatch {
        bold,
        italic,
        underline,
        size,
        color,
        font,
        link: None,
        strikethrough,
        baseline,
        letter_spacing,
        highlight,
        shadow,
    };
    let new_runs = apply_style_to_range_with_base(
        parts.runs,
        parts.text,
        (range_start, range_end),
        &patch,
        Some(parts.style),
    );
    if new_runs.len() > 1000 {
        return Err(bad(C, "exceeded 1000 text runs"));
    }
    *parts.runs = new_runs;
    refit_text_frame(obj);
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

/// Refit point text and Grow boxes after the characters or their formatting changed.
fn refit_text_frame(obj: &mut coursecraft_course::Object) {
    if let ObjectKind::Text {
        text,
        style,
        runs,
        paragraphs,
        autofit,
        point_text,
        ..
    } = &obj.kind
    {
        if *point_text && !style.wrap {
            fit_point_text_frame(&mut obj.frame, text, runs, style, paragraphs, None, false);
        } else if !*point_text && *autofit == Autofit::Grow {
            fit_grow_text_frame(&mut obj.frame, text, runs, style, paragraphs, None, false);
        }
    }
}

/// The character range `start..end` clamped to the text; `0..0` means the whole text.
fn clamped_range(start: usize, end: usize, total_chars: usize) -> (usize, usize) {
    if start == 0 && end == 0 {
        (0, 0)
    } else {
        let s_clamp = start.min(total_chars);
        let e_clamp = end.min(total_chars);
        (s_clamp.min(e_clamp), s_clamp.max(e_clamp))
    }
}

/// Like [`clamped_range`], for case changes and Clear Formatting: `None` when the
/// selection is empty after clamping (`start == end` other than `0..0`, or both
/// past the end), so there is nothing to change.
fn selection_range(start: usize, end: usize, total_chars: usize) -> Option<(usize, usize)> {
    if start == 0 && end == 0 {
        return Some((0, 0));
    }
    let s_clamp = start.min(total_chars);
    let e_clamp = end.min(total_chars);
    let range = (s_clamp.min(e_clamp), s_clamp.max(e_clamp));
    (range.0 != range.1).then_some(range)
}

pub(super) fn text_clear_formatting(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "text.clear_formatting";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let start: usize = req(C, p, "start")?;
    let end: usize = req(C, p, "end")?;

    let sl = slide_mut(s, &sid)?;
    let obj = find_object_mut(sl, &oid)?;
    let Some(parts) = text_parts_at(&mut obj.kind, p, C)? else {
        return Err(bad(C, format!("object {oid:?} is not a text object")));
    };
    let Some(range) = selection_range(start, end, parts.text.chars().count()) else {
        return Ok(Json::Null);
    };
    *parts.runs = coursecraft_course::clear_formatting_in_range(parts.runs, parts.text, range);
    refit_text_frame(obj);
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn text_change_case(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "text.change_case";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let start: usize = req(C, p, "start")?;
    let end: usize = req(C, p, "end")?;
    let case_name: String = req(C, p, "case")?;
    let change = match case_name.as_str() {
        "sentence" => CaseChange::Sentence,
        "lower" => CaseChange::Lower,
        "upper" => CaseChange::Upper,
        "title" => CaseChange::Title,
        "toggle" => CaseChange::Toggle,
        other => return Err(bad(C, format!("unknown case {other:?}"))),
    };

    let sl = slide_mut(s, &sid)?;
    let obj = find_object_mut(sl, &oid)?;
    let Some(parts) = text_parts_at(&mut obj.kind, p, C)? else {
        return Err(bad(C, format!("object {oid:?} is not a text object")));
    };
    // The runs hold the displayed text when there are runs.
    let plain: String = if parts.runs.is_empty() {
        parts.text.clone()
    } else {
        parts.runs.iter().map(|r| r.text.as_str()).collect()
    };
    let Some(range) = selection_range(start, end, plain.chars().count()) else {
        return Ok(Json::Null);
    };
    let (new_plain, new_runs) =
        coursecraft_course::change_case_in_range(parts.runs, &plain, range, change);
    if new_plain.len() > super::MAX_TEXT {
        return Err(bad(C, "the changed text is too long"));
    }
    *parts.runs = new_runs;
    *parts.text = new_plain;
    refit_text_frame(obj);
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn text_set_paragraph(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "text.set_paragraph";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let align_str: Option<String> = opt(C, p, "align")?;
    let valign_str: Option<String> = opt(C, p, "valign")?;
    let wrap: Option<bool> = opt(C, p, "wrap")?;
    let dir_str: Option<String> = opt(C, p, "direction")?;
    let insets_val: Option<coursecraft_course::TextInsets> = opt(C, p, "insets")?;

    let align = match align_str.as_deref() {
        None => None,
        Some("left") => Some(Align::Left),
        Some("center") => Some(Align::Center),
        Some("right") => Some(Align::Right),
        Some("justify") => Some(Align::Justify),
        Some(other) => return Err(bad(C, format!("unknown align {other:?}"))),
    };
    let valign = match valign_str.as_deref() {
        None => None,
        Some("top") => Some(VAlign::Top),
        Some("middle") => Some(VAlign::Middle),
        Some("bottom") => Some(VAlign::Bottom),
        Some(other) => return Err(bad(C, format!("unknown valign {other:?}"))),
    };
    let direction = match dir_str.as_deref() {
        None => None,
        Some("horizontal") => Some(coursecraft_course::TextDirection::Horizontal),
        Some("vertical") => Some(coursecraft_course::TextDirection::Vertical),
        Some("vertical270") => Some(coursecraft_course::TextDirection::Vertical270),
        Some(other) => return Err(bad(C, format!("unknown direction {other:?}"))),
    };

    let sl = slide_mut(s, &sid)?;
    let obj = find_object_mut(sl, &oid)?;
    let Some(mut parts) = text_parts_at(&mut obj.kind, p, C)? else {
        return Err(bad(C, format!("object {oid:?} is not a text object")));
    };
    if let Some(a) = align {
        parts.style.align = a;
    }
    if let Some(v) = valign {
        if let Some(slot) = parts.valign.as_deref_mut() {
            *slot = v;
        } else {
            parts.style.valign = v;
        }
    }
    if let Some(w) = wrap {
        parts.style.wrap = w;
    }
    if let Some(d) = direction {
        parts.style.direction = d;
    }
    if let Some(ins) = insets_val {
        parts.style.insets = Some(ins);
    }
    if let ObjectKind::Text {
        text,
        style,
        runs,
        paragraphs,
        autofit,
        point_text,
        ..
    } = &mut obj.kind
    {
        if wrap == Some(true) {
            *point_text = false;
        }
        if *point_text && !style.wrap && (insets_val.is_some() || direction.is_some()) {
            fit_point_text_frame(&mut obj.frame, text, runs, style, paragraphs, None, false);
        } else if !*point_text
            && *autofit == Autofit::Grow
            && (wrap.is_some() || insets_val.is_some() || direction.is_some())
        {
            fit_grow_text_frame(&mut obj.frame, text, runs, style, paragraphs, None, false);
        }
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn trim_paragraphs(paragraphs: &mut Vec<Paragraph>) {
    while paragraphs.last() == Some(&Paragraph::default()) {
        paragraphs.pop();
    }
}

pub(super) fn text_set_paragraphs(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "text.set_paragraphs";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let start: Option<usize> = opt(C, p, "start")?;
    let end: Option<usize> = opt(C, p, "end")?;
    let list_str: Option<String> = opt(C, p, "list")?;
    let level: Option<u8> = opt(C, p, "level")?;
    let line_spacing: Option<f32> = opt(C, p, "line_spacing")?;
    let space_before: Option<f32> = opt(C, p, "space_before")?;
    let space_after: Option<f32> = opt(C, p, "space_after")?;
    // A numbering scheme (`null` goes back to the level's default) and the first number
    // (`null` starts at 1). Absent leaves the paragraph alone.
    let number_scheme: Option<Option<coursecraft_course::NumberScheme>> =
        match p.get("number_scheme") {
            None => None,
            Some(Json::Null) => Some(None),
            Some(value) => Some(Some(
                serde_json::from_value(value.clone())
                    .map_err(|e| bad(C, format!("`number_scheme`: {e}")))?,
            )),
        };
    let start_at: Option<Option<u32>> = match p.get("start_at") {
        None => None,
        Some(Json::Null) => Some(None),
        Some(value) => {
            let number: u32 = serde_json::from_value(value.clone())
                .map_err(|e| bad(C, format!("`start_at`: {e}")))?;
            if !(1..=99_999).contains(&number) {
                return Err(bad(C, "start_at must be within 1..=99999"));
            }
            Some(Some(number))
        }
    };

    let list = match list_str.as_deref() {
        None => None,
        Some("none") => Some(ListKind::None),
        Some("bullet") => Some(ListKind::Bullet),
        Some("number") => Some(ListKind::Number),
        Some(other) => return Err(bad(C, format!("unknown list {other:?}"))),
    };
    if let Some(level) = level
        && level > 4
    {
        return Err(bad(C, "level must be within 0..=4"));
    }
    if let Some(spacing) = line_spacing
        && (!spacing.is_finite() || !(0.8..=3.0).contains(&spacing))
    {
        return Err(bad(C, "line_spacing must be finite and within 0.8..=3"));
    }
    for (name, value) in [("space_before", space_before), ("space_after", space_after)] {
        if let Some(value) = value
            && (!value.is_finite() || !(0.0..=200.0).contains(&value))
        {
            return Err(bad(C, format!("{name} must be finite and within 0..=200")));
        }
    }

    let sl = slide_mut(s, &sid)?;
    let obj = find_object_mut(sl, &oid)?;
    let Some(parts) = text_parts_at(&mut obj.kind, p, C)? else {
        return Err(bad(C, format!("object {oid:?} is not a text object")));
    };
    let count = coursecraft_course::text::paragraph_count(parts.text);
    let first = start.unwrap_or(0).min(count);
    let last = end.unwrap_or(count).clamp(first, count);
    let mut paragraphs = paragraphs_for(parts.text, parts.paragraphs);
    for paragraph in paragraphs.iter_mut().skip(first).take(last - first) {
        if let Some(list) = list {
            paragraph.list = list;
        }
        if let Some(level) = level {
            paragraph.level = level;
        }
        if let Some(spacing) = line_spacing {
            paragraph.line_spacing = spacing;
        }
        if let Some(value) = space_before {
            paragraph.space_before = value;
        }
        if let Some(value) = space_after {
            paragraph.space_after = value;
        }
        if let Some(scheme) = number_scheme {
            paragraph.number_scheme = scheme;
        }
        if let Some(number) = start_at {
            paragraph.start_at = number;
        }
    }
    trim_paragraphs(&mut paragraphs);
    *parts.paragraphs = paragraphs;
    if let ObjectKind::Text {
        text,
        style,
        runs,
        paragraphs,
        autofit,
        point_text,
        ..
    } = &obj.kind
    {
        if *point_text && !style.wrap {
            fit_point_text_frame(&mut obj.frame, text, runs, style, paragraphs, None, false);
        } else if !*point_text && *autofit == Autofit::Grow {
            fit_grow_text_frame(&mut obj.frame, text, runs, style, paragraphs, None, false);
        }
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn text_set_link(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "text.set_link";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let start: Option<usize> = opt(C, p, "start")?;
    let end: Option<usize> = opt(C, p, "end")?;
    let link: Option<Link> = opt(C, p, "link")?;

    if let Some(link) = &link {
        match link {
            Link::Url { url } if !is_valid_link_url(url) => {
                return Err(bad(
                    C,
                    format!("link {url:?} must start with http://, https:// or mailto:"),
                ));
            }
            Link::Slide { slide } if s.course.play_slide(slide).is_none() => {
                return Err(bad(
                    C,
                    format!("link target slide {slide:?} does not exist"),
                ));
            }
            _ => {}
        }
    }

    let sl = slide_mut(s, &sid)?;
    let obj = find_object_mut(sl, &oid)?;
    let Some(parts) = text_parts_at(&mut obj.kind, p, C)? else {
        return Err(bad(C, format!("object {oid:?} is not a text object")));
    };
    let total_chars = parts.text.chars().count();
    let start = start.unwrap_or(0).min(total_chars);
    let end = end.unwrap_or(total_chars).min(total_chars);
    let (start, end) = if start == 0 && end == total_chars {
        (0, 0)
    } else {
        (start.min(end), start.max(end))
    };
    let new_runs = apply_link_to_range(parts.runs, parts.text, (start, end), link);
    if new_runs.len() > 1000 {
        return Err(bad(C, "exceeded 1000 text runs"));
    }
    *parts.runs = new_runs;
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn object_set_autofit(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.set_autofit";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let autofit: Autofit = req(C, p, "autofit")?;

    let sl = slide_mut(s, &sid)?;
    let obj = find_object_mut(sl, &oid)?;
    let Some(parts) = text_parts_at(&mut obj.kind, p, C)? else {
        return Err(bad(C, format!("object {oid:?} is not a text object")));
    };
    let Some(current) = parts.autofit else {
        return Err(bad(C, "a table cell has no autofit setting"));
    };
    *current = autofit;
    if matches!(autofit, Autofit::Shrink | Autofit::Grow)
        && let ObjectKind::Text {
            point_text, style, ..
        } = &mut obj.kind
    {
        if *point_text && autofit == Autofit::Grow {
            style.wrap = true;
        }
        *point_text = false;
    }
    if autofit == Autofit::Grow
        && let ObjectKind::Text {
            text,
            style,
            runs,
            paragraphs,
            point_text: false,
            ..
        } = &obj.kind
    {
        fit_grow_text_frame(&mut obj.frame, text, runs, style, paragraphs, None, false);
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}
