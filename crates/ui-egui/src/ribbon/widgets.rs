//! Ribbon controls: large and small (split) buttons, icon toggles, combo fields and
//! the shared ▾ popup.
//!
//! Ported from DeckCraft `crates/ui-egui/src/widgets.rs` at commit 7fbdc085
//! (MIT OR Apache-2.0, same licence as CourseCraft), redrawn with CourseCraft's
//! palette (`coursecraft_render::palette`, DESIGN.md) and Phosphor icons.

use coursecraft_render::{Icon, Palette, palette};
use egui::{
    Align2, Color32, CornerRadius, FontFamily, FontId, Painter, Pos2, Rect, Response, Sense,
    Stroke, Ui, pos2, vec2,
};

/// Height of a group's control area (above its caption).
pub const CONTROLS_H: f32 = 66.0;
/// Height of a group's caption row.
pub const CAPTION_H: f32 = 16.0;
/// Height of a small control; three fit in [`CONTROLS_H`].
pub const SMALL_H: f32 = 22.0;

/// Width of the ▾ strip at the right of a small/toggle split control.
const SMALL_SPLIT_ARROW_W: f32 = 18.0;
const LABEL_SIZE: f32 = 11.5;
const SMALL_LABEL_SIZE: f32 = 12.0;
const LARGE_ICON: f32 = 26.0;
const SMALL_ICON: f32 = 16.0;

fn icon_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("phosphor".into()))
}

fn paint_icon(painter: &Painter, center: Pos2, icon: Icon, size: f32, color: Color32) {
    painter.text(
        center,
        Align2::CENTER_CENTER,
        icon.codepoint(),
        icon_font(size),
        color,
    );
}

fn text_width(ui: &Ui, text: &str, size: f32) -> f32 {
    ui.painter()
        .layout_no_wrap(
            text.to_owned(),
            FontId::proportional(size),
            Color32::PLACEHOLDER,
        )
        .size()
        .x
}

/// The fill behind a control in its current state, if any.
fn fill(ui: &Ui, r: &Response, selected: bool, p: &Palette) -> Option<Color32> {
    if !r.sense.senses_click() {
        return None;
    }
    if selected {
        Some(p.accent_subtle)
    } else if r.is_pointer_button_down_on() {
        Some(p.pressed)
    } else if r.hovered() && ui.is_enabled() {
        Some(p.hover)
    } else {
        None
    }
}

fn sense(enabled: bool) -> Sense {
    if enabled {
        Sense::click()
    } else {
        Sense::hover()
    }
}

/// A small ▾.
pub fn chevron(p: &Painter, c: Pos2, s: f32, col: Color32) {
    let stroke = Stroke::new(1.2, col);
    p.line_segment(
        [pos2(c.x - s, c.y - s * 0.5), pos2(c.x, c.y + s * 0.5)],
        stroke,
    );
    p.line_segment(
        [pos2(c.x, c.y + s * 0.5), pos2(c.x + s, c.y - s * 0.5)],
        stroke,
    );
}

/// Split a label over two lines near its middle word break, as large buttons do.
pub fn wrap_label(label: &str) -> Vec<&str> {
    let words: Vec<&str> = label.split(' ').collect();
    if words.len() >= 2 && label.chars().count() > 8 {
        let mut best = 1;
        let mut best_diff = usize::MAX;
        for k in 1..words.len() {
            let a: usize = words.iter().take(k).map(|w| w.chars().count() + 1).sum();
            let b: usize = words.iter().skip(k).map(|w| w.chars().count() + 1).sum();
            let d = a.abs_diff(b);
            if d < best_diff {
                best_diff = d;
                best = k;
            }
        }
        let split_at = words
            .iter()
            .take(best)
            .map(|w| w.len() + 1)
            .sum::<usize>()
            .saturating_sub(1);
        if let (Some(a), Some(b)) = (label.get(..split_at), label.get(split_at + 1..)) {
            return vec![a, b];
        }
    }
    vec![label]
}

/// A large button: icon over a one- or two-line label. With `split`, the top
/// (icon) half runs the main action and the bottom (label + ▾) half opens the
/// menu.
pub fn large(
    ui: &mut Ui,
    icon: Icon,
    label: &str,
    enabled: bool,
    split: bool,
    selected: bool,
) -> (Response, Option<Response>) {
    let p = palette(ui.ctx());
    let lines = wrap_label(label);
    let text_w = lines
        .iter()
        .map(|l| text_width(ui, l, LABEL_SIZE))
        .fold(0.0_f32, f32::max);
    let col = if enabled { p.text } else { p.text_disabled };
    let mut job = egui::text::LayoutJob::simple(
        lines.join("\n"),
        FontId::proportional(LABEL_SIZE),
        col,
        f32::INFINITY,
    );
    job.halign = egui::Align::Center;
    let galley = ui.painter().layout_job(job);

    if !split {
        let w = (text_w + 10.0).max(44.0);
        let (rect, resp) = ui.allocate_exact_size(vec2(w, CONTROLS_H - 2.0), sense(enabled));
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, label));
        let painter = ui.painter_at(rect.expand(1.0));
        let radius = CornerRadius::same(coursecraft_render::RADIUS_MD as u8);
        if let Some(c) = fill(ui, &resp, selected, &p) {
            painter.rect_filled(rect, radius, c);
        }
        paint_icon(
            &painter,
            pos2(rect.center().x, rect.min.y + 18.0),
            icon,
            LARGE_ICON,
            col,
        );
        painter.galley(pos2(rect.center().x, rect.min.y + 37.0), galley, col);
        return (resp, None);
    }

    let line2_w = if lines.len() >= 2 {
        text_width(ui, lines[1], LABEL_SIZE)
    } else {
        0.0
    };
    let w = (text_w + 10.0)
        .max(if lines.len() >= 2 {
            line2_w + 22.0
        } else {
            0.0
        })
        .max(44.0);
    let (rect, outer) = ui.allocate_exact_size(vec2(w, CONTROLS_H - 2.0), Sense::hover());
    let split_y = rect.min.y + 34.0;
    let main_rect = Rect::from_min_max(rect.min, pos2(rect.max.x, split_y));
    let arrow_rect = Rect::from_min_max(pos2(rect.min.x, split_y), rect.max);
    let main = ui.interact(main_rect, outer.id.with("main"), sense(enabled));
    main.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, label));
    let arrow = ui.interact(arrow_rect, outer.id.with("arrow"), sense(enabled));
    arrow.widget_info(|| {
        egui::WidgetInfo::labeled(egui::Role::Button, enabled, format!("{label} options"))
    });

    let painter = ui.painter_at(rect.expand(1.0));
    let r = coursecraft_render::RADIUS_MD as u8;
    let top_radius = CornerRadius {
        nw: r,
        ne: r,
        sw: 0,
        se: 0,
    };
    let bot_radius = CornerRadius {
        nw: 0,
        ne: 0,
        sw: r,
        se: r,
    };
    if let Some(c) = fill(ui, &main, selected, &p) {
        painter.rect_filled(main_rect, top_radius, c);
    }
    if let Some(c) = fill(ui, &arrow, false, &p) {
        painter.rect_filled(arrow_rect, bot_radius, c);
    }
    if enabled && ui.is_enabled() && (main.hovered() || arrow.hovered()) {
        painter.line_segment(
            [pos2(rect.min.x, split_y), pos2(rect.max.x, split_y)],
            Stroke::new(1.0, p.border),
        );
    }
    paint_icon(
        &painter,
        pos2(rect.center().x, rect.min.y + 17.5),
        icon,
        LARGE_ICON,
        col,
    );
    let text_center = rect.center().x;
    let text_y = rect.min.y + 36.0;
    painter.galley(pos2(text_center, text_y), galley, col);
    if lines.len() >= 2 {
        let chevron_x = text_center + line2_w * 0.5 + 5.0;
        let chevron_y = rect.min.y + 56.0;
        chevron(&painter, pos2(chevron_x, chevron_y), 3.0, col);
    } else {
        chevron(&painter, pos2(text_center, rect.min.y + 56.0), 3.0, col);
    }
    (main, Some(arrow))
}

/// A small button: icon and label in one row, optionally with a ▾.
pub fn small(
    ui: &mut Ui,
    icon: Icon,
    label: &str,
    enabled: bool,
    selected: bool,
    split: bool,
) -> (Response, Option<Response>) {
    let p = palette(ui.ctx());
    let arrow_w = if split { SMALL_SPLIT_ARROW_W } else { 0.0 };
    let w = 24.0 + text_width(ui, label, SMALL_LABEL_SIZE) + 6.0 + arrow_w;
    let (rect, outer) = ui.allocate_exact_size(vec2(w, SMALL_H), Sense::hover());
    let main_rect = Rect::from_min_max(rect.min, pos2(rect.max.x - arrow_w, rect.max.y));
    let main = ui.interact(main_rect, outer.id.with("main"), sense(enabled));
    main.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, label));
    let arrow = split.then(|| {
        let r = Rect::from_min_max(pos2(rect.max.x - arrow_w, rect.min.y), rect.max);
        let a = ui.interact(r, outer.id.with("arrow"), sense(enabled));
        a.widget_info(|| {
            egui::WidgetInfo::labeled(egui::Role::Button, enabled, format!("{label} options"))
        });
        a
    });
    let painter = ui.painter_at(rect);
    let r = coursecraft_render::RADIUS_SM as u8;
    let main_radius = if split {
        CornerRadius {
            nw: r,
            ne: 0,
            sw: r,
            se: 0,
        }
    } else {
        CornerRadius::same(r)
    };
    let arrow_radius = CornerRadius {
        nw: 0,
        ne: r,
        sw: 0,
        se: r,
    };
    if let Some(c) = fill(ui, &main, selected, &p) {
        painter.rect_filled(main_rect, main_radius, c);
    }
    if let Some(a) = &arrow
        && let Some(c) = fill(ui, a, false, &p)
    {
        painter.rect_filled(a.rect, arrow_radius, c);
    }
    if let Some(a) = &arrow
        && enabled
        && ui.is_enabled()
        && (main.hovered() || a.hovered())
    {
        let x = main_rect.max.x;
        painter.line_segment(
            [pos2(x, rect.min.y + 2.0), pos2(x, rect.max.y - 2.0)],
            Stroke::new(1.0, p.border),
        );
    }
    let col = if enabled { p.text } else { p.text_disabled };
    paint_icon(
        &painter,
        pos2(rect.min.x + 12.0, rect.center().y),
        icon,
        SMALL_ICON,
        col,
    );
    painter.text(
        pos2(rect.min.x + 24.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(SMALL_LABEL_SIZE),
        col,
    );
    if split {
        chevron(
            &painter,
            pos2(rect.max.x - arrow_w * 0.5, rect.center().y),
            2.8,
            p.text_secondary,
        );
    }
    (main, arrow)
}

/// A square icon toggle (Bold, Align Left…), named by `name` for screen readers.
pub fn toggle(ui: &mut Ui, icon: Icon, name: &str, enabled: bool, on: bool) -> Response {
    let p = palette(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(vec2(24.0, SMALL_H), sense(enabled));
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, enabled, on, name));
    let painter = ui.painter_at(rect);
    if let Some(c) = fill(ui, &resp, on, &p) {
        painter.rect_filled(
            rect,
            CornerRadius::same(coursecraft_render::RADIUS_SM as u8),
            c,
        );
    }
    let col = if enabled { p.text } else { p.text_disabled };
    paint_icon(&painter, rect.center(), icon, SMALL_ICON, col);
    resp
}

/// An icon toggle with a ▾ strip: the main part toggles, the strip opens a menu.
pub fn toggle_split(
    ui: &mut Ui,
    icon: Icon,
    name: &str,
    enabled: bool,
    on: bool,
) -> (Response, Response) {
    toggle_split_bar(ui, icon, name, enabled, on, None)
}

/// A [`toggle_split`] with a thin bar under the icon showing the colour the main part applies.
pub fn toggle_split_bar(
    ui: &mut Ui,
    icon: Icon,
    name: &str,
    enabled: bool,
    on: bool,
    bar: Option<Color32>,
) -> (Response, Response) {
    let p = palette(ui.ctx());
    let arrow_w = SMALL_SPLIT_ARROW_W;
    let (rect, outer) = ui.allocate_exact_size(vec2(24.0 + arrow_w, SMALL_H), Sense::hover());
    let main_rect = Rect::from_min_max(rect.min, pos2(rect.max.x - arrow_w, rect.max.y));
    let arrow_rect = Rect::from_min_max(pos2(rect.max.x - arrow_w, rect.min.y), rect.max);
    let main = ui.interact(main_rect, outer.id.with("main"), sense(enabled));
    main.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, enabled, on, name));
    let arrow = ui.interact(arrow_rect, outer.id.with("arrow"), sense(enabled));
    arrow.widget_info(|| {
        egui::WidgetInfo::labeled(egui::Role::Button, enabled, format!("{name} options"))
    });
    let painter = ui.painter_at(rect);
    let r = coursecraft_render::RADIUS_SM as u8;
    let main_radius = CornerRadius {
        nw: r,
        ne: 0,
        sw: r,
        se: 0,
    };
    let arrow_radius = CornerRadius {
        nw: 0,
        ne: r,
        sw: 0,
        se: r,
    };
    if let Some(c) = fill(ui, &main, on, &p) {
        painter.rect_filled(main_rect, main_radius, c);
    }
    if let Some(c) = fill(ui, &arrow, false, &p) {
        painter.rect_filled(arrow_rect, arrow_radius, c);
    }
    if enabled && ui.is_enabled() && (main.hovered() || arrow.hovered()) {
        let x = main_rect.max.x;
        painter.line_segment(
            [pos2(x, rect.min.y + 2.0), pos2(x, rect.max.y - 2.0)],
            Stroke::new(1.0, p.border),
        );
    }
    let col = if enabled { p.text } else { p.text_disabled };
    let icon_at = if bar.is_some() {
        main_rect.center() - vec2(0.0, 2.0)
    } else {
        main_rect.center()
    };
    paint_icon(&painter, icon_at, icon, SMALL_ICON, col);
    if let Some(color) = bar {
        let color = if enabled {
            color
        } else {
            color.gamma_multiply(0.4)
        };
        let bar_rect = Rect::from_min_max(
            pos2(main_rect.min.x + 4.0, main_rect.max.y - 5.0),
            pos2(main_rect.max.x - 4.0, main_rect.max.y - 2.0),
        );
        painter.rect_filled(bar_rect, CornerRadius::same(1), color);
        painter.rect_stroke(
            bar_rect,
            CornerRadius::same(1),
            Stroke::new(1.0, p.border),
            egui::StrokeKind::Outside,
        );
    }
    chevron(
        &painter,
        pos2(arrow_rect.center().x, rect.center().y),
        2.4,
        p.text_secondary,
    );
    (main, arrow)
}

/// A text field with a ▾ (font family, size) showing `text`.
pub fn combo_field(ui: &mut Ui, name: &str, text: &str, width: f32, enabled: bool) -> Response {
    let p = palette(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(vec2(width, SMALL_H), sense(enabled));
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, enabled, name));
    let painter = ui.painter_at(rect);
    let radius = CornerRadius::same(coursecraft_render::RADIUS_SM as u8);
    painter.rect_filled(rect.shrink(1.0), radius, p.input);
    painter.rect_stroke(
        rect.shrink(1.0),
        radius,
        Stroke::new(1.0, p.border),
        egui::StrokeKind::Inside,
    );
    let col = if enabled { p.text } else { p.text_disabled };
    painter.text(
        pos2(rect.min.x + 6.0, rect.center().y),
        Align2::LEFT_CENTER,
        text,
        FontId::proportional(SMALL_LABEL_SIZE),
        col,
    );
    chevron(
        &painter,
        pos2(rect.max.x - 8.0, rect.center().y),
        2.8,
        p.text_secondary,
    );
    resp
}

/// The ↘ launcher at the right of a group caption.
pub fn launcher(ui: &mut Ui, rect: Rect, name: &str, enabled: bool) -> Response {
    let p = palette(ui.ctx());
    let resp = ui.interact(rect, ui.scope_id().with(("launcher", name)), sense(enabled));
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, name));
    if let Some(c) = fill(ui, &resp, false, &p) {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(coursecraft_render::RADIUS_SM as u8),
            c,
        );
    }
    let col = if enabled {
        p.text_secondary
    } else {
        p.text_disabled
    };
    let r = rect.shrink(4.0);
    let stroke = Stroke::new(1.0, col);
    ui.painter()
        .line_segment([r.left_top(), r.right_bottom()], stroke);
    ui.painter()
        .line_segment([r.right_bottom(), pos2(r.max.x - 4.0, r.max.y)], stroke);
    ui.painter()
        .line_segment([r.right_bottom(), pos2(r.max.x, r.max.y - 4.0)], stroke);
    resp
}

/// One popup for both halves of a split control, anchored under the whole control.
/// `main_opens` makes a click on the main half open it too (a menu-only control).
pub fn split_popup<'a>(
    main: &'a Response,
    arrow: &'a Response,
    main_opens: bool,
) -> egui::Popup<'a> {
    let clicked = arrow.clicked() || (main_opens && main.clicked());
    let alt_down = (main.has_focus() || arrow.has_focus())
        && main.sense.senses_click()
        && main
            .ctx
            .input_mut(|i| i.consume_key(egui::Modifiers::ALT, egui::Key::ArrowDown));
    let toggle = (clicked || alt_down).then_some(egui::SetOpenCommand::Toggle);
    egui::Popup::menu(main)
        .id(main.id.with("split-menu"))
        .anchor(main.rect.union(arrow.rect))
        .open_memory(toggle)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
}
