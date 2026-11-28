//! The editor's panels in a `craft_ui::docking` workspace (M37 step 1).
//!
//! On by default (Preferences › Interface › Docked panels): the slides panel, the
//! canvas, the timeline and the Inspector are dock panels the user can move, stack,
//! tab, float, close and reopen (View › Panels). The canvas is protected: it stays
//! docked and open. The arrangement is saved in the app's preferences through
//! [`crate::EditorRequest::SaveLayout`]; the fixed egui panels remain when docking
//! is turned off.
//!
//! Docking rather than `craft_ui::panes`: CourseCraft stacks panels (Triggers over
//! Slide Layers) and tabs them (Timeline, States, Notes), which panes don't do.

use craft_ui::docking::{
    Action, DockArea, DockContent, DockStyle, Group, Layout, Node, PanelLimits, Permissions, Side,
    Workspace,
};
use craft_ui::layout::{SplitAxis, SplitSize};
use egui::{Rect, Ui, vec2};
use serde::{Deserialize, Serialize};

use crate::{BottomTab, Editor, InspectorTab};

/// One dockable part of the editor window. Stable ids, saved in the preferences.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Panel {
    Slides,
    Canvas,
    Timeline,
    /// The four inspectors, each a panel of its own (they start as tabs of one
    /// group where the single Inspector used to be).
    Format,
    Animate,
    #[serde(rename = "slide_inspector")]
    SlideInspector,
    #[serde(rename = "course_inspector")]
    CourseInspector,
    /// Settings panels (`crate::panels`): opened by a ↘ launcher or by the
    /// work that needs them, not part of the default layout.
    Text,
    /// The bottom tabs, each a panel of its own (they start as tabs of the
    /// Timeline's group, where the fixed bottom panel's tab strip used to be).
    States,
    Triggers,
    Variables,
    Question,
    /// Slide Layers, beside the Timeline's group (it used to share the
    /// Timeline panel).
    Layers,
    /// The slide's notes, a tab of the Timeline's group.
    Notes,
    /// The Grid and Guides settings panel (View › Show ↘).
    #[serde(rename = "grid_guides")]
    GridGuides,
    /// The Size and Position settings panel (Animations › Size and Position).
    #[serde(rename = "size_position")]
    SizePosition,
    /// The Shape Style settings panel (Home › Drawing ↘).
    #[serde(rename = "shape_style")]
    ShapeStyle,
}

impl Panel {
    pub const ALL: [Panel; 17] = [
        Panel::Slides,
        Panel::Canvas,
        Panel::Timeline,
        Panel::States,
        Panel::Triggers,
        Panel::Variables,
        Panel::Question,
        Panel::Notes,
        Panel::Layers,
        Panel::Format,
        Panel::Animate,
        Panel::SlideInspector,
        Panel::CourseInspector,
        Panel::Text,
        Panel::SizePosition,
        Panel::ShapeStyle,
        Panel::GridGuides,
    ];

    /// The bottom panels, in their tab order.
    pub const BOTTOM: [Panel; 5] = [
        Panel::Timeline,
        Panel::States,
        Panel::Triggers,
        Panel::Variables,
        Panel::Question,
    ];

    /// The inspector panels, in their tab order.
    pub const INSPECTORS: [Panel; 4] = [
        Panel::Format,
        Panel::Animate,
        Panel::SlideInspector,
        Panel::CourseInspector,
    ];

    /// The inspector this panel draws, if it is one.
    #[must_use]
    pub fn inspector(self) -> Option<InspectorTab> {
        match self {
            Panel::Format => Some(InspectorTab::Format),
            Panel::Animate => Some(InspectorTab::Animate),
            Panel::SlideInspector => Some(InspectorTab::Slide),
            Panel::CourseInspector => Some(InspectorTab::Course),
            Panel::Slides
            | Panel::Canvas
            | Panel::Timeline
            | Panel::Text
            | Panel::States
            | Panel::Triggers
            | Panel::Variables
            | Panel::Question
            | Panel::Layers
            | Panel::Notes
            | Panel::GridGuides
            | Panel::SizePosition
            | Panel::ShapeStyle => None,
        }
    }

    /// The bottom tab this panel draws, if it is one.
    #[must_use]
    pub fn bottom_tab(self) -> Option<BottomTab> {
        match self {
            Panel::Timeline => Some(BottomTab::Timeline),
            Panel::States => Some(BottomTab::States),
            Panel::Triggers => Some(BottomTab::Triggers),
            Panel::Variables => Some(BottomTab::Variables),
            Panel::Question => Some(BottomTab::Question),
            Panel::Slides
            | Panel::Canvas
            | Panel::Format
            | Panel::Animate
            | Panel::SlideInspector
            | Panel::CourseInspector
            | Panel::Text
            | Panel::Layers
            | Panel::Notes
            | Panel::GridGuides
            | Panel::SizePosition
            | Panel::ShapeStyle => None,
        }
    }

    /// The panel that draws `tab`.
    #[must_use]
    pub fn for_bottom_tab(tab: BottomTab) -> Panel {
        match tab {
            BottomTab::Timeline => Panel::Timeline,
            BottomTab::States => Panel::States,
            BottomTab::Triggers => Panel::Triggers,
            BottomTab::Variables => Panel::Variables,
            BottomTab::Question => Panel::Question,
        }
    }

    /// The panel that draws `tab`.
    #[must_use]
    pub fn for_inspector(tab: InspectorTab) -> Panel {
        match tab {
            InspectorTab::Format => Panel::Format,
            InspectorTab::Animate => Panel::Animate,
            InspectorTab::Slide => Panel::SlideInspector,
            InspectorTab::Course => Panel::CourseInspector,
        }
    }

    /// English tab label, looked up through `tl!` when drawn.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Panel::Slides => "Slides",
            Panel::Canvas => "Canvas",
            Panel::Timeline => "Timeline",
            Panel::Format => "Format",
            Panel::Animate => "Animate",
            Panel::SlideInspector => "Slide",
            Panel::CourseInspector => "Course",
            Panel::Text => "Text",
            Panel::States => "States",
            Panel::Triggers => "Triggers",
            Panel::Variables => "Variables",
            Panel::Question => "Question",
            Panel::Layers => "Slide Layers",
            Panel::Notes => "Notes",
            Panel::GridGuides => "Grid and Guides",
            Panel::SizePosition => "Size and Position",
            Panel::ShapeStyle => "Shape Style",
        }
    }

    #[must_use]
    pub fn permissions(self) -> Permissions {
        match self {
            // The canvas stays docked and open; panels may split beside it.
            Panel::Canvas => Permissions {
                accepts_splits: true,
                ..Permissions::PROTECTED
            },
            // View › Panels reopens them.
            Panel::Slides
            | Panel::Timeline
            | Panel::Format
            | Panel::Animate
            | Panel::SlideInspector
            | Panel::CourseInspector
            | Panel::Text
            | Panel::States
            | Panel::Triggers
            | Panel::Variables
            | Panel::Question
            | Panel::Layers
            | Panel::Notes
            | Panel::GridGuides
            | Panel::SizePosition
            | Panel::ShapeStyle => Permissions::default(),
        }
    }

    fn limits(self) -> PanelLimits {
        let min = match self {
            Panel::Slides => vec2(180.0, 160.0),
            Panel::Canvas => vec2(320.0, 200.0),
            Panel::Timeline
            | Panel::States
            | Panel::Triggers
            | Panel::Variables
            | Panel::Question
            | Panel::Notes => vec2(320.0, crate::timeline::TIMELINE_PANEL_MIN_HEIGHT),
            Panel::Layers => vec2(160.0, crate::timeline::TIMELINE_PANEL_MIN_HEIGHT),
            Panel::Format
            | Panel::Animate
            | Panel::SlideInspector
            | Panel::CourseInspector
            | Panel::Text
            | Panel::GridGuides
            | Panel::SizePosition
            | Panel::ShapeStyle => vec2(260.0, 200.0),
        };
        PanelLimits {
            min,
            ..PanelLimits::default()
        }
    }
}

/// Docking on or off, and the arrangement.
#[derive(Debug, Clone, PartialEq)]
pub struct DockState {
    /// Preferences › Interface › Docked panels (also View › Docked Panels).
    pub enabled: bool,
    pub workspace: Workspace<Panel>,
    /// Where each panel's body was drawn last frame.
    pub rects: Vec<(Panel, Rect)>,
    /// The inspector last brought forward: when `ui.inspector_tab` moves away
    /// from it (a ribbon button, a new selection), that panel is opened and
    /// activated.
    pub shown_inspector: InspectorTab,
    /// The bottom tab last brought forward, as `shown_inspector` for
    /// `ui.bottom_tab`.
    pub shown_bottom: BottomTab,
    /// Keys, text and clipboard events typed in a panel's own OS window that no
    /// widget there used: replayed into the main window's next frame so the
    /// editor's shortcuts work from a detached panel too.
    pub forwarded: Vec<egui::Event>,
}

impl Default for DockState {
    fn default() -> Self {
        Self {
            enabled: true,
            workspace: Workspace::new(default_layout()),
            rects: Vec::new(),
            shown_inspector: InspectorTab::default(),
            shown_bottom: BottomTab::default(),
            forwarded: Vec::new(),
        }
    }
}

impl DockState {
    /// Where `panel`'s body was drawn last frame.
    #[must_use]
    pub fn rect(&self, panel: Panel) -> Option<Rect> {
        self.rects
            .iter()
            .find(|(p, _)| *p == panel)
            .map(|(_, r)| *r)
    }

    /// Where the first drawn inspector panel was drawn last frame.
    #[must_use]
    pub fn inspector_rect(&self) -> Option<Rect> {
        self.rects
            .iter()
            .find(|(p, _)| p.inspector().is_some())
            .map(|(_, r)| *r)
    }
}

/// The ribbon and docking settings and the panel arrangement, as the app saves them.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutPrefs {
    /// The ribbon replaces the flat toolbar.
    pub ribbon: bool,
    /// The panels are docked (else the fixed panels).
    pub docked: bool,
    /// The saved `craft_ui::docking::Workspace`; `None` for the default.
    pub workspace: Option<serde_json::Value>,
}

impl Default for LayoutPrefs {
    fn default() -> Self {
        Self {
            ribbon: true,
            docked: true,
            workspace: None,
        }
    }
}

/// A saved workspace fitted to today's panels, or the default when it is
/// missing, does not parse or does not validate.
#[must_use]
pub fn load_workspace(saved: Option<&serde_json::Value>) -> Workspace<Panel> {
    let default = Workspace::new(default_layout());
    let Some(value) = saved else {
        return default;
    };
    let mut value = value.clone();
    forget_unknown_panels(&mut value);
    match serde_json::from_value::<Workspace<Panel>>(value) {
        Ok(mut workspace) if workspace.version <= Workspace::<Panel>::VERSION => {
            undock(&mut workspace.layout);
            if workspace.layout.validate().is_err() {
                log::warn!("saved panel layout ignored: its dock columns don't convert");
                return default;
            }
            workspace.reconcile(|_| true, |p: &Panel| p.permissions(), &default);
            workspace
        }
        Ok(_) | Err(_) => {
            log::warn!("saved panel layout ignored: it does not load in this version");
            default
        }
    }
}

/// Turn the dock columns of a layout saved by an older build into plain
/// splits of the same widths, so the outer edges take full-width drops.
pub fn undock(layout: &mut Layout<Panel>) {
    if let Some(root) = layout.root.take() {
        layout.root = undock_node(root);
    }
}

fn undock_node(node: Node<Panel>) -> Option<Node<Panel>> {
    match node {
        Node::Split {
            axis,
            size,
            first,
            second,
        } => match (undock_node(*first), undock_node(*second)) {
            (Some(first), Some(second)) => Some(Node::Split {
                axis,
                size,
                first: Box::new(first),
                second: Box::new(second),
            }),
            (Some(only), None) | (None, Some(only)) => Some(only),
            (None, None) => None,
        },
        Node::Dock { side, columns } => {
            // Columns run from the window edge inwards.
            let mut built: Option<Node<Panel>> = None;
            for column in columns.into_iter().rev() {
                let width = column.width;
                let Some(col) = column_node(column.groups) else {
                    continue;
                };
                built = Some(match (built, side) {
                    (None, _) => col,
                    (Some(inner), Side::Start) => Node::Split {
                        axis: SplitAxis::Horizontal,
                        size: SplitSize::FixedFirst(width),
                        first: Box::new(col),
                        second: Box::new(inner),
                    },
                    (Some(inner), _) => Node::Split {
                        axis: SplitAxis::Horizontal,
                        size: SplitSize::FixedSecond(width),
                        first: Box::new(inner),
                        second: Box::new(col),
                    },
                });
            }
            built
        }
        leaf => Some(leaf),
    }
}

/// A dock column's groups, top to bottom, as even vertical splits.
fn column_node(groups: Vec<Group<Panel>>) -> Option<Node<Panel>> {
    let mut nodes: Vec<Node<Panel>> = groups
        .into_iter()
        .map(|g| Node::Tabs {
            panels: g.panels,
            active: g.active,
        })
        .collect();
    let mut built = nodes.pop()?;
    let mut below = 1usize;
    while let Some(node) = nodes.pop() {
        below += 1;
        built = Node::Split {
            axis: SplitAxis::Vertical,
            size: SplitSize::Ratio(1.0 / below as f32),
            first: Box::new(node),
            second: Box::new(built),
        };
    }
    Some(built)
}

/// The docking profile: plain splits, with drop zones on each group's sides
/// and a band along every outer edge of the window, where a drop adds a
/// full-width row (top, bottom) or a full-height column (left, right).
fn style(ui: &Ui) -> DockStyle {
    let mut style = DockStyle::from_ui(ui);
    style.side_edge_zones = true;
    style.new_dock_zones = false;
    // craft-ui shrinks tabs that do not fit to a common width and cuts their names ("Ani…").
    // No tab narrower than the longest inspector name: the tabs that still do not fit go to
    // craft-ui's "…" overflow menu, where every name is whole.
    let widest = Panel::INSPECTORS
        .iter()
        .map(|panel| {
            ui.painter()
                .layout_no_wrap(
                    tl!(panel.label()).to_owned(),
                    style.font.clone(),
                    style.text,
                )
                .size()
                .x
        })
        .fold(0.0_f32, f32::max);
    style.min_tab_width = style.min_tab_width.max(widest + TAB_PADDING);
    style
}

/// craft-ui's horizontal padding inside every tab (its `TAB_PADDING`).
const TAB_PADDING: f32 = 24.0;

/// The saved id of every panel this build knows.
fn known_ids() -> Vec<serde_json::Value> {
    Panel::ALL
        .iter()
        .filter_map(|p| serde_json::to_value(p).ok())
        .collect()
}

/// Drop the panels this build does not know (a layout saved by a newer
/// version) so the rest of the layout still loads: they leave their tab
/// groups, a closed one is forgotten, and a place recorded against one is
/// dropped. A group left empty fails validation and the default is used.
pub fn forget_unknown_panels(value: &mut serde_json::Value) {
    use serde_json::Value;
    let known = known_ids();
    // Panel ids are snake_case; craft-ui's own enums (sides, placements) are
    // PascalCase, so a lowercase string that is not a known id is a panel.
    let unknown = |v: &Value| {
        v.as_str().is_some_and(|s| {
            s.chars().next().is_some_and(|c| c.is_ascii_lowercase())
                && s.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        }) && !known.contains(v)
    };
    fn mentions(v: &Value, unknown: &dyn Fn(&Value) -> bool) -> bool {
        match v {
            Value::Object(map) => map.values().any(|v| mentions(v, unknown)),
            Value::Array(items) => items.iter().any(|v| mentions(v, unknown)),
            other => unknown(other),
        }
    }
    fn walk(v: &mut Value, unknown: &dyn Fn(&Value) -> bool) {
        match v {
            Value::Object(map) => {
                if let Some(Value::Array(panels)) = map.get_mut("panels") {
                    panels.retain(|p| !unknown(p));
                    let last = panels.len().saturating_sub(1);
                    if let Some(active) = map.get_mut("active")
                        && active.as_u64().is_some_and(|a| a as usize > last)
                    {
                        *active = Value::from(last);
                    }
                }
                if let Some(Value::Array(hidden)) = map.get_mut("hidden") {
                    hidden.retain(|entry| !mentions(entry, unknown));
                }
                for (_, child) in map.iter_mut() {
                    walk(child, unknown);
                }
            }
            Value::Array(items) => items.iter_mut().for_each(|v| walk(v, unknown)),
            _ => {}
        }
    }
    walk(value, &unknown);
}

/// Whether `panel` is shown in the workspace.
#[must_use]
pub fn is_open(editor: &Editor, panel: Panel) -> bool {
    editor.ui.dock.workspace.layout.contains(&panel)
}

/// Show or hide one panel (View › Panels). Shown panels come back where they
/// were closed; the canvas can't be hidden.
pub fn set_open(editor: &mut Editor, panel: Panel, open: bool) {
    let fallback = default_anchor(editor, panel);
    if let Err(err) = editor
        .ui
        .dock
        .workspace
        .set_visible(panel, open, fallback, |p: &Panel| p.permissions())
    {
        log::debug!("panel {panel:?} not changed: {err:?}");
    }
}

/// Where a panel with no recorded place opens: a settings panel joins the
/// first open inspector's group as a tab, any other panel goes beside the canvas.
fn default_anchor(editor: &Editor, panel: Panel) -> Option<Panel> {
    let inspector = (crate::panels::family(panel) == crate::panels::Family::Settings)
        .then(|| {
            Panel::INSPECTORS
                .into_iter()
                .find(|p| *p != panel && is_open(editor, *p))
        })
        .flatten();
    inspector.or(Some(Panel::Canvas).filter(|p| *p != panel))
}

/// Open `panel` if it is closed, then make it the active tab of its group.
pub fn reveal(editor: &mut Editor, panel: Panel) {
    if is_open(editor, panel) {
        apply(editor, vec![Action::Activate { panel }]);
    } else {
        set_open(editor, panel, true);
    }
}

/// View › Reset Panel Layout: the default arrangement, every panel open.
pub fn reset(editor: &mut Editor) {
    editor.ui.dock.workspace = Workspace::new(default_layout());
}

/// Today's arrangement: slides at the left, the Inspector at the right, the
/// canvas over the timeline between them. Every part is a plain split (no
/// dock columns), so a panel dropped on an outer edge of the window takes
/// the whole width or height there (`style`).
#[must_use]
pub fn default_layout() -> Layout<Panel> {
    let slides = Node::Tabs {
        panels: vec![Panel::Slides],
        active: 0,
    };
    let inspector = Node::Tabs {
        panels: Panel::INSPECTORS.to_vec(),
        active: 0,
    };
    let centre = Node::Split {
        axis: SplitAxis::Vertical,
        size: SplitSize::FixedSecond(220.0),
        first: Box::new(Node::Tabs {
            panels: vec![Panel::Canvas],
            active: 0,
        }),
        second: Box::new(Node::Split {
            axis: SplitAxis::Horizontal,
            size: SplitSize::FixedSecond(crate::LAYERS_PANEL_WIDTH),
            first: Box::new(Node::Tabs {
                panels: Panel::BOTTOM
                    .iter()
                    .copied()
                    .chain([Panel::Notes])
                    .collect(),
                active: 0,
            }),
            second: Box::new(Node::Tabs {
                panels: vec![Panel::Layers],
                active: 0,
            }),
        }),
    };
    Layout {
        root: Some(Node::Split {
            axis: SplitAxis::Horizontal,
            size: SplitSize::FixedFirst(220.0),
            first: Box::new(slides),
            second: Box::new(Node::Split {
                axis: SplitAxis::Horizontal,
                size: SplitSize::FixedSecond(crate::PROPERTIES_PANEL_WIDTH),
                first: Box::new(centre),
                second: Box::new(inspector),
            }),
        }),
        floating: Vec::new(),
    }
}

struct Content<'a> {
    editor: &'a mut Editor,
    rects: Vec<(Panel, Rect)>,
}

impl DockContent<Panel> for Content<'_> {
    fn body(&mut self, ui: &mut Ui, panel: &Panel) {
        self.rects.push((*panel, ui.max_rect()));
        match panel {
            Panel::Slides => self.editor.slides_body(ui),
            Panel::Canvas => self.editor.canvas_body(ui),
            Panel::Format | Panel::Animate | Panel::SlideInspector | Panel::CourseInspector => {
                if let Some(tab) = panel.inspector() {
                    self.editor.inspector_body(ui, Some(tab));
                }
            }
            Panel::Text | Panel::GridGuides | Panel::SizePosition | Panel::ShapeStyle => {
                crate::panels::body(self.editor, ui, *panel);
            }
            Panel::Layers => {
                if self.editor.ui.master_view {
                    ui.add_space(8.0);
                    ui.label(tl!("Slide masters have no layers."));
                } else {
                    self.editor.layers_body(ui);
                }
            }
            Panel::Notes => crate::properties::show_notes_panel(self.editor, ui),
            Panel::Timeline => self.editor.timeline_body(ui),
            Panel::States | Panel::Triggers | Panel::Variables | Panel::Question => {
                if let Some(tab) = panel.bottom_tab() {
                    self.editor.bottom_tab_body(ui, tab);
                }
            }
        }
    }

    fn logical_label(&self, panel: &Panel) -> String {
        tl!(panel.label()).to_owned()
    }

    fn permissions(&self, panel: &Panel) -> Permissions {
        panel.permissions()
    }

    fn limits(&self, panel: &Panel) -> PanelLimits {
        panel.limits()
    }
}

/// Draw the docked panels into `ui` (the space between the top and bottom bars)
/// and apply the user's layout changes.
pub fn show(editor: &mut Editor, ui: &mut Ui) {
    reveal_inspector(editor);
    reveal_bottom_tab(editor);
    crate::panels::follow_work(editor);
    let style = style(ui);
    let layout = editor.ui.dock.workspace.layout.clone();
    let mut content = Content {
        editor,
        rects: Vec::new(),
    };
    let output = DockArea::new(egui::Id::unique("coursecraft_dock")).show_content(
        ui,
        &layout,
        &style,
        &mut content,
    );
    let Content { editor, rects } = content;
    editor.ui.dock.rects = rects;
    editor.ui.dock.forwarded.extend(output.forwarded_events);
    follow_activated_inspector(editor, &output.actions);
    follow_activated_bottom_tab(editor, &output.actions);
    apply(editor, output.actions);
    if let Some(err) = output.error {
        log::debug!("dock gesture refused: {err:?}");
    }
    record_timeline(editor, ui);
}

/// Replay the keys a detached panel's OS window passed on (see
/// [`DockState::forwarded`]) into this frame's input, before the shortcuts read it.
pub fn replay_forwarded(editor: &mut Editor, ctx: &egui::Context) {
    if editor.ui.dock.forwarded.is_empty() {
        return;
    }
    let events = std::mem::take(&mut editor.ui.dock.forwarded);
    ctx.input_mut(|i| i.events.extend(events));
}

/// Apply layout changes through the workspace, all or none.
pub fn apply(editor: &mut Editor, actions: Vec<Action<Panel>>) {
    if actions.is_empty() {
        return;
    }
    if let Err(err) = editor
        .ui
        .dock
        .workspace
        .apply_all(actions, |p: &Panel| p.permissions())
    {
        log::debug!("dock change refused: {err:?}");
    }
}

/// Keep the timeline's rect and focus up to date, as its fixed panel does.
fn record_timeline(editor: &mut Editor, ui: &Ui) {
    let rect = editor.ui.dock.rect(Panel::Timeline);
    editor.ui.timeline.panel_rect = rect;
    if let Some(rect) = rect {
        editor.ui.timeline.panel_height = rect.height();
        if ui.input(|i| i.pointer.primary_pressed())
            && let Some(pos) = ui.input(|i| i.pointer.interact_pos())
        {
            editor.ui.timeline.focused = rect.contains(pos);
        }
    }
}

/// Bring the inspector `ui.inspector_tab` names forward when it changed since
/// the last frame: opened if closed, then made the active tab of its group.
pub fn reveal_inspector(editor: &mut Editor) {
    let tab = editor.ui.inspector_tab;
    if editor.ui.dock.shown_inspector == tab {
        return;
    }
    editor.ui.dock.shown_inspector = tab;
    let panel = Panel::for_inspector(tab);
    if !is_open(editor, panel) {
        set_open(editor, panel, true);
    }
    apply(editor, vec![Action::Activate { panel }]);
}

/// Ask for the inspector `tab`: it becomes the current one and, with docking on, comes forward
/// at the start of the next frame even when it was already current but closed or behind
/// another panel's tab (`N98`). The reveal waits for the next frame, as the panels are being
/// drawn now.
pub fn request_inspector_tab(editor: &mut Editor, tab: InspectorTab) {
    editor.ui.inspector_tab = tab;
    if editor.ui.dock.shown_inspector == tab {
        // Forget what was brought forward, so that `reveal_inspector` acts again.
        editor.ui.dock.shown_inspector = if tab == InspectorTab::Slide {
            InspectorTab::Course
        } else {
            InspectorTab::Slide
        };
    }
}

/// Ask for the bottom `tab`: as [`request_inspector_tab`], for the bottom panels (`N98`).
pub fn request_bottom_tab(editor: &mut Editor, tab: BottomTab) {
    editor.ui.bottom_tab = tab;
    if editor.ui.dock.shown_bottom == tab {
        editor.ui.dock.shown_bottom = if tab == BottomTab::Timeline {
            BottomTab::States
        } else {
            BottomTab::Timeline
        };
    }
}

/// A click on an inspector's dock tab makes it the current inspector, so the
/// ribbon and the selection follow the user's choice.
fn follow_activated_inspector(editor: &mut Editor, actions: &[Action<Panel>]) {
    for action in actions {
        if let Action::Activate { panel } = action
            && let Some(tab) = panel.inspector()
        {
            editor.ui.inspector_tab = tab;
            editor.ui.dock.shown_inspector = tab;
        }
    }
}

/// Bring the bottom panel `ui.bottom_tab` names forward when it changed since
/// the last frame (a menu item, Find, a states shortcut): opened if closed,
/// then made the active tab of its group.
pub fn reveal_bottom_tab(editor: &mut Editor) {
    let tab = editor.ui.bottom_tab;
    if editor.ui.dock.shown_bottom == tab {
        return;
    }
    editor.ui.dock.shown_bottom = tab;
    reveal(editor, Panel::for_bottom_tab(tab));
}

/// A click on a bottom panel's dock tab makes it the current bottom tab, so
/// the shortcuts that act on one tab (C adds a cue point on the timeline)
/// follow the user's choice.
fn follow_activated_bottom_tab(editor: &mut Editor, actions: &[Action<Panel>]) {
    for action in actions {
        if let Action::Activate { panel } = action
            && let Some(tab) = panel.bottom_tab()
        {
            editor.ui.bottom_tab = tab;
            editor.ui.dock.shown_bottom = tab;
        }
    }
}
