//! The one dialog frame every CourseCraft dialog goes through.
//!
//! A dialog built with [`Dialog`] always:
//! - opens centred in the area below the menu bar and toolbar, never over the menu bar and
//!   never past a window edge (it is constrained to [`dialog_area`] every frame);
//! - is no taller than that area: a tall body scrolls inside the dialog;
//! - can keep its action buttons (OK, Save, Publish, Cancel, Close) in a footer that does not
//!   scroll, so they are always visible;
//! - uses the same window style and inner margins as every other dialog.
//!
//! Most dialogs only call [`Dialog::show`]. A dialog with action buttons calls
//! [`Dialog::show_with_footer`] and fills [`DialogUi::body`] then [`DialogUi::footer`].

use egui::{
    Align2, Context, Id, InnerResponse, Rect, ScrollArea, Ui, Vec2, WidgetText, Window, pos2,
};

/// Gap between a dialog and the window edge or the toolbar above it.
pub const DIALOG_EDGE_GAP: f32 = 8.0;
/// Height the window frame takes besides the body: the title bar, the frame margins and the
/// gap above the footer. A little generous, so a capped dialog never clips its footer.
const FRAME_CHROME: f32 = 84.0;

fn top_inset_id() -> Id {
    Id::unique("coursecraft_dialog_top_inset")
}

fn footer_id(window: Id) -> Id {
    window.with("dialog_footer_height")
}

/// The frame in which a popup already answered the Escape key. Dialogs drawn later in the same
/// frame must not also close on that key press.
fn escape_claim_id() -> Id {
    Id::unique("coursecraft_dialog_escape_claim")
}

fn shown_id() -> Id {
    Id::unique("coursecraft_dialog_shown_frame")
}

/// Whether a [`Dialog`] was drawn this frame or the one before. Global shortcuts use it to
/// stay out of a dialog's way (the ribbon is drawn before the dialogs).
#[must_use]
pub fn any_open(ctx: &Context) -> bool {
    ctx.data(|d| d.get_temp::<u64>(shown_id()))
        .is_some_and(|frame| frame + 1 >= ctx.cumulative_frame_nr())
}

/// Record the y below the menu bar and toolbar. Dialogs open below it.
pub(crate) fn set_top_inset(ctx: &Context, y: f32) {
    ctx.data_mut(|d| d.insert_temp(top_inset_id(), y));
}

/// The area a dialog may occupy: the window below the menu bar and toolbar, inset by
/// [`DIALOG_EDGE_GAP`] on every side.
#[must_use]
pub fn dialog_area(ctx: &Context) -> Rect {
    let screen = ctx.content_rect();
    let top = ctx
        .data(|d| d.get_temp::<f32>(top_inset_id()))
        .unwrap_or(crate::TOP_BAR_HEIGHT)
        .max(crate::TOP_BAR_HEIGHT);
    let min = pos2(
        screen.min.x + DIALOG_EDGE_GAP,
        (screen.min.y + top + DIALOG_EDGE_GAP).min(screen.max.y - 40.0),
    );
    let max = pos2(
        (screen.max.x - DIALOG_EDGE_GAP).max(min.x + 120.0),
        (screen.max.y - DIALOG_EDGE_GAP).max(min.y + 80.0),
    );
    Rect::from_min_max(min, max)
}

/// A dialog window. See the module docs.
pub struct Dialog<'a> {
    title: WidgetText,
    id: Option<Id>,
    open: Option<&'a mut bool>,
    width: Option<f32>,
    default_size: Option<Vec2>,
    resizable: bool,
    movable: bool,
    pinned_top: bool,
    close_on_escape: bool,
}

impl<'a> Dialog<'a> {
    /// A dialog with `title`. It is movable by its title bar, not resizable, not collapsible.
    pub fn new(title: impl Into<WidgetText>) -> Self {
        Self {
            title: title.into(),
            id: None,
            open: None,
            width: None,
            default_size: None,
            resizable: false,
            movable: true,
            pinned_top: false,
            close_on_escape: true,
        }
    }

    /// Use `id` instead of an id derived from the title.
    #[must_use]
    pub fn id(mut self, id: Id) -> Self {
        self.id = Some(id);
        self
    }

    /// Show a close button in the title bar that clears `open`.
    #[must_use]
    pub fn open(mut self, open: &'a mut bool) -> Self {
        self.open = Some(open);
        self
    }

    /// The dialog's outer width in points.
    #[must_use]
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// The first size of a resizable dialog.
    #[must_use]
    pub fn default_size(mut self, size: impl Into<Vec2>) -> Self {
        self.default_size = Some(size.into());
        self
    }

    /// Let the user resize the dialog.
    #[must_use]
    pub fn resizable(mut self, resizable: bool) -> Self {
        self.resizable = resizable;
        self
    }

    /// Whether the title bar drags the dialog (on by default).
    #[must_use]
    pub fn movable(mut self, movable: bool) -> Self {
        self.movable = movable;
        self
    }

    /// Open near the top of the dialog area and keep the top edge fixed as the height changes,
    /// instead of centring (for dialogs that grow, such as Find and Replace).
    #[must_use]
    pub fn pinned_top(mut self) -> Self {
        self.pinned_top = true;
        self
    }

    /// Whether pressing `Escape` closes the dialog when no popup is open (on by default).
    #[must_use]
    pub fn close_on_escape(mut self, close: bool) -> Self {
        self.close_on_escape = close;
        self
    }

    /// Show the dialog with one scrolling body.
    pub fn show<R>(
        self,
        ctx: &Context,
        add_body: impl FnOnce(&mut Ui) -> R,
    ) -> Option<InnerResponse<Option<R>>> {
        self.show_with_footer(ctx, |dialog| dialog.body(add_body))
    }

    /// Show the dialog. `content` calls [`DialogUi::body`] once and, if the dialog has action
    /// buttons, [`DialogUi::footer`] once after it.
    pub fn show_with_footer<R>(
        self,
        ctx: &Context,
        content: impl FnOnce(&mut DialogUi<'_>) -> R,
    ) -> Option<InnerResponse<Option<R>>> {
        let area = dialog_area(ctx);
        let window_id = self
            .id
            .unwrap_or_else(|| Id::unique(("coursecraft_dialog", self.title.text())));
        let mut window = Window::new(self.title)
            .id(window_id)
            .collapsible(false)
            .resizable(self.resizable)
            .movable(self.movable)
            .constrain_to(area)
            .max_size(area.size());
        window = if self.pinned_top {
            window
                .pivot(Align2::CENTER_TOP)
                .default_pos(pos2(area.center().x, area.min.y + 40.0))
        } else {
            window
                .pivot(Align2::CENTER_CENTER)
                .default_pos(area.center())
        };
        if let Some(size) = self.default_size {
            window = window.default_size(size);
        } else if let Some(width) = self.width {
            window = window.default_width(width);
        }
        let fixed_width = !self.resizable;
        let popup_was_open = egui::Popup::is_any_open(ctx);
        let frame = ctx.cumulative_frame_nr();
        if popup_was_open && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            ctx.data_mut(|d| d.insert_temp(escape_claim_id(), frame));
        }
        let close_on_escape = self.close_on_escape;
        let show_window = |window: Window<'_>| {
            window.show(ctx, move |ui| {
                let footer_h = ui
                    .ctx()
                    .data(|d| d.get_temp::<f32>(footer_id(window_id)))
                    .unwrap_or(0.0);
                let start_y = ui.cursor().min.y;
                let mut dialog = DialogUi {
                    ui,
                    window_id,
                    max_body_height: (area.height() - FRAME_CHROME - footer_h).max(80.0),
                    start_y,
                    fixed_width,
                };
                content(&mut dialog)
            })
        };

        let mark_shown = |shown: bool| {
            if shown {
                let frame = ctx.cumulative_frame_nr();
                ctx.data_mut(|d| d.insert_temp(shown_id(), frame));
            }
        };
        if let Some(open) = self.open {
            let mut still_open = *open;
            let response = show_window(window.open(&mut still_open));
            mark_shown(response.is_some());
            if response.is_some()
                && still_open
                && close_on_escape
                && ctx.data(|d| d.get_temp::<u64>(escape_claim_id())) != Some(frame)
                && !egui::Popup::is_any_open(ctx)
                && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
            {
                still_open = false;
            }
            *open = still_open;
            response
        } else {
            let response = show_window(window);
            mark_shown(response.is_some());
            response
        }
    }
}

/// The inside of a [`Dialog`]: a scrolling body and an optional fixed footer.
pub struct DialogUi<'u> {
    ui: &'u mut Ui,
    window_id: Id,
    max_body_height: f32,
    start_y: f32,
    fixed_width: bool,
}

impl DialogUi<'_> {
    /// The dialog's scrolling body. It grows with its content up to the dialog's maximum
    /// height, then scrolls.
    pub fn body<R>(&mut self, add_body: impl FnOnce(&mut Ui) -> R) -> R {
        let fixed_width = self.fixed_width;
        // Whatever the dialog drew above the body (a tab row, say) is not available to it.
        let above = (self.ui.cursor().min.y - self.start_y).max(0.0);
        self.ui.spacing_mut().scroll.floating = false;
        // egui sizes a scroll area from the room it has, and a window's room is only what it
        // already is, so give the area its full height up front; it shrinks to its content.
        let height = (self.max_body_height - above).max(60.0);
        ScrollArea::vertical()
            .id_salt(self.window_id.with("dialog_body"))
            .auto_shrink([true, true])
            .max_height(height)
            .min_scrolled_height(height)
            .show(self.ui, |ui| {
                #[cfg(test)]
                crate::scroll_audit::record_scroll(
                    ui.ctx(),
                    ui.layer_id(),
                    ui.clip_rect(),
                    true,
                    true,
                );
                if fixed_width {
                    ui.set_min_width(ui.available_width());
                }
                add_body(ui)
            })
            .inner
    }

    /// The dialog's body without an outer [`ScrollArea`], for dialogs that manage their own
    /// internal scrolling (such as a table or virtualised list) so they never nest vertical
    /// scroll areas.
    pub fn body_unscrolled<R>(&mut self, add_body: impl FnOnce(&mut Ui) -> R) -> R {
        let fixed_width = self.fixed_width;
        let above = (self.ui.cursor().min.y - self.start_y).max(0.0);
        let height = (self.max_body_height - above).max(60.0);
        self.ui
            .scope(|ui| {
                if ui.available_height() > height {
                    ui.set_max_height(height);
                }
                if fixed_width {
                    ui.set_min_width(ui.available_width());
                }
                add_body(ui)
            })
            .inner
    }

    /// The footer for the action buttons: below the body, separated by a hairline, never
    /// scrolled away.
    pub fn footer<R>(&mut self, add_footer: impl FnOnce(&mut Ui) -> R) -> R {
        let top = self.ui.cursor().min.y;
        self.ui.add_space(coursecraft_render::SPACE_SM);
        self.ui.separator();
        let result = add_footer(self.ui);
        let height = (self.ui.min_rect().max.y - top).max(0.0);
        let window_id = self.window_id;
        self.ui
            .ctx()
            .data_mut(|d| d.insert_temp(footer_id(window_id), height));
        result
    }

    /// The underlying ui, for content that sits outside the body and footer (rare).
    pub fn ui(&mut self) -> &mut Ui {
        self.ui
    }
}

/// A label-plus-field form inside a dialog.
///
/// The label column is as wide as the widest label of this form, measured with the real font,
/// so every field starts at the same x. [`Form::row`] hands the field the width left to the
/// dialog's right edge; a field that fills it ends at the same right edge as the rest. A field
/// meant to be short (a number, a pick list) takes less, and still starts at the same x.
#[derive(Debug, Clone, Copy)]
pub struct Form {
    label_width: f32,
}

impl Form {
    /// Gap between the label column and the fields.
    pub const LABEL_GAP: f32 = 12.0;

    /// A form whose label column fits every one of `labels`.
    #[must_use]
    pub fn new(ui: &Ui, labels: &[&str]) -> Self {
        let font = egui::TextStyle::Body.resolve(ui.style());
        let widest = labels
            .iter()
            .map(|label| {
                ui.painter()
                    .layout_no_wrap((*label).to_owned(), font.clone(), egui::Color32::WHITE)
                    .size()
                    .x
            })
            .fold(0.0_f32, f32::max);
        Self {
            label_width: widest.ceil(),
        }
    }

    /// The width of the label column (without the gap).
    #[must_use]
    pub fn label_width(&self) -> f32 {
        self.label_width
    }

    /// One row: `label` in the label column, then `field`, which is given the width left to
    /// the right edge.
    pub fn row<R>(&self, ui: &mut Ui, label: &str, field: impl FnOnce(&mut Ui, f32) -> R) -> R {
        let label_width = self.label_width;
        let row_height = coursecraft_render::CONTROL_COMPACT;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = Self::LABEL_GAP;
            let (rect, _) =
                ui.allocate_exact_size(egui::vec2(label_width, row_height), egui::Sense::hover());
            if ui.is_rect_visible(rect) {
                let color = coursecraft_render::palette(ui.ctx()).text_secondary;
                let font = egui::TextStyle::Body.resolve(ui.style());
                let galley = ui.painter().layout_no_wrap(label.to_owned(), font, color);
                ui.painter().galley(
                    pos2(rect.min.x, rect.center().y - galley.size().y * 0.5),
                    galley,
                    color,
                );
            }
            let width = ui.available_width().max(60.0);
            field(ui, width)
        })
        .inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, Key, Modifiers, Pos2, RawInput, vec2};

    fn run_dialog_frame(
        ctx: &Context,
        open: &mut bool,
        popup_id: Id,
        open_popup: bool,
        press_escape: bool,
    ) {
        let mut events = Vec::new();
        if press_escape {
            events.push(Event::Key {
                key: Key::Escape,
                physical_key: Some(Key::Escape),
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            });
        }
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1024.0, 768.0))),
            events,
            ..Default::default()
        };
        let output = ctx.run_ui(raw, |ui| {
            if open_popup {
                egui::Popup::open_id(ui.ctx(), popup_id);
            }
            Dialog::new("Test Esc Dialog")
                .open(open)
                .show_with_footer(ui.ctx(), |dialog| {
                    dialog.body(|ui| {
                        let resp = ui.button("Trigger");
                        egui::Popup::from_toggle_button_response(&resp)
                            .id(popup_id)
                            .show(|ui| {
                                ui.label("Popup item");
                            });
                    });
                });
        });
        output.drop_without_applying_deltas();
    }

    #[test]
    fn dialog_handles_escape_only_when_no_popup_is_open() {
        let ctx = Context::default();
        coursecraft_render::apply_style(&ctx, coursecraft_render::ThemeMode::Dark);
        let mut open = true;
        let popup_id = Id::unique("dialog_esc_test_popup");

        // Initial frame + open popup.
        run_dialog_frame(&ctx, &mut open, popup_id, false, false);
        run_dialog_frame(&ctx, &mut open, popup_id, true, false);
        assert!(open, "dialog should be open");
        assert!(
            egui::Popup::is_any_open(&ctx),
            "popup should be open before pressing Escape"
        );

        // Escape while popup is open: popup closes, dialog stays open.
        run_dialog_frame(&ctx, &mut open, popup_id, false, true);
        assert!(
            open,
            "dialog must stay open when Escape dismisses an open popup"
        );
        assert!(
            !egui::Popup::is_any_open(&ctx),
            "popup should have closed on Escape"
        );

        // Escape when no popup is open: dialog closes.
        run_dialog_frame(&ctx, &mut open, popup_id, false, true);
        assert!(!open, "dialog should close on Escape when no popup is open");
    }

    #[test]
    fn dialog_body_unscrolled_emits_zero_outer_scroll_area() {
        let ctx = Context::default();
        coursecraft_render::apply_style(&ctx, coursecraft_render::ThemeMode::Dark);
        let mut open = true;
        let mut recorded_count = 0;
        let mut nested_findings = Vec::new();
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1024.0, 768.0))),
            ..Default::default()
        };
        let output = ctx.run_ui(raw, |ui| {
            Dialog::new("Unscrolled Dialog")
                .open(&mut open)
                .show_with_footer(ui.ctx(), |dialog| {
                    dialog.body_unscrolled(|ui| {
                        ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                            crate::scroll_audit::record_scroll(
                                ui.ctx(),
                                ui.layer_id(),
                                ui.clip_rect(),
                                true,
                                false,
                            );
                            for i in 0..30 {
                                ui.label(format!("Row {i}"));
                            }
                        });
                    });
                });
            let scrolls = crate::scroll_audit::recorded_scrolls(ui.ctx());
            recorded_count = scrolls.iter().filter(|s| s.from_dialog_body).count();
            let layers = crate::test_harness::capture_layers(ui.ctx());
            nested_findings = crate::test_harness::check_nested_scrolls(&layers, &scrolls);
        });
        output.drop_without_applying_deltas();
        assert_eq!(
            recorded_count, 0,
            "DialogUi::body_unscrolled must not emit an outer DialogUi::body ScrollArea"
        );
        assert!(
            nested_findings.is_empty(),
            "inner ScrollArea inside body_unscrolled must have zero nested-scroll findings: {nested_findings:?}"
        );
    }
}
