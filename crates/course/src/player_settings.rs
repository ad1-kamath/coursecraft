//! The one place where course player settings meet per-slide overrides.
//!
//! Every reader (runtime, renderer, validation, course map, editor) asks
//! [`Course::player_for`] or [`PlayerSettings::for_slide`] for the settings in
//! effect on a slide. Nothing else reads `Slide.player` field by field, so a
//! setting cannot mean one thing in the editor and another in the player.
//! Adding a field to [`SlidePlayerOverrides`] fails to compile here until it is
//! resolved, compared and listed.

use crate::{Course, PlayerSettings, Slide, SlidePlayerOverrides};

/// One per-slide player override, as named in `Slide.player`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlayerOverrideField {
    ShowBar,
    ShowNav,
    ShowSeekbar,
    ShowMenu,
    NextRequires,
}

impl PlayerOverrideField {
    /// Every override, in inspector order.
    pub const ALL: [Self; 5] = [
        Self::NextRequires,
        Self::ShowBar,
        Self::ShowNav,
        Self::ShowSeekbar,
        Self::ShowMenu,
    ];

    /// The JSON key, the same in `player` and `Slide.player`.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::ShowBar => "show_bar",
            Self::ShowNav => "show_nav",
            Self::ShowSeekbar => "show_seekbar",
            Self::ShowMenu => "show_menu",
            Self::NextRequires => "next_requires",
        }
    }

    /// Parse a JSON key.
    #[must_use]
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|field| field.key() == key)
    }
}

impl SlidePlayerOverrides {
    /// Write every set override over `settings`.
    pub fn apply_to(&self, settings: &mut PlayerSettings) {
        let Self {
            show_bar,
            next_requires,
            show_nav,
            show_seekbar,
            show_menu,
        } = self;
        if let Some(value) = show_bar {
            settings.show_bar = *value;
        }
        if let Some(rule) = next_requires {
            settings.next_requires = rule.clone();
        }
        if let Some(value) = show_nav {
            settings.show_nav = *value;
        }
        if let Some(value) = show_seekbar {
            settings.show_seekbar = *value;
        }
        if let Some(value) = show_menu {
            settings.show_menu = *value;
        }
    }

    /// Whether `field` is set on this slide.
    #[must_use]
    pub fn is_set(&self, field: PlayerOverrideField) -> bool {
        match field {
            PlayerOverrideField::ShowBar => self.show_bar.is_some(),
            PlayerOverrideField::ShowNav => self.show_nav.is_some(),
            PlayerOverrideField::ShowSeekbar => self.show_seekbar.is_some(),
            PlayerOverrideField::ShowMenu => self.show_menu.is_some(),
            PlayerOverrideField::NextRequires => self.next_requires.is_some(),
        }
    }

    /// Clear `field`, so the slide follows the course again.
    pub fn clear(&mut self, field: PlayerOverrideField) {
        match field {
            PlayerOverrideField::ShowBar => self.show_bar = None,
            PlayerOverrideField::ShowNav => self.show_nav = None,
            PlayerOverrideField::ShowSeekbar => self.show_seekbar = None,
            PlayerOverrideField::ShowMenu => self.show_menu = None,
            PlayerOverrideField::NextRequires => self.next_requires = None,
        }
    }

    /// Whether no override is set.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        PlayerOverrideField::ALL
            .into_iter()
            .all(|field| !self.is_set(field))
    }

    /// The overrides that change the outcome on this slide: set, and different
    /// from what `course` says.
    #[must_use]
    pub fn differing_from(&self, course: &PlayerSettings) -> Vec<PlayerOverrideField> {
        let Self {
            show_bar,
            next_requires,
            show_nav,
            show_seekbar,
            show_menu,
        } = self;
        let mut out = Vec::new();
        if next_requires
            .as_ref()
            .is_some_and(|rule| *rule != course.next_requires)
        {
            out.push(PlayerOverrideField::NextRequires);
        }
        for (field, value, base) in [
            (PlayerOverrideField::ShowBar, show_bar, course.show_bar),
            (PlayerOverrideField::ShowNav, show_nav, course.show_nav),
            (
                PlayerOverrideField::ShowSeekbar,
                show_seekbar,
                course.show_seekbar,
            ),
            (PlayerOverrideField::ShowMenu, show_menu, course.show_menu),
        ] {
            if value.is_some_and(|value| value != base) {
                out.push(field);
            }
        }
        out
    }
}

impl PlayerSettings {
    /// These course settings with `slide`'s overrides applied. `None` (no
    /// slide yet) returns the course settings.
    #[must_use]
    pub fn for_slide(&self, slide: Option<&Slide>) -> Self {
        let mut settings = self.clone();
        if let Some(overrides) = slide.and_then(|slide| slide.player.as_ref()) {
            overrides.apply_to(&mut settings);
        }
        settings
    }

    /// Whether the learner's Previous / Next controls (buttons and `←`/`→`)
    /// are available: they live on the player bar.
    #[must_use]
    pub const fn nav_available(&self) -> bool {
        self.show_bar && self.show_nav
    }

    /// Whether the slide menu (outline button, `M`, drawer) is available.
    #[must_use]
    pub const fn menu_available(&self) -> bool {
        self.show_bar && self.show_menu
    }

    /// Whether the seekbar is on screen.
    #[must_use]
    pub const fn seekbar_available(&self) -> bool {
        self.show_bar && self.show_seekbar
    }
}

/// A slide whose own player settings change a course setting.
#[derive(Debug, Clone)]
pub struct PlayerOverrideUse<'c> {
    pub slide: &'c Slide,
    pub fields: Vec<PlayerOverrideField>,
}

impl Course {
    /// The player settings in effect on `slide`.
    #[must_use]
    pub fn player_for(&self, slide: &Slide) -> PlayerSettings {
        self.player.for_slide(Some(slide))
    }

    /// Slides whose overrides change a course player setting, in course order.
    /// The editor shows these next to the course settings they beat.
    #[must_use]
    pub fn player_overrides_in_effect(&self) -> Vec<PlayerOverrideUse<'_>> {
        self.slides()
            .filter_map(|slide| {
                let fields = slide.player.as_ref()?.differing_from(&self.player);
                (!fields.is_empty()).then_some(PlayerOverrideUse { slide, fields })
            })
            .collect()
    }

    /// Slides that override `field` with a value different from the course's.
    #[must_use]
    pub fn slides_overriding(&self, field: PlayerOverrideField) -> Vec<&Slide> {
        self.player_overrides_in_effect()
            .into_iter()
            .filter(|usage| usage.fields.contains(&field))
            .map(|usage| usage.slide)
            .collect()
    }
}
