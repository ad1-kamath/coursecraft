//! CourseCraft's AI client (L3).
//!
//! Everything here is pure: it builds the request an OpenAI-compatible
//! endpoint expects, checks an [`AiConfig`], and reads the reply. It never
//! opens a socket, so callers own the transport and tests never touch the
//! network. An error never contains the API key, and at most 200 characters of
//! a response body are ever quoted back.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

use std::fmt;

use serde_json::{Value, json};

pub mod agent_chat;
pub mod agents;
pub mod alt_prompt;
pub mod elevenlabs;
pub mod outline_prompt;
pub mod translate_prompt;

pub use agent_chat::{
    AgentImage, AgentPrompt, AiSource, REPLY_RULE, build_prompt as build_agent_prompt, chat_argv,
    failure_sentence as agent_failure_sentence, parse_claude_answer, parse_codex_answer,
};
pub use alt_prompt::{
    AltContext, MAX_ALT_CHARS, MAX_NEARBY_CHARS, alt_text_request, base64_encode, clean_alt_text,
    image_data_url,
};
pub use elevenlabs::{
    API_ROOT as ELEVENLABS_API_ROOT, DEFAULT_MAX_CHARS,
    DEFAULT_MAX_CHARS as ELEVENLABS_DEFAULT_MAX_CHARS, DEFAULT_MODEL,
    DEFAULT_MODEL as ELEVENLABS_DEFAULT_MODEL, ElevenLabsConfig, ElevenLabsError, ElevenLabsModel,
    ElevenLabsVoice, ElevenLabsVoicesPage, MAX_VOICE_ID_LEN, MAX_VOICES_PAGES, build_models_call,
    build_speech_call as build_elevenlabs_speech_call,
    build_speech_call_with_limit as build_elevenlabs_speech_call_with_limit, build_voice_call,
    build_voices_call, check_voice_id, parse_models as parse_elevenlabs_models,
    parse_speech as parse_elevenlabs_speech, parse_voice, parse_voices,
};
pub use outline_prompt::{
    MAX_DOCUMENT_CHARS, OutlineOptions, document_is_truncated, outline_request,
};
pub use translate_prompt::{
    MAX_BATCH_CHARS, MAX_BATCH_UNITS, TranslationBatch, TranslationOutcome, TranslationResult,
    UnitIn, batch_units, parse_translations, translate_request, translation_problem,
};

/// Largest response body this crate will read: 16 MiB.
pub const MAX_BODY: usize = 16 * 1024 * 1024;

/// Most characters of a response body an error may quote.
pub const MAX_EXCERPT: usize = 200;

/// Largest audio or video file sent for transcription: 25 MiB.
pub const MAX_AUDIO_UPLOAD: usize = 25 * 1024 * 1024;

/// Most characters of text sent to be spoken.
pub const MAX_SPEECH_CHARS: usize = 4_000;

/// Largest speech response body this crate will read: 32 MiB.
pub const MAX_SPEECH_BODY: usize = 32 * 1024 * 1024;

/// How many multipart boundaries to try before giving up.
pub const MAX_BOUNDARY_ATTEMPTS: u32 = 10_000;

/// An OpenAI-compatible endpoint, a model and an optional key.
///
/// The key is never printed: [`Debug`] shows `***` instead. This type has no
/// `Serialize`; the desktop app stores its own prefs type so a key cannot be
/// written into a course, a package or an XLIFF.
#[derive(Clone, PartialEq, Eq, Default)]
pub struct AiConfig {
    /// Base URL, for example `https://api.openai.com/v1`.
    pub base_url: String,
    /// Model name, for example `gpt-4o-mini`.
    pub model: String,
    /// API key; empty means no `Authorization` header is sent.
    pub api_key: String,
    /// Model for speech-to-text, for example `whisper-1`; empty means unset.
    pub speech_to_text_model: String,
    /// Model for text-to-speech, for example `tts-1`; empty means unset.
    pub text_to_speech_model: String,
    /// Voice for text-to-speech, for example `alloy`; empty means the default.
    pub voice: String,
}

impl fmt::Debug for AiConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AiConfig")
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .field("api_key", &"***")
            .field("speech_to_text_model", &self.speech_to_text_model)
            .field("text_to_speech_model", &self.text_to_speech_model)
            .field("voice", &self.voice)
            .finish()
    }
}

/// The HTTP method a call uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HttpMethod {
    /// `POST`, the default for the OpenAI-compatible calls.
    #[default]
    Post,
    /// `GET`, used by the ElevenLabs list and lookup calls.
    Get,
}

/// One HTTP call, ready for a transport such as `ehttp`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpCall {
    /// Request method.
    pub method: HttpMethod,
    /// Request URL.
    pub url: String,
    /// Header name/value pairs.
    pub headers: Vec<(String, String)>,
    /// Request body.
    pub body: Vec<u8>,
}

/// Who wrote a chat message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Instructions for the model.
    System,
    /// The author's input.
    User,
    /// An earlier model answer.
    Assistant,
}

impl Role {
    /// The name the OpenAI chat format uses.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::User => "user",
            Self::Assistant => "assistant",
        }
    }
}

/// A message's content: text, or text plus image data URLs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageContent {
    /// Plain text.
    Text(String),
    /// Text followed by one or more image data URLs.
    TextAndImages {
        /// The text part.
        text: String,
        /// `data:image/png;base64,…` URLs.
        images: Vec<String>,
    },
}

/// One message in a chat request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatMessage {
    /// Who wrote it.
    pub role: Role,
    /// What it says.
    pub content: MessageContent,
}

/// A chat completion request.
#[derive(Debug, Clone, PartialEq)]
pub struct ChatRequest {
    /// Messages, oldest first.
    pub messages: Vec<ChatMessage>,
    /// Optional cap on the answer length.
    pub max_tokens: Option<u32>,
    /// Optional sampling temperature.
    pub temperature: Option<f32>,
}

/// The model's answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatReply {
    /// Answer text.
    pub text: String,
    /// True when the endpoint hit `max_tokens` (`finish_reason: "length"`).
    pub truncated: bool,
}

/// What a finished background AI call produced.
///
/// Chat answers are text; captions are a WebVTT file; speech is raw audio
/// bytes. Keeping the bytes separate means a binary answer never goes through
/// UTF-8.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiReply {
    /// A chat completion.
    Chat(ChatReply),
    /// A transcription answer that parsed as timed WebVTT.
    Captions(String),
    /// A speech answer, checked to look like MP3 audio.
    Speech(Vec<u8>),
    /// One page of ElevenLabs voices.
    ElevenLabsVoices(elevenlabs::ElevenLabsVoicesPage),
    /// One ElevenLabs voice lookup.
    ElevenLabsVoice(elevenlabs::ElevenLabsVoice),
    /// The ElevenLabs models that can do text to speech.
    ElevenLabsModels(Vec<elevenlabs::ElevenLabsModel>),
}

/// A problem an author can act on. Never contains the API key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiError {
    /// The base URL is missing, malformed or uses the wrong scheme.
    InvalidBaseUrl(String),
    /// A model name is required.
    EmptyModel,
    /// A field contains a newline or another control character.
    HeaderInjection(String),
    /// The request itself is unusable.
    InvalidRequest(String),
    /// 401 or 403: the key was rejected.
    RejectedKey,
    /// 404: the path or model does not exist.
    NotFound,
    /// 429: too many requests.
    RateLimited,
    /// Any other non-success status, with a short excerpt.
    HttpStatus {
        /// HTTP status code.
        status: u16,
        /// At most [`MAX_EXCERPT`] characters of the body.
        excerpt: String,
    },
    /// The body is not the JSON shape this client expects.
    BadResponse {
        /// At most [`MAX_EXCERPT`] characters of the body.
        excerpt: String,
    },
    /// The endpoint answered successfully but with no text.
    EmptyAnswer,
    /// The answer stopped before it was finished.
    TruncatedAnswer,
    /// The response body was larger than [`MAX_BODY`].
    BodyTooLarge,
    /// 404 from speech-to-text: this server does not offer it.
    NoTranscriptionEndpoint,
    /// 404 from text-to-speech: this server does not offer it.
    NoSpeechEndpoint,
    /// A transcription answer contained no timed captions.
    NoTimedCaptions,
    /// A speech answer did not look like audio.
    NoAudio,
}

impl fmt::Display for AiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidBaseUrl(message)
            | Self::HeaderInjection(message)
            | Self::InvalidRequest(message) => f.write_str(message),
            Self::EmptyModel => f.write_str("Choose a model."),
            Self::RejectedKey => f.write_str("The endpoint rejected the key."),
            Self::NotFound => {
                f.write_str("The endpoint couldn't find that. Check the base URL and model.")
            }
            Self::RateLimited => f.write_str("The endpoint is busy. Wait a moment and try again."),
            Self::HttpStatus { status, excerpt } => {
                if excerpt.is_empty() {
                    write!(f, "The endpoint returned an error (status {status}).")
                } else {
                    write!(
                        f,
                        "The endpoint returned an error (status {status}): {excerpt}"
                    )
                }
            }
            Self::BadResponse { excerpt } => {
                if excerpt.is_empty() {
                    f.write_str("The endpoint's answer wasn't the expected JSON.")
                } else {
                    write!(
                        f,
                        "The endpoint's answer wasn't the expected JSON: {excerpt}"
                    )
                }
            }
            Self::EmptyAnswer => f.write_str("The endpoint returned an empty answer."),
            Self::TruncatedAnswer => f.write_str("The answer was cut off before it finished."),
            Self::BodyTooLarge => f.write_str("The answer was too large to read."),
            Self::NoTranscriptionEndpoint => f.write_str(
                "This server doesn't offer speech-to-text. Local servers often don't: try a hosted \
                 one.",
            ),
            Self::NoSpeechEndpoint => f.write_str(
                "This server doesn't offer text-to-speech. Local servers often don't: try a hosted \
                 one.",
            ),
            Self::NoTimedCaptions => f.write_str("The server didn't return timed captions."),
            Self::NoAudio => f.write_str("The server didn't return audio."),
        }
    }
}

impl std::error::Error for AiError {}

/// Check a configuration before any chat request is built.
///
/// Refuses a URL that is not `https://`, or `http://` to localhost,
/// `127.0.0.1` or `[::1]`, so a key never travels in clear text to another
/// machine. Also refuses an empty model and control characters (including a
/// newline) in the key or model, which could inject a header.
///
/// # Errors
/// Returns a plain sentence naming the field and how to fix it.
pub fn check_config(config: &AiConfig) -> Result<(), AiError> {
    check_base(config)?;
    check_model(&config.model)
}

/// Check the endpoint and a feature's own model before building an audio call.
///
/// The chat model is not required here: a server may offer only speech-to-text
/// or only text-to-speech.
///
/// # Errors
/// Returns the same endpoint error as [`check_config`], or [`AiError::EmptyModel`]
/// when `model` is empty.
pub fn check_audio_config(config: &AiConfig, model: &str) -> Result<(), AiError> {
    check_base(config)?;
    check_model(model)
}

fn check_base(config: &AiConfig) -> Result<(), AiError> {
    let base = config.base_url.trim();
    if base.is_empty() {
        return Err(AiError::InvalidBaseUrl(
            "Enter the endpoint's base URL.".to_string(),
        ));
    }
    if base.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(AiError::InvalidBaseUrl(
            "The base URL contains spaces or characters that can't be used.".to_string(),
        ));
    }
    let Some((scheme, _rest)) = base.split_once("://") else {
        return Err(AiError::InvalidBaseUrl(
            "The base URL must start with https://.".to_string(),
        ));
    };
    let https = scheme.eq_ignore_ascii_case("https");
    let http = scheme.eq_ignore_ascii_case("http");
    if !https && !http {
        return Err(AiError::InvalidBaseUrl(
            "The base URL must start with https://.".to_string(),
        ));
    }
    let Some(host) = host_of(base) else {
        return Err(AiError::InvalidBaseUrl(
            "The base URL needs a host, such as api.example.com.".to_string(),
        ));
    };
    if http && !is_local_host(&host) {
        return Err(AiError::InvalidBaseUrl(
            "Use https:// for a server on another machine. Plain http:// is only allowed for localhost."
                .to_string(),
        ));
    }
    if config.api_key.chars().any(char::is_control) {
        return Err(AiError::HeaderInjection(
            "The API key contains characters that can't be used.".to_string(),
        ));
    }
    Ok(())
}

fn check_model(model: &str) -> Result<(), AiError> {
    let model = model.trim();
    if model.is_empty() {
        return Err(AiError::EmptyModel);
    }
    if model.chars().any(char::is_control) {
        return Err(AiError::HeaderInjection(
            "The model name contains characters that can't be used.".to_string(),
        ));
    }
    Ok(())
}

/// Build the `POST {base}/chat/completions` call for a request.
///
/// A trailing slash on the base URL is removed. Nothing else is added, so a
/// preset that needs `/v1` already has it.
///
/// # Errors
/// Returns the same configuration errors as [`check_config`], or an error for
/// a non-finite temperature.
pub fn build_chat_call(config: &AiConfig, request: &ChatRequest) -> Result<HttpCall, AiError> {
    check_config(config)?;
    if request
        .temperature
        .is_some_and(|temperature| !temperature.is_finite())
    {
        return Err(AiError::InvalidRequest(
            "The request has an invalid temperature.".to_string(),
        ));
    }

    let base = config.base_url.trim().trim_end_matches('/');
    let url = format!("{base}/chat/completions");

    let mut headers = vec![("Content-Type".to_string(), "application/json".to_string())];
    let key = config.api_key.trim();
    if !key.is_empty() {
        headers.push(("Authorization".to_string(), format!("Bearer {key}")));
    }

    let messages: Vec<Value> = request.messages.iter().map(message_json).collect();
    let mut body = json!({
        "model": config.model.trim(),
        "messages": messages,
    });
    if let Some(max_tokens) = request.max_tokens {
        body["max_tokens"] = json!(max_tokens);
    }
    if let Some(temperature) = request.temperature {
        body["temperature"] = json!(temperature);
    }
    let body = serde_json::to_vec(&body)
        .map_err(|_| AiError::InvalidRequest("The request couldn't be prepared.".to_string()))?;

    Ok(HttpCall {
        method: HttpMethod::Post,
        url,
        headers,
        body,
    })
}

fn message_json(message: &ChatMessage) -> Value {
    let content = match &message.content {
        MessageContent::Text(text) => Value::String(text.clone()),
        MessageContent::TextAndImages { text, images } => {
            let mut parts = Vec::with_capacity(images.len() + 1);
            parts.push(json!({ "type": "text", "text": text }));
            for url in images {
                parts.push(json!({
                    "type": "image_url",
                    "image_url": { "url": url },
                }));
            }
            Value::Array(parts)
        }
    };
    json!({ "role": message.role.as_str(), "content": content })
}

/// Read a chat completion response.
///
/// `finish_reason: "length"` sets [`ChatReply::truncated`].
///
/// # Errors
/// Maps 401/403, 404, 429 and other statuses to plain sentences, and refuses a
/// body that is too large, not UTF-8 JSON in the expected shape, or empty.
/// Never panics on hostile JSON.
pub fn parse_chat_response(status: u16, body: &[u8]) -> Result<ChatReply, AiError> {
    if body.len() > MAX_BODY {
        return Err(AiError::BodyTooLarge);
    }
    match status {
        401 | 403 => return Err(AiError::RejectedKey),
        404 => return Err(AiError::NotFound),
        429 => return Err(AiError::RateLimited),
        status if !(200..=299).contains(&status) => {
            return Err(AiError::HttpStatus {
                status,
                excerpt: excerpt(body),
            });
        }
        _ => {}
    }

    let value: Value = serde_json::from_slice(body).map_err(|_| AiError::BadResponse {
        excerpt: excerpt(body),
    })?;
    let choice = value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .ok_or_else(|| AiError::BadResponse {
            excerpt: excerpt(body),
        })?;
    let text = choice
        .get("message")
        .and_then(|message| message.get("content"))
        .and_then(Value::as_str)
        .ok_or_else(|| AiError::BadResponse {
            excerpt: excerpt(body),
        })?;
    if text.trim().is_empty() {
        return Err(AiError::EmptyAnswer);
    }
    let truncated = choice
        .get("finish_reason")
        .and_then(Value::as_str)
        .is_some_and(|reason| reason == "length");
    Ok(ChatReply {
        text: text.to_string(),
        truncated,
    })
}

/// Build the `POST {base}/audio/transcriptions` multipart call.
///
/// The file is sent as it is; the server may or may not accept its format. The
/// answer is requested as WebVTT. The boundary is chosen so it cannot occur in
/// the file's bytes, and the file name is reduced to safe ASCII characters
/// because it appears in a header line.
///
/// # Errors
/// Refuses an unusable endpoint or model, a file over [`MAX_AUDIO_UPLOAD`], or
/// the (vanishingly unlikely) case where no boundary could be chosen.
pub fn build_transcription_call(
    config: &AiConfig,
    model: &str,
    file_name: &str,
    bytes: &[u8],
) -> Result<HttpCall, AiError> {
    check_audio_config(config, model)?;
    if bytes.len() > MAX_AUDIO_UPLOAD {
        return Err(AiError::InvalidRequest(format!(
            "The file is larger than the 25 MB limit (it is {:.1} MB).",
            bytes.len() as f64 / (1024.0 * 1024.0)
        )));
    }
    let boundary = pick_boundary(bytes).ok_or_else(|| {
        AiError::InvalidRequest("The file couldn't be prepared for sending.".to_string())
    })?;
    let model = model.trim();
    let file_name = safe_file_name(file_name);
    let content_type = audio_mime(&file_name);

    let mut body = Vec::with_capacity(bytes.len().saturating_add(512));
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{file_name}\"\r\nContent-Type: {content_type}\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(bytes);
    body.extend_from_slice(
        format!(
            "\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\n{model}\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"response_format\"\r\n\r\nvtt\r\n--{boundary}--\r\n"
        )
        .as_bytes(),
    );

    let base = config.base_url.trim().trim_end_matches('/');
    let url = format!("{base}/audio/transcriptions");
    let mut headers = vec![(
        "Content-Type".to_string(),
        format!("multipart/form-data; boundary={boundary}"),
    )];
    let key = config.api_key.trim();
    if !key.is_empty() {
        headers.push(("Authorization".to_string(), format!("Bearer {key}")));
    }
    Ok(HttpCall {
        method: HttpMethod::Post,
        url,
        headers,
        body,
    })
}

/// Read a transcription answer and insist on timed captions.
///
/// A JSON `{"text": …}` answer or plain text with no timing is refused with
/// [`AiError::NoTimedCaptions`]. A 404 is [`AiError::NoTranscriptionEndpoint`],
/// because many servers simply do not offer speech-to-text.
///
/// # Errors
/// Maps status codes to plain sentences and refuses a body over [`MAX_BODY`],
/// a body that is not WebVTT, or WebVTT with no cues.
pub fn parse_transcription(status: u16, body: &[u8]) -> Result<String, AiError> {
    if body.len() > MAX_BODY {
        return Err(AiError::BodyTooLarge);
    }
    match status {
        401 | 403 => return Err(AiError::RejectedKey),
        404 => return Err(AiError::NoTranscriptionEndpoint),
        429 => return Err(AiError::RateLimited),
        status if !(200..=299).contains(&status) => {
            return Err(AiError::HttpStatus {
                status,
                excerpt: excerpt(body),
            });
        }
        _ => {}
    }
    let cues = coursecraft_media::parse_webvtt(body).map_err(|_| AiError::NoTimedCaptions)?;
    if cues.is_empty() {
        return Err(AiError::NoTimedCaptions);
    }
    Ok(String::from_utf8_lossy(body).into_owned())
}

/// Build the `POST {base}/audio/speech` JSON call.
///
/// An empty voice means `alloy`. The answer is requested as MP3.
///
/// # Errors
/// Refuses an unusable endpoint or model, empty text, or text over
/// [`MAX_SPEECH_CHARS`] characters.
pub fn build_speech_call(
    config: &AiConfig,
    model: &str,
    voice: &str,
    text: &str,
) -> Result<HttpCall, AiError> {
    check_audio_config(config, model)?;
    if text.trim().is_empty() {
        return Err(AiError::InvalidRequest(
            "Enter some text to speak.".to_string(),
        ));
    }
    let characters = text.chars().count();
    if characters > MAX_SPEECH_CHARS {
        return Err(AiError::InvalidRequest(format!(
            "Text is limited to {MAX_SPEECH_CHARS} characters (this is {characters})."
        )));
    }
    let voice = if voice.trim().is_empty() {
        "alloy"
    } else {
        voice.trim()
    };
    let body = json!({
        "model": model.trim(),
        "voice": voice,
        "input": text,
        "response_format": "mp3",
    });
    let body = serde_json::to_vec(&body)
        .map_err(|_| AiError::InvalidRequest("The request couldn't be prepared.".to_string()))?;

    let base = config.base_url.trim().trim_end_matches('/');
    let url = format!("{base}/audio/speech");
    let mut headers = vec![("Content-Type".to_string(), "application/json".to_string())];
    let key = config.api_key.trim();
    if !key.is_empty() {
        headers.push(("Authorization".to_string(), format!("Bearer {key}")));
    }
    Ok(HttpCall {
        method: HttpMethod::Post,
        url,
        headers,
        body,
    })
}

/// Read a speech answer and insist that it looks like audio.
///
/// Accepts an MP3 frame sync or an `ID3` tag. A 404 is
/// [`AiError::NoSpeechEndpoint`], because many servers do not offer
/// text-to-speech.
///
/// # Errors
/// Maps status codes to plain sentences and refuses a body over
/// [`MAX_SPEECH_BODY`] or one that is not audio.
pub fn parse_speech(status: u16, body: &[u8]) -> Result<Vec<u8>, AiError> {
    if body.len() > MAX_SPEECH_BODY {
        return Err(AiError::BodyTooLarge);
    }
    match status {
        401 | 403 => return Err(AiError::RejectedKey),
        404 => return Err(AiError::NoSpeechEndpoint),
        429 => return Err(AiError::RateLimited),
        status if !(200..=299).contains(&status) => {
            return Err(AiError::HttpStatus {
                status,
                excerpt: excerpt(body),
            });
        }
        _ => {}
    }
    if !looks_like_mp3(body) {
        return Err(AiError::NoAudio);
    }
    Ok(body.to_vec())
}

fn looks_like_mp3(body: &[u8]) -> bool {
    body.starts_with(b"ID3")
        || body.first().is_some_and(|first| *first == 0xFF)
            && body.get(1).is_some_and(|second| second & 0xE0 == 0xE0)
}

fn boundary_candidate(attempt: u32) -> String {
    format!("----CourseCraftBoundary{attempt:08x}")
}

fn pick_boundary(bytes: &[u8]) -> Option<String> {
    (0..MAX_BOUNDARY_ATTEMPTS).find_map(|attempt| {
        let candidate = boundary_candidate(attempt);
        (!contains_bytes(bytes, candidate.as_bytes())).then_some(candidate)
    })
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || needle.len() > haystack.len() {
        return false;
    }
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

/// Keep only ASCII letters, digits, `.`, `-` and `_`, since the name goes in a
/// multipart header line. An empty or dot-only result becomes `audio`.
fn safe_file_name(name: &str) -> String {
    let last = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let cleaned: String = last
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        .take(120)
        .collect();
    let trimmed = cleaned.trim_matches('.');
    if trimmed.is_empty() {
        "audio".to_string()
    } else {
        trimmed.to_string()
    }
}

fn audio_mime(file_name: &str) -> &'static str {
    let extension = std::path::Path::new(file_name)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" | "oga" => "audio/ogg",
        "m4a" => "audio/mp4",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        _ => "application/octet-stream",
    }
}

/// The endpoint's host, without a port or credentials, for the confirmation
/// line shown before a request. `None` when the URL has no host.
#[must_use]
pub fn endpoint_host(config: &AiConfig) -> Option<String> {
    host_of(config.base_url.trim())
}

/// True when the endpoint is on this computer (`localhost`, `127.0.0.1` or
/// `[::1]`), so the author can be told the text stays on the machine.
#[must_use]
pub fn endpoint_is_local(config: &AiConfig) -> bool {
    host_of(config.base_url.trim()).is_some_and(|host| is_local_host(&host))
}

/// Remove one Markdown code fence around model output, if present.
///
/// Text without a fence is returned trimmed but otherwise unchanged. The
/// returned slice borrows from `text`.
#[must_use]
pub fn strip_code_fence(text: &str) -> &str {
    let trimmed = text.trim();
    let Some(rest) = trimmed.strip_prefix("```") else {
        return trimmed;
    };
    let inner = match rest.find('\n') {
        Some(newline) => rest.get(newline + 1..).unwrap_or(""),
        None => "",
    };
    let inner = inner.trim_end();
    match inner.strip_suffix("```") {
        Some(without) => without.trim(),
        None => inner.trim(),
    }
}

/// Take the authority from a URL and return its host (with brackets for IPv6).
fn host_of(url: &str) -> Option<String> {
    let (_scheme, rest) = url.split_once("://")?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if authority.is_empty() {
        return None;
    }
    let after_user = match authority.rsplit_once('@') {
        Some((_user, host)) => host,
        None => authority,
    };
    if after_user.is_empty() {
        return None;
    }
    if after_user.starts_with('[') {
        let end = after_user.find(']')?;
        let host = after_user.get(..=end)?;
        return (!host.is_empty()).then(|| host.to_string());
    }
    let host = after_user
        .split_once(':')
        .map_or(after_user, |(host, _port)| host);
    (!host.is_empty()).then(|| host.to_string())
}

fn is_local_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host == "127.0.0.1"
        || host == "[::1]"
        || host == "::1"
}

/// At most [`MAX_EXCERPT`] characters of a body, with controls made visible.
fn excerpt(body: &[u8]) -> String {
    let text = String::from_utf8_lossy(body);
    let mut out: String = text
        .chars()
        .map(|c| if c.is_control() && c != '\n' { ' ' } else { c })
        .take(MAX_EXCERPT)
        .collect();
    out = out.split_whitespace().collect::<Vec<_>>().join(" ");
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    fn config() -> AiConfig {
        AiConfig {
            base_url: "https://api.example.com/v1".to_string(),
            model: "test-model".to_string(),
            api_key: "sk-secret-value".to_string(),
            speech_to_text_model: "whisper-1".to_string(),
            text_to_speech_model: "tts-1".to_string(),
            voice: "alloy".to_string(),
        }
    }

    fn audio_config() -> AiConfig {
        AiConfig {
            model: String::new(),
            ..config()
        }
    }

    fn user_request(text: &str) -> ChatRequest {
        ChatRequest {
            messages: vec![ChatMessage {
                role: Role::User,
                content: MessageContent::Text(text.to_string()),
            }],
            max_tokens: Some(8),
            temperature: None,
        }
    }

    #[test]
    fn accepts_https_and_local_http() {
        assert!(check_config(&config()).is_ok());
        for url in [
            "http://localhost:11434/v1",
            "http://127.0.0.1:1234/v1",
            "http://[::1]:8080",
            "https://api.deepseek.com",
            "HTTPS://API.EXAMPLE.COM/v1",
        ] {
            let mut c = config();
            c.base_url = url.to_string();
            assert!(check_config(&c).is_ok(), "{url} should be accepted");
        }
    }

    #[test]
    fn refuses_plain_http_to_another_machine() {
        let mut c = config();
        c.base_url = "http://api.example.com/v1".to_string();
        let error = check_config(&c).expect_err("must refuse remote http");
        assert!(error.to_string().contains("https://"), "{error}");
        assert!(!error.to_string().contains(&c.api_key));
    }

    #[test]
    fn refuses_a_missing_or_unsupported_scheme() {
        for url in ["", "api.example.com", "ftp://api.example.com", "https:/api"] {
            let mut c = config();
            c.base_url = url.to_string();
            assert!(check_config(&c).is_err(), "{url:?} should be refused");
        }
    }

    #[test]
    fn refuses_an_empty_model() {
        let mut c = config();
        c.model = "   ".to_string();
        assert_eq!(check_config(&c).unwrap_err(), AiError::EmptyModel);
    }

    #[test]
    fn refuses_header_injection_in_key_or_model() {
        let mut c = config();
        c.api_key = "abc\r\nX-Injected: yes".to_string();
        let error = check_config(&c).expect_err("newline in key");
        assert!(matches!(error, AiError::HeaderInjection(_)));
        assert!(!error.to_string().contains("abc"));

        let mut c = config();
        c.model = "model\nX-Injected: yes".to_string();
        assert!(matches!(
            check_config(&c).unwrap_err(),
            AiError::HeaderInjection(_)
        ));
    }

    #[test]
    fn accepts_an_empty_key_and_omits_authorization() {
        let mut c = config();
        c.api_key.clear();
        let call = build_chat_call(&c, &user_request("hi")).expect("call");
        assert!(
            !call
                .headers
                .iter()
                .any(|(name, _)| name.eq_ignore_ascii_case("Authorization"))
        );
    }

    #[test]
    fn call_posts_to_chat_completions_and_trims_one_trailing_slash() {
        let mut c = config();
        c.base_url = "https://api.example.com/v1/".to_string();
        let call = build_chat_call(&c, &user_request("hi")).expect("call");
        assert_eq!(call.url, "https://api.example.com/v1/chat/completions");
        assert!(
            call.headers.iter().any(|(name, value)| {
                name == "Authorization" && value == "Bearer sk-secret-value"
            })
        );
        assert_eq!(
            call.headers
                .iter()
                .find(|(name, _)| name == "Content-Type")
                .map(|(_, value)| value.as_str()),
            Some("application/json")
        );
    }

    #[test]
    fn does_not_add_v1() {
        let mut c = config();
        c.base_url = "https://api.deepseek.com".to_string();
        let call = build_chat_call(&c, &user_request("hi")).expect("call");
        assert_eq!(call.url, "https://api.deepseek.com/chat/completions");
    }

    #[test]
    fn body_carries_model_messages_limits_and_temperature() {
        let c = config();
        let request = ChatRequest {
            messages: vec![
                ChatMessage {
                    role: Role::System,
                    content: MessageContent::Text("be brief".to_string()),
                },
                ChatMessage {
                    role: Role::User,
                    content: MessageContent::TextAndImages {
                        text: "what is this?".to_string(),
                        images: vec!["data:image/png;base64,AAAA".to_string()],
                    },
                },
            ],
            max_tokens: Some(16),
            temperature: Some(0.2),
        };
        let call = build_chat_call(&c, &request).expect("call");
        let value: Value = serde_json::from_slice(&call.body).expect("json");
        assert_eq!(value["model"], "test-model");
        assert_eq!(value["max_tokens"], 16);
        assert!((value["temperature"].as_f64().expect("number") - 0.2).abs() < 1e-6);
        assert_eq!(value["messages"][0]["role"], "system");
        assert_eq!(value["messages"][0]["content"], "be brief");
        assert_eq!(value["messages"][1]["content"][0]["type"], "text");
        assert_eq!(
            value["messages"][1]["content"][1]["image_url"]["url"],
            "data:image/png;base64,AAAA"
        );
    }

    #[test]
    fn omits_absent_options() {
        let c = config();
        let request = ChatRequest {
            messages: vec![ChatMessage {
                role: Role::User,
                content: MessageContent::Text("hi".to_string()),
            }],
            max_tokens: None,
            temperature: None,
        };
        let call = build_chat_call(&c, &request).expect("call");
        let value: Value = serde_json::from_slice(&call.body).expect("json");
        assert!(value.get("max_tokens").is_none());
        assert!(value.get("temperature").is_none());
    }

    #[test]
    fn refuses_a_non_finite_temperature() {
        let c = config();
        let mut request = user_request("hi");
        request.temperature = Some(f32::NAN);
        assert!(matches!(
            build_chat_call(&c, &request).unwrap_err(),
            AiError::InvalidRequest(_)
        ));
        request.temperature = Some(f32::INFINITY);
        assert!(build_chat_call(&c, &request).is_err());
    }

    #[test]
    fn maps_status_codes_to_plain_sentences() {
        assert_eq!(
            parse_chat_response(401, b"").unwrap_err(),
            AiError::RejectedKey
        );
        assert!(
            parse_chat_response(403, b"")
                .unwrap_err()
                .to_string()
                .contains("rejected the key")
        );
        assert!(
            parse_chat_response(404, b"")
                .unwrap_err()
                .to_string()
                .contains("base URL and model")
        );
        assert!(
            parse_chat_response(429, b"")
                .unwrap_err()
                .to_string()
                .contains("busy")
        );
        let error = parse_chat_response(418, b"short teapot").unwrap_err();
        assert!(error.to_string().contains("418"), "{error}");
        assert!(error.to_string().contains("teapot"), "{error}");
    }

    #[test]
    fn status_errors_never_quote_more_than_200_characters() {
        let body = "x".repeat(5000);
        let error = parse_chat_response(500, body.as_bytes()).unwrap_err();
        let text = error.to_string();
        assert!(text.chars().count() < MAX_EXCERPT + 100, "{}", text.len());
        assert!(!text.contains(&body));
    }

    #[test]
    fn reads_a_reply_and_marks_truncation() {
        let body = br#"{"choices":[{"message":{"role":"assistant","content":"OK"},"finish_reason":"stop"}]}"#;
        let reply = parse_chat_response(200, body).expect("reply");
        assert_eq!(reply.text, "OK");
        assert!(!reply.truncated);

        let body = br#"{"choices":[{"message":{"content":"half"},"finish_reason":"length"}]}"#;
        let reply = parse_chat_response(200, body).expect("reply");
        assert_eq!(reply.text, "half");
        assert!(reply.truncated);
    }

    #[test]
    fn refuses_an_empty_answer() {
        let body = br#"{"choices":[{"message":{"content":"   "}}]}"#;
        assert_eq!(
            parse_chat_response(200, body).unwrap_err(),
            AiError::EmptyAnswer
        );
    }

    #[test]
    fn refuses_hostile_bodies_without_panicking() {
        let cases: &[&[u8]] = &[
            b"",
            b"{}",
            b"[]",
            br#"{"choices":[]}"#,
            br#"{"choices":[{"message":{"content":123}}]}"#,
            br#"{"choices":"nope"}"#,
            br#"{"choices":[{"message":null}]}"#,
            b"\xff\xfe\xfd",
            b"not json at all",
        ];
        for body in cases {
            let result = parse_chat_response(200, body);
            assert!(result.is_err(), "{:?} should not parse", body);
        }

        // A wrong-typed finish_reason is not fatal: the text is still usable.
        let body = br#"{"choices":[{"message":{"content":"x"},"finish_reason":5}]}"#;
        let reply = parse_chat_response(200, body).expect("content is valid");
        assert!(!reply.truncated);
    }

    #[test]
    fn refuses_deeply_nested_json_without_panicking() {
        let mut body = String::from("{\"choices\":");
        body.push_str(&"[".repeat(10_000));
        body.push_str(&"]".repeat(10_000));
        body.push('}');
        assert!(parse_chat_response(200, body.as_bytes()).is_err());
    }

    #[test]
    fn refuses_a_body_over_16_mib() {
        let body = vec![b' '; MAX_BODY + 1];
        assert_eq!(
            parse_chat_response(200, &body).unwrap_err(),
            AiError::BodyTooLarge
        );
    }

    #[test]
    fn debug_hides_the_key() {
        let c = config();
        let printed = format!("{c:?}");
        assert!(!printed.contains("sk-secret-value"), "{printed}");
        assert!(printed.contains("***"), "{printed}");
    }

    #[test]
    fn no_error_contains_the_key() {
        let mut c = config();
        c.base_url = "http://api.example.com/v1".to_string();
        let print = |error: AiError| error.to_string();

        let errors = [
            print(check_config(&c).unwrap_err()),
            print(build_chat_call(&c, &user_request("hi")).unwrap_err()),
        ];
        for error in errors {
            assert!(!error.contains(&c.api_key), "{error}");
        }

        let mut c = config();
        c.model = String::new();
        assert!(!print(check_config(&c).unwrap_err()).contains(&c.api_key));

        let mut c = config();
        c.api_key = "bad\nkey".to_string();
        assert!(!print(check_config(&c).unwrap_err()).contains(&c.api_key));
    }

    #[test]
    fn endpoint_host_strips_scheme_port_and_credentials() {
        let mut c = config();
        c.base_url = "https://user:pass@api.example.com:8443/v1".to_string();
        assert_eq!(endpoint_host(&c).as_deref(), Some("api.example.com"));

        c.base_url = "http://localhost:11434/v1".to_string();
        assert_eq!(endpoint_host(&c).as_deref(), Some("localhost"));

        c.base_url = "http://[::1]:8080".to_string();
        assert_eq!(endpoint_host(&c).as_deref(), Some("[::1]"));

        c.base_url = String::new();
        assert_eq!(endpoint_host(&c), None);
    }

    #[test]
    fn strips_one_code_fence() {
        assert_eq!(strip_code_fence("```json\n{\"a\":1}\n```"), "{\"a\":1}");
        assert_eq!(strip_code_fence("```\nhello\n```"), "hello");
        assert_eq!(strip_code_fence("  plain text  "), "plain text");
        assert_eq!(strip_code_fence("```json\n{\"a\":1}"), "{\"a\":1}");
        assert_eq!(strip_code_fence("```"), "");
        assert_eq!(strip_code_fence(""), "");
        assert_eq!(
            strip_code_fence("text with ``` inside"),
            "text with ``` inside"
        );
    }

    const VTT: &[u8] = b"WEBVTT\n\n00:00:00.000 --> 00:00:02.000\nHello\n";

    #[test]
    fn transcription_is_an_exact_multipart_post() {
        let call = build_transcription_call(&audio_config(), "whisper-1", "talk.mp3", b"ABC")
            .expect("call");
        assert_eq!(call.url, "https://api.example.com/v1/audio/transcriptions");
        assert!(
            call.headers.iter().any(|(name, value)| {
                name == "Authorization" && value == "Bearer sk-secret-value"
            })
        );
        assert_eq!(
            call.headers
                .iter()
                .find(|(name, _)| name == "Content-Type")
                .map(|(_, value)| value.as_str()),
            Some("multipart/form-data; boundary=----CourseCraftBoundary00000000")
        );
        let expected = concat!(
            "------CourseCraftBoundary00000000\r\n",
            "Content-Disposition: form-data; name=\"file\"; filename=\"talk.mp3\"\r\n",
            "Content-Type: audio/mpeg\r\n\r\n",
            "ABC\r\n",
            "------CourseCraftBoundary00000000\r\n",
            "Content-Disposition: form-data; name=\"model\"\r\n\r\n",
            "whisper-1\r\n",
            "------CourseCraftBoundary00000000\r\n",
            "Content-Disposition: form-data; name=\"response_format\"\r\n\r\n",
            "vtt\r\n",
            "------CourseCraftBoundary00000000--\r\n",
        );
        assert_eq!(call.body, expected.as_bytes());
    }

    #[test]
    fn a_boundary_that_occurs_in_the_file_is_skipped() {
        let mut bytes = b"some audio ".to_vec();
        bytes.extend_from_slice(boundary_candidate(0).as_bytes());
        bytes.extend_from_slice(b" more");
        let call =
            build_transcription_call(&audio_config(), "whisper-1", "a.mp3", &bytes).expect("call");
        assert_eq!(
            call.headers
                .iter()
                .find(|(name, _)| name == "Content-Type")
                .map(|(_, value)| value.as_str()),
            Some("multipart/form-data; boundary=----CourseCraftBoundary00000001")
        );
        let text = String::from_utf8_lossy(&call.body);
        assert!(
            !text.contains("------CourseCraftBoundary00000000\r\nContent-Disposition"),
            "{text}"
        );
    }

    #[test]
    fn unsafe_file_names_are_reduced_to_safe_ascii() {
        assert_eq!(safe_file_name("../../se cret;.mp3"), "secret.mp3");
        assert_eq!(safe_file_name("résumé.wav"), "rsum.wav");
        assert_eq!(safe_file_name("///"), "audio");
        assert_eq!(safe_file_name("..."), "audio");
        let call = build_transcription_call(&audio_config(), "whisper-1", "a\r\nb.mp3", b"x")
            .expect("call");
        let text = String::from_utf8_lossy(&call.body);
        assert!(text.contains("filename=\"ab.mp3\""), "{text}");
        assert!(!text.contains("a\r\nb.mp3"), "{text}");
    }

    #[test]
    fn transcription_refuses_an_oversized_file() {
        let bytes = vec![0_u8; MAX_AUDIO_UPLOAD + 1];
        let error =
            build_transcription_call(&audio_config(), "whisper-1", "a.mp3", &bytes).unwrap_err();
        assert!(error.to_string().contains("25 MB"), "{error}");
    }

    #[test]
    fn audio_calls_do_not_need_a_chat_model() {
        assert!(check_config(&audio_config()).is_err());
        assert!(check_audio_config(&audio_config(), "whisper-1").is_ok());
        assert_eq!(
            check_audio_config(&audio_config(), "  ").unwrap_err(),
            AiError::EmptyModel
        );
    }

    #[test]
    fn transcription_accepts_only_timed_webvtt() {
        let text = parse_transcription(200, VTT).expect("captions");
        assert!(text.starts_with("WEBVTT"));
        for body in [
            b"".as_slice(),
            b"WEBVTT\n",
            b"plain words with no timing",
            br#"{"text":"not captions"}"#,
            b"00:00:01.000 --> 00:00:02.000\nNo header\n",
            b"\xff\xfe\xfd",
        ] {
            assert_eq!(
                parse_transcription(200, body).unwrap_err(),
                AiError::NoTimedCaptions,
                "{:?}",
                String::from_utf8_lossy(body)
            );
        }
    }

    #[test]
    fn transcription_maps_statuses_and_limits() {
        assert_eq!(
            parse_transcription(404, b"").unwrap_err(),
            AiError::NoTranscriptionEndpoint
        );
        assert_eq!(
            parse_transcription(401, b"").unwrap_err(),
            AiError::RejectedKey
        );
        assert_eq!(
            parse_transcription(429, b"").unwrap_err(),
            AiError::RateLimited
        );
        let error = parse_transcription(500, b"server broke").unwrap_err();
        assert!(error.to_string().contains("500"), "{error}");
        let body = vec![b' '; MAX_BODY + 1];
        assert_eq!(
            parse_transcription(200, &body).unwrap_err(),
            AiError::BodyTooLarge
        );
        assert!(
            parse_transcription(404, b"")
                .unwrap_err()
                .to_string()
                .contains("speech-to-text")
        );
    }

    #[test]
    fn speech_is_a_small_json_post() {
        let call =
            build_speech_call(&audio_config(), "tts-1", "alloy", "Hello there.").expect("call");
        assert_eq!(call.url, "https://api.example.com/v1/audio/speech");
        let value: Value = serde_json::from_slice(&call.body).expect("json");
        assert_eq!(value["model"], "tts-1");
        assert_eq!(value["voice"], "alloy");
        assert_eq!(value["input"], "Hello there.");
        assert_eq!(value["response_format"], "mp3");

        let call = build_speech_call(&audio_config(), "tts-1", "  ", "Hi").expect("default voice");
        let value: Value = serde_json::from_slice(&call.body).expect("json");
        assert_eq!(value["voice"], "alloy");
    }

    #[test]
    fn speech_refuses_empty_and_oversized_text() {
        let error = build_speech_call(&audio_config(), "tts-1", "alloy", "  ").unwrap_err();
        assert!(error.to_string().contains("Enter some text"), "{error}");
        let text = "a".repeat(MAX_SPEECH_CHARS + 1);
        let error = build_speech_call(&audio_config(), "tts-1", "alloy", &text).unwrap_err();
        assert!(error.to_string().contains("4000"), "{error}");
        let text = "a".repeat(MAX_SPEECH_CHARS);
        assert!(build_speech_call(&audio_config(), "tts-1", "alloy", &text).is_ok());
        let emoji = "🙂".repeat(MAX_SPEECH_CHARS + 1);
        assert!(build_speech_call(&audio_config(), "tts-1", "alloy", &emoji).is_err());
    }

    #[test]
    fn speech_accepts_only_audio_and_maps_statuses() {
        assert_eq!(
            parse_speech(200, b"ID3\x04\x00\x00\x00\x00\x00\x00").expect("id3"),
            b"ID3\x04\x00\x00\x00\x00\x00\x00"
        );
        assert_eq!(
            parse_speech(200, &[0xFF, 0xFB, 0x90, 0x64]).expect("frame sync"),
            vec![0xFF, 0xFB, 0x90, 0x64]
        );
        for body in [
            b"".as_slice(),
            b"RIFF....WAVE",
            b"{\"audio\":1}",
            &[0xFF, 0x00, 0x00],
        ] {
            assert_eq!(parse_speech(200, body).unwrap_err(), AiError::NoAudio);
        }
        assert_eq!(
            parse_speech(404, b"").unwrap_err(),
            AiError::NoSpeechEndpoint
        );
        assert_eq!(parse_speech(403, b"").unwrap_err(), AiError::RejectedKey);
        let body = vec![0_u8; MAX_SPEECH_BODY + 1];
        assert_eq!(parse_speech(200, &body).unwrap_err(), AiError::BodyTooLarge);
        assert!(
            parse_speech(404, b"")
                .unwrap_err()
                .to_string()
                .contains("text-to-speech")
        );
    }

    #[test]
    fn an_audio_error_never_contains_the_key() {
        let c = audio_config();
        let errors = [
            check_audio_config(&c, "").unwrap_err().to_string(),
            build_transcription_call(&c, "whisper-1", "a.mp3", b"x")
                .map(|_| String::new())
                .unwrap_or_default(),
            parse_transcription(500, b"bad").unwrap_err().to_string(),
            parse_speech(500, b"bad").unwrap_err().to_string(),
        ];
        for error in errors {
            assert!(!error.contains(&c.api_key), "{error}");
        }
    }
}
