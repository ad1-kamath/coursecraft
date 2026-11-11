//! Animated GIF and WebP decoding (L2).
//!
//! Frames are decoded up front behind hard caps so a hostile file cannot pin
//! memory. Timing is normalised to what browsers do: a zero or tiny delay is
//! treated as 100 ms, and every other delay is clamped to 20..=10000 ms.

use std::io::Cursor;

use image::codecs::gif::GifDecoder;
use image::codecs::webp::WebPDecoder;
use image::{AnimationDecoder, ImageError, ImageFormat, ImageReader};

use crate::{Limits, MediaError, Rgba8Image};

/// Largest number of frames accepted in one animation.
pub const MAX_ANIMATION_FRAMES: usize = 300;

/// Largest total decoded animation size accepted, in bytes (256 MiB).
pub const MAX_ANIMATION_BYTES: u64 = 256 * 1024 * 1024;

/// Smallest frame delay kept, in milliseconds.
pub const MIN_FRAME_DELAY_MS: u32 = 20;

/// Largest frame delay kept, in milliseconds.
pub const MAX_FRAME_DELAY_MS: u32 = 10_000;

/// Delay used for 0 ms and other tiny delays, matching browsers (100 ms).
pub const TINY_FRAME_DELAY_MS: u32 = 100;

/// One decoded animation frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnimationFrame {
    /// Delay before the next frame, in milliseconds.
    pub delay_ms: u32,
    /// Full-canvas frame pixels.
    pub image: Rgba8Image,
}

/// A decoded looping animation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Animation {
    /// Frame width in pixels.
    pub width: u32,
    /// Frame height in pixels.
    pub height: u32,
    /// Frames in playback order; never empty.
    pub frames: Vec<AnimationFrame>,
}

impl Animation {
    /// Total loop duration in milliseconds.
    #[must_use]
    pub fn total_duration_ms(&self) -> u64 {
        self.frames.iter().fold(0u64, |sum, frame| {
            sum.saturating_add(u64::from(frame.delay_ms))
        })
    }

    /// Index of the frame shown `elapsed_ms` after the animation started.
    ///
    /// Always loops. Returns 0 for an empty frame list (which decoding never
    /// produces) and for a zero-duration animation.
    #[must_use]
    pub fn frame_index_at(&self, elapsed_ms: u64) -> usize {
        if self.frames.is_empty() {
            return 0;
        }
        let total = self.total_duration_ms();
        if total == 0 {
            return 0;
        }
        let position = elapsed_ms % total;
        let mut end = 0u64;
        for (index, frame) in self.frames.iter().enumerate() {
            end = end.saturating_add(u64::from(frame.delay_ms));
            if position < end {
                return index;
            }
        }
        self.frames.len().saturating_sub(1)
    }

    /// Milliseconds from `elapsed_ms` until the next frame is due.
    #[must_use]
    pub fn ms_until_next_frame(&self, elapsed_ms: u64) -> u32 {
        if self.frames.is_empty() {
            return TINY_FRAME_DELAY_MS;
        }
        let total = self.total_duration_ms();
        if total == 0 {
            return TINY_FRAME_DELAY_MS;
        }
        let position = elapsed_ms % total;
        let mut end = 0u64;
        for frame in &self.frames {
            end = end.saturating_add(u64::from(frame.delay_ms));
            if position < end {
                let remaining = end.saturating_sub(position);
                return u32::try_from(remaining)
                    .unwrap_or(MAX_FRAME_DELAY_MS)
                    .max(1);
            }
        }
        1
    }
}

/// Normalise a GIF/WebP frame delay.
#[must_use]
pub fn clamp_frame_delay_ms(delay_ms: u32) -> u32 {
    if delay_ms <= 10 {
        TINY_FRAME_DELAY_MS
    } else {
        delay_ms.clamp(MIN_FRAME_DELAY_MS, MAX_FRAME_DELAY_MS)
    }
}

/// Decode an animated GIF or WebP into frames.
///
/// Returns `Ok(None)` for a still image (PNG, JPEG, a single-frame GIF/WebP).
/// The whole animation is decoded up front so the playback timing never blocks.
///
/// # Errors
/// Returns [`MediaError`] for unsupported/corrupt input, or when the frame,
/// per-frame size or total decoded size cap is exceeded.
pub fn decode_animation(bytes: &[u8], limits: &Limits) -> Result<Option<Animation>, MediaError> {
    if bytes.is_empty() {
        return Err(MediaError::Unsupported);
    }

    let format = match ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| MediaError::Corrupt(e.to_string()))?
        .format()
    {
        Some(format) => format,
        None => return Err(MediaError::Unsupported),
    };

    match format {
        ImageFormat::Gif | ImageFormat::WebP => {
            // Validate the canvas before the decoder allocates a frame.
            let (width, height) = ImageReader::with_format(Cursor::new(bytes), format)
                .into_dimensions()
                .map_err(MediaError::from)?;
            if width == 0 || height == 0 {
                return Err(MediaError::Corrupt(
                    "animation dimensions must be non-zero".to_string(),
                ));
            }
            if width > limits.max_width || height > limits.max_height {
                return Err(MediaError::TooLarge { width, height });
            }
            let frame_bytes = (width as u64)
                .saturating_mul(height as u64)
                .saturating_mul(4);
            if frame_bytes > limits.max_decoded_bytes {
                return Err(MediaError::TooLarge { width, height });
            }
            if format == ImageFormat::Gif {
                decode_frames(
                    GifDecoder::new(Cursor::new(bytes)).map_err(MediaError::from)?,
                    limits,
                )
            } else {
                decode_frames(
                    WebPDecoder::new(Cursor::new(bytes)).map_err(MediaError::from)?,
                    limits,
                )
            }
        }
        // Any other accepted image type is a still picture.
        ImageFormat::Png | ImageFormat::Jpeg => Ok(None),
        _ => Err(MediaError::Unsupported),
    }
}

fn decode_frames<'a, D>(decoder: D, limits: &Limits) -> Result<Option<Animation>, MediaError>
where
    D: AnimationDecoder<'a>,
{
    let mut frames: Vec<AnimationFrame> = Vec::new();
    let mut total_bytes = 0u64;
    let mut dimensions: Option<(u32, u32)> = None;

    for frame in decoder.into_frames() {
        if frames.len() >= MAX_ANIMATION_FRAMES {
            return Err(MediaError::AnimationTooLarge {
                frames: frames.len().saturating_add(1),
                bytes: total_bytes,
            });
        }
        let frame = frame.map_err(map_image_error)?;
        let delay = frame.delay();
        let (numerator, denominator) = delay.numer_denom_ms();
        let delay_ms = numerator
            .checked_div(denominator)
            .unwrap_or(TINY_FRAME_DELAY_MS);

        let buffer = frame.into_buffer();
        let (width, height) = buffer.dimensions();
        if width == 0 || height == 0 {
            return Err(MediaError::Corrupt(
                "animation frame has zero size".to_string(),
            ));
        }
        if width > limits.max_width || height > limits.max_height {
            return Err(MediaError::TooLarge { width, height });
        }
        if let Some((expected_w, expected_h)) = dimensions {
            if width != expected_w || height != expected_h {
                return Err(MediaError::Corrupt(
                    "animation frames have inconsistent sizes".to_string(),
                ));
            }
        } else {
            dimensions = Some((width, height));
        }

        let Some(frame_bytes) = (width as u64)
            .checked_mul(height as u64)
            .and_then(|pixels| pixels.checked_mul(4))
        else {
            return Err(MediaError::TooLarge { width, height });
        };
        total_bytes = total_bytes.saturating_add(frame_bytes);
        if total_bytes > MAX_ANIMATION_BYTES || frame_bytes > limits.max_decoded_bytes {
            return Err(MediaError::AnimationTooLarge {
                frames: frames.len().saturating_add(1),
                bytes: total_bytes,
            });
        }

        frames.push(AnimationFrame {
            delay_ms: clamp_frame_delay_ms(delay_ms),
            image: Rgba8Image {
                width,
                height,
                pixels: buffer.into_raw(),
            },
        });
    }

    if frames.len() <= 1 {
        return Ok(None);
    }

    let Some((width, height)) = dimensions else {
        return Ok(None);
    };
    Ok(Some(Animation {
        width,
        height,
        frames,
    }))
}

fn map_image_error(error: ImageError) -> MediaError {
    match error {
        ImageError::Unsupported(_) => MediaError::Unsupported,
        ImageError::Limits(_) => MediaError::TooLarge {
            width: 0,
            height: 0,
        },
        other => MediaError::Corrupt(other.to_string()),
    }
}
