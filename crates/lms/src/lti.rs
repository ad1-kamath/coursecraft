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
    /// A `Score` effect wins over a `Completion` effect in the same batch, so a
    /// quiz result is sent once with its percentage. A `Completion` on its own
    /// is sent with `percent: None`. Only the latest unsent report is kept, and
    /// a report identical to the last one sent is ignored.
    pub fn record(&mut self, effects: &[Effect]) {
        if self.lost {
            return;
        }
        let has_score = effects
            .iter()
            .any(|effect| matches!(effect, Effect::Score { .. }));
        let mut report = None;
        for effect in effects {
            match effect {
                Effect::Score {
                    percent, status, ..
                } => {
                    report = Some(ScoreReport {
                        percent: Some(*percent),
                        status: status_id(*status),
                    });
                }
                Effect::Completion { status } if !has_score => {
                    report = Some(ScoreReport {
                        percent: None,
                        status: status_id(*status),
                    });
                }
                _ => {}
            }
        }
        let Some(report) = report else {
            return;
        };
        if self.in_flight == Some(report) || self.last_sent == Some(report) {
            return;
        }
        if self.sent_final && report.percent.is_none() {
            // A final score already went out; a later bare completion must not
            // replace it with a progress-only report.
            return;
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
        self.last_sent = Some(report);
        self.retried = false;
    }

    /// The endpoint refused the report, or the network failed.
    ///
    /// The report is re-queued once. A second failure gives up and sets
    /// [`LtiReporter::lost`], which the player turns into the lost-reporting
    /// notice.
    pub fn mark_failed(&mut self) {
        let Some(report) = self.in_flight.take() else {
            return;
        };
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
