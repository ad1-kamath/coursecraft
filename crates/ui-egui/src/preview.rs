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
use coursecraft_course::{Course, Issue, Object, ObjectKind, Severity, Value};
use coursecraft_logic::{Effect, HostInfo, Input, PlayPlan, Player, PlayerState};
use coursecraft_playback::MediaBackend;
use coursecraft_render::{
    FrameOutput, MediaDriver, MediaFetch, MediaSource, PlayerFrame, SlideView, Ticker, encode_png,
    render_player_rgba,
};
use egui::{Align, Id, Layout, Rect, ScrollArea, Ui};
use std::collections::{BTreeMap, HashMap, HashSet};

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

/// Resume a [`Player`] using the cached [`PlayPlan`] when available.
fn resume_with_plan<'c>(
    course: &'c Course,
    state: PlayerState,
    plan: Option<&PlayPlan>,
) -> Result<Player<'c>, Vec<Issue>> {
    match plan {
        Some(plan) => Player::resume_trusted(course, state, plan),
        None => Player::resume(course, state),
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

/// Backdrop color behind the desktop preview modal (~80% black in both light and dark themes).
pub const PREVIEW_BACKDROP_COLOR: egui::Color32 = egui::Color32::from_black_alpha(204);

/// Margin kept between the non-fullscreen preview window and the app window edges.
pub const PREVIEW_MARGIN: f32 = 20.0;

/// Minimum width of the non-fullscreen preview window.
pub const PREVIEW_MIN_WIDTH: f32 = 480.0;

/// Minimum height of the non-fullscreen preview window.
pub const PREVIEW_MIN_HEIGHT: f32 = 320.0;

/// Default initial width of the non-fullscreen preview window.
const PREVIEW_DEFAULT_WIDTH: f32 = 960.0;

/// Default initial height of the non-fullscreen preview window.
const PREVIEW_DEFAULT_HEIGHT: f32 = 640.0;

/// Inner padding of the preview window frame.
const PREVIEW_FRAME_PAD: i8 = 10;

/// Outer reach of edge/corner resize grips past the window border.
const RESIZE_GRIP_OUT: f32 = 4.0;

/// Inner reach of edge resize grips inside the window border.
const RESIZE_GRIP_IN: f32 = 4.0;

/// Corner grip size along each axis inside the window border.
const RESIZE_CORNER_IN: f32 = 8.0;

/// Maximum characters shown inline for a text variable in the live variables panel.
const MAX_TEXT_PREVIEW_CHARS: usize = 24;

/// Duration (in seconds) that a changed variable stays highlighted before fading out.
const VAR_HIGHLIGHT_SECS: f64 = 1.0;

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
    /// Last egui frame timestamp (`InputState::time`), used to measure real
    /// elapsed wall time across delayed wakes.
    last_frame_time: Option<f64>,
    /// Drives the playback backend from runtime media effects.
    pub media: MediaDriver,
    /// Emitted effects formatted as JSON strings.
    pub log: Vec<String>,
    /// Requests queued from the preview, such as saving a printed slide.
    pub requests: Vec<EditorRequest>,
    /// Whether the collapsible effect log panel is visible.
    pub show_log: bool,
    /// Whether the collapsible live variables panel is visible.
    pub show_variables: bool,
    /// Variable names referenced via `%var%` in course text objects.
    referenced_vars: HashSet<String>,
    /// Last observed variable values, used to detect changes for the highlight fade.
    last_var_values: BTreeMap<String, Value>,
    /// Timestamp (`egui::InputState::time`) when each variable last changed.
    var_changed_at: HashMap<String, f64>,
    /// Whether the course has ended.
    pub ended: bool,
    /// Whether the preview window currently owns keyboard interaction: it was the
    /// last thing clicked, or the pointer is over it.
    pub has_focus: bool,
    /// Whether a text input inside the preview had focus at the end of the previous frame.
    had_text_focus: bool,
    /// Outer rect of the preview window in the last frame (tracked for tests).
    pub window_rect: Option<Rect>,
    /// Last non-fullscreen preview window rect (position + size).
    non_fullscreen_rect: Option<Rect>,
    /// Remembered non-fullscreen size for the current editor session.
    remembered_size: Option<egui::Vec2>,
    /// Whether the preview window is currently filling the app window.
    pub fullscreen: bool,
    /// Cached play order for [`Player::resume_trusted`], updated on open/restart/retry.
    play_plan: Option<PlayPlan>,
    /// Optional subset of slide IDs when running "Preview this slide" or "Preview selected slides".
    preview_subset: Option<Vec<String>>,
    /// Whether the "This is the end of the preview" popup is currently visible.
    pub end_of_preview_popup: bool,
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

fn collect_percent_vars(text: &str, out: &mut HashSet<String>) {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes.get(i) == Some(&b'%') {
            if bytes.get(i + 1) == Some(&b'%') {
                i += 2;
                continue;
            }
            let mut j = i + 1;
            while j < bytes.len() && bytes.get(j) != Some(&b'%') {
                j += 1;
            }
            if j < bytes.len() {
                let name_len = j - (i + 1);
                if (1..=128).contains(&name_len)
                    && let Some(slice) = bytes.get(i + 1..j)
                    && let Ok(name) = std::str::from_utf8(slice)
                {
                    out.insert(name.to_owned());
                    i = j + 1;
                    continue;
                }
            }
            i += 1;
        } else {
            i += 1;
        }
    }
}

fn collect_object_percent_vars(obj: &Object, out: &mut HashSet<String>) {
    match &obj.kind {
        ObjectKind::Text { text, .. } => collect_percent_vars(text, out),
        ObjectKind::Shape {
            text: Some(shape_text),
            ..
        } => collect_percent_vars(&shape_text.text, out),
        ObjectKind::Button { label, .. } => collect_percent_vars(label, out),
        ObjectKind::Checkbox { label, .. } => collect_percent_vars(label, out),
        ObjectKind::RadioGroup { options, .. } => {
            for opt in options {
                collect_percent_vars(&opt.label, out);
            }
        }
        ObjectKind::Marker { title, body, .. } => {
            collect_percent_vars(title, out);
            collect_percent_vars(body, out);
        }
        ObjectKind::Table { rows, .. } => {
            for row in rows {
                for cell in &row.cells {
                    collect_percent_vars(&cell.text, out);
                }
            }
        }
        _ => {}
    }
}

fn referenced_variables_in_course_subset(
    course: &Course,
    subset: Option<&[String]>,
) -> HashSet<String> {
    let mut out = HashSet::new();
    for slide in course.slides() {
        if let Some(ids) = subset
            && !ids.iter().any(|id| id == &slide.id)
        {
            continue;
        }
        for obj in &slide.objects {
            collect_object_percent_vars(obj, &mut out);
        }
        for layer in &slide.layers {
            for obj in &layer.objects {
                collect_object_percent_vars(obj, &mut out);
            }
        }
    }
    out
}

fn format_live_value(val: &Value) -> (String, Option<String>) {
    match val {
        Value::Bool(b) => (if *b { "true" } else { "false" }.to_owned(), None),
        Value::Number(n) => {
            if !n.is_finite() || *n == 0.0 {
                return ("0".to_owned(), None);
            }
            let formatted = format!("{n:.2}");
            let trimmed = if formatted.contains('.') {
                formatted.trim_end_matches('0').trim_end_matches('.')
            } else {
                formatted.as_str()
            };
            (trimmed.to_owned(), None)
        }
        Value::Text(s) => {
            let mut chars = s.chars();
            let prefix: String = chars.by_ref().take(MAX_TEXT_PREVIEW_CHARS).collect();
            if chars.next().is_some() {
                (format!("{prefix}…"), Some(s.clone()))
            } else {
                (s.clone(), None)
            }
        }
    }
}

fn show_variables_panel(
    ui: &mut Ui,
    course: &Course,
    player: &Player,
    referenced_vars: &HashSet<String>,
    var_changed_at: &HashMap<String, f64>,
    now: f64,
) {
    let p = coursecraft_render::palette(ui.ctx());
    ui.label(egui::RichText::new("Variables").strong());
    if course.variables.is_empty() {
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new("No variables in this course.")
                .small()
                .color(p.text_secondary),
        );
        return;
    }

    let mut vars: Vec<&coursecraft_course::VariableDef> = course.variables.iter().collect();
    vars.sort_by_key(|v| !referenced_vars.contains(&v.name));

    ScrollArea::vertical()
        .id_salt("preview_variables_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for (idx, var) in vars.iter().enumerate() {
                let live_val = player.variable(&var.name).unwrap_or(&var.value);
                let (display_val, full_tooltip) = format_live_value(live_val);
                let highlight_alpha = var_changed_at
                    .get(&var.name)
                    .and_then(|&t| {
                        let elapsed = now - t;
                        if (0.0..VAR_HIGHLIGHT_SECS).contains(&elapsed) {
                            Some(((VAR_HIGHLIGHT_SECS - elapsed) / VAR_HIGHLIGHT_SECS) as f32)
                        } else {
                            None
                        }
                    })
                    .unwrap_or(0.0);
                let row_fill = if highlight_alpha > 0.0 {
                    p.accent.gamma_multiply(0.25 * highlight_alpha)
                } else if idx % 2 == 1 {
                    p.hover.gamma_multiply(0.4)
                } else {
                    egui::Color32::TRANSPARENT
                };

                egui::Frame::new()
                    .fill(row_fill)
                    .inner_margin(egui::Margin::symmetric(6, 4))
                    .corner_radius(4.0)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            let name_w = (ui.available_width() - 60.0).max(40.0);
                            ui.allocate_ui_with_layout(
                                egui::vec2(name_w, 16.0),
                                Layout::left_to_right(Align::Center),
                                |ui| {
                                    ui.add(
                                        egui::Label::new(egui::RichText::new(&var.name).strong())
                                            .truncate(),
                                    );
                                },
                            );
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(
                                    egui::RichText::new(crate::variables::value_kind_name(
                                        live_val.kind(),
                                    ))
                                    .small()
                                    .color(p.text_secondary),
                                );
                            });
                        });
                        let val_resp = ui.add(
                            egui::Label::new(egui::RichText::new(&display_val).monospace())
                                .truncate(),
                        );
                        if let Some(full) = full_tooltip {
                            val_resp.on_hover_text(full);
                        }
                    });
            }
        });
}

impl Preview {
    /// Create a new preview for the given course, optionally previewing only a single slide.
    #[must_use]
    pub fn new(course: &Course, start_slide: Option<&str>) -> Self {
        match start_slide.filter(|s| !s.is_empty()) {
            Some(id) => Self::new_subset(course, Some(&[id.to_owned()])),
            None => Self::new_subset(course, None),
        }
    }

    /// Create a full-course preview starting at `slide_id` (not a partial subset).
    ///
    /// If `slide_id` is empty or does not refer to a play-order slide (e.g. a bank slide),
    /// starts from the course's normal start slide.
    #[must_use]
    pub fn new_from_slide(course: &Course, slide_id: &str) -> Self {
        let mut course_clone = course.clone();
        if !slide_id.is_empty()
            && course.bank_slide(slide_id).is_none()
            && course.play_slide(slide_id).is_some()
        {
            course_clone.start_slide = Some(slide_id.to_owned());
        }
        Self::new_subset(&course_clone, None)
    }

    /// Create a new preview for the given course, optionally restricted to a subset of slide IDs.
    #[must_use]
    pub fn new_subset(course: &Course, subset: Option<&[String]>) -> Self {
        let course_start = course.start_slide.clone();
        let course_clone = course.clone();
        let subset_owned: Option<Vec<String>> = subset.and_then(|ids| {
            let filtered: Vec<String> = ids.iter().filter(|id| !id.is_empty()).cloned().collect();
            if filtered.is_empty() {
                None
            } else {
                Some(filtered)
            }
        });
        let referenced_vars =
            referenced_variables_in_course_subset(&course_clone, subset_owned.as_deref());

        let errors: Vec<Issue> = course_clone
            .validate()
            .into_iter()
            .filter(|i| i.severity == Severity::Error)
            .collect();

        if !errors.is_empty() {
            return Self {
                course: course_clone,
                course_start,
                state: None,
                errors,
                view: SlideView::new(),
                frame: PlayerFrame::new(),
                ticker: Ticker::new(),
                last_frame_time: None,
                media: MediaDriver::new(),
                log: Vec::new(),
                requests: Vec::new(),
                show_log: false,
                show_variables: false,
                referenced_vars,
                last_var_values: BTreeMap::new(),
                var_changed_at: HashMap::new(),
                ended: false,
                has_focus: true,
                had_text_focus: false,
                window_rect: None,
                non_fullscreen_rect: None,
                remembered_size: None,
                fullscreen: false,
                play_plan: None,
                preview_subset: subset_owned,
                end_of_preview_popup: false,
            };
        }

        let player_res = match subset_owned.as_deref() {
            Some(ids) => Player::with_preview_subset(&course_clone, clock_seed(), ids),
            None => Player::with_draw_seed(&course_clone, clock_seed()),
        };

        match player_res {
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
                let end_of_preview_popup = player.take_preview_boundary();
                let actual_subset = if player.is_partial_preview() {
                    Some(
                        player
                            .play_order()
                            .iter()
                            .map(|s| s.id.clone())
                            .collect::<Vec<_>>(),
                    )
                } else {
                    None
                };
                let plan = player.play_plan();
                let state = player.into_state();
                let last_var_values = state.variables.clone();
                Self {
                    course: course_clone,
                    course_start,
                    state: Some(state),
                    errors: Vec::new(),
                    view: SlideView::new(),
                    frame: PlayerFrame::new(),
                    ticker: Ticker::new(),
                    last_frame_time: None,
                    media,
                    log,
                    requests,
                    show_log: false,
                    show_variables: false,
                    referenced_vars,
                    last_var_values,
                    var_changed_at: HashMap::new(),
                    ended,
                    has_focus: true,
                    had_text_focus: false,
                    window_rect: None,
                    non_fullscreen_rect: None,
                    remembered_size: None,
                    fullscreen: false,
                    play_plan: Some(plan),
                    preview_subset: actual_subset,
                    end_of_preview_popup,
                }
            }
            Err(issues) => Self {
                course: course_clone,
                course_start,
                state: None,
                errors: issues,
                view: SlideView::new(),
                frame: PlayerFrame::new(),
                ticker: Ticker::new(),
                last_frame_time: None,
                media: MediaDriver::new(),
                log: Vec::new(),
                requests: Vec::new(),
                show_log: false,
                show_variables: false,
                referenced_vars,
                last_var_values: BTreeMap::new(),
                var_changed_at: HashMap::new(),
                ended: false,
                has_focus: true,
                had_text_focus: false,
                window_rect: None,
                non_fullscreen_rect: None,
                remembered_size: None,
                fullscreen: false,
                play_plan: None,
                preview_subset: subset_owned,
                end_of_preview_popup: false,
            },
        }
    }

    /// Whether this preview is running a partial subset of slides.
    #[must_use]
    pub fn is_partial_preview(&self) -> bool {
        self.preview_subset.is_some()
    }

    /// Replace the media playback backend (useful for injecting [`NullBackend`] in tests).
    #[must_use]
    pub fn with_backend(mut self, backend: Box<dyn MediaBackend>) -> Self {
        self.media.set_backend(backend);
        self
    }

    /// Seed the initial non-fullscreen size from the editor session.
    pub fn set_initial_size(&mut self, size: Option<egui::Vec2>) {
        self.remembered_size = size;
        self.non_fullscreen_rect = None;
    }

    /// Last non-fullscreen size, to remember across preview openings in this session.
    #[must_use]
    pub fn remembered_size(&self) -> Option<egui::Vec2> {
        self.remembered_size
    }

    /// Whether the preview initialized a valid player runtime.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty() && self.state.is_some()
    }

    /// Restart playback from the first slide (or the first slide of the subset in partial preview).
    pub fn restart(&mut self) {
        self.media.stop_all();
        self.end_of_preview_popup = false;
        if !self.errors.is_empty() {
            return;
        }
        self.course.start_slide.clone_from(&self.course_start);
        let player_res = match self.preview_subset.as_deref() {
            Some(ids) => Player::with_preview_subset(&self.course, clock_seed(), ids),
            None => Player::with_draw_seed(&self.course, clock_seed()),
        };
        if let Ok(mut player) = player_res {
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
            if player.take_preview_boundary() {
                self.end_of_preview_popup = true;
            }
            self.play_plan = Some(player.play_plan());
            let state = player.into_state();
            self.last_var_values = state.variables.clone();
            self.var_changed_at.clear();
            self.state = Some(state);
            self.ticker = Ticker::new();
            self.last_frame_time = None;
            self.frame.restart();
            self.view = SlideView::new();
            self.media.set_rate(1.0);
        }
    }

    /// Feed a runtime input to the player, updating state and returning emitted effects.
    pub fn handle(&mut self, input: &Input, ctx: Option<&egui::Context>) -> Vec<Effect> {
        let Some(state) = self.state.take() else {
            return Vec::new();
        };
        let Ok(mut player) = resume_with_plan(&self.course, state, self.play_plan.as_ref()) else {
            return Vec::new();
        };
        let effects = player.handle(input);
        if matches!(input, Input::SetSpeed { .. }) {
            self.media.set_rate(player.speed());
        }
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
        if player.take_preview_boundary() {
            self.end_of_preview_popup = true;
        }
        self.play_plan = Some(player.play_plan());
        self.state = Some(player.into_state());
        effects
    }

    /// Update backend video layout positions for the current slide's videos.
    pub fn layout_videos(&mut self) {
        let Some(state) = self.state.clone() else {
            return;
        };
        if let Ok(player) = resume_with_plan(&self.course, state, self.play_plan.as_ref()) {
            self.media.layout_videos(&player, &self.view);
        }
    }

    /// Render validation error messages when the course cannot be previewed.
    fn show_errors(&self, ui: &mut Ui) {
        let p = coursecraft_render::palette(ui.ctx());
        ui.heading("Course validation errors");
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
            self.last_frame_time = None;
            self.ticker.reset();
            return;
        }

        let (now, stable_dt) = ctx.input(|i| (i.time, i.stable_dt));
        let dt = match self.last_frame_time {
            Some(prev) if (now - prev).is_finite() && now > prev => (now - prev) as f32,
            _ => stable_dt,
        };
        self.last_frame_time = Some(now);

        let mut should_show_errors = false;
        let mut close_requested = false;
        let mut boundary_hit = false;
        let mut current_state = self.state.take();
        let mut frame_effects = Vec::new();
        let drawer_was_open = self.frame.drawer_open();
        let text_edit_was_focused = self.had_text_focus || ctx.egui_wants_keyboard_input();
        let screen_rect = ctx.content_rect();
        let bounds = preview_bounds(screen_rect);
        let (min_size, max_size) = preview_size_limits(bounds);

        if self.non_fullscreen_rect.is_none() {
            let initial = self.remembered_size.map_or_else(
                || {
                    egui::vec2(
                        coursecraft_course::safe_clamp(
                            PREVIEW_DEFAULT_WIDTH,
                            min_size.x,
                            max_size.x,
                        ),
                        coursecraft_course::safe_clamp(
                            PREVIEW_DEFAULT_HEIGHT,
                            min_size.y,
                            max_size.y,
                        ),
                    )
                },
                |sz| clamp_preview_size(sz, min_size, max_size),
            );
            let rect = Rect::from_center_size(bounds.center(), initial);
            self.non_fullscreen_rect = Some(rect);
            self.remembered_size = Some(rect.size());
        } else if let Some(rect) = self.non_fullscreen_rect {
            let clamped = clamp_preview_rect(rect, bounds, min_size, max_size);
            self.non_fullscreen_rect = Some(clamped);
            self.remembered_size = Some(clamped.size());
        }

        let toggle_fullscreen_key = self.has_focus
            && !text_edit_was_focused
            && ctx.input(|i| {
                i.key_pressed(egui::Key::F11)
                    || (i.key_pressed(egui::Key::F)
                        && i.modifiers.ctrl
                        && i.modifiers.mac_cmd
                        && !i.modifiers.alt
                        && !i.modifiers.shift)
            });
        if toggle_fullscreen_key {
            self.fullscreen = !self.fullscreen;
        }

        let target_rect = if self.fullscreen {
            screen_rect
        } else {
            self.non_fullscreen_rect.unwrap_or(bounds)
        };

        let popup_was_open = egui::Popup::is_any_open(ctx);
        let modal_area = egui::Area::new(Id::unique("preview_window"))
            .kind(egui::UiKind::Modal)
            .order(egui::Order::Foreground)
            .sense(egui::Sense::hover())
            .fixed_pos(screen_rect.min)
            .interactable(true)
            .show(ctx, |ui| {
                let mut backdrop_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(screen_rect)
                        .sense(egui::Sense::click_and_drag())
                        .id_salt("preview_backdrop"),
                );
                backdrop_ui.set_min_size(screen_rect.size());
                ui.painter().rect_filled(
                    screen_rect,
                    egui::CornerRadius::ZERO,
                    PREVIEW_BACKDROP_COLOR,
                );
                let backdrop_response = backdrop_ui.response();
                if backdrop_response.has_focus() {
                    ui.memory_mut(|m| m.surrender_focus(backdrop_response.id));
                }

                let mut window_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(target_rect)
                        .sense(egui::Sense::click_and_drag())
                        .id_salt("preview_window_frame"),
                );
                let mut frame = if self.fullscreen {
                    egui::Frame::central_panel(window_ui.style())
                        .inner_margin(egui::Margin::same(PREVIEW_FRAME_PAD))
                } else {
                    egui::Frame::window(window_ui.style())
                        .inner_margin(egui::Margin::same(PREVIEW_FRAME_PAD))
                };
                if self.fullscreen {
                    frame = frame.corner_radius(egui::CornerRadius::ZERO);
                }
                let margin_sum = frame.total_margin().sum();
                let inner_size = egui::vec2(
                    (target_rect.width() - margin_sum.x).max(1.0),
                    (target_rect.height() - margin_sum.y).max(1.0),
                );
                let frame_response = frame
                    .show(&mut window_ui, |ui| {
                        ui.set_min_size(inner_size);
                        ui.set_max_size(inner_size);
                        ui.allocate_ui_with_layout(
                            inner_size,
                            Layout::top_down(Align::Min),
                            |ui| {
                                if !self.errors.is_empty() {
                                    ui.horizontal(|ui| {
                                        ui.with_layout(
                                            Layout::right_to_left(Align::Center),
                                            |ui| {
                                                if crate::widgets::icon_button(
                                                    ui,
                                                    coursecraft_render::Icon::X,
                                                    "Close preview",
                                                )
                                                .clicked()
                                                {
                                                    close_requested = true;
                                                }
                                            },
                                        );
                                    });
                                    ui.separator();
                                    self.show_errors(ui);
                                    self.state = current_state.take();
                                    return;
                                }

                                let Some(state) = current_state.take() else {
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
                                let show_variables = &mut self.show_variables;
                                let fullscreen = &mut self.fullscreen;
                                let referenced_vars = &self.referenced_vars;
                                let last_var_values = &mut self.last_var_values;
                                let var_changed_at = &mut self.var_changed_at;
                                let ended = &mut self.ended;
                                let partial_count = self.preview_subset.as_ref().map(Vec::len);

                                let mut player = match resume_with_plan(
                                    course,
                                    state,
                                    self.play_plan.as_ref(),
                                ) {
                                    Ok(p) => p,
                                    Err(issues) => {
                                        self.errors = issues;
                                        should_show_errors = true;
                                        return;
                                    }
                                };

                                // Advance clock ticker
                                if let Some(tick) = ticker.advance(dt) {
                                    let effects = player.handle(&tick);
                                    record_effects(&effects, log, ended, Some(ctx));
                                    frame_effects.extend(effects);
                                    if player.take_preview_boundary() {
                                        boundary_hit = true;
                                    }
                                }

                                // Keyboard shortcut: F2 toggles effect log
                                if ctx.input(|i| i.key_pressed(egui::Key::F2)) {
                                    *show_log = !*show_log;
                                }
                                // Keyboard shortcut: F3 toggles live variables
                                if ctx.input(|i| i.key_pressed(egui::Key::F3)) {
                                    *show_variables = !*show_variables;
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
                                    if crate::widgets::icon_toggle(
                                        ui,
                                        coursecraft_render::Icon::BracketsCurly,
                                        *show_variables,
                                        "Variables (F3)",
                                    )
                                    .clicked()
                                    {
                                        *show_variables = !*show_variables;
                                    }
                                    if let Some(count) = partial_count {
                                        let label = if count == 1 {
                                            "Partial preview (1 slide)".to_owned()
                                        } else {
                                            format!("Partial preview ({count} slides)")
                                        };
                                        ui.add_space(6.0);
                                        ui.label(egui::RichText::new(label).small().color(
                                            coursecraft_render::palette(ui.ctx()).text_secondary,
                                        ));
                                    }
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        if crate::widgets::icon_button(
                                            ui,
                                            coursecraft_render::Icon::X,
                                            "Close preview",
                                        )
                                        .clicked()
                                        {
                                            close_requested = true;
                                        }
                                        let (fs_icon, fs_tip) = if *fullscreen {
                                            (
                                                coursecraft_render::Icon::CornersIn,
                                                "Exit full screen (F11)",
                                            )
                                        } else {
                                            (
                                                coursecraft_render::Icon::CornersOut,
                                                "Full screen (F11)",
                                            )
                                        };
                                        if crate::widgets::icon_toggle(
                                            ui,
                                            fs_icon,
                                            *fullscreen,
                                            fs_tip,
                                        )
                                        .clicked()
                                        {
                                            *fullscreen = !*fullscreen;
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
                                let is_vars_visible = *show_variables;
                                let avail = ui.available_size();
                                let (slide_width, side_width) = if is_log_visible || is_vars_visible
                                {
                                    let max_side = (avail.x - LOG_GAP - 80.0).max(0.0);
                                    let min_side = 240.0_f32.min(max_side);
                                    let sw = coursecraft_course::safe_clamp(
                                        avail.x * 0.35,
                                        min_side,
                                        400.0_f32.min(max_side),
                                    );
                                    ((avail.x - sw - LOG_GAP).max(1.0), Some(sw))
                                } else {
                                    (avail.x.max(1.0), None)
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
                                            frame.can_speed = media_driver.supports_rate();
                                            frame.can_fullscreen = true;
                                            frame.fullscreen = *fullscreen;
                                            let caption = media_driver.caption_frame(
                                                &player,
                                                media.as_ref().map(|(_, source)| *source),
                                                view,
                                            );
                                            media_driver.present_captions(
                                                caption.as_ref(),
                                                frame.captions_on,
                                            );
                                            frame.set_caption_frame(if frame.captions_on {
                                                caption
                                            } else {
                                                None
                                            });
                                            let combined_holder;
                                            let media_arg = match media.as_mut() {
                                                Some((c, s)) => {
                                                    combined_holder =
                                                        media_driver.as_media_source(*s);
                                                    Some((
                                                        &mut **c,
                                                        &combined_holder as &dyn MediaSource,
                                                    ))
                                                }
                                                None => None,
                                            };
                                            let out: FrameOutput =
                                                frame.show(ui, &player, view, media_arg);
                                            if out.fullscreen_toggle {
                                                *fullscreen = !*fullscreen;
                                            }
                                            crate::content_region::mark_content_region(
                                                ctx,
                                                ui.layer_id(),
                                                out.slide_rect,
                                            );
                                            if let Some(volume) = out.volume {
                                                media_driver.set_volume(volume);
                                            }
                                            for input in out.inputs {
                                                let effects = player.handle(&input);
                                                if matches!(input, Input::SetSpeed { .. }) {
                                                    media_driver.set_rate(player.speed());
                                                }
                                                record_effects(&effects, log, ended, Some(ctx));
                                                frame_effects.extend(effects);
                                                if player.take_preview_boundary() {
                                                    boundary_hit = true;
                                                }
                                            }
                                            if let Some((cache, _)) = media.as_mut() {
                                                for input in cache.lottie_ended(&player) {
                                                    let effects = player.handle(&input);
                                                    record_effects(&effects, log, ended, Some(ctx));
                                                    frame_effects.extend(effects);
                                                    if player.take_preview_boundary() {
                                                        boundary_hit = true;
                                                    }
                                                }
                                            }
                                        },
                                    );

                                    let now = ctx.input(|i| i.time);
                                    for var in &course.variables {
                                        let current =
                                            player.variable(&var.name).unwrap_or(&var.value);
                                        match last_var_values.get(&var.name) {
                                            Some(prev) if prev != current => {
                                                last_var_values
                                                    .insert(var.name.clone(), current.clone());
                                                var_changed_at.insert(var.name.clone(), now);
                                            }
                                            None => {
                                                last_var_values
                                                    .insert(var.name.clone(), current.clone());
                                            }
                                            _ => {}
                                        }
                                    }

                                    // Right area: Live variables and/or Effect log
                                    if let Some(sw) = side_width {
                                        let gap = ui.allocate_space(egui::vec2(LOG_GAP, avail.y)).1;
                                        ui.painter().vline(
                                            gap.center().x,
                                            gap.y_range(),
                                            ui.visuals().widgets.noninteractive.bg_stroke,
                                        );
                                        ui.allocate_ui_with_layout(
                                            egui::vec2(sw, avail.y),
                                            Layout::top_down(Align::Min),
                                            |ui| {
                                                if is_vars_visible && is_log_visible {
                                                    let top_h =
                                                        ((avail.y - LOG_GAP) * 0.5).max(80.0);
                                                    let bot_h =
                                                        (avail.y - LOG_GAP - top_h).max(80.0);
                                                    ui.allocate_ui_with_layout(
                                                        egui::vec2(sw, top_h),
                                                        Layout::top_down(Align::Min),
                                                        |ui| {
                                                            show_variables_panel(
                                                                ui,
                                                                course,
                                                                &player,
                                                                referenced_vars,
                                                                var_changed_at,
                                                                now,
                                                            );
                                                        },
                                                    );
                                                    let hgap = ui
                                                        .allocate_space(egui::vec2(sw, LOG_GAP))
                                                        .1;
                                                    ui.painter().hline(
                                                        hgap.x_range(),
                                                        hgap.center().y,
                                                        ui.visuals()
                                                            .widgets
                                                            .noninteractive
                                                            .bg_stroke,
                                                    );
                                                    ui.allocate_ui_with_layout(
                                                        egui::vec2(sw, bot_h),
                                                        Layout::top_down(Align::Min),
                                                        |ui| {
                                                            ui.label(
                                                                egui::RichText::new("Effect log")
                                                                    .strong(),
                                                            );
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
                                                } else if is_vars_visible {
                                                    show_variables_panel(
                                                        ui,
                                                        course,
                                                        &player,
                                                        referenced_vars,
                                                        var_changed_at,
                                                        now,
                                                    );
                                                } else {
                                                    ui.label(
                                                        egui::RichText::new("Effect log").strong(),
                                                    );
                                                    ScrollArea::vertical()
                                                        .auto_shrink([false, false])
                                                        .stick_to_bottom(true)
                                                        .show(ui, |ui| {
                                                            for line in &*log {
                                                                ui.monospace(line);
                                                            }
                                                        });
                                                }
                                            },
                                        );
                                    }
                                });

                                self.state = Some(player.into_state());
                            },
                        );
                    })
                    .response;
                if window_ui.response().has_focus() {
                    ui.memory_mut(|m| m.surrender_focus(window_ui.response().id));
                }

                let resized_rect = if !self.fullscreen {
                    self.non_fullscreen_rect.map(|prev_rect| {
                        handle_resize_grips(ui, prev_rect, bounds, min_size, max_size)
                    })
                } else {
                    None
                };

                (backdrop_response, frame_response.rect, resized_rect)
            });

        ctx.memory_mut(|mem| mem.set_modal_layer(modal_area.response.layer_id));
        let (backdrop_response, rect, resized_rect) = modal_area.inner;
        if boundary_hit {
            self.end_of_preview_popup = true;
        }
        let mut popup_replay = false;
        let mut popup_close_preview = false;
        if self.end_of_preview_popup {
            let popup_area = egui::Area::new(Id::unique("preview_end_popup"))
                .order(egui::Order::Foreground)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .constrain_to(target_rect)
                .interactable(true)
                .show(ctx, |ui| {
                    let p = coursecraft_render::palette(ui.ctx());
                    egui::Frame::window(ui.style())
                        .inner_margin(egui::Margin::same(16))
                        .show(ui, |ui| {
                            ui.set_min_width(300.0);
                            ui.set_max_width(360.0);
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("This is the end of the preview").strong(),
                                );
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if crate::widgets::icon_button(
                                        ui,
                                        coursecraft_render::Icon::X,
                                        "Close popup",
                                    )
                                    .clicked()
                                    {
                                        self.end_of_preview_popup = false;
                                    }
                                });
                            });
                            ui.add_space(4.0);
                            ui.label(
                                egui::RichText::new(
                                    "Partial preview — only the selected slide(s) are included.",
                                )
                                .small()
                                .color(p.text_secondary),
                            );
                            ui.add_space(12.0);
                            ui.horizontal(|ui| {
                                if ui.button("Replay slide(s)").clicked() {
                                    popup_replay = true;
                                }
                                if ui.button("Close preview").clicked() {
                                    popup_close_preview = true;
                                }
                            });
                        });
                });
            ctx.move_to_top(popup_area.response.layer_id);
        }
        if popup_replay {
            self.restart();
        } else if popup_close_preview {
            close_requested = true;
        }
        if let (Some(prev_rect), Some(resized)) = (self.non_fullscreen_rect, resized_rect)
            && resized != prev_rect
        {
            self.non_fullscreen_rect = Some(resized);
            self.remembered_size = Some(resized.size());
        }
        let (pressed, pos) = ctx.input(|i| (i.pointer.any_pressed(), i.pointer.interact_pos()));
        let inside = pos.is_some_and(|p| rect.contains(p));
        self.has_focus = next_preview_focus(self.has_focus, pressed, inside);
        self.had_text_focus = ctx.egui_wants_keyboard_input();
        self.window_rect = Some(rect);

        // Escape dismisses open popups (e.g. matching dropdowns) first, then the end-of-preview
        // popup, then leaves full screen, then closes the preview (unless the player drawer or a
        // focused text input used it first). Clicking the dimmed backdrop outside the window
        // closes it only when not full screen and no popup was open.
        let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        let backdrop_clicked = !self.fullscreen
            && !popup_was_open
            && backdrop_response.clicked_by(egui::PointerButton::Primary)
            && pos.is_some_and(|p| !rect.contains(p));
        if close_requested || backdrop_clicked {
            *open = false;
        } else if escape && !drawer_was_open && !popup_was_open && !egui::Popup::is_any_open(ctx) {
            if self.end_of_preview_popup {
                self.end_of_preview_popup = false;
            } else if !text_edit_was_focused {
                if self.fullscreen {
                    self.fullscreen = false;
                } else {
                    *open = false;
                }
            }
        }

        let media_source = media.as_ref().map(|(_, s)| *s);
        self.finish_frame_media_and_schedule(
            ctx,
            *open,
            dt,
            now,
            &frame_effects,
            media_source,
            &mut should_show_errors,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn finish_frame_media_and_schedule(
        &mut self,
        ctx: &egui::Context,
        open: bool,
        dt: f32,
        now: f64,
        frame_effects: &[Effect],
        media_source: Option<&dyn MediaSource>,
        should_show_errors: &mut bool,
    ) {
        let mut wake: Option<u32> = None;
        if !open {
            self.media.stop_all();
        } else if let Some(state) = self.state.take() {
            match resume_with_plan(&self.course, state, self.play_plan.as_ref()) {
                Ok(mut player) => {
                    match media_source {
                        Some(source) => {
                            queue_prints(frame_effects, &player, source, &mut self.requests);
                        }
                        None => {
                            queue_prints(frame_effects, &player, &NoMedia, &mut self.requests);
                        }
                    }
                    dispatch_media_effects(
                        &mut self.media,
                        &mut player,
                        frame_effects,
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
                    let dt_ms = if dt.is_finite() && dt > 0.0 {
                        (f64::from(dt.min(1.0)) * 1000.0)
                            .round()
                            .clamp(0.0, u64::MAX as f64) as u64
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
                    if player.take_preview_boundary() {
                        self.end_of_preview_popup = true;
                    }
                    if !self.ended {
                        wake = player.next_wake_ms();
                    }
                    self.state = Some(player.into_state());
                }
                Err(issues) => {
                    self.errors = issues;
                    *should_show_errors = true;
                }
            }
        }

        if *should_show_errors {
            self.state = None;
        }

        if open {
            self.var_changed_at
                .retain(|_, changed_at| (0.0..=1.5).contains(&(now - *changed_at)));
            if self.view.transition_active()
                || self.media.pending_len() > 0
                || (self.show_variables && !self.var_changed_at.is_empty())
            {
                wake = Some(0);
            }
            match wake {
                Some(0) => ctx.request_repaint(),
                Some(ms) => ctx.request_repaint_after(std::time::Duration::from_millis(u64::from(
                    ms.clamp(1, 900),
                ))),
                None => {
                    self.last_frame_time = None;
                    self.ticker.reset();
                }
            }
        } else {
            self.last_frame_time = None;
            self.ticker.reset();
        }
    }
}

fn preview_bounds(screen_rect: Rect) -> Rect {
    let shrunk = screen_rect.shrink(PREVIEW_MARGIN);
    if shrunk.is_positive() {
        shrunk
    } else {
        screen_rect
    }
}

fn preview_size_limits(bounds: Rect) -> (egui::Vec2, egui::Vec2) {
    let max_size = egui::vec2(bounds.width().max(1.0), bounds.height().max(1.0));
    let min_size = egui::vec2(
        PREVIEW_MIN_WIDTH.min(max_size.x),
        PREVIEW_MIN_HEIGHT.min(max_size.y),
    );
    (min_size, max_size)
}

fn clamp_preview_size(size: egui::Vec2, min_size: egui::Vec2, max_size: egui::Vec2) -> egui::Vec2 {
    egui::vec2(
        coursecraft_course::safe_clamp(size.x, min_size.x, max_size.x),
        coursecraft_course::safe_clamp(size.y, min_size.y, max_size.y),
    )
}

fn clamp_preview_rect(
    rect: Rect,
    bounds: Rect,
    min_size: egui::Vec2,
    max_size: egui::Vec2,
) -> Rect {
    let size = clamp_preview_size(rect.size(), min_size, max_size);
    let max_left = (bounds.max.x - size.x).max(bounds.min.x);
    let max_top = (bounds.max.y - size.y).max(bounds.min.y);
    let min_x = coursecraft_course::safe_clamp(rect.min.x, bounds.min.x, max_left);
    let min_y = coursecraft_course::safe_clamp(rect.min.y, bounds.min.y, max_top);
    Rect::from_min_size(egui::pos2(min_x, min_y), size)
}

#[derive(Clone, Copy)]
enum ResizeEdge {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Top,
    Bottom,
    Left,
    Right,
}

impl ResizeEdge {
    const ALL: [Self; 8] = [
        Self::TopLeft,
        Self::TopRight,
        Self::BottomLeft,
        Self::BottomRight,
        Self::Top,
        Self::Bottom,
        Self::Left,
        Self::Right,
    ];

    fn id_suffix(self) -> &'static str {
        match self {
            Self::TopLeft => "tl",
            Self::TopRight => "tr",
            Self::BottomLeft => "bl",
            Self::BottomRight => "br",
            Self::Top => "t",
            Self::Bottom => "b",
            Self::Left => "l",
            Self::Right => "r",
        }
    }

    fn cursor(self) -> egui::CursorIcon {
        match self {
            Self::TopLeft | Self::BottomRight => egui::CursorIcon::ResizeNwSe,
            Self::TopRight | Self::BottomLeft => egui::CursorIcon::ResizeNeSw,
            Self::Top | Self::Bottom => egui::CursorIcon::ResizeVertical,
            Self::Left | Self::Right => egui::CursorIcon::ResizeHorizontal,
        }
    }

    fn grip_rect(self, r: Rect) -> Rect {
        match self {
            Self::TopLeft => Rect::from_min_max(
                egui::pos2(r.min.x - RESIZE_GRIP_OUT, r.min.y - RESIZE_GRIP_OUT),
                egui::pos2(r.min.x + RESIZE_CORNER_IN, r.min.y + RESIZE_CORNER_IN),
            ),
            Self::TopRight => Rect::from_min_max(
                egui::pos2(r.max.x - RESIZE_CORNER_IN, r.min.y - RESIZE_GRIP_OUT),
                egui::pos2(r.max.x + RESIZE_GRIP_OUT, r.min.y + RESIZE_CORNER_IN),
            ),
            Self::BottomLeft => Rect::from_min_max(
                egui::pos2(r.min.x - RESIZE_GRIP_OUT, r.max.y - RESIZE_CORNER_IN),
                egui::pos2(r.min.x + RESIZE_CORNER_IN, r.max.y + RESIZE_GRIP_OUT),
            ),
            Self::BottomRight => Rect::from_min_max(
                egui::pos2(r.max.x - RESIZE_CORNER_IN, r.max.y - RESIZE_CORNER_IN),
                egui::pos2(r.max.x + RESIZE_GRIP_OUT, r.max.y + RESIZE_GRIP_OUT),
            ),
            Self::Top => Rect::from_min_max(
                egui::pos2(r.min.x + RESIZE_CORNER_IN, r.min.y - RESIZE_GRIP_OUT),
                egui::pos2(r.max.x - RESIZE_CORNER_IN, r.min.y + RESIZE_GRIP_IN),
            ),
            Self::Bottom => Rect::from_min_max(
                egui::pos2(r.min.x + RESIZE_CORNER_IN, r.max.y - RESIZE_GRIP_IN),
                egui::pos2(r.max.x - RESIZE_CORNER_IN, r.max.y + RESIZE_GRIP_OUT),
            ),
            Self::Left => Rect::from_min_max(
                egui::pos2(r.min.x - RESIZE_GRIP_OUT, r.min.y + RESIZE_CORNER_IN),
                egui::pos2(r.min.x + RESIZE_GRIP_IN, r.max.y - RESIZE_CORNER_IN),
            ),
            Self::Right => Rect::from_min_max(
                egui::pos2(r.max.x - RESIZE_GRIP_IN, r.min.y + RESIZE_CORNER_IN),
                egui::pos2(r.max.x + RESIZE_GRIP_OUT, r.max.y - RESIZE_CORNER_IN),
            ),
        }
    }

    fn moves_left(self) -> bool {
        matches!(self, Self::Left | Self::TopLeft | Self::BottomLeft)
    }

    fn moves_right(self) -> bool {
        matches!(self, Self::Right | Self::TopRight | Self::BottomRight)
    }

    fn moves_top(self) -> bool {
        matches!(self, Self::Top | Self::TopLeft | Self::TopRight)
    }

    fn moves_bottom(self) -> bool {
        matches!(self, Self::Bottom | Self::BottomLeft | Self::BottomRight)
    }
}

fn handle_resize_grips(
    ui: &mut Ui,
    rect: Rect,
    bounds: Rect,
    min_size: egui::Vec2,
    max_size: egui::Vec2,
) -> Rect {
    let mut updated = rect;
    let ctx = ui.ctx();
    for edge in ResizeEdge::ALL {
        let grip = edge.grip_rect(rect);
        let resp = ui.interact(
            grip,
            Id::unique(("preview_resize", edge.id_suffix())),
            egui::Sense::drag(),
        );
        if resp.hovered() || resp.dragged() {
            ctx.set_cursor_icon(edge.cursor());
        }
        if resp.dragged() {
            let delta = resp.drag_delta();
            if edge.moves_left() {
                let min_bound = bounds.min.x.max(updated.max.x - max_size.x);
                let max_bound = updated.max.x - min_size.x;
                updated.min.x =
                    (updated.min.x + delta.x).clamp(min_bound.min(max_bound), max_bound);
            }
            if edge.moves_right() {
                let min_bound = updated.min.x + min_size.x;
                let max_bound = bounds.max.x.min(updated.min.x + max_size.x);
                updated.max.x =
                    (updated.max.x + delta.x).clamp(min_bound, max_bound.max(min_bound));
            }
            if edge.moves_top() {
                let min_bound = bounds.min.y.max(updated.max.y - max_size.y);
                let max_bound = updated.max.y - min_size.y;
                updated.min.y =
                    (updated.min.y + delta.y).clamp(min_bound.min(max_bound), max_bound);
            }
            if edge.moves_bottom() {
                let min_bound = updated.min.y + min_size.y;
                let max_bound = bounds.max.y.min(updated.min.y + max_size.y);
                updated.max.y =
                    (updated.max.y + delta.y).clamp(min_bound, max_bound.max(min_bound));
            }
        }
    }
    updated
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn m31_11_clamp_preview_size_and_rect_handle_hostile_bounds() {
        let inverted = clamp_preview_size(
            egui::vec2(400.0, 300.0),
            egui::vec2(500.0, 400.0),
            egui::vec2(100.0, 100.0),
        );
        assert_eq!(inverted, egui::vec2(100.0, 100.0));

        let nan_bounds = clamp_preview_size(
            egui::vec2(f32::NAN, f32::INFINITY),
            egui::vec2(f32::NAN, 10.0),
            egui::vec2(100.0, f32::NAN),
        );
        assert!(nan_bounds.x.is_finite() && nan_bounds.y.is_finite());

        let _ = clamp_preview_rect(
            Rect::NOTHING,
            Rect::NOTHING,
            egui::vec2(300.0, 200.0),
            egui::vec2(100.0, 100.0),
        );
        let rect = clamp_preview_rect(
            Rect::NOTHING,
            Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(50.0, 50.0)),
            egui::vec2(300.0, 200.0),
            egui::vec2(100.0, 100.0),
        );
        assert_eq!(rect.size(), egui::vec2(100.0, 100.0));
    }
}
