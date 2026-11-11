//! Safe SVG parsing and rasterisation (L2).
//!
//! SVGs come from authors and PowerPoint decks, so every input is hostile:
//! bytes, element count and output pixels are capped, and `<image>` references
//! may only be data URLs. Text is deliberately not rendered: the build has no
//! text feature, so no system font is ever loaded.

use resvg::tiny_skia::Pixmap;
use resvg::usvg::{self, ImageHrefResolver, Options, Tree};

use crate::{Limits, MediaError, Rgba8Image};

/// Largest SVG file accepted, in bytes (5 MiB).
pub const MAX_SVG_BYTES: usize = 5 * 1024 * 1024;

/// Largest number of elements/nodes accepted in one SVG.
pub const MAX_SVG_ELEMENTS: usize = 20_000;

/// Parse options that forbid external file and network references.
fn options() -> Options<'static> {
    Options {
        resources_dir: None,
        image_href_resolver: ImageHrefResolver {
            // Data URLs (inline raster images) are decoded in memory.
            resolve_data: ImageHrefResolver::default_data_resolver(),
            // Any path or URL is refused: usvg never reads a file or the network.
            resolve_string: Box::new(|_href, _options| None),
        },
        ..Options::default()
    }
}

fn parse(bytes: &[u8]) -> Result<Tree, MediaError> {
    if bytes.len() > MAX_SVG_BYTES {
        return Err(MediaError::SvgTooLarge {
            bytes: bytes.len(),
            max: MAX_SVG_BYTES,
        });
    }
    if bytes.is_empty() {
        return Err(MediaError::Unsupported);
    }
    let tree = Tree::from_data(bytes, &options()).map_err(map_parse_error)?;
    let count = count_nodes(&tree);
    if count > MAX_SVG_ELEMENTS {
        return Err(MediaError::SvgTooComplex {
            count,
            max: MAX_SVG_ELEMENTS,
        });
    }
    Ok(tree)
}

fn map_parse_error(error: usvg::Error) -> MediaError {
    match error {
        usvg::Error::ElementsLimitReached => MediaError::SvgTooComplex {
            count: MAX_SVG_ELEMENTS.saturating_add(1),
            max: MAX_SVG_ELEMENTS,
        },
        other => MediaError::Corrupt(format!("invalid SVG: {other}")),
    }
}

/// Count every group, shape, image and text node, stopping at the cap.
fn count_nodes(tree: &Tree) -> usize {
    let mut count = 0usize;
    let mut stack = vec![tree.root()];
    while let Some(group) = stack.pop() {
        for node in group.children() {
            count = count.saturating_add(1);
            if count > MAX_SVG_ELEMENTS {
                return count;
            }
            if let usvg::Node::Group(child) = node {
                stack.push(child);
            }
        }
    }
    count
}

/// The SVG's natural size from its `width`, `height` and `viewBox`.
///
/// Returns `None` for oversized, malformed or dimensionless SVGs.
#[must_use]
pub fn svg_natural_size(bytes: &[u8]) -> Option<(f32, f32)> {
    let tree = parse(bytes).ok()?;
    let size = tree.size();
    let (width, height) = (size.width(), size.height());
    if width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0 {
        Some((width, height))
    } else {
        None
    }
}

/// Rasterise an SVG to exactly `target_px`, limited by `limits`.
///
/// The target is clamped to the pixel cap rather than rejected: a huge drawn
/// size still produces the largest safe image. Bytes, element count and output
/// size are capped; external `<image>` references are dropped.
///
/// # Errors
/// Returns [`MediaError`] when the file is too large, malformed, has too many
/// elements, or the requested output cannot be allocated.
pub fn decode_svg(
    bytes: &[u8],
    target_px: (u32, u32),
    limits: &Limits,
) -> Result<Rgba8Image, MediaError> {
    let tree = parse(bytes)?;
    let size = tree.size();
    let natural_w = size.width();
    let natural_h = size.height();
    if !natural_w.is_finite() || !natural_h.is_finite() || natural_w <= 0.0 || natural_h <= 0.0 {
        return Err(MediaError::Corrupt("SVG has an invalid size".to_string()));
    }

    let (width, height) = clamp_target(target_px, limits)?;

    let mut pixmap = Pixmap::new(width, height).ok_or(MediaError::TooLarge { width, height })?;
    let transform = resvg::tiny_skia::Transform::from_scale(
        width as f32 / natural_w,
        height as f32 / natural_h,
    );
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    let pixels = pixmap.take_demultiplied();
    let expected = (width as usize)
        .saturating_mul(height as usize)
        .saturating_mul(4);
    if pixels.len() != expected {
        return Err(MediaError::Corrupt(
            "SVG rasteriser returned an unexpected buffer size".to_string(),
        ));
    }

    Ok(Rgba8Image {
        width,
        height,
        pixels,
    })
}

/// Clamp a requested raster size to the configured pixel limits.
fn clamp_target(target: (u32, u32), limits: &Limits) -> Result<(u32, u32), MediaError> {
    let max_width = limits.max_width.max(1);
    let max_height = limits.max_height.max(1);
    let mut width = target.0.max(1).min(max_width);
    let mut height = target.1.max(1).min(max_height);

    let max_bytes = limits.max_decoded_bytes.max(4);
    let mut bytes = (width as u64)
        .saturating_mul(height as u64)
        .saturating_mul(4);
    if bytes > max_bytes {
        // Walk down proportionally until the buffer fits. Integer division is
        // enough: at most a few dozen steps for realistic limits.
        let ratio = (max_bytes as f64 / bytes as f64).sqrt();
        width = ((width as f64 * ratio).floor() as u32).max(1);
        height = ((height as f64 * ratio).floor() as u32).max(1);
        bytes = (width as u64)
            .saturating_mul(height as u64)
            .saturating_mul(4);
        while bytes > max_bytes {
            if width >= height && width > 1 {
                width -= 1;
            } else if height > 1 {
                height -= 1;
            } else {
                return Err(MediaError::TooLarge { width, height });
            }
            bytes = (width as u64)
                .saturating_mul(height as u64)
                .saturating_mul(4);
        }
    }

    Ok((width, height))
}
