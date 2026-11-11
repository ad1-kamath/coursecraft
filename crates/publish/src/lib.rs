//! CourseCraft publishing (L4).
//!
//! Builds a SCORM 1.2 package in memory from a course, the compiled web player
//! and the course's media, and zips it. No filesystem access, so it also runs
//! in the browser; the CLI does the reading and writing.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

use std::collections::HashSet;
use std::io::Write as _;

use coursecraft_course::{BACKGROUND_AUDIO_ID, Course, Object, ObjectKind};
use coursecraft_scorm::Standard;
use coursecraft_xapi::{Flavor, PackageSpec, cmi5_xml, tincan_xml};

#[cfg(test)]
mod tests;

/// Root marker naming the package standard. Missing means SCORM 1.2.
pub const PACKAGE_MARKER: &str = "coursecraft-package.json";

/// A package standard: SCORM, one of the xAPI flavors, or LTI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageStandard {
    Scorm(Standard),
    Xapi(Flavor),
    /// An LTI 1.3 package for the CourseCraft LTI server: the SCORM 1.2 file
    /// set without a manifest, plus a `{"standard":"lti"}` marker.
    Lti,
}

/// A single file in a package, holding its relative package path and contents.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageFile {
    pub path: String,
    pub bytes: Vec<u8>,
}

/// In-memory inputs needed to build a SCORM 1.2 package.
pub struct PackageInput<'a> {
    pub course: &'a Course,
    pub course_json: &'a str,
    pub player_files: &'a [PackageFile],
    pub media: &'a dyn Fn(&str) -> Option<Vec<u8>>,
}

/// Publishing errors. Every invalid input is an error, never a panic.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PublishError {
    #[error("player files must contain index.html")]
    MissingIndexHtml,
    #[error("unsafe path for object {object}: {src}")]
    UnsafePath { object: String, src: String },
    #[error("missing media file for object {object}: {src}")]
    MissingMedia { object: String, src: String },
    #[error("duplicate package file: {0}")]
    DuplicateFile(String),
    #[error("scorm manifest error: {0}")]
    Manifest(#[from] coursecraft_scorm::ScormError),
    #[error("zip error: {0}")]
    Zip(String),
}

impl From<zip::result::ZipError> for PublishError {
    fn from(err: zip::result::ZipError) -> Self {
        PublishError::Zip(err.to_string())
    }
}

impl From<std::io::Error> for PublishError {
    fn from(err: std::io::Error) -> Self {
        PublishError::Zip(err.to_string())
    }
}

pub use coursecraft_format::is_safe_relative_path;

struct MediaRef<'a> {
    object_id: &'a str,
    src: &'a str,
}

fn collect_object_media<'a>(obj: &'a Object, out: &mut Vec<MediaRef<'a>>) {
    match &obj.kind {
        ObjectKind::Image { src, .. } | ObjectKind::Lottie { src, .. } => {
            out.push(MediaRef {
                object_id: &obj.id,
                src,
            });
        }
        ObjectKind::Audio { src, captions, .. } | ObjectKind::Video { src, captions, .. } => {
            out.push(MediaRef {
                object_id: &obj.id,
                src,
            });
            if let Some(captions) = captions {
                out.push(MediaRef {
                    object_id: &obj.id,
                    src: captions,
                });
            }
        }
        ObjectKind::Shape { fill_style, .. } | ObjectKind::Button { fill_style, .. } => {
            if let Some(src) = fill_style
                .as_ref()
                .and_then(coursecraft_course::FillStyle::picture_src)
                && !src.is_empty()
            {
                out.push(MediaRef {
                    object_id: &obj.id,
                    src,
                });
            }
        }
        _ => {}
    }
    for state in &obj.states {
        if let Some(src) = &state.src {
            out.push(MediaRef {
                object_id: &obj.id,
                src,
            });
        }
    }
}

fn collect_resource_media<'a>(course: &'a Course, out: &mut Vec<MediaRef<'a>>) {
    if let Some(logo) = &course.player.logo {
        out.push(MediaRef {
            object_id: "player-logo",
            src: logo,
        });
    }
    for resource in &course.resources {
        if let coursecraft_course::ResourceTarget::File { src } = &resource.target {
            out.push(MediaRef {
                object_id: &resource.id,
                src,
            });
        }
    }
}

fn add_web_folders(
    course: &Course,
    files: &mut Vec<PackageFile>,
    web_folder: &WebFolderFiles<'_>,
) -> Result<(), PublishError> {
    let mut web_folders = HashSet::new();
    for slide in course.slides() {
        for obj in &slide.objects {
            collect_web_folders(obj, &mut web_folders);
        }
        for layer in &slide.layers {
            for obj in &layer.objects {
                collect_web_folders(obj, &mut web_folders);
            }
        }
    }
    for master in &course.masters {
        for obj in &master.objects {
            collect_web_folders(obj, &mut web_folders);
        }
    }
    for bank in &course.banks {
        for slide in &bank.slides {
            for obj in &slide.objects {
                collect_web_folders(obj, &mut web_folders);
            }
            for layer in &slide.layers {
                for obj in &layer.objects {
                    collect_web_folders(obj, &mut web_folders);
                }
            }
        }
    }
    let mut folders: Vec<String> = web_folders.into_iter().collect();
    folders.sort();
    for folder in folders {
        if !is_safe_relative_path(&folder) || !folder.starts_with("web/") {
            return Err(PublishError::UnsafePath {
                object: "web".to_string(),
                src: folder,
            });
        }
        let entry = web_folder_entry(course, &folder);
        let folder_files = web_folder(&folder).ok_or_else(|| PublishError::MissingMedia {
            object: "web".to_string(),
            src: entry.clone().unwrap_or_else(|| folder.clone()),
        })?;
        let prefix = format!("{folder}/");
        let mut found_entry = entry.is_none();
        for (relative, bytes) in folder_files {
            if !is_safe_relative_path(&relative) || !relative.starts_with(&prefix) {
                return Err(PublishError::UnsafePath {
                    object: "web".to_string(),
                    src: relative,
                });
            }
            if entry.as_deref() == Some(relative.as_str()) {
                found_entry = true;
            }
            files.push(PackageFile {
                path: format!("media/{relative}"),
                bytes,
            });
        }
        if !found_entry {
            return Err(PublishError::MissingMedia {
                object: "web".to_string(),
                src: entry.unwrap_or(folder),
            });
        }
    }
    Ok(())
}

fn collect_web_folders(obj: &Object, out: &mut HashSet<String>) {
    if let ObjectKind::Web {
        source: coursecraft_course::WebSource::Folder { path, .. },
    } = &obj.kind
    {
        out.insert(path.clone());
    }
}

/// The media-relative entry file of the first web object using `folder`.
fn web_folder_entry(course: &Course, folder: &str) -> Option<String> {
    let mut objects = course
        .slides()
        .flat_map(|slide| {
            slide
                .objects
                .iter()
                .chain(slide.layers.iter().flat_map(|layer| layer.objects.iter()))
        })
        .chain(
            course
                .masters
                .iter()
                .flat_map(|master| master.objects.iter()),
        )
        .chain(
            course
                .banks
                .iter()
                .flat_map(|bank| bank.slides.iter())
                .flat_map(|slide| {
                    slide
                        .objects
                        .iter()
                        .chain(slide.layers.iter().flat_map(|layer| layer.objects.iter()))
                }),
        );
    objects.find_map(|object| match &object.kind {
        ObjectKind::Web {
            source: coursecraft_course::WebSource::Folder { path, entry },
        } if path == folder => Some(format!("{path}/{entry}")),
        _ => None,
    })
}

/// Build the full SCORM 1.2 file set in memory.
///
/// This keeps its original signature and behaviour; it calls
/// [`build_package_for`] with [`Standard::Scorm12`].
///
/// # Errors
/// Returns [`PublishError`] if `index.html` is missing, any path is unsafe,
/// a media file cannot be resolved, or manifest generation fails.
pub fn build_package(input: &PackageInput<'_>) -> Result<Vec<PackageFile>, PublishError> {
    build_package_for(input, Standard::Scorm12)
}

/// Build a package for a SCORM standard in memory.
///
/// This delegates to [`build_package_standard`] with
/// [`PackageStandard::Scorm`], so existing SCORM output is unchanged.
///
/// # Errors
/// Returns [`PublishError`] if `index.html` is missing, any path is unsafe,
/// a media file cannot be resolved, or manifest generation fails.
pub fn build_package_for(
    input: &PackageInput<'_>,
    standard: Standard,
) -> Result<Vec<PackageFile>, PublishError> {
    build_package_standard(input, PackageStandard::Scorm(standard))
}

/// Build a package for any supported standard in memory.
///
/// Validates player files and course media paths, replaces any player
/// `course.json` with the current course, and returns the files sorted by
/// path with no duplicates.
///
/// - SCORM packages get `imsmanifest.xml`; 2004 also gets a standard marker.
/// - cmi5 gets `cmi5.xml` and a `{"standard":"cmi5"}` marker.
/// - xAPI gets `tincan.xml` and a `{"standard":"xapi"}` marker.
/// - LTI gets the SCORM 1.2 file set without a manifest and a
///   `{"standard":"lti"}` marker.
///
/// xAPI and LTI packages have no `imsmanifest.xml`.
///
/// # Errors
/// Returns [`PublishError`] if `index.html` is missing, any path is unsafe,
/// a media file cannot be resolved, or manifest generation fails.
pub fn build_package_standard(
    input: &PackageInput<'_>,
    standard: PackageStandard,
) -> Result<Vec<PackageFile>, PublishError> {
    build_package_standard_with_folders(input, standard, &|_| None)
}

/// Build a package and include every file in each web object's folder.
///
/// `web_folder` receives a folder path relative to `media/` (for example
/// `web/site`) and returns its files keyed the same way (`web/site/index.html`).
/// Returning `None` is reported as missing media. Use this from hosts that can
/// list a folder; the plain [`build_package_standard`] cannot and rejects a
/// course that uses a folder source.
///
/// # Errors
/// Returns [`PublishError`] for the same reasons as [`build_package_standard`],
/// plus unsafe or missing web-folder files.
/// Lists a web object's folder files by media-relative path.
pub type WebFolderFiles<'a> = dyn Fn(&str) -> Option<Vec<(String, Vec<u8>)>> + 'a;

pub fn build_package_standard_with_folders(
    input: &PackageInput<'_>,
    standard: PackageStandard,
    web_folder: &WebFolderFiles<'_>,
) -> Result<Vec<PackageFile>, PublishError> {
    // 1. Validate player files and check for index.html.
    let mut has_index = false;
    let mut files = Vec::new();

    for player_file in input.player_files {
        if !is_safe_relative_path(&player_file.path) {
            return Err(PublishError::UnsafePath {
                object: "player".to_string(),
                src: player_file.path.clone(),
            });
        }
        if player_file.path == "index.html" {
            has_index = true;
        }
        if player_file.path == "course.json" || player_file.path == PACKAGE_MARKER {
            // Ours replace any course.json or standard marker in the player folder.
            continue;
        }
        files.push(player_file.clone());
    }

    if !has_index {
        return Err(PublishError::MissingIndexHtml);
    }

    // 2. Add course.json. Templates are authoring-only and never published.
    let course_json = if input.course.templates.is_empty() {
        input.course_json.to_owned()
    } else {
        let mut published = input.course.clone();
        published.templates.clear();
        published
            .to_json()
            .unwrap_or_else(|_| input.course_json.to_owned())
    };
    files.push(PackageFile {
        path: "course.json".to_string(),
        bytes: course_json.as_bytes().to_vec(),
    });

    // 3. Collect media references from base objects and layer objects, and from
    // masters, which draw under every slide that names them. Template media is
    // not collected: a template is not played.
    let mut media_refs = Vec::new();
    for slide in input.course.slides() {
        for obj in &slide.objects {
            collect_object_media(obj, &mut media_refs);
        }
        for layer in &slide.layers {
            for obj in &layer.objects {
                collect_object_media(obj, &mut media_refs);
            }
        }
    }
    for master in &input.course.masters {
        for obj in &master.objects {
            collect_object_media(obj, &mut media_refs);
        }
    }
    for bank in &input.course.banks {
        for slide in &bank.slides {
            for obj in &slide.objects {
                collect_object_media(obj, &mut media_refs);
            }
            for layer in &slide.layers {
                for obj in &layer.objects {
                    collect_object_media(obj, &mut media_refs);
                }
            }
        }
    }

    // A course-wide background track is media too.
    media_refs.extend(input.course.background_audio.iter().map(|audio| MediaRef {
        object_id: BACKGROUND_AUDIO_ID,
        src: &audio.src,
    }));

    // Course fonts are media too: every face ships under media/fonts/.
    for face in &input.course.fonts {
        media_refs.push(MediaRef {
            object_id: "font",
            src: &face.src,
        });
    }

    // Resource files ship like any other asset, under media/.
    collect_resource_media(input.course, &mut media_refs);

    let mut seen_sources = HashSet::new();
    for m in media_refs {
        let src = m.src;
        // Skip remote http:// and https:// sources.
        if src.starts_with("http://") || src.starts_with("https://") {
            continue;
        }

        if !is_safe_relative_path(src) {
            return Err(PublishError::UnsafePath {
                object: m.object_id.to_string(),
                src: src.to_string(),
            });
        }

        if !seen_sources.insert(src) {
            continue;
        }

        let bytes = (input.media)(src).ok_or_else(|| PublishError::MissingMedia {
            object: m.object_id.to_string(),
            src: src.to_string(),
        })?;

        // Package layout rule: always "media/" + src (the player loads `media/{src}`).
        let path = format!("media/{src}");

        files.push(PackageFile { path, bytes });
    }

    // 3b. Web object folders: every file ships under media/<path>/ and keeps
    // its relative path, so the iframe can load `media/web/<name>/<entry>`.
    add_web_folders(input.course, &mut files, web_folder)?;

    // 4. State the standard for the player. SCORM 1.2 stays marker-free so its
    // packages are unchanged; a missing file means 1.2.
    match standard {
        PackageStandard::Scorm(Standard::Scorm12) => {}
        PackageStandard::Scorm(Standard::Scorm2004) => {
            files.push(PackageFile {
                path: PACKAGE_MARKER.to_string(),
                bytes: b"{\"standard\":\"scorm2004\"}".to_vec(),
            });
        }
        PackageStandard::Xapi(flavor) => {
            let marker = format!("{{\"standard\":\"{}\"}}", flavor.as_id());
            files.push(PackageFile {
                path: PACKAGE_MARKER.to_string(),
                bytes: marker.into_bytes(),
            });
            let spec = PackageSpec {
                course_id: input.course.id.clone(),
                title: input.course.title.clone(),
                mastery_score: input.course.quiz.as_ref().map(|q| q.passing_percent),
            };
            let (path, xml) = match flavor {
                Flavor::Cmi5 => ("cmi5.xml", cmi5_xml(&spec)),
                Flavor::Xapi => ("tincan.xml", tincan_xml(&spec)),
            };
            files.push(PackageFile {
                path: path.to_string(),
                bytes: xml.into_bytes(),
            });
        }
        PackageStandard::Lti => {
            files.push(PackageFile {
                path: PACKAGE_MARKER.to_string(),
                bytes: b"{\"standard\":\"lti\"}".to_vec(),
            });
        }
    }

    // 5. Generate imsmanifest.xml with the full file list, sorted. xAPI
    // packages carry no imsmanifest.
    if let PackageStandard::Scorm(scorm_standard) = standard {
        let mut manifest_files: Vec<String> = files.iter().map(|f| f.path.clone()).collect();
        manifest_files.push("imsmanifest.xml".to_string());
        manifest_files.sort();
        manifest_files.dedup();

        let spec = coursecraft_scorm::ManifestSpec {
            identifier: format!("coursecraft.{}", input.course.id),
            title: input.course.title.clone(),
            launch: "index.html".to_string(),
            files: manifest_files,
            mastery_score: input.course.quiz.as_ref().map(|q| q.passing_percent),
        };

        let manifest_xml = match scorm_standard {
            Standard::Scorm12 => coursecraft_scorm::manifest_scorm12(&spec)?,
            Standard::Scorm2004 => coursecraft_scorm::manifest_scorm2004(&spec)?,
        };
        files.push(PackageFile {
            path: "imsmanifest.xml".to_string(),
            bytes: manifest_xml.into_bytes(),
        });
    }

    // 6. Return the files sorted by path with no duplicates.
    files.sort_by(|a, b| a.path.cmp(&b.path));

    for window in files.windows(2) {
        if let (Some(first), Some(second)) = (window.first(), window.get(1))
            && first.path == second.path
        {
            return Err(PublishError::DuplicateFile(first.path.clone()));
        }
    }

    Ok(files)
}

/// Pack package files into a deterministic SCORM 1.2 zip archive.
///
/// Text is deflated; pictures, video, audio and fonts, which are already
/// compressed, are stored (see [`coursecraft_format::is_precompressed`]).
/// Uses a fixed timestamp of 1980-01-01 00:00:00 for reproducible,
/// byte-identical output.
///
/// # Errors
/// Returns [`PublishError`] if writing or compressing any file fails.
pub fn zip_package(files: &[PackageFile]) -> Result<Vec<u8>, PublishError> {
    let capacity = files
        .iter()
        .map(|f| f.bytes.len().saturating_add(128))
        .fold(1 << 16, usize::saturating_add);
    let mut out = Vec::with_capacity(capacity);
    zip_package_to(files, std::io::Cursor::new(&mut out))?;
    Ok(out)
}

/// Pack package files into a zip written straight to `out` (a file, say), so
/// a large package is never held in memory a second time. Same bytes as
/// [`zip_package`].
///
/// # Errors
/// Returns [`PublishError`] if writing or compressing any file fails.
pub fn zip_package_to<W: std::io::Write + std::io::Seek>(
    files: &[PackageFile],
    out: W,
) -> Result<(), PublishError> {
    let mut writer = zip::ZipWriter::new(out);
    for file in files {
        writer.start_file(&file.path, coursecraft_format::zip_options_for(&file.path))?;
        writer.write_all(&file.bytes)?;
    }
    writer.finish()?;
    Ok(())
}
