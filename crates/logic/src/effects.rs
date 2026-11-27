//! Sample functions for the animation effect registry (L2).
//!
//! [`coursecraft_course::EFFECTS`] lists the effects and their options; this module holds one
//! sample function per registry id in [`IMPLS`]. A test checks the two tables match, so an
//! effect can't be listed without a runtime or the other way round.
//!
//! A sample function gets the eased progress and the phase. It returns how the object looks
//! at that moment as an [`ObjectSample`]. Exits reuse the entrance function: `Exiting` runs the
//! entrance backwards, so each effect is written once.

use coursecraft_course::{
    AnimEffect, Direction, EmphasisEffect, FALLBACK_EFFECT, FALLBACK_EMPHASIS, InOut, Orientation,
    Rect, RevealShape, Size,
};

use crate::animation::{AnimationPhase, ClipFrac, ConvexPoly, Mask, ObjectSample, Tint};

/// What a sample function gets besides the effect's options.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EffectCtx {
    pub phase: AnimationPhase,
    /// Eased progress in `[0, 1]`: 0 is the start of the animation, 1 its end.
    pub eased: f32,
    /// The object's frame on the slide.
    pub frame: Rect,
    pub slide_size: Size,
}

impl EffectCtx {
    /// How far the object is "in": 0 hidden/away, 1 at rest. Entering runs 0→1, exiting 1→0.
    #[must_use]
    pub fn shown(&self) -> f32 {
        let s = match self.phase {
            AnimationPhase::Entering => self.eased,
            AnimationPhase::Exiting => 1.0 - self.eased,
        };
        if s.is_finite() {
            s.clamp(0.0, 1.0)
        } else {
            1.0
        }
    }

    /// [`Self::shown`] without the clamp to `[0, 1]` (kept within `[-0.5, 1.5]`), so the
    /// Overshoot and Elastic easings carry moving and growing effects past their resting place.
    #[must_use]
    pub fn travel(&self) -> f32 {
        let s = match self.phase {
            AnimationPhase::Entering => self.eased,
            AnimationPhase::Exiting => 1.0 - self.eased,
        };
        if s.is_finite() {
            s.clamp(-0.5, 1.5)
        } else {
            1.0
        }
    }
}

/// One effect's sample function.
pub type SampleFn = fn(&AnimEffect, &EffectCtx) -> ObjectSample;

/// One registry id and its sample function.
pub struct EffectImpl {
    pub id: &'static str,
    pub sample: SampleFn,
}

macro_rules! impls {
    ($($id:literal => $f:ident),* $(,)?) => {
        /// Every effect the runtime can play. Must list exactly the ids of
        /// [`coursecraft_course::EFFECTS`].
        pub static IMPLS: &[EffectImpl] = &[$(EffectImpl { id: $id, sample: $f }),*];
    };
}

impls! {
    "appear" => appear,
    "fade" => fade,
    "grow" => grow,
    "fly_in" => fly_in,
    "float" => float,
    "split" => split,
    "wipe" => wipe,
    "shape" => shape,
    "wheel" => wheel,
    "random_bars" => random_bars,
    "zoom" => zoom,
    "dissolve" => dissolve,
    "blinds" => blinds,
    "peek" => peek,
    "expand" => expand,
    "rise" => rise,
    "spin" => spin,
    "spin_grow" => spin_grow,
    "grow_turn" => grow_turn,
    "checkerboard" => checkerboard,
    "strips" => strips,
    "flip" => flip,
    "swivel" => swivel,
    "bounce" => bounce,
    "pop" => pop,
}

fn sampler(id: &str) -> Option<SampleFn> {
    IMPLS.iter().find(|i| i.id == id).map(|i| i.sample)
}

/// Sample `effect`; an unknown id plays as [`FALLBACK_EFFECT`]. Every effect rests exactly at
/// [`ObjectSample::IDENTITY`] once fully shown.
#[must_use]
pub fn sample_effect(effect: &AnimEffect, ctx: &EffectCtx) -> ObjectSample {
    // At rest, unless an overshooting easing is carrying the object past it.
    if ctx.shown() >= 1.0 && (ctx.travel() - 1.0).abs() < 1e-4 {
        return ObjectSample::IDENTITY;
    }
    let sample = sampler(&effect.id)
        .or_else(|| sampler(FALLBACK_EFFECT))
        .unwrap_or(fade);
    sample(effect, ctx).sanitized()
}

/// The direction the effect plays from: the stored one if the effect allows it, else its
/// default, else bottom.
fn from(effect: &AnimEffect) -> Direction {
    effect.direction().unwrap_or(Direction::Bottom)
}

fn orientation(effect: &AnimEffect) -> Orientation {
    effect.orientation().unwrap_or_default()
}

/// `1 - k·(1 - s)`: `k` short of 1 at the start, exactly 1 at the end.
fn toward_one(s: f32, k: f32) -> f32 {
    1.0 - k * (1.0 - s)
}

/// Ease-out with overshoot: 0 at 0, a little past 1 on the way, 1 at 1.
fn back_out(s: f32) -> f32 {
    const C1: f32 = 1.701_58;
    const C3: f32 = C1 + 1.0;
    let t = s - 1.0;
    1.0 + C3 * t * t * t + C1 * t * t
}

/// A ball dropped on the floor: 0 at 0, three bounces, 1 at 1.
fn bounce_out(s: f32) -> f32 {
    const N: f32 = 7.5625;
    const D: f32 = 2.75;
    if s < 1.0 / D {
        N * s * s
    } else if s < 2.0 / D {
        let t = s - 1.5 / D;
        N * t * t + 0.75
    } else if s < 2.5 / D {
        let t = s - 2.25 / D;
        N * t * t + 0.9375
    } else {
        let t = s - 2.625 / D;
        N * t * t + 0.984_375
    }
}

/// A shaped reveal: no mask once fully shown, an empty one (nothing visible) at the start.
fn reveal(s: f32, polys: impl FnOnce() -> Vec<ConvexPoly>) -> Option<Mask> {
    if s >= 1.0 {
        None
    } else if s <= 0.0 {
        Some(Mask::new(Vec::new()))
    } else {
        Some(Mask::new(
            polys().into_iter().filter(|p| p.0.len() >= 3).collect(),
        ))
    }
}

fn with_mask(mask: Option<Mask>) -> ObjectSample {
    ObjectSample {
        mask,
        ..ObjectSample::IDENTITY
    }
}

// ------------------------------------------------------------------ mask geometry

type Pt = (f32, f32);

/// The object's frame in its own fractions.
fn frame_poly() -> Vec<Pt> {
    ConvexPoly::rect(0.0, 0.0, 1.0, 1.0).0
}

/// Keep the part of convex `poly` where `a·x + b·y <= k`.
fn clip_half(poly: &[Pt], a: f32, b: f32, k: f32) -> Vec<Pt> {
    let f = |p: Pt| a * p.0 + b * p.1 - k;
    let mut out = Vec::with_capacity(poly.len() + 1);
    let Some(&last) = poly.last() else {
        return out;
    };
    let mut prev = last;
    let mut fp = f(prev);
    for &cur in poly {
        let fc = f(cur);
        if fc <= 0.0 {
            if fp > 0.0 {
                out.push(cross_at(prev, cur, fp, fc));
            }
            out.push(cur);
        } else if fp <= 0.0 {
            out.push(cross_at(prev, cur, fp, fc));
        }
        prev = cur;
        fp = fc;
    }
    out
}

fn cross_at(p: Pt, q: Pt, fp: f32, fq: f32) -> Pt {
    let t = fp / (fp - fq);
    (p.0 + (q.0 - p.0) * t, p.1 + (q.1 - p.1) * t)
}

/// Rectangle `[x0, x1] × [y0, y1]` cut to the frame; empty when it has no area.
fn frame_rect(x0: f32, y0: f32, x1: f32, y1: f32) -> Option<ConvexPoly> {
    let (x0, x1) = (x0.max(0.0), x1.min(1.0));
    let (y0, y1) = (y0.max(0.0), y1.min(1.0));
    (x1 > x0 && y1 > y0).then(|| ConvexPoly::rect(x0, y0, x1, y1))
}

/// `rect` in the given orientation: `along` runs across the bars (y for horizontal bars).
fn bar(o: Orientation, along0: f32, along1: f32, across0: f32, across1: f32) -> Option<ConvexPoly> {
    match o {
        Orientation::Horizontal => frame_rect(across0, along0, across1, along1),
        Orientation::Vertical => frame_rect(along0, across0, along1, across1),
    }
}

/// A shape outline at full size (it covers the whole frame), star-shaped about the centre,
/// with vertices in clockwise order.
fn outline(shape: RevealShape) -> Vec<Pt> {
    match shape {
        RevealShape::Circle => {
            const R: f32 = 0.708;
            (0..32)
                .map(|i| {
                    let a = i as f32 * std::f32::consts::TAU / 32.0;
                    (0.5 + R * a.sin(), 0.5 - R * a.cos())
                })
                .collect()
        }
        RevealShape::Box => frame_poly(),
        RevealShape::Diamond => vec![(0.5, -0.5), (1.5, 0.5), (0.5, 1.5), (-0.5, 0.5)],
        RevealShape::Plus => {
            let (a, b) = (1.0, 0.5);
            [
                (-b, -a),
                (b, -a),
                (b, -b),
                (a, -b),
                (a, b),
                (b, b),
                (b, a),
                (-b, a),
                (-b, b),
                (-a, b),
                (-a, -b),
                (-b, -b),
            ]
            .iter()
            .map(|&(x, y)| (0.5 + x, 0.5 + y))
            .collect()
        }
    }
}

const CENTRE: Pt = (0.5, 0.5);

fn scaled(p: Pt, t: f32) -> Pt {
    (
        CENTRE.0 + (p.0 - CENTRE.0) * t,
        CENTRE.1 + (p.1 - CENTRE.1) * t,
    )
}

/// The inside of `outline` scaled by `t`, as triangles from the centre, cut to the frame.
fn star_inside(outline: &[Pt], t: f32) -> Vec<ConvexPoly> {
    (0..outline.len())
        .map(|i| {
            let a = scaled(outline[i], t);
            let b = scaled(outline[(i + 1) % outline.len()], t);
            let mut poly = frame_poly();
            for (p, q) in [(CENTRE, a), (a, b), (b, CENTRE)] {
                // Clockwise on screen: the inside is on the right of each edge.
                poly = clip_half(
                    &poly,
                    q.1 - p.1,
                    p.0 - q.0,
                    (q.1 - p.1) * p.0 + (p.0 - q.0) * p.1,
                );
            }
            ConvexPoly(poly)
        })
        .collect()
}

/// The frame outside `outline` scaled by `t`: per edge, the wedge from the centre through its
/// ends, beyond the edge. The wedges don't overlap, and each is convex.
fn star_outside(outline: &[Pt], t: f32) -> Vec<ConvexPoly> {
    (0..outline.len())
        .map(|i| {
            let a = scaled(outline[i], t);
            let b = scaled(outline[(i + 1) % outline.len()], t);
            let da = (outline[i].0 - CENTRE.0, outline[i].1 - CENTRE.1);
            let n = outline.len();
            let db = (
                outline[(i + 1) % n].0 - CENTRE.0,
                outline[(i + 1) % n].1 - CENTRE.1,
            );
            let mut poly = frame_poly();
            // Right of the ray through `a` (clockwise side), left of the ray through `b`.
            poly = clip_half(&poly, da.1, -da.0, da.1 * CENTRE.0 - da.0 * CENTRE.1);
            poly = clip_half(&poly, -db.1, db.0, -db.1 * CENTRE.0 + db.0 * CENTRE.1);
            // Beyond the edge a→b (its left side, away from the centre).
            poly = clip_half(
                &poly,
                -(b.1 - a.1),
                -(a.0 - b.0),
                -((b.1 - a.1) * a.0 + (a.0 - b.0) * a.1),
            );
            ConvexPoly(poly)
        })
        .collect()
}

/// Deterministic shuffle rank of `i` among `0..n` (the same every run and on every platform).
fn ranks(n: u32) -> Vec<u32> {
    let mix = |mut x: u32| {
        x ^= x >> 16;
        x = x.wrapping_mul(0x7feb_352d);
        x ^= x >> 15;
        x = x.wrapping_mul(0x846c_a68b);
        x ^ (x >> 16)
    };
    let mut order: Vec<u32> = (0..n).collect();
    order.sort_by_key(|&i| (mix(i.wrapping_add(0x9e37_79b9)), i));
    let mut rank = vec![0; n as usize];
    for (r, &i) in order.iter().enumerate() {
        if let Some(slot) = rank.get_mut(i as usize) {
            *slot = r as u32;
        }
    }
    rank
}

/// How many of `n` pieces show at `s` (all of them only at the very end).
fn pieces_shown(s: f32, n: u32) -> u32 {
    ((s * n as f32).floor() as u32).min(n)
}

// ------------------------------------------------------------------ effects

fn appear(_: &AnimEffect, ctx: &EffectCtx) -> ObjectSample {
    ObjectSample {
        opacity: if ctx.shown() > 0.0 { 1.0 } else { 0.0 },
        ..ObjectSample::IDENTITY
    }
}

fn fade(_: &AnimEffect, ctx: &EffectCtx) -> ObjectSample {
    ObjectSample {
        opacity: ctx.shown(),
        ..ObjectSample::IDENTITY
    }
}

fn grow(_: &AnimEffect, ctx: &EffectCtx) -> ObjectSample {
    ObjectSample {
        scale: ObjectSample::uniform(ctx.travel().max(0.0)),
        ..ObjectSample::IDENTITY
    }
}

fn fly_in(effect: &AnimEffect, ctx: &EffectCtx) -> ObjectSample {
    let f = ctx.frame;
    let (vx, vy) = from(effect).vector();
    let off = |v: f32, pos: f32, size: f32, slide: f32| {
        if v < 0.0 {
            -(pos + size)
        } else if v > 0.0 {
            slide - pos
        } else {
            0.0
        }
    };
    let full = (
        off(vx, f.x, f.w, ctx.slide_size.w as f32),
        off(vy, f.y, f.h, ctx.slide_size.h as f32),
    );
    let away = 1.0 - ctx.travel();
    ObjectSample {
        offset: (full.0 * away, full.1 * away),
        ..ObjectSample::IDENTITY
    }
}

/// How far Float travels, in slide pixels.
const FLOAT_DISTANCE: f32 = 40.0;

fn float(effect: &AnimEffect, ctx: &EffectCtx) -> ObjectSample {
    let (vx, vy) = from(effect).vector();
    let s = ctx.shown();
    let away = 1.0 - ctx.travel();
    ObjectSample {
        offset: (vx * FLOAT_DISTANCE * away, vy * FLOAT_DISTANCE * away),
        opacity: s,
        ..ObjectSample::IDENTITY
    }
}

fn split(effect: &AnimEffect, ctx: &EffectCtx) -> ObjectSample {
    let s = ctx.shown();
    // A vertical split line opens sideways; a horizontal one opens up and down.
    let o = orientation(effect);
    let inout = effect.inout().unwrap_or_default();
    with_mask(reveal(s, || match inout {
        InOut::Out => bar(o, 0.5 - s / 2.0, 0.5 + s / 2.0, 0.0, 1.0)
            .into_iter()
            .collect(),
        InOut::In => [
            bar(o, 0.0, s / 2.0, 0.0, 1.0),
            bar(o, 1.0 - s / 2.0, 1.0, 0.0, 1.0),
        ]
        .into_iter()
        .flatten()
        .collect(),
    }))
}

fn wipe(effect: &AnimEffect, ctx: &EffectCtx) -> ObjectSample {
    let s = ctx.shown();
    let side = |min_x, min_y, max_x, max_y| ObjectSample {
        clip: Some(ClipFrac {
            min_x,
            min_y,
            max_x,
            max_y,
        }),
        ..ObjectSample::IDENTITY
    };
    match from(effect) {
        Direction::Left => side(0.0, 0.0, s, 1.0),
        Direction::Right => side(1.0 - s, 0.0, 1.0, 1.0),
        Direction::Top => side(0.0, 0.0, 1.0, s),
        Direction::Bottom => side(0.0, 1.0 - s, 1.0, 1.0),
        corner => {
            // The front runs across the frame from `corner` to the opposite one.
            let (vx, vy) = corner.vector();
            let m = (vx.abs() + vy.abs()) * 0.5;
            let k = (2.0 * s - 1.0) * m - 0.5 * (vx + vy);
            with_mask(reveal(s, || {
                vec![ConvexPoly(clip_half(&frame_poly(), -vx, -vy, k))]
            }))
        }
    }
}

fn shape(effect: &AnimEffect, ctx: &EffectCtx) -> ObjectSample {
    let s = ctx.shown();
    let outline = outline(effect.shape().unwrap_or_default());
    let inout = effect.inout().unwrap_or_default();
    with_mask(reveal(s, || match inout {
        InOut::Out => star_inside(&outline, s),
        InOut::In => star_outside(&outline, 1.0 - s),
    }))
}

fn wheel(effect: &AnimEffect, ctx: &EffectCtx) -> ObjectSample {
    let s = ctx.shown();
    let n = u32::from(effect.spokes().unwrap_or(1).max(1));
    with_mask(reveal(s, || {
        const R: f32 = 1.0;
        const CHUNK: f32 = std::f32::consts::FRAC_PI_4;
        let sector = std::f32::consts::TAU / n as f32;
        let sweep = sector * s;
        let at = |a: f32| (CENTRE.0 + R * a.sin(), CENTRE.1 - R * a.cos());
        let mut polys = Vec::new();
        for k in 0..n {
            let start = sector * k as f32;
            let mut done = 0.0;
            while done < sweep {
                let step = (sweep - done).min(CHUNK);
                let a0 = start + done;
                let mut pts = vec![CENTRE];
                for j in 0..=3 {
                    pts.push(at(a0 + step * j as f32 / 3.0));
                }
                let mut poly = frame_poly();
                for w in 0..pts.len() {
                    let p = pts[w];
                    let q = pts[(w + 1) % pts.len()];
                    poly = clip_half(
                        &poly,
                        q.1 - p.1,
                        p.0 - q.0,
                        (q.1 - p.1) * p.0 + (p.0 - q.0) * p.1,
                    );
                }
                polys.push(ConvexPoly(poly));
                done += step;
            }
        }
        polys
    }))
}

/// Runs of consecutive shown pieces, as `(first, last + 1)`.
fn runs(shown: impl Iterator<Item = bool>) -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    let mut start = None;
    let mut i = 0u32;
    for on in shown {
        match (on, start) {
            (true, None) => start = Some(i),
            (false, Some(a)) => {
                out.push((a, i));
                start = None;
            }
            _ => {}
        }
        i += 1;
    }
    if let Some(a) = start {
        out.push((a, i));
    }
    out
}

fn random_bars(effect: &AnimEffect, ctx: &EffectCtx) -> ObjectSample {
    const BARS: u32 = 40;
    let s = ctx.shown();
    let o = orientation(effect);
    with_mask(reveal(s, || {
        let rank = ranks(BARS);
        let k = pieces_shown(s, BARS);
        let step = 1.0 / BARS as f32;
        runs(rank.iter().map(|&r| r < k))
            .into_iter()
            .filter_map(|(a, b)| bar(o, a as f32 * step, b as f32 * step, 0.0, 1.0))
            .collect()
    }))
}

fn zoom(_: &AnimEffect, ctx: &EffectCtx) -> ObjectSample {
    let s = ctx.shown();
    ObjectSample {
        scale: ObjectSample::uniform(ctx.travel().max(0.0)),
        opacity: s,
        ..ObjectSample::IDENTITY
    }
}

fn dissolve(_: &AnimEffect, ctx: &EffectCtx) -> ObjectSample {
    const SIDE: u32 = 8;
    let s = ctx.shown();
    with_mask(reveal(s, || {
        let rank = ranks(SIDE * SIDE);
        let k = pieces_shown(s, SIDE * SIDE);
        let step = 1.0 / SIDE as f32;
        let mut polys = Vec::new();
        for row in 0..SIDE {
            let cells =
                (0..SIDE).map(|c| rank.get((row * SIDE + c) as usize).is_some_and(|&r| r < k));
            for (a, b) in runs(cells) {
                polys.extend(frame_rect(
                    a as f32 * step,
                    row as f32 * step,
                    b as f32 * step,
                    (row + 1) as f32 * step,
                ));
            }
        }
        polys
    }))
}

fn blinds(effect: &AnimEffect, ctx: &EffectCtx) -> ObjectSample {
    const SLATS: u32 = 6;
    let s = ctx.shown();
    let o = orientation(effect);
    with_mask(reveal(s, || {
        let step = 1.0 / SLATS as f32;
        (0..SLATS)
            .filter_map(|i| {
                let a = i as f32 * step;
                bar(o, a, a + s * step, 0.0, 1.0)
            })
            .collect()
    }))
}

fn peek(effect: &AnimEffect, ctx: &EffectCtx) -> ObjectSample {
    let s = ctx.shown();
    let f = ctx.frame;
    let away = 1.0 - s;
    let (offset, clip) = match from(effect) {
        Direction::Left => ((-f.w * away, 0.0), (1.0 - s, 0.0, 1.0, 1.0)),
        Direction::Right => ((f.w * away, 0.0), (0.0, 0.0, s, 1.0)),
        Direction::Top => ((0.0, -f.h * away), (0.0, 1.0 - s, 1.0, 1.0)),
        _ => ((0.0, f.h * away), (0.0, 0.0, 1.0, s)),
    };
    ObjectSample {
        offset,
        clip: Some(ClipFrac {
            min_x: clip.0,
            min_y: clip.1,
            max_x: clip.2,
            max_y: clip.3,
        }),
        ..ObjectSample::IDENTITY
    }
}

fn expand(_: &AnimEffect, ctx: &EffectCtx) -> ObjectSample {
    let s = ctx.shown();
    ObjectSample {
        scale: (toward_one(s, 0.7), 1.0),
        opacity: s,
        ..ObjectSample::IDENTITY
    }
}

/// How far Rise up travels, in slide pixels.
const RISE_DISTANCE: f32 = 80.0;

fn rise(_: &AnimEffect, ctx: &EffectCtx) -> ObjectSample {
    let s = ctx.shown();
    ObjectSample {
        offset: (0.0, RISE_DISTANCE * (1.0 - back_out(s))),
        opacity: s,
        ..ObjectSample::IDENTITY
    }
}

fn spin(_: &AnimEffect, ctx: &EffectCtx) -> ObjectSample {
    let s = ctx.shown();
    ObjectSample {
        rotation_deg: -360.0 * (1.0 - s),
        opacity: s,
        ..ObjectSample::IDENTITY
    }
}

fn spin_grow(_: &AnimEffect, ctx: &EffectCtx) -> ObjectSample {
    let s = ctx.shown();
    ObjectSample {
        rotation_deg: -360.0 * (1.0 - s),
        scale: ObjectSample::uniform(s),
        opacity: s,
        ..ObjectSample::IDENTITY
    }
}

fn grow_turn(_: &AnimEffect, ctx: &EffectCtx) -> ObjectSample {
    let s = ctx.shown();
    ObjectSample {
        rotation_deg: -90.0 * (1.0 - s),
        scale: ObjectSample::uniform(s),
        opacity: s,
        ..ObjectSample::IDENTITY
    }
}

fn checkerboard(effect: &AnimEffect, ctx: &EffectCtx) -> ObjectSample {
    const CELLS: u32 = 6;
    let s = ctx.shown();
    // Horizontal runs across (cells open left to right); vertical runs down.
    let o = match orientation(effect) {
        Orientation::Horizontal => Orientation::Vertical,
        Orientation::Vertical => Orientation::Horizontal,
    };
    with_mask(reveal(s, || {
        let w = 1.0 / CELLS as f32;
        let mut polys = Vec::new();
        for row in 0..CELLS {
            let shift = if row % 2 == 1 { w / 2.0 } else { 0.0 };
            let (r0, r1) = (row as f32 * w, (row + 1) as f32 * w);
            for c in 0..=CELLS {
                let x0 = c as f32 * w - shift;
                polys.extend(bar(o, x0, x0 + s * w, r0, r1));
            }
        }
        polys
    }))
}

fn strips(effect: &AnimEffect, ctx: &EffectCtx) -> ObjectSample {
    const SIDE: u32 = 8;
    let s = ctx.shown();
    let (vx, vy) = from(effect).vector();
    with_mask(reveal(s, || {
        let step = 1.0 / SIDE as f32;
        // Cells in diagonal bands from the `from` corner: band d shows once the front passes it.
        let bands = (2 * SIDE - 1) as f32;
        let front = s * bands;
        (0..SIDE)
            .filter_map(|row| {
                // Distance of this row from the corner's edge, in cells.
                let dy = if vy < 0.0 { row } else { SIDE - 1 - row };
                let count = (front - dy as f32).ceil().clamp(0.0, SIDE as f32) as u32;
                if count == 0 {
                    return None;
                }
                let y0 = row as f32 * step;
                let (x0, x1) = if vx < 0.0 {
                    (0.0, count as f32 * step)
                } else {
                    (1.0 - count as f32 * step, 1.0)
                };
                frame_rect(x0, y0, x1, y0 + step)
            })
            .collect()
    }))
}

fn flip(effect: &AnimEffect, ctx: &EffectCtx) -> ObjectSample {
    let s = (ctx.shown() * std::f32::consts::FRAC_PI_2).sin();
    ObjectSample {
        scale: match orientation(effect) {
            Orientation::Vertical => (s, 1.0),
            Orientation::Horizontal => (1.0, s),
        },
        ..ObjectSample::IDENTITY
    }
}

fn swivel(effect: &AnimEffect, ctx: &EffectCtx) -> ObjectSample {
    let s = ctx.shown();
    // One and a quarter turns about the axis, settling face-on.
    let w = (2.5 * std::f32::consts::PI * (1.0 - s)).cos().abs();
    ObjectSample {
        scale: match orientation(effect) {
            Orientation::Vertical => (w, 1.0),
            Orientation::Horizontal => (1.0, w),
        },
        opacity: s,
        ..ObjectSample::IDENTITY
    }
}

fn bounce(_: &AnimEffect, ctx: &EffectCtx) -> ObjectSample {
    let f = ctx.frame;
    let drop = f.y + f.h;
    ObjectSample {
        offset: (0.0, -drop * (1.0 - bounce_out(ctx.shown()))),
        ..ObjectSample::IDENTITY
    }
}

fn pop(_: &AnimEffect, ctx: &EffectCtx) -> ObjectSample {
    let s = ctx.shown();
    ObjectSample {
        scale: ObjectSample::uniform(back_out(s).max(0.0)),
        opacity: (2.0 * s).min(1.0),
        ..ObjectSample::IDENTITY
    }
}

// ---------------------------------------------------------------------------
// Emphasis (M46 phase 6)
// ---------------------------------------------------------------------------

/// One emphasis's sample function: `t` is the eased progress of one pass (in `[0, 1]`, or a
/// little outside it for overshoot and elastic easings), `frame` the object's frame. Every
/// function returns the resting look at 0 and 1.
pub type EmphasisFn = fn(f32, Rect) -> ObjectSample;

/// One emphasis id and its sample function.
pub struct EmphasisImpl {
    pub id: &'static str,
    pub sample: EmphasisFn,
}

macro_rules! emphasis_impls {
    ($($id:literal => $f:ident),* $(,)?) => {
        /// Every emphasis the runtime can play. Must list exactly the ids of
        /// [`coursecraft_course::EMPHASIS`].
        pub static EMPHASIS_IMPLS: &[EmphasisImpl] = &[$(EmphasisImpl { id: $id, sample: $f }),*];
    };
}

emphasis_impls! {
    "pulse" => e_pulse,
    "grow_shrink" => e_grow_shrink,
    "teeter" => e_teeter,
    "spin" => e_spin,
    "shake" => e_shake,
    "heartbeat" => e_heartbeat,
    "jump" => e_jump,
    "blink" => e_blink,
    "transparency" => e_transparency,
    "darken" => e_darken,
    "lighten" => e_lighten,
    "color_pulse" => e_color_pulse,
}

/// 0 at both ends, 1 in the middle. Beyond the ends (an overshoot or elastic easing) it
/// swings the other way, so the effect springs past its rest and back.
fn hump(t: f32) -> f32 {
    (t * std::f32::consts::PI).sin()
}

fn e_pulse(t: f32, _: Rect) -> ObjectSample {
    ObjectSample {
        scale: ObjectSample::uniform(1.0 + 0.08 * hump(t)),
        ..ObjectSample::IDENTITY
    }
}

fn e_grow_shrink(t: f32, _: Rect) -> ObjectSample {
    ObjectSample {
        scale: ObjectSample::uniform(1.0 + 0.25 * hump(t)),
        ..ObjectSample::IDENTITY
    }
}

fn e_teeter(t: f32, _: Rect) -> ObjectSample {
    ObjectSample {
        rotation_deg: 7.0 * (t * 4.0 * std::f32::consts::TAU / 2.0).sin() * (1.0 - t),
        ..ObjectSample::IDENTITY
    }
}

fn e_spin(t: f32, _: Rect) -> ObjectSample {
    ObjectSample {
        rotation_deg: 360.0 * t,
        ..ObjectSample::IDENTITY
    }
}

fn e_shake(t: f32, frame: Rect) -> ObjectSample {
    let amp = (frame.w * 0.06).clamp(4.0, 24.0);
    ObjectSample {
        offset: (
            amp * (t * 5.0 * std::f32::consts::TAU).sin() * (1.0 - t),
            0.0,
        ),
        ..ObjectSample::IDENTITY
    }
}

fn e_heartbeat(t: f32, _: Rect) -> ObjectSample {
    // Two beats: a strong one at 0.15 and a softer one at 0.45, then rest.
    let beat = |c: f32, w: f32| (1.0 - ((t - c) / w).powi(2)).max(0.0);
    ObjectSample {
        scale: ObjectSample::uniform(1.0 + 0.14 * beat(0.15, 0.12) + 0.09 * beat(0.45, 0.12)),
        ..ObjectSample::IDENTITY
    }
}

fn e_jump(t: f32, frame: Rect) -> ObjectSample {
    let h = (frame.h * 0.3).clamp(8.0, 80.0);
    // Up and down once, with a little squash on landing.
    let up = (1.0 - (2.0 * t - 1.0).powi(2)).max(0.0);
    let squash = if t > 0.85 {
        hump((t - 0.85) / 0.15) * 0.06
    } else {
        0.0
    };
    ObjectSample {
        offset: (0.0, -h * up),
        scale: (1.0 + squash, 1.0 - squash),
        ..ObjectSample::IDENTITY
    }
}

fn e_blink(t: f32, _: Rect) -> ObjectSample {
    // Off three times.
    let phase = (t * 3.0).fract();
    let off = (0.25..0.6).contains(&phase) && t < 1.0;
    ObjectSample {
        opacity: if off { 0.1 } else { 1.0 },
        ..ObjectSample::IDENTITY
    }
}

fn e_transparency(t: f32, _: Rect) -> ObjectSample {
    ObjectSample {
        opacity: 1.0 - 0.55 * hump(t),
        ..ObjectSample::IDENTITY
    }
}

fn wash(rgb: [u8; 3], amount: f32) -> ObjectSample {
    ObjectSample {
        tint: (amount > 0.0).then_some(Tint { rgb, amount }),
        ..ObjectSample::IDENTITY
    }
}

fn e_darken(t: f32, _: Rect) -> ObjectSample {
    wash([0, 0, 0], 0.45 * hump(t))
}

fn e_lighten(t: f32, _: Rect) -> ObjectSample {
    wash([255, 255, 255], 0.55 * hump(t))
}

fn e_color_pulse(t: f32, _: Rect) -> ObjectSample {
    wash([242, 177, 58], 0.7 * hump(t))
}

/// Whether emphasis `id` changes colour or opacity only (no movement, scale, turn or
/// flashing), so it may still play once when the learner prefers reduced motion. An unknown id
/// plays as [`coursecraft_course::FALLBACK_EMPHASIS`] (a pulse), so it does not.
#[must_use]
pub fn emphasis_is_motion_free(id: &str) -> bool {
    matches!(id, "transparency" | "darken" | "lighten" | "color_pulse")
}

fn emphasis_sampler(id: &str) -> Option<EmphasisFn> {
    EMPHASIS_IMPLS.iter().find(|i| i.id == id).map(|i| i.sample)
}

/// `effect` at eased progress `t` of one pass. The resting look at `t <= 0` and `t >= 1`; an
/// unknown id plays as [`coursecraft_course::FALLBACK_EMPHASIS`].
#[must_use]
pub fn sample_emphasis(effect: &EmphasisEffect, t: f32, frame: Rect) -> ObjectSample {
    sample_emphasis_eased(effect, t, t, frame)
}

/// [`sample_emphasis`] with the pass's raw (un-eased) progress `raw` and its eased progress
/// `eased`. The pass is at rest when `raw` is outside `(0, 1)`; while it runs, `eased` may lie
/// outside `[0, 1]` (overshoot and elastic easings) and the effect follows it.
#[must_use]
pub fn sample_emphasis_eased(
    effect: &EmphasisEffect,
    raw: f32,
    eased: f32,
    frame: Rect,
) -> ObjectSample {
    if !raw.is_finite() || raw <= 0.0 || raw >= 1.0 || !eased.is_finite() {
        return ObjectSample::IDENTITY;
    }
    let sample = emphasis_sampler(&effect.id)
        .or_else(|| emphasis_sampler(FALLBACK_EMPHASIS))
        .unwrap_or(e_pulse);
    sample(eased, frame).sanitized()
}

#[cfg(test)]
mod tests {
    use super::*;
    use coursecraft_course::EFFECTS;

    #[test]
    fn every_registry_effect_has_a_sampler_and_back() {
        for def in EFFECTS {
            assert!(sampler(def.id).is_some(), "no sampler for `{}`", def.id);
        }
        for imp in IMPLS {
            assert!(
                coursecraft_course::effect_def(imp.id).is_some(),
                "sampler `{}` is not in the registry",
                imp.id
            );
        }
        assert!(sampler(FALLBACK_EFFECT).is_some());
    }

    #[test]
    fn every_effect_rests_at_identity_and_stays_finite() {
        let ctx = |phase, eased| EffectCtx {
            phase,
            eased,
            frame: Rect {
                x: 100.0,
                y: 100.0,
                w: 200.0,
                h: 100.0,
            },
            slide_size: Size { w: 960, h: 540 },
        };
        for def in EFFECTS {
            let e = AnimEffect::new(def.id);
            let rest = sample_effect(&e, &ctx(AnimationPhase::Entering, 1.0));
            assert_eq!(rest, ObjectSample::IDENTITY, "`{}` entered", def.id);
            let rest = sample_effect(&e, &ctx(AnimationPhase::Exiting, 0.0));
            assert_eq!(rest, ObjectSample::IDENTITY, "`{}` before exit", def.id);
            for p in [f32::NAN, -1.0, 0.0, 0.3, 0.5, 2.0] {
                let s = sample_effect(&e, &ctx(AnimationPhase::Entering, p));
                assert!(s.offset.0.is_finite() && s.offset.1.is_finite());
                assert!(s.scale.0.is_finite() && s.scale.1.is_finite());
                assert!((0.0..=1.0).contains(&s.opacity));
            }
        }
    }

    #[test]
    fn unknown_effect_plays_as_fallback() {
        let ctx = EffectCtx {
            phase: AnimationPhase::Entering,
            eased: 0.25,
            frame: Rect {
                x: 0.0,
                y: 0.0,
                w: 10.0,
                h: 10.0,
            },
            slide_size: Size { w: 960, h: 540 },
        };
        let unknown = AnimEffect {
            id: "from_the_future".to_owned(),
            ..AnimEffect::default()
        };
        assert_eq!(
            sample_effect(&unknown, &ctx),
            sample_effect(&AnimEffect::new(FALLBACK_EFFECT), &ctx)
        );
    }

    #[test]
    fn sanitizing_repairs_every_new_field() {
        use crate::animation::{ConvexPoly, MAX_MASK_POLYS, Mask, Tint};
        let s = ObjectSample {
            scale: (f32::NAN, 1e9),
            rotation_deg: f32::INFINITY,
            tint: Some(Tint {
                rgb: [1, 2, 3],
                amount: f32::NAN,
            }),
            mask: Some(Mask::new(vec![
                ConvexPoly(vec![(0.0, 0.0), (1.0, 0.0)]),
                ConvexPoly(vec![(f32::NAN, 0.0), (1.0, 0.0), (1.0, 1.0)]),
                ConvexPoly(vec![(0.0, 0.0), (0.5, 0.5), (1.0, 1.0)]),
                ConvexPoly(vec![(-9.0, 0.0), (9.0, 0.0), (9.0, 1.0), (-9.0, 1.0)]),
            ])),
            ..ObjectSample::IDENTITY
        }
        .sanitized();
        assert_eq!(s.scale, (1.0, 100.0));
        assert_eq!(s.rotation_deg, 0.0);
        assert_eq!(s.tint, None);
        let mask = s.mask.expect("mask kept");
        assert_eq!(mask.polys().len(), 1, "only the real polygon survives");
        assert!(
            mask.polys()[0]
                .0
                .iter()
                .all(|&(x, _)| (-1.0..=2.0).contains(&x))
        );

        let many = vec![ConvexPoly::rect(0.0, 0.0, 1.0, 1.0); MAX_MASK_POLYS + 1];
        let s = ObjectSample {
            mask: Some(Mask::new(many)),
            ..ObjectSample::IDENTITY
        }
        .sanitized();
        assert_eq!(s.mask, None, "too many polygons show the object whole");
    }

    /// Every effect with every value of every option it takes.
    fn all_variants() -> Vec<AnimEffect> {
        let mut out = Vec::new();
        for def in EFFECTS {
            let mut variants = vec![AnimEffect::new(def.id)];
            if let Some((_, allowed)) = def.direction() {
                variants = variants
                    .iter()
                    .flat_map(|v| {
                        allowed.iter().map(|&d| AnimEffect {
                            from: Some(d),
                            ..v.clone()
                        })
                    })
                    .collect();
            }
            if let Some((_, allowed)) = def.shape() {
                variants = variants
                    .iter()
                    .flat_map(|v| {
                        allowed.iter().map(|&x| AnimEffect {
                            shape: Some(x),
                            ..v.clone()
                        })
                    })
                    .collect();
            }
            if def.inout().is_some() {
                variants = variants
                    .iter()
                    .flat_map(|v| {
                        InOut::ALL.iter().map(|&x| AnimEffect {
                            inout: Some(x),
                            ..v.clone()
                        })
                    })
                    .collect();
            }
            if let Some((_, allowed)) = def.spokes() {
                variants = variants
                    .iter()
                    .flat_map(|v| {
                        allowed.iter().map(|&x| AnimEffect {
                            spokes: Some(x),
                            ..v.clone()
                        })
                    })
                    .collect();
            }
            if def.orientation().is_some() {
                variants = variants
                    .iter()
                    .flat_map(|v| {
                        Orientation::ALL.iter().map(|&x| AnimEffect {
                            orientation: Some(x),
                            ..v.clone()
                        })
                    })
                    .collect();
            }
            out.extend(variants);
        }
        out
    }

    fn ctx_at(eased: f32) -> EffectCtx {
        EffectCtx {
            phase: AnimationPhase::Entering,
            eased,
            frame: Rect {
                x: 100.0,
                y: 100.0,
                w: 200.0,
                h: 100.0,
            },
            slide_size: Size { w: 960, h: 540 },
        }
    }

    /// Visible part of the frame, 0..1: the mask's pieces cut to the frame, added up.
    fn mask_area(m: &Mask) -> f32 {
        const FRAME_EDGES: [(f32, f32, f32); 4] = [
            (-1.0, 0.0, 0.0),
            (1.0, 0.0, 1.0),
            (0.0, -1.0, 0.0),
            (0.0, 1.0, 1.0),
        ];
        m.polys()
            .iter()
            .map(|p| {
                let cut = FRAME_EDGES
                    .iter()
                    .fold(p.0.clone(), |acc, &(a, b, k)| clip_half(&acc, a, b, k));
                if cut.len() < 3 {
                    0.0
                } else {
                    ConvexPoly(cut).area().abs()
                }
            })
            .sum()
    }

    #[test]
    fn every_variant_starts_hidden_and_masks_grow_without_overlap() {
        let variants = all_variants();
        assert!(variants.len() > 60, "{}", variants.len());
        for e in &variants {
            let start = sample_effect(e, &ctx_at(0.0));
            let hidden = start.opacity == 0.0
                || start.scale.0 == 0.0
                || start.scale.1 == 0.0
                || start.mask.as_ref().is_some_and(|m| m.polys().is_empty())
                || start
                    .clip
                    .is_some_and(|c| c.max_x <= c.min_x || c.max_y <= c.min_y)
                || start.offset.0.abs() >= 300.0
                || start.offset.1.abs() >= 200.0;
            assert!(hidden, "{e:?} is visible at the start: {start:?}");

            let mut last = -1.0;
            for i in 1..20 {
                let s = sample_effect(e, &ctx_at(i as f32 / 20.0));
                let Some(m) = &s.mask else {
                    continue;
                };
                assert!(m.polys().len() <= crate::animation::MAX_MASK_POLYS, "{e:?}");
                for p in m.polys() {
                    assert!(p.0.len() <= crate::animation::MAX_POLY_POINTS);
                    // Convex: every turn goes the same way.
                    let n = p.0.len();
                    let turns: Vec<f32> = (0..n)
                        .map(|k| {
                            let (a, b, c) = (p.0[k], p.0[(k + 1) % n], p.0[(k + 2) % n]);
                            (b.0 - a.0) * (c.1 - b.1) - (b.1 - a.1) * (c.0 - b.0)
                        })
                        .filter(|t| t.abs() > 1e-6)
                        .collect();
                    assert!(
                        turns.iter().all(|t| *t > 0.0) || turns.iter().all(|t| *t < 0.0),
                        "{e:?} has a non-convex piece {p:?}"
                    );
                }
                let area = mask_area(m);
                assert!(area <= 1.0 + 1e-3, "{e:?} pieces overlap: {area}");
                assert!(
                    area + 1e-3 >= last,
                    "{e:?} shrinks at step {i}: {last} → {area}"
                );
                last = area;
            }
        }
    }

    #[test]
    fn emphasis_impls_match_the_registry() {
        let ids: Vec<&str> = EMPHASIS_IMPLS.iter().map(|i| i.id).collect();
        let reg: Vec<&str> = coursecraft_course::EMPHASIS.iter().map(|d| d.id).collect();
        assert_eq!(ids, reg);
        assert!(coursecraft_course::emphasis_def(FALLBACK_EMPHASIS).is_some());
    }

    #[test]
    fn every_emphasis_rests_at_both_ends_and_moves_in_between() {
        let frame = Rect {
            x: 100.0,
            y: 100.0,
            w: 200.0,
            h: 100.0,
        };
        for def in coursecraft_course::EMPHASIS {
            let e = EmphasisEffect::new(def.id);
            for t in [0.0, 1.0, -1.0, 2.0, f32::NAN] {
                assert_eq!(
                    sample_emphasis(&e, t, frame),
                    ObjectSample::IDENTITY,
                    "`{}` at {t}",
                    def.id
                );
            }
            let mut moved = false;
            for i in 1..40 {
                let s = sample_emphasis(&e, i as f32 / 40.0, frame);
                assert!(s.opacity.is_finite() && s.rotation_deg.is_finite());
                moved |= s != ObjectSample::IDENTITY;
            }
            assert!(moved, "`{}` never changes the look", def.id);
            // Near the ends it is close to rest, so passes join without a jump.
            for t in [0.001_f32, 0.999] {
                let s = sample_emphasis(&e, t, frame);
                assert!(
                    (s.scale.0 - 1.0).abs() < 0.02
                        && s.offset.0.abs() < 1.0
                        && s.offset.1.abs() < 1.0,
                    "`{}` jumps at {t}: {s:?}",
                    def.id
                );
            }
        }
    }

    #[test]
    fn emphasis_layers_over_the_timeline() {
        let json = serde_json::json!({
            "id": "o", "kind": { "type": "shape", "shape": "rectangle", "fill": "#112233" },
            "frame": { "x": 0, "y": 0, "w": 100, "h": 100 },
            "entrance": { "effect": { "type": "fade" }, "duration_ms": 1000, "easing": "linear" },
            "emphasis": [{ "effect": { "type": "grow_shrink" }, "start_ms": 500, "duration_ms": 1000, "repeat": 2 }]
        });
        let obj: coursecraft_course::Object = serde_json::from_value(json).expect("object");
        let size = Size { w: 960, h: 540 };
        let at = |ms| crate::sample_timed_animation(&obj, ms, 10_000, size).expect("visible");
        assert_eq!(at(400).scale, (1.0, 1.0));
        // Mid entrance and mid first pass: half faded and grown.
        let both = at(800);
        assert!(both.opacity < 0.9 && both.scale.0 > 1.15, "{both:?}");
        // Second pass, then rest.
        assert!(at(2000).scale.0 > 1.2);
        assert_eq!(at(2600), ObjectSample::IDENTITY);
    }

    #[test]
    fn new_easings_start_and_end_in_place_and_shape_their_path() {
        use coursecraft_course::Easing;
        for e in [Easing::Overshoot, Easing::Bounce, Easing::Elastic] {
            assert!(crate::ease(e, 0.0).abs() < 1e-4, "{e:?} at 0");
            assert!((crate::ease(e, 1.0) - 1.0).abs() < 1e-4, "{e:?} at 1");
            for i in 0..=100 {
                let v = crate::ease(e, i as f32 / 100.0);
                assert!(v.is_finite() && (-0.5..=1.5).contains(&v), "{e:?} {v}");
            }
        }
        let peak = |e| {
            (0..=100)
                .map(|i| crate::ease(e, i as f32 / 100.0))
                .fold(f32::MIN, f32::max)
        };
        assert!(peak(Easing::Overshoot) > 1.05, "overshoots");
        assert!(peak(Easing::Elastic) > 1.05, "springs past");
        assert!(
            peak(Easing::Bounce) <= 1.0 + 1e-4,
            "bounce never passes the end"
        );
        // Fly in carries the object past its place with Overshoot.
        let ctx = EffectCtx {
            phase: AnimationPhase::Entering,
            eased: crate::ease(Easing::Overshoot, 0.7),
            frame: Rect {
                x: 100.0,
                y: 100.0,
                w: 100.0,
                h: 100.0,
            },
            slide_size: Size { w: 960, h: 540 },
        };
        let s = sample_effect(&AnimEffect::with_from("fly_in", Direction::Left), &ctx);
        assert!(s.offset.0 > 0.0, "past the resting place: {s:?}");
    }

    #[test]
    fn parts_get_their_own_windows() {
        use coursecraft_course::Sequence;
        // By paragraph, 3 parts: one after another.
        assert!(
            (crate::part_progress(Sequence::ByParagraph, (0, 3), 1.0 / 6.0) - 0.5).abs() < 1e-4
        );
        assert!(crate::part_progress(Sequence::ByParagraph, (1, 3), 0.3).abs() < 1e-4);
        assert!((crate::part_progress(Sequence::ByParagraph, (2, 3), 1.0) - 1.0).abs() < 1e-4);
        // By letter they overlap: the last starts before the first is long done.
        let last_start = 1.0 - 0.25;
        assert!(crate::part_progress(Sequence::ByLetter, (9, 10), last_start - 0.01) == 0.0);
        assert!(crate::part_progress(Sequence::ByLetter, (9, 10), 1.0) > 0.99);
        // As one, or one part: unchanged.
        assert!((crate::part_progress(Sequence::AsOne, (2, 3), 0.4) - 0.4).abs() < 1e-6);
        assert!((crate::part_progress(Sequence::ByLetter, (0, 1), 0.4) - 0.4).abs() < 1e-6);
    }

    #[test]
    fn timed_parts_reveal_one_paragraph_after_another() {
        let json = serde_json::json!({
            "id": "o", "kind": { "type": "text", "text": "One\nTwo" },
            "frame": { "x": 0, "y": 0, "w": 100, "h": 100 },
            "entrance": { "effect": { "type": "fade" }, "duration_ms": 1000, "easing": "linear", "sequence": "by_paragraph" }
        });
        let obj: coursecraft_course::Object = serde_json::from_value(json).expect("object");
        assert_eq!(
            crate::plays_in_parts(&obj),
            Some(coursecraft_course::Sequence::ByParagraph)
        );
        let size = Size { w: 960, h: 540 };
        let at = |ms| crate::sample_timed_parts(&obj, ms, 10_000, size, 2).expect("visible");
        let early = at(250);
        assert!(
            early[0].opacity > 0.4 && early[1].opacity < 0.01,
            "{early:?}"
        );
        let late = at(750);
        assert!((late[0].opacity - 1.0).abs() < 1e-4 && late[1].opacity > 0.4);
        assert_eq!(at(1000), vec![ObjectSample::IDENTITY; 2]);
    }

    #[test]
    fn every_transition_ends_on_the_new_slide_and_round_trips_its_options() {
        use coursecraft_course::{TRANSITIONS, Transition, TransitionEffect};
        let size = Size { w: 960, h: 540 };
        for def in TRANSITIONS {
            let fx = def.options_like.map(AnimEffect::new);
            let effect = TransitionEffect::from_parts(def.id, fx.as_ref()).expect("known id");
            assert_eq!(effect.id(), def.id);
            // The editor reads and writes options through the effect form.
            if let Some(form) = effect.as_effect() {
                assert_eq!(
                    TransitionEffect::from_parts(def.id, Some(&form)),
                    Some(effect.clone()),
                    "{}",
                    def.id
                );
            }
            let t = Transition {
                effect: effect.clone(),
                duration_ms: 500,
            };
            let end = crate::sample_transition(&t, 1.0, size);
            assert_eq!(
                end.next,
                crate::SlideLook::REST,
                "`{}` ends at rest",
                def.id
            );
            if end.prev_on_top {
                let gone = end.prev.alpha <= 0.0
                    || end.prev.offset.0.abs() >= 960.0
                    || end.prev.offset.1.abs() >= 540.0;
                assert!(gone, "`{}` leaves the old slide in the way", def.id);
            }
            for p in [0.0, 0.25, 0.5, 0.75, f32::NAN] {
                let s = crate::sample_transition(&t, p, size);
                for look in [&s.prev, &s.next] {
                    assert!(look.alpha.is_finite() && look.scale.is_finite());
                    assert!(look.offset.0.is_finite() && look.offset.1.is_finite());
                }
            }
        }
        // Old files keep their meaning.
        let old: TransitionEffect =
            serde_json::from_value(serde_json::json!({ "type": "push", "from": "left" }))
                .expect("push");
        assert_eq!(
            old,
            TransitionEffect::Push {
                from: Direction::Left
            }
        );
    }
}
