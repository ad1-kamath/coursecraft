//! Pure agent-connection rules: which agents exist, the command lines that add
//! and remove them, and the JSON files Claude Desktop and Cursor read.
//!
//! Nothing here touches a process or the filesystem. The desktop app owns the
//! commands and files behind an `AgentHost`, so this module stays wasm-safe and
//! can be unit tested on every platform.

use std::fmt;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

/// The key CourseCraft uses in every agent's MCP list.
pub const SERVER_KEY: &str = "coursecraft";

/// The name of the backup made before an agent's settings file is written.
pub const BACKUP_SUFFIX: &str = ".coursecraft-backup";

/// An agent CourseCraft can connect to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentId {
    /// Anthropic's Claude Code CLI.
    ClaudeCode,
    /// OpenAI's Codex CLI.
    Codex,
    /// The Claude desktop app, which reads a JSON file.
    ClaudeDesktop,
    /// The Cursor editor, which reads a JSON file.
    Cursor,
    /// Google's Gemini CLI.
    GeminiCli,
}

impl AgentId {
    /// Every agent, in the order the Agents tab shows them.
    pub const ALL: [Self; 5] = [
        Self::ClaudeCode,
        Self::Codex,
        Self::ClaudeDesktop,
        Self::Cursor,
        Self::GeminiCli,
    ];

    /// The name shown to the author.
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::ClaudeCode => "Claude Code",
            Self::Codex => "Codex",
            Self::ClaudeDesktop => "Claude Desktop",
            Self::Cursor => "Cursor",
            Self::GeminiCli => "Gemini CLI",
        }
    }

    /// The vendor page that explains how to install it.
    #[must_use]
    pub const fn install_url(self) -> &'static str {
        match self {
            Self::ClaudeCode => "https://docs.anthropic.com/en/docs/claude-code/setup",
            Self::Codex => "https://developers.openai.com/codex/cli/",
            Self::ClaudeDesktop => "https://claude.ai/download",
            Self::Cursor => "https://www.cursor.com/",
            Self::GeminiCli => "https://github.com/google-gemini/gemini-cli",
        }
    }

    /// The program name to look for, for the three command-line agents.
    #[must_use]
    pub const fn program(self) -> Option<&'static str> {
        match self {
            Self::ClaudeCode => Some("claude"),
            Self::Codex => Some("codex"),
            Self::GeminiCli => Some("gemini"),
            Self::ClaudeDesktop | Self::Cursor => None,
        }
    }

    /// True for the three command-line agents.
    #[must_use]
    pub const fn is_cli(self) -> bool {
        self.program().is_some()
    }

    /// True for the two agents configured with a JSON file.
    #[must_use]
    pub const fn uses_config_file(self) -> bool {
        matches!(self, Self::ClaudeDesktop | Self::Cursor)
    }
}

/// What the author asked an agent row to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentAction {
    /// Add CourseCraft to the agent's MCP list.
    Connect,
    /// Ask the agent whether CourseCraft is connected.
    Test,
    /// Remove CourseCraft from the agent's MCP list.
    Disconnect,
}

/// What an agent row is showing.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum AgentStatus {
    /// A check is in flight.
    #[default]
    Checking,
    /// The CLI or app isn't on this computer.
    NotInstalled,
    /// Installed, but CourseCraft has not been added.
    Found,
    /// CourseCraft is in the agent's MCP list.
    Connected,
    /// A command failed; the sentence is safe to show.
    Problem(String),
}

impl AgentStatus {
    /// The short status word shown in the row.
    #[must_use]
    pub fn label(&self) -> &str {
        match self {
            Self::Checking => "Checking…",
            Self::NotInstalled => "Not installed",
            Self::Found => "Installed, not connected",
            Self::Connected => "Connected",
            Self::Problem(_) => "Problem",
        }
    }
}

/// The full command line for adding CourseCraft, program first.
///
/// Returns `None` for the two file-configured agents.
#[must_use]
pub fn add_argv(id: AgentId, exe: &str) -> Option<Vec<String>> {
    let argv = match id {
        AgentId::ClaudeCode => vec![
            "claude",
            "mcp",
            "add",
            "--transport",
            "stdio",
            "--scope",
            "user",
            SERVER_KEY,
            "--",
            exe,
            "mcp",
            "--attach",
        ],
        AgentId::Codex => vec![
            "codex", "mcp", "add", SERVER_KEY, "--", exe, "mcp", "--attach",
        ],
        AgentId::GeminiCli => vec![
            "gemini", "mcp", "add", "--scope", "user", SERVER_KEY, exe, "mcp", "--attach",
        ],
        AgentId::ClaudeDesktop | AgentId::Cursor => return None,
    };
    Some(argv.into_iter().map(str::to_owned).collect())
}

/// The full command line that asks whether CourseCraft is connected.
///
/// Gemini CLI has no "get", so its status comes from `mcp list`.
#[must_use]
pub fn status_argv(id: AgentId) -> Option<Vec<String>> {
    let argv = match id {
        AgentId::ClaudeCode => vec!["claude", "mcp", "get", SERVER_KEY],
        AgentId::Codex => vec!["codex", "mcp", "get", SERVER_KEY],
        AgentId::GeminiCli => vec!["gemini", "mcp", "list"],
        AgentId::ClaudeDesktop | AgentId::Cursor => return None,
    };
    Some(argv.into_iter().map(str::to_owned).collect())
}

/// The full command line that removes CourseCraft.
#[must_use]
pub fn remove_argv(id: AgentId) -> Option<Vec<String>> {
    let argv = match id {
        AgentId::ClaudeCode => {
            vec!["claude", "mcp", "remove", "--scope", "user", SERVER_KEY]
        }
        AgentId::Codex => vec!["codex", "mcp", "remove", SERVER_KEY],
        AgentId::GeminiCli => {
            vec!["gemini", "mcp", "remove", "--scope", "user", SERVER_KEY]
        }
        AgentId::ClaudeDesktop | AgentId::Cursor => return None,
    };
    Some(argv.into_iter().map(str::to_owned).collect())
}

/// Whether a status command's output says CourseCraft is connected.
///
/// Claude Code and Codex report it through the exit code. Gemini CLI has no
/// `get`, so its list output must name `coursecraft`.
#[must_use]
pub fn output_says_connected(id: AgentId, exit_ok: bool, stdout: &str) -> bool {
    if !exit_ok {
        return false;
    }
    match id {
        AgentId::GeminiCli => stdout.to_ascii_lowercase().contains(SERVER_KEY),
        AgentId::ClaudeCode | AgentId::Codex => true,
        AgentId::ClaudeDesktop | AgentId::Cursor => false,
    }
}

/// A problem with an agent settings file. Nothing has been written when this is
/// returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeError {
    /// The text is not JSON.
    InvalidJson,
    /// The root or its `mcpServers` value is not a JSON object.
    NotAnObject,
}

impl fmt::Display for MergeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidJson => {
                f.write_str("The settings file isn't valid JSON, so CourseCraft left it unchanged.")
            }
            Self::NotAnObject => f.write_str(
                "The settings file's mcpServers entry isn't a JSON object, so CourseCraft left it \
                 unchanged.",
            ),
        }
    }
}

impl std::error::Error for MergeError {}

fn parse_root(json_text: &str) -> Result<Map<String, Value>, MergeError> {
    if json_text.trim().is_empty() {
        return Ok(Map::new());
    }
    match serde_json::from_str::<Value>(json_text) {
        Ok(Value::Object(map)) => Ok(map),
        Ok(_) => Err(MergeError::NotAnObject),
        Err(_) => Err(MergeError::InvalidJson),
    }
}

fn servers_mut(root: &mut Map<String, Value>) -> Result<&mut Map<String, Value>, MergeError> {
    let value = root
        .entry("mcpServers".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    match value {
        Value::Object(servers) => Ok(servers),
        _ => Err(MergeError::NotAnObject),
    }
}

fn to_json(root: Map<String, Value>) -> Result<String, MergeError> {
    serde_json::to_string_pretty(&Value::Object(root)).map_err(|_| MergeError::InvalidJson)
}

/// Add CourseCraft's MCP entry, keeping every other key and server.
///
/// Empty or missing text starts as `{}`. A file that does not parse, or whose
/// `mcpServers` is not an object, is refused and must not be written.
///
/// # Errors
/// Returns [`MergeError`] for text that cannot be merged safely.
pub fn merge_mcp_server(json_text: &str, exe: &str) -> Result<String, MergeError> {
    let mut root = parse_root(json_text)?;
    let servers = servers_mut(&mut root)?;
    servers.insert(
        SERVER_KEY.to_string(),
        json!({
            "command": exe,
            "args": ["mcp", "--attach"],
        }),
    );
    to_json(root)
}

/// Remove only CourseCraft's MCP entry, keeping every other key and server.
///
/// Empty or missing text returns an empty object. A file that does not parse,
/// or whose `mcpServers` is not an object, is refused.
///
/// # Errors
/// Returns [`MergeError`] for text that cannot be changed safely.
pub fn remove_mcp_server(json_text: &str) -> Result<String, MergeError> {
    let mut root = parse_root(json_text)?;
    if let Some(servers) = root.get_mut("mcpServers") {
        match servers {
            Value::Object(map) => {
                map.remove(SERVER_KEY);
            }
            _ => return Err(MergeError::NotAnObject),
        }
    }
    to_json(root)
}

/// Whether a settings file currently lists CourseCraft.
///
/// # Errors
/// Returns [`MergeError`] when the text cannot be read safely.
pub fn file_has_mcp_server(json_text: &str) -> Result<bool, MergeError> {
    let root = parse_root(json_text)?;
    match root.get("mcpServers") {
        None => Ok(false),
        Some(Value::Object(servers)) => Ok(servers.contains_key(SERVER_KEY)),
        Some(_) => Err(MergeError::NotAnObject),
    }
}

/// The platform whose paths and executable names to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    /// macOS.
    Macos,
    /// Linux and other Unix-like systems.
    Linux,
    /// Windows.
    Windows,
}

impl Platform {
    /// The separator between `PATH` entries.
    #[must_use]
    pub const fn path_separator(self) -> char {
        match self {
            Self::Windows => ';',
            Self::Macos | Self::Linux => ':',
        }
    }
}

fn push_unique(dirs: &mut Vec<PathBuf>, dir: PathBuf) {
    if !dir.as_os_str().is_empty() && !dirs.iter().any(|existing| existing == &dir) {
        dirs.push(dir);
    }
}

/// The directories to search for a CLI, most likely first.
///
/// `path_env` is the process's `PATH` value. The `~/.nvm/versions/node` root is
/// included; the app expands each installed version's `bin` directory.
#[must_use]
pub fn search_dirs(home: &Path, path_env: &str, platform: Platform) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for entry in path_env.split(platform.path_separator()) {
        let entry = entry.trim();
        if !entry.is_empty() {
            push_unique(&mut dirs, PathBuf::from(entry));
        }
    }

    push_unique(&mut dirs, home.join(".local").join("bin"));
    push_unique(&mut dirs, home.join(".claude").join("local"));
    match platform {
        Platform::Macos => {
            push_unique(&mut dirs, PathBuf::from("/opt/homebrew/bin"));
            push_unique(&mut dirs, PathBuf::from("/usr/local/bin"));
        }
        Platform::Linux => {
            push_unique(&mut dirs, PathBuf::from("/opt/homebrew/bin"));
            push_unique(&mut dirs, PathBuf::from("/usr/local/bin"));
        }
        Platform::Windows => {
            push_unique(&mut dirs, home.join("AppData").join("Roaming").join("npm"));
        }
    }
    push_unique(&mut dirs, home.join(".npm-global").join("bin"));
    push_unique(&mut dirs, home.join(".bun").join("bin"));
    push_unique(&mut dirs, home.join(".volta").join("bin"));
    push_unique(&mut dirs, home.join(".nvm").join("versions").join("node"));
    dirs
}

/// Expand each installed Node version under `~/.nvm/versions/node` to its `bin`.
#[must_use]
pub fn nvm_bin_dirs(home: &Path, versions: &[String]) -> Vec<PathBuf> {
    versions
        .iter()
        .filter(|version| !version.trim().is_empty())
        .map(|version| {
            home.join(".nvm")
                .join("versions")
                .join("node")
                .join(version)
                .join("bin")
        })
        .collect()
}

/// The executable file names to try in each directory.
#[must_use]
pub fn cli_file_names(program: &str, platform: Platform) -> Vec<String> {
    match platform {
        Platform::Windows => vec![
            format!("{program}.exe"),
            format!("{program}.cmd"),
            format!("{program}.bat"),
            program.to_string(),
        ],
        Platform::Macos | Platform::Linux => vec![program.to_string()],
    }
}

/// The JSON file Claude Desktop or Cursor reads, or `None` for a CLI agent.
///
/// `appdata` is `%APPDATA%` on Windows. When it is present, Claude Desktop's
/// file is under it; otherwise the macOS location is used.
#[must_use]
pub fn config_path(id: AgentId, home: &Path, appdata: Option<&Path>) -> Option<PathBuf> {
    match id {
        AgentId::ClaudeDesktop => Some(match appdata {
            Some(appdata) => appdata.join("Claude").join("claude_desktop_config.json"),
            None => home
                .join("Library")
                .join("Application Support")
                .join("Claude")
                .join("claude_desktop_config.json"),
        }),
        AgentId::Cursor => Some(home.join(".cursor").join("mcp.json")),
        AgentId::ClaudeCode | AgentId::Codex | AgentId::GeminiCli => None,
    }
}

/// The app's own folder, used to decide whether it is installed.
#[must_use]
pub fn app_dir(id: AgentId, home: &Path, appdata: Option<&Path>) -> Option<PathBuf> {
    config_path(id, home, appdata).and_then(|path| path.parent().map(Path::to_path_buf))
}

/// The backup path next to an agent settings file.
#[must_use]
pub fn backup_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("settings");
    path.with_file_name(format!("{name}{BACKUP_SUFFIX}"))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use serde_json::Value;

    const EXE: &str = "/Applications/CourseCraft.app/Contents/MacOS/CourseCraft";

    #[test]
    fn every_agent_has_a_name_and_install_page() {
        for id in AgentId::ALL {
            assert!(!id.display_name().is_empty());
            assert!(id.install_url().starts_with("https://"));
        }
        assert!(AgentId::ClaudeCode.is_cli());
        assert!(AgentId::ClaudeDesktop.uses_config_file());
        assert!(AgentId::Cursor.uses_config_file());
    }

    #[test]
    fn claude_code_argv_is_exact() {
        assert_eq!(
            add_argv(AgentId::ClaudeCode, EXE).expect("add"),
            vec![
                "claude",
                "mcp",
                "add",
                "--transport",
                "stdio",
                "--scope",
                "user",
                "coursecraft",
                "--",
                EXE,
                "mcp",
                "--attach",
            ]
        );
        assert_eq!(
            status_argv(AgentId::ClaudeCode).expect("get"),
            vec!["claude", "mcp", "get", "coursecraft"]
        );
        assert_eq!(
            remove_argv(AgentId::ClaudeCode).expect("remove"),
            vec!["claude", "mcp", "remove", "--scope", "user", "coursecraft"]
        );
    }

    #[test]
    fn codex_argv_is_exact() {
        assert_eq!(
            add_argv(AgentId::Codex, EXE).expect("add"),
            vec![
                "codex",
                "mcp",
                "add",
                "coursecraft",
                "--",
                EXE,
                "mcp",
                "--attach",
            ]
        );
        assert_eq!(
            status_argv(AgentId::Codex).expect("get"),
            vec!["codex", "mcp", "get", "coursecraft"]
        );
        assert_eq!(
            remove_argv(AgentId::Codex).expect("remove"),
            vec!["codex", "mcp", "remove", "coursecraft"]
        );
    }

    #[test]
    fn gemini_cli_argv_is_exact() {
        assert_eq!(
            add_argv(AgentId::GeminiCli, EXE).expect("add"),
            vec![
                "gemini",
                "mcp",
                "add",
                "--scope",
                "user",
                "coursecraft",
                EXE,
                "mcp",
                "--attach",
            ]
        );
        assert_eq!(
            status_argv(AgentId::GeminiCli).expect("list"),
            vec!["gemini", "mcp", "list"]
        );
        assert_eq!(
            remove_argv(AgentId::GeminiCli).expect("remove"),
            vec!["gemini", "mcp", "remove", "--scope", "user", "coursecraft"]
        );
    }

    #[test]
    fn file_agents_have_no_commands() {
        for id in [AgentId::ClaudeDesktop, AgentId::Cursor] {
            assert!(add_argv(id, EXE).is_none());
            assert!(status_argv(id).is_none());
            assert!(remove_argv(id).is_none());
        }
    }

    #[test]
    fn parses_cli_status_output() {
        assert!(output_says_connected(AgentId::ClaudeCode, true, ""));
        assert!(!output_says_connected(AgentId::ClaudeCode, false, ""));
        assert!(output_says_connected(AgentId::Codex, true, "anything"));
        assert!(output_says_connected(
            AgentId::GeminiCli,
            true,
            "coursecraft: /path/to/CourseCraft"
        ));
        assert!(!output_says_connected(
            AgentId::GeminiCli,
            true,
            "other: /path/to/other"
        ));
        assert!(!output_says_connected(
            AgentId::GeminiCli,
            false,
            "coursecraft"
        ));
    }

    #[test]
    fn merge_starts_from_an_empty_or_missing_file() {
        for text in ["", "   \n"] {
            let merged = merge_mcp_server(text, EXE).expect("merge");
            let value: Value = serde_json::from_str(&merged).expect("json");
            assert_eq!(value["mcpServers"]["coursecraft"]["command"], EXE);
            assert_eq!(value["mcpServers"]["coursecraft"]["args"][0], "mcp");
            assert_eq!(value["mcpServers"]["coursecraft"]["args"][1], "--attach");
        }
    }

    #[test]
    fn merge_keeps_other_servers_and_top_level_keys() {
        let old = r#"{
            "theme": "dark",
            "mcpServers": {
                "other": {"command": "other", "args": ["serve"]}
            }
        }"#;
        let merged = merge_mcp_server(old, EXE).expect("merge");
        let value: Value = serde_json::from_str(&merged).expect("json");
        assert_eq!(value["theme"], "dark");
        assert_eq!(value["mcpServers"]["other"]["command"], "other");
        assert_eq!(value["mcpServers"]["other"]["args"][0], "serve");
        assert_eq!(value["mcpServers"]["coursecraft"]["command"], EXE);
    }

    #[test]
    fn merge_replaces_the_existing_coursecraft_entry() {
        let old = r#"{"mcpServers":{"coursecraft":{"command":"/old/path","args":[]}}}"#;
        let merged = merge_mcp_server(old, EXE).expect("merge");
        let value: Value = serde_json::from_str(&merged).expect("json");
        assert_eq!(value["mcpServers"]["coursecraft"]["command"], EXE);
        assert_eq!(value["mcpServers"]["coursecraft"]["args"][1], "--attach");
    }

    #[test]
    fn merge_refuses_broken_json_and_non_object_mcp_servers() {
        for text in [
            "{",
            "not json",
            "[]",
            r#"{"mcpServers": []}"#,
            r#"{"mcpServers": "nope"}"#,
            r#"{"mcpServers": null}"#,
        ] {
            assert!(
                merge_mcp_server(text, EXE).is_err(),
                "{text:?} must be refused"
            );
        }
    }

    #[test]
    fn remove_leaves_other_servers_and_keys() {
        let old = r#"{
            "theme": "dark",
            "mcpServers": {
                "coursecraft": {"command": "coursecraft"},
                "other": {"command": "other"}
            }
        }"#;
        let cleaned = remove_mcp_server(old).expect("remove");
        let value: Value = serde_json::from_str(&cleaned).expect("json");
        assert_eq!(value["theme"], "dark");
        assert!(value["mcpServers"].get("coursecraft").is_none());
        assert_eq!(value["mcpServers"]["other"]["command"], "other");
    }

    #[test]
    fn remove_from_an_empty_file_is_an_empty_object() {
        let cleaned = remove_mcp_server("").expect("remove");
        let value: Value = serde_json::from_str(&cleaned).expect("json");
        assert_eq!(value, json!({}));
    }

    #[test]
    fn remove_refuses_broken_json_and_non_object_mcp_servers() {
        assert_eq!(remove_mcp_server("{").unwrap_err(), MergeError::InvalidJson);
        assert_eq!(
            remove_mcp_server(r#"{"mcpServers": []}"#).unwrap_err(),
            MergeError::NotAnObject
        );
    }

    #[test]
    fn file_status_reads_only_the_coursecraft_entry() {
        let text = r#"{"mcpServers":{"coursecraft":{"command":"x"},"other":{}}}"#;
        assert!(file_has_mcp_server(text).expect("status"));
        assert!(!file_has_mcp_server("{}").expect("empty"));
        assert!(file_has_mcp_server("{").is_err());
        assert!(file_has_mcp_server(r#"{"mcpServers":[]}"#).is_err());
    }

    #[test]
    fn macos_directories_follow_the_plan() {
        let home = Path::new("/Users/ann");
        let dirs = search_dirs(home, "/usr/bin:/bin", Platform::Macos);
        assert_eq!(dirs[0], PathBuf::from("/usr/bin"));
        assert_eq!(dirs[1], PathBuf::from("/bin"));
        for expected in [
            home.join(".local/bin"),
            home.join(".claude/local"),
            PathBuf::from("/opt/homebrew/bin"),
            PathBuf::from("/usr/local/bin"),
            home.join(".npm-global/bin"),
            home.join(".bun/bin"),
            home.join(".volta/bin"),
            home.join(".nvm/versions/node"),
        ] {
            assert!(dirs.contains(&expected), "missing {}", expected.display());
        }
    }

    #[test]
    fn linux_directories_follow_the_plan() {
        let home = Path::new("/home/ann");
        let dirs = search_dirs(home, "/usr/bin", Platform::Linux);
        assert_eq!(dirs[0], PathBuf::from("/usr/bin"));
        for expected in [
            home.join(".local/bin"),
            home.join(".claude/local"),
            PathBuf::from("/opt/homebrew/bin"),
            PathBuf::from("/usr/local/bin"),
            home.join(".npm-global/bin"),
            home.join(".bun/bin"),
            home.join(".volta/bin"),
            home.join(".nvm/versions/node"),
        ] {
            assert!(dirs.contains(&expected), "missing {}", expected.display());
        }
    }

    #[test]
    fn windows_directories_follow_the_plan() {
        let home = Path::new(r"C:\Users\ann");
        let dirs = search_dirs(home, r"C:\Windows\system32;C:\Tools", Platform::Windows);
        assert_eq!(dirs[0], PathBuf::from(r"C:\Windows\system32"));
        assert_eq!(dirs[1], PathBuf::from(r"C:\Tools"));
        for expected in [
            home.join(".local/bin"),
            home.join(".claude/local"),
            home.join("AppData/Roaming/npm"),
            home.join(".npm-global/bin"),
            home.join(".bun/bin"),
            home.join(".volta/bin"),
            home.join(".nvm/versions/node"),
        ] {
            assert!(dirs.contains(&expected), "missing {}", expected.display());
        }
    }

    #[test]
    fn nvm_versions_expand_to_bin_directories() {
        let home = Path::new("/Users/ann");
        let dirs = nvm_bin_dirs(home, &["v20.1.0".to_string(), "".to_string()]);
        assert_eq!(
            dirs,
            vec![PathBuf::from("/Users/ann/.nvm/versions/node/v20.1.0/bin")]
        );
    }

    #[test]
    fn windows_tries_cmd_and_exe_first() {
        let names = cli_file_names("claude", Platform::Windows);
        assert_eq!(names[0], "claude.exe");
        assert!(names.contains(&"claude.cmd".to_string()));
        assert_eq!(cli_file_names("claude", Platform::Macos), vec!["claude"]);
    }

    #[test]
    fn config_paths_use_home_and_appdata() {
        let home = Path::new("/Users/ann");
        assert_eq!(
            config_path(AgentId::Cursor, home, None).expect("cursor"),
            home.join(".cursor/mcp.json")
        );
        assert_eq!(
            config_path(AgentId::ClaudeDesktop, home, None).expect("claude"),
            home.join("Library/Application Support/Claude/claude_desktop_config.json")
        );
        assert_eq!(
            config_path(
                AgentId::ClaudeDesktop,
                Path::new(r"C:\Users\ann"),
                Some(Path::new(r"C:\Users\ann\AppData\Roaming")),
            )
            .expect("claude windows"),
            Path::new(r"C:\Users\ann\AppData\Roaming")
                .join("Claude")
                .join("claude_desktop_config.json")
        );
        assert!(config_path(AgentId::ClaudeCode, home, None).is_none());
    }

    #[test]
    fn backup_name_sits_next_to_the_file() {
        assert_eq!(
            backup_path(Path::new("/Users/ann/.cursor/mcp.json")),
            PathBuf::from("/Users/ann/.cursor/mcp.json.coursecraft-backup")
        );
    }
}
