//! M46 phase 5: the Entrance and Exit cards of the Animate panel
//! (docs/plans/m46-animation-system.md › Animations dock panel).
//!
//! Each card has an effect picker (drawn icon and name; it opens a gallery with search, Recent
//! and category headers), one row of icon buttons per option the effect takes, Duration,
//! Easing (drawn as curves), ▶ Preview and Remove. The card edits the panel's buffers; the
//! caller turns a change into `object.set_animation` for every selected object.

use coursecraft_course::{AnimEffect, EFFECTS, Easing, EffectCategory};
use egui::{Align2, CornerRadius, FontId, Pos2, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};

use crate::anim_preview::PreviewState;
use crate::ribbon::effect_art::{OptionValue, paint_effect, paint_glyph};
use crate::tl;

/// What the card asks the canvas preview to do.
#[derive(Debug, Clone, PartialEq)]
pub enum PanelPreview {
    /// Play the card's animation once (▶ Preview).
    Play,
    /// Loop this effect while its tile is hovered.
    Hover(AnimEffect),
}

/// The card's buffers.
pub struct CardState<'a> {
    pub effect: &'a mut String,
    pub fx: &'a mut AnimEffect,
    pub duration_ms: &'a mut u32,
    pub easing: &'a mut Easing,
    /// The text sequence, when the object has text to play in parts.
    pub sequence: Option<&'a mut coursecraft_course::Sequence>,
}

const CATEGORIES: [(EffectCategory, &str); 4] = [
    (EffectCategory::Basic, "Basic"),
    (EffectCategory::Subtle, "Subtle"),
    (EffectCategory::Moderate, "Moderate"),
    (EffectCategory::Exciting, "Exciting"),
];

/// The label of effect id `id` (or `"none"`) as an entrance or exit. English.
#[must_use]
pub fn effect_label(exit: bool, id: &str) -> &'static str {
    if id == "none" {
        return "None";
    }
    coursecraft_course::effect_def(id).map_or("Unknown effect", |d| d.label(exit))
}

/// Draw one Entrance or Exit card. Returns whether the animation changed.
pub fn show_card(
    ui: &mut Ui,
    which: &'static str,
    state: CardState<'_>,
    pv: &mut PreviewState,
    preview: &mut Option<PanelPreview>,
) -> bool {
    let exit = which == "exit";
    let p = coursecraft_render::palette(ui.ctx());
    let mut changed = false;
    let card_w = ui.available_width();
    egui::Frame::NONE
        .fill(p.raised)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(coursecraft_render::RADIUS_MD as u8))
        .inner_margin(egui::Margin::same(coursecraft_render::SPACE_MD as i8))
        .show(ui, |ui| {
            ui.set_width((card_w - 2.0 * (coursecraft_render::SPACE_MD + 1.0)).max(60.0));
            if let Some(id) = effect_picker(ui, which, state.effect, state.fx, pv, preview) {
                if id != "none" {
                    pv.picked(&id);
                }
                // Picking the current effect again changes nothing.
                if id != *state.effect {
                    *state.effect = id;
                    changed = true;
                }
            }
            if state.effect.as_str() == "none" {
                return;
            }
            if state.fx.id != *state.effect {
                *state.fx = state.fx.switched_to(state.effect);
            }
            changed |= option_rows(ui, exit, state.fx);
            changed |= duration_row(ui, state.duration_ms);
            changed |= easing_row(ui, state.easing);
            if let Some(sequence) = state.sequence {
                changed |= sequence_row(ui, which, sequence);
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let play = egui::Button::new(
                    coursecraft_render::icon_text(coursecraft_render::Icon::Play, tl!("Preview"))
                        .color(p.text),
                );
                if ui
                    .add(play)
                    .on_hover_text(tl!("Play this animation on the canvas"))
                    .clicked()
                {
                    *preview = Some(PanelPreview::Play);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let tip = if exit {
                        tl!("Remove the exit animation")
                    } else {
                        tl!("Remove the entrance animation")
                    };
                    if crate::widgets::icon_button_danger(
                        ui,
                        coursecraft_render::Icon::Trash,
                        true,
                        tip,
                    )
                    .clicked()
                    {
                        *state.effect = "none".to_owned();
                        changed = true;
                    }
                });
            });
        });
    changed
}

/// The effect picker: a wide button with the effect's icon and name; it opens the gallery.
/// Returns the picked id (`"none"` to remove).
fn effect_picker(
    ui: &mut Ui,
    which: &'static str,
    effect: &str,
    fx: &AnimEffect,
    pv: &mut PreviewState,
    preview: &mut Option<PanelPreview>,
) -> Option<String> {
    let exit = which == "exit";
    let p = coursecraft_render::palette(ui.ctx());
    let label = tl!(effect_label(exit, effect));
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, label));
    let radius = CornerRadius::same(coursecraft_render::RADIUS_SM as u8);
    ui.painter()
        .rect_filled(rect, radius, if resp.hovered() { p.hover } else { p.input });
    ui.painter().rect_stroke(
        rect,
        radius,
        Stroke::new(1.0, p.border),
        egui::StrokeKind::Inside,
    );
    let art = Rect::from_min_size(rect.min + vec2(6.0, 4.5), vec2(36.0, 27.0));
    if effect == "none" {
        ui.painter().text(
            art.center(),
            Align2::CENTER_CENTER,
            coursecraft_render::Icon::XCircle.codepoint(),
            FontId::new(18.0, egui::FontFamily::Name("phosphor".into())),
            p.text_secondary,
        );
    } else {
        let mut shown = fx.clone();
        if shown.id != effect {
            shown = shown.switched_to(effect);
        }
        let ink = if exit { p.danger } else { p.success };
        paint_effect(ui.painter(), art, &shown, exit, ink, p.border_strong, None);
    }
    ui.painter().text(
        pos2(art.max.x + 8.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(coursecraft_render::SIZE_BODY),
        p.text,
    );
    crate::ribbon::widgets::chevron(
        ui.painter(),
        pos2(rect.max.x - 12.0, rect.center().y),
        4.0,
        p.text_secondary,
    );
    let mut picked = None;
    egui::Popup::menu(&resp)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            picked = effect_gallery(ui, exit, effect, fx, pv, preview);
            if picked.is_some() {
                ui.close();
            }
        });
    picked
}

/// One gallery tile.
fn tile(
    ui: &mut Ui,
    exit: bool,
    id: &'static str,
    fx: &AnimEffect,
    current: bool,
) -> egui::Response {
    let effect = if id == "none" {
        None
    } else if fx.id == id {
        Some(fx.clone())
    } else {
        Some(AnimEffect::new(id))
    };
    crate::ribbon::gallery::effect_tile(ui, effect_label(exit, id), effect, exit, current)
}

/// The gallery: search, None, Recent, then every effect under its category. Returns the
/// picked id; a hovered tile asks for a looping preview.
fn effect_gallery(
    ui: &mut Ui,
    exit: bool,
    current: &str,
    fx: &AnimEffect,
    pv: &mut PreviewState,
    preview: &mut Option<PanelPreview>,
) -> Option<String> {
    let p = coursecraft_render::palette(ui.ctx());
    let mut picked = None;
    let mut hovered: Option<&'static str> = None;
    ui.set_width(6.0 * 60.0 + 12.0);
    ui.add(
        egui::TextEdit::singleline(&mut pv.search)
            .hint_text(tl!("Search effects"))
            .desired_width(f32::INFINITY),
    );
    let query = pv.search.trim().to_lowercase();
    let heading = |ui: &mut Ui, text: &str| {
        ui.add_space(4.0);
        ui.label(RichText::new(text).size(11.0).color(p.text_secondary));
    };
    let mut row = |ui: &mut Ui, ids: &[&'static str]| {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(2.0, 2.0);
            for &id in ids {
                let r = tile(ui, exit, id, fx, id == current);
                if r.clicked() {
                    picked = Some(id.to_owned());
                }
                if r.hovered() && id != "none" {
                    hovered = Some(id);
                }
            }
        });
    };
    egui::ScrollArea::vertical()
        .max_height(440.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            if query.is_empty() {
                // None, then the effects picked recently.
                let first: Vec<&'static str> = std::iter::once("none")
                    .chain(
                        pv.recent
                            .iter()
                            .filter_map(|r| coursecraft_course::effect_def(r).map(|d| d.id)),
                    )
                    .collect();
                if first.len() > 1 {
                    heading(ui, tl!("Recent"));
                }
                row(ui, &first);
                for (category, name) in CATEGORIES {
                    let ids: Vec<&'static str> = EFFECTS
                        .iter()
                        .filter(|d| d.category == category)
                        .map(|d| d.id)
                        .collect();
                    heading(ui, tl!(name));
                    row(ui, &ids);
                }
            } else {
                let ids: Vec<&'static str> = EFFECTS
                    .iter()
                    .filter(|d| tl!(d.label(exit)).to_lowercase().contains(&query))
                    .map(|d| d.id)
                    .collect();
                if ids.is_empty() {
                    ui.label(RichText::new(tl!("No effect matches")).color(p.text_secondary));
                }
                row(ui, &ids);
            }
        });
    if let Some(id) = hovered {
        let effect = if fx.id == id {
            fx.clone()
        } else {
            fx.switched_to(id)
        };
        *preview = Some(PanelPreview::Hover(effect));
    }
    if picked.is_some() {
        pv.search.clear();
    }
    picked
}

/// One row of icon buttons per option of `fx`'s effect. Returns whether one changed.
pub(crate) fn option_rows(ui: &mut Ui, exit: bool, fx: &mut AnimEffect) -> bool {
    let Some(def) = fx.def() else {
        return false;
    };
    let p = coursecraft_render::palette(ui.ctx());
    let mut pick = None;
    for option in def.options {
        crate::properties::inspector_row(ui, tl!(option.label()), |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(1.0, 1.0);
                for v in OptionValue::all(option) {
                    let on = v.is_set(fx);
                    let name = match v {
                        OptionValue::Spokes(n) if n != 1 => {
                            craft_i18n::fmt(tl!(v.label()), &[("n", n.to_string().as_str())])
                        }
                        _ => tl!(v.label()).to_owned(),
                    };
                    let (rect, resp) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
                    resp.widget_info(|| {
                        egui::WidgetInfo::selected(egui::Role::Button, true, on, &name)
                    });
                    let radius = CornerRadius::same(coursecraft_render::RADIUS_SM as u8);
                    if on {
                        ui.painter().rect_filled(rect, radius, p.accent_subtle);
                        ui.painter().rect_stroke(
                            rect,
                            radius,
                            Stroke::new(1.0, p.accent),
                            egui::StrokeKind::Inside,
                        );
                    } else if resp.hovered() {
                        ui.painter().rect_filled(rect, radius, p.hover);
                    }
                    let ink = if on { p.accent_text } else { p.text };
                    paint_glyph(ui.painter(), rect.center(), 14.0, ink, v.glyph(exit));
                    if resp.on_hover_text(&name).clicked() && !on {
                        pick = Some(v);
                    }
                }
            });
        });
    }
    match pick {
        Some(v) => {
            v.set(fx);
            true
        }
        None => false,
    }
}

/// Duration, in seconds. Returns whether an edit was committed.
fn duration_row(ui: &mut Ui, duration_ms: &mut u32) -> bool {
    let mut changed = false;
    crate::properties::inspector_row(ui, tl!("Duration"), |ui| {
        let mut seconds = f64::from(*duration_ms) / 1000.0;
        let r = ui.add(
            egui::DragValue::new(&mut seconds)
                .speed(0.05)
                .range(0.0..=60.0)
                .fixed_decimals(2)
                .suffix(" s"),
        );
        let ms = (seconds * 1000.0).round().clamp(0.0, 60_000.0) as u32;
        if ms != *duration_ms {
            *duration_ms = ms.max(1);
        }
        let enter = r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if r.drag_stopped() || r.lost_focus() || enter {
            changed = true;
        }
    });
    changed
}

/// A small drawing of `easing`'s curve in `rect`.
fn paint_curve(painter: &egui::Painter, rect: Rect, easing: Easing, color: egui::Color32) {
    let points: Vec<Pos2> = (0..=16)
        .map(|i| {
            let t = i as f32 / 16.0;
            let y = coursecraft_logic::ease(easing, t);
            pos2(
                rect.min.x + t * rect.width(),
                rect.max.y - y.clamp(-0.2, 1.2) * rect.height(),
            )
        })
        .collect();
    painter.add(egui::Shape::line(points, Stroke::new(1.5, color)));
}

/// Easing, as a picker of drawn curves. Returns whether it changed.
fn easing_row(ui: &mut Ui, easing: &mut Easing) -> bool {
    let p = coursecraft_render::palette(ui.ctx());
    let mut changed = false;
    crate::properties::inspector_row(ui, tl!("Easing"), |ui| {
        let w = ui.available_width().max(30.0);
        let (rect, resp) = ui.allocate_exact_size(vec2(w, 24.0), Sense::click());
        let label = tl!(easing.label());
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, label));
        let radius = CornerRadius::same(coursecraft_render::RADIUS_SM as u8);
        ui.painter()
            .rect_filled(rect, radius, if resp.hovered() { p.hover } else { p.input });
        let curve = Rect::from_min_size(rect.min + vec2(6.0, 5.0), vec2(22.0, 14.0));
        paint_curve(ui.painter(), curve, *easing, p.accent);
        ui.painter().text(
            pos2(curve.max.x + 6.0, rect.center().y),
            Align2::LEFT_CENTER,
            label,
            FontId::proportional(coursecraft_render::SIZE_BODY),
            p.text,
        );
        egui::Popup::menu(&resp).show(|ui| {
            for value in Easing::ALL {
                let name = value.label();
                let (r, item) = ui.allocate_exact_size(vec2(170.0, 24.0), Sense::click());
                let on = value == *easing;
                if on {
                    ui.painter().rect_filled(r, radius, p.accent_subtle);
                } else if item.hovered() {
                    ui.painter().rect_filled(r, radius, p.hover);
                }
                let curve = Rect::from_min_size(r.min + vec2(6.0, 5.0), vec2(22.0, 14.0));
                paint_curve(ui.painter(), curve, value, p.accent);
                ui.painter().text(
                    pos2(curve.max.x + 6.0, r.center().y),
                    Align2::LEFT_CENTER,
                    tl!(name),
                    FontId::proportional(coursecraft_render::SIZE_BODY),
                    p.text,
                );
                if item.clicked() {
                    *easing = value;
                    changed = true;
                    ui.close();
                }
            }
        });
    });
    changed
}

/// What the Emphasis card asks for.
#[derive(Debug, Clone, PartialEq)]
pub enum EmphasisRequest {
    /// Add this emphasis.
    Add(coursecraft_course::Emphasis),
    /// Replace emphasis `index`.
    Set(usize, coursecraft_course::Emphasis),
    Remove(usize),
    /// Play emphasis `index` once on the canvas.
    Play(usize),
    /// Loop this effect while its tile is hovered.
    Hover(coursecraft_course::EmphasisEffect),
}

/// The Emphasis card of one object: its emphasis list and an Add emphasis picker. `start` is
/// where a new one begins (the playhead or the object's start).
pub fn show_emphasis_card(
    ui: &mut Ui,
    emphasis: &[coursecraft_course::Emphasis],
    start: u32,
    out: &mut Vec<EmphasisRequest>,
) {
    let p = coursecraft_render::palette(ui.ctx());
    let card_w = ui.available_width();
    egui::Frame::NONE
        .fill(p.raised)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(coursecraft_render::RADIUS_MD as u8))
        .inner_margin(egui::Margin::same(coursecraft_render::SPACE_MD as i8))
        .show(ui, |ui| {
            ui.set_width((card_w - 2.0 * (coursecraft_render::SPACE_MD + 1.0)).max(60.0));
            for (i, e) in emphasis.iter().enumerate() {
                if i > 0 {
                    ui.separator();
                }
                emphasis_row(ui, i, e, out);
            }
            if emphasis.len() < coursecraft_course::MAX_EMPHASIS_PER_OBJECT {
                if !emphasis.is_empty() {
                    ui.add_space(4.0);
                }
                let add = ui.add(egui::Button::new(
                    RichText::new(format!("+ {}", tl!("Add emphasis"))).color(p.accent_text),
                ));
                egui::Popup::menu(&add).show(|ui| {
                    ui.set_width(4.0 * 60.0 + 8.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = vec2(2.0, 2.0);
                        for d in coursecraft_course::EMPHASIS {
                            let effect = coursecraft_course::EmphasisEffect::new(d.id);
                            let r =
                                crate::ribbon::gallery::emphasis_tile(ui, d.label, effect.clone())
                                    .on_hover_text(tl!(d.hint));
                            if r.clicked() {
                                out.push(EmphasisRequest::Add(coursecraft_course::Emphasis::new(
                                    d.id, start,
                                )));
                                ui.close();
                            } else if r.hovered() {
                                out.push(EmphasisRequest::Hover(effect));
                            }
                        }
                    });
                });
            }
        });
}

fn emphasis_row(
    ui: &mut Ui,
    i: usize,
    e: &coursecraft_course::Emphasis,
    out: &mut Vec<EmphasisRequest>,
) {
    let p = coursecraft_render::palette(ui.ctx());
    let label = tl!(e.effect.def().map_or("Unknown effect", |d| d.label));
    ui.horizontal(|ui| {
        let (art, _) = ui.allocate_exact_size(vec2(32.0, 24.0), Sense::hover());
        crate::ribbon::effect_art::paint_emphasis(
            ui.painter(),
            art,
            &e.effect,
            p.warning,
            p.border_strong,
            None,
        );
        ui.label(RichText::new(label).color(p.text));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if crate::widgets::icon_button_danger(
                ui,
                coursecraft_render::Icon::Trash,
                true,
                tl!("Remove this emphasis"),
            )
            .clicked()
            {
                out.push(EmphasisRequest::Remove(i));
            }
            if crate::widgets::icon_button(
                ui,
                coursecraft_render::Icon::Play,
                tl!("Play this emphasis on the canvas"),
            )
            .clicked()
            {
                out.push(EmphasisRequest::Play(i));
            }
        });
    });
    // While a field is dragged or typed in, the draft lives in egui's memory: the course holds
    // the old value until the edit ends, and a fresh copy every frame would undo each step of
    // the drag (`N96`).
    let draft_id = ui.make_persistent_id(("emphasis_draft", i));
    let stored = ui.data(|d| d.get_temp::<(u32, u32, u8)>(draft_id));
    let (mut start_ms, mut duration_ms, mut repeat) =
        stored.unwrap_or((e.start_ms, e.duration_ms, e.repeat));
    let mut editing = false;
    let mut done = false;
    let mut seconds_row = |ui: &mut Ui, name: &str, ms: &mut u32, range: (f64, f64)| {
        crate::properties::inspector_row(ui, tl!(name), |ui| {
            let mut s = f64::from(*ms) / 1000.0;
            let r = ui.add(
                egui::DragValue::new(&mut s)
                    .speed(0.05)
                    .range(range.0..=range.1)
                    .fixed_decimals(2)
                    .suffix(" s"),
            );
            *ms = (s * 1000.0).round().clamp(0.0, 600_000.0) as u32;
            let enter = r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            editing |= r.dragged() || r.has_focus();
            done |= r.drag_stopped() || r.lost_focus() || enter;
        });
    };
    seconds_row(ui, "Start", &mut start_ms, (0.0, 600.0));
    // The duration is held to 60 s, so the field stops there too.
    seconds_row(ui, "Duration", &mut duration_ms, (0.05, 60.0));
    crate::properties::inspector_row(ui, tl!("Repeat"), |ui| {
        let mut n = repeat;
        let r = ui.add(
            egui::DragValue::new(&mut n)
                .speed(0.1)
                .range(1..=coursecraft_course::MAX_EMPHASIS_REPEAT)
                .suffix("×"),
        );
        repeat = n;
        editing |= r.dragged() || r.has_focus();
        done |= r.drag_stopped() || r.lost_focus() || (r.changed() && !r.dragged());
    });
    let mut changed = e.clone();
    changed.start_ms = start_ms;
    changed.duration_ms = duration_ms.clamp(1, 60_000);
    changed.repeat = repeat;
    if done || !editing {
        ui.data_mut(|d| d.remove::<(u32, u32, u8)>(draft_id));
    } else {
        ui.data_mut(|d| d.insert_temp(draft_id, (start_ms, duration_ms, repeat)));
    }
    if done && changed != *e {
        out.push(EmphasisRequest::Set(i, changed));
    }
}

/// Sequence: the text plays as one, by paragraph or by letter. Returns whether it changed.
fn sequence_row(
    ui: &mut Ui,
    which: &'static str,
    sequence: &mut coursecraft_course::Sequence,
) -> bool {
    let mut changed = false;
    crate::properties::inspector_row(ui, tl!("Sequence"), |ui| {
        let w = ui.available_width().max(30.0);
        crate::widgets::styled_combo(ui, format!("anim_sequence_{which}"), w)
            .selected_text(tl!(sequence.label()))
            .show_ui(ui, |ui| {
                for v in coursecraft_course::Sequence::ALL {
                    if ui
                        .selectable_label(*sequence == v, tl!(v.label()))
                        .clicked()
                        && *sequence != v
                    {
                        *sequence = v;
                        changed = true;
                    }
                }
            });
    });
    changed
}
