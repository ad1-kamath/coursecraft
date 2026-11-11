//! A tiny OpenAI-compatible mock endpoint for trying AI features offline.
//!
//! `cargo xtask mock-ai [--port 8099]` answers
//! `POST /v1/chat/completions` with an OpenAI-shaped reply and permissive CORS,
//! including `OPTIONS`, so the web editor can reach it too. The reply is `OK`
//! unless the last user message contains `MOCK_FILE=<name>`; then the contents
//! of `xtask/mock-ai/<name>` are returned. A vision message with an
//! `image_url` part gets `A mock description of the picture.` when no file is
//! named.
//!
//! `POST /v1/audio/transcriptions` answers a small valid WebVTT file, and
//! `POST /v1/audio/speech` answers a tiny valid MP3. A request whose model is
//! `no-audio` answers 404, so the "this server doesn't offer it" path can be
//! tried by hand.
//!
//! Everything is test tooling. It binds to `127.0.0.1` only, closes each
//! connection after one request, logs only the first line and body size, and
//! never panics on a bad request.

use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};

use crate::mock_lrs::{Request, Response, read_request, write_response};

const READ_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_FILE_NAME: usize = 128;

/// The captions answer. Two cues, so a preview has more than one line.
pub const MOCK_VTT: &str = "WEBVTT\n\n\
00:00:00.000 --> 00:00:02.000\n\
Mock captions from CourseCraft.\n\n\
00:00:02.000 --> 00:00:04.000\n\
This audio isn't real speech.\n";

/// MPEG-1 Layer III, 128 kbps, 44.1 kHz frame header.
pub const MP3_FRAME_HEADER: [u8; 4] = [0xFF, 0xFB, 0x90, 0x64];

/// A tiny MP3: an ID3 tag and two silent-like frames, generated in code so the
/// header bytes and frame lengths stay visible.
#[must_use]
pub fn mock_mp3() -> Vec<u8> {
    const FRAME_BYTES: usize = 417;
    let mut out = Vec::with_capacity(10 + 2 * FRAME_BYTES);
    out.extend_from_slice(b"ID3\x04\x00\x00\x00\x00\x00\x00");
    for _ in 0..2 {
        out.extend_from_slice(&MP3_FRAME_HEADER);
        out.extend(std::iter::repeat_n(
            0_u8,
            FRAME_BYTES - MP3_FRAME_HEADER.len(),
        ));
    }
    out
}

/// Run the mock endpoint until the process ends.
///
/// # Errors
/// Returns an error if the listener cannot bind or the reply directory is missing.
pub fn run(port: u16) -> Result<(), String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("xtask has no parent directory")?;
    let base_dir = root.join("xtask/mock-ai");
    if !base_dir.is_dir() {
        return Err(format!("{} is missing", base_dir.display()));
    }
    let listener = TcpListener::bind(("127.0.0.1", port))
        .map_err(|err| format!("bind 127.0.0.1:{port}: {err}"))?;
    let local = listener
        .local_addr()
        .map_err(|err| format!("local_addr: {err}"))?;
    println!("mock-ai: http://{}/v1/chat/completions", local);

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let base_dir = base_dir.clone();
                std::thread::spawn(move || serve(stream, &base_dir));
            }
            Err(err) => eprintln!("mock-ai: accept: {err}"),
        }
    }
    Ok(())
}

fn serve(mut stream: TcpStream, base_dir: &Path) {
    let _ = stream.set_read_timeout(Some(READ_TIMEOUT));
    let _ = stream.set_write_timeout(Some(READ_TIMEOUT));
    let response = match read_request(&mut stream) {
        Ok(request) => {
            println!(
                "mock-ai: {} ({} bytes)",
                first_line(&request),
                request.body.len()
            );
            handle(base_dir, &request)
        }
        Err(response) => with_cors(response),
    };
    let _ = write_response(&mut stream, &response);
}

fn first_line(request: &Request) -> String {
    if request.query.is_empty() {
        format!("{} {} HTTP/1.1", request.method, request.path)
    } else {
        format!(
            "{} {}?{} HTTP/1.1",
            request.method, request.path, request.query
        )
    }
}

/// Handle one request. Pure, so tests can drive it without a socket.
pub fn handle(base_dir: &Path, request: &Request) -> Response {
    let response = if request.method.eq_ignore_ascii_case("OPTIONS") {
        Response::empty(204)
    } else if request.method.eq_ignore_ascii_case("POST") && request.path == "/v1/chat/completions"
    {
        chat(base_dir, request)
    } else if request.method.eq_ignore_ascii_case("POST")
        && request.path == "/v1/audio/transcriptions"
    {
        transcription(request)
    } else if request.method.eq_ignore_ascii_case("POST") && request.path == "/v1/audio/speech" {
        speech(request)
    } else {
        Response::text(404, "Not Found")
    };
    with_cors(response)
}

fn transcription(request: &Request) -> Response {
    if requested_model(&request.body).as_deref() == Some("no-audio") {
        return Response::text(404, "Not Found");
    }
    Response {
        status: 200,
        headers: vec![(
            "Content-Type".to_string(),
            "text/vtt; charset=utf-8".to_string(),
        )],
        body: MOCK_VTT.as_bytes().to_vec(),
    }
}

fn speech(request: &Request) -> Response {
    if requested_model(&request.body).as_deref() == Some("no-audio") {
        return Response::text(404, "Not Found");
    }
    Response {
        status: 200,
        headers: vec![("Content-Type".to_string(), "audio/mpeg".to_string())],
        body: mock_mp3(),
    }
}

/// The `model` field of a JSON or multipart request body.
fn requested_model(body: &[u8]) -> Option<String> {
    if let Ok(value) = serde_json::from_slice::<Value>(body)
        && let Some(model) = value.get("model").and_then(Value::as_str)
    {
        return Some(model.to_string());
    }
    let text = String::from_utf8_lossy(body);
    let marker = "name=\"model\"\r\n\r\n";
    let after = text.split(marker).nth(1)?;
    let end = after.find("\r\n")?;
    after.get(..end).map(str::to_owned)
}

fn chat(base_dir: &Path, request: &Request) -> Response {
    let reply = reply_for(&request.body, base_dir);
    Response::json(
        200,
        &json!({
            "object": "chat.completion",
            "model": "coursecraft-mock",
            "choices": [{
                "index": 0,
                "message": { "role": "assistant", "content": reply },
                "finish_reason": "stop"
            }],
        }),
    )
}

/// The reply text: a named file's contents, a translation batch, a description
/// for an image, or `OK`.
pub fn reply_for(body: &[u8], base_dir: &Path) -> String {
    if let Some(name) = mock_file_name(body)
        && let Ok(text) = std::fs::read_to_string(base_dir.join(&name))
    {
        return text;
    }
    if let Some(reply) = translation_reply(body) {
        return reply;
    }
    if has_image_url(body) {
        return "A mock description of the picture.".to_string();
    }
    "OK".to_string()
}

/// When the system message asks for translation, prefix each text in the last
/// user message's JSON array with `[fr] `. A text containing `MOCK_DROP` is
/// left out, so a hand test can produce a partial answer.
fn translation_reply(body: &[u8]) -> Option<String> {
    let value: Value = serde_json::from_slice(body).ok()?;
    let messages = value.get("messages")?.as_array()?;
    let asks_to_translate = messages.iter().any(|message| {
        message.get("role").and_then(Value::as_str) == Some("system")
            && message
                .get("content")
                .and_then(Value::as_str)
                .is_some_and(|text| text.to_ascii_lowercase().contains("translate"))
    });
    if !asks_to_translate {
        return None;
    }
    let last_user = messages
        .iter()
        .rev()
        .find(|message| message.get("role").and_then(Value::as_str) == Some("user"))?;
    let text = message_text(last_user.get("content")?)?;
    let open = text.find('[')?;
    let close = text.rfind(']')?;
    if close <= open {
        return None;
    }
    let slice = text.get(open..=close)?;
    let units = serde_json::from_str::<Value>(slice).ok()?;
    let units = units.as_array()?;
    let mut translated = Vec::new();
    for unit in units {
        let Some(id) = unit.get("id").and_then(Value::as_str) else {
            continue;
        };
        let Some(source) = unit.get("text").and_then(Value::as_str) else {
            continue;
        };
        if source.contains("MOCK_DROP") {
            continue;
        }
        translated.push(json!({
            "id": id,
            "text": format!("[fr] {source}"),
        }));
    }
    serde_json::to_string(&Value::Array(translated)).ok()
}

fn has_image_url(body: &[u8]) -> bool {
    let Ok(value) = serde_json::from_slice::<Value>(body) else {
        return false;
    };
    let Some(messages) = value.get("messages").and_then(Value::as_array) else {
        return false;
    };
    let Some(last_user) = messages
        .iter()
        .rev()
        .find(|message| message.get("role").and_then(Value::as_str) == Some("user"))
    else {
        return false;
    };
    last_user
        .get("content")
        .and_then(Value::as_array)
        .is_some_and(|parts| {
            parts
                .iter()
                .any(|part| part.get("type").and_then(Value::as_str) == Some("image_url"))
        })
}

fn mock_file_name(body: &[u8]) -> Option<String> {
    let value: Value = serde_json::from_slice(body).ok()?;
    let messages = value.get("messages")?.as_array()?;
    let last_user = messages
        .iter()
        .rev()
        .find(|message| message.get("role").and_then(Value::as_str) == Some("user"))?;
    let text = message_text(last_user.get("content")?)?;
    let after = text.rsplit_once("MOCK_FILE=")?.1;
    let name: String = after
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        .collect();
    valid_file_name(&name).then_some(name)
}

fn message_text(content: &Value) -> Option<String> {
    match content {
        Value::String(text) => Some(text.clone()),
        Value::Array(parts) => {
            let mut out = String::new();
            for part in parts {
                if part.get("type").and_then(Value::as_str) == Some("text")
                    && let Some(text) = part.get("text").and_then(Value::as_str)
                {
                    out.push_str(text);
                    out.push('\n');
                }
            }
            Some(out)
        }
        _ => None,
    }
}

fn valid_file_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_FILE_NAME
        && !name.starts_with('.')
        && !name.contains("..")
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

fn with_cors(mut response: Response) -> Response {
    for (name, value) in [
        ("Access-Control-Allow-Origin", "*"),
        ("Access-Control-Allow-Methods", "POST, OPTIONS"),
        (
            "Access-Control-Allow-Headers",
            "Content-Type, Authorization",
        ),
    ] {
        response.headers.push((name.to_string(), value.to_string()));
    }
    response
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use std::path::PathBuf;

    fn request(method: &str, path: &str, body: &[u8]) -> Request {
        Request {
            method: method.to_string(),
            path: path.to_string(),
            query: String::new(),
            headers: Vec::new(),
            body: body.to_vec(),
        }
    }

    fn scratch(name: &str) -> PathBuf {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask has a parent");
        let dir = root
            .join("target/oc-scratch/mock-ai")
            .join(format!("{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    fn body_for(text: &str) -> Vec<u8> {
        serde_json::to_vec(&json!({
            "messages": [{ "role": "user", "content": text }],
        }))
        .expect("json")
    }

    #[test]
    fn answers_ok_by_default() {
        assert_eq!(reply_for(&body_for("Hello"), &scratch("default")), "OK");
        assert_eq!(reply_for(b"not json", &scratch("bad")), "OK");
    }

    #[test]
    fn reads_a_named_reply_file() {
        let dir = scratch("file");
        std::fs::write(dir.join("answer.txt"), "From a file").expect("write");
        assert_eq!(
            reply_for(&body_for("say MOCK_FILE=answer.txt"), &dir),
            "From a file"
        );
        assert_eq!(
            reply_for(&body_for("say MOCK_FILE=missing.txt"), &dir),
            "OK"
        );
    }

    #[test]
    fn refuses_path_traversal_names() {
        let dir = scratch("traversal");
        std::fs::write(dir.join("ok.txt"), "OK file").expect("write");
        for name in ["../secret", "..", ".hidden", "a/b", "a\\b"] {
            let text = format!("MOCK_FILE={name}");
            assert_eq!(reply_for(&body_for(&text), &dir), "OK", "{name}");
        }
    }

    #[test]
    fn reads_the_last_user_message_only() {
        let dir = scratch("last-user");
        std::fs::write(dir.join("last.txt"), "last").expect("write");
        let body = serde_json::to_vec(&json!({
            "messages": [
                { "role": "user", "content": "MOCK_FILE=last.txt" },
                { "role": "assistant", "content": "reply" },
                { "role": "user", "content": "plain" },
            ],
        }))
        .expect("json");
        assert_eq!(reply_for(&body, &dir), "OK");
    }

    #[test]
    fn reads_the_outline_rule_from_the_document_user_message() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask has a parent");
        let dir = root.join("xtask/mock-ai");
        let body = serde_json::to_vec(&json!({
            "messages": [
                { "role": "system", "content": "Turn the document into an outline." },
                { "role": "user", "content": "<document>\nA document. MOCK_FILE=outline.md\n</document>" },
            ],
        }))
        .expect("json");
        let reply = reply_for(&body, &dir);
        assert_ne!(reply, "OK");
        assert!(reply.contains("# Safety"), "{reply}");
        assert!(reply.contains("## Welcome"), "{reply}");
        assert!(reply.contains("? What do you do"), "{reply}");
        assert!(reply.contains("[x] Leave"), "{reply}");
    }

    #[test]
    fn answers_vision_messages_with_a_mock_description() {
        let dir = scratch("vision");
        let body = serde_json::to_vec(&json!({
            "messages": [
                { "role": "system", "content": "Describe the image." },
                { "role": "user", "content": [
                    { "type": "text", "text": "<slide-context>Slide title: Banner</slide-context>" },
                    { "type": "image_url", "image_url": { "url": "data:image/png;base64,AAAA" } },
                ] },
            ],
        }))
        .expect("json");
        assert_eq!(reply_for(&body, &dir), "A mock description of the picture.");
    }

    #[test]
    fn the_alt_rule_file_still_works_on_array_content() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask has a parent");
        let dir = root.join("xtask/mock-ai");
        let body = serde_json::to_vec(&json!({
            "messages": [{ "role": "user", "content": [
                { "type": "text", "text": "MOCK_FILE=alt.txt" },
                { "type": "image_url", "image_url": { "url": "data:image/png;base64,AAAA" } },
            ] }],
        }))
        .expect("json");
        assert_eq!(reply_for(&body, &dir), "A mock description of the picture.");
    }

    #[test]
    fn answers_a_translation_batch_and_keeps_markers() {
        let dir = scratch("translate");
        let batch = json!([
            { "id": "course/title", "text": "Hello %learner%" },
            { "id": "slide/s/object/t/text", "text": "<g id=\"r0\">Hello </g><g id=\"r1\">world</g>" },
        ]);
        let body = serde_json::to_vec(&json!({
            "messages": [
                { "role": "system", "content": "Translate each text into fr." },
                { "role": "user", "content": serde_json::to_string(&batch).expect("json") },
            ],
        }))
        .expect("json");
        let reply = reply_for(&body, &dir);
        let value: Value = serde_json::from_str(&reply).expect("array");
        assert_eq!(value[0]["id"], "course/title");
        assert_eq!(value[0]["text"], "[fr] Hello %learner%");
        assert_eq!(
            value[1]["text"],
            "[fr] <g id=\"r0\">Hello </g><g id=\"r1\">world</g>"
        );
    }

    #[test]
    fn a_mock_drop_unit_is_left_out_of_the_translation_answer() {
        let dir = scratch("translate-drop");
        let batch = json!([
            { "id": "a", "text": "Keep this" },
            { "id": "b", "text": "MOCK_DROP this" },
        ]);
        let body = serde_json::to_vec(&json!({
            "messages": [
                { "role": "system", "content": "Translate each text into fr." },
                { "role": "user", "content": serde_json::to_string(&batch).expect("json") },
            ],
        }))
        .expect("json");
        let value: Value = serde_json::from_str(&reply_for(&body, &dir)).expect("array");
        assert_eq!(value.as_array().map(Vec::len), Some(1));
        assert_eq!(value[0]["id"], "a");
    }

    #[test]
    fn handles_options_and_cors() {
        let dir = scratch("cors");
        let response = handle(&dir, &request("OPTIONS", "/v1/chat/completions", b""));
        assert_eq!(response.status, 204);
        assert_eq!(
            response
                .headers
                .iter()
                .find(|(name, _)| name == "Access-Control-Allow-Origin")
                .map(|(_, value)| value.as_str()),
            Some("*")
        );

        let response = handle(&dir, &request("GET", "/v1/chat/completions", b""));
        assert_eq!(response.status, 404);
        assert!(
            response
                .headers
                .iter()
                .any(|(name, _)| name == "Access-Control-Allow-Methods")
        );
    }

    #[test]
    fn answers_a_chat_completion() {
        let dir = scratch("chat");
        let response = handle(
            &dir,
            &request("POST", "/v1/chat/completions", &body_for("Hello")),
        );
        assert_eq!(response.status, 200);
        let value: Value = serde_json::from_slice(&response.body).expect("json");
        assert_eq!(value["choices"][0]["message"]["content"], "OK");
        assert_eq!(value["choices"][0]["finish_reason"], "stop");
    }

    #[test]
    fn answers_transcription_with_timed_webvtt() {
        let dir = scratch("transcription");
        let body = concat!(
            "------b\r\nContent-Disposition: form-data; name=\"file\"; filename=\"a.mp3\"\r\n\r\n",
            "ID3\r\n",
            "------b\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\n",
            "whisper-1\r\n",
            "------b\r\nContent-Disposition: form-data; name=\"response_format\"\r\n\r\nvtt\r\n",
            "------b--\r\n"
        );
        let response = handle(
            &dir,
            &request("POST", "/v1/audio/transcriptions", body.as_bytes()),
        );
        assert_eq!(response.status, 200);
        assert!(
            String::from_utf8_lossy(&response.body).starts_with("WEBVTT"),
            "{}",
            String::from_utf8_lossy(&response.body)
        );
        assert!(String::from_utf8_lossy(&response.body).contains("-->"));
        assert_eq!(
            response
                .headers
                .iter()
                .find(|(name, _)| name == "Content-Type")
                .map(|(_, value)| value.as_str()),
            Some("text/vtt; charset=utf-8")
        );
    }

    #[test]
    fn answers_speech_with_a_tiny_mp3() {
        let dir = scratch("speech");
        let body = serde_json::to_vec(&json!({
            "model": "tts-1",
            "voice": "alloy",
            "input": "Hello.",
            "response_format": "mp3",
        }))
        .expect("json");
        let response = handle(&dir, &request("POST", "/v1/audio/speech", &body));
        assert_eq!(response.status, 200);
        assert!(response.body.starts_with(b"ID3"));
        assert!(response.body.windows(2).any(|pair| pair == [0xFF, 0xFB]));
        let mp3 = mock_mp3();
        assert_eq!(response.body, mp3);
        assert_eq!(mp3.len(), 10 + 2 * 417);
    }

    #[test]
    fn a_no_audio_model_answers_404_for_both_audio_routes() {
        let dir = scratch("no-audio");
        let transcription = concat!(
            "------b\r\nContent-Disposition: form-data; name=\"file\"; filename=\"a.mp3\"\r\n\r\n",
            "ID3\r\n",
            "------b\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\n",
            "no-audio\r\n",
            "------b--\r\n"
        );
        let response = handle(
            &dir,
            &request("POST", "/v1/audio/transcriptions", transcription.as_bytes()),
        );
        assert_eq!(response.status, 404);

        let body =
            serde_json::to_vec(&json!({ "model": "no-audio", "input": "Hi" })).expect("json");
        let response = handle(&dir, &request("POST", "/v1/audio/speech", &body));
        assert_eq!(response.status, 404);
    }

    #[test]
    fn audio_routes_keep_cors_and_options() {
        let dir = scratch("audio-cors");
        let response = handle(&dir, &request("OPTIONS", "/v1/audio/speech", b""));
        assert_eq!(response.status, 204);
        assert!(
            response
                .headers
                .iter()
                .any(|(name, _)| name == "Access-Control-Allow-Origin")
        );
        let response = handle(&dir, &request("GET", "/v1/audio/speech", b""));
        assert_eq!(response.status, 404);
        assert!(
            response
                .headers
                .iter()
                .any(|(name, _)| name == "Access-Control-Allow-Methods")
        );
    }
}
