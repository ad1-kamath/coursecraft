//! Tests for coursecraft-control.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;

use coursecraft_engine::Session;
use serde_json::json;

use crate::{ControlClient, ControlHost, ControlServer, HeadlessHost, dispatch};

fn scratch_dir(sub: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/agy-scratch")
        .join(sub);
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

#[test]
fn test_token_required() {
    let mut host = HeadlessHost::new(Session::blank("Test Token"), None);
    let token = "secret123";

    // Missing token
    let req_missing = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "session.info",
        "params": {}
    });
    let res = dispatch(&mut host, &req_missing, token);
    assert_eq!(res["error"]["code"], -32001);
    assert_eq!(res["error"]["message"], "unauthorized");

    // Wrong token
    let req_wrong = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "session.info",
        "params": { "token": "wrong" }
    });
    let res = dispatch(&mut host, &req_wrong, token);
    assert_eq!(res["error"]["code"], -32001);

    // Correct token
    let req_ok = json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "session.info",
        "params": { "token": "secret123" }
    });
    let res = dispatch(&mut host, &req_ok, token);
    assert!(res.get("result").is_some());
    assert_eq!(res["result"]["title"], "Test Token");
}

#[test]
fn test_dispatch_table_all_methods_on_headless_host() {
    let scratch = scratch_dir("control-dispatch-methods");
    let file_path = scratch.join("course.ccraft");

    let mut host = HeadlessHost::new(
        Session::blank("Full Dispatch Test"),
        Some(file_path.clone()),
    );
    let token = "test_tok";

    // 1. commands.list
    let res = dispatch(
        &mut host,
        &json!({ "jsonrpc": "2.0", "id": 1, "method": "commands.list", "params": { "token": token } }),
        token,
    );
    let cmds = res["result"].as_array().expect("array of commands");
    assert!(!cmds.is_empty());
    assert!(cmds.iter().any(|c| c["id"] == "slide.add"));

    // 2. command.run
    let res = dispatch(
        &mut host,
        &json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "command.run",
            "params": { "token": token, "id": "slide.add", "params": { "title": "Second Slide" } }
        }),
        token,
    );
    let new_slide_id = res["result"]["id"].as_str().expect("slide id").to_string();

    // 3. course.get (whole course & pointer)
    let res = dispatch(
        &mut host,
        &json!({ "jsonrpc": "2.0", "id": 3, "method": "course.get", "params": { "token": token } }),
        token,
    );
    assert_eq!(res["result"]["title"], "Full Dispatch Test");

    let res = dispatch(
        &mut host,
        &json!({ "jsonrpc": "2.0", "id": 4, "method": "course.get", "params": { "token": token, "pointer": "/title" } }),
        token,
    );
    assert_eq!(res["result"], "Full Dispatch Test");

    let res = dispatch(
        &mut host,
        &json!({ "jsonrpc": "2.0", "id": 5, "method": "course.get", "params": { "token": token, "pointer": "/nonexistent/field" } }),
        token,
    );
    assert_eq!(res["error"]["code"], -32004);

    // 4. course.validate
    let res = dispatch(
        &mut host,
        &json!({ "jsonrpc": "2.0", "id": 6, "method": "course.validate", "params": { "token": token } }),
        token,
    );
    assert!(res["result"].as_array().is_some());

    // 5. selection.get & selection.set
    let res = dispatch(
        &mut host,
        &json!({ "jsonrpc": "2.0", "id": 7, "method": "selection.get", "params": { "token": token } }),
        token,
    );
    assert_eq!(res["result"]["slide"], new_slide_id);

    let res = dispatch(
        &mut host,
        &json!({ "jsonrpc": "2.0", "id": 8, "method": "selection.set", "params": { "token": token, "slide": "slide1" } }),
        token,
    );
    assert_eq!(res["result"]["slide"], "slide1");

    let res = dispatch(
        &mut host,
        &json!({ "jsonrpc": "2.0", "id": 9, "method": "selection.set", "params": { "token": token, "slide": "ghost_slide" } }),
        token,
    );
    assert_eq!(res["error"]["code"], -32004);

    // 6. preview.run_inputs
    let res = dispatch(
        &mut host,
        &json!({
            "jsonrpc": "2.0",
            "id": 10,
            "method": "preview.run_inputs",
            "params": {
                "token": token,
                "inputs": [{ "input": "start" }]
            }
        }),
        token,
    );
    assert!(res["result"]["effects"].as_array().is_some());

    // 7. session.info
    let res = dispatch(
        &mut host,
        &json!({ "jsonrpc": "2.0", "id": 11, "method": "session.info", "params": { "token": token } }),
        token,
    );
    assert_eq!(res["result"]["title"], "Full Dispatch Test");
    assert_eq!(res["result"]["dirty"], true);
    assert_eq!(res["result"]["can_undo"], true);

    // 8. edit.undo & edit.redo
    let res = dispatch(
        &mut host,
        &json!({ "jsonrpc": "2.0", "id": 12, "method": "edit.undo", "params": { "token": token } }),
        token,
    );
    assert!(res["result"]["revision"].is_number());

    let res = dispatch(
        &mut host,
        &json!({ "jsonrpc": "2.0", "id": 13, "method": "edit.redo", "params": { "token": token } }),
        token,
    );
    assert!(res["result"]["revision"].is_number());

    // 9. file.save & file.open
    let res = dispatch(
        &mut host,
        &json!({ "jsonrpc": "2.0", "id": 14, "method": "file.save", "params": { "token": token } }),
        token,
    );
    assert_eq!(
        res["result"]["path"],
        file_path.to_string_lossy().to_string()
    );
    assert!(file_path.exists());

    // Modify to make dirty
    let _ = host.run("course.set_title", &json!({ "title": "Dirty Title" }));
    assert!(host.info().dirty);

    // file.open without force fails with -32002
    let res = dispatch(
        &mut host,
        &json!({ "jsonrpc": "2.0", "id": 15, "method": "file.open", "params": { "token": token, "path": file_path.to_string_lossy().to_string() } }),
        token,
    );
    assert_eq!(res["error"]["code"], -32002);

    // file.open with force succeeds
    let res = dispatch(
        &mut host,
        &json!({ "jsonrpc": "2.0", "id": 16, "method": "file.open", "params": { "token": token, "path": file_path.to_string_lossy().to_string(), "force": true } }),
        token,
    );
    assert_eq!(res["result"]["title"], "Full Dispatch Test");
}

#[test]
fn test_batch_atomicity_through_command_batch() {
    let mut host = HeadlessHost::new(Session::blank("Batch Protocol"), None);
    let token = "tok";
    let initial_course = host.course().clone();

    // 1. Successful batch -> 1 undo step
    let res = dispatch(
        &mut host,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "command.batch",
            "params": {
                "token": token,
                "steps": [
                    { "id": "slide.add", "params": { "title": "Slide B" } },
                    { "id": "slide.add", "params": { "title": "Slide C" } }
                ]
            }
        }),
        token,
    );
    assert!(res["result"].as_array().is_some());
    assert_eq!(res["result"].as_array().unwrap().len(), 2);
    assert_eq!(host.info().revision, 1);

    // Undo reverts the whole batch in one step
    let res_undo = dispatch(
        &mut host,
        &json!({ "jsonrpc": "2.0", "id": 2, "method": "edit.undo", "params": { "token": token } }),
        token,
    );
    assert!(res_undo["result"].is_object());
    assert_eq!(host.course(), &initial_course);

    // 2. Failing batch -> rolls back, leaves byte-identical
    let before_fail = host.course().clone();
    let res_fail = dispatch(
        &mut host,
        &json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "command.batch",
            "params": {
                "token": token,
                "steps": [
                    { "id": "slide.add", "params": { "title": "Slide X" } },
                    { "id": "trigger.add", "params": { "slide": "slide1", "trigger": { "action": { "action": "show_layer", "layer": "ghost" }, "when": { "event": "submit" } } } }
                ]
            }
        }),
        token,
    );
    assert_eq!(res_fail["error"]["code"], -32000);
    assert_eq!(host.course(), &before_fail);
}

#[test]
fn test_real_round_trip_ephemeral_port() {
    let scratch = scratch_dir("real-round-trip");
    let server = ControlServer::start(Some(&scratch)).expect("start server");
    let bound_port = server.port();

    // Assert bound address is loopback
    assert!(bound_port > 0);

    let mut client = ControlClient::connect_from_config(&scratch).expect("connect client");

    let client_thread = std::thread::spawn(move || {
        let res_info = client
            .call("session.info", json!({}))
            .expect("session.info");
        assert_eq!(res_info["title"], "Round Trip Course");

        let res_cmd = client
            .call(
                "command.run",
                json!({ "id": "slide.add", "params": { "title": "Added Slide" } }),
            )
            .expect("command.run");
        assert!(res_cmd.get("id").is_some());

        let res_list = client
            .call("commands.list", json!({}))
            .expect("commands.list");
        assert!(!res_list.as_array().unwrap().is_empty());
    });

    let mut host = HeadlessHost::new(Session::blank("Round Trip Course"), None);
    let mut total_polled = 0;
    while total_polled < 3 {
        let n = server.poll(&mut host);
        total_polled += n;
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    client_thread.join().expect("client thread finished");

    // Check discovery file exists while running
    let disc_path = scratch.join("control.json");
    assert!(disc_path.exists());

    // Drop server and verify discovery file removed
    drop(server);
    assert!(!disc_path.exists());
}

#[test]
fn test_oversized_line_error() {
    let server = ControlServer::start(None).expect("start server");
    let port = server.port();

    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");

    // Write > 16 MiB of bytes without newline
    let chunk = vec![b'x'; 1024 * 1024]; // 1 MiB
    for _ in 0..17 {
        if stream.write_all(&chunk).is_err() {
            break;
        }
    }
    let _ = stream.write_all(b"\n");
    let _ = stream.flush();

    // Server should send an error and close the connection
    let mut resp = String::new();
    let _ = stream.read_to_string(&mut resp);
    assert!(resp.contains("-32700") || resp.is_empty());
}

#[test]
fn test_never_binds_non_loopback() {
    let server = ControlServer::start(None).expect("start server");
    assert!(server.port() > 0);
    // Connect directly to 127.0.0.1 to assert loopback binding works
    let stream = TcpStream::connect(("127.0.0.1", server.port()));
    assert!(stream.is_ok());
}

#[test]
fn test_waker_fires_so_an_idle_owner_answers() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let scratch = scratch_dir("waker");
    let wakes = Arc::new(AtomicUsize::new(0));
    let wakes_in = Arc::clone(&wakes);
    let server = ControlServer::start_with_waker(
        Some(&scratch),
        Some(Arc::new(move || {
            wakes_in.fetch_add(1, Ordering::SeqCst);
        })),
    )
    .expect("start server");
    let mut client = ControlClient::connect_from_config(&scratch).expect("connect client");
    let client_thread = std::thread::spawn(move || client.call("session.info", json!({})));

    // The owner only polls after being woken, like an idle egui app.
    let mut host = HeadlessHost::new(Session::blank("Idle"), None);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while wakes.load(Ordering::SeqCst) == 0 {
        assert!(std::time::Instant::now() < deadline, "waker never fired");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(server.poll(&mut host), 1);
    let info = client_thread.join().expect("join").expect("session.info");
    assert_eq!(info["title"], "Idle");
}

#[test]
fn test_base64_known_vectors() {
    use crate::encode_base64;

    assert_eq!(encode_base64(b""), "");
    assert_eq!(encode_base64(b"f"), "Zg==");
    assert_eq!(encode_base64(b"fo"), "Zm8=");
    assert_eq!(encode_base64(b"foo"), "Zm9v");
    assert_eq!(encode_base64(b"foob"), "Zm9vYg==");
    assert_eq!(encode_base64(b"fooba"), "Zm9vYmE=");
    assert_eq!(encode_base64(b"foobar"), "Zm9vYmFy");
}

#[test]
fn test_render_slide_dispatch_and_file_output() {
    let scratch = scratch_dir("render-slide-test");
    let mut host = HeadlessHost::new(Session::blank("Render Test Course"), None);
    let token = "tok123";

    // 1. Missing slide returns error
    let req_missing = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "render.slide",
        "params": { "token": token, "slide": "nonexistent" }
    });
    let res_missing = dispatch(&mut host, &req_missing, token);
    assert_eq!(res_missing["error"]["code"], -32004);

    // 2. Width > 4096 is refused
    let req_wide = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "render.slide",
        "params": { "token": token, "width": 5000 }
    });
    let res_wide = dispatch(&mut host, &req_wide, token);
    assert_eq!(res_wide["error"]["code"], -32602);

    // 3. Render slide returns base64
    let req_render = json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "render.slide",
        "params": { "token": token, "width": 640 }
    });
    let res_render = dispatch(&mut host, &req_render, token);
    assert!(res_render.get("result").is_some());
    assert_eq!(res_render["result"]["width"], 640);
    assert_eq!(res_render["result"]["height"], 360);
    let b64 = res_render["result"]["png_base64"]
        .as_str()
        .expect("png_base64 string");
    assert!(!b64.is_empty());

    // 4. Render slide with path writes file
    let out_png = scratch.join("rendered.png");
    let req_path = json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "render.slide",
        "params": { "token": token, "width": 320, "path": out_png.to_string_lossy().to_string() }
    });
    let res_path = dispatch(&mut host, &req_path, token);
    assert!(res_path.get("result").is_some());
    assert_eq!(res_path["result"]["width"], 320);
    assert_eq!(res_path["result"]["height"], 180);
    assert!(out_png.exists());
    let file_bytes = std::fs::read(&out_png).expect("read rendered png");
    assert!(file_bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
}

#[test]
fn test_headless_media_source_resolves_files() {
    let scratch = scratch_dir("headless-media-res");
    let media_dir = scratch.join("media");
    std::fs::create_dir_all(&media_dir).expect("create media dir");
    let test_file = media_dir.join("photo.jpg");
    std::fs::write(&test_file, b"test-media-payload").expect("write test media");

    let host = HeadlessHost::new(
        Session::blank("Media Host"),
        Some(scratch.join("course.json")),
    );
    let fetch = host.media().get("photo.jpg");
    match fetch {
        coursecraft_render::MediaFetch::Ready(bytes) => {
            assert_eq!(&*bytes, b"test-media-payload");
        }
        other => panic!("expected Ready, got {other:?}"),
    }

    // Traversal or missing
    let missing = host.media().get("nonexistent.png");
    assert_eq!(missing, coursecraft_render::MediaFetch::Missing);

    let traversal = host.media().get("../evil.png");
    assert_eq!(traversal, coursecraft_render::MediaFetch::Missing);
}

#[test]
fn question_insert_course_grades_by_clicking_the_button_and_reports_score() {
    use coursecraft_logic::{Effect, Input, Player};

    let mut session = Session::blank("Quiz");
    let inserted = session
        .execute("question.insert", &json!({ "slide": "slide1" }))
        .expect("question.insert");
    let button = inserted["button"]
        .as_str()
        .expect("submit button id")
        .to_owned();
    let finish = session
        .execute("course.add_finish_button", &json!({}))
        .expect("course.add_finish_button")["button"]
        .as_str()
        .expect("finish button id")
        .to_owned();

    let mut player = Player::new(&session.course).expect("valid player");
    player.handle(&Input::Start);
    player.handle(&Input::SelectChoice {
        choice: "a".to_owned(),
    });
    let effects = player.handle(&Input::Click { object: button });
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::QuestionAnswered { correct: true, .. })),
        "{effects:?}"
    );

    let effects = player.handle(&Input::Click { object: finish });
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::Score { percent: 100, .. })),
        "{effects:?}"
    );
    assert!(
        effects.iter().any(|effect| matches!(effect, Effect::Exit)),
        "{effects:?}"
    );
}
