use std::collections::BTreeMap;

use coursecraft_course::{CompletionStatus, Course, QuestionKind, Value, VariableDef};
use coursecraft_logic::{
    Answer, Effect, Input, LearnerResponse, Player, PlayerState, encode_suspend,
};

use crate::api::CmiApi;
use crate::bridge::{LOCATION_LIMIT_2004, LmsBridge, SUSPEND_LIMIT, SUSPEND_LIMIT_2004, StartPlan};
use crate::interaction::{InteractionKind, Response, interaction_record, sanitize_identifier};
use crate::time::{format_scaled_score, format_session_time, format_session_time_iso};
use coursecraft_scorm::Standard;

const HELLO: &str = include_str!("../../../examples/hello-course/course.json");
const PASS_INPUTS: &str = include_str!("../../../examples/hello-course/pass.inputs.json");
const SHOWCASE: &str = include_str!("../../../examples/showcase/course.json");

fn hello_course() -> Course {
    Course::from_json(HELLO).expect("valid hello course")
}

fn showcase_course() -> Course {
    Course::from_json(SHOWCASE).expect("valid showcase course")
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

fn click(id: &str) -> Input {
    Input::Click {
        object: id.to_string(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Call {
    Initialize,
    GetValue(String),
    SetValue(String, String),
    Commit,
    Finish,
}

#[derive(Debug, Default)]
pub struct FakeApi {
    pub calls: Vec<Call>,
    pub store: BTreeMap<String, String>,
    pub fail_initialize: bool,
    pub fail_set_value: bool,
    pub last_err: String,
}

impl FakeApi {
    pub fn new() -> Self {
        let mut store = BTreeMap::new();
        store.insert("cmi.core.lesson_status".into(), "not attempted".into());
        store.insert("cmi.interactions._count".into(), "0".into());
        Self {
            store,
            last_err: "no error".into(),
            ..Self::default()
        }
    }
}

impl CmiApi for FakeApi {
    fn initialize(&mut self) -> bool {
        self.calls.push(Call::Initialize);
        if self.fail_initialize {
            self.last_err = "101: General Exception".into();
            false
        } else {
            true
        }
    }

    fn get_value(&mut self, key: &str) -> String {
        self.calls.push(Call::GetValue(key.to_string()));
        self.store.get(key).cloned().unwrap_or_default()
    }

    fn set_value(&mut self, key: &str, value: &str) -> bool {
        self.calls
            .push(Call::SetValue(key.to_string(), value.to_string()));
        if self.fail_set_value {
            self.last_err = "401: General Set Failure".into();
            false
        } else {
            self.store.insert(key.to_string(), value.to_string());
            true
        }
    }

    fn commit(&mut self) -> bool {
        self.calls.push(Call::Commit);
        true
    }

    fn finish(&mut self) -> bool {
        self.calls.push(Call::Finish);
        true
    }

    fn last_error(&mut self) -> String {
        self.last_err.clone()
    }
}

fn passed_status_writes(api: &FakeApi) -> usize {
    api.calls
        .iter()
        .filter(|c| **c == Call::SetValue("cmi.core.lesson_status".into(), "passed".into()))
        .count()
}

#[test]
fn fresh_start_sets_incomplete_and_returns_fresh() {
    let course = hello_course();
    let mut api = FakeApi::new();
    assert_eq!(
        api.store.get("cmi.core.lesson_status").map(String::as_str),
        Some("not attempted")
    );

    let mut bridge = LmsBridge::new();
    let (plan, warnings) = bridge.start(&mut api, &course);
    assert_eq!(
        plan,
        StartPlan {
            resume: None,
            learner_name: None
        }
    );
    assert!(warnings.is_empty());
    assert_eq!(
        api.store.get("cmi.core.lesson_status").map(String::as_str),
        Some("incomplete")
    );
    assert!(bridge.is_active());
    assert!(!bridge.is_completed());
}

#[test]
fn start_reads_the_scorm12_learner_name() {
    let course = hello_course();
    let mut api = FakeApi::new();
    let mut bridge = LmsBridge::new();
    let (plan, _) = bridge.start(&mut api, &course);
    assert_eq!(plan.learner_name, None);

    api.store
        .insert("cmi.core.student_name".into(), "Ada Lovelace".into());
    let mut bridge = LmsBridge::new();
    let (plan, _) = bridge.start(&mut api, &course);
    assert_eq!(plan.learner_name.as_deref(), Some("Ada Lovelace"));
}

#[test]
fn start_reads_the_scorm2004_learner_name() {
    let course = hello_course();
    let mut api = Fake2004Api::new();
    let mut bridge = LmsBridge::for_standard(Standard::Scorm2004);
    let (plan, _) = bridge.start(&mut api, &course);
    assert_eq!(plan.learner_name.as_deref(), Some("Learner, Test"));

    api.store.insert("cmi.learner_name".into(), String::new());
    let mut bridge = LmsBridge::for_standard(Standard::Scorm2004);
    let (plan, _) = bridge.start(&mut api, &course);
    assert_eq!(plan.learner_name, None);
}

#[test]
fn pass_script_reports_passed_score_and_location() {
    let course = hello_course();
    let mut player = Player::new(&course).expect("player new");
    let mut api = FakeApi::new();
    let mut bridge = LmsBridge::new();
    let (plan, warnings) = bridge.start(&mut api, &course);
    assert_eq!(
        plan,
        StartPlan {
            resume: None,
            learner_name: None
        }
    );
    assert!(warnings.is_empty());

    let inputs: Vec<Input> = serde_json::from_str(PASS_INPUTS).expect("deserialize pass inputs");
    let mut expected_commits = 0;
    let mut dual_status_inputs = 0;
    for input in &inputs {
        let effects = player.handle(input);
        let has_score = effects.iter().any(|e| matches!(e, Effect::Score { .. }));
        let has_completion = effects
            .iter()
            .any(|e| matches!(e, Effect::Completion { .. }));
        let passed_writes_before = passed_status_writes(&api);
        if effects.iter().any(|e| {
            matches!(
                e,
                Effect::SlideEntered { .. } | Effect::Score { .. } | Effect::Completion { .. }
            )
        }) {
            expected_commits += 1;
        }
        let warnings = bridge.after_input(&mut api, &course, player.state(), &effects, 0);
        assert!(warnings.is_empty());
        if has_score && has_completion {
            dual_status_inputs += 1;
            assert_eq!(passed_status_writes(&api) - passed_writes_before, 1);
        }
    }
    assert_eq!(dual_status_inputs, 1);
    assert_eq!(passed_status_writes(&api), 1);

    assert_eq!(
        api.store.get("cmi.core.lesson_status").map(String::as_str),
        Some("passed")
    );
    assert_eq!(
        api.store.get("cmi.core.score.raw").map(String::as_str),
        Some("100")
    );
    assert_eq!(
        api.store.get("cmi.core.score.min").map(String::as_str),
        Some("0")
    );
    assert_eq!(
        api.store.get("cmi.core.score.max").map(String::as_str),
        Some("100")
    );
    assert_eq!(
        api.store
            .get("cmi.core.lesson_location")
            .map(String::as_str),
        Some("s-results")
    );
    let suspend = api
        .store
        .get("cmi.suspend_data")
        .expect("suspend_data present");
    assert!(suspend.starts_with("cc1:"));

    let commit_count = api.calls.iter().filter(|c| **c == Call::Commit).count();
    assert_eq!(commit_count, expected_commits);
    assert_eq!(commit_count, 4);
}

#[test]
fn fail_path_reports_failed_zero() {
    let course = hello_course();
    let mut player = Player::new(&course).expect("player new");
    let mut api = FakeApi::new();
    let mut bridge = LmsBridge::new();
    let (plan, warnings) = bridge.start(&mut api, &course);
    assert_eq!(
        plan,
        StartPlan {
            resume: None,
            learner_name: None
        }
    );
    assert!(warnings.is_empty());

    let fail_inputs = [
        Input::Start,
        Input::Tick { ms: 1000 },
        click("btn-start"),
        click("tab1"),
        click("tab2"),
        click("tab3"),
        click("btn-next"),
        Input::SelectChoice {
            choice: "a".to_string(),
        },
        click("btn-submit"),
        click("btn-try-again"),
        Input::SelectChoice {
            choice: "c".to_string(),
        },
        click("btn-submit"),
        Input::Next,
    ];

    for input in &fail_inputs {
        let effects = player.handle(input);
        let warnings = bridge.after_input(&mut api, &course, player.state(), &effects, 0);
        assert!(warnings.is_empty());
    }

    assert_eq!(
        api.store.get("cmi.core.lesson_status").map(String::as_str),
        Some("failed")
    );
    assert_eq!(
        api.store.get("cmi.core.score.raw").map(String::as_str),
        Some("0")
    );
    assert_eq!(
        api.store.get("cmi.core.score.min").map(String::as_str),
        Some("0")
    );
    assert_eq!(
        api.store.get("cmi.core.score.max").map(String::as_str),
        Some("100")
    );
    assert_eq!(
        api.store
            .get("cmi.core.lesson_location")
            .map(String::as_str),
        Some("s-results")
    );
}

#[test]
fn resume_returns_saved_state_and_lands_on_same_slide() {
    let course = hello_course();
    let mut player = Player::new(&course).expect("player new");
    let mut api = FakeApi::new();
    let mut bridge = LmsBridge::new();
    let (plan, _) = bridge.start(&mut api, &course);
    assert_eq!(
        plan,
        StartPlan {
            resume: None,
            learner_name: None
        }
    );

    let inputs: Vec<Input> = serde_json::from_str(PASS_INPUTS).expect("deserialize pass inputs");
    // Run half of pass script
    for input in &inputs[..5] {
        let effects = player.handle(input);
        bridge.after_input(&mut api, &course, player.state(), &effects, 0);
    }
    let mid_slide_id = player.current_slide().expect("current slide").id.clone();

    let mut new_api = FakeApi {
        store: api.store.clone(),
        ..FakeApi::new()
    };
    let mut new_bridge = LmsBridge::new();
    let (plan, warnings) = new_bridge.start(&mut new_api, &course);
    assert!(warnings.is_empty());

    match plan.resume {
        Some(saved_state) => {
            let mut resumed_player = Player::resume(&course, saved_state).expect("resume player");
            let effects = resumed_player.handle(&Input::Start);
            assert_eq!(
                resumed_player.current_slide().expect("slide").id,
                mid_slide_id
            );
            assert_eq!(
                effects.first(),
                Some(&Effect::SlideEntered {
                    slide: mid_slide_id
                })
            );
        }
        None => panic!("expected a resume plan"),
    }
}

#[test]
fn oversized_suspend_data_is_skipped_with_warning() {
    let mut course = hello_course();
    course.variables.push(VariableDef {
        name: "long_text".into(),
        value: Value::Text(String::new()),
    });
    let mut player = Player::new(&course).expect("player new");
    let _ = player.handle(&Input::Start);

    let mut api = FakeApi::new();
    let mut bridge = LmsBridge::new();
    let (plan, warnings) = bridge.start(&mut api, &course);
    assert_eq!(
        plan,
        StartPlan {
            resume: None,
            learner_name: None
        }
    );
    assert!(warnings.is_empty());

    let big_string = "x".repeat(5000);
    let mut state = player.state().clone();
    state
        .variables
        .insert("long_text".into(), Value::Text(big_string));

    let encoded = encode_suspend(&course, &state);
    assert!(encoded.len() > SUSPEND_LIMIT);

    let effects = [Effect::VariableChanged {
        variable: "long_text".into(),
        value: state.variables["long_text"].clone(),
    }];
    let warnings = bridge.after_input(&mut api, &course, &state, &effects, 0);
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("exceeds limit 4096"));
    assert!(!api.store.contains_key("cmi.suspend_data"));
}

#[test]
fn failed_initialize_makes_bridge_inactive() {
    let course = hello_course();
    let mut api = FakeApi::new();
    api.fail_initialize = true;

    let mut bridge = LmsBridge::new();
    let (plan, warnings) = bridge.start(&mut api, &course);
    assert_eq!(
        plan,
        StartPlan {
            resume: None,
            learner_name: None
        }
    );
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("LMSInitialize failed"));
    assert!(!bridge.is_active());
    assert_eq!(api.calls.len(), 1);

    let player = Player::new(&course).expect("player new");
    let effects = [Effect::SlideEntered {
        slide: "s-intro".into(),
    }];
    let warnings = bridge.after_input(&mut api, &course, player.state(), &effects, 0);
    assert!(warnings.is_empty());
    assert_eq!(api.calls.len(), 1);

    let warnings = bridge.finish_session(&mut api, 1000);
    assert!(warnings.is_empty());
    assert_eq!(api.calls.len(), 1);
}

#[test]
fn finish_sets_exit_and_session_time_once() {
    // Incomplete case:
    let course = hello_course();
    let mut api = FakeApi::new();
    let mut bridge = LmsBridge::new();
    let (plan, warnings) = bridge.start(&mut api, &course);
    assert_eq!(
        plan,
        StartPlan {
            resume: None,
            learner_name: None
        }
    );
    assert!(warnings.is_empty());

    let warnings = bridge.finish_session(&mut api, 5000);
    assert!(warnings.is_empty());
    assert_eq!(
        api.store.get("cmi.core.exit").map(String::as_str),
        Some("suspend")
    );
    assert_eq!(
        api.store.get("cmi.core.session_time").map(String::as_str),
        Some("0000:00:05.00")
    );
    assert!(bridge.is_finished());

    // Second call does nothing
    let calls_before = api.calls.len();
    let warnings = bridge.finish_session(&mut api, 10000);
    assert!(warnings.is_empty());
    assert_eq!(api.calls.len(), calls_before);

    // Passed case:
    let mut api2 = FakeApi::new();
    let mut bridge2 = LmsBridge::new();
    let (plan, warnings) = bridge2.start(&mut api2, &course);
    assert_eq!(
        plan,
        StartPlan {
            resume: None,
            learner_name: None
        }
    );
    assert!(warnings.is_empty());

    let player = Player::new(&course).expect("player new");
    let effects = [
        Effect::Score {
            raw: 10,
            max: 10,
            percent: 100,
            status: CompletionStatus::Passed,
        },
        Effect::Completion {
            status: CompletionStatus::Passed,
        },
    ];
    let warnings = bridge2.after_input(&mut api2, &course, player.state(), &effects, 0);
    assert!(warnings.is_empty());
    assert!(bridge2.is_completed());

    let warnings = bridge2.finish_session(&mut api2, 120_000);
    assert!(warnings.is_empty());
    assert_eq!(
        api2.store.get("cmi.core.exit").map(String::as_str),
        Some("")
    );
    assert_eq!(
        api2.store.get("cmi.core.session_time").map(String::as_str),
        Some("0000:02:00.00")
    );

    let calls_before2 = api2.calls.len();
    let warnings = bridge2.finish_session(&mut api2, 130_000);
    assert!(warnings.is_empty());
    assert_eq!(api2.calls.len(), calls_before2);
}

#[test]
fn session_time_formats_and_saturates() {
    assert_eq!(format_session_time(0), "0000:00:00.00");
    assert_eq!(format_session_time(3_723_450), "0001:02:03.45");
    assert_eq!(format_session_time(u64::MAX), "9999:99:99.99");
    assert_eq!(format_session_time(10_000 * 3600 * 1000), "9999:99:99.99");
}

#[test]
fn corrupt_suspend_data_warns_and_starts_fresh() {
    let course = hello_course();
    let mut api = FakeApi::new();
    api.store
        .insert("cmi.suspend_data".into(), "cc1:corrupted-bytes".into());
    let mut bridge = LmsBridge::new();
    let (plan, warnings) = bridge.start(&mut api, &course);
    assert_eq!(
        plan,
        StartPlan {
            resume: None,
            learner_name: None
        }
    );
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("failed to decode suspend_data"));
}

#[test]
fn set_value_failure_produces_warning() {
    let course = hello_course();
    let mut api = FakeApi::new();
    api.fail_set_value = true;
    let mut bridge = LmsBridge::new();
    let (plan, warnings) = bridge.start(&mut api, &course);
    assert_eq!(
        plan,
        StartPlan {
            resume: None,
            learner_name: None
        }
    );
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("401: General Set Failure"));
}

#[test]
fn unchanged_suspend_data_is_not_rewritten() {
    let course = hello_course();
    let mut player = Player::new(&course).expect("player new");
    let mut api = FakeApi::new();
    let mut bridge = LmsBridge::new();
    let _ = bridge.start(&mut api, &course);

    let effects1 = player.handle(&Input::Start);
    let _ = bridge.after_input(&mut api, &course, player.state(), &effects1, 0);
    let sets_before = api
        .calls
        .iter()
        .filter(|c| matches!(c, Call::SetValue(k, _) if k == "cmi.suspend_data"))
        .count();
    assert_eq!(sets_before, 1);

    // Call after_input with an effect that changes durable state, but state itself didn't change
    let effects2 = [Effect::StateChanged {
        object: "dummy".into(),
        state: "normal".into(),
    }];
    let _ = bridge.after_input(&mut api, &course, player.state(), &effects2, 0);
    let sets_after = api
        .calls
        .iter()
        .filter(|c| matches!(c, Call::SetValue(k, _) if k == "cmi.suspend_data"))
        .count();
    assert_eq!(sets_after, sets_before);
}

#[test]
fn exit_effect_finishes_session_with_session_time() {
    const EXIT_COURSE: &str = r##"{
  "schema_version": 1,
  "id": "exit-course",
  "title": "Exit",
  "slide_size": { "w": 1280, "h": 720 },
  "scenes": [
    {
      "id": "sc",
      "title": "Main",
      "slides": [
        {
          "id": "s1",
          "title": "One",
          "objects": [
            { "id": "btn-exit", "kind": { "type": "button", "label": "Exit" }, "frame": { "x": 80, "y": 520, "w": 200, "h": 60 } }
          ],
          "triggers": [
            { "action": { "action": "exit_course" }, "when": { "event": "click", "object": "btn-exit" } }
          ]
        }
      ]
    }
  ]
}"##;
    let course = Course::from_json(EXIT_COURSE).expect("valid exit course");
    let mut player = Player::new(&course).expect("player new");
    let mut api = FakeApi::new();
    let mut bridge = LmsBridge::new();
    let (plan, warnings) = bridge.start(&mut api, &course);
    assert_eq!(
        plan,
        StartPlan {
            resume: None,
            learner_name: None
        }
    );
    assert!(warnings.is_empty());

    let started = player.handle(&Input::Start);
    let warnings = bridge.after_input(&mut api, &course, player.state(), &started, 0);
    assert!(warnings.is_empty());
    assert!(!bridge.is_finished());

    let effects = player.handle(&click("btn-exit"));
    assert!(effects.iter().any(|e| matches!(e, Effect::Exit)));
    let warnings = bridge.after_input(&mut api, &course, player.state(), &effects, 3_723_450);
    assert!(warnings.is_empty());
    assert!(bridge.is_finished());
    assert_eq!(
        api.store.get("cmi.core.session_time").map(String::as_str),
        Some("0001:02:03.45")
    );
    assert_eq!(api.calls.last(), Some(&Call::Finish));

    let calls_before = api.calls.len();
    let warnings = bridge.after_input(&mut api, &course, player.state(), &effects, 9_999);
    assert!(warnings.is_empty());
    assert_eq!(api.calls.len(), calls_before);
}

/// A SCORM 2004 fake that enforces the 2004 data-model vocabulary, so a
/// 1.2 element name or an out-of-range value shows up as a warning.
#[derive(Debug, Default)]
pub struct Fake2004Api {
    pub calls: Vec<Call>,
    pub store: BTreeMap<String, String>,
    pub last_err: String,
}

impl Fake2004Api {
    pub fn new() -> Self {
        let mut store = BTreeMap::new();
        store.insert("cmi.completion_status".into(), "unknown".into());
        store.insert("cmi.success_status".into(), "unknown".into());
        store.insert("cmi.entry".into(), "ab-initio".into());
        store.insert("cmi.exit".into(), String::new());
        store.insert("cmi.location".into(), String::new());
        store.insert("cmi.session_time".into(), String::new());
        store.insert("cmi.suspend_data".into(), String::new());
        store.insert("cmi.score.min".into(), String::new());
        store.insert("cmi.score.max".into(), String::new());
        store.insert("cmi.score.raw".into(), String::new());
        store.insert("cmi.score.scaled".into(), String::new());
        store.insert("cmi.learner_id".into(), "learner-1".into());
        store.insert("cmi.learner_name".into(), "Learner, Test".into());
        store.insert("cmi.total_time".into(), "PT0S".into());
        store.insert("cmi.interactions._count".into(), "0".into());
        Self {
            store,
            last_err: "0: No error".into(),
            ..Self::default()
        }
    }

    fn read_only(key: &str) -> bool {
        matches!(
            key,
            "cmi.entry" | "cmi.learner_id" | "cmi.learner_name" | "cmi.total_time"
        )
    }

    fn interaction_field(key: &str) -> Option<(&str, &str)> {
        let rest = key.strip_prefix("cmi.interactions.")?;
        let (index, field) = rest.split_once('.')?;
        if index.is_empty() || !index.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        Some((index, field))
    }

    fn known(key: &str) -> bool {
        key == "cmi.interactions._count"
            || Self::interaction_field(key).is_some()
            || matches!(
                key,
                "cmi.completion_status"
                    | "cmi.success_status"
                    | "cmi.location"
                    | "cmi.exit"
                    | "cmi.session_time"
                    | "cmi.suspend_data"
                    | "cmi.score.min"
                    | "cmi.score.max"
                    | "cmi.score.raw"
                    | "cmi.score.scaled"
                    | "cmi.learner_id"
                    | "cmi.learner_name"
                    | "cmi.entry"
                    | "cmi.total_time"
            )
    }

    fn valid_value(key: &str, value: &str) -> bool {
        if let Some((_, field)) = Self::interaction_field(key) {
            return match field {
                "result" => {
                    matches!(value, "correct" | "incorrect" | "unanticipated" | "neutral")
                        || plain_decimal(value).is_some()
                }
                _ => true,
            };
        }
        match key {
            "cmi.completion_status" => {
                matches!(
                    value,
                    "completed" | "incomplete" | "not attempted" | "unknown"
                )
            }
            "cmi.success_status" => matches!(value, "passed" | "failed" | "unknown"),
            "cmi.exit" => matches!(value, "" | "timeout" | "suspend" | "logout" | "normal"),
            "cmi.session_time" => valid_time_interval(value),
            "cmi.suspend_data" => value.chars().count() <= SUSPEND_LIMIT_2004,
            "cmi.location" => value.chars().count() <= LOCATION_LIMIT_2004,
            "cmi.score.scaled" => plain_decimal(value).is_some_and(|n| (-1.0..=1.0).contains(&n)),
            "cmi.score.min" | "cmi.score.max" | "cmi.score.raw" => {
                plain_decimal(value).is_some_and(|n| (0.0..=100.0).contains(&n))
            }
            _ => true,
        }
    }
}

impl CmiApi for Fake2004Api {
    fn initialize(&mut self) -> bool {
        self.calls.push(Call::Initialize);
        self.last_err = "0: No error".into();
        true
    }

    fn get_value(&mut self, key: &str) -> String {
        self.calls.push(Call::GetValue(key.to_string()));
        match self.store.get(key).cloned() {
            Some(value) => value,
            None => {
                self.last_err = "401: Not implemented".into();
                String::new()
            }
        }
    }

    fn set_value(&mut self, key: &str, value: &str) -> bool {
        self.calls
            .push(Call::SetValue(key.to_string(), value.to_string()));
        let interaction_error = Self::interaction_field(key).and_then(|(index, field)| {
            let id_key = format!("cmi.interactions.{index}.id");
            let type_key = format!("cmi.interactions.{index}.type");
            let needs_type = field.starts_with("correct_responses.") || field == "learner_response";
            if (field != "id" && !self.store.contains_key(&id_key))
                || (needs_type && !self.store.contains_key(&type_key))
            {
                Some("408")
            } else if needs_type
                && self.store.get(&type_key).map(String::as_str) == Some("true-false")
                && !matches!(value, "true" | "false")
            {
                Some("406")
            } else {
                None
            }
        });
        if Self::read_only(key) {
            self.last_err = "403: Element is read only".into();
            false
        } else if let Some(code) = interaction_error {
            self.last_err = format!("{code}: Data model dependency not established");
            false
        } else if !Self::known(key) || !Self::valid_value(key, value) {
            self.last_err = "406: Data model element value out of range".into();
            false
        } else {
            self.store.insert(key.to_string(), value.to_string());
            if let Some((index, "id")) = Self::interaction_field(key) {
                let count = self
                    .store
                    .get("cmi.interactions._count")
                    .and_then(|count| count.parse::<u64>().ok())
                    .unwrap_or(0);
                let index = index.parse::<u64>().unwrap_or(0);
                if index >= count {
                    self.store.insert(
                        "cmi.interactions._count".into(),
                        index.saturating_add(1).to_string(),
                    );
                }
            }
            self.last_err = "0: No error".into();
            true
        }
    }

    fn commit(&mut self) -> bool {
        self.calls.push(Call::Commit);
        true
    }

    fn finish(&mut self) -> bool {
        self.calls.push(Call::Finish);
        true
    }

    fn last_error(&mut self) -> String {
        self.last_err.clone()
    }
}

fn plain_decimal(value: &str) -> Option<f64> {
    if value.is_empty() || value.contains('e') || value.contains('E') {
        return None;
    }
    value.parse::<f64>().ok().filter(|n| n.is_finite())
}

fn valid_time_interval(value: &str) -> bool {
    let Some(mut rest) = value.strip_prefix("PT") else {
        return false;
    };
    if rest.is_empty() {
        return false;
    }
    let mut any = false;
    if let Some(index) = rest.find('H') {
        if !all_digits(&rest[..index]) {
            return false;
        }
        rest = &rest[index + 1..];
        any = true;
    }
    if let Some(index) = rest.find('M') {
        if !all_digits(&rest[..index]) {
            return false;
        }
        rest = &rest[index + 1..];
        any = true;
    }
    if !rest.is_empty() {
        let Some(seconds) = rest.strip_suffix('S') else {
            return false;
        };
        let mut parts = seconds.split('.');
        let Some(whole) = parts.next() else {
            return false;
        };
        if !all_digits(whole) {
            return false;
        }
        if let Some(fraction) = parts.next()
            && (fraction.len() > 2 || !all_digits(fraction))
        {
            return false;
        }
        if parts.next().is_some() {
            return false;
        }
        any = true;
    }
    any
}

fn all_digits(value: &str) -> bool {
    !value.is_empty() && value.chars().all(|c| c.is_ascii_digit())
}

fn failed_status_writes_2004(api: &Fake2004Api) -> usize {
    api.calls
        .iter()
        .filter(|c| **c == Call::SetValue("cmi.success_status".into(), "failed".into()))
        .count()
}

#[test]
fn scorm2004_first_launch_writes_incomplete_completion() {
    let course = hello_course();
    let mut api = Fake2004Api::new();
    let mut bridge = LmsBridge::for_standard(Standard::Scorm2004);
    assert_eq!(bridge.standard(), Standard::Scorm2004);

    let (plan, warnings) = bridge.start(&mut api, &course);
    assert_eq!(
        plan,
        StartPlan {
            resume: None,
            learner_name: Some("Learner, Test".into())
        }
    );
    assert!(warnings.is_empty());
    assert_eq!(
        api.store.get("cmi.completion_status").map(String::as_str),
        Some("incomplete")
    );
    assert_eq!(
        api.store.get("cmi.success_status").map(String::as_str),
        Some("unknown")
    );
    assert!(
        !api.store.keys().any(|key| key.starts_with("cmi.core.")),
        "2004 bridge must not touch 1.2 elements"
    );
}

#[test]
fn scorm2004_resume_returns_saved_state() {
    let course = hello_course();
    let mut player = Player::new(&course).expect("player new");
    let mut api = Fake2004Api::new();
    let mut bridge = LmsBridge::for_standard(Standard::Scorm2004);
    let (plan, _) = bridge.start(&mut api, &course);
    assert_eq!(
        plan,
        StartPlan {
            resume: None,
            learner_name: Some("Learner, Test".into())
        }
    );

    let inputs: Vec<Input> = serde_json::from_str(PASS_INPUTS).expect("deserialize pass inputs");
    for input in &inputs[..5] {
        let effects = player.handle(input);
        let warnings = bridge.after_input(&mut api, &course, player.state(), &effects, 0);
        assert!(warnings.is_empty());
    }
    let mid_slide_id = player.current_slide().expect("current slide").id.clone();

    let mut new_api = Fake2004Api {
        store: api.store.clone(),
        ..Fake2004Api::new()
    };
    let mut new_bridge = LmsBridge::for_standard(Standard::Scorm2004);
    let (plan, warnings) = new_bridge.start(&mut new_api, &course);
    assert!(warnings.is_empty());

    match plan.resume {
        Some(saved_state) => {
            let mut resumed = Player::resume(&course, saved_state).expect("resume player");
            let effects = resumed.handle(&Input::Start);
            assert_eq!(resumed.current_slide().expect("slide").id, mid_slide_id);
            assert_eq!(
                effects.first(),
                Some(&Effect::SlideEntered {
                    slide: mid_slide_id
                })
            );
        }
        None => panic!("expected a resume plan"),
    }
}

#[test]
fn scorm2004_pass_reports_completed_passed_and_scaled() {
    let course = hello_course();
    let mut player = Player::new(&course).expect("player new");
    let mut api = Fake2004Api::new();
    let mut bridge = LmsBridge::for_standard(Standard::Scorm2004);
    let (plan, warnings) = bridge.start(&mut api, &course);
    assert_eq!(
        plan,
        StartPlan {
            resume: None,
            learner_name: Some("Learner, Test".into())
        }
    );
    assert!(warnings.is_empty());

    let inputs: Vec<Input> = serde_json::from_str(PASS_INPUTS).expect("deserialize pass inputs");
    for input in &inputs {
        let effects = player.handle(input);
        let warnings = bridge.after_input(&mut api, &course, player.state(), &effects, 0);
        assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    }

    assert_eq!(
        api.store.get("cmi.completion_status").map(String::as_str),
        Some("completed")
    );
    assert_eq!(
        api.store.get("cmi.success_status").map(String::as_str),
        Some("passed")
    );
    assert_eq!(
        api.store.get("cmi.score.raw").map(String::as_str),
        Some("100")
    );
    assert_eq!(
        api.store.get("cmi.score.scaled").map(String::as_str),
        Some("1")
    );
    assert_eq!(
        api.store.get("cmi.location").map(String::as_str),
        Some("s-results")
    );
    let suspend = api
        .store
        .get("cmi.suspend_data")
        .expect("suspend_data present");
    assert!(suspend.starts_with("cc1:"));
    assert!(bridge.is_completed());
}

#[test]
fn scorm2004_fail_reports_completed_failed_zero() {
    let course = hello_course();
    let mut player = Player::new(&course).expect("player new");
    let mut api = Fake2004Api::new();
    let mut bridge = LmsBridge::for_standard(Standard::Scorm2004);
    let (plan, warnings) = bridge.start(&mut api, &course);
    assert_eq!(
        plan,
        StartPlan {
            resume: None,
            learner_name: Some("Learner, Test".into())
        }
    );
    assert!(warnings.is_empty());

    let fail_inputs = [
        Input::Start,
        Input::Tick { ms: 1000 },
        click("btn-start"),
        click("tab1"),
        click("tab2"),
        click("tab3"),
        click("btn-next"),
        Input::SelectChoice {
            choice: "a".to_string(),
        },
        click("btn-submit"),
        click("btn-try-again"),
        Input::SelectChoice {
            choice: "c".to_string(),
        },
        click("btn-submit"),
        Input::Next,
    ];

    for input in &fail_inputs {
        let effects = player.handle(input);
        let warnings = bridge.after_input(&mut api, &course, player.state(), &effects, 0);
        assert!(warnings.is_empty());
    }

    assert_eq!(
        api.store.get("cmi.completion_status").map(String::as_str),
        Some("completed")
    );
    assert_eq!(
        api.store.get("cmi.success_status").map(String::as_str),
        Some("failed")
    );
    assert_eq!(
        api.store.get("cmi.score.raw").map(String::as_str),
        Some("0")
    );
    assert_eq!(
        api.store.get("cmi.score.scaled").map(String::as_str),
        Some("0")
    );
    assert_eq!(failed_status_writes_2004(&api), 1);
}

#[test]
fn scorm2004_completion_without_quiz_leaves_success_untouched() {
    let course = hello_course();
    let player = Player::new(&course).expect("player new");
    let mut api = Fake2004Api::new();
    let mut bridge = LmsBridge::for_standard(Standard::Scorm2004);
    let (plan, warnings) = bridge.start(&mut api, &course);
    assert_eq!(
        plan,
        StartPlan {
            resume: None,
            learner_name: Some("Learner, Test".into())
        }
    );
    assert!(warnings.is_empty());

    let effects = [Effect::Completion {
        status: CompletionStatus::Completed,
    }];
    let warnings = bridge.after_input(&mut api, &course, player.state(), &effects, 0);
    assert!(warnings.is_empty());
    assert_eq!(
        api.store.get("cmi.completion_status").map(String::as_str),
        Some("completed")
    );
    assert_eq!(
        api.store.get("cmi.success_status").map(String::as_str),
        Some("unknown")
    );
}

#[test]
fn scorm2004_finish_uses_normal_after_completion_and_iso_time() {
    let course = hello_course();
    let player = Player::new(&course).expect("player new");
    let mut api = Fake2004Api::new();
    let mut bridge = LmsBridge::for_standard(Standard::Scorm2004);
    let (plan, warnings) = bridge.start(&mut api, &course);
    assert_eq!(
        plan,
        StartPlan {
            resume: None,
            learner_name: Some("Learner, Test".into())
        }
    );
    assert!(warnings.is_empty());

    let effects = [
        Effect::Score {
            raw: 10,
            max: 10,
            percent: 100,
            status: CompletionStatus::Passed,
        },
        Effect::Completion {
            status: CompletionStatus::Passed,
        },
    ];
    let warnings = bridge.after_input(&mut api, &course, player.state(), &effects, 0);
    assert!(warnings.is_empty());

    let warnings = bridge.finish_session(&mut api, 3_723_450);
    assert!(warnings.is_empty());
    assert_eq!(
        api.store.get("cmi.exit").map(String::as_str),
        Some("normal")
    );
    assert_eq!(
        api.store.get("cmi.session_time").map(String::as_str),
        Some("PT1H2M3.45S")
    );

    // An unfinished session suspends instead.
    let mut api2 = Fake2004Api::new();
    let mut bridge2 = LmsBridge::for_standard(Standard::Scorm2004);
    let _ = bridge2.start(&mut api2, &course);
    let warnings = bridge2.finish_session(&mut api2, 5000);
    assert!(warnings.is_empty());
    assert_eq!(
        api2.store.get("cmi.exit").map(String::as_str),
        Some("suspend")
    );
    assert_eq!(
        api2.store.get("cmi.session_time").map(String::as_str),
        Some("PT5S")
    );
}

#[test]
fn iso_session_time_formats_and_handles_max() {
    assert_eq!(format_session_time_iso(0), "PT0S");
    assert_eq!(format_session_time_iso(3_723_450), "PT1H2M3.45S");
    assert_eq!(format_session_time_iso(120_000), "PT2M");
    let max = format_session_time_iso(u64::MAX);
    assert!(max.starts_with("PT"));
    assert!(max.ends_with('S'));
    assert!(max.contains('H'));
}

#[test]
fn scaled_score_is_plain_decimal() {
    assert_eq!(format_scaled_score(85), "0.85");
    assert_eq!(format_scaled_score(100), "1");
    assert_eq!(format_scaled_score(0), "0");
    assert_eq!(format_scaled_score(50), "0.5");
    assert_eq!(format_scaled_score(5), "0.05");
    assert!(!format_scaled_score(85).contains('e'));
}

#[test]
fn scorm2004_accepts_large_suspend_payload_that_12_refuses() {
    let mut course = hello_course();
    course.variables.push(VariableDef {
        name: "long_text".into(),
        value: Value::Text(String::new()),
    });
    let mut player = Player::new(&course).expect("player new");
    let _ = player.handle(&Input::Start);

    let big_string = "x".repeat(5000);
    let mut state = player.state().clone();
    state
        .variables
        .insert("long_text".into(), Value::Text(big_string));
    let encoded = encode_suspend(&course, &state);
    assert!(encoded.len() > SUSPEND_LIMIT);
    assert!(encoded.len() <= SUSPEND_LIMIT_2004);

    let effects = [Effect::VariableChanged {
        variable: "long_text".into(),
        value: state.variables["long_text"].clone(),
    }];

    let mut api2004 = Fake2004Api::new();
    let mut bridge2004 = LmsBridge::for_standard(Standard::Scorm2004);
    let _ = bridge2004.start(&mut api2004, &course);
    let warnings = bridge2004.after_input(&mut api2004, &course, &state, &effects, 0);
    assert!(warnings.is_empty());
    assert_eq!(
        api2004.store.get("cmi.suspend_data").map(String::as_str),
        Some(encoded.as_str())
    );

    let mut api12 = FakeApi::new();
    let mut bridge12 = LmsBridge::new();
    let _ = bridge12.start(&mut api12, &course);
    let warnings = bridge12.after_input(&mut api12, &course, &state, &effects, 0);
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("exceeds limit 4096"));
    assert!(!api12.store.contains_key("cmi.suspend_data"));
}

#[test]
fn scorm2004_warns_for_location_over_limit_without_refused_write() {
    let course = hello_course();
    let player = Player::new(&course).expect("player new");
    let mut api = Fake2004Api::new();
    let mut bridge = LmsBridge::for_standard(Standard::Scorm2004);
    let _ = bridge.start(&mut api, &course);

    let effects = [Effect::SlideEntered {
        slide: "s".repeat(LOCATION_LIMIT_2004 + 1),
    }];
    let warnings = bridge.after_input(&mut api, &course, player.state(), &effects, 0);
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("exceeds limit 1000"));
    assert_eq!(api.store.get("cmi.location").map(String::as_str), Some(""));
    assert!(
        !api.calls
            .iter()
            .any(|c| matches!(c, Call::SetValue(k, _) if k == "cmi.location"))
    );

    let at_limit = "s".repeat(LOCATION_LIMIT_2004);
    let effects = [Effect::SlideEntered {
        slide: at_limit.clone(),
    }];
    let warnings = bridge.after_input(&mut api, &course, player.state(), &effects, 0);
    assert!(warnings.is_empty());
    assert_eq!(
        api.store.get("cmi.location").map(String::as_str),
        Some(at_limit.as_str())
    );
}

fn answered_effect(slide: &str) -> Effect {
    Effect::QuestionAnswered {
        slide: slide.into(),
        correct: true,
        points: 10,
        attempts_left: None,
    }
}

fn write_12(course: &Course, slide: &str, state: &PlayerState) -> (FakeApi, Vec<String>) {
    let mut api = FakeApi::new();
    let mut bridge = LmsBridge::new();
    let (plan, warnings) = bridge.start(&mut api, course);
    assert_eq!(
        plan,
        StartPlan {
            resume: None,
            learner_name: None
        }
    );
    assert!(warnings.is_empty());
    let warnings = bridge.after_input(&mut api, course, state, &[answered_effect(slide)], 0);
    (api, warnings)
}

fn write_2004(course: &Course, slide: &str, state: &PlayerState) -> (Fake2004Api, Vec<String>) {
    let mut api = Fake2004Api::new();
    let mut bridge = LmsBridge::for_standard(Standard::Scorm2004);
    let (plan, warnings) = bridge.start(&mut api, course);
    assert_eq!(
        plan,
        StartPlan {
            resume: None,
            learner_name: Some("Learner, Test".into())
        }
    );
    assert!(warnings.is_empty());
    let warnings = bridge.after_input(&mut api, course, state, &[answered_effect(slide)], 0);
    (api, warnings)
}

#[test]
fn interaction_record_covers_every_question_kind_on_the_showcase() {
    let course = showcase_course();

    let state = answered_state(
        "s-q-mc",
        LearnerResponse::Choices(vec!["mc-b".into()]),
        1,
        true,
    );
    let record = interaction_record(&course, "s-q-mc", &state).expect("mc record");
    assert_eq!(record.id, "s-q-mc_a1");
    assert_eq!(record.kind, InteractionKind::Choice);
    assert_eq!(record.learner, Response::Ids(vec!["mc-b".into()]));
    assert_eq!(record.correct, vec![Response::Ids(vec!["mc-b".into()])]);
    assert!(!record.case_sensitive);
    assert!(record.is_correct);
    assert_eq!(record.weighting, 10);

    let state = answered_state(
        "s-q-mr",
        LearnerResponse::Choices(vec!["mr-png".into(), "mr-mp4".into(), "mr-wav".into()]),
        1,
        true,
    );
    let record = interaction_record(&course, "s-q-mr", &state).expect("mr record");
    assert_eq!(record.kind, InteractionKind::Choice);
    assert_eq!(
        record.correct,
        vec![Response::Ids(vec![
            "mr-png".into(),
            "mr-mp4".into(),
            "mr-wav".into()
        ])]
    );

    let state = answered_state(
        "s-q-tf",
        LearnerResponse::Choices(vec!["true".into()]),
        1,
        true,
    );
    let record = interaction_record(&course, "s-q-tf", &state).expect("tf record");
    assert_eq!(record.kind, InteractionKind::TrueFalse);
    assert_eq!(record.correct, vec![Response::Bool(true)]);

    let state = answered_state("s-q-fi", LearnerResponse::Text("Rust".into()), 1, true);
    let record = interaction_record(&course, "s-q-fi", &state).expect("fill-in record");
    assert_eq!(record.kind, InteractionKind::FillIn);
    assert_eq!(
        record.correct,
        vec![
            Response::Text("Rust".into()),
            Response::Text("Rust only".into())
        ]
    );

    let state = answered_state(
        "s-q-seq",
        LearnerResponse::Sequence(vec![
            "seq-spec".into(),
            "seq-logic".into(),
            "seq-verify".into(),
        ]),
        1,
        true,
    );
    let record = interaction_record(&course, "s-q-seq", &state).expect("sequence record");
    assert_eq!(record.kind, InteractionKind::Sequencing);
    assert_eq!(
        record.correct,
        vec![Response::Ids(vec![
            "seq-spec".into(),
            "seq-logic".into(),
            "seq-verify".into()
        ])]
    );

    let pairs = vec![
        ("p-scorm".into(), "p-scorm".into()),
        ("p-logic".into(), "p-logic".into()),
        ("p-publish".into(), "p-publish".into()),
    ];
    let state = answered_state("s-q-mat", LearnerResponse::Pairs(pairs.clone()), 1, true);
    let record = interaction_record(&course, "s-q-mat", &state).expect("matching record");
    assert_eq!(record.kind, InteractionKind::Matching);
    assert_eq!(record.correct, vec![Response::Pairs(pairs)]);

    let state = answered_state(
        "s-q-hs",
        LearnerResponse::Regions(vec!["r-red".into()]),
        1,
        true,
    );
    let record = interaction_record(&course, "s-q-hs", &state).expect("hotspot record");
    assert_eq!(record.kind, InteractionKind::Choice);
    assert_eq!(record.learner, Response::Ids(vec!["r-red".into()]));
    assert_eq!(record.correct, vec![Response::Ids(vec!["r-red".into()])]);

    let drops = vec![
        ("item-scorm".into(), "box-l0".into()),
        ("item-logic".into(), "box-l2".into()),
        ("item-engine".into(), "box-l5".into()),
    ];
    let state = answered_state("s-q-dd", LearnerResponse::Pairs(drops.clone()), 1, true);
    let record = interaction_record(&course, "s-q-dd", &state).expect("drag record");
    assert_eq!(record.kind, InteractionKind::Matching);
    assert_eq!(record.correct, vec![Response::Pairs(drops)]);
}

#[test]
fn freeform_interaction_reports_other_with_a_correct_response() {
    let mut course = hello_course();
    course.slide_mut("s-quiz").expect("s-quiz").question = Some(
        serde_json::from_value(serde_json::json!({
            "kind": "freeform",
            "prompt": "Show your working.",
            "choices": [],
            "points": 10,
            "correct_when": { "compare": { "variable": "tabs_seen", "op": "ge", "value": 0 } }
        }))
        .expect("freeform"),
    );
    let state = answered_state("s-quiz", LearnerResponse::Text("correct".into()), 1, true);
    let record = interaction_record(&course, "s-quiz", &state).expect("freeform record");
    assert_eq!(record.kind, InteractionKind::Other);
    assert_eq!(record.kind.as_cmi_type(), "other");
    assert_eq!(record.learner, Response::Text("correct".into()));
    assert_eq!(record.correct, vec![Response::Text("correct".into())]);
    assert_eq!(record.weighting, 10);
    assert!(record.is_correct);
    assert!(!record.survey);
}

#[test]
fn interaction_record_marks_a_wrong_answer_and_rejects_missing_inputs() {
    let course = showcase_course();
    let state = answered_state(
        "s-q-mc",
        LearnerResponse::Choices(vec!["mc-a".into()]),
        2,
        false,
    );
    let record = interaction_record(&course, "s-q-mc", &state).expect("wrong record");
    assert!(!record.is_correct);
    assert_eq!(record.id, "s-q-mc_a2");

    let mut no_response = state.clone();
    no_response.last_response = None;
    assert!(interaction_record(&course, "s-q-mc", &no_response).is_none());
    assert!(interaction_record(&course, "s-missing", &state).is_none());
    let no_question = PlayerState {
        last_response: Some(LearnerResponse::Text("x".into())),
        ..PlayerState::default()
    };
    assert!(interaction_record(&course, "s-welcome", &no_question).is_none());
}

#[test]
fn scorm12_writes_exact_choice_interaction_elements() {
    let course = showcase_course();
    let state = answered_state(
        "s-q-mc",
        LearnerResponse::Choices(vec!["mc-b".into()]),
        1,
        true,
    );
    let (api, warnings) = write_12(&course, "s-q-mc", &state);
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");

    assert_eq!(
        api.store.get("cmi.interactions.0.id").map(String::as_str),
        Some("s-q-mc_a1")
    );
    assert_eq!(
        api.store.get("cmi.interactions.0.type").map(String::as_str),
        Some("choice")
    );
    assert_eq!(
        api.store
            .get("cmi.interactions.0.weighting")
            .map(String::as_str),
        Some("10")
    );
    assert_eq!(
        api.store
            .get("cmi.interactions.0.correct_responses.0.pattern")
            .map(String::as_str),
        Some("mc-b")
    );
    assert_eq!(
        api.store
            .get("cmi.interactions.0.student_response")
            .map(String::as_str),
        Some("mc-b")
    );
    assert_eq!(
        api.store
            .get("cmi.interactions.0.result")
            .map(String::as_str),
        Some("correct")
    );
    assert!(!api.store.contains_key("cmi.interactions.0.description"));
    assert!(
        !api.store
            .contains_key("cmi.interactions.0.learner_response")
    );
}

#[test]
fn scorm12_writes_exact_matching_and_fill_in_interactions() {
    let course = showcase_course();
    let pairs = vec![
        ("p-scorm".into(), "p-logic".into()),
        ("p-logic".into(), "p-scorm".into()),
        ("p-publish".into(), "p-publish".into()),
    ];
    let state = answered_state("s-q-mat", LearnerResponse::Pairs(pairs), 1, false);
    let (api, warnings) = write_12(&course, "s-q-mat", &state);
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    assert_eq!(
        api.store
            .get("cmi.interactions.0.correct_responses.0.pattern")
            .map(String::as_str),
        Some("p-scorm.p-scorm,p-logic.p-logic,p-publish.p-publish")
    );
    assert_eq!(
        api.store
            .get("cmi.interactions.0.student_response")
            .map(String::as_str),
        Some("p-scorm.p-logic,p-logic.p-scorm,p-publish.p-publish")
    );
    assert_eq!(
        api.store
            .get("cmi.interactions.0.result")
            .map(String::as_str),
        Some("wrong")
    );

    let state = answered_state("s-q-fi", LearnerResponse::Text("Rust".into()), 1, true);
    let (api, warnings) = write_12(&course, "s-q-fi", &state);
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    assert_eq!(
        api.store
            .get("cmi.interactions.0.correct_responses.0.pattern")
            .map(String::as_str),
        Some("Rust")
    );
    assert_eq!(
        api.store
            .get("cmi.interactions.0.correct_responses.1.pattern")
            .map(String::as_str),
        Some("Rust only")
    );
    assert_eq!(
        api.store
            .get("cmi.interactions.0.student_response")
            .map(String::as_str),
        Some("Rust")
    );
    assert_eq!(
        api.store
            .get("cmi.interactions.0.result")
            .map(String::as_str),
        Some("correct")
    );
}

#[test]
fn scorm2004_writes_exact_choice_interaction_elements() {
    let course = showcase_course();
    let state = answered_state(
        "s-q-mc",
        LearnerResponse::Choices(vec!["mc-a".into()]),
        1,
        false,
    );
    let (api, warnings) = write_2004(&course, "s-q-mc", &state);
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");

    assert_eq!(
        api.store.get("cmi.interactions.0.id").map(String::as_str),
        Some("s-q-mc_a1")
    );
    assert_eq!(
        api.store.get("cmi.interactions.0.type").map(String::as_str),
        Some("choice")
    );
    assert_eq!(
        api.store
            .get("cmi.interactions.0.description")
            .map(String::as_str),
        Some("What is the primary design rule for CourseCraft code?")
    );
    assert_eq!(
        api.store
            .get("cmi.interactions.0.weighting")
            .map(String::as_str),
        Some("10")
    );
    assert_eq!(
        api.store
            .get("cmi.interactions.0.correct_responses.0.pattern")
            .map(String::as_str),
        Some("mc-b")
    );
    assert_eq!(
        api.store
            .get("cmi.interactions.0.learner_response")
            .map(String::as_str),
        Some("mc-a")
    );
    assert_eq!(
        api.store
            .get("cmi.interactions.0.result")
            .map(String::as_str),
        Some("incorrect")
    );
    assert!(
        !api.store
            .contains_key("cmi.interactions.0.student_response")
    );
}

#[test]
fn scorm2004_writes_exact_matching_and_fill_in_interactions() {
    let course = showcase_course();
    let pairs = vec![
        ("p-scorm".into(), "p-logic".into()),
        ("p-logic".into(), "p-scorm".into()),
        ("p-publish".into(), "p-publish".into()),
    ];
    let state = answered_state("s-q-mat", LearnerResponse::Pairs(pairs), 1, false);
    let (api, warnings) = write_2004(&course, "s-q-mat", &state);
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    assert_eq!(
        api.store
            .get("cmi.interactions.0.correct_responses.0.pattern")
            .map(String::as_str),
        Some("p-scorm[.]p-scorm[,]p-logic[.]p-logic[,]p-publish[.]p-publish")
    );
    assert_eq!(
        api.store
            .get("cmi.interactions.0.learner_response")
            .map(String::as_str),
        Some("p-scorm[.]p-logic[,]p-logic[.]p-scorm[,]p-publish[.]p-publish")
    );
    assert_eq!(
        api.store
            .get("cmi.interactions.0.result")
            .map(String::as_str),
        Some("incorrect")
    );

    let mut course = showcase_course();
    course
        .slide_mut("s-q-fi")
        .and_then(|s| s.question.as_mut())
        .expect("fill-in question")
        .case_sensitive = true;
    let state = answered_state("s-q-fi", LearnerResponse::Text("Rust".into()), 1, true);
    let (api, warnings) = write_2004(&course, "s-q-fi", &state);
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    assert_eq!(
        api.store
            .get("cmi.interactions.0.correct_responses.0.pattern")
            .map(String::as_str),
        Some("{case_matters=true}Rust")
    );
    assert_eq!(
        api.store
            .get("cmi.interactions.0.correct_responses.1.pattern")
            .map(String::as_str),
        Some("{case_matters=true}Rust only")
    );
    assert_eq!(
        api.store
            .get("cmi.interactions.0.learner_response")
            .map(String::as_str),
        Some("Rust")
    );
    assert_eq!(
        api.store
            .get("cmi.interactions.0.result")
            .map(String::as_str),
        Some("correct")
    );
}

#[test]
fn a_second_attempt_gets_the_next_index_and_attempt_suffix() {
    let course = showcase_course();
    let mut api = FakeApi::new();
    let mut bridge = LmsBridge::new();
    let _ = bridge.start(&mut api, &course);

    let first = answered_state(
        "s-q-mc",
        LearnerResponse::Choices(vec!["mc-a".into()]),
        1,
        false,
    );
    let warnings = bridge.after_input(&mut api, &course, &first, &[answered_effect("s-q-mc")], 0);
    assert!(warnings.is_empty());
    let second = answered_state(
        "s-q-mc",
        LearnerResponse::Choices(vec!["mc-b".into()]),
        2,
        true,
    );
    let warnings = bridge.after_input(&mut api, &course, &second, &[answered_effect("s-q-mc")], 0);
    assert!(warnings.is_empty());

    assert_eq!(
        api.store.get("cmi.interactions.0.id").map(String::as_str),
        Some("s-q-mc_a1")
    );
    assert_eq!(
        api.store.get("cmi.interactions.1.id").map(String::as_str),
        Some("s-q-mc_a2")
    );
    assert_eq!(
        api.store
            .get("cmi.interactions.0.result")
            .map(String::as_str),
        Some("wrong")
    );
    assert_eq!(
        api.store
            .get("cmi.interactions.1.result")
            .map(String::as_str),
        Some("correct")
    );
}

#[test]
fn resume_appends_interactions_after_the_saved_count() {
    let course = showcase_course();
    let state = answered_state(
        "s-q-mc",
        LearnerResponse::Choices(vec!["mc-b".into()]),
        1,
        true,
    );
    let (api, warnings) = write_2004(&course, "s-q-mc", &state);
    assert!(warnings.is_empty());
    assert_eq!(
        api.store.get("cmi.interactions._count").map(String::as_str),
        Some("1")
    );

    let mut resumed = Fake2004Api {
        store: api.store.clone(),
        ..Fake2004Api::new()
    };
    let mut bridge = LmsBridge::for_standard(Standard::Scorm2004);
    let _ = bridge.start(&mut resumed, &course);
    let warnings = bridge.after_input(
        &mut resumed,
        &course,
        &state,
        &[answered_effect("s-q-mc")],
        0,
    );
    assert!(warnings.is_empty());
    assert_eq!(
        resumed
            .store
            .get("cmi.interactions.1.id")
            .map(String::as_str),
        Some("s-q-mc_a1")
    );
    assert_eq!(
        resumed
            .store
            .get("cmi.interactions._count")
            .map(String::as_str),
        Some("2")
    );
}

#[test]
fn an_unparseable_count_disables_interactions_with_one_warning() {
    let course = showcase_course();
    let state = answered_state(
        "s-q-mc",
        LearnerResponse::Choices(vec!["mc-b".into()]),
        1,
        true,
    );

    let mut api = FakeApi::new();
    api.store
        .insert("cmi.interactions._count".into(), String::new());
    let mut bridge = LmsBridge::new();
    let (plan, warnings) = bridge.start(&mut api, &course);
    assert_eq!(
        plan,
        StartPlan {
            resume: None,
            learner_name: None
        }
    );
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("interaction reporting disabled"));

    let effects = [
        answered_effect("s-q-mc"),
        Effect::Score {
            raw: 10,
            max: 10,
            percent: 100,
            status: CompletionStatus::Passed,
        },
        Effect::Completion {
            status: CompletionStatus::Passed,
        },
    ];
    let warnings = bridge.after_input(&mut api, &course, &state, &effects, 0);
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    assert!(
        !api.store.keys().any(|key| {
            key.starts_with("cmi.interactions.") && key != "cmi.interactions._count"
        })
    );
    assert_eq!(
        api.store.get("cmi.core.score.raw").map(String::as_str),
        Some("100")
    );
    assert_eq!(
        api.store.get("cmi.core.lesson_status").map(String::as_str),
        Some("passed")
    );
}

#[test]
fn hostile_interaction_values_are_sanitized_and_cut() {
    let mut course = showcase_course();
    if let Some(question) = course
        .slide_mut("s-q-mc")
        .and_then(|slide| slide.question.as_mut())
    {
        for choice in &mut question.choices {
            if choice.correct {
                choice.id = "bad id\"two".into();
            }
        }
    }

    let state = answered_state(
        "s-q-mc",
        LearnerResponse::Choices(vec!["bad id\"two".into()]),
        1,
        true,
    );
    let (api, warnings) = write_12(&course, "s-q-mc", &state);
    assert!(warnings.is_empty());
    assert_eq!(
        api.store
            .get("cmi.interactions.0.student_response")
            .map(String::as_str),
        Some(sanitize_identifier("bad id\"two").as_str())
    );
    assert_eq!(
        api.store
            .get("cmi.interactions.0.correct_responses.0.pattern")
            .map(String::as_str),
        Some("bad_id_two")
    );
    assert_eq!(
        sanitize_identifier("bad id\"two"),
        "bad_id_two",
        "only safe identifier characters remain"
    );

    let long_text = "é".repeat(10_000);
    let state = answered_state("s-q-fi", LearnerResponse::Text(long_text.clone()), 1, false);
    let (api, warnings) = write_12(&course, "s-q-fi", &state);
    assert!(warnings.is_empty());
    let value = api
        .store
        .get("cmi.interactions.0.student_response")
        .expect("student response");
    assert_eq!(value.chars().count(), 255);
    assert!(value.starts_with('é'));

    let (api, warnings) = write_2004(&course, "s-q-fi", &state);
    assert!(warnings.is_empty());
    let value = api
        .store
        .get("cmi.interactions.0.learner_response")
        .expect("learner response");
    assert_eq!(value.chars().count(), 250);
    assert!(value.chars().all(|c| c == 'é'));

    let escaped = answered_state(
        "s-q-fi",
        LearnerResponse::Text("left[,]right[.]end\u{7}".into()),
        1,
        false,
    );
    let (api, warnings) = write_2004(&course, "s-q-fi", &escaped);
    assert!(warnings.is_empty());
    assert_eq!(
        api.store
            .get("cmi.interactions.0.learner_response")
            .map(String::as_str),
        Some("left,right.end")
    );
}

#[test]
fn true_false_answers_use_each_standards_boolean_vocabulary() {
    let course = showcase_course();
    let state = answered_state(
        "s-q-tf",
        LearnerResponse::Choices(vec!["false".into()]),
        1,
        false,
    );

    let (api, warnings) = write_12(&course, "s-q-tf", &state);
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    assert_eq!(
        api.store
            .get("cmi.interactions.0.correct_responses.0.pattern")
            .map(String::as_str),
        Some("t")
    );
    assert_eq!(
        api.store
            .get("cmi.interactions.0.student_response")
            .map(String::as_str),
        Some("f")
    );

    let (api, warnings) = write_2004(&course, "s-q-tf", &state);
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    assert_eq!(
        api.store
            .get("cmi.interactions.0.correct_responses.0.pattern")
            .map(String::as_str),
        Some("true")
    );
    assert_eq!(
        api.store
            .get("cmi.interactions.0.learner_response")
            .map(String::as_str),
        Some("false")
    );
}

#[test]
fn true_false_values_follow_the_choice_ids_not_just_their_order() {
    let mut course = showcase_course();
    let question = course
        .scenes
        .iter_mut()
        .flat_map(|scene| scene.slides.iter_mut())
        .find(|slide| slide.id == "s-q-tf")
        .and_then(|slide| slide.question.as_mut())
        .expect("true/false question");

    // `false` listed first and correct: the first position must not read as true.
    question.choices.reverse();
    for choice in &mut question.choices {
        choice.correct = choice.id == "false";
    }
    let state = answered_state(
        "s-q-tf",
        LearnerResponse::Choices(vec!["false".into()]),
        1,
        true,
    );
    let record = interaction_record(&course, "s-q-tf", &state).expect("reversed record");
    assert_eq!(record.correct, vec![Response::Bool(false)]);
    assert_eq!(record.learner, Response::Bool(false));

    // Custom ids fall back to position: the first choice is true.
    let question = course
        .scenes
        .iter_mut()
        .flat_map(|scene| scene.slides.iter_mut())
        .find(|slide| slide.id == "s-q-tf")
        .and_then(|slide| slide.question.as_mut())
        .expect("true/false question");
    let mut ids = ["no", "yes"].into_iter();
    for choice in &mut question.choices {
        choice.id = ids.next().unwrap_or_default().to_owned();
        choice.correct = choice.id == "no";
    }
    let state = answered_state(
        "s-q-tf",
        LearnerResponse::Choices(vec!["yes".into()]),
        1,
        false,
    );
    let record = interaction_record(&course, "s-q-tf", &state).expect("custom id record");
    assert_eq!(record.correct, vec![Response::Bool(true)]);
    assert_eq!(record.learner, Response::Bool(false));
}

#[test]
fn likert_survey_writes_likert_neutral_without_correct_responses() {
    let mut course = showcase_course();
    let question = course
        .slide_mut("s-q-mc")
        .and_then(|slide| slide.question.as_mut())
        .expect("likert question");
    question.kind = QuestionKind::Likert;
    for choice in &mut question.choices {
        choice.correct = false;
    }
    let state = answered_state(
        "s-q-mc",
        LearnerResponse::Choices(vec!["mc-b".into()]),
        1,
        true,
    );
    let record = interaction_record(&course, "s-q-mc", &state).expect("likert record");
    assert_eq!(record.kind, InteractionKind::Likert);
    assert!(record.survey);
    assert_eq!(record.weighting, 0);
    assert!(record.correct.is_empty());

    let (api, warnings) = write_12(&course, "s-q-mc", &state);
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    assert_eq!(
        api.store.get("cmi.interactions.0.type").map(String::as_str),
        Some("likert")
    );
    assert_eq!(
        api.store
            .get("cmi.interactions.0.weighting")
            .map(String::as_str),
        Some("0")
    );
    assert_eq!(
        api.store
            .get("cmi.interactions.0.result")
            .map(String::as_str),
        Some("neutral")
    );
    assert_eq!(
        api.store
            .get("cmi.interactions.0.student_response")
            .map(String::as_str),
        Some("mc-b")
    );
    assert!(
        !api.store
            .contains_key("cmi.interactions.0.correct_responses.0.pattern")
    );

    let (api, warnings) = write_2004(&course, "s-q-mc", &state);
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    assert_eq!(
        api.store.get("cmi.interactions.0.type").map(String::as_str),
        Some("likert")
    );
    assert_eq!(
        api.store
            .get("cmi.interactions.0.weighting")
            .map(String::as_str),
        Some("0")
    );
    assert_eq!(
        api.store
            .get("cmi.interactions.0.result")
            .map(String::as_str),
        Some("neutral")
    );
    assert_eq!(
        api.store
            .get("cmi.interactions.0.learner_response")
            .map(String::as_str),
        Some("mc-b")
    );
    assert!(
        !api.store
            .contains_key("cmi.interactions.0.correct_responses.0.pattern")
    );
}

#[test]
fn essay_survey_writes_fill_in_neutral_and_cuts_text_to_250_characters() {
    let mut course = showcase_course();
    let question = course
        .slide_mut("s-q-fi")
        .and_then(|slide| slide.question.as_mut())
        .expect("essay question");
    question.kind = QuestionKind::Essay;
    let long_text = "é".repeat(1000);
    let state = answered_state("s-q-fi", LearnerResponse::Text(long_text), 1, true);
    let record = interaction_record(&course, "s-q-fi", &state).expect("essay record");
    assert!(record.survey);
    assert_eq!(record.kind, InteractionKind::FillIn);

    let (api, warnings) = write_12(&course, "s-q-fi", &state);
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    assert_eq!(
        api.store.get("cmi.interactions.0.type").map(String::as_str),
        Some("fill-in")
    );
    assert_eq!(
        api.store
            .get("cmi.interactions.0.result")
            .map(String::as_str),
        Some("neutral")
    );
    assert!(
        !api.store
            .contains_key("cmi.interactions.0.correct_responses.0.pattern")
    );

    let (api, warnings) = write_2004(&course, "s-q-fi", &state);
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    assert_eq!(
        api.store.get("cmi.interactions.0.type").map(String::as_str),
        Some("fill-in")
    );
    assert_eq!(
        api.store
            .get("cmi.interactions.0.result")
            .map(String::as_str),
        Some("neutral")
    );
    let value = api
        .store
        .get("cmi.interactions.0.learner_response")
        .expect("learner response");
    assert_eq!(value.chars().count(), 250);
    assert!(value.chars().all(|c| c == 'é'));
}

#[test]
fn graded_question_still_writes_correct_result() {
    let course = showcase_course();
    let state = answered_state(
        "s-q-mc",
        LearnerResponse::Choices(vec!["mc-b".into()]),
        1,
        true,
    );
    let record = interaction_record(&course, "s-q-mc", &state).expect("graded record");
    assert!(!record.survey);

    let (api, warnings) = write_12(&course, "s-q-mc", &state);
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    assert_eq!(
        api.store
            .get("cmi.interactions.0.result")
            .map(String::as_str),
        Some("correct")
    );

    let (api, warnings) = write_2004(&course, "s-q-mc", &state);
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    assert_eq!(
        api.store
            .get("cmi.interactions.0.result")
            .map(String::as_str),
        Some("correct")
    );
}
