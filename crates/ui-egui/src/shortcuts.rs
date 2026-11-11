//! Keyboard shortcuts cheat sheet dialog.
//!
//! Opened via `?` (Shift+/) and `Help ▸ Keyboard shortcuts`.
//! Displays a two-column list grouped by menu category, showing action names and
//! platform-formatted keycap chips.

use coursecraft_engine::registry;
use coursecraft_render::{
    RADIUS_SM, RADIUS_XL, SIZE_BODY, SIZE_CAPTION, SIZE_HEADING, SIZE_SMALL, dialog_shadow,
    palette, theme_mode,
};
use egui::{
    Align, Align2, Area, Color32, CornerRadius, CursorIcon, Id, Key, Layout, Modifiers, Rect,
    RichText, Sense, Stroke, Ui, pos2, vec2,
};

use crate::Editor;
use crate::menu::shortcut_label;

/// A shortcut entry representing an action and its platform-aware key description.
pub(crate) struct ShortcutRow {
    pub(crate) action: &'static str,
    pub(crate) shortcut: &'static str,
}

/// A group of shortcuts under a category header.
pub(crate) struct ShortcutGroup {
    pub(crate) category: &'static str,
    pub(crate) items: Vec<ShortcutRow>,
}

/// Dialog width in points.
const DIALOG_WIDTH: f32 = 480.0;
/// Dialog height is capped at this fraction of the window height.
const DIALOG_MAX_HEIGHT_FRACTION: f32 = 0.8;
/// Height reserved for the pinned title, hint, and separator above the list.
const DIALOG_HEADER_HEIGHT: f32 = 100.0;
/// Height of one shortcut row.
const SHORTCUT_ROW_HEIGHT: f32 = 22.0;
/// Vertical gap between consecutive shortcut rows (`item_spacing.y`).
const SHORTCUT_ROW_GAP: f32 = 3.0;
/// Height of a category caption line.
const SHORTCUT_GROUP_HEADER_HEIGHT: f32 = 15.0;
/// Space above each category.
const SHORTCUT_GROUP_TOP_SPACE: f32 = 8.0;
/// Space below each category.
const SHORTCUT_GROUP_BOTTOM_SPACE: f32 = 4.0;

/// Show the modal keyboard shortcuts dialog if `editor.ui.show_shortcuts_dialog` is true.
pub fn show_shortcuts_dialog(editor: &mut Editor, ctx: &egui::Context) {
    if !editor.ui.show_shortcuts_dialog {
        return;
    }

    let p = palette(ctx);
    let mode = theme_mode(ctx);
    let screen_rect = ctx.content_rect();

    // Backdrop click-detector area (transparent, captures clicks outside dialog to close)
    let backdrop_id = Id::unique("shortcuts_backdrop");
    let mut close = false;

    // Check Escape key
    if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
        close = true;
    }

    Area::new(backdrop_id)
        .order(egui::Order::Middle)
        .fixed_pos(screen_rect.min)
        .show(ctx, |ui| {
            let (_, response) = ui.allocate_exact_size(screen_rect.size(), Sense::click());
            if response.clicked() {
                close = true;
            }
        });

    let groups = build_shortcut_groups();
    let dialog_w = DIALOG_WIDTH.min(screen_rect.width() - 32.0);
    let dialog_h = dialog_height(&groups, screen_rect.height());
    let dialog_x = screen_rect.min.x + (screen_rect.width() - dialog_w) * 0.5;
    let dialog_y = screen_rect.min.y + (screen_rect.height() - dialog_h) * 0.5;
    let dialog_rect = Rect::from_min_size(pos2(dialog_x, dialog_y), vec2(dialog_w, dialog_h));

    Area::new(Id::unique("shortcuts_dialog_area"))
        .order(egui::Order::Foreground)
        .fade_in(false)
        .fixed_pos(dialog_rect.min)
        .show(ctx, |ui| {
            let frame = egui::Frame::NONE
                .fill(p.raised)
                .corner_radius(RADIUS_XL as u8)
                .stroke(Stroke::new(1.0, p.border))
                .shadow(dialog_shadow(mode))
                .inner_margin(24.0);

            frame.show(ui, |ui| {
                ui.set_width(dialog_w - 48.0);
                ui.set_min_height(dialog_h - 48.0);
                ui.set_max_height(dialog_h - 48.0);

                // Dialog Header: Title and Close button
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Keyboard shortcuts")
                            .size(SIZE_HEADING)
                            .strong()
                            .color(p.text),
                    );

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let close_btn = egui::Button::new(
                            coursecraft_render::icon(
                                coursecraft_render::Icon::X,
                                coursecraft_render::ICON_SM,
                            )
                            .color(p.text_secondary),
                        )
                        .fill(Color32::TRANSPARENT);
                        if ui
                            .add(close_btn)
                            .on_hover_cursor(CursorIcon::PointingHand)
                            .clicked()
                        {
                            close = true;
                        }
                    });
                });

                ui.add_space(4.0);
                ui.label(
                    RichText::new("Press Esc or click outside to close.")
                        .size(SIZE_SMALL)
                        .color(p.text_tertiary),
                );

                ui.add_space(16.0);
                ui.separator();
                ui.add_space(8.0);

                let list_height = (dialog_h - 48.0 - DIALOG_HEADER_HEIGHT).max(60.0);
                egui::ScrollArea::vertical()
                    .id_salt("shortcuts_scroll_area")
                    .auto_shrink([false, false])
                    .max_height(list_height)
                    .show(ui, |ui| {
                        for group in groups {
                            ui.add_space(8.0);
                            ui.label(
                                RichText::new(group.category)
                                    .size(SIZE_CAPTION)
                                    .strong()
                                    .color(p.text_secondary),
                            );
                            ui.add_space(4.0);
                            ui.spacing_mut().item_spacing.y = SHORTCUT_ROW_GAP;

                            for item in group.items {
                                let (row_rect, _) = ui.allocate_exact_size(
                                    vec2(ui.available_width(), SHORTCUT_ROW_HEIGHT),
                                    Sense::hover(),
                                );
                                if ui.is_rect_visible(row_rect) {
                                    // Action name on the left
                                    ui.painter().text(
                                        pos2(row_rect.min.x, row_rect.center().y),
                                        Align2::LEFT_CENTER,
                                        item.action,
                                        egui::FontId::proportional(SIZE_BODY),
                                        p.text,
                                    );

                                    // Formatted keycap chip on the right
                                    let formatted_key = shortcut_label(ui.ctx(), item.shortcut);
                                    draw_keycap_chip(
                                        ui,
                                        row_rect.max.x,
                                        row_rect.center().y,
                                        &formatted_key,
                                    );
                                }
                            }
                            ui.add_space(4.0);
                        }
                    });
            });
        });

    if close {
        editor.ui.show_shortcuts_dialog = false;
    }
}

/// Height of the scrollable shortcut list, including row gaps and category headings.
fn shortcut_list_height(groups: &[ShortcutGroup]) -> f32 {
    groups
        .iter()
        .map(|group| {
            let rows = group.items.len() as f32;
            SHORTCUT_GROUP_TOP_SPACE
                + SHORTCUT_GROUP_HEADER_HEIGHT
                + 4.0
                + rows * SHORTCUT_ROW_HEIGHT
                + rows * SHORTCUT_ROW_GAP
                + SHORTCUT_GROUP_BOTTOM_SPACE
        })
        .sum()
}

/// Height the dialog needs for `groups`, capped at 80% of the window.
pub(crate) fn dialog_height(groups: &[ShortcutGroup], screen_h: f32) -> f32 {
    let content = 48.0 + DIALOG_HEADER_HEIGHT + shortcut_list_height(groups);
    let max_h = (screen_h * DIALOG_MAX_HEIGHT_FRACTION).max(160.0);
    content.min(max_h).max(160.0)
}

fn draw_keycap_chip(ui: &mut Ui, right_x: f32, center_y: f32, key_text: &str) {
    let p = palette(ui.ctx());
    let font_id = egui::FontId::proportional(SIZE_SMALL);
    let galley = ui
        .painter()
        .layout_no_wrap(key_text.to_owned(), font_id, p.text);

    let pad_x = 8.0f32;
    let pad_y = 3.0f32;
    let chip_w = galley.size().x + pad_x * 2.0;
    let chip_h = galley.size().y + pad_y * 2.0;

    let chip_rect = Rect::from_min_size(
        pos2(right_x - chip_w, center_y - chip_h * 0.5),
        vec2(chip_w, chip_h),
    );

    ui.painter()
        .rect_filled(chip_rect, CornerRadius::same(RADIUS_SM as u8), p.input);
    ui.painter().rect_stroke(
        chip_rect,
        CornerRadius::same(RADIUS_SM as u8),
        Stroke::new(1.0, p.border),
        egui::StrokeKind::Inside,
    );

    let text_pos = pos2(chip_rect.min.x + pad_x, chip_rect.min.y + pad_y);
    ui.painter().galley(text_pos, galley, p.text);
}

pub(crate) fn build_shortcut_groups() -> Vec<ShortcutGroup> {
    let mut edit_items = vec![ShortcutRow {
        action: "Cut / Copy / Paste / Select all",
        shortcut: "Ctrl+X / Ctrl+C / Ctrl+V / Ctrl+A",
    }];
    let mut slide_items = Vec::new();
    let mut arrange_items = Vec::new();

    for cmd in registry() {
        let Some(sc) = cmd.shortcut else {
            continue;
        };
        if let Some(sub) = cmd.menu.strip_prefix("Edit/") {
            edit_items.push(ShortcutRow {
                action: sub,
                shortcut: sc,
            });
        } else if let Some(sub) = cmd.menu.strip_prefix("Slide/") {
            slide_items.push(ShortcutRow {
                action: sub,
                shortcut: sc,
            });
        } else if let Some(sub) = cmd.menu.strip_prefix("Arrange/") {
            arrange_items.push(ShortcutRow {
                action: sub,
                shortcut: sc,
            });
        }
    }

    // Editor-only shortcuts
    let view_items = vec![
        ShortcutRow {
            action: "Zoom to fit",
            shortcut: "Ctrl+0",
        },
        ShortcutRow {
            action: "Zoom in",
            shortcut: "Ctrl+=",
        },
        ShortcutRow {
            action: "Zoom out",
            shortcut: "Ctrl+-",
        },
    ];

    let general_items = vec![
        ShortcutRow {
            action: "Delete selected objects",
            shortcut: "Delete",
        },
        ShortcutRow {
            action: "Nudge selection (Shift: 10 px)",
            shortcut: "Arrow keys",
        },
        ShortcutRow {
            action: "Keep proportions while resizing",
            shortcut: "Shift+drag a handle",
        },
        ShortcutRow {
            action: "Add to selection / marquee select",
            shortcut: "Shift+click / Ctrl+click / drag",
        },
        ShortcutRow {
            action: "Cancel tool / close dialog / clear selection",
            shortcut: "Escape",
        },
        ShortcutRow {
            action: "Keyboard shortcuts cheat sheet / Preferences",
            shortcut: "? / Ctrl+,",
        },
        ShortcutRow {
            action: "Publish course",
            shortcut: "Ctrl+Shift+P",
        },
    ];

    // Never show an empty heading: a category with no commands disappears.
    vec![
        ShortcutGroup {
            category: "Edit",
            items: edit_items,
        },
        ShortcutGroup {
            category: "Slide",
            items: slide_items,
        },
        ShortcutGroup {
            category: "Arrange",
            items: arrange_items,
        },
        ShortcutGroup {
            category: "View & Canvas",
            items: view_items,
        },
        ShortcutGroup {
            category: "General",
            items: general_items,
        },
    ]
    .into_iter()
    .filter(|group| !group.items.is_empty())
    .collect()
}
