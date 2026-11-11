//! Frame time accumulation and tick generation.

use coursecraft_logic::Input;

/// Accumulates frame time and emits whole-millisecond [`Input::Tick`] events.
///
/// Carries sub-millisecond remainder across frames. Ignores negative or non-finite
/// delta times, and caps any single advance step at 1000 ms.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Ticker {
    remainder_seconds: f64,
}

impl Ticker {
    /// Create a new [`Ticker`] with zero initial remainder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Advance time by `dt_seconds`.
    ///
    /// Returns `Some(Input::Tick { ms })` if at least 1 millisecond has accumulated,
    /// or `None` if `dt_seconds` was ignored or less than 1 ms accumulated.
    pub fn advance(&mut self, dt_seconds: f32) -> Option<Input> {
        if !dt_seconds.is_finite() || dt_seconds <= 0.0 {
            return None;
        }

        // Cap one step at 1.0s (1000 ms) of real time.
        let dt = f64::from(dt_seconds.min(1.0));
        let total_seconds = self.remainder_seconds + dt;
        // Small epsilon absorbs f32->f64 representation error (0.020f32 is just under 0.02).
        let total_ms = (total_seconds * 1000.0 + 1e-6).floor();
        if !total_ms.is_finite() || total_ms <= 0.0 {
            self.remainder_seconds = total_seconds;
            return None;
        }

        let ms = (total_ms as u32).min(1000);
        if ms >= 1000 {
            self.remainder_seconds = 0.0;
        } else {
            self.remainder_seconds = (total_seconds - (f64::from(ms) / 1000.0)).max(0.0);
        }

        Some(Input::Tick { ms })
    }

    /// Reset any accumulated sub-millisecond remainder.
    pub fn reset(&mut self) {
        self.remainder_seconds = 0.0;
    }
}
