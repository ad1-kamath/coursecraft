//! SCORM 1.2 LMS bridge integration for the web player (wasm only).
//!
//! Discovers the SCORM RTE `window.API` object, provides a [`CmiApi`]
//! implementation over JavaScript via `js-sys` and `web-sys`, and manages
//! session lifecycle and suspend/resume data.

// Cross-realm note: the web player runs inside an LMS iframe while the SCORM
// `API` object (and `window.opener`) live in another window, i.e. another
// JavaScript realm. `dyn_into`/`dyn_ref` use `instanceof` against this realm's
// constructors, which is false for cross-realm objects. We therefore check with
// `typeof`-based tests (`is_object`/`is_function`, realm-independent) and then
// use `unchecked_into`. It is not `unsafe`, and the `typeof` checks keep it
// sound; the methods we call work structurally across realms.

use std::cell::RefCell;
use std::rc::Rc;

use coursecraft_course::Course;
use coursecraft_lms::{CmiApi, LmsBridge, Standard, StartPlan};
use coursecraft_logic::{Effect, HostInfo, Player, PlayerState};
use eframe::wasm_bindgen::{JsCast as _, JsValue, closure::Closure};

/// Reference-counted, mutable handle to an active LMS bridge and its API connection.
pub type LmsHandle = Rc<RefCell<(LmsBridge, JsCmiApi)>>;

#[derive(Debug, Clone, Copy)]
struct CallNames {
    initialize: &'static str,
    get_value: &'static str,
    set_value: &'static str,
    commit: &'static str,
    finish: &'static str,
    get_last_error: &'static str,
    get_error_string: &'static str,
}

fn call_names(standard: Standard) -> CallNames {
    match standard {
        Standard::Scorm12 => CallNames {
            initialize: "LMSInitialize",
            get_value: "LMSGetValue",
            set_value: "LMSSetValue",
            commit: "LMSCommit",
            finish: "LMSFinish",
            get_last_error: "LMSGetLastError",
            get_error_string: "LMSGetErrorString",
        },
        Standard::Scorm2004 => CallNames {
            initialize: "Initialize",
            get_value: "GetValue",
            set_value: "SetValue",
            commit: "Commit",
            finish: "Terminate",
            get_last_error: "GetLastError",
            get_error_string: "GetErrorString",
        },
    }
}

/// Wraps a JavaScript SCORM `API` object and implements [`CmiApi`].
pub struct JsCmiApi {
    /// Underlying JavaScript `API` object.
    pub api: js_sys::Object,
    names: CallNames,
}

impl std::fmt::Debug for JsCmiApi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JsCmiApi").finish_non_exhaustive()
    }
}

impl JsCmiApi {
    /// Create a new SCORM 1.2 [`JsCmiApi`] wrapping an `API` object.
    #[must_use]
    pub fn new(api: js_sys::Object) -> Self {
        Self::for_standard(api, Standard::Scorm12)
    }

    /// Create a [`JsCmiApi`] using the call names of `standard`.
    #[must_use]
    pub fn for_standard(api: js_sys::Object, standard: Standard) -> Self {
        Self {
            api,
            names: call_names(standard),
        }
    }

    fn get_fn(&self, name: &str) -> Option<js_sys::Function> {
        let prop = js_sys::Reflect::get(&self.api, &JsValue::from_str(name)).ok()?;
        if !prop.is_function() {
            return None;
        }
        Some(prop.unchecked_into::<js_sys::Function>())
    }
}

fn is_success(js: &JsValue) -> bool {
    if js.as_bool() == Some(true) {
        return true;
    }
    match js.as_string() {
        Some(s) => s.eq_ignore_ascii_case("true"),
        None => false,
    }
}

impl CmiApi for JsCmiApi {
    fn initialize(&mut self) -> bool {
        let Some(func) = self.get_fn(self.names.initialize) else {
            return false;
        };
        match func.call1(&self.api, &JsValue::from_str("")) {
            Ok(val) => is_success(&val),
            Err(_) => false,
        }
    }

    fn get_value(&mut self, key: &str) -> String {
        let Some(func) = self.get_fn(self.names.get_value) else {
            return String::new();
        };
        match func.call1(&self.api, &JsValue::from_str(key)) {
            Ok(val) => val.as_string().unwrap_or_default(),
            Err(_) => String::new(),
        }
    }

    fn set_value(&mut self, key: &str, value: &str) -> bool {
        let Some(func) = self.get_fn(self.names.set_value) else {
            return false;
        };
        match func.call2(
            &self.api,
            &JsValue::from_str(key),
            &JsValue::from_str(value),
        ) {
            Ok(val) => is_success(&val),
            Err(_) => false,
        }
    }

    fn commit(&mut self) -> bool {
        let Some(func) = self.get_fn(self.names.commit) else {
            return false;
        };
        match func.call1(&self.api, &JsValue::from_str("")) {
            Ok(val) => is_success(&val),
            Err(_) => false,
        }
    }

    fn finish(&mut self) -> bool {
        let Some(func) = self.get_fn(self.names.finish) else {
            return false;
        };
        match func.call1(&self.api, &JsValue::from_str("")) {
            Ok(val) => is_success(&val),
            Err(_) => false,
        }
    }

    fn last_error(&mut self) -> String {
        let code = match self
            .get_fn(self.names.get_last_error)
            .and_then(|f| f.call0(&self.api).ok())
        {
            Some(val) => match val.as_string() {
                Some(s) => s,
                None => match val.as_f64() {
                    Some(n) => format!("{n:.0}"),
                    None => String::new(),
                },
            },
            None => String::new(),
        };

        let desc = match self
            .get_fn(self.names.get_error_string)
            .and_then(|f| f.call1(&self.api, &JsValue::from_str(&code)).ok())
        {
            Some(val) => val.as_string().unwrap_or_default(),
            None => String::new(),
        };

        match (code.is_empty(), desc.is_empty()) {
            (false, false) => format!("{code}: {desc}"),
            (false, true) => code,
            (true, false) => desc,
            (true, true) => "unknown error".to_string(),
        }
    }
}

fn api_property(standard: Standard) -> &'static str {
    match standard {
        Standard::Scorm12 => "API",
        Standard::Scorm2004 => "API_1484_11",
    }
}

fn check_api_property(target: &JsValue, standard: Standard) -> Option<js_sys::Object> {
    let api_val = js_sys::Reflect::get(target, &JsValue::from_str(api_property(standard))).ok()?;
    if !api_val.is_object() {
        return None;
    }
    let init_fn = js_sys::Reflect::get(
        &api_val,
        &JsValue::from_str(call_names(standard).initialize),
    )
    .ok()?;
    if !init_fn.is_function() {
        return None;
    }
    Some(api_val.unchecked_into::<js_sys::Object>())
}

fn walk_window_parents(start: web_sys::Window, standard: Standard) -> Option<js_sys::Object> {
    if let Some(api) = check_api_property(start.as_ref(), standard) {
        return Some(api);
    }
    let mut current = start;
    for _ in 0..7 {
        let parent = match current.parent() {
            Ok(Some(p)) => p,
            _ => break,
        };
        if AsRef::<JsValue>::as_ref(&parent) == AsRef::<JsValue>::as_ref(&current) {
            break;
        }
        if let Some(api) = check_api_property(parent.as_ref(), standard) {
            return Some(api);
        }
        current = parent;
    }
    None
}

/// Find the standard's `API` object by searching `window`, its parents up to 7
/// levels, and `window.opener` and its parents. SCORM 2004 looks for
/// `API_1484_11`; SCORM 1.2 looks for `API`.
///
/// Returns [`None`] if no conforming `API` object is found.
#[must_use]
pub fn find_api(standard: Standard) -> Option<js_sys::Object> {
    let win = web_sys::window()?;
    if let Some(api) = walk_window_parents(win.clone(), standard) {
        return Some(api);
    }
    let opener_val = win.opener().ok()?;
    if !opener_val.is_object() {
        return None;
    }
    let opener_win = opener_val.unchecked_into::<web_sys::Window>();
    walk_window_parents(opener_win, standard)
}

/// Read the current browser performance timestamp in milliseconds.
#[must_use]
pub fn now_ms() -> f64 {
    match web_sys::window().and_then(|w| w.performance()) {
        Some(p) => p.now(),
        None => 0.0,
    }
}

/// Log a warning to the browser console prefixed with `coursecraft lms: `.
pub fn log_warn(message: &str) {
    let line = format!("coursecraft lms: {message}");
    web_sys::console::warn_1(&JsValue::from_str(&line));
}

/// Log the standalone playback status message once.
pub fn log_standalone() {
    web_sys::console::log_1(&JsValue::from_str(
        "coursecraft: no SCORM API found; playing standalone",
    ));
}

/// A random draw seed for a fresh start, from `js_sys::Math::random`.
fn fresh_seed() -> u32 {
    let value = (js_sys::Math::random() * 4294967296.0).floor();
    if !value.is_finite() || value <= 0.0 {
        0
    } else if value >= 4294967296.0 {
        u32::MAX
    } else {
        value as u32
    }
}

/// A fresh player with a random draw seed. Resuming keeps the saved seed via
/// [`Player::resume`].
pub(crate) fn fresh_player(course: &'static Course) -> Result<Player<'static>, String> {
    Player::with_draw_seed(course, fresh_seed()).map_err(crate::app::format_issues)
}

/// Today's UTC date from the browser clock, as `YYYY-MM-DD`.
pub(crate) fn today_date() -> String {
    let now_ms = js_sys::Date::now();
    if !now_ms.is_finite() {
        return String::new();
    }
    let secs = (now_ms / 1000.0).floor();
    if secs < i64::MIN as f64 || secs > i64::MAX as f64 {
        return String::new();
    }
    coursecraft_logic::civil_date(secs as i64)
}

/// Initialize the player and optional LMS connection for a course.
///
/// # Errors
/// Returns joined issue descriptions if course validation or player creation fails.
pub fn init_player(
    course: &'static Course,
    standard: Standard,
) -> Result<(Player<'static>, Option<LmsHandle>, f64), String> {
    let start_ms = now_ms();
    let Some(api_obj) = find_api(standard) else {
        log_standalone();
        let mut player = fresh_player(course)?;
        player.set_host_info(HostInfo {
            learner_name: None,
            date: Some(today_date()),
        });
        return Ok((player, None, start_ms));
    };

    let mut bridge = LmsBridge::for_standard(standard);
    let mut api = JsCmiApi::for_standard(api_obj, standard);
    let (plan, warnings) = bridge.start(&mut api, course);
    for w in warnings {
        log_warn(&w);
    }

    let StartPlan {
        resume,
        learner_name,
    } = plan;
    let mut player = match resume {
        Some(state) => match Player::resume(course, state) {
            Ok(p) => p,
            Err(issues) => {
                let msg = crate::app::format_issues(issues);
                log_warn(&format!("Player::resume failed: {msg}"));
                fresh_player(course)?
            }
        },
        None => fresh_player(course)?,
    };
    player.set_host_info(HostInfo {
        learner_name,
        date: Some(today_date()),
    });

    let lms = Rc::new(RefCell::new((bridge, api)));
    register_page_close(&lms, start_ms);

    Ok((player, Some(lms), start_ms))
}

/// Forward effects and state to the LMS bridge after an input is handled.
pub fn after_input(
    lms: &LmsHandle,
    course: &Course,
    state: &PlayerState,
    effects: &[Effect],
    start_ms: f64,
) {
    if let Ok(mut guard) = lms.try_borrow_mut() {
        let (bridge, api) = &mut *guard;
        let session_ms = crate::app::compute_session_ms(start_ms, now_ms());
        let warnings = bridge.after_input(api, course, state, effects, session_ms);
        for w in warnings {
            log_warn(&w);
        }
    }
}

fn register_page_close(lms: &LmsHandle, start_ms: f64) {
    let Some(win) = web_sys::window() else {
        return;
    };
    let lms_clone = Rc::clone(lms);
    let closure = Closure::<dyn FnMut(JsValue)>::wrap(Box::new(move |_event| {
        if let Ok(mut guard) = lms_clone.try_borrow_mut() {
            let (bridge, api) = &mut *guard;
            let session_ms = crate::app::compute_session_ms(start_ms, now_ms());
            let warnings = bridge.finish_session(api, session_ms);
            for w in warnings {
                log_warn(&w);
            }
        }
    }));
    let js_fn: &js_sys::Function = closure.as_ref().unchecked_ref();
    let _ = win.add_event_listener_with_callback("pagehide", js_fn);
    let _ = win.add_event_listener_with_callback("beforeunload", js_fn);
    // Leaking the closure via `Closure::forget` is intentional: the event listeners
    // must remain active for the remaining lifetime of the page, which the browser
    // tears down on navigation or close anyway.
    closure.forget();
}
