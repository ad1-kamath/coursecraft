//! Periodic autosave and recovery management.

use coursecraft_course::Course;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::SystemTime;

/// Pid used in the name of an autosave that has no owner (a migrated legacy file).
/// No process has pid 0 in a user session, so such a file counts as orphaned.
const NO_OWNER_PID: u32 = 0;

/// The file name of the autosave that older builds shared between all untitled
/// courses. It is migrated once into an ownerless session file.
const LEGACY_UNTITLED_NAME: &str = "untitled.autosave.ccraft";

const UNTITLED_PREFIX: &str = "untitled-";
const AUTOSAVE_SUFFIX: &str = ".autosave.ccraft";
const OWNER_SUFFIX: &str = ".owner";

/// Identity of this app run: process id and start time (seconds since the epoch).
/// Each run's untitled course autosaves to its own file, so two windows never
/// overwrite each other and one never offers the other's live autosave.
#[must_use]
pub fn session_id() -> &'static (u32, u64) {
    static ID: std::sync::OnceLock<(u32, u64)> = std::sync::OnceLock::new();
    ID.get_or_init(|| {
        let start = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        (std::process::id(), start)
    })
}

/// The untitled autosave file of the session `pid` / `start` in `config_dir`.
#[must_use]
pub fn untitled_autosave_path_for(config_dir: &Path, pid: u32, start: u64) -> PathBuf {
    config_dir.join(format!("{UNTITLED_PREFIX}{pid}-{start}{AUTOSAVE_SUFFIX}"))
}

/// The untitled autosave file of this run.
#[must_use]
pub fn untitled_autosave_path(config_dir: &Path) -> PathBuf {
    let (pid, start) = *session_id();
    untitled_autosave_path_for(config_dir, pid, start)
}

/// The owner marker next to an untitled autosave: a small file holding the pid
/// of the process that writes it.
fn owner_marker_path(autosave_path: &Path) -> PathBuf {
    let mut name = autosave_path.as_os_str().to_owned();
    name.push(OWNER_SUFFIX);
    PathBuf::from(name)
}

/// Record this process as the owner of `autosave_path`. Best effort.
pub fn write_owner_marker(autosave_path: &Path) {
    let _ = std::fs::write(owner_marker_path(autosave_path), session_id().0.to_string());
}

/// The pid that owns an untitled autosave: the marker's pid, else the pid in the
/// file name. `None` when the name is not an untitled session autosave.
fn owner_pid(autosave_path: &Path) -> Option<u32> {
    let name = autosave_path.file_name()?.to_str()?;
    let rest = name
        .strip_prefix(UNTITLED_PREFIX)?
        .strip_suffix(AUTOSAVE_SUFFIX)?;
    let name_pid: u32 = rest.split('-').next()?.parse().ok()?;
    let marker_pid = std::fs::read_to_string(owner_marker_path(autosave_path))
        .ok()
        .and_then(|text| text.trim().parse::<u32>().ok());
    Some(marker_pid.unwrap_or(name_pid))
}

/// Delete an untitled autosave together with its owner marker.
pub fn delete_untitled_autosave(autosave_path: &Path) {
    delete_autosave(autosave_path);
    let marker = owner_marker_path(autosave_path);
    if marker.exists() {
        let _ = std::fs::remove_file(marker);
    }
}

/// Whether the process `pid` is running. When the check cannot be made, the
/// process counts as running, so its autosave is never offered by mistake.
#[must_use]
pub fn process_is_alive(pid: u32) -> bool {
    if pid == NO_OWNER_PID {
        return false;
    }
    if pid == std::process::id() {
        return true;
    }
    #[cfg(windows)]
    {
        let filter = format!("PID eq {pid}");
        let Ok(output) = std::process::Command::new("tasklist")
            .args(["/FI", &filter, "/NH", "/FO", "CSV"])
            .stdin(std::process::Stdio::null())
            .output()
        else {
            return true;
        };
        if !output.status.success() {
            return true;
        }
        String::from_utf8_lossy(&output.stdout).contains(&format!("\"{pid}\""))
    }
    #[cfg(not(windows))]
    {
        // `ps -p` succeeds exactly when the process exists.
        std::process::Command::new("ps")
            .args(["-p", &pid.to_string()])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map_or(true, |status| status.success())
    }
}

/// Move the shared autosave of older builds to an ownerless session file, so
/// it is offered for restore once like any orphan. Best effort.
fn migrate_legacy_untitled(config_dir: &Path) {
    let legacy = config_dir.join(LEGACY_UNTITLED_NAME);
    if !legacy.is_file() {
        return;
    }
    let start = std::fs::metadata(&legacy)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs());
    let target = untitled_autosave_path_for(config_dir, NO_OWNER_PID, start);
    if !target.exists() {
        let _ = std::fs::rename(&legacy, &target);
    }
}

/// Untitled autosaves in `config_dir` whose owner is no longer running, newest
/// first. `is_alive` tells whether a pid is running. The shared autosave of older
/// builds is migrated first.
#[must_use]
pub fn orphaned_untitled_autosaves(
    config_dir: &Path,
    is_alive: &dyn Fn(u32) -> bool,
) -> Vec<PathBuf> {
    migrate_legacy_untitled(config_dir);
    let Ok(entries) = std::fs::read_dir(config_dir) else {
        return Vec::new();
    };
    let mut found: Vec<(SystemTime, PathBuf)> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .filter(|path| owner_pid(path).is_some_and(|pid| !is_alive(pid)))
        .map(|path| {
            let time = std::fs::metadata(&path)
                .and_then(|m| m.modified())
                .unwrap_or(std::time::UNIX_EPOCH);
            (time, path)
        })
        .collect();
    found.sort_by_key(|entry| std::cmp::Reverse(entry.0));
    found.into_iter().map(|(_, path)| path).collect()
}

/// Compute the autosave path for a named or untitled project. An untitled
/// project autosaves to this run's own file in `config_dir`.
#[must_use]
pub fn autosave_path_for_project(project_path: Option<&Path>, config_dir: &Path) -> PathBuf {
    match project_path {
        Some(path) => {
            let file_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("project");
            let base = file_name.strip_suffix(".ccraft").unwrap_or(file_name);
            path.with_file_name(format!("{base}.autosave.ccraft"))
        }
        None => untitled_autosave_path(config_dir),
    }
}

/// Check if an autosave file exists and is strictly newer than the project file.
#[must_use]
pub fn is_autosave_newer(project_path: &Path, autosave_path: &Path) -> bool {
    let Ok(auto_meta) = std::fs::metadata(autosave_path) else {
        return false;
    };
    let Ok(auto_time) = auto_meta.modified() else {
        return false;
    };
    // A project folder's own time moves when any file is added; its course.json
    // is what an edit changes.
    let file_path = if project_path.is_dir() {
        project_path.join("course.json")
    } else {
        project_path.to_path_buf()
    };
    let Ok(file_meta) = std::fs::metadata(&file_path) else {
        return false;
    };
    let Ok(file_time) = file_meta.modified() else {
        return false;
    };
    auto_time > file_time
}

/// Format a system time as a readable string for the restore dialog.
#[must_use]
pub fn format_system_time(time: SystemTime) -> String {
    let dur = match time.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d,
        Err(_) => return "unknown time".to_string(),
    };
    let secs = dur.as_secs();
    let sec = secs % 60;
    let min = (secs / 60) % 60;
    let hour = (secs / 3600) % 24;
    let mut days = (secs / 86400) as i64;

    let mut year = 1970;
    loop {
        let leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
        let days_in_year = if leap { 366 } else { 365 };
        if days >= days_in_year {
            days -= days_in_year;
            year += 1;
        } else {
            break;
        }
    }

    let leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
    let days_in_months = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];

    let mut month = 1;
    for &dim in &days_in_months {
        if days >= dim {
            days -= dim;
            month += 1;
        } else {
            break;
        }
    }
    let day = days + 1;

    format!("{year:04}-{month:02}-{day:02} {hour:02}:{min:02}:{sec:02} UTC")
}

/// Write an autosave archive to disk.
///
/// # Errors
/// Returns an error if directory creation or file writing fails.
pub fn write_autosave(
    course: &Course,
    media: &[(String, Vec<u8>)],
    autosave_path: &Path,
) -> Result<(), String> {
    write_autosave_slices(course, media, autosave_path)
}

/// Write an autosave archive to disk from `Arc`-shared media without copying.
///
/// # Errors
/// Returns an error if directory creation or file writing fails.
pub fn write_autosave_arc(
    course: &Course,
    media: &[(String, std::sync::Arc<[u8]>)],
    autosave_path: &Path,
) -> Result<(), String> {
    write_autosave_slices(course, media, autosave_path)
}

/// Write an autosave archive to disk from `Arc`-shared media without copying,
/// aborting before renaming over `autosave_path` if `save_epoch` has moved past
/// `expected_epoch`.
///
/// Returns `Ok(true)` if the autosave was committed to `autosave_path`, or
/// `Ok(false)` if it was aborted because `save_epoch` changed.
///
/// # Errors
/// Returns an error if serialization, verification, or filesystem writing fails.
pub fn write_autosave_arc_guarded(
    course: &Course,
    media: &[(String, Arc<[u8]>)],
    autosave_path: &Path,
    save_epoch: &Arc<AtomicU64>,
    expected_epoch: u64,
) -> Result<bool, String> {
    write_autosave_slices_guarded_with_after_write(
        course,
        media,
        autosave_path,
        save_epoch,
        expected_epoch,
        &|_| Ok(()),
    )
}

pub(crate) fn write_autosave_slices_guarded_with_after_write<B: AsRef<[u8]>>(
    course: &Course,
    media: &[(String, B)],
    autosave_path: &Path,
    save_epoch: &Arc<AtomicU64>,
    expected_epoch: u64,
    after_write: &dyn Fn(&Path) -> std::io::Result<()>,
) -> Result<bool, String> {
    if save_epoch.load(Ordering::Acquire) != expected_epoch {
        return Ok(false);
    }
    if let Some(parent) = autosave_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let committed = AtomicBool::new(false);
    let should_commit = || {
        let ok = save_epoch.load(Ordering::Acquire) == expected_epoch;
        committed.store(ok, Ordering::Release);
        ok
    };
    let hooks = crate::files::Hooks {
        after_write,
        should_commit: &should_commit,
        ..crate::files::Hooks::default()
    };
    // Stream the zip to a temp file and verify it before renaming over the
    // previous autosave, avoiding holding the whole archive in memory.
    crate::files::write_atomic_stream_with(
        autosave_path,
        &mut |file| {
            let mut buf = std::io::BufWriter::with_capacity(256 * 1024, file);
            coursecraft_format::save_project_to(course, media, &mut buf).map_err(|e| {
                std::io::Error::other(format!("Autosave serialization failed: {e}"))
            })?;
            std::io::Write::flush(&mut buf)
        },
        Some(&crate::files::verify_ccraft_file),
        &hooks,
    )
    .map_err(|e| format!("Autosave write failed: {e}"))?;
    if !committed.load(Ordering::Acquire) {
        return Ok(false);
    }
    crate::files::remove_stale_temps(autosave_path);
    Ok(true)
}

fn write_autosave_slices<B: AsRef<[u8]>>(
    course: &Course,
    media: &[(String, B)],
    autosave_path: &Path,
) -> Result<(), String> {
    if let Some(parent) = autosave_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    // Stream the zip to a temp file and verify it before renaming over the
    // previous autosave, avoiding holding the whole archive in memory.
    crate::files::write_atomic_stream_with(
        autosave_path,
        &mut |file| {
            let mut buf = std::io::BufWriter::with_capacity(256 * 1024, file);
            coursecraft_format::save_project_to(course, media, &mut buf).map_err(|e| {
                std::io::Error::other(format!("Autosave serialization failed: {e}"))
            })?;
            std::io::Write::flush(&mut buf)
        },
        Some(&crate::files::verify_ccraft_file),
        &crate::files::Hooks::default(),
    )
    .map_err(|e| format!("Autosave write failed: {e}"))?;
    crate::files::remove_stale_temps(autosave_path);
    Ok(())
}

/// Delete an autosave file if it exists.
pub fn delete_autosave(autosave_path: &Path) {
    if autosave_path.exists() {
        let _ = std::fs::remove_file(autosave_path);
    }
}

/// Seconds between autosave checks.
pub const AUTOSAVE_INTERVAL_SECS: f64 = 60.0;

/// Autosave path for a project, or `None` for an untitled project when there is
/// no config directory (untitled autosaves are skipped rather than written to the
/// working directory).
#[must_use]
pub fn autosave_target_path(
    project_path: Option<&Path>,
    config_dir: Option<&Path>,
) -> Option<PathBuf> {
    match (project_path, config_dir) {
        (Some(p), _) => Some(autosave_path_for_project(Some(p), Path::new(""))),
        (None, Some(dir)) => Some(autosave_path_for_project(None, dir)),
        (None, None) => None,
    }
}

/// Seconds until the next autosave check is due (never negative).
#[must_use]
pub fn secs_until_next_autosave(current_time: f64, last_autosave_time: f64) -> f64 {
    (AUTOSAVE_INTERVAL_SECS - (current_time - last_autosave_time)).max(0.0)
}

/// Target configuration and content for autosaving.
pub struct AutosaveTarget<'a> {
    pub project_path: Option<&'a Path>,
    pub config_dir: Option<&'a Path>,
    pub course: &'a Course,
}

/// Periodic autosave timer and revision tracking.
#[derive(Debug, Clone, Default)]
pub struct Autosaver {
    pub last_autosave_time: f64,
    pub last_autosaved_revision: Option<u64>,
    /// Minutes between autosaves. `None` keeps the default; `Some(0)` turns it off.
    pub interval_minutes: Option<u32>,
}

impl Autosaver {
    /// Create a new [`Autosaver`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Seconds between autosaves, or `None` when autosave is off.
    #[must_use]
    pub fn interval_secs(&self) -> Option<f64> {
        match self.interval_minutes {
            None => Some(AUTOSAVE_INTERVAL_SECS),
            Some(0) => None,
            Some(minutes) => Some(f64::from(minutes.min(60)) * 60.0),
        }
    }

    /// Seconds until the next autosave is due, or `None` when autosave is off.
    #[must_use]
    pub fn secs_until_next(&self, current_time: f64) -> Option<f64> {
        let interval = self.interval_secs()?;
        Some((interval - (current_time - self.last_autosave_time)).max(0.0))
    }

    /// Check if an autosave is due now, advancing the timer when the interval has elapsed.
    ///
    /// Returns `true` when the autosave interval has elapsed, the project is dirty,
    /// and `current_revision` has not already been autosaved.
    pub fn take_due(&mut self, current_time: f64, is_dirty: bool, current_revision: u64) -> bool {
        let Some(interval) = self.interval_secs() else {
            return false;
        };
        if current_time - self.last_autosave_time < interval {
            return false;
        }
        self.last_autosave_time = current_time;
        is_dirty && self.last_autosaved_revision != Some(current_revision)
    }

    /// Check if autosave should be performed and write it if so.
    ///
    /// `media` is only invoked when an autosave is actually about to be written, so
    /// callers never pay for copying media bytes on idle frames.
    ///
    /// Returns true if an autosave was performed this call.
    pub fn maybe_autosave(
        &mut self,
        current_time: f64,
        is_dirty: bool,
        current_revision: u64,
        target: AutosaveTarget<'_>,
        media: impl FnOnce() -> Vec<(String, Vec<u8>)>,
    ) -> bool {
        self.maybe_autosave_slices(current_time, is_dirty, current_revision, target, media)
    }

    /// Check if autosave should be performed and stream it using `Arc`-shared media.
    pub fn maybe_autosave_arc(
        &mut self,
        current_time: f64,
        is_dirty: bool,
        current_revision: u64,
        target: AutosaveTarget<'_>,
        media: impl FnOnce() -> Vec<(String, std::sync::Arc<[u8]>)>,
    ) -> bool {
        self.maybe_autosave_slices(current_time, is_dirty, current_revision, target, media)
    }

    fn maybe_autosave_slices<B: AsRef<[u8]>>(
        &mut self,
        current_time: f64,
        is_dirty: bool,
        current_revision: u64,
        target: AutosaveTarget<'_>,
        media: impl FnOnce() -> Vec<(String, B)>,
    ) -> bool {
        if !self.take_due(current_time, is_dirty, current_revision) {
            return false;
        }
        let Some(auto_path) = autosave_target_path(target.project_path, target.config_dir) else {
            return false;
        };
        if write_autosave_slices(target.course, &media(), &auto_path).is_ok() {
            self.last_autosaved_revision = Some(current_revision);
            return true;
        }
        false
    }
}
