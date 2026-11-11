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
    /// Course session start timestamp in milliseconds (wasm only).
    #[cfg(target_arch = "wasm32")]
    pub start_ms: f64,
    /// Hidden screen-reader DOM layer (wasm only).
    #[cfg(target_arch = "wasm32")]
    pub a11y: Option<crate::a11y::AccessLayer>,
    /// Iframes over web objects (wasm only).
    #[cfg(target_arch = "wasm32")]
    pub web_objects: crate::web_object::WebObjects,
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
    ) -> Result<Self, String> {
        let (player, lms, start_ms) = crate::lms::init_player(course, standard)?;

        Ok(Self::assemble(ctx, player, lms, None, None, start_ms))
    }

    #[cfg(target_arch = "wasm32")]
    fn assemble(
        ctx: egui::Context,
        player: Player<'static>,
        lms: Option<crate::lms::LmsHandle>,
        xapi: Option<crate::lrs::XapiHandle>,
        lti: Option<crate::lti::LtiHandle>,
        start_ms: f64,
    ) -> Self {
        let title = player.display_text(&player.course().title);
        let a11y = crate::a11y::AccessLayer::new(ctx.clone(), &title);
        let mut frame = PlayerFrame::new();
        if a11y.is_some() {
            frame.focus_mut().set_dom_focus_owner(true);
        }
        Self {
            player,
            view: SlideView::new(),
            frame,
            ticker: Ticker::new(),
            media_cache: coursecraft_render::MediaCache::new(),
            media_source: crate::media::WebMediaSource::new(ctx),
            media: MediaDriver::new(),
            log: Vec::new(),
            show_log: false,
            ended: false,
            log_effects: false,
            lms,
            xapi,
            lti,
            start_ms,
            a11y,
            web_objects: crate::web_object::WebObjects::new(),
        }
    }

    /// Create a web [`Playing`] with no LMS reporting.
    ///
    /// # Errors
    /// Returns joined validation issues if the course cannot be played.
    #[cfg(target_arch = "wasm32")]
    pub fn standalone(course: &'static Course, ctx: egui::Context) -> Result<Self, String> {
        let mut player = crate::lms::fresh_player(course)?;
        player.set_host_info(coursecraft_logic::HostInfo {
            learner_name: None,
            date: Some(crate::lms::today_date()),
        });
        Ok(Self::assemble(
            ctx,
            player,
            None,
            None,
            None,
            crate::lms::now_ms(),
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
    ) -> Self {
        crate::lrs::register_page_close(&xapi, course);
        Self::assemble(ctx, player, None, Some(xapi), None, start_ms)
    }

    /// Create a web [`Playing`] for an LTI 1.3 package.
    #[cfg(target_arch = "wasm32")]
    pub fn from_lti(
        ctx: egui::Context,
        player: Player<'static>,
        lti: crate::lti::LtiHandle,
        start_ms: f64,
    ) -> Self {
        Self::assemble(ctx, player, None, None, Some(lti), start_ms)
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
        Self::from_course_with_standard(course, ctx, coursecraft_lms::Standard::Scorm12)
    }

    /// Convenience helper for web that also selects the package standard.
    #[cfg(target_arch = "wasm32")]
    pub fn from_course_with_standard(
        course: Course,
        ctx: egui::Context,
        standard: coursecraft_lms::Standard,
    ) -> Result<Self, String> {
        let course_ref: &'static Course = Box::leak(Box::new(course));
        Self::new(course_ref, ctx, standard)
    }

    /// Handle a runtime input, emitting and logging each resulting effect.
    pub fn handle_input(&mut self, input: &Input, ctx: Option<&egui::Context>) {
        let effects = self.player.handle(input);
        if matches!(input, Input::SetSpeed { .. }) {
            self.media.set_rate(self.player.speed());
        }
        for effect in &effects {
            if let Ok(line) = serde_json::to_string(effect) {
                #[cfg(not(target_arch = "wasm32"))]
                if self.log_effects {
                    println!("{line}");
                }
                #[cfg(target_arch = "wasm32")]
                {
                    web_sys::console::log_1(&eframe::wasm_bindgen::JsValue::from_str(&line));
                }
                push_log_entry(&mut self.log, line);
            }
            match effect {
                Effect::OpenUrl { url } => {
                    if let Some(ctx) = ctx {
                        ctx.open_url(egui::OpenUrl::new_tab(url));
                    }
                }
                Effect::Exit => {
                    self.ended = true;
                }
                Effect::RunJavaScript { code } => {
                    #[cfg(target_arch = "wasm32")]
                    {
                        let inputs = crate::web_object::run_javascript(&self.player, code);
                        for input in inputs {
                            self.handle_input(&input, ctx);
                        }
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        let _ = code;
                        let line = "JavaScript runs in the published course".to_owned();
                        if self.log_effects {
                            println!("{line}");
                        }
                        push_log_entry(&mut self.log, line);
                    }
                }
                Effect::AnswerRequired =>
                {
                    #[cfg(target_arch = "wasm32")]
                    if let Some(a11y) = &self.a11y {
                        a11y.announce(self.player.course().chrome_string("answerRequired"));
                    }
                }
                Effect::Print { exclude, .. } => {
                    #[cfg(not(target_arch = "wasm32"))]
                    let _ = exclude;
                    #[cfg(target_arch = "wasm32")]
                    crate::print::print_slide(
                        &self.player,
                        &self.media_source,
                        &mut self.frame,
                        exclude,
                    );
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        let line = "Printing works in the published course or the editor preview."
                            .to_owned();
                        if self.log_effects {
                            println!("{line}");
                        }
                        push_log_entry(&mut self.log, line);
                    }
                }
                _ => {}
            }
        }
        let followups = self
            .media
            .apply(&self.player, &effects, Some(&self.media_source));
        for next in followups {
            self.handle_input(&next, ctx);
        }
        #[cfg(target_arch = "wasm32")]
        if let Some(ref lms) = self.lms {
            crate::lms::after_input(
                lms,
                self.player.course(),
                self.player.state(),
                &effects,
                self.start_ms,
            );
        }
        #[cfg(target_arch = "wasm32")]
        if let Some(ref xapi) = self.xapi {
            crate::lrs::after_input(xapi, self.player.course(), self.player.state(), &effects);
        }
        #[cfg(target_arch = "wasm32")]
        if let Some(ref lti) = self.lti {
            crate::lti::after_input(lti, &effects);
        }
        #[cfg(target_arch = "wasm32")]
        if effects.iter().any(|effect| matches!(effect, Effect::Exit))
            && let Some(url) = self.xapi.as_ref().and_then(crate::lrs::return_url)
            && let Some(window) = web_sys::window()
        {
            let _ = window.location().set_href(&url);
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
        self.media.layout_videos(&self.player, &self.view);
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
                        crate::load::parse_package_standard(web.package_json.as_deref());
                    if let Some(warning) = warning {
                        crate::lms::log_warn(&warning);
                    }
                    match crate::load::parse_course(&web.course_json, "course.json") {
                        Ok(course) => match standard {
                            crate::load::PlayerStandard::Scorm12 => {
                                self.start_scorm(course, coursecraft_lms::Standard::Scorm12)
                            }
                            crate::load::PlayerStandard::Scorm2004 => {
                                self.start_scorm(course, coursecraft_lms::Standard::Scorm2004)
                            }
                            crate::load::PlayerStandard::Cmi5 => {
                                self.start_xapi(course, coursecraft_xapi::Flavor::Cmi5);
                            }
                            crate::load::PlayerStandard::Xapi => {
                                self.start_xapi(course, coursecraft_xapi::Flavor::Xapi);
                            }
                            crate::load::PlayerStandard::Lti => self.start_lti(course),
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
        match Playing::from_course_with_standard(course, self.ctx.clone(), standard) {
            Ok(mut playing) => {
                playing.frame.reduced_motion = prefers_reduced_motion();
                playing.handle_input(&Input::Start, None);
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
                let mut playing = Playing::from_lti(self.ctx.clone(), player, lti, start_ms);
                playing.frame.reduced_motion = prefers_reduced_motion();
                playing.handle_input(&Input::Start, None);
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
                match Playing::standalone(course, self.ctx.clone()) {
                    Ok(mut playing) => {
                        playing.frame.reduced_motion = prefers_reduced_motion();
                        playing.handle_input(&Input::Start, None);
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
        let mut playing = Playing::from_connected(course, ctx.clone(), player, xapi, start_ms);
        playing.frame.reduced_motion = reduced_motion;
        playing.handle_input(&Input::Start, None);
        self.state = State::Playing(Box::new(playing));
    }
}

impl eframe::App for PlayerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
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
                egui::CentralPanel::default().show(ui, |ui| {
                    ui.centered_and_justified(|ui| {
                        ui.add(egui::Label::new(message).wrap());
                    });
                });
            }
            State::Playing(ref mut playing) => {
                // 5. Repaint continuously while playing so the clock advances
                ui.ctx().request_repaint();

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

                // 1. Advance ticker and handle tick input if emitted
                let dt = ui.input(|i| i.stable_dt);
                if let Some(tick) = playing.ticker.advance(dt) {
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

                if let Some(volume) = frame_output.volume {
                    playing.media.set_volume(volume);
                }
                if frame_output.fullscreen_toggle {
                    toggle_fullscreen(ui.ctx(), is_fullscreen(ui.ctx()));
                }

                #[cfg(target_arch = "wasm32")]
                if let Some(layer) = playing.a11y.as_mut() {
                    let settings = &playing.player.effective_player();
                    let started = playing.player.state().started;
                    let show_menu = settings.show_menu && started;
                    let show_nav = settings.show_nav && started;
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
                let dt = ui.input(|i| i.stable_dt);
                let dt_ms = if dt.is_finite() && dt > 0.0 {
                    (f64::from(dt) * 1000.0).round().clamp(0.0, u64::MAX as f64) as u64
                } else {
                    0
                };
                playing.poll_media_time(Some(ui.ctx()), dt_ms);
                #[cfg(target_arch = "wasm32")]
                playing.web_objects.layout(&playing.player, &playing.view);
            }
        }
    }
}
