//! The animation effect registry: one entry per entrance/exit effect.
//!
//! This table is the single source of truth for which effects exist, which options each one
//! takes, its defaults, its category and its English labels. The runtime
//! (`coursecraft-logic`), the validator, the editor's galleries and panels, and the
//! `animation.effects` command all read it; `coursecraft-logic` has a test that every entry here
//! has a sample function and the other way round. Adding an effect means adding one entry here
//! and one sample function there.
//!
//! Plan: `docs/plans/m46-animation-system.md`.

use serde::Serialize;

use crate::model::{AnimEffect, Direction, InOut, Orientation, RevealShape};

/// The effect the runtime plays for an id this build does not know (a file from a newer
/// version). The validator warns about it.
pub const FALLBACK_EFFECT: &str = "fade";

/// Gallery group an effect is listed under.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectCategory {
    Basic,
    Subtle,
    Moderate,
    Exciting,
}

impl EffectCategory {
    /// Every category, in gallery order.
    pub const ALL: [Self; 4] = [Self::Basic, Self::Subtle, Self::Moderate, Self::Exciting];

    /// English label (the editor passes it through `tl!`).
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Basic => "Basic",
            Self::Subtle => "Subtle",
            Self::Moderate => "Moderate",
            Self::Exciting => "Exciting",
        }
    }
}

/// One option an effect accepts, with its default and allowed values.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "option", rename_all = "snake_case")]
pub enum EffectOptionDef {
    /// `from`: the side or corner the object enters from (or leaves toward, for exits).
    Direction {
        default: Direction,
        allowed: &'static [Direction],
    },
    /// `shape`: the outline of the reveal.
    Shape {
        default: RevealShape,
        allowed: &'static [RevealShape],
    },
    /// `inout`: open from the centre (`out`) or close from the edges (`in`).
    InOut { default: InOut },
    /// `spokes`: how many wedges sweep at once.
    Spokes { default: u8, allowed: &'static [u8] },
    /// `orientation`: horizontal or vertical.
    Orientation { default: Orientation },
}

/// Every JSON field an effect option can be stored in, in menu order.
pub const OPTION_FIELDS: [&str; 5] = ["from", "shape", "inout", "spokes", "orientation"];

impl EffectOptionDef {
    /// The JSON field this option is stored in.
    #[must_use]
    pub const fn field(&self) -> &'static str {
        match self {
            Self::Direction { .. } => "from",
            Self::Shape { .. } => "shape",
            Self::InOut { .. } => "inout",
            Self::Spokes { .. } => "spokes",
            Self::Orientation { .. } => "orientation",
        }
    }

    /// English section title for the Effect Options menu.
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Direction { .. } => "Direction",
            Self::Shape { .. } => "Shape",
            Self::InOut { .. } => "In or out",
            Self::Spokes { .. } => "Spokes",
            Self::Orientation { .. } => "Orientation",
        }
    }
}

/// One entrance/exit effect.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct EffectDef {
    /// Stored id (`"fly_in"`), the `type` of an [`AnimEffect`].
    pub id: &'static str,
    pub category: EffectCategory,
    /// English name as an entrance ("Fly in").
    pub entrance_label: &'static str,
    /// English name as an exit ("Fly out").
    pub exit_label: &'static str,
    /// Options the effect accepts. Fields not listed here are ignored by the runtime and
    /// reported by the validator.
    pub options: &'static [EffectOptionDef],
    /// Duration the editor uses when the effect is first applied (0 for instant effects).
    pub default_duration_ms: u32,
}

impl EffectDef {
    /// The label for `exit` (true) or entrance (false).
    #[must_use]
    pub const fn label(&self, exit: bool) -> &'static str {
        if exit {
            self.exit_label
        } else {
            self.entrance_label
        }
    }

    /// The direction option, if the effect has one.
    #[must_use]
    pub fn direction(&self) -> Option<(Direction, &'static [Direction])> {
        self.options.iter().find_map(|o| match *o {
            EffectOptionDef::Direction { default, allowed } => Some((default, allowed)),
            _ => None,
        })
    }

    /// The shape option, if the effect has one.
    #[must_use]
    pub fn shape(&self) -> Option<(RevealShape, &'static [RevealShape])> {
        self.options.iter().find_map(|o| match *o {
            EffectOptionDef::Shape { default, allowed } => Some((default, allowed)),
            _ => None,
        })
    }

    /// The in/out option's default, if the effect has one.
    #[must_use]
    pub fn inout(&self) -> Option<InOut> {
        self.options.iter().find_map(|o| match *o {
            EffectOptionDef::InOut { default } => Some(default),
            _ => None,
        })
    }

    /// The spokes option, if the effect has one.
    #[must_use]
    pub fn spokes(&self) -> Option<(u8, &'static [u8])> {
        self.options.iter().find_map(|o| match *o {
            EffectOptionDef::Spokes { default, allowed } => Some((default, allowed)),
            _ => None,
        })
    }

    /// The orientation option's default, if the effect has one.
    #[must_use]
    pub fn orientation(&self) -> Option<Orientation> {
        self.options.iter().find_map(|o| match *o {
            EffectOptionDef::Orientation { default } => Some(default),
            _ => None,
        })
    }

    /// Whether the effect takes the option stored in `field`.
    #[must_use]
    pub fn takes(&self, field: &str) -> bool {
        self.options.iter().any(|o| o.field() == field)
    }

    /// What is wrong with `effect`'s stored options for this effect: `(field, message)` for
    /// every option it doesn't take or a value it doesn't allow. Empty when all is well.
    #[must_use]
    pub fn option_problems(&self, effect: &AnimEffect) -> Vec<(&'static str, String)> {
        let mut out = Vec::new();
        let id = self.id;
        let mut check = |field: &'static str, set: bool, allowed: Option<bool>| match (set, allowed)
        {
            (false, _) | (true, Some(true)) => {}
            (true, None) => out.push((field, format!("effect `{id}` takes no `{field}`"))),
            (true, Some(false)) => out.push((
                field,
                format!("effect `{id}` does not allow this `{field}`"),
            )),
        };
        check(
            "from",
            effect.from.is_some(),
            self.direction()
                .map(|(_, a)| effect.from.is_some_and(|v| a.contains(&v))),
        );
        check(
            "shape",
            effect.shape.is_some(),
            self.shape()
                .map(|(_, a)| effect.shape.is_some_and(|v| a.contains(&v))),
        );
        check("inout", effect.inout.is_some(), self.inout().map(|_| true));
        check(
            "spokes",
            effect.spokes.is_some(),
            self.spokes()
                .map(|(_, a)| effect.spokes.is_some_and(|v| a.contains(&v))),
        );
        check(
            "orientation",
            effect.orientation.is_some(),
            self.orientation().map(|_| true),
        );
        out
    }
}

const SIDES: &[Direction] = &[
    Direction::Left,
    Direction::Right,
    Direction::Top,
    Direction::Bottom,
];

const CORNERS: &[Direction] = &[
    Direction::TopLeft,
    Direction::TopRight,
    Direction::BottomLeft,
    Direction::BottomRight,
];

const FROM_SIDE: EffectOptionDef = EffectOptionDef::Direction {
    default: Direction::Bottom,
    allowed: SIDES,
};

const FROM_ANY: EffectOptionDef = EffectOptionDef::Direction {
    default: Direction::Bottom,
    allowed: &Direction::ALL,
};

const FROM_CORNER: EffectOptionDef = EffectOptionDef::Direction {
    default: Direction::BottomRight,
    allowed: CORNERS,
};

const ANY_SHAPE: EffectOptionDef = EffectOptionDef::Shape {
    default: RevealShape::Circle,
    allowed: &RevealShape::ALL,
};

const IN_OUT: EffectOptionDef = EffectOptionDef::InOut {
    default: InOut::Out,
};

const SPOKES: EffectOptionDef = EffectOptionDef::Spokes {
    default: 1,
    allowed: &[1, 2, 3, 4, 8],
};

const HORIZONTAL: EffectOptionDef = EffectOptionDef::Orientation {
    default: Orientation::Horizontal,
};

const VERTICAL: EffectOptionDef = EffectOptionDef::Orientation {
    default: Orientation::Vertical,
};

const fn def(
    id: &'static str,
    category: EffectCategory,
    labels: (&'static str, &'static str),
    options: &'static [EffectOptionDef],
    default_duration_ms: u32,
) -> EffectDef {
    EffectDef {
        id,
        category,
        entrance_label: labels.0,
        exit_label: labels.1,
        options,
        default_duration_ms,
    }
}

use EffectCategory::{Basic, Exciting, Moderate, Subtle};

/// Every effect, in gallery order.
pub static EFFECTS: &[EffectDef] = &[
    // Basic
    // Appear shows at once; 1 ms is the shortest duration an animation may have.
    def("appear", Basic, ("Appear", "Disappear"), &[], 1),
    def("fade", Basic, ("Fade in", "Fade out"), &[], 500),
    def("grow", Basic, ("Grow", "Shrink"), &[], 500),
    def("fly_in", Basic, ("Fly in", "Fly out"), &[FROM_ANY], 500),
    def(
        "float",
        Subtle,
        ("Float in", "Float out"),
        &[FROM_SIDE],
        500,
    ),
    def("split", Basic, ("Split", "Split"), &[VERTICAL, IN_OUT], 500),
    def("wipe", Basic, ("Wipe in", "Wipe out"), &[FROM_ANY], 500),
    def(
        "shape",
        Basic,
        ("Shape", "Shape"),
        &[ANY_SHAPE, IN_OUT],
        1000,
    ),
    def("wheel", Basic, ("Wheel", "Wheel"), &[SPOKES], 1000),
    def(
        "random_bars",
        Basic,
        ("Random bars", "Random bars"),
        &[HORIZONTAL],
        500,
    ),
    def("zoom", Basic, ("Zoom in", "Zoom out"), &[], 500),
    // Subtle
    def(
        "dissolve",
        Subtle,
        ("Dissolve in", "Dissolve out"),
        &[],
        750,
    ),
    def("blinds", Subtle, ("Blinds", "Blinds"), &[HORIZONTAL], 500),
    def("peek", Subtle, ("Peek in", "Peek out"), &[FROM_SIDE], 500),
    def("expand", Subtle, ("Expand", "Contract"), &[], 750),
    def("rise", Subtle, ("Rise up", "Sink down"), &[], 750),
    // Moderate
    def("spin", Moderate, ("Spin", "Spin"), &[], 750),
    def(
        "spin_grow",
        Moderate,
        ("Spin & grow", "Spin & shrink"),
        &[],
        1000,
    ),
    def(
        "grow_turn",
        Moderate,
        ("Grow & turn", "Shrink & turn"),
        &[],
        750,
    ),
    def(
        "checkerboard",
        Moderate,
        ("Checkerboard", "Checkerboard"),
        &[HORIZONTAL],
        750,
    ),
    def(
        "strips",
        Moderate,
        ("Strips", "Strips"),
        &[FROM_CORNER],
        750,
    ),
    def("flip", Moderate, ("Flip in", "Flip out"), &[VERTICAL], 750),
    // Exciting
    def("swivel", Exciting, ("Swivel", "Swivel"), &[VERTICAL], 1000),
    def("bounce", Exciting, ("Bounce in", "Bounce out"), &[], 1000),
    def("pop", Exciting, ("Pop in", "Pop out"), &[], 500),
];

/// The registry entry for `id`, if this build knows it.
#[must_use]
pub fn effect_def(id: &str) -> Option<&'static EffectDef> {
    EFFECTS.iter().find(|d| d.id == id)
}

/// One emphasis effect. Every emphasis starts and ends at the object's resting look.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct EmphasisDef {
    /// Stored id (`"pulse"`), the `type` of an [`crate::EmphasisEffect`].
    pub id: &'static str,
    /// English name ("Pulse").
    pub label: &'static str,
    /// What it is for, in a few words (gallery tooltip). English.
    pub hint: &'static str,
    pub default_duration_ms: u32,
}

const fn emph(id: &'static str, label: &'static str, hint: &'static str, ms: u32) -> EmphasisDef {
    EmphasisDef {
        id,
        label,
        hint,
        default_duration_ms: ms,
    }
}

/// Every emphasis effect, in gallery order. `coursecraft-logic` has one sample function per id.
pub static EMPHASIS: &[EmphasisDef] = &[
    emph("pulse", "Pulse", "Grows a little and back", 500),
    emph(
        "grow_shrink",
        "Grow and shrink",
        "Grows by a quarter and back",
        1000,
    ),
    emph("teeter", "Teeter", "Rocks from side to side", 1000),
    emph("spin", "Spin", "One full turn", 1000),
    emph("shake", "Shake", "Shakes sideways, for a wrong answer", 600),
    emph(
        "heartbeat",
        "Heartbeat",
        "Two quick beats, for a call to action",
        1000,
    ),
    emph("jump", "Jump", "Hops up and lands", 800),
    emph("blink", "Blink", "Flashes off and on", 900),
    emph(
        "transparency",
        "Transparency",
        "Fades halfway and back",
        1000,
    ),
    emph("darken", "Darken", "Darkens and back", 1000),
    emph("lighten", "Lighten", "Lightens and back", 1000),
    emph(
        "color_pulse",
        "Colour pulse",
        "Washes with gold and back",
        1000,
    ),
];

/// The emphasis played for an id this build does not know. The validator warns about it.
pub const FALLBACK_EMPHASIS: &str = "pulse";

/// The emphasis registry entry for `id`, if this build knows it.
#[must_use]
pub fn emphasis_def(id: &str) -> Option<&'static EmphasisDef> {
    EMPHASIS.iter().find(|d| d.id == id)
}

impl crate::EmphasisEffect {
    /// An emphasis effect with registry id `id`.
    #[must_use]
    pub fn new(id: &str) -> Self {
        Self { id: id.to_owned() }
    }

    /// Its registry entry, if this build knows it.
    #[must_use]
    pub fn def(&self) -> Option<&'static EmphasisDef> {
        emphasis_def(&self.id)
    }
}

impl crate::Emphasis {
    /// A new emphasis of `id` at `start_ms` with the effect's default duration.
    #[must_use]
    pub fn new(id: &str, start_ms: u32) -> Self {
        Self {
            effect: crate::EmphasisEffect::new(id),
            start_ms,
            duration_ms: emphasis_def(id).map_or(1000, |d| d.default_duration_ms),
            easing: crate::Easing::Linear,
            repeat: 1,
        }
    }
}

impl AnimEffect {
    /// The effect `id` with every option at its default (written out, so the stored file says
    /// what plays).
    #[must_use]
    pub fn new(id: &str) -> Self {
        let d = effect_def(id);
        Self {
            id: id.to_owned(),
            from: d.and_then(EffectDef::direction).map(|(v, _)| v),
            shape: d.and_then(EffectDef::shape).map(|(v, _)| v),
            inout: d.and_then(EffectDef::inout),
            spokes: d.and_then(EffectDef::spokes).map(|(v, _)| v),
            orientation: d.and_then(EffectDef::orientation),
        }
    }

    /// The effect `id` coming from (or leaving toward) `from`, other options at default.
    #[must_use]
    pub fn with_from(id: &str, from: Direction) -> Self {
        Self {
            from: Some(from),
            ..Self::new(id)
        }
    }

    /// The registry entry, or `None` for an unknown id.
    #[must_use]
    pub fn def(&self) -> Option<&'static EffectDef> {
        effect_def(&self.id)
    }

    /// The direction to play: the stored one when the effect takes a direction and allows it,
    /// else the effect's default; `None` when the effect has no direction.
    #[must_use]
    pub fn direction(&self) -> Option<Direction> {
        let (default, allowed) = self.def()?.direction()?;
        Some(self.from.filter(|d| allowed.contains(d)).unwrap_or(default))
    }

    /// The reveal shape to play (same rules as [`Self::direction`]).
    #[must_use]
    pub fn shape(&self) -> Option<RevealShape> {
        let (default, allowed) = self.def()?.shape()?;
        Some(
            self.shape
                .filter(|v| allowed.contains(v))
                .unwrap_or(default),
        )
    }

    /// In or out (same rules as [`Self::direction`]).
    #[must_use]
    pub fn inout(&self) -> Option<InOut> {
        let default = self.def()?.inout()?;
        Some(self.inout.unwrap_or(default))
    }

    /// The spoke count to play (same rules as [`Self::direction`]).
    #[must_use]
    pub fn spokes(&self) -> Option<u8> {
        let (default, allowed) = self.def()?.spokes()?;
        Some(
            self.spokes
                .filter(|v| allowed.contains(v))
                .unwrap_or(default),
        )
    }

    /// The orientation to play (same rules as [`Self::direction`]).
    #[must_use]
    pub fn orientation(&self) -> Option<Orientation> {
        let default = self.def()?.orientation()?;
        Some(self.orientation.unwrap_or(default))
    }

    /// Change the effect, keeping every option the new effect also takes with that value.
    #[must_use]
    pub fn switched_to(&self, id: &str) -> Self {
        let mut next = Self::new(id);
        let Some(d) = next.def() else {
            return next;
        };
        if let (Some(v), Some((_, a))) = (self.from, d.direction())
            && a.contains(&v)
        {
            next.from = Some(v);
        }
        if let (Some(v), Some((_, a))) = (self.shape, d.shape())
            && a.contains(&v)
        {
            next.shape = Some(v);
        }
        if let (Some(v), Some(_)) = (self.inout, d.inout()) {
            next.inout = Some(v);
        }
        if let (Some(v), Some((_, a))) = (self.spokes, d.spokes())
            && a.contains(&v)
        {
            next.spokes = Some(v);
        }
        if let (Some(v), Some(_)) = (self.orientation, d.orientation()) {
            next.orientation = Some(v);
        }
        next
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Animation, Course, Severity};

    fn anim(json: &str) -> Result<Animation, serde_json::Error> {
        serde_json::from_str(json)
    }

    #[test]
    fn stored_shape_is_unchanged() {
        for json in [
            r#"{"effect":{"type":"fade"},"duration_ms":300,"easing":"ease_out"}"#,
            r#"{"effect":{"type":"fly_in","from":"left"},"duration_ms":300,"easing":"ease_out"}"#,
            r#"{"effect":{"type":"wipe","from":"top"},"duration_ms":500,"easing":"linear"}"#,
        ] {
            let a = anim(json).expect("parses");
            assert_eq!(serde_json::to_string(&a).expect("writes"), json);
        }
    }

    #[test]
    fn missing_direction_plays_the_default() {
        let a = anim(r#"{"effect":{"type":"float"}}"#).expect("parses");
        assert_eq!(a.effect.direction(), Some(Direction::Bottom));
        assert_eq!(AnimEffect::new("zoom").direction(), None);
    }

    #[test]
    fn unknown_field_is_refused_unknown_id_is_not() {
        assert!(anim(r#"{"effect":{"type":"fade","form":"left"}}"#).is_err());
        let a = anim(r#"{"effect":{"type":"from_the_future"}}"#).expect("parses");
        assert!(a.effect.def().is_none());
    }

    #[test]
    fn switching_keeps_an_allowed_direction() {
        let fly = AnimEffect::with_from("fly_in", Direction::Left);
        assert_eq!(fly.switched_to("wipe").from, Some(Direction::Left));
        assert_eq!(fly.switched_to("fade").from, None);
        assert_eq!(
            AnimEffect::new("fade").switched_to("float").from,
            Some(Direction::Bottom)
        );
    }

    #[test]
    fn registry_ids_are_unique_and_defaults_allowed() {
        for (i, d) in EFFECTS.iter().enumerate() {
            assert!(
                EFFECTS[..i].iter().all(|o| o.id != d.id),
                "duplicate `{}`",
                d.id
            );
            assert!((1..=60_000).contains(&d.default_duration_ms), "`{}`", d.id);
            if let Some((default, allowed)) = d.direction() {
                assert!(allowed.contains(&default), "`{}` default", d.id);
            }
            if let Some((default, allowed)) = d.shape() {
                assert!(allowed.contains(&default), "`{}` default", d.id);
            }
            if let Some((default, allowed)) = d.spokes() {
                assert!(allowed.contains(&default), "`{}` default", d.id);
            }
            for (i, o) in d.options.iter().enumerate() {
                assert!(
                    d.options[..i].iter().all(|p| p.field() != o.field()),
                    "`{}` lists `{}` twice",
                    d.id,
                    o.field()
                );
                assert!(OPTION_FIELDS.contains(&o.field()));
            }
            assert!(
                AnimEffect::new(d.id)
                    .def()
                    .is_some_and(|x| x.option_problems(&AnimEffect::new(d.id)).is_empty()),
                "`{}` defaults are not valid",
                d.id
            );
        }
        assert!(effect_def(FALLBACK_EFFECT).is_some());
    }

    #[test]
    fn validator_warns_about_unknown_effects_and_options() {
        let mut c = Course::from_json(include_str!("../../../examples/hello-course/course.json"))
            .expect("hello course");
        let obj = c
            .scenes
            .iter_mut()
            .flat_map(|s| s.slides.iter_mut())
            .flat_map(|s| s.layers.iter_mut())
            .flat_map(|l| l.objects.iter_mut())
            .next()
            .expect("an object");
        obj.entrance = Some(Animation {
            effect: AnimEffect {
                id: "from_the_future".to_owned(),
                ..AnimEffect::default()
            },
            duration_ms: 500,
            easing: crate::Easing::EaseOut,
            sequence: crate::Sequence::AsOne,
        });
        obj.exit = Some(Animation {
            effect: AnimEffect::with_from("zoom", Direction::Left),
            duration_ms: 500,
            easing: crate::Easing::EaseOut,
            sequence: Default::default(),
        });
        let issues = c.validate();
        let warn = |needle: &str| {
            issues.iter().any(|i| {
                i.severity == Severity::Warning
                    && i.message.contains(needle)
                    && i.path.contains("effect")
            })
        };
        assert!(warn("unknown animation effect"), "{issues:?}");
        assert!(warn("takes no `from`"), "{issues:?}");
        assert!(
            issues.iter().all(|i| i.severity != Severity::Error),
            "{issues:?}"
        );
    }

    #[test]
    fn examples_round_trip_their_animations() {
        for text in [
            include_str!("../../../examples/reference-course/course.json"),
            include_str!("../../../examples/showcase/course.json"),
        ] {
            let c = Course::from_json(text).expect("example parses");
            let again = Course::from_json(&c.to_json().expect("writes")).expect("reparses");
            assert_eq!(c, again);
        }
    }

    #[test]
    fn options_round_trip_and_switch() {
        let json = r#"{"type":"shape","shape":"diamond","inout":"in"}"#;
        let e: AnimEffect = serde_json::from_str(json).expect("parses");
        assert_eq!(serde_json::to_string(&e).expect("writes"), json);
        assert_eq!(e.shape(), Some(RevealShape::Diamond));
        assert_eq!(e.inout(), Some(InOut::In));
        // Split keeps in/out; Wheel keeps nothing.
        let split = e.switched_to("split");
        assert_eq!(split.inout, Some(InOut::In));
        assert_eq!(split.shape, None);
        assert_eq!(split.orientation, Some(Orientation::Vertical));
        assert_eq!(e.switched_to("wheel").spokes, Some(1));
        // A diagonal survives a switch to an effect that allows it, not to one that doesn't.
        let fly = AnimEffect::with_from("fly_in", Direction::TopLeft);
        assert_eq!(fly.switched_to("wipe").from, Some(Direction::TopLeft));
        assert_eq!(fly.switched_to("peek").from, Some(Direction::Bottom));
    }

    #[test]
    fn option_problems_name_the_field() {
        let wheel = effect_def("wheel").expect("wheel");
        let bad = AnimEffect {
            spokes: Some(5),
            from: Some(Direction::Left),
            ..AnimEffect::new("wheel")
        };
        let p = wheel.option_problems(&bad);
        assert!(p.iter().any(|(f, _)| *f == "spokes"));
        assert!(p.iter().any(|(f, _)| *f == "from"));
        assert_eq!(
            bad.spokes(),
            Some(1),
            "a disallowed count plays the default"
        );
        assert!(wheel.option_problems(&AnimEffect::new("wheel")).is_empty());
    }
}

// ---------------------------------------------------------------------------
// Slide transitions (M46 phase 8)
// ---------------------------------------------------------------------------

/// One slide transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TransitionDef {
    /// Stored `type` (`"wipe"`).
    pub id: &'static str,
    /// English name ("Wipe").
    pub label: &'static str,
    /// The entrance effect whose options (and option icons) this transition takes, if any.
    pub options_like: Option<&'static str>,
}

const fn tr(
    id: &'static str,
    label: &'static str,
    options_like: Option<&'static str>,
) -> TransitionDef {
    TransitionDef {
        id,
        label,
        options_like,
    }
}

/// Every slide transition, in gallery order.
pub static TRANSITIONS: &[TransitionDef] = &[
    tr("fade", "Fade", None),
    tr("fade_black", "Fade through black", None),
    tr("push", "Push", Some("fly_in")),
    tr("cover", "Cover", Some("fly_in")),
    tr("uncover", "Uncover", Some("fly_in")),
    tr("reveal", "Reveal", Some("fly_in")),
    tr("wipe", "Wipe", Some("wipe")),
    tr("split", "Split", Some("split")),
    tr("zoom", "Zoom", None),
    tr("shape", "Shape", Some("shape")),
    tr("clock", "Clock", Some("wheel")),
    tr("dissolve", "Dissolve", None),
    tr("random_bars", "Random bars", Some("random_bars")),
    tr("blinds", "Blinds", Some("blinds")),
    tr("checker", "Checkerboard", Some("checkerboard")),
];

/// The transition registry entry for `id`.
#[must_use]
pub fn transition_def(id: &str) -> Option<&'static TransitionDef> {
    TRANSITIONS.iter().find(|d| d.id == id)
}

impl crate::TransitionEffect {
    /// The stored `type`.
    #[must_use]
    pub const fn id(&self) -> &'static str {
        use crate::TransitionEffect as T;
        match self {
            T::Fade => "fade",
            T::FadeBlack => "fade_black",
            T::Push { .. } => "push",
            T::Cover { .. } => "cover",
            T::Uncover { .. } => "uncover",
            T::Reveal { .. } => "reveal",
            T::Wipe { .. } => "wipe",
            T::Split { .. } => "split",
            T::Zoom => "zoom",
            T::Shape { .. } => "shape",
            T::Clock { .. } => "clock",
            T::Dissolve => "dissolve",
            T::RandomBars { .. } => "random_bars",
            T::Blinds { .. } => "blinds",
            T::Checker { .. } => "checker",
        }
    }

    /// Its registry entry.
    #[must_use]
    pub fn def(&self) -> Option<&'static TransitionDef> {
        transition_def(self.id())
    }

    /// The transition's options as an entrance effect of [`TransitionDef::options_like`]
    /// (`None` when it takes none). The editor's option pickers and the runtime's reveal masks
    /// both use it.
    #[must_use]
    pub fn as_effect(&self) -> Option<AnimEffect> {
        use crate::TransitionEffect as T;
        let like = self.def()?.options_like?;
        let mut fx = AnimEffect::new(like);
        match *self {
            T::Push { from }
            | T::Cover { from }
            | T::Uncover { from }
            | T::Reveal { from }
            | T::Wipe { from } => fx.from = Some(from),
            T::Split { orientation, inout } => {
                fx.orientation = Some(orientation);
                fx.inout = Some(inout);
            }
            T::Shape { shape, inout } => {
                fx.shape = Some(shape);
                fx.inout = Some(inout);
            }
            T::Clock { spokes } => fx.spokes = Some(spokes),
            T::RandomBars { orientation }
            | T::Blinds { orientation }
            | T::Checker { orientation } => fx.orientation = Some(orientation),
            T::Fade | T::FadeBlack | T::Zoom | T::Dissolve => {}
        }
        Some(fx)
    }

    /// Transition `id` with the options of `fx` (each missing one at its default). `None` for an
    /// unknown id.
    #[must_use]
    pub fn from_parts(id: &str, fx: Option<&AnimEffect>) -> Option<Self> {
        use crate::TransitionEffect as T;
        let from = fx.and_then(|f| f.from).unwrap_or_default();
        let orientation = fx.and_then(|f| f.orientation).unwrap_or_default();
        let inout = fx.and_then(|f| f.inout).unwrap_or_default();
        Some(match id {
            "fade" => T::Fade,
            "fade_black" => T::FadeBlack,
            "push" => T::Push { from },
            "cover" => T::Cover { from },
            "uncover" => T::Uncover { from },
            "reveal" => T::Reveal { from },
            "wipe" => T::Wipe { from },
            "split" => T::Split { orientation, inout },
            "zoom" => T::Zoom,
            "shape" => T::Shape {
                shape: fx.and_then(|f| f.shape).unwrap_or_default(),
                inout,
            },
            "clock" => T::Clock {
                spokes: fx.and_then(|f| f.spokes).unwrap_or(1),
            },
            "dissolve" => T::Dissolve,
            "random_bars" => T::RandomBars { orientation },
            "blinds" => T::Blinds { orientation },
            "checker" => T::Checker { orientation },
            _ => return None,
        })
    }
}
