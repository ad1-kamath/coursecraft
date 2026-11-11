//! Tests for turning effects into LTI score reports.

use coursecraft_course::CompletionStatus;
use coursecraft_logic::Effect;

use crate::lti::{LtiReporter, ScoreReport};

fn score(percent: u8, status: CompletionStatus) -> Effect {
    Effect::Score {
        raw: u32::from(percent),
        max: 100,
        percent,
        status,
    }
}

fn completion(status: CompletionStatus) -> Effect {
    Effect::Completion { status }
}

#[test]
fn a_score_effect_becomes_a_reported_percentage() {
    let mut reporter = LtiReporter::new();
    reporter.record(&[score(87, CompletionStatus::Passed)]);

    assert_eq!(
        reporter.take_pending(),
        Some(ScoreReport {
            percent: Some(87),
            status: "passed",
        })
    );
    assert!(!reporter.has_pending());
}

#[test]
fn a_completion_without_a_score_reports_progress_only() {
    let mut reporter = LtiReporter::new();
    reporter.record(&[completion(CompletionStatus::Completed)]);

    assert_eq!(
        reporter.take_pending(),
        Some(ScoreReport {
            percent: None,
            status: "completed",
        })
    );
}

#[test]
fn score_and_completion_in_one_batch_are_one_score_report() {
    let mut reporter = LtiReporter::new();
    reporter.record(&[
        score(55, CompletionStatus::Failed),
        completion(CompletionStatus::Failed),
    ]);

    assert_eq!(
        reporter.take_pending(),
        Some(ScoreReport {
            percent: Some(55),
            status: "failed",
        })
    );
    assert!(!reporter.has_pending());
}

#[test]
fn only_the_latest_unsent_report_is_kept() {
    let mut reporter = LtiReporter::new();
    reporter.record(&[completion(CompletionStatus::Incomplete)]);
    reporter.record(&[completion(CompletionStatus::Completed)]);

    assert_eq!(
        reporter.take_pending(),
        Some(ScoreReport {
            percent: None,
            status: "completed",
        })
    );
    assert!(!reporter.has_pending());
}

#[test]
fn an_identical_report_after_one_was_sent_is_deduplicated() {
    let mut reporter = LtiReporter::new();
    reporter.record(&[score(70, CompletionStatus::Passed)]);
    assert!(reporter.take_pending().is_some());
    reporter.mark_sent();

    reporter.record(&[score(70, CompletionStatus::Passed)]);
    assert_eq!(reporter.take_pending(), None);
}

#[test]
fn a_bare_completion_cannot_replace_a_sent_final_score() {
    let mut reporter = LtiReporter::new();
    reporter.record(&[score(70, CompletionStatus::Passed)]);
    assert!(reporter.take_pending().is_some());
    reporter.mark_sent();

    reporter.record(&[completion(CompletionStatus::Completed)]);
    assert_eq!(reporter.take_pending(), None);
}

#[test]
fn a_failure_is_retried_once_then_given_up_as_lost() {
    let mut reporter = LtiReporter::new();
    reporter.record(&[score(42, CompletionStatus::Failed)]);

    let first = reporter.take_pending().expect("first attempt");
    reporter.mark_failed();
    assert!(!reporter.lost());
    assert_eq!(
        reporter.take_pending(),
        Some(first),
        "the report is retried"
    );

    reporter.mark_failed();
    assert!(reporter.lost());
    assert_eq!(reporter.take_pending(), None);
}

#[test]
fn a_lost_reporter_ignores_later_effects() {
    let mut reporter = LtiReporter::new();
    reporter.record(&[completion(CompletionStatus::Completed)]);
    assert!(reporter.take_pending().is_some());
    reporter.mark_failed();
    assert!(reporter.take_pending().is_some());
    reporter.mark_failed();
    assert!(reporter.lost());

    reporter.record(&[score(100, CompletionStatus::Passed)]);
    assert_eq!(reporter.take_pending(), None);
}

#[test]
fn report_json_uses_a_null_percentage_for_progress_only() {
    assert_eq!(
        ScoreReport {
            percent: Some(12),
            status: "passed",
        }
        .json_body(),
        r#"{"percent":12,"status":"passed"}"#
    );
    assert_eq!(
        ScoreReport {
            percent: None,
            status: "completed",
        }
        .json_body(),
        r#"{"percent":null,"status":"completed"}"#
    );
}
