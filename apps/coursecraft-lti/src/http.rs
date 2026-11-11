//! A tiny fixed-shape HTTP/1.1 server over `std::net`, and the request/response
//! types the handlers use. One request per connection.

use std::collections::BTreeMap;
use std::io::{self, Read, Write};
use std::net::TcpStream;

/// Requests with more header bytes than this are refused with `413`.
pub const MAX_HEADER_BYTES: usize = 64 * 1024;
/// Bodies larger than this are refused with `413` without being read.
pub const MAX_BODY_BYTES: usize = 1024 * 1024;
const MAX_HEADERS: usize = 100;

/// A parsed request. Header names are lowercase; cookies live in `headers`.
#[derive(Clone, Debug, Default)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub query: BTreeMap<String, String>,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}

impl Request {
    /// Read one cookie by name from the `Cookie` header.
    #[must_use]
    pub fn cookie(&self, name: &str) -> Option<&str> {
        let raw = self.headers.get("cookie")?;
        for pair in raw.split(';') {
            let Some((key, value)) = pair.split_once('=') else {
                continue;
            };
            if key.trim() == name {
                return Some(value.trim());
            }
        }
        None
    }

    /// The body as UTF-8, if it is valid UTF-8.
    #[must_use]
    pub fn body_text(&self) -> Option<&str> {
        std::str::from_utf8(&self.body).ok()
    }
}

/// Why a request could not be parsed into a [`Request`].
#[derive(Debug)]
pub enum ReadError {
    Io(io::Error),
    BadRequest(String),
    TooLarge,
}

/// Read and parse one request from `stream`.
///
/// # Errors
/// Returns [`ReadError`] on I/O failure, malformed headers, an unsupported
/// transfer encoding, or a header/body over the caps.
pub fn read_request(stream: &mut TcpStream) -> Result<Request, ReadError> {
    let mut buffer = Vec::with_capacity(8192);
    let mut chunk = [0u8; 8192];
    let header_end = loop {
        if let Some(end) = find_headers_end(&buffer) {
            if end > MAX_HEADER_BYTES {
                return Err(ReadError::TooLarge);
            }
            break end;
        }
        if buffer.len() >= MAX_HEADER_BYTES {
            return Err(ReadError::TooLarge);
        }
        let read = stream.read(&mut chunk).map_err(ReadError::Io)?;
        if read == 0 {
            return Err(ReadError::BadRequest(
                "connection closed before headers".into(),
            ));
        }
        let room = MAX_HEADER_BYTES.saturating_sub(buffer.len());
        let take = read.min(room);
        let Some(part) = chunk.get(..take) else {
            return Err(ReadError::TooLarge);
        };
        buffer.extend_from_slice(part);
    };

    let head = buffer
        .get(..header_end)
        .ok_or_else(|| ReadError::BadRequest("headers are truncated".into()))?;

    let mut parsed_headers = [httparse::EMPTY_HEADER; MAX_HEADERS];
    let mut parsed = httparse::Request::new(&mut parsed_headers);
    let status = parsed
        .parse(head)
        .map_err(|e| ReadError::BadRequest(format!("headers: {e}")))?;
    if status.is_partial() {
        return Err(ReadError::BadRequest("headers are incomplete".into()));
    }
    let method = parsed
        .method
        .ok_or_else(|| ReadError::BadRequest("request has no method".into()))?
        .to_string();
    let target = parsed
        .path
        .ok_or_else(|| ReadError::BadRequest("request has no path".into()))?;

    let mut headers: BTreeMap<String, String> = BTreeMap::new();
    for header in parsed.headers.iter() {
        let name = header.name.to_ascii_lowercase();
        let value = String::from_utf8_lossy(header.value).trim().to_string();
        if name == "cookie" {
            if let Some(existing) = headers.get_mut("cookie") {
                existing.push_str("; ");
                existing.push_str(&value);
            } else {
                headers.insert(name, value);
            }
        } else {
            headers.insert(name, value);
        }
    }

    if headers.contains_key("transfer-encoding") {
        return Err(ReadError::BadRequest(
            "chunked request bodies are not supported".into(),
        ));
    }
    let content_length = match headers.get("content-length") {
        Some(value) => value
            .parse::<usize>()
            .map_err(|_| ReadError::BadRequest("invalid Content-Length".into()))?,
        None => 0,
    };
    if content_length > MAX_BODY_BYTES {
        return Err(ReadError::TooLarge);
    }

    let (raw_path, raw_query) = match target.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (target, None),
    };
    let path = normalize_target(raw_path);
    let query = raw_query.map(parse_form).unwrap_or_default();

    let mut body = buffer.get(header_end..).unwrap_or_default().to_vec();
    if body.len() > content_length {
        body.truncate(content_length);
    }
    while body.len() < content_length {
        let remaining = content_length.saturating_sub(body.len());
        let want = remaining.min(chunk.len());
        let Some(part) = chunk.get_mut(..want) else {
            return Err(ReadError::TooLarge);
        };
        let read = stream.read(part).map_err(ReadError::Io)?;
        if read == 0 {
            return Err(ReadError::BadRequest(
                "connection closed inside the body".into(),
            ));
        }
        let Some(part) = chunk.get(..read) else {
            return Err(ReadError::TooLarge);
        };
        body.extend_from_slice(part);
    }

    Ok(Request {
        method,
        path,
        query,
        headers,
        body,
    })
}

/// A response to write back, always with `Connection: close`.
#[derive(Clone, Debug)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    /// An empty response with `status`.
    #[must_use]
    pub fn status(status: u16) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: Vec::new(),
        }
    }

    /// Plain text.
    #[must_use]
    pub fn text(status: u16, body: impl Into<String>) -> Self {
        let mut response = Self::text_body(status, body.into().into_bytes());
        response
            .headers
            .push(("Content-Type".into(), "text/plain; charset=utf-8".into()));
        response
    }

    /// An HTML page.
    #[must_use]
    pub fn html(status: u16, body: impl Into<String>) -> Self {
        let mut response = Self::text_body(status, body.into().into_bytes());
        response
            .headers
            .push(("Content-Type".into(), "text/html; charset=utf-8".into()));
        response
    }

    /// A JSON value.
    #[must_use]
    pub fn json(status: u16, value: &serde_json::Value) -> Self {
        let mut response = Self::text_body(status, serde_json::to_vec(value).unwrap_or_default());
        response
            .headers
            .push(("Content-Type".into(), "application/json".into()));
        response
    }

    /// Bytes with a known content type.
    #[must_use]
    pub fn file(bytes: Vec<u8>, content_type: &str) -> Self {
        let mut response = Self::text_body(200, bytes);
        response
            .headers
            .push(("Content-Type".into(), content_type.into()));
        response
    }

    /// A `302` redirect.
    #[must_use]
    pub fn redirect(location: &str) -> Self {
        let mut response = Self::status(302);
        response.headers.push(("Location".into(), location.into()));
        response
    }

    /// Set the launch session cookie.
    pub fn set_session_cookie(&mut self, value: &str) {
        self.headers.push((
            "Set-Cookie".into(),
            format!("cc_lti={value}; Path=/; HttpOnly; Secure; SameSite=None"),
        ));
    }

    fn text_body(status: u16, body: Vec<u8>) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body,
        }
    }

    /// Write the response to `stream`.
    ///
    /// # Errors
    /// Returns the underlying I/O error.
    pub fn write(&self, stream: &mut TcpStream) -> io::Result<()> {
        let mut head = format!(
            "HTTP/1.1 {} {}\r\nContent-Length: {}\r\nConnection: close\r\n",
            self.status,
            reason(self.status),
            self.body.len()
        );
        for (name, value) in &self.headers {
            head.push_str(name);
            head.push_str(": ");
            head.push_str(value);
            head.push_str("\r\n");
        }
        head.push_str("\r\n");
        stream.write_all(head.as_bytes())?;
        stream.write_all(&self.body)?;
        stream.flush()
    }
}

/// Content type by file extension, for the published course files.
#[must_use]
pub fn content_type_for(path: &str) -> &'static str {
    let extension = match path.rsplit_once('.') {
        Some((_, extension)) => extension.to_ascii_lowercase(),
        None => String::new(),
    };
    match extension.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "wasm" => "application/wasm",
        "json" => "application/json",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "mp3" => "audio/mpeg",
        "mp4" => "video/mp4",
        "m4a" => "audio/mp4",
        "wav" => "audio/wav",
        "vtt" => "text/vtt; charset=utf-8",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "woff2" => "font/woff2",
        _ => "application/octet-stream",
    }
}

/// Decode `application/x-www-form-urlencoded` text into a map. The first value
/// wins; malformed pairs are skipped.
#[must_use]
pub fn parse_form(text: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for pair in text.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (key, value) = match pair.split_once('=') {
            Some((key, value)) => (key, value),
            None => (pair, ""),
        };
        let Some(key) = percent_decode(key) else {
            continue;
        };
        let Some(value) = percent_decode(value) else {
            continue;
        };
        map.entry(key).or_insert(value);
    }
    map
}

/// Decode `%XX` escapes and `+` as a space. Returns `None` for invalid UTF-8 or
/// an incomplete escape.
#[must_use]
pub fn percent_decode(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while let Some(byte) = bytes.get(index).copied() {
        match byte {
            b'%' => {
                let high = hex_value(*bytes.get(index.checked_add(1)?)?)?;
                let low = hex_value(*bytes.get(index.checked_add(2)?)?)?;
                out.push(high.checked_mul(16)?.checked_add(low)?);
                index = index.saturating_add(3);
            }
            b'+' => {
                out.push(b' ');
                index = index.saturating_add(1);
            }
            other => {
                out.push(other);
                index = index.saturating_add(1);
            }
        }
    }
    String::from_utf8(out).ok()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte.saturating_sub(b'0')),
        b'a'..=b'f' => Some(byte.saturating_sub(b'a').saturating_add(10)),
        b'A'..=b'F' => Some(byte.saturating_sub(b'A').saturating_add(10)),
        _ => None,
    }
}

fn find_headers_end(bytes: &[u8]) -> Option<usize> {
    bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|position| position.saturating_add(4))
}

fn normalize_target(target: &str) -> String {
    let absolute = target
        .strip_prefix("http://")
        .or_else(|| target.strip_prefix("https://"));
    match absolute {
        Some(rest) => match rest.find('/') {
            Some(slash) => rest.get(slash..).unwrap_or("/").to_string(),
            None => "/".to_string(),
        },
        None if target.is_empty() => "/".to_string(),
        None => target.to_string(),
    }
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        204 => "No Content",
        302 => "Found",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Payload Too Large",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        _ => "Unknown",
    }
}
