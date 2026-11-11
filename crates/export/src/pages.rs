//! Shared page model for the review exports (PDF and Word).
//!
//! One [`ReviewPage`] is a slide picture plus the words that belong with it:
//! the heading, the speaker notes and the slide's visible text. Both document
//! writers consume the same list, so they always agree on what a reviewer sees.

use std::cmp::Ordering;

use coursecraft_course::{Course, Object, ObjectKind, Slide};
use coursecraft_render::{MediaSource, RenderMode, RenderOptions, render_slide_rgba};
use image::codecs::jpeg::JpegEncoder;

/// Longest text kept for one page, in characters.
const MAX_TEXT_CHARS: usize = 20_000;

/// Width limits for the rendered slide picture, in pixels.
const MIN_IMAGE_WIDTH: u32 = 320;
const MAX_IMAGE_WIDTH: u32 = 2400;

/// Why a review export could not be produced.
#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    /// A slide could not be drawn.
    #[error("render failed: {0}")]
    Render(String),
    /// A picture could not be encoded.
    #[error("image encoding failed: {0}")]
    Encoding(String),
}

/// What to put in a review document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReviewOptions {
    /// Include the slide notes.
    pub include_notes: bool,
    /// Include the slide's text.
    pub include_text: bool,
    /// Add a page for every slide layer.
    pub include_layers: bool,
    /// Append the question-bank slides.
    pub include_banks: bool,
    /// Width of the rendered slide picture, in pixels (clamped to 320..=2400).
    pub image_width: u32,
}

impl Default for ReviewOptions {
    fn default() -> Self {
        Self {
            include_notes: true,
            include_text: true,
            include_layers: false,
            include_banks: true,
            image_width: 1600,
        }
    }
}

/// One page of a review document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewPage {
    /// Heading, such as `"3. Check your knowledge"`.
    pub heading: String,
    /// The slide drawn as a JPEG.
    pub jpeg: Vec<u8>,
    /// Picture width in pixels.
    pub width: u32,
    /// Picture height in pixels.
    pub height: u32,
    /// Speaker notes, empty when excluded.
    pub notes: String,
    /// The slide's plain text, top to bottom.
    pub text: Vec<String>,
}

/// Build the review pages for `course`, following play order.
///
/// Bank slides are appended after the scene slides when
/// [`ReviewOptions::include_banks`] is set. Each page is drawn in authoring
/// mode at [`ReviewOptions::image_width`], clamped to 320..=2400 pixels.
///
/// # Errors
/// Returns [`ExportError::Render`] when a slide cannot be drawn and
/// [`ExportError::Encoding`] when its picture cannot be encoded.
pub fn review_pages(
    course: &Course,
    media: &dyn MediaSource,
    options: &ReviewOptions,
) -> Result<Vec<ReviewPage>, ExportError> {
    let width = options.image_width.clamp(MIN_IMAGE_WIDTH, MAX_IMAGE_WIDTH);
    let mut pages = Vec::new();
    for (index, slide) in course.slides().enumerate() {
        let number = index.saturating_add(1);
        let heading = format!("{number}. {}", slide.title);
        pages.push(build_page(
            course, media, slide, heading, width, None, options,
        )?);
        if options.include_layers {
            for layer in &slide.layers {
                let heading = format!("{number}. {} ▸ {}", slide.title, layer.name);
                pages.push(build_page(
                    course,
                    media,
                    slide,
                    heading,
                    width,
                    Some(layer.id.as_str()),
                    options,
                )?);
            }
        }
    }
    if options.include_banks {
        for bank in &course.banks {
            for slide in &bank.slides {
                let heading = format!("Bank {}: {}", bank.title, slide.title);
                pages.push(build_page(
                    course,
                    media,
                    slide,
                    heading.clone(),
                    width,
                    None,
                    options,
                )?);
                if options.include_layers {
                    for layer in &slide.layers {
                        let heading = format!("{heading} ▸ {}", layer.name);
                        pages.push(build_page(
                            course,
                            media,
                            slide,
                            heading,
                            width,
                            Some(layer.id.as_str()),
                            options,
                        )?);
                    }
                }
            }
        }
    }
    Ok(pages)
}

fn build_page(
    course: &Course,
    media: &dyn MediaSource,
    slide: &Slide,
    heading: String,
    width: u32,
    layer: Option<&str>,
    options: &ReviewOptions,
) -> Result<ReviewPage, ExportError> {
    let render = RenderOptions {
        width,
        mode: RenderMode::Authoring,
        layers: layer.map(|id| vec![id.to_owned()]).unwrap_or_default(),
        at_ms: None,
    };
    let image = render_slide_rgba(course, &slide.id, &render, media)
        .map_err(|error| ExportError::Render(error.to_string()))?;
    let jpeg = encode_jpeg(&image.pixels, image.width, image.height)?;

    let notes = if options.include_notes {
        capped(&slide.notes)
    } else {
        String::new()
    };
    let text = if options.include_text {
        let mut texts = match layer {
            Some(id) => slide.layer(id).map(layer_texts).unwrap_or_default(),
            None => slide_texts(slide),
        };
        let budget = MAX_TEXT_CHARS.saturating_sub(notes.chars().count());
        cap_texts(&mut texts, budget);
        texts
    } else {
        Vec::new()
    };

    Ok(ReviewPage {
        heading,
        jpeg,
        width: image.width,
        height: image.height,
        notes,
        text,
    })
}

fn encode_jpeg(pixels: &[u8], width: u32, height: u32) -> Result<Vec<u8>, ExportError> {
    let rgb: Vec<u8> = pixels
        .chunks(4)
        .flat_map(|pixel| pixel.iter().take(3).copied())
        .collect();
    let mut jpeg = Vec::new();
    let mut encoder = JpegEncoder::new_with_quality(&mut jpeg, 85);
    encoder
        .encode(&rgb, width, height, image::ExtendedColorType::Rgb8)
        .map_err(|error| ExportError::Encoding(error.to_string()))?;
    Ok(jpeg)
}

/// The base layer's text, top to bottom, then the question.
fn slide_texts(slide: &Slide) -> Vec<String> {
    let mut positioned: Vec<(f32, String)> = Vec::new();
    for object in &slide.objects {
        for text in object_texts(object) {
            positioned.push((object.frame.y, text));
        }
    }
    positioned.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal));

    let mut texts: Vec<String> = positioned.into_iter().map(|(_, text)| text).collect();
    if let Some(question) = &slide.question {
        if !question.prompt.trim().is_empty() {
            texts.push(question.prompt.clone());
        }
        for choice in &question.choices {
            if !choice.text.trim().is_empty() {
                texts.push(choice.text.clone());
            }
        }
    }
    texts
}

/// A layer's text, top to bottom.
fn layer_texts(layer: &coursecraft_course::Layer) -> Vec<String> {
    let mut positioned: Vec<(f32, String)> = Vec::new();
    for object in &layer.objects {
        for text in object_texts(object) {
            positioned.push((object.frame.y, text));
        }
    }
    positioned.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal));
    positioned.into_iter().map(|(_, text)| text).collect()
}

fn object_texts(object: &Object) -> Vec<String> {
    match &object.kind {
        ObjectKind::Text { text, .. } => non_empty(text),
        ObjectKind::Shape {
            text: Some(text), ..
        } => non_empty(&text.text),
        ObjectKind::Button { label, .. } => non_empty(label),
        ObjectKind::Table { rows, .. } => rows
            .iter()
            .flat_map(|row| row.cells.iter())
            .flat_map(|cell| non_empty(&cell.text))
            .collect(),
        _ => Vec::new(),
    }
}

fn non_empty(text: &str) -> Vec<String> {
    if text.trim().is_empty() {
        Vec::new()
    } else {
        vec![text.to_owned()]
    }
}

fn capped(text: &str) -> String {
    text.chars().take(MAX_TEXT_CHARS).collect()
}

fn cap_texts(texts: &mut Vec<String>, budget: usize) {
    let mut budget = budget;
    let mut kept = Vec::with_capacity(texts.len());
    for text in texts.drain(..) {
        if budget == 0 {
            break;
        }
        let length = text.chars().count();
        if length <= budget {
            kept.push(text);
            budget = budget.saturating_sub(length);
        } else {
            kept.push(text.chars().take(budget).collect());
            budget = 0;
        }
    }
    *texts = kept;
}
