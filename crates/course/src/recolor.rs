//! Course-wide colour replacement, and the colour text rules it uses.
//!
//! [`recolor_course`] changes every use of a colour in a course: fills,
//! outlines, gradient stops, shadows, glows, text and text runs, slide
//! backgrounds, states, table cells, markers, paths and the theme. Colours are
//! matched by their six RGB digits after normalising (`#abc` is `#AABBCC`, case
//! is ignored). A use keeps its own alpha: an 8-digit `#RRGGBBAA` use stays
//! 8-digit with the same `AA`. Text that is not a colour is left alone.
//!
//! It works by writing the course to JSON and reading it back with a rule
//! installed in [`crate::Color`]'s `Deserialize`, so every field of type
//! `Color` is covered, including ones added later, and nothing else is touched.

use std::cell::RefCell;

use crate::Course;

/// Normalise a colour to `#RRGGBB` or `#RRGGBBAA`, upper case. Accepts the short
/// forms `#RGB` and `#RGBA`. `None` when the text is not a colour.
#[must_use]
pub fn normalize_hex(text: &str) -> Option<String> {
    let digits = text.trim().strip_prefix('#')?;
    if !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let upper = digits.to_ascii_uppercase();
    match upper.len() {
        6 | 8 => Some(format!("#{upper}")),
        3 | 4 => {
            let doubled: String = upper.chars().flat_map(|c| [c, c]).collect();
            Some(format!("#{doubled}"))
        }
        _ => None,
    }
}

/// The six RGB digits of a normalised colour, without the `#`.
fn rgb_of(normal: &str) -> &str {
    normal.get(1..7).unwrap_or("")
}

/// The alpha digits of a normalised colour: two characters, or empty.
fn alpha_of(normal: &str) -> &str {
    normal.get(7..).unwrap_or("")
}

/// Whether two colours have the same six RGB digits. Alpha is ignored. False
/// when either is not a colour.
#[must_use]
pub fn same_rgb(a: &str, b: &str) -> bool {
    match (normalize_hex(a), normalize_hex(b)) {
        (Some(a), Some(b)) => rgb_of(&a) == rgb_of(&b),
        _ => false,
    }
}

/// Why a recolour could not be done.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecolorError {
    /// A colour was not `#RGB`, `#RGBA`, `#RRGGBB` or `#RRGGBBAA`.
    BadColor(String),
    /// The course could not be written out and read back.
    Round(String),
}

impl std::fmt::Display for RecolorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadColor(text) => write!(f, "{text:?} is not a colour such as #2F6FEB"),
            Self::Round(why) => write!(f, "the course could not be recoloured: {why}"),
        }
    }
}

impl std::error::Error for RecolorError {}

struct Active {
    /// `(from RGB digits, to RGB digits)`, both six upper-case hex digits.
    from: String,
    to: String,
    changed: usize,
}

thread_local! {
    static ACTIVE: RefCell<Option<Active>> = const { RefCell::new(None) };
}

/// Called for every colour read while a recolour runs; returns it unchanged
/// when none is running or the colour does not match.
pub(crate) fn map_color(text: String) -> String {
    ACTIVE.with(|cell| {
        let Ok(mut guard) = cell.try_borrow_mut() else {
            return text;
        };
        let Some(active) = guard.as_mut() else {
            return text;
        };
        let Some(normal) = normalize_hex(&text) else {
            return text;
        };
        if rgb_of(&normal) != active.from {
            return text;
        }
        active.changed = active.changed.saturating_add(1);
        format!("#{}{}", active.to, alpha_of(&normal))
    })
}

/// Clears the running recolour when dropped, even on an early return.
struct Reset;

impl Drop for Reset {
    fn drop(&mut self) {
        ACTIVE.with(|cell| {
            if let Ok(mut guard) = cell.try_borrow_mut() {
                *guard = None;
            }
        });
    }
}

/// A copy of `course` with every use of `from` replaced by `to`, and how many
/// colours changed. `from` and `to` are compared by their RGB digits; each use
/// keeps its own alpha. With `include_palette` false the theme palette entries
/// are left as they are.
///
/// # Errors
/// [`RecolorError::BadColor`] when `from` or `to` is not a colour.
pub fn recolor_course(
    course: &Course,
    from: &str,
    to: &str,
    include_palette: bool,
) -> Result<(Course, usize), RecolorError> {
    let (Some(from_normal), Some(to_normal)) = (normalize_hex(from), normalize_hex(to)) else {
        let bad = if normalize_hex(from).is_none() {
            from
        } else {
            to
        };
        return Err(RecolorError::BadColor(bad.to_owned()));
    };
    let (from_rgb, to_rgb) = (rgb_of(&from_normal), rgb_of(&to_normal));
    if from_rgb == to_rgb {
        return Ok((course.clone(), 0));
    }
    let mut source = course.clone();
    let palette = std::mem::take(&mut source.theme.palette);
    let value = serde_json::to_value(&source).map_err(|e| RecolorError::Round(e.to_string()))?;
    ACTIVE.with(|cell| {
        *cell.borrow_mut() = Some(Active {
            from: from_rgb.to_owned(),
            to: to_rgb.to_owned(),
            changed: 0,
        });
    });
    let reset = Reset;
    let result = serde_json::from_value::<Course>(value);
    let mut changed = ACTIVE.with(|cell| cell.borrow().as_ref().map_or(0, |a| a.changed));
    drop(reset);
    let mut out = result.map_err(|e| RecolorError::Round(e.to_string()))?;

    // A default theme is left out of the JSON, so it was never read: map it here.
    if palette.is_empty() && crate::Theme::default() == course.theme {
        changed = changed.saturating_add(recolor_theme(&mut out.theme, from_rgb, to_rgb));
    }
    out.theme.palette = if include_palette {
        palette
            .into_iter()
            .map(|mut entry| {
                if let Some(normal) = normalize_hex(&entry.color.0)
                    && rgb_of(&normal) == from_rgb
                {
                    entry.color.0 = format!("#{to_rgb}{}", alpha_of(&normal));
                    changed = changed.saturating_add(1);
                }
                entry
            })
            .collect()
    } else {
        palette
    };
    Ok((out, changed))
}

/// Replace the colours of a theme (not its palette); how many changed.
fn recolor_theme(theme: &mut crate::Theme, from_rgb: &str, to_rgb: &str) -> usize {
    let mut changed = 0;
    for color in [
        &mut theme.accent,
        &mut theme.background,
        &mut theme.text,
        &mut theme.player.bar,
        &mut theme.player.bar_text,
    ] {
        if let Some(normal) = normalize_hex(&color.0)
            && rgb_of(&normal) == from_rgb
        {
            color.0 = format!("#{to_rgb}{}", alpha_of(&normal));
            changed += 1;
        }
    }
    changed
}
