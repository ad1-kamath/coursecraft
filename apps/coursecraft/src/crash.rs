//! The crash note: a small text file a panic leaves in CourseCraft's config
//! folder, offered for reporting the next time the app starts.
//!
//! The note holds the time, version, OS and the panic message and location,
//! with paths stripped. Nothing is sent anywhere; the person decides whether to
//! report it from Help > Report a Problem.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use coursecraft_ui_egui::report::{limit_crash_note, scrub};

use crate::versions::{display_when, timestamp_id};

/// The file name inside the config folder.
pub const CRASH_FILE: &str = "last-crash.txt";

/// Where the note lives.
#[must_use]
pub fn crash_file_path(config_dir: &Path) -> PathBuf {
    config_dir.join(CRASH_FILE)
}

/// The note text for one panic. Paths and file names are stripped.
#[must_use]
pub fn format_crash_note(time: SystemTime, message: &str, location: &str) -> String {
    let note = format!(
        "Time: {}\nCourseCraft {}\nOS: {} ({})\nPanic: {}\nLocation: {}\n",
        display_when(&timestamp_id(time)),
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        scrub(message),
        scrub(location),
    );
    limit_crash_note(&note)
}

/// Write the note, replacing an older one. Errors are returned, not raised.
pub fn write_crash_note(config_dir: &Path, note: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(config_dir)?;
    std::fs::write(crash_file_path(config_dir), note)
}

/// The note a crash left, if any.
#[must_use]
pub fn read_crash_note(config_dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(crash_file_path(config_dir)).ok()?;
    (!text.trim().is_empty()).then_some(text)
}

/// Delete the note once it was reported or dismissed.
pub fn delete_crash_note(config_dir: &Path) {
    let _ = std::fs::remove_file(crash_file_path(config_dir));
}

/// The panic's message, whatever type it carries.
fn panic_message(info: &std::panic::PanicHookInfo<'_>) -> String {
    let payload = info.payload();
    if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_string()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        "(no message)".to_string()
    }
}

/// Write a crash note on every panic, then run the hook that was there
/// before, so the usual panic message still prints.
pub fn install_panic_hook(config_dir: Option<PathBuf>) {
    let Some(dir) = config_dir else {
        return;
    };
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_default();
        let note = format_crash_note(SystemTime::now(), &panic_message(info), &location);
        let _ = write_crash_note(&dir, &note);
        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn scratch(sub: &str) -> PathBuf {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/agy-scratch")
            .join(sub);
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    #[test]
    fn note_has_fields_and_no_paths() {
        let time = SystemTime::UNIX_EPOCH + Duration::from_secs(1_794_411_045);
        let note = format_crash_note(
            time,
            "could not open /Users/bob/Secret Plan/course.ccraft",
            "/Users/bob/work/src/x.rs:10:5",
        );
        assert!(note.contains("Time: 2026-11-"));
        assert!(note.contains(env!("CARGO_PKG_VERSION")));
        assert!(note.contains(std::env::consts::OS));
        assert!(note.contains("Panic: could not open <path>"));
        assert!(note.contains("Location: <path>"));
        assert!(!note.contains("bob") && !note.contains("Secret"));
    }

    #[test]
    fn file_round_trip_write_read_delete() {
        let dir = scratch("test-crash-note");
        assert_eq!(read_crash_note(&dir), None);
        write_crash_note(&dir, "first\n").unwrap();
        write_crash_note(&dir, "second\n").unwrap();
        assert_eq!(read_crash_note(&dir).as_deref(), Some("second\n"));
        assert!(dir.join(CRASH_FILE).exists());
        delete_crash_note(&dir);
        assert_eq!(read_crash_note(&dir), None);
        delete_crash_note(&dir);
    }

    #[test]
    fn blank_file_counts_as_no_note() {
        let dir = scratch("test-crash-blank");
        write_crash_note(&dir, "  \n").unwrap();
        assert_eq!(read_crash_note(&dir), None);
    }
}
