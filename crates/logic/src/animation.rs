//! Animation and transition sampling math (L2).
//!
//! Pure functions for easing, entrance/exit animation sampling, motion path polyline
//! sampling, and slide transition sampling. Used by the player runtime, editor preview,
//! and headless renderers.

use coursecraft_course::{
    Animation, Easing, MotionPath, Point, Rect, Size, Transition, TransitionEffect,
};

/// Evaluate an easing curve on normalized time `t` in `[0, 1]`.
///
/// Clamps input to `[0.0, 1.0]`. Returns `0.0` for non-finite values.
#[must_use]
pub fn ease(easing: Easing, t: f32) -> f32 {
    if !t.is_finite() {
        return 0.0;
    }
    let t = t.clamp(0.0, 1.0);
    match easing {
        Easing::Linear => t,
        Easing::EaseIn => t * t * t,
        Easing::EaseOut => {
            let inv = 1.0 - t;
            1.0 - inv * inv * inv
        }
        Easing::EaseInOut => {
            if t < 0.5 {
                4.0 * t * t * t
            } else {
                let inv = 1.0 - t;
                1.0 - 4.0 * inv * inv * inv
            }
        }
        // Back-out: about 10 % past the end, then back.
        Easing::Overshoot => {
            const C1: f32 = 1.701_58;
            let u = t - 1.0;
            1.0 + (C1 + 1.0) * u * u * u + C1 * u * u
        }
        Easing::Bounce => bounce_out(t),
        Easing::Elastic => {
            if t <= 0.0 || t >= 1.0 {
                t
            } else {
                let c4 = std::f32::consts::TAU / 3.0;
                2f32.powf(-10.0 * t) * ((t * 10.0 - 0.75) * c4).sin() + 1.0
            }
        }
    }
}

/// Standard bounce-out: lands at 1 and bounces three times, smaller each time.
fn bounce_out(t: f32) -> f32 {
    const N: f32 = 7.5625;
    const D: f32 = 2.75;
    if t < 1.0 / D {
        N * t * t
    } else if t < 2.0 / D {
        let t = t - 1.5 / D;
        N * t * t + 0.75
    } else if t < 2.5 / D {
        let t = t - 2.25 / D;
        N * t * t + 0.9375
    } else {
        let t = t - 2.625 / D;
        N * t * t + 0.984_375
    }
}

/// Clip rect as fractions `[0, 1]` of the object's frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClipFrac {
    pub min_x: f32,
    pub min_y: f32,
    pub max_x: f32,
    pub max_y: f32,
}

/// Colour wash over an object: every painted colour moves toward `rgb` by `amount`
/// (0 none, 1 fully that colour; textures are multiplied by it). Alpha is kept.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tint {
    pub rgb: [u8; 3],
    pub amount: f32,
}

/// A convex polygon in the object's frame, as fractions: `(0, 0)` top-left, `(1, 1)`
/// bottom-right. Points may lie outside `[0, 1]` (a circle that covers the corners). Either
/// winding works.
#[derive(Debug, Clone, PartialEq)]
pub struct ConvexPoly(pub Vec<(f32, f32)>);

/// Most polygons a [`Mask`] keeps; a sample function that makes more gets no mask (the
/// object shows whole), so a hostile or runaway effect cannot cost unbounded clipping work.
pub const MAX_MASK_POLYS: usize = 64;
/// Most points one [`ConvexPoly`] keeps; longer polygons are dropped.
pub const MAX_POLY_POINTS: usize = 64;

/// The visible part of an object: the union of its polygons, which must not overlap (an
/// overlap paints twice). An empty mask shows nothing. Moves, scales and rotates with the
/// object; it is never mirrored by a flip.
#[derive(Debug, Clone, PartialEq)]
pub struct Mask(pub std::sync::Arc<[ConvexPoly]>);

impl Mask {
    /// A mask of `polys`.
    #[must_use]
    pub fn new(polys: Vec<ConvexPoly>) -> Self {
        Self(polys.into())
    }

    /// The polygons.
    #[must_use]
    pub fn polys(&self) -> &[ConvexPoly] {
        &self.0
    }

    /// Drop polygons with non-finite points, fewer than 3 points, too many points or no
    /// area, and clamp points to `[-1, 2]`. `None` when there are more than
    /// [`MAX_MASK_POLYS`] polygons.
    #[must_use]
    pub fn sanitized(&self) -> Option<Self> {
        if self.0.len() > MAX_MASK_POLYS {
            return None;
        }
        let polys: Vec<ConvexPoly> = self
            .0
            .iter()
            .filter(|p| {
                (3..=MAX_POLY_POINTS).contains(&p.0.len())
                    && p.0.iter().all(|(x, y)| x.is_finite() && y.is_finite())
            })
            .map(|p| {
                ConvexPoly(
                    p.0.iter()
                        .map(|&(x, y)| (x.clamp(-1.0, 2.0), y.clamp(-1.0, 2.0)))
                        .collect(),
                )
            })
            .filter(|p| p.area().abs() > 1e-9)
            .collect();
        Some(Self::new(polys))
    }
}

impl ConvexPoly {
    /// Signed area (positive for clockwise on screen, where y grows down).
    #[must_use]
    pub fn area(&self) -> f32 {
        let n = self.0.len();
        let mut twice = 0.0;
        for i in 0..n {
            let (x0, y0) = self.0[i];
            let (x1, y1) = self.0[(i + 1) % n];
            twice += x0 * y1 - x1 * y0;
        }
        twice * 0.5
    }

    /// The axis-aligned rectangle `[x0, x1] × [y0, y1]`.
    #[must_use]
    pub fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> Self {
        Self(vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)])
    }
}

/// Sampled look of an object at a point in time, on top of its resting frame.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectSample {
    /// Translation in slide pixels.
    pub offset: (f32, f32),
    /// Horizontal and vertical scale about the frame centre, in the object's own axes.
    pub scale: (f32, f32),
    /// Extra clockwise rotation in degrees about the frame centre.
    pub rotation_deg: f32,
    pub opacity: f32,
    /// Rectangular reveal (fast path; see also `mask`).
    pub clip: Option<ClipFrac>,
    pub tint: Option<Tint>,
    /// Shaped reveal: only the inside of these polygons shows.
    pub mask: Option<Mask>,
}

impl ObjectSample {
    pub const IDENTITY: Self = Self {
        offset: (0.0, 0.0),
        scale: (1.0, 1.0),
        rotation_deg: 0.0,
        opacity: 1.0,
        clip: None,
        tint: None,
        mask: None,
    };

    /// The same scale on both axes.
    #[must_use]
    pub const fn uniform(s: f32) -> (f32, f32) {
        (s, s)
    }

    /// `self` with `over` layered on top (an emphasis over an entrance or exit): offsets add,
    /// scales and opacities multiply, rotations add, `over`'s tint wins, and the reveal (clip,
    /// mask) stays `self`'s.
    #[must_use]
    pub fn then(self, over: &Self) -> Self {
        Self {
            offset: (self.offset.0 + over.offset.0, self.offset.1 + over.offset.1),
            scale: (self.scale.0 * over.scale.0, self.scale.1 * over.scale.1),
            rotation_deg: self.rotation_deg + over.rotation_deg,
            opacity: self.opacity * over.opacity,
            tint: over.tint.or(self.tint),
            ..self
        }
    }

    /// Repair non-finite numbers (to identity), clamp opacity and tint amount to `[0, 1]`,
    /// scale to `[0, 100]` and rotation to ±3600°, and sanitize the mask, so a sample function
    /// can never hand the renderer hostile values.
    #[must_use]
    pub fn sanitized(self) -> Self {
        let fin = |v: f32, d: f32| if v.is_finite() { v } else { d };
        let scale = |v: f32| fin(v, 1.0).clamp(0.0, 100.0);
        Self {
            offset: (fin(self.offset.0, 0.0), fin(self.offset.1, 0.0)),
            scale: (scale(self.scale.0), scale(self.scale.1)),
            rotation_deg: fin(self.rotation_deg, 0.0).clamp(-3600.0, 3600.0),
            opacity: fin(self.opacity, 1.0).clamp(0.0, 1.0),
            clip: self.clip.map(|c| ClipFrac {
                min_x: fin(c.min_x, 0.0).clamp(0.0, 1.0),
                min_y: fin(c.min_y, 0.0).clamp(0.0, 1.0),
                max_x: fin(c.max_x, 1.0).clamp(0.0, 1.0),
                max_y: fin(c.max_y, 1.0).clamp(0.0, 1.0),
            }),
            tint: self
                .tint
                .map(|t| Tint {
                    rgb: t.rgb,
                    amount: fin(t.amount, 0.0).clamp(0.0, 1.0),
                })
                .filter(|t| t.amount > 0.0),
            mask: self.mask.as_ref().and_then(Mask::sanitized),
        }
    }
}

/// Whether the animation is entering or exiting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationPhase {
    Entering,
    Exiting,
}

/// Sample an entrance or exit animation at progress `progress` in `[0, 1]`.
///
/// For `Entering`, progress 0 is off-screen/hidden, 1 is fully entered.
/// For `Exiting`, progress 0 is resting/visible, 1 is fully exited.
/// The effect is looked up in the registry ([`crate::effects`]); an unknown id plays as
/// [`coursecraft_course::FALLBACK_EFFECT`].
#[must_use]
pub fn sample_animation(
    anim: &Animation,
    phase: AnimationPhase,
    progress: f32,
    frame: Rect,
    slide_size: Size,
) -> ObjectSample {
    let ctx = crate::effects::EffectCtx {
        phase,
        eased: ease(anim.easing, progress),
        frame,
        slide_size,
    };
    crate::effects::sample_effect(&anim.effect, &ctx)
}

/// Sample an active motion path at `elapsed_ms`, returning the offset relative to authored frame.
#[must_use]
pub fn sample_motion_path(
    path: &MotionPath,
    elapsed_ms: u32,
    from: (f32, f32),
    frame: Rect,
) -> (f32, f32) {
    if path.points.is_empty() {
        return from;
    }
    if let [pt] = path.points.as_slice() {
        let pt = *pt;
        return if path.relative {
            (from.0 + pt.x, from.1 + pt.y)
        } else {
            (pt.x - frame.x, pt.y - frame.y)
        };
    }

    let mut seg_lens = Vec::with_capacity(path.points.len().saturating_sub(1));
    let mut total_len = 0.0;
    for w in path.points.windows(2) {
        let dx = w[1].x - w[0].x;
        let dy = w[1].y - w[0].y;
        let len = (dx * dx + dy * dy).sqrt();
        seg_lens.push(len);
        total_len += len;
    }

    if total_len == 0.0 || !total_len.is_finite() {
        let end = path.points.last().copied().unwrap_or_default();
        return if path.relative {
            (from.0 + end.x, from.1 + end.y)
        } else {
            (end.x - frame.x, end.y - frame.y)
        };
    }

    let t = if path.duration_ms == 0 {
        1.0
    } else {
        (elapsed_ms as f32 / path.duration_ms as f32).clamp(0.0, 1.0)
    };
    let eased_t = ease(path.easing, t);
    // Overshoot and elastic easings run past the ends: the point continues along the first or
    // last segment's direction (bounded, so a hostile easing cannot fling it away).
    let target_dist = (eased_t * total_len).clamp(-total_len, 2.0 * total_len);

    let pt = point_at_distance(&path.points, &seg_lens, target_dist, total_len);

    if path.relative {
        (from.0 + pt.x, from.1 + pt.y)
    } else {
        (pt.x - frame.x, pt.y - frame.y)
    }
}

/// The point `dist` along a polyline whose segments have lengths `seg_lens` (sum `total_len`,
/// which is positive and finite). Before the start or past the end it continues along the first
/// or last segment that has a length.
fn point_at_distance(points: &[Point], seg_lens: &[f32], dist: f32, total_len: f32) -> Point {
    let at = |i: usize, frac: f32| {
        let p0 = points.get(i).copied().unwrap_or_default();
        let p1 = points.get(i + 1).copied().unwrap_or_default();
        Point {
            x: p0.x + frac * (p1.x - p0.x),
            y: p0.y + frac * (p1.y - p0.y),
        }
    };
    if dist < 0.0 {
        if let Some((i, &len)) = seg_lens.iter().enumerate().find(|(_, l)| **l > 0.0) {
            return at(i, dist / len);
        }
    } else if dist > total_len
        && let Some((i, &len)) = seg_lens.iter().enumerate().rfind(|(_, l)| **l > 0.0)
    {
        return at(i, 1.0 + (dist - total_len) / len);
    }
    let mut accum = 0.0;
    let last = seg_lens.len().saturating_sub(1);
    for (i, &seg_len) in seg_lens.iter().enumerate() {
        if dist <= accum + seg_len || i == last {
            let frac = if seg_len > 0.0 {
                ((dist - accum) / seg_len).clamp(0.0, 1.0)
            } else {
                0.0
            };
            return at(i, frac);
        }
        accum += seg_len;
    }
    points.last().copied().unwrap_or_default()
}

/// Result of sampling a slide transition between previous and next slide.
/// How one slide looks during a transition (M46 phase 8).
#[derive(Debug, Clone, PartialEq)]
pub struct SlideLook {
    /// Translation in slide pixels.
    pub offset: (f32, f32),
    pub alpha: f32,
    /// Uniform scale about the slide centre.
    pub scale: f32,
    /// Rectangular reveal, in fractions of the slide.
    pub clip: Option<ClipFrac>,
    /// Shaped reveal, in fractions of the slide.
    pub mask: Option<Mask>,
}

impl SlideLook {
    pub const REST: Self = Self {
        offset: (0.0, 0.0),
        alpha: 1.0,
        scale: 1.0,
        clip: None,
        mask: None,
    };

    /// Only moved (Push and Cover draw without the mesh pass).
    #[must_use]
    pub fn only_moves(&self) -> bool {
        self.alpha >= 1.0 && self.scale == 1.0 && self.clip.is_none() && self.mask.is_none()
    }

    fn moved(offset: (f32, f32)) -> Self {
        Self {
            offset,
            ..Self::REST
        }
    }

    fn faded(alpha: f32) -> Self {
        Self {
            alpha: alpha.clamp(0.0, 1.0),
            ..Self::REST
        }
    }
}

/// Both slides during a transition.
#[derive(Debug, Clone, PartialEq)]
pub struct TransitionSample {
    pub prev: SlideLook,
    pub next: SlideLook,
    /// Draw the old slide above the new one (Uncover, Reveal).
    pub prev_on_top: bool,
    /// A colour drawn under both slides (Fade through black).
    pub backdrop: Option<[u8; 3]>,
}

/// `transition` as it plays when the learner prefers reduced motion: a plain [`Fade`]
/// of the same duration, whatever the authored effect.
///
/// [`Fade`]: TransitionEffect::Fade
#[must_use]
pub fn reduced_motion_transition(transition: &Transition) -> Transition {
    Transition {
        effect: TransitionEffect::Fade,
        duration_ms: transition.duration_ms,
    }
}

/// Sample a slide transition at progress `progress` in `[0, 1]`.
#[must_use]
pub fn sample_transition(
    transition: &Transition,
    progress: f32,
    slide_size: Size,
) -> TransitionSample {
    let p = if progress.is_finite() {
        progress.clamp(0.0, 1.0)
    } else {
        1.0
    };
    let w = slide_size.w as f32;
    let h = slide_size.h as f32;
    let plain = |prev: SlideLook, next: SlideLook| TransitionSample {
        prev,
        next,
        prev_on_top: false,
        backdrop: None,
    };
    let away = |from: coursecraft_course::Direction, amount: f32| {
        let (vx, vy) = from.vector();
        (-vx * amount * w, -vy * amount * h)
    };
    match transition.effect {
        // The old slide stays opaque and the new one fades in over it, so no dark backdrop
        // shows through half way.
        TransitionEffect::Fade => plain(SlideLook::REST, SlideLook::faded(p)),
        TransitionEffect::FadeBlack => TransitionSample {
            backdrop: Some([0, 0, 0]),
            ..plain(
                SlideLook::faded(1.0 - 2.0 * p),
                SlideLook::faded(2.0 * p - 1.0),
            )
        },
        // The new slide comes in from `from` (a side or corner); the old one leaves the other
        // way.
        TransitionEffect::Push { from } => {
            let (vx, vy) = from.vector();
            plain(
                SlideLook::moved(away(from, p)),
                SlideLook::moved((vx * (1.0 - p) * w, vy * (1.0 - p) * h)),
            )
        }
        TransitionEffect::Cover { from } => {
            let (vx, vy) = from.vector();
            plain(
                SlideLook::REST,
                SlideLook::moved((vx * (1.0 - p) * w, vy * (1.0 - p) * h)),
            )
        }
        TransitionEffect::Uncover { from } => TransitionSample {
            prev_on_top: true,
            ..plain(SlideLook::moved(away(from, p)), SlideLook::REST)
        },
        TransitionEffect::Reveal { from } => TransitionSample {
            prev_on_top: true,
            ..plain(
                SlideLook {
                    alpha: 1.0 - p,
                    ..SlideLook::moved(away(from, 0.3 * p))
                },
                SlideLook::faded(p),
            )
        },
        TransitionEffect::Zoom => plain(
            SlideLook::REST,
            SlideLook {
                alpha: p,
                scale: 0.3 + 0.7 * p,
                ..SlideLook::REST
            },
        ),
        // The rest reveal the new slide through an entrance effect's mask, on the whole slide.
        _ => {
            let Some(fx) = transition.effect.as_effect().or_else(|| {
                matches!(transition.effect, TransitionEffect::Dissolve)
                    .then(|| coursecraft_course::AnimEffect::new("dissolve"))
            }) else {
                return plain(SlideLook::REST, SlideLook::faded(p));
            };
            let s = crate::effects::sample_effect(
                &fx,
                &crate::effects::EffectCtx {
                    phase: AnimationPhase::Entering,
                    eased: p,
                    frame: coursecraft_course::Rect {
                        x: 0.0,
                        y: 0.0,
                        w,
                        h,
                    },
                    slide_size,
                },
            );
            plain(
                SlideLook::REST,
                SlideLook {
                    offset: s.offset,
                    alpha: s.opacity,
                    scale: s.scale.0,
                    clip: s.clip,
                    mask: s.mask,
                },
            )
        }
    }
}
