//! Text formatting for the ribbon's Font and Paragraph groups and the
//! editing shortcuts (Ctrl+B, Ctrl+I, Ctrl+U).
//!
//! One place answers the three questions every text control asks:
//!
//! - **What does it act on?** [`targets`]: the text being edited on the canvas
//!   (the selected characters, or the caret), else every selected object that
//!   holds text, whole.
//! - **What is it now?** [`state`]: font, size, bold… at the target, `None`
//!   where the target is mixed.
//! - **How does it change?** [`apply`]: one [`Change`], turned into engine
//!   commands (`text.style_range`, `text.set_paragraph`, `text.set_paragraphs`)
//!   run as one undo step. Typed text not yet committed is committed in the
//!   same step, and a style picked at a caret waits for the next typed text.
//!
//! DeckCraft does this in its engine (`format.state`, `format.*`) because its
//! engine owns the text selection. Here the in-place editor's buffer and caret
//! live in the editor, so the resolver lives here, and the engine keeps its
//! explicit, id-addressed commands.

use coursecraft_course::{
    Align, CaseChange, Course, ListKind, NumberScheme, ObjectKind, Paragraph, TextDirection,
    TextRun, TextStyle, VAlign,
};
use serde_json::{Value as Json, json};

use crate::Editor;
use crate::properties::{
    PendingCaretStyle, default_shape_text_style, effects_tri_for_range, style_tri_for_range,
};

/// Which characters of one object a change applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// The object's whole text.
    Whole,
    /// Characters `start..end` of the text being edited.
    Range(usize, usize),
    /// The caret of the text being edited: character changes wait for the
    /// next typed text, paragraph changes apply to its paragraph.
    Caret(usize),
}

/// One object's text a change applies to.
#[derive(Debug, Clone, PartialEq)]
pub struct Target {
    pub slide: String,
    pub object: String,
    /// A table cell being edited, as `(row, column)`.
    pub cell: Option<(usize, usize)>,
    pub scope: Scope,
}

/// The formatting at the targets; `None` where they differ.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextState {
    pub font: Option<String>,
    pub size: Option<f32>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    pub strikethrough: Option<bool>,
    pub color: Option<String>,
    /// Character spacing in points; `None` where mixed.
    pub letter_spacing: Option<f32>,
    pub align: Option<Align>,
    pub valign: Option<VAlign>,
    pub wrap: Option<bool>,
    pub list: Option<ListKind>,
    pub level: Option<u8>,
    pub line_spacing: Option<f32>,
    /// Text shadow; `None` where mixed.
    pub shadow: Option<bool>,
    /// The highlight colour when every character has the same one.
    pub highlight: Option<String>,
    pub direction: Option<TextDirection>,
    /// The numbering scheme when every covered paragraph has the same explicit one.
    pub number_scheme: Option<NumberScheme>,
    /// The first number when every covered paragraph has the same explicit one.
    pub start_at: Option<u32>,
}

/// One formatting change, as the ribbon and the shortcuts ask for it.
#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    Bold,
    Italic,
    Underline,
    Strikethrough,
    Font(String),
    Size(f32),
    Grow,
    Shrink,
    Color(String),
    /// Extra space after each letter, in points.
    Spacing(f32),
    Align(Align),
    VAlign(VAlign),
    /// Toggle wrapping text inside the box width (converts point text to area text).
    Wrap,
    /// Bullets or Numbering: on, or off when every target paragraph has it.
    List(ListKind),
    /// One indent level in (`1`) or out (`-1`).
    Indent(i8),
    LineSpacing(f32),
    /// Text shadow on, or off when every target character has it.
    Shadow,
    /// A highlight colour, or `None` for No Color.
    Highlight(Option<String>),
    /// Remove the character formatting (links stay).
    ClearFormatting,
    /// Change the case of the letters.
    Case(CaseChange),
    /// Numbered list with this scheme.
    NumberScheme(NumberScheme),
    /// Numbered list starting at this number.
    StartAt(u32),
    /// Text direction of the whole box.
    Direction(TextDirection),
}

/// The font sizes offered, and the steps of Grow Font and Shrink Font.
pub const SIZES: [f32; 22] = [
    8.0, 9.0, 10.0, 11.0, 12.0, 14.0, 16.0, 18.0, 20.0, 24.0, 28.0, 32.0, 36.0, 40.0, 44.0, 48.0,
    54.0, 60.0, 66.0, 72.0, 80.0, 96.0,
];

/// Line spacing multiples offered.
pub const LINE_SPACINGS: [f32; 6] = [1.0, 1.15, 1.5, 2.0, 2.5, 3.0];

/// Character spacing presets (label, points).
pub const SPACINGS: [(&str, f32); 5] = [
    ("Very Tight", -3.0),
    ("Tight", -1.5),
    ("Normal", 0.0),
    ("Loose", 3.0),
    ("Very Loose", 6.0),
];

const NO_TEXT: &str = "Select a text box, or a shape that holds text";
const IN_STATE: &str = "Use the Text panel to format text in a state";
const BUTTON: &str = "Use the Format inspector to format a button's label";

/// What the text controls act on, or why they can't act.
pub fn targets(editor: &Editor) -> Result<Vec<Target>, &'static str> {
    if editor.ui.story_view {
        return Err("Open a slide first");
    }
    if editor.ui.editing_state.is_some() {
        return Err(IN_STATE);
    }
    if let Some(edit) = &editor.ui.inline_edit {
        if edit.state_name.is_some() || edit.extra_id.is_some() {
            return Err(IN_STATE);
        }
        if edit.is_button {
            return Err(BUTTON);
        }
        let scope = match &editor.ui.inline_edit_selection {
            Some((slide, object, range, caret))
                if *slide == edit.slide && *object == edit.object =>
            {
                match range {
                    Some((a, b)) if a != b => Scope::Range(*a.min(b), *a.max(b)),
                    _ => Scope::Caret(*caret),
                }
            }
            _ => Scope::Caret(edit.buffer.chars().count()),
        };
        return Ok(vec![Target {
            slide: edit.slide.clone(),
            object: edit.object.clone(),
            cell: edit.cell,
            scope,
        }]);
    }
    let course = &editor.session.course;
    let slide = &editor.selection.slide;
    let found: Vec<Target> = editor
        .selection
        .ids()
        .into_iter()
        .filter(|id| stored_text(course, slide, id, None).is_some())
        .map(|object| Target {
            slide: slide.clone(),
            object,
            cell: None,
            scope: Scope::Whole,
        })
        .collect();
    if found.is_empty() {
        Err(NO_TEXT)
    } else {
        Ok(found)
    }
}

/// One object's text as stored, or as typed while it is being edited.
///
/// Borrowed from the course or the edit buffer: the ribbon reads this every
/// frame, so it must not deep-copy the text.
struct Parts<'a> {
    style: std::borrow::Cow<'a, TextStyle>,
    runs: &'a [TextRun],
    text: &'a str,
    paragraphs: &'a [Paragraph],
}

fn stored_text<'a>(
    course: &'a Course,
    slide: &str,
    object: &str,
    cell: Option<(usize, usize)>,
) -> Option<Parts<'a>> {
    use std::borrow::Cow;
    let (obj, _) = course.slide(slide)?.object(object)?;
    match (&obj.kind, cell) {
        (
            ObjectKind::Text {
                text,
                style,
                runs,
                paragraphs,
                ..
            },
            None,
        ) => Some(Parts {
            style: Cow::Borrowed(style),
            runs,
            text,
            paragraphs,
        }),
        (
            ObjectKind::Shape {
                shape, fill, text, ..
            },
            None,
        ) if coursecraft_course::shape_allows_text(*shape) => Some(match text {
            Some(t) => Parts {
                style: Cow::Borrowed(&t.style),
                runs: &t.runs,
                text: &t.text,
                paragraphs: &t.paragraphs,
            },
            None => Parts {
                style: Cow::Owned(default_shape_text_style(fill)),
                runs: &[],
                text: "",
                paragraphs: &[],
            },
        }),
        (ObjectKind::Table { rows, .. }, Some((row, column))) => {
            let cell = rows.get(row)?.cells.get(column)?;
            Some(Parts {
                style: Cow::Borrowed(&cell.style),
                runs: &cell.runs,
                text: &cell.text,
                paragraphs: &cell.paragraphs,
            })
        }
        _ => None,
    }
}

/// The target's text, with what is being typed in place of what is stored.
fn parts<'a>(editor: &'a Editor, target: &Target) -> Option<Parts<'a>> {
    let mut parts = stored_text(
        &editor.session.course,
        &target.slide,
        &target.object,
        target.cell,
    )?;
    if let Some(edit) =
        editor.ui.inline_edit.as_ref().filter(|e| {
            e.slide == target.slide && e.object == target.object && e.cell == target.cell
        })
    {
        parts.text = &edit.buffer;
        let live_matches = edit
            .live_runs
            .iter()
            .flat_map(|r| r.text.chars())
            .eq(edit.buffer.chars());
        if live_matches {
            parts.runs = &edit.live_runs;
        }
        if !edit.live_paragraphs.is_empty() {
            parts.paragraphs = &edit.live_paragraphs;
        }
    }
    Some(parts)
}

/// The character range the target covers, `(start, end)`, empty at a caret.
fn char_range(scope: Scope, text: &str) -> (usize, usize) {
    match scope {
        Scope::Whole => (0, text.chars().count()),
        Scope::Range(a, b) => (a, b),
        Scope::Caret(c) => (c, c),
    }
}

/// The paragraphs the target covers, `(first, end)`.
fn paragraph_range(scope: Scope, text: &str) -> (usize, usize) {
    match scope {
        Scope::Whole => (0, coursecraft_course::text::paragraph_count(text)),
        Scope::Range(a, b) => coursecraft_course::text::paragraph_range_at(text, a, b),
        Scope::Caret(c) => coursecraft_course::text::paragraph_range_at(text, c, c),
    }
}

/// The runs a character range touches (`None` for plain text), or the run typing at a
/// caret would continue.
fn runs_touching(runs: &[TextRun], range: (usize, usize)) -> Vec<Option<&TextRun>> {
    let (start, end) = (range.0.min(range.1), range.0.max(range.1));
    if runs.is_empty() {
        return vec![None];
    }
    if start == end {
        return vec![crate::properties::run_at_caret(runs, start)];
    }
    let mut offset = 0usize;
    let mut out = Vec::new();
    for run in runs {
        let run_start = offset;
        offset = offset.saturating_add(run.text.chars().count());
        if offset > start && run_start < end {
            out.push(Some(run));
        }
    }
    if out.is_empty() {
        out.push(None);
    }
    out
}

/// The one value `f` gives for every run in the range, `None` when they differ.
fn same_in_range<T: PartialEq>(
    runs: &[TextRun],
    range: (usize, usize),
    f: impl Fn(Option<&TextRun>) -> T,
) -> Option<T> {
    let touched = runs_touching(runs, range);
    let first = f(*touched.first()?);
    touched.iter().all(|r| f(*r) == first).then_some(first)
}

/// Shadow at the range: on, off, or `None` when mixed.
pub(crate) fn shadow_for_range(runs: &[TextRun], range: (usize, usize)) -> Option<bool> {
    same_in_range(runs, range, |r| r.and_then(|r| r.shadow).unwrap_or(false))
}

/// The highlight colour shared by the whole range, `None` when none or mixed.
pub(crate) fn highlight_for_range(runs: &[TextRun], range: (usize, usize)) -> Option<String> {
    same_in_range(runs, range, |r| {
        r.and_then(|r| r.highlight.as_ref()).map(|c| c.0.clone())
    })
    .flatten()
}

fn merge<T: PartialEq>(acc: &mut Option<Option<T>>, value: Option<T>) {
    match acc {
        None => *acc = Some(value),
        Some(seen) if *seen != value => *seen = None,
        Some(_) => {}
    }
}

fn one_state(editor: &Editor, target: &Target) -> Option<TextState> {
    let parts = parts(editor, target)?;
    let range = char_range(target.scope, parts.text);
    let (bold, italic, underline) = style_tri_for_range(parts.runs, &parts.style, range);
    let (strikethrough, _, _) = effects_tri_for_range(parts.runs, &parts.style, range);
    // Font, size and colour read `None` where the range mixes values (`N99`).
    let base = &parts.style;
    let uniform = |own: &dyn Fn(&TextRun) -> Option<String>, base_value: String| {
        if parts.runs.is_empty() {
            Some(base_value)
        } else {
            same_in_range(parts.runs, range, |r| {
                r.and_then(own).unwrap_or_else(|| base_value.clone())
            })
        }
    };
    let font = uniform(&|r| r.font.clone(), base.font.clone());
    let size = uniform(&|r| r.size.map(|v| v.to_string()), base.size.to_string())
        .and_then(|v| v.parse::<f32>().ok());
    let color = uniform(
        &|r| r.color.as_ref().map(|c| c.0.clone()),
        base.color.0.clone(),
    );
    let (first, end) = paragraph_range(target.scope, parts.text);
    let paragraphs = coursecraft_course::text::paragraphs_for(parts.text, parts.paragraphs);
    let covered: Vec<&Paragraph> = paragraphs
        .iter()
        .skip(first)
        .take(end.saturating_sub(first).max(1))
        .collect();
    let same = |f: &dyn Fn(&Paragraph) -> f32| {
        let first = covered.first().map(|p| f(p));
        first.filter(|v| covered.iter().all(|p| (f(p) - v).abs() < f32::EPSILON))
    };
    let list = covered.first().map(|p| p.list);
    let list = list.filter(|l| covered.iter().all(|p| p.list == *l));
    let level = covered
        .first()
        .map(|p| p.level)
        .filter(|l| covered.iter().all(|p| p.level == *l));
    let mut state = TextState {
        font,
        size,
        bold,
        italic,
        underline,
        strikethrough,
        color,
        letter_spacing: same_in_range(parts.runs, range, |r| {
            r.and_then(|r| r.letter_spacing)
                .unwrap_or(parts.style.letter_spacing)
        }),
        align: Some(parts.style.align),
        valign: Some(parts.style.valign),
        wrap: Some(parts.style.wrap),
        list: list.or(covered.is_empty().then_some(ListKind::None)),
        level: level.or(covered.is_empty().then_some(0)),
        line_spacing: same(&|p| p.line_spacing).or(covered.is_empty().then_some(1.0)),
        shadow: shadow_for_range(parts.runs, range),
        highlight: highlight_for_range(parts.runs, range),
        direction: Some(parts.style.direction),
        number_scheme: covered
            .first()
            .and_then(|p| p.number_scheme)
            .filter(|scheme| covered.iter().all(|p| p.number_scheme == Some(*scheme))),
        start_at: covered
            .first()
            .and_then(|p| p.start_at)
            .filter(|n| covered.iter().all(|p| p.start_at == Some(*n))),
    };
    // A style picked at the caret shows until text is typed with it.
    if let Scope::Caret(caret) = target.scope
        && let Some(pending) = editor
            .ui
            .properties
            .pending_caret_style
            .as_ref()
            .filter(|p| {
                p.slide == target.slide
                    && p.object == target.object
                    && p.cell == target.cell
                    && p.cursor == caret
            })
    {
        let patch = &pending.patch;
        state.bold = patch.bold.or(state.bold);
        state.italic = patch.italic.or(state.italic);
        state.underline = patch.underline.or(state.underline);
        state.strikethrough = patch.strikethrough.or(state.strikethrough);
        state.size = patch.size.or(state.size);
        state.letter_spacing = patch.letter_spacing.or(state.letter_spacing);
        state.font = patch.font.clone().or(state.font);
        state.color = patch.color.as_ref().map(|c| c.0.clone()).or(state.color);
        state.shadow = patch.shadow.or(state.shadow);
        if let Some(highlight) = &patch.highlight {
            state.highlight = highlight.as_ref().map(|c| c.0.clone());
        }
    }
    Some(state)
}

/// The formatting at what the text controls act on; `None` when they can't act.
#[must_use]
pub fn state(editor: &Editor) -> Option<TextState> {
    let targets = targets(editor).ok()?;
    let states: Vec<TextState> = targets
        .iter()
        .filter_map(|t| one_state(editor, t))
        .collect();
    let first = states.first()?.clone();
    if states.len() == 1 {
        return Some(first);
    }
    let mut font = None;
    let mut size = None;
    let mut bold = None;
    let mut italic = None;
    let mut underline = None;
    let mut strike = None;
    let mut color = None;
    let mut letter_spacing = None;
    let mut align = None;
    let mut valign = None;
    let mut wrap = None;
    let mut list = None;
    let mut level = None;
    let mut spacing = None;
    let mut shadow = None;
    let mut highlight = None;
    let mut direction = None;
    let mut scheme = None;
    let mut start_at = None;
    for s in states {
        merge(&mut font, s.font);
        merge(&mut size, s.size);
        merge(&mut bold, s.bold);
        merge(&mut italic, s.italic);
        merge(&mut underline, s.underline);
        merge(&mut strike, s.strikethrough);
        merge(&mut color, s.color);
        merge(&mut letter_spacing, s.letter_spacing);
        merge(&mut align, s.align);
        merge(&mut valign, s.valign);
        merge(&mut wrap, s.wrap);
        merge(&mut list, s.list);
        merge(&mut level, s.level);
        merge(&mut spacing, s.line_spacing);
        merge(&mut shadow, s.shadow);
        merge(&mut highlight, s.highlight);
        merge(&mut direction, s.direction);
        merge(&mut scheme, s.number_scheme);
        merge(&mut start_at, s.start_at);
    }
    Some(TextState {
        font: font.flatten(),
        size: size.flatten(),
        bold: bold.flatten(),
        italic: italic.flatten(),
        underline: underline.flatten(),
        strikethrough: strike.flatten(),
        color: color.flatten(),
        letter_spacing: letter_spacing.flatten(),
        align: align.flatten(),
        valign: valign.flatten(),
        wrap: wrap.flatten(),
        list: list.flatten(),
        level: level.flatten(),
        line_spacing: spacing.flatten(),
        shadow: shadow.flatten(),
        highlight: highlight.flatten(),
        direction: direction.flatten(),
        number_scheme: scheme.flatten(),
        start_at: start_at.flatten(),
    })
}

/// The next size up (`up`) or down from `size` in [`SIZES`]; past either end,
/// one point (down) or ten percent (up).
#[must_use]
pub fn step_size(size: f32, up: bool) -> f32 {
    if up {
        SIZES
            .iter()
            .copied()
            .find(|s| *s > size + 0.01)
            .unwrap_or((size * 1.1).round().min(400.0))
    } else {
        SIZES
            .iter()
            .rev()
            .copied()
            .find(|s| *s < size - 0.01)
            .unwrap_or((size - 1.0).max(1.0))
    }
}

fn address(target: &Target) -> Json {
    let mut params = json!({ "slide": target.slide, "object": target.object });
    if let Some((row, column)) = target.cell {
        params["row"] = json!(row);
        params["column"] = json!(column);
    }
    params
}

fn align_name(align: Align) -> &'static str {
    match align {
        Align::Left => "left",
        Align::Center => "center",
        Align::Right => "right",
        Align::Justify => "justify",
    }
}

fn valign_name(valign: VAlign) -> &'static str {
    match valign {
        VAlign::Top => "top",
        VAlign::Middle => "middle",
        VAlign::Bottom => "bottom",
    }
}

fn direction_name(direction: TextDirection) -> &'static str {
    match direction {
        TextDirection::Horizontal => "horizontal",
        TextDirection::Vertical => "vertical",
        TextDirection::Vertical270 => "vertical270",
    }
}

fn case_name(case: CaseChange) -> &'static str {
    match case {
        CaseChange::Sentence => "sentence",
        CaseChange::Lower => "lower",
        CaseChange::Upper => "upper",
        CaseChange::Title => "title",
        CaseChange::Toggle => "toggle",
    }
}

fn scheme_name(scheme: NumberScheme) -> serde_json::Value {
    serde_json::to_value(scheme).unwrap_or(Json::Null)
}

fn list_name(list: ListKind) -> &'static str {
    match list {
        ListKind::None => "none",
        ListKind::Bullet => "bullet",
        ListKind::Number => "number",
    }
}

/// The character fields `change` sets, as `text.style_range` parameters.
fn character_fields(change: &Change, now: &TextState, size: f32) -> Option<Json> {
    Some(match change {
        Change::Bold => json!({ "bold": now.bold != Some(true) }),
        Change::Italic => json!({ "italic": now.italic != Some(true) }),
        Change::Underline => json!({ "underline": now.underline != Some(true) }),
        Change::Strikethrough => json!({ "strikethrough": now.strikethrough != Some(true) }),
        Change::Font(name) => json!({ "font": name }),
        Change::Size(size) => json!({ "size": size }),
        Change::Grow => json!({ "size": step_size(size, true) }),
        Change::Shrink => json!({ "size": step_size(size, false) }),
        Change::Color(color) => json!({ "color": color }),
        Change::Spacing(points) => json!({ "letter_spacing": points }),
        Change::Shadow => json!({ "shadow": now.shadow != Some(true) }),
        Change::Highlight(color) => json!({ "highlight": color }),
        Change::Align(_)
        | Change::VAlign(_)
        | Change::Wrap
        | Change::List(_)
        | Change::Indent(_)
        | Change::LineSpacing(_)
        | Change::ClearFormatting
        | Change::Case(_)
        | Change::NumberScheme(_)
        | Change::StartAt(_)
        | Change::Direction(_) => return None,
    })
}

fn patch_pending(patch: &mut coursecraft_course::TextRunPatch, fields: &Json) {
    let flag = |key: &str| fields.get(key).and_then(Json::as_bool);
    let number = |key: &str| fields.get(key).and_then(Json::as_f64).map(|v| v as f32);
    if let Some(v) = flag("bold") {
        patch.bold = Some(v);
    }
    if let Some(v) = flag("italic") {
        patch.italic = Some(v);
    }
    if let Some(v) = flag("underline") {
        patch.underline = Some(v);
    }
    if let Some(v) = flag("strikethrough") {
        patch.strikethrough = Some(v);
    }
    if let Some(v) = number("size") {
        patch.size = Some(v);
    }
    if let Some(v) = number("letter_spacing") {
        patch.letter_spacing = Some(v);
    }
    if let Some(v) = fields.get("font").and_then(Json::as_str) {
        patch.font = Some(v.to_owned());
    }
    if let Some(v) = fields.get("color").and_then(Json::as_str) {
        patch.color = Some(coursecraft_course::Color(v.to_owned()));
    }
    if let Some(v) = flag("shadow") {
        patch.shadow = Some(v);
    }
    if let Some(v) = fields.get("highlight") {
        patch.highlight = Some(
            v.as_str()
                .map(|hex| coursecraft_course::Color(hex.to_owned())),
        );
    }
}

/// Apply `change` to what the text controls act on, as one undo step.
/// Returns whether anything changed.
pub fn apply(editor: &mut Editor, change: &Change) -> bool {
    let Ok(targets) = targets(editor) else {
        return false;
    };
    let Some(now) = state(editor) else {
        return false;
    };
    let mut steps: Vec<(String, Json)> = Vec::new();
    // Typed text not yet committed goes first, so ranges match what is shown.
    let dirty = editor
        .ui
        .inline_edit
        .as_ref()
        .filter(|e| e.buffer != e.original)
        .map(|e| {
            let mut params = json!({ "slide": e.slide, "object": e.object, "text": e.buffer });
            if let Some((row, column)) = e.cell {
                params["row"] = json!(row);
                params["column"] = json!(column);
            }
            params
        });
    for target in &targets {
        let Some(parts) = parts(editor, target) else {
            continue;
        };
        if let Change::ClearFormatting | Change::Case(_) = change {
            // A caret has no characters to change.
            let (start, end) = match target.scope {
                Scope::Range(a, b) => (a, b),
                Scope::Whole => (0, 0),
                Scope::Caret(_) => continue,
            };
            let mut params = address(target);
            params["start"] = json!(start);
            params["end"] = json!(end);
            if let Change::Case(case) = change {
                params["case"] = json!(case_name(*case));
                steps.push(("text.change_case".to_owned(), params));
            } else {
                steps.push(("text.clear_formatting".to_owned(), params));
            }
            continue;
        }
        let own = one_state(editor, target).unwrap_or_default();
        let size = own.size.or(now.size).unwrap_or(parts.style.size);
        if let Some(fields) = character_fields(change, &now, size) {
            match target.scope {
                Scope::Caret(caret) => {
                    let pending =
                        editor
                            .ui
                            .properties
                            .pending_caret_style
                            .get_or_insert_with(|| PendingCaretStyle {
                                slide: target.slide.clone(),
                                object: target.object.clone(),
                                cell: target.cell,
                                cursor: caret,
                                patch: coursecraft_course::TextRunPatch::default(),
                            });
                    if pending.slide != target.slide
                        || pending.object != target.object
                        || pending.cell != target.cell
                        || pending.cursor != caret
                    {
                        *pending = PendingCaretStyle {
                            slide: target.slide.clone(),
                            object: target.object.clone(),
                            cell: target.cell,
                            cursor: caret,
                            patch: coursecraft_course::TextRunPatch::default(),
                        };
                    }
                    patch_pending(&mut pending.patch, &fields);
                }
                Scope::Whole | Scope::Range(..) => {
                    let (start, end) = match target.scope {
                        Scope::Range(a, b) => (a, b),
                        _ => (0, 0),
                    };
                    let mut params = address(target);
                    params["start"] = json!(start);
                    params["end"] = json!(end);
                    if let (Json::Object(to), Json::Object(from)) = (&mut params, fields) {
                        to.extend(from);
                    }
                    steps.push(("text.style_range".to_owned(), params));
                }
            }
            continue;
        }
        let mut params = address(target);
        match change {
            Change::Align(align) => params["align"] = json!(align_name(*align)),
            Change::VAlign(valign) => params["valign"] = json!(valign_name(*valign)),
            Change::Wrap => params["wrap"] = json!(now.wrap != Some(true)),
            Change::Direction(direction) => params["direction"] = json!(direction_name(*direction)),
            Change::List(_)
            | Change::Indent(_)
            | Change::LineSpacing(_)
            | Change::NumberScheme(_)
            | Change::StartAt(_) => {
                let (first, end) = paragraph_range(target.scope, parts.text);
                if target.scope != Scope::Whole {
                    params["start"] = json!(first);
                    params["end"] = json!(end.max(first + 1));
                }
                match change {
                    Change::List(kind) => {
                        let next = if now.list == Some(*kind) {
                            ListKind::None
                        } else {
                            *kind
                        };
                        params["list"] = json!(list_name(next));
                    }
                    Change::Indent(delta) => {
                        let level = i16::from(own.level.unwrap_or(0)) + i16::from(*delta);
                        params["level"] = json!(level.clamp(0, 4));
                    }
                    Change::LineSpacing(lines) => params["line_spacing"] = json!(lines),
                    Change::NumberScheme(scheme) => {
                        params["list"] = json!("number");
                        params["number_scheme"] = scheme_name(*scheme);
                    }
                    Change::StartAt(number) => {
                        params["list"] = json!("number");
                        params["start_at"] = json!(number);
                    }
                    _ => {}
                }
                steps.push(("text.set_paragraphs".to_owned(), params));
                continue;
            }
            _ => continue,
        }
        steps.push(("text.set_paragraph".to_owned(), params));
    }
    if steps.is_empty() {
        // A caret style: nothing to run until text is typed.
        return editor.ui.properties.pending_caret_style.is_some();
    }
    if let Some(text) = dirty {
        steps.insert(0, ("text.set".to_owned(), text));
    }
    let done = editor.run_batch(&steps).is_some();
    if done && matches!(change, Change::Case(_)) {
        // The letters changed: the edit buffer follows what is stored now.
        let refreshed = editor.ui.inline_edit.as_ref().and_then(|edit| {
            let stored = stored_text(&editor.session.course, &edit.slide, &edit.object, edit.cell)?;
            Some(if stored.runs.is_empty() {
                stored.text.to_owned()
            } else {
                stored.runs.iter().map(|r| r.text.as_str()).collect()
            })
        });
        if let (Some(text), Some(edit)) = (refreshed, editor.ui.inline_edit.as_mut()) {
            edit.buffer = text;
        }
    }
    if done && let Some(edit) = editor.ui.inline_edit.as_mut() {
        edit.original = edit.buffer.clone();
    }
    done
}
