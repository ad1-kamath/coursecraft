//! Word (`.docx`) writer for the review pages.
//!
//! The document holds the same content as the PDF: one landscape A4 section,
//! and for every [`ReviewPage`] a heading, the slide picture, the speaker notes
//! and the slide's text. It is a zip of OOXML parts with hand-written XML, so a
//! reviewer can open it in Word or Pages and comment and track changes. No XML
//! library is needed at run time.

use std::fmt::Write as _;
use std::io::Write as _;

use coursecraft_course::Course;
use coursecraft_render::MediaSource;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipWriter};

use crate::pages::{ExportError, ReviewOptions, ReviewPage, review_pages};

/// A4 landscape, in twips (1/1440 inch).
const PAGE_WIDTH_TWIPS: u32 = 16_838;
const PAGE_HEIGHT_TWIPS: u32 = 11_906;
/// Margin on every side, in twips (half an inch).
const MARGIN_TWIPS: u32 = 720;
/// Width available to text and pictures.
const TEXT_WIDTH_TWIPS: u32 = PAGE_WIDTH_TWIPS - 2 * MARGIN_TWIPS;
/// English Metric Units per twip (1 inch = 914,400 EMU = 1,440 twips).
const EMU_PER_TWIP: u64 = 635;
/// At 96 dpi a pixel is 0.75 pt, and the review layout uses 9144 EMU per pt,
/// so one pixel of the JPEG is 6858 EMU before scaling to the text width.
const EMU_PER_PIXEL: u64 = 9_144 * 3 / 4;

const RELATIONSHIPS_NS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const OFFICE_DOCUMENT_REL: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
const IMAGE_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";
const STYLES_REL: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles";

/// Write `pages` as a Word document.
///
/// The archive is deterministic: fixed part order, deflate compression and the
/// same 1980 timestamp as the publisher, so two runs are byte-identical.
///
/// # Errors
/// Returns [`ExportError::Encoding`] when a zip part cannot be written.
pub fn write_docx(course: &Course, pages: &[ReviewPage]) -> Result<Vec<u8>, ExportError> {
    // The course title is not part of the body; every page has its own heading.
    let _ = course;

    let mut writer = ZipWriter::new(std::io::Cursor::new(Vec::new()));
    write_part(
        &mut writer,
        "[Content_Types].xml",
        content_types_xml().as_bytes(),
    )?;
    write_part(
        &mut writer,
        "_rels/.rels",
        package_relationships_xml().as_bytes(),
    )?;
    write_part(
        &mut writer,
        "word/document.xml",
        document_xml(pages).as_bytes(),
    )?;
    write_part(
        &mut writer,
        "word/_rels/document.xml.rels",
        document_relationships_xml(pages).as_bytes(),
    )?;
    write_part(&mut writer, "word/styles.xml", styles_xml().as_bytes())?;
    for (index, page) in pages.iter().enumerate() {
        let name = format!("word/media/slide{}.jpg", index.saturating_add(1));
        write_part(&mut writer, &name, &page.jpeg)?;
    }
    let cursor = writer.finish().map_err(zip_error)?;
    Ok(cursor.into_inner())
}

/// Chain [`review_pages`] and [`write_docx`].
///
/// # Errors
/// Returns any error from drawing or encoding the pages, or writing the zip.
pub fn export_docx(
    course: &Course,
    media: &dyn MediaSource,
    options: &ReviewOptions,
) -> Result<Vec<u8>, ExportError> {
    let pages = review_pages(course, media, options)?;
    write_docx(course, &pages)
}

fn write_part(
    writer: &mut ZipWriter<std::io::Cursor<Vec<u8>>>,
    name: &str,
    bytes: &[u8],
) -> Result<(), ExportError> {
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .last_modified_time(DateTime::default());
    writer.start_file(name, options).map_err(zip_error)?;
    writer.write_all(bytes).map_err(zip_error)
}

fn zip_error(error: impl std::fmt::Display) -> ExportError {
    ExportError::Encoding(format!("zip: {error}"))
}

fn content_types_xml() -> String {
    let mut xml = String::new();
    xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n");
    xml.push_str("<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">");
    xml.push_str("<Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>");
    xml.push_str("<Default Extension=\"xml\" ContentType=\"application/xml\"/>");
    xml.push_str("<Default Extension=\"jpg\" ContentType=\"image/jpeg\"/>");
    xml.push_str("<Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/>");
    xml.push_str("<Override PartName=\"/word/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml\"/>");
    xml.push_str("</Types>");
    xml
}

fn package_relationships_xml() -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
         <Relationships xmlns=\"{RELATIONSHIPS_NS}\">\
         <Relationship Id=\"rId1\" Type=\"{OFFICE_DOCUMENT_REL}\" Target=\"word/document.xml\"/>\
         </Relationships>"
    )
}

fn document_relationships_xml(pages: &[ReviewPage]) -> String {
    let mut xml = String::new();
    xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n");
    let _ = write!(xml, "<Relationships xmlns=\"{RELATIONSHIPS_NS}\">");
    for index in 0..pages.len() {
        let id = index.saturating_add(1);
        let _ = write!(
            xml,
            "<Relationship Id=\"rId{id}\" Type=\"{IMAGE_REL}\" Target=\"media/slide{id}.jpg\"/>"
        );
    }
    let styles_id = pages.len().saturating_add(1);
    let _ = write!(
        xml,
        "<Relationship Id=\"rId{styles_id}\" Type=\"{STYLES_REL}\" Target=\"styles.xml\"/>"
    );
    xml.push_str("</Relationships>");
    xml
}

fn styles_xml() -> String {
    let mut xml = String::new();
    xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n");
    xml.push_str(
        "<w:styles xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">",
    );
    xml.push_str("<w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\">");
    xml.push_str("<w:name w:val=\"Normal\"/><w:qFormat/></w:style>");
    xml.push_str("<w:style w:type=\"paragraph\" w:styleId=\"Heading1\">");
    xml.push_str("<w:name w:val=\"heading 1\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/><w:qFormat/>");
    xml.push_str("<w:pPr><w:keepNext/><w:spacing w:before=\"240\" w:after=\"120\"/><w:outlineLvl w:val=\"0\"/></w:pPr>");
    xml.push_str("<w:rPr><w:b/><w:sz w:val=\"32\"/><w:szCs w:val=\"32\"/></w:rPr>");
    xml.push_str("</w:style></w:styles>");
    xml
}

fn document_xml(pages: &[ReviewPage]) -> String {
    let mut xml = String::new();
    xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n");
    xml.push_str(
        "<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" \
         xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" \
         xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\" \
         xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" \
         xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"><w:body>",
    );
    for (index, page) in pages.iter().enumerate() {
        push_heading(&mut xml, &page.heading);
        push_picture(&mut xml, index, page);
        if !page.notes.is_empty() {
            push_notes(&mut xml, &page.notes);
        }
        for line in &page.text {
            push_paragraph(&mut xml, line);
        }
        if index.saturating_add(1) < pages.len() {
            push_page_break(&mut xml);
        }
    }
    let _ = write!(
        xml,
        "<w:sectPr><w:pgSz w:w=\"{PAGE_WIDTH_TWIPS}\" w:h=\"{PAGE_HEIGHT_TWIPS}\" w:orient=\"landscape\"/>\
         <w:pgMar w:top=\"{MARGIN_TWIPS}\" w:right=\"{MARGIN_TWIPS}\" w:bottom=\"{MARGIN_TWIPS}\" \
         w:left=\"{MARGIN_TWIPS}\" w:header=\"0\" w:footer=\"0\" w:gutter=\"0\"/></w:sectPr>"
    );
    xml.push_str("</w:body></w:document>");
    xml
}

fn push_heading(xml: &mut String, heading: &str) {
    xml.push_str("<w:p><w:pPr><w:pStyle w:val=\"Heading1\"/></w:pPr>");
    xml.push_str("<w:r><w:t xml:space=\"preserve\">");
    push_escaped(xml, heading);
    xml.push_str("</w:t></w:r></w:p>");
}

fn push_picture(xml: &mut String, index: usize, page: &ReviewPage) {
    let id = index.saturating_add(1);
    let (cx, cy) = picture_extent(page);
    xml.push_str(
        "<w:p><w:r><w:drawing><wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\">",
    );
    let _ = write!(
        xml,
        "<wp:extent cx=\"{cx}\" cy=\"{cy}\"/><wp:docPr id=\"{id}\" name=\"Slide {id}\"/>"
    );
    xml.push_str("<a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">");
    xml.push_str("<pic:pic><pic:nvPicPr>");
    let _ = write!(xml, "<pic:cNvPr id=\"{id}\" name=\"slide{id}.jpg\"/>");
    xml.push_str("<pic:cNvPicPr/></pic:nvPicPr>");
    let _ = write!(
        xml,
        "<pic:blipFill><a:blip r:embed=\"rId{id}\"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>"
    );
    let _ = write!(
        xml,
        "<pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm>\
         <a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr>"
    );
    xml.push_str("</pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>");
}

/// The picture is as wide as the text column; its height follows the JPEG's
/// pixel aspect ratio.
fn picture_extent(page: &ReviewPage) -> (u64, u64) {
    let width = u64::from(TEXT_WIDTH_TWIPS).saturating_mul(EMU_PER_TWIP);
    if page.width == 0 || page.height == 0 {
        return (width, width.saturating_mul(9) / 16);
    }
    let natural_width = u64::from(page.width).saturating_mul(EMU_PER_PIXEL);
    let natural_height = u64::from(page.height).saturating_mul(EMU_PER_PIXEL);
    let height = width.saturating_mul(natural_height) / natural_width.max(1);
    (width, height.max(1))
}

fn push_notes(xml: &mut String, notes: &str) {
    xml.push_str("<w:p><w:r><w:rPr><w:b/></w:rPr><w:t xml:space=\"preserve\">Notes</w:t></w:r>");
    xml.push_str("<w:r><w:t xml:space=\"preserve\"> ");
    push_escaped(xml, notes);
    xml.push_str("</w:t></w:r></w:p>");
}

fn push_paragraph(xml: &mut String, text: &str) {
    xml.push_str("<w:p><w:r><w:t xml:space=\"preserve\">");
    push_escaped(xml, text);
    xml.push_str("</w:t></w:r></w:p>");
}

fn push_page_break(xml: &mut String) {
    xml.push_str("<w:p><w:r><w:br w:type=\"page\"/></w:r></w:p>");
}

/// Append `text` with the five XML escapes. Characters XML 1.0 forbids are
/// dropped rather than producing a document no reader can parse.
fn push_escaped(out: &mut String, text: &str) {
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\t' | '\n' | '\r' => out.push(ch),
            ch if (ch as u32) < 0x20 => {}
            ch => out.push(ch),
        }
    }
}
