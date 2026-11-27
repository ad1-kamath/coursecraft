//! LTI 1.3 score reporting as a small pure state machine.
//!
//! The published LTI package posts a score or completion report to the
//! CourseCraft LTI server on its own origin. This module turns runtime effects
//! into those reports, keeps only the latest unsent one, and remembers whether
//! the single retry was already spent. The player owns the HTTP transport, so
//! every rule here is testable without a browser or a network.
//!
//! LTI 1.3 has no suspend data: a relaunch starts the course fresh. Only the
//! final score and progress are reported.

use std::collections::VecDeque;

use coursecraft_course::CompletionStatus;
use coursecraft_logic::Effect;

/// One report for the LTI server's `/lti/score` endpoint.
///
/// A `percent` of `None` means the course completed without a score; the
/// server then omits `scoreGiven` and sends only the progress fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScoreReport {
    pub percent: Option<u8>,
    /// `passed`, `failed`, `completed` or `incomplete`.
    pub status: &'static str,
}

impl ScoreReport {
    /// The compact JSON body for `/lti/score`: `{"percent":…,"status":…}`.
    #[must_use]
    pub fn json_body(&self) -> String {
        match self.percent {
            Some(percent) => format!("{{\"percent\":{percent},\"status\":\"{}\"}}", self.status),
            None => format!("{{\"percent\":null,\"status\":\"{}\"}}", self.status),
        }
    }
}

/// Turns effects into at most one pending LTI report and tracks its delivery.
#[derive(Debug, Default)]
pub struct LtiReporter {
    pending: VecDeque<ScoreReport>,
    sent_final: bool,
    ever_completed: bool,
    passed: bool,
    best_passed_score: Option<u8>,
    last_failed_score: Option<u8>,
    in_flight: Option<ScoreReport>,
    retried: bool,
    lost: bool,
    last_sent: Option<ScoreReport>,
}

impl LtiReporter {
    /// A reporter with nothing pending.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record the reports in one input's effects.
    ///
    /// Within a single batch, the highest-rank status (`incomplete < failed <
    /// completed < passed`) wins while preserving any `Score` percentage.
    /// Across batches, status is monotonic: `completed` never downgrades to
    /// `failed` or `incomplete`, `failed` + later `completed` upgrades while
    /// keeping the recorded failed score, and `passed` only updates on a
    /// strictly higher passing score.
    pub fn record(&mut self, effects: &[Effect]) {
        if self.lost {
            return;
        }
        let mut best_status: Option<&'static str> = None;
        let mut best_rank: u8 = 0;
        let mut score_percent: Option<u8> = None;
        let mut score_rank: u8 = 0;
        for effect in effects {
            let (effect_percent, status) = match effect {
                Effect::Score {
                    percent, status, ..
                } => (Some(*percent), status_id(*status)),
                Effect::Completion { status } => (None, status_id(*status)),
                _ => continue,
            };
            let rank = status_rank(status);
            if let Some(percent) = effect_percent {
                if score_percent.is_none() || rank > score_rank {
                    score_percent = Some(percent);
                    score_rank = rank;
                } else if rank == score_rank {
                    score_percent = Some(score_percent.map_or(percent, |prev| prev.max(percent)));
                }
            }
            if best_status.is_none() || rank > best_rank {
                best_status = Some(status);
                best_rank = rank;
            }
        }
        let Some(status) = best_status else {
            return;
        };
        let mut report = ScoreReport {
            percent: score_percent,
            status,
        };

        let pending_pass_score = self
            .pending
            .iter()
            .filter(|r| r.status == "passed")
            .filter_map(|r| r.percent)
            .max();
        let in_flight_pass = self.in_flight.as_ref().filter(|r| r.status == "passed");
        let current_rank = [
            if self.passed { 3 } else { 0 },
            if self.ever_completed { 2 } else { 0 },
            u8::from(self.sent_final),
            self.in_flight.map_or(0, |r| status_rank(r.status)),
            self.pending
                .iter()
                .map(|r| status_rank(r.status))
                .max()
                .unwrap_or(0),
            self.last_sent.map_or(0, |r| status_rank(r.status)),
        ]
        .into_iter()
        .max()
        .unwrap_or(0);
        let prior_failed_score = self
            .pending
            .iter()
            .rev()
            .find(|r| r.status == "failed")
            .and_then(|r| r.percent)
            .or_else(|| {
                self.in_flight
                    .filter(|r| r.status == "failed")
                    .and_then(|r| r.percent)
            })
            .or(self.last_failed_score);

        let incoming_rank = status_rank(report.status);
        if current_rank >= 3 {
            if report.status != "passed" {
                return;
            }
            let best_score = [
                self.best_passed_score,
                in_flight_pass.and_then(|r| r.percent),
                pending_pass_score,
            ]
            .into_iter()
            .flatten()
            .max();
            match (report.percent, best_score) {
                (None, _) => return,
                (Some(new_percent), Some(best)) if new_percent <= best => return,
                _ => {}
            }
        } else if incoming_rank < current_rank {
            return;
        } else if report.status == "completed" && report.percent.is_none() {
            if current_rank >= 2 {
                return;
            }
            if let Some(prior_score) = prior_failed_score {
                report.percent = Some(prior_score);
            }
        } else if report.status == "failed"
            && report.percent.is_none()
            && (prior_failed_score.is_some() || current_rank >= 1)
        {
            return;
        }

        if self.in_flight == Some(report) || self.last_sent == Some(report) {
            return;
        }
        if report.status == "failed"
            && let Some(percent) = report.percent
        {
            self.last_failed_score = Some(percent);
        }
        if self.in_flight.is_none() {
            self.retried = false;
        }
        self.pending.clear();
        self.pending.push_back(report);
    }

    /// Take the latest unsent report, if no other report is in flight.
    pub fn take_pending(&mut self) -> Option<ScoreReport> {
        if self.in_flight.is_some() {
            return None;
        }
        let report = self.pending.pop_front()?;
        self.in_flight = Some(report);
        Some(report)
    }

    /// The endpoint accepted the report (`204`).
    pub fn mark_sent(&mut self) {
        let Some(report) = self.in_flight.take() else {
            return;
        };
        if report.status != "incomplete" {
            self.sent_final = true;
        }
        if report.status == "completed" || report.status == "passed" {
            self.ever_completed = true;
        }
        if report.status == "passed" {
            self.passed = true;
            if let Some(percent) = report.percent {
                self.best_passed_score = Some(
                    self.best_passed_score
                        .map_or(percent, |best| best.max(percent)),
                );
            }
        }
        self.last_sent = Some(report);
        self.retried = false;
    }

    /// The endpoint refused the report, or the network failed.
    ///
    /// The report is re-queued once, unless a newer report is already waiting: that
    /// one replaces it. A second failure gives up and sets
    /// [`LtiReporter::lost`], which the player turns into the lost-reporting
    /// notice.
    pub fn mark_failed(&mut self) {
        let Some(report) = self.in_flight.take() else {
            return;
        };
        if self.has_pending() {
            // A newer report arrived while this one was in flight. It replaces the
            // failed one, so the stale report is dropped and is not retried.
            return;
        }
        if self.retried {
            self.lost = true;
            self.pending.clear();
        } else {
            self.retried = true;
            self.pending.push_front(report);
        }
    }

    /// Whether reporting was attempted and has now given up for this session.
    #[must_use]
    pub fn lost(&self) -> bool {
        self.lost
    }

    /// Whether a report is waiting to be sent.
    #[must_use]
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }
}

fn status_id(status: CompletionStatus) -> &'static str {
    match status {
        CompletionStatus::Passed => "passed",
        CompletionStatus::Failed => "failed",
        CompletionStatus::Completed => "completed",
        CompletionStatus::Incomplete => "incomplete",
    }
}

fn status_rank(status: &str) -> u8 {
    match status {
        "incomplete" => 0,
        "failed" => 1,
        "completed" => 2,
        "passed" => 3,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a4_9_lti_reporter_high_water_dedup_and_retry_lost_reporting() {
        let mut reporter = LtiReporter::new();
        let mut sent = Vec::new();

        // Score(80, Passed) -> 204
        reporter.record(&[Effect::Score {
            raw: 80,
            max: 100,
            percent: 80,
            status: CompletionStatus::Passed,
        }]);
        let r1 = reporter.take_pending().expect("80 passed");
        assert_eq!(r1.percent, Some(80));
        assert_eq!(r1.status, "passed");
        sent.push(r1.percent);
        reporter.mark_sent();
        assert!(!reporter.lost());

        // Score(60, Failed) -> ignored (already passed)
        reporter.record(&[Effect::Score {
            raw: 60,
            max: 100,
            percent: 60,
            status: CompletionStatus::Failed,
        }]);
        assert!(reporter.take_pending().is_none());

        // Score(70, Passed) -> ignored (<= high-water 80)
        reporter.record(&[Effect::Score {
            raw: 70,
            max: 100,
            percent: 70,
            status: CompletionStatus::Passed,
        }]);
        assert!(reporter.take_pending().is_none());

        // Score(90, Passed) -> sent -> 500 -> retried once -> 500 -> lost() == true
        reporter.record(&[Effect::Score {
            raw: 90,
            max: 100,
            percent: 90,
            status: CompletionStatus::Passed,
        }]);
        let r2 = reporter.take_pending().expect("90 passed");
        assert_eq!(r2.percent, Some(90));
        sent.push(r2.percent);
        reporter.mark_failed();
        assert!(!reporter.lost());

        let r2_retry = reporter.take_pending().expect("90 retry");
        assert_eq!(r2_retry.percent, Some(90));
        sent.push(r2_retry.percent);
        reporter.mark_failed();
        assert!(reporter.lost());
        assert!(reporter.take_pending().is_none());

        assert_eq!(sent, vec![Some(80), Some(90), Some(90)]);
    }
}
