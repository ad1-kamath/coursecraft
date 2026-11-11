//! Web object iframes and the `RunJavaScript` host bridge (web player only).
//!
//! The iframe sits above the canvas exactly like a web video element: the
//! compositor lays it out from the last painted frame, and it is hidden during
//! transitions or when its object is not on screen.

use std::collections::{HashMap, HashSet};

use coursecraft_course::{Course, ObjectKind, Value, ValueKind, WebSource};
use coursecraft_logic::{Input, Player};
use coursecraft_render::{Hit, SlideView, Visual};
use eframe::wasm_bindgen::closure::Closure;
use eframe::wasm_bindgen::{JsCast as _, JsValue};

struct WebFrame {
    element: web_sys::HtmlElement,
    source: String,
}

/// One iframe per visible web object on the current slide.
#[derive(Default)]
pub struct WebObjects {
    frames: HashMap<String, WebFrame>,
    laid_out: HashSet<String>,
}

impl WebObjects {
    /// Create an empty frame set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Remove every iframe, for example when the player ends.
    pub fn clear(&mut self) {
        for (_, frame) in self.frames.drain() {
            remove(&frame.element);
        }
        self.laid_out.clear();
    }

    /// Position the current slide's web frames, creating or removing elements as
    /// objects appear, change source, or leave the slide.
    pub fn layout(&mut self, player: &Player<'_>, view: &SlideView) {
        // The dim backdrop and box draw over the page, so every iframe hides
        // while a lightbox is open.
        if player.lightbox_open() {
            for id in self.frames.keys().cloned().collect::<Vec<_>>() {
                if let Some(frame) = self.frames.get(&id) {
                    layout_frame(&frame.element, None);
                }
            }
            self.laid_out.clear();
            return;
        }
        let mut current: Vec<(String, String, String)> = Vec::new();
        if let Some(slide) = player.current_slide() {
            for object in slide
                .objects
                .iter()
                .chain(slide.layers.iter().flat_map(|layer| layer.objects.iter()))
            {
                if let ObjectKind::Web { source } = &object.kind {
                    let src = match source {
                        WebSource::Url { url } => url.clone(),
                        WebSource::Folder { path, entry } => {
                            format!("media/{path}/{entry}")
                        }
                    };
                    let title = if object.name.trim().is_empty() {
                        "Web object".to_owned()
                    } else {
                        object.name.clone()
                    };
                    current.push((object.id.clone(), src, title));
                }
            }
        }

        let current_ids: HashSet<String> = current.iter().map(|(id, _, _)| id.clone()).collect();
        let gone: Vec<String> = self
            .frames
            .keys()
            .filter(|id| !current_ids.contains(*id))
            .cloned()
            .collect();
        for id in gone {
            if let Some(frame) = self.frames.remove(&id) {
                remove(&frame.element);
            }
        }
        for id in self
            .laid_out
            .difference(&current_ids)
            .cloned()
            .collect::<Vec<_>>()
        {
            self.laid_out.remove(&id);
        }

        let transform = view.last_transform();
        let items = view.last_items();
        let transitioning = view.transition_active();
        for (id, src, title) in current {
            let needs_frame = self.frames.get(&id).is_none_or(|frame| frame.source != src);
            if needs_frame {
                if let Some(old) = self.frames.remove(&id) {
                    remove(&old.element);
                }
                if let Some(frame) = create_frame(&src, &title) {
                    self.frames.insert(id.clone(), frame);
                } else {
                    continue;
                }
            }
            let Some(frame) = self.frames.get(&id) else {
                continue;
            };
            let rect = transform.filter(|_| !transitioning).and_then(|t| {
                items.iter().find_map(|item| match &item.hit {
                    Hit::Object(object) if *object == id => match item.visual {
                        Visual::Web { .. } => {
                            let r = t.to_screen_rect(item.rect);
                            Some([r.min.x, r.min.y, r.width(), r.height()])
                        }
                        _ => None,
                    },
                    _ => None,
                })
            });
            layout_frame(&frame.element, rect);
            self.laid_out.insert(id);
        }
    }
}

impl Drop for WebObjects {
    fn drop(&mut self) {
        self.clear();
    }
}

fn create_frame(src: &str, title: &str) -> Option<WebFrame> {
    let document = web_sys::window()?.document()?;
    let element = document
        .create_element("iframe")
        .ok()?
        .dyn_into::<web_sys::HtmlElement>()
        .ok()?;
    let style = element.style();
    let _ = style.set_property("position", "absolute");
    let _ = style.set_property("z-index", "10");
    let _ = style.set_property("display", "none");
    let _ = style.set_property("border", "0");
    let _ = style.set_property("background", "#FFFFFF");
    let _ = element.set_attribute("src", src);
    let _ = element.set_attribute(
        "sandbox",
        "allow-scripts allow-same-origin allow-forms allow-popups",
    );
    let _ = element.set_attribute("referrerpolicy", "no-referrer");
    let _ = element.set_attribute("loading", "lazy");
    let _ = element.set_attribute("title", title);
    document.body()?.append_child(&element).ok()?;
    Some(WebFrame {
        element,
        source: src.to_owned(),
    })
}

fn layout_frame(element: &web_sys::HtmlElement, rect: Option<[f32; 4]>) {
    let style = element.style();
    match rect {
        Some([x, y, w, h]) => {
            let _ = style.set_property("display", "block");
            let _ = style.set_property("left", &format!("{x}px"));
            let _ = style.set_property("top", &format!("{y}px"));
            let _ = style.set_property("width", &format!("{w}px"));
            let _ = style.set_property("height", &format!("{h}px"));
        }
        None => {
            let _ = style.set_property("display", "none");
        }
    }
}

fn remove(element: &web_sys::HtmlElement) {
    if let Some(parent) = element.parent_node() {
        let _ = parent.remove_child(element);
    }
}

fn to_js(value: &Value) -> JsValue {
    match value {
        Value::Bool(value) => JsValue::from_bool(*value),
        Value::Number(value) => JsValue::from_f64(*value),
        Value::Text(value) => JsValue::from_str(value),
    }
}

fn set_helper(
    helper: &js_sys::Object,
    name: &str,
    function: &Closure<dyn FnMut(JsValue) -> JsValue>,
) {
    let _ = js_sys::Reflect::set(helper, &JsValue::from_str(name), function.as_ref());
}

fn set_helper2(
    helper: &js_sys::Object,
    name: &str,
    function: &Closure<dyn FnMut(JsValue, JsValue) -> JsValue>,
) {
    let _ = js_sys::Reflect::set(helper, &JsValue::from_str(name), function.as_ref());
}

/// Run author JavaScript with the `CourseCraft` helper and return the variable
/// writes and jumps it made.
///
/// Exceptions are logged and swallowed. Wrong-typed `setVar` calls are logged
/// and ignored. The caller feeds the returned inputs back into the player.
#[must_use]
pub fn run_javascript(player: &Player<'_>, code: &str) -> Vec<Input> {
    let course: &Course = player.course();
    let kinds: HashMap<String, ValueKind> = course
        .variables
        .iter()
        .map(|variable| (variable.name.clone(), variable.value.kind()))
        .collect();
    let mut values: HashMap<String, Value> = HashMap::new();
    for variable in &course.variables {
        if let Some(value) = player.variable(&variable.name) {
            values.insert(variable.name.clone(), value.clone());
        }
    }

    let inputs = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let values = std::rc::Rc::new(std::cell::RefCell::new(values));
    let helper = js_sys::Object::new();

    let values_get = std::rc::Rc::clone(&values);
    let get_var = Closure::<dyn FnMut(JsValue) -> JsValue>::new(move |name: JsValue| {
        let Some(name) = name.as_string() else {
            return JsValue::UNDEFINED;
        };
        values_get
            .borrow()
            .get(&name)
            .map_or(JsValue::UNDEFINED, to_js)
    });
    set_helper(&helper, "getVar", &get_var);

    let values_set = std::rc::Rc::clone(&values);
    let inputs_set = std::rc::Rc::clone(&inputs);
    let kinds_set = kinds.clone();
    let set_var = Closure::<dyn FnMut(JsValue, JsValue) -> JsValue>::new(
        move |name: JsValue, value: JsValue| {
            let Some(name) = name.as_string() else {
                warn("CourseCraft.setVar: the variable name must be a string");
                return JsValue::UNDEFINED;
            };
            let Some(kind) = kinds_set.get(&name) else {
                warn(&format!("CourseCraft.setVar: no variable named '{name}'"));
                return JsValue::UNDEFINED;
            };
            let value = match kind {
                ValueKind::Bool => value.as_bool().map(Value::Bool),
                ValueKind::Number => value
                    .as_f64()
                    .filter(|number| number.is_finite())
                    .map(Value::Number),
                ValueKind::Text => value.as_string().map(Value::Text),
            };
            let Some(value) = value else {
                warn(&format!(
                    "CourseCraft.setVar: '{name}' does not accept that value type"
                ));
                return JsValue::UNDEFINED;
            };
            values_set.borrow_mut().insert(name.clone(), value.clone());
            inputs_set.borrow_mut().push(Input::SetVariable {
                variable: name,
                value,
            });
            JsValue::UNDEFINED
        },
    );
    set_helper2(&helper, "setVar", &set_var);

    let inputs_jump = std::rc::Rc::clone(&inputs);
    let jump = Closure::<dyn FnMut(JsValue) -> JsValue>::new(move |slide: JsValue| {
        if let Some(slide) = slide.as_string() {
            inputs_jump.borrow_mut().push(Input::Jump { slide });
        } else {
            warn("CourseCraft.jump: the slide id must be a string");
        }
        JsValue::UNDEFINED
    });
    set_helper(&helper, "jump", &jump);

    // Build the function through `Reflect::construct` so a syntax error comes
    // back as an `Err` instead of throwing through the player.
    match compile(code) {
        Ok(function) => {
            if let Err(error) = function.call1(&JsValue::NULL, &helper) {
                warn(&format!("JavaScript failed: {error:?}"));
            }
        }
        Err(error) => warn(&format!("JavaScript has an error: {error:?}")),
    }

    drop(get_var);
    drop(set_var);
    drop(jump);
    let inputs = inputs.borrow();
    inputs.clone()
}

fn compile(code: &str) -> Result<js_sys::Function, JsValue> {
    let constructor = js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("Function"))?
        .dyn_into::<js_sys::Function>()?;
    let args = js_sys::Array::of2(&JsValue::from_str("CourseCraft"), &JsValue::from_str(code));
    js_sys::Reflect::construct(&constructor, &args)?.dyn_into::<js_sys::Function>()
}

fn warn(message: &str) {
    web_sys::console::warn_1(&JsValue::from_str(message));
}
