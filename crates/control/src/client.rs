//! Control channel client for connecting to a running CourseCraft editor.

use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::time::Duration;

use serde_json::Value as Json;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const READ_TIMEOUT: Duration = Duration::from_secs(60);
const WRITE_TIMEOUT: Duration = Duration::from_secs(60);

/// Errors encountered by [`ControlClient`].
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("connection failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("server disconnected")]
    Disconnected,
    #[error("discovery file error: {0}")]
    Discovery(String),
    #[error("server error ({code}): {message}")]
    JsonRpc { code: i32, message: String },
}

/// Client for communicating with a running CourseCraft editor over the local control channel.
pub struct ControlClient {
    stream: TcpStream,
    reader: BufReader<TcpStream>,
    token: String,
    next_id: u64,
}

impl ControlClient {
    /// Connect directly to a given port using an authorization token.
    ///
    /// # Errors
    /// Returns [`ClientError::Io`] if connecting fails or times out after 2 seconds.
    pub fn connect(port: u16, token: &str) -> Result<Self, ClientError> {
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        let stream = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT)?;
        stream.set_read_timeout(Some(READ_TIMEOUT))?;
        stream.set_write_timeout(Some(WRITE_TIMEOUT))?;
        stream.set_nodelay(true)?;

        let reader_stream = stream.try_clone()?;
        let reader = BufReader::new(reader_stream);

        Ok(Self {
            stream,
            reader,
            token: token.to_string(),
            next_id: 1,
        })
    }

    /// Read `<config_dir>/control.json` and connect to the active editor instance.
    ///
    /// # Errors
    /// Returns [`ClientError::Discovery`] if `control.json` is missing or invalid,
    /// or [`ClientError::Io`] if the connection is refused or times out.
    pub fn connect_from_config(config_dir: &Path) -> Result<Self, ClientError> {
        let control_path = config_dir.join("control.json");
        let content = std::fs::read_to_string(&control_path).map_err(|e| {
            ClientError::Discovery(format!("failed to read {}: {e}", control_path.display()))
        })?;
        let val: Json = serde_json::from_str(&content).map_err(|e| {
            ClientError::Discovery(format!("invalid JSON in {}: {e}", control_path.display()))
        })?;

        let port =
            val.get("port").and_then(Json::as_u64).ok_or_else(|| {
                ClientError::Discovery("missing 'port' field in control.json".into())
            })? as u16;
        let token = val.get("token").and_then(Json::as_str).ok_or_else(|| {
            ClientError::Discovery("missing 'token' field in control.json".into())
        })?;

        Self::connect(port, token)
    }

    /// Send a JSON-RPC 2.0 request and return the unwrapped result.
    ///
    /// # Errors
    /// Returns [`ClientError::JsonRpc`] on server errors or [`ClientError::Io`] on transmission failure.
    pub fn call(&mut self, method: &str, params: Json) -> Result<Json, ClientError> {
        let mut map = match params {
            Json::Object(m) => m,
            Json::Null => serde_json::Map::new(),
            other => {
                let mut m = serde_json::Map::new();
                m.insert("value".to_string(), other);
                m
            }
        };
        map.insert("token".to_string(), Json::String(self.token.clone()));

        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);

        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": map,
        });

        let mut line = serde_json::to_string(&req)?;
        line.push('\n');

        self.stream.write_all(line.as_bytes())?;
        self.stream.flush()?;

        let mut resp_line = String::new();
        self.reader.read_line(&mut resp_line)?;
        if resp_line.is_empty() {
            return Err(ClientError::Disconnected);
        }

        let resp: Json = serde_json::from_str(&resp_line)?;
        if let Some(err) = resp.get("error") {
            let code = err.get("code").and_then(Json::as_i64).unwrap_or(-32000) as i32;
            let message = err
                .get("message")
                .and_then(Json::as_str)
                .unwrap_or("unknown error")
                .to_string();
            return Err(ClientError::JsonRpc { code, message });
        }

        let result = resp.get("result").cloned().unwrap_or(Json::Null);
        Ok(result)
    }
}
