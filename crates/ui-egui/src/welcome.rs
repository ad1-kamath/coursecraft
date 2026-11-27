//! Welcome screen for CourseCraft.
//!
//! Displayed on launch when no course is open and via `File ▸ Welcome screen`.
//! Features quick actions (New, Open, Import PPTX, Explore examples), recent files list,
//! and embedded example courses (Healthy Habits at Work, Workplace Safety Basics and the Feature Tour).

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

const HEALTHY_HABITS_JSON: &str = include_str!("../../../examples/healthy-habits/course.json");
/// Media files of the Healthy Habits course, by their path under `media/`.
const HEALTHY_HABITS_MEDIA: &[(&str, &[u8])] = &[
    (
        "badge-round.jpg",
        include_bytes!("../../../examples/healthy-habits/media/badge-round.jpg"),
    ),
    (
        "bonus.mp3",
        include_bytes!("../../../examples/healthy-habits/media/bonus.mp3"),
    ),
    (
        "bonus.vtt",
        include_bytes!("../../../examples/healthy-habits/media/bonus.vtt"),
    ),
    (
        "breathe-circle.json",
        include_bytes!("../../../examples/healthy-habits/media/breathe-circle.json"),
    ),
    (
        "breathe-round.jpg",
        include_bytes!("../../../examples/healthy-habits/media/breathe-round.jpg"),
    ),
    (
        "breathe.jpg",
        include_bytes!("../../../examples/healthy-habits/media/breathe.jpg"),
    ),
    (
        "breathe.mp3",
        include_bytes!("../../../examples/healthy-habits/media/breathe.mp3"),
    ),
    (
        "breathe.vtt",
        include_bytes!("../../../examples/healthy-habits/media/breathe.vtt"),
    ),
    (
        "breathe_intro.mp3",
        include_bytes!("../../../examples/healthy-habits/media/breathe_intro.mp3"),
    ),
    (
        "breathe_intro.vtt",
        include_bytes!("../../../examples/healthy-habits/media/breathe_intro.vtt"),
    ),
    (
        "certificate.mp3",
        include_bytes!("../../../examples/healthy-habits/media/certificate.mp3"),
    ),
    (
        "certificate.vtt",
        include_bytes!("../../../examples/healthy-habits/media/certificate.vtt"),
    ),
    (
        "check.mp3",
        include_bytes!("../../../examples/healthy-habits/media/check.mp3"),
    ),
    (
        "check.vtt",
        include_bytes!("../../../examples/healthy-habits/media/check.vtt"),
    ),
    (
        "checkin.mp3",
        include_bytes!("../../../examples/healthy-habits/media/checkin.mp3"),
    ),
    (
        "checkin.vtt",
        include_bytes!("../../../examples/healthy-habits/media/checkin.vtt"),
    ),
    (
        "connect-round.jpg",
        include_bytes!("../../../examples/healthy-habits/media/connect-round.jpg"),
    ),
    (
        "connect.jpg",
        include_bytes!("../../../examples/healthy-habits/media/connect.jpg"),
    ),
    (
        "connect_intro.mp3",
        include_bytes!("../../../examples/healthy-habits/media/connect_intro.mp3"),
    ),
    (
        "connect_intro.vtt",
        include_bytes!("../../../examples/healthy-habits/media/connect_intro.vtt"),
    ),
    (
        "day.jpg",
        include_bytes!("../../../examples/healthy-habits/media/day.jpg"),
    ),
    (
        "day.mp3",
        include_bytes!("../../../examples/healthy-habits/media/day.mp3"),
    ),
    (
        "day.vtt",
        include_bytes!("../../../examples/healthy-habits/media/day.vtt"),
    ),
    (
        "desk.jpg",
        include_bytes!("../../../examples/healthy-habits/media/desk.jpg"),
    ),
    (
        "desk.mp3",
        include_bytes!("../../../examples/healthy-habits/media/desk.mp3"),
    ),
    (
        "desk.vtt",
        include_bytes!("../../../examples/healthy-habits/media/desk.vtt"),
    ),
    (
        "evening-round.jpg",
        include_bytes!("../../../examples/healthy-habits/media/evening-round.jpg"),
    ),
    (
        "evening.jpg",
        include_bytes!("../../../examples/healthy-habits/media/evening.jpg"),
    ),
    (
        "eyes.jpg",
        include_bytes!("../../../examples/healthy-habits/media/eyes.jpg"),
    ),
    (
        "eyes.mp3",
        include_bytes!("../../../examples/healthy-habits/media/eyes.mp3"),
    ),
    (
        "eyes.vtt",
        include_bytes!("../../../examples/healthy-habits/media/eyes.vtt"),
    ),
    (
        "eyes_done.mp3",
        include_bytes!("../../../examples/healthy-habits/media/eyes_done.mp3"),
    ),
    (
        "eyes_done.vtt",
        include_bytes!("../../../examples/healthy-habits/media/eyes_done.vtt"),
    ),
    (
        "failed.mp3",
        include_bytes!("../../../examples/healthy-habits/media/failed.mp3"),
    ),
    (
        "failed.vtt",
        include_bytes!("../../../examples/healthy-habits/media/failed.vtt"),
    ),
    (
        "finish.mp3",
        include_bytes!("../../../examples/healthy-habits/media/finish.mp3"),
    ),
    (
        "finish.vtt",
        include_bytes!("../../../examples/healthy-habits/media/finish.vtt"),
    ),
    (
        "fonts/dm-serif-display-regular.ttf",
        include_bytes!("../../../examples/healthy-habits/media/fonts/dm-serif-display-regular.ttf"),
    ),
    (
        "h_breathe.mp3",
        include_bytes!("../../../examples/healthy-habits/media/h_breathe.mp3"),
    ),
    (
        "h_breathe.vtt",
        include_bytes!("../../../examples/healthy-habits/media/h_breathe.vtt"),
    ),
    (
        "h_connect.mp3",
        include_bytes!("../../../examples/healthy-habits/media/h_connect.mp3"),
    ),
    (
        "h_connect.vtt",
        include_bytes!("../../../examples/healthy-habits/media/h_connect.vtt"),
    ),
    (
        "h_move.mp3",
        include_bytes!("../../../examples/healthy-habits/media/h_move.mp3"),
    ),
    (
        "h_move.vtt",
        include_bytes!("../../../examples/healthy-habits/media/h_move.vtt"),
    ),
    (
        "habits.mp3",
        include_bytes!("../../../examples/healthy-habits/media/habits.mp3"),
    ),
    (
        "habits.vtt",
        include_bytes!("../../../examples/healthy-habits/media/habits.vtt"),
    ),
    (
        "hero.jpg",
        include_bytes!("../../../examples/healthy-habits/media/hero.jpg"),
    ),
    (
        "move-round.jpg",
        include_bytes!("../../../examples/healthy-habits/media/move-round.jpg"),
    ),
    (
        "move.jpg",
        include_bytes!("../../../examples/healthy-habits/media/move.jpg"),
    ),
    (
        "move_intro.mp3",
        include_bytes!("../../../examples/healthy-habits/media/move_intro.mp3"),
    ),
    (
        "move_intro.vtt",
        include_bytes!("../../../examples/healthy-habits/media/move_intro.vtt"),
    ),
    (
        "music.mp3",
        include_bytes!("../../../examples/healthy-habits/media/music.mp3"),
    ),
    (
        "myth.mp3",
        include_bytes!("../../../examples/healthy-habits/media/myth.mp3"),
    ),
    (
        "myth.vtt",
        include_bytes!("../../../examples/healthy-habits/media/myth.vtt"),
    ),
    (
        "name.mp3",
        include_bytes!("../../../examples/healthy-habits/media/name.mp3"),
    ),
    (
        "name.vtt",
        include_bytes!("../../../examples/healthy-habits/media/name.vtt"),
    ),
    (
        "passed.mp3",
        include_bytes!("../../../examples/healthy-habits/media/passed.mp3"),
    ),
    (
        "passed.vtt",
        include_bytes!("../../../examples/healthy-habits/media/passed.vtt"),
    ),
    (
        "plan.jpg",
        include_bytes!("../../../examples/healthy-habits/media/plan.jpg"),
    ),
    (
        "plan.mp3",
        include_bytes!("../../../examples/healthy-habits/media/plan.mp3"),
    ),
    (
        "plan.vtt",
        include_bytes!("../../../examples/healthy-habits/media/plan.vtt"),
    ),
    (
        "quiz.mp3",
        include_bytes!("../../../examples/healthy-habits/media/quiz.mp3"),
    ),
    (
        "quiz.vtt",
        include_bytes!("../../../examples/healthy-habits/media/quiz.vtt"),
    ),
    (
        "reflect.mp3",
        include_bytes!("../../../examples/healthy-habits/media/reflect.mp3"),
    ),
    (
        "reflect.vtt",
        include_bytes!("../../../examples/healthy-habits/media/reflect.vtt"),
    ),
    (
        "scenario.mp3",
        include_bytes!("../../../examples/healthy-habits/media/scenario.mp3"),
    ),
    (
        "scenario.vtt",
        include_bytes!("../../../examples/healthy-habits/media/scenario.vtt"),
    ),
    (
        "sort.mp3",
        include_bytes!("../../../examples/healthy-habits/media/sort.mp3"),
    ),
    (
        "sort.vtt",
        include_bytes!("../../../examples/healthy-habits/media/sort.vtt"),
    ),
    (
        "steps.mp3",
        include_bytes!("../../../examples/healthy-habits/media/steps.mp3"),
    ),
    (
        "steps.vtt",
        include_bytes!("../../../examples/healthy-habits/media/steps.vtt"),
    ),
    (
        "stretch-poster.jpg",
        include_bytes!("../../../examples/healthy-habits/media/stretch-poster.jpg"),
    ),
    (
        "stretch.mp4",
        include_bytes!("../../../examples/healthy-habits/media/stretch.mp4"),
    ),
    (
        "stretch.vtt",
        include_bytes!("../../../examples/healthy-habits/media/stretch.vtt"),
    ),
    (
        "tips.mp3",
        include_bytes!("../../../examples/healthy-habits/media/tips.mp3"),
    ),
    (
        "tips.vtt",
        include_bytes!("../../../examples/healthy-habits/media/tips.vtt"),
    ),
    (
        "wave-round.jpg",
        include_bytes!("../../../examples/healthy-habits/media/wave-round.jpg"),
    ),
    (
        "welcome.mp3",
        include_bytes!("../../../examples/healthy-habits/media/welcome.mp3"),
    ),
    (
        "welcome.vtt",
        include_bytes!("../../../examples/healthy-habits/media/welcome.vtt"),
    ),
];
const SAFETY_BASICS_JSON: &str = include_str!("../../../examples/safety-basics/course.json");
/// Media files of the Workplace Safety Basics course, by their path under `media/`.
const SAFETY_BASICS_MEDIA: &[(&str, &[u8])] = &[
    (
        "hero.jpg",
        include_bytes!("../../../examples/safety-basics/media/hero.jpg"),
    ),
    (
        "lifting.jpg",
        include_bytes!("../../../examples/safety-basics/media/lifting.jpg"),
    ),
    (
        "spills.jpg",
        include_bytes!("../../../examples/safety-basics/media/spills.jpg"),
    ),
    (
        "trips.jpg",
        include_bytes!("../../../examples/safety-basics/media/trips.jpg"),
    ),
    (
        "workshop.jpg",
        include_bytes!("../../../examples/safety-basics/media/workshop.jpg"),
    ),
];
const FEATURE_TOUR_JSON: &str = include_str!("../../../examples/feature-tour/course.json");
/// Media files of the Feature Tour course, by their path under `media/`.
const FEATURE_TOUR_MEDIA: &[(&str, &[u8])] = &[
    (
        "campus-map.jpg",
        include_bytes!("../../../examples/feature-tour/media/campus-map.jpg"),
    ),
    (
        "chime.vtt",
        include_bytes!("../../../examples/feature-tour/media/chime.vtt"),
    ),
    (
        "chime.wav",
        include_bytes!("../../../examples/feature-tour/media/chime.wav"),
    ),
    (
        "done.jpg",
        include_bytes!("../../../examples/feature-tour/media/done.jpg"),
    ),
    (
        "hero.jpg",
        include_bytes!("../../../examples/feature-tour/media/hero.jpg"),
    ),
    (
        "intro.mp4",
        include_bytes!("../../../examples/feature-tour/media/intro.mp4"),
    ),
    (
        "intro.vtt",
        include_bytes!("../../../examples/feature-tour/media/intro.vtt"),
    ),
    (
        "layers.jpg",
        include_bytes!("../../../examples/feature-tour/media/layers.jpg"),
    ),
    (
        "media.jpg",
        include_bytes!("../../../examples/feature-tour/media/media.jpg"),
    ),
    (
        "quiz.jpg",
        include_bytes!("../../../examples/feature-tour/media/quiz.jpg"),
    ),
];

const TEMPLATE_BLANK_JSON: &str = include_str!("../../../examples/templates/blank.json");
const TEMPLATE_QUICK_CHECK_JSON: &str =
    include_str!("../../../examples/templates/quick-knowledge-check.json");
const TEMPLATE_LINEAR_LESSON_JSON: &str =
    include_str!("../../../examples/templates/linear-lesson.json");
const TEMPLATE_BRANCHING_SCENARIO_JSON: &str =
    include_str!("../../../examples/templates/branching-scenario.json");

/// Identifier of an embedded starter template course.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateId {
    Blank,
    QuickKnowledgeCheck,
    LinearLesson,
    BranchingScenario,
}

impl TemplateId {
    /// Every starter template, in display order.
    pub const ALL: &'static [Self] = &[
        Self::Blank,
        Self::QuickKnowledgeCheck,
        Self::LinearLesson,
        Self::BranchingScenario,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Blank => "Blank",
            Self::QuickKnowledgeCheck => "Quick Knowledge Check",
            Self::LinearLesson => "Linear Lesson",
            Self::BranchingScenario => "Branching Scenario",
        }
    }

    #[must_use]
    pub const fn description(self) -> &'static str {
        match self {
            Self::Blank => "One clean slide ready to author",
            Self::QuickKnowledgeCheck => "Intro, two quiz questions and results",
            Self::LinearLesson => "Objectives, tabbed steps and summary",
            Self::BranchingScenario => "Decision point, two outcomes and debrief",
        }
    }

    #[must_use]
    pub const fn icon(self) -> Icon {
        match self {
            Self::Blank => Icon::FilePlus,
            Self::QuickKnowledgeCheck => Icon::ListChecks,
            Self::LinearLesson => Icon::Presentation,
            Self::BranchingScenario => Icon::Path,
        }
    }
}

/// Load an embedded starter template course into memory.
///
/// # Errors
/// Returns an error string if JSON deserialization fails.
pub fn template_course(id: TemplateId) -> Result<Course, String> {
    let json = match id {
        TemplateId::Blank => TEMPLATE_BLANK_JSON,
        TemplateId::QuickKnowledgeCheck => TEMPLATE_QUICK_CHECK_JSON,
        TemplateId::LinearLesson => TEMPLATE_LINEAR_LESSON_JSON,
        TemplateId::BranchingScenario => TEMPLATE_BRANCHING_SCENARIO_JSON,
    };
    Course::from_json(json).map_err(|e| format!("Failed to parse {} template: {e}", id.name()))
}

/// Identifier of an embedded example course.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExampleId {
    /// The sample course a fresh launch opens: every feature in a wellbeing module.
    HealthyHabits,
    /// Workplace Safety Basics: a short six-slide course with tabs, a hotspot and a quiz.
    HelloCourse,
    /// The CourseCraft Feature Tour: one stop per area of the app, with media and six question types.
    FeatureShowcase,
}

impl ExampleId {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::HealthyHabits => "Healthy Habits at Work",
            Self::HelloCourse => "Workplace Safety Basics",
            Self::FeatureShowcase => "Feature tour",
        }
    }

    #[must_use]
    pub const fn description(self) -> &'static str {
        match self {
            Self::HealthyHabits => "The sample course: every feature in one short module",
            Self::HelloCourse => "A short course with tabs, a hotspot and a quiz",
            Self::FeatureShowcase => "A tour of media, layers, motion and six question types",
        }
    }
}

/// Result of loading an embedded example course with its media assets.
pub type LoadedExample = (Course, Vec<(String, Arc<[u8]>)>);

fn embedded_media(files: &[(&str, &'static [u8])]) -> Vec<(String, Arc<[u8]>)> {
    files
        .iter()
        .map(|(name, bytes)| ((*name).to_string(), Arc::from(*bytes)))
        .collect()
}

/// Load an embedded example course and its media assets into memory.
///
/// # Errors
/// Returns an error string if JSON deserialization fails.
pub fn example_course_and_media(id: ExampleId) -> Result<LoadedExample, String> {
    match id {
        ExampleId::HealthyHabits => {
            let course = Course::from_json(HEALTHY_HABITS_JSON)
                .map_err(|e| format!("Failed to parse Healthy Habits at Work: {e}"))?;
            Ok((course, embedded_media(HEALTHY_HABITS_MEDIA)))
        }
        ExampleId::HelloCourse => {
            let course = Course::from_json(SAFETY_BASICS_JSON)
                .map_err(|e| format!("Failed to parse Workplace Safety Basics: {e}"))?;
            Ok((course, embedded_media(SAFETY_BASICS_MEDIA)))
        }
        ExampleId::FeatureShowcase => {
            let course = Course::from_json(FEATURE_TOUR_JSON)
                .map_err(|e| format!("Failed to parse the Feature Tour: {e}"))?;
            Ok((course, embedded_media(FEATURE_TOUR_MEDIA)))
        }
    }
}

/// Render the welcome screen on `ui`.
pub fn show_welcome_screen(editor: &mut Editor, ui: &mut Ui, requests: &mut Vec<EditorRequest>) {
    // Esc is Back when a course is open behind the start page.
    if crate::file_page::esc_back(editor, ui.ctx()) {
        return;
    }
    let p = palette(ui.ctx());
    let mode = theme_mode(ui.ctx());

    // Ground fill across the whole available area
    let ground_rect = ui.available_rect_before_wrap();
    ui.painter()
        .rect_filled(ground_rect, CornerRadius::ZERO, p.app);

    let max_card_w = 720.0f32;
    let card_w = (ground_rect.width() - 32.0).clamp(320.0, max_card_w);
    let card_x = ground_rect.min.x + (ground_rect.width() - card_w) * 0.5;
    let card_y = ground_rect.min.y + ((ground_rect.height() - 680.0) * 0.5).max(16.0);

    let card_rect = Rect::from_min_size(pos2(card_x, card_y), vec2(card_w, 0.0));

    // Allocate child UI for the card
    let mut card_ui = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_size(
        card_rect.min,
        vec2(card_w, (ground_rect.max.y - card_y - 16.0).max(120.0)),
    )));

    let card_frame = egui::Frame::NONE
        .fill(p.raised)
        .corner_radius(RADIUS_XL as u8)
        .stroke(Stroke::new(1.0, p.border))
        .shadow(dialog_shadow(mode))
        .inner_margin(24.0);

    card_frame.show(&mut card_ui, |ui| {
        egui::ScrollArea::vertical()
            .auto_shrink([false, true])
            .show(ui, |ui| {
                start_content(editor, ui, requests, true);
            });
    });
}

/// The start page: what to make or open, templates, examples and recent
/// courses. Drawn in the welcome card and on the File page's Start section;
/// `back` draws "Back to course" in the title row when a course is open.
pub fn start_content(
    editor: &mut Editor,
    ui: &mut Ui,
    requests: &mut Vec<EditorRequest>,
    back: bool,
) {
    let p = palette(ui.ctx());
    // Title row: "CourseCraft" on the left and, if a course was already open, a quiet
    // tl!("Back to course") on the right, both centred on the same line.
    let title_row_h = SIZE_DISPLAY * 1.4;
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), title_row_h),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.label(
                RichText::new("CourseCraft")
                    .size(SIZE_DISPLAY)
                    .strong()
                    .color(p.text),
            );
            if back && editor.ui.has_loaded_course {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if crate::quiet_button(
                        ui,
                        tl!("Back to course"),
                        crate::QuietTone::Secondary,
                        true,
                    )
                    .clicked()
                    {
                        editor.ui.show_welcome = false;
                    }
                });
            }
        },
    );
    ui.add_space(4.0);
    ui.label(
        RichText::new(tl!(
            "Build interactive eLearning courses and publish them to any LMS."
        ))
        .size(SIZE_BODY)
        .color(p.text_secondary),
    );

    ui.add_space(16.0);

    // 2x2 grid of action tiles
    let tile_gap = 8.0;
    let tile_w = ((ui.available_width() - tile_gap) * 0.5).max(140.0);
    let tile_h = 60.0;
    let tile_size = vec2(tile_w, tile_h);

    let mut clicked_example = None;

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = tile_gap;

        // Tile 1: New blank course
        if render_tile(
            ui,
            tile_size,
            Icon::FilePlus,
            tl!("New blank course"),
            tl!("Start from a clean slate"),
            true,
            tl!("Create a new blank course"),
        )
        .clicked()
        {
            requests.push(EditorRequest::New);
        }

        // Tile 2: Open…
        let open_tooltip = if editor.file_ops_available {
            tl!("Open an existing course file")
        } else {
            tl!("Opening and saving files is desktop-only in this version")
        };
        if render_tile(
            ui,
            tile_size,
            Icon::FolderOpen,
            tl!("Open…"),
            tl!("Open an existing course file"),
            editor.file_ops_available,
            open_tooltip,
        )
        .clicked()
        {
            requests.push(EditorRequest::Open);
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
            tl!("Import PowerPoint…"),
            tl!("Convert slides to course pages"),
            true,
            tl!("Import PowerPoint (.pptx) file"),
        )
        .clicked()
        {
            requests.push(EditorRequest::PickPptx);
        }

        // Tile 4: Explore an example
        let resp = render_tile(
            ui,
            tile_size,
            Icon::BookOpen,
            tl!("Explore an example"),
            tl!("Three sample courses to explore"),
            true,
            tl!("Explore built-in sample courses"),
        );

        egui::Popup::menu(&resp).show(|ui| {
            if ui.button(tl!(ExampleId::HealthyHabits.name())).clicked() {
                clicked_example = Some(ExampleId::HealthyHabits);
                ui.close();
            }
            if ui.button(tl!(ExampleId::HelloCourse.name())).clicked() {
                clicked_example = Some(ExampleId::HelloCourse);
                ui.close();
            }
            if ui.button(tl!(ExampleId::FeatureShowcase.name())).clicked() {
                clicked_example = Some(ExampleId::FeatureShowcase);
                ui.close();
            }
        });
    });

    if let Some(id) = clicked_example {
        editor.open_example(id);
        requests.push(EditorRequest::OpenExample(id));
    }

    ui.add_space(14.0);

    // Start from a template section
    ui.label(
        RichText::new(tl!("Start from a template"))
            .size(SIZE_TITLE)
            .strong()
            .color(p.text),
    );
    ui.add_space(6.0);

    let tpl_gap = 8.0;
    let tpl_w = ((ui.available_width() - tpl_gap) * 0.5).max(140.0);
    let tpl_size = vec2(tpl_w, 54.0);
    let mut clicked_template = None;

    for pair in TemplateId::ALL.chunks(2) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = tpl_gap;
            for &tpl in pair {
                if render_tile(
                    ui,
                    tpl_size,
                    tpl.icon(),
                    tl!(tpl.name()),
                    tl!(tpl.description()),
                    true,
                    tl!(tpl.description()),
                )
                .clicked()
                {
                    clicked_template = Some(tpl);
                }
            }
        });
        ui.add_space(tpl_gap);
    }

    if let Some(id) = clicked_template {
        editor.open_template(id);
        requests.push(EditorRequest::OpenTemplate(id));
    }

    ui.add_space(10.0);

    // Recent courses section
    ui.label(
        RichText::new(tl!("Recent courses"))
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
            RichText::new(tl!("No recent courses yet"))
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
                ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::click());
            if ui.is_rect_visible(row_rect) {
                if resp.hovered() {
                    ui.painter().rect_filled(
                        row_rect,
                        CornerRadius::same(RADIUS_LG as u8),
                        p.hover,
                    );
                }
                let text_y = row_rect.center().y;
                let thumb = Rect::from_min_size(
                    pos2(row_rect.min.x + 6.0, row_rect.center().y - 18.0),
                    vec2(64.0, 36.0),
                );
                let preview = editor
                    .recent_previews
                    .iter()
                    .find(|(path, _)| *path == path_str)
                    .map(|(_, preview)| preview);
                paint_recent_thumbnail(ui, preview, thumb);
                let text_x = thumb.max.x + 10.0;
                let max_name_w = (row_rect.width() * 0.45 - 80.0).max(80.0);
                let name_galley = ui.painter().layout(
                    name.to_owned(),
                    egui::FontId::proportional(SIZE_BODY),
                    p.text,
                    max_name_w,
                );
                let name_w = name_galley.size().x.min(max_name_w);
                ui.painter().text(
                    pos2(text_x, text_y),
                    Align2::LEFT_CENTER,
                    if name_galley.rows.len() > 1 {
                        format!("{}…", name.chars().take(24).collect::<String>())
                    } else {
                        name.to_owned()
                    },
                    egui::FontId::proportional(SIZE_BODY),
                    p.text,
                );
                if !dir_display.is_empty() {
                    let avail_dir = (row_rect.width() - name_w - 28.0 - 80.0).max(40.0);
                    let max_chars = ((avail_dir / 6.5) as usize).clamp(12, 50);
                    let truncated = if dir_display.chars().count() > max_chars {
                        let keep = max_chars.saturating_sub(1);
                        let tail: String = dir_display
                            .chars()
                            .rev()
                            .take(keep)
                            .collect::<String>()
                            .chars()
                            .rev()
                            .collect();
                        format!("…{tail}")
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
}

/// A recent course's first slide, ready to paint (no media: pictures show as
/// their placeholder).
#[derive(Clone)]
pub struct RecentPreview {
    items: Arc<[coursecraft_render::DrawItem]>,
    background: coursecraft_course::Color,
    theme: coursecraft_course::Theme,
    size: coursecraft_course::Size,
}

impl RecentPreview {
    /// The first slide of `course`, composed once.
    #[must_use]
    pub fn of(course: &Course) -> Option<Self> {
        let slide = course.slides().next()?;
        Some(Self {
            items: coursecraft_render::compose_authoring(course, slide, None).into(),
            background: course.slide_background(slide).clone(),
            theme: course.theme.clone(),
            size: course.slide_size,
        })
    }
}

/// Draw a recent course's thumbnail in `rect`, or a blank card while it loads.
pub fn paint_recent_thumbnail(ui: &Ui, preview: Option<&RecentPreview>, rect: Rect) {
    let p = palette(ui.ctx());
    let painter = ui.painter_at(rect.expand(1.0));
    painter.rect_filled(rect, CornerRadius::same(3), p.raised);
    if let Some(preview) = preview
        && let Some(t) = coursecraft_render::SlideTransform::fit(rect, preview.size)
    {
        coursecraft_render::paint(
            &painter,
            &t,
            &preview.items,
            coursecraft_render::Highlight::default(),
            None,
            Some(&preview.theme),
            Some(&preview.background),
        );
    }
    painter.rect_stroke(
        rect,
        CornerRadius::same(3),
        Stroke::new(1.0, p.border),
        egui::StrokeKind::Inside,
    );
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
