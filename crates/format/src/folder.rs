//! A project as a folder, for keeping a course in git.
//!
//! The layout matches the `.ccraft` zip entry for entry:
//!
//! ```text
//! <name>.ccraft-folder/
//!   course.json          pretty JSON, 2 spaces, "\n" line ends, trailing newline
//!   media/...            every media file, fonts under media/fonts/
//! ```
//!
//! Nothing else is ever written into the folder, and nothing but the files this
//! module knows about is ever deleted: only files under `media/` that an earlier
//! open or save recorded and that the course no longer uses. A `.git` folder,
//! `.gitattributes`, notes or any other file a person keeps there are never
//! touched.
//!
//! A [`FolderRecord`] remembers each file's size, FNV-1a hash and modified time
//! from the last open or save. Saving writes only the files whose content
//! differs from the record, so a git diff stays small and a large course is not
//! read back from disk. Every write goes to a temporary file in the same folder,
//! is flushed, and is renamed into place. `course.json` is written last, so a
//! crash leaves either the old or the new `course.json` and every media file the
//! surviving one needs.
//!
//! Std only, and no `std::os::unix`, so Windows builds. Not built for wasm.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, SystemTime};

use coursecraft_course::Course;

use crate::{
    EntrySizes, FormatError, Limits, Project, entry_limit, is_precompressed, is_safe_relative_path,
};

/// The folder name's extension: `<name>.ccraft-folder`.
pub const FOLDER_EXTENSION: &str = "ccraft-folder";

/// The name of the course file inside a project folder.
pub const COURSE_FILE: &str = "course.json";

/// The folder holding every media file (and `fonts/` inside it).
pub const MEDIA_DIR: &str = "media";

/// The suffix of the temporary file a write goes through. A leftover one from a
/// crashed save is ignored on open and overwritten by the next save.
const TEMP_SUFFIX: &str = ".ccraft-tmp";

/// How deep `media/` may nest before the folder is refused.
const MAX_DEPTH: usize = 64;

/// How many times a locked file is tried before giving up.
const LOCK_ATTEMPTS: u32 = 5;

/// FNV-1a, 64 bit. Written out by hand because `DefaultHasher` may change
/// between Rust versions, which would make a saved record meaningless.
#[must_use]
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut state = Fnv::new();
    state.update(bytes);
    state.finish()
}

/// Streaming FNV-1a so a large file is hashed without being held whole.
struct Fnv(u64);

impl Fnv {
    fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }

    fn update(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 ^= u64::from(byte);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

/// Fast deterministic word-at-a-time 64-bit hash for comparing media blobs.
#[must_use]
pub fn fast_hash64(bytes: &[u8]) -> u64 {
    const K1: u64 = 0x517c_c1b7_2722_0a95;
    const K2: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut h: u64 = (bytes.len() as u64).wrapping_mul(K2);
    let (chunks, rem) = bytes.as_chunks::<8>();
    for &arr in chunks {
        let word = u64::from_le_bytes(arr);
        h = (h.rotate_left(5) ^ word).wrapping_mul(K1);
    }
    if !rem.is_empty() {
        let mut tail = [0u8; 8];
        if let Some(dst) = tail.get_mut(..rem.len()) {
            dst.copy_from_slice(rem);
        }
        let word = u64::from_le_bytes(tail);
        h = (h.rotate_left(5) ^ word).wrapping_mul(K1);
    }
    h ^= h >> 33;
    h = h.wrapping_mul(0xff51_afd7_ed55_8ccd);
    h ^= h >> 33;
    h
}

/// What is known about one file after the last open or save.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileRecord {
    /// Size in bytes.
    pub size: u64,
    /// FNV-1a hash of the content.
    pub hash: u64,
    /// Modified time on disk when it was recorded, when the system reports one.
    pub modified: Option<SystemTime>,
}

/// Every file of a project folder as of the last open or save, keyed by its
/// path inside the folder (`course.json`, `media/...`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FolderRecord {
    files: BTreeMap<String, FileRecord>,
}

impl FolderRecord {
    /// A record that knows no files, for a folder saved into for the first time.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The recorded files, keyed by path inside the folder.
    #[must_use]
    pub fn files(&self) -> &BTreeMap<String, FileRecord> {
        &self.files
    }

    #[cfg(test)]
    pub(crate) fn files_mut(&mut self) -> &mut BTreeMap<String, FileRecord> {
        &mut self.files
    }

    /// Number of recorded files.
    #[must_use]
    pub fn len(&self) -> usize {
        self.files.len()
    }

    /// Whether no file is recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

/// A project read from a folder, with the record to hand back when saving.
#[derive(Debug, Clone)]
pub struct OpenedFolder {
    /// The course and its media, as [`crate::open_project`] returns for a zip.
    pub project: Project,
    /// Every file read, for the cheap changed-on-disk check and the next save.
    pub record: FolderRecord,
}

/// What a save did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SaveReport {
    /// Paths written, relative to the folder. Empty when nothing had changed.
    pub written: Vec<String>,
    /// Paths deleted because the course no longer uses them.
    pub deleted: Vec<String>,
}

/// Seams the tests use to simulate a crash part way through a save.
pub struct SaveHooks<'a> {
    /// Runs after the media is written and before `course.json` is. An error
    /// stops the save there, as a crash would.
    pub before_course_json: &'a dyn Fn() -> std::io::Result<()>,
    /// Runs after `course.json` is written and before unused media is deleted.
    pub before_delete: &'a dyn Fn() -> std::io::Result<()>,
    /// Pause between retries of a locked file.
    pub retry_delay: Duration,
    /// The limits a project must fit to be opened again. A save that would
    /// exceed them is refused before anything is written, as a `.ccraft` is.
    pub limits: Limits,
}

impl Default for SaveHooks<'_> {
    fn default() -> Self {
        Self {
            before_course_json: &|| Ok(()),
            before_delete: &|| Ok(()),
            retry_delay: Duration::from_millis(400),
            limits: Limits::default(),
        }
    }
}

/// Whether a path looks like a project folder: a directory holding `course.json`.
#[must_use]
pub fn is_project_folder(path: &Path) -> bool {
    path.is_dir() && path.join(COURSE_FILE).is_file()
}

fn io_error(path: &str, error: &std::io::Error) -> FormatError {
    FormatError::Zip(format!("{path}: {error}"))
}

// ---------------------------------------------------------------------- open

/// Open a project folder with the default limits.
///
/// # Errors
/// Returns [`FormatError`] for a missing or invalid `course.json`, an unsafe
/// path, a symlink that leaves the folder, or a folder over the same limits a
/// `.ccraft` is held to.
pub fn open_project_folder(dir: &Path) -> Result<OpenedFolder, FormatError> {
    open_project_folder_with_limits(dir, Limits::default())
}

/// [`open_project_folder`] with custom limits.
///
/// # Errors
/// Returns [`FormatError`] as [`open_project_folder`] does.
pub fn open_project_folder_with_limits(
    dir: &Path,
    limits: Limits,
) -> Result<OpenedFolder, FormatError> {
    let root = canonical_root(dir)?;
    let mut sizes = EntrySizes::default();
    let mut record = FolderRecord::new();

    let course_path = dir.join(COURSE_FILE);
    let course_meta =
        std::fs::symlink_metadata(&course_path).map_err(|_| FormatError::MissingCourseJson)?;
    if course_meta.file_type().is_symlink() {
        check_link(&root, &course_path, COURSE_FILE)?;
    }
    let course_bytes = read_limited(&course_path, COURSE_FILE, false, limits, &mut sizes)?;
    let text =
        std::str::from_utf8(&course_bytes).map_err(|e| FormatError::CourseJson(e.to_string()))?;
    let course = Course::from_json(text)?;
    record.files.insert(
        COURSE_FILE.to_owned(),
        file_record(&course_path, &course_bytes),
    );

    let mut files = Vec::new();
    let media_dir = dir.join(MEDIA_DIR);
    if let Ok(meta) = std::fs::symlink_metadata(&media_dir) {
        if meta.file_type().is_symlink() {
            check_dir_link(&root, &media_dir, MEDIA_DIR)?;
        }
        let max = limits.max_entries.saturating_sub(1);
        collect_files(&root, &media_dir, MEDIA_DIR, 0, max, &mut files)?;
    }
    if files.len().saturating_add(1) > limits.max_entries {
        return Err(FormatError::TooManyEntries);
    }

    let mut media = Vec::with_capacity(files.len());
    for (rel, path) in files {
        let stored = is_precompressed(&rel);
        let bytes = read_limited(&path, &rel, stored, limits, &mut sizes)?;
        let inside = rel
            .strip_prefix("media/")
            .map_or_else(|| rel.clone(), str::to_owned);
        record.files.insert(rel, file_record(&path, &bytes));
        media.push((inside, bytes));
    }
    Ok(OpenedFolder {
        project: Project { course, media },
        record,
    })
}

fn canonical_root(dir: &Path) -> Result<PathBuf, FormatError> {
    std::fs::canonicalize(dir).map_err(|e| io_error(&dir.display().to_string(), &e))
}

/// A symlink is allowed only when it points at a file inside the folder.
fn check_link(root: &Path, path: &Path, name: &str) -> Result<(), FormatError> {
    let target =
        std::fs::canonicalize(path).map_err(|_| FormatError::UnsafePath(name.to_owned()))?;
    if target.starts_with(root) && target.is_file() {
        Ok(())
    } else {
        Err(FormatError::UnsafePath(name.to_owned()))
    }
}

/// A folder symlink is allowed only when it points at a folder inside the project.
fn check_dir_link(root: &Path, path: &Path, name: &str) -> Result<(), FormatError> {
    let target =
        std::fs::canonicalize(path).map_err(|_| FormatError::UnsafePath(name.to_owned()))?;
    if target.starts_with(root) && target.is_dir() {
        Ok(())
    } else {
        Err(FormatError::UnsafePath(name.to_owned()))
    }
}

/// Every regular file under `dir`, as (path inside the folder, path on disk),
/// sorted by path. Temp files from a crashed save are skipped. The walk stops
/// with [`FormatError::TooManyEntries`] as soon as more than `max` files are found.
fn collect_files(
    root: &Path,
    dir: &Path,
    rel_dir: &str,
    depth: usize,
    max: usize,
    out: &mut Vec<(String, PathBuf)>,
) -> Result<(), FormatError> {
    if depth > MAX_DEPTH {
        return Err(FormatError::UnsafePath(rel_dir.to_owned()));
    }
    let entries = std::fs::read_dir(dir).map_err(|e| io_error(rel_dir, &e))?;
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| io_error(rel_dir, &e))?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| FormatError::UnsafePath(format!("{rel_dir}/(not UTF-8)")))?;
        names.push(name);
    }
    names.sort();
    for name in names {
        let rel = format!("{rel_dir}/{name}");
        if !is_safe_relative_path(&rel) {
            return Err(FormatError::UnsafePath(rel));
        }
        let path = dir.join(&name);
        let meta = std::fs::symlink_metadata(&path).map_err(|e| io_error(&rel, &e))?;
        let kind = meta.file_type();
        if kind.is_symlink() {
            check_link(root, &path, &rel)?;
            out.push((rel, path));
            if out.len() > max {
                return Err(FormatError::TooManyEntries);
            }
        } else if kind.is_dir() {
            collect_files(root, &path, &rel, depth + 1, max, out)?;
        } else if kind.is_file() {
            if !name.ends_with(TEMP_SUFFIX) {
                out.push((rel, path));
                if out.len() > max {
                    return Err(FormatError::TooManyEntries);
                }
            }
        } else {
            return Err(FormatError::UnsafePath(rel));
        }
    }
    Ok(())
}

/// Read a whole file under the size limits, checking the declared size first so
/// an oversized file is refused before it is read.
fn read_limited(
    path: &Path,
    name: &str,
    stored: bool,
    limits: Limits,
    sizes: &mut EntrySizes,
) -> Result<Vec<u8>, FormatError> {
    let limit = entry_limit(stored, limits);
    let declared = std::fs::metadata(path)
        .map_err(|e| io_error(name, &e))?
        .len();
    if declared > limit {
        return Err(FormatError::EntryTooLarge(name.to_owned()));
    }
    let mut file = std::fs::File::open(path).map_err(|e| io_error(name, &e))?;
    let mut bytes = Vec::with_capacity(usize::try_from(declared).unwrap_or(0));
    (&mut file)
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|e| io_error(name, &e))?;
    if bytes.len() as u64 > limit {
        return Err(FormatError::EntryTooLarge(name.to_owned()));
    }
    sizes.add(bytes.len() as u64, stored, limits)?;
    Ok(bytes)
}

fn modified_time(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}

fn file_record(path: &Path, bytes: &[u8]) -> FileRecord {
    FileRecord {
        size: bytes.len() as u64,
        hash: fast_hash64(bytes),
        modified: modified_time(path),
    }
}

// ------------------------------------------------------------- media folders

/// Read every file under `media_dir/<path>/`, as `(path relative to media_dir,
/// bytes)` sorted by path. This is how a web folder (`web/<name>`) is loaded
/// from disk beside a bare `course.json`.
///
/// Symbolic links are skipped, and the folder itself must resolve to a place
/// inside `media_dir`, so a link cannot pull in files from elsewhere. The walk
/// stops at `max_files` files, `max_bytes` bytes in total and a fixed depth;
/// a file's size is checked before it is read.
///
/// # Errors
/// Returns [`FormatError`] for an unsafe `path`, a folder outside `media_dir`,
/// a name that is not safe UTF-8, a read failure, or a limit that is passed.
pub fn read_media_folder(
    media_dir: &Path,
    path: &str,
    max_files: usize,
    max_bytes: u64,
) -> Result<Vec<(String, Vec<u8>)>, FormatError> {
    let path = path.trim_end_matches('/');
    if !is_safe_relative_path(path) {
        return Err(FormatError::UnsafePath(path.to_owned()));
    }
    let root = canonical_root(media_dir)?;
    let start = media_dir.join(path);
    let resolved =
        std::fs::canonicalize(&start).map_err(|_| FormatError::UnsafePath(path.to_owned()))?;
    if !resolved.starts_with(&root) || !resolved.is_dir() {
        return Err(FormatError::UnsafePath(path.to_owned()));
    }
    let mut out: Vec<(String, Vec<u8>)> = Vec::new();
    let mut total = 0u64;
    let mut queue = vec![(start, path.to_owned(), 0usize)];
    while let Some((dir, prefix, depth)) = queue.pop() {
        if depth > MAX_DEPTH {
            return Err(FormatError::UnsafePath(prefix));
        }
        let entries = std::fs::read_dir(&dir).map_err(|e| io_error(&prefix, &e))?;
        for entry in entries {
            let entry = entry.map_err(|e| io_error(&prefix, &e))?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| FormatError::UnsafePath(format!("{prefix}/(not UTF-8)")))?;
            let rel = format!("{prefix}/{name}");
            if !is_safe_relative_path(&rel) {
                return Err(FormatError::UnsafePath(rel));
            }
            let entry_path = dir.join(&name);
            let kind = std::fs::symlink_metadata(&entry_path)
                .map_err(|e| io_error(&rel, &e))?
                .file_type();
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() {
                queue.push((entry_path, rel, depth + 1));
            } else if kind.is_file() {
                if out.len() >= max_files {
                    return Err(FormatError::TooManyEntries);
                }
                let declared = std::fs::metadata(&entry_path)
                    .map_err(|e| io_error(&rel, &e))?
                    .len();
                total = total.saturating_add(declared);
                if total > max_bytes {
                    return Err(FormatError::EntryTooLarge(rel));
                }
                let mut bytes = Vec::with_capacity(usize::try_from(declared).unwrap_or(0));
                std::fs::File::open(&entry_path)
                    .and_then(|file| {
                        file.take(declared.saturating_add(1))
                            .read_to_end(&mut bytes)
                    })
                    .map_err(|e| io_error(&rel, &e))?;
                if bytes.len() as u64 > declared {
                    return Err(FormatError::EntryTooLarge(rel));
                }
                out.push((rel, bytes));
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

// ---------------------------------------------------------------------- save

/// Save a course and its media into a project folder, writing only what
/// changed since `record` was taken. `record` is updated to match the folder.
///
/// # Errors
/// Returns [`FormatError`] when the course cannot be serialised, a media path is
/// unsafe or duplicated, or a write fails. A failed save leaves the folder
/// usable: the old `course.json` and every file it needs.
pub fn save_project_folder<B: AsRef<[u8]> + Sync>(
    dir: &Path,
    course: &Course,
    media: &[(String, B)],
    record: &mut FolderRecord,
) -> Result<SaveReport, FormatError> {
    save_project_folder_with(dir, course, media, record, &SaveHooks::default())
}

/// [`save_project_folder`] with injectable failure points for tests.
///
/// # Errors
/// Returns [`FormatError`] as [`save_project_folder`] does.
pub fn save_project_folder_with<B: AsRef<[u8]> + Sync>(
    dir: &Path,
    course: &Course,
    media: &[(String, B)],
    record: &mut FolderRecord,
    hooks: &SaveHooks<'_>,
) -> Result<SaveReport, FormatError> {
    let mut course_json = course.to_json()?;
    course_json.push('\n');

    let mut wanted = HashSet::new();
    for (path, _) in media {
        if !is_safe_relative_path(path) || path.ends_with(TEMP_SUFFIX) {
            return Err(FormatError::UnsafePath(path.clone()));
        }
        if !wanted.insert(format!("{MEDIA_DIR}/{path}").to_ascii_lowercase()) {
            return Err(FormatError::DuplicateEntry(path.clone()));
        }
    }

    check_fits(&course_json, media, hooks.limits)?;
    std::fs::create_dir_all(dir).map_err(|e| io_error(&dir.display().to_string(), &e))?;
    let mut writer = Writer {
        dir,
        record,
        hooks,
        report: SaveReport::default(),
        checked_dirs: HashSet::new(),
        case_stale: Vec::new(),
    };
    writer.put_all(media)?;
    (hooks.before_course_json)().map_err(|e| io_error(COURSE_FILE, &e))?;
    writer.put(COURSE_FILE, course_json.as_bytes())?;
    (hooks.before_delete)().map_err(|e| io_error(MEDIA_DIR, &e))?;
    writer.delete_unused(&wanted);
    Ok(writer.report)
}

/// Refuse a project that [`open_project_folder_with_limits`] would refuse, so a
/// save never leaves a folder that cannot be opened again.
fn check_fits<B: AsRef<[u8]>>(
    course_json: &str,
    media: &[(String, B)],
    limits: Limits,
) -> Result<(), FormatError> {
    if media.len().saturating_add(1) > limits.max_entries {
        return Err(FormatError::TooManyEntries);
    }
    let mut sizes = EntrySizes::default();
    let json_len = course_json.len() as u64;
    if json_len > entry_limit(false, limits) {
        return Err(FormatError::EntryTooLarge(COURSE_FILE.to_owned()));
    }
    sizes.add(json_len, false, limits)?;
    for (path, bytes) in media {
        let stored = is_precompressed(path);
        let len = bytes.as_ref().len() as u64;
        if len > entry_limit(stored, limits) {
            return Err(FormatError::EntryTooLarge(format!("{MEDIA_DIR}/{path}")));
        }
        sizes.add(len, stored, limits)?;
    }
    Ok(())
}

struct Writer<'a, 'h> {
    dir: &'a Path,
    record: &'a mut FolderRecord,
    hooks: &'a SaveHooks<'h>,
    report: SaveReport,
    /// Parent folders already checked to hold no symlink.
    checked_dirs: HashSet<String>,
    /// Recorded media files whose name differs from a file just written only by
    /// letter case, as (old path, new path). On a case-sensitive disk the old
    /// file is still there and must be deleted; on a case-insensitive one it is
    /// the same file.
    case_stale: Vec<(String, String)>,
}

impl Writer<'_, '_> {
    /// Write `rel` unless the record says the same content is already there.
    fn put(&mut self, rel: &str, bytes: &[u8]) -> Result<(), FormatError> {
        self.make_parents(rel)?;
        let known = self.record.files.get(rel);
        let result = write_if_changed(self.dir, rel, bytes, known, self.hooks.retry_delay);
        self.take(rel, result)
    }

    /// Write every media file, several at a time: hashing and flushing a large
    /// course is the whole cost of a save, and files are independent. The record
    /// and the report are filled in media order afterwards, so the result does
    /// not depend on which worker finished first.
    fn put_all<B: AsRef<[u8]> + Sync>(&mut self, media: &[(String, B)]) -> Result<(), FormatError> {
        let rels: Vec<String> = media
            .iter()
            .map(|(path, _)| format!("{MEDIA_DIR}/{path}"))
            .collect();
        for rel in &rels {
            self.make_parents(rel)?;
        }
        type Slot = Mutex<Option<Result<WriteOutcome, FormatError>>>;
        let slots: Vec<Slot> = rels.iter().map(|_| Mutex::new(None)).collect();
        let next = AtomicUsize::new(0);
        let workers = std::thread::available_parallelism()
            .map_or(1, usize::from)
            .clamp(1, 8)
            .min(media.len().max(1));
        let (dir, delay, known) = (self.dir, self.hooks.retry_delay, &self.record.files);
        std::thread::scope(|scope| {
            for _ in 0..workers {
                scope.spawn(|| {
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        let (Some(rel), Some((_, bytes)), Some(slot)) =
                            (rels.get(i), media.get(i), slots.get(i))
                        else {
                            break;
                        };
                        let result =
                            write_if_changed(dir, rel, bytes.as_ref(), known.get(rel), delay);
                        if let Ok(mut guard) = slot.lock() {
                            *guard = Some(result);
                        }
                    }
                });
            }
        });
        let mut first_error = None;
        for (rel, slot) in rels.iter().zip(slots) {
            let result = slot.into_inner().ok().flatten();
            if let Some(result) = result
                && let Err(error) = self.take(rel, result)
            {
                first_error.get_or_insert(error);
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    /// Fold the outcome of one write into the record and the report.
    fn take(
        &mut self,
        rel: &str,
        result: Result<WriteOutcome, FormatError>,
    ) -> Result<(), FormatError> {
        match result? {
            WriteOutcome::Unchanged => {}
            WriteOutcome::Rehash(record) => {
                self.forget_case_variants(rel);
                self.record.files.insert(rel.to_owned(), record);
            }
            WriteOutcome::Written(record) => {
                self.forget_case_variants(rel);
                self.record.files.insert(rel.to_owned(), record);
                self.report.written.push(rel.to_owned());
            }
        }
        Ok(())
    }

    /// Drop the record of files that differ from `rel` only by letter case, and
    /// remember the media ones so [`Self::delete_unused`] can remove them.
    fn forget_case_variants(&mut self, rel: &str) {
        let variants: Vec<String> = self
            .record
            .files
            .keys()
            .filter(|k| k.as_str() != rel && k.eq_ignore_ascii_case(rel))
            .cloned()
            .collect();
        for old in variants {
            self.record.files.remove(&old);
            if old.starts_with("media/") {
                self.case_stale.push((old, rel.to_owned()));
            }
        }
    }

    /// Delete the old-case twin of a renamed file, when the disk keeps both.
    fn delete_case_twins(&mut self) {
        for (old, new) in std::mem::take(&mut self.case_stale) {
            if exists_exact(self.dir, &old)
                && exists_exact(self.dir, &new)
                && std::fs::remove_file(self.dir.join(&old)).is_ok()
            {
                self.report.deleted.push(old);
            }
        }
    }

    /// Create the parent folders of `rel`, refusing to write through a symlink.
    fn make_parents(&mut self, rel: &str) -> Result<(), FormatError> {
        let Some((parent, _)) = rel.rsplit_once('/') else {
            return Ok(());
        };
        if self.checked_dirs.contains(parent) {
            return Ok(());
        }
        let mut current = self.dir.to_path_buf();
        let mut walked = String::new();
        for segment in parent.split('/') {
            current.push(segment);
            if !walked.is_empty() {
                walked.push('/');
            }
            walked.push_str(segment);
            match std::fs::symlink_metadata(&current) {
                Ok(meta) if meta.file_type().is_symlink() => {
                    return Err(FormatError::UnsafePath(walked));
                }
                Ok(meta) if meta.is_dir() => {}
                Ok(_) => return Err(FormatError::UnsafePath(walked)),
                Err(_) => std::fs::create_dir(&current).map_err(|e| io_error(&walked, &e))?,
            }
        }
        self.checked_dirs.insert(parent.to_owned());
        Ok(())
    }

    /// Delete recorded files under `media/` that the course no longer uses, and
    /// the folders that leaves empty. Nothing outside `media/` is ever deleted,
    /// and neither is a file the record does not know.
    fn delete_unused(&mut self, wanted: &HashSet<String>) {
        self.delete_case_twins();
        let unused: Vec<String> = self
            .record
            .files
            .keys()
            .filter(|rel| rel.starts_with("media/") && !wanted.contains(&rel.to_ascii_lowercase()))
            .cloned()
            .collect();
        let mut emptied = BTreeSet::new();
        for rel in unused {
            let path = self.dir.join(&rel);
            let gone = match std::fs::symlink_metadata(&path) {
                Ok(meta) if meta.is_file() || meta.file_type().is_symlink() => {
                    std::fs::remove_file(&path).is_ok()
                }
                Ok(_) => false,
                Err(_) => true,
            };
            if gone {
                self.record.files.remove(&rel);
                self.report.deleted.push(rel.clone());
                if let Some((parent, _)) = rel.rsplit_once('/') {
                    emptied.insert(parent.to_owned());
                }
            }
        }
        for mut parent in emptied {
            while parent.len() > MEDIA_DIR.len()
                && std::fs::remove_dir(self.dir.join(&parent)).is_ok()
            {
                self.checked_dirs.remove(&parent);
                match parent.rsplit_once('/') {
                    Some((up, _)) => parent = up.to_owned(),
                    None => break,
                }
            }
        }
    }
}

/// Whether `dir/rel` exists with exactly this letter case in every segment.
/// A case-insensitive disk answers true for any case; this does not.
fn exists_exact(dir: &Path, rel: &str) -> bool {
    let mut current = dir.to_path_buf();
    for segment in rel.split('/') {
        let Ok(entries) = std::fs::read_dir(&current) else {
            return false;
        };
        if !entries
            .filter_map(Result::ok)
            .any(|entry| entry.file_name().to_str() == Some(segment))
        {
            return false;
        }
        current.push(segment);
    }
    true
}

enum WriteOutcome {
    Unchanged,
    Rehash(FileRecord),
    Written(FileRecord),
}

/// Write `bytes` to `dir/rel` through a temporary file, a flush and a rename,
/// unless `known` says the same content is already there.
fn write_if_changed(
    dir: &Path,
    rel: &str,
    bytes: &[u8],
    known: Option<&FileRecord>,
    delay: Duration,
) -> Result<WriteOutcome, FormatError> {
    let path = dir.join(rel);
    let size = bytes.len() as u64;
    let hash = fast_hash64(bytes);
    if let Some(known) = known
        && known.size == size
        && std::fs::metadata(&path).is_ok_and(|m| {
            // A file replaced on disk with one of the same size (a git checkout)
            // has a new modified time: write ours back rather than keep theirs.
            m.is_file() && m.len() == size && m.modified().ok() == known.modified
        })
    {
        if known.hash == hash {
            return Ok(WriteOutcome::Unchanged);
        }
        if known.hash == fnv1a64(bytes) {
            return Ok(WriteOutcome::Rehash(FileRecord {
                size,
                hash,
                modified: known.modified,
            }));
        }
    }
    let tmp = temp_path(&path);
    let result = retrying(delay, || {
        // A stale or planted temp file (or symlink) is removed, never written
        // through: `create_new` refuses to follow anything already there.
        if std::fs::symlink_metadata(&tmp).is_ok() {
            std::fs::remove_file(&tmp).or_else(|_| std::fs::remove_dir(&tmp))?;
        }
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()
    })
    .and_then(|()| retrying(delay, || std::fs::rename(&tmp, &path)));
    if let Err(error) = result {
        let _ = std::fs::remove_file(&tmp);
        return Err(io_error(rel, &error));
    }
    sync_parent_dir(&path);
    Ok(WriteOutcome::Written(FileRecord {
        size,
        hash,
        modified: modified_time(&path),
    }))
}

fn temp_path(path: &Path) -> PathBuf {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    path.with_file_name(format!("{name}{TEMP_SUFFIX}"))
}

/// Whether an error is what a sync service or another app holding the file
/// produces: a permission or sharing violation.
fn is_lock_error(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::ResourceBusy
    ) || matches!(error.raw_os_error(), Some(32 | 33))
}

fn retrying(delay: Duration, mut step: impl FnMut() -> std::io::Result<()>) -> std::io::Result<()> {
    let mut attempt = 1;
    loop {
        match step() {
            Err(e) if is_lock_error(&e) && attempt < LOCK_ATTEMPTS => {
                attempt += 1;
                std::thread::sleep(delay);
            }
            other => return other,
        }
    }
}

/// Flush the folder entry so a rename survives a power cut. Unix only; an error
/// is ignored.
fn sync_parent_dir(path: &Path) {
    #[cfg(unix)]
    if let Some(parent) = path.parent()
        && let Ok(handle) = std::fs::File::open(parent)
    {
        let _ = handle.sync_all();
    }
    #[cfg(not(unix))]
    let _ = path;
}

// -------------------------------------------------------------------- changes

/// The files that no longer match `record` on disk: changed, replaced or
/// deleted by something other than this app, such as a teammate's `git pull`.
/// Cheap: it compares size and modified time, and hashes a file only when its
/// time moved but its size did not (a checkout that rewrote identical bytes is
/// not a change, and its new time is taken into the record). Files that are new
/// to the folder are not listed. Sorted by name.
pub fn changed_files(dir: &Path, record: &mut FolderRecord) -> Vec<String> {
    let mut changed = Vec::new();
    for (rel, known) in &mut record.files {
        let path = dir.join(rel);
        let Ok(meta) = std::fs::metadata(&path) else {
            changed.push(rel.clone());
            continue;
        };
        if meta.len() != known.size {
            changed.push(rel.clone());
        } else if meta.modified().ok() != known.modified {
            match std::fs::read(&path) {
                Ok(bytes) if fast_hash64(&bytes) == known.hash => {
                    known.modified = meta.modified().ok();
                }
                Ok(bytes) if fnv1a64(&bytes) == known.hash => {
                    known.hash = fast_hash64(&bytes);
                    known.modified = meta.modified().ok();
                }
                _ => changed.push(rel.clone()),
            }
        }
    }
    changed
}
