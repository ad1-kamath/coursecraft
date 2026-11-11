//! First-run onboarding coach marks.
//!
//! Three sequential dismissible popovers highlighting:
//! 1. Slide navigator: "Your slides live here. Right-click a slide for more options."
//! 2. Toolbar centre group: "Add text, shapes, media, buttons and questions."
//! 3. Inspector panel: "Select anything to edit it here."

use coursecraft_render::{
    CONTROL_COMPACT, RADIUS_LG, RADIUS_SM, SIZE_BODY, SIZE_SMALL, palette, popover_shadow,
    theme_mode,
};
use egui::{Align, Area, Color32, CursorIcon, Id, Layout, Pos2, RichText, Stroke, Ui, pos2, vec2};

use crate::Editor;

/// Direction the coach mark caret arrow points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrowDirection {
    Left,
    Up,
    Right,
}

/// Render the active coach mark popover, if any.
pub fn show_coach_marks(editor: &mut Editor, ui: &mut Ui) {
    if editor.ui.coach_marks_done || editor.ui.show_welcome {
        return;
    }

    let step = match editor.ui.current_coach_mark {
        Some(s @ 1..=3) => s,
        None => {
            editor.ui.current_coach_mark = Some(1);
            1
        }
        _ => return,
    };

    let p = palette(ui.ctx());
    let mode = theme_mode(ui.ctx());
    let screen = ui.ctx().content_rect();

    let popover_w = 260.0f32;
    let (target_pos, popover_pos, arrow_dir, message) = match step {
        1 => {
            // Target: Slide navigator thumbnail area
            let target = pos2(220.0, 160.0);
            let pos = pos2(236.0, 120.0);
            (
                target,
                pos,
                ArrowDirection::Left,
                "Your slides live here. Right-click a slide for more options.",
            )
        }
        2 => {
            // Target: Toolbar centre group
            let center_x = screen.center().x;
            let target = pos2(center_x, 78.0);
            let pos = pos2(center_x - popover_w * 0.5, 88.0);
            (
                target,
                pos,
                ArrowDirection::Up,
                "Add text, shapes, media, buttons and questions.",
            )
        }
        3 => {
            // Target: Inspector panel header / content
            let inspector_x = (screen.max.x - 300.0).max(popover_w + 32.0);
            let target = pos2(inspector_x, 160.0);
            let pos = pos2(inspector_x - popover_w - 16.0, 120.0);
            (
                target,
                pos,
                ArrowDirection::Right,
                "Select anything to edit it here.",
            )
        }
        _ => return,
    };

    let mut advance = false;
    let mut skip = false;

    Area::new(Id::unique("coach_mark_popover"))
        .order(egui::Order::Foreground)
        .fade_in(false)
        .fixed_pos(popover_pos)
        .show(ui.ctx(), |ui| {
            // Draw the caret arrow pointing toward the target
            draw_caret_arrow(ui, popover_pos, arrow_dir, p.raised, p.border);

            let frame = egui::Frame::NONE
                .fill(p.raised)
                .corner_radius(RADIUS_LG as u8)
                .stroke(Stroke::new(1.0, p.border))
                .shadow(popover_shadow(mode))
                .inner_margin(16.0);

            frame.show(ui, |ui| {
                ui.set_max_width(popover_w - 32.0);

                // Counter: "1 of 3"
                ui.label(
                    RichText::new(format!("{step} of 3"))
                        .size(SIZE_SMALL)
                        .color(p.text_tertiary),
                );
                ui.add_space(4.0);

                // Message text
                ui.label(RichText::new(message).size(SIZE_BODY).color(p.text));
                ui.add_space(14.0);

                // Actions: Skip (quiet) and Next/Done (primary small)
                ui.horizontal(|ui| {
                    if crate::quiet_button(ui, "Skip", crate::QuietTone::Secondary, true).clicked()
                    {
                        skip = true;
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let label = if step == 3 { "Done" } else { "Next" };
                        let btn = egui::Button::new(
                            RichText::new(label).color(Color32::WHITE).size(SIZE_SMALL),
                        )
                        .fill(p.accent)
                        .corner_radius(RADIUS_SM as u8)
                        .min_size(vec2(52.0, CONTROL_COMPACT - 2.0));
                        let resp = ui.add(btn).on_hover_cursor(CursorIcon::PointingHand);
                        if resp.clicked() {
                            advance = true;
                        }
                    });
                });
            });
        });

    let _ = target_pos;

    if skip {
        editor.ui.coach_marks_done = true;
        editor.ui.current_coach_mark = None;
    } else if advance {
        if step >= 3 {
            editor.ui.coach_marks_done = true;
            editor.ui.current_coach_mark = None;
        } else {
            editor.ui.current_coach_mark = Some(step + 1);
        }
    }
}

fn draw_caret_arrow(
    ui: &mut Ui,
    popover_pos: Pos2,
    direction: ArrowDirection,
    fill_color: Color32,
    stroke_color: Color32,
) {
    let painter = ui.painter();
    let size = 8.0f32;

    match direction {
        ArrowDirection::Left => {
            let tip = pos2(popover_pos.x - size, popover_pos.y + 24.0);
            let top = pos2(popover_pos.x + 1.0, popover_pos.y + 24.0 - size);
            let bottom = pos2(popover_pos.x + 1.0, popover_pos.y + 24.0 + size);
            painter.add(egui::epaint::Shape::convex_polygon(
                vec![tip, top, bottom],
                fill_color,
                Stroke::NONE,
            ));
            painter.line_segment([top, tip], Stroke::new(1.0, stroke_color));
            painter.line_segment([tip, bottom], Stroke::new(1.0, stroke_color));
        }
        ArrowDirection::Up => {
            let tip = pos2(popover_pos.x + 130.0, popover_pos.y - size);
            let left = pos2(popover_pos.x + 130.0 - size, popover_pos.y + 1.0);
            let right = pos2(popover_pos.x + 130.0 + size, popover_pos.y + 1.0);
            painter.add(egui::epaint::Shape::convex_polygon(
                vec![tip, right, left],
                fill_color,
                Stroke::NONE,
            ));
            painter.line_segment([left, tip], Stroke::new(1.0, stroke_color));
            painter.line_segment([tip, right], Stroke::new(1.0, stroke_color));
        }
        ArrowDirection::Right => {
            let w = 260.0;
            let tip = pos2(popover_pos.x + w + size, popover_pos.y + 24.0);
            let top = pos2(popover_pos.x + w - 1.0, popover_pos.y + 24.0 - size);
            let bottom = pos2(popover_pos.x + w - 1.0, popover_pos.y + 24.0 + size);
            painter.add(egui::epaint::Shape::convex_polygon(
                vec![tip, bottom, top],
                fill_color,
                Stroke::NONE,
            ));
            painter.line_segment([top, tip], Stroke::new(1.0, stroke_color));
            painter.line_segment([tip, bottom], Stroke::new(1.0, stroke_color));
        }
    }
}
