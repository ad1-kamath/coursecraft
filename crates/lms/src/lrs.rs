//! Asynchronous xAPI / cmi5 bridge.
//!
//! The host implements [`LrsApi`] over `fetch`, so sending never blocks play.
//! This module is a pure state machine: it queues statements, drives the cmi5
//! handshake, persists progress through the xAPI State API, and retries with
//! backoff. Every rule is testable with a fake, and no method panics on hostile
//! JSON, odd statuses or unknown response ids.

use std::collections::{BTreeMap, VecDeque};

use coursecraft_course::{CompletionStatus, Course};
use coursecraft_logic::{Effect, PlayerState, decode_suspend, encode_suspend};
use coursecraft_xapi::{AnsweredInput, Flavor, LaunchParams, StatementContext, activity_id_for};
use serde_json::{Map, Value, json};

use crate::bridge::StartPlan;
use crate::interaction::{format_response_2004, interaction_record};
use crate::time::format_session_time_iso;

/// How long the handshake may take before play starts fresh.
pub const HANDSHAKE_TIMEOUT_MS: u64 = 8000;

/// Maximum number of statements held in the queue.
pub const MAX_QUEUE: usize = 1000;

/// Maximum number of statements in one POST.
pub const MAX_BATCH: usize = 50;

/// Minimum gap between progress document PUTs.
pub const PROGRESS_INTERVAL_MS: u64 = 5000;

/// Maximum body size for a final keepalive POST.
pub const MAX_KEEPALIVE_BODY: usize = 60_000;

const API_VERSION: &str = "1.0.3";
const PROGRESS_STATE_ID: &str = "coursecraft.progress";
const LAUNCH_DATA_STATE_ID: &str = "LMS.LaunchData";
const BACKOFF_MS: [u64; 6] = [1000, 2000, 4000, 8000, 16_000, 30_000];

/// The HTTP method of an [`LrsRequest`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LrsMethod {
    Get,
    Post,
    Put,
}

/// One request the host must send asynchronously.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LrsRequest {
    pub id: u64,
    pub method: LrsMethod,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
    pub keepalive: bool,
}

/// One response from the host. A status of `0` means a network error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LrsResponse {
    pub id: u64,
    pub status: u16,
    pub body: String,
}

/// What the host's async transport has to provide.
pub trait LrsApi {
    /// Queue a request and return at once.
    fn send(&mut self, request: LrsRequest);
    /// Take every response that has arrived since the last call.
    fn poll(&mut self) -> Vec<LrsResponse>;
    /// Current time as an xAPI timestamp string.
    fn now_iso(&mut self) -> String;
    /// A fresh statement id.
    fn new_uuid(&mut self) -> String;
}

/// What one [`XapiBridge::tick`] produced.
#[derive(Debug, Clone, PartialEq)]
pub struct Tick {
    /// The start plan, returned exactly once.
    pub plan: Option<StartPlan>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Idle,
    Fetch,
    LaunchData,
    Progress,
    Ready,
    Off,
}

#[derive(Debug)]
enum Pending {
    Fetch,
    LaunchData,
    ProgressGet,
    Statements,
    ProgressPut(u64),
}

#[derive(Debug, Clone)]
struct QueuedStatement {
    value: Value,
    initialized: bool,
}

#[derive(Debug)]
struct InFlight {
    statements: Vec<QueuedStatement>,
}

#[derive(Debug, Default)]
struct ProgressDoc {
    suspend: Option<String>,
    completed: bool,
    passed: bool,
    dirty: bool,
    revision: u64,
    in_flight: Option<(u64, u64)>,
    last_put_ms: Option<u64>,
}

/// Bridges runtime effects to xAPI statements and cmi5 state.
pub struct XapiBridge {
    flavor: Flavor,
    endpoint: String,
    actor: Value,
    activity_id: String,
    activity_name: String,
    registration: Option<String>,
    fetch_url: Option<String>,
    auth: Option<String>,
    token_header: Option<String>,
    context_template: Option<Value>,
    mastery_score: Option<f64>,
    launch_mode: String,
    return_url: Option<String>,
    phase: Phase,
    handshake_started_ms: Option<u64>,
    handshake_finished: bool,
    plan_returned: bool,
    plan_ready: Option<StartPlan>,
    reporting: bool,
    finished: bool,
    progress: ProgressDoc,
    queue: VecDeque<QueuedStatement>,
    in_flight: Option<InFlight>,
    pending: BTreeMap<u64, Pending>,
    next_request: u64,
    retry_at_ms: Option<u64>,
    backoff_step: usize,
    completed_saved: bool,
    passed_saved: bool,
    failed_this_session: bool,
    terminated_sent: bool,
    overflow_warned: bool,
    warnings: Vec<String>,
}

impl XapiBridge {
    /// Create a bridge for a launch, its flavor and the course being played.
    #[must_use]
    pub fn new(params: LaunchParams, flavor: Flavor, course: &Course) -> Self {
        let activity_id = params
            .activity_id
            .clone()
            .unwrap_or_else(|| activity_id_for(&course.id));
        Self {
            flavor,
            endpoint: params.endpoint,
            actor: params.actor,
            activity_id,
            activity_name: course.title.clone(),
            registration: params.registration,
            fetch_url: params.fetch,
            auth: params.auth,
            token_header: None,
            context_template: None,
            mastery_score: None,
            launch_mode: "Normal".to_string(),
            return_url: None,
            phase: Phase::Idle,
            handshake_started_ms: None,
            handshake_finished: false,
            plan_returned: false,
            plan_ready: None,
            reporting: true,
            finished: false,
            progress: ProgressDoc::default(),
            queue: VecDeque::new(),
            in_flight: None,
            pending: BTreeMap::new(),
            next_request: 0,
            retry_at_ms: None,
            backoff_step: 0,
            completed_saved: false,
            passed_saved: false,
            failed_this_session: false,
            terminated_sent: false,
            overflow_warned: false,
            warnings: Vec::new(),
        }
    }

    /// Begin the handshake. cmi5 first posts to its fetch URL; plain xAPI goes
    /// straight to enqueueing `initialized` and reading saved progress.
    pub fn start(&mut self, api: &mut impl LrsApi) {
        if self.phase != Phase::Idle {
            return;
        }
        match self.flavor {
            Flavor::Cmi5 => {
                let url = self.fetch_url.clone().unwrap_or_default();
                let id = self.new_request();
                self.pending.insert(id, Pending::Fetch);
                self.phase = Phase::Fetch;
                api.send(LrsRequest {
                    id,
                    method: LrsMethod::Post,
                    url,
                    headers: Vec::new(),
                    body: None,
                    keepalive: false,
                });
            }
            Flavor::Xapi => {
                self.enqueue_initialized(api);
                self.send_progress_get(api);
                self.phase = Phase::Progress;
            }
        }
    }

    /// Advance the handshake and the send queue. The plan is returned once.
    pub fn tick(&mut self, api: &mut impl LrsApi, course: &Course, now_ms: u64) -> Tick {
        let mut warnings = Vec::new();
        if !self.plan_returned {
            if self.handshake_started_ms.is_none() {
                self.handshake_started_ms = Some(now_ms);
            }
            let timed_out = self
                .handshake_started_ms
                .is_some_and(|start| now_ms.saturating_sub(start) >= HANDSHAKE_TIMEOUT_MS);
            if timed_out {
                self.timed_out();
            } else {
                self.poll(api, course, now_ms, &mut warnings);
            }
        } else {
            self.poll(api, course, now_ms, &mut warnings);
        }

        let mut plan = None;
        if !self.plan_returned
            && let Some(ready) = self.plan_ready.take()
        {
            self.plan_returned = true;
            plan = Some(ready);
        }

        if self.plan_returned {
            self.maybe_pump(api, now_ms, &mut warnings);
        }

        warnings.extend(self.drain_warnings());
        Tick { plan, warnings }
    }

    /// Translate effects into statements after one player input.
    pub fn after_input(
        &mut self,
        api: &mut impl LrsApi,
        course: &Course,
        state: &PlayerState,
        effects: &[Effect],
        session_ms: u64,
        now_ms: u64,
    ) -> Vec<String> {
        if self.finished {
            return Vec::new();
        }
        let mut warnings = Vec::new();
        if self.handshake_finished {
            self.poll(api, course, now_ms, &mut warnings);
        }
        if !self.handshake_finished || !self.reporting {
            warnings.extend(self.drain_warnings());
            return warnings;
        }
        if self.is_browse() {
            // Browse and Review report nothing but must still end with `terminated`.
            if effects.iter().any(|effect| matches!(effect, Effect::Exit)) {
                warnings.extend(self.finish_session(api, course, None, session_ms));
            }
            warnings.extend(self.drain_warnings());
            return warnings;
        }

        let duration = format_session_time_iso(session_ms);
        let mut durable_changed = false;
        let mut should_finish = false;

        for effect in effects {
            match effect {
                Effect::QuestionAnswered { slide, .. } => {
                    self.enqueue_answered(api, course, state, slide);
                    durable_changed = true;
                }
                Effect::Score {
                    percent, status, ..
                } => {
                    self.handle_score(api, *percent, *status, &duration);
                }
                Effect::Completion { status } => {
                    if !matches!(status, CompletionStatus::Incomplete) {
                        self.ensure_completed(api, &duration);
                    }
                }
                Effect::SlideEntered { .. }
                | Effect::StateChanged { .. }
                | Effect::VariableChanged { .. } => {
                    durable_changed = true;
                }
                Effect::Exit => {
                    should_finish = true;
                }
                _ => {}
            }
        }

        if durable_changed {
            self.mark_progress_dirty();
        }
        // A pass or completion alone also dirties progress, and its flags must be
        // saved even when no durable state changed on this input.
        if self.progress.dirty {
            self.progress.suspend = Some(encode_suspend(course, state));
        }

        if should_finish {
            warnings.extend(self.finish_session(api, course, Some(state), session_ms));
        } else {
            self.maybe_pump(api, now_ms, &mut warnings);
        }

        warnings.extend(self.drain_warnings());
        warnings
    }

    /// Finish the session: enqueue `terminated`, flush everything with
    /// keepalive, and write progress once. Calling it again does nothing.
    pub fn finish_session(
        &mut self,
        api: &mut impl LrsApi,
        course: &Course,
        state: Option<&PlayerState>,
        session_ms: u64,
    ) -> Vec<String> {
        if self.finished {
            return Vec::new();
        }
        self.finished = true;

        let mut warnings = Vec::new();
        if !self.handshake_finished || !self.reporting {
            warnings.extend(self.drain_warnings());
            return warnings;
        }

        let duration = format_session_time_iso(session_ms);
        if !self.terminated_sent {
            self.enqueue_terminated(api, &duration);
        }

        let browse = self.is_browse();
        let write_progress = !browse && state.is_some() && self.progress.dirty;
        if write_progress && let Some(state) = state {
            self.progress.suspend = Some(encode_suspend(course, state));
        }

        let mut all: Vec<QueuedStatement> = Vec::new();
        if let Some(inflight) = self.in_flight.take() {
            all.extend(inflight.statements);
        }
        all.extend(self.queue.drain(..));
        self.pending
            .retain(|_, pending| !matches!(pending, Pending::Statements));

        self.send_keepalive_batches(api, &all, &mut warnings);

        if write_progress && let Some(suspend) = self.progress.suspend.clone() {
            self.send_keepalive_progress(api, &suspend);
        }
        self.progress.in_flight = None;

        warnings.extend(self.drain_warnings());
        warnings
    }

    /// The `returnURL` reported by `LMS.LaunchData`, if it was a safe URL.
    #[must_use]
    pub fn return_url(&self) -> Option<&str> {
        self.return_url.as_deref()
    }

    /// Whether a launch was attempted and reporting is now off for the session.
    ///
    /// True after a failed cmi5 fetch, a handshake timeout, or authentication
    /// failing mid-session. False while the handshake is still in progress, and
    /// for Browse or Review modes while they keep reporting on. A course opened
    /// with no launch parameters has no bridge at all, and SCORM never does this.
    #[must_use]
    pub fn reporting_lost(&self) -> bool {
        // `Phase::Off` is set exactly where `reporting` turns false after a
        // launch: every path that stops reporting lands here and never leaves.
        self.phase == Phase::Off
    }

    /// Whether anything is queued or in flight, so the host keeps repainting.
    #[must_use]
    pub fn is_busy(&self) -> bool {
        !self.plan_returned
            || self.in_flight.is_some()
            || !self.queue.is_empty()
            || self.progress.in_flight.is_some()
    }

    #[cfg(test)]
    pub(crate) fn queued_len(&self) -> usize {
        self.queue.len()
    }

    fn maybe_pump(&mut self, api: &mut impl LrsApi, now_ms: u64, warnings: &mut Vec<String>) {
        if self.finished || !self.handshake_finished || !self.reporting {
            return;
        }
        self.send_statements(api, now_ms, warnings);
        self.maybe_put_progress(api, now_ms);
    }

    fn poll(
        &mut self,
        api: &mut impl LrsApi,
        course: &Course,
        now_ms: u64,
        warnings: &mut Vec<String>,
    ) {
        for response in api.poll() {
            let Some(pending) = self.pending.remove(&response.id) else {
                continue;
            };
            match pending {
                Pending::Fetch => self.handle_fetch(api, &response, warnings),
                Pending::LaunchData => self.handle_launch_data(api, &response, warnings),
                Pending::ProgressGet => {
                    self.handle_progress_get(course, response.status, &response.body, warnings);
                }
                Pending::Statements => {
                    self.handle_statement_response(response.status, now_ms, warnings);
                }
                Pending::ProgressPut(revision) => {
                    self.handle_progress_put(response.status, revision);
                }
            }
        }
    }

    fn handle_fetch(
        &mut self,
        api: &mut impl LrsApi,
        response: &LrsResponse,
        warnings: &mut Vec<String>,
    ) {
        let mut token = None;
        if response.status == 200
            && let Ok(value) = serde_json::from_str::<Value>(&response.body)
            && let Some(text) = value.get("auth-token").and_then(Value::as_str)
        {
            token = Some(text.to_owned());
        }

        match token {
            Some(token) => {
                self.token_header = Some(format!("Basic {token}"));
                self.send_launch_data(api);
                self.phase = Phase::LaunchData;
            }
            None => {
                warnings.push(fetch_failure(&response.body, response.status));
                self.turn_off();
            }
        }
    }

    fn handle_launch_data(
        &mut self,
        api: &mut impl LrsApi,
        response: &LrsResponse,
        warnings: &mut Vec<String>,
    ) {
        if (200..300).contains(&response.status) {
            match serde_json::from_str::<Value>(&response.body) {
                Ok(Value::Object(document)) => self.apply_launch_data(&document, warnings),
                _ => warnings.push("LMS.LaunchData is not a JSON object; using defaults".into()),
            }
        } else {
            warnings.push(format!(
                "LMS.LaunchData unavailable ({}); using defaults",
                status_text(response.status)
            ));
        }

        self.enqueue_initialized(api);
        self.send_progress_get(api);
        self.phase = Phase::Progress;
    }

    fn apply_launch_data(&mut self, document: &Map<String, Value>, warnings: &mut Vec<String>) {
        if let Some(template) = document.get("contextTemplate")
            && template.is_object()
        {
            self.context_template = Some(template.clone());
        }
        if let Some(mode) = document.get("launchMode").and_then(Value::as_str) {
            self.launch_mode = mode.to_owned();
        }
        if let Some(score) = document.get("masteryScore") {
            match score.as_f64() {
                Some(value) if value.is_finite() && (0.0..=1.0).contains(&value) => {
                    self.mastery_score = Some(value);
                }
                _ => warnings
                    .push("LMS.LaunchData masteryScore is not a number in 0..=1; ignored".into()),
            }
        }
        if let Some(url) = document.get("returnURL") {
            match url.as_str() {
                Some(text) if text.starts_with("http://") || text.starts_with("https://") => {
                    self.return_url = Some(text.to_owned());
                }
                _ => {
                    warnings.push("LMS.LaunchData returnURL is not an http(s) URL; ignored".into())
                }
            }
        }
    }

    fn handle_progress_get(
        &mut self,
        course: &Course,
        status: u16,
        body: &str,
        warnings: &mut Vec<String>,
    ) {
        let learner_name = self.learner_name();
        let plan = if status == 404 {
            StartPlan {
                resume: None,
                learner_name,
            }
        } else if (200..300).contains(&status) {
            match serde_json::from_str::<Value>(body) {
                Ok(Value::Object(document)) => {
                    if let Some(completed) = document.get("completed").and_then(Value::as_bool) {
                        self.completed_saved = completed;
                    }
                    if let Some(passed) = document.get("passed").and_then(Value::as_bool) {
                        self.passed_saved = passed;
                    }
                    match document.get("suspend").and_then(Value::as_str) {
                        Some(suspend) if !suspend.is_empty() => {
                            match decode_suspend(course, suspend) {
                                Ok(state) => StartPlan {
                                    resume: Some(state),
                                    learner_name,
                                },
                                Err(err) => {
                                    warnings.push(format!(
                                        "failed to decode xAPI progress suspend data: {err}"
                                    ));
                                    StartPlan {
                                        resume: None,
                                        learner_name,
                                    }
                                }
                            }
                        }
                        _ => StartPlan {
                            resume: None,
                            learner_name,
                        },
                    }
                }
                _ => {
                    warnings
                        .push("xAPI progress document is not a JSON object; starting fresh".into());
                    StartPlan {
                        resume: None,
                        learner_name,
                    }
                }
            }
        } else {
            warnings.push(format!(
                "failed to read xAPI progress ({}); starting fresh",
                status_text(status)
            ));
            StartPlan {
                resume: None,
                learner_name,
            }
        };

        self.progress.completed = self.completed_saved;
        self.progress.passed = self.passed_saved;
        self.handshake_finished = true;
        self.phase = Phase::Ready;
        self.plan_ready = Some(plan);
    }

    fn handle_statement_response(&mut self, status: u16, now_ms: u64, warnings: &mut Vec<String>) {
        let Some(inflight) = self.in_flight.take() else {
            return;
        };

        if (200..300).contains(&status) || status == 409 {
            self.backoff_step = 0;
            self.retry_at_ms = None;
            return;
        }

        if status == 401 || status == 403 {
            warnings.push(format!(
                "xAPI authentication failed (HTTP {status}); reporting disabled for this session"
            ));
            self.reporting = false;
            self.phase = Phase::Off;
            self.queue.clear();
            return;
        }

        if (400..500).contains(&status) {
            warnings.push(format!(
                "xAPI statement batch rejected ({}); dropped",
                status_text(status)
            ));
            self.backoff_step = 0;
            self.retry_at_ms = None;
            return;
        }

        let step = self.backoff_step.min(BACKOFF_MS.len() - 1);
        let delay = BACKOFF_MS.get(step).copied().unwrap_or(30_000);
        self.backoff_step = (step + 1).min(BACKOFF_MS.len() - 1);
        self.retry_at_ms = Some(now_ms.saturating_add(delay));
        for statement in inflight.statements.into_iter().rev() {
            self.queue.push_front(statement);
        }
    }

    fn handle_progress_put(&mut self, status: u16, revision: u64) {
        let matches = self
            .progress
            .in_flight
            .is_some_and(|(_, inflight_revision)| inflight_revision == revision);
        if matches {
            self.progress.in_flight = None;
        }
        if (200..300).contains(&status)
            && revision == self.progress.revision
            && self.progress.in_flight.is_none()
        {
            self.progress.dirty = false;
        }
    }

    fn enqueue_initialized(&mut self, api: &mut impl LrsApi) {
        let id = api.new_uuid();
        let timestamp = api.now_iso();
        let statement = self.statement_context().initialized(&id, &timestamp);
        self.enqueue_statement(QueuedStatement {
            value: statement,
            initialized: true,
        });
    }

    fn enqueue_terminated(&mut self, api: &mut impl LrsApi, duration: &str) {
        let id = api.new_uuid();
        let timestamp = api.now_iso();
        let statement = self
            .statement_context()
            .terminated(&id, &timestamp, duration);
        self.enqueue_statement(QueuedStatement {
            value: statement,
            initialized: false,
        });
        self.terminated_sent = true;
    }

    fn enqueue_answered(
        &mut self,
        api: &mut impl LrsApi,
        course: &Course,
        state: &PlayerState,
        slide: &str,
    ) {
        let Some(record) = interaction_record(course, slide, state) else {
            return;
        };
        let input = AnsweredInput {
            question_id: record.question_id,
            attempt: record.attempt,
            interaction_type: record.kind.as_cmi_type().to_owned(),
            description: record.description,
            response: format_response_2004(&record.learner, false, record.case_sensitive),
            correct_patterns: record
                .correct
                .iter()
                .map(|correct| format_response_2004(correct, true, record.case_sensitive))
                .collect(),
            success: record.is_correct,
            points: record.weighting,
            awarded: record.awarded,
            survey: record.survey,
        };
        let id = api.new_uuid();
        let timestamp = api.now_iso();
        let statement = self.statement_context().answered(&id, &timestamp, &input);
        self.enqueue_statement(QueuedStatement {
            value: statement,
            initialized: false,
        });
    }

    fn handle_score(
        &mut self,
        api: &mut impl LrsApi,
        percent: u8,
        status: CompletionStatus,
        duration: &str,
    ) {
        let scaled = f64::from(percent) / 100.0;
        // A course with no quiz reports `Completed`: there is nothing to judge, so a
        // mastery score from the LMS must not turn it into a failure.
        let course_passed = match status {
            CompletionStatus::Passed => true,
            CompletionStatus::Failed => false,
            CompletionStatus::Completed | CompletionStatus::Incomplete => return,
        };
        let passed = self
            .mastery_score
            .map_or(course_passed, |mastery| scaled >= mastery);
        if passed {
            self.enqueue_passed(api, scaled, duration);
        } else {
            self.enqueue_failed(api, scaled, duration);
        }
        self.ensure_completed(api, duration);
    }

    fn enqueue_passed(&mut self, api: &mut impl LrsApi, scaled: f64, duration: &str) {
        if self.passed_saved || self.failed_this_session || self.finished {
            return;
        }
        let id = api.new_uuid();
        let timestamp = api.now_iso();
        let statement = self
            .statement_context()
            .passed(&id, &timestamp, scaled, duration);
        self.enqueue_statement(QueuedStatement {
            value: statement,
            initialized: false,
        });
        self.passed_saved = true;
        self.progress.passed = true;
        self.mark_progress_dirty();
    }

    fn enqueue_failed(&mut self, api: &mut impl LrsApi, scaled: f64, duration: &str) {
        if self.passed_saved || self.failed_this_session || self.finished {
            return;
        }
        let id = api.new_uuid();
        let timestamp = api.now_iso();
        let statement = self
            .statement_context()
            .failed(&id, &timestamp, scaled, duration);
        self.enqueue_statement(QueuedStatement {
            value: statement,
            initialized: false,
        });
        self.failed_this_session = true;
    }

    fn ensure_completed(&mut self, api: &mut impl LrsApi, duration: &str) {
        if self.completed_saved || self.finished {
            return;
        }
        let id = api.new_uuid();
        let timestamp = api.now_iso();
        let statement = self
            .statement_context()
            .completed(&id, &timestamp, duration);
        self.enqueue_statement(QueuedStatement {
            value: statement,
            initialized: false,
        });
        self.completed_saved = true;
        self.progress.completed = true;
        self.mark_progress_dirty();
    }

    fn enqueue_statement(&mut self, statement: QueuedStatement) {
        if self.queue.len() >= MAX_QUEUE {
            let position = self.queue.iter().position(|queued| !queued.initialized);
            if let Some(position) = position {
                self.queue.remove(position);
                if !self.overflow_warned {
                    self.overflow_warned = true;
                    self.warnings.push(format!(
                        "xAPI statement queue is full ({MAX_QUEUE}); dropped the oldest statement"
                    ));
                }
            } else {
                return;
            }
        }
        self.queue.push_back(statement);
    }

    fn send_statements(&mut self, api: &mut impl LrsApi, now_ms: u64, warnings: &mut Vec<String>) {
        if self.in_flight.is_some() || self.queue.is_empty() || self.finished {
            return;
        }
        if let Some(retry_at) = self.retry_at_ms
            && now_ms < retry_at
        {
            return;
        }

        let take = self.queue.len().min(MAX_BATCH);
        let batch: Vec<QueuedStatement> = self.queue.iter().take(take).cloned().collect();
        let body = match statements_body(&batch) {
            Ok(body) => body,
            Err(err) => {
                self.queue.drain(..take);
                warnings.push(format!("failed to encode xAPI statement batch: {err}"));
                return;
            }
        };
        self.queue.drain(..take);

        let id = self.new_request();
        self.pending.insert(id, Pending::Statements);
        self.in_flight = Some(InFlight { statements: batch });
        api.send(LrsRequest {
            id,
            method: LrsMethod::Post,
            url: self.statements_url(),
            headers: self.json_headers(),
            body: Some(body),
            keepalive: false,
        });
    }

    fn maybe_put_progress(&mut self, api: &mut impl LrsApi, now_ms: u64) {
        if self.progress.in_flight.is_some() || !self.progress.dirty || self.finished {
            return;
        }
        if let Some(last) = self.progress.last_put_ms
            && now_ms.saturating_sub(last) < PROGRESS_INTERVAL_MS
        {
            return;
        }
        let Some(suspend) = self.progress.suspend.clone() else {
            return;
        };
        let Some(url) = self.state_url(PROGRESS_STATE_ID) else {
            return;
        };
        let body = json!({
            "suspend": suspend,
            "completed": self.progress.completed,
            "passed": self.progress.passed,
        })
        .to_string();

        let id = self.new_request();
        let revision = self.progress.revision;
        self.pending.insert(id, Pending::ProgressPut(revision));
        self.progress.in_flight = Some((id, revision));
        self.progress.last_put_ms = Some(now_ms);
        api.send(LrsRequest {
            id,
            method: LrsMethod::Put,
            url,
            headers: self.json_headers(),
            body: Some(body),
            keepalive: false,
        });
    }

    fn send_keepalive_batches(
        &mut self,
        api: &mut impl LrsApi,
        statements: &[QueuedStatement],
        warnings: &mut Vec<String>,
    ) {
        let mut batches: Vec<Vec<QueuedStatement>> = Vec::new();
        let mut batch: Vec<QueuedStatement> = Vec::new();
        let mut size = 2; // the surrounding [ and ]
        for statement in statements {
            let item_size = match serde_json::to_string(&statement.value) {
                Ok(text) => text.len(),
                Err(err) => {
                    warnings.push(format!("failed to encode xAPI statement: {err}"));
                    continue;
                }
            };
            let separator = usize::from(!batch.is_empty());
            if !batch.is_empty() && size + item_size + separator > MAX_KEEPALIVE_BODY {
                batches.push(std::mem::take(&mut batch));
                size = 2;
            }
            size = size
                .saturating_add(item_size)
                .saturating_add(usize::from(!batch.is_empty()));
            batch.push(statement.clone());
        }
        if !batch.is_empty() {
            batches.push(batch);
        }

        for batch in batches {
            let body = match statements_body(&batch) {
                Ok(body) => body,
                Err(err) => {
                    warnings.push(format!("failed to encode keepalive batch: {err}"));
                    continue;
                }
            };
            let id = self.new_request();
            api.send(LrsRequest {
                id,
                method: LrsMethod::Post,
                url: self.statements_url(),
                headers: self.json_headers(),
                body: Some(body),
                keepalive: true,
            });
        }
    }

    fn send_keepalive_progress(&mut self, api: &mut impl LrsApi, suspend: &str) {
        let Some(url) = self.state_url(PROGRESS_STATE_ID) else {
            return;
        };
        let body = json!({
            "suspend": suspend,
            "completed": self.progress.completed,
            "passed": self.progress.passed,
        })
        .to_string();
        let id = self.new_request();
        api.send(LrsRequest {
            id,
            method: LrsMethod::Put,
            url,
            headers: self.json_headers(),
            body: Some(body),
            keepalive: true,
        });
    }

    fn send_launch_data(&mut self, api: &mut impl LrsApi) {
        let Some(url) = self.state_url(LAUNCH_DATA_STATE_ID) else {
            return;
        };
        let id = self.new_request();
        self.pending.insert(id, Pending::LaunchData);
        api.send(LrsRequest {
            id,
            method: LrsMethod::Get,
            url,
            headers: self.read_headers(),
            body: None,
            keepalive: false,
        });
    }

    fn send_progress_get(&mut self, api: &mut impl LrsApi) {
        let Some(url) = self.state_url(PROGRESS_STATE_ID) else {
            return;
        };
        let id = self.new_request();
        self.pending.insert(id, Pending::ProgressGet);
        api.send(LrsRequest {
            id,
            method: LrsMethod::Get,
            url,
            headers: self.read_headers(),
            body: None,
            keepalive: false,
        });
    }

    fn statements_url(&self) -> String {
        format!("{}statements", self.endpoint)
    }

    fn state_url(&self, state_id: &str) -> Option<String> {
        let agent = serde_json::to_string(&self.actor).ok()?;
        let mut url = format!(
            "{}activities/state?stateId={}&activityId={}&agent={}",
            self.endpoint,
            percent_encode(state_id),
            percent_encode(&self.activity_id),
            percent_encode(&agent),
        );
        if let Some(registration) = &self.registration {
            url.push_str("&registration=");
            url.push_str(&percent_encode(registration));
        }
        Some(url)
    }

    fn read_headers(&self) -> Vec<(String, String)> {
        let mut headers = vec![(
            "X-Experience-API-Version".to_string(),
            API_VERSION.to_string(),
        )];
        if let Some(auth) = self.authorization() {
            headers.push(("Authorization".to_string(), auth.to_owned()));
        }
        headers
    }

    fn json_headers(&self) -> Vec<(String, String)> {
        let mut headers = self.read_headers();
        headers.push(("Content-Type".to_string(), "application/json".to_string()));
        headers
    }

    fn authorization(&self) -> Option<&str> {
        match self.flavor {
            Flavor::Cmi5 => self.token_header.as_deref(),
            Flavor::Xapi => self.auth.as_deref(),
        }
    }

    fn statement_context(&self) -> StatementContext {
        StatementContext {
            flavor: self.flavor,
            actor: self.actor.clone(),
            activity_id: self.activity_id.clone(),
            activity_name: self.activity_name.clone(),
            registration: self.registration.clone(),
            context_template: self.context_template.clone(),
            mastery_score: self.mastery_score,
        }
    }

    fn mark_progress_dirty(&mut self) {
        self.progress.revision = self.progress.revision.saturating_add(1);
        self.progress.dirty = true;
    }

    fn is_browse(&self) -> bool {
        self.launch_mode != "Normal"
    }

    /// The launch actor's `name`, when it has one.
    fn learner_name(&self) -> Option<String> {
        self.actor
            .get("name")
            .and_then(Value::as_str)
            .filter(|name| !name.trim().is_empty())
            .map(str::to_owned)
    }

    fn turn_off(&mut self) {
        self.reporting = false;
        self.phase = Phase::Off;
        self.handshake_finished = false;
        self.plan_ready = Some(StartPlan {
            resume: None,
            learner_name: self.learner_name(),
        });
        self.pending.clear();
        self.in_flight = None;
        self.queue.clear();
    }

    fn timed_out(&mut self) {
        if self.plan_ready.is_none() {
            self.warnings.push(format!(
                "xAPI handshake timed out after {HANDSHAKE_TIMEOUT_MS} ms; starting fresh"
            ));
            self.plan_ready = Some(StartPlan {
                resume: None,
                learner_name: self.learner_name(),
            });
        }
        self.reporting = false;
        self.handshake_finished = false;
        self.phase = Phase::Off;
        self.pending.clear();
        self.in_flight = None;
        self.queue.clear();
    }

    fn new_request(&mut self) -> u64 {
        self.next_request = self.next_request.saturating_add(1);
        self.next_request
    }

    fn drain_warnings(&mut self) -> Vec<String> {
        std::mem::take(&mut self.warnings)
    }
}

fn statements_body(batch: &[QueuedStatement]) -> Result<String, serde_json::Error> {
    let values: Vec<&Value> = batch.iter().map(|statement| &statement.value).collect();
    serde_json::to_string(&values)
}

fn fetch_failure(body: &str, status: u16) -> String {
    if let Ok(value) = serde_json::from_str::<Value>(body)
        && let Some(code) = value.get("error-code")
    {
        let code = match code {
            Value::String(text) => text.clone(),
            other => other.to_string(),
        };
        let code: String = code.chars().take(100).collect();
        return format!("cmi5 fetch failed: error-code {code}");
    }
    format!("cmi5 fetch failed ({})", status_text(status))
}

fn status_text(status: u16) -> String {
    if status == 0 {
        "network error".to_string()
    } else {
        format!("HTTP {status}")
    }
}

fn percent_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}
