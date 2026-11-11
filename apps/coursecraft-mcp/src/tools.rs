//! MCP tool definitions, JSON schemas, and execution handlers.

use serde_json::Value as Json;

use crate::backend::McpBackend;

/// Tool metadata exposed in `tools/list`.
pub struct ToolDef {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Json,
}

/// All available MCP tools.
#[must_use]
pub fn all_tools() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "command_list",
            description: "List available engine editing commands.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolDef {
            name: "command_run",
            description: "Execute a single engine command by id with parameters.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Command id, e.g. slide.add" },
                    "params": { "type": "object", "description": "Command parameters" }
                },
                "required": ["id"]
            }),
        },
        ToolDef {
            name: "command_batch",
            description: "Execute a batch of engine commands atomically with one undo step.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "steps": {
                        "type": "array",
                        "description": "List of command steps to execute",
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": { "type": "string" },
                                "params": { "type": "object" }
                            },
                            "required": ["id"]
                        }
                    }
                },
                "required": ["steps"]
            }),
        },
        ToolDef {
            name: "course_inspect",
            description: "Inspect the course document model or a value at a JSON pointer.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "pointer": {
                        "type": "string",
                        "description": "Optional RFC 6901 JSON pointer (e.g. /title or /scenes/0)"
                    }
                }
            }),
        },
        ToolDef {
            name: "validate",
            description: "Validate the course for structure and trigger errors.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolDef {
            name: "simulate",
            description: "Simulate player inputs on the course and return emitted effects.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "inputs": {
                        "type": "array",
                        "description": "Player inputs to feed sequentially",
                        "items": { "type": "object" }
                    },
                    "start_slide": {
                        "type": "string",
                        "description": "Optional start slide ID override"
                    }
                },
                "required": ["inputs"]
            }),
        },
        ToolDef {
            name: "selection_get",
            description: "Get the current slide, layer, and object selection.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolDef {
            name: "selection_set",
            description: "Set the current slide, layer, and object selection.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "slide": { "type": "string", "description": "Slide ID to select" },
                    "layer": {
                        "type": ["string", "null"],
                        "description": "Layer ID to select, or null to clear"
                    },
                    "object": {
                        "type": ["string", "null"],
                        "description": "Object ID to select, or null to clear"
                    }
                }
            }),
        },
        ToolDef {
            name: "undo",
            description: "Undo the last mutating command.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolDef {
            name: "redo",
            description: "Redo the last undone command.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolDef {
            name: "session_info",
            description: "Get session metadata, dirty state, revision, and undo availability.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolDef {
            name: "file_save",
            description: "Save the course to a .ccraft package or course.json folder.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Filesystem path to save to" }
                }
            }),
        },
        ToolDef {
            name: "file_open",
            description: "Open a course from a .ccraft file or course.json folder.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Filesystem path to load" },
                    "force": { "type": "boolean", "description": "Discard unsaved changes if dirty" }
                },
                "required": ["path"]
            }),
        },
        ToolDef {
            name: "render_preview",
            description: "Render a slide to PNG without a window and return an image preview and slide metadata.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "slide": {
                        "type": "string",
                        "description": "Optional slide ID to render (defaults to current selection)"
                    },
                    "width": {
                        "type": "integer",
                        "description": "Width in pixels (default 960, clamp 16..=4096)"
                    },
                    "mode": {
                        "type": "string",
                        "enum": ["authoring", "player"],
                        "description": "Rendering mode: authoring (static/edit preview) or player (interactive simulation state)"
                    },
                    "layers": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Specific layer IDs to compose on top"
                    },
                    "at_ms": {
                        "type": "integer",
                        "description": "Timestamp in milliseconds to tick to in player mode (default: slide duration)"
                    }
                }
            }),
        },
    ]
}

fn as_text_content(val: Json) -> Vec<Json> {
    let pretty_text = serde_json::to_string_pretty(&val).unwrap_or_default();
    vec![serde_json::json!({
        "type": "text",
        "text": pretty_text,
    })]
}

/// Execute a tool call against the backend.
pub fn call_tool(
    name: &str,
    arguments: &Json,
    backend: &mut dyn McpBackend,
) -> Result<Vec<Json>, String> {
    let empty_map = serde_json::Map::new();
    let args = arguments.as_object().unwrap_or(&empty_map);

    match name {
        "command_list" => backend
            .call("commands.list", serde_json::json!({}))
            .map(as_text_content),

        "command_run" => {
            let id = args
                .get("id")
                .and_then(Json::as_str)
                .ok_or_else(|| "missing argument 'id'".to_string())?;
            let params = args
                .get("params")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));
            backend
                .call(
                    "command.run",
                    serde_json::json!({ "id": id, "params": params }),
                )
                .map(as_text_content)
        }

        "command_batch" => {
            let steps = args
                .get("steps")
                .ok_or_else(|| "missing argument 'steps'".to_string())?;
            backend
                .call("command.batch", serde_json::json!({ "steps": steps }))
                .map(as_text_content)
        }

        "course_inspect" => {
            let pointer = args.get("pointer").and_then(Json::as_str);
            let mut params = serde_json::Map::new();
            if let Some(p) = pointer {
                params.insert("pointer".to_string(), Json::String(p.to_string()));
            }
            backend
                .call("course.get", Json::Object(params))
                .map(as_text_content)
        }

        "validate" => backend
            .call("course.validate", serde_json::json!({}))
            .map(as_text_content),

        "simulate" => {
            let inputs = args
                .get("inputs")
                .ok_or_else(|| "missing argument 'inputs'".to_string())?;
            let mut params = serde_json::Map::new();
            params.insert("inputs".to_string(), inputs.clone());
            if let Some(ss) = args.get("start_slide").and_then(Json::as_str) {
                params.insert("start_slide".to_string(), Json::String(ss.to_string()));
            }
            backend
                .call("preview.run_inputs", Json::Object(params))
                .map(as_text_content)
        }

        "selection_get" => backend
            .call("selection.get", serde_json::json!({}))
            .map(as_text_content),

        "selection_set" => {
            let mut params = serde_json::Map::new();
            if let Some(slide) = args.get("slide").and_then(Json::as_str) {
                params.insert("slide".to_string(), Json::String(slide.to_string()));
            }
            if let Some(layer) = args.get("layer") {
                params.insert("layer".to_string(), layer.clone());
            }
            if let Some(object) = args.get("object") {
                params.insert("object".to_string(), object.clone());
            }
            backend
                .call("selection.set", Json::Object(params))
                .map(as_text_content)
        }

        "undo" => backend
            .call("edit.undo", serde_json::json!({}))
            .map(as_text_content),

        "redo" => backend
            .call("edit.redo", serde_json::json!({}))
            .map(as_text_content),

        "session_info" => backend
            .call("session.info", serde_json::json!({}))
            .map(as_text_content),

        "file_save" => {
            let mut params = serde_json::Map::new();
            if let Some(path) = args.get("path").and_then(Json::as_str) {
                params.insert("path".to_string(), Json::String(path.to_string()));
            }
            backend
                .call("file.save", Json::Object(params))
                .map(as_text_content)
        }

        "file_open" => {
            let path = args
                .get("path")
                .and_then(Json::as_str)
                .ok_or_else(|| "missing argument 'path'".to_string())?;
            let force = args.get("force").and_then(Json::as_bool).unwrap_or(false);
            backend
                .call(
                    "file.open",
                    serde_json::json!({ "path": path, "force": force }),
                )
                .map(as_text_content)
        }

        "render_preview" => {
            let mut params = serde_json::Map::new();
            if let Some(slide) = args.get("slide").and_then(Json::as_str) {
                params.insert("slide".to_string(), Json::String(slide.to_string()));
            }
            let width = args.get("width").and_then(Json::as_u64).unwrap_or(960);
            params.insert("width".to_string(), Json::Number(width.into()));

            let mode_str = args
                .get("mode")
                .and_then(Json::as_str)
                .unwrap_or("authoring");
            params.insert("mode".to_string(), Json::String(mode_str.to_string()));

            if let Some(layers) = args.get("layers") {
                params.insert("layers".to_string(), layers.clone());
            }
            if let Some(at_ms) = args.get("at_ms").and_then(Json::as_u64) {
                params.insert("at_ms".to_string(), Json::Number(at_ms.into()));
            }

            let res = backend.call("render.slide", Json::Object(params))?;
            let data = res["png_base64"]
                .as_str()
                .ok_or_else(|| "missing png_base64 in render response".to_string())?;
            let w = res["width"].as_u64().unwrap_or(width);
            let h = res["height"].as_u64().unwrap_or(0);
            let title = res.get("title").and_then(Json::as_str).unwrap_or("Slide");

            Ok(vec![
                serde_json::json!({
                    "type": "image",
                    "data": data,
                    "mimeType": "image/png",
                }),
                serde_json::json!({
                    "type": "text",
                    "text": format!("{title} — {w}×{h}, {mode_str}"),
                }),
            ])
        }

        unknown => Err(format!("unknown tool: {unknown}")),
    }
}
