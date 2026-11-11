//! Slide view widget and pointer input handling.

use coursecraft_course::{
    Color, Orientation, Question, QuestionKind, Transition, TransitionEffect, Value,
};
use coursecraft_logic::{Input, Player};
use egui::{Color32, CornerRadius, Event, PointerButton, Pos2, Rect, Stroke, StrokeKind, Ui};

use crate::compose::{DrawItem, Hit, Visual, compose};
use crate::layout::SlideTransform;
use crate::media::{MediaCache, MediaSource};
use crate::paint::{self, Highlight};

/// The lightbox slide is scaled to this fraction of the slide rect.
pub const LIGHTBOX_SCALE: f32 = 0.85;
/// Opacity of the black backdrop over the base slide.
pub const LIGHTBOX_BACKDROP: f32 = 0.55;
/// Seconds the backdrop takes to fade in.
pub const LIGHTBOX_FADE_SECONDS: f64 = 0.18;

/// The screen rect of the box that holds a lightboxed slide.
#[must_use]
pub fn lightbox_box_rect(slide_rect: Rect, slide_size: coursecraft_course::Size) -> Option<Rect> {
    if !slide_rect.is_positive()
        || !slide_rect.is_finite()
        || slide_size.w == 0
        || slide_size.h == 0
    {
        return None;
    }
    let size = egui::vec2(
        slide_rect.width() * LIGHTBOX_SCALE,
        slide_rect.height() * LIGHTBOX_SCALE,
    );
    if !size.x.is_finite() || !size.y.is_finite() || size.x <= 0.0 || size.y <= 0.0 {
        return None;
    }
    Some(Rect::from_center_size(slide_rect.center(), size))
}

/// The close button at the top-right of a lightbox box.
#[must_use]
pub fn lightbox_close_rect(box_rect: Rect) -> Option<Rect> {
    const BUTTON: f32 = 32.0;
    const INSET: f32 = 8.0;
    if !box_rect.is_positive() || !box_rect.is_finite() {
        return None;
    }
    Some(Rect::from_min_size(
        egui::pos2(box_rect.right() - INSET - BUTTON, box_rect.top() + INSET),
        egui::vec2(BUTTON, BUTTON),
    ))
}

/// Find the topmost hit target under `slide_pos` in slide coordinates.
///
/// Returns `None` if `slide_pos` contains non-finite numbers, or if no item
/// with a non-[`Hit::None`] target contains the point. A text link wins over
/// the object that carries it.
#[must_use]
pub fn hit_test(items: &[DrawItem], slide_pos: Pos2) -> Option<Hit> {
    if !slide_pos.x.is_finite() || !slide_pos.y.is_finite() {
        return None;
    }
    for item in items.iter().rev() {
        if let Some(link) = crate::compose::item_link_at(item, slide_pos)
            && let Hit::Object(object) = &item.hit
        {
            return Some(Hit::TextLink {
                object: object.clone(),
                link: link.link.clone(),
            });
        }
        if item.hit != Hit::None && crate::compose::item_hit_test(item, slide_pos) {
            return Some(item.hit.clone());
        }
    }
    None
}

/// A drag gesture that has not been released yet. Render-only; the runtime hears `drag_dropped` or `object_dropped`.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemDrag {
    pub item: String,
    /// Composed rect at the press, in slide coordinates.
    pub origin: egui::Rect,
    /// Press position in slide coordinates.
    pub press: Pos2,
    /// Latest pointer position in slide coordinates.
    pub current: Pos2,
    /// Whether this is a draggable object drag (vs question drag item).
    pub is_draggable_object: bool,
    /// Targets this drag can drop on (for draggable objects).
    pub targets: Vec<String>,
    /// The slide rectangle in slide coordinates, used to keep a draggable
    /// object on the slide while it follows the pointer.
    pub bounds: Option<egui::Rect>,
}

/// A slider or dial being dragged. Render-only; the runtime hears `set_input_value`.
#[derive(Debug, Clone, PartialEq)]
pub struct ValueDrag {
    pub object: String,
    pub last: f64,
}

/// A scrolling panel being dragged. Render-only; the runtime hears `panel_scrolled`.
#[derive(Debug, Clone, PartialEq)]
pub struct PanelDrag {
    pub object: String,
    pub start_offset: f32,
    pub start: Pos2,
    pub viewport: f32,
    pub content_height: f32,
    /// True when the gesture began on the scrollbar thumb.
    pub on_thumb: bool,
    /// Slide pixels of scroll per slide pixel of pointer travel.
    pub ratio: f32,
    pub moved: bool,
}

/// Presentational pointer state machine for the slide view.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PointerState {
    pub hovered: Option<Hit>,
    pub pressed: Option<Hit>,
    /// Active drag of a drag-and-drop item. Escape or a disappearing item cancels it.
    pub drag: Option<ItemDrag>,
    /// Slider or dial being dragged.
    pub value_drag: Option<ValueDrag>,
    /// Scrolling panel content or scrollbar being dragged.
    pub panel_drag: Option<PanelDrag>,
    /// Drop-target ids for the unlocked drag-and-drop question on this frame.
    pub drop_targets: Vec<String>,
    /// Draggable object specs for objects that can be dragged: `(object_id, targets)`.
    pub draggable_specs: Vec<(String, Vec<String>)>,
    /// The slide rectangle in slide coordinates, if known.
    pub slide_bounds: Option<egui::Rect>,
    /// Object whose hover the keyboard focus is holding, if any.
    pub(crate) keyboard_hover: Option<String>,
    /// Last pointer position seen, for wheel events (which carry no position).
    pub(crate) last_pointer: Option<Pos2>,
    /// Whether a marker label is open, so a background click closes it.
    pub(crate) markers_open: bool,
}

impl PointerState {
    /// Create a new, empty pointer state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Drop hovered or pressed items if they are no longer in `items`.
    ///
    /// When the active slide changes, old hover/pressed objects vanish from
    /// composition and are dropped silently without emitting [`Input::HoverEnd`].
    pub fn prune_uncomposed(&mut self, items: &[DrawItem]) {
        if let Some(ref hit) = self.hovered
            && !items.iter().any(|item| &item.hit == hit)
        {
            self.hovered = None;
        }
        if let Some(ref hit) = self.pressed
            && !items.iter().any(|item| &item.hit == hit)
        {
            self.pressed = None;
        }
        if let Some(drag) = &self.drag {
            let alive = items.iter().any(|item| {
                if drag.is_draggable_object {
                    matches!(&item.hit, Hit::Object(id) if id == &drag.item)
                } else {
                    matches!(&item.hit, Hit::DragItem(id) if id == &drag.item)
                }
            });
            if !alive {
                self.drag = None;
                self.pressed = None;
            }
        }
        if let Some(drag) = &self.value_drag
            && !items
                .iter()
                .any(|item| matches!(&item.hit, Hit::Object(id) if id == &drag.object))
        {
            self.value_drag = None;
            self.pressed = None;
        }
        if let Some(drag) = &self.panel_drag
            && !items
                .iter()
                .any(|item| matches!(&item.hit, Hit::Object(id) if id == &drag.object))
        {
            self.panel_drag = None;
            self.pressed = None;
        }
    }

    /// Process a single egui event and append any resulting runtime inputs to `out`.
    #[allow(clippy::cognitive_complexity)]
    pub fn handle_event(
        &mut self,
        event: &Event,
        t: &SlideTransform,
        items: &[DrawItem],
        covered: &dyn Fn(Pos2) -> bool,
        out: &mut Vec<Input>,
    ) {
        match event {
            Event::MouseWheel { unit, delta, .. } => {
                let Some(pos) = self.last_pointer else {
                    return;
                };
                if !t.slide_rect().contains(pos) || covered(pos) {
                    return;
                }
                let slide_pos = t.to_slide_pos(pos);
                let Some((object, offset, content, viewport, _)) = panel_at(items, slide_pos)
                else {
                    return;
                };
                let unit = match unit {
                    egui::MouseWheelUnit::Point => 1.0,
                    egui::MouseWheelUnit::Line => 30.0,
                    egui::MouseWheelUnit::Page => viewport.max(40.0),
                };
                let scale = t.scale().max(1e-6);
                let delta_slide = delta.y * unit / scale;
                if !delta_slide.is_finite() {
                    return;
                }
                let next =
                    coursecraft_course::clamp_panel_offset(content, viewport, offset - delta_slide);
                out.push(Input::PanelScrolled {
                    object: object.to_owned(),
                    offset: next,
                });
            }
            Event::PointerMoved(pos) => {
                self.last_pointer = Some(*pos);
                let slide_pos = t.to_slide_pos(*pos);
                if let Some(drag) = &mut self.drag {
                    if slide_pos.x.is_finite() && slide_pos.y.is_finite() {
                        drag.current = slide_pos;
                        clamp_draggable(drag);
                    }
                    return;
                }
                if let Some(drag) = &mut self.value_drag {
                    if slide_pos.x.is_finite()
                        && slide_pos.y.is_finite()
                        && let Some(item) = items
                            .iter()
                            .find(|item| matches!(&item.hit, Hit::Object(id) if id == &drag.object))
                        && let Some(Value::Number(number)) = input_value_at(item, slide_pos)
                        && (number - drag.last).abs() > f64::EPSILON
                    {
                        drag.last = number;
                        out.push(Input::SetInputValue {
                            object: drag.object.clone(),
                            value: Value::Number(number),
                        });
                    }
                    return;
                }
                if let Some(drag) = &mut self.panel_drag {
                    if slide_pos.x.is_finite() && slide_pos.y.is_finite() {
                        let delta = slide_pos.y - drag.start.y;
                        if !drag.moved && delta.abs() > 3.0 {
                            drag.moved = true;
                        }
                        if drag.moved {
                            let offset = drag.start_offset - delta * drag.ratio;
                            let offset = coursecraft_course::clamp_panel_offset(
                                drag.content_height,
                                drag.viewport,
                                offset,
                            );
                            out.push(Input::PanelScrolled {
                                object: drag.object.clone(),
                                offset,
                            });
                        }
                    }
                    return;
                }
                if t.slide_rect().contains(*pos) && !covered(*pos) {
                    let slide_pos = t.to_slide_pos(*pos);
                    let hit = hit_test(items, slide_pos);
                    match hit {
                        Some(Hit::Object(id)) => {
                            let held_by_keyboard =
                                self.keyboard_hover.as_deref() == Some(id.as_str());
                            if self.hovered.as_ref() == Some(&Hit::Object(id.clone())) {
                                // The pointer already holds this hover.
                            } else if held_by_keyboard {
                                // Keyboard focus holds it; do not duplicate hover_start.
                                if let Some(Hit::Object(old)) = self.hovered.take()
                                    && self.keyboard_hover.as_deref() != Some(old.as_str())
                                {
                                    out.push(Input::HoverEnd { object: old });
                                }
                            } else {
                                if let Some(Hit::Object(old)) = self.hovered.take()
                                    && self.keyboard_hover.as_deref() != Some(old.as_str())
                                {
                                    out.push(Input::HoverEnd { object: old });
                                }
                                out.push(Input::HoverStart { object: id.clone() });
                                self.hovered = Some(Hit::Object(id.clone()));
                            }
                        }
                        Some(Hit::Choice(_))
                        | Some(Hit::SequenceMove { .. })
                        | Some(Hit::QuestionChrome(_))
                        | Some(Hit::HotspotImage)
                        | Some(Hit::DragItem(_))
                        | Some(Hit::TextLink { .. })
                        | Some(Hit::None)
                        | None => {
                            if let Some(Hit::Object(old)) = self.hovered.take()
                                && self.keyboard_hover.as_deref() != Some(old.as_str())
                            {
                                out.push(Input::HoverEnd { object: old });
                            }
                            self.hovered = None;
                        }
                    }
                } else {
                    if let Some(Hit::Object(old)) = self.hovered.take()
                        && self.keyboard_hover.as_deref() != Some(old.as_str())
                    {
                        out.push(Input::HoverEnd { object: old });
                    }
                    self.hovered = None;
                }
            }
            Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                ..
            } => {
                let hit = if t.slide_rect().contains(*pos) && !covered(*pos) {
                    let slide_pos = t.to_slide_pos(*pos);
                    hit_test(items, slide_pos)
                } else {
                    None
                };

                if *pressed {
                    let slide_pos = t.to_slide_pos(*pos);
                    self.pressed = hit.clone();
                    self.drag = None;
                    self.value_drag = None;
                    self.panel_drag = None;
                    if let Some(Hit::DragItem(id)) = &hit
                        && slide_pos.x.is_finite()
                        && slide_pos.y.is_finite()
                        && let Some(item) = items
                            .iter()
                            .rev()
                            .find(|it| matches!(&it.hit, Hit::DragItem(drag_id) if drag_id == id))
                    {
                        self.drag = Some(ItemDrag {
                            item: id.clone(),
                            origin: item.rect,
                            press: slide_pos,
                            current: slide_pos,
                            is_draggable_object: false,
                            targets: self.drop_targets.clone(),
                            bounds: self.slide_bounds,
                        });
                    }
                    if let Some(Hit::Object(id)) = &hit
                        && slide_pos.x.is_finite()
                        && slide_pos.y.is_finite()
                        && let Some((_, targets)) =
                            self.draggable_specs.iter().find(|(obj_id, _)| obj_id == id)
                        && let Some(item) = items
                            .iter()
                            .rev()
                            .find(|it| matches!(&it.hit, Hit::Object(hit_id) if hit_id == id))
                    {
                        self.drag = Some(ItemDrag {
                            item: id.clone(),
                            origin: item.rect,
                            press: slide_pos,
                            current: slide_pos,
                            is_draggable_object: true,
                            targets: targets.clone(),
                            bounds: self.slide_bounds,
                        });
                    }
                    if let Some(Hit::Object(id)) = &hit
                        && slide_pos.x.is_finite()
                        && slide_pos.y.is_finite()
                        && let Some(item) = items
                            .iter()
                            .rev()
                            .find(|it| matches!(&it.hit, Hit::Object(hit_id) if hit_id == id))
                        && matches!(item.visual, Visual::Slider { .. } | Visual::Dial { .. })
                        && let Some(Value::Number(number)) = input_value_at(item, slide_pos)
                    {
                        out.push(Input::SetInputValue {
                            object: id.clone(),
                            value: Value::Number(number),
                        });
                        self.value_drag = Some(ValueDrag {
                            object: id.clone(),
                            last: number,
                        });
                    }
                    let starts_control = match &hit {
                        Some(Hit::DragItem(_))
                        | Some(Hit::Choice(_))
                        | Some(Hit::SequenceMove { .. }) => true,
                        Some(Hit::Object(id)) => items.iter().rev().any(|item| {
                            matches!(&item.hit, Hit::Object(hit_id) if hit_id == id)
                                && matches!(
                                    item.visual,
                                    Visual::Slider { .. }
                                        | Visual::Dial { .. }
                                        | Visual::TextEntry { .. }
                                )
                        }),
                        _ => false,
                    };
                    if !starts_control
                        && slide_pos.x.is_finite()
                        && slide_pos.y.is_finite()
                        && let Some((panel, offset, content, viewport, thumb)) =
                            panel_at(items, slide_pos)
                    {
                        let on_thumb = thumb.is_some_and(|thumb| thumb.contains(slide_pos));
                        let max_offset = (content - viewport).max(0.0);
                        let thumb_travel = thumb.map_or(viewport.max(1.0), |thumb| {
                            (viewport - thumb.height()).max(1.0)
                        });
                        let ratio = if on_thumb {
                            (max_offset / thumb_travel).max(0.0)
                        } else {
                            1.0
                        };
                        self.panel_drag = Some(PanelDrag {
                            object: panel.to_owned(),
                            start_offset: offset,
                            start: slide_pos,
                            viewport,
                            content_height: content,
                            on_thumb,
                            ratio,
                            moved: false,
                        });
                    }
                } else {
                    let was_pressed = self.pressed.take();
                    let panel_moved = self.panel_drag.take().is_some_and(|drag| drag.moved);
                    let background_click =
                        self.markers_open && was_pressed.is_none() && hit.is_none();
                    if let Some(drag) = self.drag.take() {
                        let is_draggable = drag.is_draggable_object;
                        let moved = (drag.current - drag.press).length() > 3.0;
                        if is_draggable && !moved {
                            // A press on a draggable object with <= 3 px movement stays an ordinary click.
                            out.push(Input::Click { object: drag.item });
                        } else if is_draggable {
                            let mut drag = drag;
                            let slide_pos = t.to_slide_pos(*pos);
                            if slide_pos.x.is_finite() && slide_pos.y.is_finite() {
                                drag.current = slide_pos;
                                clamp_draggable(&mut drag);
                            }
                            let delta = drag.current - drag.press;
                            let new_min = drag.origin.min + delta;
                            if new_min.x.is_finite() && new_min.y.is_finite() {
                                out.push(Input::ObjectDropped {
                                    object: drag.item,
                                    x: new_min.x,
                                    y: new_min.y,
                                });
                            }
                        } else {
                            emit_drag_drop(drag, *pos, t, items, &self.drop_targets, covered, out);
                        }
                    } else if self.value_drag.take().is_some() {
                        // A slider or dial owns the gesture; no click follows.
                    } else if panel_moved {
                        // A panel drag owns the gesture; no click follows.
                    } else if let Some(pressed_hit) = was_pressed
                        && hit == Some(pressed_hit.clone())
                    {
                        match pressed_hit {
                            Hit::Object(object) => {
                                let item = items.iter().rev().find(
                                    |it| matches!(&it.hit, Hit::Object(hit_id) if hit_id == &object),
                                );
                                let input = item
                                    .and_then(|item| input_value_at(item, t.to_slide_pos(*pos)));
                                match input {
                                    Some(value) => out.push(Input::SetInputValue { object, value }),
                                    // A text field takes typing, not a click trigger.
                                    None if item.is_some_and(|item| {
                                        matches!(item.visual, Visual::TextEntry { .. })
                                    }) => {}
                                    None if item.is_some_and(|item| {
                                        matches!(item.visual, Visual::Marker { .. })
                                    }) =>
                                    {
                                        out.push(Input::MarkerToggled { object })
                                    }
                                    None => out.push(Input::Click { object }),
                                }
                            }
                            Hit::TextLink { object, link } => {
                                out.push(Input::LinkClicked { object, link });
                            }
                            Hit::Choice(choice) => {
                                out.push(Input::SelectChoice { choice });
                            }
                            Hit::SequenceMove { choice, to } => {
                                out.push(Input::SequenceMoved { choice, to });
                            }
                            Hit::HotspotImage => {
                                let slide_pos = t.to_slide_pos(*pos);
                                if slide_pos.x.is_finite() && slide_pos.y.is_finite() {
                                    out.push(Input::HotspotClicked {
                                        x: slide_pos.x,
                                        y: slide_pos.y,
                                    });
                                }
                            }
                            Hit::DragItem(_) | Hit::QuestionChrome(_) | Hit::None => {}
                        }
                    }
                    if background_click {
                        out.push(Input::MarkersClosed);
                    }
                }
            }
            Event::Key {
                key: egui::Key::Escape,
                pressed: true,
                ..
            } if self.drag.is_some() => {
                self.drag = None;
                self.pressed = None;
            }
            Event::PointerGone => {
                self.last_pointer = None;
                if let Some(Hit::Object(old)) = self.hovered.take()
                    && self.keyboard_hover.as_deref() != Some(old.as_str())
                {
                    out.push(Input::HoverEnd { object: old });
                }
                self.hovered = None;
                self.pressed = None;
                self.drag = None;
                self.panel_drag = None;
            }
            _ => {}
        }
    }

    /// Process a batch of egui events and return the resulting runtime inputs in event order.
    pub fn handle_events(
        &mut self,
        events: &[Event],
        t: &SlideTransform,
        items: &[DrawItem],
        covered: &dyn Fn(Pos2) -> bool,
    ) -> Vec<Input> {
        let mut out = Vec::new();
        for event in events {
            self.handle_event(event, t, items, covered, &mut out);
        }
        out
    }

    /// Returns the presentational highlight for painting.
    #[must_use]
    pub fn highlight(&self) -> Highlight {
        Highlight {
            hovered: self.hovered.clone(),
            pressed: self.pressed.clone(),
        }
    }
}

/// The topmost scrolling panel under `slide_pos`, with its state.
fn panel_at(items: &[DrawItem], slide_pos: Pos2) -> Option<(String, f32, f32, f32, Option<Rect>)> {
    if !slide_pos.x.is_finite() || !slide_pos.y.is_finite() {
        return None;
    }
    for item in items.iter().rev() {
        let Visual::ScrollPanel {
            content_height,
            offset,
            thumb,
            ..
        } = &item.visual
        else {
            continue;
        };
        if !item.rect.contains(slide_pos) {
            continue;
        }
        let Hit::Object(id) = &item.hit else {
            continue;
        };
        return Some((
            id.clone(),
            *offset,
            *content_height,
            item.rect.height(),
            *thumb,
        ));
    }
    None
}

/// The value a press at `slide_pos` would commit on an input item.
fn input_value_at(item: &DrawItem, slide_pos: Pos2) -> Option<Value> {
    if !slide_pos.x.is_finite() || !slide_pos.y.is_finite() {
        return None;
    }
    match &item.visual {
        Visual::Slider {
            min,
            max,
            step,
            orientation,
            ..
        } => {
            let rect = item.rect;
            if rect.width() <= 0.0 || rect.height() <= 0.0 {
                return None;
            }
            let fraction = match orientation {
                Orientation::Horizontal => {
                    ((slide_pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0)
                }
                Orientation::Vertical => {
                    (1.0 - (slide_pos.y - rect.top()) / rect.height()).clamp(0.0, 1.0)
                }
            };
            let raw = min + f64::from(fraction) * (max - min);
            Some(Value::Number(coursecraft_course::snap_to_step(
                *min, *max, *step, raw,
            )))
        }
        Visual::Dial {
            min,
            max,
            step,
            start_angle,
            end_angle,
            ..
        } => {
            let center = item.rect.center();
            let dx = slide_pos.x - center.x;
            let dy = slide_pos.y - center.y;
            if !dx.is_finite() || !dy.is_finite() || (dx == 0.0 && dy == 0.0) {
                return None;
            }
            let degrees = f64::from(dx.atan2(-dy).to_degrees());
            let start = f64::from(*start_angle);
            let span = (f64::from(*end_angle) - start).max(f64::EPSILON);
            let relative = (degrees - start).rem_euclid(360.0);
            let fraction = if relative > span {
                if relative - span < 360.0 - relative {
                    1.0
                } else {
                    0.0
                }
            } else {
                (relative / span).clamp(0.0, 1.0)
            };
            let raw = min + fraction * (max - min);
            Some(Value::Number(coursecraft_course::snap_to_step(
                *min, *max, *step, raw,
            )))
        }
        Visual::Checkbox { checked, .. } => Some(Value::Bool(!checked)),
        Visual::RadioGroup { options } => {
            if options.is_empty() || item.rect.height() <= 0.0 {
                return None;
            }
            #[allow(clippy::cast_precision_loss)]
            let count = options.len() as f32;
            let row = ((slide_pos.y - item.rect.top()) / item.rect.height() * count)
                .floor()
                .clamp(0.0, count - 1.0);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let index = row as usize;
            options
                .get(index)
                .map(|(_, value, _)| Value::Text(value.clone()))
        }
        _ => None,
    }
}

fn emit_drag_drop(
    drag: ItemDrag,
    screen_pos: Pos2,
    t: &SlideTransform,
    items: &[DrawItem],
    targets: &[String],
    covered: &dyn Fn(Pos2) -> bool,
    out: &mut Vec<Input>,
) {
    let slide_pos = t.to_slide_pos(screen_pos);
    let current = if slide_pos.x.is_finite() && slide_pos.y.is_finite() {
        slide_pos
    } else {
        drag.current
    };
    let delta = current - drag.press;
    let new_min = drag.origin.min + delta;
    if !new_min.x.is_finite() || !new_min.y.is_finite() {
        return;
    }
    let centre = drag.origin.translate(delta).center();
    let outside = !t.slide_rect().contains(screen_pos) || covered(screen_pos);
    let target = if outside {
        None
    } else {
        top_drop_target(items, targets, centre).map(|item| match &item.hit {
            Hit::Object(id) => id.clone(),
            _ => String::new(),
        })
    };
    let target = target.filter(|id| !id.is_empty());
    out.push(Input::DragDropped {
        item: drag.item,
        target,
        x: new_min.x,
        y: new_min.y,
    });
}

fn top_drop_target<'a>(
    items: &'a [DrawItem],
    targets: &[String],
    centre: Pos2,
) -> Option<&'a DrawItem> {
    if !centre.x.is_finite() || !centre.y.is_finite() {
        return None;
    }
    items.iter().rev().find(|item| {
        matches!(&item.hit, Hit::Object(id) if targets.iter().any(|target| target == id))
            && crate::compose::item_hit_test(item, centre)
    })
}

/// Keep a draggable object's translated rect inside the slide.
fn clamp_draggable(drag: &mut ItemDrag) {
    if !drag.is_draggable_object {
        return;
    }
    let Some(bounds) = drag.bounds else {
        return;
    };
    if !bounds.is_finite() {
        return;
    }
    let delta = drag.current - drag.press;
    if !delta.x.is_finite() || !delta.y.is_finite() {
        return;
    }
    let size = drag.origin.size();
    let max_x = (bounds.right() - size.x).max(bounds.left());
    let max_y = (bounds.bottom() - size.y).max(bounds.top());
    let mut new_min = drag.origin.min + delta;
    new_min.x = new_min.x.clamp(bounds.left(), max_x);
    new_min.y = new_min.y.clamp(bounds.top(), max_y);
    drag.current = drag.press + (new_min - drag.origin.min);
}

/// Draw items with the dragged item lifted to the top and translated by the gesture.
fn lifted_items(items: &[DrawItem], drag: &ItemDrag) -> Vec<DrawItem> {
    let delta = drag.current - drag.press;
    if !delta.x.is_finite() || !delta.y.is_finite() {
        return items.to_vec();
    }
    if drag.is_draggable_object && delta.length() <= 3.0 {
        // Below the drag threshold the object has not moved.
        return items.to_vec();
    }
    let mut out = Vec::with_capacity(items.len());
    let mut lifted = None;
    for item in items {
        let is_target_item = if drag.is_draggable_object {
            matches!(&item.hit, Hit::Object(id) if id == &drag.item)
        } else {
            matches!(&item.hit, Hit::DragItem(id) if id == &drag.item)
        };
        if is_target_item {
            let mut copy = item.clone();
            copy.rect = copy.rect.translate(delta);
            lifted = Some(copy);
        } else {
            out.push(item.clone());
        }
    }
    if let Some(item) = lifted {
        out.push(item);
    }
    out
}

fn open_drop_targets(player: &Player<'_>) -> Vec<String> {
    let Some(slide) = player.current_slide() else {
        return Vec::new();
    };
    let Some(question) = slide.question.as_ref() else {
        return Vec::new();
    };
    if question.kind != QuestionKind::DragDrop {
        return Vec::new();
    }
    let locked = player.state().answers.get(&slide.id).is_some_and(|answer| {
        answer.correct || (question.attempts != 0 && answer.attempts >= question.attempts)
    });
    if locked {
        Vec::new()
    } else {
        question.drop_targets.clone()
    }
}

fn open_draggable_specs(player: &Player<'_>) -> Vec<(String, Vec<String>)> {
    let Some(slide) = player.current_slide() else {
        return Vec::new();
    };
    let mut specs = Vec::new();
    for object in &slide.objects {
        if let Some(drag) = &object.drag {
            specs.push((object.id.clone(), drag.targets.clone()));
        }
    }
    for layer_id in &player.state().visible_layers {
        if let Some(layer) = slide.layers.iter().find(|l| &l.id == layer_id) {
            for object in &layer.objects {
                if let Some(drag) = &object.drag {
                    specs.push((object.id.clone(), drag.targets.clone()));
                }
            }
        }
    }
    specs
}

#[derive(Debug, Clone, PartialEq)]
struct ActiveTransition {
    transition: Transition,
    start_time: f64,
    prev_items: Vec<DrawItem>,
    prev_bg: Option<Color>,
}

/// Slide view widget holding presentational hover and pressed states.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SlideView {
    pointer: PointerState,
    last_transform: Option<SlideTransform>,
    last_items: Vec<DrawItem>,
    current_slide_id: Option<String>,
    active_transition: Option<ActiveTransition>,
    last_bg: Option<Color>,
    /// Base slide items frozen under an open lightbox.
    lightbox_base_items: Vec<DrawItem>,
    lightbox_base_bg: Option<Color>,
    /// Screen rect of the lightbox box, and the transform inside it.
    lightbox_box: Option<Rect>,
    lightbox_transform: Option<SlideTransform>,
    /// When the current lightbox started opening, for the backdrop fade.
    lightbox_opened_at: Option<f64>,
    /// The last object a pointer press sent `click` for, for focus return.
    last_clicked_object: Option<String>,
    /// Object whose hover the keyboard focus currently holds.
    keyboard_hover: Option<String>,
    /// Keyboard drag gesture to paint, if one is held.
    keyboard_drag: Option<crate::focus::KeyboardDragVisual>,
    /// Text-entry buffers, so a field keeps what the learner typed until commit.
    text_buffers: std::collections::BTreeMap<String, String>,
    /// Player preference: no marker pulse or other motion.
    reduced_motion: bool,
}

impl SlideView {
    /// Create a new [`SlideView`] with no initial hovered or pressed items.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Tell the pointer path which object the keyboard focus is hovering.
    ///
    /// Pointer hover stays separate, but neither path may send a duplicate
    /// `hover_start` or `hover_end` for the object the other is holding.
    pub fn set_keyboard_hover(&mut self, object: Option<&str>) {
        self.keyboard_hover = object.map(str::to_owned);
    }

    /// Tell the view about a keyboard drag so it lifts the item and outlines
    /// the current drop target exactly as the pointer path does.
    pub fn set_keyboard_drag(&mut self, drag: Option<crate::focus::KeyboardDragVisual>) {
        self.keyboard_drag = drag;
    }

    /// Respect the player's reduced-motion preference: no marker pulse.
    pub fn set_reduced_motion(&mut self, reduced: bool) {
        self.reduced_motion = reduced;
    }

    /// The object the pointer currently hovers, if any.
    #[must_use]
    pub fn hovered_object(&self) -> Option<&str> {
        match self.pointer.hovered.as_ref() {
            Some(Hit::Object(id)) => Some(id.as_str()),
            _ => None,
        }
    }

    /// The object whose activation most recently sent `Input::Click`, if any.
    /// Used to return focus to the opener when a lightbox closes.
    #[must_use]
    pub fn last_clicked_object(&self) -> Option<&str> {
        self.last_clicked_object.as_deref()
    }

    /// The screen rect of the open lightbox box, from the most recent frame.
    #[must_use]
    pub fn lightbox_box(&self) -> Option<Rect> {
        self.lightbox_box
    }

    /// The slide transform inside the open lightbox box.
    #[must_use]
    pub fn lightbox_transform(&self) -> Option<SlideTransform> {
        self.lightbox_transform
    }

    /// Forget a pointer hover on `object` after keyboard focus releases it, so
    /// the next pointer movement can report the hover again.
    pub fn release_keyboard_hover(&mut self, object: &str) {
        if self.hovered_object() == Some(object) {
            self.pointer.hovered = None;
        }
    }

    /// Return the [`SlideTransform`] computed in the most recent call to [`show`].
    #[must_use]
    pub fn last_transform(&self) -> Option<SlideTransform> {
        self.last_transform
    }

    /// Return the slide items composed in the most recent call to [`show`].
    #[must_use]
    pub fn last_items(&self) -> &[DrawItem] {
        &self.last_items
    }

    /// Whether a slide transition is still running.
    #[must_use]
    pub fn transition_active(&self) -> bool {
        self.active_transition.is_some()
    }

    /// Present the slide view inside `ui`, painting the slide and returning pointer inputs.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        player: &Player<'_>,
        media: Option<(&mut MediaCache, &dyn MediaSource)>,
    ) -> Vec<Input> {
        self.show_covered(ui, player, media, &[])
    }

    /// Present the slide view while treating `covered_rects` as UI drawn above it.
    ///
    /// Clicks and hovers inside a covered rect do not reach slide objects. The
    /// player frame uses this for its floating strip and outline drawer.
    #[allow(clippy::cognitive_complexity)]
    pub fn show_covered(
        &mut self,
        ui: &mut Ui,
        player: &Player<'_>,
        mut media: Option<(&mut MediaCache, &dyn MediaSource)>,
        covered_rects: &[Rect],
    ) -> Vec<Input> {
        let size = ui.available_size();
        if !size.x.is_finite() || !size.y.is_finite() || size.x <= 0.0 || size.y <= 0.0 {
            self.last_items.clear();
            self.current_slide_id = None;
            self.active_transition = None;
            self.last_bg = None;
            return Vec::new();
        }

        let (rect, _response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(
            rect,
            CornerRadius::ZERO,
            Color32::from_rgb(0x20, 0x21, 0x24),
        );

        let current_slide = player.current_slide();
        let current_id = current_slide.map(|s| s.id.clone());
        let current_bg = current_slide.map(|s| player.course().slide_background(s).clone());

        let slide_size = match current_slide {
            Some(_) => player.course().slide_size,
            None => {
                self.last_items.clear();
                self.current_slide_id = None;
                self.active_transition = None;
                self.last_bg = None;
                return Vec::new();
            }
        };

        let now = ui.input(|i| i.time);
        if let Some((cache, _)) = media.as_mut() {
            let elapsed_ms = (now.max(0.0) * 1000.0).min(u64::MAX as f64) as u64;
            cache.set_animation_time(Some(elapsed_ms));
        }

        // Opening the lightbox freezes the last painted base frame; closing
        // drops it. Replacing the box slide keeps the same base frame.
        let lightbox_open = player.lightbox_open();
        if lightbox_open {
            if self.lightbox_opened_at.is_none() {
                self.lightbox_base_items = self.last_items.clone();
                self.lightbox_base_bg = self.last_bg.clone();
                self.lightbox_opened_at = Some(now);
            }
        } else if self.lightbox_opened_at.take().is_some() {
            self.lightbox_base_items.clear();
            self.lightbox_base_bg = None;
            self.lightbox_box = None;
            self.lightbox_transform = None;
        }

        if let Some(ref prev_id) = self.current_slide_id
            && Some(prev_id) != current_id.as_ref()
            && !lightbox_open
            && let Some(slide) = current_slide
            && let Some(ref trans) = slide.transition
        {
            self.active_transition = Some(ActiveTransition {
                transition: trans.clone(),
                start_time: now,
                prev_items: self.last_items.clone(),
                prev_bg: self.last_bg.clone(),
            });
        }
        self.current_slide_id = current_id;
        self.last_bg = current_bg;

        let Some(t) = SlideTransform::fit(rect, slide_size) else {
            self.last_items.clear();
            return Vec::new();
        };
        self.last_transform = Some(t);

        // A lightbox maps slide coordinates into the box instead of the whole
        // slide rect. The base underneath takes no hits.
        let box_transform = if lightbox_open {
            lightbox_box_rect(t.slide_rect(), slide_size)
                .and_then(|box_rect| SlideTransform::fit(box_rect, slide_size))
        } else {
            None
        };
        self.lightbox_box = box_transform.as_ref().map(SlideTransform::slide_rect);
        self.lightbox_transform = box_transform;
        let active_t = box_transform.unwrap_or(t);

        let mut items = compose(player);
        if self.reduced_motion {
            for item in &mut items {
                if let Visual::Marker { pulse, .. } = &mut item.visual {
                    *pulse = false;
                }
            }
        }
        crate::text_layout::fill_item_links(ui.ctx(), &mut items);

        self.pointer.prune_uncomposed(&items);

        let events = ui.input(|i| i.events.clone());
        let ctx = ui.ctx().clone();
        let layer = ui.layer_id();
        let covered = |pos: Pos2| {
            covered_rects.iter().any(|rect| rect.contains(pos))
                || ctx.layer_id_at(pos) != Some(layer)
        };
        self.pointer.drop_targets = open_drop_targets(player);
        self.pointer.draggable_specs = open_draggable_specs(player);
        self.pointer.slide_bounds = Some(egui::Rect::from_min_size(
            Pos2::ZERO,
            egui::vec2(
                player.course().slide_size.w as f32,
                player.course().slide_size.h as f32,
            ),
        ));
        self.pointer.markers_open = player.marker_open().is_some();
        self.pointer.keyboard_hover = self.keyboard_hover.clone();
        let mut inputs = self
            .pointer
            .handle_events(&events, &active_t, &items, &covered);
        for input in &inputs {
            if let Input::Click { object } = input {
                self.last_clicked_object = Some(object.clone());
            }
        }
        let keyboard_drag = self.keyboard_drag.as_ref().and_then(|drag| {
            let item = items.iter().rev().find(
                |item| matches!(&item.hit, Hit::DragItem(id) | Hit::Object(id) if id == &drag.item),
            )?;
            let target = drag.target.as_ref().and_then(|target_id| {
                items
                    .iter()
                    .rev()
                    .find(|item| matches!(&item.hit, Hit::Object(id) if id == target_id))
            });
            let is_draggable_object = player
                .current_slide()
                .and_then(|s| s.object(&drag.item))
                .is_some_and(|(obj, _)| obj.drag.is_some());
            let lift = ItemDrag {
                item: drag.item.clone(),
                origin: item.rect,
                press: item.rect.center(),
                current: target.map_or(item.rect.center(), |target| target.rect.center()),
                is_draggable_object,
                targets: Vec::new(),
                bounds: self.pointer.slide_bounds,
            };
            Some((lift, target.map(crate::compose::item_bounds)))
        });
        let lifted = if let Some(drag) = self.pointer.drag.as_ref() {
            Some(lifted_items(&items, drag))
        } else {
            keyboard_drag
                .as_ref()
                .map(|(lift, _)| lifted_items(&items, lift))
        };
        let display: &[DrawItem] = match lifted.as_deref() {
            Some(list) => list,
            None => &items,
        };
        let drop_outline = if let Some(drag) = self.pointer.drag.as_ref() {
            let delta = drag.current - drag.press;
            if !delta.x.is_finite() || !delta.y.is_finite() {
                None
            } else {
                let centre = drag.origin.translate(delta).center();
                let targets = if drag.is_draggable_object {
                    &drag.targets
                } else {
                    &self.pointer.drop_targets
                };
                top_drop_target(&items, targets, centre).map(crate::compose::item_bounds)
            }
        } else {
            keyboard_drag.as_ref().and_then(|(_, target)| *target)
        };

        let theme = Some(&player.course().theme);
        let slide_bg = player
            .current_slide()
            .map(|s| player.course().slide_background(s));

        let mut transition_info = None;
        if !lightbox_open && let Some(ref trans) = self.active_transition {
            let dur_sec = trans.transition.duration_ms as f64 / 1000.0;
            let elapsed = now - trans.start_time;
            if dur_sec <= 0.0 || elapsed >= dur_sec {
                self.active_transition = None;
            } else {
                let p = (elapsed / dur_sec).clamp(0.0, 1.0) as f32;
                let sample = coursecraft_logic::animation::sample_transition(
                    &trans.transition,
                    p,
                    slide_size,
                );
                transition_info = Some((
                    trans.transition.effect.clone(),
                    sample,
                    trans.prev_items.clone(),
                    trans.prev_bg.clone(),
                ));
                ui.ctx().request_repaint();
            }
        }

        if let Some((effect, sample, prev_items, prev_bg)) = transition_info {
            match effect {
                TransitionEffect::Fade => {
                    paint::paint_background(&painter, &t, theme, slide_bg, (0.0, 0.0));
                    let mut old_items = prev_items;
                    for item in &mut old_items {
                        item.opacity *= sample.prev_alpha;
                    }
                    paint::paint_items(
                        &painter,
                        &t,
                        &old_items,
                        Highlight::default(),
                        media.as_mut().map(|(c, s)| (&mut **c, *s)),
                        theme,
                        (0.0, 0.0),
                    );
                    let mut next_items = display.to_vec();
                    for item in &mut next_items {
                        item.opacity *= sample.next_alpha;
                    }
                    paint::paint_items(
                        &painter,
                        &t,
                        &next_items,
                        self.pointer.highlight(),
                        media,
                        theme,
                        (0.0, 0.0),
                    );
                }
                TransitionEffect::Push { .. } => {
                    paint::paint_background(
                        &painter,
                        &t,
                        theme,
                        prev_bg.as_ref(),
                        sample.prev_offset,
                    );
                    paint::paint_items(
                        &painter,
                        &t,
                        &prev_items,
                        Highlight::default(),
                        media.as_mut().map(|(c, s)| (&mut **c, *s)),
                        theme,
                        sample.prev_offset,
                    );
                    paint::paint_background(&painter, &t, theme, slide_bg, sample.next_offset);
                    paint::paint_items(
                        &painter,
                        &t,
                        display,
                        self.pointer.highlight(),
                        media,
                        theme,
                        sample.next_offset,
                    );
                }
                TransitionEffect::Cover { .. } => {
                    paint::paint_background(&painter, &t, theme, prev_bg.as_ref(), (0.0, 0.0));
                    paint::paint_items(
                        &painter,
                        &t,
                        &prev_items,
                        Highlight::default(),
                        media.as_mut().map(|(c, s)| (&mut **c, *s)),
                        theme,
                        (0.0, 0.0),
                    );
                    paint::paint_background(&painter, &t, theme, slide_bg, sample.next_offset);
                    paint::paint_items(
                        &painter,
                        &t,
                        display,
                        self.pointer.highlight(),
                        media,
                        theme,
                        sample.next_offset,
                    );
                }
            }
        } else if let Some(box_rect) = box_transform.as_ref().map(SlideTransform::slide_rect) {
            // Base slide, frozen, then the dim backdrop and the boxed slide.
            paint::paint_background(
                &painter,
                &t,
                theme,
                self.lightbox_base_bg.as_ref(),
                (0.0, 0.0),
            );
            let mut base_items = self.lightbox_base_items.clone();
            for item in &mut base_items {
                item.hit = Hit::None;
            }
            paint::paint_items(
                &painter,
                &t,
                &base_items,
                Highlight::default(),
                media.as_mut().map(|(c, s)| (&mut **c, *s)),
                theme,
                (0.0, 0.0),
            );

            let progress = if self.reduced_motion {
                1.0
            } else {
                self.lightbox_opened_at.map_or(1.0, |start| {
                    ((now - start) / LIGHTBOX_FADE_SECONDS).clamp(0.0, 1.0) as f32
                })
            };
            if progress < 1.0 {
                ui.ctx().request_repaint();
            }
            let alpha = (LIGHTBOX_BACKDROP * 255.0 * progress)
                .round()
                .clamp(0.0, 255.0) as u8;
            painter.rect_filled(
                t.slide_rect(),
                CornerRadius::ZERO,
                Color32::from_black_alpha(alpha),
            );
            let border = crate::design::palette(ui.ctx()).border;
            painter.rect_stroke(
                box_rect,
                CornerRadius::same(8),
                Stroke::new(1.0, border),
                StrokeKind::Inside,
            );

            let painted = without_widget_text(display);
            let box_painter = painter.with_clip_rect(box_rect);
            paint::paint_background(&box_painter, &active_t, theme, slide_bg, (0.0, 0.0));
            paint::paint_items(
                &box_painter,
                &active_t,
                &painted,
                self.pointer.highlight(),
                media,
                theme,
                (0.0, 0.0),
            );
        } else {
            let painted = without_widget_text(display);
            paint::paint_background(&painter, &t, theme, slide_bg, (0.0, 0.0));
            paint::paint_items(
                &painter,
                &t,
                &painted,
                self.pointer.highlight(),
                media,
                theme,
                (0.0, 0.0),
            );
        }

        if let Some(rect) = drop_outline {
            let accent = theme
                .and_then(|th| crate::color::parse_color(&th.accent))
                .unwrap_or(Color32::from_rgb(0x2F, 0x6F, 0xEB));
            let width = (2.0 * active_t.scale()).max(1.0);
            painter.with_clip_rect(active_t.slide_rect()).rect_stroke(
                active_t.to_screen_rect(rect),
                CornerRadius::ZERO,
                Stroke::new(width, accent),
                StrokeKind::Outside,
            );
        }

        if self.active_transition.is_none() || lightbox_open {
            inputs.extend(show_question_widgets(ui, player, &active_t, &items, theme));
            inputs.extend(self.show_text_entry_widgets(ui, &active_t, &items, theme));
        }

        self.last_items = items;

        inputs
    }
}

impl SlideView {
    /// Draw a real text field over every enabled text-entry object. Text is
    /// committed to the runtime on Enter (single-line) or when focus leaves.
    fn show_text_entry_widgets(
        &mut self,
        ui: &mut Ui,
        t: &SlideTransform,
        items: &[DrawItem],
        theme: Option<&coursecraft_course::Theme>,
    ) -> Vec<Input> {
        let mut inputs = Vec::new();
        let text = theme
            .and_then(|th| crate::color::parse_color(&th.text))
            .unwrap_or_else(|| Color32::from_rgb(0x1F, 0x1F, 0x1F));
        let accent = theme
            .and_then(|th| crate::color::parse_color(&th.accent))
            .unwrap_or_else(|| Color32::from_rgb(0x2F, 0x6F, 0xEB));
        let mut host = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(t.slide_rect())
                .id_salt("input-text-widgets"),
        );
        host.set_clip_rect(t.slide_rect());
        let scale = t.scale();
        let font = crate::font::slide_font(ui.ctx(), false, false, (22.0 * scale).max(1.0));
        for item in items {
            let Visual::TextEntry {
                text: current,
                placeholder,
                multiline,
            } = &item.visual
            else {
                continue;
            };
            if item.dimmed {
                continue;
            }
            let Hit::Object(object) = &item.hit else {
                continue;
            };
            let screen = t.to_screen_rect(item.rect);
            if !screen.is_positive() {
                continue;
            }
            let id = crate::focus::input_text_id(object);
            let has_focus = host.memory(|m| m.has_focus(id));
            let mut buffer = if has_focus {
                self.text_buffers
                    .get(object)
                    .cloned()
                    .unwrap_or_else(|| current.clone())
            } else {
                current.clone()
            };
            apply_slide_widget_style(
                host.style_mut(),
                &font,
                scale,
                text,
                accent,
                screen.height(),
            );
            let pad_x = (8.0 * scale).clamp(0.0, 127.0).round() as i8;
            let pad_y = if *multiline {
                (4.0 * scale).clamp(0.0, 127.0) as i8
            } else {
                ((screen.height() - font.size * 1.3) * 0.5).clamp(0.0, 127.0) as i8
            };
            let hint = egui::RichText::new(placeholder.clone())
                .font(font.clone())
                .color(Color32::from_rgb(0x8A, 0x8A, 0x8A));
            let edit = if *multiline {
                egui::TextEdit::multiline(&mut buffer)
            } else {
                egui::TextEdit::singleline(&mut buffer)
            };
            let response = host.put(
                screen,
                edit.id(id)
                    .frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(pad_x, pad_y)))
                    .hint_text(hint)
                    .text_color(text)
                    .desired_width(screen.width().max(0.0))
                    .font(font.clone()),
            );
            if has_focus || response.has_focus() {
                self.text_buffers.insert(object.clone(), buffer.clone());
            }
            let enter = !*multiline
                && response.has_focus()
                && host.input(|i| i.key_pressed(egui::Key::Enter));
            if (enter || response.lost_focus()) && buffer != *current {
                inputs.push(Input::SetInputValue {
                    object: object.clone(),
                    value: Value::Text(buffer),
                });
                self.text_buffers.remove(object);
            }
        }
        inputs
    }
}

/// Items for painting while live widgets are on top: the answer boxes keep their
/// frames, but the fill-in text and the matching value are drawn by the widgets
/// themselves, so they are blanked here to avoid drawing them twice.
pub(crate) fn without_widget_text(items: &[DrawItem]) -> std::borrow::Cow<'_, [DrawItem]> {
    let needs = items.iter().any(|i| {
        matches!(
            i.visual,
            Visual::FillIn { .. } | Visual::MatchRow { .. } | Visual::TextEntry { .. }
        )
    });
    if !needs {
        return std::borrow::Cow::Borrowed(items);
    }
    let mut out = items.to_vec();
    for item in &mut out {
        if item.dimmed {
            continue;
        }
        match &mut item.visual {
            Visual::FillIn {
                text, placeholder, ..
            } => {
                text.clear();
                placeholder.clear();
            }
            Visual::MatchRow { value, .. } => value.clear(),
            Visual::TextEntry {
                text, placeholder, ..
            } => {
                text.clear();
                placeholder.clear();
            }
            _ => {}
        }
    }
    std::borrow::Cow::Owned(out)
}

/// Light slide styling for the question widgets, scoped to their own `Ui`/popups.
fn apply_slide_widget_style(
    style: &mut egui::Style,
    font: &egui::FontId,
    scale: f32,
    text: Color32,
    accent: Color32,
    row_h: f32,
) {
    let light = Color32::from_rgb(0xF5, 0xF6, 0xF8);
    let border = Color32::from_rgb(0xD0, 0xD4, 0xDC);
    let [r, g, b, _] = accent.to_srgba_unmultiplied();
    let hover = Color32::from_rgba_unmultiplied(r, g, b, 40);
    let selected = Color32::from_rgba_unmultiplied(r, g, b, 70);
    style.visuals = egui::Visuals::light();
    let v = &mut style.visuals;
    v.override_text_color = Some(text);
    v.window_fill = Color32::WHITE;
    v.panel_fill = Color32::WHITE;
    v.extreme_bg_color = light;
    v.window_stroke = egui::Stroke::new(1.0, border);
    v.selection.bg_fill = selected;
    v.selection.stroke = egui::Stroke::new(1.0, text);
    v.popup_shadow = egui::Shadow::NONE;
    for w in [
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.bg_fill = Color32::TRANSPARENT;
        w.weak_bg_fill = Color32::TRANSPARENT;
        w.bg_stroke = egui::Stroke::NONE;
        w.fg_stroke = egui::Stroke::new(1.0, text);
        w.expansion = 0.0;
    }
    v.widgets.hovered.weak_bg_fill = hover;
    v.widgets.hovered.bg_fill = hover;
    v.widgets.active.weak_bg_fill = hover;
    v.widgets.active.bg_fill = hover;
    style.override_font_id = Some(font.clone());
    style.spacing.button_padding = egui::vec2(8.0 * scale, 0.0);
    style.spacing.interact_size.y = row_h.max(1.0);
    style.spacing.item_spacing = egui::vec2(0.0, 2.0 * scale);
}

fn show_question_widgets(
    ui: &mut Ui,
    player: &Player<'_>,
    t: &SlideTransform,
    items: &[DrawItem],
    theme: Option<&coursecraft_course::Theme>,
) -> Vec<Input> {
    let mut inputs = Vec::new();
    let Some(question) = player.current_slide().and_then(|s| s.question.as_ref()) else {
        return inputs;
    };
    let text = theme
        .and_then(|th| crate::color::parse_color(&th.text))
        .unwrap_or_else(|| Color32::from_rgb(0x1F, 0x1F, 0x1F));
    let accent = theme
        .and_then(|th| crate::color::parse_color(&th.accent))
        .unwrap_or_else(|| Color32::from_rgb(0x2F, 0x6F, 0xEB));
    let style = WidgetStyle {
        font: crate::font::slide_font(ui.ctx(), false, false, (22.0 * t.scale()).max(1.0)),
        text,
        accent,
    };
    let mut host = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(t.slide_rect())
            .id_salt("question-widgets"),
    );
    host.set_clip_rect(t.slide_rect());
    match question.kind {
        QuestionKind::FillIn | QuestionKind::ShortAnswer | QuestionKind::Essay => {
            show_fill_in(&mut host, player, t, items, &style, &mut inputs)
        }
        QuestionKind::Matching => {
            show_matching(&mut host, player, t, items, question, &style, &mut inputs);
        }
        QuestionKind::MultipleChoice
        | QuestionKind::MultipleResponse
        | QuestionKind::TrueFalse
        | QuestionKind::Sequence
        | QuestionKind::Hotspot
        | QuestionKind::DragDrop
        | QuestionKind::Likert
        | QuestionKind::Freeform => {}
    }
    inputs
}

struct WidgetStyle {
    font: egui::FontId,
    text: Color32,
    accent: Color32,
}

fn show_fill_in(
    ui: &mut Ui,
    player: &Player<'_>,
    t: &SlideTransform,
    items: &[DrawItem],
    style: &WidgetStyle,
    inputs: &mut Vec<Input>,
) {
    let Some(item) = items
        .iter()
        .find(|i| matches!(i.visual, Visual::FillIn { .. }))
    else {
        return;
    };
    let multiline = matches!(
        &item.visual,
        Visual::FillIn {
            multiline: true,
            ..
        }
    );
    let screen = t.to_screen_rect(item.rect);
    if !screen.is_positive() {
        return;
    }
    let scale = t.scale();
    let mut text = player.text_entry().to_owned();
    let font_size = style.font.size;
    apply_slide_widget_style(
        ui.style_mut(),
        &style.font,
        scale,
        style.text,
        style.accent,
        screen.height(),
    );
    let pad_x = (8.0 * scale).clamp(0.0, 127.0).round() as i8;
    let pad_y = if multiline {
        (8.0 * scale).clamp(0.0, 127.0) as i8
    } else {
        ((screen.height() - font_size * 1.3) * 0.5).clamp(0.0, 127.0) as i8
    };
    let hint = egui::RichText::new(player.course().chrome_string("fillInPlaceholder"))
        .font(style.font.clone())
        .color(Color32::from_rgb(0x8A, 0x8A, 0x8A));
    let edit = if multiline {
        let line = font_size * 1.3;
        let rows = if line.is_finite() && line > 0.0 {
            ((screen.height() - 16.0 * scale) / line).max(1.0) as usize
        } else {
            1
        };
        egui::TextEdit::multiline(&mut text).desired_rows(rows.max(1))
    } else {
        egui::TextEdit::singleline(&mut text)
    };
    let response = ui.put(
        screen,
        edit.id(crate::focus::fill_in_id())
            .frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(pad_x, pad_y)))
            .hint_text(hint)
            .text_color(style.text)
            .desired_width(screen.width().max(0.0))
            .font(style.font.clone()),
    );
    if response.has_focus() {
        let radius = (6.0 * scale).clamp(0.0, 255.0).round() as u8;
        ui.painter().rect_stroke(
            screen,
            CornerRadius::same(radius),
            egui::Stroke::new((2.0 * scale).max(1.0), style.accent),
            egui::StrokeKind::Inside,
        );
    }
    if response.changed() && text != player.text_entry() {
        inputs.push(Input::TextEntered { text });
    }
}

fn show_matching(
    ui: &mut Ui,
    player: &Player<'_>,
    t: &SlideTransform,
    items: &[DrawItem],
    question: &Question,
    style: &WidgetStyle,
    inputs: &mut Vec<Input>,
) {
    let options = player.matching_options();
    for item in items {
        let Visual::MatchRow { id, value, .. } = &item.visual else {
            continue;
        };
        let row = t.to_screen_rect(item.rect);
        let scale = t.scale();
        let combo_w = (row.width() * 0.46).max(1.0);
        let combo_h = (row.height() - 12.0 * scale).max(1.0);
        let combo_rect = egui::Rect::from_min_size(
            egui::pos2(row.max.x - combo_w - 8.0 * scale, row.min.y + 6.0 * scale),
            egui::vec2(combo_w, combo_h),
        );
        if !combo_rect.is_positive() {
            continue;
        }
        let current = player.match_for(id).map(str::to_owned);
        let mut pick: Option<Option<String>> = None;
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(combo_rect)
                .id_salt(("match-slot", id)),
        );
        apply_slide_widget_style(
            child.style_mut(),
            &style.font,
            scale,
            style.text,
            style.accent,
            combo_h,
        );
        let (popup_font, text_c, accent_c) = (style.font.clone(), style.text, style.accent);
        let combo = egui::ComboBox::from_id_salt(("match-pair", id))
            .selected_text(value.clone())
            .truncate()
            .width(combo_w)
            .popup_style(egui::style::StyleModifier::new(move |st| {
                apply_slide_widget_style(st, &popup_font, 1.0, text_c, accent_c, 22.0);
                st.spacing.button_padding = egui::vec2(8.0, 3.0);
                st.spacing.item_spacing = egui::vec2(0.0, 2.0);
            }));
        let combo_response = combo
            .show_ui(&mut child, |ui| {
                let is_clear = current.is_none();
                if ui
                    .selectable_label(is_clear, player.course().chrome_string("choosePlaceholder"))
                    .clicked()
                    && !is_clear
                {
                    pick = Some(None);
                }
                for opt in &options {
                    let label = question
                        .pairs
                        .iter()
                        .find(|p| p.id == *opt)
                        .map(|p| player.display_text(&p.right))
                        .unwrap_or_default();
                    let selected = current.as_deref() == Some(opt.as_str());
                    if ui.selectable_label(selected, label).clicked() && !selected {
                        pick = Some(Some(opt.clone()));
                    }
                }
            })
            .response;
        if !value.is_empty() {
            combo_response.on_hover_text(value.as_str());
        }
        if let Some(right) = pick {
            inputs.push(Input::MatchSelected {
                left: id.clone(),
                right,
            });
        }
    }
}
