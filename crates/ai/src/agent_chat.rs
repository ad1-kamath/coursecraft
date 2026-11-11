//! Chat through an installed coding agent (Claude Code or Codex).
//!
//! Everything here is pure: a [`ChatRequest`] becomes one prompt and a list of
//! images, the command lines are built exactly as the plan says, and the answers
//! are read back into the [`ChatReply`] the rest of the editor already uses.
//! The desktop app owns the temporary folder and the processes.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::agents::AgentId;
use crate::{ChatReply, ChatRequest, MessageContent};

/// The line that asks the agent for the answer alone.
pub const REPLY_RULE: &str = "Reply with only the answer, no commentary.";

/// Largest image this module will decode from a data URL: 8 MiB.
pub const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;

/// Most characters of an agent's output an error sentence quotes.
pub const MAX_EXCERPT: usize = 200;

/// What powers CourseCraft's own AI features.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AiSource {
    /// An OpenAI-compatible API endpoint (the original behaviour).
    #[default]
    Endpoint,
    /// The author's installed Claude Code CLI.
    ClaudeCode,
    /// The author's installed Codex CLI.
    Codex,
}

impl AiSource {
    /// Every source, in the order the AI features tab shows them.
    pub const ALL: [Self; 3] = [Self::ClaudeCode, Self::Codex, Self::Endpoint];

    /// The agent this source runs through, or `None` for an endpoint.
    #[must_use]
    pub const fn agent_id(self) -> Option<AgentId> {
        match self {
            Self::Endpoint => None,
            Self::ClaudeCode => Some(AgentId::ClaudeCode),
            Self::Codex => Some(AgentId::Codex),
        }
    }

    /// True when this source is an installed agent.
    #[must_use]
    pub const fn is_agent(self) -> bool {
        self.agent_id().is_some()
    }

    /// The name shown to the author.
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Endpoint => "An API endpoint",
            Self::ClaudeCode => "Claude Code",
            Self::Codex => "Codex",
        }
    }
}

/// One image decoded from a chat request, ready to be written to disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentImage {
    /// The name the prompt uses, such as `image-1.png`.
    pub file_name: String,
    /// The media type from the data URL.
    pub media_type: String,
    /// The decoded image bytes.
    pub bytes: Vec<u8>,
}

/// One chat request as a prompt and its images.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentPrompt {
    /// The whole prompt, sent on standard input.
    pub text: String,
    /// Images to write beside the prompt before the agent runs.
    pub images: Vec<AgentImage>,
}

/// Turn a chat request into one prompt and at most one image per data URL.
///
/// The messages are joined in order, with a blank line between them. Image data
/// URLs are decoded, capped at [`MAX_IMAGE_BYTES`] each, and named in the
/// prompt. The reply rule is always the last line.
///
/// # Errors
/// Returns a plain sentence when an image is not a supported data URL, is
/// larger than the cap, or cannot be decoded.
pub fn build_prompt(request: &ChatRequest) -> Result<AgentPrompt, String> {
    let mut parts: Vec<String> = Vec::new();
    let mut images: Vec<AgentImage> = Vec::new();

    for message in &request.messages {
        let (text, urls) = match &message.content {
            MessageContent::Text(text) => (text.as_str(), &[][..]),
            MessageContent::TextAndImages { text, images } => (text.as_str(), images.as_slice()),
        };
        if !text.trim().is_empty() {
            parts.push(text.to_string());
        }
        for url in urls {
            let image = decode_image(url, images.len().saturating_add(1))?;
            parts.push(format!(
                "Image {} is saved as {} in the current folder.",
                images.len().saturating_add(1),
                image.file_name
            ));
            images.push(image);
        }
    }

    if parts.is_empty() {
        parts.push("Follow the instructions in the course task.".to_string());
    }
    parts.push(REPLY_RULE.to_string());

    Ok(AgentPrompt {
        text: parts.join("\n\n"),
        images,
    })
}

fn decode_image(url: &str, number: usize) -> Result<AgentImage, String> {
    let Some(rest) = url.strip_prefix("data:") else {
        return Err("An image in the request wasn't a data URL.".to_string());
    };
    let Some((metadata, payload)) = rest.split_once(',') else {
        return Err("An image in the request wasn't a data URL.".to_string());
    };
    let mut pieces = metadata.split(';');
    let media_type = pieces.next().unwrap_or("").trim();
    if !media_type.starts_with("image/") {
        return Err("An image in the request wasn't an image.".to_string());
    }
    if !pieces.any(|piece| piece.eq_ignore_ascii_case("base64")) {
        return Err("An image in the request wasn't base64-encoded.".to_string());
    }
    let bytes = base64_decode(payload)
        .ok_or_else(|| "An image in the request couldn't be decoded.".to_string())?;
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(format!(
            "An image is larger than the {} MB limit.",
            MAX_IMAGE_BYTES / (1024 * 1024)
        ));
    }
    let extension = image_extension(media_type);
    Ok(AgentImage {
        file_name: format!("image-{number}.{extension}"),
        media_type: media_type.to_string(),
        bytes,
    })
}

fn image_extension(media_type: &str) -> &'static str {
    match media_type.to_ascii_lowercase().as_str() {
        "image/jpeg" | "image/jpg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        _ => "png",
    }
}

/// Standard base64, ignoring whitespace. `None` when a character is invalid.
fn base64_decode(input: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(input.len().div_ceil(4).saturating_mul(3));
    let mut buffer = 0_u32;
    let mut bits = 0_u32;
    for byte in input.bytes() {
        if byte.is_ascii_whitespace() {
            continue;
        }
        if byte == b'=' {
            break;
        }
        let value = match byte {
            b'A'..=b'Z' => u32::from(byte - b'A'),
            b'a'..=b'z' => u32::from(byte - b'a') + 26,
            b'0'..=b'9' => u32::from(byte - b'0') + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        };
        buffer = (buffer << 6) | value;
        bits = bits.saturating_add(6);
        if bits >= 8 {
            bits = bits.saturating_sub(8);
            out.push(((buffer >> bits) & 0xff) as u8);
            buffer &= (1_u32 << bits).saturating_sub(1);
        }
    }
    Some(out)
}

/// The command line for one agent, program first.
///
/// Claude Code reads the prompt from standard input and may read the images in
/// its working folder. Codex also receives one `--image` per image and writes
/// its answer to `answer_path`.
///
/// # Errors
/// Returns a plain sentence for an endpoint source, which has no process.
pub fn chat_argv(
    source: AiSource,
    answer_path: &Path,
    images: &[AgentImage],
) -> Result<Vec<String>, String> {
    match source {
        AiSource::ClaudeCode => Ok(
            ["claude", "-p", "--output-format", "json", "--tools", "Read"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
        ),
        AiSource::Codex => {
            let mut argv: Vec<String> = [
                "codex",
                "exec",
                "--skip-git-repo-check",
                "--sandbox",
                "read-only",
                "--output-last-message",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect();
            argv.push(answer_path.to_string_lossy().into_owned());
            for image in images {
                argv.push("--image".to_string());
                argv.push(image.file_name.clone());
            }
            argv.push("-".to_string());
            Ok(argv)
        }
        AiSource::Endpoint => Err("An API endpoint doesn't run a command.".to_string()),
    }
}

/// Read Claude Code's `--output-format json` answer.
///
/// `is_error: true` returns the JSON's `result` as a failure detail. Junk or a
/// missing `result` returns an excerpt instead.
///
/// # Errors
/// Returns a plain detail sentence; the app turns it into the author-facing
/// error.
pub fn parse_claude_answer(stdout: &str) -> Result<ChatReply, String> {
    let value: serde_json::Value =
        serde_json::from_str(stdout.trim()).map_err(|_| output_excerpt(stdout))?;
    let Some(object) = value.as_object() else {
        return Err(output_excerpt(stdout));
    };
    let is_error = object
        .get("is_error")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let result = object
        .get("result")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .trim();
    if is_error {
        return Err(if result.is_empty() {
            output_excerpt(stdout)
        } else {
            result.to_string()
        });
    }
    if result.is_empty() {
        return Err("The agent returned an empty answer.".to_string());
    }
    Ok(ChatReply {
        text: result.to_string(),
        truncated: false,
    })
}

/// Read Codex's `--output-last-message` answer file.
///
/// # Errors
/// Returns a plain detail sentence when the file is empty.
pub fn parse_codex_answer(text: &str) -> Result<ChatReply, String> {
    let answer = text.trim();
    if answer.is_empty() {
        return Err("The agent returned an empty answer.".to_string());
    }
    Ok(ChatReply {
        text: answer.to_string(),
        truncated: false,
    })
}

/// The sentence shown when an agent exits with an error or reports one.
///
/// Output that mentions login or authentication gets the sign-in sentence,
/// because that is the common cause. Other output is quoted at most
/// [`MAX_EXCERPT`] characters. Common `sk-…` keys are hidden from the quote.
#[must_use]
pub fn failure_sentence(source: AiSource, output: &str) -> String {
    let (name, program) = match source {
        AiSource::ClaudeCode => ("Claude Code", "claude"),
        AiSource::Codex => ("Codex", "codex"),
        AiSource::Endpoint => ("The AI", "coursecraft"),
    };
    let lower = output.to_ascii_lowercase();
    if lower.contains("login")
        || lower.contains("log in")
        || lower.contains("logged in")
        || lower.contains("auth")
        || lower.contains("sign in")
        || lower.contains("signed in")
    {
        return format!("{name} isn't signed in. Run `{program}` once in a terminal to sign in.");
    }
    let excerpt = output_excerpt(output);
    if excerpt.is_empty() {
        format!("{name} stopped with an error.")
    } else {
        format!("{name} stopped with an error. {excerpt}")
    }
}

/// At most [`MAX_EXCERPT`] characters, with controls made visible and keys
/// hidden.
#[must_use]
pub fn output_excerpt(output: &str) -> String {
    let redacted = redact_keys(output);
    let mut out: String = redacted
        .chars()
        .map(|character| {
            if character.is_control() && character != '\n' && character != '\t' {
                ' '
            } else {
                character
            }
        })
        .take(MAX_EXCERPT)
        .collect();
    out = out.split_whitespace().collect::<Vec<_>>().join(" ");
    out
}

fn redact_keys(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for token in text.split_inclusive(char::is_whitespace) {
        let word = token.trim_end_matches(char::is_whitespace);
        if word.starts_with("sk-") || word.starts_with("pk-") {
            out.push_str("[hidden]");
            out.push_str(token.get(word.len()..).unwrap_or(""));
        } else {
            out.push_str(token);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::alt_prompt::image_data_url;
    use crate::{ChatMessage, Role, base64_encode};

    fn request_without_images() -> ChatRequest {
        ChatRequest {
            messages: vec![
                ChatMessage {
                    role: Role::System,
                    content: MessageContent::Text("Be brief.".to_string()),
                },
                ChatMessage {
                    role: Role::User,
                    content: MessageContent::Text("Say hello.".to_string()),
                },
            ],
            max_tokens: Some(20),
            temperature: None,
        }
    }

    fn request_with_image() -> ChatRequest {
        ChatRequest {
            messages: vec![ChatMessage {
                role: Role::User,
                content: MessageContent::TextAndImages {
                    text: "Describe it.".to_string(),
                    images: vec![image_data_url("image/png", b"PNG bytes")],
                },
            }],
            max_tokens: None,
            temperature: None,
        }
    }

    #[test]
    fn sources_have_agents_and_names() {
        assert_eq!(AiSource::default(), AiSource::Endpoint);
        assert!(AiSource::Endpoint.agent_id().is_none());
        assert_eq!(AiSource::ClaudeCode.agent_id(), Some(AgentId::ClaudeCode));
        assert_eq!(AiSource::Codex.agent_id(), Some(AgentId::Codex));
        assert!(AiSource::Codex.is_agent());
        assert_eq!(AiSource::ALL.len(), 3);
    }

    #[test]
    fn prompt_joins_system_user_and_the_reply_rule() {
        let prompt = build_prompt(&request_without_images()).expect("prompt");
        assert!(prompt.images.is_empty());
        let system = prompt.text.find("Be brief.").expect("system");
        let user = prompt.text.find("Say hello.").expect("user");
        let rule = prompt.text.find(REPLY_RULE).expect("rule");
        assert!(system < user && user < rule, "{}", prompt.text);
        assert!(prompt.text.ends_with(REPLY_RULE), "{}", prompt.text);
    }

    #[test]
    fn prompt_decodes_images_and_names_them() {
        let prompt = build_prompt(&request_with_image()).expect("prompt");
        assert_eq!(prompt.images.len(), 1);
        let image = prompt.images.first().expect("image");
        assert_eq!(image.file_name, "image-1.png");
        assert_eq!(image.media_type, "image/png");
        assert_eq!(image.bytes, b"PNG bytes");
        assert!(prompt.text.contains("Describe it."));
        assert!(prompt.text.contains("image-1.png"));
        assert!(prompt.text.ends_with(REPLY_RULE));
    }

    #[test]
    fn prompt_refuses_a_non_data_url() {
        let request = ChatRequest {
            messages: vec![ChatMessage {
                role: Role::User,
                content: MessageContent::TextAndImages {
                    text: "x".to_string(),
                    images: vec!["https://example.com/image.png".to_string()],
                },
            }],
            max_tokens: None,
            temperature: None,
        };
        assert!(build_prompt(&request).is_err());
    }

    #[test]
    fn base64_round_trips_and_refuses_junk() {
        let bytes = b"\x00\x01hello world\xff";
        let encoded = base64_encode(bytes);
        assert_eq!(base64_decode(&encoded), Some(bytes.to_vec()));
        assert_eq!(base64_decode("Zm9v"), Some(b"foo".to_vec()));
        assert!(base64_decode("not base64!").is_none());
    }

    #[test]
    fn claude_code_argv_is_exact() {
        assert_eq!(
            chat_argv(AiSource::ClaudeCode, Path::new("/tmp/t/answer.txt"), &[]).expect("argv"),
            vec!["claude", "-p", "--output-format", "json", "--tools", "Read",]
        );
    }

    #[test]
    fn codex_argv_is_exact_and_lists_each_image() {
        let images = vec![
            AgentImage {
                file_name: "image-1.png".to_string(),
                media_type: "image/png".to_string(),
                bytes: Vec::new(),
            },
            AgentImage {
                file_name: "image-2.jpg".to_string(),
                media_type: "image/jpeg".to_string(),
                bytes: Vec::new(),
            },
        ];
        assert_eq!(
            chat_argv(AiSource::Codex, Path::new("/tmp/t/answer.txt"), &images).expect("argv"),
            vec![
                "codex",
                "exec",
                "--skip-git-repo-check",
                "--sandbox",
                "read-only",
                "--output-last-message",
                "/tmp/t/answer.txt",
                "--image",
                "image-1.png",
                "--image",
                "image-2.jpg",
                "-",
            ]
        );
    }

    #[test]
    fn an_endpoint_has_no_argv() {
        assert!(chat_argv(AiSource::Endpoint, Path::new("answer.txt"), &[]).is_err());
    }

    #[test]
    fn claude_json_success_is_the_result_text() {
        let reply = parse_claude_answer(r#"{"type":"result","is_error":false,"result":"OK"}"#)
            .expect("reply");
        assert_eq!(reply.text, "OK");
        assert!(!reply.truncated);
    }

    #[test]
    fn claude_json_is_error_is_a_failure_detail() {
        let error =
            parse_claude_answer(r#"{"type":"result","is_error":true,"result":"Not logged in"}"#)
                .expect_err("must fail");
        assert!(error.contains("Not logged in"), "{error}");
        assert!(
            failure_sentence(AiSource::ClaudeCode, &error).contains("isn't signed in"),
            "{error}"
        );
    }

    #[test]
    fn claude_junk_is_a_short_failure() {
        let error = parse_claude_answer("not json at all").expect_err("must fail");
        assert!(error.contains("not json"), "{error}");
        let sentence = failure_sentence(AiSource::ClaudeCode, &error);
        assert!(
            sentence.starts_with("Claude Code stopped with an error."),
            "{sentence}"
        );
    }

    #[test]
    fn codex_reads_the_answer_file() {
        let reply = parse_codex_answer("  A red ball.\n").expect("reply");
        assert_eq!(reply.text, "A red ball.");
        assert!(parse_codex_answer("  \n").is_err());
    }

    #[test]
    fn login_and_auth_output_gets_the_sign_in_sentence() {
        for word in [
            "please login first",
            "authentication failed",
            "not signed in",
        ] {
            let sentence = failure_sentence(AiSource::ClaudeCode, word);
            assert_eq!(
                sentence,
                "Claude Code isn't signed in. Run `claude` once in a terminal to sign in."
            );
        }
        assert_eq!(
            failure_sentence(AiSource::Codex, "login required"),
            "Codex isn't signed in. Run `codex` once in a terminal to sign in."
        );
    }

    #[test]
    fn other_failures_quote_at_most_200_characters() {
        let output = format!("boom\n{}", "x".repeat(500));
        let sentence = failure_sentence(AiSource::ClaudeCode, &output);
        assert!(
            sentence.starts_with("Claude Code stopped with an error. boom x"),
            "{sentence}"
        );
        let quoted = sentence
            .strip_prefix("Claude Code stopped with an error. ")
            .unwrap_or("");
        assert!(quoted.chars().count() <= MAX_EXCERPT, "{quoted}");
    }

    #[test]
    fn a_failure_never_quotes_a_key() {
        let sentence = failure_sentence(
            AiSource::Codex,
            "request failed with sk-very-secret-value and more detail",
        );
        assert!(!sentence.contains("sk-very-secret-value"), "{sentence}");
        assert!(sentence.contains("[hidden]"), "{sentence}");
    }

    #[test]
    fn source_serde_uses_kebab_case_and_defaults_to_endpoint() {
        assert_eq!(
            serde_json::to_string(&AiSource::ClaudeCode).expect("json"),
            "\"claude-code\""
        );
        assert_eq!(
            serde_json::from_str::<AiSource>("\"codex\"").expect("json"),
            AiSource::Codex
        );
    }
}
