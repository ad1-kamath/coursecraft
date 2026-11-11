//! CourseCraft LTI 1.3 tool server (standalone app).
//!
//! A small HTTP/1.1 server that lets an LTI 1.3 platform launch a published
//! CourseCraft course and receive scores back through Assignment and Grade
//! Services. It speaks plain HTTP on loopback; the user's reverse proxy adds
//! HTTPS. The protocol itself lives in `coursecraft-lti`.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod http;
pub mod server;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use coursecraft_lti::ToolConfig;
use serde::Deserialize;

/// A configuration file larger than this is refused without parsing.
const MAX_CONFIG_BYTES: u64 = 1024 * 1024;
/// A private key file larger than this is refused without parsing.
const MAX_KEY_BYTES: u64 = 64 * 1024;

/// `lti.json`: one platform registration and the tool's own key.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub issuer: String,
    pub client_id: String,
    pub deployment_id: String,
    pub auth_login_url: String,
    pub auth_token_url: String,
    pub jwks_url: String,
    /// Public HTTPS origin plus path prefix, without a trailing slash.
    pub tool_url: String,
    /// PKCS#8 DER, or PEM with base64 between the `-----` lines.
    pub private_key_file: PathBuf,
    pub key_id: String,
    #[serde(default = "default_listen")]
    pub listen: String,
    /// A folder produced by `coursecraft-cli publish --standard lti -o <dir>`,
    /// or any folder with a published course. Relative paths resolve against
    /// the configuration file.
    #[serde(default = "default_course_dir")]
    pub course_dir: PathBuf,
    #[serde(skip)]
    base_dir: PathBuf,
}

/// Configuration failures, never a panic.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("config: {0}")]
    Io(String),
    #[error("config JSON: {0}")]
    Json(String),
    #[error("private key: {0}")]
    Key(String),
}

fn default_listen() -> String {
    "127.0.0.1:8787".into()
}

fn default_course_dir() -> PathBuf {
    PathBuf::from("./published")
}

impl Config {
    /// Read `lti.json`. Relative key and course paths resolve against its folder.
    ///
    /// # Errors
    /// Returns [`ConfigError`] when the file is missing, oversized, not valid
    /// JSON, or has unknown fields.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let bytes = std::fs::read(path).map_err(|e| ConfigError::Io(e.to_string()))?;
        if bytes.len() as u64 > MAX_CONFIG_BYTES {
            return Err(ConfigError::Io(format!(
                "file is larger than {MAX_CONFIG_BYTES} bytes"
            )));
        }
        let mut config: Self =
            serde_json::from_slice(&bytes).map_err(|e| ConfigError::Json(e.to_string()))?;
        config.base_dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();
        Ok(config)
    }

    /// Read the private key and turn this file into a [`ToolConfig`].
    ///
    /// # Errors
    /// Returns [`ConfigError`] when the key file is missing, oversized or not
    /// valid PKCS#8 DER/PEM.
    pub fn tool_config(&self) -> Result<ToolConfig, ConfigError> {
        let key_path = resolve(&self.base_dir, &self.private_key_file);
        let private_key_pkcs8 = read_private_key(&key_path)?;
        Ok(ToolConfig {
            issuer: self.issuer.clone(),
            client_id: self.client_id.clone(),
            deployment_id: self.deployment_id.clone(),
            auth_login_url: self.auth_login_url.clone(),
            auth_token_url: self.auth_token_url.clone(),
            jwks_url: self.jwks_url.clone(),
            tool_url: self.tool_url.clone(),
            private_key_pkcs8,
            key_id: self.key_id.clone(),
        })
    }

    /// The course folder, resolved against the configuration file.
    #[must_use]
    pub fn course_dir(&self) -> PathBuf {
        resolve(&self.base_dir, &self.course_dir)
    }
}

/// The three URLs and key id to paste into the platform registration.
#[must_use]
pub fn check_text(cfg: &ToolConfig) -> String {
    let base = cfg.tool_url.trim_end_matches('/');
    format!(
        "login  {base}/lti/login\nlaunch {base}/lti/launch\nJWKS   {base}/.well-known/jwks.json\nkey id {}\n",
        cfg.key_id
    )
}

fn resolve(base: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}

fn read_private_key(path: &Path) -> Result<Vec<u8>, ConfigError> {
    let bytes = std::fs::read(path).map_err(|e| ConfigError::Key(e.to_string()))?;
    if bytes.len() as u64 > MAX_KEY_BYTES {
        return Err(ConfigError::Key(format!(
            "file is larger than {MAX_KEY_BYTES} bytes"
        )));
    }
    if !bytes.starts_with(b"-----BEGIN") {
        return Ok(bytes);
    }
    let text =
        std::str::from_utf8(&bytes).map_err(|_| ConfigError::Key("PEM is not UTF-8".into()))?;
    let body: String = text
        .lines()
        .filter(|line| !line.trim_start().starts_with("-----"))
        .map(str::trim)
        .collect();
    STANDARD
        .decode(body)
        .map_err(|e| ConfigError::Key(format!("PEM base64: {e}")))
}
