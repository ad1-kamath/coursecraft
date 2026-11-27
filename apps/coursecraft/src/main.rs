//! `coursecraft`: the CourseCraft editor on the desktop (eframe/wgpu) or in the
//! browser (the same code compiled to wasm and served by trunk).

// No console window behind the app on Windows, in release builds.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod ai;

#[cfg(not(target_arch = "wasm32"))]
pub mod agents;
#[cfg(not(target_arch = "wasm32"))]
pub mod autosave;
#[cfg(not(target_arch = "wasm32"))]
pub mod crash;
#[cfg(not(target_arch = "wasm32"))]
pub mod export;
#[cfg(not(target_arch = "wasm32"))]
pub mod files;
#[cfg(not(target_arch = "wasm32"))]
pub mod paste_image;
pub mod player_bundle;
#[cfg(not(target_arch = "wasm32"))]
pub mod prefs;
#[cfg(not(target_arch = "wasm32"))]
pub mod preview_server;
pub mod publish;
#[cfg(not(target_arch = "wasm32"))]
pub mod recent_previews;
#[cfg(not(target_arch = "wasm32"))]
pub mod system_fonts;
#[cfg(not(target_arch = "wasm32"))]
pub mod template_library;
#[cfg(not(target_arch = "wasm32"))]
pub mod updates;
#[cfg(not(target_arch = "wasm32"))]
pub mod versions;
#[cfg(any(target_arch = "wasm32", test))]
pub mod web_state;

#[cfg(test)]
mod bughunt_m24_preview;
#[cfg(test)]
mod bughunt_m25_versions;
#[cfg(test)]
mod files_tests;
#[cfg(all(test, not(target_arch = "wasm32")))]
mod m27_files_ui_tests;
#[cfg(test)]
mod tests;
#[cfg(all(test, not(target_arch = "wasm32")))]
mod u16_shell_tests;
#[cfg(all(test, not(target_arch = "wasm32")))]
#[cfg(test)]
mod versions_tests;

#[cfg(not(target_arch = "wasm32"))]
use std::path::{Path, PathBuf};

use coursecraft_ai::{AiReply, ChatRequest};
use coursecraft_course::Course;
use coursecraft_engine::Session;
use coursecraft_ui_egui::{AiPurpose, Editor, EditorRequest};

#[cfg(not(target_arch = "wasm32"))]
use autosave::Autosaver;
#[cfg(not(target_arch = "wasm32"))]
use files::Project;
#[cfg(not(target_arch = "wasm32"))]
use prefs::Prefs;

/// The course a fresh launch opens behind the welcome screen: the Healthy Habits sample,
/// or a blank course with `--blank`.
#[cfg(test)]
fn initial_course(blank: bool) -> Course {
    initial_course_and_media(blank).0
}

/// [`initial_course`] with the media files the sample course uses.
fn initial_course_and_media(blank: bool) -> coursecraft_ui_egui::LoadedExample {
    let blank_course = || (Session::blank("Untitled course").course, Vec::new());
    if blank {
        return blank_course();
    }
    coursecraft_ui_egui::example_course_and_media(coursecraft_ui_egui::ExampleId::HealthyHabits)
        .unwrap_or_else(|_| blank_course())
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone, PartialEq, Eq)]
enum PendingAction {
    New,
    Open,
    /// Open a specific project file or folder (for example from drag-and-drop).
    OpenPath(PathBuf),
    /// Pick a project folder to open.
    OpenFolder,
    OpenRecent(usize),
    OpenExample(coursecraft_ui_egui::ExampleId),
    OpenTemplate(coursecraft_ui_egui::TemplateId),
    Quit,
    CloseRequested,
    ImportPptx,
    /// Open this kept version as a new, unsaved copy.
    OpenVersion(PathBuf),
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone)]
struct RestorePrompt {
    /// Project file the autosave belongs to, or `None` for an untitled project.
    file_path: Option<PathBuf>,
    autosave_path: PathBuf,
    time_str: String,
}

/// Shown when the project file changed on disk after it was opened or saved.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone)]
struct ConflictPrompt {
    /// What the person was doing when Save was pressed, resumed after the choice.
    then: Option<PendingAction>,
    /// For a folder project, the files changed outside the app, by their path
    /// in the folder. Empty for a `.ccraft`.
    files: Vec<String>,
}

/// Result sent back from the background autosave thread.
#[cfg(not(target_arch = "wasm32"))]
struct AutosaveOutcome {
    revision: u64,
    epoch: u64,
    result: Result<bool, String>,
}

struct EditorApp {
    editor: Editor,
    /// One background AI call at a time, on both desktop and web.
    ai: ai::AiClient,
    /// One background agent check or action at a time, on the desktop.
    #[cfg(not(target_arch = "wasm32"))]
    agents: agents::AgentClient,
    /// Thumbnails of the recent courses, read in the background.
    #[cfg(not(target_arch = "wasm32"))]
    recent_previews: recent_previews::RecentPreviews,
    #[cfg(not(target_arch = "wasm32"))]
    project: Project,
    #[cfg(not(target_arch = "wasm32"))]
    prefs: Prefs,
    #[cfg(not(target_arch = "wasm32"))]
    config_dir: Option<PathBuf>,
    #[cfg(not(target_arch = "wasm32"))]
    autosaver: Autosaver,
    /// A background autosave receiver while one is running.
    #[cfg(not(target_arch = "wasm32"))]
    autosave_rx: Option<std::sync::mpsc::Receiver<AutosaveOutcome>>,
    /// Monotonically incremented on manual Save, Save As, Save Folder, Open, New,
    /// and autosaver resets so an in-flight background autosave never commits
    /// after a newer explicit user action.
    #[cfg(not(target_arch = "wasm32"))]
    save_epoch: std::sync::Arc<std::sync::atomic::AtomicU64>,
    #[cfg(not(target_arch = "wasm32"))]
    pending_action: Option<PendingAction>,
    #[cfg(not(target_arch = "wasm32"))]
    restore_prompt: Option<RestorePrompt>,
    #[cfg(not(target_arch = "wasm32"))]
    error_modal: Option<String>,
    #[cfg(not(target_arch = "wasm32"))]
    conflict_prompt: Option<ConflictPrompt>,
    /// The newest readable version of a project that would not open, offered
    /// by the error dialog.
    #[cfg(not(target_arch = "wasm32"))]
    open_latest_offer: Option<PathBuf>,
    /// Set once the user confirmed leaving, so the next close request is not intercepted.
    #[cfg(not(target_arch = "wasm32"))]
    allow_close: bool,
    /// Last window title sent to the viewport.
    #[cfg(not(target_arch = "wasm32"))]
    last_title: Option<String>,
    #[cfg(not(target_arch = "wasm32"))]
    control_server: Option<coursecraft_control::ControlServer>,
    /// A background publish receiver while one is running.
    #[cfg(not(target_arch = "wasm32"))]
    publish_rx: Option<std::sync::mpsc::Receiver<Result<publish::PublishOutcome, String>>>,
    /// A background MP4 export receiver while one is running.
    #[cfg(not(target_arch = "wasm32"))]
    export_rx: Option<std::sync::mpsc::Receiver<export::ExportUpdate>>,
    /// A background PDF or Word review export while one is running.
    #[cfg(not(target_arch = "wasm32"))]
    export_review_rx: Option<std::sync::mpsc::Receiver<Result<PathBuf, String>>>,
    /// A background scan of the computer's installed fonts, while one is running.
    #[cfg(not(target_arch = "wasm32"))]
    system_fonts_rx: Option<
        std::sync::mpsc::Receiver<
            std::sync::Arc<coursecraft_ui_egui::coursecraft_media::system_fonts::SystemFontIndex>,
        >,
    >,
    /// The cached installed font index found by the background scan.
    #[cfg(not(target_arch = "wasm32"))]
    system_font_index: Option<
        std::sync::Arc<coursecraft_ui_egui::coursecraft_media::system_fonts::SystemFontIndex>,
    >,
    /// Last file written by a successful publish, for "Show in folder".
    #[cfg(not(target_arch = "wasm32"))]
    published_path: Option<PathBuf>,
    /// Local HTTP server for "Open in browser" after a publish.
    #[cfg(not(target_arch = "wasm32"))]
    preview_server: Option<preview_server::PreviewServer>,
    /// The Craft suite's update check, prompt and self-update (`craft_update`).
    #[cfg(not(target_arch = "wasm32"))]
    updates: updates::Runtime,
    /// A file returned by the export save dialog in tests instead of opening a dialog.
    #[cfg(all(test, not(target_arch = "wasm32")))]
    test_export_path: Option<PathBuf>,
    /// A file returned by the audio picker in tests instead of opening a dialog.
    #[cfg(all(test, not(target_arch = "wasm32")))]
    test_picked_audio: Option<PathBuf>,
    /// Files returned by the media-library import picker in tests.
    #[cfg(all(test, not(target_arch = "wasm32")))]
    test_picked_media_files: Option<Vec<PathBuf>>,
    /// A file returned by the media-library replace picker in tests.
    #[cfg(all(test, not(target_arch = "wasm32")))]
    test_picked_media_file: Option<PathBuf>,
    /// A file returned by Save As in tests instead of opening a native dialog.
    #[cfg(all(test, not(target_arch = "wasm32")))]
    test_save_as_path: Option<PathBuf>,
    /// A folder returned by Save As Folder in tests instead of opening a native dialog.
    #[cfg(all(test, not(target_arch = "wasm32")))]
    test_save_as_folder_path: Option<PathBuf>,
    /// A file returned by Open in tests instead of opening a native dialog.
    #[cfg(all(test, not(target_arch = "wasm32")))]
    test_open_path: Option<PathBuf>,
    /// A folder returned by Open Folder in tests instead of opening a native dialog.
    #[cfg(all(test, not(target_arch = "wasm32")))]
    test_open_folder_path: Option<PathBuf>,
    /// The egui context control requests repaint, so an idle editor still answers them.
    #[cfg(not(target_arch = "wasm32"))]
    repaint_ctx: std::sync::Arc<std::sync::Mutex<Option<egui::Context>>>,
    #[cfg(target_arch = "wasm32")]
    import_tx: std::sync::mpsc::Sender<(String, Vec<u8>)>,
    #[cfg(target_arch = "wasm32")]
    import_rx: std::sync::mpsc::Receiver<(String, Vec<u8>)>,
    /// Chosen background audio bytes in the browser.
    #[cfg(target_arch = "wasm32")]
    background_audio_tx: std::sync::mpsc::Sender<(String, Vec<u8>)>,
    #[cfg(target_arch = "wasm32")]
    background_audio_rx: std::sync::mpsc::Receiver<(String, Vec<u8>)>,
    /// Chosen `.docx`, `.txt` or `.md` bytes in the browser.
    #[cfg(target_arch = "wasm32")]
    document_tx: std::sync::mpsc::Sender<(String, Vec<u8>)>,
    #[cfg(target_arch = "wasm32")]
    document_rx: std::sync::mpsc::Receiver<(String, Vec<u8>)>,
    /// Chosen `.ccraft` or `course.json` bytes in the browser.
    #[cfg(target_arch = "wasm32")]
    project_open_tx: std::sync::mpsc::Sender<(String, Vec<u8>)>,
    #[cfg(target_arch = "wasm32")]
    project_open_rx: std::sync::mpsc::Receiver<(String, Vec<u8>)>,
    /// Unsaved-changes guard and pending action for the web editor.
    #[cfg(target_arch = "wasm32")]
    web_unsaved: web_state::WebUnsavedState,
    /// Active frame-chunked web publish job.
    #[cfg(target_arch = "wasm32")]
    web_publish: Option<WebPublishJob>,
}

#[cfg(target_arch = "wasm32")]
enum WebPublishJob {
    Preparing {
        standard: coursecraft_ui_egui::PublishStandard,
    },
    Zipping {
        standard: coursecraft_ui_egui::PublishStandard,
        zipper: coursecraft_publish::PackageZipper,
    },
}

/// Whether a close request must be intercepted to ask about unsaved changes.
#[cfg(not(target_arch = "wasm32"))]
fn should_intercept_close(close_requested: bool, dirty: bool, allow_close: bool) -> bool {
    close_requested && dirty && !allow_close
}

/// Window title for the given project name and dirty state.
#[cfg(not(target_arch = "wasm32"))]
fn window_title(name: &str, dirty: bool) -> String {
    let marker = if dirty { " •" } else { "" };
    format!("CourseCraft — {name}{marker}")
}

/// Record `title` as the last sent title; returns true if it changed and must be sent.
#[cfg(not(target_arch = "wasm32"))]
fn title_changed(last: &mut Option<String>, title: &str) -> bool {
    if last.as_deref() == Some(title) {
        return false;
    }
    *last = Some(title.to_owned());
    true
}

/// Turn a chosen document file's bytes into text for Course from Document.
///
/// A `.docx` is read by `coursecraft-pptx`; a `.txt` or `.md` must be UTF-8.
fn document_text_from_bytes(name: &str, bytes: &[u8]) -> Result<String, String> {
    if name.to_ascii_lowercase().ends_with(".docx") {
        return coursecraft_pptx::docx_to_text(bytes)
            .map_err(|error| format!("Couldn't read the Word document: {error}"));
    }
    let text =
        String::from_utf8(bytes.to_vec()).map_err(|_| "The file isn't UTF-8 text.".to_string())?;
    Ok(text.strip_prefix('\u{feff}').unwrap_or(&text).to_string())
}

/// How many changed files the conflict dialog names before it says "and N more".
const CONFLICT_FILES_SHOWN: usize = 8;

/// The sentence in the conflict dialog for a folder project, naming the files
/// that changed outside the app.
fn changed_files_message(name: &str, files: &[String]) -> String {
    let mut shown: Vec<&str> = files
        .iter()
        .take(CONFLICT_FILES_SHOWN)
        .map(String::as_str)
        .collect();
    let more = files.len().saturating_sub(shown.len());
    let mut list = shown.join(", ");
    if more > 0 {
        list.push_str(&format!(" and {more} more"));
    }
    shown.clear();
    format!(
        "These files in {name} were changed by another app or computer: {list}. Overwrite them with what you have here, or keep both by saving a copy."
    )
}

/// Restore prompt for `path` when a newer autosave exists next to it.
#[cfg(not(target_arch = "wasm32"))]
fn restore_prompt_for_path(path: &Path) -> Option<RestorePrompt> {
    let auto_path = autosave::autosave_path_for_project(Some(path), Path::new(""));
    if auto_path.exists() && autosave::is_autosave_newer(path, &auto_path) {
        Some(RestorePrompt {
            file_path: Some(path.to_path_buf()),
            time_str: autosave_time_str(&auto_path),
            autosave_path: auto_path,
        })
    } else {
        None
    }
}

/// Restore prompt for the newest untitled autosave in `config_dir` whose
/// owning process is no longer running, if any. Another window's live autosave
/// is never offered. `is_alive` tells whether a process id is running.
#[cfg(not(target_arch = "wasm32"))]
fn untitled_restore_prompt(
    config_dir: Option<&Path>,
    is_alive: &dyn Fn(u32) -> bool,
) -> Option<RestorePrompt> {
    let auto_path = autosave::orphaned_untitled_autosaves(config_dir?, is_alive)
        .into_iter()
        .next()?;
    Some(RestorePrompt {
        file_path: None,
        time_str: autosave_time_str(&auto_path),
        autosave_path: auto_path,
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn autosave_time_str(auto_path: &Path) -> String {
    std::fs::metadata(auto_path)
        .and_then(|m| m.modified())
        .map(autosave::format_system_time)
        .unwrap_or_else(|_| "recent session".to_string())
}

/// Wakes the editor's UI thread when a control request arrives.
#[cfg(not(target_arch = "wasm32"))]
fn control_waker(
    slot: &std::sync::Arc<std::sync::Mutex<Option<egui::Context>>>,
) -> coursecraft_control::Waker {
    let slot = std::sync::Arc::clone(slot);
    std::sync::Arc::new(move || {
        if let Ok(guard) = slot.lock()
            && let Some(ctx) = guard.as_ref()
        {
            ctx.request_repaint();
        }
    })
}

#[cfg(not(target_arch = "wasm32"))]
impl EditorApp {
    fn new_desktop(course: Course, config_dir: Option<PathBuf>) -> Self {
        let prefs = config_dir
            .as_ref()
            .map(|d| prefs::load_prefs(d))
            .unwrap_or_default();

        let mut editor = Editor::new(course);
        editor.set_recent(prefs.recent.clone());
        if let Some(dir) = config_dir.as_deref() {
            editor.set_library_templates_available(true);
            editor.set_library_templates(
                template_library::list_templates(dir)
                    .into_iter()
                    .map(|t| coursecraft_ui_egui::LibraryTemplateEntry {
                        id: t.id,
                        title: t.title,
                    })
                    .collect(),
            );
        }
        editor.set_publish_available(player_bundle::has_player());
        editor.set_automation_available(true);
        editor.set_updates_available(true);
        editor.set_auto_check_updates(prefs.auto_check_updates);
        editor.set_agent_exe_path(
            std::env::current_exe()
                .ok()
                .map(|path| path.display().to_string()),
        );
        editor.set_automation_enabled(prefs.automation_enabled);
        editor.set_theme_pref(prefs.theme.into());
        editor.set_language(&prefs.language);
        editor.set_layout_prefs(&prefs.layout());
        editor.set_coach_marks_done(prefs.coach_marks_done);
        editor.set_preferences(prefs.autosave_minutes_clamped(), prefs.default_slide.size());
        editor.set_versions_to_keep(prefs.versions_to_keep_clamped());
        editor.set_spell_preferences(prefs.spell_check_as_you_type, prefs.spell_words.clone());
        editor.set_performance_mode(prefs.performance_mode.into());
        editor.set_ai_config(prefs.ai.as_ref().map(prefs::AiPrefs::to_config));
        editor.set_ai_source(
            prefs
                .ai
                .as_ref()
                .map_or(coursecraft_ai::AiSource::Endpoint, |ai| ai.source),
        );
        editor.set_elevenlabs_config(
            prefs
                .elevenlabs
                .as_ref()
                .map(prefs::ElevenLabsPrefs::to_config),
        );
        editor.set_speech_source(prefs.speech_source.into());
        editor.ui.show_welcome = true;
        editor.ui.has_loaded_course = false;

        let repaint_ctx = std::sync::Arc::new(std::sync::Mutex::new(None));
        let control_server = if prefs.automation_enabled {
            match coursecraft_control::ControlServer::start_with_waker(
                config_dir.as_deref(),
                Some(control_waker(&repaint_ctx)),
            ) {
                Ok(s) => {
                    editor.set_automation_port(Some(s.port()));
                    Some(s)
                }
                Err(e) => {
                    eprintln!("Warning: failed to start control server: {e}");
                    editor.set_automation_port(None);
                    None
                }
            }
        } else {
            None
        };

        let project = Project::new(editor.session.revision);
        let autosave_minutes = prefs.autosave_minutes_clamped();

        let (fonts_tx, fonts_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let index = std::sync::Arc::new(
                coursecraft_ui_egui::coursecraft_media::system_fonts::SystemFontIndex::scan_default(
                ),
            );
            let _ = fonts_tx.send(index);
        });

        Self {
            editor,
            ai: ai::AiClient::new(),
            agents: agents::AgentClient::new(),
            #[cfg(not(target_arch = "wasm32"))]
            recent_previews: recent_previews::RecentPreviews::default(),
            project,
            prefs,
            config_dir,
            autosaver: Autosaver {
                interval_minutes: Some(autosave_minutes),
                ..Autosaver::new()
            },
            autosave_rx: None,
            save_epoch: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            pending_action: None,
            restore_prompt: None,
            error_modal: None,
            conflict_prompt: None,
            open_latest_offer: None,
            allow_close: false,
            last_title: None,
            control_server,
            publish_rx: None,
            export_rx: None,
            export_review_rx: None,
            system_fonts_rx: Some(fonts_rx),
            system_font_index: None,
            published_path: None,
            preview_server: None,
            updates: updates::Runtime::default(),
            #[cfg(test)]
            test_export_path: None,
            #[cfg(test)]
            test_picked_audio: None,
            #[cfg(test)]
            test_picked_media_files: None,
            #[cfg(test)]
            test_picked_media_file: None,
            #[cfg(test)]
            test_save_as_path: None,
            #[cfg(test)]
            test_save_as_folder_path: None,
            #[cfg(test)]
            test_open_path: None,
            #[cfg(test)]
            test_open_folder_path: None,
            repaint_ctx,
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn pick_audio_path(&mut self) -> Option<PathBuf> {
        #[cfg(test)]
        if let Some(path) = self.test_picked_audio.take() {
            return Some(path);
        }
        files::pick_audio_file()
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn pick_audio(&mut self, background: bool) {
        if let Some(path) = self.pick_audio_path() {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("audio.mp3")
                .to_string();
            if let Ok(bytes) = std::fs::read(&path) {
                if background {
                    self.editor.import_background_audio(&name, &bytes);
                } else {
                    self.editor.import_audio(&name, &bytes);
                }
            }
        }
    }

    /// Copy any file into the project and point resource `resource` at it.
    #[cfg(not(target_arch = "wasm32"))]
    fn pick_resource_file(&mut self, resource: &str) {
        #[cfg(test)]
        let picked = self.test_picked_media_file.take();
        #[cfg(not(test))]
        let picked = rfd::FileDialog::new().pick_file();
        let Some(path) = picked else {
            return;
        };
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("document")
            .to_string();
        let Ok(bytes) = std::fs::read(&path) else {
            self.editor
                .set_status(format!("Couldn't read {name} for the resource."));
            return;
        };
        if let Some(src) = self.editor.add_media_bytes(&name, &bytes) {
            self.editor.run(
                "resource.update",
                serde_json::json!({ "id": resource, "target": { "kind": "file", "src": src } }),
            );
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn pick_media_files(&mut self) -> Vec<PathBuf> {
        #[cfg(test)]
        if let Some(paths) = self.test_picked_media_files.take() {
            return paths;
        }
        rfd::FileDialog::new()
            .add_filter(
                "Media (*.png, *.jpg, *.jpeg, *.webp, *.gif, *.svg, *.emf, *.wmf, *.mp3, *.wav, *.ogg, *.m4a, *.mp4, *.webm, *.json, *.lottie, *.vtt, *.ttf, *.otf)",
                &[
                    "png", "jpg", "jpeg", "webp", "gif", "svg", "emf", "wmf", "mp3", "wav", "ogg",
                    "m4a", "mp4", "webm", "json", "lottie", "vtt", "ttf", "otf",
                ],
            )
            .pick_files()
            .unwrap_or_default()
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn pick_media_file(&mut self) -> Option<PathBuf> {
        #[cfg(test)]
        if let Some(path) = self.test_picked_media_file.take() {
            return Some(path);
        }
        rfd::FileDialog::new()
            .add_filter(
                "Media (*.png, *.jpg, *.jpeg, *.webp, *.gif, *.svg, *.emf, *.wmf, *.mp3, *.wav, *.ogg, *.m4a, *.mp4, *.webm, *.json, *.lottie, *.vtt, *.ttf, *.otf)",
                &[
                    "png", "jpg", "jpeg", "webp", "gif", "svg", "emf", "wmf", "mp3", "wav", "ogg",
                    "m4a", "mp4", "webm", "json", "lottie", "vtt", "ttf", "otf",
                ],
            )
            .pick_file()
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn import_media_files(&mut self) {
        let mut added = 0usize;
        let mut failed = 0usize;
        let mut failures = Vec::new();
        for path in self.pick_media_files() {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("media")
                .to_string();
            match std::fs::read(&path) {
                Ok(bytes) => {
                    if self.editor.add_media_bytes(&name, &bytes).is_some() {
                        added = added.saturating_add(1);
                    }
                }
                Err(error) => {
                    failed += 1;
                    if failures.len() < 3 {
                        failures.push(format!("{name} ({error})"));
                    }
                }
            }
        }
        let mut status = String::new();
        if added > 0 {
            status = format!(
                "Imported {added} {}. ",
                if added == 1 { "file" } else { "files" }
            );
        }
        if failed > 0 {
            status.push_str(&format!("Couldn't read {failed}: {}", failures.join(", ")));
            if failed > failures.len() {
                status.push_str(&format!(" and {} more", failed - failures.len()));
            }
            status.push('.');
            if added > 0 {
                status.push(' ');
            }
        }
        if added > 0 {
            status.push_str("File operations are not undoable.");
        }
        if !status.is_empty() {
            self.editor.set_status(status);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn replace_media_file(&mut self, src: &str) {
        let Some(path) = self.pick_media_file() else {
            return;
        };
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("media")
            .to_string();
        match std::fs::read(&path) {
            Ok(bytes) => {
                if self.editor.replace_media_bytes(src, &bytes) {
                    self.editor.set_status(format!(
                        "Replaced {src} with {name}. File operations are not undoable."
                    ));
                }
            }
            Err(error) => self
                .editor
                .set_status(format!("Couldn't read {name} to replace {src}: {error}")),
        }
    }

    /// Startup flow: open the file given on the command line (through the normal
    /// open path), or offer to recover a leftover untitled autosave, or stay on welcome screen.
    fn startup(&mut self, initial_path: Option<PathBuf>, blank: bool) {
        if let Some(note) = self.config_dir.as_deref().and_then(crash::read_crash_note) {
            self.editor.set_crash_note(Some(note));
        }
        if !cfg!(test) && !updates::suppressed_by_env() {
            self.updates.set_enabled(self.prefs.auto_check_updates);
        }
        if blank {
            self.editor.ui.show_welcome = false;
            self.editor.ui.has_loaded_course = true;
            return;
        }
        match initial_path {
            Some(path) => {
                self.initiate_open(path);
                self.editor.ui.show_welcome = false;
                self.editor.ui.has_loaded_course = true;
            }
            None => {
                self.restore_prompt = untitled_restore_prompt(
                    self.config_dir.as_deref(),
                    &autosave::process_is_alive,
                );
                if self.restore_prompt.is_some() {
                    self.editor.ui.show_welcome = false;
                }
            }
        }
    }

    fn perform_save(&mut self) -> bool {
        self.save_project_file(false)
    }

    /// Save to the project's path. Unless `overwrite`, a file that changed on
    /// disk since it was opened or saved is not touched: the person is asked
    /// first, and the work Save was part of (leaving, opening) waits for the answer.
    fn save_project_file(&mut self, overwrite: bool) -> bool {
        let Some(path) = self.project.path.clone() else {
            return self.perform_save_as();
        };
        if !overwrite && self.project.changed_on_disk() {
            self.conflict_prompt = Some(ConflictPrompt {
                then: self.pending_action.take(),
                files: self.project.changed_files(),
            });
            return false;
        }

        let media = self.editor.used_media();
        let kind = save_kind(&path, self.project.folder.is_some(), !media.is_empty());
        if kind == SaveKind::JsonNeedsSaveAs {
            self.editor.set_status(
                "Plain JSON files cannot store imported media; save as a .ccraft project instead.",
            );
            return self.perform_save_as();
        }
        self.bump_save_epoch();
        let saved = match kind {
            SaveKind::Folder => {
                // A folder is kept in git, so there is no version history for it.
                let mut record = self.project.folder.take().unwrap_or_default();
                let result = files::save_folder_to_arc(
                    &path,
                    &self.editor.session.course,
                    &media,
                    &mut record,
                );
                self.project.folder = Some(record);
                result.map(|_| ())
            }
            SaveKind::JsonNeedsSaveAs => return self.perform_save_as(),
            SaveKind::Json => {
                self.keep_version(&path);
                self.editor
                    .session
                    .course
                    .to_json()
                    .map_err(|e| format!("Save failed: {e}"))
                    .and_then(|json| {
                        files::write_atomic(&path, format!("{json}\n").as_bytes())
                            .map_err(|e| format!("Failed to write file: {e}"))
                    })
            }
            SaveKind::Ccraft => {
                self.keep_version(&path);
                files::save_to_arc(&path, &self.editor.session.course, &media)
            }
        };
        match saved {
            Ok(()) => {
                self.project.saved_revision = self.editor.session.revision;
                if !self.project.is_folder() {
                    self.project.record_disk_stamp();
                }
                let auto_path = autosave::autosave_path_for_project(Some(&path), Path::new(""));
                autosave::delete_autosave(&auto_path);
                if let Some(ref dir) = self.config_dir {
                    let untitled_path = autosave::autosave_path_for_project(None, dir);
                    autosave::delete_untitled_autosave(&untitled_path);
                    self.prefs.add_recent(&path);
                    let _ = prefs::save_prefs(dir, &self.prefs);
                }
                self.editor.set_recent(self.prefs.recent.clone());
                true
            }
            Err(err) => {
                self.error_modal = Some(err);
                false
            }
        }
    }

    /// Copy the file about to be replaced into the version history. A failure
    /// is logged and shown in the status bar but never stops the save.
    fn keep_version(&mut self, path: &Path) {
        let Some(dir) = self.config_dir.clone() else {
            return;
        };
        let keep = self.prefs.versions_to_keep_clamped();
        let title = self.editor.session.course.title.clone();
        if let Err(e) = versions::snapshot_before_save(&dir, path, keep, &title) {
            eprintln!("Couldn't keep a version of {}: {e}", path.display());
            self.editor
                .set_status("Couldn't keep a version of this file");
        }
    }

    /// The rows for the Version History window.
    fn version_rows(&self) -> Vec<coursecraft_ui_egui::version_history::VersionRow> {
        let (Some(dir), Some(path)) = (self.config_dir.as_deref(), self.project.path.as_deref())
        else {
            return Vec::new();
        };
        versions::list_versions(dir, path)
            .into_iter()
            .map(|v| coursecraft_ui_egui::version_history::VersionRow {
                when: v.when(),
                size: versions::format_size(v.size),
                slides: v.slides,
                id: v.id,
            })
            .collect()
    }

    /// Open a kept version as a new, unsaved copy named "<title> (restored <date>)".
    /// The original project file is never touched; saving asks for a new name.
    fn open_version_copy(&mut self, version: &Path) {
        match files::open_from(version) {
            Ok((mut course, media)) => {
                let stem = version
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default();
                course.title = format!(
                    "{} (restored {})",
                    course.title,
                    versions::display_when(stem)
                );
                let media_arc = into_arc_media(media);
                self.editor.replace_course(course);
                self.editor.set_media(media_arc);
                self.project = Project::new(self.editor.session.revision);
                // Unsaved until the person saves it under a name.
                self.project.saved_revision = u64::MAX;
                self.reset_autosaver();
            }
            Err(err) => self.error_modal = Some(err),
        }
    }

    fn bump_save_epoch(&self) {
        self.save_epoch
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
    }

    fn reset_autosaver(&mut self) {
        self.bump_save_epoch();
        self.autosaver = Autosaver::new();
        self.autosaver.interval_minutes = Some(self.prefs.autosave_minutes_clamped());
    }

    fn pick_save_file_path(&mut self, title: &str) -> Option<PathBuf> {
        #[cfg(test)]
        if let Some(path) = self.test_save_as_path.take() {
            return Some(path);
        }
        files::pick_save_file(title)
    }

    fn pick_save_folder_path(&mut self, title: &str) -> Option<PathBuf> {
        #[cfg(test)]
        if let Some(path) = self.test_save_as_folder_path.take() {
            return Some(path);
        }
        files::pick_save_folder(title)
    }

    fn pick_open_file_path(&mut self) -> Option<PathBuf> {
        #[cfg(test)]
        if let Some(path) = self.test_open_path.take() {
            return Some(path);
        }
        files::pick_open_file()
    }

    fn pick_open_folder_path(&mut self) -> Option<PathBuf> {
        #[cfg(test)]
        if let Some(path) = self.test_open_folder_path.take() {
            return Some(path);
        }
        files::pick_project_folder()
    }

    /// Open version `id` as a copy, after the unsaved-changes prompt if needed.
    fn request_open_version(&mut self, id: &str, ctx: &egui::Context) {
        let path = self
            .config_dir
            .as_deref()
            .zip(self.project.path.as_deref())
            .and_then(|(dir, project)| versions::version_path(dir, project, id));
        let Some(path) = path else {
            self.editor.set_status("That version no longer exists.");
            return;
        };
        if self.project.is_dirty(&self.editor.session) {
            self.pending_action = Some(PendingAction::OpenVersion(path));
        } else {
            self.execute_pending_action(PendingAction::OpenVersion(path), ctx);
        }
    }

    /// Delete version `id` and refresh the Version History rows.
    fn request_delete_version(&mut self, id: &str) {
        let result = self
            .config_dir
            .as_deref()
            .zip(self.project.path.as_deref())
            .map_or(Ok(()), |(dir, project)| {
                versions::delete_version(dir, project, id)
            });
        if let Err(e) = result {
            self.editor.set_status(e);
        }
        let rows = self.version_rows();
        self.editor.set_version_rows(rows);
    }

    /// Save As a project folder (for git): a new folder named by the person.
    /// It must be new, empty, or an earlier project folder the dialog already
    /// confirmed replacing, so nothing else is ever written over.
    fn perform_save_as_folder(&mut self) -> bool {
        let title = self.editor.session.course.title.clone();
        let Some(path) = self.pick_save_folder_path(&title) else {
            return false;
        };
        let occupied = std::fs::read_dir(&path).is_ok_and(|mut e| e.next().is_some());
        if (path.exists() && !path.is_dir())
            || (occupied && !coursecraft_format::is_project_folder(&path))
        {
            self.error_modal = Some(format!(
                "{} already exists and is not an empty folder or a CourseCraft project folder. \
                 Choose a new name.",
                path.display()
            ));
            return false;
        }
        let prev_path = self.project.path.clone();
        let prev_disk_stamp = self.project.disk_stamp;
        let prev_folder = self.project.folder.clone();
        self.project.path = Some(path);
        self.project.disk_stamp = None;
        self.project.folder = Some(coursecraft_format::FolderRecord::new());
        if self.perform_save() {
            true
        } else {
            self.project.path = prev_path;
            self.project.disk_stamp = prev_disk_stamp;
            self.project.folder = prev_folder;
            false
        }
    }

    fn perform_save_as(&mut self) -> bool {
        let title = self.editor.session.course.title.clone();
        let Some(path) = self.pick_save_file_path(&title) else {
            return false;
        };
        let prev_path = self.project.path.clone();
        let prev_disk_stamp = self.project.disk_stamp;
        let prev_folder = self.project.folder.clone();
        self.project.folder = None;
        self.project.path = Some(path);
        // A new path has no earlier stamp; the file dialog already confirmed
        // replacing a file that is there.
        self.project.disk_stamp = None;
        if self.perform_save() {
            true
        } else {
            self.project.path = prev_path;
            self.project.disk_stamp = prev_disk_stamp;
            self.project.folder = prev_folder;
            false
        }
    }

    /// Open the autosave of `prompt`. The autosave of another, ended untitled
    /// session then moves to this session's own file, so it is offered once only.
    fn restore_autosave(&mut self, prompt: &RestorePrompt) {
        let had_error = self.error_modal.is_some();
        self.finish_open(&prompt.autosave_path, prompt.file_path.clone(), true);
        if prompt.file_path.is_none()
            && !had_error
            && self.error_modal.is_none()
            && let Some(dir) = self.config_dir.as_deref()
        {
            let own = autosave::autosave_path_for_project(None, dir);
            if own != prompt.autosave_path && std::fs::rename(&prompt.autosave_path, &own).is_ok() {
                autosave::write_owner_marker(&own);
                autosave::delete_untitled_autosave(&prompt.autosave_path);
            }
        }
    }

    fn finish_open(&mut self, path_to_read: &Path, final_path: Option<PathBuf>, is_restored: bool) {
        let opened = if path_to_read.is_dir() {
            files::open_folder_from(path_to_read).map(|(loaded, record)| (loaded, Some(record)))
        } else {
            files::open_from(path_to_read).map(|loaded| (loaded, None))
        };
        match opened {
            Ok(((course, media), record)) => {
                let media_arc = into_arc_media(media);
                self.editor.replace_course(course);
                self.editor.set_media(media_arc);
                self.project.path = final_path.clone();
                // A restored autosave of a folder project has no record of the
                // folder yet, so its first save writes every file.
                self.project.folder = match (record, final_path.as_deref()) {
                    (Some(record), _) => Some(record),
                    (None, Some(path)) if path.is_dir() => {
                        Some(coursecraft_format::FolderRecord::new())
                    }
                    _ => None,
                };
                self.project.record_disk_stamp();
                if is_restored {
                    self.project.saved_revision = u64::MAX;
                } else {
                    self.project.saved_revision = self.editor.session.revision;
                }
                if let Some(ref path) = final_path {
                    self.prefs.add_recent(path);
                    if let Some(ref dir) = self.config_dir {
                        let _ = prefs::save_prefs(dir, &self.prefs);
                    }
                    self.editor.set_recent(self.prefs.recent.clone());
                }
                self.reset_autosaver();
            }
            Err(err) => {
                // A damaged project: offer its newest readable version if any.
                self.open_latest_offer = final_path.as_deref().and_then(|p| {
                    let dir = self.config_dir.as_deref()?;
                    versions::latest_readable(dir, p).map(|v| v.path)
                });
                self.error_modal = Some(err);
            }
        }
    }

    fn initiate_open(&mut self, path: PathBuf) {
        if let Some(prompt) = restore_prompt_for_path(&path) {
            self.restore_prompt = Some(prompt);
        } else {
            self.finish_open(&path, Some(path.clone()), false);
        }
    }

    fn perform_open_recent(&mut self, idx: usize) {
        let Some(path_str) = self.prefs.recent.get(idx).cloned() else {
            return;
        };
        let path = PathBuf::from(&path_str);
        if !path.exists() {
            self.prefs.remove_recent(&path);
            if let Some(ref dir) = self.config_dir {
                let _ = prefs::save_prefs(dir, &self.prefs);
            }
            self.editor.set_recent(self.prefs.recent.clone());
            self.error_modal = Some(format!("File not found: {}", path.display()));
            return;
        }
        self.initiate_open(path);
    }

    fn execute_pending_action(&mut self, action: PendingAction, ctx: &egui::Context) {
        match action {
            PendingAction::New => {
                let course = self.prefs.blank_course();
                self.editor.replace_course(course);
                self.project = Project::new(self.editor.session.revision);
                self.reset_autosaver();
            }
            PendingAction::Open => {
                if let Some(path) = self.pick_open_file_path() {
                    self.initiate_open(path);
                }
            }
            PendingAction::OpenPath(path) => {
                self.initiate_open(path);
            }
            PendingAction::OpenFolder => {
                if let Some(path) = self.pick_open_folder_path() {
                    self.initiate_open(path);
                }
            }
            PendingAction::OpenRecent(idx) => {
                self.perform_open_recent(idx);
            }
            PendingAction::OpenExample(id) => {
                self.editor.open_example(id);
                self.project = Project::new(self.editor.session.revision);
                self.reset_autosaver();
            }
            PendingAction::OpenTemplate(id) => {
                self.editor.open_template(id);
                self.project = Project::new(self.editor.session.revision);
                self.reset_autosaver();
            }
            PendingAction::Quit | PendingAction::CloseRequested => {
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            PendingAction::OpenVersion(path) => {
                self.open_version_copy(&path);
            }
            PendingAction::ImportPptx => {
                if let Some(path) = files::pick_pptx_file() {
                    self.import_pptx_file(&path);
                }
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn update_autosave(&mut self, ctx: &egui::Context) {
        if let Some(rx) = self.autosave_rx.take() {
            match rx.try_recv() {
                Ok(outcome) => {
                    let current_epoch = self.save_epoch.load(std::sync::atomic::Ordering::Acquire);
                    if matches!(outcome.result, Ok(true)) && outcome.epoch == current_epoch {
                        self.autosaver.last_autosaved_revision = Some(outcome.revision);
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {}
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    self.autosave_rx = Some(rx);
                }
            }
        }

        let current_time = ctx.input(|i| i.time);
        let is_dirty = self.project.is_dirty(&self.editor.session);
        let rev = self.editor.session.revision;
        if self.autosave_rx.is_none()
            && self.autosaver.take_due(current_time, is_dirty, rev)
            && let Some(auto_path) = autosave::autosave_target_path(
                self.project.path.as_deref(),
                self.config_dir.as_deref(),
            )
        {
            if self.project.path.is_none() {
                autosave::write_owner_marker(&auto_path);
            }
            let course = self.editor.session.course.clone();
            let media = self.editor.used_media();
            let epoch = self.save_epoch.load(std::sync::atomic::Ordering::Acquire);
            let epoch_ref = std::sync::Arc::clone(&self.save_epoch);
            let ctx_wake = ctx.clone();
            let (tx, rx) = std::sync::mpsc::channel();
            if std::thread::Builder::new()
                .name("coursecraft-autosave".into())
                .spawn(move || {
                    let result = autosave::write_autosave_arc_guarded(
                        &course, &media, &auto_path, &epoch_ref, epoch,
                    );
                    let _ = tx.send(AutosaveOutcome {
                        revision: rev,
                        epoch,
                        result,
                    });
                    ctx_wake.request_repaint();
                })
                .is_ok()
            {
                self.autosave_rx = Some(rx);
            }
        }
        if is_dirty && let Some(wait) = self.autosaver.secs_until_next(current_time) {
            // Autosave is a timer, so never wake the UI more often than once
            // a second just to check it. The extra half second is the usual
            // margin; the floor keeps a nearly-due timer from busy-polling.
            ctx.request_repaint_after(std::time::Duration::from_secs_f64((wait + 0.5).max(1.0)));
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn pick_lottie(&mut self) {
        if let Some(path) = files::pick_lottie_file() {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("animation.json")
                .to_string();
            if let Ok(bytes) = std::fs::read(&path) {
                self.editor.import_lottie(&name, &bytes);
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn pick_web_folder(&mut self) {
        let Some(path) = files::pick_web_folder() else {
            return;
        };
        let existing: Vec<String> = self
            .editor
            .media
            .iter()
            .map(|(name, _)| name.clone())
            .collect();
        match files::import_web_folder(&path, &existing) {
            Ok(import) => {
                for (name, bytes) in import.files {
                    self.editor.add_raw_media(name, bytes.into());
                }
                coursecraft_ui_egui::insert_web_object(
                    &mut self.editor,
                    coursecraft_course::WebSource::Folder {
                        path: import.path,
                        entry: import.entry,
                    },
                );
            }
            Err(error) => self.editor.set_status(error),
        }
    }

    /// Run `action` now, or ask about unsaved changes first.
    #[cfg(not(target_arch = "wasm32"))]
    fn guard_unsaved(&mut self, action: PendingAction, ctx: &egui::Context) {
        if self.project.is_dirty(&self.editor.session) {
            self.pending_action = Some(action);
        } else {
            self.execute_pending_action(action, ctx);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    /// Write the preferences file, when there is a config folder.
    fn persist_prefs(&self) {
        if let Some(ref dir) = self.config_dir {
            let _ = prefs::save_prefs(dir, &self.prefs);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn is_blocked_file_request(&self, req: &EditorRequest) -> bool {
        let modal_open = self.pending_action.is_some()
            || self.conflict_prompt.is_some()
            || self.restore_prompt.is_some()
            || self.error_modal.is_some();
        modal_open
            && matches!(
                req,
                EditorRequest::New
                    | EditorRequest::Open
                    | EditorRequest::Save
                    | EditorRequest::SaveAs
            )
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn handle_request(&mut self, req: EditorRequest, ctx: &egui::Context) {
        if self.is_blocked_file_request(&req) {
            return;
        }
        match req {
            EditorRequest::New => self.guard_unsaved(PendingAction::New, ctx),
            EditorRequest::Open => self.guard_unsaved(PendingAction::Open, ctx),
            EditorRequest::OpenFolder => self.guard_unsaved(PendingAction::OpenFolder, ctx),
            EditorRequest::SaveAsFolder => {
                self.perform_save_as_folder();
            }
            EditorRequest::OpenRecent(idx) => {
                self.guard_unsaved(PendingAction::OpenRecent(idx), ctx);
            }
            EditorRequest::OpenExample(id) => {
                self.guard_unsaved(PendingAction::OpenExample(id), ctx);
            }
            EditorRequest::OpenTemplate(id) => {
                self.guard_unsaved(PendingAction::OpenTemplate(id), ctx);
            }
            EditorRequest::Save => {
                self.perform_save();
            }
            EditorRequest::ShowVersionHistory => {
                let rows = self.version_rows();
                self.editor.show_version_history(rows);
            }
            EditorRequest::DiscardCrashNote => {
                if let Some(dir) = self.config_dir.as_deref() {
                    crash::delete_crash_note(dir);
                }
            }
            EditorRequest::OpenVersionCopy { id } => self.request_open_version(&id, ctx),
            EditorRequest::DeleteVersion { id } => self.request_delete_version(&id),
            EditorRequest::SaveTemplateToLibrary { template_id } => {
                let Some(dir) = self.config_dir.as_deref() else {
                    return;
                };
                let Some(slide) = self.editor.session.course.template(&template_id).cloned() else {
                    return;
                };
                let media = self.editor.slide_media(&slide);
                let slide_size = self.editor.session.course.slide_size;
                match template_library::save_template(dir, &slide, slide_size, &media) {
                    Ok(saved) => {
                        self.editor.set_library_templates(
                            template_library::list_templates(dir)
                                .into_iter()
                                .map(|t| coursecraft_ui_egui::LibraryTemplateEntry {
                                    id: t.id,
                                    title: t.title,
                                })
                                .collect(),
                        );
                        self.editor
                            .set_status(format!("Saved “{}” to My library.", saved.title));
                    }
                    Err(err) => {
                        self.editor.set_status(err);
                    }
                }
            }
            EditorRequest::InsertLibraryTemplate { id, scene, after } => {
                let Some(dir) = self.config_dir.as_deref() else {
                    return;
                };
                match template_library::load_template(dir, &id) {
                    Ok((slide, media)) => {
                        self.editor
                            .insert_library_template(slide, media, scene, after);
                    }
                    Err(err) => {
                        self.editor.set_status(err);
                    }
                }
            }
            EditorRequest::DeleteLibraryTemplate { id } => {
                let Some(dir) = self.config_dir.as_deref() else {
                    return;
                };
                match template_library::delete_template(dir, &id) {
                    Ok(()) => {
                        self.editor.set_library_templates(
                            template_library::list_templates(dir)
                                .into_iter()
                                .map(|t| coursecraft_ui_egui::LibraryTemplateEntry {
                                    id: t.id,
                                    title: t.title,
                                })
                                .collect(),
                        );
                        self.editor.set_status("Deleted template from My library.");
                    }
                    Err(err) => {
                        self.editor.set_status(err);
                    }
                }
            }
            EditorRequest::SaveAs => {
                self.perform_save_as();
            }
            EditorRequest::Quit => {
                if self.project.is_dirty(&self.editor.session) {
                    self.pending_action = Some(PendingAction::Quit);
                } else {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            EditorRequest::Preview { .. } => {}
            EditorRequest::PickImage => {
                let picked = files::pick_image_file();
                if picked.is_none() {
                    // Cancelled: any pending pick waiting for this file is off.
                    self.editor.pending_image_pick = None;
                }
                if let Some(path) = picked {
                    let name = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("image.png")
                        .to_string();
                    match std::fs::read(&path) {
                        Ok(bytes) => {
                            self.editor.import_image(&name, &bytes);
                        }
                        Err(error) => self
                            .editor
                            .set_status(format!("Couldn't read {name}: {error}")),
                    }
                }
            }
            EditorRequest::PickFonts => self.pick_fonts(),
            EditorRequest::PickMediaFiles => self.import_media_files(),
            EditorRequest::ReplaceMediaFile { src } => self.replace_media_file(&src),
            EditorRequest::PickAudio => self.pick_audio(false),
            EditorRequest::PickBackgroundAudio => self.pick_audio(true),
            EditorRequest::PickResourceFile { resource } => self.pick_resource_file(&resource),
            EditorRequest::PickVideo => {
                if let Some(path) = files::pick_video_file() {
                    let name = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("video.mp4")
                        .to_string();
                    match std::fs::read(&path) {
                        Ok(bytes) => {
                            self.editor.import_video(&name, &bytes);
                        }
                        Err(error) => self
                            .editor
                            .set_status(format!("Couldn't read {name}: {error}")),
                    }
                }
            }
            EditorRequest::PickLottie => self.pick_lottie(),
            EditorRequest::PickWebFolder => self.pick_web_folder(),
            EditorRequest::PickCaptions => {
                if let Some(path) = files::pick_captions_file() {
                    let name = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("captions.vtt")
                        .to_string();
                    match std::fs::read(&path) {
                        Ok(bytes) => {
                            self.editor.import_captions(&name, &bytes);
                        }
                        Err(error) => self
                            .editor
                            .set_status(format!("Couldn't read {name}: {error}")),
                    }
                }
            }
            EditorRequest::PickPptx => {
                if self.project.is_dirty(&self.editor.session) {
                    self.pending_action = Some(PendingAction::ImportPptx);
                } else {
                    self.execute_pending_action(PendingAction::ImportPptx, ctx);
                }
            }
            EditorRequest::PickDocument => {
                if let Some(path) = files::pick_document_file() {
                    let name = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("document")
                        .to_string();
                    match std::fs::read(&path) {
                        Ok(bytes) => self.apply_document_file(&name, &bytes),
                        Err(error) => self
                            .editor
                            .set_document_error(format!("Couldn't read {name}: {error}")),
                    }
                }
            }
            EditorRequest::ExportTranslation { target_language } => {
                self.export_translation(&target_language);
            }
            EditorRequest::ImportTranslation => {
                self.import_translation();
            }
            EditorRequest::ImportQuestionsCsv => {
                self.import_questions_csv();
            }
            EditorRequest::SaveTranslation { xliff } => {
                self.save_translation(&xliff);
            }
            EditorRequest::SaveImage {
                png,
                suggested_name,
            } => {
                self.save_image(&png, &suggested_name);
            }
            EditorRequest::Publish { standard } => {
                self.start_publish(standard, ctx);
            }
            EditorRequest::ExportMp4 { width, fps } => {
                self.start_export_mp4(width, fps);
            }
            EditorRequest::ExportReview { format, options } => {
                self.start_export_review(format, options);
            }
            EditorRequest::ShowPublished => {
                self.show_published();
            }
            EditorRequest::OpenPublishedInBrowser => {
                self.open_published_in_browser();
            }
            EditorRequest::SetAutomation(enabled) => {
                self.prefs.automation_enabled = enabled;
                if let Some(ref dir) = self.config_dir {
                    let _ = prefs::save_prefs(dir, &self.prefs);
                }
                if enabled {
                    if self.control_server.is_none() {
                        match coursecraft_control::ControlServer::start_with_waker(
                            self.config_dir.as_deref(),
                            Some(control_waker(&self.repaint_ctx)),
                        ) {
                            Ok(s) => {
                                self.editor.set_automation_port(Some(s.port()));
                                self.control_server = Some(s);
                            }
                            Err(e) => {
                                eprintln!("Warning: failed to start control server: {e}");
                                self.editor.set_automation_port(None);
                            }
                        }
                    }
                } else {
                    self.control_server = None;
                    self.editor.set_automation_port(None);
                }
            }
            EditorRequest::SetPreferences {
                autosave_minutes,
                versions_to_keep,
                default_slide,
                spell_check_as_you_type,
                spell_words,
                performance_mode,
            } => {
                self.prefs.autosave_minutes = autosave_minutes.min(prefs::MAX_AUTOSAVE_MINUTES);
                self.prefs.versions_to_keep = versions_to_keep.min(prefs::MAX_VERSIONS_TO_KEEP);
                self.prefs.spell_check_as_you_type = spell_check_as_you_type;
                self.prefs.spell_words = spell_words;
                self.prefs.performance_mode = performance_mode.into();
                self.editor.set_performance_mode(performance_mode);
                if let Some(size) = prefs::DefaultSlideSize::from_valid_size(default_slide) {
                    self.prefs.default_slide = size;
                }
                self.autosaver.interval_minutes = Some(self.prefs.autosave_minutes_clamped());
                if let Some(ref dir) = self.config_dir {
                    let _ = prefs::save_prefs(dir, &self.prefs);
                }
            }
            EditorRequest::SaveLayout(layout) => {
                self.prefs.set_layout(layout);
                self.persist_prefs();
            }
            EditorRequest::SetLanguage(setting) => {
                self.editor.set_language(&setting);
                self.prefs.language.clone_from(&self.editor.language);
                self.persist_prefs();
                ctx.request_repaint();
            }
            EditorRequest::SetTheme(theme_pref) => {
                self.prefs.theme = theme_pref.into();
                self.editor.set_theme_pref(theme_pref);
                if let Some(ref dir) = self.config_dir {
                    let _ = prefs::save_prefs(dir, &self.prefs);
                }
                let mode = self.prefs.theme.resolve(ctx);
                coursecraft_ui_egui::coursecraft_render::apply_style(ctx, mode);
            }
            EditorRequest::AiChat { purpose, request } => {
                self.start_ai(purpose, request, ctx);
            }
            EditorRequest::AiTranscribe {
                purpose,
                model,
                file_name,
                bytes,
            } => {
                self.start_ai_transcription(purpose, model, file_name, bytes, ctx);
            }
            EditorRequest::AiSpeech {
                purpose,
                model,
                voice,
                text,
            } => {
                self.start_ai_speech(purpose, model, voice, text, ctx);
            }
            EditorRequest::ElevenLabsSpeech { purpose, text } => {
                self.start_elevenlabs_speech(purpose, text, ctx);
            }
            EditorRequest::ElevenLabsVoices {
                purpose,
                page_token,
            } => {
                self.start_elevenlabs_voices(purpose, page_token, ctx);
            }
            EditorRequest::ElevenLabsModels { purpose } => {
                self.start_elevenlabs_models(purpose, ctx);
            }
            EditorRequest::ElevenLabsVoiceLookup { purpose, voice_id } => {
                self.start_elevenlabs_voice(purpose, voice_id, ctx);
            }
            EditorRequest::CancelAi(purpose) => {
                self.ai.cancel();
                self.editor.set_ai_cancelled(purpose);
            }
            EditorRequest::SaveAiConfig(config) => {
                self.prefs.ai = config.as_ref().map(prefs::AiPrefs::from_config);
                if let Some(ref dir) = self.config_dir {
                    let _ = prefs::save_prefs(dir, &self.prefs);
                }
            }
            EditorRequest::SaveAiSource(source) => {
                self.save_ai_source(source);
            }
            EditorRequest::SaveElevenLabs(config) => {
                self.prefs.elevenlabs = config.as_ref().map(prefs::ElevenLabsPrefs::from_config);
                self.prefs.speech_source = self.editor.ui.speech_source.into();
                if let Some(ref dir) = self.config_dir {
                    let _ = prefs::save_prefs(dir, &self.prefs);
                }
            }
            EditorRequest::AgentDetect => {
                self.start_agent_detect(ctx);
            }
            EditorRequest::AgentAction { agent, action } => {
                self.start_agent_action(agent, action, ctx);
            }
            EditorRequest::ListSystemFonts => self.start_system_font_scan(ctx),
            EditorRequest::AddSystemFont { family, apply } => {
                self.add_system_font(&family, apply);
            }
            EditorRequest::CheckForUpdates => self.updates.request_check(),
            EditorRequest::SetAutoCheckUpdates(enabled) => self.set_auto_check_updates(enabled),
        }
    }

    /// Scan the installed font folders in the background.
    #[cfg(not(target_arch = "wasm32"))]
    fn start_system_font_scan(&mut self, ctx: &egui::Context) {
        if self.system_fonts_rx.is_some() || self.system_font_index.is_some() {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        self.system_fonts_rx = Some(rx);
        let repaint_ctx = ctx.clone();
        std::thread::spawn(move || {
            let index = std::sync::Arc::new(
                coursecraft_ui_egui::coursecraft_media::system_fonts::SystemFontIndex::scan_default(
                ),
            );
            let _ = tx.send(index);
            // Wake the UI only when there is a result to show, instead of
            // repainting every frame while the scan runs.
            repaint_ctx.request_repaint();
        });
    }

    /// Copy every face of an installed family into the course, warn if its
    /// maker restricts embedding, then run the queued `apply` command.
    #[cfg(not(target_arch = "wasm32"))]
    fn add_system_font(&mut self, family: &str, apply: Option<(String, serde_json::Value)>) {
        let already_in_course = self
            .editor
            .session
            .course
            .fonts
            .iter()
            .any(|face| face.family.eq_ignore_ascii_case(family));
        let mut added = false;
        let mut restricted = false;

        if !already_in_course
            && let Some(index) = self.system_font_index.as_ref()
            && let Some(found) = index.find_family(family)
        {
            restricted = found.embedding_restricted;
            for face in &found.faces {
                let file_name = face
                    .path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("font.ttf")
                    .to_string();
                let Some(bytes) = index.load_face_bytes(face) else {
                    continue;
                };
                if self.editor.import_font(&file_name, &bytes).is_some() {
                    added = true;
                }
            }
        }

        if restricted {
            self.editor
                .set_status(coursecraft_ui_egui::RESTRICTED_FONT_NOTE.to_string());
        }
        if let Some((cmd, params)) = apply
            && (added || already_in_course)
        {
            self.editor.run(&cmd, params);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn pick_fonts(&mut self) {
        for path in files::pick_font_files() {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("font.ttf")
                .to_string();
            if let Ok(bytes) = std::fs::read(&path) {
                self.editor.import_font(&name, &bytes);
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped_files = ctx.input(|i| i.raw.dropped_files.clone());
        for file in dropped_files {
            let path = file.path();
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("media")
                .to_string();
            let lower = name.to_ascii_lowercase();
            if lower.ends_with(".ccraft") {
                self.guard_unsaved(PendingAction::OpenPath(path.to_path_buf()), ctx);
                continue;
            }
            if lower.ends_with(".pptx") {
                self.import_pptx_file(path);
                continue;
            }
            let is_font = lower.ends_with(".ttf") || lower.ends_with(".otf");
            let is_audio = lower.ends_with(".mp3")
                || lower.ends_with(".wav")
                || lower.ends_with(".ogg")
                || lower.ends_with(".m4a");
            let is_video = lower.ends_with(".mp4") || lower.ends_with(".webm");
            let is_lottie = lower.ends_with(".lottie") || lower.ends_with(".json");
            let is_vtt = lower.ends_with(".vtt");
            let is_image = lower.ends_with(".png")
                || lower.ends_with(".jpg")
                || lower.ends_with(".jpeg")
                || lower.ends_with(".webp")
                || lower.ends_with(".gif")
                || lower.ends_with(".svg")
                || lower.ends_with(".svgz")
                || lower.ends_with(".emf")
                || lower.ends_with(".wmf")
                || lower.ends_with(".bmp")
                || lower.ends_with(".ico")
                || lower.ends_with(".tif")
                || lower.ends_with(".tiff");

            if !(is_font || is_audio || is_video || is_lottie || is_vtt || is_image) {
                let msg = match path.extension().and_then(|e| e.to_str()) {
                    Some(ext) if !ext.is_empty() => {
                        format!("CourseCraft can't import .{ext} files.")
                    }
                    _ => "CourseCraft can't import this file.".to_owned(),
                };
                self.editor.set_status(msg);
                continue;
            }

            if let Ok(bytes) = std::fs::read(path) {
                if is_font {
                    self.editor.import_font(&name, &bytes);
                } else if is_audio {
                    self.editor.import_audio(&name, &bytes);
                } else if is_video {
                    self.editor.import_video(&name, &bytes);
                } else if is_lottie {
                    self.editor.import_lottie(&name, &bytes);
                } else if is_vtt {
                    if self.editor.pending_image_pick.is_none()
                        && let Some(obj_id) = self.editor.selection.object.clone()
                        && let Some(slide) = self
                            .editor
                            .session
                            .course
                            .slide(&self.editor.selection.slide)
                        && let Some((obj, _)) = slide.object(&obj_id)
                        && matches!(
                            obj.kind,
                            coursecraft_course::ObjectKind::Audio { .. }
                                | coursecraft_course::ObjectKind::Video { .. }
                        )
                    {
                        self.editor.pending_image_pick =
                            Some(coursecraft_ui_egui::PendingImagePick::ReplaceObject(obj_id));
                    }
                    self.editor.import_captions(&name, &bytes);
                } else {
                    self.editor.import_image(&name, &bytes);
                }
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn export_translation(&mut self, target_language: &str) {
        let title = self.editor.session.course.title.clone();
        let Some(path) = files::pick_xliff_save_file(&title) else {
            return;
        };
        let result = self.editor.run(
            "course.export_xliff",
            serde_json::json!({ "target_language": target_language }),
        );
        if let Some(serde_json::Value::String(text)) = result
            && let Err(e) = files::write_atomic(&path, text.as_bytes())
        {
            self.error_modal = Some(format!("Failed to write translation: {e}"));
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn save_translation(&mut self, xliff: &str) {
        let title = self.editor.session.course.title.clone();
        let Some(path) = files::pick_xliff_save_file(&title) else {
            return;
        };
        if let Err(e) = files::write_atomic(&path, xliff.as_bytes()) {
            self.error_modal = Some(format!("Failed to write translation: {e}"));
        }
    }

    /// Ask where to save a rendered PNG, then write it.
    #[cfg(not(target_arch = "wasm32"))]
    fn save_image(&mut self, png: &[u8], suggested_name: &str) {
        let Some(path) = files::pick_image_save_file(suggested_name) else {
            return;
        };
        self.save_image_to(&path, png);
    }

    /// Write rendered PNG bytes to `path` and report the result.
    #[cfg(not(target_arch = "wasm32"))]
    fn save_image_to(&mut self, path: &Path, png: &[u8]) {
        match files::write_atomic(path, png) {
            Ok(()) => self.editor.set_status(format!("Saved {}", path.display())),
            Err(e) => self.error_modal = Some(format!("Failed to save image: {e}")),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn import_questions_csv(&mut self) {
        let Some(path) = files::pick_csv_file() else {
            return;
        };
        let csv = match std::fs::read(&path)
            .map_err(|e| e.to_string())
            .and_then(|b| String::from_utf8(b).map_err(|e| format!("not valid UTF-8: {e}")))
        {
            Ok(text) => text,
            Err(e) => {
                self.error_modal = Some(format!("Failed to read the questions file: {e}"));
                return;
            }
        };
        let params = serde_json::json!({ "csv": csv });
        if let Some(done) = self.editor.run("bank.import_csv", params) {
            let count = done
                .get("imported")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0);
            self.editor.set_status(format!(
                "Imported {count} question(s) into the question bank"
            ));
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn import_translation(&mut self) {
        let Some(path) = files::pick_xliff_file() else {
            return;
        };
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) => {
                self.error_modal = Some(format!("Failed to read translation: {e}"));
                return;
            }
        };
        let xliff = match String::from_utf8(bytes) {
            Ok(text) => text,
            Err(e) => {
                self.error_modal = Some(format!("Translation is not valid UTF-8: {e}"));
                return;
            }
        };
        if let Some(report) = self
            .editor
            .run("course.import_xliff", serde_json::json!({ "xliff": xliff }))
        {
            self.editor.set_translation_report(
                coursecraft_ui_egui::translation::import_report_text(&report),
            );
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn import_pptx_file(&mut self, path: &Path) {
        let title = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Imported course");
        match std::fs::read(path) {
            Ok(bytes) => {
                self.finish_import_pptx(&bytes, title);
            }
            Err(e) => {
                let msg = format!("Failed to read PowerPoint file: {e}");
                self.editor.ui.last_error = Some((msg.clone(), self.editor.ui.current_time));
                self.editor.ui.error_history.push(msg);
            }
        }
    }

    /// Ask for a destination and start a background publish. Never changes the session.
    #[cfg(not(target_arch = "wasm32"))]
    fn start_publish(
        &mut self,
        standard: coursecraft_ui_egui::PublishStandard,
        ctx: &egui::Context,
    ) {
        if self.publish_rx.is_some() {
            return;
        }
        let title = self.editor.session.course.title.clone();
        let Some(path) = files::pick_publish_save_file(&title, standard.file_stem()) else {
            return;
        };
        let media = self.editor.media().to_vec();
        let job = publish::PublishJob {
            course: self.editor.session.course.clone(),
            media,
            standard: publish::package_standard(standard),
            player: player_bundle::player_files(),
            out: path,
        };
        let (tx, rx) = std::sync::mpsc::channel();
        self.publish_rx = Some(rx);
        self.editor.set_publish_busy();
        let repaint = ctx.clone();
        let font_index = self.system_font_index.clone();
        std::thread::spawn(move || {
            let _ = tx.send(publish::run_publish_job_with_fonts(
                job,
                font_index.as_deref(),
            ));
            repaint.request_repaint();
        });
    }

    /// Poll the background publish, requesting repaints until it finishes.
    #[cfg(not(target_arch = "wasm32"))]
    fn poll_publish(&mut self, ctx: &egui::Context) {
        let Some(rx) = self.publish_rx.take() else {
            return;
        };
        match rx.try_recv() {
            Ok(Ok(outcome)) => {
                let name = outcome
                    .path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("course.zip")
                    .to_string();
                self.published_path = Some(outcome.path);
                self.editor.set_publish_success_with_fonts(
                    name,
                    outcome.bytes,
                    outcome.embedded_fonts,
                    outcome.font_warnings,
                    outcome.pending_save_fonts,
                );
            }
            Ok(Err(message)) => {
                self.editor.set_publish_failure(message);
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                self.publish_rx = Some(rx);
                // Waiting on a background job: check at most once a second
                // instead of forcing a redraw every frame.
                ctx.request_repaint_after(std::time::Duration::from_secs(1));
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.editor
                    .set_publish_failure("Publishing stopped unexpectedly. Try again.");
            }
        }
    }

    /// Poll the background font scan and hand its result to the editor.
    #[cfg(not(target_arch = "wasm32"))]
    fn poll_system_fonts(&mut self, ctx: &egui::Context) {
        let Some(rx) = self.system_fonts_rx.take() else {
            return;
        };
        match rx.try_recv() {
            Ok(index) => {
                self.editor.ui.system_fonts = Some(
                    index
                        .families()
                        .iter()
                        .map(|family| coursecraft_ui_egui::SystemFontChoice {
                            name: family.name.clone(),
                            embedding_restricted: family.embedding_restricted,
                        })
                        .collect(),
                );
                let provider: std::sync::Arc<dyn coursecraft_ui_egui::SystemFontProvider> =
                    std::sync::Arc::new(coursecraft_ui_egui::IndexFontProvider(
                        std::sync::Arc::clone(&index),
                    ));
                self.editor.set_system_font_provider(Some(provider));
                self.system_font_index = Some(index);
                ctx.request_repaint();
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                // The scan thread calls request_repaint() when it sends, so
                // there is no need to wake the UI on every polling frame.
                self.system_fonts_rx = Some(rx);
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.editor.ui.system_fonts = Some(Vec::new());
            }
        }
    }

    /// Reveal the last published zip in the system file browser.
    #[cfg(not(target_arch = "wasm32"))]
    fn show_published(&mut self) {
        let Some(path) = self.published_path.clone() else {
            self.editor
                .set_publish_status(Some("There is no published file to show.".to_string()));
            return;
        };
        match reveal_in_folder(&path) {
            Ok(()) => self.editor.set_publish_status(None),
            Err(message) => self.editor.set_publish_status(Some(message)),
        }
    }

    /// Serve the last published zip on a local server and open it in the browser.
    #[cfg(not(target_arch = "wasm32"))]
    fn open_published_in_browser(&mut self) {
        let Some(path) = self.published_path.clone() else {
            self.editor
                .set_publish_status(Some("There is no published file to open.".to_string()));
            return;
        };
        // Drop any previous server before starting the replacement.
        self.preview_server = None;
        let server = match preview_server::PreviewServer::start_from_file(&path) {
            Ok(server) => server,
            Err(message) => {
                self.editor.set_publish_status(Some(message));
                return;
            }
        };
        let url = server.url();
        self.preview_server = Some(server);
        match preview_server::open_url_in_browser(&url) {
            Ok(()) => self.editor.set_publish_status(None),
            Err(message) => self.editor.set_publish_status(Some(message)),
        }
    }

    /// Ask for a destination and start a background MP4 export. Never changes the session.
    #[cfg(not(target_arch = "wasm32"))]
    fn start_export_mp4(&mut self, width: u32, fps: u32) {
        if self.export_rx.is_some() {
            return;
        }
        let title = self.editor.session.course.title.clone();
        let Some(path) = self.pick_export_path(&title) else {
            return;
        };
        let media = self.editor.media().to_vec();
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let job = export::ExportMp4Job {
            course: self.editor.session.course.clone(),
            media,
            out: path,
            width,
            fps,
            cancel: std::sync::Arc::clone(&cancel),
        };
        let (tx, rx) = std::sync::mpsc::channel();
        self.export_rx = Some(rx);
        self.editor.ui.export_status = None;
        self.editor.ui.export_job = Some(coursecraft_ui_egui::ExportJobView {
            slide_index: 0,
            slide_total: 1,
            frames: 0,
            cancel,
        });
        std::thread::spawn(move || {
            let mut progress = |p: &coursecraft_export::ExportProgress| {
                let _ = tx.send(export::ExportUpdate::Progress(*p));
            };
            let result = export::run_export_job(job, &mut progress);
            let _ = tx.send(export::ExportUpdate::Finished(result));
        });
    }

    /// The save dialog seam, so tests never open a native dialog.
    #[cfg(not(target_arch = "wasm32"))]
    fn pick_export_path(&mut self, title: &str) -> Option<PathBuf> {
        #[cfg(test)]
        if let Some(path) = self.test_export_path.take() {
            return Some(path);
        }
        export::pick_export_save_file(title)
    }

    /// Ask for a destination and start a background PDF or Word export.
    #[cfg(not(target_arch = "wasm32"))]
    fn start_export_review(
        &mut self,
        format: coursecraft_ui_egui::ReviewFormat,
        options: coursecraft_ui_egui::ReviewOptionsView,
    ) {
        if self.export_review_rx.is_some() {
            return;
        }
        let title = self.editor.session.course.title.clone();
        let Some(path) = self.pick_review_export_path(&title, format) else {
            return;
        };
        let media = self.editor.media().to_vec();
        let job = export::ExportReviewJob {
            course: self.editor.session.course.clone(),
            media,
            out: path,
            format,
            options,
        };
        let (tx, rx) = std::sync::mpsc::channel();
        self.export_review_rx = Some(rx);
        self.editor.ui.export_status = None;
        std::thread::spawn(move || {
            let _ = tx.send(export::run_export_review_job(job));
        });
    }

    /// The review save dialog seam, so tests never open a native dialog.
    #[cfg(not(target_arch = "wasm32"))]
    fn pick_review_export_path(
        &mut self,
        title: &str,
        format: coursecraft_ui_egui::ReviewFormat,
    ) -> Option<PathBuf> {
        #[cfg(test)]
        if let Some(path) = self.test_export_path.take() {
            return Some(path);
        }
        export::pick_review_save_file(title, format)
    }

    /// Poll the background export, requesting repaints until it finishes.
    #[cfg(not(target_arch = "wasm32"))]
    fn poll_export(&mut self, ctx: &egui::Context) {
        let Some(rx) = self.export_rx.take() else {
            return;
        };
        loop {
            match rx.try_recv() {
                Ok(export::ExportUpdate::Progress(progress)) => {
                    if let Some(job) = &mut self.editor.ui.export_job {
                        job.slide_index = progress.slide_index;
                        job.slide_total = progress.slide_total;
                        job.frames = progress.frames_written;
                    }
                }
                Ok(export::ExportUpdate::Finished(result)) => {
                    self.finish_export(result);
                    ctx.request_repaint();
                    return;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    self.export_rx = Some(rx);
                    // The progress bar is waiting on a background job; a
                    // one-second poll is enough and keeps idle frames quiet.
                    ctx.request_repaint_after(std::time::Duration::from_secs(1));
                    return;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.finish_export(Err(
                        "Exporting stopped unexpectedly. Try again.".to_string()
                    ));
                    return;
                }
            }
        }
    }

    /// Show a finished export in the status bar, or its error.
    #[cfg(not(target_arch = "wasm32"))]
    fn finish_export(&mut self, result: Result<export::ExportMp4Outcome, String>) {
        self.editor.ui.export_job = None;
        match result {
            Ok(outcome) => {
                let name = outcome
                    .path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("video.mp4")
                    .to_string();
                let seconds = outcome.report.duration_ms / 1000;
                let skipped = outcome.report.skipped_questions.len();
                let questions = if skipped == 1 {
                    "question"
                } else {
                    "questions"
                };
                let message = format!(
                    "Exported {name} ({}:{:02}, {skipped} {questions} skipped)",
                    seconds / 60,
                    seconds % 60
                );
                self.published_path = Some(outcome.path);
                self.editor.set_export_report(message);
            }
            Err(message) => {
                self.editor.ui.last_error = Some((message.clone(), self.editor.ui.current_time));
                self.editor.ui.error_history.push(message.clone());
                self.editor.set_status(message);
            }
        }
    }

    /// Poll a background PDF or Word export, requesting repaints until it finishes.
    #[cfg(not(target_arch = "wasm32"))]
    fn poll_export_review(&mut self, ctx: &egui::Context) {
        let Some(rx) = self.export_review_rx.take() else {
            return;
        };
        match rx.try_recv() {
            Ok(result) => {
                self.finish_export_review(result);
                ctx.request_repaint();
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                self.export_review_rx = Some(rx);
                // Waiting on a background job: check at most once a second.
                ctx.request_repaint_after(std::time::Duration::from_secs(1));
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.finish_export_review(Err(
                    "Exporting the review document stopped unexpectedly. Try again.".to_string(),
                ));
            }
        }
    }

    /// Show a finished review export in the status bar, or its error.
    #[cfg(not(target_arch = "wasm32"))]
    fn finish_export_review(&mut self, result: Result<PathBuf, String>) {
        match result {
            Ok(path) => {
                let name = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("review document")
                    .to_string();
                self.published_path = Some(path);
                self.editor.set_export_report(format!("Exported {name}"));
            }
            Err(message) => {
                self.editor.ui.last_error = Some((message.clone(), self.editor.ui.current_time));
                self.editor.ui.error_history.push(message.clone());
                self.editor.set_status(message);
            }
        }
    }

    /// Start an agent detection pass on a background thread.
    #[cfg(not(target_arch = "wasm32"))]
    fn start_agent_detect(&mut self, ctx: &egui::Context) {
        if self.agents.is_pending() {
            return;
        }
        let host: std::sync::Arc<dyn agents::AgentHost> =
            std::sync::Arc::new(agents::SystemAgentHost);
        let env = agents::AgentEnv::from_system(self.editor.agent_exe_path.clone());
        let repaint_ctx = ctx.clone();
        self.agents.detect(host, env, move || {
            repaint_ctx.request_repaint();
        });
    }

    /// Start one agent action on a background thread.
    #[cfg(not(target_arch = "wasm32"))]
    fn start_agent_action(
        &mut self,
        agent: coursecraft_ai::agents::AgentId,
        action: coursecraft_ai::agents::AgentAction,
        ctx: &egui::Context,
    ) {
        if self.agents.is_pending() {
            self.editor.set_agent_action_result(
                agent,
                Err("Another agent check is still running. Try again.".to_string()),
            );
            return;
        }
        let host: std::sync::Arc<dyn agents::AgentHost> =
            std::sync::Arc::new(agents::SystemAgentHost);
        let env = agents::AgentEnv::from_system(self.editor.agent_exe_path.clone());
        let repaint_ctx = ctx.clone();
        self.agents.run_action(host, env, agent, action, move || {
            repaint_ctx.request_repaint();
        });
    }

    /// Hand a finished agent check or action back to the editor.
    #[cfg(not(target_arch = "wasm32"))]
    fn poll_agents(&mut self, ctx: &egui::Context) {
        if let Some(message) = self.agents.poll() {
            match message {
                agents::AgentMessage::Statuses(statuses) => {
                    self.editor.set_agent_statuses(statuses);
                }
                agents::AgentMessage::ActionResult(agent, result) => {
                    self.editor.set_agent_action_result(agent, result);
                }
                agents::AgentMessage::Stopped => {
                    let statuses = coursecraft_ai::agents::AgentId::ALL
                        .into_iter()
                        .map(|id| {
                            (
                                id,
                                coursecraft_ai::agents::AgentStatus::Problem(
                                    "The agent check stopped unexpectedly. Try again.".to_string(),
                                ),
                            )
                        })
                        .collect();
                    self.editor.set_agent_statuses(statuses);
                }
            }
        }
        if self.agents.is_pending() {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn save_prefs(&self) {
        if let Some(ref dir) = self.config_dir {
            let _ = prefs::save_prefs(dir, &self.prefs);
        }
    }

    /// Persist the automatic update check preference.
    #[cfg(not(target_arch = "wasm32"))]
    fn set_auto_check_updates(&mut self, enabled: bool) {
        self.prefs.auto_check_updates = enabled;
        self.editor.set_auto_check_updates(enabled);
        self.save_prefs();
    }
}

/// Open the system file browser at `path`, without a shell.
#[cfg(not(target_arch = "wasm32"))]
fn reveal_in_folder(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = std::process::Command::new("open");
        command.arg("-R").arg(path);
        command
    };

    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = std::process::Command::new("explorer");
        command.arg(format!("/select,{}", path.display()));
        command
    };

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let mut command = {
        let parent = path.parent().filter(|p| !p.as_os_str().is_empty());
        let mut command = std::process::Command::new("xdg-open");
        command.arg(parent.unwrap_or_else(|| Path::new(".")));
        command
    };

    match command.status() {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(format!("Couldn't open the folder: {status}")),
        Err(e) => Err(format!("Couldn't open the folder: {e}")),
    }
}

impl EditorApp {
    /// Store the chosen AI source without touching the endpoint values.
    #[cfg(not(target_arch = "wasm32"))]
    fn save_ai_source(&mut self, source: coursecraft_ai::AiSource) {
        match self.prefs.ai.as_mut() {
            Some(ai) => ai.source = source,
            None if source.is_agent() => {
                self.prefs.ai = Some(prefs::AiPrefs {
                    source,
                    ..prefs::AiPrefs::default()
                });
            }
            None => {}
        }
        if let Some(ref dir) = self.config_dir {
            let _ = prefs::save_prefs(dir, &self.prefs);
        }
    }

    /// Start a background AI call with the current dialog settings.
    fn start_ai(&mut self, purpose: AiPurpose, request: ChatRequest, ctx: &egui::Context) {
        let now = ctx.input(|input| input.time);
        #[cfg(not(target_arch = "wasm32"))]
        let source = self.editor.ui.ai.source;
        #[cfg(not(target_arch = "wasm32"))]
        if source.is_agent() {
            let host: std::sync::Arc<dyn agents::AgentHost> =
                std::sync::Arc::new(agents::SystemAgentHost);
            let env = agents::AgentEnv::from_system(self.editor.agent_exe_path.clone());
            let repaint_ctx = ctx.clone();
            if let Err(message) = self.ai.begin_agent(
                purpose.clone(),
                source,
                &request,
                now,
                host,
                env,
                move || {
                    repaint_ctx.request_repaint();
                },
            ) {
                self.editor.set_ai_result(purpose, Err(message));
            }
            return;
        }
        let config = self.editor.ui.ai.config.clone();
        let repaint_ctx = ctx.clone();
        if let Err(message) = self
            .ai
            .begin(purpose.clone(), &config, &request, now, move || {
                repaint_ctx.request_repaint();
            })
        {
            self.editor.set_ai_result(purpose, Err(message));
        }
    }

    /// Start a speech-to-text call with a media file's raw bytes.
    fn start_ai_transcription(
        &mut self,
        purpose: AiPurpose,
        model: String,
        file_name: String,
        bytes: Vec<u8>,
        ctx: &egui::Context,
    ) {
        let now = ctx.input(|input| input.time);
        let config = self.editor.ui.ai.config.clone();
        let repaint_ctx = ctx.clone();
        if let Err(message) = self.ai.begin_transcription(
            purpose.clone(),
            &config,
            &model,
            &file_name,
            &bytes,
            now,
            move || {
                repaint_ctx.request_repaint();
            },
        ) {
            self.editor.set_audio_result(purpose, Err(message));
        }
    }

    /// Start a text-to-speech call.
    fn start_ai_speech(
        &mut self,
        purpose: AiPurpose,
        model: String,
        voice: String,
        text: String,
        ctx: &egui::Context,
    ) {
        let now = ctx.input(|input| input.time);
        let config = self.editor.ui.ai.config.clone();
        let repaint_ctx = ctx.clone();
        if let Err(message) = self.ai.begin_speech(
            purpose.clone(),
            &config,
            &model,
            &voice,
            &text,
            now,
            move || {
                repaint_ctx.request_repaint();
            },
        ) {
            self.editor.set_audio_result(purpose, Err(message));
        }
    }

    /// Start an ElevenLabs text-to-speech call with the window's settings.
    fn start_elevenlabs_speech(&mut self, purpose: AiPurpose, text: String, ctx: &egui::Context) {
        let now = ctx.input(|input| input.time);
        let config = self.editor.ui.elevenlabs.config.clone();
        let max_chars = self.editor.ui.elevenlabs.max_chars();
        let repaint_ctx = ctx.clone();
        if let Err(message) = self.ai.begin_elevenlabs_speech(
            purpose.clone(),
            &config,
            &text,
            max_chars,
            now,
            move || {
                repaint_ctx.request_repaint();
            },
        ) {
            self.editor.set_audio_result(purpose, Err(message));
        }
    }

    /// Start one page of ElevenLabs voices.
    fn start_elevenlabs_voices(
        &mut self,
        purpose: AiPurpose,
        page_token: Option<String>,
        ctx: &egui::Context,
    ) {
        let now = ctx.input(|input| input.time);
        let config = self.editor.ui.elevenlabs.config.clone();
        let repaint_ctx = ctx.clone();
        if let Err(message) = self.ai.begin_elevenlabs_voices(
            purpose.clone(),
            &config,
            page_token.as_deref(),
            now,
            move || {
                repaint_ctx.request_repaint();
            },
        ) {
            self.editor.set_elevenlabs_result(purpose, Err(message));
        }
    }

    /// Start an ElevenLabs voice lookup.
    fn start_elevenlabs_voice(
        &mut self,
        purpose: AiPurpose,
        voice_id: String,
        ctx: &egui::Context,
    ) {
        let now = ctx.input(|input| input.time);
        let config = self.editor.ui.elevenlabs.config.clone();
        let repaint_ctx = ctx.clone();
        if let Err(message) =
            self.ai
                .begin_elevenlabs_voice(purpose.clone(), &config, &voice_id, now, move || {
                    repaint_ctx.request_repaint();
                })
        {
            self.editor.set_elevenlabs_result(purpose, Err(message));
        }
    }

    /// Start the ElevenLabs model list.
    fn start_elevenlabs_models(&mut self, purpose: AiPurpose, ctx: &egui::Context) {
        let now = ctx.input(|input| input.time);
        let config = self.editor.ui.elevenlabs.config.clone();
        let repaint_ctx = ctx.clone();
        if let Err(message) =
            self.ai
                .begin_elevenlabs_models(purpose.clone(), &config, now, move || {
                    repaint_ctx.request_repaint();
                })
        {
            self.editor.set_elevenlabs_result(purpose, Err(message));
        }
    }

    /// Hand a finished AI call back to the dialog, and keep polling until it
    /// arrives or the timeout drops it.
    fn poll_ai(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|input| input.time);
        if let Some((purpose, result)) = self.ai.poll(now) {
            match purpose {
                AiPurpose::Captions { .. } | AiPurpose::Speech { .. } => {
                    self.editor.set_audio_result(purpose, result);
                }
                AiPurpose::ElevenLabsVoices { .. }
                | AiPurpose::ElevenLabsModels
                | AiPurpose::ElevenLabsVoiceLookup => {
                    self.editor.set_elevenlabs_result(purpose, result);
                }
                _ => {
                    let chat = match result {
                        Ok(AiReply::Chat(reply)) => Ok(reply),
                        Ok(_) => Err("The endpoint returned the wrong kind of answer.".to_string()),
                        Err(message) => Err(message),
                    };
                    self.editor.set_ai_result(purpose, chat);
                }
            }
        }
        if self.ai.is_pending() {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
    }

    /// Put a chosen document's text into the Course from Document window.
    fn apply_document_file(&mut self, name: &str, bytes: &[u8]) {
        match document_text_from_bytes(name, bytes) {
            Ok(text) => self.editor.set_document_text(text),
            Err(message) => self
                .editor
                .set_document_error(format!("Couldn't open {name}. {message}")),
        }
    }

    fn finish_import_pptx(&mut self, bytes: &[u8], title: &str) {
        match coursecraft_pptx::import_pptx(bytes, title) {
            Ok(mut imported) => {
                #[cfg(not(target_arch = "wasm32"))]
                if let Some(index) = self.system_font_index.as_ref() {
                    imported
                        .report
                        .filter_installed_fonts(|name| index.has_family(name));
                }
                self.editor.replace_course(imported.course);
                let media_arc = into_arc_media(imported.media);
                self.editor.set_media(media_arc);
                #[cfg(not(target_arch = "wasm32"))]
                {
                    self.project = Project::new(self.editor.session.revision);
                    // Mark dirty so closing prompts to save
                    self.project.saved_revision = u64::MAX;
                    self.reset_autosaver();
                }
                #[cfg(target_arch = "wasm32")]
                self.web_unsaved.mark_dirty_after_import();
                let summary = imported.report.summary_window_text();
                self.editor.set_import_report(summary);
            }
            Err(e) => {
                let msg = format!("Failed to import PowerPoint: {e}");
                self.editor.ui.last_error = Some((msg.clone(), self.editor.ui.current_time));
                self.editor.ui.error_history.push(msg);
            }
        }
    }
}

/// Format chosen when saving an already-open project path.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SaveKind {
    Folder,
    Json,
    JsonNeedsSaveAs,
    Ccraft,
}

#[cfg(not(target_arch = "wasm32"))]
fn has_extension_ignore_ascii_case(path: &Path, ext: &str) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case(ext))
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn save_kind(path: &Path, is_open_folder: bool, has_media: bool) -> SaveKind {
    if is_open_folder {
        SaveKind::Folder
    } else if has_extension_ignore_ascii_case(path, "json") {
        if has_media {
            SaveKind::JsonNeedsSaveAs
        } else {
            SaveKind::Json
        }
    } else {
        SaveKind::Ccraft
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn control_save_is_folder(target: &Path, project: &Project) -> bool {
    let is_open_folder = project.folder.is_some() && project.path.as_deref() == Some(target);
    is_open_folder || has_extension_ignore_ascii_case(target, coursecraft_format::FOLDER_EXTENSION)
}

#[cfg(not(target_arch = "wasm32"))]
struct EditorHostAdapter<'a> {
    editor: &'a mut Editor,
    project: &'a mut Project,
    config_dir: Option<&'a Path>,
    prefs: &'a mut Prefs,
    save_epoch: &'a std::sync::Arc<std::sync::atomic::AtomicU64>,
}

#[cfg(not(target_arch = "wasm32"))]
impl<'a> coursecraft_control::ControlHost for EditorHostAdapter<'a> {
    fn commands(&self) -> Vec<coursecraft_control::CommandInfo> {
        coursecraft_engine::registry()
            .iter()
            .map(|c| coursecraft_control::CommandInfo {
                id: c.id.to_string(),
                label: c.label.to_string(),
                menu: c.menu.to_string(),
                params: c.params.to_string(),
                mutates: c.mutates,
            })
            .collect()
    }

    fn run(
        &mut self,
        id: &str,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value, coursecraft_control::HostError> {
        match self.editor.run(id, params.clone()) {
            Some(v) => Ok(v),
            None => {
                let msg = self
                    .editor
                    .ui
                    .last_error
                    .as_ref()
                    .map(|(m, _)| m.clone())
                    .unwrap_or_else(|| "command failed".to_string());
                Err(coursecraft_control::HostError::Engine(msg))
            }
        }
    }

    fn batch(
        &mut self,
        steps: &[(String, serde_json::Value)],
    ) -> Result<Vec<serde_json::Value>, coursecraft_control::HostError> {
        match self.editor.run_batch(steps) {
            Some(v) => Ok(v),
            None => {
                let msg = self
                    .editor
                    .ui
                    .last_error
                    .as_ref()
                    .map(|(m, _)| m.clone())
                    .unwrap_or_else(|| "batch failed".to_string());
                Err(coursecraft_control::HostError::Engine(msg))
            }
        }
    }

    fn course(&self) -> &Course {
        &self.editor.session.course
    }

    fn selection(&self) -> coursecraft_control::Selection {
        coursecraft_control::Selection {
            slide: self.editor.selection.slide.clone(),
            layer: self.editor.selection.layer.clone(),
            object: self.editor.selection.object.clone(),
        }
    }

    fn set_selection(
        &mut self,
        slide: Option<String>,
        layer: Option<Option<String>>,
        object: Option<Option<String>>,
    ) -> Result<coursecraft_control::Selection, coursecraft_control::HostError> {
        let target_slide = slide.unwrap_or_else(|| self.editor.selection.slide.clone());
        if target_slide.is_empty() {
            return Err(coursecraft_control::HostError::NotFound(
                "no slide selected".into(),
            ));
        }
        let slide_ref = self
            .editor
            .session
            .course
            .slide(&target_slide)
            .ok_or_else(|| {
                coursecraft_control::HostError::NotFound(format!(
                    "slide {target_slide:?} not found"
                ))
            })?;

        let target_layer = match layer {
            None => self.editor.selection.layer.clone(),
            Some(None) => None,
            Some(Some(lid)) => {
                if slide_ref.layer(&lid).is_none() {
                    return Err(coursecraft_control::HostError::NotFound(format!(
                        "layer {lid:?} not found"
                    )));
                }
                Some(lid)
            }
        };

        let target_object = match object {
            None => self.editor.selection.object.clone(),
            Some(None) => None,
            Some(Some(oid)) => {
                let exists = match target_layer.as_deref() {
                    Some(lid) => slide_ref
                        .layer(lid)
                        .is_some_and(|l| l.objects.iter().any(|o| o.id == oid)),
                    None => slide_ref.objects.iter().any(|o| o.id == oid),
                };
                if !exists {
                    return Err(coursecraft_control::HostError::NotFound(format!(
                        "object {oid:?} not found"
                    )));
                }
                Some(oid)
            }
        };

        self.editor.selection.slide = target_slide;
        self.editor.selection.layer = target_layer;
        self.editor.selection.object = target_object;
        Ok(self.selection())
    }

    fn save(&mut self, path: Option<&Path>) -> Result<PathBuf, coursecraft_control::HostError> {
        let target = match path {
            Some(p) => p.to_path_buf(),
            None => self.project.path.clone().ok_or_else(|| {
                coursecraft_control::HostError::BadParams(
                    "no path specified for untitled course".into(),
                )
            })?,
        };

        let media = self.editor.used_media();
        let is_ccraft = has_extension_ignore_ascii_case(&target, "ccraft");
        let is_project_folder = control_save_is_folder(&target, self.project);
        self.save_epoch
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);

        if is_project_folder {
            // Keep the record when saving over the folder that is already open.
            let same = self.project.path.as_deref() == Some(target.as_path());
            let mut record = self
                .project
                .folder
                .take()
                .filter(|_| same)
                .unwrap_or_default();
            let result = files::save_folder_to_arc(
                &target,
                &self.editor.session.course,
                &media,
                &mut record,
            );
            self.project.folder = Some(record);
            result.map_err(coursecraft_control::HostError::Engine)?;
        } else if is_ccraft {
            if let Some(dir) = self.config_dir {
                let keep = self.prefs.versions_to_keep_clamped();
                let title = &self.editor.session.course.title;
                if let Err(e) = versions::snapshot_before_save(dir, &target, keep, title) {
                    eprintln!("Couldn't keep a version of {}: {e}", target.display());
                    self.editor
                        .set_status("Couldn't keep a version of this file");
                }
            }
            files::save_to_arc(&target, &self.editor.session.course, &media)
                .map_err(coursecraft_control::HostError::Engine)?;
        } else {
            let is_dir = target.is_dir()
                || target
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_none_or(|e| !e.eq_ignore_ascii_case("json"));
            let json_file = if is_dir {
                std::fs::create_dir_all(&target)
                    .map_err(|e| coursecraft_control::HostError::Io(e.to_string()))?;
                target.join("course.json")
            } else {
                target.clone()
            };
            let json = self
                .editor
                .session
                .course
                .to_json()
                .map_err(|e| coursecraft_control::HostError::Engine(e.to_string()))?;
            files::write_atomic(&json_file, format!("{json}\n").as_bytes())
                .map_err(|e| coursecraft_control::HostError::Io(e.to_string()))?;
        }

        if !is_project_folder {
            self.project.folder = None;
        }
        self.project.path = Some(target.clone());
        self.project.saved_revision = self.editor.session.revision;
        self.project.record_disk_stamp();
        let auto_path = autosave::autosave_path_for_project(Some(&target), Path::new(""));
        autosave::delete_autosave(&auto_path);
        if let Some(dir) = self.config_dir {
            let untitled_path = autosave::autosave_path_for_project(None, dir);
            autosave::delete_untitled_autosave(&untitled_path);
            self.prefs.add_recent(&target);
            let _ = prefs::save_prefs(dir, self.prefs);
        }
        self.editor.set_recent(self.prefs.recent.clone());
        Ok(target)
    }

    fn open(&mut self, path: &Path, force: bool) -> Result<String, coursecraft_control::HostError> {
        if self.project.is_dirty(&self.editor.session) && !force {
            return Err(coursecraft_control::HostError::UnsavedChanges);
        }

        let opened = if path.is_dir() {
            files::open_folder_from(path).map(|(loaded, record)| (loaded, Some(record)))
        } else {
            files::open_from(path).map(|loaded| (loaded, None))
        };
        match opened {
            Ok(((course, media), record)) => {
                let title = course.title.clone();
                let media_arc = into_arc_media(media);
                self.editor.replace_course(course);
                self.editor.set_media(media_arc);
                self.project.path = Some(path.to_path_buf());
                self.project.folder = record;
                self.project.saved_revision = self.editor.session.revision;
                self.project.record_disk_stamp();
                self.prefs.add_recent(path);
                if let Some(dir) = self.config_dir {
                    let _ = prefs::save_prefs(dir, self.prefs);
                }
                self.editor.set_recent(self.prefs.recent.clone());
                self.save_epoch
                    .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
                Ok(title)
            }
            Err(e) => Err(coursecraft_control::HostError::Engine(e)),
        }
    }

    fn info(&self) -> coursecraft_control::SessionInfo {
        coursecraft_control::SessionInfo {
            title: self.editor.session.course.title.clone(),
            path: self
                .project
                .path
                .as_ref()
                .map(|p| p.to_string_lossy().to_string()),
            dirty: self.project.is_dirty(&self.editor.session),
            revision: self.editor.session.revision,
            can_undo: self.editor.session.can_undo(),
            can_redo: self.editor.session.can_redo(),
        }
    }

    fn undo(&mut self) -> Result<u64, coursecraft_control::HostError> {
        match self.editor.run("edit.undo", serde_json::json!({})) {
            Some(_) => Ok(self.editor.session.revision),
            None => Err(coursecraft_control::HostError::Engine(
                "nothing to undo".into(),
            )),
        }
    }

    fn redo(&mut self) -> Result<u64, coursecraft_control::HostError> {
        match self.editor.run("edit.redo", serde_json::json!({})) {
            Some(_) => Ok(self.editor.session.revision),
            None => Err(coursecraft_control::HostError::Engine(
                "nothing to redo".into(),
            )),
        }
    }

    fn media(&self) -> &dyn coursecraft_control::MediaSource {
        self.editor
    }
}

impl eframe::App for EditorApp {
    /// The window is closing for good: a ready update's helper starts now and installs it once
    /// the process has exited.
    #[cfg(not(target_arch = "wasm32"))]
    fn on_exit(&mut self) {
        // A normal close leaves no untitled autosave of this window behind.
        if let Some(dir) = self.config_dir.as_deref() {
            autosave::delete_untitled_autosave(&autosave::autosave_path_for_project(None, dir));
        }
        self.updates.start_pending_update();
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.show_app(ui);
    }
}

impl EditorApp {
    fn show_app(&mut self, ui: &mut egui::Ui) {
        self.poll_ai(ui.ctx());
        #[cfg(not(target_arch = "wasm32"))]
        self.poll_agents(ui.ctx());

        #[cfg(not(target_arch = "wasm32"))]
        {
            // 1. Intercept window close requested
            let close_requested = ui.ctx().input(|i| i.viewport().close_requested());
            if should_intercept_close(
                close_requested,
                self.project.is_dirty(&self.editor.session),
                self.allow_close,
            ) {
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.pending_action = Some(PendingAction::CloseRequested);
            }

            // 2. Periodic autosave
            self.update_autosave(ui.ctx());

            // 2a. Thumbnails of the recent courses on the start page
            self.recent_previews.update(&mut self.editor, ui.ctx());

            // 2b. Background publish results
            self.poll_publish(ui.ctx());
            self.poll_export(ui.ctx());
            self.poll_export_review(ui.ctx());
            self.poll_system_fonts(ui.ctx());
            self.updates.tick(ui.ctx());

            // 3. System theme synchronization
            let desired_mode = self.prefs.theme.resolve(ui.ctx());
            if coursecraft_ui_egui::coursecraft_render::theme_mode(ui.ctx()) != desired_mode {
                coursecraft_ui_egui::coursecraft_render::apply_style(ui.ctx(), desired_mode);
            }

            // 4. Update window title and status info
            let is_dirty = self.project.is_dirty(&self.editor.session);
            let name = self.project.file_name_display();
            let title = window_title(&name, is_dirty);
            if title_changed(&mut self.last_title, &title) {
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::Title(title));
            }
            self.editor.set_title_info(name, is_dirty);
            self.editor
                .set_project_has_file(self.project.path.is_some());
            self.editor.set_project_is_folder(self.project.is_folder());

            // 5. Sync coach marks preference if changed in editor
            if self.editor.coach_marks_done() != self.prefs.coach_marks_done {
                self.prefs.coach_marks_done = self.editor.coach_marks_done();
                if let Some(ref dir) = self.config_dir {
                    let _ = prefs::save_prefs(dir, &self.prefs);
                }
            }

            // 6. Poll control server
            if let Some(ref server) = self.control_server {
                let mut host = EditorHostAdapter {
                    editor: &mut self.editor,
                    project: &mut self.project,
                    config_dir: self.config_dir.as_deref(),
                    prefs: &mut self.prefs,
                    save_epoch: &self.save_epoch,
                };
                if server.poll(&mut host) > 0 {
                    ui.ctx().request_repaint();
                }
            }
        }

        // Insert a clipboard image before the editor UI consumes the frame's
        // input events; text and object paste stay with the editor UI.
        #[cfg(not(target_arch = "wasm32"))]
        paste_image::maybe_paste_image(ui.ctx(), &mut self.editor);

        #[cfg(not(target_arch = "wasm32"))]
        let shell_modal_esc = (self.pending_action.is_some()
            || self.conflict_prompt.is_some()
            || self.error_modal.is_some())
            && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));

        #[cfg(target_arch = "wasm32")]
        let web_modal_esc = {
            self.step_web_publish(ui.ctx());
            let is_dirty = self.web_unsaved.is_dirty(self.editor.session.revision);
            let name = self.web_unsaved.file_name_display().to_owned();
            self.editor.set_title_info(name, is_dirty);
            self.web_unsaved.pending.is_some()
                && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        };

        // 4. Editor main UI
        let _requests = self.editor.show(ui);

        #[cfg(not(target_arch = "wasm32"))]
        {
            for req in _requests {
                self.handle_request(req, ui.ctx());
            }

            self.handle_dropped_files(ui.ctx());

            // The update prompt and the result of a requested check. An installation starts,
            // and a ready update asks the window to close, only while nothing is unsaved.
            let clean = !self.project.is_dirty(&self.editor.session);
            self.updates.show(ui.ctx(), clean);

            if ui.ctx().input(|i| !i.raw.hovered_files.is_empty()) {
                let screen = ui.ctx().content_rect();
                let accent = ui.style().visuals.selection.stroke.color;
                egui::Area::new(egui::Id::unique("drop_import_overlay"))
                    .order(egui::Order::Foreground)
                    .fixed_pos(screen.min)
                    .interactable(false)
                    .show(ui.ctx(), |ui| {
                        let (rect, _) = ui.allocate_exact_size(screen.size(), egui::Sense::hover());
                        let painter = ui.painter_at(rect);
                        painter.rect_filled(
                            rect,
                            0.0,
                            egui::Color32::from_rgba_unmultiplied(0, 0, 0, 96),
                        );
                        painter.rect_stroke(
                            rect.shrink(16.0),
                            8.0,
                            egui::Stroke::new(2.0, accent),
                            egui::StrokeKind::Inside,
                        );
                        painter.text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            "Drop to import",
                            egui::FontId::proportional(20.0),
                            egui::Color32::WHITE,
                        );
                    });
            }

            // Unsaved changes confirmation modal
            if self.pending_action.is_some() {
                let modal_id = egui::Id::unique("unsaved_changes_modal");
                let mode = self.prefs.theme.resolve(ui.ctx());
                let modal = egui::Modal::new(modal_id)
                    .backdrop_color(coursecraft_ui_egui::modal_backdrop_color(mode))
                    .show(ui.ctx(), |ui| {
                        ui.set_max_width(480.0);
                        let name = self.project.file_name_display();
                        ui.heading(format!("Save changes to {name}?"));
                        ui.add_space(8.0);
                        ui.add(
                            egui::Label::new("Your changes will be lost if you don't save them.")
                                .wrap(),
                        );
                        ui.add_space(12.0);
                        ui.horizontal(|ui| {
                            if coursecraft_ui_egui::primary_button(ui, "Save").clicked()
                                && self.perform_save()
                                && let Some(action) = self.pending_action.take()
                            {
                                self.execute_pending_action(action, ui.ctx());
                            }
                            if ui.button("Don't Save").clicked()
                                && let Some(action) = self.pending_action.take()
                            {
                                self.execute_pending_action(action, ui.ctx());
                            }
                            if ui.button("Cancel").clicked() {
                                self.pending_action = None;
                            }
                        });
                    });
                if modal.should_close() || shell_modal_esc {
                    self.pending_action = None;
                }
            }

            // The file changed on disk since it was opened or saved.
            if let Some(prompt) = self.conflict_prompt.clone() {
                let modal_id = egui::Id::unique("changed_on_disk_modal");
                let mode = self.prefs.theme.resolve(ui.ctx());
                let mut done = false;
                let mut saved = false;
                let modal = egui::Modal::new(modal_id)
                    .backdrop_color(coursecraft_ui_egui::modal_backdrop_color(mode))
                    .show(ui.ctx(), |ui| {
                        ui.set_max_width(480.0);
                        let name = self.project.file_name_display();
                        if prompt.files.is_empty() {
                            ui.heading("This file changed since you opened it");
                            ui.add_space(8.0);
                            ui.add(
                                egui::Label::new(format!(
                                    "{name} was changed by another app or computer. Overwrite it with what you have here, or keep both by saving a copy."
                                ))
                                .wrap(),
                            );
                        } else {
                            ui.heading("This folder changed since you opened it");
                            ui.add_space(8.0);
                            ui.add(
                                egui::Label::new(changed_files_message(&name, &prompt.files))
                                    .wrap(),
                            );
                        }
                        ui.add_space(12.0);
                        ui.horizontal(|ui| {
                            if coursecraft_ui_egui::primary_button(ui, "Overwrite").clicked() {
                                saved = self.save_project_file(true);
                                done = true;
                            }
                            if ui.button("Save a copy").clicked() {
                                saved = if self.project.is_folder() {
                                    self.perform_save_as_folder()
                                } else {
                                    self.perform_save_as()
                                };
                                done = true;
                            }
                            if ui.button("Cancel").clicked() {
                                done = true;
                            }
                        });
                    });
                if done || modal.should_close() || shell_modal_esc {
                    self.conflict_prompt = None;
                    if saved && let Some(action) = prompt.then.clone() {
                        self.execute_pending_action(action, ui.ctx());
                    }
                }
            }

            // Restore autosave modal
            if let Some(prompt) = self.restore_prompt.clone() {
                let modal_id = egui::Id::unique("restore_autosave_modal");
                let mode = self.prefs.theme.resolve(ui.ctx());
                egui::Modal::new(modal_id)
                    .backdrop_color(coursecraft_ui_egui::modal_backdrop_color(mode))
                    .show(ui.ctx(), |ui| {
                        ui.set_max_width(480.0);
                        ui.heading("Restore unsaved changes?");
                        ui.add_space(8.0);
                        ui.add(
                            egui::Label::new(format!(
                                "Restore unsaved changes from {}?",
                                prompt.time_str
                            ))
                            .wrap(),
                        );
                        ui.add_space(12.0);
                        let mut dismiss = false;
                        ui.horizontal(|ui| {
                            if coursecraft_ui_egui::primary_button(ui, "Restore").clicked() {
                                self.restore_autosave(&prompt);
                                dismiss = true;
                            }
                            if ui.button("Discard").clicked() {
                                self.bump_save_epoch();
                                autosave::delete_untitled_autosave(&prompt.autosave_path);
                                autosave::delete_autosave(&prompt.autosave_path);
                                if let Some(ref file_path) = prompt.file_path {
                                    self.finish_open(file_path, Some(file_path.clone()), false);
                                }
                                dismiss = true;
                            }
                        });
                        if dismiss {
                            self.restore_prompt = None;
                        }
                    });
            }

            // Modal error dialog
            if let Some(ref msg) = self.error_modal.clone() {
                let modal_id = egui::Id::unique("error_modal");
                let mode = self.prefs.theme.resolve(ui.ctx());
                let modal = egui::Modal::new(modal_id)
                    .backdrop_color(coursecraft_ui_egui::modal_backdrop_color(mode))
                    .show(ui.ctx(), |ui| {
                        ui.set_max_width(480.0);
                        ui.heading("Error");
                        ui.add_space(8.0);
                        ui.add(egui::Label::new(msg).wrap());
                        ui.add_space(12.0);
                        ui.horizontal(|ui| {
                            if let Some(version) = self.open_latest_offer.clone()
                                && ui
                                    .button("Open latest version")
                                    .on_hover_text(
                                        "Open the newest kept version of this file as a copy",
                                    )
                                    .clicked()
                            {
                                self.error_modal = None;
                                self.open_latest_offer = None;
                                self.open_version_copy(&version);
                            }
                            if coursecraft_ui_egui::primary_button(ui, "OK").clicked() {
                                self.error_modal = None;
                                self.open_latest_offer = None;
                            }
                        });
                    });
                if modal.should_close() || shell_modal_esc {
                    self.error_modal = None;
                    self.open_latest_offer = None;
                }
            }
        }

        #[cfg(target_arch = "wasm32")]
        {
            while let Ok((name, bytes)) = self.project_open_rx.try_recv() {
                self.open_web_project_bytes(&name, &bytes);
            }

            while let Ok((name, bytes)) = self.document_rx.try_recv() {
                self.apply_document_file(&name, &bytes);
            }

            while let Ok((name, bytes)) = self.background_audio_rx.try_recv() {
                self.editor.import_background_audio(&name, &bytes);
            }

            while let Ok((name, bytes)) = self.import_rx.try_recv() {
                let lower = name.to_ascii_lowercase();
                if lower.ends_with(".pptx") {
                    let title = std::path::Path::new(&name)
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("Imported course");
                    self.finish_import_pptx(&bytes, title);
                } else if lower.ends_with(".ttf") || lower.ends_with(".otf") {
                    self.editor.import_font(&name, &bytes);
                } else if lower.ends_with(".mp3")
                    || lower.ends_with(".wav")
                    || lower.ends_with(".ogg")
                    || lower.ends_with(".m4a")
                {
                    self.editor.import_audio(&name, &bytes);
                } else if lower.ends_with(".mp4") || lower.ends_with(".webm") {
                    self.editor.import_video(&name, &bytes);
                } else if lower.ends_with(".lottie") || lower.ends_with(".json") {
                    self.editor.import_lottie(&name, &bytes);
                } else {
                    self.editor.import_image(&name, &bytes);
                }
            }

            let dropped_files = ui.ctx().input(|i| i.raw.dropped_files.clone());
            for file in dropped_files {
                let tx = self.import_tx.clone();
                let ctx = ui.ctx().clone();
                wasm_bindgen_futures::spawn_local(async move {
                    let path = file.path().to_path_buf();
                    let name = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("media")
                        .to_string();
                    if let Ok(bytes) = file.bytes_async().await {
                        let _ = tx.send((name, bytes));
                        ctx.request_repaint();
                    }
                });
            }

            for req in _requests {
                if self.web_unsaved.pending.is_some()
                    && matches!(
                        req,
                        EditorRequest::New
                            | EditorRequest::Open
                            | EditorRequest::Save
                            | EditorRequest::SaveAs
                            | EditorRequest::OpenExample(_)
                            | EditorRequest::OpenTemplate(_)
                            | EditorRequest::PickPptx
                    )
                {
                    continue;
                }
                match req {
                    EditorRequest::PickImage => {
                        let tx = self.import_tx.clone();
                        let ctx = ui.ctx().clone();
                        wasm_bindgen_futures::spawn_local(async move {
                            let file = rfd::AsyncFileDialog::new()
                                .add_filter(
                                    "Images (*.png, *.jpg, *.jpeg, *.webp, *.gif, *.svg, *.emf, *.wmf)",
                                    &["png", "jpg", "jpeg", "webp", "gif", "svg", "emf", "wmf"],
                                )
                                .pick_file()
                                .await;
                            if let Some(file) = file {
                                let name = file.file_name();
                                let bytes = file.read().await;
                                let _ = tx.send((name, bytes));
                                ctx.request_repaint();
                            }
                        });
                    }
                    EditorRequest::PickFonts => {
                        let tx = self.import_tx.clone();
                        let ctx = ui.ctx().clone();
                        wasm_bindgen_futures::spawn_local(async move {
                            let files = rfd::AsyncFileDialog::new()
                                .add_filter("Fonts (*.ttf, *.otf)", &["ttf", "otf"])
                                .pick_files()
                                .await;
                            if let Some(files) = files {
                                for file in files {
                                    let name = file.file_name();
                                    let bytes = file.read().await;
                                    let _ = tx.send((name, bytes));
                                }
                                ctx.request_repaint();
                            }
                        });
                    }
                    EditorRequest::PickAudio => {
                        let tx = self.import_tx.clone();
                        let ctx = ui.ctx().clone();
                        wasm_bindgen_futures::spawn_local(async move {
                            let file = rfd::AsyncFileDialog::new()
                                .add_filter(
                                    "Audio (*.mp3, *.wav, *.ogg, *.m4a)",
                                    &["mp3", "wav", "ogg", "m4a"],
                                )
                                .pick_file()
                                .await;
                            if let Some(file) = file {
                                let name = file.file_name();
                                let bytes = file.read().await;
                                let _ = tx.send((name, bytes));
                                ctx.request_repaint();
                            }
                        });
                    }
                    EditorRequest::PickBackgroundAudio => {
                        let tx = self.background_audio_tx.clone();
                        let ctx = ui.ctx().clone();
                        wasm_bindgen_futures::spawn_local(async move {
                            let file = rfd::AsyncFileDialog::new()
                                .add_filter(
                                    "Audio (*.mp3, *.wav, *.ogg, *.m4a)",
                                    &["mp3", "wav", "ogg", "m4a"],
                                )
                                .pick_file()
                                .await;
                            if let Some(file) = file {
                                let name = file.file_name();
                                let bytes = file.read().await;
                                let _ = tx.send((name, bytes));
                                ctx.request_repaint();
                            }
                        });
                    }
                    EditorRequest::PickVideo => {
                        let tx = self.import_tx.clone();
                        let ctx = ui.ctx().clone();
                        wasm_bindgen_futures::spawn_local(async move {
                            let file = rfd::AsyncFileDialog::new()
                                .add_filter("Video (*.mp4, *.webm)", &["mp4", "webm"])
                                .pick_file()
                                .await;
                            if let Some(file) = file {
                                let name = file.file_name();
                                let bytes = file.read().await;
                                let _ = tx.send((name, bytes));
                                ctx.request_repaint();
                            }
                        });
                    }
                    EditorRequest::PickLottie => {
                        let tx = self.import_tx.clone();
                        let ctx = ui.ctx().clone();
                        wasm_bindgen_futures::spawn_local(async move {
                            let file = rfd::AsyncFileDialog::new()
                                .add_filter("Lottie (*.json, *.lottie)", &["json", "lottie"])
                                .pick_file()
                                .await;
                            if let Some(file) = file {
                                let name = file.file_name();
                                let bytes = file.read().await;
                                let _ = tx.send((name, bytes));
                                ctx.request_repaint();
                            }
                        });
                    }
                    EditorRequest::PickPptx => {
                        self.guard_web_unsaved(web_state::WebPendingAction::ImportPptx, ui.ctx());
                    }
                    EditorRequest::PickDocument => {
                        let tx = self.document_tx.clone();
                        let ctx = ui.ctx().clone();
                        wasm_bindgen_futures::spawn_local(async move {
                            let file = rfd::AsyncFileDialog::new()
                                .add_filter(
                                    "Documents (*.docx, *.txt, *.md)",
                                    &["docx", "txt", "md"],
                                )
                                .pick_file()
                                .await;
                            if let Some(file) = file {
                                let name = file.file_name();
                                let bytes = file.read().await;
                                let _ = tx.send((name, bytes));
                                ctx.request_repaint();
                            }
                        });
                    }
                    EditorRequest::SetLanguage(setting) => {
                        self.editor.set_language(&setting);
                        ui.ctx().request_repaint();
                    }
                    EditorRequest::SetTheme(theme_pref) => {
                        self.editor.set_theme_pref(theme_pref);
                        let mode = theme_pref.resolve(ui.ctx());
                        coursecraft_ui_egui::coursecraft_render::apply_style(ui.ctx(), mode);
                    }
                    EditorRequest::New => {
                        self.guard_web_unsaved(web_state::WebPendingAction::New, ui.ctx());
                    }
                    EditorRequest::OpenExample(id) => {
                        self.guard_web_unsaved(
                            web_state::WebPendingAction::OpenExample(id),
                            ui.ctx(),
                        );
                    }
                    EditorRequest::OpenTemplate(id) => {
                        self.guard_web_unsaved(
                            web_state::WebPendingAction::OpenTemplate(id),
                            ui.ctx(),
                        );
                    }
                    EditorRequest::AiChat { purpose, request } => {
                        self.start_ai(purpose, request, ui.ctx());
                    }
                    EditorRequest::AiTranscribe {
                        purpose,
                        model,
                        file_name,
                        bytes,
                    } => {
                        self.start_ai_transcription(purpose, model, file_name, bytes, ui.ctx());
                    }
                    EditorRequest::AiSpeech {
                        purpose,
                        model,
                        voice,
                        text,
                    } => {
                        self.start_ai_speech(purpose, model, voice, text, ui.ctx());
                    }
                    EditorRequest::ElevenLabsSpeech { purpose, text } => {
                        self.start_elevenlabs_speech(purpose, text, ui.ctx());
                    }
                    EditorRequest::ElevenLabsVoices {
                        purpose,
                        page_token,
                    } => {
                        self.start_elevenlabs_voices(purpose, page_token, ui.ctx());
                    }
                    EditorRequest::ElevenLabsModels { purpose } => {
                        self.start_elevenlabs_models(purpose, ui.ctx());
                    }
                    EditorRequest::ElevenLabsVoiceLookup { purpose, voice_id } => {
                        self.start_elevenlabs_voice(purpose, voice_id, ui.ctx());
                    }
                    EditorRequest::CancelAi(purpose) => {
                        self.ai.cancel();
                        self.editor.set_ai_cancelled(purpose);
                    }
                    EditorRequest::SaveAiConfig(_)
                    | EditorRequest::SaveAiSource(_)
                    | EditorRequest::SaveElevenLabs(_) => {}
                    EditorRequest::AgentDetect | EditorRequest::AgentAction { .. } => {}
                    EditorRequest::ListSystemFonts => {
                        self.editor.ui.system_fonts = Some(Vec::new());
                    }
                    EditorRequest::AddSystemFont { .. } => {}
                    EditorRequest::Open => {
                        self.guard_web_unsaved(web_state::WebPendingAction::Open, ui.ctx());
                    }
                    EditorRequest::Save | EditorRequest::SaveAs => {
                        self.perform_web_save();
                    }
                    EditorRequest::Publish { standard } => {
                        if self.web_publish.is_none() {
                            self.editor.set_publish_busy();
                            self.web_publish = Some(WebPublishJob::Preparing { standard });
                            ui.ctx().request_repaint();
                        }
                    }
                    EditorRequest::SaveImage { .. } => {
                        self.editor.set_status("Saving works in the desktop app.");
                    }
                    _ => {}
                }
            }

            // Unsaved changes confirmation modal
            if self.web_unsaved.pending.is_some() {
                let modal_id = egui::Id::unique("web_unsaved_changes_modal");
                let mode = resolve_web_theme(ui.ctx());
                let modal = egui::Modal::new(modal_id)
                    .backdrop_color(coursecraft_ui_egui::modal_backdrop_color(mode))
                    .show(ui.ctx(), |ui| {
                        ui.set_max_width(480.0);
                        let name = self.web_unsaved.file_name_display().to_owned();
                        ui.heading(format!("Save changes to {name}?"));
                        ui.add_space(8.0);
                        ui.add(
                            egui::Label::new("Your changes will be lost if you don't save them.")
                                .wrap(),
                        );
                        ui.add_space(12.0);
                        ui.horizontal(|ui| {
                            if coursecraft_ui_egui::primary_button(ui, "Save").clicked()
                                && self.perform_web_save()
                                && let Some(action) = self.web_unsaved.take_pending()
                            {
                                self.execute_web_pending_action(action, ui.ctx());
                            }
                            if ui.button("Don't Save").clicked()
                                && let Some(action) = self.web_unsaved.take_pending()
                            {
                                self.execute_web_pending_action(action, ui.ctx());
                            }
                            if ui.button("Cancel").clicked() {
                                self.web_unsaved.cancel();
                            }
                        });
                    });
                if modal.should_close() || web_modal_esc {
                    self.web_unsaved.cancel();
                }
            }
        }
    }
}

/// Files the CLI's `publish` needs: put the embedded player in a temp folder and point at it.
#[cfg(not(target_arch = "wasm32"))]
fn cli_with_player(mut args: Vec<std::ffi::OsString>) -> std::process::ExitCode {
    let publishing = args.get(1).is_some_and(|a| a == "publish");
    if !publishing
        || args
            .iter()
            .any(|a| a.to_string_lossy().starts_with("--player"))
    {
        return coursecraft_cli::main_with_args(args);
    }
    let dir = std::env::temp_dir().join(format!("coursecraft-player-{}", std::process::id()));
    for file in player_bundle::player_files() {
        let path = dir.join(&file.path);
        let written = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&path, &file.bytes));
        if let Err(err) = written {
            eprintln!("error: {}: {err}", path.display());
            return std::process::ExitCode::FAILURE;
        }
    }
    args.push("--player".into());
    args.push(dir.clone().into_os_string());
    let code = coursecraft_cli::main_with_args(args);
    let _ = std::fs::remove_dir_all(&dir);
    code
}

#[cfg(not(target_arch = "wasm32"))]
fn main() -> std::process::ExitCode {
    let args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    // The update helper: a copy started by an update to install it once the app has exited.
    if let Some(done) =
        craft_update::self_update::finish(&updates::UPDATE_APP, args.get(1..).unwrap_or_default())
    {
        return match done {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("CourseCraft update helper failed: {e}");
                std::process::ExitCode::FAILURE
            }
        };
    }
    // `CourseCraft cli ...` and `CourseCraft mcp ...` run the agent tools with no window.
    let tool = args.get(1).and_then(|a| a.to_str()).map(str::to_owned);
    let tool_args = || {
        let name = format!("coursecraft {}", tool.as_deref().unwrap_or_default());
        std::iter::once(std::ffi::OsString::from(name))
            .chain(args.iter().skip(2).cloned())
            .collect::<Vec<_>>()
    };
    match tool.as_deref() {
        Some("cli") => return cli_with_player(tool_args()),
        Some("mcp") => return coursecraft_mcp::main_with_args(tool_args()),
        _ => {}
    }
    match run_editor_window() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn editor_viewport_builder() -> egui::ViewportBuilder {
    #[cfg_attr(target_os = "macos", allow(unused_mut))]
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("CourseCraft")
        .with_inner_size([1440.0, 900.0])
        .with_min_inner_size([1024.0, 600.0]);
    // The window and taskbar icon on Windows and Linux. macOS takes the Dock icon from
    // the bundle's .icns, which has the system's padding; setting this would replace it.
    #[cfg(not(target_os = "macos"))]
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../icons/window-256.png")) {
        viewport = viewport.with_icon(icon);
    }
    viewport
}

#[cfg(any(target_arch = "wasm32", test))]
fn resolve_web_theme(ctx: &egui::Context) -> coursecraft_ui_egui::coursecraft_render::ThemeMode {
    coursecraft_ui_egui::coursecraft_render::ThemePref::System.resolve(ctx)
}

#[cfg(not(target_arch = "wasm32"))]
fn parse_launch_args<I, S>(args: I) -> (bool, Option<PathBuf>)
where
    I: IntoIterator<Item = S>,
    S: Into<std::ffi::OsString>,
{
    let mut blank = false;
    let mut initial_path = None;
    for arg in args.into_iter().skip(1) {
        let os_arg: std::ffi::OsString = arg.into();
        if os_arg.is_empty() {
            continue;
        }
        let text = os_arg.to_string_lossy();
        if text == "--blank" {
            blank = true;
        } else if !text.starts_with("--") && initial_path.is_none() {
            initial_path = Some(PathBuf::from(os_arg));
        }
    }
    (blank, initial_path)
}

#[cfg(not(target_arch = "wasm32"))]
fn run_editor_window() -> eframe::Result {
    let (blank, initial_path) = parse_launch_args(std::env::args_os());

    let config_dir = prefs::config_dir_from_env();
    crash::install_panic_hook(config_dir.clone());

    let (course, media) = initial_course_and_media(blank);

    let viewport = editor_viewport_builder();
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "CourseCraft",
        options,
        Box::new(move |cc| {
            coursecraft_ui_egui::install_fonts(&cc.egui_ctx);
            let mut app = EditorApp::new_desktop(course, config_dir);
            app.editor.set_media(media);
            let mode = app.prefs.theme.resolve(&cc.egui_ctx);
            coursecraft_ui_egui::coursecraft_render::apply_style(&cc.egui_ctx, mode);
            if let Ok(mut slot) = app.repaint_ctx.lock() {
                *slot = Some(cc.egui_ctx.clone());
            }
            app.startup(initial_path, blank);
            Ok(Box::new(app))
        }),
    )
}

#[cfg(any(target_arch = "wasm32", test))]
fn should_prevent_browser_file_shortcut(
    ctrl_or_meta: bool,
    alt: bool,
    shift: bool,
    key: &str,
) -> bool {
    if !ctrl_or_meta || alt {
        return false;
    }
    match key.to_ascii_lowercase().as_str() {
        "s" => true,
        "o" | "n" => !shift,
        _ => false,
    }
}

#[cfg(target_arch = "wasm32")]
fn install_web_file_shortcut_guard(window: &web_sys::Window) {
    use eframe::wasm_bindgen::{JsCast as _, closure::Closure};

    let listener = Closure::<dyn FnMut(web_sys::Event)>::new(|event: web_sys::Event| {
        if let Ok(kb) = event.dyn_into::<web_sys::KeyboardEvent>()
            && should_prevent_browser_file_shortcut(
                kb.ctrl_key() || kb.meta_key(),
                kb.alt_key(),
                kb.shift_key(),
                &kb.key(),
            )
        {
            kb.prevent_default();
        }
    });
    let target: &web_sys::EventTarget = window.as_ref();
    let _ = target.add_event_listener_with_callback("keydown", listener.as_ref().unchecked_ref());
    listener.forget();
}

#[cfg(target_arch = "wasm32")]
fn main() {
    use eframe::wasm_bindgen::JsCast as _;

    wasm_bindgen_futures::spawn_local(async {
        let window = web_sys::window();
        if let Some(ref w) = window {
            install_web_file_shortcut_guard(w);
        }
        let blank = window
            .as_ref()
            .and_then(|w| w.location().search().ok())
            .is_some_and(|q| q.contains("blank"));
        let canvas = window
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("coursecraft_canvas"))
            .and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok());
        let Some(canvas) = canvas else {
            web_sys::console::error_1(&"coursecraft: canvas #coursecraft_canvas not found".into());
            return;
        };

        let (course, media) = initial_course_and_media(blank);
        let mut editor = Editor::new(course);
        let low_memory = web_sys::window().is_some_and(|window| {
            js_sys::Reflect::get(
                window.as_ref(),
                &eframe::wasm_bindgen::JsValue::from_str("navigator"),
            )
            .ok()
            .filter(|nav| !nav.is_null() && !nav.is_undefined())
            .and_then(|nav| {
                js_sys::Reflect::get(
                    &nav,
                    &eframe::wasm_bindgen::JsValue::from_str("deviceMemory"),
                )
                .ok()
            })
            .and_then(|v| v.as_f64())
            .is_some_and(|gb| gb.is_finite() && gb > 0.0 && gb <= 4.0)
        });
        editor.set_low_memory_device(low_memory);
        editor.set_media(media);
        editor.set_publish_available(player_bundle::has_player());
        editor.set_coach_marks_done(false);
        if !blank {
            editor.ui.show_welcome = true;
            editor.ui.has_loaded_course = false;
        }

        let (import_tx, import_rx) = std::sync::mpsc::channel();
        let (background_audio_tx, background_audio_rx) = std::sync::mpsc::channel();
        let (document_tx, document_rx) = std::sync::mpsc::channel();
        let (project_open_tx, project_open_rx) = std::sync::mpsc::channel();
        let web_unsaved = web_state::WebUnsavedState::new(editor.session.revision);
        if let Err(e) = eframe::WebRunner::new()
            .start(
                canvas,
                eframe::WebOptions::default(),
                Box::new(move |cc| {
                    coursecraft_ui_egui::install_fonts(&cc.egui_ctx);
                    coursecraft_ui_egui::coursecraft_render::apply_style(
                        &cc.egui_ctx,
                        resolve_web_theme(&cc.egui_ctx),
                    );
                    Ok(Box::new(EditorApp {
                        editor,
                        ai: ai::AiClient::new(),
                        import_tx,
                        import_rx,
                        background_audio_tx,
                        background_audio_rx,
                        document_tx,
                        document_rx,
                        project_open_tx,
                        project_open_rx,
                        web_unsaved,
                        web_publish: None,
                    }))
                }),
            )
            .await
        {
            web_sys::console::error_1(&e);
        }
    });
}

#[cfg(target_arch = "wasm32")]
impl EditorApp {
    fn step_web_publish(&mut self, ctx: &egui::Context) {
        const CHUNK_BYTES: usize = 256 * 1024;
        let Some(job) = self.web_publish.take() else {
            return;
        };
        match job {
            WebPublishJob::Preparing { standard } => {
                let pkg_standard = publish::package_standard(standard);
                let player = player_bundle::player_files();
                match publish::package_files_from(
                    &self.editor.session.course,
                    &self.editor.media,
                    pkg_standard,
                    &player,
                ) {
                    Ok(files) => {
                        self.editor.set_publish_progress(Some(0.0));
                        self.web_publish = Some(WebPublishJob::Zipping {
                            standard,
                            zipper: coursecraft_publish::PackageZipper::new(files),
                        });
                        ctx.request_repaint();
                    }
                    Err(err) => {
                        self.editor.set_publish_failure(err);
                    }
                }
            }
            WebPublishJob::Zipping {
                standard,
                mut zipper,
            } => match zipper.step(CHUNK_BYTES) {
                Ok(coursecraft_publish::ZipStep::InProgress {
                    bytes_done,
                    bytes_total,
                }) => {
                    let progress = if bytes_total == 0 {
                        1.0
                    } else {
                        bytes_done as f32 / bytes_total as f32
                    };
                    self.editor.set_publish_progress(Some(progress));
                    self.web_publish = Some(WebPublishJob::Zipping { standard, zipper });
                    ctx.request_repaint();
                }
                Ok(coursecraft_publish::ZipStep::Done(bytes)) => {
                    let stem = sanitize_filename_stem(&self.editor.session.course.title, "course");
                    let file_name = format!("{stem}-{}.zip", standard.file_stem());
                    match web_download_bytes(&file_name, &bytes, "application/zip") {
                        Ok(()) => self
                            .editor
                            .set_publish_success(file_name, bytes.len() as u64),
                        Err(err) => self
                            .editor
                            .set_publish_failure(format!("Couldn't download {file_name}: {err}")),
                    }
                }
                Err(err) => {
                    self.editor
                        .set_publish_failure(format!("Couldn't create the zip file: {err}"));
                }
            },
        }
    }

    fn guard_web_unsaved(&mut self, action: web_state::WebPendingAction, ctx: &egui::Context) {
        let rev = self.editor.session.revision;
        if let Some(ready) = self.web_unsaved.request_or_run(action, rev) {
            self.execute_web_pending_action(ready, ctx);
        }
    }

    fn execute_web_pending_action(
        &mut self,
        action: web_state::WebPendingAction,
        ctx: &egui::Context,
    ) {
        match action {
            web_state::WebPendingAction::New => {
                let course = Session::blank("Untitled course").course;
                self.editor.replace_course(course);
                self.web_unsaved.reset_for_new(self.editor.session.revision);
            }
            web_state::WebPendingAction::Open => {
                let tx = self.project_open_tx.clone();
                let ctx = ctx.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    let file = rfd::AsyncFileDialog::new()
                        .add_filter(
                            "CourseCraft Project (*.ccraft, *.json)",
                            &["ccraft", "json"],
                        )
                        .pick_file()
                        .await;
                    if let Some(file) = file {
                        let name = file.file_name();
                        let bytes = file.read().await;
                        let _ = tx.send((name, bytes));
                        ctx.request_repaint();
                    }
                });
            }
            web_state::WebPendingAction::OpenExample(id) => {
                self.editor.open_example(id);
                self.web_unsaved.reset_for_new(self.editor.session.revision);
            }
            web_state::WebPendingAction::OpenTemplate(id) => {
                self.editor.open_template(id);
                self.web_unsaved.reset_for_new(self.editor.session.revision);
            }
            web_state::WebPendingAction::ImportPptx => {
                let tx = self.import_tx.clone();
                let ctx = ctx.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    let file = rfd::AsyncFileDialog::new()
                        .add_filter("PowerPoint (*.pptx)", &["pptx"])
                        .pick_file()
                        .await;
                    if let Some(file) = file {
                        let name = file.file_name();
                        let bytes = file.read().await;
                        let _ = tx.send((name, bytes));
                        ctx.request_repaint();
                    }
                });
            }
        }
    }

    fn perform_web_save(&mut self) -> bool {
        let media = self.editor.used_media();
        match coursecraft_format::save_project(&self.editor.session.course, &media) {
            Ok(bytes) => {
                let stem = sanitize_filename_stem(&self.editor.session.course.title, "Untitled");
                let file_name = format!("{stem}.ccraft");
                match web_download_bytes(&file_name, &bytes, "application/octet-stream") {
                    Ok(()) => {
                        self.web_unsaved
                            .mark_saved_as(self.editor.session.revision, file_name.clone());
                        self.editor.set_status(format!("Downloaded {file_name}"));
                        true
                    }
                    Err(err) => {
                        self.editor
                            .set_status(format!("Couldn't download {file_name}: {err}"));
                        false
                    }
                }
            }
            Err(err) => {
                self.editor
                    .set_status(format!("Couldn't save project: {err}"));
                false
            }
        }
    }

    fn open_web_project_bytes(&mut self, name: &str, bytes: &[u8]) {
        let lower = name.to_ascii_lowercase();
        if lower.ends_with(".ccraft") {
            match coursecraft_format::open_project(bytes) {
                Ok(project) => {
                    self.editor.replace_course(project.course);
                    self.editor.set_media(into_arc_media(project.media));
                    self.web_unsaved
                        .mark_saved_as(self.editor.session.revision, name.to_owned());
                    self.editor.set_status(format!("Opened {name}"));
                }
                Err(err) => {
                    self.editor
                        .set_status(format!("Couldn't open {name}: {err}"));
                }
            }
        } else if lower.ends_with(".json") {
            match std::str::from_utf8(bytes)
                .map_err(|e| e.to_string())
                .and_then(|text| Course::from_json(text).map_err(|e| e.to_string()))
            {
                Ok(course) => {
                    self.editor.replace_course(course);
                    self.editor.set_media(Vec::new());
                    self.web_unsaved
                        .mark_saved_as(self.editor.session.revision, name.to_owned());
                    self.editor.set_status(format!("Opened {name}"));
                }
                Err(err) => {
                    self.editor
                        .set_status(format!("Couldn't open {name}: {err}"));
                }
            }
        } else {
            self.editor
                .set_status("Choose a .ccraft or course.json file to open.");
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn sanitize_filename_stem(title: &str, fallback: &str) -> String {
    let sanitized = title
        .replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_")
        .trim()
        .to_string();
    if sanitized.is_empty() {
        fallback.to_string()
    } else {
        sanitized
    }
}

#[cfg(target_arch = "wasm32")]
fn web_download_bytes(file_name: &str, bytes: &[u8], mime: &str) -> Result<(), String> {
    use eframe::wasm_bindgen::JsCast as _;

    let window = web_sys::window().ok_or_else(|| "no browser window".to_string())?;
    let document = window
        .document()
        .ok_or_else(|| "no browser document".to_string())?;
    let body = document
        .body()
        .ok_or_else(|| "no document body".to_string())?;

    let array = js_sys::Uint8Array::from(bytes);
    let parts = js_sys::Array::of1(&array.into());
    let options = web_sys::BlobPropertyBag::new();
    options.set_type(mime);
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &options)
        .map_err(|e| format!("Blob failed: {e:?}"))?;
    let url = web_sys::Url::create_object_url_with_blob(&blob)
        .map_err(|e| format!("Object URL failed: {e:?}"))?;

    let anchor_el = document
        .create_element("a")
        .map_err(|e| format!("create_element failed: {e:?}"))?;
    let anchor = anchor_el
        .dyn_into::<web_sys::HtmlAnchorElement>()
        .map_err(|_| "created element is not an anchor".to_string())?;
    anchor.set_href(&url);
    anchor.set_download(file_name);
    let _ = anchor.style().set_property("display", "none");
    let _ = body.append_child(&anchor);
    anchor.click();
    let _ = body.remove_child(&anchor);
    let _ = web_sys::Url::revoke_object_url(&url);
    Ok(())
}

fn into_arc_media(media: Vec<(String, Vec<u8>)>) -> Vec<(String, std::sync::Arc<[u8]>)> {
    media
        .into_iter()
        .map(|(k, v)| (k, std::sync::Arc::from(v)))
        .collect()
}
