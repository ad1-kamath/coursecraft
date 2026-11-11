//! Launch parameters handed to the player by a cmi5 or xAPI LMS.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::MAX_LAUNCH_QUERY;
use crate::flavor::Flavor;

/// Why launch parameters could not be read.
///
/// Messages name the offending parameter but never repeat its value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchError {
    /// The query exceeds [`MAX_LAUNCH_QUERY`].
    QueryTooLong,
    /// A required parameter is absent.
    Missing(&'static str),
    /// A URL parameter does not start with `http://` or `https://`.
    InvalidUrl(&'static str),
    /// The `actor` parameter is not a JSON object.
    InvalidActor,
}

impl std::fmt::Display for LaunchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::QueryTooLong => write!(f, "launch query is too long"),
            Self::Missing(name) => write!(f, "launch parameter `{name}` is required"),
            Self::InvalidUrl(name) => {
                write!(
                    f,
                    "launch parameter `{name}` must start with http:// or https://"
                )
            }
            Self::InvalidActor => write!(f, "launch parameter `actor` must be a JSON object"),
        }
    }
}

impl std::error::Error for LaunchError {}

/// The launch parameters a cmi5 or xAPI LMS passes in the page URL.
#[derive(Clone, PartialEq)]
pub struct LaunchParams {
    /// LRS endpoint, stored with exactly one trailing `/`.
    pub endpoint: String,
    /// The xAPI actor, always a JSON object.
    pub actor: Value,
    /// Registration id; required for cmi5, optional for xAPI.
    pub registration: Option<String>,
    /// Activity id; required for cmi5, optional for xAPI.
    pub activity_id: Option<String>,
    /// cmi5 fetch URL.
    pub fetch: Option<String>,
    /// Plain xAPI `Authorization` header value.
    pub auth: Option<String>,
}

impl std::fmt::Debug for LaunchParams {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LaunchParams")
            .field("endpoint", &self.endpoint)
            .field("actor", &self.actor)
            .field("registration", &self.registration)
            .field("activity_id", &self.activity_id)
            .field("fetch", &self.fetch.as_ref().map(|_| "<redacted>"))
            .field("auth", &self.auth.as_ref().map(|_| "<redacted>"))
            .finish()
    }
}

impl LaunchParams {
    /// Parse a URL query string with or without its leading `?`.
    ///
    /// The decoder is deliberately forgiving: bad escapes stay literal and
    /// invalid UTF-8 is replaced, so a malformed launch can never panic.
    ///
    /// # Errors
    /// Returns [`LaunchError`] for an oversized query, a missing required
    /// parameter, a non-HTTP URL, or a non-object actor.
    pub fn parse(query: &str, flavor: Flavor) -> Result<Self, LaunchError> {
        let query = query.strip_prefix('?').unwrap_or(query);
        if query.len() > MAX_LAUNCH_QUERY {
            return Err(LaunchError::QueryTooLong);
        }

        let mut params = BTreeMap::new();
        for pair in query.split('&') {
            if pair.is_empty() {
                continue;
            }
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            let key = percent_decode(key);
            if key.is_empty() {
                continue;
            }
            params.insert(key, percent_decode(value));
        }

        let endpoint = required(&params, "endpoint")?;
        let endpoint = normalize_endpoint(&endpoint).ok_or(LaunchError::InvalidUrl("endpoint"))?;

        let actor_text = required(&params, "actor")?;
        let mut actor: Value =
            serde_json::from_str(&actor_text).map_err(|_| LaunchError::InvalidActor)?;
        if !actor.is_object() {
            return Err(LaunchError::InvalidActor);
        }
        keep_first_array_elements(&mut actor);

        let (registration, activity_id, fetch, auth) = match flavor {
            Flavor::Cmi5 => {
                let registration = Some(required(&params, "registration")?);
                let activity_id = Some(required(&params, "activityId")?);
                let fetch = Some(required(&params, "fetch")?);
                if !is_http_url(fetch.as_deref().unwrap_or_default()) {
                    return Err(LaunchError::InvalidUrl("fetch"));
                }
                (registration, activity_id, fetch, None)
            }
            Flavor::Xapi => (
                params.get("registration").cloned(),
                params.get("activity_id").cloned(),
                None,
                Some(required(&params, "auth")?),
            ),
        };

        Ok(Self {
            endpoint,
            actor,
            registration,
            activity_id,
            fetch,
            auth,
        })
    }
}

fn required(params: &BTreeMap<String, String>, name: &'static str) -> Result<String, LaunchError> {
    params
        .get(name)
        .filter(|value| !value.is_empty())
        .cloned()
        .ok_or(LaunchError::Missing(name))
}

fn is_http_url(value: &str) -> bool {
    value.starts_with("http://") || value.starts_with("https://")
}

fn normalize_endpoint(value: &str) -> Option<String> {
    if value == "http://" || value == "https://" {
        return Some(value.to_owned());
    }
    let trimmed = value.trim_end_matches('/');
    if !is_http_url(trimmed) {
        return None;
    }
    Some(format!("{trimmed}/"))
}

/// Old Tin Can launches sometimes send actor properties as one-element arrays.
/// Keep the first element so the statement actor is still a plain object.
fn keep_first_array_elements(actor: &mut Value) {
    if let Value::Object(map) = actor {
        map.retain(|_, value| !matches!(value, Value::Array(items) if items.is_empty()));
        for value in map.values_mut() {
            if let Value::Array(items) = value {
                *value = items.first().cloned().unwrap_or(Value::Null);
            }
        }
    }
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
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
