//! CourseCraft editor UI (L6).
//!
//! The egui editor: menus generated from the engine's command registry, the
//! slides panel, the canvas and the editing panels. It reads the course to
//! draw and changes it only through `coursecraft_engine::Session::execute`,
//! so undo, atomicity and validity come from the engine. It never touches the
//! filesystem; the app shell handles the `EditorRequest`s it returns.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

#[cfg(not(target_arch = "wasm32"))]
pub use coursecraft_render::IndexFontProvider;
pub use coursecraft_render::SystemFontProvider;

// First, so `tl!` reads as defined before every other module.
pub mod ai_settings;
pub mod alt_text;
pub mod anim_panel;
pub mod anim_preview;
pub mod audio_ai;
pub mod canvas;
pub mod clipboard;
pub mod coach;
pub mod connect_agent;
mod content_region;
pub mod course_map;
pub mod course_settings;
pub mod crop;
pub mod dialog;
pub mod document;
pub mod elevenlabs;
pub mod export_dialog;
pub(crate) mod eyedropper;
pub mod find;
pub mod fonts;
#[macro_use]
pub mod i18n;
pub mod doc_tabs;
pub mod dock;
pub mod file_page;
pub mod grid_panel;
pub mod panels;
pub mod ribbon;
#[cfg(test)]
mod scroll_audit;
pub mod size_panel;

/// `(slide, object, selection_range, cursor_pos)` captured from an active inline text edit.
pub type InlineEditSelection = (String, String, Option<(usize, usize)>, usize);
pub mod font_picker;
pub mod format_painter;
pub mod history;
pub mod input_insert;
pub mod insert_text;
pub mod interaction_gallery;
pub mod interaction_insert;
pub mod layers;
pub mod media_library;
mod media_store;
pub use media_store::MediaStore;
pub mod focus_order;
pub mod menu;
pub mod outline;
pub mod preferences;
pub mod preview;
pub mod project_links;
pub mod properties;
pub mod publish;
pub mod question;
pub mod record_narration;
pub mod report;
pub mod resources_ui;
pub mod rulers;
pub mod selection;
pub mod shape_gallery;
pub mod shortcuts;
pub mod slide_size_dialog;
pub mod slides;
pub mod snap;
pub mod snapshot;
pub mod spell_check;
pub mod state_text_route;
pub mod states;
pub mod status;
pub mod text_format;
pub mod text_styles_ui;
pub mod theme_dialog;
pub mod timeline;
pub mod timeline_playback;
pub mod toolbar;
pub mod tracking_ui;
pub mod translate_ai;
pub mod translation;
pub mod triggers;
pub mod variables;
pub mod version_history;
pub mod web_insert;
pub mod welcome;
pub mod widgets;

#[cfg(test)]
mod alt_text_tests;
#[cfg(test)]
mod audio_ai_tests;
#[cfg(test)]
#[cfg(test)]
mod base_slide_view_tests;
#[cfg(test)]
mod bughunt_insert;
#[cfg(test)]
mod bughunt_m24_canvas;
#[cfg(test)]
mod bughunt_m24_rename;
#[cfg(test)]
mod bughunt_m24_stress;
#[cfg(test)]
mod bughunt_m24b_canvas;
#[cfg(test)]
mod bughunt_m24b_format;
#[cfg(test)]
mod bughunt_m24b_misc;
#[cfg(test)]
mod bughunt_m24b_trigger;
#[cfg(test)]
mod bughunt_m25_templates;
#[cfg(test)]
mod bughunt_m26_a10_question_panel;
#[cfg(test)]
mod bughunt_m26_a11_format_painter;
#[cfg(test)]
mod bughunt_m26_a12_preview_close;
#[cfg(test)]
mod bughunt_m26_a1_text_format;
#[cfg(test)]
mod bughunt_m26_a2_gradients;
#[cfg(test)]
mod bughunt_m26_a4_timeline;
#[cfg(test)]
mod bughunt_m26_a5_shape_preview;
#[cfg(test)]
mod bughunt_m26_a6_dialogs;
#[cfg(test)]
mod bughunt_m26_a7_video;
#[cfg(test)]
mod bughunt_m26_a9_base_slide_view;
#[cfg(test)]
mod bughunt_m26_crosscut;
#[cfg(test)]
mod bughunt_m26_f19_fonts_inline_edit;
#[cfg(test)]
mod bughunt_menus;
#[cfg(test)]
mod bughunt_stress;
#[cfg(test)]
mod bughunt_undo;
#[cfg(test)]
mod canvas_speed_tests;
#[cfg(test)]
mod daily_fixes_tests;
#[cfg(test)]
mod dialog_reopen_tests;
#[cfg(test)]
mod doc_tabs_tests;
#[cfg(test)]
mod dock_tests;
#[cfg(test)]
mod document_tests;
#[cfg(test)]
mod eyedropper_tests;
#[cfg(test)]
mod f22_fill_outline_dash_tests;
#[cfg(test)]
mod f24_exit_animation_labels_tests;
#[cfg(test)]
mod file_page_tests;
#[cfg(test)]
mod fill_style_tests;
#[cfg(test)]
mod find_tests;
#[cfg(test)]
mod fix_a_shell_tests;
#[cfg(test)]
mod fix_b_canvas_tests;
#[cfg(test)]
mod fix_c_fields_tests;
#[cfg(test)]
mod fix_d_ribbon_tests;
#[cfg(test)]
mod font_harness_tests;
#[cfg(test)]
mod font_picker_tests;
#[cfg(test)]
mod guard_counters;
#[cfg(test)]
mod guard_tests;
#[cfg(test)]
mod h7_video_revisit_tests;
#[cfg(test)]
mod history_tests;
#[cfg(test)]
mod i18n_tests;
#[cfg(test)]
mod idle_tests;
#[cfg(test)]
mod issue8_timeline_group_tests;
#[cfg(test)]
mod layout_tests;
#[cfg(test)]
mod m25_input_tests;
#[cfg(test)]
mod m26_text_format_tests;
#[cfg(test)]
mod m27_find_a11y_tests;
#[cfg(test)]
mod m27_layers_questions_tests;
#[cfg(test)]
mod m27_masters_scenes_tests;
#[cfg(test)]
mod m27_timeline_media_tests;
#[cfg(test)]
mod m27_triggers_states_tests;
#[cfg(test)]
mod m27_undo_clipboard_tests;
#[cfg(test)]
mod m29_edit_state_mode_tests;
#[cfg(test)]
mod m29_format_state_routing_tests;
#[cfg(test)]
mod m29_slice2_interactions_tests;
#[cfg(test)]
mod m29_state_extras_ui_tests;
#[cfg(test)]
mod m29_states_tab_tests;
#[cfg(test)]
mod m37_point_text_ui_tests;
#[cfg(test)]
mod m39_f_theme_size_tests;
#[cfg(test)]
mod m39_i_insert_tests;
#[cfg(test)]
mod m39_l_triggers_layers_tests;
#[cfg(test)]
mod m39_p_paste_focus_tests;
#[cfg(test)]
mod m39_t_text_tests;
#[cfg(test)]
mod m48_layer_bar_tests;
#[cfg(test)]
mod m48_nofill_effects_ui_tests;
#[cfg(test)]
mod m48_split_buttons_tests;
#[cfg(test)]
mod m49_group_anim_ui_tests;
#[cfg(test)]
mod metafile_import_tests;
#[cfg(test)]
mod neon_editor_fix_tests;
#[cfg(test)]
mod outline_tests;
#[cfg(test)]
mod panels_tests;
#[cfg(test)]
mod perf_tests;
#[cfg(test)]
mod preview_close_tests;
#[cfg(test)]
mod preview_drawer_tests;
#[cfg(test)]
mod preview_subset_tests;
#[cfg(test)]
mod preview_variables_tests;
#[cfg(test)]
mod preview_window_tests;
#[cfg(test)]
mod publish_tests;
#[cfg(test)]
mod question_panel_tests;
#[cfg(test)]
mod rename_tests;
#[cfg(test)]
mod shape_preview_tests;
#[cfg(test)]
mod slide_delete_tests;
#[cfg(test)]
mod spacing_tests;
#[cfg(test)]
mod spell_check_tests;
#[cfg(test)]
mod style_toggles_tests;
#[cfg(test)]
mod templates_tests;
#[cfg(test)]
pub(crate) mod test_harness;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod text_format_ribbon_tests;
#[cfg(test)]
mod text_format_tests;
#[cfg(test)]
mod text_resize_tests;
#[cfg(test)]
mod timeline_cue_tests;
#[cfg(test)]
mod timeline_f23_tests;
#[cfg(test)]
mod timeline_scrub_tests;
#[cfg(test)]
mod toolbar_menu_tests;
#[cfg(test)]
mod translate_tests;
#[cfg(test)]
mod trigger_editor_stale_tests;
#[cfg(test)]
mod trigger_problems_tests;
#[cfg(test)]
mod trigger_speed_tests;
#[cfg(test)]
mod try_again_tests;
#[cfg(test)]
mod u10_timeline_layers_tests;
#[cfg(test)]
mod u11_triggers_vars_question_tests;
#[cfg(test)]
mod u12_media_fonts_styles_galleries_tests;
#[cfg(test)]
mod u13_core_authoring_dialogs_tests;
#[cfg(test)]
mod u14_ai_dialogs_tests;
#[cfg(test)]
mod u15_status_welcome_tests;
#[cfg(test)]
mod u16_preview_tests;
#[cfg(test)]
mod u19_state_check_tests;
#[cfg(test)]
mod u20_state_image_picker_tests;
#[cfg(test)]
mod u21_publish_status_tests;
#[cfg(test)]
mod u23_shadow_glow_opacity_tests;
#[cfg(test)]
mod u8_inspector_tests;
#[cfg(test)]
mod u9_navigator_canvas_tests;
#[cfg(test)]
mod uiux_allowlist;
#[cfg(test)]
mod uiux_sweep_tests;
#[cfg(test)]
mod ux_fix_a_tests;
#[cfg(test)]
mod var_compare_tests;
#[cfg(test)]
mod video_selection_tests;
#[cfg(test)]
mod walkthrough_tests;

pub use ai_settings::{
    AgentRow, AiProvider, AiSettingsState, AiSettingsTab, AiTestState, show_ai_not_ready,
    show_ai_settings_window, test_connection_request,
};
pub use alt_text::{AltTextState, show_alt_text_window};
pub use audio_ai::{CaptionsState, SpeechState, show_captions_window, show_speech_window};
pub use canvas::{
    CanvasDrag, DragOp, LAYERS_STRIP_HEIGHT, PlacementDrag, ResizeHandle, ZoomLevel,
    canvas_fit_rect, canvas_rect, object_screen_rect, show_canvas_and_layers,
};
pub use coach::show_coach_marks;
pub use course_map::{CourseMapState, MapDrag, show_course_map};
pub use course_settings::{CourseSettingsState, show_course_settings_window};
pub use coursecraft_media;
pub use coursecraft_render::{self, install_fonts};
pub use document::{DocumentDialogState, DocumentRequestState, show_document_dialog};
pub use elevenlabs::{
    ElevenLabsState, SpeechSource, commit_pasted_voice, config_option as elevenlabs_config_option,
    push_save as push_elevenlabs_save, request_models as request_elevenlabs_models,
    request_voices as request_elevenlabs_voices,
};
pub use export_dialog::{
    ExportJobView, ReviewFormat, ReviewOptionsView, show_export_dialog, show_export_review_dialog,
    show_export_status,
};
pub use input_insert::{
    insert_checkbox, insert_dial, insert_radio_group, insert_slider, insert_text_entry,
};
pub use interaction_gallery::{
    GALLERY_WIDTH as INTERACTION_GALLERY_WIDTH, show_interaction_gallery,
    show_interaction_gallery_window,
};
pub use interaction_insert::{insert_marker, insert_scroll_panel};
pub use layers::{
    LAYER_ROW_HEIGHT, LAYERS_PANEL_NARROW_BELOW, LAYERS_PANEL_NARROW_WIDTH, LAYERS_PANEL_WIDTH,
    LayerRename, LayerRowDrag, LayerSettingsState, LayerSettingsTarget, LayersState,
    open_layer_settings, show_layer_settings_window, show_layers_panel,
};
pub use menu::{
    MenuRoute, adapter, adapter_reason, font_has_glyph, format_shortcut, menu_route,
    parse_shortcut, reorder_params, shortcut_label, show_menu_bar,
};
pub use outline::show_outline_dialog;
pub use preview::Preview;
pub use properties::{
    InspectorTab, PROPERTIES_PANEL_WIDTH, PropertiesState, show_properties_panel,
};
pub use publish::{PublishPhase, PublishStandard, PublishState, show_publish_dialog};
pub use question::{QuestionState, next_choice_id, show_question_tab};
pub use selection::Selection;
pub use shape_gallery::{
    GALLERY_MAX_HEIGHT, GALLERY_WIDTH, ShapeGalleryOutput, shape_picker_button, show_shape_gallery,
    show_shape_gallery_category, show_shape_gallery_selected, show_shape_gallery_window,
};
pub use shortcuts::show_shortcuts_dialog;
pub use slides::{
    PANEL_PADDING_X, PANEL_PADDING_Y, SCENE_HEADER_HEIGHT, SLIDE_INFO_HEIGHT, SLIDE_ROW_GAP,
    SLIDES_PANEL_WIDTH, THUMBNAIL_HEIGHT, THUMBNAIL_WIDTH, show_slides_panel, slide_thumbnail_rect,
};
pub use snapshot::{
    SnapshotSelect, WINDOWS, WindowSpec, open_window, snapshot_editor, window_names, window_spec,
};
pub use spell_check::{
    SpelledWord, SpellingIssue, SpellingState, WordMenuOutcome, WordRects, spell_underlines,
    underline_rects,
};
pub use states::{SUGGESTED_STATE_NAMES, StateEditBuffer, StatesState, show_states_section};
pub use status::{STATUS_BAR_HEIGHT, navigate_to_issue_path, show_status_bar};
pub use theme_dialog::{ThemeDialogState, show_theme_dialog_window};
pub use timeline::{
    BottomTab, PLAYHEAD_SNAP_PX, TIMELINE_HEADER_HEIGHT, TIMELINE_PANEL_HEIGHT,
    TIMELINE_PANEL_MAX_FRACTION, TIMELINE_PANEL_MIN_HEIGHT, TimelineDrag, TimelineDragOp,
    TimelineState, format_timeline_ms, format_timeline_readout, show_timeline_panel,
    snapped_playhead_ms,
};
pub use timeline_playback::{
    TimelineMedia, TimelinePlayback, TimelineTrack, TrackSpec, timeline_track_specs,
    timing_includes,
};
pub use toolbar::{TOOLBAR_HEIGHT, show_toolbar};
pub use translate_ai::{TranslateRow, TranslateState, show_translate_window};
pub use translation::show_translation_windows;
pub use triggers::{
    ActionTag, ConditionKind, ConditionNode, EventKindTag, JumpTargetTag, TriggerFormState,
    TriggersState, combo_popup_height, describe, describe_action, describe_condition,
    describe_event, describe_value, layer_name, object_label, object_name,
    show_trigger_editor_window, show_triggers_tab,
};
pub use variables::{VariablesState, show_variables_tab};
pub use web_insert::{insert_web_object, show_web_insert_window};
pub use welcome::{
    ExampleId, LoadedExample, TemplateId, example_course_and_media, show_welcome_screen,
    template_course,
};
pub use widgets::{
    COMBO_MIN_POPUP_HEIGHT, COMBO_MIN_WIDTH, IconTone, icon_button, icon_button_danger,
    icon_button_danger_reason, icon_button_enabled, icon_button_enabled_reason, icon_toggle,
    mixed_toggle, modal_backdrop_color, styled_combo, toolbar_button, toolbar_button_active,
    toolbar_button_enabled, toolbar_caret_button, toolbar_primary_button, toolbar_toggle,
};

use coursecraft_course::{Course, FontStyle, ShapeKind, shape_label};
use coursecraft_engine::{Session, registry};
use coursecraft_playback::{MediaKind, VideoFrame};
use egui::{Key, Modifiers, Panel, Ui};
use serde_json::Value as Json;

pub const TOP_BAR_HEIGHT: f32 = 26.0;

/// What an AI request is for. Later AI features add variants here.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AiPurpose {
    /// The AI Settings dialog's `Test connection` button.
    TestConnection,
    /// The Course from Document window.
    Outline,
    /// Alt text for the image with this media `src`.
    AltText {
        /// The image's key in the media store.
        src: String,
    },
    /// Translation batch number in the current run.
    Translate {
        /// Zero-based batch position, so a late answer cannot land in a later run's batch.
        batch: usize,
    },
    /// Captions generated from the audio or video object on a slide.
    Captions {
        /// The object whose captions will be set.
        object: String,
        /// The slide that owns it.
        slide: String,
    },
    /// Speech generated from text for a slide.
    Speech {
        /// The slide the narration will be added to.
        slide: String,
    },
    /// One page of the author's ElevenLabs voices.
    ElevenLabsVoices {
        /// Zero-based page being fetched.
        page: u32,
    },
    /// The ElevenLabs text-to-speech model list.
    ElevenLabsModels,
    /// A lookup of one ElevenLabs voice by id.
    ElevenLabsVoiceLookup,
}

/// Requests returned to the application shell.
///
/// The editor never touches the filesystem directly; it delegates file operations,
/// previewing, AI calls, and quitting to the host shell.
#[derive(Debug, Clone, PartialEq)]
pub enum EditorRequest {
    New,
    Open,
    Save,
    SaveAs,
    OpenRecent(usize),
    Preview {
        slide: String,
    },
    Quit,
    PickImage,
    PickAudio,
    /// Choose the course-wide background audio track.
    PickBackgroundAudio,
    /// Choose a file (any type) for the resource with this id.
    PickResourceFile {
        resource: String,
    },
    PickVideo,
    PickLottie,
    /// Choose a folder of HTML, CSS, JavaScript and media for a web object.
    PickWebFolder,
    PickCaptions,
    /// Choose one or more `.ttf`/`.otf` font files to add to the course.
    PickFonts,
    /// Choose files to add to the media library without placing them.
    PickMediaFiles,
    /// Choose a file whose bytes replace the media entry `src`.
    ReplaceMediaFile {
        /// The media key to replace, keeping its name.
        src: String,
    },
    PickPptx,
    /// Choose a `.docx`, `.txt` or `.md` file for Course from Document.
    PickDocument,
    ExportTranslation {
        target_language: String,
    },
    ImportTranslation,
    /// Choose a CSV file of quiz questions to import into a question bank.
    ImportQuestionsCsv,
    /// Write an already-filled XLIFF file through the normal save dialog.
    SaveTranslation {
        xliff: String,
    },
    /// Save a rendered PNG, such as a printed certificate slide.
    SaveImage {
        png: Vec<u8>,
        suggested_name: String,
    },
    Publish {
        standard: PublishStandard,
    },
    /// Save a course as an MP4 video (desktop only).
    ExportMp4 {
        /// Video width in pixels.
        width: u32,
        /// Frames per second.
        fps: u32,
    },
    /// Save a PDF or Word document for review (desktop only).
    ExportReview {
        /// PDF or Word.
        format: ReviewFormat,
        /// The review dialog's checkboxes.
        options: ReviewOptionsView,
    },
    ShowPublished,
    /// Serve the last published zip on a local server and open it in the browser (desktop only).
    OpenPublishedInBrowser,
    SetAutomation(bool),
    /// Delete the saved crash note: it was reported or dismissed.
    DiscardCrashNote,
    SetTheme(coursecraft_render::ThemePref),
    /// Preferences › Language: `"auto"` or a language code, to save.
    SetLanguage(String),
    /// The ribbon or docking setting or the panel arrangement changed: save it.
    /// Sent once a gesture ends, never while a panel is being dragged.
    SaveLayout(dock::LayoutPrefs),
    /// Save the autosave interval, the number of file versions to keep and the
    /// default slide size for new courses.
    SetPreferences {
        autosave_minutes: u32,
        versions_to_keep: u32,
        default_slide: (u32, u32),
        /// Whether misspellings are underlined while typing.
        spell_check_as_you_type: bool,
        /// The personal dictionary, saved with the other preferences.
        spell_words: Vec<String>,
        /// Editor performance mode (`Auto` / `Standard` / `Lite`).
        performance_mode: coursecraft_render::PerformanceMode,
    },
    /// File > Open Folder…: open a project folder (kept in git).
    OpenFolder,
    /// File > Save As Project Folder (for git)…
    SaveAsFolder,
    /// File > Version History: list the earlier saves of the open project.
    ShowVersionHistory,
    /// Open the earlier save named `id` as a new, untitled copy.
    OpenVersionCopy {
        id: String,
    },
    /// Delete the earlier save named `id`.
    DeleteVersion {
        id: String,
    },
    /// Save an in-course template into the shared desktop library.
    SaveTemplateToLibrary {
        template_id: String,
    },
    /// Insert a template from the shared desktop library as a new slide.
    InsertLibraryTemplate {
        id: String,
        scene: Option<String>,
        after: Option<String>,
    },
    /// Delete a template from the shared desktop library.
    DeleteLibraryTemplate {
        id: String,
    },
    OpenExample(ExampleId),
    OpenTemplate(TemplateId),
    /// Send a chat request with the values in the AI Settings dialog.
    AiChat {
        /// Which feature is asking.
        purpose: AiPurpose,
        /// The request to send.
        request: coursecraft_ai::ChatRequest,
    },
    /// Drop an in-flight AI call; its late answer is ignored.
    CancelAi(AiPurpose),
    /// Send raw audio or video bytes to be transcribed.
    AiTranscribe {
        /// Which object the captions are for.
        purpose: AiPurpose,
        /// Speech-to-text model.
        model: String,
        /// Media key; also the safe name sent to the server.
        file_name: String,
        /// The file as it is.
        bytes: Vec<u8>,
    },
    /// Send text to be spoken.
    AiSpeech {
        /// Which slide the narration is for.
        purpose: AiPurpose,
        /// Text-to-speech model.
        model: String,
        /// Voice name, or empty for the default.
        voice: String,
        /// What to say.
        text: String,
    },
    /// Persist the settings, or `None` to remove them.
    SaveAiConfig(Option<coursecraft_ai::AiConfig>),
    /// Persist what powers the AI features: an endpoint or an installed agent.
    SaveAiSource(coursecraft_ai::AiSource),
    /// Send text to ElevenLabs to be spoken.
    ElevenLabsSpeech {
        /// Which slide the narration is for.
        purpose: AiPurpose,
        /// What to say.
        text: String,
    },
    /// Fetch one page of ElevenLabs voices.
    ElevenLabsVoices {
        /// Which page is being fetched.
        purpose: AiPurpose,
        /// Token from the previous page, or `None` for the first.
        page_token: Option<String>,
    },
    /// Fetch the ElevenLabs text-to-speech models.
    ElevenLabsModels {
        /// Which feature is asking.
        purpose: AiPurpose,
    },
    /// Look one ElevenLabs voice up by id.
    ElevenLabsVoiceLookup {
        /// Which feature is asking.
        purpose: AiPurpose,
        /// The id as pasted.
        voice_id: String,
    },
    /// Persist ElevenLabs settings and the narration source, or `None` to remove
    /// only the ElevenLabs settings.
    SaveElevenLabs(Option<coursecraft_ai::ElevenLabsConfig>),
    /// Ask the app to check every agent, on a background thread.
    AgentDetect,
    /// Ask the app to run one Connect, Test or Disconnect action.
    AgentAction {
        /// Which agent the action is for.
        agent: coursecraft_ai::agents::AgentId,
        /// What to do.
        action: coursecraft_ai::agents::AgentAction,
    },
    /// Ask the app to list the fonts installed on this computer.
    ListSystemFonts,
    /// Add an installed font family to the course and optionally apply it.
    AddSystemFont {
        /// Family name to look up in the scanned list.
        family: String,
        /// Command id and parameters to run after the font is added.
        apply: Option<(String, serde_json::Value)>,
    },
    /// Ask the desktop host shell to check for a newer release and report the outcome.
    CheckForUpdates,
    /// Persist whether CourseCraft checks for updates automatically on desktop startup.
    SetAutoCheckUpdates(bool),
}

/// An installed font family offered by the picker. No file paths cross into
/// the editor; the app shell keeps those and imports the faces on request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemFontChoice {
    pub name: String,
    pub embedding_restricted: bool,
}

/// Status-bar and Fonts-window note for a font that restricts embedding.
pub const RESTRICTED_FONT_NOTE: &str = "This font's maker restricts embedding. Check your licence allows using it in a published course.";

/// Summary of a validation issue parsed from `course.validate`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationIssueSummary {
    pub severity: String,
    pub path: String,
    pub message: String,
}

/// Summary of one accessibility finding parsed from `course.audit`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditFindingSummary {
    pub severity: String,
    pub check: String,
    pub criterion: String,
    pub slide: Option<String>,
    pub object: Option<String>,
    pub message: String,
    pub fix: String,
}

/// Which editor area the last interaction or keyboard focus belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActiveArea {
    #[default]
    Canvas,
    Navigator,
}

/// An engine command and its parsed keyboard shortcut.
type RegistryShortcut = (&'static coursecraft_engine::Command, egui::KeyboardShortcut);

/// Every engine command that has a shortcut, most specific first (so Cmd+Shift+G
/// is not swallowed by Cmd+G). The registry is static, so this is built once.
fn registry_shortcuts() -> &'static [RegistryShortcut] {
    static TABLE: std::sync::OnceLock<Vec<RegistryShortcut>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table: Vec<(&'static coursecraft_engine::Command, &'static str)> = registry()
            .iter()
            .filter_map(|cmd| cmd.shortcut.map(|shortcut| (cmd, shortcut)))
            .collect();
        table.sort_by_key(|(_, shortcut)| std::cmp::Reverse(shortcut.matches('+').count()));
        table
            .into_iter()
            .filter_map(|(cmd, shortcut)| menu::parse_shortcut(shortcut).map(|sc| (cmd, sc)))
            .collect()
    })
}

/// Repeats of a held arrow key within this time stay one undo step.
const NUDGE_BURST_SECS: f64 = 1.0;

/// The last arrow-key nudge: what it moved and when, to merge a held key's
/// repeats into one undo step.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NudgeBurst {
    /// Session revision right after the nudge.
    pub revision: u64,
    /// Slide and objects that moved.
    pub slide: String,
    pub ids: Vec<String>,
    /// `UiState::current_time` of the nudge.
    pub time: f64,
}

/// A drawing tool in the gallery's Draw group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DrawTool {
    Line,
    Arrow,
    ElbowConnector,
    CurvedConnector,
    Curve,
    Freeform,
    Scribble,
}

impl DrawTool {
    /// Every draw tool, in gallery order.
    pub const ALL: &'static [Self] = &[
        Self::Line,
        Self::Arrow,
        Self::ElbowConnector,
        Self::CurvedConnector,
        Self::Curve,
        Self::Freeform,
        Self::Scribble,
    ];

    /// Plain-English tooltip and gallery label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Line => "Line",
            Self::Arrow => "Arrow",
            Self::ElbowConnector => "Elbow connector",
            Self::CurvedConnector => "Curved connector",
            Self::Curve => "Curve",
            Self::Freeform => "Freeform",
            Self::Scribble => "Scribble",
        }
    }

    /// Base object name, before the counter.
    #[must_use]
    pub const fn base_name(self) -> &'static str {
        self.label()
    }

    /// Whether the tool draws by pressing and dragging straight to an end.
    #[must_use]
    pub const fn is_drag(self) -> bool {
        matches!(
            self,
            Self::Line | Self::Arrow | Self::ElbowConnector | Self::CurvedConnector
        )
    }
}

/// Tools available for inserting objects on the canvas.
#[derive(Debug, Clone, PartialEq)]
pub enum PlacementTool {
    TextBox,
    Button,
    Shape(ShapeKind),
    Draw(DrawTool),
    Image {
        src: String,
        natural_size: (f32, f32),
    },
    Audio {
        src: String,
    },
    Video {
        src: String,
    },
    Lottie {
        src: String,
        natural_size: (f32, f32),
    },
}

impl PlacementTool {
    #[must_use]
    pub fn name(&self) -> String {
        match self {
            Self::TextBox => "Text box".to_owned(),
            Self::Button => "Button".to_owned(),
            Self::Shape(kind) => shape_label(*kind).to_owned(),
            Self::Draw(tool) => tool.label().to_owned(),
            Self::Image { .. } => "Image".to_owned(),
            Self::Audio { .. } => "Audio".to_owned(),
            Self::Video { .. } => "Video".to_owned(),
            Self::Lottie { .. } => "Lottie".to_owned(),
        }
    }
}

/// An image being cropped on the canvas.
///
/// The working window lives here until the user applies it; the course only
/// changes when Apply issues one `object.set_crop` command.
#[derive(Debug, Clone, PartialEq)]
pub struct CropSession {
    pub slide: String,
    pub object: String,
    pub edit: crop::CropEdit,
    /// The working state when crop mode opened. Applying is a no-op while the
    /// edit still matches it, so entering and leaving never writes anything.
    pub entry: crop::CropEdit,
    /// Course revision when the session opened; any undo, redo or replace
    /// bumps it and makes the working window stale.
    pub revision: u64,
}

/// Internal UI and error notification state.
pub struct UiState {
    /// Vertical scroll offset of the slides panel (tracked for tests).
    pub slides_scroll_y: f32,
    /// Slide id last scrolled into view in the slides panel.
    pub slides_scrolled_to: Option<String>,
    /// Scene/slide order at the last seen revision, for keeping place on delete/undo.
    pub(crate) last_known_slides: Vec<(String, Vec<String>)>,
    /// True when the last mutating command was `edit.undo` or `edit.redo`.
    pub(crate) pending_undo_redo: bool,
    /// The last arrow-key nudge, so a held key stays one undo step.
    pub(crate) nudge_burst: Option<NudgeBurst>,
    /// The slide at the moving end of a Shift+arrow range in the slide list.
    pub(crate) slides_cursor: Option<String>,
    /// Frames the slide list has not moved while scrolling the selected slide into view.
    pub(crate) slides_scroll_stalled: u8,
    /// Distance from the selected slide to the top of the slide list at the last scroll step.
    pub(crate) slides_scroll_gap: f32,
    /// The zoom level drawn last frame, to keep the view centre when it changes.
    pub(crate) zoom_drawn: Option<ZoomLevel>,
    /// Ctrl+S pressed in a text field: save once the field has committed.
    pub(crate) save_after_commit: bool,
    /// Active slide-thumbnail drag in the navigator.
    pub slide_drag: Option<slides::SlideDrag>,
    pub last_seen_revision: u64,
    pub last_error: Option<(String, f64)>,
    pub error_history: Vec<String>,
    pub validation_errors: usize,
    pub validation_warnings: usize,
    pub validation_issues: Vec<ValidationIssueSummary>,
    /// Cached trigger validation issues, keyed by canonical trigger path
    /// (e.g. `"/scenes/0/slides/2/triggers/1"` or `"/scenes/0/slides/2/layers/0/triggers/3"`).
    pub trigger_issues_by_path: std::collections::HashMap<String, Vec<ValidationIssueSummary>>,
    pub audit_findings: Vec<AuditFindingSummary>,
    pub audit_manual_checks: Vec<String>,
    pub(crate) audit_dirty: bool,
    pub(crate) audit_dirty_since: Option<f64>,
    pub show_validation_window: bool,
    pub show_error_history_window: bool,
    pub pptx_import_report: Option<String>,
    pub show_export_translation: bool,
    pub translation_target_language: String,
    pub translation_report: Option<String>,
    /// Slides-from-outline window and its Markdown field.
    pub show_outline_dialog: bool,
    pub outline_text: String,
    /// Put the caret in the outline field on the next frame.
    pub outline_focus: bool,
    /// The first slide the last successful import created.
    pub outline_last_first_slide: Option<String>,
    /// A warning shown above the outline field (for example, a cut-off answer).
    pub outline_warning: Option<String>,
    /// Course from Document window and its request state.
    pub document: DocumentDialogState,
    /// Find and Replace window.
    pub find: find::FindState,
    /// Check Spelling window and its current misspelling.
    pub spell_check: spell_check::SpellingState,
    /// Personal dictionary, loaded from preferences at start-up.
    pub spell_words: Vec<String>,
    /// Words ignored for this session only.
    pub spell_ignored: Vec<String>,
    /// Single occurrences ignored in the Check Spelling window this session.
    pub spell_ignored_at: Vec<(coursecraft_course::Location, usize)>,
    /// Misspellings last underlined in the in-place canvas editor (test hook).
    pub spell_inline_debug: Vec<String>,
    /// Underline rects from that frame, used by the right-click hit test.
    pub spell_inline_rects: Vec<spell_check::WordRects>,
    /// Where the word menu was opened, kept while the popup is open.
    pub spell_menu_pos: Option<egui::Pos2>,
    pub publish: PublishState,
    /// File ▸ Export ▸ Video (MP4)… is open.
    pub show_export_video: bool,
    /// Video width picked in the export dialog (1280 or 1920).
    pub export_width: u32,
    /// Frame rate picked in the export dialog (24 or 30).
    pub export_fps: u32,
    /// The running MP4 export, shown in the status bar.
    pub export_job: Option<export_dialog::ExportJobView>,
    /// Sentence shown after an export finishes.
    pub export_status: Option<String>,
    /// File ▸ Export ▸ PDF/Word for review… is open.
    pub show_export_review: bool,
    /// Which review document the dialog is exporting.
    pub review_format: ReviewFormat,
    /// The review dialog's checkboxes.
    pub review_options: ReviewOptionsView,
    pub current_time: f64,
    pub placement_tool: Option<PlacementTool>,
    /// Armed format painter, if any.
    pub format_painter: Option<format_painter::FormatPainter>,
    pub placement_drag: Option<canvas::PlacementDrag>,
    /// Active draw-tool gesture or multi-click session.
    pub draw: Option<canvas::DrawSession>,
    /// Point-editing mode for a path object.
    pub path_edit: Option<canvas::PathEdit>,
    /// Search text in the shape gallery.
    pub shape_gallery_query: String,
    /// Icon rects painted by the last open gallery (test hook).
    pub shape_gallery_rects: Vec<(ShapeKind, egui::Rect)>,
    /// Draw-tool icon rects painted by the last open gallery (test hook).
    pub shape_gallery_draw_rects: Vec<(DrawTool, egui::Rect)>,
    /// Search field rect painted by the last open gallery (test hook).
    pub shape_gallery_search_rect: Option<egui::Rect>,
    /// The Insert menu's floating shape gallery.
    pub show_shape_gallery_window: bool,
    /// Optional category filter for the floating shape gallery.
    pub shape_gallery_category: Option<coursecraft_course::ShapeCategory>,
    /// The Insert ▸ Interaction gallery window.
    pub show_interaction_gallery: bool,
    /// Card rects painted by the last open interaction gallery (test hook).
    pub interaction_gallery_rects: Vec<(&'static str, egui::Rect)>,
    /// Insert ▸ Web object window and its URL field.
    pub show_web_window: bool,
    /// Insert ▸ Symbol and Reference windows (M39 package I).
    pub insert_text: insert_text::InsertTextState,
    pub web_url: String,
    pub web_error: Option<String>,
    pub canvas_drag: Option<CanvasDrag>,
    /// The table cell selected in the inspector and by a canvas click.
    pub table_cell: Option<(usize, usize)>,
    pub inline_edit: Option<canvas::InlineEdit>,
    /// Last observed `(slide, object, selection_range, cursor_pos)` from inline edit.
    pub inline_edit_selection: Option<InlineEditSelection>,
    /// True after a click in the right panel while inline edit is open, so the canvas editor doesn't steal focus back.
    pub inline_edit_yielded_to_panel: bool,
    /// Latest egui-measured `(buffer, (w, h))` for an active point-text inline edit.
    pub(crate) inline_edit_measured_size: Option<(String, (f32, f32))>,
    /// Latest live fitted slide-space rect for an active point-text inline edit.
    pub(crate) inline_edit_live_rect: Option<egui::Rect>,
    /// Image crop mode: the canvas edits a window, Apply writes it in one step.
    pub crop_edit: Option<CropSession>,
    /// Active pointer drag inside crop mode.
    pub(crate) crop_drag: Option<canvas::CropDrag>,
    pub scene_rename: Option<slides::SceneRename>,
    pub slide_rename: Option<slides::SceneRename>,
    pub pending_delete_scene: Option<String>,
    pub properties: PropertiesState,
    pub layers: LayersState,
    pub layer_settings: LayerSettingsState,
    pub states: StatesState,
    pub timeline: TimelineState,
    pub triggers: TriggersState,
    pub variables: VariablesState,
    pub question: QuestionState,
    pub course_settings: CourseSettingsState,
    pub theme_dialog: ThemeDialogState,
    /// Optional AI endpoint settings and the test-connection status.
    pub ai: AiSettingsState,
    /// Suggest Alt Text window and the inspector's one-image request.
    pub alt: AltTextState,
    /// Captions from audio window.
    pub captions: CaptionsState,
    /// Narration from text window.
    pub speech: SpeechState,
    /// Record Narration window.
    pub record_narration: record_narration::RecordNarrationState,
    /// Which service makes narration. Remembered between windows.
    pub speech_source: SpeechSource,
    /// ElevenLabs settings, model list, voice list and paste lookup.
    pub elevenlabs: ElevenLabsState,
    /// Translate with AI window and its batch run.
    pub translate: TranslateState,
    pub trigger_form: Option<TriggerFormState>,
    pub show_course_settings_window: bool,
    pub show_theme_window: bool,
    /// Open the Text styles window on the next frame (it keeps its open flag in the context).
    pub open_text_styles_window: bool,
    pub preview_state: Option<String>,
    /// Active canvas edit-state session (`M29` Slice 2).
    pub editing_state: Option<StateEditSession>,
    /// Remembered non-fullscreen preview window size for the current session.
    pub preview_size: Option<egui::Vec2>,
    pub bottom_tab: BottomTab,
    pub inspector_tab: InspectorTab,
    pub last_selected_object: Option<String>,
    pub zoom: ZoomLevel,
    pub show_welcome: bool,
    pub has_loaded_course: bool,
    pub show_shortcuts_dialog: bool,
    pub coach_marks_done: bool,
    pub current_coach_mark: Option<u8>,
    pub last_slide_rect: Option<egui::Rect>,
    pub last_zoom_hud_rect: Option<egui::Rect>,
    /// Slide point under the pointer when the canvas context menu opened.
    pub canvas_context_pos: Option<egui::Pos2>,
    /// Canvas or navigator: where Copy/Cut/Paste act.
    pub last_active_area: ActiveArea,
    /// Private copy of the clipboard for menus and the web.
    pub clipboard: Option<clipboard::Clipboard>,
    /// The text we wrote to the OS clipboard.
    pub clipboard_text: Option<String>,
    /// Successive pastes of the same clipboard payload.
    pub clipboard_paste_count: u32,
    /// The Paste menu choice that waits for the system clipboard to answer.
    pub pending_paste_mode: Option<clipboard::PasteMode>,
    /// The frame that requested the system paste for `pending_paste_mode`.
    pub pending_paste_frame: u64,
    /// The Paste Special window.
    pub paste_special: clipboard::PasteSpecialState,
    /// The Focus Order window.
    pub focus_order: focus_order::FocusOrderState,
    /// Text waiting to be sent to `Context::copy_text`.
    pub pending_copy_text: Option<String>,
    /// Neutral status message (skipped triggers, paste results).
    pub status_message: Option<(String, f64)>,
    /// Snap moving and resizing objects to smart guides.
    pub snap_to_guides: bool,
    /// Triggers copied in the Triggers tab, for "Paste Triggers".
    pub copied_triggers: Vec<coursecraft_course::Trigger>,
    pub prefs: preferences::PrefsState,
    /// File > Version History window.
    pub versions: version_history::VersionHistoryState,
    /// Report a Problem window, crash prompt and recent status messages.
    pub report: report::ReportState,
    pub slide_size_dialog: slide_size_dialog::SlideSizeState,
    /// Rulers, grid and user guides (editor only).
    pub rulers: rulers::RulerState,
    /// Guides drawn for the active drag, in slide space.
    pub snap_guides: Vec<snap::SnapGuide>,
    /// Equal-spacing guides drawn for the active move, in slide space.
    pub spacing_guides: Vec<snap::SpacingGuide>,
    /// Object temporarily focused out of a group by double-clicking it.
    pub group_focus: Option<String>,
    /// Arrange align actions target the slide rather than the selection.
    pub align_to_slide: bool,
    /// Navigator and canvas show slide masters instead of scenes and slides.
    pub master_view: bool,
    /// Slide selected before entering Master View, restored when leaving.
    pub master_view_return_slide: Option<String>,
    /// Master waiting for a delete confirmation.
    pub pending_delete_master: Option<String>,
    /// Shared library template waiting for a delete confirmation.
    pub confirm_delete_library_template: Option<String>,
    /// In-place rename of a master in the navigator.
    pub master_rename: Option<slides::SceneRename>,
    /// In-place rename of a question bank in the navigator.
    pub bank_rename: Option<slides::SceneRename>,
    /// Question banks expanded in the navigator.
    pub bank_expanded: Vec<String>,
    /// Media Library window and its filter/selection state.
    pub show_media_library: bool,
    pub media_library: media_library::MediaLibraryState,
    /// Fonts manager window.
    pub show_fonts_window: bool,
    /// History window.
    pub show_history: bool,
    /// Installed font families offered by the picker. `None` means not asked yet.
    pub system_fonts: Option<Vec<SystemFontChoice>>,
    /// Licence and embedding facts for course font files, keyed by media path.
    pub font_facts: std::collections::HashMap<String, (bool, Option<String>)>,
    /// Font face waiting for a "remove while in use" confirmation.
    pub pending_remove_font: Option<(String, FontStyle)>,
    /// Course map replaces the slide editor in the centre.
    pub story_view: bool,
    /// The ribbon (M37), drawn above the toolbar when visible.
    pub ribbon: ribbon::RibbonState,
    /// Live animation preview on the canvas (M46 phase 5).
    pub anim_preview: anim_preview::PreviewState,
    /// The panels in a docking workspace (M37), when enabled.
    pub dock: dock::DockState,
    /// The Course Map and opened-slide tabs under the ribbon.
    pub doc_tabs: doc_tabs::DocTabs,
    /// The settings panels: the section a launcher asked for, the panel the
    /// work in hand needs, the floating ones when docking is off.
    pub panels: panels::PanelsState,
    /// The File page, opened from the ribbon's File tab.
    pub file_page: file_page::FilePage,
    /// Transient Course map state.
    pub course_map: CourseMapState,
    /// Cached per-revision / per-media-count lists for the Properties panel.
    pub(crate) properties_lists: PropertiesListCache,
    /// Cached `media_references` for the Media Library window.
    pub(crate) media_refs_cache: MediaRefsCache,
    /// Revision- and fingerprint-keyed thumbnail draw-item cache.
    pub(crate) thumbnail_cache: crate::slides::ThumbnailCache,
    /// Revision-cached slide/scene/bank summaries for the left navigator.
    pub(crate) navigator_index: crate::slides::NavigatorIndex,
    /// Cached main-canvas composed draw items for idle/hover/scroll frames.
    pub(crate) canvas_compose_cache: CanvasComposeCache,
    /// Course revision for which theme colors were last published to egui temp data.
    pub(crate) last_published_theme_rev: Option<u64>,
    /// True once the one-time large-media-library notice has been shown for the open course.
    pub memory_pressure_warned: bool,
    /// Last performance profile applied to `media_cache` and `thumbnail_cache`.
    pub(crate) applied_cache_profile: Option<coursecraft_render::PerfProfile>,
    /// Last performance profile applied to `egui::Context` via `set_render_profile`.
    pub(crate) applied_ctx_profile: Option<coursecraft_render::PerfProfile>,
}

/// Cached composed draw items for the main authoring canvas.
#[derive(Debug)]
pub(crate) struct CanvasComposeCache {
    key: Option<CanvasComposeKey>,
    items: std::sync::Arc<[coursecraft_render::DrawItem]>,
}

impl Default for CanvasComposeCache {
    fn default() -> Self {
        Self {
            key: None,
            items: std::sync::Arc::from([]),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InlineEditTargetKey {
    object: String,
    cell: Option<(usize, usize)>,
    is_extra: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct InlineEditTargetRef<'a> {
    pub(crate) object: &'a str,
    pub(crate) cell: Option<(usize, usize)>,
    pub(crate) is_extra: bool,
}

impl InlineEditTargetKey {
    fn from_ref(target: InlineEditTargetRef<'_>) -> Self {
        Self {
            object: target.object.to_owned(),
            cell: target.cell,
            is_extra: target.is_extra,
        }
    }

    fn as_ref(&self) -> InlineEditTargetRef<'_> {
        InlineEditTargetRef {
            object: self.object.as_str(),
            cell: self.cell,
            is_extra: self.is_extra,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CanvasComposeKey {
    revision: u64,
    slide_id: String,
    layer: Option<String>,
    extra_layers: Vec<String>,
    base_slide_view: coursecraft_course::BaseSlideView,
    selected_marker_id: Option<String>,
    preview_state: Option<(String, String)>,
    editing_state: Option<String>,
    inline_edit_target: Option<InlineEditTargetKey>,
    media_fingerprint: (usize, u64),
}

impl CanvasComposeCache {
    pub(crate) fn clear(&mut self) {
        self.key = None;
        self.items = std::sync::Arc::from([]);
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn get_if_matching(
        &self,
        revision: u64,
        slide_id: &str,
        layer: Option<&str>,
        extra_layers: &[String],
        base_slide_view: coursecraft_course::BaseSlideView,
        selected_marker_id: Option<&str>,
        preview_state: Option<(&str, &str)>,
        editing_state: Option<&str>,
        inline_edit_target: Option<InlineEditTargetRef<'_>>,
        media_fingerprint: (usize, u64),
    ) -> Option<std::sync::Arc<[coursecraft_render::DrawItem]>> {
        let key = self.key.as_ref()?;
        let matches = key.revision == revision
            && key.slide_id == slide_id
            && key.layer.as_deref() == layer
            && key.extra_layers.as_slice() == extra_layers
            && key.base_slide_view == base_slide_view
            && key.selected_marker_id.as_deref() == selected_marker_id
            && key
                .preview_state
                .as_ref()
                .map(|(obj, state)| (obj.as_str(), state.as_str()))
                == preview_state
            && key.editing_state.as_deref() == editing_state
            && key
                .inline_edit_target
                .as_ref()
                .map(InlineEditTargetKey::as_ref)
                == inline_edit_target
            && key.media_fingerprint == media_fingerprint;
        matches.then(|| std::sync::Arc::clone(&self.items))
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn store(
        &mut self,
        revision: u64,
        slide_id: &str,
        layer: Option<&str>,
        extra_layers: Vec<String>,
        base_slide_view: coursecraft_course::BaseSlideView,
        selected_marker_id: Option<&str>,
        preview_state: Option<(&str, &str)>,
        editing_state: Option<&str>,
        inline_edit_target: Option<InlineEditTargetRef<'_>>,
        media_fingerprint: (usize, u64),
        items: std::sync::Arc<[coursecraft_render::DrawItem]>,
    ) {
        self.key = Some(CanvasComposeKey {
            revision,
            slide_id: slide_id.to_owned(),
            layer: layer.map(str::to_owned),
            extra_layers,
            base_slide_view,
            selected_marker_id: selected_marker_id.map(str::to_owned),
            preview_state: preview_state.map(|(obj, state)| (obj.to_owned(), state.to_owned())),
            editing_state: editing_state.map(str::to_owned),
            inline_edit_target: inline_edit_target.map(InlineEditTargetKey::from_ref),
            media_fingerprint,
        });
        self.items = items;
    }
}

/// Cached per-revision / per-media-count lists for the Properties panel.
#[derive(Debug, Default)]
pub struct PropertiesListCache {
    revision: SyncRevision,
    media_count: usize,
    pub media_names: Vec<String>,
    pub slides: Vec<(String, String)>,
    pub font_families: Vec<String>,
}

impl PropertiesListCache {
    /// Ensure the cached lists match `revision`, `media`, and `course`.
    pub fn ensure(
        &mut self,
        revision: u64,
        course: &Course,
        media: &[(String, std::sync::Arc<[u8]>)],
    ) {
        if self.revision.is(revision) && self.media_count == media.len() {
            return;
        }
        self.revision.set(revision);
        self.media_count = media.len();
        self.media_names = media.iter().map(|(name, _)| name.clone()).collect();
        self.slides = course
            .slides()
            .map(|s| (s.id.clone(), s.title.clone()))
            .collect();
        self.font_families = course
            .font_families()
            .into_iter()
            .map(str::to_owned)
            .collect();
    }
}

/// Cached `media_references` for the Media Library window, keyed by revision.
#[derive(Debug)]
pub struct MediaRefsCache {
    revision: SyncRevision,
    pub references: std::sync::Arc<[coursecraft_course::MediaReference]>,
}

impl Default for MediaRefsCache {
    fn default() -> Self {
        Self {
            revision: SyncRevision::default(),
            references: std::sync::Arc::from([]),
        }
    }
}

impl MediaRefsCache {
    /// Ensure `references` matches `revision` and `course`.
    pub fn ensure(&mut self, revision: u64, course: &Course) {
        if self.revision.is(revision) {
            return;
        }
        self.revision.set(revision);
        self.references = std::sync::Arc::from(coursecraft_course::media_references(course));
    }
}

/// Revision a UI state was last synced to. Defaults to "never synced" (`u64::MAX`) so the
/// first frame always syncs, even for a freshly opened course at revision 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncRevision(u64);

impl Default for SyncRevision {
    fn default() -> Self {
        Self(u64::MAX)
    }
}

impl SyncRevision {
    /// True when already synced to `revision`.
    #[must_use]
    pub fn is(&self, revision: u64) -> bool {
        self.0 == revision
    }

    /// Record that the state is synced to `revision`.
    pub fn set(&mut self, revision: u64) {
        self.0 = revision;
    }
}

/// One template saved in the shared desktop library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryTemplateEntry {
    /// Safe file stem under `<config_dir>/templates/`.
    pub id: String,
    /// Display title of the template.
    pub title: String,
}

/// Main CourseCraft editor component.
pub struct Editor {
    pub session: Session,
    pub selection: Selection,
    pub ui: UiState,
    pub preview: Option<Preview>,
    pub recent_files: Vec<String>,
    /// The first slide of each recent course, drawn as its thumbnail on the
    /// start page (the desktop app reads them in the background).
    pub recent_previews: Vec<(String, welcome::RecentPreview)>,
    pub library_templates: Vec<LibraryTemplateEntry>,
    pub library_templates_available: bool,
    pub file_ops_available: bool,
    pub publish_available: bool,
    pub title_info: Option<(String, bool)>,
    pub media: MediaStore,
    pub media_cache: coursecraft_render::MediaCache,
    /// Cached audio waveform peak envelopes (`0.0..=1.0`, <= 2,000 peaks) keyed by media id (`src`).
    pub audio_waveforms: std::collections::HashMap<String, Option<Vec<f32>>>,
    /// Cached audio file durations in ms keyed by media id (`src`).
    pub audio_durations: std::collections::HashMap<String, Option<u64>>,
    /// Cached video durations in ms keyed by media `src`, populated off the UI thread.
    pub video_durations:
        std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, Option<u64>>>>,
    /// What the next picked file is for, if not inserting a new object on the canvas.
    pub pending_image_pick: Option<PendingImagePick>,
    pub pending_requests: Vec<EditorRequest>,
    pub automation_available: bool,
    /// Whether desktop in-app update checks are supported on this host.
    pub updates_available: bool,
    /// This app's own binary, for the Connect an AI agent dialog.
    pub agent_exe_path: Option<String>,
    pub automation_enabled: bool,
    pub automation_port: Option<u16>,
    pub theme_pref: coursecraft_render::ThemePref,
    /// Interface language setting: [`i18n::AUTO`] or a code (docs/ui/localization.md).
    pub language: String,
    /// The ribbon, docking and panel arrangement last saved (or loaded), so a
    /// change is sent once as [`EditorRequest::SaveLayout`].
    saved_layout: (bool, bool, craft_ui::docking::Workspace<dock::Panel>),
    /// User-selected editor performance mode (`Auto` / `Standard` / `Lite`).
    pub performance_mode: coursecraft_render::PerformanceMode,
    /// True when the host shell detected a constrained/low-memory device (e.g. `deviceMemory <= 4`).
    pub low_memory_device: bool,
    /// Timeline playback (authoring-only: no runtime, no document writes).
    pub timeline_playback: timeline_playback::TimelinePlayback,
    /// Session revision the timeline playback last saw; a change stops Play.
    pub timeline_last_revision: u64,
    /// Monotonic timeline clock in milliseconds, from frame deltas.
    pub timeline_mono_ms: u64,
    /// The one speller, built the first time something needs it.
    pub spell: Option<coursecraft_spell::Speller>,
    /// Set once an attempt to build the speller has been made.
    pub spell_tried: bool,
    /// Misspellings cached on the hash of the text they came from.
    pub spell_cache: spell_check::SpellCache,
    /// Bumped whenever the dictionary or ignored words change.
    pub spell_version: u64,
    /// Optional provider of installed desktop fonts (unset on wasm).
    pub system_font_provider: Option<std::sync::Arc<dyn coursecraft_render::SystemFontProvider>>,
}

/// Text tone of a [`quiet_button`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuietTone {
    /// `accent_text` at rest (primary list actions).
    Accent,
    /// `text_secondary` at rest, `text` on hover (row actions).
    Secondary,
    /// `text_secondary` at rest, `danger` on hover (destructive row actions).
    Danger,
}

/// A quiet text button: no fill at rest, the `hover` fill on hover, no border.
///
/// A disabled button is painted in `text_disabled`, never highlights and does not sense clicks.
pub fn quiet_button(ui: &mut Ui, text: &str, tone: QuietTone, enabled: bool) -> egui::Response {
    quiet_button_enabled(ui, text, tone, enabled, "Not available right now")
}

/// A quiet text button with an explicit disabled hover reason.
///
/// Wraps the button in [`Ui::add_enabled_ui`] so `.on_disabled_hover_text` fires reliably in egui.
pub fn quiet_button_enabled(
    ui: &mut Ui,
    text: &str,
    tone: QuietTone,
    enabled: bool,
    disabled_reason: &str,
) -> egui::Response {
    let p = coursecraft_render::palette(ui.ctx());
    let galley = ui.painter().layout_no_wrap(
        text.to_owned(),
        egui::FontId::proportional(coursecraft_render::SIZE_BODY),
        egui::Color32::PLACEHOLDER,
    );
    let pad_x = coursecraft_render::SPACE_MD;
    let size = egui::vec2(
        galley.size().x + 2.0 * pad_x,
        coursecraft_render::CONTROL_COMPACT,
    );
    let resp = ui
        .add_enabled_ui(enabled, |ui| {
            let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
            resp.widget_info(|| {
                egui::WidgetInfo::labeled(egui::Role::Button, enabled, text.to_owned())
            });
            if ui.is_rect_visible(rect) {
                let hovered = enabled && resp.hovered();
                let color = if !enabled {
                    p.text_disabled
                } else {
                    match (tone, hovered) {
                        (QuietTone::Accent, _) => p.accent_text,
                        (QuietTone::Secondary, true) => p.text,
                        (QuietTone::Danger, true) => p.danger,
                        (QuietTone::Secondary | QuietTone::Danger, false) => p.text_secondary,
                    }
                };
                if hovered {
                    ui.painter().rect_filled(
                        rect,
                        egui::CornerRadius::same(coursecraft_render::RADIUS_SM as u8),
                        p.hover,
                    );
                }
                let pos = egui::pos2(rect.left() + pad_x, rect.center().y - galley.size().y * 0.5);
                ui.painter().galley(pos, galley, color);
            }
            resp
        })
        .inner
        .on_disabled_hover_text(disabled_reason);
    if enabled {
        resp.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        resp
    }
}

/// What the next picked file is for, if not inserting a new object on the canvas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingImagePick {
    /// Replace the source file of an existing object (image, audio, video, or captions).
    ReplaceObject(String),
    /// Set the picture fill of a shape or button on a slide.
    PictureFill { slide: String, object: String },
    /// Set the image override (`src`) of an image object's state.
    StateImage {
        slide: String,
        object: String,
        state: String,
    },
    /// Insert a new `StateExtraKind::Image` into an object's state.
    StateExtraImage {
        slide: String,
        object: String,
        state: String,
    },
    /// Replace the image `src` of an existing `StateExtraKind::Image`.
    StateExtraReplace {
        slide: String,
        object: String,
        state: String,
        extra: String,
    },
}

/// A quiet button with `accent_text` color used for primary list actions.
pub fn quiet_primary_button(ui: &mut Ui, text: impl Into<egui::WidgetText>) -> egui::Response {
    let p = coursecraft_render::palette(ui.ctx());
    let btn = egui::Button::new(text.into().color(p.accent_text)).fill(egui::Color32::TRANSPARENT);
    ui.add(btn)
}

/// A quiet button for destructive actions with text colored `danger` on hover only.
pub fn destructive_action_button(ui: &mut Ui, text: &str) -> egui::Response {
    let p = coursecraft_render::palette(ui.ctx());
    let id = ui.auto_id_with(text);
    let hovered = ui
        .data(|d| d.get_temp::<egui::Rect>(id))
        .is_some_and(|r| ui.input(|i| i.pointer.hover_pos().is_some_and(|pos| r.contains(pos))));
    let text_color = if hovered { p.danger } else { p.text_secondary };
    let btn = egui::Button::new(egui::RichText::new(text).color(text_color))
        .fill(egui::Color32::TRANSPARENT);
    let resp = ui.add(btn);
    ui.data_mut(|d| d.insert_temp(id, resp.rect));
    resp
}

/// Active canvas state-editing session (`M29` Slice 2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateEditSession {
    pub slide_id: String,
    pub object_id: String,
    pub state_name: String,
    pub selected_extra: Option<String>,
}

/// Shared tooltip for object-level controls locked during Edit-State Mode.
pub const STATE_LOCK_TOOLTIP: &str = "Shared across all states — exit state editing to change";

impl Editor {
    /// Create a new editor for the given course.
    #[must_use]
    pub fn new(course: Course) -> Self {
        let first_slide = course
            .slides()
            .next()
            .map(|s| s.id.clone())
            .unwrap_or_default();
        let session = Session::new(course);
        let initial_revision = session.revision;
        let initial_slides = selection::slide_layout(&session.course);
        let mut selection = Selection::new(first_slide);
        selection.repair(&session.course);

        let mut editor = Self {
            session,
            selection,
            ui: UiState {
                slides_scroll_y: 0.0,
                slides_scrolled_to: None,
                last_known_slides: initial_slides,
                pending_undo_redo: false,
                nudge_burst: None,
                slides_cursor: None,
                slides_scroll_stalled: 0,
                slides_scroll_gap: f32::NAN,
                zoom_drawn: None,
                save_after_commit: false,
                slide_drag: None,
                last_seen_revision: u64::MAX,
                last_error: None,
                error_history: Vec::new(),
                validation_errors: 0,
                validation_warnings: 0,
                validation_issues: Vec::new(),
                trigger_issues_by_path: std::collections::HashMap::new(),
                audit_findings: Vec::new(),
                audit_manual_checks: Vec::new(),
                audit_dirty: false,
                audit_dirty_since: None,
                show_validation_window: false,
                show_error_history_window: false,
                pptx_import_report: None,
                show_export_translation: false,
                translation_target_language: "fr".to_owned(),
                translation_report: None,
                show_outline_dialog: false,
                outline_text: String::new(),
                outline_focus: false,
                outline_last_first_slide: None,
                outline_warning: None,
                document: DocumentDialogState::default(),
                find: find::FindState::default(),
                spell_check: spell_check::SpellingState::default(),
                spell_words: Vec::new(),
                spell_ignored: Vec::new(),
                spell_ignored_at: Vec::new(),
                spell_inline_debug: Vec::new(),
                spell_inline_rects: Vec::new(),
                spell_menu_pos: None,
                publish: PublishState::default(),
                show_export_video: false,
                export_width: 1280,
                export_fps: 30,
                export_job: None,
                export_status: None,
                show_export_review: false,
                review_format: ReviewFormat::default(),
                review_options: ReviewOptionsView::default(),
                current_time: 0.0,
                placement_tool: None,
                format_painter: None,
                placement_drag: None,
                draw: None,
                path_edit: None,
                shape_gallery_query: String::new(),
                shape_gallery_rects: Vec::new(),
                shape_gallery_draw_rects: Vec::new(),
                shape_gallery_search_rect: None,
                show_shape_gallery_window: false,
                shape_gallery_category: None,
                show_interaction_gallery: false,
                interaction_gallery_rects: Vec::new(),
                show_web_window: false,
                insert_text: insert_text::InsertTextState::default(),
                web_url: String::new(),
                web_error: None,
                canvas_drag: None,
                table_cell: None,
                inline_edit: None,
                inline_edit_selection: None,
                inline_edit_yielded_to_panel: false,
                inline_edit_measured_size: None,
                inline_edit_live_rect: None,
                crop_edit: None,
                crop_drag: None,
                scene_rename: None,
                slide_rename: None,
                pending_delete_scene: None,
                properties: PropertiesState::default(),
                layers: LayersState::default(),
                layer_settings: LayerSettingsState::default(),
                states: StatesState::default(),
                timeline: TimelineState::default(),
                triggers: TriggersState::default(),
                variables: VariablesState::default(),
                question: QuestionState::default(),
                course_settings: CourseSettingsState::default(),
                theme_dialog: ThemeDialogState::default(),
                ai: AiSettingsState::default(),
                alt: AltTextState::default(),
                captions: CaptionsState::default(),
                speech: SpeechState::default(),
                record_narration: record_narration::RecordNarrationState::default(),
                speech_source: SpeechSource::default(),
                elevenlabs: ElevenLabsState::default(),
                translate: TranslateState::default(),
                trigger_form: None,
                show_course_settings_window: false,
                show_theme_window: false,
                open_text_styles_window: false,
                preview_state: None,
                editing_state: None,
                preview_size: None,
                bottom_tab: BottomTab::Timeline,
                inspector_tab: InspectorTab::Slide,
                last_selected_object: None,
                zoom: ZoomLevel::Fit,
                show_welcome: false,
                has_loaded_course: true,
                show_shortcuts_dialog: false,
                coach_marks_done: true,
                current_coach_mark: None,
                last_slide_rect: None,
                last_zoom_hud_rect: None,
                canvas_context_pos: None,
                last_active_area: ActiveArea::Canvas,
                clipboard: None,
                clipboard_text: None,
                pending_paste_mode: None,
                pending_paste_frame: 0,
                paste_special: clipboard::PasteSpecialState::default(),
                focus_order: focus_order::FocusOrderState::default(),
                clipboard_paste_count: 0,
                pending_copy_text: None,
                status_message: None,
                snap_to_guides: true,
                copied_triggers: Vec::new(),
                prefs: preferences::PrefsState::default(),
                versions: version_history::VersionHistoryState::default(),
                report: report::ReportState::default(),
                slide_size_dialog: slide_size_dialog::SlideSizeState::default(),
                rulers: rulers::RulerState::default(),
                snap_guides: Vec::new(),
                spacing_guides: Vec::new(),
                group_focus: None,
                align_to_slide: false,
                master_view: false,
                master_view_return_slide: None,
                pending_delete_master: None,
                confirm_delete_library_template: None,
                master_rename: None,
                bank_rename: None,
                bank_expanded: Vec::new(),
                show_media_library: false,
                media_library: media_library::MediaLibraryState::default(),
                show_fonts_window: false,
                show_history: false,
                system_fonts: None,
                font_facts: std::collections::HashMap::new(),
                pending_remove_font: None,
                story_view: false,
                ribbon: ribbon::RibbonState::default(),
                anim_preview: anim_preview::PreviewState::default(),
                dock: dock::DockState::default(),
                doc_tabs: doc_tabs::DocTabs::default(),
                panels: panels::PanelsState::default(),
                file_page: file_page::FilePage::default(),
                course_map: CourseMapState::default(),
                properties_lists: PropertiesListCache::default(),
                media_refs_cache: MediaRefsCache::default(),
                thumbnail_cache: crate::slides::ThumbnailCache::default(),
                navigator_index: crate::slides::NavigatorIndex::default(),
                canvas_compose_cache: CanvasComposeCache::default(),
                last_published_theme_rev: None,
                memory_pressure_warned: false,
                applied_cache_profile: None,
                applied_ctx_profile: None,
            },
            preview: None,
            recent_files: Vec::new(),
            recent_previews: Vec::new(),
            library_templates: Vec::new(),
            library_templates_available: false,
            file_ops_available: true,
            publish_available: true,
            title_info: None,
            media: MediaStore::new(),
            media_cache: coursecraft_render::MediaCache::new(),
            audio_waveforms: std::collections::HashMap::new(),
            audio_durations: std::collections::HashMap::new(),
            video_durations: std::sync::Arc::new(std::sync::Mutex::new(
                std::collections::HashMap::new(),
            )),
            pending_image_pick: None,
            pending_requests: Vec::new(),
            automation_available: false,
            updates_available: false,
            agent_exe_path: None,
            automation_enabled: true,
            automation_port: None,
            theme_pref: coursecraft_render::ThemePref::Dark,
            language: i18n::AUTO.to_owned(),
            saved_layout: (
                true,
                true,
                craft_ui::docking::Workspace::new(dock::default_layout()),
            ),
            performance_mode: coursecraft_render::PerformanceMode::Auto,
            low_memory_device: false,
            timeline_playback: timeline_playback::TimelinePlayback::create_default(),
            timeline_last_revision: initial_revision,
            timeline_mono_ms: 0,
            spell: None,
            spell_tried: false,
            spell_cache: spell_check::SpellCache::default(),
            spell_version: 0,
            system_font_provider: None,
        };
        editor.sync_perf_profile(None);
        editor.check_revision();
        editor.refresh_audit_now();
        editor
    }

    /// Tell the editor whether the host detected a constrained/low-memory device.
    pub fn set_low_memory_device(&mut self, low: bool) {
        self.low_memory_device = low;
        self.sync_perf_profile(None);
    }

    /// Resolve the active editor [`coursecraft_render::PerfProfile`] for the current course and device.
    #[must_use]
    pub fn resolved_perf_profile(&self) -> coursecraft_render::PerfProfile {
        let slide_count: usize = self
            .session
            .course
            .scenes
            .iter()
            .map(|scene| scene.slides.len())
            .sum();
        coursecraft_render::PerfProfile::for_editor(
            self.performance_mode,
            cfg!(target_arch = "wasm32"),
            self.low_memory_device,
            slide_count,
            self.media.total_bytes(),
        )
    }

    pub(crate) fn sync_perf_profile(&mut self, ctx: Option<&egui::Context>) {
        let profile = self.resolved_perf_profile();
        if self.ui.applied_cache_profile != Some(profile) {
            self.media_cache.apply_perf_profile(&profile);
            self.ui
                .thumbnail_cache
                .set_max_entries(profile.thumbnail_cache_entries);
            self.ui.applied_cache_profile = Some(profile);
        }
        if let Some(ctx) = ctx
            && self.ui.applied_ctx_profile != Some(profile)
        {
            coursecraft_render::set_render_profile(ctx, &profile);
            self.ui.applied_ctx_profile = Some(profile);
        }
    }

    fn check_memory_pressure(&mut self) {
        #[cfg(target_arch = "wasm32")]
        const THRESHOLD: u64 = 128 * 1024 * 1024;
        #[cfg(not(target_arch = "wasm32"))]
        const THRESHOLD: u64 = 512 * 1024 * 1024;

        if !self.ui.memory_pressure_warned && self.media.total_bytes() > THRESHOLD {
            self.ui.memory_pressure_warned = true;
            self.set_status(
                "Large media library detected. Consider saving as a project folder or switching Preferences to Lite mode.",
            );
        }
    }

    /// Register (or clear) the optional system font provider for slide rendering.
    pub fn set_system_font_provider(
        &mut self,
        provider: Option<std::sync::Arc<dyn coursecraft_render::SystemFontProvider>>,
    ) {
        self.system_font_provider = provider;
    }

    /// Set the user's active theme preference.
    pub fn set_theme_pref(&mut self, pref: coursecraft_render::ThemePref) {
        self.theme_pref = pref;
    }

    /// Set the saved interface language: [`i18n::AUTO`] or a code. Applies from
    /// the next frame; an unknown code shows English but is kept.
    pub fn set_language(&mut self, setting: &str) {
        self.language = i18n::normalize(setting);
    }

    /// Apply the saved ribbon, docking and panel settings (app start).
    pub fn set_layout_prefs(&mut self, prefs: &dock::LayoutPrefs) {
        self.ui.ribbon.visible = prefs.ribbon;
        self.ui.dock.enabled = prefs.docked;
        self.ui.dock.workspace = dock::load_workspace(prefs.workspace.as_ref());
        self.saved_layout = (prefs.ribbon, prefs.docked, self.ui.dock.workspace.clone());
    }

    /// The ribbon, docking and panel settings to save.
    #[must_use]
    pub fn layout_prefs(&self) -> dock::LayoutPrefs {
        dock::LayoutPrefs {
            ribbon: self.ui.ribbon.visible,
            docked: self.ui.dock.enabled,
            workspace: serde_json::to_value(&self.ui.dock.workspace).ok(),
        }
    }

    /// The settings to save when they differ from the last saved ones.
    fn layout_changed(&mut self) -> Option<dock::LayoutPrefs> {
        let now = (
            self.ui.ribbon.visible,
            self.ui.dock.enabled,
            &self.ui.dock.workspace,
        );
        if (now.0, now.1, now.2)
            == (
                self.saved_layout.0,
                self.saved_layout.1,
                &self.saved_layout.2,
            )
        {
            return None;
        }
        self.saved_layout = (now.0, now.1, now.2.clone());
        Some(self.layout_prefs())
    }

    /// Update recent files list shown in the File menu.
    pub fn set_recent(&mut self, recent: Vec<String>) {
        self.recent_previews
            .retain(|(path, _)| recent.iter().any(|r| r == path));
        self.recent_files = recent;
    }

    /// Remember the course at `path` (a recent file) for its thumbnail.
    pub fn set_recent_preview(&mut self, path: String, course: &Course) {
        let Some(preview) = welcome::RecentPreview::of(course) else {
            return;
        };
        self.recent_previews.retain(|(p, _)| *p != path);
        self.recent_previews.push((path, preview));
    }

    /// Whether the start page is on screen (its thumbnails are wanted).
    #[must_use]
    pub fn start_page_shown(&self) -> bool {
        self.ui.show_welcome || self.ui.file_page.open
    }

    /// Update the list of templates shown under "My library".
    pub fn set_library_templates(&mut self, templates: Vec<LibraryTemplateEntry>) {
        self.library_templates = templates;
    }

    /// Configure whether the shared desktop template library is available.
    pub fn set_library_templates_available(&mut self, available: bool) {
        self.library_templates_available = available;
    }

    /// Configure whether file open/save operations are enabled.
    ///
    /// Disabled on web targets where the host shell cannot open native file pickers.
    pub fn set_file_ops_available(&mut self, available: bool) {
        self.file_ops_available = available;
    }

    /// Set file name and dirty status displayed in the status bar and window title.
    pub fn set_title_info(&mut self, name: impl Into<String>, dirty: bool) {
        self.title_info = Some((name.into(), dirty));
    }

    /// Tell the Connect an AI agent dialog where this app's own binary is.
    pub fn set_agent_exe_path(&mut self, path: Option<String>) {
        self.agent_exe_path = path;
    }

    /// Configure whether local control channel automation is supported on this platform.
    pub fn set_automation_available(&mut self, available: bool) {
        self.automation_available = available;
    }

    /// Configure whether desktop in-app update checks are supported on this host.
    pub fn set_updates_available(&mut self, available: bool) {
        self.updates_available = available;
    }

    /// Configure whether local control channel automation is enabled.
    pub fn set_automation_enabled(&mut self, enabled: bool) {
        self.automation_enabled = enabled;
    }

    /// Set the listening port of the running control server (None if stopped).
    pub fn set_automation_port(&mut self, port: Option<u16>) {
        self.automation_port = port;
    }

    /// Toggle timeline Play (authoring-only; never writes the document).
    ///
    /// Play from the end restarts at 0, and the inspector's "Add cue at
    /// playhead" reads the playhead the author sees afterwards.
    pub fn toggle_timeline_playback(&mut self, duration_ms: u32) {
        if self.timeline_playback.is_playing() {
            self.timeline_playback.pause_play();
            return;
        }
        let playhead_ms = self.ui.timeline.playhead_ms;
        let now_mono = self.timeline_mono_ms;
        let effective = self
            .timeline_playback
            .start_play(playhead_ms, now_mono, duration_ms);
        self.ui.timeline.playhead_ms = effective;
    }

    /// Replace the timeline backend (tests inject a fake frame source).
    pub fn set_timeline_playback(&mut self, playback: timeline_playback::TimelinePlayback) {
        self.timeline_playback = playback;
    }

    /// Advance timeline playback for this frame: sync the slide's tracks,
    /// stop Play on a document edit or an open preview, handle Space,
    /// tick Play and keep repainting while frames are pending.
    ///
    /// Authoring-only: this never runs triggers or engine commands.
    fn update_timeline_playback(&mut self, ui: &mut Ui) {
        let dt = ui.input(|input| input.stable_dt);
        let dt_ms = if dt.is_finite() && dt > 0.0 {
            (dt * 1000.0).round().clamp(0.0, 60_000.0) as u64
        } else {
            0
        };
        let now_mono = self.timeline_mono_ms.saturating_add(dt_ms);
        self.timeline_mono_ms = now_mono;

        let slide_id = self.selection.slide.clone();
        let duration_ms = self
            .session
            .course
            .slide(&slide_id)
            .map_or(100, |slide| slide.duration_ms.max(100));
        self.timeline_playback.update_clock(now_mono);
        self.timeline_playback.sync_slide_indexed(
            &self.session.course,
            &slide_id,
            duration_ms,
            &self.media,
            Some(self.media.index()),
            Some(self.session.revision),
            self.ui.timeline.playhead_ms,
        );

        if self.session.revision != self.timeline_last_revision {
            self.timeline_last_revision = self.session.revision;
            self.timeline_playback.pause_play();
        }
        if self.preview.is_some() {
            self.timeline_playback.pause_play();
        }

        let text_focus = ui.ctx().egui_wants_keyboard_input()
            || ui.memory(|memory| memory.focused().is_some())
            || self.ui.inline_edit.is_some()
            || self.is_cropping();
        if !text_focus
            && self.preview.is_none()
            && self.ui.bottom_tab == BottomTab::Timeline
            && ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Space))
        {
            self.toggle_timeline_playback(duration_ms);
        }

        if self.timeline_playback.is_playing() {
            let playhead_ms = self.ui.timeline.playhead_ms;
            let next = self
                .timeline_playback
                .tick(playhead_ms, now_mono, duration_ms);
            self.ui.timeline.playhead_ms = next.min(duration_ms);
        }

        let playhead_ms = self.ui.timeline.playhead_ms;
        if self.timeline_playback.needs_repaint(playhead_ms) {
            ui.ctx().request_repaint();
        }
    }

    /// Replace the current course with a new one, resetting the session, selection,
    /// and all transient UI buffers.
    pub fn replace_course(&mut self, course: Course) {
        let first_slide = course
            .slides()
            .next()
            .map(|s| s.id.clone())
            .unwrap_or_default();
        self.session = Session::new(course);
        self.selection = Selection::new(first_slide);
        self.selection.repair(&self.session.course);

        self.ui.slides_scrolled_to = None;
        self.ui.last_known_slides = selection::slide_layout(&self.session.course);
        self.ui.pending_undo_redo = false;
        self.ui.slide_drag = None;
        self.ui.timeline.active_row_drag = None;
        self.ui.timeline.slide_cue_drag = None;
        self.ui.timeline.slide_cue_selected = None;
        self.ui.timeline.slide_cue_rename = None;
        self.audio_waveforms.clear();
        self.audio_durations.clear();
        self.ui.last_seen_revision = u64::MAX;
        self.ui.last_error = None;
        self.ui.error_history.clear();
        self.ui.placement_tool = None;
        self.ui.copied_triggers.clear();
        self.ui.rulers.drag = None;
        self.ui.placement_drag = None;
        self.ui.shape_gallery_query.clear();
        self.ui.shape_gallery_rects.clear();
        self.ui.shape_gallery_search_rect = None;
        self.ui.show_shape_gallery_window = false;
        self.ui.shape_gallery_category = None;
        self.ui.show_interaction_gallery = false;
        self.ui.interaction_gallery_rects.clear();
        self.ui.show_web_window = false;
        self.ui.insert_text.reset();
        self.ui.web_error = None;
        self.ui.canvas_drag = None;
        self.ui.inline_edit = None;
        self.ui.crop_edit = None;
        self.ui.crop_drag = None;
        self.ui.scene_rename = None;
        self.ui.slide_rename = None;
        self.ui.pending_delete_scene = None;
        // The new course starts at revision 0 like the old one, so nothing keyed by revision
        // may survive, and nothing armed on the old course's ids either (`N86`).
        self.ui.properties_lists = PropertiesListCache::default();
        self.ui.media_refs_cache = MediaRefsCache::default();
        self.ui.spell_check = spell_check::SpellingState::default();
        self.ui.slide_size_dialog = slide_size_dialog::SlideSizeState::default();
        self.ui.format_painter = None;
        self.ui.show_fonts_window = false;
        self.ui.show_history = false;
        self.ui.show_validation_window = false;
        self.ui.path_edit = None;
        self.ui.draw = None;
        self.ui.table_cell = None;
        self.ui.anim_preview = anim_preview::PreviewState::default();
        self.ui.record_narration = record_narration::RecordNarrationState::default();
        self.ui.properties = PropertiesState::default();
        self.ui.layers = LayersState::default();
        self.ui.layer_settings = LayerSettingsState::default();
        self.ui.states = StatesState::default();
        self.ui.timeline = TimelineState::default();
        self.ui.triggers = TriggersState::default();
        self.ui.variables = VariablesState::default();
        self.ui.question = QuestionState::default();
        self.ui.course_settings = CourseSettingsState::default();
        self.ui.theme_dialog = ThemeDialogState::default();
        self.ui.paste_special = clipboard::PasteSpecialState::default();
        self.ui.focus_order = focus_order::FocusOrderState::default();
        self.ui.find = find::FindState::default();
        self.ui.trigger_form = None;
        self.ui.show_course_settings_window = false;
        self.ui.show_theme_window = false;
        self.ui.show_outline_dialog = false;
        self.ui.outline_text.clear();
        self.ui.outline_focus = false;
        self.ui.outline_last_first_slide = None;
        self.ui.outline_warning = None;
        // Keep a running Course from Document request: changing the course
        // must not throw away an answer that is already on its way.
        if !self.ui.document.is_running() {
            self.ui.document = DocumentDialogState::default();
        }
        if !self.ui.alt.is_running() {
            self.ui.alt = AltTextState::default();
        }
        if !self.ui.captions.is_running() {
            self.ui.captions = CaptionsState::default();
        }
        if !self.ui.speech.is_running() {
            self.ui.speech = SpeechState::default();
        }
        if !self.ui.translate.is_running() {
            self.ui.translate = TranslateState::default();
        }
        self.ui.preview_state = None;
        self.ui.editing_state = None;
        self.ui.inspector_tab = InspectorTab::Slide;
        self.ui.last_selected_object = None;
        self.ui.zoom = ZoomLevel::Fit;
        // A course really loaded: the start page and the File page have done their job.
        self.ui.show_welcome = false;
        self.ui.file_page.open = false;
        self.ui.has_loaded_course = true;
        self.ui.show_shortcuts_dialog = false;
        self.ui.last_active_area = ActiveArea::Canvas;
        self.ui.clipboard = None;
        self.ui.clipboard_text = None;
        self.ui.clipboard_paste_count = 0;
        self.ui.pending_copy_text = None;
        self.ui.status_message = None;
        self.ui.snap_guides.clear();
        self.ui.spacing_guides.clear();
        self.ui.group_focus = None;
        self.ui.master_view = false;
        self.ui.master_view_return_slide = None;
        self.ui.pending_delete_master = None;
        self.ui.confirm_delete_library_template = None;
        self.ui.master_rename = None;
        self.ui.bank_rename = None;
        self.ui.bank_expanded.clear();
        self.ui.show_media_library = false;
        self.ui.media_library = media_library::MediaLibraryState::default();
        self.ui.story_view = false;
        self.ui.course_map = CourseMapState::default();
        self.ui.doc_tabs = doc_tabs::DocTabs::default();
        self.ui.thumbnail_cache.clear();
        self.ui.navigator_index.clear();
        self.ui.canvas_compose_cache.clear();
        self.ui.last_published_theme_rev = None;
        self.ui.memory_pressure_warned = false;
        self.ui.applied_cache_profile = None;
        self.ui.publish.open = false;
        self.ui.publish.phase = PublishPhase::Idle;
        self.ui.publish.status = None;
        self.ui.show_export_video = false;
        self.ui.export_job = None;
        self.ui.export_status = None;
        self.ui.show_export_review = false;
        self.preview = None;

        self.timeline_playback.reset();
        self.timeline_last_revision = self.session.revision;
        self.timeline_mono_ms = 0;

        self.media.clear();
        self.media_cache = coursecraft_render::MediaCache::new();
        self.sync_perf_profile(None);
        self.pending_image_pick = None;
        self.pending_requests.clear();

        self.check_revision();
    }

    /// Open one of the built-in example courses (Healthy Habits, Workplace Safety Basics or the Feature Tour).
    pub fn open_example(&mut self, id: ExampleId) {
        if let Ok((course, media)) = welcome::example_course_and_media(id) {
            self.replace_course(course);
            self.set_media(media);
            self.ui.show_welcome = false;
            self.ui.has_loaded_course = true;
        }
    }

    /// Open one of the built-in starter template courses.
    pub fn open_template(&mut self, id: TemplateId) {
        if let Ok(course) = welcome::template_course(id) {
            self.replace_course(course);
            self.set_media(Vec::new());
            self.ui.show_welcome = false;
            self.ui.has_loaded_course = true;
        }
    }

    /// Configure whether the welcome screen mode is active.
    pub fn set_welcome(&mut self, welcome: bool) {
        self.ui.show_welcome = welcome;
    }

    /// True when the welcome screen mode is active.
    #[must_use]
    pub fn is_welcome(&self) -> bool {
        self.ui.show_welcome
    }

    /// Configure whether coach marks have been completed or skipped.
    pub fn set_coach_marks_done(&mut self, done: bool) {
        self.ui.coach_marks_done = done;
        if done {
            self.ui.current_coach_mark = None;
        }
    }

    /// True when coach marks have been completed or skipped.
    #[must_use]
    pub fn coach_marks_done(&self) -> bool {
        self.ui.coach_marks_done
    }

    /// Update the in-memory media list (e.g. after opening a project file).
    pub fn set_media(&mut self, media: Vec<(String, std::sync::Arc<[u8]>)>) {
        self.media.set_all(media);
        self.media_cache = coursecraft_render::MediaCache::new();
        self.ui.applied_cache_profile = None;
        self.sync_perf_profile(None);
    }

    /// Append a raw media entry directly into the editor's media store.
    pub fn add_raw_media(&mut self, name: String, bytes: std::sync::Arc<[u8]>) {
        self.media.push(name, bytes);
    }

    pub(crate) fn media_get(&self, src: &str) -> Option<&std::sync::Arc<[u8]>> {
        self.media.get(src)
    }

    pub(crate) fn media_contains(&self, src: &str) -> bool {
        self.media.contains(src)
    }

    /// Access the in-memory media list.
    #[must_use]
    pub fn media(&self) -> &[(String, std::sync::Arc<[u8]>)] {
        self.media.as_slice()
    }

    /// Mark a media-list change as a document change, so autosave and the
    /// unsaved-changes prompt see it. Media entries live outside `course.json`,
    /// so this bumps the revision without adding an undo step.
    pub(crate) fn mark_media_changed(&mut self) {
        self.session.revision = self.session.revision.wrapping_add(1);
        self.ui.last_seen_revision = self.session.revision;
    }

    /// Import one file into the media library without placing it in the course.
    ///
    /// Validation, sanitising and size limits match the Insert pickers. The
    /// entry is added to [`Self::media`] only; the course is untouched.
    pub fn add_media_bytes(&mut self, file_name: &str, bytes: &[u8]) -> Option<String> {
        let lower = file_name.to_ascii_lowercase();
        let is_image = lower.ends_with(".png")
            || lower.ends_with(".jpg")
            || lower.ends_with(".jpeg")
            || lower.ends_with(".webp")
            || lower.ends_with(".gif");
        let is_lottie = lower.ends_with(".json") || lower.ends_with(".lottie");
        let is_audio = lower.ends_with(".mp3")
            || lower.ends_with(".wav")
            || lower.ends_with(".ogg")
            || lower.ends_with(".m4a");
        let is_video = lower.ends_with(".mp4") || lower.ends_with(".webm");
        let is_font = lower.ends_with(".ttf") || lower.ends_with(".otf");

        // The Insert importers arm a placement tool (or replace a queued
        // object). The library wants the bytes only, so keep both untouched.
        let previous_tool = self.ui.placement_tool.take();
        let previous_pick = self.pending_image_pick.take();
        let src = if is_image
            || lower.ends_with(".svg")
            || lower.ends_with(".svgz")
            || coursecraft_media::is_metafile_name(&lower)
        {
            self.import_image(file_name, bytes)
        } else if is_lottie {
            self.import_lottie(file_name, bytes)
        } else if is_audio {
            self.import_audio(file_name, bytes)
        } else if is_video {
            self.import_video(file_name, bytes)
        } else if lower.ends_with(".vtt") {
            self.import_captions(file_name, bytes)
        } else if is_font {
            match coursecraft_media::font_info(bytes) {
                Ok(_) => {
                    let src = sanitize_font_src(file_name, &self.media);
                    self.media.push(src.clone(), bytes.into());
                    Some(src)
                }
                Err(e) => {
                    let msg = format!("Failed to add font '{file_name}': {e}");
                    self.ui.last_error = Some((msg.clone(), self.ui.current_time));
                    self.ui.error_history.push(msg);
                    None
                }
            }
        } else {
            if bytes.len() > Self::MAX_MEDIA_IMPORT_BYTES {
                let msg = format!(
                    "Failed to import '{file_name}': file exceeds maximum allowed size of 200 MiB (was {:.1} MiB)",
                    bytes.len() as f64 / (1024.0 * 1024.0)
                );
                self.ui.last_error = Some((msg.clone(), self.ui.current_time));
                self.ui.error_history.push(msg);
                None
            } else {
                let src = sanitize_media_src(file_name, &self.media);
                self.media.push(src.clone(), bytes.into());
                Some(src)
            }
        };
        self.ui.placement_tool = previous_tool;
        self.pending_image_pick = previous_pick;
        let src = src?;
        self.mark_media_changed();
        self.set_status(format!("Added “{src}”. File operations are not undoable."));
        Some(src)
    }

    /// Replace the bytes of an existing media entry, keeping its name.
    ///
    /// The course is untouched. Images get a fresh media cache so the new
    /// picture is decoded and drawn.
    pub fn replace_media_bytes(&mut self, src: &str, bytes: &[u8]) -> bool {
        if bytes.len() > Self::MAX_MEDIA_IMPORT_BYTES {
            let msg =
                format!("Failed to replace '{src}': file exceeds maximum allowed size of 200 MiB");
            self.ui.last_error = Some((msg.clone(), self.ui.current_time));
            self.ui.error_history.push(msg);
            return false;
        }
        // Clip art replacing an SVG entry is stored as the SVG it converts to.
        let lower = src.to_ascii_lowercase();
        let converted = if lower.ends_with(".svg") && coursecraft_media::is_metafile(bytes) {
            let fonts = CourseFonts {
                faces: &self.session.course.fonts,
                media: &self.media,
            };
            match coursecraft_media::metafile_to_svg(bytes, &fonts) {
                Ok(converted) => Some(converted.svg),
                Err(e) => {
                    let msg = format!("Failed to replace '{src}': {e}");
                    self.ui.last_error = Some((msg.clone(), self.ui.current_time));
                    self.ui.error_history.push(msg);
                    return false;
                }
            }
        } else {
            None
        };
        let bytes = converted.as_deref().unwrap_or(bytes);
        if !self.media.replace(src, bytes.into()) {
            return false;
        }
        self.media_cache = coursecraft_render::MediaCache::new();
        self.audio_waveforms.remove(src);
        self.audio_durations.remove(src);
        self.ensure_audio_waveform(src);
        self.mark_media_changed();
        self.set_status(format!(
            "Replaced “{src}”. File operations are not undoable."
        ));
        true
    }

    /// Return the media assets referenced by any object in the course, suitable for saving.
    /// Unused media is excluded from the result but kept in memory so undo can restore it;
    /// in-memory media is only released when the project is replaced.
    #[must_use]
    pub fn used_media(&self) -> Vec<(String, std::sync::Arc<[u8]>)> {
        let referenced = referenced_media_srcs(&self.session.course);
        let folders = web_folder_prefixes(&self.session.course);
        self.media
            .iter()
            .filter(|(src, _)| {
                referenced.contains(src) || folders.iter().any(|prefix| src.starts_with(prefix))
            })
            .map(|(src, bytes)| (src.clone(), std::sync::Arc::clone(bytes)))
            .collect()
    }

    /// Collect the media assets referenced by a single slide (and its layers).
    #[must_use]
    pub fn slide_media(&self, slide: &coursecraft_course::Slide) -> Vec<(String, Vec<u8>)> {
        let mut referenced = std::collections::HashSet::new();
        let mut web_prefixes = std::collections::BTreeSet::new();
        coursecraft_course::visit_slide_media(slide, &mut |r| {
            if r.role == coursecraft_course::MediaRole::WebFolder {
                referenced.insert(format!("{}/{}", r.src, r.web_entry.unwrap_or("")));
                web_prefixes.insert(format!("{}/", r.src));
            } else if !r.src.is_empty() {
                referenced.insert(r.src.to_owned());
            }
        });
        self.media
            .iter()
            .filter(|(src, _)| {
                referenced.contains(src)
                    || web_prefixes.iter().any(|prefix| src.starts_with(prefix))
            })
            .map(|(src, bytes)| (src.clone(), bytes.to_vec()))
            .collect()
    }

    /// Insert a template slide loaded from the shared library, adding its
    /// media and dispatching `slide.paste`.
    pub fn insert_library_template(
        &mut self,
        mut slide: coursecraft_course::Slide,
        media: Vec<(String, Vec<u8>)>,
        scene: Option<String>,
        after: Option<String>,
    ) {
        if let Some(ref master) = slide.master
            && self.session.course.master(master).is_none()
        {
            slide.master = None;
        }
        let prev_media_len = self.media.len();
        let mut renames = std::collections::HashMap::new();
        let mut media_added = false;
        for (src, bytes) in media {
            if let Some((existing_name, existing)) = self
                .media
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(&src))
            {
                if existing.as_ref() != bytes.as_slice() {
                    let new_src = sanitize_media_src(&src, &self.media);
                    self.media.push(new_src.clone(), bytes.into());
                    renames.insert(src, new_src);
                    media_added = true;
                } else if existing_name != &src {
                    renames.insert(src, existing_name.clone());
                }
            } else {
                self.media.push(src, bytes.into());
                media_added = true;
            }
        }
        if !renames.is_empty() {
            rewrite_slide_media_srcs(&mut slide, &renames);
        }
        let Ok(slide_json) = serde_json::to_value(&slide) else {
            self.media.truncate(prev_media_len);
            return;
        };
        let mut params = serde_json::json!({ "slide": slide_json });
        if let Some(after) = after {
            params["after"] = serde_json::json!(after);
        }
        if let Some(scene) = scene {
            params["scene"] = serde_json::json!(scene);
        }
        if self.run("slide.paste", params).is_some() {
            if media_added {
                self.mark_media_changed();
            }
        } else {
            self.media.truncate(prev_media_len);
        }
    }

    /// Media files the course references but the editor's media store does not
    /// contain, sorted for a stable report. Publishing would fail on these, so
    /// the Publish dialog blocks before building.
    #[must_use]
    pub fn missing_media(&self) -> Vec<String> {
        let referenced = referenced_media_srcs(&self.session.course);
        referenced
            .into_iter()
            .filter(|src| !self.media_contains(src))
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    /// Return the natural size (width, height) of the image `src` if available.
    #[must_use]
    pub fn natural_image_size(&mut self, src: &str) -> Option<(f32, f32)> {
        if let Some(size) = self.media_cache.natural_size(src) {
            return Some(size);
        }
        if let Some(bytes) = self.media_get(src) {
            let lower = src.to_ascii_lowercase();
            if lower.ends_with(".svg") || lower.ends_with(".svgz") {
                if let Some(size) = coursecraft_media::svg_natural_size(bytes) {
                    return Some(size);
                }
            } else if let Ok(img) =
                coursecraft_media::decode_image(bytes, &coursecraft_media::Limits::default())
            {
                return Some((img.width as f32, img.height as f32));
            }
        }
        None
    }

    /// Natural size of `src`, but only when its texture is already uploaded.
    ///
    /// Crop mode draws the source straight from the texture, so entering it
    /// before the picture is loaded would show nothing to crop.
    #[must_use]
    pub fn loaded_image_size(&self, src: &str) -> Option<(f32, f32)> {
        if let Some(size) = self.media_cache.natural_size(src) {
            return Some(size);
        }
        match self.media_cache.get_entry(src) {
            Some(coursecraft_render::MediaEntry::Texture(_, size)) => {
                Some((size[0] as f32, size[1] as f32))
            }
            _ => None,
        }
    }

    /// Queue a request for the host application shell.
    pub fn request(&mut self, req: EditorRequest) {
        self.pending_requests.push(req);
    }

    /// True while an image is being cropped on the canvas.
    #[must_use]
    pub fn is_cropping(&self) -> bool {
        self.ui.crop_edit.is_some()
    }

    /// Enter crop mode for the image `object_id` on the selected slide.
    ///
    /// Needs the loaded picture's natural size: a missing or undecodable file
    /// leaves the editor as it is (the placeholder has nothing to crop).
    pub fn start_crop(&mut self, object_id: &str) -> bool {
        let slide_id = self.selection.slide.clone();
        let found = self
            .session
            .course
            .slide(&slide_id)
            .and_then(|slide| slide.object(object_id))
            .and_then(|(object, _)| match &object.kind {
                coursecraft_course::ObjectKind::Image { src, crop, .. } => Some((
                    src.clone(),
                    crop.map(crop::CropEdit::model_rect),
                    egui::Rect::from_min_size(
                        egui::pos2(object.frame.x, object.frame.y),
                        egui::vec2(object.frame.w, object.frame.h),
                    ),
                )),
                _ => None,
            });
        let Some((src, stored, object_frame)) = found else {
            return false;
        };
        // A full or hostile stored window draws exactly like no crop, so treat
        // it the same here and start from the aspect-fit rectangle.
        let stored = stored.filter(|rect| {
            let stored = coursecraft_course::CropRect::from(*rect);
            stored.is_valid() && !stored.is_full()
        });
        let Some(source_px) = self.loaded_image_size(&src) else {
            return false;
        };
        let drawn_crop = stored.map(|rect| {
            egui::Rect::from_min_size(egui::pos2(rect.x, rect.y), egui::vec2(rect.w, rect.h))
        });
        // With no crop the picture is drawn aspect-fit inside the object's
        // frame, so start from that rectangle with a full crop: opening crop
        // mode moves nothing the author can see.
        let Some(frame) =
            coursecraft_render::paint::image_dest_rect(object_frame, source_px, drawn_crop)
        else {
            return false;
        };
        let Some(edit) = crop::CropEdit::new(stored, frame, source_px) else {
            return false;
        };
        self.ui.inline_edit = None;
        self.ui.placement_drag = None;
        self.ui.canvas_drag = None;
        self.ui.crop_drag = None;
        self.ui.group_focus = None;
        self.selection.set_many([object_id.to_owned()]);
        self.ui.crop_edit = Some(CropSession {
            slide: slide_id,
            object: object_id.to_owned(),
            edit,
            entry: edit,
            revision: self.session.revision,
        });
        true
    }

    /// Leave crop mode without changing the course.
    pub fn cancel_crop(&mut self) {
        self.ui.crop_edit = None;
        self.ui.crop_drag = None;
    }

    /// Leave crop mode and write the working crop as one undo step.
    ///
    /// Nothing is written when the window did not change, so simply opening
    /// and closing crop mode never adds an undo step.
    pub fn apply_crop(&mut self) {
        self.ui.crop_drag = None;
        let Some(session) = self.ui.crop_edit.take() else {
            return;
        };
        if session.slide != self.selection.slide {
            return;
        }
        let crop = session.edit.crop_model();
        let frame = session.edit.frame;
        let Some(slide) = self.session.course.slide(&session.slide) else {
            return;
        };
        let Some((object, _)) = slide.object(&session.object) else {
            return;
        };
        if !matches!(object.kind, coursecraft_course::ObjectKind::Image { .. }) {
            return;
        }
        // Compare with the state crop mode opened in, not with the stored
        // frame: an uncropped image starts from its aspect-fit rectangle, so
        // merely opening and applying must not write that frame back.
        let entry_crop = session.entry.crop_model();
        let unchanged_crop = (entry_crop.x - crop.x).abs() < 1e-6
            && (entry_crop.y - crop.y).abs() < 1e-6
            && (entry_crop.w - crop.w).abs() < 1e-6
            && (entry_crop.h - crop.h).abs() < 1e-6;
        let entry_frame = session.entry.frame;
        let unchanged_frame = (entry_frame.min.x - frame.min.x).abs() < 1e-4
            && (entry_frame.min.y - frame.min.y).abs() < 1e-4
            && (entry_frame.width() - frame.width()).abs() < 1e-4
            && (entry_frame.height() - frame.height()).abs() < 1e-4;
        if unchanged_frame && unchanged_crop {
            return;
        }
        // The window was edited upright; a turned or flipped picture goes back
        // to its place on the slide.
        let frame = canvas::CropXform {
            pivot: egui::pos2(
                object.frame.x + object.frame.w * 0.5,
                object.frame.y + object.frame.h * 0.5,
            ),
            rotation: if object.rotation.is_finite() {
                object.rotation
            } else {
                0.0
            },
            flip_h: object.flip_h,
            flip_v: object.flip_v,
        }
        .world_frame(frame);
        let full = coursecraft_course::CropRect::from(crop).is_full();
        let crop_json = if full {
            Json::Null
        } else {
            serde_json::json!({
                "x": crop.x,
                "y": crop.y,
                "w": crop.w,
                "h": crop.h,
            })
        };
        if let Some(state_session) = self.ui.editing_state.clone()
            && state_session.slide_id == session.slide
            && state_session.object_id == session.object
        {
            let base_has_crop = matches!(
                object.kind,
                coursecraft_course::ObjectKind::Image { crop: Some(_), .. }
            );
            let patch = if full {
                if base_has_crop {
                    serde_json::json!({ "clear_crop": true })
                } else {
                    serde_json::json!({ "crop": Json::Null })
                }
            } else {
                serde_json::json!({ "crop": crop_json })
            };
            self.run(
                "object.patch_state",
                serde_json::json!({
                    "slide": state_session.slide_id,
                    "object": state_session.object_id,
                    "name": state_session.state_name,
                    "patch": patch,
                }),
            );
            return;
        }
        let params = serde_json::json!({
            "slide": session.slide,
            "object": session.object,
            "crop": crop_json,
            "frame": {
                "x": frame.min.x,
                "y": frame.min.y,
                "w": frame.width(),
                "h": frame.height(),
            },
        });
        self.run("object.set_crop", params);
    }

    /// Clear the selected image's crop as one undo step, growing its frame so
    /// the whole picture shows at the same scale with the kept pixels unmoved.
    pub fn reset_crop(&mut self) {
        let (Some(object_id), slide_id) =
            (self.selection.object.clone(), self.selection.slide.clone())
        else {
            return;
        };
        if let Some(state_session) = self.ui.editing_state.clone()
            && state_session.slide_id == slide_id
            && state_session.object_id == object_id
        {
            let base_has_crop = self
                .session
                .course
                .slide(&slide_id)
                .and_then(|slide| slide.object(&object_id))
                .is_some_and(|(object, _)| {
                    matches!(
                        object.kind,
                        coursecraft_course::ObjectKind::Image { crop: Some(_), .. }
                    )
                });
            let patch = if base_has_crop {
                serde_json::json!({ "clear_crop": true })
            } else {
                serde_json::json!({ "crop": Json::Null })
            };
            self.run(
                "object.patch_state",
                serde_json::json!({
                    "slide": state_session.slide_id,
                    "object": state_session.object_id,
                    "name": state_session.state_name,
                    "patch": patch,
                }),
            );
            return;
        }
        let found = self
            .session
            .course
            .slide(&slide_id)
            .and_then(|slide| slide.object(&object_id))
            .and_then(|(object, _)| match &object.kind {
                coursecraft_course::ObjectKind::Image { src, crop, .. } => crop.map(|stored| {
                    (
                        src.clone(),
                        stored,
                        egui::Rect::from_min_size(
                            egui::pos2(object.frame.x, object.frame.y),
                            egui::vec2(object.frame.w, object.frame.h),
                        ),
                    )
                }),
                _ => None,
            });
        let Some((src, stored, frame)) = found else {
            return;
        };
        let target = self
            .natural_image_size(&src)
            .and_then(|source_px| {
                crop::CropEdit::new(Some(crop::CropEdit::model_rect(stored)), frame, source_px)
            })
            .map_or(frame, |edit| edit.reset_frame());
        self.run(
            "object.set_crop",
            serde_json::json!({
                "slide": slide_id,
                "object": object_id,
                "crop": Json::Null,
                "frame": {
                    "x": target.min.x,
                    "y": target.min.y,
                    "w": target.width(),
                    "h": target.height(),
                },
            }),
        );
    }

    /// Display an import report popup window.
    pub fn set_import_report(&mut self, report: impl Into<String>) {
        self.ui.pptx_import_report = Some(report.into());
    }

    /// Display the XLIFF import report in a popup window.
    pub fn set_translation_report(&mut self, report: impl Into<String>) {
        self.ui.translation_report = Some(report.into());
    }

    /// Open the Slides from Outline window and focus its field.
    pub fn open_outline_dialog(&mut self) {
        outline::open(self);
    }

    /// Replace the outline field text (used by headless UI tests).
    #[doc(hidden)]
    pub fn set_outline_text_for_test(&mut self, text: &str) {
        self.ui.outline_text = text.to_owned();
    }

    /// Problems in the current outline field (used by headless UI tests).
    #[doc(hidden)]
    #[must_use]
    pub fn outline_problems_for_test(&self) -> Vec<coursecraft_course::OutlineProblem> {
        match outline::parse(self) {
            Ok(_) => Vec::new(),
            Err(problems) => problems,
        }
    }

    /// Whether Add slides is enabled (used by headless UI tests).
    #[doc(hidden)]
    #[must_use]
    pub fn outline_dialog_can_add(&self) -> bool {
        outline::can_add(self)
    }

    /// Run Add slides for the current field (used by headless UI tests).
    #[doc(hidden)]
    pub fn add_outline_for_test(&mut self) -> bool {
        outline::add(self)
    }

    /// The first slide the last import selected (used by headless UI tests).
    #[doc(hidden)]
    #[must_use]
    pub fn outline_last_first_slide_for_test(&self) -> Option<String> {
        self.ui.outline_last_first_slide.clone()
    }

    /// Whether the outline window is open (used by headless UI tests).
    #[doc(hidden)]
    #[must_use]
    pub fn outline_dialog_open_for_test(&self) -> bool {
        self.ui.show_outline_dialog
    }

    /// Whether the Course from Document window is open (used by headless UI tests).
    #[doc(hidden)]
    #[must_use]
    pub fn document_dialog_open_for_test(&self) -> bool {
        self.ui.document.open
    }

    /// Replace the document field text (used by headless UI tests).
    #[doc(hidden)]
    pub fn set_document_text_for_test(&mut self, text: &str) {
        self.ui.document.text = text.to_owned();
    }

    /// Number of undo steps recorded (used by headless UI tests).
    #[doc(hidden)]
    #[must_use]
    pub fn undo_len_for_test(&self) -> usize {
        self.session.undo_len()
    }

    /// The selected slide id (used by headless UI tests).
    #[doc(hidden)]
    #[must_use]
    pub fn selected_slide_for_test(&self) -> String {
        self.selection.slide.clone()
    }

    /// Paste PNG bytes as a picture on the current slide, centred and selected.
    ///
    /// The bytes go through [`Self::import_image`] so they are validated and
    /// stored under a unique name. The picture is then created with a single
    /// `object.add` command, so one undo removes it. Returns `false` without
    /// changing the course in Master View or when no slide is selected.
    pub fn paste_image_png(&mut self, png_bytes: &[u8]) -> bool {
        if self.ui.master_view {
            self.set_status("Not available in slide masters");
            return false;
        }
        let slide_id = self.selection.slide.clone();
        if self.session.course.slide(&slide_id).is_none() {
            self.set_status("Select a slide to paste into");
            return false;
        }

        // import_image validates, stores under a unique name and reports the
        // natural size through the placement tool. Paste always inserts a new
        // picture, so a pending "replace" intent must not consume the import.
        let pending_pick = self.pending_image_pick.take();
        self.ui.placement_tool = None;
        let Some(src) = self.import_image("pasted-image.png", png_bytes) else {
            self.pending_image_pick = pending_pick;
            return false;
        };
        let Some(PlacementTool::Image { natural_size, .. }) = self.ui.placement_tool.take() else {
            return false;
        };

        let slide_size = self.session.course.slide_size;
        let slide_w = slide_size.w as f32;
        let slide_h = slide_size.h as f32;
        let (mut w, mut h) = natural_size;
        if !w.is_finite() || !h.is_finite() || w <= 0.0 || h <= 0.0 {
            w = 320.0;
            h = 180.0;
        }
        if (w > slide_w || h > slide_h)
            && slide_w.is_finite()
            && slide_h.is_finite()
            && slide_w > 0.0
            && slide_h > 0.0
        {
            // Shrink to fit while keeping the picture's aspect ratio.
            let scale = (slide_w / w).min(slide_h / h);
            w = (w * scale).round().max(1.0);
            h = (h * scale).round().max(1.0);
        }
        let x = ((slide_w - w) * 0.5).max(0.0).round();
        let y = ((slide_h - h) * 0.5).max(0.0).round();

        let mut params = serde_json::json!({
            "slide": slide_id,
            "kind": { "type": "image", "src": src, "alt": "" },
            "frame": { "x": x, "y": y, "w": w.round().max(1.0), "h": h.round().max(1.0) },
        });
        if let Some(ref layer) = self.selection.layer {
            params["layer"] = serde_json::json!(layer);
        }
        let Some(result) = self.run("object.add", params) else {
            return false;
        };
        result
            .get("id")
            .and_then(serde_json::Value::as_str)
            .is_some()
    }

    /// Import EMF or WMF clip art: converted to an SVG that keeps its
    /// vectors (text as outlines, in the course's fonts or Inter), then
    /// imported as that SVG under the same name with `.svg`.
    fn import_metafile(&mut self, file_name: &str, bytes: &[u8]) -> Option<String> {
        let fonts = CourseFonts {
            faces: &self.session.course.fonts,
            media: &self.media,
        };
        let converted = match coursecraft_media::metafile_to_svg(bytes, &fonts) {
            Ok(converted) => converted,
            Err(e) => {
                let msg = format!("Failed to import image '{file_name}': {e}");
                self.ui.last_error = Some((msg.clone(), self.ui.current_time));
                self.ui.error_history.push(msg);
                return None;
            }
        };
        let svg_name = coursecraft_media::metafile_svg_name(file_name);
        let src = self.import_image(&svg_name, &converted.svg)?;
        let mut lost = Vec::new();
        if converted.skipped_records > 0 {
            lost.push(format!(
                "{} drawing records CourseCraft can't read",
                converted.skipped_records
            ));
        }
        if converted.missing_glyphs > 0 {
            lost.push(format!(
                "{} characters no font draws",
                converted.missing_glyphs
            ));
        }
        if !lost.is_empty() {
            let msg = format!(
                "Imported “{file_name}” as “{src}” without {}.",
                lost.join(" and ")
            );
            self.ui.error_history.push(msg.clone());
            self.set_status(msg);
        }
        Some(src)
    }

    /// Import an image from raw bytes, validating with the decoder and learning natural dimensions.
    ///
    /// Stores the bytes under a sanitized, unique `src` key. If an image object is queued for replacement,
    /// it updates that object; otherwise, it arms the Image placement tool.
    pub fn import_image(&mut self, file_name: &str, bytes: &[u8]) -> Option<String> {
        let lower = file_name.to_ascii_lowercase();
        let is_svg = lower.ends_with(".svg") || lower.ends_with(".svgz");
        if is_metafile_import(file_name, bytes) {
            return self.import_metafile(file_name, bytes);
        }
        let (natural_width, natural_height) = if is_svg {
            let Some((width, height)) = coursecraft_media::svg_natural_size(bytes) else {
                let msg = format!("Failed to import image '{file_name}': invalid SVG");
                self.ui.last_error = Some((msg.clone(), self.ui.current_time));
                self.ui.error_history.push(msg);
                return None;
            };
            let limits = coursecraft_media::Limits::default();
            let target = (
                width.round().clamp(1.0, u32::MAX as f32) as u32,
                height.round().clamp(1.0, u32::MAX as f32) as u32,
            );
            if let Err(e) = coursecraft_media::decode_svg(bytes, target, &limits) {
                let msg = format!("Failed to import image '{file_name}': {e}");
                self.ui.last_error = Some((msg.clone(), self.ui.current_time));
                self.ui.error_history.push(msg);
                return None;
            }
            (width, height)
        } else {
            let decoded =
                match coursecraft_media::decode_image(bytes, &coursecraft_media::Limits::default())
                {
                    Ok(img) => img,
                    Err(e) => {
                        let msg = format!("Failed to import image '{file_name}': {e}");
                        self.ui.last_error = Some((msg.clone(), self.ui.current_time));
                        self.ui.error_history.push(msg);
                        return None;
                    }
                };
            if (lower.ends_with(".gif") || lower.ends_with(".webp"))
                && let Err(e) = coursecraft_media::decode_animation(
                    bytes,
                    &coursecraft_media::Limits::default(),
                )
            {
                // The first frame still shows; say why the animation won't play.
                let msg =
                    format!("Animated image “{file_name}” will show only its first frame: {e}");
                self.ui.error_history.push(msg.clone());
                self.set_status(msg);
            }
            (decoded.width as f32, decoded.height as f32)
        };

        let pending_pick = self.pending_image_pick.take();
        if let Some(PendingImagePick::StateImage {
            slide: slide_id,
            object: object_id,
            state: state_name,
        }) = pending_pick
        {
            let state_exists = self
                .session
                .course
                .slide(&slide_id)
                .and_then(|s| s.object(&object_id))
                .is_some_and(|(o, _)| o.states.iter().any(|st| st.name == state_name));
            if !state_exists {
                return None;
            }
            let src = sanitize_media_src(file_name, &self.media);
            let arc_bytes: std::sync::Arc<[u8]> = bytes.into();
            self.media.push(src.clone(), arc_bytes);
            if self
                .run(
                    "object.patch_state",
                    serde_json::json!({
                        "slide": slide_id,
                        "object": object_id,
                        "name": state_name,
                        "patch": { "src": src },
                    }),
                )
                .is_none()
            {
                self.media.retain(|(name, _)| name != &src);
                return None;
            }
            if is_svg && bytes.windows(5).any(|w| w.eq_ignore_ascii_case(b"<text")) {
                self.set_status(
                    "This SVG has text, which CourseCraft draws only as shapes. Convert text to outlines in your drawing app.",
                );
            }
            return Some(src);
        }
        if let Some(PendingImagePick::StateExtraImage {
            slide: slide_id,
            object: object_id,
            state: state_name,
        }) = pending_pick
        {
            let parent_frame = self
                .session
                .course
                .slide(&slide_id)
                .and_then(|s| s.object(&object_id))
                .and_then(|(o, _)| {
                    o.states
                        .iter()
                        .any(|st| st.name == state_name)
                        .then_some(o.frame)
                });
            let parent_frame = parent_frame?;
            let src = sanitize_media_src(file_name, &self.media);
            let arc_bytes: std::sync::Arc<[u8]> = bytes.into();
            self.media.push(src.clone(), arc_bytes);

            let (mut w, mut h) = (natural_width, natural_height);
            if !w.is_finite() || !h.is_finite() || w <= 0.0 || h <= 0.0 {
                w = 120.0;
                h = 90.0;
            }
            let max_w = parent_frame.w.clamp(48.0, 240.0);
            let max_h = parent_frame.h.clamp(48.0, 240.0);
            if w > max_w || h > max_h {
                let scale = (max_w / w).min(max_h / h);
                w = (w * scale).round().max(16.0);
                h = (h * scale).round().max(16.0);
            }
            let rel_x = ((parent_frame.w - w) * 0.5).round();
            let rel_y = ((parent_frame.h - h) * 0.5).round();
            let res = self.run(
                "object.state_add_extra",
                serde_json::json!({
                    "slide": slide_id,
                    "object": object_id,
                    "state": state_name,
                    "extra": {
                        "name": file_name,
                        "kind": {
                            "type": "image",
                            "src": src,
                            "alt": ""
                        },
                        "frame": {
                            "x": rel_x,
                            "y": rel_y,
                            "w": w.round().max(1.0),
                            "h": h.round().max(1.0)
                        }
                    }
                }),
            );
            let Some(res) = res else {
                self.media.retain(|(name, _)| name != &src);
                return None;
            };
            if let Some(id) = res.get("id").and_then(serde_json::Value::as_str)
                && let Some(session) = self.ui.editing_state.as_mut()
                && session.slide_id == slide_id
                && session.object_id == object_id
                && session.state_name == state_name
            {
                session.selected_extra = Some(id.to_owned());
            }
            if is_svg && bytes.windows(5).any(|w| w.eq_ignore_ascii_case(b"<text")) {
                self.set_status(
                    "This SVG has text, which CourseCraft draws only as shapes. Convert text to outlines in your drawing app.",
                );
            }
            return Some(src);
        }

        let src = sanitize_media_src(file_name, &self.media);
        let arc_bytes: std::sync::Arc<[u8]> = bytes.into();
        self.media.push(src.clone(), arc_bytes);

        match pending_pick {
            Some(PendingImagePick::PictureFill {
                slide: slide_id,
                object: object_id,
            }) if self.selection.slide == slide_id
                && self.selection.object.as_deref() == Some(object_id.as_str()) =>
            {
                self.apply_picture_fill(&slide_id, &object_id, &src);
            }
            Some(PendingImagePick::ReplaceObject(object_id)) => {
                let current_alt = self
                    .session
                    .course
                    .slide(&self.selection.slide)
                    .and_then(|s| s.objects.iter().find(|o| o.id == object_id))
                    .and_then(|o| match &o.kind {
                        coursecraft_course::ObjectKind::Image { alt, .. } => Some(alt.clone()),
                        _ => None,
                    })
                    .unwrap_or_default();

                self.run(
                    "object.update",
                    serde_json::json!({
                        "slide": self.selection.slide,
                        "object": object_id,
                        "kind": {
                            "type": "image",
                            "src": src,
                            "alt": current_alt,
                        }
                    }),
                );
            }
            Some(PendingImagePick::StateExtraReplace {
                slide: slide_id,
                object: object_id,
                state: state_name,
                extra: extra_id,
            }) => {
                if let Some(mut kind) = self
                    .session
                    .course
                    .slide(&slide_id)
                    .and_then(|s| s.object(&object_id))
                    .and_then(|(o, _)| o.states.iter().find(|st| st.name == state_name))
                    .and_then(|st| st.extras.iter().find(|e| e.id == extra_id))
                    .map(|e| e.kind.clone())
                    && let coursecraft_course::StateExtraKind::Image { src: ref mut s, .. } = kind
                {
                    *s = src.clone();
                    self.run(
                        "object.state_update_extra",
                        serde_json::json!({
                            "slide": slide_id,
                            "object": object_id,
                            "state": state_name,
                            "extra": extra_id,
                            "patch": { "kind": kind },
                        }),
                    );
                }
            }
            _ => {
                self.ui.placement_tool = Some(PlacementTool::Image {
                    src: src.clone(),
                    natural_size: (natural_width, natural_height),
                });
            }
        }

        if is_svg && bytes.windows(5).any(|w| w.eq_ignore_ascii_case(b"<text")) {
            self.set_status(
                "This SVG has text, which CourseCraft draws only as shapes. Convert text to outlines in your drawing app.",
            );
        }

        Some(src)
    }

    /// Make picture `src` the fill of shape or button `object_id`, keeping its
    /// picture fit when it already has a picture fill.
    fn apply_picture_fill(&mut self, slide_id: &str, object_id: &str, src: &str) {
        let mode = self
            .session
            .course
            .slide(slide_id)
            .and_then(|slide| slide.object(object_id))
            .and_then(|(obj, _)| match &obj.kind {
                coursecraft_course::ObjectKind::Shape { fill_style, .. }
                | coursecraft_course::ObjectKind::Button { fill_style, .. } => fill_style.clone(),
                _ => None,
            })
            .and_then(|style| match style {
                coursecraft_course::FillStyle::Picture { mode, .. } => Some(mode),
                _ => None,
            })
            .unwrap_or(coursecraft_course::PictureFillMode::Stretch);
        let style = coursecraft_course::FillStyle::Picture {
            src: src.to_owned(),
            mode,
            tile_scale: [1.0, 1.0],
        };
        let fill_style = serde_json::to_value(&style).unwrap_or(serde_json::Value::Null);
        self.run(
            "object.set_fill_style",
            serde_json::json!({
                "slide": slide_id,
                "object": object_id,
                "fill_style": fill_style,
            }),
        );
    }

    /// Maximum import size allowed for a media file (200 MiB).
    pub const MAX_MEDIA_IMPORT_BYTES: usize = 200 * 1024 * 1024;

    /// Import an audio file into the editor's media library and arm placement or replace selected object.
    pub fn import_audio(&mut self, file_name: &str, bytes: &[u8]) -> Option<String> {
        let src = self.import_media(MediaKind::Audio, file_name, bytes)?;
        self.ensure_audio_waveform(&src);
        Some(src)
    }

    /// Compute (once) and return the cached peak envelope for audio `src`.
    pub(crate) fn ensure_audio_waveform(&mut self, src: &str) -> Option<&[f32]> {
        if !self.audio_waveforms.contains_key(src) {
            let found = self.media.iter().find(|(name, _)| name == src);
            let peaks = found.and_then(|(_, bytes)| {
                coursecraft_playback::audio_peak_envelope(
                    src,
                    bytes,
                    coursecraft_playback::MAX_WAVEFORM_PEAKS,
                )
            });
            let dur = found.and_then(|(_, bytes)| {
                coursecraft_playback::media_duration_ms(MediaKind::Audio, src, bytes)
            });
            self.audio_waveforms.insert(src.to_owned(), peaks);
            self.audio_durations.insert(src.to_owned(), dur);
        }
        self.audio_waveforms.get(src).and_then(|v| v.as_deref())
    }

    /// Import a video file into the editor's media library and arm placement or replace selected object.
    pub fn import_video(&mut self, file_name: &str, bytes: &[u8]) -> Option<String> {
        let src = self.import_media(MediaKind::Video, file_name, bytes)?;
        #[cfg(not(target_arch = "wasm32"))]
        if let Some((_, stored_bytes)) = self.media.iter().find(|(name, _)| name == &src) {
            let cache = std::sync::Arc::clone(&self.video_durations);
            let key = src.clone();
            let bytes = std::sync::Arc::clone(stored_bytes);
            std::thread::spawn(move || {
                let ms = coursecraft_playback::media_duration_ms(MediaKind::Video, &key, &bytes);
                if let Ok(mut map) = cache.lock() {
                    map.insert(key, ms);
                }
            });
        }
        Some(src)
    }

    /// Import an audio file and make it the course's background track.
    ///
    /// Only the media entry is added; the file is not placed as an object.
    pub fn import_background_audio(&mut self, file_name: &str, bytes: &[u8]) -> Option<String> {
        let src = self.store_media(MediaKind::Audio, file_name, bytes)?;
        if self
            .run(
                "course.set_background_audio",
                serde_json::json!({ "src": src }),
            )
            .is_none()
        {
            self.media.retain(|(name, _)| name != &src);
            return None;
        }
        Some(src)
    }

    /// The `slide.update` step that lengthens base slide `slide_id` to fit
    /// media `src` starting at 0, when the media is longer than the slide,
    /// with the new duration. `None` when it already fits or the length
    /// cannot be read (the web editor, an unreadable file).
    pub(crate) fn media_fit_step(
        &self,
        slide_id: &str,
        kind: MediaKind,
        src: &str,
    ) -> Option<((String, Json), u32)> {
        let bytes = self.media_get(src)?;
        let ms = if kind == MediaKind::Video {
            let cached = self
                .video_durations
                .lock()
                .ok()
                .and_then(|map| map.get(src).copied());
            match cached {
                Some(v) => v?,
                None => {
                    let probed = coursecraft_playback::media_duration_ms(kind, src, bytes);
                    if let Ok(mut map) = self.video_durations.lock() {
                        map.insert(src.to_owned(), probed);
                    }
                    probed?
                }
            }
        } else {
            coursecraft_playback::media_duration_ms(kind, src, bytes)?
        };
        let slide = self.session.course.slide(slide_id)?;
        let needed = ms
            .div_ceil(100)
            .saturating_mul(100)
            .min(24 * 60 * 60 * 1000);
        let needed = u32::try_from(needed).ok()?;
        if needed <= slide.duration_ms {
            return None;
        }
        Some((
            (
                "slide.update".to_owned(),
                serde_json::json!({ "slide": slide_id, "duration_ms": needed }),
            ),
            needed,
        ))
    }

    /// Status line after a slide was lengthened to fit media.
    pub(crate) fn note_slide_fitted(&mut self, kind: MediaKind, duration_ms: u32) {
        let what = match kind {
            MediaKind::Audio => "sound",
            MediaKind::Video => "video",
        };
        let secs = duration_ms / 1000;
        let tenths = (duration_ms % 1000) / 100;
        self.set_status(format!(
            "Slide lengthened to {}:{:02}.{} to fit the {what}",
            secs / 60,
            secs % 60,
            tenths
        ));
    }

    /// Size-check, sanitise and store a media file, returning its `media/` name.
    fn store_media(&mut self, kind: MediaKind, file_name: &str, bytes: &[u8]) -> Option<String> {
        let label = match kind {
            MediaKind::Audio => "audio",
            MediaKind::Video => "video",
        };
        if bytes.len() > Self::MAX_MEDIA_IMPORT_BYTES {
            let msg = format!(
                "Failed to import {label} '{file_name}': file exceeds maximum allowed size of 200 MiB (was {:.1} MiB)",
                bytes.len() as f64 / (1024.0 * 1024.0)
            );
            self.ui.last_error = Some((msg.clone(), self.ui.current_time));
            self.ui.error_history.push(msg);
            return None;
        }

        let src = sanitize_media_src(file_name, &self.media);
        let arc_bytes: std::sync::Arc<[u8]> = bytes.into();
        self.media.push(src.clone(), arc_bytes);
        Some(src)
    }

    fn import_media(&mut self, kind: MediaKind, file_name: &str, bytes: &[u8]) -> Option<String> {
        let label = match kind {
            MediaKind::Audio => "audio",
            MediaKind::Video => "video",
        };
        let src = self.store_media(kind, file_name, bytes)?;

        if matches!(
            self.pending_image_pick,
            Some(PendingImagePick::ReplaceObject(_))
        ) && let Some(PendingImagePick::ReplaceObject(object_id)) =
            self.pending_image_pick.take()
        {
            let current_autoplay =
                self.session
                    .course
                    .slide(&self.selection.slide)
                    .and_then(|s| s.object(&object_id))
                    .and_then(|(o, _)| match (&o.kind, kind) {
                        (
                            coursecraft_course::ObjectKind::Audio { autoplay, .. },
                            MediaKind::Audio,
                        )
                        | (
                            coursecraft_course::ObjectKind::Video { autoplay, .. },
                            MediaKind::Video,
                        ) => Some(*autoplay),
                        _ => None,
                    })
                    .unwrap_or(false);

            self.run(
                "object.update",
                serde_json::json!({
                    "slide": self.selection.slide,
                    "object": object_id,
                    "kind": {
                        "type": label,
                        "src": src,
                        "autoplay": current_autoplay,
                    }
                }),
            );
        } else {
            self.ui.placement_tool = Some(match kind {
                MediaKind::Audio => PlacementTool::Audio { src: src.clone() },
                MediaKind::Video => PlacementTool::Video { src: src.clone() },
            });
        }

        Some(src)
    }

    /// Import a `.json` or `.lottie` animation and arm Lottie placement.
    ///
    /// The file is validated under the shared Lottie limits before its bytes are
    /// stored, so a bad file reports an error instead of becoming an object.
    pub fn import_lottie(&mut self, file_name: &str, bytes: &[u8]) -> Option<String> {
        let info = match coursecraft_render::parse_info(bytes) {
            Ok(info) => info,
            Err(error) => {
                let msg = format!("Failed to import Lottie '{file_name}': {error}");
                self.ui.last_error = Some((msg.clone(), self.ui.current_time));
                self.ui.error_history.push(msg);
                return None;
            }
        };
        let src = sanitize_media_src(file_name, &self.media);
        let arc_bytes: std::sync::Arc<[u8]> = bytes.into();
        self.media.push(src.clone(), arc_bytes);
        self.ui.placement_tool = Some(PlacementTool::Lottie {
            src: src.clone(),
            natural_size: (info.width as f32, info.height as f32),
        });
        Some(src)
    }

    /// Import a WebVTT caption file and attach it to the selected audio or video object.
    pub fn import_captions(&mut self, file_name: &str, bytes: &[u8]) -> Option<String> {
        if !file_name.to_ascii_lowercase().ends_with(".vtt") {
            let msg = format!("Failed to import captions '{file_name}': expected a .vtt file");
            self.ui.last_error = Some((msg.clone(), self.ui.current_time));
            self.ui.error_history.push(msg);
            return None;
        }
        if bytes.len() > Self::MAX_MEDIA_IMPORT_BYTES {
            let msg = format!(
                "Failed to import captions '{file_name}': file exceeds maximum allowed size of 200 MiB"
            );
            self.ui.last_error = Some((msg.clone(), self.ui.current_time));
            self.ui.error_history.push(msg);
            return None;
        }

        let path = sanitize_media_src(file_name, &self.media);
        let arc_bytes: std::sync::Arc<[u8]> = bytes.into();
        self.media.push(path.clone(), arc_bytes);

        let Some(PendingImagePick::ReplaceObject(object_id)) = (if matches!(
            self.pending_image_pick,
            Some(PendingImagePick::ReplaceObject(_))
        ) {
            self.pending_image_pick.take()
        } else {
            None
        }) else {
            return Some(path);
        };
        if self
            .run(
                "object.set_captions",
                serde_json::json!({
                    "slide": self.selection.slide,
                    "object": object_id,
                    "captions": path.clone(),
                }),
            )
            .is_none()
        {
            self.media.retain(|(name, _)| name != &path);
            return None;
        }
        Some(path)
    }

    /// Import a `.ttf`/`.otf` file as a course font.
    ///
    /// The family and style come from the file's name table, and the bytes are
    /// stored under `fonts/` before the undoable `course.add_font` command runs.
    /// Returns the media path on success.
    pub fn import_font(&mut self, file_name: &str, bytes: &[u8]) -> Option<String> {
        let info = match coursecraft_media::font_info(bytes) {
            Ok(info) => info,
            Err(e) => {
                let msg = format!("Failed to add font '{file_name}': {e}");
                self.ui.last_error = Some((msg.clone(), self.ui.current_time));
                self.ui.error_history.push(msg);
                return None;
            }
        };
        let src = sanitize_font_src(file_name, &self.media);
        let arc_bytes: std::sync::Arc<[u8]> = bytes.into();
        self.media.push(src.clone(), arc_bytes);
        let style = match info.style {
            coursecraft_media::FontFileStyle::Regular => FontStyle::Regular,
            coursecraft_media::FontFileStyle::Bold => FontStyle::Bold,
            coursecraft_media::FontFileStyle::Italic => FontStyle::Italic,
            coursecraft_media::FontFileStyle::BoldItalic => FontStyle::BoldItalic,
        };
        let result = self.run(
            "course.add_font",
            serde_json::json!({
                "src": src,
                "family": info.family,
                "style": style,
            }),
        );
        if result.is_none() {
            self.media.retain(|(name, _)| name != &src);
            return None;
        }
        Some(src)
    }

    /// Rename a font face's family, keeping its media file.
    pub fn rename_font_family(&mut self, old: &str, new: &str) -> bool {
        let Some(new) = (!new.trim().is_empty()).then(|| new.trim().to_owned()) else {
            return false;
        };
        let faces: Vec<_> = self
            .session
            .course
            .fonts
            .iter()
            .filter(|face| face.family.eq_ignore_ascii_case(old))
            .cloned()
            .collect();
        if faces.is_empty() {
            return false;
        }
        let mut steps = Vec::with_capacity(faces.len() + 1);
        steps.push((
            "course.remove_font".to_owned(),
            serde_json::json!({ "family": old }),
        ));
        for face in faces {
            steps.push((
                "course.add_font".to_owned(),
                serde_json::json!({
                    "src": face.src,
                    "family": new,
                    "style": face.style,
                }),
            ));
        }
        self.run_batch(&steps).is_some()
    }

    /// Remove every face of `family`. The caller confirms first when text uses it.
    pub fn remove_font_family(&mut self, family: &str) -> bool {
        self.run(
            "course.remove_font",
            serde_json::json!({ "family": family }),
        )
        .is_some()
    }

    /// Whether any text (including master and template text) names `family`.
    #[must_use]
    pub fn font_family_in_use(&self, family: &str) -> bool {
        let matches = |name: &str| name.trim().eq_ignore_ascii_case(family);
        if matches(&self.session.course.theme.font) {
            return true;
        }
        if self
            .session
            .course
            .theme
            .heading_font
            .as_deref()
            .is_some_and(matches)
        {
            return true;
        }
        let slide_uses = |slide: &coursecraft_course::Slide| {
            slide
                .objects
                .iter()
                .any(|object| object_uses_font(object, &matches))
                || slide
                    .layers
                    .iter()
                    .any(|layer| layer.objects.iter().any(|o| object_uses_font(o, &matches)))
        };
        self.session
            .course
            .slides()
            .chain(self.session.course.masters.iter())
            .chain(self.session.course.templates.iter())
            .any(slide_uses)
    }

    /// True when the editor is showing slide masters.
    #[must_use]
    pub fn is_master_view(&self) -> bool {
        self.ui.master_view
    }

    /// True when the selected slide is a master.
    #[must_use]
    pub fn selected_is_master(&self) -> bool {
        self.session.course.master(&self.selection.slide).is_some()
    }

    /// Toggle Master View.
    pub fn toggle_master_view(&mut self) {
        if self.ui.master_view {
            self.leave_master_view();
        } else {
            self.enter_master_view();
        }
    }

    pub(crate) fn preview_navigated_state(&mut self, state_name: String) {
        if !self.ui.master_view {
            dock::request_bottom_tab(self, BottomTab::States);
            self.ui.preview_state = Some(state_name);
            self.ui.last_selected_object = self.selection.object.clone();
        }
    }

    /// Enter Canvas Edit-State Mode for `state_name` on `object_id` (`slide_id`).
    pub fn enter_edit_state(&mut self, slide_id: &str, object_id: &str, state_name: &str) {
        if self.ui.master_view || state_name == "normal" {
            return;
        }
        let exists = self
            .session
            .course
            .slide(slide_id)
            .and_then(|s| s.object(object_id))
            .is_some_and(|(o, _)| o.states.iter().any(|st| st.name == state_name));
        if !exists {
            return;
        }
        self.selection.slide = slide_id.to_owned();
        self.selection.set_many([object_id.to_owned()]);
        self.ui.last_selected_object = Some(object_id.to_owned());
        self.ui.editing_state = Some(StateEditSession {
            slide_id: slide_id.to_owned(),
            object_id: object_id.to_owned(),
            state_name: state_name.to_owned(),
            selected_extra: None,
        });
        self.ui.preview_state = Some(state_name.to_owned());
        self.ui.inspector_tab = InspectorTab::Format;
        self.ui.format_painter = None;
        self.ui.placement_tool = None;
        self.ui.placement_drag = None;
        self.ui.draw = None;
        self.ui.path_edit = None;
        self.ui.canvas_drag = None;
        self.ui.group_focus = None;
        self.ui.states.card_strip_focused = false;
    }

    /// Exit Canvas Edit-State Mode without clearing the selected object.
    pub fn exit_edit_state(&mut self) {
        if self.ui.editing_state.take().is_some() {
            self.ui.canvas_drag = None;
            self.ui.inline_edit = None;
            self.ui.crop_edit = None;
            self.ui.crop_drag = None;
        }
    }

    /// Enter Master View, remembering the slide to return to.
    pub fn enter_master_view(&mut self) {
        if self.ui.master_view {
            return;
        }
        self.exit_edit_state();
        self.ui.master_view = true;
        // The Course map shows play slides: it cannot stay open here.
        self.ui.story_view = false;
        self.ui.master_view_return_slide = Some(self.selection.slide.clone());
        self.ui.inspector_tab = InspectorTab::Slide;
        dock::request_bottom_tab(self, BottomTab::Timeline);
        self.ui.placement_tool = None;
        self.ui.placement_drag = None;
        self.ui.canvas_drag = None;
        self.ui.inline_edit = None;
        self.ui.crop_edit = None;
        self.ui.crop_drag = None;
        self.ui.group_focus = None;
        self.ui.master_rename = None;
        let first = self
            .session
            .course
            .masters
            .first()
            .map(|master| master.id.clone())
            .unwrap_or_default();
        self.selection.slide = first;
        self.selection.layer = None;
        self.selection.clear_objects();
    }

    /// Leave Master View and return to the slide that was selected before.
    pub fn leave_master_view(&mut self) {
        if !self.ui.master_view {
            return;
        }
        self.ui.master_view = false;
        let remembered = self.ui.master_view_return_slide.take();
        let target = remembered
            .filter(|id| self.session.course.play_slide(id).is_some())
            .or_else(|| self.session.course.slides().next().map(|s| s.id.clone()))
            .unwrap_or_default();
        self.selection.slide = target;
        self.selection.layer = None;
        self.selection.clear_objects();
        self.ui.master_rename = None;
    }

    /// Create a master and select it. Used by the empty state and `+ New master`.
    pub fn create_master(&mut self) {
        if let Some(result) = self.run("master.add", serde_json::json!({}))
            && let Some(id) = result.get("id").and_then(Json::as_str)
        {
            self.ui.master_view = true;
            self.ui.story_view = false;
            self.selection.slide = id.to_owned();
            self.selection.layer = None;
            self.selection.clear_objects();
            self.ui.inspector_tab = InspectorTab::Slide;
            dock::request_bottom_tab(self, BottomTab::Timeline);
        }
    }

    /// Open the in-editor course preview window starting at the selected slide.
    pub fn open_preview(&mut self) {
        let slide = if self.selection.slide.is_empty() {
            None
        } else {
            Some(self.selection.slide.clone())
        };
        self.open_preview_at(slide.as_deref());
    }

    /// Open the in-editor course preview window for the given slide, or the whole course if `None`.
    ///
    /// Bank slides live outside play order, so starting a preview at one
    /// falls back to the whole course from its start slide.
    pub fn open_preview_at(&mut self, slide: Option<&str>) {
        self.exit_edit_state();
        let slide = slide.filter(|id| self.session.course.bank_slide(id).is_none());
        let mut preview = Preview::new(&self.session.course, slide);
        preview.set_initial_size(self.ui.preview_size);
        self.preview = Some(preview);
    }

    /// Open the in-editor full-course preview starting at `slide_id`.
    ///
    /// Falls back to the whole course from its start slide if `slide_id` is empty
    /// or refers to a question bank slide.
    pub fn open_preview_from(&mut self, slide_id: &str) {
        self.exit_edit_state();
        let is_scene_slide = !slide_id.is_empty()
            && self.session.course.bank_slide(slide_id).is_none()
            && self.session.course.play_slide(slide_id).is_some();
        let mut preview = if is_scene_slide {
            Preview::new_from_slide(&self.session.course, slide_id)
        } else {
            Preview::new(&self.session.course, None)
        };
        preview.set_initial_size(self.ui.preview_size);
        self.preview = Some(preview);
    }

    /// Open the in-editor course preview window restricted to the given slide IDs.
    ///
    /// Bank slides are ignored; if no scene slides remain, opens the whole course.
    pub fn open_preview_subset(&mut self, slides: &[String]) {
        self.exit_edit_state();
        let filtered: Vec<String> = slides
            .iter()
            .filter(|id| !id.is_empty() && self.session.course.bank_slide(id).is_none())
            .cloned()
            .collect();
        let mut preview = if filtered.is_empty() {
            Preview::new(&self.session.course, None)
        } else {
            Preview::new_subset(&self.session.course, Some(&filtered))
        };
        preview.set_initial_size(self.ui.preview_size);
        self.preview = Some(preview);
    }

    /// Open the in-editor course preview window for all currently selected slides.
    pub fn open_preview_selected(&mut self) {
        let ids = self.selection.slide_ids();
        self.open_preview_subset(&ids);
    }

    /// Show a neutral status message in the status bar.
    pub fn set_status(&mut self, message: impl Into<String>) {
        let message = message.into();
        self.ui.report.record(&message);
        self.ui.status_message = Some((message, self.ui.current_time));
    }

    /// Every selected object id on the focused layer, in selection order.
    #[must_use]
    pub fn selected_object_ids(&self) -> Vec<String> {
        self.selection.ids()
    }

    /// Delete every selected object as one undo step.
    ///
    /// In point-edit mode, Delete removes the selected point instead, refusing
    /// to leave a path with fewer than two.
    pub fn delete_selection(&mut self) {
        if let Some(session) = self.ui.editing_state.clone() {
            if let Some(extra_id) = session.selected_extra
                && self
                    .run(
                        "object.state_delete_extra",
                        serde_json::json!({
                            "slide": session.slide_id,
                            "object": session.object_id,
                            "state": session.state_name,
                            "extra": extra_id,
                        }),
                    )
                    .is_some()
                && let Some(active) = self.ui.editing_state.as_mut()
            {
                active.selected_extra = None;
            }
            return;
        }
        if crate::canvas::delete_selected_path_point(self) {
            return;
        }
        let all_ids = self.selected_object_ids();
        if all_ids.is_empty() {
            self.set_status("Select an object to delete");
            return;
        }
        // Locked objects are not deleted by a keystroke, whatever put them in the selection.
        let locked = |id: &String| {
            self.session
                .course
                .slide(&self.selection.slide)
                .and_then(|slide| slide.object(id))
                .is_some_and(|(object, _)| object.locked)
        };
        let ids: Vec<String> = all_ids.iter().filter(|id| !locked(id)).cloned().collect();
        if ids.is_empty() {
            self.set_status(tl!("Unlock the object to delete it"));
            return;
        }
        let slide = self.selection.slide.clone();
        let steps: Vec<(String, Json)> = ids
            .iter()
            .map(|id| {
                (
                    "object.delete".to_owned(),
                    serde_json::json!({ "slide": slide, "object": id }),
                )
            })
            .collect();
        let result = if steps.len() == 1 {
            self.run("object.delete", steps[0].1.clone())
                .map(|v| vec![v])
        } else {
            self.run_batch(&steps)
        };
        if result.is_some() {
            self.selection.clear_objects();
            self.ui.group_focus = None;
        }
    }

    /// Delete every selected slide as one undo step, refusing to leave the course empty.
    pub fn delete_selected_slides(&mut self) {
        let valid_ids: Vec<String> = self
            .selection
            .slide_ids()
            .into_iter()
            .filter(|id| self.session.course.slide(id).is_some())
            .collect();
        if valid_ids.is_empty() {
            return;
        }
        let total = self.session.course.slides().count();
        if valid_ids.len() >= total {
            self.set_status("A course needs at least one slide");
            return;
        }
        let steps: Vec<(String, Json)> = valid_ids
            .iter()
            .map(|id| {
                (
                    "slide.delete".to_owned(),
                    serde_json::json!({ "slide": id }),
                )
            })
            .collect();
        if steps.len() == 1 {
            self.run("slide.delete", steps[0].1.clone());
        } else {
            self.run_batch(&steps);
        }
    }

    /// Duplicate every selected object as one undo step and select the copies.
    pub fn duplicate_selection(&mut self) {
        let ids = self.selected_object_ids();
        if ids.is_empty() {
            self.set_status("Select an object to duplicate");
            return;
        }
        let slide = self.selection.slide.clone();
        let steps = vec![(
            "object.duplicate".to_owned(),
            serde_json::json!({ "slide": slide, "objects": ids }),
        )];
        if let Some(results) = self.run_batch(&steps) {
            let copies: Vec<String> = results
                .first()
                .and_then(|value| value.get("ids"))
                .and_then(Json::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Json::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default();
            if !copies.is_empty() {
                self.selection.set_many(copies);
                self.ui.group_focus = None;
            }
        }
    }

    /// Move the selection to the front or the back of its layer's stacking order.
    ///
    /// The selected objects keep their relative order. Like the one-step reorder, this
    /// is dispatched as `object.reorder` commands and batched into one undo step.
    pub fn reorder_selection_to(&mut self, to_front: bool) {
        let slide_id = self.selection.slide.clone();
        let Some(ids) = self.stack_ids() else {
            return;
        };
        if ids.is_empty() {
            return;
        }
        let selected_ids = self.selection.ids();
        let selected: std::collections::BTreeSet<&str> =
            selected_ids.iter().map(String::as_str).collect();
        if selected.is_empty() {
            return;
        }
        let mut unit: Vec<String> = Vec::new();
        let mut rest: Vec<String> = Vec::new();
        for id in &ids {
            if selected.contains(id.as_str()) {
                unit.push(id.clone());
            } else {
                rest.push(id.clone());
            }
        }
        if unit.is_empty() {
            return;
        }
        let target: Vec<String> = if to_front {
            rest.iter().chain(unit.iter()).cloned().collect()
        } else {
            unit.iter().chain(rest.iter()).cloned().collect()
        };
        if target == ids {
            self.set_status(if to_front {
                "Already at the front"
            } else {
                "Already at the back"
            });
            return;
        }
        self.run_reorder_steps(&slide_id, &target, &unit);
    }

    /// Apply a computed stacking order for a drag, moving only `moved` in one undo step.
    ///
    /// `target` is the full object order for the focused layer or base. It must contain
    /// exactly the ids that are currently stacked, or nothing happens.
    pub fn reorder_selection_to_order(&mut self, target: Vec<String>, moved: Vec<String>) {
        let slide_id = self.selection.slide.clone();
        let Some(current) = self.stack_ids() else {
            return;
        };
        if current.is_empty() || moved.is_empty() || target.len() != current.len() {
            return;
        }
        let current_set: std::collections::BTreeSet<&str> =
            current.iter().map(String::as_str).collect();
        let target_set: std::collections::BTreeSet<&str> =
            target.iter().map(String::as_str).collect();
        if target_set != current_set || !moved.iter().all(|id| current_set.contains(id.as_str())) {
            return;
        }
        if target == current {
            return;
        }
        self.run_reorder_steps(&slide_id, &target, &moved);
    }

    /// Ids in stacking order for the focused layer, or the base when no layer is selected.
    fn stack_ids(&self) -> Option<Vec<String>> {
        let slide = self.session.course.slide(&self.selection.slide)?;
        Some(match self.selection.layer.as_deref() {
            Some(lid) => slide
                .layer(lid)
                .map(|layer| layer.objects.iter().map(|o| o.id.clone()).collect())
                .unwrap_or_default(),
            None => slide.objects.iter().map(|o| o.id.clone()).collect(),
        })
    }

    /// Dispatch `object.reorder` for each moved id so `current` becomes `target`, as one step.
    fn run_reorder_steps(&mut self, slide_id: &str, target: &[String], moved: &[String]) {
        let Some(mut sim) = self.stack_ids() else {
            return;
        };
        let mut remaining: std::collections::BTreeSet<&str> =
            moved.iter().map(String::as_str).collect();
        let ordered_moved: Vec<&str> = target
            .iter()
            .map(String::as_str)
            .filter(|id| remaining.contains(id))
            .collect();
        let mut steps = Vec::with_capacity(ordered_moved.len());
        for id in ordered_moved {
            remaining.remove(id);
            let Some(cur_pos) = sim.iter().position(|x| x == id) else {
                continue;
            };
            sim.remove(cur_pos);
            let target_pos = target.iter().position(|x| x == id).unwrap_or(target.len());
            let next_anchor = target[target_pos + 1..]
                .iter()
                .map(String::as_str)
                .find(|after| !remaining.contains(after));
            let insert_at = next_anchor
                .and_then(|anchor| sim.iter().position(|x| x == anchor))
                .unwrap_or(sim.len());
            sim.insert(insert_at, id.to_owned());
            if cur_pos != insert_at {
                steps.push((
                    "object.reorder".to_owned(),
                    serde_json::json!({
                        "slide": slide_id,
                        "object": id,
                        "index": insert_at,
                    }),
                ));
            }
        }
        match steps.len() {
            0 => {}
            1 => {
                if let Some((cmd, params)) = steps.into_iter().next() {
                    self.run(&cmd, params);
                }
            }
            _ => {
                self.run_batch(&steps);
            }
        }
    }

    /// Move the selection forward (`delta > 0`) or backward, keeping relative order.
    pub fn reorder_selection(&mut self, delta: isize) {
        let slide_id = self.selection.slide.clone();
        let Some(slide) = self.session.course.slide(&slide_id) else {
            return;
        };
        let ids: Vec<String> = match self.selection.layer.as_deref() {
            Some(lid) => slide
                .layer(lid)
                .map(|layer| layer.objects.iter().map(|o| o.id.clone()).collect())
                .unwrap_or_default(),
            None => slide.objects.iter().map(|o| o.id.clone()).collect(),
        };
        if ids.is_empty() {
            return;
        }
        let selected_ids = self.selection.ids();
        let selected: std::collections::BTreeSet<&str> =
            selected_ids.iter().map(String::as_str).collect();
        if selected.is_empty() {
            return;
        }
        let mut order: Vec<Option<String>> = ids.iter().cloned().map(Some).collect();
        let is_selected = |order: &[Option<String>], index: usize| {
            order
                .get(index)
                .and_then(Option::as_deref)
                .is_some_and(|id| selected.contains(id))
        };
        if delta > 0 {
            for i in (0..order.len().saturating_sub(1)).rev() {
                if is_selected(&order, i) && !is_selected(&order, i + 1) {
                    order.swap(i, i + 1);
                }
            }
        } else if delta < 0 {
            for i in 1..order.len() {
                if is_selected(&order, i) && !is_selected(&order, i - 1) {
                    order.swap(i, i - 1);
                }
            }
        }
        let target: Vec<String> = order.into_iter().flatten().collect();
        if target == ids {
            self.set_status(if delta > 0 {
                "Already at the front"
            } else {
                "Already at the back"
            });
            return;
        }
        let steps: Vec<(String, Json)> = target
            .iter()
            .enumerate()
            .map(|(index, id)| {
                (
                    "object.reorder".to_owned(),
                    serde_json::json!({
                        "slide": slide_id,
                        "object": id,
                        "index": index,
                    }),
                )
            })
            .collect();
        self.run_batch(&steps);
    }

    /// Select `id` together with every member of its group.
    pub fn select_object_group(&mut self, id: &str) {
        let members = self.selection.group_members(&self.session.course, id);
        self.selection.set_many(members);
        self.ui.group_focus = None;
    }

    /// Add or remove `id`'s whole group, keeping `id` as the primary.
    pub fn toggle_object_group(&mut self, id: &str) {
        let members = self.selection.group_members(&self.session.course, id);
        if members.len() <= 1 {
            self.selection.toggle(id);
            return;
        }
        if members.iter().all(|member| self.selection.contains(member)) {
            for member in &members {
                self.selection.toggle(member);
            }
            self.ui.group_focus = None;
            return;
        }
        let mut ids = vec![id.to_owned()];
        for existing in self.selection.ids() {
            if existing != id && !members.contains(&existing) && !ids.contains(&existing) {
                ids.push(existing);
            }
        }
        for member in members {
            if !ids.contains(&member) {
                ids.push(member);
            }
        }
        self.selection.set_many(ids);
        self.ui.group_focus = None;
    }

    /// Grow the current selection to whole groups.
    pub fn expand_selection_groups(&mut self) {
        self.selection.expand_groups(&self.session.course);
    }

    /// True when the selection is exactly one complete group.
    #[must_use]
    pub fn selection_is_one_group(&self) -> bool {
        self.selection.is_one_group(&self.session.course)
    }

    /// Group id when a single whole group is selected and not focused on one child.
    #[must_use]
    pub fn selected_group_for_animation(&self) -> Option<String> {
        if self.ui.group_focus.is_some() {
            return None;
        }
        self.selection.selected_group_id(&self.session.course)
    }

    /// Select every object on the focused layer.
    pub fn select_all_objects(&mut self) {
        let Some(slide) = self.session.course.slide(&self.selection.slide) else {
            return;
        };
        // Locked and hidden objects cannot be picked on the canvas, so Select All
        // skips them too: a following move or delete must not reach them.
        let pickable = |o: &coursecraft_course::Object| !o.locked && !o.editor_hidden;
        let ids: Vec<String> = match self.selection.layer.as_deref() {
            Some(lid) => slide
                .layer(lid)
                .map(|layer| {
                    layer
                        .objects
                        .iter()
                        .filter(|o| pickable(o))
                        .map(|o| o.id.clone())
                        .collect()
                })
                .unwrap_or_default(),
            None => slide
                .objects
                .iter()
                .filter(|o| pickable(o))
                .map(|o| o.id.clone())
                .collect(),
        };
        self.selection.set_many(ids);
    }

    /// True when a dialog or window that owns the keyboard is open.
    ///
    /// Shortcuts that act on the course (Delete, arrows, Undo, Select All,
    /// New, Open) must not reach the editor behind it. Publish, Layer Settings,
    /// Fonts and the trigger editor are left out on purpose: they follow the
    /// course live, so Undo works behind them and they close or refresh with it.
    #[must_use]
    pub(crate) fn modal_dialog_open(&self) -> bool {
        let ui = &self.ui;
        ui.ai.open
            || ui.alt.open
            || ui.captions.open
            || ui.speech.open
            || ui.record_narration.open
            || ui.translate.open
            || ui.document.open
            || ui.prefs.open
            || ui.versions.open
            || ui.report.open
            || ui.slide_size_dialog.open
            || ui.show_course_settings_window
            || ui.show_theme_window
            || ui.paste_special.open
            || ui.focus_order.open
            || ui.theme_dialog.fonts_open
            || ui.show_shortcuts_dialog
            || ui.show_outline_dialog
            || ui.show_export_video
            || ui.show_export_review
            || ui.show_export_translation
            || ui.show_web_window
            || ui.insert_text.any_window_open()
            || ui.pending_delete_scene.is_some()
            || ui.pending_delete_master.is_some()
            || ui.confirm_delete_library_template.is_some()
            || ui.pending_remove_font.is_some()
    }

    /// The scale `Fit` shows now, from the slide drawn last frame.
    fn fit_scale(&self) -> Option<f32> {
        let rect = self.ui.last_slide_rect?;
        let width = self.session.course.slide_size.w as f32;
        (width > 0.0 && rect.width().is_finite()).then(|| rect.width() / width)
    }

    /// The zoom level one step in (Zoom in button, Ctrl+=).
    #[must_use]
    pub fn zoom_in_level(&self) -> ZoomLevel {
        self.ui.zoom.zoom_in_at(self.fit_scale())
    }

    /// The zoom level one step out (Zoom out button, Ctrl+-).
    #[must_use]
    pub fn zoom_out_level(&self) -> ZoomLevel {
        self.ui.zoom.zoom_out_at(self.fit_scale())
    }

    /// Zoom one step in.
    pub fn zoom_in(&mut self) {
        self.ui.zoom = self.zoom_in_level();
    }

    /// Zoom one step out.
    pub fn zoom_out(&mut self) {
        self.ui.zoom = self.zoom_out_level();
    }

    /// The one guard for editor keys that change the course or the selection:
    /// the start page, the File page and every modal dialog block them.
    #[must_use]
    pub(crate) fn global_keys_blocked(&self) -> bool {
        self.ui.show_welcome || self.ui.file_page.open || self.modal_dialog_open()
    }

    /// True when a window that follows the course live is open: Publish, Layer Settings, the
    /// trigger editor, Fonts, the Media Library or a cue rename. Undo and Redo work behind
    /// them, but the keys that edit the canvas behind them (Delete, Enter, arrows, Select All,
    /// Cut, Copy, Paste) do not (`N89`).
    #[must_use]
    pub(crate) fn soft_dialog_open(&self) -> bool {
        let ui = &self.ui;
        ui.publish.open
            || ui.trigger_form.is_some()
            || ui.layer_settings.is_open()
            || ui.show_media_library
            || ui.show_fonts_window
            || ui.timeline.slide_cue_rename.is_some()
    }

    /// True when the keys that edit the canvas must not reach it.
    #[must_use]
    pub(crate) fn edit_keys_blocked(&self) -> bool {
        self.global_keys_blocked() || self.soft_dialog_open()
    }

    /// Nudge every selected object by `(dx, dy)` as one undo step.
    pub fn nudge_selection(&mut self, dx: f32, dy: f32) {
        if let Some(session) = self.ui.editing_state.clone() {
            if let Some(extra_id) = session.selected_extra
                && let Some(frame) = self
                    .session
                    .course
                    .slide(&session.slide_id)
                    .and_then(|s| s.object(&session.object_id))
                    .and_then(|(o, _)| o.states.iter().find(|st| st.name == session.state_name))
                    .and_then(|st| st.extras.iter().find(|ex| ex.id == extra_id))
                    .map(|ex| ex.frame)
            {
                let nx = frame.x + dx;
                let ny = frame.y + dy;
                if nx.is_finite() && ny.is_finite() {
                    self.run(
                        "object.state_update_extra",
                        serde_json::json!({
                            "slide": session.slide_id,
                            "object": session.object_id,
                            "state": session.state_name,
                            "extra": extra_id,
                            "patch": {
                                "frame": {
                                    "x": nx,
                                    "y": ny,
                                    "w": frame.w,
                                    "h": frame.h,
                                }
                            }
                        }),
                    );
                }
            }
            return;
        }
        let Some(slide) = self.session.course.slide(&self.selection.slide) else {
            return;
        };
        let mut steps: Vec<(String, Json)> = Vec::new();
        let mut moved: Vec<String> = Vec::new();
        for id in self.selection.ids() {
            let Some((object, _)) = slide.object(&id) else {
                continue;
            };
            // A locked object stays where it is, whatever put it in the selection.
            if object.locked {
                continue;
            }
            let frame = coursecraft_course::Rect {
                x: object.frame.x + dx,
                y: object.frame.y + dy,
                w: object.frame.w,
                h: object.frame.h,
            };
            if !frame.x.is_finite() || !frame.y.is_finite() {
                continue;
            }
            moved.push(id.clone());
            steps.push((
                "object.set_frame".to_owned(),
                serde_json::json!({
                    "slide": slide.id,
                    "object": id,
                    "frame": {
                        "x": frame.x,
                        "y": frame.y,
                        "w": frame.w,
                        "h": frame.h,
                    },
                }),
            ));
        }
        if steps.is_empty() {
            return;
        }
        let slide_id = self.selection.slide.clone();
        let revision_before = self.session.revision;
        let continues_burst = self.ui.nudge_burst.as_ref().is_some_and(|burst| {
            burst.revision == revision_before
                && burst.slide == slide_id
                && burst.ids == moved
                && self.ui.current_time - burst.time <= NUDGE_BURST_SECS
        });
        if self.run_batch(&steps).is_none() {
            self.ui.nudge_burst = None;
            return;
        }
        // A held arrow key is one undo step, not one per repeat.
        if continues_burst && self.session.revision != revision_before {
            self.session.merge_last_two_undos(None);
        }
        self.ui.nudge_burst = Some(NudgeBurst {
            revision: self.session.revision,
            slide: slide_id,
            ids: moved,
            time: self.ui.current_time,
        });
    }

    /// Single place that calls `session.execute`.
    ///
    /// On `Err` it stores the error message in `ui.last_error` and returns `None`.
    /// Nothing else in the crate calls `execute`.
    pub fn run(&mut self, id: &str, params: Json) -> Option<Json> {
        #[cfg(test)]
        let before_for_log = std::env::var("CC_FAILED_LOG")
            .ok()
            .map(|_| self.session.course.clone());
        let rev_before = self.session.revision;
        let slide_before = self.selection.slide.clone();
        match self.session.execute(id, &params) {
            Ok(v) => {
                #[cfg(test)]
                if let Some(before) = before_for_log
                    && before == self.session.course
                    && !matches!(id, "edit.undo" | "edit.redo")
                    && registry().iter().any(|cmd| cmd.id == id && cmd.mutates)
                {
                    log_failed_for_test(id, &params, "NO CHANGE");
                }
                self.ui.pending_undo_redo = matches!(id, "edit.undo" | "edit.redo");
                self.select_created(id, &v);
                if self.session.revision != rev_before
                    && !self.ui.story_view
                    && !self.ui.master_view
                    && self.selection.slide == slide_before
                    && !matches!(id, "edit.undo" | "edit.redo")
                    && self.ui.ribbon.visible
                    && !self.ui.doc_tabs.open.contains(&slide_before)
                {
                    doc_tabs::pin(self, &slide_before);
                }
                self.note_removed_triggers(&v);
                Some(v)
            }
            Err(e) => {
                let msg = e.to_string();
                log_failed_for_test(id, &params, &msg);
                self.ui.last_error = Some((msg.clone(), self.ui.current_time));
                self.ui.error_history.push(msg);
                None
            }
        }
    }

    /// Run a batch of commands atomically through `session.execute_batch`.
    ///
    /// On `Err` it stores the error message in `ui.last_error` and returns `None`.
    /// On success, selects the last created item if any.
    pub fn run_batch(&mut self, steps: &[(String, Json)]) -> Option<Vec<Json>> {
        let rev_before = self.session.revision;
        let slide_before = self.selection.slide.clone();
        match self.session.execute_batch(steps) {
            Ok(results) => {
                self.ui.pending_undo_redo = false;
                for (step, res) in steps.iter().zip(&results) {
                    self.select_created(&step.0, res);
                }
                let is_undo_redo = steps
                    .iter()
                    .any(|(cmd, _)| matches!(cmd.as_str(), "edit.undo" | "edit.redo"));
                if self.session.revision != rev_before
                    && !self.ui.story_view
                    && !self.ui.master_view
                    && self.selection.slide == slide_before
                    && !is_undo_redo
                    && self.ui.ribbon.visible
                    && !self.ui.doc_tabs.open.contains(&slide_before)
                {
                    doc_tabs::pin(self, &slide_before);
                }
                let removed: u64 = results
                    .iter()
                    .filter_map(|res| res.get("removed_triggers").and_then(Json::as_u64))
                    .sum();
                self.note_removed_triggers(&serde_json::json!({ "removed_triggers": removed }));
                Some(results)
            }
            Err(e) => {
                let msg = e.to_string();
                log_failed_for_test("batch", &serde_json::json!(steps), &msg);
                self.ui.last_error = Some((msg.clone(), self.ui.current_time));
                self.ui.error_history.push(msg);
                None
            }
        }
    }

    /// Tell the author when a delete also removed the triggers that used
    /// what was deleted (they come back with one Undo).
    fn note_removed_triggers(&mut self, result: &Json) {
        let removed = result
            .get("removed_triggers")
            .and_then(Json::as_u64)
            .unwrap_or(0);
        if removed == 1 {
            self.set_status("Deleted, with 1 trigger that used it (Undo brings it back)");
        } else if removed > 1 {
            self.set_status(format!(
                "Deleted, with {removed} triggers that used it (Undo brings them back)"
            ));
        }
    }

    /// After a creating command succeeds, select the new item named by its `{"id": …}` result.
    fn select_created(&mut self, command: &str, result: &Json) {
        if command == "object.paste" {
            let ids: Vec<String> = result
                .get("ids")
                .and_then(Json::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Json::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default();
            if !ids.is_empty() {
                self.selection.set_many(ids);
            }
            return;
        }
        let Some(new_id) = result.get("id").and_then(Json::as_str) else {
            return;
        };
        match command {
            "slide.add"
            | "slide.duplicate"
            | "slide.paste"
            | "slide.add_from_layout"
            | "template.add_slide"
            | "bank.add_question"
            | "draw.insert" => {
                self.selection.slide = new_id.to_owned();
                self.selection.also_slides.clear();
                self.selection.layer = None;
                self.selection.clear_objects();
            }
            "layer.add" | "layer.duplicate" => {
                self.selection.layer = Some(new_id.to_owned());
                self.selection.clear_objects();
            }
            "object.add" | "object.duplicate" => {
                self.selection.set_many([new_id.to_owned()]);
            }
            _ => {}
        }
    }

    /// Check if the course revision changed, repairing selection and updating validation.
    pub fn check_revision(&mut self) {
        if self.session.revision != self.ui.last_seen_revision {
            self.ui.last_seen_revision = self.session.revision;
            let reselect_restored = std::mem::take(&mut self.ui.pending_undo_redo);
            self.selection.repair_with_prev_ex(
                &self.session.course,
                &self.ui.last_known_slides,
                reselect_restored,
            );
            if !slide_layout_matches(&self.session.course, &self.ui.last_known_slides) {
                self.ui.last_known_slides = selection::slide_layout(&self.session.course);
            }
            self.ui.pending_undo_redo = false;
            if let Some(session) = self.ui.editing_state.clone()
                && let Some(ref extra_id) = session.selected_extra
            {
                let extra_exists = self
                    .session
                    .course
                    .slide(&session.slide_id)
                    .and_then(|slide| slide.object(&session.object_id))
                    .and_then(|(obj, _)| obj.states.iter().find(|st| st.name == session.state_name))
                    .is_some_and(|st| st.extras.iter().any(|e| e.id == *extra_id));
                if !extra_exists && let Some(active) = self.ui.editing_state.as_mut() {
                    active.selected_extra = None;
                }
            }
            if self.ui.master_view {
                if self.session.course.master(&self.selection.slide).is_none() {
                    let first = self
                        .session
                        .course
                        .masters
                        .first()
                        .map(|master| master.id.clone())
                        .unwrap_or_default();
                    self.selection.slide = first;
                    self.selection.layer = None;
                    self.selection.clear_objects();
                }
            } else if self
                .session
                .course
                .play_slide(&self.selection.slide)
                .is_none()
            {
                let first = self
                    .session
                    .course
                    .slides()
                    .next()
                    .map(|slide| slide.id.clone())
                    .unwrap_or_default();
                self.selection.slide = first;
                self.selection.layer = None;
                self.selection.clear_objects();
            }
            if let Some(ref focused) = self.ui.group_focus
                && self
                    .session
                    .course
                    .slide(&self.selection.slide)
                    .and_then(|slide| slide.object(focused))
                    .is_none()
            {
                self.ui.group_focus = None;
            }
            self.refresh_validate_only();
            self.ui.audit_dirty = true;
            self.ui.audit_dirty_since = None;
        }
        // Leaving the slide, undo, redo or a replace makes the working crop
        // stale even when the course revision did not change (a slide change
        // does not touch the course), so check every frame, not only on a
        // revision bump.
        if self.ui.crop_edit.is_some() {
            let stale = self.ui.crop_edit.as_ref().is_some_and(|session| {
                session.revision != self.session.revision
                    || session.slide != self.selection.slide
                    || self
                        .session
                        .course
                        .slide(&session.slide)
                        .and_then(|slide| slide.object(&session.object))
                        .is_none()
            });
            if stale {
                self.cancel_crop();
            }
        }
        if let Some(session) = self.ui.editing_state.clone() {
            let state_def = self
                .session
                .course
                .slide(&session.slide_id)
                .and_then(|slide| slide.object(&session.object_id))
                .and_then(|(obj, _)| obj.states.iter().find(|st| st.name == session.state_name));
            let context_invalid = self.ui.master_view
                || self.ui.story_view
                || self.preview.is_some()
                || session.slide_id != self.selection.slide
                || self.selection.is_multi()
                || self.selection.object.as_deref() != Some(session.object_id.as_str());
            if let Some(_st) = state_def {
                if context_invalid {
                    self.exit_edit_state();
                }
            } else {
                self.ui.preview_state = None;
                self.exit_edit_state();
            }
        }
        if self
            .ui
            .slide_rename
            .as_ref()
            .is_some_and(|rename| self.session.course.slide(&rename.scene).is_none())
        {
            self.ui.slide_rename = None;
        }
        if self
            .ui
            .timeline
            .object_rename
            .as_ref()
            .is_some_and(|rename| {
                self.session
                    .course
                    .slide(&rename.slide)
                    .and_then(|slide| slide.object(&rename.object))
                    .is_none()
            })
        {
            self.ui.timeline.object_rename = None;
        }
    }

    /// Run course validation (schema and trigger checks) now, through the engine.
    pub(crate) fn refresh_validate_only(&mut self) {
        #[cfg(test)]
        crate::guard_counters::note_validation_run();
        let engine_issues = self.session.validation_issues();
        let mut errs = 0usize;
        let mut warns = 0usize;
        let mut issues = Vec::with_capacity(engine_issues.len());
        for issue in engine_issues {
            let severity = match issue.severity {
                coursecraft_course::Severity::Error => {
                    errs += 1;
                    "error"
                }
                coursecraft_course::Severity::Warning => {
                    warns += 1;
                    "warning"
                }
            };
            issues.push(ValidationIssueSummary {
                severity: severity.to_owned(),
                path: issue.path.clone(),
                message: issue.message.clone(),
            });
        }
        for r in coursecraft_course::media_references(&self.session.course) {
            if r.src.ends_with('/') {
                continue;
            }
            if !self.media_contains(&r.src) {
                warns += 1;
                issues.push(ValidationIssueSummary {
                    severity: "warning".to_owned(),
                    path: r.place.clone(),
                    message: format!("Missing media \u{201C}{}\u{201D} in {}.", r.src, r.place),
                });
            }
        }
        self.ui.trigger_issues_by_path = index_trigger_issues(&issues);
        self.ui.validation_errors = errs;
        self.ui.validation_warnings = warns;
        self.ui.validation_issues = issues;
    }

    /// Run the accessibility audit now, through the engine, and clear the dirty flag.
    pub(crate) fn refresh_audit_now(&mut self) {
        #[cfg(test)]
        crate::guard_counters::note_audit_run();
        if let Some(res) = self.run("course.audit", serde_json::json!({})) {
            let (findings, manual_checks) = parse_audit_result(&res);
            self.ui.audit_findings = findings;
            self.ui.audit_manual_checks = manual_checks;
        }
        self.ui.audit_dirty = false;
        self.ui.audit_dirty_since = None;
    }

    /// Run the accessibility audit if it is dirty and either a panel that shows
    /// it is open or edits have been quiet for 300 ms.
    pub(crate) fn maybe_run_deferred_audit(&mut self, ctx: &egui::Context) {
        if !self.ui.audit_dirty {
            return;
        }
        if self.ui.show_validation_window || self.ui.publish.open {
            self.refresh_audit_now();
            return;
        }
        let now = ctx.input(|i| i.time);
        let since = *self.ui.audit_dirty_since.get_or_insert(now);
        let elapsed = now - since;
        if elapsed >= 0.3 {
            self.refresh_audit_now();
        } else {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(
                (0.3 - elapsed).max(0.001),
            ));
        }
    }

    /// Run validation and the accessibility audit now, through the engine.
    pub(crate) fn refresh_validation(&mut self) {
        self.refresh_validate_only();
        self.refresh_audit_now();
    }

    /// Focus conditions shared by the paste event block and the desktop image
    /// paste, so both ignore a focused text field, the preview and modal panes.
    fn paste_focus(&self, ctx: &egui::Context) -> (bool, bool) {
        let preview_has_focus = self.preview.as_ref().is_some_and(|p| p.has_focus);
        let text_focus = ctx.egui_wants_keyboard_input()
            || ctx.memory(|m| m.focused().is_some())
            || self.ui.inline_edit.is_some()
            || self.is_cropping()
            || self.ui.ai.open
            || self.ui.alt.open;
        (preview_has_focus, text_focus)
    }

    fn paste_input_allowed(preview_has_focus: bool, text_focus: bool) -> bool {
        !preview_has_focus && !text_focus
    }

    /// Whether a desktop image paste shortcut should reach the editor.
    ///
    /// This uses the same focus conditions as the copy/cut/paste event block in
    /// [`Self::show`], so Cmd/Ctrl+V while typing never inserts a picture.
    #[must_use]
    pub fn wants_image_paste(&self, ctx: &egui::Context) -> bool {
        let (preview_has_focus, text_focus) = self.paste_focus(ctx);
        Self::paste_input_allowed(preview_has_focus, text_focus) && !self.edit_keys_blocked()
    }

    /// The slides panel's contents (thumbnails, scenes).
    pub(crate) fn slides_body(&mut self, ui: &mut Ui) {
        let panel_rect = ui.available_rect_before_wrap();
        slides::show_slides_panel(self, ui, panel_rect);
    }

    /// The Inspector's contents (Format, Animate, Slide, Course).
    /// The inspector: `docked` names the one inspector a dock panel draws; `None`
    /// draws the fixed panel, which switches between all four.
    pub(crate) fn inspector_body(&mut self, ui: &mut Ui, docked: Option<InspectorTab>) {
        // A child that doesn't report overflow: a wide row must not grow the
        // panel, which would push the rest of the window off screen.
        let rect = ui.available_rect_before_wrap();
        let mut content = ui.new_child(egui::UiBuilder::new().max_rect(rect));
        content.set_clip_rect(rect.intersect(ui.clip_rect()));
        properties::show_properties_panel(self, &mut content, docked);
        ui.allocate_rect(rect, egui::Sense::hover());
    }

    /// The canvas with its layers strip, or the Course Map.
    pub(crate) fn canvas_body(&mut self, ui: &mut Ui) {
        if self.ui.story_view {
            course_map::handle_map_keys(self, ui);
            course_map::show_course_map(self, ui);
        } else {
            canvas::show_canvas_and_layers(self, ui);
        }
    }

    /// The timeline's contents; its edits run as commands.
    pub(crate) fn timeline_body(&mut self, ui: &mut Ui) {
        let mut actions = Vec::new();
        // Docked, Slide Layers is a panel of its own beside the Timeline;
        // the fixed Timeline panel keeps it on its right.
        if self.ui.dock.enabled {
            timeline::show_timeline_content(self, ui, &mut actions);
        } else {
            timeline::show_timeline_panel(self, ui, &mut actions);
        }
        for (cmd, params) in actions {
            self.run(cmd, params);
        }
    }

    /// The Slide Layers dock panel.
    pub(crate) fn layers_body(&mut self, ui: &mut Ui) {
        let mut actions = Vec::new();
        layers::show_layers_panel(self, ui, &mut actions);
        for (cmd, params) in actions {
            self.run(cmd, params);
        }
    }

    /// One bottom tab drawn as a dock panel of its own (States, Triggers,
    /// Variables, Question).
    pub(crate) fn bottom_tab_body(&mut self, ui: &mut Ui, tab: BottomTab) {
        if let Some(reason) = timeline::bottom_tab_unavailable(self, tab) {
            ui.add_space(8.0);
            ui.label(reason);
            return;
        }
        let mut actions = Vec::new();
        timeline::show_bottom_tab(self, ui, tab, &mut actions);
        for (cmd, params) in actions {
            self.run(cmd, params);
        }
    }

    /// Display the editor UI inside `ui` and return any shell requests.
    #[allow(clippy::cognitive_complexity)]
    pub fn show(&mut self, ui: &mut Ui) -> Vec<EditorRequest> {
        #[cfg(test)]
        crate::guard_counters::begin_frame();
        self.ui.current_time = ui.input(|i| i.time);
        // Strings are looked up on the drawing thread in the saved language.
        i18n::set_current(i18n::resolve(&self.language));
        self.media_cache.begin_frame();
        self.sync_perf_profile(Some(ui.ctx()));
        self.check_memory_pressure();
        widgets::begin_press_guard(ui.ctx());
        // Pick mode owns the pointer and Escape before anything else sees them.
        eyedropper::process_frame(ui.ctx());
        self.check_revision();
        self.maybe_run_deferred_audit(ui.ctx());
        font_picker::publish_theme_fonts(ui.ctx(), &self.session.course.theme);
        if self.ui.last_published_theme_rev != Some(self.session.revision) {
            properties::publish_theme_colors(ui.ctx(), &self.session.course.theme);
            self.ui.last_published_theme_rev = Some(self.session.revision);
        }
        coursecraft_render::set_system_font_provider(ui.ctx(), self.system_font_provider.clone());
        coursecraft_render::install_course_fonts(
            ui.ctx(),
            &self.session.course,
            &MemoryMediaSource::from_store(&self.media),
        );
        self.update_timeline_playback(ui);
        dock::replay_forwarded(self, ui.ctx());

        // Editor shortcuts must not act on the editor's selection while the user is
        // interacting with the preview window. Crop mode owns the keyboard: only
        // Enter (apply) and Escape (cancel) act there.
        let (preview_has_focus, text_focus) = self.paste_focus(ui.ctx());

        // Copy, cut and paste arrive as events so they work in the browser too.
        // Leave them to a focused text field or the in-place editor.
        // Not behind a window that owns the keys, nor on the Start page (`N89`).
        if Self::paste_input_allowed(preview_has_focus, text_focus) && !self.edit_keys_blocked() {
            let (copy, cut, paste) = ui.input_mut(|i| {
                let mut copy = false;
                let mut cut = false;
                let mut paste = None;
                i.events.retain(|event| match event {
                    egui::Event::Copy => {
                        copy = true;
                        false
                    }
                    egui::Event::Cut => {
                        cut = true;
                        false
                    }
                    egui::Event::Paste(text) => {
                        paste = Some(text.clone());
                        false
                    }
                    _ => true,
                });
                (copy, cut, paste)
            });
            if copy {
                clipboard::copy(self);
            }
            if cut {
                clipboard::cut(self);
            }
            if let Some(text) = paste {
                clipboard::paste_text(self, &text);
            }
        }
        clipboard::expire_pending_paste_mode(self, ui.ctx().cumulative_frame_nr());

        let mut requests = Vec::new();

        // File shortcuts (Save As before Save because egui's COMMAND match ignores extra Shift).
        let file_shortcuts_base = !preview_has_focus
            && self.ui.inline_edit.is_none()
            && !self.is_cropping()
            && !self.ui.ai.open
            && !self.ui.document.open
            && !self.ui.alt.open
            && !self.ui.translate.open
            && !self.ui.show_shortcuts_dialog
            && !matches!(self.ui.publish.phase, PublishPhase::Busy);
        let file_shortcuts_allowed = file_shortcuts_base && !ui.ctx().egui_wants_keyboard_input();
        // Ctrl+S in a text field commits the field first (it loses focus), then
        // saves one frame later, so the save holds what was just typed.
        if self.ui.save_after_commit {
            self.ui.save_after_commit = false;
            if self.file_ops_available && !self.ui.show_welcome {
                requests.push(EditorRequest::Save);
            }
        } else if file_shortcuts_base
            && !self.modal_dialog_open()
            && !self.ui.show_welcome
            && ui.ctx().egui_wants_keyboard_input()
            && let Some(save_as) = menu::parse_shortcut("Ctrl+Shift+S")
            && let Some(sc) = menu::parse_shortcut("Ctrl+S")
            // Ctrl+Shift+S stays suppressed in a field; egui would match it as Ctrl+S.
            && !ui.input_mut(|i| i.consume_shortcut(&save_as))
            && ui.input_mut(|i| i.consume_shortcut(&sc))
            && let Some(focused) = ui.memory(|m| m.focused())
        {
            ui.memory_mut(|m| m.surrender_focus(focused));
            self.ui.save_after_commit = true;
            ui.ctx().request_repaint();
        }
        if file_shortcuts_allowed {
            if self.file_ops_available && !self.ui.show_welcome {
                if let Some(sc) = menu::parse_shortcut("Ctrl+Shift+S")
                    && ui.input_mut(|i| i.consume_shortcut(&sc))
                {
                    requests.push(EditorRequest::SaveAs);
                } else if let Some(sc) = menu::parse_shortcut("Ctrl+S")
                    && ui.input_mut(|i| i.consume_shortcut(&sc))
                {
                    requests.push(EditorRequest::Save);
                }
            }
            // New and Open stay on the start page and File page; a dialog blocks them.
            if !self.modal_dialog_open() {
                if self.file_ops_available
                    && let Some(sc) = menu::parse_shortcut("Ctrl+O")
                    && ui.input_mut(|i| i.consume_shortcut(&sc))
                {
                    requests.push(EditorRequest::Open);
                }
                if let Some(sc) = menu::parse_shortcut("Ctrl+N")
                    && ui.input_mut(|i| i.consume_shortcut(&sc))
                {
                    requests.push(EditorRequest::New);
                }
            }
        }

        // Escape disarms the format painter before the canvas can act on it.
        if !preview_has_focus && !text_focus {
            self.consume_format_painter_escape(ui.ctx());
            self.handle_format_painter_shortcuts(ui.ctx());
            if ui.ctx().input_mut(|i| {
                i.consume_shortcut(&egui::KeyboardShortcut::new(
                    egui::Modifiers::COMMAND,
                    egui::Key::Comma,
                ))
            }) {
                self.open_preferences();
            }
        }
        // F7 opens the course-wide spelling check when no dialog owns the keys.
        if !preview_has_focus
            && !text_focus
            && !self.ui.ai.open
            && !self.ui.document.open
            && !self.global_keys_blocked()
            && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F7))
        {
            spell_check::open(self);
        }
        // Escape closes the spelling check before the canvas can act on it.
        spell_check::consume_escape(self, ui.ctx());
        // Escape closes Find and Replace before the canvas can act on it.
        find::consume_escape(self, ui.ctx());
        // Escape closes Slides from Outline before the canvas can act on it.
        outline::consume_escape(self, ui.ctx());
        // Escape closes Course from Document too.
        document::consume_escape(self, ui.ctx());
        // Escape closes Suggest Alt Text when nothing is running.
        alt_text::consume_escape(self, ui.ctx());
        // Escape closes Translate with AI, cancelling a run first.
        translate_ai::consume_escape(self, ui.ctx());
        // Escape closes Record Narration, stopping capture and playback.
        record_narration::consume_escape(self, ui.ctx());

        // Course map toggle comes before the registry shortcuts: egui
        // matches Ctrl+M logically (extra Shift ignored), so the
        // Ctrl+Shift+M binding must be consumed first.
        if !preview_has_focus
            && !self.is_cropping()
            && !self.ui.master_view
            && !self.ui.ai.open
            && !self.ui.alt.open
            && !self.ui.translate.open
            && !self.ui.document.open
            && !self.global_keys_blocked()
            && let Some(sc) = menu::parse_shortcut("Ctrl+Shift+M")
            && !ui.ctx().egui_wants_keyboard_input()
            && ui.input_mut(|i| i.consume_shortcut(&sc))
        {
            self.ui.story_view = !self.ui.story_view;
        }

        // Redo with Ctrl+Shift+Z / Cmd+Shift+Z comes before the registry
        // shortcuts: egui matches Ctrl+Z logically (extra Shift ignored), so
        // Ctrl+Shift+Z must be consumed before edit.undo's Ctrl+Z.
        if !preview_has_focus
            && !text_focus
            && !self.is_cropping()
            && !self.ui.ai.open
            && !self.ui.document.open
            && !self.ui.alt.open
            && !self.ui.translate.open
            && !self.global_keys_blocked()
            && let Some(sc) = menu::parse_shortcut("Ctrl+Shift+Z")
            && ui.input_mut(|i| i.consume_shortcut(&sc))
            && let Some(params) = menu::adapter(self, "edit.redo")
        {
            self.run("edit.redo", params);
        }

        // Handle keyboard shortcuts from registry. More specific shortcuts run
        // first so Cmd+Shift+G is not swallowed by Cmd+G. While a text field or
        // the inline text editor has focus, skip registry shortcuts (Save, Save
        // As, Open, Preview, and Cmd+B/I/U are handled outside the registry).
        // Crop mode owns the keyboard until Enter applies or Escape cancels, AI
        // Settings owns it while the dialog is open, and the start page, File page
        // and every modal dialog block the keys that change the course.
        let registry_keys_allowed = !preview_has_focus
            && !text_focus
            && !self.is_cropping()
            && !self.ui.ai.open
            && !self.ui.document.open
            && !self.ui.alt.open
            && !self.ui.translate.open
            && !self.global_keys_blocked();
        let shortcuts: &[RegistryShortcut] = if registry_keys_allowed {
            registry_shortcuts()
        } else {
            &[]
        };
        let soft_dialog = self.soft_dialog_open();
        for &(cmd, sc) in shortcuts {
            if soft_dialog && !matches!(cmd.id, "edit.undo" | "edit.redo") {
                continue;
            }
            if cmd.id == "object.delete"
                && self.ui.bottom_tab == BottomTab::States
                && self.ui.states.card_strip_focused
            {
                continue;
            }
            if ui.input_mut(|i| i.consume_shortcut(&sc)) {
                if cmd.id == "course.find" {
                    find::open(self);
                } else if cmd.id == "object.duplicate" && self.selection.is_multi() {
                    self.duplicate_selection();
                } else if cmd.id == "object.delete" {
                    if let Some((object, cue)) = self.ui.timeline.cue_selected.take() {
                        self.run(
                            "video.remove_cue",
                            serde_json::json!({
                                "slide": self.selection.slide,
                                "object": object,
                                "cue": cue,
                            }),
                        );
                    } else if let Some((layer, cue)) = self.ui.timeline.slide_cue_selected.take() {
                        let mut params = serde_json::json!({
                            "slide": self.selection.slide,
                            "cue": cue,
                        });
                        if let Some(lid) = layer {
                            params["layer"] = serde_json::json!(lid);
                        }
                        self.run("slide.remove_cue", params);
                    } else if self.ui.last_active_area == ActiveArea::Navigator
                        && !self.ui.master_view
                        && !self.ui.story_view
                    {
                        self.delete_selected_slides();
                    } else {
                        self.delete_selection();
                    }
                } else if let Some(params) = menu::adapter(self, cmd.id) {
                    self.run(cmd.id, params);
                }
            }
        }

        // Publish is a shell action, not an engine command, so it has its own binding.
        if !preview_has_focus
            && !self.ui.ai.open
            && !self.ui.alt.open
            && !self.ui.translate.open
            && self.file_ops_available
            && !self.global_keys_blocked()
            && let Some(sc) = menu::parse_shortcut("Ctrl+Shift+P")
            && !ui.ctx().egui_wants_keyboard_input()
            && ui.input_mut(|i| i.consume_shortcut(&sc))
        {
            self.open_publish_dialog();
        }

        // Also bind Delete / Backspace (when no text field has focus) to object.delete
        if !preview_has_focus
            && !self.is_cropping()
            && self.ui.placement_tool.is_none()
            && self.ui.draw.is_none()
            && !self.ui.ai.open
            && !self.ui.alt.open
            && !self.ui.translate.open
            && !ui.ctx().egui_wants_keyboard_input()
            && !ui.memory(|m| m.focused().is_some())
            && !(self.ui.bottom_tab == BottomTab::States && self.ui.states.card_strip_focused)
        {
            let keys_blocked = self.edit_keys_blocked();
            let del_pressed = !keys_blocked
                && ui.input_mut(|i| {
                    i.consume_key(Modifiers::NONE, Key::Delete)
                        || i.consume_key(Modifiers::NONE, Key::Backspace)
                });
            if del_pressed {
                if let Some((object, cue)) = self.ui.timeline.cue_selected.take() {
                    self.run(
                        "video.remove_cue",
                        serde_json::json!({
                            "slide": self.selection.slide,
                            "object": object,
                            "cue": cue,
                        }),
                    );
                } else if let Some((layer, cue)) = self.ui.timeline.slide_cue_selected.take() {
                    let mut params = serde_json::json!({
                        "slide": self.selection.slide,
                        "cue": cue,
                    });
                    if let Some(lid) = layer {
                        params["layer"] = serde_json::json!(lid);
                    }
                    self.run("slide.remove_cue", params);
                } else if self.ui.last_active_area == ActiveArea::Navigator
                    && !self.ui.master_view
                    && !self.ui.story_view
                {
                    self.delete_selected_slides();
                } else {
                    self.delete_selection();
                }
            }
            if self.ui.bottom_tab == BottomTab::Timeline
                && self.ui.timeline.focused
                && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::C))
            {
                let mut params = serde_json::json!({
                    "slide": self.selection.slide,
                    "at_ms": self.ui.timeline.playhead_ms,
                });
                if let Some(ref lid) = self.selection.layer {
                    params["layer"] = serde_json::json!(lid);
                }
                self.run("slide.add_cue", params);
            }

            if !keys_blocked && ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::A)) {
                self.select_all_objects();
            }

            let shift = ui.input(|i| i.modifiers.shift);
            let nudge_mods = if shift {
                Modifiers::SHIFT
            } else {
                Modifiers::NONE
            };
            let step = if shift { 10.0 } else { 1.0 };
            // A key that is not held ends an arrow-key burst: the next nudge is a new undo step.
            let arrow_down = ui.input(|i| {
                [
                    Key::ArrowLeft,
                    Key::ArrowRight,
                    Key::ArrowUp,
                    Key::ArrowDown,
                ]
                .iter()
                .any(|key| i.key_down(*key))
            });
            if !arrow_down {
                self.ui.nudge_burst = None;
            }
            // With the slide list active, Up and Down walk the slides; Home and End
            // jump to the first and last. Shift extends the range.
            let slide_list_keys = !keys_blocked
                && self.ui.last_active_area == ActiveArea::Navigator
                && !self.ui.master_view
                && !self.ui.story_view;
            let slide_nav = if slide_list_keys {
                [
                    (Key::ArrowUp, slides::SlideNav::Prev),
                    (Key::ArrowDown, slides::SlideNav::Next),
                    (Key::Home, slides::SlideNav::First),
                    (Key::End, slides::SlideNav::Last),
                ]
                .into_iter()
                .find(|(key, _)| ui.input_mut(|i| i.consume_key(nudge_mods, *key)))
            } else {
                None
            };
            if let Some((_, nav)) = slide_nav {
                self.navigate_slides(nav, shift);
            }
            // In the Course map the arrows move the map selection instead.
            let left = !keys_blocked
                && !self.ui.story_view
                && ui.input_mut(|i| i.consume_key(nudge_mods, Key::ArrowLeft));
            let right = !keys_blocked
                && !self.ui.story_view
                && ui.input_mut(|i| i.consume_key(nudge_mods, Key::ArrowRight));
            let up = !keys_blocked
                && !self.ui.story_view
                && !slide_list_keys
                && ui.input_mut(|i| i.consume_key(nudge_mods, Key::ArrowUp));
            let down = !keys_blocked
                && !self.ui.story_view
                && !slide_list_keys
                && ui.input_mut(|i| i.consume_key(nudge_mods, Key::ArrowDown));
            if left {
                self.nudge_selection(-step, 0.0);
            } else if right {
                self.nudge_selection(step, 0.0);
            } else if up {
                self.nudge_selection(0.0, -step);
            } else if down {
                self.nudge_selection(0.0, step);
            }

            let fit_pressed = ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Num0));
            let in_pressed = ui.input_mut(|i| {
                i.consume_key(Modifiers::COMMAND, Key::Equals)
                    || i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Plus)
                    || i.consume_key(Modifiers::COMMAND, Key::Plus)
            });
            let out_pressed = ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Minus));
            if fit_pressed {
                self.ui.zoom = ZoomLevel::Fit;
            } else if in_pressed {
                self.zoom_in();
            } else if out_pressed {
                self.zoom_out();
            }

            let question_pressed = ui.input_mut(|i| {
                i.consume_key(Modifiers::SHIFT, Key::Slash)
                    || i.consume_key(Modifiers::NONE, Key::Questionmark)
            });
            if question_pressed {
                self.ui.show_shortcuts_dialog = !self.ui.show_shortcuts_dialog;
            }
        }

        // The layer eyes follow the slide even while the Slide Layers panel is closed, so a
        // layer shown on one slide is not shown on the next (`N108`).
        self.ui
            .layers
            .sync(&self.session.course, &self.selection, self.session.revision);

        // Auto-switch inspector tab on selection change
        if self.selection.object != self.ui.last_selected_object {
            self.ui.preview_state = None;
            match (&self.selection.object, &self.ui.inspector_tab) {
                (Some(_), InspectorTab::Slide | InspectorTab::Course) => {
                    self.ui.inspector_tab = InspectorTab::Format;
                }
                (None, InspectorTab::Format) => {
                    self.ui.inspector_tab = InspectorTab::Slide;
                }
                _ => {}
            }
            self.ui.last_selected_object = self.selection.object.clone();
        }

        self.check_revision();

        if self.selection.object.is_none() {
            self.ui.preview_state = None;
        }

        if self.ui.master_view && self.ui.bottom_tab == BottomTab::States {
            dock::request_bottom_tab(self, BottomTab::Timeline);
        }
        // F12 previews this slide, Ctrl+F12 (Cmd+F12 on macOS) previews from here,
        // and Shift+F12 previews the whole course, like the Preview menu and toolbar.
        if !self.ui.show_welcome
            && self.preview.is_none()
            && !self.ui.master_view
            && !ui.ctx().egui_wants_keyboard_input()
        {
            if ui.input_mut(|i| i.consume_key(Modifiers::SHIFT, Key::F12)) {
                self.open_preview_at(None);
                requests.push(EditorRequest::Preview {
                    slide: String::new(),
                });
            } else if ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::F12)) {
                let slide_id = self.selection.slide.clone();
                self.open_preview_from(&slide_id);
                requests.push(EditorRequest::Preview { slide: slide_id });
            } else if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F12)) {
                toolbar::preview_slide(self, &mut requests);
            }
        }
        let p = coursecraft_render::palette(ui.ctx());
        // Horizontal inner margin shared by the menu row, bottom panel and status bar, so their
        // content never touches the window edge (matches the side panels).
        let edge_margin = egui::Margin::symmetric(coursecraft_render::PANEL_MARGIN as i8, 0);

        if self.ui.show_welcome && self.ui.ribbon.visible {
            // The File page is the start page (docs/ui/ribbon.md › File).
            dialog::set_top_inset(ui.ctx(), ui.max_rect().min.y);
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE.fill(p.app))
                .show(ui, |ui| file_page::show(self, ui, &mut requests));
        } else if self.ui.show_welcome {
            // Top menu bar
            let menu_bar = Panel::top("editor_menu_bar")
                .exact_size(TOP_BAR_HEIGHT)
                .resizable(false)
                .frame(egui::Frame::NONE.fill(p.panel).inner_margin(edge_margin))
                .show_separator_line(true)
                .show(ui, |ui| {
                    menu::show_menu_bar(self, ui, &mut requests);
                });
            dialog::set_top_inset(ui.ctx(), menu_bar.response.rect.max.y);

            // Welcome screen card across the window ground
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE.fill(p.app))
                .show(ui, |ui| {
                    welcome::show_welcome_screen(self, ui, &mut requests);
                });
        }
        if self.ui.show_welcome {
            shortcuts::show_shortcuts_dialog(self, ui.ctx());
            report::show_report_windows(self, ui.ctx(), &mut requests);
            translation::show_translation_windows(self, ui.ctx(), &mut requests);
            outline::show_outline_dialog(self, ui.ctx());
            document::show_document_dialog(self, ui.ctx(), &mut requests);
            publish::show_publish_dialog(self, ui.ctx(), &mut requests);
            export_dialog::show_export_dialog(self, ui.ctx(), &mut requests);
            export_dialog::show_export_review_dialog(self, ui.ctx(), &mut requests);
            ai_settings::show_ai_settings_window(self, ui.ctx(), &mut requests);
            alt_text::show_alt_text_window(self, ui.ctx(), &mut requests);
            audio_ai::show_captions_window(self, ui.ctx(), &mut requests);
            audio_ai::show_speech_window(self, ui.ctx(), &mut requests);
            translate_ai::show_translate_window(self, ui.ctx(), &mut requests);
            layers::show_layer_settings_window(self, ui.ctx());
            preferences::show_preferences_window(self, ui.ctx(), &mut requests);

            eyedropper::paint_overlay(ui.ctx());
            requests.append(&mut self.pending_requests);
            return requests;
        }

        // The File page covers the window until an action or Esc closes it.
        if self.ui.ribbon.visible && self.ui.file_page.open {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE.fill(p.app))
                .show(ui, |ui| file_page::show(self, ui, &mut requests));
            requests.append(&mut self.pending_requests);
            return requests;
        }
        if !self.ui.ribbon.visible {
            self.ui.file_page.open = false;
        }

        // 1. Top menu bar (panel fill, hairline below); the ribbon replaces it.
        let menu_h = if self.ui.ribbon.visible {
            0.0
        } else {
            TOP_BAR_HEIGHT
        };
        let menu_bar = Panel::top("editor_menu_bar")
            .exact_size(menu_h)
            .resizable(false)
            .frame(egui::Frame::NONE.fill(p.panel).inner_margin(edge_margin))
            .show_separator_line(menu_h > 0.0)
            .show(ui, |ui| {
                if menu_h > 0.0 {
                    menu::show_menu_bar(self, ui, &mut requests);
                }
            });

        // 1b. The ribbon (M37, the default), or the older flat toolbar when the
        // ribbon is turned off in Preferences.
        self.ui.ribbon.rect = egui::Rect::NOTHING;
        let top_panel = if self.ui.ribbon.visible {
            Panel::top("editor_ribbon")
                .exact_size(ribbon::height(&self.ui.ribbon))
                .resizable(false)
                .frame(egui::Frame::NONE.fill(p.panel).inner_margin(edge_margin))
                .show_separator_line(true)
                .show(ui, |ui| {
                    ribbon::show(self, ui, &mut requests);
                })
        } else {
            Panel::top("editor_toolbar")
                .exact_size(toolbar::TOOLBAR_HEIGHT)
                .resizable(false)
                .frame(egui::Frame::NONE.fill(p.panel).inner_margin(edge_margin))
                .show_separator_line(true)
                .show(ui, |ui| {
                    toolbar::show_toolbar(self, ui, &mut requests);
                })
        };
        // 1c. Document tabs (Course Map, opened slides) under the ribbon.
        let doc_tabs_h = if self.ui.ribbon.visible {
            Panel::top("editor_doc_tabs")
                .exact_size(doc_tabs::DOC_TABS_H)
                .resizable(false)
                .frame(egui::Frame::NONE.fill(p.panel).inner_margin(edge_margin))
                .show_separator_line(false)
                .show(ui, |ui| doc_tabs::show(self, ui))
                .response
                .rect
                .height()
        } else {
            0.0
        };
        let top_bottom = top_panel.response.rect.max.y + doc_tabs_h;
        let top_h = top_panel.response.rect.height() + doc_tabs_h;
        // Dialogs open below the toolbar or the ribbon.
        dialog::set_top_inset(ui.ctx(), top_bottom);

        // 2. Bottom status bar (26 px, panel fill, hairline above)
        let status_bar = Panel::bottom("editor_status_bar")
            .exact_size(STATUS_BAR_HEIGHT)
            .resizable(false)
            .frame(egui::Frame::NONE.fill(p.panel).inner_margin(edge_margin))
            .show_separator_line(true)
            .show(ui, |ui| {
                status::show_status_bar(self, ui);
            });
        ui.ctx().data_mut(|d| {
            d.insert_temp(
                egui::Id::unique("coursecraft_screen_rect"),
                egui::Rect::from_min_max(
                    egui::pos2(0.0, 0.0),
                    egui::pos2(menu_bar.response.rect.max.x, status_bar.response.rect.max.y),
                ),
            );
        });

        if !self.ui.dock.enabled {
            panels::show_floating(self, ui.ctx());
        }
        if self.ui.dock.enabled {
            // 3–6 docked: slides, canvas, timeline and Inspector in one workspace.
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE.fill(p.panel))
                .show(ui, |ui| dock::show(self, ui));
            coach::set_chrome_rects(
                ui.ctx(),
                top_panel.response.rect,
                self.ui
                    .dock
                    .rect(dock::Panel::Slides)
                    .unwrap_or(egui::Rect::NOTHING),
                self.ui.dock.inspector_rect().unwrap_or(egui::Rect::NOTHING),
            );
        } else {
            // 3. Bottom timeline panel. Keep at least 200 px of canvas above it.
            // Hidden in Master View, which edits slide masters without a timeline.
            if self.ui.master_view {
                self.ui.timeline.panel_rect = None;
                self.ui.timeline.tab_rects.clear();
            } else {
                let content_h = ui.ctx().content_rect().height();
                let chrome_h = menu_h + top_h + STATUS_BAR_HEIGHT;
                let max_by_canvas =
                    (content_h - chrome_h - 200.0).max(timeline::TIMELINE_PANEL_MIN_HEIGHT);
                let max_panel_h = (content_h * timeline::TIMELINE_PANEL_MAX_FRACTION)
                    .min(max_by_canvas)
                    .max(timeline::TIMELINE_PANEL_MIN_HEIGHT);
                let initial_panel_h = self
                    .ui
                    .timeline
                    .panel_height
                    .clamp(timeline::TIMELINE_PANEL_MIN_HEIGHT, max_panel_h);
                // The canvas/side panels are laid out after this panel and sit on top of the outer half
                // of the default 5px resize grab zone, leaving only ~4px inside the panel. Widen it and
                // keep the first `RESIZE_GRAB_PX` of the panel free of content so it is easy to hit.
                let old_grab = ui.style().interaction.resize_grab_radius_side;
                ui.style_mut().interaction.resize_grab_radius_side = timeline::PANEL_RESIZE_GRAB_PX;
                let timeline_panel = Panel::bottom("editor_timeline_panel")
                    .default_size(initial_panel_h)
                    .size_range(timeline::TIMELINE_PANEL_MIN_HEIGHT..=max_panel_h)
                    .resizable(true)
                    .frame(egui::Frame::NONE.fill(p.panel).inner_margin(edge_margin))
                    .show_separator_line(true)
                    .show(ui, |ui| {
                        // A resizable panel otherwise shrinks to its content; always fill the
                        // size the user picked (or the default).
                        ui.set_min_height(ui.available_height());
                        ui.add_space(timeline::PANEL_RESIZE_GRAB_PX - 2.0);
                        self.timeline_body(ui);
                    });
                ui.style_mut().interaction.resize_grab_radius_side = old_grab;
                self.ui.timeline.panel_rect = Some(timeline_panel.response.rect);
                self.ui.timeline.panel_height = timeline_panel.response.rect.height();
                if ui.input(|i| i.pointer.primary_pressed())
                    && let Some(pos) = ui.input(|i| i.pointer.interact_pos())
                {
                    self.ui.timeline.focused = timeline_panel.response.rect.contains(pos);
                }
            }

            // 4. Left slides panel
            let slides_panel = Panel::left("editor_slides_panel")
                .default_size(220.0)
                .size_range(180.0..=320.0)
                .resizable(true)
                .frame(egui::Frame::NONE.fill(p.panel))
                .show_separator_line(true)
                .show(ui, |ui| self.slides_body(ui));

            // 5. Right properties panel (Inspector: Format, Animate, Slide, Course)
            let properties_panel = Panel::right("editor_properties_panel")
                .default_size(PROPERTIES_PANEL_WIDTH)
                .size_range(260.0..=400.0)
                .resizable(true)
                .frame(egui::Frame::NONE.fill(p.panel))
                .show_separator_line(true)
                .show(ui, |ui| self.inspector_body(ui, None));
            coach::set_chrome_rects(
                ui.ctx(),
                top_panel.response.rect,
                slides_panel.response.rect,
                properties_panel.response.rect,
            );

            // 6. Central canvas and layers strip, or the course map.
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE.fill(p.canvas))
                .show(ui, |ui| self.canvas_body(ui));
        }

        // Insert menu's shape gallery (search needs a real text field).
        shape_gallery::show_shape_gallery_window(self, ui.ctx());

        // Insert ▸ Interaction gallery.
        interaction_gallery::show_interaction_gallery_window(self, ui.ctx());

        // Insert ▸ Web object dialog.
        web_insert::show_web_insert_window(self, ui.ctx(), &mut requests);

        // Insert ▸ Symbol and Reference windows.
        insert_text::show_symbol_window(self, ui.ctx());
        insert_text::show_reference_window(self, ui.ctx());

        // Delete-scene confirmation
        slides::show_scene_delete_dialog(self, ui.ctx());

        // Delete-master confirmation
        slides::show_master_delete_dialog(self, ui.ctx());

        // Trigger Editor dialog window
        triggers::show_trigger_editor_window(self, ui.ctx());

        // Layer Settings dialog window
        layers::show_layer_settings_window(self, ui.ctx());

        // Course Settings dialog window
        course_settings::show_course_settings_window(self, ui.ctx());

        // Theme dialog window
        theme_dialog::show_theme_dialog_window(self, ui.ctx());
        clipboard::show_paste_special_window(self, ui.ctx());
        focus_order::show_focus_order_window(self, ui.ctx());
        text_styles_ui::show_manage_styles_window(self, ui.ctx());
        slide_size_dialog::show_slide_size_dialog(self, ui.ctx(), &mut requests);
        preferences::show_preferences_window(self, ui.ctx(), &mut requests);
        version_history::show_version_history_window(self, ui.ctx(), &mut requests);
        report::show_report_windows(self, ui.ctx(), &mut requests);

        // XLIFF export language dialog and import report
        translation::show_translation_windows(self, ui.ctx(), &mut requests);

        // Slides from Outline window
        outline::show_outline_dialog(self, ui.ctx());

        // Course from Document window
        document::show_document_dialog(self, ui.ctx(), &mut requests);

        // Find and Replace window
        find::show_find_window(self, ui.ctx());

        // Check Spelling window
        spell_check::show_spelling_window(self, ui.ctx());

        // Publish dialog
        publish::show_publish_dialog(self, ui.ctx(), &mut requests);

        // File ▸ Export ▸ Video (MP4)…
        export_dialog::show_export_dialog(self, ui.ctx(), &mut requests);
        export_dialog::show_export_review_dialog(self, ui.ctx(), &mut requests);

        // Optional AI endpoint settings
        ai_settings::show_ai_settings_window(self, ui.ctx(), &mut requests);

        // Suggest alt text for images
        alt_text::show_alt_text_window(self, ui.ctx(), &mut requests);

        // Captions from audio and narration from text
        audio_ai::show_captions_window(self, ui.ctx(), &mut requests);
        audio_ai::show_speech_window(self, ui.ctx(), &mut requests);

        // Record narration from the microphone
        record_narration::show_record_narration_window(self, ui.ctx());

        // Translate with AI
        translate_ai::show_translate_window(self, ui.ctx(), &mut requests);

        // In-editor preview window
        if self.preview.is_some() {
            let mut open = true;
            if let Some(ref mut preview) = self.preview {
                let source = MemoryMediaSource::from_store(&self.media);
                preview.show(ui.ctx(), &mut open, Some((&mut self.media_cache, &source)));
                if let Some(size) = preview.remembered_size() {
                    self.ui.preview_size = Some(size);
                }
                requests.append(&mut preview.requests);
            }
            if !open {
                self.preview = None;
            }
        }

        // First-run coach marks
        coach::show_coach_marks(self, ui);

        // Keyboard shortcuts dialog
        shortcuts::show_shortcuts_dialog(self, ui.ctx());

        // Custom font manager
        fonts::show_fonts_window(self, ui.ctx());

        // History window
        history::show_history_window(self, ui.ctx());

        // Media Library window
        media_library::show_media_library(self, ui.ctx());

        if let Some(text) = self.ui.pending_copy_text.take() {
            ui.ctx().copy_text(text);
        }

        self.check_revision();

        eyedropper::paint_overlay(ui.ctx());
        let any_popup_open = egui::Popup::is_any_open(ui.ctx());
        ui.ctx().data_mut(|d| {
            d.insert_temp(
                egui::Id::unique("coursecraft_popup_open_prev_frame"),
                any_popup_open,
            );
        });
        widgets::end_press_guard(ui.ctx());
        if !ui.input(|i| i.pointer.any_down())
            && let Some(layout) = self.layout_changed()
        {
            requests.push(EditorRequest::SaveLayout(layout));
        }
        requests.append(&mut self.pending_requests);
        requests
    }
}

/// Accent-filled primary action button with white text.
pub fn primary_button(ui: &mut Ui, label: impl Into<egui::WidgetText>) -> egui::Response {
    let p = coursecraft_render::palette(ui.ctx());
    let btn = egui::Button::new(label.into().color(egui::Color32::WHITE))
        .fill(p.accent)
        .corner_radius(coursecraft_render::RADIUS_MD as u8)
        .min_size(egui::vec2(0.0, coursecraft_render::CONTROL_PRIMARY));
    ui.add(btn)
}

/// In-memory media source borrowing from an editor's media assets.
pub struct MemoryMediaSource<'a> {
    pub media: &'a [(String, std::sync::Arc<[u8]>)],
    pub index: Option<&'a std::collections::HashMap<String, usize>>,
}

impl<'a> MemoryMediaSource<'a> {
    pub(crate) fn from_store(store: &'a MediaStore) -> Self {
        Self {
            media: store.as_slice(),
            index: Some(store.index()),
        }
    }

    fn linear_get(&self, src: &str) -> coursecraft_render::MediaFetch {
        for (name, bytes) in self.media {
            if name == src {
                return coursecraft_render::MediaFetch::Ready(std::sync::Arc::clone(bytes));
            }
        }
        coursecraft_render::MediaFetch::Missing
    }
}

impl coursecraft_render::MediaSource for MemoryMediaSource<'_> {
    fn get(&self, src: &str) -> coursecraft_render::MediaFetch {
        if let Some(index) = self.index {
            return match index.get(src) {
                Some(&idx) => match self.media.get(idx) {
                    Some((name, bytes)) if name == src => {
                        coursecraft_render::MediaFetch::Ready(std::sync::Arc::clone(bytes))
                    }
                    _ => self.linear_get(src),
                },
                None => coursecraft_render::MediaFetch::Missing,
            };
        }
        self.linear_get(src)
    }
}

impl coursecraft_render::MediaSource for Editor {
    fn get(&self, src: &str) -> coursecraft_render::MediaFetch {
        match self.media_get(src) {
            Some(bytes) => coursecraft_render::MediaFetch::Ready(std::sync::Arc::clone(bytes)),
            None => coursecraft_render::MediaFetch::Missing,
        }
    }
}

/// Media source for the authoring canvas: bytes from the editor's store
/// plus timeline video frames from [`TimelinePlayback`].
pub struct TimelineMediaSource<'a> {
    /// Byte source (images, posters).
    pub inner: &'a MemoryMediaSource<'a>,
    /// Timeline playback owning the slide's tracks.
    pub playback: &'a timeline_playback::TimelinePlayback,
    /// The playhead the author sees.
    pub playhead_ms: u32,
}

impl coursecraft_render::MediaSource for TimelineMediaSource<'_> {
    fn get(&self, src: &str) -> coursecraft_render::MediaFetch {
        self.inner.get(src)
    }

    fn video_frame(&self, object: &str) -> Option<std::sync::Arc<VideoFrame>> {
        self.playback.video_frame_for(object, self.playhead_ms)
    }

    fn video_error(&self, object: &str) -> Option<&str> {
        self.playback.video_error_for(object)
    }
}

/// Collect all media `src` paths referenced by objects and states across the
/// course: play slides, masters and templates. Used for keeping media on save.
#[must_use]
pub fn referenced_media_srcs(course: &Course) -> std::collections::HashSet<String> {
    let mut set = std::collections::HashSet::new();
    coursecraft_course::visit_course_media(
        course,
        coursecraft_course::MediaScope::AllIncludingTemplates,
        &mut |r| {
            if r.role == coursecraft_course::MediaRole::WebFolder {
                set.insert(format!("{}/{}", r.src, r.web_entry.unwrap_or("")));
            } else if !r.src.is_empty() {
                set.insert(r.src.to_owned());
            }
        },
    );
    set
}

/// Prefixes (`web/<name>/`) of every folder-sourced web object, for keeping
/// the whole folder on save and out of the unused-media filter.
#[must_use]
pub fn web_folder_prefixes(course: &Course) -> Vec<String> {
    let mut prefixes = std::collections::BTreeSet::new();
    coursecraft_course::visit_course_media(
        course,
        coursecraft_course::MediaScope::AllIncludingTemplates,
        &mut |r| {
            if r.role == coursecraft_course::MediaRole::WebFolder {
                prefixes.insert(format!("{}/", r.src));
            }
        },
    );
    prefixes.into_iter().collect()
}

fn rewrite_slide_media_srcs(
    slide: &mut coursecraft_course::Slide,
    renames: &std::collections::HashMap<String, String>,
) {
    for obj in &mut slide.objects {
        rewrite_object_media_srcs(obj, renames);
    }
    for layer in &mut slide.layers {
        for obj in &mut layer.objects {
            rewrite_object_media_srcs(obj, renames);
        }
    }
}

fn rewrite_object_media_srcs(
    obj: &mut coursecraft_course::Object,
    renames: &std::collections::HashMap<String, String>,
) {
    let remap = |s: &mut String| {
        if let Some(next) = renames.get(s.as_str()) {
            *s = next.clone();
        }
    };
    match &mut obj.kind {
        coursecraft_course::ObjectKind::Image { src, .. }
        | coursecraft_course::ObjectKind::Lottie { src, .. } => remap(src),
        coursecraft_course::ObjectKind::Audio { src, captions, .. } => {
            remap(src);
            if let Some(captions) = captions {
                remap(captions);
            }
        }
        coursecraft_course::ObjectKind::Video {
            src,
            captions,
            poster,
            ..
        } => {
            remap(src);
            if let Some(captions) = captions {
                remap(captions);
            }
            if let Some(poster) = poster {
                remap(poster);
            }
        }
        coursecraft_course::ObjectKind::Shape { fill_style, .. }
        | coursecraft_course::ObjectKind::Button { fill_style, .. } => {
            if let Some(coursecraft_course::FillStyle::Picture { src, .. }) = fill_style {
                remap(src);
            }
        }
        coursecraft_course::ObjectKind::Path { path } => {
            if let Some(coursecraft_course::FillStyle::Picture { src, .. }) = &mut path.fill_style {
                remap(src);
            }
        }
        _ => {}
    }
    for state in &mut obj.states {
        if let Some(src) = &mut state.src {
            remap(src);
        }
        if let Some(coursecraft_course::FillStyle::Picture { src, .. }) = &mut state.fill_style {
            remap(src);
        }
        for extra in &mut state.extras {
            match &mut extra.kind {
                coursecraft_course::StateExtraKind::Image { src, .. } => remap(src),
                coursecraft_course::StateExtraKind::Shape { fill_style, .. } => {
                    if let Some(coursecraft_course::FillStyle::Picture { src, .. }) = fill_style {
                        remap(src);
                    }
                }
                coursecraft_course::StateExtraKind::Path { path } => {
                    if let Some(coursecraft_course::FillStyle::Picture { src, .. }) =
                        &mut path.fill_style
                    {
                        remap(src);
                    }
                }
                coursecraft_course::StateExtraKind::Text { .. } => {}
            }
        }
    }
}

/// Sanitize a file name into a safe, unique media source key matching `[A-Za-z0-9._-]`.
#[must_use]
/// Whether any text in `object` names the family matched by `matches`.
fn object_uses_font(object: &coursecraft_course::Object, matches: &impl Fn(&str) -> bool) -> bool {
    let text_uses = |style: &coursecraft_course::TextStyle,
                     runs: &[coursecraft_course::TextRun]| {
        matches(&style.font)
            || runs
                .iter()
                .any(|run| run.font.as_deref().is_some_and(matches))
    };
    let base_uses = match &object.kind {
        coursecraft_course::ObjectKind::Text { style, runs, .. } => text_uses(style, runs),
        coursecraft_course::ObjectKind::Shape { text, .. } => text
            .as_ref()
            .is_some_and(|text| text_uses(&text.style, &text.runs)),
        _ => false,
    };
    base_uses
        || object.states.iter().any(|state| {
            state
                .text_style
                .as_ref()
                .is_some_and(|style| matches(&style.font))
                || state.runs.as_deref().is_some_and(|runs| {
                    runs.iter()
                        .any(|run| run.font.as_deref().is_some_and(matches))
                })
                || state.extras.iter().any(|extra| match &extra.kind {
                    coursecraft_course::StateExtraKind::Text { style, runs, .. } => {
                        text_uses(style, runs)
                    }
                    coursecraft_course::StateExtraKind::Shape {
                        text: Some(text), ..
                    } => text_uses(&text.style, &text.runs),
                    coursecraft_course::StateExtraKind::Shape { text: None, .. }
                    | coursecraft_course::StateExtraKind::Image { .. }
                    | coursecraft_course::StateExtraKind::Path { .. } => false,
                })
        })
}

/// Sanitize a font file name into a unique `fonts/<stem>.<ext>` media path.
#[must_use]
pub fn sanitize_font_src(file_name: &str, existing: &[(String, std::sync::Arc<[u8]>)]) -> String {
    let path = std::path::Path::new(file_name);
    let raw_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("font");
    let raw_ext = path.extension().and_then(|s| s.to_str()).unwrap_or("ttf");
    let clean_stem: String = raw_stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let stem = if clean_stem.trim_matches('-').is_empty() {
        "font"
    } else {
        clean_stem.trim_matches('-')
    };
    let ext = if raw_ext.eq_ignore_ascii_case("otf") {
        "otf"
    } else {
        "ttf"
    };
    let mut counter = 1u32;
    loop {
        let name = if counter == 1 {
            format!("fonts/{stem}.{ext}")
        } else {
            format!("fonts/{stem}-{counter}.{ext}")
        };
        if !existing
            .iter()
            .any(|(src, _)| src.eq_ignore_ascii_case(&name))
        {
            return name;
        }
        counter = counter.saturating_add(1);
    }
}

/// Whether picked bytes are EMF or WMF clip art: by extension, or by content
/// for a file whose extension names no format CourseCraft reads as a picture.
fn is_metafile_import(file_name: &str, bytes: &[u8]) -> bool {
    if coursecraft_media::is_metafile_name(file_name) {
        return true;
    }
    let lower = file_name.to_ascii_lowercase();
    let known = [
        ".png", ".jpg", ".jpeg", ".webp", ".gif", ".svg", ".svgz", ".bmp", ".ico", ".tif", ".tiff",
    ];
    !known.iter().any(|ext| lower.ends_with(ext)) && coursecraft_media::is_metafile(bytes)
}

/// Text faces for metafile import: the course's own fonts, matched by family
/// (any case) and style, else the family's regular face.
struct CourseFonts<'a> {
    faces: &'a [coursecraft_course::FontFace],
    media: &'a media_store::MediaStore,
}

impl coursecraft_media::MetafileFonts for CourseFonts<'_> {
    fn face(&self, family: &str, bold: bool, italic: bool) -> Option<(std::sync::Arc<[u8]>, u32)> {
        let style = match (bold, italic) {
            (false, false) => FontStyle::Regular,
            (true, false) => FontStyle::Bold,
            (false, true) => FontStyle::Italic,
            (true, true) => FontStyle::BoldItalic,
        };
        let family_faces = || {
            self.faces
                .iter()
                .filter(|f| f.family.eq_ignore_ascii_case(family.trim()))
        };
        let face = family_faces()
            .find(|f| f.style == style)
            .or_else(|| family_faces().find(|f| f.style == FontStyle::Regular))?;
        self.media.get(&face.src).map(|bytes| (bytes.clone(), 0))
    }
}

pub fn sanitize_media_src(file_name: &str, existing: &[(String, std::sync::Arc<[u8]>)]) -> String {
    let path = std::path::Path::new(file_name);
    let raw_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("image");
    let raw_ext = path.extension().and_then(|s| s.to_str()).unwrap_or("png");

    let clean_stem: String = raw_stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let stem = if clean_stem.trim_matches('-').is_empty() {
        "image"
    } else {
        clean_stem.trim_matches('-')
    };

    let clean_ext: String = raw_ext
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect();
    let ext = if clean_ext.is_empty() {
        "png"
    } else {
        &clean_ext
    };

    let base = format!("{stem}.{ext}");
    if !existing.iter().any(|(s, _)| s.eq_ignore_ascii_case(&base)) {
        return base;
    }

    let mut counter = 2;
    loop {
        let candidate = format!("{stem}-{counter}.{ext}");
        if !existing
            .iter()
            .any(|(s, _)| s.eq_ignore_ascii_case(&candidate))
        {
            return candidate;
        }
        counter += 1;
    }
}

fn slide_layout_matches(
    course: &coursecraft_course::Course,
    known: &[(String, Vec<String>)],
) -> bool {
    course.scenes.len() == known.len()
        && course
            .scenes
            .iter()
            .zip(known)
            .all(|(scene, (scene_id, slide_ids))| {
                scene.id == *scene_id
                    && scene.slides.len() == slide_ids.len()
                    && scene
                        .slides
                        .iter()
                        .zip(slide_ids)
                        .all(|(slide, id)| slide.id == *id)
            })
}

/// Extract the canonical trigger path prefix from a validation issue path
/// (e.g. `"/scenes/0/slides/2/triggers/1"` from `"/scenes/0/slides/2/triggers/1/action"`),
/// or `None` if the path does not refer to a specific trigger.
pub(crate) fn trigger_path_key(path: &str) -> Option<&str> {
    let marker = "/triggers/";
    let idx = path.find(marker)?;
    let rest = &path[idx + marker.len()..];
    let seg = rest.split('/').next()?;
    if seg.is_empty() || !seg.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(&path[..idx + marker.len() + seg.len()])
}

fn index_trigger_issues(
    issues: &[ValidationIssueSummary],
) -> std::collections::HashMap<String, Vec<ValidationIssueSummary>> {
    let mut map: std::collections::HashMap<String, Vec<ValidationIssueSummary>> =
        std::collections::HashMap::new();
    for issue in issues {
        if let Some(key) = trigger_path_key(&issue.path) {
            map.entry(key.to_owned()).or_default().push(issue.clone());
        }
    }
    map
}

fn parse_audit_result(val: &Json) -> (Vec<AuditFindingSummary>, Vec<String>) {
    let findings = val
        .get("findings")
        .and_then(Json::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| AuditFindingSummary {
                    severity: item
                        .get("severity")
                        .and_then(Json::as_str)
                        .unwrap_or("note")
                        .to_owned(),
                    check: item
                        .get("check")
                        .and_then(Json::as_str)
                        .unwrap_or("")
                        .to_owned(),
                    criterion: item
                        .get("criterion")
                        .and_then(Json::as_str)
                        .unwrap_or("")
                        .to_owned(),
                    slide: item.get("slide").and_then(Json::as_str).map(str::to_owned),
                    object: item.get("object").and_then(Json::as_str).map(str::to_owned),
                    message: item
                        .get("message")
                        .and_then(Json::as_str)
                        .unwrap_or("")
                        .to_owned(),
                    fix: item
                        .get("fix")
                        .and_then(Json::as_str)
                        .unwrap_or("")
                        .to_owned(),
                })
                .collect()
        })
        .unwrap_or_default();
    let manual_checks = val
        .get("manual_checks")
        .and_then(Json::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Json::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    (findings, manual_checks)
}

/// Tests only: with `CC_FAILED_LOG` set, append every failed command (test
/// name, id, error, params) to that file, to sweep the suite for silent failures.
#[allow(unused_variables)]
fn log_failed_for_test(id: &str, params: &Json, msg: &str) {
    #[cfg(test)]
    if let Ok(path) = std::env::var("CC_FAILED_LOG") {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let name = std::thread::current().name().unwrap_or("").to_owned();
            let _ = writeln!(f, "{name}\t{id}\t{msg}\t{params}");
        }
    }
}
