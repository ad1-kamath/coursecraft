//! `batch`: run one engine script over many courses.
//!
//! Each course is handled alone: a course that fails is reported and never
//! written, and the rest carry on. Writes use the same safe save as the app
//! (temp file, check it opens again, rename), so a crash never leaves a
//! half-written course.

use std::borrow::Cow;
use std::cell::OnceCell;
use std::collections::{HashMap, HashSet};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use coursecraft_course::{Course, MediaHashes, Severity};
use coursecraft_engine::Session;
use coursecraft_format::{FolderRecord, fnv1a64};
use coursecraft_publish::{
    PackageFile, PackageInput, PackageStandard, auto_embed_fonts,
    build_package_standard_with_folders, zip_package_to,
};
use coursecraft_ui_egui::coursecraft_media::system_fonts::SystemFontIndex;

use super::{
    CliFontProvider, Res, StandardArg, Step, collect_player_files, needs_system_font_scan,
    open_ccraft_file, read, write_course_output,
};

/// Most courses one batch takes.
const MAX_COURSES: usize = 10_000;
/// Largest replacement media file.
const MAX_MEDIA_BYTES: u64 = 512 * 1024 * 1024;

/// Everything `batch` was asked to do.
pub(crate) struct BatchArgs {
    pub script: PathBuf,
    pub courses: Vec<PathBuf>,
    pub dir: Option<PathBuf>,
    pub in_place: bool,
    pub out_dir: Option<PathBuf>,
    pub publish: Option<StandardArg>,
    pub publish_dir: Option<PathBuf>,
    pub player: Option<PathBuf>,
    pub replace_media: Vec<String>,
    pub dry_run: bool,
}

/// How a course is stored on disk.
enum Source {
    Ccraft,
    Folder,
    JsonFile,
}

struct Loaded {
    source: Source,
    course: Course,
    media: Vec<(String, Arc<[u8]>)>,
    record: Option<FolderRecord>,
}

enum Outcome {
    Changed,
    NoChange,
    Failed(String),
}

struct Row {
    course: String,
    outcome: Outcome,
}

struct Replacement {
    path: String,
    file: PathBuf,
}

struct Job<'a> {
    args: &'a BatchArgs,
    steps: &'a [Step],
    replacements: &'a [Replacement],
    player: Option<Vec<PackageFile>>,
    /// Lazily scanned OS fonts shared across every course in the batch (zero
    /// scans when all courses use Inter or already-embedded fonts).
    font_index: OnceCell<SystemFontIndex>,
}

fn is_ccraft(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("ccraft"))
}

fn display_name(p: &Path) -> String {
    p.display().to_string()
}

/// A name for a course's output: the file or folder name, or `course`.
fn base_name(p: &Path) -> String {
    let named = |q: &Path| {
        q.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .filter(|n| !n.is_empty() && n != "." && n != "..")
    };
    if let Some(n) = named(p) {
        if n == "course.json" {
            let parent = p
                .parent()
                .map(|q| {
                    if q.as_os_str().is_empty() {
                        Path::new(".")
                    } else {
                        q
                    }
                })
                .and_then(|q| q.canonicalize().ok());
            return parent.as_deref().and_then(named).unwrap_or(n);
        }
        return n;
    }
    p.canonicalize()
        .ok()
        .as_deref()
        .and_then(named)
        .unwrap_or_else(|| "course".to_owned())
}

fn stem(name: &str) -> String {
    Path::new(name)
        .file_stem()
        .map_or_else(|| name.to_owned(), |s| s.to_string_lossy().into_owned())
}

// ------------------------------------------------------------------ inputs

/// Every course directly inside `dir`: `.ccraft` files and project folders.
fn courses_in(dir: &Path) -> Res<Vec<PathBuf>> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut found = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
        let path = entry.path();
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if (kind.is_file() && is_ccraft(&path))
            || (kind.is_dir() && path.join("course.json").is_file())
        {
            found.push(path);
        }
    }
    found.sort();
    Ok(found)
}

fn all_courses(args: &BatchArgs) -> Res<Vec<PathBuf>> {
    let mut list = args.courses.clone();
    if let Some(dir) = &args.dir {
        for path in courses_in(dir)? {
            if !list.contains(&path) {
                list.push(path);
            }
        }
    }
    if list.is_empty() {
        return Err("no courses given: name them or use --dir <folder>".to_owned());
    }
    if list.len() > MAX_COURSES {
        return Err(format!("a batch takes at most {MAX_COURSES} courses"));
    }
    Ok(list)
}

fn check_args(args: &BatchArgs) -> Res<()> {
    if args.in_place && args.out_dir.is_some() {
        return Err("use either --in-place or --out-dir, not both".to_owned());
    }
    if !args.dry_run && !args.in_place && args.out_dir.is_none() {
        return Err("one of --in-place or --out-dir is required".to_owned());
    }
    if args.publish.is_some() && args.publish_dir.is_none() {
        return Err("--publish needs --publish-dir <dir>".to_owned());
    }
    if args.publish.is_some() && args.player.is_none() {
        return Err("--publish needs --player <dir> (the built player files)".to_owned());
    }
    if args.publish.is_none() && (args.publish_dir.is_some() || args.player.is_some()) {
        return Err("--publish-dir and --player only work with --publish <standard>".to_owned());
    }
    Ok(())
}

fn parse_replacements(raw: &[String]) -> Res<Vec<Replacement>> {
    raw.iter()
        .map(|item| {
            let (path, file) = item
                .split_once('=')
                .ok_or_else(|| format!("--replace-media {item:?}: use <path-in-course>=<file>"))?;
            if path.is_empty() || file.is_empty() {
                return Err(format!(
                    "--replace-media {item:?}: use <path-in-course>=<file>"
                ));
            }
            Ok(Replacement {
                path: path.to_owned(),
                file: PathBuf::from(file),
            })
        })
        .collect()
}

fn load(path: &Path) -> Res<Loaded> {
    let shown = display_name(path);
    if is_ccraft(path) {
        let project = open_ccraft_file(path)?;
        return Ok(Loaded {
            source: Source::Ccraft,
            course: project.course,
            media: project
                .media
                .into_iter()
                .map(|(name, bytes)| (name, Arc::from(bytes)))
                .collect(),
            record: None,
        });
    }
    let folder = if path.is_dir() {
        Some(path.to_path_buf())
    } else if path.file_name().is_some_and(|n| n == "course.json") {
        Some(
            path.parent()
                .filter(|p| !p.as_os_str().is_empty())
                .map_or_else(|| PathBuf::from("."), Path::to_path_buf),
        )
    } else {
        None
    };
    if let Some(dir) = folder {
        let opened =
            coursecraft_format::open_project_folder(&dir).map_err(|e| format!("{shown}: {e}"))?;
        return Ok(Loaded {
            source: Source::Folder,
            course: opened.project.course,
            media: opened
                .project
                .media
                .into_iter()
                .map(|(name, bytes)| (name, Arc::from(bytes)))
                .collect(),
            record: Some(opened.record),
        });
    }
    let text = read(path)?;
    let course = Course::from_json(&text).map_err(|e| format!("{shown}: {e}"))?;
    Ok(Loaded {
        source: Source::JsonFile,
        course,
        media: Vec::new(),
        record: None,
    })
}

// ------------------------------------------------------------------- media

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MediaKind {
    Image,
    Audio,
    Video,
    Other,
}

impl MediaKind {
    fn word(self) -> &'static str {
        match self {
            Self::Image => "an image",
            Self::Audio => "audio",
            Self::Video => "a video",
            Self::Other => "a file of another kind",
        }
    }
}

pub(crate) fn kind_from_bytes(bytes: &[u8]) -> Option<MediaKind> {
    let starts = |magic: &[u8]| bytes.starts_with(magic);
    let at = |from: usize, magic: &[u8]| {
        bytes
            .get(from..from.saturating_add(magic.len()))
            .is_some_and(|w| w == magic)
    };
    if starts(b"\x89PNG") || starts(&[0xFF, 0xD8, 0xFF]) || starts(b"GIF8") || starts(b"BM") {
        return Some(MediaKind::Image);
    }
    if starts(b"RIFF") && at(8, b"WEBP") {
        return Some(MediaKind::Image);
    }
    if starts(b"RIFF") && at(8, b"WAVE") {
        return Some(MediaKind::Audio);
    }
    if starts(b"ID3") || starts(b"OggS") || starts(b"fLaC") || starts(&[0xFF, 0xFB]) {
        return Some(MediaKind::Audio);
    }
    if at(4, b"ftyp") {
        // The brand after `ftyp` says what the file is: iTunes audio, a still
        // image (AVIF, HEIC) or, for anything else, a video.
        let brand = bytes.get(8..12).unwrap_or(&[]);
        return Some(match brand {
            b"M4A " | b"M4B " | b"M4P " | b"F4A " | b"F4B " => MediaKind::Audio,
            b"avif" | b"avis" | b"heic" | b"heix" | b"heim" | b"heis" | b"hevc" | b"mif1"
            | b"msf1" => MediaKind::Image,
            _ => MediaKind::Video,
        });
    }
    if starts(&[0x1A, 0x45, 0xDF, 0xA3]) {
        return Some(MediaKind::Video);
    }
    let head = bytes.get(..bytes.len().min(512)).unwrap_or(&[]);
    let text = String::from_utf8_lossy(head).to_ascii_lowercase();
    if text.contains("<svg") {
        return Some(MediaKind::Image);
    }
    None
}

fn kind_from_name(name: &str) -> MediaKind {
    let ext = Path::new(name)
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" => MediaKind::Image,
        "mp3" | "wav" | "ogg" | "m4a" | "flac" | "aac" => MediaKind::Audio,
        "mp4" | "webm" | "mov" | "m4v" => MediaKind::Video,
        _ => MediaKind::Other,
    }
}

fn media_kind(name: &str, bytes: &[u8]) -> MediaKind {
    kind_from_bytes(bytes).unwrap_or_else(|| kind_from_name(name))
}

/// Swap the bytes of a media file, keeping its path, after checking that the
/// new file is the same kind as the old one.
fn replace_media(media: &mut [(String, Arc<[u8]>)], item: &Replacement) -> Res<bool> {
    let Some(slot) = media.iter_mut().find(|(name, _)| *name == item.path) else {
        return Err(format!("media file {:?} is not in this course", item.path));
    };
    let meta =
        std::fs::metadata(&item.file).map_err(|e| format!("{}: {e}", item.file.display()))?;
    if meta.len() > MAX_MEDIA_BYTES {
        return Err(format!("{}: file is too large", item.file.display()));
    }
    let bytes = std::fs::read(&item.file).map_err(|e| format!("{}: {e}", item.file.display()))?;
    let old = media_kind(&item.path, &slot.1);
    let new = media_kind(&item.file.to_string_lossy(), &bytes);
    // Fonts, captions, Lottie files and PDFs are `Other`: they may replace a
    // file of the same kind, but never an image, sound or video.
    if old != new {
        return Err(format!(
            "{} is {} but {:?} is {}",
            item.file.display(),
            new.word(),
            item.path,
            old.word()
        ));
    }
    let changed = slot.1.as_ref() != bytes.as_slice();
    slot.1 = Arc::from(bytes);
    Ok(changed)
}

fn hashes(media: &[(String, Arc<[u8]>)]) -> MediaHashes {
    media
        .iter()
        .map(|(name, bytes)| (name.clone(), fnv1a64(bytes)))
        .collect()
}

// -------------------------------------------------------------------- work

fn run_script(course: &Course, steps: &[Step]) -> Res<Course> {
    let mut session = Session::new(course.clone());
    for (i, step) in steps.iter().enumerate() {
        session
            .execute(&step.command, &step.params)
            .map_err(|e| format!("step {i} ({}): {e}", step.command))?;
    }
    Ok(session.course)
}

fn error_count(course: &Course) -> usize {
    course
        .validate()
        .iter()
        .filter(|i| i.severity == Severity::Error)
        .count()
}

/// The result of running the script on one course, before anything is written.
struct Edited {
    course: Course,
    media: Vec<(String, Arc<[u8]>)>,
    changed: bool,
}

fn edit(job: &Job<'_>, loaded: &Loaded) -> Res<Edited> {
    let course = run_script(&loaded.course, job.steps)?;
    if error_count(&loaded.course) == 0 {
        let errors = error_count(&course);
        if errors > 0 {
            return Err(format!("the script left {errors} validation error(s)"));
        }
    }
    let mut media = loaded.media.clone();
    let mut media_changed = false;
    for item in job.replacements {
        media_changed |= replace_media(&mut media, item)?;
    }
    let changed = course != loaded.course || media_changed;
    Ok(Edited {
        course,
        media,
        changed,
    })
}

fn dry_run_text(old: &Loaded, edited: &Edited) -> String {
    let before = hashes(&old.media);
    let after = hashes(&edited.media);
    let changes =
        coursecraft_course::diff_with_media(&old.course, &edited.course, Some((&before, &after)));
    changes
        .iter()
        .map(coursecraft_course::Change::sentence)
        .collect::<Vec<_>>()
        .join("\n")
}

// ----------------------------------------------------------------- writing

/// Write a `.ccraft` the way the app does: temp file, check it opens again,
/// rename over the old one.
fn write_ccraft_safely(path: &Path, bytes: &[u8]) -> Res<()> {
    let name = path
        .file_name()
        .map_or_else(|| "course".into(), |n| n.to_string_lossy().into_owned());
    let temp = path.with_file_name(format!(".{name}.batch-tmp"));
    let result = (|| {
        std::fs::write(&temp, bytes).map_err(|e| format!("{}: {e}", temp.display()))?;
        let file = std::fs::File::open(&temp).map_err(|e| format!("{}: {e}", temp.display()))?;
        coursecraft_format::verify_project(std::io::BufReader::new(file))
            .map_err(|e| format!("the saved file could not be opened again ({e})"))?;
        std::fs::rename(&temp, path).map_err(|e| format!("{}: {e}", path.display()))
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

fn write_json_safely(path: &Path, course: &Course) -> Res<()> {
    let mut json = course.to_json().map_err(|e| e.to_string())?;
    json.push('\n');
    Course::from_json(&json)
        .map_err(|e| format!("the saved file could not be opened again ({e})"))?;
    let name = path
        .file_name()
        .map_or_else(|| "course".into(), |n| n.to_string_lossy().into_owned());
    let temp = path.with_file_name(format!(".{name}.batch-tmp"));
    let result = std::fs::write(&temp, json)
        .and_then(|()| std::fs::rename(&temp, path))
        .map_err(|e| format!("{}: {e}", path.display()));
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

fn save_in_place(path: &Path, loaded: Loaded, edited: &Edited) -> Res<()> {
    match loaded.source {
        Source::Ccraft => {
            let bytes = coursecraft_format::save_project(&edited.course, &edited.media)
                .map_err(|e| format!("save error: {e}"))?;
            write_ccraft_safely(path, &bytes)
        }
        Source::Folder => {
            let dir = if path.is_dir() {
                path.to_path_buf()
            } else {
                path.parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
            };
            let mut record = loaded.record.unwrap_or_default();
            coursecraft_format::save_project_folder(
                &dir,
                &edited.course,
                &edited.media,
                &mut record,
            )
            .map(|_| ())
            .map_err(|e| format!("save error: {e}"))
        }
        Source::JsonFile => write_json_safely(path, &edited.course),
    }
}

fn output_name(path: &Path) -> String {
    base_name(path)
}

fn save_to_dir(out_dir: &Path, name: &str, loaded: &Loaded, edited: &Edited) -> Res<()> {
    std::fs::create_dir_all(out_dir).map_err(|e| format!("{}: {e}", out_dir.display()))?;
    let target = out_dir.join(name);
    match loaded.source {
        Source::Ccraft => {
            let bytes = coursecraft_format::save_project(&edited.course, &edited.media)
                .map_err(|e| format!("save error: {e}"))?;
            write_ccraft_safely(&target, &bytes)
        }
        Source::Folder | Source::JsonFile => {
            write_course_output(&target, &edited.course, &edited.media)
        }
    }
}

// ----------------------------------------------------------------- publish

/// Prepare the package file list using shared `Arc<[u8]>` buffers without
/// copying media or building the zip in memory.
fn build_publish(
    job: &Job<'_>,
    name: &str,
    edited: &Edited,
) -> Res<Option<(PathBuf, Vec<PackageFile>)>> {
    let (Some(standard), Some(dir), Some(player)) =
        (job.args.publish, &job.args.publish_dir, &job.player)
    else {
        return Ok(None);
    };
    let errors = error_count(&edited.course);
    if errors > 0 {
        return Err(format!(
            "cannot publish: course has {errors} validation error(s)"
        ));
    }
    let (published_course, embedded_map): (Cow<'_, Course>, HashMap<String, Arc<[u8]>>) =
        if needs_system_font_scan(&edited.course) {
            let index = job.font_index.get_or_init(SystemFontIndex::scan_default);
            let auto = auto_embed_fonts(&edited.course, Some(&CliFontProvider(index)));
            let map = auto
                .embedded_files
                .into_iter()
                .map(|(face, bytes)| (face.src, Arc::from(bytes)))
                .collect();
            (Cow::Owned(auto.course), map)
        } else {
            (Cow::Borrowed(&edited.course), HashMap::new())
        };
    let json = published_course
        .to_json_compact()
        .map_err(|e| e.to_string())?;
    let lookup: HashMap<&str, &Arc<[u8]>> =
        edited.media.iter().map(|(n, b)| (n.as_str(), b)).collect();
    let media = |src: &str| {
        embedded_map
            .get(src)
            .map(Arc::clone)
            .or_else(|| lookup.get(src).map(|b| Arc::clone(b)))
    };
    let web = |folder: &str| {
        let prefix = format!("{}/", folder.trim_end_matches('/'));
        let files: Vec<(String, Arc<[u8]>)> = edited
            .media
            .iter()
            .filter(|(n, _)| n.starts_with(&prefix))
            .map(|(n, b)| (n.clone(), Arc::clone(b)))
            .collect();
        (!files.is_empty()).then_some(files)
    };
    let input = PackageInput {
        course: &published_course,
        course_json: &json,
        player_files: player,
        media: &media,
    };
    let files = build_package_standard_with_folders(&input, PackageStandard::from(standard), &web)
        .map_err(|e| format!("publish error: {e}"))?;
    Ok(Some((dir.join(format!("{}.zip", stem(name))), files)))
}

/// Courses from different folders can share a file name. Each package gets its
/// own file: `lesson.zip`, then `lesson-2.zip`, and so on.
pub(crate) fn unique_package_path(target: PathBuf, taken: &mut HashSet<String>) -> PathBuf {
    let stem = target
        .file_stem()
        .map_or_else(|| "course".to_owned(), |s| s.to_string_lossy().into_owned());
    let mut number = 1u32;
    loop {
        let name = if number == 1 {
            format!("{stem}.zip")
        } else {
            format!("{stem}-{number}.zip")
        };
        let candidate = target.with_file_name(&name);
        if taken.insert(format!("package:{}", candidate.display())) {
            return candidate;
        }
        number = number.saturating_add(1);
    }
}

fn write_publish(package: Option<(PathBuf, Vec<PackageFile>)>) -> Res<()> {
    let Some((target, files)) = package else {
        return Ok(());
    };
    if let Some(dir) = target.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let file = std::fs::File::create(&target).map_err(|e| format!("{}: {e}", target.display()))?;
    let mut writer = std::io::BufWriter::with_capacity(256 * 1024, file);
    zip_package_to(&files, &mut writer).map_err(|e| format!("zip error: {e}"))?;
    writer
        .flush()
        .map_err(|e| format!("{}: {e}", target.display()))
}

// -------------------------------------------------------------- one course

fn process(job: &Job<'_>, path: &Path, taken: &mut HashSet<String>) -> Res<Outcome> {
    let loaded = load(path)?;
    let edited = edit(job, &loaded)?;
    let name = output_name(path);
    let outcome = if edited.changed {
        Outcome::Changed
    } else {
        Outcome::NoChange
    };
    if job.args.dry_run {
        let text = dry_run_text(&loaded, &edited);
        if !text.is_empty() {
            println!("{}:\n{text}", path.display());
        }
        return Ok(outcome);
    }
    let package = build_publish(job, &name, &edited)?
        .map(|(target, files)| (unique_package_path(target, taken), files));
    if let Some(out_dir) = &job.args.out_dir {
        if !taken.insert(name.clone()) {
            return Err(format!(
                "another course is already written to {name:?} in the output folder"
            ));
        }
        save_to_dir(out_dir, &name, &loaded, &edited)?;
    } else if edited.changed {
        save_in_place(path, loaded, &edited)?;
    }
    write_publish(package)?;
    Ok(outcome)
}

// ------------------------------------------------------------------- table

fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn result_text(outcome: &Outcome) -> String {
    match outcome {
        Outcome::Changed => "changed".to_owned(),
        Outcome::NoChange => "no change".to_owned(),
        Outcome::Failed(reason) => format!("failed: {}", one_line(reason)),
    }
}

fn print_table(rows: &[Row]) {
    let width = rows
        .iter()
        .map(|r| r.course.chars().count())
        .max()
        .unwrap_or(6)
        .max("course".len());
    println!("{:<width$}  result", "course");
    for row in rows {
        println!("{:<width$}  {}", row.course, result_text(&row.outcome));
    }
    let count = |f: fn(&Outcome) -> bool| rows.iter().filter(|r| f(&r.outcome)).count();
    println!(
        "{} course(s): {} changed, {} no change, {} failed",
        rows.len(),
        count(|o| matches!(o, Outcome::Changed)),
        count(|o| matches!(o, Outcome::NoChange)),
        count(|o| matches!(o, Outcome::Failed(_))),
    );
}

/// `batch`: exit 0 when every course worked, 1 when any failed, an error (2)
/// for bad arguments or an unreadable script.
pub(crate) fn run_batch(args: &BatchArgs) -> Res<ExitCode> {
    check_args(args)?;
    let steps: Vec<Step> =
        serde_json::from_str(&read(&args.script)?).map_err(|e| format!("script: {e}"))?;
    let replacements = parse_replacements(&args.replace_media)?;
    let courses = all_courses(args)?;
    let player = match (&args.publish, &args.player) {
        (Some(_), Some(dir)) => Some(collect_player_files(dir)?),
        _ => None,
    };
    let job = Job {
        args,
        steps: &steps,
        replacements: &replacements,
        player,
        font_index: OnceCell::new(),
    };
    let mut taken = HashSet::new();
    let rows: Vec<Row> = courses
        .iter()
        .map(|path| Row {
            course: display_name(path),
            outcome: process(&job, path, &mut taken).unwrap_or_else(Outcome::Failed),
        })
        .collect();
    print_table(&rows);
    let failed = rows.iter().any(|r| matches!(r.outcome, Outcome::Failed(_)));
    Ok(if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}
