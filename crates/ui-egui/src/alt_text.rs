//! Suggest alt text for images (§ M9 step 4).
//!
//! The window lists images that have no alt text on slides, layers and masters.
//! Nothing changes until `Apply`, which writes every ticked row through one
//! `object.update` batch (one undo step), the same command the inspector uses.
//!
//! One request runs at a time. A group is one `src`, even when the picture is
//! used on several slides: one answer fills every row in the group. The request
//! is built in `coursecraft_ai`; the app owns the transport. A late answer is
//! dropped when it is for another `src`, or after Cancel.

use crate::dialog::Dialog;
use std::sync::Arc;

use coursecraft_ai::{
    AiError, AltContext, ChatReply, ChatRequest, MAX_NEARBY_CHARS, alt_text_request,
    clean_alt_text, image_data_url,
};
use coursecraft_course::{Course, ObjectKind, Slide};
use coursecraft_media::image::{ImageFormat, RgbaImage, imageops::FilterType};
use coursecraft_media::{Limits, decode_image, encode_png_rgba};
use coursecraft_render::{SIZE_BODY, SIZE_SMALL, palette};
use egui::{Button, Id, Key, Modifiers, RichText, TextEdit, TextureId, Vec2, vec2};

use crate::{AiPurpose, Editor, EditorRequest};

/// Largest file sent to the endpoint: 8 MiB.
pub const MAX_SEND_BYTES: usize = 8 * 1024 * 1024;

/// Longest side of a downscaled image, in pixels.
pub const MAX_IMAGE_SIDE: u32 = 1024;

/// One image on one slide, layer or master that needs alt text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AltRow {
    /// Slide or master id that owns the object.
    pub slide: String,
    /// Object id on that slide.
    pub object: String,
    /// Media key of the picture.
    pub src: String,
    /// Human-readable place, such as `Slide 3 ▸ Banner Graphic`.
    pub location: String,
    /// The editable suggestion.
    pub suggestion: String,
    /// Whether `Apply` writes this row.
    pub ticked: bool,
    /// A send or preparation error for this row.
    pub error: Option<String>,
}

/// Images that share a `src` and therefore one request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AltGroup {
    /// Media key sent once.
    pub src: String,
    /// Rows the answer fills.
    pub row_indices: Vec<usize>,
    /// The slide whose text gives context for the request.
    pub slide: String,
}

/// The group currently in flight.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingAlt {
    /// What is being sent.
    pub group: AltGroup,
    /// One-based position in the run, for `Image 2 of 3`.
    pub number: usize,
}

/// A one-image suggestion started from the inspector's `Suggest` button.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineAlt {
    /// Slide that owns the object.
    pub slide: String,
    /// Object the answer belongs to.
    pub object: String,
    /// Media key the answer is for.
    pub src: String,
    /// Field value when the request started; an edited field wins.
    pub baseline: String,
}

/// Transient state for the Suggest Alt Text window.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AltTextState {
    /// Whether the window is open.
    pub open: bool,
    /// The list the author reviews.
    pub rows: Vec<AltRow>,
    /// Groups that have not been started yet.
    pub queue: Vec<AltGroup>,
    /// The group in flight.
    pub pending: Option<PendingAlt>,
    /// Number of images in the run when it started.
    pub total: usize,
    /// One-based number the next group will use.
    pub next_number: usize,
    /// Whether a run is active.
    pub running: bool,
    /// Images skipped because their file is not in the media store.
    pub missing_files: usize,
    /// A result line shown above the rows (Apply, for example).
    pub notice: Option<String>,
    /// The inspector's one-image request, if any.
    pub inline: Option<InlineAlt>,
    /// The inspector's last error.
    pub inline_error: Option<String>,
}

impl AltTextState {
    /// True while a run is active.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.running
    }
}

impl Editor {
    /// Open the Suggest Alt Text window with a fresh list.
    pub fn open_alt_text_dialog(&mut self) {
        if self.ui.alt.running {
            return;
        }
        let (rows, queue, missing_files) = collect(&self.session.course, &self.media);
        self.ui.alt = AltTextState {
            open: true,
            total: queue.len(),
            rows,
            queue,
            missing_files,
            next_number: 1,
            ..AltTextState::default()
        };
    }
}

/// Start a one-image suggestion for the inspector's `Suggest` button.
pub fn suggest_for_object(editor: &mut Editor, slide_id: &str, object_id: &str) {
    if !editor.ai_ready() {
        editor.open_ai_settings();
        return;
    }
    if editor.ui.alt.running || editor.ui.alt.inline.is_some() {
        return;
    }
    let Some(slide) = editor.session.course.slide(slide_id) else {
        return;
    };
    let Some((object, _)) = slide.object(object_id) else {
        return;
    };
    let ObjectKind::Image { src, alt, .. } = &object.kind else {
        return;
    };
    let baseline = if editor.selection.slide == slide_id
        && editor.selection.object.as_deref() == Some(object_id)
    {
        editor.ui.properties.image_alt.clone()
    } else {
        alt.clone()
    };
    match build_request(&editor.session.course, &editor.media, slide_id, src) {
        Ok(request) => {
            let src = src.clone();
            editor.ui.alt.inline = Some(InlineAlt {
                slide: slide_id.to_string(),
                object: object_id.to_string(),
                src: src.clone(),
                baseline,
            });
            editor.ui.alt.inline_error = None;
            editor.request(EditorRequest::AiChat {
                purpose: AiPurpose::AltText { src },
                request,
            });
        }
        Err(message) => {
            editor.ui.alt.inline_error = Some(message);
        }
    }
}

/// Hand a finished alt-text request to the window, or to the inspector.
pub fn set_result(editor: &mut Editor, src: &str, result: Result<ChatReply, String>) {
    let expected = editor
        .ui
        .alt
        .pending
        .as_ref()
        .is_some_and(|pending| pending.group.src == src);
    if editor.ui.alt.running && expected {
        let Some(pending) = editor.ui.alt.pending.take() else {
            return;
        };
        editor.ui.alt.running = false;
        match result {
            Ok(reply) => {
                if let Some(text) = clean_alt_text(&reply.text) {
                    for index in &pending.group.row_indices {
                        if let Some(row) = editor.ui.alt.rows.get_mut(*index) {
                            row.suggestion = text.clone();
                            row.ticked = true;
                            row.error = None;
                        }
                    }
                } else {
                    mark_group_error(
                        editor,
                        &pending.group,
                        "The endpoint returned no usable description.".to_string(),
                    );
                }
            }
            Err(message) => {
                let auth = message == AiError::RejectedKey.to_string();
                mark_group_error(editor, &pending.group, message);
                if auth {
                    editor.ui.alt.queue.clear();
                    editor.ui.alt.running = false;
                    editor.ui.alt.pending = None;
                    return;
                }
            }
        }
        start_next(editor);
        return;
    }
    set_inline_result(editor, src, result);
}

fn set_inline_result(editor: &mut Editor, src: &str, result: Result<ChatReply, String>) {
    let Some(inline) = editor.ui.alt.inline.clone() else {
        return;
    };
    if inline.src != src {
        return;
    }
    editor.ui.alt.inline = None;
    match result {
        Ok(reply) => {
            let Some(text) = clean_alt_text(&reply.text) else {
                editor.ui.alt.inline_error =
                    Some("The endpoint returned no usable description.".to_string());
                return;
            };
            let same_object = editor.selection.slide == inline.slide
                && editor.selection.object.as_deref() == Some(inline.object.as_str());
            let unchanged = editor.ui.properties.image_alt == inline.baseline;
            if same_object && unchanged {
                editor.ui.properties.image_alt = text;
            }
        }
        Err(message) => {
            editor.ui.alt.inline_error = Some(message);
        }
    }
}

/// Forget an in-flight alt-text result after Cancel.
pub fn set_cancelled(editor: &mut Editor, src: &str) {
    if editor
        .ui
        .alt
        .pending
        .as_ref()
        .is_some_and(|pending| pending.group.src == src)
    {
        editor.ui.alt.pending = None;
        editor.ui.alt.running = false;
    }
    if editor
        .ui
        .alt
        .inline
        .as_ref()
        .is_some_and(|inline| inline.src == src)
    {
        editor.ui.alt.inline = None;
    }
}

/// Escape closes the window only when nothing is running.
pub fn consume_escape(editor: &mut Editor, ctx: &egui::Context) {
    if !editor.ui.alt.open || editor.ui.alt.running {
        return;
    }
    if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
        close(editor);
    }
}

fn close(editor: &mut Editor) {
    editor.ui.alt.open = false;
    editor.ui.alt.rows.clear();
    editor.ui.alt.queue.clear();
    editor.ui.alt.pending = None;
    editor.ui.alt.running = false;
    editor.ui.alt.total = 0;
    editor.ui.alt.notice = None;
    editor.ui.alt.inline_error = None;
}

fn mark_group_error(editor: &mut Editor, group: &AltGroup, message: String) {
    for index in &group.row_indices {
        if let Some(row) = editor.ui.alt.rows.get_mut(*index) {
            row.error = Some(message.clone());
            row.ticked = false;
        }
    }
}

/// Start the next group, skipping oversized or unsupported images with a row
/// error. An empty queue ends the run.
fn start_next(editor: &mut Editor) {
    loop {
        let Some(group) = editor.ui.alt.queue.first().cloned() else {
            editor.ui.alt.running = false;
            editor.ui.alt.pending = None;
            return;
        };
        editor.ui.alt.queue.remove(0);
        let number = editor.ui.alt.next_number;
        match build_request(
            &editor.session.course,
            &editor.media,
            &group.slide,
            &group.src,
        ) {
            Ok(request) => {
                editor.ui.alt.next_number = number.saturating_add(1);
                editor.ui.alt.running = true;
                editor.ui.alt.pending = Some(PendingAlt {
                    group: group.clone(),
                    number,
                });
                editor.request(EditorRequest::AiChat {
                    purpose: AiPurpose::AltText { src: group.src },
                    request,
                });
                return;
            }
            Err(message) => {
                mark_group_error(editor, &group, message);
            }
        }
    }
}

/// Draw the window when it is open.
pub fn show_alt_text_window(
    editor: &mut Editor,
    ctx: &egui::Context,
    requests: &mut Vec<EditorRequest>,
) {
    if !editor.ui.alt.open {
        return;
    }

    let ready = editor.ai_ready();
    let mut open = true;
    let mut close_clicked = false;
    let mut suggest = false;
    let mut cancel = false;
    let mut apply = false;
    let mut open_settings = false;

    Dialog::new("Suggest Alt Text")
        .id(Id::unique(("suggest_alt_text_dialog", 0_u64)))
        .width(620.0)
        .open(&mut open)
        .resizable(true)
        .show_with_footer(ctx, |dialog| {
            let p = palette(ctx);
            dialog.body(|ui| {
                ui.set_max_width(620.0);
                if !ready {
                    if crate::ai_settings::show_ai_not_ready(ui, "suggest alt text") {
                        open_settings = true;
                    }
                    ui.add_space(8.0);
                    if ui.button("Cancel").clicked() {
                        close_clicked = true;
                    }
                    return;
                }

                ui.label(
                    RichText::new(
                        "CourseCraft sends each image with a little of its slide's text, \
                             then shows the suggestion here. Nothing changes until Apply.",
                    )
                    .size(SIZE_SMALL)
                    .color(p.text_secondary),
                );
                ui.add_space(6.0);

                if editor.ui.alt.rows.is_empty() {
                    ui.label("No images need alt text.");
                } else {
                    ui.label(
                        RichText::new(consent_line(editor, editor.ui.alt.total))
                            .size(SIZE_BODY)
                            .color(p.text_secondary),
                    );
                }
                if editor.ui.alt.missing_files > 0 {
                    ui.label(
                        RichText::new(missing_line(editor.ui.alt.missing_files))
                            .size(SIZE_SMALL)
                            .color(p.text_tertiary),
                    );
                }

                if let Some(notice) = &editor.ui.alt.notice {
                    ui.add_space(4.0);
                    ui.label(RichText::new(notice).color(p.success));
                }
                if let Some(error) = &editor.ui.alt.inline_error {
                    ui.add_space(4.0);
                    ui.label(RichText::new(error).color(p.danger));
                }

                if editor.ui.alt.running {
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.add(egui::Spinner::new());
                        let number = editor
                            .ui
                            .alt
                            .pending
                            .as_ref()
                            .map_or(1, |pending| pending.number);
                        ui.label(format!("Image {number} of {}", editor.ui.alt.total));
                        if ui.button("Cancel").clicked() {
                            cancel = true;
                        }
                    });
                } else if !editor.ui.alt.rows.is_empty() {
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        let can_suggest =
                            !editor.ui.alt.queue.is_empty() && editor.ui.alt.inline.is_none();
                        if ui
                            .add_enabled(can_suggest, Button::new("Suggest"))
                            .clicked()
                        {
                            suggest = true;
                        }
                        let any_ticked = editor
                            .ui
                            .alt
                            .rows
                            .iter()
                            .any(|row| row.ticked && !row.suggestion.trim().is_empty());
                        if ui.add_enabled(any_ticked, Button::new("Apply")).clicked() {
                            apply = true;
                        }
                    });
                }

                ui.add_space(6.0);
                ui.separator();
                ui.add_space(4.0);

                let thumbs: Vec<Option<(TextureId, Vec2)>> = editor
                    .ui
                    .alt
                    .rows
                    .iter()
                    .map(|row| thumbnail(&editor.media_cache, &row.src))
                    .collect();
                for (index, row) in editor.ui.alt.rows.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        match thumbs.get(index).copied().flatten() {
                            Some((texture, size)) => {
                                ui.add(
                                    egui::Image::new((texture, size))
                                        .fit_to_exact_size(vec2(48.0, 36.0)),
                                );
                            }
                            None => {
                                ui.add_space(48.0);
                            }
                        }
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.checkbox(&mut row.ticked, "");
                                ui.label(RichText::new(&row.location).strong());
                            });
                            ui.add_sized(
                                [
                                    ui.available_width().max(90.0),
                                    coursecraft_render::CONTROL_COMPACT,
                                ],
                                TextEdit::singleline(&mut row.suggestion)
                                    .id_salt(("alt_suggestion", index))
                                    .hint_text("A description of the image"),
                            );
                            if let Some(error) = &row.error {
                                ui.label(RichText::new(error).size(SIZE_SMALL).color(p.danger));
                            }
                        });
                    });
                    ui.add_space(4.0);
                    ui.separator();
                }
            });
            if ready {
                dialog.footer(|ui| {
                    ui.horizontal(|ui| {
                        if ui.button("Close").clicked() {
                            close_clicked = true;
                        }
                    });
                });
            }

            if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape))
                && !editor.ui.alt.running
            {
                close_clicked = true;
            }
        });

    if !open {
        close_clicked = true;
    }

    if open_settings {
        editor.ui.alt.open = false;
        editor.open_ai_settings();
        return;
    }
    if cancel {
        cancel_request(editor, requests);
        return;
    }
    if suggest {
        editor.ui.alt.running = true;
        start_next(editor);
        return;
    }
    if apply {
        apply_rows(editor);
        return;
    }
    if close_clicked {
        if editor.ui.alt.running {
            cancel_request(editor, requests);
        }
        close(editor);
    }
}

fn cancel_request(editor: &mut Editor, requests: &mut Vec<EditorRequest>) {
    if let Some(pending) = editor.ui.alt.pending.take() {
        let src = pending.group.src.clone();
        // Resume from the same image if the author presses Suggest again.
        editor.ui.alt.queue.insert(0, pending.group);
        editor.ui.alt.next_number = editor.ui.alt.next_number.saturating_sub(1).max(1);
        requests.push(EditorRequest::CancelAi(AiPurpose::AltText { src }));
    }
    editor.ui.alt.running = false;
}

fn apply_rows(editor: &mut Editor) {
    let across = editor.ui.alt.rows.clone();
    let ticked: Vec<usize> = across
        .iter()
        .enumerate()
        .filter_map(|(index, row)| row.ticked.then_some(index))
        .collect();
    if ticked.is_empty() {
        editor.ui.alt.notice = Some("Tick at least one suggestion to apply.".to_string());
        return;
    }

    let mut steps: Vec<(String, serde_json::Value)> = Vec::new();
    let mut written = 0usize;
    let mut skipped = 0usize;
    for index in &ticked {
        let Some(row) = across.get(*index) else {
            skipped = skipped.saturating_add(1);
            continue;
        };
        let suggestion = row.suggestion.trim();
        if suggestion.is_empty() {
            skipped = skipped.saturating_add(1);
            continue;
        }
        let Some(slide) = editor.session.course.slide(&row.slide) else {
            skipped = skipped.saturating_add(1);
            continue;
        };
        let Some((object, _)) = slide.object(&row.object) else {
            skipped = skipped.saturating_add(1);
            continue;
        };
        let ObjectKind::Image { src, alt, crop } = &object.kind else {
            skipped = skipped.saturating_add(1);
            continue;
        };
        if !alt.trim().is_empty() {
            skipped = skipped.saturating_add(1);
            continue;
        }
        let mut kind = serde_json::json!({
            "type": "image",
            "src": src,
            "alt": suggestion,
        });
        if let Some(crop) = crop {
            kind["crop"] = serde_json::json!({
                "x": crop.x,
                "y": crop.y,
                "w": crop.w,
                "h": crop.h,
            });
        }
        steps.push((
            "object.update".to_string(),
            serde_json::json!({
                "slide": row.slide,
                "object": row.object,
                "kind": kind,
            }),
        ));
        written = written.saturating_add(1);
    }

    if !steps.is_empty() && editor.run_batch(&steps).is_none() {
        editor.ui.alt.notice = Some("Couldn't update the images. Nothing was changed.".to_string());
        return;
    }

    // The ticked rows are done with: remove them, including the skipped ones,
    // which no longer need an answer.
    let mut remove = ticked;
    remove.sort_unstable_by(|a, b| b.cmp(a));
    for index in remove {
        if index < editor.ui.alt.rows.len() {
            editor.ui.alt.rows.remove(index);
        }
    }
    editor.ui.alt.queue.clear();
    editor.ui.alt.pending = None;
    editor.ui.alt.running = false;
    editor.ui.alt.next_number = 1;

    let plural = if written == 1 { "image" } else { "images" };
    let mut message = format!("Updated {written} {plural}.");
    if skipped > 0 {
        message.push(' ');
        message.push_str(&skipped_line(skipped));
    }
    editor.ui.alt.notice = Some(message);
}

fn skipped_line(count: usize) -> String {
    if count == 1 {
        "Skipped 1 image that changed or no longer exists.".to_string()
    } else {
        format!("Skipped {count} images that changed or no longer exist.")
    }
}

fn missing_line(count: usize) -> String {
    if count == 1 {
        "1 image has no file yet.".to_string()
    } else {
        format!("{count} images have no file yet.")
    }
}

fn consent_line(editor: &Editor, count: usize) -> String {
    let images = if count == 1 {
        "1 image".to_string()
    } else {
        format!("{count} images")
    };
    if let Some(destination) = crate::ai_settings::agent_destination(editor.ui.ai.source) {
        return format!("This sends {images} and nearby slide text to {destination}.");
    }
    let host = coursecraft_ai::endpoint_host(&editor.ui.ai.config);
    let local = coursecraft_ai::endpoint_is_local(&editor.ui.ai.config);
    match host {
        Some(host) if local => {
            format!("This sends {images} and nearby slide text to {host}, on this computer.")
        }
        Some(host) => format!("This sends {images} and nearby slide text to {host}."),
        None => "This sends images and nearby slide text to the AI endpoint.".to_string(),
    }
}

fn thumbnail(cache: &coursecraft_render::MediaCache, src: &str) -> Option<(TextureId, Vec2)> {
    let texture = cache.texture_id(src)?;
    let [width, height] = match cache.natural_size(src) {
        Some((width, height)) => [width.round() as u32, height.round() as u32],
        None => match cache.get_entry(src) {
            Some(coursecraft_render::MediaEntry::Texture(_, size)) => *size,
            _ => return None,
        },
    };
    if width == 0 || height == 0 {
        return None;
    }
    let width_f = width as f32;
    let height_f = height as f32;
    let scale = (48.0 / width_f).min(36.0 / height_f);
    if !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    Some((
        texture,
        vec2((width_f * scale).max(1.0), (height_f * scale).max(1.0)),
    ))
}

fn collect(course: &Course, media: &[(String, Arc<[u8]>)]) -> (Vec<AltRow>, Vec<AltGroup>, usize) {
    let mut collector = Collector {
        media,
        rows: Vec::new(),
        groups: Vec::new(),
        missing_files: 0,
    };
    for (index, slide) in course.slides().enumerate() {
        let number = index.saturating_add(1);
        collector.collect_slide(slide, &format!("Slide {number}"));
    }
    for master in &course.masters {
        collector.collect_slide(master, &format!("Master ▸ {}", master.title.trim()));
    }
    for bank in &course.banks {
        for slide in &bank.slides {
            collector.collect_slide(
                slide,
                &format!("Bank ▸ {} ▸ {}", bank.title.trim(), slide.title.trim()),
            );
        }
    }
    (collector.rows, collector.groups, collector.missing_files)
}

struct Collector<'a> {
    media: &'a [(String, Arc<[u8]>)],
    rows: Vec<AltRow>,
    groups: Vec<AltGroup>,
    missing_files: usize,
}

impl Collector<'_> {
    fn collect_slide(&mut self, slide: &Slide, place: &str) {
        for object in &slide.objects {
            self.collect_object(slide, None, place, object);
        }
        for layer in &slide.layers {
            for object in &layer.objects {
                self.collect_object(slide, Some(layer.name.as_str()), place, object);
            }
        }
    }

    fn collect_object(
        &mut self,
        slide: &Slide,
        layer: Option<&str>,
        place: &str,
        object: &coursecraft_course::Object,
    ) {
        if object.decorative {
            return;
        }
        let ObjectKind::Image { src, alt, .. } = &object.kind else {
            return;
        };
        if !alt.trim().is_empty() {
            return;
        }
        if !has_media(self.media, src) {
            self.missing_files = self.missing_files.saturating_add(1);
            return;
        }

        let label = object_label(object);
        let location = match layer {
            Some(layer) => format!("{place} ▸ {layer} ▸ {label}"),
            None => format!("{place} ▸ {label}"),
        };
        let index = self.rows.len();
        self.rows.push(AltRow {
            slide: slide.id.clone(),
            object: object.id.clone(),
            src: src.clone(),
            location,
            suggestion: String::new(),
            ticked: false,
            error: None,
        });
        if let Some(group) = self.groups.iter_mut().find(|group| group.src == *src) {
            group.row_indices.push(index);
        } else {
            self.groups.push(AltGroup {
                src: src.clone(),
                row_indices: vec![index],
                slide: slide.id.clone(),
            });
        }
    }
}

fn object_label(object: &coursecraft_course::Object) -> String {
    let name = object.name.trim();
    if name.is_empty() {
        "Image".to_string()
    } else {
        name.to_string()
    }
}

fn has_media(media: &[(String, Arc<[u8]>)], src: &str) -> bool {
    media.iter().any(|(name, _)| name == src)
}

fn media_bytes<'a>(media: &'a [(String, Arc<[u8]>)], src: &str) -> Option<&'a [u8]> {
    media
        .iter()
        .find(|(name, _)| name == src)
        .map(|(_, bytes)| bytes.as_ref())
}

fn build_request(
    course: &Course,
    media: &[(String, Arc<[u8]>)],
    slide_id: &str,
    src: &str,
) -> Result<ChatRequest, String> {
    let Some(bytes) = media_bytes(media, src) else {
        return Err("The image file is missing.".to_string());
    };
    let data_url = prepare_image_data_url(bytes)?;
    let context = context_for(course, slide_id);
    Ok(alt_text_request(&data_url, &context))
}

fn context_for(course: &Course, slide_id: &str) -> AltContext {
    let Some(slide) = course.slide(slide_id) else {
        return AltContext::default();
    };
    AltContext {
        slide_title: slide.title.clone(),
        nearby_text: nearby_text(slide),
    }
}

fn nearby_text(slide: &Slide) -> String {
    let mut parts: Vec<String> = Vec::new();
    for object in slide
        .objects
        .iter()
        .chain(slide.layers.iter().flat_map(|layer| layer.objects.iter()))
    {
        match &object.kind {
            ObjectKind::Text { text, .. } if !text.trim().is_empty() => {
                parts.push(text.clone());
            }
            ObjectKind::Shape {
                text: Some(text), ..
            } if !text.text.trim().is_empty() => {
                parts.push(text.text.clone());
            }
            ObjectKind::Button { label, .. } if !label.trim().is_empty() => {
                parts.push(label.clone());
            }
            _ => {}
        }
    }
    if let Some(question) = &slide.question
        && !question.prompt.trim().is_empty()
    {
        parts.push(question.prompt.clone());
    }
    parts.join("\n").chars().take(MAX_NEARBY_CHARS).collect()
}

/// Turn stored image bytes into a data URL, downscaling the longest side to
/// [`MAX_IMAGE_SIDE`]. Oversized and unsupported files return a row error.
pub fn prepare_image_data_url(bytes: &[u8]) -> Result<String, String> {
    let (media_type, prepared) = prepare_image_bytes(bytes)?;
    Ok(image_data_url(&media_type, &prepared))
}

/// The media type and bytes actually sent: the original when it is small
/// enough, otherwise a PNG with the longest side at [`MAX_IMAGE_SIDE`].
pub(crate) fn prepare_image_bytes(bytes: &[u8]) -> Result<(String, Vec<u8>), String> {
    if bytes.len() > MAX_SEND_BYTES {
        return Err("Too large to send.".to_string());
    }
    let format = coursecraft_media::image::guess_format(bytes)
        .map_err(|_| "This format can't be sent.".to_string())?;
    let media_type = match format {
        ImageFormat::Png => "image/png",
        ImageFormat::Jpeg => "image/jpeg",
        ImageFormat::Gif => "image/gif",
        ImageFormat::WebP => "image/webp",
        _ => return Err("This format can't be sent.".to_string()),
    };

    let decoded = decode_image(bytes, &Limits::default())
        .map_err(|_| "The image couldn't be read.".to_string())?;
    let longest = decoded.width.max(decoded.height);
    if longest <= MAX_IMAGE_SIDE {
        return Ok((media_type.to_string(), bytes.to_vec()));
    }

    let (width, height) = if decoded.width >= decoded.height {
        (
            MAX_IMAGE_SIDE,
            scaled_side(decoded.height, longest, MAX_IMAGE_SIDE),
        )
    } else {
        (
            scaled_side(decoded.width, longest, MAX_IMAGE_SIDE),
            MAX_IMAGE_SIDE,
        )
    };
    let source = RgbaImage::from_raw(decoded.width, decoded.height, decoded.pixels)
        .ok_or_else(|| "The image couldn't be read.".to_string())?;
    let resized =
        coursecraft_media::image::imageops::resize(&source, width, height, FilterType::Triangle);
    let png = encode_png_rgba(width, height, resized.as_raw())
        .map_err(|_| "The image couldn't be prepared.".to_string())?;
    Ok(("image/png".to_string(), png))
}

fn scaled_side(value: u32, longest: u32, target: u32) -> u32 {
    if longest == 0 {
        return 1;
    }
    let scaled = u64::from(value).saturating_mul(u64::from(target)) / u64::from(longest);
    u32::try_from(scaled.max(1)).unwrap_or(1)
}
