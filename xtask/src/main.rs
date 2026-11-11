//! `cargo xtask <task>`. Tasks: `layers` (enforce the crate layering), `wasm`
//! (L0–L5 and the player build for `wasm32-unknown-unknown`), `ci`
//! (fmt + clippy + test + layers + wasm), `dep-src` (copy the sources of key
//! dependencies into `target/dep-src/` for reading, e.g. by coding agents that
//! may only read inside the workspace), `build-player` (build the release web
//! player into `target/player-dist/` for the desktop editor to embed),
//! `package-example` (build the web player with trunk, publish
//! `examples/hello-course` into the mock LMS at `target/mock-lms/` and to
//! `target/hello-course.zip`; also publishes the showcase as SCORM 2004 to
//! `target/mock-lms/scorm2004/` and `target/showcase-2004.zip`), and `mock-ai`
//! (a local OpenAI-compatible endpoint for trying AI features offline).

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod mock_ai;
mod mock_lrs;

use std::path::Path;
use std::process::{Command, ExitCode};

use serde_json::Value;

/// Layer of every workspace library crate (name without `coursecraft-`).
/// A crate may only depend on crates in strictly lower layers; a standalone
/// crate may depend on no workspace crate at all.
const LAYERS: &[(&str, u8, bool)] = &[
    // (name, layer, standalone)
    ("scorm", 0, true),
    ("xapi", 0, true),
    ("course", 1, false),
    ("video-native", 1, true),
    ("logic", 2, false),
    ("media", 2, true),
    ("playback", 2, false),
    ("render", 3, false),
    ("ai", 3, false),
    ("format", 3, false),
    ("pptx", 3, false),
    ("lms", 4, false),
    ("lti", 4, true),
    ("publish", 4, false),
    ("export", 4, false),
    ("engine", 5, false),
    ("ui-egui", 6, false),
    ("control", 6, false),
];

/// Binaries and tooling are exempt.
const EXEMPT: &[&str] = &[
    "coursecraft",
    "coursecraft-cli",
    "coursecraft-lti-server",
    "coursecraft-mcp",
    "coursecraft-player",
    "xtask",
];

fn layer_of(pkg: &str) -> Option<(u8, bool)> {
    let short = pkg.strip_prefix("coursecraft-")?;
    LAYERS
        .iter()
        .find(|(n, ..)| *n == short)
        .map(|(_, l, s)| (*l, *s))
}

fn check_layers() -> Result<(), Vec<String>> {
    let out = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .output()
        .map_err(|e| vec![format!("cargo metadata: {e}")])?;
    let meta: Value = serde_json::from_slice(&out.stdout)
        .map_err(|e| vec![format!("cargo metadata json: {e}")])?;
    let Some(packages) = meta["packages"].as_array() else {
        return Err(vec!["cargo metadata: no packages".into()]);
    };
    let mut problems = Vec::new();
    for pkg in packages {
        let name = pkg["name"].as_str().unwrap_or_default();
        if EXEMPT.contains(&name) {
            continue;
        }
        let Some((layer, standalone)) = layer_of(name) else {
            problems.push(format!(
                "{name}: not registered in xtask/src/main.rs LAYERS"
            ));
            continue;
        };
        for dep in pkg["dependencies"].as_array().into_iter().flatten() {
            let dep_name = dep["name"].as_str().unwrap_or_default();
            if dep["kind"].as_str() == Some("dev") || !dep_name.starts_with("coursecraft-") {
                continue;
            }
            if standalone {
                problems.push(format!(
                    "{name} (standalone) depends on workspace crate {dep_name}"
                ));
                continue;
            }
            match layer_of(dep_name) {
                Some((dl, _)) if dl < layer => {}
                Some((dl, _)) => problems.push(format!(
                    "{name} (L{layer}) depends on {dep_name} (L{dl}): only lower layers allowed"
                )),
                None => problems.push(format!(
                    "{name} depends on unregistered or exempt crate {dep_name}"
                )),
            }
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems)
    }
}

fn cargo(args: &[&str]) -> bool {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    println!("$ cargo {}", args.join(" "));
    Command::new(cargo)
        .args(args)
        .status()
        .is_ok_and(|s| s.success())
}

/// Crates that must keep building for the browser.
const WASM_CRATES: &[&str] = &[
    "coursecraft-scorm",
    "coursecraft-course",
    "coursecraft-logic",
    "coursecraft-media",
    "coursecraft-playback",
    "coursecraft-render",
    "coursecraft-ai",
    "coursecraft-format",
    "coursecraft-pptx",
    "coursecraft-lms",
    "coursecraft-publish",
    "coursecraft-export",
    "coursecraft-engine",
    "coursecraft-ui-egui",
    "coursecraft-player",
    "coursecraft",
];

fn check_wasm() -> bool {
    let mut args = vec!["check", "--target", "wasm32-unknown-unknown"];
    for c in WASM_CRATES {
        args.extend(["-p", c]);
    }
    cargo(&args)
}

/// Dependencies copied whole by `dep-src`.
const DEP_SRC_CRATES: &[&str] = &[
    "image",
    "rodio",
    "cpal",
    "egui",
    "epaint",
    "eframe",
    "emath",
    "ecolor",
    "egui-wgpu",
    "egui-winit",
    "zip",
    "quick-xml",
    "js-sys",
    "wasm-bindgen",
    "wasm-bindgen-futures",
    "ehttp",
    "rfd",
    "objc2",
    "objc2-foundation",
    "objc2-core-foundation",
    "objc2-core-media",
    "objc2-core-video",
    "objc2-av-foundation",
    "windows",
    "skrifa",
    "read-fonts",
    "resvg",
    "usvg",
    "tiny-skia",
    "velato",
    "kurbo",
    "peniko",
];

/// Single `web-sys` binding files copied by `dep-src` (the crate is huge).
const DEP_SRC_WEB_SYS: &[&str] = &[
    "gen_Window.rs",
    "gen_Document.rs",
    "gen_Performance.rs",
    "gen_EventTarget.rs",
    "gen_PageTransitionEvent.rs",
    "gen_console.rs",
    "gen_Location.rs",
    "gen_Element.rs",
    "gen_HtmlElement.rs",
    "gen_HtmlInputElement.rs",
    "gen_HtmlSelectElement.rs",
    "gen_HtmlOptionElement.rs",
    "gen_HtmlButtonElement.rs",
    "gen_HtmlImageElement.rs",
    "gen_Node.rs",
    "gen_Event.rs",
    "gen_FocusEvent.rs",
    "gen_KeyboardEvent.rs",
    "gen_CssStyleDeclaration.rs",
];

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn dep_src() -> Result<(), String> {
    let out = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(["metadata", "--format-version", "1"])
        .output()
        .map_err(|e| format!("cargo metadata: {e}"))?;
    let meta: Value =
        serde_json::from_slice(&out.stdout).map_err(|e| format!("cargo metadata json: {e}"))?;
    let target_dir = meta["target_directory"]
        .as_str()
        .ok_or("cargo metadata: no target_directory")?;
    let dest = Path::new(target_dir).join("dep-src");
    for pkg in meta["packages"].as_array().into_iter().flatten() {
        let (Some(name), Some(version), Some(manifest)) = (
            pkg["name"].as_str(),
            pkg["version"].as_str(),
            pkg["manifest_path"].as_str(),
        ) else {
            continue;
        };
        let Some(root) = Path::new(manifest).parent() else {
            continue;
        };
        let to = dest.join(format!("{name}-{version}"));
        if DEP_SRC_CRATES.contains(&name) {
            copy_dir(root, &to).map_err(|e| format!("{name}: {e}"))?;
            println!("dep-src: {name}-{version}");
        } else if name == "web-sys" {
            let features = to.join("src/features");
            std::fs::create_dir_all(&features).map_err(|e| format!("web-sys: {e}"))?;
            for file in DEP_SRC_WEB_SYS {
                std::fs::copy(root.join("src/features").join(file), features.join(file))
                    .map_err(|e| format!("web-sys {file}: {e}"))?;
            }
            println!("dep-src: {name}-{version} (selected bindings)");
        }
    }
    Ok(())
}

/// Publish one course for one standard into `out_dir`, optionally zipping it.
fn publish_course(
    dist: &Path,
    course: &str,
    standard: &str,
    out_dir: &Path,
    zip: Option<&Path>,
) -> Result<(), String> {
    if out_dir.exists() {
        std::fs::remove_dir_all(out_dir).map_err(|e| format!("{}: {e}", out_dir.display()))?;
    }
    std::fs::create_dir_all(out_dir).map_err(|e| format!("{}: {e}", out_dir.display()))?;
    let dist_arg = dist.to_string_lossy().into_owned();
    let out_arg = out_dir.to_string_lossy().into_owned();
    let mut args: Vec<String> = vec![
        "run".into(),
        "-q".into(),
        "-p".into(),
        "coursecraft-cli".into(),
        "--".into(),
        "publish".into(),
        "--standard".into(),
        standard.into(),
        course.into(),
        "--player".into(),
        dist_arg,
        "--out-dir".into(),
        out_arg,
    ];
    if let Some(zip) = zip {
        args.push("-o".into());
        args.push(zip.to_string_lossy().into_owned());
    }
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    if !cargo(&refs) {
        return Err(format!(
            "coursecraft-cli publish failed for {course} ({standard})"
        ));
    }
    Ok(())
}

/// Build the release web player into `target/player-dist` for the desktop editor to embed.
fn build_player() -> Result<(), String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("xtask has no parent directory")?;
    let player_dir = root.join("apps/coursecraft-player");
    let dist = root.join("target/player-dist");
    if dist.exists() {
        std::fs::remove_dir_all(&dist).map_err(|e| format!("{}: {e}", dist.display()))?;
    }
    println!("$ trunk build --release --dist {}", dist.display());
    let built = Command::new("trunk")
        .args(["build", "--release", "--dist"])
        .arg(&dist)
        .current_dir(&player_dir)
        .status()
        .map_err(|e| format!("trunk: {e} (install with `cargo install trunk`)"))?;
    if !built.success() {
        return Err("trunk build failed".into());
    }
    println!("build-player: player written to {}", dist.display());
    Ok(())
}

/// Build the release web player, publish hello-course and showcase into the mock LMS.
fn package_example() -> Result<(), String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("xtask has no parent directory")?;
    let player_dir = root.join("apps/coursecraft-player");
    println!("$ trunk build --release (in apps/coursecraft-player)");
    let built = Command::new("trunk")
        .args(["build", "--release"])
        .current_dir(&player_dir)
        .status()
        .map_err(|e| format!("trunk: {e} (install with `cargo install trunk`)"))?;
    if !built.success() {
        return Err("trunk build failed".into());
    }
    let mock = root.join("target/mock-lms");
    let package = mock.join("package");
    if package.exists() {
        std::fs::remove_dir_all(&package).map_err(|e| format!("{}: {e}", package.display()))?;
    }
    std::fs::create_dir_all(&mock).map_err(|e| format!("{}: {e}", mock.display()))?;
    let dist = player_dir.join("dist");
    let package_arg = package.to_string_lossy().into_owned();
    let dist_arg = dist.to_string_lossy().into_owned();
    let zip_arg = root
        .join("target/hello-course.zip")
        .to_string_lossy()
        .into_owned();
    let published = cargo(&[
        "run",
        "-q",
        "-p",
        "coursecraft-cli",
        "--",
        "publish",
        "examples/hello-course/course.json",
        "--player",
        &dist_arg,
        "--out-dir",
        &package_arg,
        "-o",
        &zip_arg,
    ]);
    if !published {
        return Err("coursecraft-cli publish failed for hello-course".into());
    }
    std::fs::copy(
        root.join("xtask/mock-lms/index.html"),
        mock.join("index.html"),
    )
    .map_err(|e| format!("mock LMS page: {e}"))?;

    let showcase_mock = mock.join("showcase");
    let showcase_package = showcase_mock.join("package");
    if showcase_package.exists() {
        std::fs::remove_dir_all(&showcase_package)
            .map_err(|e| format!("{}: {e}", showcase_package.display()))?;
    }
    std::fs::create_dir_all(&showcase_mock)
        .map_err(|e| format!("{}: {e}", showcase_mock.display()))?;
    let showcase_package_arg = showcase_package.to_string_lossy().into_owned();
    let showcase_zip_arg = root
        .join("target/showcase.zip")
        .to_string_lossy()
        .into_owned();
    let published_showcase = cargo(&[
        "run",
        "-q",
        "-p",
        "coursecraft-cli",
        "--",
        "publish",
        "examples/showcase/course.json",
        "--player",
        &dist_arg,
        "--out-dir",
        &showcase_package_arg,
        "-o",
        &showcase_zip_arg,
    ]);
    if !published_showcase {
        return Err("coursecraft-cli publish failed for showcase".into());
    }
    std::fs::copy(
        root.join("xtask/mock-lms/index.html"),
        showcase_mock.join("index.html"),
    )
    .map_err(|e| format!("mock LMS showcase page: {e}"))?;

    let scorm2004_mock = mock.join("scorm2004");
    let scorm2004_package = scorm2004_mock.join("package");
    if scorm2004_package.exists() {
        std::fs::remove_dir_all(&scorm2004_package)
            .map_err(|e| format!("{}: {e}", scorm2004_package.display()))?;
    }
    std::fs::create_dir_all(&scorm2004_mock)
        .map_err(|e| format!("{}: {e}", scorm2004_mock.display()))?;
    let scorm2004_package_arg = scorm2004_package.to_string_lossy().into_owned();
    let scorm2004_zip_arg = root
        .join("target/showcase-2004.zip")
        .to_string_lossy()
        .into_owned();
    let published_2004 = cargo(&[
        "run",
        "-q",
        "-p",
        "coursecraft-cli",
        "--",
        "publish",
        "--standard",
        "scorm2004",
        "examples/showcase/course.json",
        "--player",
        &dist_arg,
        "--out-dir",
        &scorm2004_package_arg,
        "-o",
        &scorm2004_zip_arg,
    ]);
    if !published_2004 {
        return Err("coursecraft-cli publish failed for SCORM 2004 showcase".into());
    }
    std::fs::copy(
        root.join("xtask/mock-lms/index.html"),
        scorm2004_mock.join("index.html"),
    )
    .map_err(|e| format!("mock LMS SCORM 2004 page: {e}"))?;

    let mock_lrs = root.join("target/mock-lrs");
    std::fs::create_dir_all(&mock_lrs).map_err(|e| format!("{}: {e}", mock_lrs.display()))?;
    std::fs::copy(
        root.join("xtask/mock-lrs/index.html"),
        mock_lrs.join("index.html"),
    )
    .map_err(|e| format!("mock LRS page: {e}"))?;

    for flavor in ["cmi5", "xapi"] {
        let showcase = mock_lrs.join(flavor).join("showcase");
        let zip = root.join(format!("target/showcase-{flavor}.zip"));
        publish_course(
            &dist,
            "examples/showcase/course.json",
            flavor,
            &showcase,
            Some(&zip),
        )?;
        let hello = mock_lrs.join(flavor).join("hello");
        publish_course(
            &dist,
            "examples/hello-course/course.json",
            flavor,
            &hello,
            None,
        )?;
    }

    println!(
        "package-example: open target/mock-lms/index.html, /showcase/index.html or /scorm2004/index.html over http (launch config `mock-lms`); xAPI packages are under target/mock-lrs (launch config `mock-lrs`)"
    );
    Ok(())
}

fn parse_port(args: impl Iterator<Item = String>) -> Result<u16, String> {
    parse_port_with_default(args, 8091)
}

fn parse_port_with_default(
    args: impl Iterator<Item = String>,
    default: u16,
) -> Result<u16, String> {
    let mut port = default;
    let mut args = args;
    while let Some(arg) = args.next() {
        if arg == "--port" {
            let Some(value) = args.next() else {
                return Err("--port needs a value".into());
            };
            port = value
                .parse::<u16>()
                .map_err(|_| format!("invalid port {value:?}"))?;
        } else {
            return Err(format!("unknown argument {arg:?}"));
        }
    }
    Ok(port)
}

fn main() -> ExitCode {
    let task = std::env::args().nth(1).unwrap_or_default();
    let layers = || match check_layers() {
        Ok(()) => {
            println!("layers: ok");
            true
        }
        Err(problems) => {
            for p in problems {
                eprintln!("layers: {p}");
            }
            false
        }
    };
    let ok = match task.as_str() {
        "layers" => layers(),
        "wasm" => check_wasm(),
        "dep-src" => dep_src().map_err(|e| eprintln!("dep-src: {e}")).is_ok(),
        "build-player" => build_player()
            .map_err(|e| eprintln!("build-player: {e}"))
            .is_ok(),
        "package-example" => package_example()
            .map_err(|e| eprintln!("package-example: {e}"))
            .is_ok(),
        "mock-lrs" => match parse_port(std::env::args().skip(2)) {
            Ok(port) => mock_lrs::run(port)
                .map_err(|e| eprintln!("mock-lrs: {e}"))
                .is_ok(),
            Err(err) => {
                eprintln!("mock-lrs: {err}");
                false
            }
        },
        "mock-ai" => match parse_port_with_default(std::env::args().skip(2), 8099) {
            Ok(port) => mock_ai::run(port)
                .map_err(|e| eprintln!("mock-ai: {e}"))
                .is_ok(),
            Err(err) => {
                eprintln!("mock-ai: {err}");
                false
            }
        },
        "ci" => {
            cargo(&["fmt", "--all", "--check"])
                && cargo(&[
                    "clippy",
                    "--workspace",
                    "--all-targets",
                    "--",
                    "-D",
                    "warnings",
                ])
                && cargo(&["test", "--workspace"])
                && layers()
                && check_wasm()
        }
        _ => {
            eprintln!(
                "usage: cargo xtask <layers|wasm|ci|dep-src|build-player|package-example|mock-lrs|mock-ai>"
            );
            false
        }
    };
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
