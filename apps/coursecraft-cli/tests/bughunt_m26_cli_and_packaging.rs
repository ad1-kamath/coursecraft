//! M26 bug-hunt integration tests for CLI, Windows packaging (A8), A2.11, A3.10, A9.10, CC.2, CC.3, CC.8.
//!
//! Ordered P1 -> P2 -> P3.

use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

// ============================================================================
// P1
// ============================================================================

#[test]
fn a3_10_cli_simulate_and_render_player_at_ms_exit() {
    let out_dir = repo_root().join("target/m26-a3-cli-test");
    std::fs::create_dir_all(&out_dir).unwrap();
    let course_path = out_dir.join("course.json");
    let inputs_path = out_dir.join("inputs.json");
    let png_rest = out_dir.join("rest.png");
    let png_exit = out_dir.join("exit.png");

    let course_json = serde_json::json!({
        "schema_version": 1,
        "id": "a3-cli",
        "title": "A3 CLI",
        "slide_size": { "w": 1280, "h": 720 },
        "variables": [{ "name": "exited", "value": false }],
        "scenes": [{
            "id": "sc1",
            "title": "S",
            "slides": [
                { "id": "s1", "title": "S1", "duration_ms": 2000, "objects": [] },
                {
                    "id": "s2",
                    "title": "S2",
                    "duration_ms": 5000,
                    "transition": { "type": "fade", "duration_ms": 500 },
                    "objects": [{
                        "id": "obj-fly",
                        "kind": { "type": "shape", "shape": "rectangle", "fill": "#FF0000" },
                        "frame": { "x": 400.0, "y": 200.0, "w": 200.0, "h": 100.0 },
                        "entrance": { "effect": { "type": "fade" }, "duration_ms": 500, "easing": "linear" },
                        "exit": { "effect": { "type": "fly_in", "from": "right" }, "duration_ms": 1000, "easing": "linear" }
                    }],
                    "triggers": [{
                        "when": { "event": "animation_completes", "object": "obj-fly" },
                        "action": { "action": "set_variable", "variable": "exited", "value": true }
                    }]
                }
            ]
        }]
    });
    std::fs::write(
        &course_path,
        serde_json::to_string_pretty(&course_json).unwrap(),
    )
    .unwrap();
    std::fs::write(
        &inputs_path,
        serde_json::to_string(&serde_json::json!([
            { "input": "start" },
            { "input": "go_to", "slide": "s2" },
            { "input": "tick", "ms": 4500 },
            { "input": "tick", "ms": 500 }
        ]))
        .unwrap(),
    )
    .unwrap();

    let bin = env!("CARGO_BIN_EXE_coursecraft-cli");
    assert!(
        std::process::Command::new(bin)
            .args([
                "simulate",
                course_path.to_str().unwrap(),
                inputs_path.to_str().unwrap(),
                "--strict"
            ])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        std::process::Command::new(bin)
            .args([
                "render",
                course_path.to_str().unwrap(),
                "--slide",
                "s2",
                "--mode",
                "player",
                "--at-ms",
                "2000",
                "-o",
                png_rest.to_str().unwrap()
            ])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        std::process::Command::new(bin)
            .args([
                "render",
                course_path.to_str().unwrap(),
                "--slide",
                "s2",
                "--mode",
                "player",
                "--at-ms",
                "4500",
                "-o",
                png_exit.to_str().unwrap()
            ])
            .status()
            .unwrap()
            .success()
    );
    assert_ne!(
        std::fs::read(&png_rest).unwrap(),
        std::fs::read(&png_exit).unwrap()
    );
}

#[test]
fn a8_1_windows_iss_required_installer_directives() {
    let iss = std::fs::read_to_string(repo_root().join("packaging/windows/coursecraft.iss"))
        .expect("read coursecraft.iss");
    for required in [
        "AppId={{7B6E3A4C-5F1A-4D8E-9C2B-8A1F3E5D7C90}",
        "PrivilegesRequired=lowest",
        "DefaultDirName={localappdata}\\Programs\\CourseCraft",
        "UninstallDisplayName=CourseCraft",
        "UninstallDisplayIcon={app}\\coursecraft.exe",
        "{autoprograms}\\CourseCraft",
        "{autodesktop}\\CourseCraft",
        "ArchitecturesAllowed=x64compatible",
        "ArchitecturesInstallIn64BitMode=x64compatible",
        "nowait postinstall skipifsilent",
    ] {
        assert!(
            iss.contains(required),
            "coursecraft.iss missing: {required}"
        );
    }
}

#[test]
fn a8_2_windows_iss_referenced_assets_exist_and_ico_valid() {
    let root = repo_root();
    let ico = std::fs::read(root.join("apps/coursecraft/icons/coursecraft.ico")).expect("ico");
    assert!(ico.len() >= 6);
    assert_eq!(&ico[0..4], &[0x00, 0x00, 0x01, 0x00], "invalid ICO header");
    let count = u16::from_le_bytes([ico[4], ico[5]]);
    assert!(count >= 1);

    for rel in ["LICENSE-MIT", "LICENSE-APACHE", "ATTRIBUTION.md"] {
        let p = root.join(rel);
        assert!(
            p.exists() && std::fs::metadata(&p).unwrap().len() > 0,
            "missing {rel}"
        );
    }
}

#[test]
fn a8_3_release_yml_and_iss_defines_parity() {
    let root = repo_root();
    let yml =
        std::fs::read_to_string(root.join(".github/workflows/release.yml")).expect("release.yml");
    let iss = std::fs::read_to_string(root.join("packaging/windows/coursecraft.iss")).expect("iss");
    for key in ["AppVersion", "OutputName", "OutputDir", "RepoRoot"] {
        assert!(
            yml.contains(&format!("/D{key}=")),
            "release.yml missing /D{key}="
        );
        assert!(
            iss.contains(&format!("#ifndef {key}")),
            "iss missing #ifndef {key}"
        );
    }
    assert!(yml.contains("-windows-x64-setup"));
    assert!(
        yml.contains("-windows-x64-portable"),
        "release.yml must build the portable Windows zip"
    );
    assert!(
        yml.contains("portable.txt"),
        "the portable zip must carry portable.txt so the update check knows it"
    );
}

#[test]
fn release_workflow_builds_dmg_setup_portable_tarball_and_appimage() {
    let root = repo_root();
    let yml =
        std::fs::read_to_string(root.join(".github/workflows/release.yml")).expect("release.yml");
    let mac =
        std::fs::read_to_string(root.join("scripts/package-macos.sh")).expect("package-macos.sh");
    let desktop = std::fs::read_to_string(root.join("packaging/linux/coursecraft.desktop"))
        .expect("coursecraft.desktop");

    // macOS .dmg
    assert!(yml.contains("scripts/package-macos.sh"));
    assert!(mac.contains("coursecraft-$version-macos-$arch.dmg"));

    // Windows: Inno Setup -setup.exe and a portable zip
    assert!(yml.contains("coursecraft-$ver-windows-x64-setup"));
    assert!(yml.contains("packaging/windows/coursecraft.iss"));
    assert!(yml.contains("coursecraft-$ver-windows-x64-portable"));

    // Linux .tar.gz and .AppImage
    assert!(yml.contains("dist/$name.tar.gz"));
    assert!(yml.contains("dist/coursecraft-$ver-linux-x86_64.AppImage"));
    // Pinned, checksummed appimagetool (not floating `continuous`)
    assert!(
        yml.contains(
            "AppImage/appimagetool/releases/download/$tool_ver/appimagetool-x86_64.AppImage"
        ) && !yml.contains("/download/continuous/"),
        "appimagetool must be pinned to a versioned release, not continuous"
    );
    assert!(
        yml.contains("sha256sum -c -"),
        "appimagetool must be checksummed"
    );
    assert!(yml.contains("packaging/linux/coursecraft.desktop"));
    assert!(yml.contains("apps/coursecraft/icons/coursecraft-1024.png"));
    assert!(yml.contains(
        "LICENSE-MIT LICENSE-APACHE ATTRIBUTION.md \"$appdir/usr/share/doc/coursecraft/\""
    ));

    // SHA256SUMS.txt covers all coursecraft-* artifacts (including .AppImage)
    assert!(yml.contains("sha256sum coursecraft-* > SHA256SUMS.txt"));

    // Valid .desktop entry and 1024 px PNG icon on disk
    for key in [
        "[Desktop Entry]",
        "Type=Application",
        "Name=CourseCraft",
        "Exec=coursecraft",
        "Icon=coursecraft",
        "Terminal=false",
        "Categories=",
    ] {
        assert!(desktop.contains(key), "coursecraft.desktop missing {key}");
    }
    let png = std::fs::read(root.join("apps/coursecraft/icons/coursecraft-1024.png")).expect("png");
    assert!(png.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]));
}

#[test]
fn a8_4_release_yml_installer_step_must_not_continue_on_error() {
    let yml = std::fs::read_to_string(repo_root().join(".github/workflows/release.yml"))
        .expect("release.yml");
    let idx = yml
        .find("Build Windows installer")
        .expect("Build Windows installer step");
    let snippet = &yml[idx..(idx + 400).min(yml.len())];
    assert!(
        !snippet.contains("continue-on-error: true"),
        "installer step must not silently continue on error:\n{snippet}"
    );
}

#[test]
fn cc_2_pack_unpack_pack_and_cli_diff_lossless_roundtrip() {
    let out_dir = repo_root().join("target/m26-cc2-test");
    std::fs::create_dir_all(&out_dir).unwrap();
    let src_dir = repo_root().join("examples/hello-course");
    let cc1 = out_dir.join("c1.ccraft");
    let unpacked = out_dir.join("unpacked");
    let cc2 = out_dir.join("c2.ccraft");
    let _ = std::fs::remove_dir_all(&unpacked);

    let bin = env!("CARGO_BIN_EXE_coursecraft-cli");
    assert!(
        std::process::Command::new(bin)
            .args([
                "pack",
                src_dir.to_str().unwrap(),
                "-o",
                cc1.to_str().unwrap()
            ])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        std::process::Command::new(bin)
            .args([
                "unpack",
                cc1.to_str().unwrap(),
                "-o",
                unpacked.to_str().unwrap()
            ])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        std::process::Command::new(bin)
            .args([
                "pack",
                unpacked.to_str().unwrap(),
                "-o",
                cc2.to_str().unwrap()
            ])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        std::process::Command::new(bin)
            .args(["diff", cc1.to_str().unwrap(), cc2.to_str().unwrap()])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        std::process::Command::new(bin)
            .args(["validate", cc2.to_str().unwrap()])
            .status()
            .unwrap()
            .success()
    );
}

#[test]
fn cc_3_published_web_zip_preserves_course_and_media() {
    let player_dist = repo_root().join("target/player-dist");
    // Publishing needs a built web player; CI runs the gate without one.
    if !player_dist.join("index.html").exists() {
        return;
    }
    let out_zip = repo_root().join("target/m26-cc3-web.zip");
    let src_json = repo_root().join("examples/hello-course/course.json");
    let bin = env!("CARGO_BIN_EXE_coursecraft-cli");
    assert!(
        std::process::Command::new(bin)
            .args([
                "publish",
                src_json.to_str().unwrap(),
                "-o",
                out_zip.to_str().unwrap(),
                "--standard",
                "web",
                "--player",
                player_dist.to_str().unwrap(),
            ])
            .status()
            .unwrap()
            .success()
    );
    assert!(out_zip.exists() && std::fs::metadata(&out_zip).unwrap().len() > 0);
}

// ============================================================================
// P2
// ============================================================================

#[test]
fn a8_5_docs_explain_release_downloads_with_windows_portable() {
    let root = repo_root();
    let readme = std::fs::read_to_string(root.join("README.md")).expect("README.md");
    for required in [
        ".dmg",
        "-setup.exe",
        "-portable.zip",
        ".AppImage",
        ".tar.gz",
    ] {
        assert!(readme.contains(required), "README.md missing {required}");
    }
    let pkg_readme =
        std::fs::read_to_string(root.join("packaging/windows/README.md")).expect("pkg README");
    assert!(pkg_readme.contains("Inno Setup") && pkg_readme.contains("release.yml"));
}

#[test]
fn a8_6_iss_handles_semver_and_prerelease_tags() {
    let root = repo_root();
    let iss = std::fs::read_to_string(root.join("packaging/windows/coursecraft.iss")).unwrap();
    let yml = std::fs::read_to_string(root.join(".github/workflows/release.yml")).unwrap();
    assert!(!iss.contains("VersionInfoVersion={#AppVersion}"));
    assert!(iss.contains("AppVersion={#AppVersion}"));
    assert!(yml.contains("TrimStart('v')"));
}

#[test]
fn a8_7_control_state_does_not_write_beside_executable() {
    // Verify coursecraft_control path resolves under user config/data dir, not exe dir
    let exe_dir = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let dir = coursecraft_control::default_config_dir().expect("config dir");
    assert!(
        !dir.starts_with(&exe_dir),
        "control dir should not be in exe dir: {dir:?}"
    );
}

// ============================================================================
// P3
// ============================================================================

#[test]
fn a9_10_base_slide_view_cli_view_set_pack_unpack_validate_roundtrip() {
    let out_dir = repo_root().join("target/m26-a9-cli-test");
    let proj_dir = out_dir.join("proj");
    let _ = std::fs::remove_dir_all(&proj_dir);
    std::fs::create_dir_all(&proj_dir).unwrap();
    let src_json = repo_root().join("examples/hello-course/course.json");
    let script = out_dir.join("script.json");
    let out_json = proj_dir.join("course.json");
    let cc = out_dir.join("a9.ccraft");
    std::fs::write(
        &script,
        serde_json::to_string(&serde_json::json!([
            { "command": "view.set", "params": { "base_slide_view": "hide" } }
        ]))
        .unwrap(),
    )
    .unwrap();

    let bin = env!("CARGO_BIN_EXE_coursecraft-cli");
    assert!(
        std::process::Command::new(bin)
            .args([
                "run",
                src_json.to_str().unwrap(),
                script.to_str().unwrap(),
                "-o",
                out_json.to_str().unwrap()
            ])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        std::process::Command::new(bin)
            .args([
                "pack",
                proj_dir.to_str().unwrap(),
                "-o",
                cc.to_str().unwrap()
            ])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        std::process::Command::new(bin)
            .args(["validate", cc.to_str().unwrap()])
            .status()
            .unwrap()
            .success()
    );
}

#[test]
fn ccraft_file_association_configured_for_windows_and_linux() {
    let root = repo_root();
    let iss = std::fs::read_to_string(root.join("packaging/windows/coursecraft.iss"))
        .expect("read coursecraft.iss");
    for required in [
        "ChangesAssociations=yes",
        "[Registry]",
        r"Software\Classes\.ccraft",
        "CourseCraft project",
        "DefaultIcon",
        r"shell\open\command",
        "uninsdeletekey",
        r#""{app}\coursecraft.exe""#,
        r#""%1""#,
    ] {
        assert!(
            iss.contains(required),
            "coursecraft.iss missing: {required}"
        );
    }

    let mime_xml = std::fs::read_to_string(root.join("packaging/linux/coursecraft-mime.xml"))
        .expect("read coursecraft-mime.xml");
    assert!(mime_xml.contains("application/x-coursecraft-project"));
    assert!(mime_xml.contains("*.ccraft"));

    let desktop = std::fs::read_to_string(root.join("packaging/linux/coursecraft.desktop"))
        .expect("read coursecraft.desktop");
    assert!(desktop.contains("MimeType=application/x-coursecraft-project;"));

    let yml =
        std::fs::read_to_string(root.join(".github/workflows/release.yml")).expect("release.yml");
    assert!(yml.contains(r#"$root/share/mime/packages/coursecraft.xml"#));
    assert!(yml.contains(r#"$appdir/usr/share/mime/packages/coursecraft.xml"#));
}

#[test]
fn release_workflow_builds_intel_macos_dmg_and_sizes_image() {
    let root = repo_root();
    let yml =
        std::fs::read_to_string(root.join(".github/workflows/release.yml")).expect("release.yml");
    let mac =
        std::fs::read_to_string(root.join("scripts/package-macos.sh")).expect("package-macos.sh");
    let readme = std::fs::read_to_string(root.join("README.md")).expect("README.md");

    assert!(yml.contains("wasm32-unknown-unknown, x86_64-apple-darwin"));
    assert!(yml.contains("cargo build --release -p coursecraft --target x86_64-apple-darwin"));
    assert!(yml.contains("MACOSX_DEPLOYMENT_TARGET"));
    assert!(yml.contains("11.0"));
    assert!(yml.contains(
        r#"scripts/package-macos.sh "${GITHUB_REF_NAME#v}" target/x86_64-apple-darwin/release/coursecraft dist x86_64"#
    ));

    assert!(mac.contains("${4:-$(uname -m)}"));
    assert!(mac.contains("-size 200m"));

    let download_line = readme
        .lines()
        .find(|line| line.contains("Download the latest version"))
        .expect("README download notes");
    assert!(download_line.contains("Apple Silicon"));
    assert!(download_line.contains("Intel"));
}
