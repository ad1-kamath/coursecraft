//! Bug-hunt proof tests for `coursecraft-lms` (0.6.1 Neon review).
//!
//! Each test failed before its fix and now guards against a regression.

use coursecraft_course::{CompletionStatus, Course};
use coursecraft_lms::{
    LrsApi, LrsMethod, LrsRequest, LrsResponse, LtiReporter, ScoreReport, XapiBridge,
};
use coursecraft_logic::{Effect, Player};
use coursecraft_xapi::{Flavor, LaunchParams};
use serde_json::json;

/// `LtiReporter::mark_failed` pushes the stale in-flight report to the front of
/// `pending` ahead of a newer higher-rank pending report. On retry it sends the
/// stale report first, and if that retry fails it clears `pending`, losing the
/// newer `passed` report.
#[test]
fn lti_mark_failed_requeues_stale_report_ahead_of_newer_passed() {
    let mut reporter = LtiReporter::new();

    // First report: Failed 40%, starts in-flight.
    reporter.record(&[Effect::Score {
        raw: 4,
        max: 10,
        percent: 40,
        status: CompletionStatus::Failed,
    }]);
    let first = reporter.take_pending().expect("first report queued");
    assert_eq!(
        first,
        ScoreReport {
            percent: Some(40),
            status: "failed",
        }
    );

    // While Failed 40% is in flight, learner retries and passes with 90%.
    reporter.record(&[Effect::Score {
        raw: 9,
        max: 10,
        percent: 90,
        status: CompletionStatus::Passed,
    }]);

    // The stale Failed 40% request fails once.
    reporter.mark_failed();

    // Only the latest unsent report (Passed 90%) should be sent next, never
    // the superseded Failed 40% report.
    let next = reporter.take_pending().expect("pending report");
    assert_eq!(
        next,
        ScoreReport {
            percent: Some(90),
            status: "passed",
        },
        "stale Failed 40% was pushed ahead of newer Passed 90%"
    );
}

struct FakeLrs {
    sent: Vec<LrsRequest>,
    responses: Vec<LrsResponse>,
    uuid: u64,
}

impl FakeLrs {
    fn new() -> Self {
        Self {
            sent: Vec::new(),
            responses: Vec::new(),
            uuid: 0,
        }
    }
}

impl LrsApi for FakeLrs {
    fn send(&mut self, request: LrsRequest) {
        self.sent.push(request);
    }

    fn poll(&mut self) -> Vec<LrsResponse> {
        std::mem::take(&mut self.responses)
    }

    fn now_iso(&mut self) -> String {
        "2026-11-27T00:00:00Z".to_owned()
    }

    fn new_uuid(&mut self) -> String {
        self.uuid += 1;
        format!("00000000-0000-4000-8000-{:012}", self.uuid)
    }
}

fn minimal_course() -> Course {
    Course::from_json(
        r#"{
          "schema_version": 1,
          "id": "xapi-auth-test",
          "title": "xAPI Auth Test",
          "scenes": [{
            "id": "sc1",
            "title": "Scene 1",
            "slides": [{
              "id": "s1",
              "title": "Slide 1",
              "duration_ms": 5000
            }]
          }]
        }"#,
    )
    .expect("valid course")
}

/// Spec (`docs/spec/triggers.md` LRS error rules): an HTTP 401 or 403 mid-session
/// must warn once, turn reporting off, and set `reporting_lost()`.
/// `XapiBridge::handle_progress_put` ignores 401/403 completely.
#[test]
fn xapi_progress_put_401_does_not_disable_reporting() {
    let course = minimal_course();
    let params = LaunchParams {
        endpoint: "https://lrs.example.invalid/xapi".to_owned(),
        actor: json!({"mbox": "mailto:learner@example.invalid"}),
        activity_id: Some("https://course.example.invalid/c1".to_owned()),
        registration: Some("11111111-1111-4111-8111-111111111111".to_owned()),
        auth: Some("Basic dGVzdDp0ZXN0".to_owned()),
        fetch: None,
    };
    let mut bridge = XapiBridge::new(params, Flavor::Xapi, &course);
    let mut api = FakeLrs::new();

    // Start handshake: GET progress state -> 404 (fresh start).
    bridge.start(&mut api);
    let _ = bridge.tick(&mut api, &course, 0);
    let get_req = api
        .sent
        .iter()
        .find(|r| r.method == LrsMethod::Get)
        .expect("initial progress GET")
        .id;
    api.responses.push(LrsResponse {
        id: get_req,
        status: 404,
        body: String::new(),
    });
    let tick = bridge.tick(&mut api, &course, 100);
    assert!(tick.plan.is_some(), "handshake should finish");

    // Acknowledge initial `initialized` statement POST if sent.
    for req in api.sent.drain(..) {
        if req.method == LrsMethod::Post {
            api.responses.push(LrsResponse {
                id: req.id,
                status: 200,
                body: "[]".to_owned(),
            });
        }
    }
    let _ = bridge.tick(&mut api, &course, 200);

    // Trigger a progress PUT by recording a slide change state.
    let mut player = Player::new(&course).expect("player");
    let fx = player.handle(&coursecraft_logic::Input::Start);
    let _ = bridge.after_input(&mut api, &course, player.state(), &fx, 6000, 6000);
    let _ = bridge.tick(&mut api, &course, 6000);

    let put_req = api
        .sent
        .iter()
        .find(|r| r.method == LrsMethod::Put)
        .expect("progress PUT should be sent")
        .id;

    // LRS rejects the progress PUT with HTTP 401 Unauthorized.
    api.responses.push(LrsResponse {
        id: put_req,
        status: 401,
        body: "Unauthorized".to_owned(),
    });
    let after_401 = bridge.tick(&mut api, &course, 6100);

    assert!(
        bridge.reporting_lost(),
        "401 on progress PUT must disable reporting and set reporting_lost()"
    );
    assert!(
        !after_401.warnings.is_empty(),
        "401 on progress PUT must emit a warning"
    );
}
