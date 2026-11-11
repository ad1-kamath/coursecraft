use super::*;
use coursecraft_course::{
    BACKGROUND_AUDIO_ID, BackgroundAudio, Object, ObjectKind, PlayerSettings, Rect, Scene, Size,
    Slide, StateDef, Timing,
};
use std::io::Read as _;

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

fn fake_player() -> Vec<PackageFile> {
    vec![
        PackageFile {
            path: "index.html".into(),
            bytes: b"<!doctype html><html><body>fake player</body></html>".to_vec(),
        },
        PackageFile {
            path: "app.js".into(),
            bytes: b"console.log('app');".to_vec(),
        },
        PackageFile {
            path: "app_bg.wasm".into(),
            bytes: b"\0asm fake wasm binary".to_vec(),
        },
        PackageFile {
            path: "course.json".into(),
            bytes: b"{\"stale\": true}".to_vec(),
        },
    ]
}

fn web_object(id: &str, path: &str, entry: &str) -> Object {
    Object {
        id: id.into(),
        name: "Site".into(),
        kind: ObjectKind::Web {
            source: coursecraft_course::WebSource::Folder {
                path: path.into(),
                entry: entry.into(),
            },
        },
        frame: Rect {
            x: 0.0,
            y: 0.0,
            w: 640.0,
            h: 360.0,
        },
        timing: Timing::default(),
        entrance: None,
        exit: None,
        motion_paths: Vec::new(),
        states: Vec::<StateDef>::new(),
        initial_state: "normal".into(),
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
    }
}

#[test]
fn background_audio_is_packaged() {
    let mut course = minimal_course("Background audio");
    course.background_audio = Some(BackgroundAudio {
        src: "music.mp3".into(),
        volume: 0.5,
        looping: true,
    });
    let player_files = fake_player();
    let course_json = course.to_json().expect("course json");
    let media =
        |src: &str| -> Option<Vec<u8>> { (src == "music.mp3").then(|| b"audio bytes".to_vec()) };
    let input = PackageInput {
        course: &course,
        course_json: &course_json,
        player_files: &player_files,
        media: &media,
    };
    let files = build_package(&input).expect("package builds");
    let paths: Vec<&str> = files.iter().map(|file| file.path.as_str()).collect();
    assert!(
        paths.contains(&"media/music.mp3"),
        "background track missing from {paths:?}"
    );
}

#[test]
fn missing_background_audio_is_reported() {
    let mut course = minimal_course("Background audio");
    course.background_audio = Some(BackgroundAudio {
        src: "music.mp3".into(),
        volume: 0.5,
        looping: true,
    });
    let player_files = fake_player();
    let course_json = course.to_json().expect("course json");
    let media = |_src: &str| -> Option<Vec<u8>> { None };
    let input = PackageInput {
        course: &course,
        course_json: &course_json,
        player_files: &player_files,
        media: &media,
    };
    let error = build_package(&input).expect_err("missing background audio must fail");
    assert_eq!(
        error,
        PublishError::MissingMedia {
            object: BACKGROUND_AUDIO_ID.into(),
            src: "music.mp3".into(),
        }
    );
}

#[test]
fn web_folder_files_keep_their_relative_paths_in_the_package() {
    let mut course = minimal_course("Web folder");
    course.scenes[0].slides[0]
        .objects
        .push(web_object("web1", "web/site", "index.html"));
    let player_files = fake_player();
    let course_json = course.to_json().expect("course json");
    let media = |src: &str| -> Option<Vec<u8>> {
        (src == "web/site/index.html").then(|| b"<html>site</html>".to_vec())
    };
    let folder = |path: &str| -> Option<Vec<(String, Vec<u8>)>> {
        (path == "web/site").then(|| {
            vec![
                ("web/site/index.html".into(), b"<html>site</html>".to_vec()),
                ("web/site/style.css".into(), b"body{}".to_vec()),
                ("web/site/assets/app.js".into(), b"run()".to_vec()),
            ]
        })
    };
    let input = PackageInput {
        course: &course,
        course_json: &course_json,
        player_files: &player_files,
        media: &media,
    };
    let files = build_package_standard_with_folders(
        &input,
        PackageStandard::Scorm(Standard::Scorm12),
        &folder,
    )
    .expect("package builds with folder");
    let paths: Vec<&str> = files.iter().map(|file| file.path.as_str()).collect();
    for path in [
        "media/web/site/index.html",
        "media/web/site/style.css",
        "media/web/site/assets/app.js",
    ] {
        assert!(paths.contains(&path), "{path} missing from {paths:?}");
    }
    let manifest = files
        .iter()
        .find(|file| file.path == "imsmanifest.xml")
        .expect("manifest");
    let manifest = std::str::from_utf8(&manifest.bytes).expect("manifest utf8");
    assert!(manifest.contains("media/web/site/assets/app.js"));
}

#[test]
fn missing_web_folder_entry_is_reported() {
    let mut course = minimal_course("Web folder");
    course.scenes[0].slides[0]
        .objects
        .push(web_object("web1", "web/site", "index.html"));
    let player_files = fake_player();
    let course_json = course.to_json().expect("course json");
    let media = |_src: &str| -> Option<Vec<u8>> { None };
    let folder = |path: &str| -> Option<Vec<(String, Vec<u8>)>> {
        (path == "web/site").then(|| vec![("web/site/other.html".into(), b"x".to_vec())])
    };
    let input = PackageInput {
        course: &course,
        course_json: &course_json,
        player_files: &player_files,
        media: &media,
    };
    let error = build_package_standard_with_folders(
        &input,
        PackageStandard::Scorm(Standard::Scorm12),
        &folder,
    )
    .expect_err("missing entry must fail");
    assert!(
        matches!(error, PublishError::MissingMedia { ref src, .. } if src == "web/site/index.html"),
        "{error:?}"
    );
}

#[test]
fn hello_course_packaged_with_fake_player() {
    let course = Course::from_json(HELLO_COURSE_JSON).expect("valid course");
    let no_media = |_src: &str| -> Option<Vec<u8>> { None };

    let input = PackageInput {
        course: &course,
        course_json: HELLO_COURSE_JSON,
        player_files: &fake_player(),
        media: &no_media,
    };

    let files = build_package(&input).expect("package builds");

    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(
        paths,
        vec![
            "app.js",
            "app_bg.wasm",
            "course.json",
            "imsmanifest.xml",
            "index.html"
        ]
    );

    // course.json is ours, replacing the stale one
    let course_file = files
        .iter()
        .find(|f| f.path == "course.json")
        .expect("course.json present");
    assert_eq!(course_file.bytes, HELLO_COURSE_JSON.as_bytes());

    // imsmanifest.xml lists all files and has mastery score 80
    let manifest_file = files
        .iter()
        .find(|f| f.path == "imsmanifest.xml")
        .expect("imsmanifest.xml present");
    let xml = std::str::from_utf8(&manifest_file.bytes).expect("valid utf-8 manifest");

    assert!(xml.contains("<adlcp:masteryscore>80</adlcp:masteryscore>"));
    assert!(xml.contains("<file href=\"app.js\"/>"));
    assert!(xml.contains("<file href=\"app_bg.wasm\"/>"));
    assert!(xml.contains("<file href=\"course.json\"/>"));
    assert!(xml.contains("<file href=\"imsmanifest.xml\"/>"));
    assert!(xml.contains("<file href=\"index.html\"/>"));
}

#[test]
fn course_with_image_packaged_under_media() {
    let mut course = minimal_course("Image Course");
    course.scenes[0].slides[0].objects.push(Object {
        id: "img1".into(),
        name: "Picture".into(),
        kind: ObjectKind::Image {
            crop: None,
            src: "media/pic.png".into(),
            alt: "A picture".into(),
        },
        frame: Rect {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 100.0,
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

    let fake_png = vec![0x89, 0x50, 0x4e, 0x47];
    let media = |src: &str| -> Option<Vec<u8>> {
        if src == "media/pic.png" {
            Some(fake_png.clone())
        } else {
            None
        }
    };

    let course_json = course.to_json().expect("serialize");
    let input = PackageInput {
        course: &course,
        course_json: &course_json,
        player_files: &fake_player(),
        media: &media,
    };

    let files = build_package(&input).expect("package builds");
    let media_file = files
        .iter()
        .find(|f| f.path == "media/media/pic.png")
        .expect("media file packaged");
    assert_eq!(media_file.bytes, fake_png);

    let manifest_file = files
        .iter()
        .find(|f| f.path == "imsmanifest.xml")
        .expect("manifest present");
    let xml = std::str::from_utf8(&manifest_file.bytes).expect("utf8");
    assert!(xml.contains("<file href=\"media/media/pic.png\"/>"));
}

#[test]
fn course_with_svg_image_packaged_under_media() {
    let mut course = minimal_course("SVG Course");
    course.scenes[0].slides[0].objects.push(Object {
        id: "svg1".into(),
        name: "Vector".into(),
        kind: ObjectKind::Image {
            crop: None,
            src: "media/logo.svg".into(),
            alt: "Logo".into(),
        },
        frame: Rect {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 100.0,
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

    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><rect width="16" height="16" fill="#123456"/></svg>"##.to_vec();
    let media = |src: &str| -> Option<Vec<u8>> {
        if src == "media/logo.svg" {
            Some(svg.clone())
        } else {
            None
        }
    };

    let course_json = course.to_json().expect("serialize");
    let input = PackageInput {
        course: &course,
        course_json: &course_json,
        player_files: &fake_player(),
        media: &media,
    };
    let files = build_package(&input).expect("package builds");
    let media_file = files
        .iter()
        .find(|f| f.path == "media/media/logo.svg")
        .expect("SVG packaged under media/");
    assert_eq!(media_file.bytes, svg);

    let manifest = files
        .iter()
        .find(|f| f.path == "imsmanifest.xml")
        .expect("manifest present");
    let xml = std::str::from_utf8(&manifest.bytes).expect("utf8");
    assert!(xml.contains("<file href=\"media/media/logo.svg\"/>"));
}

#[test]
fn course_fonts_ship_as_media() {
    let mut course = minimal_course("Fonts");
    course.fonts.push(coursecraft_course::FontFace {
        family: "Lato".into(),
        style: coursecraft_course::FontStyle::Regular,
        src: "fonts/lato.ttf".into(),
    });
    let font_bytes = b"font-bytes".to_vec();
    let media =
        |src: &str| -> Option<Vec<u8>> { (src == "fonts/lato.ttf").then(|| font_bytes.clone()) };
    let course_json = course.to_json().expect("serialize");
    let input = PackageInput {
        course: &course,
        course_json: &course_json,
        player_files: &fake_player(),
        media: &media,
    };
    let files = build_package(&input).expect("package builds");
    let packaged = files
        .iter()
        .find(|f| f.path == "media/fonts/lato.ttf")
        .expect("font packaged under media/");
    assert_eq!(packaged.bytes, b"font-bytes");
    let manifest = files
        .iter()
        .find(|f| f.path == "imsmanifest.xml")
        .expect("manifest present");
    assert!(
        std::str::from_utf8(&manifest.bytes)
            .expect("utf8")
            .contains("media/fonts/lato.ttf")
    );
}

#[test]
fn missing_media_errors() {
    let mut course = minimal_course("Missing Media");
    course.scenes[0].slides[0].objects.push(Object {
        id: "img-missing".into(),
        name: "Missing Pic".into(),
        kind: ObjectKind::Image {
            crop: None,
            src: "media/pic.png".into(),
            alt: "".into(),
        },
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

    let no_media = |_src: &str| -> Option<Vec<u8>> { None };
    let course_json = course.to_json().expect("serialize");
    let input = PackageInput {
        course: &course,
        course_json: &course_json,
        player_files: &fake_player(),
        media: &no_media,
    };

    let err = build_package(&input).expect_err("must error on missing media");
    assert_eq!(
        err,
        PublishError::MissingMedia {
            object: "img-missing".into(),
            src: "media/pic.png".into(),
        }
    );
}

#[test]
fn each_unsafe_path_case_errors() {
    let unsafe_cases = [
        "",
        "/abs/file.png",
        "foo\\bar.png",
        "C:file.png",
        "ftp://host/file.png",
        "javascript:alert(1)",
        "foo//bar.png",
        "foo/bar/",
        "foo/./bar.png",
        ".",
        "foo/../bar.png",
        "..",
        "foo\0bar.png",
        "foo\nbar.png",
    ];

    for unsafe_src in unsafe_cases {
        let mut course = minimal_course("Unsafe Path");
        course.scenes[0].slides[0].objects.push(Object {
            id: "bad-obj".into(),
            name: "Bad".into(),
            kind: ObjectKind::Image {
                crop: None,
                src: unsafe_src.into(),
                alt: "".into(),
            },
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

        let dummy_media = |_src: &str| -> Option<Vec<u8>> { Some(vec![1, 2, 3]) };
        let course_json = course.to_json().expect("serialize");
        let input = PackageInput {
            course: &course,
            course_json: &course_json,
            player_files: &fake_player(),
            media: &dummy_media,
        };

        let err = build_package(&input).expect_err("must reject unsafe path");
        assert_eq!(
            err,
            PublishError::UnsafePath {
                object: "bad-obj".into(),
                src: unsafe_src.into(),
            },
            "failed on case: {unsafe_src:?}"
        );
    }

    // Also test unsafe player file path
    let course = minimal_course("Player Unsafe");
    let course_json = course.to_json().expect("serialize");
    let no_media = |_src: &str| -> Option<Vec<u8>> { None };

    let bad_player_files = vec![
        PackageFile {
            path: "index.html".into(),
            bytes: b"ok".to_vec(),
        },
        PackageFile {
            path: "../escape.js".into(),
            bytes: b"bad".to_vec(),
        },
    ];

    let input = PackageInput {
        course: &course,
        course_json: &course_json,
        player_files: &bad_player_files,
        media: &no_media,
    };

    let err = build_package(&input).expect_err("must reject unsafe player file");
    assert_eq!(
        err,
        PublishError::UnsafePath {
            object: "player".into(),
            src: "../escape.js".into(),
        }
    );
}

#[test]
fn missing_index_html_errors() {
    let course = minimal_course("No Index");
    let course_json = course.to_json().expect("serialize");
    let no_media = |_src: &str| -> Option<Vec<u8>> { None };

    let player_without_index = vec![PackageFile {
        path: "app.js".into(),
        bytes: b"console.log('app');".to_vec(),
    }];

    let input = PackageInput {
        course: &course,
        course_json: &course_json,
        player_files: &player_without_index,
        media: &no_media,
    };

    let err = build_package(&input).expect_err("missing index.html");
    assert_eq!(err, PublishError::MissingIndexHtml);
}

#[test]
fn read_zip_back_with_zip_archive() {
    let course = Course::from_json(HELLO_COURSE_JSON).expect("valid course");
    let no_media = |_src: &str| -> Option<Vec<u8>> { None };

    let input = PackageInput {
        course: &course,
        course_json: HELLO_COURSE_JSON,
        player_files: &fake_player(),
        media: &no_media,
    };

    let files = build_package(&input).expect("package builds");
    let zip_bytes = zip_package(&files).expect("zip builds");

    let cursor = std::io::Cursor::new(zip_bytes);
    let mut archive = zip::ZipArchive::new(cursor).expect("valid zip archive");

    assert_eq!(archive.len(), files.len());
    for file in &files {
        let mut entry = archive.by_name(&file.path).expect("file in zip");
        let mut extracted = Vec::new();
        entry.read_to_end(&mut extracted).expect("read entry");
        assert_eq!(extracted, file.bytes);
    }
}

#[test]
fn deterministic_zip() {
    let course = Course::from_json(HELLO_COURSE_JSON).expect("valid course");
    let no_media = |_src: &str| -> Option<Vec<u8>> { None };

    let input = PackageInput {
        course: &course,
        course_json: HELLO_COURSE_JSON,
        player_files: &fake_player(),
        media: &no_media,
    };

    let files = build_package(&input).expect("package builds");
    let zip1 = zip_package(&files).expect("zip1");
    let zip2 = zip_package(&files).expect("zip2");

    assert_eq!(zip1, zip2, "zips must be byte-identical");
}

#[test]
fn dedupes_identical_media_sources() {
    let mut course = minimal_course("Dedupe");
    course.scenes[0].slides[0].objects.push(Object {
        id: "img1".into(),
        name: "Pic 1".into(),
        kind: ObjectKind::Image {
            crop: None,
            src: "media/logo.png".into(),
            alt: "".into(),
        },
        frame: Rect {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        },
        timing: Timing::default(),
        states: vec![StateDef {
            name: "hover".into(),
            fill: None,
            text: None,
            src: Some("media/logo.png".into()),
        }],
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
    course.scenes[0].slides[0].objects.push(Object {
        id: "img2".into(),
        name: "Pic 2".into(),
        kind: ObjectKind::Image {
            crop: None,
            src: "media/logo.png".into(),
            alt: "".into(),
        },
        frame: Rect {
            x: 10.0,
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

    let resolve_count = std::sync::atomic::AtomicUsize::new(0);
    let media = |src: &str| -> Option<Vec<u8>> {
        if src == "media/logo.png" {
            resolve_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Some(vec![9, 9, 9])
        } else {
            None
        }
    };

    let course_json = course.to_json().expect("serialize");
    let input = PackageInput {
        course: &course,
        course_json: &course_json,
        player_files: &fake_player(),
        media: &media,
    };

    let files = build_package(&input).expect("package builds");
    assert_eq!(
        resolve_count.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "identical media source resolved only once"
    );

    let logo_count = files
        .iter()
        .filter(|f| f.path == "media/media/logo.png")
        .count();
    assert_eq!(logo_count, 1, "packaged exactly once");
}

#[test]
fn skips_remote_http_and_https_sources() {
    let mut course = minimal_course("Remote Media");
    course.scenes[0].slides[0].objects.push(Object {
        id: "audio1".into(),
        name: "Remote Audio".into(),
        kind: ObjectKind::Audio {
            src: "https://example.com/stream.mp3".into(),
            autoplay: false,
            captions: None,
            volume: 1.0,
            trim: None,
        },
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
    course.scenes[0].slides[0].objects.push(Object {
        id: "video1".into(),
        name: "Remote Video".into(),
        kind: ObjectKind::Video {
            src: "http://example.com/video.mp4".into(),
            autoplay: false,
            captions: None,
            volume: 1.0,
            trim: None,
            cues: Vec::new(),
        },
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

    let no_media = |_src: &str| -> Option<Vec<u8>> { None };
    let course_json = course.to_json().expect("serialize");
    let input = PackageInput {
        course: &course,
        course_json: &course_json,
        player_files: &fake_player(),
        media: &no_media,
    };

    let files = build_package(&input).expect("package builds");
    assert!(!files.iter().any(|f| f.path.contains("example.com")));
}

#[test]
fn detects_duplicate_files_in_player() {
    let course = minimal_course("Duplicates");
    let course_json = course.to_json().expect("serialize");
    let no_media = |_src: &str| -> Option<Vec<u8>> { None };

    let player_files = vec![
        PackageFile {
            path: "index.html".into(),
            bytes: b"1".to_vec(),
        },
        PackageFile {
            path: "app.js".into(),
            bytes: b"2".to_vec(),
        },
        PackageFile {
            path: "app.js".into(),
            bytes: b"3".to_vec(),
        },
    ];

    let input = PackageInput {
        course: &course,
        course_json: &course_json,
        player_files: &player_files,
        media: &no_media,
    };

    let err = build_package(&input).expect_err("duplicate in player files");
    assert_eq!(err, PublishError::DuplicateFile("app.js".into()));
}

#[test]
fn distinct_sources_never_collide_in_package() {
    let mut course = minimal_course("No Collide");
    for (id, src) in [("i1", "a.png"), ("i2", "media/a.png")] {
        course.scenes[0].slides[0].objects.push(Object {
            id: id.into(),
            name: id.into(),
            kind: ObjectKind::Image {
                crop: None,
                src: src.into(),
                alt: "".into(),
            },
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
    }
    let media = |src: &str| -> Option<Vec<u8>> {
        match src {
            "a.png" => Some(vec![1]),
            "media/a.png" => Some(vec![2]),
            _ => None,
        }
    };
    let course_json = course.to_json().expect("serialize");
    let input = PackageInput {
        course: &course,
        course_json: &course_json,
        player_files: &fake_player(),
        media: &media,
    };
    let files = build_package(&input).expect("package builds");
    let find = |p: &str| files.iter().find(|f| f.path == p).map(|f| f.bytes.clone());
    assert_eq!(find("media/a.png"), Some(vec![1]));
    assert_eq!(find("media/media/a.png"), Some(vec![2]));
}

#[test]
fn editor_saved_project_roundtrips_to_publish_package() {
    let mut course = minimal_course("Editor Saved");
    course.scenes[0].slides[0].objects.push(Object {
        id: "img1".into(),
        name: "img1".into(),
        kind: ObjectKind::Image {
            crop: None,
            src: "photo.png".into(),
            alt: "A scenic photo".into(),
        },
        frame: Rect {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 100.0,
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

    let media_bytes = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    let editor_media = vec![("photo.png".to_string(), media_bytes.clone())];

    let archive_bytes =
        coursecraft_format::save_project(&course, &editor_media).expect("save project succeeds");

    let opened = coursecraft_format::open_project(&archive_bytes).expect("open project succeeds");
    assert_eq!(opened.course, course);

    let media_lookup = |src: &str| -> Option<Vec<u8>> {
        opened
            .media
            .iter()
            .find(|(s, _)| s == src)
            .map(|(_, b)| b.clone())
    };
    let course_json = opened.course.to_json().expect("serialize course");
    let input = PackageInput {
        course: &opened.course,
        course_json: &course_json,
        player_files: &fake_player(),
        media: &media_lookup,
    };
    let files = build_package(&input).expect("package builds");

    let media_file = files
        .iter()
        .find(|f| f.path == "media/photo.png")
        .expect("media/photo.png packaged");
    assert_eq!(media_file.bytes, media_bytes);
}

#[test]
fn course_with_audio_and_video_packaged_under_media() {
    let mut course = minimal_course("Audio and Video Course");
    course.scenes[0].slides[0].objects.push(Object {
        id: "aud1".into(),
        name: "Audio".into(),
        kind: ObjectKind::Audio {
            src: "sound.wav".into(),
            autoplay: true,
            captions: None,
            volume: 1.0,
            trim: None,
        },
        frame: Rect {
            x: 0.0,
            y: 0.0,
            w: 48.0,
            h: 48.0,
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
    course.scenes[0].slides[0].objects.push(Object {
        id: "vid1".into(),
        name: "Video".into(),
        kind: ObjectKind::Video {
            src: "clip.mp4".into(),
            autoplay: false,
            captions: None,
            volume: 1.0,
            trim: None,
            cues: Vec::new(),
        },
        frame: Rect {
            x: 10.0,
            y: 10.0,
            w: 640.0,
            h: 360.0,
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

    let wav_bytes = vec![b'R', b'I', b'F', b'F', 0, 0, 0, 0];
    let mp4_bytes = vec![0, 0, 0, 0x18, b'f', b't', b'y', b'p'];

    let media = |src: &str| -> Option<Vec<u8>> {
        match src {
            "sound.wav" => Some(wav_bytes.clone()),
            "clip.mp4" => Some(mp4_bytes.clone()),
            _ => None,
        }
    };

    let course_json = course.to_json().expect("serialize course");
    let input = PackageInput {
        course: &course,
        course_json: &course_json,
        player_files: &fake_player(),
        media: &media,
    };

    let files = build_package(&input).expect("package builds");

    let aud_file = files
        .iter()
        .find(|f| f.path == "media/sound.wav")
        .expect("media/sound.wav packaged");
    assert_eq!(aud_file.bytes, wav_bytes);

    let vid_file = files
        .iter()
        .find(|f| f.path == "media/clip.mp4")
        .expect("media/clip.mp4 packaged");
    assert_eq!(vid_file.bytes, mp4_bytes);

    let manifest = files
        .iter()
        .find(|f| f.path == "imsmanifest.xml")
        .expect("manifest present");
    let xml = std::str::from_utf8(&manifest.bytes).expect("utf-8 manifest");
    assert!(xml.contains("<file href=\"media/sound.wav\"/>"));
    assert!(xml.contains("<file href=\"media/clip.mp4\"/>"));
}

#[test]
fn course_with_a_lottie_animation_packages_its_file() {
    let mut course = minimal_course("Lottie Course");
    course.scenes[0].slides[0].objects.push(Object {
        id: "lot1".into(),
        name: "Animation".into(),
        kind: ObjectKind::Lottie {
            src: "motion.lottie".into(),
            loop_playback: false,
            autoplay: true,
            speed: 1.0,
        },
        frame: Rect {
            x: 10.0,
            y: 10.0,
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
    let lottie_bytes = b"PK\x03\x04fake lottie".to_vec();
    let media =
        |src: &str| -> Option<Vec<u8>> { (src == "motion.lottie").then(|| lottie_bytes.clone()) };
    let course_json = course.to_json().expect("serialize course");
    let input = PackageInput {
        course: &course,
        course_json: &course_json,
        player_files: &fake_player(),
        media: &media,
    };
    let files = build_package(&input).expect("package builds");
    let packaged = files
        .iter()
        .find(|f| f.path == "media/motion.lottie")
        .expect("Lottie file packaged");
    assert_eq!(packaged.bytes, lottie_bytes);
    let manifest = files
        .iter()
        .find(|f| f.path == "imsmanifest.xml")
        .expect("manifest present");
    let xml = std::str::from_utf8(&manifest.bytes).expect("utf-8 manifest");
    assert!(xml.contains("<file href=\"media/motion.lottie\"/>"));
}

#[test]
fn scorm12_package_is_unchanged_and_has_no_standard_marker() {
    let course = Course::from_json(HELLO_COURSE_JSON).expect("valid course");
    let no_media = |_src: &str| -> Option<Vec<u8>> { None };
    let input = PackageInput {
        course: &course,
        course_json: HELLO_COURSE_JSON,
        player_files: &fake_player(),
        media: &no_media,
    };

    let legacy = build_package(&input).expect("package builds");
    let explicit = build_package_for(&input, Standard::Scorm12).expect("scorm12 package builds");
    assert_eq!(legacy, explicit, "SCORM 1.2 output must stay unchanged");
    assert!(
        !legacy.iter().any(|f| f.path == PACKAGE_MARKER),
        "1.2 packages must not carry the standard marker"
    );

    let manifest = legacy
        .iter()
        .find(|f| f.path == "imsmanifest.xml")
        .expect("manifest present");
    let xml = std::str::from_utf8(&manifest.bytes).expect("utf-8 manifest");
    assert!(xml.contains("<schemaversion>1.2</schemaversion>"));
    assert!(xml.contains("adlcp:scormtype=\"sco\""));
    assert!(!xml.contains("coursecraft-package.json"));
}

#[test]
fn cmi5_package_has_course_structure_and_marker() {
    let course = Course::from_json(HELLO_COURSE_JSON).expect("valid course");
    let no_media = |_src: &str| -> Option<Vec<u8>> { None };
    let input = PackageInput {
        course: &course,
        course_json: HELLO_COURSE_JSON,
        player_files: &fake_player(),
        media: &no_media,
    };

    let files =
        build_package_standard(&input, PackageStandard::Xapi(Flavor::Cmi5)).expect("cmi5 package");
    assert!(
        !files.iter().any(|f| f.path == "imsmanifest.xml"),
        "cmi5 packages have no imsmanifest"
    );
    let marker = files
        .iter()
        .find(|f| f.path == PACKAGE_MARKER)
        .expect("marker");
    assert_eq!(marker.bytes, b"{\"standard\":\"cmi5\"}".as_slice());

    let cmi5 = files
        .iter()
        .find(|f| f.path == "cmi5.xml")
        .expect("cmi5.xml");
    let xml = std::str::from_utf8(&cmi5.bytes).expect("utf-8");
    assert!(xml.contains("https://w3id.org/xapi/profiles/cmi5/v1/CourseStructure.xsd"));
    assert!(xml.contains("urn:coursecraft:course:hello-course"));
    assert!(xml.contains("urn:coursecraft:au:hello-course"));
    assert!(xml.contains("moveOn=\"Passed\""));
    assert!(xml.contains("masteryScore=\"0.8\""));
    assert!(xml.contains("Workplace Safety Basics"));
}

#[test]
fn xapi_package_has_tincan_xml_and_marker() {
    let course = Course::from_json(HELLO_COURSE_JSON).expect("valid course");
    let no_media = |_src: &str| -> Option<Vec<u8>> { None };
    let input = PackageInput {
        course: &course,
        course_json: HELLO_COURSE_JSON,
        player_files: &fake_player(),
        media: &no_media,
    };

    let files =
        build_package_standard(&input, PackageStandard::Xapi(Flavor::Xapi)).expect("xapi package");
    assert!(
        !files.iter().any(|f| f.path == "imsmanifest.xml"),
        "xAPI packages have no imsmanifest"
    );
    let marker = files
        .iter()
        .find(|f| f.path == PACKAGE_MARKER)
        .expect("marker");
    assert_eq!(marker.bytes, b"{\"standard\":\"xapi\"}".as_slice());

    let tincan = files
        .iter()
        .find(|f| f.path == "tincan.xml")
        .expect("tincan.xml");
    let xml = std::str::from_utf8(&tincan.bytes).expect("utf-8");
    assert!(xml.contains("<tincan xmlns=\"http://projecttincan.com/tincan.xsd\">"));
    assert!(xml.contains("urn:coursecraft:activity:hello-course"));
    assert!(xml.contains("Workplace Safety Basics"));
    assert!(xml.contains("<launch lang=\"en-US\">index.html</launch>"));
}

#[test]
fn lti_package_has_the_marker_no_manifest_and_the_same_media_as_scorm12() {
    let mut course = minimal_course("LTI Course");
    course.scenes[0].slides[0].objects.push(media_object(
        "media1",
        ObjectKind::Audio {
            src: "sound.wav".into(),
            autoplay: false,
            captions: None,
            volume: 1.0,
            trim: None,
        },
    ));
    let media =
        |src: &str| -> Option<Vec<u8>> { (src == "sound.wav").then(|| b"audio bytes".to_vec()) };
    let course_json = course.to_json().expect("serialize");
    let input = PackageInput {
        course: &course,
        course_json: &course_json,
        player_files: &fake_player(),
        media: &media,
    };

    let lti = build_package_standard(&input, PackageStandard::Lti).expect("lti package");
    let scorm = build_package_standard(&input, PackageStandard::Scorm(Standard::Scorm12))
        .expect("scorm package");

    let lti_paths: Vec<&str> = lti.iter().map(|file| file.path.as_str()).collect();
    let scorm_paths: Vec<&str> = scorm.iter().map(|file| file.path.as_str()).collect();

    let marker = lti
        .iter()
        .find(|file| file.path == PACKAGE_MARKER)
        .expect("LTI marker");
    assert_eq!(marker.bytes, b"{\"standard\":\"lti\"}".as_slice());
    assert!(
        !lti_paths.contains(&"imsmanifest.xml"),
        "LTI packages have no imsmanifest: {lti_paths:?}"
    );

    // The same files as SCORM 1.2, plus the marker and minus the manifest.
    let mut expected: Vec<&str> = scorm_paths
        .into_iter()
        .filter(|path| *path != "imsmanifest.xml")
        .collect();
    expected.push(PACKAGE_MARKER);
    expected.sort_unstable();
    assert_eq!(lti_paths, expected);

    let packaged_media = lti
        .iter()
        .find(|file| file.path == "media/sound.wav")
        .expect("media packaged");
    assert_eq!(packaged_media.bytes, b"audio bytes".as_slice());
}

#[test]
fn scorm12_via_the_standard_entry_point_is_byte_identical() {
    let course = Course::from_json(HELLO_COURSE_JSON).expect("valid course");
    let no_media = |_src: &str| -> Option<Vec<u8>> { None };
    let input = PackageInput {
        course: &course,
        course_json: HELLO_COURSE_JSON,
        player_files: &fake_player(),
        media: &no_media,
    };
    let legacy = build_package(&input).expect("legacy package");
    let explicit = build_package_standard(&input, PackageStandard::Scorm(Standard::Scorm12))
        .expect("standard package");
    assert_eq!(legacy, explicit);
}

#[test]
fn scorm2004_package_adds_marker_and_2004_manifest() {
    let course = Course::from_json(HELLO_COURSE_JSON).expect("valid course");
    let no_media = |_src: &str| -> Option<Vec<u8>> { None };
    let input = PackageInput {
        course: &course,
        course_json: HELLO_COURSE_JSON,
        player_files: &fake_player(),
        media: &no_media,
    };

    let files = build_package_for(&input, Standard::Scorm2004).expect("package builds");
    let marker = files
        .iter()
        .find(|f| f.path == PACKAGE_MARKER)
        .expect("standard marker present");
    assert_eq!(marker.bytes, b"{\"standard\":\"scorm2004\"}".as_slice());

    let manifest = files
        .iter()
        .find(|f| f.path == "imsmanifest.xml")
        .expect("manifest present");
    let xml = std::str::from_utf8(&manifest.bytes).expect("utf-8 manifest");
    assert!(xml.contains("<schema>ADL SCORM</schema>"));
    assert!(xml.contains("<schemaversion>2004 4th Edition</schemaversion>"));
    assert!(xml.contains("adlcp:scormType=\"sco\""));
    assert!(xml.contains("<file href=\"coursecraft-package.json\"/>"));
    assert!(!xml.contains("masteryscore"));
    assert!(!xml.contains("sequencing"));
    assert!(!xml.contains("adlcp:scormtype"));
}

fn media_object(id: &str, kind: ObjectKind) -> Object {
    Object {
        id: id.into(),
        name: id.into(),
        kind,
        frame: Rect {
            x: 0.0,
            y: 0.0,
            w: 48.0,
            h: 48.0,
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
    }
}

#[test]
fn missing_caption_file_fails_publish_for_audio_and_video() {
    for kind in [
        ObjectKind::Audio {
            src: "sound.wav".into(),
            autoplay: false,
            captions: Some("sound.vtt".into()),
            volume: 1.0,
            trim: None,
        },
        ObjectKind::Video {
            src: "clip.mp4".into(),
            autoplay: false,
            captions: Some("clip.vtt".into()),
            volume: 1.0,
            trim: None,
            cues: Vec::new(),
        },
    ] {
        let mut course = minimal_course("Missing captions");
        course.scenes[0].slides[0]
            .objects
            .push(media_object("media1", kind));
        let media = |src: &str| -> Option<Vec<u8>> {
            if src.ends_with(".vtt") {
                None
            } else {
                Some(b"media".to_vec())
            }
        };
        let input = PackageInput {
            course: &course,
            course_json: "{}",
            player_files: &fake_player(),
            media: &media,
        };
        let err = build_package(&input).expect_err("missing vtt must fail");
        assert!(matches!(
            err,
            PublishError::MissingMedia { ref src, .. } if src.ends_with(".vtt")
        ));
    }
}

#[test]
fn caption_files_are_packaged_under_media_for_audio_and_video() {
    for (kind, vtt) in [
        (
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
        course.scenes[0].slides[0]
            .objects
            .push(media_object("media1", kind));
        let media = |src: &str| -> Option<Vec<u8>> { Some(format!("bytes:{src}").into_bytes()) };
        let input = PackageInput {
            course: &course,
            course_json: "{}",
            player_files: &fake_player(),
            media: &media,
        };
        let files = build_package(&input).expect("package builds");
        let caption = files
            .iter()
            .find(|f| f.path == format!("media/{vtt}"))
            .expect("caption file packaged");
        assert_eq!(caption.bytes, format!("bytes:{vtt}").into_bytes());
    }
}

#[test]
fn master_media_is_packaged_templates_are_stripped_and_their_media_is_not() {
    let mut course = minimal_course("Masters");
    let image = |id: &str, src: &str| Object {
        id: id.to_owned(),
        name: String::new(),
        kind: ObjectKind::Image {
            crop: None,
            src: src.to_owned(),
            alt: String::new(),
        },
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
    };

    let mut master = course.scenes[0].slides[0].clone();
    master.id = "master1".into();
    master.title = "Master".into();
    master.objects = vec![image("mi", "master.png")];
    course.masters.push(master);
    course.scenes[0].slides[0].master = Some("master1".into());

    let mut template = course.scenes[0].slides[0].clone();
    template.id = "template1".into();
    template.title = "Template".into();
    template.objects = vec![image("ti", "template.png")];
    course.templates.push(template);

    let course_json = course.to_json().expect("serialise");
    assert!(course_json.contains("\"templates\""));
    let media = |src: &str| -> Option<Vec<u8>> {
        match src {
            "master.png" | "template.png" => Some(src.as_bytes().to_vec()),
            _ => None,
        }
    };
    let input = PackageInput {
        course: &course,
        course_json: &course_json,
        player_files: &fake_player(),
        media: &media,
    };
    let files = build_package(&input).expect("package builds");
    let paths: Vec<&str> = files.iter().map(|file| file.path.as_str()).collect();
    assert!(paths.contains(&"media/master.png"), "{paths:?}");
    assert!(!paths.contains(&"media/template.png"), "{paths:?}");

    let published = files
        .iter()
        .find(|file| file.path == "course.json")
        .expect("course.json");
    let published: Course =
        Course::from_json(std::str::from_utf8(&published.bytes).expect("utf8")).expect("parses");
    assert!(published.templates.is_empty());
    assert_eq!(published.masters.len(), 1);
}

#[test]
fn a_course_without_templates_publishes_the_same_course_json_bytes() {
    let course = Course::from_json(HELLO_COURSE_JSON).expect("valid course");
    let no_media = |_src: &str| -> Option<Vec<u8>> { None };
    let input = PackageInput {
        course: &course,
        course_json: HELLO_COURSE_JSON,
        player_files: &fake_player(),
        media: &no_media,
    };
    let files = build_package(&input).expect("package builds");
    let published = files
        .iter()
        .find(|file| file.path == "course.json")
        .expect("course.json");
    assert_eq!(published.bytes, HELLO_COURSE_JSON.as_bytes());
}

#[test]
fn a_picture_shape_fill_is_packaged_as_media() {
    let mut course = minimal_course("Picture Fill Course");
    course.scenes[0].slides[0].objects.push(Object {
        id: "shape1".into(),
        name: "Photo shape".into(),
        kind: ObjectKind::Shape {
            shape: coursecraft_course::ShapeKind::Ellipse,
            fill: coursecraft_course::Color("#D9D9D9".into()),
            fill_style: Some(coursecraft_course::FillStyle::Picture {
                src: "texture.png".into(),
                mode: coursecraft_course::PictureFillMode::Stretch,
            }),
            outline: None,
            text: None,
            adjust: Vec::new(),
        },
        frame: Rect {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 100.0,
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

    let media = |src: &str| -> Option<Vec<u8>> {
        if src == "texture.png" {
            Some(vec![1, 2, 3])
        } else {
            None
        }
    };
    let course_json = course.to_json().expect("serialize");
    let input = PackageInput {
        course: &course,
        course_json: &course_json,
        player_files: &fake_player(),
        media: &media,
    };
    let files = build_package(&input).expect("picture fill packages");
    assert!(
        files.iter().any(|file| file.path == "media/texture.png"),
        "picture fill media is in the package"
    );
}

#[test]
fn media_used_only_on_a_bank_slide_is_packaged() {
    let mut course = minimal_course("Bank media");
    let mut bank_slide = course.scenes[0].slides[0].clone();
    bank_slide.id = "bank-slide".into();
    bank_slide.objects.push(Object {
        id: "bank-img".into(),
        name: "Bank image".into(),
        kind: ObjectKind::Image {
            src: "bank-only.png".into(),
            alt: "Only on a bank slide".into(),
            crop: None,
        },
        frame: Rect {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 100.0,
        },
        timing: Timing::default(),
        entrance: None,
        exit: None,
        motion_paths: Vec::new(),
        states: Vec::<StateDef>::new(),
        initial_state: "normal".into(),
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
    course.banks.push(coursecraft_course::QuestionBank {
        id: "bank1".into(),
        title: "Bank One".into(),
        slides: vec![bank_slide],
    });
    let media =
        |src: &str| -> Option<Vec<u8>> { (src == "bank-only.png").then_some(vec![9, 8, 7]) };
    let course_json = course.to_json().expect("serialize");
    let input = PackageInput {
        course: &course,
        course_json: &course_json,
        player_files: &fake_player(),
        media: &media,
    };
    let files = build_package(&input).expect("bank media packages");
    assert!(
        files.iter().any(|file| file.path == "media/bank-only.png"),
        "media used only on a bank slide is in the package: {:?}",
        files.iter().map(|f| &f.path).collect::<Vec<_>>()
    );
}

#[test]
fn resource_files_are_packaged_and_links_are_not() {
    let mut course = minimal_course("Resources");
    course.resources = vec![
        coursecraft_course::Resource {
            id: "guide".into(),
            title: "Guide".into(),
            target: coursecraft_course::ResourceTarget::File {
                src: "guide.pdf".into(),
            },
        },
        coursecraft_course::Resource {
            id: "site".into(),
            title: "Site".into(),
            target: coursecraft_course::ResourceTarget::Url {
                url: "https://example.com".into(),
            },
        },
    ];
    let player_files = fake_player();
    let course_json = course.to_json().expect("course json");
    let media =
        |src: &str| -> Option<Vec<u8>> { (src == "guide.pdf").then(|| b"%PDF-1.7".to_vec()) };
    let input = PackageInput {
        course: &course,
        course_json: &course_json,
        player_files: &player_files,
        media: &media,
    };
    let files = build_package(&input).expect("package builds");
    let paths: Vec<&str> = files.iter().map(|file| file.path.as_str()).collect();
    assert!(paths.contains(&"media/guide.pdf"), "{paths:?}");

    let missing = |_src: &str| -> Option<Vec<u8>> { None };
    let input = PackageInput {
        media: &missing,
        ..input
    };
    assert_eq!(
        build_package(&input).expect_err("missing resource file must fail"),
        PublishError::MissingMedia {
            object: "guide".into(),
            src: "guide.pdf".into(),
        }
    );
}

#[test]
fn zip_package_stores_compressed_media_and_deflates_text() {
    let files = vec![
        PackageFile {
            path: "index.html".into(),
            bytes: vec![b'a'; 4096],
        },
        PackageFile {
            path: "media/pic.png".into(),
            bytes: vec![1u8; 4096],
        },
        PackageFile {
            path: "media/clip.mp4".into(),
            bytes: vec![2u8; 4096],
        },
    ];
    let bytes = zip_package(&files).expect("zip");
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes.as_slice())).expect("open");
    for (name, method) in [
        ("index.html", zip::CompressionMethod::Deflated),
        ("media/pic.png", zip::CompressionMethod::Stored),
        ("media/clip.mp4", zip::CompressionMethod::Stored),
    ] {
        assert_eq!(
            archive.by_name(name).expect("entry").compression(),
            method,
            "{name}"
        );
    }
}

#[test]
fn zip_package_to_writes_the_same_bytes_as_zip_package() {
    let files = vec![
        PackageFile {
            path: "index.html".into(),
            bytes: b"<html></html>".to_vec(),
        },
        PackageFile {
            path: "media/pic.png".into(),
            bytes: vec![5u8; 777],
        },
    ];
    let mut out = std::io::Cursor::new(Vec::new());
    zip_package_to(&files, &mut out).expect("zip to writer");
    assert_eq!(out.into_inner(), zip_package(&files).expect("zip"));
}
