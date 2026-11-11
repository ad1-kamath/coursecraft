//! Version history: each manual save first copies the file it replaces into
//! `<config_dir>/versions/<hash of the project path>/`, outside the project's
//! folder so a sync service does not carry the copies around. Every function
//! takes the config directory as a parameter, so tests never touch the real one.

use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Most bytes of versions kept for one project (5 GB). The oldest go first.
pub const CAP_BYTES: u64 = 5 * 1024 * 1024 * 1024;

/// FNV-1a, 64 bit. Written out by hand because `DefaultHasher` may change
/// between Rust versions, which would cut a project off from its old versions.
#[must_use]
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// The project's folder name under `versions/`: the FNV-1a hash of its absolute
/// path's UTF-8 bytes, as 16 hex digits.
#[must_use]
pub fn project_hash(project: &Path) -> String {
    let absolute = std::path::absolute(project).unwrap_or_else(|_| project.to_path_buf());
    format!("{:016x}", fnv1a64(absolute.to_string_lossy().as_bytes()))
}

/// Where the versions of `project` live.
#[must_use]
pub fn versions_dir(config_dir: &Path, project: &Path) -> PathBuf {
    config_dir.join("versions").join(project_hash(project))
}

/// One kept version of a project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    /// The file stem, such as `20261111-153045`. Also the sort key.
    pub id: String,
    /// The version file.
    pub path: PathBuf,
    /// Size in bytes.
    pub size: u64,
    /// Slide count, or `None` when the version cannot be read.
    pub slides: Option<usize>,
}

impl Version {
    /// The date and time for people, such as `2026-11-11 15:30:45 UTC`.
    #[must_use]
    pub fn when(&self) -> String {
        display_when(&self.id)
    }
}

/// Year, month, day, hour, minute, second in UTC for seconds since the epoch.
fn utc_parts(secs: u64) -> (i64, i64, i64, u64, u64, u64) {
    // Stop at the year 9999 so nothing below can overflow.
    let secs = secs.min(253_402_300_799);
    let days = i64::try_from(secs / 86_400).unwrap_or(0);
    let rest = secs % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day, rest / 3_600, (rest / 60) % 60, rest % 60)
}

/// The version id for a moment: `YYYYMMDD-HHMMSS` in UTC.
#[must_use]
pub fn timestamp_id(time: SystemTime) -> String {
    let secs = time
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let (y, mo, d, h, mi, s) = utc_parts(secs);
    format!("{y:04}{mo:02}{d:02}-{h:02}{mi:02}{s:02}")
}

/// `2026-11-11 15:30:45 UTC` for an id; the id itself when it is not a timestamp.
#[must_use]
pub fn display_when(id: &str) -> String {
    let parts = (
        id.get(0..4),
        id.get(4..6),
        id.get(6..8),
        id.get(9..11),
        id.get(11..13),
        id.get(13..15),
    );
    match parts {
        (Some(y), Some(mo), Some(d), Some(h), Some(mi), Some(s)) if id.get(8..9) == Some("-") => {
            format!("{y}-{mo}-{d} {h}:{mi}:{s} UTC")
        }
        _ => id.to_string(),
    }
}

/// `1.2 MB`, `850 KB`, `12 bytes`.
#[must_use]
pub fn format_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    let b = bytes as f64;
    if b >= KB * KB * KB {
        format!("{:.1} GB", b / (KB * KB * KB))
    } else if b >= KB * KB {
        format!("{:.1} MB", b / (KB * KB))
    } else if b >= KB {
        format!("{:.0} KB", b / KB)
    } else {
        format!("{bytes} bytes")
    }
}

/// An id is a timestamp, possibly with a `-N` tiebreak: digits and dashes only.
/// This keeps an id from the dialog from naming a path outside the folder.
#[must_use]
pub fn id_is_valid(id: &str) -> bool {
    !id.is_empty() && id.len() <= 40 && id.chars().all(|c| c.is_ascii_digit() || c == '-')
}

/// Version ids of `project`, newest first.
fn ids(config_dir: &Path, project: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(versions_dir(config_dir, project)) else {
        return Vec::new();
    };
    let mut ids: Vec<String> = entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let stem = name.strip_suffix(".ccraft")?;
            id_is_valid(stem).then(|| stem.to_string())
        })
        .collect();
    ids.sort_unstable_by(|a, b| b.cmp(a));
    ids
}

fn version_file(config_dir: &Path, project: &Path, id: &str) -> PathBuf {
    versions_dir(config_dir, project).join(format!("{id}.ccraft"))
}

fn describe(config_dir: &Path, project: &Path, id: &str) -> Version {
    let path = version_file(config_dir, project, id);
    let size = std::fs::metadata(&path).map_or(0, |m| m.len());
    let slides = std::fs::File::open(&path)
        .ok()
        .and_then(|f| coursecraft_format::read_course(std::io::BufReader::new(f)).ok())
        .map(|course| course.slides().count());
    Version {
        id: id.to_string(),
        path,
        size,
        slides,
    }
}

/// Every kept version of `project`, newest first, each opened just far enough
/// to count its slides.
#[must_use]
pub fn list_versions(config_dir: &Path, project: &Path) -> Vec<Version> {
    ids(config_dir, project)
        .iter()
        .map(|id| describe(config_dir, project, id))
        .collect()
}

/// The newest version that can still be read, if any.
#[must_use]
pub fn latest_readable(config_dir: &Path, project: &Path) -> Option<Version> {
    ids(config_dir, project)
        .iter()
        .map(|id| describe(config_dir, project, id))
        .find(|v| v.slides.is_some())
}

/// The file for version `id`, if the id is well formed and the file exists.
#[must_use]
pub fn version_path(config_dir: &Path, project: &Path, id: &str) -> Option<PathBuf> {
    if !id_is_valid(id) {
        return None;
    }
    let path = version_file(config_dir, project, id);
    path.is_file().then_some(path)
}

/// Delete one version.
///
/// # Errors
/// Returns a plain sentence when the id is not a version or the file cannot be removed.
pub fn delete_version(config_dir: &Path, project: &Path, id: &str) -> Result<(), String> {
    let path = version_path(config_dir, project, id)
        .ok_or_else(|| "That version no longer exists.".to_string())?;
    std::fs::remove_file(&path).map_err(|e| format!("Couldn't delete that version: {e}"))
}

/// Whether two files hold the same bytes, compared in chunks.
fn same_bytes(a: &Path, b: &Path) -> std::io::Result<bool> {
    if std::fs::metadata(a)?.len() != std::fs::metadata(b)?.len() {
        return Ok(false);
    }
    let mut fa = std::io::BufReader::new(std::fs::File::open(a)?);
    let mut fb = std::io::BufReader::new(std::fs::File::open(b)?);
    let (mut ba, mut bb) = (vec![0u8; 64 * 1024], vec![0u8; 64 * 1024]);
    loop {
        let na = fa.read(&mut ba)?;
        if na == 0 {
            return Ok(true);
        }
        // Equal lengths: read the same count from the other file.
        let mut filled = 0;
        while filled < na {
            let Some(slot) = bb.get_mut(filled..na) else {
                return Ok(false);
            };
            let n = fb.read(slot)?;
            if n == 0 {
                return Ok(false);
            }
            filled += n;
        }
        if ba.get(..na) != bb.get(..na) {
            return Ok(false);
        }
    }
}

/// Copy the file about to be replaced into the project's version folder, then
/// prune. Returns the new version's id, or `None` when nothing was made: there
/// is no file yet (the first save), versions are off, or the file is
/// identical to the newest version.
///
/// # Errors
/// Returns a message when the copy fails. The caller logs it and saves anyway.
pub fn snapshot_before_save(
    config_dir: &Path,
    project: &Path,
    keep: u32,
    title: &str,
) -> Result<Option<String>, String> {
    snapshot_with(
        config_dir,
        project,
        keep,
        title,
        CAP_BYTES,
        SystemTime::now(),
    )
}

/// [`snapshot_before_save`] with the byte cap and the clock passed in.
///
/// # Errors
/// Returns a message when the copy fails.
pub fn snapshot_with(
    config_dir: &Path,
    project: &Path,
    keep: u32,
    title: &str,
    cap_bytes: u64,
    now: SystemTime,
) -> Result<Option<String>, String> {
    if keep == 0 || !project.is_file() {
        return Ok(None);
    }
    let dir = versions_dir(config_dir, project);
    let existing = ids(config_dir, project);
    if let Some(newest) = existing.first()
        && same_bytes(project, &version_file(config_dir, project, newest)).unwrap_or(false)
    {
        return Ok(None);
    }

    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let base = timestamp_id(now);
    let mut id = base.clone();
    let mut n = 0u32;
    while version_file(config_dir, project, &id).exists() {
        n = n.saturating_add(1);
        id = format!("{base}-{n}");
    }
    let target = version_file(config_dir, project, &id);
    let part = dir.join(format!("{id}.ccraft.part"));
    let copied = std::fs::copy(project, &part).and_then(|_| std::fs::rename(&part, &target));
    if let Err(e) = copied {
        let _ = std::fs::remove_file(&part);
        return Err(e.to_string());
    }
    write_index(&dir, project, title);
    prune(config_dir, project, keep, cap_bytes);
    Ok(Some(id))
}

/// `index.json`: the original path and the course title, for people who look in
/// the folder. Best effort.
fn write_index(dir: &Path, project: &Path, title: &str) {
    let index = serde_json::json!({
        "path": std::path::absolute(project)
            .unwrap_or_else(|_| project.to_path_buf())
            .to_string_lossy(),
        "title": title,
    });
    if let Ok(text) = serde_json::to_string_pretty(&index) {
        let _ = crate::files::write_atomic(&dir.join("index.json"), text.as_bytes());
    }
}

/// Keep the newest `keep` versions and at most `cap_bytes` in all. The newest
/// version always stays.
pub fn prune(config_dir: &Path, project: &Path, keep: u32, cap_bytes: u64) {
    let ids = ids(config_dir, project);
    let keep = usize::try_from(keep).unwrap_or(usize::MAX);
    for id in ids.iter().skip(keep) {
        let _ = std::fs::remove_file(version_file(config_dir, project, id));
    }
    let kept: Vec<&String> = ids.iter().take(keep).collect();
    let sizes: Vec<u64> = kept
        .iter()
        .map(|id| std::fs::metadata(version_file(config_dir, project, id)).map_or(0, |m| m.len()))
        .collect();
    let mut total: u64 = sizes.iter().fold(0, |acc, s| acc.saturating_add(*s));
    // Oldest last in `kept`: drop from the back while over the cap.
    for (id, size) in kept.iter().zip(sizes.iter()).skip(1).rev() {
        if total <= cap_bytes {
            break;
        }
        let _ = std::fs::remove_file(version_file(config_dir, project, id));
        total = total.saturating_sub(*size);
    }
}
