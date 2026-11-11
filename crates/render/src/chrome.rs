//! Player chrome helpers shared by the frame, the preview and tests.
//!
//! The chrome widget itself lives in [`crate::player_frame`]. This module keeps
//! the read-only helpers used by the transport controls, seek bar and
//! navigation buttons.

use coursecraft_logic::{Input, Player};

/// Default background colour of the player control strip (#1B1B1B).
pub const DEFAULT_BAR_COLOR: egui::Color32 = egui::Color32::from_rgb(0x1B, 0x1B, 0x1B);

/// Default text colour of the player control strip (#FFFFFF).
pub const DEFAULT_BAR_TEXT_COLOR: egui::Color32 = egui::Color32::from_rgb(0xFF, 0xFF, 0xFF);

/// Whether the current slide's clock has reached its end.
#[must_use]
pub fn at_slide_end(player: &Player<'_>) -> bool {
    let info = player.seek_info();
    player.state().base.ended || (info.duration_ms > 0 && info.elapsed_ms >= info.duration_ms)
}

/// Inputs for the main play/pause button.
///
/// At the end it acts as Replay, as the icon switches to the replay glyph.
#[must_use]
pub fn play_pause_inputs(player: &Player<'_>) -> Vec<Input> {
    if at_slide_end(player) {
        return replay_inputs(player);
    }
    if player.player_paused() {
        vec![Input::PlayerResume]
    } else {
        vec![Input::PlayerPause]
    }
}

/// Inputs for Replay: seek to the start, resuming first when the learner paused.
#[must_use]
pub fn replay_inputs(player: &Player<'_>) -> Vec<Input> {
    let mut inputs = Vec::new();
    if player.player_paused() {
        inputs.push(Input::PlayerResume);
    }
    inputs.push(Input::PlayerSeek { ms: 0 });
    inputs
}

/// Compute progress fraction in `[0.0, 1.0]`, guarded against duration 0 and hostile floats.
#[must_use]
pub fn progress_fraction(current_ms: u32, duration_ms: u32) -> f32 {
    if duration_ms == 0 {
        return 0.0;
    }
    let frac = current_ms as f32 / duration_ms as f32;
    if !frac.is_finite() {
        return 0.0;
    }
    frac.clamp(0.0, 1.0)
}

/// Format milliseconds into `m:ss`.
#[must_use]
pub fn format_ms(ms: u32) -> String {
    let total_secs = ms / 1000;
    let mins = total_secs / 60;
    let secs = total_secs % 60;
    format!("{mins}:{secs:02}")
}

/// Returns `(is_first, is_last)` for `current_id` among `slides`.
///
/// If `slides` is empty, or if `current_id` is not found, returns `(true, true)`
/// so that navigation is safely disabled.
#[must_use]
pub fn nav_limits<'a>(slides: impl IntoIterator<Item = &'a str>, current_id: &str) -> (bool, bool) {
    let mut iter = slides.into_iter();
    let Some(first) = iter.next() else {
        return (true, true);
    };

    let mut count: usize = 1;
    let mut current_idx: Option<usize> = if first == current_id { Some(0) } else { None };

    for item in iter {
        if item == current_id && current_idx.is_none() {
            current_idx = Some(count);
        }
        count = count.saturating_add(1);
    }

    let Some(idx) = current_idx else {
        return (true, true);
    };

    let is_first = idx == 0;
    let is_last = idx == count.saturating_sub(1);
    (is_first, is_last)
}
