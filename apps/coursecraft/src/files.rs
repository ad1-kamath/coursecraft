//! Project state, open/save file dialogs, and serialization helpers.

use coursecraft_course::Course;
use coursecraft_engine::Session;
use coursecraft_format::{FolderRecord, open_project, save_project};
use std::path::{Path, PathBuf};

/// A media entry stored alongside the course.
pub type MediaItem = (String, Vec<u8>);

/// Resulting course and media assets loaded from disk.
pub type LoadedCourse = (Course, Vec<MediaItem>);

/// Active project metadata including path, associated media, and saved revision.
#[derive(Debug, Clone)]
pub struct Project {
    /// Filesystem path to the project archive, or None for untitled courses.
    pub path: Option<PathBuf>,
    /// Course revision recorded at the last save or load.
    pub saved_revision: u64,
    /// The file's size and modified time at the last open or save.
    pub disk_stamp: Option<DiskStamp>,
    /// Set when the project is a folder kept in git: what each file looked like
    /// at the last open or save. `None` for a `.ccraft` or an unsaved course.
    pub folder: Option<FolderRecord>,
}

impl Project {
    /// Create a new project instance for an untitled course.
    #[must_use]
    pub fn new(revision: u64) -> Self {
        Self {
            path: None,
            saved_revision: revision,
            disk_stamp: None,
            folder: None,
        }
    }

    /// Whether this project is a folder (for git) rather than a `.ccraft` file.
    #[must_use]
    pub fn is_folder(&self) -> bool {
        self.folder.is_some()
    }

    /// Remember the file's current size and modified time.
    pub fn record_disk_stamp(&mut self) {
        self.disk_stamp = self.path.as_deref().and_then(disk_stamp);
    }

    /// Whether the file on disk differs from what was last opened or saved:
    /// another app or machine wrote to it. False for an untitled project, a
    /// file we never stamped, or a file that is gone. For a folder, true when
    /// any file the project knows was changed or deleted outside the app.
    #[must_use]
    pub fn changed_on_disk(&mut self) -> bool {
        if self.is_folder() {
            return !self.changed_files().is_empty();
        }
        let (Some(path), Some(seen)) = (self.path.as_deref(), self.disk_stamp) else {
            return false;
        };
        disk_stamp(path).is_some_and(|now| now != seen)
    }

    /// The files of a folder project that changed outside the app, by their path
    /// in the folder (such as `media/logo.png`). Empty for anything else. A cheap
    /// check of size and modified time against what was recorded.
    #[must_use]
    pub fn changed_files(&mut self) -> Vec<String> {
        match (self.path.as_deref(), self.folder.as_mut()) {
            (Some(path), Some(record)) => coursecraft_format::changed_files(path, record),
            _ => Vec::new(),
        }
    }

    /// Whether the session contains unsaved modifications.
    #[must_use]
    pub fn is_dirty(&self, session: &Session) -> bool {
        session.revision != self.saved_revision
    }

    /// Display name of the project file, or `"Untitled"` if not yet saved to disk.
    #[must_use]
    pub fn file_name_display(&self) -> String {
        self.path
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .map(str::to_owned)
            .unwrap_or_else(|| "Untitled".to_string())
    }
}

/// The size and modified time of a project file, recorded when it is opened or
/// saved so a change made by another app (a sync service, another machine) is
/// noticed before it is overwritten.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiskStamp {
    modified: Option<std::time::SystemTime>,
    len: u64,
}

/// The current stamp of `path`, or `None` when it cannot be read.
#[must_use]
pub fn disk_stamp(path: &Path) -> Option<DiskStamp> {
    let meta = std::fs::metadata(path).ok()?;
    Some(DiskStamp {
        modified: meta.modified().ok(),
        len: meta.len(),
    })
}

/// Save a course and its media assets to a `.ccraft` file on disk.
///
/// The new file is written next to the target, flushed, checked (it must open,
/// parse and pass every CRC) and only then renamed over the old one. A file
/// that fails the check is thrown away and the old file is left untouched.
///
/// # Errors
/// Returns an error if serialization, verification or filesystem writing fails.
pub fn save_to(path: &Path, course: &Course, media: &[MediaItem]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let bytes = save_project(course, media).map_err(|e| format!("Save failed: {e}"))?;
    write_atomic_with(path, &bytes, Some(&verify_ccraft_file), &Hooks::default())
        .map_err(|e| format!("Failed to write file: {e}"))?;
    remove_stale_temps(path);
    Ok(())
}

/// Read a `.ccraft` file back from disk and check it is whole.
pub(crate) fn verify_ccraft_file(path: &Path) -> Result<(), String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    coursecraft_format::verify_project(std::io::BufReader::new(file)).map_err(|e| plain_error(&e))
}

/// A format error in plain words. A project over the size limits gets the same
/// sentence whether it is a `.ccraft` or a folder.
pub(crate) fn plain_error(error: &coursecraft_format::FormatError) -> String {
    use coursecraft_format::FormatError;
    match error {
        FormatError::TooLarge | FormatError::EntryTooLarge(_) | FormatError::TooManyEntries => {
            format!(
                "this project is too big to open again ({error}). CourseCraft opens projects up \
                 to 4 GB, with no single file over 1 GB. Your previous save was kept"
            )
        }
        other => other.to_string(),
    }
}

/// Save a course into a project folder, writing only the files that changed
/// since `record` was taken, and update `record` to match.
///
/// Media is written first, `course.json` last, then media the course no longer
/// uses is deleted. Only files the record knows under `media/` are ever deleted,
/// never anything else in the folder (`.git`, `.gitattributes`, notes).
///
/// # Errors
/// Returns a plain sentence when the project is too big to open again or a
/// write fails. A failed save leaves the earlier `course.json` and its media.
pub fn save_folder_to(
    dir: &Path,
    course: &Course,
    media: &[MediaItem],
    record: &mut FolderRecord,
) -> Result<coursecraft_format::SaveReport, String> {
    coursecraft_format::save_project_folder(dir, course, media, record).map_err(|e| match e {
        coursecraft_format::FormatError::Zip(detail) => {
            format!("Failed to write the project folder: {detail}")
        }
        other => format!("Save failed: {}", plain_error(&other)),
    })
}

/// Open a project folder, with the record the next save compares against.
///
/// # Errors
/// Returns a sentence when the folder cannot be read or is not a CourseCraft
/// project.
pub fn open_folder_from(path: &Path) -> Result<(LoadedCourse, FolderRecord), String> {
    let opened = coursecraft_format::open_project_folder(path).map_err(|e| {
        format!(
            "This folder can't be opened. It may be damaged or not a CourseCraft project folder. The folder has not been changed. ({})",
            plain_error(&e)
        )
    })?;
    Ok(((opened.project.course, opened.project.media), opened.record))
}

/// A check run on the written temp file; `Err` carries a plain reason.
pub type Verify<'a> = dyn Fn(&Path) -> Result<(), String> + 'a;

/// Seams the tests use to inject failures. Real saves use [`Hooks::default`].
pub struct Hooks<'a> {
    /// Runs on the flushed temp file just before verification.
    pub after_write: &'a dyn Fn(&Path) -> std::io::Result<()>,
    /// Moves the temp file over the target.
    pub rename: &'a dyn Fn(&Path, &Path) -> std::io::Result<()>,
    /// Pause between retries of a locked file.
    pub retry_delay: std::time::Duration,
}

impl Default for Hooks<'_> {
    fn default() -> Self {
        Self {
            after_write: &|_| Ok(()),
            rename: &|from, to| std::fs::rename(from, to),
            retry_delay: std::time::Duration::from_millis(400),
        }
    }
}

/// How many times a locked file is tried before giving up (about 2 s in all).
const LOCK_ATTEMPTS: u32 = 5;

/// What a person sees when the file stays locked.
pub const LOCKED_MESSAGE: &str =
    "Another app, such as a sync service, is using this file. Try again in a moment.";

/// Whether an error is the kind a sync service or another app holding the file
/// produces: a permission or sharing violation.
fn is_lock_error(error: &std::io::Error) -> bool {
    if matches!(
        error.kind(),
        std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::ResourceBusy
    ) {
        return true;
    }
    // Windows: ERROR_SHARING_VIOLATION (32) and ERROR_LOCK_VIOLATION (33).
    matches!(error.raw_os_error(), Some(32 | 33))
}

/// Run `step`, trying again while it fails with a lock error.
fn with_retries(
    delay: std::time::Duration,
    mut step: impl FnMut() -> std::io::Result<()>,
) -> std::io::Result<()> {
    let mut attempt = 1;
    loop {
        match step() {
            Err(e) if is_lock_error(&e) => {
                if attempt >= LOCK_ATTEMPTS {
                    return Err(std::io::Error::new(e.kind(), LOCKED_MESSAGE));
                }
                attempt += 1;
                std::thread::sleep(delay);
            }
            other => return other,
        }
    }
}

/// Write `bytes` to `path` atomically: write a sibling temp file in the same
/// directory, flush it to disk, then rename it over the target and flush the
/// directory. The temp file is removed on error, so a crash mid-write never
/// leaves a truncated target.
///
/// # Errors
/// Returns the underlying I/O error if the write or rename fails.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    write_atomic_with(path, bytes, None, &Hooks::default())
}

/// [`write_atomic`] with an optional check of the temp file before the rename
/// and injectable steps. A failed check leaves the target untouched and returns
/// an error carrying the check's message.
///
/// # Errors
/// Returns the underlying I/O error, or `InvalidData` when `verify` fails.
pub fn write_atomic_with(
    path: &Path,
    bytes: &[u8],
    verify: Option<&Verify<'_>>,
    hooks: &Hooks<'_>,
) -> std::io::Result<()> {
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let tmp = path.with_file_name(format!("{file_name}.tmp-{}", std::process::id()));
    let result = write_and_swap(path, &tmp, bytes, verify, hooks);
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

fn write_and_swap(
    path: &Path,
    tmp: &Path,
    bytes: &[u8],
    verify: Option<&Verify<'_>>,
    hooks: &Hooks<'_>,
) -> std::io::Result<()> {
    with_retries(hooks.retry_delay, || {
        let mut file = std::fs::File::create(tmp)?;
        std::io::Write::write_all(&mut file, bytes)?;
        file.sync_all()
    })?;
    (hooks.after_write)(tmp)?;
    if let Some(check) = verify {
        check(tmp).map_err(|msg| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "The saved copy did not pass its check, so the existing file was kept. ({msg})"
                ),
            )
        })?;
    }
    with_retries(hooks.retry_delay, || (hooks.rename)(tmp, path))?;
    sync_parent_dir(path);
    Ok(())
}

/// Flush the directory entry so the rename survives a power cut. Unix only; an
/// error is ignored.
fn sync_parent_dir(path: &Path) {
    #[cfg(unix)]
    if let Some(parent) = path.parent() {
        let dir = if parent.as_os_str().is_empty() {
            Path::new(".")
        } else {
            parent
        };
        if let Ok(handle) = std::fs::File::open(dir) {
            let _ = handle.sync_all();
        }
    }
    #[cfg(not(unix))]
    let _ = path;
}

/// Whether a process with this id is running. Used to tell a temp file left by
/// a crashed save from one another running copy is writing right now.
fn process_alive(pid: u32) -> bool {
    if pid == std::process::id() {
        return true;
    }
    #[cfg(unix)]
    {
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(true)
    }
    #[cfg(windows)]
    {
        std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains(&pid.to_string()))
            .unwrap_or(true)
    }
    #[cfg(not(any(unix, windows)))]
    {
        true
    }
}

/// Remove `<name>.tmp-<pid>` files next to `path` whose process is gone: the
/// leftovers of a save that was killed. Opening never reads them. A file whose
/// suffix is not a number, or whose process is alive, is left alone.
pub fn remove_stale_temps(path: &Path) {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return;
    };
    let dir = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    let prefix = format!("{name}.tmp-");
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let file_name = entry.file_name();
        let Some(file_name) = file_name.to_str() else {
            continue;
        };
        let Some(pid) = file_name
            .strip_prefix(&prefix)
            .and_then(|rest| rest.parse::<u32>().ok())
        else {
            continue;
        };
        if !process_alive(pid) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Open a `.ccraft` archive or bare `.json` course from disk.
///
/// # Errors
/// Returns an error if the file cannot be read or parsed.
pub fn open_from(path: &Path) -> Result<LoadedCourse, String> {
    if path.is_dir() {
        return open_folder_from(path).map(|(loaded, _)| loaded);
    }
    let bytes = std::fs::read(path).map_err(|e| format!("Failed to read file: {e}"))?;
    let damaged = |detail: String| {
        format!(
            "This file can't be opened. It may be damaged or not a CourseCraft project. The file has not been changed. ({detail})"
        )
    };
    if path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
    {
        let json_str = std::str::from_utf8(&bytes).map_err(|e| damaged(e.to_string()))?;
        let course = Course::from_json(json_str).map_err(|e| damaged(e.to_string()))?;
        Ok((course, Vec::new()))
    } else {
        let proj = open_project(&bytes).map_err(|e| damaged(e.to_string()))?;
        Ok((proj.course, proj.media))
    }
}

/// A folder copied into `media/web/<name>/` for a web object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebFolderImport {
    /// The folder path relative to `media/`, always `web/<name>`.
    pub path: String,
    /// The entry file relative to the folder.
    pub entry: String,
    /// The copied files, keyed by their path relative to `media/`.
    pub files: Vec<MediaItem>,
}

/// Copy a folder of HTML, CSS, JavaScript and media into media entries.
///
/// Hidden files and symlinks are skipped. The copy is rejected when it has
/// more than [`coursecraft_course::WEB_MAX_FILES`] files or more than
/// [`coursecraft_course::WEB_MAX_BYTES`] bytes. `existing` holds the current
/// media keys so the new `web/<name>/` does not collide with one already there.
///
/// # Errors
/// Returns a sentence when the folder cannot be read, has no `index.html` at
/// its root, contains a name that is not valid UTF-8, or exceeds the caps.
pub fn import_web_folder(folder: &Path, existing: &[String]) -> Result<WebFolderImport, String> {
    let raw_name = folder
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("web");
    let stem: String = raw_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let stem = stem.trim_matches('-');
    let stem = if stem.is_empty() { "web" } else { stem };

    let mut counter = 1u32;
    let path = loop {
        let candidate = if counter == 1 {
            format!("web/{stem}")
        } else {
            format!("web/{stem}-{counter}")
        };
        let taken = existing
            .iter()
            .any(|name| name == &candidate || name.starts_with(&format!("{candidate}/")));
        if !taken {
            break candidate;
        }
        counter = counter.saturating_add(1);
    };

    let mut files = Vec::new();
    let mut bytes: u64 = 0;
    collect_web_folder(folder, &path, &mut files, &mut bytes)?;

    let entry = "index.html".to_owned();
    if !files
        .iter()
        .any(|(name, _)| name == &format!("{path}/{entry}"))
    {
        return Err("The folder needs an index.html file at its top level.".to_owned());
    }

    Ok(WebFolderImport { path, entry, files })
}

fn collect_web_folder(
    dir: &Path,
    prefix: &str,
    files: &mut Vec<MediaItem>,
    bytes: &mut u64,
) -> Result<(), String> {
    let entries = std::fs::read_dir(dir)
        .map_err(|e| format!("Couldn't read the folder {}: {e}", dir.display()))?;
    let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);

    for entry in entries {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            return Err("The folder contains a file name that is not valid UTF-8.".to_owned());
        };
        if name.starts_with('.') {
            continue;
        }
        let file_type = match entry.path().symlink_metadata() {
            Ok(metadata) => metadata.file_type(),
            Err(_) => continue,
        };
        if file_type.is_symlink() {
            continue;
        }
        let next_prefix = format!("{prefix}/{name}");
        if file_type.is_dir() {
            collect_web_folder(&entry.path(), &next_prefix, files, bytes)?;
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        if files.len() >= coursecraft_course::WEB_MAX_FILES {
            return Err(format!(
                "The folder has more than {} files.",
                coursecraft_course::WEB_MAX_FILES
            ));
        }
        let data = std::fs::read(entry.path())
            .map_err(|e| format!("Couldn't read {}: {e}", entry.path().display()))?;
        let file_bytes = u64::try_from(data.len()).unwrap_or(u64::MAX);
        *bytes = bytes.saturating_add(file_bytes);
        if *bytes > coursecraft_course::WEB_MAX_BYTES {
            return Err(format!(
                "The folder is larger than {} MiB.",
                coursecraft_course::WEB_MAX_BYTES / (1024 * 1024)
            ));
        }
        files.push((next_prefix, data));
    }
    Ok(())
}

/// Open a native file picker dialog to select an image file.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn pick_image_file() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter(
            "Images (*.png, *.jpg, *.jpeg, *.webp, *.gif, *.svg)",
            &["png", "jpg", "jpeg", "webp", "gif", "svg"],
        )
        .pick_file()
}

/// Open a native file picker dialog to select an audio file.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn pick_audio_file() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter(
            "Audio (*.mp3, *.wav, *.ogg, *.m4a)",
            &["mp3", "wav", "ogg", "m4a"],
        )
        .pick_file()
}

/// Open a native file picker dialog to select a video file.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn pick_video_file() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Video (*.mp4, *.webm)", &["mp4", "webm"])
        .pick_file()
}

/// Open a native folder picker for a web object's files.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn pick_web_folder() -> Option<PathBuf> {
    rfd::FileDialog::new().pick_folder()
}

/// Open a native file picker dialog to select a Lottie animation.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn pick_lottie_file() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Lottie (*.json, *.lottie)", &["json", "lottie"])
        .pick_file()
}

/// Open a native file picker dialog to select one or more font files.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn pick_font_files() -> Vec<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Fonts (*.ttf, *.otf)", &["ttf", "otf"])
        .pick_files()
        .unwrap_or_default()
}

/// Open a native file picker dialog to select a WebVTT caption file.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn pick_captions_file() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Captions (*.vtt)", &["vtt"])
        .pick_file()
}

/// Open a native file picker dialog to select a PowerPoint presentation (.pptx).
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn pick_pptx_file() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("PowerPoint Presentation (*.pptx)", &["pptx"])
        .pick_file()
}

/// Open a native file picker dialog to select a document for AI outlining.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn pick_document_file() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Documents (*.docx, *.txt, *.md)", &["docx", "txt", "md"])
        .pick_file()
}

/// Open a native file picker dialog to select an XLIFF translation.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn pick_xliff_file() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("XLIFF Translation (*.xlf, *.xliff)", &["xlf", "xliff"])
        .pick_file()
}

/// Open a native file picker dialog to select a CSV file of quiz questions.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn pick_csv_file() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Questions (*.csv)", &["csv"])
        .pick_file()
}

/// Open a native save dialog for an XLIFF translation.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn pick_xliff_save_file(default_title: &str) -> Option<PathBuf> {
    let sanitized_title = default_title
        .replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_")
        .trim()
        .to_string();
    let stem = if sanitized_title.is_empty() {
        "course"
    } else {
        &sanitized_title
    };
    let res = rfd::FileDialog::new()
        .add_filter("XLIFF Translation (*.xlf)", &["xlf"])
        .set_file_name(format!("{stem}.xlf"))
        .save_file();
    res.map(|mut path| {
        if path.extension().is_none() {
            path.set_extension("xlf");
        }
        path
    })
}

/// Open a native save dialog for a rendered PNG such as a certificate.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn pick_image_save_file(default_name: &str) -> Option<PathBuf> {
    let name = default_name.trim();
    let name = if name.is_empty() {
        "certificate.png"
    } else {
        name
    };
    let res = rfd::FileDialog::new()
        .add_filter("PNG image (*.png)", &["png"])
        .set_file_name(name)
        .save_file();
    res.map(|mut path| {
        if path.extension().is_none() {
            path.set_extension("png");
        }
        path
    })
}

/// Open a native save dialog for a published zip.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn pick_publish_save_file(default_title: &str, standard: &str) -> Option<PathBuf> {
    let sanitized_title = default_title
        .replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_")
        .trim()
        .to_string();
    let stem = if sanitized_title.is_empty() {
        "course"
    } else {
        &sanitized_title
    };
    let default_name = format!("{stem}-{standard}.zip");
    let res = rfd::FileDialog::new()
        .add_filter("Zip archive (*.zip)", &["zip"])
        .set_file_name(&default_name)
        .save_file();
    res.map(|mut path| {
        if path.extension().is_none() {
            path.set_extension("zip");
        }
        path
    })
}

/// Open a native file picker dialog to select a `.ccraft` or `.json` file to open.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn pick_open_file() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter(
            "CourseCraft Project (*.ccraft, *.json)",
            &["ccraft", "json"],
        )
        .pick_file()
}

/// Open a native folder picker to choose a project folder to open.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn pick_project_folder() -> Option<PathBuf> {
    rfd::FileDialog::new().pick_folder()
}

/// Open a native save dialog to name a new project folder, `<name>.ccraft-folder`.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn pick_save_folder(default_title: &str) -> Option<PathBuf> {
    let sanitized_title = default_title
        .replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_")
        .trim()
        .to_string();
    let title_stem = if sanitized_title.is_empty() {
        "Untitled"
    } else {
        &sanitized_title
    };
    let default_name = format!("{title_stem}.{}", coursecraft_format::FOLDER_EXTENSION);
    rfd::FileDialog::new()
        .set_file_name(&default_name)
        .save_file()
        .map(|mut path| {
            if path.extension().is_none() {
                path.set_extension(coursecraft_format::FOLDER_EXTENSION);
            }
            path
        })
}

/// Open a native file picker dialog to choose a destination `.ccraft` save path.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn pick_save_file(default_title: &str) -> Option<PathBuf> {
    let sanitized_title = default_title
        .replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_")
        .trim()
        .to_string();
    let title_stem = if sanitized_title.is_empty() {
        "Untitled"
    } else {
        &sanitized_title
    };
    let default_name = format!("{title_stem}.ccraft");
    let res = rfd::FileDialog::new()
        .add_filter("CourseCraft Project (*.ccraft)", &["ccraft"])
        .set_file_name(&default_name)
        .save_file();

    res.map(|mut path| {
        if path.extension().is_none() {
            path.set_extension("ccraft");
        }
        path
    })
}
