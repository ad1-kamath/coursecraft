//! Tests for coursecraft-mcp over in-memory pipes.

use std::io::Cursor;

use coursecraft_control::HeadlessHost;
use coursecraft_course::Course;
use coursecraft_engine::Session;
use serde_json::{Value as Json, json};

use crate::serve;

fn run_mcp_exchange(input_lines: &[Json], host: &mut HeadlessHost) -> Vec<Json> {
    let mut input_bytes = Vec::new();
    for msg in input_lines {
        let mut s = serde_json::to_string(msg).expect("serialize msg");
        s.push('\n');
        input_bytes.extend_from_slice(s.as_bytes());
    }

    let reader = Cursor::new(input_bytes);
    let mut writer = Vec::new();

    serve(reader, &mut writer, host).expect("serve exchange");

    let output_str = String::from_utf8(writer).expect("utf8 output");
    output_str
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse response line"))
        .collect()
}

#[test]
fn test_mcp_initialize_handshake() {
    let mut host = HeadlessHost::new(Session::blank("Handshake Test"), None);

    let messages = vec![
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": "claude-code", "version": "1.0" }
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {}
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "ping",
            "params": {}
        }),
    ];

    let responses = run_mcp_exchange(&messages, &mut host);
    // notifications/initialized produces no response, so 2 responses total
    assert_eq!(responses.len(), 2);

    // Initialize response
    let init_res = &responses[0];
    assert_eq!(init_res["id"], 1);
    assert_eq!(init_res["result"]["protocolVersion"], "2024-11-05");
    assert_eq!(init_res["result"]["serverInfo"]["name"], "coursecraft");
    assert!(init_res["result"]["capabilities"]["tools"].is_object());

    // Ping response
    let ping_res = &responses[1];
    assert_eq!(ping_res["id"], 2);
    assert!(ping_res["result"].is_object());
}

#[test]
fn test_mcp_tools_list_contains_every_tool_with_schema() {
    let mut host = HeadlessHost::new(Session::blank("Tools List Test"), None);

    let messages = vec![json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/list",
        "params": {}
    })];

    let responses = run_mcp_exchange(&messages, &mut host);
    assert_eq!(responses.len(), 1);

    let tools = responses[0]["result"]["tools"]
        .as_array()
        .expect("tools array");
    assert_eq!(tools.len(), 14);

    let expected_names = [
        "command_list",
        "command_run",
        "command_batch",
        "course_inspect",
        "validate",
        "simulate",
        "selection_get",
        "selection_set",
        "undo",
        "redo",
        "session_info",
        "file_save",
        "file_open",
        "render_preview",
    ];

    for name in expected_names {
        let tool = tools
            .iter()
            .find(|t| t["name"] == name)
            .unwrap_or_else(|| panic!("missing tool {name}"));
        assert!(!tool["description"].as_str().unwrap().is_empty());
        assert_eq!(tool["inputSchema"]["type"], "object");
    }
}

#[test]
fn test_mcp_tools_call_command_run() {
    let mut host = HeadlessHost::new(Session::blank("Command Run Test"), None);

    let messages = vec![json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "command_run",
            "arguments": {
                "id": "slide.add",
                "params": { "title": "New MCP Slide" }
            }
        }
    })];

    let responses = run_mcp_exchange(&messages, &mut host);
    assert_eq!(responses.len(), 1);

    let res = &responses[0]["result"];
    assert!(res["isError"].is_null() || res["isError"] == false);
    let content = res["content"].as_array().expect("content array");
    assert_eq!(content[0]["type"], "text");
    let text = content[0]["text"].as_str().expect("text");
    assert!(text.contains("id"));
}

#[test]
fn test_mcp_tools_call_is_error_result_for_bad_command() {
    let mut host = HeadlessHost::new(Session::blank("Bad Command Test"), None);

    let messages = vec![json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "command_run",
            "arguments": {
                "id": "trigger.add",
                "params": {
                    "slide": "slide1",
                    "trigger": { "action": { "action": "show_layer", "layer": "ghost" }, "when": { "event": "submit" } }
                }
            }
        }
    })];

    let responses = run_mcp_exchange(&messages, &mut host);
    assert_eq!(responses.len(), 1);

    // In MCP, tool errors return isError: true inside result, not a JSON-RPC error
    assert!(responses[0].get("error").is_none());
    let res = &responses[0]["result"];
    assert_eq!(res["isError"], true);
    let content = res["content"].as_array().expect("content array");
    let text = content[0]["text"].as_str().expect("text");
    assert!(text.contains("ghost"));
}

#[test]
fn test_mcp_unknown_method_error() {
    let mut host = HeadlessHost::new(Session::blank("Unknown Method Test"), None);

    let messages = vec![json!({
        "jsonrpc": "2.0",
        "id": 42,
        "method": "nonexistent/rpc_method",
        "params": {}
    })];

    let responses = run_mcp_exchange(&messages, &mut host);
    assert_eq!(responses.len(), 1);

    let err = &responses[0]["error"];
    assert_eq!(err["code"], -32601);
    assert!(
        err["message"]
            .as_str()
            .unwrap()
            .contains("Method not found")
    );
}

#[test]
fn test_mcp_rebuild_hello_course_through_command_batch() {
    let script_text = include_str!("../../../examples/hello-course/rebuild.script.json");
    let expected_text = include_str!("../../../examples/hello-course/course.json");
    let expected = Course::from_json(expected_text).expect("expected hello-course");

    let steps: Vec<Json> = serde_json::from_str(script_text).expect("parse rebuild script");

    let mut host = HeadlessHost::new(Session::blank("Initial Title"), None);

    let messages = vec![
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "command_batch",
                "arguments": {
                    "steps": steps
                }
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "course_inspect",
                "arguments": {}
            }
        }),
    ];

    let responses = run_mcp_exchange(&messages, &mut host);
    assert_eq!(responses.len(), 2);

    // Verify batch succeeded
    assert!(
        responses[0]["result"]["isError"].is_null() || responses[0]["result"]["isError"] == false
    );

    // Verify rebuilt course equals expected course
    assert_eq!(host.session.course, expected);

    // Verify course_inspect returned the expected course JSON
    let inspect_content = responses[1]["result"]["content"][0]["text"]
        .as_str()
        .expect("text content");
    let inspected_course = Course::from_json(inspect_content).expect("parse inspected course");
    assert_eq!(inspected_course, expected);
}

fn decode_base64_for_test(s: &str) -> Vec<u8> {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let find_val = |c: u8| TABLE.iter().position(|&b| b == c).map(|p| p as u8);
    let bytes: Vec<u8> = s
        .bytes()
        .filter(|&b| b != b'=' && b != b'\n' && b != b'\r')
        .collect();
    let mut out = Vec::new();
    for chunk in bytes.chunks(4) {
        let mut v = 0u32;
        for &b in chunk {
            v = (v << 6) | (find_val(b).expect("valid base64 char") as u32);
        }
        match chunk.len() {
            4 => {
                out.push((v >> 16) as u8);
                out.push((v >> 8) as u8);
                out.push(v as u8);
            }
            3 => {
                out.push((v >> 10) as u8);
                out.push((v >> 2) as u8);
            }
            2 => {
                out.push((v >> 4) as u8);
            }
            _ => {}
        }
    }
    out
}

#[test]
fn test_mcp_tools_call_render_preview() {
    let expected_text = include_str!("../../../examples/hello-course/course.json");
    let course = Course::from_json(expected_text).expect("hello-course");

    let mut host = HeadlessHost::new(Session::new(course), None);

    let messages = vec![json!({
        "jsonrpc": "2.0",
        "id": 10,
        "method": "tools/call",
        "params": {
            "name": "render_preview",
            "arguments": {
                "slide": "s-intro",
                "width": 960,
                "mode": "authoring"
            }
        }
    })];

    let responses = run_mcp_exchange(&messages, &mut host);
    assert_eq!(responses.len(), 1);

    let res = &responses[0]["result"];
    assert!(res["isError"].is_null() || res["isError"] == false);
    let content = res["content"].as_array().expect("content array");
    assert_eq!(content.len(), 2);

    // Block 0: image block
    assert_eq!(content[0]["type"], "image");
    assert_eq!(content[0]["mimeType"], "image/png");
    let b64 = content[0]["data"].as_str().expect("base64 string");
    assert!(!b64.is_empty());
    let decoded_png = decode_base64_for_test(b64);
    assert!(
        decoded_png.starts_with(b"\x89PNG\r\n\x1a\n"),
        "decoded base64 must have PNG signature"
    );

    // Block 1: text block "<slide title> — <w>×<h>, mode"
    assert_eq!(content[1]["type"], "text");
    let text = content[1]["text"].as_str().expect("text string");
    assert_eq!(text, "Welcome — 960×540, authoring");
}

#[test]
fn attached_server_starts_without_the_app_and_finds_it_when_it_opens() {
    use coursecraft_control::ControlServer;

    use crate::{AttachedEditor, McpBackend};

    let dir = std::env::temp_dir().join(format!("coursecraft-attach-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");

    // No app yet: a clear error, and the backend survives it.
    let mut attached = AttachedEditor::new(dir.clone());
    let err = attached
        .call("session.info", json!({}))
        .expect_err("no app is running");
    assert!(err.contains("Open the desktop app"), "{err}");

    // The app opens: the same backend reaches it, with no restart.
    let server = ControlServer::start(Some(&dir)).expect("start server");
    let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let caller = {
        let done = done.clone();
        std::thread::spawn(move || {
            let result = attached.call("session.info", json!({}));
            done.store(true, std::sync::atomic::Ordering::SeqCst);
            result
        })
    };
    let mut host = HeadlessHost::new(Session::blank("Opened later"), None);
    while !done.load(std::sync::atomic::Ordering::SeqCst) {
        server.poll(&mut host);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    let info = caller.join().expect("caller").expect("reaches the app");
    assert_eq!(info["title"], "Opened later");
    drop(server);
    let _ = std::fs::remove_dir_all(&dir);
}
