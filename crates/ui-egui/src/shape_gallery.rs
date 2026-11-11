//! The shape gallery popup: search, categories and shape icons.

use coursecraft_course::{SHAPE_CATALOG, ShapeCategory, ShapeInfo, ShapeKind, shape_label};
use coursecraft_render::{CONTROL_COMPACT, RADIUS_SM, SIZE_BODY, caption, paint_shape, palette};
use egui::{
    CornerRadius, FontId, Rect, Response, RichText, Sense, Stroke, StrokeKind, TextEdit, Ui, vec2,
};

use crate::DrawTool;

/// Preferred gallery width in points.
pub const GALLERY_WIDTH: f32 = 340.0;
/// Maximum gallery height in points; the grid scrolls past this.
pub const GALLERY_MAX_HEIGHT: f32 = 420.0;
const ICON_SIZE: f32 = 32.0;
const ICON_GAP: f32 = 4.0;

/// What the gallery picked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GalleryPick {
    Draw(DrawTool),
    Shape(ShapeKind),
}

/// Layout handed back to callers so the test harness can click gallery items
/// without guessing. Production callers can ignore it.
#[derive(Debug, Clone, Default)]
pub struct ShapeGalleryOutput {
    pub search_rect: Option<Rect>,
    pub icons: Vec<(ShapeKind, Rect)>,
    pub draw_icons: Vec<(DrawTool, Rect)>,
}

/// Show the gallery with no selected kind.
pub fn show_shape_gallery(
    ui: &mut Ui,
    query: &mut String,
    on_pick: &mut dyn FnMut(GalleryPick),
) -> ShapeGalleryOutput {
    show_shape_gallery_selected(ui, query, None, on_pick)
}

/// Show the gallery, highlighting `selected` when it matches a row.
pub fn show_shape_gallery_selected(
    ui: &mut Ui,
    query: &mut String,
    selected: Option<ShapeKind>,
    on_pick: &mut dyn FnMut(GalleryPick),
) -> ShapeGalleryOutput {
    let mut output = ShapeGalleryOutput::default();
    ui.set_min_width((GALLERY_WIDTH - 24.0).max(220.0));
    ui.set_max_width(GALLERY_WIDTH);
    let search = ui.add(
        TextEdit::singleline(query)
            .hint_text("Search shapes")
            .desired_width(f32::INFINITY)
            .id_salt("shape_gallery_search"),
    );
    output.search_rect = Some(search.rect);
    ui.add_space(4.0);

    let needle = query.trim().to_lowercase();
    ui.spacing_mut().item_spacing = vec2(ICON_GAP, ICON_GAP);
    egui::ScrollArea::vertical()
        .max_height(GALLERY_MAX_HEIGHT - 52.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            let mut any = false;
            let draw_matches: Vec<DrawTool> = DrawTool::ALL
                .iter()
                .copied()
                .filter(|tool| needle.is_empty() || tool.label().to_lowercase().contains(&needle))
                .collect();
            if !draw_matches.is_empty() {
                any = true;
                ui.label(caption("Draw"));
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(ICON_GAP, ICON_GAP);
                    for tool in draw_matches {
                        let (rect, response) =
                            ui.allocate_exact_size(vec2(ICON_SIZE, ICON_SIZE), Sense::click());
                        response.widget_info(|| {
                            egui::WidgetInfo::labeled(egui::Role::Button, true, tool.label())
                        });
                        let p = palette(ui.ctx());
                        if response.hovered() {
                            ui.painter().rect_filled(
                                rect,
                                CornerRadius::same(RADIUS_SM as u8),
                                p.hover,
                            );
                        }
                        paint_draw_tool(ui.painter(), rect.shrink(5.0), tool, p.text_secondary);
                        output.draw_icons.push((tool, rect));
                        let response = response.on_hover_text(tool.label());
                        if response.clicked() {
                            on_pick(GalleryPick::Draw(tool));
                            ui.close();
                        }
                    }
                });
                ui.add_space(4.0);
            }
            for category in ShapeCategory::ALL {
                // The fixed-direction line kinds stay loadable and render, but
                // the draw tools replace them in the gallery.
                if *category == ShapeCategory::Lines {
                    continue;
                }
                let matches: Vec<&ShapeInfo> = SHAPE_CATALOG
                    .iter()
                    .filter(|info| info.category == *category)
                    .filter(|info| needle.is_empty() || info.label.to_lowercase().contains(&needle))
                    .collect();
                if matches.is_empty() {
                    continue;
                }
                any = true;
                ui.label(caption(category.label()));
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(ICON_GAP, ICON_GAP);
                    for info in matches {
                        let (rect, response) =
                            ui.allocate_exact_size(vec2(ICON_SIZE, ICON_SIZE), Sense::click());
                        response.widget_info(|| {
                            egui::WidgetInfo::labeled(
                                egui::Role::Button,
                                true,
                                info.label.to_owned(),
                            )
                        });
                        let p = palette(ui.ctx());
                        let is_selected = selected == Some(info.kind);
                        if is_selected {
                            ui.painter().rect_filled(
                                rect,
                                CornerRadius::same(RADIUS_SM as u8),
                                p.accent_subtle,
                            );
                        } else if response.hovered() {
                            ui.painter().rect_filled(
                                rect,
                                CornerRadius::same(RADIUS_SM as u8),
                                p.hover,
                            );
                        }
                        let color = if is_selected {
                            p.accent_text
                        } else {
                            p.text_secondary
                        };
                        paint_shape(ui.painter(), rect.shrink(5.0), info.kind, &[], color);
                        output.icons.push((info.kind, rect));
                        let response = response.on_hover_text(info.label);
                        if response.clicked() {
                            on_pick(GalleryPick::Shape(info.kind));
                            ui.close();
                        }
                    }
                });
                ui.add_space(4.0);
            }
            if !any {
                ui.label(RichText::new("No shapes match").color(palette(ui.ctx()).text_secondary));
            }
        });

    if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
        ui.close();
    }
    output
}

fn paint_draw_tool(painter: &egui::Painter, rect: Rect, tool: DrawTool, color: egui::Color32) {
    let stroke = Stroke::new(1.5, color);
    let (l, r, t, b) = (rect.left(), rect.right(), rect.top(), rect.bottom());
    let mid_y = rect.center().y;
    match tool {
        DrawTool::Line => {
            painter.line_segment([egui::pos2(l, b), egui::pos2(r, t)], stroke);
        }
        DrawTool::Arrow => {
            painter.line_segment([egui::pos2(l, b), egui::pos2(r, t)], stroke);
            painter.add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(r, t),
                    egui::pos2(r - 8.0, t + 2.0),
                    egui::pos2(r - 2.0, t + 8.0),
                ],
                color,
                Stroke::NONE,
            ));
        }
        DrawTool::ElbowConnector => {
            painter.line_segment([egui::pos2(l, b), egui::pos2(rect.center().x, b)], stroke);
            painter.line_segment(
                [
                    egui::pos2(rect.center().x, b),
                    egui::pos2(rect.center().x, t),
                ],
                stroke,
            );
            painter.line_segment([egui::pos2(rect.center().x, t), egui::pos2(r, t)], stroke);
        }
        DrawTool::CurvedConnector => {
            let points: Vec<egui::Pos2> = (0..=12)
                .map(|i| {
                    #[allow(clippy::cast_precision_loss)]
                    let u = i as f32 / 12.0;
                    let x = l + (r - l) * u;
                    let y = b + (t - b) * (u * u * (3.0 - 2.0 * u));
                    egui::pos2(x, y)
                })
                .collect();
            painter.add(egui::Shape::line(points, stroke));
        }
        DrawTool::Curve => {
            painter.add(egui::Shape::line(
                vec![
                    egui::pos2(l, b),
                    egui::pos2(rect.center().x, t),
                    egui::pos2(r, b),
                ],
                stroke,
            ));
        }
        DrawTool::Freeform => {
            painter.add(egui::Shape::line(
                vec![
                    egui::pos2(l, mid_y),
                    egui::pos2(l + 7.0, t),
                    egui::pos2(rect.center().x, b - 2.0),
                    egui::pos2(r - 7.0, t),
                    egui::pos2(r, mid_y),
                ],
                stroke,
            ));
        }
        DrawTool::Scribble => {
            let points: Vec<egui::Pos2> = (0..=12)
                .map(|i| {
                    #[allow(clippy::cast_precision_loss)]
                    let u = i as f32 / 12.0;
                    let x = l + (r - l) * u;
                    let y = mid_y + (u * std::f32::consts::TAU).sin() * (b - t) * 0.25;
                    egui::pos2(x, y)
                })
                .collect();
            painter.add(egui::Shape::line(points, stroke));
        }
    }
}

/// A compact button showing the current shape's icon and label.
pub fn shape_picker_button(ui: &mut Ui, kind: ShapeKind, width: f32) -> Response {
    let width = width.max(32.0);
    let (rect, response) = ui.allocate_exact_size(vec2(width, CONTROL_COMPACT), Sense::click());
    let p = palette(ui.ctx());
    let fill = if response.hovered() { p.hover } else { p.input };
    ui.painter()
        .rect_filled(rect, CornerRadius::same(RADIUS_SM as u8), fill);
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(RADIUS_SM as u8),
        Stroke::new(1.0, p.border_strong),
        StrokeKind::Inside,
    );
    let icon_rect = Rect::from_center_size(
        egui::pos2(rect.left() + 14.0, rect.center().y),
        vec2(16.0, 16.0),
    );
    paint_shape(ui.painter(), icon_rect, kind, &[], p.text_secondary);
    let galley = ui.painter().layout_no_wrap(
        shape_label(kind).to_owned(),
        FontId::proportional(SIZE_BODY),
        p.text,
    );
    ui.painter().galley(
        egui::pos2(rect.left() + 26.0, rect.center().y - galley.size().y * 0.5),
        galley,
        p.text,
    );
    response.on_hover_text("Choose a shape")
}

/// The Insert menu's floating shape gallery. Search needs a real text field,
/// which a nested menu cannot host, so this opens a small window instead.
pub fn show_shape_gallery_window(editor: &mut crate::Editor, ctx: &egui::Context) {
    if !editor.ui.show_shape_gallery_window {
        return;
    }
    let mut open = true;
    let mut picked = None;
    egui::Window::new("Shapes")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(GALLERY_WIDTH)
        .show(ctx, |ui| {
            let output = show_shape_gallery(ui, &mut editor.ui.shape_gallery_query, &mut |pick| {
                picked = Some(pick)
            });
            editor.ui.shape_gallery_rects = output.icons;
            editor.ui.shape_gallery_draw_rects = output.draw_icons;
            editor.ui.shape_gallery_search_rect = output.search_rect;
        });
    let escape = ctx.input(|input| input.key_pressed(egui::Key::Escape));
    if let Some(pick) = picked {
        match pick {
            GalleryPick::Shape(kind) => crate::toolbar::insert_shape(editor, kind),
            GalleryPick::Draw(tool) => crate::toolbar::insert_draw(editor, tool),
        }
        editor.ui.show_shape_gallery_window = false;
    } else if !open || escape {
        editor.ui.show_shape_gallery_window = false;
    }
}
