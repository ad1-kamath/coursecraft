//! End-to-end tests against a server bound to port 0 and a plain TCP client.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use coursecraft_lti::{Ags, Launch, ToolConfig, jwt};
use serde_json::{Value, json};

use crate::http::{Request, Response, parse_form, read_request};
use crate::server::{Server, Shared};
use crate::{Config, check_text};

const TEST_KEY: &[u8] = include_bytes!("../tests/fixtures/test_key.pk8");
const ISSUER: &str = "https://platform.example";
const CLIENT_ID: &str = "client-1";
const DEPLOYMENT_ID: &str = "deployment-1";
const NONCE_FIELD: &str = "nonce";

fn base_config() -> ToolConfig {
    ToolConfig {
        issuer: ISSUER.into(),
        client_id: CLIENT_ID.into(),
        deployment_id: DEPLOYMENT_ID.into(),
        auth_login_url: "https://platform.example/auth".into(),
        auth_token_url: "https://platform.example/token".into(),
        jwks_url: "https://platform.example/jwks.json".into(),
        tool_url: "https://tool.example".into(),
        private_key_pkcs8: TEST_KEY.to_vec(),
        key_id: "tool-key".into(),
    }
}

fn config_with(jwks_url: &str, token_url: &str) -> ToolConfig {
    let mut config = base_config();
    config.jwks_url = jwks_url.into();
    config.auth_token_url = token_url.into();
    config
}

fn unix_now() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => i64::try_from(duration.as_secs()).unwrap_or(i64::MAX),
        Err(_) => 0,
    }
}

fn temp_course_dir() -> PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let number = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "coursecraft-lti-test-{}-{number}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp course dir");
    dir
}

struct TestServer {
    addr: SocketAddr,
    shared: Arc<Shared>,
}

fn start_server(config: ToolConfig, course_dir: PathBuf) -> TestServer {
    let server = Server::bind(config, course_dir, "127.0.0.1:0").expect("bind server");
    let addr = server.local_addr().expect("local addr");
    let shared = server.shared();
    std::thread::spawn(move || {
        let _ = server.run();
    });
    TestServer { addr, shared }
}

#[derive(Clone, Debug)]
struct Recorded {
    method: String,
    path: String,
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
}

struct Platform {
    addr: SocketAddr,
    requests: Arc<Mutex<Vec<Recorded>>>,
}

fn start_platform(handler: impl Fn(&Request) -> Response + Send + Sync + 'static) -> Platform {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind platform");
    let addr = listener.local_addr().expect("platform addr");
    let requests = Arc::new(Mutex::new(Vec::new()));
    let handler = Arc::new(handler);
    let recorded = Arc::clone(&requests);
    std::thread::spawn(move || {
        for connection in listener.incoming() {
            let Ok(mut stream) = connection else {
                continue;
            };
            let handler = Arc::clone(&handler);
            let records = Arc::clone(&recorded);
            std::thread::spawn(move || {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                let Ok(request) = read_request(&mut stream) else {
                    return;
                };
                lock(&records).push(Recorded {
                    method: request.method.clone(),
                    path: request.path.clone(),
                    headers: request.headers.clone(),
                    body: request.body.clone(),
                });
                let _ = handler(&request).write(&mut stream);
            });
        }
    });
    Platform { addr, requests }
}

fn platform_requests(platform: &Platform) -> Vec<Recorded> {
    lock(&platform.requests).clone()
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn encode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(char::from(byte));
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn request(
    addr: SocketAddr,
    method: &str,
    path: &str,
    extra_headers: &[(&str, &str)],
    body: &str,
) -> (u16, BTreeMap<String, String>, Vec<u8>) {
    let mut raw = format!(
        "{method} {path} HTTP/1.1\r\nHost: test\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    for (name, value) in extra_headers {
        raw.push_str(name);
        raw.push_str(": ");
        raw.push_str(value);
        raw.push_str("\r\n");
    }
    raw.push_str("\r\n");
    raw.push_str(body);
    let mut stream = TcpStream::connect(addr).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("read timeout");
    stream.write_all(raw.as_bytes()).expect("write request");
    stream.flush().expect("flush request");
    read_response(&mut stream)
}

fn read_response(stream: &mut TcpStream) -> (u16, BTreeMap<String, String>, Vec<u8>) {
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 4096];
    let header_end = loop {
        if let Some(position) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            break position + 4;
        }
        let read = stream.read(&mut chunk).expect("read headers");
        assert!(read > 0, "connection closed before headers");
        bytes.extend_from_slice(&chunk[..read]);
    };
    let head = String::from_utf8_lossy(&bytes[..header_end]).to_string();
    let status = head
        .lines()
        .next()
        .expect("status line")
        .split_whitespace()
        .nth(1)
        .expect("status code")
        .parse::<u16>()
        .expect("numeric status");
    let mut headers = BTreeMap::new();
    for line in head.lines().skip(1) {
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }
    let content_length = headers
        .get("content-length")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let mut body = bytes.get(header_end..).unwrap_or_default().to_vec();
    while body.len() < content_length {
        let read = stream.read(&mut chunk).expect("read body");
        if read == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..read]);
    }
    body.truncate(content_length);
    (status, headers, body)
}

fn login_with_cookie(server: &TestServer) -> (String, String, String, String) {
    let path = format!(
        "/lti/login?iss={}&login_hint=user-1&target_link_uri={}&client_id={}",
        encode(ISSUER),
        encode("https://tool.example/lti/launch"),
        encode(CLIENT_ID)
    );
    let (status, headers, _) = request(server.addr, "GET", &path, &[], "");
    assert_eq!(status, 302);
    let location = headers.get("location").expect("login redirect").clone();
    let query = location
        .split_once('?')
        .map(|(_, query)| query)
        .unwrap_or_default();
    let params = parse_form(query);
    let state = params.get("state").expect("state").clone();
    let nonce = params.get(NONCE_FIELD).expect("nonce").clone();
    let set_cookie = headers.get("set-cookie").cloned().unwrap_or_default();
    let cookie_pair = set_cookie.split(';').next().unwrap_or_default().to_string();
    (state, nonce, cookie_pair, set_cookie)
}

fn login(server: &TestServer) -> (String, String) {
    let (state, nonce, _, _) = login_with_cookie(server);
    (state, nonce)
}

fn platform_token(nonce: &str) -> String {
    let header = json!({"alg": "RS256", "typ": "JWT", "kid": "tool-key"});
    let now = unix_now();
    let claims = json!({
        "iss": ISSUER,
        "aud": CLIENT_ID,
        "sub": "user-1",
        "name": "Ada Lovelace",
        "exp": now + 600,
        "iat": now - 10,
        "nonce": nonce,
        "https://purl.imsglobal.org/spec/lti/claim/message_type": "LtiResourceLinkRequest",
        "https://purl.imsglobal.org/spec/lti/claim/version": "1.3.0",
        "https://purl.imsglobal.org/spec/lti/claim/deployment_id": DEPLOYMENT_ID,
        "https://purl.imsglobal.org/spec/lti/claim/target_link_uri": "https://tool.example/course/index.html",
        "https://purl.imsglobal.org/spec/lti/claim/resource_link": {"id": "link-1"},
        "https://purl.imsglobal.org/spec/lti/claim/roles": [
            "http://purl.imsglobal.org/vocab/lis/v2/membership#Learner"
        ]
    });
    jwt::sign_rs256(TEST_KEY, &header, &claims).expect("sign platform token")
}

#[test]
fn health_returns_ok() {
    let server = start_server(
        config_with(
            "https://platform.example/jwks",
            "https://platform.example/token",
        ),
        temp_course_dir(),
    );
    let (status, _, body) = request(server.addr, "GET", "/lti/health", &[], "");
    assert_eq!(status, 200);
    assert_eq!(String::from_utf8_lossy(&body), "ok");
}

#[test]
fn jwks_comes_from_the_fixture_key() {
    let server = start_server(
        config_with(
            "https://platform.example/jwks",
            "https://platform.example/token",
        ),
        temp_course_dir(),
    );
    let (status, headers, body) = request(server.addr, "GET", "/.well-known/jwks.json", &[], "");
    assert_eq!(status, 200);
    assert_eq!(
        headers.get("content-type").map(String::as_str),
        Some("application/json")
    );
    let value: Value = serde_json::from_slice(&body).expect("jwks json");
    assert_eq!(value["keys"][0]["kid"], "tool-key");
    assert_eq!(value["keys"][0]["alg"], "RS256");
    assert!(!value["keys"][0]["n"].as_str().expect("n").is_empty());
}

#[test]
fn login_redirects_and_stores_a_state() {
    let server = start_server(
        config_with(
            "https://platform.example/jwks",
            "https://platform.example/token",
        ),
        temp_course_dir(),
    );
    let (state, nonce) = login(&server);
    assert!(!state.is_empty());
    assert!(!nonce.is_empty());
    assert_eq!(server.shared.pending_logins(), 1);
}

#[test]
fn launch_with_unknown_state_is_400() {
    let server = start_server(
        config_with(
            "https://platform.example/jwks",
            "https://platform.example/token",
        ),
        temp_course_dir(),
    );
    let (status, _, body) = request(
        server.addr,
        "POST",
        "/lti/launch",
        &[("Content-Type", "application/x-www-form-urlencoded")],
        "state=unknown&id_token=token",
    );
    assert_eq!(status, 400);
    assert!(String::from_utf8_lossy(&body).contains("state"));
}

#[test]
fn launch_with_platform_token_sets_a_cookie() {
    let jwks = coursecraft_lti::tool_jwks(&base_config()).expect("tool jwks");
    let platform =
        start_platform(move |_| Response::file(jwks.clone().into_bytes(), "application/json"));
    let server = start_server(
        config_with(
            &format!("http://{}/jwks", platform.addr),
            "https://platform.example/token",
        ),
        temp_course_dir(),
    );
    let (state, nonce, state_cookie, _) = login_with_cookie(&server);
    let token = platform_token(&nonce);
    let body = format!("state={}&id_token={}", encode(&state), encode(&token));
    let (status, headers, _) = request(
        server.addr,
        "POST",
        "/lti/launch",
        &[
            ("Content-Type", "application/x-www-form-urlencoded"),
            ("Cookie", &state_cookie),
        ],
        &body,
    );
    assert_eq!(status, 302);
    assert_eq!(
        headers.get("location").map(String::as_str),
        Some("/course/index.html?name=Ada%20Lovelace")
    );
    let cookie = headers.get("set-cookie").expect("session cookie");
    assert!(cookie.contains("cc_lti="), "{cookie}");
    assert!(cookie.contains("HttpOnly"), "{cookie}");
    assert!(cookie.contains("SameSite=None"), "{cookie}");
    let requests = platform_requests(&platform);
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].path, "/jwks");
}

#[test]
fn course_path_traversal_is_404() {
    let server = start_server(
        config_with(
            "https://platform.example/jwks",
            "https://platform.example/token",
        ),
        temp_course_dir(),
    );
    let (status, _, _) = request(server.addr, "GET", "/course/../lti.json", &[], "");
    assert_eq!(status, 404);
}

#[test]
fn course_serves_static_files() {
    let dir = temp_course_dir();
    std::fs::write(dir.join("index.html"), "<h1>Course</h1>").expect("index");
    std::fs::create_dir_all(dir.join("assets")).expect("assets");
    std::fs::write(dir.join("assets/app.css"), "body { color: red; }").expect("css");
    let server = start_server(
        config_with(
            "https://platform.example/jwks",
            "https://platform.example/token",
        ),
        dir,
    );
    let (status, headers, body) = request(server.addr, "GET", "/course/", &[], "");
    assert_eq!(status, 200);
    assert_eq!(String::from_utf8_lossy(&body), "<h1>Course</h1>");
    assert_eq!(
        headers.get("content-type").map(String::as_str),
        Some("text/html; charset=utf-8")
    );
    let (status, headers, body) = request(server.addr, "GET", "/course/assets/app.css", &[], "");
    assert_eq!(status, 200);
    assert_eq!(String::from_utf8_lossy(&body), "body { color: red; }");
    assert_eq!(
        headers.get("content-type").map(String::as_str),
        Some("text/css; charset=utf-8")
    );
}

#[test]
fn score_without_a_cookie_is_401() {
    let server = start_server(
        config_with(
            "https://platform.example/jwks",
            "https://platform.example/token",
        ),
        temp_course_dir(),
    );
    let (status, _, _) = request(
        server.addr,
        "POST",
        "/lti/score",
        &[
            ("Content-Type", "application/json"),
            ("Origin", "https://tool.example"),
        ],
        r#"{"percent": 50, "status": "passed"}"#,
    );
    assert_eq!(status, 401);
}

#[test]
fn score_posts_to_the_platform_lineitem() {
    let platform =
        start_platform(
            |request| match (request.method.as_str(), request.path.as_str()) {
                ("POST", "/token") => Response::file(
                    br#"{"access_token":"test-token","expires_in":3600}"#.to_vec(),
                    "application/json",
                ),
                ("POST", "/lineitems/1/scores") => Response::status(204),
                _ => Response::status(404),
            },
        );
    let server = start_server(
        config_with(
            "https://platform.example/jwks",
            &format!("http://{}/token", platform.addr),
        ),
        temp_course_dir(),
    );
    let launch = Launch {
        subject: "user-1".into(),
        name: None,
        context_id: None,
        resource_link_id: "link-1".into(),
        roles: Vec::new(),
        ags: Some(Ags {
            lineitem: Some(format!("http://{}/lineitems/1", platform.addr)),
            lineitems: None,
            scope: vec![SCORE_SCOPE_URI.into()],
        }),
        deployment_id: DEPLOYMENT_ID.into(),
    };
    let cookie = server.shared.insert_session(launch);
    let (status, _, _) = request(
        server.addr,
        "POST",
        "/lti/score",
        &[
            ("Content-Type", "application/json"),
            ("Origin", "https://tool.example"),
            ("Cookie", &format!("cc_lti={cookie}")),
        ],
        r#"{"percent": 87, "status": "passed"}"#,
    );
    assert_eq!(status, 204);
    let requests = platform_requests(&platform);
    let token = requests
        .iter()
        .find(|record| record.method == "POST" && record.path == "/token")
        .expect("token request");
    assert!(String::from_utf8_lossy(&token.body).contains("grant_type=client_credentials"));
    let score = requests
        .iter()
        .find(|record| record.method == "POST" && record.path == "/lineitems/1/scores")
        .expect("score request");
    assert_eq!(
        score.headers.get("content-type").map(String::as_str),
        Some("application/vnd.ims.lis.v1.score+json")
    );
    assert_eq!(
        score.headers.get("authorization").map(String::as_str),
        Some("Bearer test-token")
    );
    let value: Value = serde_json::from_slice(&score.body).expect("score json");
    assert_eq!(value["userId"], "user-1");
    assert_eq!(value["scoreGiven"], 87);
    assert_eq!(value["scoreMaximum"], 100);
    assert_eq!(value["comment"], "passed");
}

#[test]
fn score_without_a_percent_sends_progress_only() {
    let platform =
        start_platform(
            |request| match (request.method.as_str(), request.path.as_str()) {
                ("POST", "/token") => Response::file(
                    br#"{"access_token":"test-token","expires_in":3600}"#.to_vec(),
                    "application/json",
                ),
                ("POST", "/lineitems/1/scores") => Response::status(204),
                _ => Response::status(404),
            },
        );
    let server = start_server(
        config_with(
            "https://platform.example/jwks",
            &format!("http://{}/token", platform.addr),
        ),
        temp_course_dir(),
    );
    let launch = Launch {
        subject: "user-1".into(),
        name: None,
        context_id: None,
        resource_link_id: "link-1".into(),
        roles: Vec::new(),
        ags: Some(Ags {
            lineitem: Some(format!("http://{}/lineitems/1", platform.addr)),
            lineitems: None,
            scope: vec![SCORE_SCOPE_URI.into()],
        }),
        deployment_id: DEPLOYMENT_ID.into(),
    };
    let cookie = server.shared.insert_session(launch);
    let (status, _, _) = request(
        server.addr,
        "POST",
        "/lti/score",
        &[
            ("Content-Type", "application/json"),
            ("Origin", "https://tool.example"),
            ("Cookie", &format!("cc_lti={cookie}")),
        ],
        r#"{"percent": null, "status": "completed"}"#,
    );
    assert_eq!(status, 204);
    let requests = platform_requests(&platform);
    let score = requests
        .iter()
        .find(|record| record.method == "POST" && record.path == "/lineitems/1/scores")
        .expect("score request");
    let value: Value = serde_json::from_slice(&score.body).expect("score json");
    assert_eq!(value["userId"], "user-1");
    assert!(value.get("scoreGiven").is_none(), "{value}");
    assert_eq!(value["activityProgress"], "Completed");
    assert_eq!(value["comment"], "completed");
}

#[test]
fn score_rejects_cross_site_origin_with_403() {
    let server = start_server(
        config_with(
            "https://platform.example/jwks",
            "https://platform.example/token",
        ),
        temp_course_dir(),
    );
    let launch = Launch {
        subject: "user-1".into(),
        name: None,
        context_id: None,
        resource_link_id: "link-1".into(),
        roles: Vec::new(),
        ags: Some(Ags {
            lineitem: Some("https://platform.example/lineitems/1".into()),
            lineitems: None,
            scope: Vec::new(),
        }),
        deployment_id: DEPLOYMENT_ID.into(),
    };
    let cookie = server.shared.insert_session(launch);
    let (status, _, _) = request(
        server.addr,
        "POST",
        "/lti/score",
        &[
            ("Content-Type", "application/json"),
            ("Origin", "https://evil.example"),
            ("Cookie", &format!("cc_lti={cookie}")),
        ],
        r#"{"percent": 0, "status": "failed"}"#,
    );
    assert_eq!(status, 403);
}

#[test]
fn score_rejects_missing_content_type_with_403() {
    let server = start_server(
        config_with(
            "https://platform.example/jwks",
            "https://platform.example/token",
        ),
        temp_course_dir(),
    );
    let launch = Launch {
        subject: "user-1".into(),
        name: None,
        context_id: None,
        resource_link_id: "link-1".into(),
        roles: Vec::new(),
        ags: Some(Ags {
            lineitem: Some("https://platform.example/lineitems/1".into()),
            lineitems: None,
            scope: Vec::new(),
        }),
        deployment_id: DEPLOYMENT_ID.into(),
    };
    let cookie = server.shared.insert_session(launch);
    let (status, _, _) = request(
        server.addr,
        "POST",
        "/lti/score",
        &[
            ("Origin", "https://tool.example"),
            ("Cookie", &format!("cc_lti={cookie}")),
        ],
        r#"{"percent": 0, "status": "failed"}"#,
    );
    assert_eq!(status, 403);
}

#[test]
fn score_accepts_json_with_charset_and_tool_origin() {
    let platform =
        start_platform(
            |request| match (request.method.as_str(), request.path.as_str()) {
                ("POST", "/token") => Response::file(
                    br#"{"access_token":"test-token","expires_in":3600}"#.to_vec(),
                    "application/json",
                ),
                ("POST", "/lineitems/1/scores") => Response::status(204),
                _ => Response::status(404),
            },
        );
    let mut cfg = config_with(
        "https://platform.example/jwks",
        &format!("http://{}/token", platform.addr),
    );
    cfg.tool_url = "https://tool.example/lti-app".into();
    let server = start_server(cfg, temp_course_dir());
    let launch = Launch {
        subject: "user-1".into(),
        name: None,
        context_id: None,
        resource_link_id: "link-1".into(),
        roles: Vec::new(),
        ags: Some(Ags {
            lineitem: Some(format!("http://{}/lineitems/1", platform.addr)),
            lineitems: None,
            scope: vec![SCORE_SCOPE_URI.into()],
        }),
        deployment_id: DEPLOYMENT_ID.into(),
    };
    let cookie = server.shared.insert_session(launch);
    let (status, _, _) = request(
        server.addr,
        "POST",
        "/lti/score",
        &[
            ("Content-Type", "application/json; charset=utf-8"),
            ("Origin", "https://tool.example"),
            ("Cookie", &format!("cc_lti={cookie}")),
        ],
        r#"{"percent": 90, "status": "passed"}"#,
    );
    assert_eq!(status, 204);
}

#[test]
fn oversized_body_is_413() {
    let server = start_server(
        config_with(
            "https://platform.example/jwks",
            "https://platform.example/token",
        ),
        temp_course_dir(),
    );
    let mut stream = TcpStream::connect(server.addr).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("read timeout");
    stream
        .write_all(b"POST /lti/score HTTP/1.1\r\nHost: test\r\nContent-Length: 2097152\r\n\r\n")
        .expect("write headers");
    let (status, _, _) = read_response(&mut stream);
    assert_eq!(status, 413);
}

#[test]
fn check_prints_the_tool_urls() {
    let dir = temp_course_dir();
    std::fs::write(dir.join("key.pk8"), TEST_KEY).expect("key file");
    let config_json = json!({
        "issuer": ISSUER,
        "client_id": CLIENT_ID,
        "deployment_id": DEPLOYMENT_ID,
        "auth_login_url": "https://platform.example/auth",
        "auth_token_url": "https://platform.example/token",
        "jwks_url": "https://platform.example/jwks",
        "tool_url": "https://tool.example",
        "private_key_file": "key.pk8",
        "key_id": "tool-key",
        "listen": "127.0.0.1:8787",
        "course_dir": "./published"
    });
    let path = dir.join("lti.json");
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&config_json).expect("config json"),
    )
    .expect("config file");
    let config = Config::load(&path).expect("load config");
    assert_eq!(config.listen, "127.0.0.1:8787");
    assert_eq!(config.course_dir(), dir.join("./published"));
    let tool = config.tool_config().expect("tool config");
    let text = check_text(&tool);
    assert!(text.contains("https://tool.example/lti/login"));
    assert!(text.contains("https://tool.example/lti/launch"));
    assert!(text.contains("https://tool.example/.well-known/jwks.json"));
    assert!(text.contains("tool-key"));
}

#[test]
fn launch_redirect_keeps_tool_url_path_prefix() {
    let jwks = coursecraft_lti::tool_jwks(&base_config()).expect("tool jwks");
    let platform =
        start_platform(move |_| Response::file(jwks.clone().into_bytes(), "application/json"));
    let mut cfg = config_with(
        &format!("http://{}/jwks", platform.addr),
        "https://platform.example/token",
    );
    cfg.tool_url = "https://tool.example/lti-app".into();
    let server = start_server(cfg, temp_course_dir());
    let (state, nonce, state_cookie, _) = login_with_cookie(&server);
    let token = platform_token(&nonce);
    let body = format!("state={}&id_token={}", encode(&state), encode(&token));
    let (status, headers, _) = request(
        server.addr,
        "POST",
        "/lti/launch",
        &[
            ("Content-Type", "application/x-www-form-urlencoded"),
            ("Cookie", &state_cookie),
        ],
        &body,
    );
    assert_eq!(status, 302);
    assert_eq!(
        headers.get("location").map(String::as_str),
        Some("/lti-app/course/index.html?name=Ada%20Lovelace")
    );
}

#[test]
fn launch_refetches_jwks_once_on_unknown_kid_and_rate_limits_retries() {
    let base_jwks: Value =
        serde_json::from_str(&coursecraft_lti::tool_jwks(&base_config()).expect("tool jwks"))
            .expect("jwks json");
    let mut rotated_jwks = base_jwks.clone();
    rotated_jwks["keys"][0]["kid"] = json!("rotated-key");

    let serve_rotated = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let serve_flag = std::sync::Arc::clone(&serve_rotated);
    let initial_bytes = serde_json::to_vec(&base_jwks).expect("initial jwks");
    let rotated_bytes = serde_json::to_vec(&rotated_jwks).expect("rotated jwks");
    let platform = start_platform(move |_| {
        let bytes = if serve_flag.load(std::sync::atomic::Ordering::SeqCst) {
            rotated_bytes.clone()
        } else {
            initial_bytes.clone()
        };
        Response::file(bytes, "application/json")
    });
    let server = start_server(
        config_with(
            &format!("http://{}/jwks", platform.addr),
            "https://platform.example/token",
        ),
        temp_course_dir(),
    );

    // First launch caches "tool-key".
    let (state, nonce, state_cookie, _) = login_with_cookie(&server);
    let body = format!(
        "state={}&id_token={}",
        encode(&state),
        encode(&platform_token(&nonce))
    );
    let (status, _, _) = request(
        server.addr,
        "POST",
        "/lti/launch",
        &[
            ("Content-Type", "application/x-www-form-urlencoded"),
            ("Cookie", &state_cookie),
        ],
        &body,
    );
    assert_eq!(status, 302);
    assert_eq!(platform_requests(&platform).len(), 1);

    // Platform rotates its key; launch with "rotated-key" triggers one JWKS refetch.
    serve_rotated.store(true, std::sync::atomic::Ordering::SeqCst);
    let (state, nonce, state_cookie, _) = login_with_cookie(&server);
    let header = json!({"alg": "RS256", "typ": "JWT", "kid": "rotated-key"});
    let now = unix_now();
    let claims = json!({
        "iss": ISSUER,
        "aud": CLIENT_ID,
        "sub": "user-1",
        "exp": now + 600,
        "iat": now - 10,
        "nonce": nonce,
        "https://purl.imsglobal.org/spec/lti/claim/message_type": "LtiResourceLinkRequest",
        "https://purl.imsglobal.org/spec/lti/claim/version": "1.3.0",
        "https://purl.imsglobal.org/spec/lti/claim/deployment_id": DEPLOYMENT_ID,
        "https://purl.imsglobal.org/spec/lti/claim/target_link_uri": "https://tool.example/course/index.html",
        "https://purl.imsglobal.org/spec/lti/claim/resource_link": {"id": "link-1"},
        "https://purl.imsglobal.org/spec/lti/claim/roles": [
            "http://purl.imsglobal.org/vocab/lis/v2/membership#Learner"
        ]
    });
    let rotated_token = jwt::sign_rs256(TEST_KEY, &header, &claims).expect("sign rotated");
    let body = format!(
        "state={}&id_token={}",
        encode(&state),
        encode(&rotated_token)
    );
    let (status, _, _) = request(
        server.addr,
        "POST",
        "/lti/launch",
        &[
            ("Content-Type", "application/x-www-form-urlencoded"),
            ("Cookie", &state_cookie),
        ],
        &body,
    );
    assert_eq!(status, 302);
    assert_eq!(platform_requests(&platform).len(), 2);

    // Another unknown kid within 60 s is rate-limited and does not refetch JWKS.
    let (state, nonce, state_cookie, _) = login_with_cookie(&server);
    let header = json!({"alg": "RS256", "typ": "JWT", "kid": "missing-key"});
    let mut claims = claims;
    claims["nonce"] = json!(nonce);
    let missing_token = jwt::sign_rs256(TEST_KEY, &header, &claims).expect("sign missing");
    let body = format!(
        "state={}&id_token={}",
        encode(&state),
        encode(&missing_token)
    );
    let (status, _, _) = request(
        server.addr,
        "POST",
        "/lti/launch",
        &[
            ("Content-Type", "application/x-www-form-urlencoded"),
            ("Cookie", &state_cookie),
        ],
        &body,
    );
    assert_eq!(status, 400);
    assert_eq!(platform_requests(&platform).len(), 2);
}

#[test]
fn login_sets_state_cookie_and_launch_requires_it() {
    let jwks = coursecraft_lti::tool_jwks(&base_config()).expect("tool jwks");
    let platform =
        start_platform(move |_| Response::file(jwks.clone().into_bytes(), "application/json"));
    let server = start_server(
        config_with(
            &format!("http://{}/jwks", platform.addr),
            "https://platform.example/token",
        ),
        temp_course_dir(),
    );
    let (state, nonce, state_cookie, set_cookie) = login_with_cookie(&server);
    assert!(set_cookie.contains("cc_lti_state="), "{set_cookie}");
    assert!(set_cookie.contains("Max-Age=600"), "{set_cookie}");
    assert!(set_cookie.contains("HttpOnly"), "{set_cookie}");
    assert!(set_cookie.contains("Secure"), "{set_cookie}");
    assert!(set_cookie.contains("SameSite=None"), "{set_cookie}");

    let token = platform_token(&nonce);
    let body = format!("state={}&id_token={}", encode(&state), encode(&token));
    let (status, _, _) = request(
        server.addr,
        "POST",
        "/lti/launch",
        &[("Content-Type", "application/x-www-form-urlencoded")],
        &body,
    );
    assert_eq!(status, 400);

    let (status, _, _) = request(
        server.addr,
        "POST",
        "/lti/launch",
        &[
            ("Content-Type", "application/x-www-form-urlencoded"),
            ("Cookie", "cc_lti_state=wrong-hash"),
        ],
        &body,
    );
    assert_eq!(status, 400);

    // The legitimate browser still has its pending login intact.
    let (status, _, _) = request(
        server.addr,
        "POST",
        "/lti/launch",
        &[
            ("Content-Type", "application/x-www-form-urlencoded"),
            ("Cookie", &state_cookie),
        ],
        &body,
    );
    assert_eq!(status, 302);
}

#[test]
fn invalid_login_does_not_insert_into_login_table() {
    let server = start_server(
        config_with(
            "https://platform.example/jwks",
            "https://platform.example/token",
        ),
        temp_course_dir(),
    );
    let path = format!(
        "/lti/login?iss={}&login_hint=user-1&target_link_uri={}",
        encode("https://wrong-issuer.example"),
        encode("https://tool.example/lti/launch"),
    );
    let (status, _, _) = request(server.addr, "GET", &path, &[], "");
    assert_eq!(status, 400);
    assert_eq!(server.shared.pending_logins(), 0);
}

#[test]
fn login_table_evicts_entry_with_oldest_expires_when_full() {
    let server = start_server(
        config_with(
            "https://platform.example/jwks",
            "https://platform.example/token",
        ),
        temp_course_dir(),
    );
    let now = std::time::Instant::now();
    // "z-oldest" is lexicographically last, so key-order eviction would keep it,
    // but it has the oldest expires and must be evicted first.
    server
        .shared
        .insert_pending_login("z-oldest", now + std::time::Duration::from_secs(30));
    for i in 0..9_999 {
        server.shared.insert_pending_login(
            &format!("a-newer-{i:04}"),
            now + std::time::Duration::from_secs(300),
        );
    }
    assert_eq!(server.shared.pending_logins(), 10_000);

    let _ = login_with_cookie(&server);
    assert_eq!(server.shared.pending_logins(), 10_000);
    assert!(!server.shared.has_pending_login("z-oldest"));
    assert!(server.shared.has_pending_login("a-newer-0000"));
}

#[test]
fn course_files_return_cache_control_etag_304_and_206_range() {
    let dir = temp_course_dir();
    std::fs::write(dir.join("video.mp4"), b"0123456789abcdef").unwrap();
    let server = start_server(
        config_with(
            "https://platform.example/jwks",
            "https://platform.example/token",
        ),
        dir,
    );

    let (status, headers, body) = request(server.addr, "GET", "/course/video.mp4", &[], "");
    assert_eq!(status, 200);
    assert_eq!(body, b"0123456789abcdef");
    assert!(headers.contains_key("cache-control"));
    assert_eq!(
        headers.get("accept-ranges").map(String::as_str),
        Some("bytes")
    );
    let etag = headers.get("etag").expect("ETag header").clone();

    // If-None-Match returns 304 Not Modified with empty body.
    let (status_304, headers_304, body_304) = request(
        server.addr,
        "GET",
        "/course/video.mp4",
        &[("If-None-Match", &etag)],
        "",
    );
    assert_eq!(status_304, 304);
    assert!(body_304.is_empty());
    assert_eq!(headers_304.get("etag"), Some(&etag));

    // Single byte range returns 206 Partial Content.
    let (status_206, headers_206, body_206) = request(
        server.addr,
        "GET",
        "/course/video.mp4",
        &[("Range", "bytes=4-9")],
        "",
    );
    assert_eq!(status_206, 206);
    assert_eq!(body_206, b"456789");
    assert_eq!(
        headers_206.get("content-range").map(String::as_str),
        Some("bytes 4-9/16")
    );

    // Out-of-bounds range returns 416 Range Not Satisfiable.
    let (status_416, headers_416, _) = request(
        server.addr,
        "GET",
        "/course/video.mp4",
        &[("Range", "bytes=99-120")],
        "",
    );
    assert_eq!(status_416, 416);
    assert_eq!(
        headers_416.get("content-range").map(String::as_str),
        Some("bytes */16")
    );
}

#[test]
fn ags_token_is_cached_per_server_across_learners() {
    let token_hits = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let token_hits_srv = Arc::clone(&token_hits);
    let score_hits = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let score_hits_srv = Arc::clone(&score_hits);

    let jwks = coursecraft_lti::tool_jwks(&base_config()).expect("tool jwks");
    let platform = start_platform(move |req| match req.path.as_str() {
        "/jwks" => Response::file(jwks.clone().into_bytes(), "application/json"),
        "/token" => {
            token_hits_srv.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Response::json(
                200,
                &json!({ "access_token": "server-wide-tok", "expires_in": 3600 }),
            )
        }
        "/lineitems/1/scores" => {
            score_hits_srv.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Response::status(204)
        }
        _ => Response::text(404, "not found"),
    });

    let cfg = config_with(
        &format!("http://{}/jwks", platform.addr),
        &format!("http://{}/token", platform.addr),
    );
    let server = start_server(cfg, temp_course_dir());

    // Launch two different learners (two separate sessions).
    let lineitem = format!("http://{}/lineitems/1", platform.addr);
    let make_launch = |subject: &str| coursecraft_lti::Launch {
        subject: subject.into(),
        name: None,
        context_id: None,
        resource_link_id: "rl-1".into(),
        roles: Vec::new(),
        ags: Some(coursecraft_lti::Ags {
            lineitem: Some(lineitem.clone()),
            lineitems: None,
            scope: vec!["https://purl.imsglobal.org/spec/lti-ags/scope/score".into()],
        }),
        deployment_id: DEPLOYMENT_ID.into(),
    };
    let cookie_a = server.shared.insert_session(make_launch("learner-a"));
    let cookie_b = server.shared.insert_session(make_launch("learner-b"));
    assert_ne!(cookie_a, cookie_b);

    let (st_a, _, _) = request(
        server.addr,
        "POST",
        "/lti/score",
        &[
            ("Cookie", &format!("cc_lti={cookie_a}")),
            ("Content-Type", "application/json"),
            ("Origin", "https://tool.example"),
        ],
        r#"{"percent":80,"status":"passed"}"#,
    );
    assert_eq!(st_a, 204);

    let (st_b, _, _) = request(
        server.addr,
        "POST",
        "/lti/score",
        &[
            ("Cookie", &format!("cc_lti={cookie_b}")),
            ("Content-Type", "application/json"),
            ("Origin", "https://tool.example"),
        ],
        r#"{"percent":90,"status":"passed"}"#,
    );
    assert_eq!(st_b, 204);
    assert_eq!(score_hits.load(std::sync::atomic::Ordering::SeqCst), 2);
    assert_eq!(
        token_hits.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "second learner must reuse the server-cached AGS token"
    );
}

// ===========================================================================
// M27 Step A-6: StrictLtiPlatform conformance suite (A4.1-A4.8, A4.10)
// ===========================================================================

const SCORE_SCOPE_URI: &str = "https://purl.imsglobal.org/spec/lti-ags/scope/score";

fn strict_tool_config(
    platform: &crate::strict_platform::StrictLtiPlatform,
    tool_url: &str,
) -> ToolConfig {
    ToolConfig {
        issuer: platform.issuer(),
        client_id: CLIENT_ID.into(),
        deployment_id: DEPLOYMENT_ID.into(),
        auth_login_url: platform.auth_login_url(),
        auth_token_url: platform.auth_token_url(),
        jwks_url: platform.jwks_url(),
        tool_url: tool_url.into(),
        private_key_pkcs8: TEST_KEY.to_vec(),
        key_id: "tool-key".into(),
    }
}

fn oidc_login(
    server: &TestServer,
    platform: &crate::strict_platform::StrictLtiPlatform,
    target_link_uri: &str,
) -> (String, String, String) {
    let login_path = format!(
        "/lti/login?iss={}&login_hint=user-1&target_link_uri={}&client_id={}",
        encode(&platform.issuer()),
        encode(target_link_uri),
        encode(CLIENT_ID),
    );
    let (status, headers, _) = request(server.addr, "GET", &login_path, &[], "");
    assert_eq!(status, 302, "login must redirect");
    let location = headers.get("location").expect("location header");
    let set_cookie = headers.get("set-cookie").expect("state cookie");
    let state_cookie = set_cookie
        .split(';')
        .next()
        .expect("cookie pair")
        .to_string();

    // Forward the OIDC redirect to StrictLtiPlatform so it validates all OIDC params.
    let oidc_path = location
        .strip_prefix(&format!("http://{}", platform.addr()))
        .expect("redirect points to strict platform");
    let (oidc_status, _, _) = request(platform.addr(), "GET", oidc_path, &[], "");
    assert_eq!(oidc_status, 200, "strict platform must accept OIDC params");

    let params = platform.last_oidc_params().expect("recorded oidc params");
    let state = params.get("state").expect("state").clone();
    let nonce = params.get("nonce").expect("nonce").clone();
    (state, nonce, state_cookie)
}

fn do_launch(
    server: &TestServer,
    state: &str,
    state_cookie: &str,
    id_token: &str,
) -> (u16, BTreeMap<String, String>, Vec<u8>) {
    let body = format!("state={}&id_token={}", encode(state), encode(id_token));
    request(
        server.addr,
        "POST",
        "/lti/launch",
        &[
            ("Content-Type", "application/x-www-form-urlencoded"),
            ("Cookie", state_cookie),
        ],
        &body,
    )
}

fn session_cookie_from_headers(headers: &BTreeMap<String, String>) -> String {
    headers
        .get("set-cookie")
        .expect("cc_lti set-cookie")
        .split(';')
        .next()
        .expect("cc_lti pair")
        .to_string()
}

#[test]
fn a4_1_full_oidc_launch_course_and_ags_pass_score() {
    let platform = crate::strict_platform::StrictLtiPlatform::start(CLIENT_ID).expect("platform");
    let dir = temp_course_dir();
    std::fs::write(dir.join("index.html"), "<html>course</html>").expect("index.html");
    std::fs::write(dir.join("course.json"), r#"{"title":"Test"}"#).expect("course.json");

    let server = start_server(strict_tool_config(&platform, "https://tool.example"), dir);
    platform.set_tool_jwks_url(format!("http://{}/.well-known/jwks.json", server.addr));

    let (state, nonce, state_cookie) =
        oidc_login(&server, &platform, "https://tool.example/course/index.html");
    let lineitem = platform.lineitem_url("1");
    let claims = platform.default_claims(
        &nonce,
        unix_now(),
        "https://tool.example/course/index.html",
        Some(&lineitem),
        &[SCORE_SCOPE_URI],
    );
    let token = platform.sign_id_token(&claims).expect("sign id_token");
    let (status, headers, _) = do_launch(&server, &state, &state_cookie, &token);
    assert_eq!(status, 302);
    let location = headers.get("location").expect("redirect location");
    assert_eq!(location, "/course/index.html?name=Ada%20Lovelace");
    let session_cookie = session_cookie_from_headers(&headers);

    let (html_status, _, html_body) = request(server.addr, "GET", "/course/index.html", &[], "");
    assert_eq!(html_status, 200);
    assert_eq!(String::from_utf8_lossy(&html_body), "<html>course</html>");

    let (json_status, _, json_body) = request(server.addr, "GET", "/course/course.json", &[], "");
    assert_eq!(json_status, 200);
    assert_eq!(String::from_utf8_lossy(&json_body), r#"{"title":"Test"}"#);

    let (score_status, _, _) = request(
        server.addr,
        "POST",
        "/lti/score",
        &[
            ("Cookie", &session_cookie),
            ("Content-Type", "application/json"),
            ("Origin", "https://tool.example"),
        ],
        r#"{"percent":85,"status":"passed"}"#,
    );
    assert_eq!(score_status, 204);
    assert!(
        platform.violations().is_empty(),
        "violations: {:?}",
        platform.violations()
    );
    let scores = platform.scores();
    assert_eq!(scores.len(), 1);
    assert_eq!(scores[0].payload["scoreGiven"], 85);
    assert_eq!(scores[0].payload["scoreMaximum"], 100);
    assert_eq!(scores[0].payload["activityProgress"], "Completed");
    assert_eq!(scores[0].payload["gradingProgress"], "FullyGraded");
    assert_eq!(scores[0].payload["comment"], "passed");
}

#[test]
fn a4_2_completion_only_and_incomplete_ags_payloads() {
    let platform = crate::strict_platform::StrictLtiPlatform::start(CLIENT_ID).expect("platform");
    let server = start_server(
        strict_tool_config(&platform, "https://tool.example"),
        temp_course_dir(),
    );
    platform.set_tool_jwks_url(format!("http://{}/.well-known/jwks.json", server.addr));

    let (state, nonce, state_cookie) =
        oidc_login(&server, &platform, "https://tool.example/course/index.html");
    let lineitem = platform.lineitem_url("2");
    let claims = platform.default_claims(
        &nonce,
        unix_now(),
        "https://tool.example/course/index.html",
        Some(&lineitem),
        &[SCORE_SCOPE_URI],
    );
    let token = platform.sign_id_token(&claims).expect("sign id_token");
    let (status, headers, _) = do_launch(&server, &state, &state_cookie, &token);
    assert_eq!(status, 302);
    let session_cookie = session_cookie_from_headers(&headers);

    let (s1, _, _) = request(
        server.addr,
        "POST",
        "/lti/score",
        &[
            ("Cookie", &session_cookie),
            ("Content-Type", "application/json"),
            ("Origin", "https://tool.example"),
        ],
        r#"{"percent":null,"status":"completed"}"#,
    );
    assert_eq!(s1, 204);

    let (s2, _, _) = request(
        server.addr,
        "POST",
        "/lti/score",
        &[
            ("Cookie", &session_cookie),
            ("Content-Type", "application/json"),
            ("Origin", "https://tool.example"),
        ],
        r#"{"percent":null,"status":"incomplete"}"#,
    );
    assert_eq!(s2, 204);

    assert!(
        platform.violations().is_empty(),
        "violations: {:?}",
        platform.violations()
    );
    let scores = platform.scores();
    assert_eq!(scores.len(), 2);
    assert!(scores[0].payload.get("scoreGiven").is_none());
    assert!(scores[0].payload.get("scoreMaximum").is_none());
    assert_eq!(scores[0].payload["activityProgress"], "Completed");
    assert_eq!(scores[0].payload["gradingProgress"], "FullyGraded");

    assert!(scores[1].payload.get("scoreGiven").is_none());
    assert_eq!(scores[1].payload["activityProgress"], "InProgress");
    assert_eq!(scores[1].payload["gradingProgress"], "Pending");
}

#[test]
fn a4_3_trailing_slash_lineitem_and_query_string_preservation() {
    let platform = crate::strict_platform::StrictLtiPlatform::start(CLIENT_ID).expect("platform");
    let server = start_server(
        strict_tool_config(&platform, "https://tool.example"),
        temp_course_dir(),
    );
    platform.set_tool_jwks_url(format!("http://{}/.well-known/jwks.json", server.addr));

    let lineitem_trailing = format!("http://{}/ags/lineitems/42/", platform.addr());
    let lineitem_query = format!("http://{}/ags/lineitems/42/?ctx=9", platform.addr());

    for (idx, lineitem) in [lineitem_trailing, lineitem_query].iter().enumerate() {
        let (state, nonce, state_cookie) =
            oidc_login(&server, &platform, "https://tool.example/course/index.html");
        let claims = platform.default_claims(
            &nonce,
            unix_now(),
            "https://tool.example/course/index.html",
            Some(lineitem),
            &[SCORE_SCOPE_URI],
        );
        let token = platform.sign_id_token(&claims).expect("sign");
        let (status, headers, _) = do_launch(&server, &state, &state_cookie, &token);
        assert_eq!(status, 302);
        let session_cookie = session_cookie_from_headers(&headers);
        let (score_status, _, _) = request(
            server.addr,
            "POST",
            "/lti/score",
            &[
                ("Cookie", &session_cookie),
                ("Content-Type", "application/json"),
                ("Origin", "https://tool.example"),
            ],
            r#"{"percent":90,"status":"passed"}"#,
        );
        assert_eq!(score_status, 204, "case {idx} failed");
    }

    assert!(
        platform.violations().is_empty(),
        "violations: {:?}",
        platform.violations()
    );
    let scores = platform.scores();
    assert_eq!(scores.len(), 2);
    assert_eq!(scores[0].path, "/ags/lineitems/42/scores");
    assert!(scores[0].query.is_empty());
    assert_eq!(scores[1].path, "/ags/lineitems/42/scores");
    assert_eq!(scores[1].query.get("ctx").map(String::as_str), Some("9"));
}

#[test]
fn a4_4_missing_ags_score_scope_returns_403_without_token_request() {
    let platform = crate::strict_platform::StrictLtiPlatform::start(CLIENT_ID).expect("platform");
    let server = start_server(
        strict_tool_config(&platform, "https://tool.example"),
        temp_course_dir(),
    );
    platform.set_tool_jwks_url(format!("http://{}/.well-known/jwks.json", server.addr));

    let (state, nonce, state_cookie) =
        oidc_login(&server, &platform, "https://tool.example/course/index.html");
    let lineitem = platform.lineitem_url("4");
    let claims = platform.default_claims(
        &nonce,
        unix_now(),
        "https://tool.example/course/index.html",
        Some(&lineitem),
        &["https://purl.imsglobal.org/spec/lti-ags/scope/lineitem.readonly"],
    );
    let token = platform.sign_id_token(&claims).expect("sign");
    let (status, headers, _) = do_launch(&server, &state, &state_cookie, &token);
    assert_eq!(status, 302);
    let session_cookie = session_cookie_from_headers(&headers);

    let (score_status, _, _) = request(
        server.addr,
        "POST",
        "/lti/score",
        &[
            ("Cookie", &session_cookie),
            ("Content-Type", "application/json"),
            ("Origin", "https://tool.example"),
        ],
        r#"{"percent":80,"status":"passed"}"#,
    );
    assert_eq!(score_status, 403);
    assert_eq!(platform.token_requests_count(), 0);
    assert!(platform.scores().is_empty());
}

#[test]
fn a4_5_id_token_negative_security_matrix() {
    let platform = crate::strict_platform::StrictLtiPlatform::start(CLIENT_ID).expect("platform");
    let server = start_server(
        strict_tool_config(&platform, "https://tool.example"),
        temp_course_dir(),
    );

    let now = unix_now();
    let base_claims = |nonce: &str| {
        platform.default_claims(
            nonce,
            now,
            "https://tool.example/course/index.html",
            None,
            &[],
        )
    };

    // 1. Tampered signature
    {
        let (state, nonce, cookie) = oidc_login(&server, &platform, "https://tool.example");
        let mut tok = platform.sign_id_token(&base_claims(&nonce)).expect("sign");
        let dot = tok.rfind('.').expect("jwt dot");
        let flipped = if tok.as_bytes().get(dot + 1) == Some(&b'A') {
            "B"
        } else {
            "A"
        };
        tok.replace_range(dot + 1..dot + 2, flipped);
        let (st, _, _) = do_launch(&server, &state, &cookie, &tok);
        assert_eq!(st, 400);
    }

    // 2. alg: "none" and alg: "HS256"
    for alg in ["none", "HS256"] {
        let (state, nonce, cookie) = oidc_login(&server, &platform, "https://tool.example");
        let header = json!({"alg": alg, "typ": "JWT", "kid": "kid-1"});
        let h_b64 = jwt::base64url_encode(header.to_string().as_bytes());
        let p_b64 = jwt::base64url_encode(base_claims(&nonce).to_string().as_bytes());
        let tok = format!("{h_b64}.{p_b64}.sig");
        let (st, _, _) = do_launch(&server, &state, &cookie, &tok);
        assert_eq!(st, 400, "alg {alg} must be 400");
    }

    // 3. Expired exp, future iat, wrong iss, wrong aud, multi-aud wrong azp, wrong nonce,
    //    wrong message_type, wrong version, wrong deployment_id, missing target_link_uri, missing roles
    type ClaimMutator = (&'static str, Box<dyn Fn(&mut Value)>);
    let mut mutators: Vec<ClaimMutator> = vec![
        ("expired", Box::new(move |c| c["exp"] = json!(now - 10))),
        ("future_iat", Box::new(move |c| c["iat"] = json!(now + 310))),
        (
            "wrong_iss",
            Box::new(|c| c["iss"] = json!("https://evil.example")),
        ),
        ("wrong_aud", Box::new(|c| c["aud"] = json!("other-client"))),
        (
            "multi_aud_wrong_azp",
            Box::new(|c| {
                c["aud"] = json!(["other-client", CLIENT_ID]);
                c["azp"] = json!("other-client");
            }),
        ),
        (
            "wrong_nonce",
            Box::new(|c| c["nonce"] = json!("wrong-nonce")),
        ),
        (
            "wrong_message_type",
            Box::new(|c| {
                c["https://purl.imsglobal.org/spec/lti/claim/message_type"] =
                    json!("LtiDeepLinkingRequest");
            }),
        ),
        (
            "wrong_version",
            Box::new(|c| c["https://purl.imsglobal.org/spec/lti/claim/version"] = json!("1.1.0")),
        ),
        (
            "wrong_deployment_id",
            Box::new(|c| {
                c["https://purl.imsglobal.org/spec/lti/claim/deployment_id"] = json!("wrong-dep");
            }),
        ),
        (
            "missing_target_link_uri",
            Box::new(|c| {
                c.as_object_mut()
                    .expect("obj")
                    .remove("https://purl.imsglobal.org/spec/lti/claim/target_link_uri");
            }),
        ),
        (
            "missing_roles",
            Box::new(|c| {
                c.as_object_mut()
                    .expect("obj")
                    .remove("https://purl.imsglobal.org/spec/lti/claim/roles");
            }),
        ),
    ];

    for (label, mutate) in mutators.drain(..) {
        let (state, nonce, cookie) = oidc_login(&server, &platform, "https://tool.example");
        let mut c = base_claims(&nonce);
        mutate(&mut c);
        let tok = platform.sign_id_token(&c).expect("sign");
        let (st, _, _) = do_launch(&server, &state, &cookie, &tok);
        assert_eq!(st, 400, "mutator {label} must be 400");
    }

    // 4. Missing/mismatched cookie and replayed state
    let (state, nonce, cookie) = oidc_login(&server, &platform, "https://tool.example");
    let tok = platform.sign_id_token(&base_claims(&nonce)).expect("sign");
    let (st_no_cookie, _, _) = do_launch(&server, &state, "cc_lti_state=wrong", &tok);
    assert_eq!(st_no_cookie, 400);
    let (st_ok, _, _) = do_launch(&server, &state, &cookie, &tok);
    assert_eq!(st_ok, 302);
    let (st_replay, _, _) = do_launch(&server, &state, &cookie, &tok);
    assert_eq!(st_replay, 400, "replayed state must be 400");
}

#[test]
fn a4_6_platform_jwks_key_rotation_and_cooldown() {
    let platform = crate::strict_platform::StrictLtiPlatform::start(CLIENT_ID).expect("platform");
    let server = start_server(
        strict_tool_config(&platform, "https://tool.example"),
        temp_course_dir(),
    );

    // Launch 1 with kid-1
    let (state1, nonce1, cookie1) = oidc_login(&server, &platform, "https://tool.example");
    let tok1 = platform
        .sign_id_token(&platform.default_claims(
            &nonce1,
            unix_now(),
            "https://tool.example",
            None,
            &[],
        ))
        .expect("sign");
    assert_eq!(do_launch(&server, &state1, &cookie1, &tok1).0, 302);
    assert_eq!(platform.jwks_requests_count(), 1);

    // Rotate to kid-2 -> refreshes JWKS once and succeeds
    platform.rotate_key("kid-2");
    let (state2, nonce2, cookie2) = oidc_login(&server, &platform, "https://tool.example");
    let tok2 = platform
        .sign_id_token(&platform.default_claims(
            &nonce2,
            unix_now(),
            "https://tool.example",
            None,
            &[],
        ))
        .expect("sign");
    assert_eq!(do_launch(&server, &state2, &cookie2, &tok2).0, 302);
    assert_eq!(platform.jwks_requests_count(), 2);

    // Unknown kid-3 within 60s cooldown -> rejected with 400 without hammering /jwks
    let (state3, nonce3, cookie3) = oidc_login(&server, &platform, "https://tool.example");
    let tok3 = platform
        .sign_id_token_with_kid(
            "kid-3",
            &platform.default_claims(&nonce3, unix_now(), "https://tool.example", None, &[]),
        )
        .expect("sign");
    assert_eq!(do_launch(&server, &state3, &cookie3, &tok3).0, 400);
    assert_eq!(platform.jwks_requests_count(), 2);
}

#[test]
fn a4_7_tool_jwks_client_assertion_and_token_caching() {
    let platform = crate::strict_platform::StrictLtiPlatform::start(CLIENT_ID).expect("platform");
    let server = start_server(
        strict_tool_config(&platform, "https://tool.example"),
        temp_course_dir(),
    );
    platform.set_tool_jwks_url(format!("http://{}/.well-known/jwks.json", server.addr));

    let (jwks_st, _, jwks_body) = request(server.addr, "GET", "/.well-known/jwks.json", &[], "");
    assert_eq!(jwks_st, 200);
    let jwks_val: Value = serde_json::from_slice(&jwks_body).expect("jwks");
    assert_eq!(jwks_val["keys"][0]["kid"], "tool-key");

    let (state, nonce, cookie) = oidc_login(&server, &platform, "https://tool.example");
    let lineitem = platform.lineitem_url("7");
    let claims = platform.default_claims(
        &nonce,
        unix_now(),
        "https://tool.example",
        Some(&lineitem),
        &[SCORE_SCOPE_URI],
    );
    let tok = platform.sign_id_token(&claims).expect("sign");
    let (st, headers, _) = do_launch(&server, &state, &cookie, &tok);
    assert_eq!(st, 302);
    let session_cookie = session_cookie_from_headers(&headers);

    for pct in [70, 95] {
        let body = format!(r#"{{"percent":{pct},"status":"passed"}}"#);
        let (s, _, _) = request(
            server.addr,
            "POST",
            "/lti/score",
            &[
                ("Cookie", &session_cookie),
                ("Content-Type", "application/json"),
                ("Origin", "https://tool.example"),
            ],
            &body,
        );
        assert_eq!(s, 204);
    }

    assert_eq!(platform.token_requests_count(), 1);
    assert!(
        platform.violations().is_empty(),
        "violations: {:?}",
        platform.violations()
    );
}

#[test]
fn a4_8_score_csrf_auth_and_json_validation() {
    let platform = crate::strict_platform::StrictLtiPlatform::start(CLIENT_ID).expect("platform");
    let server = start_server(
        strict_tool_config(&platform, "https://tool.example"),
        temp_course_dir(),
    );
    let lineitem = platform.lineitem_url("8");
    let cookie = server.shared.insert_session(Launch {
        subject: "u1".into(),
        name: None,
        context_id: None,
        resource_link_id: "rl-1".into(),
        roles: vec![],
        ags: Some(Ags {
            lineitem: Some(lineitem),
            lineitems: None,
            scope: vec![SCORE_SCOPE_URI.into()],
        }),
        deployment_id: DEPLOYMENT_ID.into(),
    });
    let session_hdr = format!("cc_lti={cookie}");

    // Wrong Origin -> 403
    assert_eq!(
        request(
            server.addr,
            "POST",
            "/lti/score",
            &[
                ("Cookie", &session_hdr),
                ("Content-Type", "application/json"),
                ("Origin", "https://evil.example"),
            ],
            r#"{"percent":50,"status":"passed"}"#,
        )
        .0,
        403
    );
    // Missing Origin -> 403
    assert_eq!(
        request(
            server.addr,
            "POST",
            "/lti/score",
            &[
                ("Cookie", &session_hdr),
                ("Content-Type", "application/json")
            ],
            r#"{"percent":50,"status":"passed"}"#,
        )
        .0,
        403
    );
    // Content-Type: text/plain -> 403
    assert_eq!(
        request(
            server.addr,
            "POST",
            "/lti/score",
            &[
                ("Cookie", &session_hdr),
                ("Content-Type", "text/plain"),
                ("Origin", "https://tool.example"),
            ],
            r#"{"percent":50,"status":"passed"}"#,
        )
        .0,
        403
    );
    // Missing cookie -> 401
    assert_eq!(
        request(
            server.addr,
            "POST",
            "/lti/score",
            &[
                ("Content-Type", "application/json"),
                ("Origin", "https://tool.example"),
            ],
            r#"{"percent":50,"status":"passed"}"#,
        )
        .0,
        401
    );
    // Unknown cookie -> 401
    assert_eq!(
        request(
            server.addr,
            "POST",
            "/lti/score",
            &[
                ("Cookie", "cc_lti=unknown-session"),
                ("Content-Type", "application/json"),
                ("Origin", "https://tool.example"),
            ],
            r#"{"percent":50,"status":"passed"}"#,
        )
        .0,
        401
    );
    // Extra unknown JSON field -> 400
    assert_eq!(
        request(
            server.addr,
            "POST",
            "/lti/score",
            &[
                ("Cookie", &session_hdr),
                ("Content-Type", "application/json"),
                ("Origin", "https://tool.example"),
            ],
            r#"{"percent":50,"status":"passed","extra":1}"#,
        )
        .0,
        400
    );
    // percent: 101 -> 400
    assert_eq!(
        request(
            server.addr,
            "POST",
            "/lti/score",
            &[
                ("Cookie", &session_hdr),
                ("Content-Type", "application/json"),
                ("Origin", "https://tool.example"),
            ],
            r#"{"percent":101,"status":"passed"}"#,
        )
        .0,
        400
    );

    assert_eq!(platform.token_requests_count(), 0);
    assert!(platform.scores().is_empty());
}

#[test]
fn a4_10_path_prefix_mount_and_static_course_range_etag_traversal() {
    let platform = crate::strict_platform::StrictLtiPlatform::start(CLIENT_ID).expect("platform");
    let dir = temp_course_dir();
    std::fs::write(dir.join("index.html"), b"0123456789abcdef").expect("index.html");
    let server = start_server(
        strict_tool_config(&platform, "https://tool.example/sub/app"),
        dir,
    );

    let (state, nonce, cookie) = oidc_login(
        &server,
        &platform,
        "https://tool.example/sub/app/course/index.html",
    );
    let tok = platform
        .sign_id_token(&platform.default_claims(
            &nonce,
            unix_now(),
            "https://tool.example/sub/app/course/index.html",
            None,
            &[],
        ))
        .expect("sign");
    let (st, headers, _) = do_launch(&server, &state, &cookie, &tok);
    assert_eq!(st, 302);
    assert_eq!(
        headers.get("location").map(String::as_str),
        Some("/sub/app/course/index.html?name=Ada%20Lovelace")
    );

    let (st_get, hdrs_get, body_get) = request(server.addr, "GET", "/course/index.html", &[], "");
    assert_eq!(st_get, 200);
    assert_eq!(body_get, b"0123456789abcdef");
    let etag = hdrs_get.get("etag").expect("etag").clone();

    let (st_304, _, _) = request(
        server.addr,
        "GET",
        "/course/index.html",
        &[("If-None-Match", &etag)],
        "",
    );
    assert_eq!(st_304, 304);

    let (st_206, _, body_206) = request(
        server.addr,
        "GET",
        "/course/index.html",
        &[("Range", "bytes=0-9")],
        "",
    );
    assert_eq!(st_206, 206);
    assert_eq!(body_206, b"0123456789");

    let (st_416, _, _) = request(
        server.addr,
        "GET",
        "/course/index.html",
        &[("Range", "bytes=999-1000")],
        "",
    );
    assert_eq!(st_416, 416);

    let (st_404, _, _) = request(server.addr, "GET", "/course/../secret", &[], "");
    assert_eq!(st_404, 404);
}

#[test]
fn hostile_expires_in_does_not_overflow_the_token_expiry() {
    let now = std::time::Instant::now();
    // Must not panic for any number a platform sends.
    for expires_in in [0, 59, 60, 3600, u64::MAX / 2, u64::MAX] {
        let expiry = crate::server::token_expiry(now, expires_in);
        assert!(expiry >= now);
        assert!(expiry <= now + Duration::from_secs(24 * 60 * 60));
    }
    assert_eq!(
        crate::server::token_expiry(now, 3600),
        now + Duration::from_secs(3540)
    );
}
