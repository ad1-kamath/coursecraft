//! Background AI calls for the editor app, on both desktop and web.
//!
//! The pure request/response work lives in `coursecraft-ai`. This module owns
//! the one in-flight call: it builds the HTTP call, hands it to a transport,
//! delivers the answer through a channel, and hands a late answer to nobody.
//! Tests use [`FakeTransport`]; no test opens a socket.

use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::{Arc, mpsc};

#[cfg(not(target_arch = "wasm32"))]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(not(target_arch = "wasm32"))]
use coursecraft_ai::AiSource;
use coursecraft_ai::{
    AiConfig, AiReply, ChatRequest, ElevenLabsConfig, HttpCall, HttpMethod, build_chat_call,
    build_elevenlabs_speech_call_with_limit, build_models_call, build_speech_call,
    build_transcription_call, build_voice_call, build_voices_call, parse_chat_response,
    parse_elevenlabs_models, parse_elevenlabs_speech, parse_speech, parse_transcription,
    parse_voice, parse_voices,
};
use coursecraft_ui_egui::AiPurpose;

/// How long a call may run before it is abandoned.
pub const TIMEOUT_SECONDS: f64 = 120.0;

/// ElevenLabs can take much longer to synthesise a long narration.
pub const ELEVENLABS_SPEECH_TIMEOUT_SECONDS: f64 = 300.0;

/// An installed agent gets the plan's ten minutes before it is killed.
#[cfg(not(target_arch = "wasm32"))]
pub const AGENT_TIMEOUT_SECONDS: f64 = 600.0;

/// What a transport reports back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchOutcome {
    /// The server answered, whatever the status.
    Response {
        /// HTTP status code.
        status: u16,
        /// Response body.
        body: Vec<u8>,
    },
    /// The request never reached the server, or the browser blocked it.
    Failed {
        /// Transport detail; never shown to the author raw.
        message: String,
    },
}

/// The small transport seam that lets app tests run without a network.
pub trait AiTransport: Send {
    /// Send `call`, calling `done` once with the outcome.
    fn send(&self, call: HttpCall, done: Box<dyn FnOnce(FetchOutcome) + Send>);
}

/// The real transport: `ehttp`, which works on the desktop and in the browser.
#[derive(Debug, Clone, Copy, Default)]
pub struct EhttpTransport;

impl AiTransport for EhttpTransport {
    fn send(&self, call: HttpCall, done: Box<dyn FnOnce(FetchOutcome) + Send>) {
        let mut headers = ehttp::Headers::default();
        for (name, value) in call.headers {
            headers.insert(name, value);
        }
        let method = match call.method {
            HttpMethod::Post => ehttp::Method::POST,
            HttpMethod::Get => ehttp::Method::GET,
        };
        let mut request = ehttp::Request::new(method, call.url, headers);
        request.body = call.body;
        ehttp::fetch(request, move |result| {
            let outcome = match result {
                Ok(response) => FetchOutcome::Response {
                    status: response.status,
                    body: response.bytes,
                },
                Err(error) => FetchOutcome::Failed {
                    message: error.to_string(),
                },
            };
            done(outcome);
        });
    }
}

/// Which parser an in-flight call needs. It is fixed when the call starts, so
/// the purpose alone cannot be used later (speech has two sources).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CallKind {
    Chat,
    Captions,
    Speech,
    ElevenLabsSpeech,
    ElevenLabsVoices,
    ElevenLabsVoice,
    ElevenLabsModels,
    #[cfg(not(target_arch = "wasm32"))]
    AgentChat,
}

enum PendingReply {
    Http(Receiver<FetchOutcome>),
    #[cfg(not(target_arch = "wasm32"))]
    Agent(Receiver<Result<coursecraft_ai::ChatReply, String>>),
}

struct Pending {
    purpose: AiPurpose,
    kind: CallKind,
    started: f64,
    timeout: f64,
    reply: PendingReply,
    #[cfg(not(target_arch = "wasm32"))]
    cancel: Option<Arc<AtomicBool>>,
}

/// One AI call at a time, polled once a frame by the app.
pub struct AiClient {
    transport: Box<dyn AiTransport>,
    pending: Option<Pending>,
}

impl Default for AiClient {
    fn default() -> Self {
        Self::new()
    }
}

impl AiClient {
    /// A client that uses the real `ehttp` transport.
    #[must_use]
    pub fn new() -> Self {
        Self::with_transport(Box::new(EhttpTransport))
    }

    /// A client with a custom transport (used by tests).
    #[must_use]
    pub fn with_transport(transport: Box<dyn AiTransport>) -> Self {
        Self {
            transport,
            pending: None,
        }
    }

    /// True while a call is in flight.
    #[must_use]
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// Start a call. A second call while one is running is refused.
    ///
    /// `now` is egui's frame time in seconds. `repaint` wakes the window when
    /// the answer arrives.
    ///
    /// # Errors
    /// Returns a plain sentence when the configuration is unusable or another
    /// call is already in flight. Nothing is sent in that case.
    pub fn begin(
        &mut self,
        purpose: AiPurpose,
        config: &AiConfig,
        request: &ChatRequest,
        now: f64,
        repaint: impl Fn() + Send + Sync + 'static,
    ) -> Result<(), String> {
        self.ensure_idle()?;
        let call = build_chat_call(config, request).map_err(|error| error.to_string())?;
        self.start(purpose, CallKind::Chat, call, TIMEOUT_SECONDS, now, repaint);
        Ok(())
    }

    /// Start a speech-to-text call with the file's raw bytes.
    ///
    /// # Errors
    /// Returns a plain sentence when the configuration is unusable, the file is
    /// over the limit, or another call is already in flight.
    #[allow(clippy::too_many_arguments)]
    pub fn begin_transcription(
        &mut self,
        purpose: AiPurpose,
        config: &AiConfig,
        model: &str,
        file_name: &str,
        bytes: &[u8],
        now: f64,
        repaint: impl Fn() + Send + Sync + 'static,
    ) -> Result<(), String> {
        self.ensure_idle()?;
        let call = build_transcription_call(config, model, file_name, bytes)
            .map_err(|error| error.to_string())?;
        self.start(
            purpose,
            CallKind::Captions,
            call,
            TIMEOUT_SECONDS,
            now,
            repaint,
        );
        Ok(())
    }

    /// Start a text-to-speech call.
    ///
    /// # Errors
    /// Returns a plain sentence when the configuration is unusable, the text is
    /// empty or over the limit, or another call is already in flight.
    #[allow(clippy::too_many_arguments)]
    pub fn begin_speech(
        &mut self,
        purpose: AiPurpose,
        config: &AiConfig,
        model: &str,
        voice: &str,
        text: &str,
        now: f64,
        repaint: impl Fn() + Send + Sync + 'static,
    ) -> Result<(), String> {
        self.ensure_idle()?;
        let call =
            build_speech_call(config, model, voice, text).map_err(|error| error.to_string())?;
        self.start(
            purpose,
            CallKind::Speech,
            call,
            TIMEOUT_SECONDS,
            now,
            repaint,
        );
        Ok(())
    }

    /// Start an ElevenLabs text-to-speech call.
    ///
    /// # Errors
    /// Returns a plain sentence when the key, voice or text is unusable, or
    /// another call is already in flight.
    pub fn begin_elevenlabs_speech(
        &mut self,
        purpose: AiPurpose,
        config: &ElevenLabsConfig,
        text: &str,
        max_chars: usize,
        now: f64,
        repaint: impl Fn() + Send + Sync + 'static,
    ) -> Result<(), String> {
        self.ensure_idle()?;
        let call = build_elevenlabs_speech_call_with_limit(config, text, max_chars)
            .map_err(|error| error.to_string())?;
        self.start(
            purpose,
            CallKind::ElevenLabsSpeech,
            call,
            ELEVENLABS_SPEECH_TIMEOUT_SECONDS,
            now,
            repaint,
        );
        Ok(())
    }

    /// Start an ElevenLabs voice-list call.
    ///
    /// # Errors
    /// Returns a plain sentence when the key is unusable, or another call is
    /// already in flight.
    pub fn begin_elevenlabs_voices(
        &mut self,
        purpose: AiPurpose,
        config: &ElevenLabsConfig,
        page_token: Option<&str>,
        now: f64,
        repaint: impl Fn() + Send + Sync + 'static,
    ) -> Result<(), String> {
        self.ensure_idle()?;
        let call = build_voices_call(config, page_token).map_err(|error| error.to_string())?;
        self.start(
            purpose,
            CallKind::ElevenLabsVoices,
            call,
            TIMEOUT_SECONDS,
            now,
            repaint,
        );
        Ok(())
    }

    /// Start an ElevenLabs voice lookup.
    ///
    /// # Errors
    /// Returns a plain sentence when the key or voice id is unusable, or
    /// another call is already in flight.
    pub fn begin_elevenlabs_voice(
        &mut self,
        purpose: AiPurpose,
        config: &ElevenLabsConfig,
        voice_id: &str,
        now: f64,
        repaint: impl Fn() + Send + Sync + 'static,
    ) -> Result<(), String> {
        self.ensure_idle()?;
        let call = build_voice_call(config, voice_id).map_err(|error| error.to_string())?;
        self.start(
            purpose,
            CallKind::ElevenLabsVoice,
            call,
            TIMEOUT_SECONDS,
            now,
            repaint,
        );
        Ok(())
    }

    /// Start an ElevenLabs model-list call.
    ///
    /// # Errors
    /// Returns a plain sentence when the key is unusable, or another call is
    /// already in flight.
    pub fn begin_elevenlabs_models(
        &mut self,
        purpose: AiPurpose,
        config: &ElevenLabsConfig,
        now: f64,
        repaint: impl Fn() + Send + Sync + 'static,
    ) -> Result<(), String> {
        self.ensure_idle()?;
        let call = build_models_call(config).map_err(|error| error.to_string())?;
        self.start(
            purpose,
            CallKind::ElevenLabsModels,
            call,
            TIMEOUT_SECONDS,
            now,
            repaint,
        );
        Ok(())
    }

    fn ensure_idle(&self) -> Result<(), String> {
        if self.pending.is_some() {
            return Err(
                "An AI request is already running. Wait for it to finish or cancel it.".to_string(),
            );
        }
        Ok(())
    }

    fn start(
        &mut self,
        purpose: AiPurpose,
        kind: CallKind,
        call: HttpCall,
        timeout: f64,
        now: f64,
        repaint: impl Fn() + Send + Sync + 'static,
    ) {
        let (tx, rx) = mpsc::channel();
        let repaint: Arc<dyn Fn() + Send + Sync> = Arc::new(repaint);
        self.transport.send(
            call,
            Box::new(move |outcome| {
                let _ = tx.send(outcome);
                repaint();
            }),
        );
        self.pending = Some(Pending {
            purpose,
            kind,
            started: now,
            timeout,
            reply: PendingReply::Http(rx),
            #[cfg(not(target_arch = "wasm32"))]
            cancel: None,
        });
    }

    /// Start one chat request through an installed Claude Code or Codex.
    ///
    /// The answer uses the same poll path as an HTTP call, so every chat
    /// feature is unchanged. Cancelling or timing out kills the process.
    ///
    /// # Errors
    /// Returns a plain sentence when an endpoint source is given or another
    /// call is already in flight.
    #[cfg(not(target_arch = "wasm32"))]
    #[allow(clippy::too_many_arguments)]
    pub fn begin_agent(
        &mut self,
        purpose: AiPurpose,
        source: AiSource,
        request: &ChatRequest,
        now: f64,
        host: Arc<dyn crate::agents::AgentHost>,
        env: crate::agents::AgentEnv,
        repaint: impl Fn() + Send + Sync + 'static,
    ) -> Result<(), String> {
        self.ensure_idle()?;
        if !source.is_agent() {
            return Err("Choose Claude Code or Codex, or use an API endpoint.".to_string());
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let thread_cancel = Arc::clone(&cancel);
        let request = request.clone();
        let (tx, rx) = mpsc::channel();
        let repaint: Arc<dyn Fn() + Send + Sync> = Arc::new(repaint);
        std::thread::spawn(move || {
            let result = crate::agents::run_chat(
                host.as_ref(),
                &env,
                source,
                &request,
                thread_cancel.as_ref(),
            );
            let _ = tx.send(result);
            repaint();
        });
        self.pending = Some(Pending {
            purpose,
            kind: CallKind::AgentChat,
            started: now,
            timeout: AGENT_TIMEOUT_SECONDS,
            reply: PendingReply::Agent(rx),
            cancel: Some(cancel),
        });
        Ok(())
    }

    /// Poll for an answer. Returns it once, or `None` while waiting.
    ///
    /// A call that ran longer than its own timeout is dropped, so a late answer
    /// is ignored. ElevenLabs speech gets a longer one.
    pub fn poll(&mut self, now: f64) -> Option<(AiPurpose, Result<AiReply, String>)> {
        let pending = self.pending.take()?;
        if now - pending.started >= pending.timeout {
            #[cfg(not(target_arch = "wasm32"))]
            if let Some(cancel) = &pending.cancel {
                cancel.store(true, Ordering::Relaxed);
            }
            return Some((
                pending.purpose,
                Err("The AI request took too long and was stopped. Try again.".to_string()),
            ));
        }
        let outcome = match &pending.reply {
            PendingReply::Http(rx) => rx
                .try_recv()
                .map(|outcome| outcome_to_result(outcome, pending.kind)),
            #[cfg(not(target_arch = "wasm32"))]
            PendingReply::Agent(rx) => rx.try_recv().map(|result| result.map(AiReply::Chat)),
        };
        match outcome {
            Ok(result) => Some((pending.purpose, result)),
            Err(TryRecvError::Empty) => {
                self.pending = Some(pending);
                None
            }
            Err(TryRecvError::Disconnected) => Some((
                pending.purpose,
                Err("The AI request stopped unexpectedly. Try again.".to_string()),
            )),
        }
    }

    /// Drop the in-flight call. Its late answer is ignored, and an agent
    /// process is killed.
    pub fn cancel(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(pending) = self.pending.take()
            && let Some(cancel) = pending.cancel
        {
            cancel.store(true, Ordering::Relaxed);
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.pending = None;
        }
    }
}

fn outcome_to_result(outcome: FetchOutcome, kind: CallKind) -> Result<AiReply, String> {
    match outcome {
        FetchOutcome::Response { status, body } => match kind {
            CallKind::Captions => parse_transcription(status, &body)
                .map(AiReply::Captions)
                .map_err(|error| error.to_string()),
            CallKind::Speech => parse_speech(status, &body)
                .map(AiReply::Speech)
                .map_err(|error| error.to_string()),
            CallKind::ElevenLabsSpeech => parse_elevenlabs_speech(status, &body)
                .map(AiReply::Speech)
                .map_err(|error| error.to_string()),
            CallKind::ElevenLabsVoices => parse_voices(status, &body)
                .map(AiReply::ElevenLabsVoices)
                .map_err(|error| error.to_string()),
            CallKind::ElevenLabsVoice => parse_voice(status, &body)
                .map(AiReply::ElevenLabsVoice)
                .map_err(|error| error.to_string()),
            CallKind::ElevenLabsModels => parse_elevenlabs_models(status, &body)
                .map(AiReply::ElevenLabsModels)
                .map_err(|error| error.to_string()),
            CallKind::Chat => parse_chat_response(status, &body)
                .map(AiReply::Chat)
                .map_err(|error| error.to_string()),
            #[cfg(not(target_arch = "wasm32"))]
            CallKind::AgentChat => {
                Err("An agent answer arrived through the wrong channel.".to_string())
            }
        },
        FetchOutcome::Failed { message } => Err(failed_message(&message)),
    }
}

#[cfg(target_arch = "wasm32")]
fn failed_message(_detail: &str) -> String {
    "The browser blocked this call. Use the desktop app, or a local server that allows browser \
     requests."
        .to_string()
}

#[cfg(not(target_arch = "wasm32"))]
fn failed_message(_detail: &str) -> String {
    "Couldn't reach the endpoint. Check the base URL and that the server is running.".to_string()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use coursecraft_ai::{ChatMessage, MessageContent, Role};
    use coursecraft_ui_egui::EditorRequest;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeInner {
        calls: Vec<HttpCall>,
        callbacks: Vec<Box<dyn FnOnce(FetchOutcome) + Send>>,
    }

    /// Records calls and lets a test answer them when it chooses.
    #[derive(Clone, Default)]
    struct FakeTransport {
        inner: Arc<Mutex<FakeInner>>,
    }

    impl FakeTransport {
        fn call_count(&self) -> usize {
            self.inner.lock().expect("lock").calls.len()
        }

        fn last_url(&self) -> Option<String> {
            self.inner
                .lock()
                .expect("lock")
                .calls
                .last()
                .map(|call| call.url.clone())
        }

        fn answer(&self, outcome: FetchOutcome) {
            let callback = self.inner.lock().expect("lock").callbacks.pop();
            if let Some(callback) = callback {
                callback(outcome);
            }
        }
    }

    impl AiTransport for FakeTransport {
        fn send(&self, call: HttpCall, done: Box<dyn FnOnce(FetchOutcome) + Send>) {
            let mut inner = self.inner.lock().expect("lock");
            inner.calls.push(call);
            inner.callbacks.push(done);
        }
    }

    fn config() -> AiConfig {
        AiConfig {
            base_url: "https://api.example.com/v1".to_string(),
            model: "test-model".to_string(),
            api_key: "sk-secret".to_string(),
            speech_to_text_model: "whisper-1".to_string(),
            text_to_speech_model: "tts-1".to_string(),
            voice: "alloy".to_string(),
        }
    }

    fn request() -> ChatRequest {
        ChatRequest {
            messages: vec![ChatMessage {
                role: Role::User,
                content: MessageContent::Text("Reply with OK.".to_string()),
            }],
            max_tokens: Some(8),
            temperature: None,
        }
    }

    fn reply_body(text: &str) -> Vec<u8> {
        format!(
            "{{\"choices\":[{{\"message\":{{\"content\":\"{text}\"}},\"finish_reason\":\"stop\"}}]}}"
        )
        .into_bytes()
    }

    fn client(fake: &FakeTransport) -> AiClient {
        AiClient::with_transport(Box::new(fake.clone()))
    }

    #[test]
    fn reports_a_successful_reply() {
        let fake = FakeTransport::default();
        let mut client = client(&fake);
        client
            .begin(AiPurpose::TestConnection, &config(), &request(), 0.0, || {})
            .expect("begin");
        assert_eq!(fake.call_count(), 1);
        assert_eq!(
            fake.last_url().as_deref(),
            Some("https://api.example.com/v1/chat/completions")
        );
        fake.answer(FetchOutcome::Response {
            status: 200,
            body: reply_body("OK"),
        });
        let (purpose, result) = client.poll(1.0).expect("result");
        assert_eq!(purpose, AiPurpose::TestConnection);
        let AiReply::Chat(reply) = result.expect("ok") else {
            panic!("expected a chat reply");
        };
        assert_eq!(reply.text, "OK");
        assert!(!client.is_pending());
    }

    #[test]
    fn reports_a_failed_call_without_the_key() {
        let fake = FakeTransport::default();
        let mut client = client(&fake);
        client
            .begin(AiPurpose::TestConnection, &config(), &request(), 0.0, || {})
            .expect("begin");
        fake.answer(FetchOutcome::Failed {
            message: "connection refused".to_string(),
        });
        let (_, result) = client.poll(1.0).expect("result");
        let message = result.expect_err("must fail");
        assert!(!message.is_empty());
        assert!(!message.contains("sk-secret"), "{message}");
        #[cfg(target_arch = "wasm32")]
        assert!(message.contains("browser blocked"), "{message}");
    }

    #[test]
    fn maps_an_http_error_to_a_plain_sentence() {
        let fake = FakeTransport::default();
        let mut client = client(&fake);
        client
            .begin(AiPurpose::TestConnection, &config(), &request(), 0.0, || {})
            .expect("begin");
        fake.answer(FetchOutcome::Response {
            status: 401,
            body: b"".to_vec(),
        });
        let (_, result) = client.poll(1.0).expect("result");
        assert!(result.expect_err("must fail").contains("rejected the key"));
    }

    #[test]
    fn a_second_call_is_refused_while_one_runs() {
        let fake = FakeTransport::default();
        let mut client = client(&fake);
        client
            .begin(AiPurpose::TestConnection, &config(), &request(), 0.0, || {})
            .expect("first");
        let error = client
            .begin(AiPurpose::TestConnection, &config(), &request(), 0.0, || {})
            .expect_err("second must be refused");
        assert!(error.contains("already running"), "{error}");
        assert_eq!(fake.call_count(), 1);
    }

    #[test]
    fn an_invalid_config_sends_nothing() {
        let fake = FakeTransport::default();
        let mut client = client(&fake);
        let mut bad = config();
        bad.base_url = "http://api.example.com/v1".to_string();
        let error = client
            .begin(AiPurpose::TestConnection, &bad, &request(), 0.0, || {})
            .expect_err("must refuse");
        assert!(error.contains("https://"), "{error}");
        assert_eq!(fake.call_count(), 0);
        assert!(!client.is_pending());
    }

    #[test]
    fn a_timeout_drops_the_call_and_ignores_a_late_answer() {
        let fake = FakeTransport::default();
        let mut client = client(&fake);
        client
            .begin(AiPurpose::TestConnection, &config(), &request(), 0.0, || {})
            .expect("begin");
        let (_, result) = client.poll(TIMEOUT_SECONDS).expect("timeout");
        assert!(result.expect_err("must fail").contains("took too long"));
        assert!(!client.is_pending());

        fake.answer(FetchOutcome::Response {
            status: 200,
            body: reply_body("late"),
        });
        assert!(client.poll(TIMEOUT_SECONDS + 1.0).is_none());
    }

    #[test]
    fn cancel_ignores_a_late_answer() {
        let fake = FakeTransport::default();
        let mut client = client(&fake);
        client
            .begin(AiPurpose::TestConnection, &config(), &request(), 0.0, || {})
            .expect("begin");
        client.cancel();
        assert!(!client.is_pending());
        fake.answer(FetchOutcome::Response {
            status: 200,
            body: reply_body("late"),
        });
        assert!(client.poll(1.0).is_none());
    }

    #[test]
    fn a_speech_answer_keeps_its_bytes_exactly() {
        let fake = FakeTransport::default();
        let mut client = client(&fake);
        client
            .begin_speech(
                AiPurpose::Speech {
                    slide: "s-one".to_string(),
                },
                &config(),
                "tts-1",
                "alloy",
                "Hello.",
                0.0,
                || {},
            )
            .expect("begin");
        assert_eq!(
            fake.last_url().as_deref(),
            Some("https://api.example.com/v1/audio/speech")
        );
        // Not valid UTF-8: a text transport would corrupt or reject these bytes.
        let mp3 = vec![0xFF, 0xFB, 0x90, 0x64, 0x00, 0xFF, 0x80, 0xC3];
        fake.answer(FetchOutcome::Response {
            status: 200,
            body: mp3.clone(),
        });
        let (purpose, result) = client.poll(1.0).expect("result");
        assert_eq!(
            purpose,
            AiPurpose::Speech {
                slide: "s-one".to_string()
            }
        );
        let AiReply::Speech(bytes) = result.expect("audio") else {
            panic!("expected speech");
        };
        assert_eq!(bytes, mp3);
    }

    #[test]
    fn a_transcription_answer_is_parsed_as_captions() {
        let fake = FakeTransport::default();
        let mut client = client(&fake);
        client
            .begin_transcription(
                AiPurpose::Captions {
                    object: "audio1".to_string(),
                    slide: "s-one".to_string(),
                },
                &config(),
                "whisper-1",
                "sound.mp3",
                b"ID3 audio bytes",
                0.0,
                || {},
            )
            .expect("begin");
        assert_eq!(
            fake.last_url().as_deref(),
            Some("https://api.example.com/v1/audio/transcriptions")
        );
        let vtt = b"WEBVTT\n\n00:00:00.000 --> 00:00:02.000\nHello\n".to_vec();
        fake.answer(FetchOutcome::Response {
            status: 200,
            body: vtt,
        });
        let (_, result) = client.poll(1.0).expect("result");
        let AiReply::Captions(text) = result.expect("captions") else {
            panic!("expected captions");
        };
        assert!(text.starts_with("WEBVTT"));
    }

    fn elevenlabs_config() -> ElevenLabsConfig {
        ElevenLabsConfig {
            api_key: "el-secret".to_string(),
            voice_id: "voice123".to_string(),
            voice_name: "Rachel".to_string(),
            model_id: "eleven_multilingual_v2".to_string(),
        }
    }

    fn editor_with(mp3_slide: &str) -> coursecraft_ui_egui::Editor {
        let course = crate::initial_course(true);
        let mut editor = coursecraft_ui_egui::Editor::new(course);
        editor.set_elevenlabs_config(Some(elevenlabs_config()));
        editor.open_speech_dialog();
        editor.ui.speech.text = "Welcome to the course.".to_string();
        assert_eq!(editor.ui.speech.slide, mp3_slide);
        editor
    }

    fn take_request(editor: &mut coursecraft_ui_egui::Editor) -> Option<EditorRequest> {
        editor.pending_requests.pop()
    }

    #[test]
    fn elevenlabs_runs_models_then_two_voice_pages_then_speech() {
        let fake = FakeTransport::default();
        let mut client = client(&fake);
        let course = crate::initial_course(true);
        let slide = course.slides().next().expect("slide").id.clone();
        let mut editor = editor_with(&slide);

        // A committed key starts models.
        client
            .begin_elevenlabs_models(
                AiPurpose::ElevenLabsModels,
                &elevenlabs_config(),
                0.0,
                || {},
            )
            .expect("models");
        assert_eq!(
            fake.last_url().as_deref(),
            Some("https://api.elevenlabs.io/v1/models")
        );
        fake.answer(FetchOutcome::Response {
            status: 200,
            body: br#"[
                {"model_id":"eleven_multilingual_v2","name":"Multilingual v2","can_do_text_to_speech":true,"maximum_text_length_per_request":5000}
            ]"#
            .to_vec(),
        });
        let (purpose, result) = client.poll(1.0).expect("models result");
        editor.set_elevenlabs_result(purpose, result);
        assert_eq!(editor.ui.elevenlabs.models.len(), 1);
        assert_eq!(editor.ui.elevenlabs.max_chars(), 5000);

        // Models are followed by the first voices page, one call at a time.
        let EditorRequest::ElevenLabsVoices {
            purpose,
            page_token,
        } = take_request(&mut editor).expect("first voices page")
        else {
            panic!("expected a voices request");
        };
        assert_eq!(purpose, AiPurpose::ElevenLabsVoices { page: 0 });
        assert_eq!(page_token, None);
        client
            .begin_elevenlabs_voices(
                purpose,
                &elevenlabs_config(),
                page_token.as_deref(),
                2.0,
                || {},
            )
            .expect("page 0");
        assert_eq!(
            fake.last_url().as_deref(),
            Some("https://api.elevenlabs.io/v2/voices?page_size=100")
        );
        fake.answer(FetchOutcome::Response {
            status: 200,
            body: br#"{
                "voices":[
                    {"voice_id":"voice123","name":"Rachel","category":"premade"},
                    {"voice_id":"voice456","name":"Adam","category":"default"}
                ],
                "has_more":true,
                "next_page_token":"next page"
            }"#
            .to_vec(),
        });
        let (purpose, result) = client.poll(3.0).expect("page 0 result");
        editor.set_elevenlabs_result(purpose, result);
        assert_eq!(editor.ui.elevenlabs.voices.len(), 2);
        assert_eq!(editor.ui.elevenlabs.pages_loaded, 1);

        let EditorRequest::ElevenLabsVoices {
            purpose,
            page_token,
        } = take_request(&mut editor).expect("second voices page")
        else {
            panic!("expected a second voices request");
        };
        assert_eq!(purpose, AiPurpose::ElevenLabsVoices { page: 1 });
        assert_eq!(page_token.as_deref(), Some("next page"));
        client
            .begin_elevenlabs_voices(
                purpose,
                &elevenlabs_config(),
                page_token.as_deref(),
                4.0,
                || {},
            )
            .expect("page 1");
        assert_eq!(
            fake.last_url().as_deref(),
            Some("https://api.elevenlabs.io/v2/voices?page_size=100&next_page_token=next%20page")
        );
        fake.answer(FetchOutcome::Response {
            status: 200,
            body: br#"{
                "voices":[{"voice_id":"voice789","name":"Bella","category":"cloned"}],
                "has_more":false,
                "next_page_token":null
            }"#
            .to_vec(),
        });
        let (purpose, result) = client.poll(5.0).expect("page 1 result");
        editor.set_elevenlabs_result(purpose, result);
        assert_eq!(editor.ui.elevenlabs.voices.len(), 3);
        assert_eq!(editor.ui.elevenlabs.pages_loaded, 2);
        assert!(take_request(&mut editor).is_none(), "no more pages");

        // The speech call uses the stored voice, then Add to slide is one undo.
        let mp3 = vec![0xFF, 0xFB, 0x90, 0x64, 0x00];
        let before = editor.undo_len_for_test();
        client
            .begin_elevenlabs_speech(
                AiPurpose::Speech {
                    slide: slide.clone(),
                },
                &elevenlabs_config(),
                &editor.ui.speech.text.clone(),
                5000,
                6.0,
                || {},
            )
            .expect("speech");
        assert_eq!(
            fake.last_url().as_deref(),
            Some(
                "https://api.elevenlabs.io/v1/text-to-speech/voice123?output_format=mp3_44100_128"
            )
        );
        fake.answer(FetchOutcome::Response {
            status: 200,
            body: mp3.clone(),
        });
        let (purpose, result) = client.poll(7.0).expect("speech result");
        assert_eq!(
            purpose,
            AiPurpose::Speech {
                slide: slide.clone()
            }
        );
        editor.ui.speech.phase = coursecraft_ui_egui::audio_ai::AudioPhase::Running;
        editor.set_audio_result(purpose, result);
        assert_eq!(editor.ui.speech.mp3.as_deref(), Some(mp3.as_slice()));
        let added = editor
            .add_narration_audio(&slide, &mp3)
            .expect("added to slide");
        assert!(!added.is_empty());
        assert_eq!(editor.undo_len_for_test(), before + 1);
        assert!(
            editor
                .media()
                .iter()
                .any(|(name, _)| name == "narration.mp3")
        );
        editor.run("edit.undo", serde_json::json!({}));
        assert_eq!(editor.undo_len_for_test(), before);
    }

    #[test]
    fn a_failed_voice_lookup_warns_but_the_id_still_makes_speech() {
        let fake = FakeTransport::default();
        let mut client = client(&fake);
        let course = crate::initial_course(true);
        let slide = course.slides().next().expect("slide").id.clone();
        let mut editor = editor_with(&slide);

        editor.set_elevenlabs_result(
            AiPurpose::ElevenLabsVoiceLookup,
            Err("This API key isn't allowed to read voices.".to_string()),
        );
        assert_eq!(editor.ui.elevenlabs.config.voice_id, "voice123");
        assert!(editor.ui.elevenlabs.paste_warning.is_some());

        client
            .begin_elevenlabs_speech(
                AiPurpose::Speech {
                    slide: slide.clone(),
                },
                &elevenlabs_config(),
                "Hello.",
                10_000,
                0.0,
                || {},
            )
            .expect("speech still starts");
    }

    #[test]
    fn cancel_drops_a_late_elevenlabs_answer() {
        let fake = FakeTransport::default();
        let mut client = client(&fake);
        client
            .begin_elevenlabs_voices(
                AiPurpose::ElevenLabsVoices { page: 0 },
                &elevenlabs_config(),
                None,
                0.0,
                || {},
            )
            .expect("begin");
        client.cancel();
        assert!(!client.is_pending());
        fake.answer(FetchOutcome::Response {
            status: 200,
            body: br#"{"voices":[],"has_more":false}"#.to_vec(),
        });
        assert!(client.poll(1.0).is_none());
    }

    #[test]
    fn elevenlabs_speech_gets_300_seconds_and_others_keep_120() {
        let fake = FakeTransport::default();
        let mut client = client(&fake);
        client
            .begin_elevenlabs_speech(
                AiPurpose::Speech {
                    slide: "s".to_string(),
                },
                &elevenlabs_config(),
                "Hello.",
                10_000,
                0.0,
                || {},
            )
            .expect("speech");
        assert!(client.poll(120.0).is_none(), "still waiting at 120s");
        assert!(client.poll(299.0).is_none(), "still waiting at 299s");
        let (_, result) = client
            .poll(ELEVENLABS_SPEECH_TIMEOUT_SECONDS)
            .expect("timeout");
        assert!(result.expect_err("must fail").contains("took too long"));

        let mut chat_client = self::client(&fake);
        chat_client
            .begin(AiPurpose::TestConnection, &config(), &request(), 0.0, || {})
            .expect("chat");
        let (_, result) = chat_client.poll(TIMEOUT_SECONDS).expect("chat timeout");
        assert!(result.expect_err("must fail").contains("took too long"));
    }

    #[test]
    fn an_audio_404_reads_as_offered_by_the_server() {
        let fake = FakeTransport::default();
        let mut client = client(&fake);
        client
            .begin_speech(
                AiPurpose::Speech {
                    slide: "s-one".to_string(),
                },
                &config(),
                "tts-1",
                "alloy",
                "Hello.",
                0.0,
                || {},
            )
            .expect("begin");
        fake.answer(FetchOutcome::Response {
            status: 404,
            body: b"".to_vec(),
        });
        let (_, result) = client.poll(1.0).expect("result");
        let message = result.expect_err("must fail");
        assert!(
            message.contains("doesn't offer text-to-speech"),
            "{message}"
        );
        assert!(!message.contains("sk-secret"), "{message}");
    }
}
