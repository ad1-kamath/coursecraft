#![cfg(not(target_arch = "wasm32"))]

use std::path::{Path, PathBuf};

use coursecraft_course::Course;

use super::*;

const HELLO: &str = include_str!("../../../examples/hello-course/course.json");

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/format-folder-tests")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn hello() -> Course {
    Course::from_json(HELLO).expect("hello")
}

fn media() -> Vec<(String, Vec<u8>)> {
    vec![
        (
            "images/badge.png".to_owned(),
            vec![0x89, b'P', b'N', b'G', 1],
        ),
        ("audio/chime.wav".to_owned(), b"RIFF-chime".to_vec()),
        ("fonts/lato.ttf".to_owned(), b"font".to_vec()),
    ]
}

fn sorted(mut items: Vec<(String, Vec<u8>)>) -> Vec<(String, Vec<u8>)> {
    items.sort();
    items
}

fn save(
    dir: &Path,
    course: &Course,
    media: &[(String, Vec<u8>)],
    record: &mut FolderRecord,
) -> SaveReport {
    save_project_folder(dir, course, media, record).expect("save folder")
}

#[test]
fn zip_to_folder_to_zip_round_trips() {
    let dir = scratch("round-trip").join("p.ccraft-folder");
    let zip = save_project(&hello(), &media()).expect("zip");
    let opened = open_project(&zip).expect("open zip");
    let mut record = FolderRecord::new();
    let report = save(&dir, &opened.course, &opened.media, &mut record);
    assert_eq!(report.written.len(), 4, "{report:?}");

    let folder = open_project_folder(&dir).expect("open folder");
    assert_eq!(folder.project.course, hello());
    assert_eq!(sorted(folder.project.media.clone()), sorted(media()));
    assert_eq!(folder.record, record);

    let again = save_project(&folder.project.course, &folder.project.media).expect("zip again");
    let reopened = open_project(&again).expect("reopen");
    assert_eq!(reopened.course, hello());
    assert_eq!(sorted(reopened.media), sorted(media()));
}

#[test]
fn layout_is_course_json_plus_media_and_nothing_else() {
    let dir = scratch("layout").join("p.ccraft-folder");
    save(&dir, &hello(), &media(), &mut FolderRecord::new());
    let mut names: Vec<String> = Vec::new();
    fn walk(dir: &Path, base: &Path, out: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).expect("read") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                walk(&path, base, out);
            } else {
                out.push(
                    path.strip_prefix(base)
                        .expect("prefix")
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
    walk(&dir, &dir, &mut names);
    names.sort();
    assert_eq!(
        names,
        vec![
            "course.json",
            "media/audio/chime.wav",
            "media/fonts/lato.ttf",
            "media/images/badge.png"
        ]
    );
    let text = std::fs::read_to_string(dir.join("course.json")).expect("read");
    assert!(text.ends_with("}\n") && !text.ends_with("\n\n"));
    assert!(!text.contains('\r'));
    assert!(
        text.contains("\n  \"schema_version\": 1"),
        "two-space indent"
    );
    assert!(is_project_folder(&dir));
}

#[test]
fn course_json_is_stable_between_saves() {
    let dir = scratch("stable").join("p.ccraft-folder");
    let mut course = hello();
    course.strings.insert("b".into(), "2".into());
    course.strings.insert("a".into(), "1".into());
    save(&dir, &course, &[], &mut FolderRecord::new());
    let first = std::fs::read(dir.join("course.json")).expect("read");
    let opened = open_project_folder(&dir).expect("open");
    let other = scratch("stable2").join("p.ccraft-folder");
    save(
        &other,
        &opened.project.course,
        &[],
        &mut FolderRecord::new(),
    );
    assert_eq!(
        first,
        std::fs::read(other.join("course.json")).expect("read")
    );
    let text = String::from_utf8(first).expect("utf8");
    assert!(text.find("\"a\": \"1\"") < text.find("\"b\": \"2\""));
}

#[test]
fn saving_twice_without_changes_writes_no_file() {
    let dir = scratch("twice").join("p.ccraft-folder");
    let mut record = FolderRecord::new();
    save(&dir, &hello(), &media(), &mut record);
    let stamp = |rel: &str| {
        std::fs::metadata(dir.join(rel))
            .expect("meta")
            .modified()
            .expect("mtime")
    };
    let before = (stamp("course.json"), stamp("media/audio/chime.wav"));
    std::thread::sleep(std::time::Duration::from_millis(30));
    let report = save(&dir, &hello(), &media(), &mut record);
    assert_eq!(report, SaveReport::default());
    assert_eq!(
        before,
        (stamp("course.json"), stamp("media/audio/chime.wav"))
    );

    // After a reopen the record comes from disk and gives the same answer.
    let mut reopened = open_project_folder(&dir).expect("open").record;
    assert_eq!(
        save(&dir, &hello(), &media(), &mut reopened),
        SaveReport::default()
    );
}

#[test]
fn only_changed_files_are_rewritten() {
    let dir = scratch("changed").join("p.ccraft-folder");
    let mut record = FolderRecord::new();
    save(&dir, &hello(), &media(), &mut record);
    let mut course = hello();
    course.title = "Another title".into();
    let mut items = media();
    items[1].1 = b"RIFF-new-chime".to_vec();
    let report = save(&dir, &course, &items, &mut record);
    assert_eq!(
        report.written,
        vec!["media/audio/chime.wav".to_owned(), "course.json".to_owned()]
    );
    assert!(report.deleted.is_empty());
}

#[test]
fn media_no_longer_used_is_removed_and_user_files_are_not() {
    let dir = scratch("unused").join("p.ccraft-folder");
    let mut record = FolderRecord::new();
    save(&dir, &hello(), &media(), &mut record);
    std::fs::create_dir_all(dir.join(".git")).expect("git");
    std::fs::write(dir.join(".git/HEAD"), "ref").expect("write");
    std::fs::write(dir.join(".gitattributes"), "media/** binary\n").expect("write");
    std::fs::write(dir.join("NOTES.md"), "mine").expect("write");
    std::fs::write(dir.join("media/mine.txt"), "added after the open").expect("write");

    let kept = vec![media().remove(0)];
    let report = save(&dir, &hello(), &kept, &mut record);
    assert_eq!(
        sorted(
            report
                .deleted
                .iter()
                .map(|d| (d.clone(), Vec::new()))
                .collect()
        ),
        vec![
            ("media/audio/chime.wav".to_owned(), Vec::new()),
            ("media/fonts/lato.ttf".to_owned(), Vec::new())
        ]
    );
    assert!(!dir.join("media/audio").exists(), "emptied folders go");
    assert!(dir.join("media/images/badge.png").is_file());
    assert!(dir.join(".git/HEAD").is_file());
    assert!(dir.join(".gitattributes").is_file());
    assert!(dir.join("NOTES.md").is_file());
    assert!(
        dir.join("media/mine.txt").is_file(),
        "a file the record never saw stays"
    );
    assert!(!record.files().contains_key("media/fonts/lato.ttf"));
}

#[test]
fn a_crash_before_course_json_keeps_the_old_course_and_all_its_media() {
    let dir = scratch("crash").join("p.ccraft-folder");
    let mut record = FolderRecord::new();
    save(&dir, &hello(), &media(), &mut record);
    let old = std::fs::read(dir.join("course.json")).expect("read");

    let mut course = hello();
    course.title = "New".into();
    let mut items = media();
    items.remove(0);
    items.push(("images/new.png".to_owned(), vec![9]));
    let fail = || Err(std::io::Error::other("power cut"));
    let hooks = SaveHooks {
        before_course_json: &fail,
        ..SaveHooks::default()
    };
    save_project_folder_with(&dir, &course, &items, &mut record, &hooks).expect_err("crash");
    assert_eq!(old, std::fs::read(dir.join("course.json")).expect("read"));
    for (path, bytes) in media() {
        assert_eq!(
            bytes,
            std::fs::read(dir.join("media").join(path)).expect("old media kept")
        );
    }
    let reopened = open_project_folder(&dir).expect("still opens");
    assert_eq!(reopened.project.course, hello());

    // A crash after course.json but before the deletes leaves extra files, never missing ones.
    let hooks = SaveHooks {
        before_delete: &fail,
        ..SaveHooks::default()
    };
    save_project_folder_with(&dir, &course, &items, &mut record, &hooks).expect_err("crash");
    let reopened = open_project_folder(&dir).expect("opens");
    assert_eq!(reopened.project.course.title, "New");
    assert!(dir.join("media/images/new.png").is_file());
}

#[test]
fn leftover_temp_files_are_ignored_on_open() {
    let dir = scratch("temp").join("p.ccraft-folder");
    save(&dir, &hello(), &media(), &mut FolderRecord::new());
    std::fs::write(dir.join("media/images/badge.png.ccraft-tmp"), "half").expect("write");
    let opened = open_project_folder(&dir).expect("open");
    assert_eq!(opened.project.media.len(), 3);
}

#[test]
fn unsafe_media_paths_are_refused_on_save() {
    let dir = scratch("unsafe-save").join("p.ccraft-folder");
    for bad in [
        "../x.png",
        "/abs.png",
        "a/../b.png",
        "c:/x.png",
        "a\\b.png",
        "x.ccraft-tmp",
    ] {
        let err = save_project_folder(
            &dir,
            &hello(),
            &[(bad.to_owned(), vec![1])],
            &mut FolderRecord::new(),
        )
        .expect_err(bad);
        assert_eq!(err, FormatError::UnsafePath(bad.to_owned()), "{bad}");
    }
    let dup = vec![("a.png".to_owned(), vec![1]), ("a.png".to_owned(), vec![2])];
    assert_eq!(
        save_project_folder(&dir, &hello(), &dup, &mut FolderRecord::new()),
        Err(FormatError::DuplicateEntry("a.png".to_owned()))
    );
}

#[cfg(unix)]
#[test]
fn symlinks_that_leave_the_folder_are_refused() {
    use std::os::unix::fs::symlink;
    let base = scratch("symlink");
    let dir = base.join("p.ccraft-folder");
    save(&dir, &hello(), &media(), &mut FolderRecord::new());
    let outside = base.join("secret.txt");
    std::fs::write(&outside, "secret").expect("write");

    symlink(&outside, dir.join("media/leak.txt")).expect("link");
    assert_eq!(
        open_project_folder(&dir).expect_err("file link out"),
        FormatError::UnsafePath("media/leak.txt".to_owned())
    );
    std::fs::remove_file(dir.join("media/leak.txt")).expect("rm");

    // A link to a file inside the folder is fine.
    symlink(
        dir.join("media/audio/chime.wav"),
        dir.join("media/again.wav"),
    )
    .expect("link");
    assert_eq!(
        open_project_folder(&dir)
            .expect("inside")
            .project
            .media
            .len(),
        4
    );
    std::fs::remove_file(dir.join("media/again.wav")).expect("rm");

    // A folder link is refused, and so is writing through one.
    let elsewhere = base.join("elsewhere");
    std::fs::create_dir_all(&elsewhere).expect("mkdir");
    symlink(&elsewhere, dir.join("media/linked")).expect("link");
    assert!(matches!(
        open_project_folder(&dir),
        Err(FormatError::UnsafePath(_))
    ));
    let mut record = FolderRecord::new();
    let err = save_project_folder(
        &dir,
        &hello(),
        &[("linked/x.png".to_owned(), vec![1])],
        &mut record,
    )
    .expect_err("write through link");
    assert_eq!(err, FormatError::UnsafePath("media/linked".to_owned()));
    assert!(!elsewhere.join("x.png").exists());
}

#[test]
fn a_too_big_folder_is_refused_like_the_zip() {
    let dir = scratch("limits").join("p.ccraft-folder");
    let items = vec![
        ("m1.png".to_owned(), vec![0u8; 60]),
        ("m2.png".to_owned(), vec![0u8; 60]),
        ("large.png".to_owned(), vec![0u8; 5000]),
    ];
    save(&dir, &hello(), &items, &mut FolderRecord::new());
    let zip = save_project(&hello(), &items).expect("zip");

    let cases = [
        Limits {
            max_entries: 3,
            ..Limits::default()
        },
        Limits {
            max_entry_bytes: 2000,
            ..Limits::default()
        },
        Limits {
            max_entry_bytes: 100_000,
            max_total_bytes: 3000,
            ..Limits::default()
        },
    ];
    for limits in cases {
        let from_zip = open_project_with_limits(&zip, limits).expect_err("zip refuses");
        let from_folder =
            open_project_folder_with_limits(&dir, limits).expect_err("folder refuses");
        match (&from_zip, &from_folder) {
            (FormatError::EntryTooLarge(_), FormatError::EntryTooLarge(_)) => {}
            _ => assert_eq!(from_zip, from_folder),
        }
        assert_eq!(
            from_zip.to_string().split(':').next(),
            from_folder.to_string().split(':').next()
        );
    }
}

#[test]
fn missing_and_damaged_course_json() {
    let dir = scratch("damaged").join("p.ccraft-folder");
    std::fs::create_dir_all(&dir).expect("mkdir");
    assert_eq!(
        open_project_folder(&dir).expect_err("none"),
        FormatError::MissingCourseJson
    );
    std::fs::write(dir.join("course.json"), "{ not json").expect("write");
    assert!(matches!(
        open_project_folder(&dir),
        Err(FormatError::CourseJson(_))
    ));
    std::fs::write(
        dir.join("course.json"),
        r#"{"schema_version": 9, "id": "x", "title": "T", "scenes": []}"#,
    )
    .expect("write");
    let message = open_project_folder(&dir).expect_err("newer").to_string();
    assert!(message.contains("newer CourseCraft"), "{message}");
}

#[test]
fn changed_files_notices_outside_edits_cheaply() {
    let dir = scratch("outside").join("p.ccraft-folder");
    let mut record = FolderRecord::new();
    save(&dir, &hello(), &media(), &mut record);
    assert!(changed_files(&dir, &mut record).is_empty());

    // Same bytes written again with a new time: not a change.
    std::thread::sleep(std::time::Duration::from_millis(30));
    std::fs::write(dir.join("media/fonts/lato.ttf"), b"font").expect("write");
    assert!(changed_files(&dir, &mut record).is_empty());

    // A teammate replaces a picture (different size) and deletes another file.
    std::fs::write(
        dir.join("media/images/badge.png"),
        b"a different, longer picture",
    )
    .expect("write");
    std::fs::remove_file(dir.join("media/audio/chime.wav")).expect("rm");
    // A brand-new file is not listed.
    std::fs::write(dir.join("media/new.png"), b"n").expect("write");
    assert_eq!(
        changed_files(&dir, &mut record),
        vec![
            "media/audio/chime.wav".to_owned(),
            "media/images/badge.png".to_owned()
        ]
    );
    // Saving heals a deleted file because the record check also stats it.
    let report = save(&dir, &hello(), &media(), &mut record);
    assert!(
        report.written.contains(&"media/audio/chime.wav".to_owned()),
        "{report:?}"
    );
    assert!(
        report
            .written
            .contains(&"media/images/badge.png".to_owned()),
        "{report:?}"
    );
}

#[test]
fn overwrite_replaces_a_same_size_file_changed_on_disk() {
    let dir = scratch("same-size").join("p.ccraft-folder");
    let mut record = FolderRecord::new();
    save(&dir, &hello(), &media(), &mut record);

    // A git checkout swaps the picture for different bytes of the same size.
    std::thread::sleep(std::time::Duration::from_millis(30));
    let path = dir.join("media/images/badge.png");
    std::fs::write(&path, [0x89, b'P', b'N', b'G', 2]).expect("write");
    assert_eq!(
        changed_files(&dir, &mut record),
        vec!["media/images/badge.png".to_owned()]
    );

    // The author chooses Overwrite: their picture goes back.
    let report = save(&dir, &hello(), &media(), &mut record);
    assert!(
        report
            .written
            .contains(&"media/images/badge.png".to_owned()),
        "{report:?}"
    );
    assert_eq!(
        std::fs::read(&path).expect("read"),
        vec![0x89, b'P', b'N', b'G', 1]
    );
}

#[test]
fn fnv1a_matches_the_known_vectors() {
    assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
    assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
    assert_eq!(fnv1a64(b"foobar"), 0x8594_4171_f739_67e8);
}

#[test]
fn a_save_that_could_not_be_opened_again_is_refused_before_writing() {
    let dir = scratch("save-limits").join("p.ccraft-folder");
    let items = vec![("big.png".to_owned(), vec![0u8; 50_000])];
    let hooks = SaveHooks {
        limits: Limits {
            max_entry_bytes: 40_000,
            ..Limits::default()
        },
        ..SaveHooks::default()
    };
    let err = save_project_folder_with(&dir, &hello(), &items, &mut FolderRecord::new(), &hooks)
        .expect_err("too big");
    assert_eq!(err, FormatError::EntryTooLarge("media/big.png".to_owned()));
    assert!(!dir.exists(), "nothing was written");

    let hooks = SaveHooks {
        limits: Limits {
            max_entries: 1,
            ..Limits::default()
        },
        ..SaveHooks::default()
    };
    let err = save_project_folder_with(&dir, &hello(), &items, &mut FolderRecord::new(), &hooks)
        .expect_err("too many");
    assert_eq!(err, FormatError::TooManyEntries);
}
