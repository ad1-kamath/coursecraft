//! Headless editor snapshot generation for reviews and visual validation.

use std::sync::Arc;

use coursecraft_course::Course;
use coursecraft_render::{RenderError, RgbaImage, ThemeMode, apply_style, render_ui};

use crate::{BottomTab, Editor, InspectorTab};

/// Selection options for headless editor snapshot generation.
#[derive(Debug, Clone, Default)]
pub struct SnapshotSelect {
    /// Slide to select by ID.
    pub slide: Option<String>,
    /// Object to select by ID (opens inspector for this object).
    pub object: Option<String>,
    /// Inspector tab to activate.
    pub inspector_tab: Option<InspectorTab>,
    /// Bottom panel tab to activate.
    pub bottom_tab: Option<BottomTab>,
    /// Open the Course Settings dialog.
    pub course_settings: bool,
    /// Open the Theme dialog.
    pub theme_dialog: bool,
    /// Open the Welcome screen.
    pub welcome: bool,
    /// Open the Keyboard Shortcuts dialog.
    pub shortcuts: bool,
    /// Display a specific coach mark step (1, 2, or 3).
    pub coach_mark: Option<u8>,
    /// Open the in-editor learner preview window.
    pub preview: bool,
    /// Show AI Settings on the Agents tab.
    pub connect_agent: bool,
}

/// Capture a headless editor UI snapshot for a course to an in-memory RGBA image.
///
/// Builds a headless [`Editor`] identical to the desktop UI, applies the requested
/// theme, selects the requested slide/object/tab, and rasterises the full window
/// without opening a display or requiring a GPU.
///
/// # Errors
/// Returns [`RenderError`] if rasterisation fails or dimensions are invalid.
pub fn snapshot_editor(
    course: Course,
    media: Vec<(String, Arc<[u8]>)>,
    mode: ThemeMode,
    size: (u32, u32),
    ppp: f32,
    select: SnapshotSelect,
) -> Result<RgbaImage, RenderError> {
    let mut editor = Editor::new(course);
    editor.media = media;

    if let Some(ref slide_id) = select.slide {
        editor.selection.slide = slide_id.clone();
        editor.selection.clear_objects();
    }
    if let Some(ref obj_id) = select.object {
        editor.selection.object = Some(obj_id.clone());
    }
    if let Some(tab) = select.inspector_tab {
        editor.ui.inspector_tab = tab;
    }
    if let Some(tab) = select.bottom_tab {
        editor.ui.bottom_tab = tab;
    }
    if select.course_settings {
        editor.ui.show_course_settings_window = true;
    }
    if select.theme_dialog {
        editor.ui.show_theme_window = true;
    }
    if select.welcome {
        editor.ui.show_welcome = true;
    }
    if select.shortcuts {
        editor.ui.show_shortcuts_dialog = true;
    }
    if let Some(coach) = select.coach_mark {
        editor.set_coach_marks_done(false);
        editor.ui.current_coach_mark = Some(coach);
    }
    editor.selection.repair(&editor.session.course);
    if select.connect_agent {
        editor.automation_available = true;
        editor.automation_enabled = true;
        editor.agent_exe_path =
            Some("/Applications/CourseCraft.app/Contents/MacOS/CourseCraft".to_owned());
        editor.open_agent_settings();
        editor.ui.ai.agents_detected = true;
        for row in &mut editor.ui.ai.agents {
            row.status = coursecraft_ai::agents::AgentStatus::Found;
        }
    }
    if select.preview {
        editor.open_preview();
    }

    render_ui(
        size.0,
        size.1,
        ppp,
        |ctx| {
            apply_style(ctx, mode);
            let mut style = (*ctx.global_style()).clone();
            style.animation_time = 0.0;
            ctx.set_global_style(style);
        },
        |ctx| {
            ctx.root_ui(|ui| {
                let _ = editor.show(ui);
            });
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use coursecraft_render::Palette;

    const HELLO_JSON: &str = include_str!("../../../examples/hello-course/course.json");

    #[test]
    fn test_snapshot_editor_dark_and_light() {
        let course = Course::from_json(HELLO_JSON).expect("valid course");

        // 1. Snapshot in Dark mode
        let dark_img = snapshot_editor(
            course.clone(),
            Vec::new(),
            ThemeMode::Dark,
            (1440, 900),
            1.0,
            SnapshotSelect::default(),
        )
        .expect("dark snapshot");
        assert_eq!(dark_img.width, 1440);
        assert_eq!(dark_img.height, 900);

        // A pixel at top-left inside the menu row equals Palette.panel (±2)
        let dark_pal = Palette::dark();
        let pixel_idx = (14 * 1440 + 2) * 4;
        let r = dark_img.pixels[pixel_idx];
        let g = dark_img.pixels[pixel_idx + 1];
        let b = dark_img.pixels[pixel_idx + 2];
        assert!(
            (r as i32 - dark_pal.panel.r() as i32).abs() <= 2,
            "dark r: {r} vs {}",
            dark_pal.panel.r()
        );
        assert!(
            (g as i32 - dark_pal.panel.g() as i32).abs() <= 2,
            "dark g: {g} vs {}",
            dark_pal.panel.g()
        );
        assert!(
            (b as i32 - dark_pal.panel.b() as i32).abs() <= 2,
            "dark b: {b} vs {}",
            dark_pal.panel.b()
        );

        // 2. Snapshot in Light mode
        let light_img = snapshot_editor(
            course.clone(),
            Vec::new(),
            ThemeMode::Light,
            (1440, 900),
            1.0,
            SnapshotSelect::default(),
        )
        .expect("light snapshot");
        assert_eq!(light_img.width, 1440);
        assert_eq!(light_img.height, 900);

        let light_pal = Palette::light();
        let pixel_idx = (14 * 1440 + 2) * 4;
        let r = light_img.pixels[pixel_idx];
        let g = light_img.pixels[pixel_idx + 1];
        let b = light_img.pixels[pixel_idx + 2];
        assert!(
            (r as i32 - light_pal.panel.r() as i32).abs() <= 2,
            "light r: {r} vs {}",
            light_pal.panel.r()
        );
        assert!(
            (g as i32 - light_pal.panel.g() as i32).abs() <= 2,
            "light g: {g} vs {}",
            light_pal.panel.g()
        );
        assert!(
            (b as i32 - light_pal.panel.b() as i32).abs() <= 2,
            "light b: {b} vs {}",
            light_pal.panel.b()
        );

        // 3. The slide area contains the slide background colour (white #FFFFFF for default theme)
        let has_slide_bg = (200..600).any(|y| {
            (400..900).any(|x| {
                let idx = (y * 1440 + x) * 4;
                dark_img.pixels[idx] == 255
                    && dark_img.pixels[idx + 1] == 255
                    && dark_img.pixels[idx + 2] == 255
            })
        });
        assert!(
            has_slide_bg,
            "slide area must contain slide background colour"
        );

        // 4. Snapshot with object title
        let select_obj = SnapshotSelect {
            slide: Some("s-intro".to_string()),
            object: Some("title".to_string()),
            ..Default::default()
        };
        let obj_img = snapshot_editor(
            course,
            Vec::new(),
            ThemeMode::Dark,
            (1440, 900),
            1.0,
            select_obj,
        )
        .expect("object snapshot");
        assert_eq!(obj_img.width, 1440);
        assert_eq!(obj_img.height, 900);
    }

    #[test]
    fn test_popover_and_shortcuts_are_fully_opaque() {
        let course = Course::from_json(HELLO_JSON).expect("valid course");
        let dark_pal = Palette::dark();

        // Shortcuts dialog snapshot
        let shortcuts_img = snapshot_editor(
            course.clone(),
            Vec::new(),
            ThemeMode::Dark,
            (1440, 900),
            1.0,
            SnapshotSelect {
                shortcuts: true,
                ..Default::default()
            },
        )
        .expect("shortcuts snapshot");

        // The shortcuts dialog card is centered at 720, 450.
        // A point inside the dialog background (e.g. 720, 240) should have dark_pal.raised color.
        let idx = (240 * 1440 + 720) * 4;
        let r = shortcuts_img.pixels[idx];
        let g = shortcuts_img.pixels[idx + 1];
        let b = shortcuts_img.pixels[idx + 2];
        assert!(
            (r as i32 - dark_pal.raised.r() as i32).abs() <= 2,
            "shortcuts dialog background must be opaque raised fill: got ({r}, {g}, {b}) vs {:?}",
            dark_pal.raised
        );

        // Coach mark 1 snapshot
        let coach_img = snapshot_editor(
            course,
            Vec::new(),
            ThemeMode::Dark,
            (1440, 900),
            1.0,
            SnapshotSelect {
                coach_mark: Some(1),
                ..Default::default()
            },
        )
        .expect("coach snapshot");

        // Coach mark 1 is at pos (236, 120), so inside it at (280, 160) should be dark_pal.raised.
        let idx = (160 * 1440 + 280) * 4;
        let r = coach_img.pixels[idx];
        let g = coach_img.pixels[idx + 1];
        let b = coach_img.pixels[idx + 2];
        assert!(
            (r as i32 - dark_pal.raised.r() as i32).abs() <= 2,
            "coach mark background must be opaque raised fill: got ({r}, {g}, {b}) vs {:?}",
            dark_pal.raised
        );
    }
}
