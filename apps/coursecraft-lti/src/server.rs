//! Routes, sessions and the accept loop for the LTI tool server.

use std::collections::BTreeMap;
use std::io::{Read as _, Seek as _, SeekFrom};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use coursecraft_lti::{
    Launch, LtiError, ToolConfig, ToolSigningKey, client_assertion_with_key, lineitem_score_url,
    login_redirect, score_payload, token_request_body, tool_jwks_with_key, validate_id_token,
};
use ring::rand::{SecureRandom, SystemRandom};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::http::{Request, Response, content_type_for, parse_form, percent_decode, read_request};

const MAX_CONNECTIONS: usize = 64;
const READ_TIMEOUT: Duration = Duration::from_secs(5);
const WRITE_TIMEOUT: Duration = Duration::from_secs(30);
const NETWORK_TIMEOUT: Duration = Duration::from_secs(5);
const LOGIN_TTL: Duration = Duration::from_secs(10 * 60);
const MAX_LOGINS: usize = 10_000;
const SESSION_TTL: Duration = Duration::from_secs(24 * 60 * 60);
const MAX_SESSIONS: usize = 10_000;
const JWKS_TTL: Duration = Duration::from_secs(60 * 60);
const JWKS_REFRESH_COOLDOWN: Duration = Duration::from_secs(60);
const TOKEN_REFRESH_MARGIN: Duration = Duration::from_secs(60);
const SESSION_COOKIE: &str = "cc_lti";
const STATE_COOKIE: &str = "cc_lti_state";
const SCORE_SCOPE: &str = "https://purl.imsglobal.org/spec/lti-ags/scope/score";
const SCORE_CONTENT_TYPE: &str = "application/vnd.ims.lis.v1.score+json";
const MAX_PLATFORM_BODY: u64 = 2 * 1024 * 1024;

struct PendingLogin {
    nonce: String,
    expires: Instant,
}

struct CachedToken {
    token: String,
    expires: Instant,
}

struct Session {
    launch: Launch,
    created: Instant,
}

struct JwksCache {
    jwks: String,
    expires: Instant,
    last_kid_refresh: Option<Instant>,
}

/// State shared by every connection thread.
pub struct Shared {
    cfg: ToolConfig,
    signing_key: ToolSigningKey,
    jwks_json: String,
    course_dir: PathBuf,
    logins: Mutex<BTreeMap<String, PendingLogin>>,
    sessions: Mutex<BTreeMap<String, Session>>,
    tokens: Mutex<BTreeMap<Vec<String>, CachedToken>>,
    jwks: Mutex<Option<JwksCache>>,
}

/// A bound LTI server. Call [`Server::run`] to accept connections.
pub struct Server {
    listener: TcpListener,
    shared: Arc<Shared>,
}

impl Server {
    /// Bind `listen` and prepare the shared state.
    ///
    /// # Errors
    /// Returns the bind error, such as an address already in use, or an invalid
    /// signing key.
    pub fn bind(cfg: ToolConfig, course_dir: PathBuf, listen: &str) -> std::io::Result<Self> {
        let signing_key = ToolSigningKey::from_pkcs8(&cfg.private_key_pkcs8)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e.to_string()))?;
        let jwks_json = tool_jwks_with_key(&signing_key, &cfg.key_id)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e.to_string()))?;
        let listener = TcpListener::bind(listen)?;
        Ok(Self {
            listener,
            shared: Arc::new(Shared {
                cfg,
                signing_key,
                jwks_json,
                course_dir,
                logins: Mutex::new(BTreeMap::new()),
                sessions: Mutex::new(BTreeMap::new()),
                tokens: Mutex::new(BTreeMap::new()),
                jwks: Mutex::new(None),
            }),
        })
    }

    /// The address actually bound, useful when `listen` used port 0.
    ///
    /// # Errors
    /// Returns the underlying `local_addr` error.
    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    /// A handle to the state behind the server, for tests.
    #[must_use]
    pub fn shared(&self) -> Arc<Shared> {
        Arc::clone(&self.shared)
    }

    /// Accept connections forever, one thread per connection.
    ///
    /// # Errors
    /// Returns only an unrecoverable listener error; individual connection
    /// errors are ignored.
    pub fn run(self) -> std::io::Result<()> {
        let active = Arc::new(AtomicUsize::new(0));
        for connection in self.listener.incoming() {
            let mut stream = match connection {
                Ok(stream) => stream,
                Err(_) => continue,
            };
            if active.load(Ordering::SeqCst) >= MAX_CONNECTIONS {
                let _ = stream.set_write_timeout(Some(WRITE_TIMEOUT));
                let _ = Response::status(503).write(&mut stream);
                continue;
            }
            active.fetch_add(1, Ordering::SeqCst);
            let shared = Arc::clone(&self.shared);
            let finished = Arc::clone(&active);
            let spawned = std::thread::Builder::new()
                .name("coursecraft-lti".into())
                .spawn(move || {
                    handle_connection(&mut stream, &shared);
                    finished.fetch_sub(1, Ordering::SeqCst);
                });
            if spawned.is_err() {
                active.fetch_sub(1, Ordering::SeqCst);
            }
        }
        Ok(())
    }
}

fn handle_connection(stream: &mut TcpStream, shared: &Shared) {
    let _ = stream.set_read_timeout(Some(READ_TIMEOUT));
    let _ = stream.set_write_timeout(Some(WRITE_TIMEOUT));
    let _ = stream.set_nodelay(true);

    // Last line of defence: a bug in one connection must not take the server
    // down. The handler itself is written to be panic-free.
    let outcome = catch_unwind(AssertUnwindSafe(|| match read_request(stream) {
        Ok(request) => {
            let response = handle(shared, &request);
            (request.method, request.path, response)
        }
        Err(error) => ("-".into(), "-".into(), read_error_response(error)),
    }));
    let (method, path, response) = match outcome {
        Ok(outcome) => outcome,
        Err(_) => (
            "-".into(),
            "-".into(),
            Response::text(500, "internal server error"),
        ),
    };
    // One line per request, deliberately without the body.
    eprintln!("{method} {path} {}", response.status);
    let _ = response.write(stream);
}

fn read_error_response(error: crate::http::ReadError) -> Response {
    match error {
        crate::http::ReadError::TooLarge => Response::text(413, "request is too large"),
        crate::http::ReadError::BadRequest(message) => Response::text(400, message),
        crate::http::ReadError::Io(error) => Response::text(400, error.to_string()),
    }
}

fn handle(shared: &Shared, request: &Request) -> Response {
    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/.well-known/jwks.json") => jwks_response(shared),
        ("GET" | "POST", "/lti/login") => login(shared, request),
        ("POST", "/lti/launch") => launch(shared, request),
        ("POST", "/lti/score") => score(shared, request),
        ("GET", "/lti/health") => Response::text(200, "ok"),
        ("GET", path) if is_course_path(path) => course(shared, request, path),
        (_, path) if path == "/course" || path.starts_with("/course/") => {
            Response::text(405, "method not allowed")
        }
        (
            _,
            "/.well-known/jwks.json" | "/lti/login" | "/lti/launch" | "/lti/score" | "/lti/health",
        ) => Response::text(405, "method not allowed"),
        _ => Response::text(404, "not found"),
    }
}

fn is_course_path(path: &str) -> bool {
    path == "/course" || path.starts_with("/course/")
}

fn jwks_response(shared: &Shared) -> Response {
    Response::file(shared.jwks_json.as_bytes().to_vec(), "application/json")
}

fn login(shared: &Shared, request: &Request) -> Response {
    let mut params = request.query.clone();
    if request.method == "POST"
        && let Some(text) = request.body_text()
    {
        for (key, value) in parse_form(text) {
            params.insert(key, value);
        }
    }
    let (Some(iss), Some(login_hint), Some(target_link_uri)) = (
        params.get("iss"),
        params.get("login_hint"),
        params.get("target_link_uri"),
    ) else {
        return Response::text(400, "iss, login_hint and target_link_uri are required");
    };
    let login_request = coursecraft_lti::LoginRequest {
        iss: iss.clone(),
        login_hint: login_hint.clone(),
        target_link_uri: target_link_uri.clone(),
        lti_message_hint: params.get("lti_message_hint").cloned(),
        client_id: params.get("client_id").cloned(),
    };
    let state = match random_token() {
        Ok(token) => token,
        Err(error) => return Response::text(500, error.to_string()),
    };
    let nonce = match random_token() {
        Ok(token) => token,
        Err(error) => return Response::text(500, error.to_string()),
    };
    let url = match login_redirect(&shared.cfg, &login_request, &state, &nonce) {
        Ok(url) => url,
        Err(error) => return Response::text(400, error.to_string()),
    };
    let now = Instant::now();
    {
        let mut logins = lock(&shared.logins);
        logins.retain(|_, pending| pending.expires > now);
        while logins.len() >= MAX_LOGINS {
            let Some(oldest) = logins
                .iter()
                .min_by_key(|(_, pending)| pending.expires)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            logins.remove(&oldest);
        }
        logins.insert(
            state.clone(),
            PendingLogin {
                nonce,
                expires: now + LOGIN_TTL,
            },
        );
    }
    let mut response = Response::redirect(&url);
    response.set_state_cookie(&state_cookie_value(&state));
    response
}

fn launch(shared: &Shared, request: &Request) -> Response {
    let form = request.body_text().map(parse_form).unwrap_or_default();
    let (Some(state), Some(id_token)) = (form.get("state"), form.get("id_token")) else {
        return Response::text(400, "state and id_token are required");
    };
    let expected_cookie = state_cookie_value(state);
    let Some(cookie) = request.cookie(STATE_COOKIE) else {
        return Response::text(400, "missing state cookie");
    };
    if !constant_time_eq(cookie.as_bytes(), expected_cookie.as_bytes()) {
        return Response::text(400, "invalid state cookie");
    }
    let now = Instant::now();
    let pending = {
        let mut logins = lock(&shared.logins);
        logins.remove(state)
    };
    let Some(pending) = pending else {
        return Response::text(400, "unknown or expired state");
    };
    if pending.expires <= now {
        return Response::text(400, "unknown or expired state");
    }
    let platform_jwks = match platform_jwks(shared) {
        Ok(jwks) => jwks,
        Err(message) => return Response::text(502, message),
    };
    let now_unix = unix_now();
    let launch = match validate_id_token(
        &shared.cfg,
        id_token,
        &platform_jwks,
        &pending.nonce,
        now_unix,
    ) {
        Ok(launch) => launch,
        Err(LtiError::UnknownKeyId(kid)) => match refresh_platform_jwks(shared) {
            Ok(Some(fresh_jwks)) => match validate_id_token(
                &shared.cfg,
                id_token,
                &fresh_jwks,
                &pending.nonce,
                now_unix,
            ) {
                Ok(launch) => launch,
                Err(error) => return Response::text(400, error.to_string()),
            },
            Ok(None) => return Response::text(400, LtiError::UnknownKeyId(kid).to_string()),
            Err(message) => return Response::text(502, message),
        },
        Err(error) => return Response::text(400, error.to_string()),
    };
    let id = match random_token() {
        Ok(id) => id,
        Err(error) => return Response::text(500, error.to_string()),
    };
    let learner_name = launch.name.clone();
    {
        let mut sessions = lock(&shared.sessions);
        sessions.retain(|_, session| session.created + SESSION_TTL > now);
        while sessions.len() >= MAX_SESSIONS {
            let Some(first) = sessions.keys().next().cloned() else {
                break;
            };
            sessions.remove(&first);
        }
        sessions.insert(
            id.clone(),
            Session {
                launch,
                created: now,
            },
        );
    }
    let mut response = Response::redirect(&launch_location(
        &shared.cfg.tool_url,
        learner_name.as_deref(),
    ));
    response.set_session_cookie(&id);
    response
}

fn state_cookie_value(state: &str) -> String {
    let digest = ring::digest::digest(&ring::digest::SHA256, state.as_bytes());
    URL_SAFE_NO_PAD.encode(digest.as_ref())
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn launch_location(tool_url: &str, name: Option<&str>) -> String {
    let after_scheme = tool_url
        .split_once("://")
        .map_or(tool_url, |(_, rest)| rest);
    let raw_path = after_scheme.split_once('/').map_or("", |(_, path)| path);
    let path = raw_path.split(['?', '#']).next().unwrap_or("");
    let prefix = path.trim_matches('/');
    let base = if prefix.is_empty() {
        "/course/index.html".to_string()
    } else {
        format!("/{prefix}/course/index.html")
    };
    match name.filter(|name| !name.trim().is_empty()) {
        Some(name) => format!("{base}?name={}", encode_query(name)),
        None => base,
    }
}

fn encode_query(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
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

fn platform_jwks(shared: &Shared) -> Result<String, String> {
    let now = Instant::now();
    {
        let cache = lock(&shared.jwks);
        if let Some(entry) = cache.as_ref()
            && entry.expires > now
        {
            return Ok(entry.jwks.clone());
        }
    }
    let jwks = http_get(&shared.cfg.jwks_url)?;
    let now = Instant::now();
    let mut cache = lock(&shared.jwks);
    *cache = Some(JwksCache {
        jwks: jwks.clone(),
        expires: now + JWKS_TTL,
        last_kid_refresh: None,
    });
    Ok(jwks)
}

fn refresh_platform_jwks(shared: &Shared) -> Result<Option<String>, String> {
    let now = Instant::now();
    {
        let cache = lock(&shared.jwks);
        if let Some(entry) = cache.as_ref()
            && entry
                .last_kid_refresh
                .is_some_and(|last| last + JWKS_REFRESH_COOLDOWN > now)
        {
            return Ok(None);
        }
    }
    let jwks = http_get(&shared.cfg.jwks_url)?;
    let now = Instant::now();
    let mut cache = lock(&shared.jwks);
    *cache = Some(JwksCache {
        jwks: jwks.clone(),
        expires: now + JWKS_TTL,
        last_kid_refresh: Some(now),
    });
    Ok(Some(jwks))
}

const COURSE_CACHE_CONTROL: &str = "public, max-age=3600";

fn course(shared: &Shared, request: &Request, path: &str) -> Response {
    let relative = path.strip_prefix("/course/").unwrap_or_default();
    let relative = if relative.is_empty() {
        "index.html"
    } else {
        relative
    };
    let Some(relative) = percent_decode(relative) else {
        return Response::text(404, "not found");
    };
    if !is_safe_relative_path(&relative) {
        return Response::text(404, "not found");
    }
    let file = shared.course_dir.join(&relative);
    let Ok(meta) = std::fs::metadata(&file) else {
        return Response::text(404, "not found");
    };
    if !meta.is_file() {
        return Response::text(404, "not found");
    }
    let size = meta.len();
    let mtime_nanos = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos());
    let etag = format!("\"{size:x}-{mtime_nanos:x}\"");

    if let Some(inm) = request.headers.get("if-none-match")
        && etag_matches(inm, &etag)
    {
        let mut response = Response::status(304);
        response.headers.push(("ETag".into(), etag));
        response
            .headers
            .push(("Cache-Control".into(), COURSE_CACHE_CONTROL.into()));
        return response;
    }

    let content_type = content_type_for(&relative);

    if let Some(range_header) = request.headers.get("range")
        && !range_header.contains(',')
        && let Some(parsed) = parse_byte_range(range_header, size)
    {
        let (start, end) = match parsed {
            Ok(bounds) => bounds,
            Err(()) => {
                let mut response = Response::status(416);
                response
                    .headers
                    .push(("Content-Range".into(), format!("bytes */{size}")));
                return response;
            }
        };
        let len = end.saturating_sub(start).saturating_add(1);
        let Ok(mut opened) = std::fs::File::open(&file) else {
            return Response::text(404, "not found");
        };
        if opened.seek(SeekFrom::Start(start)).is_err() {
            return Response::text(500, "failed to seek course file");
        }
        let mut buf = Vec::with_capacity(usize::try_from(len).unwrap_or(0).min(1024 * 1024));
        if opened.take(len).read_to_end(&mut buf).is_err() {
            return Response::text(500, "failed to read course file");
        }
        let mut response = Response::file(buf, content_type);
        response.status = 206;
        response.headers.push((
            "Content-Range".into(),
            format!("bytes {start}-{end}/{size}"),
        ));
        response
            .headers
            .push(("Accept-Ranges".into(), "bytes".into()));
        response.headers.push(("ETag".into(), etag));
        response
            .headers
            .push(("Cache-Control".into(), COURSE_CACHE_CONTROL.into()));
        return response;
    }

    match std::fs::read(&file) {
        Ok(bytes) => {
            let mut response = Response::file(bytes, content_type);
            response
                .headers
                .push(("Accept-Ranges".into(), "bytes".into()));
            response.headers.push(("ETag".into(), etag));
            response
                .headers
                .push(("Cache-Control".into(), COURSE_CACHE_CONTROL.into()));
            response
        }
        Err(_) => Response::text(404, "not found"),
    }
}

fn etag_matches(header: &str, etag: &str) -> bool {
    let trimmed = header.trim();
    if trimmed == "*" {
        return true;
    }
    trimmed.split(',').any(|candidate| candidate.trim() == etag)
}

/// Parse a single `Range: bytes=...` header. Returns `None` if the header is
/// not a `bytes=` range, `Err(())` if unsatisfiable, or `Ok((start, end))` inclusive.
fn parse_byte_range(header: &str, size: u64) -> Option<Result<(u64, u64), ()>> {
    let spec = header.trim().strip_prefix("bytes=")?;
    let (start_str, end_str) = spec.split_once('-')?;
    let start_str = start_str.trim();
    let end_str = end_str.trim();
    if size == 0 {
        return Some(Err(()));
    }
    if start_str.is_empty() {
        let suffix: u64 = end_str.parse().ok()?;
        if suffix == 0 {
            return Some(Err(()));
        }
        let start = size.saturating_sub(suffix);
        return Some(Ok((start, size - 1)));
    }
    let start: u64 = start_str.parse().ok()?;
    if start >= size {
        return Some(Err(()));
    }
    let end = if end_str.is_empty() {
        size - 1
    } else {
        let end: u64 = end_str.parse().ok()?;
        if end < start {
            return Some(Err(()));
        }
        end.min(size - 1)
    };
    Some(Ok((start, end)))
}

fn is_json_content_type(request: &Request) -> bool {
    let Some(raw) = request.headers.get("content-type") else {
        return false;
    };
    let mut parts = raw.split(';');
    let Some(media_type) = parts.next().map(str::trim) else {
        return false;
    };
    if !media_type.eq_ignore_ascii_case("application/json") {
        return false;
    }
    parts.all(|param| {
        let trimmed = param.trim();
        trimmed.is_empty()
            || trimmed
                .get(..8)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("charset="))
    })
}

fn url_origin(url: &str) -> Option<&str> {
    let (scheme, rest) = url.split_once("://")?;
    if scheme.is_empty()
        || !scheme
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.'))
    {
        return None;
    }
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    if authority_end == 0 {
        return None;
    }
    url.get(..scheme.len() + 3 + authority_end)
}

fn score(shared: &Shared, request: &Request) -> Response {
    if !is_json_content_type(request) {
        return Response::text(403, "forbidden: content-type must be application/json");
    }
    let Some(expected_origin) = url_origin(&shared.cfg.tool_url) else {
        return Response::text(403, "forbidden: invalid tool origin");
    };
    let Some(origin) = request.headers.get("origin").map(String::as_str) else {
        return Response::text(403, "forbidden: origin required");
    };
    if !origin.eq_ignore_ascii_case(expected_origin) {
        return Response::text(403, "forbidden: cross-site origin");
    }
    let Some(cookie) = request.cookie(SESSION_COOKIE) else {
        return Response::text(401, "no launch session");
    };
    let now = Instant::now();
    let launch = {
        let mut sessions = lock(&shared.sessions);
        let Some(session) = sessions.get(cookie) else {
            return Response::text(401, "no launch session");
        };
        if session.created + SESSION_TTL <= now {
            sessions.remove(cookie);
            return Response::text(401, "no launch session");
        }
        session.launch.clone()
    };
    let Some(ags) = launch.ags.as_ref() else {
        return Response::text(403, "this launch has no line item");
    };
    let Some(lineitem) = ags.lineitem.as_ref() else {
        return Response::text(403, "this launch has no line item");
    };
    if !ags.scope.iter().any(|scope| scope == SCORE_SCOPE) {
        return Response::text(403, "this launch does not have AGS score scope");
    }
    let Some(text) = request.body_text() else {
        return Response::text(400, "score must be JSON");
    };
    let score_request: ScoreRequest = match serde_json::from_str(text) {
        Ok(score) => score,
        Err(error) => return Response::text(400, format!("score: {error}")),
    };
    let score_body = match score_body(&launch, score_request.percent, &score_request.status) {
        Ok(body) => body,
        Err(error) => return Response::text(400, error.to_string()),
    };
    let scopes = [SCORE_SCOPE.to_string()];
    let token = match ags_token(shared, &scopes, now) {
        Ok(token) => token,
        Err(response) => return response,
    };
    let url = lineitem_score_url(lineitem);
    let sent = http_post(
        &url,
        SCORE_CONTENT_TYPE,
        score_body.as_bytes(),
        Some(&token),
    );
    let (status, body) = match sent {
        Ok(response) => response,
        Err(error) => return Response::text(502, error),
    };
    if (200..300).contains(&status) {
        Response::status(204)
    } else {
        Response::text(
            502,
            format!("platform returned {status}: {}", body_text(&body)),
        )
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScoreRequest {
    #[serde(default)]
    percent: Option<u8>,
    status: String,
}

fn score_body(launch: &Launch, percent: Option<u8>, status: &str) -> Result<String, LtiError> {
    let timestamp = iso_timestamp();
    match status {
        "passed" | "failed" | "completed" => score_payload(launch, percent, status, &timestamp),
        "incomplete" => {
            let base = score_payload(launch, percent, "completed", &timestamp)?;
            let mut value: Value =
                serde_json::from_str(&base).map_err(|error| LtiError::Json(error.to_string()))?;
            value["activityProgress"] = json!("InProgress");
            value["gradingProgress"] = json!("Pending");
            value["comment"] = json!("incomplete");
            serde_json::to_string(&value).map_err(|error| LtiError::Json(error.to_string()))
        }
        _ => Err(LtiError::BadRequest(format!(
            "unsupported status {status:?}"
        ))),
    }
}

fn ags_token(shared: &Shared, scopes: &[String], now: Instant) -> Result<String, Response> {
    let mut scope_key = scopes.to_vec();
    scope_key.sort();
    scope_key.dedup();
    {
        let tokens = lock(&shared.tokens);
        if let Some(entry) = tokens.get(&scope_key)
            && entry.expires > now
        {
            return Ok(entry.token.clone());
        }
    }
    let jti = random_token().map_err(|error| Response::text(500, error.to_string()))?;
    let assertion = client_assertion_with_key(&shared.cfg, &shared.signing_key, unix_now(), &jti)
        .map_err(|error| Response::text(500, error.to_string()))?;
    let body = token_request_body(&shared.cfg, &assertion, &scope_key);
    let (status, bytes) = http_post(
        &shared.cfg.auth_token_url,
        "application/x-www-form-urlencoded",
        body.as_bytes(),
        None,
    )
    .map_err(|error| Response::text(502, error))?;
    if !(200..300).contains(&status) {
        return Err(Response::text(
            502,
            format!("token endpoint returned {status}: {}", body_text(&bytes)),
        ));
    }
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|_| Response::text(502, "token endpoint did not return JSON".to_string()))?;
    let Some(token) = value.get("access_token").and_then(Value::as_str) else {
        return Err(Response::text(
            502,
            "token endpoint returned no access_token",
        ));
    };
    let token = token.to_string();
    let expires_in = value
        .get("expires_in")
        .and_then(Value::as_u64)
        .unwrap_or(3600);
    let expires = token_expiry(now, expires_in);
    {
        let mut tokens = lock(&shared.tokens);
        tokens.insert(
            scope_key,
            CachedToken {
                token: token.clone(),
                expires,
            },
        );
    }
    Ok(token)
}

/// Longest a cached platform token is trusted, whatever `expires_in` says.
const TOKEN_MAX_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// When to stop using a token the platform said lives `expires_in` seconds.
/// The platform controls that number, so it is clamped and the sum is checked
/// instead of trusting `Instant + Duration` not to overflow.
pub(crate) fn token_expiry(now: Instant, expires_in: u64) -> Instant {
    let ttl = Duration::from_secs(expires_in.saturating_sub(TOKEN_REFRESH_MARGIN.as_secs()))
        .min(TOKEN_MAX_TTL);
    now.checked_add(ttl).unwrap_or(now)
}

fn http_get(url: &str) -> Result<String, String> {
    let mut response = ureq::get(url)
        .config()
        .timeout_global(Some(NETWORK_TIMEOUT))
        .http_status_as_error(false)
        .build()
        .call()
        .map_err(|error| error.to_string())?;
    let status = response.status().as_u16();
    let bytes = read_platform_body(&mut response)?;
    if !(200..300).contains(&status) {
        return Err(format!("platform returned {status}: {}", body_text(&bytes)));
    }
    String::from_utf8(bytes).map_err(|_| "platform response is not UTF-8".to_string())
}

fn http_post(
    url: &str,
    content_type: &str,
    body: &[u8],
    bearer: Option<&str>,
) -> Result<(u16, Vec<u8>), String> {
    let mut request = ureq::post(url).header("Content-Type", content_type);
    if let Some(token) = bearer {
        request = request.header("Authorization", &format!("Bearer {token}"));
    }
    let mut response = request
        .config()
        .timeout_global(Some(NETWORK_TIMEOUT))
        .http_status_as_error(false)
        .build()
        .send(body)
        .map_err(|error| error.to_string())?;
    let status = response.status().as_u16();
    let bytes = read_platform_body(&mut response)?;
    Ok((status, bytes))
}

fn read_platform_body(response: &mut ureq::http::Response<ureq::Body>) -> Result<Vec<u8>, String> {
    let mut reader = response.body_mut().as_reader().take(MAX_PLATFORM_BODY);
    let mut bytes = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    Ok(bytes)
}

fn body_text(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    text.chars().take(500).collect()
}

fn random_token() -> Result<String, LtiError> {
    let mut bytes = [0u8; 32];
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| LtiError::Key("random generation failed".into()))?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

fn unix_now() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => i64::try_from(duration.as_secs()).unwrap_or(i64::MAX),
        Err(_) => 0,
    }
}

fn iso_timestamp() -> String {
    iso_from_unix(unix_now())
}

fn iso_from_unix(seconds: i64) -> String {
    let days = seconds.div_euclid(86_400);
    let remainder = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        remainder / 3600,
        (remainder % 3600) / 60,
        remainder % 60
    )
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days.saturating_add(719_468);
    let era = if z >= 0 { z } else { z.saturating_sub(146_096) } / 146_097;
    let day_of_era = z.saturating_sub(era.saturating_mul(146_097));
    let year_of_era = (day_of_era
        .saturating_sub(day_of_era / 1460)
        .saturating_add(day_of_era / 36_524)
        .saturating_sub(day_of_era / 146_096))
        / 365;
    let year = year_of_era.saturating_add(era.saturating_mul(400));
    let day_of_year = day_of_era.saturating_sub(
        year_of_era
            .saturating_mul(365)
            .saturating_add(year_of_era / 4)
            .saturating_sub(year_of_era / 100),
    );
    let month_prime = (day_of_year.saturating_mul(5).saturating_add(2)) / 153;
    let day = day_of_year
        .saturating_sub((month_prime.saturating_mul(153).saturating_add(2)) / 5)
        .saturating_add(1);
    let month = if month_prime < 10 {
        month_prime.saturating_add(3)
    } else {
        month_prime.saturating_sub(9)
    };
    (
        if month <= 2 {
            year.saturating_add(1)
        } else {
            year
        },
        u32::try_from(month).unwrap_or(1),
        u32::try_from(day).unwrap_or(1),
    )
}

/// The same rule package paths use: no absolute paths, drive letters, URL
/// schemes, backslashes, control characters, or `.`/`..` segments.
fn is_safe_relative_path(path: &str) -> bool {
    if path.is_empty() || path.starts_with('/') || path.contains('\\') || path.contains(':') {
        return false;
    }
    if path.chars().any(char::is_control) {
        return false;
    }
    for segment in path.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." {
            return false;
        }
    }
    true
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
impl Shared {
    pub(crate) fn pending_logins(&self) -> usize {
        lock(&self.logins).len()
    }

    pub(crate) fn insert_pending_login(&self, state: &str, expires: Instant) {
        lock(&self.logins).insert(
            state.into(),
            PendingLogin {
                nonce: "nonce".into(),
                expires,
            },
        );
    }

    pub(crate) fn has_pending_login(&self, state: &str) -> bool {
        lock(&self.logins).contains_key(state)
    }

    pub(crate) fn insert_session(&self, launch: Launch) -> String {
        let id = random_token().unwrap_or_default();
        let now = Instant::now();
        lock(&self.sessions).insert(
            id.clone(),
            Session {
                launch,
                created: now,
            },
        );
        id
    }
}
