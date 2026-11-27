//! Paragraph layout for slide text.
//!
//! One function lays out bullets, numbering, indents, line spacing, spacing
//! before and after, links and shrink-to-fit text. The renderer, the hit
//! regions and the tests all use it, so a bullet in the canvas is the same
//! bullet in preview and in the published player.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use coursecraft_course::{Align, Autofit, Link, ListKind, NumberScheme, Paragraph, VAlign};
use egui::epaint::text::{LayoutJob, TextFormat, TextWrapping};
use egui::{Color32, Context, Pos2, Rect, Stroke, pos2};

use crate::compose::ResolvedTextRun;
use crate::compose::{DrawItem, ResolvedLinkRect, Visual, validate_frame};

/// Default maximum number of cached text layouts per [`egui::Context`].
pub const DEFAULT_TEXT_LAYOUT_CACHE_MAX: usize = 256;

const TEXT_LAYOUT_CACHE_ID: &str = "coursecraft-text-layout-cache";

fn f32_bits(v: f32) -> u32 {
    if v.is_nan() {
        f32::NAN.to_bits()
    } else if v == 0.0 {
        0.0_f32.to_bits()
    } else {
        v.to_bits()
    }
}

fn quantize_dim_4(v: f32) -> i32 {
    if !v.is_finite() {
        return 0;
    }
    (v * 4.0).clamp(0.0, i32::MAX as f32).round() as i32
}

fn align_tag(align: Align) -> u8 {
    match align {
        Align::Left => 0,
        Align::Center => 1,
        Align::Right => 2,
        Align::Justify => 3,
    }
}

fn valign_tag(valign: VAlign) -> u8 {
    match valign {
        VAlign::Top => 0,
        VAlign::Middle => 1,
        VAlign::Bottom => 2,
    }
}

fn autofit_tag(autofit: Autofit) -> u8 {
    match autofit {
        Autofit::None | Autofit::Grow => 0,
        Autofit::Shrink => 1,
    }
}

fn baseline_tag(baseline: coursecraft_course::Baseline) -> u8 {
    match baseline {
        coursecraft_course::Baseline::Normal => 0,
        coursecraft_course::Baseline::Superscript => 1,
        coursecraft_course::Baseline::Subscript => 2,
    }
}

fn list_kind_tag(list: ListKind) -> u8 {
    match list {
        ListKind::None => 0,
        ListKind::Bullet => 1,
        ListKind::Number => 2,
    }
}

fn number_scheme_tag(scheme: NumberScheme) -> u8 {
    match scheme {
        NumberScheme::ArabicPeriod => 0,
        NumberScheme::ArabicParenR => 1,
        NumberScheme::AlphaLcPeriod => 2,
        NumberScheme::AlphaLcParenR => 3,
        NumberScheme::AlphaUcPeriod => 4,
        NumberScheme::RomanLcPeriod => 5,
        NumberScheme::RomanUcPeriod => 6,
    }
}

/// Independent dual 64-bit FNV-1a hash (128 bits total) for zero-allocation lookups.
struct DualHash {
    h1: u64,
    h2: u64,
}

impl DualHash {
    fn new() -> Self {
        Self {
            h1: 0xcbf2_9ce4_8422_2325,
            h2: 0x8422_2325_cbf2_9ce4,
        }
    }

    fn write_bytes(&mut self, bytes: &[u8]) {
        const P1: u64 = 0x0000_0100_0000_01b3;
        const P2: u64 = 0x9e37_79b9_7f4a_7c15;
        for &b in bytes {
            self.h1 ^= u64::from(b);
            self.h1 = self.h1.wrapping_mul(P1);
            self.h2 = self.h2.wrapping_mul(P2) ^ u64::from(b).wrapping_add(0x51);
        }
    }

    fn write_u8(&mut self, v: u8) {
        self.write_bytes(&[v]);
    }

    fn write_u32(&mut self, v: u32) {
        self.write_bytes(&v.to_le_bytes());
    }

    fn write_i32(&mut self, v: i32) {
        self.write_bytes(&v.to_le_bytes());
    }

    fn write_u64(&mut self, v: u64) {
        self.write_bytes(&v.to_le_bytes());
    }

    fn write_str(&mut self, s: &str) {
        self.write_u64(s.len() as u64);
        self.write_bytes(s.as_bytes());
    }

    fn finish(self) -> (u64, u64) {
        (self.h1, self.h2)
    }
}

#[derive(Clone, PartialEq, Eq)]
enum LinkKey {
    Url(String),
    Slide(String),
}

impl LinkKey {
    fn from_link(link: &Link) -> Self {
        match link {
            Link::Url { url } => Self::Url(url.clone()),
            Link::Slide { slide } => Self::Slide(slide.clone()),
        }
    }

    fn matches(&self, link: &Link) -> bool {
        match (self, link) {
            (Self::Url(a), Link::Url { url: b }) => a == b,
            (Self::Slide(a), Link::Slide { slide: b }) => a == b,
            _ => false,
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
struct RunKey {
    text: String,
    size_bits: u32,
    color: [u8; 4],
    bold: bool,
    italic: bool,
    underline: bool,
    strikethrough: bool,
    baseline: u8,
    letter_spacing_bits: u32,
    font: Option<String>,
    link: Option<LinkKey>,
    highlight: Option<[u8; 4]>,
    shadow: bool,
}

impl RunKey {
    fn from_run(run: &ResolvedTextRun) -> Self {
        Self {
            text: run.text.clone(),
            size_bits: f32_bits(run.size),
            color: run.color.to_array(),
            bold: run.bold,
            italic: run.italic,
            underline: run.underline,
            strikethrough: run.strikethrough,
            baseline: baseline_tag(run.baseline),
            letter_spacing_bits: f32_bits(run.letter_spacing),
            font: run.font.clone(),
            link: run.link.as_ref().map(LinkKey::from_link),
            highlight: run.highlight.map(|c| c.to_array()),
            shadow: run.shadow,
        }
    }

    fn matches(&self, run: &ResolvedTextRun) -> bool {
        self.size_bits == f32_bits(run.size)
            && self.color == run.color.to_array()
            && self.bold == run.bold
            && self.italic == run.italic
            && self.underline == run.underline
            && self.strikethrough == run.strikethrough
            && self.baseline == baseline_tag(run.baseline)
            && self.letter_spacing_bits == f32_bits(run.letter_spacing)
            && self.font.as_deref() == run.font.as_deref()
            && self.highlight == run.highlight.map(|c| c.to_array())
            && self.shadow == run.shadow
            && match (&self.link, &run.link) {
                (None, None) => true,
                (Some(a), Some(b)) => a.matches(b),
                _ => false,
            }
            && self.text == run.text
    }
}

#[derive(Clone, PartialEq, Eq)]
struct ParagraphKey {
    list: u8,
    level: u8,
    line_spacing_bits: u32,
    space_before_bits: u32,
    space_after_bits: u32,
    bullet: Option<String>,
    bullet_color: Option<String>,
    bullet_size_bits: Option<u32>,
    number_scheme: Option<u8>,
    start_at: Option<u32>,
}

impl ParagraphKey {
    fn from_paragraph(p: &Paragraph) -> Self {
        Self {
            list: list_kind_tag(p.list),
            level: p.level,
            line_spacing_bits: f32_bits(p.line_spacing),
            space_before_bits: f32_bits(p.space_before),
            space_after_bits: f32_bits(p.space_after),
            bullet: p.bullet.clone(),
            bullet_color: p.bullet_color.as_ref().map(|c| c.0.clone()),
            bullet_size_bits: p.bullet_size.map(f32_bits),
            number_scheme: p.number_scheme.map(number_scheme_tag),
            start_at: p.start_at,
        }
    }

    fn matches(&self, p: &Paragraph) -> bool {
        self.list == list_kind_tag(p.list)
            && self.level == p.level
            && self.line_spacing_bits == f32_bits(p.line_spacing)
            && self.space_before_bits == f32_bits(p.space_before)
            && self.space_after_bits == f32_bits(p.space_after)
            && self.bullet.as_deref() == p.bullet.as_deref()
            && self.bullet_color.as_deref() == p.bullet_color.as_ref().map(|c| c.0.as_str())
            && self.bullet_size_bits == p.bullet_size.map(f32_bits)
            && self.number_scheme == p.number_scheme.map(number_scheme_tag)
            && self.start_at == p.start_at
    }
}

#[derive(Clone, PartialEq, Eq)]
struct TextLayoutKey {
    font_epoch: u64,
    ppp_bits: u32,
    width_q: i32,
    height_q: i32,
    scale_bits: u32,
    align: u8,
    valign: u8,
    autofit: u8,
    wrap: bool,
    justify_origin_x_bits: u32,
    runs: Vec<RunKey>,
    paragraphs: Vec<ParagraphKey>,
}

struct BorrowedTextKey<'a> {
    font_epoch: u64,
    ppp_bits: u32,
    width_q: i32,
    height_q: i32,
    scale_bits: u32,
    align: u8,
    valign: u8,
    autofit: u8,
    wrap: bool,
    justify_origin_x_bits: u32,
    runs: &'a [ResolvedTextRun],
    paragraphs: &'a [Paragraph],
}

impl BorrowedTextKey<'_> {
    fn hash128(&self) -> (u64, u64) {
        let mut h = DualHash::new();
        h.write_u64(self.font_epoch);
        h.write_u32(self.ppp_bits);
        h.write_i32(self.width_q);
        h.write_i32(self.height_q);
        h.write_u32(self.scale_bits);
        h.write_u8(self.align);
        h.write_u8(self.valign);
        h.write_u8(self.autofit);
        h.write_u8(u8::from(self.wrap));
        h.write_u32(self.justify_origin_x_bits);
        h.write_u64(self.runs.len() as u64);
        for run in self.runs {
            h.write_str(&run.text);
            h.write_u32(f32_bits(run.size));
            h.write_bytes(&run.color.to_array());
            h.write_u8(
                u8::from(run.bold)
                    | (u8::from(run.italic) << 1)
                    | (u8::from(run.underline) << 2)
                    | (u8::from(run.strikethrough) << 3),
            );
            h.write_u8(baseline_tag(run.baseline));
            h.write_u32(f32_bits(run.letter_spacing));
            match run.highlight {
                Some(c) => {
                    h.write_u8(1);
                    h.write_bytes(&c.to_array());
                }
                None => h.write_u8(0),
            }
            h.write_u8(u8::from(run.shadow));
            if let Some(font) = &run.font {
                h.write_u8(1);
                h.write_str(font);
            } else {
                h.write_u8(0);
            }
            match &run.link {
                None => h.write_u8(0),
                Some(Link::Url { url }) => {
                    h.write_u8(1);
                    h.write_str(url);
                }
                Some(Link::Slide { slide }) => {
                    h.write_u8(2);
                    h.write_str(slide);
                }
            }
        }
        h.write_u64(self.paragraphs.len() as u64);
        for p in self.paragraphs {
            h.write_u8(list_kind_tag(p.list));
            h.write_u8(p.level);
            h.write_u32(f32_bits(p.line_spacing));
            h.write_u32(f32_bits(p.space_before));
            h.write_u32(f32_bits(p.space_after));
            if let Some(b) = &p.bullet {
                h.write_u8(1);
                h.write_str(b);
            } else {
                h.write_u8(0);
            }
            if let Some(c) = &p.bullet_color {
                h.write_u8(1);
                h.write_str(&c.0);
            } else {
                h.write_u8(0);
            }
            if let Some(sz) = p.bullet_size {
                h.write_u8(1);
                h.write_u32(f32_bits(sz));
            } else {
                h.write_u8(0);
            }
            if let Some(scheme) = p.number_scheme {
                h.write_u8(1);
                h.write_u8(number_scheme_tag(scheme));
            } else {
                h.write_u8(0);
            }
            if let Some(start) = p.start_at {
                h.write_u8(1);
                h.write_u32(start);
            } else {
                h.write_u8(0);
            }
        }
        h.finish()
    }

    fn to_owned_key(&self) -> TextLayoutKey {
        TextLayoutKey {
            font_epoch: self.font_epoch,
            ppp_bits: self.ppp_bits,
            width_q: self.width_q,
            height_q: self.height_q,
            scale_bits: self.scale_bits,
            align: self.align,
            valign: self.valign,
            autofit: self.autofit,
            wrap: self.wrap,
            justify_origin_x_bits: self.justify_origin_x_bits,
            runs: self.runs.iter().map(RunKey::from_run).collect(),
            paragraphs: self
                .paragraphs
                .iter()
                .map(ParagraphKey::from_paragraph)
                .collect(),
        }
    }
}

impl TextLayoutKey {
    fn matches(&self, other: &BorrowedTextKey<'_>) -> bool {
        self.font_epoch == other.font_epoch
            && self.ppp_bits == other.ppp_bits
            && self.width_q == other.width_q
            && self.height_q == other.height_q
            && self.scale_bits == other.scale_bits
            && self.align == other.align
            && self.valign == other.valign
            && self.autofit == other.autofit
            && self.wrap == other.wrap
            && self.justify_origin_x_bits == other.justify_origin_x_bits
            && self.runs.len() == other.runs.len()
            && self.paragraphs.len() == other.paragraphs.len()
            && self
                .runs
                .iter()
                .zip(other.runs.iter())
                .all(|(k, r)| k.matches(r))
            && self
                .paragraphs
                .iter()
                .zip(other.paragraphs.iter())
                .all(|(k, p)| k.matches(p))
    }
}

struct TextCacheEntry {
    key: TextLayoutKey,
    laid: Arc<LaidText>,
    last_used: u64,
}

struct TextLayoutCache {
    buckets: HashMap<(u64, u64), Vec<TextCacheEntry>>,
    total_entries: usize,
    max_entries: usize,
    tick: u64,
}

impl TextLayoutCache {
    fn new(max_entries: usize) -> Self {
        Self {
            buckets: HashMap::new(),
            total_entries: 0,
            max_entries,
            tick: 0,
        }
    }

    fn get(&mut self, hash: (u64, u64), key: &BorrowedTextKey<'_>) -> Option<Arc<LaidText>> {
        if self.max_entries == 0 {
            return None;
        }
        let bucket = self.buckets.get_mut(&hash)?;
        for entry in bucket.iter_mut() {
            if entry.key.matches(key) {
                self.tick = self.tick.saturating_add(1);
                entry.last_used = self.tick;
                return Some(Arc::clone(&entry.laid));
            }
        }
        None
    }

    fn insert(&mut self, hash: (u64, u64), key: TextLayoutKey, laid: Arc<LaidText>) {
        if self.max_entries == 0 {
            return;
        }
        self.tick = self.tick.saturating_add(1);
        let tick = self.tick;
        let bucket = self.buckets.entry(hash).or_default();
        if let Some(existing) = bucket.iter_mut().find(|e| e.key == key) {
            existing.laid = laid;
            existing.last_used = tick;
            return;
        }
        bucket.push(TextCacheEntry {
            key,
            laid,
            last_used: tick,
        });
        self.total_entries = self.total_entries.saturating_add(1);
        self.evict_excess();
    }

    fn set_limit(&mut self, max_entries: usize) {
        self.max_entries = max_entries;
        if max_entries == 0 {
            self.clear();
        } else {
            self.evict_excess();
        }
    }

    fn clear(&mut self) {
        self.buckets.clear();
        self.total_entries = 0;
    }

    fn evict_excess(&mut self) {
        while self.total_entries > self.max_entries {
            let mut oldest: Option<(u64, (u64, u64), usize)> = None;
            for (&hash, bucket) in &self.buckets {
                for (idx, entry) in bucket.iter().enumerate() {
                    if oldest.is_none_or(|(best_tick, _, _)| entry.last_used < best_tick) {
                        oldest = Some((entry.last_used, hash, idx));
                    }
                }
            }
            let Some((_, hash, idx)) = oldest else {
                break;
            };
            let mut remove_bucket = false;
            if let Some(bucket) = self.buckets.get_mut(&hash) {
                if idx < bucket.len() {
                    bucket.swap_remove(idx);
                    self.total_entries = self.total_entries.saturating_sub(1);
                }
                remove_bucket = bucket.is_empty();
            }
            if remove_bucket {
                self.buckets.remove(&hash);
            }
        }
    }
}

fn text_layout_cache(ctx: &Context) -> Arc<Mutex<TextLayoutCache>> {
    let id = egui::Id::unique(TEXT_LAYOUT_CACHE_ID);
    if let Some(existing) = ctx.data(|d| d.get_temp::<Arc<Mutex<TextLayoutCache>>>(id)) {
        return existing;
    }
    let created = Arc::new(Mutex::new(TextLayoutCache::new(
        DEFAULT_TEXT_LAYOUT_CACHE_MAX,
    )));
    ctx.data_mut(|d| {
        if let Some(existing) = d.get_temp::<Arc<Mutex<TextLayoutCache>>>(id) {
            return existing;
        }
        d.insert_temp(id, Arc::clone(&created));
        created
    })
}

/// Set the maximum number of entries retained in the per-context text layout cache.
///
/// Passing `0` disables caching and clears existing entries.
pub fn set_text_layout_cache_limit(ctx: &Context, max_entries: usize) {
    let cache = text_layout_cache(ctx);
    let mut guard = match cache.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    guard.set_limit(max_entries);
}

/// Clear all entries from the per-context text layout cache.
pub fn clear_text_layout_cache(ctx: &Context) {
    let cache = text_layout_cache(ctx);
    let mut guard = match cache.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    guard.clear();
}

#[cfg(test)]
pub(crate) fn text_layout_cache_limit(ctx: &Context) -> usize {
    let cache = text_layout_cache(ctx);
    let guard = match cache.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    guard.max_entries
}

fn translate_laid_text(base: &LaidText, offset: egui::Vec2) -> LaidText {
    if offset == egui::Vec2::ZERO {
        return base.clone();
    }
    let paragraphs = base
        .paragraphs
        .iter()
        .map(|p| LaidParagraph {
            galley: Arc::clone(&p.galley),
            pos: p.pos + offset,
            marker: p.marker.as_ref().map(|m| LaidMarker {
                galley: Arc::clone(&m.galley),
                pos: m.pos + offset,
                vector: m.vector,
            }),
            indent: p.indent,
            height: p.height,
            underlines: p.underlines.clone(),
            highlights: p.highlights.clone(),
            shadow: p.shadow.clone(),
        })
        .collect();
    let links = base
        .links
        .iter()
        .map(|l| LaidLink {
            link: l.link.clone(),
            text: l.text.clone(),
            rect: l.rect.translate(offset),
        })
        .collect();
    LaidText {
        paragraphs,
        links,
        height: base.height,
        width: base.width,
        shrink: base.shrink,
    }
}

/// Smallest scale shrink-to-fit may use.
pub const AUTOFIT_MIN_SCALE: f32 = 0.4;
/// Indent added for each paragraph level, in slide pixels before scaling.
pub const INDENT_STEP: f32 = 24.0;
/// Default marker colour when a paragraph has no run.
const FALLBACK_COLOR: Color32 = Color32::from_rgb(0x1F, 0x1F, 0x1F);
const FALLBACK_SIZE: f32 = 20.0;

/// A vector-drawn bullet when the bundled font lacks the symbol glyph.
#[derive(Debug, Clone, Copy)]
pub struct VectorBullet {
    pub ch: char,
    pub size: f32,
    pub color: Color32,
}

/// One laid-out marker (bullet or number).
#[derive(Debug, Clone)]
pub struct LaidMarker {
    pub galley: Arc<egui::Galley>,
    /// Screen-space top-left passed to `Painter::galley`. For a right-aligned
    /// marker this is its right edge, as egui expects.
    pub pos: Pos2,
    /// When `Some`, draw this vector symbol instead of `galley`.
    pub vector: Option<VectorBullet>,
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
    /// Highlighted character ranges of this paragraph and their colours. Drawn by
    /// [`highlight_rects`] behind the glyphs, so they follow justified and wrapped lines.
    pub highlights: Vec<(usize, usize, Color32)>,
    /// The same text drawn in shadow colour, laid out like `galley`, and the offset to
    /// draw it at. `None` when no run of the paragraph has a shadow.
    pub shadow: Option<(Arc<egui::Galley>, egui::Vec2)>,
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
    layout_text_with_wrap(
        ctx, rect, scale, runs, paragraphs, align, valign, autofit, true,
    )
}

/// Lay out `runs` inside `rect`, with explicit `wrap` control.
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn layout_text_with_wrap(
    ctx: &Context,
    rect: Rect,
    scale: f32,
    runs: &[ResolvedTextRun],
    paragraphs: &[Paragraph],
    align: Align,
    valign: VAlign,
    autofit: Autofit,
    wrap: bool,
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

    let offset = rect.min.to_vec2();
    let local_rect = Rect::from_min_size(pos2(0.0, 0.0), rect.size());
    let justify_x_offset = if align == Align::Justify && wrap {
        rect.min.x
    } else {
        0.0
    };
    let pending_before = crate::font::fonts_pending_apply(ctx);
    let epoch = crate::font::font_epoch(ctx);
    let borrowed = BorrowedTextKey {
        font_epoch: epoch,
        ppp_bits: f32_bits(ctx.pixels_per_point()),
        width_q: quantize_dim_4(local_rect.width()),
        height_q: quantize_dim_4(local_rect.height()),
        scale_bits: f32_bits(scale),
        align: align_tag(align),
        valign: valign_tag(valign),
        autofit: autofit_tag(autofit),
        wrap,
        justify_origin_x_bits: f32_bits(justify_x_offset),
        runs,
        paragraphs,
    };
    let hash = borrowed.hash128();
    let cache = text_layout_cache(ctx);
    if !pending_before {
        let mut guard = match cache.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Some(hit) = guard.get(hash, &borrowed) {
            return translate_laid_text(&hit, offset);
        }
    }

    #[cfg(debug_assertions)]
    crate::media::test_counts::note_text_layout_miss();

    let computed = compute_layout_at_origin(
        ctx,
        local_rect,
        scale,
        runs,
        paragraphs,
        align,
        valign,
        autofit,
        wrap,
        justify_x_offset,
    );
    let cached = Arc::new(computed);
    // Do not cache layouts computed while a `ctx.set_fonts` update is still
    // pending for the next pass (e.g. `install_course_fonts` called inside a
    // frame or lazy system-font installation during layout); otherwise the
    // stale fallback layout would be served under the new `font_epoch`.
    if !pending_before && !crate::font::fonts_pending_apply(ctx) {
        let owned_key = borrowed.to_owned_key();
        let mut guard = match cache.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard.insert(hash, owned_key, Arc::clone(&cached));
    }
    translate_laid_text(&cached, offset)
}

#[allow(clippy::too_many_arguments)]
fn compute_layout_at_origin(
    ctx: &Context,
    rect: Rect,
    scale: f32,
    runs: &[ResolvedTextRun],
    paragraphs: &[Paragraph],
    align: Align,
    valign: VAlign,
    autofit: Autofit,
    wrap: bool,
    justify_x_offset: f32,
) -> LaidText {
    let full = layout_at(
        ctx,
        rect,
        scale,
        runs,
        paragraphs,
        align,
        valign,
        1.0,
        wrap,
        justify_x_offset,
    );
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
        wrap,
        justify_x_offset,
    );
    if !floor.fits(rect) {
        return floor;
    }

    // Largest scale in the floor..=1 range that fits.
    let mut low = AUTOFIT_MIN_SCALE;
    let mut high = 1.0;
    for _ in 0..20 {
        let mid = (low + high) * 0.5;
        let candidate = layout_at(
            ctx,
            rect,
            scale,
            runs,
            paragraphs,
            align,
            valign,
            mid,
            wrap,
            justify_x_offset,
        );
        if candidate.fits(rect) {
            low = mid;
        } else {
            high = mid;
        }
    }
    layout_at(
        ctx,
        rect,
        scale,
        runs,
        paragraphs,
        align,
        valign,
        low,
        wrap,
        justify_x_offset,
    )
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
    let mut last_style: Option<ResolvedTextRun> = runs.first().cloned();
    for paragraph in &mut paragraphs {
        if let Some(last) = paragraph.last() {
            last_style = Some(last.clone());
        } else if let Some(template) = &last_style {
            paragraph.push(ResolvedTextRun {
                text: " ".to_owned(),
                size: template.size,
                color: template.color,
                bold: template.bold,
                italic: template.italic,
                underline: false,
                font: template.font.clone(),
                link: None,
                strikethrough: false,
                baseline: template.baseline,
                letter_spacing: template.letter_spacing,
                highlight: None,
                shadow: false,
            });
        } else {
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
                highlight: None,
                shadow: false,
            });
        }
    }
    paragraphs
}

/// Superscript and subscript text draws at this fraction of its size.
pub const SCRIPT_SCALE: f32 = 0.65;

#[derive(Clone, Copy, PartialEq, Eq)]
struct LevelState {
    count: u32,
    scheme: NumberScheme,
    start_at: Option<u32>,
}

impl Default for LevelState {
    fn default() -> Self {
        Self {
            count: 0,
            scheme: NumberScheme::ArabicPeriod,
            start_at: None,
        }
    }
}

fn default_scheme_for_level(level: usize) -> NumberScheme {
    match level % 3 {
        0 => NumberScheme::ArabicPeriod,
        1 => NumberScheme::AlphaLcPeriod,
        _ => NumberScheme::RomanLcPeriod,
    }
}

fn marker_for(
    levels: &mut [LevelState; 5],
    previous_list: &mut Option<ListKind>,
    style: &Paragraph,
    level: u8,
) -> Option<String> {
    let level_idx = usize::from(level.min(4));
    match style.list {
        ListKind::None => {
            *levels = [LevelState::default(); 5];
            *previous_list = Some(ListKind::None);
            None
        }
        ListKind::Bullet => {
            *levels = [LevelState::default(); 5];
            *previous_list = Some(ListKind::Bullet);
            Some(if let Some(custom) = &style.bullet {
                custom.clone()
            } else {
                match level_idx % 3 {
                    0 => "•",
                    1 => "◦",
                    _ => "▪",
                }
                .to_owned()
            })
        }
        ListKind::Number => {
            let scheme = style
                .number_scheme
                .unwrap_or_else(|| default_scheme_for_level(level_idx));
            let in_number_list = *previous_list == Some(ListKind::Number);
            let slot = levels.get_mut(level_idx)?;
            let same_list = in_number_list
                && slot.count > 0
                && slot.scheme == scheme
                && (style.start_at.is_none() || slot.start_at == style.start_at);
            if same_list {
                slot.count = slot.count.saturating_add(1);
            } else {
                slot.count = style.start_at.unwrap_or(1).max(1);
                slot.scheme = scheme;
                slot.start_at = style.start_at;
            }
            let number = slot.count;
            for deeper in levels.iter_mut().skip(level_idx.saturating_add(1)) {
                *deeper = LevelState::default();
            }
            *previous_list = Some(ListKind::Number);
            Some(format_number_scheme(number, scheme))
        }
    }
}

/// Largest number written as a Roman numeral.
const ROMAN_MAX: u32 = 3999;

fn format_number_scheme(number: u32, scheme: NumberScheme) -> String {
    let value = number.max(1);
    match scheme {
        NumberScheme::ArabicPeriod => format!("{value}."),
        NumberScheme::ArabicParenR => format!("{value})"),
        NumberScheme::AlphaLcPeriod => format!("{}.", alpha(value)),
        NumberScheme::AlphaLcParenR => format!("{})", alpha(value)),
        NumberScheme::AlphaUcPeriod => format!("{}.", alpha(value).to_ascii_uppercase()),
        // Roman numerals stop at 3999; larger values fall back to digits so a
        // hostile `start_at` cannot build a megabyte of `m` (N77).
        NumberScheme::RomanLcPeriod | NumberScheme::RomanUcPeriod if value > ROMAN_MAX => {
            format!("{value}.")
        }
        NumberScheme::RomanLcPeriod => format!("{}.", roman(value)),
        NumberScheme::RomanUcPeriod => format!("{}.", roman(value).to_ascii_uppercase()),
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
        Align::Left | Align::Justify => egui::Align::LEFT,
        Align::Center => egui::Align::Center,
        Align::Right => egui::Align::RIGHT,
    }
}

/// Spread extra horizontal space on wrapped lines across inter-word spaces
/// so each non-final line fills `target_width`, snapping each word's start
/// to whole physical pixels for crisp rendering.
///
/// Updates both `row.glyphs` (for hit testing, cursor placement, underlines and
/// links) and `row.visuals.mesh` (which egui actually renders), plus the row
/// and galley bounding boxes.
pub fn justify_galley(galley: &mut Arc<egui::Galley>, target_width: f32) {
    justify_galley_snapped(galley, target_width, 1.0, 0.0);
}

/// Like [`justify_galley`], but snaps each word's screen x
/// (`origin_x + row.pos.x + glyph.pos.x`) to `1.0 / pixels_per_point` so
/// glyphs land on whole physical pixels at any zoom or DPI scale.
pub fn justify_galley_snapped(
    galley: &mut Arc<egui::Galley>,
    target_width: f32,
    pixels_per_point: f32,
    origin_x: f32,
) {
    if !target_width.is_finite() || target_width <= 0.0 {
        return;
    }
    let ppp = if pixels_per_point.is_finite() && pixels_per_point > 0.0 {
        pixels_per_point
    } else {
        1.0
    };
    let origin_x = if origin_x.is_finite() { origin_x } else { 0.0 };

    let g = Arc::make_mut(galley);
    let row_count = g.rows.len();
    for (row_idx, placed_row) in g.rows.iter_mut().enumerate() {
        let is_last = row_idx + 1 == row_count;
        if is_last || placed_row.ends_with_newline {
            continue;
        }
        let glyphs = &placed_row.glyphs;
        let Some(first_non_ws) = glyphs.iter().position(|glyph| !glyph.chr.is_whitespace()) else {
            continue;
        };
        let Some(last_non_ws) = glyphs.iter().rposition(|glyph| !glyph.chr.is_whitespace()) else {
            continue;
        };
        let effective_end = last_non_ws + 1;
        let content_left_orig = glyphs[first_non_ws..effective_end]
            .iter()
            .map(|glyph| glyph.pos.x)
            .fold(f32::INFINITY, f32::min);
        let content_right = glyphs[..effective_end]
            .iter()
            .map(|glyph| glyph.pos.x + glyph.advance_width)
            .fold(0.0f32, f32::max);
        let extra = target_width - content_right;
        if extra <= 0.5 || !extra.is_finite() {
            continue;
        }
        let space_count = glyphs[first_non_ws..effective_end]
            .iter()
            .filter(|glyph| glyph.chr == ' ')
            .count();
        if space_count == 0 {
            continue;
        }
        let per_space = extra / space_count as f32;
        let row_origin_x = origin_x + placed_row.pos.x;
        let row = Arc::make_mut(&mut placed_row.row);
        let glyph_vertex_end = row
            .visuals
            .glyph_vertex_range
            .end
            .min(row.visuals.mesh.vertices.len());
        let mut ideal_shift = 0.0f32;
        let mut word_shift = 0.0f32;
        let mut in_word = false;
        for (idx, glyph) in row.glyphs.iter_mut().enumerate() {
            let is_ws = glyph.chr.is_whitespace();
            let active_shift = if idx >= first_non_ws && idx < effective_end && !is_ws {
                if !in_word {
                    in_word = true;
                    let screen_x = row_origin_x + glyph.pos.x + ideal_shift;
                    let snapped_screen_x = (screen_x * ppp).round() / ppp;
                    word_shift = snapped_screen_x - row_origin_x - glyph.pos.x;
                }
                word_shift
            } else {
                in_word = false;
                ideal_shift
            };
            glyph.pos.x += active_shift;
            if active_shift != 0.0 && !glyph.uv_rect.is_nothing() {
                let v_start = glyph.first_vertex as usize;
                let v_end = v_start.saturating_add(4);
                if v_end <= glyph_vertex_end {
                    for vertex in &mut row.visuals.mesh.vertices[v_start..v_end] {
                        vertex.pos.x += active_shift;
                    }
                }
            }
            if idx >= first_non_ws && idx < effective_end && glyph.chr == ' ' {
                ideal_shift += per_space;
            }
        }
        // Stretch any row-wide strikethrough/underline hline vertices that
        // epaint appended after the glyph vertices.
        if glyph_vertex_end < row.visuals.mesh.vertices.len() && content_right > 0.0 {
            let new_left = row.glyphs[first_non_ws..effective_end]
                .iter()
                .map(|glyph| glyph.pos.x)
                .fold(f32::INFINITY, f32::min);
            let new_right = row.glyphs[first_non_ws..effective_end]
                .iter()
                .map(|glyph| glyph.pos.x + glyph.advance_width)
                .fold(0.0f32, f32::max);
            let old_span = content_right - content_left_orig;
            if old_span > 0.5 && old_span.is_finite() {
                let scale_x = (new_right - new_left) / old_span;
                for vertex in &mut row.visuals.mesh.vertices[glyph_vertex_end..] {
                    vertex.pos.x = new_left + (vertex.pos.x - content_left_orig) * scale_x;
                }
            }
        }
        row.visuals.mesh_bounds = row.visuals.mesh.calc_bounds();
        row.size.x = target_width;
    }

    let mut rect = egui::Rect::ZERO;
    let mut mesh_bounds = egui::Rect::NOTHING;
    for placed_row in &g.rows {
        rect |= placed_row.rect();
        mesh_bounds |= placed_row
            .visuals
            .mesh_bounds
            .translate(placed_row.pos.to_vec2());
    }
    g.rect = rect;
    g.mesh_bounds = mesh_bounds;
}

fn is_vector_bullet_char(ch: char) -> bool {
    matches!(
        ch,
        '★' | '☆'
            | '✔'
            | '✓'
            | '➤'
            | '➢'
            | '➔'
            | '→'
            | '◆'
            | '◇'
            | '►'
            | '▶'
            | '▪'
            | '■'
            | '□'
            | '●'
            | '○'
            | '•'
    )
}

fn marker_needs_vector(galley: &egui::Galley, ch: char) -> bool {
    if is_vector_bullet_char(ch) {
        return true;
    }
    galley
        .rows
        .first()
        .and_then(|row| row.glyphs.first())
        .is_some_and(|glyph| glyph.chr == '\u{FFFD}' || glyph.uv_rect.size.x <= 0.0)
}

/// Draw one bullet or number marker (vector symbol when the font lacks it,
/// otherwise the laid-out galley). Shared across text boxes, shapes and tables.
pub fn paint_marker(painter: &egui::Painter, marker: &LaidMarker, fallback_color: Color32) {
    if let Some(vb) = marker.vector {
        let r = (vb.size * 0.28).max(1.8);
        let cx = marker.pos.x - r * 1.1;
        let cy = marker.pos.y + vb.size * 0.48;
        let color = vb.color;
        let stroke = Stroke::new((r * 0.35).max(1.0), color);
        match vb.ch {
            '★' | '☆' => {
                let mut pts = Vec::with_capacity(10);
                for i in 0..10 {
                    let angle =
                        -std::f32::consts::FRAC_PI_2 + (i as f32) * std::f32::consts::PI / 5.0;
                    let rad = if i % 2 == 0 { r * 1.25 } else { r * 0.52 };
                    pts.push(pos2(cx + rad * angle.cos(), cy + rad * angle.sin()));
                }
                if vb.ch == '★' {
                    painter.add(egui::Shape::convex_polygon(pts, color, Stroke::NONE));
                } else {
                    painter.add(egui::Shape::closed_line(pts, stroke));
                }
            }
            '✔' | '✓' => {
                let pts = vec![
                    pos2(cx - r * 0.9, cy + r * 0.05),
                    pos2(cx - r * 0.2, cy + r * 0.75),
                    pos2(cx + r * 1.0, cy - r * 0.65),
                ];
                painter.add(egui::Shape::line(
                    pts,
                    Stroke::new((r * 0.45).max(1.2), color),
                ));
            }
            '➤' | '➢' | '►' | '▶' => {
                let pts = vec![
                    pos2(cx - r * 0.75, cy - r * 0.85),
                    pos2(cx + r * 0.95, cy),
                    pos2(cx - r * 0.75, cy + r * 0.85),
                ];
                painter.add(egui::Shape::convex_polygon(pts, color, Stroke::NONE));
            }
            '➔' | '→' => {
                let line_y = cy;
                painter.line_segment(
                    [pos2(cx - r * 0.95, line_y), pos2(cx + r * 0.5, line_y)],
                    stroke,
                );
                let head = vec![
                    pos2(cx + r * 0.2, cy - r * 0.6),
                    pos2(cx + r * 1.0, cy),
                    pos2(cx + r * 0.2, cy + r * 0.6),
                ];
                painter.add(egui::Shape::convex_polygon(head, color, Stroke::NONE));
            }
            '◆' | '◇' => {
                let pts = vec![
                    pos2(cx, cy - r),
                    pos2(cx + r, cy),
                    pos2(cx, cy + r),
                    pos2(cx - r, cy),
                ];
                if vb.ch == '◆' {
                    painter.add(egui::Shape::convex_polygon(pts, color, Stroke::NONE));
                } else {
                    painter.add(egui::Shape::closed_line(pts, stroke));
                }
            }
            '▪' | '■' | '□' => {
                let side = if vb.ch == '▪' { r * 0.72 } else { r * 0.9 };
                let rect = Rect::from_center_size(pos2(cx, cy), egui::vec2(side * 2.0, side * 2.0));
                if vb.ch == '□' {
                    painter.rect_stroke(
                        rect,
                        egui::CornerRadius::ZERO,
                        stroke,
                        egui::StrokeKind::Inside,
                    );
                } else {
                    painter.rect_filled(rect, egui::CornerRadius::ZERO, color);
                }
            }
            '○' => {
                painter.circle_stroke(pos2(cx, cy), r * 0.75, stroke);
            }
            _ => {
                let rad = if vb.ch == '•' { r * 0.65 } else { r * 0.8 };
                painter.circle_filled(pos2(cx, cy), rad, color);
            }
        }
    } else {
        painter.galley(marker.pos, marker.galley.clone(), fallback_color);
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
    marker_size: f32,
    color: Color32,
) -> Option<(Arc<egui::Galley>, f32)> {
    let size = (marker_size.clamp(1.0, 400.0) * scale * shrink).max(1.0);
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
    wrap: bool,
    justify_x_offset: f32,
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
    let mut levels = [LevelState::default(); 5];
    let mut previous_list: Option<ListKind> = None;

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
        let marker = marker_for(&mut levels, &mut previous_list, &style, level);

        let mut job = LayoutJob {
            halign: halign(align),
            wrap: TextWrapping {
                max_width: if wrap { content_width } else { f32::INFINITY },
                ..Default::default()
            },
            break_on_newline: false,
            ..Default::default()
        };
        let mut spans: Vec<(usize, usize, Link, String)> = Vec::new();
        let mut underlines: Vec<(usize, usize, Stroke)> = Vec::new();
        let mut highlights: Vec<(usize, usize, Color32)> = Vec::new();
        let mut shadow_job: Option<LayoutJob> =
            paragraph_runs.iter().any(|r| r.shadow).then(|| LayoutJob {
                halign: job.halign,
                wrap: job.wrap.clone(),
                break_on_newline: false,
                ..Default::default()
            });
        let mut shadow_offset = 0.0f32;
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
            if let Some(shadow_job) = &mut shadow_job {
                let shadow_color = if run.shadow {
                    shadow_color_for(run.color)
                } else {
                    Color32::TRANSPARENT
                };
                if run.shadow {
                    shadow_offset = shadow_offset.max(shadow_offset_for(size));
                }
                shadow_job.append(
                    &run.text,
                    0.0,
                    TextFormat {
                        font_id: font_id.clone(),
                        color: shadow_color,
                        line_height: Some(line_height),
                        extra_letter_spacing: letter_spacing,
                        valign,
                        ..Default::default()
                    },
                );
            }
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
            if let Some(highlight) = run.highlight {
                highlights.push((start, char_count, highlight));
            }
            if let Some(link) = &run.link {
                spans.push((start, char_count, link.clone(), run.text.clone()));
            }
        }
        let x = match align {
            Align::Left | Align::Justify => rect.left() + text_indent,
            Align::Center => rect.left() + text_indent + content_width * 0.5,
            Align::Right => rect.right(),
        };
        let mut galley = ctx.fonts_mut(|fonts| fonts.layout_job(job));
        if align == Align::Justify && wrap {
            justify_galley_snapped(
                &mut galley,
                content_width,
                ctx.pixels_per_point(),
                x + justify_x_offset,
            );
        }
        let shadow = shadow_job.map(|job| {
            let mut shadow_galley = ctx.fonts_mut(|fonts| fonts.layout_job(job));
            if align == Align::Justify && wrap {
                justify_galley_snapped(
                    &mut shadow_galley,
                    content_width,
                    ctx.pixels_per_point(),
                    x + justify_x_offset,
                );
            }
            (shadow_galley, egui::vec2(shadow_offset, shadow_offset))
        });
        let galley_h = galley.size().y;
        let indent_width = text_indent + galley.size().x;
        max_width = max_width.max(indent_width);

        let top = y + space_before;
        let paragraph_pos = pos2(x, rect.top() + top);
        let paragraph_galley = galley.clone();
        let marker = marker.and_then(|text| {
            let marker_run = paragraph_runs.first()?;
            let marker_color = style
                .bullet_color
                .as_ref()
                .and_then(crate::color::parse_color)
                .unwrap_or(marker_run.color);
            let marker_size = style.bullet_size.unwrap_or(marker_run.size);
            let (galley, width) = marker_text(
                ctx,
                &text,
                marker_run,
                scale,
                shrink,
                marker_size,
                marker_color,
            )?;
            let right = rect.left() + text_indent - (6.0 * scale * shrink).max(1.0);
            let _ = width;
            // Sit the marker on the first line's baseline. Line spacing makes the
            // paragraph's rows taller than the marker's, so equal tops would
            // leave the marker floating above the text.
            let shift = match (first_baseline(&paragraph_galley), first_baseline(&galley)) {
                (Some(text), Some(mark)) => text - mark,
                _ => 0.0,
            };
            let mut chars = text.chars();
            let vector = match (chars.next(), chars.next()) {
                (Some(ch), None)
                    if style.list == ListKind::Bullet && marker_needs_vector(&galley, ch) =>
                {
                    Some(VectorBullet {
                        ch,
                        size: (marker_size.clamp(1.0, 400.0) * scale * shrink).max(1.0),
                        color: marker_color,
                    })
                }
                _ => None,
            };
            Some(LaidMarker {
                galley,
                pos: pos2(right, rect.top() + top + shift),
                vector,
            })
        });
        laid.push(LaidParagraph {
            galley: galley.clone(),
            pos: paragraph_pos,
            marker,
            indent: text_indent,
            height: space_before + galley_h + space_after,
            underlines,
            highlights,
            shadow,
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

/// The shadow colour of a run drawn in `color`: black at 45% of the run's own opacity, so a
/// faded or dimmed run fades its shadow too.
fn shadow_color_for(color: Color32) -> Color32 {
    let alpha = (f32::from(color.a()) * 0.45).round().clamp(0.0, 255.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Color32::from_rgba_unmultiplied(0, 0, 0, alpha as u8)
}

/// Shadow offset for text drawn at `size` screen points: 1.5% of the size, at least 1.
fn shadow_offset_for(size: f32) -> f32 {
    (size * 0.015).max(1.0)
}

/// Paint what goes behind a laid paragraph's glyphs: the highlights, then the shadow.
/// Call before painting `paragraph.galley`.
pub fn paint_paragraph_backdrop(painter: &egui::Painter, paragraph: &LaidParagraph) {
    for (rect, color) in highlight_rects(paragraph, paragraph.pos) {
        painter.rect_filled(rect, 0.0, color);
    }
    if let Some((galley, offset)) = &paragraph.shadow {
        painter.galley(
            paragraph.pos + *offset,
            Arc::clone(galley),
            Color32::TRANSPARENT,
        );
    }
}

/// Screen-space highlight rectangles for a laid paragraph drawn at `origin`: one per run of
/// highlighted glyphs of one colour on a row, covering the row's full height.
#[must_use]
pub fn highlight_rects(paragraph: &LaidParagraph, origin: Pos2) -> Vec<(Rect, Color32)> {
    let mut out = Vec::new();
    if paragraph.highlights.is_empty() {
        return out;
    }
    let mut char_index = 0usize;
    for row in &paragraph.galley.rows {
        let mut active: Option<(Rect, Color32)> = None;
        for glyph in &row.glyphs {
            let color = paragraph
                .highlights
                .iter()
                .find(|(start, end, _)| char_index >= *start && char_index < *end)
                .map(|(_, _, color)| *color);
            let left = origin.x + row.pos.x + glyph.pos.x;
            let cell = Rect::from_min_size(
                pos2(left, origin.y + row.pos.y),
                egui::vec2(glyph.advance_width, row.size.y),
            );
            match (color, &mut active) {
                (Some(color), Some((rect, current))) if *current == color => {
                    *rect = rect.union(cell);
                }
                (Some(color), _) => {
                    if let Some(done) = active.take() {
                        out.push(done);
                    }
                    active = Some((cell, color));
                }
                (None, _) => {
                    if let Some(done) = active.take() {
                        out.push(done);
                    }
                }
            }
            char_index = char_index.saturating_add(1);
        }
        if let Some(done) = active.take() {
            out.push(done);
        }
    }
    out
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

/// Where the text of a box goes: the inset text rect, the rect the text is laid out in
/// (the inset rect turned on its side for vertical text) and the quarter turn to
/// draw it with. The painter, the text effects and the link regions all use this,
/// so they agree. `None` when the inset rect is empty.
pub(crate) fn text_box_layout(
    frame: Rect,
    scale: f32,
    insets: Option<coursecraft_course::TextInsets>,
    direction: coursecraft_course::TextDirection,
) -> Option<(Rect, Rect, f32)> {
    let text_rect = if let Some(ins) = insets {
        let l = (ins.left.max(0.0) * scale).min(frame.width() * 0.45);
        let r = (ins.right.max(0.0) * scale).min(frame.width() * 0.45);
        let t = (ins.top.max(0.0) * scale).min(frame.height() * 0.45);
        let b = (ins.bottom.max(0.0) * scale).min(frame.height() * 0.45);
        Rect::from_min_max(
            pos2(frame.min.x + l, frame.min.y + t),
            pos2(frame.max.x - r, frame.max.y - b),
        )
    } else {
        frame
    };
    if !text_rect.is_positive() {
        return None;
    }
    let (layout_rect, rot_deg) = match direction {
        coursecraft_course::TextDirection::Horizontal => (text_rect, 0.0f32),
        coursecraft_course::TextDirection::Vertical => (
            Rect::from_center_size(
                text_rect.center(),
                egui::vec2(text_rect.height(), text_rect.width()),
            ),
            90.0,
        ),
        coursecraft_course::TextDirection::Vertical270 => (
            Rect::from_center_size(
                text_rect.center(),
                egui::vec2(text_rect.height(), text_rect.width()),
            ),
            270.0,
        ),
    };
    Some((text_rect, layout_rect, rot_deg))
}

/// The link regions of one text box, in slide coordinates (`scale` 1), laid out with
/// the box's wrap, insets and direction exactly as it is painted.
#[allow(clippy::too_many_arguments)]
fn box_links(
    ctx: &Context,
    frame: Rect,
    runs: &[ResolvedTextRun],
    paragraphs: &[Paragraph],
    (align, valign, autofit): (Align, VAlign, Autofit),
    (wrap, insets, direction): (
        bool,
        Option<coursecraft_course::TextInsets>,
        coursecraft_course::TextDirection,
    ),
) -> Vec<LaidLink> {
    let Some((text_rect, layout_rect, rot_deg)) = text_box_layout(frame, 1.0, insets, direction)
    else {
        return Vec::new();
    };
    let laid = layout_text_with_wrap(
        ctx,
        layout_rect,
        1.0,
        runs,
        paragraphs,
        align,
        valign,
        autofit,
        wrap,
    );
    let centre = text_rect.center();
    laid.links
        .into_iter()
        .map(|mut link| {
            if rot_deg != 0.0 {
                let corners = [
                    link.rect.left_top(),
                    link.rect.right_top(),
                    link.rect.right_bottom(),
                    link.rect.left_bottom(),
                ]
                .map(|corner| crate::compose::rotate_point(corner, centre, rot_deg));
                link.rect = Rect::from_points(&corners);
            }
            link
        })
        .collect()
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
                wrap,
                insets,
                direction,
                ..
            } => {
                let links = box_links(
                    ctx,
                    frame,
                    runs,
                    paragraphs,
                    (*align, *valign, *autofit),
                    (*wrap, *insets, *direction),
                );
                found.extend(to_fractions(frame, links));
            }
            Visual::Path(path) if path.text.is_some() => {
                if let Some(text) = &path.text {
                    let inner = frame.shrink((4.0f32).min(frame.width() * 0.25));
                    let links = box_links(
                        ctx,
                        inner,
                        &text.runs,
                        &text.paragraphs,
                        (text.align, text.valign, text.autofit),
                        (text.wrap, text.insets, text.direction),
                    );
                    found.extend(to_fractions(frame, links));
                }
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
                    let links = box_links(
                        ctx,
                        text_rect,
                        &text.runs,
                        &text.paragraphs,
                        (text.align, text.valign, text.autofit),
                        (text.wrap, text.insets, text.direction),
                    );
                    found.extend(to_fractions(frame, links));
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

/// Build an [`egui::text::LayoutJob`] for inline text editing that preserves
/// per-run fonts, sizes, colours, bold/italic, underline, strikethrough,
/// superscript/subscript, letter spacing, and paragraph alignment/line spacing
/// while keeping `job.text` character-for-character identical to the buffer.
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn inline_edit_layout_job(
    ctx: &Context,
    scale: f32,
    shrink: f32,
    runs: &[ResolvedTextRun],
    paragraphs: &[Paragraph],
    align: Align,
    wrap_width: f32,
    multiline: bool,
) -> LayoutJob {
    inline_edit_layout_job_with_wrap(
        ctx, scale, shrink, runs, paragraphs, align, wrap_width, multiline, true,
    )
}

/// Like [`inline_edit_layout_job`], with explicit `wrap` control.
///
/// When `wrap` is `false`, `TextWrapping::max_width` is set to `f32::INFINITY`
/// so unwrapped point text stays on a single line while preserving explicit
/// newline breaks when `multiline` is `true`.
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn inline_edit_layout_job_with_wrap(
    ctx: &Context,
    scale: f32,
    shrink: f32,
    runs: &[ResolvedTextRun],
    paragraphs: &[Paragraph],
    align: Align,
    wrap_width: f32,
    multiline: bool,
    wrap: bool,
) -> LayoutJob {
    let shrink = if shrink.is_finite() {
        shrink.clamp(AUTOFIT_MIN_SCALE, 1.0)
    } else {
        1.0
    };
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    let mut job = LayoutJob {
        halign: halign(align),
        wrap: TextWrapping {
            max_width: if multiline && wrap && wrap_width.is_finite() {
                wrap_width.max(1.0)
            } else {
                f32::INFINITY
            },
            ..Default::default()
        },
        break_on_newline: multiline,
        ..Default::default()
    };
    let mut paragraph_index = 0usize;
    for run in runs {
        let size = (run.size.clamp(1.0, 400.0) * scale * shrink).max(1.0);
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
        let underline = if run.underline || run.link.is_some() {
            Stroke::new((scale * shrink).max(1.0), run.color)
        } else {
            Stroke::NONE
        };
        let font_id =
            crate::font::slide_font_named(ctx, run.font.as_deref(), run.bold, run.italic, size);
        let row_height = ctx.fonts_mut(|fonts| fonts.row_height(&font_id));

        if run.text.is_empty() {
            let style = paragraphs.get(paragraph_index).cloned().unwrap_or_default();
            let spacing = if style.line_spacing.is_finite() {
                style.line_spacing.clamp(0.8, 3.0)
            } else {
                1.0
            };
            let line_height = (row_height * spacing).max(size * 0.5);
            job.append(
                "",
                0.0,
                TextFormat {
                    font_id,
                    color: run.color,
                    background: run.highlight.unwrap_or(Color32::TRANSPARENT),
                    line_height: Some(line_height),
                    strikethrough: strike,
                    underline,
                    extra_letter_spacing: letter_spacing,
                    valign,
                    ..Default::default()
                },
            );
            continue;
        }

        for piece in run.text.split_inclusive('\n') {
            let style = paragraphs.get(paragraph_index).cloned().unwrap_or_default();
            let spacing = if style.line_spacing.is_finite() {
                style.line_spacing.clamp(0.8, 3.0)
            } else {
                1.0
            };
            let line_height = (row_height * spacing).max(size * 0.5);
            job.append(
                piece,
                0.0,
                TextFormat {
                    font_id: font_id.clone(),
                    color: run.color,
                    background: run.highlight.unwrap_or(Color32::TRANSPARENT),
                    line_height: Some(line_height),
                    strikethrough: strike,
                    underline,
                    extra_letter_spacing: letter_spacing,
                    valign,
                    ..Default::default()
                },
            );
            if piece.ends_with('\n') {
                paragraph_index = paragraph_index.saturating_add(1);
            }
        }
    }
    job
}
