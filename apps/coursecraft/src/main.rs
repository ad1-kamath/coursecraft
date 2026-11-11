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
pub mod player_bundle;
#[cfg(not(target_arch = "wasm32"))]
pub mod prefs;
#[cfg(not(target_arch = "wasm32"))]
pub mod publish;
#[cfg(not(target_arch = "wasm32"))]
pub mod versions;

#[cfg(test)]
mod files_tests;
#[cfg(test)]
mod tests;
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

const HELLO_COURSE: &str = include_str!("../../../examples/hello-course/course.json");

fn initial_course(blank: bool) -> Course {
    let blank_course = || Session::blank("Untitled course").course;
    if blank {
        return blank_course();
    }
    Course::from_json(HELLO_COURSE).unwrap_or_else(|_| blank_course())
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone, PartialEq, Eq)]
enum PendingAction {
    New,
    Open,
    /// Pick a project folder to open.
    OpenFolder,
    OpenRecent(usize),
    OpenExample(coursecraft_ui_egui::ExampleId),
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

struct EditorApp {
    editor: Editor,
    /// One background AI call at a time, on both desktop and web.
    ai: ai::AiClient,
    /// One background agent check or action at a time, on the desktop.
    #[cfg(not(target_arch = "wasm32"))]
    agents: agents::AgentClient,
    #[cfg(not(target_arch = "wasm32"))]
    project: Project,
    #[cfg(not(target_arch = "wasm32"))]
    prefs: Prefs,
    #[cfg(not(target_arch = "wasm32"))]
    config_dir: Option<PathBuf>,
    #[cfg(not(target_arch = "wasm32"))]
    autosaver: Autosaver,
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
    /// Last file written by a successful publish, for "Show in folder".
    #[cfg(not(target_arch = "wasm32"))]
    published_path: Option<PathBuf>,
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

/// Restore prompt for a leftover untitled autosave in `config_dir`, if any.
#[cfg(not(target_arch = "wasm32"))]
fn untitled_restore_prompt(config_dir: Option<&Path>) -> Option<RestorePrompt> {
    let auto_path = autosave::autosave_path_for_project(None, config_dir?);
    if !auto_path.exists() {
        return None;
    }
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
        editor.set_publish_available(player_bundle::has_player());
        editor.set_automation_available(true);
        editor.set_agent_exe_path(
            std::env::current_exe()
                .ok()
                .map(|path| path.display().to_string()),
        );
        editor.set_automation_enabled(prefs.automation_enabled);
        editor.set_theme_pref(prefs.theme.into());
        editor.set_coach_marks_done(prefs.coach_marks_done);
        editor.set_preferences(prefs.autosave_minutes_clamped(), prefs.default_slide.size());
        editor.set_versions_to_keep(prefs.versions_to_keep_clamped());
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

        Self {
            editor,
            ai: ai::AiClient::new(),
            agents: agents::AgentClient::new(),
            project,
            prefs,
            config_dir,
            autosaver: Autosaver {
                interval_minutes: Some(autosave_minutes),
                ..Autosaver::new()
            },
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
            published_path: None,
            #[cfg(test)]
            test_export_path: None,
            #[cfg(test)]
            test_picked_audio: None,
            #[cfg(test)]
            test_picked_media_files: None,
            #[cfg(test)]
            test_picked_media_file: None,
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
                "Media (*.png, *.jpg, *.jpeg, *.webp, *.gif, *.svg, *.mp3, *.wav, *.ogg, *.m4a, *.mp4, *.webm, *.json, *.lottie, *.vtt, *.ttf, *.otf)",
                &[
                    "png", "jpg", "jpeg", "webp", "gif", "svg", "mp3", "wav", "ogg", "m4a", "mp4",
                    "webm", "json", "lottie", "vtt", "ttf", "otf",
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
                "Media (*.png, *.jpg, *.jpeg, *.webp, *.gif, *.svg, *.mp3, *.wav, *.ogg, *.m4a, *.mp4, *.webm, *.json, *.lottie, *.vtt, *.ttf, *.otf)",
                &[
                    "png", "jpg", "jpeg", "webp", "gif", "svg", "mp3", "wav", "ogg", "m4a", "mp4",
                    "webm", "json", "lottie", "vtt", "ttf", "otf",
                ],
            )
            .pick_file()
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn import_media_files(&mut self) {
        let mut added = 0usize;
        for path in self.pick_media_files() {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("media")
                .to_string();
            if let Ok(bytes) = std::fs::read(&path)
                && self.editor.add_media_bytes(&name, &bytes).is_some()
            {
                added = added.saturating_add(1);
            }
        }
        if added > 0 {
            self.editor.set_status(format!(
                "Imported {added} {}. File operations are not undoable.",
                if added == 1 { "file" } else { "files" }
            ));
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
                self.restore_prompt = untitled_restore_prompt(self.config_dir.as_deref());
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
        let saved = if let Some(mut record) = self.project.folder.take() {
            // A folder is kept in git, so there is no version history for it.
            let result =
                files::save_folder_to(&path, &self.editor.session.course, &media, &mut record);
            self.project.folder = Some(record);
            result.map(|_| ())
        } else {
            self.keep_version(&path);
            files::save_to(&path, &self.editor.session.course, &media)
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
                    autosave::delete_autosave(&untitled_path);
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
                let media_arc: Vec<(String, std::sync::Arc<[u8]>)> = media
                    .iter()
                    .map(|(k, v)| (k.clone(), std::sync::Arc::from(v.as_slice())))
                    .collect();
                self.editor.replace_course(course);
                self.editor.set_media(media_arc);
                self.project = Project::new(self.editor.session.revision);
                // Unsaved until the person saves it under a name.
                self.project.saved_revision = u64::MAX;
                self.autosaver = Autosaver::new();
                self.autosaver.interval_minutes = Some(self.prefs.autosave_minutes_clamped());
            }
            Err(err) => self.error_modal = Some(err),
        }
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
        let title = &self.editor.session.course.title;
        let Some(path) = files::pick_save_folder(title) else {
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
        self.project.path = Some(path);
        self.project.disk_stamp = None;
        self.project.folder = Some(coursecraft_format::FolderRecord::new());
        self.perform_save()
    }

    fn perform_save_as(&mut self) -> bool {
        let title = &self.editor.session.course.title;
        let Some(path) = files::pick_save_file(title) else {
            return false;
        };
        self.project.folder = None;
        self.project.path = Some(path);
        // A new path has no earlier stamp; the file dialog already confirmed
        // replacing a file that is there.
        self.project.disk_stamp = None;
        self.perform_save()
    }

    fn finish_open(&mut self, path_to_read: &Path, final_path: Option<PathBuf>, is_restored: bool) {
        let opened = if path_to_read.is_dir() {
            files::open_folder_from(path_to_read).map(|(loaded, record)| (loaded, Some(record)))
        } else {
            files::open_from(path_to_read).map(|loaded| (loaded, None))
        };
        match opened {
            Ok(((course, media), record)) => {
                let media_arc: Vec<(String, std::sync::Arc<[u8]>)> = media
                    .iter()
                    .map(|(k, v)| (k.clone(), std::sync::Arc::from(v.as_slice())))
                    .collect();
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
                self.autosaver = Autosaver::new();
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
                self.autosaver = Autosaver::new();
                self.autosaver.interval_minutes = Some(self.prefs.autosave_minutes_clamped());
            }
            PendingAction::Open => {
                if let Some(path) = files::pick_open_file() {
                    self.initiate_open(path);
                }
            }
            PendingAction::OpenFolder => {
                if let Some(path) = files::pick_project_folder() {
                    self.initiate_open(path);
                }
            }
            PendingAction::OpenRecent(idx) => {
                self.perform_open_recent(idx);
            }
            PendingAction::OpenExample(id) => {
                self.editor.open_example(id);
                self.project = Project::new(self.editor.session.revision);
                self.autosaver = Autosaver::new();
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
        let current_time = ctx.input(|i| i.time);
        let is_dirty = self.project.is_dirty(&self.editor.session);
        let rev = self.editor.session.revision;
        let editor = &self.editor;
        self.autosaver.maybe_autosave(
            current_time,
            is_dirty,
            rev,
            autosave::AutosaveTarget {
                project_path: self.project.path.as_deref(),
                config_dir: self.config_dir.as_deref(),
                course: &self.editor.session.course,
            },
            || editor.used_media(),
        );
        if is_dirty && let Some(wait) = self.autosaver.secs_until_next(current_time) {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(wait + 0.5));
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
                    self.editor.media.push((name, bytes.into()));
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
    fn handle_request(&mut self, req: EditorRequest, ctx: &egui::Context) {
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
                if let Some(path) = files::pick_image_file() {
                    let name = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("image.png")
                        .to_string();
                    if let Ok(bytes) = std::fs::read(&path) {
                        self.editor.import_image(&name, &bytes);
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
                    if let Ok(bytes) = std::fs::read(&path) {
                        self.editor.import_video(&name, &bytes);
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
                    if let Ok(bytes) = std::fs::read(&path) {
                        self.editor.import_captions(&name, &bytes);
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
                self.start_publish(standard);
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
            } => {
                self.prefs.autosave_minutes = autosave_minutes.min(prefs::MAX_AUTOSAVE_MINUTES);
                self.prefs.versions_to_keep = versions_to_keep.min(prefs::MAX_VERSIONS_TO_KEEP);
                if let Some(preset) = prefs::SlidePreset::from_size(default_slide) {
                    self.prefs.default_slide = preset;
                }
                self.autosaver.interval_minutes = Some(self.prefs.autosave_minutes_clamped());
                if let Some(ref dir) = self.config_dir {
                    let _ = prefs::save_prefs(dir, &self.prefs);
                }
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
            if let Ok(bytes) = std::fs::read(path) {
                let lower = name.to_ascii_lowercase();
                if lower.ends_with(".pptx") {
                    self.import_pptx_file(path);
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
    fn start_publish(&mut self, standard: coursecraft_ui_egui::PublishStandard) {
        if self.publish_rx.is_some() {
            return;
        }
        let title = self.editor.session.course.title.clone();
        let Some(path) = files::pick_publish_save_file(&title, standard.file_stem()) else {
            return;
        };
        let media: Vec<files::MediaItem> = self
            .editor
            .media()
            .iter()
            .map(|(name, bytes)| (name.clone(), bytes.to_vec()))
            .collect();
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
        std::thread::spawn(move || {
            let _ = tx.send(publish::run_publish_job(job));
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
                self.editor.set_publish_success(name, outcome.bytes);
            }
            Ok(Err(message)) => {
                self.editor.set_publish_failure(message);
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                self.publish_rx = Some(rx);
                ctx.request_repaint();
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.editor
                    .set_publish_failure("Publishing stopped unexpectedly. Try again.");
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
        let media: Vec<files::MediaItem> = self
            .editor
            .media()
            .iter()
            .map(|(name, bytes)| (name.clone(), bytes.to_vec()))
            .collect();
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
        let media: Vec<files::MediaItem> = self
            .editor
            .media()
            .iter()
            .map(|(name, bytes)| (name.clone(), bytes.to_vec()))
            .collect();
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
                    ctx.request_repaint();
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
                ctx.request_repaint();
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
            Ok(imported) => {
                self.editor.replace_course(imported.course);
                let media_arc: Vec<(String, std::sync::Arc<[u8]>)> = imported
                    .media
                    .iter()
                    .map(|(k, v)| (k.clone(), std::sync::Arc::from(v.as_slice())))
                    .collect();
                self.editor.set_media(media_arc);
                #[cfg(not(target_arch = "wasm32"))]
                {
                    self.project = Project::new(self.editor.session.revision);
                    // Mark dirty so closing prompts to save
                    self.project.saved_revision = u64::MAX;
                    self.autosaver = Autosaver::new();
                }
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

#[cfg(not(target_arch = "wasm32"))]
struct EditorHostAdapter<'a> {
    editor: &'a mut Editor,
    project: &'a mut Project,
    config_dir: Option<&'a Path>,
    prefs: &'a mut Prefs,
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
        let is_ccraft = target
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("ccraft"));
        let is_project_folder = target
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case(coursecraft_format::FOLDER_EXTENSION));

        if is_project_folder {
            // Keep the record when saving over the folder that is already open.
            let same = self.project.path.as_deref() == Some(target.as_path());
            let mut record = self
                .project
                .folder
                .take()
                .filter(|_| same)
                .unwrap_or_default();
            let result =
                files::save_folder_to(&target, &self.editor.session.course, &media, &mut record);
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
            files::save_to(&target, &self.editor.session.course, &media)
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
            autosave::delete_autosave(&untitled_path);
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
                let media_arc: Vec<(String, std::sync::Arc<[u8]>)> = media
                    .iter()
                    .map(|(k, v)| (k.clone(), std::sync::Arc::from(v.as_slice())))
                    .collect();
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
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
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

            // 2b. Background publish results
            self.poll_publish(ui.ctx());
            self.poll_export(ui.ctx());
            self.poll_export_review(ui.ctx());

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
                };
                if server.poll(&mut host) > 0 {
                    ui.ctx().request_repaint();
                }
            }
        }

        // 4. Editor main UI
        let _requests = self.editor.show(ui);

        #[cfg(not(target_arch = "wasm32"))]
        {
            for req in _requests {
                self.handle_request(req, ui.ctx());
            }

            self.handle_dropped_files(ui.ctx());

            // Unsaved changes confirmation modal
            if self.pending_action.is_some() {
                let modal_id = egui::Id::unique("unsaved_changes_modal");
                egui::Modal::new(modal_id).show(ui.ctx(), |ui| {
                    let name = self.project.file_name_display();
                    ui.heading(format!("Save changes to {name}?"));
                    ui.add_space(8.0);
                    ui.label("Your changes will be lost if you don't save them.");
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked()
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
            }

            // The file changed on disk since it was opened or saved.
            if let Some(prompt) = self.conflict_prompt.clone() {
                let modal_id = egui::Id::unique("changed_on_disk_modal");
                egui::Modal::new(modal_id).show(ui.ctx(), |ui| {
                    let name = self.project.file_name_display();
                    if prompt.files.is_empty() {
                        ui.heading("This file changed since you opened it");
                        ui.add_space(8.0);
                        ui.label(format!(
                            "{name} was changed by another app or computer. Overwrite it with what you have here, or keep both by saving a copy."
                        ));
                    } else {
                        ui.heading("This folder changed since you opened it");
                        ui.add_space(8.0);
                        ui.label(changed_files_message(&name, &prompt.files));
                    }
                    ui.add_space(12.0);
                    let mut done = false;
                    let mut saved = false;
                    ui.horizontal(|ui| {
                        if ui.button("Overwrite").clicked() {
                            saved = self.save_project_file(true);
                            done = true;
                        }
                        if ui.button("Save a Copy").clicked() {
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
                    if done {
                        self.conflict_prompt = None;
                        if saved && let Some(action) = prompt.then.clone() {
                            self.execute_pending_action(action, ui.ctx());
                        }
                    }
                });
            }

            // Restore autosave modal
            if let Some(prompt) = self.restore_prompt.clone() {
                let modal_id = egui::Id::unique("restore_autosave_modal");
                egui::Modal::new(modal_id).show(ui.ctx(), |ui| {
                    ui.heading("Restore unsaved changes?");
                    ui.add_space(8.0);
                    ui.label(format!("Restore unsaved changes from {}?", prompt.time_str));
                    ui.add_space(12.0);
                    let mut dismiss = false;
                    ui.horizontal(|ui| {
                        if ui.button("Restore").clicked() {
                            self.finish_open(&prompt.autosave_path, prompt.file_path.clone(), true);
                            dismiss = true;
                        }
                        if ui.button("Discard").clicked() {
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
                egui::Modal::new(modal_id).show(ui.ctx(), |ui| {
                    ui.heading("Error");
                    ui.add_space(8.0);
                    ui.label(msg);
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if let Some(version) = self.open_latest_offer.clone()
                            && ui
                                .button("Open Latest Version")
                                .on_hover_text(
                                    "Open the newest kept version of this file as a copy",
                                )
                                .clicked()
                        {
                            self.error_modal = None;
                            self.open_latest_offer = None;
                            self.open_version_copy(&version);
                        }
                        if ui.button("OK").clicked() {
                            self.error_modal = None;
                            self.open_latest_offer = None;
                        }
                    });
                });
            }
        }

        #[cfg(target_arch = "wasm32")]
        {
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
                match req {
                    EditorRequest::PickImage => {
                        let tx = self.import_tx.clone();
                        let ctx = ui.ctx().clone();
                        wasm_bindgen_futures::spawn_local(async move {
                            let file = rfd::AsyncFileDialog::new()
                                .add_filter(
                                    "Images (*.png, *.jpg, *.jpeg, *.webp, *.gif, *.svg)",
                                    &["png", "jpg", "jpeg", "webp", "gif", "svg"],
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
                        let tx = self.import_tx.clone();
                        let ctx = ui.ctx().clone();
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
                    EditorRequest::SetTheme(theme_pref) => {
                        self.editor.set_theme_pref(theme_pref);
                        let mode = theme_pref.resolve(ui.ctx());
                        coursecraft_ui_egui::coursecraft_render::apply_style(ui.ctx(), mode);
                    }
                    EditorRequest::New => {
                        let course = Session::blank("Untitled course").course;
                        self.editor.replace_course(course);
                    }
                    EditorRequest::OpenExample(id) => {
                        self.editor.open_example(id);
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
                    EditorRequest::SaveImage { .. } => {
                        self.editor.set_status("Saving works in the desktop app.");
                    }
                    _ => {}
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
    // `CourseCraft cli ...` and `CourseCraft mcp ...` run the agent tools with no window.
    let args: Vec<std::ffi::OsString> = std::env::args_os().collect();
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
fn run_editor_window() -> eframe::Result {
    let blank = std::env::args().any(|a| a == "--blank");
    let initial_path = std::env::args()
        .skip(1)
        .find(|a| !a.starts_with("--"))
        .map(PathBuf::from);

    let config_dir = prefs::config_dir_from_env();
    crash::install_panic_hook(config_dir.clone());

    let course = initial_course(blank);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("CourseCraft")
            .with_inner_size([1440.0, 900.0]),
        ..Default::default()
    };
    eframe::run_native(
        "CourseCraft",
        options,
        Box::new(move |cc| {
            coursecraft_ui_egui::install_fonts(&cc.egui_ctx);
            let mut app = EditorApp::new_desktop(course, config_dir);
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

#[cfg(target_arch = "wasm32")]
fn main() {
    use eframe::wasm_bindgen::JsCast as _;

    wasm_bindgen_futures::spawn_local(async {
        let window = web_sys::window();
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

        let mut editor = Editor::new(initial_course(blank));
        editor.set_file_ops_available(false);
        editor.set_publish_available(false);
        editor.set_coach_marks_done(false);
        if !blank {
            editor.ui.show_welcome = true;
            editor.ui.has_loaded_course = false;
        }

        let (import_tx, import_rx) = std::sync::mpsc::channel();
        let (background_audio_tx, background_audio_rx) = std::sync::mpsc::channel();
        let (document_tx, document_rx) = std::sync::mpsc::channel();
        if let Err(e) = eframe::WebRunner::new()
            .start(
                canvas,
                eframe::WebOptions::default(),
                Box::new(move |cc| {
                    coursecraft_ui_egui::install_fonts(&cc.egui_ctx);
                    coursecraft_ui_egui::coursecraft_render::apply_style(
                        &cc.egui_ctx,
                        coursecraft_ui_egui::coursecraft_render::ThemeMode::Dark,
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
                    }))
                }),
            )
            .await
        {
            web_sys::console::error_1(&e);
        }
    });
}
