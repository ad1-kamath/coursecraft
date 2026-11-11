//! A small mock LMS and LRS for testing xAPI / cmi5 packages.
//!
//! The server serves the published packages under `target/mock-lrs/`, hands out
//! cmi5 fetch tokens, stores statements and the xAPI State API documents, and
//! checks each incoming statement against the cmi5 rules. Failures are recorded
//! as `problems` rather than refused, so the test page can show them.
//!
//! Everything is test tooling. It binds to `127.0.0.1` only, closes each
//! connection after one request, and never panics on a bad request.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Map, Value, json};

const MAX_BODY: usize = 16 * 1024 * 1024;
const MAX_HEADER: usize = 64 * 1024;
const READ_TIMEOUT: Duration = Duration::from_secs(10);
const CMI5_CATEGORY: &str = "https://w3id.org/xapi/cmi5/context/categories/cmi5";
const MOVEON_CATEGORY: &str = "https://w3id.org/xapi/cmi5/context/categories/moveon";
const SESSION_ID_EXTENSION: &str = "https://w3id.org/xapi/cmi5/context/extensions/sessionid";
const MASTERY_EXTENSION: &str = "https://w3id.org/xapi/cmi5/context/extensions/masteryscore";
const VERB_BASE: &str = "http://adlnet.gov/expapi/verbs/";
const LAUNCH_DATA: &str = "LMS.LaunchData";

#[cfg(test)]
mod tests;

/// One HTTP request, already parsed from the socket.
#[derive(Debug, Clone)]
pub struct Request {
    /// Uppercase HTTP method.
    pub method: String,
    /// Path without the query string.
    pub path: String,
    /// Query string without the leading `?`.
    pub query: String,
    /// Header name/value pairs.
    pub headers: Vec<(String, String)>,
    /// Raw request body.
    pub body: Vec<u8>,
}

impl Request {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

/// One HTTP response.
#[derive(Debug, Clone)]
pub struct Response {
    /// Status code.
    pub status: u16,
    /// Header name/value pairs.
    pub headers: Vec<(String, String)>,
    /// Raw body.
    pub body: Vec<u8>,
}

impl Response {
    pub(crate) fn text(status: u16, body: impl Into<String>) -> Self {
        Self {
            status,
            headers: vec![(
                "Content-Type".to_string(),
                "text/plain; charset=utf-8".to_string(),
            )],
            body: body.into().into_bytes(),
        }
    }

    pub(crate) fn json(status: u16, value: &Value) -> Self {
        Self {
            status,
            headers: vec![("Content-Type".to_string(), "application/json".to_string())],
            body: serde_json::to_vec(value).unwrap_or_default(),
        }
    }

    pub(crate) fn empty(status: u16) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Flavor {
    Cmi5,
    Xapi,
}

impl Flavor {
    fn id(self) -> &'static str {
        match self {
            Self::Cmi5 => "cmi5",
            Self::Xapi => "xapi",
        }
    }
}

#[derive(Debug, Clone)]
struct Registration {
    flavor: Flavor,
    course: String,
    launch_mode: String,
    mastery: Option<f64>,
    completed: u32,
    passed: bool,
}

#[derive(Debug, Default)]
struct Session {
    initialized: bool,
    terminated: bool,
    verbs: HashSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct StateKey {
    state_id: String,
    activity_id: String,
    agent: String,
    registration: Option<String>,
}

/// Everything the mock server remembers between requests.
pub struct LrsStore {
    port: u16,
    base_dir: PathBuf,
    registrations: HashMap<String, Registration>,
    sessions: HashMap<String, Session>,
    fetches: HashMap<String, bool>,
    tokens: HashSet<String>,
    statements: Vec<Value>,
    statement_ids: HashSet<String>,
    problems: Vec<String>,
    state: HashMap<StateKey, Vec<u8>>,
    fail_remaining: u32,
    fail_status: u16,
    next_session: u64,
    next_fetch: u64,
}

impl LrsStore {
    /// Create an empty store for a server on `port` serving `base_dir`.
    #[must_use]
    pub fn new(port: u16, base_dir: PathBuf) -> Self {
        Self {
            port,
            base_dir,
            registrations: HashMap::new(),
            sessions: HashMap::new(),
            fetches: HashMap::new(),
            tokens: HashSet::new(),
            statements: Vec::new(),
            statement_ids: HashSet::new(),
            problems: Vec::new(),
            state: HashMap::new(),
            fail_remaining: 0,
            fail_status: 500,
            next_session: 0,
            next_fetch: 0,
        }
    }
}

struct LaunchData<'a> {
    activity: &'a str,
    agent: &'a Value,
    registration: &'a str,
    session_id: &'a str,
    mode: &'a str,
    mastery: Option<f64>,
    want_return: bool,
}

/// Run the server until the process ends.
///
/// # Errors
/// Returns an error if the listener cannot bind.
pub fn run(port: u16) -> Result<(), String> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("xtask has no parent directory")?;
    let base_dir = root.join("target/mock-lrs");
    let listener = TcpListener::bind(("127.0.0.1", port))
        .map_err(|err| format!("bind 127.0.0.1:{port}: {err}"))?;
    let local = listener
        .local_addr()
        .map_err(|err| format!("local_addr: {err}"))?;
    let store = Arc::new(Mutex::new(LrsStore::new(local.port(), base_dir)));
    println!("mock-lrs: http://127.0.0.1:{}/", local.port());

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let store = Arc::clone(&store);
                std::thread::spawn(move || serve(stream, &store));
            }
            Err(err) => eprintln!("mock-lrs: accept: {err}"),
        }
    }
    Ok(())
}

fn serve(mut stream: TcpStream, store: &Arc<Mutex<LrsStore>>) {
    let _ = stream.set_read_timeout(Some(READ_TIMEOUT));
    let _ = stream.set_write_timeout(Some(READ_TIMEOUT));
    let response = match read_request(&mut stream) {
        Ok(request) => {
            let mut guard = store
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            handle(&mut guard, &request)
        }
        Err(response) => response,
    };
    let _ = write_response(&mut stream, &response);
}

/// Handle one request. Pure: it never touches a socket, so tests can drive it.
pub fn handle(store: &mut LrsStore, request: &Request) -> Response {
    let path = request.path.as_str();
    match path {
        "/mock/launch" => launch(store, request),
        "/mock/reset" => reset(store, request),
        "/mock/log" => log(store, request),
        "/mock/fail" => fail(store, request),
        "/mock/returned" => returned(request),
        _ => {
            if let Some(id) = path.strip_prefix("/fetch/") {
                return fetch(store, request, id);
            }
            // Only the LRS resources: xAPI packages are also served under `/xapi/`.
            if let Some(rest) = path.strip_prefix("/xapi/")
                && matches!(rest, "statements" | "activities/state")
            {
                return xapi(store, request, rest);
            }
            static_file(store, path)
        }
    }
}

fn is_method(request: &Request, method: &str) -> bool {
    request.method.eq_ignore_ascii_case(method)
}

fn launch(store: &mut LrsStore, request: &Request) -> Response {
    if !is_method(request, "POST") {
        return Response::text(405, "Method Not Allowed");
    }
    let params = query_map(&request.query);
    let flavor = match params.get("flavor").map(String::as_str) {
        Some("cmi5") => Flavor::Cmi5,
        Some("xapi") => Flavor::Xapi,
        _ => return Response::text(400, "flavor must be cmi5 or xapi"),
    };
    let course = match params.get("course").map(String::as_str) {
        Some("showcase") => "showcase",
        Some("hello") => "hello",
        _ => return Response::text(400, "course must be showcase or hello"),
    };
    let mode = params.get("mode").map(String::as_str).unwrap_or("Normal");
    if !matches!(mode, "Normal" | "Browse" | "Review") {
        return Response::text(400, "mode must be Normal, Browse or Review");
    }
    let mastery = match params.get("mastery") {
        Some(text) => match text.parse::<f64>() {
            Ok(value) if value.is_finite() && (0.0..=1.0).contains(&value) => Some(value),
            _ => return Response::text(400, "mastery must be a number in 0..=1"),
        },
        None => None,
    };
    let want_return = params.get("return").is_some_and(|value| value == "1");

    let registration = registration_for(store, course, flavor);
    if let Some(entry) = store.registrations.get_mut(&registration) {
        entry.launch_mode = mode.to_string();
        entry.mastery = mastery;
    }

    let base = format!("http://127.0.0.1:{}", store.port);
    let endpoint = format!("{base}/xapi/");
    let activity = format!("urn:coursecraft:mock:activity:{course}");
    let actor = actor_json();
    let actor_text = actor.to_string();

    match flavor {
        Flavor::Cmi5 => {
            store.next_session = store.next_session.saturating_add(1);
            let session_id = format!("mock-session-{}", store.next_session);
            store.next_fetch = store.next_fetch.saturating_add(1);
            let fetch_id = format!("mock-fetch-{}", store.next_fetch);
            store.fetches.insert(fetch_id.clone(), false);
            let fetch_url = format!("{base}/fetch/{fetch_id}");
            write_launch_data(
                store,
                &LaunchData {
                    activity: &activity,
                    agent: &actor,
                    registration: &registration,
                    session_id: &session_id,
                    mode,
                    mastery,
                    want_return,
                },
            );
            let url = format!(
                "{base}/cmi5/{course}/index.html?endpoint={}&actor={}&registration={}&activityId={}&fetch={}",
                encode(&endpoint),
                encode(&actor_text),
                encode(&registration),
                encode(&activity),
                encode(&fetch_url),
            );
            Response::json(200, &json!({ "url": url }))
        }
        Flavor::Xapi => {
            let url = format!(
                "{base}/xapi/{course}/index.html?endpoint={}&actor={}&registration={}&activity_id={}&auth={}",
                encode(&endpoint),
                encode(&actor_text),
                encode(&registration),
                encode(&activity),
                encode("Basic mock-xapi"),
            );
            Response::json(200, &json!({ "url": url }))
        }
    }
}

fn registration_for(store: &mut LrsStore, course: &str, flavor: Flavor) -> String {
    if let Some((id, _)) = store
        .registrations
        .iter()
        .find(|(_, entry)| entry.course == course && entry.flavor == flavor)
    {
        return id.clone();
    }
    let id = format!("urn:coursecraft:mock:registration:{}-{course}", flavor.id());
    store.registrations.insert(
        id.clone(),
        Registration {
            flavor,
            course: course.to_string(),
            launch_mode: "Normal".to_string(),
            mastery: None,
            completed: 0,
            passed: false,
        },
    );
    id
}

fn actor_json() -> Value {
    json!({
        "objectType": "Agent",
        "name": "Learner, Test",
        "account": {
            "homePage": "http://127.0.0.1",
            "name": "learner-1",
        },
    })
}

fn write_launch_data(store: &mut LrsStore, data: &LaunchData<'_>) {
    let mut document = Map::new();
    document.insert(
        "contextTemplate".into(),
        json!({
            "contextActivities": {
                "grouping": [ { "id": data.activity } ],
            },
            "extensions": { SESSION_ID_EXTENSION: data.session_id },
        }),
    );
    document.insert("launchMode".into(), json!(data.mode));
    document.insert(
        "moveOn".into(),
        json!(if data.mastery.is_some() {
            "Passed"
        } else {
            "Completed"
        }),
    );
    document.insert(
        "entitlementKey".into(),
        json!({ "alternate": "coursecraft-mock" }),
    );
    if let Some(score) = data.mastery {
        document.insert("masteryScore".into(), json!(score));
    }
    if data.want_return {
        document.insert(
            "returnURL".into(),
            json!(format!("http://127.0.0.1:{}/mock/returned", store.port)),
        );
    }
    let key = StateKey {
        state_id: LAUNCH_DATA.to_string(),
        activity_id: data.activity.to_string(),
        agent: data.agent.to_string(),
        registration: Some(data.registration.to_string()),
    };
    store.state.insert(
        key,
        serde_json::to_vec(&Value::Object(document)).unwrap_or_default(),
    );
}

fn fetch(store: &mut LrsStore, request: &Request, id: &str) -> Response {
    if !is_method(request, "POST") {
        return Response::text(405, "Method Not Allowed");
    }
    let used = match store.fetches.get(id) {
        Some(used) => *used,
        None => return Response::text(404, "unknown fetch id"),
    };
    if used {
        store.problems.push(format!("fetch id {id} reused"));
        return Response::json(
            200,
            &json!({ "error-code": "1", "error-text": "Already in use" }),
        );
    }
    store.fetches.insert(id.to_string(), true);
    let token = format!("mock-token-{id}");
    store.tokens.insert(format!("Basic {token}"));
    Response::json(200, &json!({ "auth-token": token }))
}

fn reset(store: &mut LrsStore, request: &Request) -> Response {
    if !is_method(request, "POST") {
        return Response::text(405, "Method Not Allowed");
    }
    store.registrations.clear();
    store.sessions.clear();
    store.fetches.clear();
    store.tokens.clear();
    store.statements.clear();
    store.statement_ids.clear();
    store.problems.clear();
    store.state.clear();
    store.fail_remaining = 0;
    store.fail_status = 500;
    store.next_session = 0;
    store.next_fetch = 0;
    Response::json(200, &json!({ "ok": true }))
}

fn log(store: &LrsStore, request: &Request) -> Response {
    if !is_method(request, "GET") {
        return Response::text(405, "Method Not Allowed");
    }
    let state_keys: Vec<Value> = store
        .state
        .keys()
        .map(|key| {
            json!({
                "stateId": key.state_id,
                "activityId": key.activity_id,
                "agent": key.agent,
                "registration": key.registration,
            })
        })
        .collect();
    Response::json(
        200,
        &json!({
            "statements": store.statements.clone(),
            "problems": store.problems.clone(),
            "stateKeys": state_keys,
        }),
    )
}

fn fail(store: &mut LrsStore, request: &Request) -> Response {
    if !is_method(request, "POST") {
        return Response::text(405, "Method Not Allowed");
    }
    let params = query_map(&request.query);
    let count = params
        .get("count")
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(3);
    let status = params
        .get("status")
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(500);
    store.fail_remaining = count;
    store.fail_status = status;
    Response::json(
        200,
        &json!({ "ok": true, "count": count, "status": status }),
    )
}

fn returned(request: &Request) -> Response {
    if !is_method(request, "GET") {
        return Response::text(405, "Method Not Allowed");
    }
    Response {
        status: 200,
        headers: vec![(
            "Content-Type".to_string(),
            "text/html; charset=utf-8".to_string(),
        )],
        body: b"<!DOCTYPE html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>Returned</title></head><body style=\"font:16px system-ui;padding:2rem\"><h1>Returned</h1><p>The learner returned to the LMS.</p></body></html>".to_vec(),
    }
}

fn xapi(store: &mut LrsStore, request: &Request, rest: &str) -> Response {
    if request
        .header("X-Experience-API-Version")
        .is_none_or(str::is_empty)
    {
        return Response::text(400, "X-Experience-API-Version header required");
    }
    let auth = request.header("Authorization").unwrap_or_default();
    if !store.tokens.contains(auth) && auth != "Basic mock-xapi" {
        return Response::text(401, "Unauthorized");
    }
    match (request.method.as_str(), rest) {
        ("POST", "statements") => post_statements(store, request),
        ("GET", "statements") => get_statements(store),
        ("GET", "activities/state")
        | ("PUT", "activities/state")
        | ("DELETE", "activities/state") => state_api(store, request),
        _ => Response::text(404, "Not Found"),
    }
}

fn post_statements(store: &mut LrsStore, request: &Request) -> Response {
    if store.fail_remaining > 0 {
        store.fail_remaining = store.fail_remaining.saturating_sub(1);
        return Response::text(store.fail_status, "injected failure");
    }
    let Ok(value) = serde_json::from_slice::<Value>(&request.body) else {
        return Response::text(400, "invalid JSON");
    };
    let statements = match value {
        Value::Array(items) => items,
        Value::Object(_) => vec![value],
        _ => return Response::text(400, "statement must be an object or array"),
    };
    let mut ids = Vec::new();
    for statement in statements {
        let Some(id) = statement.get("id").and_then(Value::as_str) else {
            return Response::text(400, "statement has no string id");
        };
        let id = id.to_string();
        if !store.statement_ids.contains(&id) {
            store.statement_ids.insert(id.clone());
            validate(store, &statement);
            store.statements.push(statement);
        }
        ids.push(id);
    }
    Response::json(200, &json!(ids))
}

fn get_statements(store: &LrsStore) -> Response {
    Response::json(200, &json!({ "statements": store.statements.clone() }))
}

fn state_api(store: &mut LrsStore, request: &Request) -> Response {
    let params = query_map(&request.query);
    let Some(state_id) = params.get("stateId") else {
        return Response::text(400, "stateId required");
    };
    let Some(activity_id) = params.get("activityId") else {
        return Response::text(400, "activityId required");
    };
    let Some(agent) = params.get("agent") else {
        return Response::text(400, "agent required");
    };
    let key = StateKey {
        state_id: state_id.clone(),
        activity_id: activity_id.clone(),
        agent: agent.clone(),
        registration: params.get("registration").cloned(),
    };
    match request.method.as_str() {
        "GET" => match store.state.get(&key) {
            Some(body) => Response {
                status: 200,
                headers: vec![("Content-Type".to_string(), "application/json".to_string())],
                body: body.clone(),
            },
            None => Response::text(404, "Not Found"),
        },
        "PUT" => {
            if state_id == LAUNCH_DATA {
                store
                    .problems
                    .push("an AU tried to write protected state LMS.LaunchData".to_string());
                return Response::text(403, "LMS.LaunchData is written by the LMS only");
            }
            store.state.insert(key, request.body.clone());
            Response::empty(204)
        }
        "DELETE" => {
            if store.state.remove(&key).is_some() {
                Response::empty(204)
            } else {
                Response::text(404, "Not Found")
            }
        }
        _ => Response::text(405, "Method Not Allowed"),
    }
}

fn validate(store: &mut LrsStore, statement: &Value) {
    let verb = verb_of(statement).to_string();
    let registration = statement
        .pointer("/context/registration")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let session_id = statement
        .pointer(&format!(
            "/context/extensions/{}",
            escape_pointer(SESSION_ID_EXTENSION)
        ))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let info = store.registrations.get(&registration).cloned();
    let categories = category_ids(statement);
    let inferred_cmi5 = !session_id.is_empty() || categories.contains(&CMI5_CATEGORY);
    let flavor = info.as_ref().map_or(
        if inferred_cmi5 {
            Flavor::Cmi5
        } else {
            Flavor::Xapi
        },
        |entry| entry.flavor,
    );
    let mode = info
        .as_ref()
        .map_or_else(|| "Normal".to_string(), |entry| entry.launch_mode.clone());
    let mastery = info.as_ref().and_then(|entry| entry.mastery);

    let mut problems = Vec::new();
    if flavor == Flavor::Cmi5 {
        if registration.is_empty() {
            problems.push(format!("{verb}: missing registration"));
        }
        if session_id.is_empty() {
            problems.push(format!("{verb}: missing sessionid"));
        }
    }

    let session_key = if flavor == Flavor::Cmi5 && !session_id.is_empty() {
        format!("session:{session_id}")
    } else {
        format!("registration:{registration}")
    };
    let after_terminated = {
        let session = store.sessions.entry(session_key).or_default();
        if session.terminated {
            problems.push(format!("{verb}: statement after terminated"));
            true
        } else {
            if verb == "initialized" {
                if session.initialized {
                    problems.push("repeated verb initialized".to_string());
                } else if !session.verbs.is_empty() {
                    problems.push("initialized: not the first statement".to_string());
                }
                session.initialized = true;
            } else if !session.initialized {
                problems.push(format!("{verb}: statement before initialized"));
            }
            if !session.verbs.insert(verb.clone()) && verb != "answered" {
                problems.push(format!("repeated verb {verb}"));
            }
            if verb == "terminated" {
                session.terminated = true;
            }
            false
        }
    };
    store.problems.extend(problems);
    if after_terminated {
        return;
    }

    if flavor == Flavor::Cmi5 {
        validate_categories(store, statement, &verb);
        if mode != "Normal" && !matches!(verb.as_str(), "initialized" | "terminated") {
            store.problems.push(format!(
                "{mode}: may only send initialized and terminated (got {verb})"
            ));
        }
    }
    validate_duration(store, statement, &verb);
    validate_registration(store, &registration, &verb);
    if flavor == Flavor::Cmi5 {
        validate_mastery(store, statement, mastery, &verb);
    }
}

fn validate_categories(store: &mut LrsStore, statement: &Value, verb: &str) {
    let categories = category_ids(statement);
    let defined = matches!(
        verb,
        "initialized" | "completed" | "passed" | "failed" | "terminated"
    );
    if defined && !categories.contains(&CMI5_CATEGORY) {
        store
            .problems
            .push(format!("{verb}: missing cmi5 category"));
    }
    if verb == "answered" && categories.contains(&CMI5_CATEGORY) {
        store
            .problems
            .push("answered: unexpected cmi5 category".to_string());
    }
    let move_on = matches!(verb, "completed" | "passed" | "failed");
    if move_on && !categories.contains(&MOVEON_CATEGORY) {
        store
            .problems
            .push(format!("{verb}: missing moveon category"));
    }
    if !move_on && categories.contains(&MOVEON_CATEGORY) {
        store
            .problems
            .push(format!("{verb}: unexpected moveon category"));
    }
}

fn validate_duration(store: &mut LrsStore, statement: &Value, verb: &str) {
    let needs_duration = matches!(verb, "completed" | "passed" | "failed" | "terminated");
    if needs_duration
        && statement
            .pointer("/result/duration")
            .and_then(Value::as_str)
            .is_none_or(str::is_empty)
    {
        store.problems.push(format!("{verb}: missing duration"));
    }
    if verb == "completed" && statement.pointer("/result/score").is_some() {
        store
            .problems
            .push("completed: must not carry a score".to_string());
    }
}

fn validate_registration(store: &mut LrsStore, registration: &str, verb: &str) {
    let problem = {
        let Some(entry) = store.registrations.get_mut(registration) else {
            return;
        };
        match verb {
            "completed" => {
                entry.completed = entry.completed.saturating_add(1);
                (entry.completed > 1).then(|| "second completed for registration".to_string())
            }
            "passed" => {
                let second = entry.passed;
                entry.passed = true;
                second.then(|| "second passed for registration".to_string())
            }
            "failed" if entry.passed => Some("failed after passed".to_string()),
            _ => None,
        }
    };
    if let Some(problem) = problem {
        store.problems.push(problem);
    }
}

fn validate_mastery(store: &mut LrsStore, statement: &Value, mastery: Option<f64>, verb: &str) {
    let Some(mastery) = mastery else {
        return;
    };
    if !matches!(verb, "passed" | "failed") {
        return;
    }
    let extension = statement
        .pointer(&format!(
            "/context/extensions/{}",
            escape_pointer(MASTERY_EXTENSION)
        ))
        .and_then(Value::as_f64);
    match extension {
        Some(value) if (value - mastery).abs() <= f64::EPSILON => {}
        Some(_) => store.problems.push(format!(
            "{verb}: masteryscore does not match launch mastery"
        )),
        None => store.problems.push(format!("{verb}: missing masteryscore")),
    }
    let Some(scaled) = statement
        .pointer("/result/score/scaled")
        .and_then(Value::as_f64)
    else {
        store.problems.push(format!("{verb}: missing scaled score"));
        return;
    };
    let agrees = if verb == "passed" {
        scaled >= mastery
    } else {
        scaled < mastery
    };
    if !agrees {
        store
            .problems
            .push(format!("{verb}: scaled score disagrees with mastery"));
    }
}

fn verb_of(statement: &Value) -> &str {
    statement
        .pointer("/verb/id")
        .and_then(Value::as_str)
        .and_then(|id| id.strip_prefix(VERB_BASE))
        .unwrap_or_default()
}

fn category_ids(statement: &Value) -> Vec<&str> {
    let mut out = Vec::new();
    let Some(categories) = statement.pointer("/context/contextActivities/category") else {
        return out;
    };
    match categories {
        Value::Array(items) => {
            for item in items {
                if let Some(id) = item.get("id").and_then(Value::as_str) {
                    out.push(id);
                }
            }
        }
        Value::Object(_) => {
            if let Some(id) = categories.get("id").and_then(Value::as_str) {
                out.push(id);
            }
        }
        _ => {}
    }
    out
}

/// Escape a key for a JSON pointer path segment.
fn escape_pointer(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

fn static_file(store: &LrsStore, path: &str) -> Response {
    let decoded = percent_decode(path);
    let Some(relative) = decoded.strip_prefix('/') else {
        return Response::text(404, "Not Found");
    };
    if relative.starts_with('/')
        || relative.contains('\\')
        || relative.as_bytes().get(1).is_some_and(|byte| *byte == b':')
        || relative.split('/').any(|part| part == "..")
    {
        return Response::text(404, "Not Found");
    }
    let relative = if relative.is_empty() {
        "index.html"
    } else {
        relative
    };
    let Some(content_type) = content_type(relative) else {
        return Response::text(404, "Not Found");
    };
    match std::fs::read(store.base_dir.join(relative)) {
        Ok(body) => Response {
            status: 200,
            headers: vec![("Content-Type".to_string(), content_type.to_string())],
            body,
        },
        Err(_) => Response::text(404, "Not Found"),
    }
}

fn content_type(path: &str) -> Option<&'static str> {
    let (_, extension) = path.rsplit_once('.')?;
    match extension.to_ascii_lowercase().as_str() {
        "html" => Some("text/html; charset=utf-8"),
        "js" => Some("text/javascript; charset=utf-8"),
        "wasm" => Some("application/wasm"),
        "json" => Some("application/json"),
        "css" => Some("text/css; charset=utf-8"),
        "xml" => Some("application/xml"),
        "png" => Some("image/png"),
        "jpg" => Some("image/jpeg"),
        "svg" => Some("image/svg+xml"),
        "mp3" => Some("audio/mpeg"),
        "mp4" => Some("video/mp4"),
        "vtt" => Some("text/vtt; charset=utf-8"),
        "woff2" => Some("font/woff2"),
        _ => None,
    }
}

fn query_map(query: &str) -> BTreeMap<String, String> {
    let query = query.strip_prefix('?').unwrap_or(query);
    let mut out = BTreeMap::new();
    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let key = percent_decode(key);
        if key.is_empty() {
            continue;
        }
        out.insert(key, percent_decode(value));
    }
    out
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while let Some(byte) = bytes.get(index).copied() {
        if byte == b'+' {
            out.push(b' ');
            index += 1;
        } else if byte == b'%' {
            match (bytes.get(index + 1), bytes.get(index + 2)) {
                (Some(high), Some(low)) => match (hex_value(*high), hex_value(*low)) {
                    (Some(high), Some(low)) => {
                        out.push(high * 16 + low);
                        index += 3;
                    }
                    _ => {
                        out.push(b'%');
                        index += 1;
                    }
                },
                _ => {
                    out.push(b'%');
                    index += 1;
                }
            }
        } else {
            out.push(byte);
            index += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn encode(value: &str) -> String {
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

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

pub(crate) fn read_request(stream: &mut TcpStream) -> Result<Request, Response> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 8192];
    let header_end = loop {
        if let Some(position) = find_header_end(&buffer) {
            break position;
        }
        if buffer.len() > MAX_HEADER {
            return Err(Response::text(431, "Request Header Fields Too Large"));
        }
        match stream.read(&mut chunk) {
            Ok(0) => return Err(Response::text(400, "Bad Request")),
            Ok(read) => buffer.extend_from_slice(chunk.get(..read).unwrap_or_default()),
            Err(_) => return Err(Response::text(400, "Bad Request")),
        }
    };

    let header_text = String::from_utf8_lossy(buffer.get(..header_end).unwrap_or_default());
    let mut lines = header_text.split("\r\n");
    let request_line = lines.next().unwrap_or_default();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let target = parts.next().unwrap_or_default().to_string();
    if method.is_empty() || target.is_empty() {
        return Err(Response::text(400, "Bad Request"));
    }
    let (path, query) = target
        .split_once('?')
        .map_or((target.as_str(), ""), |(path, query)| (path, query));

    let mut headers = Vec::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.push((name.trim().to_string(), value.trim().to_string()));
        }
    }

    let content_length = headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.parse::<usize>().ok())
        .unwrap_or(0);
    if content_length > MAX_BODY {
        return Err(Response::text(413, "Payload Too Large"));
    }

    let body_start = header_end.saturating_add(4);
    let mut body = buffer.get(body_start..).unwrap_or_default().to_vec();
    while body.len() < content_length {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => body.extend_from_slice(chunk.get(..read).unwrap_or_default()),
            Err(_) => return Err(Response::text(400, "Bad Request")),
        }
    }
    body.truncate(content_length);

    Ok(Request {
        method,
        path: path.to_string(),
        query: query.to_string(),
        headers,
        body,
    })
}

pub(crate) fn write_response(stream: &mut TcpStream, response: &Response) -> std::io::Result<()> {
    let mut head = format!(
        "HTTP/1.1 {} {}\r\n",
        response.status,
        reason_phrase(response.status)
    );
    for (name, value) in &response.headers {
        head.push_str(name);
        head.push_str(": ");
        head.push_str(value);
        head.push_str("\r\n");
    }
    head.push_str("Connection: close\r\n");
    head.push_str(&format!("Content-Length: {}\r\n\r\n", response.body.len()));
    stream.write_all(head.as_bytes())?;
    stream.write_all(&response.body)?;
    stream.flush()
}

fn reason_phrase(status: u16) -> &'static str {
    match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Payload Too Large",
        431 => "Request Header Fields Too Large",
        500 => "Internal Server Error",
        _ => "Status",
    }
}
