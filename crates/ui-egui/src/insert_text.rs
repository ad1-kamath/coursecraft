//! Insert ▸ Text group: the shared "insert this text" helper, the Slide Number field, the
//! Symbol picker window and the Reference (variable) picker window (M39 package I).
//!
//! [`insert_text`] is the one way these controls put text on a slide. While a text object is
//! edited in place, the text goes in at the caret (replacing the selection) as part of that
//! edit. Otherwise a new text box holding the text is created at the slide centre and
//! selected, in one undo step.

use crate::dialog::Dialog;

use coursecraft_course::{Course, ValueKind};
use egui::{Id, Rect, RichText, Vec2};

/// The reference a Slide Number field inserts.
pub const SLIDE_NUMBER_REF: &str = "%Course.SlideNumber%";
/// The reference "Slide number of total" inserts.
pub const SLIDE_NUMBER_OF_TOTAL_REF: &str = "%Course.SlideNumber% / %Course.TotalSlides%";
/// How many symbols the "Recently used" row keeps.
pub const RECENT_MAX: usize = 16;

/// One group of the Symbol picker.
pub struct SymbolGroup {
    pub label: &'static str,
    /// Each symbol with its English name (searched, and shown as the tooltip).
    pub symbols: &'static [(char, &'static str)],
}

/// The symbols the Symbol picker offers. Every one is drawn by the bundled fonts: a test checks
/// each against the installed font definitions. The bundled Inter is a Latin subset, so Greek
/// letters, most arrows, most maths signs and most geometric shapes are not offered.
pub const SYMBOL_GROUPS: &[SymbolGroup] = &[
    SymbolGroup {
        label: "Common",
        symbols: &[
            ('©', "Copyright"),
            ('®', "Registered trademark"),
            ('™', "Trademark"),
            ('°', "Degree"),
            ('±', "Plus or minus"),
            ('×', "Multiplication"),
            ('÷', "Division"),
            ('€', "Euro"),
            ('£', "Pound"),
            ('¥', "Yen"),
            ('¢', "Cent"),
            ('§', "Section"),
            ('¶', "Paragraph"),
            ('•', "Bullet"),
            ('…', "Ellipsis"),
            ('—', "Em dash"),
            ('–', "En dash"),
            ('«', "Left double angle quote"),
            ('»', "Right double angle quote"),
            ('‹', "Left single angle quote"),
            ('›', "Right single angle quote"),
            ('¿', "Inverted question mark"),
            ('¡', "Inverted exclamation mark"),
            ('‘', "Left single quote"),
            ('’', "Right single quote"),
            ('“', "Left double quote"),
            ('”', "Right double quote"),
            ('†', "Dagger"),
            ('‡', "Double dagger"),
            ('·', "Middle dot"),
            ('µ', "Micro"),
            ('¼', "One quarter"),
            ('½', "One half"),
            ('¾', "Three quarters"),
            ('²', "Superscript two"),
            ('³', "Superscript three"),
            ('‰', "Per mille"),
        ],
    },
    SymbolGroup {
        label: "Arrows",
        symbols: &[
            ('←', "Left arrow"),
            ('→', "Right arrow"),
            ('↑', "Up arrow"),
            ('↓', "Down arrow"),
        ],
    },
    SymbolGroup {
        label: "Shapes and check marks",
        symbols: &[('✓', "Check mark"), ('●', "Black circle")],
    },
    SymbolGroup {
        label: "Maths",
        symbols: &[('−', "Minus"), ('¬', "Not")],
    },
];

/// The state of the Insert ▸ Symbol and Insert ▸ Reference windows, and of an insert waiting
/// for the in-place editor.
#[derive(Debug, Default)]
pub struct InsertTextState {
    pub symbol_open: bool,
    /// The group tab of the Symbol window (an index into [`SYMBOL_GROUPS`]).
    pub symbol_group: usize,
    pub symbol_query: String,
    pub symbol_selected: Option<char>,
    /// Symbols inserted this session, newest first.
    pub recent: Vec<char>,
    pub reference_open: bool,
    pub reference_filter: String,
    pub reference_selected: Option<String>,
    /// Text for the in-place editor to put in at the caret on its next pass.
    pending_inline: String,
    pub(crate) symbol_rect: Option<Rect>,
    pub(crate) reference_rect: Option<Rect>,
}

impl InsertTextState {
    /// Whether one of the two windows is open.
    #[must_use]
    pub fn any_window_open(&self) -> bool {
        self.symbol_open || self.reference_open
    }

    /// Whether `pos` is over one of the open windows (as of the last frame they were drawn).
    #[must_use]
    pub fn window_contains(&self, pos: egui::Pos2) -> bool {
        (self.symbol_open && self.symbol_rect.is_some_and(|r| r.contains(pos)))
            || (self.reference_open && self.reference_rect.is_some_and(|r| r.contains(pos)))
    }

    /// Take the text waiting for the in-place editor.
    pub(crate) fn take_pending_inline(&mut self) -> Option<String> {
        (!self.pending_inline.is_empty()).then(|| std::mem::take(&mut self.pending_inline))
    }

    /// Close both windows and drop anything waiting.
    pub fn reset(&mut self) {
        self.symbol_open = false;
        self.reference_open = false;
        self.pending_inline.clear();
        self.symbol_rect = None;
        self.reference_rect = None;
    }

    fn note_recent(&mut self, c: char) {
        self.recent.retain(|&r| r != c);
        self.recent.insert(0, c);
        self.recent.truncate(RECENT_MAX);
    }
}

/// Put `text` on the slide: at the caret of the text being edited in place (replacing the
/// selection), else in a new text box at the slide centre, selected. One undo step either way.
/// Returns whether anything was inserted.
pub fn insert_text(editor: &mut crate::Editor, text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    if editor.ui.inline_edit.is_some() {
        // The in-place editor owns the buffer and the caret. It splices the text in on its
        // next pass, and the edit commits as one step like any typing.
        editor.ui.insert_text.pending_inline.push_str(text);
        return true;
    }
    if editor.ui.editing_state.is_some() {
        editor.set_status(tl!(crate::STATE_LOCK_TOOLTIP));
        return false;
    }
    if editor.selection.slide.is_empty() {
        editor.set_status(tl!("Select a slide first"));
        return false;
    }
    let size = editor.session.course.slide_size;
    let (slide_w, slide_h) = (size.w as f32, size.h as f32);
    let (w, h) = ((slide_w * 0.6).clamp(120.0, 300.0), 60.0_f32);
    let mut params = serde_json::json!({
        "slide": editor.selection.slide,
        "kind": {
            "type": "text",
            "text": text,
            "autofit": "grow",
            "style": { "color": editor.session.course.theme.text.0 },
        },
        "frame": {
            "x": ((slide_w - w) * 0.5).round().max(0.0),
            "y": ((slide_h - h) * 0.5).round().max(0.0),
            "w": w.round(),
            "h": h,
        },
    });
    if let Some(layer) = &editor.selection.layer {
        params["layer"] = serde_json::json!(layer);
    }
    let Some(result) = editor.run("object.add", params) else {
        return false;
    };
    if let Some(id) = result.get("id").and_then(serde_json::Value::as_str) {
        editor.selection.set_many([id.to_owned()]);
    }
    true
}

/// Insert ▸ Slide Number: the slide number reference, or "number / total".
pub fn insert_slide_number(editor: &mut crate::Editor, of_total: bool) -> bool {
    insert_text(
        editor,
        if of_total {
            SLIDE_NUMBER_OF_TOTAL_REF
        } else {
            SLIDE_NUMBER_REF
        },
    )
}

/// Open the Symbol window.
pub fn open_symbol_picker(editor: &mut crate::Editor) {
    editor.ui.insert_text.symbol_open = true;
}

/// Open the Reference window.
pub fn open_reference_picker(editor: &mut crate::Editor) {
    editor.ui.insert_text.reference_open = true;
}

/// Whether Insert ▸ Text controls can put text on the slide now.
pub(crate) fn available(editor: &crate::Editor) -> Result<(), &'static str> {
    if editor.ui.inline_edit.is_some() {
        Ok(())
    } else if editor.ui.editing_state.is_some() {
        Err(crate::STATE_LOCK_TOOLTIP)
    } else if editor.selection.slide.is_empty() {
        Err("Select a slide first")
    } else {
        Ok(())
    }
}

/// Splice `text` into the in-place editor's `buffer` at the caret of the text edit `id`
/// (replacing the selection; at the end when it has no caret yet), and move the caret after it.
pub(crate) fn splice_into_buffer(ctx: &egui::Context, id: Id, buffer: &mut String, text: &str) {
    let mut state = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
    let len = buffer.chars().count();
    let (start, end) = state.cursor.char_range().map_or((len, len), |range| {
        let sorted = range.as_sorted_char_range();
        (sorted.start.0.min(len), sorted.end.0.min(len))
    });
    let byte = |chars: usize| {
        buffer
            .char_indices()
            .nth(chars)
            .map_or(buffer.len(), |(index, _)| index)
    };
    let (from, to) = (byte(start), byte(end));
    buffer.replace_range(from..to, text);
    let caret = start.saturating_add(text.chars().count());
    state
        .cursor
        .set_char_range(Some(egui::text_selection::CCursorRange::one(
            egui::text::CCursor::new(caret),
        )));
    state.store(ctx, id);
}

/// Put the text Insert ▸ Symbol, Reference and Slide Number left for the in-place editor into
/// `buffer` at the caret, and take the keyboard back from the window it came from.
pub(crate) fn apply_pending_inline(
    editor: &mut crate::Editor,
    ctx: &egui::Context,
    id: Id,
    buffer: &mut String,
) {
    if let Some(text) = editor.ui.insert_text.take_pending_inline() {
        splice_into_buffer(ctx, id, buffer, &text);
        editor.ui.inline_edit_yielded_to_panel = false;
    }
}

/// A name a `%Name%` reference can hold: non-empty, at most 128 bytes, letters, digits, `_`,
/// `-` and `.` (the rules of `variable_reference_spans`).
#[must_use]
pub fn valid_reference_name(name: &str) -> bool {
    let reference = format!("%{name}%");
    coursecraft_course::variable_reference_spans(&reference)
        .first()
        .is_some_and(|span| span.start == 0 && span.end == reference.len())
}

/// One row of the Reference window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceEntry {
    pub name: String,
    pub kind: ValueKind,
    pub builtin: bool,
}

/// The variables a reference can name: the built-in ones, then the course's, with names that
/// cannot be written as a reference left out.
#[must_use]
pub fn reference_entries(course: &Course) -> Vec<ReferenceEntry> {
    let builtin = coursecraft_course::BUILTIN_VARIABLES
        .iter()
        .map(|&(name, kind)| ReferenceEntry {
            name: name.to_owned(),
            kind,
            builtin: true,
        });
    let own = course.variables.iter().map(|v| ReferenceEntry {
        name: v.name.clone(),
        kind: v.value.kind(),
        builtin: false,
    });
    builtin
        .chain(own)
        .filter(|entry| valid_reference_name(&entry.name))
        .collect()
}

fn kind_label(kind: ValueKind) -> &'static str {
    match kind {
        ValueKind::Bool => tl!("True/false"),
        ValueKind::Number => tl!("Number"),
        ValueKind::Text => tl!("Text"),
    }
}

/// The symbols of group `group` that match `query` (by name or by the character itself), or
/// every symbol of every group when `query` is not empty and `all_groups` is set.
#[must_use]
pub fn matching_symbols(query: &str, group: usize) -> Vec<(char, &'static str)> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return SYMBOL_GROUPS
            .get(group)
            .map(|g| g.symbols.to_vec())
            .unwrap_or_default();
    }
    SYMBOL_GROUPS
        .iter()
        .flat_map(|g| g.symbols.iter().copied())
        .filter(|&(c, name)| {
            query.chars().eq(std::iter::once(c))
                || name.to_lowercase().contains(&query)
                || tl!(name).to_lowercase().contains(&query)
        })
        .collect()
}

fn insert_symbol(editor: &mut crate::Editor, c: char) {
    let mut buf = [0u8; 4];
    if insert_text(editor, c.encode_utf8(&mut buf)) {
        editor.ui.insert_text.note_recent(c);
    }
}

/// Draw the Insert ▸ Symbol window.
pub fn show_symbol_window(editor: &mut crate::Editor, ctx: &egui::Context) {
    if !editor.ui.insert_text.symbol_open {
        editor.ui.insert_text.symbol_rect = None;
        return;
    }
    let mut open = true;
    let mut close = false;
    let mut insert: Option<char> = None;
    let can_insert = available(editor).is_ok();
    let insert_reason = match available(editor) {
        Err(reason) => tl!(reason),
        Ok(()) => tl!("Select a symbol first"),
    };
    let response = Dialog::new(tl!("Symbol"))
        .id(Id::unique("insert_symbol_window"))
        .pinned_top()
        .open(&mut open)
        .width(420.0)
        .show_with_footer(ctx, |dialog| {
            let state = &mut editor.ui.insert_text;
            dialog.body(|ui| {
                ui.horizontal_wrapped(|ui| {
                    for (index, group) in SYMBOL_GROUPS.iter().enumerate() {
                        ui.selectable_value(&mut state.symbol_group, index, tl!(group.label));
                    }
                });
                ui.add(
                    egui::TextEdit::singleline(&mut state.symbol_query)
                        .id_salt("insert_symbol_search")
                        .hint_text(tl!("Search by name"))
                        .desired_width(ui.available_width()),
                );
                if !state.recent.is_empty() {
                    ui.add_space(coursecraft_render::SPACE_SM);
                    ui.label(RichText::new(tl!("Recently used")).small().weak());
                    let recent = state.recent.clone();
                    symbol_grid(ui, &recent, &mut state.symbol_selected, &mut insert);
                    ui.add_space(coursecraft_render::SPACE_SM);
                    ui.separator();
                }
                let shown = matching_symbols(&state.symbol_query, state.symbol_group);
                if shown.is_empty() {
                    ui.label(tl!("No symbol matches the search."));
                } else {
                    let chars: Vec<char> = shown.iter().map(|&(c, _)| c).collect();
                    symbol_grid(ui, &chars, &mut state.symbol_selected, &mut insert);
                }
                if let Some(selected) = state.symbol_selected
                    && let Some(&(_, name)) = SYMBOL_GROUPS
                        .iter()
                        .flat_map(|g| g.symbols.iter())
                        .find(|&&(c, _)| c == selected)
                {
                    ui.add_space(coursecraft_render::SPACE_SM);
                    ui.label(format!("{selected}  {}", tl!(name)));
                }
            });
            dialog.footer(|ui| {
                ui.horizontal(|ui| {
                    let selected = editor.ui.insert_text.symbol_selected;
                    let insert_button = ui.add_enabled(
                        selected.is_some() && can_insert,
                        egui::Button::new(tl!("Insert")),
                    );
                    let insert_button = insert_button.on_disabled_hover_text(insert_reason);
                    if insert_button.clicked() {
                        insert = insert.or(selected);
                    }
                    if ui.button(tl!("Close")).clicked() {
                        close = true;
                    }
                });
            });
        });
    editor.ui.insert_text.symbol_rect = response.map(|r| r.response.rect);
    if let Some(c) = insert
        && can_insert
    {
        insert_symbol(editor, c);
    }
    if close || !open {
        editor.ui.insert_text.symbol_open = false;
        editor.ui.insert_text.symbol_rect = None;
    }
}

/// A wrapped grid of symbol buttons. A click selects; a double click inserts.
fn symbol_grid(
    ui: &mut egui::Ui,
    chars: &[char],
    selected: &mut Option<char>,
    insert: &mut Option<char>,
) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::splat(2.0);
        for &c in chars {
            let is_selected = *selected == Some(c);
            let button = egui::Button::new(RichText::new(c.to_string()).size(18.0))
                .min_size(Vec2::splat(32.0))
                .selected(is_selected);
            let response = ui
                .add(button)
                .on_hover_text(format!("U+{:04X}", u32::from(c)));
            if response.clicked() {
                *selected = Some(c);
            }
            if response.double_clicked() {
                *selected = Some(c);
                *insert = Some(c);
            }
        }
    });
}

/// Draw the Insert ▸ Reference window.
pub fn show_reference_window(editor: &mut crate::Editor, ctx: &egui::Context) {
    if !editor.ui.insert_text.reference_open {
        editor.ui.insert_text.reference_rect = None;
        return;
    }
    let mut open = true;
    let mut close = false;
    let mut insert: Option<String> = None;
    let can_insert = available(editor).is_ok();
    let insert_reason = match available(editor) {
        Err(reason) => tl!(reason),
        Ok(()) => tl!("Select or type a valid variable name"),
    };
    let entries = reference_entries(&editor.session.course);
    let response = Dialog::new(tl!("Reference"))
        .id(Id::unique("insert_reference_window"))
        .open(&mut open)
        .width(380.0)
        .show_with_footer(ctx, |dialog| {
            dialog.body(|ui| {
                let state = &mut editor.ui.insert_text;
                ui.add(
                    egui::TextEdit::singleline(&mut state.reference_filter)
                        .id_salt("insert_reference_filter")
                        .hint_text(tl!("Filter variables"))
                        .desired_width(ui.available_width()),
                );
                ui.add_space(coursecraft_render::SPACE_SM);
                let filter = state.reference_filter.trim().to_lowercase();
                let mut any = false;
                for entry in entries
                    .iter()
                    .filter(|e| filter.is_empty() || e.name.to_lowercase().contains(&filter))
                {
                    any = true;
                    let is_selected = state.reference_selected.as_deref() == Some(&entry.name);
                    let label = if entry.builtin {
                        format!(
                            "{}   ({}, {})",
                            entry.name,
                            kind_label(entry.kind),
                            tl!("built in")
                        )
                    } else {
                        format!("{}   ({})", entry.name, kind_label(entry.kind))
                    };
                    let response = ui.selectable_label(is_selected, label);
                    if response.clicked() {
                        state.reference_selected = Some(entry.name.clone());
                    }
                    if response.double_clicked() {
                        state.reference_selected = Some(entry.name.clone());
                        insert = Some(entry.name.clone());
                    }
                }
                if !any {
                    ui.label(tl!("No variable matches the filter."));
                }
            });
            dialog.footer(|ui| {
                ui.horizontal(|ui| {
                    let selected = editor.ui.insert_text.reference_selected.clone();
                    let valid = selected
                        .as_deref()
                        .is_some_and(|name| entries.iter().any(|e| e.name == name));
                    if ui
                        .add_enabled(valid && can_insert, egui::Button::new(tl!("Insert")))
                        .on_disabled_hover_text(insert_reason)
                        .clicked()
                    {
                        insert = insert.take().or(selected);
                    }
                    if ui.button(tl!("Close")).clicked() {
                        close = true;
                    }
                });
            });
        });
    editor.ui.insert_text.reference_rect = response.map(|r| r.response.rect);
    if let Some(name) = insert
        && can_insert
        && valid_reference_name(&name)
    {
        insert_text(editor, &format!("%{name}%"));
    }
    if close || !open {
        editor.ui.insert_text.reference_open = false;
        editor.ui.insert_text.reference_rect = None;
    }
}
