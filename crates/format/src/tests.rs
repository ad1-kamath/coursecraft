use super::*;
use coursecraft_course::{
    Course, Object, ObjectKind, PlayerSettings, Rect, Scene, Size, Slide, Timing,
};

const HELLO_COURSE_JSON: &str = include_str!("../../../examples/hello-course/course.json");

fn minimal_course(title: &str) -> Course {
    Course {
        schema_version: 1,
        id: "test-course".into(),
        title: title.into(),
        slide_size: Size::default(),
        theme: coursecraft_course::Theme::default(),
        variables: Vec::new(),
        scenes: vec![Scene {
            id: "scene1".into(),
            title: "Scene 1".into(),
            slides: vec![Slide {
                id: "slide1".into(),
                title: "Slide 1".into(),
                duration_ms: 5000,
                background: None,
                objects: Vec::new(),
                layers: Vec::new(),
                triggers: Vec::new(),
                question: None,
                draw: None,
                transition: None,
                notes: String::new(),
                master: None,
                revisit: coursecraft_course::RevisitMode::Auto,
                player: None,
            }],
        }],
        masters: Vec::new(),
        templates: Vec::new(),
        banks: Vec::new(),
        start_slide: None,
        quiz: None,
        player: PlayerSettings::default(),
        language: None,
        strings: std::collections::BTreeMap::new(),
        fonts: Vec::new(),
        background_audio: None,
        glossary: Vec::new(),
        resources: Vec::new(),
        text_styles: Vec::new(),
    }
}

#[test]
fn ccraft_round_trip_keeps_fonts_and_their_media() {
    let mut course = minimal_course("Fonts");
    course.fonts.push(coursecraft_course::FontFace {
        family: "Lato".into(),
        style: coursecraft_course::FontStyle::Bold,
        src: "fonts/lato-bold.ttf".into(),
    });
    let media = vec![("fonts/lato-bold.ttf".to_string(), b"font".to_vec())];
    let saved = save_project(&course, &media).expect("save");
    let opened = open_project(&saved).expect("open");
    assert_eq!(opened.course.fonts, course.fonts);
    assert_eq!(opened.media, media);
}

// ---------------------------------------------------------------------------
// Moved tests for is_safe_relative_path
// ---------------------------------------------------------------------------

#[test]
fn safe_relative_path_accepts_plain_paths() {
    for p in [
        "index.html",
        "app.js",
        "media/pic.png",
        "assets/icons/star.svg",
    ] {
        assert!(is_safe_relative_path(p), "should accept {p:?}");
    }
}

#[test]
fn safe_relative_path_rejects_separators_and_schemes() {
    for p in [
        "",
        "/index.html",
        "/media/pic.png",
        "media\\pic.png",
        "a\\b\\c",
        "C:file.png",
        "C:/file.png",
        "file://foo.png",
        "http://foo.com/pic.png",
        "javascript:alert(1)",
    ] {
        assert!(!is_safe_relative_path(p), "should reject {p:?}");
    }
}

#[test]
fn safe_relative_path_rejects_bad_segments() {
    for p in [
        "foo//bar",
        "foo/bar/",
        "/foo/bar",
        ".",
        "foo/./bar",
        "./bar",
        "bar/.",
        "..",
        "foo/../bar",
        "../bar",
        "bar/..",
    ] {
        assert!(!is_safe_relative_path(p), "should reject {p:?}");
    }
}

#[test]
fn safe_relative_path_rejects_control_chars() {
    for p in [
        "foo\0bar",
        "foo\nbar",
        "foo\rbar",
        "foo\tbar",
        "foo\x1fbar",
        "foo\x7fbar",
    ] {
        assert!(!is_safe_relative_path(p), "should reject {p:?}");
    }
}

// ---------------------------------------------------------------------------
// Format tests
// ---------------------------------------------------------------------------

#[test]
fn round_trip_hello_course_and_media() {
    let course = Course::from_json(HELLO_COURSE_JSON).expect("valid hello-course");
    let media = vec![
        (
            "images/badge.png".to_string(),
            vec![0x89, b'P', b'N', b'G', 1, 2, 3],
        ),
        (
            "audio/chime.wav".to_string(),
            vec![b'R', b'I', b'F', b'F', 4, 5, 6],
        ),
    ];

    let bytes = save_project(&course, &media).expect("save_project succeeds");
    let project = open_project(&bytes).expect("open_project succeeds");

    assert_eq!(project.course, course);
    assert_eq!(project.media, media);
}

#[test]
fn deterministic_bytes() {
    let course = Course::from_json(HELLO_COURSE_JSON).expect("valid hello-course");
    let media = vec![
        ("img1.png".to_string(), vec![1, 2, 3, 4]),
        ("snd1.mp3".to_string(), vec![5, 6, 7, 8]),
    ];

    let bytes1 = save_project(&course, &media).expect("save first time");
    let bytes2 = save_project(&course, &media).expect("save second time");

    assert_eq!(bytes1, bytes2, "output bytes must be deterministic");
}

#[test]
fn missing_course_json() {
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    writer
        .start_file("media/icon.png", options)
        .expect("start file");
    writer.write_all(b"fake png").expect("write file");
    let bytes = writer.finish().expect("finish").into_inner();

    let err = open_project(&bytes).expect_err("should fail without course.json");
    assert_eq!(err, FormatError::MissingCourseJson);
}

#[test]
fn bad_json() {
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    writer
        .start_file("course.json", options)
        .expect("start file");
    writer
        .write_all(b"{ not valid json: true }")
        .expect("write file");
    let bytes = writer.finish().expect("finish").into_inner();

    let err = open_project(&bytes).expect_err("should fail on invalid JSON");
    assert!(
        matches!(err, FormatError::CourseJson(_)),
        "expected CourseJson error, got {err:?}"
    );
}

#[test]
fn unsafe_entry_name() {
    // 1. Handcrafted zip with "../evil"
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    let course_json = Course::to_json(&minimal_course("test")).expect("to_json");
    writer
        .start_file("course.json", options)
        .expect("start course.json");
    writer
        .write_all(course_json.as_bytes())
        .expect("write course.json");
    writer
        .start_file("../evil", options)
        .expect("start ../evil");
    writer.write_all(b"payload").expect("write evil");
    let bytes = writer.finish().expect("finish").into_inner();

    let err = open_project(&bytes).expect_err("should reject ../evil");
    assert_eq!(err, FormatError::UnsafePath("../evil".to_string()));

    // 2. Handcrafted zip with "/abs"
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    writer
        .start_file("course.json", options)
        .expect("start course.json");
    writer
        .write_all(course_json.as_bytes())
        .expect("write course.json");
    writer.start_file("/abs", options).expect("start /abs");
    writer.write_all(b"payload").expect("write abs");
    let bytes = writer.finish().expect("finish").into_inner();

    let err = open_project(&bytes).expect_err("should reject /abs");
    assert_eq!(err, FormatError::UnsafePath("/abs".to_string()));

    // 3. save_project rejects unsafe media paths
    let course = minimal_course("test");
    let bad_media = vec![("../evil.png".to_string(), vec![1, 2, 3])];
    let err = save_project(&course, &bad_media).expect_err("save rejects unsafe path");
    assert_eq!(err, FormatError::UnsafePath("../evil.png".to_string()));
}

#[test]
fn duplicate_entry() {
    // 1. save_project rejects duplicate media paths
    let course = minimal_course("test");
    let dup_media = vec![
        ("badge.png".to_string(), vec![1, 2]),
        ("badge.png".to_string(), vec![3, 4]),
    ];
    let err = save_project(&course, &dup_media).expect_err("save rejects duplicates");
    assert_eq!(err, FormatError::DuplicateEntry("badge.png".to_string()));

    // 2. zip 9 ZipWriter prevents writing duplicate filenames at creation time.
    // As noted in task requirements ("a duplicate entry, if zip 9 lets you write one
    // (otherwise skip and say so)"): zip 9's ZipWriter returns an error if start_file
    // is called with a name that is already in the archive.
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    writer.start_file("file.txt", options).expect("first file");
    let dup_res = writer.start_file("file.txt", options);
    assert!(
        dup_res.is_err(),
        "zip 9 does not allow writing duplicate filenames via ZipWriter"
    );
}

#[test]
fn too_many_entries() {
    let course = minimal_course("test");
    let media = vec![
        ("m1.png".to_string(), vec![1]),
        ("m2.png".to_string(), vec![2]),
        ("m3.png".to_string(), vec![3]),
    ];
    let bytes = save_project(&course, &media).expect("save succeeds");

    // 4 total entries: course.json + 3 media files. Limit to 3.
    let limits = Limits {
        max_entries: 3,
        ..Limits::default()
    };
    let err = open_project_with_limits(&bytes, limits).expect_err("should reject too many entries");
    assert_eq!(err, FormatError::TooManyEntries);
}

#[test]
fn entry_over_small_limit() {
    let course = minimal_course("test");
    let media = vec![("large.png".to_string(), vec![0u8; 5000])];
    let bytes = save_project(&course, &media).expect("save succeeds");

    // course.json is ~500 bytes. Cap single entry bytes at 2000 bytes, so
    // course.json fits but media/large.png exceeds the limit.
    let limits = Limits {
        max_entry_bytes: 2000,
        ..Limits::default()
    };
    let err =
        open_project_with_limits(&bytes, limits).expect_err("should reject oversized single entry");
    assert_eq!(
        err,
        FormatError::EntryTooLarge("media/large.png".to_string())
    );
}

#[test]
fn total_archive_over_small_limit() {
    let course = minimal_course("test");
    let media = vec![
        ("m1.png".to_string(), vec![0u8; 60]),
        ("m2.png".to_string(), vec![0u8; 60]),
    ];
    let bytes = save_project(&course, &media).expect("save succeeds");

    // Total bytes will exceed 120 bytes. Set max_total_bytes to 80.
    let limits = Limits {
        max_entry_bytes: 1000,
        max_total_bytes: 80,
        ..Limits::default()
    };
    let err = open_project_with_limits(&bytes, limits).expect_err("should reject oversized total");
    assert_eq!(err, FormatError::TooLarge);
}

#[test]
fn unknown_non_media_entries_ignored() {
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    let course_json = Course::to_json(&minimal_course("test")).expect("to_json");

    writer
        .start_file("course.json", options)
        .expect("start course.json");
    writer
        .write_all(course_json.as_bytes())
        .expect("write course.json");

    writer
        .start_file("media/icon.png", options)
        .expect("start media file");
    writer.write_all(b"icon bytes").expect("write media file");

    writer
        .start_file("README.md", options)
        .expect("start readme");
    writer.write_all(b"# Hello").expect("write readme");

    writer
        .start_file("metadata.xml", options)
        .expect("start metadata");
    writer.write_all(b"<xml/>").expect("write metadata");

    let bytes = writer.finish().expect("finish").into_inner();
    let project = open_project(&bytes).expect("open succeeds ignoring unknown entries");

    assert_eq!(project.media.len(), 1);
    assert_eq!(project.media[0].0, "icon.png");
    assert_eq!(project.media[0].1, b"icon bytes");
}

#[test]
fn directory_entries_skipped() {
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    let course_json = Course::to_json(&minimal_course("test")).expect("to_json");

    writer
        .start_file("course.json", options)
        .expect("start course.json");
    writer
        .write_all(course_json.as_bytes())
        .expect("write course.json");

    writer
        .add_directory("media", options)
        .expect("add media dir");

    writer
        .start_file("media/photo.jpg", options)
        .expect("start photo");
    writer.write_all(b"photo bytes").expect("write photo");

    let bytes = writer.finish().expect("finish").into_inner();
    let project = open_project(&bytes).expect("open succeeds skipping directories");

    assert_eq!(project.media.len(), 1);
    assert_eq!(project.media[0].0, "photo.jpg");
    assert_eq!(project.media[0].1, b"photo bytes");
}

#[test]
fn unsafe_directory_entry_rejected() {
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    let course_json = Course::to_json(&minimal_course("test")).expect("to_json");

    writer
        .start_file("course.json", options)
        .expect("start course.json");
    writer
        .write_all(course_json.as_bytes())
        .expect("write course.json");

    writer
        .add_directory("../evil_dir", options)
        .expect("add evil dir");

    let bytes = writer.finish().expect("finish").into_inner();
    let err = open_project(&bytes).expect_err("should reject unsafe directory");
    assert_eq!(err, FormatError::UnsafePath("../evil_dir/".to_string()));
}

#[test]
fn empty_zip() {
    let writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let bytes = writer.finish().expect("finish").into_inner();

    let err = open_project(&bytes).expect_err("should fail on empty zip");
    assert_eq!(err, FormatError::MissingCourseJson);
}

// Deterministic Linear Congruential Generator for non-panicking pseudo-random test inputs.
struct Lcg(u64);

impl Lcg {
    fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next_u32(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1);
        (self.0 >> 32) as u32
    }

    fn next_bytes(&mut self, len: usize) -> Vec<u8> {
        let mut buf = vec![0u8; len];
        for b in &mut buf {
            *b = self.next_u32() as u8;
        }
        buf
    }
}

#[test]
fn random_garbage_bytes_never_panic() {
    let mut rng = Lcg::new(0x1234_5678_9ABC_DEF0);
    for _ in 0..500 {
        let len = (rng.next_u32() % 2048) as usize;
        let garbage = rng.next_bytes(len);
        let _ = open_project(&garbage);
    }
}

#[test]
fn lottie_animation_survives_a_ccraft_round_trip() {
    let mut course = minimal_course("Lottie");
    course.scenes[0].slides[0].objects.push(Object {
        id: "lot1".into(),
        name: "Animation".into(),
        kind: ObjectKind::Lottie {
            src: "motion.lottie".into(),
            loop_playback: false,
            autoplay: true,
            speed: 2.5,
        },
        frame: Rect {
            x: 10.0,
            y: 20.0,
            w: 320.0,
            h: 180.0,
        },
        timing: Timing::default(),
        states: Vec::new(),
        initial_state: "normal".into(),
        entrance: None,
        exit: None,
        motion_paths: Vec::new(),
        focus_order: None,
        decorative: false,
        description: None,
        group: None,
        panel: None,
        rotation: 0.0,
        flip_h: false,
        flip_v: false,
        opacity: 1.0,
        shadow: None,
        glow: None,
        drag: None,
    });
    let media = vec![("motion.lottie".to_string(), b"PK\x03\x04".to_vec())];
    let bytes = save_project(&course, &media).expect("save");
    let project = open_project(&bytes).expect("open");
    assert_eq!(project.course, course);
    assert_eq!(project.media, media);
    assert!(matches!(
        project.course.scenes[0].slides[0].objects[0].kind,
        ObjectKind::Lottie {
            loop_playback: false,
            autoplay: true,
            speed,
            ..
        } if (speed - 2.5).abs() < f32::EPSILON
    ));
}

#[test]
fn caption_files_survive_a_ccraft_round_trip_for_audio_and_video() {
    for (id, kind, vtt) in [
        (
            "audio1",
            ObjectKind::Audio {
                src: "sound.wav".into(),
                autoplay: false,
                captions: Some("sound.vtt".into()),
                volume: 1.0,
                trim: None,
            },
            "sound.vtt",
        ),
        (
            "video1",
            ObjectKind::Video {
                src: "clip.mp4".into(),
                autoplay: false,
                captions: Some("clip.vtt".into()),
                volume: 1.0,
                trim: None,
                cues: Vec::new(),
            },
            "clip.vtt",
        ),
    ] {
        let mut course = minimal_course("Captions");
        course.scenes[0].slides[0].objects.push(Object {
            id: id.into(),
            name: id.into(),
            kind,
            frame: Rect {
                x: 0.0,
                y: 0.0,
                w: 10.0,
                h: 10.0,
            },
            timing: Timing::default(),
            states: Vec::new(),
            initial_state: "normal".into(),
            entrance: None,
            exit: None,
            motion_paths: Vec::new(),
            focus_order: None,
            decorative: false,
            description: None,
            group: None,
            panel: None,
            rotation: 0.0,
            flip_h: false,
            flip_v: false,
            opacity: 1.0,
            shadow: None,
            glow: None,
            drag: None,
        });
        let media = vec![(vtt.to_string(), b"WEBVTT".to_vec())];
        let bytes = save_project(&course, &media).expect("save");
        let project = open_project(&bytes).expect("open");
        assert_eq!(project.course, course);
        assert_eq!(project.media, media);
        let object = &project.course.scenes[0].slides[0].objects[0];
        let captions = match &object.kind {
            ObjectKind::Audio { captions, .. } | ObjectKind::Video { captions, .. } => {
                captions.as_deref()
            }
            _ => None,
        };
        assert_eq!(captions, Some(vtt));
    }
}

fn method_of(bytes: &[u8], name: &str) -> zip::CompressionMethod {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("zip opens");
    let file = archive.by_name(name).expect("entry exists");
    file.compression()
}

#[test]
fn already_compressed_media_is_stored_and_text_is_deflated() {
    let media = vec![
        ("a.png".to_string(), vec![7u8; 4096]),
        ("b.MP4".to_string(), vec![8u8; 4096]),
        ("notes.txt".to_string(), vec![9u8; 4096]),
    ];
    let bytes = save_project(&minimal_course("t"), &media).expect("save");
    assert_eq!(
        method_of(&bytes, "media/a.png"),
        zip::CompressionMethod::Stored
    );
    assert_eq!(
        method_of(&bytes, "media/b.MP4"),
        zip::CompressionMethod::Stored
    );
    assert_eq!(
        method_of(&bytes, "media/notes.txt"),
        zip::CompressionMethod::Deflated
    );
    assert_eq!(
        method_of(&bytes, "course.json"),
        zip::CompressionMethod::Deflated
    );
    let project = open_project(&bytes).expect("open");
    assert_eq!(project.media.len(), 3);
    assert!(project.media.iter().all(|(_, b)| b.len() == 4096));
}

#[test]
fn files_saved_with_everything_deflated_still_open() {
    let course_json = minimal_course("old").to_json().expect("json");
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    writer.start_file("course.json", options).expect("start");
    writer.write_all(course_json.as_bytes()).expect("write");
    writer.start_file("media/pic.png", options).expect("start");
    writer.write_all(&[3u8; 10_000]).expect("write");
    let bytes = writer.finish().expect("finish").into_inner();
    let project = open_project(&bytes).expect("an older, fully deflated file opens");
    assert_eq!(project.media.len(), 1);
}

#[test]
fn save_project_to_writes_the_same_bytes_as_save_project() {
    let media = vec![("a.png".to_string(), vec![1u8; 300])];
    let course = minimal_course("same");
    let expected = save_project(&course, &media).expect("save");
    let mut out = std::io::Cursor::new(Vec::new());
    save_project_to(&course, &media, &mut out).expect("save to writer");
    assert_eq!(out.into_inner(), expected);
}

#[test]
fn precompressed_check_ignores_case_and_folders() {
    assert!(is_precompressed("img/Photo.JPG"));
    assert!(is_precompressed("video/clip.webm"));
    assert!(!is_precompressed("captions/a.vtt"));
    assert!(!is_precompressed("noextension"));
    assert!(!is_precompressed("dir.png/readme"));
}

#[test]
fn long_narration_and_documents_are_stored() {
    assert!(is_precompressed("narration/take1.WAV"));
    assert!(is_precompressed("a.aiff"));
    assert!(is_precompressed("handout.pdf"));
}

#[test]
fn verify_refuses_a_project_that_could_not_be_opened_again() {
    let media = vec![
        ("a.png".to_string(), vec![7u8; 5000]),
        ("b.png".to_string(), vec![8u8; 5000]),
    ];
    let bytes = save_project(&minimal_course("v"), &media).expect("save");
    let verify = |limits| verify_project_with_limits(std::io::Cursor::new(&bytes), limits);

    let one_entry_over = Limits {
        max_entry_bytes: 4000,
        ..Limits::default()
    };
    assert_eq!(
        verify(one_entry_over),
        Err(FormatError::EntryTooLarge("media/a.png".to_string()))
    );
    let total_over = Limits {
        max_total_bytes: 9000,
        ..Limits::default()
    };
    assert_eq!(verify(total_over), Err(FormatError::TooLarge));
    let too_many = Limits {
        max_entries: 2,
        ..Limits::default()
    };
    assert_eq!(verify(too_many), Err(FormatError::TooManyEntries));

    // Verify and open agree on the same limits.
    for limits in [one_entry_over, total_over, too_many] {
        assert!(open_project_with_limits(&bytes, limits).is_err());
    }
    assert_eq!(verify(Limits::default()), Ok(()));
}

#[test]
fn verify_project_accepts_a_whole_save_and_rejects_damage() {
    let media = vec![("a.png".to_string(), vec![7u8; 5000])];
    let bytes = save_project(&minimal_course("v"), &media).expect("save");
    verify_project(std::io::Cursor::new(&bytes)).expect("a whole save verifies");

    let mut damaged = bytes.clone();
    let at = damaged
        .windows(100)
        .position(|w| w.iter().all(|b| *b == 7))
        .expect("stored bytes present");
    damaged[at + 50] ^= 0xFF;
    assert!(verify_project(std::io::Cursor::new(&damaged)).is_err());
    assert!(verify_project(std::io::Cursor::new(&bytes[..bytes.len() / 2])).is_err());
    assert!(verify_project(std::io::Cursor::new(Vec::<u8>::new())).is_err());
}
