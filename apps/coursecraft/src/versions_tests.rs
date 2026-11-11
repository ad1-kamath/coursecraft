//! Tests for version history: when a version is made, pruning, damaged
//! versions and Open as Copy. Every test uses a scratch config folder, never
//! the real application-support one.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::files::{open_from, save_to};
use crate::prefs::{Prefs, load_prefs, save_prefs};
use crate::versions::{self, snapshot_with};
use coursecraft_course::Course;

const HELLO_JSON: &str = include_str!("../../../examples/hello-course/course.json");

fn scratch_dir(sub: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/agy-scratch")
        .join(sub);
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn titled(title: &str) -> Course {
    let mut course = Course::from_json(HELLO_JSON).unwrap();
    course.title = title.to_string();
    course
}

fn at(secs: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000 + secs)
}

fn app_in(config: &Path, project: &Path) -> crate::EditorApp {
    let mut app = crate::EditorApp::new_desktop(titled("Versions"), Some(config.to_path_buf()));
    app.project.path = Some(project.to_path_buf());
    app
}

#[test]
fn fnv1a_matches_the_published_test_vectors_and_the_hash_is_stable() {
    assert_eq!(versions::fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
    assert_eq!(versions::fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
    assert_eq!(versions::fnv1a64(b"foobar"), 0x8594_4171_f739_67e8);
    let hash = versions::project_hash(Path::new("/projects/course.ccraft"));
    assert_eq!(hash.len(), 16);
    assert_eq!(
        hash,
        versions::project_hash(Path::new("/projects/course.ccraft"))
    );
    assert_ne!(
        hash,
        versions::project_hash(Path::new("/projects/other.ccraft"))
    );
}

#[test]
fn timestamps_are_utc_and_readable() {
    assert_eq!(
        versions::timestamp_id(SystemTime::UNIX_EPOCH),
        "19700101-000000"
    );
    let leap_day = SystemTime::UNIX_EPOCH + Duration::from_secs(951_782_400);
    assert_eq!(versions::timestamp_id(leap_day), "20000229-000000");
    assert_eq!(
        versions::display_when("20261111-153045"),
        "2026-11-11 15:30:45 UTC"
    );
    assert_eq!(versions::display_when("odd"), "odd");
    assert!(versions::id_is_valid("20261111-153045-2"));
    assert!(!versions::id_is_valid("../x"));
    assert!(!versions::id_is_valid(""));
}

#[test]
fn a_version_is_made_on_the_second_save_and_not_the_first() {
    let dir = scratch_dir("versions-second-save");
    let cfg = dir.join("cfg");
    let project = dir.join("p.ccraft");
    let mut app = app_in(&cfg, &project);

    assert!(app.perform_save());
    assert!(
        versions::list_versions(&cfg, &project).is_empty(),
        "no file to copy yet"
    );

    app.editor.session.course.title = "Second".into();
    assert!(app.perform_save());
    let list = versions::list_versions(&cfg, &project);
    assert_eq!(list.len(), 1);
    assert!(list[0].slides.is_some());
    // The version is the file that was replaced: the first save's title.
    assert_eq!(open_from(&list[0].path).unwrap().0.title, "Versions");
    // The index names the project, and nothing was written next to the project.
    let index =
        std::fs::read_to_string(versions::versions_dir(&cfg, &project).join("index.json")).unwrap();
    assert!(index.contains("p.ccraft") && index.contains("Second"));
    assert!(!dir.join("versions").exists());
}

#[test]
fn identical_bytes_make_no_version() {
    let dir = scratch_dir("versions-identical");
    let cfg = dir.join("cfg");
    let project = dir.join("p.ccraft");
    save_to(&project, &titled("A"), &[]).unwrap();
    assert!(
        snapshot_with(&cfg, &project, 10, "A", versions::CAP_BYTES, at(0))
            .unwrap()
            .is_some()
    );
    assert!(
        snapshot_with(&cfg, &project, 10, "A", versions::CAP_BYTES, at(5))
            .unwrap()
            .is_none()
    );
    assert_eq!(versions::list_versions(&cfg, &project).len(), 1);

    save_to(&project, &titled("B"), &[]).unwrap();
    assert!(
        snapshot_with(&cfg, &project, 10, "B", versions::CAP_BYTES, at(9))
            .unwrap()
            .is_some()
    );
    assert_eq!(versions::list_versions(&cfg, &project).len(), 2);
}

#[test]
fn only_the_newest_n_versions_are_kept_and_zero_keeps_none() {
    let dir = scratch_dir("versions-prune");
    let cfg = dir.join("cfg");
    let project = dir.join("p.ccraft");
    for n in 0..6u64 {
        save_to(&project, &titled(&format!("v{n}")), &[]).unwrap();
        snapshot_with(&cfg, &project, 3, "t", versions::CAP_BYTES, at(n * 10)).unwrap();
    }
    let list = versions::list_versions(&cfg, &project);
    assert_eq!(list.len(), 3);
    let titles: Vec<String> = list
        .iter()
        .map(|v| open_from(&v.path).unwrap().0.title)
        .collect();
    assert_eq!(titles, ["v5", "v4", "v3"], "newest first");

    // Two in the same second do not overwrite each other.
    save_to(&project, &titled("same-second"), &[]).unwrap();
    snapshot_with(&cfg, &project, 50, "t", versions::CAP_BYTES, at(100)).unwrap();
    save_to(&project, &titled("same-second-2"), &[]).unwrap();
    snapshot_with(&cfg, &project, 50, "t", versions::CAP_BYTES, at(100)).unwrap();
    assert_eq!(versions::list_versions(&cfg, &project).len(), 5);

    let off = scratch_dir("versions-off");
    let project = off.join("p.ccraft");
    save_to(&project, &titled("x"), &[]).unwrap();
    assert!(
        snapshot_with(
            &off.join("cfg"),
            &project,
            0,
            "t",
            versions::CAP_BYTES,
            at(0)
        )
        .unwrap()
        .is_none()
    );
    assert!(versions::list_versions(&off.join("cfg"), &project).is_empty());
}

#[test]
fn the_size_cap_drops_the_oldest_but_never_the_newest() {
    let dir = scratch_dir("versions-cap");
    let cfg = dir.join("cfg");
    let project = dir.join("p.ccraft");
    let media = |n: u8| vec![("a.png".to_string(), vec![n; 20_000])];
    let mut size = 0;
    for n in 0..4u8 {
        save_to(&project, &titled("c"), &media(n)).unwrap();
        size = std::fs::metadata(&project).unwrap().len();
        snapshot_with(
            &cfg,
            &project,
            50,
            "c",
            2 * size + 100,
            at(u64::from(n) * 10),
        )
        .unwrap();
    }
    let list = versions::list_versions(&cfg, &project);
    assert_eq!(list.len(), 2, "a cap of two files keeps two");
    assert!(list.iter().map(|v| v.size).sum::<u64>() <= 2 * size + 100);

    // A cap below one file still keeps the newest.
    save_to(&project, &titled("c"), &media(9)).unwrap();
    snapshot_with(&cfg, &project, 50, "c", 10, at(99)).unwrap();
    assert_eq!(versions::list_versions(&cfg, &project).len(), 1);
}

#[test]
fn a_failed_copy_does_not_fail_the_save() {
    let dir = scratch_dir("versions-copy-fails");
    let cfg = dir.join("cfg");
    std::fs::create_dir_all(&cfg).unwrap();
    // `versions` is a plain file, so no version folder can be made.
    std::fs::write(cfg.join("versions"), b"in the way").unwrap();
    let project = dir.join("p.ccraft");
    let mut app = app_in(&cfg, &project);
    assert!(app.perform_save());
    app.editor.session.course.title = "Changed".into();
    assert!(app.perform_save(), "the save still succeeds");
    assert_eq!(open_from(&project).unwrap().0.title, "Changed");
    assert!(
        app.editor
            .ui
            .status_message
            .as_ref()
            .is_some_and(|(m, _)| m == "Couldn't keep a version of this file")
    );
}

#[test]
fn open_as_copy_leaves_the_original_alone_and_has_no_path() {
    let dir = scratch_dir("versions-open-copy");
    let cfg = dir.join("cfg");
    let project = dir.join("p.ccraft");
    let mut app = app_in(&cfg, &project);
    assert!(app.perform_save());
    app.editor.session.course.title = "Newer".into();
    assert!(app.perform_save());
    let original = std::fs::read(&project).unwrap();

    let version = versions::list_versions(&cfg, &project).remove(0);
    app.open_version_copy(&version.path);
    assert!(app.project.path.is_none(), "opens untitled-style");
    assert!(app.project.is_dirty(&app.editor.session), "needs a Save As");
    let title = app.editor.session.course.title.clone();
    assert!(title.starts_with("Versions (restored "), "{title}");
    assert!(title.ends_with("UTC)"), "{title}");
    assert_eq!(
        std::fs::read(&project).unwrap(),
        original,
        "original untouched"
    );
}

#[test]
fn a_damaged_version_is_listed_as_damaged_and_never_offered() {
    let dir = scratch_dir("versions-damaged");
    let cfg = dir.join("cfg");
    let project = dir.join("p.ccraft");
    save_to(&project, &titled("good"), &[]).unwrap();
    snapshot_with(&cfg, &project, 10, "good", versions::CAP_BYTES, at(0)).unwrap();
    let vdir = versions::versions_dir(&cfg, &project);
    std::fs::write(vdir.join("29990101-000000.ccraft"), b"not a zip at all").unwrap();

    let list = versions::list_versions(&cfg, &project);
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].id, "29990101-000000");
    assert_eq!(list[0].slides, None, "damaged");
    assert!(list[1].slides.is_some());
    let latest = versions::latest_readable(&cfg, &project).unwrap();
    assert_eq!(latest.id, list[1].id, "skips the damaged newest one");

    // A damaged version can still be deleted; a bad id cannot reach outside.
    versions::delete_version(&cfg, &project, "29990101-000000").unwrap();
    assert_eq!(versions::list_versions(&cfg, &project).len(), 1);
    assert!(versions::delete_version(&cfg, &project, "../../x").is_err());
}

#[test]
fn a_damaged_project_offers_its_latest_version() {
    let dir = scratch_dir("versions-offer");
    let cfg = dir.join("cfg");
    let project = dir.join("p.ccraft");
    let mut app = app_in(&cfg, &project);
    assert!(app.perform_save());
    app.editor.session.course.title = "Two".into();
    assert!(app.perform_save());

    std::fs::write(&project, b"damaged beyond repair").unwrap();
    app.finish_open(&project, Some(project.clone()), false);
    assert!(
        app.error_modal
            .as_ref()
            .is_some_and(|m| m.contains("can't be opened"))
    );
    let offer = app.open_latest_offer.clone().expect("a version is offered");
    assert_eq!(open_from(&offer).unwrap().0.title, "Versions");

    // A damaged file with no versions offers nothing.
    let other = dir.join("lonely.ccraft");
    std::fs::write(&other, b"junk").unwrap();
    app.finish_open(&other, Some(other.clone()), false);
    assert!(app.open_latest_offer.is_none());
}

#[test]
fn versions_to_keep_defaults_round_trips_and_clamps() {
    let dir = scratch_dir("versions-prefs");
    assert_eq!(Prefs::default().versions_to_keep, 10);
    save_prefs(
        &dir,
        &Prefs {
            versions_to_keep: 25,
            ..Prefs::default()
        },
    )
    .unwrap();
    assert_eq!(load_prefs(&dir).versions_to_keep, 25);
    std::fs::write(dir.join("prefs.json"), "{\"recent\": []}").unwrap();
    assert_eq!(
        load_prefs(&dir).versions_to_keep,
        10,
        "old files get the default"
    );
    std::fs::write(dir.join("prefs.json"), "{\"versions_to_keep\": 9999}").unwrap();
    assert_eq!(load_prefs(&dir).versions_to_keep_clamped(), 100);
}
