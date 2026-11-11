//! Periodic autosave and recovery management.

use coursecraft_course::Course;
use coursecraft_format::save_project;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Compute the autosave path for a named or untitled project.
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
        None => config_dir.join("untitled.autosave.ccraft"),
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
    if let Some(parent) = autosave_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let bytes =
        save_project(course, media).map_err(|e| format!("Autosave serialization failed: {e}"))?;
    // The same checked path as a manual save, but a different file: autosave
    // never prompts and never touches the project file itself.
    crate::files::write_atomic_with(
        autosave_path,
        &bytes,
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
        let Some(interval) = self.interval_secs() else {
            return false;
        };
        if current_time - self.last_autosave_time < interval {
            return false;
        }
        self.last_autosave_time = current_time;

        if is_dirty && self.last_autosaved_revision != Some(current_revision) {
            let Some(auto_path) = autosave_target_path(target.project_path, target.config_dir)
            else {
                return false;
            };
            if write_autosave(target.course, &media(), &auto_path).is_ok() {
                self.last_autosaved_revision = Some(current_revision);
                return true;
            }
        }
        false
    }
}
