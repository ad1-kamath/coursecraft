use serde_json::{Value, json};

use crate::jwt;
use crate::{
    Ags, Launch, LoginRequest, LtiError, ToolConfig, client_assertion, lineitem_score_url,
    login_redirect, score_payload, token_request_body, tool_jwks, validate_id_token,
};

const TEST_KEY: &[u8] = include_bytes!("../tests/fixtures/test_key.pk8");
const NOW: i64 = 1_999_999_100;
const NONCE: &str = "nonce-1";

fn cfg() -> ToolConfig {
    ToolConfig {
        issuer: "https://platform.example".into(),
        client_id: "client-1".into(),
        deployment_id: "deployment-1".into(),
        auth_login_url: "https://platform.example/auth".into(),
        auth_token_url: "https://platform.example/token".into(),
        jwks_url: "https://tool.example/.well-known/jwks.json".into(),
        tool_url: "https://tool.example".into(),
        private_key_pkcs8: TEST_KEY.to_vec(),
        key_id: "tool-key".into(),
    }
}

fn jwks(cfg: &ToolConfig) -> String {
    tool_jwks(cfg).expect("tool jwks")
}

fn jwk_value(cfg: &ToolConfig) -> Value {
    serde_json::from_str(&jwks(cfg)).expect("jwks json")
}

fn components(cfg: &ToolConfig) -> (Vec<u8>, Vec<u8>) {
    let jwks = jwk_value(cfg);
    let key = jwks["keys"][0].clone();
    let n = jwt::base64url_decode(key["n"].as_str().expect("n")).expect("n base64");
    let e = jwt::base64url_decode(key["e"].as_str().expect("e")).expect("e base64");
    (n, e)
}

fn header() -> Value {
    json!({"alg": "RS256", "typ": "JWT", "kid": "tool-key"})
}

fn claims() -> Value {
    json!({
        "iss": "https://platform.example",
        "aud": "client-1",
        "sub": "user-1",
        "name": "Ada Lovelace",
        "exp": 2_000_000_000_i64,
        "iat": 1_999_999_000_i64,
        "nonce": NONCE,
        "https://purl.imsglobal.org/spec/lti/claim/message_type": "LtiResourceLinkRequest",
        "https://purl.imsglobal.org/spec/lti/claim/version": "1.3.0",
        "https://purl.imsglobal.org/spec/lti/claim/deployment_id": "deployment-1",
        "https://purl.imsglobal.org/spec/lti/claim/context": {"id": "context-1"},
        "https://purl.imsglobal.org/spec/lti/claim/resource_link": {"id": "link-1"},
        "https://purl.imsglobal.org/spec/lti/claim/roles": [
            "http://purl.imsglobal.org/vocab/lis/v2/membership#Learner"
        ],
        "https://purl.imsglobal.org/spec/lti-ags/claim/endpoint": {
            "lineitem": "https://platform.example/lineitems/1",
            "lineitems": "https://platform.example/lineitems",
            "scope": ["https://purl.imsglobal.org/spec/lti-ags/scope/score"]
        }
    })
}

fn signed(header: &Value, claims: &Value) -> String {
    let cfg = cfg();
    jwt::sign_rs256(&cfg.private_key_pkcs8, header, claims).expect("signed token")
}

fn token() -> String {
    signed(&header(), &claims())
}

fn validate(cfg: &ToolConfig, token: &str) -> Result<Launch, LtiError> {
    validate_id_token(cfg, token, &jwks(cfg), NONCE, NOW)
}

fn assert_invalid(cfg: &ToolConfig, token: &str) {
    assert!(
        matches!(validate(cfg, token), Err(LtiError::InvalidToken(_))),
        "token should be refused: {token}"
    );
}

#[test]
fn tool_jwks_has_one_rs256_key() {
    let cfg = cfg();
    let jwks = jwk_value(&cfg);
    let keys = jwks["keys"].as_array().expect("keys");
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0]["kty"], "RSA");
    assert_eq!(keys[0]["alg"], "RS256");
    assert_eq!(keys[0]["use"], "sig");
    assert_eq!(keys[0]["kid"], "tool-key");
    assert!(!keys[0]["n"].as_str().expect("n").is_empty());
    assert!(!keys[0]["e"].as_str().expect("e").is_empty());
}

#[test]
fn validates_and_fills_launch() {
    let cfg = cfg();
    let launch = validate(&cfg, &token()).expect("launch");
    assert_eq!(
        launch,
        Launch {
            subject: "user-1".into(),
            name: Some("Ada Lovelace".into()),
            context_id: Some("context-1".into()),
            resource_link_id: "link-1".into(),
            roles: vec!["http://purl.imsglobal.org/vocab/lis/v2/membership#Learner".into()],
            ags: Some(Ags {
                lineitem: Some("https://platform.example/lineitems/1".into()),
                lineitems: Some("https://platform.example/lineitems".into()),
                scope: vec!["https://purl.imsglobal.org/spec/lti-ags/scope/score".into()],
            }),
            deployment_id: "deployment-1".into(),
        }
    );
}

#[test]
fn audience_array_is_accepted() {
    let mut claims = claims();
    claims["aud"] = json!(["other-client", "client-1"]);
    let cfg = cfg();
    assert!(validate(&cfg, &signed(&header(), &claims)).is_ok());
}

#[test]
fn wrong_issuer_is_refused() {
    let mut claims = claims();
    claims["iss"] = json!("https://other.example");
    let cfg = cfg();
    assert_invalid(&cfg, &signed(&header(), &claims));
}

#[test]
fn wrong_audience_is_refused() {
    let mut claims = claims();
    claims["aud"] = json!("other-client");
    let cfg = cfg();
    assert_invalid(&cfg, &signed(&header(), &claims));
}

#[test]
fn expired_token_is_refused() {
    let mut claims = claims();
    claims["exp"] = json!(NOW);
    let cfg = cfg();
    assert_invalid(&cfg, &signed(&header(), &claims));
}

#[test]
fn future_iat_is_refused() {
    let mut claims = claims();
    claims["iat"] = json!(NOW + 301);
    let cfg = cfg();
    assert_invalid(&cfg, &signed(&header(), &claims));
}

#[test]
fn wrong_nonce_is_refused() {
    let mut claims = claims();
    claims["nonce"] = json!("other-nonce");
    let cfg = cfg();
    assert_invalid(&cfg, &signed(&header(), &claims));
}

#[test]
fn wrong_deployment_is_refused() {
    let mut claims = claims();
    claims["https://purl.imsglobal.org/spec/lti/claim/deployment_id"] = json!("other-deployment");
    let cfg = cfg();
    assert_invalid(&cfg, &signed(&header(), &claims));
}

#[test]
fn wrong_message_type_is_refused() {
    let mut claims = claims();
    claims["https://purl.imsglobal.org/spec/lti/claim/message_type"] =
        json!("LtiDeepLinkingRequest");
    let cfg = cfg();
    assert_invalid(&cfg, &signed(&header(), &claims));
}

#[test]
fn tampered_payload_is_refused() {
    let cfg = cfg();
    let token = token();
    let (header, payload, signature) = jwt::split_jwt(&token).expect("split");
    let mut claims: Value =
        serde_json::from_slice(&jwt::base64url_decode(payload).expect("payload")).expect("json");
    claims["sub"] = json!("attacker");
    let payload = jwt::base64url_encode(&serde_json::to_vec(&claims).expect("json"));
    assert_invalid(&cfg, &format!("{header}.{payload}.{signature}"));
}

#[test]
fn unknown_kid_is_refused() {
    let mut header = header();
    header["kid"] = json!("unknown-key");
    let cfg = cfg();
    assert_invalid(&cfg, &signed(&header, &claims()));
}

#[test]
fn alg_none_is_refused() {
    let cfg = cfg();
    let header = jwt::base64url_encode(
        &serde_json::to_vec(&json!({"alg": "none", "typ": "JWT"})).expect("json"),
    );
    let payload = jwt::base64url_encode(&serde_json::to_vec(&claims()).expect("json"));
    assert_invalid(&cfg, &format!("{header}.{payload}."));
}

#[test]
fn login_redirect_encodes_every_parameter() {
    let cfg = cfg();
    let req = LoginRequest {
        iss: cfg.issuer.clone(),
        login_hint: "user+1@example.com".into(),
        target_link_uri: "https://tool.example/launch?x=1".into(),
        lti_message_hint: Some("hint with space".into()),
        client_id: Some(cfg.client_id.clone()),
    };
    let url = login_redirect(&cfg, &req, "state value", "nonce+value").expect("login url");
    assert!(url.starts_with("https://platform.example/auth?"));
    for part in [
        "scope=openid",
        "response_type=id_token",
        "client_id=client-1",
        "redirect_uri=https%3A%2F%2Ftool.example%2Flti%2Flaunch",
        "login_hint=user%2B1%40example.com",
        "state=state%20value",
        "response_mode=form_post",
        "nonce=nonce%2Bvalue",
        "prompt=none",
        "lti_message_hint=hint%20with%20space",
    ] {
        assert!(url.contains(part), "missing {part} in {url}");
    }
}

#[test]
fn login_redirect_refuses_wrong_issuer_or_client() {
    let cfg = cfg();
    let mut req = LoginRequest {
        iss: "https://other.example".into(),
        login_hint: "user-1".into(),
        target_link_uri: "https://tool.example/launch".into(),
        lti_message_hint: None,
        client_id: None,
    };
    assert!(matches!(
        login_redirect(&cfg, &req, "state", "nonce"),
        Err(LtiError::BadRequest(_))
    ));
    req.iss = cfg.issuer.clone();
    req.client_id = Some("other-client".into());
    assert!(matches!(
        login_redirect(&cfg, &req, "state", "nonce"),
        Err(LtiError::BadRequest(_))
    ));
}

#[test]
fn client_assertion_round_trips_through_tool_jwks() {
    let cfg = cfg();
    let assertion = client_assertion(&cfg, NOW, "jti-1").expect("assertion");
    let (header, payload, signature) = jwt::split_jwt(&assertion).expect("split");
    let (n, e) = components(&cfg);
    let signature = jwt::base64url_decode(signature).expect("signature base64");
    jwt::verify_rs256(&n, &e, format!("{header}.{payload}").as_bytes(), &signature)
        .expect("signature verifies");

    let header: Value =
        serde_json::from_slice(&jwt::base64url_decode(header).expect("header")).expect("json");
    assert_eq!(header["alg"], "RS256");
    assert_eq!(header["kid"], "tool-key");
    let claims: Value =
        serde_json::from_slice(&jwt::base64url_decode(payload).expect("payload")).expect("json");
    assert_eq!(claims["iss"], "client-1");
    assert_eq!(claims["sub"], "client-1");
    assert_eq!(claims["aud"], "https://platform.example/token");
    assert_eq!(claims["iat"], NOW);
    assert_eq!(claims["exp"], NOW + 300);
    assert_eq!(claims["jti"], "jti-1");
}

#[test]
fn token_request_body_is_encoded() {
    let cfg = cfg();
    let body = token_request_body(&cfg, "a.b.c", &["scope one".into(), "scope-two".into()]);
    assert!(body.starts_with("grant_type=client_credentials&client_assertion_type="));
    assert!(body.contains(
        "client_assertion_type=urn%3Aietf%3Aparams%3Aoauth%3Aclient-assertion-type%3Ajwt-bearer"
    ));
    assert!(body.contains("&client_assertion=a.b.c&"));
    assert!(body.contains("&scope=scope%20one%20scope-two"));
}

#[test]
fn score_payload_has_ags_shape() {
    let cfg = cfg();
    let launch = validate(&cfg, &token()).expect("launch");
    let body = score_payload(&launch, Some(87), "passed", "2026-01-02T03:04:05Z").expect("score");
    let value: Value = serde_json::from_str(&body).expect("score json");
    assert_eq!(value["userId"], "user-1");
    assert_eq!(value["scoreGiven"], 87);
    assert_eq!(value["scoreMaximum"], 100);
    assert_eq!(value["activityProgress"], "Completed");
    assert_eq!(value["gradingProgress"], "FullyGraded");
    assert_eq!(value["timestamp"], "2026-01-02T03:04:05Z");
    assert_eq!(value["comment"], "passed");
    assert!(score_payload(&launch, Some(0), "unknown", "2026-01-02T03:04:05Z").is_err());
}

#[test]
fn a_progress_only_score_omits_score_given() {
    let cfg = cfg();
    let launch = validate(&cfg, &token()).expect("launch");
    let body = score_payload(&launch, None, "completed", "2026-01-02T03:04:05Z")
        .expect("progress payload");
    let value: Value = serde_json::from_str(&body).expect("payload json");
    assert_eq!(value["userId"], "user-1");
    assert!(value.get("scoreGiven").is_none(), "{value}");
    assert!(value.get("scoreMaximum").is_none(), "{value}");
    assert_eq!(value["activityProgress"], "Completed");
    assert_eq!(value["gradingProgress"], "FullyGraded");
    assert_eq!(value["comment"], "completed");
}

#[test]
fn lineitem_score_url_keeps_the_query() {
    assert_eq!(
        lineitem_score_url("https://platform.example/lineitems/1"),
        "https://platform.example/lineitems/1/scores"
    );
    assert_eq!(
        lineitem_score_url("https://platform.example/lineitems/1?type=x"),
        "https://platform.example/lineitems/1/scores?type=x"
    );
}

#[test]
fn oversized_token_is_bad_request() {
    let cfg = cfg();
    let token = "a".repeat(1024 * 1024);
    assert!(matches!(
        validate_id_token(&cfg, &token, &jwks(&cfg), NONCE, NOW),
        Err(LtiError::BadRequest(_))
    ));
}
