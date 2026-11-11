//! Font file inspection: read a family name and style from a TrueType or
//! OpenType file, rejecting hostile input instead of panicking.

use skrifa::MetadataProvider as _;
use skrifa::attribute::Style;
use skrifa::string::StringId;

use crate::MediaError;

/// Maximum permitted size of a course font file (15 MiB).
pub const MAX_FONT_BYTES: usize = 15 * 1024 * 1024;

/// The four faces a course can register per family.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontFileStyle {
    Regular,
    Bold,
    Italic,
    BoldItalic,
}

impl FontFileStyle {
    /// A stable lower-case name, used by the editor and tests.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Regular => "regular",
            Self::Bold => "bold",
            Self::Italic => "italic",
            Self::BoldItalic => "bold_italic",
        }
    }
}

/// Family and style read from a font file's `name` table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontInfo {
    /// Family name, from nameID 16 (typographic family) or 1.
    pub family: String,
    /// Subfamily name, from nameID 17 (typographic subfamily) or 2.
    pub subfamily: String,
    /// The face style inferred from the subfamily and the OS/2 attributes.
    pub style: FontFileStyle,
}

/// Inspect a TrueType or OpenType font.
///
/// Reads nameID 16/17 when present, falling back to 1/2. The file must be a
/// single font (not a collection) and must be at most [`MAX_FONT_BYTES`].
///
/// # Errors
/// Returns [`MediaError::FontTooLarge`] when the input exceeds the cap and
/// [`MediaError::InvalidFont`] when it does not parse or has no family name.
pub fn font_info(bytes: &[u8]) -> Result<FontInfo, MediaError> {
    if bytes.len() > MAX_FONT_BYTES {
        return Err(MediaError::FontTooLarge {
            bytes: bytes.len(),
            max: MAX_FONT_BYTES,
        });
    }
    if bytes.is_empty() {
        return Err(MediaError::InvalidFont("font file is empty".to_string()));
    }

    let font = skrifa::FontRef::from_index(bytes, 0)
        .map_err(|error| MediaError::InvalidFont(error.to_string()))?;

    // MetadataProvider's string helper prefers English strings and falls back
    // to the first record, which is what a file browser shows.
    let read = |id: u16| -> Option<String> {
        font.localized_strings(StringId::new(id))
            .english_or_first()
            .map(|value| value.to_string())
    };

    let family = read(16)
        .or_else(|| read(1))
        .map(|value| clean_name(&value))
        .unwrap_or_default();
    if family.is_empty() {
        return Err(MediaError::InvalidFont(
            "font has no family name".to_string(),
        ));
    }
    let subfamily = read(17)
        .or_else(|| read(2))
        .map(|value| clean_name(&value))
        .unwrap_or_default();

    let attributes = font.attributes();
    let lower = subfamily.to_ascii_lowercase();
    let bold = lower.contains("bold")
        || lower.contains("black")
        || lower.contains("heavy")
        || lower.contains("semibold")
        || attributes.weight.value() >= 600.0;
    let italic = lower.contains("italic")
        || lower.contains("oblique")
        || !matches!(attributes.style, Style::Normal);

    let style = match (bold, italic) {
        (false, false) => FontFileStyle::Regular,
        (true, false) => FontFileStyle::Bold,
        (false, true) => FontFileStyle::Italic,
        (true, true) => FontFileStyle::BoldItalic,
    };

    Ok(FontInfo {
        family,
        subfamily,
        style,
    })
}

/// Trim, drop control characters, and cap the name to 64 characters.
fn clean_name(value: &str) -> String {
    value
        .chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .chars()
        .take(64)
        .collect::<String>()
        .trim()
        .to_string()
}
