//! Markdown outline → slide outline (§ M9 step 1).
//!
//! An *outline* is the neutral, pure-data shape a Markdown document is read
//! into before the engine turns it into slides. Parsing and checking live here
//! so the same rules apply to the editor dialog and to `course.import_outline`.
//! Nothing in this module looks at the network or at an AI model: one line of
//! text in, one outline out.
//!
//! The Markdown dialect (see `docs/automation.md`):
//!
//! | Line            | Meaning                                   |
//! |-----------------|-------------------------------------------|
//! | `# Title`       | scene                                     |
//! | `## Title`      | slide                                     |
//! | `- text`        | body line                                 |
//! | `> text`        | speaker notes (joined by newlines)        |
//! | `? prompt`      | starts the slide's question               |
//! | `[x] text`      | choice, correct                           |
//! | `[ ] text`      | choice, wrong                             |
//! | anything else   | body line (a plain paragraph line)        |

use serde::{Deserialize, Serialize};

/// Most slides one outline may import.
pub const MAX_OUTLINE_SLIDES: usize = 500;
/// Most body lines one slide may carry.
pub const MAX_BODY_LINES: usize = 50;
/// Most choices one question may carry.
pub const MAX_CHOICES: usize = 20;
/// Longest single line, in bytes.
pub const MAX_LINE_BYTES: usize = 4000;

/// A whole outline: an optional course title and the scenes that follow.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CourseOutline {
    /// Course title, set from the first `# ` line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default)]
    pub scenes: Vec<OutlineScene>,
}

/// One scene and its slides.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutlineScene {
    pub title: String,
    #[serde(default)]
    pub slides: Vec<OutlineSlide>,
}

/// One slide: a title, body lines, optional notes and optional question.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutlineSlide {
    pub title: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub body: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question: Option<OutlineQuestion>,
}

/// A multiple-choice question with exactly one correct choice.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutlineQuestion {
    #[serde(default)]
    pub prompt: String,
    #[serde(default)]
    pub choices: Vec<OutlineChoice>,
}

/// One answer choice.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutlineChoice {
    pub text: String,
    #[serde(default)]
    pub correct: bool,
}

/// One thing wrong with an outline.
///
/// `line` is 1-based in the Markdown the outline was parsed from, and `0` when
/// the problem came from [`CourseOutline::check`] on a hand-built or JSON document
/// where there is no source text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutlineProblem {
    pub line: usize,
    pub message: String,
}

impl OutlineProblem {
    fn new(line: usize, message: impl Into<String>) -> Self {
        Self {
            line,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for OutlineProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.line == 0 {
            f.write_str(&self.message)
        } else {
            write!(f, "line {}: {}", self.line, self.message)
        }
    }
}

/// The parser's working state while it walks the Markdown.
struct Parser {
    outline: CourseOutline,
    /// True once a scene exists (so later `#` lines can open another scene).
    seen_scene: bool,
    /// Problems found so far, in line order.
    problems: Vec<OutlineProblem>,
    /// Index of the slide body/notes/choices attach to, if one is open.
    slide: Option<usize>,
    /// True while the open slide's question is still accepting choices.
    in_question: bool,
    /// The `?` line of every slide in creation order, so a question that breaks
    /// the two-choices/one-correct rule names the line it started on.
    question_lines: Vec<Option<usize>>,
}

impl Parser {
    fn new() -> Self {
        Self {
            outline: CourseOutline::default(),
            seen_scene: false,
            problems: Vec::new(),
            slide: None,
            in_question: false,
            question_lines: Vec::new(),
        }
    }

    fn push_scene(&mut self, title: String) {
        self.outline.scenes.push(OutlineScene {
            title,
            slides: Vec::new(),
        });
        self.seen_scene = true;
        self.slide = None;
        self.in_question = false;
    }

    /// Index of the open slide, or a problem about needing one.
    fn open_slide(&mut self, line: usize, what: &str) -> Option<usize> {
        if let Some(i) = self.slide {
            return Some(i);
        }
        self.problems.push(OutlineProblem::new(
            line,
            format!("{what} needs a slide above it; start the slide with `## Title`."),
        ));
        None
    }

    fn open_question(&mut self) -> Option<&mut OutlineQuestion> {
        let i = self.slide?;
        if !self.in_question {
            return None;
        }
        self.outline
            .scenes
            .last_mut()?
            .slides
            .get_mut(i)?
            .question
            .as_mut()
    }

    fn line(&mut self, number: usize, raw: &str) {
        let line = raw.trim_end_matches('\r').trim();
        if line.is_empty() {
            return;
        }
        if line.len() > MAX_LINE_BYTES {
            self.problems.push(OutlineProblem::new(
                number,
                format!(
                    "This line is {} bytes long; keep each line under {MAX_LINE_BYTES}.",
                    line.len()
                ),
            ));
            return;
        }
        if let Some(text) = line.strip_prefix("## ") {
            self.heading(number, text);
            return;
        }
        if let Some(text) = line.strip_prefix("# ") {
            self.scene(number, text);
        } else if let Some(text) = line.strip_prefix("> ") {
            self.notes(number, text);
        } else if let Some(text) = line.strip_prefix("? ") {
            self.question(number, text);
        } else if let Some((correct, text)) = choice(line) {
            self.choice(number, correct, text);
        } else if let Some(text) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")) {
            self.body(number, text);
        } else {
            self.body(number, line);
        }
    }

    fn scene(&mut self, number: usize, text: &str) {
        let title = text.trim().to_owned();
        if title.is_empty() {
            self.problems
                .push(OutlineProblem::new(number, "This scene needs a title."));
            self.slide = None;
            self.in_question = false;
            return;
        }
        if !self.seen_scene {
            self.outline.title = Some(title.clone());
        }
        self.push_scene(title);
    }

    fn heading(&mut self, number: usize, text: &str) {
        let title = text.trim().to_owned();
        if title.is_empty() {
            self.problems
                .push(OutlineProblem::new(number, "This slide needs a title."));
            self.slide = None;
            self.in_question = false;
            return;
        }
        if !self.seen_scene {
            // Slides before any scene get a home named after the course title,
            // else `Scene 1`. A later `# ` line still opens its own scene.
            let name = self
                .outline
                .title
                .clone()
                .unwrap_or_else(|| "Scene 1".to_owned());
            self.push_scene(name);
        }
        self.in_question = false;
        let Some(scene) = self.outline.scenes.last_mut() else {
            return;
        };
        scene.slides.push(OutlineSlide {
            title,
            ..OutlineSlide::default()
        });
        self.slide = Some(scene.slides.len() - 1);
        self.question_lines.push(None);
    }

    fn body(&mut self, number: usize, text: &str) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        if self.in_question {
            self.problems.push(OutlineProblem::new(
                number,
                "This line is inside a question; write another `[x]` or `[ ]` choice, or start a new `## Slide`.",
            ));
            return;
        }
        let Some(i) = self.open_slide(number, "This body line") else {
            return;
        };
        let Some(slide) = self.slide_mut(i) else {
            return;
        };
        if slide.body.len() >= MAX_BODY_LINES {
            self.problems.push(OutlineProblem::new(
                number,
                format!("This slide has more than {MAX_BODY_LINES} body lines."),
            ));
            return;
        }
        slide.body.push(text.to_owned());
    }

    fn notes(&mut self, number: usize, text: &str) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        let Some(i) = self.open_slide(number, "A notes line") else {
            return;
        };
        let Some(slide) = self.slide_mut(i) else {
            return;
        };
        match slide.notes.as_mut() {
            Some(notes) => {
                notes.push('\n');
                notes.push_str(text);
            }
            None => slide.notes = Some(text.to_owned()),
        }
    }

    fn question(&mut self, number: usize, text: &str) {
        let Some(i) = self.open_slide(number, "This question") else {
            return;
        };
        let Some(slide) = self.slide_mut(i) else {
            return;
        };
        if slide.question.is_some() {
            self.problems.push(OutlineProblem::new(
                number,
                "This slide already has a question; a slide can hold only one.",
            ));
            // Choices after the second `?` belong to neither question.
            self.in_question = false;
            return;
        }
        slide.question = Some(OutlineQuestion {
            prompt: text.trim().to_owned(),
            choices: Vec::new(),
        });
        if let Some(line) = self.question_lines.last_mut() {
            *line = Some(number);
        }
        self.in_question = true;
    }

    fn choice(&mut self, number: usize, correct: bool, text: &str) {
        let text = text.trim();
        if text.is_empty() {
            self.problems.push(OutlineProblem::new(
                number,
                "This choice is empty; write the answer text after `[x] ` or `[ ] `.",
            ));
            return;
        }
        if self.slide.is_none() || !self.in_question {
            let what = if self.slide.is_some() {
                "This choice is not inside a question; start one with `? Question`."
            } else {
                "This choice needs a slide above it; start the slide with `## Title`."
            };
            self.problems
                .push(OutlineProblem::new(number, what.to_owned()));
            return;
        }
        match self.open_question() {
            None => {}
            Some(q) if q.choices.len() >= MAX_CHOICES => self.problems.push(OutlineProblem::new(
                number,
                format!("This question has more than {MAX_CHOICES} choices."),
            )),
            Some(q) => q.choices.push(OutlineChoice {
                text: text.to_owned(),
                correct,
            }),
        }
    }

    fn slide_mut(&mut self, i: usize) -> Option<&mut OutlineSlide> {
        self.outline.scenes.last_mut()?.slides.get_mut(i)
    }
}

/// `[x] text` / `[ ] text` → (correct, text).
fn choice(line: &str) -> Option<(bool, &str)> {
    let rest = line
        .strip_prefix("[x] ")
        .or_else(|| line.strip_prefix("[X] "));
    if let Some(text) = rest {
        return Some((true, text));
    }
    line.strip_prefix("[ ] ").map(|text| (false, text))
}

/// Read Markdown into an [`CourseOutline`].
///
/// Collects every problem rather than stopping at the first, so the editor can
/// show all of them at once. An empty result list means the outline is usable;
/// callers still run [`CourseOutline::check`] because limits and question rules are
/// also enforced there.
pub fn outline_from_markdown(text: &str) -> Result<CourseOutline, Vec<OutlineProblem>> {
    let mut parser = Parser::new();
    for (i, raw) in text.lines().enumerate() {
        parser.line(i + 1, raw);
    }
    let outline = parser.outline;
    let question_lines = parser.question_lines;
    let mut problems = parser.problems;
    if outline.scenes.is_empty() {
        problems.push(OutlineProblem::new(
            0,
            "There is nothing to import; start with `# Scene` and `## Slide`.",
        ));
    }
    problems.extend(outline.check_with_question_lines(|slide| {
        question_lines.get(slide).copied().flatten().unwrap_or(0)
    }));
    problems.sort_by_key(|p| (p.line == 0, p.line));
    problems.dedup();
    if problems.is_empty() {
        Ok(outline)
    } else {
        Err(problems)
    }
}

impl CourseOutline {
    /// Every slide across all scenes.
    pub fn slides(&self) -> impl Iterator<Item = &OutlineSlide> {
        self.scenes.iter().flat_map(|s| s.slides.iter())
    }

    /// Count of slides that carry a question.
    #[must_use]
    pub fn question_count(&self) -> usize {
        self.slides().filter(|s| s.question.is_some()).count()
    }

    /// Check a hand-built or JSON outline against the same rules the parser
    /// applies. Problems report `line: 0` because there is no source text.
    #[must_use]
    pub fn check(&self) -> Vec<OutlineProblem> {
        self.check_with_question_lines(|_| 0)
    }

    /// Like [`CourseOutline::check`], but a question rule names the `?` line when the
    /// caller knows it (the Markdown parser does).
    fn check_with_question_lines(
        &self,
        question_line: impl Fn(usize) -> usize,
    ) -> Vec<OutlineProblem> {
        let mut out = Vec::new();
        let mut slide_count = 0usize;
        if let Some(title) = &self.title
            && title.len() > MAX_LINE_BYTES
        {
            out.push(OutlineProblem::new(0, "The outline title is too long."));
        }
        for scene in &self.scenes {
            if scene.title.trim().is_empty() {
                out.push(OutlineProblem::new(0, "A scene has no title."));
            } else if scene.title.len() > MAX_LINE_BYTES {
                out.push(OutlineProblem::new(0, "A scene title is too long."));
            }
            for slide in &scene.slides {
                check_slide(slide, question_line(slide_count), &mut out);
                slide_count += 1;
            }
        }
        if self.scenes.is_empty() {
            out.push(OutlineProblem::new(0, "The outline has no scenes."));
        }
        if slide_count > MAX_OUTLINE_SLIDES {
            out.push(OutlineProblem::new(
                0,
                format!("The outline has {slide_count} slides; the limit is {MAX_OUTLINE_SLIDES}."),
            ));
        }
        out
    }
}

fn check_slide(slide: &OutlineSlide, question_line: usize, out: &mut Vec<OutlineProblem>) {
    if slide.title.trim().is_empty() {
        out.push(OutlineProblem::new(0, "A slide has no title."));
    } else if slide.title.len() > MAX_LINE_BYTES {
        out.push(OutlineProblem::new(0, "A slide title is too long."));
    }
    if slide.body.len() > MAX_BODY_LINES {
        out.push(OutlineProblem::new(
            0,
            format!(
                "Slide \"{}\" has {} body lines; the limit is {MAX_BODY_LINES}.",
                slide.title,
                slide.body.len()
            ),
        ));
    }
    for line in &slide.body {
        if line.len() > MAX_LINE_BYTES {
            out.push(OutlineProblem::new(
                0,
                format!(
                    "Slide \"{}\" has a body line that is too long.",
                    slide.title
                ),
            ));
            break;
        }
    }
    if let Some(notes) = slide.notes.as_deref()
        && notes.lines().any(|line| line.len() > MAX_LINE_BYTES)
    {
        out.push(OutlineProblem::new(
            0,
            format!("Slide \"{}\" has notes that are too long.", slide.title),
        ));
    }
    let Some(q) = &slide.question else {
        return;
    };
    if q.prompt.trim().is_empty() {
        out.push(OutlineProblem::new(
            question_line,
            format!("The question on slide \"{}\" has no prompt.", slide.title),
        ));
    } else if q.prompt.len() > MAX_LINE_BYTES {
        out.push(OutlineProblem::new(
            question_line,
            format!(
                "The question on slide \"{}\" has a prompt that is too long.",
                slide.title
            ),
        ));
    }
    if q.choices.len() > MAX_CHOICES {
        out.push(OutlineProblem::new(
            question_line,
            format!(
                "The question on slide \"{}\" has {} choices; the limit is {MAX_CHOICES}.",
                slide.title,
                q.choices.len()
            ),
        ));
    }
    let correct = q.choices.iter().filter(|c| c.correct).count();
    if q.choices.len() < 2 {
        out.push(OutlineProblem::new(
            question_line,
            format!(
                "The question on slide \"{}\" needs at least 2 choices.",
                slide.title
            ),
        ));
    }
    if correct != 1 {
        out.push(OutlineProblem::new(
            question_line,
            format!(
                "The question on slide \"{}\" needs exactly one correct choice, marked `[x]`; it has {correct}.",
                slide.title
            ),
        ));
    }
    for c in &q.choices {
        if c.text.trim().is_empty() {
            out.push(OutlineProblem::new(
                question_line,
                format!(
                    "The question on slide \"{}\" has an empty choice.",
                    slide.title
                ),
            ));
            break;
        }
        if c.text.len() > MAX_LINE_BYTES {
            out.push(OutlineProblem::new(
                question_line,
                format!(
                    "The question on slide \"{}\" has a choice that is too long.",
                    slide.title
                ),
            ));
            break;
        }
    }
}
