//! Backend interface for the MCP server.

use std::path::PathBuf;

use coursecraft_control::{ClientError, ControlClient, HeadlessHost, dispatch};
use serde_json::Value as Json;

/// Common backend trait for headless and attached MCP sessions.
pub trait McpBackend {
    /// Execute a control method and return the result or an error message.
    fn call(&mut self, method: &str, params: Json) -> Result<Json, String>;
}

impl McpBackend for HeadlessHost {
    fn call(&mut self, method: &str, params: Json) -> Result<Json, String> {
        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params,
        });
        let resp = dispatch(self, &req, "");
        if let Some(err) = resp.get("error") {
            let msg = err.get("message").and_then(Json::as_str).unwrap_or("error");
            Err(msg.to_string())
        } else {
            Ok(resp.get("result").cloned().unwrap_or(Json::Null))
        }
    }
}

/// A running editor, reached with a new connection on every call.
///
/// The server stays up whether or not the app is open, and picks up a restarted
/// app's new port and token. Nothing is ever retried, so a command can't run twice.
pub struct AttachedEditor {
    config_dir: PathBuf,
}

impl AttachedEditor {
    /// Reach the editor whose `control.json` lives in `config_dir`.
    #[must_use]
    pub fn new(config_dir: PathBuf) -> Self {
        Self { config_dir }
    }
}

impl McpBackend for AttachedEditor {
    fn call(&mut self, method: &str, params: Json) -> Result<Json, String> {
        let mut client = ControlClient::connect_from_config(&self.config_dir).map_err(|_| {
            format!(
                "CourseCraft isn't reachable. Open the desktop app and check Help > Allow automation, \
                 then try again. To work on a course file without the app, use {} (`guide` explains how).",
                crate::cli_hint()
            )
        })?;
        client.call(method, params).map_err(|e| match e {
            ClientError::JsonRpc { message, .. } => message,
            other => other.to_string(),
        })
    }
}
