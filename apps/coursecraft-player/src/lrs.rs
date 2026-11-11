//! Asynchronous xAPI / cmi5 transport for the web player (wasm only).
//!
//! [`JsLrsApi`] implements [`LrsApi`] over the browser: ordinary requests use
//! `ehttp`, while finish requests use the native `fetch` so the browser keeps
//! them alive across a page unload. The [`XapiHandle`] pairs the bridge with
//! the transport and the last player state, so the page-close handler can end
//! the session without reaching into the app.
//!
//! Nothing here logs a request, a header, a URL with a query or a token.
//! Warnings come from the bridge, which already redacts them.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use coursecraft_course::Course;
use coursecraft_lms::{LrsApi, LrsMethod, LrsRequest, LrsResponse, XapiBridge};
use coursecraft_logic::{Effect, PlayerState};
use eframe::wasm_bindgen::{JsCast as _, JsValue, closure::Closure};

/// Reference-counted, mutable handle to an xAPI bridge and its transport.
pub type XapiHandle = Rc<RefCell<XapiHandleInner>>;

/// The bridge, its transport and the last state it reported.
pub struct XapiHandleInner {
    /// Statement and state machine.
    pub bridge: XapiBridge,
    /// Browser transport.
    pub api: JsLrsApi,
    /// The most recent player state, used by the page-close handler.
    pub state: PlayerState,
    /// Session start timestamp in milliseconds, taken once play begins.
    pub start_ms: f64,
}

/// Browser transport for [`LrsApi`].
pub struct JsLrsApi {
    responses: Arc<Mutex<Vec<LrsResponse>>>,
    ctx: egui::Context,
}

impl JsLrsApi {
    /// Create a transport that repaints `ctx` when a response arrives.
    #[must_use]
    pub fn new(ctx: egui::Context) -> Self {
        Self {
            responses: Arc::new(Mutex::new(Vec::new())),
            ctx,
        }
    }

    fn send_fetch(&self, request: LrsRequest) {
        let id = request.id;
        let method = match request.method {
            LrsMethod::Get => ehttp::Method::GET,
            LrsMethod::Post => ehttp::Method::POST,
            LrsMethod::Put => ehttp::Method::PUT,
        };
        let headers = ehttp::Headers {
            headers: request.headers,
        };
        let mut fetch = ehttp::Request::new(method, request.url, headers);
        if let Some(body) = request.body {
            fetch.body = body.into_bytes();
        }

        let responses = Arc::clone(&self.responses);
        let ctx = self.ctx.clone();
        ehttp::fetch(fetch, move |result| {
            let response = match result {
                Ok(response) => LrsResponse {
                    id,
                    status: response.status,
                    body: String::from_utf8_lossy(&response.bytes).into_owned(),
                },
                Err(_) => LrsResponse {
                    id,
                    status: 0,
                    body: String::new(),
                },
            };
            let mut guard = responses
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            guard.push(response);
            ctx.request_repaint();
        });
    }
}

impl LrsApi for JsLrsApi {
    fn send(&mut self, request: LrsRequest) {
        if request.keepalive {
            send_keepalive(&request);
        } else {
            self.send_fetch(request);
        }
    }

    fn poll(&mut self) -> Vec<LrsResponse> {
        let mut guard = self
            .responses
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        std::mem::take(&mut *guard)
    }

    fn now_iso(&mut self) -> String {
        String::from(js_sys::Date::new_0().to_iso_string())
    }

    fn new_uuid(&mut self) -> String {
        let mut digits: Vec<char> = (0..32).map(|_| random_hex()).collect();
        if let Some(slot) = digits.get_mut(12) {
            *slot = '4';
        }
        let variant = 8 + random_below(4);
        if let Some(slot) = digits.get_mut(16) {
            *slot = char::from_digit(variant, 16).unwrap_or('8');
        }
        let mut out = String::with_capacity(36);
        for (index, digit) in digits.iter().enumerate() {
            if matches!(index, 8 | 12 | 16 | 20) {
                out.push('-');
            }
            out.push(*digit);
        }
        out
    }
}

fn random_below(bound: u32) -> u32 {
    let value = (js_sys::Math::random() * f64::from(bound)).floor();
    if !value.is_finite() || value <= 0.0 {
        0
    } else if value >= f64::from(bound) {
        bound.saturating_sub(1)
    } else {
        value as u32
    }
}

fn random_hex() -> char {
    char::from_digit(random_below(16), 16).unwrap_or('0')
}

/// Send a finish request through the browser `fetch` with `keepalive: true`.
///
/// Uses `Reflect` and `typeof` checks so a missing `fetch` or a cross-realm
/// function never throws. A rejected promise is swallowed.
fn send_keepalive(request: &LrsRequest) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let global: &JsValue = window.as_ref();
    let Ok(fetch) = js_sys::Reflect::get(global, &JsValue::from_str("fetch")) else {
        return;
    };
    if !fetch.is_function() {
        return;
    }
    let fetch: js_sys::Function = fetch.unchecked_into();

    let init = js_sys::Object::new();
    let method = match request.method {
        LrsMethod::Get => "GET",
        LrsMethod::Post => "POST",
        LrsMethod::Put => "PUT",
    };
    let _ = js_sys::Reflect::set(
        &init,
        &JsValue::from_str("method"),
        &JsValue::from_str(method),
    );

    let headers = js_sys::Object::new();
    for (name, value) in &request.headers {
        let _ = js_sys::Reflect::set(
            &headers,
            &JsValue::from_str(name),
            &JsValue::from_str(value),
        );
    }
    let _ = js_sys::Reflect::set(&init, &JsValue::from_str("headers"), &headers);
    if let Some(body) = &request.body {
        let _ = js_sys::Reflect::set(&init, &JsValue::from_str("body"), &JsValue::from_str(body));
    }
    let _ = js_sys::Reflect::set(&init, &JsValue::from_str("keepalive"), &JsValue::TRUE);

    let Ok(promise) = fetch.call2(global, &JsValue::from_str(&request.url), &init) else {
        return;
    };
    let promise: js_sys::Promise = promise.unchecked_into();
    wasm_bindgen_futures::spawn_local(async move {
        let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
    });
}

/// Current browser performance timestamp as whole milliseconds, saturating.
#[must_use]
pub fn now_ms_u64() -> u64 {
    let now = crate::lms::now_ms();
    if !now.is_finite() || now <= 0.0 {
        0
    } else if now >= u64::MAX as f64 {
        u64::MAX
    } else {
        now.floor() as u64
    }
}

/// Wrap a started bridge in a shared handle.
#[must_use]
pub fn new_handle(
    bridge: XapiBridge,
    api: JsLrsApi,
    state: PlayerState,
    start_ms: f64,
) -> XapiHandle {
    Rc::new(RefCell::new(XapiHandleInner {
        bridge,
        api,
        state,
        start_ms,
    }))
}

/// Forward effects and the new state to the bridge after one player input.
pub fn after_input(handle: &XapiHandle, course: &Course, state: &PlayerState, effects: &[Effect]) {
    let Ok(mut guard) = handle.try_borrow_mut() else {
        return;
    };
    let XapiHandleInner {
        bridge,
        api,
        state: last_state,
        start_ms,
    } = &mut *guard;
    *last_state = state.clone();
    let session_ms = crate::app::compute_session_ms(*start_ms, crate::lms::now_ms());
    let warnings = bridge.after_input(api, course, state, effects, session_ms, now_ms_u64());
    for warning in warnings {
        crate::lms::log_warn(&warning);
    }
}

/// Tick the bridge once per frame and log its warnings.
pub fn update(handle: &XapiHandle, course: &Course) {
    let Ok(mut guard) = handle.try_borrow_mut() else {
        return;
    };
    let XapiHandleInner { bridge, api, .. } = &mut *guard;
    let tick = bridge.tick(api, course, now_ms_u64());
    for warning in tick.warnings {
        crate::lms::log_warn(&warning);
    }
}

/// Finish the session from the stored state, used on page close.
pub fn finish_session(handle: &XapiHandle, course: &Course) {
    let Ok(mut guard) = handle.try_borrow_mut() else {
        return;
    };
    let XapiHandleInner {
        bridge,
        api,
        state,
        start_ms,
    } = &mut *guard;
    let session_ms = crate::app::compute_session_ms(*start_ms, crate::lms::now_ms());
    let warnings = bridge.finish_session(api, course, Some(state), session_ms);
    for warning in warnings {
        crate::lms::log_warn(&warning);
    }
}

/// The launch's safe `returnURL`, if it supplied one.
#[must_use]
pub fn return_url(handle: &XapiHandle) -> Option<String> {
    let guard = handle.try_borrow().ok()?;
    guard.bridge.return_url().map(str::to_owned)
}

/// Whether the launch attempted reporting that has since stopped for the
/// session, so the player should tell the learner.
#[must_use]
pub fn reporting_lost(handle: &XapiHandle) -> bool {
    handle
        .try_borrow()
        .map(|guard| guard.bridge.reporting_lost())
        .unwrap_or(false)
}

/// Register `pagehide` and `beforeunload` once, finishing the session on close.
pub fn register_page_close(handle: &XapiHandle, course: &'static Course) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let handle = Rc::clone(handle);
    let closure = Closure::<dyn FnMut(JsValue)>::wrap(Box::new(move |_event| {
        finish_session(&handle, course);
    }));
    let js_fn: &js_sys::Function = closure.as_ref().unchecked_ref();
    let _ = window.add_event_listener_with_callback("pagehide", js_fn);
    let _ = window.add_event_listener_with_callback("beforeunload", js_fn);
    // The listeners must live for the rest of the page; the browser tears them
    // down on navigation or close anyway.
    closure.forget();
}
