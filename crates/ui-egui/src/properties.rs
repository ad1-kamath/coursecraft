//! Properties side panel: segmented inspector (Format, Animate, Slide, Course).

use crate::Editor;
use coursecraft_course::{
    Align, AnimEffect, Autofit, Color, Course, Easing, Glow, ListKind, ObjectKind, Paragraph,
    RevisitMode, Shadow, ShapeKind, TextRun, TextStyle, TimerFormat, Timing, Trim, VAlign,
    paragraphs_for,
};
use coursecraft_render::parse_color;
use egui::{
    Color32, CornerRadius, DragValue, FontId, Rect, Response, RichText, Sense, Stroke, StrokeKind,
    TextEdit, Ui, vec2,
};
use serde_json::Value as Json;

pub const PROPERTIES_PANEL_WIDTH: f32 = 320.0;
pub const PROPERTIES_PANEL_MIN_WIDTH: f32 = 260.0;
pub const PROPERTIES_PANEL_MAX_WIDTH: f32 = 400.0;
/// Width of the inspector's label column. "Navigation buttons", the longest label, measures
/// about 116 pt at the body size, so 120 shows every label whole; a longer one is elided.
pub const LABEL_COL_WIDTH: f32 = 120.0;

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

/// The inspector's tab row (Format, Animate, Slide, Course), drawn in its
/// header when the ribbon replaces the toolbar that used to switch them.
fn inspector_tabs(editor: &mut crate::Editor, ui: &mut Ui) {
    let tabs = [
        (
            InspectorTab::Format,
            coursecraft_render::Icon::SlidersHorizontal,
        ),
        (InspectorTab::Animate, coursecraft_render::Icon::Sparkle),
        (InspectorTab::Slide, coursecraft_render::Icon::Presentation),
        (
            InspectorTab::Course,
            coursecraft_render::Icon::GraduationCap,
        ),
    ];
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        let available = ui.available_width();
        let width = ((available - 6.0) / 4.0).floor().max(24.0);
        // Names when they fit in a quarter of the panel each, else icons.
        let font = egui::TextStyle::Button.resolve(ui.style());
        let fits = tabs.iter().all(|(tab, _)| {
            ui.painter()
                .layout_no_wrap(tl!(tab.name()).to_owned(), font.clone(), Color32::WHITE)
                .size()
                .x
                + 2.0 * ui.spacing().button_padding.x
                <= width
        });
        for (tab, icon) in tabs {
            let active = editor.ui.inspector_tab == tab;
            let name = tl!(tab.name());
            let text = if fits {
                RichText::new(name)
            } else {
                coursecraft_render::icon(icon, 15.0)
            };
            let resp = ui
                .add_sized([width, 24.0], egui::Button::selectable(active, text))
                .on_hover_text(craft_i18n::fmt(tl!("{tab} inspector"), &[("tab", name)]));
            if resp.clicked() {
                editor.ui.inspector_tab = tab;
            }
        }
    });
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

/// Last non-transparent fill (and optional fill style) remembered when the
/// Fill checkbox is unticked so re-ticking restores it.
#[derive(Debug, Clone, PartialEq)]
pub struct RememberedFill {
    pub color: String,
    pub fill_style: Option<coursecraft_course::FillStyle>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PropertiesState {
    pub last_synced_revision: u64,
    pub last_synced_selection: crate::Selection,
    pub last_synced_editing_state: Option<String>,
    pub extra_cache_key: (u64, String, String, String, String),
    pub extra_name: String,
    pub extra_text: String,
    pub extra_alt: String,
    pub extra_x: f32,
    pub extra_y: f32,
    pub extra_w: f32,
    pub extra_h: f32,
    pub extra_rotation: f32,
    pub extra_opacity: f32,

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
    /// Keyed by `(slide_id, object_id, optional_state_name)`.
    pub remembered_fills:
        std::collections::HashMap<(String, String, Option<String>), RememberedFill>,
    pub fill_add_stop_rect: Option<Rect>,
    /// Set for one frame when the author asks to choose a picture file for the fill.
    pub pick_picture_fill: bool,
    pub shadow_enabled: bool,
    pub shadow: coursecraft_course::Shadow,
    pub shadow_alpha: u8,
    pub glow_enabled: bool,
    pub glow: coursecraft_course::Glow,
    pub glow_alpha: u8,
    pub drag_enabled: bool,
    pub drag: coursecraft_course::DragSpec,
    pub drag_toggle_rect: Option<Rect>,
    pub start_ms: u32,
    pub until_end: bool,
    pub end_ms: u32,
    pub initial_state: String,

    // Text buffers
    pub text_body: String,
    /// Spell check data for `text_body`, refreshed once per frame by the panel.
    pub spell_words: Vec<coursecraft_spell::Misspelling>,
    pub spell_suggestions: std::collections::BTreeMap<String, Vec<String>>,
    pub spell_add: Vec<String>,
    pub spell_ignore: Vec<String>,
    /// Where the word menu was opened, kept while the popup is open.
    pub spell_menu_pos: Option<egui::Pos2>,
    pub text_size: f32,
    pub text_size_seen: f32,
    pub text_letter_spacing: f32,
    pub text_letter_spacing_seen: f32,
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
    pub line_spacing_seen: f32,
    /// Whether the custom line-spacing DragValue inside the ComboBox popup was interacted with.
    pub line_spacing_editing: bool,
    /// Space before and after the paragraph (Text panel), as typed, and the
    /// value last read from the course.
    pub space_before: f32,
    pub space_before_seen: f32,
    pub space_after: f32,
    pub space_after_seen: f32,
    /// Snapshot of active canvas inline-edit `(slide, object, selection_range, cursor_pos)`.
    pub inline_selection: Option<crate::InlineEditSelection>,
    /// Pending style overrides toggled at a caret (no selection), applied to the next typed chars.
    pub pending_caret_style: Option<PendingCaretStyle>,
    /// The font picker's "Add font…" entry was clicked.
    pub add_font_requested: bool,
    /// Search text for the "On this computer" font list.
    pub system_font_search: String,
    /// Whether we have already asked the app for the installed font list.
    pub system_fonts_list_requested: bool,
    /// Recently chosen font families (newest first).
    pub recent_fonts: Vec<String>,

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
    pub shape_outline_dash_pattern: Option<Vec<f32>>,
    pub shape_outline_dash_length: f32,
    pub shape_outline_gap: f32,
    pub shape_outline_join: coursecraft_course::LineJoin,
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
    pub path_outline_dash_pattern: Option<Vec<f32>>,
    pub path_outline_dash_length: f32,
    pub path_outline_gap: f32,
    pub path_outline_join: coursecraft_course::LineJoin,
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
    pub multi_font: String,
    pub multi_font_mixed: bool,
    pub multi_fill_allowed: bool,
    pub multi_fill_enabled: Option<bool>,
    pub multi_fill: String,
    pub multi_fill_mixed: bool,
    pub multi_outline_enabled: Option<bool>,
    pub multi_decorative: Option<bool>,

    // Interactive group buffers
    pub group_name: String,
    pub group_description: String,
    pub group_focus_order: Option<u32>,
    pub group_name_rect: Option<Rect>,
    pub group_description_rect: Option<Rect>,

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
    /// Options of the entrance effect (direction, shape, …); its id follows `entrance_effect`.
    pub entrance_fx: AnimEffect,
    pub entrance_duration_ms: u32,
    pub entrance_easing: Easing,
    pub entrance_sequence: coursecraft_course::Sequence,

    pub exit_effect: String,
    /// Options of the exit effect; its id follows `exit_effect`.
    pub exit_fx: AnimEffect,
    pub exit_duration_ms: u32,
    pub exit_easing: Easing,
    pub exit_sequence: coursecraft_course::Sequence,

    // Slide transition buffers
    pub slide_transition_effect: String,
    /// The transition's options, in their entrance-effect form (`TransitionEffect::as_effect`).
    pub slide_transition_fx: AnimEffect,
    pub slide_transition_duration_ms: u32,
}

impl Default for PropertiesState {
    fn default() -> Self {
        Self {
            last_synced_revision: u64::MAX,
            last_synced_selection: crate::Selection::default(),
            last_synced_editing_state: None,
            extra_cache_key: (
                u64::MAX,
                String::new(),
                String::new(),
                String::new(),
                String::new(),
            ),
            extra_name: String::new(),
            extra_text: String::new(),
            extra_alt: String::new(),
            extra_x: 0.0,
            extra_y: 0.0,
            extra_w: 100.0,
            extra_h: 100.0,
            extra_rotation: 0.0,
            extra_opacity: 1.0,
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
            pick_picture_fill: false,
            shadow_enabled: false,
            shadow: Shadow {
                color: Color("#00000080".into()),
                offset_x: 4.0,
                offset_y: 4.0,
                blur: 8.0,
            },
            shadow_alpha: 0x80,
            glow_enabled: false,
            glow: Glow {
                color: Color("#2F6FEB80".into()),
                size: 8.0,
            },
            glow_alpha: 0x80,
            drag_enabled: false,
            drag: coursecraft_course::DragSpec::default(),
            drag_toggle_rect: None,
            start_ms: 0,
            until_end: true,
            end_ms: 5000,
            initial_state: "normal".into(),
            text_body: String::new(),
            spell_words: Vec::new(),
            spell_suggestions: std::collections::BTreeMap::new(),
            spell_add: Vec::new(),
            spell_ignore: Vec::new(),
            spell_menu_pos: None,
            text_size: 20.0,
            text_size_seen: f32::NAN,
            text_letter_spacing: 0.0,
            text_letter_spacing_seen: f32::NAN,
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
            line_spacing_seen: f32::NAN,
            line_spacing_editing: false,
            space_before: 0.0,
            space_before_seen: f32::NAN,
            space_after: 0.0,
            space_after_seen: f32::NAN,
            inline_selection: None,
            pending_caret_style: None,
            add_font_requested: false,
            system_font_search: String::new(),
            system_fonts_list_requested: false,
            recent_fonts: Vec::new(),
            button_label: String::new(),
            button_fill: "#2F6FEB".into(),
            shape_kind: ShapeKind::Rectangle,
            shape_fill: "#D0D7DE".into(),
            shape_outline_enabled: false,
            shape_outline_color: "#1F1F1F".into(),
            shape_outline_width: 1.5,
            shape_outline_dash: coursecraft_course::DashStyle::Solid,
            shape_outline_dash_pattern: None,
            shape_outline_dash_length: 3.0,
            shape_outline_gap: 2.0,
            shape_outline_join: coursecraft_course::LineJoin::Miter,
            shape_gallery_query: String::new(),
            shape_gallery_rects: Vec::new(),
            shape_gallery_search_rect: None,
            path_outline_color: "#1F1F1F".into(),
            path_outline_width: 2.0,
            path_outline_dash: coursecraft_course::DashStyle::Solid,
            path_outline_dash_pattern: None,
            path_outline_dash_length: 3.0,
            path_outline_gap: 2.0,
            path_outline_join: coursecraft_course::LineJoin::Miter,
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
            multi_font: String::new(),
            multi_font_mixed: false,
            multi_fill_allowed: false,
            multi_fill_enabled: None,
            multi_fill: String::new(),
            multi_fill_mixed: false,
            multi_outline_enabled: None,
            multi_decorative: None,
            group_name: String::new(),
            group_description: String::new(),
            group_focus_order: None,
            group_name_rect: None,
            group_description_rect: None,
            course_title: String::new(),
            slide_title: String::new(),
            slide_duration_ms: 5000,
            slide_background: "#FFFFFF".into(),
            slide_notes: String::new(),
            slide_revisit: RevisitMode::Auto,
            draw_count_rect: None,
            entrance_effect: "none".into(),
            entrance_fx: AnimEffect::default(),
            entrance_duration_ms: 500,
            entrance_easing: Easing::EaseOut,
            entrance_sequence: coursecraft_course::Sequence::AsOne,
            exit_effect: "none".into(),
            exit_fx: AnimEffect::default(),
            exit_duration_ms: 500,
            exit_easing: Easing::EaseOut,
            exit_sequence: coursecraft_course::Sequence::AsOne,
            slide_transition_effect: "none".into(),
            slide_transition_fx: AnimEffect::default(),
            slide_transition_duration_ms: 500,
            remembered_fills: std::collections::HashMap::new(),
        }
    }
}

impl PropertiesState {
    pub fn remember_fill(
        &mut self,
        slide_id: &str,
        object_id: &str,
        state_name: Option<&str>,
        color: &str,
        fill_style: Option<coursecraft_course::FillStyle>,
    ) {
        let trimmed = color.trim();
        if !trimmed.is_empty() && trimmed != "#00000000" {
            self.remembered_fills.insert(
                (
                    slide_id.to_owned(),
                    object_id.to_owned(),
                    state_name.map(str::to_owned),
                ),
                RememberedFill {
                    color: trimmed.to_owned(),
                    fill_style,
                },
            );
        }
    }

    pub fn restore_fill(
        &self,
        slide_id: &str,
        object_id: &str,
        state_name: Option<&str>,
        candidate: &str,
        default_color: &str,
    ) -> (String, Option<coursecraft_course::FillStyle>) {
        if let Some(rem) = self.remembered_fills.get(&(
            slide_id.to_owned(),
            object_id.to_owned(),
            state_name.map(str::to_owned),
        )) && !rem.color.is_empty()
            && rem.color != "#00000000"
        {
            return (rem.color.clone(), rem.fill_style.clone());
        }
        let trimmed = candidate.trim();
        if !trimmed.is_empty() && trimmed != "#00000000" {
            return (trimmed.to_owned(), None);
        }
        (default_color.to_owned(), None)
    }

    /// Re-sync edit buffers with current course data if selection or revision changed.
    fn sync_object_visual_fields(
        &mut self,
        obj: &coursecraft_course::Object,
        edit_state: Option<&coursecraft_course::StateDef>,
    ) {
        self.rotation = coursecraft_course::normalize_rotation(
            edit_state.and_then(|s| s.rotation).unwrap_or(obj.rotation),
        );
        self.opacity = coursecraft_course::clamp_opacity(
            edit_state.and_then(|s| s.opacity).unwrap_or(obj.opacity),
        );
        let resolved_shadow = match edit_state {
            Some(s) if s.clear_shadow => None,
            Some(s) if s.shadow.is_some() => s.shadow.as_ref(),
            _ => obj.shadow.as_ref(),
        };
        self.shadow_enabled = resolved_shadow.is_some();
        if let Some(shadow) = resolved_shadow {
            self.shadow = shadow.clone();
        }
        self.shadow_alpha = parse_color(&self.shadow.color).map_or(255, |c| c.a());
        let resolved_glow = match edit_state {
            Some(s) if s.clear_glow => None,
            Some(s) if s.glow.is_some() => s.glow.as_ref(),
            _ => obj.glow.as_ref(),
        };
        self.glow_enabled = resolved_glow.is_some();
        if let Some(glow) = resolved_glow {
            self.glow = glow.clone();
        }
        self.glow_alpha = parse_color(&self.glow.color).map_or(255, |c| c.a());
        let base_fill_style = match &obj.kind {
            ObjectKind::Shape { fill_style, .. } | ObjectKind::Button { fill_style, .. } => {
                fill_style.clone()
            }
            _ => None,
        };
        self.fill_style = match edit_state {
            Some(s) if s.clear_fill_style => None,
            Some(s) if s.fill_style.is_some() => s.fill_style.clone(),
            Some(s) if s.fill.is_some() => None,
            _ => base_fill_style,
        };
    }

    pub fn sync(
        &mut self,
        course: &Course,
        selection: &crate::Selection,
        revision: u64,
        editing_state: Option<&crate::StateEditSession>,
    ) {
        let active_state_name = editing_state
            .filter(|s| {
                s.slide_id == selection.slide
                    && selection.object.as_deref() == Some(s.object_id.as_str())
            })
            .map(|s| s.state_name.clone());
        if self.last_synced_revision == revision
            && &self.last_synced_selection == selection
            && self.last_synced_editing_state == active_state_name
        {
            return;
        }
        self.last_synced_revision = revision;
        self.last_synced_selection = selection.clone();
        self.last_synced_editing_state = active_state_name.clone();

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
            self.slide_transition_effect = t.effect.id().to_owned();
            self.slide_transition_fx = t.effect.as_effect().unwrap_or_default();
            self.slide_transition_duration_ms = t.duration_ms;
        } else {
            self.slide_transition_effect = "none".to_owned();
            self.slide_transition_fx = AnimEffect::default();
            self.slide_transition_duration_ms = 500;
        }

        if selection.is_multi() {
            self.sync_multi(slide, selection);
        }

        self.cue_edits.clear();
        self.cue_row_rects.clear();
        self.text_size_seen = f32::NAN;
        self.text_letter_spacing_seen = f32::NAN;
        self.line_spacing_seen = f32::NAN;
        self.space_before_seen = f32::NAN;
        self.space_after_seen = f32::NAN;

        if let Some(ref obj_id) = selection.object
            && let Some((obj, _)) = slide.object(obj_id)
        {
            let edit_state = active_state_name
                .as_deref()
                .and_then(|name| obj.states.iter().find(|s| s.name == name));
            self.sync_object_visual_fields(obj, edit_state);
            self.name = obj.name.clone();
            self.frame_x = obj.frame.x;
            self.frame_y = obj.frame.y;
            self.frame_w = obj.frame.w;
            self.frame_h = obj.frame.h;
            self.drag_enabled = obj.drag.is_some();
            self.drag_toggle_rect = None;
            if let Some(drag) = &obj.drag {
                self.drag = drag.clone();
            } else {
                self.drag = coursecraft_course::DragSpec::default();
            }
            self.start_ms = obj.timing.start_ms;
            self.until_end = obj.timing.end_ms.is_none();
            self.end_ms = obj.timing.end_ms.unwrap_or(slide.duration_ms);
            self.initial_state = obj.initial_state.clone();
            self.description = obj.description.clone().unwrap_or_default();
            self.decorative = obj.decorative;
            self.focus_order = obj.focus_order;

            if !selection.is_multi() {
                if let Some(ref a) = obj.entrance {
                    self.entrance_effect = a.effect.id.clone();
                    self.entrance_fx = a.effect.clone();
                    self.entrance_duration_ms = a.duration_ms;
                    self.entrance_easing = a.easing;
                    self.entrance_sequence = a.sequence;
                } else {
                    self.entrance_effect = "none".to_owned();
                    self.entrance_fx = AnimEffect::default();
                    self.entrance_duration_ms = 500;
                    self.entrance_easing = Easing::EaseOut;
                    self.entrance_sequence = coursecraft_course::Sequence::AsOne;
                }

                if let Some(ref a) = obj.exit {
                    self.exit_effect = a.effect.id.clone();
                    self.exit_fx = a.effect.clone();
                    self.exit_duration_ms = a.duration_ms;
                    self.exit_easing = a.easing;
                    self.exit_sequence = a.sequence;
                } else {
                    self.exit_effect = "none".to_owned();
                    self.exit_fx = AnimEffect::default();
                    self.exit_duration_ms = 500;
                    self.exit_easing = Easing::EaseOut;
                    self.exit_sequence = coursecraft_course::Sequence::AsOne;
                }
            }

            match &obj.kind {
                ObjectKind::Text { text, style, .. } => {
                    let eff_text = edit_state
                        .and_then(|s| s.text.as_deref())
                        .unwrap_or(text.as_str());
                    let mut eff_style = edit_state
                        .and_then(|s| s.text_style.as_ref())
                        .unwrap_or(style)
                        .clone();
                    if let Some(s) = edit_state
                        && s.text_style.is_none()
                        && let Some(c) = &s.fill
                    {
                        eff_style.color = c.clone();
                    }
                    self.text_body = eff_text.to_owned();
                    self.text_size = eff_style.size;
                    self.text_color = eff_style.color.0.clone();
                    self.text_bold = eff_style.bold;
                    self.text_italic = eff_style.italic;
                    self.text_underline = eff_style.underline;
                    self.text_align = eff_style.align;
                    self.text_valign = eff_style.valign;
                }
                ObjectKind::Button { label, fill, .. } => {
                    self.button_label = edit_state
                        .and_then(|s| s.text.clone())
                        .unwrap_or_else(|| label.clone());
                    self.button_fill = edit_state
                        .and_then(|s| s.fill.as_ref())
                        .unwrap_or(fill)
                        .0
                        .clone();
                }
                ObjectKind::Shape {
                    shape,
                    fill,
                    outline,
                    text,
                    ..
                } => {
                    self.shape_kind = *shape;
                    let eff_fill = edit_state.and_then(|s| s.fill.as_ref()).unwrap_or(fill);
                    self.shape_fill = eff_fill.0.clone();
                    let eff_outline = match edit_state {
                        Some(s) if s.clear_outline => None,
                        Some(s) if s.outline.is_some() => s.outline.as_ref(),
                        _ => outline.as_ref(),
                    };
                    self.shape_outline_enabled = eff_outline.is_some();
                    if let Some(outline) = eff_outline {
                        self.shape_outline_color = outline.color.0.clone();
                        self.shape_outline_width = outline.width;
                        sync_dash_fields(
                            outline,
                            &mut self.shape_outline_dash,
                            &mut self.shape_outline_dash_pattern,
                            &mut self.shape_outline_dash_length,
                            &mut self.shape_outline_gap,
                        );
                        self.shape_outline_join = outline.join;
                    } else {
                        self.shape_outline_color = "#1F1F1F".into();
                        self.shape_outline_width = 1.5;
                        self.shape_outline_dash = coursecraft_course::DashStyle::Solid;
                        self.shape_outline_dash_pattern = None;
                        self.shape_outline_dash_length = 3.0;
                        self.shape_outline_gap = 2.0;
                        self.shape_outline_join = coursecraft_course::LineJoin::Miter;
                    }
                    let base_style = text
                        .as_ref()
                        .map(|t| t.style.clone())
                        .unwrap_or_else(|| default_shape_text_style(eff_fill));
                    let eff_style = edit_state
                        .and_then(|s| s.text_style.as_ref())
                        .unwrap_or(&base_style);
                    self.text_body = edit_state
                        .and_then(|s| s.text.clone())
                        .or_else(|| text.as_ref().map(|t| t.text.clone()))
                        .unwrap_or_default();
                    self.text_size = eff_style.size;
                    self.text_color = eff_style.color.0.clone();
                    self.text_bold = eff_style.bold;
                    self.text_italic = eff_style.italic;
                    self.text_underline = eff_style.underline;
                    self.text_align = eff_style.align;
                    self.text_valign = eff_style.valign;
                }
                ObjectKind::Path { path } => {
                    let eff_outline = edit_state
                        .and_then(|s| s.outline.as_ref())
                        .unwrap_or(&path.outline);
                    self.path_outline_color = eff_outline.color.0.clone();
                    self.path_outline_width = eff_outline.width;
                    sync_dash_fields(
                        eff_outline,
                        &mut self.path_outline_dash,
                        &mut self.path_outline_dash_pattern,
                        &mut self.path_outline_dash_length,
                        &mut self.path_outline_gap,
                    );
                    self.path_outline_join = eff_outline.join;
                    self.path_start_end = path.start_end;
                    self.path_end_end = path.end_end;
                    self.path_style = path.style;
                    self.path_closed = path.closed;
                    self.path_fill = edit_state
                        .and_then(|s| s.fill.as_ref())
                        .or(path.fill.as_ref())
                        .map_or_else(|| "#2F6FEB".into(), |fill| fill.0.clone());
                    self.path_connector = path.connector;
                }
                ObjectKind::Image { src, alt, .. } => {
                    self.image_src = edit_state
                        .and_then(|s| s.src.clone())
                        .unwrap_or_else(|| src.clone());
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
            self.opacity = coursecraft_course::clamp_opacity(first.opacity);
        }

        let one_group = selection
            .object
            .as_deref()
            .and_then(|id| slide.object(id))
            .and_then(|(o, _)| o.group.as_deref())
            .filter(|gid| {
                objects.len() >= 2
                    && objects.iter().all(|o| o.group.as_deref() == Some(*gid))
                    && slide
                        .group(gid)
                        .is_some_and(|_| slide.group_members(gid).len() == objects.len())
            })
            .and_then(|gid| slide.group(gid).map(|(g, _)| g));

        let (entrance_src, exit_src) = if let Some(group) = one_group {
            (group.entrance.as_ref(), group.exit.as_ref())
        } else {
            let same_entrance = objects
                .first()
                .is_some_and(|first| objects.iter().all(|o| o.entrance == first.entrance));
            let same_exit = objects
                .first()
                .is_some_and(|first| objects.iter().all(|o| o.exit == first.exit));
            (
                same_entrance
                    .then(|| objects.first().and_then(|o| o.entrance.as_ref()))
                    .flatten(),
                same_exit
                    .then(|| objects.first().and_then(|o| o.exit.as_ref()))
                    .flatten(),
            )
        };

        if let Some(a) = entrance_src {
            self.entrance_effect = a.effect.id.clone();
            self.entrance_fx = a.effect.clone();
            self.entrance_duration_ms = a.duration_ms;
            self.entrance_easing = a.easing;
            self.entrance_sequence = a.sequence;
        } else {
            self.entrance_effect = "none".to_owned();
            self.entrance_fx = AnimEffect::default();
            self.entrance_duration_ms = 500;
            self.entrance_easing = Easing::EaseOut;
            self.entrance_sequence = coursecraft_course::Sequence::AsOne;
        }

        if let Some(a) = exit_src {
            self.exit_effect = a.effect.id.clone();
            self.exit_fx = a.effect.clone();
            self.exit_duration_ms = a.duration_ms;
            self.exit_easing = a.easing;
            self.exit_sequence = a.sequence;
        } else {
            self.exit_effect = "none".to_owned();
            self.exit_fx = AnimEffect::default();
            self.exit_duration_ms = 500;
            self.exit_easing = Easing::EaseOut;
            self.exit_sequence = coursecraft_course::Sequence::AsOne;
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
        self.multi_font = String::new();
        self.multi_font_mixed = false;
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
                let same_font = styles.iter().all(|style| style.font == first.font);
                self.multi_font = if same_font {
                    crate::font_picker::displayed_font(&first.font)
                } else {
                    String::new()
                };
                self.multi_font_mixed = !same_font;
            }
        }

        let all_shapes = objects
            .iter()
            .all(|object| matches!(object.kind, ObjectKind::Shape { .. }));
        let all_buttons = objects
            .iter()
            .all(|object| matches!(object.kind, ObjectKind::Button { .. }));
        self.multi_fill_allowed = all_shapes || all_buttons;
        self.multi_fill_enabled = None;
        self.multi_fill = String::new();
        self.multi_fill_mixed = false;
        self.multi_outline_enabled = None;
        if self.multi_fill_allowed {
            let fills: Vec<&Color> = objects
                .iter()
                .filter_map(|object| match &object.kind {
                    ObjectKind::Shape { fill, .. } | ObjectKind::Button { fill, .. } => Some(fill),
                    _ => None,
                })
                .collect();
            self.multi_fill_enabled = all_same_bool(fills.iter().map(|fill| fill.0 != "#00000000"));
            if let Some(first) = fills.first() {
                let same = fills.iter().all(|fill| *fill == *first);
                self.multi_fill = if same { first.0.clone() } else { String::new() };
                self.multi_fill_mixed = !same;
            }
        }
        if all_shapes {
            self.multi_outline_enabled = all_same_bool(objects.iter().map(|object| {
                matches!(
                    &object.kind,
                    ObjectKind::Shape {
                        outline: Some(_),
                        ..
                    }
                )
            }));
            self.shape_outline_enabled = self.multi_outline_enabled != Some(false);
            if let Some(outline) = objects.iter().find_map(|object| match &object.kind {
                ObjectKind::Shape { outline, .. } => outline.as_ref(),
                _ => None,
            }) {
                self.shape_outline_color = outline.color.0.clone();
                self.shape_outline_width = outline.width;
                sync_dash_fields(
                    outline,
                    &mut self.shape_outline_dash,
                    &mut self.shape_outline_dash_pattern,
                    &mut self.shape_outline_dash_length,
                    &mut self.shape_outline_gap,
                );
                self.shape_outline_join = outline.join;
            } else {
                self.shape_outline_color = "#1F1F1F".to_owned();
                self.shape_outline_width = 1.5;
                self.shape_outline_dash = coursecraft_course::DashStyle::Solid;
                self.shape_outline_dash_pattern = None;
                self.shape_outline_dash_length = 3.0;
                self.shape_outline_gap = 2.0;
                self.shape_outline_join = coursecraft_course::LineJoin::Miter;
            }
        }

        self.multi_decorative = all_same_bool(objects.iter().map(|object| object.decorative));

        self.group_name.clear();
        self.group_description.clear();
        self.group_focus_order = None;
        self.group_name_rect = None;
        self.group_description_rect = None;
        if let Some(first_group) = objects.first().and_then(|o| o.group.as_deref())
            && objects
                .iter()
                .all(|o| o.group.as_deref() == Some(first_group))
            && let Some((group, _)) = slide.interactive_group(first_group)
        {
            self.group_name = group.name.clone();
            self.group_description = group.description.clone().unwrap_or_default();
            self.group_focus_order = group.focus_order;
        }
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

/// Standard inspector row layout: fixed 120 pt label column on the left, control filling the
/// remaining width. A label too long for the column is elided with "..." and shows in full on
/// hover. The controls never grow the row: whatever they need beyond the width is cut at the
/// row edge instead of pushing the following rows past the panel's right margin.
pub fn inspector_row<R>(ui: &mut Ui, label: &str, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
    inspector_row_inner(ui, label, None, add_contents)
}

#[derive(Clone, Copy)]
struct StateEditCtx<'a> {
    slide_id: &'a str,
    object_id: &'a str,
    state_name: &'a str,
    state: &'a coursecraft_course::StateDef,
}

fn patch_state_action(ctx: &StateEditCtx<'_>, patch: Json) -> (&'static str, Json) {
    (
        "object.patch_state",
        serde_json::json!({
            "slide": ctx.slide_id,
            "object": ctx.object_id,
            "name": ctx.state_name,
            "patch": patch,
        }),
    )
}

/// The text of the edited state: its own values, or the base object's where it has none.
fn state_text_view<'a>(
    ctx: &'a StateEditCtx<'a>,
    base_style: &'a TextStyle,
    base_runs: &'a [coursecraft_course::TextRun],
    base_paragraphs: &'a [coursecraft_course::Paragraph],
    base_text: &'a str,
) -> crate::state_text_route::StateTextView<'a> {
    crate::state_text_route::StateTextView {
        style: ctx.state.text_style.as_ref().unwrap_or(base_style),
        runs: ctx.state.runs.as_deref().unwrap_or(base_runs),
        paragraphs: ctx.state.paragraphs.as_deref().unwrap_or(base_paragraphs),
        text: ctx.state.text.as_deref().unwrap_or(base_text),
    }
}

/// Turns a text command of the inspector into a patch of the edited state (`N84`). Without
/// this the engine would run it on the base object.
fn route_text_set_to_state(
    action: (&'static str, Json),
    ctx: &StateEditCtx<'_>,
    view: &crate::state_text_route::StateTextView<'_>,
) -> (&'static str, Json) {
    if action.0 != "text.set" {
        return match crate::state_text_route::patch_for(&action, view) {
            Some(patch) => patch_state_action(ctx, patch),
            None => action,
        };
    }
    let params = action.1;
    let mut patch = serde_json::Map::new();
    for key in ["text", "runs", "paragraphs", "autofit"] {
        if let Some(val) = params.get(key) {
            patch.insert(key.to_owned(), val.clone());
        }
    }
    if let Some(style_patch) = params.get("style") {
        let mut merged = serde_json::to_value(view.style).unwrap_or(Json::Null);
        if let (Some(dst), Some(src)) = (merged.as_object_mut(), style_patch.as_object()) {
            for (k, v) in src {
                dst.insert(k.clone(), v.clone());
            }
        }
        patch.insert("text_style".to_owned(), merged);
    }
    patch_state_action(ctx, Json::Object(patch))
}

struct OverrideIndicator<'a> {
    state_name: &'a str,
    reset_tooltip: String,
    reset_clicked: &'a mut bool,
}

#[allow(clippy::too_many_arguments)]
fn inspector_row_state<R>(
    ui: &mut Ui,
    label: &str,
    state_ctx: Option<&StateEditCtx<'_>>,
    overridden: bool,
    reset_label: &str,
    reset_patch: Json,
    actions: &mut Vec<(&'static str, Json)>,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> R {
    let mut reset_clicked = false;
    let indicator = state_ctx
        .filter(|_| overridden)
        .map(|ctx| OverrideIndicator {
            state_name: ctx.state_name,
            reset_tooltip: format!("Reset {reset_label} to Normal"),
            reset_clicked: &mut reset_clicked,
        });
    let res = inspector_row_inner(ui, label, indicator, add_contents);
    if reset_clicked && let Some(ctx) = state_ctx {
        actions.push(patch_state_action(ctx, reset_patch));
    }
    res
}

fn inspector_row_inner<R>(
    ui: &mut Ui,
    label: &str,
    mut indicator: Option<OverrideIndicator<'_>>,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> R {
    use egui::text::{LayoutJob, TextWrapping};
    let p = coursecraft_render::palette(ui.ctx());
    // Keynote style: labels never carry a trailing colon. Every label of the shared Format,
    // Text and Shape Style rows goes through the lookup here (`N109`); a label that was
    // translated already is not in the catalog and stays as it is.
    let label = tl!(label.trim_end_matches(':'));
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
                let text_max_w = if indicator.is_some() {
                    (label_rect.width() - 10.0).max(10.0)
                } else {
                    label_rect.width()
                };
                let mut job = LayoutJob::simple_singleline(
                    label.to_owned(),
                    FontId::proportional(coursecraft_render::SIZE_BODY),
                    p.text_secondary,
                );
                job.wrap = TextWrapping::truncate_at_width(text_max_w);
                let galley = ui.painter().layout_job(job);
                let elided = galley.elided;
                let pos = egui::pos2(
                    label_rect.min.x,
                    label_rect.center().y - galley.size().y * 0.5,
                );
                ui.painter()
                    .with_clip_rect(ui.clip_rect().intersect(label_rect))
                    .galley(pos, galley, p.text_secondary);
                if let Some(ref ind) = indicator {
                    let dot_center = egui::pos2(label_rect.right() - 4.0, label_rect.center().y);
                    ui.painter().circle_filled(dot_center, 3.0, p.accent);
                    label_resp
                        .clone()
                        .on_hover_text(format!("Overridden in \u{201C}{}\u{201D}", ind.state_name));
                } else if elided {
                    label_resp.on_hover_text(label);
                }
            }
            let reset_reserve = if indicator.is_some() { 34.0 } else { 0.0 };
            let rem_w = (ui.available_width() - reset_reserve).max(30.0);
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
            child.spacing_mut().text_edit_width =
                child.spacing().text_edit_width.min((rem_w - 8.0).max(30.0));
            let inner = add_contents(&mut child);
            let used = child.min_rect();
            ui.allocate_rect(
                Rect::from_min_size(origin, vec2(rem_w, used.height().max(row_h))),
                Sense::hover(),
            );
            if let Some(ref mut ind) = indicator
                && crate::widgets::icon_button(
                    ui,
                    coursecraft_render::Icon::ArrowCounterClockwise,
                    &ind.reset_tooltip,
                )
                .clicked()
            {
                *ind.reset_clicked = true;
            }
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
            let mut response = ui.add_sized(
                [rem_w, coursecraft_render::CONTROL_COMPACT],
                TextEdit::singleline(color_hex).id_salt(id_salt),
            );
            let valid = parse_color(&Color(color_hex.trim().to_owned())).is_some();
            if !valid && !color_hex.trim().is_empty() {
                let p = coursecraft_render::palette(ui.ctx());
                ui.painter().rect_stroke(
                    response.rect,
                    CornerRadius::same(4),
                    Stroke::new(1.5, p.danger),
                    StrokeKind::Outside,
                );
                response = response.on_hover_text(tl!(
                    "Invalid hex colour (expected #RGB, #RRGGBB, or #RRGGBBAA)"
                ));
            }
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

    let field_key = crate::eyedropper::field_key(&id_salt);
    let popup_id = ui.make_persistent_id((id_salt, "color_picker"));
    let pending_id = popup_id.with("pending");
    let mut pending = ui.data(|d| d.get_temp::<bool>(pending_id)).unwrap_or(false);
    let mut swatch_picked = false;
    // A finished eyedropper pick lands here on the next frame. It follows the
    // exact same path as a swatch click: one command, one undo step, remembered.
    if let Some(color) = crate::eyedropper::take_pending(ui.ctx(), &field_key) {
        *color_hex = color_to_hex(color);
        pending = true;
        swatch_picked = true;
    }
    egui::Popup::menu(&response)
        .id(popup_id)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            swatch_picked |=
                color_picker_body(ui, color_hex, parsed, popup_id, &field_key, &mut pending);
        });
    let picked = swatch_picked || (pending && !ui.input(|i| i.pointer.any_down()));
    if picked {
        pending = false;
        remember_color(ui.ctx(), color_hex);
    }
    ui.data_mut(|d| d.insert_temp(pending_id, pending));
    picked
}

/// The inside of a colour popup: the eyedropper, the palette, theme and recent
/// swatches, and the colour wheel. Shared by [`color_swatch_picker`] and the
/// ribbon's Font Color. Returns true when a swatch was picked; dragging the
/// wheel writes `color_hex` and sets `pending` until the pointer is released.
/// The eyedropper closes the popup; its pick arrives through
/// `eyedropper::take_pending(field_key)` on a later frame.
pub(crate) fn color_picker_body(
    ui: &mut Ui,
    color_hex: &mut String,
    parsed: Option<Color32>,
    id_salt: egui::Id,
    field_key: &str,
    pending: &mut bool,
) -> bool {
    let mut swatch_picked = false;
    let max_h = (ui.ctx().content_rect().height() - 24.0).max(160.0);
    egui::ScrollArea::vertical()
        .id_salt((id_salt, "color_picker_scroll"))
        .max_height(max_h)
        .auto_shrink([true, true])
        .show(ui, |ui| {
            ui.spacing_mut().slider_width = 220.0;
            let eyedropper = ui
                .add(
                    egui::Button::new(coursecraft_render::icon_text(
                        coursecraft_render::Icon::Target,
                        "Eyedropper",
                    ))
                    .min_size(egui::vec2(120.0, 0.0)),
                )
                .on_hover_text("Pick a colour from anywhere in the window (Esc cancels)");
            if eyedropper.clicked() {
                ui.close();
                crate::eyedropper::start_pick(ui.ctx(), field_key.to_owned());
            }
            ui.add_space(4.0);
            let palette: Vec<(String, String)> = ui
                .ctx()
                .data(|d| d.get_temp(palette_colors_id()))
                .unwrap_or_default();
            if let Some(hex) = swatch_row(ui, "Palette", &palette) {
                *color_hex = hex;
                *pending = true;
                swatch_picked = true;
            }
            let theme: Vec<(String, String)> = ui
                .ctx()
                .data(|d| d.get_temp(theme_colors_id()))
                .unwrap_or_default();
            if let Some(hex) = swatch_row(ui, "Theme", &theme) {
                *color_hex = hex;
                *pending = true;
                swatch_picked = true;
            }
            let recent: Vec<(String, String)> = recent_colors(ui.ctx())
                .into_iter()
                .map(|hex| (hex.clone(), hex))
                .collect();
            if let Some(hex) = swatch_row(ui, "Recent", &recent) {
                *color_hex = hex;
                *pending = true;
                swatch_picked = true;
            }
            // "No fill" and an empty field have no colour to start from, and
            // dragging from invisible would stay invisible.
            let mut color = match parsed {
                Some(color) if color.a() > 0 || *pending => color,
                _ => Color32::WHITE,
            };
            if egui::color_picker::color_picker_color32(
                ui,
                &mut color,
                egui::color_picker::Alpha::OnlyBlend,
            ) {
                *color_hex = color_to_hex(color);
                *pending = true;
            }
        });
    swatch_picked
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
pub(crate) fn swatch_row(ui: &mut Ui, title: &str, colors: &[(String, String)]) -> Option<String> {
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
                        DragValue::new(v1).speed(speed1).suffix(" px"),
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
                        DragValue::new(v2).speed(speed2).suffix(" px"),
                    );
                    let enter = r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    (r, enter)
                },
            )
            .inner;

        (r1, r2)
    })
}

/// One Entrance or Exit card of the Animate panel (`anim_panel`): a change sets
/// `action_to_run` to the `object.set_animation` call for `obj_id`.
#[allow(clippy::too_many_arguments)]
fn show_anim_row(
    ui: &mut Ui,
    title: &str,
    which: &'static str,
    effect_str: &mut String,
    fx: &mut AnimEffect,
    duration_ms: &mut u32,
    easing: &mut Easing,
    (sequence, has_text): (&mut coursecraft_course::Sequence, bool),
    slide_id: &str,
    obj_id: &str,
    action_to_run: &mut Option<(&'static str, Json)>,
    pv: &mut crate::anim_preview::PreviewState,
    preview: &mut Option<crate::anim_panel::PanelPreview>,
) {
    inspector_section_header(ui, title);
    let changed = crate::anim_panel::show_card(
        ui,
        which,
        crate::anim_panel::CardState {
            effect: effect_str,
            fx,
            duration_ms,
            easing,
            sequence: has_text.then_some(&mut *sequence),
        },
        pv,
        preview,
    );
    if changed {
        *action_to_run = Some(with_sequence(
            animation_command(
                which,
                effect_str,
                fx,
                *duration_ms,
                *easing,
                slide_id,
                obj_id,
            ),
            *sequence,
        ));
        if pv.auto && effect_str.as_str() != "none" {
            *preview = Some(crate::anim_panel::PanelPreview::Play);
        }
    }
}

/// Start the canvas preview a card asked for, on the selected objects.
fn run_card_preview(
    editor: &mut Editor,
    ctx: &egui::Context,
    exit: bool,
    preview: Option<crate::anim_panel::PanelPreview>,
) {
    let slide = editor.selection.slide.clone();
    let ids = editor.selection.ids();
    apply_card_preview(
        &mut editor.ui.anim_preview,
        &editor.ui.properties,
        (&slide, ids),
        ctx,
        exit,
        preview,
    );
}

/// [`run_card_preview`] on borrowed parts of the editor.
fn apply_card_preview(
    pv: &mut crate::anim_preview::PreviewState,
    props: &PropertiesState,
    (slide, ids): (&str, Vec<String>),
    ctx: &egui::Context,
    exit: bool,
    preview: Option<crate::anim_panel::PanelPreview>,
) {
    apply_card_preview_for(pv, props, (slide, ids, None), ctx, exit, preview);
}

fn apply_card_preview_for(
    pv: &mut crate::anim_preview::PreviewState,
    props: &PropertiesState,
    (slide, ids, group): (&str, Vec<String>, Option<&str>),
    ctx: &egui::Context,
    exit: bool,
    preview: Option<crate::anim_panel::PanelPreview>,
) {
    let Some(preview) = preview else {
        return;
    };
    let (effect, fx, duration_ms, easing) = if exit {
        (
            &props.exit_effect,
            &props.exit_fx,
            props.exit_duration_ms,
            props.exit_easing,
        )
    } else {
        (
            &props.entrance_effect,
            &props.entrance_fx,
            props.entrance_duration_ms,
            props.entrance_easing,
        )
    };
    let slot = crate::anim_preview::Slot::of_exit(exit);
    match preview {
        crate::anim_panel::PanelPreview::Play => {
            if effect.as_str() == "none" {
                return;
            }
            let sequence = if exit {
                props.exit_sequence
            } else {
                props.entrance_sequence
            };
            let animation = coursecraft_course::Animation {
                effect: fx.switched_to(effect),
                duration_ms,
                easing,
                sequence,
            };
            if let Some(gid) = group {
                pv.play_group(slide, ids, gid.to_owned(), slot, animation);
            } else {
                pv.play(slide, ids, slot, animation);
            }
        }
        crate::anim_panel::PanelPreview::Hover(hovered) => {
            let duration = if effect.as_str() == "none" {
                hovered.def().map_or(500, |d| d.default_duration_ms)
            } else {
                duration_ms
            };
            if let Some(gid) = group {
                pv.hover_group(
                    slide,
                    ids,
                    gid.to_owned(),
                    slot,
                    hovered,
                    duration,
                    easing,
                    ctx.cumulative_pass_nr(),
                );
            } else {
                pv.hover(
                    slide,
                    ids,
                    slot,
                    hovered,
                    duration,
                    easing,
                    ctx.cumulative_pass_nr(),
                );
            }
        }
    }
}

/// `cmd` (an `object.set_animation` call) with the animation's text `sequence` set.
pub(crate) fn with_sequence(
    mut cmd: (&'static str, Json),
    sequence: coursecraft_course::Sequence,
) -> (&'static str, Json) {
    if !sequence.is_as_one()
        && let Some(anim) = cmd.1.get_mut("animation").and_then(Json::as_object_mut)
    {
        anim.insert("sequence".to_owned(), serde_json::json!(sequence));
    }
    cmd
}

/// The `object.set_animation` call for one row of the Animations section.
///
/// The engine stores an [`coursecraft_course::Animation`]: the effect (with
/// its options) nested under `effect`, plus duration and easing. `none`
/// clears the animation.
pub(crate) fn animation_command(
    which: &'static str,
    effect: &str,
    options: &AnimEffect,
    duration_ms: u32,
    easing: Easing,
    slide_id: &str,
    obj_id: &str,
) -> (&'static str, Json) {
    let animation = if effect == "none" {
        Json::Null
    } else {
        // The chosen effect with every option of `options` it also takes.
        serde_json::json!({
            "effect": options.switched_to(effect),
            "duration_ms": duration_ms.clamp(1, 60_000),
            "easing": easing,
        })
    };
    (
        "object.set_animation",
        serde_json::json!({
            "slide": slide_id,
            "object": obj_id,
            "which": which,
            "animation": animation,
        }),
    )
}

/// Pending style overrides toggled while a caret (with no selection) is active.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingCaretStyle {
    pub slide: String,
    pub object: String,
    pub cell: Option<(usize, usize)>,
    pub cursor: usize,
    pub patch: coursecraft_course::TextRunPatch,
}

/// The run that typing at `cursor_char` would inherit: at `0`, the first
/// non-empty run; otherwise the run containing the character before the caret.
pub(crate) fn run_at_caret(runs: &[TextRun], cursor_char: usize) -> Option<&TextRun> {
    if cursor_char == 0 {
        return runs.iter().find(|r| !r.text.is_empty());
    }
    let target = cursor_char - 1;
    let mut offset = 0usize;
    let mut last = None;
    for run in runs.iter().filter(|r| !r.text.is_empty()) {
        let len = run.text.chars().count();
        let next = offset.saturating_add(len);
        if target < next {
            return Some(run);
        }
        offset = next;
        last = Some(run);
    }
    last
}

/// Strikethrough, baseline and letter spacing in effect at the cursor.
fn effects_at_cursor(
    runs: &[TextRun],
    style: &TextStyle,
    cursor_char: usize,
) -> (bool, coursecraft_course::Baseline, f32) {
    let Some(run) = run_at_caret(runs, cursor_char) else {
        return (style.strikethrough, style.baseline, style.letter_spacing);
    };
    (
        run.strikethrough.unwrap_or(style.strikethrough),
        run.baseline.unwrap_or(style.baseline),
        run.letter_spacing.unwrap_or(style.letter_spacing),
    )
}

pub(crate) fn style_at_cursor(
    runs: &[TextRun],
    style: &TextStyle,
    cursor_char: usize,
) -> (bool, bool, bool, f32, String) {
    let Some(found_run) = run_at_caret(runs, cursor_char) else {
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

/// Effective `(bold, italic, underline, size, color)` for a character range
/// `[start, end)`. A boolean style is `true` only if every selected character
/// has it; neighbours outside `[start, end)` never count.
pub(crate) fn style_for_range(
    runs: &[TextRun],
    style: &TextStyle,
    range: (usize, usize),
) -> (bool, bool, bool, f32, String) {
    let (start, end) = (range.0.min(range.1), range.0.max(range.1));
    if start == end {
        return style_at_cursor(runs, style, start);
    }
    if runs.is_empty() {
        return (
            style.bold,
            style.italic,
            style.underline,
            style.size,
            style.color.0.clone(),
        );
    }
    let mut offset = 0usize;
    let mut any = false;
    let mut all_bold = true;
    let mut all_italic = true;
    let mut all_underline = true;
    let mut first_size = None;
    let mut first_color = None;
    for run in runs {
        let len = run.text.chars().count();
        let run_start = offset;
        let run_end = offset.saturating_add(len);
        offset = run_end;
        if run_end <= start || run_start >= end {
            continue;
        }
        any = true;
        all_bold &= run.bold.unwrap_or(style.bold);
        all_italic &= run.italic.unwrap_or(style.italic);
        all_underline &= run.underline.unwrap_or(style.underline);
        if first_size.is_none() {
            first_size = Some(run.size.unwrap_or(style.size));
        }
        if first_color.is_none() {
            first_color = Some(
                run.color
                    .as_ref()
                    .map_or_else(|| style.color.0.clone(), |c| c.0.clone()),
            );
        }
    }
    if !any {
        return style_at_cursor(runs, style, start);
    }
    (
        all_bold,
        all_italic,
        all_underline,
        first_size.unwrap_or(style.size),
        first_color.unwrap_or_else(|| style.color.0.clone()),
    )
}

fn tri_bool(all_true: bool, any_true: bool) -> Option<bool> {
    if all_true {
        Some(true)
    } else if any_true {
        None
    } else {
        Some(false)
    }
}

/// Tri-state `(bold, italic, underline)` for a character range `[start, end)`.
/// Returns `Some(true)` if all selected chars have the style, `Some(false)` if
/// none do, and `None` (Mixed) when runs disagree.
pub(crate) fn style_tri_for_range(
    runs: &[TextRun],
    style: &TextStyle,
    range: (usize, usize),
) -> (Option<bool>, Option<bool>, Option<bool>) {
    let (start, end) = (range.0.min(range.1), range.0.max(range.1));
    if start == end {
        let (b, i, u, _, _) = style_at_cursor(runs, style, start);
        return (Some(b), Some(i), Some(u));
    }
    if runs.is_empty() {
        return (Some(style.bold), Some(style.italic), Some(style.underline));
    }
    let mut offset = 0usize;
    let mut any = false;
    let (mut all_b, mut any_b) = (true, false);
    let (mut all_i, mut any_i) = (true, false);
    let (mut all_u, mut any_u) = (true, false);
    for run in runs {
        let len = run.text.chars().count();
        let run_start = offset;
        let run_end = offset.saturating_add(len);
        offset = run_end;
        if run_end <= start || run_start >= end {
            continue;
        }
        any = true;
        let b = run.bold.unwrap_or(style.bold);
        let i = run.italic.unwrap_or(style.italic);
        let u = run.underline.unwrap_or(style.underline);
        all_b &= b;
        any_b |= b;
        all_i &= i;
        any_i |= i;
        all_u &= u;
        any_u |= u;
    }
    if !any {
        let (b, i, u, _, _) = style_at_cursor(runs, style, start);
        return (Some(b), Some(i), Some(u));
    }
    (
        tri_bool(all_b, any_b),
        tri_bool(all_i, any_i),
        tri_bool(all_u, any_u),
    )
}

/// Tri-state `(strikethrough, superscript, subscript)` for `[start, end)`.
pub(crate) fn effects_tri_for_range(
    runs: &[TextRun],
    style: &TextStyle,
    range: (usize, usize),
) -> (Option<bool>, Option<bool>, Option<bool>) {
    let (start, end) = (range.0.min(range.1), range.0.max(range.1));
    if start == end {
        let (st, base, _) = effects_at_cursor(runs, style, start);
        return (
            Some(st),
            Some(base == coursecraft_course::Baseline::Superscript),
            Some(base == coursecraft_course::Baseline::Subscript),
        );
    }
    if runs.is_empty() {
        return (
            Some(style.strikethrough),
            Some(style.baseline == coursecraft_course::Baseline::Superscript),
            Some(style.baseline == coursecraft_course::Baseline::Subscript),
        );
    }
    let mut offset = 0usize;
    let mut any = false;
    let (mut all_s, mut any_s) = (true, false);
    let (mut all_sup, mut any_sup) = (true, false);
    let (mut all_sub, mut any_sub) = (true, false);
    for run in runs {
        let len = run.text.chars().count();
        let run_start = offset;
        let run_end = offset.saturating_add(len);
        offset = run_end;
        if run_end <= start || run_start >= end {
            continue;
        }
        any = true;
        let s = run.strikethrough.unwrap_or(style.strikethrough);
        let b = run.baseline.unwrap_or(style.baseline);
        let is_sup = b == coursecraft_course::Baseline::Superscript;
        let is_sub = b == coursecraft_course::Baseline::Subscript;
        all_s &= s;
        any_s |= s;
        all_sup &= is_sup;
        any_sup |= is_sup;
        all_sub &= is_sub;
        any_sub |= is_sub;
    }
    if !any {
        let (st, base, _) = effects_at_cursor(runs, style, start);
        return (
            Some(st),
            Some(base == coursecraft_course::Baseline::Superscript),
            Some(base == coursecraft_course::Baseline::Subscript),
        );
    }
    (
        tri_bool(all_s, any_s),
        tri_bool(all_sup, any_sup),
        tri_bool(all_sub, any_sub),
    )
}

/// Effective `(strikethrough, common_baseline, letter_spacing)` for `[start, end)`.
/// `common_baseline` is `Some(b)` only when every selected character has `b`.
fn effects_for_range(
    runs: &[TextRun],
    style: &TextStyle,
    range: (usize, usize),
) -> (bool, Option<coursecraft_course::Baseline>, f32) {
    let (start, end) = (range.0.min(range.1), range.0.max(range.1));
    if start == end {
        let (strike, base, spacing) = effects_at_cursor(runs, style, start);
        return (strike, Some(base), spacing);
    }
    if runs.is_empty() {
        return (
            style.strikethrough,
            Some(style.baseline),
            style.letter_spacing,
        );
    }
    let mut offset = 0usize;
    let mut any = false;
    let mut all_strike = true;
    let mut common_baseline: Option<Option<coursecraft_course::Baseline>> = None;
    let mut first_spacing = None;
    for run in runs {
        let len = run.text.chars().count();
        let run_start = offset;
        let run_end = offset.saturating_add(len);
        offset = run_end;
        if run_end <= start || run_start >= end {
            continue;
        }
        any = true;
        all_strike &= run.strikethrough.unwrap_or(style.strikethrough);
        let b = run.baseline.unwrap_or(style.baseline);
        common_baseline = match common_baseline {
            None => Some(Some(b)),
            Some(Some(prev)) if prev == b => Some(Some(prev)),
            _ => Some(None),
        };
        if first_spacing.is_none() {
            first_spacing = Some(run.letter_spacing.unwrap_or(style.letter_spacing));
        }
    }
    if !any {
        let (strike, base, spacing) = effects_at_cursor(runs, style, start);
        return (strike, Some(base), spacing);
    }
    (
        all_strike,
        common_baseline.flatten(),
        first_spacing.unwrap_or(style.letter_spacing),
    )
}

/// The font named by the run at `cursor_char`, if any.
pub(crate) fn font_at_cursor(runs: &[TextRun], cursor_char: usize) -> Option<String> {
    run_at_caret(runs, cursor_char)
        .and_then(|run| run.font.clone())
        .or_else(|| runs.iter().rev().find_map(|run| run.font.clone()))
}

/// The link on the run at `cursor_char`, if any.
fn link_at_char(runs: &[TextRun], cursor_char: usize) -> Option<coursecraft_course::Link> {
    run_at_caret(runs, cursor_char)
        .and_then(|run| run.link.clone())
        .or_else(|| runs.iter().rev().find_map(|run| run.link.clone()))
}

/// Render the segmented inspector panel (Format · Animate · Slide · Course).
/// Bring the shared inspector state up to date for this frame: the selection,
/// the canvas text selection, spelling. Returns whether the selection changed.
fn prepare(editor: &mut Editor, ui: &Ui) -> bool {
    let selection_changed = editor.ui.properties.last_synced_selection != editor.selection;
    editor.ui.properties.sync(
        &editor.session.course,
        &editor.selection,
        editor.session.revision,
        editor.ui.editing_state.as_ref(),
    );
    editor.ui.properties.inline_selection = if editor.ui.inline_edit.is_some() {
        editor.ui.inline_edit_selection.clone()
    } else {
        None
    };

    // Spell data is computed once per frame here, where the editor is mutable,
    // and read by the Text Content box without threading `Editor` through the
    // whole inspector.
    let spelled = if editor.ui.inspector_tab == InspectorTab::Format
        && !editor.ui.properties.text_body.is_empty()
    {
        let spell_text = editor.ui.properties.text_body.clone();
        editor.spell_words(&spell_text)
    } else {
        Vec::new()
    };
    // Suggestions are slow to work out, so only when the word menu is open
    // or about to open (a secondary press arrives a frame before the menu).
    let menu_coming = editor.ui.properties.spell_menu_pos.is_some()
        || ui.input(|i| i.pointer.secondary_pressed() || i.pointer.secondary_down());
    let suggestions = if menu_coming && !spelled.is_empty() {
        editor.spell_suggestion_map(&spelled)
    } else {
        std::collections::BTreeMap::new()
    };
    let props = &mut editor.ui.properties;
    props.spell_words = spelled
        .iter()
        .map(|word| word.misspelling.clone())
        .collect();
    props.spell_suggestions = suggestions;
    props.spell_add.clear();
    props.spell_ignore.clear();

    selection_changed
}

pub fn show_properties_panel(editor: &mut Editor, ui: &mut Ui, docked: Option<InspectorTab>) {
    let tab = docked.unwrap_or(editor.ui.inspector_tab);
    let selection_changed = prepare(editor, ui);
    let mut actions_to_run: Vec<(&'static str, Json)> = Vec::new();
    let p = coursecraft_render::palette(ui.ctx());

    // 1. Pinned inspector header: with the ribbon, the four inspector tabs; with
    // the older toolbar (which switches them), the current tab's name.
    egui::Frame::NONE
        .fill(p.panel)
        .inner_margin(egui::Margin::symmetric(
            coursecraft_render::PANEL_MARGIN as i8,
            8,
        ))
        .show(ui, |ui| {
            // Docked, the dock tab names the inspector and switches it.
            if docked.is_some() {
                if tab == InspectorTab::Format {
                    ui.horizontal(|ui| show_arrange_icons(editor, ui, &mut actions_to_run));
                }
                return;
            }
            if editor.ui.ribbon.visible {
                inspector_tabs(editor, ui);
                if editor.ui.inspector_tab == InspectorTab::Format
                    && editor.selection.object.is_some()
                {
                    ui.add_space(6.0);
                    ui.horizontal(|ui| show_arrange_icons(editor, ui, &mut actions_to_run));
                }
                return;
            }
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
        .id_salt(("properties_panel_scroll", tab.name()))
        .max_width(avail_w)
        .auto_shrink([false, false]);
    if selection_changed {
        scroll = scroll.vertical_scroll_offset(0.0);
    }
    scroll.show(ui, |ui| {
        ui.add_space(coursecraft_render::PANEL_MARGIN);
        let inner_w = (avail_w - 2.0 * coursecraft_render::PANEL_MARGIN).max(40.0);
        egui::Frame::NONE
            .inner_margin(egui::Margin::symmetric(
                coursecraft_render::PANEL_MARGIN as i8,
                0,
            ))
            .show(ui, |ui| {
                ui.set_max_width(inner_w);
                ui.spacing_mut().text_edit_width = ui
                    .spacing()
                    .text_edit_width
                    .min((inner_w - LABEL_COL_WIDTH - 16.0).max(40.0));
                match tab {
                    InspectorTab::Format => show_format_tab(editor, ui, &mut actions_to_run),
                    InspectorTab::Animate => show_animate_tab(editor, ui, &mut actions_to_run),
                    InspectorTab::Slide => show_slide_tab(editor, ui, &mut actions_to_run),
                    InspectorTab::Course => show_course_tab(editor, ui, &mut actions_to_run),
                }
            });
        ui.add_space(coursecraft_render::PANEL_MARGIN);
    });

    finish(editor, actions_to_run);
}

/// The Text settings panel (`docs/ui/panels.md`): Font, Paragraph and Text Box
/// for the selected text box or shape text. It holds every text setting the
/// Format inspector has, through the same controls, and the ones only this
/// panel has (space before and after a paragraph). A setting not built yet is
/// drawn disabled with its reason.
pub fn show_text_panel(editor: &mut Editor, ui: &mut Ui) {
    prepare(editor, ui);
    let mut actions_to_run: Vec<(&'static str, Json)> = Vec::new();
    let avail_w = ui.available_width();
    egui::ScrollArea::vertical()
        .id_salt("text_panel_scroll")
        .max_width(avail_w)
        // In a floating window the space is whatever the window is sized to.
        .max_height(ui.available_height())
        // A launcher's jump to a section lands at once, not over several frames.
        .animated(false)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add_space(coursecraft_render::PANEL_MARGIN);
            let inner_w = (avail_w - 2.0 * coursecraft_render::PANEL_MARGIN).max(40.0);
            egui::Frame::NONE
                .inner_margin(egui::Margin::symmetric(
                    coursecraft_render::PANEL_MARGIN as i8,
                    0,
                ))
                .show(ui, |ui| {
                    ui.set_max_width(inner_w);
                    ui.spacing_mut().text_edit_width = ui
                        .spacing()
                        .text_edit_width
                        .min((inner_w - LABEL_COL_WIDTH - 16.0).max(40.0));
                    text_panel_sections(editor, ui, &mut actions_to_run);
                });
            ui.add_space(coursecraft_render::PANEL_MARGIN);
        });
    let mut synced_inline_text = None;
    if !actions_to_run.is_empty()
        && let Some(edit) = editor.ui.inline_edit.as_ref().filter(|e| {
            e.state_name.is_none() && e.extra_id.is_none() && !e.is_button && e.buffer != e.original
        })
        && actions_to_run.iter().any(|(cmd, params)| {
            *cmd != "text.set"
                && params.get("slide").and_then(Json::as_str) == Some(edit.slide.as_str())
                && params.get("object").and_then(Json::as_str) == Some(edit.object.as_str())
        })
    {
        let mut params = serde_json::json!({
            "slide": edit.slide,
            "object": edit.object,
            "text": edit.buffer,
        });
        if let Some((row, column)) = edit.cell {
            params["row"] = serde_json::json!(row);
            params["column"] = serde_json::json!(column);
        }
        actions_to_run.insert(0, ("text.set", params));
        synced_inline_text = Some(edit.buffer.clone());
    }
    finish(editor, actions_to_run);
    if let (Some(buf), Some(edit)) = (synced_inline_text, editor.ui.inline_edit.as_mut()) {
        edit.original = buf;
    }
}

/// The text the Text panel edits, with any state being edited applied.
struct TextTarget<'a> {
    style: std::borrow::Cow<'a, TextStyle>,
    runs: &'a [TextRun],
    paragraphs: &'a [Paragraph],
    text: &'a str,
    autofit: Option<Autofit>,
    /// True for a point-text object (width fits content, no fixed wrap width).
    point_text: bool,
    /// Shape text that has no text yet: the paragraph controls wait for some.
    empty_shape: bool,
}

/// A row for a setting that is not built yet: drawn disabled with its reason.
fn not_yet_row(ui: &mut Ui, label: &str, feature: &str) {
    inspector_row(ui, label, |ui| {
        ui.add_enabled_ui(false, |ui| ui.label(tl!("Not available yet")))
            .response
            .on_disabled_hover_text(format!("{} {feature}", tl!("Not available yet:")));
    });
}

fn text_panel_sections(
    editor: &mut Editor,
    ui: &mut Ui,
    actions_to_run: &mut Vec<(&'static str, Json)>,
) {
    use crate::panels::{Section, section_header};
    use std::borrow::Cow;

    let focus = &mut editor.ui.panels;
    let props = &mut editor.ui.properties;
    let lists = &editor.ui.properties_lists;
    let system_fonts = editor.ui.system_fonts.as_deref();
    let mut font_requests: Vec<crate::EditorRequest> = Vec::new();
    let mut action_to_run: Option<(&'static str, Json)> = None;

    let slide = editor.session.course.slide(&editor.selection.slide);
    let obj = if editor.selection.is_multi() {
        None
    } else {
        slide.and_then(|slide| {
            editor
                .selection
                .object
                .as_ref()
                .and_then(|id| slide.object(id))
                .map(|(obj, _)| obj)
        })
    };
    let state = obj.and_then(|obj| {
        editor
            .ui
            .editing_state
            .as_ref()
            .filter(|s| s.slide_id == editor.selection.slide && s.object_id == obj.id)
            .and_then(|s| obj.states.iter().find(|st| st.name == s.state_name))
    });
    let state_ctx = match (slide, obj, state) {
        (Some(slide), Some(obj), Some(state)) => Some(StateEditCtx {
            slide_id: &slide.id,
            object_id: &obj.id,
            state_name: &state.name,
            state,
        }),
        _ => None,
    };
    let target = obj.and_then(|obj| match &obj.kind {
        ObjectKind::Text {
            text,
            style,
            runs,
            paragraphs,
            autofit,
            point_text,
            ..
        } => {
            let mut eff = state
                .and_then(|s| s.text_style.as_ref())
                .unwrap_or(style)
                .clone();
            if let Some(state) = state
                && state.text_style.is_none()
                && let Some(c) = &state.fill
            {
                eff.color = c.clone();
            }
            Some(TextTarget {
                style: Cow::Owned(eff),
                runs: state.and_then(|s| s.runs.as_deref()).unwrap_or(runs),
                paragraphs: state
                    .and_then(|s| s.paragraphs.as_deref())
                    .unwrap_or(paragraphs),
                text: state.and_then(|s| s.text.as_deref()).unwrap_or(text),
                autofit: Some(state.and_then(|s| s.autofit).unwrap_or(*autofit)),
                point_text: *point_text,
                empty_shape: false,
            })
        }
        ObjectKind::Shape {
            shape, fill, text, ..
        } if coursecraft_course::shape_allows_text(*shape) => {
            let fill = state.and_then(|s| s.fill.as_ref()).unwrap_or(fill);
            let (base, runs, paragraphs, body, autofit): (Cow<'_, TextStyle>, _, _, &str, _) =
                match text {
                    Some(t) => (
                        Cow::Borrowed(&t.style),
                        t.runs.as_slice(),
                        t.paragraphs.as_slice(),
                        t.text.as_str(),
                        Some(t.autofit),
                    ),
                    None => (
                        Cow::Owned(default_shape_text_style(fill)),
                        &[][..],
                        &[][..],
                        "",
                        None,
                    ),
                };
            Some(TextTarget {
                style: match state.and_then(|s| s.text_style.as_ref()) {
                    Some(style) => Cow::Borrowed(style),
                    None => base.clone(),
                },
                runs: state.and_then(|s| s.runs.as_deref()).unwrap_or(runs),
                paragraphs: state
                    .and_then(|s| s.paragraphs.as_deref())
                    .unwrap_or(paragraphs),
                text: state.and_then(|s| s.text.as_deref()).unwrap_or(body),
                autofit: state.and_then(|s| s.autofit).or(autofit),
                point_text: false,
                empty_shape: text.is_none(),
            })
        }
        _ => None,
    });
    let route = |action: (&'static str, Json), target: &TextTarget<'_>| match &state_ctx {
        Some(ctx) => {
            let view = crate::state_text_route::StateTextView {
                style: &target.style,
                runs: target.runs,
                paragraphs: target.paragraphs,
                text: target.text,
            };
            route_text_set_to_state(action, ctx, &view)
        }
        None => action,
    };

    if target.is_none() {
        ui.add_space(4.0);
        ui.label(
            RichText::new(tl!(
                "Select a text box, or a shape that holds text, to change its text."
            ))
            .color(coursecraft_render::palette(ui.ctx()).text_secondary),
        );
    }
    // The ids keep this panel's text box apart from the Format inspector's,
    // which may be on screen at the same time.
    let prefix = if obj.is_some_and(|o| matches!(o.kind, ObjectKind::Shape { .. })) {
        "shape@text_panel"
    } else {
        "text@text_panel"
    };

    section_header(focus, ui, Section::Font);
    if let (Some(target), Some(slide), Some(obj)) = (&target, slide, obj) {
        {
            if let Some(action) = show_text_controls(
                ui,
                props,
                &slide.id,
                &obj.id,
                &target.style,
                target.runs,
                target.text,
                prefix,
                &lists.font_families,
                None,
                target.autofit,
                target.point_text,
                false,
                system_fonts,
                &mut font_requests,
            ) {
                action_to_run = Some(route(action, target));
            }
        }
    }
    not_yet_row(ui, tl!("Caps"), tl!("all caps and small caps"));
    not_yet_row(ui, tl!("Underline"), tl!("underline styles"));

    section_header(focus, ui, Section::Paragraph);
    match (&target, slide, obj) {
        (Some(target), Some(slide), Some(obj)) if !target.empty_shape => {
            if let Some(action) = show_paragraph_controls(
                ui,
                props,
                &slide.id,
                &obj.id,
                target.runs,
                target.paragraphs,
                &lists.slides,
                target.text,
                prefix,
                None,
                true,
            ) {
                action_to_run = Some(route(action, target));
            }
        }
        (Some(_), _, _) => {
            ui.label(
                RichText::new(tl!("Type some text first."))
                    .color(coursecraft_render::palette(ui.ctx()).text_secondary),
            );
        }
        _ => {}
    }
    not_yet_row(ui, tl!("Tab stops"), tl!("tab stops"));

    section_header(focus, ui, Section::TextBox);
    if let (Some(target), Some(slide), Some(obj)) = (&target, slide, obj)
        && let Some(action) = show_text_box_controls(
            ui,
            &slide.id,
            &obj.id,
            &target.style,
            prefix,
            target.autofit,
            target.point_text,
        )
    {
        action_to_run = Some(route(action, target));
    }
    not_yet_row(ui, tl!("Columns"), tl!("text columns"));

    editor.pending_requests.append(&mut font_requests);
    if let Some(action) = action_to_run {
        actions_to_run.push(action);
    }
}

/// Run what the inspector or a settings panel asked for this frame.
fn finish(editor: &mut Editor, mut actions_to_run: Vec<(&'static str, Json)>) {
    let spell_add = std::mem::take(&mut editor.ui.properties.spell_add);
    for word in spell_add {
        editor.add_spell_word(&word);
    }
    let spell_ignore = std::mem::take(&mut editor.ui.properties.spell_ignore);
    for word in spell_ignore {
        editor.ignore_spell_word(&word);
    }

    if std::mem::take(&mut editor.ui.properties.add_font_requested) {
        editor
            .pending_requests
            .push(crate::EditorRequest::PickFonts);
    }

    if let Some((slide, object)) = editor.ui.properties.alt_suggest_request.take() {
        crate::alt_text::suggest_for_object(editor, &slide, &object);
    }

    let panel_text_edit = actions_to_run.iter().find_map(|(cmd, params)| {
        if *cmd != "text.set" || params.get("row").is_some() {
            return None;
        }
        let sid = params.get("slide").and_then(Json::as_str)?;
        let oid = params.get("object").and_then(Json::as_str)?;
        let txt = params.get("text").and_then(Json::as_str)?;
        Some((sid.to_owned(), oid.to_owned(), txt.to_owned()))
    });

    let ran = if actions_to_run.len() > 1 {
        let steps: Vec<(String, Json)> = actions_to_run
            .into_iter()
            .map(|(cmd, params)| (cmd.to_owned(), params))
            .collect();
        editor.run_batch(&steps).is_some()
    } else if let Some((cmd, params)) = actions_to_run.pop() {
        let deleted_state = (cmd == "object.delete_state")
            .then(|| {
                params
                    .get("name")
                    .and_then(|v| v.as_str())
                    .map(str::to_owned)
            })
            .flatten();
        let ok = editor.run(cmd, params).is_some();
        if ok
            && let Some(name) = deleted_state
            && editor.ui.preview_state.as_deref() == Some(name.as_str())
        {
            editor.ui.preview_state = None;
        }
        ok
    } else {
        false
    };

    if ran
        && let Some((sid, oid, new_text)) = panel_text_edit
        && let Some(edit) = editor.ui.inline_edit.as_mut()
        && edit.slide == sid
        && edit.object == oid
        && edit.state_name.is_none()
        && edit.extra_id.is_none()
        && edit.cell.is_none()
        && edit.buffer != new_text
    {
        edit.buffer = new_text.clone();
        edit.original = new_text;
        editor.ui.inline_edit_measured_size = None;
        editor.ui.inline_edit_live_rect = None;
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
    let state_edit_locked = editor.ui.editing_state.is_some();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = vec2(2.0, 2.0);
        for (icon, label, edge) in [
            (Icon::TextAlignLeft, "Align left", "left"),
            (Icon::TextAlignCenter, "Align center", "center"),
            (Icon::TextAlignRight, "Align right", "right"),
            (Icon::ArrowUp, "Align top", "top"),
            (Icon::ArrowsDownUp, "Align middle", "middle"),
            (Icon::ArrowDown, "Align bottom", "bottom"),
        ] {
            let params = crate::menu::align_params(editor, edge);
            let enabled = !state_edit_locked && params.is_some();
            let tooltip = if state_edit_locked {
                crate::STATE_LOCK_TOOLTIP
            } else if params.is_some() {
                label
            } else {
                "Select an object first"
            };
            let response = crate::widgets::icon_button_enabled(ui, icon, enabled, tooltip);
            if response.clicked()
                && let Some(params) = params
            {
                actions_to_run.push(("object.align", params));
            }
        }
        let more = crate::widgets::icon_button_enabled(
            ui,
            Icon::DotsThree,
            !state_edit_locked,
            if state_edit_locked {
                crate::STATE_LOCK_TOOLTIP
            } else {
                "More arrange options"
            },
        );
        egui::Popup::menu(&more).show(|ui| {
            for (label, axis) in [
                ("Distribute horizontally", "horizontal"),
                ("Distribute vertically", "vertical"),
            ] {
                let params = crate::menu::distribute_params(editor, axis);
                let enabled = params.is_some();
                if ui
                    .add_enabled(enabled, egui::Button::new(label))
                    .on_disabled_hover_text("Select at least three objects")
                    .clicked()
                    && let Some(params) = params
                {
                    actions_to_run.push(("object.distribute", params));
                    ui.close();
                }
            }
            ui.separator();
            for (label, dimension) in [("Match width", "width"), ("Match height", "height")] {
                let params = crate::menu::match_size_params(editor, dimension);
                let enabled = params.is_some();
                if ui
                    .add_enabled(enabled, egui::Button::new(label))
                    .on_disabled_hover_text("Select at least two objects")
                    .clicked()
                    && let Some(params) = params
                {
                    actions_to_run.push(("object.match_size", params));
                    ui.close();
                }
            }
        });
    });
}

/// Compact list of `StateExtra`s inside the active state, allowing select, reorder, and delete.
fn show_state_extras_list(
    editor: &mut Editor,
    ui: &mut Ui,
    session: &crate::StateEditSession,
    state: &coursecraft_course::StateDef,
    actions_to_run: &mut Vec<(&'static str, Json)>,
) {
    let p = coursecraft_render::palette(ui.ctx());
    let len = state.extras.len();
    for (idx, extra) in state.extras.iter().enumerate() {
        let is_selected = session.selected_extra.as_deref() == Some(extra.id.as_str());
        let kind_tag = match &extra.kind {
            coursecraft_course::StateExtraKind::Shape { .. } => "Shape",
            coursecraft_course::StateExtraKind::Image { .. } => "Image",
            coursecraft_course::StateExtraKind::Text { .. } => "Text",
            coursecraft_course::StateExtraKind::Path { .. } => "Path",
        };
        let label = if extra.name.trim().is_empty() {
            format!("{} ({kind_tag})", extra.id)
        } else {
            format!("{} ({kind_tag})", extra.name)
        };
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let del_resp = ui
                    .add(egui::Button::new("×").min_size(egui::vec2(24.0, 24.0)))
                    .on_hover_text("Delete extra");
                if del_resp.clicked() {
                    actions_to_run.push((
                        "object.state_delete_extra",
                        serde_json::json!({
                            "slide": session.slide_id,
                            "object": session.object_id,
                            "state": session.state_name,
                            "extra": extra.id,
                        }),
                    ));
                    if is_selected && let Some(active) = editor.ui.editing_state.as_mut() {
                        active.selected_extra = None;
                    }
                }

                let down_resp = ui
                    .add_enabled(
                        idx + 1 < len,
                        egui::Button::new("↓").min_size(egui::vec2(24.0, 24.0)),
                    )
                    .on_hover_text("Move forward")
                    .on_disabled_hover_text("Already last");
                if down_resp.clicked() {
                    actions_to_run.push((
                        "object.state_reorder_extra",
                        serde_json::json!({
                            "slide": session.slide_id,
                            "object": session.object_id,
                            "state": session.state_name,
                            "extra": extra.id,
                            "index": idx + 1,
                        }),
                    ));
                }

                let up_resp = ui
                    .add_enabled(
                        idx > 0,
                        egui::Button::new("↑").min_size(egui::vec2(24.0, 24.0)),
                    )
                    .on_hover_text("Move backward")
                    .on_disabled_hover_text("Already first");
                if up_resp.clicked() {
                    actions_to_run.push((
                        "object.state_reorder_extra",
                        serde_json::json!({
                            "slide": session.slide_id,
                            "object": session.object_id,
                            "state": session.state_name,
                            "extra": extra.id,
                            "index": idx - 1,
                        }),
                    ));
                }

                let row_w = ui.available_width().max(32.0);
                let sel_resp = ui.add_sized(
                    [row_w, coursecraft_render::CONTROL_COMPACT],
                    egui::Button::selectable(is_selected, label),
                );
                if sel_resp.clicked()
                    && let Some(active) = editor.ui.editing_state.as_mut()
                {
                    active.selected_extra = Some(extra.id.clone());
                }
            });
        });
    }
    if session.selected_extra.is_some() {
        ui.add_space(4.0);
        if ui
            .add(
                egui::Button::new(RichText::new("Edit parent").color(p.accent_text))
                    .frame(false)
                    .min_size(egui::vec2(24.0, 24.0)),
            )
            .clicked()
            && let Some(active) = editor.ui.editing_state.as_mut()
        {
            active.selected_extra = None;
        }
    }
}

/// Format inspector routed to a selected `StateExtra` via `object.state_update_extra`.
#[allow(clippy::too_many_arguments)]
fn show_state_extra_properties(
    editor: &mut Editor,
    ui: &mut Ui,
    slide_id: &str,
    object_id: &str,
    state_name: &str,
    extra: &coursecraft_course::StateExtra,
    actions_to_run: &mut Vec<(&'static str, Json)>,
) {
    let mut replace_image_requested = false;
    let props = &mut editor.ui.properties;
    let rev = editor.session.revision;
    if props.extra_cache_key.0 != rev
        || props.extra_cache_key.1 != slide_id
        || props.extra_cache_key.2 != object_id
        || props.extra_cache_key.3 != state_name
        || props.extra_cache_key.4 != extra.id
    {
        props.extra_cache_key = (
            rev,
            slide_id.to_owned(),
            object_id.to_owned(),
            state_name.to_owned(),
            extra.id.clone(),
        );
        props.extra_name = extra.name.clone();
        props.extra_x = extra.frame.x;
        props.extra_y = extra.frame.y;
        props.extra_w = extra.frame.w;
        props.extra_h = extra.frame.h;
        props.extra_rotation = extra.rotation;
        props.extra_opacity = extra.opacity;
        props.extra_text = match &extra.kind {
            coursecraft_course::StateExtraKind::Text { text, .. } => text.clone(),
            coursecraft_course::StateExtraKind::Shape { text: Some(t), .. } => t.text.clone(),
            _ => String::new(),
        };
        props.extra_alt = match &extra.kind {
            coursecraft_course::StateExtraKind::Image { alt, .. } => alt.clone(),
            _ => String::new(),
        };
    }

    let push_patch = |patch: Json, actions_to_run: &mut Vec<(&'static str, Json)>| {
        actions_to_run.push((
            "object.state_update_extra",
            serde_json::json!({
                "slide": slide_id,
                "object": object_id,
                "state": state_name,
                "extra": extra.id,
                "patch": patch,
            }),
        ));
    };

    inspector_row(ui, "ID", |ui| {
        ui.label(RichText::new(&extra.id).monospace().strong());
    });

    inspector_row(ui, "Name", |ui| {
        let rem_w = ui.available_width().max(30.0);
        let r = ui.add_sized(
            [rem_w, coursecraft_render::CONTROL_COMPACT],
            TextEdit::singleline(&mut props.extra_name).id_salt("prop_extra_name"),
        );
        if is_committed(&r, ui) && props.extra_name != extra.name {
            push_patch(
                serde_json::json!({ "name": props.extra_name.trim() }),
                actions_to_run,
            );
        }
    });

    inspector_row(ui, "Opacity", |ui| {
        let rem_w = ui.available_width().max(30.0);
        let r = ui.add_sized(
            [rem_w, coursecraft_render::CONTROL_COMPACT],
            egui::Slider::new(&mut props.extra_opacity, 0.0..=1.0)
                .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
        );
        let enter = r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if r.drag_stopped() || r.lost_focus() || enter {
            let op = coursecraft_course::clamp_opacity(props.extra_opacity);
            props.extra_opacity = op;
            if (op - extra.opacity).abs() > 1e-4 {
                push_patch(serde_json::json!({ "opacity": op }), actions_to_run);
            }
        }
    });

    inspector_separator(ui);
    inspector_section_header(ui, "Position & size");

    let mut frame_changed = false;
    inspector_row(ui, "X / Y", |ui| {
        let w = ((ui.available_width() - 8.0) * 0.5).max(30.0);
        let rx = ui.add_sized(
            [w, coursecraft_render::CONTROL_COMPACT],
            egui::DragValue::new(&mut props.extra_x)
                .speed(1.0)
                .prefix("X ")
                .suffix(" px"),
        );
        let ry = ui.add_sized(
            [w, coursecraft_render::CONTROL_COMPACT],
            egui::DragValue::new(&mut props.extra_y)
                .speed(1.0)
                .prefix("Y ")
                .suffix(" px"),
        );
        if rx.drag_stopped() || rx.lost_focus() || ry.drag_stopped() || ry.lost_focus() {
            frame_changed = true;
        }
    });
    inspector_row(ui, "W / H", |ui| {
        let w = ((ui.available_width() - 8.0) * 0.5).max(30.0);
        let rw = ui.add_sized(
            [w, coursecraft_render::CONTROL_COMPACT],
            egui::DragValue::new(&mut props.extra_w)
                .speed(1.0)
                .range(1.0..=10000.0)
                .prefix("W ")
                .suffix(" px"),
        );
        let rh = ui.add_sized(
            [w, coursecraft_render::CONTROL_COMPACT],
            egui::DragValue::new(&mut props.extra_h)
                .speed(1.0)
                .range(1.0..=10000.0)
                .prefix("H ")
                .suffix(" px"),
        );
        if rw.drag_stopped() || rw.lost_focus() || rh.drag_stopped() || rh.lost_focus() {
            frame_changed = true;
        }
    });
    if frame_changed {
        let new_frame = coursecraft_course::Rect {
            x: props.extra_x,
            y: props.extra_y,
            w: props.extra_w.max(1.0),
            h: props.extra_h.max(1.0),
        };
        if new_frame != extra.frame {
            push_patch(serde_json::json!({ "frame": new_frame }), actions_to_run);
        }
    }

    inspector_row(ui, "Rotation", |ui| {
        let r = ui.add_sized(
            [72.0, coursecraft_render::CONTROL_COMPACT],
            egui::DragValue::new(&mut props.extra_rotation)
                .speed(1.0)
                .suffix("°"),
        );
        if (r.drag_stopped() || r.lost_focus())
            && (props.extra_rotation - extra.rotation).abs() > 1e-3
        {
            push_patch(
                serde_json::json!({ "rotation": props.extra_rotation }),
                actions_to_run,
            );
        }
        let mut fh = extra.flip_h;
        if ui
            .selectable_label(fh, "Flip H")
            .on_hover_text("Flip horizontally")
            .clicked()
        {
            fh = !fh;
            push_patch(serde_json::json!({ "flip_h": fh }), actions_to_run);
        }
        let mut fv = extra.flip_v;
        if ui
            .selectable_label(fv, "Flip V")
            .on_hover_text("Flip vertically")
            .clicked()
        {
            fv = !fv;
            push_patch(serde_json::json!({ "flip_v": fv }), actions_to_run);
        }
    });

    inspector_separator(ui);
    match &extra.kind {
        coursecraft_course::StateExtraKind::Shape {
            shape,
            fill,
            outline,
            text,
            ..
        } => {
            inspector_section_header(ui, "Shape");
            inspector_row(ui, "Fill", |ui| {
                let mut c = fill.0.clone();
                if color_swatch_picker(ui, &mut c, "extra_shape_fill", 20.0, 4) {
                    let mut new_kind = extra.kind.clone();
                    if let coursecraft_course::StateExtraKind::Shape {
                        fill: ref mut f, ..
                    } = new_kind
                    {
                        *f = Color(c);
                    }
                    push_patch(serde_json::json!({ "kind": new_kind }), actions_to_run);
                }
            });
            let mut has_outline = outline.is_some();
            inspector_row(ui, "Outline", |ui| {
                if ui.checkbox(&mut has_outline, "Enabled").changed() {
                    let mut new_kind = extra.kind.clone();
                    if let coursecraft_course::StateExtraKind::Shape {
                        outline: ref mut out,
                        ..
                    } = new_kind
                    {
                        *out = if has_outline {
                            Some(coursecraft_course::Outline {
                                color: Color("#222222".to_owned()),
                                width: 2.0,
                                dash: coursecraft_course::DashStyle::Solid,
                                dash_pattern: None,
                                join: coursecraft_course::LineJoin::Miter,
                                miter_limit: None,
                            })
                        } else {
                            None
                        };
                    }
                    push_patch(serde_json::json!({ "kind": new_kind }), actions_to_run);
                }
                if let Some(out) = outline {
                    let mut c = out.color.0.clone();
                    if color_swatch_picker(ui, &mut c, "extra_shape_outline", 20.0, 4) {
                        let mut new_kind = extra.kind.clone();
                        if let coursecraft_course::StateExtraKind::Shape {
                            outline: Some(ref mut o),
                            ..
                        } = new_kind
                        {
                            o.color = Color(c);
                        }
                        push_patch(serde_json::json!({ "kind": new_kind }), actions_to_run);
                    }
                }
            });
            if coursecraft_course::shape_allows_text(*shape) {
                inspector_row(ui, "Text", |ui| {
                    let rem_w = ui.available_width().max(30.0);
                    let r = ui.add_sized(
                        [rem_w, coursecraft_render::CONTROL_COMPACT],
                        TextEdit::singleline(&mut props.extra_text)
                            .id_salt("prop_extra_shape_text"),
                    );
                    let cur = text.as_ref().map_or("", |t| t.text.as_str());
                    if is_committed(&r, ui) && props.extra_text != cur {
                        let fill_color = fill.clone();
                        let mut new_kind = extra.kind.clone();
                        if let coursecraft_course::StateExtraKind::Shape {
                            text: ref mut st, ..
                        } = new_kind
                        {
                            let entry = st.get_or_insert_with(|| coursecraft_course::ShapeText {
                                text: String::new(),
                                style: default_shape_text_style(&fill_color),
                                runs: Vec::new(),
                                paragraphs: Vec::new(),
                                autofit: coursecraft_course::Autofit::None,
                            });
                            entry.text = props.extra_text.clone();
                            entry.runs.clear();
                        }
                        push_patch(serde_json::json!({ "kind": new_kind }), actions_to_run);
                    }
                });
            }
        }
        coursecraft_course::StateExtraKind::Text { text, style, .. } => {
            inspector_section_header(ui, "Text");
            inspector_row(ui, "Text", |ui| {
                let rem_w = ui.available_width().max(30.0);
                let r = ui.add_sized(
                    [rem_w, 56.0],
                    TextEdit::multiline(&mut props.extra_text).id_salt("prop_extra_text_body"),
                );
                if is_committed(&r, ui) && props.extra_text != *text {
                    let mut new_kind = extra.kind.clone();
                    if let coursecraft_course::StateExtraKind::Text {
                        text: ref mut t,
                        runs: ref mut rns,
                        ..
                    } = new_kind
                    {
                        *t = props.extra_text.clone();
                        rns.clear();
                    }
                    push_patch(serde_json::json!({ "kind": new_kind }), actions_to_run);
                }
            });
            inspector_row(ui, "Color", |ui| {
                let mut c = style.color.0.clone();
                if color_swatch_picker(ui, &mut c, "extra_text_color", 20.0, 4) {
                    let mut new_kind = extra.kind.clone();
                    if let coursecraft_course::StateExtraKind::Text {
                        style: ref mut st, ..
                    } = new_kind
                    {
                        st.color = Color(c);
                    }
                    push_patch(serde_json::json!({ "kind": new_kind }), actions_to_run);
                }
            });
        }
        coursecraft_course::StateExtraKind::Image { src, alt, .. } => {
            inspector_section_header(ui, "Image");
            inspector_row(ui, "File", |ui| {
                let name = src.rsplit('/').next().unwrap_or(src);
                ui.label(name).on_hover_text(src);
                if ui
                    .add(egui::Button::new("Replace…").min_size(egui::vec2(24.0, 24.0)))
                    .clicked()
                {
                    replace_image_requested = true;
                }
            });
            inspector_row(ui, "Alt text", |ui| {
                let rem_w = ui.available_width().max(30.0);
                let r = ui.add_sized(
                    [rem_w, coursecraft_render::CONTROL_COMPACT],
                    TextEdit::singleline(&mut props.extra_alt).id_salt("prop_extra_alt"),
                );
                if is_committed(&r, ui) && props.extra_alt != *alt {
                    let mut new_kind = extra.kind.clone();
                    if let coursecraft_course::StateExtraKind::Image { alt: ref mut a, .. } =
                        new_kind
                    {
                        *a = props.extra_alt.clone();
                    }
                    push_patch(serde_json::json!({ "kind": new_kind }), actions_to_run);
                }
            });
        }
        coursecraft_course::StateExtraKind::Path { path } => {
            inspector_section_header(ui, "Path");
            inspector_row(ui, "Stroke", |ui| {
                let mut c = path.outline.color.0.clone();
                if color_swatch_picker(ui, &mut c, "extra_path_stroke", 20.0, 4) {
                    let mut new_kind = extra.kind.clone();
                    if let coursecraft_course::StateExtraKind::Path { path: ref mut pt } = new_kind
                    {
                        pt.outline.color = Color(c);
                    }
                    push_patch(serde_json::json!({ "kind": new_kind }), actions_to_run);
                }
            });
        }
    }

    inspector_separator(ui);
    inspector_section_header(ui, "Effects");
    let mut has_shadow = extra.shadow.is_some();
    inspector_row(ui, "Shadow", |ui| {
        if ui.checkbox(&mut has_shadow, "Enabled").changed() {
            let val = if has_shadow {
                serde_json::json!({ "color": "#00000044", "offset_x": 0.0, "offset_y": 4.0, "blur": 8.0 })
            } else {
                Json::Null
            };
            push_patch(serde_json::json!({ "shadow": val }), actions_to_run);
        }
    });
    let mut has_glow = extra.glow.is_some();
    inspector_row(ui, "Glow", |ui| {
        if ui.checkbox(&mut has_glow, "Enabled").changed() {
            let val = if has_glow {
                serde_json::json!({ "color": "#1166CC88", "size": 8.0 })
            } else {
                Json::Null
            };
            push_patch(serde_json::json!({ "glow": val }), actions_to_run);
        }
    });
    if replace_image_requested {
        editor.pending_image_pick = Some(crate::PendingImagePick::StateExtraReplace {
            slide: slide_id.to_owned(),
            object: object_id.to_owned(),
            state: state_name.to_owned(),
            extra: extra.id.clone(),
        });
        editor.request(crate::EditorRequest::PickImage);
    }
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

    let active_state_extra = editor
        .ui
        .editing_state
        .clone()
        .filter(|s| {
            s.slide_id == editor.selection.slide
                && editor.selection.object.as_deref() == Some(s.object_id.as_str())
        })
        .and_then(|session| {
            let slide = editor.session.course.slide(&session.slide_id)?;
            let (obj, _) = slide.object(&session.object_id)?;
            let state = obj
                .states
                .iter()
                .find(|st| st.name == session.state_name)?
                .clone();
            Some((session, state))
        });

    if let Some((ref session, ref state)) = active_state_extra
        && !state.extras.is_empty()
    {
        inspector_section_header(ui, "Items in this state");
        show_state_extras_list(editor, ui, session, state, actions_to_run);
        inspector_separator(ui);
    }

    if let Some((ref session, ref state)) = active_state_extra
        && let Some(ref extra_id) = session.selected_extra
        && let Some(extra) = state.extras.iter().find(|e| e.id == *extra_id)
    {
        show_state_extra_properties(
            editor,
            ui,
            &session.slide_id,
            &session.object_id,
            &session.state_name,
            extra,
            actions_to_run,
        );
    } else {
        show_object_properties(editor, ui, actions_to_run);
    }

    if crate::question::selected_object_is_question_related(
        &editor.session.course,
        &editor.selection,
    ) {
        inspector_separator(ui);
        inspector_section_header(ui, "Question");
        crate::question::show_question_section(editor, ui, actions_to_run);
    }

    if !editor.ui.master_view {
        inspector_separator(ui);
        inspector_section_header(ui, "States");
        crate::states::show_compact_states_section(editor, ui, actions_to_run);
    }

    inspector_separator(ui);
    show_accessibility_section(editor, ui, actions_to_run);
}

fn parse_finite(value: &str) -> Option<f32> {
    let value: f32 = value.trim().parse().ok()?;
    value.is_finite().then_some(value)
}

/// A default shape text style that contrasts with the fill, centred both ways.
pub(crate) fn default_shape_text_style(fill: &Color) -> TextStyle {
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

fn format_group_state_label(state: &str) -> String {
    match state {
        "normal" => "Normal".to_owned(),
        "hidden" => "Hidden".to_owned(),
        "disabled" => "Disabled".to_owned(),
        other => other.to_owned(),
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
    let selected_group_id = is_one_group
        .then(|| objects.first().and_then(|object| object.group.clone()))
        .flatten();
    let interactive_group = selected_group_id
        .as_deref()
        .and_then(|gid| slide.interactive_group(gid))
        .map(|(g, _)| g.clone());
    let group_states: Vec<String> = selected_group_id
        .as_deref()
        .filter(|_| interactive_group.is_some())
        .map(|gid| {
            let mut states = vec![
                "normal".to_owned(),
                "hidden".to_owned(),
                "disabled".to_owned(),
            ];
            let mut extra: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
            for member in slide.group_members(gid) {
                for st in &member.states {
                    if !matches!(st.name.as_str(), "normal" | "hidden" | "disabled") {
                        extra.insert(st.name.clone());
                    }
                }
            }
            states.extend(extra);
            states
        })
        .unwrap_or_default();
    let master_view = editor.ui.master_view;
    let group_ids: Vec<String> = objects.iter().map(|object| object.id.clone()).collect();
    let font_families = editor.ui.properties_lists.font_families.clone();
    let system_fonts = editor.ui.system_fonts.as_deref();
    let mut font_requests: Vec<crate::EditorRequest> = Vec::new();
    let mut open_trigger_for_group: Option<String> = None;
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

    if let Some(ref group_id) = selected_group_id
        && !master_view
    {
        ui.add_space(coursecraft_render::SPACE_SM);
        let mut interactive = interactive_group.is_some();
        let cb = ui
            .checkbox(&mut interactive, tl!("Act as one button"))
            .on_hover_text(tl!(
                "Share hover, down, selected, visited, and disabled states across all objects in this group"
            ));
        if cb.changed() {
            actions_to_run.push((
                "group.set_interactive",
                serde_json::json!({
                    "slide": slide.id,
                    "group": group_id,
                    "interactive": interactive,
                }),
            ));
        }

        if let Some(ref group) = interactive_group {
            ui.add_space(coursecraft_render::SPACE_SM);

            inspector_row(ui, tl!("Name"), |ui| {
                let rem_w = ui.available_width().max(30.0);
                let r = ui.add_sized(
                    [rem_w, coursecraft_render::CONTROL_COMPACT],
                    TextEdit::singleline(&mut props.group_name).id_salt("prop_group_name"),
                );
                props.group_name_rect = Some(r.rect);
                if is_committed(&r, ui) && props.group_name != group.name {
                    actions_to_run.push((
                        "group.update",
                        serde_json::json!({
                            "slide": slide.id,
                            "group": group_id,
                            "name": props.group_name,
                        }),
                    ));
                }
            });

            inspector_row(ui, tl!("Initial state"), |ui| {
                let rem_w = ui.available_width().max(30.0);
                let mut current_state = group.initial_state.clone();
                crate::widgets::styled_combo(ui, "prop_group_initial_state", rem_w)
                    .selected_text(format_group_state_label(&current_state))
                    .show_ui(ui, |ui| {
                        for state in &group_states {
                            let label = format_group_state_label(state);
                            if ui
                                .selectable_value(&mut current_state, state.clone(), label)
                                .clicked()
                                && current_state != group.initial_state
                            {
                                actions_to_run.push((
                                    "group.update",
                                    serde_json::json!({
                                        "slide": slide.id,
                                        "group": group_id,
                                        "initial_state": state,
                                    }),
                                ));
                            }
                        }
                    });
            });

            inspector_row(ui, tl!("Accessible description"), |ui| {
                let rem_w = ui.available_width().max(30.0);
                let r = ui.add_sized(
                    [rem_w, coursecraft_render::CONTROL_COMPACT],
                    TextEdit::singleline(&mut props.group_description)
                        .id_salt("prop_group_description"),
                );
                props.group_description_rect = Some(r.rect);
                let current = group.description.as_deref().unwrap_or("");
                if is_committed(&r, ui) && props.group_description.trim() != current {
                    let value = if props.group_description.trim().is_empty() {
                        Json::Null
                    } else {
                        Json::String(props.group_description.trim().to_owned())
                    };
                    actions_to_run.push((
                        "group.update",
                        serde_json::json!({
                            "slide": slide.id,
                            "group": group_id,
                            "description": value,
                        }),
                    ));
                }
            });

            inspector_row(ui, tl!("Focus order"), |ui| {
                let mut order = props.group_focus_order.unwrap_or(0);
                let r = ui.add(DragValue::new(&mut order).speed(1.0));
                if r.changed() {
                    props.group_focus_order = Some(order);
                }
                let enter = r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if (r.drag_stopped() || r.lost_focus() || enter)
                    && props.group_focus_order != group.focus_order
                {
                    let committed = props.group_focus_order.unwrap_or(order);
                    props.group_focus_order = Some(committed);
                    actions_to_run.push((
                        "group.update",
                        serde_json::json!({
                            "slide": slide.id,
                            "group": group_id,
                            "focus_order": committed,
                        }),
                    ));
                }
                if ui
                    .add_enabled(
                        props.group_focus_order.is_some(),
                        egui::Button::new(tl!("Clear"))
                            .min_size(vec2(24.0, coursecraft_render::CONTROL_COMPACT)),
                    )
                    .on_disabled_hover_text(tl!("No focus order set"))
                    .clicked()
                {
                    props.group_focus_order = None;
                    actions_to_run.push((
                        "group.update",
                        serde_json::json!({
                            "slide": slide.id,
                            "group": group_id,
                            "focus_order": Json::Null,
                        }),
                    ));
                }
            });

            ui.add_space(coursecraft_render::SPACE_SM);
            if ui
                .add(
                    egui::Button::new(tl!("Add trigger…"))
                        .min_size(vec2(0.0, coursecraft_render::CONTROL_COMPACT)),
                )
                .clicked()
            {
                open_trigger_for_group = Some(group_id.clone());
            }
        }
    }

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

        inspector_row(ui, "Font", |ui| {
            let label = if props.multi_font_mixed {
                "Mixed"
            } else if props.multi_font.is_empty() {
                "Inter"
            } else {
                props.multi_font.as_str()
            };
            if let Some(picked) = crate::font_picker::show_font_picker(
                ui,
                "multi_text",
                label,
                &font_families,
                &mut props.recent_fonts,
                system_fonts,
                &mut props.system_fonts_list_requested,
                &mut props.system_font_search,
                &mut font_requests,
            ) {
                match picked {
                    crate::font_picker::FontPickerAction::PickCourseOrRecent(name) => {
                        if props.multi_font_mixed || props.multi_font != name {
                            props.multi_font = name.clone();
                            props.multi_font_mixed = false;
                            for object in &objects {
                                actions_to_run.push((
                                    "text.style_range",
                                    serde_json::json!({
                                        "slide": slide.id,
                                        "object": object.id,
                                        "start": 0,
                                        "end": 0,
                                        "font": name,
                                    }),
                                ));
                            }
                        }
                    }
                    crate::font_picker::FontPickerAction::PickSystem(name) => {
                        props.multi_font = name.clone();
                        props.multi_font_mixed = false;
                        font_requests.push(crate::EditorRequest::AddSystemFont {
                            family: name.clone(),
                            apply: None,
                        });
                        for object in &objects {
                            actions_to_run.push((
                                "text.style_range",
                                serde_json::json!({
                                    "slide": slide.id,
                                    "object": object.id,
                                    "start": 0,
                                    "end": 0,
                                    "font": name,
                                }),
                            ));
                        }
                    }
                    crate::font_picker::FontPickerAction::AddFont => {
                        props.add_font_requested = true;
                    }
                }
            }
        });

        inspector_row(ui, "Style", |ui| {
            let bold_tip = crate::menu::tooltip_with_shortcut(
                ui.ctx(),
                "Bold",
                Some(crate::shortcuts::shortcut_for_action(
                    crate::shortcuts::ShortcutAction::Bold,
                )),
            );
            let bold = props.multi_bold;
            if crate::widgets::mixed_toggle(ui, RichText::new("B").strong(), bold, &bold_tip)
                .clicked()
            {
                let value = bold != Some(true);
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
            let italic_tip = crate::menu::tooltip_with_shortcut(
                ui.ctx(),
                "Italic",
                Some(crate::shortcuts::shortcut_for_action(
                    crate::shortcuts::ShortcutAction::Italic,
                )),
            );
            let italic = props.multi_italic;
            if crate::widgets::mixed_toggle(ui, RichText::new("I").italics(), italic, &italic_tip)
                .clicked()
            {
                let value = italic != Some(true);
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
            let underline_tip = crate::menu::tooltip_with_shortcut(
                ui.ctx(),
                "Underline",
                Some(crate::shortcuts::shortcut_for_action(
                    crate::shortcuts::ShortcutAction::Underline,
                )),
            );
            let underline = props.multi_underline;
            if crate::widgets::mixed_toggle(
                ui,
                RichText::new("U").underline(),
                underline,
                &underline_tip,
            )
            .clicked()
            {
                let value = underline != Some(true);
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
            let mixed = props.multi_text_size_mixed;
            let mut size_val = if mixed {
                16.0
            } else {
                parse_finite(&props.multi_text_size).unwrap_or(16.0)
            };
            let r = ui.add(
                DragValue::new(&mut size_val)
                    .speed(0.5)
                    .range(1.0..=400.0)
                    .custom_formatter(move |v, _| {
                        if mixed {
                            "Mixed".to_owned()
                        } else {
                            format!("{v:.0} pt")
                        }
                    }),
            );
            if r.changed() {
                props.multi_text_size = format!("{size_val}");
                props.multi_text_size_mixed = false;
            }
            let enter = r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (r.drag_stopped() || r.lost_focus() || enter) && !props.multi_text_size_mixed {
                for object in &objects {
                    let needs = matches!(
                        &object.kind,
                        ObjectKind::Text { style, runs, .. }
                            if (style.size - size_val).abs() >= 0.001
                                || runs.iter().any(|run| run.size.is_some_and(|s| (s - size_val).abs() >= 0.001))
                    );
                    if needs {
                        actions_to_run.push((
                            "text.style_range",
                            serde_json::json!({
                                "slide": slide.id,
                                "object": object.id,
                                "start": 0,
                                "end": 0,
                                "size": size_val,
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
                (Align::Justify, "Justify"),
            ] {
                if ui.selectable_label(current == Some(align), label).clicked() {
                    let name = match align {
                        Align::Left => "left",
                        Align::Center => "center",
                        Align::Right => "right",
                        Align::Justify => "justify",
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
        inspector_row(ui, "Fill", |ui| {
            let mut checked = props.multi_fill_enabled == Some(true);
            let r = ui.add(
                egui::Checkbox::new(&mut checked, "")
                    .indeterminate(props.multi_fill_enabled.is_none()),
            );
            if r.changed() {
                props.multi_fill_enabled = Some(checked);
                if !checked {
                    for object in &objects {
                        let (mut kind, fill_color, fill_style) = match &object.kind {
                            ObjectKind::Shape {
                                fill, fill_style, ..
                            }
                            | ObjectKind::Button {
                                fill, fill_style, ..
                            } if fill.0 != "#00000000" => (
                                serde_json::to_value(&object.kind).unwrap_or(Json::Null),
                                fill.0.as_str(),
                                fill_style.clone(),
                            ),
                            _ => continue,
                        };
                        props.remember_fill(&slide.id, &object.id, None, fill_color, fill_style);
                        kind["fill"] = serde_json::json!("#00000000");
                        kind["fill_style"] = Json::Null;
                        actions_to_run.push((
                            "object.update",
                            serde_json::json!({
                                "slide": slide.id,
                                "object": object.id,
                                "kind": kind,
                            }),
                        ));
                    }
                } else {
                    for object in &objects {
                        let (mut kind, default_color) = match &object.kind {
                            ObjectKind::Shape { fill, .. } if fill.0 == "#00000000" => (
                                serde_json::to_value(&object.kind).unwrap_or(Json::Null),
                                "#D0D7DE",
                            ),
                            ObjectKind::Button { fill, .. } if fill.0 == "#00000000" => (
                                serde_json::to_value(&object.kind).unwrap_or(Json::Null),
                                "#2F6FEB",
                            ),
                            _ => continue,
                        };
                        let (next_color, next_style) = props.restore_fill(
                            &slide.id,
                            &object.id,
                            None,
                            &props.multi_fill,
                            default_color,
                        );
                        kind["fill"] = serde_json::json!(next_color);
                        kind["fill_style"] = fill_style_json(&next_style);
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
            if props.multi_fill_enabled != Some(false) {
                let (fill_response, valid_fill) =
                    show_color_field(ui, "", &mut props.multi_fill, "prop_multi_fill");
                if is_committed(&fill_response, ui)
                    && valid_fill
                    && !props.multi_fill.trim().is_empty()
                {
                    let fill = props.multi_fill.trim().to_owned();
                    for object in &objects {
                        let (kind, needs) = match &object.kind {
                            ObjectKind::Shape { fill: current, .. }
                            | ObjectKind::Button { fill: current, .. } => {
                                let mut kind =
                                    serde_json::to_value(&object.kind).unwrap_or(Json::Null);
                                kind["fill"] = serde_json::json!(fill);
                                (kind, current.0 != fill)
                            }
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
            }
        });
        let all_shapes = objects
            .iter()
            .all(|object| matches!(object.kind, ObjectKind::Shape { .. }));
        if all_shapes {
            inspector_separator(ui);
            let mut outline_changed = false;
            inspector_row(ui, "Outline", |ui| {
                let mut checked = props.multi_outline_enabled == Some(true);
                let r = ui.add(
                    egui::Checkbox::new(&mut checked, "")
                        .indeterminate(props.multi_outline_enabled.is_none()),
                );
                if r.changed() {
                    props.shape_outline_enabled = checked;
                    props.multi_outline_enabled = Some(checked);
                    outline_changed = true;
                }
            });
            if props.multi_outline_enabled != Some(false) {
                inspector_row(ui, "Colour", |ui| {
                    let (cr, valid_color) = show_color_field(
                        ui,
                        "",
                        &mut props.shape_outline_color,
                        "prop_multi_shape_outline_color",
                    );
                    if is_committed(&cr, ui) && valid_color {
                        outline_changed = true;
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
                        outline_changed = true;
                    }
                });
                if show_dash_controls(
                    ui,
                    "prop_multi_shape_outline",
                    &mut props.shape_outline_dash,
                    &mut props.shape_outline_dash_pattern,
                    &mut props.shape_outline_dash_length,
                    &mut props.shape_outline_gap,
                ) {
                    outline_changed = true;
                }
            }
            if outline_changed {
                let out_json = if props.shape_outline_enabled {
                    outline_json(
                        &props.shape_outline_color,
                        props.shape_outline_width,
                        props.shape_outline_dash,
                        &props.shape_outline_dash_pattern,
                        props.shape_outline_join,
                    )
                } else {
                    Json::Null
                };
                for object in &objects {
                    let mut kind = serde_json::to_value(&object.kind).unwrap_or(Json::Null);
                    kind["outline"] = out_json.clone();
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
    }

    inspector_separator(ui);
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
            for object in &objects {
                if (opacity - object.opacity).abs() > 0.001 {
                    actions_to_run.push((
                        "object.set_opacity",
                        serde_json::json!({
                            "slide": slide.id,
                            "object": object.id,
                            "opacity": opacity,
                        }),
                    ));
                }
            }
        }
    });

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
            if crate::widgets::icon_button(ui, icon, label).clicked() {
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
    editor.pending_requests.append(&mut font_requests);
    if let Some(group_id) = open_trigger_for_group {
        let slide_clone = slide.clone();
        let selected_layer = editor.selection.layer.clone();
        editor.ui.trigger_form = Some(
            crate::triggers::TriggerFormState::new_for_owner(
                &slide_clone,
                selected_layer.as_deref(),
                &editor.session.course,
            )
            .with_selected_object(&slide_clone, Some(&group_id)),
        );
    }
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
                let id = ui.make_persistent_id(("prop_focus_order", obj.id.as_str()));
                let current = props.focus_order.unwrap_or(0);
                let (_, committed) =
                    crate::grid_panel::committed_drag(ui, id, current, |d| d.speed(1.0));
                if let Some(order) = committed {
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
                    .on_disabled_hover_text("No focus order set")
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
    let state_edit_locked = editor.ui.editing_state.is_some();
    if state_edit_locked {
        ui.label(RichText::new(crate::STATE_LOCK_TOOLTIP).color(p.text_secondary));
        ui.add_space(6.0);
    }

    ui.add_enabled_ui(!state_edit_locked, |ui| {
        let selected_group = editor.selected_group_for_animation();
        if let Some(gid) = selected_group {
            show_group_animations_section(editor, ui, &gid, actions_to_run);
            inspector_separator(ui);
        } else if editor.selection.is_multi() {
            let slide_id = editor.selection.slide.clone();
            let ids = editor.selection.ids();
            auto_preview_toggle(ui, &mut editor.ui.anim_preview.auto);
            let props = &mut editor.ui.properties;
            let pv = &mut editor.ui.anim_preview;
            let mut entrance_action: Option<(&'static str, Json)> = None;
            let mut exit_action: Option<(&'static str, Json)> = None;
            let (mut entrance_preview, mut exit_preview) = (None, None);
            show_anim_row(
                ui,
                "Entrance animation",
                "entrance",
                &mut props.entrance_effect,
                &mut props.entrance_fx,
                &mut props.entrance_duration_ms,
                &mut props.entrance_easing,
                (&mut props.entrance_sequence, true),
                &slide_id,
                "",
                &mut entrance_action,
                pv,
                &mut entrance_preview,
            );
            inspector_separator(ui);
            show_anim_row(
                ui,
                "Exit animation",
                "exit",
                &mut props.exit_effect,
                &mut props.exit_fx,
                &mut props.exit_duration_ms,
                &mut props.exit_easing,
                (&mut props.exit_sequence, true),
                &slide_id,
                "",
                &mut exit_action,
                pv,
                &mut exit_preview,
            );
            run_card_preview(editor, ui.ctx(), false, entrance_preview);
            run_card_preview(editor, ui.ctx(), true, exit_preview);
            let props = &mut editor.ui.properties;
            if entrance_action.is_some() {
                for id in &ids {
                    actions_to_run.push(with_sequence(
                        animation_command(
                            "entrance",
                            &props.entrance_effect,
                            &props.entrance_fx,
                            props.entrance_duration_ms,
                            props.entrance_easing,
                            &slide_id,
                            id,
                        ),
                        props.entrance_sequence,
                    ));
                }
            }
            if exit_action.is_some() {
                for id in &ids {
                    actions_to_run.push(with_sequence(
                        animation_command(
                            "exit",
                            &props.exit_effect,
                            &props.exit_fx,
                            props.exit_duration_ms,
                            props.exit_easing,
                            &slide_id,
                            id,
                        ),
                        props.exit_sequence,
                    ));
                }
            }
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
    })
    .response
    .on_disabled_hover_text(crate::STATE_LOCK_TOOLTIP);
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
                let rem_w = ui.available_width().max(30.0);
                crate::widgets::styled_combo(ui, "prop_master", rem_w)
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

    let has_question = editor
        .session
        .course
        .slide(&slide_id)
        .is_some_and(|slide| slide.question.is_some());
    if has_question {
        inspector_section_header(ui, "Question");
        crate::question::show_question_section(editor, ui, actions_to_run);
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
            let rem_w = ui.available_width().max(30.0);
            crate::widgets::styled_combo(ui, "prop_draw_bank", rem_w)
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
        let mut count_rect = None;
        inspector_row(ui, "Count", |ui| {
            let id = ui.make_persistent_id("prop_draw_count");
            let (resp, committed) = crate::grid_panel::committed_drag(ui, id, draw.count, |d| {
                d.speed(1.0).range(1..=max_count)
            });
            count_rect = Some(resp.rect);
            if let Some(count) = committed {
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
    let show_notes = !crate::panels::is_shown(editor, crate::dock::Panel::Notes);
    if let Some(act) = show_slide_properties(
        &mut editor.ui.properties,
        slide,
        ui,
        is_master,
        &mut narrate_requested,
        &bool_names,
        show_notes,
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
    let slide_size = editor.session.course.slide_size;
    let ratio = crate::slide_size_dialog::format_aspect_ratio(slide_size.w, slide_size.h);
    inspector_row(ui, "Slide size", |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("{} × {} px ({ratio})", slide_size.w, slide_size.h));
            if ui.button("Change…").clicked() {
                editor.open_slide_size_dialog();
            }
        });
    });
    if let Some(q) = quiz {
        inspector_row(ui, "Pass mark", |ui| {
            ui.label(format!("{}%", q.passing_percent));
        });
    }
    inspector_row(ui, tl!("Tracking"), |ui| {
        ui.add(
            egui::Label::new(crate::tracking_ui::tracking_summary(&editor.session.course)).wrap(),
        );
    });

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

fn show_autofit_row(
    ui: &mut Ui,
    slide_id: &str,
    obj_id: &str,
    id_prefix: &str,
    autofit: Autofit,
) -> Option<(&'static str, Json)> {
    let mut action_to_run = None;
    let allow_grow = !id_prefix.starts_with("shape");
    inspector_row(ui, tl!("Autofit"), |ui| {
        let selected = match autofit {
            Autofit::None => tl!("Off"),
            Autofit::Shrink => tl!("Shrink text on overflow"),
            Autofit::Grow => tl!("Resize box to fit text"),
        };
        let mut chosen = None;
        let rem_w = ui.available_width().max(30.0);
        crate::widgets::styled_combo(ui, ("autofit", id_prefix, obj_id), rem_w)
            .selected_text(selected)
            .show_ui(ui, |ui| {
                if ui
                    .selectable_label(autofit == Autofit::None, tl!("Off"))
                    .clicked()
                {
                    chosen = Some(Autofit::None);
                }
                if ui
                    .selectable_label(autofit == Autofit::Shrink, tl!("Shrink text on overflow"))
                    .clicked()
                {
                    chosen = Some(Autofit::Shrink);
                }
                if allow_grow
                    && ui
                        .selectable_label(autofit == Autofit::Grow, tl!("Resize box to fit text"))
                        .clicked()
                {
                    chosen = Some(Autofit::Grow);
                }
            });
        if let Some(value) = chosen
            && value != autofit
        {
            let autofit_str = match value {
                Autofit::None => "none",
                Autofit::Shrink => "shrink",
                Autofit::Grow => "grow",
            };
            action_to_run = Some((
                "object.set_autofit",
                serde_json::json!({
                    "slide": slide_id,
                    "object": obj_id,
                    "autofit": autofit_str,
                }),
            ));
        }
    });
    action_to_run
}

fn show_text_box_rows(
    ui: &mut Ui,
    slide_id: &str,
    obj_id: &str,
    style: &TextStyle,
    point_text: bool,
) -> Option<(&'static str, Json)> {
    let mut action_to_run = None;
    inspector_row(ui, tl!("VAlign"), |ui| {
        let cur_valign = style.valign;
        for (valign, name) in [
            (VAlign::Top, "top"),
            (VAlign::Middle, "middle"),
            (VAlign::Bottom, "bottom"),
        ] {
            let label = match valign {
                VAlign::Top => tl!("Top"),
                VAlign::Middle => tl!("Middle"),
                VAlign::Bottom => tl!("Bottom"),
            };
            if ui.selectable_label(cur_valign == valign, label).clicked() && cur_valign != valign {
                action_to_run = Some((
                    "text.set_paragraph",
                    serde_json::json!({ "slide": slide_id, "object": obj_id, "valign": name }),
                ));
            }
        }
    });

    inspector_row(ui, tl!("Wrap"), |ui| {
        let mut wrap = style.wrap;
        let resp = ui.checkbox(&mut wrap, tl!("Wrap text"));
        let resp = if point_text {
            resp.on_hover_text(tl!(
                "Fits width to text. Turn on wrap or drag a side handle for fixed width."
            ))
        } else {
            resp
        };
        if resp.changed() {
            action_to_run = Some((
                "text.set_paragraph",
                serde_json::json!({ "slide": slide_id, "object": obj_id, "wrap": wrap }),
            ));
        }
    });

    inspector_row(ui, tl!("Direction"), |ui| {
        let cur_dir = style.direction;
        for (dir, name, label) in [
            (
                coursecraft_course::TextDirection::Horizontal,
                "horizontal",
                tl!("Horiz"),
            ),
            (
                coursecraft_course::TextDirection::Vertical,
                "vertical",
                tl!("Vert 90"),
            ),
            (
                coursecraft_course::TextDirection::Vertical270,
                "vertical270",
                tl!("Vert 270"),
            ),
        ] {
            if ui.selectable_label(cur_dir == dir, label).clicked() && cur_dir != dir {
                action_to_run = Some((
                    "text.set_paragraph",
                    serde_json::json!({ "slide": slide_id, "object": obj_id, "direction": name }),
                ));
            }
        }
    });

    inspector_row(ui, tl!("Insets"), |ui| {
        let mut ins = style.insets.unwrap_or_default();
        let mut changed = false;
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            for pair in [
                [("L", &mut ins.left), ("T", &mut ins.top)],
                [("R", &mut ins.right), ("B", &mut ins.bottom)],
            ] {
                ui.horizontal(|ui| {
                    for (label, val) in pair {
                        ui.label(label);
                        // The insets come from the document every frame: edit a buffer and
                        // commit once, so a drag or a typed number is one undo step.
                        let id = ui.make_persistent_id(("prop_text_inset", obj_id, label));
                        let (_, committed) = crate::grid_panel::committed_drag(ui, id, *val, |d| {
                            d.speed(0.5)
                                .range(0.0..=200.0)
                                .max_decimals(1)
                                .suffix(" px")
                        });
                        if let Some(v) = committed {
                            *val = v;
                            changed = true;
                        }
                    }
                });
            }
        });
        if changed {
            action_to_run = Some((
                "text.set_paragraph",
                serde_json::json!({ "slide": slide_id, "object": obj_id, "insets": ins }),
            ));
        }
    });
    action_to_run
}

fn show_text_box_controls(
    ui: &mut Ui,
    slide_id: &str,
    obj_id: &str,
    style: &TextStyle,
    id_prefix: &str,
    autofit: Option<Autofit>,
    point_text: bool,
) -> Option<(&'static str, Json)> {
    let mut action_to_run = None;
    if let Some(autofit) = autofit
        && let Some(action) = show_autofit_row(ui, slide_id, obj_id, id_prefix, autofit)
    {
        action_to_run = Some(action);
    }
    if let Some(action) = show_text_box_rows(ui, slide_id, obj_id, style, point_text) {
        action_to_run = Some(action);
    }
    action_to_run
}

/// Overrides the toolbar's tri-states (bold, italic, underline, strike, super, sub)
/// with what a pending caret style sets.
fn apply_pending_tri(
    patch: &coursecraft_course::TextRunPatch,
    [bold, italic, underline, strike, sup, sub]: [&mut Option<bool>; 6],
) {
    if let Some(b) = patch.bold {
        *bold = Some(b);
    }
    if let Some(it) = patch.italic {
        *italic = Some(it);
    }
    if let Some(u) = patch.underline {
        *underline = Some(u);
    }
    if let Some(st) = patch.strikethrough {
        *strike = Some(st);
    }
    if let Some(base) = patch.baseline {
        *sup = Some(base == coursecraft_course::Baseline::Superscript);
        *sub = Some(base == coursecraft_course::Baseline::Subscript);
    }
}

/// A pending-caret-style setter: `(props, caret, patch edit)`.
type SetPending<'a> =
    dyn Fn(&mut PropertiesState, usize, &dyn Fn(&mut coursecraft_course::TextRunPatch)) + 'a;

/// The Text panel row with the Shadow toggle, the highlight swatch and No Color.
/// Returns the command to run for a range; at a caret it records a pending style instead.
fn show_shadow_highlight_row(
    ui: &mut Ui,
    props: &mut PropertiesState,
    (tri_shadow, run_highlight): (Option<bool>, Option<String>),
    caret: Option<usize>,
    char_range: (usize, usize),
    (slide_id, obj_id): (&str, &str),
    set_pending: &SetPending<'_>,
) -> Option<(&'static str, serde_json::Value)> {
    let mut action_to_run = None;
    inspector_row(ui, "Highlight", |ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        if crate::widgets::mixed_toggle(ui, "Sh", tri_shadow, "Text shadow").clicked() {
            let next = tri_shadow != Some(true);
            if let Some(pos) = caret {
                set_pending(props, pos, &|p| p.shadow = Some(next));
            } else {
                action_to_run = Some((
                    "text.style_range",
                    serde_json::json!({
                        "slide": slide_id,
                        "object": obj_id,
                        "start": char_range.0,
                        "end": char_range.1,
                        "shadow": next,
                    }),
                ));
            }
        }
        let mut hex = run_highlight.clone().unwrap_or_default();
        let picked = color_swatch_picker(ui, &mut hex, "prop_run_highlight", 18.0, 4);
        let none = ui
            .add_enabled(
                run_highlight.is_some(),
                egui::Button::new(tl!("No Color")).small(),
            )
            .on_disabled_hover_text(tl!("The text has no highlight."))
            .clicked();
        let next: Option<Option<String>> = if none {
            Some(None)
        } else if picked && parse_color(&Color(hex.trim().to_owned())).is_some() {
            Some(Some(hex.trim().to_owned()))
        } else {
            None
        };
        if let Some(next) = next {
            if let Some(pos) = caret {
                set_pending(props, pos, &|p| {
                    p.highlight = Some(next.clone().map(Color));
                });
            } else {
                action_to_run = Some((
                    "text.style_range",
                    serde_json::json!({
                        "slide": slide_id,
                        "object": obj_id,
                        "start": char_range.0,
                        "end": char_range.1,
                        "highlight": next,
                    }),
                ));
            }
        }
    });
    action_to_run
}

/// The shadow state and highlight colour the text controls show: from the runs in `range`,
/// overridden by a pending caret style at `caret`.
fn shadow_and_highlight(
    runs: &[TextRun],
    range: (usize, usize),
    caret: Option<usize>,
    props: &PropertiesState,
    slide_id: &str,
    obj_id: &str,
    cell: Option<(usize, usize)>,
) -> (Option<bool>, Option<String>) {
    let mut tri_shadow = crate::text_format::shadow_for_range(runs, range);
    let mut run_highlight = crate::text_format::highlight_for_range(runs, range);
    if let Some(pos) = caret
        && let Some(pending) = props.pending_caret_style.as_ref().filter(|p| {
            p.slide == slide_id && p.object == obj_id && p.cell == cell && p.cursor == pos
        })
    {
        tri_shadow = pending.patch.shadow.or(tri_shadow);
        if let Some(highlight) = &pending.patch.highlight {
            run_highlight = highlight.as_ref().map(|c| c.0.clone());
        }
    }
    (tri_shadow, run_highlight)
}

/// Turns a font-picker choice in the text controls into the command to run (or a request).
fn font_pick_action(
    picked: crate::font_picker::FontPickerAction,
    shown_font: &str,
    slide_id: &str,
    obj_id: &str,
    char_range: (usize, usize),
    props: &mut PropertiesState,
    font_requests: &mut Vec<crate::EditorRequest>,
) -> Option<(&'static str, serde_json::Value)> {
    let params = |name: &str| {
        serde_json::json!({
            "slide": slide_id,
            "object": obj_id,
            "start": char_range.0,
            "end": char_range.1,
            "font": name,
        })
    };
    match picked {
        crate::font_picker::FontPickerAction::PickCourseOrRecent(name) => {
            (shown_font != name).then(|| ("text.style_range", params(&name)))
        }
        crate::font_picker::FontPickerAction::PickSystem(name) => {
            font_requests.push(crate::EditorRequest::AddSystemFont {
                apply: Some(("text.style_range".to_owned(), params(&name))),
                family: name,
            });
            None
        }
        crate::font_picker::FontPickerAction::AddFont => {
            props.add_font_requested = true;
            None
        }
    }
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
    point_text: bool,
    include_box_controls: bool,
    system_fonts: Option<&[crate::SystemFontChoice]>,
    font_requests: &mut Vec<crate::EditorRequest>,
) -> Option<(&'static str, Json)> {
    let mut action_to_run = None;
    let text_edit_id = egui::Id::unique(("prop_text_body", id_prefix));
    let text_edit_state = egui::TextEdit::load_state(ui.ctx(), text_edit_id);
    let cursor_range = text_edit_state.as_ref().and_then(|s| s.cursor.char_range());
    let text_edit_has_focus = ui.ctx().memory(|m| m.has_focus(text_edit_id));
    enum TextStyleTarget {
        Selection(usize, usize),
        Caret(usize),
        WholeObject,
    }
    let target = if let Some(cr) = cursor_range
        && !cr.is_empty()
    {
        let sorted = cr.as_sorted_char_range();
        TextStyleTarget::Selection(sorted.start.0, sorted.end.0)
    } else if let Some((_, _, inline_range, inline_cursor)) = props
        .inline_selection
        .as_ref()
        .filter(|(s, o, _, _)| s == slide_id && o == obj_id)
    {
        if let Some((s, e)) = inline_range
            && s != e
        {
            TextStyleTarget::Selection((*s).min(*e), (*s).max(*e))
        } else {
            TextStyleTarget::Caret(*inline_cursor)
        }
    } else if let Some(cr) = cursor_range
        && text_edit_has_focus
    {
        TextStyleTarget::Caret(cr.primary.index.0)
    } else {
        TextStyleTarget::WholeObject
    };

    let (char_range, cursor_pos): ((usize, usize), usize) = match target {
        TextStyleTarget::Selection(s, e) => ((s, e), s),
        TextStyleTarget::Caret(pos) => ((0, 0), pos),
        TextStyleTarget::WholeObject => ((0, 0), 0),
    };

    let total_chars = current_text.chars().count();
    let (eff_bold, eff_italic, eff_underline, eff_size, eff_color) = match target {
        TextStyleTarget::Selection(s, e) => style_for_range(runs, style, (s, e)),
        TextStyleTarget::Caret(pos) => style_at_cursor(runs, style, pos),
        TextStyleTarget::WholeObject => style_for_range(runs, style, (0, total_chars)),
    };
    let (eff_strike, eff_baseline, eff_spacing) = match target {
        TextStyleTarget::Selection(s, e) => effects_for_range(runs, style, (s, e)),
        TextStyleTarget::Caret(pos) => {
            let (st, base, sp) = effects_at_cursor(runs, style, pos);
            (st, Some(base), sp)
        }
        TextStyleTarget::WholeObject => effects_for_range(runs, style, (0, total_chars)),
    };
    let (mut tri_bold, mut tri_italic, mut tri_underline) = match target {
        TextStyleTarget::Selection(s, e) => style_tri_for_range(runs, style, (s, e)),
        TextStyleTarget::Caret(_) => (Some(eff_bold), Some(eff_italic), Some(eff_underline)),
        TextStyleTarget::WholeObject => style_tri_for_range(runs, style, (0, total_chars)),
    };
    let (mut tri_strike, mut tri_super, mut tri_sub) = match target {
        TextStyleTarget::Selection(s, e) => effects_tri_for_range(runs, style, (s, e)),
        TextStyleTarget::Caret(_) => (
            Some(eff_strike),
            Some(eff_baseline == Some(coursecraft_course::Baseline::Superscript)),
            Some(eff_baseline == Some(coursecraft_course::Baseline::Subscript)),
        ),
        TextStyleTarget::WholeObject => effects_tri_for_range(runs, style, (0, total_chars)),
    };

    if let TextStyleTarget::Caret(pos) = target {
        let matches_pending = props.pending_caret_style.as_ref().is_some_and(|p| {
            p.slide == slide_id && p.object == obj_id && p.cell == cell && p.cursor == pos
        });
        if matches_pending {
            if let Some(pending) = &props.pending_caret_style {
                apply_pending_tri(
                    &pending.patch,
                    [
                        &mut tri_bold,
                        &mut tri_italic,
                        &mut tri_underline,
                        &mut tri_strike,
                        &mut tri_super,
                        &mut tri_sub,
                    ],
                );
            }
        } else if props
            .pending_caret_style
            .as_ref()
            .is_some_and(|p| p.slide == slide_id && p.object == obj_id && p.cell == cell)
        {
            props.pending_caret_style = None;
        }
    } else if props
        .pending_caret_style
        .as_ref()
        .is_some_and(|p| p.slide == slide_id && p.object == obj_id && p.cell == cell)
    {
        props.pending_caret_style = None;
    }

    // Font picker: Inter plus every course family, each name drawn in its own
    // face. "Add font…" opens the same picker the File menu uses.
    let current_font = font_at_cursor(runs, cursor_pos).unwrap_or_else(|| style.font.clone());
    let shown_font = crate::font_picker::displayed_font(&current_font);
    inspector_row(ui, "Font", |ui| {
        if let Some(picked) = crate::font_picker::show_font_picker(
            ui,
            id_prefix,
            &shown_font,
            font_families,
            &mut props.recent_fonts,
            system_fonts,
            &mut props.system_fonts_list_requested,
            &mut props.system_font_search,
            font_requests,
        ) && let Some(action) = font_pick_action(
            picked,
            &shown_font,
            slide_id,
            obj_id,
            char_range,
            props,
            font_requests,
        ) {
            action_to_run = Some(action);
        }
    });

    if include_box_controls
        && let Some(autofit) = autofit
        && let Some(action) = show_autofit_row(ui, slide_id, obj_id, id_prefix, autofit)
    {
        action_to_run = Some(action);
    }

    // Formatting toolbar
    let set_pending =
        |props: &mut PropertiesState,
         pos: usize,
         apply: &dyn Fn(&mut coursecraft_course::TextRunPatch)| {
            let entry = props
                .pending_caret_style
                .get_or_insert_with(|| PendingCaretStyle {
                    slide: slide_id.to_owned(),
                    object: obj_id.to_owned(),
                    cell,
                    cursor: pos,
                    patch: coursecraft_course::TextRunPatch::default(),
                });
            entry.slide = slide_id.to_owned();
            entry.object = obj_id.to_owned();
            entry.cell = cell;
            entry.cursor = pos;
            apply(&mut entry.patch);
        };
    inspector_row(ui, "Style", |ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        let bold_tip = crate::menu::tooltip_with_shortcut(
            ui.ctx(),
            "Bold",
            Some(crate::shortcuts::shortcut_for_action(
                crate::shortcuts::ShortcutAction::Bold,
            )),
        );
        if crate::widgets::mixed_toggle(ui, RichText::new("B").strong(), tri_bold, &bold_tip)
            .clicked()
        {
            let next = tri_bold != Some(true);
            if let TextStyleTarget::Caret(pos) = target {
                set_pending(props, pos, &|p| p.bold = Some(next));
            } else {
                action_to_run = Some((
                    "text.style_range",
                    serde_json::json!({
                        "slide": slide_id,
                        "object": obj_id,
                        "start": char_range.0,
                        "end": char_range.1,
                        "bold": next,
                    }),
                ));
            }
        }
        let italic_tip = crate::menu::tooltip_with_shortcut(
            ui.ctx(),
            "Italic",
            Some(crate::shortcuts::shortcut_for_action(
                crate::shortcuts::ShortcutAction::Italic,
            )),
        );
        if crate::widgets::mixed_toggle(ui, RichText::new("I").italics(), tri_italic, &italic_tip)
            .clicked()
        {
            let next = tri_italic != Some(true);
            if let TextStyleTarget::Caret(pos) = target {
                set_pending(props, pos, &|p| p.italic = Some(next));
            } else {
                action_to_run = Some((
                    "text.style_range",
                    serde_json::json!({
                        "slide": slide_id,
                        "object": obj_id,
                        "start": char_range.0,
                        "end": char_range.1,
                        "italic": next,
                    }),
                ));
            }
        }
        let underline_tip = crate::menu::tooltip_with_shortcut(
            ui.ctx(),
            "Underline",
            Some(crate::shortcuts::shortcut_for_action(
                crate::shortcuts::ShortcutAction::Underline,
            )),
        );
        if crate::widgets::mixed_toggle(
            ui,
            RichText::new("U").underline(),
            tri_underline,
            &underline_tip,
        )
        .clicked()
        {
            let next = tri_underline != Some(true);
            if let TextStyleTarget::Caret(pos) = target {
                set_pending(props, pos, &|p| p.underline = Some(next));
            } else {
                action_to_run = Some((
                    "text.style_range",
                    serde_json::json!({
                        "slide": slide_id,
                        "object": obj_id,
                        "start": char_range.0,
                        "end": char_range.1,
                        "underline": next,
                    }),
                ));
            }
        }

        if crate::widgets::mixed_toggle(
            ui,
            RichText::new("S").strikethrough(),
            tri_strike,
            "Strikethrough",
        )
        .clicked()
        {
            let next = tri_strike != Some(true);
            if let TextStyleTarget::Caret(pos) = target {
                set_pending(props, pos, &|p| p.strikethrough = Some(next));
            } else {
                action_to_run = Some((
                    "text.style_range",
                    serde_json::json!({
                        "slide": slide_id,
                        "object": obj_id,
                        "start": char_range.0,
                        "end": char_range.1,
                        "strikethrough": next,
                    }),
                ));
            }
        }
        for (label, tip, tri_val, target_base) in [
            (
                "x\u{b2}",
                "Superscript",
                tri_super,
                coursecraft_course::Baseline::Superscript,
            ),
            (
                "x\u{2082}",
                "Subscript",
                tri_sub,
                coursecraft_course::Baseline::Subscript,
            ),
        ] {
            if crate::widgets::mixed_toggle(ui, label, tri_val, tip).clicked() {
                let next = if tri_val == Some(true) {
                    coursecraft_course::Baseline::Normal
                } else {
                    target_base
                };
                if let TextStyleTarget::Caret(pos) = target {
                    set_pending(props, pos, &|p| p.baseline = Some(next));
                } else {
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
        }
    });

    // The size sits on its own row: seven controls do not fit one row of the panel.
    inspector_row(ui, "Font size", |ui| {
        if props.text_size_seen.is_nan() || (eff_size - props.text_size_seen).abs() > 0.001 {
            props.text_size = eff_size;
            props.text_size_seen = eff_size;
        }
        let rem_w = ui.available_width();
        let r_size = add_drag_left(
            ui,
            [rem_w, coursecraft_render::CONTROL_COMPACT],
            DragValue::new(&mut props.text_size)
                .speed(1.0)
                .range(1.0..=200.0)
                .suffix(" pt"),
        );
        let enter_size = r_size.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if (r_size.drag_stopped() || r_size.lost_focus() || enter_size)
            && props.text_size.is_finite()
            && props.text_size > 0.0
            && (props.text_size - eff_size).abs() > 0.01
        {
            action_to_run = Some((
                "text.style_range",
                serde_json::json!({
                    "slide": slide_id,
                    "object": obj_id,
                    "start": char_range.0,
                    "end": char_range.1,
                    "size": props.text_size,
                }),
            ));
        }
    });

    inspector_row(ui, "Spacing", |ui| {
        if props.text_letter_spacing_seen.is_nan()
            || (eff_spacing - props.text_letter_spacing_seen).abs() > 0.0001
        {
            props.text_letter_spacing = eff_spacing;
            props.text_letter_spacing_seen = eff_spacing;
        }
        let r = ui.add(
            DragValue::new(&mut props.text_letter_spacing)
                .speed(0.1)
                .range(
                    coursecraft_course::MIN_LETTER_SPACING..=coursecraft_course::MAX_LETTER_SPACING,
                )
                .suffix(" pt"),
        );
        let enter = r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if (r.drag_stopped() || r.lost_focus() || enter)
            && props.text_letter_spacing.is_finite()
            && (props.text_letter_spacing - eff_spacing).abs() > 0.001
        {
            action_to_run = Some((
                "text.style_range",
                serde_json::json!({
                    "slide": slide_id,
                    "object": obj_id,
                    "start": char_range.0,
                    "end": char_range.1,
                    "letter_spacing": props.text_letter_spacing,
                }),
            ));
        }
    });

    // Shadow and highlight, from the runs under the selection or caret.
    let shadow_range = match target {
        TextStyleTarget::Selection(s, e) => (s, e),
        TextStyleTarget::Caret(pos) => (pos, pos),
        TextStyleTarget::WholeObject => (0, total_chars),
    };
    let caret = match target {
        TextStyleTarget::Caret(pos) => Some(pos),
        _ => None,
    };
    let (tri_shadow, run_highlight) =
        shadow_and_highlight(runs, shadow_range, caret, props, slide_id, obj_id, cell);
    // One row holds both: the highlight swatch with No Color, and the shadow toggle.
    // The Text panel shows it; the Format inspector keeps its shorter list.
    if !include_box_controls
        && let Some(action) = show_shadow_highlight_row(
            ui,
            props,
            (tri_shadow, run_highlight),
            caret,
            char_range,
            (slide_id, obj_id),
            &set_pending,
        )
    {
        action_to_run = Some(action);
    }

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
        ui.spacing_mut().button_padding.x = 2.0;
        ui.spacing_mut().item_spacing.x = 2.0;
        let cur_align = style.align;
        for (align, name) in [
            (Align::Left, "left"),
            (Align::Center, "center"),
            (Align::Right, "right"),
            (Align::Justify, "justify"),
        ] {
            let label = match align {
                Align::Left => "Left",
                Align::Center => "Center",
                Align::Right => "Right",
                Align::Justify => "Justify",
            };
            if ui.selectable_label(cur_align == align, label).clicked() && cur_align != align {
                action_to_run = Some((
                    "text.set_paragraph",
                    serde_json::json!({ "slide": slide_id, "object": obj_id, "align": name }),
                ));
            }
        }
    });

    if include_box_controls
        && let Some(action) = show_text_box_rows(ui, slide_id, obj_id, style, point_text)
    {
        action_to_run = Some(action);
    }

    inspector_row(ui, "Text content", |ui| {
        let rem_w = ui.available_width().max(30.0);
        let output = TextEdit::multiline(&mut props.text_body)
            .id(text_edit_id)
            .desired_rows(3)
            .min_size(vec2(rem_w, 60.0))
            .show(ui);
        if !props.spell_words.is_empty() {
            let words = crate::spell_check::underline_rects(
                &output.galley,
                output.galley_pos,
                &props.text_body,
                &props.spell_words,
            );
            crate::spell_check::spell_underlines(
                ui.painter(),
                &output.galley,
                output.galley_pos,
                &props.text_body,
                &props.spell_words,
                crate::spell_check::squiggle_color(ui.ctx()),
            );
            let secondary = ui.input(|i| {
                i.pointer.secondary_pressed()
                    || i.pointer.secondary_clicked()
                    || i.pointer.secondary_released()
            });
            if secondary {
                props.spell_menu_pos =
                    ui.input(|i| i.pointer.press_origin().or_else(|| i.pointer.latest_pos()));
            } else if !output.response.context_menu_opened() {
                props.spell_menu_pos = None;
            }
            if let Some(click_pos) = props.spell_menu_pos {
                let mut choice = crate::spell_check::WordMenuOutcome::default();
                let _ = output.response.context_menu(|menu_ui| {
                    choice = crate::spell_check::show_word_menu(
                        menu_ui,
                        click_pos,
                        &words,
                        &props.spell_suggestions,
                    );
                });
                let (changed, added, ignored) =
                    crate::spell_check::apply_word_menu(choice, &mut props.text_body);
                if changed {
                    action_to_run = Some((
                        "text.set",
                        serde_json::json!({
                            "slide": slide_id,
                            "object": obj_id,
                            "text": props.text_body,
                        }),
                    ));
                }
                props.spell_add.extend(added);
                props.spell_ignore.extend(ignored);
            }
        }
        let r = &output.response;
        if (r.lost_focus() || r.changed())
            && props.text_body != current_text
            && action_to_run.is_none()
        {
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
    full: bool,
) -> Option<(&'static str, Json)> {
    let mut action_to_run = None;
    let text_edit_id = egui::Id::unique(("prop_text_body", id_prefix));
    let text_edit_state = egui::TextEdit::load_state(ui.ctx(), text_edit_id);
    let cursor_range = text_edit_state.as_ref().and_then(|s| s.cursor.char_range());
    let (char_range, cursor_pos): ((usize, usize), usize) = if let Some(cr) = cursor_range
        && !cr.is_empty()
    {
        let sorted = cr.as_sorted_char_range();
        ((sorted.start.0, sorted.end.0), sorted.start.0)
    } else if let Some((_, _, inline_range, inline_cursor)) = props
        .inline_selection
        .as_ref()
        .filter(|(s, o, _, _)| s == slide_id && o == obj_id)
    {
        (inline_range.unwrap_or((0, 0)), *inline_cursor)
    } else if let Some(cr) = cursor_range {
        ((0, 0), cr.primary.index.0)
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
            .on_disabled_hover_text("Already at minimum indent")
            .clicked()
        {
            let mut params = paragraph_base();
            params["level"] = serde_json::json!(level.saturating_sub(1));
            action_to_run = Some(("text.set_paragraphs", params));
        }
        if ui
            .add_enabled(level < 4, egui::Button::new("Increase"))
            .on_hover_text("Increase indent")
            .on_disabled_hover_text("Already at maximum indent")
            .clicked()
        {
            let mut params = paragraph_base();
            params["level"] = serde_json::json!(level + 1);
            action_to_run = Some(("text.set_paragraphs", params));
        }
    });

    inspector_row(ui, "Spacing", |ui| {
        let current = current_paragraph.line_spacing;
        if props.line_spacing_seen.is_nan() || (current - props.line_spacing_seen).abs() > 0.001 {
            props.line_spacing_custom = current;
            props.line_spacing_seen = current;
        }
        let id = egui::Id::unique(("line_spacing", id_prefix, obj_id));
        let mut chosen = None;
        let mut popup_was_drawn = false;
        let rem_w = ui.available_width().max(30.0);
        crate::widgets::styled_combo(ui, id, rem_w)
            .selected_text(format!("{current:.2}x"))
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show_ui(ui, |ui| {
                popup_was_drawn = true;
                for value in [1.0_f32, 1.15, 1.5, 2.0] {
                    if ui
                        .selectable_label((current - value).abs() < 0.001, format!("{value}x"))
                        .clicked()
                    {
                        chosen = Some(value);
                        props.line_spacing_editing = false;
                        ui.close();
                    }
                }
                ui.separator();
                let response = ui.add(
                    DragValue::new(&mut props.line_spacing_custom)
                        .speed(0.05)
                        .range(0.8..=3.0)
                        .prefix("custom ")
                        .suffix("×"),
                );
                if response.has_focus() || response.changed() || response.dragged() {
                    props.line_spacing_editing = true;
                }
                let enter = response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if (response.drag_stopped() || response.lost_focus() || enter)
                    && props.line_spacing_custom.is_finite()
                    && (props.line_spacing_custom - current).abs() > 0.001
                {
                    chosen = Some(props.line_spacing_custom);
                    props.line_spacing_editing = false;
                    ui.close();
                }
            });
        if chosen.is_none() && props.line_spacing_editing && !popup_was_drawn {
            props.line_spacing_editing = false;
            if props.line_spacing_custom.is_finite()
                && (props.line_spacing_custom - current).abs() > 0.001
            {
                chosen = Some(props.line_spacing_custom);
            }
        }
        if let Some(value) = chosen {
            let mut params = paragraph_base();
            params["line_spacing"] = serde_json::json!(value);
            action_to_run = Some(("text.set_paragraphs", params));
        }
    });

    if full {
        let before = current_paragraph.space_before;
        let after = current_paragraph.space_after;
        for (label, key, value) in [
            ("Space before", "space_before", before),
            ("Space after", "space_after", after),
        ] {
            let (buf, seen) = if key == "space_before" {
                (&mut props.space_before, &mut props.space_before_seen)
            } else {
                (&mut props.space_after, &mut props.space_after_seen)
            };
            if seen.is_nan() || (value - *seen).abs() > 0.001 {
                *buf = value;
                *seen = value;
            }
            inspector_row(ui, tl!(label), |ui| {
                let r = ui.add(
                    DragValue::new(buf)
                        .speed(0.5)
                        .range(0.0..=200.0)
                        .suffix(" px"),
                );
                let enter = r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if (r.drag_stopped() || r.lost_focus() || enter)
                    && buf.is_finite()
                    && (*buf - value).abs() > 0.001
                {
                    let mut params = paragraph_base();
                    params[key] = serde_json::json!(*buf);
                    action_to_run = Some(("text.set_paragraphs", params));
                }
            });
        }
    }

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
        crate::dialog::Dialog::new("Link")
            .id(egui::Id::unique(("link_dialog", id_prefix, obj_id)))
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
                    crate::widgets::styled_combo(ui, ("link_slide_pick", id_prefix, obj_id), 180.0)
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
        Some(coursecraft_course::FillStyle::Pattern { .. }) => "Pattern",
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
    state_ctx: Option<&StateEditCtx<'_>>,
    base_fill_style_some: bool,
    actions: &mut Vec<(&'static str, Json)>,
) {
    let mut changed = false;
    let mut live_changed = false;
    let mut next_style: Option<coursecraft_course::FillStyle> = props.fill_style.clone();

    let fill_style_overridden =
        state_ctx.is_some_and(|ctx| ctx.state.fill_style.is_some() || ctx.state.clear_fill_style);
    inspector_row_state(
        ui,
        "Fill type",
        state_ctx,
        fill_style_overridden,
        "Fill style",
        serde_json::json!({ "fill_style": null, "clear_fill_style": false }),
        actions,
        |ui| {
            let rem_w = ui.available_width().max(30.0);
            let current = fill_style_kind(&props.fill_style);
            crate::widgets::styled_combo(ui, "prop_fill_type", rem_w)
                .selected_text(current)
                .show_ui(ui, |ui| {
                    for label in ["Solid", "Linear gradient", "Radial gradient", "Picture"] {
                        if ui.selectable_label(current == label, label).clicked()
                            && current != label
                        {
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
                                _ => {
                                    let first = media_names
                                        .iter()
                                        .find(|name| coursecraft_course::is_image_src(name))
                                        .cloned();
                                    match first {
                                        Some(src) => Some(coursecraft_course::FillStyle::Picture {
                                            src,
                                            mode: coursecraft_course::PictureFillMode::Stretch,
                                            tile_scale: [1.0, 1.0],
                                        }),
                                        None => {
                                            // No picture in the course yet: go straight to the
                                            // file picker. The fill changes when a file arrives,
                                            // so cancelling the picker leaves it as it was.
                                            props.pick_picture_fill = true;
                                            continue;
                                        }
                                    }
                                }
                            };
                            changed = true;
                        }
                    }
                });
        },
    );

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
                if r.changed() {
                    live_changed = true;
                }
                if r.drag_stopped() || r.lost_focus() || enter {
                    changed = true;
                }
            });
            show_gradient_stops(
                ui,
                stops,
                &mut changed,
                &mut live_changed,
                &mut props.fill_add_stop_rect,
            );
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
                if r.changed() {
                    live_changed = true;
                }
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
                if r.changed() {
                    live_changed = true;
                }
                if r.drag_stopped() || r.lost_focus() || enter {
                    changed = true;
                }
            });
            show_gradient_stops(
                ui,
                stops,
                &mut changed,
                &mut live_changed,
                &mut props.fill_add_stop_rect,
            );
        }
        Some(coursecraft_course::FillStyle::Picture { src, mode, .. }) => {
            inspector_row(ui, "Image", |ui| {
                let rem_w = ui.available_width().max(30.0);
                let selected = if src.is_empty() {
                    "Choose an image".to_owned()
                } else {
                    src.clone()
                };
                crate::widgets::styled_combo(ui, "prop_picture_src", rem_w)
                    .selected_text(selected)
                    .show_ui(ui, |ui| {
                        if ui.selectable_label(false, "Choose a file…").clicked() {
                            props.pick_picture_fill = true;
                        }
                        let pictures: Vec<&String> = media_names
                            .iter()
                            .filter(|name| coursecraft_course::is_image_src(name))
                            .collect();
                        if !pictures.is_empty() {
                            ui.separator();
                        }
                        for name in pictures {
                            if ui.selectable_label(src == name, name).clicked() && src != name {
                                *src = name.clone();
                                changed = true;
                            }
                        }
                    });
            });
            inspector_row(ui, "Picture fit", |ui| {
                let rem_w = ui.available_width().max(30.0);
                crate::widgets::styled_combo(ui, "prop_picture_mode", rem_w)
                    .selected_text(match mode {
                        coursecraft_course::PictureFillMode::Stretch => "Stretch",
                        coursecraft_course::PictureFillMode::Fill => "Fill",
                        coursecraft_course::PictureFillMode::Tile => "Tile",
                    })
                    .show_ui(ui, |ui| {
                        for (value, label) in [
                            (coursecraft_course::PictureFillMode::Stretch, "Stretch"),
                            (coursecraft_course::PictureFillMode::Fill, "Fill"),
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
        Some(coursecraft_course::FillStyle::Pattern { preset, .. }) => {
            inspector_row(ui, "Pattern", |ui| {
                ui.label(preset.as_str());
            });
        }
        None => {}
    }

    if changed {
        if let Some(
            coursecraft_course::FillStyle::Linear { stops, .. }
            | coursecraft_course::FillStyle::Radial { stops, .. },
        ) = &mut next_style
        {
            stops.sort_by(|a, b| a.position.total_cmp(&b.position));
        }
        props.fill_style = next_style;
        if let Some(ctx) = state_ctx {
            let patch = match &props.fill_style {
                Some(_) => serde_json::json!({
                    "fill_style": fill_style_json(&props.fill_style),
                }),
                None if base_fill_style_some => serde_json::json!({
                    "clear_fill_style": true,
                }),
                None => serde_json::json!({
                    "fill_style": null,
                    "clear_fill_style": false,
                }),
            };
            actions.push(patch_state_action(ctx, patch));
        } else {
            actions.push((
                "object.set_fill_style",
                serde_json::json!({
                    "slide": slide_id,
                    "object": object_id,
                    "fill_style": fill_style_json(&props.fill_style),
                }),
            ));
        }
    } else if live_changed {
        props.fill_style = next_style;
    }
}

/// Choose a position in `0.0..=1.0` for a newly added gradient stop by splitting
/// the largest open gap between existing stops (or the `[0.0, 1.0]` endpoints).
fn next_gradient_stop_position(stops: &[coursecraft_course::GradientStop]) -> f32 {
    if stops.is_empty() {
        return 0.5;
    }
    let mut positions: Vec<f32> = stops.iter().map(|s| s.position.clamp(0.0, 1.0)).collect();
    positions.sort_by(f32::total_cmp);

    let mut best_pos = 0.5;
    let mut best_gap = -1.0_f32;
    let mut consider = |a: f32, b: f32| {
        let gap = b - a;
        if gap > best_gap {
            best_gap = gap;
            best_pos = ((a + b) * 0.5).clamp(0.0, 1.0);
        }
    };
    if let Some(&first) = positions.first() {
        consider(0.0, first);
    }
    for pair in positions.windows(2) {
        if let (Some(&a), Some(&b)) = (pair.first(), pair.get(1)) {
            consider(a, b);
        }
    }
    if let Some(&last) = positions.last() {
        consider(last, 1.0);
    }
    best_pos
}

fn gradient_stop_row<R>(ui: &mut Ui, label: &str, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
    use egui::text::{LayoutJob, TextWrapping};
    let p = coursecraft_render::palette(ui.ctx());
    let row_h = coursecraft_render::CONTROL_COMPACT;
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), row_h),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let (label_rect, _) = ui.allocate_exact_size(vec2(56.0, row_h), Sense::hover());
            if ui.is_rect_visible(label_rect) {
                let mut job = LayoutJob::simple_singleline(
                    label.to_owned(),
                    FontId::proportional(coursecraft_render::SIZE_BODY),
                    p.text_secondary,
                );
                job.wrap = TextWrapping::truncate_at_width(label_rect.width());
                let galley = ui.painter().layout_job(job);
                let pos = egui::pos2(
                    label_rect.min.x,
                    label_rect.center().y - galley.size().y * 0.5,
                );
                ui.painter().galley(pos, galley, p.text_secondary);
            }
            add_contents(ui)
        },
    )
    .inner
}

fn show_gradient_stops(
    ui: &mut Ui,
    stops: &mut Vec<coursecraft_course::GradientStop>,
    changed: &mut bool,
    live_changed: &mut bool,
    add_stop_rect: &mut Option<Rect>,
) {
    let mut remove: Option<usize> = None;
    let mut move_up: Option<usize> = None;
    let mut move_down: Option<usize> = None;
    let mut add = false;
    let len = stops.len();
    let can_remove = len > 2;
    for i in 0..len {
        let Some(stop) = stops.get_mut(i) else {
            continue;
        };
        ui.push_id(("fill_stop", i), |ui| {
            gradient_stop_row(ui, &format!("Colour {}", i + 1), |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                ui.spacing_mut().button_padding = vec2(3.0, 1.0);
                // Leave room for the position field, move up/down buttons and
                // optional remove button, so the whole row stays inside the panel.
                let reserve = 44.0 + 36.0 + if can_remove { 18.0 } else { 0.0 };
                let field_w = (ui.available_width() - reserve).max(40.0);
                let (color_response, valid) = ui
                    .allocate_ui_with_layout(
                        vec2(field_w, coursecraft_render::CONTROL_COMPACT),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| show_color_field(ui, "", &mut stop.color.0, "prop_stop_color"),
                    )
                    .inner;
                if is_committed(&color_response, ui) && valid {
                    *changed = true;
                }
                let mut percent = stop.position * 100.0;
                let response = ui
                    .add_sized(
                        [42.0, coursecraft_render::CONTROL_COMPACT],
                        DragValue::new(&mut percent)
                            .speed(1.0)
                            .range(0.0..=100.0)
                            .suffix("%"),
                    )
                    .on_hover_text("Where this colour sits along the gradient");
                let enter = response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if response.changed() {
                    stop.position = (percent / 100.0).clamp(0.0, 1.0);
                    *live_changed = true;
                }
                if response.drag_stopped() || response.lost_focus() || enter {
                    stop.position = (percent / 100.0).clamp(0.0, 1.0);
                    *changed = true;
                }
                if ui
                    .add_enabled(i > 0, egui::Button::new("↑").small())
                    .on_hover_text("Move colour up")
                    .on_disabled_hover_text("Already first colour")
                    .clicked()
                {
                    move_up = Some(i);
                }
                if ui
                    .add_enabled(i + 1 < len, egui::Button::new("↓").small())
                    .on_hover_text("Move colour down")
                    .on_disabled_hover_text("Already last colour")
                    .clicked()
                {
                    move_down = Some(i);
                }
                if can_remove
                    && ui
                        .small_button("×")
                        .on_hover_text("Remove this colour")
                        .clicked()
                {
                    remove = Some(i);
                }
            });
        });
    }
    if let Some(i) = move_up
        && i > 0
        && i < stops.len()
    {
        let (left, right) = stops.split_at_mut(i);
        std::mem::swap(&mut left[i - 1].color, &mut right[0].color);
        *changed = true;
    }
    if let Some(i) = move_down
        && i + 1 < stops.len()
    {
        let (left, right) = stops.split_at_mut(i + 1);
        std::mem::swap(&mut left[i].color, &mut right[0].color);
        *changed = true;
    }
    if let Some(index) = remove
        && stops.len() > 2
    {
        stops.remove(index);
        *changed = true;
    }
    if stops.len() < coursecraft_course::MAX_GRADIENT_STOPS {
        let response = ui.button("Add colour");
        *add_stop_rect = Some(response.rect);
        if response.clicked() {
            add = true;
        }
    }
    if add && stops.len() < coursecraft_course::MAX_GRADIENT_STOPS {
        let position = next_gradient_stop_position(stops);
        let color = stops
            .last()
            .map_or_else(|| Color("#D0D7DE".to_owned()), |last| last.color.clone());
        stops.push(coursecraft_course::GradientStop { color, position });
        *changed = true;
    }
}

fn alpha_to_percent(alpha: u8) -> f32 {
    (f32::from(alpha) / 255.0 * 100.0).round()
}

fn set_color_alpha_percent(color: &mut Color, percent: f32) -> u8 {
    let c = parse_color(color).unwrap_or(Color32::BLACK);
    let a = (percent.clamp(0.0, 100.0) / 100.0 * 255.0).round() as u8;
    color.0 = color_to_hex(Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a));
    a
}

fn apply_color_alpha(color: &mut Color, alpha: u8) {
    if let Some(c) = parse_color(color) {
        color.0 = color_to_hex(Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), alpha));
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EffectsHiddenReason {
    Visible,
    NoFillOrOutline,
    NoFill,
}

fn effective_fill_style<'a>(
    base: Option<&'a coursecraft_course::FillStyle>,
    state: Option<&'a coursecraft_course::StateDef>,
) -> Option<&'a coursecraft_course::FillStyle> {
    match state {
        Some(s) if s.fill_style.is_some() => s.fill_style.as_ref(),
        // ST-05: solid `fill` without `fill_style` suppresses the base `fill_style`.
        Some(s) if s.clear_fill_style || s.fill.is_some() => None,
        _ => base,
    }
}

fn fill_style_visible(style: &coursecraft_course::FillStyle) -> bool {
    match style {
        coursecraft_course::FillStyle::Linear { stops, .. }
        | coursecraft_course::FillStyle::Radial { stops, .. } => {
            stops
                .iter()
                .take(coursecraft_course::MAX_GRADIENT_STOPS)
                .filter(|stop| parse_color(&stop.color).is_some())
                .count()
                >= 2
        }
        coursecraft_course::FillStyle::Picture { src, .. } => !src.trim().is_empty(),
        coursecraft_course::FillStyle::Pattern { .. } => true,
    }
}

fn outline_visible(outline: &coursecraft_course::Outline) -> bool {
    outline.width.is_finite()
        && outline.width > 0.0
        && outline.width <= 50.0
        && parse_color(&outline.color).is_some_and(|c| c.a() > 0)
}

fn effects_hidden_reason(
    obj: &coursecraft_course::Object,
    state_ctx: Option<&StateEditCtx<'_>>,
) -> EffectsHiddenReason {
    let state = state_ctx.map(|ctx| ctx.state);
    match &obj.kind {
        ObjectKind::Shape {
            shape,
            fill,
            fill_style,
            outline,
            ..
        } => {
            let eff_fill = state.and_then(|s| s.fill.as_ref()).unwrap_or(fill);
            let eff_fill_style = effective_fill_style(fill_style.as_ref(), state);
            let eff_outline = if !coursecraft_course::shape_allows_text(*shape) {
                None
            } else {
                match state {
                    Some(s) if s.outline.is_some() => s.outline.as_ref(),
                    Some(s) if s.clear_outline => None,
                    _ => outline.as_ref(),
                }
            };
            let has_fill = eff_fill_style.is_some_and(fill_style_visible)
                || parse_color(eff_fill).is_some_and(|c| c.a() > 0);
            let has_outline = eff_outline.is_some_and(outline_visible);
            if !has_fill && !has_outline {
                EffectsHiddenReason::NoFillOrOutline
            } else {
                EffectsHiddenReason::Visible
            }
        }
        ObjectKind::Button {
            fill, fill_style, ..
        } => {
            let eff_fill = state.and_then(|s| s.fill.as_ref()).unwrap_or(fill);
            let eff_fill_style = effective_fill_style(fill_style.as_ref(), state);
            let has_fill = eff_fill_style.is_some_and(fill_style_visible)
                || parse_color(eff_fill).is_some_and(|c| c.a() > 0);
            if !has_fill {
                EffectsHiddenReason::NoFill
            } else {
                EffectsHiddenReason::Visible
            }
        }
        ObjectKind::Path { path } => {
            let eff_fill = state.and_then(|s| s.fill.as_ref()).or(path.fill.as_ref());
            let eff_fill_style = effective_fill_style(path.fill_style.as_ref(), state);
            let eff_outline = match state {
                Some(s) if s.outline.is_some() => s.outline.as_ref(),
                Some(s) if s.clear_outline => None,
                _ => Some(&path.outline),
            };
            let has_fill = path.closed
                && (eff_fill_style.is_some_and(fill_style_visible)
                    || eff_fill.is_some_and(|f| parse_color(f).is_some_and(|c| c.a() > 0)));
            let has_outline = eff_outline.is_some_and(outline_visible);
            if !has_fill && !has_outline {
                EffectsHiddenReason::NoFillOrOutline
            } else {
                EffectsHiddenReason::Visible
            }
        }
        _ => EffectsHiddenReason::Visible,
    }
}

/// Shadow and glow toggles with their values.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn show_effect_controls(
    ui: &mut Ui,
    props: &mut PropertiesState,
    slide_id: &str,
    object_id: &str,
    state_ctx: Option<&StateEditCtx<'_>>,
    base_shadow_some: bool,
    base_glow_some: bool,
    hidden_reason: EffectsHiddenReason,
    actions: &mut Vec<(&'static str, Json)>,
) {
    let mut shadow_changed = false;
    let mut glow_changed = false;
    inspector_separator(ui);
    inspector_section_header(ui, "Effects");

    let shadow_overridden =
        state_ctx.is_some_and(|ctx| ctx.state.shadow.is_some() || ctx.state.clear_shadow);
    let mut draw_shadow = |ui: &mut Ui| {
        inspector_row_state(
            ui,
            "Shadow",
            state_ctx,
            shadow_overridden,
            "Shadow",
            serde_json::json!({ "shadow": null, "clear_shadow": false }),
            actions,
            |ui| {
                if ui.checkbox(&mut props.shadow_enabled, "").changed() {
                    shadow_changed = true;
                }
            },
        );
        if props.shadow_enabled {
            inspector_row(ui, "Colour", |ui| {
                let (response, valid) =
                    show_color_field(ui, "", &mut props.shadow.color.0, "prop_shadow_color");
                if is_committed(&response, ui) && valid {
                    apply_color_alpha(&mut props.shadow.color, props.shadow_alpha);
                    shadow_changed = true;
                }
            });
            inspector_row(ui, "Opacity", |ui| {
                let mut opacity_pct = alpha_to_percent(props.shadow_alpha);
                let rem_w = ui.available_width().max(30.0);
                let response = add_drag_left(
                    ui,
                    [rem_w, coursecraft_render::CONTROL_COMPACT],
                    DragValue::new(&mut opacity_pct)
                        .speed(1.0)
                        .range(0.0..=100.0)
                        .fixed_decimals(0)
                        .suffix(" %"),
                );
                if response.changed() {
                    props.shadow_alpha =
                        set_color_alpha_percent(&mut props.shadow.color, opacity_pct);
                }
                let enter = response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if response.drag_stopped() || response.lost_focus() || enter {
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
                        DragValue::new(value).speed(0.5).range(range).suffix(" px"),
                    );
                    let enter =
                        response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if response.drag_stopped() || response.lost_focus() || enter {
                        shadow_changed = true;
                    }
                });
            }
        }
    };
    let shadow_note = match hidden_reason {
        EffectsHiddenReason::NoFillOrOutline => {
            Some(tl!("Add a fill or outline to see the shadow."))
        }
        EffectsHiddenReason::NoFill => Some(tl!("Add a fill to see the shadow.")),
        EffectsHiddenReason::Visible => None,
    };
    if let Some(note) = shadow_note {
        ui.scope(|ui| {
            ui.multiply_opacity(0.6);
            draw_shadow(ui);
        });
        let p = coursecraft_render::palette(ui.ctx());
        ui.add(
            egui::Label::new(
                RichText::new(note)
                    .size(coursecraft_render::SIZE_CAPTION)
                    .color(p.text_secondary),
            )
            .wrap(),
        );
    } else {
        draw_shadow(ui);
    }

    let glow_overridden =
        state_ctx.is_some_and(|ctx| ctx.state.glow.is_some() || ctx.state.clear_glow);
    let mut draw_glow = |ui: &mut Ui| {
        inspector_row_state(
            ui,
            "Glow",
            state_ctx,
            glow_overridden,
            "Glow",
            serde_json::json!({ "glow": null, "clear_glow": false }),
            actions,
            |ui| {
                if ui.checkbox(&mut props.glow_enabled, "").changed() {
                    glow_changed = true;
                }
            },
        );
        if props.glow_enabled {
            inspector_row(ui, "Colour", |ui| {
                let (response, valid) =
                    show_color_field(ui, "", &mut props.glow.color.0, "prop_glow_color");
                if is_committed(&response, ui) && valid {
                    apply_color_alpha(&mut props.glow.color, props.glow_alpha);
                    glow_changed = true;
                }
            });
            inspector_row(ui, "Opacity", |ui| {
                let mut opacity_pct = alpha_to_percent(props.glow_alpha);
                let rem_w = ui.available_width().max(30.0);
                let response = add_drag_left(
                    ui,
                    [rem_w, coursecraft_render::CONTROL_COMPACT],
                    DragValue::new(&mut opacity_pct)
                        .speed(1.0)
                        .range(0.0..=100.0)
                        .fixed_decimals(0)
                        .suffix(" %"),
                );
                if response.changed() {
                    props.glow_alpha = set_color_alpha_percent(&mut props.glow.color, opacity_pct);
                }
                let enter = response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if response.drag_stopped() || response.lost_focus() || enter {
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
                        .range(0.0..=100.0)
                        .suffix(" px"),
                );
                let enter = response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if response.drag_stopped() || response.lost_focus() || enter {
                    glow_changed = true;
                }
            });
        }
    };
    let glow_note = match hidden_reason {
        EffectsHiddenReason::NoFillOrOutline => Some(tl!("Add a fill or outline to see the glow.")),
        EffectsHiddenReason::NoFill => Some(tl!("Add a fill to see the glow.")),
        EffectsHiddenReason::Visible => None,
    };
    if let Some(note) = glow_note {
        ui.scope(|ui| {
            ui.multiply_opacity(0.6);
            draw_glow(ui);
        });
        let p = coursecraft_render::palette(ui.ctx());
        ui.add(
            egui::Label::new(
                RichText::new(note)
                    .size(coursecraft_render::SIZE_CAPTION)
                    .color(p.text_secondary),
            )
            .wrap(),
        );
    } else {
        draw_glow(ui);
    }

    if shadow_changed {
        if let Some(ctx) = state_ctx {
            let patch = if props.shadow_enabled {
                serde_json::json!({
                    "shadow": serde_json::to_value(&props.shadow).unwrap_or(Json::Null),
                })
            } else if base_shadow_some {
                serde_json::json!({ "clear_shadow": true })
            } else {
                serde_json::json!({ "shadow": null, "clear_shadow": false })
            };
            actions.push(patch_state_action(ctx, patch));
        } else {
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
    }
    if glow_changed {
        if let Some(ctx) = state_ctx {
            let patch = if props.glow_enabled {
                serde_json::json!({
                    "glow": serde_json::to_value(&props.glow).unwrap_or(Json::Null),
                })
            } else if base_glow_some {
                serde_json::json!({ "clear_glow": true })
            } else {
                serde_json::json!({ "glow": null, "clear_glow": false })
            };
            actions.push(patch_state_action(ctx, patch));
        } else {
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
    system_fonts: Option<&[crate::SystemFontChoice]>,
    font_requests: &mut Vec<crate::EditorRequest>,
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
            .on_disabled_hover_text("A table must have at least one row")
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
            .on_disabled_hover_text("A table must have at least one column")
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
            false,
            true,
            system_fonts,
            font_requests,
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
            false,
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
            let enter = response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if response.drag_stopped() || response.lost_focus() || enter {
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
            let enter = response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if response.drag_stopped() || response.lost_focus() || enter {
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
        let combo = crate::widgets::styled_combo(ui, "prop_input_variable", rem_w)
            .selected_text(label)
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
                if crate::grid_panel::drag_committed(ui, &r) {
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
                if crate::grid_panel::drag_committed(ui, &r) {
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
                if crate::grid_panel::drag_committed(ui, &r) {
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

#[allow(clippy::too_many_arguments)]
fn show_timer_controls(
    ui: &mut Ui,
    props: &mut PropertiesState,
    course: &Course,
    slide_id: &str,
    object_id: &str,
    font_families: &[String],
    system_fonts: Option<&[crate::SystemFontChoice]>,
    font_requests: &mut Vec<crate::EditorRequest>,
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
        let enter = r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if r.drag_stopped() || r.lost_focus() || enter {
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
        let shown = crate::font_picker::displayed_font(&props.timer_font);
        if let Some(picked) = crate::font_picker::show_font_picker(
            ui,
            "timer",
            &shown,
            font_families,
            &mut props.recent_fonts,
            system_fonts,
            &mut props.system_fonts_list_requested,
            &mut props.system_font_search,
            font_requests,
        ) {
            match picked {
                crate::font_picker::FontPickerAction::PickCourseOrRecent(name) => {
                    if shown != name {
                        props.timer_font = name;
                        action_to_run = Some(commit(props));
                    }
                }
                crate::font_picker::FontPickerAction::PickSystem(name) => {
                    props.timer_font = name.clone();
                    action_to_run = Some(commit(props));
                    font_requests.push(crate::EditorRequest::AddSystemFont {
                        family: name,
                        apply: None,
                    });
                }
                crate::font_picker::FontPickerAction::AddFont => {
                    props.add_font_requested = true;
                }
            }
        }
    });
    inspector_row(ui, "Style", |ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        let bold_tip = crate::menu::tooltip_with_shortcut(
            ui.ctx(),
            "Bold",
            Some(crate::shortcuts::shortcut_for_action(
                crate::shortcuts::ShortcutAction::Bold,
            )),
        );
        if crate::widgets::mixed_toggle(
            ui,
            RichText::new("B").strong(),
            Some(props.text_bold),
            &bold_tip,
        )
        .clicked()
        {
            props.text_bold = !props.text_bold;
            action_to_run = Some(commit(props));
        }
        let italic_tip = crate::menu::tooltip_with_shortcut(
            ui.ctx(),
            "Italic",
            Some(crate::shortcuts::shortcut_for_action(
                crate::shortcuts::ShortcutAction::Italic,
            )),
        );
        if crate::widgets::mixed_toggle(
            ui,
            RichText::new("I").italics(),
            Some(props.text_italic),
            &italic_tip,
        )
        .clicked()
        {
            props.text_italic = !props.text_italic;
            action_to_run = Some(commit(props));
        }
        let underline_tip = crate::menu::tooltip_with_shortcut(
            ui.ctx(),
            "Underline",
            Some(crate::shortcuts::shortcut_for_action(
                crate::shortcuts::ShortcutAction::Underline,
            )),
        );
        if crate::widgets::mixed_toggle(
            ui,
            RichText::new("U").underline(),
            Some(props.text_underline),
            &underline_tip,
        )
        .clicked()
        {
            props.text_underline = !props.text_underline;
            action_to_run = Some(commit(props));
        }
        ui.label("Size");
        let r = ui.add(
            DragValue::new(&mut props.text_size)
                .speed(1.0)
                .range(1.0..=400.0)
                .suffix(" pt"),
        );
        let enter = r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if r.drag_stopped() || r.lost_focus() || enter {
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
        let combo = crate::widgets::styled_combo(ui, "prop_meter_variable", rem_w)
            .selected_text(label)
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
        let enter = r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if r.drag_stopped() || r.lost_focus() || enter {
            action_to_run = Some(commit(props));
        }
    });
    inspector_row(ui, "Max", |ui| {
        let r = ui.add(DragValue::new(&mut props.input_max).speed(1.0));
        let enter = r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if r.drag_stopped() || r.lost_focus() || enter {
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
        let lo = coursecraft_course::safe_clamp(
            frame_height,
            0.0,
            coursecraft_course::MAX_SCROLL_CONTENT_HEIGHT,
        );
        let r = ui.add(
            DragValue::new(&mut props.scroll_content_height)
                .speed(1.0)
                .range(lo..=coursecraft_course::MAX_SCROLL_CONTENT_HEIGHT)
                .suffix(" px"),
        );
        props.scroll_content_height_rect = Some(r.rect);
        if crate::grid_panel::drag_committed(ui, &r) {
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
        let height = coursecraft_course::safe_clamp(
            fit_height,
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
        let combo = crate::widgets::styled_combo(ui, "prop_panel_target", rem_w)
            .selected_text(label)
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
        if crate::grid_panel::drag_committed(ui, &r) {
            commit_input_patch(props, kind, slide_id, object_id, action_to_run);
        }
    });
    inspector_row(ui, "Max", |ui| {
        let r = ui.add(DragValue::new(&mut props.input_max).speed(0.1));
        if crate::grid_panel::drag_committed(ui, &r) {
            commit_input_patch(props, kind, slide_id, object_id, action_to_run);
        }
    });
    inspector_row(ui, "Step", |ui| {
        let r = ui.add(
            DragValue::new(&mut props.input_step)
                .speed(0.1)
                .range(0.0..=1_000_000.0),
        );
        if crate::grid_panel::drag_committed(ui, &r) {
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
        // Laid out from the right: the buttons and the value field take their
        // size first and the label field fills what is left, so the row (and
        // with it the whole panel) never grows past the panel's edge.
        let row_w = ui.available_width();
        ui.allocate_ui_with_layout(
            vec2(row_w, coursecraft_render::CONTROL_COMPACT),
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| {
                if ui
                    .add_enabled(
                        count > coursecraft_course::MIN_RADIO_OPTIONS,
                        egui::Button::new("✕").small(),
                    )
                    .on_hover_text("Remove option")
                    .on_disabled_hover_text("At least two options are required")
                    .clicked()
                {
                    remove = Some(index);
                }
                if ui
                    .add_enabled(index + 1 < count, egui::Button::new("↓").small())
                    .on_hover_text("Move down")
                    .on_disabled_hover_text("Already last option")
                    .clicked()
                {
                    move_down = Some(index);
                }
                if ui
                    .add_enabled(index > 0, egui::Button::new("↑").small())
                    .on_hover_text("Move up")
                    .on_disabled_hover_text("Already first option")
                    .clicked()
                {
                    move_up = Some(index);
                }
                let value_w = (ui.available_width() * 0.4).clamp(64.0, 100.0);
                let value = ui.add_sized(
                    [value_w, coursecraft_render::CONTROL_COMPACT],
                    TextEdit::singleline(&mut option.value)
                        .id_salt(("prop_input_option_value", index))
                        .hint_text("Value"),
                );
                let label_w = ui.available_width().max(40.0);
                let label = ui.add_sized(
                    [label_w, coursecraft_render::CONTROL_COMPACT],
                    TextEdit::singleline(&mut option.label)
                        .id_salt(("prop_input_option_label", index))
                        .hint_text("Label"),
                );
                props.input_option_rects.push(label.rect);
                if label.lost_focus() || value.lost_focus() {
                    edited = true;
                }
            },
        );
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
        .on_disabled_hover_text("Maximum number of options reached")
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
    let mut font_requests: Vec<crate::EditorRequest> = Vec::new();
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
    editor.ui.properties_lists.ensure(
        editor.session.revision,
        &editor.session.course,
        &editor.media,
    );
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
    let video_preview_error: Option<&str> = match &obj.kind {
        ObjectKind::Video { .. } => editor.timeline_playback.video_error_for(&obj.id),
        _ => None,
    };

    let props = &mut editor.ui.properties;
    let lists = &editor.ui.properties_lists;
    let system_fonts = editor.ui.system_fonts.as_deref();

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

    let state_edit_locked = editor.ui.editing_state.is_some();
    let state_ctx_owned = editor
        .ui
        .editing_state
        .as_ref()
        .filter(|s| s.slide_id == slide.id && s.object_id == obj.id)
        .and_then(|s| {
            obj.states
                .iter()
                .find(|st| st.name == s.state_name)
                .map(|st| (s.state_name.clone(), st.clone()))
        });
    let state_ctx = state_ctx_owned.as_ref().map(|(name, st)| StateEditCtx {
        slide_id: &slide.id,
        object_id: &obj.id,
        state_name: name,
        state: st,
    });
    let state_ctx_ref = state_ctx.as_ref();

    // 2. Name
    ui.add_enabled_ui(!state_edit_locked, |ui| {
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
    })
    .response
    .on_disabled_hover_text(crate::STATE_LOCK_TOOLTIP);

    let opacity_overridden = state_ctx_ref.is_some_and(|ctx| ctx.state.opacity.is_some());
    let mut opacity_action = None;
    inspector_row_state(
        ui,
        "Opacity",
        state_ctx_ref,
        opacity_overridden,
        "Opacity",
        serde_json::json!({ "opacity": null }),
        actions_to_run,
        |ui| {
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
                let base_or_curr = state_ctx_ref
                    .and_then(|ctx| ctx.state.opacity)
                    .unwrap_or(obj.opacity);
                if (opacity - base_or_curr).abs() > 0.001 || state_ctx_ref.is_some() {
                    if let Some(ctx) = state_ctx_ref {
                        opacity_action = Some(patch_state_action(
                            ctx,
                            serde_json::json!({ "opacity": opacity }),
                        ));
                    } else if (opacity - obj.opacity).abs() > 0.001 {
                        opacity_action = Some((
                            "object.set_opacity",
                            serde_json::json!({
                                "slide": slide.id,
                                "object": obj.id,
                                "opacity": opacity,
                            }),
                        ));
                    }
                }
            }
        },
    );
    if let Some(act) = opacity_action {
        actions_to_run.push(act);
    }

    inspector_separator(ui);

    // 3. Kind-specific fields
    match &obj.kind {
        ObjectKind::Text {
            text,
            style,
            runs,
            paragraphs,
            autofit,
            point_text,
            outline,
            ..
        } => {
            let mut eff_style = state_ctx_ref
                .and_then(|ctx| ctx.state.text_style.as_ref())
                .unwrap_or(style)
                .clone();
            if let Some(ctx) = state_ctx_ref
                && ctx.state.text_style.is_none()
                && let Some(c) = &ctx.state.fill
            {
                eff_style.color = c.clone();
            }
            let eff_runs = state_ctx_ref
                .and_then(|ctx| ctx.state.runs.as_deref())
                .unwrap_or(runs);
            let eff_text = state_ctx_ref
                .and_then(|ctx| ctx.state.text.as_deref())
                .unwrap_or(text.as_str());
            let eff_autofit = state_ctx_ref
                .and_then(|ctx| ctx.state.autofit)
                .unwrap_or(*autofit);
            if let Some(action) = show_text_controls(
                ui,
                props,
                &slide.id,
                &obj.id,
                &eff_style,
                eff_runs,
                eff_text,
                "text",
                &lists.font_families,
                None,
                Some(eff_autofit),
                *point_text,
                true,
                system_fonts,
                &mut font_requests,
            ) {
                action_to_run = Some(match state_ctx_ref {
                    Some(ctx) => {
                        let view = state_text_view(ctx, style, runs, paragraphs, text);
                        route_text_set_to_state(action, ctx, &view)
                    }
                    None => action,
                });
            }
            let outline_overridden = state_ctx_ref
                .is_some_and(|ctx| ctx.state.outline.is_some() || ctx.state.clear_outline);
            let eff_outline = match state_ctx_ref {
                Some(ctx) if ctx.state.clear_outline => None,
                Some(ctx) if ctx.state.outline.is_some() => ctx.state.outline.as_ref(),
                _ => outline.as_ref(),
            };
            inspector_row_state(
                ui,
                "Outline",
                state_ctx_ref,
                outline_overridden,
                "Outline",
                serde_json::json!({ "outline": null, "clear_outline": false }),
                actions_to_run,
                |ui| {
                    let mut has_border = eff_outline.is_some();
                    let mut changed = ui.checkbox(&mut has_border, "").changed();
                    let mut cur =
                        eff_outline
                            .cloned()
                            .unwrap_or_else(|| coursecraft_course::Outline {
                                color: coursecraft_course::Color("#1F1F1F".to_owned()),
                                width: 1.0,
                                dash: coursecraft_course::DashStyle::Solid,
                                dash_pattern: None,
                                join: coursecraft_course::LineJoin::Miter,
                                miter_limit: None,
                            });
                    if has_border {
                        // The outline comes from the document every frame: edit a buffer and
                        // commit once, so a drag or a typed number is one undo step.
                        let id = ui.make_persistent_id(("prop_text_border_width", obj.id.as_str()));
                        let (_, committed) =
                            crate::grid_panel::committed_drag(ui, id, cur.width, |d| {
                                d.speed(0.25).range(0.5..=50.0).suffix(" px")
                            });
                        if let Some(width) = committed {
                            cur.width = width;
                            changed = true;
                        }
                        let mut hex = cur.color.0.clone();
                        let (cr, valid) =
                            show_color_field(ui, "", &mut hex, "prop_text_border_color");
                        if is_committed(&cr, ui) && valid && hex.trim() != cur.color.0 {
                            cur.color = coursecraft_course::Color(hex.trim().to_owned());
                            changed = true;
                        }
                    }
                    if changed {
                        if let Some(ctx) = state_ctx_ref {
                            let patch = if has_border {
                                serde_json::json!({ "outline": cur })
                            } else if outline.is_some() {
                                serde_json::json!({ "clear_outline": true })
                            } else {
                                serde_json::json!({ "outline": null, "clear_outline": false })
                            };
                            action_to_run = Some(patch_state_action(ctx, patch));
                        } else {
                            let mut kind = serde_json::to_value(&obj.kind).unwrap_or(Json::Null);
                            kind["outline"] = if has_border {
                                serde_json::to_value(&cur).unwrap_or(Json::Null)
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
                    }
                },
            );
        }
        ObjectKind::Button {
            label,
            fill,
            fill_style,
        } => {
            let eff_label = state_ctx_ref
                .and_then(|ctx| ctx.state.text.as_deref())
                .unwrap_or(label.as_str());
            let eff_fill = state_ctx_ref
                .and_then(|ctx| ctx.state.fill.as_ref())
                .unwrap_or(fill);
            let fill_style_json = serde_json::to_value(fill_style).unwrap_or(Json::Null);
            let label_overridden = state_ctx_ref.is_some_and(|ctx| ctx.state.text.is_some());
            inspector_row_state(
                ui,
                "Label",
                state_ctx_ref,
                label_overridden,
                "Label",
                serde_json::json!({ "text": null }),
                actions_to_run,
                |ui| {
                    let rem_w = ui.available_width().max(30.0);
                    let r = ui.add_sized(
                        [rem_w, coursecraft_render::CONTROL_COMPACT],
                        TextEdit::singleline(&mut props.button_label).id_salt("prop_btn_label"),
                    );
                    props.btn_label_rect = Some(r.rect);
                    if is_committed(&r, ui) && props.button_label != eff_label {
                        if let Some(ctx) = state_ctx_ref {
                            action_to_run = Some(patch_state_action(
                                ctx,
                                serde_json::json!({ "text": props.button_label }),
                            ));
                        } else {
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
                    }
                },
            );

            let has_fill = eff_fill.0 != "#00000000";
            let fill_overridden = state_ctx_ref.is_some_and(|ctx| ctx.state.fill.is_some());
            inspector_row_state(
                ui,
                "Fill",
                state_ctx_ref,
                fill_overridden,
                "Fill",
                serde_json::json!({ "fill": null }),
                actions_to_run,
                |ui| {
                    let mut checked = has_fill;
                    if ui.checkbox(&mut checked, "").changed() {
                        let state_name = state_ctx_ref.map(|ctx| ctx.state_name);
                        if !checked {
                            let current_fill_style = if state_ctx_ref.is_some() {
                                props.fill_style.clone()
                            } else {
                                fill_style.clone()
                            };
                            props.remember_fill(
                                &slide.id,
                                &obj.id,
                                state_name,
                                &eff_fill.0,
                                current_fill_style,
                            );
                            props.button_fill = "#00000000".to_owned();
                            props.fill_style = None;
                            if let Some(ctx) = state_ctx_ref {
                                action_to_run = Some(patch_state_action(
                                    ctx,
                                    serde_json::json!({
                                        "fill": "#00000000",
                                        "fill_style": null,
                                        "clear_fill_style": false,
                                    }),
                                ));
                            } else {
                                action_to_run = Some((
                                    "object.update",
                                    serde_json::json!({
                                        "slide": slide.id,
                                        "object": obj.id,
                                        "kind": {
                                            "type": "button",
                                            "label": props.button_label,
                                            "fill": "#00000000",
                                            "fill_style": Json::Null,
                                        },
                                    }),
                                ));
                            }
                        } else {
                            let (next_color, next_style) = props.restore_fill(
                                &slide.id,
                                &obj.id,
                                state_name,
                                &props.button_fill,
                                "#2F6FEB",
                            );
                            props.button_fill = next_color.clone();
                            props.fill_style = next_style.clone();
                            if let Some(ctx) = state_ctx_ref {
                                action_to_run = Some(patch_state_action(
                                    ctx,
                                    serde_json::json!({
                                        "fill": next_color,
                                        "fill_style": crate::properties::fill_style_json(&next_style),
                                        "clear_fill_style": false,
                                    }),
                                ));
                            } else {
                                action_to_run = Some((
                                    "object.update",
                                    serde_json::json!({
                                        "slide": slide.id,
                                        "object": obj.id,
                                        "kind": {
                                            "type": "button",
                                            "label": props.button_label,
                                            "fill": next_color,
                                            "fill_style": crate::properties::fill_style_json(&next_style),
                                        },
                                    }),
                                ));
                            }
                        }
                    }
                    if has_fill {
                        let (cr, valid_color) =
                            show_color_field(ui, "", &mut props.button_fill, "prop_btn_fill");
                        props.color_rect = Some(cr.rect);
                        if is_committed(&cr, ui)
                            && valid_color
                            && props.button_fill.trim() != eff_fill.0
                        {
                            if let Some(ctx) = state_ctx_ref {
                                action_to_run = Some(patch_state_action(
                                    ctx,
                                    serde_json::json!({ "fill": props.button_fill.trim() }),
                                ));
                            } else {
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
                        }
                    }
                },
            );

            if has_fill {
                show_fill_style_editor(
                    ui,
                    props,
                    &eff_fill.0,
                    &slide.id,
                    &obj.id,
                    &lists.media_names,
                    state_ctx_ref,
                    fill_style.is_some(),
                    actions_to_run,
                );
            }
        }
        ObjectKind::Shape {
            shape,
            fill,
            fill_style: _,
            outline,
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
            ui.add_enabled_ui(!state_edit_locked, |ui| {
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
            })
            .response
            .on_disabled_hover_text(crate::STATE_LOCK_TOOLTIP);

            let eff_fill = state_ctx_ref
                .and_then(|ctx| ctx.state.fill.as_ref())
                .unwrap_or(fill);
            shape_fill_rows(
                ui,
                props,
                slide,
                obj,
                shape,
                fill,
                state_ctx_ref,
                &lists.media_names,
                actions_to_run,
                &mut action_to_run,
            );

            if !adjust.is_empty() {
                ui.add_enabled_ui(!state_edit_locked, |ui| {
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
                })
                .response
                .on_disabled_hover_text(crate::STATE_LOCK_TOOLTIP);
            }

            inspector_separator(ui);
            shape_outline_rows(
                ui,
                props,
                slide,
                obj,
                shape,
                fill,
                outline,
                state_ctx_ref,
                actions_to_run,
                &mut action_to_run,
            );

            if coursecraft_course::shape_allows_text(*shape) {
                inspector_separator(ui);
                let default_style;
                let (base_style, base_runs, base_text, base_autofit) = match text {
                    Some(text) => (
                        &text.style,
                        text.runs.as_slice(),
                        text.text.as_str(),
                        Some(text.autofit),
                    ),
                    None => {
                        default_style = default_shape_text_style(eff_fill);
                        (&default_style, &[][..], "", None)
                    }
                };
                let eff_style = state_ctx_ref
                    .and_then(|ctx| ctx.state.text_style.as_ref())
                    .unwrap_or(base_style);
                let eff_runs = state_ctx_ref
                    .and_then(|ctx| ctx.state.runs.as_deref())
                    .unwrap_or(base_runs);
                let eff_text = state_ctx_ref
                    .and_then(|ctx| ctx.state.text.as_deref())
                    .unwrap_or(base_text);
                let eff_autofit = state_ctx_ref
                    .and_then(|ctx| ctx.state.autofit)
                    .or(base_autofit);
                if let Some(action) = show_text_controls(
                    ui,
                    props,
                    &slide.id,
                    &obj.id,
                    eff_style,
                    eff_runs,
                    eff_text,
                    "shape",
                    &lists.font_families,
                    None,
                    eff_autofit,
                    false,
                    true,
                    system_fonts,
                    &mut font_requests,
                ) {
                    action_to_run = Some(match state_ctx_ref {
                        Some(ctx) => {
                            let base_paras = text.as_ref().map_or(&[][..], |t| &t.paragraphs[..]);
                            let view =
                                state_text_view(ctx, base_style, base_runs, base_paras, base_text);
                            route_text_set_to_state(action, ctx, &view)
                        }
                        None => action,
                    });
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

            let path_outline_overridden = state_ctx_ref
                .is_some_and(|ctx| ctx.state.outline.is_some() || ctx.state.clear_outline);
            let build_path_outline = |props: &PropertiesState| {
                outline_json(
                    &props.path_outline_color,
                    props.path_outline_width,
                    props.path_outline_dash,
                    &props.path_outline_dash_pattern,
                    props.path_outline_join,
                )
            };
            inspector_row_state(
                ui,
                "Colour",
                state_ctx_ref,
                path_outline_overridden,
                "Outline",
                serde_json::json!({ "outline": null, "clear_outline": false }),
                actions_to_run,
                |ui| {
                    let (cr, valid_color) = show_color_field(
                        ui,
                        "",
                        &mut props.path_outline_color,
                        "prop_path_outline_color",
                    );
                    props.color_rect = Some(cr.rect);
                    if is_committed(&cr, ui) && valid_color {
                        if let Some(ctx) = state_ctx_ref {
                            action_to_run = Some(patch_state_action(
                                ctx,
                                serde_json::json!({ "outline": build_path_outline(props) }),
                            ));
                        } else {
                            let mut kind = base_kind.clone();
                            kind["outline"] = build_path_outline(props);
                            action_to_run = Some((
                                "object.update",
                                serde_json::json!({
                                    "slide": slide.id,
                                    "object": obj.id,
                                    "kind": kind,
                                }),
                            ));
                        }
                    }
                },
            );

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
                    if let Some(ctx) = state_ctx_ref {
                        action_to_run = Some(patch_state_action(
                            ctx,
                            serde_json::json!({ "outline": build_path_outline(props) }),
                        ));
                    } else {
                        let mut kind = base_kind.clone();
                        kind["outline"] = build_path_outline(props);
                        action_to_run = Some((
                            "object.update",
                            serde_json::json!({
                                "slide": slide.id,
                                "object": obj.id,
                                "kind": kind,
                            }),
                        ));
                    }
                }
            });

            if show_dash_controls(
                ui,
                "prop_path_outline",
                &mut props.path_outline_dash,
                &mut props.path_outline_dash_pattern,
                &mut props.path_outline_dash_length,
                &mut props.path_outline_gap,
            ) {
                if let Some(ctx) = state_ctx_ref {
                    action_to_run = Some(patch_state_action(
                        ctx,
                        serde_json::json!({ "outline": build_path_outline(props) }),
                    ));
                } else {
                    let mut kind = base_kind.clone();
                    kind["outline"] = build_path_outline(props);
                    action_to_run = Some((
                        "object.update",
                        serde_json::json!({
                            "slide": slide.id,
                            "object": obj.id,
                            "kind": kind,
                        }),
                    ));
                }
            }

            inspector_separator(ui);
            ui.add_enabled_ui(!state_edit_locked, |ui| {
                inspector_row(ui, "Begin", |ui| {
                    show_line_end_combo(ui, "prop_path_start_end", &mut props.path_start_end);
                });
            })
            .response
            .on_disabled_hover_text(crate::STATE_LOCK_TOOLTIP);
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

            ui.add_enabled_ui(!state_edit_locked, |ui| {
                inspector_row(ui, "End", |ui| {
                    show_line_end_combo(ui, "prop_path_end_end", &mut props.path_end_end);
                });
            })
            .response
            .on_disabled_hover_text(crate::STATE_LOCK_TOOLTIP);
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
                    crate::widgets::styled_combo(ui, "prop_path_connector", rem_w)
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
                    crate::widgets::styled_combo(ui, "prop_path_style", rem_w)
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
                let path_fill_overridden =
                    state_ctx_ref.is_some_and(|ctx| ctx.state.fill.is_some());
                inspector_row_state(
                    ui,
                    "Fill",
                    state_ctx_ref,
                    path_fill_overridden,
                    "Fill",
                    serde_json::json!({ "fill": null }),
                    actions_to_run,
                    |ui| {
                        let (cr, valid_color) =
                            show_color_field(ui, "", &mut props.path_fill, "prop_path_fill");
                        props.color_rect = Some(cr.rect);
                        if is_committed(&cr, ui) && valid_color {
                            if let Some(ctx) = state_ctx_ref {
                                action_to_run = Some(patch_state_action(
                                    ctx,
                                    serde_json::json!({ "fill": props.path_fill.trim() }),
                                ));
                            } else {
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
                        }
                    },
                );
            }
        }
        ObjectKind::Image {
            src,
            alt,
            crop,
            shape,
            ..
        } => {
            let cropping = editor
                .ui
                .crop_edit
                .as_ref()
                .is_some_and(|session| session.object == obj.id);
            let can_crop = cropping || src_loaded;
            let eff_has_crop = match state_ctx_ref {
                Some(ctx) if ctx.state.clear_crop => false,
                Some(ctx) if ctx.state.crop.is_some() => true,
                _ => crop.is_some(),
            };
            show_image_properties(
                ui,
                props,
                ImagePropTarget {
                    src,
                    alt,
                    slide_id: &slide.id,
                    obj_id: &obj.id,
                    has_crop: eff_has_crop,
                    cropping,
                    can_crop,
                    base_shape: *shape,
                    state_ctx: state_ctx_ref,
                    media_names: &lists.media_names,
                },
                &mut editor.pending_image_pick,
                &mut replace_image_requested,
                &mut reset_natural_size_src,
                &mut suggest_alt_requested,
                &mut crop_action,
                actions_to_run,
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
                        let enter = at.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        time_commit = at.drag_stopped() || at.lost_focus() || enter;
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
            if let Some(err) = video_preview_error {
                ui.colored_label(p.danger, err);
            }
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
                        )
                        .suffix("×"),
                );
                if crate::grid_panel::drag_committed(ui, &r)
                    && (props.lottie_speed - speed).abs() > f32::EPSILON
                {
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
                &lists.font_families,
                system_fonts,
                &mut font_requests,
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
                &lists.font_families,
                &lists.slides,
                actions_to_run,
                system_fonts,
                &mut font_requests,
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

    ui.add_enabled_ui(!state_edit_locked, |ui| {
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
    })
    .response
    .on_disabled_hover_text(crate::STATE_LOCK_TOOLTIP);

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

    if state_edit_locked {
        // The named text style is shared across all states.
        ui.add_enabled_ui(false, |ui| {
            crate::text_styles_ui::show_style_section(ui, &editor.session.course, &slide.id, obj)
        })
        .response
        .on_disabled_hover_text(crate::STATE_LOCK_TOOLTIP);
    } else if let Some(action) =
        crate::text_styles_ui::show_style_section(ui, &editor.session.course, &slide.id, obj)
    {
        action_to_run = Some(action);
    }
    inspector_separator(ui);

    // 5. Timing
    inspector_section_header(ui, "Timing");
    let mut timing_committed = false;

    ui.add_enabled_ui(!state_edit_locked, |ui| {
        inspector_row(ui, "Start (ms)", |ui| {
            let rem_w = ui.available_width().max(30.0);
            let r = add_drag_left(
                ui,
                [rem_w, coursecraft_render::CONTROL_COMPACT],
                DragValue::new(&mut props.start_ms)
                    .speed(10.0)
                    .suffix(" ms"),
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
                    DragValue::new(&mut props.end_ms).speed(10.0).suffix(" ms"),
                );
                let enter = r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                timing_committed |= r.drag_stopped() || r.lost_focus() || enter;
            });
        }
    })
    .response
    .on_disabled_hover_text(crate::STATE_LOCK_TOOLTIP);

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
    let rotation_overridden = state_ctx_ref.is_some_and(|ctx| ctx.state.rotation.is_some());
    inspector_row_state(
        ui,
        "Rotation",
        state_ctx_ref,
        rotation_overridden,
        "Rotation",
        serde_json::json!({ "rotation": null }),
        actions_to_run,
        |ui| {
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
                let curr = state_ctx_ref
                    .and_then(|ctx| ctx.state.rotation)
                    .unwrap_or(obj.rotation);
                if (rotation - coursecraft_course::normalize_rotation(curr)).abs() > 0.001
                    || state_ctx_ref.is_some()
                {
                    if let Some(ctx) = state_ctx_ref {
                        action_to_run = Some(patch_state_action(
                            ctx,
                            serde_json::json!({ "rotation": rotation }),
                        ));
                    } else if (rotation - coursecraft_course::normalize_rotation(obj.rotation))
                        .abs()
                        > 0.001
                    {
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
            }
        },
    );

    let flip_overridden =
        state_ctx_ref.is_some_and(|ctx| ctx.state.flip_h.is_some() || ctx.state.flip_v.is_some());
    inspector_row_state(
        ui,
        "Flip",
        state_ctx_ref,
        flip_overridden,
        "Flip",
        serde_json::json!({ "flip_h": null, "flip_v": null }),
        actions_to_run,
        |ui| {
            let eff_flip_h = state_ctx_ref
                .and_then(|ctx| ctx.state.flip_h)
                .unwrap_or(obj.flip_h);
            let eff_flip_v = state_ctx_ref
                .and_then(|ctx| ctx.state.flip_v)
                .unwrap_or(obj.flip_v);
            for (icon, label, axis, next_val) in [
                (
                    coursecraft_render::Icon::ArrowsOut,
                    "Flip horizontal",
                    "horizontal",
                    !eff_flip_h,
                ),
                (
                    coursecraft_render::Icon::ArrowsDownUp,
                    "Flip vertical",
                    "vertical",
                    !eff_flip_v,
                ),
            ] {
                if crate::widgets::icon_button(ui, icon, label).clicked() {
                    if let Some(ctx) = state_ctx_ref {
                        let key = if axis == "horizontal" {
                            "flip_h"
                        } else {
                            "flip_v"
                        };
                        action_to_run = Some(patch_state_action(
                            ctx,
                            serde_json::json!({ (key): next_val }),
                        ));
                    } else {
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
            }
        },
    );

    show_effect_controls(
        ui,
        props,
        &slide.id,
        &obj.id,
        state_ctx_ref,
        obj.shadow.is_some(),
        obj.glow.is_some(),
        effects_hidden_reason(obj, state_ctx_ref),
        actions_to_run,
    );

    show_drag_controls(ui, props, slide, obj, actions_to_run);

    inspector_separator(ui);
    match &obj.kind {
        ObjectKind::Text {
            text,
            style,
            runs,
            paragraphs,
            ..
        } => {
            let eff_runs = state_ctx_ref
                .and_then(|ctx| ctx.state.runs.as_deref())
                .unwrap_or(runs);
            let eff_paras = state_ctx_ref
                .and_then(|ctx| ctx.state.paragraphs.as_deref())
                .unwrap_or(paragraphs);
            let eff_text = state_ctx_ref
                .and_then(|ctx| ctx.state.text.as_deref())
                .unwrap_or(text.as_str());
            if let Some(action) = show_paragraph_controls(
                ui,
                props,
                &slide.id,
                &obj.id,
                eff_runs,
                eff_paras,
                &lists.slides,
                eff_text,
                "text",
                None,
                false,
            ) {
                action_to_run = Some(match state_ctx_ref {
                    Some(ctx) => {
                        let view = state_text_view(ctx, style, runs, paragraphs, text);
                        route_text_set_to_state(action, ctx, &view)
                    }
                    None => action,
                });
            }
        }
        ObjectKind::Shape {
            shape,
            text: Some(text),
            ..
        } if coursecraft_course::shape_allows_text(*shape) => {
            let eff_runs = state_ctx_ref
                .and_then(|ctx| ctx.state.runs.as_deref())
                .unwrap_or(&text.runs);
            let eff_paras = state_ctx_ref
                .and_then(|ctx| ctx.state.paragraphs.as_deref())
                .unwrap_or(&text.paragraphs);
            let eff_text = state_ctx_ref
                .and_then(|ctx| ctx.state.text.as_deref())
                .unwrap_or(text.text.as_str());
            if let Some(action) = show_paragraph_controls(
                ui,
                props,
                &slide.id,
                &obj.id,
                eff_runs,
                eff_paras,
                &lists.slides,
                eff_text,
                "shape",
                None,
                false,
            ) {
                action_to_run = Some(match state_ctx_ref {
                    Some(ctx) => {
                        let view = state_text_view(
                            ctx,
                            &text.style,
                            &text.runs,
                            &text.paragraphs,
                            &text.text,
                        );
                        route_text_set_to_state(action, ctx, &view)
                    }
                    None => action,
                });
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

    editor.pending_requests.append(&mut font_requests);

    if let Some(act) = action_to_run {
        actions_to_run.push(act);
    }
}

/// The Shape Style settings panel: Fill, Outline and Effects of the selected
/// object, drawn with the Format inspector's own rows (state edits included).
/// Fill and Outline are for shapes; Effects are for any object.
pub fn show_shape_style_panel(editor: &mut Editor, ui: &mut Ui) {
    prepare(editor, ui);
    let mut actions_to_run: Vec<(&'static str, Json)> = Vec::new();
    let avail_w = ui.available_width();
    egui::ScrollArea::vertical()
        .id_salt("shape_style_panel_scroll")
        .max_width(avail_w)
        .max_height(ui.available_height())
        .animated(false)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Frame::NONE
                .inner_margin(egui::Margin::same(coursecraft_render::PANEL_MARGIN as i8))
                .show(ui, |ui| {
                    shape_style_sections(editor, ui, &mut actions_to_run)
                });
        });
    finish(editor, actions_to_run);
}

fn shape_style_sections(
    editor: &mut Editor,
    ui: &mut Ui,
    actions_to_run: &mut Vec<(&'static str, Json)>,
) {
    use crate::panels::{Section, section_header};
    let focus = &mut editor.ui.panels;
    let props = &mut editor.ui.properties;
    let lists = &editor.ui.properties_lists;
    let slide = editor.session.course.slide(&editor.selection.slide);
    let obj = if editor.selection.is_multi() {
        None
    } else {
        slide.and_then(|slide| {
            editor
                .selection
                .object
                .as_ref()
                .and_then(|id| slide.object(id))
                .map(|(obj, _)| obj)
        })
    };
    let (Some(slide), Some(obj)) = (slide, obj) else {
        section_header(focus, ui, Section::Fill);
        ui.label(
            RichText::new(tl!("Select one object to style it."))
                .color(coursecraft_render::palette(ui.ctx()).text_secondary),
        );
        return;
    };
    let state = editor
        .ui
        .editing_state
        .as_ref()
        .filter(|s| s.slide_id == editor.selection.slide && s.object_id == obj.id)
        .and_then(|s| obj.states.iter().find(|st| st.name == s.state_name));
    let state_ctx = state.map(|state| StateEditCtx {
        slide_id: &slide.id,
        object_id: &obj.id,
        state_name: &state.name,
        state,
    });
    let state_ctx_ref = state_ctx.as_ref();
    let mut action_to_run: Option<(&'static str, Json)> = None;
    match &obj.kind {
        ObjectKind::Shape {
            shape,
            fill,
            outline,
            ..
        } => {
            section_header(focus, ui, Section::Fill);
            shape_fill_rows(
                ui,
                props,
                slide,
                obj,
                shape,
                fill,
                state_ctx_ref,
                &lists.media_names,
                actions_to_run,
                &mut action_to_run,
            );
            inspector_separator(ui);
            section_header(focus, ui, Section::Outline);
            shape_outline_rows(
                ui,
                props,
                slide,
                obj,
                shape,
                fill,
                outline,
                state_ctx_ref,
                actions_to_run,
                &mut action_to_run,
            );
        }
        _ => {
            section_header(focus, ui, Section::Fill);
            ui.label(
                RichText::new(tl!(
                    "Fill and outline are for shapes; the Format panel styles this object."
                ))
                .color(coursecraft_render::palette(ui.ctx()).text_secondary),
            );
        }
    }
    // Draws its own Effects heading.
    show_effect_controls(
        ui,
        props,
        &slide.id,
        &obj.id,
        state_ctx_ref,
        obj.shadow.is_some(),
        obj.glow.is_some(),
        effects_hidden_reason(obj, state_ctx_ref),
        actions_to_run,
    );
    if let Some(action) = action_to_run {
        actions_to_run.push(action);
    }
}

/// A shape's Fill rows: the fill on or off, its colour, and the fill style
/// (gradient, picture). Shared by the Format inspector and the Shape Style panel.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn shape_fill_rows(
    ui: &mut Ui,
    props: &mut PropertiesState,
    slide: &coursecraft_course::Slide,
    obj: &coursecraft_course::Object,
    shape: &ShapeKind,
    fill: &Color,
    state_ctx_ref: Option<&StateEditCtx<'_>>,
    media_names: &[String],
    actions_to_run: &mut Vec<(&'static str, Json)>,
    action_to_run: &mut Option<(&'static str, Json)>,
) {
    let base_kind = serde_json::to_value(&obj.kind).unwrap_or(Json::Null);
    let kind_with = |shape: ShapeKind, fill: &str| -> Json {
        let mut kind = base_kind.clone();
        kind["shape"] = serde_json::json!(shape);
        kind["fill"] = serde_json::json!(fill);
        kind
    };
    let eff_fill = state_ctx_ref
        .and_then(|ctx| ctx.state.fill.as_ref())
        .unwrap_or(fill);
    let has_fill = eff_fill.0 != "#00000000";
    let fill_overridden = state_ctx_ref.is_some_and(|ctx| ctx.state.fill.is_some());
    inspector_row_state(
        ui,
        "Fill",
        state_ctx_ref,
        fill_overridden,
        "Fill",
        serde_json::json!({ "fill": null }),
        actions_to_run,
        |ui| {
            let mut checked = has_fill;
            if ui.checkbox(&mut checked, "").changed() {
                let base_fill_style = match &obj.kind {
                    ObjectKind::Shape { fill_style, .. } => fill_style.clone(),
                    _ => None,
                };
                let state_name = state_ctx_ref.map(|ctx| ctx.state_name);
                if !checked {
                    let current_fill_style = if state_ctx_ref.is_some() {
                        props.fill_style.clone()
                    } else {
                        base_fill_style
                    };
                    props.remember_fill(
                        &slide.id,
                        &obj.id,
                        state_name,
                        &eff_fill.0,
                        current_fill_style,
                    );
                    props.shape_fill = "#00000000".to_owned();
                    props.fill_style = None;
                    if let Some(ctx) = state_ctx_ref {
                        *action_to_run = Some(patch_state_action(
                            ctx,
                            serde_json::json!({
                                "fill": "#00000000",
                                "fill_style": null,
                                "clear_fill_style": false,
                            }),
                        ));
                    } else {
                        let mut kind = kind_with(*shape, "#00000000");
                        kind["fill_style"] = Json::Null;
                        *action_to_run = Some((
                            "object.update",
                            serde_json::json!({
                                "slide": slide.id,
                                "object": obj.id,
                                "kind": kind,
                            }),
                        ));
                    }
                } else {
                    let (next_color, next_style) = props.restore_fill(
                        &slide.id,
                        &obj.id,
                        state_name,
                        &props.shape_fill,
                        "#D0D7DE",
                    );
                    props.shape_fill = next_color.clone();
                    props.fill_style = next_style.clone();
                    if let Some(ctx) = state_ctx_ref {
                        *action_to_run = Some(patch_state_action(
                            ctx,
                            serde_json::json!({
                                "fill": next_color,
                                "fill_style": fill_style_json(&next_style),
                                "clear_fill_style": false,
                            }),
                        ));
                    } else {
                        let mut kind = kind_with(*shape, &next_color);
                        kind["fill_style"] = fill_style_json(&next_style);
                        *action_to_run = Some((
                            "object.update",
                            serde_json::json!({
                                "slide": slide.id,
                                "object": obj.id,
                                "kind": kind,
                            }),
                        ));
                    }
                }
            }
            if has_fill {
                let (cr, valid_color) =
                    show_color_field(ui, "", &mut props.shape_fill, "prop_shape_fill");
                props.color_rect = Some(cr.rect);
                if is_committed(&cr, ui) && valid_color && props.shape_fill.trim() != eff_fill.0 {
                    if let Some(ctx) = state_ctx_ref {
                        *action_to_run = Some(patch_state_action(
                            ctx,
                            serde_json::json!({ "fill": props.shape_fill.trim() }),
                        ));
                    } else {
                        *action_to_run = Some((
                            "object.update",
                            serde_json::json!({
                                "slide": slide.id,
                                "object": obj.id,
                                "kind": kind_with(*shape, props.shape_fill.trim()),
                            }),
                        ));
                    }
                }
            }
        },
    );

    if has_fill {
        let base_fill_style_some = matches!(
            &obj.kind,
            ObjectKind::Shape {
                fill_style: Some(_),
                ..
            }
        );
        show_fill_style_editor(
            ui,
            props,
            &eff_fill.0,
            &slide.id,
            &obj.id,
            media_names,
            state_ctx_ref,
            base_fill_style_some,
            actions_to_run,
        );
    }
}

/// A shape's Outline rows: on or off, colour, width, dash and corners. Shared
/// by the Format inspector and the Shape Style panel.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn shape_outline_rows(
    ui: &mut Ui,
    props: &mut PropertiesState,
    slide: &coursecraft_course::Slide,
    obj: &coursecraft_course::Object,
    shape: &ShapeKind,
    fill: &Color,
    outline: &Option<coursecraft_course::Outline>,
    state_ctx_ref: Option<&StateEditCtx<'_>>,
    actions_to_run: &mut Vec<(&'static str, Json)>,
    action_to_run: &mut Option<(&'static str, Json)>,
) {
    let base_kind = serde_json::to_value(&obj.kind).unwrap_or(Json::Null);
    let kind_with = |shape: ShapeKind, fill: &str| -> Json {
        let mut kind = base_kind.clone();
        kind["shape"] = serde_json::json!(shape);
        kind["fill"] = serde_json::json!(fill);
        kind
    };
    let shape_outline_overridden =
        state_ctx_ref.is_some_and(|ctx| ctx.state.outline.is_some() || ctx.state.clear_outline);
    let build_shape_outline = |props: &PropertiesState| {
        outline_json(
            &props.shape_outline_color,
            props.shape_outline_width,
            props.shape_outline_dash,
            &props.shape_outline_dash_pattern,
            props.shape_outline_join,
        )
    };
    inspector_row_state(
        ui,
        "Outline",
        state_ctx_ref,
        shape_outline_overridden,
        "Outline",
        serde_json::json!({ "outline": null, "clear_outline": false }),
        actions_to_run,
        |ui| {
            let mut has_outline = props.shape_outline_enabled;
            if ui.checkbox(&mut has_outline, "").changed() {
                props.shape_outline_enabled = has_outline;
                if let Some(ctx) = state_ctx_ref {
                    let patch = if has_outline {
                        serde_json::json!({ "outline": build_shape_outline(props) })
                    } else if outline.is_some() {
                        serde_json::json!({ "clear_outline": true })
                    } else {
                        serde_json::json!({ "outline": null, "clear_outline": false })
                    };
                    *action_to_run = Some(patch_state_action(ctx, patch));
                } else {
                    let out_json = if has_outline {
                        build_shape_outline(props)
                    } else {
                        Json::Null
                    };
                    let mut kind = kind_with(*shape, &fill.0);
                    kind["outline"] = out_json;
                    *action_to_run = Some((
                        "object.update",
                        serde_json::json!({
                            "slide": slide.id,
                            "object": obj.id,
                            "kind": kind,
                        }),
                    ));
                }
            }
        },
    );

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
                if let Some(ctx) = state_ctx_ref {
                    *action_to_run = Some(patch_state_action(
                        ctx,
                        serde_json::json!({ "outline": build_shape_outline(props) }),
                    ));
                } else {
                    let mut kind = kind_with(*shape, &fill.0);
                    kind["outline"] = build_shape_outline(props);
                    *action_to_run = Some((
                        "object.update",
                        serde_json::json!({
                            "slide": slide.id,
                            "object": obj.id,
                            "kind": kind,
                        }),
                    ));
                }
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
                if let Some(ctx) = state_ctx_ref {
                    *action_to_run = Some(patch_state_action(
                        ctx,
                        serde_json::json!({ "outline": build_shape_outline(props) }),
                    ));
                } else {
                    let mut kind = kind_with(*shape, &fill.0);
                    kind["outline"] = build_shape_outline(props);
                    *action_to_run = Some((
                        "object.update",
                        serde_json::json!({
                            "slide": slide.id,
                            "object": obj.id,
                            "kind": kind,
                        }),
                    ));
                }
            }
        });

        if show_dash_controls(
            ui,
            "prop_shape_outline",
            &mut props.shape_outline_dash,
            &mut props.shape_outline_dash_pattern,
            &mut props.shape_outline_dash_length,
            &mut props.shape_outline_gap,
        ) {
            if let Some(ctx) = state_ctx_ref {
                *action_to_run = Some(patch_state_action(
                    ctx,
                    serde_json::json!({ "outline": build_shape_outline(props) }),
                ));
            } else {
                let mut kind = kind_with(*shape, &fill.0);
                kind["outline"] = build_shape_outline(props);
                *action_to_run = Some((
                    "object.update",
                    serde_json::json!({
                        "slide": slide.id,
                        "object": obj.id,
                        "kind": kind,
                    }),
                ));
            }
        }
        if show_corners_control(ui, "prop_shape_outline", &mut props.shape_outline_join) {
            if let Some(ctx) = state_ctx_ref {
                *action_to_run = Some(patch_state_action(
                    ctx,
                    serde_json::json!({ "outline": build_shape_outline(props) }),
                ));
            } else {
                *action_to_run = Some((
                    "object.set_outline_join",
                    serde_json::json!({
                        "slide": slide.id,
                        "object": obj.id,
                        "join": props.shape_outline_join,
                    }),
                ));
            }
        }
    }
}

fn show_slide_properties(
    props: &mut PropertiesState,
    slide: &coursecraft_course::Slide,
    ui: &mut Ui,
    is_master: bool,
    narrate_requested: &mut bool,
    bool_names: &[String],
    show_notes: bool,
) -> Option<(&'static str, Json)> {
    let mut action_to_run = None;

    inspector_row(ui, "Slide title", |ui| {
        let rem_w = ui.available_width().max(30.0);
        let r = ui.add_sized(
            [rem_w, coursecraft_render::CONTROL_COMPACT],
            TextEdit::singleline(&mut props.slide_title)
                .desired_width(rem_w)
                .id_salt("prop_slide_title"),
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
            let rem_w = ui.available_width().max(90.0);
            crate::widgets::styled_combo(ui, "prop_slide_revisit", rem_w)
                .selected_text(label)
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
        for (key, label, value, on_label, off_label) in [
            ("show_bar", "Player bar", overrides.show_bar, "On", "Off"),
            (
                "show_nav",
                "Navigation buttons",
                overrides.show_nav,
                "Show",
                "Hide",
            ),
            (
                "show_seekbar",
                "Seekbar",
                overrides.show_seekbar,
                "Show",
                "Hide",
            ),
            (
                "show_menu",
                "Slide menu",
                overrides.show_menu,
                "Show",
                "Hide",
            ),
        ] {
            inspector_row(ui, label, |ui| {
                if let Some(choice) =
                    tri_state_combo(ui, ("prop_slide_player", key), value, on_label, off_label)
                {
                    action_to_run = Some((
                        "slide.set_player_overrides",
                        serde_json::json!({ "slide": slide.id, key: choice }),
                    ));
                }
            });
        }
        // With the Notes panel open the notes live there instead of duplicating them here.
        if show_notes && let Some(act) = notes_editor(ui, props, slide, narrate_requested, 52.0) {
            action_to_run = Some(act);
        }
    }

    action_to_run
}

/// The slide's notes, full width: a "Notes" caption with Narrate…, then the
/// text, at least `height` tall. Shared by the Slide inspector and the Notes panel.
fn notes_editor(
    ui: &mut Ui,
    props: &mut PropertiesState,
    slide: &coursecraft_course::Slide,
    narrate_requested: &mut bool,
    height: f32,
) -> Option<(&'static str, Json)> {
    let p = coursecraft_render::palette(ui.ctx());
    ui.horizontal(|ui| {
        ui.label(coursecraft_render::body(tl!("Notes")).color(p.text_secondary));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if crate::quiet_button(ui, tl!("Narrate…"), crate::QuietTone::Accent, true).clicked()
            {
                *narrate_requested = true;
            }
        });
    });
    let r = ui.add_sized(
        [ui.available_width(), height.max(52.0)],
        TextEdit::multiline(&mut props.slide_notes)
            .id_salt("prop_slide_notes")
            .desired_rows(3),
    );
    (is_committed(&r, ui) && props.slide_notes != slide.notes).then(|| {
        (
            "slide.update",
            serde_json::json!({
                "slide": slide.id,
                "notes": props.slide_notes.trim(),
            }),
        )
    })
}

/// The Notes dock panel: the current slide's notes, filling the panel.
pub fn show_notes_panel(editor: &mut Editor, ui: &mut Ui) {
    prepare(editor, ui);
    let slide_id = editor.selection.slide.clone();
    let Some(slide) = editor.session.course.slide(&slide_id) else {
        ui.add_space(8.0);
        ui.label(tl!("Select a slide to write its notes."));
        return;
    };
    let mut actions_to_run = Vec::new();
    let mut narrate_requested = false;
    egui::Frame::NONE
        .inner_margin(egui::Margin::same(coursecraft_render::PANEL_MARGIN as i8))
        .show(ui, |ui| {
            let height = ui.available_height() - 28.0;
            if let Some(act) = notes_editor(
                ui,
                &mut editor.ui.properties,
                slide,
                &mut narrate_requested,
                height,
            ) {
                actions_to_run.push(act);
            }
        });
    if narrate_requested {
        editor.open_narration_from_notes(&slide_id);
    }
    finish(editor, actions_to_run);
}

/// "Course default / <on_label> / <off_label>". Returns the new choice (`None` clears it).
fn tri_state_combo(
    ui: &mut Ui,
    id_salt: (&str, &str),
    value: Option<bool>,
    on_label: &str,
    off_label: &str,
) -> Option<Option<bool>> {
    let label = match value {
        None => "Course default",
        Some(true) => on_label,
        Some(false) => off_label,
    };
    let mut chosen = None;
    let rem_w = ui.available_width().max(90.0);
    crate::widgets::styled_combo(ui, id_salt, rem_w)
        .selected_text(label)
        .show_ui(ui, |ui| {
            for (option, text) in [
                (None, "Course default"),
                (Some(true), on_label),
                (Some(false), off_label),
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
    let label = coursecraft_course::transition_def(&props.slide_transition_effect)
        .map_or("None", |d| d.label);
    inspector_row(ui, "Effect", |ui| {
        let rem_w = ui.available_width().max(30.0);
        crate::widgets::styled_combo(ui, "slide_transition_combo", rem_w)
            .selected_text(crate::tl!(label))
            .show_ui(ui, |ui| {
                let ids = std::iter::once(("none", "None")).chain(
                    coursecraft_course::TRANSITIONS
                        .iter()
                        .map(|d| (d.id, d.label)),
                );
                for (id, name) in ids {
                    if ui
                        .selectable_value(
                            &mut props.slide_transition_effect,
                            id.to_owned(),
                            crate::tl!(name),
                        )
                        .clicked()
                    {
                        trans_changed = true;
                    }
                }
            });
    });

    let def = coursecraft_course::transition_def(&props.slide_transition_effect);
    if let Some(def) = def {
        // Options follow the effect: keep the ones the new one also takes.
        match def.options_like {
            Some(like) if props.slide_transition_fx.id != like => {
                props.slide_transition_fx = if props.slide_transition_fx.id.is_empty() {
                    AnimEffect::new(like)
                } else {
                    props.slide_transition_fx.switched_to(like)
                };
            }
            None => props.slide_transition_fx = AnimEffect::default(),
            _ => {}
        }
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
        if def.options_like.is_some() {
            trans_changed |=
                crate::anim_panel::option_rows(ui, false, &mut props.slide_transition_fx);
        }
    }

    if trans_changed {
        let trans_json = def
            .and_then(|d| {
                let fx = d.options_like.map(|_| &props.slide_transition_fx);
                coursecraft_course::TransitionEffect::from_parts(d.id, fx)
            })
            .map_or(Json::Null, |effect| {
                serde_json::to_value(coursecraft_course::Transition {
                    effect,
                    duration_ms: props.slide_transition_duration_ms,
                })
                .unwrap_or(Json::Null)
            });
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
    /// The object has a crop stored in the course (or active state).
    has_crop: bool,
    /// The canvas is cropping this object right now.
    cropping: bool,
    /// The picture file is loaded, so crop mode can show it.
    can_crop: bool,
    base_shape: Option<ShapeKind>,
    state_ctx: Option<&'a StateEditCtx<'a>>,
    media_names: &'a [String],
}

#[allow(clippy::too_many_arguments)]
fn sync_dash_fields(
    outline: &coursecraft_course::Outline,
    dash: &mut coursecraft_course::DashStyle,
    pattern: &mut Option<Vec<f32>>,
    dash_len: &mut f32,
    gap: &mut f32,
) {
    *dash = outline.dash;
    *pattern = outline.dash_pattern.clone();
    let effective = outline.effective_dash_pattern();
    *dash_len = effective.first().copied().unwrap_or(3.0);
    *gap = effective.get(1).copied().unwrap_or(2.0);
}

fn outline_json(
    color: &str,
    width: f32,
    dash: coursecraft_course::DashStyle,
    dash_pattern: &Option<Vec<f32>>,
    join: coursecraft_course::LineJoin,
) -> Json {
    let mut obj = match dash_pattern {
        Some(pattern) => serde_json::json!({
            "color": color.trim(),
            "width": width,
            "dash": dash,
            "dash_pattern": pattern,
        }),
        None => serde_json::json!({
            "color": color.trim(),
            "width": width,
            "dash": dash,
        }),
    };
    if !join.is_default() {
        obj["join"] = serde_json::json!(join);
    }
    obj
}

fn show_corners_control(
    ui: &mut Ui,
    id_prefix: &str,
    join: &mut coursecraft_course::LineJoin,
) -> bool {
    let mut changed = false;
    inspector_row(ui, "Corners", |ui| {
        let label = match *join {
            coursecraft_course::LineJoin::Miter => "Sharp",
            coursecraft_course::LineJoin::Round => "Round",
            coursecraft_course::LineJoin::Bevel => "Bevel",
        };
        let rem_w = ui.available_width().max(30.0);
        crate::widgets::styled_combo(ui, format!("{id_prefix}_corners"), rem_w)
            .selected_text(label)
            .show_ui(ui, |ui| {
                for (choice, choice_label) in [
                    (coursecraft_course::LineJoin::Miter, "Sharp"),
                    (coursecraft_course::LineJoin::Round, "Round"),
                    (coursecraft_course::LineJoin::Bevel, "Bevel"),
                ] {
                    if ui.selectable_value(join, choice, choice_label).clicked() {
                        changed = true;
                    }
                }
            });
    });
    changed
}

fn show_dash_controls(
    ui: &mut Ui,
    id_prefix: &str,
    dash: &mut coursecraft_course::DashStyle,
    dash_pattern: &mut Option<Vec<f32>>,
    dash_len: &mut f32,
    gap: &mut f32,
) -> bool {
    let mut changed = false;
    inspector_row(ui, "Dash", |ui| {
        let label = match *dash {
            coursecraft_course::DashStyle::Solid => "Solid",
            coursecraft_course::DashStyle::Dash => "Dash",
            coursecraft_course::DashStyle::Dot => "Dot",
            coursecraft_course::DashStyle::DashDot => "Dash-dot",
            coursecraft_course::DashStyle::Custom => "Custom",
        };
        let rem_w = ui.available_width().max(30.0);
        crate::widgets::styled_combo(ui, format!("{id_prefix}_dash"), rem_w)
            .selected_text(label)
            .show_ui(ui, |ui| {
                for (choice, choice_label) in [
                    (coursecraft_course::DashStyle::Solid, "Solid"),
                    (coursecraft_course::DashStyle::Dash, "Dash"),
                    (coursecraft_course::DashStyle::Dot, "Dot"),
                    (coursecraft_course::DashStyle::DashDot, "Dash-dot"),
                    (coursecraft_course::DashStyle::Custom, "Custom"),
                ] {
                    if ui.selectable_value(dash, choice, choice_label).clicked() {
                        if choice == coursecraft_course::DashStyle::Custom {
                            if dash_pattern.is_none() {
                                *dash_pattern = Some(vec![*dash_len, *gap]);
                            }
                        } else {
                            *dash_pattern = None;
                            let def = choice.default_pattern();
                            if def.len() >= 2 {
                                *dash_len = def[0];
                                *gap = def[1];
                            }
                        }
                        changed = true;
                    }
                }
            });
    });

    if *dash != coursecraft_course::DashStyle::Solid {
        let mut spacing_committed = false;
        for (label, value) in [("Dash length", &mut *dash_len), ("Gap", &mut *gap)] {
            inspector_row(ui, label, |ui| {
                let rem_w = ui.available_width().max(30.0);
                let r = add_drag_left(
                    ui,
                    [rem_w, coursecraft_render::CONTROL_COMPACT],
                    DragValue::new(value)
                        .speed(0.1)
                        .range(0.2..=50.0)
                        .suffix("×"),
                );
                let enter = r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if (r.drag_stopped() || r.lost_focus() || enter)
                    && value.is_finite()
                    && (0.2..=50.0).contains(value)
                {
                    spacing_committed = true;
                }
            });
        }
        if spacing_committed {
            let d = dash_len.clamp(0.2, 50.0);
            let g = gap.clamp(0.2, 50.0);
            *dash_pattern = if *dash == coursecraft_course::DashStyle::DashDot {
                let dot = dash_pattern
                    .as_ref()
                    .and_then(|p| p.get(2).copied())
                    .unwrap_or(1.0);
                Some(vec![d, g, dot, g])
            } else {
                Some(vec![d, g])
            };
            changed = true;
        }
    }
    changed
}

#[allow(clippy::too_many_arguments)]
fn image_shape_label(shape: Option<ShapeKind>) -> &'static str {
    match shape {
        None => "Rectangle",
        Some(ShapeKind::Rectangle) => "Rectangle",
        Some(ShapeKind::RoundedRectangle) => "Rounded rectangle",
        Some(ShapeKind::Ellipse) => "Ellipse",
        Some(ShapeKind::Triangle) => "Triangle",
        Some(ShapeKind::Diamond) => "Diamond",
        Some(ShapeKind::Star5) => "Star",
        Some(ShapeKind::Pentagon) => "Pentagon",
        Some(ShapeKind::Hexagon) => "Hexagon",
        Some(ShapeKind::ArrowRight) => "Right arrow",
        Some(_) => "Shape",
    }
}

#[allow(clippy::too_many_arguments)]
fn show_image_properties(
    ui: &mut Ui,
    props: &mut PropertiesState,
    target: ImagePropTarget<'_>,
    pending_image_pick: &mut Option<crate::PendingImagePick>,
    replace_image_requested: &mut bool,
    reset_natural_size_src: &mut Option<String>,
    suggest_alt_requested: &mut bool,
    crop_action: &mut CropAction,
    actions: &mut Vec<(&'static str, Json)>,
    action_to_run: &mut Option<(&'static str, Json)>,
) {
    let state_ctx = target.state_ctx;
    if let Some(ctx) = state_ctx {
        let src_overridden = ctx.state.src.is_some();
        let display_src = ctx.state.src.as_deref().unwrap_or("Same as normal");
        inspector_row_state(
            ui,
            "Image",
            state_ctx,
            src_overridden,
            "Image",
            serde_json::json!({ "src": null }),
            actions,
            |ui| {
                ui.label(display_src);
            },
        );
        inspector_row(ui, "", |ui| {
            if ui.button("Choose image…").clicked() {
                *pending_image_pick = Some(crate::PendingImagePick::StateImage {
                    slide: ctx.slide_id.to_owned(),
                    object: ctx.object_id.to_owned(),
                    state: ctx.state_name.to_owned(),
                });
                *replace_image_requested = true;
            }
            ui.menu_button("From project…", |ui| {
                let images: Vec<&String> = target
                    .media_names
                    .iter()
                    .filter(|n| coursecraft_course::is_image_src(n))
                    .collect();
                if images.is_empty() {
                    ui.label("No images in project");
                } else {
                    for name in images {
                        let selected = ctx.state.src.as_deref() == Some(name.as_str());
                        if ui.selectable_label(selected, name).clicked() {
                            actions
                                .push(patch_state_action(ctx, serde_json::json!({ "src": name })));
                            ui.close();
                        }
                    }
                }
            });
        });
        if src_overridden {
            inspector_row(ui, "", |ui| {
                if ui.button("Clear").clicked() {
                    actions.push(patch_state_action(ctx, serde_json::json!({ "src": null })));
                }
            });
        }
    } else {
        inspector_row(ui, "Src", |ui| {
            ui.label(target.src);
        });
        inspector_row(ui, "", |ui| {
            if ui.button("Replace image…").clicked() {
                *replace_image_requested = true;
            }
        });
    }
    ui.add_enabled_ui(state_ctx.is_none(), |ui| {
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
    })
    .response
    .on_disabled_hover_text(crate::STATE_LOCK_TOOLTIP);

    *crop_action = show_crop_row(
        ui,
        target.has_crop,
        target.cropping,
        target.can_crop,
        state_ctx,
        actions,
    );

    let eff_shape = match state_ctx {
        Some(ctx) if ctx.state.clear_image_shape => None,
        Some(ctx) if ctx.state.image_shape.is_some() => ctx.state.image_shape,
        _ => target.base_shape,
    };
    let shape_overridden =
        state_ctx.is_some_and(|ctx| ctx.state.image_shape.is_some() || ctx.state.clear_image_shape);
    let mut shape_state_action = None;
    inspector_row_state(
        ui,
        "Image shape",
        state_ctx,
        shape_overridden,
        "Image shape",
        serde_json::json!({ "image_shape": null, "clear_image_shape": false }),
        actions,
        |ui| {
            let rem_w = ui.available_width().max(30.0);
            crate::widgets::styled_combo(ui, "prop_image_shape", rem_w)
                .selected_text(image_shape_label(eff_shape))
                .show_ui(ui, |ui| {
                    let choices: [(Option<ShapeKind>, &str); 9] = [
                        (None, "Rectangle"),
                        (Some(ShapeKind::RoundedRectangle), "Rounded rectangle"),
                        (Some(ShapeKind::Ellipse), "Ellipse"),
                        (Some(ShapeKind::Triangle), "Triangle"),
                        (Some(ShapeKind::Diamond), "Diamond"),
                        (Some(ShapeKind::Star5), "Star"),
                        (Some(ShapeKind::Pentagon), "Pentagon"),
                        (Some(ShapeKind::Hexagon), "Hexagon"),
                        (Some(ShapeKind::ArrowRight), "Right arrow"),
                    ];
                    for (choice, label) in choices {
                        let selected = eff_shape == choice;
                        if ui.selectable_label(selected, label).clicked() && !selected {
                            if let Some(ctx) = state_ctx {
                                let patch = match choice {
                                    Some(kind) => serde_json::json!({ "image_shape": kind }),
                                    None if target.base_shape.is_some() => {
                                        serde_json::json!({ "clear_image_shape": true })
                                    }
                                    None => {
                                        serde_json::json!({ "image_shape": null, "clear_image_shape": false })
                                    }
                                };
                                shape_state_action = Some(patch_state_action(ctx, patch));
                            } else {
                                *action_to_run = Some((
                                    "object.update",
                                    serde_json::json!({
                                        "slide": target.slide_id,
                                        "object": target.obj_id,
                                        "kind": {
                                            "type": "image",
                                            "src": target.src,
                                            "alt": target.alt,
                                            "shape": choice,
                                        },
                                    }),
                                ));
                            }
                        }
                    }
                });
        },
    );
    if let Some(act) = shape_state_action {
        actions.push(act);
    }
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
    if std::mem::take(&mut editor.ui.properties.pick_picture_fill)
        && let Some(object) = editor.selection.object.clone()
    {
        editor.pending_image_pick = Some(crate::PendingImagePick::PictureFill {
            slide: editor.selection.slide.clone(),
            object,
        });
        editor.request(crate::EditorRequest::PickImage);
    }
    if replace_image_requested {
        if editor.pending_image_pick.is_none() {
            editor.pending_image_pick = editor
                .selection
                .object
                .clone()
                .map(crate::PendingImagePick::ReplaceObject);
        }
        editor.request(crate::EditorRequest::PickImage);
    }
    if replace_audio_requested {
        editor.pending_image_pick = editor
            .selection
            .object
            .clone()
            .map(crate::PendingImagePick::ReplaceObject);
        editor.request(crate::EditorRequest::PickAudio);
    }
    if replace_video_requested {
        editor.pending_image_pick = editor
            .selection
            .object
            .clone()
            .map(crate::PendingImagePick::ReplaceObject);
        editor.request(crate::EditorRequest::PickVideo);
    }
    if pick_captions_requested {
        editor.pending_image_pick = editor
            .selection
            .object
            .clone()
            .map(crate::PendingImagePick::ReplaceObject);
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

/// The Auto-preview switch at the top of the Animate panel.
fn auto_preview_toggle(ui: &mut Ui, auto: &mut bool) {
    ui.checkbox(auto, crate::tl!("Auto-preview"))
        .on_hover_text(crate::tl!(
            "Play an animation on the canvas when you pick it or hover over an effect"
        ));
    ui.add_space(6.0);
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

    auto_preview_toggle(ui, &mut editor.ui.anim_preview.auto);
    let editor_playhead = editor.ui.timeline.playhead_ms;
    // By paragraph and by letter only mean something for objects with text.
    let has_text = match &obj.kind {
        coursecraft_course::ObjectKind::Text { .. } => true,
        coursecraft_course::ObjectKind::Shape { text, .. } => text.is_some(),
        _ => false,
    };
    let selected_ids = editor.selection.ids();
    let props = &mut editor.ui.properties;
    let pv = &mut editor.ui.anim_preview;
    let mut action_to_run: Option<(&'static str, Json)> = None;
    let (mut entrance_preview, mut exit_preview) = (None, None);

    show_anim_row(
        ui,
        "Entrance animation",
        "entrance",
        &mut props.entrance_effect,
        &mut props.entrance_fx,
        &mut props.entrance_duration_ms,
        &mut props.entrance_easing,
        (&mut props.entrance_sequence, has_text),
        &slide.id,
        &obj.id,
        &mut action_to_run,
        pv,
        &mut entrance_preview,
    );
    inspector_separator(ui);
    inspector_section_header(ui, "Emphasis");
    let playhead = editor_playhead;
    let emphasis_start =
        if playhead >= obj.timing.start_ms && obj.timing.end_ms.is_none_or(|end| playhead < end) {
            playhead
        } else {
            obj.timing.start_ms
        };
    let mut emphasis_requests = Vec::new();
    crate::anim_panel::show_emphasis_card(
        ui,
        &obj.emphasis,
        emphasis_start,
        &mut emphasis_requests,
    );
    inspector_separator(ui);
    show_anim_row(
        ui,
        "Exit animation",
        "exit",
        &mut props.exit_effect,
        &mut props.exit_fx,
        &mut props.exit_duration_ms,
        &mut props.exit_easing,
        (&mut props.exit_sequence, has_text),
        &slide.id,
        &obj.id,
        &mut action_to_run,
        pv,
        &mut exit_preview,
    );
    inspector_separator(ui);
    inspector_section_header(ui, "Motion paths");
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
                    let dur_id = ui.make_persistent_id(("mp_duration", path.id.as_str()));
                    let (_, committed_dur) = ui
                        .allocate_ui_with_layout(
                            vec2(64.0, coursecraft_render::CONTROL_COMPACT),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                crate::grid_panel::committed_drag(
                                    ui,
                                    dur_id,
                                    path.duration_ms,
                                    |d| d.speed(50.0).range(1..=60000).suffix(" ms"),
                                )
                            },
                        )
                        .inner;
                    if let Some(dur) = committed_dur {
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
                    let ease_label = crate::tl!(ease.label());
                    let rem_w = ui.available_width().max(30.0);
                    crate::widgets::styled_combo(ui, format!("mp_easing_{}", path.id), rem_w)
                        .selected_text(ease_label)
                        .show_ui(ui, |ui| {
                            for val in Easing::ALL {
                                let l = crate::tl!(val.label());
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

    let ctx = ui.ctx().clone();
    apply_card_preview(
        pv,
        props,
        (&slide.id, selected_ids.clone()),
        &ctx,
        false,
        entrance_preview,
    );
    apply_card_preview(
        pv,
        props,
        (&slide.id, selected_ids.clone()),
        &ctx,
        true,
        exit_preview,
    );
    for request in emphasis_requests {
        use crate::anim_panel::EmphasisRequest as R;
        match request {
            R::Add(e) => {
                if pv.auto {
                    pv.play_emphasis(&slide.id, selected_ids.clone(), e.clone());
                }
                actions_to_run.push((
                    "object.add_emphasis",
                    serde_json::json!({
                        "slide": slide.id, "object": obj.id, "effect": e.effect.id,
                        "start_ms": e.start_ms,
                    }),
                ));
            }
            R::Set(index, e) => {
                if pv.auto {
                    pv.play_emphasis(&slide.id, selected_ids.clone(), e.clone());
                }
                actions_to_run.push((
                    "object.set_emphasis",
                    serde_json::json!({
                        "slide": slide.id, "object": obj.id, "index": index, "emphasis": e,
                    }),
                ));
            }
            R::Remove(index) => actions_to_run.push((
                "object.remove_emphasis",
                serde_json::json!({ "slide": slide.id, "object": obj.id, "index": index }),
            )),
            R::Play(index) => {
                if let Some(e) = obj.emphasis.get(index) {
                    pv.play_emphasis(&slide.id, selected_ids.clone(), e.clone());
                }
            }
            R::Hover(effect) => {
                pv.hover_emphasis(
                    &slide.id,
                    selected_ids.clone(),
                    &effect,
                    ctx.cumulative_pass_nr(),
                );
            }
        }
    }

    if let Some(act) = action_to_run
        && !animation_unchanged(&act, obj)
    {
        actions_to_run.push(act);
    }
}

/// Animate inspector for a selected whole group: edits the `Group` record directly.
fn show_group_animations_section(
    editor: &mut Editor,
    ui: &mut Ui,
    gid: &str,
    actions_to_run: &mut Vec<(&'static str, Json)>,
) {
    let Some(slide) = editor.session.course.slide(&editor.selection.slide) else {
        return;
    };
    let Some((group, owner)) = slide.group(gid) else {
        return;
    };
    let slide_id = slide.id.clone();
    let group_entrance = group.entrance.clone();
    let group_exit = group.exit.clone();
    let group_emphasis = group.emphasis.clone();
    let editor_playhead = editor.ui.timeline.playhead_ms;
    let emphasis_start = slide.group_timing(gid, owner).map_or(editor_playhead, |t| {
        if editor_playhead >= t.start_ms && t.end_ms.is_none_or(|end| editor_playhead < end) {
            editor_playhead
        } else {
            t.start_ms
        }
    });

    inspector_section_header(ui, "Group animation");
    auto_preview_toggle(ui, &mut editor.ui.anim_preview.auto);
    let selected_ids = editor.selection.ids();
    let props = &mut editor.ui.properties;
    let pv = &mut editor.ui.anim_preview;
    let mut action_to_run: Option<(&'static str, Json)> = None;
    let (mut entrance_preview, mut exit_preview) = (None, None);

    show_anim_row(
        ui,
        "Entrance animation",
        "entrance",
        &mut props.entrance_effect,
        &mut props.entrance_fx,
        &mut props.entrance_duration_ms,
        &mut props.entrance_easing,
        (&mut props.entrance_sequence, false),
        &slide_id,
        gid,
        &mut action_to_run,
        pv,
        &mut entrance_preview,
    );
    inspector_separator(ui);
    inspector_section_header(ui, "Emphasis");
    let mut emphasis_requests = Vec::new();
    crate::anim_panel::show_emphasis_card(
        ui,
        &group_emphasis,
        emphasis_start,
        &mut emphasis_requests,
    );
    inspector_separator(ui);
    show_anim_row(
        ui,
        "Exit animation",
        "exit",
        &mut props.exit_effect,
        &mut props.exit_fx,
        &mut props.exit_duration_ms,
        &mut props.exit_easing,
        (&mut props.exit_sequence, false),
        &slide_id,
        gid,
        &mut action_to_run,
        pv,
        &mut exit_preview,
    );

    let ctx = ui.ctx().clone();
    apply_card_preview_for(
        pv,
        props,
        (&slide_id, selected_ids.clone(), Some(gid)),
        &ctx,
        false,
        entrance_preview,
    );
    apply_card_preview_for(
        pv,
        props,
        (&slide_id, selected_ids.clone(), Some(gid)),
        &ctx,
        true,
        exit_preview,
    );
    for request in emphasis_requests {
        use crate::anim_panel::EmphasisRequest as R;
        match request {
            R::Add(e) => {
                if pv.auto {
                    pv.play_group_emphasis(
                        &slide_id,
                        selected_ids.clone(),
                        gid.to_owned(),
                        e.clone(),
                    );
                }
                actions_to_run.push((
                    "object.add_emphasis",
                    serde_json::json!({
                        "slide": slide_id, "object": gid, "effect": e.effect.id,
                        "start_ms": e.start_ms,
                    }),
                ));
            }
            R::Set(index, e) => {
                if pv.auto {
                    pv.play_group_emphasis(
                        &slide_id,
                        selected_ids.clone(),
                        gid.to_owned(),
                        e.clone(),
                    );
                }
                actions_to_run.push((
                    "object.set_emphasis",
                    serde_json::json!({
                        "slide": slide_id, "object": gid, "index": index, "emphasis": e,
                    }),
                ));
            }
            R::Remove(index) => actions_to_run.push((
                "object.remove_emphasis",
                serde_json::json!({ "slide": slide_id, "object": gid, "index": index }),
            )),
            R::Play(index) => {
                if let Some(e) = group_emphasis.get(index) {
                    pv.play_group_emphasis(
                        &slide_id,
                        selected_ids.clone(),
                        gid.to_owned(),
                        e.clone(),
                    );
                }
            }
            R::Hover(effect) => {
                pv.hover_group_emphasis(
                    &slide_id,
                    selected_ids.clone(),
                    gid.to_owned(),
                    &effect,
                    ctx.cumulative_pass_nr(),
                );
            }
        }
    }

    if let Some(act) = action_to_run
        && !anim_slots_unchanged(&act, &group_entrance, &group_exit)
    {
        actions_to_run.push(act);
    }
}

/// Whether `act` is an `object.set_animation` that would store what `obj`
/// already has (a combo re-picking its value, a duration field losing focus
/// unchanged). Running it would add an undo step that changes nothing.
pub(crate) fn animation_unchanged(
    act: &(&'static str, Json),
    obj: &coursecraft_course::Object,
) -> bool {
    anim_slots_unchanged(act, &obj.entrance, &obj.exit)
}

fn anim_slots_unchanged(
    act: &(&'static str, Json),
    entrance: &Option<coursecraft_course::Animation>,
    exit: &Option<coursecraft_course::Animation>,
) -> bool {
    let (cmd, params) = act;
    if *cmd != "object.set_animation" {
        return false;
    }
    let stored = match params.get("which").and_then(Json::as_str) {
        Some("entrance") => entrance,
        Some("exit") => exit,
        _ => return false,
    };
    let wanted: Option<coursecraft_course::Animation> = match params.get("animation") {
        Some(Json::Null) | None => None,
        Some(value) => match serde_json::from_value(value.clone()) {
            Ok(animation) => Some(animation),
            Err(_) => return false,
        },
    };
    &wanted == stored
}

pub(crate) fn is_committed(r: &Response, ui: &Ui) -> bool {
    if r.lost_focus() {
        ui.input_mut(|i| {
            i.consume_key(egui::Modifiers::NONE, egui::Key::Enter);
        });
        return true;
    }
    if r.has_focus() && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)) {
        return true;
    }
    ui.data(|d| d.get_temp::<(egui::Id, u64)>(color_picked_id()))
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
fn show_crop_row(
    ui: &mut Ui,
    has_crop: bool,
    cropping: bool,
    can_crop: bool,
    state_ctx: Option<&StateEditCtx<'_>>,
    actions: &mut Vec<(&'static str, Json)>,
) -> CropAction {
    let mut action = CropAction::None;
    let crop_overridden =
        state_ctx.is_some_and(|ctx| ctx.state.crop.is_some() || ctx.state.clear_crop);
    inspector_row_state(
        ui,
        "Crop",
        state_ctx,
        crop_overridden,
        "Crop",
        serde_json::json!({ "crop": null, "clear_crop": false }),
        actions,
        |ui| {
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
        },
    );
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
    crate::widgets::styled_combo(ui, id, rem_w)
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
