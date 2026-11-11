//! Control channel TCP server and discovery file management.

use std::io::{BufRead, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

use serde_json::Value as Json;

use crate::dispatch;
use crate::host::ControlHost;

const MAX_CONNECTIONS: usize = 8;
const MAX_LINE_BYTES: usize = 16 * 1024 * 1024; // 16 MiB
const READ_TIMEOUT: Duration = Duration::from_secs(600); // 10 min
const WRITE_TIMEOUT: Duration = Duration::from_secs(60);

/// Server errors.
#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to bind loopback address")]
    NotLoopback,
    #[error("failed to write discovery file: {0}")]
    Discovery(String),
}

/// Request forwarded from a client connection thread to the main thread.
pub struct ServerRequest {
    pub raw_json: Json,
    pub reply_tx: Sender<Json>,
}

/// Called from a connection thread after it queues a request, so an idle owner
/// (e.g. an egui app that only draws on input) wakes up and calls [`ControlServer::poll`].
pub type Waker = Arc<dyn Fn() + Send + Sync>;

/// Control server that binds loopback and answers requests through [`poll`].
pub struct ControlServer {
    port: u16,
    token: String,
    pid: u32,
    config_dir: Option<PathBuf>,
    request_rx: Receiver<ServerRequest>,
    active_connections: Arc<AtomicUsize>,
    shutdown: Arc<AtomicBool>,
}

impl ControlServer {
    /// Bind an ephemeral port on `127.0.0.1` and start listening for control clients.
    ///
    /// # Errors
    /// Returns an error if socket binding or discovery file creation fails.
    pub fn start(config_dir: Option<&Path>) -> Result<Self, ServerError> {
        Self::start_with_waker(config_dir, None)
    }

    /// Like [`ControlServer::start`], calling `waker` whenever a request is queued.
    ///
    /// # Errors
    /// Returns an error if socket binding or discovery file creation fails.
    pub fn start_with_waker(
        config_dir: Option<&Path>,
        waker: Option<Waker>,
    ) -> Result<Self, ServerError> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;
        if !addr.ip().is_loopback() {
            return Err(ServerError::NotLoopback);
        }
        let port = addr.port();
        let token = generate_token();
        let pid = std::process::id();

        if let Some(dir) = config_dir {
            write_discovery_file(dir, port, &token, pid)
                .map_err(|e| ServerError::Discovery(e.to_string()))?;
        }

        let (request_tx, request_rx) = channel::<ServerRequest>();
        let active_connections = Arc::new(AtomicUsize::new(0));
        let shutdown = Arc::new(AtomicBool::new(false));

        let shutdown_accept = shutdown.clone();
        let active_accept = active_connections.clone();

        std::thread::spawn(move || {
            accept_loop(listener, request_tx, active_accept, shutdown_accept, waker);
        });

        Ok(Self {
            port,
            token,
            pid,
            config_dir: config_dir.map(Path::to_path_buf),
            request_rx,
            active_connections,
            shutdown,
        })
    }

    /// Bound port number.
    #[must_use]
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Number of active client connections.
    #[must_use]
    pub fn active_connections(&self) -> usize {
        self.active_connections.load(Ordering::SeqCst)
    }

    /// Authentication token required in request params.
    #[must_use]
    pub fn token(&self) -> &str {
        &self.token
    }

    /// Process all pending client requests on the current thread using `host`.
    ///
    /// Returns the number of requests answered.
    pub fn poll(&self, host: &mut dyn ControlHost) -> usize {
        let mut count = 0;
        while let Ok(req) = self.request_rx.try_recv() {
            let response = dispatch(host, &req.raw_json, &self.token);
            let _ = req.reply_tx.send(response);
            count += 1;
        }
        count
    }
}

impl Drop for ControlServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        // Wake up accept thread by connecting
        let _ = TcpStream::connect(SocketAddr::from(([127, 0, 0, 1], self.port)));

        // Remove discovery file if it contains our pid
        if let Some(ref dir) = self.config_dir {
            let path = dir.join("control.json");
            if let Ok(text) = std::fs::read_to_string(&path)
                && let Ok(val) = serde_json::from_str::<Json>(&text)
                && val.get("pid").and_then(Json::as_u64) == Some(u64::from(self.pid))
            {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
}

fn accept_loop(
    listener: TcpListener,
    request_tx: Sender<ServerRequest>,
    active_connections: Arc<AtomicUsize>,
    shutdown: Arc<AtomicBool>,
    waker: Option<Waker>,
) {
    for stream_res in listener.incoming() {
        if shutdown.load(Ordering::SeqCst) {
            break;
        }
        let Ok(stream) = stream_res else {
            continue;
        };
        if active_connections.load(Ordering::SeqCst) >= MAX_CONNECTIONS {
            // Drop connection when limit is reached
            continue;
        }

        active_connections.fetch_add(1, Ordering::SeqCst);
        let active = active_connections.clone();
        let tx = request_tx.clone();
        let waker = waker.clone();

        std::thread::spawn(move || {
            handle_connection(stream, tx, waker);
            active.fetch_sub(1, Ordering::SeqCst);
        });
    }
}

fn handle_connection(
    mut stream: TcpStream,
    request_tx: Sender<ServerRequest>,
    waker: Option<Waker>,
) {
    let _ = stream.set_read_timeout(Some(READ_TIMEOUT));
    let _ = stream.set_write_timeout(Some(WRITE_TIMEOUT));
    let _ = stream.set_nodelay(true);

    let reader_stream = match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    };
    let mut reader = std::io::BufReader::new(reader_stream);
    let mut line_buf = String::new();

    loop {
        match read_line_capped(&mut reader, &mut line_buf) {
            Ok(Some(())) => {
                let trimmed = line_buf.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let raw_json: Json = match serde_json::from_str(trimmed) {
                    Ok(j) => j,
                    Err(e) => {
                        let err_resp = serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": Json::Null,
                            "error": {
                                "code": crate::dispatch::CODE_PARSE_ERROR,
                                "message": format!("parse error: {e}")
                            }
                        });
                        let mut out = serde_json::to_string(&err_resp).unwrap_or_default();
                        out.push('\n');
                        let _ = stream.write_all(out.as_bytes());
                        let _ = stream.flush();
                        continue;
                    }
                };

                let (reply_tx, reply_rx) = channel::<Json>();
                if request_tx
                    .send(ServerRequest { raw_json, reply_tx })
                    .is_err()
                {
                    break;
                }
                if let Some(wake) = &waker {
                    wake();
                }

                match reply_rx.recv() {
                    Ok(reply) => {
                        let mut out = match serde_json::to_string(&reply) {
                            Ok(s) => s,
                            Err(_) => break,
                        };
                        out.push('\n');
                        if stream.write_all(out.as_bytes()).is_err() || stream.flush().is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            Ok(None) => break, // Clean EOF
            Err(_) => {
                // Oversized line or I/O error: close connection
                let err_resp = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": Json::Null,
                    "error": {
                        "code": crate::dispatch::CODE_PARSE_ERROR,
                        "message": "line exceeded 16 MiB limit"
                    }
                });
                let mut out = serde_json::to_string(&err_resp).unwrap_or_default();
                out.push('\n');
                let _ = stream.write_all(out.as_bytes());
                let _ = stream.flush();
                break;
            }
        }
    }
}

fn read_line_capped<R: BufRead>(reader: &mut R, buf: &mut String) -> std::io::Result<Option<()>> {
    buf.clear();
    let mut total_read = 0;
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            if total_read == 0 {
                return Ok(None);
            }
            return Ok(Some(()));
        }
        if let Some(pos) = available.iter().position(|&b| b == b'\n') {
            let consume_len = pos + 1;
            if total_read + consume_len > MAX_LINE_BYTES {
                reader.consume(consume_len);
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "line exceeded 16 MiB limit",
                ));
            }
            let s = std::str::from_utf8(&available[..consume_len])
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            buf.push_str(s);
            reader.consume(consume_len);
            return Ok(Some(()));
        }
        let len = available.len();
        if total_read + len > MAX_LINE_BYTES {
            reader.consume(len);
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "line exceeded 16 MiB limit",
            ));
        }
        let s = std::str::from_utf8(available)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        buf.push_str(s);
        total_read += len;
        reader.consume(len);
    }
}

fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    let mut filled = false;
    #[cfg(unix)]
    {
        if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
            use std::io::Read;
            if f.read_exact(&mut bytes).is_ok() {
                filled = true;
            }
        }
    }
    if !filled {
        use std::hash::{DefaultHasher, Hasher};
        let mut hasher = DefaultHasher::new();
        let now = std::time::SystemTime::now();
        let pid = std::process::id();
        let stack_var = 42u8;
        hasher.write_u32(pid);
        if let Ok(dur) = now.duration_since(std::time::UNIX_EPOCH) {
            hasher.write_u128(dur.as_nanos());
        }
        let addr = std::ptr::from_ref(&stack_var) as usize;
        hasher.write_usize(addr);
        for chunk in bytes.chunks_mut(8) {
            hasher.write_u64(0x9e37_79b9_7f4a_7c15);
            let val = hasher.finish();
            let val_bytes = val.to_ne_bytes();
            let len = chunk.len().min(8);
            chunk[..len].copy_from_slice(&val_bytes[..len]);
        }
    }
    let mut s = String::with_capacity(64);
    for b in bytes {
        use std::fmt::Write;
        let _ = write!(s, "{b:02x}");
    }
    s
}

fn write_discovery_file(
    config_dir: &Path,
    port: u16,
    token: &str,
    pid: u32,
) -> Result<(), std::io::Error> {
    std::fs::create_dir_all(config_dir)?;
    let target = config_dir.join("control.json");
    let tmp = config_dir.join(format!("control.json.tmp-{pid}"));

    let data = serde_json::json!({
        "port": port,
        "token": token,
        "pid": pid,
        "version": env!("CARGO_PKG_VERSION"),
    });
    let text = serde_json::to_string_pretty(&data)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    std::fs::write(&tmp, text.as_bytes())?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        let _ = std::fs::set_permissions(&tmp, perms);
    }

    let res = std::fs::rename(&tmp, &target);
    if res.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    res
}

/// Standard configuration directory resolution for CourseCraft.
#[must_use]
pub fn default_config_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME")
            .map(|h| PathBuf::from(h).join("Library/Application Support/CourseCraft"))
    }
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("CourseCraft"))
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(|xdg| PathBuf::from(xdg).join("coursecraft"))
            .or_else(|| {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config/coursecraft"))
            })
    }
}
