//! Desktop agent detection and connection, behind an [`AgentHost`] seam.
//!
//! The pure rules (command lines, JSON merging, search paths) live in
//! `coursecraft_ai::agents`. This module runs them: it finds CLIs, runs their
//! commands with a timeout, and reads and writes the two settings files.
//! Tests use [`FakeHost`]; no test starts a process or writes a real file.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::{Duration, Instant};

use coursecraft_ai::agent_chat::{
    AiSource, build_prompt, chat_argv, failure_sentence, output_excerpt, parse_claude_answer,
    parse_codex_answer,
};
use coursecraft_ai::agents::{
    AgentAction, AgentId, AgentStatus, MergeError, Platform, add_argv, app_dir, backup_path,
    cli_file_names, config_path, file_has_mcp_server, merge_mcp_server, nvm_bin_dirs,
    output_says_connected, remove_argv, remove_mcp_server, search_dirs, status_argv,
};
use coursecraft_ai::{ChatReply, ChatRequest};

/// How long one agent command may run before it is killed.
pub const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);

/// How long a chat request may run before the agent is killed.
pub const CHAT_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// Most bytes of a command's output this app keeps.
pub const MAX_OUTPUT: usize = 64 * 1024;

/// Most characters of a command's output a problem sentence quotes.
pub const MAX_EXCERPT: usize = 200;

/// A problem starting or running a command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostError {
    /// The command ran longer than its timeout and was killed.
    Timeout,
    /// The author cancelled the command, so it was killed.
    Cancelled,
    /// The command could not be started, or a file could not be read or written.
    Io(String),
}

impl std::fmt::Display for HostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Timeout => f.write_str("The command took too long and was stopped."),
            Self::Cancelled => f.write_str("The command was cancelled."),
            Self::Io(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for HostError {}

/// What a command printed and whether it exited successfully.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    /// True when the exit status was success.
    pub success: bool,
    /// Up to [`MAX_OUTPUT`] bytes of standard output, lossily decoded.
    pub stdout: String,
    /// Up to [`MAX_OUTPUT`] bytes of standard error, lossily decoded.
    pub stderr: String,
}

/// The process and file operations agent support needs. Tests replace it.
pub trait AgentHost: Send + Sync {
    /// Find `program` in `dirs`, trying the platform's executable names.
    fn find_cli(&self, program: &str, dirs: &[PathBuf], platform: Platform) -> Option<PathBuf>;

    /// Run `argv` (program first), with `path_dirs` prepended to `PATH`.
    fn run(&self, argv: &[String], path_dirs: &[PathBuf]) -> Result<CommandOutput, HostError>;

    /// Run `argv` with `input` on standard input, in `cwd`.
    ///
    /// The process is killed when `cancelled` becomes true or `timeout` passes.
    #[allow(clippy::too_many_arguments)]
    fn run_with_input(
        &self,
        argv: &[String],
        path_dirs: &[PathBuf],
        input: &str,
        cwd: &Path,
        timeout: Duration,
        cancelled: &AtomicBool,
    ) -> Result<CommandOutput, HostError>;

    /// Read a file, or `None` when it does not exist.
    fn read_file(&self, path: &Path) -> Result<Option<String>, HostError>;

    /// Write bytes to a file, replacing it.
    fn write_bytes(&self, path: &Path, bytes: &[u8]) -> Result<(), HostError>;

    /// Make a fresh temporary folder.
    fn create_temp_dir(&self, prefix: &str) -> Result<PathBuf, HostError>;

    /// Remove a temporary folder and everything in it.
    fn remove_dir_all(&self, path: &Path) -> Result<(), HostError>;

    /// Write a file in one step, replacing it atomically.
    fn write_file_atomic(&self, path: &Path, text: &str) -> Result<(), HostError>;

    /// Copy one file over another.
    fn copy_file(&self, from: &Path, to: &Path) -> Result<(), HostError>;

    /// Whether `path` is an existing directory.
    fn dir_exists(&self, path: &Path) -> bool;
}

/// The real host: `std::process::Command` and the standard library.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemAgentHost;

impl AgentHost for SystemAgentHost {
    fn find_cli(&self, program: &str, dirs: &[PathBuf], platform: Platform) -> Option<PathBuf> {
        for dir in dirs {
            for name in cli_file_names(program, platform) {
                let candidate = dir.join(name);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
        None
    }

    fn run(&self, argv: &[String], path_dirs: &[PathBuf]) -> Result<CommandOutput, HostError> {
        run_command(argv, path_dirs, None, None, COMMAND_TIMEOUT, None)
    }

    fn run_with_input(
        &self,
        argv: &[String],
        path_dirs: &[PathBuf],
        input: &str,
        cwd: &Path,
        timeout: Duration,
        cancelled: &AtomicBool,
    ) -> Result<CommandOutput, HostError> {
        run_command(
            argv,
            path_dirs,
            Some(input),
            Some(cwd),
            timeout,
            Some(cancelled),
        )
    }

    fn read_file(&self, path: &Path) -> Result<Option<String>, HostError> {
        match std::fs::read(path) {
            Ok(bytes) => String::from_utf8(bytes)
                .map(Some)
                .map_err(|_| HostError::Io("The settings file isn't UTF-8 text.".to_string())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(HostError::Io(error.to_string())),
        }
    }

    fn write_bytes(&self, path: &Path, bytes: &[u8]) -> Result<(), HostError> {
        std::fs::write(path, bytes).map_err(|error| HostError::Io(error.to_string()))
    }

    fn create_temp_dir(&self, prefix: &str) -> Result<PathBuf, HostError> {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let base = std::env::temp_dir();
        for _ in 0..100 {
            let number = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = base.join(format!("{prefix}-{}-{number}", std::process::id()));
            match std::fs::create_dir(&path) {
                Ok(()) => return Ok(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(HostError::Io(error.to_string())),
            }
        }
        Err(HostError::Io(
            "A temporary folder couldn't be made.".to_string(),
        ))
    }

    fn remove_dir_all(&self, path: &Path) -> Result<(), HostError> {
        match std::fs::remove_dir_all(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(HostError::Io(error.to_string())),
        }
    }

    fn write_file_atomic(&self, path: &Path, text: &str) -> Result<(), HostError> {
        crate::files::write_atomic(path, text.as_bytes())
            .map_err(|error| HostError::Io(error.to_string()))
    }

    fn copy_file(&self, from: &Path, to: &Path) -> Result<(), HostError> {
        std::fs::copy(from, to)
            .map(|_| ())
            .map_err(|error| HostError::Io(error.to_string()))
    }

    fn dir_exists(&self, path: &Path) -> bool {
        path.is_dir()
    }
}

fn run_command(
    argv: &[String],
    path_dirs: &[PathBuf],
    input: Option<&str>,
    cwd: Option<&Path>,
    timeout: Duration,
    cancelled: Option<&AtomicBool>,
) -> Result<CommandOutput, HostError> {
    let Some(program) = argv.first() else {
        return Err(HostError::Io(
            "The agent command was empty, so nothing ran.".to_string(),
        ));
    };
    let mut command = Command::new(program);
    command
        .args(argv.iter().skip(1))
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    if let Ok(path) = std::env::join_paths(path_dirs.iter()) {
        command.env("PATH", path);
    }

    let mut child = command
        .spawn()
        .map_err(|error| HostError::Io(error.to_string()))?;
    let stdin_handle = match (input, child.stdin.take()) {
        (Some(text), Some(mut stdin)) => {
            let text = text.to_string();
            Some(std::thread::spawn(move || {
                let _ = stdin.write_all(text.as_bytes());
                let _ = stdin.flush();
            }))
        }
        _ => None,
    };
    let budget = Arc::new(AtomicUsize::new(0));
    let stdout_budget = Arc::clone(&budget);
    let stderr_budget = Arc::clone(&budget);
    let stdout_handle = child
        .stdout
        .take()
        .map(|reader| std::thread::spawn(move || read_capped(reader, stdout_budget)));
    let stderr_handle = child
        .stderr
        .take()
        .map(|reader| std::thread::spawn(move || read_capped(reader, stderr_budget)));

    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                join_writer(stdin_handle);
                let stdout = join_reader(stdout_handle);
                let stderr = join_reader(stderr_handle);
                return Ok(CommandOutput {
                    success: status.success(),
                    stdout,
                    stderr,
                });
            }
            Ok(None) => {
                if cancelled.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
                    stop_child(&mut child, stdin_handle, stdout_handle, stderr_handle);
                    return Err(HostError::Cancelled);
                }
                if Instant::now() >= deadline {
                    stop_child(&mut child, stdin_handle, stdout_handle, stderr_handle);
                    return Err(HostError::Timeout);
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(error) => {
                stop_child(&mut child, stdin_handle, stdout_handle, stderr_handle);
                return Err(HostError::Io(error.to_string()));
            }
        }
    }
}

fn join_writer(handle: Option<std::thread::JoinHandle<()>>) {
    if let Some(handle) = handle {
        let _ = handle.join();
    }
}

fn stop_child(
    child: &mut std::process::Child,
    stdin_handle: Option<std::thread::JoinHandle<()>>,
    stdout_handle: Option<std::thread::JoinHandle<String>>,
    stderr_handle: Option<std::thread::JoinHandle<String>>,
) {
    let _ = child.kill();
    let _ = child.wait();
    join_writer(stdin_handle);
    let _ = join_reader(stdout_handle);
    let _ = join_reader(stderr_handle);
}

fn join_reader(handle: Option<std::thread::JoinHandle<String>>) -> String {
    handle
        .map(|handle| handle.join().unwrap_or_default())
        .unwrap_or_default()
}

fn keep_bytes(budget: &AtomicUsize, requested: usize) -> usize {
    let mut used = budget.load(Ordering::Relaxed);
    loop {
        let keep = requested.min(MAX_OUTPUT.saturating_sub(used));
        match budget.compare_exchange_weak(
            used,
            used.saturating_add(keep),
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => return keep,
            Err(current) => used = current,
        }
    }
}

fn read_capped(mut reader: impl Read, budget: Arc<AtomicUsize>) -> String {
    let mut out = Vec::new();
    let mut buffer = [0_u8; 8192];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                let keep = keep_bytes(&budget, read);
                if let (Some(chunk), true) = (buffer.get(..read), keep > 0)
                    && let Some(part) = chunk.get(..keep)
                {
                    out.extend_from_slice(part);
                }
            }
            Err(_) => break,
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Where to find the CLIs and settings files on this computer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentEnv {
    /// The user's home folder.
    pub home: PathBuf,
    /// `%APPDATA%` on Windows, when it can be read.
    pub appdata: Option<PathBuf>,
    /// The process's `PATH`.
    pub path_env: String,
    /// Which platform's paths and executable names to use.
    pub platform: Platform,
    /// This app's own binary, needed to add it to an agent.
    pub exe: Option<String>,
}

impl AgentEnv {
    /// Read the current process's environment.
    #[must_use]
    pub fn from_system(exe: Option<String>) -> Self {
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from)
            .unwrap_or_default();
        let appdata = std::env::var_os("APPDATA").map(PathBuf::from);
        let path_env = std::env::var_os("PATH")
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_default();
        let platform = if cfg!(target_os = "windows") {
            Platform::Windows
        } else if cfg!(target_os = "macos") {
            Platform::Macos
        } else {
            Platform::Linux
        };
        Self {
            home,
            appdata,
            path_env,
            platform,
            exe,
        }
    }
}

/// Every search directory, including each installed nvm version's `bin`.
fn all_search_dirs(env: &AgentEnv) -> Vec<PathBuf> {
    let mut dirs = search_dirs(&env.home, &env.path_env, env.platform);
    let root = env.home.join(".nvm").join("versions").join("node");
    if let Ok(entries) = std::fs::read_dir(&root) {
        let mut versions: Vec<String> = entries
            .filter_map(Result::ok)
            .filter_map(|entry| entry.file_name().into_string().ok())
            .collect();
        versions.sort();
        for dir in nvm_bin_dirs(&env.home, &versions) {
            if !dirs.contains(&dir) {
                dirs.push(dir);
            }
        }
    }
    dirs
}

fn describe(id: AgentId, error: &HostError) -> String {
    match error {
        HostError::Timeout => format!(
            "{} took too long and was stopped. Try again.",
            id.display_name()
        ),
        HostError::Cancelled => format!("{} was cancelled.", id.display_name()),
        HostError::Io(message) => format!(
            "{} couldn't run: {}",
            id.display_name(),
            shorten(message, MAX_EXCERPT)
        ),
    }
}

fn settings_problem(id: AgentId, error: MergeError) -> String {
    format!("{}: {error}", id.display_name())
}

fn shorten(text: &str, limit: usize) -> String {
    let mut out: String = text
        .chars()
        .map(|character| {
            if character.is_control() && character != '\n' && character != '\t' {
                ' '
            } else {
                character
            }
        })
        .take(limit)
        .collect();
    out = out.split_whitespace().collect::<Vec<_>>().join(" ");
    out
}

fn command_failed(id: AgentId, output: &CommandOutput) -> String {
    let detail = if output.stderr.trim().is_empty() {
        output.stdout.trim()
    } else {
        output.stderr.trim()
    };
    if detail.is_empty() {
        format!("{} stopped with an error.", id.display_name())
    } else {
        format!(
            "{} stopped with an error. {}",
            id.display_name(),
            shorten(detail, MAX_EXCERPT)
        )
    }
}

fn combined_output(output: &CommandOutput) -> String {
    let stderr = output.stderr.trim();
    let stdout = output.stdout.trim();
    match (stderr.is_empty(), stdout.is_empty()) {
        (false, false) => format!("{stderr}\n{stdout}"),
        (false, true) => stderr.to_string(),
        (true, false) => stdout.to_string(),
        (true, true) => String::new(),
    }
}

/// Run one chat request through Claude Code or Codex.
///
/// The prompt goes on standard input, images are written to a fresh temporary
/// folder, and that folder is the working directory. It is removed afterwards,
/// whatever happens. A cancelled or timed-out process is killed by the host.
///
/// # Errors
/// Returns a plain sentence for a missing or failing agent. The API key is never
/// part of the prompt, arguments or environment this function sets.
pub fn run_chat(
    host: &dyn AgentHost,
    env: &AgentEnv,
    source: AiSource,
    request: &ChatRequest,
    cancelled: &AtomicBool,
) -> Result<ChatReply, String> {
    let Some(id) = source.agent_id() else {
        return Err("Choose Claude Code or Codex, or use an API endpoint.".to_string());
    };
    let name = id.display_name();
    let Some(program) = id.program() else {
        return Err(format!("{name} has no command line."));
    };
    let dirs = all_search_dirs(env);
    let Some(found) = host.find_cli(program, &dirs, env.platform) else {
        return Err(format!("{name} isn't installed on this computer."));
    };
    let prompt = build_prompt(request)?;
    let dir = host.create_temp_dir("coursecraft-ai").map_err(|error| {
        format!(
            "{name} couldn't start: {}",
            output_excerpt(&error.to_string())
        )
    })?;
    let answer_path = dir.join("answer.txt");

    let result = (|| -> Result<ChatReply, String> {
        for image in &prompt.images {
            host.write_bytes(&dir.join(&image.file_name), &image.bytes)
                .map_err(|error| {
                    format!(
                        "{name} couldn't prepare the request: {}",
                        output_excerpt(&error.to_string())
                    )
                })?;
        }
        let argv = chat_argv(source, &answer_path, &prompt.images)
            .map(|argv| with_program(argv, &found))?;
        let output =
            match host.run_with_input(&argv, &dirs, &prompt.text, &dir, CHAT_TIMEOUT, cancelled) {
                Ok(output) => output,
                Err(HostError::Cancelled) => return Err("The request was cancelled.".to_string()),
                Err(HostError::Timeout) => {
                    return Err(format!("{name} took too long and was stopped. Try again."));
                }
                Err(HostError::Io(message)) => {
                    return Err(format!("{name} couldn't run: {}", output_excerpt(&message)));
                }
            };
        if !output.success {
            return Err(failure_sentence(source, &combined_output(&output)));
        }
        match source {
            AiSource::ClaudeCode => parse_claude_answer(&output.stdout)
                .map_err(|detail| failure_sentence(source, &detail)),
            AiSource::Codex => {
                let answer = host
                    .read_file(&answer_path)
                    .map_err(|error| failure_sentence(source, &output_excerpt(&error.to_string())))?
                    .ok_or_else(|| failure_sentence(source, "The answer file was missing."))?;
                parse_codex_answer(&answer).map_err(|detail| failure_sentence(source, &detail))
            }
            AiSource::Endpoint => Err("An API endpoint doesn't run an agent.".to_string()),
        }
    })();

    let _ = host.remove_dir_all(&dir);
    result
}

/// Check every agent, in display order.
#[must_use]
pub fn detect_all(host: &dyn AgentHost, env: &AgentEnv) -> Vec<(AgentId, AgentStatus)> {
    AgentId::ALL
        .into_iter()
        .map(|id| (id, detect_agent(host, env, id)))
        .collect()
}

/// Run the CLI that was found rather than whatever `PATH` resolves first.
fn with_program(mut argv: Vec<String>, found: &Path) -> Vec<String> {
    if let Some(first) = argv.first_mut() {
        *first = found.to_string_lossy().into_owned();
    }
    argv
}

/// Check one agent.
#[must_use]
pub fn detect_agent(host: &dyn AgentHost, env: &AgentEnv, id: AgentId) -> AgentStatus {
    let Some(program) = id.program() else {
        return detect_file_agent(host, env, id);
    };
    let dirs = all_search_dirs(env);
    let Some(found) = host.find_cli(program, &dirs, env.platform) else {
        return AgentStatus::NotInstalled;
    };
    let Some(argv) = status_argv(id).map(|argv| with_program(argv, &found)) else {
        return AgentStatus::Found;
    };
    match host.run(&argv, &dirs) {
        Ok(output) => {
            if output_says_connected(id, output.success, &output.stdout) {
                AgentStatus::Connected
            } else {
                AgentStatus::Found
            }
        }
        Err(error) => AgentStatus::Problem(describe(id, &error)),
    }
}

fn detect_file_agent(host: &dyn AgentHost, env: &AgentEnv, id: AgentId) -> AgentStatus {
    let Some(dir) = app_dir(id, &env.home, env.appdata.as_deref()) else {
        return AgentStatus::NotInstalled;
    };
    if !host.dir_exists(&dir) {
        return AgentStatus::NotInstalled;
    }
    let Some(path) = config_path(id, &env.home, env.appdata.as_deref()) else {
        return AgentStatus::NotInstalled;
    };
    match host.read_file(&path) {
        Ok(None) => AgentStatus::Found,
        Ok(Some(text)) => match file_has_mcp_server(&text) {
            Ok(true) => AgentStatus::Connected,
            Ok(false) => AgentStatus::Found,
            Err(error) => AgentStatus::Problem(settings_problem(id, error)),
        },
        Err(error) => AgentStatus::Problem(describe(id, &error)),
    }
}

/// Run one Connect, Test or Disconnect action, returning the new status.
///
/// # Errors
/// Returns a plain sentence when the CLI is missing, the command fails or the
/// settings file cannot be changed safely.
pub fn run_action(
    host: &dyn AgentHost,
    env: &AgentEnv,
    id: AgentId,
    action: AgentAction,
) -> Result<AgentStatus, String> {
    if id.is_cli() {
        run_cli_action(host, env, id, action)
    } else {
        run_file_action(host, env, id, action)
    }
}

fn run_cli_action(
    host: &dyn AgentHost,
    env: &AgentEnv,
    id: AgentId,
    action: AgentAction,
) -> Result<AgentStatus, String> {
    let Some(program) = id.program() else {
        return Err(format!("{} has no command line.", id.display_name()));
    };
    let dirs = all_search_dirs(env);
    let Some(found) = host.find_cli(program, &dirs, env.platform) else {
        return Err(format!(
            "{} isn't installed on this computer.",
            id.display_name()
        ));
    };
    let argv = match action {
        AgentAction::Connect => {
            let exe = env.exe.as_deref().ok_or_else(|| {
                format!(
                    "CourseCraft couldn't find its own location, so it can't connect {}.",
                    id.display_name()
                )
            })?;
            add_argv(id, exe)
        }
        AgentAction::Test => status_argv(id),
        AgentAction::Disconnect => remove_argv(id),
    }
    .map(|argv| with_program(argv, &found))
    .ok_or_else(|| format!("{} has no command line.", id.display_name()))?;

    let output = host
        .run(&argv, &dirs)
        .map_err(|error| describe(id, &error))?;
    if !output.success {
        return Err(command_failed(id, &output));
    }
    match action {
        AgentAction::Connect => Ok(AgentStatus::Connected),
        AgentAction::Test => {
            if output_says_connected(id, true, &output.stdout) {
                Ok(AgentStatus::Connected)
            } else {
                Err(format!(
                    "CourseCraft isn't connected to {}. Choose Connect first.",
                    id.display_name()
                ))
            }
        }
        AgentAction::Disconnect => Ok(AgentStatus::Found),
    }
}

fn run_file_action(
    host: &dyn AgentHost,
    env: &AgentEnv,
    id: AgentId,
    action: AgentAction,
) -> Result<AgentStatus, String> {
    let Some(path) = config_path(id, &env.home, env.appdata.as_deref()) else {
        return Err(format!("{} has no settings file.", id.display_name()));
    };
    let Some(dir) = path.parent() else {
        return Err(format!(
            "CourseCraft couldn't find {}'s folder.",
            id.display_name()
        ));
    };
    if !host.dir_exists(dir) {
        return Err(format!(
            "{} isn't installed on this computer.",
            id.display_name()
        ));
    }

    match action {
        AgentAction::Connect => {
            let exe = env.exe.as_deref().ok_or_else(|| {
                format!(
                    "CourseCraft couldn't find its own location, so it can't connect {}.",
                    id.display_name()
                )
            })?;
            let existing = host
                .read_file(&path)
                .map_err(|error| describe(id, &error))?;
            let merged = merge_mcp_server(existing.as_deref().unwrap_or(""), exe)
                .map_err(|error| settings_problem(id, error))?;
            backup_then_write(host, &path, existing.as_deref(), &merged)
                .map_err(|error| describe(id, &error))?;
            Ok(AgentStatus::Connected)
        }
        AgentAction::Test => {
            let existing = host
                .read_file(&path)
                .map_err(|error| describe(id, &error))?;
            match file_has_mcp_server(existing.as_deref().unwrap_or("")) {
                Ok(true) => Ok(AgentStatus::Connected),
                Ok(false) => Err(format!(
                    "CourseCraft isn't connected to {}. Choose Connect first.",
                    id.display_name()
                )),
                Err(error) => Err(settings_problem(id, error)),
            }
        }
        AgentAction::Disconnect => {
            let existing = host
                .read_file(&path)
                .map_err(|error| describe(id, &error))?;
            let cleaned = remove_mcp_server(existing.as_deref().unwrap_or(""))
                .map_err(|error| settings_problem(id, error))?;
            if existing.is_some() {
                backup_then_write(host, &path, existing.as_deref(), &cleaned)
                    .map_err(|error| describe(id, &error))?;
            }
            Ok(AgentStatus::Found)
        }
    }
}

fn backup_then_write(
    host: &dyn AgentHost,
    path: &Path,
    existing: Option<&str>,
    new_text: &str,
) -> Result<(), HostError> {
    if existing.is_some() {
        host.copy_file(path, &backup_path(path))?;
    }
    host.write_file_atomic(path, new_text)
}

/// A finished background agent check or action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentMessage {
    /// Detection finished for every agent.
    Statuses(Vec<(AgentId, AgentStatus)>),
    /// One action finished.
    ActionResult(AgentId, Result<AgentStatus, String>),
    /// The worker stopped without sending a result.
    Stopped,
}

/// One background agent task at a time, polled once a frame by the app.
pub struct AgentClient {
    pending: Option<Receiver<AgentMessage>>,
}

impl Default for AgentClient {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentClient {
    /// A client with no task in flight.
    #[must_use]
    pub fn new() -> Self {
        Self { pending: None }
    }

    /// True while a check or action is running.
    #[must_use]
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// Start a detection pass on a background thread.
    ///
    /// Returns false when a task is already running.
    pub fn detect(
        &mut self,
        host: Arc<dyn AgentHost>,
        env: AgentEnv,
        repaint: impl Fn() + Send + Sync + 'static,
    ) -> bool {
        if self.pending.is_some() {
            return false;
        }
        let (tx, rx) = channel();
        let repaint: Arc<dyn Fn() + Send + Sync> = Arc::new(repaint);
        std::thread::spawn(move || {
            let statuses = detect_all(host.as_ref(), &env);
            let _ = tx.send(AgentMessage::Statuses(statuses));
            repaint();
        });
        self.pending = Some(rx);
        true
    }

    /// Start one Connect, Test or Disconnect on a background thread.
    ///
    /// Returns false when a task is already running.
    pub fn run_action(
        &mut self,
        host: Arc<dyn AgentHost>,
        env: AgentEnv,
        agent: AgentId,
        action: AgentAction,
        repaint: impl Fn() + Send + Sync + 'static,
    ) -> bool {
        if self.pending.is_some() {
            return false;
        }
        let (tx, rx) = channel();
        let repaint: Arc<dyn Fn() + Send + Sync> = Arc::new(repaint);
        std::thread::spawn(move || {
            let result = run_action(host.as_ref(), &env, agent, action);
            let _ = tx.send(AgentMessage::ActionResult(agent, result));
            repaint();
        });
        self.pending = Some(rx);
        true
    }

    /// Take a finished message, or `None` while the task is still running.
    pub fn poll(&mut self) -> Option<AgentMessage> {
        let receiver = self.pending.as_ref()?;
        match receiver.try_recv() {
            Ok(message) => {
                self.pending = None;
                Some(message)
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                self.pending = None;
                Some(AgentMessage::Stopped)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use coursecraft_ai::AiReply;
    use coursecraft_ui_egui::AiPurpose;
    use std::collections::{BTreeMap, BTreeSet, VecDeque};
    use std::sync::Mutex;

    #[derive(Debug, Clone)]
    struct ChatStep {
        output: Result<CommandOutput, HostError>,
        answer_file: Option<String>,
        wait_for_cancel: bool,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct ChatCall {
        argv: Vec<String>,
        cwd: PathBuf,
        input: String,
    }

    #[derive(Debug, Clone, Default)]
    struct FakeState {
        clis: BTreeMap<String, PathBuf>,
        files: BTreeMap<PathBuf, String>,
        written: BTreeMap<PathBuf, Vec<u8>>,
        dirs: BTreeSet<PathBuf>,
        commands: VecDeque<Result<CommandOutput, HostError>>,
        chat_steps: VecDeque<ChatStep>,
        chat_calls: Vec<ChatCall>,
        cancelled_seen: bool,
        removed: BTreeSet<PathBuf>,
        temp_counter: usize,
        calls: Vec<Vec<String>>,
    }

    /// A host that answers from memory and records what it was asked to run.
    #[derive(Clone, Default)]
    struct FakeHost {
        state: Arc<Mutex<FakeState>>,
    }

    impl FakeHost {
        fn with_cli(&self, program: &str, path: &str) {
            self.state
                .lock()
                .expect("lock")
                .clis
                .insert(program.to_string(), PathBuf::from(path));
        }

        fn with_dir(&self, path: &str) {
            self.state
                .lock()
                .expect("lock")
                .dirs
                .insert(PathBuf::from(path));
        }

        fn with_file(&self, path: &str, text: &str) {
            self.state
                .lock()
                .expect("lock")
                .files
                .insert(PathBuf::from(path), text.to_string());
        }

        fn push(&self, result: Result<CommandOutput, HostError>) {
            self.state.lock().expect("lock").commands.push_back(result);
        }

        fn push_chat(&self, result: Result<CommandOutput, HostError>, answer_file: Option<&str>) {
            self.state
                .lock()
                .expect("lock")
                .chat_steps
                .push_back(ChatStep {
                    output: result,
                    answer_file: answer_file.map(str::to_string),
                    wait_for_cancel: false,
                });
        }

        fn push_chat_waiting_for_cancel(&self) {
            self.state
                .lock()
                .expect("lock")
                .chat_steps
                .push_back(ChatStep {
                    output: Err(HostError::Cancelled),
                    answer_file: None,
                    wait_for_cancel: true,
                });
        }

        fn calls(&self) -> Vec<Vec<String>> {
            self.state.lock().expect("lock").calls.clone()
        }

        fn chat_calls(&self) -> Vec<ChatCall> {
            self.state.lock().expect("lock").chat_calls.clone()
        }

        fn cancelled_seen(&self) -> bool {
            self.state.lock().expect("lock").cancelled_seen
        }

        fn written(&self) -> Vec<PathBuf> {
            self.state
                .lock()
                .expect("lock")
                .written
                .keys()
                .cloned()
                .collect()
        }

        fn removed(&self) -> BTreeSet<PathBuf> {
            self.state.lock().expect("lock").removed.clone()
        }

        fn file(&self, path: &str) -> Option<String> {
            self.state
                .lock()
                .expect("lock")
                .files
                .get(Path::new(path))
                .cloned()
        }
    }

    impl AgentHost for FakeHost {
        fn find_cli(
            &self,
            program: &str,
            _dirs: &[PathBuf],
            _platform: Platform,
        ) -> Option<PathBuf> {
            self.state.lock().expect("lock").clis.get(program).cloned()
        }

        fn run(&self, argv: &[String], _path_dirs: &[PathBuf]) -> Result<CommandOutput, HostError> {
            let mut state = self.state.lock().expect("lock");
            state.calls.push(argv.to_vec());
            state.commands.pop_front().unwrap_or(Ok(CommandOutput {
                success: false,
                stdout: String::new(),
                stderr: "no fake answer".to_string(),
            }))
        }

        fn run_with_input(
            &self,
            argv: &[String],
            _path_dirs: &[PathBuf],
            input: &str,
            cwd: &Path,
            _timeout: Duration,
            cancelled: &AtomicBool,
        ) -> Result<CommandOutput, HostError> {
            let step = {
                let mut state = self.state.lock().expect("lock");
                state.chat_calls.push(ChatCall {
                    argv: argv.to_vec(),
                    cwd: cwd.to_path_buf(),
                    input: input.to_string(),
                });
                state.chat_steps.pop_front().unwrap_or(ChatStep {
                    output: Ok(CommandOutput {
                        success: false,
                        stdout: String::new(),
                        stderr: "no fake chat answer".to_string(),
                    }),
                    answer_file: None,
                    wait_for_cancel: false,
                })
            };
            if step.wait_for_cancel {
                let deadline = Instant::now() + Duration::from_secs(5);
                while Instant::now() < deadline {
                    if cancelled.load(Ordering::Relaxed) {
                        self.state.lock().expect("lock").cancelled_seen = true;
                        return Err(HostError::Cancelled);
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
                return Err(HostError::Timeout);
            }
            if let Some(answer) = step.answer_file {
                let mut state = self.state.lock().expect("lock");
                if let Some(path) = argv
                    .iter()
                    .position(|arg| arg == "--output-last-message")
                    .and_then(|index| argv.get(index.saturating_add(1)))
                {
                    state.files.insert(PathBuf::from(path), answer);
                }
            }
            step.output
        }

        fn read_file(&self, path: &Path) -> Result<Option<String>, HostError> {
            Ok(self.state.lock().expect("lock").files.get(path).cloned())
        }

        fn write_bytes(&self, path: &Path, bytes: &[u8]) -> Result<(), HostError> {
            self.state
                .lock()
                .expect("lock")
                .written
                .insert(path.to_path_buf(), bytes.to_vec());
            Ok(())
        }

        fn create_temp_dir(&self, prefix: &str) -> Result<PathBuf, HostError> {
            let mut state = self.state.lock().expect("lock");
            state.temp_counter = state.temp_counter.saturating_add(1);
            let path = PathBuf::from(format!("/fake/tmp/{prefix}-{}", state.temp_counter));
            state.dirs.insert(path.clone());
            Ok(path)
        }

        fn remove_dir_all(&self, path: &Path) -> Result<(), HostError> {
            let mut state = self.state.lock().expect("lock");
            state.dirs.remove(path);
            state.files.retain(|file, _| !file.starts_with(path));
            state.removed.insert(path.to_path_buf());
            Ok(())
        }

        fn write_file_atomic(&self, path: &Path, text: &str) -> Result<(), HostError> {
            self.state
                .lock()
                .expect("lock")
                .files
                .insert(path.to_path_buf(), text.to_string());
            Ok(())
        }

        fn copy_file(&self, from: &Path, to: &Path) -> Result<(), HostError> {
            let mut state = self.state.lock().expect("lock");
            let text = state
                .files
                .get(from)
                .cloned()
                .ok_or_else(|| HostError::Io(format!("{} does not exist", from.display())))?;
            state.files.insert(to.to_path_buf(), text);
            Ok(())
        }

        fn dir_exists(&self, path: &Path) -> bool {
            self.state.lock().expect("lock").dirs.contains(path)
        }
    }

    fn ok(stdout: &str) -> Result<CommandOutput, HostError> {
        Ok(CommandOutput {
            success: true,
            stdout: stdout.to_string(),
            stderr: String::new(),
        })
    }

    fn failed(stderr: &str) -> Result<CommandOutput, HostError> {
        Ok(CommandOutput {
            success: false,
            stdout: String::new(),
            stderr: stderr.to_string(),
        })
    }

    fn env() -> AgentEnv {
        AgentEnv {
            home: PathBuf::from("/fake/home"),
            appdata: None,
            path_env: String::new(),
            platform: Platform::Macos,
            exe: Some("/Applications/CourseCraft".to_string()),
        }
    }

    fn row(statuses: &[(AgentId, AgentStatus)], id: AgentId) -> &AgentStatus {
        &statuses
            .iter()
            .find(|(agent, _)| *agent == id)
            .expect("status row")
            .1
    }

    #[test]
    fn cli_agent_runs_detect_connect_detect_test_disconnect() {
        let host = FakeHost::default();
        host.with_cli("claude", "/fake/home/.local/bin/claude");
        let env = env();

        // Found: the CLI exists but `mcp get` fails.
        host.push(failed("not added"));
        let statuses = detect_all(&host, &env);
        assert_eq!(row(&statuses, AgentId::ClaudeCode), &AgentStatus::Found);

        // Connect.
        host.push(ok(""));
        assert_eq!(
            run_action(&host, &env, AgentId::ClaudeCode, AgentAction::Connect),
            Ok(AgentStatus::Connected)
        );

        // Detection now sees it.
        host.push(ok("coursecraft"));
        let statuses = detect_all(&host, &env);
        assert_eq!(row(&statuses, AgentId::ClaudeCode), &AgentStatus::Connected);

        // Test.
        host.push(ok("coursecraft"));
        assert_eq!(
            run_action(&host, &env, AgentId::ClaudeCode, AgentAction::Test),
            Ok(AgentStatus::Connected)
        );

        // Disconnect.
        host.push(ok(""));
        assert_eq!(
            run_action(&host, &env, AgentId::ClaudeCode, AgentAction::Disconnect),
            Ok(AgentStatus::Found)
        );

        // Every call runs the CLI that was found, not a PATH lookup.
        let found = Path::new("/fake/home/.local/bin/claude");
        let calls = host.calls();
        assert_eq!(
            calls[0],
            with_program(status_argv(AgentId::ClaudeCode).expect("get"), found)
        );
        assert_eq!(
            calls[1],
            with_program(
                add_argv(AgentId::ClaudeCode, "/Applications/CourseCraft").expect("add"),
                found
            )
        );
        assert_eq!(
            calls[4],
            with_program(remove_argv(AgentId::ClaudeCode).expect("remove"), found)
        );
    }

    #[test]
    fn cursor_connect_backs_up_keeps_other_servers_and_disconnects() {
        let host = FakeHost::default();
        let config = "/fake/home/.cursor/mcp.json";
        let old = r#"{
            "theme": "dark",
            "mcpServers": {
                "other": {"command": "other", "args": ["serve"]}
            }
        }"#;
        host.with_dir("/fake/home/.cursor");
        host.with_file(config, old);
        let env = env();

        let statuses = detect_all(&host, &env);
        assert_eq!(row(&statuses, AgentId::Cursor), &AgentStatus::Found);

        assert_eq!(
            run_action(&host, &env, AgentId::Cursor, AgentAction::Connect),
            Ok(AgentStatus::Connected)
        );

        let backup = "/fake/home/.cursor/mcp.json.coursecraft-backup";
        assert_eq!(host.file(backup).as_deref(), Some(old));

        let written = host.file(config).expect("written config");
        let value: serde_json::Value = serde_json::from_str(&written).expect("valid merged json");
        assert_eq!(value["theme"], "dark");
        assert_eq!(value["mcpServers"]["other"]["command"], "other");
        assert_eq!(
            value["mcpServers"]["coursecraft"]["command"],
            "/Applications/CourseCraft"
        );

        let statuses = detect_all(&host, &env);
        assert_eq!(row(&statuses, AgentId::Cursor), &AgentStatus::Connected);

        assert_eq!(
            run_action(&host, &env, AgentId::Cursor, AgentAction::Disconnect),
            Ok(AgentStatus::Found)
        );
        let cleaned = host.file(config).expect("cleaned config");
        let value: serde_json::Value = serde_json::from_str(&cleaned).expect("valid cleaned json");
        assert!(value["mcpServers"].get("coursecraft").is_none());
        assert_eq!(value["mcpServers"]["other"]["command"], "other");
    }

    #[test]
    fn a_broken_cursor_file_is_left_untouched_with_a_problem_sentence() {
        let host = FakeHost::default();
        let config = "/fake/home/.cursor/mcp.json";
        let broken = r#"{"mcpServers": {"other": "#;
        host.with_dir("/fake/home/.cursor");
        host.with_file(config, broken);
        let env = env();

        let statuses = detect_all(&host, &env);
        let status = row(&statuses, AgentId::Cursor);
        assert!(matches!(status, AgentStatus::Problem(_)), "{status:?}");

        let error = run_action(&host, &env, AgentId::Cursor, AgentAction::Connect)
            .expect_err("must refuse broken json");
        assert!(
            error.contains("left it unchanged") || error.contains("isn't valid JSON"),
            "{error}"
        );
        assert_eq!(host.file(config).as_deref(), Some(broken));
        assert!(
            host.file("/fake/home/.cursor/mcp.json.coursecraft-backup")
                .is_none()
        );
    }

    #[test]
    fn a_timeout_becomes_a_problem_sentence() {
        let host = FakeHost::default();
        host.with_cli("codex", "/fake/home/.local/bin/codex");
        host.push(Err(HostError::Timeout));
        let statuses = detect_all(&host, &env());
        let status = row(&statuses, AgentId::Codex);
        match status {
            AgentStatus::Problem(sentence) => {
                assert!(sentence.contains("took too long"), "{sentence}")
            }
            other => panic!("expected a problem, got {other:?}"),
        }
    }

    #[test]
    fn command_output_is_capped_at_64_kib() {
        let budget = Arc::new(AtomicUsize::new(0));
        let noisy = vec![b'x'; MAX_OUTPUT * 3];
        let text = read_capped(std::io::Cursor::new(noisy), budget);
        assert_eq!(text.len(), MAX_OUTPUT);
    }

    #[test]
    fn a_failed_connect_reports_the_output_as_a_plain_sentence() {
        let host = FakeHost::default();
        host.with_cli("claude", "/fake/home/.local/bin/claude");
        host.push(failed("login required"));
        let error = run_action(&host, &env(), AgentId::ClaudeCode, AgentAction::Connect)
            .expect_err("must fail");
        assert!(
            error.contains("Claude Code stopped with an error."),
            "{error}"
        );
        assert!(error.contains("login required"), "{error}");
    }

    #[test]
    fn a_missing_cli_has_no_action() {
        let host = FakeHost::default();
        let error = run_action(&host, &env(), AgentId::GeminiCli, AgentAction::Connect)
            .expect_err("must fail");
        assert!(error.contains("isn't installed"), "{error}");
        assert!(host.calls().is_empty());
    }

    #[test]
    fn claude_desktop_uses_appdata_when_given() {
        let host = FakeHost::default();
        let appdata = PathBuf::from(r"C:\Users\ann\AppData\Roaming");
        host.with_dir(&appdata.join("Claude").to_string_lossy());
        let mut env = env();
        env.home = PathBuf::from(r"C:\Users\ann");
        env.appdata = Some(appdata);
        env.platform = Platform::Windows;

        let statuses = detect_all(&host, &env);
        assert_eq!(row(&statuses, AgentId::ClaudeDesktop), &AgentStatus::Found);
    }

    fn chat_success(stdout: &str) -> Result<CommandOutput, HostError> {
        Ok(CommandOutput {
            success: true,
            stdout: stdout.to_string(),
            stderr: String::new(),
        })
    }

    fn wait_for_agent_reply(
        client: &mut crate::ai::AiClient,
    ) -> Option<(AiPurpose, Result<AiReply, String>)> {
        for step in 0..600 {
            let now = f64::from(step) * 0.05;
            if let Some(result) = client.poll(now) {
                return Some(result);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        None
    }

    #[test]
    fn outline_runs_through_claude_code_end_to_end() {
        use coursecraft_ai::{OutlineOptions, outline_request};

        let host = FakeHost::default();
        host.with_cli("claude", "/fake/home/.local/bin/claude");
        host.push_chat(
            chat_success(r##"{"is_error":false,"result":"# Safety"}"##),
            None,
        );
        let request = outline_request("Keep exits clear.", &OutlineOptions::default());
        let mut client = crate::ai::AiClient::new();
        client
            .begin_agent(
                AiPurpose::Outline,
                AiSource::ClaudeCode,
                &request,
                0.0,
                Arc::new(host.clone()),
                env(),
                || {},
            )
            .expect("begin");
        let (purpose, result) = wait_for_agent_reply(&mut client).expect("reply");
        assert_eq!(purpose, AiPurpose::Outline);
        let AiReply::Chat(reply) = result.expect("chat reply") else {
            panic!("expected a chat reply");
        };
        assert_eq!(reply.text, "# Safety");

        let call = host.chat_calls().pop().expect("chat call");
        assert_eq!(
            call.argv.first().map(String::as_str),
            Some("/fake/home/.local/bin/claude")
        );
        assert_eq!(
            call.argv.get(1).map(String::as_str),
            Some("-p"),
            "Claude Code reads the prompt on stdin"
        );
        assert!(call.input.contains("Keep exits clear."), "{}", call.input);
        assert!(
            call.input.contains("Reply with only the answer"),
            "{}",
            call.input
        );
        assert!(host.removed().contains(&call.cwd));
        assert!(!call.cwd.as_os_str().is_empty());
    }

    #[test]
    fn alt_text_runs_through_codex_with_an_image_and_cleans_up() {
        use coursecraft_ai::{AltContext, alt_text_request, image_data_url};

        let host = FakeHost::default();
        host.with_cli("codex", "/fake/home/.local/bin/codex");
        host.push_chat(chat_success(""), Some("A red ball on grass."));
        let request = alt_text_request(
            &image_data_url("image/png", b"PNG bytes"),
            &AltContext::default(),
        );
        let mut client = crate::ai::AiClient::new();
        client
            .begin_agent(
                AiPurpose::AltText {
                    src: "ball.png".to_string(),
                },
                AiSource::Codex,
                &request,
                0.0,
                Arc::new(host.clone()),
                env(),
                || {},
            )
            .expect("begin");
        let (_, result) = wait_for_agent_reply(&mut client).expect("reply");
        let AiReply::Chat(reply) = result.expect("chat reply") else {
            panic!("expected a chat reply");
        };
        assert_eq!(reply.text, "A red ball on grass.");

        let call = host.chat_calls().pop().expect("chat call");
        assert!(
            call.argv.iter().any(|arg| arg == "--image"),
            "{:?}",
            call.argv
        );
        assert!(
            call.argv.iter().any(|arg| arg == "image-1.png"),
            "{:?}",
            call.argv
        );
        assert!(call.input.contains("image-1.png"), "{}", call.input);
        let image_path = call.cwd.join("image-1.png");
        assert!(
            host.written().contains(&image_path),
            "the image is written in the temp folder"
        );
        assert!(
            host.removed().contains(&call.cwd),
            "the temp folder is removed"
        );
    }

    #[test]
    fn an_agent_timeout_is_a_plain_sentence_and_cleans_up() {
        use coursecraft_ai::{OutlineOptions, outline_request};

        let host = FakeHost::default();
        host.with_cli("claude", "/fake/home/.local/bin/claude");
        host.push_chat(Err(HostError::Timeout), None);
        let request = outline_request("Text.", &OutlineOptions::default());
        let error = run_chat(
            &host,
            &env(),
            AiSource::ClaudeCode,
            &request,
            &AtomicBool::new(false),
        )
        .expect_err("must time out");
        assert!(error.contains("Claude Code took too long"), "{error}");
        let call = host.chat_calls().pop().expect("chat call");
        assert!(host.removed().contains(&call.cwd));
    }

    #[test]
    fn cancelling_an_agent_request_kills_the_process() {
        use coursecraft_ai::{OutlineOptions, outline_request};

        let host = FakeHost::default();
        host.with_cli("codex", "/fake/home/.local/bin/codex");
        host.push_chat_waiting_for_cancel();
        let request = outline_request("Text.", &OutlineOptions::default());
        let mut client = crate::ai::AiClient::new();
        client
            .begin_agent(
                AiPurpose::Outline,
                AiSource::Codex,
                &request,
                0.0,
                Arc::new(host.clone()),
                env(),
                || {},
            )
            .expect("begin");
        for _ in 0..200 {
            if !host.chat_calls().is_empty() {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        client.cancel();
        assert!(!client.is_pending());
        for _ in 0..200 {
            if host.cancelled_seen() {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(host.cancelled_seen(), "the process sees the cancel");
    }

    #[test]
    fn an_agent_error_never_contains_the_api_key() {
        use coursecraft_ai::{OutlineOptions, outline_request};

        let host = FakeHost::default();
        host.with_cli("claude", "/fake/home/.local/bin/claude");
        host.push_chat(
            Err(HostError::Io("sk-very-secret-value failed".to_string())),
            None,
        );
        let request = outline_request("Text.", &OutlineOptions::default());
        let error = run_chat(
            &host,
            &env(),
            AiSource::ClaudeCode,
            &request,
            &AtomicBool::new(false),
        )
        .expect_err("must fail");
        assert!(!error.contains("sk-very-secret-value"), "{error}");
    }
}
