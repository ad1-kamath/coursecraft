//! LTI 1.3 for CourseCraft, as pure functions (L4, standalone).
//!
//! Implements the tool side of an LTI 1.3 launch without network, clock or
//! I/O: the OIDC login redirect, `id_token` validation against the platform's
//! JWKS with RS256 through `ring`, the tool's own JWKS, the OAuth
//! `client_credentials` assertion, and the Assignment and Grade Services score
//! payload. The server that uses these functions lives in `apps/coursecraft-lti`.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod jwt;

use ring::rsa::PublicKeyComponents;
use ring::signature::RsaKeyPair;
use serde_json::{Value, json};

const MESSAGE_TYPE: &str = "https://purl.imsglobal.org/spec/lti/claim/message_type";
const VERSION: &str = "https://purl.imsglobal.org/spec/lti/claim/version";
const DEPLOYMENT_ID: &str = "https://purl.imsglobal.org/spec/lti/claim/deployment_id";
const CONTEXT: &str = "https://purl.imsglobal.org/spec/lti/claim/context";
const RESOURCE_LINK: &str = "https://purl.imsglobal.org/spec/lti/claim/resource_link";
const ROLES: &str = "https://purl.imsglobal.org/spec/lti/claim/roles";
const AGS: &str = "https://purl.imsglobal.org/spec/lti-ags/claim/endpoint";

/// Platform registration and tool key material.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolConfig {
    pub issuer: String,
    pub client_id: String,
    pub deployment_id: String,
    pub auth_login_url: String,
    pub auth_token_url: String,
    pub jwks_url: String,
    /// Public HTTPS origin plus path prefix, without a trailing slash.
    pub tool_url: String,
    pub private_key_pkcs8: Vec<u8>,
    pub key_id: String,
}

/// The platform's OIDC third-party login request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoginRequest {
    pub iss: String,
    pub login_hint: String,
    pub target_link_uri: String,
    pub lti_message_hint: Option<String>,
    pub client_id: Option<String>,
}

/// The validated result of a resource-link launch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Launch {
    pub subject: String,
    pub name: Option<String>,
    pub context_id: Option<String>,
    pub resource_link_id: String,
    pub roles: Vec<String>,
    pub ags: Option<Ags>,
    pub deployment_id: String,
}

/// Assignment and Grade Services endpoints and granted scopes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ags {
    pub lineitem: Option<String>,
    pub lineitems: Option<String>,
    pub scope: Vec<String>,
}

/// Every protocol failure, never a panic.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum LtiError {
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("invalid token: {0}")]
    InvalidToken(String),
    #[error("key: {0}")]
    Key(String),
    #[error("json: {0}")]
    Json(String),
}

/// Build the platform login URL for an OIDC third-party initiation.
pub fn login_redirect(
    cfg: &ToolConfig,
    req: &LoginRequest,
    state: &str,
    nonce: &str,
) -> Result<String, LtiError> {
    if req.iss != cfg.issuer {
        return Err(LtiError::BadRequest(
            "login request issuer does not match the tool".into(),
        ));
    }
    if let Some(client_id) = req.client_id.as_deref()
        && client_id != cfg.client_id
    {
        return Err(LtiError::BadRequest(
            "login request client_id does not match the tool".into(),
        ));
    }
    let redirect_uri = format!("{}/lti/launch", cfg.tool_url.trim_end_matches('/'));
    let mut pairs: Vec<(&str, String)> = vec![
        ("scope", "openid".into()),
        ("response_type", "id_token".into()),
        ("client_id", cfg.client_id.clone()),
        ("redirect_uri", redirect_uri),
        ("login_hint", req.login_hint.clone()),
        ("state", state.into()),
        ("response_mode", "form_post".into()),
        ("nonce", nonce.into()),
        ("prompt", "none".into()),
    ];
    if let Some(hint) = req.lti_message_hint.as_deref() {
        pairs.push(("lti_message_hint", hint.into()));
    }
    let query = pairs
        .iter()
        .map(|(key, value)| format!("{}={}", percent_encode(key), percent_encode(value)))
        .collect::<Vec<_>>()
        .join("&");
    let separator = if cfg.auth_login_url.contains('?') {
        '&'
    } else {
        '?'
    };
    Ok(format!("{}{separator}{query}", cfg.auth_login_url))
}

/// Validate a platform `id_token` and fill the launch details.
pub fn validate_id_token(
    cfg: &ToolConfig,
    id_token: &str,
    platform_jwks: &str,
    expected_nonce: &str,
    now_unix: i64,
) -> Result<Launch, LtiError> {
    let (header_b64, payload_b64, signature_b64) = jwt::split_jwt(id_token)?;
    let header = decode_part(header_b64, "header")?;
    let alg = header
        .get("alg")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("header has no alg"))?;
    if alg != "RS256" {
        return Err(invalid(format!("unsupported alg {alg:?}")));
    }
    let kid = match header.get("kid") {
        None | Some(Value::Null) => None,
        Some(Value::String(kid)) => Some(kid.as_str()),
        Some(_) => return Err(invalid("header kid is not a string")),
    };
    let jwks: Value = serde_json::from_str(platform_jwks)
        .map_err(|e| LtiError::Json(format!("platform JWKS: {e}")))?;
    let jwk = find_jwk(&jwks, kid)?;
    let n = jwk_component(jwk, "n")?;
    let e = jwk_component(jwk, "e")?;
    if signature_b64.is_empty() {
        return Err(invalid("token has no signature"));
    }
    let signature = jwt::base64url_decode(signature_b64)?;
    let signing_input = format!("{header_b64}.{payload_b64}");
    jwt::verify_rs256(&n, &e, signing_input.as_bytes(), &signature)?;
    let claims = decode_part(payload_b64, "payload")?;
    check_claims(cfg, &claims, expected_nonce, now_unix)?;
    build_launch(&claims)
}

/// The tool's public JWKS, built from its PKCS#8 private key.
pub fn tool_jwks(cfg: &ToolConfig) -> Result<String, LtiError> {
    let key =
        RsaKeyPair::from_pkcs8(&cfg.private_key_pkcs8).map_err(|e| LtiError::Key(e.to_string()))?;
    let public = PublicKeyComponents::<Vec<u8>>::from(key.public());
    let jwks = json!({
        "keys": [{
            "kty": "RSA",
            "alg": "RS256",
            "use": "sig",
            "kid": cfg.key_id.as_str(),
            "n": jwt::base64url_encode(&public.n),
            "e": jwt::base64url_encode(&public.e),
        }]
    });
    serde_json::to_string(&jwks).map_err(|e| LtiError::Json(e.to_string()))
}

/// A signed `client_credentials` assertion for the platform token endpoint.
pub fn client_assertion(cfg: &ToolConfig, now_unix: i64, jti: &str) -> Result<String, LtiError> {
    let header = json!({
        "alg": "RS256",
        "typ": "JWT",
        "kid": cfg.key_id.as_str(),
    });
    let claims = json!({
        "iss": cfg.client_id.as_str(),
        "sub": cfg.client_id.as_str(),
        "aud": cfg.auth_token_url.as_str(),
        "iat": now_unix,
        "exp": now_unix.saturating_add(300),
        "jti": jti,
    });
    jwt::sign_rs256(&cfg.private_key_pkcs8, &header, &claims)
}

/// The `application/x-www-form-urlencoded` body for an access-token request.
pub fn token_request_body(_cfg: &ToolConfig, assertion: &str, scopes: &[String]) -> String {
    format!(
        "grant_type=client_credentials&client_assertion_type={}&client_assertion={}&scope={}",
        percent_encode("urn:ietf:params:oauth:client-assertion-type:jwt-bearer"),
        percent_encode(assertion),
        percent_encode(&scopes.join(" ")),
    )
}

/// An AGS score for one learner. `status` is `passed`, `failed` or `completed`.
///
/// `percent` is `None` when the course completed without a score; the payload
/// then omits `scoreGiven` and `scoreMaximum` and sends only the progress
/// fields.
pub fn score_payload(
    launch: &Launch,
    percent: Option<u8>,
    status: &str,
    timestamp_iso: &str,
) -> Result<String, LtiError> {
    match status {
        "passed" | "failed" | "completed" => {}
        _ => {
            return Err(LtiError::BadRequest(format!(
                "unsupported completion status {status:?}"
            )));
        }
    }
    let mut payload = json!({
        "userId": launch.subject.as_str(),
        "activityProgress": "Completed",
        "gradingProgress": "FullyGraded",
        "timestamp": timestamp_iso,
        "comment": status,
    });
    if let Some(percent) = percent {
        if percent > 100 {
            return Err(LtiError::BadRequest("percent must be 0..=100".into()));
        }
        payload["scoreGiven"] = json!(percent);
        payload["scoreMaximum"] = json!(100);
    }
    serde_json::to_string(&payload).map_err(|e| LtiError::Json(e.to_string()))
}

/// Append `/scores` to a line-item URL, before any query string.
pub fn lineitem_score_url(lineitem: &str) -> String {
    match lineitem.split_once('?') {
        Some((base, query)) => format!("{base}/scores?{query}"),
        None => format!("{lineitem}/scores"),
    }
}

fn invalid(message: impl Into<String>) -> LtiError {
    LtiError::InvalidToken(message.into())
}

fn decode_part(part: &str, name: &str) -> Result<Value, LtiError> {
    let bytes =
        jwt::base64url_decode(part).map_err(|_| invalid(format!("{name} is not base64url")))?;
    serde_json::from_slice(&bytes).map_err(|_| invalid(format!("{name} is not JSON")))
}

fn find_jwk<'a>(jwks: &'a Value, kid: Option<&str>) -> Result<&'a Value, LtiError> {
    let keys = jwks
        .get("keys")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("platform JWKS has no keys"))?;
    if let Some(kid) = kid {
        return keys
            .iter()
            .find(|key| {
                key.get("kid").and_then(Value::as_str) == Some(kid)
                    && key.get("kty").and_then(Value::as_str) == Some("RSA")
            })
            .ok_or_else(|| invalid(format!("no RSA key with kid {kid:?}")));
    }
    let mut rsa = keys
        .iter()
        .filter(|key| key.get("kty").and_then(Value::as_str) == Some("RSA"));
    let Some(only) = rsa.next() else {
        return Err(invalid("platform JWKS has no RSA key"));
    };
    if rsa.next().is_some() {
        return Err(invalid(
            "platform JWKS has several RSA keys and the token has no kid",
        ));
    }
    Ok(only)
}

fn jwk_component(jwk: &Value, name: &str) -> Result<Vec<u8>, LtiError> {
    let text = jwk
        .get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| invalid(format!("JWK has no {name}")))?;
    jwt::base64url_decode(text).map_err(|_| invalid(format!("JWK {name} is not base64url")))
}

fn check_claims(
    cfg: &ToolConfig,
    claims: &Value,
    expected_nonce: &str,
    now_unix: i64,
) -> Result<(), LtiError> {
    if claims.get("iss").and_then(Value::as_str) != Some(cfg.issuer.as_str()) {
        return Err(invalid("issuer does not match"));
    }
    if !audience_has(claims.get("aud"), &cfg.client_id) {
        return Err(invalid("audience does not contain the client_id"));
    }
    let exp = claims
        .get("exp")
        .and_then(Value::as_i64)
        .ok_or_else(|| invalid("exp is missing"))?;
    if exp <= now_unix {
        return Err(invalid("token is expired"));
    }
    let iat = claims
        .get("iat")
        .and_then(Value::as_i64)
        .ok_or_else(|| invalid("iat is missing"))?;
    if iat > now_unix.saturating_add(300) {
        return Err(invalid("token was issued in the future"));
    }
    if claims.get("nonce").and_then(Value::as_str) != Some(expected_nonce) {
        return Err(invalid("nonce does not match"));
    }
    if claims.get(MESSAGE_TYPE).and_then(Value::as_str) != Some("LtiResourceLinkRequest") {
        return Err(invalid("not an LtiResourceLinkRequest"));
    }
    if claims.get(VERSION).and_then(Value::as_str) != Some("1.3.0") {
        return Err(invalid("LTI version is not 1.3.0"));
    }
    if claims.get(DEPLOYMENT_ID).and_then(Value::as_str) != Some(cfg.deployment_id.as_str()) {
        return Err(invalid("deployment_id does not match"));
    }
    Ok(())
}

fn audience_has(aud: Option<&Value>, client_id: &str) -> bool {
    match aud {
        Some(Value::String(value)) => value == client_id,
        Some(Value::Array(values)) => values.iter().any(|value| value.as_str() == Some(client_id)),
        _ => false,
    }
}

fn build_launch(claims: &Value) -> Result<Launch, LtiError> {
    let subject = required_str(claims, "sub")?.to_string();
    let name = claims
        .get("name")
        .and_then(Value::as_str)
        .map(str::to_string);
    let context_id = claims
        .get(CONTEXT)
        .and_then(|context| context.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let resource_link_id = claims
        .get(RESOURCE_LINK)
        .and_then(|link| link.get("id"))
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("resource_link.id is missing"))?
        .to_string();
    let roles = match claims.get(ROLES) {
        Some(Value::Array(values)) => values
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
        Some(_) => return Err(invalid("roles is not an array")),
        None => Vec::new(),
    };
    let ags = match claims.get(AGS) {
        Some(value) => Some(Ags {
            lineitem: value
                .get("lineitem")
                .and_then(Value::as_str)
                .map(str::to_string),
            lineitems: value
                .get("lineitems")
                .and_then(Value::as_str)
                .map(str::to_string),
            scope: match value.get("scope") {
                Some(Value::Array(items)) => items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect(),
                Some(_) => return Err(invalid("AGS scope is not an array")),
                None => Vec::new(),
            },
        }),
        None => None,
    };
    let deployment_id = required_str(claims, DEPLOYMENT_ID)?.to_string();
    Ok(Launch {
        subject,
        name,
        context_id,
        resource_link_id,
        roles,
        ags,
        deployment_id,
    })
}

fn required_str<'a>(value: &'a Value, name: &str) -> Result<&'a str, LtiError> {
    value
        .get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| invalid(format!("{name} is missing")))
}

fn percent_encode(value: &str) -> String {
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

#[cfg(test)]
mod tests;
