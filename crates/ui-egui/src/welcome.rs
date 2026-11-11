//! Welcome screen for CourseCraft.
//!
//! Displayed on launch when no course is open and via `File ▸ Welcome screen`.
//! Features quick actions (New, Open, Import PPTX, Explore examples), recent files list,
//! and embedded example courses (hello-course and feature showcase).

use std::sync::Arc;

use coursecraft_course::Course;
use coursecraft_render::{
    Icon, RADIUS_LG, RADIUS_XL, SIZE_BODY, SIZE_BODY_STRONG, SIZE_DISPLAY, SIZE_SMALL, SIZE_TITLE,
    dialog_shadow, palette, theme_mode,
};
use egui::{
    Align, Align2, CornerRadius, CursorIcon, Layout, Rect, RichText, Sense, Stroke, Ui, pos2, vec2,
};

use crate::{Editor, EditorRequest};

const HELLO_COURSE_JSON: &str = include_str!("../../../examples/hello-course/course.json");
const SHOWCASE_COURSE_JSON: &str = include_str!("../../../examples/showcase/course.json");

const SHOWCASE_BANNER_BYTES: &[u8] = include_bytes!("../../../examples/showcase/media/banner.png");
const SHOWCASE_CHIME_BYTES: &[u8] = include_bytes!("../../../examples/showcase/media/chime.wav");
const SHOWCASE_FLOORPLAN_BYTES: &[u8] =
    include_bytes!("../../../examples/showcase/media/floorplan.png");
const SHOWCASE_INTRO_BYTES: &[u8] = include_bytes!("../../../examples/showcase/media/intro.mp4");

/// Identifier of an embedded example course.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExampleId {
    /// Minimal introduction course.
    HelloCourse,
    /// Full feature showcase course with media assets.
    FeatureShowcase,
}

impl ExampleId {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::HelloCourse => "Hello course",
            Self::FeatureShowcase => "Feature showcase",
        }
    }

    #[must_use]
    pub const fn description(self) -> &'static str {
        match self {
            Self::HelloCourse => "A quick 4-slide tour with quiz and tabs",
            Self::FeatureShowcase => "Full showcase with animations, media and questions",
        }
    }
}

/// Result of loading an embedded example course with its media assets.
pub type LoadedExample = (Course, Vec<(String, Arc<[u8]>)>);

/// Load an embedded example course and its media assets into memory.
///
/// # Errors
/// Returns an error string if JSON deserialization fails.
pub fn example_course_and_media(id: ExampleId) -> Result<LoadedExample, String> {
    match id {
        ExampleId::HelloCourse => {
            let course = Course::from_json(HELLO_COURSE_JSON)
                .map_err(|e| format!("Failed to parse hello course: {e}"))?;
            Ok((course, Vec::new()))
        }
        ExampleId::FeatureShowcase => {
            let course = Course::from_json(SHOWCASE_COURSE_JSON)
                .map_err(|e| format!("Failed to parse feature showcase: {e}"))?;
            let media = vec![
                ("banner.png".to_string(), Arc::from(SHOWCASE_BANNER_BYTES)),
                ("chime.wav".to_string(), Arc::from(SHOWCASE_CHIME_BYTES)),
                (
                    "floorplan.png".to_string(),
                    Arc::from(SHOWCASE_FLOORPLAN_BYTES),
                ),
                ("intro.mp4".to_string(), Arc::from(SHOWCASE_INTRO_BYTES)),
            ];
            Ok((course, media))
        }
    }
}

/// Render the welcome screen on `ui`.
pub fn show_welcome_screen(editor: &mut Editor, ui: &mut Ui, requests: &mut Vec<EditorRequest>) {
    let p = palette(ui.ctx());
    let mode = theme_mode(ui.ctx());

    // Ground fill across the whole available area
    let ground_rect = ui.available_rect_before_wrap();
    ui.painter()
        .rect_filled(ground_rect, CornerRadius::ZERO, p.app);

    let max_card_w = 720.0f32;
    let card_w = (ground_rect.width() - 32.0).clamp(320.0, max_card_w);
    let card_x = ground_rect.min.x + (ground_rect.width() - card_w) * 0.5;
    let card_y = ground_rect.min.y + ((ground_rect.height() - 560.0) * 0.5).max(16.0);

    let card_rect = Rect::from_min_size(pos2(card_x, card_y), vec2(card_w, 0.0));

    // Allocate child UI for the card
    let mut card_ui = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_size(
        card_rect.min,
        vec2(card_w, ground_rect.height() - card_y - 16.0),
    )));

    let card_frame = egui::Frame::NONE
        .fill(p.raised)
        .corner_radius(RADIUS_XL as u8)
        .stroke(Stroke::new(1.0, p.border))
        .shadow(dialog_shadow(mode))
        .inner_margin(32.0);

    card_frame.show(&mut card_ui, |ui| {
        // Top right: quiet "Back to course" if a course was already open
        if editor.ui.has_loaded_course {
            ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                if crate::quiet_button(ui, "Back to course", crate::QuietTone::Secondary, true)
                    .clicked()
                {
                    editor.ui.show_welcome = false;
                }
            });
            ui.add_space(-20.0);
        }

        // Header
        ui.label(
            RichText::new("CourseCraft")
                .size(SIZE_DISPLAY)
                .strong()
                .color(p.text),
        );
        ui.add_space(4.0);
        ui.label(
            RichText::new("Build interactive eLearning courses and publish them to any LMS.")
                .size(SIZE_BODY)
                .color(p.text_secondary),
        );

        ui.add_space(24.0);

        // 2x2 grid of action tiles
        let tile_gap = 12.0;
        let tile_w = ((ui.available_width() - tile_gap) * 0.5).max(140.0);
        let tile_h = 76.0;
        let tile_size = vec2(tile_w, tile_h);

        let mut clicked_example = None;

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = tile_gap;

            // Tile 1: New blank course
            if render_tile(
                ui,
                tile_size,
                Icon::FilePlus,
                "New blank course",
                "Start from a clean slate",
                true,
                "Create a new blank course",
            )
            .clicked()
            {
                requests.push(EditorRequest::New);
                editor.ui.show_welcome = false;
                editor.ui.has_loaded_course = true;
            }

            // Tile 2: Open…
            let open_tooltip = if editor.file_ops_available {
                "Open an existing course file"
            } else {
                "Opening and saving files is desktop-only in this version"
            };
            if render_tile(
                ui,
                tile_size,
                Icon::FolderOpen,
                "Open…",
                "Open an existing course file",
                editor.file_ops_available,
                open_tooltip,
            )
            .clicked()
            {
                requests.push(EditorRequest::Open);
                editor.ui.show_welcome = false;
                editor.ui.has_loaded_course = true;
            }
        });

        ui.add_space(tile_gap);

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = tile_gap;

            // Tile 3: Import PowerPoint…
            if render_tile(
                ui,
                tile_size,
                Icon::MicrosoftPowerpointLogo,
                "Import PowerPoint…",
                "Convert slides to course pages",
                true,
                "Import PowerPoint (.pptx) file",
            )
            .clicked()
            {
                requests.push(EditorRequest::PickPptx);
                editor.ui.show_welcome = false;
                editor.ui.has_loaded_course = true;
            }

            // Tile 4: Explore an example
            let resp = render_tile(
                ui,
                tile_size,
                Icon::BookOpen,
                "Explore an example",
                "Hello course or feature showcase",
                true,
                "Explore built-in sample courses",
            );

            egui::Popup::menu(&resp).show(|ui| {
                if ui.button("Hello course").clicked() {
                    clicked_example = Some(ExampleId::HelloCourse);
                    ui.close();
                }
                if ui.button("Feature showcase").clicked() {
                    clicked_example = Some(ExampleId::FeatureShowcase);
                    ui.close();
                }
            });
        });

        if let Some(id) = clicked_example {
            editor.open_example(id);
            requests.push(EditorRequest::OpenExample(id));
        }

        ui.add_space(28.0);

        // Recent courses section
        ui.label(
            RichText::new("Recent courses")
                .size(SIZE_TITLE)
                .strong()
                .color(p.text),
        );
        ui.add_space(8.0);

        let filtered_recent: Vec<(usize, String)> = editor
            .recent_files
            .iter()
            .enumerate()
            .map(|(i, s)| (i, s.clone()))
            .collect();

        if filtered_recent.is_empty() {
            ui.add_space(6.0);
            ui.label(
                RichText::new("No recent courses yet")
                    .size(SIZE_BODY)
                    .color(p.text_tertiary),
            );
            ui.add_space(14.0);
        } else {
            for (idx, path_str) in filtered_recent.into_iter().take(5) {
                let file_path = std::path::Path::new(&path_str);
                let name = file_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(&path_str);
                let dir_display = file_path.parent().and_then(|p| p.to_str()).unwrap_or("");

                let (row_rect, resp) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
                if ui.is_rect_visible(row_rect) {
                    if resp.hovered() {
                        ui.painter().rect_filled(
                            row_rect,
                            CornerRadius::same(RADIUS_LG as u8),
                            p.hover,
                        );
                    }
                    let text_y = row_rect.center().y - 7.0;
                    ui.painter().text(
                        pos2(row_rect.min.x + 8.0, text_y),
                        Align2::LEFT_CENTER,
                        name,
                        egui::FontId::proportional(SIZE_BODY),
                        p.text,
                    );
                    if !dir_display.is_empty() {
                        let truncated = if dir_display.len() > 50 {
                            format!("…{}", &dir_display[dir_display.len().saturating_sub(47)..])
                        } else {
                            dir_display.to_owned()
                        };
                        ui.painter().text(
                            pos2(row_rect.max.x - 8.0, text_y),
                            Align2::RIGHT_CENTER,
                            truncated,
                            egui::FontId::proportional(SIZE_SMALL),
                            p.text_tertiary,
                        );
                    }
                }

                if resp.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                    requests.push(EditorRequest::OpenRecent(idx));
                    editor.ui.show_welcome = false;
                    editor.ui.has_loaded_course = true;
                }
            }
            ui.add_space(8.0);
        }

        ui.add_space(16.0);
        ui.separator();
        ui.add_space(8.0);

        // Footer: Version info
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("CourseCraft v{}", env!("CARGO_PKG_VERSION")))
                    .size(SIZE_SMALL)
                    .color(p.text_tertiary),
            );
        });
    });
}

fn render_tile(
    ui: &mut Ui,
    size: egui::Vec2,
    tile_icon: Icon,
    title: &str,
    hint: &str,
    enabled: bool,
    tooltip: &str,
) -> egui::Response {
    let p = palette(ui.ctx());
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(size, sense);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, title.to_owned()));

    if ui.is_rect_visible(rect) {
        let hovered = enabled && resp.hovered();
        let pressed = enabled && resp.is_pointer_button_down_on();
        let bg = if !enabled {
            p.input
        } else if pressed {
            p.pressed
        } else if hovered {
            p.hover
        } else {
            p.input
        };

        ui.painter()
            .rect_filled(rect, CornerRadius::same(RADIUS_LG as u8), bg);
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(RADIUS_LG as u8),
            Stroke::new(1.0, p.border),
            egui::StrokeKind::Inside,
        );

        let icon_color = if !enabled {
            p.text_disabled
        } else if hovered {
            p.accent_text
        } else {
            p.text
        };
        let title_color = if !enabled { p.text_disabled } else { p.text };
        let hint_color = if !enabled {
            p.text_disabled
        } else {
            p.text_tertiary
        };

        // Icon at left
        let icon_galley = ui.painter().layout_no_wrap(
            tile_icon.codepoint().to_string(),
            egui::FontId::new(24.0, egui::FontFamily::Name("phosphor".into())),
            icon_color,
        );
        let icon_pos = pos2(
            rect.min.x + 16.0,
            rect.center().y - icon_galley.size().y * 0.5,
        );
        ui.painter().galley(icon_pos, icon_galley, icon_color);

        // Title and hint text to the right of icon
        let text_left = rect.min.x + 52.0;
        let title_galley = ui.painter().layout_no_wrap(
            title.to_owned(),
            egui::FontId::proportional(SIZE_BODY_STRONG),
            title_color,
        );
        let hint_galley = ui.painter().layout_no_wrap(
            hint.to_owned(),
            egui::FontId::proportional(SIZE_SMALL),
            hint_color,
        );

        let title_pos = pos2(text_left, rect.center().y - 15.0);
        let hint_pos = pos2(text_left, rect.center().y + 4.0);

        ui.painter().galley(title_pos, title_galley, title_color);
        ui.painter().galley(hint_pos, hint_galley, hint_color);
    }

    let resp = resp.on_hover_text(tooltip);
    if enabled {
        resp.on_hover_cursor(CursorIcon::PointingHand)
    } else {
        resp
    }
}
