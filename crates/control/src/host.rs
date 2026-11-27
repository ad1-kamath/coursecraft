//! Control channel host trait and headless host implementation.

use std::path::{Path, PathBuf};

use coursecraft_course::Course;
use coursecraft_engine::{EngineError, Session, registry};
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

/// Information about a registered engine command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandInfo {
    pub id: String,
    pub label: String,
    pub menu: String,
    pub params: String,
    pub mutates: bool,
}

/// Current selection in the editor or headless session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Selection {
    pub slide: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object: Option<String>,
}

/// Metadata and dirty/undo state for the current session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionInfo {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub dirty: bool,
    pub revision: u64,
    pub can_undo: bool,
    pub can_redo: bool,
}

/// Errors produced by [`ControlHost`] operations.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum HostError {
    #[error("{0}")]
    Engine(String),
    #[error("unsaved changes")]
    UnsavedChanges,
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    BadParams(String),
    #[error("{0}")]
    Io(String),
}

impl From<EngineError> for HostError {
    fn from(err: EngineError) -> Self {
        Self::Engine(err.to_string())
    }
}

/// Backend interface for driving editing and inspection.
pub trait ControlHost {
    /// List all registered commands.
    fn commands(&self) -> Vec<CommandInfo>;

    /// Run a command by id with parameters.
    fn run(&mut self, id: &str, params: &Json) -> Result<Json, HostError>;

    /// Run a batch of commands atomically.
    fn batch(&mut self, steps: &[(String, Json)]) -> Result<Vec<Json>, HostError>;

    /// Access the current course document model.
    fn course(&self) -> &Course;

    /// Get the active slide, layer, and object selection.
    fn selection(&self) -> Selection;

    /// Update the selection, validating that referenced IDs exist in the course.
    fn set_selection(
        &mut self,
        slide: Option<String>,
        layer: Option<Option<String>>,
        object: Option<Option<String>>,
    ) -> Result<Selection, HostError>;

    /// Save the course to a `.ccraft` package or `course.json` folder.
    fn save(&mut self, path: Option<&Path>) -> Result<PathBuf, HostError>;

    /// Open a `.ccraft` package or `course.json` file.
    fn open(&mut self, path: &Path, force: bool) -> Result<String, HostError>;

    /// Retrieve session status.
    fn info(&self) -> SessionInfo;

    /// Revert the last mutating command.
    fn undo(&mut self) -> Result<u64, HostError>;

    /// Redo the last undone command.
    fn redo(&mut self) -> Result<u64, HostError>;

    /// Abstract provider for media assets referenced by objects in the course.
    fn media(&self) -> &dyn coursecraft_render::MediaSource;

    /// Run player inputs on a fresh player instance and return emitted effects.
    fn preview_inputs(
        &self,
        inputs: &[coursecraft_logic::Input],
        start_slide: Option<&str>,
    ) -> Result<Vec<coursecraft_logic::Effect>, HostError> {
        let mut course = self.course().clone();
        if let Some(ss) = start_slide {
            course.start_slide = Some(ss.to_string());
        }
        let mut player = coursecraft_logic::Player::new(&course).map_err(|errs| {
            let msg = errs
                .iter()
                .map(|i| format!("{}: {}", i.path, i.message))
                .collect::<Vec<_>>()
                .join("\n");
            HostError::Engine(msg)
        })?;
        let mut effects = Vec::new();
        for input in inputs {
            effects.extend(player.handle(input));
        }
        Ok(effects)
    }
}

/// Synchronous media source for headless sessions.
///
/// Resolves assets from in-memory packages (e.g. `.ccraft`) or disk `<course_dir>/media/<src>`.
pub struct HeadlessMediaSource {
    course_path: Option<PathBuf>,
    in_memory: std::sync::Mutex<std::collections::HashMap<String, std::sync::Arc<[u8]>>>,
}

impl HeadlessMediaSource {
    /// Create a new headless media source with an optional course file/folder path.
    #[must_use]
    pub fn new(course_path: Option<PathBuf>) -> Self {
        Self {
            course_path,
            in_memory: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// Load or replace in-memory media entries from package pairs.
    pub fn set_in_memory(&self, media: &[(String, Vec<u8>)]) {
        let mut map = self.in_memory.lock().unwrap_or_else(|p| p.into_inner());
        map.clear();
        for (name, bytes) in media {
            map.insert(name.clone(), std::sync::Arc::from(bytes.as_slice()));
        }
    }

    /// Return a snapshot of all in-memory media entries as `(name, bytes)` pairs.
    #[must_use]
    pub fn snapshot_in_memory(&self) -> Vec<(String, Vec<u8>)> {
        let map = self.in_memory.lock().unwrap_or_else(|p| p.into_inner());
        let mut items: Vec<(String, Vec<u8>)> =
            map.iter().map(|(k, v)| (k.clone(), v.to_vec())).collect();
        items.sort_by(|a, b| a.0.cmp(&b.0));
        items
    }

    /// Update the current course path.
    pub fn set_path(&mut self, path: Option<PathBuf>) {
        self.course_path = path;
    }
}

/// Write `media` into the `media/` folder next to a bare `.json` course. A file
/// that already holds the same bytes is left alone.
fn write_sibling_media(target: &Path, media: &[(String, Vec<u8>)]) -> Result<(), HostError> {
    if media.is_empty() {
        return Ok(());
    }
    let dir = target
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .join("media");
    for (name, bytes) in media {
        if !coursecraft_format::is_safe_relative_path(name) {
            return Err(HostError::Io(format!(
                "media name {name:?} is not a safe path, nothing was marked saved"
            )));
        }
        let file = dir.join(name);
        if std::fs::read(&file).is_ok_and(|old| old == *bytes) {
            continue;
        }
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent).map_err(|e| HostError::Io(e.to_string()))?;
        }
        std::fs::write(&file, bytes).map_err(|e| HostError::Io(e.to_string()))?;
    }
    Ok(())
}

impl coursecraft_render::MediaSource for HeadlessMediaSource {
    fn get(&self, src: &str) -> coursecraft_render::MediaFetch {
        if !coursecraft_format::is_safe_relative_path(src) {
            return coursecraft_render::MediaFetch::Missing;
        }

        let map = self.in_memory.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(bytes) = map.get(src) {
            return coursecraft_render::MediaFetch::Ready(std::sync::Arc::clone(bytes));
        }
        drop(map);

        if let Some(ref p) = self.course_path
            && let Some(bytes) = read_contained(&media_dir_of(p), src)
        {
            return coursecraft_render::MediaFetch::Ready(bytes.into());
        }

        coursecraft_render::MediaFetch::Missing
    }
}

/// The `media/` folder that belongs to a course path: inside a project folder,
/// or beside a bare `course.json` or `.ccraft`.
fn media_dir_of(course_path: &Path) -> PathBuf {
    let course_dir = if course_path.is_dir() {
        course_path
    } else {
        course_path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
    };
    course_dir.join("media")
}

/// Read `media_dir/src` when it is a regular file that really lives inside
/// `media_dir` (a symlink out of it is refused) and is within the size limit.
fn read_contained(media_dir: &Path, src: &str) -> Option<Vec<u8>> {
    let root = std::fs::canonicalize(media_dir).ok()?;
    let file = std::fs::canonicalize(media_dir.join(src)).ok()?;
    if !file.starts_with(&root) {
        return None;
    }
    let meta = std::fs::metadata(&file).ok()?;
    if !meta.is_file() || meta.len() > coursecraft_format::Limits::default().max_entry_bytes {
        return None;
    }
    std::fs::read(&file).ok()
}

/// Headless host wrapping an in-memory [`Session`] and optional project file path.
pub struct HeadlessHost {
    pub session: Session,
    pub path: Option<PathBuf>,
    pub saved_revision: u64,
    pub selection: Selection,
    pub media_source: HeadlessMediaSource,
    /// The project folder last opened or saved, with what is known to be in it,
    /// so a folder save writes only what changed and deletes dropped media.
    folder: Option<(PathBuf, coursecraft_format::FolderRecord)>,
}

impl HeadlessHost {
    /// Create a new headless host for the given session.
    #[must_use]
    pub fn new(session: Session, path: Option<PathBuf>) -> Self {
        let saved_revision = session.revision;
        let slide = session
            .course
            .start_index()
            .and_then(|idx| session.course.slides().nth(idx))
            .map(|s| s.id.clone())
            .unwrap_or_default();
        let media_source = HeadlessMediaSource::new(path.clone());
        Self {
            session,
            path,
            saved_revision,
            selection: Selection {
                slide,
                layer: None,
                object: None,
            },
            media_source,
            folder: None,
        }
    }

    /// Open a course file (`.ccraft`, `.ccraft-folder`, or `course.json`) into a new headless host.
    pub fn open_path(path: &Path) -> Result<Self, HostError> {
        let mut host = Self::new(Session::blank(""), None);
        ControlHost::open(&mut host, path, true)?;
        Ok(host)
    }

    fn collect_media_for_save(&self) -> Result<Vec<(String, Vec<u8>)>, HostError> {
        use coursecraft_render::{MediaFetch, MediaSource};

        let mut media = self.media_source.snapshot_in_memory();
        let mut seen: std::collections::HashSet<String> =
            media.iter().map(|(k, _)| k.clone()).collect();

        for r in coursecraft_course::media_references(&self.session.course) {
            if let Some(folder) = r.src.strip_suffix('/') {
                // A web folder: every file under `media/<folder>/` goes with it.
                let Some(path) = self.path.as_deref() else {
                    continue;
                };
                let media_dir = media_dir_of(path);
                if !media_dir.join(folder).is_dir() {
                    continue;
                }
                let files = coursecraft_format::read_media_folder(
                    &media_dir,
                    folder,
                    coursecraft_course::WEB_MAX_FILES,
                    coursecraft_course::WEB_MAX_BYTES,
                )
                .map_err(|e| HostError::Io(format!("web folder {folder}: {e}")))?;
                for (name, bytes) in files {
                    if seen.insert(name.clone()) {
                        media.push((name, bytes));
                    }
                }
                continue;
            }
            if seen.contains(&r.src) {
                continue;
            }
            if let MediaFetch::Ready(bytes) = self.media_source.get(&r.src) {
                seen.insert(r.src.clone());
                media.push((r.src, bytes.to_vec()));
            }
        }

        media.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(media)
    }

    fn select_created(&mut self, command: &str, result: &Json) {
        let Some(new_id) = result.get("id").and_then(Json::as_str) else {
            return;
        };
        match command {
            "slide.add" => {
                self.selection.slide = new_id.to_owned();
                self.selection.layer = None;
                self.selection.object = None;
            }
            "layer.add" => {
                self.selection.layer = Some(new_id.to_owned());
                self.selection.object = None;
            }
            "object.add" | "object.duplicate" => {
                self.selection.object = Some(new_id.to_owned());
            }
            _ => {}
        }
    }

    fn repair_selection(&mut self) {
        if self.selection.slide.is_empty()
            || self.session.course.slide(&self.selection.slide).is_none()
        {
            self.selection.slide = self
                .session
                .course
                .start_index()
                .and_then(|i| self.session.course.slides().nth(i))
                .map(|s| s.id.clone())
                .unwrap_or_default();
            self.selection.layer = None;
            self.selection.object = None;
            return;
        }

        let Some(slide) = self.session.course.slide(&self.selection.slide) else {
            return;
        };

        if let Some(ref lid) = self.selection.layer
            && slide.layer(lid).is_none()
        {
            self.selection.layer = None;
            self.selection.object = None;
        }

        if let Some(ref oid) = self.selection.object {
            let exists = match self.selection.layer.as_deref() {
                Some(lid) => slide
                    .layer(lid)
                    .is_some_and(|l| l.objects.iter().any(|o| &o.id == oid)),
                None => slide.objects.iter().any(|o| &o.id == oid),
            };
            if !exists {
                self.selection.object = None;
            }
        }
    }
}

impl ControlHost for HeadlessHost {
    fn commands(&self) -> Vec<CommandInfo> {
        registry()
            .iter()
            .map(|c| CommandInfo {
                id: c.id.to_string(),
                label: c.label.to_string(),
                menu: c.menu.to_string(),
                params: c.params.to_string(),
                mutates: c.mutates,
            })
            .collect()
    }

    fn run(&mut self, id: &str, params: &Json) -> Result<Json, HostError> {
        let result = self.session.execute(id, params)?;
        self.select_created(id, &result);
        self.repair_selection();
        Ok(result)
    }

    fn batch(&mut self, steps: &[(String, Json)]) -> Result<Vec<Json>, HostError> {
        let results = self.session.execute_batch(steps)?;
        for (step, res) in steps.iter().zip(&results) {
            self.select_created(&step.0, res);
        }
        self.repair_selection();
        Ok(results)
    }

    fn course(&self) -> &Course {
        &self.session.course
    }

    fn selection(&self) -> Selection {
        self.selection.clone()
    }

    fn set_selection(
        &mut self,
        slide: Option<String>,
        layer: Option<Option<String>>,
        object: Option<Option<String>>,
    ) -> Result<Selection, HostError> {
        let target_slide = slide.unwrap_or_else(|| self.selection.slide.clone());
        if target_slide.is_empty() {
            return Err(HostError::NotFound("no slide selected".into()));
        }
        let slide_ref = self
            .session
            .course
            .slide(&target_slide)
            .ok_or_else(|| HostError::NotFound(format!("slide {target_slide:?} not found")))?;

        let target_layer = match layer {
            None => self.selection.layer.clone(),
            Some(None) => None,
            Some(Some(lid)) => {
                if slide_ref.layer(&lid).is_none() {
                    return Err(HostError::NotFound(format!("layer {lid:?} not found")));
                }
                Some(lid)
            }
        };

        let target_object = match object {
            None => self.selection.object.clone(),
            Some(None) => None,
            Some(Some(oid)) => {
                let exists = match target_layer.as_deref() {
                    Some(lid) => slide_ref
                        .layer(lid)
                        .is_some_and(|l| l.objects.iter().any(|o| o.id == oid)),
                    None => slide_ref.objects.iter().any(|o| o.id == oid),
                };
                if !exists {
                    return Err(HostError::NotFound(format!("object {oid:?} not found")));
                }
                Some(oid)
            }
        };

        self.selection = Selection {
            slide: target_slide,
            layer: target_layer,
            object: target_object,
        };
        Ok(self.selection.clone())
    }

    fn save(&mut self, path: Option<&Path>) -> Result<PathBuf, HostError> {
        let target = match path {
            Some(p) => p.to_path_buf(),
            None => self.path.clone().ok_or_else(|| {
                HostError::BadParams("no path specified for untitled course".into())
            })?,
        };

        if let Some(parent) = target.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let is_ccraft = target
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("ccraft"));

        if is_ccraft {
            let media = self.collect_media_for_save()?;
            let file_name = target
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("file");
            let tmp = target.with_file_name(format!("{file_name}.tmp-{}", std::process::id()));
            let result = (|| -> Result<(), HostError> {
                {
                    let file =
                        std::fs::File::create(&tmp).map_err(|e| HostError::Io(e.to_string()))?;
                    let mut buf = std::io::BufWriter::new(file);
                    coursecraft_format::save_project_to(&self.session.course, &media, &mut buf)
                        .map_err(|e| HostError::Engine(format!("save failed: {e}")))?;
                    std::io::Write::flush(&mut buf).map_err(|e| HostError::Io(e.to_string()))?;
                    let file = buf
                        .into_inner()
                        .map_err(|e| HostError::Io(e.error().to_string()))?;
                    file.sync_all().map_err(|e| HostError::Io(e.to_string()))?;
                }
                {
                    let file =
                        std::fs::File::open(&tmp).map_err(|e| HostError::Io(e.to_string()))?;
                    coursecraft_format::verify_project(std::io::BufReader::new(file))
                        .map_err(|e| HostError::Engine(format!("save verification failed: {e}")))?;
                }
                std::fs::rename(&tmp, &target).map_err(|e| HostError::Io(e.to_string()))?;
                Ok(())
            })();
            if result.is_err() {
                let _ = std::fs::remove_file(&tmp);
            }
            result?;
            self.media_source.set_in_memory(&media);
        } else {
            let is_dir = target.is_dir()
                || target
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_none_or(|e| !e.eq_ignore_ascii_case("json"));
            if is_dir {
                let media = self.collect_media_for_save()?;
                // Keep the record of the folder this host last opened or saved,
                // so media the course dropped is deleted and unchanged files
                // are not written again. Another folder starts with no record.
                let mut record = match self.folder.take() {
                    Some((dir, record)) if dir == target => record,
                    _ => coursecraft_format::FolderRecord::default(),
                };
                let saved = coursecraft_format::save_project_folder(
                    &target,
                    &self.session.course,
                    &media,
                    &mut record,
                )
                .map_err(|e| HostError::Engine(format!("save failed: {e}")));
                self.folder = Some((target.clone(), record));
                saved?;
                self.media_source.set_in_memory(&media);
            } else {
                let json_text = self
                    .session
                    .course
                    .to_json()
                    .map_err(|e| HostError::Engine(e.to_string()))?;
                // A bare `.json` keeps its pictures and sounds in a sibling
                // `media/` folder. Write them first; if one fails, return the
                // error before the course file or the clean mark changes.
                let media = self.collect_media_for_save()?;
                write_sibling_media(&target, &media)?;
                std::fs::write(&target, format!("{json_text}\n"))
                    .map_err(|e| HostError::Io(e.to_string()))?;
            }
        }

        self.path = Some(target.clone());
        self.media_source.set_path(Some(target.clone()));
        self.saved_revision = self.session.revision;
        Ok(target)
    }

    fn open(&mut self, path: &Path, force: bool) -> Result<String, HostError> {
        if self.session.revision != self.saved_revision && !force {
            return Err(HostError::UnsavedChanges);
        }

        let is_ccraft = path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("ccraft"));

        // Read and parse first. The session media changes only when the open worked.
        let new_media: Vec<(String, Vec<u8>)>;
        let mut new_folder = None;
        let course = if is_ccraft {
            let file = std::fs::File::open(path).map_err(|e| HostError::Io(e.to_string()))?;
            let reader = std::io::BufReader::with_capacity(256 * 1024, file);
            let proj = coursecraft_format::open_project_from(reader)
                .map_err(|e| HostError::Engine(e.to_string()))?;
            new_media = proj.media;
            proj.course
        } else if path.is_dir() {
            let opened = coursecraft_format::open_project_folder(path)
                .map_err(|e| HostError::Engine(e.to_string()))?;
            new_media = opened.project.media;
            new_folder = Some((path.to_path_buf(), opened.record));
            opened.project.course
        } else {
            let text = std::fs::read_to_string(path).map_err(|e| HostError::Io(e.to_string()))?;
            let course = Course::from_json(&text).map_err(|e| HostError::Engine(e.to_string()))?;
            new_media = Vec::new();
            course
        };
        self.media_source.set_in_memory(&new_media);

        self.media_source.set_path(Some(path.to_path_buf()));
        self.folder = new_folder;

        let title = course.title.clone();
        self.session = Session::new(course);
        self.path = Some(path.to_path_buf());
        self.saved_revision = self.session.revision;
        self.repair_selection();
        Ok(title)
    }

    fn info(&self) -> SessionInfo {
        SessionInfo {
            title: self.session.course.title.clone(),
            path: self.path.as_ref().map(|p| p.to_string_lossy().to_string()),
            dirty: self.session.revision != self.saved_revision,
            revision: self.session.revision,
            can_undo: self.session.can_undo(),
            can_redo: self.session.can_redo(),
        }
    }

    fn undo(&mut self) -> Result<u64, HostError> {
        self.session.undo()?;
        self.repair_selection();
        Ok(self.session.revision)
    }

    fn redo(&mut self) -> Result<u64, HostError> {
        self.session.redo()?;
        self.repair_selection();
        Ok(self.session.revision)
    }

    fn media(&self) -> &dyn coursecraft_render::MediaSource {
        &self.media_source
    }
}
