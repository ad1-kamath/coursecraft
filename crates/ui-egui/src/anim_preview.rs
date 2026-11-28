//! M46 phase 5: live preview of an entrance or exit on the canvas
//! (docs/plans/m46-animation-system.md › Live preview on the canvas).
//!
//! The canvas plays the selected objects' animation in place, through the same
//! `coursecraft-logic` sampler as the player; the other objects stay still. It plays once when
//! an effect or option is picked or ▶ Preview is pressed, and loops while the pointer rests on
//! an effect tile. It stops when the selection changes or the canvas is clicked.

use coursecraft_course::{AnimEffect, Animation, Course, Easing, Emphasis, EmphasisEffect};
use coursecraft_logic::{AnimationPhase, ObjectSample, ease, sample_animation};
use coursecraft_render::{DrawItem, Hit};

/// Which animation of the object plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Entrance,
    Exit,
}

impl Slot {
    #[must_use]
    pub const fn of_exit(exit: bool) -> Self {
        if exit { Self::Exit } else { Self::Entrance }
    }
}

/// Plays once, or loops while a tile stays hovered.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Mode {
    Once,
    /// Kept alive by [`hover`] every frame; `pass` is the last frame it was called.
    Hover {
        pass: u64,
    },
}

/// What a preview plays.
#[derive(Debug, Clone, PartialEq)]
enum Played {
    Anim(Slot, Animation),
    Emphasis(Emphasis),
}

impl Played {
    /// Length of the whole run, in ms.
    fn duration_ms(&self) -> f64 {
        match self {
            Self::Anim(_, a) => f64::from(a.duration_ms.max(1)),
            Self::Emphasis(e) => f64::from(e.duration_ms.max(1)) * f64::from(e.repeat.clamp(1, 20)),
        }
    }

    /// The text sequence the run plays in, when it plays in parts.
    fn sequence(&self) -> Option<coursecraft_course::Sequence> {
        match self {
            Self::Anim(_, a) if !a.sequence.is_as_one() => Some(a.sequence),
            _ => None,
        }
    }

    /// The sample of part `part` (`(k, n)`) of an object with `frame` at `t_ms` into the run.
    fn sample(
        &self,
        t_ms: f64,
        frame: coursecraft_course::Rect,
        size: coursecraft_course::Size,
        part: (usize, usize),
    ) -> ObjectSample {
        match self {
            Self::Anim(slot, a) => {
                let raw = (t_ms / f64::from(a.duration_ms.max(1))).clamp(0.0, 1.0) as f32;
                let progress = coursecraft_logic::part_progress(a.sequence, part, raw);
                let phase = match slot {
                    Slot::Entrance => AnimationPhase::Entering,
                    Slot::Exit => AnimationPhase::Exiting,
                };
                sample_animation(a, phase, progress, frame, size)
            }
            Self::Emphasis(e) => {
                let dur = f64::from(e.duration_ms.max(1));
                if t_ms <= 0.0 || t_ms >= self.duration_ms() {
                    return ObjectSample::IDENTITY;
                }
                let t = ease(e.easing, ((t_ms % dur) / dur) as f32);
                coursecraft_logic::effects::sample_emphasis(&e.effect, t, frame)
            }
        }
    }
}

/// One preview in progress.
#[derive(Debug, Clone, PartialEq)]
pub struct AnimPreview {
    slide: String,
    objects: Vec<String>,
    group: Option<String>,
    what: Played,
    mode: Mode,
    /// Input time the preview started at; set by the first canvas frame that draws it.
    started: Option<f64>,
}

/// The editor's preview state.
#[derive(Debug, Clone, PartialEq)]
pub struct PreviewState {
    pub current: Option<AnimPreview>,
    /// Auto-preview: play when an effect or option is picked, and on hover.
    pub auto: bool,
    /// Effect ids picked recently, newest first (the effect picker's Recent row).
    pub recent: Vec<String>,
    /// The effect picker's search text.
    pub search: String,
}

impl Default for PreviewState {
    fn default() -> Self {
        Self {
            current: None,
            auto: true,
            recent: Vec::new(),
            search: String::new(),
        }
    }
}

/// How many recent effects the picker keeps.
pub const RECENT_MAX: usize = 6;
/// Hold at rest (entrance) or hidden (exit) after a pass, in ms.
const HOLD_MS: f64 = 600.0;
/// Hidden (entrance) or at rest (exit) before a pass, in ms.
const LEAD_MS: f64 = 150.0;

impl PreviewState {
    /// Play `animation` of `slot` once on `objects` of `slide` (from the next canvas frame).
    pub fn play(&mut self, slide: &str, objects: Vec<String>, slot: Slot, animation: Animation) {
        self.play_target(slide, objects, None, Played::Anim(slot, animation));
    }

    /// Play `animation` of `slot` once as a single unit on `group` (with member `objects`).
    pub fn play_group(
        &mut self,
        slide: &str,
        objects: Vec<String>,
        group: String,
        slot: Slot,
        animation: Animation,
    ) {
        self.play_target(slide, objects, Some(group), Played::Anim(slot, animation));
    }

    /// Play `emphasis` once on `objects` of `slide` (from its first pass, not its start time).
    pub fn play_emphasis(&mut self, slide: &str, objects: Vec<String>, emphasis: Emphasis) {
        self.play_target(slide, objects, None, Played::Emphasis(emphasis));
    }

    /// Play `emphasis` once as a single unit on `group` (with member `objects`).
    pub fn play_group_emphasis(
        &mut self,
        slide: &str,
        objects: Vec<String>,
        group: String,
        emphasis: Emphasis,
    ) {
        self.play_target(slide, objects, Some(group), Played::Emphasis(emphasis));
    }

    fn play_target(
        &mut self,
        slide: &str,
        objects: Vec<String>,
        group: Option<String>,
        what: Played,
    ) {
        if objects.is_empty() {
            return;
        }
        self.current = Some(AnimPreview {
            slide: slide.to_owned(),
            objects,
            group,
            what,
            mode: Mode::Once,
            started: None,
        });
    }

    /// Loop emphasis `effect` (one pass of its default length) while called every frame.
    pub fn hover_emphasis(
        &mut self,
        slide: &str,
        objects: Vec<String>,
        effect: &EmphasisEffect,
        pass: u64,
    ) {
        let e = Emphasis::new(&effect.id, 0);
        self.hover_played(slide, objects, None, Played::Emphasis(e), pass);
    }

    /// Loop emphasis `effect` as a single unit on `group` while called every frame.
    pub fn hover_group_emphasis(
        &mut self,
        slide: &str,
        objects: Vec<String>,
        group: String,
        effect: &EmphasisEffect,
        pass: u64,
    ) {
        let e = Emphasis::new(&effect.id, 0);
        self.hover_played(slide, objects, Some(group), Played::Emphasis(e), pass);
    }

    fn hover_played(
        &mut self,
        slide: &str,
        objects: Vec<String>,
        group: Option<String>,
        what: Played,
        pass: u64,
    ) {
        if !self.auto || objects.is_empty() {
            return;
        }
        if let Some(p) = &mut self.current
            && matches!(p.mode, Mode::Hover { .. })
            && p.what == what
            && p.objects == objects
            && p.group == group
        {
            p.mode = Mode::Hover { pass };
            return;
        }
        self.current = Some(AnimPreview {
            slide: slide.to_owned(),
            objects,
            group,
            what,
            mode: Mode::Hover { pass },
            started: None,
        });
    }

    /// Loop `effect` (with `duration_ms` and `easing`) while called every frame (`pass` is
    /// `ctx.cumulative_pass_nr()`). Does nothing when Auto-preview is off.
    #[allow(clippy::too_many_arguments)]
    pub fn hover(
        &mut self,
        slide: &str,
        objects: Vec<String>,
        slot: Slot,
        effect: AnimEffect,
        duration_ms: u32,
        easing: Easing,
        pass: u64,
    ) {
        let what = Played::Anim(
            slot,
            Animation {
                effect,
                duration_ms,
                easing,
                sequence: Default::default(),
            },
        );
        self.hover_played(slide, objects, None, what, pass);
    }

    /// Loop `effect` as a single unit on `group` while called every frame.
    #[allow(clippy::too_many_arguments)]
    pub fn hover_group(
        &mut self,
        slide: &str,
        objects: Vec<String>,
        group: String,
        slot: Slot,
        effect: AnimEffect,
        duration_ms: u32,
        easing: Easing,
        pass: u64,
    ) {
        let what = Played::Anim(
            slot,
            Animation {
                effect,
                duration_ms,
                easing,
                sequence: Default::default(),
            },
        );
        self.hover_played(slide, objects, Some(group), what, pass);
    }

    /// Note `id` as just picked.
    pub fn picked(&mut self, id: &str) {
        self.recent.retain(|r| r != id);
        self.recent.insert(0, id.to_owned());
        self.recent.truncate(RECENT_MAX);
    }

    /// Stop any preview.
    pub fn stop(&mut self) {
        self.current = None;
    }

    /// Whether a preview is playing.
    #[must_use]
    pub fn is_playing(&self) -> bool {
        self.current.is_some()
    }

    /// The canvas items with the preview applied at input time `now` (seconds), or `None`
    /// when nothing plays. Drops a preview that is finished, no longer hovered (`pass` is
    /// this frame's pass number), or no longer matches the selection.
    pub fn items(
        &mut self,
        course: &Course,
        selection: (&str, &[String]),
        items: &[DrawItem],
        now: f64,
        pass: u64,
    ) -> Option<Vec<DrawItem>> {
        let p = self.current.as_mut()?;
        let alive = match p.mode {
            Mode::Hover { pass: last } => last.saturating_add(1) >= pass,
            Mode::Once => true,
        };
        if !alive || p.slide != selection.0 || p.objects != selection.1 {
            self.current = None;
            return None;
        }
        let slide = course.slide(&p.slide)?;
        let started = *p.started.get_or_insert(now);
        let elapsed_ms = ((now - started) * 1000.0).max(0.0);
        let duration = p.what.duration_ms();
        let cycle = LEAD_MS + duration + HOLD_MS;
        let t = match p.mode {
            Mode::Once if elapsed_ms > cycle => {
                self.current = None;
                return None;
            }
            Mode::Once => elapsed_ms,
            Mode::Hover { .. } => elapsed_ms % cycle,
        };
        let into = (t - LEAD_MS).clamp(0.0, duration);
        if let Some(ref gid) = p.group {
            let Some(owner) = slide.group(gid).map(|(_, o)| o).or_else(|| {
                p.objects
                    .first()
                    .and_then(|id| slide.object(id))
                    .map(|(_, o)| o)
            }) else {
                self.current = None;
                return None;
            };
            let Some(group_frame) = slide.group_frame(gid, owner) else {
                self.current = None;
                return None;
            };
            let sample = p.what.sample(into, group_frame, course.slide_size, (0, 1));
            let ga = (sample != ObjectSample::IDENTITY).then(|| {
                std::sync::Arc::new(coursecraft_render::GroupAnim {
                    id: gid.clone(),
                    frame: egui::Rect::from_min_size(
                        egui::pos2(group_frame.x, group_frame.y),
                        egui::vec2(group_frame.w, group_frame.h),
                    ),
                    sample: sample.clone(),
                })
            });
            let out = items
                .iter()
                .filter_map(|item| {
                    let Hit::Object(id) = &item.hit else {
                        return Some(item.clone());
                    };
                    if !p.objects.iter().any(|o| o == id) {
                        return Some(item.clone());
                    }
                    if sample.opacity <= 0.0 {
                        return None;
                    }
                    let mut item = item.clone();
                    item.group_anim = ga.clone();
                    Some(item)
                })
                .collect();
            return Some(out);
        }
        let out = items
            .iter()
            .flat_map(|item| {
                let Hit::Object(id) = &item.hit else {
                    return vec![item.clone()];
                };
                if !p.objects.iter().any(|o| o == id) {
                    return vec![item.clone()];
                }
                let Some((obj, _)) = slide.object(id) else {
                    return vec![item.clone()];
                };
                if !item.is_state_extra
                    && let Some(seq) = p.what.sequence()
                    && let Some(mut parts) = coursecraft_render::sequence::split_parts(item, seq)
                {
                    let n = parts.len();
                    for (k, part) in parts.iter_mut().enumerate() {
                        let s = p.what.sample(into, obj.frame, course.slide_size, (k, n));
                        coursecraft_render::apply_sample_to_items(std::slice::from_mut(part), &s);
                    }
                    parts.retain(|part| part.opacity > 0.0);
                    return parts;
                }
                let sample = p.what.sample(into, obj.frame, course.slide_size, (0, 1));
                if sample.opacity <= 0.0 {
                    return Vec::new();
                }
                let mut item = item.clone();
                coursecraft_render::apply_sample_to_items(std::slice::from_mut(&mut item), &sample);
                vec![item]
            })
            .collect();
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn course() -> Course {
        let json = serde_json::json!({
            "schema_version": 1, "id": "p", "title": "P",
            "slide_size": { "w": 1000, "h": 600 },
            "scenes": [{ "id": "sc", "title": "S", "slides": [{
                "id": "sl", "title": "One", "duration_ms": 5000, "objects": [
                    { "id": "a", "kind": { "type": "shape", "shape": "rectangle", "fill": "#112233" },
                      "frame": { "x": 100, "y": 100, "w": 200, "h": 120 } },
                    { "id": "b", "kind": { "type": "shape", "shape": "rectangle", "fill": "#445566" },
                      "frame": { "x": 500, "y": 100, "w": 200, "h": 120 } }
                ] }] }]
        });
        Course::from_json(&json.to_string()).expect("course")
    }

    fn items(c: &Course) -> Vec<DrawItem> {
        let slide = c.slide("sl").expect("slide");
        coursecraft_render::compose_authoring(c, slide, None)
    }

    fn fade() -> Animation {
        Animation {
            effect: AnimEffect::new("fade"),
            duration_ms: 1000,
            easing: Easing::Linear,
            sequence: Default::default(),
        }
    }

    fn rect_of<'a>(items: &'a [DrawItem], id: &str) -> Option<&'a DrawItem> {
        items.iter().find(|i| i.hit == Hit::Object(id.into()))
    }

    #[test]
    fn a_once_preview_plays_the_selected_object_then_stops() {
        let c = course();
        let base = items(&c);
        let sel = vec!["a".to_owned()];
        let mut s = PreviewState::default();
        s.play("sl", sel.clone(), Slot::Entrance, fade());
        // First frame: starts hidden, the other object untouched.
        let first = s.items(&c, ("sl", &sel), &base, 10.0, 1).expect("playing");
        assert!(rect_of(&first, "a").is_none(), "hidden before it enters");
        assert_eq!(rect_of(&first, "b"), rect_of(&base, "b"));
        // Mid-way: drawn.
        let mid = s.items(&c, ("sl", &sel), &base, 10.65, 2).expect("playing");
        assert!(rect_of(&mid, "a").is_some());
        // After the hold: finished.
        assert!(s.items(&c, ("sl", &sel), &base, 13.0, 3).is_none());
        assert!(!s.is_playing());
    }

    #[test]
    fn a_hover_preview_loops_until_the_pointer_leaves() {
        let c = course();
        let base = items(&c);
        let sel = vec!["a".to_owned()];
        let mut s = PreviewState::default();
        let f = fade();
        s.hover(
            "sl",
            sel.clone(),
            Slot::Exit,
            f.effect.clone(),
            1000,
            Easing::Linear,
            5,
        );
        assert!(s.items(&c, ("sl", &sel), &base, 1.0, 5).is_some());
        // Still hovered long after one pass: it loops.
        s.hover(
            "sl",
            sel.clone(),
            Slot::Exit,
            f.effect.clone(),
            1000,
            Easing::Linear,
            6,
        );
        assert!(s.items(&c, ("sl", &sel), &base, 9.0, 6).is_some());
        // Not hovered for a frame: it stops.
        assert!(s.items(&c, ("sl", &sel), &base, 9.1, 8).is_none());
    }

    #[test]
    fn previews_stop_on_a_new_selection_and_respect_auto_preview() {
        let c = course();
        let base = items(&c);
        let sel = vec!["a".to_owned()];
        let mut s = PreviewState::default();
        s.play("sl", sel.clone(), Slot::Entrance, fade());
        let other = vec!["b".to_owned()];
        assert!(s.items(&c, ("sl", &other), &base, 0.0, 1).is_none());
        assert!(!s.is_playing());
        s.auto = false;
        s.hover(
            "sl",
            sel,
            Slot::Entrance,
            AnimEffect::new("fade"),
            500,
            Easing::Linear,
            1,
        );
        assert!(!s.is_playing(), "hover previews follow Auto-preview");
    }

    #[test]
    fn recent_effects_are_newest_first_and_bounded() {
        let mut s = PreviewState::default();
        for id in ["fade", "wipe", "fade", "a", "b", "c", "d", "e"] {
            s.picked(id);
        }
        assert_eq!(s.recent.len(), RECENT_MAX);
        assert_eq!(s.recent[0], "e");
        assert_eq!(s.recent.iter().filter(|r| *r == "fade").count(), 1);
    }

    #[test]
    fn an_emphasis_preview_moves_the_object_and_ends_at_rest() {
        let c = course();
        let base = items(&c);
        let sel = vec!["a".to_owned()];
        let mut s = PreviewState::default();
        s.play_emphasis("sl", sel.clone(), Emphasis::new("grow_shrink", 2000));
        let _ = s.items(&c, ("sl", &sel), &base, 0.0, 1).expect("playing");
        let mid = s.items(&c, ("sl", &sel), &base, 0.65, 2).expect("playing");
        assert_ne!(rect_of(&mid, "a"), rect_of(&base, "a"), "grown mid-way");
        let held = s.items(&c, ("sl", &sel), &base, 1.3, 3).expect("holding");
        assert_eq!(rect_of(&held, "a"), rect_of(&base, "a"), "back at rest");
    }
}
