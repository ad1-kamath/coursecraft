//! `coursecraft-cli`: the headless front door. Everything here goes through the
//! same crates the editor uses, so agents and CI see exactly what learners get.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use coursecraft_course::{AuditReport, AuditSeverity, Course, Severity};
use coursecraft_engine::{Session, registry};
use coursecraft_logic::{Effect, Input, Player};
use coursecraft_publish::{
    PackageFile, PackageInput, PackageStandard, build_package_standard_with_folders, zip_package_to,
};
use coursecraft_scorm::Standard;
use coursecraft_xapi::Flavor;
use serde::Deserialize;

mod batch;
#[cfg(test)]
mod batch_tests;
#[cfg(test)]
mod render_layer_tests;
#[cfg(test)]
mod tests;

/// Package standard accepted by `--standard`.
#[derive(Clone, Copy, Debug, Default, ValueEnum)]
enum StandardArg {
    #[default]
    Scorm12,
    Scorm2004,
    Cmi5,
    Xapi,
    Lti,
    Web,
}

impl From<StandardArg> for PackageStandard {
    fn from(value: StandardArg) -> Self {
        match value {
            StandardArg::Scorm12 => Self::Scorm(Standard::Scorm12),
            StandardArg::Scorm2004 => Self::Scorm(Standard::Scorm2004),
            StandardArg::Cmi5 => Self::Xapi(Flavor::Cmi5),
            StandardArg::Xapi => Self::Xapi(Flavor::Xapi),
            StandardArg::Lti => Self::Lti,
            StandardArg::Web => Self::Web,
        }
    }
}

#[derive(Parser)]
#[command(name = "coursecraft-cli", version, about = "Headless CourseCraft")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Print the guide for AI agents: setup, every command, and how to work.
    Guide,
    /// Create a blank course.
    New {
        out: PathBuf,
        #[arg(long, default_value = "Untitled course")]
        title: String,
    },
    /// Check a course; exits 1 on errors.
    Validate { course: PathBuf },
    /// Check a course for accessibility problems; exits 1 on errors.
    #[command(name = "a11y-check")]
    A11yCheck {
        /// Path to course.json or .ccraft archive.
        course: PathBuf,
        /// Print the report as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Say in plain words what differs between two courses; exits 1 when they differ.
    Diff {
        /// The earlier course: a course.json, a folder or a .ccraft archive.
        a: PathBuf,
        /// The later course.
        b: PathBuf,
        /// Print the changes as a JSON array.
        #[arg(long)]
        json: bool,
    },
    /// Unpack a .ccraft into a project folder (for git).
    Unpack {
        /// The .ccraft archive to unpack.
        archive: PathBuf,
        /// The folder to create; it must not exist or must be empty.
        #[arg(short, long)]
        out: PathBuf,
    },
    /// Pack a project folder into a .ccraft archive.
    Pack {
        /// A project folder holding course.json.
        folder: PathBuf,
        /// The .ccraft file to write.
        #[arg(short, long)]
        out: PathBuf,
    },
    /// Play a course headlessly: feed inputs (JSON array) and print effects as JSON lines.
    Simulate {
        course: PathBuf,
        inputs: PathBuf,
        /// Draw seed for question banks (default 0).
        #[arg(long, default_value_t = 0)]
        seed: u32,
        /// Exit non-zero if any runtime warning is reported.
        #[arg(long)]
        strict: bool,
    },
    /// Apply a script of engine commands: JSON array of {"command": id, "params": {...}}.
    Run {
        course: PathBuf,
        script: PathBuf,
        #[arg(short, long)]
        out: PathBuf,
    },
    /// Add quiz questions from a CSV file to a question bank.
    #[command(name = "import-questions")]
    ImportQuestions {
        course: PathBuf,
        /// The CSV file (see examples/questions.csv).
        csv: PathBuf,
        #[arg(short, long)]
        out: PathBuf,
        /// Bank to add to (made if missing). Default: a new bank.
        #[arg(long)]
        bank: Option<String>,
    },
    /// Run one engine script on many courses and print a table of results.
    Batch {
        /// The script, in the same format as `run`.
        script: PathBuf,
        /// Courses: `.ccraft` files, project folders or `course.json`.
        courses: Vec<PathBuf>,
        /// Also take every course directly inside this folder.
        #[arg(long)]
        dir: Option<PathBuf>,
        /// Save each changed course over itself, safely.
        #[arg(long)]
        in_place: bool,
        /// Write each course to this folder instead.
        #[arg(long)]
        out_dir: Option<PathBuf>,
        /// Also build a package per course.
        #[arg(long, value_enum)]
        publish: Option<StandardArg>,
        /// Where `--publish` writes the packages.
        #[arg(long)]
        publish_dir: Option<PathBuf>,
        /// The built player files `--publish` needs.
        #[arg(long)]
        player: Option<PathBuf>,
        /// Swap a media file's bytes: `<path-in-course>=<file>`. Repeatable.
        #[arg(long = "replace-media")]
        replace_media: Vec<String>,
        /// Write nothing; show what would change.
        #[arg(long)]
        dry_run: bool,
    },
    /// List engine commands.
    Commands,
    /// Print the SCORM 1.2 imsmanifest.xml for a course.
    Manifest { course: PathBuf },
    /// Publish a course to a SCORM, cmi5, xAPI or LTI package.
    Publish {
        course: PathBuf,
        /// Built player folder (defaults to `target/player-dist` if present).
        #[arg(long)]
        player: Option<PathBuf>,
        #[arg(short, long)]
        out: Option<PathBuf>,
        #[arg(long)]
        out_dir: Option<PathBuf>,
        /// Standard to publish for.
        #[arg(long, value_enum, default_value = "scorm12")]
        standard: StandardArg,
    },
    /// Import a PowerPoint (.pptx) presentation.
    #[command(name = "import-pptx")]
    ImportPptx {
        deck: PathBuf,
        #[arg(short, long)]
        out: PathBuf,
    },
    /// Export a course to an MP4 video (macOS and Windows).
    #[command(name = "export-mp4")]
    ExportMp4 {
        /// Path to course.json or .ccraft archive.
        course: PathBuf,
        /// Output .mp4 path.
        #[arg(short, long)]
        out: PathBuf,
        /// Video width in pixels (default 1280).
        #[arg(long, default_value_t = 1280)]
        width: u32,
        /// Frames per second (default 30).
        #[arg(long, default_value_t = 30)]
        fps: u32,
    },
    /// Export a PDF for review: one page per slide, with notes and text.
    #[command(name = "export-pdf")]
    ExportPdf {
        /// Path to course.json or .ccraft archive.
        course: PathBuf,
        /// Output .pdf path.
        #[arg(short, long)]
        out: PathBuf,
        /// Add a page for every slide layer.
        #[arg(long)]
        layers: bool,
        /// Leave out the slide notes.
        #[arg(long)]
        no_notes: bool,
        /// Leave out the slide text.
        #[arg(long)]
        no_text: bool,
        /// Picture width in pixels (default 1600, clamp 320..=2400).
        #[arg(long, default_value_t = 1600)]
        width: u32,
    },
    /// Export a Word document for review: one page per slide, with notes and text.
    #[command(name = "export-docx")]
    ExportDocx {
        /// Path to course.json or .ccraft archive.
        course: PathBuf,
        /// Output .docx path.
        #[arg(short, long)]
        out: PathBuf,
        /// Add a page for every slide layer.
        #[arg(long)]
        layers: bool,
        /// Leave out the slide notes.
        #[arg(long)]
        no_notes: bool,
        /// Leave out the slide text.
        #[arg(long)]
        no_text: bool,
        /// Picture width in pixels (default 1600, clamp 320..=2400).
        #[arg(long, default_value_t = 1600)]
        width: u32,
    },
    /// Export every learner-visible string to XLIFF 1.2.
    #[command(name = "export-xliff")]
    ExportXliff {
        /// Path to course.json or .ccraft archive.
        course: PathBuf,
        /// BCP 47 language tag to translate into.
        #[arg(long)]
        target_language: String,
        /// Output .xlf path.
        #[arg(short, long)]
        out: PathBuf,
    },
    /// Apply an XLIFF 1.2 translation, writing a translated copy.
    #[command(name = "import-xliff")]
    ImportXliff {
        /// Path to course.json or .ccraft archive.
        course: PathBuf,
        /// Input .xlf path.
        xliff: PathBuf,
        /// Output course.json, .ccraft or directory (never the input).
        #[arg(short, long)]
        out: PathBuf,
    },
    /// Render a slide to PNG without a window.
    Render {
        /// Path to course.json or .ccraft archive.
        course: PathBuf,
        /// Slide identifier to render.
        #[arg(long)]
        slide: String,
        /// Image width in pixels (default 1280, clamp 16..=4096).
        #[arg(long, default_value = "1280")]
        width: u32,
        /// Rendering mode: authoring or player (default authoring).
        #[arg(long, default_value = "authoring")]
        mode: String,
        /// Specific layers to compose on top.
        #[arg(long = "layer")]
        layer: Vec<String>,
        /// Timestamp in milliseconds to tick to (player mode only).
        #[arg(long)]
        at_ms: Option<u32>,
        /// Output PNG path.
        #[arg(short, long)]
        out: PathBuf,
    },
    /// Call a method on a running editor through the control channel.
    Editor {
        /// JSON-RPC method name, e.g. commands.list, command.run, course.get.
        method: String,
        /// Method parameters as a JSON string.
        #[arg(default_value = "{}")]
        params: String,
        /// Override path to application config directory containing control.json.
        #[arg(long)]
        config_dir: Option<PathBuf>,
    },
    /// Snapshot the editor UI to a PNG file.
    #[command(name = "snapshot-editor")]
    SnapshotEditor {
        /// Path to course.json or .ccraft archive.
        #[arg(required_unless_present = "list_windows")]
        course: Option<PathBuf>,
        /// Theme mode: dark or light (default dark).
        #[arg(long, default_value = "dark")]
        theme: String,
        /// Size in points, formatted as WxH (default 1440x900).
        #[arg(long, default_value = "1440x900")]
        size: String,
        /// Scale factor (pixels per point, default 2.0).
        #[arg(long, default_value = "2.0")]
        scale: f32,
        /// Selected slide ID.
        #[arg(long)]
        slide: Option<String>,
        /// Selected object ID.
        #[arg(long)]
        object: Option<String>,
        /// Inspector tab: format, animate, slide, or course.
        #[arg(long)]
        inspector: Option<String>,
        /// Bottom panel tab: timeline, triggers, variables, or question.
        #[arg(long)]
        bottom: Option<String>,
        /// Render welcome screen instead of editor panels.
        #[arg(long)]
        welcome: bool,
        /// Open the keyboard shortcuts dialog.
        #[arg(long)]
        shortcuts: bool,
        /// Display a specific coach mark step (1, 2, or 3).
        #[arg(long)]
        coach: Option<u8>,
        /// Open the in-editor learner preview window.
        #[arg(long)]
        preview: bool,
        /// Show AI Settings on the Agents tab.
        #[arg(long)]
        connect_agent: bool,
        /// Snapshot a blank course.
        #[arg(long)]
        blank: bool,
        /// Open one editor window or dialog by name (see --list-windows).
        #[arg(long)]
        window: Option<String>,
        /// Print the names --window accepts and exit.
        #[arg(long)]
        list_windows: bool,
        /// Output PNG path.
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
    /// Snapshot the learner player UI to a PNG file.
    #[command(name = "snapshot-player")]
    SnapshotPlayer {
        /// Path to course.json or .ccraft archive.
        course: PathBuf,
        /// Slide ID to open (default: the course start slide).
        #[arg(long)]
        slide: Option<String>,
        /// Size in points, formatted as WxH (default 1280x720).
        #[arg(long, default_value = "1280x720")]
        size: String,
        /// Scale factor (pixels per point, default 2.0).
        #[arg(long, default_value = "2.0")]
        scale: f32,
        /// Player state: playing, paused, hidden, drawer, ended, loading, or start.
        #[arg(long, default_value = "playing")]
        state: String,
        /// Output PNG path.
        #[arg(short, long)]
        out: PathBuf,
    },
}

#[derive(Deserialize)]
struct Step {
    command: String,
    #[serde(default)]
    params: serde_json::Value,
}

type Res<T> = Result<T, String>;

struct DiskCliMedia(PathBuf);
impl coursecraft_render::MediaSource for DiskCliMedia {
    fn get(&self, src: &str) -> coursecraft_render::MediaFetch {
        if !coursecraft_format::is_safe_relative_path(src) {
            return coursecraft_render::MediaFetch::Missing;
        }
        let p = self.0.join(src);
        match std::fs::read(&p) {
            Ok(bytes) => coursecraft_render::MediaFetch::Ready(bytes.into()),
            Err(_) => coursecraft_render::MediaFetch::Missing,
        }
    }
}

struct InMemoryCliMedia(std::collections::HashMap<String, std::sync::Arc<[u8]>>);
impl coursecraft_render::MediaSource for InMemoryCliMedia {
    fn get(&self, src: &str) -> coursecraft_render::MediaFetch {
        self.0.get(src).cloned().map_or(
            coursecraft_render::MediaFetch::Missing,
            coursecraft_render::MediaFetch::Ready,
        )
    }
}

/// Whether `p` names a `.ccraft` archive (any letter case).
fn is_ccraft_path(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("ccraft"))
}

/// Resolve `p` for a same-file check even when it does not exist yet: the
/// parent is resolved and the file name added back.
fn resolve_for_compare(p: &Path) -> PathBuf {
    if let Ok(real) = std::fs::canonicalize(p) {
        return real;
    }
    let parent = p
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    match (std::fs::canonicalize(parent), p.file_name()) {
        (Ok(dir), Some(name)) => dir.join(name),
        _ => p.to_path_buf(),
    }
}

/// Refuse an output path that is the input course, or the `course.json` inside
/// an input project folder. Commands that promise "a copy" use this.
fn refuse_input_as_output(input: &Path, out: &Path) -> Res<()> {
    let target = resolve_for_compare(out);
    let input_file = resolve_for_compare(&course_file(input));
    if target == resolve_for_compare(input) || target == input_file {
        return Err(format!(
            "{}: the output must not be the input course; choose another path",
            out.display()
        ));
    }
    Ok(())
}

fn read(p: &Path) -> Res<String> {
    std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))
}

/// Load a course from a `course.json`, a project folder or a `.ccraft`. Every
/// command that takes a `<course>` goes through here.
fn load(p: &Path) -> Res<Course> {
    load_course_any(p)
}

/// The `course.json` a path names: the file itself, or the one inside a folder.
fn course_file(p: &Path) -> PathBuf {
    if p.is_dir() {
        p.join("course.json")
    } else {
        p.to_path_buf()
    }
}

/// Open a `.ccraft` archive by streaming from disk through a 256 KiB buffer.
pub(crate) fn open_ccraft_file(p: &Path) -> Res<coursecraft_format::Project> {
    let file = std::fs::File::open(p).map_err(|e| format!("{}: {e}", p.display()))?;
    let reader = std::io::BufReader::with_capacity(256 * 1024, file);
    coursecraft_format::open_project_from(reader).map_err(|e| format!("{}: {e}", p.display()))
}

/// Load `course.json`, a folder that contains it, or a `.ccraft` archive.
fn load_course_any(p: &Path) -> Res<Course> {
    let is_ccraft = p
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("ccraft"));
    if is_ccraft {
        return Ok(open_ccraft_file(p)?.course);
    }
    let target = if p.is_dir() {
        p.join("course.json")
    } else {
        p.to_path_buf()
    };
    let text =
        std::fs::read_to_string(&target).map_err(|e| format!("{}: {e}", target.display()))?;
    Course::from_json(&text).map_err(|e| format!("{}: {e}", target.display()))
}

const MAX_MEDIA_FILES: usize = 100_000;
const MAX_MEDIA_DEPTH: usize = 32;

/// Hash every regular file under `dir`, keyed by its path relative to `root`
/// with `/` separators. Symbolic links are skipped.
fn hash_media_dir(
    root: &Path,
    dir: &Path,
    depth: usize,
    out: &mut coursecraft_course::MediaHashes,
) -> Res<()> {
    if depth > MAX_MEDIA_DEPTH {
        return Ok(());
    }
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
        let path = entry.path();
        let kind = entry
            .file_type()
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if kind.is_dir() {
            hash_media_dir(root, &path, depth + 1, out)?;
        } else if kind.is_file() && out.len() < MAX_MEDIA_FILES {
            let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            out.insert(rel, coursecraft_format::fnv1a64(&bytes));
        }
    }
    Ok(())
}

/// A course and, when its media can be found, the hash of each media file:
/// a `.ccraft`'s `media/` entries, or the `media/` folder beside `course.json`.
fn load_with_media(p: &Path) -> Res<(Course, Option<coursecraft_course::MediaHashes>)> {
    let is_ccraft = p
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("ccraft"));
    if is_ccraft {
        let project = open_ccraft_file(p)?;
        let hashes = project
            .media
            .iter()
            .map(|(path, bytes)| (path.clone(), coursecraft_format::fnv1a64(bytes)))
            .collect();
        return Ok((project.course, Some(hashes)));
    }
    let course = load_course_any(p)?;
    let folder = if p.is_dir() {
        p.to_path_buf()
    } else {
        p.parent().map_or_else(PathBuf::new, Path::to_path_buf)
    };
    let media_dir = folder.join("media");
    if !media_dir.is_dir() {
        return Ok((course, None));
    }
    let mut hashes = coursecraft_course::MediaHashes::new();
    hash_media_dir(&media_dir, &media_dir, 0, &mut hashes)?;
    Ok((course, Some(hashes)))
}

/// `unpack <a.ccraft> -o <dir>`: the folder holds exactly what the zip held.
fn run_unpack(archive: &Path, out: &Path) -> Res<ExitCode> {
    let project = open_ccraft_file(archive)?;
    let occupied = std::fs::read_dir(out).is_ok_and(|mut entries| entries.next().is_some());
    if occupied || (out.exists() && !out.is_dir()) {
        return Err(format!(
            "{}: the output folder must not exist or must be empty",
            out.display()
        ));
    }
    let mut record = coursecraft_format::FolderRecord::new();
    let report =
        coursecraft_format::save_project_folder(out, &project.course, &project.media, &mut record)
            .map_err(|e| format!("{}: {e}", out.display()))?;
    println!(
        "Unpacked {} files into {}",
        report.written.len(),
        out.display()
    );
    Ok(ExitCode::SUCCESS)
}

/// `pack <dir> -o <a.ccraft>`: read the folder as the app does, write the zip.
fn run_pack(folder: &Path, out: &Path) -> Res<ExitCode> {
    if !folder.is_dir() {
        return Err(format!("{}: not a project folder", folder.display()));
    }
    let opened = coursecraft_format::open_project_folder(folder)
        .map_err(|e| format!("{}: {e}", folder.display()))?;
    write_ccraft_archive(out, &opened.project.course, &opened.project.media)?;
    println!(
        "Packed {} files into {}",
        opened.project.media.len() + 1,
        out.display()
    );
    Ok(ExitCode::SUCCESS)
}

/// The text `diff` prints and whether the courses differ.
fn diff_output(a: &Path, b: &Path, json: bool) -> Res<(String, bool)> {
    let (course_a, media_a) = load_with_media(a)?;
    let (course_b, media_b) = load_with_media(b)?;
    let media = media_a.as_ref().zip(media_b.as_ref());
    let changes = coursecraft_course::diff_with_media(&course_a, &course_b, media);
    let text = if json {
        serde_json::to_string_pretty(&changes).map_err(|e| e.to_string())?
    } else if changes.is_empty() {
        "No differences.".to_owned()
    } else {
        changes
            .iter()
            .map(coursecraft_course::Change::sentence)
            .collect::<Vec<_>>()
            .join("\n")
    };
    Ok((text, !changes.is_empty()))
}

/// `diff <a> <b>`: exit 0 when the same, 1 when different, 2 on error.
fn run_diff(a: &Path, b: &Path, json: bool) -> Res<ExitCode> {
    let (text, differs) = diff_output(a, b, json)?;
    println!("{text}");
    Ok(if differs {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}

fn audit_exit_code(report: &AuditReport) -> ExitCode {
    if report
        .findings
        .iter()
        .any(|f| f.severity == AuditSeverity::Error)
    {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn audit_json(report: &AuditReport) -> Res<String> {
    serde_json::to_string_pretty(report).map_err(|e| e.to_string())
}

fn audit_text(course: &Course, report: &AuditReport) -> String {
    use std::fmt::Write as _;

    let mut out = String::new();
    let mut current: Option<Option<&str>> = None;
    for finding in &report.findings {
        let slide = finding.slide.as_deref();
        if current != Some(slide) {
            current = Some(slide);
            match slide {
                Some(id) => {
                    let title = course.slide(id).map_or("", |s| s.title.as_str());
                    let _ = writeln!(out, "\nSlide: {title} ({id})");
                }
                None => {
                    let _ = writeln!(out, "\nCourse:");
                }
            }
        }
        let severity = match finding.severity {
            AuditSeverity::Error => "error",
            AuditSeverity::Warning => "warning",
            AuditSeverity::Note => "note",
        };
        let object = finding
            .object
            .as_deref()
            .map_or_else(String::new, |id| format!(" object {id:?}"));
        let _ = writeln!(out, "  {severity:<7} {}{object}", finding.criterion);
        let _ = writeln!(out, "    {}", finding.message);
        let _ = writeln!(out, "    fix: {}", finding.fix);
    }

    if report.findings.is_empty() {
        out.push_str("\nNo automated findings.\n");
    }
    let errors = report
        .findings
        .iter()
        .filter(|f| f.severity == AuditSeverity::Error)
        .count();
    let warnings = report
        .findings
        .iter()
        .filter(|f| f.severity == AuditSeverity::Warning)
        .count();
    let notes = report
        .findings
        .iter()
        .filter(|f| f.severity == AuditSeverity::Note)
        .count();
    let _ = writeln!(
        out,
        "\n{errors} error(s), {warnings} warning(s), {notes} note(s)"
    );
    out.push_str("\nAutomated checks only. These still need a person:\n");
    for check in report.manual_checks {
        let _ = writeln!(out, "- {check}");
    }
    out
}

fn run_a11y_check(course: PathBuf, json: bool) -> Res<ExitCode> {
    let loaded = load_course_any(&course)?;
    let report = loaded.audit();
    if json {
        println!("{}", audit_json(&report)?);
    } else {
        print!("{}", audit_text(&loaded, &report));
    }
    Ok(audit_exit_code(&report))
}

fn write(p: &Path, course: &Course) -> Res<()> {
    let json = course.to_json().map_err(|e| e.to_string())?;
    std::fs::write(p, json + "\n").map_err(|e| format!("{}: {e}", p.display()))
}

fn json_line<T: serde::Serialize>(v: &T) -> Res<()> {
    println!("{}", serde_json::to_string(v).map_err(|e| e.to_string())?);
    Ok(())
}

pub(crate) fn run_simulate(
    course: &Path,
    inputs: &Path,
    seed: u32,
    strict: bool,
    out: &mut impl std::io::Write,
    err: &mut impl std::io::Write,
) -> Res<ExitCode> {
    let c = load(course)?;
    let inputs: Vec<Input> =
        serde_json::from_str(&read(inputs)?).map_err(|e| format!("inputs: {e}"))?;
    let mut p = Player::with_draw_seed(&c, seed).map_err(|errs| {
        errs.iter()
            .map(|i| format!("{}: {}", i.path, i.message))
            .collect::<Vec<_>>()
            .join("\n")
    })?;
    let mut warning_count: usize = 0;
    let mut first_warning: Option<String> = None;
    for input in &inputs {
        for effect in p.handle(input) {
            if let Effect::Warning { ref message } = effect {
                warning_count += 1;
                if first_warning.is_none() {
                    first_warning = Some(message.clone());
                }
            }
            let line = serde_json::to_string(&effect).map_err(|e| e.to_string())?;
            writeln!(out, "{line}").map_err(|e| e.to_string())?;
        }
    }
    if strict && let Some(first) = first_warning {
        writeln!(
            err,
            "simulate --strict: {warning_count} runtime warning(s); first: {first}"
        )
        .map_err(|e| e.to_string())?;
        Ok(ExitCode::FAILURE)
    } else {
        Ok(ExitCode::SUCCESS)
    }
}

fn collect_player_files(dir: &Path) -> Res<Vec<PackageFile>> {
    if !dir.is_dir() {
        return Err(format!("player directory not found: {}", dir.display()));
    }
    let mut files = Vec::new();
    collect_files_recursive(dir, dir, &mut files)?;
    Ok(files)
}

fn collect_files_recursive(base: &Path, current: &Path, out: &mut Vec<PackageFile>) -> Res<()> {
    let entries = std::fs::read_dir(current).map_err(|e| format!("{}: {e}", current.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("{}: {e}", current.display()))?;
        let path = entry.path();
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let file_type = entry
            .file_type()
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if file_type.is_symlink() {
            eprintln!("warning: skipping symlink {}", path.display());
        } else if file_type.is_dir() {
            collect_files_recursive(base, &path, out)?;
        } else if file_type.is_file() {
            let rel = path
                .strip_prefix(base)
                .map_err(|e| format!("{}: {e}", path.display()))?;
            let mut parts = Vec::new();
            for comp in rel.components() {
                if let std::path::Component::Normal(os_str) = comp {
                    let s = os_str
                        .to_str()
                        .ok_or_else(|| format!("non-utf8 path: {}", path.display()))?;
                    parts.push(s);
                }
            }
            let rel_str = parts.join("/");
            let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            out.push(PackageFile {
                path: rel_str,
                bytes: std::sync::Arc::from(bytes),
            });
        }
    }
    Ok(())
}

fn run_render(
    course: PathBuf,
    slide: String,
    width: u32,
    mode: String,
    layer: Vec<String>,
    at_ms: Option<u32>,
    out: PathBuf,
) -> Res<ExitCode> {
    let render_mode = match mode.to_ascii_lowercase().as_str() {
        "authoring" => coursecraft_render::RenderMode::Authoring,
        "player" => coursecraft_render::RenderMode::Player,
        other => {
            eprintln!("unknown mode '{other}'; expected 'authoring' or 'player'");
            return Ok(ExitCode::FAILURE);
        }
    };

    let is_ccraft = course
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("ccraft"));

    let (course_model, media_source): (Course, Box<dyn coursecraft_render::MediaSource>) =
        if is_ccraft {
            let proj = open_ccraft_file(&course)?;
            let mut map = std::collections::HashMap::new();
            for (name, data) in proj.media {
                map.insert(name, std::sync::Arc::from(data));
            }
            (proj.course, Box::new(InMemoryCliMedia(map)))
        } else {
            let target = if course.is_dir() {
                course.join("course.json")
            } else {
                course.clone()
            };
            let text = std::fs::read_to_string(&target)
                .map_err(|e| format!("{}: {e}", target.display()))?;
            let c = Course::from_json(&text).map_err(|e| format!("{}: {e}", target.display()))?;
            let parent = target.parent().unwrap_or_else(|| Path::new("."));
            (c, Box::new(DiskCliMedia(parent.join("media"))))
        };

    let opts = coursecraft_render::RenderOptions {
        width,
        mode: render_mode,
        layers: layer,
        at_ms,
    };

    let img =
        coursecraft_render::render_slide_rgba(&course_model, &slide, &opts, media_source.as_ref())
            .map_err(|e| format!("render error: {e}"))?;

    let png_bytes =
        coursecraft_render::encode_png(&img).map_err(|e| format!("encode png error: {e}"))?;

    if let Some(parent) = out.parent()
        && !parent.as_os_str().is_empty()
    {
        let _ = std::fs::create_dir_all(parent);
    }

    std::fs::write(&out, png_bytes).map_err(|e| format!("{}: {e}", out.display()))?;

    println!(
        "Rendered slide '{}' ({}x{}) to {}",
        slide,
        img.width,
        img.height,
        out.display()
    );
    Ok(ExitCode::SUCCESS)
}

type CliMediaEntry = (String, std::sync::Arc<[u8]>);

fn parse_size(s: &str) -> Res<(u32, u32)> {
    let parts: Vec<&str> = s.split(['x', 'X']).collect();
    if parts.len() != 2 {
        return Err(format!("invalid size '{s}'; expected WxH (e.g. 1440x900)"));
    }
    let w: u32 = parts[0]
        .trim()
        .parse()
        .map_err(|_| format!("invalid width in '{s}'"))?;
    let h: u32 = parts[1]
        .trim()
        .parse()
        .map_err(|_| format!("invalid height in '{s}'"))?;
    if w == 0 || h == 0 {
        return Err(format!("size dimensions must be positive, got {w}x{h}"));
    }
    Ok((w, h))
}

fn load_media_from_dir(dir: &Path) -> Vec<CliMediaEntry> {
    let mut media = Vec::new();
    if !dir.is_dir() {
        return media;
    }
    let max_bytes = coursecraft_format::Limits::default().max_entry_bytes;
    // Each entry is a folder and its path inside `dir`, with `/` separators so
    // keys match the names a `.ccraft` uses on every platform.
    let mut stack = vec![(dir.to_path_buf(), String::new(), 0usize)];
    while let Some((current, prefix, depth)) = stack.pop() {
        if depth > MAX_MEDIA_DEPTH {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            // `file_type` does not follow links: a link, and so a link loop, is skipped.
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            let rel = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            if kind.is_dir() {
                stack.push((entry.path(), rel, depth + 1));
            } else if kind.is_file()
                && media.len() < MAX_MEDIA_FILES
                && coursecraft_format::is_safe_relative_path(&rel)
                && entry.metadata().is_ok_and(|m| m.len() <= max_bytes)
                && let Ok(bytes) = std::fs::read(entry.path())
            {
                media.push((rel, std::sync::Arc::from(bytes.as_slice())));
            }
        }
    }
    media.sort_by(|a, b| a.0.cmp(&b.0));
    media
}

#[allow(clippy::too_many_arguments)]
fn run_snapshot_editor(
    course: Option<PathBuf>,
    theme: String,
    size: String,
    scale: f32,
    slide: Option<String>,
    object: Option<String>,
    inspector: Option<String>,
    bottom: Option<String>,
    welcome: bool,
    shortcuts: bool,
    coach: Option<u8>,
    preview: bool,
    connect_agent: bool,
    blank: bool,
    window: Option<String>,
    list_windows: bool,
    out: Option<PathBuf>,
) -> Res<ExitCode> {
    if list_windows {
        for name in coursecraft_ui_egui::window_names() {
            println!("{name}");
        }
        return Ok(ExitCode::SUCCESS);
    }
    let (Some(course), Some(out)) = (course, out) else {
        return Err("snapshot-editor needs a course and -o <png> (or --list-windows)".to_owned());
    };
    if let Some(name) = window.as_deref()
        && coursecraft_ui_egui::window_spec(name).is_none()
    {
        let names: Vec<&str> = coursecraft_ui_egui::window_names().collect();
        return Err(format!(
            "unknown window '{name}'; expected one of: {}",
            names.join(", ")
        ));
    }
    let theme_mode = match theme.to_ascii_lowercase().as_str() {
        "dark" => coursecraft_render::ThemeMode::Dark,
        "light" => coursecraft_render::ThemeMode::Light,
        other => {
            eprintln!("unknown theme '{other}'; expected 'dark' or 'light'");
            return Ok(ExitCode::FAILURE);
        }
    };

    let (width, height) = parse_size(&size)?;

    let is_ccraft = course
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("ccraft"));

    let (course_model, media): (Course, Vec<CliMediaEntry>) = if blank {
        (
            coursecraft_engine::Session::blank("Untitled course").course,
            Vec::new(),
        )
    } else if is_ccraft {
        let proj = open_ccraft_file(&course)?;
        let media_vec = proj
            .media
            .into_iter()
            .map(|(n, d)| (n, std::sync::Arc::from(d)))
            .collect();
        (proj.course, media_vec)
    } else {
        let target = if course.is_dir() {
            course.join("course.json")
        } else {
            course.clone()
        };
        let text =
            std::fs::read_to_string(&target).map_err(|e| format!("{}: {e}", target.display()))?;
        let c = Course::from_json(&text).map_err(|e| format!("{}: {e}", target.display()))?;
        let parent = target.parent().unwrap_or_else(|| Path::new("."));
        let media_vec = load_media_from_dir(&parent.join("media"));
        (c, media_vec)
    };

    let inspector_tab = match inspector.as_deref() {
        Some("format") => Some(coursecraft_ui_egui::InspectorTab::Format),
        Some("animate") => Some(coursecraft_ui_egui::InspectorTab::Animate),
        Some("slide") => Some(coursecraft_ui_egui::InspectorTab::Slide),
        Some("course") => Some(coursecraft_ui_egui::InspectorTab::Course),
        Some(other) => {
            return Err(format!(
                "unknown inspector tab '{other}'; expected format, animate, slide, or course"
            ));
        }
        None => None,
    };

    let bottom_tab = match bottom.as_deref() {
        Some("timeline") => Some(coursecraft_ui_egui::BottomTab::Timeline),
        Some("triggers") => Some(coursecraft_ui_egui::BottomTab::Triggers),
        Some("variables") => Some(coursecraft_ui_egui::BottomTab::Variables),
        Some("question") => Some(coursecraft_ui_egui::BottomTab::Question),
        Some(other) => {
            return Err(format!(
                "unknown bottom tab '{other}'; expected timeline, triggers, variables, or question"
            ));
        }
        None => None,
    };

    let select = coursecraft_ui_egui::SnapshotSelect {
        slide,
        object,
        inspector_tab,
        bottom_tab,
        course_settings: false,
        theme_dialog: false,
        welcome,
        shortcuts,
        coach_mark: coach,
        preview,
        connect_agent,
        window,
    };

    let img = coursecraft_ui_egui::snapshot_editor(
        course_model,
        media,
        theme_mode,
        (width, height),
        scale,
        select,
    )
    .map_err(|e| format!("snapshot error: {e}"))?;

    let png_bytes =
        coursecraft_render::encode_png(&img).map_err(|e| format!("encode png error: {e}"))?;

    if let Some(parent) = out.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }

    std::fs::write(&out, png_bytes).map_err(|e| format!("{}: {e}", out.display()))?;
    println!(
        "Snapshotted editor ({}x{}) to {}",
        img.width,
        img.height,
        out.display()
    );
    Ok(ExitCode::SUCCESS)
}

fn run_snapshot_player(
    course: PathBuf,
    slide: Option<String>,
    size: String,
    scale: f32,
    state: String,
    out: PathBuf,
) -> Res<ExitCode> {
    let select = match state.to_ascii_lowercase().as_str() {
        "playing" => coursecraft_render::PlayerSnapshotSelect::Playing,
        "paused" => coursecraft_render::PlayerSnapshotSelect::Paused,
        "hidden" => coursecraft_render::PlayerSnapshotSelect::Hidden,
        "drawer" => coursecraft_render::PlayerSnapshotSelect::Drawer,
        "ended" => coursecraft_render::PlayerSnapshotSelect::Ended,
        "loading" => coursecraft_render::PlayerSnapshotSelect::Loading,
        "start" => coursecraft_render::PlayerSnapshotSelect::Start,
        other => {
            return Err(format!(
                "unknown player state '{other}'; expected playing, paused, hidden, drawer, ended, loading, or start"
            ));
        }
    };
    let (width, height) = parse_size(&size)?;
    let is_ccraft = course
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("ccraft"));

    let (course_model, media): (Course, Box<dyn coursecraft_render::MediaSource>) = if is_ccraft {
        let project = open_ccraft_file(&course)?;
        let media_map = project
            .media
            .into_iter()
            .map(|(name, data)| (name, std::sync::Arc::from(data)))
            .collect();
        (project.course, Box::new(InMemoryCliMedia(media_map)))
    } else {
        let target = if course.is_dir() {
            course.join("course.json")
        } else {
            course.clone()
        };
        let text =
            std::fs::read_to_string(&target).map_err(|e| format!("{}: {e}", target.display()))?;
        let model = Course::from_json(&text).map_err(|e| format!("{}: {e}", target.display()))?;
        let parent = target.parent().unwrap_or_else(|| Path::new("."));
        (model, Box::new(DiskCliMedia(parent.join("media"))))
    };

    let img = coursecraft_render::snapshot_player(
        course_model,
        slide.as_deref(),
        media.as_ref(),
        (width, height),
        scale,
        select,
    )
    .map_err(|e| format!("snapshot error: {e}"))?;

    let png_bytes =
        coursecraft_render::encode_png(&img).map_err(|e| format!("encode png error: {e}"))?;
    if let Some(parent) = out.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    std::fs::write(&out, png_bytes).map_err(|e| format!("{}: {e}", out.display()))?;
    println!(
        "Snapshotted player ({}x{}) to {}",
        img.width,
        img.height,
        out.display()
    );
    Ok(ExitCode::SUCCESS)
}

/// Read every file under `media/<path>/`, keeping paths relative to `media/`.
/// Links are skipped, the folder must be inside `media/`, and the web-folder
/// file and size caps apply. A refusal is reported on stderr.
fn collect_web_folder_files(
    media_dir: &Path,
    path: &str,
) -> Option<Vec<(String, std::sync::Arc<[u8]>)>> {
    if !media_dir.join(path.trim_end_matches('/')).is_dir() {
        return None;
    }
    match coursecraft_format::read_media_folder(
        media_dir,
        path,
        coursecraft_course::WEB_MAX_FILES,
        coursecraft_course::WEB_MAX_BYTES,
    ) {
        Ok(files) => Some(
            files
                .into_iter()
                .map(|(name, bytes)| (name, std::sync::Arc::from(bytes)))
                .collect(),
        ),
        Err(error) => {
            eprintln!("warning: web folder {path}: {error}");
            None
        }
    }
}

/// Read `media_dir/src` when it is a regular file that lives inside
/// `media_dir` (a link that leaves it is refused) and is within the size limit.
fn read_media_file(media_dir: &Path, src: &str) -> Option<Vec<u8>> {
    let root = std::fs::canonicalize(media_dir).ok()?;
    let file = std::fs::canonicalize(media_dir.join(src)).ok()?;
    if !file.starts_with(&root) {
        return None;
    }
    let meta = std::fs::metadata(&file).ok()?;
    if !meta.is_file() || meta.len() > coursecraft_format::Limits::default().max_entry_bytes {
        return None;
    }
    std::fs::read(&file).ok()
}

fn is_valid_player_dir(dir: &Path) -> bool {
    if !dir.is_dir() || !dir.join("index.html").is_file() {
        return false;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let path = entry.path();
        path.is_file() && path.extension().and_then(|ext| ext.to_str()) == Some("wasm")
    })
}

fn resolve_player_dir(player: Option<PathBuf>) -> Res<PathBuf> {
    resolve_player_dir_in(
        player,
        Path::new("."),
        std::env::current_exe().ok().as_deref(),
    )
}

fn resolve_player_dir_in(player: Option<PathBuf>, cwd: &Path, exe: Option<&Path>) -> Res<PathBuf> {
    if let Some(p) = player {
        return Ok(p);
    }
    let cwd_candidate = cwd.join("target/player-dist");
    if is_valid_player_dir(&cwd_candidate) {
        return Ok(cwd_candidate);
    }
    if let Some(exe_dir) = exe.and_then(Path::parent) {
        let candidates = [
            exe_dir.join("target/player-dist"),
            exe_dir.join("player-dist"),
            exe_dir
                .parent()
                .map(|p| p.join("player-dist"))
                .unwrap_or_default(),
            exe_dir
                .parent()
                .and_then(|p| p.parent())
                .map(|p| p.join("target/player-dist"))
                .unwrap_or_default(),
        ];
        for candidate in candidates {
            if !candidate.as_os_str().is_empty() && is_valid_player_dir(&candidate) {
                return Ok(candidate);
            }
        }
    }
    Err(
        "missing --player <folder> and no built player found at target/player-dist; build it with `cargo xtask build-player`"
            .to_string(),
    )
}

fn run_publish(
    course: PathBuf,
    player: Option<PathBuf>,
    out: Option<PathBuf>,
    out_dir: Option<PathBuf>,
    standard: PackageStandard,
) -> Res<ExitCode> {
    if out.is_none() && out_dir.is_none() {
        return Err("at least one of -o/--out or --out-dir is required".to_string());
    }

    let player = resolve_player_dir(player)?;
    let course = course_file(&course);
    // A `.ccraft` carries its own media; the folder beside it is not its media.
    let (c, archive_media): (Course, Option<MediaMap>) = if is_ccraft_path(&course) {
        let project = open_ccraft_file(&course)?;
        let map = project
            .media
            .into_iter()
            .map(|(name, bytes)| (name, std::sync::Arc::from(bytes)))
            .collect();
        (project.course, Some(map))
    } else {
        (load(&course)?, None)
    };

    let issues = c.validate();
    let errors = issues
        .iter()
        .filter(|i| i.severity == Severity::Error)
        .count();
    if errors > 0 {
        for i in &issues {
            if i.severity == Severity::Error {
                eprintln!("error: {} — {}", i.path, i.message);
            }
        }
        return Err(format!(
            "cannot publish: course has {errors} validation error(s)"
        ));
    }

    let player_files = collect_player_files(&player)?;

    let (published_course, embedded_files, embedded_families, font_warnings) =
        if needs_system_font_scan(&c) {
            let font_index =
                coursecraft_ui_egui::coursecraft_media::system_fonts::SystemFontIndex::scan_default(
                );
            let auto =
                coursecraft_publish::auto_embed_fonts(&c, Some(&CliFontProvider(&font_index)));
            let published = if auto.embedded_files.is_empty() {
                c
            } else {
                auto.course
            };
            (
                published,
                auto.embedded_files,
                auto.embedded_families,
                auto.warnings,
            )
        } else {
            (c, Vec::new(), Vec::new(), Vec::new())
        };
    let published_course_text = published_course
        .to_json_compact()
        .map_err(|e| e.to_string())?;

    let course_dir = course.parent().unwrap_or_else(|| Path::new("."));
    let course_dir = if course_dir.as_os_str().is_empty() {
        Path::new(".")
    } else {
        course_dir
    };

    // `src` is relative to the project's `media/` folder (docs/spec/course-model.md);
    // `build_package_standard_with_folders` checks it is a safe relative path before asking for it.
    let embedded_map: std::collections::HashMap<String, std::sync::Arc<[u8]>> = embedded_files
        .into_iter()
        .map(|(face, bytes)| (face.src, std::sync::Arc::from(bytes)))
        .collect();
    let media_resolver = |src: &str| {
        embedded_map
            .get(src)
            .cloned()
            .or_else(|| match &archive_media {
                Some(map) => map.get(src).cloned(),
                None => read_media_file(&course_dir.join("media"), src).map(std::sync::Arc::from),
            })
    };
    let web_folders = |path: &str| match &archive_media {
        Some(map) => {
            let prefix = format!("{}/", path.trim_end_matches('/'));
            let mut files: Vec<(String, std::sync::Arc<[u8]>)> = map
                .iter()
                .filter(|(name, _)| name.starts_with(&prefix))
                .map(|(name, bytes)| (name.clone(), std::sync::Arc::clone(bytes)))
                .collect();
            files.sort_by(|a, b| a.0.cmp(&b.0));
            (!files.is_empty()).then_some(files)
        }
        None => collect_web_folder_files(&course_dir.join("media"), path),
    };

    let pkg_input = PackageInput {
        course: &published_course,
        course_json: &published_course_text,
        player_files: &player_files,
        media: &media_resolver,
    };

    let files = build_package_standard_with_folders(&pkg_input, standard, &web_folders)
        .map_err(|e| format!("publish error: {e}"))?;

    if !embedded_families.is_empty() {
        println!("embedded fonts: {}", embedded_families.join(", "));
    }
    for w in &font_warnings {
        eprintln!("warning: {w}");
    }

    if let Some(dir) = &out_dir {
        for file in &files {
            let dest = dir.join(&file.path);
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("{}: {e}", parent.display()))?;
            }
            std::fs::write(&dest, &file.bytes).map_err(|e| format!("{}: {e}", dest.display()))?;
        }
    }

    let zip_len = if let Some(zip_path) = &out {
        if let Some(parent) = zip_path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        let file =
            std::fs::File::create(zip_path).map_err(|e| format!("{}: {e}", zip_path.display()))?;
        let mut writer = std::io::BufWriter::with_capacity(256 * 1024, file);
        zip_package_to(&files, &mut writer).map_err(|e| format!("zip error: {e}"))?;
        std::io::Write::flush(&mut writer).map_err(|e| format!("{}: {e}", zip_path.display()))?;
        std::fs::metadata(zip_path).map_or(0, |m| m.len())
    } else {
        0
    };

    println!("published {} file(s), {} bytes zip", files.len(), zip_len);
    Ok(ExitCode::SUCCESS)
}

type MediaMap = std::collections::HashMap<String, std::sync::Arc<[u8]>>;

struct LoadedProject {
    course: Course,
    media: Vec<(String, Vec<u8>)>,
}

fn load_project_any(p: &Path) -> Res<LoadedProject> {
    let is_ccraft = p
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("ccraft"));
    if is_ccraft {
        let project = open_ccraft_file(p)?;
        return Ok(LoadedProject {
            course: project.course,
            media: project.media,
        });
    }
    if p.is_dir() {
        let opened = coursecraft_format::open_project_folder(p)
            .map_err(|e| format!("{}: {e}", p.display()))?;
        return Ok(LoadedProject {
            course: opened.project.course,
            media: opened.project.media,
        });
    }
    let target = p.to_path_buf();
    let text =
        std::fs::read_to_string(&target).map_err(|e| format!("{}: {e}", target.display()))?;
    let course = Course::from_json(&text).map_err(|e| format!("{}: {e}", target.display()))?;
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    let media = load_media_from_dir(&parent.join("media"))
        .into_iter()
        .map(|(name, bytes)| (name, bytes.to_vec()))
        .collect();
    Ok(LoadedProject { course, media })
}

/// Synchronous media for an export; like `.ccraft`, it never reads the disk twice.
#[cfg(not(target_arch = "wasm32"))]
struct CliExportMedia(std::collections::HashMap<String, std::sync::Arc<[u8]>>);

#[cfg(not(target_arch = "wasm32"))]
impl coursecraft_export::ExportMedia for CliExportMedia {
    fn bytes(&self, src: &str) -> Option<std::sync::Arc<[u8]>> {
        self.0.get(src).cloned()
    }
}

/// Run `export-mp4` and return its exit code and, on success, the report JSON.
#[cfg(not(target_arch = "wasm32"))]
fn run_export_mp4(
    course: PathBuf,
    out: PathBuf,
    width: u32,
    fps: u32,
) -> Res<(ExitCode, Option<String>)> {
    if !cfg!(any(target_os = "macos", windows)) {
        eprintln!("MP4 export needs macOS or Windows.");
        return Ok((ExitCode::FAILURE, None));
    }

    let loaded = load_project_any(&course)?;
    if let Some(parent) = out.parent()
        && !parent.as_os_str().is_empty()
        && let Err(error) = std::fs::create_dir_all(parent)
    {
        eprintln!("error: {}: {error}", parent.display());
        return Ok((ExitCode::FAILURE, None));
    }

    let media = CliExportMedia(
        loaded
            .media
            .into_iter()
            .map(|(name, bytes)| (name, std::sync::Arc::from(bytes)))
            .collect(),
    );
    let options = coursecraft_export::Mp4Options {
        width,
        fps,
        ..Default::default()
    };
    let mut shown = None;
    let mut progress = |p: &coursecraft_export::ExportProgress| {
        if shown != Some(p.slide_index) {
            shown = Some(p.slide_index);
            let total = p.slide_total.max(1);
            let index = p.slide_index.saturating_add(1).min(total);
            eprintln!("slide {index}/{total}");
        }
        true
    };

    match coursecraft_export::export_mp4(&loaded.course, &media, &out, &options, &mut progress) {
        Ok(report) => {
            let json = serde_json::json!({
                "duration_ms": report.duration_ms,
                "frames": report.frames,
                "skipped_questions": report.skipped_questions,
                "branch_slides": report.branch_slides,
                "warnings": report.warnings,
            });
            let json = serde_json::to_string(&json).map_err(|e| e.to_string())?;
            Ok((ExitCode::SUCCESS, Some(json)))
        }
        Err(error) => {
            eprintln!("error: {error}");
            Ok((ExitCode::FAILURE, None))
        }
    }
}

/// The browser has no encoder.
#[cfg(target_arch = "wasm32")]
fn run_export_mp4(
    _course: PathBuf,
    _out: PathBuf,
    _width: u32,
    _fps: u32,
) -> Res<(ExitCode, Option<String>)> {
    eprintln!("MP4 export needs macOS or Windows.");
    Ok((ExitCode::FAILURE, None))
}

/// Run `export-pdf` and return its exit code.
fn run_export_pdf(
    course: PathBuf,
    out: PathBuf,
    layers: bool,
    no_notes: bool,
    no_text: bool,
    width: u32,
) -> Res<ExitCode> {
    let loaded = load_project_any(&course)?;
    if let Some(parent) = out.parent()
        && !parent.as_os_str().is_empty()
        && let Err(error) = std::fs::create_dir_all(parent)
    {
        eprintln!("error: {}: {error}", parent.display());
        return Ok(ExitCode::FAILURE);
    }

    let media = InMemoryCliMedia(
        loaded
            .media
            .into_iter()
            .map(|(name, bytes)| (name, std::sync::Arc::from(bytes)))
            .collect(),
    );
    let options = coursecraft_export::ReviewOptions {
        include_notes: !no_notes,
        include_text: !no_text,
        include_layers: layers,
        include_banks: true,
        image_width: width,
    };

    match coursecraft_export::export_pdf(&loaded.course, &media, &options) {
        Ok(bytes) => {
            if let Err(error) = std::fs::write(&out, &bytes) {
                eprintln!("error: {}: {error}", out.display());
                return Ok(ExitCode::FAILURE);
            }
            println!("wrote {} ({} bytes)", out.display(), bytes.len());
            Ok(ExitCode::SUCCESS)
        }
        Err(error) => {
            eprintln!("error: {error}");
            Ok(ExitCode::FAILURE)
        }
    }
}

/// Run `export-docx` and return its exit code.
fn run_export_docx(
    course: PathBuf,
    out: PathBuf,
    layers: bool,
    no_notes: bool,
    no_text: bool,
    width: u32,
) -> Res<ExitCode> {
    let loaded = load_project_any(&course)?;
    if let Some(parent) = out.parent()
        && !parent.as_os_str().is_empty()
        && let Err(error) = std::fs::create_dir_all(parent)
    {
        eprintln!("error: {}: {error}", parent.display());
        return Ok(ExitCode::FAILURE);
    }

    let media = InMemoryCliMedia(
        loaded
            .media
            .into_iter()
            .map(|(name, bytes)| (name, std::sync::Arc::from(bytes)))
            .collect(),
    );
    let options = coursecraft_export::ReviewOptions {
        include_notes: !no_notes,
        include_text: !no_text,
        include_layers: layers,
        include_banks: true,
        image_width: width,
    };

    match coursecraft_export::export_docx(&loaded.course, &media, &options) {
        Ok(bytes) => {
            if let Err(error) = std::fs::write(&out, &bytes) {
                eprintln!("error: {}: {error}", out.display());
                return Ok(ExitCode::FAILURE);
            }
            println!("wrote {} ({} bytes)", out.display(), bytes.len());
            Ok(ExitCode::SUCCESS)
        }
        Err(error) => {
            eprintln!("error: {error}");
            Ok(ExitCode::FAILURE)
        }
    }
}

/// Write `bytes` to `path` through a temporary file and a rename, creating the
/// parent folder. A failed write leaves any existing file as it was.
fn write_file_atomic(path: &Path, bytes: &[u8]) -> Res<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    let name = path
        .file_name()
        .map_or_else(|| "output".into(), |n| n.to_string_lossy().into_owned());
    let tmp = path.with_file_name(format!(".{name}.tmp-{}", std::process::id()));
    let result = std::fs::write(&tmp, bytes)
        .and_then(|()| std::fs::rename(&tmp, path))
        .map_err(|e| format!("{}: {e}", path.display()));
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// Write a `.ccraft` archive, checking that it opens again before it replaces
/// anything.
fn write_ccraft_archive<B: AsRef<[u8]>>(
    out: &Path,
    course: &Course,
    media: &[(String, B)],
) -> Res<()> {
    let bytes = coursecraft_format::save_project(course, media)
        .map_err(|e| format!("save project error: {e}"))?;
    coursecraft_format::verify_project(std::io::Cursor::new(&bytes))
        .map_err(|e| format!("the saved file could not be opened again ({e})"))?;
    write_file_atomic(out, &bytes)
}

/// Write `course.json` text and, when the course has media, the media into the
/// `media/` folder beside it, as a bare `course.json` expects. A file that
/// already holds the same bytes is left alone; a link is never written through.
fn write_course_json_file<B: AsRef<[u8]>>(
    out: &Path,
    course: &Course,
    media: &[(String, B)],
) -> Res<()> {
    let mut json = course.to_json().map_err(|e| e.to_string())?;
    json.push('\n');
    let media_dir = out
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .join("media");
    for (path, _) in media {
        if !coursecraft_format::is_safe_relative_path(path) {
            return Err(format!("media name {path:?} is not a safe path"));
        }
    }
    for (path, data) in media {
        let dest = media_dir.join(path);
        if std::fs::read(&dest).is_ok_and(|old| old == data.as_ref()) {
            continue;
        }
        let mut walk = media_dir.clone();
        let mut linked = media_dir.is_symlink();
        for segment in path.split('/') {
            walk.push(segment);
            linked |= walk.is_symlink();
        }
        if linked {
            return Err(format!(
                "{}: refusing to write through a link",
                dest.display()
            ));
        }
        write_file_atomic(&dest, data.as_ref())?;
    }
    write_file_atomic(out, json.as_bytes())
}

/// Write a course to `.ccraft`, `.json` (with its media beside it) or a project
/// folder, by the extension of `out`.
fn write_course_output<B: AsRef<[u8]> + Sync>(
    out: &Path,
    course: &Course,
    media: &[(String, B)],
) -> Res<()> {
    let is_json = out
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("json"));
    if is_ccraft_path(out) {
        return write_ccraft_archive(out, course, media);
    }
    if is_json {
        return write_course_json_file(out, course, media);
    }
    let mut record = coursecraft_format::FolderRecord::new();
    coursecraft_format::save_project_folder(out, course, media, &mut record)
        .map_err(|e| format!("{}: {e}", out.display()))?;
    Ok(())
}

fn run_export_xliff(course: PathBuf, target_language: String, out: PathBuf) -> Res<ExitCode> {
    let loaded = load_project_any(&course)?;
    let text = coursecraft_format::xliff::export_xliff(&loaded.course, &target_language)
        .map_err(|e| format!("export xliff error: {e}"))?;
    if let Some(parent) = out.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    std::fs::write(&out, text).map_err(|e| format!("{}: {e}", out.display()))?;
    println!("Exported translation to {}", out.display());
    Ok(ExitCode::SUCCESS)
}

fn print_import_report(report: &coursecraft_format::xliff::ImportReport) {
    println!("Applied {} unit(s).", report.applied);
    for (name, ids) in [
        ("missing target", &report.missing_target),
        ("source changed", &report.source_changed),
        ("unknown id", &report.unknown_ids),
        ("placeholder mismatch", &report.placeholder_mismatch),
        ("unsupported script", &report.unsupported_script),
    ] {
        if !ids.is_empty() {
            println!("{name}: {}", ids.join(", "));
        }
    }
}

fn run_import_xliff(course: PathBuf, xliff: PathBuf, out: PathBuf) -> Res<ExitCode> {
    refuse_input_as_output(&course, &out)?;
    let loaded = load_project_any(&course)?;
    let bytes = std::fs::read(&xliff).map_err(|e| format!("{}: {e}", xliff.display()))?;
    let doc = coursecraft_format::xliff::parse_xliff(&bytes)
        .map_err(|e| format!("{}: {e}", xliff.display()))?;
    let (translated, report) = coursecraft_format::xliff::apply_xliff(&loaded.course, &doc);
    if let Some(issue) = translated
        .validate()
        .into_iter()
        .find(|i| i.severity == Severity::Error)
    {
        return Err(format!(
            "translated course is invalid: {} ({})",
            issue.message, issue.path
        ));
    }
    write_course_output(&out, &translated, &loaded.media)?;
    print_import_report(&report);
    let skipped = !report.source_changed.is_empty() || !report.placeholder_mismatch.is_empty();
    Ok(if skipped {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    })
}

fn run_import_questions(
    course: PathBuf,
    csv: &Path,
    out: PathBuf,
    bank: Option<String>,
) -> Res<ExitCode> {
    refuse_input_as_output(&course, &out)?;
    let loaded = load_project_any(&course)?;
    let bytes = std::fs::read(csv).map_err(|e| format!("{}: {e}", csv.display()))?;
    let text = String::from_utf8(bytes)
        .map_err(|e| format!("{}: not valid UTF-8 ({e})", csv.display()))?;
    let mut session = Session::new(loaded.course);
    let mut params = serde_json::json!({ "csv": text });
    if let (Some(name), Some(map)) = (bank, params.as_object_mut()) {
        map.insert("bank".to_owned(), serde_json::Value::String(name));
    }
    let done = session
        .execute("bank.import_csv", &params)
        .map_err(|e| e.to_string())?;
    write_course_output(&out, &session.course, &loaded.media)?;
    println!(
        "Imported {} question(s) into bank {}",
        done["imported"].as_u64().unwrap_or(0),
        done["bank"].as_str().unwrap_or("?")
    );
    Ok(ExitCode::SUCCESS)
}

/// True when `course` references at least one non-Inter font face that is not
/// already embedded in `course.fonts`, matching [`coursecraft_publish::auto_embed_fonts`].
pub(crate) fn needs_system_font_scan(course: &Course) -> bool {
    coursecraft_publish::collect_used_font_faces(course)
        .into_iter()
        .any(|(family, style)| {
            !course
                .fonts
                .iter()
                .any(|f| f.family.eq_ignore_ascii_case(&family) && f.style == style)
        })
}

pub(crate) struct CliFontProvider<'a>(
    pub(crate) &'a coursecraft_ui_egui::coursecraft_media::system_fonts::SystemFontIndex,
);

impl coursecraft_publish::PublishFontProvider for CliFontProvider<'_> {
    fn lookup_font(
        &self,
        family: &str,
        style: coursecraft_course::FontStyle,
    ) -> coursecraft_publish::PublishFontLookup {
        use coursecraft_course::FontStyle;
        use coursecraft_ui_egui::coursecraft_media::FontFileStyle;
        let media_style = match style {
            FontStyle::Regular => FontFileStyle::Regular,
            FontStyle::Bold => FontFileStyle::Bold,
            FontStyle::Italic => FontFileStyle::Italic,
            FontStyle::BoldItalic => FontFileStyle::BoldItalic,
        };
        let Some((_, face)) = self.0.resolve_face(family, media_style) else {
            return coursecraft_publish::PublishFontLookup::Missing;
        };
        if face.embedding_restricted {
            return coursecraft_publish::PublishFontLookup::Restricted;
        }
        let Some(bytes) = self.0.load_face_bytes(face) else {
            return coursecraft_publish::PublishFontLookup::Unreadable;
        };
        let actual_style = match face.style {
            FontFileStyle::Regular => FontStyle::Regular,
            FontFileStyle::Bold => FontStyle::Bold,
            FontFileStyle::Italic => FontStyle::Italic,
            FontFileStyle::BoldItalic => FontStyle::BoldItalic,
        };
        let extension = if bytes.starts_with(b"OTTO") {
            "otf"
        } else if bytes.starts_with(b"ttcf") {
            "ttc"
        } else {
            "ttf"
        };
        coursecraft_publish::PublishFontLookup::Found {
            style: actual_style,
            bytes: bytes.to_vec(),
            extension,
        }
    }
}

fn run_import_pptx(deck: PathBuf, out: PathBuf) -> Res<ExitCode> {
    let bytes = std::fs::read(&deck).map_err(|e| format!("{}: {e}", deck.display()))?;
    let title = deck
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Imported course");
    let mut imported = coursecraft_pptx::import_pptx(&bytes, title)
        .map_err(|e| format!("{}: {e}", deck.display()))?;
    let font_index =
        coursecraft_ui_egui::coursecraft_media::system_fonts::SystemFontIndex::scan_default();
    imported
        .report
        .filter_installed_fonts(|name| font_index.has_family(name));
    if !imported.report.missing_fonts.is_empty() {
        eprintln!("{}", imported.report.summary_window_text());
    }

    write_course_output(&out, &imported.course, &imported.media)?;

    for item in &imported.report.items {
        println!("Slide {}: {}", item.slide, item.note);
    }
    let skipped_count = imported.report.items.len();
    println!(
        "{} slides, {} items skipped",
        imported.report.slides, skipped_count
    );

    Ok(ExitCode::SUCCESS)
}

fn explain_non_course_file(path: &Path) -> Option<&'static str> {
    if path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("csv"))
    {
        return Some(
            "This looks like a CSV file, not a course. Use `coursecraft-cli import-questions`.",
        );
    }
    let text = std::fs::read_to_string(path).ok()?;
    if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(&text)
        && !arr.is_empty()
    {
        if serde_json::from_str::<Vec<Step>>(&text).is_ok() {
            return Some(
                "This looks like a build script, not a course. Use `coursecraft-cli run`.",
            );
        }
        if serde_json::from_str::<Vec<Input>>(&text).is_ok() {
            return Some(
                "This looks like a simulate inputs file, not a course. Use `coursecraft-cli simulate`.",
            );
        }
    }
    None
}

fn run_editor(method: String, params: String, config_dir: Option<PathBuf>) -> Res<ExitCode> {
    let config_dir = config_dir
        .or_else(coursecraft_control::default_config_dir)
        .ok_or_else(|| "cannot determine config directory".to_string())?;

    let mut client = match coursecraft_control::ControlClient::connect_from_config(&config_dir) {
        Ok(c) => c,
        Err(
            coursecraft_control::ClientError::Discovery(_)
            | coursecraft_control::ClientError::Io(_),
        ) => {
            eprintln!(
                "no running editor: open the CourseCraft desktop app and check Help > Allow automation. \
                 Headless commands such as validate and run work without it."
            );
            return Ok(ExitCode::FAILURE);
        }
        Err(e) => {
            eprintln!("error: {e}");
            return Ok(ExitCode::FAILURE);
        }
    };

    let params_val: serde_json::Value = match serde_json::from_str(&params) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("invalid JSON params: {e}");
            return Ok(ExitCode::FAILURE);
        }
    };

    match client.call(&method, params_val) {
        Ok(res) => {
            let pretty = serde_json::to_string_pretty(&res)
                .map_err(|e| format!("failed to serialize result: {e}"))?;
            println!("{pretty}");
            Ok(ExitCode::SUCCESS)
        }
        Err(e) => {
            eprintln!("{e}");
            Ok(ExitCode::FAILURE)
        }
    }
}

fn run_validate(course: &Path, out: &mut impl std::io::Write) -> Res<ExitCode> {
    let c = match load(course) {
        Ok(c) => c,
        Err(err) => {
            if let Some(hint) = explain_non_course_file(course) {
                return Err(hint.to_string());
            }
            return Err(err);
        }
    };
    let issues = c.validate();
    for i in &issues {
        let tag = if i.severity == Severity::Error {
            "error"
        } else {
            "warning"
        };
        let _ = writeln!(out, "{tag}: {} — {}", i.path, i.message);
    }
    let font_warnings: Vec<String> = if needs_system_font_scan(&c) {
        let font_index =
            coursecraft_ui_egui::coursecraft_media::system_fonts::SystemFontIndex::scan_default();
        coursecraft_publish::auto_embed_fonts(&c, Some(&CliFontProvider(&font_index)))
            .warnings
            .into_iter()
            .filter(|w| !w.contains("responsibility"))
            .collect()
    } else {
        Vec::new()
    };
    for w in &font_warnings {
        let _ = writeln!(out, "warning: /fonts — {w}");
    }
    let glyph_warnings = coursecraft_render::undrawable_chars(&c);
    for u in &glyph_warnings {
        let place = match (&u.slide, &u.object) {
            (Some(s), Some(o)) => format!("/slides/{s}/objects/{o}"),
            (Some(s), None) => format!("/slides/{s}"),
            _ => "/title".to_owned(),
        };
        let _ = writeln!(
            out,
            "warning: {place} — no bundled font can draw U+{:04X} ({}); it shows as an empty box unless the font is embedded",
            u.ch as u32, u.ch
        );
    }
    let errors = issues
        .iter()
        .filter(|i| i.severity == Severity::Error)
        .count();
    let warnings = issues.len() - errors + font_warnings.len() + glyph_warnings.len();
    let _ = writeln!(out, "{errors} error(s), {warnings} warning(s)");
    Ok(if errors == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

fn run(cli: Cli) -> Res<ExitCode> {
    match cli.cmd {
        Cmd::New { out, title } => {
            let course = Session::blank(&title).course;
            if is_ccraft_path(&out) {
                write_ccraft_archive(&out, &course, &Vec::<(String, Vec<u8>)>::new())?;
            } else {
                write(&out, &course)?;
            }
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Validate { course } => {
            let mut out = std::io::stdout().lock();
            run_validate(&course, &mut out)
        }
        Cmd::A11yCheck { course, json } => run_a11y_check(course, json),
        Cmd::Diff { a, b, json } => run_diff(&a, &b, json),
        Cmd::Unpack { archive, out } => run_unpack(&archive, &out),
        Cmd::Pack { folder, out } => run_pack(&folder, &out),
        Cmd::Simulate {
            course,
            inputs,
            seed,
            strict,
        } => {
            let mut out = std::io::stdout().lock();
            let mut err = std::io::stderr().lock();
            run_simulate(&course, &inputs, seed, strict, &mut out, &mut err)
        }
        Cmd::Run {
            course,
            script,
            out,
        } => {
            let loaded = load_project_any(&course)?;
            let mut s = Session::new(loaded.course);
            let steps: Vec<Step> =
                serde_json::from_str(&read(&script)?).map_err(|e| format!("script: {e}"))?;
            for (i, step) in steps.iter().enumerate() {
                let result = s
                    .execute(&step.command, &step.params)
                    .map_err(|e| format!("step {i} ({}): {e}", step.command))?;
                json_line(
                    &serde_json::json!({ "step": i, "command": step.command, "result": result }),
                )?;
            }
            if is_ccraft_path(&out) {
                write_ccraft_archive(&out, &s.course, &loaded.media)?;
            } else {
                write_course_json_file(&out, &s.course, &loaded.media)?;
            }
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Batch {
            script,
            courses,
            dir,
            in_place,
            out_dir,
            publish,
            publish_dir,
            player,
            replace_media,
            dry_run,
        } => batch::run_batch(&batch::BatchArgs {
            script,
            courses,
            dir,
            in_place,
            out_dir,
            publish,
            publish_dir,
            player,
            replace_media,
            dry_run,
        }),
        Cmd::ImportQuestions {
            course,
            csv,
            out,
            bank,
        } => run_import_questions(course, &csv, out, bank),
        Cmd::Commands => {
            for c in registry() {
                let menu = if c.menu.is_empty() { "-" } else { c.menu };
                println!(
                    "{:<20} {:<24} {:<10} {}",
                    c.id,
                    menu,
                    c.shortcut.unwrap_or("-"),
                    c.params
                );
            }
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Manifest { course } => {
            let c = load(&course)?;
            let spec = coursecraft_scorm::ManifestSpec {
                identifier: format!("coursecraft.{}", c.id),
                title: c.title.clone(),
                launch: "index.html".into(),
                files: vec!["course.json".into()],
                mastery_score: c.quiz.map(|q| q.passing_percent),
            };
            print!(
                "{}",
                coursecraft_scorm::manifest_scorm12(&spec).map_err(|e| e.to_string())?
            );
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Publish {
            course,
            player,
            out,
            out_dir,
            standard,
        } => run_publish(course, player, out, out_dir, standard.into()),
        Cmd::ImportPptx { deck, out } => run_import_pptx(deck, out),
        Cmd::ExportMp4 {
            course,
            out,
            width,
            fps,
        } => {
            let (code, report) = run_export_mp4(course, out, width, fps)?;
            if let Some(report) = report {
                println!("{report}");
            }
            Ok(code)
        }
        Cmd::ExportPdf {
            course,
            out,
            layers,
            no_notes,
            no_text,
            width,
        } => run_export_pdf(course, out, layers, no_notes, no_text, width),
        Cmd::ExportDocx {
            course,
            out,
            layers,
            no_notes,
            no_text,
            width,
        } => run_export_docx(course, out, layers, no_notes, no_text, width),
        Cmd::ExportXliff {
            course,
            target_language,
            out,
        } => run_export_xliff(course, target_language, out),
        Cmd::ImportXliff { course, xliff, out } => run_import_xliff(course, xliff, out),
        Cmd::Render {
            course,
            slide,
            width,
            mode,
            layer,
            at_ms,
            out,
        } => run_render(course, slide, width, mode, layer, at_ms, out),
        Cmd::Guide => {
            print!("{}", include_str!("../../../docs/cli.md"));
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Editor {
            method,
            params,
            config_dir,
        } => run_editor(method, params, config_dir),
        Cmd::SnapshotEditor {
            course,
            theme,
            size,
            scale,
            slide,
            object,
            inspector,
            bottom,
            welcome,
            shortcuts,
            coach,
            preview,
            connect_agent,
            blank,
            window,
            list_windows,
            out,
        } => run_snapshot_editor(
            course,
            theme,
            size,
            scale,
            slide,
            object,
            inspector,
            bottom,
            welcome,
            shortcuts,
            coach,
            preview,
            connect_agent,
            blank,
            window,
            list_windows,
            out,
        ),
        Cmd::SnapshotPlayer {
            course,
            slide,
            size,
            scale,
            state,
            out,
        } => run_snapshot_player(course, slide, size, scale, state, out),
    }
}

/// Parse `args` (the first is the program name) and run the command.
pub fn main_with_args(args: impl IntoIterator<Item = std::ffi::OsString>) -> ExitCode {
    match run(Cli::parse_from(args)) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(2)
        }
    }
}
