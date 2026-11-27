//! Slide item composition into drawing items.

use std::collections::{BTreeMap, BTreeSet};

use coursecraft_course::{
    Autofit, ConnectorStyle, Course, DashStyle, FillStyle, LineEnd, Link, MarkerIcon, MeterStyle,
    Object, ObjectKind, Orientation, Paragraph, PathPoint, PathStyle, PictureFillMode, Question,
    QuestionKind, ScrollBar, ShapeKind, Slide, TextStyle, TimerFormat, Timing, Value,
    clamp_panel_offset, format_timer_text, is_valid_link_url, shape_allows_text,
};
use coursecraft_logic::Player;

use crate::color::parse_color;

const FALLBACK_GREY: egui::Color32 = egui::Color32::from_rgb(128, 128, 128);
pub const DEFAULT_BUTTON_COLOR: egui::Color32 = egui::Color32::from_rgb(0x2F, 0x6F, 0xEB);
pub const DEFAULT_TEXT_COLOR: egui::Color32 = egui::Color32::from_rgb(0x1F, 0x1F, 0x1F);

/// Padding around a target label's text on its soft backing pill.
const LABEL_PAD: egui::Vec2 = egui::Vec2::new(4.0, 2.0);

/// Hit target of a composed item.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Hit {
    None,
    Object(String),
    Choice(String),
    /// Up or down control on a sequence row. `to` is the destination index.
    SequenceMove {
        choice: String,
        to: usize,
    },
    /// Question chrome that swallows clicks (fill-in, matching, disabled buttons).
    QuestionChrome(String),
    /// The hotspot image, while the question is unlocked. A click places a marker.
    HotspotImage,
    /// A drag item, while the question is unlocked. A press starts a drag.
    DragItem(String),
    /// A text link inside an object. A click runs the link instead of the
    /// object's own click triggers.
    TextLink {
        object: String,
        link: Link,
    },
}

/// Placeholder shown in an empty fill-in field.
pub const FILL_IN_PLACEHOLDER: &str = "Type your answer";
/// Placeholder shown until a matching row has a selection.
pub const MATCH_PLACEHOLDER: &str = "Choose…";

/// Resolved presentation of a rich text run.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedTextRun {
    pub text: String,
    pub size: f32,
    pub color: egui::Color32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub baseline: coursecraft_course::Baseline,
    /// Extra space after each letter, in points.
    pub letter_spacing: f32,
    /// Font family name. `None` uses the built-in Inter family.
    pub font: Option<String>,
    pub link: Option<Link>,
    /// Background colour behind the glyphs. `None` draws no highlight.
    pub highlight: Option<egui::Color32>,
    /// Draws a soft shadow behind the glyphs.
    pub shadow: bool,
}

/// A laid-out link region as fractions of the item's frame.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedLinkRect {
    pub link: Link,
    /// `0..=1` inside the item frame.
    pub rect: egui::Rect,
    /// The link's text, for the keyboard ring and screen readers.
    pub text: String,
}

/// The screen rect of the `index`th link in `item`, in the order the runtime
/// lists links (`Player::object_links`).
///
/// A link that wraps lays out one rect per line, so consecutive rects that
/// share a target belong to one link and the result is their union. `None` when
/// the item has no such link.
#[must_use]
pub fn link_group_rect(item: &DrawItem, index: usize) -> Option<egui::Rect> {
    let mut group = 0usize;
    let mut previous: Option<&Link> = None;
    let mut union: Option<egui::Rect> = None;
    for link in &item.links {
        if previous.is_some_and(|previous| previous != &link.link) {
            group += 1;
        }
        previous = Some(&link.link);
        if group > index {
            break;
        }
        if group != index {
            continue;
        }
        let (width, height) = (item.rect.width(), item.rect.height());
        let rect = egui::Rect::from_min_max(
            egui::pos2(
                item.rect.min.x + link.rect.min.x * width,
                item.rect.min.y + link.rect.min.y * height,
            ),
            egui::pos2(
                item.rect.min.x + link.rect.max.x * width,
                item.rect.min.y + link.rect.max.y * height,
            ),
        );
        union = Some(union.map_or(rect, |union| union.union(rect)));
    }
    union
}

/// Resolved outline of a shape.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedOutline {
    pub color: egui::Color32,
    pub width: f32,
    pub dash: DashStyle,
    pub dash_pattern: Option<Vec<f32>>,
    pub join: coursecraft_course::LineJoin,
    pub miter_limit: Option<f32>,
}

/// Resolved drawn path. Points stay as fractions of the item's frame.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedSubpath {
    pub points: Vec<PathPoint>,
    pub closed: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedPath {
    pub points: Vec<PathPoint>,
    pub style: PathStyle,
    pub closed: bool,
    pub subpaths: Vec<ResolvedSubpath>,
    pub fill: Option<egui::Color32>,
    pub fill_style: Option<ResolvedFill>,
    pub outline: Option<ResolvedOutline>,
    pub start_end: LineEnd,
    pub end_end: LineEnd,
    pub connector: Option<ConnectorStyle>,
    pub text: Option<ResolvedShapeText>,
}

/// Resolved text carried inside a shape.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedShapeText {
    pub runs: Vec<ResolvedTextRun>,
    pub align: coursecraft_course::Align,
    pub valign: coursecraft_course::VAlign,
    pub paragraphs: Vec<Paragraph>,
    pub autofit: Autofit,
    pub wrap: bool,
    pub insets: Option<coursecraft_course::TextInsets>,
    pub direction: coursecraft_course::TextDirection,
}

/// Resolved text carried inside one table cell.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedCellText {
    pub runs: Vec<ResolvedTextRun>,
    pub align: coursecraft_course::Align,
    pub valign: coursecraft_course::VAlign,
    pub paragraphs: Vec<Paragraph>,
}

/// One resolved table cell. Spans are kept; covered cells are not drawn.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedTableCell {
    pub text: ResolvedCellText,
    pub fill: Option<egui::Color32>,
    pub row_span: u8,
    pub col_span: u8,
}

/// One resolved table row.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedTableRow {
    pub height: f32,
    pub cells: Vec<ResolvedTableCell>,
}

/// A resolved table ready for drawing.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedTable {
    pub columns: Vec<f32>,
    pub rows: Vec<ResolvedTableRow>,
    pub outer: Option<ResolvedOutline>,
    pub inner: Option<ResolvedOutline>,
    pub header_row: bool,
}

/// One resolved gradient stop. Colours are already usable for drawing.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedGradientStop {
    pub color: egui::Color32,
    pub position: f32,
}

/// A resolved non-solid fill for a shape or button.
#[derive(Debug, Clone, PartialEq)]
pub enum ResolvedFill {
    /// Linear gradient at `angle` degrees clockwise from left-to-right.
    Linear {
        angle: f32,
        stops: Vec<ResolvedGradientStop>,
    },
    /// Radial gradient from `center`, as fractions of the frame.
    Radial {
        center: [f32; 2],
        stops: Vec<ResolvedGradientStop>,
    },
    /// A picture clipped to the shape, stretched or tiled.
    Picture {
        src: String,
        tile: bool,
        /// Cover the shape keeping the picture's proportions, centred (mode `fill`).
        cover: bool,
        tile_scale: [f32; 2],
    },
    /// A repeating preset pattern fill with foreground and background colours.
    Pattern {
        preset: String,
        fg: egui::Color32,
        bg: egui::Color32,
    },
}

/// A resolved drop shadow.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedShadow {
    pub color: egui::Color32,
    pub offset_x: f32,
    pub offset_y: f32,
    pub blur: f32,
}

/// A resolved outer glow.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedGlow {
    pub color: egui::Color32,
    pub size: f32,
}

/// How one answer or reviewed row was marked while reviewing answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewMark {
    /// The learner chose this and it is correct.
    Correct,
    /// The learner chose this and it is wrong.
    Incorrect,
    /// The learner missed this correct answer.
    MissedCorrect,
}

/// Resolved presentation of a slide item.
#[derive(Debug, Clone, PartialEq)]
pub enum Visual {
    Text {
        runs: Vec<ResolvedTextRun>,
        align: coursecraft_course::Align,
        valign: coursecraft_course::VAlign,
        paragraphs: Vec<Paragraph>,
        autofit: Autofit,
        outline: Option<ResolvedOutline>,
        wrap: bool,
        insets: Option<coursecraft_course::TextInsets>,
        direction: coursecraft_course::TextDirection,
    },
    Button {
        label: String,
        fill: egui::Color32,
        fill_style: Option<ResolvedFill>,
        hover_fill: Option<egui::Color32>,
        down_fill: Option<egui::Color32>,
        down_has_fill_style: bool,
        disabled: bool,
    },
    Shape {
        shape: ShapeKind,
        adjust: Vec<f32>,
        fill: egui::Color32,
        fill_style: Option<ResolvedFill>,
        outline: Option<ResolvedOutline>,
        text: Option<ResolvedShapeText>,
        hover_fill: Option<egui::Color32>,
        down_fill: Option<egui::Color32>,
        down_has_fill_style: bool,
        disabled: bool,
    },
    Image {
        src: String,
        alt: String,
        /// The kept part of the source, as validated source fractions. `None`
        /// shows the whole picture.
        crop: Option<egui::Rect>,
        outline: Option<ResolvedOutline>,
        shape: Option<ShapeKind>,
    },
    ImagePlaceholder {
        label: String,
    },
    Video {
        src: String,
        poster: Option<String>,
        playing: bool,
        show_placeholder: bool,
    },
    /// A web object. Only the published player shows the page; every other host
    /// draws a placeholder card naming the source.
    Web {
        label: String,
    },
    /// A random draw from a question bank. Only the authoring view composes
    /// this; at play time the slide is replaced by the drawn questions.
    DrawCard {
        text: String,
    },
    /// A text field. `text` is the variable's current text.
    TextEntry {
        text: String,
        placeholder: String,
        multiline: bool,
    },
    /// A horizontal or vertical slider. `value` is the variable's current number.
    Slider {
        value: f64,
        min: f64,
        max: f64,
        step: f64,
        orientation: Orientation,
    },
    /// A knob. Angles are clockwise degrees from 12 o'clock.
    Dial {
        value: f64,
        min: f64,
        max: f64,
        step: f64,
        start_angle: f32,
        end_angle: f32,
    },
    /// A true/false checkbox.
    Checkbox {
        checked: bool,
        label: String,
    },
    /// All options of a radio group, in model order: `(label, value, selected)`.
    RadioGroup {
        options: Vec<(String, String, bool)>,
    },
    /// A marker dot, with its label card when `open`.
    Marker {
        title: String,
        body: Vec<ResolvedTextRun>,
        paragraphs: Vec<Paragraph>,
        icon: MarkerIcon,
        color: egui::Color32,
        open: bool,
        /// Draw the closed dot's pulse ring (player only, not reduced motion).
        pulse: bool,
    },
    /// A scrolling panel frame; its children carry their own draw items.
    ScrollPanel {
        content_height: f32,
        bar: ScrollBar,
        offset: f32,
        /// Editor-only frame and content-height outline.
        authoring: bool,
        /// Scrollbar thumb in slide coordinates, when one is drawn.
        thumb: Option<egui::Rect>,
    },
    /// A countdown. `runs` carries the formatted remaining time in the
    /// object's text style; `authoring` is true on the canvas, where a
    /// hidden timer gets a labelled placeholder.
    Timer {
        runs: Vec<ResolvedTextRun>,
        align: coursecraft_course::Align,
        valign: coursecraft_course::VAlign,
        format: TimerFormat,
        authoring: bool,
    },
    /// A bar or ring filled from a number variable. `fraction` is the clamped
    /// `0..=1` fill; `percent` is the whole percentage for `show_value`.
    ProgressMeter {
        fraction: f32,
        style: MeterStyle,
        show_value: bool,
        percent: u8,
    },
    Audio {
        src: String,
    },
    /// A Lottie animation. `time_ms` comes from the runtime's media clock;
    /// `None` shows frame 0.
    Lottie {
        src: String,
        loop_playback: bool,
        speed: f32,
    },
    ChoiceRow {
        text: String,
        multiple: bool,
        selected: bool,
        /// Answer review mark, or `None` when not reviewing.
        mark: Option<ReviewMark>,
    },
    Prompt {
        text: String,
    },
    /// Fill-in answer. `text` is the learner's entry, not substituted.
    /// `multiline` is true for an essay, which draws as a multi-line box.
    FillIn {
        text: String,
        placeholder: String,
        multiline: bool,
        /// Answer review mark, or `None` when not reviewing.
        mark: Option<ReviewMark>,
    },
    /// One row of a sequence question, in the current order.
    SequenceRow {
        text: String,
        /// Answer review mark, or `None` when not reviewing.
        mark: Option<ReviewMark>,
    },
    /// Up (`up`) or down button beside a sequence row.
    SequenceButton {
        up: bool,
        enabled: bool,
    },
    /// One matching row. `value` is the chosen right-hand text, or [`MATCH_PLACEHOLDER`].
    MatchRow {
        id: String,
        left: String,
        value: String,
        /// Answer review mark, or `None` when not reviewing.
        mark: Option<ReviewMark>,
    },
    /// The accepted answer written under a wrong fill-in or matching row.
    CorrectAnswer {
        text: String,
    },
    /// A correct hotspot region outlined over its image.
    RegionOutline {
        ellipse: bool,
    },
    /// A learner marker on a hotspot image. Drawn above the image; it does not take hits.
    HotspotMarker,
    /// A drawn line, curve, freeform or connector.
    Path(ResolvedPath),
    /// A grid of rich-text cells with fills and borders.
    Table(ResolvedTable),
}

/// A single item ready for drawing, positioned in slide coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct DrawItem {
    pub hit: Hit,
    pub rect: egui::Rect,
    pub visual: Visual,
    pub dimmed: bool,
    pub opacity: f32,
    pub clip: Option<coursecraft_logic::ClipFrac>,
    /// Clockwise rotation in degrees about the frame centre.
    pub rotation: f32,
    /// Mirror horizontally about the frame centre.
    pub flip_h: bool,
    /// Mirror vertically about the frame centre.
    pub flip_v: bool,
    /// Drop shadow drawn behind the item, before its own content.
    pub shadow: Option<ResolvedShadow>,
    /// Outer glow drawn behind the item, before its own content.
    pub glow: Option<ResolvedGlow>,
    /// Lottie playback position from the runtime's media clock. `None` keeps
    /// the animation on frame 0 (canvas, thumbnails and static renders).
    pub media_time_ms: Option<u64>,
    /// Link regions for text items, as fractions of the frame. Filled by the
    /// render layout pass; empty for everything else.
    pub links: Vec<ResolvedLinkRect>,
    /// The editor shows a mark when a table cell's text overflows. The player
    /// leaves this false and only clips.
    pub show_table_overflow: bool,
    /// A slide-coordinate clip rect, used by children of a scrolling panel.
    pub clip_rect: Option<egui::Rect>,
    /// Answer review mark for a drag item, or `None`.
    pub mark: Option<ReviewMark>,
    /// True when this item is a visual-only extra from an active object state.
    pub is_state_extra: bool,
    /// Animation scale, tint and shaped reveal the painter applies to what it paints.
    pub anim: AnimLook,
}

/// The parts of an animation sample the painter applies to the painted meshes. Offset,
/// rotation, opacity and the rectangular clip go into the item's own fields instead.
#[derive(Debug, Clone, PartialEq)]
pub struct AnimLook {
    /// Scale already applied to `rect`. The painter lays the content out at `rect / scale`
    /// and then scales the painted meshes, so text and borders shrink with the object instead
    /// of rewrapping.
    pub scale: (f32, f32),
    pub tint: Option<coursecraft_logic::Tint>,
    /// Shaped reveal, placed on the animated object's frame.
    pub mask: Option<PlacedMask>,
}

/// A [`coursecraft_logic::Mask`] placed on the slide: its polygons are fractions of `frame`,
/// turned `rotation` degrees clockwise about the frame centre. State extras share their
/// object's placement, so one reveal covers the object and its extras.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedMask {
    pub mask: coursecraft_logic::Mask,
    /// The object's frame in slide coordinates, after the animation's move and scale.
    pub frame: egui::Rect,
    /// How the frame looks turned on screen. A single flip turns the other way, and the
    /// mask itself is never mirrored.
    pub rotation: f32,
}

impl PlacedMask {
    /// The polygons in slide coordinates, shifted by `offset`.
    #[must_use]
    pub fn slide_polys(&self, offset: egui::Vec2) -> Vec<Vec<egui::Pos2>> {
        let f = self.frame.translate(offset);
        let c = f.center();
        self.mask
            .polys()
            .iter()
            .map(|p| {
                p.0.iter()
                    .map(|&(x, y)| {
                        let pt = egui::pos2(f.min.x + x * f.width(), f.min.y + y * f.height());
                        rotate_point(pt, c, self.rotation)
                    })
                    .collect()
            })
            .collect()
    }
}

impl AnimLook {
    /// No scale, tint or mask.
    pub const IDENTITY: Self = Self {
        scale: (1.0, 1.0),
        tint: None,
        mask: None,
    };

    /// Whether the painter has anything to do.
    #[must_use]
    pub fn is_identity(&self) -> bool {
        self.scale == (1.0, 1.0) && self.tint.is_none() && self.mask.is_none()
    }
}

/// A stored crop as source fractions, or `None` when it would draw wrongly or
/// shows the whole picture.
///
/// A crop is usable when every part is finite, the window has a positive size
/// and it lies inside the source picture. Anything else is treated as no crop, so
/// a hostile file can never produce a NaN UV, an inverted rect, or a panic, and
/// a crop that keeps everything draws exactly like no crop.
#[must_use]
pub fn valid_crop_rect(crop: &coursecraft_course::CropRect) -> Option<egui::Rect> {
    if !crop.is_valid() || crop.is_full() {
        return None;
    }
    let (x, y, w, h) = (crop.x, crop.y, crop.w, crop.h);
    Some(egui::Rect::from_min_max(
        egui::pos2(x.clamp(0.0, 1.0), y.clamp(0.0, 1.0)),
        egui::pos2((x + w).clamp(0.0, 1.0), (y + h).clamp(0.0, 1.0)),
    ))
}

/// Validate an object frame, converting it to an [`egui::Rect`] in slide coords.
///
/// Returns `None` if any component is non-finite or if width or height is negative.
#[must_use]
pub fn validate_frame(frame: &coursecraft_course::Rect) -> Option<egui::Rect> {
    if !frame.x.is_finite()
        || !frame.y.is_finite()
        || !frame.w.is_finite()
        || !frame.h.is_finite()
        || frame.w < 0.0
        || frame.h < 0.0
    {
        return None;
    }
    let max_x = frame.x + frame.w;
    let max_y = frame.y + frame.h;
    if !max_x.is_finite() || !max_y.is_finite() {
        return None;
    }
    Some(egui::Rect::from_min_size(
        egui::pos2(frame.x, frame.y),
        egui::vec2(frame.w, frame.h),
    ))
}

/// The scrollbar thumb of a scrolling panel, in slide coordinates, when one is
/// drawn.
pub fn scroll_panel_thumb(
    frame: egui::Rect,
    content_height: f32,
    offset: f32,
    bar: ScrollBar,
) -> Option<egui::Rect> {
    if bar != ScrollBar::Visible
        || !frame.is_positive()
        || !content_height.is_finite()
        || content_height <= frame.height() + 0.5
    {
        return None;
    }
    let track_width = (6.0f32).min(frame.width() * 0.5).max(2.0);
    let inset = 4.0f32.min(frame.width() * 0.25);
    let track = egui::Rect::from_min_max(
        egui::pos2(frame.right() - inset - track_width, frame.top() + inset),
        egui::pos2(frame.right() - inset, frame.bottom() - inset),
    );
    if !track.is_positive() {
        return None;
    }
    let max_offset = (content_height - frame.height()).max(0.0);
    let offset = clamp_panel_offset(content_height, frame.height(), offset);
    let fraction = if max_offset > 0.0 {
        (offset / max_offset).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let thumb_height = (track.height() * frame.height() / content_height)
        .max(24.0f32.min(track.height()))
        .min(track.height());
    let travel = (track.height() - thumb_height).max(0.0);
    Some(egui::Rect::from_min_size(
        egui::pos2(track.left(), track.top() + travel * fraction),
        egui::vec2(track.width(), thumb_height),
    ))
}

/// Returns whether an object with `timing` is visible at `clock_ms`.
///
/// Visible when `start_ms <= clock_ms < end_ms` (`end_ms` None = open).
#[must_use]
pub fn is_visible_at(timing: &Timing, clock_ms: u32) -> bool {
    if clock_ms < timing.start_ms {
        return false;
    }
    if let Some(end_ms) = timing.end_ms
        && clock_ms >= end_ms
    {
        return false;
    }
    true
}

/// Resolve a text object's runs with state overrides and fallback colours.
pub fn resolve_text_runs(
    text: &str,
    style: &TextStyle,
    runs: &[coursecraft_course::TextRun],
    override_text: Option<&str>,
    style_color: egui::Color32,
    link_color: egui::Color32,
    theme_font: Option<&str>,
) -> Vec<ResolvedTextRun> {
    let base_size = if style.size.is_finite() && style.size > 0.0 {
        style.size
    } else {
        20.0
    };
    // Text written before custom fonts defaulted to "Open Sans", which has
    // always been drawn with Inter. Let the course theme name the face for
    // that legacy default while keeping an explicit course family.
    let base_font = if style.font.trim().eq_ignore_ascii_case("Open Sans") {
        theme_font.unwrap_or(&style.font).to_owned()
    } else {
        style.font.clone()
    };
    if let Some(override_text) = override_text {
        return vec![ResolvedTextRun {
            text: override_text.to_owned(),
            size: base_size,
            color: style_color,
            bold: style.bold,
            italic: style.italic,
            underline: style.underline,
            strikethrough: style.strikethrough,
            baseline: style.baseline,
            letter_spacing: style.letter_spacing,
            font: Some(base_font),
            link: None,
            highlight: None,
            shadow: false,
        }];
    }
    if runs.is_empty() {
        return vec![ResolvedTextRun {
            text: text.to_owned(),
            size: base_size,
            color: style_color,
            bold: style.bold,
            italic: style.italic,
            underline: style.underline,
            strikethrough: style.strikethrough,
            baseline: style.baseline,
            letter_spacing: style.letter_spacing,
            font: Some(base_font),
            link: None,
            highlight: None,
            shadow: false,
        }];
    }
    runs.iter()
        .take(1000)
        .map(|r| {
            let size = r.size.unwrap_or(base_size);
            let size = if size.is_finite() && size > 0.0 {
                size
            } else {
                20.0
            };
            let explicit_color = r.color.as_ref().and_then(parse_color);
            let color = explicit_color.unwrap_or(if r.link.is_some() {
                link_color
            } else {
                style_color
            });
            ResolvedTextRun {
                text: r.text.clone(),
                size,
                color,
                bold: r.bold.unwrap_or(style.bold),
                italic: r.italic.unwrap_or(style.italic),
                underline: r.underline.unwrap_or(style.underline),
                strikethrough: r.strikethrough.unwrap_or(style.strikethrough),
                baseline: r.baseline.unwrap_or(style.baseline),
                letter_spacing: r.letter_spacing.unwrap_or(style.letter_spacing),
                font: r.font.clone().or_else(|| Some(base_font.clone())),
                link: r.link.clone().filter(|link| match link {
                    Link::Url { url } => is_valid_link_url(url),
                    Link::Slide { .. } => true,
                }),
                highlight: r
                    .highlight
                    .as_ref()
                    .and_then(parse_color)
                    .filter(|color| color.a() > 0),
                shadow: r.shadow.unwrap_or(false),
            }
        })
        .collect()
}

/// Resolve a shape outline, dropping colours and widths that cannot draw.
fn resolve_outline(outline: &coursecraft_course::Outline) -> Option<ResolvedOutline> {
    if !outline.width.is_finite() || outline.width <= 0.0 || outline.width > 50.0 {
        return None;
    }
    let color = parse_color(&outline.color)?;
    if color.a() == 0 {
        return None;
    }
    Some(ResolvedOutline {
        color,
        width: outline.width,
        dash: outline.dash,
        dash_pattern: outline.dash_pattern.clone(),
        join: outline.join,
        miter_limit: outline.miter_limit,
    })
}

/// Resolve gradient stops, keeping drawable colours and ordering by position.
fn resolve_gradient_stops(stops: &[coursecraft_course::GradientStop]) -> Vec<ResolvedGradientStop> {
    let mut resolved: Vec<ResolvedGradientStop> = stops
        .iter()
        .take(coursecraft_course::MAX_GRADIENT_STOPS)
        .filter_map(|stop| {
            let color = parse_color(&stop.color)?;
            let position = if stop.position.is_finite() {
                stop.position.clamp(0.0, 1.0)
            } else {
                0.0
            };
            Some(ResolvedGradientStop { color, position })
        })
        .collect();
    resolved.sort_by(|a, b| a.position.total_cmp(&b.position));
    resolved
}

/// Resolve a shape or button fill style. Anything unsafe draws the solid fill.
fn resolve_fill_style(style: &FillStyle) -> Option<ResolvedFill> {
    match style {
        FillStyle::Linear { angle, stops } => {
            let stops = resolve_gradient_stops(stops);
            if stops.len() < 2 {
                return None;
            }
            let angle = if angle.is_finite() {
                coursecraft_course::normalize_rotation(*angle)
            } else {
                0.0
            };
            Some(ResolvedFill::Linear { angle, stops })
        }
        FillStyle::Radial { center, stops } => {
            let stops = resolve_gradient_stops(stops);
            if stops.len() < 2 {
                return None;
            }
            let x = if center[0].is_finite() {
                center[0].clamp(0.0, 1.0)
            } else {
                0.5
            };
            let y = if center[1].is_finite() {
                center[1].clamp(0.0, 1.0)
            } else {
                0.5
            };
            Some(ResolvedFill::Radial {
                center: [x, y],
                stops,
            })
        }
        FillStyle::Picture {
            src,
            mode,
            tile_scale,
        } => {
            if src.trim().is_empty() {
                return None;
            }
            Some(ResolvedFill::Picture {
                src: src.clone(),
                tile: *mode == PictureFillMode::Tile,
                cover: *mode == PictureFillMode::Fill,
                tile_scale: *tile_scale,
            })
        }
        FillStyle::Pattern { preset, fg, bg } => Some(ResolvedFill::Pattern {
            preset: preset.clone(),
            fg: parse_color(fg).unwrap_or(egui::Color32::BLACK),
            bg: parse_color(bg).unwrap_or(egui::Color32::WHITE),
        }),
    }
}

fn resolve_shadow(shadow: &coursecraft_course::Shadow) -> Option<ResolvedShadow> {
    let color = parse_color(&shadow.color)?;
    if color.a() == 0 {
        return None;
    }
    Some(ResolvedShadow {
        color,
        offset_x: if shadow.offset_x.is_finite() {
            shadow.offset_x.clamp(-200.0, 200.0)
        } else {
            0.0
        },
        offset_y: if shadow.offset_y.is_finite() {
            shadow.offset_y.clamp(-200.0, 200.0)
        } else {
            0.0
        },
        blur: if shadow.blur.is_finite() {
            shadow.blur.clamp(0.0, 100.0)
        } else {
            0.0
        },
    })
}

fn resolve_glow(glow: &coursecraft_course::Glow) -> Option<ResolvedGlow> {
    let color = parse_color(&glow.color)?;
    if color.a() == 0 {
        return None;
    }
    Some(ResolvedGlow {
        color,
        size: if glow.size.is_finite() {
            glow.size.clamp(0.0, 100.0)
        } else {
            0.0
        },
    })
}

/// Resolve a table, bolding the header row and giving it a faint accent wash
/// when a cell has no fill of its own.
fn resolve_table(
    columns: &[f32],
    rows: &[coursecraft_course::TableRow],
    borders: &coursecraft_course::TableBorders,
    header_row: bool,
    accent: egui::Color32,
    theme: Option<&coursecraft_course::Theme>,
) -> ResolvedTable {
    let theme_font = theme.map(|theme| theme.font.as_str());
    let resolved_rows = rows
        .iter()
        .enumerate()
        .map(|(row_index, row)| {
            let header = header_row && row_index == 0;
            let cells = row
                .cells
                .iter()
                .map(|cell| {
                    let style_color = parse_color(&cell.style.color).unwrap_or(DEFAULT_TEXT_COLOR);
                    let mut runs = resolve_text_runs(
                        &cell.text,
                        &cell.style,
                        &cell.runs,
                        None,
                        style_color,
                        accent,
                        theme_font,
                    );
                    if header {
                        for run in &mut runs {
                            run.bold = true;
                        }
                    }
                    let fill = cell.fill.as_ref().and_then(parse_color).or_else(|| {
                        header.then(|| {
                            egui::Color32::from_rgba_unmultiplied(
                                accent.r(),
                                accent.g(),
                                accent.b(),
                                38,
                            )
                        })
                    });
                    ResolvedTableCell {
                        text: ResolvedCellText {
                            runs,
                            align: cell.style.align,
                            valign: cell.valign,
                            paragraphs: cell.paragraphs.clone(),
                        },
                        fill,
                        row_span: cell.row_span.max(1),
                        col_span: cell.col_span.max(1),
                    }
                })
                .collect();
            ResolvedTableRow {
                height: row.height,
                cells,
            }
        })
        .collect();
    ResolvedTable {
        columns: columns.to_vec(),
        rows: resolved_rows,
        outer: borders.outer.as_ref().and_then(resolve_outline),
        inner: borders.inner.as_ref().and_then(resolve_outline),
        header_row,
    }
}

/// Sanitise a stored path for drawing: finite fractions inside the frame, at
/// most the limit, and a fill only when closed.
fn resolve_path(
    path: &coursecraft_course::Path,
    state_overrides: &ResolvedStateOverrides<'_>,
    default_txt_color: egui::Color32,
    default_btn_color: egui::Color32,
    theme_font: Option<&str>,
) -> Option<ResolvedPath> {
    let clean_points = |pts: &[PathPoint]| -> Vec<PathPoint> {
        pts.iter()
            .take(coursecraft_course::PATH_MAX_POINTS)
            .filter(|point| point.x.is_finite() && point.y.is_finite())
            .map(|point| PathPoint {
                x: point.x.clamp(0.0, 1.0),
                y: point.y.clamp(0.0, 1.0),
            })
            .collect()
    };
    let points = clean_points(&path.points);
    if points.len() < coursecraft_course::PATH_MIN_POINTS {
        return None;
    }
    let subpaths: Vec<ResolvedSubpath> = path
        .subpaths
        .iter()
        .filter_map(|sub| {
            let pts = clean_points(&sub.points);
            (pts.len() >= coursecraft_course::PATH_MIN_POINTS).then_some(ResolvedSubpath {
                points: pts,
                closed: sub.closed,
            })
        })
        .collect();
    let start_end = path.start_end;
    let end_end = path.end_end;
    let connector = path
        .connector
        .filter(|_| points.len() == coursecraft_course::PATH_MIN_POINTS && subpaths.is_empty());
    let fill = if path.closed {
        state_overrides
            .fill
            .and_then(parse_color)
            .or_else(|| path.fill.as_ref().and_then(parse_color))
    } else {
        None
    };
    let fill_style = if path.closed {
        match state_overrides.fill_style {
            NullableOverride::Set(fs) => resolve_fill_style(fs),
            NullableOverride::Clear => None,
            NullableOverride::Inherit => path.fill_style.as_ref().and_then(resolve_fill_style),
        }
    } else {
        None
    };
    let outline = match state_overrides.outline {
        NullableOverride::Set(o) => resolve_outline(o),
        NullableOverride::Clear => None,
        NullableOverride::Inherit => resolve_outline(&path.outline),
    };
    let text = path.text.as_ref().map(|text| {
        let style = state_overrides.text_style.unwrap_or(&text.style);
        ResolvedShapeText {
            runs: resolve_state_text_runs(
                &text.text,
                &text.style,
                &text.runs,
                state_overrides,
                None,
                (default_txt_color, default_btn_color, theme_font),
            ),
            align: style.align,
            valign: style.valign,
            paragraphs: state_overrides
                .paragraphs
                .map(<[coursecraft_course::Paragraph]>::to_vec)
                .unwrap_or_else(|| text.paragraphs.clone()),
            autofit: state_overrides.autofit.unwrap_or(text.autofit),
            wrap: style.wrap,
            insets: style.insets,
            direction: style.direction,
        }
    });
    Some(ResolvedPath {
        points,
        style: path.style,
        closed: path.closed,
        subpaths,
        fill,
        fill_style,
        outline,
        start_end,
        end_end,
        connector,
        text,
    })
}

/// Rotate `point` clockwise by `degrees` about `centre`.
#[must_use]
pub fn rotate_point(point: egui::Pos2, centre: egui::Pos2, degrees: f32) -> egui::Pos2 {
    if !degrees.is_finite() || degrees == 0.0 {
        return point;
    }
    let radians = degrees.to_radians();
    let (sin, cos) = radians.sin_cos();
    let dx = point.x - centre.x;
    let dy = point.y - centre.y;
    egui::pos2(
        centre.x + dx * cos - dy * sin,
        centre.y + dx * sin + dy * cos,
    )
}

/// Mirror `point` about `centre` on the requested axes.
#[must_use]
pub fn flip_point(point: egui::Pos2, centre: egui::Pos2, flip_h: bool, flip_v: bool) -> egui::Pos2 {
    egui::pos2(
        if flip_h {
            2.0 * centre.x - point.x
        } else {
            point.x
        },
        if flip_v {
            2.0 * centre.y - point.y
        } else {
            point.y
        },
    )
}

/// Map a slide-space point into an item's unrotated, unflipped frame space.
#[must_use]
pub fn item_local_point(item: &DrawItem, point: egui::Pos2) -> egui::Pos2 {
    let centre = item.rect.center();
    let point = flip_point(point, centre, item.flip_h, item.flip_v);
    rotate_point(point, centre, -item.rotation)
}

/// The four slide-space corners of an item's rotated and flipped frame.
#[must_use]
pub fn item_corners(item: &DrawItem) -> [egui::Pos2; 4] {
    let centre = item.rect.center();
    let corners = [
        item.rect.left_top(),
        item.rect.right_top(),
        item.rect.right_bottom(),
        item.rect.left_bottom(),
    ];
    corners.map(|corner| {
        let corner = rotate_point(corner, centre, item.rotation);
        flip_point(corner, centre, item.flip_h, item.flip_v)
    })
}

/// The axis-aligned bounding box of an item's rotated frame.
#[must_use]
pub fn item_bounds(item: &DrawItem) -> egui::Rect {
    if item.rotation == 0.0 && !item.flip_h && !item.flip_v {
        return item.rect;
    }
    let mut corners = item_corners(item).into_iter();
    let Some(first) = corners.next() else {
        return item.rect;
    };
    let mut min = first;
    let mut max = first;
    for corner in corners {
        min.x = min.x.min(corner.x);
        min.y = min.y.min(corner.y);
        max.x = max.x.max(corner.x);
        max.y = max.y.max(corner.y);
    }
    egui::Rect::from_min_max(min, max)
}

/// Where a DOM overlay (web video or iframe) for `item` goes: its frame in screen
/// pixels as `[x, y, w, h]`, or `None` while the overlay must hide.
///
/// An overlay cannot be cut to a scroll panel's viewport, so an item that is not
/// wholly inside its `clip_rect` hides, and the canvas poster, which is clipped,
/// shows instead.
#[must_use]
pub fn overlay_screen_rect(
    transform: &crate::layout::SlideTransform,
    item: &DrawItem,
) -> Option<[f32; 4]> {
    if let Some(clip) = item.clip_rect
        && clip.is_positive()
    {
        let tolerance = 0.5;
        let inside = item.rect.min.x >= clip.min.x - tolerance
            && item.rect.min.y >= clip.min.y - tolerance
            && item.rect.max.x <= clip.max.x + tolerance
            && item.rect.max.y <= clip.max.y + tolerance;
        if !inside {
            return None;
        }
    }
    let r = transform.to_screen_rect(item.rect);
    Some([r.min.x, r.min.y, r.width(), r.height()])
}

/// The link under `point` in slide coordinates, if any.
#[must_use]
pub fn item_link_at(item: &DrawItem, point: egui::Pos2) -> Option<&ResolvedLinkRect> {
    if item.links.is_empty()
        || !point.x.is_finite()
        || !point.y.is_finite()
        || !item.rect.is_positive()
    {
        return None;
    }
    if let Some(clip) = item.clip_rect
        && !clip.contains(point)
    {
        return None;
    }
    // Text is not mirrored horizontally (the painter turns it by the opposite
    // angle instead), so undo exactly that transform, not the object's own.
    let point = if item.flip_h {
        let centre = item.rect.center();
        rotate_point(
            flip_point(point, centre, false, item.flip_v),
            centre,
            item.rotation,
        )
    } else {
        item_local_point(item, point)
    };
    let width = item.rect.width();
    let height = item.rect.height();
    if !(width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0) {
        return None;
    }
    let fraction = egui::pos2(
        (point.x - item.rect.min.x) / width,
        (point.y - item.rect.min.y) / height,
    );
    item.links
        .iter()
        .rev()
        .find(|link| link.rect.contains(fraction))
}

/// Whether `point` in slide coordinates hits `item`.
///
/// A path hits within `max(6 px, stroke/2 + 2 px)` of its flattened line, or
/// anywhere inside a closed, filled one. Every other kind keeps frame hit
/// testing. This is the one rule the player and the editor use. Rotation and
/// flips are undone first, so the hit shape is the exact rotated frame.
#[must_use]
pub fn item_hit_test(item: &DrawItem, point: egui::Pos2) -> bool {
    item_hit_test_scaled(item, point, 1.0)
}

/// Like [`item_hit_test`], with the pixel tolerance given in screen pixels.
///
/// `scale` is screen pixels per slide pixel. A path hits within
/// `max(6 / scale, stroke/2 + 2)` slide pixels of its line, so a thin line stays
/// as easy to click when the slide is zoomed out. A `scale` that is not a
/// positive finite number counts as 1.
#[must_use]
pub fn item_hit_test_scaled(item: &DrawItem, point: egui::Pos2, scale: f32) -> bool {
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    if !point.x.is_finite() || !point.y.is_finite() || !item.rect.is_positive() {
        return false;
    }
    if let Some(clip) = item.clip_rect
        && !clip.contains(point)
    {
        return false;
    }
    let point = item_local_point(item, point);
    let Visual::Path(path) = &item.visual else {
        return item.rect.contains(point);
    };
    let to_slide = |points: &[PathPoint]| -> Vec<crate::path_geometry::Pt> {
        points
            .iter()
            .map(|p| {
                (
                    item.rect.min.x + p.x * item.rect.width(),
                    item.rect.min.y + p.y * item.rect.height(),
                )
            })
            .collect()
    };
    let half_stroke = path
        .outline
        .as_ref()
        .map_or(0.0, |outline| outline.width * 0.5);
    let threshold = (half_stroke + 2.0).max(6.0 / scale);
    let target = (point.x, point.y);
    // The main contour, then every subpath. Closed contours fill together with
    // even-odd winding, as they draw, so a hole is not a hit.
    let mut crossings = 0usize;
    let contours = std::iter::once((to_slide(&path.points), path.closed, path.connector)).chain(
        path.subpaths
            .iter()
            .map(|sub| (to_slide(&sub.points), sub.closed, None)),
    );
    for (points, closed, connector) in contours {
        let flat = crate::path_geometry::flatten(&points, path.style, connector, closed);
        if crate::path_geometry::path_distance(&flat, target) <= threshold {
            return true;
        }
        if closed && crate::path_geometry::path_contains(&flat, target) {
            crossings += 1;
        }
    }
    // A path with a colour fill or a gradient or picture fill style has a fill.
    let has_fill = path.fill.is_some() || path.fill_style.is_some();
    has_fill && crossings % 2 == 1
}

///
/// Returns `None` if the object frame is invalid, or for non-visual objects (such as audio).
/// Timing and hidden-state filtering are left to the caller.
#[must_use]
pub fn compose_object(object: &Object, current_state: &str) -> Option<DrawItem> {
    compose_object_with_theme(object, current_state, None)
}

/// Compose a single object with an optional course theme.
#[must_use]
pub fn compose_object_with_theme(
    object: &Object,
    current_state: &str,
    theme: Option<&coursecraft_course::Theme>,
) -> Option<DrawItem> {
    compose_object_with_value(object, current_state, theme, None)
}

/// Compose an object with the value its input variable currently holds.
///
/// `value` is `None` on the authoring canvas with no course variable, or for
/// non-input objects. Inputs read their displayed value from the variable on
/// every frame, so a `set_variable` action moves a slider.
#[must_use]
pub fn compose_object_with_value(
    object: &Object,
    current_state: &str,
    theme: Option<&coursecraft_course::Theme>,
    value: Option<&Value>,
) -> Option<DrawItem> {
    compose_object_opts(
        object,
        current_state,
        theme,
        value,
        ComposeOpts {
            authoring: true,
            ..ComposeOpts::default()
        },
    )
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum PointerState {
    #[default]
    None,
    Hovered,
    Pressed,
}

fn pointer_state_for(
    hovered: Option<&str>,
    pressed: Option<&str>,
    object_id: &str,
) -> PointerState {
    if pressed == Some(object_id) {
        PointerState::Pressed
    } else if hovered == Some(object_id) {
        PointerState::Hovered
    } else {
        PointerState::None
    }
}

#[derive(Debug, Clone, Copy, Default)]
enum NullableOverride<T> {
    #[default]
    Inherit,
    Set(T),
    Clear,
}

fn apply_nullable<'a, T>(dst: &mut NullableOverride<&'a T>, val: Option<&'a T>, clear: bool) {
    if let Some(v) = val {
        *dst = NullableOverride::Set(v);
    } else if clear {
        *dst = NullableOverride::Clear;
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct ResolvedStateOverrides<'a> {
    fill: Option<&'a coursecraft_course::Color>,
    text: Option<&'a str>,
    src: Option<&'a str>,
    fill_style: NullableOverride<&'a coursecraft_course::FillStyle>,
    outline: NullableOverride<&'a coursecraft_course::Outline>,
    text_style: Option<&'a TextStyle>,
    runs: Option<&'a [coursecraft_course::TextRun]>,
    paragraphs: Option<&'a [coursecraft_course::Paragraph]>,
    autofit: Option<Autofit>,
    opacity: Option<f32>,
    shadow: NullableOverride<&'a coursecraft_course::Shadow>,
    glow: NullableOverride<&'a coursecraft_course::Glow>,
    crop: NullableOverride<&'a coursecraft_course::CropRect>,
    image_shape: NullableOverride<coursecraft_course::ShapeKind>,
    rotation: Option<f32>,
    flip_h: Option<bool>,
    flip_v: Option<bool>,
}

fn resolve_state_text_runs(
    base_text: &str,
    base_style: &TextStyle,
    base_runs: &[coursecraft_course::TextRun],
    state_overrides: &ResolvedStateOverrides<'_>,
    override_fill_color: Option<egui::Color32>,
    (default_txt_color, link_color, theme_font): (egui::Color32, egui::Color32, Option<&str>),
) -> Vec<ResolvedTextRun> {
    let style = state_overrides.text_style.unwrap_or(base_style);
    let style_color = override_fill_color
        .unwrap_or_else(|| parse_color(&style.color).unwrap_or(default_txt_color));
    let (runs, override_text) = match state_overrides.runs {
        Some(r) if !r.is_empty() => (r, None),
        Some(_) => (&[][..], state_overrides.text),
        None => (base_runs, state_overrides.text),
    };
    resolve_text_runs(
        base_text,
        style,
        runs,
        override_text,
        style_color,
        link_color,
        theme_font,
    )
}

fn resolve_shape_text(
    shape: coursecraft_course::ShapeKind,
    text: Option<&coursecraft_course::ShapeText>,
    state_overrides: &ResolvedStateOverrides<'_>,
    theme: Option<&coursecraft_course::Theme>,
    (default_txt_color, default_btn_color): (egui::Color32, egui::Color32),
) -> Option<ResolvedShapeText> {
    if !shape_allows_text(shape) {
        return None;
    }
    let theme_font = theme.map(|t| t.font.as_str());
    if let Some(text) = text {
        let resolved_style = state_overrides.text_style.unwrap_or(&text.style);
        return Some(ResolvedShapeText {
            runs: resolve_state_text_runs(
                &text.text,
                &text.style,
                &text.runs,
                state_overrides,
                None,
                (default_txt_color, default_btn_color, theme_font),
            ),
            align: resolved_style.align,
            valign: resolved_style.valign,
            paragraphs: state_overrides
                .paragraphs
                .map(<[coursecraft_course::Paragraph]>::to_vec)
                .unwrap_or_else(|| text.paragraphs.clone()),
            autofit: state_overrides.autofit.unwrap_or(text.autofit),
            wrap: resolved_style.wrap,
            insets: resolved_style.insets,
            direction: resolved_style.direction,
        });
    }
    if state_overrides.text.is_none() && !state_overrides.runs.is_some_and(|r| !r.is_empty()) {
        return None;
    }
    let base_style = TextStyle {
        color: theme.map_or_else(
            || coursecraft_course::Color(String::from("#1E293B")),
            |t| t.text.clone(),
        ),
        ..TextStyle::default()
    };
    let resolved_style = state_overrides.text_style.unwrap_or(&base_style);
    Some(ResolvedShapeText {
        runs: resolve_state_text_runs(
            "",
            &base_style,
            &[],
            state_overrides,
            None,
            (default_txt_color, default_btn_color, theme_font),
        ),
        align: state_overrides
            .text_style
            .map_or(coursecraft_course::Align::Center, |s| s.align),
        valign: state_overrides
            .text_style
            .map_or(coursecraft_course::VAlign::Middle, |s| s.valign),
        paragraphs: state_overrides
            .paragraphs
            .map(<[coursecraft_course::Paragraph]>::to_vec)
            .unwrap_or_default(),
        autofit: state_overrides.autofit.unwrap_or(Autofit::None),
        wrap: resolved_style.wrap,
        insets: resolved_style.insets,
        direction: resolved_style.direction,
    })
}

/// Resolve an object's effective state overrides in layers:
/// base (`normal`) -> runtime state (`selected`, `visited`, custom...) -> `hover`/`down`.
/// `down` wins over `hover`, and pointer states are skipped when `disabled` or `hidden`.
fn resolve_state_overrides<'a>(
    object: &'a Object,
    current_state: &str,
    pointer: PointerState,
) -> ResolvedStateOverrides<'a> {
    let mut out = ResolvedStateOverrides::default();
    let apply = |out: &mut ResolvedStateOverrides<'a>, state: &'a coursecraft_course::StateDef| {
        if let Some(fill) = state.fill.as_ref() {
            out.fill = Some(fill);
        }
        if let Some(text) = state.text.as_deref() {
            out.text = Some(text);
        }
        if let Some(src) = state.src.as_deref().filter(|s| !s.is_empty()) {
            out.src = Some(src);
        }
        if let Some(fs) = state.fill_style.as_ref() {
            out.fill_style = NullableOverride::Set(fs);
        } else if state.clear_fill_style || state.fill.is_some() {
            // ST-05: solid `fill` without `fill_style` suppresses lower layers' `fill_style`.
            out.fill_style = NullableOverride::Clear;
        }
        apply_nullable(
            &mut out.outline,
            state.outline.as_ref(),
            state.clear_outline,
        );
        if let Some(ts) = state.text_style.as_ref() {
            out.text_style = Some(ts);
        }
        if let Some(runs) = state.runs.as_deref() {
            out.runs = Some(runs);
        } else if state.text.is_some() {
            out.runs = None;
        }
        if let Some(paragraphs) = state.paragraphs.as_deref() {
            out.paragraphs = Some(paragraphs);
        }
        if let Some(autofit) = state.autofit {
            out.autofit = Some(autofit);
        }
        if let Some(opacity) = state.opacity {
            out.opacity = Some(opacity);
        }
        apply_nullable(&mut out.shadow, state.shadow.as_ref(), state.clear_shadow);
        apply_nullable(&mut out.glow, state.glow.as_ref(), state.clear_glow);
        apply_nullable(&mut out.crop, state.crop.as_ref(), state.clear_crop);
        if let Some(shape) = state.image_shape {
            out.image_shape = NullableOverride::Set(shape);
        } else if state.clear_image_shape {
            out.image_shape = NullableOverride::Clear;
        }
        if let Some(rotation) = state.rotation {
            out.rotation = Some(rotation);
        }
        if let Some(flip_h) = state.flip_h {
            out.flip_h = Some(flip_h);
        }
        if let Some(flip_v) = state.flip_v {
            out.flip_v = Some(flip_v);
        }
    };
    if current_state != "normal"
        && let Some(state) = object.states.iter().find(|s| s.name == current_state)
    {
        apply(&mut out, state);
    }
    if current_state != "disabled" && current_state != "hidden" {
        let pointer_name = match pointer {
            PointerState::Pressed => Some("down"),
            PointerState::Hovered => Some("hover"),
            PointerState::None => None,
        };
        if let Some(name) = pointer_name
            && let Some(state) = object.states.iter().find(|s| s.name == name)
        {
            apply(&mut out, state);
        }
    }
    out
}

#[derive(Debug, Clone, Copy, Default)]
struct ComposeOpts {
    marker_open: bool,
    marker_pulse: bool,
    authoring: bool,
    timer_remaining_ms: Option<u32>,
    pointer: PointerState,
}

fn apply_runtime_media_state(item: &mut DrawItem, player: &Player<'_>, id: &str) {
    if matches!(item.visual, Visual::Lottie { .. }) {
        item.media_time_ms = player.media_clock_ms(id);
    }
    if let Visual::Video { playing, .. } = &mut item.visual {
        *playing = player.is_media_playing(id);
    }
}

/// Compose a marker or panel as the running player draws it.
pub(crate) fn compose_object_with_runtime(
    object: &Object,
    current_state: &str,
    theme: Option<&coursecraft_course::Theme>,
    value: Option<&Value>,
    marker_open: bool,
) -> Option<DrawItem> {
    compose_object_with_runtime_and_timer(
        object,
        current_state,
        theme,
        value,
        marker_open,
        None,
        PointerState::None,
    )
}

/// Compose as the running player draws it, with a timer's live remaining time.
pub(crate) fn compose_object_with_runtime_and_timer(
    object: &Object,
    current_state: &str,
    theme: Option<&coursecraft_course::Theme>,
    value: Option<&Value>,
    marker_open: bool,
    timer_remaining_ms: Option<u32>,
    pointer: PointerState,
) -> Option<DrawItem> {
    compose_object_opts(
        object,
        current_state,
        theme,
        value,
        ComposeOpts {
            marker_open,
            marker_pulse: true,
            authoring: false,
            timer_remaining_ms,
            pointer,
        },
    )
}

fn resolve_timer_visual(
    duration_ms: u32,
    format: coursecraft_course::TimerFormat,
    style: &TextStyle,
    state_overrides: &ResolvedStateOverrides<'_>,
    (default_txt_color, default_btn_color, theme_font): (
        egui::Color32,
        egui::Color32,
        Option<&str>,
    ),
    opts: ComposeOpts,
) -> Visual {
    let remaining = opts.timer_remaining_ms.unwrap_or(duration_ms);
    let text = format_timer_text(remaining, format);
    let resolved_style = state_overrides.text_style.unwrap_or(style);
    let style_color = if state_overrides.text_style.is_some() {
        parse_color(&resolved_style.color).unwrap_or(default_txt_color)
    } else {
        state_overrides
            .fill
            .and_then(parse_color)
            .unwrap_or_else(|| parse_color(&resolved_style.color).unwrap_or(default_txt_color))
    };
    let runs = resolve_text_runs(
        &text,
        resolved_style,
        &[],
        None,
        style_color,
        default_btn_color,
        theme_font,
    );
    Visual::Timer {
        runs,
        align: resolved_style.align,
        valign: resolved_style.valign,
        format,
        authoring: opts.authoring,
    }
}

fn compose_object_opts(
    object: &Object,
    current_state: &str,
    theme: Option<&coursecraft_course::Theme>,
    value: Option<&Value>,
    opts: ComposeOpts,
) -> Option<DrawItem> {
    let rect = validate_frame(&object.frame)?;
    let state_overrides = resolve_state_overrides(object, current_state, opts.pointer);
    let disabled = current_state == "disabled";

    let default_btn_color = theme
        .and_then(|t| parse_color(&t.accent))
        .unwrap_or(DEFAULT_BUTTON_COLOR);
    let default_txt_color = theme
        .and_then(|t| parse_color(&t.text))
        .unwrap_or(DEFAULT_TEXT_COLOR);

    let hover_fill = object
        .states
        .iter()
        .find(|s| s.name == "hover" && s.fill_style.is_none())
        .and_then(|s| s.fill.as_ref())
        .and_then(parse_color);

    let down_fill = object
        .states
        .iter()
        .find(|s| s.name == "down" && s.fill_style.is_none())
        .and_then(|s| s.fill.as_ref())
        .and_then(parse_color);

    let down_has_fill_style = object
        .states
        .iter()
        .any(|s| s.name == "down" && s.fill_style.is_some());

    let visual = match &object.kind {
        ObjectKind::Text {
            text,
            style,
            runs,
            paragraphs,
            autofit,
            outline,
            ..
        } => {
            let resolved_style = state_overrides.text_style.unwrap_or(style);
            let resolved_runs = resolve_state_text_runs(
                text,
                style,
                runs,
                &state_overrides,
                state_overrides.fill.and_then(parse_color),
                (
                    default_txt_color,
                    default_btn_color,
                    theme.map(|t| t.font.as_str()),
                ),
            );

            Visual::Text {
                runs: resolved_runs,
                align: resolved_style.align,
                valign: resolved_style.valign,
                paragraphs: state_overrides
                    .paragraphs
                    .map(<[coursecraft_course::Paragraph]>::to_vec)
                    .unwrap_or_else(|| paragraphs.clone()),
                autofit: state_overrides.autofit.unwrap_or(*autofit),
                outline: match state_overrides.outline {
                    NullableOverride::Set(o) => resolve_outline(o),
                    NullableOverride::Clear => None,
                    NullableOverride::Inherit => outline.as_ref().and_then(resolve_outline),
                },
                wrap: resolved_style.wrap,
                insets: resolved_style.insets,
                direction: resolved_style.direction,
            }
        }
        ObjectKind::Button {
            label,
            fill,
            fill_style,
        } => {
            let state_fill = state_overrides.fill.and_then(parse_color);
            let resolved_label = state_overrides
                .text
                .map(str::to_owned)
                .unwrap_or_else(|| label.clone());
            let resolved_fill =
                state_fill.unwrap_or_else(|| parse_color(fill).unwrap_or(default_btn_color));
            Visual::Button {
                label: resolved_label,
                fill: resolved_fill,
                fill_style: match state_overrides.fill_style {
                    NullableOverride::Set(fs) => resolve_fill_style(fs),
                    NullableOverride::Clear => None,
                    NullableOverride::Inherit => fill_style.as_ref().and_then(resolve_fill_style),
                },
                hover_fill,
                down_fill,
                down_has_fill_style,
                disabled,
            }
        }
        ObjectKind::Shape {
            shape,
            fill,
            fill_style,
            outline,
            text,
            adjust,
        } => {
            let state_fill = state_overrides.fill.and_then(parse_color);
            let resolved_fill =
                state_fill.unwrap_or_else(|| parse_color(fill).unwrap_or(FALLBACK_GREY));
            let text = resolve_shape_text(
                *shape,
                text.as_ref(),
                &state_overrides,
                theme,
                (default_txt_color, default_btn_color),
            );
            Visual::Shape {
                shape: *shape,
                adjust: adjust.clone(),
                fill: resolved_fill,
                fill_style: match state_overrides.fill_style {
                    NullableOverride::Set(fs) => resolve_fill_style(fs),
                    NullableOverride::Clear => None,
                    NullableOverride::Inherit => fill_style.as_ref().and_then(resolve_fill_style),
                },
                outline: if !shape_allows_text(*shape) {
                    None
                } else {
                    match state_overrides.outline {
                        NullableOverride::Set(o) => resolve_outline(o),
                        NullableOverride::Clear => None,
                        NullableOverride::Inherit => outline.as_ref().and_then(resolve_outline),
                    }
                },
                text,
                hover_fill,
                down_fill,
                down_has_fill_style,
                disabled,
            }
        }
        ObjectKind::Path { path } => {
            let resolved = resolve_path(
                path,
                &state_overrides,
                default_txt_color,
                default_btn_color,
                theme.map(|t| t.font.as_str()),
            )?;
            Visual::Path(resolved)
        }
        ObjectKind::Table {
            columns,
            rows,
            borders,
            header_row,
        } => Visual::Table(resolve_table(
            columns,
            rows,
            borders,
            *header_row,
            default_btn_color,
            theme,
        )),
        ObjectKind::Image {
            src,
            alt,
            crop,
            outline,
            shape,
        } => Visual::Image {
            src: state_overrides.src.unwrap_or(src).to_string(),
            alt: alt.clone(),
            crop: match state_overrides.crop {
                NullableOverride::Set(c) => valid_crop_rect(c),
                NullableOverride::Clear => None,
                NullableOverride::Inherit => crop.as_ref().and_then(valid_crop_rect),
            },
            outline: match state_overrides.outline {
                NullableOverride::Set(o) => resolve_outline(o),
                NullableOverride::Clear => None,
                NullableOverride::Inherit => outline.as_ref().and_then(resolve_outline),
            },
            shape: match state_overrides.image_shape {
                NullableOverride::Set(s) => Some(s),
                NullableOverride::Clear => None,
                NullableOverride::Inherit => *shape,
            },
        },
        ObjectKind::Video { src, poster, .. } => Visual::Video {
            src: state_overrides.src.unwrap_or(src).to_string(),
            poster: poster.clone(),
            playing: false,
            show_placeholder: opts.authoring,
        },
        ObjectKind::Web { source } => Visual::Web {
            label: source.label(),
        },
        ObjectKind::TextEntry {
            placeholder,
            multiline,
            ..
        } => Visual::TextEntry {
            text: match value {
                Some(Value::Text(text)) => text.clone(),
                _ => String::new(),
            },
            placeholder: placeholder.clone(),
            multiline: *multiline,
        },
        ObjectKind::Slider {
            min,
            max,
            step,
            orientation,
            ..
        } => Visual::Slider {
            value: match value {
                Some(Value::Number(number)) => *number,
                _ => *min,
            },
            min: *min,
            max: *max,
            step: *step,
            orientation: *orientation,
        },
        ObjectKind::Dial {
            min,
            max,
            step,
            start_angle,
            end_angle,
            ..
        } => Visual::Dial {
            value: match value {
                Some(Value::Number(number)) => *number,
                _ => *min,
            },
            min: *min,
            max: *max,
            step: *step,
            start_angle: *start_angle,
            end_angle: *end_angle,
        },
        ObjectKind::Checkbox { label, .. } => Visual::Checkbox {
            checked: matches!(value, Some(Value::Bool(true))),
            label: label.clone(),
        },
        ObjectKind::RadioGroup { options, .. } => Visual::RadioGroup {
            options: options
                .iter()
                .map(|option| {
                    let selected =
                        matches!(value, Some(Value::Text(text)) if *text == option.value);
                    (option.label.clone(), option.value.clone(), selected)
                })
                .collect(),
        },
        ObjectKind::Marker {
            title,
            body,
            runs,
            paragraphs,
            icon,
            color,
        } => {
            let marker_color = color
                .as_ref()
                .and_then(parse_color)
                .or_else(|| theme.and_then(|t| parse_color(&t.accent)))
                .unwrap_or(DEFAULT_BUTTON_COLOR);
            let marker_text = theme
                .map(|t| t.text.clone())
                .unwrap_or_else(|| coursecraft_course::Color("#1F1F1F".to_owned()));
            let style = TextStyle {
                size: 14.0,
                color: marker_text,
                ..TextStyle::default()
            };
            let body_runs = resolve_text_runs(
                body,
                &style,
                runs,
                None,
                default_txt_color,
                default_btn_color,
                theme.map(|t| t.font.as_str()),
            );
            Visual::Marker {
                title: title.clone(),
                body: body_runs,
                paragraphs: paragraphs.clone(),
                icon: *icon,
                color: marker_color,
                open: opts.marker_open,
                pulse: opts.marker_pulse && !opts.marker_open,
            }
        }
        ObjectKind::ScrollPanel {
            content_height,
            bar,
        } => {
            let content = if content_height.is_finite() {
                content_height.max(0.0)
            } else {
                0.0
            };
            Visual::ScrollPanel {
                content_height: content,
                bar: *bar,
                offset: 0.0,
                authoring: opts.authoring,
                thumb: if opts.authoring {
                    None
                } else {
                    scroll_panel_thumb(rect, content, 0.0, *bar)
                },
            }
        }
        ObjectKind::Timer {
            duration_ms,
            format,
            style,
            ..
        } => resolve_timer_visual(
            *duration_ms,
            *format,
            style,
            &state_overrides,
            (
                default_txt_color,
                default_btn_color,
                theme.map(|t| t.font.as_str()),
            ),
            opts,
        ),
        ObjectKind::ProgressMeter {
            min,
            max,
            style,
            show_value,
            ..
        } => {
            let raw = match value {
                Some(Value::Number(number)) => *number,
                _ => *min,
            };
            let fraction = meter_fraction(raw, *min, *max);
            Visual::ProgressMeter {
                fraction,
                style: *style,
                show_value: *show_value,
                percent: (fraction * 100.0).round().clamp(0.0, 100.0) as u8,
            }
        }
        ObjectKind::Audio { src, .. } => Visual::Audio {
            src: state_overrides.src.unwrap_or(src).to_string(),
        },
        ObjectKind::Lottie {
            src,
            loop_playback,
            autoplay: _,
            speed,
        } => Visual::Lottie {
            src: state_overrides.src.unwrap_or(src).to_string(),
            loop_playback: *loop_playback,
            speed: coursecraft_course::clamp_lottie_speed(*speed),
        },
    };

    let visible = !matches!(visual, Visual::Audio { .. });
    Some(DrawItem {
        hit: Hit::Object(object.id.clone()),
        rect,
        visual,
        // A disabled input ignores input and is drawn dimmed everywhere.
        dimmed: disabled && object.kind.input_kind().is_some(),
        opacity: if visible {
            coursecraft_course::clamp_opacity(state_overrides.opacity.unwrap_or(object.opacity))
        } else {
            1.0
        },
        clip: None,
        rotation: if visible {
            coursecraft_course::normalize_rotation(
                state_overrides.rotation.unwrap_or(object.rotation),
            )
        } else {
            0.0
        },
        flip_h: visible && state_overrides.flip_h.unwrap_or(object.flip_h),
        flip_v: visible && state_overrides.flip_v.unwrap_or(object.flip_v),
        shadow: if visible {
            match state_overrides.shadow {
                NullableOverride::Set(s) => resolve_shadow(s),
                NullableOverride::Clear => None,
                NullableOverride::Inherit => object.shadow.as_ref().and_then(resolve_shadow),
            }
        } else {
            None
        },
        glow: if visible {
            match state_overrides.glow {
                NullableOverride::Set(g) => resolve_glow(g),
                NullableOverride::Clear => None,
                NullableOverride::Inherit => object.glow.as_ref().and_then(resolve_glow),
            }
        } else {
            None
        },
        media_time_ms: None,
        links: Vec::new(),
        show_table_overflow: false,
        clip_rect: None,
        mark: None,
        is_state_extra: false,
        anim: AnimLook::IDENTITY,
    })
}

/// Compose an object and any active-state visual extras in draw order
/// (parent first, then state extras).
///
/// Returns an empty `Vec` if the object frame is invalid.
#[must_use]
pub fn compose_object_items(
    object: &Object,
    current_state: &str,
    theme: Option<&coursecraft_course::Theme>,
    value: Option<&Value>,
) -> Vec<DrawItem> {
    compose_object_items_opts(
        object,
        current_state,
        theme,
        value,
        ComposeOpts {
            authoring: true,
            ..ComposeOpts::default()
        },
    )
}

fn compose_object_items_opts(
    object: &Object,
    current_state: &str,
    theme: Option<&coursecraft_course::Theme>,
    value: Option<&Value>,
    opts: ComposeOpts,
) -> Vec<DrawItem> {
    let Some(parent) = compose_object_opts(object, current_state, theme, value, opts) else {
        return Vec::new();
    };
    if matches!(parent.visual, Visual::Audio { .. }) {
        return vec![parent];
    }
    let parent_rect = parent.rect;
    let parent_rot = parent.rotation;
    let parent_flip_h = parent.flip_h;
    let parent_flip_v = parent.flip_v;
    let parent_opacity = parent.opacity;
    let parent_dimmed = parent.dimmed;
    let disabled = current_state == "disabled";

    let mut items = vec![parent];
    if current_state == "hidden" {
        return items;
    }

    let mut push_extras = |extras: &[coursecraft_course::StateExtra]| {
        for extra in extras {
            if let Some(item) = compose_state_extra(
                object,
                extra,
                parent_rect,
                (parent_rot, parent_flip_h, parent_flip_v),
                parent_opacity,
                parent_dimmed,
                disabled,
                theme,
            ) {
                items.push(item);
            }
        }
    };

    if current_state != "normal"
        && let Some(state) = object.states.iter().find(|s| s.name == current_state)
    {
        push_extras(&state.extras);
    }

    if !disabled {
        let pointer_name = match opts.pointer {
            PointerState::Pressed => Some("down"),
            PointerState::Hovered => Some("hover"),
            PointerState::None => None,
        };
        if let Some(name) = pointer_name
            && name != current_state
            && let Some(state) = object.states.iter().find(|s| s.name == name)
        {
            push_extras(&state.extras);
        }
    }

    items
}

#[allow(clippy::too_many_arguments)]
fn compose_state_extra(
    object: &Object,
    extra: &coursecraft_course::StateExtra,
    parent_rect: egui::Rect,
    (parent_rot, parent_flip_h, parent_flip_v): (f32, bool, bool),
    parent_opacity: f32,
    parent_dimmed: bool,
    disabled: bool,
    theme: Option<&coursecraft_course::Theme>,
) -> Option<DrawItem> {
    let rel_rect = validate_frame(&extra.frame)?;
    let unrotated =
        egui::Rect::from_min_size(parent_rect.min + rel_rect.min.to_vec2(), rel_rect.size());
    let parent_center = parent_rect.center();
    let extra_center = flip_point(
        rotate_point(unrotated.center(), parent_center, parent_rot),
        parent_center,
        parent_flip_h,
        parent_flip_v,
    );
    let rect = egui::Rect::from_center_size(extra_center, unrotated.size());

    let flip_sign = if parent_flip_h ^ parent_flip_v {
        -1.0
    } else {
        1.0
    };
    let extra_rot = coursecraft_course::normalize_rotation(extra.rotation);
    let rotation = coursecraft_course::normalize_rotation(parent_rot + flip_sign * extra_rot);
    let flip_h = parent_flip_h ^ extra.flip_h;
    let flip_v = parent_flip_v ^ extra.flip_v;
    let opacity =
        (parent_opacity * coursecraft_course::clamp_opacity(extra.opacity)).clamp(0.0, 1.0);

    let default_btn_color = theme
        .and_then(|t| parse_color(&t.accent))
        .unwrap_or(DEFAULT_BUTTON_COLOR);
    let default_txt_color = theme
        .and_then(|t| parse_color(&t.text))
        .unwrap_or(DEFAULT_TEXT_COLOR);
    let theme_font = theme.map(|t| t.font.as_str());
    let no_overrides = ResolvedStateOverrides::default();

    let visual = match &extra.kind {
        coursecraft_course::StateExtraKind::Shape {
            shape,
            fill,
            fill_style,
            outline,
            text,
            adjust,
        } => {
            let resolved_fill = parse_color(fill).unwrap_or(FALLBACK_GREY);
            let resolved_text = resolve_shape_text(
                *shape,
                text.as_ref(),
                &no_overrides,
                theme,
                (default_txt_color, default_btn_color),
            );
            Visual::Shape {
                shape: *shape,
                adjust: adjust.clone(),
                fill: resolved_fill,
                fill_style: fill_style.as_ref().and_then(resolve_fill_style),
                outline: if !shape_allows_text(*shape) {
                    None
                } else {
                    outline.as_ref().and_then(resolve_outline)
                },
                text: resolved_text,
                hover_fill: None,
                down_fill: None,
                down_has_fill_style: false,
                disabled,
            }
        }
        coursecraft_course::StateExtraKind::Image {
            src,
            alt,
            crop,
            outline,
            shape,
        } => Visual::Image {
            src: src.clone(),
            alt: alt.clone(),
            crop: crop.as_ref().and_then(valid_crop_rect),
            outline: outline.as_ref().and_then(resolve_outline),
            shape: *shape,
        },
        coursecraft_course::StateExtraKind::Text {
            text,
            style,
            runs,
            paragraphs,
            autofit,
            outline,
            point_text: _,
        } => {
            let resolved_runs = resolve_state_text_runs(
                text,
                style,
                runs,
                &no_overrides,
                None,
                (default_txt_color, default_btn_color, theme_font),
            );
            Visual::Text {
                runs: resolved_runs,
                align: style.align,
                valign: style.valign,
                paragraphs: paragraphs.clone(),
                autofit: *autofit,
                outline: outline.as_ref().and_then(resolve_outline),
                wrap: style.wrap,
                insets: style.insets,
                direction: style.direction,
            }
        }
        coursecraft_course::StateExtraKind::Path { path } => {
            let resolved = resolve_path(
                path,
                &no_overrides,
                default_txt_color,
                default_btn_color,
                theme_font,
            )?;
            Visual::Path(resolved)
        }
    };

    Some(DrawItem {
        hit: Hit::Object(object.id.clone()),
        rect,
        visual,
        dimmed: parent_dimmed,
        opacity,
        clip: None,
        rotation,
        flip_h,
        flip_v,
        shadow: extra.shadow.as_ref().and_then(resolve_shadow),
        glow: extra.glow.as_ref().and_then(resolve_glow),
        media_time_ms: None,
        links: Vec::new(),
        show_table_overflow: false,
        clip_rect: None,
        mark: None,
        is_state_extra: true,
        anim: AnimLook::IDENTITY,
    })
}

/// Row height and stride (height plus gap) so `count` rows fit the grid's height when they can.
fn row_metrics(grid: &QuestionGrid, count: usize) -> (f32, f32) {
    const DEFAULT_H: f32 = 56.0;
    const DEFAULT_GAP: f32 = 16.0;
    const MIN_H: f32 = 28.0;
    const MIN_GAP: f32 = 4.0;
    let n = count.max(1) as f32;
    let available = grid.available;
    let default_stride = DEFAULT_H + DEFAULT_GAP;
    if n * default_stride <= available {
        return (DEFAULT_H, default_stride);
    }
    let min_stride = MIN_H + MIN_GAP;
    if !available.is_finite() || n * min_stride > available {
        return (MIN_H, min_stride);
    }
    let stride = available / n;
    let h = (stride - MIN_GAP).clamp(MIN_H, DEFAULT_H);
    (h, h + (stride - h).max(0.0))
}

fn finite_slide(slide_w: f32, slide_h: f32) -> (f32, f32) {
    let w = if slide_w.is_finite() && slide_w > 0.0 {
        slide_w
    } else {
        1280.0
    };
    let h = if slide_h.is_finite() && slide_h > 0.0 {
        slide_h
    } else {
        720.0
    };
    (w, h)
}

fn row_rect(grid: &QuestionGrid, index: usize, row_h: f32, stride: f32) -> egui::Rect {
    let y = grid.row_top + stride * index as f32;
    egui::Rect::from_min_size(
        egui::pos2(grid.row_x, y),
        egui::vec2(grid.row_w.max(0.0), row_h.max(0.0)),
    )
}

/// Where a question's prompt and answer rows go, in slide coordinates.
///
/// Without a frame this is the original full-width layout: the prompt at the top, the rows
/// from y = 160 down to 24 px above the slide's bottom edge. With a frame the prompt sits at
/// the frame's top, the rows follow it, and everything stays inside the frame.
#[derive(Debug, Clone, Copy)]
struct QuestionGrid {
    prompt: egui::Rect,
    row_x: f32,
    row_top: f32,
    row_w: f32,
    /// The lowest y a row may reach.
    bottom: f32,
    /// Height the rows may use (`bottom - row_top`).
    available: f32,
}

/// A question frame the layout can use: finite, with a positive size.
fn usable_question_frame(frame: Option<&coursecraft_course::Rect>) -> Option<egui::Rect> {
    let f = frame?;
    let ok = [f.x, f.y, f.w, f.h].iter().all(|v| v.is_finite()) && f.w > 0.0 && f.h > 0.0;
    ok.then(|| egui::Rect::from_min_size(egui::pos2(f.x, f.y), egui::vec2(f.w, f.h)))
}

fn question_grid(
    slide_w: f32,
    slide_h: f32,
    frame: Option<&coursecraft_course::Rect>,
) -> QuestionGrid {
    let (slide_w, slide_h) = finite_slide(slide_w, slide_h);
    let Some(frame) = usable_question_frame(frame) else {
        let prompt = question_prompt_rect(slide_w, slide_h);
        let bottom = slide_h - 24.0;
        return QuestionGrid {
            prompt,
            row_x: 80.0,
            row_top: 160.0,
            row_w: 900.0f32.min(prompt.width()),
            bottom,
            available: (bottom - 160.0).max(0.0),
        };
    };
    let prompt_h = (frame.height() * 0.3).clamp(0.0, 80.0);
    let prompt = egui::Rect::from_min_size(frame.min, egui::vec2(frame.width(), prompt_h));
    let row_top = (prompt.max.y + 20.0).min(frame.max.y);
    QuestionGrid {
        prompt,
        row_x: frame.min.x,
        row_top,
        row_w: 900.0f32.min(frame.width()),
        bottom: frame.max.y,
        available: (frame.max.y - row_top).max(0.0),
    }
}

/// The rect the question prompt occupies, honouring the question's own frame.
///
/// Like [`question_prompt_rect`], this is the one layout function for the prompt. A question
/// without a frame gives the same rect as [`question_prompt_rect`].
#[must_use]
pub fn question_prompt_rect_in(
    slide_w: f32,
    slide_h: f32,
    frame: Option<&coursecraft_course::Rect>,
) -> egui::Rect {
    question_grid(slide_w, slide_h, frame).prompt
}

fn sequence_button_rects(row: egui::Rect) -> (egui::Rect, egui::Rect) {
    let side = row.height().clamp(0.0, 28.0);
    let y = row.min.y + (row.height() - side) * 0.5;
    let down_x = (row.max.x - 8.0 - side).max(row.min.x);
    let up_x = (down_x - 4.0 - side).max(row.min.x);
    let up = egui::Rect::from_min_size(egui::pos2(up_x, y), egui::vec2(side, side));
    let down = egui::Rect::from_min_size(egui::pos2(down_x, y), egui::vec2(side, side));
    (up, down)
}

fn is_permutation(order: &[String], authored: &[String]) -> bool {
    order.len() == authored.len()
        && authored.iter().all(|id| order.contains(id))
        && order.iter().all(|id| authored.contains(id))
}

struct QuestionCompose<'a> {
    slide_w: f32,
    slide_h: f32,
    question: &'a Question,
    selected: &'a BTreeSet<String>,
    sequence: &'a [String],
    text_entry: &'a str,
    matches: &'a BTreeMap<String, String>,
    fill_placeholder: &'a str,
    match_placeholder: &'a str,
    /// Whether the answer review marks apply to this question.
    reviewing: bool,
    /// Whether the graded answer was correct, while reviewing.
    answer_correct: Option<bool>,
    /// Drag item → dropped target, while reviewing.
    drops: Option<&'a BTreeMap<String, String>>,
    /// `correctAnswer` chrome string, filled per row.
    correct_answer: &'a str,
    dimmed: bool,
}

fn push_item(items: &mut Vec<DrawItem>, hit: Hit, rect: egui::Rect, visual: Visual, dimmed: bool) {
    items.push(DrawItem {
        hit,
        rect,
        visual,
        dimmed,
        opacity: 1.0,
        clip: None,
        rotation: 0.0,
        flip_h: false,
        flip_v: false,
        shadow: None,
        glow: None,
        media_time_ms: None,
        links: Vec::new(),
        show_table_overflow: false,
        clip_rect: None,
        mark: None,
        is_state_extra: false,
        anim: AnimLook::IDENTITY,
    });
}

/// The rect the question prompt occupies in slide coordinates.
///
/// This is the one layout function for the prompt: the renderer and the editor's
/// overlap warning both use it, so they can never disagree.
#[must_use]
pub fn question_prompt_rect(slide_w: f32, slide_h: f32) -> egui::Rect {
    let (slide_w, _slide_h) = finite_slide(slide_w, slide_h);
    let margin = if slide_w < 320.0 {
        (slide_w * 0.1).min(80.0)
    } else {
        80.0
    };
    let prompt_w = (slide_w - 2.0 * margin).max(0.0);
    egui::Rect::from_min_size(egui::pos2(margin, 60.0), egui::vec2(prompt_w, 80.0))
}

fn compose_question(view: &QuestionCompose<'_>, items: &mut Vec<DrawItem>) {
    let (slide_w, slide_h) = finite_slide(view.slide_w, view.slide_h);
    let question = view.question;
    let grid = question_grid(slide_w, slide_h, question.frame.as_ref());
    let prompt_rect = grid.prompt;
    if question.kind != QuestionKind::Freeform || !question.prompt.is_empty() {
        push_item(
            items,
            Hit::None,
            prompt_rect,
            Visual::Prompt {
                text: question.prompt.clone(),
            },
            view.dimmed,
        );
    }

    match question.kind {
        QuestionKind::MultipleChoice
        | QuestionKind::MultipleResponse
        | QuestionKind::TrueFalse
        | QuestionKind::Likert => {
            let (row_h, stride) = row_metrics(&grid, question.choices.len());
            let multiple = question.kind == QuestionKind::MultipleResponse;
            let likert = question.kind == QuestionKind::Likert;
            for (i, choice) in question.choices.iter().enumerate() {
                let selected = view.selected.contains(&choice.id);
                let mark = (view.reviewing && !likert)
                    .then_some(if selected {
                        if choice.correct {
                            ReviewMark::Correct
                        } else {
                            ReviewMark::Incorrect
                        }
                    } else {
                        ReviewMark::MissedCorrect
                    })
                    .filter(|_| selected || choice.correct);
                push_item(
                    items,
                    if view.reviewing {
                        Hit::QuestionChrome(choice.id.clone())
                    } else {
                        Hit::Choice(choice.id.clone())
                    },
                    row_rect(&grid, i, row_h, stride),
                    Visual::ChoiceRow {
                        text: choice.text.clone(),
                        multiple,
                        selected,
                        mark,
                    },
                    view.dimmed,
                );
            }
        }
        QuestionKind::FillIn | QuestionKind::ShortAnswer | QuestionKind::Essay => {
            let (row_h, stride) = row_metrics(&grid, 1);
            let multiline = question.kind == QuestionKind::Essay;
            let mut rect = row_rect(&grid, 0, row_h, stride);
            if multiline {
                let height = (row_h * 4.0).min((grid.bottom - rect.min.y).max(0.0));
                rect = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), height));
            }
            let mark = if view.reviewing && question.kind == QuestionKind::FillIn {
                view.answer_correct.map(|correct| {
                    if correct {
                        ReviewMark::Correct
                    } else {
                        ReviewMark::Incorrect
                    }
                })
            } else {
                None
            };
            push_item(
                items,
                Hit::QuestionChrome("fill-in".to_owned()),
                rect,
                Visual::FillIn {
                    text: view.text_entry.to_owned(),
                    placeholder: view.fill_placeholder.to_owned(),
                    multiline,
                    mark,
                },
                view.dimmed,
            );
            if mark == Some(ReviewMark::Incorrect)
                && let Some(accepted) = question.accepted.first()
            {
                let text =
                    coursecraft_course::fill(view.correct_answer, &[("answer", accepted.as_str())]);
                push_item(
                    items,
                    Hit::None,
                    correct_answer_rect(rect, row_h),
                    Visual::CorrectAnswer { text },
                    view.dimmed,
                );
            }
        }
        QuestionKind::Sequence => {
            let authored: Vec<String> = question.choices.iter().map(|c| c.id.clone()).collect();
            let order: Vec<String> = if is_permutation(view.sequence, &authored) {
                view.sequence.to_vec()
            } else {
                authored
            };
            let (row_h, stride) = row_metrics(&grid, order.len());
            let last = order.len().saturating_sub(1);
            for (i, id) in order.iter().enumerate() {
                let text = question
                    .choices
                    .iter()
                    .find(|c| c.id == *id)
                    .map(|c| c.text.clone())
                    .unwrap_or_default();
                let rect = row_rect(&grid, i, row_h, stride);
                let mark = view.reviewing.then(|| {
                    if view
                        .sequence
                        .get(i)
                        .zip(question.choices.get(i))
                        .is_some_and(|(chosen, choice)| *chosen == choice.id)
                    {
                        ReviewMark::Correct
                    } else {
                        ReviewMark::Incorrect
                    }
                });
                push_item(
                    items,
                    Hit::QuestionChrome(format!("seq-row-{id}")),
                    rect,
                    Visual::SequenceRow { text, mark },
                    view.dimmed,
                );
                if view.reviewing {
                    continue;
                }
                let (up, down) = sequence_button_rects(rect);
                let can_up = i > 0;
                let can_down = i < last;
                push_item(
                    items,
                    if can_up {
                        Hit::SequenceMove {
                            choice: id.clone(),
                            to: i - 1,
                        }
                    } else {
                        Hit::QuestionChrome(format!("seq-up-{id}"))
                    },
                    up,
                    Visual::SequenceButton {
                        up: true,
                        enabled: can_up,
                    },
                    view.dimmed,
                );
                push_item(
                    items,
                    if can_down {
                        Hit::SequenceMove {
                            choice: id.clone(),
                            to: i + 1,
                        }
                    } else {
                        Hit::QuestionChrome(format!("seq-down-{id}"))
                    },
                    down,
                    Visual::SequenceButton {
                        up: false,
                        enabled: can_down,
                    },
                    view.dimmed,
                );
            }
        }
        QuestionKind::Matching => {
            let (row_h, stride) = row_metrics(&grid, question.pairs.len());
            for (i, pair) in question.pairs.iter().enumerate() {
                let value = view
                    .matches
                    .get(&pair.id)
                    .and_then(|right| question.pairs.iter().find(|p| p.id == *right))
                    .map(|p| p.right.clone())
                    .unwrap_or_else(|| view.match_placeholder.to_owned());
                let mark = view.reviewing.then(|| {
                    if view.matches.get(&pair.id) == Some(&pair.id) {
                        ReviewMark::Correct
                    } else {
                        ReviewMark::Incorrect
                    }
                });
                let rect = row_rect(&grid, i, row_h, stride);
                push_item(
                    items,
                    Hit::QuestionChrome(pair.id.clone()),
                    rect,
                    Visual::MatchRow {
                        id: pair.id.clone(),
                        left: pair.left.clone(),
                        value,
                        mark,
                    },
                    view.dimmed,
                );
                if mark == Some(ReviewMark::Incorrect) {
                    let text = coursecraft_course::fill(
                        view.correct_answer,
                        &[("answer", pair.right.as_str())],
                    );
                    push_item(
                        items,
                        Hit::None,
                        correct_answer_rect(rect, row_h),
                        Visual::CorrectAnswer { text },
                        view.dimmed,
                    );
                }
            }
        }
        QuestionKind::Hotspot => push_hotspot_outlines(view, items),
        QuestionKind::DragDrop => mark_drag_items(view, items),
        QuestionKind::Freeform => {}
    }
}

/// Outline every correct hotspot region while reviewing.
fn push_hotspot_outlines(view: &QuestionCompose<'_>, items: &mut Vec<DrawItem>) {
    if !view.reviewing {
        return;
    }
    for region in &view.question.regions {
        if !region.correct
            || !region.x.is_finite()
            || !region.y.is_finite()
            || !region.w.is_finite()
            || !region.h.is_finite()
            || region.w <= 0.0
            || region.h <= 0.0
        {
            continue;
        }
        push_item(
            items,
            Hit::None,
            egui::Rect::from_min_size(
                egui::pos2(region.x, region.y),
                egui::vec2(region.w, region.h),
            ),
            Visual::RegionOutline {
                ellipse: matches!(region.shape, coursecraft_course::HotspotShape::Ellipse),
            },
            view.dimmed,
        );
    }
}

/// Mark each drag item against its correct target while reviewing.
fn mark_drag_items(view: &QuestionCompose<'_>, items: &mut [DrawItem]) {
    let Some(drops) = view.drops else {
        return;
    };
    for drag in &view.question.drag_items {
        let mark = if drops.get(&drag.item) == Some(&drag.target) {
            ReviewMark::Correct
        } else {
            ReviewMark::Incorrect
        };
        for item in items.iter_mut() {
            if !item.is_state_extra && matches!(&item.hit, Hit::Object(id) if id == &drag.item) {
                item.mark = Some(mark);
            }
        }
    }
}

/// The rect of the accepted-answer line written directly under a wrong row.
fn correct_answer_rect(rect: egui::Rect, row_h: f32) -> egui::Rect {
    egui::Rect::from_min_size(
        egui::pos2(rect.min.x, rect.max.y),
        egui::vec2(rect.width(), (row_h * 0.6).max(0.0)),
    )
}

/// Clamped `0..=1` fill of a progress meter: values outside the range clamp.
fn meter_fraction(value: f64, min: f64, max: f64) -> f32 {
    if !value.is_finite() || !min.is_finite() || !max.is_finite() || max <= min {
        return 0.0;
    }
    let fraction = (value - min) / (max - min);
    if fraction.is_finite() {
        fraction.clamp(0.0, 1.0) as f32
    } else {
        0.0
    }
}

/// The value an input object reads from the running player, if it has one.
fn player_input_value<'a>(player: &'a Player<'_>, obj: &Object) -> Option<&'a Value> {
    obj.kind
        .input_variable()
        .and_then(|(name, _)| player.variable(name))
}

/// The value an input object reads on the authoring canvas: the variable's
/// default, or `None` when the course has no such variable yet.
fn course_input_value<'a>(course: &'a Course, obj: &Object) -> Option<&'a Value> {
    obj.kind
        .input_variable()
        .and_then(|(name, _)| course.variable(name).map(|def| &def.value))
}

/// Push a slide's master objects, under everything. Master objects never take
/// a press. In authoring they dim with the base objects; in play they ignore
/// timing and animation and are shown unless their initial state is hidden.
fn push_master_items(
    course: &Course,
    player: Option<&Player<'_>>,
    slide: &Slide,
    theme: Option<&coursecraft_course::Theme>,
    authoring: bool,
    dimmed: bool,
    items: &mut Vec<DrawItem>,
) {
    let Some(master) = slide.master.as_deref().and_then(|id| course.master(id)) else {
        return;
    };
    for obj in &master.objects {
        if !authoring && obj.initial_state == "hidden" {
            continue;
        }
        let Some(mut item) = (if authoring {
            compose_object_with_value(
                obj,
                &obj.initial_state,
                theme,
                course_input_value(course, obj),
            )
        } else {
            // At play time read the live variable, not its authored default.
            let value = match player {
                Some(player) => player_input_value(player, obj),
                None => course_input_value(course, obj),
            };
            compose_object_with_runtime(obj, &obj.initial_state, theme, value, false)
        }) else {
            continue;
        };
        if matches!(item.visual, Visual::Audio { .. }) {
            continue;
        }
        item.hit = Hit::None;
        item.dimmed = dimmed || (authoring && obj.initial_state == "hidden");
        items.push(item);
    }
}

/// Push the base layer's objects, unless a visible layer hides them.
fn push_base_items(
    player: &Player<'_>,
    slide: &Slide,
    base_hidden: bool,
    hovered: Option<&str>,
    pressed: Option<&str>,
    items: &mut Vec<DrawItem>,
    filled: &mut Vec<std::ops::Range<usize>>,
) {
    if base_hidden {
        return;
    }
    let theme = Some(&player.course().theme);
    let base_clock_ms = player.state().base.ms;
    for obj in &slide.objects {
        let current_state = player.object_state(&obj.id).unwrap_or(&obj.initial_state);
        if current_state == "hidden" {
            continue;
        }
        if !is_visible_at(&obj.timing, base_clock_ms) {
            continue;
        }
        let Some(sample) = player.sample_object(&obj.id) else {
            continue;
        };
        if sample.opacity <= 0.0 {
            continue;
        }
        let pointer_id = player
            .interactive_group_of(&obj.id)
            .unwrap_or(obj.id.as_str());
        let pointer = if current_state == "disabled" {
            PointerState::None
        } else {
            pointer_state_for(hovered, pressed, pointer_id)
        };
        let mut obj_items = compose_object_items_opts(
            obj,
            current_state,
            theme,
            player_input_value(player, obj),
            ComposeOpts {
                marker_open: player.marker_open() == Some(obj.id.as_str()),
                marker_pulse: true,
                authoring: false,
                timer_remaining_ms: player.timer_remaining_ms(&obj.id),
                pointer,
            },
        );
        if obj_items
            .first()
            .is_some_and(|item| !matches!(item.visual, Visual::Audio { .. }))
        {
            // Fill in %variables% first: a by-letter or by-word entrance splits the text, and
            // a token split across parts could not be found any more.
            for item in &mut obj_items {
                fill_item_variables(player, item);
            }
            crate::sequence::apply_object_samples(
                &mut obj_items,
                coursecraft_logic::plays_in_parts(obj),
                &sample,
                |n| player.sample_object_parts(&obj.id, n),
            );
            if let Some(parent) = obj_items.first_mut() {
                apply_runtime_media_state(parent, player, &obj.id);
            }
            filled.push(items.len()..items.len() + obj_items.len());
            items.extend(obj_items);
        }
    }
}

/// Apply an animation sample to one object's draw items: the first is the object, the rest
/// (state extras) move, scale and turn with it about its centre.
pub fn apply_sample_to_items(items: &mut [DrawItem], sample: &coursecraft_logic::ObjectSample) {
    let Some((first, rest)) = items.split_first_mut() else {
        return;
    };
    let fin = |v: f32, d: f32| if v.is_finite() { v } else { d };
    let offset = egui::vec2(fin(sample.offset.0, 0.0), fin(sample.offset.1, 0.0));
    let scale = egui::vec2(
        fin(sample.scale.0, 1.0).max(0.0),
        fin(sample.scale.1, 1.0).max(0.0),
    );
    let turn = fin(sample.rotation_deg, 0.0);
    let opacity = fin(sample.opacity, 1.0);

    first.rect = first.rect.translate(offset);
    let parent_center = first.rect.center();
    first.rect = egui::Rect::from_center_size(parent_center, first.rect.size() * scale);
    first.opacity = (first.opacity * opacity).clamp(0.0, 1.0);
    first.clip = sample.clip;
    let mirrored = first.flip_h != first.flip_v;
    // The mesh pass rotates first and mirrors after, so a mirrored item needs the
    // opposite sign to turn clockwise on screen, like its orbiting state extras.
    first.rotation += if mirrored { -turn } else { turn };
    let look = AnimLook {
        scale: (scale.x, scale.y),
        tint: sample.tint,
        mask: sample.mask.clone().map(|mask| PlacedMask {
            mask,
            frame: first.rect,
            rotation: if mirrored {
                -first.rotation
            } else {
                first.rotation
            },
        }),
    };
    first.anim = look.clone();

    for extra in rest {
        extra.rect = extra.rect.translate(offset);
        let rel_center = (extra.rect.center() - parent_center) * scale;
        let new_center = rotate_point(parent_center + rel_center, parent_center, turn);
        extra.rect = egui::Rect::from_center_size(new_center, extra.rect.size() * scale);
        extra.opacity = (extra.opacity * opacity).clamp(0.0, 1.0);
        extra.clip = sample.clip;
        extra.rotation += turn;
        extra.anim = look.clone();
    }
}

/// Compose the question prompt and response rows. A question belongs to the
/// base layer, so a visible `hide_base_objects` layer hides it too.
fn push_question_items(
    player: &Player<'_>,
    slide: &Slide,
    base_hidden: bool,
    items: &mut Vec<DrawItem>,
) {
    let Some(question) = slide.question.as_ref() else {
        return;
    };
    if base_hidden {
        return;
    }
    let reviewing = player.is_reviewing() && player.state().answers.contains_key(&slide.id);
    compose_question(
        &QuestionCompose {
            slide_w: player.course().slide_size.w as f32,
            slide_h: player.course().slide_size.h as f32,
            question,
            selected: &player.state().selected,
            sequence: player.sequence_order(),
            text_entry: player.text_entry(),
            matches: &player.state().matches,
            fill_placeholder: player.course().chrome_string("fillInPlaceholder"),
            match_placeholder: player.course().chrome_string("choosePlaceholder"),
            reviewing,
            answer_correct: player.state().answers.get(&slide.id).map(|a| a.correct),
            drops: reviewing.then_some(&player.state().drops),
            correct_answer: player.course().chrome_string("correctAnswer"),
            dimmed: false,
        },
        items,
    );
}

/// Compose the draw items for the current slide of `player` in bottom-to-top order.
///
/// Order:
/// 1. Base objects (bottom to top).
/// 2. Question prompt and choices (if a question exists on the slide).
/// 3. Objects on each visible layer, in layer order and bottom to top within each layer.
#[must_use]
pub fn compose(player: &Player<'_>) -> Vec<DrawItem> {
    compose_with_pointer(player, None, None)
}

/// Compose the draw items for the current slide of `player` in bottom-to-top
/// order, applying presentational `hover` and `down` state overrides for the
/// given hovered/pressed object ids.
#[must_use]
pub fn compose_with_pointer(
    player: &Player<'_>,
    hovered: Option<&str>,
    pressed: Option<&str>,
) -> Vec<DrawItem> {
    let Some(slide) = player.current_slide() else {
        return Vec::new();
    };
    let hovered = hovered.map(|id| player.interactive_group_of(id).unwrap_or(id));
    let pressed = pressed.map(|id| player.interactive_group_of(id).unwrap_or(id));

    let mut items = Vec::new();
    let base_hidden = player.base_objects_hidden();

    // 1. Master objects, under everything. They ignore timing and animation,
    // are always shown except when their initial state is `hidden`, and never
    // take a press.
    let theme = Some(&player.course().theme);
    push_master_items(
        player.course(),
        Some(player),
        slide,
        theme,
        false,
        false,
        &mut items,
    );

    // 2. Base objects
    let mut filled: Vec<std::ops::Range<usize>> = Vec::new();
    push_base_items(
        player,
        slide,
        base_hidden,
        hovered,
        pressed,
        &mut items,
        &mut filled,
    );

    // Drag items sit above every other base object (drop targets included), so a
    // dropped item is never hidden behind the target it landed on. When a target's
    // label would be covered by a dropped item, the label and its soft backing pill
    // move just outside the target instead — directly above the target's frame,
    // centred on it and clamped inside the slide, or directly below when there is
    // no room above — so both labels stay fully readable. The label moves only
    // while a drag item overlaps it, so an empty target keeps its label where the
    // author put it. Drag items draw above everything, so an item still wins
    // wherever it happens to overlap a moved label.
    if !base_hidden
        && let Some(q) = &slide.question
        && q.kind == coursecraft_course::QuestionKind::DragDrop
    {
        let (slide_w, slide_h) = finite_slide(
            player.course().slide_size.w as f32,
            player.course().slide_size.h as f32,
        );
        let target_rects: Vec<egui::Rect> = q
            .drop_targets
            .iter()
            .filter_map(|id| slide.objects.iter().find(|o| &o.id == id))
            .filter_map(|o| validate_frame(&o.frame))
            .collect();
        let is_drag_item = |item: &DrawItem| matches!(&item.hit, Hit::Object(id) | Hit::DragItem(id) if q.drag_items.iter().any(|d| &d.item == id));
        let drag_rects: Vec<egui::Rect> = items
            .iter()
            .filter(|item| is_drag_item(item))
            .map(|item| item.rect)
            .collect();
        let is_target_label = |item: &DrawItem| {
            !item.is_state_extra
                && matches!(item.hit, Hit::Object(_))
                && matches!(item.visual, Visual::Text { .. })
                && target_rects
                    .iter()
                    .any(|target| target.intersects(item.rect))
                && drag_rects.iter().any(|drag| drag.intersects(item.rect))
        };
        let (drag, rest): (Vec<_>, Vec<_>) = items.into_iter().partition(|i| is_drag_item(i));
        let (labels, others): (Vec<_>, Vec<_>) = rest.into_iter().partition(|i| is_target_label(i));
        items = others;
        for mut label in labels {
            if let Some(pill_rect) = target_rects
                .iter()
                .find(|target| target.intersects(label.rect))
                .and_then(|target| moved_label_rect(label.rect, *target, slide_w, slide_h))
            {
                // While it is out of the way the label is chrome: the item it was
                // covering keeps the press.
                label.hit = Hit::None;
                label.rect = pill_rect.shrink2(LABEL_PAD);
            }
            if let Visual::Text { runs, .. } = &mut label.visual {
                for run in runs {
                    run.color = egui::Color32::from_rgb(0x1D, 0x1D, 0x1F);
                }
            }
            let clip = label.clip;
            let pill = DrawItem {
                hit: Hit::None,
                rect: label.rect.expand2(LABEL_PAD),
                visual: Visual::Shape {
                    shape: coursecraft_course::ShapeKind::RoundedRectangle,
                    adjust: Vec::new(),
                    fill: egui::Color32::from_rgba_unmultiplied(255, 255, 255, 217),
                    fill_style: None,
                    outline: None,
                    text: None,
                    hover_fill: None,
                    down_fill: None,
                    down_has_fill_style: false,
                    disabled: true,
                },
                dimmed: label.dimmed,
                opacity: label.opacity,
                clip,
                rotation: 0.0,
                flip_h: false,
                flip_v: false,
                shadow: None,
                glow: None,
                media_time_ms: None,
                links: Vec::new(),
                show_table_overflow: false,
                clip_rect: None,
                mark: None,
                is_state_extra: false,
                anim: AnimLook::IDENTITY,
            };
            items.push(pill);
            items.push(label);
        }
        items.extend(drag);
    }

    // 2. Question prompt + response rows
    push_question_items(player, slide, base_hidden, &mut items);

    // 3. Visible layers in order
    let layers_start = items.len();
    for layer_id in &player.state().visible_layers {
        if let Some(layer) = slide.layer(layer_id) {
            let layer_clock_ms = player
                .state()
                .layer_clocks
                .get(layer_id)
                .map_or(0, |c| c.ms);
            for obj in &layer.objects {
                let current_state = player.object_state(&obj.id).unwrap_or(&obj.initial_state);
                if current_state == "hidden" {
                    continue;
                }
                if !is_visible_at(&obj.timing, layer_clock_ms) {
                    continue;
                }
                let Some(sample) = player.sample_object(&obj.id) else {
                    continue;
                };
                if sample.opacity <= 0.0 {
                    continue;
                }
                let pointer_id = player
                    .interactive_group_of(&obj.id)
                    .unwrap_or(obj.id.as_str());
                let pointer = if current_state == "disabled" {
                    PointerState::None
                } else {
                    pointer_state_for(hovered, pressed, pointer_id)
                };
                let mut obj_items = compose_object_items_opts(
                    obj,
                    current_state,
                    theme,
                    player_input_value(player, obj),
                    ComposeOpts {
                        marker_open: player.marker_open() == Some(obj.id.as_str()),
                        marker_pulse: true,
                        authoring: false,
                        timer_remaining_ms: player.timer_remaining_ms(&obj.id),
                        pointer,
                    },
                );
                if obj_items
                    .first()
                    .is_some_and(|item| !matches!(item.visual, Visual::Audio { .. }))
                {
                    for item in &mut obj_items {
                        fill_item_variables(player, item);
                    }
                    crate::sequence::apply_object_samples(
                        &mut obj_items,
                        coursecraft_logic::plays_in_parts(obj),
                        &sample,
                        |n| player.sample_object_parts(&obj.id, n),
                    );
                    if let Some(parent) = obj_items.first_mut() {
                        apply_runtime_media_state(parent, player, &obj.id);
                    }
                    filled.push(items.len()..items.len() + obj_items.len());
                    items.extend(obj_items);
                }
            }
        }
    }

    for (at, item) in items.iter_mut().enumerate() {
        // Objects' items were filled in before their animation sequence split them.
        if !filled.iter().any(|range| range.contains(&at)) {
            fill_item_variables(player, item);
        }
    }

    retarget_question_hits(player, &mut items);
    retarget_review_hits(player, &mut items);
    if !base_hidden {
        push_hotspot_markers(player, &mut items);
    }
    apply_panels(player, slide, &mut items, layers_start);
    remap_interactive_group_hits(player, &mut items);
    items
}

/// Replace `%Name%` tokens in every text a draw item carries with the variable's value.
fn fill_item_variables(player: &Player<'_>, item: &mut DrawItem) {
    match &mut item.visual {
        Visual::Text { runs, .. } => {
            for run in runs {
                run.text = player.display_text(&run.text);
            }
        }
        Visual::Button { label, .. } => *label = player.display_text(label),
        Visual::Shape {
            text: Some(text), ..
        } => {
            for run in &mut text.runs {
                run.text = player.display_text(&run.text);
            }
        }
        Visual::Path(path) => {
            if let Some(text) = &mut path.text {
                for run in &mut text.runs {
                    run.text = player.display_text(&run.text);
                }
            }
        }
        Visual::Table(table) => {
            for row in &mut table.rows {
                for cell in &mut row.cells {
                    for run in &mut cell.text.runs {
                        run.text = player.display_text(&run.text);
                    }
                }
            }
        }
        Visual::Prompt { text } => *text = player.display_text(text),
        Visual::ChoiceRow { text, .. } => *text = player.display_text(text),
        Visual::SequenceRow { text, .. } => *text = player.display_text(text),
        Visual::CorrectAnswer { text } => *text = player.display_text(text),
        Visual::Marker { title, body, .. } => {
            *title = player.display_text(title);
            for run in body {
                run.text = player.display_text(&run.text);
            }
        }
        Visual::MatchRow { left, value, .. } => {
            *left = player.display_text(left);
            *value = player.display_text(value);
        }
        // Fill-in keeps the learner's raw entry. Sequence buttons have no text.
        _ => {}
    }
}

/// Remap member hits to their owning interactive group id so pointer hover,
/// press, click, and highlight treat all members as one hit target.
fn remap_interactive_group_hits(player: &Player<'_>, items: &mut [DrawItem]) {
    for item in items {
        if let Hit::Object(member_id) = &item.hit
            && let Some(group_id) = player.interactive_group_of(member_id)
        {
            item.hit = Hit::Object(group_id.to_owned());
        }
    }
}

/// Shift and clip scrolling-panel children, update panel scrollbars, and put
/// open markers above everything else.
///
/// `layers_start` is the index of the first layer item: an open base marker draws
/// above the base objects but below the layers.
fn apply_panels(
    player: &Player<'_>,
    slide: &Slide,
    items: &mut Vec<DrawItem>,
    layers_start: usize,
) {
    for item in items.iter_mut() {
        let Hit::Object(id) = &item.hit else {
            continue;
        };
        let Some((object, _)) = slide.object(id) else {
            continue;
        };
        let Some(panel_id) = object.panel.as_deref() else {
            continue;
        };
        let Some((panel, _)) = slide.object(panel_id) else {
            continue;
        };
        let Some(panel_rect) = validate_frame(&panel.frame) else {
            continue;
        };
        let offset = player.panel_offset(panel_id);
        if offset.abs() > f32::EPSILON {
            item.rect = item.rect.translate(egui::vec2(0.0, -offset));
            // A shaped reveal is placed on the frame, so it scrolls with it.
            if let Some(mask) = &mut item.anim.mask {
                mask.frame = mask.frame.translate(egui::vec2(0.0, -offset));
            }
        }
        item.clip_rect = Some(match item.clip_rect {
            Some(existing) => existing.intersect(panel_rect),
            None => panel_rect,
        });
    }

    for item in items.iter_mut() {
        let Hit::Object(id) = &item.hit else {
            continue;
        };
        let Some((object, _)) = slide.object(id) else {
            continue;
        };
        let ObjectKind::ScrollPanel { .. } = object.kind else {
            continue;
        };
        let offset = player.panel_offset(id);
        if let Visual::ScrollPanel {
            content_height,
            bar,
            offset: visual_offset,
            thumb,
            authoring,
            ..
        } = &mut item.visual
        {
            *visual_offset = offset;
            *thumb = if *authoring {
                None
            } else {
                scroll_panel_thumb(item.rect, *content_height, offset, *bar)
            };
        }
    }

    // Open marker labels draw above every other object of their own layer: a base
    // marker stays below the layers, a layer's marker above that layer.
    let (mut head, mut base_open, mut tail, mut layer_open) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for (at, item) in items.drain(..).enumerate() {
        let open = matches!(&item.visual, Visual::Marker { open: true, .. });
        match (at < layers_start, open) {
            (true, true) => base_open.push(item),
            (true, false) => head.push(item),
            (false, true) => layer_open.push(item),
            (false, false) => tail.push(item),
        }
    }
    head.append(&mut base_open);
    head.append(&mut tail);
    head.append(&mut layer_open);
    *items = head;

    // A panel's scrollbar is chrome: draw a copy right above the panel's last child
    // so the children cannot cover it, and whatever lies above the panel (a layer,
    // say) still covers it. The copy takes no hits; the panel item behind it keeps
    // the wheel and drag targets.
    let mut scrollbars: Vec<(usize, DrawItem)> = Vec::new();
    for (at, item) in items.iter().enumerate() {
        let (Visual::ScrollPanel { thumb: Some(_), .. }, Hit::Object(panel_id)) =
            (&item.visual, &item.hit)
        else {
            continue;
        };
        let last_child = items
            .iter()
            .enumerate()
            .rev()
            .find(|(_, other)| {
                matches!(&other.hit, Hit::Object(id)
                    if slide.object(id).is_some_and(|(object, _)| object.panel.as_deref() == Some(panel_id.as_str())))
            })
            .map_or(at, |(index, _)| index.max(at));
        let mut chrome = item.clone();
        chrome.hit = Hit::None;
        scrollbars.push((last_child + 1, chrome));
    }
    // Insert from the back so earlier positions stay valid.
    for (position, chrome) in scrollbars.into_iter().rev() {
        items.insert(position.min(items.len()), chrome);
    }
}

/// The pill rect for a target label moved out of its target's way: directly above
/// the target's frame, centred on it and clamped inside the slide, or directly
/// below when there is no room above. Returns `None` for hostile geometry, which
/// leaves the label where the author put it.
fn moved_label_rect(
    label: egui::Rect,
    target: egui::Rect,
    slide_w: f32,
    slide_h: f32,
) -> Option<egui::Rect> {
    const GAP: f32 = 4.0;
    if !slide_w.is_finite() || !slide_h.is_finite() {
        return None;
    }
    let size = label.size() + LABEL_PAD * 2.0;
    if !size.x.is_finite() || !size.y.is_finite() || size.x <= 0.0 || size.y <= 0.0 {
        return None;
    }
    let centre_x = target.center().x;
    if !centre_x.is_finite() {
        return None;
    }
    let above = target.top() - GAP - size.y;
    let below = target.bottom() + GAP;
    let top = if above.is_finite() && above >= 0.0 {
        above
    } else {
        below
    };
    let max_x = (slide_w - size.x).max(0.0);
    let max_y = (slide_h - size.y).max(0.0);
    let x = coursecraft_course::safe_clamp(centre_x - size.x * 0.5, 0.0, max_x);
    Some(egui::Rect::from_min_size(
        egui::pos2(x, coursecraft_course::safe_clamp(top, 0.0, max_y)),
        size,
    ))
}

fn question_response_locked(player: &Player<'_>, slide: &Slide, question: &Question) -> bool {
    player.state().answers.get(&slide.id).is_some_and(|answer| {
        answer.correct || (question.attempts != 0 && answer.attempts >= question.attempts)
    })
}

/// While an unlocked hotspot or drag-and-drop question is showing, retarget the
/// image and the drag items so the pointer starts a marker or a drag instead of a click.
fn retarget_question_hits(player: &Player<'_>, items: &mut [DrawItem]) {
    let Some(slide) = player.current_slide() else {
        return;
    };
    let Some(question) = slide.question.as_ref() else {
        return;
    };
    if question_response_locked(player, slide, question) {
        return;
    }
    match question.kind {
        QuestionKind::Hotspot => {
            let Some(id) = question.hotspot_image.clone() else {
                return;
            };
            for item in items {
                if item.hit == Hit::Object(id.clone()) {
                    item.hit = Hit::HotspotImage;
                }
            }
        }
        QuestionKind::DragDrop => {
            let ids: Vec<&str> = question
                .drag_items
                .iter()
                .map(|d| d.item.as_str())
                .collect();
            for item in items.iter_mut() {
                let next = match &item.hit {
                    Hit::Object(id) if ids.contains(&id.as_str()) => Some(id.clone()),
                    _ => None,
                };
                if let Some(id) = next {
                    item.hit = Hit::DragItem(id);
                }
            }
        }
        _ => {}
    }
}

/// While reviewing, nothing in a question takes a real interaction: every
/// response hit becomes chrome so a click sends no input.
fn retarget_review_hits(player: &Player<'_>, items: &mut [DrawItem]) {
    if !player.is_reviewing() {
        return;
    }
    let question = player.current_slide().and_then(|s| s.question.as_ref());
    let hotspot_image = question.and_then(|q| q.hotspot_image.clone());
    let drag_items: Vec<String> = question
        .map(|q| q.drag_items.iter().map(|d| d.item.clone()).collect())
        .unwrap_or_default();
    for item in items {
        item.hit = match &item.hit {
            Hit::Choice(id) | Hit::DragItem(id) => Hit::QuestionChrome(id.clone()),
            Hit::SequenceMove { choice, .. } => Hit::QuestionChrome(choice.clone()),
            Hit::HotspotImage => Hit::QuestionChrome("hotspot".to_owned()),
            Hit::Object(id) if Some(id) == hotspot_image.as_ref() => {
                Hit::QuestionChrome("hotspot".to_owned())
            }
            Hit::Object(id) if drag_items.contains(id) => Hit::QuestionChrome(id.clone()),
            other => other.clone(),
        };
    }
}

fn push_hotspot_markers(player: &Player<'_>, items: &mut Vec<DrawItem>) {
    let Some(slide) = player.current_slide() else {
        return;
    };
    let Some(question) = slide.question.as_ref() else {
        return;
    };
    if question.kind != QuestionKind::Hotspot {
        return;
    }
    for (x, y) in &player.state().hotspot_markers {
        if !x.is_finite() || !y.is_finite() {
            continue;
        }
        push_item(
            items,
            Hit::None,
            egui::Rect::from_center_size(egui::pos2(*x, *y), egui::vec2(24.0, 24.0)),
            Visual::HotspotMarker,
            false,
        );
    }
}

/// Compose the draw items for authoring a slide.
///
/// Unlike [`compose`], this does not require a running [`Player`], ignores object timing,
/// displays objects whose initial state is `"hidden"` (marking them as dimmed), and dims
/// base items when a valid `focus_layer` is provided.
#[must_use]
pub fn compose_authoring(
    course: &Course,
    slide: &Slide,
    focus_layer: Option<&str>,
) -> Vec<DrawItem> {
    compose_authoring_layers(course, slide, focus_layer, &[], true)
}

/// Compose authoring items with extra, unfocused layers visible.
///
/// Every layer draws in authored order (`slide.layers` runs back to front), with
/// the focused layer in its own place. Ids in `extra_layers` draw dimmed and take
/// no hits, so authors can line a layer up against the others without selecting
/// their objects. `dim_others` false leaves the base, master and question
/// undimmed while a layer is focused.
#[must_use]
pub fn compose_authoring_layers(
    course: &Course,
    slide: &Slide,
    focus_layer: Option<&str>,
    extra_layers: &[String],
    dim_others: bool,
) -> Vec<DrawItem> {
    compose_authoring_layers_opts(course, slide, focus_layer, extra_layers, dim_others, true)
}

/// Like [`compose_authoring_layers`], with `show_base` controlling whether the
/// master slide, base objects, question chrome and draw card are included.
#[must_use]
pub fn compose_authoring_layers_opts(
    course: &Course,
    slide: &Slide,
    focus_layer: Option<&str>,
    extra_layers: &[String],
    dim_others: bool,
    show_base: bool,
) -> Vec<DrawItem> {
    let mut items = Vec::new();
    let focused_layer = focus_layer.and_then(|id| slide.layer(id));
    let has_focus_layer = focused_layer.is_some();
    let hide_base = !show_base || focused_layer.is_some_and(|layer| layer.hide_base_objects);
    // With the base hidden there is nothing to dim; masters stay.
    let dim_base = has_focus_layer && dim_others && !hide_base;
    let theme = Some(&course.theme);

    // 1. Master objects, under everything. They dim with the base objects when
    // a layer is focused and never take a press.
    if show_base {
        push_master_items(course, None, slide, theme, true, dim_base, &mut items);
    }

    // 2. Base objects
    if !hide_base {
        for obj in &slide.objects {
            let mut obj_items = compose_object_items(
                obj,
                &obj.initial_state,
                theme,
                course_input_value(course, obj),
            );
            for item in &mut obj_items {
                item.dimmed = item.dimmed || dim_base || obj.initial_state == "hidden";
            }
            items.extend(obj_items);
        }
    }

    // 2. Question prompt + response rows (static; no learner interaction)
    if let Some(ref q) = slide.question
        && !hide_base
    {
        let no_selection = BTreeSet::new();
        let no_matches = BTreeMap::new();
        let authored: Vec<String> = q.choices.iter().map(|c| c.id.clone()).collect();
        compose_question(
            &QuestionCompose {
                slide_w: course.slide_size.w as f32,
                slide_h: course.slide_size.h as f32,
                question: q,
                selected: &no_selection,
                sequence: &authored,
                text_entry: "",
                matches: &no_matches,
                fill_placeholder: course.chrome_string("fillInPlaceholder"),
                match_placeholder: course.chrome_string("choosePlaceholder"),
                reviewing: false,
                answer_correct: None,
                drops: None,
                correct_answer: course.chrome_string("correctAnswer"),
                dimmed: dim_base,
            },
            &mut items,
        );
    }

    // 2b. A draw from a question bank shows a centred card in the authoring
    // view (and in thumbnails, which paint the same items).
    if !hide_base && let Some(draw) = slide.draw.as_ref() {
        let (bank_title, bank_size) = course.bank(&draw.bank).map_or_else(
            || (draw.bank.clone(), 0),
            |bank| (bank.title.clone(), bank.slides.len()),
        );
        let text = format!(
            "Draws {} of {} questions from {}",
            draw.count, bank_size, bank_title
        );
        let (slide_w, slide_h) =
            finite_slide(course.slide_size.w as f32, course.slide_size.h as f32);
        let card_w = (slide_w - 160.0).clamp(0.0, 640.0);
        let card_h = 140.0f32.min((slide_h - 80.0).max(0.0));
        let card_rect = egui::Rect::from_min_size(
            egui::pos2(
                ((slide_w - card_w) * 0.5).max(0.0),
                ((slide_h - card_h) * 0.5).max(0.0),
            ),
            egui::vec2(card_w, card_h),
        );
        push_item(
            &mut items,
            Hit::None,
            card_rect,
            Visual::DrawCard { text },
            dim_base,
        );
    }

    // 3. Layers in authored order, with the focused layer taking hits.
    for layer in &slide.layers {
        let is_focused = focused_layer.is_some_and(|focused| focused.id == layer.id);
        let is_extra =
            has_focus_layer && !is_focused && extra_layers.iter().any(|id| id == &layer.id);
        if !is_focused && !is_extra {
            continue;
        }
        for obj in &layer.objects {
            let mut obj_items = compose_object_items(
                obj,
                &obj.initial_state,
                theme,
                course_input_value(course, obj),
            );
            for item in &mut obj_items {
                if is_extra {
                    item.hit = Hit::None;
                    item.dimmed = true;
                } else {
                    item.dimmed = item.dimmed || obj.initial_state == "hidden";
                }
            }
            items.extend(obj_items);
        }
    }

    items
}
