//! Headless learner-player snapshots for reviews and visual validation.

use coursecraft_course::{Course, Issue};
use coursecraft_logic::{Input, Player};
use egui::Context;

use crate::design::{ThemeMode, apply_style};
use crate::media::{MediaCache, MediaSource};
use crate::offscreen::{RenderError, RgbaImage, render_ui};
use crate::player_frame::{PlayerFrame, show_loading_text};
use crate::view::SlideView;

/// Player state to capture for a headless snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlayerSnapshotSelect {
    /// A running course with the control strip visible.
    #[default]
    Playing,
    /// A running course paused by the learner, with the strip still visible.
    Paused,
    /// A running course with the strip auto-hidden by idle time.
    Hidden,
    /// A running course with the outline drawer fully open.
    Drawer,
    /// The ended overlay over the slide.
    Ended,
    /// The loading screen shown while a course is being fetched.
    Loading,
}

fn playable_error(issues: Vec<Issue>) -> RenderError {
    RenderError::NotPlayable(
        issues
            .into_iter()
            .map(|issue| format!("{}: {}", issue.path, issue.message))
            .collect(),
    )
}

fn snapshot_player_state<'a>(
    course: &'a Course,
    slide: Option<&str>,
    paused: bool,
) -> Result<(Player<'a>, String), RenderError> {
    let mut player = Player::new(course).map_err(playable_error)?;
    player.handle(&Input::Start);
    // A draw slide resolves to its first drawn question at seed 0.
    let target_index = match slide {
        Some(id) => player
            .slide_position(id)
            .ok_or_else(|| RenderError::SlideNotFound(id.to_owned()))?,
        None => player.state().slide,
    };
    let target = player
        .play_order()
        .get(target_index)
        .copied()
        .ok_or_else(|| RenderError::SlideNotFound("<start>".to_owned()))?;
    if player.state().slide != target_index {
        player.handle(&Input::GoTo {
            slide: target.id.clone(),
        });
    }

    let mut state = player.state().clone();
    state.started = true;
    for (index, slide) in player.play_order().iter().enumerate() {
        if index <= target_index {
            state.visited.insert(slide.id.clone());
        }
    }
    if target.duration_ms > 0 {
        state.base.ms = target.duration_ms / 2;
    }
    let mut player = Player::resume(course, state).map_err(playable_error)?;
    if paused {
        player.handle(&Input::PlayerPause);
    }
    Ok((player, target.id.clone()))
}

/// Capture a headless player UI snapshot to an in-memory RGBA image.
///
/// The slide is letterboxed on the player ground exactly as the published
/// player draws it. `select` chooses the strip, paused, drawer, ended, or
/// loading state.
///
/// # Errors
/// Returns [`RenderError`] if the course cannot be played, the slide is missing,
/// or rasterisation fails.
pub fn snapshot_player(
    course: Course,
    slide: Option<&str>,
    media: &dyn MediaSource,
    size: (u32, u32),
    ppp: f32,
    select: PlayerSnapshotSelect,
) -> Result<RgbaImage, RenderError> {
    let title = course.title.clone();
    let mut frame = PlayerFrame::new();
    match select {
        PlayerSnapshotSelect::Hidden => frame.hide_strip(),
        PlayerSnapshotSelect::Drawer => frame.open_drawer(),
        PlayerSnapshotSelect::Ended => frame.ended = true,
        PlayerSnapshotSelect::Playing
        | PlayerSnapshotSelect::Paused
        | PlayerSnapshotSelect::Loading => {}
    }
    let mut view = SlideView::new();
    let mut cache = MediaCache::new();

    let (player, _) = if select == PlayerSnapshotSelect::Loading {
        (None, String::new())
    } else {
        let paused = select == PlayerSnapshotSelect::Paused;
        let (player, id) = snapshot_player_state(&course, slide, paused)?;
        (Some(player), id)
    };

    render_ui(
        size.0,
        size.1,
        ppp,
        |ctx| {
            apply_style(ctx, ThemeMode::Dark);
            crate::install_course_fonts(ctx, &course, media);
            let mut style = (*ctx.global_style()).clone();
            style.animation_time = 0.0;
            ctx.set_global_style(style);
        },
        |ctx: &Context| {
            ctx.root_ui(|ui| {
                if let Some(player) = &player {
                    frame.show(ui, player, &mut view, Some((&mut cache, media)));
                } else {
                    show_loading_text(ui, &title, course.chrome_string("loading"));
                }
            });
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::design::PLAYER_GROUND;
    use crate::media::MediaFetch;

    const SHOWCASE_JSON: &str = include_str!("../../../examples/showcase/course.json");

    struct NoMedia;
    impl MediaSource for NoMedia {
        fn get(&self, _src: &str) -> MediaFetch {
            MediaFetch::Missing
        }
    }

    fn pixel(img: &RgbaImage, x: u32, y: u32) -> (u8, u8, u8) {
        let index = ((y as usize) * (img.width as usize) + x as usize) * 4;
        (
            img.pixels[index],
            img.pixels[index + 1],
            img.pixels[index + 2],
        )
    }

    fn close(actual: (u8, u8, u8), expected: (u8, u8, u8)) -> bool {
        (actual.0 as i32 - expected.0 as i32).abs() <= 2
            && (actual.1 as i32 - expected.1 as i32).abs() <= 2
            && (actual.2 as i32 - expected.2 as i32).abs() <= 2
    }

    #[test]
    fn snapshot_player_letterboxes_on_the_ground() {
        let course = Course::from_json(SHOWCASE_JSON).expect("valid course");
        let img = snapshot_player(
            course,
            Some("s-media"),
            &NoMedia,
            (390, 844),
            1.0,
            PlayerSnapshotSelect::Playing,
        )
        .expect("snapshot");
        assert_eq!(img.width, 390);
        assert_eq!(img.height, 844);
        assert!(
            close(pixel(&img, 4, 4), (0x0E, 0x0E, 0x10)),
            "letterbox must use the player ground"
        );
        assert!(
            close(
                pixel(&img, 4, 4),
                (PLAYER_GROUND.r(), PLAYER_GROUND.g(), PLAYER_GROUND.b())
            ),
            "ground constant must match the rendered corner"
        );
    }

    #[test]
    fn snapshot_player_strip_visibility_and_idle_hiding() {
        let course = Course::from_json(SHOWCASE_JSON).expect("valid course");
        let visible = snapshot_player(
            course.clone(),
            Some("s-media"),
            &NoMedia,
            (390, 844),
            1.0,
            PlayerSnapshotSelect::Playing,
        )
        .expect("visible snapshot");
        let hidden = snapshot_player(
            course,
            Some("s-media"),
            &NoMedia,
            (390, 844),
            1.0,
            PlayerSnapshotSelect::Hidden,
        )
        .expect("hidden snapshot");

        let strip_pixel = (195, 844 - 12 - 22);
        assert!(
            !close(
                pixel(&visible, strip_pixel.0, strip_pixel.1),
                (0x0E, 0x0E, 0x10)
            ),
            "the control strip must be painted while active"
        );
        assert!(
            close(
                pixel(&hidden, strip_pixel.0, strip_pixel.1),
                (0x0E, 0x0E, 0x10)
            ),
            "an idle player must hide the strip"
        );
    }

    #[test]
    fn snapshot_player_drawer_loading_and_ended() {
        let course = Course::from_json(SHOWCASE_JSON).expect("valid course");
        let drawer = snapshot_player(
            course.clone(),
            Some("s-media"),
            &NoMedia,
            (390, 844),
            1.0,
            PlayerSnapshotSelect::Drawer,
        )
        .expect("drawer snapshot");
        assert!(
            close(pixel(&drawer, 8, 420), (0x2F, 0x2F, 0x33)),
            "the drawer must paint its raised surface"
        );

        let loading = snapshot_player(
            course.clone(),
            None,
            &NoMedia,
            (390, 844),
            1.0,
            PlayerSnapshotSelect::Loading,
        )
        .expect("loading snapshot");
        assert!(
            close(pixel(&loading, 4, 4), (0x0E, 0x0E, 0x10)),
            "the loading screen uses the player ground"
        );

        let ended = snapshot_player(
            course,
            Some("s-media"),
            &NoMedia,
            (390, 844),
            1.0,
            PlayerSnapshotSelect::Ended,
        )
        .expect("ended snapshot");
        let (r, g, b) = pixel(&ended, 195 - 120, 422);
        assert!(
            r < 120 && g < 120 && b < 120,
            "the ended overlay must dim the slide: got ({r}, {g}, {b})"
        );
    }

    #[test]
    fn snapshot_player_paused_keeps_the_strip() {
        let course = Course::from_json(SHOWCASE_JSON).expect("valid course");
        let img = snapshot_player(
            course,
            Some("s-media"),
            &NoMedia,
            (390, 844),
            1.0,
            PlayerSnapshotSelect::Paused,
        )
        .expect("paused snapshot");
        let strip_pixel = (195, 844 - 12 - 22);
        assert!(
            !close(
                pixel(&img, strip_pixel.0, strip_pixel.1),
                (0x0E, 0x0E, 0x10)
            ),
            "a paused player keeps the control strip visible"
        );
    }

    #[test]
    fn snapshot_player_rejects_missing_slide() {
        let course = Course::from_json(SHOWCASE_JSON).expect("valid course");
        let result = snapshot_player(
            course,
            Some("missing"),
            &NoMedia,
            (390, 844),
            1.0,
            PlayerSnapshotSelect::Playing,
        );
        assert!(matches!(result, Err(RenderError::SlideNotFound(_))));
    }

    #[test]
    fn snapshot_of_a_draw_slide_shows_its_first_drawn_question() {
        fn question_slide(id: &str) -> serde_json::Value {
            serde_json::json!({
                "id": id,
                "title": id,
                "question": {
                    "kind": "multiple_choice",
                    "prompt": "Pick one",
                    "choices": [
                        { "id": "a", "text": "A" },
                        { "id": "b", "text": "B", "correct": true }
                    ],
                    "attempts": 1
                },
                "triggers": [{
                    "action": { "action": "submit_question" },
                    "when": { "event": "submit" }
                }]
            })
        }
        let bank_slides: Vec<serde_json::Value> =
            (1..=5).map(|i| question_slide(&format!("bq{i}"))).collect();
        let text = serde_json::json!({
            "schema_version": 1,
            "id": "draw-snapshot",
            "title": "Draw Snapshot",
            "scenes": [{ "id": "sc", "title": "Scene", "slides": [
                { "id": "s-intro", "title": "Intro" },
                { "id": "s-draw", "title": "Draw", "draw": { "bank": "bank1", "count": 2 } },
                { "id": "s-end", "title": "End" }
            ]}],
            "banks": [{ "id": "bank1", "title": "Bank One", "slides": bank_slides }]
        })
        .to_string();
        let course = Course::from_json(&text).expect("valid course");
        // A draw slide snapshots fine (its first drawn question at seed 0).
        let img = snapshot_player(
            course.clone(),
            Some("s-draw"),
            &NoMedia,
            (390, 844),
            1.0,
            PlayerSnapshotSelect::Playing,
        )
        .expect("draw snapshot");
        assert_eq!(img.width, 390);
        // A bank slide that was not drawn has no position and is rejected.
        let undrawn = (1..=5)
            .map(|i| format!("bq{i}"))
            .find(|id| {
                coursecraft_logic::Player::new(&course)
                    .expect("player")
                    .slide_position(id)
                    .is_none()
            })
            .expect("an undrawn bank slide");
        let result = snapshot_player(
            course,
            Some(&undrawn),
            &NoMedia,
            (390, 844),
            1.0,
            PlayerSnapshotSelect::Playing,
        );
        assert!(matches!(result, Err(RenderError::SlideNotFound(_))));
    }

    #[test]
    fn player_snapshot_select_default_is_playing() {
        assert_eq!(
            PlayerSnapshotSelect::default(),
            PlayerSnapshotSelect::Playing
        );
    }
}
