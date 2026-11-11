//! Hidden ARIA DOM layer for the web player.
//!
//! A canvas is invisible to a screen reader. This layer keeps a real HTML
//! element for every item in the pure [`AccessTree`], positioned over the same
//! place the canvas paints it. It is visually transparent and never takes a
//! pointer, but it owns native focus so `Tab`, `Shift+Tab` and screen-reader
//! navigation work as they do on any other page.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use coursecraft_logic::Player;
use coursecraft_render::{
    AccessNode, AccessRole, AccessTree, DrawItem, ExternalFocus, FocusManager, FocusTarget,
    PlayerFrame, SlideTransform, announcement, build_access_tree,
};
use eframe::wasm_bindgen::{JsCast as _, JsValue, closure::Closure};
use web_sys::{
    Element, Event, FocusEvent, HtmlCanvasElement, HtmlElement, HtmlInputElement,
    HtmlOptionElement, HtmlSelectElement, KeyboardEvent,
};

const LAYER_ID: &str = "coursecraft_a11y";
const TRANSPARENT_IMAGE: &str =
    "data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///ywAAAAAAQABAAACAUwAOw==";
const CANVAS_ID: &str = "coursecraft_canvas";

type RoleMap = BTreeMap<String, (AccessRole, Option<FocusTarget>)>;

/// The hidden DOM layer that mirrors the player's canvas.
pub struct AccessLayer {
    ctx: egui::Context,
    document: web_sys::Document,
    root: HtmlElement,
    /// Managed children live here, below the live region.
    items: Element,
    live: Element,
    canvas: Option<HtmlCanvasElement>,
    text_agent: Option<HtmlInputElement>,
    elements: BTreeMap<String, Element>,
    roles: Rc<RefCell<RoleMap>>,
    events: Rc<RefCell<Vec<ExternalFocus>>>,
    drag_held: Rc<Cell<bool>>,
    previous: Option<AccessTree>,
    last_focused: Option<FocusTarget>,
    /// Our own sync removed the focused element. Put focus back once the new
    /// target is known, or a keyboard or screen-reader learner lands on `body`.
    restore_focus: bool,
    /// True while `reconcile` runs, so removing a focused element isn't read as
    /// the learner leaving the course.
    reconciling: Rc<Cell<bool>>,
    last_transform: Option<SlideTransform>,
    listeners: Vec<Closure<dyn FnMut(Event)>>,
}

impl AccessLayer {
    /// Create and attach the layer, if a document is available.
    #[must_use]
    pub fn new(ctx: egui::Context, title: &str) -> Option<Self> {
        let window = web_sys::window()?;
        let document = window.document()?;
        let body = document.body()?;
        let root = document
            .create_element("main")
            .ok()?
            .dyn_into::<HtmlElement>()
            .ok()?;
        let _ = root.set_attribute("id", LAYER_ID);
        let _ = root.set_attribute("aria-label", title);
        let style = root.style();
        let _ = style.set_property("position", "absolute");
        let _ = style.set_property("left", "0");
        let _ = style.set_property("top", "0");
        let _ = style.set_property("width", "100%");
        let _ = style.set_property("height", "100%");
        let _ = style.set_property("overflow", "hidden");
        let _ = style.set_property("opacity", "0");
        let _ = style.set_property("pointer-events", "none");
        let _ = style.set_property("z-index", "5");

        let items = document.create_element("div").ok()?;
        if let Some(item_style) = element_style(&items) {
            let _ = item_style.set_property("position", "absolute");
            let _ = item_style.set_property("left", "0");
            let _ = item_style.set_property("top", "0");
            let _ = item_style.set_property("width", "100%");
            let _ = item_style.set_property("height", "100%");
            let _ = item_style.set_property("pointer-events", "none");
        }

        let live = document.create_element("div").ok()?;
        let _ = live.set_attribute("aria-live", "polite");
        let _ = live.set_attribute("aria-atomic", "true");
        if let Some(live_style) = element_style(&live) {
            let _ = live_style.set_property("position", "absolute");
            let _ = live_style.set_property("left", "0");
            let _ = live_style.set_property("top", "0");
            let _ = live_style.set_property("width", "1px");
            let _ = live_style.set_property("height", "1px");
            let _ = live_style.set_property("overflow", "hidden");
        }

        root.append_child(&live).ok()?;
        root.append_child(&items).ok()?;
        body.append_child(&root).ok()?;

        document.set_title(title);
        if let Some(html) = document.document_element() {
            let _ = html.set_attribute("lang", "en");
        }

        let canvas = document
            .get_element_by_id(CANVAS_ID)
            .and_then(|element| element.dyn_into::<HtmlCanvasElement>().ok());
        if let Some(canvas) = &canvas {
            hide_from_assistive_tech(canvas);
        }

        let roles = Rc::new(RefCell::new(BTreeMap::new()));
        let events = Rc::new(RefCell::new(Vec::new()));
        let drag_held = Rc::new(Cell::new(false));
        let mut layer = Self {
            ctx,
            document,
            root,
            items,
            live,
            canvas,
            text_agent: None,
            elements: BTreeMap::new(),
            roles,
            events,
            drag_held,
            previous: None,
            last_focused: None,
            restore_focus: false,
            reconciling: Rc::new(Cell::new(false)),
            last_transform: None,
            listeners: Vec::new(),
        };
        layer.install_listeners();
        layer.keep_eframe_text_agent_out();
        Some(layer)
    }

    /// Drain DOM events into the focus manager before the frame runs.
    pub fn before_frame(&self, focus: &mut FocusManager) {
        let drained = match self.events.try_borrow_mut() {
            Ok(mut queue) => std::mem::take(&mut *queue),
            Err(_) => return,
        };
        if drained.is_empty() {
            return;
        }
        for event in drained {
            focus.external(event);
        }
        self.ctx.request_repaint();
    }

    /// Announce a message that is not derived from a tree change.
    pub fn announce(&self, text: &str) {
        self.set_live(text);
    }

    /// Synchronise the DOM with the frame that was just painted.
    pub fn after_frame(
        &mut self,
        player: &Player<'_>,
        frame: &PlayerFrame,
        items: &[DrawItem],
        transform: Option<SlideTransform>,
        show_nav: bool,
        show_menu: bool,
    ) {
        self.keep_eframe_text_agent_out();
        if let Some(html) = self.document.document_element() {
            let lang = player.course().language.as_deref().unwrap_or("en");
            if html.get_attribute("lang").as_deref() != Some(lang) {
                let _ = html.set_attribute("lang", lang);
            }
        }
        let tree = build_access_tree(player, frame, items, show_nav, show_menu);
        let dom_owned = self.dom_owns_focus();
        let in_layer = self.focus_in_layer();
        let changed = self.previous.as_ref() != Some(&tree);
        let layout_changed = self.last_transform != transform;
        if changed || layout_changed {
            self.reconciling.set(true);
            self.reconcile(&tree, transform);
            self.reconciling.set(false);
            self.last_transform = transform;
        }
        if in_layer && !self.focus_in_layer() {
            self.restore_focus = true;
        }
        if changed {
            if let Some(text) = announcement(self.previous.as_ref(), &tree, player) {
                self.set_live(&text);
            }
            self.previous = Some(tree);
        }
        self.push_focus(frame.focus(), dom_owned);
    }

    fn focus_in_layer(&self) -> bool {
        self.document
            .active_element()
            .is_some_and(|active| self.root.contains(Some(&active)))
    }

    fn dom_owns_focus(&self) -> bool {
        let active = self.document.active_element();
        let in_layer = active
            .as_ref()
            .is_some_and(|active| self.root.contains(Some(active)));
        let on_canvas = active.as_ref().is_some_and(|active| {
            self.canvas
                .as_ref()
                .is_some_and(|canvas| canvas.is_same_node(Some(active)))
                || self
                    .text_agent
                    .as_ref()
                    .is_some_and(|input| input.is_same_node(Some(active)))
        });
        in_layer || on_canvas
    }

    fn set_live(&self, text: &str) {
        self.live.set_text_content(None);
        self.live.set_text_content(Some(text));
    }

    fn install_listeners(&mut self) {
        let Some(target) = self.root.clone().dyn_into::<web_sys::EventTarget>().ok() else {
            return;
        };

        let events = Rc::clone(&self.events);
        let roles = Rc::clone(&self.roles);
        let focus_in = Closure::<dyn FnMut(Event)>::new(move |event: Event| {
            let Some(element) = event
                .target()
                .and_then(|target| target.dyn_into::<Element>().ok())
            else {
                return;
            };
            let key = element.get_attribute("data-cc-key").unwrap_or_default();
            let target = roles
                .try_borrow()
                .ok()
                .and_then(|map| map.get(&key).cloned())
                .and_then(|(_, target)| target);
            if let Some(target) = target
                && let Ok(mut queue) = events.try_borrow_mut()
            {
                queue.push(ExternalFocus::Focused(target));
            }
        });
        add(&target, "focusin", focus_in, &mut self.listeners);

        let events = Rc::clone(&self.events);
        let root = self.root.clone();
        let reconciling = Rc::clone(&self.reconciling);
        let focus_out = Closure::<dyn FnMut(Event)>::new(move |event: Event| {
            if reconciling.get() {
                return;
            }
            let related = event
                .dyn_into::<FocusEvent>()
                .ok()
                .and_then(|event| event.related_target())
                .and_then(|target| target.dyn_into::<web_sys::Node>().ok());
            let stays_inside = related
                .as_ref()
                .is_some_and(|node| root.contains(Some(node)));
            if !stays_inside && let Ok(mut queue) = events.try_borrow_mut() {
                queue.push(ExternalFocus::Blurred);
            }
        });
        add(&target, "focusout", focus_out, &mut self.listeners);

        let events = Rc::clone(&self.events);
        let roles = Rc::clone(&self.roles);
        let click = Closure::<dyn FnMut(Event)>::new(move |event: Event| {
            let Some(element) = event
                .target()
                .and_then(|target| target.dyn_into::<Element>().ok())
            else {
                return;
            };
            let key = element.get_attribute("data-cc-key").unwrap_or_default();
            let Some((role, target)) = roles
                .try_borrow()
                .ok()
                .and_then(|map| map.get(&key).cloned())
            else {
                return;
            };
            if matches!(
                role,
                AccessRole::Button
                    | AccessRole::Radio
                    | AccessRole::Checkbox
                    | AccessRole::RadioGroup
                    | AccessRole::Link
                    | AccessRole::Tab
                    | AccessRole::ListItem
            ) && let Some(target) = target
                && let Ok(mut queue) = events.try_borrow_mut()
            {
                queue.push(ExternalFocus::Activated(target));
            }
        });
        add(&target, "click", click, &mut self.listeners);

        let events = Rc::clone(&self.events);
        let roles = Rc::clone(&self.roles);
        let input = Closure::<dyn FnMut(Event)>::new(move |event: Event| {
            let Some(element) = event
                .target()
                .and_then(|target| target.dyn_into::<Element>().ok())
            else {
                return;
            };
            let key = element.get_attribute("data-cc-key").unwrap_or_default();
            let role = roles
                .try_borrow()
                .ok()
                .and_then(|map| map.get(&key).map(|(role, _)| *role));
            let value = match role {
                Some(AccessRole::TextBox) => element
                    .clone()
                    .dyn_into::<HtmlInputElement>()
                    .ok()
                    .map(|input| input.value()),
                Some(AccessRole::TextArea) => element_value(&element),
                _ => None,
            };
            if let Some(value) = value
                && let Ok(mut queue) = events.try_borrow_mut()
            {
                queue.push(ExternalFocus::Text(value));
            }
        });
        add(&target, "input", input, &mut self.listeners);

        let events = Rc::clone(&self.events);
        let roles = Rc::clone(&self.roles);
        let change = Closure::<dyn FnMut(Event)>::new(move |event: Event| {
            let Some(element) = event
                .target()
                .and_then(|target| target.dyn_into::<Element>().ok())
            else {
                return;
            };
            let key = element.get_attribute("data-cc-key").unwrap_or_default();
            let Some((role, target)) = roles
                .try_borrow()
                .ok()
                .and_then(|map| map.get(&key).cloned())
            else {
                return;
            };
            if role != AccessRole::Select {
                return;
            }
            let Some(target) = target else {
                return;
            };
            let FocusTarget::MatchRow(left) = target else {
                return;
            };
            let Some(select) = element.dyn_into::<HtmlSelectElement>().ok() else {
                return;
            };
            let value = select.value();
            let right = (!value.is_empty()).then_some(value);
            if let Ok(mut queue) = events.try_borrow_mut() {
                queue.push(ExternalFocus::Match { left, right });
            }
        });
        add(&target, "change", change, &mut self.listeners);

        let events = Rc::clone(&self.events);
        let roles = Rc::clone(&self.roles);
        let drag_held = Rc::clone(&self.drag_held);
        let keydown = Closure::<dyn FnMut(Event)>::new(move |event: Event| {
            let Some(keyboard) = event.dyn_into::<KeyboardEvent>().ok() else {
                return;
            };
            let Some(element) = keyboard
                .target()
                .and_then(|target| target.dyn_into::<Element>().ok())
            else {
                return;
            };
            let key = element.get_attribute("data-cc-key").unwrap_or_default();
            let Some((role, target)) = roles
                .try_borrow()
                .ok()
                .and_then(|map| map.get(&key).cloned())
            else {
                return;
            };
            let Some(target) = target else {
                return;
            };
            let Some(mapped) = map_key(&keyboard.key()) else {
                return;
            };
            let hotspot_or_drag = matches!(target, FocusTarget::Hotspot | FocusTarget::DragItem(_));
            let control_key = matches!(
                mapped,
                egui::Key::ArrowUp
                    | egui::Key::ArrowDown
                    | egui::Key::ArrowLeft
                    | egui::Key::ArrowRight
                    | egui::Key::Enter
                    | egui::Key::Space
                    | egui::Key::Escape
            ) || (mapped == egui::Key::Tab && drag_held.get());
            // A native select opens a popup on arrows on macOS, and that popup hangs
            // off an invisible control. Up and down cycle the match the same way
            // everywhere, and the canvas shows the result.
            let match_key = matches!(target, FocusTarget::MatchRow(_))
                && matches!(mapped, egui::Key::ArrowUp | egui::Key::ArrowDown);
            let chrome_key = !matches!(
                role,
                AccessRole::TextBox | AccessRole::TextArea | AccessRole::Select
            ) && matches!(
                mapped,
                egui::Key::ArrowLeft
                    | egui::Key::ArrowRight
                    | egui::Key::Home
                    | egui::Key::End
                    | egui::Key::M
                    | egui::Key::C
                    | egui::Key::Escape
            );
            let text_key = role == AccessRole::TextBox && mapped == egui::Key::Enter;
            let input_key = matches!(
                role,
                AccessRole::Slider | AccessRole::Checkbox | AccessRole::RadioGroup
            ) && matches!(
                mapped,
                egui::Key::ArrowUp
                    | egui::Key::ArrowDown
                    | egui::Key::ArrowLeft
                    | egui::Key::ArrowRight
                    | egui::Key::Home
                    | egui::Key::End
                    | egui::Key::Enter
                    | egui::Key::Space
            );
            if !(hotspot_or_drag && control_key)
                && !match_key
                && !chrome_key
                && !text_key
                && !input_key
            {
                return;
            }
            keyboard.prevent_default();
            if let Ok(mut queue) = events.try_borrow_mut() {
                queue.push(ExternalFocus::Key {
                    key: mapped,
                    shift: keyboard.shift_key(),
                });
            }
        });
        add(&target, "keydown", keydown, &mut self.listeners);
    }

    fn keep_eframe_text_agent_out(&mut self) {
        if let Some(canvas) = &self.canvas {
            hide_from_assistive_tech(canvas);
        }
        self.text_agent = self.canvas.as_ref().and_then(find_text_agent).or_else(|| {
            self.document
                .query_selector("input[autocapitalize='off']")
                .ok()
                .flatten()
                .and_then(|element| element.dyn_into::<HtmlInputElement>().ok())
        });
        if let Some(input) = &self.text_agent {
            hide_from_assistive_tech(input);
        }
    }

    fn reconcile(&mut self, tree: &AccessTree, transform: Option<SlideTransform>) {
        let mut roles = BTreeMap::new();
        let mut desired = BTreeSet::new();
        for node in &tree.nodes {
            desired.insert(node.key.clone());
            let element = match self.elements.get(&node.key).cloned() {
                Some(element) => element,
                None => {
                    let Some(element) = self.create_element(node) else {
                        continue;
                    };
                    let parent = self.parent_element(tree, node);
                    let _ = parent.append_child(&element);
                    self.elements.insert(node.key.clone(), element.clone());
                    element
                }
            };
            let _ = element.set_attribute("data-cc-key", &node.key);
            self.update_element(&element, node);
            apply_rect(&element, node, transform);
            roles.insert(node.key.clone(), (node.role, node.target.clone()));
        }

        self.reorder(tree);
        self.remove_extra(&desired);
        self.elements.retain(|key, _| desired.contains(key));
        if let Ok(mut map) = self.roles.try_borrow_mut() {
            *map = roles;
        }
        let held = tree.nodes.iter().any(|node| node.pressed == Some(true));
        self.drag_held.set(held);
    }

    /// The parent a new node belongs under: rows inside their table, cells
    /// inside their row, everything else in the flat item list.
    fn parent_element(&self, tree: &AccessTree, node: &AccessNode) -> Element {
        let before = tree
            .nodes
            .iter()
            .position(|candidate| candidate.key == node.key)
            .unwrap_or(tree.nodes.len());
        let earlier = tree.nodes.get(..before).unwrap_or(&[]);
        let parent_key = match node.role {
            AccessRole::Row => earlier
                .iter()
                .rev()
                .find(|candidate| candidate.role == AccessRole::Table)
                .map(|candidate| candidate.key.as_str()),
            AccessRole::Cell | AccessRole::ColumnHeader => earlier
                .iter()
                .rev()
                .find(|candidate| candidate.role == AccessRole::Row)
                .map(|candidate| candidate.key.as_str()),
            _ => None,
        };
        parent_key
            .and_then(|key| self.elements.get(key).cloned())
            .unwrap_or_else(|| self.items.clone())
    }

    fn create_element(&self, node: &AccessNode) -> Option<Element> {
        let name = match node.role {
            AccessRole::Heading => "h1",
            AccessRole::Text => "p",
            AccessRole::Image => "img",
            AccessRole::Button | AccessRole::Tab => "button",
            // Input sliders and dials get a native range control; the player's
            // own seek and volume sliders stay buttons.
            AccessRole::Slider => {
                if node.options.iter().any(|(name, _)| name == "min") {
                    "input"
                } else {
                    "button"
                }
            }
            AccessRole::Link => "a",
            AccessRole::Radio | AccessRole::Checkbox | AccessRole::TextBox => "input",
            AccessRole::TextArea => "textarea",
            AccessRole::RadioGroup => "fieldset",
            AccessRole::Select => "select",
            AccessRole::Table => "table",
            AccessRole::Row => "tr",
            AccessRole::Cell => "td",
            AccessRole::ColumnHeader => "th",
            AccessRole::Dialog | AccessRole::ListItem => "div",
            AccessRole::Timer => "div",
            AccessRole::ProgressBar => "div",
        };
        let element = self.document.create_element(name).ok()?;
        match node.role {
            AccessRole::Image => {
                let _ = element.set_attribute("src", TRANSPARENT_IMAGE);
            }
            AccessRole::Dialog => {
                let _ = element.set_attribute("role", "dialog");
                let _ = element.set_attribute("aria-modal", "true");
                let _ = element.set_attribute("aria-label", &node.label);
            }
            AccessRole::Timer => {
                let _ = element.set_attribute("role", "timer");
                let _ = element.set_attribute("aria-label", &node.label);
            }
            AccessRole::ProgressBar => {
                let _ = element.set_attribute("role", "progressbar");
                let _ = element.set_attribute("aria-label", &node.label);
            }
            AccessRole::Link => {
                let _ = element.set_attribute("role", "link");
            }
            AccessRole::Button => {
                let _ = element.set_attribute("type", "button");
            }
            AccessRole::Tab => {
                let _ = element.set_attribute("type", "button");
                let _ = element.set_attribute("role", "tab");
            }
            AccessRole::ListItem => {
                let _ = element.set_attribute("role", "listitem");
                let _ = element.set_attribute("tabindex", "0");
            }
            AccessRole::Slider => {
                let _ = element.set_attribute("role", "slider");
                if node.options.iter().any(|(name, _)| name == "min") {
                    let _ = element.set_attribute("type", "range");
                } else {
                    let _ = element.set_attribute("type", "button");
                }
            }
            AccessRole::Radio => {
                let _ = element.set_attribute("type", "radio");
            }
            AccessRole::Checkbox => {
                let _ = element.set_attribute("type", "checkbox");
            }
            AccessRole::TextBox => {
                let _ = element.set_attribute("type", "text");
            }
            AccessRole::TextArea => {}
            AccessRole::RadioGroup => {
                let _ = element.set_attribute("role", "radiogroup");
            }
            AccessRole::Table => {
                let _ = element.set_attribute("role", "table");
            }
            AccessRole::Row => {
                let _ = element.set_attribute("role", "row");
            }
            AccessRole::ColumnHeader => {
                let _ = element.set_attribute("scope", "col");
            }
            AccessRole::Heading | AccessRole::Text | AccessRole::Select | AccessRole::Cell => {}
        }
        Some(element)
    }

    fn update_element(&self, element: &Element, node: &AccessNode) {
        match node.role {
            AccessRole::Heading | AccessRole::Text | AccessRole::Button | AccessRole::Link => {
                element.set_text_content(Some(&node.label));
            }
            AccessRole::Tab => {
                element.set_text_content(Some(&node.label));
                let selected = if node.pressed == Some(true) {
                    "true"
                } else {
                    "false"
                };
                let _ = element.set_attribute("aria-selected", selected);
            }
            AccessRole::ListItem => {
                element.set_text_content(Some(&node.label));
                if let Some(definition) = &node.value {
                    let _ = element.set_attribute("aria-description", definition);
                } else {
                    let _ = element.remove_attribute("aria-description");
                }
            }
            AccessRole::Slider => {
                let is_range = element.dyn_ref::<HtmlInputElement>().is_some();
                if is_range {
                    let _ = element.set_attribute("aria-label", &node.label);
                } else {
                    element.set_text_content(Some(&node.label));
                }
                if let Some(value) = &node.value {
                    let _ = element.set_attribute("aria-valuetext", value);
                    for (name, number) in &node.options {
                        let attribute = match name.as_str() {
                            "min" => "aria-valuemin",
                            "max" => "aria-valuemax",
                            "now" => "aria-valuenow",
                            _ => continue,
                        };
                        let _ = element.set_attribute(attribute, number);
                    }
                    if is_range {
                        let number = |wanted: &str| {
                            node.options
                                .iter()
                                .find(|(name, _)| name == wanted)
                                .map(|(_, number)| number.as_str())
                        };
                        let _ = element.set_attribute("step", "any");
                        for (attribute, wanted) in [("min", "min"), ("max", "max")] {
                            if let Some(number) = number(wanted) {
                                let _ = element.set_attribute(attribute, number);
                            }
                        }
                        if let Some(now) = number("now") {
                            set_element_value(element, now);
                        }
                    }
                }
            }
            AccessRole::Image => {
                if let Some(image) = element.dyn_ref::<web_sys::HtmlImageElement>() {
                    image.set_alt(&node.label);
                } else {
                    let _ = element.set_attribute("alt", &node.label);
                }
            }
            AccessRole::Radio | AccessRole::Checkbox => {
                let _ = element.set_attribute("aria-label", &node.label);
                if let Some(input) = element.dyn_ref::<HtmlInputElement>() {
                    input.set_checked(node.checked.unwrap_or(false));
                }
            }
            AccessRole::TextBox => {
                let _ = element.set_attribute("aria-label", &node.label);
                if let Some(input) = element.dyn_ref::<HtmlInputElement>() {
                    let value = node.value.as_deref().unwrap_or_default();
                    if input.value() != value {
                        input.set_value(value);
                    }
                }
            }
            AccessRole::TextArea => {
                let _ = element.set_attribute("aria-label", &node.label);
                let value = node.value.as_deref().unwrap_or_default();
                if element_value(element).as_deref() != Some(value) {
                    set_element_value(element, value);
                }
            }
            AccessRole::RadioGroup => {
                let _ = element.set_attribute("aria-label", &node.label);
                if let Some(value) = &node.value {
                    let _ = element.set_attribute("data-cc-value", value);
                }
            }
            AccessRole::Select => {
                let _ = element.set_attribute("aria-label", &node.label);
                self.update_select(element, node);
            }
            AccessRole::Cell | AccessRole::ColumnHeader => {
                element.set_text_content(Some(&node.label));
            }
            AccessRole::Dialog => {
                let _ = element.set_attribute("aria-label", &node.label);
            }
            AccessRole::Timer => {
                element.set_text_content(Some(node.value.as_deref().unwrap_or_default()));
            }
            AccessRole::ProgressBar => {
                let _ = element.set_attribute("aria-label", &node.label);
                if let Some(value) = &node.value {
                    let _ = element.set_attribute("aria-valuetext", value);
                }
                for (name, number) in &node.options {
                    let attribute = match name.as_str() {
                        "min" => "aria-valuemin",
                        "max" => "aria-valuemax",
                        "now" => "aria-valuenow",
                        _ => continue,
                    };
                    let _ = element.set_attribute(attribute, number);
                }
            }
            AccessRole::Table | AccessRole::Row => {}
        }
        if let Some(pressed) = node.pressed {
            let _ = element.set_attribute("aria-pressed", if pressed { "true" } else { "false" });
        } else {
            let _ = element.remove_attribute("aria-pressed");
        }
        if let Some(expanded) = node.expanded {
            let _ = element.set_attribute("aria-expanded", if expanded { "true" } else { "false" });
        } else {
            let _ = element.remove_attribute("aria-expanded");
        }
        match &node.hint {
            Some(hint) => {
                let _ = element.set_attribute("aria-description", hint);
            }
            None => {
                let _ = element.remove_attribute("aria-description");
            }
        }
    }

    fn update_select(&self, element: &Element, node: &AccessNode) {
        let Some(select) = element.dyn_ref::<HtmlSelectElement>() else {
            return;
        };
        let current = option_pairs(select);
        if current != node.options {
            while let Some(child) = select.first_child() {
                let _ = select.remove_child(&child);
            }
            for (value, label) in &node.options {
                if let Ok(option) = self.document.create_element("option") {
                    let _ = option.set_attribute("value", value);
                    option.set_text_content(Some(label));
                    if let Some(option) = option.dyn_ref::<HtmlOptionElement>() {
                        option.set_value(value);
                    }
                    let _ = select.append_child(&option);
                }
            }
        }
        let value = node.value.as_deref().unwrap_or_default();
        if select.value() != value {
            select.set_value(value);
        }
    }

    fn reorder(&self, tree: &AccessTree) {
        let mut anchors: BTreeMap<String, Option<web_sys::Node>> = BTreeMap::new();
        let mut table_key = String::new();
        let mut row_key = String::new();
        let mut dialog_key = String::new();
        for node in &tree.nodes {
            let Some(element) = self.elements.get(&node.key) else {
                continue;
            };
            let dialog_parent = |key: &str| {
                self.elements
                    .get(key)
                    .cloned()
                    .unwrap_or_else(|| self.items.clone())
            };
            let (parent, anchor_key) = match node.role {
                AccessRole::Dialog => {
                    dialog_key = node.key.clone();
                    table_key.clear();
                    row_key.clear();
                    (self.items.clone(), String::new())
                }
                AccessRole::Table => {
                    table_key = node.key.clone();
                    row_key.clear();
                    if dialog_key.is_empty() {
                        (self.items.clone(), String::new())
                    } else {
                        (dialog_parent(&dialog_key), dialog_key.clone())
                    }
                }
                AccessRole::Row => {
                    row_key = node.key.clone();
                    let parent = self
                        .elements
                        .get(&table_key)
                        .cloned()
                        .unwrap_or_else(|| self.items.clone());
                    (parent, table_key.clone())
                }
                AccessRole::Cell | AccessRole::ColumnHeader => {
                    let parent = self
                        .elements
                        .get(&row_key)
                        .cloned()
                        .unwrap_or_else(|| self.items.clone());
                    (parent, row_key.clone())
                }
                _ => {
                    if dialog_key.is_empty() {
                        (self.items.clone(), String::new())
                    } else {
                        (dialog_parent(&dialog_key), dialog_key.clone())
                    }
                }
            };
            let anchor = anchors
                .entry(anchor_key)
                .or_insert_with(|| parent.first_child());
            let same = anchor
                .as_ref()
                .is_some_and(|current| current.is_same_node(Some(element)));
            if same {
                *anchor = element.next_sibling();
            } else {
                let _ = parent.insert_before(element, anchor.as_ref());
                *anchor = element.next_sibling();
            }
        }
    }

    fn remove_extra(&self, desired: &BTreeSet<String>) {
        let mut extra = Vec::new();
        for (key, element) in &self.elements {
            if !desired.contains(key) {
                extra.push(element.clone());
            }
        }
        for element in extra {
            if let Some(parent) = element.parent_node() {
                let _ = parent.remove_child(&element);
            }
        }
    }

    fn push_focus(&mut self, focus: &FocusManager, dom_owned: bool) {
        let focused = focus.focused().cloned();
        if !focus.keyboard_mode() {
            self.last_focused = focused;
            self.restore_focus = false;
            return;
        }
        if focused == self.last_focused && !self.restore_focus {
            return;
        }
        if !dom_owned && !self.restore_focus {
            self.last_focused = focused;
            return;
        }
        if let Some(target) = &focused
            && let Some(tree) = &self.previous
            && let Some(node) = tree
                .nodes
                .iter()
                .find(|node| node.target.as_ref() == Some(target))
            && let Some(element) = self.elements.get(&node.key)
            && let Some(element) = element.dyn_ref::<HtmlElement>()
        {
            focus_without_scroll(element);
            self.restore_focus = false;
        }
        self.last_focused = focused;
    }
}

fn add(
    target: &web_sys::EventTarget,
    name: &str,
    closure: Closure<dyn FnMut(Event)>,
    listeners: &mut Vec<Closure<dyn FnMut(Event)>>,
) {
    let function: &js_sys::Function = closure.as_ref().unchecked_ref();
    if target
        .add_event_listener_with_callback(name, function)
        .is_ok()
    {
        listeners.push(closure);
    }
}

fn hide_from_assistive_tech(element: &Element) {
    let _ = element.set_attribute("aria-hidden", "true");
    let _ = element.set_attribute("tabindex", "-1");
}

fn element_style(element: &Element) -> Option<web_sys::CssStyleDeclaration> {
    element.dyn_ref::<HtmlElement>().map(HtmlElement::style)
}

fn find_text_agent(canvas: &HtmlCanvasElement) -> Option<HtmlInputElement> {
    let mut node = canvas.next_sibling();
    while let Some(current) = node {
        if let Ok(input) = current.clone().dyn_into::<HtmlInputElement>()
            && input.type_() == "text"
        {
            return Some(input);
        }
        node = current.next_sibling();
    }
    let parent = canvas.parent_node()?;
    let children = parent.child_nodes();
    for index in 0..children.length() {
        if let Some(child) = children.item(index)
            && let Ok(input) = child.dyn_into::<HtmlInputElement>()
            && input.type_() == "text"
        {
            return Some(input);
        }
    }
    None
}

fn option_pairs(select: &HtmlSelectElement) -> Vec<(String, String)> {
    let children = select.children();
    let mut options = Vec::new();
    for index in 0..children.length() {
        let Some(child) = children.item(index) else {
            continue;
        };
        let Some(option) = child.dyn_ref::<HtmlOptionElement>() else {
            continue;
        };
        let label = option.text_content().unwrap_or_default();
        options.push((option.value(), label));
    }
    options
}

fn apply_rect(element: &Element, node: &AccessNode, transform: Option<SlideTransform>) {
    let rect = if node.screen {
        node.rect
    } else {
        transform.map_or(node.rect, |transform| transform.to_screen_rect(node.rect))
    };
    let rect = if rect.min.x.is_finite()
        && rect.min.y.is_finite()
        && rect.width().is_finite()
        && rect.height().is_finite()
        && rect.width() >= 0.0
        && rect.height() >= 0.0
    {
        rect
    } else {
        egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1.0, 1.0))
    };
    let style = element_style(element);
    let Some(style) = style else {
        return;
    };
    let _ = style.set_property("position", "absolute");
    let _ = style.set_property("box-sizing", "border-box");
    let _ = style.set_property("margin", "0");
    let _ = style.set_property("padding", "0");
    let _ = style.set_property("left", &format!("{}px", rect.min.x));
    let _ = style.set_property("top", &format!("{}px", rect.min.y));
    let _ = style.set_property("width", &format!("{}px", rect.width()));
    let _ = style.set_property("height", &format!("{}px", rect.height()));
}

fn map_key(key: &str) -> Option<egui::Key> {
    match key {
        "ArrowUp" => Some(egui::Key::ArrowUp),
        "ArrowDown" => Some(egui::Key::ArrowDown),
        "ArrowLeft" => Some(egui::Key::ArrowLeft),
        "ArrowRight" => Some(egui::Key::ArrowRight),
        "Home" => Some(egui::Key::Home),
        "End" => Some(egui::Key::End),
        "Enter" => Some(egui::Key::Enter),
        " " | "Spacebar" => Some(egui::Key::Space),
        "Escape" | "Esc" => Some(egui::Key::Escape),
        "Tab" => Some(egui::Key::Tab),
        "m" | "M" => Some(egui::Key::M),
        "c" | "C" => Some(egui::Key::C),
        _ => None,
    }
}

/// Read an `<input>`/`<textarea>`'s `value` without the textarea web-sys type.
fn element_value(element: &Element) -> Option<String> {
    js_sys::Reflect::get(element.as_ref(), &JsValue::from_str("value"))
        .ok()
        .and_then(|value| value.as_string())
}

/// Write an `<input>`/`<textarea>`'s `value`.
fn set_element_value(element: &Element, value: &str) {
    let _ = js_sys::Reflect::set(
        element.as_ref(),
        &JsValue::from_str("value"),
        &JsValue::from_str(value),
    );
}

fn focus_without_scroll(element: &HtmlElement) {
    let options = js_sys::Object::new();
    let _ = js_sys::Reflect::set(
        &options,
        &JsValue::from_str("preventScroll"),
        &JsValue::from_bool(true),
    );
    let Some(function) = js_sys::Reflect::get(element.as_ref(), &JsValue::from_str("focus"))
        .ok()
        .and_then(|value| value.dyn_into::<js_sys::Function>().ok())
    else {
        return;
    };
    let _ = function.call1(element.as_ref(), options.as_ref());
}

impl Drop for AccessLayer {
    fn drop(&mut self) {
        self.root.remove();
    }
}
