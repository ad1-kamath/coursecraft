//! The ribbon's galleries (docs/ui/ribbon.md › Design, Transitions, Animations):
//! a row of tiles in the tab, all of them under ▾, and the options beside them
//! (effect direction, duration, Apply To All). A pick is one command.

use coursecraft_course::{
    AnimEffect, Animation, Direction, EFFECTS, Easing, Object, Slide, Transition, TransitionEffect,
};
use coursecraft_render::Icon;
use egui::{Align2, Color32, CornerRadius, FontFamily, FontId, Rect, Response, Sense, Stroke, Ui};
use serde_json::json;

use super::spec::{EffectOp, Gallery, Item, Motion};
use crate::Editor;

/// One tile of a gallery.
pub struct Tile {
    pub label: &'static str,
    pub icon: Icon,
    /// A theme's background, accent and text colours, drawn instead of the icon.
    pub swatch: Option<[Color32; 3]>,
    /// An entrance/exit effect, drawn by its own sample function instead of the icon
    /// (`effect_art`); `true` for an exit.
    pub effect: Option<(AnimEffect, bool)>,
    /// An emphasis effect, drawn by its own sample function (amber).
    pub emphasis: Option<coursecraft_course::EmphasisEffect>,
    /// A slide transition, drawn by its own sample (old slide grey, new slide accent).
    pub transition: Option<TransitionEffect>,
    pub selected: bool,
}

/// A built-in theme: its name and the colours `course.set_theme` sets.
struct ThemePreset {
    name: &'static str,
    accent: &'static str,
    background: &'static str,
    text: &'static str,
}

/// CourseCraft's own theme presets (colours chosen for contrast, not copied).
const THEMES: &[ThemePreset] = &[
    ThemePreset {
        name: "CourseCraft",
        accent: "#2F6FEB",
        background: "#FFFFFF",
        text: "#1F1F1F",
    },
    ThemePreset {
        name: "Lagoon",
        accent: "#0E8A8A",
        background: "#F4FBFB",
        text: "#123536",
    },
    ThemePreset {
        name: "Meadow",
        accent: "#3B8A3E",
        background: "#F7FAF2",
        text: "#1E2D1A",
    },
    ThemePreset {
        name: "Ember",
        accent: "#D2522B",
        background: "#FFF8F3",
        text: "#33201A",
    },
    ThemePreset {
        name: "Plum",
        accent: "#7A3E9D",
        background: "#FBF7FD",
        text: "#2A1A33",
    },
    ThemePreset {
        name: "Slate",
        accent: "#4C6A88",
        background: "#F3F5F7",
        text: "#1C2630",
    },
    ThemePreset {
        name: "Night",
        accent: "#F2B13A",
        background: "#1B1F27",
        text: "#F1F3F6",
    },
    ThemePreset {
        name: "Ink",
        accent: "#E04F6D",
        background: "#101820",
        text: "#EDEFF2",
    },
];

fn hex(color: &str) -> Color32 {
    coursecraft_render::parse_color(&coursecraft_course::Color(color.to_owned()))
        .unwrap_or(Color32::GRAY)
}

fn same_color(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

fn slide(editor: &Editor) -> Option<&Slide> {
    editor.session.course.slide(&editor.selection.slide)
}

/// The selected objects of the current slide.
fn objects(editor: &Editor) -> Vec<&Object> {
    let Some(slide) = slide(editor) else {
        return Vec::new();
    };
    editor
        .selection
        .ids()
        .iter()
        .filter_map(|id| slide.object(id).map(|(o, _)| o))
        .collect()
}

fn animation(object: &Object, motion: Motion) -> Option<&Animation> {
    match motion {
        Motion::Entrance => object.entrance.as_ref(),
        Motion::Exit => object.exit.as_ref(),
        Motion::Transition => None,
    }
}

fn transition_index(t: Option<&Transition>) -> usize {
    t.and_then(|t| {
        coursecraft_course::TRANSITIONS
            .iter()
            .position(|d| d.id == t.effect.id())
    })
    .map_or(0, |i| i + 1)
}

/// Transition `def` keeping the options of `old` that it also takes.
fn transition_like(
    def: &coursecraft_course::TransitionDef,
    old: Option<&TransitionEffect>,
) -> Option<TransitionEffect> {
    let fx = def.options_like.map(|like| {
        old.and_then(TransitionEffect::as_effect)
            .map_or_else(|| AnimEffect::new(like), |f| f.switched_to(like))
    });
    TransitionEffect::from_parts(def.id, fx.as_ref())
}

/// Tile index of an animation: 0 is None, then the registry in order. An unknown effect
/// selects no tile.
fn effect_index(a: Option<&Animation>) -> Option<usize> {
    match a {
        None => Some(0),
        Some(a) => EFFECTS
            .iter()
            .position(|d| d.id == a.effect.id)
            .map(|i| i + 1),
    }
}

/// The tiles of `gallery`, with the current one selected.
#[must_use]
pub fn tiles(editor: &Editor, gallery: Gallery) -> Vec<Tile> {
    match gallery {
        Gallery::Themes => {
            let theme = &editor.session.course.theme;
            THEMES
                .iter()
                .map(|t| Tile {
                    label: t.name,
                    icon: Icon::Palette,
                    swatch: Some([hex(t.background), hex(t.accent), hex(t.text)]),
                    effect: None,
                    emphasis: None,
                    transition: None,
                    selected: same_color(&theme.accent.0, t.accent)
                        && same_color(&theme.background.0, t.background)
                        && same_color(&theme.text.0, t.text),
                })
                .collect()
        }
        Gallery::Transitions => {
            let current = slide(editor).map(|s| transition_index(s.transition.as_ref()));
            let old = slide(editor)
                .and_then(|s| s.transition.as_ref())
                .map(|t| t.effect.clone());
            let none = Tile {
                label: "None",
                icon: Icon::XCircle,
                swatch: None,
                effect: None,
                emphasis: None,
                transition: None,
                selected: current == Some(0),
            };
            std::iter::once(none)
                .chain(
                    coursecraft_course::TRANSITIONS
                        .iter()
                        .enumerate()
                        .map(|(i, d)| Tile {
                            label: d.label,
                            icon: Icon::FilmStrip,
                            swatch: None,
                            effect: None,
                            emphasis: None,
                            transition: transition_like(d, old.as_ref()),
                            selected: current == Some(i + 1),
                        }),
                )
                .collect()
        }
        Gallery::Entrance | Gallery::Exit => {
            let motion = motion_of(gallery);
            let first = objects(editor)
                .first()
                .and_then(|o| animation(o, motion))
                .cloned();
            let current = effect_index(first.as_ref());
            let exit = motion == Motion::Exit;
            let none = Tile {
                label: "None",
                icon: Icon::XCircle,
                swatch: None,
                effect: None,
                emphasis: None,
                transition: None,
                selected: current == Some(0),
            };
            std::iter::once(none)
                .chain(EFFECTS.iter().enumerate().map(|(i, d)| {
                    // The current effect's tile shows its chosen options.
                    let effect = match &first {
                        Some(a) if a.effect.id == d.id => a.effect.clone(),
                        _ => AnimEffect::new(d.id),
                    };
                    Tile {
                        label: d.label(exit),
                        icon: Icon::Sparkle,
                        swatch: None,
                        effect: Some((effect, exit)),
                        emphasis: None,
                        transition: None,
                        selected: current == Some(i + 1),
                    }
                }))
                .collect()
        }
        Gallery::Emphasis => coursecraft_course::EMPHASIS
            .iter()
            .map(|d| Tile {
                label: d.label,
                icon: Icon::Sparkle,
                swatch: None,
                effect: None,
                emphasis: Some(coursecraft_course::EmphasisEffect::new(d.id)),
                transition: None,
                selected: false,
            })
            .collect(),
    }
}

fn motion_of(gallery: Gallery) -> Motion {
    match gallery {
        Gallery::Emphasis => Motion::Entrance,
        Gallery::Exit => Motion::Exit,
        Gallery::Entrance => Motion::Entrance,
        Gallery::Themes | Gallery::Transitions => Motion::Transition,
    }
}

/// Whether `gallery` can be used now, with the reason when it cannot.
pub fn available(editor: &Editor, gallery: Gallery) -> Result<(), &'static str> {
    match gallery {
        Gallery::Themes => Ok(()),
        Gallery::Transitions => transition_available(editor),
        Gallery::Entrance | Gallery::Exit | Gallery::Emphasis => animation_available(editor),
    }
}

fn transition_available(editor: &Editor) -> Result<(), &'static str> {
    if editor.ui.master_view {
        Err(super::MASTER)
    } else if slide(editor).is_none() {
        Err("Select a slide first")
    } else {
        Ok(())
    }
}

fn animation_available(editor: &Editor) -> Result<(), &'static str> {
    if editor.ui.editing_state.is_some() {
        Err(super::STATE)
    } else if objects(editor).is_empty() {
        Err("Select an object first")
    } else {
        Ok(())
    }
}

/// Pick tile `index` of `gallery`.
pub fn apply(editor: &mut Editor, gallery: Gallery, index: usize) {
    if available(editor, gallery).is_err() {
        return;
    }
    match gallery {
        Gallery::Emphasis => add_emphasis(editor, index),
        Gallery::Themes => {
            if let Some(t) = THEMES.get(index) {
                editor.run(
                    "course.set_theme",
                    json!({ "accent": t.accent, "background": t.background, "text": t.text }),
                );
            }
        }
        Gallery::Transitions => {
            let Some(s) = slide(editor) else {
                return;
            };
            let old = s.transition.as_ref();
            let duration_ms = old.map_or(500, |t| t.duration_ms);
            let effect = index
                .checked_sub(1)
                .and_then(|i| coursecraft_course::TRANSITIONS.get(i))
                .and_then(|d| transition_like(d, old.map(|t| &t.effect)));
            let transition = effect.map(|effect| Transition {
                effect,
                duration_ms,
            });
            let id = s.id.clone();
            set_transition(editor, &id, transition.as_ref());
        }
        Gallery::Entrance | Gallery::Exit => {
            let motion = motion_of(gallery);
            let changes: Vec<(String, String, Option<Animation>)> = {
                let Some(s) = slide(editor) else {
                    return;
                };
                objects(editor)
                    .iter()
                    .map(|o| {
                        let old = animation(o, motion);
                        let def = index.checked_sub(1).and_then(|i| EFFECTS.get(i));
                        let effect = def.map(|d| {
                            old.map_or_else(
                                || AnimEffect::new(d.id),
                                |a| a.effect.switched_to(d.id),
                            )
                        });
                        let animation = effect.map(|effect| Animation {
                            effect,
                            // Appear has a default of 0: the engine needs 1 ms at least.
                            duration_ms: old
                                .map_or(def.map_or(500, |d| d.default_duration_ms), |a| {
                                    a.duration_ms
                                })
                                .clamp(1, 60_000),
                            easing: old.map_or(Easing::EaseOut, |a| a.easing),
                            // The text sequence belongs to the object, not to the effect.
                            sequence: old.map_or_else(Default::default, |a| a.sequence),
                        });
                        (s.id.clone(), o.id.clone(), animation)
                    })
                    .collect()
            };
            // One pick is one undo step, however many objects are selected.
            let steps = changes
                .iter()
                .map(|(slide, object, animation)| {
                    set_animation_step(slide, object, motion, animation.as_ref())
                })
                .collect();
            run_as_one_step(editor, steps);
            if let Some(d) = index.checked_sub(1).and_then(|i| EFFECTS.get(i)) {
                editor.ui.anim_preview.picked(d.id);
            }
            preview_current(editor, motion);
        }
    }
}

/// Add emphasis `index` of the registry to every selected object, at the timeline's playhead
/// when it is inside the object's time on screen, else at the object's start. Plays it once.
fn add_emphasis(editor: &mut Editor, index: usize) {
    let Some(def) = coursecraft_course::EMPHASIS.get(index) else {
        return;
    };
    let playhead = editor.ui.timeline.playhead_ms;
    let adds: Vec<(String, String, u32)> = {
        let Some(s) = slide(editor) else {
            return;
        };
        objects(editor)
            .iter()
            .map(|o| {
                let inside = playhead >= o.timing.start_ms
                    && o.timing.end_ms.is_none_or(|end| playhead < end);
                let start = if inside { playhead } else { o.timing.start_ms };
                (s.id.clone(), o.id.clone(), start)
            })
            .collect()
    };
    let steps = adds
        .iter()
        .map(|(slide, object, start)| {
            (
                "object.add_emphasis".to_owned(),
                json!({ "slide": slide, "object": object, "effect": def.id, "start_ms": start }),
            )
        })
        .collect();
    run_as_one_step(editor, steps);
    if editor.ui.anim_preview.auto && !adds.is_empty() {
        let slide = editor.selection.slide.clone();
        let ids = editor.selection.ids();
        editor.ui.anim_preview.play_emphasis(
            &slide,
            ids,
            coursecraft_course::Emphasis::new(def.id, 0),
        );
    }
}

fn slot_of(motion: Motion) -> crate::anim_preview::Slot {
    crate::anim_preview::Slot::of_exit(motion == Motion::Exit)
}

/// Play the first selected object's `motion` animation once on the canvas, when
/// Auto-preview is on.
pub fn preview_current(editor: &mut Editor, motion: Motion) {
    if !editor.ui.anim_preview.auto || motion == Motion::Transition {
        return;
    }
    let Some(a) = objects(editor)
        .first()
        .and_then(|o| animation(o, motion))
        .cloned()
    else {
        return;
    };
    let slide = editor.selection.slide.clone();
    let ids = editor.selection.ids();
    editor.ui.anim_preview.play(&slide, ids, slot_of(motion), a);
}

/// Loop `effect` on the selected objects while its tile is hovered (keeps their duration and
/// easing, or the effect's defaults).
pub fn preview_hover(
    editor: &mut Editor,
    ctx: &egui::Context,
    motion: Motion,
    effect: &AnimEffect,
) {
    let old = objects(editor)
        .first()
        .and_then(|o| animation(o, motion))
        .cloned();
    let duration = old.as_ref().map_or_else(
        || effect.def().map_or(500, |d| d.default_duration_ms),
        |a| a.duration_ms,
    );
    let easing = old.map_or(Easing::EaseOut, |a| a.easing);
    // A hovered tile shows its own effect, with the options the object has where they apply.
    let effect = match objects(editor).first().and_then(|o| animation(o, motion)) {
        Some(a) if a.effect.id != effect.id => a.effect.switched_to(&effect.id),
        _ => effect.clone(),
    };
    let slide = editor.selection.slide.clone();
    let ids = editor.selection.ids();
    editor.ui.anim_preview.hover(
        &slide,
        ids,
        slot_of(motion),
        effect,
        duration,
        easing,
        ctx.cumulative_pass_nr(),
    );
}

fn set_transition(editor: &mut Editor, slide: &str, transition: Option<&Transition>) {
    editor.run(
        "slide.set_transition",
        json!({ "slide": slide, "transition": transition }),
    );
}

/// The `object.set_animation` step for one object's entrance or exit.
fn set_animation_step(
    slide: &str,
    object: &str,
    motion: Motion,
    animation: Option<&Animation>,
) -> (String, serde_json::Value) {
    let which = if motion == Motion::Exit {
        "exit"
    } else {
        "entrance"
    };
    (
        "object.set_animation".to_owned(),
        json!({ "slide": slide, "object": object, "which": which, "animation": animation }),
    )
}

/// Run `steps` as one undo step: a ribbon pick on several selected objects is one action.
fn run_as_one_step(editor: &mut Editor, mut steps: Vec<(String, serde_json::Value)>) {
    match steps.len() {
        0 => {}
        1 => {
            let (id, params) = steps.remove(0);
            editor.run(&id, params);
        }
        _ => {
            editor.run_batch(&steps);
        }
    }
}

fn transition_from(effect: &TransitionEffect) -> Option<Direction> {
    effect.as_effect().and_then(|fx| fx.from)
}

fn effect_from(effect: &AnimEffect) -> Option<Direction> {
    effect.direction()
}

/// The current direction of `motion`'s effect, when it has one.
fn current_from(editor: &Editor, motion: Motion) -> Option<Direction> {
    match motion {
        Motion::Transition => slide(editor)
            .and_then(|s| s.transition.as_ref())
            .and_then(|t| transition_from(&t.effect)),
        Motion::Entrance | Motion::Exit => objects(editor)
            .first()
            .and_then(|o| animation(o, motion))
            .and_then(|a| effect_from(&a.effect)),
    }
}

/// The first selected object's effect of `motion` (entrance or exit).
fn current_effect(editor: &Editor, motion: Motion) -> Option<AnimEffect> {
    objects(editor)
        .first()
        .and_then(|o| animation(o, motion))
        .map(|a| a.effect.clone())
}

/// Whether `motion`'s current effect can come from `dir` (transitions take every side).
fn allows_from(editor: &Editor, motion: Motion, dir: Direction) -> bool {
    match motion {
        Motion::Transition => true,
        Motion::Entrance | Motion::Exit => objects(editor)
            .first()
            .and_then(|o| animation(o, motion))
            .and_then(|a| a.effect.def())
            .and_then(coursecraft_course::EffectDef::direction)
            .is_some_and(|(_, allowed)| allowed.contains(&dir)),
    }
}

/// The current duration of `motion`'s effect, in ms.
fn current_duration(editor: &Editor, motion: Motion) -> Option<u32> {
    match motion {
        Motion::Transition => slide(editor)
            .and_then(|s| s.transition.as_ref())
            .map(|t| t.duration_ms),
        Motion::Entrance | Motion::Exit => objects(editor)
            .first()
            .and_then(|o| animation(o, motion))
            .map(|a| a.duration_ms),
    }
}

/// Whether an effect option can be used now.
pub fn effect_available(editor: &Editor, op: EffectOp) -> Result<(), &'static str> {
    let base = |motion: Motion| match motion {
        Motion::Transition => transition_available(editor),
        Motion::Entrance | Motion::Exit => animation_available(editor),
    };
    match op {
        EffectOp::Options(Motion::Transition) => {
            base(Motion::Transition)?;
            if current_transition_options(editor).is_some() {
                Ok(())
            } else {
                Err("Pick a transition that has options first")
            }
        }
        EffectOp::Options(motion) => {
            base(motion)?;
            let effect = current_effect(editor, motion);
            let has_options = effect
                .as_ref()
                .is_some_and(|e| e.def().is_some_and(|d| !d.options.is_empty()));
            if has_options || (effect.is_some() && first_has_text(editor)) {
                Ok(())
            } else {
                Err("Pick an effect that has options first")
            }
        }
        EffectOp::From(motion, dir) => {
            base(motion)?;
            if current_from(editor, motion).is_none() {
                return Err("Pick an effect that comes from a side first");
            }
            if allows_from(editor, motion, dir) {
                Ok(())
            } else {
                Err("This effect can't come from this side")
            }
        }
        EffectOp::Duration(motion) => {
            base(motion)?;
            if current_duration(editor, motion).is_some() {
                Ok(())
            } else {
                Err("Pick an effect first")
            }
        }
        EffectOp::TransitionToAll => transition_available(editor),
    }
}

/// Whether an effect option shows as on.
#[must_use]
pub fn effect_selected(editor: &Editor, op: EffectOp) -> bool {
    match op {
        EffectOp::From(motion, dir) => current_from(editor, motion) == Some(dir),
        _ => false,
    }
}

/// Run an effect option (the duration field commits its own value).
pub fn run_effect(editor: &mut Editor, op: EffectOp) {
    if effect_available(editor, op).is_err() {
        return;
    }
    match op {
        EffectOp::From(motion, dir) => set_from(editor, motion, dir),
        EffectOp::TransitionToAll => {
            let transition = slide(editor).and_then(|s| s.transition.clone());
            editor.run(
                "slide.set_transition_all",
                json!({ "transition": transition }),
            );
        }
        EffectOp::Options(_) | EffectOp::Duration(_) => {}
    }
}

fn set_from(editor: &mut Editor, motion: Motion, dir: Direction) {
    let with_from = |effect: &AnimEffect| match effect.def().and_then(|d| d.direction()) {
        Some((_, allowed)) if allowed.contains(&dir) => AnimEffect::with_from(&effect.id, dir),
        _ => effect.clone(),
    };
    update(
        editor,
        motion,
        |t| {
            if let Some(mut fx) = t.effect.as_effect()
                && fx.from.is_some()
            {
                fx.from = Some(dir);
                if let Some(e) = TransitionEffect::from_parts(t.effect.id(), Some(&fx)) {
                    t.effect = e;
                }
            }
        },
        |a| a.effect = with_from(&a.effect),
    );
}

fn set_duration(editor: &mut Editor, motion: Motion, ms: u32) {
    update(
        editor,
        motion,
        |t| t.duration_ms = ms,
        |a| a.duration_ms = ms,
    );
}

/// Change the current slide's transition, or every selected object's
/// animation of `motion`, where it has one.
fn update(
    editor: &mut Editor,
    motion: Motion,
    transition: impl Fn(&mut Transition),
    anim: impl Fn(&mut Animation),
) {
    let Some(s) = slide(editor) else {
        return;
    };
    let id = s.id.clone();
    match motion {
        Motion::Transition => {
            if let Some(mut t) = s.transition.clone() {
                transition(&mut t);
                set_transition(editor, &id, Some(&t));
            }
        }
        Motion::Entrance | Motion::Exit => {
            let changes: Vec<(String, Animation)> = objects(editor)
                .iter()
                .filter_map(|o| {
                    let mut a = animation(o, motion)?.clone();
                    anim(&mut a);
                    Some((o.id.clone(), a))
                })
                .collect();
            let steps = changes
                .iter()
                .map(|(object, a)| set_animation_step(&id, object, motion, Some(a)))
                .collect();
            run_as_one_step(editor, steps);
        }
    }
}

/// The Duration field of `motion`, in seconds; a committed edit is one command.
pub fn duration_field(
    editor: &mut Editor,
    ui: &mut Ui,
    item: &'static Item,
    motion: Motion,
    width: f32,
    enabled: bool,
) -> Response {
    let current = current_duration(editor, motion).unwrap_or(500);
    let inner = ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let p = coursecraft_render::palette(ui.ctx());
        let color = if enabled { p.text } else { p.text_disabled };
        ui.label(egui::RichText::new(tl!(item.label)).size(11.0).color(color));
        ui.add_enabled_ui(enabled, |ui| {
            ui.set_width(width);
            crate::grid_panel::committed_number(
                ui,
                egui::Id::unique(("ribbon_duration", item.id)),
                current as f32 / 1000.0,
                0.1..=60.0,
                " s",
            )
        })
        .inner
    });
    if let Some(seconds) = inner.inner {
        let ms = (seconds * 1000.0).round().clamp(1.0, 60_000.0) as u32;
        if ms != current {
            set_duration(editor, motion, ms);
        }
    }
    inner.response
}

/// Size of one tile.
const TILE: egui::Vec2 = egui::vec2(58.0, 62.0);

/// Draw a tile; returns its response.
fn tile(ui: &mut Ui, t: &Tile, enabled: bool) -> Response {
    let p = coursecraft_render::palette(ui.ctx());
    let label = tl!(t.label);
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(TILE, sense);
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, enabled, t.selected, label));
    let radius = CornerRadius::same(coursecraft_render::RADIUS_SM as u8);
    if t.selected {
        ui.painter().rect_filled(rect, radius, p.accent_subtle);
        ui.painter().rect_stroke(
            rect,
            radius,
            Stroke::new(1.0, p.accent),
            egui::StrokeKind::Inside,
        );
    } else if enabled && resp.hovered() {
        ui.painter().rect_filled(rect, radius, p.hover);
    }
    let ink = if enabled { p.text } else { p.text_disabled };
    let art = Rect::from_center_size(
        egui::pos2(rect.center().x, rect.min.y + 22.0),
        egui::vec2(40.0, 30.0),
    );
    match t.swatch {
        Some([background, accent, text]) => {
            ui.painter()
                .rect_filled(art, CornerRadius::same(3), background);
            ui.painter().rect_stroke(
                art,
                CornerRadius::same(3),
                Stroke::new(1.0, p.border),
                egui::StrokeKind::Inside,
            );
            ui.painter().rect_filled(
                Rect::from_min_size(art.min, egui::vec2(art.width(), 6.0)),
                CornerRadius::same(2),
                accent,
            );
            ui.painter().text(
                art.center() + egui::vec2(0.0, 3.0),
                Align2::CENTER_CENTER,
                "Aa",
                FontId::proportional(12.0),
                text,
            );
        }
        None if t.transition.is_some() => {
            if let Some(effect) = &t.transition {
                let at = (enabled && resp.hovered()).then(|| {
                    ui.ctx().request_repaint();
                    super::effect_art::hover_shown(ui.input(|i| i.time))
                });
                let (old, new) = if enabled {
                    (p.border_strong, p.accent)
                } else {
                    (p.border, p.text_disabled)
                };
                super::effect_art::paint_transition(ui.painter(), art, effect, old, new, at);
            }
        }
        None if t.emphasis.is_some() => {
            if let Some(effect) = &t.emphasis {
                let at = (enabled && resp.hovered()).then(|| {
                    ui.ctx().request_repaint();
                    super::effect_art::hover_shown(ui.input(|i| i.time))
                });
                let ink = if enabled { p.warning } else { p.text_disabled };
                super::effect_art::paint_emphasis(
                    ui.painter(),
                    art,
                    effect,
                    ink,
                    p.border_strong,
                    at,
                );
            }
        }
        None => match &t.effect {
            Some((effect, exit)) => {
                let kind = if !enabled {
                    p.text_disabled
                } else if *exit {
                    p.danger
                } else {
                    p.success
                };
                // On hover the icon plays its effect in a loop.
                let at = (enabled && resp.hovered()).then(|| {
                    ui.ctx().request_repaint();
                    super::effect_art::hover_shown(ui.input(|i| i.time))
                });
                super::effect_art::paint_effect(
                    ui.painter(),
                    art,
                    effect,
                    *exit,
                    kind,
                    p.border_strong,
                    at,
                );
            }
            None => {
                ui.painter().text(
                    art.center(),
                    Align2::CENTER_CENTER,
                    t.icon.codepoint(),
                    FontId::new(22.0, FontFamily::Name("phosphor".into())),
                    ink,
                );
            }
        },
    }
    ui.painter().text(
        egui::pos2(rect.center().x, rect.max.y - 9.0),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(10.5),
        ink,
    );
    resp.on_hover_text(label)
}

/// One effect tile (the Animate panel's effect gallery uses the ribbon's tiles): `effect`
/// drawn by its own sample function, or the None tile.
pub fn effect_tile(
    ui: &mut Ui,
    label: &'static str,
    effect: Option<AnimEffect>,
    exit: bool,
    selected: bool,
) -> Response {
    let t = Tile {
        label,
        icon: Icon::XCircle,
        swatch: None,
        effect: effect.map(|e| (e, exit)),
        emphasis: None,
        transition: None,
        selected,
    };
    tile(ui, &t, true)
}

/// One emphasis tile (the Animate panel's emphasis picker).
pub fn emphasis_tile(
    ui: &mut Ui,
    label: &'static str,
    effect: coursecraft_course::EmphasisEffect,
) -> Response {
    let t = Tile {
        label,
        icon: Icon::Sparkle,
        swatch: None,
        effect: None,
        emphasis: Some(effect),
        transition: None,
        selected: false,
    };
    tile(ui, &t, true)
}

/// Draw `gallery` in the ribbon: its first `shown` tiles and a ▾ for all.
pub fn show(
    editor: &mut Editor,
    ui: &mut Ui,
    item: &'static Item,
    gallery: Gallery,
    shown: u8,
    enabled: bool,
) -> Response {
    let tiles = tiles(editor, gallery);
    let mut picked = None;
    let mut hovered = None;
    let inner = ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        // Keep the selected tile in view.
        let shown = usize::from(shown.max(1)).min(tiles.len());
        let start = tiles
            .iter()
            .position(|t| t.selected)
            .map_or(0, |i| i.saturating_sub(shown.saturating_sub(1)));
        for (i, t) in tiles.iter().enumerate().skip(start).take(shown) {
            let r = tile(ui, t, enabled);
            if r.clicked() {
                picked = Some(i);
            }
            if r.hovered() {
                hovered = Some(i);
            }
        }
        let (more_rect, more) = ui.allocate_exact_size(
            egui::vec2(16.0, TILE.y),
            if enabled {
                Sense::click()
            } else {
                Sense::hover()
            },
        );
        let p = coursecraft_render::palette(ui.ctx());
        if enabled && more.hovered() {
            ui.painter().rect_filled(more_rect, 3.0, p.hover);
        }
        super::widgets::chevron(
            ui.painter(),
            more_rect.center(),
            4.0,
            if enabled { p.text } else { p.text_disabled },
        );
        let name = craft_i18n::fmt(tl!("All {gallery}"), &[("gallery", tl!(item.label))]);
        let more = more.on_hover_text(&name);
        more.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, &name));
        egui::Popup::menu(&more).show(|ui| {
            ui.set_max_width(TILE.x * 5.0 + 12.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(2.0, 2.0);
                for (i, t) in tiles.iter().enumerate() {
                    let r = tile(ui, t, enabled);
                    if r.clicked() {
                        picked = Some(i);
                        ui.close();
                    }
                    if r.hovered() {
                        hovered = Some(i);
                    }
                }
            });
        });
    });
    let hovered = hovered.and_then(|i| tiles.get(i));
    if let Some(index) = picked {
        apply(editor, gallery, index);
    } else if enabled && let Some((effect, _)) = hovered.and_then(|t| t.effect.as_ref()) {
        preview_hover(editor, ui.ctx(), motion_of(gallery), effect);
    } else if enabled && let Some(effect) = hovered.and_then(|t| t.emphasis.as_ref()) {
        let slide = editor.selection.slide.clone();
        let ids = editor.selection.ids();
        editor
            .ui
            .anim_preview
            .hover_emphasis(&slide, ids, effect, ui.ctx().cumulative_pass_nr());
    }
    inner.response
}

/// Effect Options ▾ of the Entrance or Exit gallery: one section per option of the current
/// effect, from the registry. Direction is a compass; the rest are rows. A pick sets that
/// option on every selected object whose effect takes it.
pub fn effect_options_menu(editor: &mut Editor, ui: &mut Ui, motion: Motion) {
    use super::effect_art::{OptionValue, paint_glyph};
    let current = if motion == Motion::Transition {
        current_transition_options(editor)
    } else {
        current_effect(editor, motion)
    };
    let Some(effect) = current else {
        return;
    };
    let Some(def) = effect.def() else {
        return;
    };
    let exit = motion == Motion::Exit;
    let p = coursecraft_render::palette(ui.ctx());
    let mut picked = None;
    ui.set_min_width(150.0);
    for (n, option) in def.options.iter().enumerate() {
        if n > 0 {
            ui.separator();
        }
        ui.label(
            egui::RichText::new(tl!(option.label()))
                .size(11.0)
                .color(p.text_secondary),
        );
        let values = OptionValue::all(option);
        let label = |v: OptionValue| match v {
            OptionValue::Spokes(n) if n != 1 => {
                craft_i18n::fmt(tl!(v.label()), &[("n", n.to_string().as_str())])
            }
            _ => tl!(v.label()).to_owned(),
        };
        let mut button = |ui: &mut Ui, v: OptionValue, size: egui::Vec2, text: bool| {
            let on = v.is_set(&effect);
            let name = label(v);
            let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
            resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, on, &name));
            let radius = CornerRadius::same(coursecraft_render::RADIUS_SM as u8);
            if on {
                ui.painter().rect_filled(rect, radius, p.accent_subtle);
            } else if resp.hovered() {
                ui.painter().rect_filled(rect, radius, p.hover);
            }
            let ink = if on { p.accent_text } else { p.text };
            let glyph_at = if text {
                egui::pos2(rect.min.x + 13.0, rect.center().y)
            } else {
                rect.center()
            };
            paint_glyph(ui.painter(), glyph_at, 16.0, ink, v.glyph(exit));
            if text {
                ui.painter().text(
                    egui::pos2(rect.min.x + 28.0, rect.center().y),
                    Align2::LEFT_CENTER,
                    &name,
                    FontId::proportional(12.0),
                    ink,
                );
            }
            let resp = if text {
                resp
            } else {
                resp.on_hover_text(&name)
            };
            if resp.clicked() {
                picked = Some(v);
            }
        };
        if let coursecraft_course::EffectOptionDef::Direction { .. } = option {
            // A 3×3 compass: each cell is the side or corner the object comes from.
            const GRID: [[Option<Direction>; 3]; 3] = [
                [
                    Some(Direction::TopLeft),
                    Some(Direction::Top),
                    Some(Direction::TopRight),
                ],
                [Some(Direction::Left), None, Some(Direction::Right)],
                [
                    Some(Direction::BottomLeft),
                    Some(Direction::Bottom),
                    Some(Direction::BottomRight),
                ],
            ];
            egui::Grid::new(("effect_compass", exit))
                .spacing(egui::vec2(2.0, 2.0))
                .show(ui, |ui| {
                    for row in GRID {
                        for cell in row {
                            match cell.map(OptionValue::From).filter(|v| values.contains(v)) {
                                Some(v) => button(ui, v, egui::vec2(26.0, 26.0), false),
                                None => {
                                    ui.allocate_exact_size(egui::vec2(26.0, 26.0), Sense::hover());
                                }
                            }
                        }
                        ui.end_row();
                    }
                });
        } else {
            for v in values {
                button(
                    ui,
                    v,
                    egui::vec2(ui.available_width().max(140.0), 24.0),
                    true,
                );
            }
        }
    }
    if let Some(v) = picked {
        if motion == Motion::Transition {
            set_transition_option(editor, v);
        } else {
            set_option(editor, motion, v);
        }
        ui.close();
    }
    if motion != Motion::Transition && first_has_text(editor) {
        if !def.options.is_empty() {
            ui.separator();
        }
        sequence_section(editor, ui, motion);
    }
}

/// The current slide's transition options, in their entrance-effect form.
fn current_transition_options(editor: &Editor) -> Option<AnimEffect> {
    slide(editor)
        .and_then(|s| s.transition.as_ref())
        .and_then(|t| t.effect.as_effect())
}

/// Set option `value` on the current slide's transition.
pub fn set_transition_option(editor: &mut Editor, value: super::effect_art::OptionValue) {
    update(
        editor,
        Motion::Transition,
        |t| {
            if let Some(mut fx) = t.effect.as_effect() {
                value.set(&mut fx);
                if let Some(e) = TransitionEffect::from_parts(t.effect.id(), Some(&fx)) {
                    t.effect = e;
                }
            }
        },
        |_| {},
    );
}

/// Whether the first selected object has text to play in parts.
fn first_has_text(editor: &Editor) -> bool {
    objects(editor).first().is_some_and(|o| match &o.kind {
        coursecraft_course::ObjectKind::Text { .. } => true,
        coursecraft_course::ObjectKind::Shape { text, .. } => text.is_some(),
        _ => false,
    })
}

/// The Sequence section of Effect Options: as one object, by paragraph, by letter.
fn sequence_section(editor: &mut Editor, ui: &mut Ui, motion: Motion) {
    use coursecraft_course::Sequence;
    let p = coursecraft_render::palette(ui.ctx());
    let current = objects(editor)
        .first()
        .and_then(|o| animation(o, motion))
        .map(|a| a.sequence);
    ui.label(
        egui::RichText::new(tl!("Sequence"))
            .size(11.0)
            .color(p.text_secondary),
    );
    let mut picked = None;
    for v in Sequence::ALL {
        let icon = match v {
            Sequence::AsOne => Icon::Square,
            Sequence::ByParagraph => Icon::ListBullets,
            Sequence::ByLetter => Icon::TextAa,
        };
        let on = current == Some(v);
        let name = tl!(v.label());
        let size = egui::vec2(ui.available_width().max(140.0), 24.0);
        let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
        resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, on, name));
        let radius = CornerRadius::same(coursecraft_render::RADIUS_SM as u8);
        if on {
            ui.painter().rect_filled(rect, radius, p.accent_subtle);
        } else if resp.hovered() {
            ui.painter().rect_filled(rect, radius, p.hover);
        }
        let ink = if on { p.accent_text } else { p.text };
        super::effect_art::paint_glyph(
            ui.painter(),
            egui::pos2(rect.min.x + 13.0, rect.center().y),
            16.0,
            ink,
            super::effect_art::Glyph::Icon(icon),
        );
        ui.painter().text(
            egui::pos2(rect.min.x + 28.0, rect.center().y),
            Align2::LEFT_CENTER,
            name,
            FontId::proportional(12.0),
            ink,
        );
        if resp.clicked() && !on {
            picked = Some(v);
        }
    }
    if let Some(v) = picked {
        set_sequence(editor, motion, v);
        ui.close();
    }
}

/// Set the text sequence of every selected object's `motion` animation.
pub fn set_sequence(editor: &mut Editor, motion: Motion, sequence: coursecraft_course::Sequence) {
    update(editor, motion, |_| {}, |a| a.sequence = sequence);
    preview_current(editor, motion);
}

/// Set option `value` on every selected object's effect of `motion` that takes it.
pub fn set_option(editor: &mut Editor, motion: Motion, value: super::effect_art::OptionValue) {
    update(
        editor,
        motion,
        |_| {},
        |a| {
            let mut fx = a.effect.clone();
            value.set(&mut fx);
            if fx.def().is_some_and(|d| d.option_problems(&fx).is_empty()) {
                a.effect = fx;
            }
        },
    );
    preview_current(editor, motion);
}
