//! Standard-neutral question interaction records.
//!
//! A [`InteractionRecord`] is what one graded question answer looks like to an
//! LMS, without any SCORM or xAPI vocabulary. The SCORM bridge formats it per
//! standard; later standards reuse the same record.

use coursecraft_course::{Course, Question, QuestionKind};
use coursecraft_logic::{LearnerResponse, PlayerState};

/// The interaction type vocabulary shared by the supported standards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractionKind {
    TrueFalse,
    Choice,
    FillIn,
    Matching,
    Sequencing,
    Likert,
    Other,
}

impl InteractionKind {
    /// The SCORM interaction type token (`true-false`, `choice`, …).
    #[must_use]
    pub fn as_cmi_type(self) -> &'static str {
        match self {
            Self::TrueFalse => "true-false",
            Self::Choice => "choice",
            Self::FillIn => "fill-in",
            Self::Matching => "matching",
            Self::Sequencing => "sequencing",
            Self::Likert => "likert",
            Self::Other => "other",
        }
    }
}

/// A learner's answer or one fully correct answer, independent of dialect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Response {
    Bool(bool),
    Ids(Vec<String>),
    Text(String),
    Pairs(Vec<(String, String)>),
}

/// One graded question answer, ready for an LMS-specific formatter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InteractionRecord {
    /// Sanitized slide id plus `_a` and the attempt number.
    pub id: String,
    /// The sanitized slide id, stable across attempts.
    pub question_id: String,
    /// How many graded attempts this answer is, starting at 1.
    pub attempt: u32,
    pub kind: InteractionKind,
    /// The question prompt.
    pub description: String,
    pub learner: Response,
    /// Each entry is one fully correct answer.
    pub correct: Vec<Response>,
    pub case_sensitive: bool,
    pub is_correct: bool,
    /// The question's point value.
    pub weighting: u32,
    /// The points the learner earned.
    pub awarded: u32,
    /// Whether this is an ungraded survey answer (Likert, short answer or essay).
    pub survey: bool,
}

/// Build the interaction record for a graded answer, or `None` when there is
/// no such question or no submitted response in `state`.
#[must_use]
pub fn interaction_record(
    course: &Course,
    slide: &str,
    state: &PlayerState,
) -> Option<InteractionRecord> {
    let slide = course.play_slide(slide)?;
    let question = slide.question.as_ref()?;
    let learner = state.last_response.as_ref()?;
    let answer = state.answers.get(&slide.id)?;

    Some(InteractionRecord {
        id: format!("{}_a{}", sanitize_identifier(&slide.id), answer.attempts),
        question_id: sanitize_identifier(&slide.id),
        attempt: answer.attempts,
        kind: kind_of(question.kind),
        description: question.prompt.clone(),
        learner: response_of(question, learner),
        correct: correct_responses(question),
        case_sensitive: question.case_sensitive,
        is_correct: answer.correct,
        weighting: question.max_points(),
        awarded: answer.points,
        survey: question.is_survey(),
    })
}

/// Keep only `[A-Za-z0-9_:-]`, replacing anything else with `_`.
#[must_use]
pub fn sanitize_identifier(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '_' | ':' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Format a response with the SCORM 2004 xAPI interaction vocabulary:
/// `true`/`false`, `[,]` and `[.]` separators, and a `{case_matters=true}`
/// prefix on a case-sensitive correct text pattern.
///
/// This is the same vocabulary xAPI `answered` statements use; the SCORM
/// bridge's own formatter is kept byte-for-byte unchanged.
#[must_use]
pub(crate) fn format_response_2004(
    response: &Response,
    correct: bool,
    case_sensitive: bool,
) -> String {
    match response {
        Response::Bool(value) => {
            if *value {
                "true".to_owned()
            } else {
                "false".to_owned()
            }
        }
        Response::Ids(ids) => ids
            .iter()
            .map(|id| sanitize_identifier(id))
            .collect::<Vec<_>>()
            .join("[,]"),
        Response::Pairs(pairs) => pairs
            .iter()
            .map(|(left, right)| {
                format!(
                    "{}[.]{}",
                    sanitize_identifier(left),
                    sanitize_identifier(right)
                )
            })
            .collect::<Vec<_>>()
            .join("[,]"),
        Response::Text(text) => {
            let text: String = text.chars().filter(|c| !c.is_control()).collect();
            let mut value = text.replace("[,]", ",").replace("[.]", ".");
            if correct && case_sensitive {
                value = format!("{{case_matters=true}}{value}");
            }
            value.chars().take(250).collect()
        }
    }
}

fn kind_of(kind: QuestionKind) -> InteractionKind {
    match kind {
        QuestionKind::TrueFalse => InteractionKind::TrueFalse,
        QuestionKind::MultipleChoice | QuestionKind::MultipleResponse | QuestionKind::Hotspot => {
            InteractionKind::Choice
        }
        QuestionKind::Likert => InteractionKind::Likert,
        QuestionKind::FillIn | QuestionKind::ShortAnswer | QuestionKind::Essay => {
            InteractionKind::FillIn
        }
        QuestionKind::Sequence => InteractionKind::Sequencing,
        QuestionKind::Matching | QuestionKind::DragDrop => InteractionKind::Matching,
        QuestionKind::Freeform => InteractionKind::Other,
    }
}

/// The boolean a true/false choice stands for: the choice with id `true` is
/// true when there is one, otherwise the first choice is.
fn true_false_value(question: &Question, id: &str) -> bool {
    if question.choices.iter().any(|c| c.id == "true") {
        id == "true"
    } else {
        question.choices.first().is_some_and(|c| c.id == id)
    }
}

fn response_of(question: &Question, response: &LearnerResponse) -> Response {
    match response {
        LearnerResponse::Choices(ids) if question.kind == QuestionKind::TrueFalse => {
            Response::Bool(ids.first().is_some_and(|id| true_false_value(question, id)))
        }
        LearnerResponse::Choices(ids) | LearnerResponse::Sequence(ids) => {
            Response::Ids(ids.clone())
        }
        LearnerResponse::Text(text) => Response::Text(text.clone()),
        LearnerResponse::Pairs(pairs) => Response::Pairs(pairs.clone()),
        LearnerResponse::Regions(ids) => Response::Ids(ids.clone()),
    }
}

fn correct_responses(question: &Question) -> Vec<Response> {
    match question.kind {
        QuestionKind::TrueFalse => {
            let value = question
                .choices
                .iter()
                .find(|c| c.correct)
                .is_some_and(|c| true_false_value(question, &c.id));
            vec![Response::Bool(value)]
        }
        QuestionKind::MultipleChoice => question
            .choices
            .iter()
            .filter(|c| c.correct)
            .map(|c| Response::Ids(vec![c.id.clone()]))
            .collect(),
        QuestionKind::MultipleResponse => vec![Response::Ids(
            question
                .choices
                .iter()
                .filter(|c| c.correct)
                .map(|c| c.id.clone())
                .collect(),
        )],
        QuestionKind::FillIn => question
            .accepted
            .iter()
            .map(|answer| Response::Text(answer.trim().to_owned()))
            .collect(),
        QuestionKind::Sequence => vec![Response::Ids(
            question.choices.iter().map(|c| c.id.clone()).collect(),
        )],
        QuestionKind::Matching => vec![Response::Pairs(
            question
                .pairs
                .iter()
                .map(|p| (p.id.clone(), p.id.clone()))
                .collect(),
        )],
        QuestionKind::Hotspot => {
            let correct: Vec<String> = question
                .regions
                .iter()
                .filter(|r| r.correct)
                .map(|r| r.id.clone())
                .collect();
            if question.markers == 1 {
                correct
                    .into_iter()
                    .map(|id| Response::Ids(vec![id]))
                    .collect()
            } else {
                vec![Response::Ids(correct)]
            }
        }
        QuestionKind::DragDrop => vec![Response::Pairs(
            question
                .drag_items
                .iter()
                .map(|d| (d.item.clone(), d.target.clone()))
                .collect(),
        )],
        QuestionKind::Likert | QuestionKind::ShortAnswer | QuestionKind::Essay => vec![],
        QuestionKind::Freeform => vec![Response::Text("correct".to_owned())],
    }
}
