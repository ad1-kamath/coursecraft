//! Help > Report a Problem, and the "closed unexpectedly" prompt.
//!
//! There is no server. A report opens as a pre-filled GitHub issue in the
//! person's browser, or is copied to the clipboard. Nothing is sent
//! automatically, and the technical details are shown in full so the person
//! sees exactly what would be shared. Course content, file names, file paths,
//! user names, AI keys and settings are never included.

use std::collections::VecDeque;

use egui::{Button, Context, ScrollArea, TextEdit, Window};

use crate::project_links::{BUG_TEMPLATE, FEATURE_TEMPLATE, new_issue_url};
use crate::{Editor, EditorRequest};
use coursecraft_render::design;

/// How many recent status messages the report carries.
pub const RECENT_MESSAGES: usize = 20;

/// Longest crash note kept, in chars, so a report stays a sensible size.
const MAX_CRASH_NOTE_CHARS: usize = 3_000;

/// Longest single status message in a report, in chars.
const MAX_MESSAGE_CHARS: usize = 200;

/// File extensions that mark a word as a file name.
const FILE_EXTENSIONS: &[&str] = &[
    "ccraft", "json", "png", "jpg", "jpeg", "gif", "svg", "webp", "bmp", "mp3", "wav", "m4a",
    "ogg", "flac", "mp4", "webm", "mov", "m4v", "pdf", "doc", "docx", "pptx", "xlsx", "txt", "md",
    "csv", "zip", "html", "htm", "js", "css", "ttf", "otf", "woff", "woff2", "xlf", "xliff", "srt",
    "vtt", "lottie", "scorm", "xml",
];

/// What the report and crash windows show and the person typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportState {
    pub open: bool,
    /// "What happened?"
    pub what: String,
    /// "Steps to reproduce"
    pub steps: String,
    /// Whether the technical details go into the report. On by default.
    pub include_details: bool,
    /// The note a crash left last time, already stripped of paths.
    pub crash_note: Option<String>,
    /// The "closed unexpectedly" dialog is showing.
    pub crash_prompt: bool,
    /// The newest status messages of this session, oldest first.
    pub messages: VecDeque<String>,
}

impl Default for ReportState {
    fn default() -> Self {
        Self {
            open: false,
            what: String::new(),
            steps: String::new(),
            include_details: true,
            crash_note: None,
            crash_prompt: false,
            messages: VecDeque::new(),
        }
    }
}

impl ReportState {
    /// Remember a status message, keeping only the newest [`RECENT_MESSAGES`].
    pub fn record(&mut self, message: &str) {
        if self.messages.len() >= RECENT_MESSAGES {
            self.messages.pop_front();
        }
        self.messages.push_back(message.to_string());
    }

    /// The technical details, as shown to the person and put in the report.
    #[must_use]
    pub fn technical_details(&self) -> String {
        let mut out = format!(
            "CourseCraft {}\nOS: {} ({})\nApp: {}\n",
            env!("CARGO_PKG_VERSION"),
            std::env::consts::OS,
            std::env::consts::ARCH,
            if cfg!(target_arch = "wasm32") {
                "web"
            } else {
                "desktop"
            }
        );
        out.push_str("\nRecent messages:\n");
        if self.messages.is_empty() {
            out.push_str("(none)\n");
        }
        for message in &self.messages {
            let clean: String = scrub(message).chars().take(MAX_MESSAGE_CHARS).collect();
            out.push_str(&format!("- {}\n", clean.replace('\n', " ")));
        }
        if let Some(note) = &self.crash_note {
            out.push_str("\nLast crash:\n");
            out.push_str(note.trim_end());
            out.push('\n');
        }
        out
    }

    /// The full report text: what the person wrote, then the details when
    /// they are included.
    #[must_use]
    pub fn report_text(&self) -> String {
        let mut out = format!(
            "## What happened?\n{}\n\n## Steps to reproduce\n{}\n",
            or_blank(&self.what),
            or_blank(&self.steps)
        );
        if self.include_details {
            out.push_str("\n## Technical details\n");
            out.push_str(&self.technical_details());
        }
        out
    }

    /// A short issue title from the first line of "What happened?".
    #[must_use]
    pub fn title(&self) -> String {
        let first = self.what.lines().find(|l| !l.trim().is_empty());
        match first {
            Some(line) => line.trim().chars().take(80).collect(),
            None => "Problem report".to_string(),
        }
    }

    /// The pre-filled "Report a problem" issue link.
    #[must_use]
    pub fn issue_url(&self) -> String {
        new_issue_url(BUG_TEMPLATE, &self.title(), &self.report_text())
    }
}

fn or_blank(text: &str) -> &str {
    if text.trim().is_empty() {
        "(not given)"
    } else {
        text.trim()
    }
}

/// Replace anything that looks like a file path with `<path>` and any file
/// name with `<file>`, so reports carry no user names or project names.
#[must_use]
pub fn scrub(text: &str) -> String {
    scrub_file_names(&scrub_paths(text))
}

/// Replace anything that looks like a path (`/Users/…`, `C:\…`, `~/…`,
/// `\\server\share`) with `<path>`. A path in quotes or brackets ends at the
/// closing mark, so folder names with spaces go too; an unquoted one ends at
/// the next space.
#[must_use]
pub fn scrub_paths(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let before = i.checked_sub(1).and_then(|p| chars.get(p)).copied();
        if path_starts_at(&chars, i, before) {
            i = skip_path(&chars, i, before);
            out.push_str("<path>");
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

fn opener_close(open: char) -> Option<char> {
    match open {
        '"' => Some('"'),
        '\'' => Some('\''),
        '`' => Some('`'),
        '(' => Some(')'),
        '[' => Some(']'),
        '<' => Some('>'),
        '{' => Some('}'),
        _ => None,
    }
}

fn path_starts_at(chars: &[char], i: usize, before: Option<char>) -> bool {
    let boundary =
        before.is_none_or(|b| b.is_whitespace() || opener_close(b).is_some() || b == '=');
    if !boundary {
        return false;
    }
    let at = |n: usize| chars.get(i + n).copied();
    match chars.get(i).copied() {
        Some('/') => at(1).is_some_and(|c| !c.is_whitespace() && c != '/'),
        Some('~') => at(1) == Some('/'),
        Some('\\') => at(1) == Some('\\') && at(2).is_some_and(|c| !c.is_whitespace()),
        Some(drive) if drive.is_ascii_alphabetic() => {
            at(1) == Some(':') && matches!(at(2), Some('\\' | '/'))
        }
        _ => false,
    }
}

/// The index just past the path starting at `i`.
fn skip_path(chars: &[char], i: usize, before: Option<char>) -> usize {
    let close = before.and_then(opener_close);
    let mut end = i;
    while let Some(&c) = chars.get(end) {
        let stop = match close {
            Some(closer) => c == closer || c == '\n',
            None => c.is_whitespace() && !next_word_is_path_part(chars, end),
        };
        if stop {
            break;
        }
        end += 1;
    }
    if close.is_none() {
        // Sentence punctuation after an unquoted path is not part of it.
        while end > i + 1 && chars.get(end - 1).is_some_and(|c| ".,;:!?)".contains(*c)) {
            end -= 1;
        }
    }
    end
}

/// True when the space at `at` is inside an unquoted path: the next word has a
/// path separator, as in `/Users/me/My Course/file`.
fn next_word_is_path_part(chars: &[char], at: usize) -> bool {
    if chars.get(at) != Some(&' ') {
        return false;
    }
    chars
        .iter()
        .skip(at + 1)
        .take_while(|c| !c.is_whitespace())
        .any(|c| matches!(c, '/' | '\\'))
}

/// Replace words that end in a known file extension with `<file>`.
fn scrub_file_names(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut word = String::new();
    for ch in text.chars() {
        if ch.is_alphanumeric() || matches!(ch, '.' | '_' | '-') {
            word.push(ch);
        } else {
            flush_word(&mut out, &mut word);
            out.push(ch);
        }
    }
    flush_word(&mut out, &mut word);
    out
}

fn flush_word(out: &mut String, word: &mut String) {
    if is_file_name(word) {
        out.push_str("<file>");
        out.push_str(&".".repeat(word.len() - word.trim_end_matches('.').len()));
    } else {
        out.push_str(word);
    }
    word.clear();
}

fn is_file_name(word: &str) -> bool {
    let trimmed = word.trim_end_matches('.');
    let Some((stem, ext)) = trimmed.rsplit_once('.') else {
        return false;
    };
    !stem.is_empty() && FILE_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str())
}

/// Keep a crash note to a sensible size, on a char boundary.
#[must_use]
pub fn limit_crash_note(note: &str) -> String {
    note.chars().take(MAX_CRASH_NOTE_CHARS).collect()
}

impl Editor {
    /// Open the Report a Problem window.
    pub fn open_report(&mut self) {
        self.ui.report.open = true;
    }

    /// Give the editor the note a crash left last time, and ask whether to
    /// report it. `None` clears it.
    pub fn set_crash_note(&mut self, note: Option<String>) {
        let note = note.map(|n| limit_crash_note(&scrub_paths(&n)));
        self.ui.report.crash_prompt = note.is_some();
        self.ui.report.crash_note = note;
    }

    /// Open the "closed unexpectedly" dialog's report window with the note.
    fn report_crash(&mut self, requests: &mut Vec<EditorRequest>) {
        self.ui.report.crash_prompt = false;
        self.ui.report.open = true;
        requests.push(EditorRequest::DiscardCrashNote);
    }

    /// "Not now" and "Don't ask again": forget the note and its file.
    fn dismiss_crash(&mut self, requests: &mut Vec<EditorRequest>) {
        self.ui.report.crash_prompt = false;
        self.ui.report.crash_note = None;
        requests.push(EditorRequest::DiscardCrashNote);
    }
}

/// Open a link in the browser.
pub fn open_link(ctx: &Context, url: &str) {
    ctx.open_url(egui::OpenUrl::new_tab(url));
}

/// Open the "Suggest a Feature" issue form.
pub fn open_feature_request(ctx: &Context) {
    open_link(ctx, &new_issue_url(FEATURE_TEMPLATE, "", ""));
}

/// Show the report window and the crash prompt when they are open.
pub fn show_report_windows(editor: &mut Editor, ctx: &Context, requests: &mut Vec<EditorRequest>) {
    show_crash_prompt(editor, ctx, requests);
    show_report_window(editor, ctx);
}

fn show_crash_prompt(editor: &mut Editor, ctx: &Context, requests: &mut Vec<EditorRequest>) {
    if !editor.ui.report.crash_prompt {
        return;
    }
    Window::new("CourseCraft closed unexpectedly")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            ui.label("CourseCraft closed unexpectedly last time. Report it?");
            ui.add_space(design::SPACE_MD);
            ui.horizontal(|ui| {
                if ui.add(Button::new("Report…")).clicked() {
                    editor.report_crash(requests);
                }
                if ui.add(Button::new("Not now")).clicked() {
                    editor.dismiss_crash(requests);
                }
                if ui.add(Button::new("Don't ask again")).clicked() {
                    editor.dismiss_crash(requests);
                }
            });
        });
}

fn show_report_window(editor: &mut Editor, ctx: &Context) {
    if !editor.ui.report.open {
        return;
    }
    let mut open = true;
    let mut done = false;
    Window::new("Report a Problem")
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_width(520.0)
        .show(ctx, |ui| {
            ScrollArea::vertical().max_height(560.0).show(ui, |ui| {
                report_fields(&mut editor.ui.report, ui);
            });
            ui.add_space(design::SPACE_MD);
            done = report_buttons(&editor.ui.report, ui, ctx);
        });
    if done || !open {
        editor.ui.report.open = false;
    }
}

fn report_fields(state: &mut ReportState, ui: &mut egui::Ui) {
    ui.label("What happened?");
    ui.add(
        TextEdit::multiline(&mut state.what)
            .desired_rows(3)
            .desired_width(f32::INFINITY),
    );
    ui.add_space(design::SPACE_SM);
    ui.label("Steps to reproduce");
    ui.add(
        TextEdit::multiline(&mut state.steps)
            .desired_rows(3)
            .desired_width(f32::INFINITY),
    );
    ui.add_space(design::SPACE_MD);
    ui.checkbox(&mut state.include_details, "Include technical details");
    ui.label("Technical details (this is everything that would be shared besides your text)");
    let details = state.technical_details();
    ui.add_enabled_ui(state.include_details, |ui| {
        ui.add(
            TextEdit::multiline(&mut details.as_str())
                .desired_rows(8)
                .desired_width(f32::INFINITY)
                .font(egui::TextStyle::Monospace),
        );
    });
}

/// The buttons and the privacy line. True when the window should close.
fn report_buttons(state: &ReportState, ui: &mut egui::Ui, ctx: &Context) -> bool {
    let mut close = false;
    ui.horizontal(|ui| {
        if ui.add(Button::new("Open on GitHub")).clicked() {
            open_link(ctx, &state.issue_url());
            close = true;
        }
        if ui.add(Button::new("Copy to Clipboard")).clicked() {
            ctx.copy_text(state.report_text());
        }
        if ui.add(Button::new("Cancel")).clicked() {
            close = true;
        }
    });
    ui.add_space(design::SPACE_SM);
    ui.label(design::caption(
        "This opens GitHub in your browser. Nothing is sent until you submit it there. \
         You need a free GitHub account; if you don't have one, use Copy.",
    ));
    close
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_editor() -> Editor {
        let json = include_str!("../../../examples/hello-course/course.json");
        Editor::new(coursecraft_course::Course::from_json(json).unwrap())
    }

    #[test]
    fn scrubs_unix_windows_and_home_paths() {
        assert_eq!(
            scrub_paths("Couldn't read /Users/aditya/My Course/a.ccraft: denied"),
            "Couldn't read <path>: denied"
        );
        assert_eq!(
            scrub_paths("open C:\\Users\\Bob\\x.txt now"),
            "open <path> now"
        );
        assert_eq!(scrub_paths("at ~/Documents/x ."), "at <path> .");
        assert_eq!(scrub_paths("path=/home/bob/x.json,"), "path=<path>,");
        assert_eq!(scrub_paths(r"share \\server\share\f"), "share <path>");
    }

    #[test]
    fn quoted_paths_with_spaces_go_whole() {
        assert_eq!(
            scrub_paths("Saved \"/Users/a b/My Course/x.ccraft\" ok"),
            "Saved \"<path>\" ok"
        );
        assert_eq!(
            scrub_paths("Saved '/Users/a b/c' and (C:\\Program Files\\x)"),
            "Saved '<path>' and (<path>)"
        );
    }

    #[test]
    fn ordinary_text_is_left_alone() {
        for text in [
            "Added slide 3/4",
            "https://example.com/a/b",
            "and/or n/a",
            "Ratio 16:9 saved",
            "Done.",
        ] {
            assert_eq!(scrub_paths(text), text);
        }
    }

    #[test]
    fn file_names_are_replaced() {
        assert_eq!(scrub("Inserted logo.PNG."), "Inserted <file>.");
        assert_eq!(
            scrub("Replaced my-voice_1.mp3 and a.docx"),
            "Replaced <file> and <file>"
        );
        assert_eq!(
            scrub("Version 1.2 saved, e.g. fine"),
            "Version 1.2 saved, e.g. fine"
        );
    }

    #[test]
    fn ring_buffer_keeps_the_newest_twenty() {
        let mut state = ReportState::default();
        for n in 0..30 {
            state.record(&format!("message {n}"));
        }
        assert_eq!(state.messages.len(), RECENT_MESSAGES);
        assert_eq!(
            state.messages.front().map(String::as_str),
            Some("message 10")
        );
        assert_eq!(
            state.messages.back().map(String::as_str),
            Some("message 29")
        );
    }

    #[test]
    fn details_have_version_os_app_messages_and_no_paths() {
        let mut state = ReportState::default();
        state.record("Saved /Users/bob/Secret Plan/course.ccraft");
        state.crash_note = Some("panic at x".into());
        let details = state.technical_details();
        assert!(details.contains(env!("CARGO_PKG_VERSION")));
        assert!(details.contains(std::env::consts::OS));
        assert!(details.contains(std::env::consts::ARCH));
        assert!(details.contains("App: desktop"));
        assert!(details.contains("- Saved <path>"));
        assert!(details.contains("Last crash:\npanic at x"));
        assert!(!details.contains("bob") && !details.contains("Secret"));
        assert!(!details.contains("course.ccraft"));
    }

    #[test]
    fn report_text_respects_the_checkbox() {
        let mut state = ReportState {
            what: "It broke\nbadly".into(),
            ..ReportState::default()
        };
        assert_eq!(state.title(), "It broke");
        assert!(state.report_text().contains("## Technical details"));
        assert!(state.report_text().contains("(not given)"));
        state.include_details = false;
        assert!(!state.report_text().contains("Technical details"));
        assert!(!state.report_text().contains(env!("CARGO_PKG_VERSION")));
        assert!(state.issue_url().contains("title=It%20broke&body="));
    }

    #[test]
    fn crash_note_is_stripped_and_prompts() {
        let mut editor = test_editor();
        editor.set_crash_note(Some("boom at /Users/x/src/a.rs:1".into()));
        assert!(editor.ui.report.crash_prompt);
        assert_eq!(
            editor.ui.report.crash_note.as_deref(),
            Some("boom at <path>")
        );
        let mut requests = Vec::new();
        editor.report_crash(&mut requests);
        assert!(editor.ui.report.open && !editor.ui.report.crash_prompt);
        assert!(editor.ui.report.crash_note.is_some());
        editor.dismiss_crash(&mut requests);
        assert!(editor.ui.report.crash_note.is_none());
        assert_eq!(requests.len(), 2);
    }

    #[test]
    fn set_status_feeds_the_ring_buffer() {
        let mut editor = test_editor();
        editor.set_status("Hello");
        assert_eq!(
            editor.ui.report.messages.back().map(String::as_str),
            Some("Hello")
        );
    }
}
