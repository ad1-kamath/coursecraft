//! ElevenLabs state for the Narration from Text window.
//!
//! The window can use the configured AI provider or ElevenLabs. Both are shown
//! in one place, but their settings are separate: this module owns the
//! ElevenLabs values, the voice and model lists, and the result handlers. It
//! never talks to the network; the app owns the transport.

use coursecraft_ai::{
    AiReply, ELEVENLABS_DEFAULT_MAX_CHARS, ELEVENLABS_DEFAULT_MODEL, ElevenLabsConfig,
    ElevenLabsModel, ElevenLabsVoice, ElevenLabsVoicesPage, MAX_VOICES_PAGES, check_voice_id,
};

use crate::{AiPurpose, Editor, EditorRequest};

/// Which service makes the narration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SpeechSource {
    /// The OpenAI-compatible endpoint in AI Settings (the original behaviour).
    #[default]
    AiProvider,
    /// The author's own ElevenLabs account.
    ElevenLabs,
}

impl SpeechSource {
    /// The segmented control's labels, in order.
    pub const ALL: [Self; 2] = [Self::AiProvider, Self::ElevenLabs];

    /// Label shown in the segmented control.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::AiProvider => "AI provider",
            Self::ElevenLabs => "ElevenLabs",
        }
    }
}

/// ElevenLabs settings and the lists loaded for the window.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ElevenLabsState {
    /// The current values.
    pub config: ElevenLabsConfig,
    /// The values last pushed to the app to save.
    pub saved: ElevenLabsConfig,
    /// Whether the API key field shows its text.
    pub show_key: bool,
    /// Models that can do text to speech.
    pub models: Vec<ElevenLabsModel>,
    /// Why the model list failed, if it did. The default model still works.
    pub models_error: Option<String>,
    /// Whether the model request has finished (successfully or not).
    pub models_loaded: bool,
    /// Voices loaded so far.
    pub voices: Vec<ElevenLabsVoice>,
    /// Why the voice list failed, if it did.
    pub voices_error: Option<String>,
    /// True once at least one voice page arrived.
    pub voices_loaded: bool,
    /// Number of voice pages received; the next expected page index.
    pub pages_loaded: u32,
    /// Token for the next page, when one is still waiting.
    pub next_page_token: Option<String>,
    /// Filter text above the voice list.
    pub voice_filter: String,
    /// The id typed into "Or paste a voice ID".
    pub pasted_id: String,
    /// A lookup warning. The pasted id is still used.
    pub paste_warning: Option<String>,
    /// A syntactically unusable pasted id.
    pub paste_error: Option<String>,
    /// A voice or model list is being fetched.
    pub listing_running: bool,
    /// A voice lookup is being fetched.
    pub lookup_running: bool,
}

impl ElevenLabsState {
    /// The model id the speech call will use.
    #[must_use]
    pub fn model_id(&self) -> &str {
        let model = self.config.model_id.trim();
        if model.is_empty() {
            ELEVENLABS_DEFAULT_MODEL
        } else {
            model
        }
    }

    /// The selected model's name, or its id, or the default.
    #[must_use]
    pub fn model_label(&self) -> String {
        let id = self.model_id();
        self.models
            .iter()
            .find(|model| model.id == id)
            .map_or_else(|| id.to_string(), |model| model.name.clone())
    }

    /// The character limit for the selected model.
    #[must_use]
    pub fn max_chars(&self) -> usize {
        let id = self.model_id();
        self.models
            .iter()
            .find(|model| model.id == id)
            .and_then(|model| model.max_chars)
            .filter(|limit| *limit > 0)
            .unwrap_or(ELEVENLABS_DEFAULT_MAX_CHARS)
    }

    /// The selected voice's display name.
    #[must_use]
    pub fn voice_label(&self) -> String {
        if self.config.voice_name.trim().is_empty() {
            self.config.voice_id.clone()
        } else {
            self.config.voice_name.clone()
        }
    }

    /// Voices in display order: `default` category first, then by name.
    #[must_use]
    pub fn sorted_voices(&self) -> Vec<&ElevenLabsVoice> {
        let filter = self.voice_filter.trim().to_lowercase();
        let mut voices: Vec<&ElevenLabsVoice> = self
            .voices
            .iter()
            .filter(|voice| {
                filter.is_empty()
                    || voice.name.to_lowercase().contains(&filter)
                    || voice.category.to_lowercase().contains(&filter)
            })
            .collect();
        voices.sort_by(|a, b| {
            let a_default = is_default_voice(&a.category);
            let b_default = is_default_voice(&b.category);
            b_default
                .cmp(&a_default)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                .then_with(|| a.id.cmp(&b.id))
        });
        voices
    }
}

impl Editor {
    /// Set the ElevenLabs settings from the app at startup (or clear them).
    pub fn set_elevenlabs_config(&mut self, config: Option<ElevenLabsConfig>) {
        let config = config.unwrap_or_default();
        if self.ui.elevenlabs.pasted_id.is_empty() {
            self.ui.elevenlabs.pasted_id = config.voice_id.clone();
        }
        self.ui.elevenlabs.saved = config.clone();
        self.ui.elevenlabs.config = config;
    }

    /// Set the remembered narration source.
    pub fn set_speech_source(&mut self, source: SpeechSource) {
        self.ui.speech_source = source;
    }

    /// Hand a finished ElevenLabs list or lookup to the window.
    pub fn set_elevenlabs_result(&mut self, purpose: AiPurpose, result: Result<AiReply, String>) {
        match purpose {
            AiPurpose::ElevenLabsVoices { page } => set_voices_result(self, page, result),
            AiPurpose::ElevenLabsModels => set_models_result(self, result),
            AiPurpose::ElevenLabsVoiceLookup => set_lookup_result(self, result),
            _ => {}
        }
    }
}

/// ElevenLabs lists its built-in voices under the `premade` category.
fn is_default_voice(category: &str) -> bool {
    category.eq_ignore_ascii_case("premade") || category.eq_ignore_ascii_case("default")
}

/// True when a config has anything worth storing.
#[must_use]
pub fn config_option(state: &ElevenLabsState) -> Option<ElevenLabsConfig> {
    let config = &state.config;
    if config.api_key.trim().is_empty()
        && config.voice_id.trim().is_empty()
        && config.voice_name.trim().is_empty()
        && config.model_id.trim().is_empty()
    {
        None
    } else {
        Some(config.clone())
    }
}

/// Start loading models, then voices. Called when a key is committed or a list
/// is refreshed.
pub fn request_models(editor: &mut Editor, requests: &mut Vec<EditorRequest>) {
    let state = &mut editor.ui.elevenlabs;
    if state.config.api_key.trim().is_empty() {
        state.models_error = Some("Enter your ElevenLabs API key.".to_string());
        state.models_loaded = true;
        return;
    }
    state.listing_running = true;
    state.models_error = None;
    requests.push(EditorRequest::ElevenLabsModels {
        purpose: AiPurpose::ElevenLabsModels,
    });
}

/// Start loading voices from the first page.
pub fn request_voices(editor: &mut Editor, requests: &mut Vec<EditorRequest>) {
    let state = &mut editor.ui.elevenlabs;
    if state.config.api_key.trim().is_empty() {
        state.voices_error = Some("Enter your ElevenLabs API key.".to_string());
        return;
    }
    state.voices.clear();
    state.voices_error = None;
    state.voices_loaded = false;
    state.pages_loaded = 0;
    state.next_page_token = None;
    push_voices_page(editor, requests, 0, None);
}

fn push_voices_page(
    editor: &mut Editor,
    requests: &mut Vec<EditorRequest>,
    page: u32,
    token: Option<String>,
) {
    editor.ui.elevenlabs.listing_running = true;
    requests.push(EditorRequest::ElevenLabsVoices {
        purpose: AiPurpose::ElevenLabsVoices { page },
        page_token: token,
    });
}

fn set_models_result(editor: &mut Editor, result: Result<AiReply, String>) {
    let state = &mut editor.ui.elevenlabs;
    state.listing_running = false;
    state.models_loaded = true;
    match result {
        Ok(AiReply::ElevenLabsModels(models)) => {
            state.models = models;
            state.models_error = None;
        }
        Ok(_) => {
            state.models_error = Some("The server returned the wrong kind of answer.".to_string());
        }
        Err(message) => state.models_error = Some(message),
    }
    // The default model still works after a failed list, and voices are useful
    // either way, so the two loads run one after the other.
    let mut requests = Vec::new();
    request_voices(editor, &mut requests);
    for request in requests {
        editor.request(request);
    }
}

fn set_voices_result(editor: &mut Editor, page: u32, result: Result<AiReply, String>) {
    let state = &mut editor.ui.elevenlabs;
    if page != state.pages_loaded {
        return;
    }
    state.listing_running = false;
    match result {
        Ok(AiReply::ElevenLabsVoices(ElevenLabsVoicesPage {
            voices,
            has_more,
            next_page_token,
        })) => {
            for voice in voices {
                if !state.voices.iter().any(|known| known.id == voice.id) {
                    state.voices.push(voice);
                }
            }
            state.voices_loaded = true;
            state.voices_error = None;
            state.pages_loaded = page.saturating_add(1);
            if has_more
                && page.saturating_add(1) < MAX_VOICES_PAGES
                && let Some(token) = next_page_token.filter(|token| !token.trim().is_empty())
            {
                state.next_page_token = Some(token.clone());
                let mut requests = Vec::new();
                push_voices_page(editor, &mut requests, page.saturating_add(1), Some(token));
                for request in requests {
                    editor.request(request);
                }
                return;
            }
            state.next_page_token = None;
        }
        Ok(_) => {
            state.voices_error = Some("The server returned the wrong kind of answer.".to_string());
            state.next_page_token = None;
        }
        Err(message) => {
            state.voices_error = Some(message);
            state.next_page_token = None;
        }
    }
}

fn set_lookup_result(editor: &mut Editor, result: Result<AiReply, String>) {
    let state = &mut editor.ui.elevenlabs;
    state.lookup_running = false;
    match result {
        Ok(AiReply::ElevenLabsVoice(voice)) => {
            state.config.voice_id = voice.id;
            state.config.voice_name = voice.name;
            state.pasted_id = state.config.voice_id.clone();
            state.paste_warning = None;
            state.paste_error = None;
            state.saved = state.config.clone();
            let config = state.config.clone();
            editor.request(EditorRequest::SaveElevenLabs(Some(config)));
        }
        Ok(_) => {
            state.paste_warning = Some("The server returned the wrong kind of answer.".to_string());
        }
        Err(message) => {
            // A restricted key can speak with a voice it cannot read, so the
            // pasted id stays in use and the lookup only warns.
            state.paste_warning = Some(message);
        }
    }
}

/// Validate a pasted id and, when it looks usable, start a lookup.
///
/// Returns true when a lookup request was pushed. The id is kept even when the
/// lookup fails.
pub fn commit_pasted_voice(editor: &mut Editor, requests: &mut Vec<EditorRequest>) -> bool {
    let state = &mut editor.ui.elevenlabs;
    let typed = state.pasted_id.trim().to_string();
    let id = match check_voice_id(&typed) {
        Ok(id) => id.to_string(),
        Err(error) => {
            state.paste_error = Some(error.to_string());
            state.paste_warning = None;
            return false;
        }
    };
    state.paste_error = None;
    state.paste_warning = None;
    state.config.voice_id = id.clone();
    state.config.voice_name.clear();
    state.saved = state.config.clone();
    let config = state.config.clone();
    requests.push(EditorRequest::SaveElevenLabs(Some(config)));
    if state.config.api_key.trim().is_empty() {
        return false;
    }
    state.lookup_running = true;
    requests.push(EditorRequest::ElevenLabsVoiceLookup {
        purpose: AiPurpose::ElevenLabsVoiceLookup,
        voice_id: id,
    });
    true
}

/// Save a changed config through the app.
pub fn push_save(editor: &mut Editor, requests: &mut Vec<EditorRequest>) {
    let config = config_option(&editor.ui.elevenlabs);
    editor.ui.elevenlabs.saved = editor.ui.elevenlabs.config.clone();
    requests.push(EditorRequest::SaveElevenLabs(config));
}
