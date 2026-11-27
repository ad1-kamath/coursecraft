//! Windows metafiles (EMF, EMF+ and WMF) converted to SVG, kept as vectors.
//!
//! Clip art from Office and older decks arrives as metafiles. `craft-metafile`
//! plays the file's records into a neutral picture (paths, pattern fills,
//! bitmaps and text, each with its clip); this module writes that picture as
//! an SVG, so the rest of CourseCraft (the renderer, the media library and the
//! published player) treats it like any other vector picture.
//!
//! The SVG renderer draws no text, so text items are written as glyph
//! outlines: the face comes from a [`MetafileFonts`] source, falling back to
//! the bundled Inter faces. Characters no face has are counted, not drawn.
//!
//! Input is hostile: bytes are capped, the reader enforces its own limits, and
//! the SVG written stays within [`MAX_SVG_BYTES`] and [`MAX_SVG_ELEMENTS`]
//! (bitmaps are scaled down until it fits) or the conversion fails.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::Arc;

use base64::Engine as _;
use craft_metafile::{
    Cap, ClipPath, HAlign, Item, Join, LineStyle, Picture, Seg, Text, Tile, VAlign,
};
use skrifa::MetadataProvider as _;
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::prelude::{LocationRef, Size};

use crate::text_metrics::{INTER_BOLD, INTER_BOLD_ITALIC, INTER_ITALIC, INTER_REGULAR};
use crate::{MAX_SVG_BYTES, MAX_SVG_ELEMENTS, MediaError};

/// Largest metafile accepted, in bytes (32 MiB).
pub const MAX_METAFILE_BYTES: usize = 32 * 1024 * 1024;

/// Longest bitmap side written on the first try; later tries halve it.
const FIRST_BITMAP_SIDE: u32 = 2048;
/// Shortest bitmap side tried before the conversion gives up on size.
const LAST_BITMAP_SIDE: u32 = 128;
/// LOGFONT height used when the file asks for the default size (0).
const DEFAULT_TEXT_HEIGHT: f32 = 16.0;
/// Characters outlined from one file.
const MAX_GLYPHS: usize = 50_000;

/// A metafile written as SVG.
#[derive(Clone, Debug, PartialEq)]
pub struct MetafileSvg {
    /// The SVG document (UTF-8).
    pub svg: Vec<u8>,
    /// Natural width in CSS pixels (the metafile's points times 4/3).
    pub width: f32,
    /// Natural height in CSS pixels.
    pub height: f32,
    /// Records of a type the reader does not play: part of the file was not drawn.
    pub skipped_records: u32,
    /// Characters drawn by no available face (left as gaps).
    pub missing_glyphs: u32,
    /// Bitmaps written smaller than their own size to keep the SVG within its cap.
    pub reduced_bitmaps: u32,
}

/// Where text faces come from. A `None` answer falls back to bundled Inter.
pub trait MetafileFonts {
    /// The font file and face index for `family` (as the metafile names it).
    fn face(&self, family: &str, bold: bool, italic: bool) -> Option<FontFile>;
}

/// Bundled faces only: every family is drawn in Inter.
#[derive(Clone, Copy, Debug, Default)]
pub struct BundledFonts;

impl MetafileFonts for BundledFonts {
    fn face(&self, _family: &str, _bold: bool, _italic: bool) -> Option<(Arc<[u8]>, u32)> {
        None
    }
}

/// True when `bytes` start like an EMF or a WMF (placeable or plain).
#[must_use]
pub fn is_metafile(bytes: &[u8]) -> bool {
    craft_metafile::is_metafile(bytes)
}

/// True when a file name has a metafile extension (`.emf`, `.wmf`).
#[must_use]
pub fn is_metafile_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".emf") || lower.ends_with(".wmf")
}

/// `name` with its metafile extension replaced by `.svg` (other names gain `.svg`).
#[must_use]
pub fn metafile_svg_name(name: &str) -> String {
    // The extension is four ASCII bytes, so the cut is on a character boundary.
    let stem = if is_metafile_name(name) {
        name.get(..name.len().saturating_sub(4)).unwrap_or(name)
    } else {
        name
    };
    let stem = if stem.is_empty() { "picture" } else { stem };
    format!("{stem}.svg")
}

/// Convert an EMF, EMF+ or WMF file to an SVG that keeps its vectors.
///
/// # Errors
/// [`MediaError::Unsupported`] when the bytes are not a metafile,
/// [`MediaError::Corrupt`] when nothing can be read or the picture has no
/// size, and the SVG size errors when even scaled-down bitmaps do not fit.
pub fn metafile_to_svg(bytes: &[u8], fonts: &dyn MetafileFonts) -> Result<MetafileSvg, MediaError> {
    if bytes.len() > MAX_METAFILE_BYTES {
        return Err(MediaError::Corrupt(format!(
            "metafile too large: {} bytes (max {MAX_METAFILE_BYTES})",
            bytes.len()
        )));
    }
    if !craft_metafile::is_metafile(bytes) {
        return Err(MediaError::Unsupported);
    }
    let picture = craft_metafile::parse(bytes).map_err(|e| MediaError::Corrupt(e.to_string()))?;
    picture_to_svg(&picture, fonts)
}

/// Write a parsed picture as SVG (see [`metafile_to_svg`]).
///
/// # Errors
/// As [`metafile_to_svg`], for the picture's size and the SVG caps.
pub fn picture_to_svg(
    picture: &Picture,
    fonts: &dyn MetafileFonts,
) -> Result<MetafileSvg, MediaError> {
    let (w, h) = (f64::from(picture.width), f64::from(picture.height));
    if !(w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0) {
        return Err(MediaError::Corrupt("metafile has no size".to_string()));
    }
    let mut faces = FaceCache::default();
    let mut side = FIRST_BITMAP_SIDE;
    loop {
        let mut writer = Writer::new(picture, side);
        let outcome = writer.write(picture, fonts, &mut faces);
        match outcome {
            Ok(()) => {
                return Ok(MetafileSvg {
                    svg: writer.out.into_bytes(),
                    width: (w * 4.0 / 3.0) as f32,
                    height: (h * 4.0 / 3.0) as f32,
                    skipped_records: picture.skipped_records,
                    missing_glyphs: writer.missing_glyphs,
                    reduced_bitmaps: writer.reduced_bitmaps,
                });
            }
            Err(Full::Bytes) if writer.bitmap_bytes > 0 && side > LAST_BITMAP_SIDE => side /= 2,
            Err(Full::Bytes) => {
                return Err(MediaError::SvgTooLarge {
                    bytes: writer.out.len(),
                    max: MAX_SVG_BYTES,
                });
            }
            Err(Full::Elements) => {
                return Err(MediaError::SvgTooComplex {
                    count: writer.elements,
                    max: MAX_SVG_ELEMENTS,
                });
            }
        }
    }
}

/// Why a write stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Full {
    Bytes,
    Elements,
}

/// The clip an item was drawn under, as a comparable key.
#[derive(Clone, PartialEq)]
struct ClipKey {
    region: Option<Vec<[u32; 4]>>,
    paths: Vec<*const ClipPath>,
}

impl ClipKey {
    fn of(region: Option<&Vec<[f32; 4]>>, clips: &[Arc<ClipPath>]) -> Self {
        ClipKey {
            region: region.map(|rects| {
                rects
                    .iter()
                    .map(|r| {
                        [
                            r[0].to_bits(),
                            r[1].to_bits(),
                            r[2].to_bits(),
                            r[3].to_bits(),
                        ]
                    })
                    .collect()
            }),
            paths: clips.iter().map(Arc::as_ptr).collect(),
        }
    }
}

/// A font file and the face index inside it.
pub type FontFile = (Arc<[u8]>, u32);

/// Faces loaded for text, by family and style.
#[derive(Default)]
struct FaceCache {
    faces: HashMap<(String, bool, bool), Option<FontFile>>,
}

impl FaceCache {
    fn get(
        &mut self,
        fonts: &dyn MetafileFonts,
        family: &str,
        bold: bool,
        italic: bool,
    ) -> (Arc<[u8]>, u32) {
        let key = (family.to_ascii_lowercase(), bold, italic);
        let found = self
            .faces
            .entry(key)
            .or_insert_with(|| fonts.face(family, bold, italic))
            .clone();
        found.unwrap_or_else(|| {
            let bytes: &'static [u8] = match (bold, italic) {
                (false, false) => INTER_REGULAR,
                (true, false) => INTER_BOLD,
                (false, true) => INTER_ITALIC,
                (true, true) => INTER_BOLD_ITALIC,
            };
            (Arc::from(bytes), 0)
        })
    }
}

/// Strokes queued to be written as one path.
struct Pending {
    key: ClipKey,
    region: Option<Vec<[f32; 4]>>,
    clips: Vec<Arc<ClipPath>>,
    style: String,
    d: String,
}

struct Writer {
    out: String,
    elements: usize,
    /// Hairline stroke width in picture units.
    hairline: f64,
    max_side: u32,
    bitmap_bytes: usize,
    reduced_bitmaps: u32,
    missing_glyphs: u32,
    glyphs: usize,
    next_id: usize,
    regions: HashMap<Vec<[u32; 4]>, String>,
    clip_paths: HashMap<*const ClipPath, String>,
    tiles: HashMap<*const Tile, String>,
    /// Clip groups currently open: the key and how many `<g>` it opened.
    open: Option<(ClipKey, usize)>,
}

impl Writer {
    fn new(picture: &Picture, max_side: u32) -> Self {
        let diag = f64::from(picture.width).hypot(f64::from(picture.height));
        Writer {
            out: String::new(),
            elements: 0,
            hairline: (diag / 1000.0).max(1e-3),
            max_side,
            bitmap_bytes: 0,
            reduced_bitmaps: 0,
            missing_glyphs: 0,
            glyphs: 0,
            next_id: 0,
            regions: HashMap::new(),
            clip_paths: HashMap::new(),
            tiles: HashMap::new(),
            open: None,
        }
    }

    fn check(&self) -> Result<(), Full> {
        if self.out.len() > MAX_SVG_BYTES {
            Err(Full::Bytes)
        } else if self.elements > MAX_SVG_ELEMENTS {
            Err(Full::Elements)
        } else {
            Ok(())
        }
    }

    fn element(&mut self) -> Result<(), Full> {
        self.elements = self.elements.saturating_add(1);
        self.check()
    }

    fn id(&mut self, prefix: &str) -> String {
        self.next_id = self.next_id.saturating_add(1);
        format!("{prefix}{}", self.next_id)
    }

    fn write(
        &mut self,
        picture: &Picture,
        fonts: &dyn MetafileFonts,
        faces: &mut FaceCache,
    ) -> Result<(), Full> {
        let (w, h) = (f64::from(picture.width), f64::from(picture.height));
        self.out.push_str(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\"",
        );
        let _ = write!(
            self.out,
            " width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\">",
            num(w * 4.0 / 3.0),
            num(h * 4.0 / 3.0),
            num(w),
            num(h)
        );
        // Consecutive strokes with the same pen and clip become one path.
        let mut pending: Option<Pending> = None;
        for item in &picture.items {
            let (region, clips) = item_clip(item);
            // An empty region clips everything away.
            if region.is_some_and(Vec::is_empty) {
                continue;
            }
            let key = ClipKey::of(region, clips);
            if let Item::Path {
                segs,
                fill: None,
                stroke: Some((color, width)),
                line,
                ..
            } = item
                && color[3] == 255
            {
                let style = self.stroke_attrs(*color, *width, line);
                let d = path_data(segs);
                if let Some(p) = pending.as_mut()
                    && p.key == key
                    && p.style == style
                {
                    p.d.push_str(&d);
                    if p.d.len() > MAX_SVG_BYTES {
                        return Err(Full::Bytes);
                    }
                    continue;
                }
                self.flush(pending.take())?;
                pending = Some(Pending {
                    key,
                    region: region.cloned(),
                    clips: clips.to_vec(),
                    style,
                    d,
                });
                continue;
            }
            self.flush(pending.take())?;
            self.enter_clip(key, region, clips)?;
            self.item(item, fonts, faces)?;
            self.check()?;
        }
        self.flush(pending.take())?;
        self.close_clip();
        self.out.push_str("</svg>");
        self.check()
    }

    fn flush(&mut self, pending: Option<Pending>) -> Result<(), Full> {
        let Some(p) = pending else { return Ok(()) };
        self.enter_clip(p.key, p.region.as_ref(), &p.clips)?;
        self.stroke_path(&p.style, &p.d)
    }

    fn stroke_path(&mut self, style: &str, d: &str) -> Result<(), Full> {
        let _ = write!(self.out, "<path d=\"{d}\" fill=\"none\"{style}/>");
        self.element()
    }

    fn stroke_attrs(&self, color: [u8; 4], width: f32, line: &LineStyle) -> String {
        let mut s = String::new();
        let width = if width > 0.0 && width.is_finite() {
            f64::from(width)
        } else {
            self.hairline
        };
        let _ = write!(
            s,
            " stroke=\"{}\" stroke-width=\"{}\"",
            hex(color),
            num(width)
        );
        if color[3] != 255 {
            let _ = write!(
                s,
                " stroke-opacity=\"{}\"",
                num(f64::from(color[3]) / 255.0)
            );
        }
        s.push_str(match line.cap {
            Cap::Round => " stroke-linecap=\"round\"",
            Cap::Square => " stroke-linecap=\"square\"",
            Cap::Flat => "",
        });
        match line.join {
            Join::Round => s.push_str(" stroke-linejoin=\"round\""),
            Join::Bevel => s.push_str(" stroke-linejoin=\"bevel\""),
            Join::Miter => {
                let limit = if line.miter_limit.is_finite() {
                    line.miter_limit.max(1.0)
                } else {
                    10.0
                };
                let _ = write!(s, " stroke-miterlimit=\"{}\"", num(f64::from(limit)));
            }
        }
        let dash_ok = !line.dash.is_empty()
            && line.dash.iter().all(|d| d.is_finite() && *d >= 0.0)
            && line.dash.iter().any(|d| *d > 0.0);
        if dash_ok {
            let list: Vec<String> = line.dash.iter().map(|d| num(f64::from(*d))).collect();
            let _ = write!(s, " stroke-dasharray=\"{}\"", list.join(" "));
        }
        s
    }

    /// Open the groups for `key`, closing the ones open for another clip.
    fn enter_clip(
        &mut self,
        key: ClipKey,
        region: Option<&Vec<[f32; 4]>>,
        clips: &[Arc<ClipPath>],
    ) -> Result<(), Full> {
        if self.open.as_ref().is_some_and(|(open, _)| *open == key) {
            return Ok(());
        }
        self.close_clip();
        let mut ids = Vec::new();
        if let (Some(rects), Some(bits)) = (region, key.region.as_ref()) {
            let id = match self.regions.get(bits) {
                Some(id) => id.clone(),
                None => {
                    let id = self.id("r");
                    let _ = write!(self.out, "<clipPath id=\"{id}\">");
                    for r in rects {
                        let _ = write!(
                            self.out,
                            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/>",
                            num(f64::from(r[0])),
                            num(f64::from(r[1])),
                            num(f64::from(r[2] - r[0]).max(0.0)),
                            num(f64::from(r[3] - r[1]).max(0.0))
                        );
                        self.element()?;
                    }
                    self.out.push_str("</clipPath>");
                    self.element()?;
                    self.regions.insert(bits.clone(), id.clone());
                    id
                }
            };
            ids.push(id);
        }
        for clip in clips {
            let ptr = Arc::as_ptr(clip);
            let id = match self.clip_paths.get(&ptr) {
                Some(id) => id.clone(),
                None => {
                    let id = self.id("c");
                    let rule = if clip.even_odd {
                        " clip-rule=\"evenodd\""
                    } else {
                        ""
                    };
                    let _ = write!(
                        self.out,
                        "<clipPath id=\"{id}\"><path d=\"{}\"{rule}/></clipPath>",
                        path_data(&clip.segs)
                    );
                    self.element()?;
                    self.element()?;
                    self.clip_paths.insert(ptr, id.clone());
                    id
                }
            };
            ids.push(id);
        }
        for id in &ids {
            let _ = write!(self.out, "<g clip-path=\"url(#{id})\">");
            self.element()?;
        }
        self.open = Some((key, ids.len()));
        Ok(())
    }

    fn close_clip(&mut self) {
        if let Some((_, depth)) = self.open.take() {
            for _ in 0..depth {
                self.out.push_str("</g>");
            }
        }
    }

    fn item(
        &mut self,
        item: &Item,
        fonts: &dyn MetafileFonts,
        faces: &mut FaceCache,
    ) -> Result<(), Full> {
        match item {
            Item::Path {
                segs,
                fill,
                stroke,
                line,
                even_odd,
                ..
            } => {
                let d = path_data(segs);
                let _ = write!(self.out, "<path d=\"{d}\"");
                match fill {
                    Some(color) => {
                        let _ = write!(self.out, " fill=\"{}\"", hex(*color));
                        if color[3] != 255 {
                            let _ = write!(
                                self.out,
                                " fill-opacity=\"{}\"",
                                num(f64::from(color[3]) / 255.0)
                            );
                        }
                        if *even_odd {
                            self.out.push_str(" fill-rule=\"evenodd\"");
                        }
                    }
                    None => self.out.push_str(" fill=\"none\""),
                }
                if let Some((color, width)) = stroke {
                    let attrs = self.stroke_attrs(*color, *width, line);
                    self.out.push_str(&attrs);
                }
                self.out.push_str("/>");
                self.element()
            }
            Item::Pattern {
                segs,
                even_odd,
                tile,
                ..
            } => {
                let id = self.tile(tile)?;
                let rule = if *even_odd {
                    " fill-rule=\"evenodd\""
                } else {
                    ""
                };
                let _ = write!(
                    self.out,
                    "<path d=\"{}\" fill=\"url(#{id})\"{rule}/>",
                    path_data(segs)
                );
                self.element()
            }
            Item::Bitmap {
                transform,
                width,
                height,
                rgba,
                ..
            } => {
                let Some(png) = self.png(*width, *height, rgba) else {
                    return Ok(());
                };
                let [a, b, c, d, e, f] = *transform;
                let _ = write!(
                    self.out,
                    "<image width=\"1\" height=\"1\" preserveAspectRatio=\"none\" transform=\"matrix({} {} {} {} {} {})\" xlink:href=\"data:image/png;base64,{png}\"/>",
                    num(a),
                    num(b),
                    num(c),
                    num(d),
                    num(e),
                    num(f)
                );
                self.element()
            }
            Item::Text(text) => self.text(text, fonts, faces),
        }
    }

    fn tile(&mut self, tile: &Arc<Tile>) -> Result<String, Full> {
        let ptr = Arc::as_ptr(tile);
        if let Some(id) = self.tiles.get(&ptr) {
            return Ok(id.clone());
        }
        let id = self.id("p");
        let tw = f64::from(tile.width) * f64::from(tile.cell[0]);
        let th = f64::from(tile.height) * f64::from(tile.cell[1]);
        let png = self
            .png(tile.width, tile.height, &tile.rgba)
            .unwrap_or_default();
        let _ = write!(
            self.out,
            "<pattern id=\"{id}\" patternUnits=\"userSpaceOnUse\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\">",
            num(f64::from(tile.origin[0])),
            num(f64::from(tile.origin[1])),
            num(tw),
            num(th)
        );
        if !png.is_empty() {
            let _ = write!(
                self.out,
                "<image width=\"{}\" height=\"{}\" preserveAspectRatio=\"none\" style=\"image-rendering:pixelated\" xlink:href=\"data:image/png;base64,{png}\"/>",
                num(tw),
                num(th)
            );
            self.element()?;
        }
        self.out.push_str("</pattern>");
        self.element()?;
        self.tiles.insert(ptr, id.clone());
        Ok(id)
    }

    /// PNG as base64, scaled down to the current side cap. None when the pixels are unusable.
    fn png(&mut self, width: u32, height: u32, rgba: &[u8]) -> Option<String> {
        let expected = (width as usize)
            .checked_mul(height as usize)?
            .checked_mul(4)?;
        if width == 0 || height == 0 || rgba.len() != expected {
            return None;
        }
        let image = image::RgbaImage::from_raw(width, height, rgba.to_vec())?;
        let image = if width.max(height) > self.max_side {
            let scale = f64::from(self.max_side) / f64::from(width.max(height));
            let nw = ((f64::from(width) * scale).round() as u32).max(1);
            let nh = ((f64::from(height) * scale).round() as u32).max(1);
            self.reduced_bitmaps = self.reduced_bitmaps.saturating_add(1);
            image::imageops::resize(&image, nw, nh, image::imageops::FilterType::Triangle)
        } else {
            image
        };
        let mut png = Vec::new();
        let encoded = if image.pixels().all(|p| p.0[3] == 255) {
            image::DynamicImage::ImageRgba8(image)
                .to_rgb8()
                .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        } else {
            image.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        };
        encoded.ok()?;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&png);
        self.bitmap_bytes = self.bitmap_bytes.saturating_add(b64.len());
        Some(b64)
    }

    fn text(
        &mut self,
        text: &Text,
        fonts: &dyn MetafileFonts,
        faces: &mut FaceCache,
    ) -> Result<(), Full> {
        let bold = text.font.weight >= 600;
        let (bytes, index) = faces.get(fonts, &text.font.family, bold, text.font.italic);
        let Ok(font) = skrifa::FontRef::from_index(&bytes, index) else {
            self.missing_glyphs = self.missing_glyphs.saturating_add(count_ink(&text.text));
            return Ok(());
        };
        let unscaled = font.metrics(Size::unscaled(), LocationRef::default());
        let upem = f32::from(unscaled.units_per_em.max(1));
        let raw_height = if text.font.height.is_finite() && text.font.height != 0.0 {
            text.font.height
        } else {
            DEFAULT_TEXT_HEIGHT
        };
        // Negative: the em height; positive: the cell height (ascent plus descent).
        let em = if raw_height < 0.0 {
            -raw_height
        } else {
            let cell = (unscaled.ascent - unscaled.descent).max(1.0);
            raw_height * upem / cell
        };
        if !(em.is_finite() && em > 0.0) {
            return Ok(());
        }
        let size = Size::new(em);
        let metrics = font.metrics(size, LocationRef::default());
        let glyph_metrics = font.glyph_metrics(size, LocationRef::default());
        let charmap = font.charmap();
        let outlines = font.outline_glyphs();
        let sx = match metrics.average_width {
            Some(avg) if text.font.width > 0.0 && avg > 0.0 && text.font.width.is_finite() => {
                text.font.width / avg
            }
            _ => 1.0,
        };

        // Advances: the file's own when it gives one per character.
        let chars: Vec<char> = text.text.chars().collect();
        let given = text.dx.len() >= chars.len();
        let mut advances = Vec::with_capacity(chars.len());
        let mut glyphs = Vec::with_capacity(chars.len());
        for (i, ch) in chars.iter().enumerate() {
            let gid = charmap.map(*ch).filter(|g| g.to_u32() != 0);
            let adv = if given {
                text.dx.get(i).copied().unwrap_or(0.0)
            } else {
                gid.and_then(|g| glyph_metrics.advance_width(g))
                    .unwrap_or(em * 0.5)
                    * sx
            };
            advances.push(if adv.is_finite() { adv } else { 0.0 });
            glyphs.push(gid);
        }
        let total: f32 = advances.iter().sum();
        let x0 = match text.h_align {
            HAlign::Left => 0.0,
            HAlign::Center => -total / 2.0,
            HAlign::Right => -total,
        };
        let base = match text.v_align {
            VAlign::Top => metrics.ascent,
            VAlign::Baseline => 0.0,
            VAlign::Bottom => metrics.descent,
        };

        let mut body = String::new();
        let mut inner = 0usize;
        if let Some(bg) = text.background {
            let _ = write!(
                body,
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\"/>",
                num(f64::from(x0)),
                num(f64::from(base - metrics.ascent)),
                num(f64::from(total.abs())),
                num(f64::from(metrics.ascent - metrics.descent)),
                hex(bg)
            );
            inner += 1;
        }
        let mut pen = SvgPath::default();
        let mut x = x0;
        for (ch, (gid, adv)) in chars.iter().zip(glyphs.iter().zip(&advances)) {
            if !ch.is_whitespace() && !ch.is_control() {
                match gid.and_then(|g| outlines.get(g)) {
                    Some(glyph) if self.glyphs < MAX_GLYPHS => {
                        pen.origin = (x, base, sx);
                        let settings = DrawSettings::unhinted(size, LocationRef::default());
                        if glyph.draw(settings, &mut pen).is_err() {
                            self.missing_glyphs = self.missing_glyphs.saturating_add(1);
                        }
                        self.glyphs += 1;
                    }
                    _ => self.missing_glyphs = self.missing_glyphs.saturating_add(1),
                }
            }
            x += adv;
        }
        let (lo, hi) = (x0.min(x0 + total), x0.max(x0 + total));
        let mut decoration = |deco: Option<skrifa::metrics::Decoration>, fallback: f32| {
            let (offset, thickness) = deco.map_or((fallback, em / 14.0), |d| {
                (d.offset, d.thickness.max(em / 30.0))
            });
            let top = base - offset - thickness / 2.0;
            let _ = write!(
                pen.d,
                "M{} {}H{}V{}H{}Z",
                num(f64::from(lo)),
                num(f64::from(top)),
                num(f64::from(hi)),
                num(f64::from(top + thickness)),
                num(f64::from(lo))
            );
        };
        if text.font.underline {
            decoration(metrics.underline, -em / 10.0);
        }
        if text.font.strike {
            decoration(metrics.strikeout, em * 0.3);
        }
        if !pen.d.is_empty() {
            let _ = write!(body, "<path d=\"{}\" fill=\"{}\"", pen.d, hex(text.color));
            if text.color[3] != 255 {
                let _ = write!(
                    body,
                    " fill-opacity=\"{}\"",
                    num(f64::from(text.color[3]) / 255.0)
                );
            }
            body.push_str("/>");
            inner += 1;
        }
        if body.is_empty() {
            return Ok(());
        }
        let mut close = 0;
        if let Some([cx0, cy0, cx1, cy1]) = text.clip {
            let id = self.id("t");
            let _ = write!(
                self.out,
                "<clipPath id=\"{id}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/></clipPath><g clip-path=\"url(#{id})\">",
                num(f64::from(cx0)),
                num(f64::from(cy0)),
                num(f64::from(cx1 - cx0).max(0.0)),
                num(f64::from(cy1 - cy0).max(0.0))
            );
            self.elements = self.elements.saturating_add(3);
            close += 1;
        }
        let [a, b, c, d, e, f] = text.transform;
        let _ = write!(
            self.out,
            "<g transform=\"matrix({} {} {} {} {} {})\">{body}</g>",
            num(a),
            num(b),
            num(c),
            num(d),
            num(e),
            num(f)
        );
        for _ in 0..close {
            self.out.push_str("</g>");
        }
        self.elements = self.elements.saturating_add(inner);
        self.element()
    }
}

fn item_clip(item: &Item) -> (Option<&Vec<[f32; 4]>>, &[Arc<ClipPath>]) {
    match item {
        Item::Path { region, clips, .. }
        | Item::Bitmap { region, clips, .. }
        | Item::Pattern { region, clips, .. } => (region.as_ref(), clips),
        Item::Text(text) => (text.region.as_ref(), &text.clips),
    }
}

/// Glyph outlines into SVG path data: font units are already scaled to the
/// em; y is flipped (fonts are y up) and x stretched by the font width.
#[derive(Default)]
struct SvgPath {
    d: String,
    origin: (f32, f32, f32),
}

impl SvgPath {
    fn pt(&mut self, x: f32, y: f32) {
        let (ox, oy, sx) = self.origin;
        let _ = write!(
            self.d,
            "{} {}",
            num(f64::from(ox + x * sx)),
            num(f64::from(oy - y))
        );
    }
}

impl OutlinePen for SvgPath {
    fn move_to(&mut self, x: f32, y: f32) {
        self.d.push('M');
        self.pt(x, y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.d.push('L');
        self.pt(x, y);
    }
    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.d.push('Q');
        self.pt(cx0, cy0);
        self.d.push(' ');
        self.pt(x, y);
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.d.push('C');
        self.pt(cx0, cy0);
        self.d.push(' ');
        self.pt(cx1, cy1);
        self.d.push(' ');
        self.pt(x, y);
    }
    fn close(&mut self) {
        self.d.push('Z');
    }
}

fn count_ink(text: &str) -> u32 {
    let n = text
        .chars()
        .filter(|c| !c.is_whitespace() && !c.is_control())
        .count();
    u32::try_from(n).unwrap_or(u32::MAX)
}

fn path_data(segs: &[Seg]) -> String {
    let mut d = String::with_capacity(segs.len().saturating_mul(16));
    for seg in segs {
        match *seg {
            Seg::Move(x, y) => {
                let _ = write!(d, "M{} {}", num(f64::from(x)), num(f64::from(y)));
            }
            Seg::Line(x, y) => {
                let _ = write!(d, "L{} {}", num(f64::from(x)), num(f64::from(y)));
            }
            Seg::Cubic(x1, y1, x2, y2, x3, y3) => {
                let _ = write!(
                    d,
                    "C{} {} {} {} {} {}",
                    num(f64::from(x1)),
                    num(f64::from(y1)),
                    num(f64::from(x2)),
                    num(f64::from(y2)),
                    num(f64::from(x3)),
                    num(f64::from(y3))
                );
            }
            Seg::Close => d.push('Z'),
        }
    }
    d
}

fn hex(c: [u8; 4]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

/// A short decimal: three places, trailing zeros dropped, never `NaN`.
fn num(v: f64) -> String {
    if !v.is_finite() {
        return "0".to_string();
    }
    let mut s = format!("{v:.3}");
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    if s == "-0" { "0".to_string() } else { s }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::{Limits, decode_svg};
    use craft_metafile::write::{self, Op, Pen};
    use kurbo::{Affine, BezPath, Rect, Shape as _};

    fn square(x: f64, y: f64, side: f64) -> BezPath {
        Rect::new(x, y, x + side, y + side).to_path(0.1)
    }

    /// A 100 × 50 pt picture: a red square on the left, a blue stroke across.
    fn ops() -> Vec<Op> {
        let mut line = BezPath::new();
        line.move_to((0.0, 40.0));
        line.line_to((100.0, 40.0));
        vec![
            Op::Fill {
                path: square(10.0, 10.0, 20.0),
                even_odd: false,
                rgb: [255, 0, 0],
            },
            Op::Stroke {
                path: line,
                pen: Pen {
                    rgb: [0, 0, 255],
                    width: 4.0,
                    cap: Cap::Flat,
                    join: Join::Miter,
                    miter: 10.0,
                    dash: None,
                },
            },
        ]
    }

    fn region() -> Rect {
        Rect::new(0.0, 0.0, 100.0, 50.0)
    }

    fn pixel(img: &crate::Rgba8Image, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * img.width + x) * 4) as usize;
        [
            img.pixels[i],
            img.pixels[i + 1],
            img.pixels[i + 2],
            img.pixels[i + 3],
        ]
    }

    fn render(svg: &MetafileSvg) -> crate::Rgba8Image {
        // One pixel per point.
        decode_svg(&svg.svg, (100, 50), &Limits::default()).expect("the SVG renders")
    }

    #[test]
    fn emf_becomes_vector_svg_with_its_size_and_colours() {
        let emf = write::emf(&ops(), region(), "test", "clip art").expect("EMF written");
        assert!(is_metafile(&emf));
        let svg = metafile_to_svg(&emf, &BundledFonts).expect("converted");
        let text = String::from_utf8(svg.svg.clone()).unwrap();
        assert!(text.starts_with("<svg"), "{text}");
        assert!(text.contains("<path"), "vectors stay paths: {text}");
        assert!(!text.contains("<image"), "no bitmap for vector content");
        assert!((svg.width - 100.0 * 4.0 / 3.0).abs() < 1.0, "{}", svg.width);
        assert!(
            (svg.height - 50.0 * 4.0 / 3.0).abs() < 1.0,
            "{}",
            svg.height
        );
        let img = render(&svg);
        assert_eq!(pixel(&img, 20, 20), [255, 0, 0, 255]);
        assert_eq!(pixel(&img, 60, 40), [0, 0, 255, 255]);
        assert_eq!(
            pixel(&img, 60, 20)[3],
            0,
            "outside the drawing stays transparent"
        );
    }

    #[test]
    fn wmf_becomes_vector_svg() {
        let wmf = write::wmf(&ops(), region()).expect("WMF written");
        let svg = metafile_to_svg(&wmf.bytes, &BundledFonts).expect("converted");
        let img = render(&svg);
        assert_eq!(pixel(&img, 20, 20), [255, 0, 0, 255]);
        assert_eq!(pixel(&img, 60, 40), [0, 0, 255, 255]);
    }

    #[test]
    fn bitmaps_are_embedded_as_png() {
        let image = Arc::new(write::Image {
            width: 2,
            height: 2,
            rgba: [0, 255, 0, 255].repeat(4),
        });
        let ops = vec![Op::Image {
            image,
            xf: Affine::translate((50.0, 0.0)) * Affine::scale(25.0),
            opacity: 1.0,
        }];
        let emf = write::emf(&ops, region(), "test", "bitmap").expect("EMF written");
        let svg = metafile_to_svg(&emf, &BundledFonts).expect("converted");
        let text = String::from_utf8(svg.svg.clone()).unwrap();
        assert!(text.contains("data:image/png;base64,"), "{text}");
        let img = render(&svg);
        assert_eq!(pixel(&img, 60, 10), [0, 255, 0, 255]);
        assert_eq!(pixel(&img, 10, 10)[3], 0);
    }

    #[test]
    fn clip_paths_clip() {
        let ops = vec![
            Op::Clip {
                path: square(0.0, 0.0, 25.0),
                even_odd: false,
            },
            Op::Fill {
                path: square(0.0, 0.0, 50.0),
                even_odd: false,
                rgb: [255, 0, 0],
            },
            Op::Unclip,
        ];
        let emf = write::emf(&ops, region(), "test", "clip").expect("EMF written");
        let svg = metafile_to_svg(&emf, &BundledFonts).expect("converted");
        let text = String::from_utf8(svg.svg.clone()).unwrap();
        assert!(text.contains("<clipPath"), "{text}");
        let img = render(&svg);
        assert_eq!(pixel(&img, 10, 10), [255, 0, 0, 255]);
        assert_eq!(pixel(&img, 40, 40)[3], 0, "clipped away");
    }

    fn text_item(text: &str, family: &str) -> Item {
        Item::Text(Box::new(Text {
            transform: [1.0, 0.0, 0.0, 1.0, 10.0, 10.0],
            text: text.to_string(),
            dx: Vec::new(),
            font: craft_metafile::Font {
                family: family.to_string(),
                height: -30.0,
                weight: 700,
                ..Default::default()
            },
            color: [0, 0, 0, 255],
            background: None,
            h_align: HAlign::Left,
            v_align: VAlign::Top,
            clip: None,
            region: None,
            clips: Vec::new(),
        }))
    }

    #[test]
    fn text_is_drawn_as_glyph_outlines() {
        let picture = Picture {
            width: 100.0,
            height: 50.0,
            items: vec![text_item("HH", "Arial")],
            skipped_records: 0,
        };
        let svg = picture_to_svg(&picture, &BundledFonts).expect("converted");
        let text = String::from_utf8(svg.svg.clone()).unwrap();
        assert!(
            !text.contains("<text"),
            "the renderer draws no <text>: {text}"
        );
        assert_eq!(svg.missing_glyphs, 0);
        let img = render(&svg);
        // The H's left stem sits just right of the reference point, inside the cap height.
        let inked = (10..20).any(|x| (15..35).any(|y| pixel(&img, x, y)[3] > 200));
        assert!(inked, "the glyphs are visible");
    }

    #[test]
    fn missing_glyphs_are_counted() {
        let picture = Picture {
            width: 100.0,
            height: 50.0,
            items: vec![text_item("\u{F041}", "Wingdings")],
            skipped_records: 0,
        };
        let svg = picture_to_svg(&picture, &BundledFonts).expect("converted");
        assert_eq!(svg.missing_glyphs, 1);
    }

    #[test]
    fn regions_become_clip_rectangles_and_empty_regions_draw_nothing() {
        let fill = |region: Option<Vec<[f32; 4]>>| Item::Path {
            segs: vec![
                Seg::Move(0.0, 0.0),
                Seg::Line(50.0, 0.0),
                Seg::Line(50.0, 50.0),
                Seg::Line(0.0, 50.0),
                Seg::Close,
            ],
            fill: Some([255, 0, 0, 255]),
            stroke: None,
            line: LineStyle::default(),
            even_odd: false,
            region,
            clips: Vec::new(),
        };
        let picture = Picture {
            width: 100.0,
            height: 50.0,
            items: vec![
                fill(Some(vec![[0.0, 0.0, 20.0, 20.0]])),
                fill(Some(Vec::new())),
            ],
            skipped_records: 3,
        };
        let svg = picture_to_svg(&picture, &BundledFonts).expect("converted");
        assert_eq!(svg.skipped_records, 3);
        let img = render(&svg);
        assert_eq!(pixel(&img, 10, 10), [255, 0, 0, 255]);
        assert_eq!(pixel(&img, 30, 30)[3], 0);
    }

    #[test]
    fn large_bitmaps_are_scaled_down_until_the_svg_fits() {
        // Noise does not compress: 3000 × 3000 RGBA is far over the SVG cap.
        let mut seed = 0x1234_5678_u32;
        let rgba: Vec<u8> = (0..3000 * 3000 * 4)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                (seed & 0xff) as u8
            })
            .collect();
        let item = Item::Bitmap {
            rect: [0.0, 0.0, 100.0, 50.0],
            transform: [100.0, 0.0, 0.0, 50.0, 0.0, 0.0],
            width: 3000,
            height: 3000,
            rgba,
            region: None,
            clips: Vec::new(),
        };
        let picture = Picture {
            width: 100.0,
            height: 50.0,
            items: vec![item],
            skipped_records: 0,
        };
        let svg = picture_to_svg(&picture, &BundledFonts).expect("converted");
        assert!(svg.svg.len() <= MAX_SVG_BYTES);
        assert_eq!(svg.reduced_bitmaps, 1);
        render(&svg);
    }

    #[test]
    fn hostile_bytes_never_panic() {
        assert_eq!(
            metafile_to_svg(b"not a metafile", &BundledFonts),
            Err(MediaError::Unsupported)
        );
        let emf = write::emf(&ops(), region(), "test", "clip art").expect("EMF written");
        for len in (0..emf.len()).step_by(7) {
            let _ = metafile_to_svg(&emf[..len], &BundledFonts);
        }
        let mut flipped = emf.clone();
        for i in (0..flipped.len()).step_by(3) {
            flipped[i] ^= 0xa5;
            let _ = metafile_to_svg(&flipped, &BundledFonts);
        }
    }

    #[test]
    fn names_and_numbers() {
        assert!(is_metafile_name("Clip.EMF"));
        assert!(is_metafile_name("a.wmf"));
        assert!(!is_metafile_name("a.svg"));
        assert_eq!(metafile_svg_name("image3.emf"), "image3.svg");
        assert_eq!(metafile_svg_name("Logo.WMF"), "Logo.svg");
        assert_eq!(metafile_svg_name(".emf"), "picture.svg");
        assert_eq!(num(1.5), "1.5");
        assert_eq!(num(2.0), "2");
        assert_eq!(num(-0.0001), "0");
        assert_eq!(num(f64::NAN), "0");
    }
}
