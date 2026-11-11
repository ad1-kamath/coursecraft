//! Exporting a course to MP4, PDF or Word from the desktop editor: no egui, so
//! the job stays testable.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use coursecraft_course::Course;
use coursecraft_export::mp4::ExportError as Mp4ExportError;
use coursecraft_export::{ExportMedia, ExportProgress, ExportReport, Mp4Options, export_mp4};
use coursecraft_ui_egui::coursecraft_render::{MediaFetch, MediaSource};
use coursecraft_ui_egui::{ReviewFormat, ReviewOptionsView};

use crate::files::MediaItem;

/// Everything a background export needs. It owns the course and media, so an
/// export never borrows the live session while the window keeps drawing.
pub struct ExportMp4Job {
    pub course: Course,
    pub media: Vec<MediaItem>,
    pub out: PathBuf,
    pub width: u32,
    pub fps: u32,
    pub cancel: Arc<AtomicBool>,
}

/// The written file and what the export did.
#[derive(Debug, Clone)]
pub struct ExportMp4Outcome {
    pub path: PathBuf,
    pub report: ExportReport,
}

/// A message from the worker thread to the editor.
#[derive(Debug, Clone)]
pub enum ExportUpdate {
    /// One frame was written.
    Progress(ExportProgress),
    /// The export stopped, successfully or not.
    Finished(Result<ExportMp4Outcome, String>),
}

/// Ask for an `.mp4` destination, defaulting to the course title.
#[must_use]
pub fn pick_export_save_file(default_title: &str) -> Option<PathBuf> {
    let default_name = format!("{}.mp4", default_file_stem(default_title));
    let res = rfd::FileDialog::new()
        .add_filter("MP4 video (*.mp4)", &["mp4"])
        .set_file_name(&default_name)
        .save_file();
    res.map(|mut path| {
        if path.extension().is_none() {
            path.set_extension("mp4");
        }
        path
    })
}

/// Ask for a `.pdf` or `.docx` destination, defaulting to the course title.
#[must_use]
pub fn pick_review_save_file(default_title: &str, format: ReviewFormat) -> Option<PathBuf> {
    let extension = format.extension();
    let default_name = format!("{}.{extension}", default_file_stem(default_title));
    let filter = format!("{} document (*.{extension})", format.label());
    let res = rfd::FileDialog::new()
        .add_filter(filter, &[extension])
        .set_file_name(&default_name)
        .save_file();
    res.map(|mut path| {
        if path.extension().is_none() {
            path.set_extension(extension);
        }
        path
    })
}

fn default_file_stem(default_title: &str) -> String {
    let sanitized_title = default_title
        .replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_")
        .trim()
        .to_string();
    if sanitized_title.is_empty() {
        "course".to_string()
    } else {
        sanitized_title
    }
}

/// Everything a background review export needs.
pub struct ExportReviewJob {
    pub course: Course,
    pub media: Vec<MediaItem>,
    pub out: PathBuf,
    pub format: ReviewFormat,
    pub options: ReviewOptionsView,
}

/// Run a PDF or Word review export and write it to the chosen path.
///
/// # Errors
/// Returns a plain sentence an author can act on.
pub fn run_export_review_job(job: ExportReviewJob) -> Result<PathBuf, String> {
    let ExportReviewJob {
        course,
        media,
        out,
        format,
        options,
    } = job;
    let media = InMemoryExportMedia(
        media
            .into_iter()
            .map(|(name, bytes)| (name, Arc::from(bytes)))
            .collect(),
    );
    let options = coursecraft_export::ReviewOptions {
        include_notes: options.include_notes,
        include_text: options.include_text,
        include_layers: options.include_layers,
        include_banks: options.include_banks,
        image_width: 1600,
    };
    let bytes = match format {
        ReviewFormat::Pdf => coursecraft_export::export_pdf(&course, &media, &options),
        ReviewFormat::Word => coursecraft_export::export_docx(&course, &media, &options),
    }
    .map_err(|error| format!("Couldn't make the review document: {error}"))?;
    crate::files::write_atomic(&out, &bytes)
        .map_err(|error| format!("Couldn't write {}: {error}", out.display()))?;
    Ok(out)
}

/// Read media from the editor's in-memory library, like the publish job.
struct InMemoryExportMedia(HashMap<String, Arc<[u8]>>);

impl ExportMedia for InMemoryExportMedia {
    fn bytes(&self, src: &str) -> Option<Arc<[u8]>> {
        self.0.get(src).cloned()
    }
}

impl MediaSource for InMemoryExportMedia {
    fn get(&self, src: &str) -> MediaFetch {
        self.0
            .get(src)
            .cloned()
            .map_or(MediaFetch::Missing, MediaFetch::Ready)
    }
}

/// Run the export, reporting every frame and stopping when the job is cancelled.
///
/// # Errors
/// Returns a plain sentence an author can act on.
pub fn run_export_job(
    job: ExportMp4Job,
    progress: &mut dyn FnMut(&ExportProgress),
) -> Result<ExportMp4Outcome, String> {
    let ExportMp4Job {
        course,
        media,
        out,
        width,
        fps,
        cancel,
    } = job;
    let media = InMemoryExportMedia(
        media
            .into_iter()
            .map(|(name, bytes)| (name, Arc::from(bytes)))
            .collect(),
    );
    let options = Mp4Options {
        width,
        fps,
        ..Mp4Options::default()
    };
    export_mp4(&course, &media, &out, &options, &mut |p| {
        progress(p);
        !cancel.load(Ordering::Relaxed)
    })
    .map(|report| ExportMp4Outcome { path: out, report })
    .map_err(readable_error)
}

fn readable_error(error: Mp4ExportError) -> String {
    match error {
        Mp4ExportError::Cancelled => "Export cancelled.".to_string(),
        Mp4ExportError::NotPlayable(issues) => format!(
            "The course can't be exported until these problems are fixed: {}",
            issues.join("; ")
        ),
        Mp4ExportError::TooLong => {
            "The export is longer than the two-hour limit. Split the course and try again."
                .to_string()
        }
        Mp4ExportError::Media(message) => {
            format!("Couldn't prepare the media for export: {message}")
        }
        Mp4ExportError::Render(message) => format!("Couldn't draw the video: {message}"),
        Mp4ExportError::Encoder(message) => format!("Couldn't encode the video: {message}"),
    }
}
