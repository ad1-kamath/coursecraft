//! Embed the compiled web player from `target/player-dist` into desktop builds.
//!
//! A build made without `cargo xtask build-player` still compiles: the generated
//! list is empty, and the Publish dialog explains what is missing.

use std::env;
use std::fmt::Write as _;
use std::path::{Component, Path, PathBuf};

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap_or_default());
    let Some(root) = manifest
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
    else {
        println!("cargo:warning=player bundle: cannot find the workspace root");
        write_bundle(&[]);
        return;
    };

    let dist = root.join("target/player-dist");
    let index = dist.join("index.html");
    // Watch the folder so the embedded list appears when `build-player` runs after
    // an earlier plain `cargo build`. It must exist: cargo treats a missing watched
    // path as always changed, which would rebuild the editor on every build.
    if let Err(err) = std::fs::create_dir_all(&dist) {
        println!("cargo:warning=player bundle: {}: {err}", dist.display());
    }
    println!("cargo:rerun-if-changed={}", dist.display());
    println!("cargo:rerun-if-env-changed=CARGO_CFG_TARGET_ARCH");

    let arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let mut files = Vec::new();
    if arch != "wasm32" && index.is_file() {
        match collect_files(&dist, &dist) {
            Ok(mut found) => {
                found.sort_by(|a, b| a.0.cmp(&b.0));
                files = found;
            }
            Err(err) => {
                println!("cargo:warning=player bundle: {err}");
                files.clear();
            }
        }
    }
    write_bundle(&files);
}

fn collect_files(base: &Path, current: &Path) -> Result<Vec<(String, PathBuf)>, String> {
    let entries = std::fs::read_dir(current).map_err(|e| format!("{}: {e}", current.display()))?;
    let mut files = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| format!("{}: {e}", current.display()))?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || name == "course.json" {
            continue;
        }
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if file_type.is_dir() {
            files.extend(collect_files(base, &path)?);
        } else if file_type.is_file() {
            let relative = path
                .strip_prefix(base)
                .map_err(|e| format!("{}: {e}", path.display()))?;
            let mut parts = Vec::new();
            for component in relative.components() {
                if let Component::Normal(part) = component {
                    let part = part
                        .to_str()
                        .ok_or_else(|| format!("non-UTF-8 path: {}", path.display()))?;
                    parts.push(part);
                }
            }
            files.push((parts.join("/"), path));
        }
    }
    Ok(files)
}

fn write_bundle(files: &[(String, PathBuf)]) {
    let mut source = String::new();
    source.push_str("pub static PLAYER_FILES: &[(&str, &[u8])] = &[\n");
    for (relative, path) in files {
        // Paths are quoted and escaped, so spaces and quotes in the workspace
        // path survive as valid Rust and as valid `include_bytes!` input.
        let _ = writeln!(source, "    ({relative:?}, include_bytes!({path:?})),");
    }
    source.push_str("];\n");

    let Some(out_dir) = env::var_os("OUT_DIR") else {
        println!("cargo:warning=player bundle: OUT_DIR is not set");
        return;
    };
    let target = PathBuf::from(out_dir).join("player_bundle.rs");
    if let Err(err) = std::fs::write(&target, source) {
        println!(
            "cargo:warning=player bundle: cannot write {}: {err}",
            target.display()
        );
    }
}
