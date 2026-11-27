//! CourseCraft player application and presentation state.

use coursecraft_course::Course;
use coursecraft_logic::{Effect, Input, Player};
use coursecraft_playback::MediaBackend;
use coursecraft_render::{MediaDriver, MediaSource, PlayerFrame, SlideView, Ticker};

/// Maximum number of log entries retained before dropping the oldest.
pub const MAX_LOG_ENTRIES: usize = 2000;

/// Push a log entry with custom capacity, dropping oldest entries when the limit is exceeded.
pub fn capped_push_log(log: &mut Vec<String>, line: String, max_entries: usize) {
    if max_entries == 0 {
        log.clear();
        return;
    }
    if log.len() >= max_entries {
        let overflow = log.len().saturating_sub(max_entries).saturating_add(1);
        log.drain(0..overflow);
    }
    log.push(line);
}

/// Push a log entry, capping the log length at [`MAX_LOG_ENTRIES`] by dropping oldest entries.
pub fn push_log_entry(log: &mut Vec<String>, line: String) {
    capped_push_log(log, line, MAX_LOG_ENTRIES);
}

/// Format validation issues into a single error string.
pub fn format_issues(issues: Vec<coursecraft_course::Issue>) -> String {
    issues
        .into_iter()
        .map(|i| format!("{}: {}", i.path, i.message))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Today's UTC date from the system clock, as `YYYY-MM-DD`.
#[cfg(not(target_arch = "wasm32"))]
fn system_date() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX)
        });
    coursecraft_logic::civil_date(secs)
}

/// Compute session elapsed time in milliseconds, guarding against negative, NaN,
/// infinite, and out-of-range floats.
#[cfg(any(target_arch = "wasm32", test))]
#[must_use]
pub fn compute_session_ms(start_ms: f64, now_ms: f64) -> u64 {
    if !start_ms.is_finite() || !now_ms.is_finite() {
        return 0;
    }
    let elapsed = now_ms - start_ms;
    if elapsed <= 0.0 {
        return 0;
    }
    if elapsed >= (u64::MAX as f64) {
        return u64::MAX;
    }
    elapsed.floor() as u64
}

/// Active player state holding the runtime, compositor, and effect log.
pub struct Playing {
    /// Runtime player owning course state.
    pub player: Player<'static>,
    /// Slide compositor and pointer input view.
    pub view: SlideView,
    /// Learner-facing player frame: strip, seek bar, and outline.
    pub frame: PlayerFrame,
    /// Clock accumulator ticking real time.
    pub ticker: Ticker,
    /// Last egui frame timestamp (`InputState::time`), used to compute real
    /// elapsed wall time across delayed wakes without losing precision to
    /// `stable_dt`'s 100 ms cap.
    last_frame_time: Option<f64>,
    /// Media texture cache.
    pub media_cache: coursecraft_render::MediaCache,
    /// Host media provider for images.
    #[cfg(not(target_arch = "wasm32"))]
    pub media_source: crate::media::DesktopMediaSource,
    /// Host media provider for images.
    #[cfg(target_arch = "wasm32")]
    pub media_source: crate::media::WebMediaSource,
    /// Drives the playback backend from runtime media effects.
    pub media: MediaDriver,
    /// Effect log lines (serialized JSON), capped at [`MAX_LOG_ENTRIES`].
    pub log: Vec<String>,
    /// Whether the effect log panel is visible.
    pub show_log: bool,
    /// Whether the course has ended ([`Effect::Exit`]).
    pub ended: bool,
    /// Whether to log each effect to stdout on desktop.
    pub log_effects: bool,
    /// LMS bridge and API connection (wasm only).
    #[cfg(target_arch = "wasm32")]
    pub lms: Option<crate::lms::LmsHandle>,
    /// xAPI / cmi5 bridge and transport (wasm only).
    #[cfg(target_arch = "wasm32")]
    pub xapi: Option<crate::lrs::XapiHandle>,
    /// LTI 1.3 reporter and transport (wasm only).
    #[cfg(target_arch = "wasm32")]
    pub lti: Option<crate::lti::LtiHandle>,
    /// Browser `localStorage` resume state for standalone web launches (wasm only).
    #[cfg(target_arch = "wasm32")]
    pub web_resume: Option<crate::web_resume::WebResumeHandle>,
    /// Leaked course reference for restarting (wasm only).
    #[cfg(target_arch = "wasm32")]
    course: &'static Course,
    /// Course session start timestamp in milliseconds (wasm only).
    #[cfg(target_arch = "wasm32")]
    pub start_ms: f64,
    /// Hidden screen-reader DOM layer (wasm only).
    #[cfg(target_arch = "wasm32")]
    pub a11y: Option<crate::a11y::AccessLayer>,
    /// Iframes over web objects (wasm only).
    #[cfg(target_arch = "wasm32")]
    pub web_objects: crate::web_object::WebObjects,
    /// Last slide id whose media was marked protected in `media_source` (wasm only).
    #[cfg(target_arch = "wasm32")]
    last_slide_id: Option<String>,
    /// Last slide id for which next-slide image prefetching was triggered (wasm only).
    #[cfg(target_arch = "wasm32")]
    last_prefetched_slide_id: Option<String>,
}

impl Drop for Playing {
    fn drop(&mut self) {
        self.media.stop_all();
        #[cfg(target_arch = "wasm32")]
        self.web_objects.clear();
    }
}

impl std::fmt::Debug for Playing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut d = f.debug_struct("Playing");
        d.field("log_len", &self.log.len())
            .field("show_log", &self.show_log)
            .field("ended", &self.ended)
            .field("log_effects", &self.log_effects);
        #[cfg(target_arch = "wasm32")]
        d.field("has_lms", &self.lms.is_some())
            .field("has_xapi", &self.xapi.is_some())
            .field("has_lti", &self.lti.is_some())
            .field("has_web_resume", &self.web_resume.is_some())
            .field("has_a11y", &self.a11y.is_some())
            .field("start_ms", &self.start_ms);
        d.finish()
    }
}

impl Playing {
    /// Create a new [`Playing`] instance for a static course on desktop.
    ///
    /// # Errors
    /// Returns joined validation issues if the course cannot be played.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new(
        course: &'static Course,
        log_effects: bool,
        course_path: Option<&std::path::Path>,
    ) -> Result<Self, String> {
        let mut player = Player::new(course).map_err(format_issues)?;
        player.set_host_info(coursecraft_logic::HostInfo {
            learner_name: None,
            date: Some(system_date()),
        });
        let default_path = std::path::Path::new("course.json");
        let path = course_path.unwrap_or(default_path);
        let media_source = crate::media::DesktopMediaSource::new(path);

        Ok(Self {
            player,
            view: SlideView::new(),
            frame: PlayerFrame::new(),
            ticker: Ticker::new(),
            last_frame_time: None,
            media_cache: coursecraft_render::MediaCache::new(),
            media_source,
            media: MediaDriver::new(),
            log: Vec::new(),
            show_log: false,
            ended: false,
            log_effects,
        })
    }

    /// Create a new [`Playing`] instance for a static course on web.
    ///
    /// # Errors
    /// Returns joined validation issues if the course cannot be played.
    #[cfg(target_arch = "wasm32")]
    pub fn new(
        course: &'static Course,
        ctx: egui::Context,
        standard: coursecraft_lms::Standard,
        log_effects: bool,
    ) -> Result<Self, String> {
        let (player, lms, start_ms) = crate::lms::init_player(course, standard)?;

        Ok(Self::assemble(
            course,
            ctx,
            player,
            lms,
            None,
            None,
            None,
            start_ms,
            log_effects,
        ))
    }

    #[cfg(target_arch = "wasm32")]
    #[allow(clippy::too_many_arguments)]
    fn assemble(
        course: &'static Course,
        ctx: egui::Context,
        player: Player<'static>,
        lms: Option<crate::lms::LmsHandle>,
        xapi: Option<crate::lrs::XapiHandle>,
        lti: Option<crate::lti::LtiHandle>,
        web_resume: Option<crate::web_resume::WebResumeHandle>,
        start_ms: f64,
        log_effects: bool,
    ) -> Self {
        let title = player.display_text(&player.course().title);
        let a11y = crate::a11y::AccessLayer::new(ctx.clone(), &title);
        let mut frame = PlayerFrame::new();
        if a11y.is_some() {
            frame.focus_mut().set_dom_focus_owner(true);
        }
        let lite_cache = crate::load::web_is_constrained_device();
        let profile = coursecraft_render::PerfProfile::for_player_cache(lite_cache, true);
        let mut media_cache = coursecraft_render::MediaCache::new();
        media_cache.set_picture_budget(profile.media_budget_bytes);
        coursecraft_render::set_render_profile(&ctx, &profile);
        let media_source =
            crate::media::WebMediaSource::with_max_bytes(ctx, profile.web_media_cache_bytes);
        Self {
            player,
            view: SlideView::new(),
            frame,
            ticker: Ticker::new(),
            last_frame_time: None,
            media_cache,
            media_source,
            media: MediaDriver::new(),
            log: Vec::new(),
            show_log: false,
            ended: false,
            log_effects,
            lms,
            xapi,
            lti,
            web_resume,
            course,
            start_ms,
            a11y,
            web_objects: crate::web_object::WebObjects::new(),
            last_slide_id: None,
            last_prefetched_slide_id: None,
        }
    }

    /// Create a web [`Playing`] with no LMS reporting and no `localStorage` resume.
    ///
    /// # Errors
    /// Returns joined validation issues if the course cannot be played.
    #[cfg(target_arch = "wasm32")]
    pub fn standalone(
        course: &'static Course,
        ctx: egui::Context,
        log_effects: bool,
    ) -> Result<Self, String> {
        let mut player = crate::lms::fresh_player(course)?;
        player.set_host_info(coursecraft_logic::HostInfo {
            learner_name: None,
            date: Some(crate::lms::today_date()),
        });
        Ok(Self::assemble(
            course,
            ctx,
            player,
            None,
            None,
            None,
            None,
            crate::lms::now_ms(),
            log_effects,
        ))
    }

    /// Create a web [`Playing`] for a Publish to Web course with `localStorage` resume.
    ///
    /// # Errors
    /// Returns joined validation issues if the course cannot be played.
    #[cfg(target_arch = "wasm32")]
    pub fn web(
        course: &'static Course,
        ctx: egui::Context,
        log_effects: bool,
    ) -> Result<Self, String> {
        let (player, web_resume, start_ms) = crate::web_resume::init_web_player(course)?;
        Ok(Self::assemble(
            course,
            ctx,
            player,
            None,
            None,
            None,
            Some(web_resume),
            start_ms,
            log_effects,
        ))
    }

    /// Create a web [`Playing`] from a finished xAPI handshake.
    #[cfg(target_arch = "wasm32")]
    pub fn from_connected(
        course: &'static Course,
        ctx: egui::Context,
        player: Player<'static>,
        xapi: crate::lrs::XapiHandle,
        start_ms: f64,
        log_effects: bool,
    ) -> Self {
        crate::lrs::register_page_close(&xapi, course);
        Self::assemble(
            course,
            ctx,
            player,
            None,
            Some(xapi),
            None,
            None,
            start_ms,
            log_effects,
        )
    }

    /// Create a web [`Playing`] for an LTI 1.3 package.
    #[cfg(target_arch = "wasm32")]
    pub fn from_lti(
        course: &'static Course,
        ctx: egui::Context,
        player: Player<'static>,
        lti: crate::lti::LtiHandle,
        start_ms: f64,
        log_effects: bool,
    ) -> Self {
        crate::lti::register_page_close(&lti);
        Self::assemble(
            course,
            ctx,
            player,
            None,
            None,
            Some(lti),
            None,
            start_ms,
            log_effects,
        )
    }

    /// Clear saved web resume data and restart the course from the beginning.
    #[cfg(target_arch = "wasm32")]
    pub fn restart(&mut self, ctx: Option<&egui::Context>) {
        self.media.stop_all();
        self.web_objects.clear();
        if let Some(ref web_resume) = self.web_resume {
            crate::web_resume::clear(web_resume);
        }
        if let Ok(mut player) = crate::lms::fresh_player(self.course) {
            player.set_host_info(coursecraft_logic::HostInfo {
                learner_name: None,
                date: Some(crate::lms::today_date()),
            });
            self.player = player;
            self.ended = false;
            self.ticker = Ticker::new();
            self.last_frame_time = None;
            self.view = SlideView::new();
            self.frame.restart();
            self.media.set_rate(1.0);
            self.last_slide_id = None;
            self.last_prefetched_slide_id = None;
            self.handle_input(&Input::Start, ctx);
        }
    }

    /// Replace the media playback backend (useful for injecting [`NullBackend`] in tests).
    #[must_use]
    pub fn with_backend(mut self, backend: Box<dyn MediaBackend>) -> Self {
        self.media.set_backend(backend);
        self
    }

    /// Convenience helper that leaks the owned [`Course`] and creates [`Playing`].
    #[cfg(not(target_arch = "wasm32"))]
    pub fn from_course(course: Course, log_effects: bool) -> Result<Self, String> {
        let course_ref: &'static Course = Box::leak(Box::new(course));
        Self::new(course_ref, log_effects, None)
    }

    /// Convenience helper that leaks the owned [`Course`] and creates [`Playing`] with course path.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn from_course_with_path(
        course: Course,
        log_effects: bool,
        course_path: &std::path::Path,
    ) -> Result<Self, String> {
        let course_ref: &'static Course = Box::leak(Box::new(course));
        Self::new(course_ref, log_effects, Some(course_path))
    }

    /// Convenience helper that leaks the owned [`Course`] and creates [`Playing`] for web.
    #[cfg(target_arch = "wasm32")]
    pub fn from_course(course: Course, ctx: egui::Context) -> Result<Self, String> {
        Self::from_course_with_standard(course, ctx, coursecraft_lms::Standard::Scorm12, false)
    }

    /// Convenience helper for web that also selects the package standard.
    #[cfg(target_arch = "wasm32")]
    pub fn from_course_with_standard(
        course: Course,
        ctx: egui::Context,
        standard: coursecraft_lms::Standard,
        log_effects: bool,
    ) -> Result<Self, String> {
        let course_ref: &'static Course = Box::leak(Box::new(course));
        Self::new(course_ref, ctx, standard, log_effects)
    }

    /// Handle a runtime input, emitting and logging each resulting effect.
    pub fn handle_input(&mut self, input: &Input, ctx: Option<&egui::Context>) {
        let media = &mut self.media;
        let media_source = &self.media_source;
        let log = &mut self.log;
        let log_effects = self.log_effects;
        let ended = &mut self.ended;
        #[cfg(target_arch = "wasm32")]
        let lms = &self.lms;
        #[cfg(target_arch = "wasm32")]
        let xapi = &self.xapi;
        #[cfg(target_arch = "wasm32")]
        let lti = &self.lti;
        #[cfg(target_arch = "wasm32")]
        let web_resume = &self.web_resume;
        #[cfg(target_arch = "wasm32")]
        let start_ms = self.start_ms;
        #[cfg(target_arch = "wasm32")]
        let frame = &mut self.frame;
        #[cfg(target_arch = "wasm32")]
        let a11y = &self.a11y;

        crate::session::sequence_input(
            &mut self.player,
            input,
            &mut |player, effects| {
                #[cfg(not(target_arch = "wasm32"))]
                let _ = (player, effects);
                #[cfg(target_arch = "wasm32")]
                {
                    if let Some(lms) = lms {
                        crate::lms::after_input(
                            lms,
                            player.course(),
                            player.state(),
                            effects,
                            start_ms,
                        );
                    }
                    if let Some(xapi) = xapi {
                        crate::lrs::after_input(xapi, player.course(), player.state(), effects);
                    }
                    if let Some(lti) = lti {
                        crate::lti::after_input(lti, effects);
                    }
                    if let Some(web_resume) = web_resume {
                        crate::web_resume::after_input(
                            web_resume,
                            player.course(),
                            player.state(),
                            effects,
                        );
                    }
                }
            },
            &mut |player, effects| {
                let mut followups = Vec::new();
                for effect in effects {
                    if let Ok(line) = serde_json::to_string(effect) {
                        #[cfg(not(target_arch = "wasm32"))]
                        if log_effects {
                            println!("{line}");
                        }
                        #[cfg(target_arch = "wasm32")]
                        if log_effects {
                            web_sys::console::log_1(&eframe::wasm_bindgen::JsValue::from_str(
                                &line,
                            ));
                        }
                        push_log_entry(log, line);
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
                        Effect::RunJavaScript { code } => {
                            #[cfg(target_arch = "wasm32")]
                            {
                                followups.extend(crate::web_object::run_javascript(player, code));
                            }
                            #[cfg(not(target_arch = "wasm32"))]
                            {
                                let _ = code;
                                let line = "JavaScript runs in the published course".to_owned();
                                if log_effects {
                                    println!("{line}");
                                }
                                push_log_entry(log, line);
                            }
                        }
                        Effect::AnswerRequired =>
                        {
                            #[cfg(target_arch = "wasm32")]
                            if let Some(a11y) = a11y {
                                a11y.announce(player.course().chrome_string("answerRequired"));
                            }
                        }
                        Effect::Print { exclude, .. } => {
                            #[cfg(not(target_arch = "wasm32"))]
                            let _ = exclude;
                            #[cfg(target_arch = "wasm32")]
                            crate::print::print_slide(player, media_source, frame, exclude);
                            #[cfg(not(target_arch = "wasm32"))]
                            {
                                let line =
                                    "Printing works in the published course or the editor preview."
                                        .to_owned();
                                if log_effects {
                                    println!("{line}");
                                }
                                push_log_entry(log, line);
                            }
                        }
                        _ => {}
                    }
                }
                followups.extend(media.apply(player, effects, Some(media_source)));
                #[cfg(target_arch = "wasm32")]
                if effects.iter().any(|effect| matches!(effect, Effect::Exit))
                    && let Some(url) = xapi.as_ref().and_then(crate::lrs::return_url)
                    && let Some(window) = web_sys::window()
                {
                    let _ = window.location().set_href(&url);
                }
                followups
            },
        );
        // Read the speed after the runtime handled the input, so media never lags a change.
        if matches!(input, Input::SetSpeed { .. }) {
            self.media.set_rate(self.player.speed());
        }
    }

    /// Poll media events, resolve pending media fetches, and update video layouts.
    pub fn poll_media(&mut self, ctx: Option<&egui::Context>) {
        for input in self.media.poll(Some(&self.media_source)) {
            self.handle_input(&input, ctx);
        }
        for input in self.media_cache.lottie_ended(&self.player) {
            self.handle_input(&input, ctx);
        }
        self.layout_videos();
    }

    /// Report playing videos' positions so cue points can fire. Call right
    /// after [`Playing::poll_media`], next to the video element layout.
    pub fn poll_media_time(&mut self, ctx: Option<&egui::Context>, dt_ms: u64) {
        for input in self.media.media_time_inputs(&self.player, dt_ms) {
            self.handle_input(&input, ctx);
        }
    }

    /// Update backend video layout positions for the current slide's videos.
    pub fn layout_videos(&mut self) {
        let hide = should_hide_dom_overlays(&self.player, &self.frame, self.ended);
        self.media
            .layout_videos_with_visibility(&self.player, &self.view, hide);
    }

    #[cfg(target_arch = "wasm32")]
    fn sync_web_media_prefetch(&mut self) {
        let Some(slide) = self.player.current_slide() else {
            return;
        };
        if self.last_slide_id.as_deref() != Some(slide.id.as_str()) {
            let protected = crate::media::slide_media_srcs(self.player.course(), slide);
            self.media_source.set_current_slide_media(&protected);
            self.last_slide_id = Some(slide.id.clone());
        }
        if self.last_prefetched_slide_id.as_deref() != Some(slide.id.as_str())
            && self.media_source.is_idle()
        {
            if let Some(next_slide) = self
                .player
                .slide_position(&slide.id)
                .and_then(|pos| self.player.play_order().get(pos + 1).copied())
            {
                let next_images = crate::media::slide_image_srcs(self.player.course(), next_slide);
                self.media_source.prefetch(&next_images);
            }
            self.last_prefetched_slide_id = Some(slide.id.clone());
        }
    }
}

/// A course waiting for its xAPI / cmi5 handshake (wasm only).
#[cfg(target_arch = "wasm32")]
pub struct Connecting {
    /// The leaked course being connected.
    pub course: &'static Course,
    /// Bridge driving the handshake.
    pub bridge: coursecraft_lms::XapiBridge,
    /// Browser transport.
    pub api: crate::lrs::JsLrsApi,
    /// Whether the learner prefers reduced motion.
    pub reduced_motion: bool,
}

#[cfg(target_arch = "wasm32")]
impl std::fmt::Debug for Connecting {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Connecting").finish_non_exhaustive()
    }
}

/// Player application state.
#[derive(Debug)]
pub enum State {
    /// Waiting for course assets to load.
    Loading,
    /// Waiting for an xAPI / cmi5 handshake (wasm only).
    #[cfg(target_arch = "wasm32")]
    Connecting(Box<Connecting>),
    /// Course loading or initialization failed.
    Failed(String),
    /// Running the course.
    Playing(Box<Playing>),
}

/// CourseCraft player application.
pub struct PlayerApp {
    /// Current app state.
    pub state: State,
    #[cfg(target_arch = "wasm32")]
    web_loader: Option<crate::load::WebLoader>,
    #[cfg(target_arch = "wasm32")]
    ctx: egui::Context,
    #[cfg(target_arch = "wasm32")]
    log_effects: bool,
    #[cfg(target_arch = "wasm32")]
    loading_overlay_removed: bool,
}

#[cfg(not(target_arch = "wasm32"))]
impl PlayerApp {
    /// Initialize desktop player from command-line arguments.
    pub fn new_desktop(args: impl IntoIterator<Item = String>) -> Self {
        let config = match crate::load::parse_args(args) {
            Ok(c) => c,
            Err(err) => {
                return Self {
                    state: State::Failed(err),
                };
            }
        };
        let course = match crate::load::load_course_sync(&config.course_path) {
            Ok(c) => c,
            Err(err) => {
                return Self {
                    state: State::Failed(err),
                };
            }
        };
        match Playing::from_course_with_path(course, config.log_effects, &config.course_path) {
            Ok(mut playing) => {
                playing.handle_input(&Input::Start, None);
                Self {
                    state: State::Playing(Box::new(playing)),
                }
            }
            Err(err) => Self {
                state: State::Failed(err),
            },
        }
    }
}

/// Whether the page may go full screen. False inside an LMS iframe that lacks
/// `allowfullscreen`.
#[cfg(target_arch = "wasm32")]
fn fullscreen_available() -> bool {
    web_sys::window()
        .and_then(|window| window.document())
        .is_some_and(|document| document.fullscreen_enabled())
}

#[cfg(not(target_arch = "wasm32"))]
fn fullscreen_available() -> bool {
    true
}

#[cfg(target_arch = "wasm32")]
fn is_fullscreen(_ctx: &egui::Context) -> bool {
    web_sys::window()
        .and_then(|window| window.document())
        .is_some_and(|document| document.fullscreen_element().is_some())
}

#[cfg(not(target_arch = "wasm32"))]
fn is_fullscreen(ctx: &egui::Context) -> bool {
    ctx.input(|i| i.viewport().fullscreen.unwrap_or(false))
}

/// Enter or leave full screen. A browser that rejects the request is logged.
#[cfg(target_arch = "wasm32")]
fn toggle_fullscreen(_ctx: &egui::Context, currently: bool) {
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return;
    };
    if currently {
        document.exit_fullscreen();
    } else if let Some(element) = document.document_element()
        && let Err(error) = element.request_fullscreen()
    {
        log::warn!("full screen request was refused: {error:?}");
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn toggle_fullscreen(ctx: &egui::Context, currently: bool) {
    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!currently));
}

#[cfg(target_arch = "wasm32")]
fn prefers_reduced_motion() -> bool {
    web_sys::window()
        .and_then(|window| {
            window
                .match_media("(prefers-reduced-motion: reduce)")
                .ok()
                .flatten()
        })
        .is_some_and(|query| query.matches())
}

#[cfg(target_arch = "wasm32")]
impl PlayerApp {
    /// Initialize web player, starting asynchronous fetch of `course.json`.
    pub fn new_web(cc: &eframe::CreationContext<'_>) -> Self {
        coursecraft_render::install_fonts(&cc.egui_ctx);
        let loader = crate::load::start_web_fetch(&cc.egui_ctx);
        Self {
            state: State::Loading,
            web_loader: Some(loader),
            ctx: cc.egui_ctx.clone(),
            log_effects: crate::load::web_log_effects(),
            loading_overlay_removed: false,
        }
    }

    fn poll_web_loader(&mut self) {
        let Some(loader) = self.web_loader.clone() else {
            return;
        };
        let fetched = loader
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        if let Some(result) = fetched {
            self.web_loader = None;
            match result {
                Ok(web) => {
                    let (standard, warning) =
                        coursecraft_lms::reporting_from_marker(web.package_json.as_deref());
                    if let Some(warning) = warning {
                        crate::lms::log_warn(&warning);
                    }
                    match crate::load::parse_course(&web.course_json, "course.json") {
                        Ok(course) => match standard {
                            coursecraft_lms::ReportingStandard::Scorm(standard) => {
                                self.start_scorm(course, standard)
                            }
                            coursecraft_lms::ReportingStandard::Xapi(flavor) => {
                                self.start_xapi(course, flavor);
                            }
                            coursecraft_lms::ReportingStandard::Lti => self.start_lti(course),
                            coursecraft_lms::ReportingStandard::Web => self.start_web(course),
                        },
                        Err(e) => {
                            self.state = State::Failed(e);
                        }
                    }
                }
                Err(err) => {
                    self.state = State::Failed(err);
                }
            }
        }
    }

    /// Begin a SCORM launch with the package's standard.
    fn start_scorm(&mut self, course: Course, standard: coursecraft_lms::Standard) {
        match Playing::from_course_with_standard(
            course,
            self.ctx.clone(),
            standard,
            self.log_effects,
        ) {
            Ok(mut playing) => {
                playing.frame.reduced_motion = prefers_reduced_motion();
                playing.frame.awaiting_start = true;
                self.state = State::Playing(Box::new(playing));
            }
            Err(err) => {
                self.state = State::Failed(err);
            }
        }
    }

    /// Begin a standalone web session with no LMS reporting.
    fn start_web(&mut self, course: Course) {
        let course: &'static Course = Box::leak(Box::new(course));
        match Playing::web(course, self.ctx.clone(), self.log_effects) {
            Ok(mut playing) => {
                playing.frame.reduced_motion = prefers_reduced_motion();
                playing.frame.awaiting_start = true;
                self.state = State::Playing(Box::new(playing));
            }
            Err(err) => {
                self.state = State::Failed(err);
            }
        }
    }

    /// Begin an LTI 1.3 session. There is no handshake: LTI has no suspend
    /// data, so the course starts fresh and reports as it plays.
    fn start_lti(&mut self, course: Course) {
        let course: &'static Course = Box::leak(Box::new(course));
        match crate::lti::init_player(course, self.ctx.clone()) {
            Ok((player, lti, start_ms)) => {
                let mut playing = Playing::from_lti(
                    course,
                    self.ctx.clone(),
                    player,
                    lti,
                    start_ms,
                    self.log_effects,
                );
                playing.frame.reduced_motion = prefers_reduced_motion();
                playing.frame.awaiting_start = true;
                self.state = State::Playing(Box::new(playing));
            }
            Err(err) => {
                self.state = State::Failed(err);
            }
        }
    }

    /// Begin an xAPI / cmi5 launch from the page query string.
    fn start_xapi(&mut self, course: Course, flavor: coursecraft_xapi::Flavor) {
        let course: &'static Course = Box::leak(Box::new(course));
        let query = web_sys::window()
            .and_then(|window| window.location().search().ok())
            .unwrap_or_default();
        let params = match coursecraft_xapi::LaunchParams::parse(&query, flavor) {
            Ok(params) => params,
            Err(err) => {
                crate::lms::log_warn(&format!("xAPI launch failed: {err}"));
                crate::lms::log_standalone();
                match Playing::standalone(course, self.ctx.clone(), self.log_effects) {
                    Ok(mut playing) => {
                        playing.frame.reduced_motion = prefers_reduced_motion();
                        playing.frame.awaiting_start = true;
                        self.state = State::Playing(Box::new(playing));
                    }
                    Err(err) => {
                        self.state = State::Failed(err);
                    }
                }
                return;
            }
        };

        let mut bridge = coursecraft_lms::XapiBridge::new(params, flavor, course);
        let mut api = crate::lrs::JsLrsApi::new(self.ctx.clone());
        bridge.start(&mut api);
        self.state = State::Connecting(Box::new(Connecting {
            course,
            bridge,
            api,
            reduced_motion: prefers_reduced_motion(),
        }));
    }

    /// Tick the handshake and enter play when the bridge returns a plan.
    fn poll_connecting(&mut self, ctx: &egui::Context) {
        let tick = {
            let State::Connecting(connecting) = &mut self.state else {
                return;
            };
            connecting.bridge.tick(
                &mut connecting.api,
                connecting.course,
                crate::lrs::now_ms_u64(),
            )
        };
        for warning in tick.warnings {
            crate::lms::log_warn(&warning);
        }
        let Some(plan) = tick.plan else {
            ctx.request_repaint();
            return;
        };

        let start_ms = crate::lms::now_ms();
        let State::Connecting(connecting) = std::mem::replace(&mut self.state, State::Loading)
        else {
            return;
        };
        let Connecting {
            course,
            bridge,
            api,
            reduced_motion,
        } = *connecting;

        let coursecraft_lms::StartPlan {
            resume,
            learner_name,
        } = plan;
        let mut player = match resume {
            Some(state) => match Player::resume(course, state) {
                Ok(player) => player,
                Err(issues) => {
                    crate::lms::log_warn(&format!(
                        "Player::resume failed: {}",
                        format_issues(issues)
                    ));
                    match crate::lms::fresh_player(course) {
                        Ok(player) => player,
                        Err(msg) => {
                            self.state = State::Failed(msg);
                            return;
                        }
                    }
                }
            },
            None => match crate::lms::fresh_player(course) {
                Ok(player) => player,
                Err(msg) => {
                    self.state = State::Failed(msg);
                    return;
                }
            },
        };
        player.set_host_info(coursecraft_logic::HostInfo {
            learner_name,
            date: Some(crate::lms::today_date()),
        });

        let xapi = crate::lrs::new_handle(bridge, api, player.state().clone(), start_ms);
        let mut playing = Playing::from_connected(
            course,
            ctx.clone(),
            player,
            xapi,
            start_ms,
            self.log_effects,
        );
        playing.frame.reduced_motion = reduced_motion;
        playing.frame.awaiting_start = true;
        self.state = State::Playing(Box::new(playing));
    }
}

/// Render the course load failure screen on `PLAYER_GROUND` inside a raised card.
pub(crate) fn show_failed_screen(ui: &mut egui::Ui, message: &str) {
    let full = ui.available_rect_before_wrap();
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.fill(coursecraft_render::PLAYER_GROUND))
        .show(ui, |ui| {
            let max_card_w = (full.width() - 32.0).clamp(200.0, 480.0);
            ui.centered_and_justified(|ui| {
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(0x23, 0x23, 0x26))
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_white_alpha(0x1E)))
                    .corner_radius(egui::CornerRadius::same(12))
                    .inner_margin(egui::Margin::same(24))
                    .show(ui, |ui| {
                        ui.set_max_width(max_card_w - 48.0);
                        ui.vertical(|ui| {
                            ui.label(
                                egui::RichText::new("Unable to load course")
                                    .size(16.0)
                                    .strong()
                                    .color(egui::Color32::from_rgb(0xF2, 0xF2, 0xF5)),
                            );
                            ui.add_space(8.0);
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(message)
                                        .size(13.0)
                                        .color(egui::Color32::from_rgb(0xD1, 0xD1, 0xD6)),
                                )
                                .wrap(),
                            );
                            ui.add_space(12.0);
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(
                                        "Check that the course package is complete and reload the page.",
                                    )
                                    .size(12.0)
                                    .color(egui::Color32::from_rgb(0xA8, 0xA8, 0xB0)),
                                )
                                .wrap(),
                            );
                        });
                    });
            });
        });
}

impl eframe::App for PlayerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        #[cfg(target_arch = "wasm32")]
        if !self.loading_overlay_removed {
            self.loading_overlay_removed = true;
            crate::remove_loading_overlay();
        }
        #[cfg(target_arch = "wasm32")]
        if matches!(self.state, State::Loading) {
            self.poll_web_loader();
        }
        #[cfg(target_arch = "wasm32")]
        if matches!(self.state, State::Connecting(_)) {
            self.poll_connecting(ui.ctx());
        }

        if let State::Playing(ref playing) = self.state {
            coursecraft_render::install_course_fonts(
                ui.ctx(),
                playing.player.course(),
                &playing.media_source,
            );
        }

        match self.state {
            State::Loading => {
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ui, |ui| {
                        coursecraft_render::show_loading(ui, "");
                    });
            }
            #[cfg(target_arch = "wasm32")]
            State::Connecting(ref connecting) => {
                ui.ctx().request_repaint();
                let loading = connecting.course.chrome_string("loading");
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ui, |ui| {
                        coursecraft_render::show_loading_text(ui, "", loading);
                    });
            }
            State::Failed(ref message) => {
                show_failed_screen(ui, message);
            }
            State::Playing(ref mut playing) => {
                playing.show_ui(ui);
            }
        }
    }
}

impl Playing {
    /// Render one frame of the active player UI inside `ui`.
    pub(crate) fn show_ui(&mut self, ui: &mut egui::Ui) {
        let playing = self;
        playing.media_cache.begin_frame();

        #[cfg(target_arch = "wasm32")]
        if let Some(ref xapi) = playing.xapi {
            crate::lrs::update(xapi, playing.player.course());
            // A refreshed cmi5 course can't reuse its single-use launch;
            // tell the learner their work isn't being recorded.
            if crate::lrs::reporting_lost(xapi) && playing.frame.notice.is_none() {
                playing.frame.notice = Some(
                    playing
                        .player
                        .course()
                        .chrome_string("reportingLost")
                        .to_owned(),
                );
            }
        }

        #[cfg(target_arch = "wasm32")]
        if let Some(ref lti) = playing.lti {
            crate::lti::update(lti);
            // The one retry failed, so the platform will not see any
            // more results.
            if crate::lti::lost(lti) && playing.frame.notice.is_none() {
                playing.frame.notice = Some(
                    playing
                        .player
                        .course()
                        .chrome_string("reportingLost")
                        .to_owned(),
                );
            }
        }

        // 6. F2 toggles the effect log panel
        if ui.input(|i| i.key_pressed(egui::Key::F2)) {
            playing.show_log = !playing.show_log;
        }

        #[cfg(target_arch = "wasm32")]
        if let Some(layer) = playing.a11y.as_ref() {
            layer.before_frame(playing.frame.focus_mut());
        }

        // 1. Advance ticker and handle tick input if emitted. Nothing runs
        // until the learner presses Play on the start gate.
        let was_awaiting_start = playing.frame.awaiting_start;
        let (now, stable_dt) = ui.input(|i| (i.time, i.stable_dt));
        let dt = match playing.last_frame_time {
            Some(prev) if (now - prev).is_finite() && now > prev => (now - prev) as f32,
            _ => stable_dt,
        };
        playing.last_frame_time = Some(now);
        if !was_awaiting_start && let Some(tick) = playing.ticker.advance(dt) {
            playing.handle_input(&tick, Some(ui.ctx()));
        }

        // Right panel for effect log if active
        if playing.show_log {
            egui::Panel::right("log_panel")
                .default_size(360.0)
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            for line in &playing.log {
                                ui.monospace(line);
                            }
                        });
                });
        }

        // Central area runs the player frame (slide, strip, rule, drawer)
        let mut frame_output = coursecraft_render::FrameOutput::default();
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                playing.frame.ended = playing.ended;
                playing.frame.can_speed = playing.media.supports_rate();
                playing.frame.can_fullscreen = fullscreen_available();
                playing.frame.fullscreen = is_fullscreen(ui.ctx());
                #[cfg(target_arch = "wasm32")]
                {
                    playing.frame.can_restart = playing.web_resume.is_some();
                }
                let caption = playing.media.caption_frame(
                    &playing.player,
                    Some(&playing.media_source),
                    &playing.view,
                );
                playing
                    .media
                    .present_captions(caption.as_ref(), playing.frame.captions_on);
                playing
                    .frame
                    .set_caption_frame(if playing.frame.captions_on {
                        caption
                    } else {
                        None
                    });
                let source = playing.media.as_media_source(&playing.media_source);
                frame_output = playing.frame.show(
                    ui,
                    &playing.player,
                    &mut playing.view,
                    Some((&mut playing.media_cache, &source as &dyn MediaSource)),
                );
            });

        #[cfg(target_arch = "wasm32")]
        playing.sync_web_media_prefetch();

        #[cfg(target_arch = "wasm32")]
        let start_from_dom = playing
            .a11y
            .as_ref()
            .is_some_and(crate::a11y::AccessLayer::take_start_request);
        #[cfg(not(target_arch = "wasm32"))]
        let start_from_dom = false;
        if playing.frame.awaiting_start && (frame_output.start || start_from_dom) {
            // Start inside the learner's gesture, so the browser lets the
            // first slide's audio play.
            playing.frame.awaiting_start = false;
            playing.handle_input(&Input::Start, Some(ui.ctx()));
        }

        if let Some(volume) = frame_output.volume {
            playing.media.set_volume(volume);
        }
        if frame_output.fullscreen_toggle {
            toggle_fullscreen(ui.ctx(), is_fullscreen(ui.ctx()));
        }
        #[cfg(target_arch = "wasm32")]
        if frame_output.restart {
            playing.restart(Some(ui.ctx()));
        }

        #[cfg(target_arch = "wasm32")]
        if let Some(layer) = playing.a11y.as_mut() {
            let settings = &playing.player.effective_player();
            let started = playing.player.state().started;
            let show_menu = settings.menu_available() && started;
            let show_nav = settings.nav_available() && started;
            let transform = if playing.player.lightbox_open() {
                playing.view.lightbox_transform()
            } else {
                playing.view.last_transform()
            };
            let items = playing.view.last_items();
            layer.after_frame(
                &playing.player,
                &playing.frame,
                items,
                transform,
                show_nav,
                show_menu,
            );
        }

        // Feed returned inputs after the frame has painted
        for input in frame_output.inputs {
            playing.handle_input(&input, Some(ui.ctx()));
        }

        // Poll media ends, pending async media fetches, and update video element layout
        playing.poll_media(Some(ui.ctx()));
        let dt_ms = if !was_awaiting_start && dt.is_finite() && dt > 0.0 {
            (f64::from(dt.min(1.0)) * 1000.0)
                .round()
                .clamp(0.0, u64::MAX as f64) as u64
        } else {
            0
        };
        playing.poll_media_time(Some(ui.ctx()), dt_ms);
        #[cfg(target_arch = "wasm32")]
        {
            let hide = should_hide_dom_overlays(&playing.player, &playing.frame, playing.ended);
            playing
                .web_objects
                .layout(&playing.player, &playing.view, hide);
        }

        // Schedule the next wake only when time-driven work is pending.
        let mut wake: Option<u32> = if playing.frame.awaiting_start || playing.ended {
            None
        } else {
            playing.player.next_wake_ms()
        };
        if playing.view.transition_active() || playing.media.pending_len() > 0 {
            wake = Some(0);
        }
        #[cfg(target_arch = "wasm32")]
        if !playing.media_source.is_idle() {
            wake = Some(0);
        }
        match wake {
            Some(0) => ui.ctx().request_repaint(),
            Some(ms) => ui
                .ctx()
                .request_repaint_after(std::time::Duration::from_millis(u64::from(
                    ms.clamp(1, 900),
                ))),
            None => {
                playing.last_frame_time = None;
                playing.ticker.reset();
            }
        }
    }
}

/// Returns `true` when DOM overlays (HTML `<video>` and `<iframe>` web objects)
/// must be hidden so they do not cover the start gate, outline drawer, ended
/// card, or lightbox.
pub(crate) fn should_hide_dom_overlays(
    player: &Player<'_>,
    frame: &PlayerFrame,
    ended: bool,
) -> bool {
    frame.awaiting_start || ended || frame.drawer_open() || player.lightbox_open()
}
