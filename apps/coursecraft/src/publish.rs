//! Publishing from the desktop editor: no egui, so the package logic stays testable.

use std::collections::HashMap;
use std::path::PathBuf;

use coursecraft_course::Course;
use coursecraft_publish::{
    PackageFile, PackageInput, PackageStandard, PublishError, build_package_standard_with_folders,
    zip_package_to,
};
use coursecraft_scorm::Standard;
use coursecraft_ui_egui::PublishStandard;
use coursecraft_xapi::Flavor;

use crate::files::MediaItem;

/// Everything a background publish needs. It owns the course and media, so a
/// publish never borrows the live session while the window keeps drawing.
pub struct PublishJob {
    pub course: Course,
    pub media: Vec<MediaItem>,
    pub standard: PackageStandard,
    pub player: Vec<PackageFile>,
    pub out: PathBuf,
}

/// The written file and its size in bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishOutcome {
    pub path: PathBuf,
    pub bytes: u64,
}

/// Map the UI's small standard enum onto the publishing crate's type.
#[must_use]
pub fn package_standard(standard: PublishStandard) -> PackageStandard {
    match standard {
        PublishStandard::Scorm12 => PackageStandard::Scorm(Standard::Scorm12),
        PublishStandard::Scorm2004 => PackageStandard::Scorm(Standard::Scorm2004),
        PublishStandard::Cmi5 => PackageStandard::Xapi(Flavor::Cmi5),
        PublishStandard::Xapi => PackageStandard::Xapi(Flavor::Xapi),
        PublishStandard::Lti => PackageStandard::Lti,
    }
}

/// Build the package files for a job, resolving media from the in-memory items
/// exactly like the CLI's `.ccraft` path.
///
/// # Errors
/// Returns a plain sentence naming the object and file when media is missing,
/// or describing another package problem.
pub(crate) fn package_files(job: &PublishJob) -> Result<Vec<PackageFile>, String> {
    let course_json = job
        .course
        .to_json()
        .map_err(|e| format!("Couldn't prepare the course for publishing: {e}"))?;
    let mut course_json = course_json;
    // Save writes a trailing newline; publish stays byte-identical to that form.
    course_json.push('\n');

    let media: HashMap<&str, &[u8]> = job
        .media
        .iter()
        .map(|(name, bytes)| (name.as_str(), bytes.as_slice()))
        .collect();
    let resolve = |src: &str| media.get(src).map(|bytes| bytes.to_vec());
    let folder = |path: &str| {
        let prefix = format!("{path}/");
        let files: Vec<(String, Vec<u8>)> = job
            .media
            .iter()
            .filter(|(name, _)| name.starts_with(&prefix))
            .map(|(name, bytes)| (name.clone(), bytes.clone()))
            .collect();
        (!files.is_empty()).then_some(files)
    };

    let input = PackageInput {
        course: &job.course,
        course_json: &course_json,
        player_files: &job.player,
        media: &resolve,
    };
    build_package_standard_with_folders(&input, job.standard, &folder).map_err(readable_error)
}

/// Build the package and write it to the job's output path atomically.
///
/// # Errors
/// Returns a plain sentence an author can act on.
pub fn run_publish_job(job: PublishJob) -> Result<PublishOutcome, String> {
    let files = package_files(&job)?;

    if let Some(parent) = job.out.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Couldn't create {}: {e}", parent.display()))?;
    }
    // Stream the zip into a temporary file next to the target and rename it
    // into place, so a big package is never held in memory a second time.
    let file_name = job
        .out
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("package.zip");
    let tmp = job
        .out
        .with_file_name(format!("{file_name}.tmp-{}", std::process::id()));
    let written = std::fs::File::create(&tmp)
        .map_err(|e| format!("Couldn't write {}: {e}", job.out.display()))
        .and_then(|file| {
            let mut file = std::io::BufWriter::new(file);
            zip_package_to(&files, &mut file)
                .map_err(|e| format!("Couldn't create the zip file: {e}"))?;
            std::io::Write::flush(&mut file)
                .map_err(|e| format!("Couldn't write {}: {e}", job.out.display()))
        })
        .and_then(|()| {
            std::fs::rename(&tmp, &job.out)
                .map_err(|e| format!("Couldn't write {}: {e}", job.out.display()))
        });
    if let Err(e) = written {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    let bytes = std::fs::metadata(&job.out).map_or(0, |m| m.len());

    Ok(PublishOutcome {
        path: job.out,
        bytes,
    })
}

fn readable_error(error: PublishError) -> String {
    match error {
        PublishError::MissingMedia { object, src } => format!(
            "Couldn't find the media file '{src}' used by object {object}. It may have been moved or deleted."
        ),
        PublishError::MissingIndexHtml => {
            "The bundled player is missing its index.html. Run `cargo xtask build-player` and rebuild the editor.".to_string()
        }
        PublishError::UnsafePath { object, src } => format!(
            "The path '{src}' used by object {object} can't be included in a published package."
        ),
        other => format!("Couldn't build the package: {other}"),
    }
}
