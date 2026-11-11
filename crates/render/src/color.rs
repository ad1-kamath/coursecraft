//! Color parsing for CourseCraft render items.

use coursecraft_course::Color;

/// Parse a CourseCraft [`Color`] string into an [`egui::Color32`].
///
/// Accepts `#RRGGBB` (opaque) and `#RRGGBBAA` (unmultiplied alpha).
/// Any other format, non-hex character, or malformed string returns `None`.
#[must_use]
pub fn parse_color(c: &Color) -> Option<egui::Color32> {
    let hex = c.0.strip_prefix('#')?;
    if !hex.is_ascii() {
        return None;
    }
    match hex.len() {
        6 => {
            let r = u8::from_str_radix(hex.get(0..2)?, 16).ok()?;
            let g = u8::from_str_radix(hex.get(2..4)?, 16).ok()?;
            let b = u8::from_str_radix(hex.get(4..6)?, 16).ok()?;
            Some(egui::Color32::from_rgb(r, g, b))
        }
        8 => {
            let r = u8::from_str_radix(hex.get(0..2)?, 16).ok()?;
            let g = u8::from_str_radix(hex.get(2..4)?, 16).ok()?;
            let b = u8::from_str_radix(hex.get(4..6)?, 16).ok()?;
            let a = u8::from_str_radix(hex.get(6..8)?, 16).ok()?;
            Some(egui::Color32::from_rgba_unmultiplied(r, g, b, a))
        }
        _ => None,
    }
}
