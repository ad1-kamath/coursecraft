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

pub mod ai_settings;
pub mod alt_text;
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
pub mod find;
pub mod fonts;
pub mod format_painter;
pub mod input_insert;
pub mod interaction_insert;
pub mod layers;
pub mod media_library;
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
pub mod states;
pub mod status;
pub mod text_styles_ui;
pub mod theme_dialog;
pub mod timeline;
pub mod timeline_playback;
pub mod toolbar;
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
mod document_tests;
#[cfg(test)]
mod find_tests;
#[cfg(test)]
mod layout_tests;
#[cfg(test)]
mod outline_tests;
#[cfg(test)]
mod perf_tests;
#[cfg(test)]
pub(crate) mod test_harness;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod translate_tests;
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
    show_shape_gallery_selected, show_shape_gallery_window,
};
pub use shortcuts::show_shortcuts_dialog;
pub use slides::{
    PANEL_PADDING_X, PANEL_PADDING_Y, SCENE_HEADER_HEIGHT, SLIDE_INFO_HEIGHT, SLIDE_ROW_GAP,
    SLIDES_PANEL_WIDTH, THUMBNAIL_HEIGHT, THUMBNAIL_WIDTH, show_slides_panel, slide_thumbnail_rect,
};
pub use snapshot::{
    SnapshotSelect, WINDOWS, WindowSpec, open_window, snapshot_editor, window_names, window_spec,
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
pub use welcome::{ExampleId, example_course_and_media, show_welcome_screen};
pub use widgets::{
    IconTone, icon_button, icon_button_danger, icon_button_enabled, icon_toggle, toolbar_button,
    toolbar_button_active, toolbar_button_enabled, toolbar_caret_button, toolbar_primary_button,
    toolbar_toggle,
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
    SetAutomation(bool),
    /// Delete the saved crash note: it was reported or dismissed.
    DiscardCrashNote,
    SetTheme(coursecraft_render::ThemePref),
    /// Save the autosave interval, the number of file versions to keep and the
    /// default slide size for new courses.
    SetPreferences {
        autosave_minutes: u32,
        versions_to_keep: u32,
        default_slide: (u32, u32),
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
    OpenExample(ExampleId),
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
}

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
            Self::TextBox => "Text Box".to_owned(),
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
    /// Active slide-thumbnail drag in the navigator.
    pub slide_drag: Option<slides::SlideDrag>,
    pub last_seen_revision: u64,
    pub last_error: Option<(String, f64)>,
    pub error_history: Vec<String>,
    pub validation_errors: usize,
    pub validation_warnings: usize,
    pub validation_issues: Vec<ValidationIssueSummary>,
    pub audit_findings: Vec<AuditFindingSummary>,
    pub audit_manual_checks: Vec<String>,
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
    /// Objects not drawn on the authoring canvas. Editor only.
    pub hidden_objects: std::collections::BTreeSet<String>,
    /// Objects the canvas cannot select or move. Editor only.
    pub locked_objects: std::collections::BTreeSet<String>,
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
    /// Insert ▸ Web object window and its URL field.
    pub show_web_window: bool,
    pub web_url: String,
    pub web_error: Option<String>,
    pub canvas_drag: Option<CanvasDrag>,
    /// The table cell selected in the inspector and by a canvas click.
    pub table_cell: Option<(usize, usize)>,
    pub inline_edit: Option<canvas::InlineEdit>,
    /// Image crop mode: the canvas edits a window, Apply writes it in one step.
    pub crop_edit: Option<CropSession>,
    /// Active pointer drag inside crop mode.
    pub(crate) crop_drag: Option<canvas::CropDrag>,
    pub scene_rename: Option<slides::SceneRename>,
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
    /// Font face waiting for a "remove while in use" confirmation.
    pub pending_remove_font: Option<(String, FontStyle)>,
    /// Course map replaces the slide editor in the centre.
    pub story_view: bool,
    /// Transient Course map state.
    pub course_map: CourseMapState,
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

/// Main CourseCraft editor component.
pub struct Editor {
    pub session: Session,
    pub selection: Selection,
    pub ui: UiState,
    pub preview: Option<Preview>,
    pub recent_files: Vec<String>,
    pub file_ops_available: bool,
    pub publish_available: bool,
    pub title_info: Option<(String, bool)>,
    pub media: Vec<(String, std::sync::Arc<[u8]>)>,
    pub media_cache: coursecraft_render::MediaCache,
    pub pending_replace_object: Option<String>,
    pub pending_requests: Vec<EditorRequest>,
    pub automation_available: bool,
    /// This app's own binary, for the Connect an AI agent dialog.
    pub agent_exe_path: Option<String>,
    pub automation_enabled: bool,
    pub automation_port: Option<u16>,
    pub theme_pref: coursecraft_render::ThemePref,
    /// Timeline playback (authoring-only: no runtime, no document writes).
    pub timeline_playback: timeline_playback::TimelinePlayback,
    /// Session revision the timeline playback last saw; a change stops Play.
    pub timeline_last_revision: u64,
    /// Monotonic timeline clock in milliseconds, from frame deltas.
    pub timeline_mono_ms: u64,
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
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(size, sense);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, text.to_owned()));
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
    if enabled {
        resp.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        resp
    }
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
        let mut selection = Selection::new(first_slide);
        selection.repair(&session.course);

        let mut editor = Self {
            session,
            selection,
            ui: UiState {
                slides_scroll_y: 0.0,
                slides_scrolled_to: None,
                slide_drag: None,
                last_seen_revision: u64::MAX,
                last_error: None,
                error_history: Vec::new(),
                validation_errors: 0,
                validation_warnings: 0,
                validation_issues: Vec::new(),
                audit_findings: Vec::new(),
                audit_manual_checks: Vec::new(),
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
                hidden_objects: std::collections::BTreeSet::new(),
                locked_objects: std::collections::BTreeSet::new(),
                format_painter: None,
                placement_drag: None,
                draw: None,
                path_edit: None,
                shape_gallery_query: String::new(),
                shape_gallery_rects: Vec::new(),
                shape_gallery_draw_rects: Vec::new(),
                shape_gallery_search_rect: None,
                show_shape_gallery_window: false,
                show_web_window: false,
                web_url: String::new(),
                web_error: None,
                canvas_drag: None,
                table_cell: None,
                inline_edit: None,
                crop_edit: None,
                crop_drag: None,
                scene_rename: None,
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
                group_focus: None,
                align_to_slide: false,
                master_view: false,
                master_view_return_slide: None,
                pending_delete_master: None,
                master_rename: None,
                bank_rename: None,
                bank_expanded: Vec::new(),
                show_media_library: false,
                media_library: media_library::MediaLibraryState::default(),
                show_fonts_window: false,
                pending_remove_font: None,
                story_view: false,
                course_map: CourseMapState::default(),
            },
            preview: None,
            recent_files: Vec::new(),
            file_ops_available: true,
            publish_available: true,
            title_info: None,
            media: Vec::new(),
            media_cache: coursecraft_render::MediaCache::new(),
            pending_replace_object: None,
            pending_requests: Vec::new(),
            automation_available: false,
            agent_exe_path: None,
            automation_enabled: true,
            automation_port: None,
            theme_pref: coursecraft_render::ThemePref::Dark,
            timeline_playback: timeline_playback::TimelinePlayback::create_default(),
            timeline_last_revision: initial_revision,
            timeline_mono_ms: 0,
        };
        editor.check_revision();
        editor
    }

    /// Set the user's active theme preference.
    pub fn set_theme_pref(&mut self, pref: coursecraft_render::ThemePref) {
        self.theme_pref = pref;
    }

    /// Update recent files list shown in the File menu.
    pub fn set_recent(&mut self, recent: Vec<String>) {
        self.recent_files = recent;
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
        self.timeline_playback.sync_slide(
            &self.session.course,
            &slide_id,
            duration_ms,
            &self.media,
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
        self.ui.slide_drag = None;
        self.ui.timeline.active_row_drag = None;
        self.ui.last_seen_revision = u64::MAX;
        self.ui.last_error = None;
        self.ui.error_history.clear();
        self.ui.placement_tool = None;
        self.ui.hidden_objects.clear();
        self.ui.copied_triggers.clear();
        self.ui.locked_objects.clear();
        self.ui.placement_drag = None;
        self.ui.shape_gallery_query.clear();
        self.ui.shape_gallery_rects.clear();
        self.ui.shape_gallery_search_rect = None;
        self.ui.show_shape_gallery_window = false;
        self.ui.show_web_window = false;
        self.ui.web_error = None;
        self.ui.canvas_drag = None;
        self.ui.inline_edit = None;
        self.ui.crop_edit = None;
        self.ui.crop_drag = None;
        self.ui.scene_rename = None;
        self.ui.pending_delete_scene = None;
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
        self.ui.inspector_tab = InspectorTab::Slide;
        self.ui.last_selected_object = None;
        self.ui.zoom = ZoomLevel::Fit;
        self.ui.show_welcome = false;
        self.ui.has_loaded_course = true;
        self.ui.show_shortcuts_dialog = false;
        self.ui.last_active_area = ActiveArea::Canvas;
        self.ui.clipboard = None;
        self.ui.clipboard_text = None;
        self.ui.clipboard_paste_count = 0;
        self.ui.pending_copy_text = None;
        self.ui.status_message = None;
        self.ui.snap_guides.clear();
        self.ui.group_focus = None;
        self.ui.master_view = false;
        self.ui.master_view_return_slide = None;
        self.ui.pending_delete_master = None;
        self.ui.master_rename = None;
        self.ui.bank_rename = None;
        self.ui.bank_expanded.clear();
        self.ui.show_media_library = false;
        self.ui.media_library = media_library::MediaLibraryState::default();
        self.ui.story_view = false;
        self.ui.course_map = CourseMapState::default();
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
        self.pending_replace_object = None;
        self.pending_requests.clear();

        self.check_revision();
    }

    /// Open one of the built-in example courses (Hello course or Feature showcase).
    pub fn open_example(&mut self, id: ExampleId) {
        if let Ok((course, media)) = welcome::example_course_and_media(id) {
            self.replace_course(course);
            self.set_media(media);
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
        self.media = media;
        self.media_cache = coursecraft_render::MediaCache::new();
    }

    /// Access the in-memory media list.
    #[must_use]
    pub fn media(&self) -> &[(String, std::sync::Arc<[u8]>)] {
        &self.media
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
        let previous_replace = self.pending_replace_object.take();
        let src = if is_image || lower.ends_with(".svg") || lower.ends_with(".svgz") {
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
                    self.media.push((src.clone(), bytes.into()));
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
                self.media.push((src.clone(), bytes.into()));
                Some(src)
            }
        };
        self.ui.placement_tool = previous_tool;
        self.pending_replace_object = previous_replace;
        let src = src?;
        self.mark_media_changed();
        self.set_status(format!("Added {src}. File operations are not undoable."));
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
        let Some((_, slot)) = self.media.iter_mut().find(|(name, _)| name == src) else {
            return false;
        };
        *slot = bytes.into();
        self.media_cache = coursecraft_render::MediaCache::new();
        self.mark_media_changed();
        self.set_status(format!("Replaced {src}. File operations are not undoable."));
        true
    }

    /// Return the media assets referenced by any object in the course, suitable for saving.
    /// Unused media is excluded from the result but kept in memory so undo can restore it;
    /// in-memory media is only released when the project is replaced.
    #[must_use]
    pub fn used_media(&self) -> Vec<(String, Vec<u8>)> {
        let referenced = referenced_media_srcs(&self.session.course);
        let folders = web_folder_prefixes(&self.session.course);
        self.media
            .iter()
            .filter(|(src, _)| {
                referenced.contains(src) || folders.iter().any(|prefix| src.starts_with(prefix))
            })
            .map(|(src, bytes)| (src.clone(), bytes.to_vec()))
            .collect()
    }

    /// Media files the course references but the editor's media store does not
    /// contain, sorted for a stable report. Publishing would fail on these, so
    /// the Publish dialog blocks before building.
    #[must_use]
    pub fn missing_media(&self) -> Vec<String> {
        let mut referenced = std::collections::HashSet::new();
        for face in &self.session.course.fonts {
            if !face.src.is_empty() {
                referenced.insert(face.src.clone());
            }
        }
        for slide in self
            .session
            .course
            .slides()
            .chain(self.session.course.masters.iter())
        {
            for obj in &slide.objects {
                collect_object_media_srcs(obj, &mut referenced);
            }
            for layer in &slide.layers {
                for obj in &layer.objects {
                    collect_object_media_srcs(obj, &mut referenced);
                }
            }
        }
        referenced
            .into_iter()
            .filter(|src| !self.media.iter().any(|(name, _)| name == src))
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
        for (name, bytes) in &self.media {
            if name == src {
                let lower = name.to_ascii_lowercase();
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

    /// Import an image from raw bytes, validating with the decoder and learning natural dimensions.
    ///
    /// Stores the bytes under a sanitized, unique `src` key. If an image object is queued for replacement,
    /// it updates that object; otherwise, it arms the Image placement tool.
    pub fn import_image(&mut self, file_name: &str, bytes: &[u8]) -> Option<String> {
        let lower = file_name.to_ascii_lowercase();
        let is_svg = lower.ends_with(".svg") || lower.ends_with(".svgz");
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
                    format!("Animated image '{file_name}' will show only its first frame: {e}");
                self.ui.error_history.push(msg.clone());
                self.set_status(msg);
            }
            (decoded.width as f32, decoded.height as f32)
        };

        let src = sanitize_media_src(file_name, &self.media);
        let arc_bytes: std::sync::Arc<[u8]> = bytes.into();
        self.media.push((src.clone(), arc_bytes));

        if let Some(object_id) = self.pending_replace_object.take() {
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
        } else {
            self.ui.placement_tool = Some(PlacementTool::Image {
                src: src.clone(),
                natural_size: (natural_width, natural_height),
            });
        }

        Some(src)
    }

    /// Maximum import size allowed for a media file (200 MiB).
    pub const MAX_MEDIA_IMPORT_BYTES: usize = 200 * 1024 * 1024;

    /// Import an audio file into the editor's media library and arm placement or replace selected object.
    pub fn import_audio(&mut self, file_name: &str, bytes: &[u8]) -> Option<String> {
        self.import_media(MediaKind::Audio, file_name, bytes)
    }

    /// Import a video file into the editor's media library and arm placement or replace selected object.
    pub fn import_video(&mut self, file_name: &str, bytes: &[u8]) -> Option<String> {
        self.import_media(MediaKind::Video, file_name, bytes)
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
        self.media.push((src.clone(), arc_bytes));
        Some(src)
    }

    fn import_media(&mut self, kind: MediaKind, file_name: &str, bytes: &[u8]) -> Option<String> {
        let label = match kind {
            MediaKind::Audio => "audio",
            MediaKind::Video => "video",
        };
        let src = self.store_media(kind, file_name, bytes)?;

        if let Some(object_id) = self.pending_replace_object.take() {
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
        self.media.push((src.clone(), arc_bytes));
        self.ui.placement_tool = Some(PlacementTool::Lottie {
            src: src.clone(),
            natural_size: (info.width as f32, info.height as f32),
        });
        Some(src)
    }

    /// Import a WebVTT caption file and attach it to the selected audio or video object.
    pub fn import_captions(&mut self, file_name: &str, bytes: &[u8]) -> Option<String> {
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
        self.media.push((path.clone(), arc_bytes));

        let Some(object_id) = self.pending_replace_object.take() else {
            return Some(path);
        };
        self.run(
            "object.set_captions",
            serde_json::json!({
                "slide": self.selection.slide,
                "object": object_id,
                "captions": path.clone(),
            }),
        );
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
        self.media.push((src.clone(), arc_bytes));
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
        self.run("course.remove_font", serde_json::json!({ "family": old }));
        for face in faces {
            if self
                .run(
                    "course.add_font",
                    serde_json::json!({
                        "src": face.src,
                        "family": new,
                        "style": face.style,
                    }),
                )
                .is_none()
            {
                return false;
            }
        }
        true
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

    /// Enter Master View, remembering the slide to return to.
    pub fn enter_master_view(&mut self) {
        if self.ui.master_view {
            return;
        }
        self.ui.master_view = true;
        // The Course map shows play slides: it cannot stay open here.
        self.ui.story_view = false;
        self.ui.master_view_return_slide = Some(self.selection.slide.clone());
        self.ui.inspector_tab = InspectorTab::Slide;
        self.ui.bottom_tab = BottomTab::Timeline;
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

    /// Open the in-editor course preview window starting at the given slide.
    ///
    /// Bank slides live outside play order, so starting a preview at one
    /// falls back to the course's own start slide.
    pub fn open_preview_at(&mut self, slide: Option<&str>) {
        let slide = slide.filter(|id| self.session.course.bank_slide(id).is_none());
        self.preview = Some(Preview::new(&self.session.course, slide));
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
        if crate::canvas::delete_selected_path_point(self) {
            return;
        }
        let ids = self.selected_object_ids();
        if ids.is_empty() {
            self.set_status("Select an object to delete");
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

    /// Dispatch `object.reorder` for each moved id at its index in `target`, as one step.
    fn run_reorder_steps(&mut self, slide_id: &str, target: &[String], moved: &[String]) {
        let steps: Vec<(String, Json)> = moved
            .iter()
            .filter_map(|id| {
                let index = target.iter().position(|candidate| candidate == id)?;
                Some((
                    "object.reorder".to_owned(),
                    serde_json::json!({
                        "slide": slide_id,
                        "object": id,
                        "index": index,
                    }),
                ))
            })
            .collect();
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

    /// Select every object on the focused layer.
    pub fn select_all_objects(&mut self) {
        let Some(slide) = self.session.course.slide(&self.selection.slide) else {
            return;
        };
        let ids: Vec<String> = match self.selection.layer.as_deref() {
            Some(lid) => slide
                .layer(lid)
                .map(|layer| layer.objects.iter().map(|o| o.id.clone()).collect())
                .unwrap_or_default(),
            None => slide.objects.iter().map(|o| o.id.clone()).collect(),
        };
        self.selection.set_many(ids);
    }

    /// Nudge every selected object by `(dx, dy)` as one undo step.
    pub fn nudge_selection(&mut self, dx: f32, dy: f32) {
        let Some(slide) = self.session.course.slide(&self.selection.slide) else {
            return;
        };
        let mut steps: Vec<(String, Json)> = Vec::new();
        for id in self.selection.ids() {
            let Some((object, _)) = slide.object(&id) else {
                continue;
            };
            let frame = coursecraft_course::Rect {
                x: object.frame.x + dx,
                y: object.frame.y + dy,
                w: object.frame.w,
                h: object.frame.h,
            };
            if !frame.x.is_finite() || !frame.y.is_finite() {
                continue;
            }
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
        if !steps.is_empty() {
            self.run_batch(&steps);
        }
    }

    /// Single place that calls `session.execute`.
    ///
    /// On `Err` it stores the error message in `ui.last_error` and returns `None`.
    /// Nothing else in the crate calls `execute`.
    pub fn run(&mut self, id: &str, params: Json) -> Option<Json> {
        match self.session.execute(id, &params) {
            Ok(v) => {
                self.select_created(id, &v);
                Some(v)
            }
            Err(e) => {
                let msg = e.to_string();
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
        match self.session.execute_batch(steps) {
            Ok(results) => {
                for (step, res) in steps.iter().zip(&results) {
                    self.select_created(&step.0, res);
                }
                Some(results)
            }
            Err(e) => {
                let msg = e.to_string();
                self.ui.last_error = Some((msg.clone(), self.ui.current_time));
                self.ui.error_history.push(msg);
                None
            }
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
            self.selection.repair(&self.session.course);
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
            self.refresh_validation();
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
    }

    /// Run validation and the accessibility audit now, through the engine.
    pub(crate) fn refresh_validation(&mut self) {
        if let Some(res) = self.run("course.validate", serde_json::json!({})) {
            let (errs, warns, issues) = parse_validation_result(&res);
            self.ui.validation_errors = errs;
            self.ui.validation_warnings = warns;
            self.ui.validation_issues = issues;
        }
        if let Some(res) = self.run("course.audit", serde_json::json!({})) {
            let (findings, manual_checks) = parse_audit_result(&res);
            self.ui.audit_findings = findings;
            self.ui.audit_manual_checks = manual_checks;
        }
    }

    /// Display the editor UI inside `ui` and return any shell requests.
    #[allow(clippy::cognitive_complexity)]
    pub fn show(&mut self, ui: &mut Ui) -> Vec<EditorRequest> {
        self.ui.current_time = ui.input(|i| i.time);
        self.check_revision();
        properties::publish_theme_colors(ui.ctx(), &self.session.course.theme);
        coursecraft_render::install_course_fonts(
            ui.ctx(),
            &self.session.course,
            &MemoryMediaSource { media: &self.media },
        );
        self.update_timeline_playback(ui);

        // Editor shortcuts must not act on the editor's selection while the user is
        // interacting with the preview window.
        let preview_has_focus = self.preview.as_ref().is_some_and(|p| p.has_focus);
        // Crop mode owns the keyboard: only Enter (apply) and Escape (cancel)
        // act, so nudges, deletes and undo cannot change the course under it.
        let text_focus = ui.ctx().egui_wants_keyboard_input()
            || ui.memory(|m| m.focused().is_some())
            || self.ui.inline_edit.is_some()
            || self.is_cropping()
            || self.ui.ai.open
            || self.ui.alt.open;

        // Copy, cut and paste arrive as events so they work in the browser too.
        // Leave them to a focused text field or the in-place editor.
        if !preview_has_focus && !text_focus {
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
            && let Some(sc) = menu::parse_shortcut("Ctrl+Shift+M")
            && !ui.ctx().egui_wants_keyboard_input()
            && ui.input_mut(|i| i.consume_shortcut(&sc))
        {
            self.ui.story_view = !self.ui.story_view;
        }

        // Handle keyboard shortcuts from registry. More specific shortcuts run
        // first so Cmd+Shift+G is not swallowed by Cmd+G.
        let mut shortcuts: Vec<(&coursecraft_engine::Command, &str)> = registry()
            .iter()
            // Crop mode owns the keyboard until Enter applies or Escape cancels,
            // and AI Settings owns it while the dialog is open.
            .filter(|_| {
                !preview_has_focus
                    && !self.is_cropping()
                    && !self.ui.ai.open
                    && !self.ui.document.open
                    && !self.ui.alt.open
                    && !self.ui.translate.open
            })
            .filter_map(|cmd| cmd.shortcut.map(|shortcut| (cmd, shortcut)))
            .collect();
        shortcuts.sort_by_key(|(_, shortcut)| std::cmp::Reverse(shortcut.matches('+').count()));
        for (cmd, sc_str) in shortcuts {
            if let Some(sc) = menu::parse_shortcut(sc_str) {
                let conflict = sc.conflicts_with_text_editing()
                    && (ui.ctx().egui_wants_keyboard_input()
                        || ui.memory(|m| m.focused().is_some()));
                if !conflict && ui.input_mut(|i| i.consume_shortcut(&sc)) {
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
                        } else {
                            self.delete_selection();
                        }
                    } else if let Some(params) = menu::adapter(self, cmd.id) {
                        self.run(cmd.id, params);
                    }
                }
            }
        }

        // Publish is a shell action, not an engine command, so it has its own binding.
        if !preview_has_focus
            && !self.ui.ai.open
            && !self.ui.alt.open
            && !self.ui.translate.open
            && self.file_ops_available
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
        {
            let del_pressed = ui.input_mut(|i| {
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
                } else {
                    self.delete_selection();
                }
            }

            if ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::A)) {
                self.select_all_objects();
            }

            let shift = ui.input(|i| i.modifiers.shift);
            let nudge_mods = if shift {
                Modifiers::SHIFT
            } else {
                Modifiers::NONE
            };
            let step = if shift { 10.0 } else { 1.0 };
            // In the Course map the arrows move the map selection instead.
            let left =
                !self.ui.story_view && ui.input_mut(|i| i.consume_key(nudge_mods, Key::ArrowLeft));
            let right =
                !self.ui.story_view && ui.input_mut(|i| i.consume_key(nudge_mods, Key::ArrowRight));
            let up =
                !self.ui.story_view && ui.input_mut(|i| i.consume_key(nudge_mods, Key::ArrowUp));
            let down =
                !self.ui.story_view && ui.input_mut(|i| i.consume_key(nudge_mods, Key::ArrowDown));
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
                self.ui.zoom = self.ui.zoom.zoom_in();
            } else if out_pressed {
                self.ui.zoom = self.ui.zoom.zoom_out();
            }

            let question_pressed = ui.input_mut(|i| {
                i.consume_key(Modifiers::SHIFT, Key::Slash)
                    || i.consume_key(Modifiers::NONE, Key::Questionmark)
            });
            if question_pressed {
                self.ui.show_shortcuts_dialog = !self.ui.show_shortcuts_dialog;
            }
        }

        // Auto-switch inspector tab on selection change
        if self.selection.object != self.ui.last_selected_object {
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

        let mut requests = Vec::new();
        let p = coursecraft_render::palette(ui.ctx());
        // Horizontal inner margin shared by the menu row, bottom panel and status bar, so their
        // content never touches the window edge (matches the side panels).
        let edge_margin = egui::Margin::symmetric(coursecraft_render::PANEL_MARGIN as i8, 0);

        if self.ui.show_welcome {
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

            requests.append(&mut self.pending_requests);
            return requests;
        }

        // 1. Top menu bar (panel fill, hairline below)
        Panel::top("editor_menu_bar")
            .exact_size(TOP_BAR_HEIGHT)
            .resizable(false)
            .frame(egui::Frame::NONE.fill(p.panel).inner_margin(edge_margin))
            .show_separator_line(true)
            .show(ui, |ui| {
                menu::show_menu_bar(self, ui, &mut requests);
            });

        // 1b. Keynote-style toolbar (52 px, panel fill, hairline below)
        let toolbar_panel = Panel::top("editor_toolbar")
            .exact_size(toolbar::TOOLBAR_HEIGHT)
            .resizable(false)
            .frame(egui::Frame::NONE.fill(p.panel).inner_margin(edge_margin))
            .show_separator_line(true)
            .show(ui, |ui| {
                toolbar::show_toolbar(self, ui, &mut requests);
            });
        // Dialogs open below the toolbar.
        dialog::set_top_inset(ui.ctx(), toolbar_panel.response.rect.max.y);

        // 2. Bottom status bar (26 px, panel fill, hairline above)
        Panel::bottom("editor_status_bar")
            .exact_size(STATUS_BAR_HEIGHT)
            .resizable(false)
            .frame(egui::Frame::NONE.fill(p.panel).inner_margin(edge_margin))
            .show_separator_line(true)
            .show(ui, |ui| {
                status::show_status_bar(self, ui);
            });

        // 3. Bottom timeline panel. Keep at least 200 px of canvas above it.
        let content_h = ui.ctx().content_rect().height();
        let chrome_h = TOP_BAR_HEIGHT + toolbar::TOOLBAR_HEIGHT + STATUS_BAR_HEIGHT;
        let max_by_canvas = (content_h - chrome_h - 200.0).max(timeline::TIMELINE_PANEL_MIN_HEIGHT);
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
                let mut actions = Vec::new();
                timeline::show_timeline_panel(self, ui, &mut actions);
                for (cmd, params) in actions {
                    self.run(cmd, params);
                }
            });
        ui.style_mut().interaction.resize_grab_radius_side = old_grab;
        self.ui.timeline.panel_rect = Some(timeline_panel.response.rect);
        self.ui.timeline.panel_height = timeline_panel.response.rect.height();

        // 4. Left slides panel
        Panel::left("editor_slides_panel")
            .default_size(220.0)
            .size_range(180.0..=320.0)
            .resizable(true)
            .frame(egui::Frame::NONE.fill(p.panel))
            .show_separator_line(true)
            .show(ui, |ui| {
                let panel_rect = ui.available_rect_before_wrap();
                slides::show_slides_panel(self, ui, panel_rect);
            });

        // 5. Right properties panel (Inspector: Format, Animate, Slide, Course)
        Panel::right("editor_properties_panel")
            .default_size(300.0)
            .size_range(260.0..=400.0)
            .resizable(true)
            .frame(egui::Frame::NONE.fill(p.panel))
            .show_separator_line(true)
            .show(ui, |ui| {
                // A child that doesn't report overflow: a wide row must not grow the
                // panel, which would push the rest of the window off screen.
                let rect = ui.available_rect_before_wrap();
                let mut content = ui.new_child(egui::UiBuilder::new().max_rect(rect));
                content.set_clip_rect(rect.intersect(ui.clip_rect()));
                properties::show_properties_panel(self, &mut content);
                ui.allocate_rect(rect, egui::Sense::hover());
            });

        // 6. Central canvas and layers strip, or the course map.
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(p.canvas))
            .show(ui, |ui| {
                if self.ui.story_view {
                    course_map::handle_map_keys(self, ui);
                    course_map::show_course_map(self, ui);
                } else {
                    canvas::show_canvas_and_layers(self, ui);
                }
            });

        // Insert menu's shape gallery (search needs a real text field).
        shape_gallery::show_shape_gallery_window(self, ui.ctx());

        // Insert ▸ Web object dialog.
        web_insert::show_web_insert_window(self, ui.ctx(), &mut requests);

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
        text_styles_ui::show_manage_styles_window(self, ui.ctx());
        slide_size_dialog::show_slide_size_dialog(self, ui.ctx());
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
                let source = MemoryMediaSource { media: &self.media };
                preview.show(ui.ctx(), &mut open, Some((&mut self.media_cache, &source)));
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

        // Media Library window
        media_library::show_media_library(self, ui.ctx());

        if let Some(text) = self.ui.pending_copy_text.take() {
            ui.ctx().copy_text(text);
        }

        self.check_revision();

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
}

impl coursecraft_render::MediaSource for MemoryMediaSource<'_> {
    fn get(&self, src: &str) -> coursecraft_render::MediaFetch {
        for (name, bytes) in self.media {
            if name == src {
                return coursecraft_render::MediaFetch::Ready(std::sync::Arc::clone(bytes));
            }
        }
        coursecraft_render::MediaFetch::Missing
    }
}

impl coursecraft_render::MediaSource for Editor {
    fn get(&self, src: &str) -> coursecraft_render::MediaFetch {
        for (name, bytes) in &self.media {
            if name == src {
                return coursecraft_render::MediaFetch::Ready(std::sync::Arc::clone(bytes));
            }
        }
        coursecraft_render::MediaFetch::Missing
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
}

/// Collect all media `src` paths referenced by objects and states across the
/// course: play slides, masters and templates. Used for keeping media on save.
#[must_use]
pub fn referenced_media_srcs(course: &Course) -> std::collections::HashSet<String> {
    let mut set = std::collections::HashSet::new();
    for face in &course.fonts {
        if !face.src.is_empty() {
            set.insert(face.src.clone());
        }
    }
    if let Some(logo) = &course.player.logo
        && !logo.is_empty()
    {
        set.insert(logo.clone());
    }
    for resource in &course.resources {
        if let coursecraft_course::ResourceTarget::File { src } = &resource.target
            && !src.is_empty()
        {
            set.insert(src.clone());
        }
    }
    for slide in course
        .slides()
        .chain(course.masters.iter())
        .chain(course.templates.iter())
    {
        for obj in &slide.objects {
            collect_object_media_srcs(obj, &mut set);
        }
        for layer in &slide.layers {
            for obj in &layer.objects {
                collect_object_media_srcs(obj, &mut set);
            }
        }
    }
    set
}

/// Prefixes (`web/<name>/`) of every folder-sourced web object, for keeping
/// the whole folder on save and out of the unused-media filter.
#[must_use]
pub fn web_folder_prefixes(course: &Course) -> Vec<String> {
    let mut prefixes = std::collections::BTreeSet::new();
    for slide in course
        .slides()
        .chain(course.masters.iter())
        .chain(course.templates.iter())
    {
        for obj in &slide.objects {
            if let coursecraft_course::ObjectKind::Web {
                source: coursecraft_course::WebSource::Folder { path, .. },
            } = &obj.kind
            {
                prefixes.insert(format!("{path}/"));
            }
        }
        for layer in &slide.layers {
            for obj in &layer.objects {
                if let coursecraft_course::ObjectKind::Web {
                    source: coursecraft_course::WebSource::Folder { path, .. },
                } = &obj.kind
                {
                    prefixes.insert(format!("{path}/"));
                }
            }
        }
    }
    prefixes.into_iter().collect()
}

fn collect_object_media_srcs(
    obj: &coursecraft_course::Object,
    set: &mut std::collections::HashSet<String>,
) {
    match &obj.kind {
        coursecraft_course::ObjectKind::Image { src, .. }
        | coursecraft_course::ObjectKind::Lottie { src, .. }
            if !src.is_empty() =>
        {
            set.insert(src.clone());
        }
        coursecraft_course::ObjectKind::Audio { src, captions, .. }
        | coursecraft_course::ObjectKind::Video { src, captions, .. } => {
            if !src.is_empty() {
                set.insert(src.clone());
            }
            if let Some(captions) = captions
                && !captions.is_empty()
            {
                set.insert(captions.clone());
            }
        }
        coursecraft_course::ObjectKind::Shape { fill_style, .. }
        | coursecraft_course::ObjectKind::Button { fill_style, .. } => {
            if let Some(src) = fill_style
                .as_ref()
                .and_then(coursecraft_course::FillStyle::picture_src)
                && !src.is_empty()
            {
                set.insert(src.to_owned());
            }
        }
        coursecraft_course::ObjectKind::Web {
            source: coursecraft_course::WebSource::Folder { path, entry },
        } => {
            set.insert(format!("{path}/{entry}"));
        }
        _ => {}
    }
    for state in &obj.states {
        if let Some(src) = &state.src
            && !src.is_empty()
        {
            set.insert(src.clone());
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
    match &object.kind {
        coursecraft_course::ObjectKind::Text { style, runs, .. } => text_uses(style, runs),
        coursecraft_course::ObjectKind::Shape { text, .. } => text
            .as_ref()
            .is_some_and(|text| text_uses(&text.style, &text.runs)),
        _ => false,
    }
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
        if !existing.iter().any(|(src, _)| src == &name) {
            return name;
        }
        counter = counter.saturating_add(1);
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
    if !existing.iter().any(|(s, _)| s == &base) {
        return base;
    }

    let mut counter = 2;
    loop {
        let candidate = format!("{stem}-{counter}.{ext}");
        if !existing.iter().any(|(s, _)| s == &candidate) {
            return candidate;
        }
        counter += 1;
    }
}

fn parse_validation_result(val: &Json) -> (usize, usize, Vec<ValidationIssueSummary>) {
    let mut errors = 0;
    let mut warnings = 0;
    let mut issues = Vec::new();

    if let Json::Array(items) = val {
        for item in items {
            let severity = item
                .get("severity")
                .and_then(|s| s.as_str())
                .unwrap_or("error");
            let path = item.get("path").and_then(|p| p.as_str()).unwrap_or("");
            let message = item.get("message").and_then(|m| m.as_str()).unwrap_or("");

            if severity == "error" {
                errors += 1;
            } else if severity == "warning" {
                warnings += 1;
            }

            issues.push(ValidationIssueSummary {
                severity: severity.to_owned(),
                path: path.to_owned(),
                message: message.to_owned(),
            });
        }
    }

    (errors, warnings, issues)
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
