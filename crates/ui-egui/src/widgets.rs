//! Icon button and toolbar widget primitives for CourseCraft.
//!
//! Provides the icon buttons and Keynote-style toolbar buttons specified in M5 step 2.

use coursecraft_render::{
    CONTROL_COMPACT, ICON_SM, ICON_TOOLBAR, Icon, RADIUS_MD, SIZE_SMALL, palette,
};
use egui::{
    Color32, CornerRadius, CursorIcon, FontFamily, FontId, Response, Sense, Ui, pos2, vec2,
};

/// Tone for icon buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IconTone {
    /// `text_secondary` at rest, `text` on hover/press.
    #[default]
    Secondary,
    /// `accent_text` at rest and on hover.
    Accent,
    /// `text_secondary` at rest, `danger` on hover.
    Danger,
}

/// A compact 26×26 icon button.
///
/// Quiet at rest (no background fill), `hover` fill on hover, and `pressed` fill when pressed.
/// Displays the icon in `text_secondary`, turning `text` on hover.
pub fn icon_button(ui: &mut Ui, icon: Icon, tooltip: &str) -> Response {
    icon_button_impl(ui, icon, false, IconTone::Secondary, true, tooltip)
}

/// A compact 26×26 icon button with explicit enabled state.
pub fn icon_button_enabled(ui: &mut Ui, icon: Icon, enabled: bool, tooltip: &str) -> Response {
    icon_button_impl(ui, icon, false, IconTone::Secondary, enabled, tooltip)
}

/// A compact 26×26 icon button with danger tone on hover (e.g. delete actions).
pub fn icon_button_danger(ui: &mut Ui, icon: Icon, enabled: bool, tooltip: &str) -> Response {
    icon_button_impl(ui, icon, false, IconTone::Danger, enabled, tooltip)
}

/// A compact 26×26 icon toggle button.
///
/// When active, uses the filled icon in `accent_text` on `accent_subtle` background fill.
pub fn icon_toggle(ui: &mut Ui, icon: Icon, active: bool, tooltip: &str) -> Response {
    icon_button_impl(ui, icon, active, IconTone::Accent, true, tooltip)
}

/// How long a control takes to fade between its rest, hover and active fills.
const HOVER_FADE_SECONDS: f32 = 0.1;

/// The fill for a quiet control, fading between rest, hover and active. Pressing is instant.
fn state_fill(
    ui: &Ui,
    resp: &Response,
    p: &coursecraft_render::Palette,
    hovered: bool,
    pressed: bool,
    active: bool,
) -> Color32 {
    let ctx = ui.ctx();
    let hover = ctx.animate_bool_with_time(resp.id.with("hover"), hovered, HOVER_FADE_SECONDS);
    let on = ctx.animate_bool_with_time(resp.id.with("active"), active, HOVER_FADE_SECONDS);
    let rest = Color32::TRANSPARENT.lerp_to_gamma(p.hover, hover);
    if pressed && !active {
        p.pressed
    } else {
        rest.lerp_to_gamma(p.accent_subtle, on)
    }
}

fn icon_button_impl(
    ui: &mut Ui,
    icon: Icon,
    active: bool,
    tone: IconTone,
    enabled: bool,
    tooltip: &str,
) -> Response {
    let p = palette(ui.ctx());
    let size = vec2(CONTROL_COMPACT, CONTROL_COMPACT);
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(size, sense);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, tooltip.to_owned()));

    if ui.is_rect_visible(rect) {
        let radius = CornerRadius::same(RADIUS_MD as u8);
        let hovered = enabled && resp.hovered();
        let pressed = enabled && resp.is_pointer_button_down_on();

        ui.painter().rect_filled(
            rect,
            radius,
            state_fill(ui, &resp, &p, hovered, pressed, active),
        );

        // Icon color
        let icon_color = if !enabled {
            p.text_disabled
        } else if active {
            p.accent_text
        } else {
            match tone {
                IconTone::Danger if hovered => p.danger,
                IconTone::Accent => p.accent_text,
                _ if hovered || pressed => p.text,
                _ => p.text_secondary,
            }
        };

        let font_id = FontId::new(
            ICON_SM,
            if active {
                FontFamily::Name("phosphor-fill".into())
            } else {
                FontFamily::Name("phosphor".into())
            },
        );
        let galley = ui
            .painter()
            .layout_no_wrap(icon.codepoint().to_string(), font_id, icon_color);
        let pos = pos2(
            rect.center().x - galley.size().x * 0.5,
            rect.center().y - galley.size().y * 0.5,
        );
        ui.painter().galley(pos, galley, icon_color);
    }

    let resp = resp.on_hover_text(tooltip);
    if enabled {
        resp.on_hover_cursor(CursorIcon::PointingHand)
    } else {
        resp
    }
}

/// A Keynote-style toolbar button with an 18 px icon above an 11 pt label.
///
/// 44 px tall, at least 52 px wide (or fits label), quiet at rest with `hover` fill on hover.
/// Below ≈1100 px window width, drops the label and shows icon-only with tooltip.
pub fn toolbar_button(ui: &mut Ui, icon: Icon, label: &str, tooltip: &str) -> Response {
    toolbar_button_impl(ui, icon, label, false, true, tooltip)
}

/// A Keynote-style toolbar toggle button.
///
/// When active, uses accent colours (`accent_text` on `accent_subtle`).
pub fn toolbar_toggle(
    ui: &mut Ui,
    icon: Icon,
    label: &str,
    active: bool,
    tooltip: &str,
) -> Response {
    toolbar_button_impl(ui, icon, label, active, true, tooltip)
}

/// A Keynote-style toolbar button with active state.
pub fn toolbar_button_active(
    ui: &mut Ui,
    icon: Icon,
    label: &str,
    active: bool,
    tooltip: &str,
) -> Response {
    toolbar_button_impl(ui, icon, label, active, true, tooltip)
}

/// A Keynote-style toolbar button with explicit enabled and active states.
pub fn toolbar_button_enabled(
    ui: &mut Ui,
    icon: Icon,
    label: &str,
    active: bool,
    enabled: bool,
    tooltip: &str,
) -> Response {
    toolbar_button_impl(ui, icon, label, active, enabled, tooltip)
}

/// A Keynote-style toolbar button with an accent fill and white icon/label.
///
/// Used for the primary toolbar action (Publish). Below ≈1100 px window width it
/// drops the label and shows the icon alone, like the other toolbar buttons.
pub fn toolbar_primary_button(ui: &mut Ui, icon: Icon, label: &str, tooltip: &str) -> Response {
    let p = palette(ui.ctx());
    let show_label = ui.ctx().content_rect().width() >= 1100.0;
    let label_galley = if show_label {
        Some(ui.painter().layout_no_wrap(
            label.to_owned(),
            FontId::proportional(SIZE_SMALL),
            Color32::PLACEHOLDER,
        ))
    } else {
        None
    };
    let width = label_galley
        .as_ref()
        .map_or(38.0, |lg| (lg.size().x + 10.0).max(52.0));

    let (rect, resp) = ui.allocate_exact_size(vec2(width, 44.0), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, tooltip.to_owned()));

    if ui.is_rect_visible(rect) {
        let radius = CornerRadius::same(RADIUS_MD as u8);
        let hover = ui.ctx().animate_bool_with_time(
            resp.id.with("hover"),
            resp.hovered(),
            HOVER_FADE_SECONDS,
        );
        let fill = if resp.is_pointer_button_down_on() {
            p.accent_pressed
        } else {
            p.accent.lerp_to_gamma(p.accent_hover, hover)
        };
        ui.painter().rect_filled(rect, radius, fill);

        let icon_font = FontId::new(ICON_TOOLBAR, FontFamily::Name("phosphor".into()));
        let icon_galley =
            ui.painter()
                .layout_no_wrap(icon.codepoint().to_string(), icon_font, Color32::WHITE);
        if let Some(lg) = label_galley {
            ui.painter().galley(
                pos2(
                    rect.center().x - icon_galley.size().x * 0.5,
                    rect.top() + 4.0,
                ),
                icon_galley,
                Color32::WHITE,
            );
            ui.painter().galley(
                pos2(
                    rect.center().x - lg.size().x * 0.5,
                    rect.bottom() - lg.size().y - 4.0,
                ),
                lg,
                Color32::WHITE,
            );
        } else {
            ui.painter().galley(
                pos2(
                    rect.center().x - icon_galley.size().x * 0.5,
                    rect.center().y - icon_galley.size().y * 0.5,
                ),
                icon_galley,
                Color32::WHITE,
            );
        }
    }

    resp.on_hover_text(tooltip)
        .on_hover_cursor(CursorIcon::PointingHand)
}

/// A small caret button companion for toolbar dropdowns (e.g. next to Add Slide).
pub fn toolbar_caret_button(ui: &mut Ui, tooltip: &str) -> Response {
    let p = palette(ui.ctx());
    let size = vec2(16.0, 44.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, tooltip.to_owned()));

    if ui.is_rect_visible(rect) {
        let radius = CornerRadius::same(RADIUS_MD as u8);
        let hovered = resp.hovered();
        let pressed = resp.is_pointer_button_down_on();

        if pressed {
            ui.painter().rect_filled(rect, radius, p.pressed);
        } else if hovered {
            ui.painter().rect_filled(rect, radius, p.hover);
        }

        let icon_color = if hovered || pressed {
            p.text
        } else {
            p.text_secondary
        };

        let font_id = FontId::new(12.0, FontFamily::Name("phosphor".into()));
        let galley = ui.painter().layout_no_wrap(
            Icon::CaretDown.codepoint().to_string(),
            font_id,
            icon_color,
        );
        let pos = pos2(
            rect.center().x - galley.size().x * 0.5,
            rect.center().y - galley.size().y * 0.5,
        );
        ui.painter().galley(pos, galley, icon_color);
    }

    resp.on_hover_text(tooltip)
        .on_hover_cursor(CursorIcon::PointingHand)
}

fn toolbar_button_impl(
    ui: &mut Ui,
    icon: Icon,
    label: &str,
    active: bool,
    enabled: bool,
    tooltip: &str,
) -> Response {
    let p = palette(ui.ctx());
    // Drop labels and show icons only below ≈1100 px so nothing clips or overlaps at 1024.
    let show_label = ui.ctx().content_rect().width() >= 1100.0;

    let height = 44.0;
    let label_font = FontId::proportional(SIZE_SMALL);
    let label_galley = if show_label {
        Some(
            ui.painter()
                .layout_no_wrap(label.to_owned(), label_font, Color32::PLACEHOLDER),
        )
    } else {
        None
    };

    let width = if let Some(ref lg) = label_galley {
        (lg.size().x + 10.0).max(52.0)
    } else {
        38.0
    };

    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(vec2(width, height), sense);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, tooltip.to_owned()));

    if ui.is_rect_visible(rect) {
        let radius = CornerRadius::same(RADIUS_MD as u8);
        let hovered = enabled && resp.hovered();
        let pressed = enabled && resp.is_pointer_button_down_on();

        ui.painter().rect_filled(
            rect,
            radius,
            state_fill(ui, &resp, &p, hovered, pressed, active),
        );

        // Colors
        let (icon_color, text_color) = if !enabled {
            (p.text_disabled, p.text_disabled)
        } else if active {
            (p.accent_text, p.accent_text)
        } else if hovered || pressed {
            (p.text, p.text)
        } else {
            (p.text_secondary, p.text_secondary)
        };

        // Icon
        let icon_font = FontId::new(
            ICON_TOOLBAR,
            if active {
                FontFamily::Name("phosphor-fill".into())
            } else {
                FontFamily::Name("phosphor".into())
            },
        );
        let icon_galley =
            ui.painter()
                .layout_no_wrap(icon.codepoint().to_string(), icon_font, icon_color);

        if let Some(lg) = label_galley {
            let icon_pos = pos2(
                rect.center().x - icon_galley.size().x * 0.5,
                rect.top() + 4.0,
            );
            ui.painter().galley(icon_pos, icon_galley, icon_color);

            let label_pos = pos2(
                rect.center().x - lg.size().x * 0.5,
                rect.bottom() - lg.size().y - 4.0,
            );
            ui.painter().galley(label_pos, lg, text_color);
        } else {
            let icon_pos = pos2(
                rect.center().x - icon_galley.size().x * 0.5,
                rect.center().y - icon_galley.size().y * 0.5,
            );
            ui.painter().galley(icon_pos, icon_galley, icon_color);
        }
    }

    let resp = resp.on_hover_text(tooltip);
    if enabled {
        resp.on_hover_cursor(CursorIcon::PointingHand)
    } else {
        resp
    }
}
