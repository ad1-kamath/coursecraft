//! Hand-written PDF 1.4 writer for the review pages.
//!
//! One A4 landscape page per [`ReviewPage`]: the heading, the slide picture as
//! a JPEG, the speaker notes and the slide's text. Text that does not fit
//! continues on a page headed `"{heading} (continued)"`. Helvetica is used, so
//! the writer needs no font program and no new dependency.

use coursecraft_course::Course;
use coursecraft_render::MediaSource;

use crate::pages::{ExportError, ReviewOptions, ReviewPage, review_pages};

/// A4 landscape, in points.
const PAGE_WIDTH: f32 = 842.0;
const PAGE_HEIGHT: f32 = 595.0;
/// Margin on every side, in points.
const MARGIN: f32 = 36.0;
/// Width available to text and pictures.
const CONTENT_WIDTH: f32 = PAGE_WIDTH - 2.0 * MARGIN;
const HEADING_SIZE: f32 = 16.0;
const NOTES_SIZE: f32 = 10.0;
const NOTES_LEADING: f32 = 12.0;
const TEXT_SIZE: f32 = 9.0;
const TEXT_LEADING: f32 = 11.0;

/// Helvetica advance widths for ASCII 0x20..=0x7E, in 1/1000 em.
const HELVETICA_WIDTHS: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556,
    556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556, 1015, 667, 667, 722, 722, 667,
    611, 778, 722, 278, 500, 667, 556, 833, 722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667,
    667, 611, 278, 278, 278, 469, 556, 333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500,
    222, 833, 556, 556, 556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584,
];

/// CP1252 characters that are not Latin-1, and their WinAnsi bytes.
const CP1252_SPECIALS: &[(char, u8)] = &[
    ('\u{20AC}', 0x80),
    ('\u{201A}', 0x82),
    ('\u{0192}', 0x83),
    ('\u{201E}', 0x84),
    ('\u{2026}', 0x85),
    ('\u{2020}', 0x86),
    ('\u{2021}', 0x87),
    ('\u{02C6}', 0x88),
    ('\u{2030}', 0x89),
    ('\u{0160}', 0x8A),
    ('\u{2039}', 0x8B),
    ('\u{0152}', 0x8C),
    ('\u{017D}', 0x8E),
    ('\u{2018}', 0x91),
    ('\u{2019}', 0x92),
    ('\u{201C}', 0x93),
    ('\u{201D}', 0x94),
    ('\u{2022}', 0x95),
    ('\u{2013}', 0x96),
    ('\u{2014}', 0x97),
    ('\u{02DC}', 0x98),
    ('\u{2122}', 0x99),
    ('\u{0161}', 0x9A),
    ('\u{203A}', 0x9B),
    ('\u{0153}', 0x9C),
    ('\u{017E}', 0x9E),
    ('\u{0178}', 0x9F),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Font {
    Bold,
    Regular,
}

struct Line {
    font: Font,
    size: f32,
    x: f32,
    baseline: f32,
    bytes: Vec<u8>,
}

struct PdfPage {
    /// The review page's heading without any continuation suffix.
    base: String,
    /// Index into the review page list for the picture, if this page has one.
    image: Option<usize>,
    image_rect: Option<(f32, f32, f32, f32)>,
    lines: Vec<Line>,
    image_obj: usize,
    page_obj: usize,
    content_obj: usize,
}

impl PdfPage {
    fn new(base: &str) -> Self {
        Self {
            base: base.to_owned(),
            image: None,
            image_rect: None,
            lines: Vec::new(),
            image_obj: 0,
            page_obj: 0,
            content_obj: 0,
        }
    }
}

/// Write `pages` as a PDF 1.4 document.
///
/// # Errors
/// The writer is currently infallible; the result matches the other exporters
/// and leaves room for a failing encoder later.
pub fn write_pdf(course: &Course, pages: &[ReviewPage]) -> Result<Vec<u8>, ExportError> {
    let mut layout = layout_pages(pages);

    // Object numbers: 1 catalogue, 2 page tree, 3 info, 4 bold font,
    // 5 regular font, then one image per page that has a picture, then the
    // page dictionaries, then the content streams.
    let mut next = 6usize;
    for page in &mut layout {
        if page.image.is_some() {
            page.image_obj = next;
            next = next.saturating_add(1);
        }
    }
    let page_base = next;
    for (index, page) in layout.iter_mut().enumerate() {
        page.page_obj = page_base.saturating_add(index);
    }
    let content_base = page_base.saturating_add(layout.len());
    for (index, page) in layout.iter_mut().enumerate() {
        page.content_obj = content_base.saturating_add(index);
    }
    let total = if layout.is_empty() {
        5
    } else {
        content_base.saturating_add(layout.len()).saturating_sub(1)
    };

    let mut objects: Vec<(usize, Vec<u8>)> = Vec::with_capacity(total);
    objects.push((1, b"<< /Type /Catalog /Pages 2 0 R >>".to_vec()));
    let kids = layout
        .iter()
        .map(|page| format!("{} 0 R", page.page_obj))
        .collect::<Vec<_>>()
        .join(" ");
    objects.push((
        2,
        format!(
            "<< /Type /Pages /Kids [{}] /Count {} >>",
            kids,
            layout.len()
        )
        .into_bytes(),
    ));
    objects.push((3, info_object(course)));
    objects.push((
        4,
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>"
            .to_vec(),
    ));
    objects.push((
        5,
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
            .to_vec(),
    ));
    for page in &layout {
        if let Some(index) = page.image
            && let Some(review) = pages.get(index)
        {
            objects.push((page.image_obj, image_object(review)));
        }
    }
    for page in &layout {
        objects.push((page.page_obj, page_object(page)));
    }
    for page in &layout {
        objects.push((page.content_obj, content_object(page)));
    }
    objects.sort_by_key(|(number, _)| *number);

    let mut out = Vec::new();
    out.extend_from_slice(b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n");
    let mut offsets = vec![0usize; total.saturating_add(1)];
    for (number, body) in &objects {
        if let Some(slot) = offsets.get_mut(*number) {
            *slot = out.len();
        }
        out.extend_from_slice(format!("{number} 0 obj\n").as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    }

    let xref_offset = out.len();
    out.extend_from_slice(b"xref\n");
    out.extend_from_slice(format!("0 {}\n", total.saturating_add(1)).as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for number in 1..=total {
        let offset = offsets.get(number).copied().unwrap_or(0);
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R /Info 3 0 R >>\nstartxref\n{}\n%%EOF",
            total.saturating_add(1),
            xref_offset
        )
        .as_bytes(),
    );
    Ok(out)
}

/// Chain [`review_pages`] and [`write_pdf`].
///
/// # Errors
/// Returns any error from drawing or encoding the pages, or writing the PDF.
pub fn export_pdf(
    course: &Course,
    media: &dyn MediaSource,
    options: &ReviewOptions,
) -> Result<Vec<u8>, ExportError> {
    let pages = review_pages(course, media, options)?;
    write_pdf(course, &pages)
}

fn info_object(course: &Course) -> Vec<u8> {
    let mut info = Vec::new();
    info.extend_from_slice(b"<< /Title (");
    info.extend_from_slice(&encode_winansi(&course.title));
    info.extend_from_slice(b") /Producer (CourseCraft) >>");
    info
}

fn image_object(page: &ReviewPage) -> Vec<u8> {
    let mut object = Vec::new();
    object.extend_from_slice(
        format!(
            "<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream\n",
            page.width,
            page.height,
            page.jpeg.len()
        )
        .as_bytes(),
    );
    object.extend_from_slice(&page.jpeg);
    object.extend_from_slice(b"\nendstream");
    object
}

fn page_object(page: &PdfPage) -> Vec<u8> {
    let mut dict = format!(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {PAGE_WIDTH} {PAGE_HEIGHT}] /Resources << /Font << /F1 4 0 R /F2 5 0 R >>"
    );
    if page.image.is_some() {
        dict.push_str(&format!(" /XObject << /Im0 {} 0 R >>", page.image_obj));
    }
    dict.push_str(&format!(" >> /Contents {} 0 R >>", page.content_obj));
    dict.into_bytes()
}

fn content_object(page: &PdfPage) -> Vec<u8> {
    let mut data = Vec::new();
    if let Some((x, y, width, height)) = page.image_rect {
        data.extend_from_slice(format!("q {width} 0 0 {height} {x} {y} cm /Im0 Do Q\n").as_bytes());
    }
    for line in &page.lines {
        let font = match line.font {
            Font::Bold => "F1",
            Font::Regular => "F2",
        };
        data.extend_from_slice(
            format!(
                "BT /{font} {} Tf 1 0 0 1 {} {} Tm (",
                line.size, line.x, line.baseline
            )
            .as_bytes(),
        );
        data.extend_from_slice(&line.bytes);
        data.extend_from_slice(b") Tj ET\n");
    }
    let mut object = Vec::new();
    object.extend_from_slice(format!("<< /Length {} >>\nstream\n", data.len()).as_bytes());
    object.extend_from_slice(&data);
    object.extend_from_slice(b"\nendstream");
    object
}

/// Lay out every review page, adding continuation pages where text overflows.
fn layout_pages(pages: &[ReviewPage]) -> Vec<PdfPage> {
    let top = PAGE_HEIGHT - MARGIN;
    let mut out = Vec::new();
    for (index, page) in pages.iter().enumerate() {
        let mut current = PdfPage::new(&page.heading);
        let mut cursor = top;
        push_heading(&mut current, &mut cursor, &page.heading);

        if page.width > 0 && page.height > 0 {
            let height = CONTENT_WIDTH * page.height as f32 / page.width as f32;
            if height.is_finite() && height > 0.0 {
                current.image = Some(index);
                current.image_rect = Some((MARGIN, cursor - height, CONTENT_WIDTH, height));
                cursor = cursor - height - 12.0;
            }
        }

        let notes = wrap(&page.notes, NOTES_SIZE, CONTENT_WIDTH);
        for line in notes {
            ensure_room(&mut current, &mut out, &mut cursor, NOTES_SIZE);
            push_line(
                &mut current,
                &mut cursor,
                Font::Regular,
                NOTES_SIZE,
                NOTES_LEADING,
                &line,
            );
        }

        if !page.notes.is_empty() && !page.text.is_empty() {
            cursor -= 6.0;
        }

        let mut text = Vec::new();
        for paragraph in &page.text {
            text.extend(wrap(paragraph, TEXT_SIZE, CONTENT_WIDTH));
        }
        for line in text {
            ensure_room(&mut current, &mut out, &mut cursor, TEXT_SIZE);
            push_line(
                &mut current,
                &mut cursor,
                Font::Regular,
                TEXT_SIZE,
                TEXT_LEADING,
                &line,
            );
        }
        out.push(current);
    }
    out
}

fn push_heading(page: &mut PdfPage, cursor: &mut f32, text: &str) {
    page.lines.push(Line {
        font: Font::Bold,
        size: HEADING_SIZE,
        x: MARGIN,
        baseline: *cursor - HEADING_SIZE,
        bytes: encode_winansi(text),
    });
    *cursor -= HEADING_SIZE + 8.0;
}

fn push_line(
    page: &mut PdfPage,
    cursor: &mut f32,
    font: Font,
    size: f32,
    leading: f32,
    text: &str,
) {
    page.lines.push(Line {
        font,
        size,
        x: MARGIN,
        baseline: *cursor - size,
        bytes: encode_winansi(text),
    });
    *cursor -= leading;
}

fn ensure_room(page: &mut PdfPage, out: &mut Vec<PdfPage>, cursor: &mut f32, size: f32) {
    if *cursor - size >= MARGIN {
        return;
    }
    let heading = format!("{} (continued)", page.base);
    let mut next = PdfPage::new(&page.base);
    let mut next_cursor = PAGE_HEIGHT - MARGIN;
    push_heading(&mut next, &mut next_cursor, &heading);
    let previous = std::mem::replace(page, next);
    out.push(previous);
    *cursor = next_cursor;
}

/// Wrap `text` to `max_width` points at `size`.
fn wrap(text: &str, size: f32, max_width: f32) -> Vec<String> {
    let space = char_width(' ', size);
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        let mut width = 0.0f32;
        for word in paragraph.split_whitespace() {
            let word_width = text_width(word, size);
            if !line.is_empty() && width + space + word_width > max_width {
                lines.push(std::mem::take(&mut line));
                width = 0.0;
            }
            if !line.is_empty() {
                line.push(' ');
                width += space;
            }
            for ch in word.chars() {
                let advance = char_width(ch, size);
                if !line.is_empty() && width + advance > max_width {
                    lines.push(std::mem::take(&mut line));
                    width = 0.0;
                }
                line.push(ch);
                width += advance;
            }
        }
        if !line.is_empty() {
            lines.push(line);
        }
    }
    lines
}

fn text_width(text: &str, size: f32) -> f32 {
    text.chars().map(|ch| char_width(ch, size)).sum()
}

fn char_width(ch: char, size: f32) -> f32 {
    let code = ch as u32;
    let units = if (0x20..=0x7E).contains(&code) {
        HELVETICA_WIDTHS
            .get((code - 0x20) as usize)
            .copied()
            .unwrap_or(556) as f32
    } else {
        550.0
    };
    units * size / 1000.0
}

/// Encode `text` as a WinAnsi PDF string body.
///
/// Characters outside WinAnsi become `?`; the string delimiters `(`, `)` and
/// the backslash are escaped.
#[must_use]
pub fn encode_winansi(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    for ch in text.chars() {
        let byte = winansi_byte(ch).unwrap_or(b'?');
        if matches!(byte, b'(' | b')' | b'\\') {
            out.push(b'\\');
        }
        out.push(byte);
    }
    out
}

fn winansi_byte(ch: char) -> Option<u8> {
    let code = ch as u32;
    if (0x20..=0x7E).contains(&code) || (0xA0..=0xFF).contains(&code) {
        return Some(code as u8);
    }
    CP1252_SPECIALS
        .iter()
        .find_map(|(candidate, byte)| (*candidate == ch).then_some(*byte))
}
