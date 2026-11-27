//! Regression tests for file-handling bugs found in the 0.6.1 Neon review
//! (N48 web folders beside a bare `course.json`, N55 folder saves).

use std::path::{Path, PathBuf};

use coursecraft_control::{ControlHost, HeadlessHost};
use coursecraft_course::Course;

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/control-io-fixes")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

const WEB_COURSE: &str = r#"{
  "schema_version": 1,
  "id": "web-course",
  "title": "Web course",
  "scenes": [{
    "id": "sc1",
    "title": "Scene 1",
    "slides": [{
      "id": "s1",
      "title": "Slide 1",
      "duration_ms": 5000,
      "objects": [{
        "id": "site",
        "kind": { "type": "web", "source": { "type": "folder", "path": "web/site" } },
        "frame": { "x": 10, "y": 10, "w": 400, "h": 300 }
      }]
    }]
  }]
}"#;

const IMAGE_COURSE: &str = r#"{
  "schema_version": 1,
  "id": "img-course",
  "title": "Image course",
  "scenes": [{
    "id": "sc1",
    "title": "Scene 1",
    "slides": [{
      "id": "s1",
      "title": "Slide 1",
      "duration_ms": 5000,
      "objects": [{
        "id": "img1",
        "kind": { "type": "image", "src": "pic.png" },
        "frame": { "x": 10, "y": 10, "w": 100, "h": 100 }
      }]
    }]
  }]
}"#;

#[test]
fn bare_course_json_keeps_its_web_folder_when_saved_as_ccraft() {
    let dir = scratch("web-folder");
    std::fs::write(dir.join("course.json"), WEB_COURSE).expect("course");
    let site = dir.join("media/web/site");
    std::fs::create_dir_all(site.join("js")).expect("site");
    std::fs::write(site.join("index.html"), b"<html></html>").expect("index");
    std::fs::write(site.join("js/app.js"), b"// js").expect("js");

    let mut host = HeadlessHost::open_path(&dir.join("course.json")).expect("open");
    let out = dir.join("out.ccraft");
    host.save(Some(&out)).expect("save");

    let project = coursecraft_format::open_project_from(std::fs::File::open(&out).expect("file"))
        .expect("ccraft opens");
    let names: Vec<&str> = project.media.iter().map(|(n, _)| n.as_str()).collect();
    assert!(names.contains(&"web/site/index.html"), "{names:?}");
    assert!(names.contains(&"web/site/js/app.js"), "{names:?}");
}

#[test]
fn folder_save_deletes_media_the_session_dropped_and_skips_unchanged_files() {
    let dir = scratch("folder-record");
    let folder = dir.join("project");
    let course = Course::from_json(IMAGE_COURSE).expect("course");
    let mut record = coursecraft_format::FolderRecord::new();
    coursecraft_format::save_project_folder(
        &folder,
        &course,
        &[("pic.png".to_owned(), b"png".to_vec())],
        &mut record,
    )
    .expect("seed");

    let mut host = HeadlessHost::open_path(&folder).expect("open folder");
    // Something else puts an extra file in the project, then the session drops it.
    std::fs::write(folder.join("media/extra.bin"), b"extra").expect("extra");
    host.media_source.set_in_memory(&[
        ("pic.png".to_owned(), b"png".to_vec()),
        ("extra.bin".to_owned(), b"extra".to_vec()),
    ]);
    host.save(None).expect("first save");
    assert!(folder.join("media/extra.bin").is_file());

    host.media_source
        .set_in_memory(&[("pic.png".to_owned(), b"png".to_vec())]);
    host.save(None).expect("second save");
    assert!(
        !folder.join("media/extra.bin").exists(),
        "media the session no longer holds must be deleted from the folder"
    );
    assert!(folder.join("media/pic.png").is_file());
}

#[test]
fn media_symlink_that_leaves_the_media_folder_is_not_served() {
    use coursecraft_render::{MediaFetch, MediaSource};
    let dir = scratch("media-link");
    std::fs::write(dir.join("course.json"), IMAGE_COURSE).expect("course");
    std::fs::create_dir_all(dir.join("media")).expect("media");
    let secret = dir.join("secret.txt");
    std::fs::write(&secret, b"secret").expect("secret");
    let link = dir.join("media/pic.png");
    #[cfg(unix)]
    let made = std::os::unix::fs::symlink(&secret, &link).is_ok();
    #[cfg(windows)]
    let made = std::os::windows::fs::symlink_file(&secret, &link).is_ok();
    #[cfg(not(any(unix, windows)))]
    let made = false;
    if !made {
        eprintln!("skipped: this OS refused to create a file symlink");
        return;
    }
    let host = HeadlessHost::open_path(&dir.join("course.json")).expect("open");
    assert!(matches!(
        host.media_source.get("pic.png"),
        MediaFetch::Missing
    ));
}
