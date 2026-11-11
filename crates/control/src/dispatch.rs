//! JSON-RPC 2.0 protocol dispatch for CourseCraft control channel.

use std::path::Path;

use coursecraft_course::Severity;
use serde_json::Value as Json;

use crate::host::{ControlHost, HostError};

pub const CODE_PARSE_ERROR: i32 = -32700;
pub const CODE_INVALID_REQUEST: i32 = -32600;
pub const CODE_METHOD_NOT_FOUND: i32 = -32601;
pub const CODE_INVALID_PARAMS: i32 = -32602;
pub const CODE_ENGINE_ERROR: i32 = -32000;
pub const CODE_UNAUTHORIZED: i32 = -32001;
pub const CODE_UNSAVED_CHANGES: i32 = -32002;
pub const CODE_NOT_FOUND: i32 = -32004;

fn success(id: Json, result: Json) -> Json {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result,
    })
}

fn error(id: Json, code: i32, message: &str) -> Json {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message,
        }
    })
}

/// Pure method dispatch over a [`ControlHost`].
///
/// Handles JSON-RPC 2.0 framing, token authorization, method execution, and error code translation.
#[must_use]
pub fn dispatch(host: &mut dyn ControlHost, request: &Json, expected_token: &str) -> Json {
    let Some(obj) = request.as_object() else {
        return error(
            Json::Null,
            CODE_INVALID_REQUEST,
            "request must be a JSON object",
        );
    };

    let id = obj.get("id").cloned().unwrap_or(Json::Null);

    let is_jsonrpc_2 = obj
        .get("jsonrpc")
        .and_then(Json::as_str)
        .is_some_and(|v| v == "2.0");
    if !is_jsonrpc_2 {
        return error(
            id,
            CODE_INVALID_REQUEST,
            "missing or invalid jsonrpc version",
        );
    }

    let Some(method) = obj.get("method").and_then(Json::as_str) else {
        return error(id, CODE_INVALID_REQUEST, "missing method name");
    };

    let params = obj.get("params");

    // Token authorization
    if !expected_token.is_empty() {
        let token = params
            .and_then(Json::as_object)
            .and_then(|p| p.get("token"))
            .and_then(Json::as_str);
        if token != Some(expected_token) {
            return error(id, CODE_UNAUTHORIZED, "unauthorized");
        }
    }

    let empty_params = serde_json::Map::new();
    let p_map = params.and_then(Json::as_object).unwrap_or(&empty_params);

    match method {
        "commands.list" => {
            let list = host.commands();
            match serde_json::to_value(list) {
                Ok(v) => success(id, v),
                Err(e) => error(id, CODE_ENGINE_ERROR, &e.to_string()),
            }
        }

        "command.run" => {
            let Some(cmd_id) = p_map.get("id").and_then(Json::as_str) else {
                return error(id, CODE_INVALID_PARAMS, "missing command id");
            };
            let cmd_params = p_map
                .get("params")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));
            match host.run(cmd_id, &cmd_params) {
                Ok(res) => success(id, res),
                Err(e) => match e {
                    HostError::Engine(msg) => error(id, CODE_ENGINE_ERROR, &msg),
                    HostError::BadParams(msg) => error(id, CODE_INVALID_PARAMS, &msg),
                    HostError::NotFound(msg) => error(id, CODE_NOT_FOUND, &msg),
                    other => error(id, CODE_ENGINE_ERROR, &other.to_string()),
                },
            }
        }

        "command.batch" => handle_command_batch(host, p_map, &id),

        "course.get" => {
            let course = host.course();
            let course_val = match serde_json::to_value(course) {
                Ok(v) => v,
                Err(e) => return error(id, CODE_ENGINE_ERROR, &e.to_string()),
            };

            let pointer = p_map.get("pointer").and_then(Json::as_str);
            if let Some(ptr) = pointer {
                if ptr.is_empty() {
                    success(id, course_val)
                } else {
                    let normalized_ptr = if ptr.starts_with('/') {
                        ptr.to_string()
                    } else {
                        format!("/{ptr}")
                    };
                    match course_val.pointer(&normalized_ptr) {
                        Some(val) => success(id, val.clone()),
                        None => error(id, CODE_NOT_FOUND, &format!("pointer not found: {ptr}")),
                    }
                }
            } else {
                success(id, course_val)
            }
        }

        "course.validate" => {
            let issues = host.course().validate();
            let issues_json: Vec<Json> = issues
                .into_iter()
                .map(|i| {
                    serde_json::json!({
                        "severity": if i.severity == Severity::Error { "error" } else { "warning" },
                        "path": i.path,
                        "message": i.message,
                    })
                })
                .collect();
            success(id, Json::Array(issues_json))
        }

        "selection.get" => {
            let sel = host.selection();
            match serde_json::to_value(sel) {
                Ok(v) => success(id, v),
                Err(e) => error(id, CODE_ENGINE_ERROR, &e.to_string()),
            }
        }

        "selection.set" => {
            let slide = p_map
                .get("slide")
                .and_then(Json::as_str)
                .map(str::to_string);
            let layer = p_map.get("layer").map(|v| v.as_str().map(str::to_string));
            let object = p_map.get("object").map(|v| v.as_str().map(str::to_string));

            match host.set_selection(slide, layer, object) {
                Ok(sel) => match serde_json::to_value(sel) {
                    Ok(v) => success(id, v),
                    Err(e) => error(id, CODE_ENGINE_ERROR, &e.to_string()),
                },
                Err(e) => match e {
                    HostError::NotFound(msg) => error(id, CODE_NOT_FOUND, &msg),
                    HostError::BadParams(msg) => error(id, CODE_INVALID_PARAMS, &msg),
                    other => error(id, CODE_ENGINE_ERROR, &other.to_string()),
                },
            }
        }

        "preview.run_inputs" => {
            let Some(inputs_val) = p_map.get("inputs") else {
                return error(id, CODE_INVALID_PARAMS, "missing inputs array");
            };
            let inputs: Vec<coursecraft_logic::Input> =
                match serde_json::from_value(inputs_val.clone()) {
                    Ok(inputs) => inputs,
                    Err(e) => {
                        return error(id, CODE_INVALID_PARAMS, &format!("invalid inputs: {e}"));
                    }
                };
            let start_slide = p_map.get("start_slide").and_then(Json::as_str);

            match host.preview_inputs(&inputs, start_slide) {
                Ok(effects) => match serde_json::to_value(effects) {
                    Ok(effects_val) => success(id, serde_json::json!({ "effects": effects_val })),
                    Err(e) => error(id, CODE_ENGINE_ERROR, &e.to_string()),
                },
                Err(e) => error(id, CODE_ENGINE_ERROR, &e.to_string()),
            }
        }

        "file.save" => {
            let path_opt = p_map.get("path").and_then(Json::as_str).map(Path::new);
            match host.save(path_opt) {
                Ok(saved_path) => success(
                    id,
                    serde_json::json!({ "path": saved_path.to_string_lossy().to_string() }),
                ),
                Err(e) => match e {
                    HostError::BadParams(msg) => error(id, CODE_INVALID_PARAMS, &msg),
                    HostError::Io(msg) | HostError::Engine(msg) => {
                        error(id, CODE_ENGINE_ERROR, &msg)
                    }
                    other => error(id, CODE_ENGINE_ERROR, &other.to_string()),
                },
            }
        }

        "file.open" => {
            let Some(path_str) = p_map.get("path").and_then(Json::as_str) else {
                return error(id, CODE_INVALID_PARAMS, "missing file path");
            };
            let force = p_map.get("force").and_then(Json::as_bool).unwrap_or(false);

            match host.open(Path::new(path_str), force) {
                Ok(title) => success(id, serde_json::json!({ "title": title })),
                Err(HostError::UnsavedChanges) => {
                    error(id, CODE_UNSAVED_CHANGES, "unsaved changes")
                }
                Err(HostError::NotFound(msg)) => error(id, CODE_NOT_FOUND, &msg),
                Err(other) => error(id, CODE_ENGINE_ERROR, &other.to_string()),
            }
        }

        "session.info" => {
            let info = host.info();
            match serde_json::to_value(info) {
                Ok(v) => success(id, v),
                Err(e) => error(id, CODE_ENGINE_ERROR, &e.to_string()),
            }
        }

        "edit.undo" => match host.undo() {
            Ok(rev) => success(id, serde_json::json!({ "revision": rev })),
            Err(e) => error(id, CODE_ENGINE_ERROR, &e.to_string()),
        },

        "edit.redo" => match host.redo() {
            Ok(rev) => success(id, serde_json::json!({ "revision": rev })),
            Err(e) => error(id, CODE_ENGINE_ERROR, &e.to_string()),
        },

        "render.slide" => handle_render_slide(host, p_map, &id),

        _ => error(id, CODE_METHOD_NOT_FOUND, "method not found"),
    }
}

fn handle_command_batch(
    host: &mut dyn ControlHost,
    p_map: &serde_json::Map<String, Json>,
    id: &Json,
) -> Json {
    let Some(steps_val) = p_map.get("steps").and_then(Json::as_array) else {
        return error(id.clone(), CODE_INVALID_PARAMS, "missing steps array");
    };
    let mut steps = Vec::with_capacity(steps_val.len());
    for (idx, step) in steps_val.iter().enumerate() {
        let Some(step_obj) = step.as_object() else {
            return error(
                id.clone(),
                CODE_INVALID_PARAMS,
                &format!("step {idx} must be an object"),
            );
        };
        let cmd_id = step_obj
            .get("id")
            .or_else(|| step_obj.get("command"))
            .and_then(Json::as_str);
        let Some(cmd_id) = cmd_id else {
            return error(
                id.clone(),
                CODE_INVALID_PARAMS,
                &format!("step {idx} missing id/command"),
            );
        };
        let cmd_params = step_obj
            .get("params")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        steps.push((cmd_id.to_string(), cmd_params));
    }

    match host.batch(&steps) {
        Ok(results) => success(id.clone(), Json::Array(results)),
        Err(e) => match e {
            HostError::Engine(msg) => error(id.clone(), CODE_ENGINE_ERROR, &msg),
            HostError::BadParams(msg) => error(id.clone(), CODE_INVALID_PARAMS, &msg),
            HostError::NotFound(msg) => error(id.clone(), CODE_NOT_FOUND, &msg),
            other => error(id.clone(), CODE_ENGINE_ERROR, &other.to_string()),
        },
    }
}

fn handle_render_slide(
    host: &mut dyn ControlHost,
    p_map: &serde_json::Map<String, Json>,
    id: &Json,
) -> Json {
    let slide_id = p_map
        .get("slide")
        .and_then(Json::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| host.selection().slide);

    if slide_id.is_empty() {
        return error(id.clone(), CODE_INVALID_PARAMS, "missing or empty slide id");
    }

    let slide_title = host
        .course()
        .slide(&slide_id)
        .map(|s| s.title.clone())
        .unwrap_or_default();

    let width = if let Some(w_val) = p_map.get("width") {
        let Some(w) = w_val.as_u64() else {
            return error(id.clone(), CODE_INVALID_PARAMS, "width must be an integer");
        };
        if w > 4096 {
            return error(id.clone(), CODE_INVALID_PARAMS, "width cannot exceed 4096");
        }
        w as u32
    } else {
        coursecraft_render::DEFAULT_RENDER_WIDTH
    };

    let mode = if let Some(m_val) = p_map.get("mode").and_then(Json::as_str) {
        match m_val {
            "authoring" => coursecraft_render::RenderMode::Authoring,
            "player" => coursecraft_render::RenderMode::Player,
            other => {
                return error(
                    id.clone(),
                    CODE_INVALID_PARAMS,
                    &format!("unknown mode '{other}'; expected 'authoring' or 'player'"),
                );
            }
        }
    } else {
        coursecraft_render::RenderMode::Authoring
    };

    let mut layers = Vec::new();
    if let Some(layers_val) = p_map.get("layers").and_then(Json::as_array) {
        for l in layers_val {
            if let Some(s) = l.as_str() {
                layers.push(s.to_string());
            }
        }
    }

    let at_ms = p_map
        .get("at_ms")
        .and_then(Json::as_u64)
        .map(|ms| ms as u32);

    let path_opt = p_map.get("path").and_then(Json::as_str);

    let opts = coursecraft_render::RenderOptions {
        width,
        mode,
        layers,
        at_ms,
    };

    let img = match coursecraft_render::render_slide_rgba(
        host.course(),
        &slide_id,
        &opts,
        host.media(),
    ) {
        Ok(img) => img,
        Err(coursecraft_render::RenderError::SlideNotFound(msg)) => {
            return error(
                id.clone(),
                CODE_NOT_FOUND,
                &format!("slide not found: {msg}"),
            );
        }
        Err(coursecraft_render::RenderError::NotPlayable(issues)) => {
            return error(
                id.clone(),
                CODE_ENGINE_ERROR,
                &format!("course is not playable: {}", issues.join("; ")),
            );
        }
        Err(e) => return error(id.clone(), CODE_ENGINE_ERROR, &e.to_string()),
    };

    let png_bytes = match coursecraft_render::encode_png(&img) {
        Ok(bytes) => bytes,
        Err(e) => return error(id.clone(), CODE_ENGINE_ERROR, &e.to_string()),
    };

    if let Some(target_path) = path_opt {
        if let Err(e) = std::fs::write(target_path, &png_bytes) {
            return error(
                id.clone(),
                CODE_ENGINE_ERROR,
                &format!("failed to write PNG to '{target_path}': {e}"),
            );
        }
        success(
            id.clone(),
            serde_json::json!({
                "width": img.width,
                "height": img.height,
                "path": target_path,
                "title": slide_title,
            }),
        )
    } else {
        let png_base64 = encode_base64(&png_bytes);
        success(
            id.clone(),
            serde_json::json!({
                "width": img.width,
                "height": img.height,
                "png_base64": png_base64,
                "title": slide_title,
            }),
        )
    }
}

const BASE64_CHARS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard RFC 4648 base64 encoding without external dependencies.
#[must_use]
pub fn encode_base64(bytes: &[u8]) -> String {
    let mut out = String::with_capacity((bytes.len().saturating_add(2) / 3) * 4);
    let (chunks, rem) = bytes.as_chunks::<3>();
    for chunk in chunks {
        let b0 = chunk[0];
        let b1 = chunk[1];
        let b2 = chunk[2];
        out.push(BASE64_CHARS[(b0 >> 2) as usize] as char);
        out.push(BASE64_CHARS[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        out.push(BASE64_CHARS[(((b1 & 0x0F) << 2) | (b2 >> 6)) as usize] as char);
        out.push(BASE64_CHARS[(b2 & 0x3F) as usize] as char);
    }
    match rem.len() {
        1 => {
            let b0 = rem[0];
            out.push(BASE64_CHARS[(b0 >> 2) as usize] as char);
            out.push(BASE64_CHARS[((b0 & 0x03) << 4) as usize] as char);
            out.push('=');
            out.push('=');
        }
        2 => {
            let b0 = rem[0];
            let b1 = rem[1];
            out.push(BASE64_CHARS[(b0 >> 2) as usize] as char);
            out.push(BASE64_CHARS[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
            out.push(BASE64_CHARS[((b1 & 0x0F) << 2) as usize] as char);
            out.push('=');
        }
        _ => {}
    }
    out
}
