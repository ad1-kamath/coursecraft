//! Design tokens, themes, palettes, and styling for CourseCraft.
//!
//! Provides the design system defined in `docs/plans/m5-look-and-feel.md`:
//! exact colour roles and hex values for Dark and Light modes, type sizes,
//! spacing scale, corner radii, control heights, shadows, and WCAG contrast calculations.

use std::collections::BTreeMap;

use egui::epaint::Shadow;
use egui::{
    Color32, CornerRadius, FontFamily, FontId, Id, Margin, RichText, Stroke, TextStyle, vec2,
};

use crate::font::install_fonts;

/// Theme mode for the editor and application chrome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeMode {
    /// Dark theme (default). Refined neutral greys.
    #[default]
    Dark,
    /// Light theme. Clean, bright surfaces with strong contrast.
    Light,
}

/// Theme preference stored in preferences and selectable from menus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemePref {
    /// Always use dark theme.
    #[default]
    Dark,
    /// Always use light theme.
    Light,
    /// Match operating system theme (defaults to dark if undetermined).
    System,
}

impl ThemePref {
    /// Resolve preference to a concrete [`ThemeMode`] given the egui context.
    #[must_use]
    pub fn resolve(self, ctx: &egui::Context) -> ThemeMode {
        match self {
            Self::Dark => ThemeMode::Dark,
            Self::Light => ThemeMode::Light,
            Self::System => match ctx.system_theme() {
                Some(egui::Theme::Light) => ThemeMode::Light,
                _ => ThemeMode::Dark,
            },
        }
    }
}

/// Colour palette roles for application chrome.
///
/// All values are sRGB and meet WCAG AA contrast standards on their respective surfaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    /// Window ground.
    pub app: Color32,
    /// Backdrop behind the slide canvas.
    pub canvas: Color32,
    /// Surfaces of toolbars, navigator, inspector, timeline, and status bar.
    pub panel: Color32,
    /// Surfaces of popovers, menus, dialogs, and cards.
    pub raised: Color32,
    /// Background of text fields, drag values, and combo buttons.
    pub input: Color32,
    /// Hover fill on panels.
    pub hover: Color32,
    /// Pressed or neutral-selected fill.
    pub pressed: Color32,
    /// Hairlines and separators (1 px).
    pub border: Color32,
    /// Input outlines and focused-panel edges.
    pub border_strong: Color32,
    /// Primary text.
    pub text: Color32,
    /// Labels and section headers.
    pub text_secondary: Color32,
    /// Placeholders, hints, and timecodes.
    pub text_tertiary: Color32,
    /// Disabled text.
    pub text_disabled: Color32,
    /// Primary actions, selection handles, active tabs.
    pub accent: Color32,
    /// Hover state on accent fills.
    pub accent_hover: Color32,
    /// Pressed state on accent fills.
    pub accent_pressed: Color32,
    /// Accent colour when used as text or icons on panel.
    pub accent_text: Color32,
    /// Selected row or thumbnail fill, text selection ground.
    pub accent_subtle: Color32,
    /// Success indicator (green).
    pub success: Color32,
    /// Warning indicator (yellow/amber).
    pub warning: Color32,
    /// Shape adjustment handles (yellow diamonds).
    pub adjust_handle: Color32,
    /// Danger or error indicator (red).
    pub danger: Color32,
}

impl Palette {
    /// The default dark palette from the M5 design specification.
    #[must_use]
    pub const fn dark() -> Self {
        Self {
            app: Color32::from_rgb(0x1C, 0x1C, 0x1E),
            canvas: Color32::from_rgb(0x16, 0x16, 0x18),
            panel: Color32::from_rgb(0x25, 0x25, 0x28),
            raised: Color32::from_rgb(0x2F, 0x2F, 0x33),
            input: Color32::from_rgb(0x1E, 0x1E, 0x21),
            hover: Color32::from_rgb(0x32, 0x32, 0x36),
            pressed: Color32::from_rgb(0x3A, 0x3A, 0x3F),
            border: Color32::from_rgb(0x36, 0x36, 0x3A),
            border_strong: Color32::from_rgb(0x48, 0x48, 0x4E),
            text: Color32::from_rgb(0xF2, 0xF2, 0xF5),
            text_secondary: Color32::from_rgb(0xA8, 0xA8, 0xB0),
            text_tertiary: Color32::from_rgb(0x8E, 0x8E, 0x96),
            text_disabled: Color32::from_rgb(0x55, 0x55, 0x5B),
            accent: Color32::from_rgb(0x0A, 0x6C, 0xFF),
            accent_hover: Color32::from_rgb(0x2B, 0x80, 0xFF),
            accent_pressed: Color32::from_rgb(0x00, 0x58, 0xD6),
            accent_text: Color32::from_rgb(0x4D, 0x9D, 0xFF),
            accent_subtle: Color32::from_rgb(0x1C, 0x35, 0x56),
            success: Color32::from_rgb(0x30, 0xD1, 0x58),
            warning: Color32::from_rgb(0xFF, 0xD6, 0x0A),
            adjust_handle: Color32::from_rgb(0xFF, 0xD6, 0x0A),
            danger: Color32::from_rgb(0xFF, 0x45, 0x3A),
        }
    }

    /// The light palette from the M5 design specification.
    #[must_use]
    pub const fn light() -> Self {
        Self {
            app: Color32::from_rgb(0xEC, 0xEC, 0xEE),
            canvas: Color32::from_rgb(0xE3, 0xE3, 0xE6),
            panel: Color32::from_rgb(0xF6, 0xF6, 0xF7),
            raised: Color32::from_rgb(0xFF, 0xFF, 0xFF),
            input: Color32::from_rgb(0xFF, 0xFF, 0xFF),
            hover: Color32::from_rgb(0xE9, 0xE9, 0xEC),
            pressed: Color32::from_rgb(0xDE, 0xDE, 0xE2),
            border: Color32::from_rgb(0xE1, 0xE1, 0xE4),
            border_strong: Color32::from_rgb(0xC9, 0xC9, 0xCE),
            text: Color32::from_rgb(0x1D, 0x1D, 0x1F),
            text_secondary: Color32::from_rgb(0x5E, 0x5E, 0x66),
            text_tertiary: Color32::from_rgb(0x6E, 0x6E, 0x75),
            text_disabled: Color32::from_rgb(0xB4, 0xB4, 0xBA),
            accent: Color32::from_rgb(0x00, 0x66, 0xE0),
            accent_hover: Color32::from_rgb(0x1A, 0x75, 0xE6),
            accent_pressed: Color32::from_rgb(0x00, 0x55, 0xBD),
            accent_text: Color32::from_rgb(0x00, 0x66, 0xE0),
            accent_subtle: Color32::from_rgb(0xDC, 0xEB, 0xFF),
            success: Color32::from_rgb(0x1F, 0x9D, 0x44),
            warning: Color32::from_rgb(0xB3, 0x6B, 0x00),
            adjust_handle: Color32::from_rgb(0xB3, 0x6B, 0x00),
            danger: Color32::from_rgb(0xD7, 0x00, 0x15),
        }
    }

    /// Return the palette corresponding to `mode`.
    #[must_use]
    pub const fn for_mode(mode: ThemeMode) -> Self {
        match mode {
            ThemeMode::Dark => Self::dark(),
            ThemeMode::Light => Self::light(),
        }
    }
}

// ----------------------------------------------------------------------------
// Spacing scale tokens (4 pt base)
// ----------------------------------------------------------------------------

pub const SPACE_XS: f32 = 2.0;
pub const SPACE_SM: f32 = 4.0;
pub const SPACE_6: f32 = 6.0;
pub const SPACE_MD: f32 = 8.0;
pub const SPACE_LG: f32 = 12.0;
pub const SPACE_XL: f32 = 16.0;
pub const SPACE_20: f32 = 20.0;
pub const SPACE_24: f32 = 24.0;
pub const SPACE_32: f32 = 32.0;
pub const SPACE_48: f32 = 48.0;

/// Clear alias module for spacing scale tokens.
pub mod space {
    pub const S2: f32 = 2.0;
    pub const S4: f32 = 4.0;
    pub const S6: f32 = 6.0;
    pub const S8: f32 = 8.0;
    pub const S12: f32 = 12.0;
    pub const S16: f32 = 16.0;
    pub const S20: f32 = 20.0;
    pub const S24: f32 = 24.0;
    pub const S32: f32 = 32.0;
    pub const S48: f32 = 48.0;
}

// ----------------------------------------------------------------------------
// Corner radii tokens
// ----------------------------------------------------------------------------

/// Radius for checkboxes, small status chips, and timeline bars (4 px).
pub const RADIUS_SM: f32 = 4.0;
/// Radius for standard buttons, text inputs, and thumbnails (6 px).
pub const RADIUS_MD: f32 = 6.0;
/// Radius for popovers, menus, and cards (8 px).
pub const RADIUS_LG: f32 = 8.0;
/// Radius for dialogs and welcome cards (12 px).
pub const RADIUS_XL: f32 = 12.0;

// ----------------------------------------------------------------------------
// Control heights & layout tokens
// ----------------------------------------------------------------------------

/// Neutral ground behind the letterboxed course in the learner player (#0E0E10).
pub const PLAYER_GROUND: Color32 = Color32::from_rgb(0x0E, 0x0E, 0x10);

/// Compact icon size (16 px).
pub const ICON_SM: f32 = 16.0;
/// Toolbar icon size (18 px).
pub const ICON_TOOLBAR: f32 = 18.0;
/// Compact control height for inspector fields and timeline items (26 px).
pub const CONTROL_COMPACT: f32 = 26.0;
/// Toolbar button height (30 px).
pub const CONTROL_TOOLBAR: f32 = 30.0;
/// Primary dialog button height (32 px).
pub const CONTROL_PRIMARY: f32 = 32.0;
/// Standard panel inner padding / margin (12 px).
pub const PANEL_MARGIN: f32 = 12.0;

// ----------------------------------------------------------------------------
// Typography size tokens (points)
// ----------------------------------------------------------------------------

pub const SIZE_DISPLAY: f32 = 28.0;
pub const SIZE_HEADING: f32 = 20.0;
pub const SIZE_TITLE: f32 = 15.0;
pub const SIZE_BODY: f32 = 13.0;
pub const SIZE_BODY_STRONG: f32 = 13.0;
pub const SIZE_CAPTION: f32 = 11.0;
pub const SIZE_SMALL: f32 = 11.0;
pub const SIZE_MONO: f32 = 12.0;

// ----------------------------------------------------------------------------
// Shadow constructors
// ----------------------------------------------------------------------------

/// Drop shadow for popovers and popup menus.
#[must_use]
pub fn popover_shadow(mode: ThemeMode) -> Shadow {
    let color = match mode {
        ThemeMode::Dark => Color32::from_rgba_unmultiplied(0, 0, 0, 0x66),
        ThemeMode::Light => Color32::from_rgba_unmultiplied(0, 0, 0, 0x2A),
    };
    Shadow {
        offset: [0, 6],
        blur: 20,
        spread: 0,
        color,
    }
}

/// Drop shadow for modal and floating dialogs.
#[must_use]
pub fn dialog_shadow(mode: ThemeMode) -> Shadow {
    let color = match mode {
        ThemeMode::Dark => Color32::from_rgba_unmultiplied(0, 0, 0, 0x66),
        ThemeMode::Light => Color32::from_rgba_unmultiplied(0, 0, 0, 0x2A),
    };
    Shadow {
        offset: [0, 12],
        blur: 32,
        spread: 0,
        color,
    }
}

/// Drop shadow for the slide on the canvas ground.
#[must_use]
pub fn slide_shadow(mode: ThemeMode) -> Shadow {
    let color = match mode {
        ThemeMode::Dark => Color32::from_rgba_unmultiplied(0, 0, 0, 0x80),
        ThemeMode::Light => Color32::from_rgba_unmultiplied(0, 0, 0, 0x33),
    };
    Shadow {
        offset: [0, 8],
        blur: 24,
        spread: 0,
        color,
    }
}

// ----------------------------------------------------------------------------
// Named text style helpers
// ----------------------------------------------------------------------------

/// Caption text (11 pt semibold, section header in `text_secondary`).
#[must_use]
pub fn caption(text: impl Into<String>) -> RichText {
    RichText::new(text)
        .text_style(TextStyle::Name("caption".into()))
        .weak()
}

/// Title text (15 pt semibold, panel headers and inspector content headers).
#[must_use]
pub fn title(text: impl Into<String>) -> RichText {
    RichText::new(text).text_style(TextStyle::Name("title".into()))
}

/// Heading text (20 pt semibold, dialog titles, empty-state titles).
#[must_use]
pub fn heading(text: impl Into<String>) -> RichText {
    RichText::new(text).text_style(TextStyle::Heading)
}

/// Display text (28 pt semibold, welcome screen title).
#[must_use]
pub fn display(text: impl Into<String>) -> RichText {
    RichText::new(text).text_style(TextStyle::Name("display".into()))
}

/// Small text (11 pt regular, hints, metadata, thumbnail numbers).
#[must_use]
pub fn small(text: impl Into<String>) -> RichText {
    RichText::new(text).text_style(TextStyle::Small)
}

/// Body text (13 pt regular, standard controls and labels).
#[must_use]
pub fn body(text: impl Into<String>) -> RichText {
    RichText::new(text).text_style(TextStyle::Body)
}

/// Body strong text (13 pt semibold, emphasised labels).
#[must_use]
pub fn body_strong(text: impl Into<String>) -> RichText {
    RichText::new(text).text_style(TextStyle::Name("body_strong".into()))
}

// ----------------------------------------------------------------------------
// Apply style & palette access
// ----------------------------------------------------------------------------

const PALETTE_ID: &str = "coursecraft-palette";
const THEME_MODE_ID: &str = "coursecraft-theme-mode";

/// Retrieve the active [`Palette`] from `ctx` data, defaulting to dark mode.
#[must_use]
pub fn palette(ctx: &egui::Context) -> Palette {
    ctx.data(|d| d.get_temp(Id::unique(PALETTE_ID)))
        .unwrap_or_else(Palette::dark)
}

/// Retrieve the active [`ThemeMode`] from `ctx` data, defaulting to dark mode.
#[must_use]
pub fn theme_mode(ctx: &egui::Context) -> ThemeMode {
    ctx.data(|d| d.get_temp(Id::unique(THEME_MODE_ID)))
        .unwrap_or(ThemeMode::Dark)
}

/// Build and apply the CourseCraft theme style onto an [`egui::Context`].
pub fn apply_style(ctx: &egui::Context, mode: ThemeMode) {
    install_fonts(ctx);

    let palette = Palette::for_mode(mode);
    let is_dark = mode == ThemeMode::Dark;

    // 2-3% tint of panel for zebra alternating rows
    let faint_bg_color = match mode {
        ThemeMode::Dark => Color32::from_rgb(0x2B, 0x2B, 0x2F),
        ThemeMode::Light => Color32::from_rgb(0xEE, 0xEE, 0xF0),
    };

    let visuals = egui::Visuals {
        dark_mode: is_dark,
        override_text_color: None,
        widgets: egui::style::Widgets {
            noninteractive: egui::style::WidgetVisuals {
                bg_fill: palette.panel,
                weak_bg_fill: palette.panel,
                bg_stroke: Stroke::new(1.0, palette.border),
                corner_radius: CornerRadius::same(RADIUS_MD as u8),
                fg_stroke: Stroke::new(1.0, palette.text_secondary),
                expansion: 0.0,
            },
            inactive: egui::style::WidgetVisuals {
                bg_fill: palette.input,
                weak_bg_fill: palette.pressed,
                bg_stroke: Stroke::NONE,
                corner_radius: CornerRadius::same(RADIUS_MD as u8),
                fg_stroke: Stroke::new(1.0, palette.text),
                expansion: 0.0,
            },
            hovered: egui::style::WidgetVisuals {
                bg_fill: palette.hover,
                weak_bg_fill: palette.hover,
                bg_stroke: Stroke::NONE,
                corner_radius: CornerRadius::same(RADIUS_MD as u8),
                fg_stroke: Stroke::new(1.5, palette.text),
                expansion: 0.0,
            },
            active: egui::style::WidgetVisuals {
                bg_fill: palette.pressed,
                weak_bg_fill: palette.pressed,
                bg_stroke: Stroke::NONE,
                corner_radius: CornerRadius::same(RADIUS_MD as u8),
                fg_stroke: Stroke::new(2.0, palette.text),
                expansion: 0.0,
            },
            open: egui::style::WidgetVisuals {
                bg_fill: palette.pressed,
                weak_bg_fill: palette.pressed,
                bg_stroke: Stroke::new(1.0, palette.border_strong),
                corner_radius: CornerRadius::same(RADIUS_MD as u8),
                fg_stroke: Stroke::new(1.0, palette.text),
                expansion: 0.0,
            },
        },
        selection: egui::style::Selection {
            bg_fill: palette.accent_subtle,
            stroke: Stroke::new(1.0, palette.accent_text),
        },
        hyperlink_color: palette.accent_text,
        faint_bg_color,
        extreme_bg_color: palette.input,
        code_bg_color: palette.input,
        warn_fg_color: palette.warning,
        error_fg_color: palette.danger,
        window_corner_radius: CornerRadius::same(RADIUS_XL as u8),
        window_shadow: dialog_shadow(mode),
        window_fill: palette.raised,
        window_stroke: Stroke::new(1.0, palette.border),
        window_highlight_topmost: false,
        menu_corner_radius: CornerRadius::same(RADIUS_LG as u8),
        panel_fill: palette.panel,
        popup_shadow: popover_shadow(mode),
        text_cursor: egui::style::TextCursorStyle {
            stroke: Stroke::new(2.0, palette.accent_text),
            ..Default::default()
        },
        weak_text_color: Some(palette.text_secondary),
        ..egui::Visuals::default()
    };

    let spacing = egui::Spacing {
        item_spacing: vec2(8.0, 6.0),
        button_padding: vec2(10.0, 5.0),
        interact_size: vec2(40.0, CONTROL_COMPACT),
        indent: 16.0,
        scroll: egui::style::ScrollStyle::floating(),
        window_margin: Margin::same(PANEL_MARGIN as i8),
        menu_margin: Margin::same(6),
        ..egui::Spacing::default()
    };

    let inter_regular = FontFamily::Name("inter".into());
    // Chrome uses medium and semibold; bold stays for slide text, where authors ask for it.
    let inter_medium = FontFamily::Name("inter-medium".into());
    let inter_semibold = FontFamily::Name("inter-semibold".into());

    let mut text_styles = BTreeMap::new();
    text_styles.insert(
        TextStyle::Heading,
        FontId::new(SIZE_HEADING, inter_semibold.clone()),
    );
    text_styles.insert(
        TextStyle::Body,
        FontId::new(SIZE_BODY, inter_regular.clone()),
    );
    text_styles.insert(
        TextStyle::Monospace,
        FontId::new(SIZE_MONO, FontFamily::Monospace),
    );
    text_styles.insert(TextStyle::Button, FontId::new(SIZE_BODY, inter_medium));
    text_styles.insert(
        TextStyle::Small,
        FontId::new(SIZE_SMALL, inter_regular.clone()),
    );
    text_styles.insert(
        TextStyle::Name("display".into()),
        FontId::new(SIZE_DISPLAY, inter_semibold.clone()),
    );
    text_styles.insert(
        TextStyle::Name("heading".into()),
        FontId::new(SIZE_HEADING, inter_semibold.clone()),
    );
    text_styles.insert(
        TextStyle::Name("title".into()),
        FontId::new(SIZE_TITLE, inter_semibold.clone()),
    );
    text_styles.insert(
        TextStyle::Name("caption".into()),
        FontId::new(SIZE_CAPTION, inter_semibold.clone()),
    );
    text_styles.insert(
        TextStyle::Name("small".into()),
        FontId::new(SIZE_SMALL, inter_regular),
    );
    text_styles.insert(
        TextStyle::Name("body_strong".into()),
        FontId::new(SIZE_BODY_STRONG, inter_semibold),
    );

    ctx.all_styles_mut(|style| {
        style.visuals = visuals.clone();
        style.spacing = spacing.clone();
        style.text_styles = text_styles.clone();
        style.animation_time = 0.12;
    });

    ctx.data_mut(|d| {
        d.insert_temp(Id::unique(PALETTE_ID), palette);
        d.insert_temp(Id::unique(THEME_MODE_ID), mode);
    });
}

// ----------------------------------------------------------------------------
// WCAG contrast ratio calculations
// ----------------------------------------------------------------------------

/// Calculate relative luminance for sRGB [`Color32`] according to WCAG 2.1 specs.
#[must_use]
pub fn relative_luminance(c: Color32) -> f32 {
    let to_linear = |v: u8| -> f32 {
        let s = v as f32 / 255.0;
        if s <= 0.04045 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    let r = to_linear(c.r());
    let g = to_linear(c.g());
    let b = to_linear(c.b());
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// Compute WCAG 2.1 contrast ratio between two sRGB colours (range 1.0..=21.0).
#[must_use]
pub fn contrast_ratio(a: Color32, b: Color32) -> f32 {
    let l1 = relative_luminance(a);
    let l2 = relative_luminance(b);
    let (lighter, darker) = if l1 >= l2 { (l1, l2) } else { (l2, l1) };
    (lighter + 0.05) / (darker + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_palette_matches_spec_hex_values() {
        let dark = Palette::dark();
        assert_eq!(dark.app, Color32::from_rgb(0x1C, 0x1C, 0x1E));
        assert_eq!(dark.canvas, Color32::from_rgb(0x16, 0x16, 0x18));
        assert_eq!(dark.panel, Color32::from_rgb(0x25, 0x25, 0x28));
        assert_eq!(dark.raised, Color32::from_rgb(0x2F, 0x2F, 0x33));
        assert_eq!(dark.input, Color32::from_rgb(0x1E, 0x1E, 0x21));
        assert_eq!(dark.hover, Color32::from_rgb(0x32, 0x32, 0x36));
        assert_eq!(dark.pressed, Color32::from_rgb(0x3A, 0x3A, 0x3F));
        assert_eq!(dark.border, Color32::from_rgb(0x36, 0x36, 0x3A));
        assert_eq!(dark.border_strong, Color32::from_rgb(0x48, 0x48, 0x4E));
        assert_eq!(dark.text, Color32::from_rgb(0xF2, 0xF2, 0xF5));
        assert_eq!(dark.text_secondary, Color32::from_rgb(0xA8, 0xA8, 0xB0));
        assert_eq!(dark.text_tertiary, Color32::from_rgb(0x8E, 0x8E, 0x96));
        assert_eq!(dark.text_disabled, Color32::from_rgb(0x55, 0x55, 0x5B));
        assert_eq!(dark.accent, Color32::from_rgb(0x0A, 0x6C, 0xFF));
        assert_eq!(dark.accent_hover, Color32::from_rgb(0x2B, 0x80, 0xFF));
        assert_eq!(dark.accent_pressed, Color32::from_rgb(0x00, 0x58, 0xD6));
        assert_eq!(dark.accent_text, Color32::from_rgb(0x4D, 0x9D, 0xFF));
        assert_eq!(dark.accent_subtle, Color32::from_rgb(0x1C, 0x35, 0x56));
        assert_eq!(dark.success, Color32::from_rgb(0x30, 0xD1, 0x58));
        assert_eq!(dark.warning, Color32::from_rgb(0xFF, 0xD6, 0x0A));
        assert_eq!(dark.adjust_handle, Color32::from_rgb(0xFF, 0xD6, 0x0A));
        assert_eq!(dark.danger, Color32::from_rgb(0xFF, 0x45, 0x3A));
    }

    #[test]
    fn light_palette_matches_spec_hex_values() {
        let light = Palette::light();
        assert_eq!(light.app, Color32::from_rgb(0xEC, 0xEC, 0xEE));
        assert_eq!(light.canvas, Color32::from_rgb(0xE3, 0xE3, 0xE6));
        assert_eq!(light.panel, Color32::from_rgb(0xF6, 0xF6, 0xF7));
        assert_eq!(light.raised, Color32::from_rgb(0xFF, 0xFF, 0xFF));
        assert_eq!(light.input, Color32::from_rgb(0xFF, 0xFF, 0xFF));
        assert_eq!(light.hover, Color32::from_rgb(0xE9, 0xE9, 0xEC));
        assert_eq!(light.pressed, Color32::from_rgb(0xDE, 0xDE, 0xE2));
        assert_eq!(light.border, Color32::from_rgb(0xE1, 0xE1, 0xE4));
        assert_eq!(light.border_strong, Color32::from_rgb(0xC9, 0xC9, 0xCE));
        assert_eq!(light.text, Color32::from_rgb(0x1D, 0x1D, 0x1F));
        assert_eq!(light.text_secondary, Color32::from_rgb(0x5E, 0x5E, 0x66));
        assert_eq!(light.text_tertiary, Color32::from_rgb(0x6E, 0x6E, 0x75));
        assert_eq!(light.text_disabled, Color32::from_rgb(0xB4, 0xB4, 0xBA));
        assert_eq!(light.accent, Color32::from_rgb(0x00, 0x66, 0xE0));
        assert_eq!(light.accent_hover, Color32::from_rgb(0x1A, 0x75, 0xE6));
        assert_eq!(light.accent_pressed, Color32::from_rgb(0x00, 0x55, 0xBD));
        assert_eq!(light.accent_text, Color32::from_rgb(0x00, 0x66, 0xE0));
        assert_eq!(light.accent_subtle, Color32::from_rgb(0xDC, 0xEB, 0xFF));
        assert_eq!(light.success, Color32::from_rgb(0x1F, 0x9D, 0x44));
        assert_eq!(light.warning, Color32::from_rgb(0xB3, 0x6B, 0x00));
        assert_eq!(light.adjust_handle, Color32::from_rgb(0xB3, 0x6B, 0x00));
        assert_eq!(light.danger, Color32::from_rgb(0xD7, 0x00, 0x15));
    }

    #[test]
    fn wcag_contrast_requirements_dark_and_light() {
        for mode in [ThemeMode::Dark, ThemeMode::Light] {
            let p = Palette::for_mode(mode);
            for surface in [p.panel, p.raised, p.app] {
                let text_cr = contrast_ratio(p.text, surface);
                assert!(
                    text_cr >= 4.5,
                    "text on surface failed AA: {text_cr:.2} (mode: {mode:?})"
                );

                let sec_cr = contrast_ratio(p.text_secondary, surface);
                assert!(
                    sec_cr >= 4.5,
                    "text_secondary on surface failed AA: {sec_cr:.2} (mode: {mode:?})"
                );
            }

            let tert_panel_cr = contrast_ratio(p.text_tertiary, p.panel);
            assert!(
                tert_panel_cr >= 4.5,
                "text_tertiary on panel failed AA: {tert_panel_cr:.2} (mode: {mode:?})"
            );

            let white_accent_cr = contrast_ratio(Color32::WHITE, p.accent);
            assert!(
                white_accent_cr >= 4.5,
                "white on accent failed AA: {white_accent_cr:.2} (mode: {mode:?})"
            );

            let accent_text_panel_cr = contrast_ratio(p.accent_text, p.panel);
            assert!(
                accent_text_panel_cr >= 4.5,
                "accent_text on panel failed AA: {accent_text_panel_cr:.2} (mode: {mode:?})"
            );
        }
    }

    #[test]
    fn apply_style_and_palette_roundtrip() {
        let ctx = egui::Context::default();
        apply_style(&ctx, ThemeMode::Dark);
        assert_eq!(palette(&ctx), Palette::dark());
        assert_eq!(theme_mode(&ctx), ThemeMode::Dark);

        let style = ctx.style_of(egui::Theme::Dark);
        assert_eq!(style.visuals.panel_fill, Palette::dark().panel);
        assert_eq!(
            style.visuals.selection.bg_fill,
            Palette::dark().accent_subtle
        );

        apply_style(&ctx, ThemeMode::Light);
        assert_eq!(palette(&ctx), Palette::light());
        assert_eq!(theme_mode(&ctx), ThemeMode::Light);

        let style = ctx.style_of(egui::Theme::Light);
        assert_eq!(style.visuals.panel_fill, Palette::light().panel);
        assert_eq!(
            style.visuals.selection.bg_fill,
            Palette::light().accent_subtle
        );
    }
}
