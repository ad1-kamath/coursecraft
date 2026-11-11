//! Integration tests for coursecraft-cli commands.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use coursecraft_course::{CompletionStatus, Course};
use coursecraft_format::{open_project, save_project};
use coursecraft_logic::{Effect, Input, Player};
use coursecraft_pptx::zip;
use coursecraft_render::{MediaFetch, MediaSource, RenderMode, RenderOptions, render_slide_rgba};

use crate::{Cli, Cmd, StandardArg, run};

const HELLO_JSON: &str = include_str!("../../../examples/hello-course/course.json");
const SHOWCASE_JSON: &str = include_str!("../../../examples/showcase/course.json");
const SHOWCASE_INPUTS: &str = include_str!("../../../examples/showcase/pass.inputs.json");
const HELLO_INPUTS: &str = include_str!("../../../examples/hello-course/pass.inputs.json");

fn scratch_dir(sub: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/agy-scratch")
        .join(sub);
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

// ---------------------------------------------------------------- interactions

fn insert_interaction(kind: &str) -> (coursecraft_engine::Session, Vec<String>, Vec<String>) {
    let mut session = coursecraft_engine::Session::blank("Interaction");
    let result = session
        .execute(
            "interaction.insert",
            &serde_json::json!({ "slide": "slide1", "kind": kind }),
        )
        .expect("interaction.insert");
    let ids = |key: &str| -> Vec<String> {
        result[key]
            .as_array()
            .map(|values| {
                values
                    .iter()
                    .filter_map(|value| value.as_str())
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    };
    (session, ids("objects"), ids("layers"))
}

#[test]
fn simulate_inserted_tabs_shows_clicked_layer_and_states() {
    let (session, objects, layers) = insert_interaction("tabs");
    let mut player = Player::new(&session.course).expect("player");
    let start = player.handle(&Input::Start);
    assert!(start.contains(&Effect::LayerShown {
        layer: layers[0].clone()
    }));

    let effects = player.handle(&Input::Click {
        object: objects[1].clone(),
    });
    assert!(effects.contains(&Effect::LayerShown {
        layer: layers[1].clone()
    }));
    assert!(effects.contains(&Effect::LayerHidden {
        layer: layers[0].clone()
    }));
    assert!(effects.contains(&Effect::StateChanged {
        object: objects[1].clone(),
        state: "selected".into()
    }));
    assert!(effects.contains(&Effect::StateChanged {
        object: objects[0].clone(),
        state: "normal".into()
    }));
}

#[test]
fn simulate_inserted_accordion_moves_headers_and_closes_again() {
    let (session, objects, layers) = insert_interaction("accordion");
    let slide = session.course.slide("slide1").expect("slide");
    let base_second_y = slide
        .objects
        .iter()
        .find(|object| object.name == "Header 2 button")
        .map(|object| object.frame.y)
        .expect("base header 2");
    let layer = slide.layer(&layers[0]).expect("section 1 layer");
    let moved_y = layer
        .objects
        .iter()
        .find(|object| object.name.starts_with("Header 2 button"))
        .map(|object| object.frame.y)
        .expect("moved header 2");
    assert!(moved_y > base_second_y);

    let mut player = Player::new(&session.course).expect("player");
    player.handle(&Input::Start);
    let effects = player.handle(&Input::Click {
        object: objects[0].clone(),
    });
    assert!(effects.contains(&Effect::LayerShown {
        layer: layers[0].clone()
    }));

    let next_header = slide
        .layer(&layers[0])
        .and_then(|layer| {
            layer
                .objects
                .iter()
                .find(|object| object.name.starts_with("Header 2 button"))
        })
        .map(|object| object.id.clone())
        .expect("header copy");
    let effects = player.handle(&Input::Click {
        object: next_header,
    });
    assert!(
        effects.contains(&Effect::LayerShown {
            layer: layers[1].clone()
        }),
        "clicking another header opens its section: {effects:?}"
    );

    let own_header = slide
        .layer(&layers[1])
        .and_then(|layer| {
            layer
                .objects
                .iter()
                .find(|object| object.name.starts_with("Header 2 button"))
        })
        .map(|object| object.id.clone())
        .expect("own header");
    let effects = player.handle(&Input::Click { object: own_header });
    assert!(effects.contains(&Effect::LayerHidden {
        layer: layers[1].clone()
    }));
}

const TINY_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
    0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08, 0xD7, 0x63, 0xF8, 0xCF, 0xC0, 0x00,
    0x00, 0x03, 0x01, 0x01, 0x00, 0x18, 0xDD, 0x8D, 0xB0, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E,
    0x44, 0xAE, 0x42, 0x60, 0x82,
];

fn make_test_pptx(path: &Path) {
    let mut file = std::fs::File::create(path).unwrap();
    let mut zip = zip::ZipWriter::new(&mut file);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

    let pres_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
    <p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
                    xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
                    xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
        <p:sldSz cx="9144000" cy="5143500"/>
        <p:sldIdLst>
            <p:sldId id="256" r:id="rId1"/>
        </p:sldIdLst>
    </p:presentation>"#;
    zip.start_file("ppt/presentation.xml", options).unwrap();
    zip.write_all(pres_xml.as_bytes()).unwrap();

    let pres_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
    <Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
        <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide1.xml"/>
    </Relationships>"#;
    zip.start_file("ppt/_rels/presentation.xml.rels", options)
        .unwrap();
    zip.write_all(pres_rels.as_bytes()).unwrap();

    let slide_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
    <p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
           xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
           xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
        <p:cSld>
            <p:spTree>
                <p:sp>
                    <p:nvSpPr>
                        <p:cNvPr id="2" name="Title 1"/>
                        <p:nvPr><p:ph type="title"/></p:nvPr>
                    </p:nvSpPr>
                    <p:spPr>
                        <a:xfrm><a:off x="100000" y="100000"/><a:ext cx="2000000" cy="500000"/></a:xfrm>
                        <a:prstGeom prst="rect"/>
                    </p:spPr>
                    <p:txBody><a:bodyPr/><a:p><a:r><a:t>Imported Slide Title</a:t></a:r></a:p></p:txBody>
                </p:sp>
                <p:pic>
                    <p:nvPicPr>
                        <p:cNvPr id="3" name="Photo" descr="A photo"/>
                        <p:cNvPicPr/><p:nvPr/>
                    </p:nvPicPr>
                    <p:blipFill><a:blip r:embed="rIdPic"/></p:blipFill>
                    <p:spPr><a:xfrm><a:off x="100000" y="700000"/><a:ext cx="1000000" cy="1000000"/></a:xfrm></p:spPr>
                </p:pic>
            </p:spTree>
        </p:cSld>
    </p:sld>"#;
    zip.start_file("ppt/slides/slide1.xml", options).unwrap();
    zip.write_all(slide_xml.as_bytes()).unwrap();

    let slide_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
    <Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
        <Relationship Id="rIdPic" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/image1.png"/>
    </Relationships>"#;
    zip.start_file("ppt/slides/_rels/slide1.xml.rels", options)
        .unwrap();
    zip.write_all(slide_rels.as_bytes()).unwrap();

    zip.start_file("ppt/media/image1.png", options).unwrap();
    zip.write_all(TINY_PNG).unwrap();

    zip.finish().unwrap();
}

#[test]
fn simulate_inserted_results_slide_passes_fails_retries_and_reviews() {
    use coursecraft_course::Value;

    let dir = scratch_dir("test-cli-results-slide");
    let script = dir.join("results.script.json");
    std::fs::write(
        &script,
        r#"[{"command":"results.insert","params":{"after":"s-quiz"}}]"#,
    )
    .expect("write script");
    let out = dir.join("course.json");
    let code = run(Cli {
        cmd: Cmd::Run {
            course: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../examples/hello-course/course.json"),
            script,
            out: out.clone(),
        },
    })
    .expect("run results.insert script");
    assert_eq!(code, ExitCode::SUCCESS);

    let course = Course::from_json(&std::fs::read_to_string(&out).expect("read course"))
        .expect("parse course");
    let results = course
        .slides()
        .find(|slide| slide.id.starts_with("results"))
        .expect("inserted results slide");
    let results_id = results.id.clone();
    let button = |layer: &str, label: &str| -> String {
        results
            .layer(layer)
            .and_then(|layer| layer.objects.iter().find(|object| object.name == label))
            .map(|object| object.id.clone())
            .expect("results button")
    };
    let finish = button("passed", "Finish");
    let retry = button("failed", "Retry quiz");
    let review = button("failed", "Review quiz");

    let to_quiz = || -> Vec<Input> {
        vec![
            Input::Start,
            Input::Tick { ms: 1000 },
            Input::Click {
                object: "btn-start".into(),
            },
            Input::Click {
                object: "tab1".into(),
            },
            Input::Click {
                object: "tab2".into(),
            },
            Input::Click {
                object: "tab3".into(),
            },
            Input::Click {
                object: "btn-next".into(),
            },
        ]
    };
    let play = |inputs: Vec<Input>| -> (Player, Vec<Effect>) {
        let mut player = Player::new(&course).expect("player");
        let mut effects = Vec::new();
        for input in inputs {
            effects.extend(player.handle(&input));
        }
        (player, effects)
    };

    // Right answer: the Passed layer shows and Finish exits.
    let mut right = to_quiz();
    right.push(Input::SelectChoice { choice: "b".into() });
    right.push(Input::Click {
        object: "btn-submit".into(),
    });
    right.push(Input::Click {
        object: "btn-continue-ok".into(),
    });
    let (mut player, effects) = play(right);
    assert!(
        effects.contains(&Effect::SlideEntered {
            slide: results_id.clone()
        }),
        "right path effects: {effects:?}"
    );
    assert!(effects.contains(&Effect::LayerShown {
        layer: "passed".into()
    }));
    let finished = player.handle(&Input::Click { object: finish });
    assert!(finished.contains(&Effect::Exit));

    // Wrong answer: the Failed layer shows, Retry returns to the question.
    let mut wrong = to_quiz();
    wrong.push(Input::SelectChoice { choice: "a".into() });
    wrong.push(Input::Click {
        object: "btn-submit".into(),
    });
    wrong.push(Input::Click {
        object: "btn-try-again".into(),
    });
    wrong.push(Input::Next);
    let (mut player, effects) = play(wrong.clone());
    assert!(effects.contains(&Effect::SlideEntered {
        slide: results_id.clone()
    }));
    assert!(effects.contains(&Effect::LayerShown {
        layer: "failed".into()
    }));
    let retried = player.handle(&Input::Click { object: retry });
    assert!(retried.contains(&Effect::SlideEntered {
        slide: "s-quiz".into()
    }));
    assert_eq!(player.value("Quiz.Attempts"), Some(Value::Number(1.0)));

    // Review from the Failed layer walks the answered question.
    let (mut player, effects) = play(wrong);
    assert!(effects.contains(&Effect::LayerShown {
        layer: "failed".into()
    }));
    let reviewing = player.handle(&Input::Click { object: review });
    assert!(reviewing.contains(&Effect::ReviewChanged { reviewing: true }));
}

#[test]
fn test_cli_import_pptx_to_ccraft() {
    let dir = scratch_dir("test-cli-import-ccraft");
    let pptx_path = dir.join("presentation.pptx");
    let ccraft_out = dir.join("deck.ccraft");

    make_test_pptx(&pptx_path);

    let code = run(Cli {
        cmd: Cmd::ImportPptx {
            deck: pptx_path,
            out: ccraft_out.clone(),
        },
    })
    .expect("CLI import-pptx to ccraft should succeed");
    assert_eq!(code, ExitCode::SUCCESS);

    assert!(ccraft_out.exists());
    let bytes = std::fs::read(&ccraft_out).expect("read ccraft");
    let project = open_project(&bytes).expect("open project");

    assert_eq!(project.course.title, "presentation");
    assert_eq!(
        project.course.scenes[0].slides[0].title,
        "Imported Slide Title"
    );
    assert_eq!(project.media.len(), 1);
    assert_eq!(project.media[0].0, "image1.png");
}

#[test]
fn test_cli_import_pptx_to_folder() {
    let dir = scratch_dir("test-cli-import-folder");
    let pptx_path = dir.join("deck.pptx");
    let folder_out = dir.join("output_folder");

    make_test_pptx(&pptx_path);

    let code = run(Cli {
        cmd: Cmd::ImportPptx {
            deck: pptx_path,
            out: folder_out.clone(),
        },
    })
    .expect("CLI import-pptx to folder should succeed");
    assert_eq!(code, ExitCode::SUCCESS);

    let course_json_path = folder_out.join("course.json");
    assert!(course_json_path.exists());
    let course_text = std::fs::read_to_string(&course_json_path).expect("read course.json");
    let course = Course::from_json(&course_text).expect("parse course");

    assert_eq!(course.title, "deck");
    assert_eq!(course.scenes[0].slides[0].title, "Imported Slide Title");

    let media_file = folder_out.join("media").join("image1.png");
    assert!(media_file.exists());
    let img_bytes = std::fs::read(media_file).expect("read media file");
    assert_eq!(img_bytes, TINY_PNG);
}

#[test]
fn test_cli_editor_no_running_server() {
    let dir = scratch_dir("test-cli-editor-none");
    let code = run(Cli {
        cmd: Cmd::Editor {
            method: "session.info".into(),
            params: "{}".into(),
            config_dir: Some(dir),
        },
    })
    .expect("should exit with code");
    assert_eq!(code, ExitCode::FAILURE);
}

#[test]
fn test_cli_editor_round_trip() {
    let dir = scratch_dir("test-cli-editor-server");
    let server = coursecraft_control::ControlServer::start(Some(&dir)).expect("start server");

    let handle = std::thread::spawn(move || {
        let code = run(Cli {
            cmd: Cmd::Editor {
                method: "session.info".into(),
                params: "{}".into(),
                config_dir: Some(dir),
            },
        })
        .expect("run editor cmd");
        assert_eq!(code, ExitCode::SUCCESS);
    });

    let mut host = coursecraft_control::HeadlessHost::new(
        coursecraft_engine::Session::blank("CLI Editor Test"),
        None,
    );

    let mut polled = 0;
    while polled < 1 {
        polled += server.poll(&mut host);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    handle.join().expect("cli thread finished");
}

#[test]
fn test_cli_render_hello_course_slide() {
    let scratch = scratch_dir("test-cli-render");
    let out_png = scratch.join("intro.png");
    let course_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-course/course.json");

    let start = std::time::Instant::now();
    let code = run(Cli {
        cmd: Cmd::Render {
            course: course_path,
            slide: "s-intro".to_string(),
            width: 1280,
            mode: "authoring".to_string(),
            layer: Vec::new(),
            at_ms: None,
            out: out_png.clone(),
        },
    })
    .expect("run render cmd");
    let elapsed = start.elapsed();
    println!("hello-course s-intro rendered in {:?}", elapsed);

    assert_eq!(code, ExitCode::SUCCESS);
    assert!(out_png.exists());
    let bytes = std::fs::read(&out_png).expect("read rendered png");
    assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
}

#[test]
fn test_cli_showcase_validates_clean() {
    let course_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/showcase/course.json");
    let code = run(Cli {
        cmd: Cmd::Validate {
            course: course_path.clone(),
        },
    })
    .expect("run validate cmd");
    assert_eq!(code, ExitCode::SUCCESS);

    let text = std::fs::read_to_string(&course_path).expect("read showcase course.json");
    let course = Course::from_json(&text).expect("parse showcase course");
    let issues = course.validate();
    assert!(issues.is_empty(), "showcase validation issues: {issues:?}");
}

#[test]
fn test_cli_showcase_simulate_pass() {
    let course_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/showcase/course.json");
    let inputs_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/showcase/pass.inputs.json");

    let code = run(Cli {
        cmd: Cmd::Simulate {
            course: course_path.clone(),
            inputs: inputs_path.clone(),
            seed: 0,
        },
    })
    .expect("run simulate cmd");
    assert_eq!(code, ExitCode::SUCCESS);

    let course_text = std::fs::read_to_string(&course_path).expect("read course");
    let course = Course::from_json(&course_text).expect("parse course");
    let inputs_text = std::fs::read_to_string(&inputs_path).expect("read inputs");
    let inputs: Vec<Input> = serde_json::from_str(&inputs_text).expect("parse inputs");

    let mut player = Player::new(&course).expect("create player");
    let mut effects = Vec::new();
    for input in &inputs {
        effects.extend(player.handle(input));
    }

    assert!(
        effects.contains(&Effect::Score {
            raw: 80,
            max: 80,
            percent: 100,
            status: CompletionStatus::Passed,
        }),
        "expected Score effect with 100% and Passed"
    );
    assert!(
        effects.contains(&Effect::Completion {
            status: CompletionStatus::Passed,
        }),
        "expected Completion effect with Passed"
    );
}

#[test]
fn test_cli_showcase_simulate_fail() {
    let course_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/showcase/course.json");
    let inputs_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/showcase/fail.inputs.json");

    let code = run(Cli {
        cmd: Cmd::Simulate {
            course: course_path.clone(),
            inputs: inputs_path.clone(),
            seed: 0,
        },
    })
    .expect("run simulate cmd");
    assert_eq!(code, ExitCode::SUCCESS);

    let course_text = std::fs::read_to_string(&course_path).expect("read course");
    let course = Course::from_json(&course_text).expect("parse course");
    let inputs_text = std::fs::read_to_string(&inputs_path).expect("read inputs");
    let inputs: Vec<Input> = serde_json::from_str(&inputs_text).expect("parse inputs");

    let mut player = Player::new(&course).expect("create player");
    let mut effects = Vec::new();
    for input in &inputs {
        effects.extend(player.handle(input));
    }

    assert!(
        effects.contains(&Effect::Score {
            raw: 0,
            max: 80,
            percent: 0,
            status: CompletionStatus::Failed,
        }),
        "expected Score effect with 0% and Failed"
    );
    assert!(
        effects.contains(&Effect::Completion {
            status: CompletionStatus::Failed,
        }),
        "expected Completion effect with Failed"
    );
}

#[test]
fn simulate_seed_changes_the_picks_and_is_repeatable() {
    fn question_slide(id: &str) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "title": id,
            "question": {
                "kind": "multiple_choice",
                "prompt": "Pick one",
                "choices": [
                    { "id": "a", "text": "A" },
                    { "id": "b", "text": "B", "correct": true }
                ],
                "attempts": 1
            },
            "triggers": [{
                "action": { "action": "submit_question" },
                "when": { "event": "submit" }
            }]
        })
    }
    let bank_slides: Vec<serde_json::Value> =
        (1..=5).map(|i| question_slide(&format!("bq{i}"))).collect();
    let course_json = serde_json::json!({
        "schema_version": 1,
        "id": "draw-seed",
        "title": "Draw Seed",
        "scenes": [{ "id": "sc", "title": "Scene", "slides": [
            { "id": "s-intro", "title": "Intro" },
            { "id": "s-draw", "title": "Draw", "draw": { "bank": "bank1", "count": 2 } },
            { "id": "s-end", "title": "End" }
        ]}],
        "banks": [{ "id": "bank1", "title": "Bank One", "slides": bank_slides }]
    });
    let scratch = scratch_dir("test-cli-simulate-seed");
    let course_path = scratch.join("course.json");
    let inputs_path = scratch.join("inputs.json");
    std::fs::write(&course_path, course_json.to_string()).expect("write course");
    std::fs::write(&inputs_path, r#"[{"input": "start"}]"#).expect("write inputs");

    for seed in [1u32, 1, 2] {
        let code = run(Cli {
            cmd: Cmd::Simulate {
                course: course_path.clone(),
                inputs: inputs_path.clone(),
                seed,
            },
        })
        .expect("run simulate cmd");
        assert_eq!(code, ExitCode::SUCCESS, "seed {seed}");
    }

    let course = Course::from_json(&course_json.to_string()).expect("valid course");
    let picks = |seed: u32| {
        let player = Player::with_draw_seed(&course, seed).expect("player");
        player
            .play_order()
            .iter()
            .map(|slide| slide.id.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(picks(1), picks(1), "the same seed is repeatable");
    // Default seed 0 is what a seedless simulate uses.
    assert_eq!(picks(0), picks(0));
    let mut differs = false;
    for seed in 0..20u32 {
        if picks(seed) != picks(1) {
            differs = true;
            break;
        }
    }
    assert!(differs, "some seed in 0..20 picks differently than seed 1");
}

#[test]
fn test_cli_showcase_publish_includes_media_and_manifest() {
    let scratch = scratch_dir("test-cli-publish-showcase");
    let course_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/showcase/course.json");
    let player_dir = scratch.join("fake-player");
    std::fs::create_dir_all(&player_dir).expect("create fake player dir");
    std::fs::write(
        player_dir.join("index.html"),
        "<!doctype html><html></html>",
    )
    .expect("write fake index.html");

    let out_dir = scratch.join("package");
    let zip_out = scratch.join("showcase.zip");

    let code = run(Cli {
        cmd: Cmd::Publish {
            course: course_path,
            player: player_dir,
            out: Some(zip_out.clone()),
            out_dir: Some(out_dir.clone()),
            standard: StandardArg::Scorm12,
        },
    })
    .expect("run publish cmd");
    assert_eq!(code, ExitCode::SUCCESS);

    assert!(out_dir.join("media/banner.png").exists());
    assert!(out_dir.join("media/floorplan.png").exists());
    assert!(out_dir.join("media/chime.wav").exists());
    assert!(out_dir.join("media/intro.mp4").exists());
    assert!(out_dir.join("imsmanifest.xml").exists());

    let manifest_xml =
        std::fs::read_to_string(out_dir.join("imsmanifest.xml")).expect("read manifest");
    assert!(manifest_xml.contains("media/banner.png"));
    assert!(manifest_xml.contains("media/floorplan.png"));
    assert!(manifest_xml.contains("media/chime.wav"));
    assert!(manifest_xml.contains("media/intro.mp4"));

    assert!(zip_out.exists());
}

#[test]
fn test_cli_publish_lti_has_marker_and_no_manifest() {
    let scratch = scratch_dir("test-cli-publish-lti");
    let course_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/showcase/course.json");
    let player_dir = scratch.join("fake-player");
    std::fs::create_dir_all(&player_dir).expect("create fake player dir");
    std::fs::write(
        player_dir.join("index.html"),
        "<!doctype html><html></html>",
    )
    .expect("write fake index.html");

    let out_dir = scratch.join("package");
    let zip_out = scratch.join("showcase-lti.zip");

    let code = run(Cli {
        cmd: Cmd::Publish {
            course: course_path,
            player: player_dir,
            out: Some(zip_out.clone()),
            out_dir: Some(out_dir.clone()),
            standard: StandardArg::Lti,
        },
    })
    .expect("run publish lti");
    assert_eq!(code, ExitCode::SUCCESS);

    let marker = std::fs::read_to_string(out_dir.join("coursecraft-package.json"))
        .expect("read package marker");
    assert_eq!(marker, r#"{"standard":"lti"}"#);
    assert!(
        !out_dir.join("imsmanifest.xml").exists(),
        "LTI packages have no imsmanifest.xml"
    );
    assert!(out_dir.join("media/banner.png").exists());
    assert!(out_dir.join("media/chime.wav").exists());
    assert!(zip_out.exists());
}

struct LocalMediaSource {
    dir: PathBuf,
}

impl MediaSource for LocalMediaSource {
    fn get(&self, src: &str) -> MediaFetch {
        match std::fs::read(self.dir.join(src)) {
            Ok(bytes) => MediaFetch::Ready(bytes.into()),
            Err(_) => MediaFetch::Missing,
        }
    }
}

#[test]
fn test_showcase_render_smoke_test() {
    let course_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/showcase/course.json");
    let course_text = std::fs::read_to_string(&course_path).expect("read showcase course");
    let course = Course::from_json(&course_text).expect("parse course");
    let media = LocalMediaSource {
        dir: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/showcase/media"),
    };

    let opts = RenderOptions {
        width: 1280,
        mode: RenderMode::Authoring,
        layers: Vec::new(),
        at_ms: None,
    };

    let slides: Vec<_> = course.slides().collect();
    assert!(!slides.is_empty(), "showcase should have slides");
    for slide in &slides {
        let img = render_slide_rgba(&course, &slide.id, &opts, &media)
            .unwrap_or_else(|e| panic!("rendering slide {:?} failed: {e}", slide.id));
        assert!(img.width > 0, "slide {:?} width > 0", slide.id);
        assert!(img.height > 0, "slide {:?} height > 0", slide.id);
        assert_eq!(
            img.pixels.len(),
            (img.width * img.height * 4) as usize,
            "slide {:?} rgba pixel buffer length",
            slide.id
        );
    }
}

const A11Y_CLEAN: &str = r#"{
  "schema_version": 1,
  "id": "clean",
  "title": "A clean course",
  "scenes": [{
    "id": "scene",
    "title": "Scene",
    "slides": [{ "id": "slide", "title": "A titled slide" }]
  }]
}"#;

const A11Y_BAD: &str = r#"{
  "schema_version": 1,
  "id": "bad",
  "title": "",
  "scenes": [{
    "id": "scene",
    "title": "Scene",
    "slides": [{
      "id": "slide",
      "title": "A titled slide",
      "objects": [{
        "id": "photo",
        "kind": { "type": "image", "src": "photo.png" },
        "frame": { "x": 0, "y": 0, "w": 200, "h": 200 }
      }]
    }]
  }]
}"#;

#[test]
fn cli_a11y_check_exit_codes_and_json_flag() {
    let dir = scratch_dir("test-cli-a11y");
    let clean = dir.join("clean.json");
    std::fs::write(&clean, A11Y_CLEAN).unwrap();
    let bad = dir.join("bad.json");
    std::fs::write(&bad, A11Y_BAD).unwrap();

    for json in [false, true] {
        let code = run(Cli {
            cmd: Cmd::A11yCheck {
                course: clean.clone(),
                json,
            },
        })
        .expect("clean course loads");
        assert_eq!(code, ExitCode::SUCCESS);

        let code = run(Cli {
            cmd: Cmd::A11yCheck {
                course: bad.clone(),
                json,
            },
        })
        .expect("bad course loads");
        assert_eq!(code, ExitCode::FAILURE);
    }

    assert!(
        run(Cli {
            cmd: Cmd::A11yCheck {
                course: dir.join("missing.json"),
                json: false,
            },
        })
        .is_err(),
        "a missing file is a load failure, not an audit error"
    );
}

#[test]
fn cli_a11y_check_accepts_a_ccraft_archive() {
    let dir = scratch_dir("test-cli-a11y-ccraft");
    let course = Course::from_json(A11Y_BAD).unwrap();
    let bytes = save_project(&course, &[]).expect("save project");
    let path = dir.join("bad.ccraft");
    std::fs::write(&path, bytes).unwrap();

    let code = run(Cli {
        cmd: Cmd::A11yCheck {
            course: path,
            json: true,
        },
    })
    .expect("ccraft loads");
    assert_eq!(code, ExitCode::FAILURE);
}

#[test]
fn cli_a11y_json_and_text_are_honest() {
    let course = Course::from_json(A11Y_BAD).unwrap();
    let report = course.audit();

    let json = crate::audit_json(&report).expect("serialise report");
    let value: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    assert!(value["findings"].as_array().is_some_and(|a| !a.is_empty()));
    assert_eq!(value["manual_checks"].as_array().map(Vec::len), Some(7));

    let text = crate::audit_text(&course, &report);
    assert!(text.contains("Automated checks only. These still need a person:"));
    assert!(text.contains("WCAG 2.4.2"));
    assert!(text.contains("fix:"));
    let lower = text.to_ascii_lowercase();
    assert!(!lower.contains("compliant"));
    assert!(!lower.contains("passes wcag"));
    assert!(!lower.contains("accessible course"));
}

fn hello_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-course/course.json")
}

fn translated_hello_xliff() -> &'static str {
    r#"<?xml version="1.0" encoding="UTF-8"?>
<xliff version="1.2" xmlns="urn:oasis:names:tc:xliff:document:1.2">
  <file original="course.json" datatype="plaintext" source-language="en" target-language="fr">
    <body>
      <trans-unit id="course/title" xml:space="preserve"><source>Workplace Safety Basics</source><target>Les bases de la sécurité au travail</target></trans-unit>
      <trans-unit id="slide/s-intro/title" xml:space="preserve"><source>Welcome</source><target>Bienvenue</target></trans-unit>
      <trans-unit id="slide/s-intro/object/btn-start/label" xml:space="preserve"><source>Start</source><target>Commencer</target></trans-unit>
    </body>
  </file>
</xliff>"#
}

#[test]
fn cli_export_xliff_writes_ids_and_leaves_the_input_untouched() {
    let dir = scratch_dir("cli-export-xliff");
    let course = hello_path();
    let before = std::fs::read_to_string(&course).expect("read input");
    let out = dir.join("hello.xlf");
    let code = run(Cli {
        cmd: Cmd::ExportXliff {
            course: course.clone(),
            target_language: "fr".to_owned(),
            out: out.clone(),
        },
    })
    .expect("export xliff");
    assert_eq!(code, ExitCode::SUCCESS);
    assert_eq!(
        std::fs::read_to_string(&course).expect("read input after"),
        before,
        "input must not change"
    );
    let text = std::fs::read_to_string(&out).expect("read xlf");
    assert!(text.contains(r#"<trans-unit id="course/title""#));
    assert!(text.contains(r#"<trans-unit id="slide/s-intro/object/btn-start/label""#));
    assert!(text.contains(r#"target-language="fr""#));
}

#[test]
fn cli_import_xliff_writes_a_copy_and_sets_the_language() {
    let dir = scratch_dir("cli-import-xliff");
    let course = hello_path();
    let before = std::fs::read_to_string(&course).expect("read input");
    let xliff = dir.join("fr.xlf");
    std::fs::write(&xliff, translated_hello_xliff()).expect("write xlf");
    let out = dir.join("translated.json");
    let code = run(Cli {
        cmd: Cmd::ImportXliff {
            course: course.clone(),
            xliff,
            out: out.clone(),
        },
    })
    .expect("import xliff");
    assert_eq!(code, ExitCode::SUCCESS);
    assert_eq!(
        std::fs::read_to_string(&course).expect("read input after"),
        before,
        "input must not change"
    );
    let translated = Course::from_json(&std::fs::read_to_string(&out).expect("read output"))
        .expect("valid translated course");
    assert_eq!(translated.language.as_deref(), Some("fr"));
    assert_eq!(translated.title, "Les bases de la sécurité au travail");
    assert_eq!(
        translated
            .slide("s-intro")
            .map(|slide| slide.title.as_str()),
        Some("Bienvenue")
    );
    assert_eq!(
        translated
            .slide("s-intro")
            .and_then(|slide| slide.objects.iter().find(|o| o.id == "btn-start"))
            .and_then(|object| match &object.kind {
                coursecraft_course::ObjectKind::Button { label, .. } => Some(label.as_str()),
                _ => None,
            }),
        Some("Commencer")
    );
}

#[test]
fn cli_import_xliff_exits_one_when_a_source_changed() {
    let dir = scratch_dir("cli-import-xliff-stale");
    let course = hello_path();
    let xliff = dir.join("stale.xlf");
    std::fs::write(
        &xliff,
        r#"<?xml version="1.0" encoding="UTF-8"?>
<xliff version="1.2" xmlns="urn:oasis:names:tc:xliff:document:1.2">
  <file original="course.json" datatype="plaintext" source-language="en" target-language="fr">
    <body>
      <trans-unit id="course/title"><source>Old title</source><target>Vieux titre</target></trans-unit>
    </body>
  </file>
</xliff>"#,
    )
    .expect("write xlf");
    let out = dir.join("translated.json");
    let code = run(Cli {
        cmd: Cmd::ImportXliff { course, xliff, out },
    })
    .expect("import runs");
    assert_eq!(code, ExitCode::from(1));
}

#[test]
fn cli_import_xliff_keeps_media_in_a_ccraft() {
    let dir = scratch_dir("cli-import-xliff-ccraft");
    let course = Course::from_json(HELLO_JSON).expect("valid course");
    let media = vec![("pic.png".to_owned(), vec![1u8, 2, 3, 4])];
    let ccraft_bytes = save_project(&course, &media).expect("save project");
    let input = dir.join("input.ccraft");
    std::fs::write(&input, &ccraft_bytes).expect("write ccraft");
    let xliff = dir.join("fr.xlf");
    std::fs::write(&xliff, translated_hello_xliff()).expect("write xlf");
    let out = dir.join("output.ccraft");
    let code = run(Cli {
        cmd: Cmd::ImportXliff {
            course: input.clone(),
            xliff,
            out: out.clone(),
        },
    })
    .expect("import ccraft");
    assert_eq!(code, ExitCode::SUCCESS);
    assert_eq!(std::fs::read(&input).expect("input bytes"), ccraft_bytes);
    let project = open_project(&std::fs::read(&out).expect("read output")).expect("open output");
    assert_eq!(project.course.title, "Les bases de la sécurité au travail");
    assert_eq!(project.media, media);
}

fn pseudo(text: &str) -> String {
    format!("[xx]{text}[/xx]")
}

#[test]
fn pseudo_translated_showcase_simulates_the_same_way() {
    let course = Course::from_json(SHOWCASE_JSON).expect("valid showcase");
    let xml = coursecraft_format::xliff::export_xliff(&course, "fr").expect("export");
    let mut doc = coursecraft_format::xliff::parse_xliff(xml.as_bytes()).expect("parse");
    for unit in &mut doc.units {
        unit.target = Some(coursecraft_format::xliff::ParsedText {
            segments: unit
                .source
                .segments
                .iter()
                .map(|segment| coursecraft_format::xliff::TextSegment {
                    text: pseudo(&segment.text),
                    run: segment.run,
                })
                .collect(),
        });
    }
    let (translated, report) = coursecraft_format::xliff::apply_xliff(&course, &doc);
    assert_eq!(report.applied, doc.units.len());
    assert!(
        translated
            .validate()
            .iter()
            .all(|issue| issue.severity != coursecraft_course::Severity::Error)
    );

    let accepted = translated
        .slides()
        .find_map(|slide| {
            slide
                .question
                .as_ref()
                .filter(|q| q.kind == coursecraft_course::QuestionKind::FillIn)
                .and_then(|q| q.accepted.first())
                .cloned()
        })
        .expect("showcase has a fill-in answer");
    let inputs: Vec<Input> = serde_json::from_str(SHOWCASE_INPUTS).expect("inputs");
    let mut translated_inputs = inputs.clone();
    for input in &mut translated_inputs {
        if let Input::TextEntered { text } = input {
            *text = accepted.clone();
        }
    }

    let effects = |model: &Course, inputs: &[Input]| -> Vec<Effect> {
        let mut player = Player::new(model).expect("valid player");
        let mut out = Vec::new();
        for input in inputs {
            out.extend(player.handle(input));
        }
        out
    };
    let before = effects(&course, &inputs);
    let after = effects(&translated, &translated_inputs);
    let normalise = |list: Vec<Effect>| -> Vec<Effect> {
        list.into_iter()
            .map(|effect| match effect {
                Effect::TextEntryChanged { .. } => Effect::TextEntryChanged {
                    text: String::new(),
                },
                other => other,
            })
            .collect()
    };
    assert_eq!(normalise(before), normalise(after));
}

#[test]
fn a_filled_hello_course_simulates_the_same_way() {
    let course = Course::from_json(HELLO_JSON).expect("valid hello-course");
    let units = coursecraft_format::xliff::translation_units(&course, "fr");
    let targets: std::collections::BTreeMap<String, String> = units
        .into_iter()
        .map(|unit| (unit.id, format!("[fr] {}", unit.source)))
        .collect();
    let xml = coursecraft_format::xliff::fill_xliff(&course, "fr", &targets).expect("fill");
    let doc = coursecraft_format::xliff::parse_xliff(xml.as_bytes()).expect("parse filled");
    let (translated, report) = coursecraft_format::xliff::apply_xliff(&course, &doc);
    assert_eq!(report.applied, targets.len());
    assert!(report.source_changed.is_empty());

    let inputs: Vec<Input> = serde_json::from_str(HELLO_INPUTS).expect("inputs");
    let effects = |model: &Course| -> Vec<Effect> {
        let mut player = Player::new(model).expect("valid player");
        let mut out = Vec::new();
        for input in &inputs {
            out.extend(player.handle(input));
        }
        out
    };
    let before = effects(&course);
    let after = effects(&translated);
    let normalise = |list: Vec<Effect>| -> Vec<Effect> {
        list.into_iter()
            .map(|effect| match effect {
                Effect::TextEntryChanged { .. } => Effect::TextEntryChanged {
                    text: String::new(),
                },
                other => other,
            })
            .collect()
    };
    assert_eq!(normalise(before), normalise(after));
}

/// `docs/cli.md` is what agents read, so every engine command must be listed there.
#[test]
fn cli_guide_lists_every_engine_command() {
    let guide = include_str!("../../../docs/cli.md");
    let missing: Vec<&str> = coursecraft_engine::registry()
        .iter()
        .map(|c| c.id)
        .filter(|id| {
            !guide
                .lines()
                .any(|l| l.split_whitespace().next() == Some(*id))
        })
        .collect();
    assert!(missing.is_empty(), "add to docs/cli.md: {missing:?}");
}

#[test]
fn simulate_accepts_the_new_interaction_inputs_and_reports_their_effects() {
    let course = Course::from_json(
        &serde_json::json!({
            "schema_version": 1,
            "id": "new-inputs",
            "title": "New inputs",
            "variables": [
                { "name": "key_hits", "value": 0.0 },
                { "name": "drops", "value": 0.0 }
            ],
            "scenes": [{
                "id": "sc",
                "title": "Scene",
                "slides": [{
                    "id": "s1",
                    "title": "One",
                    "objects": [
                        { "id": "card", "kind": { "type": "shape", "shape": "rectangle", "fill": "#112233" },
                          "frame": { "x": 0.0, "y": 0.0, "w": 40.0, "h": 40.0 },
                          "drag": { "targets": ["bin"] } },
                        { "id": "bin", "kind": { "type": "shape", "shape": "rectangle", "fill": "#334455" },
                          "frame": { "x": 200.0, "y": 0.0, "w": 80.0, "h": 80.0 } }
                    ],
                    "triggers": [
                        { "action": { "action": "set_variable", "variable": "key_hits", "value": 1 },
                          "when": { "event": "key_pressed", "key": "a" } },
                        { "action": { "action": "set_variable", "variable": "drops", "value": 1 },
                          "when": { "event": "dropped_on", "object": "card", "target": "bin" } }
                    ]
                }]
            }]
        })
        .to_string(),
    )
    .expect("valid course");
    let inputs: Vec<Input> = serde_json::from_str(
        r#"[
            { "input": "start" },
            { "input": "focus_changed", "object": null },
            { "input": "key_pressed", "key": "a" },
            { "input": "object_dropped", "object": "card", "x": 210.0, "y": 10.0 }
        ]"#,
    )
    .expect("the new inputs parse");
    let mut player = Player::new(&course).expect("player");
    let mut effects = Vec::new();
    for input in &inputs {
        effects.extend(player.handle(input));
    }
    assert_eq!(
        player.variable("key_hits"),
        Some(&coursecraft_course::Value::Number(1.0))
    );
    assert_eq!(
        player.variable("drops"),
        Some(&coursecraft_course::Value::Number(1.0))
    );
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::ObjectDropped { object, target }
            if object == "card" && target == "bin"
    )));
}

#[test]
fn simulate_drives_timers_with_clock_ticks_and_prints_their_effects() {
    let course = Course::from_json(
        &serde_json::json!({
            "schema_version": 1,
            "id": "timer-simulate",
            "title": "Timer simulate",
            "variables": [{ "name": "ended", "value": 0.0 }],
            "scenes": [{
                "id": "sc",
                "title": "Scene",
                "slides": [{
                    "id": "s1",
                    "title": "One",
                    "duration_ms": 60000,
                    "objects": [
                        { "id": "t1", "kind": { "type": "timer", "duration_ms": 3000 },
                          "frame": { "x": 0.0, "y": 0.0, "w": 200.0, "h": 60.0 } }
                    ],
                    "triggers": [
                        { "action": { "action": "set_variable", "variable": "ended", "value": 1 },
                          "when": { "event": "timer_ends", "timer": "t1" } }
                    ]
                }]
            }]
        })
        .to_string(),
    )
    .expect("valid course");
    // The same inputs a `simulate` script drives: start, then clock ticks.
    let inputs: Vec<Input> = serde_json::from_str(
        r#"[
            { "input": "start" },
            { "input": "tick", "ms": 1000 },
            { "input": "tick", "ms": 1000 },
            { "input": "tick", "ms": 2000 }
        ]"#,
    )
    .expect("tick inputs parse");
    let mut player = Player::new(&course).expect("player");
    let mut effects = Vec::new();
    for input in &inputs {
        effects.extend(player.handle(input));
    }
    // `simulate` prints these effects as JSON like any other effect.
    let printed = serde_json::to_value(&effects).expect("effects serialise");
    let names: Vec<&str> = printed
        .as_array()
        .expect("effects array")
        .iter()
        .filter_map(|effect| effect["effect"].as_str())
        .collect();
    assert!(names.contains(&"timer_started"), "{printed}");
    assert!(names.contains(&"timer_ended"), "{printed}");
    assert_eq!(
        player.variable("ended"),
        Some(&coursecraft_course::Value::Number(1.0))
    );
    assert_eq!(player.timer_remaining_ms("t1"), Some(0));
}

#[test]
fn simulate_an_inserted_question_cue_pauses_shows_the_layer_and_resumes() {
    let mut session = coursecraft_engine::Session::blank("Cue question");
    let video = session
        .execute(
            "object.add",
            &serde_json::json!({
                "slide": "slide1",
                "kind": { "type": "video", "src": "clip.mp4", "autoplay": true },
                "frame": { "x": 40.0, "y": 40.0, "w": 480.0, "h": 270.0 }
            }),
        )
        .expect("object.add")["id"]
        .as_str()
        .expect("video id")
        .to_owned();
    let result = session
        .execute(
            "video.add_question_cue",
            &serde_json::json!({ "slide": "slide1", "object": video, "at_ms": 1000 }),
        )
        .expect("video.add_question_cue");
    let cue = result["cue"].as_str().expect("cue").to_owned();
    let layer = result["layer"].as_str().expect("layer").to_owned();
    let continue_id = result["continue"].as_str().expect("continue").to_owned();

    let mut player = Player::new(&session.course).expect("player");
    let start = player.handle(&Input::Start);
    assert!(start.iter().any(|effect| matches!(
        effect,
        Effect::Media {
            object,
            op: coursecraft_logic::MediaOp::Play
        } if object == &video
    )));

    let at_cue = player.handle(&Input::MediaTime {
        object: video.clone(),
        at_ms: 1000.0,
    });
    assert!(at_cue.iter().any(|effect| matches!(
        effect,
        Effect::CueReached { object, cue: c } if object == &video && c == &cue
    )));
    assert!(at_cue.iter().any(|effect| matches!(
        effect,
        Effect::Media {
            object,
            op: coursecraft_logic::MediaOp::Pause
        } if object == &video
    )));
    assert!(at_cue.iter().any(|effect| matches!(
        effect,
        Effect::LayerShown { layer: l } if l == &layer
    )));
    assert_eq!(player.state().visible_layers, vec![layer.clone()]);

    let resumed = player.handle(&Input::Click {
        object: continue_id,
    });
    assert!(
        resumed
            .iter()
            .any(|effect| matches!(effect, Effect::LayerHidden { layer: l } if l == &layer))
    );
    assert!(resumed.iter().any(|effect| matches!(
        effect,
        Effect::Media {
            object,
            op: coursecraft_logic::MediaOp::Play
        } if object == &video
    )));
    assert!(player.state().visible_layers.is_empty());
    assert!(player.is_media_playing(&video));
}

#[test]
fn export_mp4_parses_its_arguments() {
    use clap::Parser as _;

    let cli = Cli::parse_from([
        "coursecraft-cli",
        "export-mp4",
        "examples/hello-course",
        "-o",
        "out.mp4",
        "--width",
        "1920",
        "--fps",
        "24",
    ]);
    match cli.cmd {
        Cmd::ExportMp4 {
            course,
            out,
            width,
            fps,
        } => {
            assert_eq!(course, PathBuf::from("examples/hello-course"));
            assert_eq!(out, PathBuf::from("out.mp4"));
            assert_eq!(width, 1920);
            assert_eq!(fps, 24);
        }
        _ => panic!("expected export-mp4"),
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn export_mp4_writes_a_video_and_prints_a_report() {
    if !cfg!(target_os = "macos") {
        return;
    }
    let dir = scratch_dir("test-cli-export-mp4");
    let out = dir.join("hello.mp4");
    let course = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-course");
    let (code, json) = crate::run_export_mp4(course, out.clone(), 640, 10).expect("run export-mp4");
    assert_eq!(code, ExitCode::SUCCESS);
    assert!(out.exists(), "the video is written");

    let json = json.expect("a report on success");
    let report: serde_json::Value = serde_json::from_str(&json).expect("valid report JSON");
    assert!(report["duration_ms"].is_u64(), "{report}");
    assert!(report["frames"].is_u64(), "{report}");
    assert!(report["skipped_questions"].is_array(), "{report}");
    assert!(report["branch_slides"].is_array(), "{report}");
    assert!(report["warnings"].is_array(), "{report}");
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn export_mp4_bad_output_directory_exits_one() {
    let dir = scratch_dir("test-cli-export-mp4-bad-out");
    let blocker = dir.join("blocked");
    std::fs::write(&blocker, b"not a directory").expect("write blocker");
    let out = blocker.join("out.mp4");
    let course = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-course");
    let (code, _) = crate::run_export_mp4(course, out, 640, 10).expect("run export-mp4");
    assert_eq!(code, ExitCode::FAILURE);
}

#[test]
fn export_docx_writes_a_zip_file() {
    let dir = scratch_dir("test-cli-export-docx");
    let out = dir.join("hello.docx");
    let course = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-course");
    let code = crate::run_export_docx(course, out.clone(), false, false, false, 800)
        .expect("run export-docx");
    assert_eq!(code, ExitCode::SUCCESS);
    let bytes = std::fs::read(&out).expect("read the exported Word document");
    assert!(bytes.starts_with(b"PK"), "the file starts with PK");
}

#[test]
fn export_pdf_writes_a_pdf_file() {
    let dir = scratch_dir("test-cli-export-pdf");
    let out = dir.join("hello.pdf");
    let course = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-course");
    let code = crate::run_export_pdf(course, out.clone(), false, false, false, 800)
        .expect("run export-pdf");
    assert_eq!(code, ExitCode::SUCCESS);
    let bytes = std::fs::read(&out).expect("read the exported PDF");
    assert!(bytes.starts_with(b"%PDF"), "the file starts with %PDF");
}

// ---------------------------------------------------------------- diff

fn diff_code(a: &Path, b: &Path, json: bool) -> Result<ExitCode, String> {
    run(Cli {
        cmd: Cmd::Diff {
            a: a.to_path_buf(),
            b: b.to_path_buf(),
            json,
        },
    })
}

#[test]
fn diff_exit_codes_are_0_same_1_different_2_error() {
    let dir = scratch_dir("test-cli-diff");
    let a = dir.join("a.json");
    let b = dir.join("b.json");
    std::fs::write(&a, HELLO_JSON).expect("write a");
    std::fs::write(&b, HELLO_JSON.replace("Workplace Safety Basics", "Safety")).expect("write b");

    assert_eq!(diff_code(&a, &a, false), Ok(ExitCode::SUCCESS));
    assert_eq!(diff_code(&a, &b, false), Ok(ExitCode::FAILURE));
    assert!(diff_code(&a, &dir.join("missing.json"), false).is_err());
    let (same, differs) = crate::diff_output(&a, &a, false).expect("diff");
    assert_eq!((same.as_str(), differs), ("No differences.", false));

    let (text, differs) = crate::diff_output(&a, &b, false).expect("diff");
    assert!(differs);
    assert!(
        text.contains("Slide \"Welcome\": text of \"title\" changed from \"Workplace Safety Basics\" to \"Safety\""),
        "{text}"
    );
}

#[test]
fn diff_json_parses_and_a_newer_course_is_refused_plainly() {
    let dir = scratch_dir("test-cli-diff-json");
    let a = dir.join("a.json");
    let b = dir.join("b.json");
    std::fs::write(&a, HELLO_JSON).expect("write a");
    std::fs::write(&b, HELLO_JSON.replace("Welcome", "Hello")).expect("write b");
    let (text, differs) = crate::diff_output(&a, &b, true).expect("diff");
    assert!(differs);
    let parsed: serde_json::Value = serde_json::from_str(&text).expect("json");
    assert_eq!(parsed[0]["kind"], "renamed");
    let (empty, differs) = crate::diff_output(&a, &a, true).expect("diff");
    assert!(!differs);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&empty).expect("json"),
        serde_json::json!([])
    );

    let newer = dir.join("newer.json");
    std::fs::write(
        &newer,
        HELLO_JSON.replace("\"schema_version\": 1", "\"schema_version\": 7"),
    )
    .expect("write newer");
    let error = crate::diff_output(&a, &newer, false).expect_err("newer refused");
    assert!(error.contains("newer CourseCraft"), "{error}");
}

#[test]
fn diff_compares_media_in_ccraft_and_folders() {
    let dir = scratch_dir("test-cli-diff-media");
    let course = Course::from_json(HELLO_JSON).expect("hello");
    let one = dir.join("one.ccraft");
    let two = dir.join("two.ccraft");
    std::fs::write(
        &one,
        save_project(
            &course,
            &[("a.png".into(), vec![1]), ("b.png".into(), vec![2])],
        )
        .expect("save"),
    )
    .expect("write");
    std::fs::write(
        &two,
        save_project(
            &course,
            &[("b.png".into(), vec![3]), ("c.png".into(), vec![4])],
        )
        .expect("save"),
    )
    .expect("write");
    let (text, differs) = crate::diff_output(&one, &two, false).expect("diff");
    assert!(differs);
    assert!(text.contains("media file \"a.png\" removed"), "{text}");
    assert!(
        text.contains("media file \"b.png\" has different content"),
        "{text}"
    );
    assert!(text.contains("media file \"c.png\" added"), "{text}");

    let folder = dir.join("folder");
    std::fs::create_dir_all(folder.join("media")).expect("mkdir");
    std::fs::write(folder.join("course.json"), HELLO_JSON).expect("write");
    std::fs::write(folder.join("media/b.png"), [2]).expect("write");
    std::fs::write(folder.join("media/a.png"), [1]).expect("write");
    let (text, differs) = crate::diff_output(&one, &folder, false).expect("diff");
    assert!(!differs, "{text}");
}

// ---------------------------------------------------------------- folders

fn sample_media() -> Vec<(String, Vec<u8>)> {
    vec![
        ("img/a.png".to_owned(), vec![0x89, b'P', b'N', b'G', 1, 2]),
        ("fonts/f.ttf".to_owned(), b"font".to_vec()),
    ]
}

fn read_tree(dir: &Path) -> std::collections::BTreeMap<String, Vec<u8>> {
    fn walk(dir: &Path, base: &Path, out: &mut std::collections::BTreeMap<String, Vec<u8>>) {
        for entry in std::fs::read_dir(dir).expect("read dir") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                walk(&path, base, out);
            } else {
                let rel = path.strip_prefix(base).expect("prefix");
                out.insert(
                    rel.to_string_lossy().replace('\\', "/"),
                    std::fs::read(&path).expect("read"),
                );
            }
        }
    }
    let mut out = std::collections::BTreeMap::new();
    walk(dir, dir, &mut out);
    out
}

#[test]
fn unpack_then_pack_then_unpack_is_byte_identical() {
    let dir = scratch_dir("test-cli-pack");
    let course = Course::from_json(HELLO_JSON).expect("hello");
    let original = dir.join("a.ccraft");
    std::fs::write(
        &original,
        save_project(&course, &sample_media()).expect("save"),
    )
    .expect("write");

    let first = dir.join("first.ccraft-folder");
    let code = run(Cli {
        cmd: Cmd::Unpack {
            archive: original,
            out: first.clone(),
        },
    })
    .expect("unpack");
    assert_eq!(code, ExitCode::SUCCESS);
    let packed = dir.join("b.ccraft");
    run(Cli {
        cmd: Cmd::Pack {
            folder: first.clone(),
            out: packed.clone(),
        },
    })
    .expect("pack");
    let second = dir.join("second.ccraft-folder");
    run(Cli {
        cmd: Cmd::Unpack {
            archive: packed,
            out: second.clone(),
        },
    })
    .expect("unpack again");

    let tree = read_tree(&first);
    assert_eq!(tree, read_tree(&second));
    let names: Vec<&str> = tree.keys().map(String::as_str).collect();
    assert_eq!(
        names,
        ["course.json", "media/fonts/f.ttf", "media/img/a.png"]
    );
    assert_eq!(tree["media/img/a.png"], sample_media()[0].1);
    assert_eq!(
        tree["course.json"],
        format!("{}\n", course.to_json().expect("json")).into_bytes()
    );
}

#[test]
fn unpack_refuses_a_folder_that_has_files_in_it() {
    let dir = scratch_dir("test-cli-unpack-busy");
    let course = Course::from_json(HELLO_JSON).expect("hello");
    let archive = dir.join("a.ccraft");
    std::fs::write(&archive, save_project(&course, &[]).expect("save")).expect("write");
    let out = dir.join("busy");
    std::fs::create_dir_all(&out).expect("mkdir");
    std::fs::write(out.join("mine.txt"), "keep").expect("write");
    let error = run(Cli {
        cmd: Cmd::Unpack {
            archive,
            out: out.clone(),
        },
    })
    .expect_err("refused");
    assert!(error.contains("must not exist or must be empty"), "{error}");
    assert!(out.join("mine.txt").is_file());
}

#[test]
fn every_command_accepts_a_project_folder() {
    let dir = scratch_dir("test-cli-folder-everywhere");
    let course = Course::from_json(HELLO_JSON).expect("hello");
    let folder = dir.join("p.ccraft-folder");
    coursecraft_format::save_project_folder(
        &folder,
        &course,
        &sample_media(),
        &mut coursecraft_format::FolderRecord::new(),
    )
    .expect("save folder");

    assert_eq!(
        run(Cli {
            cmd: Cmd::Validate {
                course: folder.clone()
            }
        }),
        Ok(ExitCode::SUCCESS)
    );
    assert!(
        run(Cli {
            cmd: Cmd::A11yCheck {
                course: folder.clone(),
                json: true
            }
        })
        .is_ok()
    );
    let single = dir.join("single.json");
    std::fs::write(&single, HELLO_JSON).expect("write");
    assert_eq!(diff_code(&folder, &single, false), Ok(ExitCode::SUCCESS));
    let (text, _) = crate::diff_output(&folder, &single, false).expect("diff");
    assert_eq!(text, "No differences.");

    let loaded = crate::load_project_any(&folder).expect("load folder");
    assert_eq!(loaded.media.len(), 2);
    assert_eq!(loaded.course, course);
}

#[test]
fn import_questions_adds_a_bank_from_the_template() {
    let dir = scratch_dir("import_questions");
    let course = dir.join("course.json");
    std::fs::write(&course, HELLO_JSON).unwrap();
    let out = dir.join("out.json");
    let csv = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/questions.csv");
    let cli = Cli {
        cmd: Cmd::ImportQuestions {
            course,
            csv,
            out: out.clone(),
            bank: Some("Imported".to_owned()),
        },
    };
    run(cli).expect("import");
    let imported = Course::from_json(&std::fs::read_to_string(&out).unwrap()).unwrap();
    let bank = imported
        .banks
        .iter()
        .find(|b| b.title == "Imported")
        .unwrap();
    assert_eq!(bank.slides.len(), 4);
}
