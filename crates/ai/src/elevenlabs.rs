//! ElevenLabs text-to-speech, voice and model requests.
//!
//! Like the rest of this crate everything here is pure: it builds an
//! [`HttpCall`] for the app's transport and reads the answer. No key is ever
//! printed, and a voice id is checked before it can go into a URL path.

use std::fmt;

use serde_json::{Value, json};

use crate::{HttpCall, HttpMethod, MAX_BODY, MAX_SPEECH_BODY};

/// The ElevenLabs API root.
pub const API_ROOT: &str = "https://api.elevenlabs.io";

/// Model used when the author has not chosen one.
pub const DEFAULT_MODEL: &str = "eleven_multilingual_v2";

/// Most characters sent in one speech request when the model does not say.
pub const DEFAULT_MAX_CHARS: usize = 10_000;

/// Longest voice id this client accepts.
pub const MAX_VOICE_ID_LEN: usize = 64;

/// Most pages of voices the client will follow.
pub const MAX_VOICES_PAGES: u32 = 10;

/// Most entries kept from one hostile list answer.
const MAX_LIST_ENTRIES: usize = 1_000;

/// Largest text length a model may claim, so a hostile number stays bounded.
const MAX_MODEL_CHARS: usize = 10_000_000;

/// ElevenLabs settings. The key is never printed.
#[derive(Clone, PartialEq, Eq, Default)]
pub struct ElevenLabsConfig {
    /// API key.
    pub api_key: String,
    /// Voice id used in the speech URL path.
    pub voice_id: String,
    /// Human name of the voice, for the window only.
    pub voice_name: String,
    /// Model id; empty means [`DEFAULT_MODEL`].
    pub model_id: String,
}

impl fmt::Debug for ElevenLabsConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ElevenLabsConfig")
            .field("api_key", &"***")
            .field("voice_id", &self.voice_id)
            .field("voice_name", &self.voice_name)
            .field("model_id", &self.model_id)
            .finish()
    }
}

/// One voice from the list or a lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElevenLabsVoice {
    /// Voice id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Category such as `premade` or `cloned`.
    pub category: String,
}

/// One page of voices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElevenLabsVoicesPage {
    /// Voices on this page.
    pub voices: Vec<ElevenLabsVoice>,
    /// True when another page is available.
    pub has_more: bool,
    /// Token for the next page.
    pub next_page_token: Option<String>,
}

/// One text-to-speech model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElevenLabsModel {
    /// Model id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// The model's own character limit, when it gives one.
    pub max_chars: Option<usize>,
}

/// A problem an author can act on. It never contains the API key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElevenLabsError {
    /// No API key was entered.
    MissingKey,
    /// The key contains a control character.
    InvalidKey,
    /// No voice was chosen.
    MissingVoice,
    /// The voice id contains characters that cannot go in a URL path.
    InvalidVoiceId,
    /// There is no text to speak.
    EmptyText,
    /// The text is longer than this client or the model allows.
    TextTooLong {
        /// The limit.
        limit: usize,
        /// The number of characters.
        characters: usize,
    },
    /// 401 or 403, or `invalid_api_key`.
    RejectedKey,
    /// A restricted key without the needed permission.
    MissingPermissions,
    /// The account is out of credits.
    OutOfCredits,
    /// The voice does not exist.
    VoiceNotFound,
    /// The model rejected the text length.
    ProviderTextTooLong,
    /// 429.
    Busy,
    /// The answer body was too large.
    BodyTooLarge,
    /// The JSON was not the shape this client expects.
    BadResponse {
        /// At most [`MAX_EXCERPT`] characters of the body.
        excerpt: String,
    },
    /// Another non-success status.
    HttpStatus {
        /// HTTP status code.
        status: u16,
        /// At most [`MAX_EXCERPT`] characters of the body.
        excerpt: String,
    },
    /// A speech answer did not look like audio.
    NoAudio,
}

impl fmt::Display for ElevenLabsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingKey => f.write_str("Enter your ElevenLabs API key."),
            Self::InvalidKey => f.write_str("The API key contains characters that can't be used."),
            Self::MissingVoice => f.write_str("Choose a voice or paste a voice ID."),
            Self::InvalidVoiceId => {
                f.write_str("A voice ID can only use letters, digits, _ and -.")
            }
            Self::EmptyText => f.write_str("Enter some text to speak."),
            Self::TextTooLong { limit, characters } => write!(
                f,
                "Text is limited to {limit} characters (this is {characters})."
            ),
            Self::RejectedKey => f.write_str("ElevenLabs rejected this API key."),
            Self::MissingPermissions => f.write_str(
                "This API key isn't allowed to do that. In ElevenLabs, give the key access to \
                 text to speech and voices, or paste a voice ID.",
            ),
            Self::OutOfCredits => f.write_str("Your ElevenLabs account is out of credits."),
            Self::VoiceNotFound => f.write_str("ElevenLabs can't find that voice."),
            Self::ProviderTextTooLong => f.write_str("That's too much text for this model."),
            Self::Busy => {
                f.write_str("ElevenLabs is busy or you've hit a limit. Try again shortly.")
            }
            Self::BodyTooLarge => f.write_str("The answer was too large to read."),
            Self::BadResponse { excerpt } => {
                if excerpt.is_empty() {
                    f.write_str("ElevenLabs's answer wasn't the expected JSON.")
                } else {
                    write!(f, "ElevenLabs's answer wasn't the expected JSON: {excerpt}")
                }
            }
            Self::HttpStatus { status, excerpt } => {
                if excerpt.is_empty() {
                    write!(f, "ElevenLabs returned an error (status {status}).")
                } else {
                    write!(
                        f,
                        "ElevenLabs returned an error (status {status}): {excerpt}"
                    )
                }
            }
            Self::NoAudio => f.write_str("ElevenLabs didn't return audio."),
        }
    }
}

impl std::error::Error for ElevenLabsError {}

/// Check a voice id before it is put into a URL path.
///
/// # Errors
/// Accepts 1 to [`MAX_VOICE_ID_LEN`] ASCII letters, digits, `_` and `-`, around
/// surrounding whitespace. Anything else is refused before a call is built.
pub fn check_voice_id(id: &str) -> Result<&str, ElevenLabsError> {
    let id = id.trim();
    if id.is_empty() {
        return Err(ElevenLabsError::MissingVoice);
    }
    if id.len() > MAX_VOICE_ID_LEN
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
    {
        return Err(ElevenLabsError::InvalidVoiceId);
    }
    Ok(id)
}

fn check_key(config: &ElevenLabsConfig) -> Result<&str, ElevenLabsError> {
    let key = config.api_key.trim();
    if key.is_empty() {
        return Err(ElevenLabsError::MissingKey);
    }
    if key.chars().any(char::is_control) {
        return Err(ElevenLabsError::InvalidKey);
    }
    Ok(key)
}

fn speech_headers(config: &ElevenLabsConfig) -> Result<Vec<(String, String)>, ElevenLabsError> {
    let key = check_key(config)?;
    Ok(vec![
        ("xi-api-key".to_string(), key.to_string()),
        ("Content-Type".to_string(), "application/json".to_string()),
    ])
}

fn get_headers(config: &ElevenLabsConfig) -> Result<Vec<(String, String)>, ElevenLabsError> {
    let key = check_key(config)?;
    Ok(vec![("xi-api-key".to_string(), key.to_string())])
}

/// Build the text-to-speech call. An empty model id uses [`DEFAULT_MODEL`].
///
/// # Errors
/// Refuses an empty key, empty or invalid voice id, empty text, text over
/// [`DEFAULT_MAX_CHARS`] characters, or a key with control characters.
pub fn build_speech_call(
    config: &ElevenLabsConfig,
    text: &str,
) -> Result<HttpCall, ElevenLabsError> {
    build_speech_call_with_limit(config, text, DEFAULT_MAX_CHARS)
}

/// Build the text-to-speech call with the selected model's own character limit.
///
/// # Errors
/// The same as [`build_speech_call`], with `max_chars` in place of the default
/// limit.
pub fn build_speech_call_with_limit(
    config: &ElevenLabsConfig,
    text: &str,
    max_chars: usize,
) -> Result<HttpCall, ElevenLabsError> {
    let headers = speech_headers(config)?;
    let voice = check_voice_id(&config.voice_id)?;
    if text.trim().is_empty() {
        return Err(ElevenLabsError::EmptyText);
    }
    let characters = text.chars().count();
    if characters > max_chars {
        return Err(ElevenLabsError::TextTooLong {
            limit: max_chars,
            characters,
        });
    }
    let model = model_id(config);
    let body = json!({ "text": text, "model_id": model });
    let body = serde_json::to_vec(&body).map_err(|_| ElevenLabsError::BadResponse {
        excerpt: String::new(),
    })?;
    Ok(HttpCall {
        method: HttpMethod::Post,
        url: format!("{API_ROOT}/v1/text-to-speech/{voice}?output_format=mp3_44100_128"),
        headers,
        body,
    })
}

/// Build a call for one page of voices.
///
/// # Errors
/// Refuses an empty key or one with control characters.
pub fn build_voices_call(
    config: &ElevenLabsConfig,
    page_token: Option<&str>,
) -> Result<HttpCall, ElevenLabsError> {
    let headers = get_headers(config)?;
    let mut url = format!("{API_ROOT}/v2/voices?page_size=100");
    if let Some(token) = page_token.map(str::trim).filter(|token| !token.is_empty()) {
        url.push_str("&next_page_token=");
        url.push_str(&encode_query(token));
    }
    Ok(HttpCall {
        method: HttpMethod::Get,
        url,
        headers,
        body: Vec::new(),
    })
}

/// Build a call that looks up one voice by id.
///
/// # Errors
/// Refuses an empty or invalid voice id, or an unusable key.
pub fn build_voice_call(config: &ElevenLabsConfig, id: &str) -> Result<HttpCall, ElevenLabsError> {
    let headers = get_headers(config)?;
    let id = check_voice_id(id)?;
    Ok(HttpCall {
        method: HttpMethod::Get,
        url: format!("{API_ROOT}/v1/voices/{id}"),
        headers,
        body: Vec::new(),
    })
}

/// Build the model list call.
///
/// # Errors
/// Refuses an empty key or one with control characters.
pub fn build_models_call(config: &ElevenLabsConfig) -> Result<HttpCall, ElevenLabsError> {
    let headers = get_headers(config)?;
    Ok(HttpCall {
        method: HttpMethod::Get,
        url: format!("{API_ROOT}/v1/models"),
        headers,
        body: Vec::new(),
    })
}

fn model_id(config: &ElevenLabsConfig) -> &str {
    let model = config.model_id.trim();
    if model.is_empty() {
        DEFAULT_MODEL
    } else {
        model
    }
}

/// Read a page of voices.
///
/// # Errors
/// Refuses a body over [`MAX_BODY`], a non-success answer, or a body whose
/// `voices` field is not an array. Entries without a usable id are skipped.
pub fn parse_voices(status: u16, body: &[u8]) -> Result<ElevenLabsVoicesPage, ElevenLabsError> {
    if body.len() > MAX_BODY {
        return Err(ElevenLabsError::BodyTooLarge);
    }
    status_error(status, body, false)?;
    let value: Value = serde_json::from_slice(body).map_err(|_| bad_response(body))?;
    let voices = value
        .get("voices")
        .and_then(Value::as_array)
        .ok_or_else(|| bad_response(body))?;
    let voices = voices
        .iter()
        .take(MAX_LIST_ENTRIES)
        .filter_map(voice_from_value)
        .collect();
    let has_more = value
        .get("has_more")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let next_page_token = value
        .get("next_page_token")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(str::to_string);
    Ok(ElevenLabsVoicesPage {
        voices,
        has_more,
        next_page_token,
    })
}

/// Read one voice lookup.
///
/// # Errors
/// Refuses a body over [`MAX_BODY`], a non-success answer, or a body without a
/// usable voice id.
pub fn parse_voice(status: u16, body: &[u8]) -> Result<ElevenLabsVoice, ElevenLabsError> {
    if body.len() > MAX_BODY {
        return Err(ElevenLabsError::BodyTooLarge);
    }
    status_error(status, body, true)?;
    let value: Value = serde_json::from_slice(body).map_err(|_| bad_response(body))?;
    voice_from_value(&value).ok_or_else(|| bad_response(body))
}

/// Read the model list, keeping only models that can do text to speech.
///
/// A model whose `can_do_text_to_speech` is missing is kept: only an explicit
/// `false` excludes it.
///
/// # Errors
/// Refuses a body over [`MAX_BODY`], a non-success answer, or a body that is
/// not an array.
pub fn parse_models(status: u16, body: &[u8]) -> Result<Vec<ElevenLabsModel>, ElevenLabsError> {
    if body.len() > MAX_BODY {
        return Err(ElevenLabsError::BodyTooLarge);
    }
    status_error(status, body, false)?;
    let value: Value = serde_json::from_slice(body).map_err(|_| bad_response(body))?;
    let models = value.as_array().ok_or_else(|| bad_response(body))?;
    Ok(models
        .iter()
        .take(MAX_LIST_ENTRIES)
        .filter(|model| model.get("can_do_text_to_speech").and_then(Value::as_bool) != Some(false))
        .filter_map(model_from_value)
        .collect())
}

/// Read a speech answer and insist it looks like MP3 audio.
///
/// # Errors
/// Refuses a body over [`MAX_SPEECH_BODY`], maps an ElevenLabs error, and
/// refuses a body that is not audio.
pub fn parse_speech(status: u16, body: &[u8]) -> Result<Vec<u8>, ElevenLabsError> {
    if body.len() > MAX_SPEECH_BODY {
        return Err(ElevenLabsError::BodyTooLarge);
    }
    status_error(status, body, true)?;
    if !looks_like_mp3(body) {
        return Err(ElevenLabsError::NoAudio);
    }
    Ok(body.to_vec())
}

fn looks_like_mp3(body: &[u8]) -> bool {
    body.starts_with(b"ID3")
        || body.first().is_some_and(|first| *first == 0xFF)
            && body.get(1).is_some_and(|second| second & 0xE0 == 0xE0)
}

fn voice_from_value(value: &Value) -> Option<ElevenLabsVoice> {
    let id = value.get("voice_id").and_then(Value::as_str)?;
    let id = check_voice_id(id).ok()?.to_string();
    let name = value
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or(&id)
        .to_string();
    let category = value
        .get("category")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    Some(ElevenLabsVoice { id, name, category })
}

fn model_from_value(value: &Value) -> Option<ElevenLabsModel> {
    let id = value.get("model_id").and_then(Value::as_str)?;
    let id = id.trim();
    if id.is_empty() {
        return None;
    }
    let name = value
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or(id)
        .to_string();
    let max_chars = value
        .get("maximum_text_length_per_request")
        .and_then(|number| {
            number.as_u64().or_else(|| {
                number
                    .as_f64()
                    .filter(|v| v.is_finite() && *v >= 0.0)
                    .map(|v| v as u64)
            })
        })
        .and_then(|number| usize::try_from(number).ok())
        .map(|number| number.min(MAX_MODEL_CHARS))
        .filter(|number| *number > 0);
    Some(ElevenLabsModel {
        id: id.to_string(),
        name,
        max_chars,
    })
}

fn bad_response(body: &[u8]) -> ElevenLabsError {
    ElevenLabsError::BadResponse {
        excerpt: crate::excerpt(body),
    }
}

/// Read the API's own error first, then fall back to the HTTP status.
fn status_error(status: u16, body: &[u8], voice_context: bool) -> Result<(), ElevenLabsError> {
    if (200..=299).contains(&status) {
        return Ok(());
    }
    // The API's own reason wins over the status code.
    if let Some(code) = detail_status(body) {
        match code.as_str() {
            "quota_exceeded" => return Err(ElevenLabsError::OutOfCredits),
            "invalid_api_key" => return Err(ElevenLabsError::RejectedKey),
            "missing_permissions" => return Err(ElevenLabsError::MissingPermissions),
            "voice_not_found" => return Err(ElevenLabsError::VoiceNotFound),
            "max_character_limit_exceeded" => return Err(ElevenLabsError::ProviderTextTooLong),
            _ => {}
        }
    }
    Err(match status {
        401 | 403 => ElevenLabsError::RejectedKey,
        404 if voice_context => ElevenLabsError::VoiceNotFound,
        429 => ElevenLabsError::Busy,
        status => ElevenLabsError::HttpStatus {
            status,
            excerpt: crate::excerpt(body),
        },
    })
}

fn detail_status(body: &[u8]) -> Option<String> {
    let value: Value = serde_json::from_slice(body).ok()?;
    let detail = value.get("detail")?;
    let status = detail.get("status")?.as_str()?.trim();
    if status.is_empty() {
        return None;
    }
    Some(status.to_ascii_lowercase())
}

/// Percent-encode one query value, keeping only unreserved ASCII.
fn encode_query(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    fn config() -> ElevenLabsConfig {
        ElevenLabsConfig {
            api_key: "el-secret-key".to_string(),
            voice_id: "21m00Tcm4TlvDq8ikWAM".to_string(),
            voice_name: "Rachel".to_string(),
            model_id: "eleven_multilingual_v2".to_string(),
        }
    }

    fn header<'a>(call: &'a HttpCall, name: &str) -> Option<&'a str> {
        call.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    fn error_body(status: &str) -> Vec<u8> {
        format!(r#"{{"detail":{{"status":"{status}","message":"nope"}}}}"#).into_bytes()
    }

    #[test]
    fn speech_call_is_an_exact_post() {
        let call = build_speech_call(&config(), "Hello there.").expect("call");
        assert_eq!(call.method, HttpMethod::Post);
        assert_eq!(
            call.url,
            "https://api.elevenlabs.io/v1/text-to-speech/21m00Tcm4TlvDq8ikWAM?output_format=mp3_44100_128"
        );
        assert_eq!(header(&call, "xi-api-key"), Some("el-secret-key"));
        assert_eq!(header(&call, "Content-Type"), Some("application/json"));
        let value: Value = serde_json::from_slice(&call.body).expect("json");
        assert_eq!(value["text"], "Hello there.");
        assert_eq!(value["model_id"], "eleven_multilingual_v2");
    }

    #[test]
    fn an_empty_model_uses_the_default() {
        let mut c = config();
        c.model_id = "  ".to_string();
        let call = build_speech_call(&c, "Hi").expect("call");
        let value: Value = serde_json::from_slice(&call.body).expect("json");
        assert_eq!(value["model_id"], DEFAULT_MODEL);
    }

    #[test]
    fn a_model_limit_can_replace_the_default() {
        let ok = build_speech_call_with_limit(&config(), "hello", 5).expect("at the limit");
        let value: Value = serde_json::from_slice(&ok.body).expect("json");
        assert_eq!(value["text"], "hello");
        assert_eq!(
            build_speech_call_with_limit(&config(), "hello!", 5),
            Err(ElevenLabsError::TextTooLong {
                limit: 5,
                characters: 6,
            })
        );
    }

    #[test]
    fn voices_call_lists_one_hundred_and_encodes_the_token() {
        let call = build_voices_call(&config(), None).expect("call");
        assert_eq!(call.method, HttpMethod::Get);
        assert_eq!(
            call.url,
            "https://api.elevenlabs.io/v2/voices?page_size=100"
        );
        assert!(call.body.is_empty());
        assert_eq!(header(&call, "xi-api-key"), Some("el-secret-key"));

        let call = build_voices_call(&config(), Some("a b/c+d&e")).expect("call");
        assert_eq!(
            call.url,
            "https://api.elevenlabs.io/v2/voices?page_size=100&next_page_token=a%20b%2Fc%2Bd%26e"
        );
        let call = build_voices_call(&config(), Some("   ")).expect("call");
        assert_eq!(
            call.url,
            "https://api.elevenlabs.io/v2/voices?page_size=100"
        );
    }

    #[test]
    fn voice_and_model_calls_are_gets() {
        let call = build_voice_call(&config(), "abc_123-X").expect("call");
        assert_eq!(call.method, HttpMethod::Get);
        assert_eq!(call.url, "https://api.elevenlabs.io/v1/voices/abc_123-X");
        assert_eq!(header(&call, "xi-api-key"), Some("el-secret-key"));

        let call = build_models_call(&config()).expect("call");
        assert_eq!(call.method, HttpMethod::Get);
        assert_eq!(call.url, "https://api.elevenlabs.io/v1/models");
        assert_eq!(header(&call, "xi-api-key"), Some("el-secret-key"));
    }

    #[test]
    fn voice_ids_that_cannot_go_in_a_path_are_refused() {
        for id in ["../x", "a b", "café", "a/b", "a?b", "a#b", "a%2Fb"] {
            let error = check_voice_id(id).expect_err(id);
            assert!(
                matches!(
                    error,
                    ElevenLabsError::InvalidVoiceId | ElevenLabsError::MissingVoice
                ),
                "{id}: {error}"
            );
        }
        let too_long = "a".repeat(MAX_VOICE_ID_LEN + 1);
        assert_eq!(
            check_voice_id(&too_long),
            Err(ElevenLabsError::InvalidVoiceId)
        );
        assert_eq!(check_voice_id("   "), Err(ElevenLabsError::MissingVoice));
        assert_eq!(check_voice_id("  abc_123-X  "), Ok("abc_123-X"));

        let mut c = config();
        c.voice_id = "../x".to_string();
        assert_eq!(
            build_speech_call(&c, "Hi"),
            Err(ElevenLabsError::InvalidVoiceId)
        );
        assert_eq!(
            build_voice_call(&c, "../x"),
            Err(ElevenLabsError::InvalidVoiceId)
        );
    }

    #[test]
    fn empty_fields_and_long_text_are_refused_before_a_call() {
        let mut c = config();
        c.api_key.clear();
        assert_eq!(
            build_speech_call(&c, "Hi"),
            Err(ElevenLabsError::MissingKey)
        );
        assert_eq!(
            build_voices_call(&c, None),
            Err(ElevenLabsError::MissingKey)
        );
        assert_eq!(build_models_call(&c), Err(ElevenLabsError::MissingKey));

        let mut c = config();
        c.api_key = "bad\nkey".to_string();
        assert_eq!(
            build_speech_call(&c, "Hi"),
            Err(ElevenLabsError::InvalidKey)
        );

        let mut c = config();
        c.voice_id.clear();
        assert_eq!(
            build_speech_call(&c, "Hi"),
            Err(ElevenLabsError::MissingVoice)
        );

        assert_eq!(
            build_speech_call(&config(), "   "),
            Err(ElevenLabsError::EmptyText)
        );
        let text = "a".repeat(DEFAULT_MAX_CHARS + 1);
        assert_eq!(
            build_speech_call(&config(), &text),
            Err(ElevenLabsError::TextTooLong {
                limit: DEFAULT_MAX_CHARS,
                characters: DEFAULT_MAX_CHARS + 1,
            })
        );
        let ok = "a".repeat(DEFAULT_MAX_CHARS);
        assert!(build_speech_call(&config(), &ok).is_ok());
        // Counted by characters, not bytes: 10,001 emoji are refused, not 10,001 bytes.
        let emoji = "🙂".repeat(DEFAULT_MAX_CHARS + 1);
        assert!(build_speech_call(&config(), &emoji).is_err());
    }

    #[test]
    fn parses_voices_and_skips_unusable_entries() {
        let body = br#"{
            "voices": [
                {"voice_id":"abc","name":"Rachel","category":"premade","preview_url":"https://x"},
                {"voice_id":"def","name":"Adam"},
                {"voice_id":"../bad","name":"Nope"},
                {"name":"No id"},
                {"voice_id":123,"name":"Wrong"}
            ],
            "has_more": true,
            "next_page_token": "tok"
        }"#;
        let page = parse_voices(200, body).expect("page");
        assert_eq!(page.voices.len(), 2);
        assert_eq!(page.voices[0].id, "abc");
        assert_eq!(page.voices[0].name, "Rachel");
        assert_eq!(page.voices[0].category, "premade");
        assert_eq!(page.voices[1].category, "");
        assert!(page.has_more);
        assert_eq!(page.next_page_token.as_deref(), Some("tok"));

        let page = parse_voices(200, br#"{"voices":[]}"#).expect("empty");
        assert!(page.voices.is_empty());
        assert!(!page.has_more);
        assert_eq!(page.next_page_token, None);

        for body in [
            b"".as_slice(),
            b"{}",
            b"[]",
            b"not json",
            br#"{"voices":"nope"}"#,
            b"\xff\xfe\xfd",
        ] {
            assert_eq!(
                parse_voices(200, body),
                Err(ElevenLabsError::BadResponse {
                    excerpt: crate::excerpt(body)
                })
            );
        }
    }

    #[test]
    fn parses_one_voice() {
        let voice = parse_voice(
            200,
            br#"{"voice_id":"abc","name":"Rachel","category":"premade"}"#,
        )
        .expect("voice");
        assert_eq!(
            voice,
            ElevenLabsVoice {
                id: "abc".to_string(),
                name: "Rachel".to_string(),
                category: "premade".to_string(),
            }
        );
        let voice = parse_voice(200, br#"{"voice_id":"abc"}"#).expect("name falls back");
        assert_eq!(voice.name, "abc");

        for body in [
            b"".as_slice(),
            b"{}",
            b"[]",
            br#"{"voice_id":"../bad","name":"No"}"#,
            br#"{"name":"No id"}"#,
        ] {
            assert!(parse_voice(200, body).is_err(), "{:?}", body);
        }
    }

    #[test]
    fn parses_models_and_keeps_only_text_to_speech_ones() {
        let body = br#"[
            {"model_id":"a","name":"Alpha","can_do_text_to_speech":true,"maximum_text_length_per_request":5000},
            {"model_id":"b","name":"Beta","can_do_text_to_speech":false},
            {"model_id":"c","name":"Gamma"},
            {"model_id":"d","can_do_text_to_speech":true,"maximum_text_length_per_request":"many"},
            {"name":"No id"}
        ]"#;
        let models = parse_models(200, body).expect("models");
        assert_eq!(models.len(), 3);
        assert_eq!(models[0].id, "a");
        assert_eq!(models[0].max_chars, Some(5000));
        assert_eq!(models[1].id, "c");
        assert_eq!(models[1].name, "Gamma");
        assert_eq!(models[2].id, "d");
        assert_eq!(models[2].name, "d");
        assert_eq!(models[2].max_chars, None);

        for body in [b"".as_slice(), b"{}", b"not json", b"\xff\xfe"] {
            assert!(parse_models(200, body).is_err(), "{:?}", body);
        }
    }

    #[test]
    fn parses_speech_and_insists_on_mp3() {
        let mp3 = [0xFF_u8, 0xFB, 0x90, 0x64];
        assert_eq!(parse_speech(200, &mp3).expect("audio"), mp3);
        assert_eq!(
            parse_speech(200, b"ID3\x04\x00\x00\x00").expect("id3"),
            b"ID3\x04\x00\x00\x00"
        );
        for body in [b"".as_slice(), b"RIFF....WAVE", br#"{"audio":1}"#] {
            assert_eq!(parse_speech(200, body), Err(ElevenLabsError::NoAudio));
        }
        let body = vec![0_u8; MAX_SPEECH_BODY + 1];
        assert_eq!(parse_speech(200, &body), Err(ElevenLabsError::BodyTooLarge));
    }

    #[test]
    fn each_detail_error_has_its_plain_sentence() {
        let cases = [
            (
                "quota_exceeded",
                "Your ElevenLabs account is out of credits.",
            ),
            ("invalid_api_key", "ElevenLabs rejected this API key."),
            (
                "missing_permissions",
                "This API key isn't allowed to do that. In ElevenLabs, give the key access to text to speech and voices, or paste a voice ID.",
            ),
            ("voice_not_found", "ElevenLabs can't find that voice."),
            (
                "max_character_limit_exceeded",
                "That's too much text for this model.",
            ),
        ];
        for (code, sentence) in cases {
            let body = error_body(code);
            assert_eq!(
                parse_voices(400, &body).expect_err("error").to_string(),
                sentence
            );
        }
        // quota_exceeded may arrive with 401, and detail wins over the status.
        assert_eq!(
            parse_voices(401, &error_body("quota_exceeded"))
                .expect_err("quota")
                .to_string(),
            "Your ElevenLabs account is out of credits."
        );
    }

    #[test]
    fn statuses_map_to_plain_sentences() {
        assert_eq!(
            parse_models(401, b"").expect_err("401").to_string(),
            "ElevenLabs rejected this API key."
        );
        assert_eq!(
            parse_models(403, b"").expect_err("403").to_string(),
            "ElevenLabs rejected this API key."
        );
        assert_eq!(
            parse_models(429, b"").expect_err("429").to_string(),
            "ElevenLabs is busy or you've hit a limit. Try again shortly."
        );
        assert_eq!(
            parse_speech(404, b"").expect_err("voice").to_string(),
            "ElevenLabs can't find that voice."
        );
        assert_eq!(
            parse_voice(404, b"").expect_err("voice").to_string(),
            "ElevenLabs can't find that voice."
        );
        let error = parse_models(500, b"server broke")
            .expect_err("500")
            .to_string();
        assert!(error.contains("500"), "{error}");
        assert!(error.contains("server broke"), "{error}");

        for body in [
            br#"{"detail":[{"loc":["body"],"msg":"bad"}]}"#.as_slice(),
            b"plain text",
        ] {
            let error = parse_voices(422, body).expect_err("422").to_string();
            assert!(error.contains("422"), "{error}");
        }
    }

    #[test]
    fn hostile_sizes_and_deep_json_do_not_panic() {
        let body = vec![b' '; MAX_BODY + 1];
        assert_eq!(parse_voices(200, &body), Err(ElevenLabsError::BodyTooLarge));
        assert_eq!(parse_voice(200, &body), Err(ElevenLabsError::BodyTooLarge));
        assert_eq!(parse_models(200, &body), Err(ElevenLabsError::BodyTooLarge));

        let mut body = String::from("{\"voices\":");
        body.push_str(&"[".repeat(10_000));
        body.push_str(&"]".repeat(10_000));
        body.push('}');
        assert!(parse_voices(200, body.as_bytes()).is_err());
    }

    #[test]
    fn debug_and_errors_never_contain_the_key() {
        let c = config();
        let printed = format!("{c:?}");
        assert!(!printed.contains("el-secret-key"), "{printed}");
        assert!(printed.contains("***"), "{printed}");

        let mut bad = config();
        bad.api_key = "el-secret-key\nX-Injected: yes".to_string();
        let errors = [
            build_speech_call(&bad, "Hi").expect_err("key").to_string(),
            build_voices_call(&bad, None).expect_err("key").to_string(),
            build_models_call(&bad).expect_err("key").to_string(),
            build_voice_call(&bad, "abc").expect_err("key").to_string(),
        ];
        for error in errors {
            assert!(!error.contains("el-secret-key"), "{error}");
        }

        let body = br#"{"detail":{"status":"invalid_api_key","message":"el-secret-key"}}"#;
        let error = parse_voices(401, body).expect_err("error").to_string();
        assert!(!error.contains("el-secret-key"), "{error}");
    }
}
