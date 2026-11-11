//! Keyboard focus order and activation for the learner player.
//!
//! The manager only reports what the learner does: it turns keys into the same
//! [`Input`]s the pointer path sends and draws the focus ring. Every play-time
//! decision still belongs to `coursecraft-logic`.

use std::collections::{BTreeMap, BTreeSet};

use coursecraft_course::{Link, ObjectKind, QuestionKind, RadioOption, Slide, Value};
use coursecraft_logic::{Input, Player};
use egui::{Color32, Event, Id, Key, Modifiers, Rect, Stroke, StrokeKind, Ui, pos2, vec2};

use crate::chrome::{nav_limits, play_pause_inputs, replay_inputs};
use crate::color::parse_color;
use crate::compose::{DrawItem, Hit, validate_frame};
use crate::design::relative_luminance;
use crate::drawer::{DrawerTab, DrawerView, available_tabs, visible_glossary};
use crate::layout::SlideTransform;

/// Distance the hotspot keyboard cursor moves per arrow press, in slide pixels.
pub const HOTSPOT_STEP: f32 = 16.0;
/// Distance when Shift is held, in slide pixels.
pub const HOTSPOT_STEP_FINE: f32 = 4.0;

const FALLBACK_ACCENT: Color32 = Color32::from_rgb(0x2F, 0x6F, 0xEB);
const DARK_RING: Color32 = Color32::from_rgb(0x1F, 0x1F, 0x1F);
const LIGHT_RING: Color32 = Color32::from_rgb(0xF2, 0xF2, 0xF5);

/// One player-chrome button in the focus order.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ChromeButton {
    /// The notice banner's dismiss button.
    DismissNotice,
    /// The review banner's Exit review button.
    ExitReview,
    /// Play the slide, or pause it while running. At the end it replays.
    PlayPause,
    /// Seek the current slide back to the start.
    Replay,
    /// The current slide's seek bar.
    Seek,
    /// Mute or unmute the player.
    Mute,
    /// The player volume slider.
    Volume,
    /// Open or close the outline drawer.
    Outline,
    /// Turn captions on or off.
    Captions,
    /// Change the playback speed.
    Speed,
    Previous,
    Next,
    /// Toggle full screen.
    Fullscreen,
    /// The drawer's close button.
    DrawerClose,
    /// One slide row in the open drawer.
    DrawerRow(String),
    /// A tab button in the drawer.
    DrawerTab(DrawerTab),
    /// One resource in the drawer's Resources tab.
    ResourceRow(String),
    /// The glossary search field.
    GlossarySearch,
    /// One term in the drawer's Glossary tab.
    GlossaryTerm(String),
    /// The transcript text in the drawer's Transcript tab.
    TranscriptText,
    /// The × button at the top-right of an open lightbox.
    CloseLightbox,
}

/// A chrome preference change that the frame applies after the focus update.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChromeRequest {
    /// Mute when audible, otherwise restore the last audible level.
    ToggleMute,
    /// Move the volume by `delta`, clamped to `0.0..=1.0`.
    NudgeVolume(f32),
    /// Set the volume outright, clamped to `0.0..=1.0`.
    SetVolume(f32),
    /// Ask the host to toggle full screen.
    ToggleFullscreen,
    /// Move keyboard focus into the glossary search field.
    FocusGlossarySearch,
}

/// Which transport controls the strip laid out this frame.
///
/// Narrow widths drop controls in a fixed order, and the focus list and access
/// tree must never offer a control that is not on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChromeVisibility {
    pub play: bool,
    pub replay: bool,
    pub seek: bool,
    pub volume: bool,
    pub volume_slider: bool,
    pub captions: bool,
    pub outline: bool,
    pub previous: bool,
    pub next: bool,
    pub speed: bool,
    pub fullscreen: bool,
}

impl ChromeVisibility {
    /// Nothing is on screen.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            play: false,
            replay: false,
            seek: false,
            volume: false,
            volume_slider: false,
            captions: false,
            outline: false,
            previous: false,
            next: false,
            speed: false,
            fullscreen: false,
        }
    }
}

impl Default for ChromeVisibility {
    fn default() -> Self {
        Self {
            play: true,
            replay: true,
            seek: true,
            volume: true,
            volume_slider: true,
            captions: true,
            outline: true,
            previous: true,
            next: true,
            // Opt-in controls: only a layout that placed them turns these on.
            speed: false,
            fullscreen: false,
        }
    }
}

/// A focusable identity that survives list rebuilds.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FocusTarget {
    Object(String),
    /// A link inside `object`. `index` is its run order within the object.
    Link {
        object: String,
        index: usize,
        link: Link,
    },
    Choice(String),
    SequenceMove {
        choice: String,
        up: bool,
    },
    FillIn,
    MatchRow(String),
    Hotspot,
    DragItem(String),
    Chrome(ChromeButton),
}

impl FocusTarget {
    /// Is this target one of the player-chrome controls?
    #[must_use]
    pub fn is_chrome(&self) -> bool {
        matches!(self, Self::Chrome(_))
    }
}

/// A focus change reported by the web DOM layer rather than by egui's canvas.
///
/// The browser owns native `Tab` and screen-reader navigation, so these events
/// feed the same manager the keyboard does without a second implementation of
/// any action.
#[derive(Debug, Clone, PartialEq)]
pub enum ExternalFocus {
    /// DOM focus landed on one of our elements.
    Focused(FocusTarget),
    /// A button, radio or checkbox was clicked.
    Activated(FocusTarget),
    /// A key pressed on one of our elements.
    Key { key: Key, shift: bool },
    /// Typed text for the fill-in field.
    Text(String),
    /// A matching row was chosen.
    Match { left: String, right: Option<String> },
    /// DOM focus left the layer.
    Blurred,
}

/// The id of the fill-in text field, so focus can move into and out of it.
#[must_use]
pub fn fill_in_id() -> Id {
    Id::unique("coursecraft-fill-in")
}

/// The id of a text-entry input's field, so focus can move into and out of it.
#[must_use]
pub fn input_text_id(object: &str) -> Id {
    Id::unique(("coursecraft-input-text", object))
}

/// How the keyboard drag gesture should be painted by the slide view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyboardDragVisual {
    pub item: String,
    /// `None` means "back to start".
    pub target: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct KeyboardDrag {
    item: String,
    targets: Vec<String>,
    index: usize,
    is_draggable_object: bool,
}

impl KeyboardDrag {
    fn current_target(&self) -> Option<&str> {
        self.targets.get(self.index).map(String::as_str)
    }
}

/// Build the focus order for the current player state.
///
/// Order: author `focus_order` objects, question controls, the base-layer
/// objects a click reaches, objects on visible layers, then player chrome.
/// `decorative` objects are never included.
#[must_use]
pub fn build_list(
    player: &Player<'_>,
    show_nav: bool,
    show_menu: bool,
    drawer_open: bool,
) -> Vec<FocusTarget> {
    build_list_with_visibility(
        player,
        show_nav,
        show_menu,
        drawer_open,
        ChromeVisibility::default(),
        &DrawerView::default(),
    )
}

/// The open drawer's focus order: close, the tabs, then the chosen tab's items.
fn drawer_targets(player: &Player<'_>, drawer: &DrawerView) -> Vec<FocusTarget> {
    let course = player.course();
    let mut list = vec![FocusTarget::Chrome(ChromeButton::DrawerClose)];
    let tabs = available_tabs(course);
    if tabs.len() > 1 {
        list.extend(
            tabs.iter()
                .map(|tab| FocusTarget::Chrome(ChromeButton::DrawerTab(*tab))),
        );
    }
    match drawer.effective_tab(course) {
        DrawerTab::Menu => {
            for slide in player.play_order() {
                list.push(FocusTarget::Chrome(ChromeButton::DrawerRow(
                    slide.id.clone(),
                )));
            }
        }
        DrawerTab::Resources => {
            for resource in &course.resources {
                list.push(FocusTarget::Chrome(ChromeButton::ResourceRow(
                    resource.id.clone(),
                )));
            }
        }
        DrawerTab::Glossary => {
            list.push(FocusTarget::Chrome(ChromeButton::GlossarySearch));
            for entry in visible_glossary(course, &drawer.query) {
                list.push(FocusTarget::Chrome(ChromeButton::GlossaryTerm(
                    entry.id.clone(),
                )));
            }
        }
        DrawerTab::Transcript => list.push(FocusTarget::Chrome(ChromeButton::TranscriptText)),
    }
    list
}

/// The transport strip's controls, in reading order, for those that were laid out.
fn push_strip_targets(
    list: &mut Vec<FocusTarget>,
    player: &Player<'_>,
    slide: &coursecraft_course::Slide,
    visibility: ChromeVisibility,
    show_menu: bool,
    show_nav: bool,
) {
    let started = player.state().started;
    if started && visibility.play {
        list.push(FocusTarget::Chrome(ChromeButton::PlayPause));
    }
    if started && visibility.replay {
        list.push(FocusTarget::Chrome(ChromeButton::Replay));
    }
    if started && visibility.seek && player.effective_player().show_seekbar {
        list.push(FocusTarget::Chrome(ChromeButton::Seek));
    }
    if started && visibility.volume {
        list.push(FocusTarget::Chrome(ChromeButton::Mute));
    }
    if started && visibility.volume_slider {
        list.push(FocusTarget::Chrome(ChromeButton::Volume));
    }
    if visibility.captions && crate::media_driver::slide_has_captions(slide) {
        list.push(FocusTarget::Chrome(ChromeButton::Captions));
    }
    if started && visibility.speed && player.course().player.show_speed {
        list.push(FocusTarget::Chrome(ChromeButton::Speed));
    }
    if visibility.outline && show_menu {
        list.push(FocusTarget::Chrome(ChromeButton::Outline));
    }
    if visibility.previous && show_nav {
        let (is_first, _) = nav_limits(
            player.play_order().iter().map(|s| s.id.as_str()),
            slide.id.as_str(),
        );
        if !is_first {
            list.push(FocusTarget::Chrome(ChromeButton::Previous));
        }
    }
    if visibility.next && show_nav {
        let (_, is_last) = nav_limits(
            player.play_order().iter().map(|s| s.id.as_str()),
            slide.id.as_str(),
        );
        if !is_last {
            list.push(FocusTarget::Chrome(ChromeButton::Next));
        }
    }
    if started && visibility.fullscreen && player.course().player.show_fullscreen {
        list.push(FocusTarget::Chrome(ChromeButton::Fullscreen));
    }
}

/// Build the focus order with only the chrome controls that were laid out.
#[must_use]
pub(crate) fn build_list_with_visibility(
    player: &Player<'_>,
    show_nav: bool,
    show_menu: bool,
    drawer_open: bool,
    visibility: ChromeVisibility,
    drawer: &DrawerView,
) -> Vec<FocusTarget> {
    if drawer_open && !player.lightbox_open() {
        return drawer_targets(player, drawer);
    }
    let Some(slide) = player.current_slide() else {
        return Vec::new();
    };

    let mut ordered = Vec::new();
    let mut default = Vec::new();
    let link_targets = player.link_objects();
    let mut linked = Vec::new();
    let links_for = |id: &str| -> Vec<FocusTarget> {
        link_targets
            .iter()
            .find(|(object, _)| object == id)
            .map(|(object, links)| {
                links
                    .iter()
                    .enumerate()
                    .map(|(index, link)| FocusTarget::Link {
                        object: object.clone(),
                        index,
                        link: link.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    for id in player.keyboard_objects() {
        let Some((obj, _layer)) = slide.object(&id) else {
            continue;
        };
        if obj.decorative || is_control_object(player, slide, &id) {
            continue;
        }
        let target = FocusTarget::Object(id.clone());
        let links = links_for(&id);
        if !links.is_empty() {
            linked.push(id.clone());
        }
        match obj.focus_order {
            Some(order) => {
                ordered.push((order, target));
                for link in links {
                    ordered.push((order, link));
                }
            }
            None => {
                default.push(target);
                default.extend(links);
            }
        }
    }
    // Links on text that has no click trigger of its own still reach the
    // keyboard. They sit beside their object's place in the reading order.
    for (id, links) in &link_targets {
        if linked.iter().any(|linked| linked == id) {
            continue;
        }
        let Some((obj, _)) = slide.object(id) else {
            continue;
        };
        if obj.decorative || is_control_object(player, slide, id) {
            continue;
        }
        let targets: Vec<FocusTarget> = links
            .iter()
            .enumerate()
            .map(|(index, link)| FocusTarget::Link {
                object: id.clone(),
                index,
                link: link.clone(),
            })
            .collect();
        match obj.focus_order {
            Some(order) => {
                for target in targets {
                    ordered.push((order, target));
                }
            }
            None => default.extend(targets),
        }
    }
    // Stable: equal focus_order values keep their default reading order.
    ordered.sort_by_key(|(order, _)| *order);

    let mut list: Vec<FocusTarget> = ordered.into_iter().map(|(_, target)| target).collect();
    if !player.base_objects_hidden() {
        list.extend(question_controls(player, slide));
    }
    list.extend(default);

    // An open lightbox traps focus inside the box: its slide's controls and
    // the close button, with no transport chrome.
    if player.lightbox_open() {
        list.push(FocusTarget::Chrome(ChromeButton::CloseLightbox));
        return list;
    }

    push_strip_targets(&mut list, player, slide, visibility, show_menu, show_nav);
    list
}

/// [`build_list`] with the notice banner's dismiss button first while a notice
/// is showing, so every caller agrees on the notice's place in the order.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_list_with_notice(
    player: &Player<'_>,
    show_nav: bool,
    show_menu: bool,
    drawer_open: bool,
    notice_showing: bool,
    review_showing: bool,
    visibility: ChromeVisibility,
    drawer: &DrawerView,
) -> Vec<FocusTarget> {
    let mut list =
        build_list_with_visibility(player, show_nav, show_menu, drawer_open, visibility, drawer);
    if review_showing {
        list.insert(0, FocusTarget::Chrome(ChromeButton::ExitReview));
    }
    if notice_showing {
        list.insert(0, FocusTarget::Chrome(ChromeButton::DismissNotice));
    }
    list
}

/// Objects that the hotspot or drag-and-drop question turns into controls.
fn is_control_object(player: &Player<'_>, slide: &Slide, id: &str) -> bool {
    if player.question_locked() {
        return false;
    }
    let Some(question) = slide.question.as_ref() else {
        return false;
    };
    match question.kind {
        QuestionKind::Hotspot => question.hotspot_image.as_deref() == Some(id),
        QuestionKind::DragDrop => question.drag_items.iter().any(|d| d.item == id),
        _ => false,
    }
}

fn question_controls(player: &Player<'_>, slide: &Slide) -> Vec<FocusTarget> {
    let Some(question) = slide.question.as_ref() else {
        return Vec::new();
    };
    if player.question_locked() && !player.is_reviewing() {
        return Vec::new();
    }
    match question.kind {
        QuestionKind::MultipleChoice
        | QuestionKind::MultipleResponse
        | QuestionKind::TrueFalse
        | QuestionKind::Likert => question
            .choices
            .iter()
            .map(|c| FocusTarget::Choice(c.id.clone()))
            .collect(),
        QuestionKind::FillIn | QuestionKind::ShortAnswer | QuestionKind::Essay => {
            vec![FocusTarget::FillIn]
        }
        QuestionKind::Sequence => {
            if player.is_reviewing() {
                // The sequence buttons are not composed while reviewing.
                return Vec::new();
            }
            let authored: Vec<String> = question.choices.iter().map(|c| c.id.clone()).collect();
            let order: Vec<String> = if is_permutation(player.sequence_order(), &authored) {
                player.sequence_order().to_vec()
            } else {
                authored
            };
            let last = order.len().saturating_sub(1);
            let mut controls = Vec::new();
            for (i, id) in order.iter().enumerate() {
                if i > 0 {
                    controls.push(FocusTarget::SequenceMove {
                        choice: id.clone(),
                        up: true,
                    });
                }
                if i < last {
                    controls.push(FocusTarget::SequenceMove {
                        choice: id.clone(),
                        up: false,
                    });
                }
            }
            controls
        }
        QuestionKind::Matching => question
            .pairs
            .iter()
            .map(|p| FocusTarget::MatchRow(p.id.clone()))
            .collect(),
        QuestionKind::Hotspot => vec![FocusTarget::Hotspot],
        QuestionKind::DragDrop => question
            .drag_items
            .iter()
            .filter(|d| !is_decorative(slide, &d.item))
            .map(|d| FocusTarget::DragItem(d.item.clone()))
            .collect(),
        QuestionKind::Freeform => Vec::new(),
    }
}

fn is_decorative(slide: &Slide, id: &str) -> bool {
    slide.object(id).is_some_and(|(obj, _)| obj.decorative)
}

fn is_permutation(order: &[String], authored: &[String]) -> bool {
    order.len() == authored.len()
        && authored.iter().all(|id| order.contains(id))
        && order.iter().all(|id| authored.contains(id))
}

fn first_target_on_layer(player: &Player<'_>, layer: &str) -> Option<FocusTarget> {
    let slide = player.current_slide()?;
    for id in player.keyboard_objects() {
        let Some((obj, owner)) = slide.object(&id) else {
            continue;
        };
        if owner != Some(layer) || obj.decorative || is_control_object(player, slide, &id) {
            continue;
        }
        return Some(FocusTarget::Object(id));
    }
    None
}

/// Whether `object` is a text-entry input on the current slide.
fn focused_is_text_entry(player: &Player<'_>, object: &str) -> bool {
    player
        .current_slide()
        .and_then(|slide| slide.object(object))
        .is_some_and(|(obj, _)| matches!(obj.kind, ObjectKind::TextEntry { .. }))
}

/// The current open drop targets for the drag-and-drop question.
fn open_targets(player: &Player<'_>) -> Vec<String> {
    if player.question_locked() {
        return Vec::new();
    }
    player
        .current_slide()
        .and_then(|s| s.question.as_ref())
        .filter(|q| q.kind == QuestionKind::DragDrop)
        .map(|q| q.drop_targets.clone())
        .unwrap_or_default()
}

/// Owns the keyboard focus list and the keyboard-only interaction state.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FocusManager {
    focused: Option<FocusTarget>,
    list: Vec<FocusTarget>,
    prev_slide: Option<String>,
    prev_layers: Vec<String>,
    prev_drawer: bool,
    /// The learner hasn't moved focus on this slide yet, so it tracks the first
    /// item: an earlier object can appear later through its timing window.
    auto_first: bool,
    keyboard_mode: bool,
    hovered: Option<String>,
    drag: Option<KeyboardDrag>,
    hotspot_cursor: Option<(f32, f32)>,
    last_ring: Option<Rect>,
    /// Events pushed by the web DOM layer, processed on the next update.
    external: Vec<ExternalFocus>,
    /// The DOM owns typing in the fill-in, so egui must not request focus for it.
    dom_focus_owner: bool,
    /// The notice banner's dismiss button is showing, so it leads the focus list.
    notice_visible: bool,
    /// Which transport controls the strip laid out this frame.
    chrome: ChromeVisibility,
    /// A volume change requested by keyboard or the DOM layer.
    chrome_request: Option<ChromeRequest>,
    /// Input objects whose arrow keys belong to the control, not slide nav.
    input_arrows: BTreeSet<String>,
    /// DOM text-entry buffers, committed on Enter or when focus leaves.
    text_buffers: BTreeMap<String, String>,
    /// What the learner chose inside the open drawer.
    drawer: DrawerView,
    /// Whether a lightbox was open on the last rebuild.
    lightbox_open: bool,
    /// The target focus should return to when the lightbox closes.
    lightbox_return: Option<FocusTarget>,
}

/// Which chrome controls the caller allows this frame.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ChromeOptions {
    pub show_nav: bool,
    pub show_menu: bool,
}

impl FocusManager {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Queue a focus change reported by the web DOM layer.
    pub fn external(&mut self, event: ExternalFocus) {
        self.external.push(event);
    }

    /// Tell the manager whether a DOM element owns typing on the current frame.
    ///
    /// When true, the egui fill-in never requests focus, so the DOM input keeps
    /// the learner's keystrokes and caret.
    pub fn set_dom_focus_owner(&mut self, owns: bool) {
        self.dom_focus_owner = owns;
    }

    /// Replace what the learner chose inside the drawer. The frame calls this
    /// before every update from the state it paints.
    pub fn set_drawer_view(&mut self, view: DrawerView) {
        self.drawer = view;
    }

    /// The drawer tab, search text and open term as keyboard activation left them.
    #[must_use]
    pub fn drawer_view(&self) -> &DrawerView {
        &self.drawer
    }

    /// Whether the focus list is currently trapped inside a lightbox.
    #[must_use]
    pub fn lightbox_open(&self) -> bool {
        self.lightbox_open
    }

    /// Remember what to focus again when the lightbox closes. The player frame
    /// calls this with the object whose click opened the box.
    pub fn remember_lightbox_opener(&mut self, target: Option<FocusTarget>) {
        if let Some(target) = target {
            self.lightbox_return = Some(target);
        }
    }

    /// Tell the manager whether the notice banner's dismiss button is showing.
    ///
    /// The player frame calls this every frame from the same state that draws
    /// the banner, so the focus list and the banner can never disagree.
    pub fn set_notice_visible(&mut self, visible: bool) {
        self.notice_visible = visible;
    }

    /// Tell the manager which transport controls the strip has room for.
    pub fn set_chrome_visibility(&mut self, visibility: ChromeVisibility) {
        self.chrome = visibility;
    }

    /// The visibility last reported by the strip.
    #[must_use]
    pub fn chrome_visibility(&self) -> ChromeVisibility {
        self.chrome
    }

    /// Take the pending volume request, if any.
    #[must_use]
    pub fn take_chrome_request(&mut self) -> Option<ChromeRequest> {
        self.chrome_request.take()
    }

    /// Push queued external key presses into egui's input.
    ///
    /// They then travel through the ordinary [`FocusManager::handle_keys`] and
    /// chrome paths, so an external key behaves exactly like a canvas key.
    pub fn flush_external_keys(&mut self, ui: &Ui) {
        let mut keys = Vec::new();
        self.external.retain(|event| match event {
            ExternalFocus::Key { key, shift } => {
                keys.push((*key, *shift));
                false
            }
            _ => true,
        });
        if keys.is_empty() {
            return;
        }
        let mut any_shift = false;
        ui.input_mut(|input| {
            for (key, shift) in keys {
                any_shift |= shift;
                input.events.push(Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: if shift {
                        Modifiers::SHIFT
                    } else {
                        Modifiers::NONE
                    },
                });
            }
            if any_shift {
                input.modifiers.shift = true;
            }
        });
    }

    /// Apply the queued non-key external events.
    #[cfg(test)]
    fn process_external(
        &mut self,
        player: &Player<'_>,
        drawer_open: &mut bool,
        inputs: &mut Vec<Input>,
    ) {
        let mut captions_on = false;
        let mut notice_dismissed = false;
        self.process_external_with_captions(
            player,
            drawer_open,
            inputs,
            &mut captions_on,
            &mut notice_dismissed,
        );
    }

    fn process_external_with_captions(
        &mut self,
        player: &Player<'_>,
        drawer_open: &mut bool,
        inputs: &mut Vec<Input>,
        captions_on: &mut bool,
        notice_dismissed: &mut bool,
    ) {
        for event in std::mem::take(&mut self.external) {
            match event {
                ExternalFocus::Focused(target) => {
                    self.focus_external(target, player);
                }
                ExternalFocus::Activated(target) => {
                    if self.focus_external(target, player) {
                        self.activate(player, drawer_open, inputs, captions_on, notice_dismissed);
                    }
                }
                // Keys were turned into ordinary egui events by `flush_external_keys`.
                ExternalFocus::Key { .. } => {}
                ExternalFocus::Text(text) => {
                    if let Some(FocusTarget::Object(id)) = self.focused.clone()
                        && focused_is_text_entry(player, &id)
                    {
                        self.text_buffers.insert(id, text);
                    } else {
                        inputs.push(Input::TextEntered { text });
                    }
                }
                ExternalFocus::Match { left, right } => {
                    inputs.push(Input::MatchSelected { left, right });
                }
                ExternalFocus::Blurred => {
                    if let Some(FocusTarget::Object(id)) = self.focused.clone()
                        && let Some(text) = self.text_buffers.remove(&id)
                    {
                        inputs.push(Input::SetInputValue {
                            object: id,
                            value: Value::Text(text),
                        });
                    }
                    self.keyboard_mode = false;
                }
            }
        }
    }

    fn focus_external(&mut self, target: FocusTarget, player: &Player<'_>) -> bool {
        if !self.list.contains(&target) {
            return false;
        }
        let keeps_drag = matches!(
            (&self.drag, &target),
            (Some(drag), FocusTarget::DragItem(id) | FocusTarget::Object(id)) if &drag.item == id
        );
        if !keeps_drag {
            self.drag = None;
        }
        self.focused = Some(target);
        self.keyboard_mode = true;
        self.auto_first = false;
        if matches!(self.focused, Some(FocusTarget::Hotspot)) {
            self.reset_hotspot(player);
        } else {
            self.hotspot_cursor = None;
        }
        true
    }

    /// Current focus target. `None` when the list is empty.
    #[must_use]
    pub fn focused(&self) -> Option<&FocusTarget> {
        self.focused.as_ref()
    }

    /// The focus list built on the most recent update.
    #[must_use]
    pub fn list(&self) -> &[FocusTarget] {
        &self.list
    }

    /// Whether the focus ring is showing.
    #[must_use]
    pub fn keyboard_mode(&self) -> bool {
        self.keyboard_mode
    }

    /// The object whose hover the keyboard focus is holding, if any.
    #[must_use]
    pub fn hovered_object(&self) -> Option<&str> {
        self.hovered.as_deref()
    }

    /// The screen-space ring rect from the most recent paint. Test hook.
    #[must_use]
    pub fn ring_rect(&self) -> Option<Rect> {
        self.last_ring
    }

    /// The keyboard drag gesture to paint, if one is held.
    #[must_use]
    pub fn drag_visual(&self) -> Option<KeyboardDragVisual> {
        let drag = self.drag.as_ref()?;
        Some(KeyboardDragVisual {
            item: drag.item.clone(),
            target: drag.current_target().map(str::to_owned),
        })
    }

    /// Whether this key belongs to the focused slide control rather than chrome.
    #[must_use]
    pub fn captures_arrow(&self, key: Key) -> bool {
        if self.drag.is_some() {
            return matches!(
                key,
                Key::ArrowUp | Key::ArrowDown | Key::ArrowLeft | Key::ArrowRight
            );
        }
        if let Some(FocusTarget::Object(id)) = &self.focused
            && self.input_arrows.contains(id)
        {
            return matches!(
                key,
                Key::ArrowUp | Key::ArrowDown | Key::ArrowLeft | Key::ArrowRight
            );
        }
        match self.focused {
            Some(FocusTarget::Hotspot | FocusTarget::FillIn) => matches!(
                key,
                Key::ArrowUp | Key::ArrowDown | Key::ArrowLeft | Key::ArrowRight
            ),
            Some(FocusTarget::MatchRow(_)) => {
                matches!(key, Key::ArrowUp | Key::ArrowDown)
            }
            Some(FocusTarget::DragItem(_)) => matches!(
                key,
                Key::ArrowUp | Key::ArrowDown | Key::ArrowLeft | Key::ArrowRight
            ),
            Some(FocusTarget::Chrome(ChromeButton::Seek | ChromeButton::Volume)) => matches!(
                key,
                Key::ArrowUp | Key::ArrowDown | Key::ArrowLeft | Key::ArrowRight
            ),
            _ => false,
        }
    }

    /// Rebuild focus and turn this frame's keys into runtime inputs.
    pub fn update(
        &mut self,
        ui: &Ui,
        player: &Player<'_>,
        show_nav: bool,
        show_menu: bool,
        drawer_open: &mut bool,
        pointer_hovered: Option<&str>,
    ) -> Vec<Input> {
        let mut captions_on = false;
        let mut notice_dismissed = false;
        self.update_with_captions(
            ui,
            player,
            ChromeOptions {
                show_nav,
                show_menu,
            },
            drawer_open,
            pointer_hovered,
            &mut captions_on,
            &mut notice_dismissed,
        )
    }

    /// Like [`FocusManager::update`], and lets the captions button toggle
    /// [`crate::PlayerFrame::captions_on`] and a notice activation dismiss it.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn update_with_captions(
        &mut self,
        ui: &Ui,
        player: &Player<'_>,
        chrome: ChromeOptions,
        drawer_open: &mut bool,
        pointer_hovered: Option<&str>,
        captions_on: &mut bool,
        notice_dismissed: &mut bool,
    ) -> Vec<Input> {
        let show_nav = chrome.show_nav;
        let show_menu = chrome.show_menu;
        let mut inputs = Vec::new();
        self.flush_external_keys(ui);
        let before_focus = self.focused.clone();
        let before_mode = self.keyboard_mode;
        let focus_key = any_focus_key(ui);

        if ui.input(|i| i.pointer.any_pressed()) {
            self.keyboard_mode = false;
        }
        // The ring is off, so the learner can't see what has focus. The first
        // key only shows it: it neither moves focus nor activates anything.
        let ring_key = reveal_key(ui);

        let slide_changed = self.rebuild(player, show_nav, show_menu, *drawer_open);

        self.process_external_with_captions(
            player,
            drawer_open,
            &mut inputs,
            captions_on,
            notice_dismissed,
        );

        let reveal = !self.keyboard_mode && ring_key;
        if ring_key && !reveal {
            self.auto_first = false;
        }
        if ring_key {
            self.keyboard_mode = true;
        }

        if matches!(self.focused, Some(FocusTarget::Hotspot)) && self.hotspot_cursor.is_none() {
            self.reset_hotspot(player);
        }
        if self.focused.is_some()
            && !matches!(self.focused, Some(FocusTarget::Hotspot))
            && self.hotspot_cursor.is_some()
        {
            self.hotspot_cursor = None;
        }

        if reveal {
            consume_reveal_keys(ui);
        } else if self.drag.is_some() {
            self.update_drag_targets(player);
            if self.drag.is_none() {
                // The question stopped accepting drops; fall through to keys.
                self.handle_keys(
                    ui,
                    player,
                    drawer_open,
                    &mut inputs,
                    captions_on,
                    notice_dismissed,
                );
            } else {
                self.handle_drag_keys(ui, player, &mut inputs);
            }
        } else {
            self.handle_keys(
                ui,
                player,
                drawer_open,
                &mut inputs,
                captions_on,
                notice_dismissed,
            );
        }
        if self.drag.is_none() {
            self.forward_unconsumed_key_pressed(ui, player, &mut inputs);
        }

        // A pointer press ends keyboard mode without moving focus, and that must
        // still release the hover the keyboard was holding.
        if before_focus != self.focused || before_mode != self.keyboard_mode {
            let emit_start = self.keyboard_mode;
            self.emit_hover(
                player,
                pointer_hovered,
                !slide_changed,
                emit_start,
                &mut inputs,
            );
        }
        if focus_key {
            // Consume egui's own Tab/arrow focus pass so there is one focus order.
            ui.memory_mut(|memory| memory.move_focus(egui::FocusDirection::None));
        }
        self.scroll_focused_into_view(player, &mut inputs);
        inputs
    }

    /// If focus lands on a child that is only partly inside its scrolling
    /// panel, scroll the panel just enough to show it.
    fn scroll_focused_into_view(&self, player: &Player<'_>, inputs: &mut Vec<Input>) {
        let Some(FocusTarget::Object(id)) = &self.focused else {
            return;
        };
        let Some(slide) = player.current_slide() else {
            return;
        };
        let Some((object, _)) = slide.object(id) else {
            return;
        };
        let Some(panel_id) = object.panel.as_deref() else {
            return;
        };
        let Some((panel, _)) = slide.object(panel_id) else {
            return;
        };
        let ObjectKind::ScrollPanel { content_height, .. } = panel.kind else {
            return;
        };
        let viewport = panel.frame.h;
        if !viewport.is_finite() || viewport <= 0.0 || object.frame.h >= viewport {
            return;
        }
        let current = player.panel_offset(panel_id);
        let top = object.frame.y - current;
        let bottom = top + object.frame.h;
        let panel_top = panel.frame.y;
        let panel_bottom = panel_top + viewport;
        let mut offset = current;
        if top < panel_top {
            offset -= panel_top - top;
        } else if bottom > panel_bottom {
            offset += bottom - panel_bottom;
        }
        let offset = coursecraft_course::clamp_panel_offset(content_height, viewport, offset);
        if (offset - current).abs() > 0.01 {
            inputs.push(Input::PanelScrolled {
                object: panel_id.to_owned(),
                offset,
            });
        }
    }

    /// Rebuild the list and move focus after a slide, layer or drawer change.
    ///
    /// Returns whether the slide changed this frame.
    pub(crate) fn rebuild(
        &mut self,
        player: &Player<'_>,
        show_nav: bool,
        show_menu: bool,
        drawer_open: bool,
    ) -> bool {
        let slide_id = player.current_slide().map(|s| s.id.clone());
        let layers = player.state().visible_layers.clone();
        let lightbox = player.lightbox_open();
        let lightbox_opened = lightbox && !self.lightbox_open;
        let lightbox_closed = !lightbox && self.lightbox_open;
        let new_list = build_list_with_notice(
            player,
            show_nav,
            show_menu,
            drawer_open,
            self.notice_visible,
            player.is_reviewing(),
            self.chrome,
            &self.drawer,
        );
        let slide_changed = slide_id != self.prev_slide;
        let drawer_changed = drawer_open != self.prev_drawer;
        let new_layers: Vec<String> = layers
            .iter()
            .filter(|layer| !self.prev_layers.contains(layer))
            .cloned()
            .collect();
        let hidden_layers: Vec<String> = self
            .prev_layers
            .iter()
            .filter(|layer| !layers.contains(layer))
            .cloned()
            .collect();

        if lightbox_opened {
            self.lightbox_return = self.lightbox_return.take().or_else(|| self.focused.clone());
            self.focused = new_list.first().cloned();
            self.auto_first = false;
            self.drag = None;
            self.hotspot_cursor = None;
            self.hovered = None;
        } else if lightbox_closed {
            let restored = self
                .lightbox_return
                .take()
                .filter(|target| new_list.contains(target))
                .or_else(|| new_list.first().cloned());
            self.focused = restored;
            self.auto_first = false;
            self.drag = None;
            self.hotspot_cursor = None;
            self.hovered = None;
        } else if slide_changed {
            self.focused = new_list.first().cloned();
            self.auto_first = true;
            self.drag = None;
            self.hotspot_cursor = None;
            self.hovered = None;
        } else if drawer_changed {
            self.auto_first = false;
            if drawer_open {
                self.focused = new_list.first().cloned();
            } else {
                let outline = FocusTarget::Chrome(ChromeButton::Outline);
                self.focused = if new_list.contains(&outline) {
                    Some(outline)
                } else {
                    new_list.first().cloned()
                };
            }
        } else if !new_layers.is_empty() {
            self.auto_first = false;
            if let Some(layer) = new_layers.last()
                && let Some(target) = first_target_on_layer(player, layer)
            {
                self.focused = Some(target);
            }
        } else if !hidden_layers.is_empty() {
            self.auto_first = false;
            self.focused = new_list.first().cloned();
        } else if let Some(focused) = self.focused.clone()
            && !new_list.contains(&focused)
        {
            self.focused = self.fallback_target(&focused, &new_list);
        }
        if self.focused.is_none() && !new_list.is_empty() {
            self.focused = new_list.first().cloned();
        }
        if self.auto_first {
            self.focused = new_list.first().cloned();
        }

        let mut input_arrows = BTreeSet::new();
        if let Some(slide) = player.current_slide() {
            for target in &new_list {
                let FocusTarget::Object(id) = target else {
                    continue;
                };
                if slide.object(id).is_some_and(|(obj, _)| {
                    matches!(
                        obj.kind,
                        ObjectKind::TextEntry { .. }
                            | ObjectKind::Slider { .. }
                            | ObjectKind::Dial { .. }
                            | ObjectKind::RadioGroup { .. }
                            | ObjectKind::ScrollPanel { .. }
                    )
                }) {
                    input_arrows.insert(id.clone());
                }
            }
        }
        self.input_arrows = input_arrows;
        self.list = new_list;
        self.prev_slide = slide_id;
        self.prev_layers = layers;
        self.prev_drawer = drawer_open;
        self.lightbox_open = lightbox;
        slide_changed
    }

    fn fallback_target(
        &self,
        focused: &FocusTarget,
        new_list: &[FocusTarget],
    ) -> Option<FocusTarget> {
        if let FocusTarget::SequenceMove { choice, up } = focused {
            let other = FocusTarget::SequenceMove {
                choice: choice.clone(),
                up: !up,
            };
            if new_list.contains(&other) {
                return Some(other);
            }
        }
        if new_list.is_empty() {
            return None;
        }
        match self.list.iter().position(|t| t == focused) {
            Some(index) => new_list.get(index).or_else(|| new_list.last()).cloned(),
            None => new_list.last().cloned(),
        }
    }

    fn emit_hover(
        &mut self,
        player: &Player<'_>,
        pointer_hovered: Option<&str>,
        allow_end: bool,
        emit_start: bool,
        inputs: &mut Vec<Input>,
    ) {
        let desired = if emit_start {
            match &self.focused {
                Some(FocusTarget::Object(id)) => Some(id.clone()),
                _ => None,
            }
        } else {
            None
        };
        if desired == self.hovered {
            return;
        }
        if let Some(old) = self.hovered.take() {
            let still_reachable = player.keyboard_objects().iter().any(|id| id == &old);
            if allow_end && still_reachable && pointer_hovered != Some(old.as_str()) {
                inputs.push(Input::HoverEnd { object: old });
            }
        }
        if let Some(new) = desired {
            if pointer_hovered != Some(new.as_str()) {
                inputs.push(Input::HoverStart {
                    object: new.clone(),
                });
            }
            self.hovered = Some(new);
        }
    }

    fn move_focus(&mut self, backwards: bool, player: &Player<'_>) {
        self.auto_first = false;
        if self.list.is_empty() {
            self.focused = None;
            return;
        }
        let next = match self
            .focused
            .as_ref()
            .and_then(|f| self.list.iter().position(|t| t == f))
        {
            Some(index) if backwards => (index + self.list.len() - 1) % self.list.len(),
            Some(index) => (index + 1) % self.list.len(),
            None if backwards => self.list.len() - 1,
            None => 0,
        };
        self.focused = self.list.get(next).cloned();
        if matches!(self.focused, Some(FocusTarget::Hotspot)) {
            self.reset_hotspot(player);
        }
        if !matches!(self.focused, Some(FocusTarget::Hotspot)) {
            self.hotspot_cursor = None;
        }
    }

    #[allow(clippy::cognitive_complexity)]
    fn handle_keys(
        &mut self,
        ui: &Ui,
        player: &Player<'_>,
        drawer_open: &mut bool,
        inputs: &mut Vec<Input>,
        captions_on: &mut bool,
        notice_dismissed: &mut bool,
    ) {
        if player.marker_open().is_some() && consume(ui, Key::Escape) {
            inputs.push(Input::MarkersClosed);
            return;
        }
        // Count presses: on a slow frame several can arrive at once.
        let (forwards, backwards) = ui.input_mut(|i| {
            (
                i.count_and_consume_key(Modifiers::NONE, Key::Tab),
                i.count_and_consume_key(Modifiers::SHIFT, Key::Tab),
            )
        });
        if forwards + backwards > 0 {
            if matches!(self.focused, Some(FocusTarget::FillIn)) {
                ui.memory_mut(|m| m.surrender_focus(fill_in_id()));
            }
            if let Some(FocusTarget::Object(id)) = self.focused.clone()
                && matches!(
                    player
                        .current_slide()
                        .and_then(|s| s.object(&id))
                        .map(|(obj, _)| &obj.kind),
                    Some(ObjectKind::TextEntry { .. })
                )
            {
                ui.memory_mut(|m| m.surrender_focus(input_text_id(&id)));
            }
            for _ in 0..forwards {
                self.move_focus(false, player);
            }
            for _ in 0..backwards {
                self.move_focus(true, player);
            }
            return;
        }

        if let Some(FocusTarget::Object(id)) = self.focused.clone()
            && let Some((obj, _)) = player.current_slide().and_then(|s| s.object(&id))
        {
            match &obj.kind {
                ObjectKind::Marker { .. } => {
                    if consume(ui, Key::Space) || consume(ui, Key::Enter) {
                        inputs.push(Input::MarkerToggled { object: id });
                    }
                    return;
                }
                ObjectKind::ScrollPanel { content_height, .. } => {
                    self.handle_scroll_panel_keys(
                        ui,
                        player,
                        &id,
                        obj.frame.h,
                        *content_height,
                        inputs,
                    );
                    return;
                }
                ObjectKind::TextEntry { multiline, .. } => {
                    // Only the screen-reader field fills this buffer. Otherwise the
                    // live text box owns the text and must see Enter itself.
                    let enter = !*multiline
                        && self.text_buffers.contains_key(&id)
                        && consume(ui, Key::Enter);
                    if enter && let Some(text) = self.text_buffers.remove(&id) {
                        inputs.push(Input::SetInputValue {
                            object: id.clone(),
                            value: Value::Text(text),
                        });
                    }
                    if self.keyboard_mode && !self.dom_focus_owner {
                        ui.memory_mut(|m| m.request_focus(input_text_id(&id)));
                    }
                    return;
                }
                ObjectKind::Slider { .. } | ObjectKind::Dial { .. } => {
                    self.handle_number_input(ui, player, &id, &obj.kind, inputs);
                    return;
                }
                ObjectKind::Checkbox { variable, .. } => {
                    if consume(ui, Key::Space) || consume(ui, Key::Enter) {
                        let checked = matches!(player.variable(variable), Some(Value::Bool(true)));
                        inputs.push(Input::SetInputValue {
                            object: id,
                            value: Value::Bool(!checked),
                        });
                    }
                    return;
                }
                ObjectKind::RadioGroup { options, variable } => {
                    self.handle_radio_keys(ui, player, &id, variable, options, inputs);
                    return;
                }
                _ => {}
            }
        }

        if matches!(self.focused, Some(FocusTarget::FillIn)) {
            // Enter does nothing in the fill-in field; Space is typing, so leave it.
            // In an essay, Enter adds a new line: leave it for the text box.
            let essay = player
                .current_slide()
                .and_then(|slide| slide.question.as_ref())
                .is_some_and(|question| question.kind == QuestionKind::Essay);
            if !essay {
                let _ = consume(ui, Key::Enter);
            }
            if self.keyboard_mode && !self.dom_focus_owner && !ui.ctx().text_edit_focused() {
                ui.memory_mut(|m| m.request_focus(fill_in_id()));
            }
            return;
        }

        if let Some(FocusTarget::MatchRow(left)) = self.focused.clone() {
            let up = consume(ui, Key::ArrowUp);
            let down = consume(ui, Key::ArrowDown);
            if up || down {
                self.cycle_match(player, &left, down, inputs);
            }
            return;
        }

        if matches!(self.focused, Some(FocusTarget::Hotspot)) {
            let fine = ui.input(|i| i.modifiers.shift);
            let step = if fine {
                HOTSPOT_STEP_FINE
            } else {
                HOTSPOT_STEP
            };
            let mut moved = false;
            if let Some(cursor) = self.hotspot_cursor.as_mut() {
                let (left, right, up, down) = ui.input_mut(|i| {
                    let modifiers = i.modifiers;
                    (
                        i.count_and_consume_key(modifiers, Key::ArrowLeft),
                        i.count_and_consume_key(modifiers, Key::ArrowRight),
                        i.count_and_consume_key(modifiers, Key::ArrowUp),
                        i.count_and_consume_key(modifiers, Key::ArrowDown),
                    )
                });
                // Counts stay small: they are key presses within one frame.
                let steps = |back: usize, forward: usize| (forward as f32 - back as f32) * step;
                cursor.0 += steps(left, right);
                cursor.1 += steps(up, down);
                moved = left + right + up + down > 0;
            }
            if moved {
                self.clamp_hotspot(player);
            }
            if (consume(ui, Key::Enter) || consume(ui, Key::Space))
                && let Some((x, y)) = self.hotspot_cursor
            {
                inputs.push(Input::HotspotClicked { x, y });
            }
            return;
        }

        if let Some(FocusTarget::DragItem(id)) = self.focused.clone() {
            if consume(ui, Key::Enter) || consume(ui, Key::Space) {
                let targets = open_targets(player);
                self.drag = Some(KeyboardDrag {
                    item: id,
                    index: 0,
                    targets,
                    is_draggable_object: false,
                });
            }
            return;
        }

        if let Some(FocusTarget::Object(id)) = self.focused.clone()
            && let Some((obj, _)) = player.current_slide().and_then(|s| s.object(&id))
            && let Some(drag_spec) = &obj.drag
            && (consume(ui, Key::Enter) || consume(ui, Key::Space))
        {
            self.drag = Some(KeyboardDrag {
                item: id,
                index: 0,
                targets: drag_spec.targets.clone(),
                is_draggable_object: true,
            });
            return;
        }

        if matches!(self.focused, Some(FocusTarget::Chrome(ChromeButton::Seek))) {
            self.handle_seek_keys(ui, player, inputs);
            return;
        }

        if matches!(
            self.focused,
            Some(FocusTarget::Chrome(ChromeButton::Volume))
        ) {
            self.handle_volume_keys(ui);
            return;
        }

        if consume(ui, Key::Enter) || consume(ui, Key::Space) {
            self.activate(player, drawer_open, inputs, captions_on, notice_dismissed);
        }
    }

    fn forward_unconsumed_key_pressed(
        &self,
        ui: &Ui,
        player: &Player<'_>,
        inputs: &mut Vec<Input>,
    ) {
        if ui.ctx().text_edit_focused() || matches!(self.focused, Some(FocusTarget::FillIn)) {
            return;
        }
        if !player.state().started {
            return;
        }
        if let Some(FocusTarget::Object(id)) = &self.focused
            && focused_is_text_entry(player, id)
        {
            return;
        }
        const KEYS: [(Key, &str); 42] = [
            (Key::A, "a"),
            (Key::B, "b"),
            (Key::C, "c"),
            (Key::D, "d"),
            (Key::E, "e"),
            (Key::F, "f"),
            (Key::G, "g"),
            (Key::H, "h"),
            (Key::I, "i"),
            (Key::J, "j"),
            (Key::K, "k"),
            (Key::L, "l"),
            (Key::M, "m"),
            (Key::N, "n"),
            (Key::O, "o"),
            (Key::P, "p"),
            (Key::Q, "q"),
            (Key::R, "r"),
            (Key::S, "s"),
            (Key::T, "t"),
            (Key::U, "u"),
            (Key::V, "v"),
            (Key::W, "w"),
            (Key::X, "x"),
            (Key::Y, "y"),
            (Key::Z, "z"),
            (Key::Num0, "0"),
            (Key::Num1, "1"),
            (Key::Num2, "2"),
            (Key::Num3, "3"),
            (Key::Num4, "4"),
            (Key::Num5, "5"),
            (Key::Num6, "6"),
            (Key::Num7, "7"),
            (Key::Num8, "8"),
            (Key::Num9, "9"),
            (Key::ArrowUp, "arrow_up"),
            (Key::ArrowDown, "arrow_down"),
            (Key::ArrowLeft, "arrow_left"),
            (Key::ArrowRight, "arrow_right"),
            (Key::Enter, "enter"),
            (Key::Space, "space"),
        ];

        ui.input_mut(|i| {
            for (key, name) in KEYS {
                // Arrows belong to navigation and to the focused control.
                if matches!(
                    key,
                    Key::ArrowUp | Key::ArrowDown | Key::ArrowLeft | Key::ArrowRight
                ) {
                    continue;
                }
                let count = i.count_and_consume_key(Modifiers::NONE, key);
                for _ in 0..count {
                    inputs.push(Input::KeyPressed {
                        key: (*name).to_owned(),
                    });
                }
            }
        });
        // Escape is shared with the host (the editor preview closes on it) and
        // with controls (markers, drags). Only claim it when a trigger listens.
        if player.has_key_trigger("escape") {
            let count = ui.input_mut(|i| i.count_and_consume_key(Modifiers::NONE, Key::Escape));
            for _ in 0..count {
                inputs.push(Input::KeyPressed {
                    key: "escape".to_owned(),
                });
            }
        }
    }

    /// Arrow keys scroll one line, PageUp/PageDown one viewport, and Home/End
    /// the ends of the panel's content.
    fn handle_scroll_panel_keys(
        &mut self,
        ui: &Ui,
        player: &Player<'_>,
        object: &str,
        viewport: f32,
        content_height: f32,
        inputs: &mut Vec<Input>,
    ) {
        let (up, down, page_up, page_down, home, end) = ui.input_mut(|i| {
            let modifiers = i.modifiers;
            (
                i.count_and_consume_key(modifiers, Key::ArrowUp),
                i.count_and_consume_key(modifiers, Key::ArrowDown),
                i.count_and_consume_key(modifiers, Key::PageUp),
                i.count_and_consume_key(modifiers, Key::PageDown),
                i.count_and_consume_key(modifiers, Key::Home),
                i.count_and_consume_key(modifiers, Key::End),
            )
        });
        if up + down + page_up + page_down + home + end == 0 {
            return;
        }
        let current = player.panel_offset(object);
        let line = 40.0f32;
        let page = (viewport.max(0.0) * 0.9).max(line);
        #[allow(clippy::cast_precision_loss)]
        let mut offset = current + down as f32 * line + page_down as f32 * page
            - up as f32 * line
            - page_up as f32 * page;
        if home > 0 {
            offset = 0.0;
        }
        if end > 0 {
            offset = f32::MAX;
        }
        let offset = coursecraft_course::clamp_panel_offset(content_height, viewport, offset);
        if (offset - current).abs() > 0.01 {
            inputs.push(Input::PanelScrolled {
                object: object.to_owned(),
                offset,
            });
        }
    }

    /// Arrow keys step a slider or dial; Shift takes ten steps. Home and End
    /// jump to the range ends.
    fn handle_number_input(
        &mut self,
        ui: &Ui,
        player: &Player<'_>,
        object: &str,
        kind: &ObjectKind,
        inputs: &mut Vec<Input>,
    ) {
        let (variable, min, max, step) = match kind {
            ObjectKind::Slider {
                variable,
                min,
                max,
                step,
                ..
            }
            | ObjectKind::Dial {
                variable,
                min,
                max,
                step,
                ..
            } => (variable, *min, *max, *step),
            _ => return,
        };
        let shift = ui.input(|i| {
            i.modifiers.shift
                || i.events.iter().any(|event| {
                    matches!(
                        event,
                        Event::Key {
                            pressed: true,
                            modifiers,
                            ..
                        } if modifiers.shift
                    )
                })
        });
        let (left, right, down, up, home, end) = ui.input_mut(|i| {
            let modifiers = i.modifiers;
            (
                i.count_and_consume_key(modifiers, Key::ArrowLeft),
                i.count_and_consume_key(modifiers, Key::ArrowRight),
                i.count_and_consume_key(modifiers, Key::ArrowDown),
                i.count_and_consume_key(modifiers, Key::ArrowUp),
                i.count_and_consume_key(modifiers, Key::Home),
                i.count_and_consume_key(modifiers, Key::End),
            )
        });
        if left + right + down + up + home + end == 0 {
            return;
        }
        let current = match player.variable(variable) {
            Some(Value::Number(number)) => *number,
            _ => min,
        };
        let unit = if step > 0.0 {
            step
        } else if max > min {
            (max - min) / 100.0
        } else {
            0.0
        };
        let multiplier = if shift { 10.0 } else { 1.0 };
        #[allow(clippy::cast_precision_loss)]
        let increase = (right as f64 + up as f64) * unit * multiplier;
        #[allow(clippy::cast_precision_loss)]
        let decrease = (left as f64 + down as f64) * unit * multiplier;
        let mut value = current + increase - decrease;
        if home > 0 {
            value = min;
        }
        if end > 0 {
            value = max;
        }
        let value = coursecraft_course::snap_to_step(min, max, step, value);
        if (value - current).abs() > f64::EPSILON {
            inputs.push(Input::SetInputValue {
                object: object.to_owned(),
                value: Value::Number(value),
            });
        }
    }

    /// Arrows move the radio selection within the group.
    fn handle_radio_keys(
        &mut self,
        ui: &Ui,
        player: &Player<'_>,
        object: &str,
        variable: &str,
        options: &[RadioOption],
        inputs: &mut Vec<Input>,
    ) {
        if options.is_empty() {
            return;
        }
        let (left, right, down, up) = ui.input_mut(|i| {
            let modifiers = i.modifiers;
            (
                i.count_and_consume_key(modifiers, Key::ArrowLeft),
                i.count_and_consume_key(modifiers, Key::ArrowRight),
                i.count_and_consume_key(modifiers, Key::ArrowDown),
                i.count_and_consume_key(modifiers, Key::ArrowUp),
            )
        });
        let backwards = left + up;
        let forwards = right + down;
        if backwards + forwards == 0 {
            return;
        }
        let current = match player.variable(variable) {
            Some(Value::Text(text)) => options.iter().position(|option| &option.value == text),
            _ => None,
        };
        let mut index = current.unwrap_or(0);
        let count = options.len();
        for _ in 0..forwards {
            index = (index + 1) % count;
        }
        for _ in 0..backwards {
            index = (index + count - 1) % count;
        }
        if let Some(option) = options.get(index)
            && current != Some(index)
        {
            inputs.push(Input::SetInputValue {
                object: object.to_owned(),
                value: Value::Text(option.value.clone()),
            });
        }
    }

    /// Left/Right step the seek slider by one second; Home/End jump to an end.
    fn handle_seek_keys(&mut self, ui: &Ui, player: &Player<'_>, inputs: &mut Vec<Input>) {
        let left = consume(ui, Key::ArrowLeft);
        let right = consume(ui, Key::ArrowRight);
        let home = consume(ui, Key::Home);
        let end = consume(ui, Key::End);
        if !left && !right && !home && !end {
            return;
        }
        let info = player.seek_info();
        if !info.seek_allowed || info.duration_ms == 0 {
            return;
        }
        let mut target = info.elapsed_ms;
        if left {
            target = target.saturating_sub(1_000);
        }
        if right {
            target = target.saturating_add(1_000).min(info.duration_ms);
        }
        if home {
            target = 0;
        }
        if end {
            target = info.duration_ms;
        }
        if target != info.elapsed_ms {
            inputs.push(Input::PlayerSeek {
                ms: u64::from(target),
            });
        }
    }

    /// Arrows nudge the volume; Home and End go to silence and full.
    fn handle_volume_keys(&mut self, ui: &Ui) {
        let left = consume(ui, Key::ArrowLeft);
        let down = consume(ui, Key::ArrowDown);
        let right = consume(ui, Key::ArrowRight);
        let up = consume(ui, Key::ArrowUp);
        let home = consume(ui, Key::Home);
        let end = consume(ui, Key::End);
        const STEP: f32 = 0.05;
        let delta = (right as i8 + up as i8 - left as i8 - down as i8) as f32 * STEP;
        if home {
            self.chrome_request = Some(ChromeRequest::SetVolume(0.0));
        } else if end {
            self.chrome_request = Some(ChromeRequest::SetVolume(1.0));
        } else if delta != 0.0 {
            self.chrome_request = Some(ChromeRequest::NudgeVolume(delta));
        }
    }

    fn activate(
        &mut self,
        player: &Player<'_>,
        drawer_open: &mut bool,
        inputs: &mut Vec<Input>,
        captions_on: &mut bool,
        notice_dismissed: &mut bool,
    ) {
        if player.is_reviewing()
            && matches!(
                self.focused.as_ref(),
                Some(
                    FocusTarget::Choice(_)
                        | FocusTarget::SequenceMove { .. }
                        | FocusTarget::FillIn
                        | FocusTarget::MatchRow(_)
                        | FocusTarget::Hotspot
                        | FocusTarget::DragItem(_)
                )
            )
        {
            return;
        }
        match self.focused.clone() {
            Some(FocusTarget::Object(id)) => {
                let kind = player
                    .current_slide()
                    .and_then(|slide| slide.object(&id))
                    .map(|(obj, _)| &obj.kind);
                match kind {
                    Some(ObjectKind::Marker { .. }) => {
                        inputs.push(Input::MarkerToggled { object: id });
                    }
                    Some(ObjectKind::Checkbox { variable, .. }) => {
                        let checked = matches!(player.variable(variable), Some(Value::Bool(true)));
                        inputs.push(Input::SetInputValue {
                            object: id,
                            value: Value::Bool(!checked),
                        });
                    }
                    Some(ObjectKind::RadioGroup { options, variable }) => {
                        let current = match player.variable(variable) {
                            Some(Value::Text(text)) => {
                                options.iter().position(|option| &option.value == text)
                            }
                            _ => None,
                        };
                        let next = current.map_or(0, |index| (index + 1) % options.len().max(1));
                        if let Some(option) = options.get(next) {
                            inputs.push(Input::SetInputValue {
                                object: id,
                                value: Value::Text(option.value.clone()),
                            });
                        }
                    }
                    Some(
                        ObjectKind::TextEntry { .. }
                        | ObjectKind::Slider { .. }
                        | ObjectKind::Dial { .. },
                    ) => {}
                    _ => inputs.push(Input::Click { object: id }),
                }
            }
            Some(FocusTarget::Link { object, link, .. }) => {
                inputs.push(Input::LinkClicked { object, link })
            }
            Some(FocusTarget::Choice(id)) => inputs.push(Input::SelectChoice { choice: id }),
            Some(FocusTarget::SequenceMove { choice, up }) => {
                if let Some(to) = sequence_destination(player, &choice, up) {
                    inputs.push(Input::SequenceMoved { choice, to });
                }
            }
            Some(FocusTarget::Chrome(ChromeButton::DismissNotice)) => *notice_dismissed = true,
            Some(FocusTarget::Chrome(ChromeButton::ExitReview)) => {
                inputs.push(Input::ExitReview);
            }
            Some(FocusTarget::Chrome(ChromeButton::PlayPause)) => {
                inputs.extend(play_pause_inputs(player));
            }
            Some(FocusTarget::Chrome(ChromeButton::Replay)) => {
                inputs.extend(replay_inputs(player));
            }
            Some(FocusTarget::Chrome(ChromeButton::Mute)) => {
                self.chrome_request = Some(ChromeRequest::ToggleMute);
            }
            Some(FocusTarget::Chrome(ChromeButton::Seek | ChromeButton::Volume)) => {}
            Some(FocusTarget::Chrome(ChromeButton::Outline)) => {
                *drawer_open = !*drawer_open;
            }
            Some(FocusTarget::Chrome(ChromeButton::Captions)) => {
                *captions_on = !*captions_on;
            }
            Some(FocusTarget::Chrome(ChromeButton::Speed)) => {
                inputs.push(Input::SetSpeed {
                    rate: next_rate(player.speed()),
                });
            }
            Some(FocusTarget::Chrome(ChromeButton::Fullscreen)) => {
                self.chrome_request = Some(ChromeRequest::ToggleFullscreen);
            }
            Some(FocusTarget::Chrome(ChromeButton::Previous)) => inputs.push(Input::Previous),
            Some(FocusTarget::Chrome(ChromeButton::Next)) => {
                if player.can_next() {
                    inputs.push(Input::Next);
                }
            }
            Some(FocusTarget::Chrome(ChromeButton::DrawerClose)) => *drawer_open = false,
            Some(FocusTarget::Chrome(ChromeButton::DrawerTab(tab))) => {
                self.drawer.tab = tab;
                self.drawer.selected = None;
            }
            Some(FocusTarget::Chrome(ChromeButton::ResourceRow(resource))) => {
                inputs.push(Input::OpenResource { resource });
            }
            Some(FocusTarget::Chrome(ChromeButton::GlossaryTerm(term))) => {
                self.drawer.selected = Some(term);
            }
            Some(FocusTarget::Chrome(ChromeButton::GlossarySearch)) => {
                self.chrome_request = Some(ChromeRequest::FocusGlossarySearch);
            }
            Some(FocusTarget::Chrome(ChromeButton::TranscriptText)) => {}
            Some(FocusTarget::Chrome(ChromeButton::DrawerRow(slide))) => {
                let current = player.current_slide().is_some_and(|s| s.id == slide);
                if !current && player.can_go_to(&slide) {
                    inputs.push(Input::GoTo { slide });
                }
            }
            Some(FocusTarget::Chrome(ChromeButton::CloseLightbox)) => {
                inputs.push(Input::CloseLightbox);
            }
            Some(FocusTarget::FillIn)
            | Some(FocusTarget::MatchRow(_))
            | Some(FocusTarget::Hotspot)
            | Some(FocusTarget::DragItem(_))
            | None => {}
        }
    }

    fn cycle_match(
        &self,
        player: &Player<'_>,
        left: &str,
        forwards: bool,
        inputs: &mut Vec<Input>,
    ) {
        let options = player.matching_options();
        if options.is_empty() {
            return;
        }
        let current = player.match_for(left);
        let index = match current {
            None => 0,
            Some(right) => options
                .iter()
                .position(|id| id == right)
                .map_or(0, |i| i + 1),
        };
        let count = options.len() + 1;
        let next = if forwards {
            (index + 1) % count
        } else {
            (index + count - 1) % count
        };
        let right = if next == 0 {
            None
        } else {
            options.get(next - 1).cloned()
        };
        inputs.push(Input::MatchSelected {
            left: left.to_owned(),
            right,
        });
    }

    fn reset_hotspot(&mut self, player: &Player<'_>) {
        let Some(rect) = hotspot_rect(player) else {
            self.hotspot_cursor = None;
            return;
        };
        let point = player
            .state()
            .hotspot_markers
            .last()
            .copied()
            .unwrap_or_else(|| (rect.center().x, rect.center().y));
        self.hotspot_cursor = Some(clamp_point(rect, point.0, point.1));
    }

    fn clamp_hotspot(&mut self, player: &Player<'_>) {
        let Some(rect) = hotspot_rect(player) else {
            return;
        };
        if let Some((x, y)) = self.hotspot_cursor {
            self.hotspot_cursor = Some(clamp_point(rect, x, y));
        }
    }

    fn update_drag_targets(&mut self, player: &Player<'_>) {
        let Some(drag) = self.drag.as_mut() else {
            return;
        };
        let still_focused = if drag.is_draggable_object {
            matches!(
                &self.focused,
                Some(FocusTarget::Object(id)) if id == &drag.item
            )
        } else {
            matches!(
                &self.focused,
                Some(FocusTarget::DragItem(id)) if id == &drag.item
            )
        };
        if !still_focused {
            self.drag = None;
            return;
        }
        if drag.is_draggable_object {
            let targets = player
                .current_slide()
                .and_then(|s| s.object(&drag.item))
                .and_then(|(o, _)| o.drag.as_ref())
                .map(|d| d.targets.clone())
                .unwrap_or_default();
            if targets != drag.targets {
                let old = drag.current_target().map(str::to_owned);
                drag.index = old
                    .and_then(|id| targets.iter().position(|t| *t == id))
                    .unwrap_or_else(|| drag.index.min(targets.len()));
                drag.targets = targets;
            }
            return;
        }
        if player.question_locked() {
            self.drag = None;
            return;
        }
        let targets = open_targets(player);
        if targets.is_empty()
            && player
                .current_slide()
                .and_then(|s| s.question.as_ref())
                .is_none_or(|q| q.kind != QuestionKind::DragDrop)
        {
            self.drag = None;
            return;
        }
        if targets != drag.targets {
            let old = drag.current_target().map(str::to_owned);
            drag.index = old
                .and_then(|id| targets.iter().position(|t| *t == id))
                .unwrap_or_else(|| drag.index.min(targets.len()));
            drag.targets = targets;
        }
    }

    fn handle_drag_keys(&mut self, ui: &Ui, player: &Player<'_>, inputs: &mut Vec<Input>) {
        if consume(ui, Key::Escape) {
            self.drag = None;
            return;
        }
        let Some(drag) = self.drag.as_mut() else {
            return;
        };
        let backwards = if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Tab)) {
            false
        } else if ui.input_mut(|i| i.consume_key(Modifiers::SHIFT, Key::Tab)) {
            true
        } else {
            consume(ui, Key::ArrowUp) || consume(ui, Key::ArrowLeft)
        };
        let forwards = consume(ui, Key::ArrowDown) || consume(ui, Key::ArrowRight);
        if backwards || forwards {
            let count = drag.targets.len() + 1;
            drag.index = if backwards {
                (drag.index + count - 1) % count
            } else {
                (drag.index + 1) % count
            };
            return;
        }
        if consume(ui, Key::Enter) || consume(ui, Key::Space) {
            let item = drag.item.clone();
            let target = drag.current_target().map(str::to_owned);
            if drag.is_draggable_object {
                if let Some((x, y)) = drop_position(player, &item, target.as_deref()) {
                    inputs.push(Input::ObjectDropped { object: item, x, y });
                }
            } else {
                if let Some((x, y)) = drop_position(player, &item, target.as_deref()) {
                    inputs.push(Input::DragDropped { item, target, x, y });
                }
            }
            self.drag = None;
        }
    }

    /// Paint the focus ring, hotspot cursor or keyboard drag overlay.
    pub fn paint(
        &mut self,
        ui: &Ui,
        player: &Player<'_>,
        items: &[DrawItem],
        transform: Option<&SlideTransform>,
    ) {
        self.last_ring = None;
        if !self.keyboard_mode {
            return;
        }
        let Some(transform) = transform else {
            return;
        };
        let accent = parse_color(&player.course().theme.accent).unwrap_or(FALLBACK_ACCENT);
        let contrast = ring_contrast(player);
        let painter = ui.painter().with_clip_rect(transform.slide_rect());

        if matches!(self.focused, Some(FocusTarget::Hotspot))
            && let Some((x, y)) = self.hotspot_cursor
        {
            let center = transform.to_screen_pos(pos2(x, y));
            let radius = (12.0 * transform.scale()).max(6.0);
            painter.circle_stroke(center, radius + 1.0, Stroke::new(1.0, contrast));
            painter.circle_stroke(center, radius, Stroke::new(2.0, accent));
            self.last_ring = Some(Rect::from_center_size(
                center,
                vec2(radius * 2.0, radius * 2.0),
            ));
            return;
        }

        let Some(target) = self.focused.as_ref() else {
            return;
        };
        // Objects (including rotated and flipped ones) get a ring on their
        // exact frame. Question chrome keeps its axis-aligned rect.
        let focused_item = match target {
            FocusTarget::Object(id) | FocusTarget::DragItem(id) => {
                items.iter().rev().find(|item| match &item.hit {
                    Hit::Object(hit) | Hit::DragItem(hit) => hit == id,
                    Hit::HotspotImage => {
                        player
                            .current_slide()
                            .and_then(|s| s.question.as_ref())
                            .and_then(|q| q.hotspot_image.as_deref())
                            == Some(id.as_str())
                    }
                    _ => false,
                })
            }
            _ => None,
        };
        if let Some(item) = focused_item {
            let corners =
                crate::compose::item_corners(item).map(|point| transform.to_screen_pos(point));
            draw_ring_corners(&painter, corners, accent, contrast, transform.scale());
            self.last_ring = Some(transform.to_screen_rect(crate::compose::item_bounds(item)));
            return;
        }
        let Some(rect) = target_rect(player, target, items) else {
            return;
        };
        let screen = transform.to_screen_rect(rect);
        draw_ring(&painter, screen, accent, contrast, transform.scale());
        self.last_ring = Some(screen);
    }

    /// Paint the ring around one chrome button when it owns focus.
    pub fn paint_chrome(
        &self,
        ui: &Ui,
        rect: Rect,
        button: &ChromeButton,
        accent: Color32,
        background: Color32,
    ) {
        if !self.keyboard_mode
            || self.focused.as_ref() != Some(&FocusTarget::Chrome(button.clone()))
        {
            return;
        }
        let contrast = contrast_for(background);
        draw_ring(ui.painter(), rect.shrink(1.0), accent, contrast, 1.0);
    }

    /// The chrome button that currently owns focus.
    #[must_use]
    pub fn chrome_focused(&self) -> Option<&ChromeButton> {
        match self.focused.as_ref() {
            Some(FocusTarget::Chrome(button)) => Some(button),
            _ => None,
        }
    }
}

/// The rate after `current` in the speed list, wrapping to the slowest.
pub(crate) fn next_rate(current: f32) -> f32 {
    let rates = coursecraft_logic::PLAYBACK_RATES;
    rates
        .iter()
        .copied()
        .find(|rate| *rate > current + 1e-3)
        .unwrap_or(rates[0])
}

fn ring_contrast(player: &Player<'_>) -> Color32 {
    let background = player
        .current_slide()
        .map(|s| player.course().slide_background(s))
        .unwrap_or(&player.course().theme.background);
    contrast_for(parse_color(background).unwrap_or(Color32::WHITE))
}

fn contrast_for(background: Color32) -> Color32 {
    if relative_luminance(background) > 0.5 {
        DARK_RING
    } else {
        LIGHT_RING
    }
}

fn draw_ring(painter: &egui::Painter, rect: Rect, accent: Color32, contrast: Color32, scale: f32) {
    let inner = (2.0 * scale).max(1.0);
    let outer = (1.0 * scale).max(1.0);
    painter.rect_stroke(
        rect.expand(inner),
        egui::CornerRadius::ZERO,
        Stroke::new(outer, contrast),
        StrokeKind::Outside,
    );
    painter.rect_stroke(
        rect,
        egui::CornerRadius::ZERO,
        Stroke::new(inner, accent),
        StrokeKind::Outside,
    );
}

fn draw_ring_corners(
    painter: &egui::Painter,
    corners: [egui::Pos2; 4],
    accent: Color32,
    contrast: Color32,
    scale: f32,
) {
    let inner = (2.0 * scale).max(1.0);
    let outer = (1.0 * scale).max(1.0);
    painter.add(egui::Shape::closed_line(
        corners.to_vec(),
        Stroke::new(inner + outer * 2.0, contrast),
    ));
    painter.add(egui::Shape::closed_line(
        corners.to_vec(),
        Stroke::new(inner, accent),
    ));
}

fn target_rect(player: &Player<'_>, target: &FocusTarget, items: &[DrawItem]) -> Option<Rect> {
    match target {
        FocusTarget::Object(id) | FocusTarget::DragItem(id) => {
            items.iter().rev().find_map(|item| match &item.hit {
                Hit::Object(hit) | Hit::DragItem(hit) if hit == id => Some(item.rect),
                Hit::HotspotImage
                    if player
                        .current_slide()
                        .and_then(|s| s.question.as_ref())
                        .and_then(|q| q.hotspot_image.as_deref())
                        == Some(id.as_str()) =>
                {
                    Some(item.rect)
                }
                _ => None,
            })
        }
        FocusTarget::Link { object, index, .. } => items.iter().rev().find_map(|item| {
            if !matches!(&item.hit, Hit::Object(hit) if hit == object) {
                return None;
            }
            let link = item.links.get(*index)?;
            let width = item.rect.width();
            let height = item.rect.height();
            Some(Rect::from_min_max(
                egui::pos2(
                    item.rect.min.x + link.rect.min.x * width,
                    item.rect.min.y + link.rect.min.y * height,
                ),
                egui::pos2(
                    item.rect.min.x + link.rect.max.x * width,
                    item.rect.min.y + link.rect.max.y * height,
                ),
            ))
        }),
        FocusTarget::Choice(id) => items.iter().rev().find_map(|item| match &item.hit {
            Hit::Choice(hit) if hit == id => Some(item.rect),
            _ => None,
        }),
        FocusTarget::SequenceMove { choice, up } => {
            let to = sequence_destination(player, choice, *up)?;
            items.iter().rev().find_map(|item| match &item.hit {
                Hit::SequenceMove { choice: c, to: t } if c == choice && *t == to => {
                    Some(item.rect)
                }
                _ => None,
            })
        }
        FocusTarget::FillIn => items.iter().find_map(|item| match &item.hit {
            Hit::QuestionChrome(id) if id == "fill-in" => Some(item.rect),
            _ => None,
        }),
        FocusTarget::MatchRow(left) => items.iter().find_map(|item| match &item.hit {
            Hit::QuestionChrome(id) if id == left => Some(item.rect),
            _ => None,
        }),
        FocusTarget::Hotspot => hotspot_rect(player),
        FocusTarget::Chrome(_) => None,
    }
}

fn sequence_destination(player: &Player<'_>, choice: &str, up: bool) -> Option<usize> {
    let order = player.sequence_order();
    let index = order.iter().position(|id| id == choice)?;
    if up {
        index.checked_sub(1)
    } else {
        let next = index.checked_add(1)?;
        (next < order.len()).then_some(next)
    }
}

fn hotspot_rect(player: &Player<'_>) -> Option<Rect> {
    let slide = player.current_slide()?;
    let question = slide.question.as_ref()?;
    if question.kind != QuestionKind::Hotspot {
        return None;
    }
    let id = question.hotspot_image.as_deref()?;
    let (object, _) = slide.object(id)?;
    let mut rect = validate_frame(&object.frame)?;
    if let Some(sample) = player.sample_object(id) {
        if sample.opacity <= 0.0 {
            return None;
        }
        rect = rect.translate(vec2(sample.offset.0, sample.offset.1));
        let center = rect.center();
        let half = rect.size() * 0.5 * sample.scale;
        rect = Rect::from_min_max(center - half, center + half);
    }
    Some(rect)
}

fn clamp_point(rect: Rect, x: f32, y: f32) -> (f32, f32) {
    let x = if x.is_finite() {
        x.clamp(rect.left(), rect.right())
    } else {
        rect.center().x
    };
    let y = if y.is_finite() {
        y.clamp(rect.top(), rect.bottom())
    } else {
        rect.center().y
    };
    (x, y)
}

fn drop_position(player: &Player<'_>, item: &str, target: Option<&str>) -> Option<(f32, f32)> {
    let slide = player.current_slide()?;
    let (object, _) = slide.object(item)?;
    let frame = object.frame;
    if !frame.x.is_finite() || !frame.y.is_finite() || !frame.w.is_finite() || !frame.h.is_finite()
    {
        return None;
    }
    match target {
        Some(id) => {
            let bounds = player.object_current_bounds(id)?;
            if !bounds.x.is_finite()
                || !bounds.y.is_finite()
                || !bounds.w.is_finite()
                || !bounds.h.is_finite()
            {
                return None;
            }
            Some((
                bounds.x + bounds.w * 0.5 - frame.w * 0.5,
                bounds.y + bounds.h * 0.5 - frame.h * 0.5,
            ))
        }
        None => Some((frame.x, frame.y)),
    }
}

fn any_focus_key(ui: &Ui) -> bool {
    ui.input(|i| {
        i.events.iter().any(|event| {
            matches!(
                event,
                Event::Key {
                    pressed: true,
                    key: Key::Tab
                        | Key::Enter
                        | Key::Space
                        | Key::ArrowUp
                        | Key::ArrowDown
                        | Key::ArrowLeft
                        | Key::ArrowRight,
                    ..
                }
            )
        })
    })
}

/// Does this frame hold a key that turns on the focus ring?
///
/// Arrows don't count: they already navigate slides with no focus shown. Enter
/// and Space don't count while a text field is taking the learner's typing.
fn reveal_key(ui: &Ui) -> bool {
    let typing = ui.ctx().text_edit_focused();
    ui.input(|i| {
        i.events.iter().any(|event| {
            matches!(event, Event::Key { pressed: true, key, .. }
                if *key == Key::Tab || (!typing && matches!(key, Key::Enter | Key::Space)))
        })
    })
}

fn consume_reveal_keys(ui: &Ui) {
    ui.input_mut(|i| {
        i.consume_key(Modifiers::NONE, Key::Tab);
        i.consume_key(Modifiers::SHIFT, Key::Tab);
        i.consume_key(Modifiers::NONE, Key::Enter);
        i.consume_key(Modifiers::NONE, Key::Space);
    });
}

fn consume(ui: &Ui, key: Key) -> bool {
    ui.input_mut(|i| i.consume_key(Modifiers::NONE, key))
}

/// The image frame rect helper used by keyboard focus tests.
#[must_use]
pub fn hotspot_frame(player: &Player<'_>) -> Option<Rect> {
    hotspot_rect(player)
}

#[cfg(test)]
mod tests;
