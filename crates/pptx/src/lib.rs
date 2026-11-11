//! CourseCraft PowerPoint import (L3).
//!
//! Reads a `.pptx` file's bytes and builds a [`coursecraft_course::Course`]
//! plus the media it references. The file is hostile input: entry counts,
//! sizes, paths, shapes per slide, and XML depth are bounded. No filesystem
//! access; the app passes the bytes in.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Read as _;

use coursecraft_course::{
    Align, Autofit, Color, ConnectorStyle, Course, DashStyle, FillStyle, Glow, Glue, GradientStop,
    LineEnd, Link, ListKind, Object, ObjectKind, Outline, Paragraph, Path, PathPoint, PathStyle,
    PictureFillMode, Point, Rect, SCHEMA_VERSION, Scene, Shadow, ShapeKind, ShapeText, Size, Slide,
    TableBorders, TableCell, TableRow, TextRun, TextStyle, Timing, VAlign, is_valid_link_url,
    normalize_table_fractions, path_frame_and_points,
};

/// A point in the coordinate space of one geometry helper.
type Pt = (f32, f32);

#[doc(hidden)]
pub use zip;

mod docx;

pub use docx::{DocxError, MAX_DOCX_BYTES, MAX_DOCX_ENTRIES, MAX_DOCX_XML_DEPTH, docx_to_text};

#[cfg(test)]
mod tests;

/// Maximum number of archive entries allowed when opening a presentation.
pub const MAX_ENTRIES: usize = 10_000;

/// Maximum uncompressed bytes across all entries in an archive (512 MiB).
pub const MAX_TOTAL_BYTES: u64 = 512 * 1024 * 1024;

/// Maximum uncompressed bytes permitted for a single entry (256 MiB).
pub const MAX_ENTRY_BYTES: u64 = 256 * 1024 * 1024;

/// Maximum slides allowed in a presentation.
pub const MAX_SLIDES: usize = 500;

/// Maximum XML element nesting depth.
pub const MAX_XML_DEPTH: usize = 64;

/// Maximum shapes permitted per slide.
pub const MAX_SHAPES_PER_SLIDE: usize = 2000;

/// Resource limits applied when reading a `.pptx` archive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Maximum number of entries in the archive.
    pub max_entries: usize,
    /// Maximum total uncompressed bytes across all entries.
    pub max_total_bytes: u64,
    /// Maximum uncompressed bytes permitted for any single entry.
    pub max_entry_bytes: u64,
    /// Maximum number of slides allowed.
    pub max_slides: usize,
    /// Maximum XML element nesting depth.
    pub max_xml_depth: usize,
    /// Maximum shapes allowed on any individual slide.
    pub max_shapes_per_slide: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_entries: MAX_ENTRIES,
            max_total_bytes: MAX_TOTAL_BYTES,
            max_entry_bytes: MAX_ENTRY_BYTES,
            max_slides: MAX_SLIDES,
            max_xml_depth: MAX_XML_DEPTH,
            max_shapes_per_slide: MAX_SHAPES_PER_SLIDE,
        }
    }
}

/// Categorises skipped PPTX elements and notes for reporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SkipKind {
    /// Charts (`graphicFrame` with chart).
    Chart,
    /// SmartArt diagrams (`dgm`).
    SmartArt,
    /// Tables (`a:tbl`).
    Table,
    /// Media elements (`p:pic` with video or audio).
    Media,
    /// OLE objects.
    OleObject,
    /// Animations or timing tracks (`p:timing`).
    Animation,
    /// Slide transitions (`p:transition`).
    Transition,
    /// Gradient, pattern, or picture shape fills.
    UnsupportedFill,
    /// Connectors or lines (`p:cxnSp`).
    Connector,
    /// Custom shape geometry (`a:custGeom`).
    CustomGeometry,
    /// Placeholders without text.
    EmptyPlaceholder,
    /// Slides hidden by the author (`show="0"`).
    HiddenSlide,
    /// Image parts that could not be decoded.
    UndecodablePicture,
    /// Package-escaping or external relationship targets.
    ExternalTarget,
    /// Invalid, huge, or non-numeric EMU values, or a negative size.
    InvalidGeometry,
    /// Rotation or flip attributes on shapes or groups.
    RotationOrFlip,
    /// Adjustment handles (`a:avLst`) ignored on preset geometry.
    ShapeAdjustment,
    /// Scheme colors mapped to default light grey.
    SchemeColor,
    /// Aspect ratio differs from 16:9 (letterboxed).
    AspectRatioMismatch,
}

impl std::fmt::Display for SkipKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Chart => write!(f, "Chart"),
            Self::SmartArt => write!(f, "SmartArt"),
            Self::Table => write!(f, "Table"),
            Self::Media => write!(f, "Audio/video media"),
            Self::OleObject => write!(f, "OLE object"),
            Self::Animation => write!(f, "Animation"),
            Self::Transition => write!(f, "Slide transition"),
            Self::UnsupportedFill => write!(f, "Unsupported fill"),
            Self::Connector => write!(f, "Connector/line"),
            Self::CustomGeometry => write!(f, "Custom geometry"),
            Self::EmptyPlaceholder => write!(f, "Empty placeholder"),
            Self::HiddenSlide => write!(f, "Hidden slide"),
            Self::UndecodablePicture => write!(f, "Undecodable picture"),
            Self::ExternalTarget => write!(f, "External target"),
            Self::InvalidGeometry => write!(f, "Invalid geometry"),
            Self::RotationOrFlip => write!(f, "Rotation/flip"),
            Self::ShapeAdjustment => write!(f, "Shape adjustment"),
            Self::SchemeColor => write!(f, "Scheme colour"),
            Self::AspectRatioMismatch => write!(f, "Aspect ratio mismatch"),
        }
    }
}

/// A single report entry describing a skipped or transformed element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportItem {
    /// 1-based slide number where the item occurred.
    pub slide: usize,
    /// Category of the skipped item.
    pub kind: SkipKind,
    /// Human-readable explanation.
    pub note: String,
}

/// Diagnostic report collected during presentation import.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ImportReport {
    /// Number of slides imported.
    pub slides: usize,
    /// List of skipped or transformed items.
    pub items: Vec<ReportItem>,
    /// Deck fonts that are neither Inter nor a font in the imported course,
    /// sorted and de-duplicated. Adding them keeps the deck's look.
    pub missing_fonts: Vec<String>,
}

impl ImportReport {
    /// Formats a concise summary string for UI dialogs:
    /// e.g. `"Imported 12 slides. Skipped: 2 charts on slides 3, 5; …"`.
    #[must_use]
    pub fn summary_window_text(&self) -> String {
        let fonts = self.missing_fonts_sentence();
        if self.items.is_empty() {
            let plural = if self.slides == 1 { "slide" } else { "slides" };
            return format!("Imported {} {plural}.{fonts}", self.slides);
        }

        let mut by_kind: BTreeMap<SkipKind, Vec<usize>> = BTreeMap::new();
        for item in &self.items {
            by_kind.entry(item.kind).or_default().push(item.slide);
        }

        let mut parts = Vec::new();
        for (kind, mut slides) in by_kind {
            slides.sort_unstable();
            slides.dedup();
            let count = slides.len();
            let plural_kind = match kind {
                SkipKind::Chart => {
                    if count == 1 {
                        "chart"
                    } else {
                        "charts"
                    }
                }
                SkipKind::SmartArt => "SmartArt",
                SkipKind::Table => {
                    if count == 1 {
                        "table"
                    } else {
                        "tables"
                    }
                }
                SkipKind::Media => "media",
                SkipKind::OleObject => {
                    if count == 1 {
                        "OLE object"
                    } else {
                        "OLE objects"
                    }
                }
                SkipKind::Animation => {
                    if count == 1 {
                        "animation"
                    } else {
                        "animations"
                    }
                }
                SkipKind::Transition => {
                    if count == 1 {
                        "transition"
                    } else {
                        "transitions"
                    }
                }
                SkipKind::UnsupportedFill => {
                    if count == 1 {
                        "unsupported fill"
                    } else {
                        "unsupported fills"
                    }
                }
                SkipKind::Connector => {
                    if count == 1 {
                        "connector"
                    } else {
                        "connectors"
                    }
                }
                SkipKind::CustomGeometry => "custom geometry",
                SkipKind::EmptyPlaceholder => {
                    if count == 1 {
                        "empty placeholder"
                    } else {
                        "empty placeholders"
                    }
                }
                SkipKind::HiddenSlide => {
                    if count == 1 {
                        "hidden slide"
                    } else {
                        "hidden slides"
                    }
                }
                SkipKind::UndecodablePicture => {
                    if count == 1 {
                        "undecodable picture"
                    } else {
                        "undecodable pictures"
                    }
                }
                SkipKind::ExternalTarget => {
                    if count == 1 {
                        "external target"
                    } else {
                        "external targets"
                    }
                }
                SkipKind::InvalidGeometry => "invalid geometry",
                SkipKind::RotationOrFlip => "rotation/flip",
                SkipKind::ShapeAdjustment => "shape adjustment",
                SkipKind::SchemeColor => {
                    if count == 1 {
                        "scheme colour"
                    } else {
                        "scheme colours"
                    }
                }
                SkipKind::AspectRatioMismatch => "aspect ratio letterbox",
            };
            let slide_list: Vec<String> = slides.into_iter().map(|s| s.to_string()).collect();
            let slide_word = if count == 1 { "slide" } else { "slides" };
            parts.push(format!(
                "{count} {plural_kind} on {slide_word} {}",
                slide_list.join(", ")
            ));
        }

        let plural = if self.slides == 1 { "slide" } else { "slides" };
        format!(
            "Imported {} {plural}. Skipped: {}.{fonts}",
            self.slides,
            parts.join("; ")
        )
    }

    fn missing_fonts_sentence(&self) -> String {
        if self.missing_fonts.is_empty() {
            String::new()
        } else {
            format!(
                "\nAdd these fonts to keep the look: {}.",
                self.missing_fonts.join(", ")
            )
        }
    }
}

/// The result of a successful PowerPoint import.
#[derive(Debug, Clone, PartialEq)]
pub struct Imported {
    /// Validated CourseCraft course document.
    pub course: Course,
    /// Associated media assets stored as `(relative_path, bytes)`.
    pub media: Vec<(String, Vec<u8>)>,
    /// Summary report of imported slides and skipped items.
    pub report: ImportReport,
}

/// Errors produced during PowerPoint import.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PptxError {
    /// The input is not a valid zip archive.
    #[error("not a zip archive: {0}")]
    NotAZip(String),
    /// Archive is missing the mandatory `ppt/presentation.xml` part.
    #[error("missing ppt/presentation.xml")]
    MissingPresentationXml,
    /// Uncompressed archive size exceeds the configured maximum.
    #[error("archive too large")]
    TooLarge,
    /// Archive contains more entries than permitted.
    #[error("too many entries")]
    TooManyEntries,
    /// An entry exceeds the permitted uncompressed size limit.
    #[error("entry too large: {0}")]
    EntryTooLarge(String),
    /// XML element nesting exceeds the depth limit.
    #[error("xml depth exceeded in {0}")]
    XmlDepthExceeded(String),
    /// Malformed XML encountered in a presentation part.
    #[error("malformed XML in {part}: {message}")]
    MalformedXml {
        /// Name of the part containing the invalid XML.
        part: String,
        /// Underlying error description.
        message: String,
    },
    /// The presentation defines no slides.
    #[error("presentation contains no slides")]
    NoSlides,
    /// Number of slides exceeds the configured limit.
    #[error("too many slides: {0}")]
    TooManySlides(usize),
    /// Number of shapes on a slide exceeds the configured limit.
    #[error("too many shapes on slide")]
    TooManyShapes,
    /// An archive entry has an unsafe relative path.
    #[error("unsafe path: {0}")]
    UnsafePath(String),
    /// Archive contains duplicate file names.
    #[error("duplicate entry: {0}")]
    DuplicateEntry(String),
    /// Resulting course failed validation.
    #[error("course validation failed: {0}")]
    Validation(String),
}

impl From<zip::result::ZipError> for PptxError {
    fn from(err: zip::result::ZipError) -> Self {
        PptxError::NotAZip(err.to_string())
    }
}

impl From<std::io::Error> for PptxError {
    fn from(err: std::io::Error) -> Self {
        PptxError::NotAZip(err.to_string())
    }
}

/// Check whether a relative path inside a package is safe.
#[must_use]
pub fn is_safe_relative_path(path: &str) -> bool {
    if path.is_empty() || path.starts_with('/') || path.contains('\\') || path.contains(':') {
        return false;
    }
    if path.chars().any(char::is_control) {
        return false;
    }
    for segment in path.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." {
            return false;
        }
    }
    true
}

/// Resolves a relationship target relative to a part folder.
///
/// A leading `/` means the package root, as Open Packaging Conventions define it.
/// Returns `None` if the target is external, contains invalid characters,
/// or escapes the root via `..`.
#[must_use]
pub fn resolve_target(part_folder: &str, target: &str) -> Option<String> {
    if target.is_empty()
        || target.contains('\\')
        || target.contains(':')
        || target.chars().any(char::is_control)
    {
        return None;
    }
    let mut segments: Vec<&str> = if target.starts_with('/') {
        Vec::new()
    } else {
        part_folder.split('/').filter(|s| !s.is_empty()).collect()
    };
    for seg in target.split('/') {
        if seg.is_empty() || seg == "." {
            continue;
        }
        if seg == ".." {
            if segments.is_empty() {
                return None;
            }
            segments.pop();
        } else {
            segments.push(seg);
        }
    }
    if segments.is_empty() {
        return None;
    }
    let resolved = segments.join("/");
    if !is_safe_relative_path(&resolved) {
        return None;
    }
    Some(resolved)
}

/// Returns the local name of an XML tag or attribute, stripping any prefix.
fn local_name(tag: &str) -> &str {
    if let Some((_, local)) = tag.split_once(':') {
        local
    } else {
        tag
    }
}

/// A lightweight parsed XML node.
#[derive(Debug, Clone)]
struct XmlNode {
    name: String,
    attributes: Vec<(String, String)>,
    text: String,
    children: Vec<XmlNode>,
}

impl XmlNode {
    fn attr(&self, key: &str) -> Option<&str> {
        for (k, v) in &self.attributes {
            if k == key {
                return Some(v.as_str());
            }
        }
        let target_local = local_name(key);
        for (k, v) in &self.attributes {
            if local_name(k) == target_local {
                return Some(v.as_str());
            }
        }
        None
    }

    fn child(&self, name: &str) -> Option<&XmlNode> {
        self.children.iter().find(|c| c.name == name)
    }

    fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a XmlNode> {
        self.children.iter().filter(move |c| c.name == name)
    }

    fn find_descendant(&self, name: &str) -> Option<&XmlNode> {
        if self.name == name {
            return Some(self);
        }
        for child in &self.children {
            if let Some(found) = child.find_descendant(name) {
                return Some(found);
            }
        }
        None
    }
}

/// Parse an XML string into an [`XmlNode`] tree, enforcing `max_depth`.
fn parse_xml_tree(xml_str: &str, part_name: &str, max_depth: usize) -> Result<XmlNode, PptxError> {
    let mut reader = quick_xml::reader::Reader::from_str(xml_str);
    reader.config_mut().trim_text(false);
    let mut stack: Vec<XmlNode> = Vec::new();
    let mut root: Option<XmlNode> = None;

    loop {
        match reader.read_event() {
            Ok(quick_xml::events::Event::Start(e)) => {
                if stack.len().saturating_add(1) > max_depth {
                    return Err(PptxError::XmlDepthExceeded(part_name.to_string()));
                }
                let node = make_xml_node(&e, &reader, part_name)?;
                stack.push(node);
            }
            Ok(quick_xml::events::Event::Empty(e)) => {
                if stack.len().saturating_add(1) > max_depth {
                    return Err(PptxError::XmlDepthExceeded(part_name.to_string()));
                }
                let node = make_xml_node(&e, &reader, part_name)?;
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(node);
                } else if root.is_none() {
                    root = Some(node);
                }
            }
            Ok(quick_xml::events::Event::End(_)) => {
                let Some(node) = stack.pop() else {
                    return Err(PptxError::MalformedXml {
                        part: part_name.to_string(),
                        message: "Unexpected closing tag".to_string(),
                    });
                };
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(node);
                } else {
                    root = Some(node);
                }
            }
            Ok(quick_xml::events::Event::Text(t)) => {
                if let Some(parent) = stack.last_mut() {
                    let decoded = t.decode().map_err(|e| PptxError::MalformedXml {
                        part: part_name.to_string(),
                        message: e.to_string(),
                    })?;
                    let unescaped = quick_xml::escape::unescape(&decoded).map_err(|e| {
                        PptxError::MalformedXml {
                            part: part_name.to_string(),
                            message: e.to_string(),
                        }
                    })?;
                    parent.text.push_str(&unescaped);
                }
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Err(e) => {
                return Err(PptxError::MalformedXml {
                    part: part_name.to_string(),
                    message: e.to_string(),
                });
            }
            _ => {}
        }
    }

    root.ok_or_else(|| PptxError::MalformedXml {
        part: part_name.to_string(),
        message: "Empty XML document".to_string(),
    })
}

fn make_xml_node<B: std::io::BufRead>(
    e: &quick_xml::events::BytesStart<'_>,
    reader: &quick_xml::reader::Reader<B>,
    part_name: &str,
) -> Result<XmlNode, PptxError> {
    let name_str =
        std::str::from_utf8(e.name().into_inner()).map_err(|e| PptxError::MalformedXml {
            part: part_name.to_string(),
            message: e.to_string(),
        })?;
    let local = local_name(name_str).to_string();
    let mut attributes = Vec::new();
    for attr_res in e.attributes() {
        let attr = attr_res.map_err(|e| PptxError::MalformedXml {
            part: part_name.to_string(),
            message: e.to_string(),
        })?;
        let key =
            std::str::from_utf8(attr.key.into_inner()).map_err(|e| PptxError::MalformedXml {
                part: part_name.to_string(),
                message: e.to_string(),
            })?;
        let val = attr
            .decoded_and_normalized_value(quick_xml::XmlVersion::Implicit1_0, reader.decoder())
            .map_err(|e| PptxError::MalformedXml {
                part: part_name.to_string(),
                message: e.to_string(),
            })?;
        attributes.push((key.to_string(), val.into_owned()));
    }
    Ok(XmlNode {
        name: local,
        attributes,
        text: String::new(),
        children: Vec::new(),
    })
}

/// Represents an entry in a `_rels` file.
#[derive(Debug, Clone)]
struct Relationship {
    target: Option<String>,
    is_external: bool,
    /// The raw address of an external relationship (`TargetMode="External"`).
    external_target: Option<String>,
}

/// Parse a `_rels` file into a map of relationship id to [`Relationship`].
fn parse_relationships(
    xml_str: &str,
    part_name: &str,
    part_folder: &str,
    max_depth: usize,
) -> Result<HashMap<String, Relationship>, PptxError> {
    let root = parse_xml_tree(xml_str, part_name, max_depth)?;
    let mut map = HashMap::new();
    for child in root.children_named("Relationship") {
        let Some(id) = child.attr("Id") else {
            continue;
        };
        let target_raw = child.attr("Target").unwrap_or_default();
        let target_mode = child.attr("TargetMode");
        let is_external = target_mode == Some("External")
            || target_raw.starts_with("http://")
            || target_raw.starts_with("https://");

        let target = if is_external {
            None
        } else {
            resolve_target(part_folder, target_raw)
        };

        map.insert(
            id.to_string(),
            Relationship {
                target,
                is_external,
                external_target: if is_external && !target_raw.is_empty() {
                    Some(target_raw.to_string())
                } else {
                    None
                },
            },
        );
    }
    Ok(map)
}

/// Safe relative scale and letterboxing transform.
#[derive(Debug, Clone, Copy)]
struct SlideLayout {
    scale: f64,
    offset_x: f64,
    offset_y: f64,
    aspect_ratio_mismatch: bool,
    cx: f64,
    cy: f64,
}

impl SlideLayout {
    fn from_emu(cx: f64, cy: f64) -> Self {
        let valid_cx = if cx > 0.0 && cx.is_finite() {
            cx
        } else {
            9_144_000.0
        };
        let valid_cy = if cy > 0.0 && cy.is_finite() {
            cy
        } else {
            5_143_500.0
        };

        let scale = (1280.0 / valid_cx).min(720.0 / valid_cy);
        let scaled_w = valid_cx * scale;
        let scaled_h = valid_cy * scale;
        let offset_x = (1280.0 - scaled_w) / 2.0;
        let offset_y = (720.0 - scaled_h) / 2.0;

        let aspect_ratio_mismatch = (valid_cx * 720.0 - valid_cy * 1280.0).abs() > 100.0;

        Self {
            scale,
            offset_x,
            offset_y,
            aspect_ratio_mismatch,
            cx: valid_cx,
            cy: valid_cy,
        }
    }

    /// Multiplier from CSS px at 96 dpi to slide px (EMU scale is px per EMU).
    fn font_scale(self) -> f64 {
        self.scale * 914_400.0 / 96.0
    }

    fn to_slide_px(self, x: f64, y: f64, w: f64, h: f64) -> Rect {
        let px_x = (x * self.scale + self.offset_x) as f32;
        let px_y = (y * self.scale + self.offset_y) as f32;
        let px_w = (w * self.scale) as f32;
        let px_h = (h * self.scale) as f32;
        Rect {
            x: px_x,
            y: px_y,
            w: px_w.max(0.0),
            h: px_h.max(0.0),
        }
    }
}

/// Bounding dimensions of an OpenXML group shape.
#[derive(Debug, Clone, Copy, Default)]
struct GroupBounds {
    off: (f64, f64),
    ext: (f64, f64),
    ch_off: (f64, f64),
    ch_ext: (f64, f64),
}

/// Nested group coordinate transform: an affine map plus the rotation and flips
/// inherited by the objects inside it.
#[derive(Debug, Clone, Copy)]
struct GroupTransform {
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    trans_x: f64,
    trans_y: f64,
    rotation: f64,
    flip_h: bool,
    flip_v: bool,
}

impl Default for GroupTransform {
    fn default() -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            trans_x: 0.0,
            trans_y: 0.0,
            rotation: 0.0,
            flip_h: false,
            flip_v: false,
        }
    }
}

impl GroupTransform {
    fn compose(&self, bounds: GroupBounds, rotation: f64, flip_h: bool, flip_v: bool) -> Self {
        let sx = if bounds.ch_ext.0 != 0.0 {
            bounds.ext.0 / bounds.ch_ext.0
        } else {
            1.0
        };
        let sy = if bounds.ch_ext.1 != 0.0 {
            bounds.ext.1 / bounds.ch_ext.1
        } else {
            1.0
        };
        let radians = rotation.to_radians();
        let (sin, cos) = radians.sin_cos();
        let flip_x = if flip_h { -1.0 } else { 1.0 };
        let flip_y = if flip_v { -1.0 } else { 1.0 };

        // Local map: scale the child, then flip and rotate about the group's
        // parent-space centre.
        let centre_x = bounds.off.0 + bounds.ext.0 * 0.5;
        let centre_y = bounds.off.1 + bounds.ext.1 * 0.5;
        let t0_x = bounds.off.0 - bounds.ch_off.0 * sx;
        let t0_y = bounds.off.1 - bounds.ch_off.1 * sy;
        let rf_tx = cos * flip_x * t0_x - sin * flip_y * t0_y;
        let rf_ty = sin * flip_x * t0_x + cos * flip_y * t0_y;
        let rf_cx = cos * flip_x * centre_x - sin * flip_y * centre_y;
        let rf_cy = sin * flip_x * centre_x + cos * flip_y * centre_y;
        let local_tx = rf_tx + centre_x - rf_cx;
        let local_ty = rf_ty + centre_y - rf_cy;

        let m00 = cos * flip_x * sx;
        let m01 = -sin * flip_y * sy;
        let m10 = sin * flip_x * sx;
        let m11 = cos * flip_y * sy;

        Self {
            a: self.a * m00 + self.c * m10,
            b: self.b * m00 + self.d * m10,
            c: self.a * m01 + self.c * m11,
            d: self.b * m01 + self.d * m11,
            trans_x: self.a * local_tx + self.c * local_ty + self.trans_x,
            trans_y: self.b * local_tx + self.d * local_ty + self.trans_y,
            rotation: self.rotation + rotation,
            flip_h: self.flip_h ^ flip_h,
            flip_v: self.flip_v ^ flip_v,
        }
    }

    fn apply(&self, x: f64, y: f64, w: f64, h: f64) -> (f64, f64, f64, f64) {
        let centre_x = x + w * 0.5;
        let centre_y = y + h * 0.5;
        let tx = self.a * centre_x + self.c * centre_y + self.trans_x;
        let ty = self.b * centre_x + self.d * centre_y + self.trans_y;
        let scale_x = (self.a * self.a + self.b * self.b).sqrt();
        let scale_y = (self.c * self.c + self.d * self.d).sqrt();
        (
            tx - w * scale_x * 0.5,
            ty - h * scale_y * 0.5,
            w * scale_x,
            h * scale_y,
        )
    }
}

/// Parse an EMU coordinate, rejecting non-numeric, negative, or absurdly huge values.
fn parse_emu(s: &str) -> Option<f64> {
    let trimmed = s.trim();
    let val: i64 = trimmed.parse().ok()?;
    if !(0..=100_000_000_000).contains(&val) {
        return None;
    }
    Some(val as f64)
}

/// Parse an EMU position. Unlike a size it may be negative: a shape can sit partly
/// off the slide. Non-numeric or absurdly huge values are still rejected.
fn parse_emu_offset(s: &str) -> Option<f64> {
    let val: i64 = s.trim().parse().ok()?;
    if !(-100_000_000_000..=100_000_000_000).contains(&val) {
        return None;
    }
    Some(val as f64)
}

/// Sanitise a media file name to `[a-z0-9._-]`, lowercase, and deduplicate with `-2`, `-3`.
fn sanitize_media_name(name: &str, existing: &HashSet<String>) -> String {
    let path = std::path::Path::new(name);
    let raw_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("image");
    let raw_ext = path.extension().and_then(|s| s.to_str()).unwrap_or("png");

    let clean_stem: String = raw_stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let stem = if clean_stem.trim_matches('-').is_empty() {
        "image"
    } else {
        clean_stem.trim_matches('-')
    };

    let clean_ext: String = raw_ext
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect();
    let ext = if clean_ext.is_empty() {
        "png"
    } else {
        &clean_ext
    };

    let base = format!("{stem}.{ext}");
    if !existing.contains(&base) {
        return base;
    }

    let mut counter: usize = 2;
    loop {
        let candidate = format!("{stem}-{counter}.{ext}");
        if !existing.contains(&candidate) {
            return candidate;
        }
        counter = counter.saturating_add(1);
    }
}

/// A raw `a:stCxn` / `a:endCxn` attachment waiting for its target.
struct PendingConnectorEnd {
    target: String,
    idx: Option<u8>,
}

struct PendingConnector {
    connector: String,
    start: Option<PendingConnectorEnd>,
    end: Option<PendingConnectorEnd>,
}

/// Slide import tracker collecting objects and per-slide deduplicated skip reports.
struct SlideTracker {
    slide_idx: usize,
    seen_skips: HashSet<SkipKind>,
    items: Vec<ReportItem>,
    text_count: usize,
    shape_count: usize,
    path_count: usize,
    image_count: usize,
    table_count: usize,
    /// DrawingML shape id (`cNvPr/@id`) to imported object id.
    pptx_ids: HashMap<String, String>,
    pending_connectors: Vec<PendingConnector>,
}

impl SlideTracker {
    fn new(slide_idx: usize) -> Self {
        Self {
            slide_idx,
            seen_skips: HashSet::new(),
            items: Vec::new(),
            text_count: 0,
            shape_count: 0,
            path_count: 0,
            image_count: 0,
            table_count: 0,
            pptx_ids: HashMap::new(),
            pending_connectors: Vec::new(),
        }
    }

    fn report_skip(&mut self, kind: SkipKind, note: impl Into<String>) {
        if self.seen_skips.insert(kind) {
            self.items.push(ReportItem {
                slide: self.slide_idx,
                kind,
                note: note.into(),
            });
        }
    }
}

/// Extracted text information from `<p:txBody>`.
struct ExtractedText {
    text: String,
    style: TextStyle,
    runs: Vec<TextRun>,
    paragraphs: Vec<Paragraph>,
    autofit: Autofit,
    has_text: bool,
}

/// What link resolution needs while reading one slide's text.
#[derive(Clone, Copy)]
struct TextLinkContext<'a> {
    slide_rels: &'a HashMap<String, Relationship>,
    slide_parts: &'a HashMap<String, String>,
}

/// Extracted shape geometry, fill and outline information.
struct ExtractedShape {
    kind: Option<ShapeKind>,
    fill: Option<Color>,
    fill_style: Option<FillStyle>,
    outline: Option<Outline>,
    adjust: Vec<f32>,
    is_custom: bool,
    shadow: Option<Shadow>,
    glow: Option<Glow>,
}

/// Import a presentation from raw `.pptx` bytes using default limits.
///
/// # Errors
/// Returns [`PptxError`] if the archive is invalid, limits are exceeded, or XML is malformed.
pub fn import_pptx(bytes: &[u8], title: &str) -> Result<Imported, PptxError> {
    import_pptx_with_limits(bytes, title, Limits::default())
}

/// Import a presentation with caller-specified resource limits.
///
/// # Errors
/// Returns [`PptxError`] if the archive violates limits or is malformed.
pub fn import_pptx_with_limits(
    bytes: &[u8],
    title: &str,
    limits: Limits,
) -> Result<Imported, PptxError> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;

    if archive.len() > limits.max_entries {
        return Err(PptxError::TooManyEntries);
    }

    if archive.has_duplicate_names() {
        return Err(PptxError::DuplicateEntry(
            "duplicate central directory entry".to_string(),
        ));
    }

    let mut entries: HashMap<String, Vec<u8>> = HashMap::new();
    let mut total_bytes: u64 = 0;

    for i in 0..archive.len() {
        let file = archive.by_index(i)?;
        let name = file.name()?.to_string();

        if entries.contains_key(&name) {
            return Err(PptxError::DuplicateEntry(name));
        }

        if let Some(dir_path) = name.strip_suffix('/') {
            if !is_safe_relative_path(dir_path) {
                return Err(PptxError::UnsafePath(name));
            }
            continue;
        }

        if !is_safe_relative_path(&name) {
            return Err(PptxError::UnsafePath(name));
        }

        if file.size() > limits.max_entry_bytes {
            return Err(PptxError::EntryTooLarge(name));
        }

        let mut entry_bytes = Vec::new();
        let read_limit = limits.max_entry_bytes.saturating_add(1);
        file.take(read_limit).read_to_end(&mut entry_bytes)?;

        if entry_bytes.len() as u64 > limits.max_entry_bytes {
            return Err(PptxError::EntryTooLarge(name));
        }

        total_bytes = total_bytes
            .checked_add(entry_bytes.len() as u64)
            .ok_or(PptxError::TooLarge)?;

        if total_bytes > limits.max_total_bytes {
            return Err(PptxError::TooLarge);
        }

        entries.insert(name, entry_bytes);
    }

    let pres_xml_bytes = entries
        .get("ppt/presentation.xml")
        .ok_or(PptxError::MissingPresentationXml)?;
    let pres_xml_str =
        std::str::from_utf8(pres_xml_bytes).map_err(|e| PptxError::MalformedXml {
            part: "ppt/presentation.xml".to_string(),
            message: e.to_string(),
        })?;

    let pres_rels = if let Some(rels_bytes) = entries.get("ppt/_rels/presentation.xml.rels") {
        let rels_str = std::str::from_utf8(rels_bytes).map_err(|e| PptxError::MalformedXml {
            part: "ppt/_rels/presentation.xml.rels".to_string(),
            message: e.to_string(),
        })?;
        parse_relationships(
            rels_str,
            "ppt/_rels/presentation.xml.rels",
            "ppt",
            limits.max_xml_depth,
        )?
    } else {
        HashMap::new()
    };

    let pres_tree = parse_xml_tree(pres_xml_str, "ppt/presentation.xml", limits.max_xml_depth)?;

    // Geometry slide size from <p:sldSz cx="..." cy="..."/>
    let (mut emu_cx, mut emu_cy) = (9_144_000.0, 5_143_500.0);
    if let Some(sld_sz) = pres_tree.find_descendant("sldSz") {
        if let Some(cx_str) = sld_sz.attr("cx")
            && let Some(val) = parse_emu(cx_str)
        {
            emu_cx = val;
        }
        if let Some(cy_str) = sld_sz.attr("cy")
            && let Some(val) = parse_emu(cy_str)
        {
            emu_cy = val;
        }
    }
    let layout = SlideLayout::from_emu(emu_cx, emu_cy);

    // Read slide order from <p:sldIdLst>
    struct SlideRef {
        part_path: String,
        is_hidden: bool,
    }

    let mut slide_refs: Vec<SlideRef> = Vec::new();
    if let Some(sld_id_lst) = pres_tree.find_descendant("sldIdLst") {
        for sld_id in sld_id_lst.children_named("sldId") {
            let Some(rid) = sld_id.attr("r:id").or_else(|| sld_id.attr("id")) else {
                continue;
            };
            let is_hidden = sld_id
                .attr("show")
                .is_some_and(|s| s == "0" || s == "false");
            if let Some(rel) = pres_rels.get(rid)
                && let Some(ref target) = rel.target
            {
                slide_refs.push(SlideRef {
                    part_path: target.clone(),
                    is_hidden,
                });
            }
        }
    }

    if slide_refs.is_empty() {
        return Err(PptxError::NoSlides);
    }

    if slide_refs.len() > limits.max_slides {
        return Err(PptxError::TooManySlides(slide_refs.len()));
    }

    let mut report = ImportReport::default();
    if layout.aspect_ratio_mismatch {
        report.items.push(ReportItem {
            slide: 1,
            kind: SkipKind::AspectRatioMismatch,
            note: format!(
                "Presentation aspect ratio ({:.0}x{:.0} EMU) differs from 16:9; letterboxed",
                layout.cx, layout.cy
            ),
        });
    }

    let mut imported_slides: Vec<Slide> = Vec::new();
    let mut imported_media: Vec<(String, Vec<u8>)> = Vec::new();
    let mut existing_media_names: HashSet<String> = HashSet::new();

    // Slide part path -> the slide id this import gives it, for slide-jump links.
    let slide_parts: HashMap<String, String> = slide_refs
        .iter()
        .enumerate()
        .filter(|(_, slide)| !slide.is_hidden)
        .map(|(index, slide)| {
            (
                slide.part_path.clone(),
                format!("slide{}", index.saturating_add(1)),
            )
        })
        .collect();

    let mut resolver = PlaceholderResolver::new(limits.max_xml_depth);

    for (slide_num, sld_ref) in slide_refs.into_iter().enumerate() {
        let slide_idx = slide_num.saturating_add(1);
        let mut tracker = SlideTracker::new(slide_idx);

        if sld_ref.is_hidden {
            tracker.report_skip(SkipKind::HiddenSlide, "Hidden slide skipped");
            report.items.extend(tracker.items);
            continue;
        }

        let Some(slide_xml_bytes) = entries.get(&sld_ref.part_path) else {
            tracker.report_skip(
                SkipKind::ExternalTarget,
                format!("Missing slide part {}", sld_ref.part_path),
            );
            report.items.extend(tracker.items);
            continue;
        };

        let slide_xml_str =
            std::str::from_utf8(slide_xml_bytes).map_err(|e| PptxError::MalformedXml {
                part: sld_ref.part_path.clone(),
                message: e.to_string(),
            })?;

        let slide_tree = parse_xml_tree(slide_xml_str, &sld_ref.part_path, limits.max_xml_depth)?;

        if slide_tree
            .attr("show")
            .is_some_and(|s| s == "0" || s == "false")
        {
            tracker.report_skip(SkipKind::HiddenSlide, "Hidden slide skipped");
            report.items.extend(tracker.items);
            continue;
        }

        // Slide rels
        let slide_folder = std::path::Path::new(&sld_ref.part_path)
            .parent()
            .and_then(|p| p.to_str())
            .unwrap_or("ppt/slides");
        let slide_file_name = std::path::Path::new(&sld_ref.part_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("slide1.xml");
        let rels_path = format!("{slide_folder}/_rels/{slide_file_name}.rels");

        let slide_rels = if let Some(rels_bytes) = entries.get(&rels_path) {
            let rels_str =
                std::str::from_utf8(rels_bytes).map_err(|e| PptxError::MalformedXml {
                    part: rels_path.clone(),
                    message: e.to_string(),
                })?;
            parse_relationships(rels_str, &rels_path, slide_folder, limits.max_xml_depth)?
        } else {
            HashMap::new()
        };

        // Check for slide transition
        if slide_tree.find_descendant("transition").is_some() {
            tracker.report_skip(SkipKind::Transition, "Slide transition skipped");
        }

        // Check for animations / timing
        if slide_tree.find_descendant("timing").is_some() {
            tracker.report_skip(SkipKind::Animation, "Animations/timing skipped");
        }

        // Extract slide notes if present
        let mut slide_notes = String::new();
        for rel in slide_rels.values() {
            if let Some(ref target) = rel.target
                && target.contains("notesSlide")
                && let Some(notes_bytes) = entries.get(target)
                && let Ok(notes_str) = std::str::from_utf8(notes_bytes)
                && let Ok(notes_tree) = parse_xml_tree(notes_str, target, limits.max_xml_depth)
                && let Some(notes_text) = extract_notes_body(&notes_tree, layout)
            {
                slide_notes = notes_text;
                break;
            }
        }

        let layout_path = slide_rels
            .values()
            .filter_map(|r| r.target.as_deref())
            .filter(|t| t.starts_with("ppt/slideLayouts/") && entries.contains_key(*t))
            .min()
            .map(str::to_string);

        let mut slide_title: Option<String> = None;
        let mut objects: Vec<Object> = Vec::new();

        if let Some(sp_tree) = slide_tree.find_descendant("spTree") {
            let initial_xfrm = GroupTransform::default();
            let mut cx = SlideContext {
                layout: &layout,
                slide_rels: &slide_rels,
                entries: &entries,
                objects: &mut objects,
                slide_title: &mut slide_title,
                imported_media: &mut imported_media,
                existing_media_names: &mut existing_media_names,
                tracker: &mut tracker,
                limits: &limits,
                resolver: &mut resolver,
                layout_path,
                slide_parts: &slide_parts,
            };
            process_group_children(sp_tree, initial_xfrm, 0, &mut cx)?;
        }
        resolve_connector_glues(&mut objects, &mut tracker);

        let final_title = slide_title
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| format!("Slide {slide_idx}"));

        let slide = Slide {
            id: format!("slide{slide_idx}"),
            title: final_title,
            duration_ms: 5000,
            background: None,
            objects,
            layers: Vec::new(),
            triggers: Vec::new(),
            question: None,
            draw: None,
            transition: None,
            notes: slide_notes,
            master: None,
            revisit: coursecraft_course::RevisitMode::Auto,
            player: None,
        };

        imported_slides.push(slide);
        report.items.extend(tracker.items);
    }

    if imported_slides.is_empty() {
        return Err(PptxError::NoSlides);
    }

    report.slides = imported_slides.len();

    let (mut deck_fonts, major_font, minor_font) =
        collect_theme_fonts(&entries, limits.max_xml_depth);
    resolve_theme_font_tokens(
        &mut imported_slides,
        major_font.as_deref(),
        minor_font.as_deref(),
    );
    for slide in &imported_slides {
        collect_slide_fonts(slide, &mut deck_fonts);
    }
    let mut missing_fonts: Vec<String> = deck_fonts
        .into_iter()
        .filter(|font| {
            !font.eq_ignore_ascii_case("Inter") && !font.eq_ignore_ascii_case("Open Sans")
        })
        .collect();
    missing_fonts.sort_by_key(|font| font.to_ascii_lowercase());
    missing_fonts.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    report.missing_fonts = missing_fonts;

    let clean_title = if title.trim().is_empty() {
        "Imported course".to_string()
    } else {
        title.trim().to_string()
    };

    let course = Course {
        schema_version: SCHEMA_VERSION,
        id: "imported-course".to_string(),
        title: clean_title,
        slide_size: Size { w: 1280, h: 720 },
        variables: Vec::new(),
        scenes: vec![Scene {
            id: "imported".to_string(),
            title: "Imported".to_string(),
            slides: imported_slides,
        }],
        masters: Vec::new(),
        templates: Vec::new(),
        banks: Vec::new(),
        start_slide: None,
        quiz: None,
        player: coursecraft_course::PlayerSettings::default(),
        theme: coursecraft_course::Theme::default(),
        language: None,
        strings: std::collections::BTreeMap::new(),
        fonts: Vec::new(),
        background_audio: None,
        glossary: Vec::new(),
        resources: Vec::new(),
        text_styles: Vec::new(),
    };

    // Ensure resulting course is valid
    let issues = course.validate();
    let errors: Vec<String> = issues
        .iter()
        .filter(|i| i.severity == coursecraft_course::Severity::Error)
        .map(|i| format!("{}: {}", i.path, i.message))
        .collect();
    if !errors.is_empty() {
        return Err(PptxError::Validation(errors.join("; ")));
    }

    Ok(Imported {
        course,
        media: imported_media,
        report,
    })
}

/// Fonts named by the presentation's theme parts (`a:majorFont` and
/// `a:minorFont`).
fn collect_theme_fonts(
    entries: &Entries,
    max_depth: usize,
) -> (HashSet<String>, Option<String>, Option<String>) {
    let mut fonts = HashSet::new();
    let mut major = None;
    let mut minor = None;
    for (path, bytes) in entries {
        if !(path.starts_with("ppt/theme/") && path.ends_with(".xml")) {
            continue;
        }
        let Ok(text) = std::str::from_utf8(bytes) else {
            continue;
        };
        let Ok(tree) = parse_xml_tree(text, path, max_depth) else {
            continue;
        };
        for (scheme_name, slot) in [("majorFont", &mut major), ("minorFont", &mut minor)] {
            if let Some(scheme) = tree.find_descendant(scheme_name)
                && let Some(typeface) = scheme
                    .child("latin")
                    .and_then(|latin| latin.attr("typeface"))
                    .map(str::trim)
                    .filter(|name| !name.is_empty() && !name.starts_with('+'))
            {
                fonts.insert(typeface.to_string());
                if slot.is_none() {
                    *slot = Some(typeface.to_string());
                }
            }
        }
    }
    (fonts, major, minor)
}

/// Replace PowerPoint theme font tokens (`+mj-lt`, `+mn-lt`, and their East
/// Asian / complex-script variants) with the theme's actual Latin typeface.
fn resolve_theme_font_tokens(slides: &mut [Slide], major: Option<&str>, minor: Option<&str>) {
    let resolve = |font: &mut Option<String>| {
        let Some(token) = font.as_deref() else {
            return;
        };
        if !token.starts_with('+') {
            return;
        }
        let lower = token.to_ascii_lowercase();
        let chosen = if lower.starts_with("+mj") {
            major
        } else if lower.starts_with("+mn") {
            minor
        } else {
            None
        };
        *font = chosen.map(str::to_owned);
    };
    let resolve_kind = |kind: &mut ObjectKind| {
        let runs = match kind {
            ObjectKind::Text { runs, .. } => Some(runs),
            ObjectKind::Shape {
                text: Some(text), ..
            } => Some(&mut text.runs),
            _ => None,
        };
        if let Some(runs) = runs {
            for run in runs {
                resolve(&mut run.font);
            }
        }
    };
    for slide in slides {
        for object in &mut slide.objects {
            resolve_kind(&mut object.kind);
        }
        for layer in &mut slide.layers {
            for object in &mut layer.objects {
                resolve_kind(&mut object.kind);
            }
        }
    }
}

/// Fonts named by text runs on one imported slide.
fn collect_slide_fonts(slide: &Slide, fonts: &mut HashSet<String>) {
    let mut add_kind = |kind: &ObjectKind| {
        let (runs, style_font) = match kind {
            ObjectKind::Text { runs, style, .. } => (runs.as_slice(), Some(&style.font)),
            ObjectKind::Shape {
                text: Some(text), ..
            } => (text.runs.as_slice(), Some(&text.style.font)),
            _ => (&[][..], None),
        };
        if let Some(font) = style_font
            && !font.eq_ignore_ascii_case("Inter")
            && !font.eq_ignore_ascii_case("Open Sans")
        {
            fonts.insert(font.clone());
        }
        for run in runs {
            if let Some(font) = &run.font
                && !font.eq_ignore_ascii_case("Inter")
                && !font.eq_ignore_ascii_case("Open Sans")
            {
                fonts.insert(font.clone());
            }
        }
    };
    for object in &slide.objects {
        add_kind(&object.kind);
    }
    for layer in &slide.layers {
        for object in &layer.objects {
            add_kind(&object.kind);
        }
    }
}

/// A placeholder frame in EMU: `(x, y, w, h)`.
type EmuFrame = (f64, f64, f64, f64);

/// Package parts keyed by path.
type Entries = HashMap<String, Vec<u8>>;

/// Resolves placeholder geometry from slide layouts and masters.
///
/// Each part is parsed at most once per import (cached by path). Only
/// `slide -> slideLayouts/* -> slideMasters/*` links are followed, so the
/// walk is bounded and cannot loop.
struct PlaceholderResolver {
    max_depth: usize,
    trees: HashMap<String, Option<XmlNode>>,
    masters: HashMap<String, Option<String>>,
}

impl PlaceholderResolver {
    fn new(max_depth: usize) -> Self {
        Self {
            max_depth,
            trees: HashMap::new(),
            masters: HashMap::new(),
        }
    }

    /// Parse (once) the part at `path`; `None` if missing or malformed.
    fn tree(&mut self, entries: &Entries, path: &str) -> Option<&XmlNode> {
        if !self.trees.contains_key(path) {
            let parsed = entries
                .get(path)
                .and_then(|b| std::str::from_utf8(b).ok())
                .and_then(|s| parse_xml_tree(s, path, self.max_depth).ok());
            self.trees.insert(path.to_string(), parsed);
        }
        self.trees.get(path).and_then(Option::as_ref)
    }

    /// The first part under `prefix` that `owner_path` relates to.
    fn related_part(&self, entries: &Entries, owner_path: &str, prefix: &str) -> Option<String> {
        let (folder, file) = owner_path.rsplit_once('/')?;
        let rels_path = format!("{folder}/_rels/{file}.rels");
        let bytes = entries.get(&rels_path)?;
        let text = std::str::from_utf8(bytes).ok()?;
        let rels = parse_relationships(text, &rels_path, folder, self.max_depth).ok()?;
        rels.values()
            .filter_map(|r| r.target.as_deref())
            .filter(|t| t.starts_with(prefix) && entries.contains_key(*t))
            .min()
            .map(str::to_string)
    }

    fn master_of(&mut self, entries: &Entries, layout_path: &str) -> Option<String> {
        if let Some(cached) = self.masters.get(layout_path) {
            return cached.clone();
        }
        let found = self.related_part(entries, layout_path, "ppt/slideMasters/");
        self.masters.insert(layout_path.to_string(), found.clone());
        found
    }

    /// Resolve the frame for a placeholder (`type`, `idx`) via layout then master.
    fn resolve(
        &mut self,
        entries: &Entries,
        layout_path: Option<&str>,
        ph_type: Option<&str>,
        ph_idx: Option<&str>,
    ) -> Option<EmuFrame> {
        let layout_path = layout_path?;
        if let Some(tree) = self.tree(entries, layout_path)
            && let Some(frame) = find_placeholder_frame(tree, ph_type, ph_idx, true)
        {
            return Some(frame);
        }
        let master_path = self.master_of(entries, layout_path)?;
        let tree = self.tree(entries, &master_path)?;
        find_placeholder_frame(tree, ph_type, ph_idx, false)
    }
}

/// Normalise a placeholder type for matching (`ctrTitle` is a `title`, none is `body`).
fn normalize_ph_type(t: Option<&str>, lenient: bool) -> &str {
    match t {
        Some("ctrTitle") => "title",
        Some("subTitle" | "obj") if lenient => "body",
        Some(other) => other,
        None => "body",
    }
}

/// Find the `a:xfrm` frame of the matching placeholder shape in a layout/master tree.
fn find_placeholder_frame(
    tree: &XmlNode,
    ph_type: Option<&str>,
    ph_idx: Option<&str>,
    use_idx: bool,
) -> Option<EmuFrame> {
    let sp_tree = tree.find_descendant("spTree")?;
    let want_type = normalize_ph_type(ph_type, !use_idx);
    let candidates = |by_idx: bool| {
        sp_tree.children_named("sp").filter_map(move |sp| {
            let ph = sp.child("nvSpPr")?.child("nvPr")?.child("ph")?;
            let matches = if by_idx {
                ph_idx.is_some() && ph.attr("idx") == ph_idx
            } else {
                normalize_ph_type(ph.attr("type"), !use_idx) == want_type
            };
            if !matches {
                return None;
            }
            sp_frame(sp)
        })
    };
    if use_idx && let Some(frame) = candidates(true).next() {
        return Some(frame);
    }
    candidates(false).next()
}

/// Read `spPr/xfrm` (`off` + `ext`) of a shape in EMU.
fn sp_frame(sp: &XmlNode) -> Option<EmuFrame> {
    let xfrm = sp.child("spPr")?.child("xfrm")?;
    let off = xfrm.child("off")?;
    let ext = xfrm.child("ext")?;
    Some((
        parse_emu_offset(off.attr("x")?)?,
        parse_emu_offset(off.attr("y")?)?,
        parse_emu(ext.attr("cx")?)?,
        parse_emu(ext.attr("cy")?)?,
    ))
}

/// Context passed through slide content extraction.
struct SlideContext<'a> {
    layout: &'a SlideLayout,
    slide_rels: &'a HashMap<String, Relationship>,
    entries: &'a HashMap<String, Vec<u8>>,
    objects: &'a mut Vec<Object>,
    slide_title: &'a mut Option<String>,
    imported_media: &'a mut Vec<(String, Vec<u8>)>,
    existing_media_names: &'a mut HashSet<String>,
    tracker: &'a mut SlideTracker,
    limits: &'a Limits,
    resolver: &'a mut PlaceholderResolver,
    layout_path: Option<String>,
    slide_parts: &'a HashMap<String, String>,
}

fn drawing_id(node: &XmlNode) -> Option<&str> {
    node.child("nvSpPr")
        .and_then(|nv| nv.child("cNvPr"))
        .or_else(|| node.child("nvPicPr").and_then(|nv| nv.child("cNvPr")))
        .or_else(|| node.child("nvCxnSpPr").and_then(|nv| nv.child("cNvPr")))
        .or_else(|| {
            node.child("nvGraphicFramePr")
                .and_then(|nv| nv.child("cNvPr"))
        })
        .and_then(|c| c.attr("id"))
}

fn record_pptx_id(node: &XmlNode, before: usize, cx: &mut SlideContext<'_>) {
    let Some(drawing_id) = drawing_id(node) else {
        return;
    };
    let ids: Vec<String> = cx
        .objects
        .iter()
        .skip(before)
        .map(|object| object.id.clone())
        .collect();
    for object_id in ids {
        cx.tracker.pptx_ids.insert(drawing_id.to_owned(), object_id);
    }
}

struct GlueTarget {
    frame: Rect,
    rectangle_like: bool,
    connector: bool,
}

fn source_site(frame: Rect, site: u8) -> Point {
    match site % 4 {
        0 => Point {
            x: frame.x + frame.w * 0.5,
            y: frame.y,
        },
        1 => Point {
            x: frame.x + frame.w,
            y: frame.y + frame.h * 0.5,
        },
        2 => Point {
            x: frame.x + frame.w * 0.5,
            y: frame.y + frame.h,
        },
        _ => Point {
            x: frame.x,
            y: frame.y + frame.h * 0.5,
        },
    }
}

fn nearest_site(frame: Rect, point: Point) -> u8 {
    let mut best = (f32::INFINITY, 0u8);
    for site in 0..4u8 {
        let candidate = source_site(frame, site);
        let dx = candidate.x - point.x;
        let dy = candidate.y - point.y;
        let distance = dx * dx + dy * dy;
        if distance.is_finite() && distance < best.0 {
            best = (distance, site);
        }
    }
    best.1
}

/// DrawingML's rectangle connection indices are 0 top, 1 left, 2 bottom and 3
/// right; ours are 0 top, 1 right, 2 bottom and 3 left.
fn rectangle_site(idx: u8) -> u8 {
    match idx % 4 {
        0 => 0,
        1 => 3,
        2 => 2,
        _ => 1,
    }
}

fn resolve_connector_glues(objects: &mut [Object], tracker: &mut SlideTracker) {
    if tracker.pending_connectors.is_empty() {
        return;
    }
    let targets: HashMap<String, GlueTarget> = objects
        .iter()
        .map(|object| {
            let rectangle_like = matches!(
                &object.kind,
                ObjectKind::Shape { shape, .. }
                    if matches!(
                        shape,
                        ShapeKind::Rectangle
                            | ShapeKind::RoundedRectangle
                            | ShapeKind::SnipSingleCorner
                            | ShapeKind::SnipSameSide
                            | ShapeKind::SnipDiagonal
                            | ShapeKind::SnipRound
                            | ShapeKind::RoundSingleCorner
                            | ShapeKind::RoundSameSide
                            | ShapeKind::RoundDiagonal
                    )
            );
            (
                object.id.clone(),
                GlueTarget {
                    frame: object.frame,
                    rectangle_like,
                    connector: matches!(&object.kind, ObjectKind::Path { path } if path.connector.is_some()),
                },
            )
        })
        .collect();
    let pending: HashMap<String, PendingConnector> = tracker
        .pending_connectors
        .drain(..)
        .map(|pending| (pending.connector.clone(), pending))
        .collect();

    for object in objects.iter_mut() {
        let Some(pending) = pending.get(&object.id) else {
            continue;
        };
        let ObjectKind::Path { path } = &mut object.kind else {
            continue;
        };
        if path.connector.is_none() {
            continue;
        }
        let points = path.slide_points(&object.frame);
        let (Some(&start), Some(&end)) = (points.first(), points.last()) else {
            continue;
        };
        if let Some(connection) = &pending.start
            && let Some(target_id) = tracker.pptx_ids.get(&connection.target)
            && let Some(target) = targets.get(target_id)
            && !target.connector
        {
            let site = if target.rectangle_like {
                connection
                    .idx
                    .map(rectangle_site)
                    .unwrap_or_else(|| nearest_site(target.frame, start))
            } else {
                nearest_site(target.frame, start)
            };
            path.start_glue = Some(Glue {
                object: target_id.clone(),
                site,
            });
        }
        if let Some(connection) = &pending.end
            && let Some(target_id) = tracker.pptx_ids.get(&connection.target)
            && let Some(target) = targets.get(target_id)
            && !target.connector
        {
            let site = if target.rectangle_like {
                connection
                    .idx
                    .map(rectangle_site)
                    .unwrap_or_else(|| nearest_site(target.frame, end))
            } else {
                nearest_site(target.frame, end)
            };
            path.end_glue = Some(Glue {
                object: target_id.clone(),
                site,
            });
        }
    }
}

/// Recursively process children of a group or shape tree up to depth 8.
fn process_group_children(
    parent: &XmlNode,
    transform: GroupTransform,
    depth: usize,
    cx: &mut SlideContext<'_>,
) -> Result<(), PptxError> {
    for child in &parent.children {
        if cx.objects.len() >= cx.limits.max_shapes_per_slide {
            return Err(PptxError::TooManyShapes);
        }

        match child.name.as_str() {
            "sp" => {
                let before = cx.objects.len();
                process_shape(child, transform, cx);
                record_pptx_id(child, before, cx);
            }
            "pic" => {
                let before = cx.objects.len();
                process_picture(child, transform, cx);
                record_pptx_id(child, before, cx);
            }
            "grpSp" => {
                if depth >= 8 {
                    cx.tracker.report_skip(
                        SkipKind::RotationOrFlip,
                        "Nested group beyond depth limit 8 skipped",
                    );
                    continue;
                }

                let mut grp_off = (0.0, 0.0);
                let mut grp_ext = (0.0, 0.0);
                let mut grp_ch_off = (0.0, 0.0);
                let mut grp_ch_ext = (0.0, 0.0);
                let mut grp_rotation = 0.0;
                let mut grp_flip_h = false;
                let mut grp_flip_v = false;

                if let Some(grp_pr) = child.child("grpSpPr")
                    && let Some(xfrm) = grp_pr.child("xfrm")
                {
                    grp_rotation = xfrm_rotation(Some(xfrm));
                    (grp_flip_h, grp_flip_v) = xfrm_flips(Some(xfrm));

                    if let Some(off) = xfrm.child("off")
                        && let (Some(x), Some(y)) = (
                            off.attr("x").and_then(parse_emu_offset),
                            off.attr("y").and_then(parse_emu_offset),
                        )
                    {
                        grp_off = (x, y);
                    }
                    if let Some(ext) = xfrm.child("ext")
                        && let (Some(cx_val), Some(cy_val)) = (
                            ext.attr("cx").and_then(parse_emu),
                            ext.attr("cy").and_then(parse_emu),
                        )
                    {
                        grp_ext = (cx_val, cy_val);
                    }
                    if let Some(ch_off) = xfrm.child("chOff")
                        && let (Some(x), Some(y)) = (
                            ch_off.attr("x").and_then(parse_emu_offset),
                            ch_off.attr("y").and_then(parse_emu_offset),
                        )
                    {
                        grp_ch_off = (x, y);
                    }
                    if let Some(ch_ext) = xfrm.child("chExt")
                        && let (Some(cx_val), Some(cy_val)) = (
                            ch_ext.attr("cx").and_then(parse_emu),
                            ch_ext.attr("cy").and_then(parse_emu),
                        )
                    {
                        grp_ch_ext = (cx_val, cy_val);
                    }
                }

                let new_transform = transform.compose(
                    GroupBounds {
                        off: grp_off,
                        ext: grp_ext,
                        ch_off: grp_ch_off,
                        ch_ext: grp_ch_ext,
                    },
                    grp_rotation,
                    grp_flip_h,
                    grp_flip_v,
                );

                process_group_children(child, new_transform, depth.saturating_add(1), cx)?;
            }
            "cxnSp" => process_connector(child, transform, cx),
            "graphicFrame" => {
                if child.find_descendant("tbl").is_some() {
                    let before = cx.objects.len();
                    process_table(child, transform, cx);
                    if cx.objects.len() == before {
                        cx.tracker.report_skip(SkipKind::Table, "Table skipped");
                    } else {
                        record_pptx_id(child, before, cx);
                    }
                } else if child.find_descendant("chart").is_some() {
                    cx.tracker.report_skip(SkipKind::Chart, "Chart skipped");
                } else if child.find_descendant("dgm").is_some()
                    || child.find_descendant("diagram").is_some()
                    || child
                        .find_descendant("graphicData")
                        .and_then(|g| g.attr("uri"))
                        .is_some_and(|u| u.contains("diagram") || u.contains("dgm"))
                {
                    cx.tracker
                        .report_skip(SkipKind::SmartArt, "SmartArt skipped");
                } else if child.find_descendant("oleObj").is_some() {
                    cx.tracker
                        .report_skip(SkipKind::OleObject, "OLE object skipped");
                } else {
                    cx.tracker
                        .report_skip(SkipKind::Chart, "Graphic frame (chart or diagram) skipped");
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Import one `p:graphicFrame` whose graphic data holds an `a:tbl`.
fn process_table(frame_node: &XmlNode, transform: GroupTransform, cx: &mut SlideContext<'_>) {
    let Some(tbl) = frame_node.find_descendant("tbl") else {
        return;
    };
    let Some(xfrm) = frame_node.child("xfrm") else {
        cx.tracker.report_skip(
            SkipKind::InvalidGeometry,
            "Table missing its graphic frame position",
        );
        return;
    };
    let (Some(off), Some(ext)) = (xfrm.child("off"), xfrm.child("ext")) else {
        cx.tracker.report_skip(
            SkipKind::InvalidGeometry,
            "Table missing off or ext geometry",
        );
        return;
    };
    let (Some(x), Some(y), Some(w), Some(h)) = (
        off.attr("x").and_then(parse_emu_offset),
        off.attr("y").and_then(parse_emu_offset),
        ext.attr("cx").and_then(parse_emu),
        ext.attr("cy").and_then(parse_emu),
    ) else {
        cx.tracker.report_skip(
            SkipKind::InvalidGeometry,
            "Table EMU coordinates invalid or huge",
        );
        return;
    };
    let own_rotation = xfrm_rotation(Some(xfrm));
    let (own_flip_h, own_flip_v) = xfrm_flips(Some(xfrm));
    let (root_x, root_y, root_w, root_h) = transform.apply(x, y, w, h);
    let frame = cx.layout.to_slide_px(root_x, root_y, root_w, root_h);

    let rows_nodes: Vec<&XmlNode> = tbl.children_named("tr").collect();
    let grid_nodes: Vec<&XmlNode> = tbl
        .child("tblGrid")
        .map(|grid| grid.children_named("gridCol").collect())
        .unwrap_or_default();
    let mut column_count = grid_nodes.len();
    if column_count == 0 {
        column_count = rows_nodes
            .iter()
            .map(|row| row.children_named("tc").count())
            .max()
            .unwrap_or(0);
    }
    if rows_nodes.is_empty() || column_count == 0 {
        return;
    }

    let mut column_widths: Vec<f64> = grid_nodes
        .iter()
        .filter_map(|node| node.attr("w").and_then(parse_emu))
        .collect();
    if column_widths.len() != column_count || !column_widths.iter().all(|value| *value > 0.0) {
        column_widths = vec![1.0; column_count];
    }
    let mut columns: Vec<f32> = column_widths.iter().map(|value| *value as f32).collect();
    normalize_table_fractions(&mut columns);

    let mut row_heights: Vec<f64> = rows_nodes
        .iter()
        .map(|node| node.attr("h").and_then(parse_emu).unwrap_or(0.0))
        .collect();
    if !row_heights.iter().all(|value| *value > 0.0) {
        row_heights = vec![1.0; rows_nodes.len()];
    }
    let mut heights: Vec<f32> = row_heights.iter().map(|value| *value as f32).collect();
    normalize_table_fractions(&mut heights);

    let links = TextLinkContext {
        slide_rels: cx.slide_rels,
        slide_parts: cx.slide_parts,
    };
    let mut rows: Vec<TableRow> = Vec::with_capacity(rows_nodes.len());
    let mut outer_borders: Vec<Outline> = Vec::new();
    let mut inner_borders: Vec<Outline> = Vec::new();

    for (row_index, row_node) in rows_nodes.iter().enumerate() {
        let mut cells: Vec<TableCell> = Vec::with_capacity(column_count);
        let mut column = 0usize;
        for cell_node in row_node.children_named("tc") {
            if column >= column_count {
                break;
            }
            let col_span =
                attr_span(cell_node, "gridSpan").min(column_count.saturating_sub(column));
            let row_span = attr_span(cell_node, "rowSpan")
                .min(rows_nodes.len().saturating_sub(row_index).max(1));
            let merged = attr_flag(cell_node, "hMerge") || attr_flag(cell_node, "vMerge");
            let props = cell_node.child("tcPr");
            if merged {
                cells.push(TableCell::default());
            } else {
                let text = cell_node
                    .child("txBody")
                    .map(|body| extract_text_runs(body, *cx.layout, Some(links)))
                    .map(|info| TableCell {
                        text: info.text,
                        style: info.style.clone(),
                        runs: info.runs,
                        paragraphs: info.paragraphs,
                        fill: props.and_then(table_cell_fill),
                        row_span: u8::try_from(row_span.max(1)).unwrap_or(u8::MAX),
                        col_span: u8::try_from(col_span.max(1)).unwrap_or(u8::MAX),
                        valign: info.style.valign,
                    })
                    .unwrap_or_default();
                if let Some(props) = props {
                    for (side, outer) in [
                        ("lnL", column == 0),
                        ("lnR", column.saturating_add(col_span) >= column_count),
                        ("lnT", row_index == 0),
                        (
                            "lnB",
                            row_index.saturating_add(row_span) >= rows_nodes.len(),
                        ),
                    ] {
                        if let Some(outline) = table_side_outline(props, side, *cx.layout) {
                            if outer {
                                outer_borders.push(outline);
                            } else {
                                inner_borders.push(outline);
                            }
                        }
                    }
                }
                let span = col_span.max(1);
                cells.push(text);
                for _ in 1..span {
                    cells.push(TableCell::default());
                }
            }
            column = column.saturating_add(if merged { 1 } else { col_span.max(1) });
        }
        cells.resize(column_count, TableCell::default());
        rows.push(TableRow {
            height: heights.get(row_index).copied().unwrap_or(1.0),
            cells,
        });
    }

    let first_row = tbl
        .child("tblPr")
        .and_then(|props| props.attr("firstRow"))
        .is_some_and(|value| value == "1" || value == "true");
    let borders = TableBorders {
        outer: dominant_outline(&outer_borders),
        inner: dominant_outline(&inner_borders),
    };

    cx.tracker.table_count = cx.tracker.table_count.saturating_add(1);
    let name = frame_node
        .child("nvGraphicFramePr")
        .and_then(|nv| nv.child("cNvPr"))
        .and_then(|c| c.attr("name"))
        .unwrap_or("")
        .to_string();
    let id = format!("table{}", cx.tracker.table_count);
    let name = if name.is_empty() {
        format!("Table {}", cx.tracker.table_count)
    } else {
        name
    };
    cx.objects.push(Object {
        id,
        name,
        kind: ObjectKind::Table {
            columns,
            rows,
            borders,
            header_row: first_row,
        },
        frame,
        timing: Timing::default(),
        states: Vec::new(),
        initial_state: "normal".to_string(),
        entrance: None,
        exit: None,
        motion_paths: Vec::new(),
        focus_order: None,
        decorative: false,
        description: None,
        group: None,
        panel: None,
        rotation: normalized_pptx_rotation(transform.rotation + own_rotation),
        flip_h: transform.flip_h ^ own_flip_h,
        flip_v: transform.flip_v ^ own_flip_v,
        opacity: 1.0,
        shadow: None,
        glow: None,
        drag: None,
    });
}

fn attr_span(node: &XmlNode, name: &str) -> usize {
    node.attr(name)
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|value| *value > 0 && *value <= 50)
        .unwrap_or(1)
}

fn attr_flag(node: &XmlNode, name: &str) -> bool {
    node.attr(name)
        .is_some_and(|value| value == "1" || value == "true")
}

fn table_cell_fill(props: &XmlNode) -> Option<Color> {
    props
        .child("solidFill")
        .and_then(|fill| fill.child("srgbClr"))
        .and_then(srgb_color)
}

fn table_side_outline(props: &XmlNode, side: &str, layout: SlideLayout) -> Option<Outline> {
    let line = props.child(side)?;
    if line.child("noFill").is_some() {
        return None;
    }
    let color = line
        .child("solidFill")
        .and_then(|fill| fill.child("srgbClr"))
        .and_then(srgb_color)
        .unwrap_or_else(|| Color("#000000".to_string()));
    let width = line
        .attr("w")
        .and_then(|value| value.trim().parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map_or(1.0, |emu| (emu * layout.scale) as f32)
        .clamp(0.0, 50.0);
    if width <= 0.0 {
        return None;
    }
    let dash = match line.child("prstDash").and_then(|dash| dash.attr("val")) {
        Some("dash" | "lgDash" | "sysDash") => DashStyle::Dash,
        Some("dot" | "sysDot") => DashStyle::Dot,
        _ => DashStyle::Solid,
    };
    Some(Outline { color, width, dash })
}

fn dominant_outline(candidates: &[Outline]) -> Option<Outline> {
    let mut counts: BTreeMap<String, (usize, Outline)> = BTreeMap::new();
    for outline in candidates {
        let key = format!(
            "{}|{:.3}|{:?}",
            outline.color.0, outline.width, outline.dash
        );
        let entry = counts.entry(key).or_insert((0, outline.clone()));
        entry.0 = entry.0.saturating_add(1);
    }
    counts
        .into_values()
        .max_by_key(|(count, _)| *count)
        .map(|(_, outline)| outline)
}

/// Extract notes body text from a notes slide XML document.
fn extract_notes_body(tree: &XmlNode, layout: SlideLayout) -> Option<String> {
    for sp in tree.children_named("sp").chain(
        tree.find_descendant("spTree")
            .into_iter()
            .flat_map(|t| t.children_named("sp")),
    ) {
        if let Some(nv) = sp.child("nvSpPr")
            && let Some(nv_pr) = nv.child("nvPr")
            && let Some(ph) = nv_pr.child("ph")
            && ph.attr("type").is_some_and(|t| t == "body")
            && let Some(tx_body) = sp.child("txBody")
        {
            let text_info = extract_text_runs(tx_body, layout, None);
            let trimmed = text_info.text.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

fn xfrm_flips(xfrm: Option<&XmlNode>) -> (bool, bool) {
    let flag = |name: &str| {
        xfrm.and_then(|xfrm| xfrm.attr(name))
            .is_some_and(|value| value == "1" || value == "true")
    };
    (flag("flipH"), flag("flipV"))
}

/// Read `a:xfrm@rot` in 60000ths of a degree. Missing or hostile values mean
/// no rotation. The result is normalised into `0..360`.
fn xfrm_rotation(xfrm: Option<&XmlNode>) -> f64 {
    let Some(value) = xfrm.and_then(|xfrm| xfrm.attr("rot")) else {
        return 0.0;
    };
    let Ok(raw) = value.trim().parse::<f64>() else {
        return 0.0;
    };
    if !raw.is_finite() || raw.abs() > 360_000_000.0 {
        return 0.0;
    }
    (raw / 60_000.0).rem_euclid(360.0)
}

/// The two slide-space ends of a straight connector in `frame`.
///
/// DrawingML's default runs from top-left to bottom-right. A flip mirrors the
/// relevant axis, which is what makes every diagonal corner choice correct.
fn connector_points(frame: Rect, flip_h: bool, flip_v: bool) -> [Point; 2] {
    let left = frame.x;
    let right = frame.x + frame.w;
    let top = frame.y;
    let bottom = frame.y + frame.h;
    let start = Point {
        x: if flip_h { right } else { left },
        y: if flip_v { bottom } else { top },
    };
    let end = Point {
        x: if flip_h { left } else { right },
        y: if flip_v { top } else { bottom },
    };
    [start, end]
}

fn line_end_from(ln: Option<&XmlNode>, name: &str) -> LineEnd {
    let Some(ln) = ln else {
        return LineEnd::None;
    };
    let Some(node) = ln.child(name) else {
        return LineEnd::None;
    };
    match node.attr("type") {
        Some("triangle" | "stealth") => LineEnd::Arrow,
        Some("arrow") => LineEnd::OpenArrow,
        Some("diamond") => LineEnd::Diamond,
        Some("oval") => LineEnd::Dot,
        _ => LineEnd::None,
    }
}

fn path_outline(sp_pr: &XmlNode, layout: SlideLayout) -> Outline {
    if let Some(outline) = extract_outline(sp_pr, layout) {
        return outline;
    }
    let ln_has_no_fill = sp_pr
        .child("ln")
        .is_some_and(|ln| ln.child("noFill").is_some());
    // An explicit `noFill` means the author drew no line. So does an empty `a:ln`
    // on a filled path, while an empty one on a bare line must stay visible.
    if ln_has_no_fill || (sp_pr.child("ln").is_some() && sp_pr.child("solidFill").is_some()) {
        Outline {
            color: Color("#00000000".to_owned()),
            width: 0.0,
            dash: DashStyle::Solid,
        }
    } else {
        Outline {
            color: Color("#000000".to_owned()),
            width: 1.0,
            dash: DashStyle::Solid,
        }
    }
}

/// Read `a:srgbClr` with its optional `a:alpha` (thousandths of a percent).
fn srgb_color(srgb: &XmlNode) -> Option<Color> {
    let value = srgb.attr("val")?;
    let alpha = srgb
        .child("alpha")
        .and_then(|alpha| alpha.attr("val"))
        .and_then(|val| val.trim().parse::<f64>().ok())
        .filter(|val| val.is_finite())
        .map(|val| (val / 100_000.0).clamp(0.0, 1.0));
    let hex = match alpha {
        Some(alpha) if alpha < 1.0 => {
            format!("#{value}{:02X}", (alpha * 255.0).round() as u8)
        }
        _ => format!("#{value}"),
    };
    let color = Color(hex);
    color.is_valid().then_some(color)
}

fn solid_fill(sp_pr: &XmlNode) -> Option<Color> {
    if sp_pr.child("noFill").is_some() {
        return None;
    }
    srgb_color(sp_pr.child("solidFill")?.child("srgbClr")?)
}

/// Read a colour node's `srgbClr`, falling back to the old default fill grey.
fn node_color(node: &XmlNode) -> Color {
    node.child("srgbClr")
        .and_then(srgb_color)
        .unwrap_or_else(|| Color("#D9D9D9".to_owned()))
}

fn attr_f64(node: &XmlNode, name: &str) -> Option<f64> {
    node.attr(name)
        .and_then(|value| value.trim().parse::<f64>().ok())
        .filter(|value| value.is_finite())
}

/// `a:gradFill` into a linear or radial [`FillStyle`].
fn extract_gradient_fill(grad: &XmlNode) -> Option<FillStyle> {
    let mut stops: Vec<GradientStop> = Vec::new();
    if let Some(list) = grad.child("gsLst") {
        for stop in list.children_named("gs") {
            let position = attr_f64(stop, "pos")
                .map(|value| (value / 100_000.0).clamp(0.0, 1.0) as f32)
                .unwrap_or(0.0);
            stops.push(GradientStop {
                color: node_color(stop),
                position,
            });
        }
    }
    if stops.is_empty() {
        return None;
    }
    stops.sort_by(|a, b| a.position.total_cmp(&b.position));
    stops.truncate(coursecraft_course::MAX_GRADIENT_STOPS);
    if stops.len() < 2 {
        return None;
    }
    if let Some(line) = grad.child("lin") {
        let angle = attr_f64(line, "ang")
            .map(|value| (value / 60_000.0).rem_euclid(360.0) as f32)
            .unwrap_or(0.0);
        return Some(FillStyle::Linear { angle, stops });
    }
    if let Some(path) = grad.child("path") {
        let center = path.child("fillToRect").map_or([0.5, 0.5], |rect| {
            let left = attr_f64(rect, "l").unwrap_or(50_000.0) / 100_000.0;
            let right = attr_f64(rect, "r").unwrap_or(50_000.0) / 100_000.0;
            let top = attr_f64(rect, "t").unwrap_or(50_000.0) / 100_000.0;
            let bottom = attr_f64(rect, "b").unwrap_or(50_000.0) / 100_000.0;
            [
                ((left + (1.0 - right)) * 0.5).clamp(0.0, 1.0) as f32,
                ((top + (1.0 - bottom)) * 0.5).clamp(0.0, 1.0) as f32,
            ]
        });
        return Some(FillStyle::Radial { center, stops });
    }
    Some(FillStyle::Linear { angle: 0.0, stops })
}

/// `a:blipFill` on a shape into a picture source and fit.
fn extract_picture_fill(
    blip_fill: &XmlNode,
    cx: &mut SlideContext<'_>,
) -> Option<(String, PictureFillMode)> {
    let blip = blip_fill.child("blip")?;
    let (target_path, pic_bytes) = preferred_picture(blip, cx)?;
    let file_name = std::path::Path::new(&target_path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("image.png");
    let safe_src = sanitize_media_name(file_name, cx.existing_media_names);
    cx.existing_media_names.insert(safe_src.clone());
    cx.imported_media.push((safe_src.clone(), pic_bytes));
    let mode = if blip_fill.child("tile").is_some() {
        PictureFillMode::Tile
    } else {
        PictureFillMode::Stretch
    };
    Some((safe_src, mode))
}

/// `a:effectLst`'s outer shadow and glow, converted to slide pixels.
fn extract_effects(sp_pr: &XmlNode, layout: SlideLayout) -> (Option<Shadow>, Option<Glow>) {
    let Some(effects) = sp_pr.child("effectLst") else {
        return (None, None);
    };
    let shadow = effects.child("outerShdw").map(|shadow| {
        let distance = attr_f64(shadow, "dist").unwrap_or(0.0) * layout.scale;
        let direction = (attr_f64(shadow, "dir").unwrap_or(0.0) / 60_000.0).to_radians();
        let offset_x = (distance * direction.cos()).clamp(-200.0, 200.0) as f32;
        let offset_y = (distance * direction.sin()).clamp(-200.0, 200.0) as f32;
        let blur =
            (attr_f64(shadow, "blurRad").unwrap_or(0.0) * layout.scale).clamp(0.0, 100.0) as f32;
        Shadow {
            color: node_color(shadow),
            offset_x,
            offset_y,
            blur,
        }
    });
    let glow = effects.child("glow").map(|glow| Glow {
        color: node_color(glow),
        size: (attr_f64(glow, "rad").unwrap_or(0.0) * layout.scale).clamp(0.0, 100.0) as f32,
    });
    (shadow, glow)
}

/// `(rotation, flip_h, flip_v)` inherited by a path object.
type PathTransform = (f64, bool, bool);

fn push_path_object(
    cx: &mut SlideContext<'_>,
    name: &str,
    default_name: &str,
    frame: Rect,
    path: Path,
    transform: PathTransform,
) -> String {
    let (rotation, flip_h, flip_v) = transform;
    cx.tracker.path_count = cx.tracker.path_count.saturating_add(1);
    let id = format!("path{}", cx.tracker.path_count);
    let name = if name.is_empty() {
        format!("{default_name} {}", cx.tracker.path_count)
    } else {
        name.to_owned()
    };
    cx.objects.push(Object {
        id: id.clone(),
        name,
        kind: ObjectKind::Path { path },
        frame,
        timing: Timing::default(),
        states: Vec::new(),
        initial_state: "normal".to_owned(),
        entrance: None,
        exit: None,
        motion_paths: Vec::new(),
        focus_order: None,
        decorative: false,
        description: None,
        group: None,
        panel: None,
        rotation: normalized_pptx_rotation(rotation),
        flip_h,
        flip_v,
        opacity: 1.0,
        shadow: None,
        glow: None,
        drag: None,
    });
    id
}

/// PPTX angles are clockwise degrees; clamp them into the model's range.
fn normalized_pptx_rotation(rotation: f64) -> f32 {
    if !rotation.is_finite() {
        return 0.0;
    }
    let degrees = rotation.rem_euclid(360.0);
    #[allow(clippy::cast_possible_truncation)]
    let degrees = degrees as f32;
    coursecraft_course::normalize_rotation(degrees)
}

fn connection_end(node: &XmlNode, name: &str) -> Option<PendingConnectorEnd> {
    let end = node.child(name)?;
    let target = end.attr("id")?.trim();
    if target.is_empty() {
        return None;
    }
    let idx = end
        .attr("idx")
        .and_then(|value| value.trim().parse::<u32>().ok())
        .map(|value| {
            #[allow(clippy::cast_possible_truncation)]
            let value = value.min(u32::from(u8::MAX)) as u8;
            value
        });
    Some(PendingConnectorEnd {
        target: target.to_owned(),
        idx,
    })
}

/// Import a `p:cxnSp` connector as a two-point path.
fn process_connector(cxn: &XmlNode, transform: GroupTransform, cx: &mut SlideContext<'_>) {
    let name = cxn
        .child("nvCxnSpPr")
        .and_then(|nv| nv.child("cNvPr"))
        .and_then(|c| c.attr("name"))
        .unwrap_or("")
        .to_string();
    let start_connection = connection_end(cxn, "stCxn");
    let end_connection = connection_end(cxn, "endCxn");
    let Some((emu_x, emu_y, emu_w, emu_h)) = sp_frame(cxn) else {
        cx.tracker
            .report_skip(SkipKind::Connector, "Connector missing off or ext geometry");
        return;
    };
    let (root_x, root_y, root_w, root_h) = transform.apply(emu_x, emu_y, emu_w, emu_h);
    let frame = cx.layout.to_slide_px(root_x, root_y, root_w, root_h);
    let sp_pr = cxn.child("spPr");
    let xfrm = sp_pr.and_then(|p| p.child("xfrm"));
    let (flip_h, flip_v) = xfrm_flips(xfrm);
    let rotation = transform.rotation + xfrm_rotation(xfrm);
    let preset = sp_pr
        .and_then(|p| p.child("prstGeom"))
        .and_then(|g| g.attr("prst"));
    let connector = match preset {
        Some(preset) if preset.starts_with("bentConnector") => ConnectorStyle::Elbow,
        Some(preset) if preset.starts_with("curvedConnector") => ConnectorStyle::Curved,
        _ => ConnectorStyle::Straight,
    };
    let outline = sp_pr.map_or_else(
        || Outline {
            color: Color("#000000".to_owned()),
            width: 1.0,
            dash: DashStyle::Solid,
        },
        |sp_pr| path_outline(sp_pr, *cx.layout),
    );
    let ln = sp_pr.and_then(|p| p.child("ln"));
    let start_end = line_end_from(ln, "headEnd");
    let end_end = line_end_from(ln, "tailEnd");
    let points = connector_points(frame, flip_h, flip_v);
    let Some((path_frame, fractions)) = path_frame_and_points(&points) else {
        cx.tracker.report_skip(
            SkipKind::InvalidGeometry,
            "Connector geometry has no usable frame",
        );
        return;
    };
    let path = Path {
        points: fractions,
        style: PathStyle::Straight,
        closed: false,
        fill: None,
        outline,
        start_end,
        end_end,
        connector: Some(connector),
        start_glue: None,
        end_glue: None,
    };
    let connector = push_path_object(
        cx,
        &name,
        "Connector",
        path_frame,
        path,
        (rotation, transform.flip_h, transform.flip_v),
    );
    if start_connection.is_some() || end_connection.is_some() {
        cx.tracker.pending_connectors.push(PendingConnector {
            connector,
            start: start_connection,
            end: end_connection,
        });
    }
}

fn parse_coord(node: &XmlNode, name: &str) -> Option<f32> {
    let value = node.attr(name)?.trim().parse::<f64>().ok()?;
    if !value.is_finite() || value.abs() > 1e9 {
        return None;
    }
    #[allow(clippy::cast_possible_truncation)]
    Some(value as f32)
}

fn path_point(node: &XmlNode) -> Option<Pt> {
    let pt = node.child("pt")?;
    Some((parse_coord(pt, "x")?, parse_coord(pt, "y")?))
}

fn sample_cubic(p0: Pt, p1: Pt, p2: Pt, p3: Pt, out: &mut Vec<Pt>) {
    let steps = 16usize;
    for i in 1..=steps {
        #[allow(clippy::cast_precision_loss)]
        let t = i as f32 / steps as f32;
        let u = 1.0 - t;
        let x =
            u * u * u * p0.0 + 3.0 * u * u * t * p1.0 + 3.0 * u * t * t * p2.0 + t * t * t * p3.0;
        let y =
            u * u * u * p0.1 + 3.0 * u * u * t * p1.1 + 3.0 * u * t * t * p2.1 + t * t * t * p3.1;
        out.push((x, y));
    }
}

fn sample_quad(p0: Pt, p1: Pt, p2: Pt, out: &mut Vec<Pt>) {
    let steps = 12usize;
    for i in 1..=steps {
        #[allow(clippy::cast_precision_loss)]
        let t = i as f32 / steps as f32;
        let u = 1.0 - t;
        out.push((
            u * u * p0.0 + 2.0 * u * t * p1.0 + t * t * p2.0,
            u * u * p0.1 + 2.0 * u * t * p1.1 + t * t * p2.1,
        ));
    }
}

fn sample_arc(current: Pt, rx: f32, ry: f32, start_angle: f32, swing: f32, out: &mut Vec<Pt>) {
    if rx <= 0.0 || ry <= 0.0 || !swing.is_finite() {
        return;
    }
    let centre = (
        current.0 - rx * start_angle.cos(),
        current.1 - ry * start_angle.sin(),
    );
    let steps = ((swing.abs() / (std::f32::consts::PI / 12.0)).ceil() as usize).clamp(2, 256);
    for i in 1..=steps {
        #[allow(clippy::cast_precision_loss)]
        let t = i as f32 / steps as f32;
        let angle = start_angle + swing * t;
        out.push((centre.0 + rx * angle.cos(), centre.1 + ry * angle.sin()));
    }
}

/// Flatten the first `a:path` in `a:custGeom` into slide coordinates.
fn custom_geometry_path(
    sp_pr: &XmlNode,
    frame: Rect,
    tracker: &mut SlideTracker,
    layout: SlideLayout,
) -> Option<Path> {
    let path_lst = sp_pr.child("custGeom")?.child("pathLst")?;
    let mut paths = path_lst.children_named("path");
    let first = paths.next()?;
    if paths.next().is_some() {
        tracker.report_skip(
            SkipKind::CustomGeometry,
            "Custom geometry has several subpaths; only the first was imported",
        );
    }
    let local_w = parse_coord(first, "w").filter(|value| *value > 0.0);
    let local_h = parse_coord(first, "h").filter(|value| *value > 0.0);
    let mut local: Vec<Pt> = Vec::new();
    let mut current: Option<Pt> = None;
    let mut closed = false;
    let mut extra_subpath = false;
    for command in &first.children {
        if local.len() >= coursecraft_course::PATH_MAX_POINTS {
            break;
        }
        match command.name.as_str() {
            "moveTo" | "lnTo" => {
                if let Some(point) = path_point(command) {
                    if command.name == "moveTo" && !local.is_empty() {
                        // Several contours inside one path: keep the first only.
                        extra_subpath = true;
                        break;
                    }
                    local.push(point);
                    current = Some(point);
                }
            }
            "cubicBezTo" => {
                let Some(from) = current else {
                    continue;
                };
                let controls: Vec<Pt> = command
                    .children_named("pt")
                    .filter_map(|pt| Some((parse_coord(pt, "x")?, parse_coord(pt, "y")?)))
                    .collect();
                if let (Some(&c1), Some(&c2), Some(&end)) =
                    (controls.first(), controls.get(1), controls.get(2))
                {
                    sample_cubic(from, c1, c2, end, &mut local);
                    current = Some(end);
                }
            }
            "quadBezTo" => {
                let Some(from) = current else {
                    continue;
                };
                let controls: Vec<Pt> = command
                    .children_named("pt")
                    .filter_map(|pt| Some((parse_coord(pt, "x")?, parse_coord(pt, "y")?)))
                    .collect();
                if let (Some(&c1), Some(&end)) = (controls.first(), controls.get(1)) {
                    sample_quad(from, c1, end, &mut local);
                    current = Some(end);
                }
            }
            "arcTo" => {
                let Some(from) = current else {
                    continue;
                };
                let rx = parse_coord(command, "wR").unwrap_or(0.0);
                let ry = parse_coord(command, "hR").unwrap_or(0.0);
                let start = parse_coord(command, "stAng").unwrap_or(0.0) / 60_000.0
                    * std::f32::consts::PI
                    / 180.0;
                let swing = parse_coord(command, "swAng").unwrap_or(0.0) / 60_000.0
                    * std::f32::consts::PI
                    / 180.0;
                sample_arc(from, rx, ry, start, swing, &mut local);
                current = local.last().copied();
            }
            "close" => closed = true,
            _ => {}
        }
    }
    if extra_subpath {
        tracker.report_skip(
            SkipKind::CustomGeometry,
            "Custom geometry has several subpaths; only the first was imported",
        );
    }
    if local.len() < 2 {
        return None;
    }
    let scale_x = local_w.map_or(1.0, |w| if frame.w > 0.0 { frame.w / w } else { 1.0 });
    let scale_y = local_h.map_or(1.0, |h| if frame.h > 0.0 { frame.h / h } else { 1.0 });
    let frame_w = if frame.w > 0.0 { frame.w } else { 1.0 };
    let frame_h = if frame.h > 0.0 { frame.h } else { 1.0 };
    let mut fractions: Vec<PathPoint> = Vec::new();
    for &(x, y) in &local {
        if !x.is_finite() || !y.is_finite() {
            continue;
        }
        let slide_x = frame.x + x * scale_x;
        let slide_y = frame.y + y * scale_y;
        fractions.push(PathPoint {
            x: ((slide_x - frame.x) / frame_w).clamp(0.0, 1.0),
            y: ((slide_y - frame.y) / frame_h).clamp(0.0, 1.0),
        });
    }
    if fractions.len() < 2 {
        return None;
    }
    fractions.truncate(coursecraft_course::PATH_MAX_POINTS);
    let outline = path_outline(sp_pr, layout);
    let ln = sp_pr.child("ln");
    Some(Path {
        points: fractions,
        style: PathStyle::Straight,
        closed,
        fill: if closed { solid_fill(sp_pr) } else { None },
        outline,
        start_end: line_end_from(ln, "headEnd"),
        end_end: line_end_from(ln, "tailEnd"),
        connector: None,
        start_glue: None,
        end_glue: None,
    })
}

/// Process a single `<p:sp>` shape or text box.
fn process_shape(sp: &XmlNode, transform: GroupTransform, cx: &mut SlideContext<'_>) {
    let name = sp
        .child("nvSpPr")
        .and_then(|nv| nv.child("cNvPr"))
        .and_then(|c| c.attr("name"))
        .unwrap_or("")
        .to_string();

    let is_title_ph = sp
        .child("nvSpPr")
        .and_then(|nv| nv.child("nvPr"))
        .and_then(|np| np.child("ph"))
        .and_then(|ph| ph.attr("type"))
        .is_some_and(|t| t == "title" || t == "ctrTitle");

    let is_any_ph = sp
        .child("nvSpPr")
        .and_then(|nv| nv.child("nvPr"))
        .and_then(|np| np.child("ph"))
        .is_some();

    let ph = sp
        .child("nvSpPr")
        .and_then(|nv| nv.child("nvPr"))
        .and_then(|np| np.child("ph"));

    let own_xfrm = sp.child("spPr").and_then(|p| p.child("xfrm"));
    if own_xfrm.is_none() && ph.is_none() {
        return;
    }
    let own_rotation = xfrm_rotation(own_xfrm);
    let (own_flip_h, own_flip_v) = xfrm_flips(own_xfrm);
    let object_rotation = transform.rotation + own_rotation;
    let object_flip_h = transform.flip_h ^ own_flip_h;
    let object_flip_v = transform.flip_v ^ own_flip_v;

    let mut text_info = sp
        .child("txBody")
        .map(|tx| {
            extract_text_runs(
                tx,
                *cx.layout,
                Some(TextLinkContext {
                    slide_rels: cx.slide_rels,
                    slide_parts: cx.slide_parts,
                }),
            )
        })
        .unwrap_or(ExtractedText {
            text: String::new(),
            style: TextStyle::default(),
            runs: Vec::new(),
            paragraphs: Vec::new(),
            autofit: Autofit::None,
            has_text: false,
        });

    // PowerPoint centres text in autoshapes and in centred-title/subtitle
    // placeholders unless the paragraph says otherwise; text boxes stay left.
    let explicit_align = sp
        .child("txBody")
        .and_then(|tx| tx.children_named("p").next())
        .and_then(|p| p.child("pPr"))
        .and_then(|p_pr| p_pr.attr("algn"))
        .is_some();
    let is_text_box = sp
        .child("nvSpPr")
        .and_then(|nv| nv.child("cNvSpPr"))
        .and_then(|c| c.attr("txBox"))
        .is_some_and(|v| v == "1" || v == "true");
    let is_autoshape = sp.child("spPr").and_then(|p| p.child("prstGeom")).is_some()
        && ph.is_none()
        && !is_text_box;
    let centred_ph = ph
        .and_then(|p| p.attr("type"))
        .is_some_and(|t| t == "ctrTitle" || t == "subTitle");
    if !explicit_align && (is_autoshape || centred_ph) {
        text_info.style.align = Align::Center;
    }

    // Placeholder sizes normally come from the layout/master text styles, which
    // aren't read. Without an explicit run size, use the standard Office
    // defaults so a title doesn't import as body-sized text.
    if let Some(ph) = ph
        && !text_info.runs.iter().any(|r| r.size.is_some())
    {
        let pt = match ph.attr("type") {
            Some("title" | "ctrTitle") => 44.0,
            Some("subTitle") => 24.0,
            _ => 28.0,
        };
        let px = (pt * (4.0 / 3.0) * cx.layout.font_scale()).clamp(1.0, 400.0);
        if px.is_finite() {
            text_info.style.size = px as f32;
        }
    }

    if is_title_ph && cx.slide_title.is_none() && text_info.has_text {
        let first_line = text_info.text.lines().next().unwrap_or("").trim();
        let extracted_title: String = first_line.chars().take(200).collect();
        if !extracted_title.is_empty() {
            *cx.slide_title = Some(extracted_title);
        }
    }

    // Placeholders without text are skipped
    if is_any_ph && !text_info.has_text {
        cx.tracker.report_skip(
            SkipKind::EmptyPlaceholder,
            "Placeholder without text skipped",
        );
        return;
    }

    let (emu_x, emu_y, emu_w, emu_h) = if let Some(xfrm) = own_xfrm {
        let (Some(off_node), Some(ext_node)) = (xfrm.child("off"), xfrm.child("ext")) else {
            cx.tracker.report_skip(
                SkipKind::InvalidGeometry,
                "Shape missing off or ext geometry",
            );
            return;
        };

        let (Some(emu_x), Some(emu_y), Some(emu_w), Some(emu_h)) = (
            off_node.attr("x").and_then(parse_emu_offset),
            off_node.attr("y").and_then(parse_emu_offset),
            ext_node.attr("cx").and_then(parse_emu),
            ext_node.attr("cy").and_then(parse_emu),
        ) else {
            cx.tracker.report_skip(
                SkipKind::InvalidGeometry,
                "Shape EMU coordinates invalid or huge",
            );
            return;
        };
        (emu_x, emu_y, emu_w, emu_h)
    } else {
        // Placeholder with inherited geometry: layout, then master.
        let resolved = ph.and_then(|ph| {
            cx.resolver.resolve(
                cx.entries,
                cx.layout_path.as_deref(),
                ph.attr("type"),
                ph.attr("idx"),
            )
        });
        let Some(frame) = resolved else {
            cx.tracker.report_skip(
                SkipKind::InvalidGeometry,
                "Placeholder without position skipped",
            );
            return;
        };
        frame
    };

    let (root_x, root_y, root_w, root_h) = transform.apply(emu_x, emu_y, emu_w, emu_h);
    let frame = cx.layout.to_slide_px(root_x, root_y, root_w, root_h);

    let empty_sp_pr = XmlNode {
        name: "spPr".to_string(),
        attributes: Vec::new(),
        text: String::new(),
        children: Vec::new(),
    };
    let sp_pr = sp.child("spPr").unwrap_or(&empty_sp_pr);

    // A preset `line` autoshape is a two-point drawn line, not a filled band.
    if sp_pr
        .child("prstGeom")
        .and_then(|geometry| geometry.attr("prst"))
        == Some("line")
    {
        let (flip_h, flip_v) = xfrm_flips(own_xfrm);
        let points = connector_points(frame, flip_h, flip_v);
        if let Some((path_frame, fractions)) = path_frame_and_points(&points) {
            let ln = sp_pr.child("ln");
            let path = Path {
                points: fractions,
                style: PathStyle::Straight,
                closed: false,
                fill: None,
                outline: path_outline(sp_pr, *cx.layout),
                start_end: line_end_from(ln, "headEnd"),
                end_end: line_end_from(ln, "tailEnd"),
                connector: None,
                start_glue: None,
                end_glue: None,
            };
            push_path_object(
                cx,
                &name,
                "Line",
                path_frame,
                path,
                (object_rotation, transform.flip_h, transform.flip_v),
            );
        }
        return;
    }

    // `a:custGeom/a:pathLst` becomes a drawn path with the first subpath.
    if sp_pr.child("custGeom").is_some() {
        match custom_geometry_path(sp_pr, frame, cx.tracker, *cx.layout) {
            Some(path) => {
                push_path_object(
                    cx,
                    &name,
                    "Freeform",
                    frame,
                    path,
                    (object_rotation, object_flip_h, object_flip_v),
                );
            }
            None => cx.tracker.report_skip(
                SkipKind::CustomGeometry,
                "Custom geometry shape had no usable path",
            ),
        }
        return;
    }

    let shape_info = extract_shape_info(sp_pr, cx);
    if shape_info.is_custom {
        cx.tracker
            .report_skip(SkipKind::CustomGeometry, "Custom geometry shape skipped");
        return;
    }

    // Determine what objects to emit
    match (shape_info.kind, shape_info.fill) {
        (Some(sk), Some(fill)) => {
            // Styled shape. Its text lives inside it so it moves and imports
            // with the shape.
            cx.tracker.shape_count = cx.tracker.shape_count.saturating_add(1);
            let shape_id = format!("shape{}", cx.tracker.shape_count);
            let shape_name = if name.is_empty() {
                format!("Shape {}", cx.tracker.shape_count)
            } else {
                name.clone()
            };
            let text = if text_info.has_text && coursecraft_course::shape_allows_text(sk) {
                Some(ShapeText {
                    text: text_info.text,
                    style: text_info.style,
                    runs: text_info.runs,
                    paragraphs: text_info.paragraphs,
                    autofit: text_info.autofit,
                })
            } else {
                None
            };

            cx.objects.push(Object {
                id: shape_id,
                name: shape_name,
                kind: ObjectKind::Shape {
                    shape: sk,
                    fill,
                    fill_style: shape_info.fill_style.clone(),
                    outline: if coursecraft_course::shape_allows_text(sk) {
                        shape_info.outline
                    } else {
                        None
                    },
                    text,
                    adjust: shape_info.adjust.clone(),
                },
                frame,
                timing: Timing::default(),
                states: Vec::new(),
                initial_state: "normal".to_string(),
                entrance: None,
                exit: None,
                motion_paths: Vec::new(),
                focus_order: None,
                decorative: false,
                description: None,
                group: None,
                panel: None,
                rotation: normalized_pptx_rotation(object_rotation),
                flip_h: object_flip_h,
                flip_v: object_flip_v,
                opacity: 1.0,
                shadow: shape_info.shadow.clone(),
                glow: shape_info.glow.clone(),
                drag: None,
            });
        }
        (Some(sk), None) => {
            // Basic shape without explicit fill
            if sk == ShapeKind::Rectangle && text_info.has_text {
                // Text box
                cx.tracker.text_count = cx.tracker.text_count.saturating_add(1);
                let text_id = format!("text{}", cx.tracker.text_count);
                let text_name = if name.is_empty() {
                    format!("Text {}", cx.tracker.text_count)
                } else {
                    name
                };

                cx.objects.push(Object {
                    id: text_id,
                    name: text_name,
                    kind: ObjectKind::Text {
                        text: text_info.text,
                        style: text_info.style,
                        runs: text_info.runs,
                        paragraphs: text_info.paragraphs,
                        autofit: text_info.autofit,
                        text_style: None,
                    },
                    frame,
                    timing: Timing::default(),
                    states: Vec::new(),
                    initial_state: "normal".to_string(),
                    entrance: None,
                    exit: None,
                    motion_paths: Vec::new(),
                    focus_order: None,
                    decorative: false,
                    description: None,
                    group: None,
                    panel: None,
                    rotation: normalized_pptx_rotation(object_rotation),
                    flip_h: object_flip_h,
                    flip_v: object_flip_v,
                    opacity: 1.0,
                    shadow: None,
                    glow: None,
                    drag: None,
                });
            } else {
                // Non-rectangle shape (roundRect, ellipse) defaults to #D9D9D9
                cx.tracker.shape_count = cx.tracker.shape_count.saturating_add(1);
                let shape_id = format!("shape{}", cx.tracker.shape_count);
                let shape_name = if name.is_empty() {
                    format!("Shape {}", cx.tracker.shape_count)
                } else {
                    name.clone()
                };
                let text = if text_info.has_text && coursecraft_course::shape_allows_text(sk) {
                    Some(ShapeText {
                        text: text_info.text,
                        style: text_info.style,
                        runs: text_info.runs,
                        paragraphs: text_info.paragraphs,
                        autofit: text_info.autofit,
                    })
                } else {
                    None
                };

                cx.objects.push(Object {
                    id: shape_id,
                    name: shape_name,
                    kind: ObjectKind::Shape {
                        shape: sk,
                        fill: Color("#D9D9D9".to_string()),
                        fill_style: shape_info.fill_style.clone(),
                        outline: if coursecraft_course::shape_allows_text(sk) {
                            shape_info.outline
                        } else {
                            None
                        },
                        text,
                        adjust: shape_info.adjust.clone(),
                    },
                    frame,
                    timing: Timing::default(),
                    states: Vec::new(),
                    initial_state: "normal".to_string(),
                    entrance: None,
                    exit: None,
                    motion_paths: Vec::new(),
                    focus_order: None,
                    decorative: false,
                    description: None,
                    group: None,
                    panel: None,
                    rotation: normalized_pptx_rotation(object_rotation),
                    flip_h: object_flip_h,
                    flip_v: object_flip_v,
                    opacity: 1.0,
                    shadow: shape_info.shadow.clone(),
                    glow: shape_info.glow.clone(),
                    drag: None,
                });
            }
        }
        (None, _) => {
            // No explicit preset geometry: if it has text, treat as text box
            if text_info.has_text {
                cx.tracker.text_count = cx.tracker.text_count.saturating_add(1);
                let text_id = format!("text{}", cx.tracker.text_count);
                let text_name = if name.is_empty() {
                    format!("Text {}", cx.tracker.text_count)
                } else {
                    name
                };

                cx.objects.push(Object {
                    id: text_id,
                    name: text_name,
                    kind: ObjectKind::Text {
                        text: text_info.text,
                        style: text_info.style,
                        runs: text_info.runs,
                        paragraphs: text_info.paragraphs,
                        autofit: text_info.autofit,
                        text_style: None,
                    },
                    frame,
                    timing: Timing::default(),
                    states: Vec::new(),
                    initial_state: "normal".to_string(),
                    entrance: None,
                    exit: None,
                    motion_paths: Vec::new(),
                    focus_order: None,
                    decorative: false,
                    description: None,
                    group: None,
                    panel: None,
                    rotation: normalized_pptx_rotation(object_rotation),
                    flip_h: object_flip_h,
                    flip_v: object_flip_v,
                    opacity: 1.0,
                    shadow: None,
                    glow: None,
                    drag: None,
                });
            }
        }
    }
}

/// Extract shape geometry, fill and effects from `<p:spPr>`.
fn extract_shape_info(sp_pr: &XmlNode, cx: &mut SlideContext<'_>) -> ExtractedShape {
    let mut kind = None;
    let mut adjust = Vec::new();
    let mut is_custom = false;

    if sp_pr.child("custGeom").is_some() {
        is_custom = true;
    } else if let Some(prst) = sp_pr.child("prstGeom") {
        match prst.attr("prst") {
            Some(name) => match coursecraft_course::shape_kind_for_ooxml(name) {
                Some(kind_value) => {
                    kind = Some(kind_value);
                    adjust = map_adjust(kind_value, name, prst.child("avLst"));
                }
                None => is_custom = true,
            },
            None => is_custom = true,
        }
        if kind.is_some() && adjust.is_empty() && has_adjustment_value(prst.child("avLst")) {
            cx.tracker.report_skip(
                SkipKind::ShapeAdjustment,
                "Shape adjustment values ignored; the default proportions are used",
            );
        }
    }

    let mut fill = None;
    let mut fill_style = None;
    if sp_pr.child("noFill").is_some() {
        fill = Some(Color("#00000000".to_string()));
    } else if let Some(solid) = sp_pr.child("solidFill") {
        if let Some(srgb) = solid.child("srgbClr") {
            fill = Some(srgb_color(srgb).unwrap_or_else(|| Color("#D9D9D9".to_string())));
        } else if solid.child("schemeClr").is_some() {
            cx.tracker.report_skip(
                SkipKind::SchemeColor,
                "Scheme colour replaced with default fill #D9D9D9",
            );
            fill = Some(Color("#D9D9D9".to_string()));
        } else {
            fill = Some(Color("#D9D9D9".to_string()));
        }
    } else if let Some(grad) = sp_pr.child("gradFill") {
        fill_style = extract_gradient_fill(grad);
        if fill_style.is_some() {
            fill = Some(Color("#D9D9D9".to_string()));
        } else {
            cx.tracker.report_skip(
                SkipKind::UnsupportedFill,
                "Gradient fill had fewer than two stops and was replaced with the default fill",
            );
            fill = Some(Color("#D9D9D9".to_string()));
        }
    } else if let Some(blip_fill) = sp_pr.child("blipFill") {
        fill_style =
            extract_picture_fill(blip_fill, cx).map(|(src, mode)| FillStyle::Picture { src, mode });
        fill = Some(Color("#D9D9D9".to_string()));
        if fill_style.is_none() {
            cx.tracker.report_skip(
                SkipKind::UnsupportedFill,
                "Picture fill could not be read; replaced with the default fill",
            );
        }
    } else if sp_pr.child("pattFill").is_some() {
        cx.tracker.report_skip(
            SkipKind::UnsupportedFill,
            "Pattern fill replaced with default fill",
        );
        fill = Some(Color("#D9D9D9".to_string()));
    }

    let outline = extract_outline(sp_pr, *cx.layout);
    let (shadow, glow) = extract_effects(sp_pr, *cx.layout);

    ExtractedShape {
        kind,
        fill,
        fill_style,
        outline,
        adjust,
        is_custom,
        shadow,
        glow,
    }
}

fn has_adjustment_value(av: Option<&XmlNode>) -> bool {
    av.is_some_and(|av| av.children_named("gd").next().is_some())
}

fn formula_value(node: &XmlNode) -> Option<f64> {
    let formula = node.attr("fmla")?.trim();
    let value = formula.strip_prefix("val")?.trim();
    value.parse::<f64>().ok()
}

fn guide_value(av: &XmlNode, name: &str) -> Option<f64> {
    av.children_named("gd")
        .find(|gd| gd.attr("name") == Some(name))
        .and_then(formula_value)
}

fn guide_or(av: &XmlNode, name: &str, fallback: f64) -> f64 {
    guide_value(av, name).unwrap_or(fallback)
}

fn fraction(value: f64) -> f32 {
    #[allow(clippy::cast_possible_truncation)]
    let value = (value / 100_000.0) as f32;
    value
}

fn degrees(value: f64) -> f32 {
    #[allow(clippy::cast_possible_truncation)]
    let value = (value / 60_000.0 * std::f64::consts::PI / 180.0) as f32;
    value
}

/// Map a preset's `a:avLst` to CourseCraft adjustment values.
///
/// The units differ per preset: most are 1/100000 of `min(w, h)`, star inner
/// radii are 1/50000 of the outer radius, and angles are 60000ths of a degree.
/// Unknown presets and presets with no guides fall back to the defaults.
fn map_adjust(kind: ShapeKind, preset: &str, av: Option<&XmlNode>) -> Vec<f32> {
    let Some(av) = av else {
        return Vec::new();
    };
    if !has_adjustment_value(Some(av)) {
        return Vec::new();
    }
    let value = |name: &str, fallback: f64| guide_or(av, name, fallback);
    let raw: Vec<f32> = match preset {
        "roundRect" | "snip1Rect" | "round1Rect" => vec![fraction(value("adj", 16667.0))],
        "snip2SameRect" | "snip2DiagRect" | "snipRoundRect" | "round2SameRect"
        | "round2DiagRect" => vec![
            fraction(value("adj1", 16667.0)),
            fraction(value("adj2", 16667.0)),
        ],
        "parallelogram" | "trapezoid" | "hexagon" | "octagon" | "plus" => {
            vec![fraction(value("adj", 25000.0))]
        }
        "rightArrow" | "leftArrow" | "upArrow" | "downArrow" | "leftRightArrow" | "upDownArrow" => {
            vec![
                fraction(value("adj1", 50000.0)),
                fraction(value("adj2", 50000.0)) * 0.5,
            ]
        }
        "homePlate" | "chevron" => vec![fraction(value("adj", 50000.0))],
        "wedgeRectCallout" | "wedgeRoundRectCallout" | "wedgeEllipseCallout" => vec![
            0.5 + fraction(value("adj1", -20833.0)),
            0.5 + fraction(value("adj2", 62500.0)),
        ],
        "callout1" | "borderCallout1" | "accentCallout1" | "accentBorderCallout1" => vec![
            0.5 + fraction(value("adj1", -12500.0)),
            0.5 + fraction(value("adj2", 93750.0)),
        ],
        "callout2" | "borderCallout2" | "accentCallout2" | "accentBorderCallout2" => vec![
            0.5 + fraction(value("adj1", -12500.0)),
            0.5 + fraction(value("adj2", 62500.0)),
            0.5 + fraction(value("adj3", -12500.0)),
            0.5 + fraction(value("adj4", 93750.0)),
        ],
        "callout3" | "borderCallout3" | "accentCallout3" | "accentBorderCallout3" => vec![
            0.5 + fraction(value("adj1", -12500.0)),
            0.5 + fraction(value("adj2", 62500.0)),
            0.5 + fraction(value("adj3", -12500.0)),
            0.5 + fraction(value("adj4", 62500.0)),
            0.5 + fraction(value("adj5", -12500.0)),
            0.5 + fraction(value("adj6", 93750.0)),
        ],
        "donut" | "frame" | "bevel" | "foldedCorner" | "can" | "cube" | "leftBracket"
        | "rightBracket" | "leftBrace" | "rightBrace" => {
            vec![fraction(value("adj", 25000.0))]
        }
        "star4" | "star5" | "star6" | "star7" | "star8" | "star10" | "star12" | "star16" => {
            let ratio = value("adj", 50000.0) / 50_000.0;
            #[allow(clippy::cast_possible_truncation)]
            let ratio = ratio as f32;
            vec![ratio]
        }
        "pie" | "chord" => vec![
            degrees(value("adj1", 0.0)),
            degrees(value("adj2", 16_200_000.0)),
        ],
        "arc" => vec![
            degrees(value("adj1", 16_200_000.0)),
            degrees(value("adj2", 0.0)),
        ],
        "blockArc" => vec![
            fraction(value("adj3", 25000.0)),
            degrees(value("adj1", 10_800_000.0)),
            degrees(value("adj2", 0.0)),
        ],
        _ => return Vec::new(),
    };
    // A callout's wedge is wound from start to end, so keep the end ahead.
    let mut raw = raw;
    if matches!(preset, "pie" | "chord" | "arc")
        && let (Some(start), Some(end)) = (raw.first().copied(), raw.get_mut(1))
        && start.is_finite()
        && end.is_finite()
        && *end < start
    {
        *end += ((start - *end) / std::f32::consts::TAU).ceil() * std::f32::consts::TAU;
    } else if preset == "blockArc"
        && let (Some(start), Some(end)) = (raw.get(1).copied(), raw.get_mut(2))
        && start.is_finite()
        && end.is_finite()
        && *end < start
    {
        *end += ((start - *end) / std::f32::consts::TAU).ceil() * std::f32::consts::TAU;
    }
    coursecraft_course::clamp_adjust(kind, &raw)
}

/// Read `<a:ln>` into an [`Outline`]. A missing line, `<a:noFill/>` inside it, or
/// an empty `<a:ln>` that sets neither a colour nor a width means no outline. Widths are EMU, converted to slide pixels (96 dpi).
fn extract_outline(sp_pr: &XmlNode, layout: SlideLayout) -> Option<Outline> {
    let ln = sp_pr.child("ln")?;
    if ln.child("noFill").is_some() {
        return None;
    }
    if ln.child("solidFill").is_none() && ln.attr("w").is_none() {
        return None;
    }
    let color = ln
        .child("solidFill")
        .and_then(|solid| solid.child("srgbClr"))
        .and_then(srgb_color)
        .unwrap_or_else(|| Color("#000000".to_string()));
    // PowerPoint stores line width in EMU; `layout.scale` converts EMU to the
    // 1280x720 slide pixels every other frame measurement uses.
    let width = ln
        .attr("w")
        .and_then(|w| w.trim().parse::<f64>().ok())
        .filter(|w| w.is_finite() && *w >= 0.0)
        .map_or(1.0, |emu| (emu * layout.scale) as f32)
        .clamp(0.0, 50.0);
    let dash = match ln.child("prstDash").and_then(|dash| dash.attr("val")) {
        Some("dash" | "lgDash" | "sysDash") => DashStyle::Dash,
        Some("dot" | "sysDot") => DashStyle::Dot,
        _ => DashStyle::Solid,
    };
    Some(Outline { color, width, dash })
}

/// Extract text runs and styling from `<p:txBody>`.
fn extract_text_runs(
    tx_body: &XmlNode,
    layout: SlideLayout,
    links: Option<TextLinkContext<'_>>,
) -> ExtractedText {
    let mut valign = VAlign::Top;
    let mut autofit = Autofit::None;
    if let Some(body_pr) = tx_body.child("bodyPr") {
        match body_pr.attr("anchor") {
            Some("ctr") => valign = VAlign::Middle,
            Some("b") => valign = VAlign::Bottom,
            _ => valign = VAlign::Top,
        }
        if body_pr.child("normAutofit").is_some() {
            autofit = Autofit::Shrink;
        }
    }

    let mut align = Align::Left;
    let mut first_paragraph = true;
    let mut runs: Vec<TextRun> = Vec::new();
    let mut paragraphs: Vec<Paragraph> = Vec::new();

    for p in tx_body.children_named("p") {
        let p_pr = p.child("pPr");
        if first_paragraph {
            if let Some(p_pr) = p_pr {
                match p_pr.attr("algn") {
                    Some("ctr") => align = Align::Center,
                    Some("r") => align = Align::Right,
                    _ => align = Align::Left,
                }
            }
            first_paragraph = false;
        } else {
            // Join paragraphs with \n
            if let Some(last) = runs.last_mut() {
                last.text.push('\n');
            } else {
                runs.push(TextRun {
                    text: "\n".to_string(),
                    bold: None,
                    italic: None,
                    underline: None,
                    size: None,
                    color: None,
                    link: None,
                    font: None,
                    strikethrough: None,
                    baseline: None,
                    letter_spacing: None,
                });
            }
        }

        paragraphs.push(paragraph_from(p_pr, layout));

        for child in &p.children {
            match child.name.as_str() {
                "r" => {
                    let mut bold = None;
                    let mut italic = None;
                    let mut underline = None;
                    let mut size = None;
                    let mut color = None;
                    let mut link = None;
                    let mut font = None;

                    if let Some(r_pr) = child.child("rPr") {
                        if let Some(typeface) = r_pr
                            .child("latin")
                            .and_then(|latin| latin.attr("typeface"))
                            .map(str::trim)
                            .filter(|name| !name.is_empty())
                        {
                            font = Some(typeface.to_string());
                        }
                        if let Some(b) = r_pr.attr("b") {
                            bold = Some(b == "1" || b == "true");
                        }
                        if let Some(i) = r_pr.attr("i") {
                            italic = Some(i == "1" || i == "true");
                        }
                        if let Some(u) = r_pr.attr("u") {
                            underline = Some(u != "none");
                        }
                        if let Some(sz_str) = r_pr.attr("sz")
                            && let Ok(sz_val) = sz_str.trim().parse::<f64>()
                        {
                            let pt = sz_val / 100.0;
                            let px = (pt * (4.0 / 3.0) * layout.font_scale()).clamp(1.0, 400.0);
                            if pt > 0.0 && px.is_finite() {
                                size = Some(px as f32);
                            }
                        }
                        if let Some(solid) = r_pr.child("solidFill")
                            && let Some(srgb) = solid.child("srgbClr")
                            && let Some(val) = srgb.attr("val")
                        {
                            let col = Color(format!("#{val}"));
                            if col.is_valid() {
                                color = Some(col);
                            }
                        }
                        link = links.and_then(|links| resolve_run_link(r_pr, links));
                    }

                    let t_text = child.child("t").map(|t| t.text.as_str()).unwrap_or("");
                    if !t_text.is_empty() {
                        runs.push(TextRun {
                            text: t_text.to_string(),
                            bold,
                            italic,
                            underline,
                            size,
                            color,
                            link,
                            font,
                            strikethrough: None,
                            baseline: None,
                            letter_spacing: None,
                        });
                    }
                }
                "br" => {
                    if let Some(last) = runs.last_mut() {
                        last.text.push('\n');
                    } else {
                        runs.push(TextRun {
                            text: "\n".to_string(),
                            bold: None,
                            italic: None,
                            underline: None,
                            size: None,
                            color: None,
                            link: None,
                            font: None,
                            strikethrough: None,
                            baseline: None,
                            letter_spacing: None,
                        });
                    }
                }
                _ => {}
            }
        }
    }

    let concat_text: String = runs.iter().map(|r| r.text.as_str()).collect();
    let has_text = !concat_text.trim().is_empty();

    // Paragraph default: the first explicit run size, else the course default.
    let default_size = runs.iter().find_map(|r| r.size).unwrap_or(16.0);
    let style = TextStyle {
        font: "Inter".to_string(),
        size: default_size,
        color: Color("#1F1F1F".to_string()),
        bold: false,
        italic: false,
        underline: false,
        align,
        valign,
        strikethrough: false,
        baseline: coursecraft_course::Baseline::Normal,
        letter_spacing: 0.0,
    };

    ExtractedText {
        text: concat_text,
        style,
        runs,
        paragraphs,
        autofit,
        has_text,
    }
}

/// Read one paragraph's list, level and spacing.
fn paragraph_from(p_pr: Option<&XmlNode>, layout: SlideLayout) -> Paragraph {
    let Some(p_pr) = p_pr else {
        return Paragraph::default();
    };
    let level = p_pr
        .attr("lvl")
        .and_then(|value| value.trim().parse::<u8>().ok())
        .map(|value| value.min(4))
        .unwrap_or(0);
    let list = if p_pr.child("buNone").is_some() {
        ListKind::None
    } else if p_pr.child("buChar").is_some() {
        ListKind::Bullet
    } else if p_pr.child("buAutoNum").is_some() {
        ListKind::Number
    } else {
        ListKind::None
    };
    let line_spacing = p_pr
        .child("lnSpc")
        .and_then(|spc| spc.child("spcPct"))
        .and_then(|pct| pct.attr("val"))
        .and_then(|value| value.trim().parse::<f64>().ok())
        .map(|value| (value / 100_000.0) as f32)
        .filter(|value| value.is_finite())
        .map(|value| value.clamp(0.8, 3.0))
        .unwrap_or(1.0);
    let space = |name: &str| -> f32 {
        p_pr.child(name)
            .and_then(|spc| spc.child("spcPts"))
            .and_then(|pts| pts.attr("val"))
            .and_then(|value| value.trim().parse::<f64>().ok())
            .filter(|value| value.is_finite() && *value >= 0.0)
            .map(|value| (value / 100.0 * (4.0 / 3.0) * layout.font_scale()) as f32)
            .map(|value| value.clamp(0.0, 200.0))
            .unwrap_or(0.0)
    };
    Paragraph {
        list,
        level,
        line_spacing,
        space_before: space("spcBef"),
        space_after: space("spcAft"),
    }
}

/// Resolve a run's `a:hlinkClick` to a URL or a slide in this import.
fn resolve_run_link(r_pr: &XmlNode, links: TextLinkContext<'_>) -> Option<Link> {
    let hlink = r_pr.child("hlinkClick")?;
    let action = hlink.attr("action").unwrap_or_default();
    let rid = hlink.attr("r:id").or_else(|| hlink.attr("id"));
    let rel = rid.and_then(|id| links.slide_rels.get(id));
    if action == "ppaction://hlinksldjump"
        && let Some(rel) = rel
        && let Some(target) = rel.target.as_deref()
        && let Some(slide) = links.slide_parts.get(target)
    {
        return Some(Link::Slide {
            slide: slide.clone(),
        });
    }
    if let Some(url) = rel.and_then(|rel| rel.external_target.clone())
        && is_valid_link_url(&url)
    {
        return Some(Link::Url { url });
    }
    None
}

/// Map a picture's `a:srcRect` onto the crop rect CourseCraft stores.
///
/// PowerPoint gives the fraction cropped off each side in thousandths of a
/// percent. The result is the kept window of the source; anything that does not
/// leave a positive, finite window inside the picture is ignored (no crop).
fn src_rect_crop(blip_fill: &XmlNode) -> Option<coursecraft_course::CropRect> {
    let src_rect = blip_fill.child("srcRect")?;
    // A side that is missing or unreadable counts as nothing cropped there.
    let side = |name: &str| -> Option<f32> {
        match src_rect.attr(name) {
            None => Some(0.0),
            Some(v) => v
                .trim()
                .parse::<f64>()
                .ok()
                .filter(|v| v.is_finite())
                .filter(|v| (0.0..=100_000.0).contains(v))
                .map(|v| (v / 100_000.0) as f32),
        }
    };
    let left = side("l")?;
    let top = side("t")?;
    let right = side("r")?;
    let bottom = side("b")?;
    // Values are applied in the order PowerPoint does, each clamped so the crop
    // never leaves the picture. Only what changes the window is kept.
    let x = left.min(1.0);
    let y = top.min(1.0);
    let w = 1.0 - x - right.min(1.0 - x);
    let h = 1.0 - y - bottom.min(1.0 - y);
    let crop = coursecraft_course::CropRect { x, y, w, h };
    // A crop that keeps the whole picture is the same as no crop.
    if !crop.is_valid() || crop.is_full() {
        return None;
    }
    Some(crop)
}

/// Resolve a picture relationship to its archive bytes.
fn relationship_picture<'a>(
    embed_id: &str,
    cx: &'a SlideContext<'_>,
) -> Option<(&'a str, &'a Vec<u8>)> {
    let rel = cx.slide_rels.get(embed_id)?;
    if rel.is_external {
        return None;
    }
    let target = rel.target.as_deref()?;
    let bytes = cx.entries.get(target)?;
    Some((target, bytes))
}

fn is_svg_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".svg") || lower.ends_with(".svgz")
}

fn picture_bytes_are_valid(path: &str, bytes: &[u8]) -> bool {
    if is_svg_path(path) {
        coursecraft_media::decode_svg(bytes, (32, 32), &coursecraft_media::Limits::default())
            .is_ok()
    } else {
        coursecraft_media::decode_image(bytes, &coursecraft_media::Limits::default()).is_ok()
    }
}

/// Pick the picture part for a `a:blip`: an `asvg:svgBlip` vector part wins
/// over the PNG fallback that older PowerPoint versions need.
fn preferred_picture(blip: &XmlNode, cx: &SlideContext<'_>) -> Option<(String, Vec<u8>)> {
    if let Some(svg) = blip.find_descendant("svgBlip")
        && let Some(embed_id) = svg.attr("r:embed").or_else(|| svg.attr("embed"))
        && let Some((path, bytes)) = relationship_picture(embed_id, cx)
        && is_svg_path(path)
        && picture_bytes_are_valid(path, bytes)
    {
        return Some((path.to_owned(), bytes.clone()));
    }

    let embed_id = blip.attr("r:embed").or_else(|| blip.attr("embed"))?;
    let (path, bytes) = relationship_picture(embed_id, cx)?;
    picture_bytes_are_valid(path, bytes).then(|| (path.to_owned(), bytes.clone()))
}

/// Process a `<p:pic>` picture element.
fn process_picture(pic: &XmlNode, transform: GroupTransform, cx: &mut SlideContext<'_>) {
    // Check for media (video or audio) inside pic
    if pic.find_descendant("videoFile").is_some() || pic.find_descendant("audioFile").is_some() {
        cx.tracker
            .report_skip(SkipKind::Media, "Audio/video picture skipped");
        return;
    }

    let c_nv = pic.child("nvPicPr").and_then(|nv| nv.child("cNvPr"));
    let name = c_nv.and_then(|c| c.attr("name")).unwrap_or("").to_string();
    let descr = c_nv.and_then(|c| c.attr("descr")).unwrap_or("").to_string();
    let alt = if !descr.trim().is_empty() {
        descr
    } else {
        name.clone()
    };

    // Frame
    let Some(sp_pr) = pic.child("spPr") else {
        return;
    };
    let Some(xfrm) = sp_pr.child("xfrm") else {
        return;
    };

    let (own_flip_h, own_flip_v) = xfrm_flips(Some(xfrm));
    let rotation = transform.rotation + xfrm_rotation(Some(xfrm));
    let flip_h = transform.flip_h ^ own_flip_h;
    let flip_v = transform.flip_v ^ own_flip_v;

    let (Some(off_node), Some(ext_node)) = (xfrm.child("off"), xfrm.child("ext")) else {
        cx.tracker
            .report_skip(SkipKind::InvalidGeometry, "Picture missing geometry");
        return;
    };

    let (Some(emu_x), Some(emu_y), Some(emu_w), Some(emu_h)) = (
        off_node.attr("x").and_then(parse_emu_offset),
        off_node.attr("y").and_then(parse_emu_offset),
        ext_node.attr("cx").and_then(parse_emu),
        ext_node.attr("cy").and_then(parse_emu),
    ) else {
        cx.tracker.report_skip(
            SkipKind::InvalidGeometry,
            "Picture geometry invalid or huge",
        );
        return;
    };

    let (root_x, root_y, root_w, root_h) = transform.apply(emu_x, emu_y, emu_w, emu_h);
    let frame = cx.layout.to_slide_px(root_x, root_y, root_w, root_h);

    let Some(blip_fill) = pic.child("blipFill") else {
        return;
    };
    let Some(blip) = blip_fill.child("blip") else {
        return;
    };
    let Some(embed_id) = blip.attr("r:embed").or_else(|| blip.attr("embed")) else {
        return;
    };

    if !cx.slide_rels.contains_key(embed_id) {
        cx.tracker
            .report_skip(SkipKind::ExternalTarget, "Missing picture relationship");
        return;
    }

    if cx
        .slide_rels
        .get(embed_id)
        .is_some_and(|rel| rel.is_external)
    {
        cx.tracker
            .report_skip(SkipKind::ExternalTarget, "External picture target skipped");
        return;
    }

    let Some((target_path, pic_bytes)) = preferred_picture(blip, cx) else {
        cx.tracker.report_skip(
            SkipKind::UndecodablePicture,
            "Picture file not found or undecodable",
        );
        return;
    };

    let crop = src_rect_crop(blip_fill);

    let file_name = std::path::Path::new(&target_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("image.png");
    let safe_src = sanitize_media_name(file_name, cx.existing_media_names);
    cx.existing_media_names.insert(safe_src.clone());
    cx.imported_media.push((safe_src.clone(), pic_bytes));

    cx.tracker.image_count = cx.tracker.image_count.saturating_add(1);
    let obj_id = format!("image{}", cx.tracker.image_count);
    let obj_name = if name.is_empty() {
        format!("Picture {}", cx.tracker.image_count)
    } else {
        name
    };

    cx.objects.push(Object {
        id: obj_id,
        name: obj_name,
        kind: ObjectKind::Image {
            src: safe_src,
            alt,
            crop,
        },
        frame,
        timing: Timing::default(),
        states: Vec::new(),
        initial_state: "normal".to_string(),
        entrance: None,
        exit: None,
        motion_paths: Vec::new(),
        focus_order: None,
        decorative: false,
        description: None,
        group: None,
        panel: None,
        rotation: normalized_pptx_rotation(rotation),
        flip_h,
        flip_v,
        opacity: 1.0,
        shadow: None,
        glow: None,
        drag: None,
    });
}
