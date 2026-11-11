//! Media Library window: every file in the project's media store in one place.
//!
//! The list lives outside `course.json`, so Import, Replace and Delete change
//! only the editor's media store. They are not undoable; the window says so.

use coursecraft_course::{MediaReference, media_references};
use coursecraft_render::{Icon, MediaView, RADIUS_SM, SIZE_SMALL, icon, palette};
use egui::{
    Align, Align2, Button, Color32, ComboBox, Context, FontFamily, FontId, Grid, Label, Layout,
    Rect, RichText, ScrollArea, Sense, TextEdit, Ui, pos2, vec2,
};

use crate::{Editor, EditorRequest, MemoryMediaSource};

/// The kind filter in the Media Library.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MediaKindFilter {
    #[default]
    All,
    Images,
    Audio,
    Video,
    Lottie,
    Fonts,
    Captions,
    Web,
    Documents,
    Other,
}

impl MediaKindFilter {
    /// Every filter, in combo order.
    pub const ALL: &'static [Self] = &[
        Self::All,
        Self::Images,
        Self::Audio,
        Self::Video,
        Self::Lottie,
        Self::Fonts,
        Self::Captions,
        Self::Web,
        Self::Documents,
        Self::Other,
    ];

    /// The combo label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Images => "Images",
            Self::Audio => "Audio",
            Self::Video => "Video",
            Self::Lottie => "Lottie",
            Self::Fonts => "Fonts",
            Self::Captions => "Captions",
            Self::Web => "Web",
            Self::Documents => "Documents",
            Self::Other => "Other",
        }
    }

    fn icon(self) -> Icon {
        match self {
            Self::All | Self::Other | Self::Documents => Icon::Article,
            Self::Images => Icon::Image,
            Self::Audio => Icon::Headphones,
            Self::Video => Icon::VideoCamera,
            Self::Lottie => Icon::Sparkle,
            Self::Fonts => Icon::TextAa,
            Self::Captions => Icon::Subtitles,
            Self::Web => Icon::Link,
        }
    }
}

/// A pending Delete command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DeleteTarget {
    Selected(String),
    Unused,
}

/// Media Library filter and selection state, held in `UiState`.
#[derive(Debug, Clone, Default)]
pub struct MediaLibraryState {
    pub filter: String,
    pub kind: MediaKindFilter,
    pub selected: Option<String>,
    pub(crate) confirm: Option<DeleteTarget>,
}

enum Action {
    Import,
    Insert(String),
    Replace(String),
    DeleteSelected(String),
    DeleteUnused,
}

/// Draw the Media Library window when it is open.
pub fn show_media_library(editor: &mut Editor, ctx: &Context) {
    if !editor.ui.show_media_library {
        return;
    }
    let mut open = true;
    let mut action = None;
    egui::Window::new("Media Library")
        .id(egui::Id::unique("media_library_window"))
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_size(vec2(720.0, 480.0))
        .show(ctx, |ui| {
            action = window_body(editor, ui);
        });
    editor.ui.show_media_library = open;
    if !open {
        editor.ui.media_library.confirm = None;
        return;
    }
    apply(editor, action);
    show_confirm(editor, ctx);
}

fn window_body(editor: &mut Editor, ui: &mut Ui) -> Option<Action> {
    let p = palette(ui.ctx());
    let references = media_references(&editor.session.course);
    let mut action = None;

    ui.horizontal(|ui| {
        let import = ui.add_enabled(editor.file_ops_available, Button::new("Import…"));
        let import = if editor.file_ops_available {
            import
        } else {
            import.on_disabled_hover_text("Available in the desktop app")
        };
        if import.clicked() {
            action = Some(Action::Import);
        }

        let selected = editor.ui.media_library.selected.clone();
        let insertable = selected
            .as_deref()
            .is_some_and(|src| insert_kind(kind_for(src)));
        let has_slide = !editor.ui.master_view
            && editor
                .session
                .course
                .slide(&editor.selection.slide)
                .is_some();
        let insert = ui.add_enabled(insertable && has_slide, Button::new("Insert on slide"));
        let insert = if editor.ui.master_view {
            insert.on_disabled_hover_text("Not available in Master View")
        } else if !insertable {
            insert.on_disabled_hover_text("Select an image, audio, video or Lottie file")
        } else {
            insert
        };
        if insert.clicked()
            && let Some(src) = selected.clone()
        {
            action = Some(Action::Insert(src));
        }

        let replace = ui.add_enabled(
            selected.is_some() && editor.file_ops_available,
            Button::new("Replace file…"),
        );
        let replace = if editor.file_ops_available {
            replace
        } else {
            replace.on_disabled_hover_text("Available in the desktop app")
        };
        if replace.clicked()
            && let Some(src) = selected.clone()
        {
            action = Some(Action::Replace(src));
        }

        let selected_unused = selected.as_deref().is_some_and(|src| {
            editor.media.iter().any(|(name, _)| name == src) && used_count(&references, src) == 0
        });
        let delete = ui.add_enabled(selected_unused, Button::new("Delete"));
        let delete = if selected.is_some() && !selected_unused {
            delete.on_disabled_hover_text("Delete is only available for files nothing uses")
        } else {
            delete
        };
        if delete.clicked()
            && let Some(src) = selected.clone()
        {
            action = Some(Action::DeleteSelected(src));
        }

        let has_unused = editor
            .media
            .iter()
            .any(|(src, _)| used_count(&references, src) == 0);
        if ui
            .add_enabled(has_unused, Button::new("Delete unused"))
            .clicked()
        {
            action = Some(Action::DeleteUnused);
        }
    });

    ui.horizontal(|ui| {
        ui.label("Filter");
        ui.add(
            TextEdit::singleline(&mut editor.ui.media_library.filter)
                .hint_text("File name")
                .desired_width(240.0),
        );
        ui.label("Kind");
        ComboBox::from_id_salt("media_library_kind")
            .selected_text(editor.ui.media_library.kind.label())
            .show_ui(ui, |ui| {
                for kind in MediaKindFilter::ALL {
                    ui.selectable_value(&mut editor.ui.media_library.kind, *kind, kind.label());
                }
            });
    });

    let filter = editor.ui.media_library.filter.trim().to_ascii_lowercase();
    let kind_filter = editor.ui.media_library.kind;
    let mut selected = editor.ui.media_library.selected.clone();
    let rows: Vec<(String, usize, MediaKindFilter)> = editor
        .media
        .iter()
        .map(|(src, bytes)| (src.clone(), bytes.len(), kind_for(src)))
        .collect();

    ui.add_space(4.0);
    ScrollArea::vertical()
        .id_salt("media_library_table")
        .auto_shrink([false, false])
        .max_height(260.0)
        .show(ui, |ui| {
            Grid::new("media_library_rows")
                .num_columns(5)
                .striped(true)
                .spacing([12.0, 6.0])
                .show(ui, |ui| {
                    ui.label("");
                    ui.strong("Name");
                    ui.strong("Kind");
                    ui.strong("Size");
                    ui.strong("Used by");
                    ui.end_row();
                    for (src, size, kind) in &rows {
                        if kind_filter != MediaKindFilter::All && *kind != kind_filter {
                            continue;
                        }
                        if !filter.is_empty() && !src.to_ascii_lowercase().contains(&filter) {
                            continue;
                        }
                        let mut row_clicked = paint_thumbnail(editor, ui, src, *kind).clicked();
                        row_clicked |= ui
                            .selectable_label(
                                selected.as_deref() == Some(src.as_str()),
                                src.as_str(),
                            )
                            .clicked();
                        row_clicked |= ui
                            .add(Label::new(kind.label()).sense(Sense::click()))
                            .clicked();
                        row_clicked |= ui
                            .add(Label::new(format_size(*size)).sense(Sense::click()))
                            .clicked();
                        row_clicked |= ui
                            .add(
                                Label::new(format!("Used by {}", used_count(&references, src)))
                                    .sense(Sense::click()),
                            )
                            .clicked();
                        if row_clicked {
                            editor.ui.media_library.selected = Some(src.clone());
                            selected = Some(src.clone());
                        }
                        ui.end_row();
                    }
                });
        });

    ui.add_space(4.0);
    if let Some(src) = editor.ui.media_library.selected.clone() {
        let places: Vec<MediaReference> = references
            .iter()
            .filter(|reference| reference_matches(&reference.src, &src))
            .cloned()
            .collect();
        if places.is_empty() {
            ui.label(RichText::new("Nothing uses this file yet.").color(p.text_secondary));
        } else {
            ui.label(RichText::new("Places using this file").strong());
            ScrollArea::vertical()
                .id_salt("media_library_places")
                .max_height(120.0)
                .show(ui, |ui| {
                    for reference in places {
                        if ui.button(&reference.place).clicked() {
                            go_to_place(editor, &reference);
                        }
                    }
                });
        }
    }

    ui.separator();
    let total: u64 = editor
        .media
        .iter()
        .map(|(_, bytes)| u64::try_from(bytes.len()).unwrap_or(u64::MAX))
        .fold(0u64, u64::saturating_add);
    ui.horizontal(|ui| {
        ui.label(format!(
            "{} {}",
            editor.media.len(),
            plural(editor.media.len(), "file", "files")
        ));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(format!("Total {}", format_size_usize(total)));
        });
    });
    ui.label(
        RichText::new("File operations are not undoable.")
            .size(SIZE_SMALL)
            .color(p.text_tertiary),
    );

    action
}

fn apply(editor: &mut Editor, action: Option<Action>) {
    let Some(action) = action else {
        return;
    };
    match action {
        Action::Import => editor.request(EditorRequest::PickMediaFiles),
        Action::Insert(src) => insert_on_slide(editor, &src),
        Action::Replace(src) => editor.request(EditorRequest::ReplaceMediaFile { src }),
        Action::DeleteSelected(src) => {
            let references = media_references(&editor.session.course);
            if editor.media.iter().any(|(name, _)| name == &src)
                && used_count(&references, &src) == 0
            {
                editor.ui.media_library.confirm = Some(DeleteTarget::Selected(src));
            } else {
                editor.set_status("That file is used; remove its references first.");
            }
        }
        Action::DeleteUnused => {
            if unused_srcs(editor).is_empty() {
                editor.set_status("Every file is used.");
            } else {
                editor.ui.media_library.confirm = Some(DeleteTarget::Unused);
            }
        }
    }
}

fn show_confirm(editor: &mut Editor, ctx: &Context) {
    let Some(target) = editor.ui.media_library.confirm.clone() else {
        return;
    };
    let to_delete = match &target {
        DeleteTarget::Selected(src) => {
            let references = media_references(&editor.session.course);
            if editor.media.iter().any(|(name, _)| name == src) && used_count(&references, src) == 0
            {
                vec![src.clone()]
            } else {
                Vec::new()
            }
        }
        DeleteTarget::Unused => unused_srcs(editor),
    };
    if to_delete.is_empty() {
        editor.ui.media_library.confirm = None;
        return;
    }
    let count = to_delete.len();
    let mut open = true;
    let mut confirmed = false;
    let mut cancel = false;
    egui::Window::new("Delete files")
        .id(egui::Id::unique("media_library_delete_confirm"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .min_width(340.0)
        .default_width(340.0)
        .show(ctx, |ui| {
            ui.label(format!(
                "Delete {count} {}? This cannot be undone.",
                plural(count, "file", "files")
            ));
            ui.horizontal(|ui| {
                if ui.button("Delete").clicked() {
                    confirmed = true;
                }
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
            });
        });
    if confirmed {
        editor
            .media
            .retain(|(src, _)| !to_delete.iter().any(|deleted| deleted == src));
        editor.media_cache = coursecraft_render::MediaCache::new();
        if editor
            .ui
            .media_library
            .selected
            .as_ref()
            .is_some_and(|selected| to_delete.contains(selected))
        {
            editor.ui.media_library.selected = None;
        }
        editor.mark_media_changed();
        editor.set_status(format!(
            "Deleted {}. File operations are not undoable.",
            plural(count, "file", "files")
        ));
        editor.ui.media_library.confirm = None;
    } else if cancel || !open {
        editor.ui.media_library.confirm = None;
    }
}

fn insert_on_slide(editor: &mut Editor, src: &str) {
    if editor.ui.master_view {
        return;
    }
    let slide_id = editor.selection.slide.clone();
    if slide_id.is_empty() || editor.session.course.slide(&slide_id).is_none() {
        return;
    }
    let kind = kind_for(src);
    let (kind_json, (mut width, mut height)) = match kind {
        MediaKindFilter::Images => {
            let (width, height) = editor.natural_image_size(src).unwrap_or((320.0, 180.0));
            (
                serde_json::json!({ "type": "image", "src": src, "alt": "" }),
                (width, height),
            )
        }
        MediaKindFilter::Audio => (
            serde_json::json!({ "type": "audio", "src": src, "autoplay": false }),
            (48.0, 48.0),
        ),
        MediaKindFilter::Video => (
            serde_json::json!({ "type": "video", "src": src, "autoplay": false }),
            (640.0, 360.0),
        ),
        MediaKindFilter::Lottie => {
            let size = editor
                .media
                .iter()
                .find(|(name, _)| name == src)
                .and_then(|(_, bytes)| coursecraft_render::parse_info(bytes).ok())
                .map_or((320.0, 180.0), |info| {
                    (info.width as f32, info.height as f32)
                });
            (serde_json::json!({ "type": "lottie", "src": src }), size)
        }
        _ => return,
    };
    let slide_size = editor.session.course.slide_size;
    let slide_w = slide_size.w.max(1) as f32;
    let slide_h = slide_size.h.max(1) as f32;
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        width = 320.0;
        height = 180.0;
    }
    if (width > slide_w || height > slide_h) && slide_w > 0.0 && slide_h > 0.0 {
        let scale = (slide_w / width).min(slide_h / height);
        width = (width * scale).round().max(1.0);
        height = (height * scale).round().max(1.0);
    }
    let x = ((slide_w - width) * 0.5).max(0.0).round();
    let y = ((slide_h - height) * 0.5).max(0.0).round();
    let mut params = serde_json::json!({
        "slide": slide_id,
        "kind": kind_json,
        "frame": {
            "x": x,
            "y": y,
            "w": width.round().max(1.0),
            "h": height.round().max(1.0),
        },
    });
    if let Some(layer) = &editor.selection.layer {
        params["layer"] = serde_json::json!(layer);
    }
    let result = editor.run("object.add", params);
    if let Some(id) = result
        .as_ref()
        .and_then(|value| value.get("id"))
        .and_then(serde_json::Value::as_str)
    {
        editor.selection.set_many([id.to_owned()]);
    }
    editor.set_status(format!("Inserted {src}. File operations are not undoable."));
}

fn go_to_place(editor: &mut Editor, reference: &MediaReference) {
    let Some(slide) = reference.slide.as_deref() else {
        return;
    };
    if editor.session.course.master(slide).is_some() {
        editor.enter_master_view();
    } else {
        editor.leave_master_view();
    }
    crate::status::navigate_to_audit_finding(
        &editor.session.course,
        &mut editor.selection,
        Some(slide),
        None,
    );
}

fn paint_thumbnail(
    editor: &mut Editor,
    ui: &mut Ui,
    src: &str,
    kind: MediaKindFilter,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(40.0, 40.0), Sense::click());
    let p = palette(ui.ctx());
    ui.painter()
        .rect_filled(rect, egui::CornerRadius::same(RADIUS_SM as u8), p.hover);
    if kind == MediaKindFilter::Images {
        let source = MemoryMediaSource {
            media: &editor.media,
        };
        if let MediaView::Ready { texture, .. } =
            editor
                .media_cache
                .texture_for(ui.ctx(), src, &source, (40, 40))
        {
            ui.painter().image(
                texture.id(),
                rect,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
            return response;
        }
    }
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        icon(kind.icon(), 22.0).text().to_owned(),
        FontId::new(22.0, FontFamily::Name("phosphor".into())),
        p.text_secondary,
    );
    response
}

fn kind_for(src: &str) -> MediaKindFilter {
    let extension = src
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .unwrap_or_default();
    match extension.as_str() {
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "svg" | "svgz" => MediaKindFilter::Images,
        "mp3" | "wav" | "ogg" | "m4a" | "aac" | "flac" => MediaKindFilter::Audio,
        "mp4" | "webm" | "mov" | "m4v" => MediaKindFilter::Video,
        "json" | "lottie" => MediaKindFilter::Lottie,
        "ttf" | "otf" | "woff" | "woff2" => MediaKindFilter::Fonts,
        "vtt" => MediaKindFilter::Captions,
        "html" | "htm" | "css" | "js" | "mjs" => MediaKindFilter::Web,
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "txt" | "rtf" | "odt"
        | "csv" | "zip" => MediaKindFilter::Documents,
        _ => MediaKindFilter::Other,
    }
}

fn insert_kind(kind: MediaKindFilter) -> bool {
    matches!(
        kind,
        MediaKindFilter::Images
            | MediaKindFilter::Audio
            | MediaKindFilter::Video
            | MediaKindFilter::Lottie
    )
}

fn reference_matches(reference: &str, src: &str) -> bool {
    reference == src || (reference.ends_with('/') && src.starts_with(reference))
}

fn used_count(references: &[MediaReference], src: &str) -> usize {
    references
        .iter()
        .filter(|reference| reference_matches(&reference.src, src))
        .count()
}

fn unused_srcs(editor: &Editor) -> Vec<String> {
    let references = media_references(&editor.session.course);
    editor
        .media
        .iter()
        .filter(|(src, _)| used_count(&references, src) == 0)
        .map(|(src, _)| src.clone())
        .collect()
}

fn format_size(bytes: usize) -> String {
    format_size_usize(u64::try_from(bytes).unwrap_or(u64::MAX))
}

fn format_size_usize(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    let bytes = bytes as f64;
    if bytes < MB {
        format!("{:.1} KB", bytes / KB)
    } else {
        format!("{:.1} MB", bytes / MB)
    }
}

fn plural<'a>(count: usize, one: &'a str, many: &'a str) -> &'a str {
    if count == 1 { one } else { many }
}
