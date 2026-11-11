//! Speed test on a 300-slide course with heavy media. Ignored: it writes about
//! 5 GB under `target/big-course/` and takes minutes. Run with:
//!
//! ```text
//! /usr/bin/time -l cargo test --release -p coursecraft-cli --test big_course -- --ignored --nocapture
//! ```
//!
//! It lives in `coursecraft-cli` because the editor app has no library target
//! and this crate already links the format, publish, logic and course crates.
//! `/usr/bin/time -l` reports the overall peak memory (no `unsafe` here).

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use coursecraft_course::{Course, Severity};
use coursecraft_format::{
    FolderRecord, Limits, open_project_folder, open_project_with_limits, save_project_folder,
    save_project_to,
};
use coursecraft_logic::{Input, Player};
use coursecraft_publish::{
    PackageFile, PackageInput, PackageStandard, build_package_standard_with_folders, zip_package_to,
};
use coursecraft_scorm::Standard;
use serde_json::{Value, json};

const SLIDES: usize = 300;
const SCENES: usize = 10;
const PNG_SIDE: usize = 590; // 590 * 590 * 3 + rows is about 1.04 MB, stored.
const VIDEO_BYTES: usize = 100 * 1024 * 1024;
const VIDEOS: usize = 10;

/// Deletes the working folder when dropped, even when the test fails.
struct Cleanup(PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        // `BIG_KEEP=1` keeps the folder between staged runs (see `BIG_STAGE`).
        if std::env::var_os("BIG_KEEP").is_none() {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 11
    }
    fn fill(&mut self, buf: &mut [u8]) {
        for chunk in buf.chunks_mut(8) {
            let bytes = self.next().to_le_bytes();
            for (d, s) in chunk.iter_mut().zip(bytes) {
                *d = s;
            }
        }
    }
}

fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (i, slot) in table.iter_mut().enumerate() {
        let mut c = i as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
        *slot = c;
    }
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc = table[((crc ^ u32::from(b)) & 0xFF) as usize] ^ (crc >> 8);
    }
    !crc
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &x in data {
        a = (a + u32::from(x)) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let mut body = kind.to_vec();
    body.extend_from_slice(data);
    out.extend_from_slice(&body);
    out.extend_from_slice(&crc32(&body).to_be_bytes());
}

/// A valid RGB PNG of random pixels in stored (uncompressed) deflate blocks.
fn random_png(rng: &mut Lcg) -> Vec<u8> {
    let mut raw = vec![0u8; PNG_SIDE * (PNG_SIDE * 3 + 1)];
    rng.fill(&mut raw);
    for row in raw.chunks_mut(PNG_SIDE * 3 + 1) {
        if let Some(first) = row.first_mut() {
            *first = 0; // filter: none
        }
    }
    let mut z = vec![0x78, 0x01];
    let mut blocks = raw.chunks(65535).peekable();
    while let Some(b) = blocks.next() {
        z.push(u8::from(blocks.peek().is_none()));
        z.extend_from_slice(&(b.len() as u16).to_le_bytes());
        z.extend_from_slice(&(!(b.len() as u16)).to_le_bytes());
        z.extend_from_slice(b);
    }
    z.extend_from_slice(&adler32(&raw).to_be_bytes());
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&(PNG_SIDE as u32).to_be_bytes());
    ihdr.extend_from_slice(&(PNG_SIDE as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(&mut png, b"IHDR", &ihdr);
    chunk(&mut png, b"IDAT", &z);
    chunk(&mut png, b"IEND", &[]);
    png
}

fn slide_json(n: usize) -> Value {
    let id = format!("s-{n}");
    let target = if n + 1 < SLIDES {
        json!("next")
    } else {
        json!({"id": "s-0"})
    };
    let mut objects = vec![
        json!({"id": format!("t{n}"), "kind": {"type": "text", "text": format!("Slide {n} title"), "style": {"size": 44, "bold": true}}, "frame": {"x": 60, "y": 40, "w": 1100, "h": 80}}),
        json!({"id": format!("a{n}"), "kind": {"type": "text", "text": "First paragraph of body text for the speed test."}, "frame": {"x": 60, "y": 140, "w": 500, "h": 160}}),
        json!({"id": format!("b{n}"), "kind": {"type": "text", "text": "Second paragraph of body text for the speed test."}, "frame": {"x": 60, "y": 320, "w": 500, "h": 160}}),
        json!({"id": format!("p{n}"), "kind": {"type": "image", "src": format!("img/p{n}.png"), "alt": "Test picture"}, "frame": {"x": 600, "y": 140, "w": 560, "h": 340}}),
        json!({"id": format!("h{n}"), "kind": {"type": "shape", "shape": "rounded_rectangle", "fill": "#2E8B57"}, "frame": {"x": 60, "y": 520, "w": 200, "h": 80}}),
        json!({"id": format!("btn{n}"), "kind": {"type": "button", "label": "Next"}, "frame": {"x": 1000, "y": 600, "w": 200, "h": 60}}),
    ];
    if n.is_multiple_of(SLIDES.div_ceil(VIDEOS)) {
        let v = n / SLIDES.div_ceil(VIDEOS);
        objects.push(json!({"id": format!("v{n}"), "kind": {"type": "video", "src": format!("video/v{v}.mp4")}, "frame": {"x": 600, "y": 500, "w": 280, "h": 160}}));
    }
    let mut slide = json!({
        "id": id, "title": format!("Slide {n}"), "duration_ms": 3000,
        "objects": objects,
        "triggers": [{"action": {"action": "jump_to_slide", "target": target}, "when": {"event": "click", "object": format!("btn{n}")}}]
    });
    if n.is_multiple_of(10) {
        slide["layers"] = json!([{"id": format!("lay{n}"), "name": "Hint", "objects": [
            {"id": format!("lt{n}"), "kind": {"type": "text", "text": "A hint."}, "frame": {"x": 300, "y": 200, "w": 500, "h": 100}}
        ]}]);
        slide["question"] = json!({"kind": "multiple_choice", "prompt": "Pick the right one", "choices": [
            {"id": "a", "text": "Wrong"}, {"id": "b", "text": "Right", "correct": true}
        ], "points": 10, "attempts": 2});
    }
    slide
}

fn build_course() -> Result<Course, String> {
    let per_scene = SLIDES / SCENES;
    let scenes: Vec<Value> = (0..SCENES)
        .map(|s| {
            json!({"id": format!("sc{s}"), "title": format!("Scene {s}"),
                "slides": (s * per_scene..(s + 1) * per_scene).map(slide_json).collect::<Vec<_>>()})
        })
        .collect();
    let doc = json!({"schema_version": 1, "id": "big", "title": "Big course",
        "slide_size": {"w": 1280, "h": 720}, "quiz": {"passing_percent": 50}, "scenes": scenes});
    Course::from_json(&doc.to_string()).map_err(|e| e.to_string())
}

fn free_gb() -> f64 {
    let out = std::process::Command::new("df")
        .args(["-k", "."])
        .output()
        .ok();
    out.and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| {
            s.lines()
                .nth(1)
                .and_then(|l| l.split_whitespace().nth(3).map(String::from))
        })
        .and_then(|k| k.parse::<f64>().ok())
        .map_or(0.0, |k| k / 1024.0 / 1024.0)
}

/// Current resident memory in MB (not the peak; the peak comes from `/usr/bin/time -l`).
fn rss_mb() -> f64 {
    let pid = std::process::id().to_string();
    std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &pid])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse::<f64>().ok())
        .map_or(0.0, |k| k / 1024.0)
}

fn row(stage: &str, took: Duration, note: &str) {
    println!(
        "| {stage:<28} | {:>9.0} ms | rss {:>5.0} MB | {note}",
        took.as_secs_f64() * 1000.0,
        rss_mb()
    );
}

#[test]
#[ignore = "writes about 5 GB and takes minutes; run by hand with --ignored"]
fn big_course_speed() -> Result<(), String> {
    assert!(
        free_gb() >= 15.0,
        "need 15 GB free, have {:.1} GB",
        free_gb()
    );
    let dir = PathBuf::from("../../target/big-course");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let _guard = Cleanup(dir.clone());

    // `BIG_STAGE=save|publish|open` runs one stage alone, so `/usr/bin/time -l`
    // gives that stage's own peak memory (macOS keeps freed pages in the
    // resident count, which inflates a single all-in-one run). Default: all.
    let stage = std::env::var("BIG_STAGE").unwrap_or_else(|_| "all".into());
    let run = |name: &str| stage == "all" || stage == name;

    if !run("save") && !run("publish") {
        return open_stage(&dir);
    }

    // Generate.
    let t = Instant::now();
    let mut rng = Lcg(0x5EED);
    let mut media: Vec<(String, Vec<u8>)> = Vec::new();
    for n in 0..SLIDES {
        media.push((format!("img/p{n}.png"), random_png(&mut rng)));
    }
    for v in 0..VIDEOS {
        let mut blob = vec![0u8; VIDEO_BYTES];
        rng.fill(&mut blob);
        media.push((format!("video/v{v}.mp4"), blob));
    }
    let course = build_course()?;
    let media_total: usize = media.iter().map(|(_, b)| b.len()).sum();
    row(
        "generate (not budgeted)",
        t.elapsed(),
        &format!("media {:.0} MB", media_total as f64 / 1e6),
    );

    // Validate.
    let t = Instant::now();
    let issues = course.validate();
    let validate_time = t.elapsed();
    let errors: Vec<_> = issues
        .iter()
        .filter(|i| i.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "validate errors: {:?}",
        errors.first().map(|i| (&i.path, &i.message))
    );
    row("validate (<= 200 ms)", validate_time, "");

    // Save.
    let file = dir.join("big.ccraft");
    if run("save") {
        let t = Instant::now();
        let out = std::fs::File::create(&file).map_err(|e| e.to_string())?;
        save_project_to(&course, &media, out).map_err(|e| e.to_string())?;
        let save_time = t.elapsed();
        let size = std::fs::metadata(&file).map_or(0, |m| m.len());
        row(
            "save .ccraft (<= 5 s)",
            save_time,
            &format!("file {:.0} MB", size as f64 / 1e6),
        );

        // What the desktop app adds to a save: check the written file (streamed,
        // no media in memory), and copy the file it replaces into the version
        // history.
        let t = Instant::now();
        let reader =
            std::io::BufReader::new(std::fs::File::open(&file).map_err(|e| e.to_string())?);
        coursecraft_format::verify_project(reader).map_err(|e| e.to_string())?;
        row("verify saved file", t.elapsed(), "CRC of every entry");
        let t = Instant::now();
        std::fs::copy(&file, dir.join("version.ccraft")).map_err(|e| e.to_string())?;
        row("keep a version (copy)", t.elapsed(), "");
        let _ = std::fs::remove_file(dir.join("version.ccraft"));
    }

    // The project as a folder for git: the first save writes everything, the
    // second finds nothing changed and writes nothing, and a reopen reads the
    // record back. Same 5 s budget as the .ccraft.
    if run("save") {
        let folder = dir.join("big.ccraft-folder");
        let mut record = FolderRecord::new();
        let t = Instant::now();
        let report = save_project_folder(&folder, &course, &media, &mut record)
            .map_err(|e| e.to_string())?;
        row(
            "save folder, first (<= 5 s)",
            t.elapsed(),
            &format!("{} files written", report.written.len()),
        );
        let t = Instant::now();
        let again = save_project_folder(&folder, &course, &media, &mut record)
            .map_err(|e| e.to_string())?;
        row(
            "save folder, unchanged (<= 5 s)",
            t.elapsed(),
            &format!("{} files written", again.written.len()),
        );
        assert!(again.written.is_empty(), "an unchanged save wrote files");
        assert!(again.deleted.is_empty());
    }

    // Publish SCORM 1.2 and the manifest-free zip, from the in-memory media.
    let player = vec![PackageFile {
        path: "index.html".into(),
        bytes: b"<html></html>".to_vec(),
    }];
    let course_json = course.to_json().map_err(|e| e.to_string())?;
    for (name, standard) in [
        (
            "publish SCORM 1.2 (<= 60 s)",
            PackageStandard::Scorm(Standard::Scorm12),
        ),
        ("publish LTI zip (<= 60 s)", PackageStandard::Lti),
    ] {
        if !run("publish") {
            break;
        }
        let t = Instant::now();
        let lookup: HashMap<&str, &[u8]> = media
            .iter()
            .map(|(n, b)| (n.as_str(), b.as_slice()))
            .collect();
        let resolve = |src: &str| lookup.get(src).map(|b| b.to_vec());
        let input = PackageInput {
            course: &course,
            course_json: &course_json,
            player_files: &player,
            media: &resolve,
        };
        let files = build_package_standard_with_folders(&input, standard, &|_| None)
            .map_err(|e| e.to_string())?;
        let out = std::fs::File::create(dir.join("out.zip")).map_err(|e| e.to_string())?;
        zip_package_to(&files, out).map_err(|e| e.to_string())?;
        let size = std::fs::metadata(dir.join("out.zip")).map_or(0, |m| m.len());
        row(
            name,
            t.elapsed(),
            &format!("zip {:.0} MB", size as f64 / 1e6),
        );
    }
    drop(media);

    if stage == "all" {
        return open_stage(&dir);
    }
    println!(
        "peak memory budget: {:.0} MB",
        (2.0 * media_total as f64 + 500e6) / 1e6
    );
    Ok(())
}

/// Open the saved project as the app does (read the file, then parse it), then
/// simulate a run through every slide.
fn open_stage(dir: &std::path::Path) -> Result<(), String> {
    let file = dir.join("big.ccraft");
    let t = Instant::now();
    let bytes = std::fs::read(&file).map_err(|e| e.to_string())?;
    let project = open_project_with_limits(&bytes, Limits::default()).map_err(|e| e.to_string())?;
    row(
        "open .ccraft (<= 5 s)",
        t.elapsed(),
        &format!("{} media files", project.media.len()),
    );
    drop(bytes);
    drop(project.media);
    let course = project.course;

    let folder = dir.join("big.ccraft-folder");
    if folder.is_dir() {
        let t = Instant::now();
        let opened = open_project_folder(&folder).map_err(|e| e.to_string())?;
        row(
            "open folder (<= 5 s)",
            t.elapsed(),
            &format!("{} media files", opened.project.media.len()),
        );
    }

    // Simulate a run through every slide.
    let t = Instant::now();
    let mut player = Player::new(&course).map_err(|e| format!("{} issues", e.len()))?;
    player.handle(&Input::Start);
    let mut visited = 0usize;
    for n in 0..SLIDES {
        if n.is_multiple_of(10) {
            player.handle(&Input::SelectChoice { choice: "b".into() });
            player.handle(&Input::Submit);
        }
        player.handle(&Input::Click {
            object: format!("btn{n}"),
        });
        visited += 1;
    }
    row(
        "simulate all slides",
        t.elapsed(),
        &format!("{visited} slides"),
    );
    Ok(())
}
