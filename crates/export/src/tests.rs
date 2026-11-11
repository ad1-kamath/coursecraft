//! Tests for the MP4 exporter and the pure audio helpers.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use coursecraft_course::{Action, Course, SlideTarget};
use coursecraft_render::{MediaFetch, MediaSource};

use crate::audio_mix::{mix_add, resample_to_stereo};
use crate::docx::{export_docx, write_docx};
use crate::mp4::{ExportMedia, branch_slides, exceeds_max_total};
use crate::pages::{ReviewOptions, review_pages};
use crate::pdf::{encode_winansi, export_pdf, write_pdf};

// Only the macOS encoder tests below use it.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
struct NoMedia;

impl ExportMedia for NoMedia {
    fn bytes(&self, _src: &str) -> Option<Arc<[u8]>> {
        None
    }
}

struct NoRenderMedia;

impl MediaSource for NoRenderMedia {
    fn get(&self, _src: &str) -> MediaFetch {
        MediaFetch::Missing
    }
}

fn hello_course() -> Course {
    Course::from_json(include_str!("../../../examples/hello-course/course.json"))
        .expect("hello course parses")
}

#[test]
fn resampler_44100_to_48000_keeps_a_constant_signal_constant() {
    let input = vec![1000i16; 4410 * 2];
    let out = resample_to_stereo(&input, 2, 44_100);
    assert_eq!(out.len(), 4800 * 2, "one second at 48 kHz stereo");
    assert!(
        out.iter().all(|sample| *sample == 1000),
        "linear interpolation of a constant signal must not change it"
    );
}

#[test]
fn mix_add_saturates_instead_of_wrapping() {
    let mut dst = [i16::MAX, i16::MIN, 0, 100];
    mix_add(&mut dst, &[1000, -1000, 32_000, -50]);
    assert_eq!(dst, [i16::MAX, i16::MIN, 32_000, 50]);
}

#[test]
fn branch_scan_finds_a_jump_away_from_next() {
    let course = hello_course();
    assert!(
        branch_slides(&course).is_empty(),
        "hello-course only jumps to Next"
    );

    let mut course = course;
    course.scenes[0].slides[0].triggers[0].action = Action::JumpToSlide {
        target: SlideTarget::Id("s-tabs".to_owned()),
    };
    assert_eq!(branch_slides(&course), vec!["s-intro".to_owned()]);
}

#[test]
fn max_total_ms_is_a_hard_limit() {
    assert!(!exceeds_max_total(0, 0));
    assert!(!exceeds_max_total(100, 100));
    assert!(exceeds_max_total(101, 100));
    let options = crate::mp4::Mp4Options::default();
    assert_eq!(options.max_total_ms, 2 * 60 * 60 * 1000);
}

// ---------------------------------------------------------------- review pages and PDF

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn find_last(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).rposition(|w| w == needle)
}

/// Walk the xref table, check every object offset and every stream length,
/// and return the `/Count` from the page tree.
fn check_pdf(pdf: &[u8]) -> usize {
    assert!(pdf.starts_with(b"%PDF-1.4"), "PDF header");
    assert!(pdf.ends_with(b"%%EOF"), "PDF trailer marker");

    let startxref = find_last(pdf, b"startxref").expect("startxref");
    let mut cursor = startxref + b"startxref".len();
    while pdf.get(cursor).is_some_and(u8::is_ascii_whitespace) {
        cursor += 1;
    }
    let digits = cursor;
    while pdf.get(cursor).is_some_and(u8::is_ascii_digit) {
        cursor += 1;
    }
    let xref_offset: usize = std::str::from_utf8(&pdf[digits..cursor])
        .expect("xref offset is ASCII")
        .parse()
        .expect("xref offset is a number");
    assert!(
        pdf.get(xref_offset..)
            .is_some_and(|rest| rest.starts_with(b"xref\n")),
        "startxref points at the xref table"
    );

    let mut cursor = xref_offset + b"xref\n".len();
    let header_end = find(&pdf[cursor..], b"\n").expect("xref header line") + cursor;
    let header = std::str::from_utf8(&pdf[cursor..header_end]).expect("xref header");
    let mut fields = header.split_whitespace();
    assert_eq!(fields.next(), Some("0"));
    let count: usize = fields
        .next()
        .expect("object count")
        .parse()
        .expect("object count is a number");
    cursor = header_end + 1;

    let mut offsets = vec![0usize; count];
    for (number, slot) in offsets.iter_mut().enumerate() {
        assert!(cursor + 20 <= pdf.len(), "xref entry {number} is complete");
        let entry = &pdf[cursor..cursor + 20];
        if number == 0 {
            assert_eq!(&entry[10..18], b" 65535 f", "object 0 is free");
        } else {
            assert_eq!(&entry[10..18], b" 00000 n", "xref entry {number} kind");
        }
        *slot = std::str::from_utf8(&entry[..10])
            .expect("offset is ASCII")
            .parse()
            .expect("offset is a number");
        cursor += 20;
    }
    assert_eq!(offsets[0], 0, "object 0 is free");

    for number in 1..count {
        let offset = offsets[number];
        let expected = format!("{number} 0 obj\n");
        assert!(
            pdf.get(offset..)
                .is_some_and(|rest| rest.starts_with(expected.as_bytes())),
            "object {number} starts at its xref offset"
        );
        let end = if number + 1 < count {
            offsets[number + 1]
        } else {
            xref_offset
        };
        let object = &pdf[offset..end];

        if let Some(length_at) = find(object, b"/Length ") {
            let mut index = length_at + b"/Length ".len();
            let digits = index;
            while object.get(index).is_some_and(u8::is_ascii_digit) {
                index += 1;
            }
            let length: usize = std::str::from_utf8(&object[digits..index])
                .expect("stream length is ASCII")
                .parse()
                .expect("stream length is a number");
            let stream_at = find(&object[index..], b">>\nstream\n").expect("stream keyword")
                + index
                + b">>\nstream\n".len();
            let after = stream_at + length;
            assert!(
                object
                    .get(after..)
                    .is_some_and(|rest| rest.starts_with(b"\nendstream")),
                "object {number} stream length matches"
            );
        }
    }

    let pages_object = &pdf[offsets[2]..offsets[3]];
    let count_at = find(pages_object, b"/Count ").expect("page count");
    let mut index = count_at + b"/Count ".len();
    let digits = index;
    while pages_object.get(index).is_some_and(u8::is_ascii_digit) {
        index += 1;
    }
    std::str::from_utf8(&pages_object[digits..index])
        .expect("page count is ASCII")
        .parse()
        .expect("page count is a number")
}

#[test]
fn review_pages_follow_play_order_with_jpeg_pictures() {
    let course = hello_course();
    let pages =
        review_pages(&course, &NoRenderMedia, &ReviewOptions::default()).expect("review pages");
    assert_eq!(pages.len(), 4, "one page per play-order slide");
    let headings: Vec<&str> = pages.iter().map(|page| page.heading.as_str()).collect();
    assert_eq!(
        headings,
        [
            "1. Welcome",
            "2. Three hazards",
            "3. Check your knowledge",
            "4. Results"
        ]
    );
    for page in &pages {
        assert!(
            page.jpeg.starts_with(&[0xFF, 0xD8]),
            "{} is a JPEG",
            page.heading
        );
        assert!(
            page.width > 0 && page.height > 0,
            "{} has a size",
            page.heading
        );
    }
    assert_eq!(
        pages[0].text.first().map(String::as_str),
        Some("Workplace Safety Basics"),
        "text is ordered by frame y"
    );
    let quiz = &pages[2];
    assert!(
        quiz.text
            .iter()
            .any(|line| line == "You see a spill in the corridor. What do you do first?"),
        "{:?}",
        quiz.text
    );
    assert!(
        quiz.text
            .iter()
            .any(|line| line == "Clean it or mark it, then report it"),
        "{:?}",
        quiz.text
    );
}

#[test]
fn review_pages_adds_a_page_for_every_layer() {
    let course = hello_course();
    let options = ReviewOptions {
        include_layers: true,
        ..ReviewOptions::default()
    };
    let pages = review_pages(&course, &NoRenderMedia, &options).expect("review pages with layers");
    assert_eq!(pages.len(), 10, "4 slides plus 6 layer pages");
    let headings: Vec<&str> = pages.iter().map(|page| page.heading.as_str()).collect();
    assert!(headings.contains(&"1. Welcome ▸ About"), "{headings:?}");
    assert!(
        headings.contains(&"2. Three hazards ▸ Slips"),
        "{headings:?}"
    );
    assert!(
        headings.contains(&"3. Check your knowledge ▸ Correct"),
        "{headings:?}"
    );
    assert!(
        headings.contains(&"3. Check your knowledge ▸ Incorrect"),
        "{headings:?}"
    );
}

#[test]
fn winansi_encoder_replaces_unknown_chars_and_escapes() {
    assert_eq!(
        encode_winansi("Hi ☃ (there) \\ ok"),
        b"Hi ? \\(there\\) \\\\ ok".to_vec()
    );
    assert_eq!(
        encode_winansi("\u{201C}quoted\u{201D}"),
        vec![0x93, b'q', b'u', b'o', b't', b'e', b'd', 0x94],
        "CP1252 punctuation maps into WinAnsi"
    );
}

#[test]
fn pdf_has_a_valid_xref_and_matching_stream_lengths() {
    let course = hello_course();
    let options = ReviewOptions::default();
    let pdf = export_pdf(&course, &NoRenderMedia, &options).expect("export pdf");
    assert!(pdf.starts_with(b"%PDF-1.4"));
    assert!(pdf.ends_with(b"%%EOF"));
    let count = check_pdf(&pdf);
    assert_eq!(count, 4, "one PDF page per review page");
}

#[test]
fn pdf_page_count_matches_the_page_list() {
    let course = hello_course();
    let pages =
        review_pages(&course, &NoRenderMedia, &ReviewOptions::default()).expect("review pages");
    let pdf = write_pdf(&course, &pages).expect("write pdf");
    assert_eq!(check_pdf(&pdf), pages.len());
}

#[test]
fn long_notes_overflow_onto_a_continuation_page() {
    let course = hello_course();
    let mut pages =
        review_pages(&course, &NoRenderMedia, &ReviewOptions::default()).expect("review pages");
    pages.truncate(1);
    pages[0].notes = "A long note for the reviewer. ".repeat(400);
    let pdf = write_pdf(&course, &pages).expect("write pdf");
    let count = check_pdf(&pdf);
    assert!(
        count > pages.len(),
        "{count} pages for {} review page",
        pages.len()
    );
    assert!(
        find(&pdf, b"continued").is_some(),
        "the continuation page is headed (continued)"
    );
}

// ---------------------------------------------------------------- Word

fn read_zip_part(bytes: &[u8], name: &str) -> Vec<u8> {
    use std::io::Read as _;

    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("valid zip");
    let mut file = archive.by_name(name).expect("part exists");
    let mut out = Vec::new();
    file.read_to_end(&mut out).expect("read part");
    out
}

fn zip_part_names(bytes: &[u8]) -> Vec<String> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("valid zip");
    let mut names = Vec::with_capacity(archive.len());
    for index in 0..archive.len() {
        let file = archive.by_index(index).expect("part");
        names.push(file.name().expect("part name").into_owned());
    }
    names.sort();
    names
}

fn parse_xml(bytes: &[u8]) {
    let text = std::str::from_utf8(bytes).expect("part is UTF-8");
    let mut reader = quick_xml::reader::Reader::from_str(text);
    loop {
        let event = reader.read_event().expect("part parses");
        if matches!(event, quick_xml::events::Event::Eof) {
            break;
        }
    }
}

const DOCX_XML_PARTS: [&str; 5] = [
    "[Content_Types].xml",
    "_rels/.rels",
    "word/document.xml",
    "word/_rels/document.xml.rels",
    "word/styles.xml",
];

#[test]
fn docx_is_a_zip_with_exactly_the_review_parts() {
    let course = hello_course();
    let pages =
        review_pages(&course, &NoRenderMedia, &ReviewOptions::default()).expect("review pages");
    let docx =
        export_docx(&course, &NoRenderMedia, &ReviewOptions::default()).expect("export docx");
    assert!(docx.starts_with(b"PK"), "the document is a zip");

    let mut expected = DOCX_XML_PARTS
        .iter()
        .map(|name| (*name).to_owned())
        .collect::<Vec<_>>();
    for index in 1..=pages.len() {
        expected.push(format!("word/media/slide{index}.jpg"));
    }
    expected.sort();
    assert_eq!(zip_part_names(&docx), expected);
}

#[test]
fn docx_xml_parts_parse_with_quick_xml() {
    let course = hello_course();
    let pages =
        review_pages(&course, &NoRenderMedia, &ReviewOptions::default()).expect("review pages");
    let docx = write_docx(&course, &pages).expect("write docx");
    for name in DOCX_XML_PARTS {
        parse_xml(&read_zip_part(&docx, name));
    }
}

#[test]
fn docx_has_one_picture_per_page_and_every_embed_has_a_relationship() {
    let course = hello_course();
    let pages =
        review_pages(&course, &NoRenderMedia, &ReviewOptions::default()).expect("review pages");
    let docx = write_docx(&course, &pages).expect("write docx");

    let document = String::from_utf8(read_zip_part(&docx, "word/document.xml"))
        .expect("document.xml is UTF-8");
    let mut reader = quick_xml::reader::Reader::from_str(&document);
    let mut embeds = Vec::new();
    loop {
        let event = reader.read_event().expect("document.xml parses");
        match event {
            quick_xml::events::Event::Start(event) | quick_xml::events::Event::Empty(event) => {
                if event.name().as_ref().ends_with(b":blip") {
                    for attribute in event.attributes() {
                        let attribute = attribute.expect("attribute");
                        if attribute.key.as_ref() == b"r:embed" {
                            embeds.push(
                                String::from_utf8(attribute.value.into_owned())
                                    .expect("relationship id is UTF-8"),
                            );
                        }
                    }
                }
            }
            quick_xml::events::Event::Eof => break,
            _ => {}
        }
    }
    assert_eq!(embeds.len(), pages.len(), "one picture per review page");

    let rels = String::from_utf8(read_zip_part(&docx, "word/_rels/document.xml.rels"))
        .expect("document rels are UTF-8");
    let mut reader = quick_xml::reader::Reader::from_str(&rels);
    let mut ids = Vec::new();
    loop {
        let event = reader.read_event().expect("rels parse");
        match event {
            quick_xml::events::Event::Start(event) | quick_xml::events::Event::Empty(event) => {
                if event.name().as_ref() == b"Relationship"
                    || event.name().as_ref().ends_with(b":Relationship")
                {
                    for attribute in event.attributes() {
                        let attribute = attribute.expect("attribute");
                        if attribute.key.as_ref() == b"Id" {
                            ids.push(
                                String::from_utf8(attribute.value.into_owned())
                                    .expect("id is UTF-8"),
                            );
                        }
                    }
                }
            }
            quick_xml::events::Event::Eof => break,
            _ => {}
        }
    }
    for embed in &embeds {
        assert!(ids.contains(embed), "{embed} has a relationship");
    }
}

#[test]
fn docx_escapes_special_characters() {
    let course = hello_course();
    let mut pages =
        review_pages(&course, &NoRenderMedia, &ReviewOptions::default()).expect("review pages");
    pages.truncate(1);
    pages[0].heading = "1. A & B <C> \"D\" 'E'".to_owned();
    pages[0].notes = "A note with & and <tags>.".to_owned();
    pages[0].text = vec!["Text with & and <tags> and \"quotes\" and 'apostrophes'.".to_owned()];

    let docx = write_docx(&course, &pages).expect("write docx");
    let document = String::from_utf8(read_zip_part(&docx, "word/document.xml"))
        .expect("document.xml is UTF-8");
    assert!(
        document.contains("1. A &amp; B &lt;C&gt; &quot;D&quot; &apos;E&apos;"),
        "{document}"
    );
    assert!(
        document.contains("A note with &amp; and &lt;tags&gt;."),
        "{document}"
    );
    assert!(
        document.contains(
            "Text with &amp; and &lt;tags&gt; and &quot;quotes&quot; and &apos;apostrophes&apos;."
        ),
        "{document}"
    );
    assert!(!document.contains("1. A & B"), "no raw ampersand");
}

#[test]
fn docx_is_byte_identical_on_two_runs() {
    let course = hello_course();
    let pages =
        review_pages(&course, &NoRenderMedia, &ReviewOptions::default()).expect("review pages");
    let first = write_docx(&course, &pages).expect("first write");
    let second = write_docx(&course, &pages).expect("second write");
    assert_eq!(first, second, "deterministic zip");
}

#[cfg(target_os = "macos")]
mod encoder {
    use super::{NoMedia, hello_course};
    use crate::mp4::{ExportError, Mp4Options, export_mp4};
    use coursecraft_video_native::VideoFile;

    fn test_options() -> Mp4Options {
        Mp4Options {
            width: 320,
            fps: 10,
            slide_tail_ms: 0,
            max_total_ms: 60_000,
        }
    }

    #[test]
    fn exports_hello_course_and_reports_every_slide() {
        let course = hello_course();
        let dir = tempfile::tempdir().expect("temp dir");
        let out = dir.path().join("hello.mp4");
        let mut seen = std::collections::BTreeSet::new();
        let mut calls = 0u64;
        let report = export_mp4(&course, &NoMedia, &out, &test_options(), &mut |progress| {
            calls += 1;
            seen.insert(progress.slide_index);
            true
        })
        .expect("export succeeds");

        assert!(out.is_file(), "the MP4 file was written");
        assert!(report.frames > 0);
        assert!(calls >= 4, "progress was called at least once per slide");
        assert_eq!(seen, (0..4).collect(), "every slide was drawn");

        let expected: u64 = course
            .scenes
            .iter()
            .flat_map(|scene| scene.slides.iter())
            .map(|slide| u64::from(slide.duration_ms))
            .sum();
        let video = VideoFile::open(&out).expect("the MP4 reads back");
        let duration = video.duration_ms();
        assert!(
            duration.abs_diff(expected) <= 500,
            "expected about {expected} ms, got {duration} ms"
        );

        assert_eq!(report.skipped_questions, vec!["s-quiz".to_owned()]);
        assert!(report.branch_slides.is_empty());
    }

    #[test]
    fn cancelling_leaves_no_file() {
        let course = hello_course();
        let dir = tempfile::tempdir().expect("temp dir");
        let out = dir.path().join("cancelled.mp4");
        let mut calls = 0u64;
        let error = export_mp4(&course, &NoMedia, &out, &test_options(), &mut |_| {
            calls += 1;
            calls < 2
        })
        .expect_err("the second progress call cancels");

        assert!(matches!(error, ExportError::Cancelled));
        assert!(!out.exists(), "a cancelled export removes its partial file");
    }
}
