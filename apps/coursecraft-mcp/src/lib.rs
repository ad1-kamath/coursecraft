//! CourseCraft MCP server over stdio.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

mod backend;
mod tools;

#[cfg(test)]
mod tests;

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;
use coursecraft_control::{HeadlessHost, default_config_dir};
use coursecraft_course::Course;
use coursecraft_engine::Session;
use serde_json::Value as Json;

pub use backend::{AttachedEditor, McpBackend};

const PROTOCOL_VERSION: &str = "2025-06-18";
const ACCEPTED_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

#[derive(Parser, Debug)]
#[command(
    name = "coursecraft-mcp",
    version,
    about = "CourseCraft MCP server over stdio"
)]
struct Args {
    /// Open a course file (.ccraft or course.json) in headless mode.
    #[arg(long)]
    course: Option<PathBuf>,

    /// Create a blank course with title in headless mode.
    #[arg(long)]
    create: Option<String>,

    /// Attach to a running CourseCraft editor.
    #[arg(long)]
    attach: bool,

    /// Config directory where control.json is located (used with --attach).
    #[arg(long)]
    config_dir: Option<PathBuf>,
}

fn load_course(path: &Path) -> Result<Course, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("ccraft"))
    {
        let proj = coursecraft_format::open_project(&bytes)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(proj.course)
    } else {
        let text = std::str::from_utf8(&bytes)
            .map_err(|e| format!("{}: non-utf8 course.json ({e})", path.display()))?;
        Course::from_json(text).map_err(|e| format!("{}: {e}", path.display()))
    }
}

/// Process JSON-RPC messages from `reader` and send responses to `writer`.
pub fn serve<R: BufRead, W: Write>(
    reader: R,
    mut writer: W,
    backend: &mut dyn McpBackend,
) -> std::io::Result<()> {
    for line_res in reader.lines() {
        let line = line_res?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let req: Json = match serde_json::from_str(trimmed) {
            Ok(j) => j,
            Err(e) => {
                let err_resp = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": Json::Null,
                    "error": { "code": -32700, "message": format!("Parse error: {e}") }
                });
                let mut out = serde_json::to_string(&err_resp)?;
                out.push('\n');
                writer.write_all(out.as_bytes())?;
                writer.flush()?;
                continue;
            }
        };

        if let Some(resp) = handle_message(&req, backend) {
            let mut out = serde_json::to_string(&resp)?;
            out.push('\n');
            writer.write_all(out.as_bytes())?;
            writer.flush()?;
        }
    }
    Ok(())
}

fn handle_message(req: &Json, backend: &mut dyn McpBackend) -> Option<Json> {
    let id = req.get("id").cloned().unwrap_or(Json::Null);
    let method = req.get("method").and_then(Json::as_str)?;

    match method {
        "notifications/initialized" => None,

        "initialize" => {
            let requested_version = req
                .get("params")
                .and_then(|p| p.get("protocolVersion"))
                .and_then(Json::as_str);

            let negotiated_version = match requested_version {
                Some(v) if ACCEPTED_VERSIONS.contains(&v) => v,
                _ => PROTOCOL_VERSION,
            };

            Some(serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "protocolVersion": negotiated_version,
                    "capabilities": {
                        "tools": {}
                    },
                    "serverInfo": {
                        "name": "coursecraft",
                        "version": env!("CARGO_PKG_VERSION")
                    },
                    "instructions": format!(
                        "CourseCraft eLearning authoring. Look before editing, and save with file_save. \
                         The CLI is a second route to the same commands: if a tool here fails, try {} \
                         (`guide` prints its full documentation).",
                        cli_hint()
                    )
                }
            }))
        }

        "ping" => Some(serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {}
        })),

        "tools/list" => {
            let tools_list: Vec<Json> = tools::all_tools()
                .into_iter()
                .map(|t| {
                    serde_json::json!({
                        "name": t.name,
                        "description": t.description,
                        "inputSchema": t.input_schema,
                    })
                })
                .collect();

            Some(serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "tools": tools_list
                }
            }))
        }

        "tools/call" => {
            let params = req.get("params");
            let name = params.and_then(|p| p.get("name")).and_then(Json::as_str);
            let arguments = params
                .and_then(|p| p.get("arguments"))
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));

            let Some(name) = name else {
                return Some(serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": { "code": -32602, "message": "missing tool name" }
                }));
            };

            let tool_result = tools::call_tool(name, &arguments, backend);
            match tool_result {
                Ok(content) => Some(serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "content": content
                    }
                })),
                Err(err_msg) => Some(serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "content": [
                            {
                                "type": "text",
                                "text": err_msg
                            }
                        ],
                        "isError": true
                    }
                })),
            }
        }

        _ => Some(serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": -32601,
                "message": format!("Method not found: {method}")
            }
        })),
    }
}

/// How to reach the CLI from this same install, so an agent has a second route.
#[must_use]
pub fn cli_hint() -> String {
    let exe = std::env::current_exe().ok();
    match exe {
        Some(path) if !path.to_string_lossy().contains("coursecraft-mcp") => {
            format!("\"{}\" cli", path.display())
        }
        _ => "coursecraft-cli".to_owned(),
    }
}

/// Parse `args` (the first is the program name) and serve MCP over stdio.
pub fn main_with_args(args: impl IntoIterator<Item = std::ffi::OsString>) -> ExitCode {
    let args = Args::parse_from(args);

    let mut backend: Box<dyn McpBackend> = if let Some(ref course_path) = args.course {
        match load_course(course_path) {
            Ok(course) => {
                let session = Session::new(course);
                Box::new(HeadlessHost::new(session, Some(course_path.clone())))
            }
            Err(e) => {
                eprintln!("failed to load course: {e}");
                return ExitCode::FAILURE;
            }
        }
    } else if let Some(ref title) = args.create {
        let session = Session::blank(title);
        Box::new(HeadlessHost::new(session, None))
    } else if args.attach {
        let config_dir = args.config_dir.or_else(default_config_dir);
        let Some(dir) = config_dir else {
            eprintln!("failed to determine config directory for --attach");
            return ExitCode::FAILURE;
        };
        // Connects on each call, so the app can open or restart at any time.
        Box::new(AttachedEditor::new(dir))
    } else {
        eprintln!("specify --course <path>, --create <title>, or --attach");
        return ExitCode::FAILURE;
    };

    let stdin = std::io::stdin();
    let stdout = std::io::stdout();

    if let Err(e) = serve(stdin.lock(), stdout.lock(), backend.as_mut()) {
        eprintln!("server error: {e}");
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}
