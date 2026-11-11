//! Properties side panel: segmented inspector (Format, Animate, Slide, Course).

use crate::Editor;
use coursecraft_course::{
    Align, AnimEffect, Autofit, BUILTIN_STATES, Color, Course, Direction, Easing, Glow, ListKind,
    ObjectKind, Paragraph, RevisitMode, Shadow, ShapeKind, TextRun, TextStyle, TimerFormat, Timing,
    TransitionEffect, Trim, VAlign, paragraphs_for,
};
use coursecraft_render::parse_color;
use egui::{
    Color32, CornerRadius, DragValue, FontId, Rect, Response, RichText, Sense, Stroke, StrokeKind,
    TextEdit, Ui, vec2,
};
use serde_json::Value as Json;

pub const PROPERTIES_PANEL_WIDTH: f32 = 300.0;
pub const PROPERTIES_PANEL_MIN_WIDTH: f32 = 260.0;
pub const PROPERTIES_PANEL_MAX_WIDTH: f32 = 400.0;
pub const LABEL_COL_WIDTH: f32 = 92.0;

/// Tab in the segmented inspector panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InspectorTab {
    #[default]
    Format,
    Animate,
    Slide,
    Course,
}

impl InspectorTab {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Format => "Format",
            Self::Animate => "Animate",
            Self::Slide => "Slide",
            Self::Course => "Course",
        }
    }
}

/// Local edit buffers for the properties panel.
///
/// Buffers are reset whenever the selection or `session.revision` changes,
/// ensuring that undo/redo immediately updates the fields.
/// One cue's inspector edit buffers.
#[derive(Debug, Clone, PartialEq)]
pub struct CueEdit {
    pub name: String,
    pub at_ms: f64,
    pub pause: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PropertiesState {
    pub last_synced_revision: u64,
    pub last_synced_selection: crate::Selection,

    // Tracked rects for test interaction
    pub frame_x_rect: Option<Rect>,
    pub btn_label_rect: Option<Rect>,
    pub color_rect: Option<Rect>,
    pub slide_title_rect: Option<Rect>,

    // Object fields
    pub name: String,
    pub frame_x: f32,
    pub frame_y: f32,
    pub frame_w: f32,
    pub frame_h: f32,
    pub rotation: f32,
    pub rotation_rect: Option<Rect>,
    pub opacity: f32,
    pub opacity_rect: Option<Rect>,
    /// Buffered fill style for a shape or button. `None` is a solid fill.
    pub fill_style: Option<coursecraft_course::FillStyle>,
    pub fill_add_stop_rect: Option<Rect>,
    pub shadow_enabled: bool,
    pub shadow: coursecraft_course::Shadow,
    pub glow_enabled: bool,
    pub glow: coursecraft_course::Glow,
    pub drag_enabled: bool,
    pub drag: coursecraft_course::DragSpec,
    pub drag_toggle_rect: Option<Rect>,
    pub start_ms: u32,
    pub until_end: bool,
    pub end_ms: u32,
    pub initial_state: String,

    // Text buffers
    pub text_body: String,
    pub text_size: f32,
    pub text_color: String,
    pub text_bold: bool,
    pub text_italic: bool,
    pub text_underline: bool,
    pub text_align: Align,
    pub text_valign: VAlign,
    /// Link dialog state, shared by text and shape text.
    pub link_dialog_open: bool,
    pub link_url: String,
    pub link_slide: String,
    pub link_use_slide: bool,
    /// Last custom line-spacing value entered.
    pub line_spacing_custom: f32,
    /// The font picker's "Add font…" entry was clicked.
    pub add_font_requested: bool,

    // Button buffers
    pub button_label: String,
    pub button_fill: String,

    // Shape buffers
    pub shape_kind: ShapeKind,
    pub shape_fill: String,
    pub shape_outline_enabled: bool,
    pub shape_outline_color: String,
    pub shape_outline_width: f32,
    pub shape_outline_dash: coursecraft_course::DashStyle,
    /// Search text in the shape gallery popup.
    pub shape_gallery_query: String,
    /// Icon rects painted by the last open gallery (test hook).
    pub shape_gallery_rects: Vec<(ShapeKind, Rect)>,
    /// Search field rect painted by the last open gallery (test hook).
    pub shape_gallery_search_rect: Option<Rect>,

    // Path buffers
    pub path_outline_color: String,
    pub path_outline_width: f32,
    pub path_outline_dash: coursecraft_course::DashStyle,
    pub path_start_end: coursecraft_course::LineEnd,
    pub path_end_end: coursecraft_course::LineEnd,
    pub path_style: coursecraft_course::PathStyle,
    pub path_closed: bool,
    pub path_fill: String,
    pub path_connector: Option<coursecraft_course::ConnectorStyle>,

    // Image buffers
    pub image_src: String,
    pub image_alt: String,
    /// A `Suggest` click waiting to be sent after the panel finishes drawing.
    pub alt_suggest_request: Option<(String, String)>,

    // Media buffers
    pub media_src: String,
    pub media_autoplay: bool,
    pub media_captions: String,
    /// Volume shown in the inspector, in percent (0–100).
    pub media_volume: f32,
    /// Trim start text, as `m:ss.t`.
    pub media_trim_start: String,
    /// Trim end text, as `m:ss.t`; empty means to the end of the file.
    pub media_trim_end: String,
    pub media_volume_rect: Option<Rect>,
    pub media_trim_start_rect: Option<Rect>,
    pub media_trim_end_rect: Option<Rect>,
    /// Video cue edit buffers keyed by cue id.
    pub cue_edits: std::collections::BTreeMap<String, CueEdit>,
    /// Cue row rects for headless tests.
    pub cue_row_rects: Vec<(String, Rect)>,

    // Lottie buffers
    pub lottie_loop: bool,
    pub lottie_autoplay: bool,
    pub lottie_speed: f32,

    // Web object buffers
    pub web_is_url: bool,
    pub web_url: String,
    pub web_path: String,
    pub web_entry: String,

    // Input object buffers
    pub input_variable: String,
    pub input_placeholder: String,
    pub input_multiline: bool,
    pub input_max_chars: u32,
    pub input_min: f64,
    pub input_max: f64,
    pub input_step: f64,
    pub input_orientation: coursecraft_course::Orientation,
    pub input_start_angle: f32,
    pub input_end_angle: f32,
    pub input_label: String,
    pub input_options: Vec<coursecraft_course::RadioOption>,
    /// Inspector rects for headless tests.
    pub input_variable_rect: Option<Rect>,
    pub input_option_rects: Vec<Rect>,

    // Timer and progress meter buffers
    pub timer_duration_ms: u32,
    pub timer_format: TimerFormat,
    pub timer_autostart: bool,
    pub timer_font: String,
    pub timer_duration_rect: Option<Rect>,
    pub meter_style: coursecraft_course::MeterStyle,
    pub meter_show_value: bool,

    // Marker and scrolling panel buffers
    pub marker_title: String,
    pub marker_body: String,
    pub marker_icon: coursecraft_course::MarkerIcon,
    pub marker_color: String,
    pub marker_use_theme_color: bool,
    pub marker_title_rect: Option<Rect>,
    pub marker_icon_rects: Vec<Rect>,
    pub scroll_content_height: f32,
    pub scroll_bar_visible: bool,
    pub scroll_content_height_rect: Option<Rect>,
    /// The scrolling panel this object belongs to; empty means none.
    pub panel_target: String,
    pub panel_target_rect: Option<Rect>,

    // Table buffers
    pub table_outer_enabled: bool,
    pub table_outer_color: String,
    pub table_outer_width: f32,
    pub table_inner_enabled: bool,
    pub table_inner_color: String,
    pub table_inner_width: f32,
    /// The cell whose fill and text the inspector edits.
    pub table_cell: Option<(usize, usize)>,
    pub table_cell_fill: String,

    // Accessibility buffers
    pub description: String,
    pub decorative: bool,
    pub focus_order: Option<u32>,

    // Multi-selection buffers
    pub multi_count: usize,
    pub multi_x_rect: Option<Rect>,
    pub multi_x: String,
    pub multi_y: String,
    pub multi_w: String,
    pub multi_h: String,
    pub multi_x_mixed: bool,
    pub multi_y_mixed: bool,
    pub multi_w_mixed: bool,
    pub multi_h_mixed: bool,
    pub multi_text: bool,
    pub multi_text_size: String,
    pub multi_text_size_mixed: bool,
    pub multi_text_color: String,
    pub multi_text_color_mixed: bool,
    pub multi_bold: Option<bool>,
    pub multi_italic: Option<bool>,
    pub multi_underline: Option<bool>,
    pub multi_align: Option<Align>,
    pub multi_fill_allowed: bool,
    pub multi_fill: String,
    pub multi_fill_mixed: bool,
    pub multi_decorative: Option<bool>,

    // Course buffers
    pub course_title: String,

    // Slide buffers (no selection)
    pub slide_title: String,
    pub slide_duration_ms: u32,
    pub slide_background: String,
    pub slide_notes: String,
    pub slide_revisit: RevisitMode,
    /// Draw count field rect for headless tests.
    pub draw_count_rect: Option<Rect>,

    // Animation buffers
    pub entrance_effect: String,
    pub entrance_direction: Direction,
    pub entrance_duration_ms: u32,
    pub entrance_easing: Easing,

    pub exit_effect: String,
    pub exit_direction: Direction,
    pub exit_duration_ms: u32,
    pub exit_easing: Easing,

    // Slide transition buffers
    pub slide_transition_effect: String,
    pub slide_transition_direction: Direction,
    pub slide_transition_duration_ms: u32,
}

impl Default for PropertiesState {
    fn default() -> Self {
        Self {
            last_synced_revision: u64::MAX,
            last_synced_selection: crate::Selection::default(),
            frame_x_rect: None,
            btn_label_rect: None,
            color_rect: None,
            slide_title_rect: None,
            name: String::new(),
            frame_x: 0.0,
            frame_y: 0.0,
            frame_w: 100.0,
            frame_h: 100.0,
            rotation: 0.0,
            rotation_rect: None,
            opacity: 1.0,
            opacity_rect: None,
            fill_style: None,
            fill_add_stop_rect: None,
            shadow_enabled: false,
            shadow: Shadow {
                color: Color("#00000080".into()),
                offset_x: 4.0,
                offset_y: 4.0,
                blur: 8.0,
            },
            glow_enabled: false,
            glow: Glow {
                color: Color("#2F6FEB80".into()),
                size: 8.0,
            },
            drag_enabled: false,
            drag: coursecraft_course::DragSpec::default(),
            drag_toggle_rect: None,
            start_ms: 0,
            until_end: true,
            end_ms: 5000,
            initial_state: "normal".into(),
            text_body: String::new(),
            text_size: 20.0,
            text_color: "#1F1F1F".into(),
            text_bold: false,
            text_italic: false,
            text_underline: false,
            text_align: Align::Left,
            text_valign: VAlign::Top,
            link_dialog_open: false,
            link_url: String::new(),
            link_slide: String::new(),
            link_use_slide: false,
            line_spacing_custom: 1.0,
            add_font_requested: false,
            button_label: String::new(),
            button_fill: "#2F6FEB".into(),
            shape_kind: ShapeKind::Rectangle,
            shape_fill: "#D0D7DE".into(),
            shape_outline_enabled: false,
            shape_outline_color: "#1F1F1F".into(),
            shape_outline_width: 1.5,
            shape_outline_dash: coursecraft_course::DashStyle::Solid,
            shape_gallery_query: String::new(),
            shape_gallery_rects: Vec::new(),
            shape_gallery_search_rect: None,
            path_outline_color: "#1F1F1F".into(),
            path_outline_width: 2.0,
            path_outline_dash: coursecraft_course::DashStyle::Solid,
            path_start_end: coursecraft_course::LineEnd::None,
            path_end_end: coursecraft_course::LineEnd::None,
            path_style: coursecraft_course::PathStyle::Straight,
            path_closed: false,
            path_fill: "#2F6FEB".into(),
            path_connector: None,
            image_src: String::new(),
            image_alt: String::new(),
            alt_suggest_request: None,
            media_src: String::new(),
            web_is_url: true,
            web_url: String::new(),
            web_path: String::new(),
            web_entry: coursecraft_course::WEB_DEFAULT_ENTRY.to_owned(),
            timer_duration_ms: coursecraft_course::DEFAULT_TIMER_DURATION_MS,
            timer_format: TimerFormat::MmSs,
            timer_autostart: true,
            timer_font: "Inter".to_owned(),
            timer_duration_rect: None,
            meter_style: coursecraft_course::MeterStyle::Bar,
            meter_show_value: false,
            input_variable: String::new(),
            input_placeholder: String::new(),
            input_multiline: false,
            input_max_chars: 0,
            input_min: 0.0,
            input_max: 100.0,
            input_step: 0.0,
            input_orientation: coursecraft_course::Orientation::Horizontal,
            input_start_angle: -135.0,
            input_end_angle: 135.0,
            input_label: String::new(),
            input_options: Vec::new(),
            input_variable_rect: None,
            input_option_rects: Vec::new(),
            marker_title: String::new(),
            marker_body: String::new(),
            marker_icon: coursecraft_course::MarkerIcon::Info,
            marker_color: "#2F6FEB".to_owned(),
            marker_use_theme_color: true,
            marker_title_rect: None,
            marker_icon_rects: Vec::new(),
            scroll_content_height: 0.0,
            scroll_bar_visible: true,
            scroll_content_height_rect: None,
            panel_target: String::new(),
            panel_target_rect: None,
            media_autoplay: false,
            media_captions: String::new(),
            media_volume: 100.0,
            media_trim_start: format_clip_time(0),
            media_trim_end: String::new(),
            media_volume_rect: None,
            media_trim_start_rect: None,
            media_trim_end_rect: None,
            cue_edits: std::collections::BTreeMap::new(),
            cue_row_rects: Vec::new(),
            lottie_loop: true,
            lottie_autoplay: true,
            lottie_speed: 1.0,
            table_outer_enabled: true,
            table_outer_color: "#1F1F1F".into(),
            table_outer_width: 1.0,
            table_inner_enabled: true,
            table_inner_color: "#1F1F1F".into(),
            table_inner_width: 1.0,
            table_cell: None,
            table_cell_fill: "#FFFFFF".into(),
            description: String::new(),
            decorative: false,
            focus_order: None,
            multi_count: 0,
            multi_x_rect: None,
            multi_x: String::new(),
            multi_y: String::new(),
            multi_w: String::new(),
            multi_h: String::new(),
            multi_x_mixed: false,
            multi_y_mixed: false,
            multi_w_mixed: false,
            multi_h_mixed: false,
            multi_text: false,
            multi_text_size: String::new(),
            multi_text_size_mixed: false,
            multi_text_color: String::new(),
            multi_text_color_mixed: false,
            multi_bold: None,
            multi_italic: None,
            multi_underline: None,
            multi_align: None,
            multi_fill_allowed: false,
            multi_fill: String::new(),
            multi_fill_mixed: false,
            multi_decorative: None,
            course_title: String::new(),
            slide_title: String::new(),
            slide_duration_ms: 5000,
            slide_background: "#FFFFFF".into(),
            slide_notes: String::new(),
            slide_revisit: RevisitMode::Auto,
            draw_count_rect: None,
            entrance_effect: "none".into(),
            entrance_direction: Direction::Bottom,
            entrance_duration_ms: 500,
            entrance_easing: Easing::EaseOut,
            exit_effect: "none".into(),
            exit_direction: Direction::Bottom,
            exit_duration_ms: 500,
            exit_easing: Easing::EaseOut,
            slide_transition_effect: "none".into(),
            slide_transition_direction: Direction::Bottom,
            slide_transition_duration_ms: 500,
        }
    }
}

impl PropertiesState {
    /// Re-sync edit buffers with current course data if selection or revision changed.
    pub fn sync(&mut self, course: &Course, selection: &crate::Selection, revision: u64) {
        if self.last_synced_revision == revision && &self.last_synced_selection == selection {
            return;
        }
        self.last_synced_revision = revision;
        self.last_synced_selection = selection.clone();

        self.course_title = course.title.clone();

        let Some(slide) = course.slide(&selection.slide) else {
            return;
        };

        self.slide_title = slide.title.clone();
        self.slide_duration_ms = slide.duration_ms;
        self.slide_background = course.slide_background(slide).0.clone();
        self.slide_notes = slide.notes.clone();
        self.slide_revisit = slide.revisit;

        if let Some(ref t) = slide.transition {
            self.slide_transition_effect = match t.effect {
                TransitionEffect::Fade => "fade".to_owned(),
                TransitionEffect::Push { .. } => "push".to_owned(),
                TransitionEffect::Cover { .. } => "cover".to_owned(),
            };
            self.slide_transition_direction = match t.effect {
                TransitionEffect::Push { from } | TransitionEffect::Cover { from } => from,
                _ => Direction::Bottom,
            };
            self.slide_transition_duration_ms = t.duration_ms;
        } else {
            self.slide_transition_effect = "none".to_owned();
            self.slide_transition_direction = Direction::Bottom;
            self.slide_transition_duration_ms = 500;
        }

        if selection.is_multi() {
            self.sync_multi(slide, selection);
        }

        self.cue_edits.clear();
        self.cue_row_rects.clear();

        if let Some(ref obj_id) = selection.object
            && let Some((obj, _)) = slide.object(obj_id)
        {
            self.name = obj.name.clone();
            self.frame_x = obj.frame.x;
            self.frame_y = obj.frame.y;
            self.frame_w = obj.frame.w;
            self.frame_h = obj.frame.h;
            self.rotation = coursecraft_course::normalize_rotation(obj.rotation);
            self.opacity = coursecraft_course::clamp_opacity(obj.opacity);
            self.shadow_enabled = obj.shadow.is_some();
            if let Some(shadow) = &obj.shadow {
                self.shadow = shadow.clone();
            }
            self.glow_enabled = obj.glow.is_some();
            if let Some(glow) = &obj.glow {
                self.glow = glow.clone();
            }
            self.drag_enabled = obj.drag.is_some();
            self.drag_toggle_rect = None;
            if let Some(drag) = &obj.drag {
                self.drag = drag.clone();
            } else {
                self.drag = coursecraft_course::DragSpec::default();
            }
            self.fill_style = match &obj.kind {
                ObjectKind::Shape { fill_style, .. } | ObjectKind::Button { fill_style, .. } => {
                    fill_style.clone()
                }
                _ => None,
            };
            self.start_ms = obj.timing.start_ms;
            self.until_end = obj.timing.end_ms.is_none();
            self.end_ms = obj.timing.end_ms.unwrap_or(slide.duration_ms);
            self.initial_state = obj.initial_state.clone();
            self.description = obj.description.clone().unwrap_or_default();
            self.decorative = obj.decorative;
            self.focus_order = obj.focus_order;

            if let Some(ref a) = obj.entrance {
                self.entrance_effect = match a.effect {
                    AnimEffect::Fade => "fade".to_owned(),
                    AnimEffect::FlyIn { .. } => "fly_in".to_owned(),
                    AnimEffect::Zoom => "zoom".to_owned(),
                    AnimEffect::Wipe { .. } => "wipe".to_owned(),
                    AnimEffect::Float { .. } => "float".to_owned(),
                };
                self.entrance_direction = match a.effect {
                    AnimEffect::FlyIn { from }
                    | AnimEffect::Wipe { from }
                    | AnimEffect::Float { from } => from,
                    _ => Direction::Bottom,
                };
                self.entrance_duration_ms = a.duration_ms;
                self.entrance_easing = a.easing;
            } else {
                self.entrance_effect = "none".to_owned();
                self.entrance_direction = Direction::Bottom;
                self.entrance_duration_ms = 500;
                self.entrance_easing = Easing::EaseOut;
            }

            if let Some(ref a) = obj.exit {
                self.exit_effect = match a.effect {
                    AnimEffect::Fade => "fade".to_owned(),
                    AnimEffect::FlyIn { .. } => "fly_in".to_owned(),
                    AnimEffect::Zoom => "zoom".to_owned(),
                    AnimEffect::Wipe { .. } => "wipe".to_owned(),
                    AnimEffect::Float { .. } => "float".to_owned(),
                };
                self.exit_direction = match a.effect {
                    AnimEffect::FlyIn { from }
                    | AnimEffect::Wipe { from }
                    | AnimEffect::Float { from } => from,
                    _ => Direction::Bottom,
                };
                self.exit_duration_ms = a.duration_ms;
                self.exit_easing = a.easing;
            } else {
                self.exit_effect = "none".to_owned();
                self.exit_direction = Direction::Bottom;
                self.exit_duration_ms = 500;
                self.exit_easing = Easing::EaseOut;
            }

            match &obj.kind {
                ObjectKind::Text { text, style, .. } => {
                    self.text_body = text.clone();
                    self.text_size = style.size;
                    self.text_color = style.color.0.clone();
                    self.text_bold = style.bold;
                    self.text_italic = style.italic;
                    self.text_underline = style.underline;
                    self.text_align = style.align;
                    self.text_valign = style.valign;
                }
                ObjectKind::Button { label, fill, .. } => {
                    self.button_label = label.clone();
                    self.button_fill = fill.0.clone();
                }
                ObjectKind::Shape {
                    shape,
                    fill,
                    outline,
                    text,
                    ..
                } => {
                    self.shape_kind = *shape;
                    self.shape_fill = fill.0.clone();
                    self.shape_outline_enabled = outline.is_some();
                    if let Some(outline) = outline {
                        self.shape_outline_color = outline.color.0.clone();
                        self.shape_outline_width = outline.width;
                        self.shape_outline_dash = outline.dash;
                    } else {
                        self.shape_outline_color = "#1F1F1F".into();
                        self.shape_outline_width = 1.5;
                        self.shape_outline_dash = coursecraft_course::DashStyle::Solid;
                    }
                    if let Some(text) = text {
                        self.text_body = text.text.clone();
                        self.text_size = text.style.size;
                        self.text_color = text.style.color.0.clone();
                        self.text_bold = text.style.bold;
                        self.text_italic = text.style.italic;
                        self.text_underline = text.style.underline;
                        self.text_align = text.style.align;
                        self.text_valign = text.style.valign;
                    } else {
                        self.text_body.clear();
                        let default = default_shape_text_style(fill);
                        self.text_size = default.size;
                        self.text_color = default.color.0.clone();
                        self.text_bold = default.bold;
                        self.text_italic = default.italic;
                        self.text_underline = default.underline;
                        self.text_align = default.align;
                        self.text_valign = default.valign;
                    }
                }
                ObjectKind::Path { path } => {
                    self.path_outline_color = path.outline.color.0.clone();
                    self.path_outline_width = path.outline.width;
                    self.path_outline_dash = path.outline.dash;
                    self.path_start_end = path.start_end;
                    self.path_end_end = path.end_end;
                    self.path_style = path.style;
                    self.path_closed = path.closed;
                    self.path_fill = path
                        .fill
                        .as_ref()
                        .map_or_else(|| "#2F6FEB".into(), |fill| fill.0.clone());
                    self.path_connector = path.connector;
                }
                ObjectKind::Image { src, alt, .. } => {
                    self.image_src = src.clone();
                    self.image_alt = alt.clone();
                }
                ObjectKind::Audio {
                    src,
                    autoplay,
                    captions,
                    volume,
                    trim,
                    ..
                } => {
                    self.media_src = src.clone();
                    self.media_autoplay = *autoplay;
                    self.media_captions = captions.clone().unwrap_or_default();
                    self.media_volume = *volume * 100.0;
                    self.media_trim_start =
                        format_clip_time(trim.as_ref().map_or(0, |trim| trim.start_ms));
                    self.media_trim_end = trim
                        .as_ref()
                        .and_then(|trim| trim.end_ms)
                        .map(format_clip_time)
                        .unwrap_or_default();
                }
                ObjectKind::Video {
                    src,
                    autoplay,
                    captions,
                    cues,
                    volume,
                    trim,
                    ..
                } => {
                    self.media_src = src.clone();
                    self.media_autoplay = *autoplay;
                    self.media_captions = captions.clone().unwrap_or_default();
                    self.media_volume = *volume * 100.0;
                    self.media_trim_start =
                        format_clip_time(trim.as_ref().map_or(0, |trim| trim.start_ms));
                    self.media_trim_end = trim
                        .as_ref()
                        .and_then(|trim| trim.end_ms)
                        .map(format_clip_time)
                        .unwrap_or_default();
                    for cue in cues {
                        self.cue_edits.insert(
                            cue.id.clone(),
                            CueEdit {
                                name: cue.name.clone(),
                                at_ms: cue.at_ms,
                                pause: cue.pause,
                            },
                        );
                    }
                }
                ObjectKind::Lottie {
                    src,
                    loop_playback,
                    autoplay,
                    speed,
                } => {
                    self.media_src = src.clone();
                    self.lottie_loop = *loop_playback;
                    self.lottie_autoplay = *autoplay;
                    self.lottie_speed = *speed;
                }
                ObjectKind::Web { source } => match source {
                    coursecraft_course::WebSource::Url { url } => {
                        self.web_is_url = true;
                        self.web_url = url.clone();
                    }
                    coursecraft_course::WebSource::Folder { path, entry } => {
                        self.web_is_url = false;
                        self.web_path = path.clone();
                        self.web_entry = entry.clone();
                    }
                },
                ObjectKind::TextEntry {
                    variable,
                    placeholder,
                    multiline,
                    max_chars,
                } => {
                    self.input_variable = variable.clone();
                    self.input_placeholder = placeholder.clone();
                    self.input_multiline = *multiline;
                    self.input_max_chars = *max_chars;
                }
                ObjectKind::Slider {
                    variable,
                    min,
                    max,
                    step,
                    orientation,
                } => {
                    self.input_variable = variable.clone();
                    self.input_min = *min;
                    self.input_max = *max;
                    self.input_step = *step;
                    self.input_orientation = *orientation;
                }
                ObjectKind::Dial {
                    variable,
                    min,
                    max,
                    step,
                    start_angle,
                    end_angle,
                } => {
                    self.input_variable = variable.clone();
                    self.input_min = *min;
                    self.input_max = *max;
                    self.input_step = *step;
                    self.input_start_angle = *start_angle;
                    self.input_end_angle = *end_angle;
                }
                ObjectKind::Checkbox { variable, label } => {
                    self.input_variable = variable.clone();
                    self.input_label = label.clone();
                }
                ObjectKind::RadioGroup { variable, options } => {
                    self.input_variable = variable.clone();
                    self.input_options = options.clone();
                }
                ObjectKind::Timer {
                    duration_ms,
                    format,
                    autostart,
                    style,
                } => {
                    self.timer_duration_ms = *duration_ms;
                    self.timer_format = *format;
                    self.timer_autostart = *autostart;
                    self.timer_font = style.font.clone();
                    self.text_size = style.size;
                    self.text_color = style.color.0.clone();
                    self.text_bold = style.bold;
                    self.text_italic = style.italic;
                    self.text_underline = style.underline;
                    self.text_align = style.align;
                    self.text_valign = style.valign;
                }
                ObjectKind::ProgressMeter {
                    variable,
                    min,
                    max,
                    style,
                    show_value,
                } => {
                    self.input_variable = variable.clone();
                    self.input_min = *min;
                    self.input_max = *max;
                    self.meter_style = *style;
                    self.meter_show_value = *show_value;
                }
                ObjectKind::Marker {
                    title,
                    body,
                    icon,
                    color,
                    ..
                } => {
                    self.marker_title = title.clone();
                    self.marker_body = body.clone();
                    self.marker_icon = *icon;
                    self.marker_use_theme_color = color.is_none();
                    self.marker_color = color
                        .as_ref()
                        .map_or_else(|| "#2F6FEB".to_owned(), |color| color.0.clone());
                }
                ObjectKind::ScrollPanel {
                    content_height,
                    bar,
                } => {
                    self.scroll_content_height = *content_height;
                    self.scroll_bar_visible = *bar == coursecraft_course::ScrollBar::Visible;
                }
                ObjectKind::Table { borders, .. } => {
                    self.table_outer_enabled = borders.outer.is_some();
                    self.table_inner_enabled = borders.inner.is_some();
                    if let Some(outer) = &borders.outer {
                        self.table_outer_color = outer.color.0.clone();
                        self.table_outer_width = outer.width;
                    }
                    if let Some(inner) = &borders.inner {
                        self.table_inner_color = inner.color.0.clone();
                        self.table_inner_width = inner.width;
                    }
                }
            }
            self.panel_target = obj.panel.clone().unwrap_or_default();
        }
    }

    fn sync_multi(&mut self, slide: &coursecraft_course::Slide, selection: &crate::Selection) {
        let objects: Vec<&coursecraft_course::Object> = selection
            .ids()
            .iter()
            .filter_map(|id| slide.object(id).map(|(object, _)| object))
            .collect();
        self.multi_count = objects.len();
        if objects.is_empty() {
            return;
        }

        let common_f32 = |get: fn(&coursecraft_course::Object) -> f32| -> (String, bool) {
            let first = get(objects[0]);
            let all_same = objects
                .iter()
                .all(|object| (get(object) - first).abs() < 0.001);
            if all_same {
                (format!("{first}"), false)
            } else {
                (String::new(), true)
            }
        };
        (self.multi_x, self.multi_x_mixed) = common_f32(|object| object.frame.x);
        (self.multi_y, self.multi_y_mixed) = common_f32(|object| object.frame.y);
        (self.multi_w, self.multi_w_mixed) = common_f32(|object| object.frame.w);
        (self.multi_h, self.multi_h_mixed) = common_f32(|object| object.frame.h);
        if let Some(first) = objects.first() {
            self.rotation = coursecraft_course::normalize_rotation(first.rotation);
        }

        self.multi_text = objects
            .iter()
            .all(|object| matches!(object.kind, ObjectKind::Text { .. }));
        self.multi_text_size = String::new();
        self.multi_text_size_mixed = false;
        self.multi_text_color = String::new();
        self.multi_text_color_mixed = false;
        self.multi_bold = None;
        self.multi_italic = None;
        self.multi_underline = None;
        self.multi_align = None;
        if self.multi_text {
            let styles: Vec<&TextStyle> = objects
                .iter()
                .filter_map(|object| match &object.kind {
                    ObjectKind::Text { style, .. } => Some(style),
                    _ => None,
                })
                .collect();
            if let Some(first) = styles.first() {
                let same_size = styles
                    .iter()
                    .all(|style| (style.size - first.size).abs() < 0.001);
                self.multi_text_size = if same_size {
                    format!("{}", first.size)
                } else {
                    String::new()
                };
                self.multi_text_size_mixed = !same_size;
                let same_color = styles.iter().all(|style| style.color == first.color);
                self.multi_text_color = if same_color {
                    first.color.0.clone()
                } else {
                    String::new()
                };
                self.multi_text_color_mixed = !same_color;
                self.multi_bold = all_same_bool(styles.iter().map(|style| style.bold));
                self.multi_italic = all_same_bool(styles.iter().map(|style| style.italic));
                self.multi_underline = all_same_bool(styles.iter().map(|style| style.underline));
                self.multi_align = all_same_align(styles.iter().map(|style| style.align));
            }
        }

        let all_shapes = objects
            .iter()
            .all(|object| matches!(object.kind, ObjectKind::Shape { .. }));
        let all_buttons = objects
            .iter()
            .all(|object| matches!(object.kind, ObjectKind::Button { .. }));
        self.multi_fill_allowed = all_shapes || all_buttons;
        self.multi_fill = String::new();
        self.multi_fill_mixed = false;
        if self.multi_fill_allowed {
            let fills: Vec<&Color> = objects
                .iter()
                .filter_map(|object| match &object.kind {
                    ObjectKind::Shape { fill, .. } | ObjectKind::Button { fill, .. } => Some(fill),
                    _ => None,
                })
                .collect();
            if let Some(first) = fills.first() {
                let same = fills.iter().all(|fill| *fill == *first);
                self.multi_fill = if same { first.0.clone() } else { String::new() };
                self.multi_fill_mixed = !same;
            }
        }

        self.multi_decorative = all_same_bool(objects.iter().map(|object| object.decorative));
    }
}

fn all_same_bool(values: impl Iterator<Item = bool>) -> Option<bool> {
    let mut iter = values;
    let first = iter.next()?;
    if iter.all(|value| value == first) {
        Some(first)
    } else {
        None
    }
}

fn all_same_align(values: impl Iterator<Item = Align>) -> Option<Align> {
    let mut iter = values;
    let first = iter.next()?;
    if iter.all(|value| value == first) {
        Some(first)
    } else {
        None
    }
}

/// A drag value laid out from the left of `size`, like the combos and text fields beside it
/// (`add_sized` would centre its text).
pub(crate) fn add_drag_left(ui: &mut Ui, size: [f32; 2], drag: DragValue<'_>) -> Response {
    ui.allocate_ui_with_layout(
        vec2(size[0], size[1]),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| ui.add(drag),
    )
    .inner
}

/// Standard inspector row layout: fixed 92 px label column on the left, control filling the
/// remaining width. A label too long for the column is elided with "..." and shows in full on
/// hover. The controls never grow the row: whatever they need beyond the width is cut at the
/// row edge instead of pushing the following rows past the panel's right margin.
pub fn inspector_row<R>(ui: &mut Ui, label: &str, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
    inspector_row_inner(ui, label, add_contents)
}

fn inspector_row_inner<R>(ui: &mut Ui, label: &str, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
    use egui::text::{LayoutJob, TextWrapping};
    let p = coursecraft_render::palette(ui.ctx());
    // Keynote style: labels never carry a trailing colon.
    let label = label.trim_end_matches(':');
    let row_h = coursecraft_render::CONTROL_COMPACT;
    let align = egui::Align::Center;
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), row_h),
        egui::Layout::left_to_right(align),
        |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let (label_rect, label_resp) =
                ui.allocate_exact_size(vec2(LABEL_COL_WIDTH, row_h), Sense::hover());
            if ui.is_rect_visible(label_rect) {
                let mut job = LayoutJob::simple_singleline(
                    label.to_owned(),
                    FontId::proportional(coursecraft_render::SIZE_BODY),
                    p.text_secondary,
                );
                job.wrap = TextWrapping::truncate_at_width(label_rect.width());
                let galley = ui.painter().layout_job(job);
                let elided = galley.elided;
                let pos = egui::pos2(
                    label_rect.min.x,
                    label_rect.center().y - galley.size().y * 0.5,
                );
                ui.painter()
                    .with_clip_rect(ui.clip_rect().intersect(label_rect))
                    .galley(pos, galley, p.text_secondary);
                if elided {
                    label_resp.on_hover_text(label);
                }
            }
            let rem_w = ui.available_width().max(30.0);
            let origin = ui.cursor().min;
            let max_h = row_h;
            let layout = egui::Layout::left_to_right(align);
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(Rect::from_min_size(origin, vec2(rem_w, max_h)))
                    .layout(layout),
            );
            child.spacing_mut().button_padding = vec2(4.0, 2.0);
            child.spacing_mut().item_spacing.x = 4.0;
            let inner = add_contents(&mut child);
            let used = child.min_rect();
            ui.allocate_rect(
                Rect::from_min_size(origin, vec2(rem_w, used.height().max(row_h))),
                Sense::hover(),
            );
            inner
        },
    )
    .inner
}

/// 1 px hairline separator with 16 px spacing above and below.
pub fn inspector_separator(ui: &mut Ui) {
    let p = coursecraft_render::palette(ui.ctx());
    ui.add_space(16.0);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().line_segment(
        [rect.left_center(), rect.right_center()],
        Stroke::new(1.0, p.border),
    );
    ui.add_space(16.0);
}

/// Section header using the `caption` style.
pub fn inspector_section_header(ui: &mut Ui, title: &str) {
    ui.label(coursecraft_render::caption(title));
    ui.add_space(8.0);
}

/// Swatch (18×18, radius 4, 1 px border_strong) plus hex text input filling remaining width on one row.
pub(crate) fn show_color_field(
    ui: &mut Ui,
    label: &str,
    color_hex: &mut String,
    id_salt: &'static str,
) -> (Response, bool) {
    let mut render_swatch_and_input = |ui: &mut Ui| -> Response {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let picked = color_swatch_picker(ui, color_hex, id_salt, 18.0, 4);
            let rem_w = ui.available_width().max(30.0);
            let response = ui.add_sized(
                [rem_w, coursecraft_render::CONTROL_COMPACT],
                TextEdit::singleline(color_hex).id_salt(id_salt),
            );
            if picked {
                mark_color_picked(&response);
            }
            response
        })
        .inner
    };

    let resp = if label.is_empty() {
        render_swatch_and_input(ui)
    } else {
        inspector_row(ui, label, |ui| render_swatch_and_input(ui))
    };
    let is_valid = parse_color(&Color(color_hex.trim().to_owned())).is_some();
    (resp, is_valid)
}

/// A colour as the hex the course stores: `#RRGGBB`, or `#RRGGBBAA` when it is
/// not fully opaque.
pub(crate) fn color_to_hex(color: Color32) -> String {
    let [r, g, b, a] = color.to_srgba_unmultiplied();
    if a == 255 {
        format!("#{r:02X}{g:02X}{b:02X}")
    } else {
        format!("#{r:02X}{g:02X}{b:02X}{a:02X}")
    }
}

/// A clickable swatch that opens a colour picker and writes the choice into
/// `color_hex`. Returns true on the frame a choice is finished (the pointer is
/// released), so one drag across the picker is one undo step.
pub(crate) fn color_swatch_picker(
    ui: &mut Ui,
    color_hex: &mut String,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    side: f32,
    radius: u8,
) -> bool {
    let p = coursecraft_render::palette(ui.ctx());
    let parsed = parse_color(&Color(color_hex.trim().to_owned()));
    let (rect, response) = ui.allocate_exact_size(vec2(side, side), Sense::click());
    let response = response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text("Pick a color");
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ColorWell, true, "Pick a color"));
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(radius),
        parsed.unwrap_or(Color32::TRANSPARENT),
    );
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(radius),
        Stroke::new(1.0, p.border_strong),
        StrokeKind::Outside,
    );

    let popup_id = ui.make_persistent_id((id_salt, "color_picker"));
    let pending_id = popup_id.with("pending");
    let mut pending = ui.data(|d| d.get_temp::<bool>(pending_id)).unwrap_or(false);
    let mut swatch_picked = false;
    egui::Popup::menu(&response)
        .id(popup_id)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            ui.spacing_mut().slider_width = 220.0;
            let palette: Vec<(String, String)> = ui
                .ctx()
                .data(|d| d.get_temp(palette_colors_id()))
                .unwrap_or_default();
            if let Some(hex) = swatch_row(ui, "Palette", &palette) {
                *color_hex = hex;
                pending = true;
                swatch_picked = true;
            }
            let theme: Vec<(String, String)> = ui
                .ctx()
                .data(|d| d.get_temp(theme_colors_id()))
                .unwrap_or_default();
            if let Some(hex) = swatch_row(ui, "Theme", &theme) {
                *color_hex = hex;
                pending = true;
                swatch_picked = true;
            }
            let recent: Vec<(String, String)> = recent_colors(ui.ctx())
                .into_iter()
                .map(|hex| (hex.clone(), hex))
                .collect();
            if let Some(hex) = swatch_row(ui, "Recent", &recent) {
                *color_hex = hex;
                pending = true;
                swatch_picked = true;
            }
            // "No fill" and an empty field have no colour to start from, and
            // dragging from invisible would stay invisible.
            let mut color = match parsed {
                Some(color) if color.a() > 0 || pending => color,
                _ => Color32::WHITE,
            };
            if egui::color_picker::color_picker_color32(
                ui,
                &mut color,
                egui::color_picker::Alpha::OnlyBlend,
            ) {
                *color_hex = color_to_hex(color);
                pending = true;
            }
        });
    let picked = swatch_picked || (pending && !ui.input(|i| i.pointer.any_down()));
    if picked {
        pending = false;
        remember_color(ui.ctx(), color_hex);
    }
    ui.data_mut(|d| d.insert_temp(pending_id, pending));
    picked
}

/// Most recent colours kept for the picker.
const MAX_RECENT_COLORS: usize = 8;

fn theme_colors_id() -> egui::Id {
    egui::Id::unique("coursecraft_theme_colors")
}

fn palette_colors_id() -> egui::Id {
    egui::Id::unique("coursecraft_palette_colors")
}

fn recent_colors_id() -> egui::Id {
    egui::Id::unique("coursecraft_recent_colors")
}

/// Store the course theme colours (label, hex) for the picker to read.
pub(crate) fn publish_theme_colors(ctx: &egui::Context, theme: &coursecraft_course::Theme) {
    let colors: Vec<(String, String)> = vec![
        ("Accent".to_owned(), theme.accent.0.clone()),
        ("Background".to_owned(), theme.background.0.clone()),
        ("Text".to_owned(), theme.text.0.clone()),
        ("Player bar".to_owned(), theme.player.bar.0.clone()),
        (
            "Player bar text".to_owned(),
            theme.player.bar_text.0.clone(),
        ),
    ];
    ctx.data_mut(|d| d.insert_temp(theme_colors_id(), colors));
    let palette: Vec<(String, String)> = theme
        .palette
        .iter()
        .map(|c| (c.name.clone(), c.color.0.clone()))
        .collect();
    ctx.data_mut(|d| d.insert_temp(palette_colors_id(), palette));
}

/// Recent colours, newest first.
pub(crate) fn recent_colors(ctx: &egui::Context) -> Vec<String> {
    ctx.data(|d| d.get_temp::<Vec<String>>(recent_colors_id()))
        .unwrap_or_default()
}

/// Add a finished choice to the recent list: newest first, no duplicates.
pub(crate) fn remember_color(ctx: &egui::Context, hex: &str) {
    let hex = hex.trim().to_uppercase();
    if parse_color(&Color(hex.clone())).is_none() {
        return;
    }
    let mut list = recent_colors(ctx);
    list.retain(|c| *c != hex);
    list.insert(0, hex);
    list.truncate(MAX_RECENT_COLORS);
    ctx.data_mut(|d| d.insert_temp(recent_colors_id(), list));
}

/// A labelled row of small swatches `(tooltip label, hex)`. Returns the clicked hex.
fn swatch_row(ui: &mut Ui, title: &str, colors: &[(String, String)]) -> Option<String> {
    if colors.is_empty() {
        return None;
    }
    let p = coursecraft_render::palette(ui.ctx());
    let mut clicked = None;
    ui.label(
        RichText::new(title)
            .size(coursecraft_render::SIZE_SMALL)
            .color(p.text_secondary),
    );
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
        for (label, hex) in colors {
            let Some(color) = parse_color(&Color(hex.clone())) else {
                continue;
            };
            let (rect, resp) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::click());
            ui.painter().rect_filled(rect, CornerRadius::same(3), color);
            ui.painter().rect_stroke(
                rect,
                CornerRadius::same(3),
                Stroke::new(1.0, p.border_strong),
                StrokeKind::Outside,
            );
            let name = if label == hex {
                hex.clone()
            } else {
                format!("{label} {hex}")
            };
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &name));
            let resp = resp
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text(&name);
            if resp.clicked() {
                clicked = Some(hex.clone());
            }
        }
    });
    ui.add_space(4.0);
    clicked
}

fn color_picked_id() -> egui::Id {
    egui::Id::unique("coursecraft_color_picked")
}

/// Tell [`is_committed`] that `field`'s value was just set by the picker.
pub(crate) fn mark_color_picked(field: &Response) {
    let pass = field.ctx.cumulative_pass_nr();
    field
        .ctx
        .data_mut(|d| d.insert_temp(color_picked_id(), (field.id, pass)));
}

/// Paired numeric fields (X/Y, W/H) sitting on one row as two equal halves, each with a small leading label.
#[allow(clippy::too_many_arguments)]
pub(crate) fn show_paired_numeric_row(
    ui: &mut Ui,
    label: &str,
    l1: &str,
    v1: &mut f32,
    speed1: f32,
    l2: &str,
    v2: &mut f32,
    speed2: f32,
) -> ((Response, bool), (Response, bool)) {
    inspector_row(ui, label, |ui| {
        let p = coursecraft_render::palette(ui.ctx());
        let total_w = ui.available_width();
        let gap = 6.0;
        let half_w = ((total_w - gap) * 0.5).max(30.0);
        ui.spacing_mut().item_spacing.x = gap;

        let r1 = ui
            .allocate_ui_with_layout(
                vec2(half_w, coursecraft_render::CONTROL_COMPACT),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    ui.label(
                        RichText::new(l1.trim_end_matches(':'))
                            .size(coursecraft_render::SIZE_SMALL)
                            .color(p.text_secondary),
                    );
                    let rem = ui.available_width().max(20.0);
                    let r = add_drag_left(
                        ui,
                        [rem, coursecraft_render::CONTROL_COMPACT],
                        DragValue::new(v1).speed(speed1),
                    );
                    let enter = r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    (r, enter)
                },
            )
            .inner;

        let r2 = ui
            .allocate_ui_with_layout(
                vec2(half_w, coursecraft_render::CONTROL_COMPACT),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    ui.label(
                        RichText::new(l2.trim_end_matches(':'))
                            .size(coursecraft_render::SIZE_SMALL)
                            .color(p.text_secondary),
                    );
                    let rem = ui.available_width().max(20.0);
                    let r = add_drag_left(
                        ui,
                        [rem, coursecraft_render::CONTROL_COMPACT],
                        DragValue::new(v2).speed(speed2),
                    );
                    let enter = r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    (r, enter)
                },
            )
            .inner;

        (r1, r2)
    })
}

#[allow(clippy::too_many_arguments)]
fn show_anim_row(
    ui: &mut Ui,
    title: &str,
    which: &'static str,
    effect_str: &mut String,
    direction: &mut Direction,
    duration_ms: &mut u32,
    easing: &mut Easing,
    slide_id: &str,
    obj_id: &str,
    action_to_run: &mut Option<(&'static str, Json)>,
) {
    inspector_section_header(ui, title);
    let mut changed = false;

    let effect_label = match effect_str.as_str() {
        "fade" => "Fade",
        "fly_in" => "Fly in",
        "zoom" => "Zoom",
        "wipe" => "Wipe",
        "float" => "Float",
        _ => "None",
    };

    inspector_row(ui, "Effect", |ui| {
        let rem_w = ui.available_width().max(30.0);
        egui::ComboBox::from_id_salt(format!("anim_effect_{which}"))
            .width(rem_w)
            .selected_text(effect_label)
            .show_ui(ui, |ui| {
                if ui
                    .selectable_value(effect_str, "none".to_string(), "None")
                    .clicked()
                {
                    changed = true;
                }
                if ui
                    .selectable_value(effect_str, "fade".to_string(), "Fade")
                    .clicked()
                {
                    changed = true;
                }
                if ui
                    .selectable_value(effect_str, "fly_in".to_string(), "Fly in")
                    .clicked()
                {
                    changed = true;
                }
                if ui
                    .selectable_value(effect_str, "zoom".to_string(), "Zoom")
                    .clicked()
                {
                    changed = true;
                }
                if ui
                    .selectable_value(effect_str, "wipe".to_string(), "Wipe")
                    .clicked()
                {
                    changed = true;
                }
                if ui
                    .selectable_value(effect_str, "float".to_string(), "Float")
                    .clicked()
                {
                    changed = true;
                }
            });
    });

    if effect_str.as_str() != "none" {
        inspector_row(ui, "Duration", |ui| {
            let rem_w = ui.available_width().max(30.0);
            let r = add_drag_left(
                ui,
                [rem_w, coursecraft_render::CONTROL_COMPACT],
                DragValue::new(duration_ms)
                    .speed(50.0)
                    .range(1..=60000)
                    .suffix(" ms"),
            );
            let enter = r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if r.drag_stopped() || r.lost_focus() || enter {
                changed = true;
            }
        });

        if matches!(effect_str.as_str(), "fly_in" | "wipe" | "float") {
            let dir_label = match direction {
                Direction::Left => "Left",
                Direction::Right => "Right",
                Direction::Top => "Top",
                Direction::Bottom => "Bottom",
            };
            inspector_row(ui, "Direction", |ui| {
                let rem_w = ui.available_width().max(30.0);
                egui::ComboBox::from_id_salt(format!("anim_dir_{which}"))
                    .width(rem_w)
                    .selected_text(dir_label)
                    .show_ui(ui, |ui| {
                        for (val, l) in [
                            (Direction::Bottom, "Bottom"),
                            (Direction::Left, "Left"),
                            (Direction::Right, "Right"),
                            (Direction::Top, "Top"),
                        ] {
                            if ui.selectable_value(direction, val, l).clicked() {
                                changed = true;
                            }
                        }
                    });
            });
        }

        let ease_label = match easing {
            Easing::Linear => "Linear",
            Easing::EaseIn => "EaseIn",
            Easing::EaseOut => "EaseOut",
            Easing::EaseInOut => "EaseInOut",
        };
        inspector_row(ui, "Easing", |ui| {
            let rem_w = ui.available_width().max(30.0);
            egui::ComboBox::from_id_salt(format!("anim_ease_{which}"))
                .width(rem_w)
                .selected_text(ease_label)
                .show_ui(ui, |ui| {
                    for (val, l) in [
                        (Easing::EaseOut, "EaseOut"),
                        (Easing::Linear, "Linear"),
                        (Easing::EaseIn, "EaseIn"),
                        (Easing::EaseInOut, "EaseInOut"),
                    ] {
                        if ui.selectable_value(easing, val, l).clicked() {
                            changed = true;
                        }
                    }
                });
        });
    }

    if changed {
        let anim_json = if effect_str.as_str() == "none" {
            serde_json::Value::Null
        } else {
            let mut val = serde_json::json!({
                "type": effect_str,
                "duration_ms": duration_ms,
                "easing": easing,
            });
            if matches!(effect_str.as_str(), "fly_in" | "wipe" | "float") {
                val["from"] = serde_json::json!(direction);
            }
            val
        };

        let cmd = if which == "entrance" {
            "object.set_entrance"
        } else {
            "object.set_exit"
        };

        *action_to_run = Some((
            cmd,
            serde_json::json!({
                "slide": slide_id,
                "object": obj_id,
                "animation": anim_json,
            }),
        ));
    }
}

/// Strikethrough, baseline and letter spacing in effect at the cursor.
fn effects_at_cursor(
    runs: &[TextRun],
    style: &TextStyle,
    cursor_char: usize,
) -> (bool, coursecraft_course::Baseline, f32) {
    let mut offset = 0;
    for run in runs {
        let len = run.text.chars().count();
        if cursor_char >= offset && cursor_char <= offset + len {
            return (
                run.strikethrough.unwrap_or(style.strikethrough),
                run.baseline.unwrap_or(style.baseline),
                run.letter_spacing.unwrap_or(style.letter_spacing),
            );
        }
        offset += len;
    }
    (style.strikethrough, style.baseline, style.letter_spacing)
}

fn style_at_cursor(
    runs: &[TextRun],
    style: &TextStyle,
    cursor_char: usize,
) -> (bool, bool, bool, f32, String) {
    let mut current_offset = 0;
    let mut found_run = None;
    for run in runs {
        let len = run.text.chars().count();
        if cursor_char >= current_offset && cursor_char <= current_offset + len {
            found_run = Some(run);
            break;
        }
        current_offset += len;
    }

    let Some(found_run) = found_run else {
        return (
            style.bold,
            style.italic,
            style.underline,
            style.size,
            style.color.0.clone(),
        );
    };

    (
        found_run.bold.unwrap_or(style.bold),
        found_run.italic.unwrap_or(style.italic),
        found_run.underline.unwrap_or(style.underline),
        found_run.size.unwrap_or(style.size),
        found_run
            .color
            .as_ref()
            .map_or_else(|| style.color.0.clone(), |c| c.0.clone()),
    )
}

/// The font named by the run at `cursor_char`, if any.
fn font_at_cursor(runs: &[TextRun], cursor_char: usize) -> Option<String> {
    let mut offset = 0;
    for run in runs {
        let len = run.text.chars().count();
        if cursor_char >= offset && cursor_char < offset.saturating_add(len) {
            return run.font.clone();
        }
        offset = offset.saturating_add(len);
    }
    runs.iter().rev().find_map(|run| run.font.clone())
}

/// The link on the run at `cursor_char`, if any.
fn link_at_char(runs: &[TextRun], cursor_char: usize) -> Option<coursecraft_course::Link> {
    let mut offset = 0;
    for run in runs {
        let len = run.text.chars().count();
        if cursor_char >= offset && cursor_char < offset.saturating_add(len) {
            return run.link.clone();
        }
        offset = offset.saturating_add(len);
    }
    runs.iter().rev().find_map(|run| run.link.clone())
}

/// Render the segmented inspector panel (Format · Animate · Slide · Course).
pub fn show_properties_panel(editor: &mut Editor, ui: &mut Ui) {
    let selection_changed = editor.ui.properties.last_synced_selection != editor.selection;
    editor.ui.properties.sync(
        &editor.session.course,
        &editor.selection,
        editor.session.revision,
    );

    let mut actions_to_run: Vec<(&'static str, Json)> = Vec::new();
    let p = coursecraft_render::palette(ui.ctx());

    // 1. Pinned inspector header: the current tab's name (the toolbar switches tabs).
    egui::Frame::NONE
        .fill(p.panel)
        .inner_margin(egui::Margin::symmetric(
            coursecraft_render::PANEL_MARGIN as i8,
            8,
        ))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(coursecraft_render::title(editor.ui.inspector_tab.name()));
                if editor.ui.inspector_tab == InspectorTab::Format
                    && editor.selection.object.is_some()
                {
                    ui.add_space(6.0);
                    show_arrange_icons(editor, ui, &mut actions_to_run);
                }
            });
        });

    ui.separator();

    // 2. ScrollArea with panel padding
    let avail_w = ui.available_width();
    let mut scroll = egui::ScrollArea::vertical()
        .id_salt("properties_panel_scroll")
        .max_width(avail_w)
        .auto_shrink([false, false]);
    if selection_changed {
        scroll = scroll.vertical_scroll_offset(0.0);
    }
    scroll.show(ui, |ui| {
        ui.add_space(coursecraft_render::PANEL_MARGIN);
        egui::Frame::NONE
            .inner_margin(egui::Margin::symmetric(
                coursecraft_render::PANEL_MARGIN as i8,
                0,
            ))
            .show(ui, |ui| match editor.ui.inspector_tab {
                InspectorTab::Format => show_format_tab(editor, ui, &mut actions_to_run),
                InspectorTab::Animate => show_animate_tab(editor, ui, &mut actions_to_run),
                InspectorTab::Slide => show_slide_tab(editor, ui, &mut actions_to_run),
                InspectorTab::Course => show_course_tab(editor, ui, &mut actions_to_run),
            });
        ui.add_space(coursecraft_render::PANEL_MARGIN);
    });

    if std::mem::take(&mut editor.ui.properties.add_font_requested) {
        editor
            .pending_requests
            .push(crate::EditorRequest::PickFonts);
    }

    if let Some((slide, object)) = editor.ui.properties.alt_suggest_request.take() {
        crate::alt_text::suggest_for_object(editor, &slide, &object);
    }

    if actions_to_run.len() > 1 {
        let steps: Vec<(String, Json)> = actions_to_run
            .into_iter()
            .map(|(cmd, params)| (cmd.to_owned(), params))
            .collect();
        editor.run_batch(&steps);
    } else if let Some((cmd, params)) = actions_to_run.pop() {
        editor.run(cmd, params);
    }
}

/// Compact row of align, distribute and match-size icon buttons.
fn show_arrange_icons(
    editor: &mut Editor,
    ui: &mut Ui,
    actions_to_run: &mut Vec<(&'static str, Json)>,
) {
    use coursecraft_render::Icon;

    if editor.selection.object.is_none() {
        return;
    }
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = vec2(2.0, 2.0);
        ui.spacing_mut().button_padding = vec2(2.0, 1.0);
        let icon_size = coursecraft_render::ICON_SM - 2.0;
        for (icon, label, edge) in [
            (Icon::TextAlignLeft, "Align Left", "left"),
            (Icon::TextAlignCenter, "Align Center", "center"),
            (Icon::TextAlignRight, "Align Right", "right"),
            (Icon::ArrowUp, "Align Top", "top"),
            (Icon::ArrowsDownUp, "Align Middle", "middle"),
            (Icon::ArrowDown, "Align Bottom", "bottom"),
        ] {
            let params = crate::menu::align_params(editor, edge);
            let enabled = params.is_some();
            let button = egui::Button::new(coursecraft_render::icon(icon, icon_size)).frame(false);
            let response = ui.add_enabled(enabled, button);
            let response = if enabled {
                response.on_hover_text(label)
            } else {
                response.on_disabled_hover_text("Select an object first")
            };
            if response.clicked()
                && let Some(params) = params
            {
                actions_to_run.push(("object.align", params));
            }
        }
        for (icon, label, axis) in [
            (Icon::TextColumns, "Distribute Horizontally", "horizontal"),
            (Icon::Rows, "Distribute Vertically", "vertical"),
        ] {
            let params = crate::menu::distribute_params(editor, axis);
            let enabled = params.is_some();
            let button = egui::Button::new(coursecraft_render::icon(icon, icon_size)).frame(false);
            let response = ui.add_enabled(enabled, button);
            let response = if enabled {
                response.on_hover_text(label)
            } else {
                response.on_disabled_hover_text("Select at least three objects")
            };
            if response.clicked()
                && let Some(params) = params
            {
                actions_to_run.push(("object.distribute", params));
            }
        }
        for (icon, label, dimension) in [
            (Icon::TextColumns, "Match Width", "width"),
            (Icon::Rows, "Match Height", "height"),
        ] {
            let params = crate::menu::match_size_params(editor, dimension);
            let enabled = params.is_some();
            let button = egui::Button::new(coursecraft_render::icon(icon, icon_size)).frame(false);
            let response = ui.add_enabled(enabled, button);
            let response = if enabled {
                response.on_hover_text(label)
            } else {
                response.on_disabled_hover_text("Select at least two objects")
            };
            if response.clicked()
                && let Some(params) = params
            {
                actions_to_run.push(("object.match_size", params));
            }
        }
    });
}

fn show_format_tab(
    editor: &mut Editor,
    ui: &mut Ui,
    actions_to_run: &mut Vec<(&'static str, Json)>,
) {
    let p = coursecraft_render::palette(ui.ctx());

    if editor.selection.object.is_none() {
        // Centered empty state
        ui.vertical_centered(|ui| {
            ui.add_space(60.0);
            ui.label(
                RichText::new("Select an object on the slide to edit it")
                    .color(p.text_secondary)
                    .size(coursecraft_render::SIZE_BODY),
            );
            ui.add_space(8.0);
            let edit_link = egui::Button::new(
                RichText::new("Edit slide")
                    .color(p.accent_text)
                    .size(coursecraft_render::SIZE_BODY),
            )
            .frame(false);
            if ui.add(edit_link).clicked() {
                editor.ui.inspector_tab = InspectorTab::Slide;
            }
        });
        return;
    }

    if editor.selection.is_multi() {
        show_multi_properties(editor, ui, actions_to_run);
        return;
    }

    show_object_properties(editor, ui, actions_to_run);

    inspector_separator(ui);
    inspector_section_header(ui, "States");
    crate::states::show_states_section(editor, ui, actions_to_run);

    inspector_separator(ui);
    show_accessibility_section(editor, ui, actions_to_run);
}

fn parse_finite(value: &str) -> Option<f32> {
    let value: f32 = value.trim().parse().ok()?;
    value.is_finite().then_some(value)
}

/// A default shape text style that contrasts with the fill, centred both ways.
fn default_shape_text_style(fill: &Color) -> TextStyle {
    let dark = Color("#1F1F1F".to_owned());
    let light = Color("#FFFFFF".to_owned());
    let color = match coursecraft_course::parse_color(fill) {
        Some(fill) if fill.a > 0 => {
            let luminance = coursecraft_course::relative_luminance(fill);
            if luminance < 0.45 { light } else { dark }
        }
        _ => dark,
    };
    TextStyle {
        color,
        align: Align::Center,
        valign: VAlign::Middle,
        ..TextStyle::default()
    }
}

fn show_multi_properties(
    editor: &mut Editor,
    ui: &mut Ui,
    actions_to_run: &mut Vec<(&'static str, Json)>,
) {
    let p = coursecraft_render::palette(ui.ctx());
    let Some(slide) = editor.session.course.slide(&editor.selection.slide) else {
        return;
    };
    let objects: Vec<coursecraft_course::Object> = editor
        .selection
        .ids()
        .iter()
        .filter_map(|id| slide.object(id).map(|(object, _)| object.clone()))
        .collect();
    if objects.is_empty() {
        return;
    }
    let is_one_group = editor.selection.is_one_group(&editor.session.course);
    let group_ids: Vec<String> = objects.iter().map(|object| object.id.clone()).collect();
    let props = &mut editor.ui.properties;
    let header = if is_one_group {
        format!("Group of {} objects", objects.len())
    } else {
        format!("{} objects selected", objects.len())
    };
    ui.label(
        RichText::new(header)
            .strong()
            .size(coursecraft_render::SIZE_BODY)
            .color(p.text),
    );

    ui.horizontal(|ui| {
        let label = if is_one_group { "Ungroup" } else { "Group" };
        let command = if is_one_group {
            "object.ungroup"
        } else {
            "object.group"
        };
        let enabled = objects.len() >= 2;
        let response = ui.add_enabled(enabled, egui::Button::new(label));
        let response = if enabled {
            response
        } else {
            response.on_disabled_hover_text("Select at least two objects")
        };
        if response.clicked() {
            actions_to_run.push((
                command,
                serde_json::json!({
                    "slide": slide.id,
                    "objects": group_ids,
                }),
            ));
        }
    });

    inspector_separator(ui);
    inspector_section_header(ui, "Frame");

    let mut set_x: Option<f32> = None;
    let mut set_y: Option<f32> = None;
    let mut set_w: Option<f32> = None;
    let mut set_h: Option<f32> = None;

    inspector_row(ui, "Position", |ui| {
        ui.label(RichText::new("X").color(p.text_secondary));
        let mut r = ui.add_sized(
            [44.0, coursecraft_render::CONTROL_COMPACT],
            TextEdit::singleline(&mut props.multi_x)
                .id_salt("prop_multi_x")
                .hint_text(if props.multi_x_mixed { "–" } else { "" }),
        );
        if props.multi_x_mixed {
            r = r.on_hover_text("Mixed values");
        }
        props.multi_x_rect = Some(r.rect);
        let enter = r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if r.lost_focus() || enter {
            set_x = parse_finite(&props.multi_x);
        }
        ui.label(RichText::new("Y").color(p.text_secondary));
        let mut r = ui.add_sized(
            [44.0, coursecraft_render::CONTROL_COMPACT],
            TextEdit::singleline(&mut props.multi_y)
                .id_salt("prop_multi_y")
                .hint_text(if props.multi_y_mixed { "–" } else { "" }),
        );
        if props.multi_y_mixed {
            r = r.on_hover_text("Mixed values");
        }
        let enter = r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if r.lost_focus() || enter {
            set_y = parse_finite(&props.multi_y);
        }
    });

    inspector_row(ui, "Size", |ui| {
        ui.label(RichText::new("W").color(p.text_secondary));
        let mut r = ui.add_sized(
            [44.0, coursecraft_render::CONTROL_COMPACT],
            TextEdit::singleline(&mut props.multi_w)
                .id_salt("prop_multi_w")
                .hint_text(if props.multi_w_mixed { "–" } else { "" }),
        );
        if props.multi_w_mixed {
            r = r.on_hover_text("Mixed values");
        }
        let enter = r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if r.lost_focus() || enter {
            set_w = parse_finite(&props.multi_w).map(|value| value.max(4.0));
        }
        ui.label(RichText::new("H").color(p.text_secondary));
        let mut r = ui.add_sized(
            [44.0, coursecraft_render::CONTROL_COMPACT],
            TextEdit::singleline(&mut props.multi_h)
                .id_salt("prop_multi_h")
                .hint_text(if props.multi_h_mixed { "–" } else { "" }),
        );
        if props.multi_h_mixed {
            r = r.on_hover_text("Mixed values");
        }
        let enter = r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if r.lost_focus() || enter {
            set_h = parse_finite(&props.multi_h).map(|value| value.max(4.0));
        }
    });

    if set_x.is_some() || set_y.is_some() || set_w.is_some() || set_h.is_some() {
        for object in &objects {
            let frame = coursecraft_course::Rect {
                x: set_x.unwrap_or(object.frame.x),
                y: set_y.unwrap_or(object.frame.y),
                w: set_w.unwrap_or(object.frame.w),
                h: set_h.unwrap_or(object.frame.h),
            };
            if [frame.x, frame.y, frame.w, frame.h]
                .iter()
                .all(|value| value.is_finite())
                && frame != object.frame
            {
                actions_to_run.push((
                    "object.set_frame",
                    serde_json::json!({
                        "slide": slide.id,
                        "object": object.id,
                        "frame": { "x": frame.x, "y": frame.y, "w": frame.w, "h": frame.h },
                    }),
                ));
            }
        }
    }

    if props.multi_text {
        inspector_separator(ui);
        inspector_section_header(ui, "Text");

        inspector_row(ui, "Style", |ui| {
            let bold = props.multi_bold;
            if ui
                .selectable_label(bold == Some(true), RichText::new("B").strong())
                .clicked()
            {
                let value = bold.is_none_or(|value| !value);
                for object in &objects {
                    let needs = matches!(
                        &object.kind,
                        ObjectKind::Text { style, .. } if style.bold != value
                    );
                    if needs {
                        actions_to_run.push((
                            "text.style_range",
                            serde_json::json!({
                                "slide": slide.id,
                                "object": object.id,
                                "start": 0,
                                "end": 0,
                                "bold": value,
                            }),
                        ));
                    }
                }
            }
            let italic = props.multi_italic;
            if ui
                .selectable_label(italic == Some(true), RichText::new("I").italics())
                .clicked()
            {
                let value = italic.is_none_or(|value| !value);
                for object in &objects {
                    let needs = matches!(
                        &object.kind,
                        ObjectKind::Text { style, .. } if style.italic != value
                    );
                    if needs {
                        actions_to_run.push((
                            "text.style_range",
                            serde_json::json!({
                                "slide": slide.id,
                                "object": object.id,
                                "start": 0,
                                "end": 0,
                                "italic": value,
                            }),
                        ));
                    }
                }
            }
            let underline = props.multi_underline;
            if ui
                .selectable_label(underline == Some(true), RichText::new("U").underline())
                .clicked()
            {
                let value = underline.is_none_or(|value| !value);
                for object in &objects {
                    let needs = matches!(
                        &object.kind,
                        ObjectKind::Text { style, .. } if style.underline != value
                    );
                    if needs {
                        actions_to_run.push((
                            "text.style_range",
                            serde_json::json!({
                                "slide": slide.id,
                                "object": object.id,
                                "start": 0,
                                "end": 0,
                                "underline": value,
                            }),
                        ));
                    }
                }
            }
        });

        inspector_row(ui, "Size", |ui| {
            let r = ui.add_sized(
                [
                    ui.available_width().max(30.0),
                    coursecraft_render::CONTROL_COMPACT,
                ],
                TextEdit::singleline(&mut props.multi_text_size)
                    .id_salt("prop_multi_text_size")
                    .hint_text(if props.multi_text_size_mixed {
                        "Mixed"
                    } else {
                        ""
                    }),
            );
            let enter = r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (r.lost_focus() || enter)
                && let Some(size) = parse_finite(&props.multi_text_size)
                && size > 0.0
            {
                for object in &objects {
                    let needs = matches!(
                        &object.kind,
                        ObjectKind::Text { style, .. } if (style.size - size).abs() >= 0.001
                    );
                    if needs {
                        actions_to_run.push((
                            "text.style_range",
                            serde_json::json!({
                                "slide": slide.id,
                                "object": object.id,
                                "start": 0,
                                "end": 0,
                                "size": size,
                            }),
                        ));
                    }
                }
            }
        });

        let (color_response, valid_color) = show_color_field(
            ui,
            "Color",
            &mut props.multi_text_color,
            "prop_multi_text_color",
        );
        if is_committed(&color_response, ui)
            && valid_color
            && !props.multi_text_color.trim().is_empty()
        {
            let color = props.multi_text_color.trim().to_owned();
            for object in &objects {
                let needs = matches!(
                    &object.kind,
                    ObjectKind::Text { style, .. } if style.color.0 != color
                );
                if needs {
                    actions_to_run.push((
                        "text.style_range",
                        serde_json::json!({
                            "slide": slide.id,
                            "object": object.id,
                            "start": 0,
                            "end": 0,
                            "color": color,
                        }),
                    ));
                }
            }
        }

        inspector_row(ui, "Align", |ui| {
            let current = props.multi_align;
            for (align, label) in [
                (Align::Left, "Left"),
                (Align::Center, "Center"),
                (Align::Right, "Right"),
            ] {
                if ui.selectable_label(current == Some(align), label).clicked() {
                    let name = match align {
                        Align::Left => "left",
                        Align::Center => "center",
                        Align::Right => "right",
                    };
                    for object in &objects {
                        let needs = matches!(
                            &object.kind,
                            ObjectKind::Text { style, .. } if style.align != align
                        );
                        if needs {
                            actions_to_run.push((
                                "text.set_paragraph",
                                serde_json::json!({
                                    "slide": slide.id,
                                    "object": object.id,
                                    "align": name,
                                }),
                            ));
                        }
                    }
                }
            }
        });
    }

    if props.multi_fill_allowed {
        inspector_separator(ui);
        let (fill_response, valid_fill) =
            show_color_field(ui, "Fill", &mut props.multi_fill, "prop_multi_fill");
        if is_committed(&fill_response, ui) && valid_fill && !props.multi_fill.trim().is_empty() {
            let fill = props.multi_fill.trim().to_owned();
            for object in &objects {
                let (kind, needs) = match &object.kind {
                    ObjectKind::Shape { fill: current, .. } => {
                        let mut kind = serde_json::to_value(&object.kind).unwrap_or(Json::Null);
                        kind["fill"] = serde_json::json!(fill);
                        (kind, current.0 != fill)
                    }
                    ObjectKind::Button {
                        label,
                        fill: current,
                        ..
                    } => (
                        serde_json::json!({
                            "type": "button",
                            "label": label,
                            "fill": fill,
                        }),
                        current.0 != fill,
                    ),
                    _ => continue,
                };
                if needs {
                    actions_to_run.push((
                        "object.update",
                        serde_json::json!({
                            "slide": slide.id,
                            "object": object.id,
                            "kind": kind,
                        }),
                    ));
                }
            }
        }
        let all_shapes = objects
            .iter()
            .all(|object| matches!(object.kind, ObjectKind::Shape { .. }));
        if all_shapes {
            inspector_row(ui, "No fill", |ui| {
                let all_no_fill = objects.iter().all(|object| {
                    matches!(&object.kind, ObjectKind::Shape { fill, .. } if fill.0 == "#00000000")
                });
                let mut checked = all_no_fill;
                if ui.checkbox(&mut checked, "").changed() {
                    let next_fill = if checked {
                        "#00000000".to_owned()
                    } else {
                        props.multi_fill.trim().to_owned()
                    };
                    let next_fill = if next_fill.is_empty() || next_fill == "#00000000" {
                        "#D0D7DE".to_owned()
                    } else {
                        next_fill
                    };
                    for object in &objects {
                        let needs = matches!(&object.kind, ObjectKind::Shape { fill, .. } if fill.0 != next_fill);
                        if needs {
                            let mut kind = serde_json::to_value(&object.kind).unwrap_or(Json::Null);
                            kind["fill"] = serde_json::json!(next_fill);
                            actions_to_run.push((
                                "object.update",
                                serde_json::json!({
                                    "slide": slide.id,
                                    "object": object.id,
                                    "kind": kind,
                                }),
                            ));
                        }
                    }
                }
            });
        }
    }

    inspector_separator(ui);
    inspector_row(ui, "Decorative", |ui| {
        let mut checked = props.multi_decorative.unwrap_or(false);
        let response = ui.checkbox(&mut checked, "");
        if props.multi_decorative.is_none() {
            ui.label(
                RichText::new("Mixed")
                    .size(coursecraft_render::SIZE_SMALL)
                    .color(p.text_tertiary),
            );
        }
        if response.changed() {
            for object in &objects {
                if object.decorative != checked {
                    actions_to_run.push((
                        "object.update",
                        serde_json::json!({
                            "slide": slide.id,
                            "object": object.id,
                            "decorative": checked,
                        }),
                    ));
                }
            }
        }
    });

    inspector_separator(ui);
    inspector_section_header(ui, "Rotate");
    inspector_row(ui, "Rotation", |ui| {
        let rem_w = ui.available_width().max(30.0);
        let r = add_drag_left(
            ui,
            [rem_w, coursecraft_render::CONTROL_COMPACT],
            DragValue::new(&mut props.rotation)
                .speed(1.0)
                .range(0.0..=360.0)
                .suffix("°"),
        );
        let enter = r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if r.drag_stopped() || r.lost_focus() || enter {
            let rotation = coursecraft_course::normalize_rotation(props.rotation);
            props.rotation = rotation;
            let ids: Vec<String> = objects.iter().map(|object| object.id.clone()).collect();
            actions_to_run.push((
                "object.set_rotation",
                serde_json::json!({
                    "slide": slide.id,
                    "objects": ids,
                    "degrees": rotation,
                }),
            ));
        }
    });

    inspector_row(ui, "Flip", |ui| {
        let icon_size = coursecraft_render::ICON_SM - 2.0;
        for (icon, label, axis) in [
            (
                coursecraft_render::Icon::ArrowsOut,
                "Flip horizontal",
                "horizontal",
            ),
            (
                coursecraft_render::Icon::ArrowsDownUp,
                "Flip vertical",
                "vertical",
            ),
        ] {
            let button = egui::Button::new(coursecraft_render::icon(icon, icon_size)).frame(false);
            if ui.add(button).on_hover_text(label).clicked() {
                let ids: Vec<String> = objects.iter().map(|object| object.id.clone()).collect();
                actions_to_run.push((
                    "object.flip",
                    serde_json::json!({
                        "slide": slide.id,
                        "objects": ids,
                        "axis": axis,
                    }),
                ));
            }
        }
    });
}

/// Object accessibility fields: text alternative, decorative flag and focus order.
fn show_accessibility_section(
    editor: &mut Editor,
    ui: &mut Ui,
    actions_to_run: &mut Vec<(&'static str, Json)>,
) {
    let Some(slide) = editor.session.course.slide(&editor.selection.slide) else {
        return;
    };
    let Some(ref obj_id) = editor.selection.object else {
        return;
    };
    let Some((obj, _)) = slide.object(obj_id) else {
        return;
    };
    let props = &mut editor.ui.properties;
    let mut action_to_run: Option<(&'static str, Json)> = None;

    egui::CollapsingHeader::new("Accessibility")
        .default_open(true)
        .id_salt("prop_accessibility")
        .show(ui, |ui| {
            inspector_row(ui, "Description", |ui| {
                let rem_w = ui.available_width().max(30.0);
                let r = ui.add_sized(
                    [rem_w, coursecraft_render::CONTROL_COMPACT],
                    TextEdit::singleline(&mut props.description).id_salt("prop_description"),
                );
                let current = obj.description.as_deref().unwrap_or("");
                if is_committed(&r, ui) && props.description.trim() != current {
                    let value = if props.description.trim().is_empty() {
                        Json::Null
                    } else {
                        Json::String(props.description.trim().to_owned())
                    };
                    action_to_run = Some((
                        "object.update",
                        serde_json::json!({
                            "slide": slide.id,
                            "object": obj.id,
                            "description": value,
                        }),
                    ));
                }
            });

            inspector_row(ui, "Decorative", |ui| {
                if ui.checkbox(&mut props.decorative, "").changed() {
                    action_to_run = Some((
                        "object.update",
                        serde_json::json!({
                            "slide": slide.id,
                            "object": obj.id,
                            "decorative": props.decorative,
                        }),
                    ));
                }
            });

            inspector_row(ui, "Focus order", |ui| {
                let mut order = props.focus_order.unwrap_or(0);
                let r = ui.add(DragValue::new(&mut order).speed(1.0));
                let enter = r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if (r.drag_stopped() || r.lost_focus() || enter) && props.focus_order != Some(order)
                {
                    props.focus_order = Some(order);
                    action_to_run = Some((
                        "object.update",
                        serde_json::json!({
                            "slide": slide.id,
                            "object": obj.id,
                            "focus_order": order,
                        }),
                    ));
                }
                if ui
                    .add_enabled(props.focus_order.is_some(), egui::Button::new("Clear"))
                    .clicked()
                {
                    props.focus_order = None;
                    action_to_run = Some((
                        "object.update",
                        serde_json::json!({
                            "slide": slide.id,
                            "object": obj.id,
                            "focus_order": Json::Null,
                        }),
                    ));
                }
            });
        });

    if let Some(act) = action_to_run {
        actions_to_run.push(act);
    }
}

fn show_animate_tab(
    editor: &mut Editor,
    ui: &mut Ui,
    actions_to_run: &mut Vec<(&'static str, Json)>,
) {
    let p = coursecraft_render::palette(ui.ctx());

    if editor.selection.is_multi() {
        ui.vertical_centered(|ui| {
            ui.add_space(8.0);
            ui.label(
                RichText::new("Select one object to animate it")
                    .color(p.text_secondary)
                    .size(coursecraft_render::SIZE_BODY),
            );
            ui.add_space(8.0);
        });
        inspector_separator(ui);
    } else if editor.selection.object.is_some() {
        show_animations_section(editor, ui, actions_to_run);
        inspector_separator(ui);
    } else {
        ui.vertical_centered(|ui| {
            ui.add_space(8.0);
            ui.label(
                RichText::new("Select an object to animate it")
                    .color(p.text_secondary)
                    .size(coursecraft_render::SIZE_BODY),
            );
            ui.add_space(8.0);
        });
        inspector_separator(ui);
    }

    inspector_section_header(ui, "Transition");
    if let Some(slide) = editor.session.course.slide(&editor.selection.slide) {
        let mut action = None;
        show_transition_section(&mut editor.ui.properties, slide, ui, &mut action);
        if let Some(act) = action {
            actions_to_run.push(act);
        }
    }
}

fn show_slide_tab(
    editor: &mut Editor,
    ui: &mut Ui,
    actions_to_run: &mut Vec<(&'static str, Json)>,
) {
    let slide_id = editor.selection.slide.clone();
    let Some(is_master) = editor
        .session
        .course
        .slide(&slide_id)
        .map(|slide| editor.session.course.master(&slide.id).is_some())
    else {
        ui.label("No slide selected");
        return;
    };

    if !is_master {
        let masters = editor.session.course.masters.clone();
        if masters.is_empty() {
            inspector_row(ui, "Master", |ui| {
                if crate::quiet_button(ui, "Create a master", crate::QuietTone::Accent, true)
                    .clicked()
                {
                    editor.enter_master_view();
                }
            });
        } else {
            let current = editor
                .session
                .course
                .slide(&slide_id)
                .and_then(|slide| slide.master.clone());
            let selected_label = current
                .as_deref()
                .and_then(|id| editor.session.course.master(id))
                .map(|master| master.title.clone())
                .unwrap_or_else(|| "None".to_owned());
            let mut set_to: Option<Option<String>> = None;
            inspector_row(ui, "Master", |ui| {
                egui::ComboBox::from_id_salt("prop_master")
                    .selected_text(selected_label)
                    .show_ui(ui, |ui| {
                        if ui.selectable_label(current.is_none(), "None").clicked() {
                            set_to = Some(None);
                            ui.close();
                        }
                        for master in &masters {
                            if ui
                                .selectable_label(
                                    current.as_deref() == Some(master.id.as_str()),
                                    &master.title,
                                )
                                .clicked()
                            {
                                set_to = Some(Some(master.id.clone()));
                                ui.close();
                            }
                        }
                    });
            });
            if let Some(master) = set_to {
                actions_to_run.push((
                    "slide.set_master",
                    serde_json::json!({ "slides": [slide_id], "master": master }),
                ));
            }
        }
        inspector_separator(ui);
    }

    let Some(slide) = editor.session.course.slide(&slide_id) else {
        ui.label("No slide selected");
        return;
    };

    // A draw slide edits its bank and count here (through draw.update).
    if let Some(draw) = slide.draw.clone() {
        inspector_section_header(ui, "Draw");
        let banks = editor.session.course.banks.clone();
        let current_label = banks
            .iter()
            .find(|bank| bank.id == draw.bank)
            .map(|bank| bank.title.clone())
            .unwrap_or_else(|| draw.bank.clone());
        let bank_size = banks
            .iter()
            .find(|bank| bank.id == draw.bank)
            .map_or(0, |bank| bank.slides.len());
        let mut set_bank: Option<String> = None;
        inspector_row(ui, "Bank", |ui| {
            egui::ComboBox::from_id_salt("prop_draw_bank")
                .selected_text(current_label)
                .show_ui(ui, |ui| {
                    for bank in &banks {
                        if ui
                            .selectable_label(draw.bank == bank.id, &bank.title)
                            .clicked()
                        {
                            set_bank = Some(bank.id.clone());
                            ui.close();
                        }
                    }
                });
        });
        if let Some(bank_id) = set_bank {
            actions_to_run.push((
                "draw.update",
                serde_json::json!({ "slide": slide_id, "bank": bank_id }),
            ));
        }
        // The widget clamps the count to the bank's size.
        let max_count = bank_size.max(1) as u32;
        let mut count = draw.count;
        let mut count_rect = None;
        inspector_row(ui, "Count", |ui| {
            let resp = ui.add(DragValue::new(&mut count).speed(1.0).range(1..=max_count));
            count_rect = Some(resp.rect);
            let enter = resp.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (resp.drag_stopped() || resp.lost_focus() || enter) && count != draw.count {
                actions_to_run.push((
                    "draw.update",
                    serde_json::json!({ "slide": slide_id, "count": count }),
                ));
            }
        });
        if let Some(rect) = count_rect {
            editor.ui.properties.draw_count_rect = Some(rect);
        }
        inspector_separator(ui);
    }

    // The pinned inspector header already reads "Slide"; don't repeat it as a caption.
    let mut narrate_requested = false;
    let bool_names = crate::course_settings::bool_variable_names(&editor.session.course);
    if let Some(act) = show_slide_properties(
        &mut editor.ui.properties,
        slide,
        ui,
        is_master,
        &mut narrate_requested,
        &bool_names,
    ) {
        actions_to_run.push(act);
    }
    if narrate_requested {
        editor.open_narration_from_notes(&slide_id);
    }
}

fn show_course_tab(
    editor: &mut Editor,
    ui: &mut Ui,
    actions_to_run: &mut Vec<(&'static str, Json)>,
) {
    // The pinned inspector header already reads "Course"; don't repeat it as a caption.
    let title = editor.session.course.title.clone();
    let slide_count = editor.session.course.slides().count();
    let quiz = editor.session.course.quiz;

    inspector_row(ui, "Title", |ui| {
        let rem_w = ui.available_width().max(30.0);
        let r = ui.add_sized(
            [rem_w, coursecraft_render::CONTROL_COMPACT],
            TextEdit::singleline(&mut editor.ui.properties.course_title)
                .id_salt("course_inspector_title"),
        );
        let enter = r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if (r.lost_focus() || enter) && editor.ui.properties.course_title.trim() != title.as_str() {
            actions_to_run.push((
                "course.update",
                serde_json::json!({ "title": editor.ui.properties.course_title.trim() }),
            ));
        }
    });
    inspector_row(ui, "Slides", |ui| {
        ui.label(format!("{slide_count} slides"));
    });
    if let Some(q) = quiz {
        inspector_row(ui, "Pass mark", |ui| {
            ui.label(format!("{}%", q.passing_percent));
        });
    }

    inspector_separator(ui);
    ui.horizontal(|ui| {
        if ui.button("Course settings…").clicked() {
            editor.ui.show_course_settings_window = true;
        }
        if ui.button("Theme…").clicked() {
            editor.ui.show_theme_window = true;
        }
    });
}

/// The Text panel controls, shared by text objects and shape text.
///
/// `current_text` is the text being edited and `id_prefix` keeps the text box's
/// egui state separate for different owners. The caller applies the returned
/// command after the panel finishes drawing.
#[allow(clippy::too_many_arguments)]
fn show_text_controls(
    ui: &mut Ui,
    props: &mut PropertiesState,
    slide_id: &str,
    obj_id: &str,
    style: &TextStyle,
    runs: &[TextRun],
    current_text: &str,
    id_prefix: &str,
    font_families: &[String],
    cell: Option<(usize, usize)>,
    autofit: Option<Autofit>,
) -> Option<(&'static str, Json)> {
    let mut action_to_run = None;
    let text_edit_id = egui::Id::unique(("prop_text_body", id_prefix));
    let text_edit_state = egui::TextEdit::load_state(ui.ctx(), text_edit_id);
    let cursor_range = text_edit_state.as_ref().and_then(|s| s.cursor.char_range());
    let (char_range, cursor_pos): ((usize, usize), usize) = if let Some(cr) = cursor_range {
        if !cr.is_empty() {
            let sorted = cr.as_sorted_char_range();
            ((sorted.start.0, sorted.end.0), sorted.start.0)
        } else {
            ((0, 0), cr.primary.index.0)
        }
    } else {
        ((0, 0), 0)
    };

    let (eff_bold, eff_italic, eff_underline, eff_size, eff_color) =
        style_at_cursor(runs, style, cursor_pos);
    let (eff_strike, eff_baseline, eff_spacing) = effects_at_cursor(runs, style, cursor_pos);

    // Font picker: Inter plus every course family, each name drawn in its own
    // face. "Add font…" opens the same picker the File menu uses.
    let current_font = font_at_cursor(runs, cursor_pos).unwrap_or_else(|| style.font.clone());
    let shown_font = if current_font.eq_ignore_ascii_case("Open Sans") {
        "Inter".to_owned()
    } else {
        current_font
    };
    inspector_row(ui, "Font", |ui| {
        let selected_font = coursecraft_render::slide_font_named(
            ui.ctx(),
            Some(&shown_font),
            false,
            false,
            coursecraft_render::SIZE_BODY,
        );
        egui::ComboBox::from_id_salt(("prop_font", id_prefix))
            .selected_text(egui::RichText::new(&shown_font).font(selected_font))
            .show_ui(ui, |ui| {
                for name in std::iter::once("Inter".to_owned()).chain(font_families.iter().cloned())
                {
                    let font = coursecraft_render::slide_font_named(
                        ui.ctx(),
                        Some(&name),
                        false,
                        false,
                        coursecraft_render::SIZE_BODY,
                    );
                    if ui
                        .selectable_label(shown_font == name, egui::RichText::new(&name).font(font))
                        .clicked()
                        && shown_font != name
                    {
                        action_to_run = Some((
                            "text.style_range",
                            serde_json::json!({
                                "slide": slide_id,
                                "object": obj_id,
                                "start": char_range.0,
                                "end": char_range.1,
                                "font": name,
                            }),
                        ));
                        ui.close();
                    }
                }
                ui.separator();
                if ui.button("Add font…").clicked() {
                    props.add_font_requested = true;
                    ui.close();
                }
            });
    });

    if let Some(autofit) = autofit {
        inspector_row(ui, "Autofit", |ui| {
            let selected = if autofit == Autofit::Shrink {
                "Shrink text on overflow"
            } else {
                "Off"
            };
            let mut chosen = None;
            egui::ComboBox::from_id_salt(("autofit", id_prefix, obj_id))
                .selected_text(selected)
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(autofit == Autofit::None, "Off")
                        .clicked()
                    {
                        chosen = Some(Autofit::None);
                    }
                    if ui
                        .selectable_label(autofit == Autofit::Shrink, "Shrink text on overflow")
                        .clicked()
                    {
                        chosen = Some(Autofit::Shrink);
                    }
                });
            if let Some(value) = chosen
                && value != autofit
            {
                action_to_run = Some((
                    "object.set_autofit",
                    serde_json::json!({
                        "slide": slide_id,
                        "object": obj_id,
                        "autofit": if value == Autofit::Shrink { "shrink" } else { "none" },
                    }),
                ));
            }
        });
    }

    // Formatting toolbar
    inspector_row(ui, "Style", |ui| {
        if ui
            .selectable_label(eff_bold, RichText::new("B").strong())
            .clicked()
        {
            action_to_run = Some((
                "text.style_range",
                serde_json::json!({
                    "slide": slide_id,
                    "object": obj_id,
                    "start": char_range.0,
                    "end": char_range.1,
                    "bold": !eff_bold,
                }),
            ));
        }
        if ui
            .selectable_label(eff_italic, RichText::new("I").italics())
            .clicked()
        {
            action_to_run = Some((
                "text.style_range",
                serde_json::json!({
                    "slide": slide_id,
                    "object": obj_id,
                    "start": char_range.0,
                    "end": char_range.1,
                    "italic": !eff_italic,
                }),
            ));
        }
        if ui
            .selectable_label(eff_underline, RichText::new("U").underline())
            .clicked()
        {
            action_to_run = Some((
                "text.style_range",
                serde_json::json!({
                    "slide": slide_id,
                    "object": obj_id,
                    "start": char_range.0,
                    "end": char_range.1,
                    "underline": !eff_underline,
                }),
            ));
        }

        if ui
            .selectable_label(eff_strike, RichText::new("S").strikethrough())
            .on_hover_text("Strikethrough")
            .clicked()
        {
            action_to_run = Some((
                "text.style_range",
                serde_json::json!({
                    "slide": slide_id,
                    "object": obj_id,
                    "start": char_range.0,
                    "end": char_range.1,
                    "strikethrough": !eff_strike,
                }),
            ));
        }
        for (label, tip, target) in [
            (
                "x\u{b2}",
                "Superscript",
                coursecraft_course::Baseline::Superscript,
            ),
            (
                "x\u{2082}",
                "Subscript",
                coursecraft_course::Baseline::Subscript,
            ),
        ] {
            if ui
                .selectable_label(eff_baseline == target, label)
                .on_hover_text(tip)
                .clicked()
            {
                let next = if eff_baseline == target {
                    coursecraft_course::Baseline::Normal
                } else {
                    target
                };
                action_to_run = Some((
                    "text.style_range",
                    serde_json::json!({
                        "slide": slide_id,
                        "object": obj_id,
                        "start": char_range.0,
                        "end": char_range.1,
                        "baseline": next,
                    }),
                ));
            }
        }
    });

    // The size sits on its own row: seven controls do not fit one row of the panel.
    inspector_row(ui, "Font size", |ui| {
        let mut size_val = eff_size;
        let rem_w = ui.available_width();
        let r_size = add_drag_left(
            ui,
            [rem_w, coursecraft_render::CONTROL_COMPACT],
            DragValue::new(&mut size_val).speed(1.0).range(1.0..=200.0),
        );
        let enter_size = r_size.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if (r_size.drag_stopped() || r_size.lost_focus() || enter_size)
            && size_val.is_finite()
            && size_val > 0.0
            && (size_val - eff_size).abs() > 0.01
        {
            action_to_run = Some((
                "text.style_range",
                serde_json::json!({
                    "slide": slide_id,
                    "object": obj_id,
                    "start": char_range.0,
                    "end": char_range.1,
                    "size": size_val,
                }),
            ));
        }
    });

    inspector_row(ui, "Spacing", |ui| {
        let mut spacing = eff_spacing;
        let r = ui.add(
            DragValue::new(&mut spacing)
                .speed(0.1)
                .range(
                    coursecraft_course::MIN_LETTER_SPACING..=coursecraft_course::MAX_LETTER_SPACING,
                )
                .suffix(" pt"),
        );
        let enter = r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if (r.drag_stopped() || r.lost_focus() || enter)
            && spacing.is_finite()
            && (spacing - eff_spacing).abs() > 0.001
        {
            action_to_run = Some((
                "text.style_range",
                serde_json::json!({
                    "slide": slide_id,
                    "object": obj_id,
                    "start": char_range.0,
                    "end": char_range.1,
                    "letter_spacing": spacing,
                }),
            ));
        }
    });

    let mut col_val = eff_color.clone();
    let (cr, valid_color) = show_color_field(ui, "Color", &mut col_val, "prop_run_color");
    props.color_rect = Some(cr.rect);
    if is_committed(&cr, ui) && valid_color && col_val.trim() != eff_color.as_str() {
        action_to_run = Some((
            "text.style_range",
            serde_json::json!({
                "slide": slide_id,
                "object": obj_id,
                "start": char_range.0,
                "end": char_range.1,
                "color": col_val.trim(),
            }),
        ));
    }

    // Paragraph alignment
    inspector_row(ui, "Align", |ui| {
        let cur_align = style.align;
        for (align, name) in [
            (Align::Left, "left"),
            (Align::Center, "center"),
            (Align::Right, "right"),
        ] {
            let label = match align {
                Align::Left => "Left",
                Align::Center => "Center",
                Align::Right => "Right",
            };
            if ui.selectable_label(cur_align == align, label).clicked() && cur_align != align {
                action_to_run = Some((
                    "text.set_paragraph",
                    serde_json::json!({ "slide": slide_id, "object": obj_id, "align": name }),
                ));
            }
        }
    });

    inspector_row(ui, "VAlign", |ui| {
        let cur_valign = style.valign;
        for (valign, name) in [
            (VAlign::Top, "top"),
            (VAlign::Middle, "middle"),
            (VAlign::Bottom, "bottom"),
        ] {
            let label = match valign {
                VAlign::Top => "Top",
                VAlign::Middle => "Middle",
                VAlign::Bottom => "Bottom",
            };
            if ui.selectable_label(cur_valign == valign, label).clicked() && cur_valign != valign {
                action_to_run = Some((
                    "text.set_paragraph",
                    serde_json::json!({ "slide": slide_id, "object": obj_id, "valign": name }),
                ));
            }
        }
    });

    inspector_row(ui, "Text Content", |ui| {
        let rem_w = ui.available_width().max(30.0);
        let r = ui.add_sized(
            [rem_w, 60.0],
            TextEdit::multiline(&mut props.text_body)
                .id(text_edit_id)
                .desired_rows(3),
        );
        if (r.lost_focus() || r.changed()) && props.text_body != current_text {
            action_to_run = Some((
                "text.set",
                serde_json::json!({
                    "slide": slide_id,
                    "object": obj_id,
                    "text": props.text_body,
                }),
            ));
        }
    });

    if let (Some((row, column)), Some((_, params))) = (cell, action_to_run.as_mut())
        && let Some(map) = params.as_object_mut()
    {
        map.insert("row".to_owned(), serde_json::json!(row));
        map.insert("column".to_owned(), serde_json::json!(column));
    }
    action_to_run
}

#[allow(clippy::too_many_arguments)]
fn show_paragraph_controls(
    ui: &mut Ui,
    props: &mut PropertiesState,
    slide_id: &str,
    obj_id: &str,
    runs: &[TextRun],
    paragraphs: &[Paragraph],
    slides: &[(String, String)],
    current_text: &str,
    id_prefix: &str,
    cell: Option<(usize, usize)>,
) -> Option<(&'static str, Json)> {
    let mut action_to_run = None;
    let text_edit_id = egui::Id::unique(("prop_text_body", id_prefix));
    let text_edit_state = egui::TextEdit::load_state(ui.ctx(), text_edit_id);
    let cursor_range = text_edit_state.as_ref().and_then(|s| s.cursor.char_range());
    let (char_range, cursor_pos): ((usize, usize), usize) = if let Some(cr) = cursor_range {
        if !cr.is_empty() {
            let sorted = cr.as_sorted_char_range();
            ((sorted.start.0, sorted.end.0), sorted.start.0)
        } else {
            ((0, 0), cr.primary.index.0)
        }
    } else {
        ((0, 0), 0)
    };
    let (para_start, para_end) = if cursor_range.is_some() {
        coursecraft_course::text::paragraph_range_at(current_text, char_range.0, char_range.1)
    } else {
        (0, coursecraft_course::text::paragraph_count(current_text))
    };
    let paragraph_styles = paragraphs_for(current_text, paragraphs);
    let current_paragraph = paragraph_styles
        .get(para_start)
        .cloned()
        .unwrap_or_default();
    let link_range = if cursor_range.is_some() {
        (char_range.0, char_range.1)
    } else {
        (0, 0)
    };
    let current_link = link_at_char(runs, cursor_pos);

    let paragraph_base = || {
        serde_json::json!({
            "slide": slide_id,
            "object": obj_id,
            "start": para_start,
            "end": para_end,
        })
    };

    inspector_row(ui, "Lists", |ui| {
        for (kind, label) in [
            (ListKind::Bullet, "Bullets"),
            (ListKind::Number, "Numbering"),
        ] {
            if ui
                .selectable_label(current_paragraph.list == kind, label)
                .clicked()
            {
                let list = if current_paragraph.list == kind {
                    "none"
                } else if kind == ListKind::Bullet {
                    "bullet"
                } else {
                    "number"
                };
                let mut params = paragraph_base();
                params["list"] = serde_json::json!(list);
                action_to_run = Some(("text.set_paragraphs", params));
            }
        }
    });

    inspector_row(ui, "Indent", |ui| {
        let level = current_paragraph.level.min(4);
        if ui
            .add_enabled(level > 0, egui::Button::new("Decrease"))
            .on_hover_text("Decrease indent")
            .clicked()
        {
            let mut params = paragraph_base();
            params["level"] = serde_json::json!(level.saturating_sub(1));
            action_to_run = Some(("text.set_paragraphs", params));
        }
        if ui
            .add_enabled(level < 4, egui::Button::new("Increase"))
            .on_hover_text("Increase indent")
            .clicked()
        {
            let mut params = paragraph_base();
            params["level"] = serde_json::json!(level + 1);
            action_to_run = Some(("text.set_paragraphs", params));
        }
    });

    inspector_row(ui, "Spacing", |ui| {
        let current = current_paragraph.line_spacing;
        let id = egui::Id::unique(("line_spacing", id_prefix, obj_id));
        let mut chosen = None;
        egui::ComboBox::from_id_salt(id)
            .selected_text(format!("{current:.2}x"))
            .show_ui(ui, |ui| {
                for value in [1.0_f32, 1.15, 1.5, 2.0] {
                    if ui
                        .selectable_label((current - value).abs() < 0.001, format!("{value}x"))
                        .clicked()
                    {
                        chosen = Some(value);
                    }
                }
                ui.separator();
                let mut custom = props.line_spacing_custom;
                let response = ui.add(
                    DragValue::new(&mut custom)
                        .speed(0.05)
                        .range(0.8..=3.0)
                        .prefix("custom "),
                );
                props.line_spacing_custom = custom;
                if response.drag_stopped() || response.lost_focus() {
                    chosen = Some(custom);
                }
            });
        if let Some(value) = chosen {
            let mut params = paragraph_base();
            params["line_spacing"] = serde_json::json!(value);
            action_to_run = Some(("text.set_paragraphs", params));
        }
    });

    inspector_row(ui, "Link", |ui| {
        if ui.button("Link…").clicked() {
            props.link_dialog_open = true;
            props.link_use_slide = false;
            props.link_url.clear();
            props.link_slide.clear();
            match &current_link {
                Some(coursecraft_course::Link::Url { url }) => {
                    props.link_url = url.clone();
                }
                Some(coursecraft_course::Link::Slide { slide }) => {
                    props.link_use_slide = true;
                    props.link_slide = slide.clone();
                }
                None => {}
            }
        }
    });

    if props.link_dialog_open {
        let mut close = false;
        let mut apply = false;
        let mut remove = false;
        egui::Window::new("Link")
            .id(egui::Id::unique(("link_dialog", id_prefix, obj_id)))
            .collapsible(false)
            .resizable(false)
            .show(ui.ctx(), |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut props.link_use_slide, false, "URL");
                    ui.selectable_value(&mut props.link_use_slide, true, "Slide");
                });
                if props.link_use_slide {
                    let chosen = slides
                        .iter()
                        .find(|(id, _)| id == &props.link_slide)
                        .map(|(_, title)| title.clone())
                        .unwrap_or_else(|| "Choose a slide…".to_owned());
                    egui::ComboBox::from_id_salt(("link_slide_pick", id_prefix, obj_id))
                        .selected_text(chosen)
                        .show_ui(ui, |ui| {
                            for (id, title) in slides {
                                ui.selectable_value(&mut props.link_slide, id.clone(), title);
                            }
                        });
                } else {
                    ui.add(
                        TextEdit::singleline(&mut props.link_url)
                            .hint_text("https://example.com")
                            .desired_width(240.0),
                    );
                }
                ui.horizontal(|ui| {
                    if ui.button("Apply").clicked() {
                        apply = true;
                    }
                    if ui.button("Remove link").clicked() {
                        remove = true;
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
            });
        if apply {
            let link = if props.link_use_slide {
                serde_json::json!({ "type": "slide", "slide": props.link_slide })
            } else {
                serde_json::json!({ "type": "url", "url": props.link_url.trim() })
            };
            action_to_run = Some((
                "text.set_link",
                serde_json::json!({
                    "slide": slide_id,
                    "object": obj_id,
                    "start": link_range.0,
                    "end": link_range.1,
                    "link": link,
                }),
            ));
            props.link_dialog_open = false;
        } else if remove {
            action_to_run = Some((
                "text.set_link",
                serde_json::json!({
                    "slide": slide_id,
                    "object": obj_id,
                    "start": link_range.0,
                    "end": link_range.1,
                    "link": null,
                }),
            ));
            props.link_dialog_open = false;
        } else if close {
            props.link_dialog_open = false;
        }
    }

    if let (Some((row, column)), Some((_, params))) = (cell, action_to_run.as_mut())
        && let Some(map) = params.as_object_mut()
    {
        map.insert("row".to_owned(), serde_json::json!(row));
        map.insert("column".to_owned(), serde_json::json!(column));
    }
    action_to_run
}

#[allow(clippy::cognitive_complexity)]
fn default_gradient_stops(solid: &str) -> Vec<coursecraft_course::GradientStop> {
    let start = if solid.trim().is_empty() {
        "#D0D7DE"
    } else {
        solid.trim()
    };
    vec![
        coursecraft_course::GradientStop {
            color: Color(start.to_owned()),
            position: 0.0,
        },
        coursecraft_course::GradientStop {
            color: Color("#FFFFFF".to_owned()),
            position: 1.0,
        },
    ]
}

fn fill_style_kind(style: &Option<coursecraft_course::FillStyle>) -> &'static str {
    match style {
        None => "Solid",
        Some(coursecraft_course::FillStyle::Linear { .. }) => "Linear gradient",
        Some(coursecraft_course::FillStyle::Radial { .. }) => "Radial gradient",
        Some(coursecraft_course::FillStyle::Picture { .. }) => "Picture",
    }
}

fn fill_style_json(style: &Option<coursecraft_course::FillStyle>) -> Json {
    style
        .as_ref()
        .and_then(|style| serde_json::to_value(style).ok())
        .unwrap_or(Json::Null)
}

/// Fill-type picker, gradient stop editor and picture chooser for shapes and buttons.
#[allow(clippy::too_many_arguments)]
fn show_fill_style_editor(
    ui: &mut Ui,
    props: &mut PropertiesState,
    solid: &str,
    slide_id: &str,
    object_id: &str,
    media_names: &[String],
    actions: &mut Vec<(&'static str, Json)>,
) {
    let mut changed = false;
    let mut next_style: Option<coursecraft_course::FillStyle> = props.fill_style.clone();

    inspector_row(ui, "Fill type", |ui| {
        let rem_w = ui.available_width().max(30.0);
        let current = fill_style_kind(&props.fill_style);
        egui::ComboBox::from_id_salt("prop_fill_type")
            .width(rem_w)
            .selected_text(current)
            .show_ui(ui, |ui| {
                for label in ["Solid", "Linear gradient", "Radial gradient", "Picture"] {
                    if ui.selectable_label(current == label, label).clicked() && current != label {
                        next_style = match label {
                            "Solid" => None,
                            "Linear gradient" => Some(coursecraft_course::FillStyle::Linear {
                                angle: 0.0,
                                stops: default_gradient_stops(solid),
                            }),
                            "Radial gradient" => Some(coursecraft_course::FillStyle::Radial {
                                center: [0.5, 0.5],
                                stops: default_gradient_stops(solid),
                            }),
                            _ => Some(coursecraft_course::FillStyle::Picture {
                                src: media_names.first().cloned().unwrap_or_default(),
                                mode: coursecraft_course::PictureFillMode::Stretch,
                            }),
                        };
                        changed = true;
                    }
                }
            });
    });

    match &mut next_style {
        Some(coursecraft_course::FillStyle::Linear { angle, stops }) => {
            inspector_row(ui, "Angle", |ui| {
                let rem_w = ui.available_width().max(30.0);
                let r = add_drag_left(
                    ui,
                    [rem_w, coursecraft_render::CONTROL_COMPACT],
                    DragValue::new(angle)
                        .speed(1.0)
                        .range(0.0..=360.0)
                        .suffix("°"),
                );
                let enter = r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if r.drag_stopped() || r.lost_focus() || enter {
                    changed = true;
                }
            });
            show_gradient_stops(ui, stops, &mut changed, &mut props.fill_add_stop_rect);
        }
        Some(coursecraft_course::FillStyle::Radial { center, stops }) => {
            inspector_row(ui, "Centre X", |ui| {
                let rem_w = ui.available_width().max(30.0);
                let r = add_drag_left(
                    ui,
                    [rem_w, coursecraft_render::CONTROL_COMPACT],
                    DragValue::new(&mut center[0]).speed(0.01).range(0.0..=1.0),
                );
                let enter = r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if r.drag_stopped() || r.lost_focus() || enter {
                    changed = true;
                }
            });
            inspector_row(ui, "Centre Y", |ui| {
                let rem_w = ui.available_width().max(30.0);
                let r = add_drag_left(
                    ui,
                    [rem_w, coursecraft_render::CONTROL_COMPACT],
                    DragValue::new(&mut center[1]).speed(0.01).range(0.0..=1.0),
                );
                let enter = r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if r.drag_stopped() || r.lost_focus() || enter {
                    changed = true;
                }
            });
            show_gradient_stops(ui, stops, &mut changed, &mut props.fill_add_stop_rect);
        }
        Some(coursecraft_course::FillStyle::Picture { src, mode }) => {
            inspector_row(ui, "Image", |ui| {
                let rem_w = ui.available_width().max(30.0);
                let selected = if src.is_empty() {
                    "Choose an image".to_owned()
                } else {
                    src.clone()
                };
                egui::ComboBox::from_id_salt("prop_picture_src")
                    .width(rem_w)
                    .selected_text(selected)
                    .show_ui(ui, |ui| {
                        for name in media_names {
                            if ui.selectable_label(src == name, name).clicked() && src != name {
                                *src = name.clone();
                                changed = true;
                            }
                        }
                    });
            });
            inspector_row(ui, "Picture fit", |ui| {
                let rem_w = ui.available_width().max(30.0);
                egui::ComboBox::from_id_salt("prop_picture_mode")
                    .width(rem_w)
                    .selected_text(match mode {
                        coursecraft_course::PictureFillMode::Stretch => "Stretch",
                        coursecraft_course::PictureFillMode::Tile => "Tile",
                    })
                    .show_ui(ui, |ui| {
                        for (value, label) in [
                            (coursecraft_course::PictureFillMode::Stretch, "Stretch"),
                            (coursecraft_course::PictureFillMode::Tile, "Tile"),
                        ] {
                            if ui.selectable_label(*mode == value, label).clicked()
                                && *mode != value
                            {
                                *mode = value;
                                changed = true;
                            }
                        }
                    });
            });
        }
        None => {}
    }

    if changed {
        props.fill_style = next_style;
        actions.push((
            "object.set_fill_style",
            serde_json::json!({
                "slide": slide_id,
                "object": object_id,
                "fill_style": fill_style_json(&props.fill_style),
            }),
        ));
    }
}

fn show_gradient_stops(
    ui: &mut Ui,
    stops: &mut Vec<coursecraft_course::GradientStop>,
    changed: &mut bool,
    add_stop_rect: &mut Option<Rect>,
) {
    let mut remove: Option<usize> = None;
    let mut add = false;
    let len = stops.len();
    let can_remove = len > 2;
    for i in 0..len {
        let Some(stop) = stops.get_mut(i) else {
            continue;
        };
        ui.push_id(("fill_stop", i), |ui| {
            inspector_row(ui, &format!("Stop {}", i + 1), |ui| {
                let (color_response, valid) =
                    show_color_field(ui, "", &mut stop.color.0, "prop_stop_color");
                if is_committed(&color_response, ui) && valid {
                    *changed = true;
                }
                let mut percent = stop.position * 100.0;
                let response = ui.add(
                    DragValue::new(&mut percent)
                        .speed(1.0)
                        .range(0.0..=100.0)
                        .suffix("%"),
                );
                let enter = response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if response.drag_stopped() || response.lost_focus() || enter {
                    stop.position = (percent / 100.0).clamp(0.0, 1.0);
                    *changed = true;
                }
                if can_remove && ui.small_button("×").clicked() {
                    remove = Some(i);
                }
            });
        });
    }
    if let Some(index) = remove
        && stops.len() > 2
    {
        stops.remove(index);
        *changed = true;
    }
    if stops.len() < coursecraft_course::MAX_GRADIENT_STOPS {
        let response = ui.button("Add stop");
        *add_stop_rect = Some(response.rect);
        if response.clicked() {
            add = true;
        }
    }
    if add && stops.len() < coursecraft_course::MAX_GRADIENT_STOPS {
        let (position, color) = match stops.last() {
            Some(last) => (
                ((last.position + 1.0) * 0.5).clamp(0.0, 1.0),
                last.color.clone(),
            ),
            None => (0.5, Color("#D0D7DE".to_owned())),
        };
        stops.push(coursecraft_course::GradientStop { color, position });
        *changed = true;
    }
    stops.sort_by(|a, b| a.position.total_cmp(&b.position));
}

/// Shadow and glow toggles with their values.
fn show_effect_controls(
    ui: &mut Ui,
    props: &mut PropertiesState,
    slide_id: &str,
    object_id: &str,
    actions: &mut Vec<(&'static str, Json)>,
) {
    let mut shadow_changed = false;
    let mut glow_changed = false;
    inspector_separator(ui);
    inspector_section_header(ui, "Effects");

    inspector_row(ui, "Shadow", |ui| {
        if ui.checkbox(&mut props.shadow_enabled, "").changed() {
            shadow_changed = true;
        }
    });
    if props.shadow_enabled {
        inspector_row(ui, "Colour", |ui| {
            let (response, valid) =
                show_color_field(ui, "", &mut props.shadow.color.0, "prop_shadow_color");
            if is_committed(&response, ui) && valid {
                shadow_changed = true;
            }
        });
        for (label, value, range) in [
            ("Offset X", &mut props.shadow.offset_x, -200.0..=200.0),
            ("Offset Y", &mut props.shadow.offset_y, -200.0..=200.0),
            ("Blur", &mut props.shadow.blur, 0.0..=100.0),
        ] {
            inspector_row(ui, label, |ui| {
                let rem_w = ui.available_width().max(30.0);
                let response = add_drag_left(
                    ui,
                    [rem_w, coursecraft_render::CONTROL_COMPACT],
                    DragValue::new(value).speed(0.5).range(range),
                );
                let enter = response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if response.drag_stopped() || response.lost_focus() || enter {
                    shadow_changed = true;
                }
            });
        }
    }

    inspector_row(ui, "Glow", |ui| {
        if ui.checkbox(&mut props.glow_enabled, "").changed() {
            glow_changed = true;
        }
    });
    if props.glow_enabled {
        inspector_row(ui, "Colour", |ui| {
            let (response, valid) =
                show_color_field(ui, "", &mut props.glow.color.0, "prop_glow_color");
            if is_committed(&response, ui) && valid {
                glow_changed = true;
            }
        });
        inspector_row(ui, "Size", |ui| {
            let rem_w = ui.available_width().max(30.0);
            let response = add_drag_left(
                ui,
                [rem_w, coursecraft_render::CONTROL_COMPACT],
                DragValue::new(&mut props.glow.size)
                    .speed(0.5)
                    .range(0.0..=100.0),
            );
            let enter = response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if response.drag_stopped() || response.lost_focus() || enter {
                glow_changed = true;
            }
        });
    }

    if shadow_changed {
        let shadow = if props.shadow_enabled {
            serde_json::to_value(&props.shadow).unwrap_or(Json::Null)
        } else {
            Json::Null
        };
        actions.push((
            "object.set_shadow",
            serde_json::json!({
                "slide": slide_id,
                "object": object_id,
                "shadow": shadow,
            }),
        ));
    }
    if glow_changed {
        let glow = if props.glow_enabled {
            serde_json::to_value(&props.glow).unwrap_or(Json::Null)
        } else {
            Json::Null
        };
        actions.push((
            "object.set_glow",
            serde_json::json!({
                "slide": slide_id,
                "object": object_id,
                "glow": glow,
            }),
        ));
    }
}

fn show_drag_controls(
    ui: &mut Ui,
    props: &mut PropertiesState,
    slide: &coursecraft_course::Slide,
    obj: &coursecraft_course::Object,
    actions: &mut Vec<(&'static str, Json)>,
) {
    // Question widgets are not allowed to be draggable
    let is_question_widget = slide.question.as_ref().is_some_and(|q| match q.kind {
        coursecraft_course::QuestionKind::Hotspot => q.hotspot_image.as_deref() == Some(&obj.id),
        coursecraft_course::QuestionKind::DragDrop => {
            q.drag_items.iter().any(|d| d.item == obj.id)
                || q.drop_targets.iter().any(|t| t == &obj.id)
        }
        _ => false,
    });
    if is_question_widget {
        return;
    }

    let mut drag_changed = false;
    inspector_separator(ui);
    inspector_section_header(ui, "Draggable");

    inspector_row(ui, "Draggable", |ui| {
        let response = ui.checkbox(&mut props.drag_enabled, "");
        props.drag_toggle_rect = Some(response.rect);
        if response.changed() {
            drag_changed = true;
        }
    });

    if props.drag_enabled {
        inspector_row(ui, "Snap to center", |ui| {
            if ui.checkbox(&mut props.drag.snap, "").changed() {
                drag_changed = true;
            }
        });
        inspector_row(ui, "Return on miss", |ui| {
            if ui.checkbox(&mut props.drag.return_on_miss, "").changed() {
                drag_changed = true;
            }
        });

        // Target list with available objects on the slide.
        inspector_row(ui, "Targets", |ui| {
            ui.vertical(|ui| {
                for candidate in slide
                    .objects
                    .iter()
                    .chain(slide.layers.iter().flat_map(|layer| layer.objects.iter()))
                {
                    if candidate.id == obj.id {
                        continue;
                    }
                    let mut is_target = props.drag.targets.iter().any(|t| t == &candidate.id);
                    let label = if candidate.name.is_empty() {
                        candidate.id.clone()
                    } else {
                        format!("{} ({})", candidate.name, candidate.id)
                    };
                    if ui.checkbox(&mut is_target, label).changed() {
                        if is_target {
                            if !props.drag.targets.iter().any(|t| t == &candidate.id) {
                                props.drag.targets.push(candidate.id.clone());
                            }
                        } else {
                            props.drag.targets.retain(|t| t != &candidate.id);
                        }
                        drag_changed = true;
                    }
                }
            });
        });
    }

    if drag_changed {
        let drag_val = if props.drag_enabled {
            serde_json::to_value(&props.drag).unwrap_or(Json::Null)
        } else {
            Json::Null
        };
        actions.push((
            "object.set_drag",
            serde_json::json!({
                "slide": slide.id,
                "object": obj.id,
                "drag": drag_val,
            }),
        ));
    }
}

fn commit_web_folder(
    props: &mut PropertiesState,
    slide_id: &str,
    object_id: &str,
    current: &coursecraft_course::WebSource,
    action_to_run: &mut Option<(&'static str, Json)>,
) {
    let path = props.web_path.trim();
    if path.is_empty() {
        return;
    }
    let entry = props.web_entry.trim();
    let entry = if entry.is_empty() {
        coursecraft_course::WEB_DEFAULT_ENTRY
    } else {
        entry
    };
    let unchanged = matches!(
        current,
        coursecraft_course::WebSource::Folder { path: old_path, entry: old_entry }
            if old_path == path && old_entry == entry
    );
    if unchanged {
        return;
    }
    *action_to_run = Some((
        "object.set_web_source",
        serde_json::json!({
            "slide": slide_id,
            "object": object_id,
            "source": { "type": "folder", "path": path, "entry": entry },
        }),
    ));
}

#[allow(clippy::too_many_arguments)]
fn show_table_controls(
    ui: &mut Ui,
    props: &mut PropertiesState,
    slide_id: &str,
    object_id: &str,
    columns: &[f32],
    rows: &[coursecraft_course::TableRow],
    borders: &coursecraft_course::TableBorders,
    header_row: bool,
    selected_cell: Option<(usize, usize)>,
    font_families: &[String],
    slides: &[(String, String)],
    actions: &mut Vec<(&'static str, Json)>,
) {
    if props.table_cell != selected_cell {
        props.table_cell = selected_cell;
        if let Some((row, column)) = selected_cell
            && let Some(cell) = rows.get(row).and_then(|line| line.cells.get(column))
        {
            props.text_body = cell.text.clone();
            props.text_size = cell.style.size;
            props.text_color = cell.style.color.0.clone();
            props.text_bold = cell.style.bold;
            props.text_italic = cell.style.italic;
            props.text_underline = cell.style.underline;
            props.text_align = cell.style.align;
            props.text_valign = cell.valign;
            props.table_cell_fill = cell
                .fill
                .as_ref()
                .map_or_else(|| "#FFFFFF".into(), |fill| fill.0.clone());
        }
    }

    inspector_row(ui, "Header row", |ui| {
        let mut checked = header_row;
        if ui.checkbox(&mut checked, "").changed() {
            actions.push((
                "table.set_header_row",
                serde_json::json!({
                    "slide": slide_id,
                    "object": object_id,
                    "header_row": checked,
                }),
            ));
        }
    });

    inspector_separator(ui);
    inspector_section_header(ui, "Rows");
    let row = selected_cell.map(|(row, _)| row).unwrap_or(rows.len());
    inspector_row(ui, "Row", |ui| {
        ui.label(format!(
            "{} of {}",
            row.saturating_add(1).min(rows.len().max(1)),
            rows.len()
        ));
    });
    ui.horizontal_wrapped(|ui| {
        if ui.button("Insert above").clicked() {
            actions.push((
                "table.insert_row",
                serde_json::json!({
                    "slide": slide_id,
                    "object": object_id,
                    "index": row.min(rows.len()),
                }),
            ));
        }
        if ui.button("Insert below").clicked() {
            actions.push((
                "table.insert_row",
                serde_json::json!({
                    "slide": slide_id,
                    "object": object_id,
                    "index": row.saturating_add(1).min(rows.len()),
                }),
            ));
        }
        if ui
            .add_enabled(rows.len() > 1, egui::Button::new("Delete row"))
            .clicked()
        {
            actions.push((
                "table.delete_row",
                serde_json::json!({
                    "slide": slide_id,
                    "object": object_id,
                    "index": row.min(rows.len().saturating_sub(1)),
                }),
            ));
        }
    });

    inspector_separator(ui);
    inspector_section_header(ui, "Columns");
    let column = selected_cell
        .map(|(_, column)| column)
        .unwrap_or(columns.len());
    inspector_row(ui, "Column", |ui| {
        ui.label(format!(
            "{} of {}",
            column.saturating_add(1).min(columns.len().max(1)),
            columns.len()
        ));
    });
    ui.horizontal_wrapped(|ui| {
        if ui.button("Insert left").clicked() {
            actions.push((
                "table.insert_column",
                serde_json::json!({
                    "slide": slide_id,
                    "object": object_id,
                    "index": column.min(columns.len()),
                }),
            ));
        }
        if ui.button("Insert right").clicked() {
            actions.push((
                "table.insert_column",
                serde_json::json!({
                    "slide": slide_id,
                    "object": object_id,
                    "index": column.saturating_add(1).min(columns.len()),
                }),
            ));
        }
        if ui
            .add_enabled(columns.len() > 1, egui::Button::new("Delete column"))
            .clicked()
        {
            actions.push((
                "table.delete_column",
                serde_json::json!({
                    "slide": slide_id,
                    "object": object_id,
                    "index": column.min(columns.len().saturating_sub(1)),
                }),
            ));
        }
    });

    inspector_separator(ui);
    if let Some((row, column)) = selected_cell
        && let Some(cell) = rows.get(row).and_then(|line| line.cells.get(column))
    {
        inspector_section_header(ui, "Cell");
        inspector_row(ui, "Fill", |ui| {
            let (response, valid_color) =
                show_color_field(ui, "", &mut props.table_cell_fill, "prop_table_cell_fill");
            props.color_rect = Some(response.rect);
            if is_committed(&response, ui) && valid_color {
                actions.push((
                    "table.set_cell",
                    serde_json::json!({
                        "slide": slide_id,
                        "object": object_id,
                        "row": row,
                        "column": column,
                        "fill": props.table_cell_fill.trim(),
                    }),
                ));
            }
        });
        if let Some(action) = show_text_controls(
            ui,
            props,
            slide_id,
            object_id,
            &cell.style,
            &cell.runs,
            &cell.text,
            "table_cell",
            font_families,
            Some((row, column)),
            None,
        ) {
            actions.push(action);
        }
        if let Some(action) = show_paragraph_controls(
            ui,
            props,
            slide_id,
            object_id,
            &cell.runs,
            &cell.paragraphs,
            slides,
            &cell.text,
            "table_cell",
            Some((row, column)),
        ) {
            actions.push(action);
        }
    } else {
        inspector_row(ui, "Cell", |ui| {
            ui.label(
                RichText::new("Click a cell to edit its text and fill")
                    .color(coursecraft_render::palette(ui.ctx()).text_secondary)
                    .size(coursecraft_render::SIZE_SMALL),
            );
        });
    }

    inspector_separator(ui);
    inspector_section_header(ui, "Borders");
    let border_json = |enabled: bool, color: &str, width: f32| -> Json {
        if enabled {
            serde_json::json!({ "color": color.trim(), "width": width })
        } else {
            Json::Null
        }
    };
    inspector_row(ui, "Outer", |ui| {
        if ui.checkbox(&mut props.table_outer_enabled, "").changed() {
            actions.push((
                "table.set_borders",
                serde_json::json!({
                    "slide": slide_id,
                    "object": object_id,
                    "outer": border_json(
                        props.table_outer_enabled,
                        &props.table_outer_color,
                        props.table_outer_width,
                    ),
                }),
            ));
        }
        if props.table_outer_enabled {
            let (response, valid_color) = show_color_field(
                ui,
                "",
                &mut props.table_outer_color,
                "prop_table_outer_color",
            );
            props.color_rect = Some(response.rect);
            if is_committed(&response, ui) && valid_color {
                actions.push((
                    "table.set_borders",
                    serde_json::json!({
                        "slide": slide_id,
                        "object": object_id,
                        "outer": {
                            "color": props.table_outer_color.trim(),
                            "width": props.table_outer_width,
                        },
                    }),
                ));
            }
        }
    });
    if props.table_outer_enabled {
        inspector_row(ui, "Outer width", |ui| {
            let response = ui.add(
                DragValue::new(&mut props.table_outer_width)
                    .speed(0.25)
                    .range(0.0..=50.0)
                    .suffix(" px"),
            );
            if response.drag_stopped() || response.lost_focus() {
                actions.push((
                    "table.set_borders",
                    serde_json::json!({
                        "slide": slide_id,
                        "object": object_id,
                        "outer": {
                            "color": props.table_outer_color.trim(),
                            "width": props.table_outer_width,
                        },
                    }),
                ));
            }
        });
    }
    inspector_row(ui, "Inner", |ui| {
        if ui.checkbox(&mut props.table_inner_enabled, "").changed() {
            actions.push((
                "table.set_borders",
                serde_json::json!({
                    "slide": slide_id,
                    "object": object_id,
                    "inner": border_json(
                        props.table_inner_enabled,
                        &props.table_inner_color,
                        props.table_inner_width,
                    ),
                }),
            ));
        }
        if props.table_inner_enabled {
            let (response, valid_color) = show_color_field(
                ui,
                "",
                &mut props.table_inner_color,
                "prop_table_inner_color",
            );
            props.color_rect = Some(response.rect);
            if is_committed(&response, ui) && valid_color {
                actions.push((
                    "table.set_borders",
                    serde_json::json!({
                        "slide": slide_id,
                        "object": object_id,
                        "inner": {
                            "color": props.table_inner_color.trim(),
                            "width": props.table_inner_width,
                        },
                    }),
                ));
            }
        }
    });
    if props.table_inner_enabled {
        inspector_row(ui, "Inner width", |ui| {
            let response = ui.add(
                DragValue::new(&mut props.table_inner_width)
                    .speed(0.25)
                    .range(0.0..=50.0)
                    .suffix(" px"),
            );
            if response.drag_stopped() || response.lost_focus() {
                actions.push((
                    "table.set_borders",
                    serde_json::json!({
                        "slide": slide_id,
                        "object": object_id,
                        "inner": {
                            "color": props.table_inner_color.trim(),
                            "width": props.table_inner_width,
                        },
                    }),
                ));
            }
        });
    }
    let _ = borders;
}

#[allow(clippy::cognitive_complexity)]
/// Build the `object.set_input` patch for an input's current inspector buffers.
pub(crate) fn input_patch(kind: &ObjectKind, props: &PropertiesState) -> Json {
    match kind {
        ObjectKind::TextEntry { .. } => serde_json::json!({
            "variable": props.input_variable,
            "placeholder": props.input_placeholder,
            "multiline": props.input_multiline,
            "max_chars": props.input_max_chars,
        }),
        ObjectKind::Slider { .. } => serde_json::json!({
            "variable": props.input_variable,
            "min": props.input_min,
            "max": props.input_max,
            "step": props.input_step,
            "orientation": props.input_orientation,
        }),
        ObjectKind::Dial { .. } => serde_json::json!({
            "variable": props.input_variable,
            "min": props.input_min,
            "max": props.input_max,
            "step": props.input_step,
            "start_angle": props.input_start_angle,
            "end_angle": props.input_end_angle,
        }),
        ObjectKind::Checkbox { .. } => serde_json::json!({
            "variable": props.input_variable,
            "label": props.input_label,
        }),
        ObjectKind::RadioGroup { .. } => serde_json::json!({
            "variable": props.input_variable,
            "options": props.input_options,
        }),
        _ => Json::Null,
    }
}

fn unique_input_variable_name(course: &Course, kind: &ObjectKind, object_name: &str) -> String {
    let base: String = object_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let base = if base.trim_matches('_').is_empty() {
        match kind {
            ObjectKind::TextEntry { .. } => "text_entry",
            ObjectKind::Slider { .. } => "slider",
            ObjectKind::Dial { .. } => "dial",
            ObjectKind::Checkbox { .. } => "checkbox",
            ObjectKind::RadioGroup { .. } => "radio",
            _ => "input",
        }
        .to_owned()
    } else {
        base
    };
    if course.variable(&base).is_none() {
        return base;
    }
    (2u64..)
        .map(|n| format!("{base}{n}"))
        .find(|name| course.variable(name).is_none())
        .unwrap_or_else(|| format!("{base}2"))
}

fn default_input_value(kind: &ObjectKind) -> coursecraft_course::Value {
    match kind {
        ObjectKind::Slider { min, .. } | ObjectKind::Dial { min, .. } => {
            coursecraft_course::Value::Number(*min)
        }
        ObjectKind::Checkbox { .. } => coursecraft_course::Value::Bool(false),
        _ => coursecraft_course::Value::Text(String::new()),
    }
}

fn commit_input_patch(
    props: &PropertiesState,
    kind: &ObjectKind,
    slide_id: &str,
    object_id: &str,
    action_to_run: &mut Option<(&'static str, Json)>,
) {
    *action_to_run = Some((
        "object.set_input",
        serde_json::json!({
            "slide": slide_id,
            "object": object_id,
            "patch": input_patch(kind, props),
        }),
    ));
}

#[allow(clippy::too_many_arguments)]
fn show_input_controls(
    ui: &mut Ui,
    props: &mut PropertiesState,
    course: &Course,
    slide_id: &str,
    object_id: &str,
    object_name: &str,
    kind: &ObjectKind,
    actions: &mut Vec<(&'static str, Json)>,
) {
    let required = kind
        .input_variable()
        .map(|(_, value_kind)| value_kind)
        .unwrap_or(coursecraft_course::ValueKind::Text);
    let mut action_to_run: Option<(&'static str, Json)> = None;
    let mut create_variable = false;

    inspector_row(ui, "Variable", |ui| {
        let rem_w = ui.available_width().max(30.0);
        let label = if props.input_variable.is_empty() {
            "Choose…".to_owned()
        } else {
            props.input_variable.clone()
        };
        let combo = egui::ComboBox::from_id_salt("prop_input_variable")
            .selected_text(label)
            .width(rem_w)
            .show_ui(ui, |ui| {
                for variable in &course.variables {
                    if variable.value.kind() != required {
                        continue;
                    }
                    if ui
                        .selectable_value(
                            &mut props.input_variable,
                            variable.name.clone(),
                            &variable.name,
                        )
                        .clicked()
                    {
                        commit_input_patch(props, kind, slide_id, object_id, &mut action_to_run);
                    }
                }
                ui.separator();
                if ui.button("New variable…").clicked() {
                    create_variable = true;
                }
            });
        props.input_variable_rect = Some(combo.response.rect);
    });
    if create_variable {
        let name = unique_input_variable_name(course, kind, object_name);
        let value = default_input_value(kind);
        let mut patch = input_patch(kind, props);
        patch["variable"] = Json::String(name.clone());
        actions.push((
            "variable.add",
            serde_json::json!({ "name": name, "value": value }),
        ));
        actions.push((
            "object.set_input",
            serde_json::json!({
                "slide": slide_id,
                "object": object_id,
                "patch": patch,
            }),
        ));
        props.input_variable = name;
    }

    match kind {
        ObjectKind::TextEntry { .. } => {
            inspector_row(ui, "Placeholder", |ui| {
                let rem_w = ui.available_width().max(30.0);
                let r = ui.add_sized(
                    [rem_w, coursecraft_render::CONTROL_COMPACT],
                    TextEdit::singleline(&mut props.input_placeholder)
                        .id_salt("prop_input_placeholder"),
                );
                if is_committed(&r, ui) {
                    commit_input_patch(props, kind, slide_id, object_id, &mut action_to_run);
                }
            });
            inspector_row(ui, "Multiline", |ui| {
                if ui.checkbox(&mut props.input_multiline, "").changed() {
                    commit_input_patch(props, kind, slide_id, object_id, &mut action_to_run);
                }
            });
            inspector_row(ui, "Max chars", |ui| {
                let r = ui.add(
                    DragValue::new(&mut props.input_max_chars)
                        .speed(1.0)
                        .range(0..=coursecraft_course::MAX_INPUT_CHARS),
                );
                if r.changed() {
                    commit_input_patch(props, kind, slide_id, object_id, &mut action_to_run);
                }
            });
        }
        ObjectKind::Slider { .. } => {
            show_number_range(ui, props, kind, slide_id, object_id, &mut action_to_run);
            inspector_row(ui, "Direction", |ui| {
                ui.horizontal(|ui| {
                    if ui
                        .selectable_value(
                            &mut props.input_orientation,
                            coursecraft_course::Orientation::Horizontal,
                            "Horizontal",
                        )
                        .clicked()
                    {
                        commit_input_patch(props, kind, slide_id, object_id, &mut action_to_run);
                    }
                    if ui
                        .selectable_value(
                            &mut props.input_orientation,
                            coursecraft_course::Orientation::Vertical,
                            "Vertical",
                        )
                        .clicked()
                    {
                        commit_input_patch(props, kind, slide_id, object_id, &mut action_to_run);
                    }
                });
            });
        }
        ObjectKind::Dial { .. } => {
            show_number_range(ui, props, kind, slide_id, object_id, &mut action_to_run);
            inspector_row(ui, "Start angle", |ui| {
                let r = ui.add(
                    DragValue::new(&mut props.input_start_angle)
                        .speed(1.0)
                        .range(-360.0..=360.0)
                        .suffix("°"),
                );
                if r.changed() {
                    commit_input_patch(props, kind, slide_id, object_id, &mut action_to_run);
                }
            });
            inspector_row(ui, "End angle", |ui| {
                let r = ui.add(
                    DragValue::new(&mut props.input_end_angle)
                        .speed(1.0)
                        .range(-360.0..=360.0)
                        .suffix("°"),
                );
                if r.changed() {
                    commit_input_patch(props, kind, slide_id, object_id, &mut action_to_run);
                }
            });
        }
        ObjectKind::Checkbox { .. } => {
            inspector_row(ui, "Label", |ui| {
                let rem_w = ui.available_width().max(30.0);
                let r = ui.add_sized(
                    [rem_w, coursecraft_render::CONTROL_COMPACT],
                    TextEdit::singleline(&mut props.input_label).id_salt("prop_input_label"),
                );
                if is_committed(&r, ui) {
                    commit_input_patch(props, kind, slide_id, object_id, &mut action_to_run);
                }
            });
        }
        ObjectKind::RadioGroup { .. } => {
            show_radio_options(ui, props, kind, slide_id, object_id, &mut action_to_run);
        }
        _ => {}
    }
    if let Some(action) = action_to_run {
        actions.push(action);
    }
}

fn show_marker_controls(
    ui: &mut Ui,
    props: &mut PropertiesState,
    slide_id: &str,
    object_id: &str,
    actions: &mut Vec<(&'static str, Json)>,
) {
    inspector_row(ui, "Title", |ui| {
        let rem_w = ui.available_width().max(30.0);
        let r = ui.add_sized(
            [rem_w, coursecraft_render::CONTROL_COMPACT],
            TextEdit::singleline(&mut props.marker_title).id_salt("prop_marker_title"),
        );
        props.marker_title_rect = Some(r.rect);
        if is_committed(&r, ui) {
            actions.push((
                "object.set_marker",
                serde_json::json!({
                    "slide": slide_id,
                    "object": object_id,
                    "patch": { "title": props.marker_title },
                }),
            ));
        }
    });
    inspector_row(ui, "Body", |ui| {
        let rem_w = ui.available_width().max(30.0);
        let r = ui.add_sized(
            [rem_w, coursecraft_render::CONTROL_COMPACT * 2.0],
            TextEdit::multiline(&mut props.marker_body).id_salt("prop_marker_body"),
        );
        if r.lost_focus() {
            actions.push((
                "object.set_marker",
                serde_json::json!({
                    "slide": slide_id,
                    "object": object_id,
                    "patch": { "body": props.marker_body },
                }),
            ));
        }
    });
    inspector_row(ui, "Icon", |ui| {
        props.marker_icon_rects.clear();
        ui.horizontal_wrapped(|ui| {
            for (icon, label) in [
                (coursecraft_course::MarkerIcon::Info, "Info"),
                (coursecraft_course::MarkerIcon::Plus, "Plus"),
                (coursecraft_course::MarkerIcon::Question, "Question"),
                (coursecraft_course::MarkerIcon::Star, "Star"),
                (coursecraft_course::MarkerIcon::Pin, "Pin"),
            ] {
                let selected = props.marker_icon == icon;
                let response = ui.selectable_label(selected, label);
                props.marker_icon_rects.push(response.rect);
                if response.clicked() {
                    props.marker_icon = icon;
                    let icon = serde_json::to_value(icon).unwrap_or(Json::Null);
                    actions.push((
                        "object.set_marker",
                        serde_json::json!({
                            "slide": slide_id,
                            "object": object_id,
                            "patch": { "icon": icon },
                        }),
                    ));
                }
            }
        });
    });
    inspector_row(ui, "Theme colour", |ui| {
        if ui
            .checkbox(&mut props.marker_use_theme_color, "Use theme accent")
            .changed()
        {
            let color = if props.marker_use_theme_color {
                Json::Null
            } else {
                Json::String(props.marker_color.clone())
            };
            actions.push((
                "object.set_marker",
                serde_json::json!({
                    "slide": slide_id,
                    "object": object_id,
                    "patch": { "color": color },
                }),
            ));
        }
    });
    if !props.marker_use_theme_color {
        let (response, valid) =
            show_color_field(ui, "Colour", &mut props.marker_color, "prop_marker_color");
        if is_committed(&response, ui) && valid {
            actions.push((
                "object.set_marker",
                serde_json::json!({
                    "slide": slide_id,
                    "object": object_id,
                    "patch": { "color": props.marker_color },
                }),
            ));
        }
    }
}

/// The `object.set_timer` patch for a timer's current inspector buffers.
pub(crate) fn timer_patch(props: &PropertiesState) -> Json {
    serde_json::json!({
        "duration_ms": props.timer_duration_ms,
        "format": props.timer_format,
        "autostart": props.timer_autostart,
        "style": {
            "font": props.timer_font,
            "size": props.text_size,
            "color": props.text_color,
            "bold": props.text_bold,
            "italic": props.text_italic,
            "underline": props.text_underline,
            "align": props.text_align,
            "valign": props.text_valign,
        },
    })
}

fn show_timer_controls(
    ui: &mut Ui,
    props: &mut PropertiesState,
    course: &Course,
    slide_id: &str,
    object_id: &str,
    font_families: &[String],
    actions: &mut Vec<(&'static str, Json)>,
) {
    let mut action_to_run: Option<(&'static str, Json)> = None;
    let commit = |props: &PropertiesState| -> (&'static str, Json) {
        (
            "object.set_timer",
            serde_json::json!({
                "slide": slide_id,
                "object": object_id,
                "patch": timer_patch(props),
            }),
        )
    };
    inspector_row(ui, "Duration", |ui| {
        let r = ui.add(
            DragValue::new(&mut props.timer_duration_ms)
                .speed(100.0)
                .range(1..=coursecraft_course::MAX_TIMER_DURATION_MS)
                .suffix(" ms"),
        );
        props.timer_duration_rect = Some(r.rect);
        if r.drag_stopped() || r.lost_focus() {
            action_to_run = Some(commit(props));
        }
    });
    inspector_row(ui, "Format", |ui| {
        ui.horizontal(|ui| {
            if ui
                .selectable_value(&mut props.timer_format, TimerFormat::MmSs, "mm:ss")
                .clicked()
            {
                action_to_run = Some(commit(props));
            }
            if ui
                .selectable_value(&mut props.timer_format, TimerFormat::Seconds, "Seconds")
                .clicked()
            {
                action_to_run = Some(commit(props));
            }
            if ui
                .selectable_value(&mut props.timer_format, TimerFormat::Hidden, "Hidden")
                .clicked()
            {
                action_to_run = Some(commit(props));
            }
        });
    });
    inspector_row(ui, "Autostart", |ui| {
        if ui.checkbox(&mut props.timer_autostart, "").changed() {
            action_to_run = Some(commit(props));
        }
    });
    inspector_row(ui, "Font", |ui| {
        let shown = if props.timer_font.eq_ignore_ascii_case("Open Sans") {
            "Inter".to_owned()
        } else {
            props.timer_font.clone()
        };
        egui::ComboBox::from_id_salt("prop_timer_font")
            .selected_text(shown.clone())
            .show_ui(ui, |ui| {
                for name in std::iter::once("Inter".to_owned()).chain(font_families.iter().cloned())
                {
                    if ui.selectable_label(shown == name, name.clone()).clicked() && shown != name {
                        props.timer_font = name;
                        action_to_run = Some(commit(props));
                        ui.close();
                    }
                }
            });
    });
    inspector_row(ui, "Style", |ui| {
        if ui
            .selectable_label(props.text_bold, RichText::new("B").strong())
            .clicked()
        {
            props.text_bold = !props.text_bold;
            action_to_run = Some(commit(props));
        }
        if ui
            .selectable_label(props.text_italic, RichText::new("I").italics())
            .clicked()
        {
            props.text_italic = !props.text_italic;
            action_to_run = Some(commit(props));
        }
        if ui
            .selectable_label(props.text_underline, RichText::new("U").underline())
            .clicked()
        {
            props.text_underline = !props.text_underline;
            action_to_run = Some(commit(props));
        }
        ui.label("Size");
        let r = ui.add(
            DragValue::new(&mut props.text_size)
                .speed(1.0)
                .range(1.0..=400.0),
        );
        if r.drag_stopped() || r.lost_focus() {
            action_to_run = Some(commit(props));
        }
    });
    let (cr, valid_color) =
        show_color_field(ui, "Color", &mut props.text_color, "prop_timer_color");
    props.color_rect = Some(cr.rect);
    if is_committed(&cr, ui) && valid_color {
        action_to_run = Some(commit(props));
    }
    inspector_row(ui, "Align", |ui| {
        ui.horizontal(|ui| {
            for (align, name) in [
                (Align::Left, "Left"),
                (Align::Center, "Center"),
                (Align::Right, "Right"),
            ] {
                if ui
                    .selectable_value(&mut props.text_align, align, name)
                    .clicked()
                {
                    action_to_run = Some(commit(props));
                }
            }
        });
    });
    inspector_row(ui, "Valign", |ui| {
        ui.horizontal(|ui| {
            for (valign, name) in [
                (VAlign::Top, "Top"),
                (VAlign::Middle, "Middle"),
                (VAlign::Bottom, "Bottom"),
            ] {
                if ui
                    .selectable_value(&mut props.text_valign, valign, name)
                    .clicked()
                {
                    action_to_run = Some(commit(props));
                }
            }
        });
    });
    let _ = course;
    if let Some(action) = action_to_run {
        actions.push(action);
    }
}

/// The `object.set_meter` patch for a meter's current inspector buffers.
pub(crate) fn meter_patch(props: &PropertiesState) -> Json {
    serde_json::json!({
        "variable": props.input_variable,
        "min": props.input_min,
        "max": props.input_max,
        "style": props.meter_style,
        "show_value": props.meter_show_value,
    })
}

fn show_meter_controls(
    ui: &mut Ui,
    props: &mut PropertiesState,
    course: &Course,
    slide_id: &str,
    object_id: &str,
    actions: &mut Vec<(&'static str, Json)>,
) {
    let mut action_to_run: Option<(&'static str, Json)> = None;
    let commit = |props: &PropertiesState| -> (&'static str, Json) {
        (
            "object.set_meter",
            serde_json::json!({
                "slide": slide_id,
                "object": object_id,
                "patch": meter_patch(props),
            }),
        )
    };
    let mut create_variable = false;
    inspector_row(ui, "Variable", |ui| {
        let rem_w = ui.available_width().max(30.0);
        let label = if props.input_variable.is_empty() {
            "Choose…".to_owned()
        } else {
            props.input_variable.clone()
        };
        let combo = egui::ComboBox::from_id_salt("prop_meter_variable")
            .selected_text(label)
            .width(rem_w)
            .show_ui(ui, |ui| {
                for variable in &course.variables {
                    if variable.value.kind() != coursecraft_course::ValueKind::Number {
                        continue;
                    }
                    if ui
                        .selectable_value(
                            &mut props.input_variable,
                            variable.name.clone(),
                            &variable.name,
                        )
                        .clicked()
                    {
                        action_to_run = Some(commit(props));
                    }
                }
                ui.separator();
                if ui.button("New variable…").clicked() {
                    create_variable = true;
                }
            });
        props.input_variable_rect = Some(combo.response.rect);
    });
    if create_variable {
        let name = unique_input_variable_name(
            course,
            &ObjectKind::ProgressMeter {
                variable: String::new(),
                min: 0.0,
                max: 100.0,
                style: coursecraft_course::MeterStyle::Bar,
                show_value: false,
            },
            "Progress meter",
        );
        let mut patch = meter_patch(props);
        patch["variable"] = Json::String(name.clone());
        actions.push((
            "variable.add",
            serde_json::json!({ "name": name, "value": 0.0 }),
        ));
        actions.push((
            "object.set_meter",
            serde_json::json!({
                "slide": slide_id,
                "object": object_id,
                "patch": patch,
            }),
        ));
        props.input_variable = name;
    }
    inspector_row(ui, "Min", |ui| {
        let r = ui.add(DragValue::new(&mut props.input_min).speed(1.0));
        if r.drag_stopped() || r.lost_focus() {
            action_to_run = Some(commit(props));
        }
    });
    inspector_row(ui, "Max", |ui| {
        let r = ui.add(DragValue::new(&mut props.input_max).speed(1.0));
        if r.drag_stopped() || r.lost_focus() {
            action_to_run = Some(commit(props));
        }
    });
    inspector_row(ui, "Style", |ui| {
        ui.horizontal(|ui| {
            if ui
                .selectable_value(
                    &mut props.meter_style,
                    coursecraft_course::MeterStyle::Bar,
                    "Bar",
                )
                .clicked()
            {
                action_to_run = Some(commit(props));
            }
            if ui
                .selectable_value(
                    &mut props.meter_style,
                    coursecraft_course::MeterStyle::Ring,
                    "Ring",
                )
                .clicked()
            {
                action_to_run = Some(commit(props));
            }
        });
    });
    inspector_row(ui, "Show value", |ui| {
        if ui.checkbox(&mut props.meter_show_value, "").changed() {
            action_to_run = Some(commit(props));
        }
    });
    if let Some(action) = action_to_run {
        actions.push(action);
    }
}

fn show_scroll_panel_controls(
    ui: &mut Ui,
    props: &mut PropertiesState,
    slide_id: &str,
    object_id: &str,
    frame_height: f32,
    fit_height: f32,
    actions: &mut Vec<(&'static str, Json)>,
) {
    inspector_row(ui, "Content height", |ui| {
        let r = ui.add(
            DragValue::new(&mut props.scroll_content_height)
                .speed(1.0)
                .range(frame_height..=coursecraft_course::MAX_SCROLL_CONTENT_HEIGHT),
        );
        props.scroll_content_height_rect = Some(r.rect);
        if r.changed() {
            actions.push((
                "object.set_scroll_panel",
                serde_json::json!({
                    "slide": slide_id,
                    "object": object_id,
                    "patch": { "content_height": props.scroll_content_height },
                }),
            ));
        }
    });
    inspector_row(ui, "Scrollbar", |ui| {
        if ui
            .checkbox(&mut props.scroll_bar_visible, "Show scrollbar")
            .changed()
        {
            actions.push((
                "object.set_scroll_panel",
                serde_json::json!({
                    "slide": slide_id,
                    "object": object_id,
                    "patch": { "bar": if props.scroll_bar_visible { "visible" } else { "hidden" } },
                }),
            ));
        }
    });
    if ui.button("Fit to content").clicked() {
        let height = fit_height.clamp(
            frame_height.max(0.0),
            coursecraft_course::MAX_SCROLL_CONTENT_HEIGHT,
        );
        props.scroll_content_height = height;
        actions.push((
            "object.set_scroll_panel",
            serde_json::json!({
                "slide": slide_id,
                "object": object_id,
                "patch": { "content_height": height },
            }),
        ));
    }
}

fn show_panel_picker(
    ui: &mut Ui,
    props: &mut PropertiesState,
    panels: &[(String, String)],
    slide_id: &str,
    object_id: &str,
    object_kind: &ObjectKind,
    actions: &mut Vec<(&'static str, Json)>,
) {
    if matches!(object_kind, ObjectKind::ScrollPanel { .. }) {
        return;
    }
    if panels.is_empty() && props.panel_target.is_empty() {
        return;
    }
    let before = props.panel_target.clone();
    inspector_row(ui, "Inside panel", |ui| {
        let label = if props.panel_target.is_empty() {
            "(None)".to_owned()
        } else {
            panels
                .iter()
                .find(|(id, _)| id == &props.panel_target)
                .map_or_else(|| props.panel_target.clone(), |(_, name)| name.clone())
        };
        let rem_w = ui.available_width().max(30.0);
        let combo = egui::ComboBox::from_id_salt("prop_panel_target")
            .selected_text(label)
            .width(rem_w)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut props.panel_target, String::new(), "(None)");
                for (id, name) in panels {
                    ui.selectable_value(&mut props.panel_target, id.clone(), name);
                }
            });
        props.panel_target_rect = Some(combo.response.rect);
    });
    if props.panel_target != before {
        let panel = if props.panel_target.is_empty() {
            Json::Null
        } else {
            Json::String(props.panel_target.clone())
        };
        actions.push((
            "object.set_panel",
            serde_json::json!({
                "slide": slide_id,
                "object": object_id,
                "panel": panel,
            }),
        ));
    }
}

fn show_number_range(
    ui: &mut Ui,
    props: &mut PropertiesState,
    kind: &ObjectKind,
    slide_id: &str,
    object_id: &str,
    action_to_run: &mut Option<(&'static str, Json)>,
) {
    inspector_row(ui, "Min", |ui| {
        let r = ui.add(DragValue::new(&mut props.input_min).speed(0.1));
        if r.changed() {
            commit_input_patch(props, kind, slide_id, object_id, action_to_run);
        }
    });
    inspector_row(ui, "Max", |ui| {
        let r = ui.add(DragValue::new(&mut props.input_max).speed(0.1));
        if r.changed() {
            commit_input_patch(props, kind, slide_id, object_id, action_to_run);
        }
    });
    inspector_row(ui, "Step", |ui| {
        let r = ui.add(
            DragValue::new(&mut props.input_step)
                .speed(0.1)
                .range(0.0..=1_000_000.0),
        );
        if r.changed() {
            commit_input_patch(props, kind, slide_id, object_id, action_to_run);
        }
    });
}

fn show_radio_options(
    ui: &mut Ui,
    props: &mut PropertiesState,
    kind: &ObjectKind,
    slide_id: &str,
    object_id: &str,
    action_to_run: &mut Option<(&'static str, Json)>,
) {
    let mut remove: Option<usize> = None;
    let mut move_up: Option<usize> = None;
    let mut move_down: Option<usize> = None;
    let mut edited = false;
    props.input_option_rects.clear();
    inspector_section_header(ui, "Options");
    let count = props.input_options.len();
    for (index, option) in props.input_options.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            let label_w = (ui.available_width() - 150.0).max(60.0);
            let r = ui.add_sized(
                [label_w, coursecraft_render::CONTROL_COMPACT],
                TextEdit::singleline(&mut option.label)
                    .id_salt(("prop_input_option_label", index))
                    .hint_text("Label"),
            );
            props.input_option_rects.push(r.rect);
            let mut changed = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if r.lost_focus() {
                changed = true;
            }
            let value_w = 70.0;
            let r = ui.add_sized(
                [value_w, coursecraft_render::CONTROL_COMPACT],
                TextEdit::singleline(&mut option.value)
                    .id_salt(("prop_input_option_value", index))
                    .hint_text("Value"),
            );
            if r.lost_focus() {
                changed = true;
            }
            if changed {
                edited = true;
            }
            if ui
                .add_enabled(index > 0, egui::Button::new("↑").small())
                .clicked()
            {
                move_up = Some(index);
            }
            if ui
                .add_enabled(index + 1 < count, egui::Button::new("↓").small())
                .clicked()
            {
                move_down = Some(index);
            }
            if ui
                .add_enabled(
                    count > coursecraft_course::MIN_RADIO_OPTIONS,
                    egui::Button::new("✕").small(),
                )
                .clicked()
            {
                remove = Some(index);
            }
        });
    }
    // The closure above may have changed the number of options; edit after it.
    if let Some(index) = move_up {
        props.input_options.swap(index, index.saturating_sub(1));
    }
    if let Some(index) = move_down
        && index + 1 < props.input_options.len()
    {
        props.input_options.swap(index, index + 1);
    }
    if let Some(index) = remove
        && props.input_options.len() > coursecraft_course::MIN_RADIO_OPTIONS
    {
        props.input_options.remove(index);
    }
    if edited || move_up.is_some() || move_down.is_some() || remove.is_some() {
        commit_input_patch(props, kind, slide_id, object_id, action_to_run);
    }
    if ui
        .add_enabled(
            props.input_options.len() < coursecraft_course::MAX_RADIO_OPTIONS,
            egui::Button::new("+ Add option"),
        )
        .clicked()
    {
        let mut n = props.input_options.len() + 1;
        let value = loop {
            let candidate = format!("option{n}");
            if !props
                .input_options
                .iter()
                .any(|option| option.value == candidate)
            {
                break candidate;
            }
            n += 1;
        };
        props.input_options.push(coursecraft_course::RadioOption {
            label: format!("Option {n}"),
            value,
        });
        commit_input_patch(props, kind, slide_id, object_id, action_to_run);
    }
}

#[allow(clippy::cognitive_complexity)]
fn show_object_properties(
    editor: &mut Editor,
    ui: &mut Ui,
    actions_to_run: &mut Vec<(&'static str, Json)>,
) {
    let mut action_to_run: Option<(&'static str, Json)> = None;
    let mut replace_image_requested = false;
    let mut replace_audio_requested = false;
    let mut replace_video_requested = false;
    let mut pick_captions_requested = false;
    let mut generate_captions_requested = false;
    let mut reset_natural_size_src: Option<String> = None;
    let mut suggest_alt_requested = false;
    let mut crop_action = CropAction::None;
    let mut edit_points_requested = false;
    let Some(slide) = editor.session.course.slide(&editor.selection.slide) else {
        ui.label("No slide selected");
        return;
    };

    let Some(ref obj_id) = editor.selection.object else {
        return;
    };
    let Some((obj, _)) = slide.object(obj_id) else {
        ui.label("Object not found");
        return;
    };
    let cue_duration_ms = match slide.object(obj_id) {
        Some((_, Some(layer))) => slide
            .layer(layer)
            .map(|layer| layer.duration_ms)
            .unwrap_or(slide.duration_ms),
        _ => slide.duration_ms,
    };
    let media_names: Vec<String> = editor.media.iter().map(|(name, _)| name.clone()).collect();
    let slides: Vec<(String, String)> = editor
        .session
        .course
        .slides()
        .map(|s| (s.id.clone(), s.title.clone()))
        .collect();
    let font_families: Vec<String> = editor
        .session
        .course
        .font_families()
        .into_iter()
        .map(str::to_owned)
        .collect();
    let selected_cell = editor.ui.table_cell;
    let playhead_ms = editor.ui.timeline.playhead_ms;
    // Crop mode needs the decoded picture; look it up before the props borrow.
    let src_loaded = match &obj.kind {
        ObjectKind::Image { src, .. } => is_loaded_image(&editor.media_cache, src),
        _ => false,
    };
    let caption_source_ok = match &obj.kind {
        ObjectKind::Audio { src, .. } | ObjectKind::Video { src, .. } => editor
            .media
            .iter()
            .find(|(name, _)| name == src)
            .is_some_and(|(_, bytes)| bytes.len() <= coursecraft_ai::MAX_AUDIO_UPLOAD),
        _ => false,
    };

    let props = &mut editor.ui.properties;

    let panels: Vec<(String, String)> = {
        let layer = slide.object(obj_id).and_then(|(_, layer)| layer);
        let objects = match layer {
            Some(id) => slide
                .layer(id)
                .map_or(&[][..], |layer| layer.objects.as_slice()),
            None => slide.objects.as_slice(),
        };
        objects
            .iter()
            .filter(|object| matches!(object.kind, ObjectKind::ScrollPanel { .. }))
            .map(|object| {
                (
                    object.id.clone(),
                    if object.name.trim().is_empty() {
                        object.id.clone()
                    } else {
                        object.name.clone()
                    },
                )
            })
            .collect()
    };

    // 1. Read-only ID
    inspector_row(ui, "ID", |ui| {
        ui.label(RichText::new(&obj.id).monospace().strong());
    });

    // 2. Name
    inspector_row(ui, "Name", |ui| {
        let rem_w = ui.available_width().max(30.0);
        let r = ui.add_sized(
            [rem_w, coursecraft_render::CONTROL_COMPACT],
            TextEdit::singleline(&mut props.name).id_salt("prop_obj_name"),
        );
        if is_committed(&r, ui) && props.name != obj.name {
            action_to_run = Some((
                "object.update",
                serde_json::json!({
                    "slide": slide.id,
                    "object": obj.id,
                    "name": props.name.trim(),
                }),
            ));
        }
    });

    inspector_row(ui, "Opacity", |ui| {
        let rem_w = ui.available_width().max(30.0);
        let response = ui.add_sized(
            [rem_w, coursecraft_render::CONTROL_COMPACT],
            egui::Slider::new(&mut props.opacity, 0.0..=1.0)
                .custom_formatter(|value, _| format!("{:.0}%", value * 100.0)),
        );
        props.opacity_rect = Some(response.rect);
        let enter = response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if response.drag_stopped() || response.lost_focus() || enter {
            let opacity = coursecraft_course::clamp_opacity(props.opacity);
            props.opacity = opacity;
            if (opacity - obj.opacity).abs() > 0.001 {
                actions_to_run.push((
                    "object.set_opacity",
                    serde_json::json!({
                        "slide": slide.id,
                        "object": obj.id,
                        "opacity": opacity,
                    }),
                ));
            }
        }
    });

    inspector_separator(ui);

    // 3. Kind-specific fields
    match &obj.kind {
        ObjectKind::Text {
            text,
            style,
            runs,
            autofit,
            ..
        } => {
            if let Some(action) = show_text_controls(
                ui,
                props,
                &slide.id,
                &obj.id,
                style,
                runs,
                text,
                "text",
                &font_families,
                None,
                Some(*autofit),
            ) {
                action_to_run = Some(action);
            }
        }
        ObjectKind::Button {
            label: _,
            fill,
            fill_style,
        } => {
            let fill_style_json = serde_json::to_value(fill_style).unwrap_or(Json::Null);
            inspector_row(ui, "Label", |ui| {
                let rem_w = ui.available_width().max(30.0);
                let r = ui.add_sized(
                    [rem_w, coursecraft_render::CONTROL_COMPACT],
                    TextEdit::singleline(&mut props.button_label).id_salt("prop_btn_label"),
                );
                props.btn_label_rect = Some(r.rect);
                if is_committed(&r, ui) && props.button_label != obj.kind_label().unwrap_or("") {
                    action_to_run = Some((
                        "object.update",
                        serde_json::json!({
                            "slide": slide.id,
                            "object": obj.id,
                            "kind": {
                                "type": "button",
                                "label": props.button_label,
                                "fill": fill.0,
                                "fill_style": fill_style_json.clone(),
                            },
                        }),
                    ));
                }
            });

            inspector_row(ui, "Fill", |ui| {
                let (cr, valid_color) =
                    show_color_field(ui, "", &mut props.button_fill, "prop_btn_fill");
                props.color_rect = Some(cr.rect);
                if is_committed(&cr, ui) && valid_color && props.button_fill.trim() != fill.0 {
                    action_to_run = Some((
                        "object.update",
                        serde_json::json!({
                            "slide": slide.id,
                            "object": obj.id,
                            "kind": {
                                "type": "button",
                                "label": props.button_label,
                                "fill": props.button_fill.trim(),
                                "fill_style": fill_style_json.clone(),
                            },
                        }),
                    ));
                }
            });

            show_fill_style_editor(
                ui,
                props,
                &fill.0,
                &slide.id,
                &obj.id,
                &media_names,
                actions_to_run,
            );
        }
        ObjectKind::Shape {
            shape,
            fill,
            fill_style: _,
            outline: _,
            text,
            adjust,
        } => {
            let base_kind = serde_json::to_value(&obj.kind).unwrap_or(Json::Null);
            let kind_with = |shape: ShapeKind, fill: &str| -> Json {
                let mut kind = base_kind.clone();
                kind["shape"] = serde_json::json!(shape);
                kind["fill"] = serde_json::json!(fill);
                kind
            };
            inspector_row(ui, "Shape", |ui| {
                let rem_w = ui.available_width().max(30.0);
                let response =
                    crate::shape_gallery::shape_picker_button(ui, props.shape_kind, rem_w);
                egui::Popup::menu(&response)
                    .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                    .show(|ui| {
                        let mut picked = None;
                        let output = crate::shape_gallery::show_shape_gallery_selected(
                            ui,
                            &mut props.shape_gallery_query,
                            Some(props.shape_kind),
                            &mut |pick| {
                                if let crate::shape_gallery::GalleryPick::Shape(kind) = pick {
                                    picked = Some(kind);
                                }
                            },
                        );
                        props.shape_gallery_rects = output.icons;
                        props.shape_gallery_search_rect = output.search_rect;
                        if let Some(kind) = picked
                            && kind != props.shape_kind
                        {
                            props.shape_kind = kind;
                            let mut kind_json = kind_with(kind, &fill.0);
                            if !coursecraft_course::shape_allows_text(kind) {
                                kind_json["text"] = Json::Null;
                                kind_json["outline"] = Json::Null;
                            }
                            action_to_run = Some((
                                "object.update",
                                serde_json::json!({
                                    "slide": slide.id,
                                    "object": obj.id,
                                    "kind": kind_json,
                                }),
                            ));
                        }
                    });
            });

            let no_fill = fill.0 == "#00000000";
            inspector_row(ui, "Fill", |ui| {
                let (cr, valid_color) =
                    show_color_field(ui, "", &mut props.shape_fill, "prop_shape_fill");
                props.color_rect = Some(cr.rect);
                if is_committed(&cr, ui) && valid_color && props.shape_fill.trim() != fill.0 {
                    action_to_run = Some((
                        "object.update",
                        serde_json::json!({
                            "slide": slide.id,
                            "object": obj.id,
                            "kind": kind_with(*shape, props.shape_fill.trim()),
                        }),
                    ));
                }
            });

            show_fill_style_editor(
                ui,
                props,
                &fill.0,
                &slide.id,
                &obj.id,
                &media_names,
                actions_to_run,
            );

            inspector_row(ui, "No fill", |ui| {
                let mut checked = no_fill;
                if ui.checkbox(&mut checked, "").changed() {
                    let next = if checked {
                        "#00000000".to_owned()
                    } else if props.shape_fill.trim().is_empty()
                        || props.shape_fill.trim() == "#00000000"
                    {
                        "#D0D7DE".to_owned()
                    } else {
                        props.shape_fill.trim().to_owned()
                    };
                    props.shape_fill = next.clone();
                    let mut kind = kind_with(*shape, &next);
                    if checked {
                        kind["fill_style"] = Json::Null;
                    }
                    action_to_run = Some((
                        "object.update",
                        serde_json::json!({
                            "slide": slide.id,
                            "object": obj.id,
                            "kind": kind,
                        }),
                    ));
                }
            });

            if !adjust.is_empty() {
                inspector_row(ui, "Adjust", |ui| {
                    if ui.button("Reset shape").clicked() {
                        let mut kind = kind_with(*shape, &fill.0);
                        kind["adjust"] = serde_json::json!([]);
                        action_to_run = Some((
                            "object.update",
                            serde_json::json!({
                                "slide": slide.id,
                                "object": obj.id,
                                "kind": kind,
                            }),
                        ));
                    }
                });
            }

            inspector_separator(ui);
            inspector_row(ui, "No outline", |ui| {
                let mut no_outline = !props.shape_outline_enabled;
                if ui.checkbox(&mut no_outline, "").changed() {
                    let enabled = !no_outline;
                    props.shape_outline_enabled = enabled;
                    let outline_json = if enabled {
                        serde_json::json!({
                            "color": props.shape_outline_color.trim(),
                            "width": props.shape_outline_width,
                            "dash": props.shape_outline_dash,
                        })
                    } else {
                        Json::Null
                    };
                    let mut kind = kind_with(*shape, &fill.0);
                    kind["outline"] = outline_json;
                    action_to_run = Some((
                        "object.update",
                        serde_json::json!({
                            "slide": slide.id,
                            "object": obj.id,
                            "kind": kind,
                        }),
                    ));
                }
            });

            if props.shape_outline_enabled {
                inspector_row(ui, "Colour", |ui| {
                    let (cr, valid_color) = show_color_field(
                        ui,
                        "",
                        &mut props.shape_outline_color,
                        "prop_shape_outline_color",
                    );
                    props.color_rect = Some(cr.rect);
                    if is_committed(&cr, ui) && valid_color {
                        let mut kind = kind_with(*shape, &fill.0);
                        kind["outline"] = serde_json::json!({
                            "color": props.shape_outline_color.trim(),
                            "width": props.shape_outline_width,
                            "dash": props.shape_outline_dash,
                        });
                        action_to_run = Some((
                            "object.update",
                            serde_json::json!({
                                "slide": slide.id,
                                "object": obj.id,
                                "kind": kind,
                            }),
                        ));
                    }
                });

                inspector_row(ui, "Width", |ui| {
                    let rem_w = ui.available_width().max(30.0);
                    let r = add_drag_left(
                        ui,
                        [rem_w, coursecraft_render::CONTROL_COMPACT],
                        DragValue::new(&mut props.shape_outline_width)
                            .speed(0.25)
                            .range(0.0..=50.0)
                            .suffix(" px"),
                    );
                    let enter = r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if (r.drag_stopped() || r.lost_focus() || enter)
                        && props.shape_outline_width.is_finite()
                        && (0.0..=50.0).contains(&props.shape_outline_width)
                    {
                        let mut kind = kind_with(*shape, &fill.0);
                        kind["outline"] = serde_json::json!({
                            "color": props.shape_outline_color.trim(),
                            "width": props.shape_outline_width,
                            "dash": props.shape_outline_dash,
                        });
                        action_to_run = Some((
                            "object.update",
                            serde_json::json!({
                                "slide": slide.id,
                                "object": obj.id,
                                "kind": kind,
                            }),
                        ));
                    }
                });

                inspector_row(ui, "Dash", |ui| {
                    let label = match props.shape_outline_dash {
                        coursecraft_course::DashStyle::Solid => "Solid",
                        coursecraft_course::DashStyle::Dash => "Dash",
                        coursecraft_course::DashStyle::Dot => "Dot",
                    };
                    let rem_w = ui.available_width().max(30.0);
                    let mut changed = false;
                    egui::ComboBox::from_id_salt("prop_shape_outline_dash")
                        .width(rem_w)
                        .selected_text(label)
                        .show_ui(ui, |ui| {
                            for (dash, label) in [
                                (coursecraft_course::DashStyle::Solid, "Solid"),
                                (coursecraft_course::DashStyle::Dash, "Dash"),
                                (coursecraft_course::DashStyle::Dot, "Dot"),
                            ] {
                                if ui
                                    .selectable_value(&mut props.shape_outline_dash, dash, label)
                                    .clicked()
                                {
                                    changed = true;
                                }
                            }
                        });
                    if changed {
                        let mut kind = kind_with(*shape, &fill.0);
                        kind["outline"] = serde_json::json!({
                            "color": props.shape_outline_color.trim(),
                            "width": props.shape_outline_width,
                            "dash": props.shape_outline_dash,
                        });
                        action_to_run = Some((
                            "object.update",
                            serde_json::json!({
                                "slide": slide.id,
                                "object": obj.id,
                                "kind": kind,
                            }),
                        ));
                    }
                });
            }

            if coursecraft_course::shape_allows_text(*shape) {
                inspector_separator(ui);
                let default_style;
                let (style, runs, current_text, autofit) = match text {
                    Some(text) => (
                        &text.style,
                        text.runs.as_slice(),
                        text.text.as_str(),
                        Some(text.autofit),
                    ),
                    None => {
                        default_style = default_shape_text_style(fill);
                        (&default_style, &[][..], "", None)
                    }
                };
                if let Some(action) = show_text_controls(
                    ui,
                    props,
                    &slide.id,
                    &obj.id,
                    style,
                    runs,
                    current_text,
                    "shape",
                    &font_families,
                    None,
                    autofit,
                ) {
                    action_to_run = Some(action);
                }
            }
        }
        ObjectKind::Path { path } => {
            let base_kind = serde_json::to_value(&obj.kind).unwrap_or(Json::Null);
            inspector_row(ui, "Points", |ui| {
                ui.label(format!("{}", path.points.len()));
                if ui.button("Edit points").clicked() {
                    edit_points_requested = true;
                }
            });

            inspector_row(ui, "Colour", |ui| {
                let (cr, valid_color) = show_color_field(
                    ui,
                    "",
                    &mut props.path_outline_color,
                    "prop_path_outline_color",
                );
                props.color_rect = Some(cr.rect);
                if is_committed(&cr, ui) && valid_color {
                    let mut kind = base_kind.clone();
                    kind["outline"] = serde_json::json!({
                        "color": props.path_outline_color.trim(),
                        "width": props.path_outline_width,
                        "dash": props.path_outline_dash,
                    });
                    action_to_run = Some((
                        "object.update",
                        serde_json::json!({
                            "slide": slide.id,
                            "object": obj.id,
                            "kind": kind,
                        }),
                    ));
                }
            });

            inspector_row(ui, "Width", |ui| {
                let rem_w = ui.available_width().max(30.0);
                let r = add_drag_left(
                    ui,
                    [rem_w, coursecraft_render::CONTROL_COMPACT],
                    DragValue::new(&mut props.path_outline_width)
                        .speed(0.25)
                        .range(0.0..=50.0)
                        .suffix(" px"),
                );
                let enter = r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if (r.drag_stopped() || r.lost_focus() || enter)
                    && props.path_outline_width.is_finite()
                    && (0.0..=50.0).contains(&props.path_outline_width)
                {
                    let mut kind = base_kind.clone();
                    kind["outline"] = serde_json::json!({
                        "color": props.path_outline_color.trim(),
                        "width": props.path_outline_width,
                        "dash": props.path_outline_dash,
                    });
                    action_to_run = Some((
                        "object.update",
                        serde_json::json!({
                            "slide": slide.id,
                            "object": obj.id,
                            "kind": kind,
                        }),
                    ));
                }
            });

            inspector_row(ui, "Dash", |ui| {
                let label = match props.path_outline_dash {
                    coursecraft_course::DashStyle::Solid => "Solid",
                    coursecraft_course::DashStyle::Dash => "Dash",
                    coursecraft_course::DashStyle::Dot => "Dot",
                };
                let rem_w = ui.available_width().max(30.0);
                let mut changed = false;
                egui::ComboBox::from_id_salt("prop_path_outline_dash")
                    .width(rem_w)
                    .selected_text(label)
                    .show_ui(ui, |ui| {
                        for (dash, label) in [
                            (coursecraft_course::DashStyle::Solid, "Solid"),
                            (coursecraft_course::DashStyle::Dash, "Dash"),
                            (coursecraft_course::DashStyle::Dot, "Dot"),
                        ] {
                            if ui
                                .selectable_value(&mut props.path_outline_dash, dash, label)
                                .clicked()
                            {
                                changed = true;
                            }
                        }
                    });
                if changed {
                    let mut kind = base_kind.clone();
                    kind["outline"] = serde_json::json!({
                        "color": props.path_outline_color.trim(),
                        "width": props.path_outline_width,
                        "dash": props.path_outline_dash,
                    });
                    action_to_run = Some((
                        "object.update",
                        serde_json::json!({
                            "slide": slide.id,
                            "object": obj.id,
                            "kind": kind,
                        }),
                    ));
                }
            });

            inspector_separator(ui);
            inspector_row(ui, "Begin", |ui| {
                show_line_end_combo(ui, "prop_path_start_end", &mut props.path_start_end);
            });
            if props.path_start_end != path.start_end {
                let mut kind = base_kind.clone();
                kind["start_end"] = serde_json::json!(props.path_start_end);
                action_to_run = Some((
                    "object.update",
                    serde_json::json!({
                        "slide": slide.id,
                        "object": obj.id,
                        "kind": kind,
                    }),
                ));
            }

            inspector_row(ui, "End", |ui| {
                show_line_end_combo(ui, "prop_path_end_end", &mut props.path_end_end);
            });
            if props.path_end_end != path.end_end {
                let mut kind = base_kind.clone();
                kind["end_end"] = serde_json::json!(props.path_end_end);
                action_to_run = Some((
                    "object.update",
                    serde_json::json!({
                        "slide": slide.id,
                        "object": obj.id,
                        "kind": kind,
                    }),
                ));
            }

            if let Some(connector) = path.connector {
                let label = match connector {
                    coursecraft_course::ConnectorStyle::Straight => "Straight",
                    coursecraft_course::ConnectorStyle::Elbow => "Elbow",
                    coursecraft_course::ConnectorStyle::Curved => "Curved",
                };
                inspector_row(ui, "Connector", |ui| {
                    let rem_w = ui.available_width().max(30.0);
                    let mut changed = false;
                    egui::ComboBox::from_id_salt("prop_path_connector")
                        .width(rem_w)
                        .selected_text(label)
                        .show_ui(ui, |ui| {
                            for (style, label) in [
                                (coursecraft_course::ConnectorStyle::Straight, "Straight"),
                                (coursecraft_course::ConnectorStyle::Elbow, "Elbow"),
                                (coursecraft_course::ConnectorStyle::Curved, "Curved"),
                            ] {
                                if ui
                                    .selectable_value(&mut props.path_connector, Some(style), label)
                                    .clicked()
                                {
                                    changed = true;
                                }
                            }
                        });
                    if changed {
                        let mut kind = base_kind.clone();
                        kind["connector"] = serde_json::json!(props.path_connector);
                        action_to_run = Some((
                            "object.update",
                            serde_json::json!({
                                "slide": slide.id,
                                "object": obj.id,
                                "kind": kind,
                            }),
                        ));
                    }
                });
            } else {
                inspector_row(ui, "Style", |ui| {
                    let rem_w = ui.available_width().max(30.0);
                    let mut changed = false;
                    egui::ComboBox::from_id_salt("prop_path_style")
                        .width(rem_w)
                        .selected_text(match props.path_style {
                            coursecraft_course::PathStyle::Straight => "Straight",
                            coursecraft_course::PathStyle::Curved => "Curved",
                        })
                        .show_ui(ui, |ui| {
                            for (style, label) in [
                                (coursecraft_course::PathStyle::Straight, "Straight"),
                                (coursecraft_course::PathStyle::Curved, "Curved"),
                            ] {
                                if ui
                                    .selectable_value(&mut props.path_style, style, label)
                                    .clicked()
                                {
                                    changed = true;
                                }
                            }
                        });
                    if changed {
                        let mut kind = base_kind.clone();
                        kind["style"] = serde_json::json!(props.path_style);
                        action_to_run = Some((
                            "object.update",
                            serde_json::json!({
                                "slide": slide.id,
                                "object": obj.id,
                                "kind": kind,
                            }),
                        ));
                    }
                });
            }

            inspector_row(ui, "Closed", |ui| {
                let mut closed = path.closed;
                if ui.checkbox(&mut closed, "").changed() {
                    props.path_closed = closed;
                    let mut kind = base_kind.clone();
                    kind["closed"] = serde_json::json!(closed);
                    kind["fill"] = if closed {
                        serde_json::json!(props.path_fill.trim())
                    } else {
                        Json::Null
                    };
                    action_to_run = Some((
                        "object.update",
                        serde_json::json!({
                            "slide": slide.id,
                            "object": obj.id,
                            "kind": kind,
                        }),
                    ));
                }
            });

            if path.closed {
                inspector_row(ui, "Fill", |ui| {
                    let (cr, valid_color) =
                        show_color_field(ui, "", &mut props.path_fill, "prop_path_fill");
                    props.color_rect = Some(cr.rect);
                    if is_committed(&cr, ui) && valid_color {
                        let mut kind = base_kind.clone();
                        kind["fill"] = serde_json::json!(props.path_fill.trim());
                        action_to_run = Some((
                            "object.update",
                            serde_json::json!({
                                "slide": slide.id,
                                "object": obj.id,
                                "kind": kind,
                            }),
                        ));
                    }
                });
            }
        }
        ObjectKind::Image { src, alt, crop } => {
            let cropping = editor
                .ui
                .crop_edit
                .as_ref()
                .is_some_and(|session| session.object == obj.id);
            let can_crop = cropping || src_loaded;
            show_image_properties(
                ui,
                props,
                ImagePropTarget {
                    src,
                    alt,
                    slide_id: &slide.id,
                    obj_id: &obj.id,
                    has_crop: crop.is_some(),
                    cropping,
                    can_crop,
                },
                &mut replace_image_requested,
                &mut reset_natural_size_src,
                &mut suggest_alt_requested,
                &mut crop_action,
                &mut action_to_run,
            );
            if suggest_alt_requested {
                props.alt_suggest_request = Some((slide.id.clone(), obj.id.clone()));
            }
        }
        ObjectKind::Audio {
            src,
            autoplay: _,
            captions,
            volume,
            trim,
            ..
        } => {
            let p = coursecraft_render::palette(ui.ctx());
            inspector_row(ui, "Src", |ui| {
                let rem_w = ui.available_width().max(30.0);
                let r = ui.add_sized(
                    [rem_w, coursecraft_render::CONTROL_COMPACT],
                    TextEdit::singleline(&mut props.media_src).id_salt("prop_audio_src"),
                );
                if props.media_src.trim().is_empty() {
                    ui.colored_label(p.danger, "Required");
                }
                if is_committed(&r, ui)
                    && !props.media_src.trim().is_empty()
                    && props.media_src.trim() != src
                {
                    action_to_run = Some((
                        "object.update",
                        serde_json::json!({
                            "slide": slide.id,
                            "object": obj.id,
                            "kind": {
                                "type": "audio",
                                "src": props.media_src.trim(),
                                "autoplay": props.media_autoplay,
                            },
                        }),
                    ));
                }
            });

            inspector_row(ui, "Autoplay", |ui| {
                if ui.checkbox(&mut props.media_autoplay, "").changed() {
                    action_to_run = Some((
                        "object.update",
                        serde_json::json!({
                            "slide": slide.id,
                            "object": obj.id,
                            "kind": {
                                "type": "audio",
                                "src": props.media_src.trim(),
                                "autoplay": props.media_autoplay,
                            },
                        }),
                    ));
                }
            });

            show_media_volume_row(ui, props, &slide.id, &obj.id, *volume, &mut action_to_run);
            show_media_trim_row(ui, props, &slide.id, &obj.id, *trim, &mut action_to_run);

            inspector_row(ui, "Captions", |ui| {
                show_captions_row(
                    ui,
                    props,
                    &slide.id,
                    &obj.id,
                    captions,
                    caption_source_ok,
                    &mut pick_captions_requested,
                    &mut generate_captions_requested,
                    &mut action_to_run,
                );
            });

            inspector_row(ui, "", |ui| {
                if ui.button("Replace audio…").clicked() {
                    replace_audio_requested = true;
                }
            });
        }
        ObjectKind::Video {
            src,
            autoplay: _,
            captions,
            cues,
            volume,
            trim,
            ..
        } => {
            let p = coursecraft_render::palette(ui.ctx());
            inspector_row(ui, "Src", |ui| {
                let rem_w = ui.available_width().max(30.0);
                let r = ui.add_sized(
                    [rem_w, coursecraft_render::CONTROL_COMPACT],
                    TextEdit::singleline(&mut props.media_src).id_salt("prop_video_src"),
                );
                if props.media_src.trim().is_empty() {
                    ui.colored_label(p.danger, "Required");
                }
                if is_committed(&r, ui)
                    && !props.media_src.trim().is_empty()
                    && props.media_src.trim() != src
                {
                    action_to_run = Some((
                        "object.update",
                        serde_json::json!({
                            "slide": slide.id,
                            "object": obj.id,
                            "kind": {
                                "type": "video",
                                "src": props.media_src.trim(),
                                "autoplay": props.media_autoplay,
                            },
                        }),
                    ));
                }
            });

            inspector_row(ui, "Autoplay", |ui| {
                if ui.checkbox(&mut props.media_autoplay, "").changed() {
                    action_to_run = Some((
                        "object.update",
                        serde_json::json!({
                            "slide": slide.id,
                            "object": obj.id,
                            "kind": {
                                "type": "video",
                                "src": props.media_src.trim(),
                                "autoplay": props.media_autoplay,
                            },
                        }),
                    ));
                }
            });

            show_media_volume_row(ui, props, &slide.id, &obj.id, *volume, &mut action_to_run);
            show_media_trim_row(ui, props, &slide.id, &obj.id, *trim, &mut action_to_run);

            inspector_row(ui, "Captions", |ui| {
                show_captions_row(
                    ui,
                    props,
                    &slide.id,
                    &obj.id,
                    captions,
                    caption_source_ok,
                    &mut pick_captions_requested,
                    &mut generate_captions_requested,
                    &mut action_to_run,
                );
            });

            ui.add_space(6.0);
            ui.label(RichText::new("Cues").color(p.text_secondary));
            let max_ms = f64::from(cue_duration_ms.max(1));
            if ui.button("Add cue at playhead").clicked() {
                action_to_run = Some((
                    "video.add_cue",
                    serde_json::json!({
                        "slide": slide.id,
                        "object": obj.id,
                        "at_ms": f64::from(playhead_ms).min(max_ms),
                    }),
                ));
            }
            if ui.button("Add question here").clicked() {
                action_to_run = Some((
                    "video.add_question_cue",
                    serde_json::json!({
                        "slide": slide.id,
                        "object": obj.id,
                        "at_ms": f64::from(playhead_ms).min(max_ms),
                    }),
                ));
            }
            props.cue_row_rects.clear();
            let mut cue_action: Option<(&'static str, Json)> = None;
            for cue in cues {
                let edit = props
                    .cue_edits
                    .entry(cue.id.clone())
                    .or_insert_with(|| CueEdit {
                        name: cue.name.clone(),
                        at_ms: cue.at_ms,
                        pause: cue.pause,
                    });
                let row = ui.vertical(|ui| {
                    let mut name_commit = false;
                    let mut time_commit = false;
                    ui.horizontal(|ui| {
                        let name = ui.add_sized(
                            [ui.available_width() * 0.62, 20.0],
                            TextEdit::singleline(&mut edit.name)
                                .id_salt(("inspector_cue_name", cue.id.as_str())),
                        );
                        let at = ui.add(
                            DragValue::new(&mut edit.at_ms)
                                .speed(50.0)
                                .range(0.0..=max_ms)
                                .suffix(" ms"),
                        );
                        name_commit = is_committed(&name, ui);
                        time_commit = at.drag_stopped() || at.lost_focus();
                    });
                    let mut pause_changed = false;
                    let mut delete = false;
                    ui.horizontal(|ui| {
                        pause_changed = ui.checkbox(&mut edit.pause, "Pause").changed();
                        delete = ui.button("Delete cue").clicked();
                    });
                    if delete {
                        cue_action = Some((
                            "video.remove_cue",
                            serde_json::json!({
                                "slide": slide.id,
                                "object": obj.id,
                                "cue": cue.id,
                            }),
                        ));
                    } else if name_commit || time_commit || pause_changed {
                        cue_action = Some((
                            "video.set_cue",
                            serde_json::json!({
                                "slide": slide.id,
                                "object": obj.id,
                                "cue": cue.id,
                                "patch": {
                                    "at_ms": edit.at_ms,
                                    "name": edit.name,
                                    "pause": edit.pause,
                                },
                            }),
                        ));
                    }
                });
                props
                    .cue_row_rects
                    .push((cue.id.clone(), row.response.rect));
            }
            if let Some(action) = cue_action {
                action_to_run = Some(action);
            }
            ui.add_space(6.0);

            inspector_row(ui, "", |ui| {
                if ui.button("Replace video…").clicked() {
                    replace_video_requested = true;
                }
            });
        }
        ObjectKind::Lottie { src, speed, .. } => {
            let p = coursecraft_render::palette(ui.ctx());
            inspector_row(ui, "Src", |ui| {
                let rem_w = ui.available_width().max(30.0);
                let r = ui.add_sized(
                    [rem_w, coursecraft_render::CONTROL_COMPACT],
                    TextEdit::singleline(&mut props.media_src).id_salt("prop_lottie_src"),
                );
                if props.media_src.trim().is_empty() {
                    ui.colored_label(p.danger, "Required");
                }
                if is_committed(&r, ui)
                    && !props.media_src.trim().is_empty()
                    && props.media_src.trim() != src
                {
                    action_to_run = Some((
                        "object.update",
                        serde_json::json!({
                            "slide": slide.id,
                            "object": obj.id,
                            "kind": {
                                "type": "lottie",
                                "src": props.media_src.trim(),
                                "loop": props.lottie_loop,
                                "autoplay": props.lottie_autoplay,
                                "speed": props.lottie_speed,
                            },
                        }),
                    ));
                }
            });

            inspector_row(ui, "Loop", |ui| {
                if ui.checkbox(&mut props.lottie_loop, "").changed() {
                    action_to_run = Some((
                        "object.set_lottie",
                        serde_json::json!({
                            "slide": slide.id,
                            "object": obj.id,
                            "loop": props.lottie_loop,
                        }),
                    ));
                }
            });

            inspector_row(ui, "Autoplay", |ui| {
                if ui.checkbox(&mut props.lottie_autoplay, "").changed() {
                    action_to_run = Some((
                        "object.set_lottie",
                        serde_json::json!({
                            "slide": slide.id,
                            "object": obj.id,
                            "autoplay": props.lottie_autoplay,
                        }),
                    ));
                }
            });

            inspector_row(ui, "Speed", |ui| {
                let r = ui.add(
                    egui::DragValue::new(&mut props.lottie_speed)
                        .speed(0.05)
                        .range(
                            coursecraft_course::LOTTIE_MIN_SPEED
                                ..=coursecraft_course::LOTTIE_MAX_SPEED,
                        ),
                );
                if r.changed() && (props.lottie_speed - speed).abs() > f32::EPSILON {
                    action_to_run = Some((
                        "object.set_lottie",
                        serde_json::json!({
                            "slide": slide.id,
                            "object": obj.id,
                            "speed": props.lottie_speed,
                        }),
                    ));
                }
            });
        }
        ObjectKind::TextEntry { .. }
        | ObjectKind::Slider { .. }
        | ObjectKind::Dial { .. }
        | ObjectKind::Checkbox { .. }
        | ObjectKind::RadioGroup { .. } => {
            show_input_controls(
                ui,
                props,
                &editor.session.course,
                &slide.id,
                &obj.id,
                &obj.name,
                &obj.kind,
                actions_to_run,
            );
        }
        ObjectKind::Web { source } => {
            let p = coursecraft_render::palette(ui.ctx());
            inspector_row(ui, "Source", |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut props.web_is_url, true, "Web address");
                    ui.selectable_value(&mut props.web_is_url, false, "Folder");
                });
            });
            if props.web_is_url {
                inspector_row(ui, "URL", |ui| {
                    let rem_w = ui.available_width().max(30.0);
                    let r = ui.add_sized(
                        [rem_w, coursecraft_render::CONTROL_COMPACT],
                        TextEdit::singleline(&mut props.web_url).id_salt("prop_web_url"),
                    );
                    let url = props.web_url.trim();
                    if !url.is_empty() && !coursecraft_course::is_valid_web_url(url) {
                        ui.colored_label(p.danger, "Must start with https://");
                    }
                    if is_committed(&r, ui)
                        && !url.is_empty()
                        && !matches!(source, coursecraft_course::WebSource::Url { url: old } if old == url)
                    {
                        action_to_run = Some((
                            "object.set_web_source",
                            serde_json::json!({
                                "slide": slide.id,
                                "object": obj.id,
                                "source": { "type": "url", "url": url },
                            }),
                        ));
                    }
                });
            } else {
                inspector_row(ui, "Folder", |ui| {
                    let rem_w = ui.available_width().max(30.0);
                    let r = ui.add_sized(
                        [rem_w, coursecraft_render::CONTROL_COMPACT],
                        TextEdit::singleline(&mut props.web_path)
                            .id_salt("prop_web_folder")
                            .hint_text("web/<name>"),
                    );
                    if is_committed(&r, ui) {
                        commit_web_folder(props, &slide.id, &obj.id, source, &mut action_to_run);
                    }
                });
                inspector_row(ui, "Entry", |ui| {
                    let rem_w = ui.available_width().max(30.0);
                    let r = ui.add_sized(
                        [rem_w, coursecraft_render::CONTROL_COMPACT],
                        TextEdit::singleline(&mut props.web_entry).id_salt("prop_web_entry"),
                    );
                    if is_committed(&r, ui) {
                        commit_web_folder(props, &slide.id, &obj.id, source, &mut action_to_run);
                    }
                });
            }
        }
        ObjectKind::Timer { .. } => {
            show_timer_controls(
                ui,
                props,
                &editor.session.course,
                &slide.id,
                &obj.id,
                &font_families,
                actions_to_run,
            );
        }
        ObjectKind::ProgressMeter { .. } => {
            show_meter_controls(
                ui,
                props,
                &editor.session.course,
                &slide.id,
                &obj.id,
                actions_to_run,
            );
        }
        ObjectKind::Marker { .. } => {
            show_marker_controls(ui, props, &slide.id, &obj.id, actions_to_run);
        }
        ObjectKind::ScrollPanel { .. } => {
            let fit_height = {
                let top = obj.frame.y;
                let layer = slide.object(&obj.id).and_then(|(_, layer)| layer);
                let objects = match layer {
                    Some(id) => slide
                        .layer(id)
                        .map_or(&[][..], |layer| layer.objects.as_slice()),
                    None => slide.objects.as_slice(),
                };
                let bottom = objects
                    .iter()
                    .filter(|child| child.panel.as_deref() == Some(obj.id.as_str()))
                    .map(|child| child.frame.y + child.frame.h)
                    .fold(top + obj.frame.h, f32::max);
                (bottom - top).max(obj.frame.h)
            };
            show_scroll_panel_controls(
                ui,
                props,
                &slide.id,
                &obj.id,
                obj.frame.h,
                fit_height,
                actions_to_run,
            );
        }
        ObjectKind::Table {
            columns,
            rows,
            borders,
            header_row,
        } => {
            show_table_controls(
                ui,
                props,
                &slide.id,
                &obj.id,
                columns,
                rows,
                borders,
                *header_row,
                selected_cell,
                &font_families,
                &slides,
                actions_to_run,
            );
        }
    }

    show_panel_picker(
        ui,
        props,
        &panels,
        &slide.id,
        &obj.id,
        &obj.kind,
        actions_to_run,
    );

    if edit_points_requested {
        editor.ui.path_edit = Some(crate::canvas::PathEdit::new(
            slide.id.clone(),
            obj.id.clone(),
        ));
    }

    inspector_separator(ui);

    // 4. Frame x, y, w, h
    inspector_section_header(ui, "Frame");
    let mut frame_committed = false;

    let ((rx, enter_x), (ry, enter_y)) = show_paired_numeric_row(
        ui,
        "Position",
        "X",
        &mut props.frame_x,
        1.0,
        "Y",
        &mut props.frame_y,
        1.0,
    );
    props.frame_x_rect = Some(rx.rect);
    frame_committed |= rx.drag_stopped() || rx.lost_focus() || enter_x;
    frame_committed |= ry.drag_stopped() || ry.lost_focus() || enter_y;

    let ((rw, enter_w), (rh, enter_h)) = show_paired_numeric_row(
        ui,
        "Size",
        "W",
        &mut props.frame_w,
        1.0,
        "H",
        &mut props.frame_h,
        1.0,
    );
    frame_committed |= rw.drag_stopped() || rw.lost_focus() || enter_w;
    frame_committed |= rh.drag_stopped() || rh.lost_focus() || enter_h;

    if frame_committed {
        let fx = props.frame_x.round();
        let fy = props.frame_y.round();
        let fw = props.frame_w.round().max(4.0);
        let fh = props.frame_h.round().max(4.0);
        if fx.is_finite()
            && fy.is_finite()
            && fw.is_finite()
            && fh.is_finite()
            && (fx != obj.frame.x || fy != obj.frame.y || fw != obj.frame.w || fh != obj.frame.h)
        {
            action_to_run = Some((
                "object.set_frame",
                serde_json::json!({
                    "slide": slide.id,
                    "object": obj.id,
                    "frame": { "x": fx, "y": fy, "w": fw, "h": fh },
                }),
            ));
        }
    }

    if let Some(action) =
        crate::text_styles_ui::show_style_section(ui, &editor.session.course, &slide.id, obj)
    {
        action_to_run = Some(action);
    }
    inspector_separator(ui);

    // 5. Timing
    inspector_section_header(ui, "Timing");
    let mut timing_committed = false;

    inspector_row(ui, "Start (ms)", |ui| {
        let rem_w = ui.available_width().max(30.0);
        let r = add_drag_left(
            ui,
            [rem_w, coursecraft_render::CONTROL_COMPACT],
            DragValue::new(&mut props.start_ms).speed(10.0),
        );
        let enter = r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        timing_committed |= r.drag_stopped() || r.lost_focus() || enter;
    });

    inspector_row(ui, "End", |ui| {
        if ui
            .checkbox(&mut props.until_end, "Until slide end")
            .changed()
        {
            timing_committed = true;
        }
    });

    if !props.until_end {
        inspector_row(ui, "End (ms)", |ui| {
            let rem_w = ui.available_width().max(30.0);
            let r = add_drag_left(
                ui,
                [rem_w, coursecraft_render::CONTROL_COMPACT],
                DragValue::new(&mut props.end_ms).speed(10.0),
            );
            let enter = r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            timing_committed |= r.drag_stopped() || r.lost_focus() || enter;
        });
    }

    if timing_committed {
        let new_timing = Timing {
            start_ms: props.start_ms,
            end_ms: if props.until_end {
                None
            } else {
                Some(props.end_ms)
            },
        };
        if new_timing != obj.timing {
            action_to_run = Some((
                "object.update",
                serde_json::json!({
                    "slide": slide.id,
                    "object": obj.id,
                    "timing": new_timing,
                }),
            ));
        }
    }

    inspector_separator(ui);

    // 6. Rotation and flips
    inspector_section_header(ui, "Rotate");
    inspector_row(ui, "Rotation", |ui| {
        let rem_w = ui.available_width().max(30.0);
        let r = add_drag_left(
            ui,
            [rem_w, coursecraft_render::CONTROL_COMPACT],
            DragValue::new(&mut props.rotation)
                .speed(1.0)
                .range(0.0..=360.0)
                .suffix("°"),
        );
        props.rotation_rect = Some(r.rect);
        let enter = r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if r.drag_stopped() || r.lost_focus() || enter {
            let rotation = coursecraft_course::normalize_rotation(props.rotation);
            props.rotation = rotation;
            if (rotation - coursecraft_course::normalize_rotation(obj.rotation)).abs() > 0.001 {
                action_to_run = Some((
                    "object.set_rotation",
                    serde_json::json!({
                        "slide": slide.id,
                        "object": obj.id,
                        "degrees": rotation,
                    }),
                ));
            }
        }
    });

    inspector_row(ui, "Flip", |ui| {
        let icon_size = coursecraft_render::ICON_SM - 2.0;
        for (icon, label, axis) in [
            (
                coursecraft_render::Icon::ArrowsOut,
                "Flip horizontal",
                "horizontal",
            ),
            (
                coursecraft_render::Icon::ArrowsDownUp,
                "Flip vertical",
                "vertical",
            ),
        ] {
            let button = egui::Button::new(coursecraft_render::icon(icon, icon_size)).frame(false);
            if ui.add(button).on_hover_text(label).clicked() {
                action_to_run = Some((
                    "object.flip",
                    serde_json::json!({
                        "slide": slide.id,
                        "object": obj.id,
                        "axis": axis,
                    }),
                ));
            }
        }
    });

    show_effect_controls(ui, props, &slide.id, &obj.id, actions_to_run);

    show_drag_controls(ui, props, slide, obj, actions_to_run);

    inspector_separator(ui);

    // 7. Initial state
    inspector_section_header(ui, "State");
    let mut available_states: Vec<&str> = BUILTIN_STATES.to_vec();
    for s in &obj.states {
        if !available_states.contains(&s.name.as_str()) {
            available_states.push(&s.name);
        }
    }

    inspector_row(ui, "Initial", |ui| {
        let rem_w = ui.available_width().max(30.0);
        egui::ComboBox::from_id_salt("prop_initial_state")
            .width(rem_w)
            .selected_text(&props.initial_state)
            .show_ui(ui, |ui| {
                for state in available_states {
                    if ui
                        .selectable_value(&mut props.initial_state, state.to_owned(), state)
                        .clicked()
                        && props.initial_state != obj.initial_state
                    {
                        action_to_run = Some((
                            "object.update",
                            serde_json::json!({
                                "slide": slide.id,
                                "object": obj.id,
                                "initial_state": state,
                            }),
                        ));
                    }
                }
            });
    });

    inspector_separator(ui);
    match &obj.kind {
        ObjectKind::Text {
            text,
            runs,
            paragraphs,
            ..
        } => {
            if let Some(action) = show_paragraph_controls(
                ui, props, &slide.id, &obj.id, runs, paragraphs, &slides, text, "text", None,
            ) {
                action_to_run = Some(action);
            }
        }
        ObjectKind::Shape {
            shape,
            text: Some(text),
            ..
        } if coursecraft_course::shape_allows_text(*shape) => {
            if let Some(action) = show_paragraph_controls(
                ui,
                props,
                &slide.id,
                &obj.id,
                &text.runs,
                &text.paragraphs,
                &slides,
                &text.text,
                "shape",
                None,
            ) {
                action_to_run = Some(action);
            }
        }
        _ => {}
    }

    handle_image_actions(
        editor,
        replace_image_requested,
        replace_audio_requested,
        replace_video_requested,
        pick_captions_requested,
        reset_natural_size_src,
        actions_to_run,
    );
    if generate_captions_requested && let Some(object) = editor.selection.object.clone() {
        editor.open_captions_dialog(&editor.selection.slide.clone(), &object);
    }
    match crop_action {
        CropAction::None => {}
        CropAction::Start => {
            if let Some(id) = editor.selection.object.clone() {
                editor.start_crop(&id);
            }
        }
        CropAction::Reset => editor.reset_crop(),
        CropAction::Apply => editor.apply_crop(),
        CropAction::Cancel => editor.cancel_crop(),
    }

    if let Some(act) = action_to_run {
        actions_to_run.push(act);
    }
}

fn show_slide_properties(
    props: &mut PropertiesState,
    slide: &coursecraft_course::Slide,
    ui: &mut Ui,
    is_master: bool,
    narrate_requested: &mut bool,
    bool_names: &[String],
) -> Option<(&'static str, Json)> {
    let mut action_to_run = None;

    inspector_row(ui, "Slide title", |ui| {
        let rem_w = ui.available_width().max(30.0);
        let r = ui.add_sized(
            [rem_w, coursecraft_render::CONTROL_COMPACT],
            TextEdit::singleline(&mut props.slide_title).id_salt("prop_slide_title"),
        );
        props.slide_title_rect = Some(r.rect);
        if is_committed(&r, ui)
            && !props.slide_title.trim().is_empty()
            && props.slide_title.trim() != slide.title
        {
            action_to_run = Some((
                "slide.update",
                serde_json::json!({
                    "slide": slide.id,
                    "title": props.slide_title.trim(),
                }),
            ));
        }
    });

    if !is_master {
        inspector_row(ui, "Duration", |ui| {
            let rem_w = ui.available_width().max(30.0);
            let r = add_drag_left(
                ui,
                [rem_w, coursecraft_render::CONTROL_COMPACT],
                DragValue::new(&mut props.slide_duration_ms)
                    .speed(50.0)
                    .range(100..=86_400_000)
                    .suffix(" ms"),
            );
            let enter = r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (r.drag_stopped() || r.lost_focus() || enter)
                && props.slide_duration_ms != slide.duration_ms
            {
                action_to_run = Some((
                    "slide.update",
                    serde_json::json!({
                        "slide": slide.id,
                        "duration_ms": props.slide_duration_ms,
                    }),
                ));
            }
        });

        inspector_row(ui, "When revisiting", |ui| {
            let label = match props.slide_revisit {
                RevisitMode::Auto => "Automatic",
                RevisitMode::Reset => "Start over",
                RevisitMode::Resume => "Resume where it left off",
            };
            let mut chosen = props.slide_revisit;
            egui::ComboBox::from_id_salt("prop_slide_revisit")
                .selected_text(label)
                .width(ui.available_width().max(90.0))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut chosen, RevisitMode::Auto, "Automatic");
                    ui.selectable_value(&mut chosen, RevisitMode::Reset, "Start over");
                    ui.selectable_value(
                        &mut chosen,
                        RevisitMode::Resume,
                        "Resume where it left off",
                    );
                });
            if chosen != props.slide_revisit {
                props.slide_revisit = chosen;
                let value = match chosen {
                    RevisitMode::Auto => "auto",
                    RevisitMode::Reset => "reset",
                    RevisitMode::Resume => "resume",
                };
                action_to_run = Some((
                    "slide.update",
                    serde_json::json!({
                        "slide": slide.id,
                        "revisit": value,
                    }),
                ));
            }
        });
    }

    inspector_row(ui, "Background", |ui| {
        let (cr, valid_color) =
            show_color_field(ui, "", &mut props.slide_background, "prop_slide_bg");
        if is_committed(&cr, ui)
            && valid_color
            && slide.background.as_ref().map(|c| c.0.as_str())
                != Some(props.slide_background.trim())
        {
            action_to_run = Some((
                "slide.set_background",
                serde_json::json!({
                    "slide": slide.id,
                    "background": props.slide_background.trim(),
                }),
            ));
        }
    });
    // On its own row: beside the colour field it overflows the panel.
    if slide.background.is_some() {
        inspector_row(ui, "", |ui| {
            if ui.button("Use theme").clicked() {
                action_to_run = Some((
                    "slide.set_background",
                    serde_json::json!({
                        "slide": slide.id,
                        "background": serde_json::Value::Null,
                    }),
                ));
            }
        });
    }

    if !is_master {
        inspector_row(ui, "Next unlocks", |ui| {
            let current = slide
                .player
                .as_ref()
                .and_then(|overrides| overrides.next_requires.as_ref());
            if let Some(choice) = crate::course_settings::next_rule_editor(
                ui,
                "prop_slide_next",
                current,
                true,
                true,
                bool_names,
            ) {
                action_to_run = Some((
                    "slide.set_player_overrides",
                    serde_json::json!({ "slide": slide.id, "next_requires": choice }),
                ));
            }
        });
        let overrides = slide.player.clone().unwrap_or_default();
        ui.label(RichText::new("Player on this slide").strong());
        for (key, label, value) in [
            ("show_nav", "Navigation buttons", overrides.show_nav),
            ("show_seekbar", "Seekbar", overrides.show_seekbar),
            ("show_menu", "Slide menu", overrides.show_menu),
        ] {
            inspector_row(ui, label, |ui| {
                if let Some(choice) = tri_state_combo(ui, ("prop_slide_player", key), value) {
                    action_to_run = Some((
                        "slide.set_player_overrides",
                        serde_json::json!({ "slide": slide.id, key: choice }),
                    ));
                }
            });
        }
        // Notes take the full width, so they are comfortable to read and write.
        let p = coursecraft_render::palette(ui.ctx());
        ui.horizontal(|ui| {
            ui.label(coursecraft_render::body("Notes").color(p.text_secondary));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if crate::quiet_button(ui, "Narrate…", crate::QuietTone::Accent, true).clicked() {
                    *narrate_requested = true;
                }
            });
        });
        let r = ui.add_sized(
            [ui.available_width(), 52.0],
            TextEdit::multiline(&mut props.slide_notes)
                .id_salt("prop_slide_notes")
                .desired_rows(3),
        );
        if is_committed(&r, ui) && props.slide_notes != slide.notes {
            action_to_run = Some((
                "slide.update",
                serde_json::json!({
                    "slide": slide.id,
                    "notes": props.slide_notes.trim(),
                }),
            ));
        }
    }

    action_to_run
}

/// "Course default / Show / Hide". Returns the new choice (`None` clears it).
fn tri_state_combo(
    ui: &mut Ui,
    id_salt: (&str, &str),
    value: Option<bool>,
) -> Option<Option<bool>> {
    let label = match value {
        None => "Course default",
        Some(true) => "Show",
        Some(false) => "Hide",
    };
    let mut chosen = None;
    egui::ComboBox::from_id_salt(id_salt)
        .selected_text(label)
        .width(ui.available_width().max(90.0))
        .show_ui(ui, |ui| {
            for (option, text) in [
                (None, "Course default"),
                (Some(true), "Show"),
                (Some(false), "Hide"),
            ] {
                if ui.selectable_label(value == option, text).clicked() && value != option {
                    chosen = Some(option);
                }
            }
        });
    chosen
}

fn show_transition_section(
    props: &mut PropertiesState,
    slide: &coursecraft_course::Slide,
    ui: &mut Ui,
    action_to_run: &mut Option<(&'static str, Json)>,
) {
    let mut trans_changed = false;
    let trans_label = match props.slide_transition_effect.as_str() {
        "fade" => "Fade",
        "push" => "Push",
        "cover" => "Cover",
        _ => "None",
    };

    inspector_row(ui, "Effect", |ui| {
        let rem_w = ui.available_width().max(30.0);
        egui::ComboBox::from_id_salt("slide_transition_combo")
            .width(rem_w)
            .selected_text(trans_label)
            .show_ui(ui, |ui| {
                if ui
                    .selectable_value(
                        &mut props.slide_transition_effect,
                        "none".to_string(),
                        "None",
                    )
                    .clicked()
                {
                    trans_changed = true;
                }
                if ui
                    .selectable_value(
                        &mut props.slide_transition_effect,
                        "fade".to_string(),
                        "Fade",
                    )
                    .clicked()
                {
                    trans_changed = true;
                }
                if ui
                    .selectable_value(
                        &mut props.slide_transition_effect,
                        "push".to_string(),
                        "Push",
                    )
                    .clicked()
                {
                    trans_changed = true;
                }
                if ui
                    .selectable_value(
                        &mut props.slide_transition_effect,
                        "cover".to_string(),
                        "Cover",
                    )
                    .clicked()
                {
                    trans_changed = true;
                }
            });
    });

    if props.slide_transition_effect.as_str() != "none" {
        inspector_row(ui, "Duration", |ui| {
            let rem_w = ui.available_width().max(30.0);
            let r = add_drag_left(
                ui,
                [rem_w, coursecraft_render::CONTROL_COMPACT],
                DragValue::new(&mut props.slide_transition_duration_ms)
                    .speed(50.0)
                    .range(1..=60000)
                    .suffix(" ms"),
            );
            let enter = r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if r.drag_stopped() || r.lost_focus() || enter {
                trans_changed = true;
            }
        });

        if matches!(props.slide_transition_effect.as_str(), "push" | "cover") {
            let dir_label = match props.slide_transition_direction {
                Direction::Left => "Left",
                Direction::Right => "Right",
                Direction::Top => "Top",
                Direction::Bottom => "Bottom",
            };
            inspector_row(ui, "Direction", |ui| {
                let rem_w = ui.available_width().max(30.0);
                egui::ComboBox::from_id_salt("slide_transition_dir_combo")
                    .width(rem_w)
                    .selected_text(dir_label)
                    .show_ui(ui, |ui| {
                        for (val, l) in [
                            (Direction::Bottom, "Bottom"),
                            (Direction::Left, "Left"),
                            (Direction::Right, "Right"),
                            (Direction::Top, "Top"),
                        ] {
                            if ui
                                .selectable_value(&mut props.slide_transition_direction, val, l)
                                .clicked()
                            {
                                trans_changed = true;
                            }
                        }
                    });
            });
        }
    }

    if trans_changed {
        let trans_json = if props.slide_transition_effect.as_str() == "none" {
            serde_json::Value::Null
        } else {
            match props.slide_transition_effect.as_str() {
                "fade" => serde_json::json!({
                    "type": "fade",
                    "duration_ms": props.slide_transition_duration_ms,
                }),
                "push" => serde_json::json!({
                    "type": "push",
                    "from": props.slide_transition_direction,
                    "duration_ms": props.slide_transition_duration_ms,
                }),
                "cover" => serde_json::json!({
                    "type": "cover",
                    "from": props.slide_transition_direction,
                    "duration_ms": props.slide_transition_duration_ms,
                }),
                _ => serde_json::Value::Null,
            }
        };
        *action_to_run = Some((
            "slide.set_transition",
            serde_json::json!({
                "slide": slide.id,
                "transition": trans_json,
            }),
        ));
    }
}

struct ImagePropTarget<'a> {
    src: &'a str,
    alt: &'a str,
    slide_id: &'a str,
    obj_id: &'a str,
    /// The object has a crop stored in the course.
    has_crop: bool,
    /// The canvas is cropping this object right now.
    cropping: bool,
    /// The picture file is loaded, so crop mode can show it.
    can_crop: bool,
}

#[allow(clippy::too_many_arguments)]
fn show_image_properties(
    ui: &mut Ui,
    props: &mut PropertiesState,
    target: ImagePropTarget<'_>,
    replace_image_requested: &mut bool,
    reset_natural_size_src: &mut Option<String>,
    suggest_alt_requested: &mut bool,
    crop_action: &mut CropAction,
    action_to_run: &mut Option<(&'static str, Json)>,
) {
    inspector_row(ui, "Src", |ui| {
        ui.label(target.src);
    });
    inspector_row(ui, "", |ui| {
        if ui.button("Replace image…").clicked() {
            *replace_image_requested = true;
        }
    });
    inspector_row(ui, "", |ui| {
        if ui.button("Reset to natural size").clicked() {
            *reset_natural_size_src = Some(target.src.to_owned());
        }
    });
    inspector_row(ui, "Alt", |ui| {
        let rem_w = (ui.available_width() - 76.0).max(30.0);
        let r = ui.add_sized(
            [rem_w, coursecraft_render::CONTROL_COMPACT],
            TextEdit::singleline(&mut props.image_alt).id_salt("prop_image_alt"),
        );
        if ui.button("Suggest").clicked() {
            *suggest_alt_requested = true;
        }
        if is_committed(&r, ui) && props.image_alt != target.alt {
            *action_to_run = Some((
                "object.update",
                serde_json::json!({
                    "slide": target.slide_id,
                    "object": target.obj_id,
                    "kind": {
                        "type": "image",
                        "src": target.src,
                        "alt": props.image_alt.trim(),
                    },
                }),
            ));
        }
    });
    *crop_action = show_crop_row(ui, target.has_crop, target.cropping, target.can_crop);
}

#[allow(clippy::too_many_arguments)]
fn show_captions_row(
    ui: &mut Ui,
    props: &PropertiesState,
    slide_id: &str,
    obj_id: &str,
    captions: &Option<String>,
    can_generate: bool,
    pick_requested: &mut bool,
    generate_requested: &mut bool,
    action: &mut Option<(&'static str, Json)>,
) {
    let label = if props.media_captions.is_empty() {
        "None".to_owned()
    } else {
        props.media_captions.clone()
    };
    // Wrapped, so a long file name can't push the buttons off the panel.
    ui.horizontal_wrapped(|ui| {
        ui.label(label);
        if ui.button("Choose…").clicked() {
            *pick_requested = true;
        }
        if can_generate && ui.button("Generate…").clicked() {
            *generate_requested = true;
        }
        if captions.is_some() && ui.button("Clear").clicked() {
            *action = Some((
                "object.set_captions",
                serde_json::json!({
                    "slide": slide_id,
                    "object": obj_id,
                    "captions": null,
                }),
            ));
        }
    });
}

fn handle_image_actions(
    editor: &mut Editor,
    replace_image_requested: bool,
    replace_audio_requested: bool,
    replace_video_requested: bool,
    pick_captions_requested: bool,
    reset_natural_size_src: Option<String>,
    actions_to_run: &mut Vec<(&'static str, Json)>,
) {
    if replace_image_requested {
        editor.pending_replace_object = editor.selection.object.clone();
        editor.request(crate::EditorRequest::PickImage);
    }
    if replace_audio_requested {
        editor.pending_replace_object = editor.selection.object.clone();
        editor.request(crate::EditorRequest::PickAudio);
    }
    if replace_video_requested {
        editor.pending_replace_object = editor.selection.object.clone();
        editor.request(crate::EditorRequest::PickVideo);
    }
    if pick_captions_requested {
        editor.pending_replace_object = editor.selection.object.clone();
        editor.request(crate::EditorRequest::PickCaptions);
    }
    if let Some(src) = reset_natural_size_src
        && let Some((w, h)) = editor.natural_image_size(&src)
        && let Some(ref obj_id) = editor.selection.object
        && let Some(slide) = editor.session.course.slide(&editor.selection.slide)
        && let Some((obj, _)) = slide.object(obj_id)
    {
        actions_to_run.push((
            "object.set_frame",
            serde_json::json!({
                "slide": slide.id,
                "object": obj.id,
                "frame": {
                    "x": obj.frame.x,
                    "y": obj.frame.y,
                    "w": w,
                    "h": h,
                },
            }),
        ));
    }
}

pub fn show_animations_section(
    editor: &mut Editor,
    ui: &mut Ui,
    actions_to_run: &mut Vec<(&'static str, Json)>,
) {
    let Some(slide) = editor.session.course.slide(&editor.selection.slide) else {
        return;
    };
    let Some(ref obj_id) = editor.selection.object else {
        return;
    };
    let Some((obj, _)) = slide.object(obj_id) else {
        return;
    };

    let props = &mut editor.ui.properties;
    let mut action_to_run: Option<(&'static str, Json)> = None;

    show_anim_row(
        ui,
        "Entrance Animation",
        "entrance",
        &mut props.entrance_effect,
        &mut props.entrance_direction,
        &mut props.entrance_duration_ms,
        &mut props.entrance_easing,
        &slide.id,
        &obj.id,
        &mut action_to_run,
    );
    inspector_separator(ui);
    show_anim_row(
        ui,
        "Exit Animation",
        "exit",
        &mut props.exit_effect,
        &mut props.exit_direction,
        &mut props.exit_duration_ms,
        &mut props.exit_easing,
        &slide.id,
        &obj.id,
        &mut action_to_run,
    );
    inspector_separator(ui);
    inspector_section_header(ui, "Motion Paths");
    if crate::quiet_button(ui, "+ Add motion path", crate::QuietTone::Accent, true).clicked() {
        action_to_run = Some((
            "object.add_motion_path",
            serde_json::json!({
                "slide": slide.id,
                "object": obj.id,
            }),
        ));
    }

    let p = coursecraft_render::palette(ui.ctx());
    for path in &obj.motion_paths {
        ui.add_space(coursecraft_render::SPACE_MD);
        let card_margin = coursecraft_render::SPACE_MD;
        let card_w = ui.available_width();
        egui::Frame::NONE
            .fill(p.raised)
            .stroke(Stroke::new(1.0, p.border))
            .corner_radius(CornerRadius::same(coursecraft_render::RADIUS_MD as u8))
            .inner_margin(egui::Margin::same(card_margin as i8))
            .show(ui, |ui| {
                ui.set_width((card_w - 2.0 * (card_margin + 1.0)).max(60.0));

                // Path: name on the left, quiet delete on the right.
                inspector_row(ui, "Path", |ui| {
                    ui.label(RichText::new(&path.id).monospace());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if crate::widgets::icon_button_danger(
                            ui,
                            coursecraft_render::Icon::Trash,
                            true,
                            "Delete motion path",
                        )
                        .clicked()
                        {
                            action_to_run = Some((
                                "object.remove_motion_path",
                                serde_json::json!({
                                    "slide": slide.id,
                                    "object": obj.id,
                                    "path": path.id,
                                }),
                            ));
                        }
                    });
                });

                // Relative: checkbox.
                inspector_row(ui, "Relative", |ui| {
                    let mut rel = path.relative;
                    if ui.checkbox(&mut rel, "").changed() && rel != path.relative {
                        action_to_run = Some((
                            "object.update_motion_path",
                            serde_json::json!({
                                "slide": slide.id,
                                "object": obj.id,
                                "path": path.id,
                                "relative": rel,
                            }),
                        ));
                    }
                });

                // Duration: drag value and easing combo on one row.
                inspector_row(ui, "Duration", |ui| {
                    let mut dur = path.duration_ms;
                    let r = add_drag_left(
                        ui,
                        [64.0, coursecraft_render::CONTROL_COMPACT],
                        DragValue::new(&mut dur)
                            .speed(50.0)
                            .range(1..=60000)
                            .suffix(" ms"),
                    );
                    let enter = r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if (r.drag_stopped() || r.lost_focus() || enter) && dur != path.duration_ms {
                        action_to_run = Some((
                            "object.update_motion_path",
                            serde_json::json!({
                                "slide": slide.id,
                                "object": obj.id,
                                "path": path.id,
                                "duration_ms": dur,
                            }),
                        ));
                    }

                    let mut ease = path.easing;
                    let ease_label = match ease {
                        Easing::Linear => "Linear",
                        Easing::EaseIn => "EaseIn",
                        Easing::EaseOut => "EaseOut",
                        Easing::EaseInOut => "EaseInOut",
                    };
                    egui::ComboBox::from_id_salt(format!("mp_easing_{}", path.id))
                        .selected_text(ease_label)
                        .width(ui.available_width().max(30.0))
                        .show_ui(ui, |ui| {
                            for (val, l) in [
                                (Easing::EaseOut, "EaseOut"),
                                (Easing::Linear, "Linear"),
                                (Easing::EaseIn, "EaseIn"),
                                (Easing::EaseInOut, "EaseInOut"),
                            ] {
                                if ui.selectable_value(&mut ease, val, l).clicked()
                                    && val != path.easing
                                {
                                    action_to_run = Some((
                                        "object.update_motion_path",
                                        serde_json::json!({
                                            "slide": slide.id,
                                            "object": obj.id,
                                            "path": path.id,
                                            "easing": val,
                                        }),
                                    ));
                                }
                            }
                        });
                });

                // One row per point, with paired X/Y halves.
                let mut points_changed = false;
                let mut updated_points = path.points.clone();
                for (pt_idx, pt) in updated_points.iter_mut().enumerate() {
                    let ((rx, enter_x), (ry, enter_y)) = show_paired_numeric_row(
                        ui,
                        &format!("Point {}", pt_idx + 1),
                        "X",
                        &mut pt.x,
                        1.0,
                        "Y",
                        &mut pt.y,
                        1.0,
                    );
                    if rx.drag_stopped()
                        || rx.lost_focus()
                        || ry.drag_stopped()
                        || ry.lost_focus()
                        || enter_x
                        || enter_y
                    {
                        points_changed = true;
                    }
                }
                if points_changed && updated_points != path.points {
                    action_to_run = Some((
                        "object.update_motion_path",
                        serde_json::json!({
                            "slide": slide.id,
                            "object": obj.id,
                            "path": path.id,
                            "points": updated_points,
                        }),
                    ));
                }
            });
    }

    if let Some(act) = action_to_run {
        actions_to_run.push(act);
    }
}

pub(crate) fn is_committed(r: &Response, ui: &Ui) -> bool {
    r.lost_focus()
        || (r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
        || ui.data(|d| d.get_temp::<(egui::Id, u64)>(color_picked_id()))
            == Some((r.id, ui.ctx().cumulative_pass_nr()))
}

/// Parse a clip time typed in the inspector: `ss`, `m:ss`, `m:ss.t` or
/// `h:mm:ss`, all as whole seconds except the optional tenths/fractions.
/// Returns milliseconds, or `None` for text that is not a time.
pub(crate) fn parse_clip_time(text: &str) -> Option<u64> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let segments: Vec<&str> = text.split(':').collect();
    if segments.len() > 3 {
        return None;
    }
    let mut total_ms: u64 = 0;
    for (index, segment) in segments.iter().enumerate() {
        if segment.is_empty() {
            return None;
        }
        let last = index + 1 == segments.len();
        if last {
            let (seconds, millis) = match segment.split_once('.') {
                Some((whole, fraction)) => {
                    if whole.is_empty()
                        || fraction.is_empty()
                        || fraction.len() > 3
                        || !whole.bytes().all(|b| b.is_ascii_digit())
                        || !fraction.bytes().all(|b| b.is_ascii_digit())
                    {
                        return None;
                    }
                    let seconds: u64 = whole.parse().ok()?;
                    let frac_len = u32::try_from(fraction.len()).ok()?;
                    let fraction: u64 = fraction.parse().ok()?;
                    let scale = 10u64.checked_pow(frac_len)?;
                    (seconds, fraction.checked_mul(1000)? / scale)
                }
                None => {
                    if !segment.bytes().all(|b| b.is_ascii_digit()) {
                        return None;
                    }
                    (segment.parse().ok()?, 0)
                }
            };
            if segments.len() > 1 && seconds >= 60 {
                return None;
            }
            total_ms = total_ms
                .checked_add(seconds.checked_mul(1000)?)?
                .checked_add(millis)?;
        } else {
            if !segment.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            let value: u64 = segment.parse().ok()?;
            // Minutes inside `h:mm:ss` must be under an hour.
            if index > 0 && value >= 60 {
                return None;
            }
            let remaining = u32::try_from(segments.len() - 1 - index).ok()?;
            let factor = 60u64.checked_pow(remaining)?;
            total_ms = total_ms.checked_add(value.checked_mul(1000)?.checked_mul(factor)?)?;
        }
    }
    Some(total_ms)
}

/// Format milliseconds as `m:ss.t` (`0:00.0`), the inspector's trim style.
#[must_use]
pub(crate) fn format_clip_time(ms: u64) -> String {
    let total_seconds = ms / 1000;
    let tenths = (ms % 1000) / 100;
    let minutes = total_seconds / 60;
    let seconds = total_seconds % 60;
    format!("{minutes}:{seconds:02}.{tenths}")
}

/// The inspector's Volume row for an audio or video object.
fn show_media_volume_row(
    ui: &mut Ui,
    props: &mut PropertiesState,
    slide_id: &str,
    object_id: &str,
    volume: f32,
    action_to_run: &mut Option<(&'static str, Json)>,
) {
    inspector_row(ui, "Volume", |ui| {
        let rem_w = ui.available_width().max(30.0);
        let response = ui.add_sized(
            [rem_w, coursecraft_render::CONTROL_COMPACT],
            egui::Slider::new(&mut props.media_volume, 0.0..=100.0)
                .suffix("%")
                .max_decimals(0),
        );
        props.media_volume_rect = Some(response.rect);
        let enter = response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if response.drag_stopped() || response.lost_focus() || enter {
            let new_volume = (props.media_volume / 100.0).clamp(0.0, 1.0);
            props.media_volume = new_volume * 100.0;
            if (new_volume - volume).abs() > 0.001 {
                *action_to_run = Some((
                    "object.set_media",
                    serde_json::json!({
                        "slide": slide_id,
                        "object": object_id,
                        "volume": new_volume,
                    }),
                ));
            }
        }
    });
}

/// The inspector's Trim row: start and end text plus a Reset button.
fn show_media_trim_row(
    ui: &mut Ui,
    props: &mut PropertiesState,
    slide_id: &str,
    object_id: &str,
    trim: Option<Trim>,
    action_to_run: &mut Option<(&'static str, Json)>,
) {
    let mut commit = false;
    inspector_row(ui, "Trim", |ui| {
        let p = coursecraft_render::palette(ui.ctx());
        ui.label(RichText::new("Start").color(p.text_secondary));
        let start = ui.add_sized(
            [52.0, coursecraft_render::CONTROL_COMPACT],
            TextEdit::singleline(&mut props.media_trim_start)
                .id_salt("prop_media_trim_start")
                .hint_text("0:00.0"),
        );
        props.media_trim_start_rect = Some(start.rect);
        commit |= is_committed(&start, ui);
        ui.label(RichText::new("End").color(p.text_secondary));
        let end = ui.add_sized(
            [52.0, coursecraft_render::CONTROL_COMPACT],
            TextEdit::singleline(&mut props.media_trim_end)
                .id_salt("prop_media_trim_end")
                .hint_text("end"),
        );
        props.media_trim_end_rect = Some(end.rect);
        commit |= is_committed(&end, ui);
    });
    let p = coursecraft_render::palette(ui.ctx());
    if trim.is_some() && ui.button("Reset").clicked() {
        *action_to_run = Some((
            "object.set_media",
            serde_json::json!({
                "slide": slide_id,
                "object": object_id,
                "trim": Json::Null,
            }),
        ));
        return;
    }
    let parsed_start = parse_clip_time(&props.media_trim_start);
    let parsed_end = if props.media_trim_end.trim().is_empty() {
        Some(None)
    } else {
        parse_clip_time(&props.media_trim_end).map(Some)
    };
    if parsed_start.is_none() || parsed_end.is_none() {
        ui.colored_label(p.danger, "Use m:ss.t");
    } else if commit && let (Some(start_ms), Some(end_ms)) = (parsed_start, parsed_end) {
        *action_to_run = Some((
            "object.set_media",
            serde_json::json!({
                "slide": slide_id,
                "object": object_id,
                "trim": { "start_ms": start_ms, "end_ms": end_ms },
            }),
        ));
    }
    ui.label(RichText::new("End empty = to the end of the file").color(p.text_secondary));
}

trait ObjectKindHelpers {
    fn kind_label(&self) -> Option<&str>;
}

impl ObjectKindHelpers for coursecraft_course::Object {
    fn kind_label(&self) -> Option<&str> {
        match &self.kind {
            ObjectKind::Button { label, .. } => Some(label),
            _ => None,
        }
    }
}

/// What the inspector's Crop row can ask the editor to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CropAction {
    None,
    Start,
    Reset,
    Apply,
    Cancel,
}

/// The inspector's Crop row: opens crop mode, clears the crop, and shows
/// Apply/Cancel while the canvas is cropping.
fn show_crop_row(ui: &mut Ui, has_crop: bool, cropping: bool, can_crop: bool) -> CropAction {
    let mut action = CropAction::None;
    inspector_row(ui, "Crop", |ui| {
        if cropping {
            if ui.button("Apply").clicked() {
                action = CropAction::Apply;
            }
            if ui.button("Cancel").clicked() {
                action = CropAction::Cancel;
            }
            return;
        }
        ui.add_enabled_ui(can_crop, |ui| {
            let crop = ui.button("Crop");
            let crop = if can_crop {
                crop
            } else {
                crop.on_disabled_hover_text("Load the image file first")
            };
            if crop.clicked() {
                action = CropAction::Start;
            }
        });
        if has_crop && ui.button("Reset").clicked() {
            action = CropAction::Reset;
        }
    });
    action
}

fn line_end_label(end: coursecraft_course::LineEnd) -> &'static str {
    match end {
        coursecraft_course::LineEnd::None => "None",
        coursecraft_course::LineEnd::Arrow => "Arrow",
        coursecraft_course::LineEnd::OpenArrow => "Open arrow",
        coursecraft_course::LineEnd::Dot => "Dot",
        coursecraft_course::LineEnd::Diamond => "Diamond",
    }
}

fn show_line_end_combo(ui: &mut Ui, id: &str, value: &mut coursecraft_course::LineEnd) {
    let rem_w = ui.available_width().max(30.0);
    egui::ComboBox::from_id_salt(id)
        .width(rem_w)
        .selected_text(line_end_label(*value))
        .show_ui(ui, |ui| {
            for end in [
                coursecraft_course::LineEnd::None,
                coursecraft_course::LineEnd::Arrow,
                coursecraft_course::LineEnd::OpenArrow,
                coursecraft_course::LineEnd::Dot,
                coursecraft_course::LineEnd::Diamond,
            ] {
                ui.selectable_value(value, end, line_end_label(end));
            }
        });
}

/// Whether the media cache holds a decoded texture for `src`.
fn is_loaded_image(cache: &coursecraft_render::MediaCache, src: &str) -> bool {
    cache.is_loaded_image(src)
}
