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
        if let Some(open) = self.open {
            window = window.open(open);
        }
        if let Some(size) = self.default_size {
            window = window.default_size(size);
        } else if let Some(width) = self.width {
            window = window.default_width(width);
        }
        let fixed_width = !self.resizable;
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
