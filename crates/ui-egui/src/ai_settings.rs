//! Optional AI settings: endpoint, model, key and a test connection.
//!
//! The same window also has an **Agents** tab for connecting CourseCraft to an
//! agent that drives it from outside. The editor only holds the values. Building
//! the request and reading the answer happen in `coursecraft-ai`, and the app
//! owns the transport through `EditorRequest::AiChat`. Agent commands and files
//! are the app's job too; the UI only sends `EditorRequest`s.

use crate::dialog::Dialog;
use coursecraft_ai::agents::{AgentAction, AgentId, AgentStatus};
use coursecraft_ai::{
    AiConfig, AiSource, ChatMessage, ChatReply, ChatRequest, MessageContent, Role, check_config,
    endpoint_host,
};
use egui::{Button, ComboBox, Key, Modifiers, RichText, TextEdit, Ui};

use crate::{AiPurpose, Editor, EditorRequest};

/// The tiny prompt used by `Test connection`.
pub const TEST_PROMPT: &str = "Reply with OK.";

/// A chat request that asks the endpoint to answer with one short word.
#[must_use]
pub fn test_connection_request() -> ChatRequest {
    ChatRequest {
        messages: vec![ChatMessage {
            role: Role::User,
            content: MessageContent::Text(TEST_PROMPT.to_string()),
        }],
        max_tokens: Some(8),
        temperature: None,
    }
}

/// A preset endpoint. Picking one only fills in the base URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AiProvider {
    Ollama,
    LmStudio,
    DeepSeek,
    OpenAi,
    /// Any other URL; leaves the URL as it is.
    #[default]
    Other,
}

impl AiProvider {
    /// Every provider, in the order the dropdown shows them.
    pub const ALL: [Self; 5] = [
        Self::Ollama,
        Self::LmStudio,
        Self::DeepSeek,
        Self::OpenAi,
        Self::Other,
    ];

    /// Name shown in the dropdown.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Ollama => "Ollama (on this computer)",
            Self::LmStudio => "LM Studio (on this computer)",
            Self::DeepSeek => "DeepSeek",
            Self::OpenAi => "OpenAI",
            Self::Other => "Other",
        }
    }

    /// The base URL a preset fills in, or `None` for `Other`.
    #[must_use]
    pub fn base_url(self) -> Option<&'static str> {
        match self {
            Self::Ollama => Some("http://localhost:11434/v1"),
            Self::LmStudio => Some("http://localhost:1234/v1"),
            Self::DeepSeek => Some("https://api.deepseek.com"),
            Self::OpenAi => Some("https://api.openai.com/v1"),
            Self::Other => None,
        }
    }

    /// The preset matching `url`, ignoring a trailing slash, or `Other`.
    #[must_use]
    pub fn from_url(url: &str) -> Self {
        let normalized = url.trim().trim_end_matches('/');
        Self::ALL
            .into_iter()
            .find(|provider| {
                provider
                    .base_url()
                    .is_some_and(|preset| preset.eq_ignore_ascii_case(normalized))
            })
            .unwrap_or(Self::Other)
    }
}

/// The label for one choice in the AI features tab's **Use:** row.
#[must_use]
pub fn source_label(source: AiSource) -> &'static str {
    match source {
        AiSource::Endpoint => "An API endpoint",
        AiSource::ClaudeCode => "Claude Code (your Claude plan)",
        AiSource::Codex => "Codex (your ChatGPT plan)",
    }
}

/// Where an agent source sends content, for a "what's sent and where" notice.
#[must_use]
pub fn agent_destination(source: AiSource) -> Option<&'static str> {
    match source {
        AiSource::Endpoint => None,
        AiSource::ClaudeCode => {
            Some("Claude Code on this computer, which sends it to Anthropic under your plan")
        }
        AiSource::Codex => Some("Codex on this computer, which sends it to OpenAI under your plan"),
    }
}

/// Which tab the AI Settings window is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AiSettingsTab {
    /// What powers CourseCraft's own AI features.
    #[default]
    Features,
    /// Tools that drive CourseCraft from outside.
    Agents,
}

/// What the Test connection row is showing.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum AiTestState {
    /// Nothing has been tested.
    #[default]
    Idle,
    /// A request is in flight; `host` was taken when it started.
    Running {
        /// The endpoint's host, shown on success.
        host: Option<String>,
    },
    /// The endpoint answered.
    Success {
        /// The endpoint's host.
        host: Option<String>,
    },
    /// The call failed with a plain sentence.
    Failed(String),
}

/// One row in the Agents tab.
#[derive(Debug, Clone, PartialEq)]
pub struct AgentRow {
    /// Which agent this row is for.
    pub id: AgentId,
    /// Last known state.
    pub status: AgentStatus,
    /// A check or action for this row is running.
    pub busy: bool,
    /// A short success sentence shown under the row.
    pub notice: Option<String>,
    /// The last action tried, so "Try again" can repeat it.
    pub last_action: Option<AgentAction>,
}

impl AgentRow {
    fn new(id: AgentId) -> Self {
        Self {
            id,
            status: AgentStatus::Checking,
            busy: false,
            notice: None,
            last_action: None,
        }
    }
}

/// Transient state for the AI Settings dialog, kept in `editor.ui`.
#[derive(Debug, Clone, PartialEq)]
pub struct AiSettingsState {
    /// Whether the dialog is open.
    pub open: bool,
    /// Which tab is showing.
    pub tab: AiSettingsTab,
    /// What powers the features: an endpoint or an installed agent.
    pub source: AiSource,
    /// The source last applied with Save or Remove.
    pub saved_source: AiSource,
    /// The editable values.
    pub config: AiConfig,
    /// The values last applied with Save or Remove.
    pub saved: AiConfig,
    /// Preset chosen in the dropdown. Only used to show the selection.
    pub provider: AiProvider,
    /// Whether the API key field shows its text.
    pub show_key: bool,
    /// Test connection status.
    pub test: AiTestState,
    /// One row per agent, in display order.
    pub agents: Vec<AgentRow>,
    /// Whether the current opening has already asked the app to detect agents.
    pub agents_detected: bool,
    /// Whether a detection pass is in flight.
    pub detection_running: bool,
}

impl Default for AiSettingsState {
    fn default() -> Self {
        Self {
            open: false,
            tab: AiSettingsTab::Features,
            source: AiSource::Endpoint,
            saved_source: AiSource::Endpoint,
            config: AiConfig::default(),
            saved: AiConfig::default(),
            provider: AiProvider::Other,
            show_key: false,
            test: AiTestState::Idle,
            agents: AgentId::ALL.into_iter().map(AgentRow::new).collect(),
            agents_detected: false,
            detection_running: false,
        }
    }
}

impl Editor {
    /// Set the endpoint from the app at startup (or clear it).
    pub fn set_ai_config(&mut self, config: Option<AiConfig>) {
        let config = config.unwrap_or_default();
        self.ui.ai.provider = AiProvider::from_url(&config.base_url);
        self.ui.ai.config = config.clone();
        self.ui.ai.saved = config;
        self.ui.ai.test = AiTestState::Idle;
    }

    /// Set what powers the AI features from the app at startup (or reset it).
    pub fn set_ai_source(&mut self, source: AiSource) {
        self.ui.ai.source = source;
        self.ui.ai.saved_source = source;
    }

    /// Open AI Settings on the AI features tab with the saved values.
    pub fn open_ai_settings(&mut self) {
        if !self.ui.ai.open {
            self.ui.ai.agents_detected = false;
        }
        self.ui.ai.tab = AiSettingsTab::Features;
        self.ui.ai.config = self.ui.ai.saved.clone();
        self.ui.ai.source = self.ui.ai.saved_source;
        self.ui.ai.provider = AiProvider::from_url(&self.ui.ai.config.base_url);
        self.ui.ai.test = AiTestState::Idle;
        self.ui.ai.open = true;
    }

    /// Open AI Settings on the Agents tab.
    ///
    /// Detection runs once per opening; opening an already visible window again
    /// only switches the tab.
    pub fn open_agent_settings(&mut self) {
        if !self.ui.ai.open {
            self.ui.ai.agents_detected = false;
        }
        self.ui.ai.tab = AiSettingsTab::Agents;
        self.ui.ai.open = true;
    }

    /// True when an AI feature may send a request with the saved settings.
    ///
    /// An agent source is ready when that agent was found on this computer.
    #[must_use]
    pub fn ai_ready(&self) -> bool {
        let source = self.ui.ai.source;
        if let Some(id) = source.agent_id() {
            return self
                .ui
                .ai
                .agents
                .iter()
                .find(|row| row.id == id)
                .is_some_and(|row| {
                    matches!(row.status, AgentStatus::Found | AgentStatus::Connected)
                });
        }
        check_config(&self.ui.ai.config).is_ok()
    }

    /// Hand a background result back to the dialog.
    pub fn set_ai_result(&mut self, purpose: AiPurpose, result: Result<ChatReply, String>) {
        match purpose {
            AiPurpose::TestConnection => {
                let host = match &self.ui.ai.test {
                    AiTestState::Running { host } => host.clone(),
                    _ if self.ui.ai.source.is_agent() => None,
                    _ => endpoint_host(&self.ui.ai.config),
                };
                self.ui.ai.test = match result {
                    Ok(_) => AiTestState::Success { host },
                    Err(message) => AiTestState::Failed(message),
                };
            }
            AiPurpose::Outline => crate::document::set_result(self, result),
            AiPurpose::AltText { src } => {
                crate::alt_text::set_result(self, &src, result);
            }
            AiPurpose::Translate { batch } => {
                crate::translate_ai::set_result(self, batch, result);
            }
            AiPurpose::Captions { .. }
            | AiPurpose::Speech { .. }
            | AiPurpose::ElevenLabsVoices { .. }
            | AiPurpose::ElevenLabsModels
            | AiPurpose::ElevenLabsVoiceLookup => {}
        }
    }

    /// Forget an in-flight result after the author cancelled it.
    pub fn set_ai_cancelled(&mut self, purpose: AiPurpose) {
        match purpose {
            AiPurpose::TestConnection => {
                if matches!(self.ui.ai.test, AiTestState::Running { .. }) {
                    self.ui.ai.test = AiTestState::Idle;
                }
            }
            AiPurpose::Outline => crate::document::set_cancelled(self),
            AiPurpose::AltText { src } => crate::alt_text::set_cancelled(self, &src),
            AiPurpose::Translate { batch } => crate::translate_ai::set_cancelled(self, batch),
            AiPurpose::Captions { .. } | AiPurpose::Speech { .. } => {
                self.set_audio_cancelled(purpose);
            }
            AiPurpose::ElevenLabsVoices { .. }
            | AiPurpose::ElevenLabsModels
            | AiPurpose::ElevenLabsVoiceLookup => {
                self.ui.elevenlabs.listing_running = false;
                self.ui.elevenlabs.lookup_running = false;
            }
        }
    }

    /// Hand a finished detection pass back to the Agents tab.
    pub fn set_agent_statuses(&mut self, statuses: Vec<(AgentId, AgentStatus)>) {
        for (id, status) in statuses {
            if let Some(row) = self.ui.ai.agents.iter_mut().find(|row| row.id == id) {
                row.busy = false;
                // A failed action's sentence stays until the author retries it,
                // even though the tab refreshes in the background.
                if matches!(row.status, AgentStatus::Problem(_)) && row.last_action.is_some() {
                    continue;
                }
                if matches!(status, AgentStatus::Problem(_)) {
                    row.notice = None;
                }
                row.status = status;
            }
        }
        self.ui.ai.detection_running = false;
        self.ui.ai.agents_detected = true;
    }

    /// Hand a finished Connect, Test or Disconnect back to its row.
    pub fn set_agent_action_result(&mut self, agent: AgentId, result: Result<AgentStatus, String>) {
        let Some(index) = self.ui.ai.agents.iter().position(|row| row.id == agent) else {
            return;
        };
        let action = self.ui.ai.agents.get(index).and_then(|row| row.last_action);
        let Some(row) = self.ui.ai.agents.get_mut(index) else {
            return;
        };
        row.busy = false;
        // The agent client is idle again, so a fresh detection may run.
        self.ui.ai.detection_running = false;
        match result {
            Ok(status) => {
                row.notice = match (action, status.clone()) {
                    (Some(AgentAction::Test), AgentStatus::Connected) => Some(format!(
                        "Connected. Ask {} to use CourseCraft.",
                        agent.display_name()
                    )),
                    (Some(AgentAction::Connect), AgentStatus::Connected) => Some(match agent {
                        AgentId::ClaudeDesktop => {
                            "Connected. Restart Claude Desktop to pick it up.".to_string()
                        }
                        AgentId::Cursor => "Connected. Restart Cursor to pick it up.".to_string(),
                        _ => "Connected.".to_string(),
                    }),
                    (Some(AgentAction::Disconnect), _) => {
                        Some(format!("Disconnected from {}.", agent.display_name()))
                    }
                    _ => None,
                };
                row.status = status;
                self.ui.ai.agents_detected = false;
            }
            Err(message) => {
                row.notice = None;
                row.status = AgentStatus::Problem(message);
                self.ui.ai.agents_detected = false;
            }
        }
    }
}

/// The shared "AI isn't set up yet" state. Returns true when the author clicked
/// `Open AI Settings`.
pub fn show_ai_not_ready(ui: &mut Ui, what: &str) -> bool {
    ui.label(format!(
        "AI isn't set up yet. CourseCraft can {what} once an AI is set up in AI Settings."
    ));
    ui.add_space(8.0);
    crate::primary_button(ui, "Open AI Settings").clicked()
}

/// Draw the AI Settings dialog when it is open.
pub fn show_ai_settings_window(
    editor: &mut Editor,
    ctx: &egui::Context,
    requests: &mut Vec<EditorRequest>,
) {
    if !editor.ui.ai.open {
        return;
    }

    let mut open = true;
    let mut close = false;
    let mut save = false;
    let mut remove = false;
    let content_rect = ctx.content_rect();
    let max_width = if content_rect.width().is_finite() {
        (content_rect.width() - 40.0).clamp(320.0, 560.0)
    } else {
        560.0
    };

    Dialog::new("AI Settings")
        .width(max_width)
        .open(&mut open)
        .show_with_footer(ctx, |dialog| {
            let tabs = dialog.ui();
            tabs.set_max_width(max_width);
            tabs.horizontal(|ui| {
                ui.selectable_value(&mut editor.ui.ai.tab, AiSettingsTab::Agents, "Agents");
                ui.selectable_value(
                    &mut editor.ui.ai.tab,
                    AiSettingsTab::Features,
                    "AI features",
                );
            });
            tabs.add_space(coursecraft_render::SPACE_SM);

            dialog.body(|ui| match editor.ui.ai.tab {
                AiSettingsTab::Features => show_features_tab(editor, ui, requests),
                AiSettingsTab::Agents => show_agents_tab(editor, ui, requests),
            });

            // Outside the scrolling body, so the way out never scrolls off.
            dialog.footer(|ui| {
                ui.add_space(coursecraft_render::SPACE_SM);
                ui.horizontal(|ui| match editor.ui.ai.tab {
                    AiSettingsTab::Features => {
                        if crate::primary_button(ui, "Save").clicked() {
                            save = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                        if ui.button("Remove").clicked() {
                            remove = true;
                        }
                    }
                    AiSettingsTab::Agents => {
                        if ui.button("Close").clicked() {
                            close = true;
                        }
                    }
                });
            });

            if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
                close = true;
            }
        });

    if !open {
        close = true;
    }

    if remove {
        cancel_running(editor, requests);
        editor.ui.ai.config = AiConfig::default();
        editor.ui.ai.saved = AiConfig::default();
        editor.ui.ai.source = AiSource::Endpoint;
        editor.ui.ai.saved_source = AiSource::Endpoint;
        editor.ui.ai.provider = AiProvider::Other;
        editor.ui.ai.test = AiTestState::Idle;
        requests.push(EditorRequest::SaveAiConfig(None));
        requests.push(EditorRequest::SaveAiSource(AiSource::Endpoint));
        return;
    }

    if save {
        editor.ui.ai.saved = editor.ui.ai.config.clone();
        editor.ui.ai.saved_source = editor.ui.ai.source;
        editor.ui.ai.open = false;
        cancel_running(editor, requests);
        requests.push(EditorRequest::SaveAiConfig(Some(
            editor.ui.ai.saved.clone(),
        )));
        requests.push(EditorRequest::SaveAiSource(editor.ui.ai.saved_source));
    } else if close {
        cancel_running(editor, requests);
        editor.ui.ai.config = editor.ui.ai.saved.clone();
        editor.ui.ai.source = editor.ui.ai.saved_source;
        editor.ui.ai.provider = AiProvider::from_url(&editor.ui.ai.config.base_url);
        editor.ui.ai.test = AiTestState::Idle;
        editor.ui.ai.open = false;
    }
}

fn show_features_tab(editor: &mut Editor, ui: &mut Ui, requests: &mut Vec<EditorRequest>) {
    let p = coursecraft_render::palette(ui.ctx());
    ui.label(
        "Optional. CourseCraft works fully without this. Your content is only \
         sent when you ask for an AI feature.",
    );
    ui.add_space(coursecraft_render::SPACE_MD);

    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut chosen = None;
        ui.vertical(|ui| {
            ui.label("Use:");
            ui.horizontal(|ui| {
                for option in AiSource::ALL {
                    if ui
                        .selectable_label(editor.ui.ai.source == option, source_label(option))
                        .clicked()
                        && editor.ui.ai.source != option
                    {
                        chosen = Some(option);
                    }
                }
            });
        });
        if let Some(source) = chosen {
            editor.ui.ai.source = source;
            editor.ui.ai.test = AiTestState::Idle;
            if source.is_agent() {
                editor.ui.ai.agents_detected = false;
            }
        }
        ui.add_space(coursecraft_render::SPACE_SM);
    }

    let source = editor.ui.ai.source;
    let ready = if let Some(id) = source.agent_id() {
        show_agent_source(editor, ui, requests, id)
    } else {
        show_endpoint_settings(editor, ui)
    };

    ui.add_space(coursecraft_render::SPACE_MD);
    match editor.ui.ai.test.clone() {
        AiTestState::Running { .. } => {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new());
                ui.label("Testing…");
                if ui.button("Cancel").clicked() {
                    editor.ui.ai.test = AiTestState::Idle;
                    requests.push(EditorRequest::CancelAi(AiPurpose::TestConnection));
                }
            });
        }
        AiTestState::Success { host } => {
            let text = match host {
                Some(host) => format!("Connected to {host}."),
                None => "Connected.".to_string(),
            };
            ui.label(RichText::new(text).color(p.accent_text));
        }
        AiTestState::Failed(message) => {
            ui.label(RichText::new(message).color(p.danger));
        }
        AiTestState::Idle => {}
    }

    ui.add_space(coursecraft_render::SPACE_SM);
    ui.horizontal(|ui| {
        if ui
            .add_enabled(ready, Button::new("Test connection"))
            .clicked()
        {
            let host = if source.is_agent() {
                None
            } else {
                endpoint_host(&editor.ui.ai.config)
            };
            editor.ui.ai.test = AiTestState::Running { host };
            requests.push(EditorRequest::AiChat {
                purpose: AiPurpose::TestConnection,
                request: test_connection_request(),
            });
        }
    });

    // The footer owns the Save and Remove buttons; this tab only edits values.
}

fn show_endpoint_settings(editor: &mut Editor, ui: &mut Ui) -> bool {
    let p = coursecraft_render::palette(ui.ctx());
    ui.horizontal(|ui| {
        ui.label("Provider");
        ComboBox::from_id_salt("ai_provider")
            .selected_text(editor.ui.ai.provider.label())
            .show_ui(ui, |ui| {
                for provider in AiProvider::ALL {
                    if ui
                        .selectable_label(editor.ui.ai.provider == provider, provider.label())
                        .clicked()
                    {
                        editor.ui.ai.provider = provider;
                        if let Some(url) = provider.base_url() {
                            editor.ui.ai.config.base_url = url.to_string();
                        }
                    }
                }
            });
    });

    ui.add_space(coursecraft_render::SPACE_SM);
    ui.horizontal(|ui| {
        ui.label("Base URL");
        ui.add(
            TextEdit::singleline(&mut editor.ui.ai.config.base_url)
                .id_salt("ai_base_url")
                .desired_width(320.0),
        );
    });

    ui.horizontal(|ui| {
        ui.label("Model");
        ui.add(
            TextEdit::singleline(&mut editor.ui.ai.config.model)
                .id_salt("ai_model")
                .desired_width(320.0),
        );
    });

    ui.add_space(coursecraft_render::SPACE_SM);
    ui.horizontal(|ui| {
        ui.label("API key");
        ui.add(
            TextEdit::singleline(&mut editor.ui.ai.config.api_key)
                .id_salt("ai_api_key")
                .password(!editor.ui.ai.show_key)
                .desired_width(260.0),
        );
        let label = if editor.ui.ai.show_key {
            "Hide"
        } else {
            "Show"
        };
        if ui.button(label).clicked() {
            editor.ui.ai.show_key = !editor.ui.ai.show_key;
        }
    });
    ui.small("Leave empty for a local server");
    #[cfg(target_arch = "wasm32")]
    ui.small("Kept for this session only.");

    ui.add_space(coursecraft_render::SPACE_MD);
    egui::CollapsingHeader::new(RichText::new("Audio").strong())
        .id_salt("ai_audio_section")
        .default_open(false)
        .show(ui, |ui| {
            ui.small(
                "Optional. Each audio feature uses its own model; leave one \
                 empty and that feature stays off.",
            );
            ui.horizontal(|ui| {
                ui.label("Speech-to-text model");
                ui.add(
                    TextEdit::singleline(&mut editor.ui.ai.config.speech_to_text_model)
                        .id_salt("ai_speech_to_text_model")
                        .hint_text("whisper-1")
                        .desired_width(240.0),
                );
            });
            ui.horizontal(|ui| {
                ui.label("Text-to-speech model");
                ui.add(
                    TextEdit::singleline(&mut editor.ui.ai.config.text_to_speech_model)
                        .id_salt("ai_text_to_speech_model")
                        .hint_text("tts-1")
                        .desired_width(240.0),
                );
            });
            ui.horizontal(|ui| {
                ui.label("Voice");
                ui.add(
                    TextEdit::singleline(&mut editor.ui.ai.config.voice)
                        .id_salt("ai_voice")
                        .hint_text("alloy")
                        .desired_width(240.0),
                );
            });
        });

    ui.add_space(coursecraft_render::SPACE_MD);
    let checked = check_config(&editor.ui.ai.config);
    if let Err(ref reason) = checked {
        ui.label(RichText::new(reason.to_string()).color(p.danger));
        ui.add_space(coursecraft_render::SPACE_SM);
    }
    checked.is_ok()
}

fn show_agent_source(
    editor: &mut Editor,
    ui: &mut Ui,
    requests: &mut Vec<EditorRequest>,
    id: AgentId,
) -> bool {
    let p = coursecraft_render::palette(ui.ctx());
    if !editor.ui.ai.agents_detected && !editor.ui.ai.detection_running {
        editor.ui.ai.detection_running = true;
        requests.push(EditorRequest::AgentDetect);
    }

    let status = editor
        .ui
        .ai
        .agents
        .iter()
        .find(|row| row.id == id)
        .map(|row| row.status.clone())
        .unwrap_or(AgentStatus::Checking);
    let found = matches!(&status, AgentStatus::Found | AgentStatus::Connected);

    ui.horizontal(|ui| {
        if matches!(&status, AgentStatus::Checking) {
            ui.add(egui::Spinner::new());
        }
        let color = match &status {
            AgentStatus::Connected => p.success,
            AgentStatus::Problem(_) => p.warning,
            AgentStatus::Found | AgentStatus::NotInstalled | AgentStatus::Checking => {
                p.text_secondary
            }
        };
        ui.label(
            RichText::new(status.label())
                .size(coursecraft_render::SIZE_SMALL)
                .color(color),
        );
        if let AgentStatus::Problem(sentence) = &status {
            ui.label(
                RichText::new(sentence)
                    .size(coursecraft_render::SIZE_SMALL)
                    .color(p.warning),
            );
        }
    });
    if matches!(&status, AgentStatus::NotInstalled)
        && crate::quiet_button(ui, "How to install", crate::QuietTone::Accent, true).clicked()
    {
        ui.ctx().open_url(egui::OpenUrl::new_tab(id.install_url()));
    }
    if !found && !matches!(&status, AgentStatus::Checking) {
        ui.small("CourseCraft uses your plan through this app. Nothing is sent until you ask.");
    }

    found
}

fn show_agents_tab(editor: &mut Editor, ui: &mut Ui, requests: &mut Vec<EditorRequest>) {
    let p = coursecraft_render::palette(ui.ctx());

    if !editor.automation_available {
        show_setup_by_hand(editor, ui, true);
        return;
    }

    if !editor.automation_enabled {
        ui.horizontal(|ui| {
            ui.colored_label(
                p.warning,
                "Automation is off, so agents can't reach this app.",
            );
            if crate::quiet_button(ui, "Turn on", crate::QuietTone::Accent, true).clicked() {
                editor.automation_enabled = true;
                requests.push(EditorRequest::SetAutomation(true));
            }
        });
        ui.add_space(coursecraft_render::SPACE_MD);
    }

    ui.label(
        RichText::new("These tools can work on the open course while this app is running.")
            .color(p.text_secondary),
    );
    ui.add_space(coursecraft_render::SPACE_SM);

    if !editor.ui.ai.agents_detected && !editor.ui.ai.detection_running {
        editor.ui.ai.detection_running = true;
        requests.push(EditorRequest::AgentDetect);
    }

    if editor.ui.ai.detection_running {
        ui.horizontal(|ui| {
            ui.add(egui::Spinner::new());
            ui.label(RichText::new("Checking…").color(p.text_secondary));
        });
    }

    let mut retry_detect = false;
    for index in 0..editor.ui.ai.agents.len() {
        let Some((id, click)) = show_agent_row(editor, ui, index) else {
            continue;
        };
        match click {
            RowClick::Action(action) => {
                if let Some(row) = editor.ui.ai.agents.get_mut(index) {
                    row.busy = true;
                    row.last_action = Some(action);
                    row.notice = None;
                }
                requests.push(EditorRequest::AgentAction { agent: id, action });
            }
            RowClick::Detect => retry_detect = true,
        }
    }

    if retry_detect {
        for row in &mut editor.ui.ai.agents {
            if matches!(row.status, AgentStatus::Problem(_)) {
                row.status = AgentStatus::Checking;
            }
        }
        editor.ui.ai.agents_detected = false;
        if !editor.ui.ai.detection_running {
            editor.ui.ai.detection_running = true;
            requests.push(EditorRequest::AgentDetect);
        }
    }

    ui.add_space(coursecraft_render::SPACE_LG);
    show_setup_by_hand(editor, ui, false);
}

/// What a click in an agent row asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RowClick {
    /// Run Connect, Test or Disconnect.
    Action(AgentAction),
    /// No action was recorded for this problem; check again.
    Detect,
}

/// Draw one row; returns the agent and what the author clicked.
fn show_agent_row(editor: &mut Editor, ui: &mut Ui, index: usize) -> Option<(AgentId, RowClick)> {
    let row = editor.ui.ai.agents.get(index)?;
    let id = row.id;
    let name = id.display_name();
    let status = row.status.clone();
    let busy = row.busy;
    let notice = row.notice.clone();
    let last_action = row.last_action;
    let p = coursecraft_render::palette(ui.ctx());
    let can_act =
        !editor.ui.ai.detection_running && !editor.ui.ai.agents.iter().any(|row| row.busy);
    let mut clicked = None;

    egui::Frame::NONE
        .inner_margin(egui::Margin::symmetric(0, 2))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.horizontal(|ui| {
                ui.label(RichText::new(name).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if busy {
                        ui.add(egui::Spinner::new());
                        ui.label(RichText::new("Working…").color(p.text_secondary));
                        return;
                    }
                    match &status {
                        AgentStatus::NotInstalled => {
                            if crate::quiet_button(
                                ui,
                                "How to install",
                                crate::QuietTone::Accent,
                                true,
                            )
                            .clicked()
                            {
                                ui.ctx().open_url(egui::OpenUrl::new_tab(id.install_url()));
                            }
                        }
                        AgentStatus::Found => {
                            if crate::quiet_button(ui, "Connect", crate::QuietTone::Accent, can_act)
                                .clicked()
                            {
                                clicked = Some(RowClick::Action(AgentAction::Connect));
                            }
                        }
                        AgentStatus::Connected => {
                            if crate::quiet_button(
                                ui,
                                "Disconnect",
                                crate::QuietTone::Danger,
                                can_act,
                            )
                            .clicked()
                            {
                                clicked = Some(RowClick::Action(AgentAction::Disconnect));
                            }
                            if crate::quiet_button(ui, "Test", crate::QuietTone::Secondary, can_act)
                                .clicked()
                            {
                                clicked = Some(RowClick::Action(AgentAction::Test));
                            }
                        }
                        AgentStatus::Problem(_) => {
                            if crate::quiet_button(
                                ui,
                                "Try again",
                                crate::QuietTone::Accent,
                                can_act,
                            )
                            .clicked()
                            {
                                clicked = Some(match last_action {
                                    Some(action) => RowClick::Action(action),
                                    None => RowClick::Detect,
                                });
                            }
                        }
                        AgentStatus::Checking => {}
                    }
                });
            });
            ui.horizontal(|ui| {
                let color = match &status {
                    AgentStatus::Connected => p.success,
                    AgentStatus::Problem(_) => p.warning,
                    AgentStatus::Found | AgentStatus::NotInstalled | AgentStatus::Checking => {
                        p.text_secondary
                    }
                };
                if matches!(&status, AgentStatus::Checking) {
                    ui.add(egui::Spinner::new());
                }
                ui.label(
                    RichText::new(status.label())
                        .size(coursecraft_render::SIZE_SMALL)
                        .color(color),
                );
                if let AgentStatus::Problem(sentence) = &status {
                    ui.label(
                        RichText::new(sentence)
                            .size(coursecraft_render::SIZE_SMALL)
                            .color(p.warning),
                    );
                }
            });
            if let Some(notice) = notice {
                ui.small(RichText::new(notice).color(p.success));
            }
        });
    clicked.map(|click| (id, click))
}

fn show_setup_by_hand(editor: &mut Editor, ui: &mut Ui, expanded: bool) {
    let p = coursecraft_render::palette(ui.ctx());
    egui::CollapsingHeader::new(RichText::new("Set up by hand").strong())
        .id_salt("agents_setup_by_hand")
        .default_open(expanded)
        .show(ui, |ui| {
            let Some(exe) = editor.agent_exe_path.clone() else {
                ui.label("This build can't find its own location, so see docs/cli.md to connect.");
                return;
            };
            ui.label(
                RichText::new(
                    "Run a line once in your terminal. After that, your agent can work on the \
                     course whenever this app is open.",
                )
                .color(p.text_secondary),
            );
            if let Some(warning) = crate::connect_agent::exe_location_warning(&exe) {
                ui.add_space(coursecraft_render::SPACE_MD);
                ui.colored_label(p.warning, warning);
            }
            crate::connect_agent::copy_row(
                ui,
                "Claude Code",
                &crate::connect_agent::claude_line(&exe),
            );
            crate::connect_agent::copy_row(ui, "Codex", &crate::connect_agent::codex_line(&exe));
            crate::connect_agent::copy_row(
                ui,
                "Check the CLI",
                &crate::connect_agent::cli_line(&exe),
            );
            crate::connect_agent::copy_row(
                ui,
                "Tell any agent",
                &crate::connect_agent::agent_message(&exe),
            );
            ui.add_space(coursecraft_render::SPACE_LG);
            ui.label(
                coursecraft_render::small(
                    "The CLI and the MCP connection run the same commands. If one fails, an agent \
                     can use the other. Both need this app open to change the open course.",
                )
                .color(p.text_secondary),
            );
        });
}

fn cancel_running(editor: &Editor, requests: &mut Vec<EditorRequest>) {
    if matches!(editor.ui.ai.test, AiTestState::Running { .. }) {
        requests.push(EditorRequest::CancelAi(AiPurpose::TestConnection));
    }
}
