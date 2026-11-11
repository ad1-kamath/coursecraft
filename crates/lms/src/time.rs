//! Formatting for SCORM 1.2 session time.

use std::fmt::Write as _;

/// Format millisecond duration into SCORM 1.2 `HHHH:MM:SS.SS` format.
///
/// Hours are formatted with at least 4 digits (zero-padded).
/// For inputs of 9999 hours or more, the value saturates at `"9999:99:99.99"`.
#[must_use]
pub fn format_session_time(ms: u64) -> String {
    let hundredths = (ms / 10) % 100;
    let total_seconds = ms / 1000;
    let seconds = total_seconds % 60;
    let total_minutes = total_seconds / 60;
    let minutes = total_minutes % 60;
    let hours = total_minutes / 60;

    if hours >= 9999 {
        return "9999:99:99.99".to_string();
    }

    let mut out = String::with_capacity(14);
    // Writing to String cannot fail; ignore Result.
    let _ = write!(out, "{hours:04}:{minutes:02}:{seconds:02}.{hundredths:02}");
    out
}

/// Format millisecond duration as an ISO 8601 duration for SCORM 2004
/// `cmi.session_time`, for example `PT1H2M3.45S`.
///
/// Uses `u128` arithmetic so `u64::MAX` cannot overflow or panic. Zero
/// components are omitted, and a zero duration is `PT0S`.
#[must_use]
pub fn format_session_time_iso(ms: u64) -> String {
    let ms = u128::from(ms);
    let total_seconds = ms / 1000;
    let hundredths = (ms % 1000) / 10;
    let seconds = total_seconds % 60;
    let total_minutes = total_seconds / 60;
    let minutes = total_minutes % 60;
    let hours = total_minutes / 60;

    let mut out = String::from("PT");
    // Writing to String cannot fail; ignore Result.
    if hours > 0 {
        let _ = write!(out, "{hours}H");
    }
    if minutes > 0 {
        let _ = write!(out, "{minutes}M");
    }
    if seconds > 0 || hundredths > 0 || (hours == 0 && minutes == 0) {
        if hundredths > 0 {
            let _ = write!(out, "{seconds}.{hundredths:02}S");
        } else {
            let _ = write!(out, "{seconds}S");
        }
    }
    out
}

/// Format a percentage as the plain decimal SCORM 2004 `cmi.score.scaled`
/// value, clamped to 0..=100 and never in exponent form (`0.85`, `1`, `0`).
#[must_use]
pub fn format_scaled_score(percent: u8) -> String {
    let percent = percent.min(100);
    let whole = percent / 100;
    let fraction = percent % 100;
    if fraction == 0 {
        return whole.to_string();
    }
    let mut out = format!("{whole}.{fraction:02}");
    while out.ends_with('0') {
        out.pop();
    }
    out
}
