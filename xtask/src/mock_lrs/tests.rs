//! Tests for the mock LMS/LRS request handler.

use std::path::PathBuf;

use serde_json::{Map, Value, json};

use super::*;

fn empty_store() -> LrsStore {
    LrsStore::new(8091, PathBuf::from("target/mock-lrs"))
}

fn request(method: &str, path: &str, query: &str) -> Request {
    Request {
        method: method.to_string(),
        path: path.to_string(),
        query: query.to_string(),
        headers: Vec::new(),
        body: Vec::new(),
    }
}

fn launch_request(
    flavor: &str,
    course: &str,
    mode: &str,
    mastery: Option<&str>,
    want_return: bool,
) -> Request {
    let mut query = format!("flavor={flavor}&course={course}&mode={mode}");
    if let Some(mastery) = mastery {
        query.push_str("&mastery=");
        query.push_str(mastery);
    }
    if want_return {
        query.push_str("&return=1");
    }
    request("POST", "/mock/launch", &query)
}

fn xapi_request(method: &str, path: &str, query: &str, body: Vec<u8>) -> Request {
    Request {
        method: method.to_string(),
        path: path.to_string(),
        query: query.to_string(),
        headers: vec![
            ("X-Experience-API-Version".to_string(), "1.0.3".to_string()),
            ("Authorization".to_string(), "Basic mock-xapi".to_string()),
            ("Content-Type".to_string(), "application/json".to_string()),
        ],
        body,
    }
}

fn first_registration(store: &LrsStore) -> String {
    store
        .registrations
        .keys()
        .next()
        .cloned()
        .expect("a registration")
}

fn post_statements(store: &mut LrsStore, statements: &[Value]) -> Response {
    let body = serde_json::to_vec(&Value::Array(statements.to_vec())).unwrap_or_default();
    handle(store, &xapi_request("POST", "/xapi/statements", "", body))
}

fn cmi5_statement(verb: &str, registration: &str, session: &str, id: &str) -> Value {
    let mut result = Map::new();
    if matches!(verb, "completed" | "passed" | "failed" | "terminated") {
        result.insert("duration".into(), json!("PT1M"));
    }
    if verb == "completed" {
        result.insert("completion".into(), json!(true));
    }
    if matches!(verb, "passed" | "failed") {
        result.insert("score".into(), json!({ "scaled": 0.9 }));
    }
    let mut categories = Vec::new();
    if matches!(
        verb,
        "initialized" | "completed" | "passed" | "failed" | "terminated"
    ) {
        categories.push(json!({ "id": CMI5_CATEGORY }));
    }
    if matches!(verb, "completed" | "passed" | "failed") {
        categories.push(json!({ "id": MOVEON_CATEGORY }));
    }
    let mut extensions = Map::new();
    extensions.insert(SESSION_ID_EXTENSION.into(), json!(session));
    json!({
        "id": id,
        "actor": actor_json(),
        "verb": { "id": format!("{VERB_BASE}{verb}") },
        "object": { "id": "urn:coursecraft:mock:activity:hello" },
        "result": Value::Object(result),
        "context": {
            "registration": registration,
            "extensions": Value::Object(extensions),
            "contextActivities": { "category": categories },
        },
    })
}

fn add_extension(mut statement: Value, key: &str, value: Value) -> Value {
    if let Some(extensions) = statement
        .pointer_mut("/context/extensions")
        .and_then(Value::as_object_mut)
    {
        extensions.insert(key.to_string(), value);
    }
    statement
}

fn category_ids_of(statement: &mut Value) -> &mut Vec<Value> {
    statement
        .pointer_mut("/context/contextActivities/category")
        .and_then(Value::as_array_mut)
        .expect("category array")
}

fn cmi5_registration(store: &mut LrsStore, mastery: Option<&str>) -> String {
    handle(
        store,
        &launch_request("cmi5", "hello", "Normal", mastery, false),
    );
    first_registration(store)
}

#[test]
fn launch_returns_cmi5_url_and_writes_launch_data() {
    let mut store = empty_store();
    let response = handle(
        &mut store,
        &launch_request("cmi5", "hello", "Normal", Some("0.8"), true),
    );
    assert_eq!(response.status, 200);
    let body: Value = serde_json::from_slice(&response.body).expect("launch json");
    let url = body.get("url").and_then(Value::as_str).expect("url");
    assert!(url.contains("/cmi5/hello/index.html?"));
    assert!(url.contains("endpoint="));
    assert!(url.contains("fetch="));
    assert!(url.contains("activityId="));

    let document = store
        .state
        .iter()
        .find(|(key, _)| key.state_id == LAUNCH_DATA)
        .map(|(_, body)| body)
        .expect("LMS.LaunchData");
    let document: Value = serde_json::from_slice(document).expect("launch data json");
    assert_eq!(
        document.get("launchMode").and_then(Value::as_str),
        Some("Normal")
    );
    assert_eq!(
        document.get("masteryScore").and_then(Value::as_f64),
        Some(0.8)
    );
    assert!(
        document
            .get("returnURL")
            .and_then(Value::as_str)
            .is_some_and(|url| url.ends_with("/mock/returned"))
    );
    let template = document.get("contextTemplate").expect("template");
    assert!(
        template
            .pointer(&format!(
                "/extensions/{}",
                escape_pointer(SESSION_ID_EXTENSION)
            ))
            .is_some()
    );
    assert!(
        template
            .pointer("/contextActivities/grouping/0/id")
            .is_some()
    );
}

#[test]
fn launch_returns_xapi_url() {
    let mut store = empty_store();
    let response = handle(
        &mut store,
        &launch_request("xapi", "showcase", "Normal", None, false),
    );
    assert_eq!(response.status, 200);
    let body: Value = serde_json::from_slice(&response.body).expect("launch json");
    let url = body.get("url").and_then(Value::as_str).expect("url");
    assert!(url.contains("/xapi/showcase/index.html?"));
    assert!(url.contains("auth=Basic%20mock-xapi"));
}

#[test]
fn fetch_token_once_then_reuse_error() {
    let mut store = empty_store();
    handle(
        &mut store,
        &launch_request("cmi5", "hello", "Normal", None, false),
    );
    let id = store.fetches.keys().next().cloned().expect("fetch id");
    let path = format!("/fetch/{id}");

    let first = handle(&mut store, &request("POST", &path, ""));
    assert_eq!(first.status, 200);
    let body: Value = serde_json::from_slice(&first.body).expect("fetch json");
    assert!(
        body.get("auth-token")
            .and_then(Value::as_str)
            .is_some_and(|token| token == format!("mock-token-{id}"))
    );

    let second = handle(&mut store, &request("POST", &path, ""));
    assert_eq!(second.status, 200);
    let body: Value = serde_json::from_slice(&second.body).expect("fetch json");
    assert_eq!(body.get("error-code").and_then(Value::as_str), Some("1"));
    assert!(
        store
            .problems
            .iter()
            .any(|problem| problem.contains("reused"))
    );

    let get = handle(&mut store, &request("GET", &path, ""));
    assert_eq!(get.status, 405);

    let statement = Request {
        method: "POST".to_string(),
        path: "/xapi/statements".to_string(),
        query: String::new(),
        headers: vec![
            ("X-Experience-API-Version".to_string(), "1.0.3".to_string()),
            (
                "Authorization".to_string(),
                format!("Basic mock-token-{id}"),
            ),
        ],
        body: br#"{"id":"after-fetch"}"#.to_vec(),
    };
    assert_eq!(handle(&mut store, &statement).status, 200);
}

#[test]
fn xapi_auth_is_required() {
    let mut store = empty_store();
    let no_auth = Request {
        method: "POST".to_string(),
        path: "/xapi/statements".to_string(),
        query: String::new(),
        headers: vec![("X-Experience-API-Version".to_string(), "1.0.3".to_string())],
        body: br#"{"id":"a"}"#.to_vec(),
    };
    assert_eq!(handle(&mut store, &no_auth).status, 401);

    let with_auth = xapi_request("POST", "/xapi/statements", "", br#"{"id":"a"}"#.to_vec());
    assert_eq!(handle(&mut store, &with_auth).status, 200);
}

#[test]
fn xapi_version_is_required() {
    let mut store = empty_store();
    let no_version = Request {
        method: "POST".to_string(),
        path: "/xapi/statements".to_string(),
        query: String::new(),
        headers: vec![("Authorization".to_string(), "Basic mock-xapi".to_string())],
        body: br#"{"id":"a"}"#.to_vec(),
    };
    assert_eq!(handle(&mut store, &no_version).status, 400);
}

#[test]
fn state_round_trip_with_encoded_agent() {
    let mut store = empty_store();
    let agent = actor_json().to_string();
    let query = format!(
        "stateId={}&activityId={}&agent={}&registration={}",
        encode("coursecraft.progress"),
        encode("urn:coursecraft:mock:activity:hello"),
        encode(&agent),
        encode("urn:coursecraft:mock:registration:cmi5-hello"),
    );
    let body = br#"{"suspend":"cc1:abc","completed":false,"passed":false}"#.to_vec();
    let put = handle(
        &mut store,
        &xapi_request("PUT", "/xapi/activities/state", &query, body.clone()),
    );
    assert_eq!(put.status, 204);
    let get = handle(
        &mut store,
        &xapi_request("GET", "/xapi/activities/state", &query, Vec::new()),
    );
    assert_eq!(get.status, 200);
    assert_eq!(get.body, body);

    let missing = handle(
        &mut store,
        &xapi_request(
            "GET",
            "/xapi/activities/state",
            &format!(
                "stateId={}&activityId={}&agent={}",
                encode("nope"),
                encode("urn:x"),
                encode(&agent)
            ),
            Vec::new(),
        ),
    );
    assert_eq!(missing.status, 404);
}

#[test]
fn duplicate_statement_ids_are_ignored() {
    let mut store = empty_store();
    let statement = json!({
        "id": "dup",
        "actor": actor_json(),
        "verb": { "id": format!("{VERB_BASE}initialized") },
        "object": { "id": "urn:act" },
    });
    let first = post_statements(&mut store, std::slice::from_ref(&statement));
    assert_eq!(first.status, 200);
    let second = post_statements(&mut store, std::slice::from_ref(&statement));
    assert_eq!(second.status, 200);
    assert_eq!(store.statements.len(), 1);
    let ids: Value = serde_json::from_slice(&second.body).expect("ids json");
    assert_eq!(ids.as_array().map(Vec::len), Some(1));
}

#[test]
fn lms_launch_data_is_protected() {
    let mut store = empty_store();
    let query = format!(
        "stateId={}&activityId={}&agent={}",
        encode(LAUNCH_DATA),
        encode("urn:act"),
        encode(&actor_json().to_string())
    );
    let response = handle(
        &mut store,
        &xapi_request("PUT", "/xapi/activities/state", &query, b"{}".to_vec()),
    );
    assert_eq!(response.status, 403);
    assert!(
        store
            .problems
            .iter()
            .any(|problem| problem.contains("LMS.LaunchData"))
    );
}

#[test]
fn path_traversal_is_refused() {
    let mut store = empty_store();
    assert_eq!(
        handle(&mut store, &request("GET", "/..%2fCargo.toml", "")).status,
        404
    );
    assert_eq!(
        handle(&mut store, &request("GET", "/a/../../Cargo.toml", "")).status,
        404
    );
    assert_eq!(
        handle(&mut store, &request("GET", "/..%5cCargo.toml", "")).status,
        404
    );
}

#[test]
fn fail_injection_fails_the_next_statement_posts() {
    let mut store = empty_store();
    let response = handle(
        &mut store,
        &request("POST", "/mock/fail", "count=1&status=503"),
    );
    assert_eq!(response.status, 200);
    let statement = json!({
        "id": "fail-1",
        "actor": actor_json(),
        "verb": { "id": format!("{VERB_BASE}initialized") },
        "object": { "id": "urn:act" },
    });
    let failed = post_statements(&mut store, std::slice::from_ref(&statement));
    assert_eq!(failed.status, 503);
    let accepted = post_statements(&mut store, std::slice::from_ref(&statement));
    assert_eq!(accepted.status, 200);
}

#[test]
fn good_cmi5_sequence_has_no_problems() {
    let mut store = empty_store();
    let registration = cmi5_registration(&mut store, None);
    let statements = vec![
        cmi5_statement("initialized", &registration, "session-1", "s1"),
        cmi5_statement("completed", &registration, "session-1", "s2"),
        cmi5_statement("passed", &registration, "session-1", "s3"),
        cmi5_statement("terminated", &registration, "session-1", "s4"),
    ];
    let response = post_statements(&mut store, &statements);
    assert_eq!(response.status, 200);
    assert!(store.problems.is_empty(), "{:?}", store.problems);
}

#[test]
fn repeated_answered_statements_are_allowed() {
    let mut store = empty_store();
    let registration = cmi5_registration(&mut store, None);
    let statements = vec![
        cmi5_statement("initialized", &registration, "session-1", "s1"),
        cmi5_statement("answered", &registration, "session-1", "s2"),
        cmi5_statement("answered", &registration, "session-1", "s3"),
        cmi5_statement("terminated", &registration, "session-1", "s4"),
    ];
    let response = post_statements(&mut store, &statements);
    assert_eq!(response.status, 200);
    assert!(store.problems.is_empty(), "{:?}", store.problems);
}

#[test]
fn initialized_must_be_first() {
    let mut store = empty_store();
    let registration = cmi5_registration(&mut store, None);
    let statements = vec![cmi5_statement(
        "completed",
        &registration,
        "session-1",
        "s1",
    )];
    post_statements(&mut store, &statements);
    assert!(
        store
            .problems
            .iter()
            .any(|problem| problem.contains("before initialized"))
    );
}

#[test]
fn nothing_may_follow_terminated() {
    let mut store = empty_store();
    let registration = cmi5_registration(&mut store, None);
    let statements = vec![
        cmi5_statement("initialized", &registration, "session-1", "s1"),
        cmi5_statement("terminated", &registration, "session-1", "s2"),
        cmi5_statement("completed", &registration, "session-1", "s3"),
    ];
    post_statements(&mut store, &statements);
    assert!(
        store
            .problems
            .iter()
            .any(|problem| problem.contains("after terminated"))
    );
}

#[test]
fn cmi5_requires_sessionid_and_registration() {
    let mut store = empty_store();
    let registration = cmi5_registration(&mut store, None);
    let mut no_session = cmi5_statement("initialized", &registration, "session-1", "s1");
    if let Some(extensions) = no_session
        .pointer_mut("/context/extensions")
        .and_then(Value::as_object_mut)
    {
        extensions.remove(SESSION_ID_EXTENSION);
    }
    post_statements(&mut store, &[no_session]);
    assert!(
        store
            .problems
            .iter()
            .any(|problem| problem.contains("missing sessionid"))
    );

    let mut no_registration = cmi5_statement("initialized", &registration, "session-2", "s2");
    if let Some(context) = no_registration
        .pointer_mut("/context")
        .and_then(Value::as_object_mut)
    {
        context.remove("registration");
    }
    post_statements(&mut store, &[no_registration]);
    assert!(
        store
            .problems
            .iter()
            .any(|problem| problem.contains("missing registration"))
    );
}

#[test]
fn cmi5_category_rules() {
    let mut store = empty_store();
    let registration = cmi5_registration(&mut store, None);
    let mut missing = cmi5_statement("initialized", &registration, "session-1", "s1");
    category_ids_of(&mut missing).clear();
    post_statements(&mut store, &[missing]);
    assert!(
        store
            .problems
            .iter()
            .any(|problem| problem.contains("missing cmi5 category"))
    );

    let mut answered = cmi5_statement("answered", &registration, "session-2", "s2");
    category_ids_of(&mut answered).push(json!({ "id": CMI5_CATEGORY }));
    let statements = vec![
        cmi5_statement("initialized", &registration, "session-2", "s3"),
        answered,
    ];
    post_statements(&mut store, &statements);
    assert!(
        store
            .problems
            .iter()
            .any(|problem| problem.contains("unexpected cmi5 category"))
    );
}

#[test]
fn moveon_category_rules() {
    let mut store = empty_store();
    let registration = cmi5_registration(&mut store, None);
    let mut missing = cmi5_statement("completed", &registration, "session-1", "s1");
    category_ids_of(&mut missing)
        .retain(|entry| entry.get("id").and_then(Value::as_str) != Some(MOVEON_CATEGORY));
    let statements = vec![
        cmi5_statement("initialized", &registration, "session-1", "s0"),
        missing,
    ];
    post_statements(&mut store, &statements);
    assert!(
        store
            .problems
            .iter()
            .any(|problem| problem.contains("missing moveon category"))
    );

    let mut unexpected = cmi5_statement("initialized", &registration, "session-2", "s2");
    category_ids_of(&mut unexpected).push(json!({ "id": MOVEON_CATEGORY }));
    post_statements(&mut store, &[unexpected]);
    assert!(
        store
            .problems
            .iter()
            .any(|problem| problem.contains("unexpected moveon category"))
    );
}

#[test]
fn duration_rules() {
    let mut store = empty_store();
    let registration = cmi5_registration(&mut store, None);
    let mut completed = cmi5_statement("completed", &registration, "session-1", "s1");
    if let Some(result) = completed
        .pointer_mut("/result")
        .and_then(Value::as_object_mut)
    {
        result.remove("duration");
    }
    let statements = vec![
        cmi5_statement("initialized", &registration, "session-1", "s0"),
        completed,
    ];
    post_statements(&mut store, &statements);
    assert!(
        store
            .problems
            .iter()
            .any(|problem| problem.contains("missing duration"))
    );

    let mut terminated = cmi5_statement("terminated", &registration, "session-2", "s2");
    if let Some(result) = terminated
        .pointer_mut("/result")
        .and_then(Value::as_object_mut)
    {
        result.remove("duration");
    }
    let statements = vec![
        cmi5_statement("initialized", &registration, "session-2", "s3"),
        terminated,
    ];
    post_statements(&mut store, &statements);
    assert!(
        store
            .problems
            .iter()
            .filter(|problem| problem.contains("missing duration"))
            .count()
            >= 2
    );
}

#[test]
fn completed_must_not_carry_a_score() {
    let mut store = empty_store();
    let registration = cmi5_registration(&mut store, None);
    let mut completed = cmi5_statement("completed", &registration, "session-1", "s1");
    if let Some(result) = completed
        .pointer_mut("/result")
        .and_then(Value::as_object_mut)
    {
        result.insert("score".into(), json!({ "scaled": 0.5 }));
    }
    let statements = vec![
        cmi5_statement("initialized", &registration, "session-1", "s0"),
        completed,
    ];
    post_statements(&mut store, &statements);
    assert!(
        store
            .problems
            .iter()
            .any(|problem| problem.contains("must not carry a score"))
    );
}

#[test]
fn completed_and_passed_only_once_per_registration() {
    let mut store = empty_store();
    let registration = cmi5_registration(&mut store, None);
    let statements = vec![
        cmi5_statement("initialized", &registration, "session-1", "s1"),
        cmi5_statement("completed", &registration, "session-1", "s2"),
        cmi5_statement("passed", &registration, "session-1", "s3"),
        cmi5_statement("terminated", &registration, "session-1", "s4"),
        cmi5_statement("initialized", &registration, "session-2", "s5"),
        cmi5_statement("completed", &registration, "session-2", "s6"),
        cmi5_statement("passed", &registration, "session-2", "s7"),
        cmi5_statement("terminated", &registration, "session-2", "s8"),
    ];
    post_statements(&mut store, &statements);
    assert!(
        store
            .problems
            .iter()
            .any(|problem| problem.contains("second completed"))
    );
    assert!(
        store
            .problems
            .iter()
            .any(|problem| problem.contains("second passed"))
    );
}

#[test]
fn failed_after_passed_is_a_problem() {
    let mut store = empty_store();
    let registration = cmi5_registration(&mut store, None);
    let statements = vec![
        cmi5_statement("initialized", &registration, "session-1", "s1"),
        cmi5_statement("passed", &registration, "session-1", "s2"),
        cmi5_statement("terminated", &registration, "session-1", "s3"),
        cmi5_statement("initialized", &registration, "session-2", "s4"),
        cmi5_statement("failed", &registration, "session-2", "s5"),
        cmi5_statement("terminated", &registration, "session-2", "s6"),
    ];
    post_statements(&mut store, &statements);
    assert!(
        store
            .problems
            .iter()
            .any(|problem| problem.contains("failed after passed"))
    );
}

#[test]
fn a_verb_may_not_repeat_in_a_session() {
    let mut store = empty_store();
    let registration = cmi5_registration(&mut store, None);
    let statements = vec![
        cmi5_statement("initialized", &registration, "session-1", "s1"),
        cmi5_statement("initialized", &registration, "session-1", "s2"),
    ];
    post_statements(&mut store, &statements);
    assert!(
        store
            .problems
            .iter()
            .any(|problem| problem.contains("repeated verb"))
    );
}

#[test]
fn browse_may_only_send_initialized_and_terminated() {
    let mut store = empty_store();
    handle(
        &mut store,
        &launch_request("cmi5", "hello", "Browse", None, false),
    );
    let registration = first_registration(&store);
    let statements = vec![
        cmi5_statement("initialized", &registration, "session-1", "s1"),
        cmi5_statement("completed", &registration, "session-1", "s2"),
    ];
    post_statements(&mut store, &statements);
    assert!(
        store
            .problems
            .iter()
            .any(|problem| problem.contains("may only send initialized and terminated"))
    );
}

#[test]
fn mastery_score_and_scaled_must_agree() {
    let mut store = empty_store();
    let registration = cmi5_registration(&mut store, Some("0.8"));
    let statements = vec![
        cmi5_statement("initialized", &registration, "session-1", "s1"),
        cmi5_statement("completed", &registration, "session-1", "s2"),
        cmi5_statement("passed", &registration, "session-1", "s3"),
    ];
    post_statements(&mut store, &statements);
    assert!(
        store
            .problems
            .iter()
            .any(|problem| problem.contains("missing masteryscore"))
    );

    let mut second = empty_store();
    let registration = cmi5_registration(&mut second, Some("0.8"));
    let mut passed = add_extension(
        cmi5_statement("passed", &registration, "session-1", "s3"),
        MASTERY_EXTENSION,
        json!(0.8),
    );
    if let Some(score) = passed
        .pointer_mut("/result/score")
        .and_then(Value::as_object_mut)
    {
        score.insert("scaled".into(), json!(0.5));
    }
    let statements = vec![
        cmi5_statement("initialized", &registration, "session-1", "s1"),
        cmi5_statement("completed", &registration, "session-1", "s2"),
        passed,
    ];
    post_statements(&mut second, &statements);
    assert!(
        second
            .problems
            .iter()
            .any(|problem| problem.contains("scaled score disagrees"))
    );
}

#[test]
fn xapi_package_paths_are_static_files_not_lrs_requests() {
    let mut store = empty_store();
    // No version header: an LRS route would answer 400. A package page is a file.
    let page = request("GET", "/xapi/hello/index.html", "");
    assert_eq!(handle(&mut store, &page).status, 404);
    let statements = request("GET", "/xapi/statements", "");
    assert_eq!(handle(&mut store, &statements).status, 400);
}
