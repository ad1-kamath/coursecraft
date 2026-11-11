//! `bank.import_csv`: quiz questions from a spreadsheet.
//!
//! The reader is a small RFC 4180 one (quoted fields, doubled quotes, commas
//! and line breaks inside quotes, a UTF-8 byte order mark, `\r\n` or `\n`).
//! Every row is checked first; if any row is wrong nothing is imported and the
//! error lists every problem with its row number.

use coursecraft_course::{Choice, Question, QuestionKind, Slide};
use serde_json::{Value as Json, json};

use super::{bad, bank_create, keep_valid, new_bank_slide, opt, req};
use crate::{EngineError, Result, Session};

/// Largest CSV text accepted.
pub const MAX_CSV_BYTES: usize = 5 * 1024 * 1024;
/// Most question rows accepted.
pub const MAX_CSV_ROWS: usize = 2_000;
const MAX_CHOICES: usize = 10;
const MAX_ACCEPTED: usize = 50;
const MAX_FIELD_BYTES: usize = 4096;
const MAX_POINTS: u32 = 10_000;
const DEFAULT_POINTS: u32 = 10;
const MAX_PROBLEMS: usize = 100;

/// Split RFC 4180 text into records of fields. Blank lines are skipped.
///
/// # Errors
/// A sentence when the text ends inside a quoted field or has too many rows.
pub fn parse_csv(text: &str) -> std::result::Result<Vec<Vec<String>>, String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut records: Vec<Vec<String>> = Vec::new();
    let mut record: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut field_started = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    quoted = false;
                }
            } else {
                field.push(c);
            }
            continue;
        }
        match c {
            '"' if !field_started => {
                quoted = true;
                field_started = true;
            }
            ',' => {
                record.push(std::mem::take(&mut field));
                field_started = false;
            }
            '\r' | '\n' => {
                if c == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                finish_record(&mut records, &mut record, &mut field, &mut field_started)?;
            }
            other => {
                field.push(other);
                field_started = true;
            }
        }
    }
    if quoted {
        return Err("the file ends inside a quoted field (a quote was never closed)".to_owned());
    }
    finish_record(&mut records, &mut record, &mut field, &mut field_started)?;
    Ok(records)
}

fn finish_record(
    records: &mut Vec<Vec<String>>,
    record: &mut Vec<String>,
    field: &mut String,
    field_started: &mut bool,
) -> std::result::Result<(), String> {
    let blank = record.is_empty() && field.is_empty() && !*field_started;
    if !blank {
        record.push(std::mem::take(field));
        records.push(std::mem::take(record));
        if records.len() > MAX_CSV_ROWS + 1 {
            return Err(format!(
                "the file has more than {MAX_CSV_ROWS} question rows; split it into smaller files"
            ));
        }
    }
    field.clear();
    record.clear();
    *field_started = false;
    Ok(())
}

/// One checked row, ready to become a question slide.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedQuestion {
    pub kind: QuestionKind,
    pub prompt: String,
    pub choices: Vec<Choice>,
    pub accepted: Vec<String>,
    pub points: u32,
    pub feedback_correct: Option<String>,
    pub feedback_incorrect: Option<String>,
}

struct Columns {
    kind: usize,
    question: usize,
    correct: usize,
    choices: Vec<Option<usize>>,
    feedback_correct: Option<usize>,
    feedback_incorrect: Option<usize>,
    points: Option<usize>,
}

fn find_column(header: &[String], name: &str) -> Option<usize> {
    header
        .iter()
        .position(|h| h.trim().eq_ignore_ascii_case(name))
}

fn read_header(header: &[String]) -> std::result::Result<Columns, Vec<String>> {
    let mut missing = Vec::new();
    let mut need = |name: &str| {
        let found = find_column(header, name);
        if found.is_none() {
            missing.push(format!("the header row has no `{name}` column"));
        }
        found.unwrap_or(0)
    };
    let kind = need("type");
    let question = need("question");
    let correct = need("correct");
    if !missing.is_empty() {
        return Err(missing);
    }
    Ok(Columns {
        kind,
        question,
        correct,
        choices: (1..=MAX_CHOICES)
            .map(|n| find_column(header, &format!("choice{n}")))
            .collect(),
        feedback_correct: find_column(header, "feedback_correct"),
        feedback_incorrect: find_column(header, "feedback_incorrect"),
        points: find_column(header, "points"),
    })
}

fn cell(row: &[String], index: usize) -> &str {
    row.get(index).map_or("", |s| s.trim())
}

fn kind_of(raw: &str) -> Option<QuestionKind> {
    let key: String = raw
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c == '-' || c == ' ' { '_' } else { c })
        .collect();
    match key.as_str() {
        "multiple_choice" => Some(QuestionKind::MultipleChoice),
        "multiple_response" => Some(QuestionKind::MultipleResponse),
        "true_false" => Some(QuestionKind::TrueFalse),
        "fill_in" => Some(QuestionKind::FillIn),
        _ => None,
    }
}

fn check_len(row: usize, what: &str, text: &str, problems: &mut Vec<String>) {
    if text.len() > MAX_FIELD_BYTES {
        problems.push(format!(
            "row {row}: the {what} is longer than {MAX_FIELD_BYTES} bytes"
        ));
    }
}

fn choice_letter(index: usize) -> String {
    let letter = u8::try_from(index)
        .ok()
        .and_then(|i| b'a'.checked_add(i))
        .map_or('z', char::from);
    letter.to_string()
}

fn row_choices(
    cols: &Columns,
    cells: &[String],
    row: usize,
    problems: &mut Vec<String>,
) -> Vec<String> {
    let texts: Vec<&str> = cols
        .choices
        .iter()
        .map(|c| c.map_or("", |i| cell(cells, i)))
        .collect();
    let last = texts.iter().rposition(|t| !t.is_empty());
    let Some(last) = last else {
        return Vec::new();
    };
    for (i, text) in texts.iter().enumerate().take(last) {
        if text.is_empty() {
            problems.push(format!(
                "row {row}: choice{} is empty but choice{} is filled",
                i + 1,
                last + 1
            ));
            return Vec::new();
        }
    }
    texts
        .iter()
        .take(last + 1)
        .map(|t| (*t).to_owned())
        .collect()
}

fn choice_numbers(
    row: usize,
    raw: &str,
    count: usize,
    problems: &mut Vec<String>,
) -> Option<Vec<usize>> {
    let mut numbers: Vec<usize> = Vec::new();
    for part in raw.split([';', ',']) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let Ok(n) = part.parse::<usize>() else {
            problems.push(format!(
                "row {row}: correct answer {part:?} is not a choice number"
            ));
            return None;
        };
        if n == 0 || n > count {
            problems.push(format!(
                "row {row}: correct answer {n} but only {count} choice{}",
                if count == 1 { "" } else { "s" }
            ));
            return None;
        }
        if !numbers.contains(&n) {
            numbers.push(n);
        }
    }
    Some(numbers)
}

fn parse_points(cols: &Columns, cells: &[String], row: usize, problems: &mut Vec<String>) -> u32 {
    let raw = cols.points.map_or("", |i| cell(cells, i));
    if raw.is_empty() {
        return DEFAULT_POINTS;
    }
    match raw.parse::<u32>() {
        Ok(p) if p <= MAX_POINTS => p,
        _ => {
            problems.push(format!(
                "row {row}: points {raw:?} must be a whole number from 0 to {MAX_POINTS}"
            ));
            DEFAULT_POINTS
        }
    }
}

fn parse_row(
    cols: &Columns,
    cells: &[String],
    row: usize,
    problems: &mut Vec<String>,
) -> Option<ParsedQuestion> {
    let before = problems.len();
    let kind_text = cell(cells, cols.kind);
    let Some(kind) = kind_of(kind_text) else {
        let shown = if kind_text.is_empty() {
            "is empty".to_owned()
        } else {
            format!("{kind_text:?} is not known")
        };
        problems.push(format!(
            "row {row}: type {shown}; use multiple_choice, multiple_response, true_false or fill_in"
        ));
        return None;
    };
    let prompt = cell(cells, cols.question).to_owned();
    if prompt.is_empty() {
        problems.push(format!("row {row}: the question is empty"));
    }
    check_len(row, "question", &prompt, problems);
    let points = parse_points(cols, cells, row, problems);
    let mut question = ParsedQuestion {
        kind,
        prompt,
        choices: Vec::new(),
        accepted: Vec::new(),
        points,
        feedback_correct: cols
            .feedback_correct
            .map(|i| cell(cells, i).to_owned())
            .filter(|t| !t.is_empty()),
        feedback_incorrect: cols
            .feedback_incorrect
            .map(|i| cell(cells, i).to_owned())
            .filter(|t| !t.is_empty()),
    };
    for text in [&question.feedback_correct, &question.feedback_incorrect]
        .into_iter()
        .flatten()
    {
        check_len(row, "feedback", text, problems);
    }
    let correct = cell(cells, cols.correct);
    match kind {
        QuestionKind::TrueFalse => true_false(&mut question, correct, row, problems),
        QuestionKind::FillIn => fill_in(&mut question, correct, row, problems),
        _ => choice_question(&mut question, cols, cells, correct, row, problems),
    }
    (problems.len() == before).then_some(question)
}

fn true_false(q: &mut ParsedQuestion, correct: &str, row: usize, problems: &mut Vec<String>) {
    let truth = match correct.to_ascii_lowercase().as_str() {
        "true" => true,
        "false" => false,
        _ => {
            problems.push(format!(
                "row {row}: a true_false question needs correct to be true or false, not {correct:?}"
            ));
            return;
        }
    };
    q.choices = vec![
        Choice {
            id: "true".to_owned(),
            text: "True".to_owned(),
            correct: truth,
            points: 0,
        },
        Choice {
            id: "false".to_owned(),
            text: "False".to_owned(),
            correct: !truth,
            points: 0,
        },
    ];
}

fn fill_in(q: &mut ParsedQuestion, correct: &str, row: usize, problems: &mut Vec<String>) {
    let accepted: Vec<String> = correct
        .split(';')
        .map(str::trim)
        .filter(|a| !a.is_empty())
        .map(str::to_owned)
        .collect();
    if accepted.is_empty() {
        problems.push(format!(
            "row {row}: a fill_in question needs at least one accepted answer in correct"
        ));
    }
    if accepted.len() > MAX_ACCEPTED {
        problems.push(format!(
            "row {row}: a fill_in question takes at most {MAX_ACCEPTED} accepted answers"
        ));
    }
    for answer in &accepted {
        check_len(row, "accepted answer", answer, problems);
    }
    q.accepted = accepted;
}

fn choice_question(
    q: &mut ParsedQuestion,
    cols: &Columns,
    cells: &[String],
    correct: &str,
    row: usize,
    problems: &mut Vec<String>,
) {
    let texts = row_choices(cols, cells, row, problems);
    if texts.len() < 2 {
        if problems
            .iter()
            .all(|p| !p.starts_with(&format!("row {row}: choice")))
        {
            problems.push(format!(
                "row {row}: needs at least 2 choices, found {}",
                texts.len()
            ));
        }
        return;
    }
    for text in &texts {
        check_len(row, "choice", text, problems);
    }
    let Some(numbers) = choice_numbers(row, correct, texts.len(), problems) else {
        return;
    };
    let wanted_one = q.kind == QuestionKind::MultipleChoice;
    if numbers.is_empty() || (wanted_one && numbers.len() != 1) {
        problems.push(if wanted_one {
            format!("row {row}: a multiple_choice question needs exactly one correct answer")
        } else {
            format!("row {row}: a multiple_response question needs at least one correct answer")
        });
        return;
    }
    q.choices = texts
        .into_iter()
        .enumerate()
        .map(|(i, text)| Choice {
            id: choice_letter(i),
            text,
            correct: numbers.contains(&(i + 1)),
            points: 0,
        })
        .collect();
}

/// Check a whole CSV text and turn it into questions.
///
/// # Errors
/// Every problem found, each starting with its row number. The header row is
/// row 1, so the first question is row 2, as a spreadsheet shows it.
pub fn parse_questions(csv: &str) -> std::result::Result<Vec<ParsedQuestion>, Vec<String>> {
    if csv.len() > MAX_CSV_BYTES {
        return Err(vec![format!(
            "the file is larger than {} MB",
            MAX_CSV_BYTES / (1024 * 1024)
        )]);
    }
    let records = parse_csv(csv).map_err(|e| vec![e])?;
    let Some((header, rows)) = records.split_first() else {
        return Err(vec!["the file is empty".to_owned()]);
    };
    let cols = read_header(header)?;
    if rows.is_empty() {
        return Err(vec![
            "the file has a header row but no questions".to_owned(),
        ]);
    }
    let mut problems = Vec::new();
    let mut questions = Vec::new();
    for (index, cells) in rows.iter().enumerate() {
        if problems.len() >= MAX_PROBLEMS {
            problems.push(format!(
                "more than {MAX_PROBLEMS} problems; fix these first"
            ));
            break;
        }
        if let Some(q) = parse_row(&cols, cells, index + 2, &mut problems) {
            questions.push(q);
        }
    }
    if problems.is_empty() {
        Ok(questions)
    } else {
        Err(problems)
    }
}

fn slide_title(prompt: &str) -> String {
    let flat: String = prompt.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut title: String = flat.chars().take(60).collect();
    if flat.chars().count() > 60 {
        title.push('…');
    }
    if title.is_empty() {
        "Question".to_owned()
    } else {
        title
    }
}

fn set_feedback(slide: &mut Slide, layer: Option<&str>, text: Option<&str>) {
    let (Some(layer), Some(text)) = (layer, text) else {
        return;
    };
    let target = slide
        .layers
        .iter_mut()
        .find(|l| l.id == layer)
        .and_then(|l| l.objects.first_mut());
    if let Some(coursecraft_course::Object {
        kind: coursecraft_course::ObjectKind::Text {
            text: shown, runs, ..
        },
        ..
    }) = target
    {
        *shown = text.to_owned();
        runs.clear();
    }
}

fn fill_question(q: &mut Question, parsed: &ParsedQuestion) {
    q.prompt.clone_from(&parsed.prompt);
    q.points = parsed.points;
    q.choices.clone_from(&parsed.choices);
    q.accepted.clone_from(&parsed.accepted);
    q.shuffle = false;
}

/// Find a bank by title (ignoring case) or make one. No name makes a new bank.
fn target_bank(s: &mut Session, name: Option<&str>) -> Result<String> {
    if let Some(name) = name {
        let wanted = name.trim();
        if let Some(bank) = s
            .course
            .banks
            .iter()
            .find(|b| b.title.trim().eq_ignore_ascii_case(wanted))
        {
            return Ok(bank.id.clone());
        }
    }
    let title = name
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .unwrap_or("Imported questions");
    let made = bank_create(s, &json!({ "title": title }))?;
    made.get("id")
        .and_then(Json::as_str)
        .map(str::to_owned)
        .ok_or_else(|| EngineError::Invalid("bank was not created".into()))
}

/// `bank.import_csv {csv, bank?}`: every row becomes a question slide in the
/// named bank (made if it does not exist) or in a new bank. All or nothing.
pub(super) fn bank_import_csv(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "bank.import_csv";
    let was_valid = s.course.is_playable();
    let csv: String = req(C, p, "csv")?;
    let name: Option<String> = opt(C, p, "bank")?;
    let parsed = parse_questions(&csv).map_err(|problems| {
        bad(
            C,
            format!(
                "{} problem{} in the CSV, nothing was imported:\n{}",
                problems.len(),
                if problems.len() == 1 { "" } else { "s" },
                problems.join("\n")
            ),
        )
    })?;
    let bank_id = target_bank(s, name.as_deref())?;
    let mut ids = Vec::with_capacity(parsed.len());
    for item in &parsed {
        let mut slide = new_bank_slide(&s.course, item.kind, slide_title(&item.prompt), &|q| {
            fill_question(q, item)
        })?;
        let (correct, incorrect) = slide
            .question
            .as_ref()
            .map(|q| (q.correct_layer.clone(), q.incorrect_layer.clone()))
            .unwrap_or_default();
        set_feedback(
            &mut slide,
            correct.as_deref(),
            item.feedback_correct.as_deref(),
        );
        set_feedback(
            &mut slide,
            incorrect.as_deref(),
            item.feedback_incorrect.as_deref(),
        );
        ids.push(slide.id.clone());
        let bank = s
            .course
            .banks
            .iter_mut()
            .find(|b| b.id == bank_id)
            .ok_or_else(|| EngineError::NotFound(format!("bank {bank_id:?} does not exist")))?;
        bank.slides.push(slide);
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "bank": bank_id, "imported": ids.len(), "slides": ids }))
}
