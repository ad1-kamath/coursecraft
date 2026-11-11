//! In-editor course preview window.
//!
//! Plays a clone of the current course starting at the selected slide.
//!
//! # Safety and Memory Architecture
//! To avoid leaking memory per preview without relying on `unsafe`, `Box::leak`,
//! or self-referential lifetimes, [`Preview`] holds an owned [`Course`] and
//! [`Option<PlayerState>`]. During each UI frame, a stack-local [`Player`] is
//! re-created using [`Player::resume(&self.course, state)`]. This keeps all
//! references bounded to the local frame stack, avoids lifetime pollution, and
//! costs negligible validation overhead for courses of standard authoring size.

use crate::EditorRequest;
use coursecraft_course::{Course, Issue, Severity};
use coursecraft_logic::{Effect, HostInfo, Input, Player, PlayerState};
use coursecraft_playback::MediaBackend;
use coursecraft_render::{
    FrameOutput, MediaDriver, MediaFetch, MediaSource, PlayerFrame, SlideView, Ticker, encode_png,
    render_player_rgba,
};
use egui::{Align, Id, Layout, Rect, ScrollArea, Ui};

/// A draw seed from the system clock, so Restart can draw different questions.
/// This crate is desktop-only (L6), so the system clock does not affect wasm.
fn clock_seed() -> u32 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            let secs = elapsed.as_secs() % (u64::from(u32::MAX).saturating_add(1));
            u32::try_from(secs).unwrap_or(0)
        })
}

/// The host info preview supplies: no learner name, today's date.
fn preview_host_info() -> HostInfo {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX)
        });
    HostInfo {
        learner_name: None,
        date: Some(coursecraft_logic::civil_date(secs)),
    }
}

/// The preview has no media bytes at effect-handling time; images render only
/// when a media source is threaded through, so printed slides use what is known.
struct NoMedia;

impl MediaSource for NoMedia {
    fn get(&self, _src: &str) -> MediaFetch {
        MediaFetch::Missing
    }
}

/// The save-dialog name for a printed slide.
fn print_file_name(player: &Player<'_>) -> String {
    let title = player
        .current_slide()
        .map(|slide| slide.title.trim())
        .unwrap_or_default();
    let cleaned: String = title
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => ch,
        })
        .collect();
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        "certificate.png".to_owned()
    } else {
        format!("{cleaned}.png")
    }
}

/// Queue a PNG of the current slide for `Effect::Print`.
fn queue_prints(
    effects: &[Effect],
    player: &Player<'_>,
    media: &dyn MediaSource,
    requests: &mut Vec<EditorRequest>,
) {
    for effect in effects {
        let Effect::Print { exclude, .. } = effect else {
            continue;
        };
        match render_player_rgba(player, 1600, exclude, media, None)
            .and_then(|image| encode_png(&image))
        {
            Ok(png) => requests.push(EditorRequest::SaveImage {
                png,
                suggested_name: print_file_name(player),
            }),
            Err(error) => eprintln!("preview print render failed: {error}"),
        }
    }
}

/// Gap between the slide area and the effect log.
const LOG_GAP: f32 = 10.0;

/// Maximum number of log lines retained before dropping the oldest entries.
pub const MAX_PREVIEW_LOG_ENTRIES: usize = 1000;

/// State for the in-editor course preview window.
pub struct Preview {
    /// Owned copy of the course being previewed.
    pub course: Course,
    /// The course's own start slide, which Restart goes back to.
    course_start: Option<String>,
    /// Player state machine, re-instantiated into a transient [`Player`] each frame.
    pub state: Option<PlayerState>,
    /// Validation errors encountered when initializing or resuming the course.
    pub errors: Vec<Issue>,
    /// Compositor and pointer input handler.
    pub view: SlideView,
    /// Learner-facing player frame shared with the published player.
    pub frame: PlayerFrame,
    /// Real-time frame clock accumulator.
    pub ticker: Ticker,
    /// Drives the playback backend from runtime media effects.
    pub media: MediaDriver,
    /// Emitted effects formatted as JSON strings.
    pub log: Vec<String>,
    /// Requests queued from the preview, such as saving a printed slide.
    pub requests: Vec<EditorRequest>,
    /// Whether the collapsible effect log panel is visible.
    pub show_log: bool,
    /// Whether the course has ended.
    pub ended: bool,
    /// Whether the preview window currently owns keyboard interaction: it was the
    /// last thing clicked, or the pointer is over it.
    pub has_focus: bool,
    /// Outer rect of the preview window in the last frame (tracked for tests).
    pub window_rect: Option<Rect>,
}

impl Drop for Preview {
    fn drop(&mut self) {
        self.media.stop_all();
    }
}

/// Next focus state of the preview window: a press decides (inside gives focus,
/// outside takes it away); without a press, hovering the window grants focus and
/// otherwise the previous state is kept.
#[must_use]
pub fn next_preview_focus(prev: bool, pressed: bool, pointer_inside: bool) -> bool {
    if pressed {
        pointer_inside
    } else {
        prev || pointer_inside
    }
}

fn record_effects(
    effects: &[Effect],
    log: &mut Vec<String>,
    ended: &mut bool,
    ctx: Option<&egui::Context>,
) {
    for effect in effects {
        if let Ok(line) = serde_json::to_string(effect) {
            if log.len() >= MAX_PREVIEW_LOG_ENTRIES {
                let overflow = log
                    .len()
                    .saturating_sub(MAX_PREVIEW_LOG_ENTRIES)
                    .saturating_add(1);
                log.drain(0..overflow.min(log.len()));
            }
            log.push(line);
        }
        match effect {
            Effect::OpenUrl { url } => {
                if let Some(ctx) = ctx {
                    ctx.open_url(egui::OpenUrl::new_tab(url));
                }
            }
            Effect::Exit => {
                *ended = true;
            }
            Effect::RunJavaScript { .. } => {
                log.push("JavaScript runs in the published course".to_owned());
            }
            Effect::Print { .. } => {
                log.push("Printing works in the published course or the editor preview".to_owned());
            }
            _ => {}
        }
    }
}

/// Upper bound on follow-up inputs chained from one batch of effects.
const MAX_MEDIA_FOLLOWUPS: usize = 1000;

/// Apply media effects and feed the follow-up inputs (missing media) back to
/// the player until no more arise.
fn dispatch_media_effects(
    media: &mut MediaDriver,
    player: &mut Player<'_>,
    effects: &[Effect],
    media_source: Option<&dyn MediaSource>,
    log: &mut Vec<String>,
    ended: &mut bool,
    ctx: Option<&egui::Context>,
) {
    let mut queue: std::collections::VecDeque<Input> =
        media.apply(player, effects, media_source).into();
    feed_inputs(media, player, &mut queue, media_source, log, ended, ctx);
}

fn feed_inputs(
    media: &mut MediaDriver,
    player: &mut Player<'_>,
    queue: &mut std::collections::VecDeque<Input>,
    media_source: Option<&dyn MediaSource>,
    log: &mut Vec<String>,
    ended: &mut bool,
    ctx: Option<&egui::Context>,
) {
    let mut budget = MAX_MEDIA_FOLLOWUPS;
    while let Some(input) = queue.pop_front() {
        if budget == 0 {
            break;
        }
        budget -= 1;
        let effects = player.handle(&input);
        record_effects(&effects, log, ended, ctx);
        queue.extend(media.apply(player, &effects, media_source));
    }
}

fn poll_media_events(
    media: &mut MediaDriver,
    player: &mut Player<'_>,
    media_source: Option<&dyn MediaSource>,
    log: &mut Vec<String>,
    ended: &mut bool,
    ctx: Option<&egui::Context>,
) {
    let mut queue: std::collections::VecDeque<Input> = media.poll(media_source).into();
    feed_inputs(media, player, &mut queue, media_source, log, ended, ctx);
}

impl Preview {
    /// Create a new preview for the given course, optionally overriding the start slide.
    #[must_use]
    pub fn new(course: &Course, start_slide: Option<&str>) -> Self {
        let course_start = course.start_slide.clone();
        let mut course_clone = course.clone();
        if let Some(slide) = start_slide
            && !slide.is_empty()
        {
            course_clone.start_slide = Some(slide.to_owned());
        }

        let errors: Vec<Issue> = course_clone
            .validate()
            .into_iter()
            .filter(|i| i.severity == Severity::Error)
            .collect();

        if !errors.is_empty() {
            return Self {
                course: course_clone,
                course_start: course_start.clone(),
                state: None,
                errors,
                view: SlideView::new(),
                frame: PlayerFrame::new(),
                ticker: Ticker::new(),
                media: MediaDriver::new(),
                log: Vec::new(),
                requests: Vec::new(),
                show_log: false,
                ended: false,
                has_focus: false,
                window_rect: None,
            };
        }

        match Player::with_draw_seed(&course_clone, clock_seed()) {
            Ok(mut player) => {
                player.set_host_info(preview_host_info());
                let effects = player.handle(&Input::Start);
                let mut log = Vec::new();
                let mut ended = false;
                let mut requests = Vec::new();
                record_effects(&effects, &mut log, &mut ended, None);
                queue_prints(&effects, &player, &NoMedia, &mut requests);
                let mut media = MediaDriver::new();
                let mut queue: std::collections::VecDeque<Input> =
                    media.apply(&player, &effects, None).into();
                feed_inputs(
                    &mut media,
                    &mut player,
                    &mut queue,
                    None,
                    &mut log,
                    &mut ended,
                    None,
                );
                let state = player.state().clone();
                Self {
                    course: course_clone,
                    course_start: course_start.clone(),
                    state: Some(state),
                    errors: Vec::new(),
                    view: SlideView::new(),
                    frame: PlayerFrame::new(),
                    ticker: Ticker::new(),
                    media,
                    log,
                    requests,
                    show_log: false,
                    ended,
                    has_focus: false,
                    window_rect: None,
                }
            }
            Err(issues) => Self {
                course: course_clone,
                course_start: course_start.clone(),
                state: None,
                errors: issues,
                view: SlideView::new(),
                frame: PlayerFrame::new(),
                ticker: Ticker::new(),
                media: MediaDriver::new(),
                log: Vec::new(),
                requests: Vec::new(),
                show_log: false,
                ended: false,
                has_focus: false,
                window_rect: None,
            },
        }
    }

    /// Replace the media playback backend (useful for injecting [`NullBackend`] in tests).
    #[must_use]
    pub fn with_backend(mut self, backend: Box<dyn MediaBackend>) -> Self {
        self.media.set_backend(backend);
        self
    }

    /// Whether the preview initialized a valid player runtime.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty() && self.state.is_some()
    }

    /// Restart playback from the start slide.
    pub fn restart(&mut self) {
        self.media.stop_all();
        if !self.errors.is_empty() {
            return;
        }
        // A preview can open on the selected slide, but restarting means the whole course.
        self.course.start_slide.clone_from(&self.course_start);
        if let Ok(mut player) = Player::with_draw_seed(&self.course, clock_seed()) {
            self.log.clear();
            self.requests.clear();
            player.set_host_info(preview_host_info());
            let effects = player.handle(&Input::Start);
            self.ended = false;
            record_effects(&effects, &mut self.log, &mut self.ended, None);
            queue_prints(&effects, &player, &NoMedia, &mut self.requests);
            let mut queue: std::collections::VecDeque<Input> =
                self.media.apply(&player, &effects, None).into();
            feed_inputs(
                &mut self.media,
                &mut player,
                &mut queue,
                None,
                &mut self.log,
                &mut self.ended,
                None,
            );
            self.state = Some(player.state().clone());
            self.ticker = Ticker::new();
            self.frame.restart();
            self.view = SlideView::new();
        }
    }

    /// Feed a runtime input to the player, updating state and returning emitted effects.
    pub fn handle(&mut self, input: &Input, ctx: Option<&egui::Context>) -> Vec<Effect> {
        let Some(state) = self.state.clone() else {
            return Vec::new();
        };
        let Ok(mut player) = Player::resume(&self.course, state) else {
            return Vec::new();
        };
        let effects = player.handle(input);
        record_effects(&effects, &mut self.log, &mut self.ended, ctx);
        queue_prints(&effects, &player, &NoMedia, &mut self.requests);
        dispatch_media_effects(
            &mut self.media,
            &mut player,
            &effects,
            None,
            &mut self.log,
            &mut self.ended,
            ctx,
        );
        self.state = Some(player.state().clone());
        effects
    }

    /// Update backend video layout positions for the current slide's videos.
    pub fn layout_videos(&mut self) {
        let Some(state) = self.state.clone() else {
            return;
        };
        if let Ok(player) = Player::resume(&self.course, state) {
            self.media.layout_videos(&player, &self.view);
        }
    }

    /// Render validation error messages when the course cannot be previewed.
    fn show_errors(&self, ui: &mut Ui) {
        let p = coursecraft_render::palette(ui.ctx());
        ui.heading("Course Validation Errors");
        ui.label("This course cannot be previewed because it has validation errors:");
        ui.add_space(8.0);
        ScrollArea::vertical().show(ui, |ui| {
            for err in &self.errors {
                ui.colored_label(p.danger, format!("{}: {}", err.path, err.message));
            }
        });
    }

    /// Display the preview window and advance real-time simulation.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        open: &mut bool,
        mut media: Option<(
            &mut coursecraft_render::MediaCache,
            &dyn coursecraft_render::MediaSource,
        )>,
    ) {
        if !*open {
            self.media.stop_all();
            return;
        }

        ctx.request_repaint();

        let mut should_show_errors = false;
        let mut close_requested = false;
        let current_state = self.state.clone();
        let mut frame_effects = Vec::new();
        let drawer_was_open = self.frame.drawer_open();
        let max_width = (ctx.content_rect().width() - 40.0).clamp(320.0, 960.0);
        let max_height = (ctx.content_rect().height() - 40.0).clamp(240.0, 640.0);

        let modal = egui::Modal::new(Id::unique("preview_modal")).show(ctx, |ui| {
            ui.set_max_width(max_width);
            ui.set_max_height(max_height);
            if !self.errors.is_empty() {
                self.show_errors(ui);
                return;
            }

            let Some(state) = current_state else {
                self.show_errors(ui);
                return;
            };

            let course = &self.course;
            let view = &mut self.view;
            let frame = &mut self.frame;
            let ticker = &mut self.ticker;
            let media_driver = &mut self.media;
            let log = &mut self.log;
            let show_log = &mut self.show_log;
            let ended = &mut self.ended;

            let mut player = match Player::resume(course, state) {
                Ok(p) => p,
                Err(issues) => {
                    self.errors = issues;
                    should_show_errors = true;
                    return;
                }
            };

            // Advance clock ticker
            let dt = ctx.input(|i| i.stable_dt);
            if let Some(tick) = ticker.advance(dt) {
                let effects = player.handle(&tick);
                record_effects(&effects, log, ended, Some(ctx));
                frame_effects.extend(effects);
            }

            // Keyboard shortcut: F2 toggles effect log
            if ctx.input(|i| i.key_pressed(egui::Key::F2)) {
                *show_log = !*show_log;
            }

            // Top controls bar
            let mut should_restart = false;
            ui.horizontal(|ui| {
                if crate::widgets::icon_button(
                    ui,
                    coursecraft_render::Icon::ArrowClockwise,
                    "Restart preview from the first slide",
                )
                .clicked()
                {
                    should_restart = true;
                }
                if crate::widgets::icon_toggle(
                    ui,
                    coursecraft_render::Icon::Notepad,
                    *show_log,
                    "Effect log (F2)",
                )
                .clicked()
                {
                    *show_log = !*show_log;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if crate::widgets::icon_button(ui, coursecraft_render::Icon::X, "Close preview")
                        .clicked()
                    {
                        close_requested = true;
                    }
                });
            });
            ui.separator();

            if should_restart {
                drop(player);
                self.restart();
                return;
            }

            let is_log_visible = *show_log;
            let avail = ui.available_size();
            let (slide_width, log_width) = if is_log_visible {
                let lw = (avail.x * 0.35).clamp(240.0, 400.0);
                (avail.x - lw - LOG_GAP, Some(lw))
            } else {
                (avail.x, None)
            };

            // The content must take exactly `avail`: any extra pixel (item spacing between
            // the slide and the log, the separator, row padding) makes the window
            // auto-grow a little on every frame. Zero the spacing and lay the gaps out
            // explicitly so the sums are exact.
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                // Left area: the shared player frame (slide, strip, rule, drawer)
                ui.allocate_ui_with_layout(
                    egui::vec2(slide_width, avail.y),
                    Layout::top_down(Align::Min),
                    |ui| {
                        ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                        frame.ended = *ended;
                        let caption = media_driver.caption_frame(
                            &player,
                            media.as_ref().map(|(_, source)| *source),
                            view,
                        );
                        media_driver.present_captions(caption.as_ref(), frame.captions_on);
                        frame.set_caption_frame(if frame.captions_on { caption } else { None });
                        let combined_holder;
                        let media_arg = match media.as_mut() {
                            Some((c, s)) => {
                                combined_holder = media_driver.as_media_source(*s);
                                Some((&mut **c, &combined_holder as &dyn MediaSource))
                            }
                            None => None,
                        };
                        let out: FrameOutput = frame.show(ui, &player, view, media_arg);
                        if let Some(volume) = out.volume {
                            media_driver.set_volume(volume);
                        }
                        for input in out.inputs {
                            let effects = player.handle(&input);
                            record_effects(&effects, log, ended, Some(ctx));
                            frame_effects.extend(effects);
                        }
                        if let Some((cache, _)) = media.as_mut() {
                            for input in cache.lottie_ended(&player) {
                                let effects = player.handle(&input);
                                record_effects(&effects, log, ended, Some(ctx));
                                frame_effects.extend(effects);
                            }
                        }
                    },
                );

                // Right area: Effect log
                if let Some(lw) = log_width {
                    let gap = ui.allocate_space(egui::vec2(LOG_GAP, avail.y)).1;
                    ui.painter().vline(
                        gap.center().x,
                        gap.y_range(),
                        ui.visuals().widgets.noninteractive.bg_stroke,
                    );
                    ui.allocate_ui_with_layout(
                        egui::vec2(lw, avail.y),
                        Layout::top_down(Align::Min),
                        |ui| {
                            ui.label(egui::RichText::new("Effect Log").strong());
                            ScrollArea::vertical()
                                .auto_shrink([false, false])
                                .stick_to_bottom(true)
                                .show(ui, |ui| {
                                    for line in &*log {
                                        ui.monospace(line);
                                    }
                                });
                        },
                    );
                }
            });

            self.state = Some(player.state().clone());
        });

        let rect = modal.response.rect;
        let (pressed, pos) = ctx.input(|i| (i.pointer.any_pressed(), i.pointer.interact_pos()));
        let inside = pos.is_some_and(|p| rect.contains(p));
        self.has_focus = next_preview_focus(self.has_focus, pressed, inside);
        self.window_rect = Some(rect);

        // Escape closes the preview unless the player used it first (the outline
        // drawer takes the first Escape).
        let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        if close_requested || (escape && !drawer_was_open) {
            *open = false;
        }

        if !*open {
            self.media.stop_all();
        } else if let Some(state) = self.state.clone()
            && let Ok(mut player) = Player::resume(&self.course, state)
        {
            let media_source = media.as_ref().map(|(_, s)| *s);
            match media_source {
                Some(source) => {
                    queue_prints(&frame_effects, &player, source, &mut self.requests);
                }
                None => {
                    queue_prints(&frame_effects, &player, &NoMedia, &mut self.requests);
                }
            }
            dispatch_media_effects(
                &mut self.media,
                &mut player,
                &frame_effects,
                media_source,
                &mut self.log,
                &mut self.ended,
                Some(ctx),
            );
            poll_media_events(
                &mut self.media,
                &mut player,
                media_source,
                &mut self.log,
                &mut self.ended,
                Some(ctx),
            );
            let dt = ctx.input(|i| i.stable_dt);
            let dt_ms = if dt.is_finite() && dt > 0.0 {
                (f64::from(dt) * 1000.0).round().clamp(0.0, u64::MAX as f64) as u64
            } else {
                0
            };
            let mut queue: std::collections::VecDeque<Input> =
                self.media.media_time_inputs(&player, dt_ms).into();
            feed_inputs(
                &mut self.media,
                &mut player,
                &mut queue,
                media_source,
                &mut self.log,
                &mut self.ended,
                Some(ctx),
            );
            self.media.layout_videos(&player, &self.view);
            self.state = Some(player.state().clone());
        }

        if should_show_errors {
            self.state = None;
        }
    }
}
