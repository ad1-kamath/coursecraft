//! XLIFF 1.2 export and import for course text.
//!
//! Export walks every learner-visible string in the model and writes one
//! `<trans-unit>` per string with a stable id. Import reads a hostile file
//! (bounded size, depth, unit count and unit size, no DTD or entity
//! declarations), applies the units whose source still matches the course, and
//! reports everything it skipped.
//!
//! String variables are deliberately not exported: triggers can compare them,
//! so translating one would change play-time behaviour.

use coursecraft_course::{CHROME_STRINGS, Course, Object, ObjectKind, TextRun, align_paragraphs};
use quick_xml::events::attributes::Attribute;
use quick_xml::events::{BytesDecl, BytesStart, BytesText, Event};
use quick_xml::reader::Reader;
use quick_xml::writer::Writer;

#[cfg(test)]
mod tests;

/// XLIFF 1.2 namespace.
pub const XLIFF_NS: &str = "urn:oasis:names:tc:xliff:document:1.2";
/// Largest XLIFF file accepted by [`parse_xliff`] (32 MiB).
pub const MAX_XLIFF_BYTES: usize = 32 * 1024 * 1024;
/// Largest number of `<trans-unit>` elements accepted.
pub const MAX_UNITS: usize = 200_000;
/// Largest XML nesting depth accepted.
pub const MAX_DEPTH: usize = 16;
/// Largest text (source plus target) accepted for one unit (256 KiB).
pub const MAX_UNIT_BYTES: usize = 256 * 1024;

/// An XLIFF problem. Every hostile input produces one of these, never a panic.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum XliffError {
    /// The file is larger than [`MAX_XLIFF_BYTES`].
    #[error("XLIFF file is too large")]
    TooLarge,
    /// The file has more than [`MAX_UNITS`] translation units.
    #[error("XLIFF file has too many trans-units")]
    TooManyUnits,
    /// XML nesting is deeper than [`MAX_DEPTH`].
    #[error("XLIFF XML is nested too deeply")]
    TooDeep,
    /// One unit's text exceeds [`MAX_UNIT_BYTES`].
    #[error("XLIFF trans-unit is too large")]
    UnitTooLarge,
    /// A DOCTYPE or entity declaration was present.
    #[error("XLIFF must not contain a DOCTYPE or entity declaration")]
    Doctype,
    /// The input is not valid UTF-8.
    #[error("XLIFF is not valid UTF-8: {0}")]
    InvalidUtf8(String),
    /// The XML is malformed or contains an unknown entity.
    #[error("invalid XLIFF XML: {0}")]
    Xml(String),
    /// The document is not XLIFF 1.2.
    #[error("unsupported XLIFF version {0:?}; expected 1.2")]
    UnsupportedVersion(String),
    /// The document has no `<xliff>` root or no `<body>`.
    #[error("XLIFF document is missing its xliff/body elements")]
    MissingStructure,
    /// A `<trans-unit>` has no `id`.
    #[error("XLIFF trans-unit is missing its id")]
    MissingId,
}

/// One run of text in a unit. `run` is the index from `<g id="rN">`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TextSegment {
    /// Segment text with XML entities already decoded.
    pub text: String,
    /// Run index from a `<g id="rN">`, or `None` for the base style.
    pub run: Option<usize>,
}

/// The text of a `<source>` or `<target>`, split into run segments.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParsedText {
    /// Segments in document order.
    pub segments: Vec<TextSegment>,
}

impl ParsedText {
    /// The concatenated text, with no markup.
    #[must_use]
    pub fn text(&self) -> String {
        self.segments.iter().map(|s| s.text.as_str()).collect()
    }

    /// True when there is no text at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.segments.iter().all(|s| s.text.is_empty())
    }

    /// Drop the whitespace the writer puts between `<g>` elements.
    fn drop_gutter(&mut self) {
        if !self.segments.iter().any(|segment| segment.run.is_some()) {
            return;
        }
        self.segments
            .retain(|segment| segment.run.is_some() || !segment.text.trim().is_empty());
    }

    fn push(&mut self, text: &str, run: Option<usize>) {
        if text.is_empty() {
            return;
        }
        if let Some(last) = self.segments.last_mut()
            && last.run == run
        {
            last.text.push_str(text);
            return;
        }
        self.segments.push(TextSegment {
            text: text.to_owned(),
            run,
        });
    }
}

/// One `<trans-unit>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XliffUnit {
    /// Stable unit id, e.g. `slide/s1/title`.
    pub id: String,
    /// The source text as exported.
    pub source: ParsedText,
    /// The translation, when the file has a non-empty `<target>`.
    pub target: Option<ParsedText>,
}

/// A parsed XLIFF 1.2 file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XliffDocument {
    /// Source language from `<file source-language>`.
    pub source_language: String,
    /// Target language from `<file target-language>`.
    pub target_language: String,
    /// Translation units in document order.
    pub units: Vec<XliffUnit>,
}

/// What [`apply_xliff`] changed and skipped.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ImportReport {
    /// Units applied to the course.
    pub applied: usize,
    /// Units skipped because their target was absent or empty.
    pub missing_target: Vec<String>,
    /// Units skipped because their source no longer matches the course.
    pub source_changed: Vec<String>,
    /// Units whose id is not in this course.
    pub unknown_ids: Vec<String>,
    /// Units skipped because `%var%` or `{name}` placeholders changed.
    pub placeholder_mismatch: Vec<String>,
    /// Applied units whose text uses a script the bundled font lacks.
    pub unsupported_script: Vec<String>,
}

struct ExportUnit<'a> {
    id: String,
    text: &'a str,
    runs: Option<&'a [TextRun]>,
}

/// Export every learner-visible string to XLIFF 1.2.
///
/// # Errors
/// Returns [`XliffError::Xml`] if writing fails.
pub fn export_xliff(course: &Course, target_language: &str) -> Result<String, XliffError> {
    let source_language = course.language.as_deref().unwrap_or("en");
    let units = collect_units(course);

    let mut writer = Writer::new_with_indent(Vec::new(), b' ', 2);
    write_decl(&mut writer)?;
    writer
        .create_element("xliff")
        .with_attribute(("version", "1.2"))
        .with_attribute(("xmlns", XLIFF_NS))
        .write_inner_content(|writer| {
            writer
                .create_element("file")
                .with_attribute(("original", "course.json"))
                .with_attribute(("datatype", "plaintext"))
                .with_attribute(("source-language", source_language))
                .with_attribute(("target-language", target_language))
                .write_inner_content(|writer| {
                    writer
                        .create_element("body")
                        .write_inner_content(|writer| {
                            for unit in &units {
                                write_unit(writer, unit, None)?;
                            }
                            Ok(())
                        })?;
                    Ok(())
                })?;
            Ok(())
        })
        .map_err(xml_error)?;

    String::from_utf8(writer.into_inner()).map_err(|e| XliffError::Xml(e.to_string()))
}

/// One learner-visible string an AI translator is asked to translate.
///
/// `source` carries the same formatting markers the XLIFF path understands:
/// a text object with runs becomes `<g id="r0">…</g><g id="r1">…</g>`. Plain
/// text has no markers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranslationUnit {
    /// Stable unit id, as in the exported XLIFF.
    pub id: String,
    /// Source text, with formatting markers and XML-escaped character data.
    pub source: String,
}

/// Every string that [`export_xliff`] would write, as `{ id, source }`.
///
/// `target_language` is accepted so callers can pass the language they will
/// fill in; it does not change the list.
#[must_use]
pub fn translation_units(course: &Course, _target_language: &str) -> Vec<TranslationUnit> {
    collect_units(course)
        .into_iter()
        .map(|unit| TranslationUnit {
            id: unit.id,
            source: match unit.runs {
                Some(runs) => runs
                    .iter()
                    .enumerate()
                    .map(|(index, run)| {
                        format!("<g id=\"r{index}\">{}</g>", escape_marker_text(&run.text))
                    })
                    .collect(),
                None => escape_marker_text(unit.text),
            },
        })
        .collect()
}

/// Build an XLIFF 1.2 file from a course and finished targets.
///
/// `targets` maps unit id to target text, in the same marker form as
/// [`TranslationUnit::source`]. An empty or missing target writes no
/// `<target>` element; the importer treats that unit as untranslated and
/// leaves the course text alone. Target text whose markers are malformed
/// returns [`XliffError::Xml`].
///
/// # Errors
/// Returns [`XliffError::Xml`] if writing fails or a target's markers do not
/// parse.
pub fn fill_xliff(
    course: &Course,
    target_language: &str,
    targets: &std::collections::BTreeMap<String, String>,
) -> Result<String, XliffError> {
    let source_language = course.language.as_deref().unwrap_or("en");
    let units = collect_units(course);
    let mut parsed: Vec<Option<ParsedText>> = Vec::with_capacity(units.len());
    for unit in &units {
        let target = targets
            .get(&unit.id)
            .filter(|text| !text.is_empty())
            .map(|text| parse_marked_text(text))
            .transpose()?;
        parsed.push(target);
    }

    let mut writer = Writer::new_with_indent(Vec::new(), b' ', 2);
    write_decl(&mut writer)?;
    writer
        .create_element("xliff")
        .with_attribute(("version", "1.2"))
        .with_attribute(("xmlns", XLIFF_NS))
        .write_inner_content(|writer| {
            writer
                .create_element("file")
                .with_attribute(("original", "course.json"))
                .with_attribute(("datatype", "plaintext"))
                .with_attribute(("source-language", source_language))
                .with_attribute(("target-language", target_language))
                .write_inner_content(|writer| {
                    writer
                        .create_element("body")
                        .write_inner_content(|writer| {
                            for (unit, target) in units.iter().zip(&parsed) {
                                write_unit(writer, unit, target.as_ref())?;
                            }
                            Ok(())
                        })?;
                    Ok(())
                })?;
            Ok(())
        })
        .map_err(xml_error)?;

    String::from_utf8(writer.into_inner()).map_err(|e| XliffError::Xml(e.to_string()))
}

fn escape_marker_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Parse the marker form used by [`TranslationUnit`] back into run segments.
fn parse_marked_text(text: &str) -> Result<ParsedText, XliffError> {
    let mut parsed = ParsedText::default();
    let mut plain = String::new();
    let mut rest = text;
    while let Some(open) = rest.find('<') {
        let Some(before) = rest.get(..open) else {
            return Err(XliffError::Xml("invalid marker text".to_owned()));
        };
        plain.push_str(before);
        let Some(after) = rest.get(open..) else {
            return Err(XliffError::Xml("invalid marker text".to_owned()));
        };
        if let Some((run, consumed)) = parse_open_tag(after) {
            parsed.push(&unescape_marker_text(&plain), None);
            plain.clear();
            let Some(inner) = after.get(consumed..) else {
                return Err(XliffError::Xml("invalid formatting marker".to_owned()));
            };
            let Some(close) = inner.find("</g>") else {
                return Err(XliffError::Xml(
                    "formatting marker is not closed".to_owned(),
                ));
            };
            let Some(segment) = inner.get(..close) else {
                return Err(XliffError::Xml("invalid formatting marker".to_owned()));
            };
            parsed.push(&unescape_marker_text(segment), Some(run));
            rest = inner.get(close.saturating_add(4)..).unwrap_or_default();
        } else if after.starts_with("</g>") {
            return Err(XliffError::Xml(
                "formatting marker closes without opening".to_owned(),
            ));
        } else {
            plain.push('<');
            rest = after.get(1..).unwrap_or_default();
        }
    }
    plain.push_str(rest);
    parsed.push(&unescape_marker_text(&plain), None);
    Ok(parsed)
}

/// Parse `<g id="rN">` at the start of `text`, returning the run index and the
/// number of bytes consumed.
fn parse_open_tag(text: &str) -> Option<(usize, usize)> {
    let rest = text.strip_prefix("<g")?;
    let mut offset = 2usize;
    let mut cursor = rest;
    let skip_space = |cursor: &mut &str, offset: &mut usize| {
        let trimmed = cursor.trim_start_matches([' ', '\t', '\r', '\n']);
        *offset = offset.saturating_add(cursor.len().saturating_sub(trimmed.len()));
        *cursor = trimmed;
    };
    skip_space(&mut cursor, &mut offset);
    let after_id = cursor.strip_prefix("id")?;
    offset = offset.saturating_add(2);
    cursor = after_id;
    skip_space(&mut cursor, &mut offset);
    let after_eq = cursor.strip_prefix('=')?;
    offset = offset.saturating_add(1);
    cursor = after_eq;
    skip_space(&mut cursor, &mut offset);
    let after_quote = cursor.strip_prefix('"')?;
    offset = offset.saturating_add(1);
    cursor = after_quote;
    let after_r = cursor.strip_prefix('r')?;
    offset = offset.saturating_add(1);
    let digits = after_r.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    let index = after_r.get(..digits)?.parse::<usize>().ok()?;
    offset = offset.saturating_add(digits);
    let after_digits = after_r.get(digits..)?;
    let after_close_quote = after_digits.strip_prefix('"')?;
    offset = offset.saturating_add(1);
    let after_space = after_close_quote.trim_start_matches([' ', '\t', '\r', '\n']);
    offset = offset.saturating_add(after_close_quote.len().saturating_sub(after_space.len()));
    let after_gt = after_space.strip_prefix('>')?;
    offset = offset.saturating_add(1);
    let _ = after_gt;
    Some((index, offset))
}

fn unescape_marker_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('&') {
        let Some(before) = rest.get(..open) else {
            break;
        };
        out.push_str(before);
        let Some(after) = rest.get(open..) else {
            break;
        };
        let Some(semi) = after.find(';') else {
            out.push_str(after);
            return out;
        };
        let Some(entity) = after.get(1..semi) else {
            out.push_str(after);
            return out;
        };
        match entity {
            "amp" => out.push('&'),
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "quot" => out.push('"'),
            "apos" => out.push('\''),
            _ => {
                if let Some(character) = numeric_reference(entity) {
                    out.push(character);
                } else {
                    let Some(original) = after.get(..=semi) else {
                        break;
                    };
                    out.push_str(original);
                }
            }
        }
        rest = after.get(semi.saturating_add(1)..).unwrap_or_default();
    }
    out.push_str(rest);
    out
}

fn numeric_reference(entity: &str) -> Option<char> {
    let digits = entity.strip_prefix('#')?;
    let code = if let Some(hex) = digits.strip_prefix(['x', 'X']) {
        u32::from_str_radix(hex, 16).ok()?
    } else {
        digits.parse::<u32>().ok()?
    };
    char::from_u32(code)
}

fn write_decl(writer: &mut Writer<Vec<u8>>) -> Result<(), XliffError> {
    writer
        .write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))
        .map_err(xml_error)?;
    Ok(())
}

fn write_unit(
    writer: &mut Writer<Vec<u8>>,
    unit: &ExportUnit<'_>,
    target: Option<&ParsedText>,
) -> std::io::Result<()> {
    writer
        .create_element("trans-unit")
        .with_attribute(("id", unit.id.as_str()))
        .with_attribute(("xml:space", "preserve"))
        .write_inner_content(|writer| {
            writer
                .create_element("source")
                .write_inner_content(|writer| {
                    match unit.runs {
                        Some(runs) => {
                            for (index, run) in runs.iter().enumerate() {
                                writer
                                    .create_element("g")
                                    .with_attribute(("id", format!("r{index}").as_str()))
                                    .write_text_content(BytesText::new(&run.text))?;
                            }
                        }
                        None => {
                            writer.write_event(Event::Text(BytesText::new(unit.text)))?;
                        }
                    }
                    Ok(())
                })?;
            if let Some(target) = target {
                writer
                    .create_element("target")
                    .write_inner_content(|writer| write_segments(writer, target))?;
            }
            Ok(())
        })?;
    Ok(())
}

fn write_segments(writer: &mut Writer<Vec<u8>>, text: &ParsedText) -> std::io::Result<()> {
    for segment in &text.segments {
        match segment.run {
            Some(index) => {
                let id = format!("r{index}");
                writer
                    .create_element("g")
                    .with_attribute(("id", id.as_str()))
                    .write_text_content(BytesText::new(&segment.text))?;
            }
            None if !segment.text.is_empty() => {
                writer.write_event(Event::Text(BytesText::new(&segment.text)))?;
            }
            None => {}
        }
    }
    Ok(())
}

fn collect_units(course: &Course) -> Vec<ExportUnit<'_>> {
    let mut units = Vec::new();
    if !course.title.is_empty() {
        units.push(ExportUnit {
            id: "course/title".to_owned(),
            text: &course.title,
            runs: None,
        });
    }
    for scene in &course.scenes {
        if !scene.title.is_empty() {
            units.push(ExportUnit {
                id: format!("scene/{}/title", scene.id),
                text: &scene.title,
                runs: None,
            });
        }
        for slide in &scene.slides {
            collect_slide_units(&mut units, slide);
        }
    }
    for bank in &course.banks {
        for slide in &bank.slides {
            collect_slide_units(&mut units, slide);
        }
    }
    for master in &course.masters {
        if !master.title.is_empty() {
            units.push(ExportUnit {
                id: format!("master/{}/title", master.id),
                text: &master.title,
                runs: None,
            });
        }
        if !master.notes.is_empty() {
            units.push(ExportUnit {
                id: format!("master/{}/notes", master.id),
                text: &master.notes,
                runs: None,
            });
        }
        for object in &master.objects {
            collect_object_units(&mut units, "master", &master.id, object);
        }
    }
    for (key, _) in CHROME_STRINGS {
        let text = course.chrome_string(key);
        if !text.is_empty() {
            units.push(ExportUnit {
                id: format!("chrome/{key}"),
                text,
                runs: None,
            });
        }
    }
    units
}

/// Export units for one playable slide (a scene slide or a bank slide).
fn collect_slide_units<'a>(units: &mut Vec<ExportUnit<'a>>, slide: &'a coursecraft_course::Slide) {
    if !slide.title.is_empty() {
        units.push(ExportUnit {
            id: format!("slide/{}/title", slide.id),
            text: &slide.title,
            runs: None,
        });
    }
    if !slide.notes.is_empty() {
        units.push(ExportUnit {
            id: format!("slide/{}/notes", slide.id),
            text: &slide.notes,
            runs: None,
        });
    }
    for object in slide
        .objects
        .iter()
        .chain(slide.layers.iter().flat_map(|layer| layer.objects.iter()))
    {
        collect_object_units(units, "slide", &slide.id, object);
    }
    if let Some(question) = &slide.question {
        let sid = &slide.id;
        if !question.prompt.is_empty() {
            units.push(ExportUnit {
                id: format!("slide/{sid}/question/prompt"),
                text: &question.prompt,
                runs: None,
            });
        }
        for choice in &question.choices {
            if !choice.text.is_empty() {
                units.push(ExportUnit {
                    id: format!("slide/{sid}/question/choice/{}", choice.id),
                    text: &choice.text,
                    runs: None,
                });
            }
        }
        for (index, answer) in question.accepted.iter().enumerate() {
            if !answer.is_empty() {
                units.push(ExportUnit {
                    id: format!("slide/{sid}/question/accepted/{index}"),
                    text: answer,
                    runs: None,
                });
            }
        }
        for pair in &question.pairs {
            if !pair.left.is_empty() {
                units.push(ExportUnit {
                    id: format!("slide/{sid}/question/pair/{}/left", pair.id),
                    text: &pair.left,
                    runs: None,
                });
            }
            if !pair.right.is_empty() {
                units.push(ExportUnit {
                    id: format!("slide/{sid}/question/pair/{}/right", pair.id),
                    text: &pair.right,
                    runs: None,
                });
            }
        }
    }
}

fn collect_object_units<'a>(
    units: &mut Vec<ExportUnit<'a>>,
    owner: &str,
    slide_id: &str,
    object: &'a Object,
) {
    let oid = &object.id;
    let prefix = format!("{owner}/{slide_id}/object/{oid}");
    match &object.kind {
        ObjectKind::Text { text, runs, .. } => {
            if !runs.is_empty() {
                let joined: String = runs.iter().map(|run| run.text.as_str()).collect();
                if !joined.is_empty() {
                    units.push(ExportUnit {
                        id: format!("{prefix}/text"),
                        text,
                        runs: Some(runs.as_slice()),
                    });
                }
            } else if !text.is_empty() {
                units.push(ExportUnit {
                    id: format!("{prefix}/text"),
                    text,
                    runs: None,
                });
            }
        }
        ObjectKind::Button { label, .. } => {
            if !label.is_empty() {
                units.push(ExportUnit {
                    id: format!("{prefix}/label"),
                    text: label,
                    runs: None,
                });
            }
        }
        ObjectKind::Image { alt, .. } => {
            if !alt.is_empty() {
                units.push(ExportUnit {
                    id: format!("{prefix}/alt"),
                    text: alt,
                    runs: None,
                });
            }
        }
        ObjectKind::Shape {
            text: Some(text), ..
        } => {
            if !text.runs.is_empty() {
                let joined: String = text.runs.iter().map(|run| run.text.as_str()).collect();
                if !joined.is_empty() {
                    units.push(ExportUnit {
                        id: format!("{prefix}/text"),
                        text: &text.text,
                        runs: Some(text.runs.as_slice()),
                    });
                }
            } else if !text.text.is_empty() {
                units.push(ExportUnit {
                    id: format!("{prefix}/text"),
                    text: &text.text,
                    runs: None,
                });
            }
        }
        ObjectKind::Table { rows, .. } => {
            for (row_index, row) in rows.iter().enumerate() {
                for (column, cell) in row.cells.iter().enumerate() {
                    let id = format!("{prefix}/table/{row_index}/{column}/text");
                    if !cell.runs.is_empty() {
                        let joined: String =
                            cell.runs.iter().map(|run| run.text.as_str()).collect();
                        if !joined.is_empty() {
                            units.push(ExportUnit {
                                id,
                                text: &cell.text,
                                runs: Some(cell.runs.as_slice()),
                            });
                        }
                    } else if !cell.text.is_empty() {
                        units.push(ExportUnit {
                            id,
                            text: &cell.text,
                            runs: None,
                        });
                    }
                }
            }
        }
        ObjectKind::TextEntry { placeholder, .. } => {
            if !placeholder.is_empty() {
                units.push(ExportUnit {
                    id: format!("{prefix}/placeholder"),
                    text: placeholder,
                    runs: None,
                });
            }
        }
        ObjectKind::Checkbox { label, .. } => {
            if !label.is_empty() {
                units.push(ExportUnit {
                    id: format!("{prefix}/label"),
                    text: label,
                    runs: None,
                });
            }
        }
        ObjectKind::RadioGroup { options, .. } => {
            for (index, option) in options.iter().enumerate() {
                if !option.label.is_empty() {
                    units.push(ExportUnit {
                        id: format!("{prefix}/options/{index}/label"),
                        text: &option.label,
                        runs: None,
                    });
                }
            }
        }
        ObjectKind::Marker {
            title, body, runs, ..
        } => push_marker_units(units, &prefix, title, body, runs),
        ObjectKind::Shape { text: None, .. }
        | ObjectKind::Path { .. }
        | ObjectKind::Audio { .. }
        | ObjectKind::Video { .. }
        | ObjectKind::Slider { .. }
        | ObjectKind::Dial { .. }
        | ObjectKind::ScrollPanel { .. }
        | ObjectKind::Timer { .. }
        | ObjectKind::ProgressMeter { .. }
        | ObjectKind::Web { .. }
        | ObjectKind::Lottie { .. } => {}
    }
    if let Some(description) = &object.description
        && !description.is_empty()
    {
        units.push(ExportUnit {
            id: format!("{prefix}/description"),
            text: description,
            runs: None,
        });
    }
    for state in &object.states {
        if let Some(text) = &state.text
            && !text.is_empty()
        {
            units.push(ExportUnit {
                id: format!("{prefix}/state/{}/text", state.name),
                text,
                runs: None,
            });
        }
    }
}

fn push_marker_units<'a>(
    units: &mut Vec<ExportUnit<'a>>,
    prefix: &str,
    title: &'a str,
    body: &'a str,
    runs: &'a [TextRun],
) {
    if !title.is_empty() {
        units.push(ExportUnit {
            id: format!("{prefix}/title"),
            text: title,
            runs: None,
        });
    }
    if !runs.is_empty() {
        let joined: String = runs.iter().map(|run| run.text.as_str()).collect();
        if !joined.is_empty() {
            units.push(ExportUnit {
                id: format!("{prefix}/body"),
                text: body,
                runs: Some(runs),
            });
        }
    } else if !body.is_empty() {
        units.push(ExportUnit {
            id: format!("{prefix}/body"),
            text: body,
            runs: None,
        });
    }
}

fn xml_error(error: std::io::Error) -> XliffError {
    XliffError::Xml(error.to_string())
}

// ---------------------------------------------------------------- parsing

#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    None,
    Source,
    Target,
}

struct UnitBuilder {
    id: String,
    source: ParsedText,
    target: Option<ParsedText>,
    bytes: usize,
}

impl UnitBuilder {
    fn new(id: String) -> Self {
        Self {
            id,
            source: ParsedText::default(),
            target: None,
            bytes: 0,
        }
    }

    fn push(&mut self, text: &str) -> Result<(), XliffError> {
        self.bytes = self
            .bytes
            .checked_add(text.len())
            .ok_or(XliffError::UnitTooLarge)?;
        if self.bytes > MAX_UNIT_BYTES {
            return Err(XliffError::UnitTooLarge);
        }
        Ok(())
    }
}

/// Parse XLIFF 1.2 bytes.
///
/// # Errors
/// Rejects oversize files, too many units, deep nesting, a DOCTYPE or entity
/// declaration, invalid UTF-8, malformed XML and versions other than 1.2.
pub fn parse_xliff(bytes: &[u8]) -> Result<XliffDocument, XliffError> {
    if bytes.len() > MAX_XLIFF_BYTES {
        return Err(XliffError::TooLarge);
    }
    let text = std::str::from_utf8(bytes).map_err(|e| XliffError::InvalidUtf8(e.to_string()))?;
    reject_declarations(text)?;
    reject_control_chars(text)?;

    let mut reader = Reader::from_str(text);
    reader.config_mut().trim_text(false);

    let mut depth = 0usize;
    let mut version: Option<String> = None;
    let mut seen_xliff = false;
    let mut seen_file = false;
    let mut seen_body = false;
    let mut source_language = String::from("en");
    let mut target_language = String::new();
    let mut section = Section::None;
    let mut unit: Option<UnitBuilder> = None;
    let mut units = Vec::new();
    let mut g_stack: Vec<Option<usize>> = Vec::new();

    loop {
        let event = reader
            .read_event()
            .map_err(|e| XliffError::Xml(e.to_string()))?;
        match event {
            Event::Start(start) => {
                depth = depth.saturating_add(1);
                if depth > MAX_DEPTH {
                    return Err(XliffError::TooDeep);
                }
                let name = local_name(start.name().into_inner());
                match name {
                    "xliff" => {
                        seen_xliff = true;
                        if let Some(v) = attribute(&reader, &start, "version")? {
                            if v != "1.2" {
                                return Err(XliffError::UnsupportedVersion(v));
                            }
                            version = Some(v);
                        }
                    }
                    "file" => {
                        seen_file = true;
                        if let Some(v) = attribute(&reader, &start, "source-language")? {
                            source_language = v;
                        }
                        if let Some(v) = attribute(&reader, &start, "target-language")? {
                            target_language = v;
                        }
                    }
                    "body" => seen_body = true,
                    "trans-unit" => {
                        if unit.is_some() {
                            return Err(XliffError::Xml("nested trans-unit".to_owned()));
                        }
                        let id = attribute(&reader, &start, "id")?.ok_or(XliffError::MissingId)?;
                        if units.len() >= MAX_UNITS {
                            return Err(XliffError::TooManyUnits);
                        }
                        unit = Some(UnitBuilder::new(id));
                    }
                    "source" => section = Section::Source,
                    "target" => {
                        section = Section::Target;
                        if let Some(unit) = &mut unit {
                            unit.target.get_or_insert_with(ParsedText::default);
                        }
                    }
                    "g" => {
                        let id = attribute(&reader, &start, "id")?;
                        g_stack.push(id.as_deref().and_then(run_index));
                    }
                    _ => {}
                }
            }
            Event::Empty(start) => {
                let name = local_name(start.name().into_inner());
                match name {
                    "xliff" => {
                        if let Some(v) = attribute(&reader, &start, "version")? {
                            if v != "1.2" {
                                return Err(XliffError::UnsupportedVersion(v));
                            }
                            version = Some(v);
                        }
                    }
                    "file" => {
                        if let Some(v) = attribute(&reader, &start, "source-language")? {
                            source_language = v;
                        }
                        if let Some(v) = attribute(&reader, &start, "target-language")? {
                            target_language = v;
                        }
                    }
                    _ => {}
                }
            }
            Event::End(end) => {
                let name = local_name(end.name().into_inner());
                match name {
                    "source" | "target" => section = Section::None,
                    "g" => {
                        let _ = g_stack.pop();
                    }
                    "trans-unit" => {
                        if let Some(mut built) = unit.take() {
                            built.source.drop_gutter();
                            if let Some(target) = &mut built.target {
                                target.drop_gutter();
                            }
                            units.push(XliffUnit {
                                id: built.id,
                                source: built.source,
                                target: built.target,
                            });
                        }
                    }
                    _ => {}
                }
                depth = depth.saturating_sub(1);
            }
            Event::Text(text) => {
                if section != Section::None
                    && let Some(unit) = &mut unit
                {
                    let decoded = text.decode().map_err(|e| XliffError::Xml(e.to_string()))?;
                    let unescaped = quick_xml::escape::unescape(&decoded)
                        .map_err(|e| XliffError::Xml(e.to_string()))?;
                    append_text(
                        unit,
                        section,
                        g_stack.first().copied().flatten(),
                        &unescaped,
                    )?;
                }
            }
            Event::CData(text) => {
                if section != Section::None
                    && let Some(unit) = &mut unit
                {
                    let decoded = text.decode().map_err(|e| XliffError::Xml(e.to_string()))?;
                    append_text(unit, section, g_stack.first().copied().flatten(), &decoded)?;
                }
            }
            Event::GeneralRef(reference) => {
                if section != Section::None
                    && let Some(unit) = &mut unit
                {
                    let value = resolve_reference(&reference)?;
                    append_text(unit, section, g_stack.first().copied().flatten(), &value)?;
                }
            }
            Event::Eof => break,
            Event::Decl(_) | Event::Comment(_) | Event::PI(_) => {}
            Event::DocType(_) => return Err(XliffError::Doctype),
        }
    }

    if !seen_xliff || !seen_file || !seen_body || version.is_none() {
        return Err(XliffError::MissingStructure);
    }
    if unit.is_some() {
        return Err(XliffError::Xml("unclosed trans-unit".to_owned()));
    }

    Ok(XliffDocument {
        source_language,
        target_language,
        units,
    })
}

fn append_text(
    unit: &mut UnitBuilder,
    section: Section,
    run: Option<usize>,
    text: &str,
) -> Result<(), XliffError> {
    unit.push(text)?;
    match section {
        Section::Source => unit.source.push(text, run),
        Section::Target => unit
            .target
            .get_or_insert_with(ParsedText::default)
            .push(text, run),
        Section::None => {}
    }
    Ok(())
}

fn resolve_reference(reference: &quick_xml::events::BytesRef<'_>) -> Result<String, XliffError> {
    if let Some(c) = reference
        .resolve_char_ref()
        .map_err(|e| XliffError::Xml(e.to_string()))?
    {
        return Ok(c.to_string());
    }
    let name = reference
        .decode()
        .map_err(|e| XliffError::Xml(e.to_string()))?;
    match name.as_ref() {
        "lt" => Ok("<".to_owned()),
        "gt" => Ok(">".to_owned()),
        "amp" => Ok("&".to_owned()),
        "apos" => Ok("'".to_owned()),
        "quot" => Ok("\"".to_owned()),
        other => Err(XliffError::Xml(format!("unknown entity &{other};"))),
    }
}

fn attribute<B: std::io::BufRead>(
    reader: &Reader<B>,
    start: &BytesStart<'_>,
    want: &str,
) -> Result<Option<String>, XliffError> {
    for attr in start.attributes() {
        let attr: Attribute<'_> = attr.map_err(|e| XliffError::Xml(e.to_string()))?;
        let key = local_name(attr.key.into_inner());
        if key == want || (want == "xml:space" && key == "space") {
            let value = attr
                .decoded_and_normalized_value(quick_xml::XmlVersion::Implicit1_0, reader.decoder())
                .map_err(|e| XliffError::Xml(e.to_string()))?;
            return Ok(Some(value.into_owned()));
        }
    }
    Ok(None)
}

fn local_name(name: &[u8]) -> &str {
    let name = std::str::from_utf8(name).unwrap_or_default();
    name.rsplit(':').next().unwrap_or(name)
}

fn run_index(id: &str) -> Option<usize> {
    id.strip_prefix('r')?.parse::<usize>().ok()
}

fn reject_declarations(text: &str) -> Result<(), XliffError> {
    let lower = text.to_ascii_lowercase();
    if lower.contains("<!doctype") || lower.contains("<!entity") {
        return Err(XliffError::Doctype);
    }
    if text.contains("]]>") {
        return Err(XliffError::Xml(
            "raw ]]> is not allowed in character data".to_owned(),
        ));
    }
    Ok(())
}

fn reject_control_chars(text: &str) -> Result<(), XliffError> {
    if text
        .chars()
        .any(|c| matches!(c, '\u{0}'..='\u{8}' | '\u{B}' | '\u{C}' | '\u{E}'..='\u{1F}' | '\u{FFFE}' | '\u{FFFF}'))
    {
        return Err(XliffError::Xml(
            "control character in XML text".to_owned(),
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------- applying

/// Apply every usable unit to a copy of `course`.
///
/// Units with no target, a changed source, an unknown id or different
/// placeholders are left alone and listed in the report. The caller must
/// validate the returned course before saving it.
#[must_use]
pub fn apply_xliff(course: &Course, doc: &XliffDocument) -> (Course, ImportReport) {
    let mut out = course.clone();
    let mut report = ImportReport::default();
    for unit in &doc.units {
        apply_unit(&mut out, unit, &mut report);
    }
    if !doc.target_language.is_empty() {
        out.language = Some(doc.target_language.clone());
    }
    (out, report)
}

fn apply_unit(course: &mut Course, unit: &XliffUnit, report: &mut ImportReport) {
    let Some(current) = current_text(course, &unit.id) else {
        report.unknown_ids.push(unit.id.clone());
        return;
    };
    let source = unit.source.text();
    if source != current {
        report.source_changed.push(unit.id.clone());
        return;
    }
    let Some(target) = &unit.target else {
        report.missing_target.push(unit.id.clone());
        return;
    };
    if target.is_empty() {
        report.missing_target.push(unit.id.clone());
        return;
    }
    let target_text = target.text();
    if !placeholders_match(&source, &target_text, unit.id.starts_with("chrome/")) {
        report.placeholder_mismatch.push(unit.id.clone());
        return;
    }
    if !set_text(course, &unit.id, target) {
        report.unknown_ids.push(unit.id.clone());
        return;
    }
    report.applied = report.applied.saturating_add(1);
    if target_text
        .chars()
        .any(coursecraft_course::script_unsupported)
    {
        report.unsupported_script.push(unit.id.clone());
    }
}

fn current_text(course: &Course, id: &str) -> Option<String> {
    if id == "course/title" {
        return Some(course.title.clone());
    }
    if let Some(key) = id.strip_prefix("chrome/") {
        return CHROME_STRINGS
            .iter()
            .any(|(k, _)| *k == key)
            .then(|| course.chrome_string(key).to_owned());
    }
    if let Some(rest) = id.strip_prefix("scene/")
        && let Some(scene_id) = rest.strip_suffix("/title")
    {
        return course
            .scenes
            .iter()
            .find(|scene| scene.id == scene_id)
            .map(|scene| scene.title.clone());
    }
    if let Some(rest) = id.strip_prefix("master/") {
        let (master_id, field) = rest.split_once('/')?;
        let master = course
            .masters
            .iter()
            .find(|master| master.id == master_id)?;
        return slide_text(master, field);
    }
    let rest = id.strip_prefix("slide/")?;
    let (slide_id, field) = rest.split_once('/')?;
    let slide = course.play_slide(slide_id)?;
    slide_text(slide, field)
}

fn slide_text(slide: &coursecraft_course::Slide, field: &str) -> Option<String> {
    if field == "title" {
        return Some(slide.title.clone());
    }
    if field == "notes" {
        return Some(slide.notes.clone());
    }
    if let Some(object_field) = field.strip_prefix("object/") {
        let (object_id, field) = object_field.split_once('/')?;
        let (object, _) = slide.object(object_id)?;
        return object_text(object, field);
    }
    let question = slide.question.as_ref()?;
    if field == "question/prompt" {
        return Some(question.prompt.clone());
    }
    if let Some(choice_id) = field.strip_prefix("question/choice/") {
        return question
            .choices
            .iter()
            .find(|choice| choice.id == choice_id)
            .map(|choice| choice.text.clone());
    }
    if let Some(index) = field.strip_prefix("question/accepted/") {
        let index = index.parse::<usize>().ok()?;
        return question.accepted.get(index).cloned();
    }
    let pair_field = field.strip_prefix("question/pair/")?;
    let (pair_id, side) = pair_field.split_once('/')?;
    let pair = question.pairs.iter().find(|pair| pair.id == pair_id)?;
    match side {
        "left" => Some(pair.left.clone()),
        "right" => Some(pair.right.clone()),
        _ => None,
    }
}

fn object_text(object: &Object, field: &str) -> Option<String> {
    if let Some(rest) = field
        .strip_prefix("table/")
        .and_then(|rest| rest.strip_suffix("/text"))
    {
        let (row, column) = rest.split_once('/')?;
        let row: usize = row.parse().ok()?;
        let column: usize = column.parse().ok()?;
        let ObjectKind::Table { rows, .. } = &object.kind else {
            return None;
        };
        let cell = rows.get(row)?.cells.get(column)?;
        return Some(if cell.runs.is_empty() {
            cell.text.clone()
        } else {
            cell.runs.iter().map(|run| run.text.as_str()).collect()
        });
    }
    if field == "title" {
        return match &object.kind {
            ObjectKind::Marker { title, .. } => Some(title.clone()),
            _ => None,
        };
    }
    if field == "body" {
        return match &object.kind {
            ObjectKind::Marker { body, runs, .. } => {
                if runs.is_empty() {
                    Some(body.clone())
                } else {
                    Some(runs.iter().map(|run| run.text.as_str()).collect())
                }
            }
            _ => None,
        };
    }
    if field == "text" {
        return match &object.kind {
            ObjectKind::Text { text, runs, .. } => {
                if runs.is_empty() {
                    Some(text.clone())
                } else {
                    Some(runs.iter().map(|run| run.text.as_str()).collect())
                }
            }
            ObjectKind::Shape {
                text: Some(text), ..
            } => {
                if text.runs.is_empty() {
                    Some(text.text.clone())
                } else {
                    Some(text.runs.iter().map(|run| run.text.as_str()).collect())
                }
            }
            _ => None,
        };
    }
    if field == "label" {
        return match &object.kind {
            ObjectKind::Button { label, .. } | ObjectKind::Checkbox { label, .. } => {
                Some(label.clone())
            }
            _ => None,
        };
    }
    if field == "placeholder" {
        return match &object.kind {
            ObjectKind::TextEntry { placeholder, .. } => Some(placeholder.clone()),
            _ => None,
        };
    }
    if let Some(rest) = field
        .strip_prefix("options/")
        .and_then(|rest| rest.strip_suffix("/label"))
    {
        let index: usize = rest.parse().ok()?;
        let ObjectKind::RadioGroup { options, .. } = &object.kind else {
            return None;
        };
        return options.get(index).map(|option| option.label.clone());
    }
    if field == "alt" {
        return match &object.kind {
            ObjectKind::Image { alt, .. } => Some(alt.clone()),
            _ => None,
        };
    }
    if field == "description" {
        return object.description.clone();
    }
    if let Some(rest) = field.strip_prefix("state/")
        && let Some(name) = rest.strip_suffix("/text")
    {
        return object
            .states
            .iter()
            .find(|state| state.name == name)
            .and_then(|state| state.text.clone());
    }
    None
}

fn set_text(course: &mut Course, id: &str, target: &ParsedText) -> bool {
    if id == "course/title" {
        course.title = target.text();
        return true;
    }
    if let Some(key) = id.strip_prefix("chrome/") {
        let Some((_, default)) = CHROME_STRINGS.iter().find(|(k, _)| *k == key) else {
            return false;
        };
        let text = target.text();
        if text == *default {
            course.strings.remove(key);
        } else {
            course.strings.insert(key.to_owned(), text);
        }
        return true;
    }
    if let Some(rest) = id.strip_prefix("scene/")
        && let Some(scene_id) = rest.strip_suffix("/title")
    {
        let Some(scene) = course.scenes.iter_mut().find(|scene| scene.id == scene_id) else {
            return false;
        };
        scene.title = target.text();
        return true;
    }
    if let Some(rest) = id.strip_prefix("master/") {
        let Some((master_id, field)) = rest.split_once('/') else {
            return false;
        };
        let Some(master) = course
            .masters
            .iter_mut()
            .find(|master| master.id == master_id)
        else {
            return false;
        };
        return set_slide_text(master, field, target);
    }
    let Some(rest) = id.strip_prefix("slide/") else {
        return false;
    };
    let Some((slide_id, field)) = rest.split_once('/') else {
        return false;
    };
    let Some(slide) = course.slide_mut(slide_id) else {
        return false;
    };
    set_slide_text(slide, field, target)
}

fn set_slide_text(slide: &mut coursecraft_course::Slide, field: &str, target: &ParsedText) -> bool {
    if field == "title" {
        slide.title = target.text();
        return true;
    }
    if field == "notes" {
        slide.notes = target.text();
        return true;
    }
    if let Some(object_field) = field.strip_prefix("object/") {
        let Some((object_id, field)) = object_field.split_once('/') else {
            return false;
        };
        let Some(object) = object_mut(slide, object_id) else {
            return false;
        };
        return set_object_text(object, field, target);
    }
    let Some(question) = slide.question.as_mut() else {
        return false;
    };
    if field == "question/prompt" {
        question.prompt = target.text();
        return true;
    }
    if let Some(choice_id) = field.strip_prefix("question/choice/") {
        let Some(choice) = question
            .choices
            .iter_mut()
            .find(|choice| choice.id == choice_id)
        else {
            return false;
        };
        choice.text = target.text();
        return true;
    }
    if let Some(index) = field.strip_prefix("question/accepted/") {
        let Ok(index) = index.parse::<usize>() else {
            return false;
        };
        let Some(answer) = question.accepted.get_mut(index) else {
            return false;
        };
        *answer = target.text();
        return true;
    }
    let Some(pair_field) = field.strip_prefix("question/pair/") else {
        return false;
    };
    let Some((pair_id, side)) = pair_field.split_once('/') else {
        return false;
    };
    let Some(pair) = question.pairs.iter_mut().find(|pair| pair.id == pair_id) else {
        return false;
    };
    match side {
        "left" => pair.left = target.text(),
        "right" => pair.right = target.text(),
        _ => return false,
    }
    true
}

fn object_mut<'a>(slide: &'a mut coursecraft_course::Slide, id: &str) -> Option<&'a mut Object> {
    if let Some(index) = slide.objects.iter().position(|object| object.id == id) {
        return slide.objects.get_mut(index);
    }
    for layer in &mut slide.layers {
        if let Some(index) = layer.objects.iter().position(|object| object.id == id) {
            return layer.objects.get_mut(index);
        }
    }
    None
}

fn set_object_text(object: &mut Object, field: &str, target: &ParsedText) -> bool {
    if let Some(rest) = field
        .strip_prefix("table/")
        .and_then(|rest| rest.strip_suffix("/text"))
    {
        let Some((row, column)) = rest.split_once('/') else {
            return false;
        };
        let (Ok(row), Ok(column)) = (row.parse::<usize>(), column.parse::<usize>()) else {
            return false;
        };
        let ObjectKind::Table { rows, .. } = &mut object.kind else {
            return false;
        };
        let Some(cell) = rows
            .get_mut(row)
            .and_then(|line| line.cells.get_mut(column))
        else {
            return false;
        };
        let has_runs = !cell.runs.is_empty();
        let target_has_groups = target.segments.iter().any(|segment| segment.run.is_some());
        let formatting = cell.runs.clone();
        let old_text = cell.text.clone();
        let new_text = target.text();
        cell.paragraphs = align_paragraphs(&old_text, &cell.paragraphs, &new_text);
        cell.text = new_text;
        if target_has_groups || has_runs {
            let mut new_runs = Vec::with_capacity(target.segments.len());
            for segment in &target.segments {
                let mut run = segment
                    .run
                    .and_then(|index| formatting.get(index))
                    .cloned()
                    .unwrap_or_else(|| TextRun {
                        text: String::new(),
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
                run.text = segment.text.clone();
                new_runs.push(run);
            }
            cell.runs = new_runs;
        }
        return true;
    }
    if field == "text" {
        let (text, runs, paragraphs) = match &mut object.kind {
            ObjectKind::Text {
                text,
                runs,
                paragraphs,
                ..
            } => (text, runs, paragraphs),
            ObjectKind::Shape {
                text: Some(text), ..
            } => (&mut text.text, &mut text.runs, &mut text.paragraphs),
            _ => return false,
        };
        let has_runs = !runs.is_empty();
        let target_has_groups = target.segments.iter().any(|segment| segment.run.is_some());
        let formatting: Vec<TextRun> = runs.clone();
        let old_text = text.clone();
        let new_text = target.text();
        let aligned = align_paragraphs(&old_text, paragraphs, &new_text);
        *paragraphs = aligned;
        *text = new_text;
        if target_has_groups || has_runs {
            let mut new_runs = Vec::with_capacity(target.segments.len());
            for segment in &target.segments {
                let mut run = segment
                    .run
                    .and_then(|index| formatting.get(index))
                    .cloned()
                    .unwrap_or_else(|| TextRun {
                        text: String::new(),
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
                run.text = segment.text.clone();
                new_runs.push(run);
            }
            *runs = new_runs;
        }
        return true;
    }
    if field == "title" {
        if let ObjectKind::Marker { title, .. } = &mut object.kind {
            *title = target.text();
            return true;
        }
        return false;
    }
    if field == "body" {
        let ObjectKind::Marker {
            body,
            runs,
            paragraphs,
            ..
        } = &mut object.kind
        else {
            return false;
        };
        let has_runs = !runs.is_empty();
        let target_has_groups = target.segments.iter().any(|segment| segment.run.is_some());
        let formatting: Vec<TextRun> = runs.clone();
        let old_text = body.clone();
        let new_text = target.text();
        let aligned = align_paragraphs(&old_text, paragraphs, &new_text);
        *paragraphs = aligned;
        *body = new_text;
        if target_has_groups || has_runs {
            let mut new_runs = Vec::with_capacity(target.segments.len());
            for segment in &target.segments {
                let mut run = segment
                    .run
                    .and_then(|index| formatting.get(index))
                    .cloned()
                    .unwrap_or_else(|| TextRun {
                        text: String::new(),
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
                run.text = segment.text.clone();
                new_runs.push(run);
            }
            *runs = new_runs;
        }
        return true;
    }
    if field == "label" {
        if let ObjectKind::Button { label, .. } | ObjectKind::Checkbox { label, .. } =
            &mut object.kind
        {
            *label = target.text();
            return true;
        }
        return false;
    }
    if field == "placeholder" {
        if let ObjectKind::TextEntry { placeholder, .. } = &mut object.kind {
            *placeholder = target.text();
            return true;
        }
        return false;
    }
    if let Some(rest) = field
        .strip_prefix("options/")
        .and_then(|rest| rest.strip_suffix("/label"))
    {
        let Some(index) = rest.parse::<usize>().ok() else {
            return false;
        };
        let ObjectKind::RadioGroup { options, .. } = &mut object.kind else {
            return false;
        };
        let Some(option) = options.get_mut(index) else {
            return false;
        };
        option.label = target.text();
        return true;
    }
    if field == "alt" {
        if let ObjectKind::Image { alt, .. } = &mut object.kind {
            *alt = target.text();
            return true;
        }
        return false;
    }
    if field == "description" {
        if object.description.is_some() {
            object.description = Some(target.text());
            return true;
        }
        return false;
    }
    if let Some(rest) = field.strip_prefix("state/")
        && let Some(name) = rest.strip_suffix("/text")
        && let Some(state) = object.states.iter_mut().find(|state| state.name == name)
    {
        if state.text.is_some() {
            state.text = Some(target.text());
            return true;
        }
        return false;
    }
    false
}

fn placeholders_match(source: &str, target: &str, braces: bool) -> bool {
    if percent_placeholders(source) != percent_placeholders(target) {
        return false;
    }
    if braces && brace_placeholders(source) != brace_placeholders(target) {
        return false;
    }
    true
}

fn percent_placeholders(text: &str) -> std::collections::BTreeSet<String> {
    let mut set = std::collections::BTreeSet::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes.get(i) != Some(&b'%') {
            i = i.saturating_add(1);
            continue;
        }
        if bytes.get(i.saturating_add(1)) == Some(&b'%') {
            i = i.saturating_add(2);
            continue;
        }
        let mut j = i.saturating_add(1);
        let mut end = None;
        while j < bytes.len() {
            if bytes.get(j) == Some(&b'%') {
                end = Some(j);
                break;
            }
            j = j.saturating_add(1);
        }
        let Some(end) = end else { break };
        if let Some(name) = text.get(i.saturating_add(1)..end)
            && !name.is_empty()
            && name.len() <= 128
        {
            set.insert(name.to_owned());
        }
        i = end.saturating_add(1);
    }
    set
}

fn brace_placeholders(text: &str) -> std::collections::BTreeSet<String> {
    let mut set = std::collections::BTreeSet::new();
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        let Some(after) = rest.get(open.saturating_add(1)..) else {
            break;
        };
        let Some(close) = after.find('}') else {
            break;
        };
        if let Some(name) = after.get(..close)
            && !name.is_empty()
        {
            set.insert(name.to_owned());
        }
        rest = after.get(close.saturating_add(1)..).unwrap_or_default();
    }
    set
}
