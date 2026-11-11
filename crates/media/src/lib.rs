//! CourseCraft media decoding (L2).
//!
//! Turns image bytes into RGBA pixels for the renderer. Input is hostile:
//! dimensions and decoded size are capped and nothing panics.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

use std::io::Cursor;

use image::{ImageError, ImageFormat, ImageReader};

pub use image;

#[cfg(test)]
mod tests;

pub mod animation;
pub mod font;
pub mod svg;
pub mod webvtt;
pub use animation::{
    Animation, AnimationFrame, MAX_ANIMATION_BYTES, MAX_ANIMATION_FRAMES, MAX_FRAME_DELAY_MS,
    MIN_FRAME_DELAY_MS, TINY_FRAME_DELAY_MS, clamp_frame_delay_ms, decode_animation,
};
pub use font::{FontFileStyle, FontInfo, MAX_FONT_BYTES, font_info};
pub use svg::{MAX_SVG_BYTES, MAX_SVG_ELEMENTS, decode_svg, svg_natural_size};
pub use webvtt::{CaptionError, Cue, cue_text_at, parse_webvtt};

/// Default maximum permitted image width (8192 px).
pub const DEFAULT_MAX_WIDTH: u32 = 8192;

/// Default maximum permitted image height (8192 px).
pub const DEFAULT_MAX_HEIGHT: u32 = 8192;

/// Default maximum permitted decoded buffer size (64 MiB).
pub const DEFAULT_MAX_DECODED_BYTES: u64 = 64 * 1024 * 1024;

/// Resource limits applied during image decoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Maximum allowed width in pixels.
    pub max_width: u32,
    /// Maximum allowed height in pixels.
    pub max_height: u32,
    /// Maximum allowed decoded pixel buffer size in bytes (width * height * 4 for RGBA).
    pub max_decoded_bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_width: DEFAULT_MAX_WIDTH,
            max_height: DEFAULT_MAX_HEIGHT,
            max_decoded_bytes: DEFAULT_MAX_DECODED_BYTES,
        }
    }
}

/// Decoded image in 8-bit unmultiplied RGBA format.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rgba8Image {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Raw unmultiplied RGBA pixel bytes (length is `width * height * 4`).
    pub pixels: Vec<u8>,
}

/// Media decoding errors. Every hostile or invalid input produces an error, never a panic.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum MediaError {
    /// Format is not supported or could not be determined.
    #[error("unsupported image format")]
    Unsupported,
    /// Dimensions or decoded pixel size exceed configured limits.
    #[error("image too large: {width}x{height}")]
    TooLarge { width: u32, height: u32 },
    /// Image data is truncated, malformed, or corrupt.
    #[error("corrupt image: {0}")]
    Corrupt(String),
    /// Failed to encode image into requested format.
    #[error("failed to encode image: {0}")]
    Encode(String),
    /// Font file exceeds the permitted size.
    #[error("font too large: {bytes} bytes (max {max})")]
    FontTooLarge { bytes: usize, max: usize },
    /// Font file is truncated, malformed, or has no usable name.
    #[error("invalid font: {0}")]
    InvalidFont(String),
    /// SVG file exceeds the permitted size.
    #[error("SVG too large: {bytes} bytes (max {max})")]
    SvgTooLarge { bytes: usize, max: usize },
    /// SVG expands to more elements/nodes than permitted.
    #[error("SVG too complex: {count} elements (max {max})")]
    SvgTooComplex { count: usize, max: usize },
    /// Animation exceeds the frame or decoded-byte cap.
    #[error("animation too large: {frames} frames, {bytes} decoded bytes")]
    AnimationTooLarge { frames: usize, bytes: u64 },
}

impl From<ImageError> for MediaError {
    fn from(err: ImageError) -> Self {
        match err {
            ImageError::Unsupported(_) => MediaError::Unsupported,
            ImageError::Limits(_) => MediaError::TooLarge {
                width: 0,
                height: 0,
            },
            other => MediaError::Corrupt(other.to_string()),
        }
    }
}

/// Decode an image from raw bytes into an 8-bit RGBA pixel buffer.
///
/// Guesses the image format from the byte content (not file extension).
/// Reads header dimensions first and validates them against `limits` before allocating
/// or decoding pixel data. For animated formats (e.g. GIF), only the first frame is decoded.
///
/// # Errors
/// Returns [`MediaError`] if the format is unsupported, dimensions or memory exceed limits,
/// or the file bytes are corrupt or truncated.
pub fn decode_image(bytes: &[u8], limits: &Limits) -> Result<Rgba8Image, MediaError> {
    if bytes.is_empty() {
        return Err(MediaError::Unsupported);
    }

    let cursor = Cursor::new(bytes);
    let reader = ImageReader::new(cursor)
        .with_guessed_format()
        .map_err(|e| MediaError::Corrupt(e.to_string()))?;

    let format = match reader.format() {
        Some(fmt) => fmt,
        None => return Err(MediaError::Unsupported),
    };

    match format {
        ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::WebP | ImageFormat::Gif => {}
        _ => return Err(MediaError::Unsupported),
    }

    // Read dimensions first without decoding pixels
    let (width, height) = reader.into_dimensions().map_err(MediaError::from)?;

    if width == 0 || height == 0 {
        return Err(MediaError::Corrupt(
            "image dimensions must be non-zero".to_string(),
        ));
    }

    if width > limits.max_width || height > limits.max_height {
        return Err(MediaError::TooLarge { width, height });
    }

    let Some(pixel_count) = (width as u64).checked_mul(height as u64) else {
        return Err(MediaError::TooLarge { width, height });
    };
    let Some(decoded_bytes) = pixel_count.checked_mul(4) else {
        return Err(MediaError::TooLarge { width, height });
    };
    if decoded_bytes > limits.max_decoded_bytes {
        return Err(MediaError::TooLarge { width, height });
    }

    // Prepare reader with strict limits
    let mut decode_reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut image_limits = image::Limits::default();
    image_limits.max_image_width = Some(limits.max_width);
    image_limits.max_image_height = Some(limits.max_height);
    image_limits.max_alloc = Some(limits.max_decoded_bytes);
    decode_reader.limits(image_limits);

    let dyn_img = decode_reader.decode().map_err(|e| match e {
        ImageError::Limits(_) => MediaError::TooLarge { width, height },
        ImageError::Unsupported(_) => MediaError::Unsupported,
        other => MediaError::Corrupt(other.to_string()),
    })?;

    let rgba = dyn_img.to_rgba8();
    let (decoded_w, decoded_h) = rgba.dimensions();
    let pixels = rgba.into_raw();

    Ok(Rgba8Image {
        width: decoded_w,
        height: decoded_h,
        pixels,
    })
}

/// Encode an 8-bit RGBA pixel buffer as PNG bytes.
///
/// # Errors
/// Returns [`MediaError::Corrupt`] or [`MediaError::TooLarge`] if dimensions are zero,
/// or [`MediaError::Encode`] if buffer sizing or encoding fails.
pub fn encode_png_rgba(width: u32, height: u32, pixels: &[u8]) -> Result<Vec<u8>, MediaError> {
    use image::ImageEncoder;

    if width == 0 || height == 0 {
        return Err(MediaError::Corrupt(
            "image dimensions must be non-zero".to_string(),
        ));
    }

    let Some(pixel_count) = (width as u64).checked_mul(height as u64) else {
        return Err(MediaError::TooLarge { width, height });
    };
    let Some(expected_bytes) = pixel_count.checked_mul(4) else {
        return Err(MediaError::TooLarge { width, height });
    };
    if (pixels.len() as u64) != expected_bytes {
        return Err(MediaError::Encode(format!(
            "invalid pixel buffer size: expected {expected_bytes} bytes, got {}",
            pixels.len()
        )));
    }

    let mut out = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut out);
    encoder
        .write_image(pixels, width, height, image::ExtendedColorType::Rgba8)
        .map_err(|e| MediaError::Encode(e.to_string()))?;

    Ok(out)
}
