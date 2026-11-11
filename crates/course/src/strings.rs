//! Every author-visible string in a course, in play order, then masters and
//! bank slides: the walker behind find and replace. It also holds the matcher
//! and the pure replacement helpers, so the engine and the UI share one
//! definition.
//!
//! The walker covers what the XLIFF export covers, plus the layer and question
//! strings of masters and bank slides. Templates are not included. Chrome strings are not
//! course content (they are the player's own text), so they are not included.
//!
//! Matching is by simple lowercase folding. `ß` is not folded to `ss`, and a
//! query that matches only part of a character that folds to several (such as
//! `i` inside `İ`) is not a match.

use std::ops::Range;

use serde::{Deserialize, Serialize};

use crate::{Course, Object, ObjectKind, Question, Slide, TextRun};

/// Most hits [`find_in_course`] returns before it reports `truncated`.
pub const MAX_HITS: usize = 1000;
/// Characters of context shown on each side of a hit.
const SNIPPET_CONTEXT: usize = 30;
/// Longest variable name the runtime accepts inside `%name%`.
const MAX_VARIABLE_NAME: usize = 128;

/// Where a string lives. `slide` is `None` for course and scene titles.
/// `field` names the string inside that place, for example `text`, `label`,
/// `question/choice/<id>` or `scene/<id>/title`. Serialises so a hit can be
/// passed back as an `only` entry.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Location {
    #[serde(default)]
    pub slide: Option<String>,
    #[serde(default)]
    pub object: Option<String>,
    #[serde(default)]
    pub layer: Option<String>,
    pub field: String,
}

/// One author-visible string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StringItem {
    pub location: Location,
    /// Human label such as `Slide 3 ▸ Title`.
    pub label: String,
    /// The text as the author sees it: the joined runs when there are runs.
    pub text: String,
}

/// One match returned by [`find_in_course`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hit {
    pub slide: Option<String>,
    pub object: Option<String>,
    pub layer: Option<String>,
    pub field: String,
    pub label: String,
    pub snippet: String,
    /// Byte range of the match inside `snippet`.
    pub snippet_match: Range<usize>,
    /// Byte range of the match inside the string.
    pub start: usize,
    pub end: usize,
}

/// A mutable view of one string, for replacement.
pub enum StringTarget<'a> {
    /// A plain string.
    Plain(&'a mut String),
    /// An optional string, such as a description or a state's text.
    Optional(&'a mut Option<String>),
    /// A text object with runs. `text` is the joined runs.
    Runs {
        text: &'a mut String,
        runs: &'a mut Vec<TextRun>,
    },
}

/// What one replacement did to one string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ReplaceOutcome {
    /// Matches replaced.
    pub replaced: usize,
    /// Matches left alone because they overlap a `%name%` reference.
    pub skipped: usize,
    /// True when the result would be longer than `max_bytes`. Nothing changed.
    pub too_long: bool,
}

// ---------------------------------------------------------------- walker

/// Every author-visible string, in play order, then masters and bank slides.
#[must_use]
pub fn course_strings(course: &Course) -> Vec<StringItem> {
    let mut out = Vec::new();
    add(
        &mut out,
        Location {
            slide: None,
            object: None,
            layer: None,
            field: "course/title".to_owned(),
        },
        "Course ▸ Title".to_owned(),
        &course.title,
    );
    let mut number = 0usize;
    for (scene_index, scene) in course.scenes.iter().enumerate() {
        add(
            &mut out,
            Location {
                slide: None,
                object: None,
                layer: None,
                field: format!("scene/{}/title", scene.id),
            },
            format!("Scene {} ▸ Title", scene_index.saturating_add(1)),
            &scene.title,
        );
        for slide in &scene.slides {
            number = number.saturating_add(1);
            walk_slide(&mut out, slide, &format!("Slide {number}"));
        }
    }
    for master in &course.masters {
        let name = if master.title.is_empty() {
            master.id.as_str()
        } else {
            master.title.as_str()
        };
        walk_slide(&mut out, master, &format!("Master \"{name}\""));
    }
    for bank in &course.banks {
        let name = if bank.title.is_empty() {
            bank.id.as_str()
        } else {
            bank.title.as_str()
        };
        for slide in &bank.slides {
            walk_slide(&mut out, slide, &format!("Bank \"{name}\""));
        }
    }
    out
}

fn slide_location(
    slide: &str,
    object: Option<&str>,
    layer: Option<&str>,
    field: String,
) -> Location {
    Location {
        slide: Some(slide.to_owned()),
        object: object.map(str::to_owned),
        layer: layer.map(str::to_owned),
        field,
    }
}

fn add(out: &mut Vec<StringItem>, location: Location, label: String, text: &str) {
    if text.is_empty() {
        return;
    }
    out.push(StringItem {
        location,
        label,
        text: text.to_owned(),
    });
}

fn walk_slide(out: &mut Vec<StringItem>, slide: &Slide, place: &str) {
    let sid = slide.id.as_str();
    add(
        out,
        slide_location(sid, None, None, "title".to_owned()),
        format!("{place} ▸ Title"),
        &slide.title,
    );
    add(
        out,
        slide_location(sid, None, None, "notes".to_owned()),
        format!("{place} ▸ Notes"),
        &slide.notes,
    );
    for object in &slide.objects {
        walk_object(out, sid, place, None, object);
    }
    for layer in &slide.layers {
        let name = if layer.name.is_empty() {
            layer.id.as_str()
        } else {
            layer.name.as_str()
        };
        let layer_place = format!("{place} ▸ Layer \"{name}\"");
        for object in &layer.objects {
            walk_object(out, sid, &layer_place, Some(layer.id.as_str()), object);
        }
    }
    if let Some(question) = &slide.question {
        walk_question(out, sid, place, question);
    }
}

fn walk_object(
    out: &mut Vec<StringItem>,
    slide: &str,
    place: &str,
    layer: Option<&str>,
    object: &Object,
) {
    let name = if object.name.is_empty() {
        object.id.as_str()
    } else {
        object.name.as_str()
    };
    let object_place = format!("{place} ▸ {name}");
    let at = |field: String| slide_location(slide, Some(object.id.as_str()), layer, field);
    match &object.kind {
        ObjectKind::Text { text, runs, .. } => {
            if runs.is_empty() {
                add(
                    out,
                    at("text".to_owned()),
                    format!("{object_place} ▸ Text"),
                    text,
                );
            } else {
                let joined: String = runs.iter().map(|run| run.text.as_str()).collect();
                add(
                    out,
                    at("text".to_owned()),
                    format!("{object_place} ▸ Text"),
                    &joined,
                );
            }
        }
        ObjectKind::Button { label, .. } => add(
            out,
            at("label".to_owned()),
            format!("{object_place} ▸ Button label"),
            label,
        ),
        ObjectKind::Image { alt, .. } => add(
            out,
            at("alt".to_owned()),
            format!("{object_place} ▸ Alt text"),
            alt,
        ),
        ObjectKind::Shape {
            text: Some(text), ..
        } => {
            if text.runs.is_empty() {
                add(
                    out,
                    at("text".to_owned()),
                    format!("{object_place} ▸ Shape text"),
                    &text.text,
                );
            } else {
                let joined: String = text.runs.iter().map(|run| run.text.as_str()).collect();
                add(
                    out,
                    at("text".to_owned()),
                    format!("{object_place} ▸ Shape text"),
                    &joined,
                );
            }
        }
        ObjectKind::Table { rows, .. } => {
            for (row_index, row) in rows.iter().enumerate() {
                for (column, cell) in row.cells.iter().enumerate() {
                    let text = if cell.runs.is_empty() {
                        cell.text.clone()
                    } else {
                        cell.runs.iter().map(|run| run.text.as_str()).collect()
                    };
                    add(
                        out,
                        at(format!("table/{row_index}/{column}/text")),
                        format!(
                            "{object_place} ▸ Table cell {},{}",
                            row_index.saturating_add(1),
                            column.saturating_add(1)
                        ),
                        &text,
                    );
                }
            }
        }
        ObjectKind::TextEntry { placeholder, .. } => add(
            out,
            at("placeholder".to_owned()),
            format!("{object_place} ▸ Placeholder"),
            placeholder,
        ),
        ObjectKind::Checkbox { label, .. } => add(
            out,
            at("label".to_owned()),
            format!("{object_place} ▸ Checkbox label"),
            label,
        ),
        ObjectKind::RadioGroup { options, .. } => {
            for (index, option) in options.iter().enumerate() {
                add(
                    out,
                    at(format!("options/{index}/label")),
                    format!("{object_place} ▸ Radio option {}", index.saturating_add(1)),
                    &option.label,
                );
            }
        }
        ObjectKind::Marker {
            title, body, runs, ..
        } => {
            add(
                out,
                at("title".to_owned()),
                format!("{object_place} ▸ Marker title"),
                title,
            );
            if runs.is_empty() {
                add(
                    out,
                    at("body".to_owned()),
                    format!("{object_place} ▸ Marker body"),
                    body,
                );
            } else {
                let joined: String = runs.iter().map(|run| run.text.as_str()).collect();
                add(
                    out,
                    at("body".to_owned()),
                    format!("{object_place} ▸ Marker body"),
                    &joined,
                );
            }
        }
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
    if let Some(description) = &object.description {
        add(
            out,
            at("description".to_owned()),
            format!("{object_place} ▸ Description"),
            description,
        );
    }
    for state in &object.states {
        if let Some(text) = &state.text {
            add(
                out,
                at(format!("state/{}/text", state.name)),
                format!("{object_place} ▸ State \"{}\" text", state.name),
                text,
            );
        }
    }
}

fn walk_question(out: &mut Vec<StringItem>, slide: &str, place: &str, question: &Question) {
    let at = |field: String| slide_location(slide, None, None, field);
    add(
        out,
        at("question/prompt".to_owned()),
        format!("{place} ▸ Question prompt"),
        &question.prompt,
    );
    for (index, choice) in question.choices.iter().enumerate() {
        add(
            out,
            at(format!("question/choice/{}", choice.id)),
            format!("{place} ▸ Choice {}", index.saturating_add(1)),
            &choice.text,
        );
    }
    for (index, answer) in question.accepted.iter().enumerate() {
        add(
            out,
            at(format!("question/accepted/{index}")),
            format!("{place} ▸ Accepted answer {}", index.saturating_add(1)),
            answer,
        );
    }
    for pair in &question.pairs {
        add(
            out,
            at(format!("question/pair/{}/left", pair.id)),
            format!("{place} ▸ Pair {} left", pair.id),
            &pair.left,
        );
        add(
            out,
            at(format!("question/pair/{}/right", pair.id)),
            format!("{place} ▸ Pair {} right", pair.id),
            &pair.right,
        );
    }
}

// ---------------------------------------------------------------- lookup

/// A mutable view of the string at `location`, or `None` when nothing there
/// matches. Every location [`course_strings`] returns resolves here.
#[must_use]
pub fn string_mut<'a>(course: &'a mut Course, location: &Location) -> Option<StringTarget<'a>> {
    let field = location.field.as_str();
    match &location.slide {
        None => {
            if field == "course/title" {
                return Some(StringTarget::Plain(&mut course.title));
            }
            let scene_id = field.strip_prefix("scene/")?.strip_suffix("/title")?;
            let scene = course
                .scenes
                .iter_mut()
                .find(|scene| scene.id == scene_id)?;
            Some(StringTarget::Plain(&mut scene.title))
        }
        Some(slide_id) => {
            let slide = course.slide_mut(slide_id)?;
            slide_target(slide, location)
        }
    }
}

fn slide_target<'a>(slide: &'a mut Slide, location: &Location) -> Option<StringTarget<'a>> {
    let field = location.field.as_str();
    if let Some(object_id) = location.object.as_deref() {
        let object = object_mut(slide, location.layer.as_deref(), object_id)?;
        return object_target(object, field);
    }
    match field {
        "title" => Some(StringTarget::Plain(&mut slide.title)),
        "notes" => Some(StringTarget::Plain(&mut slide.notes)),
        _ => question_target(slide.question.as_mut()?, field),
    }
}

fn object_mut<'a>(slide: &'a mut Slide, layer: Option<&str>, id: &str) -> Option<&'a mut Object> {
    match layer {
        None => slide.objects.iter_mut().find(|object| object.id == id),
        Some(layer_id) => slide
            .layers
            .iter_mut()
            .find(|layer| layer.id == layer_id)?
            .objects
            .iter_mut()
            .find(|object| object.id == id),
    }
}

fn object_target<'a>(object: &'a mut Object, field: &str) -> Option<StringTarget<'a>> {
    if let Some(name) = field
        .strip_prefix("state/")
        .and_then(|rest| rest.strip_suffix("/text"))
    {
        let state = object.states.iter_mut().find(|state| state.name == name)?;
        return Some(StringTarget::Optional(&mut state.text));
    }
    if let Some(rest) = field
        .strip_prefix("table/")
        .and_then(|rest| rest.strip_suffix("/text"))
    {
        let (row, column) = rest.split_once('/')?;
        let row: usize = row.parse().ok()?;
        let column: usize = column.parse().ok()?;
        let ObjectKind::Table { rows, .. } = &mut object.kind else {
            return None;
        };
        let cell = rows.get_mut(row)?.cells.get_mut(column)?;
        return if cell.runs.is_empty() {
            Some(StringTarget::Plain(&mut cell.text))
        } else {
            Some(StringTarget::Runs {
                text: &mut cell.text,
                runs: &mut cell.runs,
            })
        };
    }
    if let Some(rest) = field
        .strip_prefix("options/")
        .and_then(|rest| rest.strip_suffix("/label"))
    {
        let index: usize = rest.parse().ok()?;
        let ObjectKind::RadioGroup { options, .. } = &mut object.kind else {
            return None;
        };
        return Some(StringTarget::Plain(&mut options.get_mut(index)?.label));
    }
    match field {
        "description" => Some(StringTarget::Optional(&mut object.description)),
        "title" => match &mut object.kind {
            ObjectKind::Marker { title, .. } => Some(StringTarget::Plain(title)),
            _ => None,
        },
        "body" => match &mut object.kind {
            ObjectKind::Marker { body, runs, .. } => {
                if runs.is_empty() {
                    Some(StringTarget::Plain(body))
                } else {
                    Some(StringTarget::Runs { text: body, runs })
                }
            }
            _ => None,
        },
        "text" => match &mut object.kind {
            ObjectKind::Text { text, runs, .. } => {
                if runs.is_empty() {
                    Some(StringTarget::Plain(text))
                } else {
                    Some(StringTarget::Runs { text, runs })
                }
            }
            ObjectKind::Shape {
                text: Some(text), ..
            } => {
                if text.runs.is_empty() {
                    Some(StringTarget::Plain(&mut text.text))
                } else {
                    Some(StringTarget::Runs {
                        text: &mut text.text,
                        runs: &mut text.runs,
                    })
                }
            }
            _ => None,
        },
        "label" => match &mut object.kind {
            ObjectKind::Button { label, .. } | ObjectKind::Checkbox { label, .. } => {
                Some(StringTarget::Plain(label))
            }
            _ => None,
        },
        "placeholder" => match &mut object.kind {
            ObjectKind::TextEntry { placeholder, .. } => Some(StringTarget::Plain(placeholder)),
            _ => None,
        },
        "alt" => match &mut object.kind {
            ObjectKind::Image { alt, .. } => Some(StringTarget::Plain(alt)),
            _ => None,
        },
        _ => None,
    }
}

fn question_target<'a>(question: &'a mut Question, field: &str) -> Option<StringTarget<'a>> {
    if field == "question/prompt" {
        return Some(StringTarget::Plain(&mut question.prompt));
    }
    if let Some(choice_id) = field.strip_prefix("question/choice/") {
        let choice = question
            .choices
            .iter_mut()
            .find(|choice| choice.id == choice_id)?;
        return Some(StringTarget::Plain(&mut choice.text));
    }
    if let Some(index) = field.strip_prefix("question/accepted/") {
        let index: usize = index.parse().ok()?;
        return Some(StringTarget::Plain(question.accepted.get_mut(index)?));
    }
    if let Some(rest) = field.strip_prefix("question/pair/") {
        let (pair_id, side) = rest.rsplit_once('/')?;
        let pair = question.pairs.iter_mut().find(|pair| pair.id == pair_id)?;
        return match side {
            "left" => Some(StringTarget::Plain(&mut pair.left)),
            "right" => Some(StringTarget::Plain(&mut pair.right)),
            _ => None,
        };
    }
    None
}

// ---------------------------------------------------------------- search

/// Find every match of `query` in `course`, in walk order. Returns at most
/// [`MAX_HITS`] hits and whether there were more.
#[must_use]
pub fn find_in_course(
    course: &Course,
    query: &str,
    case_sensitive: bool,
    whole_word: bool,
) -> (Vec<Hit>, bool) {
    let mut hits = Vec::new();
    if query.is_empty() {
        return (hits, false);
    }
    for item in course_strings(course) {
        for range in find_matches(&item.text, query, case_sensitive, whole_word) {
            let (snippet, snippet_match) = snippet(&item.text, range.clone());
            hits.push(Hit {
                slide: item.location.slide.clone(),
                object: item.location.object.clone(),
                layer: item.location.layer.clone(),
                field: item.location.field.clone(),
                label: item.label.clone(),
                snippet,
                snippet_match,
                start: range.start,
                end: range.end,
            });
            if hits.len() > MAX_HITS {
                hits.truncate(MAX_HITS);
                return (hits, true);
            }
        }
    }
    (hits, false)
}

/// Byte ranges of the non-overlapping matches of `query` in `text`, left to
/// right. Every range starts and ends on a character boundary of `text`. An
/// empty query matches nothing. With `whole_word`, a match whose neighbour
/// is alphanumeric or `_` is skipped.
#[must_use]
pub fn find_matches(
    text: &str,
    query: &str,
    case_sensitive: bool,
    whole_word: bool,
) -> Vec<Range<usize>> {
    let mut found = Vec::new();
    if query.is_empty() || text.is_empty() {
        return found;
    }
    let (folded_text, map) = fold(text, case_sensitive);
    let (folded_query, _) = fold(query, case_sensitive);
    if folded_query.is_empty() {
        return found;
    }
    let mut from = 0usize;
    while let Some(rest) = folded_text.get(from..) {
        let Some(relative) = rest.find(folded_query.as_str()) else {
            break;
        };
        let folded_start = from.saturating_add(relative);
        let folded_end = folded_start.saturating_add(folded_query.len());
        let step = folded_text
            .get(folded_start..)
            .and_then(|tail| tail.chars().next())
            .map_or(1, char::len_utf8);
        let next_from = folded_start.saturating_add(step);
        let start = map.get(folded_start).copied().flatten();
        let end = map.get(folded_end).copied().flatten();
        let (Some(start), Some(end)) = (start, end) else {
            from = next_from;
            continue;
        };
        if whole_word && (word_before(text, start) || word_after(text, end)) {
            from = next_from;
            continue;
        }
        found.push(start..end);
        from = folded_end;
    }
    found
}

/// Lowercase (or keep, when case-sensitive) `text`, and map each byte of the
/// result to the original byte offset where a character starts. Positions in
/// the middle of one character's folded form map to `None`. The last entry
/// maps the end of the text.
fn fold(text: &str, case_sensitive: bool) -> (String, Vec<Option<usize>>) {
    let mut folded = String::with_capacity(text.len());
    let mut map: Vec<Option<usize>> = Vec::with_capacity(text.len().saturating_add(1));
    for (offset, ch) in text.char_indices() {
        let start = folded.len();
        if case_sensitive {
            folded.push(ch);
        } else {
            folded.extend(ch.to_lowercase());
        }
        map.resize(folded.len(), None);
        if let Some(slot) = map.get_mut(start) {
            *slot = Some(offset);
        }
    }
    map.push(Some(text.len()));
    (folded, map)
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

fn word_before(text: &str, position: usize) -> bool {
    text.get(..position)
        .and_then(|head| head.chars().next_back())
        .is_some_and(is_word_char)
}

fn word_after(text: &str, position: usize) -> bool {
    text.get(position..)
        .and_then(|tail| tail.chars().next())
        .is_some_and(is_word_char)
}

/// Up to 30 characters either side of `range`, with `…` where cut, and the
/// byte range of the match inside the returned snippet.
#[must_use]
pub fn snippet(text: &str, range: Range<usize>) -> (String, Range<usize>) {
    let before = text.get(..range.start).unwrap_or("");
    let matched = text.get(range.clone()).unwrap_or("");
    let after = text.get(range.end..).unwrap_or("");
    let mut out = String::new();
    let before_len = before.chars().count();
    if before_len > SNIPPET_CONTEXT {
        out.push('…');
    }
    out.extend(
        before
            .chars()
            .skip(before_len.saturating_sub(SNIPPET_CONTEXT)),
    );
    let match_start = out.len();
    out.push_str(matched);
    let match_end = out.len();
    out.extend(after.chars().take(SNIPPET_CONTEXT));
    if after.chars().count() > SNIPPET_CONTEXT {
        out.push('…');
    }
    (out, match_start..match_end)
}

/// Byte ranges of the `%name%` references in `text`, including both `%`
/// signs. The rules match the XLIFF placeholder check: `%%` is an escape, and
/// a name is non-empty and at most 128 bytes.
#[must_use]
pub fn variable_reference_spans(text: &str) -> Vec<Range<usize>> {
    let bytes = text.as_bytes();
    let mut spans = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes.get(i) != Some(&b'%') {
            i = i.saturating_add(1);
            continue;
        }
        if bytes.get(i.saturating_add(1)) == Some(&b'%') {
            i = i.saturating_add(2);
            continue;
        }
        let Some(close) = bytes
            .iter()
            .enumerate()
            .skip(i.saturating_add(1))
            .find(|(_, byte)| **byte == b'%')
            .map(|(index, _)| index)
        else {
            break;
        };
        let name_len = close.saturating_sub(i).saturating_sub(1);
        if name_len > 0 && name_len <= MAX_VARIABLE_NAME {
            spans.push(i..close.saturating_add(1));
        }
        i = close.saturating_add(1);
    }
    spans
}

// ---------------------------------------------------------------- replace

/// Replace the matches of `query` in one string with `replacement`. Matches
/// that overlap a `%name%` reference are left alone and counted in `skipped`.
/// Nothing changes when there is nothing to replace, or when the result would
/// be longer than `max_bytes` and longer than it is now.
pub fn replace_in_target(
    target: StringTarget<'_>,
    query: &str,
    replacement: &str,
    case_sensitive: bool,
    whole_word: bool,
    max_bytes: usize,
) -> ReplaceOutcome {
    match target {
        StringTarget::Plain(text) => replace_text(
            text,
            query,
            replacement,
            case_sensitive,
            whole_word,
            max_bytes,
        ),
        StringTarget::Optional(slot) => match slot.as_mut() {
            Some(text) => replace_text(
                text,
                query,
                replacement,
                case_sensitive,
                whole_word,
                max_bytes,
            ),
            None => ReplaceOutcome::default(),
        },
        StringTarget::Runs { text, runs } => {
            let plain: String = runs.iter().map(|run| run.text.as_str()).collect();
            let (matches, skipped) = accepted_matches(&plain, query, case_sensitive, whole_word);
            if matches.is_empty() {
                return ReplaceOutcome {
                    replaced: 0,
                    skipped,
                    too_long: false,
                };
            }
            if grows_past(&plain, &matches, replacement, max_bytes) {
                return ReplaceOutcome {
                    replaced: 0,
                    skipped,
                    too_long: true,
                };
            }
            let new_text = splice(&plain, &matches, replacement);
            let new_runs = replace_runs(runs, &matches, replacement);
            *runs = if all_plain(&new_runs) {
                Vec::new()
            } else {
                new_runs
            };
            *text = new_text;
            ReplaceOutcome {
                replaced: matches.len(),
                skipped,
                too_long: false,
            }
        }
    }
}

fn replace_text(
    text: &mut String,
    query: &str,
    replacement: &str,
    case_sensitive: bool,
    whole_word: bool,
    max_bytes: usize,
) -> ReplaceOutcome {
    let (matches, skipped) = accepted_matches(text, query, case_sensitive, whole_word);
    if matches.is_empty() {
        return ReplaceOutcome {
            replaced: 0,
            skipped,
            too_long: false,
        };
    }
    if grows_past(text, &matches, replacement, max_bytes) {
        return ReplaceOutcome {
            replaced: 0,
            skipped,
            too_long: true,
        };
    }
    *text = splice(text, &matches, replacement);
    ReplaceOutcome {
        replaced: matches.len(),
        skipped,
        too_long: false,
    }
}

/// The matches that may be replaced, and how many were skipped because they
/// overlap a `%name%` reference.
fn accepted_matches(
    text: &str,
    query: &str,
    case_sensitive: bool,
    whole_word: bool,
) -> (Vec<Range<usize>>, usize) {
    let spans = variable_reference_spans(text);
    let mut accepted = Vec::new();
    let mut skipped = 0usize;
    for found in find_matches(text, query, case_sensitive, whole_word) {
        if spans
            .iter()
            .any(|span| found.start < span.end && span.start < found.end)
        {
            skipped = skipped.saturating_add(1);
        } else {
            accepted.push(found);
        }
    }
    (accepted, skipped)
}

/// True when the replaced text would be longer than `max_bytes` and longer
/// than the text it replaces. Checked before anything is built.
fn grows_past(text: &str, matches: &[Range<usize>], replacement: &str, max_bytes: usize) -> bool {
    let removed: usize = matches.iter().map(|range| range.len()).sum();
    let added = matches.len().saturating_mul(replacement.len());
    let new_len = text.len().saturating_sub(removed).saturating_add(added);
    new_len > max_bytes && new_len > text.len()
}

/// `text` with each range replaced. Ranges must be sorted and non-overlapping.
fn splice(text: &str, ranges: &[Range<usize>], replacement: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last = 0usize;
    for range in ranges {
        out.push_str(text.get(last..range.start).unwrap_or(""));
        out.push_str(replacement);
        last = range.end;
    }
    out.push_str(text.get(last..).unwrap_or(""));
    out
}

/// Replace inside runs. Each match goes into the run where it starts, and the
/// matched text is removed from the runs it covers. Runs left empty are
/// dropped, and neighbours with the same formatting are merged.
fn replace_runs(runs: &[TextRun], ranges: &[Range<usize>], replacement: &str) -> Vec<TextRun> {
    let mut out = Vec::with_capacity(runs.len());
    let mut offset = 0usize;
    let mut next = 0usize;
    let mut skip_until = 0usize;
    for run in runs {
        let mut kept = String::new();
        for (index, ch) in run.text.char_indices() {
            let position = offset.saturating_add(index);
            if let Some(range) = ranges.get(next)
                && range.start == position
            {
                kept.push_str(replacement);
                skip_until = range.end;
                next = next.saturating_add(1);
            }
            if position >= skip_until {
                kept.push(ch);
            }
        }
        offset = offset.saturating_add(run.text.len());
        if !kept.is_empty() {
            out.push(TextRun {
                text: kept,
                ..run.clone()
            });
        }
    }
    crate::normalize_runs(&out)
}

fn all_plain(runs: &[TextRun]) -> bool {
    runs.iter().all(|run| {
        run.bold.is_none()
            && run.italic.is_none()
            && run.underline.is_none()
            && run.size.is_none()
            && run.color.is_none()
            && run.link.is_none()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matches<'a>(text: &'a str, query: &str, case: bool, whole: bool) -> Vec<&'a str> {
        find_matches(text, query, case, whole)
            .into_iter()
            .map(|range| text.get(range).unwrap_or("?"))
            .collect()
    }

    fn assert_boundaries(text: &str, ranges: &[Range<usize>]) {
        for range in ranges {
            assert!(text.is_char_boundary(range.start), "start {range:?}");
            assert!(text.is_char_boundary(range.end), "end {range:?}");
        }
    }

    #[test]
    fn case_sensitive_match_respects_case() {
        assert_eq!(matches("Cat cat CAT", "cat", true, false), vec!["cat"]);
    }

    #[test]
    fn case_insensitive_match_finds_every_case() {
        assert_eq!(
            matches("Cat cat CAT", "cat", false, false),
            vec!["Cat", "cat", "CAT"]
        );
    }

    #[test]
    fn whole_word_skips_partial_words() {
        assert_eq!(
            matches("cat catalog bobcat cat_1 cat.", "cat", true, true),
            vec!["cat", "cat"]
        );
    }

    #[test]
    fn whole_word_treats_underscore_and_digits_as_word_characters() {
        assert!(matches("cat1 _cat", "cat", true, true).is_empty());
    }

    #[test]
    fn empty_query_matches_nothing() {
        assert!(find_matches("anything", "", false, false).is_empty());
        assert!(find_matches("", "", false, false).is_empty());
    }

    #[test]
    fn adjacent_matches_are_all_found_without_overlap() {
        let text = "aaaa";
        assert_eq!(matches(text, "aa", true, false), vec!["aa", "aa"]);
        assert_eq!(matches(text, "aaa", true, false), vec!["aaa"]);
    }

    #[test]
    fn matches_are_left_to_right() {
        let found = find_matches("abcabcabc", "bc", true, false);
        let starts: Vec<usize> = found.iter().map(|r| r.start).collect();
        assert_eq!(starts, vec![1, 4, 7]);
    }

    #[test]
    fn non_ascii_text_matches_on_char_boundaries() {
        let text = "café naïve Ünïcode 日本語 café";
        let found = find_matches(text, "café", true, false);
        assert_eq!(found.len(), 2);
        assert_boundaries(text, &found);
        assert_eq!(matches(text, "CAFÉ", false, false), vec!["café", "café"]);
    }

    #[test]
    fn sharp_s_does_not_panic_and_keeps_boundaries() {
        let text = "Straße straße STRASSE";
        let found = find_matches(text, "straße", false, false);
        assert_eq!(found.len(), 2);
        assert_boundaries(text, &found);
        let found = find_matches(text, "ß", false, false);
        assert_boundaries(text, &found);
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn dotted_capital_i_changes_byte_length_when_folded() {
        let text = "İstanbul İ istanbul";
        // Searching for the full folded form matches the dotted capital.
        let found = find_matches(text, "İ", false, false);
        assert_eq!(found.len(), 2);
        assert_boundaries(text, &found);
        // A plain `i` matches the folded form's first byte only, which is the
        // middle of a folded character, so it is not a match inside `İ`.
        let found = find_matches(text, "i", false, false);
        assert_boundaries(text, &found);
        for range in &found {
            let slice = text.get(range.clone());
            assert_eq!(slice, Some("i"), "matched {range:?}");
        }
    }

    #[test]
    fn combining_marks_and_emoji_keep_boundaries() {
        let text = "e\u{301}clair 🎉 party 🎉 e\u{301}";
        let found = find_matches(text, "🎉", true, false);
        assert_eq!(found.len(), 2);
        assert_boundaries(text, &found);
        let found = find_matches(text, "e\u{301}", true, false);
        assert_eq!(found.len(), 2);
        assert_boundaries(text, &found);
    }

    #[test]
    fn long_query_and_text_do_not_panic() {
        let text = "x".repeat(5000);
        let query = "x".repeat(4096);
        let found = find_matches(&text, &query, false, false);
        assert!(!found.is_empty());
        assert_boundaries(&text, &found);
    }

    #[test]
    fn snippet_cuts_context_with_ellipses() {
        let text = format!("{}needle{}", "a".repeat(40), "b".repeat(40));
        let start = text.find("needle").unwrap_or(0);
        let (snip, range) = snippet(&text, start..start.saturating_add(6));
        assert!(snip.starts_with('…'));
        assert!(snip.ends_with('…'));
        assert_eq!(snip.get(range), Some("needle"));
        assert_eq!(snip.chars().filter(|c| *c == 'a').count(), 30);
        assert_eq!(snip.chars().filter(|c| *c == 'b').count(), 30);
    }

    #[test]
    fn snippet_without_cut_has_no_ellipsis() {
        let (snip, range) = snippet("hi cat there", 3..6);
        assert_eq!(snip, "hi cat there");
        assert_eq!(range, 3..6);
    }

    #[test]
    fn variable_spans_follow_xliff_rules() {
        assert_eq!(variable_reference_spans("Hi %name%!"), vec![3..9]);
        assert!(variable_reference_spans("100%% sure").is_empty());
        assert!(variable_reference_spans("%%name%%").len() <= 1);
        assert!(variable_reference_spans("50% off").is_empty());
        assert!(variable_reference_spans("%%").is_empty());
    }

    #[test]
    fn course_strings_include_bank_slides() {
        let json = serde_json::json!({
            "schema_version": 1,
            "id": "bank-strings",
            "title": "Bank Strings",
            "scenes": [{
                "id": "sc",
                "title": "Scene",
                "slides": [{
                    "id": "s-draw",
                    "title": "Draw",
                    "draw": { "bank": "bank1", "count": 1 }
                }]
            }],
            "banks": [{
                "id": "bank1",
                "title": "Bank One",
                "slides": [{
                    "id": "bq1",
                    "title": "Bank question title",
                    "question": {
                        "kind": "multiple_choice",
                        "prompt": "Bank prompt text",
                        "choices": [
                            { "id": "a", "text": "Choice A" },
                            { "id": "b", "text": "Choice B", "correct": true }
                        ],
                        "attempts": 1
                    },
                    "triggers": [{
                        "action": { "action": "submit_question" },
                        "when": { "event": "submit" }
                    }]
                }]
            }]
        });
        let course = crate::Course::from_json(&json.to_string()).expect("valid course");
        let items = course_strings(&course);
        assert!(
            items.iter().any(|item| item.text == "Bank question title"
                && item.location.slide.as_deref() == Some("bq1")),
            "bank slide title is walked: {items:?}"
        );
        assert!(
            items.iter().any(|item| item.text == "Bank prompt text"),
            "bank question prompt is walked"
        );
        // And a bank slide string can be replaced through its location.
        let (hits, _) = find_in_course(&course, "Bank prompt", false, false);
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn variable_names_longer_than_the_limit_are_not_references() {
        let long = format!("%{}%", "n".repeat(129));
        assert!(variable_reference_spans(&long).is_empty());
    }
}
