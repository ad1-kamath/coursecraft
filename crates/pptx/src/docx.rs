//! Read plain text out of a `.docx` file (§ M9 step 3).
//!
//! Only `word/document.xml` is read. Paragraphs keep their order, a heading
//! style becomes `#`/`##`, a list paragraph becomes `- `, and table cells
//! become lines. Images and everything else are ignored.
//!
//! The bytes are hostile input: the archive entry count, the decompressed size
//! (32 MiB) and the XML nesting depth are bounded, and every error is returned
//! rather than panicking.

use std::io::Read as _;

use crate::{PptxError, XmlNode, parse_xml_tree};

/// Most uncompressed bytes read from a Word document (32 MiB).
pub const MAX_DOCX_BYTES: u64 = 32 * 1024 * 1024;

/// Most entries a Word archive may contain.
pub const MAX_DOCX_ENTRIES: usize = 10_000;

/// Deepest XML nesting accepted in `word/document.xml`.
pub const MAX_DOCX_XML_DEPTH: usize = 64;

/// What went wrong while reading a Word document.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DocxError {
    /// The bytes are not a valid zip archive.
    #[error("not a zip archive: {0}")]
    NotAZip(String),
    /// The archive has no `word/document.xml` part.
    #[error("the document has no word/document.xml")]
    MissingDocumentXml,
    /// The archive has more entries than permitted.
    #[error("too many archive entries")]
    TooManyEntries,
    /// `word/document.xml` is larger than the permitted decompressed size.
    #[error("the document is too large")]
    TooLarge,
    /// The document XML nests deeper than permitted.
    #[error("the document XML is nested too deeply")]
    XmlDepthExceeded,
    /// The document XML is not valid XML.
    #[error("the document XML is malformed: {0}")]
    MalformedXml(String),
    /// `word/document.xml` is not UTF-8.
    #[error("the document XML is not UTF-8")]
    NotUtf8,
}

impl From<zip::result::ZipError> for DocxError {
    fn from(error: zip::result::ZipError) -> Self {
        Self::NotAZip(error.to_string())
    }
}

impl From<std::io::Error> for DocxError {
    fn from(error: std::io::Error) -> Self {
        Self::NotAZip(error.to_string())
    }
}

impl From<PptxError> for DocxError {
    fn from(error: PptxError) -> Self {
        match error {
            PptxError::XmlDepthExceeded(_) => Self::XmlDepthExceeded,
            PptxError::MalformedXml { message, .. } => Self::MalformedXml(message),
            other => Self::MalformedXml(other.to_string()),
        }
    }
}

/// Read a Word document's text, never panicking on bad input.
///
/// # Errors
/// Returns [`DocxError`] when the bytes are not a zip, the document part is
/// missing, the decompressed size is over [`MAX_DOCX_BYTES`], or the XML is
/// malformed or too deeply nested.
pub fn docx_to_text(bytes: &[u8]) -> Result<String, DocxError> {
    docx_to_text_with_limits(bytes, MAX_DOCX_BYTES, MAX_DOCX_ENTRIES, MAX_DOCX_XML_DEPTH)
}

fn docx_to_text_with_limits(
    bytes: &[u8],
    max_bytes: u64,
    max_entries: usize,
    max_depth: usize,
) -> Result<String, DocxError> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
    if archive.len() > max_entries {
        return Err(DocxError::TooManyEntries);
    }

    let mut document: Option<Vec<u8>> = None;
    for index in 0..archive.len() {
        let mut file = archive.by_index(index)?;
        let name = file.name()?.to_string();
        if name != "word/document.xml" {
            continue;
        }
        if file.size() > max_bytes {
            return Err(DocxError::TooLarge);
        }
        let mut part = Vec::new();
        file.by_ref()
            .take(max_bytes.saturating_add(1))
            .read_to_end(&mut part)?;
        if part.len() as u64 > max_bytes {
            return Err(DocxError::TooLarge);
        }
        document = Some(part);
        break;
    }

    let document = document.ok_or(DocxError::MissingDocumentXml)?;
    let xml = std::str::from_utf8(&document).map_err(|_| DocxError::NotUtf8)?;
    let root = parse_xml_tree(xml, "word/document.xml", max_depth)?;
    let body = root
        .find_descendant("body")
        .ok_or_else(|| DocxError::MalformedXml("no body".to_string()))?;

    let mut lines = Vec::new();
    collect_blocks(body, &mut lines);
    Ok(lines.join("\n"))
}

/// Walk block children in document order, collecting paragraph lines.
fn collect_blocks(node: &XmlNode, out: &mut Vec<String>) {
    for child in &node.children {
        match child.name.as_str() {
            "p" => push_paragraph(child, out),
            "tbl" => collect_table(child, out),
            "sdt" => match child.child("sdtContent") {
                Some(content) => collect_blocks(content, out),
                None => collect_blocks(child, out),
            },
            "sectPr" => {}
            // Content controls and other wrappers may hold paragraphs.
            _ => collect_blocks(child, out),
        }
    }
}

fn collect_table(table: &XmlNode, out: &mut Vec<String>) {
    for row in table.children_named("tr") {
        for cell in row.children_named("tc") {
            collect_blocks(cell, out);
        }
    }
}

fn push_paragraph(paragraph: &XmlNode, out: &mut Vec<String>) {
    let mut text = String::new();
    collect_text(paragraph, &mut text);
    let text = text.trim();
    if text.is_empty() {
        return;
    }
    let style = paragraph_style(paragraph);
    let marker = if is_heading(&style, "title") || is_heading(&style, "heading1") {
        "# "
    } else if is_heading(&style, "heading2") {
        "## "
    } else if is_list(paragraph, &style) {
        "- "
    } else {
        ""
    };
    let mut line = String::with_capacity(marker.len() + text.len());
    line.push_str(marker);
    line.push_str(text);
    out.push(line);
}

/// The paragraph's `w:pStyle` value, normalised to lowercase without spaces.
fn paragraph_style(paragraph: &XmlNode) -> String {
    paragraph
        .child("pPr")
        .and_then(|properties| properties.child("pStyle"))
        .and_then(|style| style.attr("val"))
        .map(normalise_style)
        .unwrap_or_default()
}

fn normalise_style(style: &str) -> String {
    style
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '_' && *c != '-')
        .flat_map(char::to_lowercase)
        .collect()
}

fn is_heading(style: &str, wanted: &str) -> bool {
    style == wanted
}

fn is_list(paragraph: &XmlNode, style: &str) -> bool {
    let numbered = paragraph
        .child("pPr")
        .and_then(|properties| properties.child("numPr"))
        .is_some();
    numbered || style.starts_with("list")
}

/// Collect the visible text of a paragraph, ignoring images and drawings.
fn collect_text(node: &XmlNode, out: &mut String) {
    for child in &node.children {
        match child.name.as_str() {
            "t" => out.push_str(&child.text),
            "tab" => out.push(' '),
            "br" | "cr" => out.push(' '),
            // A nested paragraph is handled by its own block.
            "p" => {}
            _ => collect_text(child, out),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use std::io::Write as _;

    use super::*;

    fn zip_with(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buffer = std::io::Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut buffer);
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            for (name, bytes) in entries {
                zip.start_file(*name, options).unwrap();
                zip.write_all(bytes).unwrap();
            }
            zip.finish().unwrap();
        }
        buffer.into_inner()
    }

    fn document(xml: &str) -> Vec<u8> {
        zip_with(&[("word/document.xml", xml.as_bytes())])
    }

    fn wrap(body: &str) -> String {
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
             <w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\
             <w:body>{body}</w:body></w:document>"
        )
    }

    #[test]
    fn reads_headings_lists_tables_and_ignores_images() {
        let body = "\
            <w:p><w:pPr><w:pStyle w:val=\"Title\"/></w:pPr><w:r><w:t>Safety</w:t></w:r></w:p>\
            <w:p><w:pPr><w:pStyle w:val=\"Heading1\"/></w:pPr><w:r><w:t>Welcome</w:t></w:r></w:p>\
            <w:p><w:pPr><w:pStyle w:val=\"Heading 2\"/></w:pPr><w:r><w:t>Exits</w:t></w:r></w:p>\
            <w:p><w:r><w:t>Keep exits clear</w:t></w:r></w:p>\
            <w:p><w:pPr><w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"1\"/></w:numPr></w:pPr>\
                <w:r><w:t>First item</w:t></w:r></w:p>\
            <w:tbl><w:tr>\
                <w:tc><w:p><w:r><w:t>Cell A</w:t></w:r></w:p></w:tc>\
                <w:tc><w:p><w:r><w:t>Cell B</w:t></w:r></w:p></w:tc>\
            </w:tr></w:tbl>\
            <w:p><w:r><w:drawing><w:inline><w:docPr/></w:inline></w:drawing></w:r></w:p>";
        let bytes = document(&wrap(body));
        let text = docx_to_text(&bytes).expect("text");
        assert_eq!(
            text,
            "# Safety\n# Welcome\n## Exits\nKeep exits clear\n- First item\nCell A\nCell B"
        );
    }

    #[test]
    fn reads_style_names_case_and_space_insensitively() {
        let body = "\
            <w:p><w:pPr><w:pStyle w:val=\"heading1\"/></w:pPr><w:r><w:t>One</w:t></w:r></w:p>\
            <w:p><w:pPr><w:pStyle w:val=\"HEADING2\"/></w:pPr><w:r><w:t>Two</w:t></w:r></w:p>";
        let text = docx_to_text(&document(&wrap(body))).expect("text");
        assert_eq!(text, "# One\n## Two");
    }

    #[test]
    fn accepts_a_document_with_no_text() {
        let bytes = document(&wrap("<w:p/>"));
        assert_eq!(docx_to_text(&bytes).expect("text"), "");
    }

    #[test]
    fn refuses_bytes_that_are_not_a_zip() {
        assert!(matches!(
            docx_to_text(b"not a zip archive"),
            Err(DocxError::NotAZip(_))
        ));
    }

    #[test]
    fn refuses_a_zip_without_the_document_part() {
        let bytes = zip_with(&[("word/other.xml", b"<x/>")]);
        assert_eq!(
            docx_to_text(&bytes).unwrap_err(),
            DocxError::MissingDocumentXml
        );
    }

    #[test]
    fn refuses_bad_xml_without_panicking() {
        for xml in [
            "<w:document><w:body><w:p></w:body></w:document>",
            "<w:document><w:body>",
            "",
        ] {
            let bytes = document(xml);
            assert!(docx_to_text(&bytes).is_err(), "{xml:?} should fail");
        }
    }

    #[test]
    fn refuses_deeply_nested_xml() {
        let depth = MAX_DOCX_XML_DEPTH + 20;
        let mut body = String::new();
        for _ in 0..depth {
            body.push_str("<w:p>");
        }
        for _ in 0..depth {
            body.push_str("</w:p>");
        }
        let bytes = document(&wrap(&body));
        assert_eq!(
            docx_to_text(&bytes).unwrap_err(),
            DocxError::XmlDepthExceeded
        );
    }

    #[test]
    fn refuses_a_document_larger_than_the_limit() {
        // The private limit makes this fast: the real limit is 32 MiB.
        let body = "<w:p><w:r><w:t>abcdef</w:t></w:r></w:p>";
        let bytes = document(&wrap(body));
        assert_eq!(
            docx_to_text_with_limits(&bytes, 10, MAX_DOCX_ENTRIES, MAX_DOCX_XML_DEPTH).unwrap_err(),
            DocxError::TooLarge
        );
    }

    #[test]
    fn refuses_too_many_entries() {
        let bytes = zip_with(&[("a", b""), ("b", b""), ("word/document.xml", b"<x/>")]);
        assert_eq!(
            docx_to_text_with_limits(&bytes, MAX_DOCX_BYTES, 2, MAX_DOCX_XML_DEPTH).unwrap_err(),
            DocxError::TooManyEntries
        );
    }

    #[test]
    fn refuses_non_utf8_xml() {
        let bytes = document_bytes(b"<w:document>\xff</w:document>");
        assert_eq!(docx_to_text(&bytes).unwrap_err(), DocxError::NotUtf8);
    }

    fn document_bytes(xml: &[u8]) -> Vec<u8> {
        zip_with(&[("word/document.xml", xml)])
    }

    #[test]
    fn local_name_handles_prefixed_elements() {
        assert_eq!(crate::local_name("w:document"), "document");
        assert_eq!(crate::local_name("document"), "document");
    }
}
