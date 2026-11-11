//! CourseCraft project format (L3).
//!
//! A `.ccraft` file is a zip holding `course.json` and the course's
//! `media/`. Opening one treats the file as hostile: entry counts, sizes and
//! paths are bounded and checked. No filesystem access; the app reads and
//! writes the bytes.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

use std::collections::HashSet;
use std::io::{Read as _, Write as _};

use coursecraft_course::Course;

pub mod xliff;

#[cfg(not(target_arch = "wasm32"))]
mod folder;
#[cfg(not(target_arch = "wasm32"))]
pub use folder::{
    COURSE_FILE, FOLDER_EXTENSION, FileRecord, FolderRecord, MEDIA_DIR, OpenedFolder, SaveHooks,
    SaveReport, changed_files, fnv1a64, is_project_folder, open_project_folder,
    open_project_folder_with_limits, save_project_folder, save_project_folder_with,
};

#[cfg(test)]
mod folder_tests;
#[cfg(test)]
mod tests;

/// Maximum number of archive entries allowed when opening a project archive.
pub const MAX_ENTRIES: usize = 10_000;

/// Maximum uncompressed bytes across all entries in a project archive (4 GiB).
///
/// Only entries that are stored (not deflated) can use more than
/// [`MAX_DEFLATED_TOTAL_BYTES`]: a stored entry cannot expand, so it is no
/// decompression bomb.
pub const MAX_TOTAL_BYTES: u64 = 4 * 1024 * 1024 * 1024;

/// Maximum uncompressed bytes permitted for a single entry (1 GiB).
pub const MAX_ENTRY_BYTES: u64 = 1024 * 1024 * 1024;

/// Maximum uncompressed bytes across all deflated entries (512 MiB).
pub const MAX_DEFLATED_TOTAL_BYTES: u64 = 512 * 1024 * 1024;

/// Maximum uncompressed bytes permitted for a single deflated entry (256 MiB).
pub const MAX_DEFLATED_ENTRY_BYTES: u64 = 256 * 1024 * 1024;

/// Whether a file is stored without deflating. Picture, video, audio, font,
/// archive and document files qualify: their formats are already compressed,
/// so deflating them again only costs time. Uncompressed WAV and AIFF audio
/// qualifies too, because long narration would otherwise hit the tighter caps
/// on deflated entries and the project could not be opened. The check is on
/// the extension, case-insensitive.
#[must_use]
pub fn is_precompressed(path: &str) -> bool {
    let ext = path.rsplit_once('.').map_or("", |(_, e)| e);
    [
        "png", "jpg", "jpeg", "gif", "webp", "avif", "heic", "mp4", "m4v", "mov", "webm", "mkv",
        "mp3", "m4a", "aac", "ogg", "oga", "opus", "flac", "wav", "aif", "aiff", "woff", "woff2",
        "zip", "gz", "pdf", "docx", "pptx", "xlsx",
    ]
    .iter()
    .any(|known| ext.eq_ignore_ascii_case(known))
}

/// Zip write options for a file: stored when the format is already
/// compressed, deflated otherwise. A fixed timestamp keeps output
/// deterministic.
#[must_use]
pub fn zip_options_for(path: &str) -> zip::write::SimpleFileOptions {
    let method = if is_precompressed(path) {
        zip::CompressionMethod::Stored
    } else {
        zip::CompressionMethod::Deflated
    };
    zip::write::SimpleFileOptions::default()
        .compression_method(method)
        .last_modified_time(zip::DateTime::default())
}

/// Resource limits applied when reading a `.ccraft` project archive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Maximum number of entries permitted.
    pub max_entries: usize,
    /// Maximum total uncompressed bytes permitted across all entries.
    pub max_total_bytes: u64,
    /// Maximum uncompressed bytes permitted for any single entry.
    pub max_entry_bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_entries: MAX_ENTRIES,
            max_total_bytes: MAX_TOTAL_BYTES,
            max_entry_bytes: MAX_ENTRY_BYTES,
        }
    }
}

/// An open CourseCraft project holding its parsed course model and media files.
#[derive(Clone, Debug, PartialEq)]
pub struct Project {
    /// The course document model parsed from `course.json`.
    pub course: Course,
    /// Media assets stored in the project, relative to `media/` with the prefix stripped.
    pub media: Vec<(String, Vec<u8>)>,
}

/// Project format errors. Every hostile or invalid input produces an error, never a panic.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum FormatError {
    /// Archive is missing the required `course.json` file.
    #[error("missing course.json")]
    MissingCourseJson,
    /// Failed to parse `course.json` into a [`Course`].
    #[error("invalid course.json: {0}")]
    CourseJson(String),
    /// An entry path is unsafe (e.g. contains `..`, leading slash, or drive letter).
    #[error("unsafe path: {0}")]
    UnsafePath(String),
    /// A duplicate entry was encountered.
    #[error("duplicate entry: {0}")]
    DuplicateEntry(String),
    /// Archive contains more entries than permitted.
    #[error("too many entries")]
    TooManyEntries,
    /// A single entry exceeds the maximum permitted byte size.
    #[error("entry too large: {0}")]
    EntryTooLarge(String),
    /// Total uncompressed size of archive entries exceeds the permitted maximum.
    #[error("archive too large")]
    TooLarge,
    /// Zip archive format error or decompression failure.
    #[error("zip error: {0}")]
    Zip(String),
}

impl From<zip::result::ZipError> for FormatError {
    fn from(err: zip::result::ZipError) -> Self {
        FormatError::Zip(err.to_string())
    }
}

impl From<std::io::Error> for FormatError {
    fn from(err: std::io::Error) -> Self {
        FormatError::Zip(err.to_string())
    }
}

impl From<serde_json::Error> for FormatError {
    fn from(err: serde_json::Error) -> Self {
        FormatError::CourseJson(err.to_string())
    }
}

/// Validate whether a relative path is safe for package inclusion and project archives.
///
/// Rejects empty paths, leading slashes, backslashes, colons (drive letters and
/// URL schemes), control characters, and any segment that is empty, `.` or `..`.
#[must_use]
pub fn is_safe_relative_path(path: &str) -> bool {
    if path.is_empty() {
        return false;
    }
    if path.starts_with('/') || path.contains('\\') || path.contains(':') {
        return false;
    }
    if path.chars().any(char::is_control) {
        return false;
    }
    for segment in path.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." {
            return false;
        }
    }
    true
}

/// Save a course and its media files to a deterministic `.ccraft` zip archive.
///
/// Serialises `course` to `course.json` with a trailing newline, validates each
/// media path with [`is_safe_relative_path`], rejects duplicate media paths,
/// and stores media under `media/<path>`. Deflates text and stores media that is already
/// compressed (see [`is_precompressed`]). Uses a fixed
/// 1980-01-01 timestamp for deterministic output.
///
/// # Errors
/// Returns [`FormatError`] if JSON serialization fails, a media path is unsafe,
/// duplicate media paths are provided, or zip writing fails.
pub fn save_project(course: &Course, media: &[(String, Vec<u8>)]) -> Result<Vec<u8>, FormatError> {
    // Size the buffer once so a 1 GB project is not copied again on every
    // doubling. Stored media is the bulk and stays its own size.
    let capacity = media
        .iter()
        .map(|(_, bytes)| bytes.len().saturating_add(256))
        .fold(1 << 20, usize::saturating_add);
    let mut out = Vec::with_capacity(capacity);
    save_project_to(course, media, std::io::Cursor::new(&mut out))?;
    Ok(out)
}

/// Save a project straight into a seekable writer such as a file, without
/// holding the whole archive in memory. Same bytes as [`save_project`].
///
/// # Errors
/// Returns [`FormatError`] for the same reasons as [`save_project`], plus any
/// write error from `out`.
pub fn save_project_to<W: std::io::Write + std::io::Seek>(
    course: &Course,
    media: &[(String, Vec<u8>)],
    out: W,
) -> Result<(), FormatError> {
    let mut course_json = course.to_json()?;
    course_json.push('\n');

    let mut seen_media = HashSet::new();
    for (path, _) in media {
        if !is_safe_relative_path(path) {
            return Err(FormatError::UnsafePath(path.clone()));
        }
        if !seen_media.insert(path.as_str()) {
            return Err(FormatError::DuplicateEntry(path.clone()));
        }
    }

    let mut writer = zip::ZipWriter::new(out);
    writer.start_file("course.json", zip_options_for("course.json"))?;
    writer.write_all(course_json.as_bytes())?;

    for (path, bytes) in media {
        let zip_path = format!("media/{path}");
        writer.start_file(&zip_path, zip_options_for(path))?;
        writer.write_all(bytes)?;
    }

    writer.finish()?;
    Ok(())
}

/// Read just the course out of a `.ccraft` without loading any media. Cheap
/// enough to describe many large files, for example a list of versions.
///
/// # Errors
/// Returns [`FormatError`] when the zip is unreadable or `course.json` is
/// missing or invalid.
pub fn read_course<R: std::io::Read + std::io::Seek>(reader: R) -> Result<Course, FormatError> {
    let mut archive = zip::ZipArchive::new(reader)?;
    let file = archive
        .by_name("course.json")
        .map_err(|_| FormatError::MissingCourseJson)?;
    // course.json is deflated text, so keep the same bomb cap as a full open.
    let mut text = String::new();
    file.take(MAX_DEFLATED_ENTRY_BYTES)
        .read_to_string(&mut text)
        .map_err(|e| FormatError::Zip(format!("course.json: {e}")))?;
    Ok(Course::from_json(&text)?)
}

/// Check that a saved project is whole and can be opened again: the zip opens,
/// `course.json` parses, every entry reads to the end with its CRC matching,
/// and the sizes are within the limits [`open_project`] enforces. Entries are
/// streamed to a sink, so media is never held in memory. Used to verify a
/// file on disk before it replaces an earlier save.
///
/// # Errors
/// Returns [`FormatError`] naming the first problem found.
pub fn verify_project<R: std::io::Read + std::io::Seek>(reader: R) -> Result<(), FormatError> {
    verify_project_with_limits(reader, Limits::default())
}

/// [`verify_project`] with custom limits.
///
/// # Errors
/// Returns [`FormatError`] naming the first problem found.
pub fn verify_project_with_limits<R: std::io::Read + std::io::Seek>(
    reader: R,
    limits: Limits,
) -> Result<(), FormatError> {
    let mut archive = zip::ZipArchive::new(reader)?;
    if archive.len() > limits.max_entries {
        return Err(FormatError::TooManyEntries);
    }
    let mut course_seen = false;
    let mut sizes = EntrySizes::default();
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let name = file.name()?.to_string();
        let stored = file.compression() == zip::CompressionMethod::Stored;
        let size = if name == "course.json" {
            let mut text = String::new();
            file.read_to_string(&mut text)
                .map_err(|e| FormatError::Zip(format!("{name}: {e}")))?;
            Course::from_json(&text)?;
            course_seen = true;
            text.len() as u64
        } else {
            std::io::copy(&mut file, &mut std::io::sink())
                .map_err(|e| FormatError::Zip(format!("{name}: {e}")))?
        };
        if size > entry_limit(stored, limits) {
            return Err(FormatError::EntryTooLarge(name));
        }
        sizes.add(size, stored, limits)?;
    }
    if course_seen {
        Ok(())
    } else {
        Err(FormatError::MissingCourseJson)
    }
}

/// The largest a single entry may be. A stored entry cannot expand, so it may
/// use the full limits. A deflated entry is a possible bomb and keeps the
/// older, tighter caps.
fn entry_limit(stored: bool, limits: Limits) -> u64 {
    if stored {
        limits.max_entry_bytes
    } else {
        limits.max_entry_bytes.min(MAX_DEFLATED_ENTRY_BYTES)
    }
}

/// Running totals of entry sizes, checked against the archive-wide limits.
#[derive(Default)]
struct EntrySizes {
    total: u64,
    deflated: u64,
}

impl EntrySizes {
    fn add(&mut self, size: u64, stored: bool, limits: Limits) -> Result<(), FormatError> {
        if !stored {
            self.deflated = self
                .deflated
                .checked_add(size)
                .ok_or(FormatError::TooLarge)?;
            if self.deflated > limits.max_total_bytes.min(MAX_DEFLATED_TOTAL_BYTES) {
                return Err(FormatError::TooLarge);
            }
        }
        self.total = self.total.checked_add(size).ok_or(FormatError::TooLarge)?;
        if self.total > limits.max_total_bytes {
            return Err(FormatError::TooLarge);
        }
        Ok(())
    }
}

/// Open a `.ccraft` project archive from memory using default limits.
///
/// # Errors
/// Returns [`FormatError`] if the archive violates limits, contains unsafe paths,
/// has invalid or missing `course.json`, or contains duplicate entries.
pub fn open_project(bytes: &[u8]) -> Result<Project, FormatError> {
    open_project_with_limits(bytes, Limits::default())
}

/// Open a `.ccraft` project archive with custom limits.
///
/// Binds entry count and entry sizes to guard against hostile inputs like zip bombs.
/// Directory entries (ending in `/`) are skipped. Unknown non-media files are ignored.
///
/// # Errors
/// Returns [`FormatError`] if the archive violates limits, contains unsafe paths,
/// has invalid or missing `course.json`, or contains duplicate entries.
pub fn open_project_with_limits(bytes: &[u8], limits: Limits) -> Result<Project, FormatError> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;

    if archive.len() > limits.max_entries {
        return Err(FormatError::TooManyEntries);
    }

    if archive.has_duplicate_names() {
        return Err(FormatError::DuplicateEntry(
            "duplicate central directory entry".to_string(),
        ));
    }

    let mut seen_entries = HashSet::new();
    let mut course_json_bytes = None;
    let mut media = Vec::new();
    let mut sizes = EntrySizes::default();

    for i in 0..archive.len() {
        let file = archive.by_index(i)?;
        let name = file.name()?.to_string();

        if !seen_entries.insert(name.clone()) {
            return Err(FormatError::DuplicateEntry(name));
        }

        if let Some(dir_path) = name.strip_suffix('/') {
            if !is_safe_relative_path(dir_path) {
                return Err(FormatError::UnsafePath(name));
            }
            continue;
        }

        if !is_safe_relative_path(&name) {
            return Err(FormatError::UnsafePath(name));
        }

        let stored = file.compression() == zip::CompressionMethod::Stored;
        let entry_limit = entry_limit(stored, limits);

        if file.size() > entry_limit {
            return Err(FormatError::EntryTooLarge(name));
        }

        // Reserve the declared size up front (it is within the limit) so a
        // 100 MB video is read without regrowing its buffer.
        let reserve = usize::try_from(file.size()).unwrap_or(0);
        let mut entry_bytes = Vec::with_capacity(reserve);
        let read_limit = entry_limit.saturating_add(1);
        file.take(read_limit).read_to_end(&mut entry_bytes)?;

        if entry_bytes.len() as u64 > entry_limit {
            return Err(FormatError::EntryTooLarge(name));
        }

        sizes.add(entry_bytes.len() as u64, stored, limits)?;

        if name == "course.json" {
            course_json_bytes = Some(entry_bytes);
        } else if let Some(media_rel) = name.strip_prefix("media/") {
            media.push((media_rel.to_string(), entry_bytes));
        }
    }

    let course_bytes = course_json_bytes.ok_or(FormatError::MissingCourseJson)?;
    let course_str =
        std::str::from_utf8(&course_bytes).map_err(|e| FormatError::CourseJson(e.to_string()))?;
    let course = Course::from_json(course_str)?;

    Ok(Project { course, media })
}
