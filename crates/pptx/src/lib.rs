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

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Read as _;
use std::sync::Arc;

use coursecraft_course::{
    Align, Autofit, Baseline, Color, ConnectorStyle, Course, DashStyle, FillStyle, Glow, Glue,
    GradientStop, LineEnd, LineJoin, Link, ListKind, MAX_OBJECT_FRAME_SIZE, NumberScheme, Object,
    ObjectKind, Outline, Paragraph, Path, PathPoint, PathStyle, PictureFillMode, Point, Rect,
    SCHEMA_VERSION, Scene, Shadow, ShapeKind, ShapeText, Slide, TableBorders, TableCell, TableRow,
    TextRun, TextStyle, Timing, VAlign, is_valid_link_url, normalize_table_fractions,
    path_frame_and_points,
};

/// A point in the coordinate space of one geometry helper.
type Pt = (f32, f32);

#[doc(hidden)]
pub use zip;

mod docx;
mod theme;
mod timing;

use theme::{ThemePalette, resolve_first_color_child, resolve_solid_fill};

pub use docx::{DocxError, MAX_DOCX_BYTES, MAX_DOCX_ENTRIES, MAX_DOCX_XML_DEPTH, docx_to_text};

#[cfg(test)]
mod bughunt_m26_a13_pptx;
#[cfg(test)]
mod m26_fidelity_tests;
#[cfg(test)]
mod m27_anim_tests;
#[cfg(test)]
mod m27_table_tests;
#[cfg(test)]
mod m39_t_tests;
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
    /// EMF or WMF clip art imported as SVG with part of it left out
    /// (records the reader does not play, or characters no font draws).
    MetafilePart,
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
            Self::MetafilePart => write!(f, "Partly read clip art"),
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
                SkipKind::MetafilePart => "partly read clip art",
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
                "\nAdd these fonts to keep the look: {} (shown as Inter).",
                self.missing_fonts.join(", ")
            )
        }
    }

    /// Remove any font family from `missing_fonts` that is installed on the
    /// user's computer (used by desktop/CLI shells after `import_pptx`).
    pub fn filter_installed_fonts(&mut self, is_installed: impl Fn(&str) -> bool) {
        self.missing_fonts.retain(|font| !is_installed(font));
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
            Ok(quick_xml::events::Event::GeneralRef(reference)) => {
                if let Some(parent) = stack.last_mut() {
                    if let Some(c) =
                        reference
                            .resolve_char_ref()
                            .map_err(|e| PptxError::MalformedXml {
                                part: part_name.to_string(),
                                message: e.to_string(),
                            })?
                    {
                        parent.text.push(c);
                    } else {
                        let name = reference.decode().map_err(|e| PptxError::MalformedXml {
                            part: part_name.to_string(),
                            message: e.to_string(),
                        })?;
                        let ch = match name.as_ref() {
                            "lt" => '<',
                            "gt" => '>',
                            "amp" => '&',
                            "apos" => '\'',
                            "quot" => '"',
                            other => {
                                return Err(PptxError::MalformedXml {
                                    part: part_name.to_string(),
                                    message: format!("unknown entity &{other};"),
                                });
                            }
                        };
                        parent.text.push(ch);
                    }
                }
            }
            Ok(quick_xml::events::Event::CData(cdata)) => {
                if let Some(parent) = stack.last_mut() {
                    let decoded = cdata.decode().map_err(|e| PptxError::MalformedXml {
                        part: part_name.to_string(),
                        message: e.to_string(),
                    })?;
                    parent.text.push_str(&decoded);
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
    slide_size: coursecraft_course::Size,
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

        let (w, h) = if valid_cx >= valid_cy {
            let w = 1280_u32;
            let h = ((1280.0 * valid_cy / valid_cx).round() as u32).clamp(1, 8192);
            (w, h)
        } else {
            let h = 1280_u32;
            let w = ((1280.0 * valid_cx / valid_cy).round() as u32).clamp(1, 8192);
            (w, h)
        };
        let scale = f64::from(w) / valid_cx;

        Self {
            slide_size: coursecraft_course::Size { w, h },
            scale,
            offset_x: 0.0,
            offset_y: 0.0,
            aspect_ratio_mismatch: false,
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

    /// Resolve the child's `(rotation_for_paint, flip_h, flip_v)` in slide space,
    /// accounting for parent group reflection + rotation and converting OOXML's
    /// `R(theta) * F` order to `paint.rs`'s `F * R(alpha)` order.
    fn resolve_rotation_and_flips(
        &self,
        own_rotation: f64,
        own_flip_h: bool,
        own_flip_v: bool,
    ) -> (f64, bool, bool) {
        let radians = own_rotation.to_radians();
        let (sin, cos) = radians.sin_cos();
        let fx = if own_flip_h { -1.0 } else { 1.0 };
        let fy = if own_flip_v { -1.0 } else { 1.0 };
        let m00 = cos * fx;
        let m01 = -sin * fy;
        let m10 = sin * fx;
        let m11 = cos * fy;
        let a = self.a * m00 + self.c * m10;
        let b = self.b * m00 + self.d * m10;
        let c = self.a * m01 + self.c * m11;
        let d = self.b * m01 + self.d * m11;
        let det = a * d - b * c;
        if det >= 0.0 {
            let flip_h = self.flip_h ^ own_flip_h;
            let flip_v = self.flip_v ^ own_flip_v;
            let theta = b.atan2(a).to_degrees();
            if flip_h && flip_v {
                // Both flipped is a 180-degree rotation; keep both flags off and use theta.
                (theta, false, false)
            } else {
                (theta, false, false)
            }
        } else {
            let prefer_v = (self.flip_v ^ own_flip_v) && !(self.flip_h ^ own_flip_h);
            if prefer_v {
                let theta = b.atan2(a).to_degrees();
                (-theta, false, true)
            } else {
                let theta = (-b).atan2(-a).to_degrees();
                (-theta, true, false)
            }
        }
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
    /// DrawingML shape id (`cNvPr/@id`) to imported object ids (may be multiple for
    /// multi-subpath custom geometry or flattened `grpSp` groups).
    pptx_ids: HashMap<String, Vec<String>>,
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

    fn report_animation(&mut self, note: impl Into<String>) {
        self.seen_skips.insert(SkipKind::Animation);
        self.items.push(ReportItem {
            slide: self.slide_idx,
            kind: SkipKind::Animation,
            note: note.into(),
        });
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

/// Inherited `bodyPr` and `lstStyle`/`txStyles` nodes for text extraction.
#[derive(Default)]
struct TextInheritance {
    body_pr: Vec<XmlNode>,
    lst_styles: Vec<XmlNode>,
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
fn slide_layout_from_presentation(pres_tree: &XmlNode) -> SlideLayout {
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
    SlideLayout::from_emu(emu_cx, emu_cy)
}

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

    let mut names: HashSet<String> = HashSet::new();
    for i in 0..archive.len() {
        let file = archive.by_index(i)?;
        let name = file.name()?.to_string();

        if !names.insert(name.clone()) {
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
    }

    let entries = ZipEntries::new(archive, names, limits);

    let pres_xml_bytes = entries
        .get_checked("ppt/presentation.xml")?
        .ok_or(PptxError::MissingPresentationXml)?;
    let pres_xml_str =
        std::str::from_utf8(&pres_xml_bytes).map_err(|e| PptxError::MalformedXml {
            part: "ppt/presentation.xml".to_string(),
            message: e.to_string(),
        })?;

    let pres_rels = if let Some(rels_bytes) =
        entries.get_checked("ppt/_rels/presentation.xml.rels")?
    {
        let rels_str = std::str::from_utf8(&rels_bytes).map_err(|e| PptxError::MalformedXml {
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

    let layout = slide_layout_from_presentation(&pres_tree);

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

        let Some(slide_xml_bytes) = entries.get_checked(&sld_ref.part_path)? else {
            tracker.report_skip(
                SkipKind::ExternalTarget,
                format!("Missing slide part {}", sld_ref.part_path),
            );
            report.items.extend(tracker.items);
            continue;
        };

        let slide_xml_str =
            std::str::from_utf8(&slide_xml_bytes).map_err(|e| PptxError::MalformedXml {
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

        let slide_rels = if let Some(rels_bytes) = entries.get_checked(&rels_path)? {
            let rels_str =
                std::str::from_utf8(&rels_bytes).map_err(|e| PptxError::MalformedXml {
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

        // Extract slide notes if present
        let mut slide_notes = String::new();
        for rel in slide_rels.values() {
            if let Some(ref target) = rel.target
                && target.contains("notesSlide")
                && let Some(notes_bytes) = entries.get(target)
                && let Ok(notes_str) = std::str::from_utf8(&notes_bytes)
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
            .filter(|t| t.starts_with("ppt/slideLayouts/") && entries.contains_key(t))
            .min()
            .map(str::to_string);

        let theme_palette = resolver.resolve_palette(&entries, layout_path.as_deref(), &slide_tree);

        let (slide_background, bg_object) = extract_slide_background(
            &mut resolver,
            &entries,
            &slide_tree,
            &slide_rels,
            layout_path.as_deref(),
            &layout,
            &theme_palette,
            &mut imported_media,
            &mut existing_media_names,
            &mut tracker,
            &limits,
            &slide_parts,
        );

        let mut slide_title: Option<String> = None;
        let mut objects: Vec<Object> = Vec::new();
        if let Some(bg) = bg_object {
            objects.push(bg);
        }

        let table_styles = parse_table_styles(&entries, &theme_palette);

        let slide_hide_master = slide_tree
            .attr("showMasterSp")
            .is_some_and(|s| s == "0" || s == "false");
        let layout_tree_opt = layout_path
            .as_deref()
            .and_then(|lp| resolver.tree(&entries, lp).cloned());
        let layout_hide_master = layout_tree_opt.as_ref().is_some_and(|lt| {
            lt.attr("showMasterSp")
                .is_some_and(|s| s == "0" || s == "false")
        });
        if !slide_hide_master && !layout_hide_master {
            let master_path = layout_path
                .as_deref()
                .and_then(|lp| resolver.master_of(&entries, lp))
                .or_else(|| {
                    entries
                        .contains_key("ppt/slideMasters/slideMaster1.xml")
                        .then(|| "ppt/slideMasters/slideMaster1.xml".to_string())
                });
            if let Some(mp) = master_path
                && let Some(master_tree) = resolver.tree(&entries, &mp).cloned()
                && let Some(sp_tree) = master_tree.find_descendant("spTree")
            {
                let master_rels = resolver.rels_of(&entries, &mp);
                let mut mcx = SlideContext {
                    layout: &layout,
                    slide_rels: &master_rels,
                    entries: &entries,
                    objects: &mut objects,
                    slide_title: &mut None,
                    imported_media: &mut imported_media,
                    existing_media_names: &mut existing_media_names,
                    tracker: &mut tracker,
                    limits: &limits,
                    resolver: &mut resolver,
                    layout_path: layout_path.clone(),
                    slide_parts: &slide_parts,
                    theme: &theme_palette,
                    table_styles: &table_styles,
                };
                process_non_placeholder_children(sp_tree, GroupTransform::default(), 0, &mut mcx)?;
            }
            // Shape ids are numbered per XML part. Settle this part's
            // connectors and forget its ids so a slide id such as 2 cannot
            // reach a master shape with the same id (N52).
            resolve_connector_glues(&mut objects, &mut tracker);
            tracker.pptx_ids.clear();
        }
        if let Some(lp) = layout_path.as_deref()
            && let Some(layout_tree) = layout_tree_opt.as_ref()
            && let Some(sp_tree) = layout_tree.find_descendant("spTree")
        {
            let layout_rels = resolver.rels_of(&entries, lp);
            let mut lcx = SlideContext {
                layout: &layout,
                slide_rels: &layout_rels,
                entries: &entries,
                objects: &mut objects,
                slide_title: &mut None,
                imported_media: &mut imported_media,
                existing_media_names: &mut existing_media_names,
                tracker: &mut tracker,
                limits: &limits,
                resolver: &mut resolver,
                layout_path: layout_path.clone(),
                slide_parts: &slide_parts,
                theme: &theme_palette,
                table_styles: &table_styles,
            };
            process_non_placeholder_children(sp_tree, GroupTransform::default(), 0, &mut lcx)?;
        }
        resolve_connector_glues(&mut objects, &mut tracker);
        tracker.pptx_ids.clear();

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
                theme: &theme_palette,
                table_styles: &table_styles,
            };
            process_group_children(sp_tree, initial_xfrm, 0, &mut cx)?;
        }
        resolve_connector_glues(&mut objects, &mut tracker);

        let mut triggers: Vec<coursecraft_course::Trigger> = Vec::new();
        let max_anim_end_ms = slide_tree
            .find_descendant("timing")
            .map(|timing_node| {
                timing::apply_slide_timing(
                    timing_node,
                    &layout,
                    &mut objects,
                    &mut triggers,
                    &mut tracker,
                )
            })
            .unwrap_or(0);
        let slide_duration_ms = 5000u32
            .max(max_anim_end_ms.saturating_add(500))
            .min(coursecraft_course::MAX_TIMELINE_DURATION_MS);

        let final_title = slide_title
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| format!("Slide {slide_idx}"));

        let slide = Slide {
            id: format!("slide{slide_idx}"),
            title: final_title,
            duration_ms: slide_duration_ms,
            background: slide_background,
            objects,
            layers: Vec::new(),
            groups: Vec::new(),
            triggers,
            cue_points: Vec::new(),
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

    drop_links_to_unimported_slides(&mut imported_slides, &mut report.items);

    report.slides = imported_slides.len();

    let embedded_fonts = import_embedded_fonts(
        &pres_tree,
        &pres_rels,
        &entries,
        &mut imported_media,
        &mut report,
    );

    let (mut deck_fonts, major_font, minor_font) =
        collect_theme_fonts(&entries, limits.max_xml_depth);
    if let Some(err) = entries.take_error() {
        return Err(err);
    }
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
            !font.eq_ignore_ascii_case("Inter")
                && !font.eq_ignore_ascii_case("Open Sans")
                && !embedded_fonts
                    .iter()
                    .any(|face| face.family.eq_ignore_ascii_case(font))
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
        slide_size: layout.slide_size,
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
        tracking: Default::default(),
        player: coursecraft_course::PlayerSettings::default(),
        theme: coursecraft_course::Theme::default(),
        language: None,
        strings: std::collections::BTreeMap::new(),
        fonts: embedded_fonts,
        background_audio: None,
        glossary: Vec::new(),
        editor: Default::default(),
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

/// Parse `<p:embeddedFontLst>` from `ppt/presentation.xml`, de-obfuscating
/// `.fntdata` / `.odttf` parts using their ECMA-376 GUID key.
fn import_embedded_fonts(
    pres_tree: &XmlNode,
    pres_rels: &HashMap<String, Relationship>,
    entries: &Entries<'_>,
    imported_media: &mut Vec<(String, Vec<u8>)>,
    report: &mut ImportReport,
) -> Vec<coursecraft_course::FontFace> {
    use coursecraft_course::{FontFace, FontStyle};

    let Some(list) = pres_tree.find_descendant("embeddedFontLst") else {
        return Vec::new();
    };
    let mut faces = Vec::new();
    let mut used_paths: HashSet<String> = imported_media
        .iter()
        .map(|(p, _)| p.to_ascii_lowercase())
        .collect();

    for entry in list.children_named("embeddedFont") {
        let Some(family) = entry
            .child("font")
            .and_then(|n| n.attr("typeface"))
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
        else {
            continue;
        };

        let style_nodes = [
            ("regular", FontStyle::Regular),
            ("bold", FontStyle::Bold),
            ("italic", FontStyle::Italic),
            ("boldItalic", FontStyle::BoldItalic),
        ];
        for (tag, style) in style_nodes {
            let Some(node) = entry.child(tag) else {
                continue;
            };
            let Some(rid) = node.attr("r:id").or_else(|| node.attr("id")) else {
                continue;
            };
            let Some(target) = pres_rels.get(rid).and_then(|r| r.target.as_deref()) else {
                continue;
            };
            let Ok(Some(raw_bytes)) = entries.get_checked(target) else {
                report.items.push(ReportItem {
                    slide: 1,
                    kind: SkipKind::Media,
                    note: format!(
                        "Embedded font \"{family}\" ({}) could not be read",
                        style.as_str()
                    ),
                });
                continue;
            };
            let font_key = node.attr("fontKey");
            let Some(bytes) = decode_embedded_font_bytes(&raw_bytes, font_key, target) else {
                report.items.push(ReportItem {
                    slide: 1,
                    kind: SkipKind::Media,
                    note: format!(
                        "Embedded font \"{family}\" ({}) could not be read",
                        style.as_str()
                    ),
                });
                continue;
            };
            if faces
                .iter()
                .any(|f: &FontFace| f.family.eq_ignore_ascii_case(&family) && f.style == style)
            {
                continue;
            }
            let ext = if bytes.starts_with(b"OTTO") {
                "otf"
            } else {
                "ttf"
            };
            let slug = sanitize_font_slug(&family);
            let mut candidate = format!("fonts/{slug}-{}.{ext}", style.as_str());
            let mut n = 2usize;
            while used_paths.contains(&candidate.to_ascii_lowercase()) {
                candidate = format!("fonts/{slug}-{}-{n}.{ext}", style.as_str());
                n += 1;
            }
            used_paths.insert(candidate.to_ascii_lowercase());
            imported_media.push((candidate.clone(), bytes));
            faces.push(FontFace {
                family: family.clone(),
                style,
                src: candidate,
            });
        }
    }
    faces
}

fn sanitize_font_slug(family: &str) -> String {
    let mut slug = String::new();
    for ch in family.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
        } else if (ch == ' ' || ch == '-' || ch == '_') && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let trimmed = slug.trim_matches('-').to_owned();
    if trimmed.is_empty() {
        "font".to_owned()
    } else {
        trimmed
    }
}

fn parse_guid_bytes(text: &str) -> Option<[u8; 16]> {
    let hex: Vec<u8> = text.bytes().filter(|b| b.is_ascii_hexdigit()).collect();
    if hex.len() < 32 {
        return None;
    }
    // Search for a 32-hex-digit GUID run (in case filename has extra chars).
    let start = hex.len().saturating_sub(32);
    let slice = &hex[start..start + 32];
    let mut out = [0u8; 16];
    for i in 0..16 {
        let pair = std::str::from_utf8(&slice[i * 2..i * 2 + 2]).ok()?;
        out[i] = u8::from_str_radix(pair, 16).ok()?;
    }
    Some(out)
}

fn xor_font_header(raw: &[u8], key: &[u8; 16], reversed: bool) -> Vec<u8> {
    let mut out = raw.to_vec();
    let limit = out.len().min(32);
    for (i, byte) in out.iter_mut().take(limit).enumerate() {
        let k = if reversed {
            key[15 - (i % 16)]
        } else {
            key[i % 16]
        };
        *byte ^= k;
    }
    out
}

fn decode_embedded_font_bytes(
    raw: &[u8],
    font_key: Option<&str>,
    target_path: &str,
) -> Option<Vec<u8>> {
    if coursecraft_media::font_info(raw).is_ok() {
        return Some(raw.to_vec());
    }
    let key = font_key
        .and_then(parse_guid_bytes)
        .or_else(|| parse_guid_bytes(target_path))?;
    let reversed = xor_font_header(raw, &key, true);
    if coursecraft_media::font_info(&reversed).is_ok() {
        return Some(reversed);
    }
    let forward = xor_font_header(raw, &key, false);
    if coursecraft_media::font_info(&forward).is_ok() {
        return Some(forward);
    }
    None
}

/// Fonts named by the presentation's theme parts (`a:majorFont` and
/// `a:minorFont`).
fn collect_theme_fonts(
    entries: &Entries<'_>,
    max_depth: usize,
) -> (HashSet<String>, Option<String>, Option<String>) {
    let mut fonts = HashSet::new();
    let mut major = None;
    let mut minor = None;
    let theme_paths =
        entries.names_matching(|path| path.starts_with("ppt/theme/") && path.ends_with(".xml"));
    for path in theme_paths {
        let Some(bytes) = entries.get(&path) else {
            continue;
        };
        let Ok(text) = std::str::from_utf8(&bytes) else {
            continue;
        };
        let Ok(tree) = parse_xml_tree(text, &path, max_depth) else {
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
    let resolve_str = |font: &str| -> Option<String> {
        if !font.starts_with('+') {
            return None;
        }
        let lower = font.to_ascii_lowercase();
        let chosen = if lower.starts_with("+mj") {
            major
        } else if lower.starts_with("+mn") {
            minor
        } else {
            None
        };
        chosen.map(str::to_owned)
    };
    let resolve = |font: &mut Option<String>| {
        if let Some(token) = font.as_deref()
            && token.starts_with('+')
        {
            *font = resolve_str(token);
        }
    };
    let resolve_kind = |kind: &mut ObjectKind| match kind {
        ObjectKind::Text { style, runs, .. } => {
            if let Some(replacement) = resolve_str(&style.font) {
                style.font = replacement;
            }
            for run in runs {
                resolve(&mut run.font);
            }
        }
        ObjectKind::Shape {
            text: Some(text), ..
        } => {
            if let Some(replacement) = resolve_str(&text.style.font) {
                text.style.font = replacement;
            }
            for run in &mut text.runs {
                resolve(&mut run.font);
            }
        }
        ObjectKind::Table { rows, .. } => {
            for row in rows {
                for cell in &mut row.cells {
                    if let Some(replacement) = resolve_str(&cell.style.font) {
                        cell.style.font = replacement;
                    }
                    for run in &mut cell.runs {
                        resolve(&mut run.font);
                    }
                }
            }
        }
        _ => {}
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
    let mut push_name = |font: &str| {
        if !font.trim().is_empty()
            && !font.eq_ignore_ascii_case("Inter")
            && !font.eq_ignore_ascii_case("Open Sans")
        {
            fonts.insert(font.trim().to_owned());
        }
    };
    let mut add_kind = |kind: &ObjectKind| match kind {
        ObjectKind::Text { runs, style, .. } => {
            push_name(&style.font);
            for run in runs {
                if let Some(font) = &run.font {
                    push_name(font);
                }
            }
        }
        ObjectKind::Shape {
            text: Some(text), ..
        } => {
            push_name(&text.style.font);
            for run in &text.runs {
                if let Some(font) = &run.font {
                    push_name(font);
                }
            }
        }
        ObjectKind::Table { rows, .. } => {
            for row in rows {
                for cell in &row.cells {
                    push_name(&cell.style.font);
                    for run in &cell.runs {
                        if let Some(font) = &run.font {
                            push_name(font);
                        }
                    }
                }
            }
        }
        _ => {}
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

/// Package parts keyed by path, read lazily from the zip archive so unused
/// entries are never decompressed or buffered.
struct ZipEntries<'a> {
    archive: RefCell<zip::ZipArchive<std::io::Cursor<&'a [u8]>>>,
    names: HashSet<String>,
    cache: RefCell<HashMap<String, Arc<[u8]>>>,
    limits: Limits,
    total_bytes: Cell<u64>,
    error: RefCell<Option<PptxError>>,
}

impl<'a> ZipEntries<'a> {
    fn new(
        archive: zip::ZipArchive<std::io::Cursor<&'a [u8]>>,
        names: HashSet<String>,
        limits: Limits,
    ) -> Self {
        Self {
            archive: RefCell::new(archive),
            names,
            cache: RefCell::new(HashMap::new()),
            limits,
            total_bytes: Cell::new(0),
            error: RefCell::new(None),
        }
    }

    fn contains_key(&self, name: &str) -> bool {
        self.names.contains(name)
    }

    fn names_matching(&self, mut pred: impl FnMut(&str) -> bool) -> Vec<String> {
        self.names
            .iter()
            .filter(|name| pred(name.as_str()))
            .cloned()
            .collect()
    }

    fn get_checked(&self, name: &str) -> Result<Option<Arc<[u8]>>, PptxError> {
        if let Some(cached) = self.cache.borrow().get(name) {
            return Ok(Some(Arc::clone(cached)));
        }
        if !self.names.contains(name) {
            return Ok(None);
        }
        let mut archive = self.archive.borrow_mut();
        let file = match archive.by_name(name) {
            Ok(file) => file,
            Err(zip::result::ZipError::FileNotFound) => return Ok(None),
            Err(err) => return Err(err.into()),
        };
        if file.size() > self.limits.max_entry_bytes {
            return Err(PptxError::EntryTooLarge(name.to_string()));
        }
        let mut entry_bytes = Vec::new();
        let read_limit = self.limits.max_entry_bytes.saturating_add(1);
        file.take(read_limit).read_to_end(&mut entry_bytes)?;
        if entry_bytes.len() as u64 > self.limits.max_entry_bytes {
            return Err(PptxError::EntryTooLarge(name.to_string()));
        }
        let total = self
            .total_bytes
            .get()
            .checked_add(entry_bytes.len() as u64)
            .ok_or(PptxError::TooLarge)?;
        if total > self.limits.max_total_bytes {
            return Err(PptxError::TooLarge);
        }
        self.total_bytes.set(total);
        let arc: Arc<[u8]> = Arc::from(entry_bytes);
        self.cache
            .borrow_mut()
            .insert(name.to_string(), Arc::clone(&arc));
        Ok(Some(arc))
    }

    fn get(&self, name: &str) -> Option<Arc<[u8]>> {
        match self.get_checked(name) {
            Ok(bytes) => bytes,
            Err(err) => {
                let mut slot = self.error.borrow_mut();
                if slot.is_none() {
                    *slot = Some(err);
                }
                None
            }
        }
    }

    fn take_error(&self) -> Option<PptxError> {
        self.error.borrow_mut().take()
    }
}

type Entries<'a> = ZipEntries<'a>;

/// Resolves placeholder geometry from slide layouts and masters.
///
/// Each part is parsed at most once per import (cached by path). Only
/// `slide -> slideLayouts/* -> slideMasters/*` links are followed, so the
/// walk is bounded and cannot loop.
struct PlaceholderResolver {
    max_depth: usize,
    trees: HashMap<String, Option<XmlNode>>,
    masters: HashMap<String, Option<String>>,
    themes: HashMap<String, Option<String>>,
}

impl PlaceholderResolver {
    fn new(max_depth: usize) -> Self {
        Self {
            max_depth,
            trees: HashMap::new(),
            masters: HashMap::new(),
            themes: HashMap::new(),
        }
    }

    /// Parse (once) the part at `path`; `None` if missing or malformed.
    fn tree(&mut self, entries: &Entries<'_>, path: &str) -> Option<&XmlNode> {
        if !self.trees.contains_key(path) {
            let parsed = entries
                .get(path)
                .and_then(|b| std::str::from_utf8(&b).ok().map(str::to_owned))
                .and_then(|s| parse_xml_tree(&s, path, self.max_depth).ok());
            self.trees.insert(path.to_string(), parsed);
        }
        self.trees.get(path).and_then(Option::as_ref)
    }

    /// Parse relationships for `owner_path` if present.
    fn rels_of(&self, entries: &Entries<'_>, owner_path: &str) -> HashMap<String, Relationship> {
        let Some((folder, file)) = owner_path.rsplit_once('/') else {
            return HashMap::new();
        };
        let rels_path = format!("{folder}/_rels/{file}.rels");
        let Some(bytes) = entries.get(&rels_path) else {
            return HashMap::new();
        };
        let Ok(text) = std::str::from_utf8(&bytes) else {
            return HashMap::new();
        };
        parse_relationships(text, &rels_path, folder, self.max_depth).unwrap_or_default()
    }

    /// The first part under `prefix` that `owner_path` relates to.
    fn related_part(
        &self,
        entries: &Entries<'_>,
        owner_path: &str,
        prefix: &str,
    ) -> Option<String> {
        let rels = self.rels_of(entries, owner_path);
        rels.values()
            .filter_map(|r| r.target.as_deref())
            .filter(|t| t.starts_with(prefix) && entries.contains_key(t))
            .min()
            .map(str::to_string)
    }

    fn master_of(&mut self, entries: &Entries<'_>, layout_path: &str) -> Option<String> {
        if let Some(cached) = self.masters.get(layout_path) {
            return cached.clone();
        }
        let found = self.related_part(entries, layout_path, "ppt/slideMasters/");
        self.masters.insert(layout_path.to_string(), found.clone());
        found
    }

    fn theme_of(&mut self, entries: &Entries<'_>, master_path: &str) -> Option<String> {
        if let Some(cached) = self.themes.get(master_path) {
            return cached.clone();
        }
        let found = self
            .related_part(entries, master_path, "ppt/theme/")
            .or_else(|| {
                entries
                    .contains_key("ppt/theme/theme1.xml")
                    .then(|| "ppt/theme/theme1.xml".to_string())
            });
        self.themes.insert(master_path.to_string(), found.clone());
        found
    }

    /// Resolve the theme colour palette for a slide via its layout -> master -> theme.
    fn resolve_palette(
        &mut self,
        entries: &Entries<'_>,
        layout_path: Option<&str>,
        slide_tree: &XmlNode,
    ) -> ThemePalette {
        let master_path = layout_path
            .and_then(|lp| self.master_of(entries, lp))
            .or_else(|| {
                entries
                    .contains_key("ppt/slideMasters/slideMaster1.xml")
                    .then(|| "ppt/slideMasters/slideMaster1.xml".to_string())
            });
        let theme_path = master_path
            .as_deref()
            .and_then(|mp| self.theme_of(entries, mp))
            .or_else(|| {
                entries
                    .contains_key("ppt/theme/theme1.xml")
                    .then(|| "ppt/theme/theme1.xml".to_string())
            });

        let mut palette = theme_path
            .as_deref()
            .and_then(|tp| self.tree(entries, tp))
            .map(ThemePalette::from_theme_tree)
            .unwrap_or_default();

        if let Some(mp) = master_path.as_deref()
            && let Some(master_tree) = self.tree(entries, mp)
        {
            palette.apply_clr_map(master_tree);
        }
        if let Some(lp) = layout_path
            && let Some(layout_tree) = self.tree(entries, lp)
        {
            palette.apply_clr_map(layout_tree);
        }
        palette.apply_clr_map(slide_tree);
        palette
    }

    /// Resolve the frame for a placeholder (`type`, `idx`) via layout then master.
    fn resolve(
        &mut self,
        entries: &Entries<'_>,
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

    /// Collect inherited `bodyPr` and `lstStyle` / `txStyles` nodes for a shape.
    fn resolve_text_inheritance(
        &mut self,
        entries: &Entries<'_>,
        layout_path: Option<&str>,
        ph_type: Option<&str>,
        ph_idx: Option<&str>,
        is_placeholder: bool,
    ) -> TextInheritance {
        let mut body_pr = Vec::new();
        let mut lst_styles = Vec::new();

        let Some(lp) = layout_path else {
            return TextInheritance {
                body_pr,
                lst_styles,
            };
        };

        if is_placeholder
            && let Some(layout_tree) = self.tree(entries, lp)
            && let Some(sp) = find_placeholder_sp(layout_tree, ph_type, ph_idx, true)
            && let Some(tx_body) = sp.child("txBody")
        {
            if let Some(bp) = tx_body.child("bodyPr") {
                body_pr.push(bp.clone());
            }
            if let Some(ls) = tx_body.child("lstStyle") {
                lst_styles.push(ls.clone());
            }
        }

        let Some(mp) = self.master_of(entries, lp) else {
            return TextInheritance {
                body_pr,
                lst_styles,
            };
        };
        let Some(master_tree) = self.tree(entries, &mp) else {
            return TextInheritance {
                body_pr,
                lst_styles,
            };
        };

        if is_placeholder
            && let Some(sp) = find_placeholder_sp(master_tree, ph_type, ph_idx, false)
            && let Some(tx_body) = sp.child("txBody")
        {
            if let Some(bp) = tx_body.child("bodyPr") {
                body_pr.push(bp.clone());
            }
            if let Some(ls) = tx_body.child("lstStyle") {
                lst_styles.push(ls.clone());
            }
        }

        if let Some(tx_styles) = master_tree.find_descendant("txStyles") {
            let style_name = if !is_placeholder {
                "otherStyle"
            } else {
                match normalize_ph_type(ph_type, true) {
                    "title" => "titleStyle",
                    "body" => "bodyStyle",
                    _ => "otherStyle",
                }
            };
            if let Some(style_node) = tx_styles.child(style_name) {
                lst_styles.push(style_node.clone());
            }
        }

        TextInheritance {
            body_pr,
            lst_styles,
        }
    }

    /// Find the effective `p:bg` node and the relationships of the part that owns it.
    fn resolve_background(
        &mut self,
        entries: &Entries<'_>,
        slide_tree: &XmlNode,
        slide_rels: &HashMap<String, Relationship>,
        layout_path: Option<&str>,
    ) -> Option<(XmlNode, HashMap<String, Relationship>)> {
        if let Some(bg) = slide_tree.child("cSld").and_then(|c| c.child("bg")) {
            return Some((bg.clone(), slide_rels.clone()));
        }
        let lp = layout_path?;
        if let Some(layout_tree) = self.tree(entries, lp)
            && let Some(bg) = layout_tree.child("cSld").and_then(|c| c.child("bg"))
        {
            return Some((bg.clone(), self.rels_of(entries, lp)));
        }
        let mp = self.master_of(entries, lp)?;
        let master_tree = self.tree(entries, &mp)?;
        let bg = master_tree.child("cSld").and_then(|c| c.child("bg"))?;
        Some((bg.clone(), self.rels_of(entries, &mp)))
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

/// Find the matching placeholder `<p:sp>` in a layout/master tree.
fn find_placeholder_sp<'a>(
    tree: &'a XmlNode,
    ph_type: Option<&str>,
    ph_idx: Option<&str>,
    use_idx: bool,
) -> Option<&'a XmlNode> {
    let sp_tree = tree.find_descendant("spTree")?;
    let want_type = normalize_ph_type(ph_type, !use_idx);
    let find_matching = |by_idx: bool| {
        sp_tree.children_named("sp").find(move |sp| {
            let Some(ph) = sp
                .child("nvSpPr")
                .and_then(|nv| nv.child("nvPr"))
                .and_then(|np| np.child("ph"))
            else {
                return false;
            };
            if by_idx {
                ph_idx.is_some() && ph.attr("idx") == ph_idx
            } else {
                normalize_ph_type(ph.attr("type"), !use_idx) == want_type
            }
        })
    };
    if use_idx && let Some(sp) = find_matching(true) {
        return Some(sp);
    }
    find_matching(false)
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
struct SlideContext<'a, 'z> {
    layout: &'a SlideLayout,
    slide_rels: &'a HashMap<String, Relationship>,
    entries: &'a Entries<'z>,
    objects: &'a mut Vec<Object>,
    slide_title: &'a mut Option<String>,
    imported_media: &'a mut Vec<(String, Vec<u8>)>,
    existing_media_names: &'a mut HashSet<String>,
    tracker: &'a mut SlideTracker,
    limits: &'a Limits,
    resolver: &'a mut PlaceholderResolver,
    layout_path: Option<String>,
    slide_parts: &'a HashMap<String, String>,
    theme: &'a ThemePalette,
    table_styles: &'a TableStyleSet,
}

#[allow(clippy::too_many_arguments)]
fn extract_slide_background(
    resolver: &mut PlaceholderResolver,
    entries: &Entries<'_>,
    slide_tree: &XmlNode,
    slide_rels: &HashMap<String, Relationship>,
    layout_path: Option<&str>,
    layout: &SlideLayout,
    theme_palette: &ThemePalette,
    imported_media: &mut Vec<(String, Vec<u8>)>,
    existing_media_names: &mut HashSet<String>,
    tracker: &mut SlideTracker,
    limits: &Limits,
    slide_parts: &HashMap<String, String>,
) -> (Option<Color>, Option<Object>) {
    let Some((bg_node, bg_rels)) =
        resolver.resolve_background(entries, slide_tree, slide_rels, layout_path)
    else {
        return (None, None);
    };
    if let Some(bg_pr) = bg_node.child("bgPr") {
        if bg_pr.child("noFill").is_some() {
            return (None, None);
        }
        if let Some(color) = resolve_solid_fill(bg_pr, theme_palette) {
            return (Some(color), None);
        }
        if let Some(grad) = bg_pr.child("gradFill")
            && let Some(fill_style) = extract_gradient_fill(grad, theme_palette)
        {
            let fallback = match &fill_style {
                FillStyle::Linear { stops, .. } | FillStyle::Radial { stops, .. } => stops
                    .first()
                    .map(|s| s.color.clone())
                    .unwrap_or_else(|| Color("#FFFFFF".to_string())),
                FillStyle::Picture { .. } | FillStyle::Pattern { .. } => {
                    Color("#FFFFFF".to_string())
                }
            };
            return (None, Some(make_background_shape(fallback, fill_style)));
        }
        if let Some(blip_fill) = bg_pr.child("blipFill") {
            let mut dummy_title = None;
            let mut dummy_objects = Vec::new();
            let mut bg_cx = SlideContext {
                layout,
                slide_rels: &bg_rels,
                entries,
                objects: &mut dummy_objects,
                slide_title: &mut dummy_title,
                imported_media,
                existing_media_names,
                tracker,
                limits,
                resolver,
                layout_path: layout_path.map(str::to_string),
                slide_parts,
                theme: theme_palette,
                table_styles: &TableStyleSet::default(),
            };
            if let Some((src, mode, tile_scale)) = extract_picture_fill(blip_fill, &mut bg_cx) {
                return (
                    None,
                    Some(make_background_shape(
                        Color("#FFFFFF".to_string()),
                        FillStyle::Picture {
                            src,
                            mode,
                            tile_scale,
                        },
                    )),
                );
            }
        }
    } else if let Some(bg_ref) = bg_node.child("bgRef")
        && let Some(color) = resolve_first_color_child(bg_ref, theme_palette)
    {
        return (Some(color), None);
    }
    (None, None)
}

fn make_background_shape(fill: Color, fill_style: FillStyle) -> Object {
    Object {
        id: "slide-bg".to_string(),
        name: "Slide Background".to_string(),
        kind: ObjectKind::Shape {
            shape: ShapeKind::Rectangle,
            fill,
            fill_style: Some(fill_style),
            outline: None,
            text: None,
            adjust: Vec::new(),
        },
        frame: Rect {
            x: 0.0,
            y: 0.0,
            w: 1280.0,
            h: 720.0,
        },
        timing: Timing::default(),
        states: Vec::new(),
        initial_state: "normal".to_string(),
        entrance: None,
        exit: None,
        motion_paths: Vec::new(),
        emphasis: Vec::new(),
        focus_order: None,
        decorative: true,
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
        locked: true,
        editor_hidden: false,
    }
}

fn clamp_object_frame(frame: Rect, tracker: &mut SlideTracker) -> Rect {
    let max_dim = MAX_OBJECT_FRAME_SIZE;
    let x = if frame.x.is_finite() {
        frame.x.clamp(-max_dim, max_dim)
    } else {
        0.0
    };
    let y = if frame.y.is_finite() {
        frame.y.clamp(-max_dim, max_dim)
    } else {
        0.0
    };
    let mut w = if frame.w.is_finite() && frame.w > 0.0 {
        frame.w
    } else {
        1.0
    };
    let mut h = if frame.h.is_finite() && frame.h > 0.0 {
        frame.h
    } else {
        1.0
    };
    let clamped_pos = x != frame.x || y != frame.y;
    let oversized = w > max_dim || h > max_dim;
    if oversized {
        let scale = (max_dim / w.max(h)).min(1.0);
        w = (w * scale).clamp(1.0, max_dim);
        h = (h * scale).clamp(1.0, max_dim);
    }
    if clamped_pos || oversized {
        tracker.report_skip(
            SkipKind::InvalidGeometry,
            "Shape frame exceeded 100,000 px limit and was clamped",
        );
    }
    Rect { x, y, w, h }
}

fn drawing_id(node: &XmlNode) -> Option<&str> {
    node.child("nvSpPr")
        .and_then(|nv| nv.child("cNvPr"))
        .or_else(|| node.child("nvPicPr").and_then(|nv| nv.child("cNvPr")))
        .or_else(|| node.child("nvCxnSpPr").and_then(|nv| nv.child("cNvPr")))
        .or_else(|| node.child("nvGrpSpPr").and_then(|nv| nv.child("cNvPr")))
        .or_else(|| {
            node.child("nvGraphicFramePr")
                .and_then(|nv| nv.child("cNvPr"))
        })
        .and_then(|c| c.attr("id"))
}

fn record_pptx_id(node: &XmlNode, before: usize, cx: &mut SlideContext<'_, '_>) {
    let Some(drawing_id) = drawing_id(node) else {
        return;
    };
    let ids: Vec<String> = cx
        .objects
        .iter()
        .skip(before)
        .map(|object| object.id.clone())
        .collect();
    if !ids.is_empty() {
        cx.tracker
            .pptx_ids
            .entry(drawing_id.to_owned())
            .or_default()
            .extend(ids);
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
            && let Some(target_id) = tracker
                .pptx_ids
                .get(&connection.target)
                .and_then(|ids| ids.last())
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
            && let Some(target_id) = tracker
                .pptx_ids
                .get(&connection.target)
                .and_then(|ids| ids.last())
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
fn is_placeholder_node(node: &XmlNode) -> bool {
    [
        "nvSpPr",
        "nvPicPr",
        "nvGrpSpPr",
        "nvCxnSpPr",
        "nvGraphicFramePr",
    ]
    .iter()
    .any(|nv| {
        node.child(nv)
            .and_then(|n| n.child("nvPr"))
            .and_then(|p| p.child("ph"))
            .is_some()
    })
}

fn process_non_placeholder_children(
    parent: &XmlNode,
    transform: GroupTransform,
    depth: usize,
    cx: &mut SlideContext<'_, '_>,
) -> Result<(), PptxError> {
    let filtered = XmlNode {
        name: parent.name.clone(),
        attributes: parent.attributes.clone(),
        text: parent.text.clone(),
        children: parent
            .children
            .iter()
            .filter(|c| !is_placeholder_node(c))
            .cloned()
            .collect(),
    };
    process_group_children(&filtered, transform, depth, cx)
}

fn process_group_children(
    parent: &XmlNode,
    transform: GroupTransform,
    depth: usize,
    cx: &mut SlideContext<'_, '_>,
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

                let before = cx.objects.len();
                process_group_children(child, new_transform, depth.saturating_add(1), cx)?;
                record_pptx_id(child, before, cx);
            }
            "cxnSp" => {
                let before = cx.objects.len();
                process_connector(child, transform, cx);
                record_pptx_id(child, before, cx);
            }
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

#[derive(Debug, Clone, Default)]
struct TablePartStyle {
    fill: Option<Color>,
    text_color: Option<Color>,
    bold: Option<bool>,
    border: Option<Outline>,
}

#[derive(Debug, Clone, Default)]
struct TableStyleDef {
    whole: TablePartStyle,
    first_row: TablePartStyle,
    last_row: TablePartStyle,
    first_col: TablePartStyle,
    last_col: TablePartStyle,
    band1_h: TablePartStyle,
    band2_h: TablePartStyle,
    band1_v: TablePartStyle,
    band2_v: TablePartStyle,
}

fn parse_table_part_style(node: &XmlNode, theme: &ThemePalette) -> TablePartStyle {
    let tc_style = node.child("tcStyle");
    let fill = tc_style
        .and_then(|s| s.child("fill"))
        .and_then(|f| resolve_solid_fill(f, theme));
    let border = tc_style.and_then(|s| s.child("tcBdr")).and_then(|bdr| {
        ["insideH", "insideV", "top", "bottom", "left", "right"]
            .iter()
            .find_map(|side| {
                let ln = bdr.child(side)?.child("ln")?;
                if ln.child("noFill").is_some() {
                    return None;
                }
                let color = resolve_solid_fill(ln, theme)?;
                let width = ln
                    .attr("w")
                    .and_then(|v| v.trim().parse::<f64>().ok())
                    .map_or(1.0, |emu| (emu / 12700.0) as f32)
                    .clamp(0.5, 10.0);
                let (join, miter_limit) = extract_join_from_ln(ln);
                Some(Outline {
                    color,
                    width,
                    dash: DashStyle::Solid,
                    dash_pattern: None,
                    join,
                    miter_limit,
                })
            })
    });
    let tx_style = node.child("tcTxStyle");
    let text_color = tx_style.and_then(|t| resolve_first_color_child(t, theme));
    let bold = tx_style
        .and_then(|t| t.attr("b"))
        .map(|b| b == "on" || b == "1");
    TablePartStyle {
        fill,
        text_color,
        bold,
        border,
    }
}

#[derive(Default)]
struct TableStyleSet {
    styles: HashMap<String, TableStyleDef>,
}

fn parse_table_styles(entries: &Entries<'_>, theme: &ThemePalette) -> TableStyleSet {
    let mut set = TableStyleSet::default();
    let Some(bytes) = entries.get("ppt/tableStyles.xml") else {
        return set;
    };
    let Ok(xml_str) = std::str::from_utf8(&bytes) else {
        return set;
    };
    let Ok(root) = parse_xml_tree(xml_str, "ppt/tableStyles.xml", 32) else {
        return set;
    };
    for style_node in root.children_named("tblStyle") {
        let Some(id) = style_node.attr("styleId") else {
            continue;
        };
        let mut def = TableStyleDef::default();
        if let Some(part) = style_node.child("wholeTbl") {
            def.whole = parse_table_part_style(part, theme);
        }
        if let Some(part) = style_node.child("firstRow") {
            def.first_row = parse_table_part_style(part, theme);
        }
        if let Some(part) = style_node.child("lastRow") {
            def.last_row = parse_table_part_style(part, theme);
        }
        if let Some(part) = style_node.child("firstCol") {
            def.first_col = parse_table_part_style(part, theme);
        }
        if let Some(part) = style_node.child("lastCol") {
            def.last_col = parse_table_part_style(part, theme);
        }
        if let Some(part) = style_node.child("band1H") {
            def.band1_h = parse_table_part_style(part, theme);
        }
        if let Some(part) = style_node.child("band2H") {
            def.band2_h = parse_table_part_style(part, theme);
        }
        if let Some(part) = style_node.child("band1V") {
            def.band1_v = parse_table_part_style(part, theme);
        }
        if let Some(part) = style_node.child("band2V") {
            def.band2_v = parse_table_part_style(part, theme);
        }
        set.styles.insert(id.to_ascii_uppercase(), def);
    }
    set
}

fn blend_hex_colors(a: &str, b: &str, t: f32) -> Color {
    let parse = |s: &str| -> (f32, f32, f32) {
        let h = s.trim_start_matches('#');
        if h.len() >= 6 && h.is_ascii() {
            let r = u8::from_str_radix(&h[0..2], 16).unwrap_or(0) as f32;
            let g = u8::from_str_radix(&h[2..4], 16).unwrap_or(0) as f32;
            let bl = u8::from_str_radix(&h[4..6], 16).unwrap_or(0) as f32;
            (r, g, bl)
        } else {
            (217.0, 217.0, 217.0)
        }
    };
    let (ar, ag, ab) = parse(a);
    let (br, bg, bb) = parse(b);
    let t = t.clamp(0.0, 1.0);
    let mix = |x: f32, y: f32| (x + (y - x) * t).round().clamp(0.0, 255.0) as u8;
    Color(format!(
        "#{:02X}{:02X}{:02X}",
        mix(ar, br),
        mix(ag, bg),
        mix(ab, bb)
    ))
}

fn default_table_grid_outline(layout: SlideLayout) -> Outline {
    Outline {
        color: Color("#000000".to_string()),
        width: ((12_700.0 * layout.scale) as f32).clamp(0.5, 50.0),
        dash: DashStyle::Solid,
        dash_pattern: None,
        join: LineJoin::Miter,
        miter_limit: None,
    }
}

fn builtin_table_style(
    style_id: &str,
    theme: &ThemePalette,
    layout: SlideLayout,
) -> Option<TableStyleDef> {
    let id = style_id.trim().to_ascii_uppercase();
    // Standard Office built-in Medium Style 2 (Accent 1..6) and Light/No-Style grids.
    let accent_key = match id.as_str() {
        "{5C22544A-7EE6-4342-B048-85BDC9FD1C3A}" | "{9DCAF9ED-07DC-4A11-8D7F-57B35C25682E}" => {
            Some("accent1")
        }
        "{21E4AEA4-8DFA-4A89-87EB-49C32662AFE0}" | "{D113A9D2-9D6B-4929-AA2D-F23B5EE8CBE7}" => {
            Some("accent2")
        }
        "{F5AB1C69-6EDB-4FF4-983F-18BD219EF322}" | "{18603FDC-E32A-4AB5-989C-0864C322B819}" => {
            Some("accent3")
        }
        "{00A15C55-8517-42AA-B614-E9B94910E393}" | "{306799F8-075E-4A3A-A7F6-7FBC6576F1A4}" => {
            Some("accent4")
        }
        "{7DF18680-E054-41AD-8BC1-D1AEF772440D}" | "{E269D01E-BC32-4049-B463-5C60D7B0CCD2}" => {
            Some("accent5")
        }
        "{93296810-A885-4BE3-A3E7-6D5BEEA58F35}" | "{B508A5F1-E17E-4042-9D08-0E8B325DF00B}" => {
            Some("accent6")
        }
        "{5940675A-B579-460E-94D1-54222C63F5DA}" => {
            return Some(TableStyleDef {
                whole: TablePartStyle {
                    border: Some(default_table_grid_outline(layout)),
                    ..Default::default()
                },
                ..Default::default()
            });
        }
        "{2D5ABB26-0587-4C30-8999-92F81FD0307C}" => {
            return Some(TableStyleDef::default());
        }
        _ => None,
    }?;
    let accent = theme
        .slots
        .get(accent_key)
        .map(|&(r, g, b)| Color(format!("#{r:02X}{g:02X}{b:02X}")))
        .unwrap_or_else(|| Color("#4472C4".to_string()));
    let band1 = blend_hex_colors("#FFFFFF", &accent.0, 0.22);
    let band2 = blend_hex_colors("#FFFFFF", &accent.0, 0.10);
    let border = Some(Outline {
        color: Color("#FFFFFF".to_string()),
        width: 1.0,
        dash: DashStyle::Solid,
        dash_pattern: None,
        join: LineJoin::Miter,
        miter_limit: None,
    });
    Some(TableStyleDef {
        whole: TablePartStyle {
            fill: Some(band2.clone()),
            border: border.clone(),
            ..Default::default()
        },
        first_row: TablePartStyle {
            fill: Some(accent),
            text_color: Some(Color("#FFFFFF".to_string())),
            bold: Some(true),
            border: border.clone(),
        },
        last_row: TablePartStyle {
            bold: Some(true),
            border: border.clone(),
            ..Default::default()
        },
        first_col: TablePartStyle {
            bold: Some(true),
            ..Default::default()
        },
        last_col: TablePartStyle {
            bold: Some(true),
            ..Default::default()
        },
        band1_h: TablePartStyle {
            fill: Some(band1),
            border,
            ..Default::default()
        },
        band2_h: TablePartStyle {
            fill: Some(band2),
            ..Default::default()
        },
        ..Default::default()
    })
}

/// Import one `p:graphicFrame` whose graphic data holds an `a:tbl`.
fn process_table(frame_node: &XmlNode, transform: GroupTransform, cx: &mut SlideContext<'_, '_>) {
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
    // Check the size before any allocation: a hostile grid of thousands of
    // columns and rows would otherwise build millions of cells (N80). The
    // model refuses such a table anyway.
    if rows_nodes.len() > coursecraft_course::TABLE_MAX_AXIS
        || column_count > coursecraft_course::TABLE_MAX_AXIS
        || rows_nodes.len().saturating_mul(column_count) > coursecraft_course::TABLE_MAX_CELLS
    {
        cx.tracker.report_skip(
            SkipKind::Table,
            format!(
                "Table with {} rows and {column_count} columns is too large and was skipped",
                rows_nodes.len()
            ),
        );
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

    let tbl_pr = tbl.child("tblPr");
    let first_row = tbl_pr.is_some_and(|p| attr_flag(p, "firstRow"));
    let last_row = tbl_pr.is_some_and(|p| attr_flag(p, "lastRow"));
    let first_col = tbl_pr.is_some_and(|p| attr_flag(p, "firstCol"));
    let last_col = tbl_pr.is_some_and(|p| attr_flag(p, "lastCol"));
    let band_row = tbl_pr.is_some_and(|p| attr_flag(p, "bandRow"));
    let band_col = tbl_pr.is_some_and(|p| attr_flag(p, "bandCol"));
    let explicit_style_id = tbl_pr
        .and_then(|p| p.child("tableStyleId"))
        .map(|n| n.text.trim())
        .filter(|s| !s.is_empty());
    let lookup_style = |id: &str| -> Option<TableStyleDef> {
        let upper = id.to_ascii_uppercase();
        cx.table_styles
            .styles
            .get(&upper)
            .cloned()
            .or_else(|| builtin_table_style(&upper, cx.theme, *cx.layout))
    };
    let (table_style, apply_default_grid) = match explicit_style_id {
        Some(raw_id) => match lookup_style(raw_id) {
            Some(style) => (Some(style), false),
            None => {
                cx.tracker.report_skip(
                    SkipKind::Table,
                    format!("Table style {raw_id} not recognized; borders may differ"),
                );
                (None, false)
            }
        },
        None => (None, true),
    };

    let links = TextLinkContext {
        slide_rels: cx.slide_rels,
        slide_parts: cx.slide_parts,
    };
    let mut rows: Vec<TableRow> = Vec::with_capacity(rows_nodes.len());
    let mut border_acc = TableBorderAccumulator::default();

    for (row_index, row_node) in rows_nodes.iter().enumerate() {
        let mut cells: Vec<TableCell> = Vec::with_capacity(column_count);
        let mut column = 0usize;
        let mut remaining_h_merge = 0usize;
        for cell_node in row_node.children_named("tc") {
            if column >= column_count {
                break;
            }
            let h_merge = attr_flag(cell_node, "hMerge");
            let v_merge = attr_flag(cell_node, "vMerge");
            if h_merge && remaining_h_merge > 0 {
                remaining_h_merge -= 1;
                continue;
            }
            let col_span =
                attr_span(cell_node, "gridSpan").min(column_count.saturating_sub(column));
            let row_span = attr_span(cell_node, "rowSpan")
                .min(rows_nodes.len().saturating_sub(row_index).max(1));
            let merged = h_merge || v_merge;
            let props = cell_node.child("tcPr");
            if merged {
                cells.push(TableCell::default());
                column = column.saturating_add(1);
            } else {
                let mut part_fill: Option<Color> = None;
                let mut part_text_color: Option<Color> = None;
                let mut part_bold = false;
                if let Some(style) = &table_style {
                    let mut apply_part = |part: &TablePartStyle| {
                        if let Some(f) = &part.fill {
                            part_fill = Some(f.clone());
                        }
                        if let Some(tc) = &part.text_color {
                            part_text_color = Some(tc.clone());
                        }
                        if let Some(b) = part.bold {
                            part_bold = b;
                        }
                    };
                    apply_part(&style.whole);
                    if band_row {
                        let data_idx = row_index.saturating_sub(usize::from(first_row));
                        if !(first_row && row_index == 0) {
                            if data_idx % 2 == 0 {
                                apply_part(&style.band1_h);
                            } else {
                                apply_part(&style.band2_h);
                            }
                        }
                    }
                    if band_col {
                        let data_col = column.saturating_sub(usize::from(first_col));
                        if !(first_col && column == 0) {
                            if data_col % 2 == 0 {
                                apply_part(&style.band1_v);
                            } else {
                                apply_part(&style.band2_v);
                            }
                        }
                    }
                    if first_col && column == 0 {
                        apply_part(&style.first_col);
                    }
                    if last_col && column.saturating_add(col_span) >= column_count {
                        apply_part(&style.last_col);
                    }
                    if last_row && row_index + 1 == rows_nodes.len() {
                        apply_part(&style.last_row);
                    }
                    if first_row && row_index == 0 {
                        apply_part(&style.first_row);
                    }
                }

                let cell_fill = props
                    .and_then(|p| table_cell_fill(p, cx.theme))
                    .or(part_fill);
                let tc_valign = props.and_then(|p| p.attr("anchor")).map(|a| match a {
                    "ctr" => VAlign::Middle,
                    "b" => VAlign::Bottom,
                    _ => VAlign::Top,
                });
                let cell = cell_node
                    .child("txBody")
                    .map(|body| {
                        let mut info =
                            extract_text_runs(body, *cx.layout, Some(links), cx.theme, None);
                        if let Some(tc) = &part_text_color {
                            for run in &mut info.runs {
                                if run.color.is_none() {
                                    run.color = Some(tc.clone());
                                }
                            }
                            info.style.color = tc.clone();
                        }
                        if part_bold {
                            for run in &mut info.runs {
                                if run.bold.is_none() {
                                    run.bold = Some(true);
                                }
                            }
                        }
                        let has_body_anchor = body
                            .child("bodyPr")
                            .and_then(|bp| bp.attr("anchor"))
                            .is_some();
                        if !has_body_anchor && let Some(v) = tc_valign {
                            info.style.valign = v;
                        }
                        TableCell {
                            text: info.text,
                            style: info.style.clone(),
                            runs: info.runs,
                            paragraphs: info.paragraphs,
                            fill: cell_fill.clone(),
                            row_span: u8::try_from(row_span.max(1)).unwrap_or(u8::MAX),
                            col_span: u8::try_from(col_span.max(1)).unwrap_or(u8::MAX),
                            valign: info.style.valign,
                        }
                    })
                    .unwrap_or_else(|| TableCell {
                        fill: cell_fill,
                        row_span: u8::try_from(row_span.max(1)).unwrap_or(u8::MAX),
                        col_span: u8::try_from(col_span.max(1)).unwrap_or(u8::MAX),
                        valign: tc_valign.unwrap_or(VAlign::Top),
                        ..TableCell::default()
                    });
                border_acc.record_cell(
                    props,
                    column,
                    col_span,
                    column_count,
                    row_index,
                    row_span,
                    rows_nodes.len(),
                    *cx.layout,
                    cx.theme,
                );
                let span = col_span.max(1);
                cells.push(cell);
                for _ in 1..span {
                    cells.push(TableCell::default());
                }
                remaining_h_merge = span.saturating_sub(1);
                column = column.saturating_add(span);
            }
        }
        cells.resize(column_count, TableCell::default());
        rows.push(TableRow {
            height: heights.get(row_index).copied().unwrap_or(1.0),
            cells,
        });
    }

    let borders = border_acc.finish(table_style.as_ref(), apply_default_grid, *cx.layout);

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
    let frame = clamp_object_frame(frame, cx.tracker);
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
        emphasis: Vec::new(),
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
        locked: false,
        editor_hidden: false,
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

fn table_cell_fill(props: &XmlNode, theme: &ThemePalette) -> Option<Color> {
    resolve_solid_fill(props, theme)
}

enum SideLine {
    Solid(Outline),
    ExplicitNone,
    Inherited,
}

#[derive(Default)]
struct TableBorderAccumulator {
    outer_borders: Vec<Outline>,
    inner_borders: Vec<Outline>,
    outer_explicit_none: bool,
    outer_inherited: bool,
    inner_explicit_none: bool,
    inner_inherited: bool,
}

impl TableBorderAccumulator {
    #[allow(clippy::too_many_arguments)]
    fn record_cell(
        &mut self,
        props: Option<&XmlNode>,
        column: usize,
        col_span: usize,
        column_count: usize,
        row_index: usize,
        row_span: usize,
        row_count: usize,
        layout: SlideLayout,
        theme: &ThemePalette,
    ) {
        for (side, outer) in [
            ("lnL", column == 0),
            ("lnR", column.saturating_add(col_span) >= column_count),
            ("lnT", row_index == 0),
            ("lnB", row_index.saturating_add(row_span) >= row_count),
        ] {
            let side_line = match props {
                Some(p) => table_side_outline(p, side, layout, theme),
                None => SideLine::Inherited,
            };
            match (side_line, outer) {
                (SideLine::Solid(outline), true) => self.outer_borders.push(outline),
                (SideLine::Solid(outline), false) => self.inner_borders.push(outline),
                (SideLine::ExplicitNone, true) => self.outer_explicit_none = true,
                (SideLine::ExplicitNone, false) => self.inner_explicit_none = true,
                (SideLine::Inherited, true) => self.outer_inherited = true,
                (SideLine::Inherited, false) => self.inner_inherited = true,
            }
        }
    }

    fn finish(
        self,
        table_style: Option<&TableStyleDef>,
        apply_default_grid: bool,
        layout: SlideLayout,
    ) -> TableBorders {
        let style_border = table_style.and_then(|s| {
            s.whole
                .border
                .clone()
                .or_else(|| s.first_row.border.clone())
        });
        let resolve = |explicit: &[Outline], all_none: bool| -> Option<Outline> {
            if let Some(dom) = dominant_outline(explicit) {
                return Some(dom);
            }
            if table_style.is_some() {
                if all_none { None } else { style_border.clone() }
            } else if apply_default_grid {
                if all_none {
                    None
                } else {
                    Some(default_table_grid_outline(layout))
                }
            } else {
                None
            }
        };
        TableBorders {
            outer: resolve(
                &self.outer_borders,
                self.outer_explicit_none && !self.outer_inherited,
            ),
            inner: resolve(
                &self.inner_borders,
                self.inner_explicit_none && !self.inner_inherited,
            ),
        }
    }
}

fn table_side_outline(
    props: &XmlNode,
    side: &str,
    layout: SlideLayout,
    theme: &ThemePalette,
) -> SideLine {
    let Some(line) = props.child(side) else {
        return SideLine::Inherited;
    };
    if line.child("noFill").is_some() {
        return SideLine::ExplicitNone;
    }
    let color = resolve_solid_fill(line, theme).unwrap_or_else(|| Color("#000000".to_string()));
    let width = line
        .attr("w")
        .and_then(|value| value.trim().parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map_or(1.0, |emu| (emu * layout.scale) as f32)
        .clamp(0.0, 50.0);
    if width <= 0.0 {
        return SideLine::ExplicitNone;
    }
    let (dash, dash_pattern) = extract_dash_from_ln(line);
    let (join, miter_limit) = extract_join_from_ln(line);
    SideLine::Solid(Outline {
        color,
        width,
        dash,
        dash_pattern,
        join,
        miter_limit,
    })
}

fn parse_ooxml_dash_pct(raw: &str) -> Option<f32> {
    let trimmed = raw.trim();
    let mult = if let Some(pct) = trimmed.strip_suffix('%') {
        pct.trim().parse::<f32>().ok()? / 100.0
    } else {
        trimmed.parse::<f32>().ok()? / 100_000.0
    };
    (mult.is_finite() && mult > 0.0).then(|| mult.clamp(0.1, 100.0))
}

fn extract_dash_from_ln(ln: &XmlNode) -> (DashStyle, Option<Vec<f32>>) {
    if let Some(cust) = ln.child("custDash") {
        let mut pattern = Vec::new();
        for ds in cust.children_named("ds") {
            let d = ds.attr("d").and_then(parse_ooxml_dash_pct);
            let sp = ds.attr("sp").and_then(parse_ooxml_dash_pct);
            if let (Some(d), Some(sp)) = (d, sp) {
                pattern.push(d);
                pattern.push(sp);
                if pattern.len() >= 32 {
                    break;
                }
            }
        }
        if !pattern.is_empty() {
            return (DashStyle::Custom, Some(pattern));
        }
    }
    map_dash_style(ln.child("prstDash").and_then(|dash| dash.attr("val")))
}

fn map_dash_style(val: Option<&str>) -> (DashStyle, Option<Vec<f32>>) {
    match val {
        Some("dash") => (DashStyle::Dash, None),
        Some("sysDash") => (DashStyle::Dash, Some(vec![3.0, 1.0])),
        Some("lgDash") => (DashStyle::Dash, Some(vec![8.0, 3.0])),
        Some("dot") => (DashStyle::Dot, None),
        Some("sysDot") => (DashStyle::Dot, Some(vec![1.0, 1.0])),
        Some("dashDot") => (DashStyle::DashDot, None),
        Some("sysDashDot") => (DashStyle::DashDot, Some(vec![3.0, 1.0, 1.0, 1.0])),
        Some("lgDashDot") => (DashStyle::DashDot, Some(vec![8.0, 3.0, 1.0, 3.0])),
        Some("dashDotDot" | "sysDashDotDot") => {
            (DashStyle::DashDot, Some(vec![3.0, 1.0, 1.0, 1.0, 1.0, 1.0]))
        }
        Some("lgDashDotDot") => (DashStyle::DashDot, Some(vec![8.0, 3.0, 1.0, 3.0, 1.0, 3.0])),
        _ => (DashStyle::Solid, None),
    }
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
            let default_theme = ThemePalette::default();
            let text_info = extract_text_runs(tx_body, layout, None, &default_theme, None);
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

fn path_outline(sp_pr: &XmlNode, layout: SlideLayout, theme: &ThemePalette) -> Outline {
    if let Some(outline) = extract_outline(sp_pr, layout, theme) {
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
            dash_pattern: None,
            join: LineJoin::Miter,
            miter_limit: None,
        }
    } else {
        Outline {
            color: Color("#000000".to_owned()),
            width: 1.0,
            dash: DashStyle::Solid,
            dash_pattern: None,
            join: LineJoin::Miter,
            miter_limit: None,
        }
    }
}

fn solid_fill(sp_pr: &XmlNode, theme: &ThemePalette) -> Option<Color> {
    resolve_solid_fill(sp_pr, theme)
}

/// Read a colour node (`srgbClr`, `schemeClr`, etc.), falling back to `#D9D9D9`.
fn node_color(node: &XmlNode, theme: &ThemePalette) -> Color {
    resolve_first_color_child(node, theme).unwrap_or_else(|| Color("#D9D9D9".to_owned()))
}

fn attr_f64(node: &XmlNode, name: &str) -> Option<f64> {
    node.attr(name)
        .and_then(|value| value.trim().parse::<f64>().ok())
        .filter(|value| value.is_finite())
}

/// `a:gradFill` into a linear or radial [`FillStyle`].
fn extract_gradient_fill(grad: &XmlNode, theme: &ThemePalette) -> Option<FillStyle> {
    let mut stops: Vec<GradientStop> = Vec::new();
    if let Some(list) = grad.child("gsLst") {
        for stop in list.children_named("gs") {
            let position = attr_f64(stop, "pos")
                .map(|value| (value / 100_000.0).clamp(0.0, 1.0) as f32)
                .unwrap_or(0.0);
            stops.push(GradientStop {
                color: node_color(stop, theme),
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
    cx: &mut SlideContext<'_, '_>,
) -> Option<(String, PictureFillMode, [f32; 2])> {
    let blip = blip_fill.child("blip")?;
    let (target_path, pic_bytes) = preferred_picture(blip, cx)?;
    let file_name = std::path::Path::new(&target_path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("image.png");
    let safe_src = sanitize_media_name(file_name, cx.existing_media_names);
    cx.existing_media_names.insert(safe_src.clone());
    cx.imported_media.push((safe_src.clone(), pic_bytes));
    let (mode, tile_scale) = if let Some(tile) = blip_fill.child("tile") {
        let sx = attr_f64(tile, "sx")
            .map(|v| (v / 100_000.0).clamp(0.01, 100.0) as f32)
            .unwrap_or(1.0);
        let sy = attr_f64(tile, "sy")
            .map(|v| (v / 100_000.0).clamp(0.01, 100.0) as f32)
            .unwrap_or(1.0);
        (PictureFillMode::Tile, [sx, sy])
    } else {
        (PictureFillMode::Stretch, [1.0, 1.0])
    };
    Some((safe_src, mode, tile_scale))
}

/// `a:effectLst`'s outer shadow and glow, converted to slide pixels.
fn extract_effects(
    sp_pr: &XmlNode,
    layout: SlideLayout,
    theme: &ThemePalette,
) -> (Option<Shadow>, Option<Glow>) {
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
            color: node_color(shadow, theme),
            offset_x,
            offset_y,
            blur,
        }
    });
    let glow = effects.child("glow").map(|glow| Glow {
        color: node_color(glow, theme),
        size: (attr_f64(glow, "rad").unwrap_or(0.0) * layout.scale).clamp(0.0, 100.0) as f32,
    });
    (shadow, glow)
}

/// `(rotation, flip_h, flip_v)` inherited by a path object.
type PathTransform = (f64, bool, bool);

fn push_path_object(
    cx: &mut SlideContext<'_, '_>,
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
    let frame = clamp_object_frame(frame, cx.tracker);
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
        emphasis: Vec::new(),
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
        locked: false,
        editor_hidden: false,
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
fn process_connector(cxn: &XmlNode, transform: GroupTransform, cx: &mut SlideContext<'_, '_>) {
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
    let (own_flip_h, own_flip_v) = xfrm_flips(xfrm);
    let own_rotation = xfrm_rotation(xfrm);
    let (rotation, obj_flip_h, obj_flip_v) =
        transform.resolve_rotation_and_flips(own_rotation, false, false);
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
            dash_pattern: None,
            join: LineJoin::Miter,
            miter_limit: None,
        },
        |sp_pr| path_outline(sp_pr, *cx.layout, cx.theme),
    );
    let ln = sp_pr.and_then(|p| p.child("ln"));
    let start_end = line_end_from(ln, "headEnd");
    let end_end = line_end_from(ln, "tailEnd");
    let points = connector_points(frame, own_flip_h, own_flip_v);
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
        subpaths: Vec::new(),
        fill: None,
        fill_style: None,
        outline,
        start_end,
        end_end,
        connector: Some(connector),
        start_glue: None,
        end_glue: None,
        text: None,
    };
    let connector = push_path_object(
        cx,
        &name,
        "Connector",
        path_frame,
        path,
        (rotation, obj_flip_h, obj_flip_v),
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
    let steps = 32usize;
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
    let steps = 24usize;
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
    let steps = ((swing.abs() / (std::f32::consts::PI / 36.0)).ceil() as usize).clamp(2, 256);
    for i in 1..=steps {
        #[allow(clippy::cast_precision_loss)]
        let t = i as f32 / steps as f32;
        let angle = start_angle + swing * t;
        out.push((centre.0 + rx * angle.cos(), centre.1 + ry * angle.sin()));
    }
}

/// Flatten all `<a:path>` elements and their subpaths in `a:custGeom` into slide paths.
fn custom_geometry_paths(
    sp_pr: &XmlNode,
    frame: Rect,
    text_info: &ExtractedText,
    cx: &mut SlideContext<'_, '_>,
) -> Vec<Path> {
    let Some(path_lst) = sp_pr.child("custGeom").and_then(|c| c.child("pathLst")) else {
        return Vec::new();
    };
    let mut result = Vec::new();
    let frame_w = if frame.w > 0.0 { frame.w } else { 1.0 };
    let frame_h = if frame.h > 0.0 { frame.h } else { 1.0 };

    let fill_style = sp_pr
        .child("gradFill")
        .and_then(|grad| extract_gradient_fill(grad, cx.theme))
        .or_else(|| {
            sp_pr.child("blipFill").and_then(|blip_fill| {
                extract_picture_fill(blip_fill, cx).map(|(src, mode, tile_scale)| {
                    FillStyle::Picture {
                        src,
                        mode,
                        tile_scale,
                    }
                })
            })
        });
    let solid = if sp_pr.child("noFill").is_some() {
        None
    } else {
        solid_fill(sp_pr, cx.theme)
    };
    let outline = path_outline(sp_pr, *cx.layout, cx.theme);
    let ln = sp_pr.child("ln");
    let start_end = line_end_from(ln, "headEnd");
    let end_end = line_end_from(ln, "tailEnd");

    // One `<p:sp>` may hold many `<a:path>` elements and each becomes an
    // object, so they count against the per-slide shape cap (N81).
    let room = cx
        .limits
        .max_shapes_per_slide
        .saturating_sub(cx.objects.len());
    for path_node in path_lst.children_named("path") {
        if result.len() >= room {
            cx.tracker.report_skip(
                SkipKind::CustomGeometry,
                "Custom geometry had more paths than the shape limit; extra paths were skipped",
            );
            break;
        }
        let local_w = parse_coord(path_node, "w").filter(|value| *value > 0.0);
        let local_h = parse_coord(path_node, "h").filter(|value| *value > 0.0);
        let scale_x = local_w.map_or(1.0, |w| if frame.w > 0.0 { frame.w / w } else { 1.0 });
        let scale_y = local_h.map_or(1.0, |h| if frame.h > 0.0 { frame.h / h } else { 1.0 });
        let no_fill = path_node.attr("fill") == Some("none");
        let no_stroke =
            path_node.attr("stroke") == Some("0") || path_node.attr("stroke") == Some("false");

        let mut contours: Vec<(Vec<Pt>, bool)> = Vec::new();
        let mut local: Vec<Pt> = Vec::new();
        let mut current: Option<Pt> = None;
        let mut closed = false;

        for command in &path_node.children {
            match command.name.as_str() {
                "moveTo" => {
                    if let Some(point) = path_point(command) {
                        if local.len() >= 2 {
                            contours.push((std::mem::take(&mut local), closed));
                            closed = false;
                        } else {
                            local.clear();
                        }
                        local.push(point);
                        current = Some(point);
                    }
                }
                "lnTo" => {
                    if let Some(point) = path_point(command)
                        && local.len() < coursecraft_course::PATH_MAX_POINTS
                    {
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
                        && local.len() < coursecraft_course::PATH_MAX_POINTS
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
                    if let (Some(&c1), Some(&end)) = (controls.first(), controls.get(1))
                        && local.len() < coursecraft_course::PATH_MAX_POINTS
                    {
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
                    if local.len() < coursecraft_course::PATH_MAX_POINTS {
                        sample_arc(from, rx, ry, start, swing, &mut local);
                        current = local.last().copied();
                    }
                }
                "close" => closed = true,
                _ => {}
            }
        }
        if local.len() >= 2 {
            contours.push((local, closed));
        }

        let to_fractions = |pts: &[Pt]| -> Vec<PathPoint> {
            pts.iter()
                .filter(|(x, y)| x.is_finite() && y.is_finite())
                .take(coursecraft_course::PATH_MAX_POINTS)
                .map(|&(x, y)| {
                    let slide_x = frame.x + x * scale_x;
                    let slide_y = frame.y + y * scale_y;
                    PathPoint {
                        x: ((slide_x - frame.x) / frame_w).clamp(0.0, 1.0),
                        y: ((slide_y - frame.y) / frame_h).clamp(0.0, 1.0),
                    }
                })
                .collect()
        };

        let mut converted: Vec<(Vec<PathPoint>, bool)> = contours
            .into_iter()
            .filter_map(|(pts, cl)| {
                let fracs = to_fractions(&pts);
                (fracs.len() >= 2).then_some((fracs, cl))
            })
            .collect();
        if converted.is_empty() {
            continue;
        }
        let (first_pts, first_closed) = converted.remove(0);
        let subpaths = converted
            .into_iter()
            .map(|(points, cl)| coursecraft_course::PathSubpath { points, closed: cl })
            .collect();
        let path_closed = first_closed && !no_fill;
        let path_outline = if no_stroke {
            Outline {
                color: outline.color.clone(),
                width: 0.0,
                dash: outline.dash,
                dash_pattern: outline.dash_pattern.clone(),
                join: outline.join,
                miter_limit: outline.miter_limit,
            }
        } else {
            outline.clone()
        };
        let attach_text = result.is_empty() && text_info.has_text;
        result.push(Path {
            points: first_pts,
            style: PathStyle::Straight,
            closed: path_closed,
            subpaths,
            fill: if path_closed {
                solid
                    .clone()
                    .or_else(|| fill_style.as_ref().map(|_| Color("#D9D9D9".to_string())))
            } else {
                None
            },
            fill_style: if path_closed {
                fill_style.clone()
            } else {
                None
            },
            outline: path_outline,
            start_end,
            end_end,
            connector: None,
            start_glue: None,
            end_glue: None,
            text: if attach_text {
                Some(ShapeText {
                    text: text_info.text.clone(),
                    style: text_info.style.clone(),
                    runs: text_info.runs.clone(),
                    paragraphs: text_info.paragraphs.clone(),
                    autofit: text_info.autofit,
                })
            } else {
                None
            },
        });
    }
    result
}

/// Process a single `<p:sp>` shape or text box.
fn process_shape(sp: &XmlNode, transform: GroupTransform, cx: &mut SlideContext<'_, '_>) {
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
    let (object_rotation, object_flip_h, object_flip_v) =
        transform.resolve_rotation_and_flips(own_rotation, own_flip_h, own_flip_v);

    let inheritance = cx.resolver.resolve_text_inheritance(
        cx.entries,
        cx.layout_path.as_deref(),
        ph.and_then(|p| p.attr("type")),
        ph.and_then(|p| p.attr("idx")),
        is_any_ph,
    );

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
                cx.theme,
                Some(&inheritance),
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
                subpaths: Vec::new(),
                fill: None,
                fill_style: None,
                outline: path_outline(sp_pr, *cx.layout, cx.theme),
                start_end: line_end_from(ln, "headEnd"),
                end_end: line_end_from(ln, "tailEnd"),
                connector: None,
                start_glue: None,
                end_glue: None,
                text: None,
            };
            let (line_rot, line_fh, line_fv) =
                transform.resolve_rotation_and_flips(own_rotation, false, false);
            push_path_object(
                cx,
                &name,
                "Line",
                path_frame,
                path,
                (line_rot, line_fh, line_fv),
            );
        }
        return;
    }

    // `a:custGeom/a:pathLst` becomes drawn path(s) with all subpaths, curves, fills and text.
    if sp_pr.child("custGeom").is_some() {
        let paths = custom_geometry_paths(sp_pr, frame, &text_info, cx);
        if paths.is_empty() {
            cx.tracker.report_skip(
                SkipKind::CustomGeometry,
                "Custom geometry shape had no usable path",
            );
        } else {
            for path in paths {
                push_path_object(
                    cx,
                    &name,
                    "Freeform",
                    frame,
                    path,
                    (object_rotation, object_flip_h, object_flip_v),
                );
            }
        }
        return;
    }

    let shape_info = extract_shape_info(sp_pr, cx);
    if shape_info.is_custom {
        cx.tracker
            .report_skip(SkipKind::CustomGeometry, "Custom geometry shape skipped");
        return;
    }

    let is_tx_box = sp
        .child("nvSpPr")
        .and_then(|nv| nv.child("cNvSpPr"))
        .and_then(|c| c.attr("txBox"))
        == Some("1");

    // Determine what objects to emit
    match (shape_info.kind, shape_info.fill) {
        (Some(ShapeKind::Rectangle), Some(ref fill))
            if is_tx_box
                && fill.0 == "#00000000"
                && shape_info.fill_style.is_none()
                && text_info.has_text =>
        {
            cx.tracker.text_count = cx.tracker.text_count.saturating_add(1);
            let text_id = format!("text{}", cx.tracker.text_count);
            let text_name = if name.is_empty() {
                format!("Text {}", cx.tracker.text_count)
            } else {
                name
            };
            let frame = clamp_object_frame(frame, cx.tracker);
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
                    outline: shape_info.outline,
                    point_text: false,
                },
                frame,
                timing: Timing::default(),
                states: Vec::new(),
                initial_state: "normal".to_string(),
                entrance: None,
                exit: None,
                motion_paths: Vec::new(),
                emphasis: Vec::new(),
                focus_order: None,
                decorative: false,
                description: None,
                group: None,
                panel: None,
                rotation: normalized_pptx_rotation(object_rotation),
                flip_h: object_flip_h,
                flip_v: object_flip_v,
                opacity: 1.0,
                shadow: shape_info.shadow,
                glow: shape_info.glow,
                drag: None,
                locked: false,
                editor_hidden: false,
            });
        }
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

            let frame = clamp_object_frame(frame, cx.tracker);
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
                emphasis: Vec::new(),
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
                locked: false,
                editor_hidden: false,
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

                let frame = clamp_object_frame(frame, cx.tracker);
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
                        outline: shape_info.outline,
                        point_text: false,
                    },
                    frame,
                    timing: Timing::default(),
                    states: Vec::new(),
                    initial_state: "normal".to_string(),
                    entrance: None,
                    exit: None,
                    motion_paths: Vec::new(),
                    emphasis: Vec::new(),
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
                    locked: false,
                    editor_hidden: false,
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

                let frame = clamp_object_frame(frame, cx.tracker);
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
                    emphasis: Vec::new(),
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
                    locked: false,
                    editor_hidden: false,
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

                let frame = clamp_object_frame(frame, cx.tracker);
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
                        outline: shape_info.outline,
                        point_text: false,
                    },
                    frame,
                    timing: Timing::default(),
                    states: Vec::new(),
                    initial_state: "normal".to_string(),
                    entrance: None,
                    exit: None,
                    motion_paths: Vec::new(),
                    emphasis: Vec::new(),
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
                    locked: false,
                    editor_hidden: false,
                });
            }
        }
    }
}

/// Extract shape geometry, fill and effects from `<p:spPr>`.
fn extract_shape_info(sp_pr: &XmlNode, cx: &mut SlideContext<'_, '_>) -> ExtractedShape {
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
        if let Some(resolved) = resolve_first_color_child(solid, cx.theme) {
            fill = Some(resolved);
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
        fill_style = extract_gradient_fill(grad, cx.theme);
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
            extract_picture_fill(blip_fill, cx).map(|(src, mode, tile_scale)| FillStyle::Picture {
                src,
                mode,
                tile_scale,
            });
        fill = Some(Color("#D9D9D9".to_string()));
        if fill_style.is_none() {
            cx.tracker.report_skip(
                SkipKind::UnsupportedFill,
                "Picture fill could not be read; replaced with the default fill",
            );
        }
    } else if let Some(patt) = sp_pr.child("pattFill") {
        let fg = patt
            .child("fgClr")
            .and_then(|n| resolve_first_color_child(n, cx.theme))
            .unwrap_or_else(|| Color("#000000".to_string()));
        let bg = patt
            .child("bgClr")
            .and_then(|n| resolve_first_color_child(n, cx.theme))
            .unwrap_or_else(|| Color("#FFFFFF".to_string()));
        let preset = patt.attr("prst").unwrap_or("pct50").to_string();
        let weight = match preset.as_str() {
            "pct50" | "smCheck" | "lgCheck" => 0.50,
            "pct25" | "ltUpDiag" | "ltDnDiag" | "dashVert" | "dashHorz" => 0.25,
            "wdUpDiag" | "wdDnDiag" | "cross" | "diagCross" | "trellis" => 0.40,
            "dotGrid" | "pct20" | "pct10" => 0.20,
            _ => 0.35,
        };
        fill = Some(blend_hex_colors(&bg.0, &fg.0, weight));
        fill_style = Some(FillStyle::Pattern { preset, fg, bg });
    }

    let outline = extract_outline(sp_pr, *cx.layout, cx.theme);
    let (shadow, glow) = extract_effects(sp_pr, *cx.layout, cx.theme);

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
        "triangle" => vec![fraction(value("adj", 50000.0))],
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
fn extract_outline(sp_pr: &XmlNode, layout: SlideLayout, theme: &ThemePalette) -> Option<Outline> {
    let ln = sp_pr.child("ln")?;
    if ln.child("noFill").is_some() {
        return None;
    }
    if ln.child("solidFill").is_none() && ln.attr("w").is_none() {
        return None;
    }
    let color = resolve_solid_fill(ln, theme).unwrap_or_else(|| Color("#000000".to_string()));
    // PowerPoint stores line width in EMU; `layout.scale` converts EMU to the
    // 1280x720 slide pixels every other frame measurement uses.
    let width = ln
        .attr("w")
        .and_then(|w| w.trim().parse::<f64>().ok())
        .filter(|w| w.is_finite() && *w >= 0.0)
        .map_or(1.0, |emu| (emu * layout.scale) as f32)
        .clamp(0.0, 50.0);
    let (dash, dash_pattern) = extract_dash_from_ln(ln);
    let (join, miter_limit) = extract_join_from_ln(ln);
    Some(Outline {
        color,
        width,
        dash,
        dash_pattern,
        join,
        miter_limit,
    })
}

/// PowerPoint's default miter limit (`<a:miter lim="800000"/>` = 800% = 8.0).
const DEFAULT_PPTX_MITER_LIMIT: f32 = 8.0;

fn extract_join_from_ln(ln: &XmlNode) -> (LineJoin, Option<f32>) {
    if ln.child("round").is_some() {
        return (LineJoin::Round, None);
    }
    if ln.child("bevel").is_some() {
        return (LineJoin::Bevel, None);
    }
    if let Some(miter) = ln.child("miter") {
        let limit = miter
            .attr("lim")
            .and_then(|v| v.trim().parse::<f64>().ok())
            .filter(|v| v.is_finite() && *v > 0.0)
            .map(|pct| (pct / 100_000.0) as f32)
            .filter(|lim| (*lim - DEFAULT_PPTX_MITER_LIMIT).abs() > 1e-4 && *lim <= 100.0);
        return (LineJoin::Miter, limit);
    }
    (LineJoin::Miter, None)
}

#[derive(Default)]
struct RunDefaults {
    bold: Option<bool>,
    italic: Option<bool>,
    underline: Option<bool>,
    strikethrough: Option<bool>,
    baseline: Option<Baseline>,
    size: Option<f32>,
    color: Option<Color>,
    font: Option<String>,
    highlight: Option<Color>,
    shadow: Option<bool>,
}

fn parse_run_props(
    r_pr: &XmlNode,
    layout: SlideLayout,
    theme: &ThemePalette,
    out: &mut RunDefaults,
) {
    if out.font.is_none()
        && let Some(typeface) = r_pr
            .child("latin")
            .and_then(|latin| latin.attr("typeface"))
            .map(str::trim)
            .filter(|name| !name.is_empty())
    {
        out.font = Some(typeface.to_string());
    }
    if out.bold.is_none()
        && let Some(b) = r_pr.attr("b")
    {
        out.bold = Some(b == "1" || b == "true");
    }
    if out.italic.is_none()
        && let Some(i) = r_pr.attr("i")
    {
        out.italic = Some(i == "1" || i == "true");
    }
    if out.underline.is_none()
        && let Some(u) = r_pr.attr("u")
    {
        out.underline = Some(u != "none");
    }
    if out.strikethrough.is_none()
        && let Some(strike) = r_pr.attr("strike")
    {
        out.strikethrough = Some(strike != "noStrike");
    }
    if out.baseline.is_none()
        && let Some(base_str) = r_pr.attr("baseline")
        && let Ok(val) = base_str.trim().parse::<f64>()
        && val.is_finite()
    {
        out.baseline = Some(if val > 0.0 {
            Baseline::Superscript
        } else if val < 0.0 {
            Baseline::Subscript
        } else {
            Baseline::Normal
        });
    }
    if out.size.is_none()
        && let Some(sz_str) = r_pr.attr("sz")
        && let Ok(sz_val) = sz_str.trim().parse::<f64>()
    {
        let pt = sz_val / 100.0;
        let px = (pt * (4.0 / 3.0) * layout.font_scale()).clamp(1.0, 400.0);
        if pt > 0.0 && px.is_finite() {
            out.size = Some(px as f32);
        }
    }
    if out.color.is_none()
        && let Some(col) = resolve_solid_fill(r_pr, theme)
    {
        out.color = Some(col);
    }
    if out.highlight.is_none()
        && let Some(highlight) = r_pr.child("highlight")
        && let Some(col) = resolve_first_color_child(highlight, theme)
    {
        out.highlight = Some(col);
    }
    if out.shadow.is_none()
        && r_pr
            .child("effectLst")
            .is_some_and(|effects| effects.child("outerShdw").is_some())
    {
        out.shadow = Some(true);
    }
}

fn lvl_p_pr_name(level: u8) -> &'static str {
    match level {
        0 => "lvl1pPr",
        1 => "lvl2pPr",
        2 => "lvl3pPr",
        3 => "lvl4pPr",
        4 => "lvl5pPr",
        5 => "lvl6pPr",
        6 => "lvl7pPr",
        7 => "lvl8pPr",
        _ => "lvl9pPr",
    }
}

/// Extract text runs and styling from `<p:txBody>`.
fn extract_body_props(
    tx_body: &XmlNode,
    layout: SlideLayout,
    inheritance: Option<&TextInheritance>,
) -> (
    VAlign,
    Autofit,
    bool,
    Option<coursecraft_course::TextInsets>,
    coursecraft_course::TextDirection,
) {
    let mut valign = VAlign::Top;
    let mut autofit = Autofit::None;
    let mut wrap = true;
    let mut insets: Option<coursecraft_course::TextInsets> = None;
    let mut direction = coursecraft_course::TextDirection::Horizontal;
    let mut found_anchor = false;
    let mut found_autofit = false;
    let mut found_wrap = false;
    let mut found_insets = false;
    let mut found_dir = false;
    for body_pr in tx_body
        .child("bodyPr")
        .into_iter()
        .chain(inheritance.into_iter().flat_map(|inh| inh.body_pr.iter()))
    {
        if !found_anchor && let Some(anchor) = body_pr.attr("anchor") {
            valign = match anchor {
                "ctr" => VAlign::Middle,
                "b" => VAlign::Bottom,
                _ => VAlign::Top,
            };
            found_anchor = true;
        }
        if !found_autofit && body_pr.child("normAutofit").is_some() {
            autofit = Autofit::Shrink;
            found_autofit = true;
        }
        if !found_wrap && let Some(w) = body_pr.attr("wrap") {
            wrap = w != "none";
            found_wrap = true;
        }
        if !found_insets
            && (body_pr.attr("lIns").is_some()
                || body_pr.attr("tIns").is_some()
                || body_pr.attr("rIns").is_some()
                || body_pr.attr("bIns").is_some())
        {
            let emu_to_px = |attr_name: &str, default_emu: f64| -> f32 {
                let emu = body_pr
                    .attr(attr_name)
                    .and_then(|v| v.trim().parse::<f64>().ok())
                    .filter(|v| v.is_finite() && *v >= 0.0)
                    .unwrap_or(default_emu);
                (emu * layout.scale) as f32
            };
            insets = Some(coursecraft_course::TextInsets {
                left: emu_to_px("lIns", 91440.0),
                top: emu_to_px("tIns", 45720.0),
                right: emu_to_px("rIns", 91440.0),
                bottom: emu_to_px("bIns", 45720.0),
            });
            found_insets = true;
        }
        if !found_dir && let Some(vert) = body_pr.attr("vert") {
            direction = match vert {
                "vert" | "eaVert" => coursecraft_course::TextDirection::Vertical,
                "vert270" => coursecraft_course::TextDirection::Vertical270,
                _ => coursecraft_course::TextDirection::Horizontal,
            };
            found_dir = true;
        }
    }
    if !found_insets && tx_body.child("bodyPr").is_some() {
        insets = Some(coursecraft_course::TextInsets {
            left: (91_440.0 * layout.scale) as f32,
            top: (45_720.0 * layout.scale) as f32,
            right: (91_440.0 * layout.scale) as f32,
            bottom: (45_720.0 * layout.scale) as f32,
        });
    }
    (valign, autofit, wrap, insets, direction)
}

fn extract_text_runs(
    tx_body: &XmlNode,
    layout: SlideLayout,
    links: Option<TextLinkContext<'_>>,
    theme: &ThemePalette,
    inheritance: Option<&TextInheritance>,
) -> ExtractedText {
    let (valign, autofit, wrap, insets, direction) =
        extract_body_props(tx_body, layout, inheritance);

    let mut align = Align::Left;
    let mut first_paragraph = true;
    let mut runs: Vec<TextRun> = Vec::new();
    let mut paragraphs: Vec<Paragraph> = Vec::new();
    let default_18pt_px = (18.0 * (4.0 / 3.0) * layout.font_scale()).clamp(1.0, 400.0) as f32;

    for p in tx_body.children_named("p") {
        let p_pr = p.child("pPr");
        let raw_level = p_pr
            .and_then(|pr| pr.attr("lvl"))
            .and_then(|v| v.trim().parse::<u8>().ok())
            .unwrap_or(0)
            .min(8);
        let lvl_tag = lvl_p_pr_name(raw_level);

        // Build priority chain of pPr nodes for this paragraph.
        let mut p_pr_chain: Vec<&XmlNode> = Vec::new();
        if let Some(pr) = p_pr {
            p_pr_chain.push(pr);
        }
        if let Some(ls) = tx_body.child("lstStyle") {
            if let Some(lvl_pr) = ls.child(lvl_tag) {
                p_pr_chain.push(lvl_pr);
            } else if raw_level == 0
                && let Some(def_pr) = ls.child("defPPr")
            {
                p_pr_chain.push(def_pr);
            }
        }
        if let Some(inh) = inheritance {
            for ls in &inh.lst_styles {
                if let Some(lvl_pr) = ls.child(lvl_tag) {
                    p_pr_chain.push(lvl_pr);
                } else if raw_level == 0
                    && let Some(def_pr) = ls.child("defPPr")
                {
                    p_pr_chain.push(def_pr);
                }
            }
        }

        if first_paragraph {
            if let Some(algn) = p_pr_chain.iter().find_map(|pr| pr.attr("algn")) {
                align = match algn {
                    "ctr" => Align::Center,
                    "r" => Align::Right,
                    "just" | "justLow" | "dist" => Align::Justify,
                    _ => Align::Left,
                };
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
                    highlight: None,
                    shadow: None,
                });
            }
        }

        let mut para_defaults = RunDefaults::default();
        for pr in &p_pr_chain {
            if let Some(def_r_pr) = pr.child("defRPr") {
                parse_run_props(def_r_pr, layout, theme, &mut para_defaults);
            }
        }

        let para_base_px = para_defaults.size.unwrap_or(default_18pt_px);
        paragraphs.push(paragraph_from(
            &p_pr_chain,
            raw_level.min(4),
            layout,
            theme,
            para_base_px,
        ));

        for child in &p.children {
            match child.name.as_str() {
                "r" => {
                    let mut run_props = RunDefaults::default();
                    let mut link = None;

                    if let Some(r_pr) = child.child("rPr") {
                        parse_run_props(r_pr, layout, theme, &mut run_props);
                        link = links.and_then(|links| resolve_run_link(r_pr, links));
                    }

                    let t_text = child.child("t").map(|t| t.text.as_str()).unwrap_or("");
                    if !t_text.is_empty() {
                        runs.push(TextRun {
                            text: t_text.to_string(),
                            bold: run_props.bold.or(para_defaults.bold),
                            italic: run_props.italic.or(para_defaults.italic),
                            underline: run_props.underline.or(para_defaults.underline),
                            size: run_props.size.or(para_defaults.size),
                            color: run_props.color.or_else(|| para_defaults.color.clone()),
                            link,
                            font: run_props.font.or_else(|| para_defaults.font.clone()),
                            strikethrough: run_props.strikethrough.or(para_defaults.strikethrough),
                            baseline: run_props.baseline.or(para_defaults.baseline),
                            letter_spacing: None,
                            highlight: run_props
                                .highlight
                                .or_else(|| para_defaults.highlight.clone()),
                            shadow: run_props.shadow.or(para_defaults.shadow),
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
                            highlight: None,
                            shadow: None,
                        });
                    }
                }
                _ => {}
            }
        }
    }

    let concat_text: String = runs.iter().map(|r| r.text.as_str()).collect();
    let has_text = !concat_text.trim().is_empty();

    // Paragraph default: the first explicit/inherited run size, else 18pt default.
    let default_size = runs.iter().find_map(|r| r.size).unwrap_or(default_18pt_px);
    let style = TextStyle {
        font: "Inter".to_string(),
        size: default_size,
        color: Color("#1F1F1F".to_string()),
        bold: false,
        italic: false,
        underline: false,
        align,
        valign,
        wrap,
        insets,
        direction,
        strikethrough: false,
        baseline: Baseline::Normal,
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

fn map_number_scheme(val: Option<&str>) -> Option<NumberScheme> {
    match val? {
        "arabicPeriod" => Some(NumberScheme::ArabicPeriod),
        "arabicParenR" => Some(NumberScheme::ArabicParenR),
        "alphaLcPeriod" => Some(NumberScheme::AlphaLcPeriod),
        "alphaLcParenR" => Some(NumberScheme::AlphaLcParenR),
        "alphaUcPeriod" => Some(NumberScheme::AlphaUcPeriod),
        "romanLcPeriod" => Some(NumberScheme::RomanLcPeriod),
        "romanUcPeriod" => Some(NumberScheme::RomanUcPeriod),
        _ => None,
    }
}

/// Read one paragraph's list, level, bullet/numbering and spacing from its `pPr` chain.
fn paragraph_from(
    p_pr_chain: &[&XmlNode],
    level: u8,
    layout: SlideLayout,
    theme: &ThemePalette,
    para_base_px: f32,
) -> Paragraph {
    if p_pr_chain.is_empty() {
        return Paragraph::default();
    }
    let mut list = ListKind::None;
    let mut bullet = None;
    let mut number_scheme = None;
    let mut start_at = None;
    for pr in p_pr_chain {
        if pr.child("buNone").is_some() {
            list = ListKind::None;
            break;
        } else if let Some(bu_char) = pr.child("buChar") {
            list = ListKind::Bullet;
            bullet = bu_char.attr("char").map(str::to_owned);
            break;
        } else if let Some(bu_num) = pr.child("buAutoNum") {
            list = ListKind::Number;
            number_scheme = map_number_scheme(bu_num.attr("type"));
            start_at = bu_num
                .attr("startAt")
                .and_then(|v| v.trim().parse::<u32>().ok())
                .filter(|&v| v >= 1);
            break;
        }
    }

    let bullet_color = p_pr_chain
        .iter()
        .find_map(|pr| pr.child("buClr"))
        .and_then(|clr| resolve_first_color_child(clr, theme));

    let bullet_size = p_pr_chain.iter().find_map(|pr| {
        if let Some(pct) = pr
            .child("buSzPct")
            .and_then(|n| n.attr("val"))
            .and_then(|v| v.trim().parse::<f64>().ok())
            .filter(|v| v.is_finite() && *v > 0.0)
        {
            let px = (f64::from(para_base_px) * (pct / 100_000.0)).clamp(1.0, 400.0);
            Some(px as f32)
        } else if let Some(pts) = pr
            .child("buSzPts")
            .and_then(|n| n.attr("val"))
            .and_then(|v| v.trim().parse::<f64>().ok())
            .filter(|v| v.is_finite() && *v > 0.0)
        {
            let px = (pts / 100.0 * (4.0 / 3.0) * layout.font_scale()).clamp(1.0, 400.0);
            Some(px as f32)
        } else {
            None
        }
    });

    let line_spacing = p_pr_chain
        .iter()
        .find_map(|pr| {
            let spc = pr.child("lnSpc")?;
            if let Some(val) = spc
                .child("spcPct")
                .and_then(|pct| pct.attr("val"))
                .and_then(|value| value.trim().parse::<f64>().ok())
                .filter(|v| v.is_finite())
            {
                return Some((val / 100_000.0) as f32);
            }
            if let Some(pts) = spc
                .child("spcPts")
                .and_then(|p| p.attr("val"))
                .and_then(|value| value.trim().parse::<f64>().ok())
                .filter(|v| v.is_finite() && *v > 0.0)
            {
                let exact_px = (pts / 100.0) * (4.0 / 3.0) * layout.font_scale();
                let base_px = f64::from(para_base_px.max(1.0));
                return Some((exact_px / base_px) as f32);
            }
            None
        })
        .filter(|value| value.is_finite())
        .map(|value| value.clamp(0.8, 3.0))
        .unwrap_or(1.0);
    let space = |name: &str| -> f32 {
        p_pr_chain
            .iter()
            .find_map(|pr| {
                pr.child(name)
                    .and_then(|spc| spc.child("spcPts"))
                    .and_then(|pts| pts.attr("val"))
                    .and_then(|value| value.trim().parse::<f64>().ok())
            })
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
        bullet,
        bullet_color,
        bullet_size,
        number_scheme,
        start_at,
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

/// Drop `Link::Slide` values whose target slide was not imported (for example,
/// a hidden slide with `<p:sld show="0">`) and record one report line per link.
fn drop_links_to_unimported_slides(slides: &mut [Slide], report_items: &mut Vec<ReportItem>) {
    let imported_ids: HashSet<String> = slides.iter().map(|s| s.id.clone()).collect();
    let mut check_runs = |slide_num: usize, runs: &mut [TextRun]| {
        for run in runs {
            if let Some(Link::Slide { slide: target }) = &run.link
                && !imported_ids.contains(target)
            {
                report_items.push(ReportItem {
                    slide: slide_num,
                    kind: SkipKind::HiddenSlide,
                    note: format!("Dropped link to skipped hidden slide {target}"),
                });
                run.link = None;
            }
        }
    };
    let mut check_objects = |slide_num: usize, objects: &mut [Object]| {
        for object in objects {
            match &mut object.kind {
                ObjectKind::Text { runs, .. } => check_runs(slide_num, runs),
                ObjectKind::Shape {
                    text: Some(text), ..
                } => check_runs(slide_num, &mut text.runs),
                ObjectKind::Table { rows, .. } => {
                    for row in rows {
                        for cell in &mut row.cells {
                            check_runs(slide_num, &mut cell.runs);
                        }
                    }
                }
                _ => {}
            }
        }
    };
    for slide in slides {
        let slide_num = slide
            .id
            .strip_prefix("slide")
            .and_then(|n| n.parse::<usize>().ok())
            .unwrap_or(1);
        check_objects(slide_num, &mut slide.objects);
        for layer in &mut slide.layers {
            check_objects(slide_num, &mut layer.objects);
        }
    }
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

/// Resolve a picture relationship to its archive path and bytes.
fn relationship_picture(embed_id: &str, cx: &SlideContext<'_, '_>) -> Option<(String, Arc<[u8]>)> {
    let rel = cx.slide_rels.get(embed_id)?;
    if rel.is_external {
        return None;
    }
    let target = rel.target.as_deref()?;
    let bytes = cx.entries.get(target)?;
    Some((target.to_owned(), bytes))
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

/// A picture part ready to store: EMF and WMF clip art becomes SVG (vectors
/// kept, renamed `.svg`); other formats are kept as they are when they decode.
fn usable_picture(
    path: String,
    bytes: &[u8],
    tracker: &mut SlideTracker,
) -> Option<(String, Vec<u8>)> {
    let metafile = coursecraft_media::is_metafile_name(&path)
        || (!is_svg_path(&path) && coursecraft_media::is_metafile(bytes));
    if !metafile {
        return picture_bytes_are_valid(&path, bytes).then(|| (path, bytes.to_vec()));
    }
    let converted =
        coursecraft_media::metafile_to_svg(bytes, &coursecraft_media::BundledFonts).ok()?;
    if !picture_bytes_are_valid("picture.svg", &converted.svg) {
        return None;
    }
    let name = std::path::Path::new(&path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("picture.emf");
    let mut lost = Vec::new();
    if converted.skipped_records > 0 {
        lost.push(format!("{} unread records", converted.skipped_records));
    }
    if converted.missing_glyphs > 0 {
        lost.push(format!(
            "{} characters no font draws",
            converted.missing_glyphs
        ));
    }
    if !lost.is_empty() {
        tracker.report_skip(
            SkipKind::MetafilePart,
            format!("{name} imported as SVG without {}", lost.join(" and ")),
        );
    }
    let folder = path.rsplit_once('/').map_or("", |(folder, _)| folder);
    let svg_name = coursecraft_media::metafile_svg_name(name);
    let svg_path = if folder.is_empty() {
        svg_name
    } else {
        format!("{folder}/{svg_name}")
    };
    Some((svg_path, converted.svg))
}

/// Pick the picture part for a `a:blip`: an `asvg:svgBlip` vector part wins
/// over the PNG fallback that older PowerPoint versions need.
fn preferred_picture(blip: &XmlNode, cx: &mut SlideContext<'_, '_>) -> Option<(String, Vec<u8>)> {
    if let Some(svg) = blip.find_descendant("svgBlip")
        && let Some(embed_id) = svg.attr("r:embed").or_else(|| svg.attr("embed"))
        && let Some((path, bytes)) = relationship_picture(embed_id, cx)
        && is_svg_path(&path)
        && picture_bytes_are_valid(&path, &bytes)
    {
        return Some((path, bytes.to_vec()));
    }

    let embed_id = blip.attr("r:embed").or_else(|| blip.attr("embed"))?;
    let (path, bytes) = relationship_picture(embed_id, cx)?;
    usable_picture(path, &bytes, cx.tracker)
}

/// Process a `<p:pic>` picture element (or video/audio media picture).
fn process_picture(pic: &XmlNode, transform: GroupTransform, cx: &mut SlideContext<'_, '_>) {
    let has_audio = pic.find_descendant("audioFile").is_some();
    let has_video =
        pic.find_descendant("videoFile").is_some() || pic.find_descendant("media").is_some();
    if has_audio && !has_video {
        cx.tracker
            .report_skip(SkipKind::Media, "Audio picture skipped");
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
    let own_rotation = xfrm_rotation(Some(xfrm));
    let (rotation, flip_h, flip_v) =
        transform.resolve_rotation_and_flips(own_rotation, own_flip_h, own_flip_v);

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

    let mut poster_src: Option<String> = None;
    let mut crop: Option<coursecraft_course::CropRect> = None;
    if let Some(blip_fill) = pic.child("blipFill")
        && let Some(blip) = blip_fill.child("blip")
        && let Some(embed_id) = blip.attr("r:embed").or_else(|| blip.attr("embed"))
    {
        if !cx.slide_rels.contains_key(embed_id)
            || cx
                .slide_rels
                .get(embed_id)
                .is_some_and(|rel| rel.is_external)
        {
            if !has_video {
                cx.tracker.report_skip(
                    SkipKind::ExternalTarget,
                    "Missing or external picture target",
                );
                return;
            }
        } else if let Some((target_path, pic_bytes)) = preferred_picture(blip, cx) {
            let file_name = std::path::Path::new(&target_path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("image.png");
            let safe_src = sanitize_media_name(file_name, cx.existing_media_names);
            cx.existing_media_names.insert(safe_src.clone());
            cx.imported_media.push((safe_src.clone(), pic_bytes));
            poster_src = Some(safe_src);
            crop = src_rect_crop(blip_fill);
        } else if !has_video {
            cx.tracker.report_skip(
                SkipKind::UndecodablePicture,
                "Picture file not found or undecodable",
            );
            return;
        }
    } else if !has_video {
        return;
    }

    let outline = extract_outline(sp_pr, *cx.layout, cx.theme);
    let shape = sp_pr
        .child("prstGeom")
        .and_then(|prst| prst.attr("prst"))
        .and_then(coursecraft_course::shape_kind_for_ooxml)
        .filter(|sk| *sk != ShapeKind::Rectangle);
    let frame = clamp_object_frame(frame, cx.tracker);

    if has_video {
        let vid_rid = pic
            .find_descendant("media")
            .and_then(|m| m.attr("r:embed").or_else(|| m.attr("embed")))
            .or_else(|| {
                pic.find_descendant("videoFile").and_then(|v| {
                    v.attr("r:link")
                        .or_else(|| v.attr("link"))
                        .or_else(|| v.attr("r:embed"))
                        .or_else(|| v.attr("embed"))
                })
            });
        let video_part = vid_rid
            .and_then(|rid| cx.slide_rels.get(rid))
            .filter(|rel| !rel.is_external)
            .and_then(|rel| rel.target.as_deref())
            .and_then(|target| {
                cx.entries
                    .get(target)
                    .map(|bytes| (target.to_string(), bytes.to_vec()))
            });
        if let Some((vid_path, vid_bytes)) = video_part {
            let file_name = std::path::Path::new(&vid_path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("video.mp4");
            let safe_vid = sanitize_media_name(file_name, cx.existing_media_names);
            cx.existing_media_names.insert(safe_vid.clone());
            cx.imported_media.push((safe_vid.clone(), vid_bytes));
            cx.tracker.image_count = cx.tracker.image_count.saturating_add(1);
            let obj_id = format!("video{}", cx.tracker.image_count);
            let obj_name = if name.is_empty() {
                format!("Video {}", cx.tracker.image_count)
            } else {
                name
            };
            cx.objects.push(Object {
                id: obj_id,
                name: obj_name,
                kind: ObjectKind::Video {
                    src: safe_vid,
                    autoplay: false,
                    captions: None,
                    volume: 1.0,
                    trim: None,
                    cues: Vec::new(),
                    poster: poster_src,
                },
                frame,
                timing: Timing::default(),
                states: Vec::new(),
                initial_state: "normal".to_string(),
                entrance: None,
                exit: None,
                motion_paths: Vec::new(),
                emphasis: Vec::new(),
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
                locked: false,
                editor_hidden: false,
            });
            return;
        }
        cx.tracker.report_skip(
            SkipKind::Media,
            "Video media part missing; kept poster picture",
        );
    }

    let Some(safe_src) = poster_src else {
        return;
    };

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
            outline,
            shape,
        },
        frame,
        timing: Timing::default(),
        states: Vec::new(),
        initial_state: "normal".to_string(),
        entrance: None,
        exit: None,
        motion_paths: Vec::new(),
        emphasis: Vec::new(),
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
        locked: false,
        editor_hidden: false,
    });
}
