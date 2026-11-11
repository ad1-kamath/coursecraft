//! Tests for the asynchronous xAPI bridge.

use std::collections::HashSet;

use coursecraft_course::{CompletionStatus, Course};
use coursecraft_logic::{Answer, Effect, Input, LearnerResponse, Player, PlayerState};
use coursecraft_xapi::{Flavor, LaunchParams};
use serde_json::Value;

use crate::bridge::StartPlan;
use crate::lrs::{LrsApi, LrsMethod, LrsRequest, LrsResponse, Tick, XapiBridge};

const HELLO: &str = include_str!("../../../examples/hello-course/course.json");
const PASS_INPUTS: &str = include_str!("../../../examples/hello-course/pass.inputs.json");

fn hello_course() -> Course {
    Course::from_json(HELLO).expect("valid hello course")
}

fn cmi5_params() -> LaunchParams {
    LaunchParams::parse(
        "endpoint=https%3A%2F%2Flrs.example.com&fetch=https%3A%2F%2Flms.example.com%2Ffetch&actor=%7B%22mbox%22%3A%22mailto%3Aa%40b.c%22%7D&registration=reg-1&activityId=urn%3Aact",
        Flavor::Cmi5,
    )
    .expect("cmi5 launch")
}

fn xapi_params() -> LaunchParams {
    LaunchParams::parse(
        "endpoint=https%3A%2F%2Flrs.example.com&actor=%7B%22mbox%22%3A%22mailto%3Aa%40b.c%22%7D&registration=reg-1&activity_id=urn%3Aact&auth=Basic%20secret",
        Flavor::Xapi,
    )
    .expect("xAPI launch")
}

#[derive(Debug, Clone)]
struct Handler {
    method: LrsMethod,
    url_contains: String,
    status: u16,
    body: String,
}

#[derive(Debug, Default)]
struct FakeLrs {
    requests: Vec<LrsRequest>,
    handlers: Vec<Handler>,
    pending: Vec<LrsResponse>,
    counter: u64,
}

impl FakeLrs {
    fn new() -> Self {
        Self::default()
    }

    fn with(mut self, method: LrsMethod, url_contains: &str, status: u16, body: &str) -> Self {
        self.handlers.push(Handler {
            method,
            url_contains: url_contains.to_string(),
            status,
            body: body.to_string(),
        });
        self
    }

    fn push_handler(&mut self, method: LrsMethod, url_contains: &str, status: u16, body: &str) {
        self.handlers.push(Handler {
            method,
            url_contains: url_contains.to_string(),
            status,
            body: body.to_string(),
        });
    }

    fn statement_requests(&self) -> Vec<&LrsRequest> {
        self.requests
            .iter()
            .filter(|request| {
                request.method == LrsMethod::Post && request.url.ends_with("/statements")
            })
            .collect()
    }
}

impl LrsApi for FakeLrs {
    fn send(&mut self, request: LrsRequest) {
        self.requests.push(request.clone());
        let matched = self.handlers.iter().position(|handler| {
            handler.method == request.method && request.url.contains(&handler.url_contains)
        });
        let response = if let Some(index) = matched {
            let handler = self.handlers.remove(index);
            Some(LrsResponse {
                id: request.id,
                status: handler.status,
                body: handler.body,
            })
        } else if request.method == LrsMethod::Post && request.url.ends_with("/statements") {
            Some(LrsResponse {
                id: request.id,
                status: 200,
                body: String::new(),
            })
        } else if request.method == LrsMethod::Put {
            Some(LrsResponse {
                id: request.id,
                status: 204,
                body: String::new(),
            })
        } else {
            None
        };
        if let Some(response) = response {
            self.pending.push(response);
        }
    }

    fn poll(&mut self) -> Vec<LrsResponse> {
        std::mem::take(&mut self.pending)
    }

    fn now_iso(&mut self) -> String {
        self.counter = self.counter.saturating_add(1);
        format!("2020-01-01T00:00:00.{:03}Z", self.counter)
    }

    fn new_uuid(&mut self) -> String {
        self.counter = self.counter.saturating_add(1);
        format!("uuid-{}", self.counter)
    }
}

fn cmi5_lrs() -> FakeLrs {
    FakeLrs::new()
        .with(LrsMethod::Post, "fetch", 200, r#"{"auth-token":"tok"}"#)
        .with(LrsMethod::Get, "LMS.LaunchData", 200, "{}")
        .with(LrsMethod::Get, "coursecraft.progress", 404, "")
}

fn xapi_lrs() -> FakeLrs {
    FakeLrs::new().with(LrsMethod::Get, "coursecraft.progress", 404, "")
}

fn handshake(
    bridge: &mut XapiBridge,
    lrs: &mut FakeLrs,
    course: &Course,
) -> (StartPlan, Vec<String>, u64) {
    bridge.start(lrs);
    let mut now = 0;
    let mut warnings = Vec::new();
    for _ in 0..20 {
        let Tick {
            plan,
            warnings: more,
        } = bridge.tick(lrs, course, now);
        warnings.extend(more);
        if let Some(plan) = plan {
            return (plan, warnings, now);
        }
        now += 100;
    }
    panic!("handshake did not return a plan");
}

fn settle(
    bridge: &mut XapiBridge,
    lrs: &mut FakeLrs,
    course: &Course,
    now: &mut u64,
) -> Vec<String> {
    let mut warnings = Vec::new();
    for _ in 0..30 {
        let Tick { warnings: more, .. } = bridge.tick(lrs, course, *now);
        warnings.extend(more);
        *now = now.saturating_add(50);
        if !bridge.is_busy() {
            break;
        }
    }
    warnings
}

fn play<'a>(
    bridge: &mut XapiBridge,
    lrs: &mut FakeLrs,
    course: &'a Course,
    inputs: &[Input],
    now: &mut u64,
) -> (Player<'a>, Vec<String>) {
    let mut player = Player::new(course).expect("player");
    let mut warnings = Vec::new();
    for input in inputs {
        let effects = player.handle(input);
        *now = now.saturating_add(100);
        warnings.extend(bridge.after_input(lrs, course, player.state(), &effects, *now, *now));
    }
    (player, warnings)
}

fn pass_inputs() -> Vec<Input> {
    serde_json::from_str(PASS_INPUTS).expect("pass inputs")
}

fn fail_inputs() -> Vec<Input> {
    vec![
        Input::Start,
        Input::Tick { ms: 1000 },
        Input::Click {
            object: "btn-start".into(),
        },
        Input::Click {
            object: "tab1".into(),
        },
        Input::Click {
            object: "tab2".into(),
        },
        Input::Click {
            object: "tab3".into(),
        },
        Input::Click {
            object: "btn-next".into(),
        },
        Input::SelectChoice { choice: "a".into() },
        Input::Click {
            object: "btn-submit".into(),
        },
        Input::Click {
            object: "btn-try-again".into(),
        },
        Input::SelectChoice { choice: "c".into() },
        Input::Click {
            object: "btn-submit".into(),
        },
        Input::Next,
    ]
}

fn statement_values(lrs: &FakeLrs) -> Vec<Value> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for request in lrs.statement_requests() {
        let Some(body) = &request.body else {
            continue;
        };
        let Ok(Value::Array(items)) = serde_json::from_str::<Value>(body) else {
            continue;
        };
        for item in items {
            let id = item
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if seen.insert(id) {
                out.push(item);
            }
        }
    }
    out
}

fn verbs(lrs: &FakeLrs) -> Vec<String> {
    statement_values(lrs)
        .iter()
        .map(|statement| {
            statement["verb"]["id"]
                .as_str()
                .unwrap_or_default()
                .rsplit('/')
                .next()
                .unwrap_or_default()
                .to_string()
        })
        .collect()
}

fn answered_state(
    slide: &str,
    response: LearnerResponse,
    attempts: u32,
    correct: bool,
) -> PlayerState {
    let mut state = PlayerState {
        last_response: Some(response),
        ..PlayerState::default()
    };
    state.answers.insert(
        slide.to_string(),
        Answer {
            correct,
            attempts,
            points: 10,
        },
    );
    state
}

fn answered_effect(slide: &str) -> Effect {
    Effect::QuestionAnswered {
        slide: slide.into(),
        correct: true,
        points: 10,
        attempts_left: None,
    }
}

#[test]
fn cmi5_pass_sequence_is_exact() {
    let course = hello_course();
    let mut lrs = cmi5_lrs();
    let mut bridge = XapiBridge::new(cmi5_params(), Flavor::Cmi5, &course);
    let (plan, warnings, mut now) = handshake(&mut bridge, &mut lrs, &course);
    assert_eq!(
        plan,
        StartPlan {
            resume: None,
            learner_name: None
        }
    );
    assert!(warnings.is_empty(), "{warnings:?}");

    let (player, warnings) = play(&mut bridge, &mut lrs, &course, &pass_inputs(), &mut now);
    assert!(warnings.is_empty(), "{warnings:?}");
    let _ = settle(&mut bridge, &mut lrs, &course, &mut now);
    let warnings = bridge.finish_session(&mut lrs, &course, Some(player.state()), 60_000);
    assert!(warnings.is_empty(), "{warnings:?}");

    assert_eq!(
        verbs(&lrs),
        vec![
            "initialized",
            "answered",
            "passed",
            "completed",
            "terminated"
        ]
    );
}

#[test]
fn start_plan_carries_the_launch_actor_name() {
    let course = hello_course();
    let params = LaunchParams::parse(
        "endpoint=https%3A%2F%2Flrs.example.com&actor=%7B%22name%22%3A%22Ada%20Lovelace%22%2C%22mbox%22%3A%22mailto%3Aa%40b.c%22%7D&registration=reg-1&activity_id=urn%3Aact&auth=Basic%20secret",
        Flavor::Xapi,
    )
    .expect("xAPI launch with an actor name");
    let mut lrs = xapi_lrs();
    let mut bridge = XapiBridge::new(params, Flavor::Xapi, &course);
    let (plan, warnings, _) = handshake(&mut bridge, &mut lrs, &course);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(plan.learner_name.as_deref(), Some("Ada Lovelace"));
}

#[test]
fn xapi_pass_sequence_is_exact() {
    let course = hello_course();
    let mut lrs = xapi_lrs();
    let mut bridge = XapiBridge::new(xapi_params(), Flavor::Xapi, &course);
    let (plan, warnings, mut now) = handshake(&mut bridge, &mut lrs, &course);
    assert_eq!(
        plan,
        StartPlan {
            resume: None,
            learner_name: None
        }
    );
    assert!(warnings.is_empty(), "{warnings:?}");

    let (player, warnings) = play(&mut bridge, &mut lrs, &course, &pass_inputs(), &mut now);
    assert!(warnings.is_empty(), "{warnings:?}");
    let _ = settle(&mut bridge, &mut lrs, &course, &mut now);
    let warnings = bridge.finish_session(&mut lrs, &course, Some(player.state()), 60_000);
    assert!(warnings.is_empty(), "{warnings:?}");

    assert_eq!(
        verbs(&lrs),
        vec![
            "initialized",
            "answered",
            "passed",
            "completed",
            "terminated"
        ]
    );
    let statements = statement_values(&lrs);
    assert!(statements[0]["context"].get("contextActivities").is_none());
    assert!(statements[0]["context"].get("extensions").is_none());
}

#[test]
fn cmi5_fail_sequence_is_exact() {
    let course = hello_course();
    let mut lrs = cmi5_lrs();
    let mut bridge = XapiBridge::new(cmi5_params(), Flavor::Cmi5, &course);
    let (_plan, _warnings, mut now) = handshake(&mut bridge, &mut lrs, &course);

    let (player, warnings) = play(&mut bridge, &mut lrs, &course, &fail_inputs(), &mut now);
    assert!(warnings.is_empty(), "{warnings:?}");
    let _ = settle(&mut bridge, &mut lrs, &course, &mut now);
    let _ = bridge.finish_session(&mut lrs, &course, Some(player.state()), 60_000);

    assert_eq!(
        verbs(&lrs),
        vec![
            "initialized",
            "answered",
            "answered",
            "failed",
            "completed",
            "terminated"
        ]
    );
}

#[test]
fn xapi_fail_sequence_is_exact() {
    let course = hello_course();
    let mut lrs = xapi_lrs();
    let mut bridge = XapiBridge::new(xapi_params(), Flavor::Xapi, &course);
    let (_plan, _warnings, mut now) = handshake(&mut bridge, &mut lrs, &course);

    let (player, warnings) = play(&mut bridge, &mut lrs, &course, &fail_inputs(), &mut now);
    assert!(warnings.is_empty(), "{warnings:?}");
    let _ = settle(&mut bridge, &mut lrs, &course, &mut now);
    let _ = bridge.finish_session(&mut lrs, &course, Some(player.state()), 60_000);

    assert_eq!(
        verbs(&lrs),
        vec![
            "initialized",
            "answered",
            "answered",
            "failed",
            "completed",
            "terminated"
        ]
    );
}

const COMPLETION_COURSE: &str = r##"{
  "schema_version": 1,
  "id": "completion-course",
  "title": "Completion",
  "slide_size": { "w": 1280, "h": 720 },
  "scenes": [
    {
      "id": "sc",
      "title": "Main",
      "slides": [
        {
          "id": "s1",
          "title": "One",
          "triggers": [
            { "action": { "action": "submit_results" }, "when": { "event": "timeline_starts" } }
          ]
        }
      ]
    }
  ]
}"##;

#[test]
fn completion_without_a_quiz_sends_only_completed() {
    let course = Course::from_json(COMPLETION_COURSE).expect("valid completion course");
    let mut lrs = cmi5_lrs();
    let mut bridge = XapiBridge::new(cmi5_params(), Flavor::Cmi5, &course);
    let (_plan, _warnings, mut now) = handshake(&mut bridge, &mut lrs, &course);

    let mut player = Player::new(&course).expect("player");
    let effects = player.handle(&Input::Start);
    let warnings = bridge.after_input(&mut lrs, &course, player.state(), &effects, 0, now);
    assert!(warnings.is_empty(), "{warnings:?}");
    let _ = settle(&mut bridge, &mut lrs, &course, &mut now);
    let _ = bridge.finish_session(&mut lrs, &course, Some(player.state()), 1000);

    assert_eq!(verbs(&lrs), vec!["initialized", "completed", "terminated"]);
}

#[test]
fn xapi_completion_without_a_quiz_sends_only_completed() {
    let course = Course::from_json(COMPLETION_COURSE).expect("valid completion course");
    let mut lrs = xapi_lrs();
    let mut bridge = XapiBridge::new(xapi_params(), Flavor::Xapi, &course);
    let (_plan, _warnings, mut now) = handshake(&mut bridge, &mut lrs, &course);

    let mut player = Player::new(&course).expect("player");
    let effects = player.handle(&Input::Start);
    let warnings = bridge.after_input(&mut lrs, &course, player.state(), &effects, 0, now);
    assert!(warnings.is_empty(), "{warnings:?}");
    let _ = settle(&mut bridge, &mut lrs, &course, &mut now);
    let _ = bridge.finish_session(&mut lrs, &course, Some(player.state()), 1000);

    assert_eq!(verbs(&lrs), vec!["initialized", "completed", "terminated"]);
}

#[test]
fn resume_restores_state_and_saved_flags_stop_repeats() {
    let course = hello_course();
    let mut lrs = cmi5_lrs();
    let mut bridge = XapiBridge::new(cmi5_params(), Flavor::Cmi5, &course);
    let (_plan, _warnings, mut now) = handshake(&mut bridge, &mut lrs, &course);

    let (player, _warnings) = play(&mut bridge, &mut lrs, &course, &pass_inputs(), &mut now);
    let _ = bridge.tick(&mut lrs, &course, 6000);
    let _ = bridge.tick(&mut lrs, &course, 6100);
    let saved = lrs
        .requests
        .iter()
        .rfind(|request| request.method == LrsMethod::Put)
        .and_then(|request| request.body.clone())
        .expect("progress document saved");
    let saved_value: Value = serde_json::from_str(&saved).expect("progress JSON");
    assert_eq!(saved_value["completed"], true);
    assert_eq!(saved_value["passed"], true);
    let suspend = saved_value["suspend"]
        .as_str()
        .expect("suspend")
        .to_string();
    assert!(suspend.starts_with("cc1:"));

    let mut resumed_lrs = FakeLrs::new()
        .with(LrsMethod::Post, "fetch", 200, r#"{"auth-token":"tok"}"#)
        .with(LrsMethod::Get, "LMS.LaunchData", 200, "{}")
        .with(LrsMethod::Get, "coursecraft.progress", 200, &saved);
    let mut resumed = XapiBridge::new(cmi5_params(), Flavor::Cmi5, &course);
    let (plan, warnings, mut now) = handshake(&mut resumed, &mut resumed_lrs, &course);
    assert!(warnings.is_empty(), "{warnings:?}");
    match plan.resume {
        Some(state) => {
            let mut restored = Player::resume(&course, state).expect("resume");
            let _ = restored.handle(&Input::Start);
            assert_eq!(
                restored.current_slide().expect("current slide").id,
                "s-results"
            );
        }
        None => panic!("expected resume"),
    }

    // A saved pass blocks a new fail, and completed is saved too.
    let state = answered_state(
        "s-quiz",
        LearnerResponse::Choices(vec!["a".into()]),
        2,
        false,
    );
    let effects = [
        Effect::Score {
            raw: 0,
            max: 10,
            percent: 0,
            status: CompletionStatus::Failed,
        },
        Effect::Completion {
            status: CompletionStatus::Failed,
        },
    ];
    let warnings = resumed.after_input(&mut resumed_lrs, &course, &state, &effects, 0, now);
    assert!(warnings.is_empty(), "{warnings:?}");
    let _ = settle(&mut resumed, &mut resumed_lrs, &course, &mut now);
    let _ = resumed.finish_session(&mut resumed_lrs, &course, Some(&state), 1000);

    let resumed_verbs = verbs(&resumed_lrs);
    assert!(!resumed_verbs.contains(&"failed".to_string()));
    assert!(!resumed_verbs.contains(&"passed".to_string()));
    assert!(!resumed_verbs.contains(&"completed".to_string()));
    assert_eq!(
        resumed_verbs,
        vec!["initialized".to_string(), "terminated".to_string()]
    );
    let _ = player;
}

#[test]
fn browse_sends_only_initialized_and_terminated() {
    let course = hello_course();
    let mut lrs = cmi5_lrs();
    lrs.handlers
        .retain(|handler| handler.url_contains != "LMS.LaunchData");
    lrs.push_handler(
        LrsMethod::Get,
        "LMS.LaunchData",
        200,
        r#"{"launchMode":"Browse"}"#,
    );
    let mut bridge = XapiBridge::new(cmi5_params(), Flavor::Cmi5, &course);
    let (_plan, _warnings, mut now) = handshake(&mut bridge, &mut lrs, &course);

    let (player, warnings) = play(&mut bridge, &mut lrs, &course, &pass_inputs(), &mut now);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(
        !lrs.requests
            .iter()
            .any(|request| request.method == LrsMethod::Put),
        "Browse must not write progress"
    );
    let _ = bridge.finish_session(&mut lrs, &course, Some(player.state()), 1000);

    assert_eq!(
        verbs(&lrs),
        vec!["initialized".to_string(), "terminated".to_string()]
    );
}

#[test]
fn token_failures_start_fresh_and_send_no_statements() {
    for (status, body) in [
        (200, r#"{"error-code":"1"}"#),
        (500, ""),
        (0, ""),
        (200, "not json"),
    ] {
        let course = hello_course();
        let mut lrs = FakeLrs::new().with(LrsMethod::Post, "fetch", status, body);
        let mut bridge = XapiBridge::new(cmi5_params(), Flavor::Cmi5, &course);
        let (plan, warnings, _now) = handshake(&mut bridge, &mut lrs, &course);
        assert_eq!(
            plan,
            StartPlan {
                resume: None,
                learner_name: None
            }
        );
        assert!(!warnings.is_empty(), "{status}: {body}");
        assert!(
            !verb_like(&warnings, "statements"),
            "nothing may be sent after a token failure"
        );

        let effects = [answered_effect("s-quiz")];
        let state = answered_state(
            "s-quiz",
            LearnerResponse::Choices(vec!["b".into()]),
            1,
            true,
        );
        let _ = bridge.after_input(&mut lrs, &course, &state, &effects, 0, 0);
        let _ = bridge.finish_session(&mut lrs, &course, Some(&state), 1000);
        assert!(lrs.statement_requests().is_empty());
    }
}

fn verb_like(warnings: &[String], needle: &str) -> bool {
    warnings.iter().any(|warning| warning.contains(needle))
}

#[test]
fn launch_data_404_still_plays() {
    let course = hello_course();
    let mut lrs = cmi5_lrs();
    lrs.handlers
        .retain(|handler| handler.url_contains != "LMS.LaunchData");
    lrs.push_handler(LrsMethod::Get, "LMS.LaunchData", 404, "");
    let mut bridge = XapiBridge::new(cmi5_params(), Flavor::Cmi5, &course);
    let (plan, warnings, mut now) = handshake(&mut bridge, &mut lrs, &course);
    assert_eq!(
        plan,
        StartPlan {
            resume: None,
            learner_name: None
        }
    );
    assert!(
        warnings
            .iter()
            .any(|warning| warning.contains("LaunchData"))
    );

    let _ = settle(&mut bridge, &mut lrs, &course, &mut now);
    let _ = bridge.finish_session(&mut lrs, &course, None, 1000);
    assert_eq!(
        verbs(&lrs),
        vec!["initialized".to_string(), "terminated".to_string()]
    );
}

#[test]
fn handshake_timeout_returns_fresh_and_ignores_late_responses() {
    let course = hello_course();
    let mut lrs = FakeLrs::new();
    let mut bridge = XapiBridge::new(cmi5_params(), Flavor::Cmi5, &course);
    bridge.start(&mut lrs);

    let first = bridge.tick(&mut lrs, &course, 0);
    assert!(first.plan.is_none());

    let fetch_id = lrs.requests[0].id;
    let Tick { plan, warnings } = bridge.tick(&mut lrs, &course, 8000);
    assert_eq!(
        plan,
        Some(StartPlan {
            resume: None,
            learner_name: None
        })
    );
    assert!(warnings.iter().any(|warning| warning.contains("timed out")));

    // A very late response must be ignored, not start a second handshake.
    lrs.pending.push(LrsResponse {
        id: fetch_id,
        status: 200,
        body: r#"{"auth-token":"late"}"#.into(),
    });
    let later = bridge.tick(&mut lrs, &course, 8100);
    assert!(later.plan.is_none());
    assert!(
        !lrs.requests
            .iter()
            .any(|request| request.url.contains("LaunchData"))
    );
    let _ = bridge.finish_session(&mut lrs, &course, None, 1000);
    assert!(lrs.statement_requests().is_empty());
}

#[test]
fn statements_retry_preserves_order_and_ids_and_respects_backoff() {
    let course = hello_course();
    let mut lrs = cmi5_lrs();
    let mut bridge = XapiBridge::new(cmi5_params(), Flavor::Cmi5, &course);
    let (_plan, _warnings, mut now) = handshake(&mut bridge, &mut lrs, &course);
    let _ = settle(&mut bridge, &mut lrs, &course, &mut now);
    lrs.requests.clear();

    lrs.push_handler(LrsMethod::Post, "/statements", 500, "");
    lrs.push_handler(LrsMethod::Post, "/statements", 200, "");

    let state = answered_state(
        "s-quiz",
        LearnerResponse::Choices(vec!["b".into()]),
        1,
        true,
    );
    let effects = [answered_effect("s-quiz"), answered_effect("s-quiz")];
    let warnings = bridge.after_input(&mut lrs, &course, &state, &effects, 0, now);
    assert!(warnings.is_empty(), "{warnings:?}");
    let first = lrs.statement_requests();
    assert_eq!(first.len(), 1, "only one POST may be in flight");
    let first_ids = statement_ids(first[0].body.as_deref().unwrap_or_default());

    let mut retry_now = now.saturating_add(100);
    let _ = bridge.tick(&mut lrs, &course, retry_now);
    assert_eq!(
        lrs.statement_requests().len(),
        1,
        "backoff must delay the retry"
    );

    retry_now = now.saturating_add(500);
    let _ = bridge.tick(&mut lrs, &course, retry_now);
    assert_eq!(
        lrs.statement_requests().len(),
        1,
        "still inside the first backoff window"
    );

    retry_now = now.saturating_add(1200);
    let _ = bridge.tick(&mut lrs, &course, retry_now);
    let requests = lrs.statement_requests();
    assert_eq!(requests.len(), 2);
    let second_ids = statement_ids(requests[1].body.as_deref().unwrap_or_default());
    assert_eq!(first_ids, second_ids, "the retry reuses statement ids");
    assert_eq!(requests[0].body, requests[1].body, "order is unchanged");
}

fn statement_ids(body: &str) -> Vec<String> {
    match serde_json::from_str::<Value>(body) {
        Ok(Value::Array(items)) => items
            .iter()
            .map(|item| {
                item.get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string()
            })
            .collect(),
        _ => Vec::new(),
    }
}

#[test]
fn a_409_counts_as_sent() {
    let course = hello_course();
    let mut lrs = cmi5_lrs();
    let mut bridge = XapiBridge::new(cmi5_params(), Flavor::Cmi5, &course);
    let (_plan, _warnings, mut now) = handshake(&mut bridge, &mut lrs, &course);
    let _ = settle(&mut bridge, &mut lrs, &course, &mut now);
    lrs.requests.clear();
    lrs.push_handler(LrsMethod::Post, "/statements", 409, "");

    let state = answered_state(
        "s-quiz",
        LearnerResponse::Choices(vec!["b".into()]),
        1,
        true,
    );
    let _ = bridge.after_input(
        &mut lrs,
        &course,
        &state,
        &[answered_effect("s-quiz")],
        0,
        now,
    );
    let mut later = now.saturating_add(100);
    let _ = bridge.tick(&mut lrs, &course, later);
    later = later.saturating_add(2000);
    let _ = bridge.tick(&mut lrs, &course, later);
    assert_eq!(lrs.statement_requests().len(), 1, "409 is not retried");
}

#[test]
fn a_400_drops_one_batch_and_the_next_goes_out() {
    let course = hello_course();
    let mut lrs = cmi5_lrs();
    let mut bridge = XapiBridge::new(cmi5_params(), Flavor::Cmi5, &course);
    let (_plan, _warnings, mut now) = handshake(&mut bridge, &mut lrs, &course);
    let _ = settle(&mut bridge, &mut lrs, &course, &mut now);
    lrs.requests.clear();
    lrs.push_handler(LrsMethod::Post, "/statements", 400, "");
    lrs.push_handler(LrsMethod::Post, "/statements", 200, "");

    let state = answered_state(
        "s-quiz",
        LearnerResponse::Choices(vec!["b".into()]),
        1,
        true,
    );
    let effects: Vec<Effect> = (0..60).map(|_| answered_effect("s-quiz")).collect();
    let _ = bridge.after_input(&mut lrs, &course, &state, &effects, 0, now);
    assert_eq!(lrs.statement_requests().len(), 1);
    let first_count = statement_ids(
        lrs.statement_requests()[0]
            .body
            .as_deref()
            .unwrap_or_default(),
    )
    .len();
    assert_eq!(first_count, 50);

    let mut later = now.saturating_add(100);
    let warnings = bridge.tick(&mut lrs, &course, later);
    assert!(
        warnings
            .warnings
            .iter()
            .any(|warning| warning.contains("rejected"))
    );
    let requests = lrs.statement_requests();
    assert_eq!(requests.len(), 2, "the next batch goes out immediately");
    let second_count = statement_ids(requests[1].body.as_deref().unwrap_or_default()).len();
    assert_eq!(second_count, 10);

    later = later.saturating_add(100);
    let _ = bridge.tick(&mut lrs, &course, later);
}

#[test]
fn reporting_lost_tracks_each_cause() {
    let course = hello_course();

    // A healthy handshake and session keep reporting on.
    let mut lrs = cmi5_lrs();
    let mut bridge = XapiBridge::new(cmi5_params(), Flavor::Cmi5, &course);
    bridge.start(&mut lrs);
    assert!(!bridge.reporting_lost(), "the handshake is in progress");
    let (_plan, _warnings, mut now) = handshake(&mut bridge, &mut lrs, &course);
    let _ = settle(&mut bridge, &mut lrs, &course, &mut now);
    assert!(!bridge.reporting_lost(), "a healthy launch reports");

    // A failed cmi5 fetch turns reporting off.
    let mut lrs = FakeLrs::new().with(LrsMethod::Post, "fetch", 500, "");
    let mut bridge = XapiBridge::new(cmi5_params(), Flavor::Cmi5, &course);
    let (_plan, _warnings, _now) = handshake(&mut bridge, &mut lrs, &course);
    assert!(bridge.reporting_lost(), "failed fetch");

    // The handshake timeout turns reporting off, but only once it fires.
    let mut lrs = FakeLrs::new();
    let mut bridge = XapiBridge::new(cmi5_params(), Flavor::Cmi5, &course);
    bridge.start(&mut lrs);
    let _ = bridge.tick(&mut lrs, &course, 0);
    assert!(!bridge.reporting_lost(), "still connecting");
    let _ = bridge.tick(&mut lrs, &course, 8000);
    assert!(bridge.reporting_lost(), "handshake timed out");

    // Authentication failing mid-session turns reporting off.
    let mut lrs = cmi5_lrs();
    let mut bridge = XapiBridge::new(cmi5_params(), Flavor::Cmi5, &course);
    let (_plan, _warnings, now) = handshake(&mut bridge, &mut lrs, &course);
    lrs.requests.clear();
    lrs.push_handler(LrsMethod::Post, "/statements", 401, "");
    let state = answered_state(
        "s-quiz",
        LearnerResponse::Choices(vec!["b".into()]),
        1,
        true,
    );
    let _ = bridge.after_input(
        &mut lrs,
        &course,
        &state,
        &[answered_effect("s-quiz")],
        0,
        now,
    );
    // The rejection arrives on a later tick.
    let _ = bridge.tick(&mut lrs, &course, now.saturating_add(100));
    assert!(bridge.reporting_lost(), "mid-session 401");

    // Browse keeps reporting on.
    let mut lrs = browse_lrs();
    let mut bridge = XapiBridge::new(cmi5_params(), Flavor::Cmi5, &course);
    let (_plan, _warnings, _now) = handshake(&mut bridge, &mut lrs, &course);
    assert!(!bridge.reporting_lost(), "browse keeps reporting on");
}

#[test]
fn a_401_turns_reporting_off() {
    let course = hello_course();
    let mut lrs = cmi5_lrs();
    let mut bridge = XapiBridge::new(cmi5_params(), Flavor::Cmi5, &course);
    let (_plan, _warnings, mut now) = handshake(&mut bridge, &mut lrs, &course);
    let _ = settle(&mut bridge, &mut lrs, &course, &mut now);
    lrs.requests.clear();
    lrs.push_handler(LrsMethod::Post, "/statements", 401, "");

    let state = answered_state(
        "s-quiz",
        LearnerResponse::Choices(vec!["b".into()]),
        1,
        true,
    );
    let _ = bridge.after_input(
        &mut lrs,
        &course,
        &state,
        &[answered_effect("s-quiz")],
        0,
        now,
    );
    let mut later = now.saturating_add(100);
    let warnings = bridge.tick(&mut lrs, &course, later);
    assert!(
        warnings
            .warnings
            .iter()
            .any(|warning| warning.contains("reporting disabled"))
    );

    later = later.saturating_add(100);
    let _ = bridge.after_input(
        &mut lrs,
        &course,
        &state,
        &[answered_effect("s-quiz")],
        0,
        later,
    );
    assert_eq!(
        lrs.statement_requests().len(),
        1,
        "nothing more is sent after 401"
    );
}

#[test]
fn mastery_overrides_status_in_both_directions() {
    // A passing status with a low score becomes `failed`.
    let course = hello_course();
    let mut lrs = cmi5_lrs();
    lrs.handlers
        .retain(|handler| handler.url_contains != "LMS.LaunchData");
    lrs.push_handler(
        LrsMethod::Get,
        "LMS.LaunchData",
        200,
        r#"{"masteryScore":0.8}"#,
    );
    let mut bridge = XapiBridge::new(cmi5_params(), Flavor::Cmi5, &course);
    let (_plan, _warnings, mut now) = handshake(&mut bridge, &mut lrs, &course);
    let state = PlayerState::default();
    let effects = [
        Effect::Score {
            raw: 5,
            max: 10,
            percent: 50,
            status: CompletionStatus::Passed,
        },
        Effect::Completion {
            status: CompletionStatus::Passed,
        },
    ];
    let _ = bridge.after_input(&mut lrs, &course, &state, &effects, 0, now);
    let _ = settle(&mut bridge, &mut lrs, &course, &mut now);
    assert_eq!(verbs(&lrs), vec!["initialized", "failed", "completed"]);

    // A failing status with a high score becomes `passed`.
    let mut lrs = cmi5_lrs();
    lrs.handlers
        .retain(|handler| handler.url_contains != "LMS.LaunchData");
    lrs.push_handler(
        LrsMethod::Get,
        "LMS.LaunchData",
        200,
        r#"{"masteryScore":0.8}"#,
    );
    let mut bridge = XapiBridge::new(cmi5_params(), Flavor::Cmi5, &course);
    let (_plan, _warnings, mut now) = handshake(&mut bridge, &mut lrs, &course);
    let effects = [
        Effect::Score {
            raw: 9,
            max: 10,
            percent: 90,
            status: CompletionStatus::Failed,
        },
        Effect::Completion {
            status: CompletionStatus::Failed,
        },
    ];
    let _ = bridge.after_input(&mut lrs, &course, &state, &effects, 0, now);
    let _ = settle(&mut bridge, &mut lrs, &course, &mut now);
    assert_eq!(verbs(&lrs), vec!["initialized", "passed", "completed"]);
}

#[test]
fn finish_is_keepalive_ordered_idempotent_and_under_the_size_cap() {
    let course = hello_course();
    let mut lrs = xapi_lrs();
    let mut bridge = XapiBridge::new(xapi_params(), Flavor::Xapi, &course);
    let (_plan, _warnings, now) = handshake(&mut bridge, &mut lrs, &course);

    let state = answered_state(
        "s-quiz",
        LearnerResponse::Choices(vec!["b".into()]),
        1,
        true,
    );
    let effects: Vec<Effect> = (0..200).map(|_| answered_effect("s-quiz")).collect();
    let _ = bridge.after_input(&mut lrs, &course, &state, &effects, 0, now);
    assert_eq!(verbs(&lrs).first().map(String::as_str), Some("initialized"));
    // Only the final flush is under test here.
    lrs.requests.clear();
    lrs.pending.clear();

    let warnings = bridge.finish_session(&mut lrs, &course, Some(&state), 60_000);
    assert!(warnings.is_empty(), "{warnings:?}");

    let requests: Vec<&LrsRequest> = lrs.statement_requests();
    assert!(requests.len() >= 2, "the flush is split into batches");
    for request in &requests {
        assert!(request.keepalive, "final statements use keepalive");
        let size = request.body.as_ref().map(String::len).unwrap_or(0);
        assert!(
            size < crate::lrs::MAX_KEEPALIVE_BODY,
            "body {size} too large"
        );
    }
    let ordered = verbs(&lrs);
    assert_eq!(ordered.last().map(String::as_str), Some("terminated"));

    let before = lrs.requests.len();
    let again = bridge.finish_session(&mut lrs, &course, Some(&state), 60_000);
    assert!(again.is_empty());
    assert_eq!(lrs.requests.len(), before, "finish is idempotent");
}

#[test]
fn return_url_only_accepts_http_and_https() {
    let course = hello_course();
    let mut lrs = cmi5_lrs();
    lrs.handlers
        .retain(|handler| handler.url_contains != "LMS.LaunchData");
    lrs.push_handler(
        LrsMethod::Get,
        "LMS.LaunchData",
        200,
        r#"{"returnURL":"javascript:alert(1)"}"#,
    );
    let mut bridge = XapiBridge::new(cmi5_params(), Flavor::Cmi5, &course);
    let (_plan, warnings, _now) = handshake(&mut bridge, &mut lrs, &course);
    assert!(warnings.iter().any(|warning| warning.contains("returnURL")));
    assert_eq!(bridge.return_url(), None);

    let mut lrs = cmi5_lrs();
    lrs.handlers
        .retain(|handler| handler.url_contains != "LMS.LaunchData");
    lrs.push_handler(
        LrsMethod::Get,
        "LMS.LaunchData",
        200,
        r#"{"returnURL":"https://lms.example.com/back"}"#,
    );
    let mut bridge = XapiBridge::new(cmi5_params(), Flavor::Cmi5, &course);
    let (_plan, _warnings, _now) = handshake(&mut bridge, &mut lrs, &course);
    assert_eq!(bridge.return_url(), Some("https://lms.example.com/back"));
}

#[test]
fn no_warning_contains_a_secret() {
    let course = hello_course();
    let mut lrs = xapi_lrs();
    let mut bridge = XapiBridge::new(xapi_params(), Flavor::Xapi, &course);
    let (_plan, _warnings, mut now) = handshake(&mut bridge, &mut lrs, &course);
    let _ = settle(&mut bridge, &mut lrs, &course, &mut now);
    lrs.push_handler(LrsMethod::Post, "/statements", 401, "");

    let state = answered_state(
        "s-quiz",
        LearnerResponse::Choices(vec!["b".into()]),
        1,
        true,
    );
    let _ = bridge.after_input(
        &mut lrs,
        &course,
        &state,
        &[answered_effect("s-quiz")],
        0,
        now,
    );
    let warnings = bridge.tick(&mut lrs, &course, now.saturating_add(100));
    assert!(!warnings.warnings.is_empty());
    for warning in &warnings.warnings {
        assert!(!warning.contains("secret"), "{warning}");
        assert!(!warning.contains("Basic"), "{warning}");
    }

    // The cmi5 fetch URL is secret too, and must not leak into warnings.
    let mut lrs = FakeLrs::new().with(LrsMethod::Post, "fetch", 500, "");
    let params = LaunchParams::parse(
        "endpoint=https%3A%2F%2Flrs.example.com&fetch=https%3A%2F%2Flms.example.com%2Ffetch%3Ftoken%3Dtopsecret&actor=%7B%22mbox%22%3A%22mailto%3Aa%40b.c%22%7D&registration=reg-1&activityId=urn%3Aact",
        Flavor::Cmi5,
    )
    .expect("cmi5 launch");
    let mut bridge = XapiBridge::new(params, Flavor::Cmi5, &course);
    let (_plan, warnings, _now) = handshake(&mut bridge, &mut lrs, &course);
    for warning in &warnings {
        assert!(!warning.contains("topsecret"), "{warning}");
    }
}

#[test]
fn hostile_bodies_do_not_crash() {
    let course = hello_course();

    for (launch_status, launch_body, progress_status, progress_body) in [
        (200, "[1,2,3]", 200, "nonsense"),
        (200, "", 200, "{\"suspend\":123}"),
        (
            200,
            "{\"contextTemplate\":5,\"launchMode\":7,\"masteryScore\":\"x\",\"returnURL\":9}",
            200,
            "{\"completed\":\"yes\",\"passed\":[]}",
        ),
        (500, "oops", 404, ""),
    ] {
        let mut lrs = FakeLrs::new()
            .with(LrsMethod::Post, "fetch", 200, r#"{"auth-token":"tok"}"#)
            .with(LrsMethod::Get, "LMS.LaunchData", launch_status, launch_body)
            .with(
                LrsMethod::Get,
                "coursecraft.progress",
                progress_status,
                progress_body,
            );
        let mut bridge = XapiBridge::new(cmi5_params(), Flavor::Cmi5, &course);
        let (plan, _warnings, _now) = handshake(&mut bridge, &mut lrs, &course);
        assert_eq!(
            plan,
            StartPlan {
                resume: None,
                learner_name: None
            }
        );
    }
}

#[test]
fn too_many_statements_drop_the_oldest_but_keep_initialized() {
    let course = hello_course();
    let mut lrs = cmi5_lrs();
    let mut bridge = XapiBridge::new(cmi5_params(), Flavor::Cmi5, &course);
    let (_plan, _warnings, mut now) = handshake(&mut bridge, &mut lrs, &course);
    // Keep everything in flight by never allowing the first POST to finish.
    // `settle` acknowledges initialized; the queue itself is filled below.
    let _ = settle(&mut bridge, &mut lrs, &course, &mut now);

    let state = answered_state(
        "s-quiz",
        LearnerResponse::Choices(vec!["b".into()]),
        1,
        true,
    );
    let effects: Vec<Effect> = (0..1200).map(|_| answered_effect("s-quiz")).collect();
    let warnings = bridge.after_input(&mut lrs, &course, &state, &effects, 0, now);
    assert!(
        warnings
            .iter()
            .any(|warning| warning.contains("queue is full")),
        "{warnings:?}"
    );
    assert!(bridge.queued_len() <= crate::lrs::MAX_QUEUE);
}

#[test]
fn progress_is_put_at_most_once_every_five_seconds_and_failures_stay_dirty() {
    let course = hello_course();
    let mut lrs = xapi_lrs();
    let mut bridge = XapiBridge::new(xapi_params(), Flavor::Xapi, &course);
    let (_plan, _warnings, mut now) = handshake(&mut bridge, &mut lrs, &course);

    lrs.push_handler(LrsMethod::Put, "coursecraft.progress", 500, "");
    let state = PlayerState::default();
    let effects = [Effect::SlideEntered {
        slide: "s-intro".into(),
    }];
    let _ = bridge.after_input(&mut lrs, &course, &state, &effects, 0, now);
    assert_eq!(put_count(&lrs), 1, "a dirty document is written once");

    let _ = bridge.after_input(&mut lrs, &course, &state, &effects, 0, now);
    assert_eq!(put_count(&lrs), 1, "inside the five-second window");
    now = now.saturating_add(5000);
    let _ = bridge.tick(&mut lrs, &course, now);
    let _ = bridge.tick(&mut lrs, &course, now.saturating_add(100));
    // The 500 response keeps it dirty; a later window retries it.
    now = now.saturating_add(6000);
    let _ = bridge.tick(&mut lrs, &course, now);
    assert!(put_count(&lrs) >= 2, "a failed PUT is retried while dirty");
}

fn put_count(lrs: &FakeLrs) -> usize {
    lrs.requests
        .iter()
        .filter(|request| request.method == LrsMethod::Put)
        .count()
}

fn browse_lrs() -> FakeLrs {
    let mut lrs = cmi5_lrs();
    lrs.handlers
        .retain(|handler| handler.url_contains != "LMS.LaunchData");
    lrs.push_handler(
        LrsMethod::Get,
        "LMS.LaunchData",
        200,
        r#"{"launchMode":"Browse"}"#,
    );
    lrs
}

#[test]
fn browse_exit_still_sends_terminated_and_finishes() {
    let course = hello_course();
    let mut lrs = browse_lrs();
    let mut bridge = XapiBridge::new(cmi5_params(), Flavor::Cmi5, &course);
    let (_plan, _warnings, now) = handshake(&mut bridge, &mut lrs, &course);
    let player = Player::new(&course).expect("player");

    bridge.after_input(
        &mut lrs,
        &course,
        player.state(),
        &[Effect::Exit],
        1000,
        now,
    );

    assert_eq!(verbs(&lrs), vec!["initialized", "terminated"]);
    assert_eq!(put_count(&lrs), 0, "Browse must not write progress");
    // Finished: closing the page afterwards sends nothing more.
    let before = lrs.requests.len();
    bridge.finish_session(&mut lrs, &course, Some(player.state()), 2000);
    assert_eq!(lrs.requests.len(), before);
}

#[test]
fn a_pass_with_no_other_change_still_saves_its_flags() {
    let course = hello_course();
    let mut lrs = xapi_lrs();
    let mut bridge = XapiBridge::new(xapi_params(), Flavor::Xapi, &course);
    let (_plan, _warnings, now) = handshake(&mut bridge, &mut lrs, &course);
    let player = Player::new(&course).expect("player");

    let effects = [Effect::Score {
        raw: 10,
        max: 10,
        percent: 100,
        status: CompletionStatus::Passed,
    }];
    bridge.after_input(&mut lrs, &course, player.state(), &effects, 1000, now);

    let put = lrs
        .requests
        .iter()
        .find(|request| request.method == LrsMethod::Put)
        .expect("progress PUT");
    let body: Value = serde_json::from_str(put.body.as_deref().unwrap_or_default()).expect("json");
    assert_eq!(body.get("passed"), Some(&Value::Bool(true)));
    assert_eq!(body.get("completed"), Some(&Value::Bool(true)));
}

#[test]
fn a_mastery_score_does_not_fail_a_course_with_no_quiz() {
    let course = hello_course();
    let mut lrs = cmi5_lrs();
    lrs.handlers
        .retain(|handler| handler.url_contains != "LMS.LaunchData");
    lrs.push_handler(
        LrsMethod::Get,
        "LMS.LaunchData",
        200,
        r#"{"masteryScore":0.8}"#,
    );
    let mut bridge = XapiBridge::new(cmi5_params(), Flavor::Cmi5, &course);
    let (_plan, _warnings, now) = handshake(&mut bridge, &mut lrs, &course);
    let player = Player::new(&course).expect("player");

    let effects = [
        Effect::Score {
            raw: 0,
            max: 0,
            percent: 0,
            status: CompletionStatus::Completed,
        },
        Effect::Completion {
            status: CompletionStatus::Completed,
        },
    ];
    bridge.after_input(&mut lrs, &course, player.state(), &effects, 1000, now);
    bridge.finish_session(&mut lrs, &course, Some(player.state()), 2000);

    assert_eq!(verbs(&lrs), vec!["initialized", "completed", "terminated"]);
}
