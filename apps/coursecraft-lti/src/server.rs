//! Routes, sessions and the accept loop for the LTI tool server.

use std::collections::BTreeMap;
use std::io::Read as _;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use coursecraft_lti::{
    Launch, LtiError, ToolConfig, client_assertion, lineitem_score_url, login_redirect,
    score_payload, token_request_body, tool_jwks, validate_id_token,
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
const TOKEN_REFRESH_MARGIN: Duration = Duration::from_secs(60);
const SESSION_COOKIE: &str = "cc_lti";
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
    token: Option<CachedToken>,
}

/// State shared by every connection thread.
pub struct Shared {
    cfg: ToolConfig,
    course_dir: PathBuf,
    logins: Mutex<BTreeMap<String, PendingLogin>>,
    sessions: Mutex<BTreeMap<String, Session>>,
    jwks: Mutex<Option<(String, Instant)>>,
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
    /// Returns the bind error, such as an address already in use.
    pub fn bind(cfg: ToolConfig, course_dir: PathBuf, listen: &str) -> std::io::Result<Self> {
        let listener = TcpListener::bind(listen)?;
        Ok(Self {
            listener,
            shared: Arc::new(Shared {
                cfg,
                course_dir,
                logins: Mutex::new(BTreeMap::new()),
                sessions: Mutex::new(BTreeMap::new()),
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
        ("GET", path) if is_course_path(path) => course(shared, path),
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
    match tool_jwks(&shared.cfg) {
        Ok(text) => Response::file(text.into_bytes(), "application/json"),
        Err(error) => Response::text(500, error.to_string()),
    }
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
    let now = Instant::now();
    {
        let mut logins = lock(&shared.logins);
        logins.retain(|_, pending| pending.expires > now);
        while logins.len() >= MAX_LOGINS {
            let Some(first) = logins.keys().next().cloned() else {
                break;
            };
            logins.remove(&first);
        }
        logins.insert(
            state.clone(),
            PendingLogin {
                nonce: nonce.clone(),
                expires: now + LOGIN_TTL,
            },
        );
    }
    match login_redirect(&shared.cfg, &login_request, &state, &nonce) {
        Ok(url) => Response::redirect(&url),
        Err(error) => Response::text(400, error.to_string()),
    }
}

fn launch(shared: &Shared, request: &Request) -> Response {
    let form = request.body_text().map(parse_form).unwrap_or_default();
    let (Some(state), Some(id_token)) = (form.get("state"), form.get("id_token")) else {
        return Response::text(400, "state and id_token are required");
    };
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
                token: None,
            },
        );
    }
    let mut response = Response::redirect(&launch_location(&request.path, learner_name.as_deref()));
    response.set_session_cookie(&id);
    response
}

fn launch_location(path: &str, name: Option<&str>) -> String {
    let base = match path.strip_suffix("/lti/launch") {
        Some(prefix) => format!("{prefix}/course/index.html"),
        None => "/course/index.html".into(),
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
        if let Some((jwks, expires)) = cache.as_ref()
            && *expires > now
        {
            return Ok(jwks.clone());
        }
    }
    let jwks = http_get(&shared.cfg.jwks_url)?;
    let mut cache = lock(&shared.jwks);
    *cache = Some((jwks.clone(), now + JWKS_TTL));
    Ok(jwks)
}

fn course(shared: &Shared, path: &str) -> Response {
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
    match std::fs::read(&file) {
        Ok(bytes) => Response::file(bytes, content_type_for(&relative)),
        Err(_) => Response::text(404, "not found"),
    }
}

fn score(shared: &Shared, request: &Request) -> Response {
    let Some(cookie) = request.cookie(SESSION_COOKIE) else {
        return Response::text(401, "no launch session");
    };
    let now = Instant::now();
    let (launch, cached) = {
        let mut sessions = lock(&shared.sessions);
        let Some(session) = sessions.get(cookie) else {
            return Response::text(401, "no launch session");
        };
        if session.created + SESSION_TTL <= now {
            sessions.remove(cookie);
            return Response::text(401, "no launch session");
        }
        let cached = session
            .token
            .as_ref()
            .filter(|token| token.expires > now)
            .map(|token| token.token.clone());
        (session.launch.clone(), cached)
    };
    let Some(lineitem) = launch.ags.as_ref().and_then(|ags| ags.lineitem.clone()) else {
        return Response::text(403, "this launch has no line item");
    };
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
    let token = match cached {
        Some(token) => token,
        None => match fetch_token(shared, cookie, now) {
            Ok(token) => token,
            Err(response) => return response,
        },
    };
    let url = lineitem_score_url(&lineitem);
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

fn fetch_token(shared: &Shared, cookie: &str, now: Instant) -> Result<String, Response> {
    let jti = random_token().map_err(|error| Response::text(500, error.to_string()))?;
    let assertion = client_assertion(&shared.cfg, unix_now(), &jti)
        .map_err(|error| Response::text(500, error.to_string()))?;
    let body = token_request_body(&shared.cfg, &assertion, &[SCORE_SCOPE.to_string()]);
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
    let ttl = expires_in.saturating_sub(TOKEN_REFRESH_MARGIN.as_secs());
    {
        let mut sessions = lock(&shared.sessions);
        if let Some(session) = sessions.get_mut(cookie) {
            session.token = Some(CachedToken {
                token: token.clone(),
                expires: now + Duration::from_secs(ttl),
            });
        }
    }
    Ok(token)
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

    pub(crate) fn insert_session(&self, launch: Launch) -> String {
        let id = random_token().unwrap_or_default();
        let now = Instant::now();
        lock(&self.sessions).insert(
            id.clone(),
            Session {
                launch,
                created: now,
                token: None,
            },
        );
        id
    }
}
