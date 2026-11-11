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

/// Resolved outline of a shape.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedOutline {
    pub color: egui::Color32,
    pub width: f32,
    pub dash: DashStyle,
}

/// Resolved drawn path. Points stay as fractions of the item's frame.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedPath {
    pub points: Vec<PathPoint>,
    pub style: PathStyle,
    pub closed: bool,
    pub fill: Option<egui::Color32>,
    pub outline: Option<ResolvedOutline>,
    pub start_end: LineEnd,
    pub end_end: LineEnd,
    pub connector: Option<ConnectorStyle>,
}

/// Resolved text carried inside a shape.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedShapeText {
    pub runs: Vec<ResolvedTextRun>,
    pub align: coursecraft_course::Align,
    pub valign: coursecraft_course::VAlign,
    pub paragraphs: Vec<Paragraph>,
    pub autofit: Autofit,
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
    Picture { src: String, tile: bool },
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
    },
    Button {
        label: String,
        fill: egui::Color32,
        fill_style: Option<ResolvedFill>,
        hover_fill: Option<egui::Color32>,
        down_fill: Option<egui::Color32>,
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
        disabled: bool,
    },
    Image {
        src: String,
        alt: String,
        /// The kept part of the source, as validated source fractions. `None`
        /// shows the whole picture.
        crop: Option<egui::Rect>,
    },
    ImagePlaceholder {
        label: String,
    },
    Video {
        src: String,
        poster: Option<String>,
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
fn resolve_text_runs(
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
        FillStyle::Picture { src, mode } => {
            if src.trim().is_empty() {
                return None;
            }
            Some(ResolvedFill::Picture {
                src: src.clone(),
                tile: *mode == PictureFillMode::Tile,
            })
        }
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
    state_fill: Option<&coursecraft_course::Color>,
) -> Option<ResolvedPath> {
    let mut points: Vec<PathPoint> = Vec::with_capacity(path.points.len().min(2000));
    for point in path.points.iter().take(coursecraft_course::PATH_MAX_POINTS) {
        if !point.x.is_finite() || !point.y.is_finite() {
            continue;
        }
        points.push(PathPoint {
            x: point.x.clamp(0.0, 1.0),
            y: point.y.clamp(0.0, 1.0),
        });
    }
    if points.len() < coursecraft_course::PATH_MIN_POINTS {
        return None;
    }
    let start_end = path.start_end;
    let end_end = path.end_end;
    let connector = path
        .connector
        .filter(|_| points.len() == coursecraft_course::PATH_MIN_POINTS);
    let fill = if path.closed {
        state_fill
            .and_then(parse_color)
            .or_else(|| path.fill.as_ref().and_then(parse_color))
    } else {
        None
    };
    Some(ResolvedPath {
        points,
        style: path.style,
        closed: path.closed,
        fill,
        outline: resolve_outline(&path.outline),
        start_end,
        end_end,
        connector,
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
    let point = item_local_point(item, point);
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
    let points: Vec<crate::path_geometry::Pt> = path
        .points
        .iter()
        .map(|p| {
            (
                item.rect.min.x + p.x * item.rect.width(),
                item.rect.min.y + p.y * item.rect.height(),
            )
        })
        .collect();
    let flat = crate::path_geometry::flatten(&points, path.style, path.connector, path.closed);
    let half_stroke = path
        .outline
        .as_ref()
        .map_or(0.0, |outline| outline.width * 0.5);
    let threshold = (half_stroke + 2.0).max(6.0);
    if crate::path_geometry::path_distance(&flat, (point.x, point.y)) <= threshold {
        return true;
    }
    path.closed
        && path.fill.is_some()
        && crate::path_geometry::path_contains(&flat, (point.x, point.y))
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

#[derive(Debug, Clone, Copy, Default)]
struct ComposeOpts {
    marker_open: bool,
    marker_pulse: bool,
    authoring: bool,
    timer_remaining_ms: Option<u32>,
}

/// Compose a marker or panel as the running player draws it.
pub(crate) fn compose_object_with_runtime(
    object: &Object,
    current_state: &str,
    theme: Option<&coursecraft_course::Theme>,
    value: Option<&Value>,
    marker_open: bool,
) -> Option<DrawItem> {
    compose_object_with_runtime_and_timer(object, current_state, theme, value, marker_open, None)
}

/// Compose as the running player draws it, with a timer's live remaining time.
pub(crate) fn compose_object_with_runtime_and_timer(
    object: &Object,
    current_state: &str,
    theme: Option<&coursecraft_course::Theme>,
    value: Option<&Value>,
    marker_open: bool,
    timer_remaining_ms: Option<u32>,
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
        },
    )
}

fn compose_object_opts(
    object: &Object,
    current_state: &str,
    theme: Option<&coursecraft_course::Theme>,
    value: Option<&Value>,
    opts: ComposeOpts,
) -> Option<DrawItem> {
    let rect = validate_frame(&object.frame)?;
    let state_def = object.states.iter().find(|s| s.name == current_state);
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
        .find(|s| s.name == "hover")
        .and_then(|s| s.fill.as_ref())
        .and_then(parse_color);

    let down_fill = object
        .states
        .iter()
        .find(|s| s.name == "down")
        .and_then(|s| s.fill.as_ref())
        .and_then(parse_color);

    let visual = match &object.kind {
        ObjectKind::Text {
            text,
            style,
            runs,
            paragraphs,
            autofit,
            ..
        } => {
            let style_color = state_def
                .and_then(|s| s.fill.as_ref())
                .and_then(parse_color)
                .unwrap_or_else(|| parse_color(&style.color).unwrap_or(default_txt_color));
            let override_text = state_def.and_then(|s| s.text.clone());
            let resolved_runs = resolve_text_runs(
                text,
                style,
                runs,
                override_text.as_deref(),
                style_color,
                default_btn_color,
                theme.map(|t| t.font.as_str()),
            );

            Visual::Text {
                runs: resolved_runs,
                align: style.align,
                valign: style.valign,
                paragraphs: paragraphs.clone(),
                autofit: *autofit,
            }
        }
        ObjectKind::Button {
            label,
            fill,
            fill_style,
        } => {
            let state_fill = state_def
                .and_then(|s| s.fill.as_ref())
                .and_then(parse_color);
            let resolved_label = state_def
                .and_then(|s| s.text.clone())
                .unwrap_or_else(|| label.clone());
            let resolved_fill =
                state_fill.unwrap_or_else(|| parse_color(fill).unwrap_or(default_btn_color));
            Visual::Button {
                label: resolved_label,
                fill: resolved_fill,
                fill_style: if state_fill.is_some() {
                    None
                } else {
                    fill_style.as_ref().and_then(resolve_fill_style)
                },
                hover_fill,
                down_fill,
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
            let state_fill = state_def
                .and_then(|s| s.fill.as_ref())
                .and_then(parse_color);
            let resolved_fill =
                state_fill.unwrap_or_else(|| parse_color(fill).unwrap_or(FALLBACK_GREY));
            let override_text = state_def.and_then(|s| s.text.clone());
            let text = if !shape_allows_text(*shape) {
                None
            } else if let Some(text) = text {
                let style_color = parse_color(&text.style.color).unwrap_or(default_txt_color);
                Some(ResolvedShapeText {
                    runs: resolve_text_runs(
                        &text.text,
                        &text.style,
                        &text.runs,
                        override_text.as_deref(),
                        style_color,
                        default_btn_color,
                        theme.map(|t| t.font.as_str()),
                    ),
                    align: text.style.align,
                    valign: text.style.valign,
                    paragraphs: text.paragraphs.clone(),
                    autofit: text.autofit,
                })
            } else {
                override_text.map(|override_text| ResolvedShapeText {
                    runs: vec![ResolvedTextRun {
                        text: override_text,
                        size: 20.0,
                        color: default_txt_color,
                        bold: false,
                        italic: false,
                        underline: false,
                        font: None,
                        link: None,
                        strikethrough: false,
                        baseline: coursecraft_course::Baseline::Normal,
                        letter_spacing: 0.0,
                    }],
                    align: coursecraft_course::Align::Center,
                    valign: coursecraft_course::VAlign::Middle,
                    paragraphs: Vec::new(),
                    autofit: Autofit::None,
                })
            };
            Visual::Shape {
                shape: *shape,
                adjust: adjust.clone(),
                fill: resolved_fill,
                fill_style: if state_fill.is_some() {
                    None
                } else {
                    fill_style.as_ref().and_then(resolve_fill_style)
                },
                outline: outline
                    .as_ref()
                    .filter(|_| shape_allows_text(*shape))
                    .and_then(resolve_outline),
                text,
                hover_fill,
                down_fill,
                disabled,
            }
        }
        ObjectKind::Path { path } => {
            let resolved = resolve_path(path, state_def.and_then(|s| s.fill.as_ref()))?;
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
        ObjectKind::Image { src, alt, crop } => Visual::Image {
            src: state_def
                .and_then(|s| s.src.as_deref())
                .filter(|s| !s.is_empty())
                .unwrap_or(src)
                .to_string(),
            alt: alt.clone(),
            crop: crop.as_ref().and_then(valid_crop_rect),
        },
        ObjectKind::Video { src, .. } => Visual::Video {
            src: state_def
                .and_then(|s| s.src.as_deref())
                .filter(|s| !s.is_empty())
                .unwrap_or(src)
                .to_string(),
            poster: None,
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
        } => {
            let remaining = opts.timer_remaining_ms.unwrap_or(*duration_ms);
            let text = format_timer_text(remaining, *format);
            let style_color = state_def
                .and_then(|s| s.fill.as_ref())
                .and_then(parse_color)
                .unwrap_or_else(|| parse_color(&style.color).unwrap_or(default_txt_color));
            let runs = resolve_text_runs(
                &text,
                style,
                &[],
                None,
                style_color,
                default_btn_color,
                theme.map(|t| t.font.as_str()),
            );
            Visual::Timer {
                runs,
                align: style.align,
                valign: style.valign,
                format: *format,
                authoring: opts.authoring,
            }
        }
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
            src: state_def
                .and_then(|s| s.src.as_deref())
                .filter(|s| !s.is_empty())
                .unwrap_or(src)
                .to_string(),
        },
        ObjectKind::Lottie {
            src,
            loop_playback,
            autoplay: _,
            speed,
        } => Visual::Lottie {
            src: state_def
                .and_then(|s| s.src.as_deref())
                .filter(|s| !s.is_empty())
                .unwrap_or(src)
                .to_string(),
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
            coursecraft_course::clamp_opacity(object.opacity)
        } else {
            1.0
        },
        clip: None,
        rotation: if visible {
            coursecraft_course::normalize_rotation(object.rotation)
        } else {
            0.0
        },
        flip_h: visible && object.flip_h,
        flip_v: visible && object.flip_v,
        shadow: if visible {
            object.shadow.as_ref().and_then(resolve_shadow)
        } else {
            None
        },
        glow: if visible {
            object.glow.as_ref().and_then(resolve_glow)
        } else {
            None
        },
        media_time_ms: None,
        links: Vec::new(),
        show_table_overflow: false,
        clip_rect: None,
        mark: None,
    })
}

/// Row height and stride (height plus gap) so `count` rows stay on the slide when they can.
fn row_metrics(slide_h: f32, count: usize) -> (f32, f32) {
    const DEFAULT_H: f32 = 56.0;
    const DEFAULT_GAP: f32 = 16.0;
    const MIN_H: f32 = 28.0;
    const MIN_GAP: f32 = 4.0;
    let n = count.max(1) as f32;
    let available = (slide_h - 160.0 - 24.0).max(0.0);
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

fn row_rect(index: usize, row_w: f32, row_h: f32, stride: f32) -> egui::Rect {
    let y = 160.0 + stride * index as f32;
    egui::Rect::from_min_size(
        egui::pos2(80.0, y),
        egui::vec2(row_w.max(0.0), row_h.max(0.0)),
    )
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
    });
}

/// The rect the question prompt occupies in slide coordinates.
///
/// This is the one layout function for the prompt: the renderer and the editor's
/// overlap warning both use it, so they can never disagree.
#[must_use]
pub fn question_prompt_rect(slide_w: f32, slide_h: f32) -> egui::Rect {
    let (slide_w, _slide_h) = finite_slide(slide_w, slide_h);
    let prompt_w = (slide_w - 160.0).max(0.0);
    egui::Rect::from_min_size(egui::pos2(80.0, 60.0), egui::vec2(prompt_w, 80.0))
}

fn compose_question(view: &QuestionCompose<'_>, items: &mut Vec<DrawItem>) {
    let (slide_w, slide_h) = finite_slide(view.slide_w, view.slide_h);
    let prompt_rect = question_prompt_rect(slide_w, slide_h);
    let question = view.question;
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

    let row_w = 900.0f32.min(prompt_rect.width());
    match question.kind {
        QuestionKind::MultipleChoice
        | QuestionKind::MultipleResponse
        | QuestionKind::TrueFalse
        | QuestionKind::Likert => {
            let (row_h, stride) = row_metrics(slide_h, question.choices.len());
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
                    row_rect(i, row_w, row_h, stride),
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
            let (row_h, stride) = row_metrics(slide_h, 1);
            let multiline = question.kind == QuestionKind::Essay;
            let mut rect = row_rect(0, row_w, row_h, stride);
            if multiline {
                let height = (row_h * 4.0).min((slide_h - 24.0 - rect.min.y).max(0.0));
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
            let (row_h, stride) = row_metrics(slide_h, order.len());
            let last = order.len().saturating_sub(1);
            for (i, id) in order.iter().enumerate() {
                let text = question
                    .choices
                    .iter()
                    .find(|c| c.id == *id)
                    .map(|c| c.text.clone())
                    .unwrap_or_default();
                let rect = row_rect(i, row_w, row_h, stride);
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
            let (row_h, stride) = row_metrics(slide_h, question.pairs.len());
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
                let rect = row_rect(i, row_w, row_h, stride);
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
            if matches!(&item.hit, Hit::Object(id) if id == &drag.item) {
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
            compose_object_with_runtime(
                obj,
                &obj.initial_state,
                theme,
                course_input_value(course, obj),
                false,
            )
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
    items: &mut Vec<DrawItem>,
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
        if let Some(mut item) = compose_object_with_runtime_and_timer(
            obj,
            current_state,
            theme,
            player_input_value(player, obj),
            player.marker_open() == Some(obj.id.as_str()),
            player.timer_remaining_ms(&obj.id),
        ) && !matches!(item.visual, Visual::Audio { .. })
        {
            item.rect = item
                .rect
                .translate(egui::vec2(sample.offset.0, sample.offset.1));
            let center = item.rect.center();
            let half = item.rect.size() * 0.5 * sample.scale;
            item.rect = egui::Rect::from_min_max(center - half, center + half);
            item.opacity = (item.opacity * sample.opacity).clamp(0.0, 1.0);
            item.clip = sample.clip;
            if matches!(item.visual, Visual::Lottie { .. }) {
                item.media_time_ms = player.media_clock_ms(&obj.id);
            }
            items.push(item);
        }
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
    let Some(slide) = player.current_slide() else {
        return Vec::new();
    };

    let mut items = Vec::new();
    let base_hidden = player.base_objects_hidden();

    // 1. Master objects, under everything. They ignore timing and animation,
    // are always shown except when their initial state is `hidden`, and never
    // take a press.
    let theme = Some(&player.course().theme);
    push_master_items(player.course(), slide, theme, false, false, &mut items);

    // 2. Base objects
    push_base_items(player, slide, base_hidden, &mut items);

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
            matches!(item.hit, Hit::Object(_))
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
            };
            items.push(pill);
            items.push(label);
        }
        items.extend(drag);
    }

    // 2. Question prompt + response rows
    push_question_items(player, slide, base_hidden, &mut items);

    // 3. Visible layers in order
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
                if let Some(mut item) = compose_object_with_runtime_and_timer(
                    obj,
                    current_state,
                    theme,
                    player_input_value(player, obj),
                    player.marker_open() == Some(obj.id.as_str()),
                    player.timer_remaining_ms(&obj.id),
                ) && !matches!(item.visual, Visual::Audio { .. })
                {
                    item.rect = item
                        .rect
                        .translate(egui::vec2(sample.offset.0, sample.offset.1));
                    let center = item.rect.center();
                    let half = item.rect.size() * 0.5 * sample.scale;
                    item.rect = egui::Rect::from_min_max(center - half, center + half);
                    item.opacity = (item.opacity * sample.opacity).clamp(0.0, 1.0);
                    item.clip = sample.clip;
                    if matches!(item.visual, Visual::Lottie { .. }) {
                        item.media_time_ms = player.media_clock_ms(&obj.id);
                    }
                    items.push(item);
                }
            }
        }
    }

    for item in &mut items {
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

    retarget_question_hits(player, &mut items);
    retarget_review_hits(player, &mut items);
    if !base_hidden {
        push_hotspot_markers(player, &mut items);
    }
    apply_panels(player, slide, &mut items);
    items
}

/// Shift and clip scrolling-panel children, update panel scrollbars, and put
/// open markers above everything else.
fn apply_panels(player: &Player<'_>, slide: &Slide, items: &mut Vec<DrawItem>) {
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

    // A panel's scrollbar is chrome: draw a copy above its children so they
    // cannot cover it. The copy takes no hits; the panel item behind it keeps
    // the wheel and drag targets.
    let mut scrollbars = Vec::new();
    for item in items.iter() {
        if matches!(&item.visual, Visual::ScrollPanel { thumb: Some(_), .. })
            && matches!(&item.hit, Hit::Object(_))
        {
            let mut chrome = item.clone();
            chrome.hit = Hit::None;
            scrollbars.push(chrome);
        }
    }
    items.extend(scrollbars);

    // Open marker labels draw above every other object.
    let mut open = Vec::new();
    let mut rest = Vec::with_capacity(items.len());
    for item in items.drain(..) {
        if matches!(&item.visual, Visual::Marker { open: true, .. }) {
            open.push(item);
        } else {
            rest.push(item);
        }
    }
    rest.append(&mut open);
    *items = rest;
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
    let x = (centre_x - size.x * 0.5).clamp(0.0, max_x);
    Some(egui::Rect::from_min_size(
        egui::pos2(x, top.clamp(0.0, max_y)),
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
    let mut items = Vec::new();
    let focused_layer = focus_layer.and_then(|id| slide.layer(id));
    let has_focus_layer = focused_layer.is_some();
    let hide_base = focused_layer.is_some_and(|layer| layer.hide_base_objects);
    // With the base hidden there is nothing to dim; masters stay.
    let dim_base = has_focus_layer && dim_others && !hide_base;
    let theme = Some(&course.theme);

    // 1. Master objects, under everything. They dim with the base objects when
    // a layer is focused and never take a press.
    push_master_items(course, slide, theme, true, dim_base, &mut items);

    // 2. Base objects
    if !hide_base {
        for obj in &slide.objects {
            if let Some(mut item) = compose_object_with_value(
                obj,
                &obj.initial_state,
                theme,
                course_input_value(course, obj),
            ) {
                item.dimmed = item.dimmed || dim_base || obj.initial_state == "hidden";
                items.push(item);
            }
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
            if let Some(mut item) = compose_object_with_value(
                obj,
                &obj.initial_state,
                theme,
                course_input_value(course, obj),
            ) {
                if is_extra {
                    item.hit = Hit::None;
                    item.dimmed = true;
                } else {
                    item.dimmed = item.dimmed || obj.initial_state == "hidden";
                }
                items.push(item);
            }
        }
    }

    items
}
