//! Headless, WASM-safe unwrapped text measurement using `skrifa`.
//!
//! Provides the single workspace copy of the four bundled Inter faces and
//! pure-Rust text measurement for point-text auto-fitting in `coursecraft-engine`.
//! All numeric inputs are treated as hostile: non-finite values are sanitized
//! and outputs are clamped to finite bounds.

use skrifa::MetadataProvider as _;
use skrifa::prelude::{GlyphId, LocationRef, Size};

/// Bundled Inter Regular font bytes (single copy shared across the workspace).
pub const INTER_REGULAR: &[u8] = include_bytes!("../fonts/Inter-Regular.ttf");
/// Bundled Inter Bold font bytes (single copy shared across the workspace).
pub const INTER_BOLD: &[u8] = include_bytes!("../fonts/Inter-Bold.ttf");
/// Bundled Inter Italic font bytes (single copy shared across the workspace).
pub const INTER_ITALIC: &[u8] = include_bytes!("../fonts/Inter-Italic.ttf");
/// Bundled Inter Bold Italic font bytes (single copy shared across the workspace).
pub const INTER_BOLD_ITALIC: &[u8] = include_bytes!("../fonts/Inter-BoldItalic.ttf");

/// Bundled symbol fallback (a subset of DejaVu Sans, renamed as its licence requires).
///
/// It draws the common UI symbols Inter lacks, such as the check, cross and star marks.
pub const SYMBOLS_FALLBACK: &[u8] = include_bytes!("../fonts/SymbolsFallback.ttf");

/// Return `true` when a bundled font (Inter Regular or the symbol fallback) has a glyph for `ch`.
///
/// Whitespace and control characters count as drawable because they need no glyph.
#[must_use]
pub fn bundled_fonts_can_draw(ch: char) -> bool {
    if ch.is_whitespace() || ch.is_control() {
        return true;
    }
    [INTER_REGULAR, SYMBOLS_FALLBACK].iter().any(|bytes| {
        skrifa::FontRef::new(bytes)
            .ok()
            .and_then(|font| font.charmap().map(ch))
            .is_some_and(|gid| gid != GlyphId::NOTDEF)
    })
}

/// Horizontal padding added on each side of a point-text box (8.0 px total).
pub const POINT_TEXT_PAD_X: f32 = 4.0;
/// Vertical padding added on each side of a point-text box (4.0 px total).
pub const POINT_TEXT_PAD_Y: f32 = 2.0;
/// Minimum width of a fitted point-text box in slide pixels.
pub const POINT_TEXT_MIN_W: f32 = 20.0;
/// Maximum permitted width or height returned by measurement and box fitting.
pub const POINT_TEXT_MAX_DIM: f32 = 10_000.0;

const DEFAULT_FONT_SIZE: f32 = 20.0;
const MIN_FONT_SIZE: f32 = 1.0;
const MAX_FONT_SIZE: f32 = 400.0;
const MIN_LINE_SPACING: f32 = 0.8;
const MAX_LINE_SPACING: f32 = 3.0;
const MIN_LETTER_SPACING: f32 = -20.0;
const MAX_LETTER_SPACING: f32 = 50.0;
const MAX_PARAGRAPH_SPACE: f32 = 400.0;
const MAX_LIST_INDENT: f32 = 200.0;
const MIN_SCRIPT_SCALE: f32 = 0.1;
const MAX_SCRIPT_SCALE: f32 = 2.0;
const MAX_RUNS: usize = 2_000;
const MAX_CHARS: usize = 100_000;
/// Fallback line-height multiplier if font metrics cannot be read.
const FALLBACK_LINE_HEIGHT_RATIO: f32 = 1.25;

/// One styled text run to measure without wrapping.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeasuredRun<'a> {
    /// Text content of this run (may contain `\n` paragraph breaks).
    pub text: &'a str,
    /// Font size in slide pixels before `script_scale`.
    pub size: f32,
    /// Whether the bold face is active.
    pub bold: bool,
    /// Whether the italic face is active.
    pub italic: bool,
    /// Extra horizontal spacing added per glyph in slide pixels.
    pub letter_spacing: f32,
    /// Baseline scale factor (`1.0` for normal, `0.65` for superscript/subscript).
    pub script_scale: f32,
}

impl Default for MeasuredRun<'_> {
    fn default() -> Self {
        Self {
            text: "",
            size: DEFAULT_FONT_SIZE,
            bold: false,
            italic: false,
            letter_spacing: 0.0,
            script_scale: 1.0,
        }
    }
}

/// Per-paragraph layout metrics used during unwrapped measurement.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParagraphMetrics {
    /// Line-height multiplier (`1.0` by default).
    pub line_spacing: f32,
    /// Vertical space added before this paragraph in slide pixels.
    pub space_before: f32,
    /// Vertical space added after this paragraph in slide pixels.
    pub space_after: f32,
    /// Leading horizontal indent (list marker + nesting) in slide pixels.
    pub list_indent: f32,
}

impl Default for ParagraphMetrics {
    fn default() -> Self {
        Self {
            line_spacing: 1.0,
            space_before: 0.0,
            space_after: 0.0,
            list_indent: 0.0,
        }
    }
}

/// Optional inner text box insets in slide pixels (`left`, `top`, `right`, `bottom`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TextBoxInsets {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl TextBoxInsets {
    /// Zero insets on all sides.
    pub const ZERO: Self = Self {
        left: 0.0,
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
    };
}

fn sanitize(value: f32, fallback: f32, min: f32, max: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        fallback.clamp(min, max)
    }
}

fn font_bytes_for_style(bold: bool, italic: bool) -> &'static [u8] {
    match (bold, italic) {
        (false, false) => INTER_REGULAR,
        (true, false) => INTER_BOLD,
        (false, true) => INTER_ITALIC,
        (true, true) => INTER_BOLD_ITALIC,
    }
}

fn is_wide_fallback_char(ch: char) -> bool {
    let cp = ch as u32;
    matches!(
        cp,
        0x1100..=0x115F
            | 0x2190..=0x21FF
            | 0x2300..=0x23FF
            | 0x25A0..=0x27BF
            | 0x2900..=0x2BFF
            | 0x2E80..=0xA4CF
            | 0xAC00..=0xD7A3
            | 0xF900..=0xFAFF
            | 0xFE10..=0xFE6F
            | 0xFF00..=0xFF60
            | 0xFFE0..=0xFFE6
            | 0x1F000..=0x1FAFF
    )
}

fn fallback_advance(ch: char, scaled_size: f32) -> f32 {
    if is_wide_fallback_char(ch) {
        scaled_size * 1.05
    } else {
        scaled_size * 0.65
    }
}

fn font_row_height(font: Option<&skrifa::FontRef<'_>>, scaled_size: f32) -> f32 {
    let Some(font) = font else {
        return scaled_size * FALLBACK_LINE_HEIGHT_RATIO;
    };
    let metrics = font.metrics(Size::new(scaled_size), LocationRef::default());
    let raw = metrics.ascent - metrics.descent + metrics.leading;
    if raw.is_finite() && raw > 0.0 {
        raw.max(scaled_size)
    } else {
        scaled_size * FALLBACK_LINE_HEIGHT_RATIO
    }
}

fn measure_segment(
    segment: &str,
    font: Option<&skrifa::FontRef<'_>>,
    scaled_size: f32,
    letter_spacing: f32,
    remaining_chars: &mut usize,
) -> (f32, usize) {
    let mut width = 0.0_f32;
    let mut count = 0_usize;
    let size = Size::new(scaled_size);
    let location = LocationRef::default();
    let charmap = font.map(|f| f.charmap());
    let glyph_metrics = font.map(|f| f.glyph_metrics(size, location));

    for ch in segment.chars() {
        if *remaining_chars == 0 {
            break;
        }
        *remaining_chars -= 1;
        if ch == '\r' {
            continue;
        }

        let adv = match (&charmap, &glyph_metrics) {
            (Some(cm), Some(gm)) => match cm.map(ch) {
                Some(gid) if gid != GlyphId::NOTDEF => gm
                    .advance_width(gid)
                    .filter(|v| v.is_finite() && *v >= 0.0)
                    .unwrap_or_else(|| fallback_advance(ch, scaled_size)),
                _ => fallback_advance(ch, scaled_size),
            },
            _ => fallback_advance(ch, scaled_size),
        };
        width = (width + adv).min(POINT_TEXT_MAX_DIM);
        count = count.saturating_add(1);
    }

    if count > 0 && letter_spacing != 0.0 {
        let gaps = count.saturating_sub(1) as f32;
        let extra = if letter_spacing > 0.0 {
            // Match epaint's extra_letter_spacing across glyph advances.
            letter_spacing * (count as f32)
        } else {
            letter_spacing * gaps
        };
        width = (width + extra).clamp(0.0, POINT_TEXT_MAX_DIM);
    }
    (width, count)
}

struct ParagraphAccum {
    width: f32,
    row_height: f32,
    has_content: bool,
}

impl ParagraphAccum {
    fn new() -> Self {
        Self {
            width: 0.0,
            row_height: 0.0,
            has_content: false,
        }
    }

    fn finish(
        &mut self,
        para_idx: usize,
        paragraphs: &[ParagraphMetrics],
        default_row_h: f32,
        max_w: &mut f32,
        total_h: &mut f32,
    ) {
        let pm = paragraphs.get(para_idx).copied().unwrap_or_default();
        let line_spacing = sanitize(pm.line_spacing, 1.0, MIN_LINE_SPACING, MAX_LINE_SPACING);
        let space_before = sanitize(pm.space_before, 0.0, 0.0, MAX_PARAGRAPH_SPACE);
        let space_after = sanitize(pm.space_after, 0.0, 0.0, MAX_PARAGRAPH_SPACE);
        let list_indent = sanitize(pm.list_indent, 0.0, 0.0, MAX_LIST_INDENT);

        let base_row_h = if self.has_content && self.row_height > 0.0 {
            self.row_height
        } else {
            default_row_h
        };
        let para_w = (list_indent + self.width).min(POINT_TEXT_MAX_DIM);
        let para_h = space_before + (base_row_h * line_spacing) + space_after;

        *max_w = max_w.max(para_w);
        *total_h = (*total_h + para_h).min(POINT_TEXT_MAX_DIM);

        self.width = 0.0;
        self.row_height = 0.0;
        self.has_content = false;
    }
}

/// Measure unwrapped text content dimensions `(width, height)` in slide pixels.
///
/// Splits runs on `\n` into paragraphs, measures glyphs with `skrifa` against
/// the bundled Inter faces, and returns the content bounding size before outer
/// point-text padding or text box insets.
#[must_use]
pub fn measure_unwrapped_text(
    runs: &[MeasuredRun<'_>],
    paragraphs: &[ParagraphMetrics],
    default_size: f32,
) -> (f32, f32) {
    let default_size = sanitize(
        default_size,
        DEFAULT_FONT_SIZE,
        MIN_FONT_SIZE,
        MAX_FONT_SIZE,
    );
    let default_font = skrifa::FontRef::new(INTER_REGULAR).ok();
    let default_row_h = font_row_height(default_font.as_ref(), default_size);

    let mut max_para_w = 0.0_f32;
    let mut total_h = 0.0_f32;
    let mut para_idx = 0_usize;
    let mut accum = ParagraphAccum::new();
    let mut remaining_chars = MAX_CHARS;

    for run in runs.iter().take(MAX_RUNS) {
        if remaining_chars == 0 {
            break;
        }
        let base_size = sanitize(run.size, default_size, MIN_FONT_SIZE, MAX_FONT_SIZE);
        let script_scale = if run.script_scale.is_finite() && run.script_scale > 0.0 {
            run.script_scale.clamp(MIN_SCRIPT_SCALE, MAX_SCRIPT_SCALE)
        } else {
            1.0
        };
        let scaled_size = (base_size * script_scale).clamp(MIN_FONT_SIZE, MAX_FONT_SIZE);
        let letter_spacing = sanitize(
            run.letter_spacing,
            0.0,
            MIN_LETTER_SPACING,
            MAX_LETTER_SPACING,
        );
        let font = skrifa::FontRef::new(font_bytes_for_style(run.bold, run.italic)).ok();
        let run_row_h = font_row_height(font.as_ref(), scaled_size);

        let mut first = true;
        for segment in run.text.split('\n') {
            if !first {
                if remaining_chars == 0 {
                    break;
                }
                remaining_chars -= 1;
                accum.finish(
                    para_idx,
                    paragraphs,
                    default_row_h,
                    &mut max_para_w,
                    &mut total_h,
                );
                para_idx = para_idx.saturating_add(1);
            }
            first = false;

            if !segment.is_empty() {
                let (seg_w, count) = measure_segment(
                    segment,
                    font.as_ref(),
                    scaled_size,
                    letter_spacing,
                    &mut remaining_chars,
                );
                if count > 0 {
                    accum.width = (accum.width + seg_w).min(POINT_TEXT_MAX_DIM);
                    accum.row_height = accum.row_height.max(run_row_h);
                    accum.has_content = true;
                }
            }
            if remaining_chars == 0 {
                break;
            }
        }
    }

    accum.finish(
        para_idx,
        paragraphs,
        default_row_h,
        &mut max_para_w,
        &mut total_h,
    );

    let min_h = default_row_h.clamp(1.0, POINT_TEXT_MAX_DIM);
    (
        sanitize(max_para_w, 0.0, 0.0, POINT_TEXT_MAX_DIM),
        sanitize(total_h, min_h, 1.0, POINT_TEXT_MAX_DIM),
    )
}

/// Convert measured unwrapped content dimensions into a fitted point-text box size.
///
/// Adds `2.0 * POINT_TEXT_PAD_X` / `2.0 * POINT_TEXT_PAD_Y` and `insets`, rounds
/// up with `ceil()`, enforces `POINT_TEXT_MIN_W`, swaps `(w, h)` for vertical
/// text, and clamps both dimensions to `1.0..=POINT_TEXT_MAX_DIM`.
#[must_use]
pub fn point_text_box_size(
    content_w: f32,
    content_h: f32,
    insets: TextBoxInsets,
    vertical: bool,
) -> (f32, f32) {
    let cw = sanitize(content_w, 0.0, 0.0, POINT_TEXT_MAX_DIM);
    let ch = sanitize(content_h, 1.0, 0.0, POINT_TEXT_MAX_DIM);
    let left = sanitize(insets.left, 0.0, 0.0, POINT_TEXT_MAX_DIM);
    let right = sanitize(insets.right, 0.0, 0.0, POINT_TEXT_MAX_DIM);
    let top = sanitize(insets.top, 0.0, 0.0, POINT_TEXT_MAX_DIM);
    let bottom = sanitize(insets.bottom, 0.0, 0.0, POINT_TEXT_MAX_DIM);

    let w = (cw + left + right + 2.0 * POINT_TEXT_PAD_X)
        .ceil()
        .clamp(POINT_TEXT_MIN_W, POINT_TEXT_MAX_DIM);
    let h = (ch + top + bottom + 2.0 * POINT_TEXT_PAD_Y)
        .ceil()
        .clamp(1.0, POINT_TEXT_MAX_DIM);

    if vertical { (h, w) } else { (w, h) }
}

/// One measured character with its advance width and line height for wrapping.
struct WrappedChar {
    ch: char,
    advance: f32,
    row_height: f32,
}

/// Wrap one paragraph's measured characters to `avail_w` and return its content
/// height (sum of wrapped line heights times `line_spacing`, plus before/after
/// spacing), always at least one line tall.
fn wrap_paragraph_height(
    chars: &[WrappedChar],
    pm: ParagraphMetrics,
    default_row_h: f32,
    wrap_width: f32,
) -> f32 {
    let line_spacing = sanitize(pm.line_spacing, 1.0, MIN_LINE_SPACING, MAX_LINE_SPACING);
    let space_before = sanitize(pm.space_before, 0.0, 0.0, MAX_PARAGRAPH_SPACE);
    let space_after = sanitize(pm.space_after, 0.0, 0.0, MAX_PARAGRAPH_SPACE);
    let list_indent = sanitize(pm.list_indent, 0.0, 0.0, MAX_LIST_INDENT);
    let avail_w = (wrap_width - list_indent).max(1.0);

    if chars.is_empty() {
        return (space_before + default_row_h * line_spacing + space_after)
            .clamp(1.0, POINT_TEXT_MAX_DIM);
    }

    let mut total_lines_h = 0.0_f32;
    let mut line_w = 0.0_f32;
    let mut line_h = 0.0_f32;
    let mut line_has_content = false;
    let mut idx = 0_usize;

    while idx < chars.len() {
        // Group consecutive whitespace or consecutive non-whitespace (a word).
        let is_ws = chars[idx].ch.is_whitespace();
        let start = idx;
        let mut token_w = 0.0_f32;
        let mut token_h = 0.0_f32;
        while idx < chars.len() && chars[idx].ch.is_whitespace() == is_ws {
            token_w = (token_w + chars[idx].advance).min(POINT_TEXT_MAX_DIM);
            token_h = token_h.max(chars[idx].row_height);
            idx += 1;
        }

        if is_ws {
            // Trailing whitespace stays on the current line without forcing a
            // spurious blank wrap, matching egui/epaint's galley wrapping.
            line_w = (line_w + token_w).min(POINT_TEXT_MAX_DIM);
            line_h = line_h.max(token_h);
            line_has_content = true;
            continue;
        }

        if line_has_content && line_w + token_w > avail_w {
            let h = if line_h > 0.0 { line_h } else { default_row_h };
            total_lines_h = (total_lines_h + h * line_spacing).min(POINT_TEXT_MAX_DIM);
            line_w = 0.0;
            line_h = 0.0;
            line_has_content = false;
        }

        if token_w <= avail_w {
            line_w = (line_w + token_w).min(POINT_TEXT_MAX_DIM);
            line_h = line_h.max(token_h);
            line_has_content = true;
        } else {
            // Single word wider than `avail_w`: break by character.
            for wc in &chars[start..idx] {
                if line_has_content && line_w + wc.advance > avail_w {
                    let h = if line_h > 0.0 { line_h } else { default_row_h };
                    total_lines_h = (total_lines_h + h * line_spacing).min(POINT_TEXT_MAX_DIM);
                    line_w = 0.0;
                    line_h = 0.0;
                }
                line_w = (line_w + wc.advance).min(POINT_TEXT_MAX_DIM);
                line_h = line_h.max(wc.row_height);
                line_has_content = true;
            }
        }
    }

    let final_h = if line_has_content && line_h > 0.0 {
        line_h
    } else {
        default_row_h
    };
    total_lines_h = (total_lines_h + final_h * line_spacing).min(POINT_TEXT_MAX_DIM);
    (space_before + total_lines_h + space_after).clamp(1.0, POINT_TEXT_MAX_DIM)
}

/// Measure wrapped text content height in slide pixels for a fixed `wrap_width`.
///
/// Splits runs on `\n` into paragraphs, wraps words at `wrap_width` (breaking
/// overlong words by character) using `skrifa` against the bundled Inter faces,
/// and returns total content height before box padding or insets (never smaller
/// than one line, clamped to `1.0..=POINT_TEXT_MAX_DIM`).
#[must_use]
pub fn measure_wrapped_text_height(
    runs: &[MeasuredRun<'_>],
    paragraphs: &[ParagraphMetrics],
    default_size: f32,
    wrap_width: f32,
) -> f32 {
    let default_size = sanitize(
        default_size,
        DEFAULT_FONT_SIZE,
        MIN_FONT_SIZE,
        MAX_FONT_SIZE,
    );
    let wrap_width = sanitize(wrap_width, POINT_TEXT_MIN_W, 1.0, POINT_TEXT_MAX_DIM);
    let default_font = skrifa::FontRef::new(INTER_REGULAR).ok();
    let default_row_h = font_row_height(default_font.as_ref(), default_size);

    let mut total_h = 0.0_f32;
    let mut para_idx = 0_usize;
    let mut para_chars: Vec<WrappedChar> = Vec::new();
    let mut remaining_chars = MAX_CHARS;

    for run in runs.iter().take(MAX_RUNS) {
        if remaining_chars == 0 {
            break;
        }
        let base_size = sanitize(run.size, default_size, MIN_FONT_SIZE, MAX_FONT_SIZE);
        let script_scale = if run.script_scale.is_finite() && run.script_scale > 0.0 {
            run.script_scale.clamp(MIN_SCRIPT_SCALE, MAX_SCRIPT_SCALE)
        } else {
            1.0
        };
        let scaled_size = (base_size * script_scale).clamp(MIN_FONT_SIZE, MAX_FONT_SIZE);
        let letter_spacing = sanitize(
            run.letter_spacing,
            0.0,
            MIN_LETTER_SPACING,
            MAX_LETTER_SPACING,
        );
        let font = skrifa::FontRef::new(font_bytes_for_style(run.bold, run.italic)).ok();
        let run_row_h = font_row_height(font.as_ref(), scaled_size);
        let size = Size::new(scaled_size);
        let location = LocationRef::default();
        let charmap = font.as_ref().map(|f| f.charmap());
        let glyph_metrics = font.as_ref().map(|f| f.glyph_metrics(size, location));

        let mut first = true;
        for segment in run.text.split('\n') {
            if !first {
                if remaining_chars == 0 {
                    break;
                }
                remaining_chars -= 1;
                let pm = paragraphs.get(para_idx).copied().unwrap_or_default();
                let para_h = wrap_paragraph_height(&para_chars, pm, default_row_h, wrap_width);
                total_h = (total_h + para_h).min(POINT_TEXT_MAX_DIM);
                para_chars.clear();
                para_idx = para_idx.saturating_add(1);
            }
            first = false;

            for ch in segment.chars() {
                if remaining_chars == 0 {
                    break;
                }
                remaining_chars -= 1;
                if ch == '\r' {
                    continue;
                }
                let raw_adv = match (&charmap, &glyph_metrics) {
                    (Some(cm), Some(gm)) => match cm.map(ch) {
                        Some(gid) if gid != GlyphId::NOTDEF => gm
                            .advance_width(gid)
                            .filter(|v| v.is_finite() && *v >= 0.0)
                            .unwrap_or_else(|| fallback_advance(ch, scaled_size)),
                        _ => fallback_advance(ch, scaled_size),
                    },
                    _ => fallback_advance(ch, scaled_size),
                };
                let advance = (raw_adv + letter_spacing).clamp(0.0, POINT_TEXT_MAX_DIM);
                para_chars.push(WrappedChar {
                    ch,
                    advance,
                    row_height: run_row_h,
                });
            }
            if remaining_chars == 0 {
                break;
            }
        }
    }

    let pm = paragraphs.get(para_idx).copied().unwrap_or_default();
    let para_h = wrap_paragraph_height(&para_chars, pm, default_row_h, wrap_width);
    total_h = (total_h + para_h).min(POINT_TEXT_MAX_DIM);

    let min_h = default_row_h.clamp(1.0, POINT_TEXT_MAX_DIM);
    sanitize(total_h, min_h, min_h, POINT_TEXT_MAX_DIM)
}

/// Convert a measured wrapped content height into a fitted Grow text-box height.
///
/// Adds vertical insets (`top + bottom`) and `2.0 * POINT_TEXT_PAD_Y`, rounds up
/// with `ceil()`, and clamps to `1.0..=POINT_TEXT_MAX_DIM`.
#[must_use]
pub fn grow_text_box_height(content_h: f32, insets: TextBoxInsets) -> f32 {
    let ch = sanitize(content_h, 1.0, 1.0, POINT_TEXT_MAX_DIM);
    let top = sanitize(insets.top, 0.0, 0.0, POINT_TEXT_MAX_DIM);
    let bottom = sanitize(insets.bottom, 0.0, 0.0, POINT_TEXT_MAX_DIM);
    (ch + top + bottom + 2.0 * POINT_TEXT_PAD_Y)
        .ceil()
        .clamp(1.0, POINT_TEXT_MAX_DIM)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_fonts_draw_ui_symbols_and_reject_unbundled_scripts() {
        for ch in [
            '\u{2715}', '\u{2713}', '\u{2714}', '\u{2717}', '\u{2605}', '\u{2606}', '\u{2192}',
            '\u{2022}', 'a', ' ',
        ] {
            assert!(bundled_fonts_can_draw(ch), "{ch:?}");
        }
        assert!(!bundled_fonts_can_draw('\u{0928}'));
    }

    fn plain_run(text: &str, size: f32) -> MeasuredRun<'_> {
        MeasuredRun {
            text,
            size,
            bold: false,
            italic: false,
            letter_spacing: 0.0,
            script_scale: 1.0,
        }
    }

    #[test]
    fn empty_string_has_zero_width_and_positive_line_height() {
        let (w, h) = measure_unwrapped_text(&[plain_run("", 20.0)], &[], 20.0);
        assert_eq!(w, 0.0);
        assert!(
            (20.0..=40.0).contains(&h),
            "unexpected empty line height: {h}"
        );

        let (box_w, box_h) = point_text_box_size(w, h, TextBoxInsets::ZERO, false);
        assert_eq!(box_w, POINT_TEXT_MIN_W);
        assert!(box_h >= h.ceil());
    }

    #[test]
    fn single_line_scales_with_length_and_size() {
        let (w_short, h_20) = measure_unwrapped_text(&[plain_run("Next", 20.0)], &[], 20.0);
        let (w_long, _) = measure_unwrapped_text(&[plain_run("Submit Answer", 20.0)], &[], 20.0);
        let (w_big, h_40) = measure_unwrapped_text(&[plain_run("Next", 40.0)], &[], 40.0);

        assert!(w_short > 20.0 && w_short < 100.0, "w_short={w_short}");
        assert!(w_long > w_short * 2.0, "w_long={w_long}, w_short={w_short}");
        assert!(w_big > w_short * 1.8, "w_big={w_big}, w_short={w_short}");
        assert!(h_40 > h_20 * 1.8, "h_40={h_40}, h_20={h_20}");
    }

    #[test]
    fn multiline_takes_max_width_and_summed_height() {
        let (w1, h1) = measure_unwrapped_text(&[plain_run("Line 1", 20.0)], &[], 20.0);
        let (w2, _) = measure_unwrapped_text(&[plain_run("Longer second line", 20.0)], &[], 20.0);
        let (w_multi, h_multi) =
            measure_unwrapped_text(&[plain_run("Line 1\nLonger second line", 20.0)], &[], 20.0);

        assert!((w_multi - w2).abs() < 0.01, "w_multi={w_multi}, w2={w2}");
        assert!(w_multi > w1);
        assert!(
            (h_multi - 2.0 * h1).abs() < 0.01,
            "h_multi={h_multi}, 2*h1={}",
            2.0 * h1
        );
    }

    #[test]
    fn bold_is_wider_than_regular() {
        let regular = MeasuredRun {
            text: "Submit Answer",
            size: 24.0,
            bold: false,
            italic: false,
            letter_spacing: 0.0,
            script_scale: 1.0,
        };
        let bold = MeasuredRun {
            bold: true,
            ..regular
        };
        let (w_reg, _) = measure_unwrapped_text(&[regular], &[], 24.0);
        let (w_bold, _) = measure_unwrapped_text(&[bold], &[], 24.0);
        assert!(
            w_bold > w_reg,
            "expected bold ({w_bold}) wider than regular ({w_reg})"
        );
    }

    #[test]
    fn hostile_nan_inf_and_huge_inputs_never_panic_and_stay_clamped() {
        let hostile_runs = [
            MeasuredRun {
                text: "Hostile \r\n text 🎉",
                size: f32::NAN,
                bold: true,
                italic: true,
                letter_spacing: f32::INFINITY,
                script_scale: f32::NEG_INFINITY,
            },
            MeasuredRun {
                text: "More",
                size: f32::INFINITY,
                bold: false,
                italic: false,
                letter_spacing: f32::NEG_INFINITY,
                script_scale: 0.0,
            },
            MeasuredRun {
                text: "Negative",
                size: -9999.0,
                bold: false,
                italic: true,
                letter_spacing: f32::NAN,
                script_scale: 99999.0,
            },
        ];
        let hostile_paras = [
            ParagraphMetrics {
                line_spacing: f32::NAN,
                space_before: f32::INFINITY,
                space_after: -50.0,
                list_indent: f32::INFINITY,
            },
            ParagraphMetrics {
                line_spacing: 999.0,
                space_before: f32::NEG_INFINITY,
                space_after: f32::NAN,
                list_indent: -10.0,
            },
        ];
        let (w, h) = measure_unwrapped_text(&hostile_runs, &hostile_paras, f32::NAN);
        assert!(w.is_finite() && (0.0..=POINT_TEXT_MAX_DIM).contains(&w));
        assert!(h.is_finite() && (1.0..=POINT_TEXT_MAX_DIM).contains(&h));

        let (bw, bh) = point_text_box_size(
            f32::INFINITY,
            f32::NAN,
            TextBoxInsets {
                left: f32::NAN,
                top: f32::INFINITY,
                right: -100.0,
                bottom: f32::NEG_INFINITY,
            },
            false,
        );
        assert!(bw.is_finite() && (POINT_TEXT_MIN_W..=POINT_TEXT_MAX_DIM).contains(&bw));
        assert!(bh.is_finite() && (1.0..=POINT_TEXT_MAX_DIM).contains(&bh));

        let (vw, vh) = point_text_box_size(120.0, 24.0, TextBoxInsets::ZERO, true);
        assert_eq!((vw, vh), (28.0, 128.0));
    }

    #[test]
    fn paragraph_metrics_apply_spacing_and_list_indent() {
        let run = plain_run("Item", 20.0);
        let (base_w, base_h) = measure_unwrapped_text(&[run], &[], 20.0);
        let pm = ParagraphMetrics {
            line_spacing: 1.5,
            space_before: 6.0,
            space_after: 4.0,
            list_indent: 24.0,
        };
        let (w, h) = measure_unwrapped_text(&[run], &[pm], 20.0);
        assert!((w - (base_w + 24.0)).abs() < 0.01);
        assert!((h - (base_h * 1.5 + 10.0)).abs() < 0.01);
    }

    #[test]
    fn hostile_newline_and_cr_flood_respects_char_cap() {
        let many_newlines = "\n".repeat(MAX_CHARS + 50_000);
        let (w_nl, h_nl) = measure_unwrapped_text(&[plain_run(&many_newlines, 20.0)], &[], 20.0);
        assert_eq!(w_nl, 0.0);
        assert_eq!(h_nl, POINT_TEXT_MAX_DIM);

        // Trailing text after a flood of \r beyond MAX_CHARS must be capped and not measured.
        let mut cr_flood = "\r".repeat(MAX_CHARS + 10);
        cr_flood.push_str("Should not be reached");
        let (w_cr, h_cr) = measure_unwrapped_text(&[plain_run(&cr_flood, 20.0)], &[], 20.0);
        assert_eq!(w_cr, 0.0);
        assert!(h_cr.is_finite() && (1.0..=POINT_TEXT_MAX_DIM).contains(&h_cr));

        // Trailing text after a flood of \n beyond MAX_CHARS must also be capped.
        let mut nl_flood = "\n".repeat(MAX_CHARS + 10);
        nl_flood.push_str("Should not be reached");
        let (w_nl_tail, _) = measure_unwrapped_text(&[plain_run(&nl_flood, 20.0)], &[], 20.0);
        assert_eq!(w_nl_tail, 0.0);
    }

    #[test]
    fn wrapped_height_grows_as_width_narrows_and_never_below_one_line() {
        let (_, one_line_h) = measure_unwrapped_text(&[plain_run("A", 20.0)], &[], 20.0);
        let empty_h = measure_wrapped_text_height(&[plain_run("", 20.0)], &[], 20.0, 200.0);
        assert!(
            (empty_h - one_line_h).abs() < 0.01,
            "empty wrapped text must be at least one line: {empty_h} vs {one_line_h}"
        );

        let sentence = "The quick brown fox jumps over the lazy dog again and again";
        let h_wide = measure_wrapped_text_height(&[plain_run(sentence, 20.0)], &[], 20.0, 1000.0);
        let h_mid = measure_wrapped_text_height(&[plain_run(sentence, 20.0)], &[], 20.0, 200.0);
        let h_narrow = measure_wrapped_text_height(&[plain_run(sentence, 20.0)], &[], 20.0, 80.0);

        assert!(
            (h_wide - one_line_h).abs() < 0.01,
            "wide box should fit on 1 line: {h_wide} vs {one_line_h}"
        );
        assert!(
            h_mid >= one_line_h * 2.5,
            "medium box should wrap onto >=3 lines: {h_mid} vs {one_line_h}"
        );
        assert!(
            h_narrow > h_mid * 1.5,
            "narrow box should wrap onto more lines: {h_narrow} vs {h_mid}"
        );

        let box_h = grow_text_box_height(
            h_mid,
            TextBoxInsets {
                left: 8.0,
                top: 6.0,
                right: 8.0,
                bottom: 4.0,
            },
        );
        assert_eq!(box_h, (h_mid + 10.0 + 2.0 * POINT_TEXT_PAD_Y).ceil());
    }

    #[test]
    fn wrapped_height_hostile_inputs_clamped_without_panic() {
        let hostile = [MeasuredRun {
            text: &"Supercalifragilisticexpialidocious ".repeat(4000),
            size: f32::NAN,
            bold: true,
            italic: false,
            letter_spacing: f32::INFINITY,
            script_scale: -10.0,
        }];
        let h = measure_wrapped_text_height(&hostile, &[], f32::NEG_INFINITY, f32::NAN);
        assert!(h.is_finite() && (1.0..=POINT_TEXT_MAX_DIM).contains(&h));

        let bh = grow_text_box_height(
            f32::INFINITY,
            TextBoxInsets {
                left: f32::NAN,
                top: f32::INFINITY,
                right: -5.0,
                bottom: f32::NEG_INFINITY,
            },
        );
        assert!(bh.is_finite() && (1.0..=POINT_TEXT_MAX_DIM).contains(&bh));
    }
}
