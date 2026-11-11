//! Tests for `bank.import_csv`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use coursecraft_course::{Course, QuestionKind, Severity};
use serde_json::json;

use crate::Session;
use crate::commands::{MAX_CSV_BYTES, MAX_CSV_ROWS, parse_csv};

const HELLO: &str = include_str!("../../../examples/hello-course/course.json");
const TEMPLATE: &str = include_str!("../../../examples/questions.csv");

fn session() -> Session {
    Session::new(Course::from_json(HELLO).unwrap())
}

fn import(s: &mut Session, csv: &str) -> Result<serde_json::Value, String> {
    s.execute("bank.import_csv", &json!({"csv": csv}))
        .map_err(|e| e.to_string())
}

#[test]
fn the_template_imports_one_question_of_each_type() {
    let mut s = session();
    let before = s.undo_len();
    let result = import(&mut s, TEMPLATE).unwrap();
    assert_eq!(result["imported"], 4);
    assert_eq!(s.undo_len(), before + 1, "one undo step");
    let bank = s.course.banks.last().unwrap();
    assert_eq!(bank.slides.len(), 4);
    let kinds: Vec<QuestionKind> = bank
        .slides
        .iter()
        .map(|sl| sl.question.as_ref().unwrap().kind)
        .collect();
    assert_eq!(
        kinds,
        [
            QuestionKind::MultipleChoice,
            QuestionKind::MultipleResponse,
            QuestionKind::TrueFalse,
            QuestionKind::FillIn
        ]
    );
    let q = |i: usize| bank.slides[i].question.as_ref().unwrap();
    assert_eq!(q(0).choices.len(), 4);
    assert!(q(0).choices[0].correct && !q(0).choices[1].correct);
    assert_eq!(q(1).choices.iter().filter(|c| c.correct).count(), 2, "1;3");
    assert!(q(2).prompt.contains("goggles"), "quoted comma kept");
    assert_eq!(q(3).accepted, ["112", "one one two"]);
    assert_eq!(q(1).points, 20);
    assert!(
        !s.course
            .validate()
            .iter()
            .any(|i| i.severity == Severity::Error)
    );
    s.undo().unwrap();
    assert_eq!(
        s.course.banks.len(),
        Course::from_json(HELLO).unwrap().banks.len()
    );
}

#[test]
fn a_named_bank_is_reused_by_title() {
    let mut s = session();
    let params = json!({"csv": TEMPLATE, "bank": "Quiz A"});
    s.execute("bank.import_csv", &params).unwrap();
    s.execute("bank.import_csv", &params).unwrap();
    let banks: Vec<_> = s
        .course
        .banks
        .iter()
        .filter(|b| b.title == "Quiz A")
        .collect();
    assert_eq!(banks.len(), 1);
    assert_eq!(banks[0].slides.len(), 8);
}

#[test]
fn every_problem_is_listed_with_its_row_and_nothing_is_imported() {
    let mut s = session();
    let before = s.course.clone();
    let csv = "type,question,choice1,choice2,choice3,choice4,correct\r\n\
        multiple_choice,Fine?,a,b,c,d,2\r\n\
        multiple_choice,Bad?,a,b,c,d,5\r\n\
        true_false,Maybe?,,,,,perhaps\r\n\
        essay,What?,,,,,\r\n\
        fill_in,,,,,,x\r\n\
        multiple_choice,One choice,a,,,,1\r\n";
    let err = import(&mut s, csv).unwrap_err();
    assert!(
        err.contains("row 3: correct answer 5 but only 4 choices"),
        "{err}"
    );
    assert!(err.contains("row 4:"), "{err}");
    assert!(err.contains("row 5: type \"essay\""), "{err}");
    assert!(err.contains("row 6: the question is empty"), "{err}");
    assert!(err.contains("row 7: needs at least 2 choices"), "{err}");
    assert_eq!(s.course, before, "all or nothing");
}

#[test]
fn the_reader_handles_rfc_4180() {
    let text = "\u{feff}a,b,c\r\n\"x, y\",\"he said \"\"hi\"\"\",\"line\nbreak\"\n\n1,2,3";
    let rows = parse_csv(text).unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0], ["a", "b", "c"]);
    assert_eq!(rows[1], ["x, y", "he said \"hi\"", "line\nbreak"]);
    assert_eq!(rows[2], ["1", "2", "3"]);
    assert!(parse_csv("a,\"open").is_err());
}

#[test]
fn headers_are_case_insensitive_and_order_free() {
    let mut s = session();
    let csv = "Correct,QUESTION,Type,Choice2,Choice1\ntrue,Is it?,True_False,,\n";
    import(&mut s, csv).unwrap();
}

#[test]
fn hostile_input_is_capped() {
    let mut s = session();
    let big = "x".repeat(MAX_CSV_BYTES + 1);
    assert!(import(&mut s, &big).unwrap_err().contains("larger than"));
    let mut rows = String::from("type,question,correct\n");
    for _ in 0..=MAX_CSV_ROWS {
        rows.push_str("true_false,Q?,true\n");
    }
    assert!(import(&mut s, &rows).unwrap_err().contains("more than"));
    assert!(
        import(&mut s, "type,question\nfill_in,Q")
            .unwrap_err()
            .contains("`correct`")
    );
    assert!(import(&mut s, "").unwrap_err().contains("empty"));
}
