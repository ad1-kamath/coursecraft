//! Preferences storage and recent files tracking.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Maximum number of recent files retained.
pub const MAX_RECENT: usize = 8;

fn default_true() -> bool {
    true
}

/// User theme preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThemePref {
    /// Always use the dark theme (default).
    #[default]
    Dark,
    /// Always use the light theme.
    Light,
    /// Follow the operating system theme (defaults to dark if undetermined).
    System,
}

impl From<coursecraft_ui_egui::coursecraft_render::ThemePref> for ThemePref {
    fn from(p: coursecraft_ui_egui::coursecraft_render::ThemePref) -> Self {
        match p {
            coursecraft_ui_egui::coursecraft_render::ThemePref::Dark => Self::Dark,
            coursecraft_ui_egui::coursecraft_render::ThemePref::Light => Self::Light,
            coursecraft_ui_egui::coursecraft_render::ThemePref::System => Self::System,
        }
    }
}

impl From<ThemePref> for coursecraft_ui_egui::coursecraft_render::ThemePref {
    fn from(p: ThemePref) -> Self {
        match p {
            ThemePref::Dark => coursecraft_ui_egui::coursecraft_render::ThemePref::Dark,
            ThemePref::Light => coursecraft_ui_egui::coursecraft_render::ThemePref::Light,
            ThemePref::System => coursecraft_ui_egui::coursecraft_render::ThemePref::System,
        }
    }
}

impl ThemePref {
    /// Resolve preference to a concrete theme mode given the egui context.
    #[must_use]
    pub fn resolve(
        self,
        ctx: &egui::Context,
    ) -> coursecraft_ui_egui::coursecraft_render::ThemeMode {
        coursecraft_ui_egui::coursecraft_render::ThemePref::from(self).resolve(ctx)
    }
}

/// Optional AI endpoint settings, stored only on the desktop.
///
/// The key is never printed: [`Debug`] shows `***` instead.
#[derive(Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AiPrefs {
    /// What powers the built-in AI features: an endpoint or an installed agent.
    #[serde(default)]
    pub source: coursecraft_ai::AiSource,
    /// Base URL of the OpenAI-compatible endpoint.
    pub base_url: String,
    /// Model name.
    pub model: String,
    /// API key; empty when the server needs none.
    pub api_key: String,
    /// Speech-to-text model, omitted when unset.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub speech_to_text_model: String,
    /// Text-to-speech model, omitted when unset.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text_to_speech_model: String,
    /// Text-to-speech voice, omitted when unset.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub voice: String,
}

impl std::fmt::Debug for AiPrefs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AiPrefs")
            .field("source", &self.source)
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .field("api_key", &"***")
            .finish()
    }
}

impl AiPrefs {
    /// Read the config used by the AI client.
    #[must_use]
    pub fn to_config(&self) -> coursecraft_ai::AiConfig {
        coursecraft_ai::AiConfig {
            base_url: self.base_url.clone(),
            model: self.model.clone(),
            api_key: self.api_key.clone(),
            speech_to_text_model: self.speech_to_text_model.clone(),
            text_to_speech_model: self.text_to_speech_model.clone(),
            voice: self.voice.clone(),
        }
    }

    /// Store a config in prefs.
    #[must_use]
    pub fn from_config(config: &coursecraft_ai::AiConfig) -> Self {
        Self {
            source: coursecraft_ai::AiSource::Endpoint,
            base_url: config.base_url.clone(),
            model: config.model.clone(),
            api_key: config.api_key.clone(),
            speech_to_text_model: config.speech_to_text_model.clone(),
            text_to_speech_model: config.text_to_speech_model.clone(),
            voice: config.voice.clone(),
        }
    }
}

/// Optional ElevenLabs narration settings, stored only on the desktop.
///
/// The key is never printed: [`Debug`] shows `***` instead.
#[derive(Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ElevenLabsPrefs {
    /// API key.
    pub api_key: String,
    /// Voice id used in the speech URL path.
    pub voice_id: String,
    /// Human name of the voice.
    pub voice_name: String,
    /// Model id; empty means the default.
    pub model_id: String,
}

impl std::fmt::Debug for ElevenLabsPrefs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ElevenLabsPrefs")
            .field("api_key", &"***")
            .field("voice_id", &self.voice_id)
            .field("voice_name", &self.voice_name)
            .field("model_id", &self.model_id)
            .finish()
    }
}

impl ElevenLabsPrefs {
    /// Read the config used by the ElevenLabs client.
    #[must_use]
    pub fn to_config(&self) -> coursecraft_ai::ElevenLabsConfig {
        coursecraft_ai::ElevenLabsConfig {
            api_key: self.api_key.clone(),
            voice_id: self.voice_id.clone(),
            voice_name: self.voice_name.clone(),
            model_id: self.model_id.clone(),
        }
    }

    /// Store a config in prefs.
    #[must_use]
    pub fn from_config(config: &coursecraft_ai::ElevenLabsConfig) -> Self {
        Self {
            api_key: config.api_key.clone(),
            voice_id: config.voice_id.clone(),
            voice_name: config.voice_name.clone(),
            model_id: config.model_id.clone(),
        }
    }
}

/// Which service the Narration from Text window used last.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SpeechSourcePref {
    /// The OpenAI-compatible endpoint in AI Settings.
    #[default]
    AiProvider,
    /// The author's own ElevenLabs account.
    ElevenLabs,
}

impl From<SpeechSourcePref> for coursecraft_ui_egui::SpeechSource {
    fn from(source: SpeechSourcePref) -> Self {
        match source {
            SpeechSourcePref::AiProvider => Self::AiProvider,
            SpeechSourcePref::ElevenLabs => Self::ElevenLabs,
        }
    }
}

impl From<coursecraft_ui_egui::SpeechSource> for SpeechSourcePref {
    fn from(source: coursecraft_ui_egui::SpeechSource) -> Self {
        match source {
            coursecraft_ui_egui::SpeechSource::AiProvider => Self::AiProvider,
            coursecraft_ui_egui::SpeechSource::ElevenLabs => Self::ElevenLabs,
        }
    }
}

/// Persisted user preferences.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Prefs {
    #[serde(default)]
    pub recent: Vec<String>,
    #[serde(default = "default_true")]
    pub automation_enabled: bool,
    #[serde(default)]
    pub theme: ThemePref,
    #[serde(default)]
    pub coach_marks_done: bool,
    /// Optional AI endpoint, omitted entirely when there is none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai: Option<AiPrefs>,
    /// Optional ElevenLabs narration settings, omitted when there are none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elevenlabs: Option<ElevenLabsPrefs>,
    /// Which service the Narration from Text window used last.
    #[serde(default)]
    pub speech_source: SpeechSourcePref,
    /// Minutes between autosaves. 0 turns autosave off.
    #[serde(default = "default_autosave_minutes")]
    pub autosave_minutes: u32,
    /// How many earlier saves of each project to keep. 0 keeps none.
    #[serde(default = "default_versions_to_keep")]
    pub versions_to_keep: u32,
    /// Slide size of a new course.
    #[serde(default)]
    pub default_slide: SlidePreset,
}

/// Largest autosave interval, in minutes.
pub const MAX_AUTOSAVE_MINUTES: u32 = 60;

fn default_autosave_minutes() -> u32 {
    1
}

/// Most file versions a project can keep.
pub const MAX_VERSIONS_TO_KEEP: u32 = 100;

fn default_versions_to_keep() -> u32 {
    10
}

/// The slide sizes a new course can start with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SlidePreset {
    #[default]
    Wide16x9,
    Wide16x10,
    Standard4x3,
    Square,
}

impl SlidePreset {
    /// Width and height in pixels.
    #[must_use]
    pub fn size(self) -> (u32, u32) {
        match self {
            Self::Wide16x9 => (1280, 720),
            Self::Wide16x10 => (1280, 800),
            Self::Standard4x3 => (960, 720),
            Self::Square => (1080, 1080),
        }
    }

    /// The preset with this size, if it is one.
    #[must_use]
    pub fn from_size(size: (u32, u32)) -> Option<Self> {
        [
            Self::Wide16x9,
            Self::Wide16x10,
            Self::Standard4x3,
            Self::Square,
        ]
        .into_iter()
        .find(|p| p.size() == size)
    }
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            recent: Vec::new(),
            automation_enabled: true,
            theme: ThemePref::default(),
            coach_marks_done: false,
            ai: None,
            elevenlabs: None,
            speech_source: SpeechSourcePref::default(),
            autosave_minutes: default_autosave_minutes(),
            versions_to_keep: default_versions_to_keep(),
            default_slide: SlidePreset::default(),
        }
    }
}

impl Prefs {
    /// The autosave interval, repaired into range.
    #[must_use]
    pub fn autosave_minutes_clamped(&self) -> u32 {
        self.autosave_minutes.min(MAX_AUTOSAVE_MINUTES)
    }

    /// The number of versions to keep, repaired into range.
    #[must_use]
    pub fn versions_to_keep_clamped(&self) -> u32 {
        self.versions_to_keep.min(MAX_VERSIONS_TO_KEEP)
    }

    /// A new blank course at the default slide size.
    #[must_use]
    pub fn blank_course(&self) -> coursecraft_course::Course {
        let mut course = coursecraft_engine::Session::blank("Untitled course").course;
        let (w, h) = self.default_slide.size();
        course.slide_size = coursecraft_course::Size { w, h };
        course
    }

    /// Add a path to the recent files list, deduplicating and keeping at most [`MAX_RECENT`].
    pub fn add_recent(&mut self, path: &Path) {
        let path_str = path.to_string_lossy().to_string();
        self.recent.retain(|p| p != &path_str);
        self.recent.insert(0, path_str);
        self.recent.truncate(MAX_RECENT);
    }

    /// Remove a path from the recent files list.
    pub fn remove_recent(&mut self, path: &Path) {
        let path_str = path.to_string_lossy().to_string();
        self.recent.retain(|p| p != &path_str);
    }
}

/// Determine the default application configuration directory from environment variables.
#[must_use]
pub fn config_dir_from_env() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME")
            .map(|h| PathBuf::from(h).join("Library/Application Support/CourseCraft"))
    }
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("CourseCraft"))
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
            Some(PathBuf::from(xdg).join("coursecraft"))
        } else if let Some(home) = std::env::var_os("HOME") {
            Some(PathBuf::from(home).join(".config/coursecraft"))
        } else {
            None
        }
    }
}

/// Load preferences from `prefs.json` inside the specified directory.
///
/// Missing or corrupt files safely return default preferences without crashing.
#[must_use]
pub fn load_prefs(config_dir: &Path) -> Prefs {
    let prefs_file = config_dir.join("prefs.json");
    let Ok(data) = std::fs::read(&prefs_file) else {
        return Prefs::default();
    };
    serde_json::from_slice(&data).unwrap_or_default()
}

/// Save preferences to `prefs.json` inside the specified directory.
///
/// # Errors
/// Returns an error if the directory cannot be created or the file cannot be written.
pub fn save_prefs(config_dir: &Path, prefs: &Prefs) -> Result<(), String> {
    if let Err(e) = std::fs::create_dir_all(config_dir) {
        return Err(format!("Failed to create config directory: {e}"));
    }
    let prefs_file = config_dir.join("prefs.json");
    let json = serde_json::to_string_pretty(prefs).map_err(|e| e.to_string())?;
    crate::files::write_atomic(&prefs_file, json.as_bytes())
        .map_err(|e| format!("Failed to write preferences: {e}"))?;

    #[cfg(unix)]
    if prefs.ai.as_ref().is_some_and(|ai| !ai.api_key.is_empty())
        || prefs
            .elevenlabs
            .as_ref()
            .is_some_and(|elevenlabs| !elevenlabs.api_key.is_empty())
    {
        use std::os::unix::fs::PermissionsExt;
        let permissions = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(&prefs_file, permissions)
            .map_err(|e| format!("Failed to secure preferences: {e}"))?;
    }
    Ok(())
}
