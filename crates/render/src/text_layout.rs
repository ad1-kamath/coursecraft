//! Paragraph layout for slide text.
//!
//! One function lays out bullets, numbering, indents, line spacing, spacing
//! before and after, links and shrink-to-fit text. The renderer, the hit
//! regions and the tests all use it, so a bullet in the canvas is the same
//! bullet in preview and in the published player.

use std::sync::Arc;

use coursecraft_course::{Align, Autofit, Link, ListKind, Paragraph, VAlign};
use egui::epaint::text::{LayoutJob, TextFormat, TextWrapping};
use egui::{Color32, Context, Pos2, Rect, Stroke, pos2};

use crate::compose::ResolvedTextRun;
use crate::compose::{DrawItem, ResolvedLinkRect, Visual, validate_frame};

/// Smallest scale shrink-to-fit may use.
pub const AUTOFIT_MIN_SCALE: f32 = 0.4;
/// Indent added for each paragraph level, in slide pixels before scaling.
pub const INDENT_STEP: f32 = 24.0;
/// Default marker colour when a paragraph has no run.
const FALLBACK_COLOR: Color32 = Color32::from_rgb(0x1F, 0x1F, 0x1F);
const FALLBACK_SIZE: f32 = 20.0;

/// One laid-out marker (bullet or number).
#[derive(Debug, Clone)]
pub struct LaidMarker {
    pub galley: Arc<egui::Galley>,
    /// Screen-space top-left passed to `Painter::galley`. For a right-aligned
    /// marker this is its right edge, as egui expects.
    pub pos: Pos2,
}

/// One laid-out paragraph.
#[derive(Debug, Clone)]
pub struct LaidParagraph {
    pub galley: Arc<egui::Galley>,
    /// Screen-space position passed to `Painter::galley`. For centre and right
    /// alignment this is the content centre or right edge.
    pub pos: Pos2,
    pub marker: Option<LaidMarker>,
    pub indent: f32,
    pub height: f32,
    /// Underlined character ranges of this paragraph and their strokes. Drawn by
    /// [`underline_segments`] at the baseline, because egui puts an underline at
    /// the bottom of the line box, which line spacing pushes away from the text.
    pub underlines: Vec<(usize, usize, Stroke)>,
}

/// A link region in screen space.
#[derive(Debug, Clone, PartialEq)]
pub struct LaidLink {
    pub link: Link,
    pub text: String,
    pub rect: Rect,
}

/// The result of laying out a text box.
#[derive(Debug, Clone)]
pub struct LaidText {
    pub paragraphs: Vec<LaidParagraph>,
    pub links: Vec<LaidLink>,
    pub height: f32,
    pub width: f32,
    pub shrink: f32,
}

impl LaidText {
    /// Whether every paragraph fits inside `rect`.
    #[must_use]
    pub fn fits(&self, rect: Rect) -> bool {
        self.height <= rect.height().max(0.0) + 0.5 && self.width <= rect.width().max(0.0) + 0.5
    }
}

/// Lay out `runs` inside `rect`, applying paragraph formatting and shrink-to-fit.
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn layout_text(
    ctx: &Context,
    rect: Rect,
    scale: f32,
    runs: &[ResolvedTextRun],
    paragraphs: &[Paragraph],
    align: Align,
    valign: VAlign,
    autofit: Autofit,
) -> LaidText {
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    let rect = if [rect.min.x, rect.min.y, rect.max.x, rect.max.y]
        .iter()
        .all(|value| value.is_finite())
        && rect.width() > 0.0
        && rect.height() > 0.0
    {
        rect
    } else {
        Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(1.0, 1.0))
    };

    let full = layout_at(ctx, rect, scale, runs, paragraphs, align, valign, 1.0);
    if autofit != Autofit::Shrink || full.fits(rect) {
        return full;
    }

    let floor = layout_at(
        ctx,
        rect,
        scale,
        runs,
        paragraphs,
        align,
        valign,
        AUTOFIT_MIN_SCALE,
    );
    if !floor.fits(rect) {
        return floor;
    }

    // Largest scale in the floor..=1 range that fits.
    let mut low = AUTOFIT_MIN_SCALE;
    let mut high = 1.0;
    for _ in 0..20 {
        let mid = (low + high) * 0.5;
        let candidate = layout_at(ctx, rect, scale, runs, paragraphs, align, valign, mid);
        if candidate.fits(rect) {
            low = mid;
        } else {
            high = mid;
        }
    }
    layout_at(ctx, rect, scale, runs, paragraphs, align, valign, low)
}

/// Split runs at newlines. Every paragraph has at least one run so an empty
/// line still gets the font's row height.
fn split_paragraphs(runs: &[ResolvedTextRun]) -> Vec<Vec<ResolvedTextRun>> {
    let mut paragraphs: Vec<Vec<ResolvedTextRun>> = vec![Vec::new()];
    for run in runs.iter().take(2000) {
        let mut first = true;
        for piece in run.text.split('\n') {
            if !first {
                paragraphs.push(Vec::new());
            }
            first = false;
            if !piece.is_empty() {
                let mut part = run.clone();
                part.text = piece.to_owned();
                if let Some(target) = paragraphs.last_mut() {
                    target.push(part);
                }
            }
        }
    }
    for paragraph in &mut paragraphs {
        if paragraph.is_empty() {
            paragraph.push(ResolvedTextRun {
                text: " ".to_owned(),
                size: FALLBACK_SIZE,
                color: FALLBACK_COLOR,
                bold: false,
                italic: false,
                underline: false,
                font: None,
                link: None,
                strikethrough: false,
                baseline: coursecraft_course::Baseline::Normal,
                letter_spacing: 0.0,
            });
        }
    }
    paragraphs
}

/// Superscript and subscript text draws at this fraction of its size.
pub const SCRIPT_SCALE: f32 = 0.65;

fn marker_for(
    counters: &mut [u32; 5],
    previous: &mut Option<(ListKind, u8)>,
    list: ListKind,
    level: u8,
) -> Option<String> {
    let level = usize::from(level.min(4));
    match list {
        ListKind::None => {
            *counters = [0; 5];
            *previous = Some((ListKind::None, level as u8));
            None
        }
        ListKind::Bullet => {
            *counters = [0; 5];
            *previous = Some((ListKind::Bullet, level as u8));
            Some(
                match level % 3 {
                    0 => "•",
                    1 => "◦",
                    _ => "▪",
                }
                .to_owned(),
            )
        }
        ListKind::Number => {
            let continues =
                matches!(*previous, Some((ListKind::Number, prev)) if usize::from(prev) == level);
            let counter = counters.get_mut(level)?;
            if continues {
                *counter = counter.saturating_add(1);
            } else {
                *counter = 1;
            }
            let number = *counter;
            for deeper in counters.iter_mut().skip(level.saturating_add(1)) {
                *deeper = 0;
            }
            *previous = Some((ListKind::Number, level as u8));
            Some(format_number(number, level))
        }
    }
}

/// `1.`, `a.`, `i.` by level, cycling every three levels.
fn format_number(number: u32, level: usize) -> String {
    let value = number.max(1);
    match level % 3 {
        0 => format!("{value}."),
        1 => format!("{}.", alpha(value)),
        _ => format!("{}.", roman(value)),
    }
}

fn alpha(mut value: u32) -> String {
    let mut out = String::new();
    while value > 0 {
        let digit = ((value - 1) % 26) as u8;
        out.insert(0, char::from(b'a' + digit));
        value = (value - 1) / 26;
    }
    if out.is_empty() {
        out.push('a');
    }
    out
}

fn roman(value: u32) -> String {
    const PAIRS: &[(u32, &str)] = &[
        (1000, "m"),
        (900, "cm"),
        (500, "d"),
        (400, "cd"),
        (100, "c"),
        (90, "xc"),
        (50, "l"),
        (40, "xl"),
        (10, "x"),
        (9, "ix"),
        (5, "v"),
        (4, "iv"),
        (1, "i"),
    ];
    let mut value = value;
    let mut out = String::new();
    for (amount, glyph) in PAIRS {
        while value >= *amount {
            out.push_str(glyph);
            value -= amount;
        }
    }
    if out.is_empty() {
        out.push('i');
    }
    out
}

fn halign(align: Align) -> egui::Align {
    match align {
        Align::Left => egui::Align::LEFT,
        Align::Center => egui::Align::Center,
        Align::Right => egui::Align::RIGHT,
    }
}

/// Distance from a galley's top to the baseline of its first glyph.
fn first_baseline(galley: &egui::Galley) -> Option<f32> {
    let row = galley.rows.first()?;
    let glyph = row.glyphs.first()?;
    let y = row.pos.y + glyph.pos.y;
    y.is_finite().then_some(y)
}

fn marker_text(
    ctx: &Context,
    text: &str,
    run: &ResolvedTextRun,
    scale: f32,
    shrink: f32,
    color: Color32,
) -> Option<(Arc<egui::Galley>, f32)> {
    let size = (run.size.clamp(1.0, 400.0) * scale * shrink).max(1.0);
    let font_id =
        crate::font::slide_font_named(ctx, run.font.as_deref(), run.bold, run.italic, size);
    let mut job = LayoutJob {
        halign: egui::Align::RIGHT,
        break_on_newline: false,
        ..Default::default()
    };
    job.append(
        text,
        0.0,
        TextFormat {
            font_id,
            color,
            ..Default::default()
        },
    );
    let galley = ctx.fonts_mut(|fonts| fonts.layout_job(job));
    let width = galley.size().x;
    Some((galley, width))
}

#[allow(clippy::too_many_arguments)]
fn layout_at(
    ctx: &Context,
    rect: Rect,
    scale: f32,
    runs: &[ResolvedTextRun],
    paragraphs: &[Paragraph],
    align: Align,
    valign: VAlign,
    shrink: f32,
) -> LaidText {
    let shrink = if shrink.is_finite() {
        shrink.clamp(AUTOFIT_MIN_SCALE, 1.0)
    } else {
        1.0
    };
    let split = split_paragraphs(runs);
    let mut laid = Vec::with_capacity(split.len());
    let mut links = Vec::new();
    let mut y = 0.0f32;
    let mut max_width = 0.0f32;
    let mut counters = [0u32; 5];
    let mut previous: Option<(ListKind, u8)> = None;

    for (index, paragraph_runs) in split.iter().enumerate() {
        let style = paragraphs.get(index).cloned().unwrap_or_default();
        let level = style.level.min(4);
        let spacing = if style.line_spacing.is_finite() {
            style.line_spacing.clamp(0.8, 3.0)
        } else {
            1.0
        };
        let space_before = if style.space_before.is_finite() {
            style.space_before.clamp(0.0, 200.0)
        } else {
            0.0
        };
        let space_after = if style.space_after.is_finite() {
            style.space_after.clamp(0.0, 200.0)
        } else {
            0.0
        };
        let step = INDENT_STEP * scale * shrink;
        let indent = f32::from(level) * step;
        let text_indent = if style.list == ListKind::None {
            indent
        } else {
            indent + step
        };
        let content_width = (rect.width() - text_indent).max(1.0);
        let marker = marker_for(&mut counters, &mut previous, style.list, level);

        let mut job = LayoutJob {
            halign: halign(align),
            wrap: TextWrapping {
                max_width: content_width,
                ..Default::default()
            },
            break_on_newline: false,
            ..Default::default()
        };
        let mut spans: Vec<(usize, usize, Link, String)> = Vec::new();
        let mut underlines: Vec<(usize, usize, Stroke)> = Vec::new();
        let mut char_count = 0usize;
        for run in paragraph_runs {
            let start = char_count;
            char_count = char_count.saturating_add(run.text.chars().count());
            let size = (run.size.clamp(1.0, 400.0) * scale * shrink).max(1.0);
            // Superscript and subscript draw smaller; the format's valign moves them.
            let (size, valign) = match run.baseline {
                coursecraft_course::Baseline::Normal => (size, egui::Align::BOTTOM),
                coursecraft_course::Baseline::Superscript => {
                    ((size * SCRIPT_SCALE).max(1.0), egui::Align::TOP)
                }
                coursecraft_course::Baseline::Subscript => {
                    ((size * SCRIPT_SCALE).max(1.0), egui::Align::BOTTOM)
                }
            };
            let letter_spacing =
                coursecraft_course::clamp_letter_spacing(run.letter_spacing) * scale * shrink;
            let strike = if run.strikethrough {
                Stroke::new((scale * shrink).max(1.0), run.color)
            } else {
                Stroke::NONE
            };
            let font_id =
                crate::font::slide_font_named(ctx, run.font.as_deref(), run.bold, run.italic, size);
            let row_height = ctx.fonts_mut(|fonts| fonts.row_height(&font_id));
            let line_height = (row_height * spacing).max(size * 0.5);
            let underline = if run.underline || run.link.is_some() {
                Stroke::new((scale * shrink).max(1.0), run.color)
            } else {
                Stroke::NONE
            };
            job.append(
                &run.text,
                0.0,
                TextFormat {
                    font_id,
                    color: run.color,
                    line_height: Some(line_height),
                    strikethrough: strike,
                    extra_letter_spacing: letter_spacing,
                    valign,
                    ..Default::default()
                },
            );
            if underline != Stroke::NONE {
                underlines.push((start, char_count, underline));
            }
            if let Some(link) = &run.link {
                spans.push((start, char_count, link.clone(), run.text.clone()));
            }
        }
        let galley = ctx.fonts_mut(|fonts| fonts.layout_job(job));
        let galley_h = galley.size().y;
        let indent_width = text_indent + galley.size().x;
        max_width = max_width.max(indent_width);

        let x = match align {
            Align::Left => rect.left() + text_indent,
            Align::Center => rect.left() + text_indent + content_width * 0.5,
            Align::Right => rect.right(),
        };
        let top = y + space_before;
        let paragraph_pos = pos2(x, rect.top() + top);
        let paragraph_galley = galley.clone();
        let marker = marker.and_then(|text| {
            let marker_run = paragraph_runs.first()?;
            let (galley, width) =
                marker_text(ctx, &text, marker_run, scale, shrink, marker_run.color)?;
            let right = rect.left() + text_indent - (6.0 * scale * shrink).max(1.0);
            let _ = width;
            // Sit the marker on the first line's baseline. Line spacing makes the
            // paragraph's rows taller than the marker's, so equal tops would
            // leave the marker floating above the text.
            let shift = match (first_baseline(&paragraph_galley), first_baseline(&galley)) {
                (Some(text), Some(mark)) => text - mark,
                _ => 0.0,
            };
            Some(LaidMarker {
                galley,
                pos: pos2(right, rect.top() + top + shift),
            })
        });
        laid.push(LaidParagraph {
            galley: galley.clone(),
            pos: paragraph_pos,
            marker,
            indent: text_indent,
            height: space_before + galley_h + space_after,
            underlines,
        });

        links.extend(collect_links(&galley, paragraph_pos, &spans));
        y = y + space_before + galley_h + space_after;
    }

    let y_offset = match valign {
        VAlign::Top => 0.0,
        VAlign::Middle => ((rect.height() - y).max(0.0)) * 0.5,
        VAlign::Bottom => (rect.height() - y).max(0.0),
    };
    if y_offset > 0.0 {
        for paragraph in &mut laid {
            paragraph.pos.y += y_offset;
            if let Some(marker) = &mut paragraph.marker {
                marker.pos.y += y_offset;
            }
        }
        for link in &mut links {
            link.rect = link.rect.translate(egui::vec2(0.0, y_offset));
        }
    }

    LaidText {
        paragraphs: laid,
        links,
        height: y,
        width: max_width,
        shrink,
    }
}

/// Turn per-character glyph boxes into one rect per linked span per row.
/// Screen-space underline strokes for a laid paragraph drawn at `origin`:
/// one segment per run of underlined glyphs on a row, just below the baseline.
#[must_use]
pub fn underline_segments(paragraph: &LaidParagraph, origin: Pos2) -> Vec<([Pos2; 2], Stroke)> {
    let mut out = Vec::new();
    if paragraph.underlines.is_empty() {
        return out;
    }
    let mut char_index = 0usize;
    for row in &paragraph.galley.rows {
        let mut active: Option<(f32, f32, f32, Stroke)> = None;
        for glyph in &row.glyphs {
            let stroke = paragraph
                .underlines
                .iter()
                .find(|(start, end, _)| char_index >= *start && char_index < *end)
                .map(|(_, _, stroke)| *stroke);
            let left = origin.x + row.pos.x + glyph.pos.x;
            let right = left + glyph.advance_width;
            let descent = (glyph.font_height - glyph.font_ascent).max(0.0);
            let y = origin.y + row.pos.y + glyph.pos.y + (descent * 0.45).max(1.0);
            match (stroke, &mut active) {
                (Some(stroke), Some((_, end, _, current))) if *current == stroke => {
                    *end = right;
                }
                (Some(stroke), _) => {
                    if let Some((start, end, y0, s0)) = active.take() {
                        out.push(([pos2(start, y0), pos2(end, y0)], s0));
                    }
                    active = Some((left, right, y, stroke));
                }
                (None, _) => {
                    if let Some((start, end, y0, s0)) = active.take() {
                        out.push(([pos2(start, y0), pos2(end, y0)], s0));
                    }
                }
            }
            char_index = char_index.saturating_add(1);
        }
        if let Some((start, end, y0, s0)) = active.take() {
            out.push(([pos2(start, y0), pos2(end, y0)], s0));
        }
    }
    out
}

fn collect_links(
    galley: &egui::Galley,
    origin: Pos2,
    spans: &[(usize, usize, Link, String)],
) -> Vec<LaidLink> {
    let mut out: Vec<LaidLink> = Vec::new();
    let mut char_index = 0usize;
    for row in &galley.rows {
        let mut active: Option<(usize, Rect)> = None;
        for glyph in &row.glyphs {
            let span = spans
                .iter()
                .position(|(start, end, _, _)| char_index >= *start && char_index < *end);
            if let Some(span_index) = span {
                let rect = Rect::from_min_size(
                    pos2(origin.x + row.pos.x + glyph.pos.x, origin.y + row.pos.y),
                    egui::vec2(glyph.advance_width, row.size.y),
                );
                match &mut active {
                    Some((index, current)) if *index == span_index => {
                        *current = current.union(rect);
                    }
                    _ => {
                        flush_link(&mut out, &mut active, spans);
                        active = Some((span_index, rect));
                    }
                }
            } else {
                flush_link(&mut out, &mut active, spans);
            }
            char_index = char_index.saturating_add(1);
        }
        flush_link(&mut out, &mut active, spans);
    }
    out
}

fn flush_link(
    out: &mut Vec<LaidLink>,
    active: &mut Option<(usize, Rect)>,
    spans: &[(usize, usize, Link, String)],
) {
    if let Some((index, rect)) = active.take()
        && let Some((_, _, link, text)) = spans.get(index)
    {
        out.push(LaidLink {
            link: link.clone(),
            text: text.clone(),
            rect,
        });
    }
}

/// Fill each text item's link regions from the same layout the painter uses.
///
/// Runs in slide coordinates with `scale = 1`, then converts the screen-space
/// result back to fractions of the item frame so hit testing, rotation and the
/// focus ring all agree with what is drawn. No play-time decision is made here.
pub fn fill_item_links(ctx: &Context, items: &mut [DrawItem]) {
    for item in items {
        let mut found = Vec::new();
        let frame = if item.rect.is_positive() {
            item.rect
        } else {
            validate_frame(&coursecraft_course::Rect {
                x: item.rect.min.x,
                y: item.rect.min.y,
                w: item.rect.width(),
                h: item.rect.height(),
            })
            .unwrap_or(Rect::NOTHING)
        };
        if !frame.is_positive() {
            item.links = Vec::new();
            continue;
        }
        match &item.visual {
            Visual::Text {
                runs,
                align,
                valign,
                paragraphs,
                autofit,
            } => {
                let laid =
                    layout_text(ctx, frame, 1.0, runs, paragraphs, *align, *valign, *autofit);
                found.extend(to_fractions(frame, laid.links));
            }
            Visual::Shape {
                shape,
                adjust,
                text: Some(text),
                ..
            } => {
                let (tx, ty, tw, th) = crate::shape_geometry::shape_text_rect(
                    *shape,
                    frame.width(),
                    frame.height(),
                    adjust,
                );
                let text_rect = Rect::from_min_size(
                    pos2(frame.min.x + tx, frame.min.y + ty),
                    egui::vec2(tw, th),
                );
                if text_rect.is_positive() {
                    let laid = layout_text(
                        ctx,
                        text_rect,
                        1.0,
                        &text.runs,
                        &text.paragraphs,
                        text.align,
                        text.valign,
                        text.autofit,
                    );
                    found.extend(to_fractions(frame, laid.links));
                }
            }
            _ => {}
        }
        item.links = found;
    }
}

fn to_fractions(frame: Rect, links: Vec<LaidLink>) -> Vec<ResolvedLinkRect> {
    let width = frame.width();
    let height = frame.height();
    links
        .into_iter()
        .filter_map(|link| {
            if !(width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0) {
                return None;
            }
            let rect = Rect::from_min_max(
                pos2(
                    ((link.rect.min.x - frame.min.x) / width).clamp(0.0, 1.0),
                    ((link.rect.min.y - frame.min.y) / height).clamp(0.0, 1.0),
                ),
                pos2(
                    ((link.rect.max.x - frame.min.x) / width).clamp(0.0, 1.0),
                    ((link.rect.max.y - frame.min.y) / height).clamp(0.0, 1.0),
                ),
            );
            rect.is_positive().then_some(ResolvedLinkRect {
                link: link.link,
                rect,
                text: link.text,
            })
        })
        .collect()
}
