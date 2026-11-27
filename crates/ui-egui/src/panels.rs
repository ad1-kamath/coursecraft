//! Settings panels: what a ribbon group's ↘ launcher opens (`docs/ui/panels.md`).
//!
//! Every dock panel belongs to a [`Family`]. The settings panels hold every
//! setting of one category, in [`Section`]s; a launcher opens its panel at one
//! section. A settings panel also opens by itself when the author starts work
//! that needs it (editing text opens Text), so the controls are there without a
//! click. With docking off, a launcher opens a settings panel as a floating
//! window; the work in hand does not, since it would cover the canvas.

use crate::Editor;
use crate::dock::Panel;

/// The three kinds of panel, in View › Panels order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    /// Where the course is built: slides, canvas, the timeline and the bottom
    /// tabs beside it (states, triggers, variables, question).
    Workspace,
    /// Quick settings for the selection: Format, Animate, Slide, Course.
    Inspectors,
    /// Every setting of one category, opened by a ↘ launcher.
    Settings,
}

impl Family {
    pub const ALL: [Family; 3] = [Family::Workspace, Family::Inspectors, Family::Settings];

    /// English heading, looked up through `tl!` when drawn.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Family::Workspace => "Workspace",
            Family::Inspectors => "Inspectors",
            Family::Settings => "Settings",
        }
    }
}

/// A part of a settings panel a launcher can open it at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Font,
    Paragraph,
    TextBox,
    Grid,
    Guides,
    Rulers,
    Size,
    Rotate,
    Position,
    Fill,
    Outline,
}

impl Section {
    /// English section heading, looked up through `tl!` when drawn.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Section::Font => "Font",
            Section::Paragraph => "Paragraph",
            Section::TextBox => "Text Box",
            Section::Grid => "Grid",
            Section::Guides => "Guides",
            Section::Rulers => "Rulers",
            Section::Size => "Size",
            Section::Rotate => "Rotate",
            Section::Position => "Position",
            Section::Fill => "Fill",
            Section::Outline => "Outline",
        }
    }

    /// The panel the section belongs to.
    #[must_use]
    pub fn panel(self) -> Panel {
        match self {
            Section::Font | Section::Paragraph | Section::TextBox => Panel::Text,
            Section::Grid | Section::Guides | Section::Rulers => Panel::GridGuides,
            Section::Size | Section::Rotate | Section::Position => Panel::SizePosition,
            Section::Fill | Section::Outline => Panel::ShapeStyle,
        }
    }
}

/// The family `panel` is listed under.
#[must_use]
pub fn family(panel: Panel) -> Family {
    match panel {
        Panel::Slides
        | Panel::Canvas
        | Panel::Timeline
        | Panel::States
        | Panel::Triggers
        | Panel::Variables
        | Panel::Question
        | Panel::Layers
        | Panel::Notes => Family::Workspace,
        Panel::Format | Panel::Animate | Panel::SlideInspector | Panel::CourseInspector => {
            Family::Inspectors
        }
        Panel::Text | Panel::GridGuides | Panel::SizePosition | Panel::ShapeStyle => {
            Family::Settings
        }
    }
}

/// The sections of a settings panel, top to bottom; empty for other panels.
#[must_use]
pub fn sections(panel: Panel) -> &'static [Section] {
    match panel {
        Panel::Text => &[Section::Font, Section::Paragraph, Section::TextBox],
        Panel::GridGuides => &[Section::Grid, Section::Guides, Section::Rulers],
        Panel::SizePosition => &[Section::Size, Section::Rotate, Section::Position],
        Panel::ShapeStyle => &[Section::Fill, Section::Outline],
        Panel::Slides
        | Panel::Canvas
        | Panel::Timeline
        | Panel::States
        | Panel::Triggers
        | Panel::Variables
        | Panel::Question
        | Panel::Layers
        | Panel::Notes
        | Panel::Format
        | Panel::Animate
        | Panel::SlideInspector
        | Panel::CourseInspector => &[],
    }
}

/// What the settings panels remember between frames.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PanelsState {
    /// The section a launcher asked for; the panel scrolls to it once.
    pub focus: Option<Section>,
    /// Frames spent scrolling to `focus`.
    pub focus_tries: u8,
    /// The panel the work in hand needed last frame (see [`wanted`]).
    pub wanted: Option<Panel>,
    /// With docking off, the settings panels shown as floating windows.
    pub floating: Vec<Panel>,
}

/// Open `panel`, bring it forward and scroll it to `section`. A closed panel
/// comes back where it was closed, else as a tab beside an open inspector.
/// Launchers only reveal: an open panel stays open.
pub fn open_at(editor: &mut Editor, panel: Panel, section: Option<Section>) {
    if let Some(tab) = panel.inspector() {
        editor.ui.inspector_tab = tab;
    }
    editor.ui.panels.focus = section.filter(|s| s.panel() == panel);
    editor.ui.panels.focus_tries = 0;
    if editor.ui.dock.enabled {
        crate::dock::reveal(editor, panel);
    } else if let Some(tab) = panel.bottom_tab() {
        // A bottom panel is a tab of the fixed bottom area, never a floating window (`N88`).
        editor.ui.bottom_tab = tab;
    } else if has_floating_body(panel) && !editor.ui.panels.floating.contains(&panel) {
        editor.ui.panels.floating.push(panel);
    }
}

/// Whether [`body`] draws `panel`, so that it can open as a floating window.
fn has_floating_body(panel: Panel) -> bool {
    matches!(
        panel,
        Panel::Text | Panel::GridGuides | Panel::SizePosition | Panel::ShapeStyle
    )
}

/// Close `panel` (View › Panels, a floating window's ×).
pub fn close(editor: &mut Editor, panel: Panel) {
    if editor.ui.dock.enabled {
        crate::dock::set_open(editor, panel, false);
    } else {
        editor.ui.panels.floating.retain(|p| *p != panel);
    }
}

/// Whether `panel` is on screen as a dock panel or a floating window.
#[must_use]
pub fn is_shown(editor: &Editor, panel: Panel) -> bool {
    if editor.ui.dock.enabled {
        crate::dock::is_open(editor, panel)
    } else {
        editor.ui.panels.floating.contains(&panel)
    }
}

/// The settings panel the work in hand needs, if any: editing text on the
/// canvas needs Text.
#[must_use]
pub fn wanted(editor: &Editor) -> Option<(Panel, Section)> {
    if editor.ui.inline_edit.is_some() {
        return Some((Panel::Text, Section::Font));
    }
    None
}

/// Open the panel the work in hand needs when that work starts. Only the start
/// counts: a panel the author closes while working stays closed. Docked panels
/// only: with docking off the fixed inspector already holds these controls, and
/// a window that opened by itself would cover the canvas being worked on.
pub fn follow_work(editor: &mut Editor) {
    let now = wanted(editor);
    let panel = now.map(|(panel, _)| panel);
    if panel == editor.ui.panels.wanted {
        return;
    }
    editor.ui.panels.wanted = panel;
    if let Some((panel, section)) = now {
        open_at(editor, panel, Some(section));
    }
}

/// Draw a settings panel's body.
pub fn body(editor: &mut Editor, ui: &mut egui::Ui, panel: Panel) {
    match panel {
        Panel::Text => crate::properties::show_text_panel(editor, ui),
        Panel::GridGuides => crate::grid_panel::show(editor, ui),
        Panel::SizePosition => crate::size_panel::show(editor, ui),
        Panel::ShapeStyle => crate::properties::show_shape_style_panel(editor, ui),
        Panel::Slides
        | Panel::Canvas
        | Panel::Timeline
        | Panel::States
        | Panel::Triggers
        | Panel::Variables
        | Panel::Question
        | Panel::Layers
        | Panel::Notes
        | Panel::Format
        | Panel::Animate
        | Panel::SlideInspector
        | Panel::CourseInspector => {}
    }
}

/// With docking off, the settings panels as floating windows with a ×.
pub fn show_floating(editor: &mut Editor, ctx: &egui::Context) {
    let shown = editor.ui.panels.floating.clone();
    for panel in shown {
        let mut open = true;
        egui::Window::new(tl!(panel.label()))
            .id(egui::Id::unique(("settings_panel", panel.label())))
            .open(&mut open)
            .default_width(crate::PROPERTIES_PANEL_WIDTH)
            .default_height(480.0)
            // The body scrolls; the window must not grow with it past the screen.
            .max_height((ctx.content_rect().height() - 120.0).max(240.0))
            .resizable(true)
            .show(ctx, |ui| body(editor, ui, panel));
        if !open {
            editor.ui.panels.floating.retain(|p| *p != panel);
        }
    }
}

/// A section heading in a settings panel; scrolls into view when a launcher
/// asked for it. A panel that just opened is still settling its size, so the
/// scroll is asked again until the heading is on screen, for a few frames at most.
pub fn section_header(state: &mut PanelsState, ui: &mut egui::Ui, section: Section) {
    let before = ui.cursor().min;
    crate::properties::inspector_section_header(ui, tl!(section.label()));
    if state.focus != Some(section) || ui.is_sizing_pass() {
        return;
    }
    let rect = egui::Rect::from_min_max(before, ui.cursor().min + egui::vec2(1.0, 0.0));
    state.focus_tries += 1;
    if ui.clip_rect().contains_rect(rect) && state.focus_tries > 1
        || state.focus_tries >= FOCUS_TRIES
    {
        state.focus = None;
        state.focus_tries = 0;
    } else {
        ui.scroll_to_rect(rect, Some(egui::Align::TOP));
        ui.ctx().request_repaint();
    }
}

/// Frames a launcher's section scroll is retried before giving up.
const FOCUS_TRIES: u8 = 8;
