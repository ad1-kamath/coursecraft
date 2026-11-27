//! The ribbon: tab strip and the open tab's groups, drawn from [`spec`].
//!
//! M37 step 1 (docs/ui/implementation.md § 6): the ribbon is drawn beside the old
//! menu bar and toolbar, behind View › Ribbon. Every control comes from the spec;
//! one whose feature is missing is drawn disabled with its reason in the tooltip.

pub mod contextual;
pub mod effect_art;
pub mod gallery;
pub mod spec;
pub mod widgets;

#[cfg(test)]
mod contextual_tests;
#[cfg(test)]
mod gallery_tests;
#[cfg(test)]
mod spec_tests;

use coursecraft_render::Icon;
use egui::{Align2, FontId, Rect, Response, Sense, Ui, pos2, vec2};
use serde_json::json;

use crate::properties::InspectorTab;
use crate::{Editor, EditorRequest};
use coursecraft_render::ThemePref;
use spec::{Action, Group, Item, Kind, Menu, Tab, TabWhen, UiAction, UiMenu};
use widgets::{CAPTION_H, CONTROLS_H, SMALL_H};

/// Height of the tab strip.
pub const STRIP_H: f32 = 26.0;
/// Height of the whole ribbon when open.
pub const RIBBON_H: f32 = STRIP_H + CONTROLS_H + CAPTION_H + 8.0;

/// What the ribbon shows.
#[derive(Debug, Clone, PartialEq)]
pub struct RibbonState {
    /// Drawn instead of the flat toolbar (Preferences › Interface › Ribbon).
    pub visible: bool,
    /// The open tab's id.
    pub tab: &'static str,
    /// Only the tab strip is shown.
    pub collapsed: bool,
    /// Each group's drawn width (group id, width), measured when it was last
    /// drawn whole; groups that don't fit shrink to one button, rightmost first.
    pub group_widths: Vec<(&'static str, f32)>,
    /// Where the ribbon was drawn last frame: a click there leaves the text
    /// being edited on the canvas open, so Bold acts on it.
    pub rect: Rect,
    /// What the text controls act on this frame (`crate::text_format`).
    pub text: Result<crate::text_format::TextState, &'static str>,
    /// Master View was open last frame (it opens on the Slide Master tab).
    pub in_master: bool,
    /// The current slide's object ids last frame: a selected id missing there
    /// was just inserted, and opens its contextual tab.
    pub seen_objects: Option<(String, Vec<String>)>,
    /// The tab whose single click just opened it (or expanded the ribbon), and
    /// when: the second click of that double-click must not collapse it again.
    pub tab_opened: Option<(&'static str, f64)>,
    /// The colour Font Color applies when its main part is clicked (`#RRGGBB`), the last one
    /// picked from the ribbon. Unset: the theme accent.
    pub last_font_color: Option<String>,
    /// The colour Highlight applies when its main part is clicked. Unset: yellow.
    pub last_highlight: Option<String>,
}

/// Longest gap between the two clicks of a double-click on a tab.
const DOUBLE_CLICK_SECS: f64 = 0.6;

impl Default for RibbonState {
    fn default() -> Self {
        Self {
            visible: true,
            tab: "home",
            collapsed: false,
            group_widths: Vec::new(),
            rect: Rect::NOTHING,
            text: Err(""),
            seen_objects: None,
            tab_opened: None,
            in_master: false,
            last_font_color: None,
            last_highlight: None,
        }
    }
}

/// Height the ribbon panel needs now.
#[must_use]
pub fn height(state: &RibbonState) -> f32 {
    if state.collapsed {
        STRIP_H + 4.0
    } else {
        RIBBON_H
    }
}

/// The tooltip of a disabled control.
#[must_use]
pub fn disabled_reason(action: Action) -> Option<String> {
    match action {
        Action::NotYet(reason) => Some(craft_i18n::fmt(
            tl!("Not available yet: {reason}"),
            &[("reason", tl!(reason))],
        )),
        Action::NotPlanned(reason) => Some(craft_i18n::fmt(
            tl!("Not planned: {reason}"),
            &[("reason", tl!(reason))],
        )),
        Action::Command(_) | Action::Ui(_) | Action::Text(_) => None,
    }
}

/// Whether `action` can run now, with the reason when it cannot.
pub fn availability(
    editor: &Editor,
    action: Action,
    blocked: Option<&'static str>,
) -> Result<(), String> {
    if let Some(reason) = disabled_reason(action) {
        return Err(reason);
    }
    if let Some(reason) = blocked {
        return Err(tl!(reason).to_owned());
    }
    match action {
        Action::Command(id) => {
            if crate::menu::adapter(editor, id).is_some() {
                Ok(())
            } else {
                Err(tl!(crate::menu::adapter_reason(editor, id)).to_owned())
            }
        }
        Action::Ui(ui_action) => ui_available(editor, ui_action).map_err(|r| tl!(r).to_owned()),
        Action::Text(_) => match &editor.ui.ribbon.text {
            Ok(_) => Ok(()),
            Err(reason) => Err(tl!(reason).to_owned()),
        },
        Action::NotYet(_) | Action::NotPlanned(_) => Ok(()),
    }
}

/// Select All needs the slide editor.
fn select_all_available(editor: &Editor) -> Result<(), &'static str> {
    if editor.ui.story_view {
        Err("Open a slide first")
    } else {
        Ok(())
    }
}

pub(crate) const MASTER: &str = "Not available in Master View";

pub(crate) const STATE: &str = "Not available inside a state";

/// The user documentation, on the project's site.
const GUIDE_URL: &str = "https://github.com/ad1-kamath/coursecraft/tree/main/docs";

/// Whether the Format or Animation Painter can be clicked now.
fn painter_available(
    editor: &Editor,
    kind: crate::format_painter::PainterKind,
) -> Result<(), &'static str> {
    let armed = editor
        .ui
        .format_painter
        .as_ref()
        .is_some_and(|p| p.kind == kind);
    if editor.ui.editing_state.is_some() {
        Err(crate::STATE_LOCK_TOOLTIP)
    } else if editor.ui.master_view {
        Err(MASTER)
    } else if armed || editor.can_arm_format_painter() {
        Ok(())
    } else if kind == crate::format_painter::PainterKind::Animation {
        Err("Select one object to copy its animations")
    } else {
        Err("Select an object to copy its format")
    }
}

fn focus_order_available(editor: &Editor) -> Result<(), &'static str> {
    if editor.ui.master_view {
        Err(MASTER)
    } else if editor.ui.editing_state.is_some() {
        Err(STATE)
    } else if editor
        .session
        .course
        .slide(&editor.selection.slide)
        .is_some()
    {
        Ok(())
    } else {
        Err("Select a slide first")
    }
}

/// A slide dialog needs a slide, in neither a master nor a state.
fn slide_dialog_available(editor: &Editor) -> Result<(), &'static str> {
    if editor.ui.master_view {
        Err(MASTER)
    } else if editor.ui.editing_state.is_some() {
        Err(STATE)
    } else if editor
        .session
        .course
        .slide(&editor.selection.slide)
        .is_none()
    {
        Err("Select a slide first")
    } else {
        Ok(())
    }
}

pub(crate) fn ui_available(editor: &Editor, action: UiAction) -> Result<(), &'static str> {
    let master = editor.ui.master_view;
    let state = editor.ui.editing_state.is_some();
    match action {
        UiAction::Paste
        | UiAction::PasteKeepSource
        | UiAction::PasteTextOnly
        | UiAction::PasteSpecial => crate::menu::clipboard_rules(editor).paste,
        UiAction::FocusOrder => focus_order_available(editor),
        UiAction::SelectAll => select_all_available(editor),
        UiAction::Cut => crate::menu::clipboard_rules(editor).cut,
        UiAction::Copy => crate::menu::clipboard_rules(editor).copy,
        UiAction::AnimationPainter => {
            painter_available(editor, crate::format_painter::PainterKind::Animation)
        }
        UiAction::FormatPainter => {
            painter_available(editor, crate::format_painter::PainterKind::Format)
        }
        UiAction::NewSlide | UiAction::NewScene | UiAction::Preview => {
            if master {
                Err(MASTER)
            } else {
                Ok(())
            }
        }
        UiAction::DuplicateSlide => {
            if master {
                Err(MASTER)
            } else if editor
                .session
                .course
                .slide(&editor.selection.slide)
                .is_some()
            {
                Ok(())
            } else {
                Err("Select a slide first")
            }
        }
        UiAction::Arrange => {
            if state {
                // Order and alignment are shared across all states (`N95`).
                Err(STATE)
            } else if editor.selection.object.is_some() {
                Ok(())
            } else {
                Err("Select an object first")
            }
        }
        UiAction::Publish => {
            if editor.file_ops_available {
                Ok(())
            } else {
                Err(crate::menu::FILE_OPS_DISABLED_MSG)
            }
        }
        UiAction::NarrationFromText | UiAction::RecordNarration => {
            if master {
                Err(MASTER)
            } else if state {
                Err(STATE)
            } else {
                Ok(())
            }
        }
        UiAction::FindReplace
        | UiAction::AltText
        | UiAction::Spelling
        | UiAction::FormatInspector
        | UiAction::Player => Ok(()),
        UiAction::SlideLayer | UiAction::Trigger | UiAction::Question | UiAction::Interaction => {
            slide_dialog_available(editor)
        }
        UiAction::InsertSlideNumber
        | UiAction::InsertSlideNumberOfTotal
        | UiAction::InsertSymbol
        | UiAction::InsertReference => {
            // The pickers open anywhere; the text goes in when the author chooses it.
            if matches!(action, UiAction::InsertSymbol | UiAction::InsertReference) {
                Ok(())
            } else {
                crate::insert_text::available(editor)
            }
        }
        UiAction::Table
        | UiAction::Button
        | UiAction::Audio
        | UiAction::Video
        | UiAction::Lottie
        | UiAction::WebObject
        | UiAction::InputText
        | UiAction::InputCheckbox
        | UiAction::InputRadio
        | UiAction::Slider
        | UiAction::Dial
        | UiAction::Marker
        | UiAction::ScrollPanel => {
            if state {
                Err(STATE)
            } else if master && !matches!(action, UiAction::Button) {
                Err(MASTER)
            } else {
                Ok(())
            }
        }
        UiAction::ResultsSlide | UiAction::Certificate | UiAction::CourseMap => {
            if master {
                Err(MASTER)
            } else {
                Ok(())
            }
        }
        UiAction::Picture
        | UiAction::Shape
        | UiAction::Caption
        | UiAction::TextBox
        | UiAction::MediaLibrary
        | UiAction::SlideView
        | UiAction::SlideMaster
        | UiAction::ZoomFit
        | UiAction::ZoomIn
        | UiAction::ZoomOut
        | UiAction::Zoom100
        | UiAction::Rulers
        | UiAction::Grid
        | UiAction::Guides
        | UiAction::SnapToGrid
        | UiAction::SnapToGuides
        | UiAction::ThemeLight
        | UiAction::ThemeDark
        | UiAction::ThemeSystem
        | UiAction::ImportPptx
        | UiAction::ImportQuestions
        | UiAction::ImportDocument
        | UiAction::ImportOutline => Ok(()),
        UiAction::AddMotionPath => {
            if master {
                Err(MASTER)
            } else if state {
                Err(STATE)
            } else if editor.selection.object.is_none() {
                Err("Select an object first")
            } else {
                Ok(())
            }
        }
        UiAction::Automation => {
            if editor.automation_available {
                Ok(())
            } else {
                Err("Automation is desktop-only")
            }
        }
        UiAction::SlideSize
        | UiAction::Themes
        | UiAction::ThemeFonts
        | UiAction::SlideInspector
        | UiAction::AnimateInspector
        | UiAction::Guide
        | UiAction::Shortcuts
        | UiAction::Welcome
        | UiAction::GitHub
        | UiAction::ReportProblem
        | UiAction::SuggestFeature
        | UiAction::CheckUpdates
        | UiAction::ConnectAgent
        | UiAction::AiSettings => Ok(()),
        UiAction::PanelAt(..) => Ok(()),
        UiAction::Context(op) => contextual::available(editor, op),
        UiAction::Gallery(g) => gallery::available(editor, g),
        UiAction::InsertMaster => Ok(()),
        UiAction::DeleteMaster | UiAction::RenameMaster => {
            if editor.ui.master_view
                && editor
                    .session
                    .course
                    .master(&editor.selection.slide)
                    .is_some()
            {
                Ok(())
            } else {
                Err("Select a slide master first")
            }
        }
        UiAction::Effect(op) => gallery::effect_available(editor, op),
        UiAction::Panel(panel)
            if crate::panels::family(panel) == crate::panels::Family::Settings =>
        {
            Ok(())
        }
        UiAction::Panel(_) | UiAction::ResetPanels => {
            if editor.ui.dock.enabled {
                Ok(())
            } else {
                Err("Turn on Docked Panels first")
            }
        }
    }
}

/// Run an enabled control's engine command.
pub(crate) fn run_command(editor: &mut Editor, id: &'static str) {
    if id == "object.duplicate" && editor.selection.is_multi() {
        // Every selected object, as Ctrl+D does (`N97`).
        editor.duplicate_selection();
    } else if let Some(params) = crate::menu::adapter(editor, id) {
        editor.run(id, params);
    }
}

/// Run an enabled control's action.
fn run(editor: &mut Editor, action: Action, resp: &Response, requests: &mut Vec<EditorRequest>) {
    match action {
        Action::Command(id) => run_command(editor, id),
        Action::Ui(ui_action) => run_ui(editor, ui_action, resp, requests),
        Action::Text(op) => {
            if let Some(change) = text_change(editor, op) {
                apply_text_change(editor, &change);
            }
        }
        Action::NotYet(_) | Action::NotPlanned(_) => {}
    }
}

fn run_ui(
    editor: &mut Editor,
    action: UiAction,
    resp: &Response,
    requests: &mut Vec<EditorRequest>,
) {
    match action {
        UiAction::Paste => crate::menu::paste_now(editor, &resp.ctx),
        UiAction::PasteKeepSource => crate::clipboard::paste_now_with(
            editor,
            &resp.ctx,
            crate::clipboard::PasteMode::KeepSource,
        ),
        UiAction::PasteTextOnly => crate::clipboard::paste_now_with(
            editor,
            &resp.ctx,
            crate::clipboard::PasteMode::TextOnly,
        ),
        UiAction::PasteSpecial => editor.open_paste_special(),
        UiAction::FocusOrder => editor.open_focus_order(),
        UiAction::Cut => crate::clipboard::cut(editor),
        UiAction::Copy => crate::clipboard::copy(editor),
        UiAction::FormatPainter => crate::toolbar::format_painter_clicked(editor, resp),
        UiAction::AnimationPainter => crate::toolbar::painter_clicked(
            editor,
            resp,
            crate::format_painter::PainterKind::Animation,
        ),
        UiAction::NewSlide => crate::toolbar::add_new_slide(editor),
        UiAction::NewScene => crate::toolbar::add_new_scene(editor),
        UiAction::DuplicateSlide => {
            let slide = editor.selection.slide.clone();
            editor.run("slide.duplicate", json!({ "slide": slide }));
        }
        UiAction::NarrationFromText => editor.open_speech_dialog(),
        UiAction::FindReplace => crate::find::open(editor),
        UiAction::SelectAll => editor.select_all_objects(),
        UiAction::AltText => editor.open_alt_text_dialog(),
        UiAction::Spelling => crate::spell_check::open(editor),
        UiAction::FormatInspector => {
            crate::dock::request_inspector_tab(editor, InspectorTab::Format)
        }
        // Its menu opens on click.
        UiAction::Arrange => {}
        UiAction::Player => editor.ui.show_course_settings_window = true,
        UiAction::Preview => crate::toolbar::preview_slide(editor, requests),
        UiAction::Publish => editor.open_publish_dialog(),
        UiAction::SlideLayer => crate::toolbar::add_new_layer(editor),
        UiAction::Picture => crate::toolbar::insert_image(editor, requests),
        UiAction::MediaLibrary => editor.ui.show_media_library = true,
        UiAction::Shape => {
            editor.ui.shape_gallery_category = None;
            editor.ui.show_shape_gallery_window = true;
        }
        UiAction::Caption => {
            editor.ui.shape_gallery_category = Some(coursecraft_course::ShapeCategory::Callouts);
            editor.ui.show_shape_gallery_window = true;
        }
        UiAction::Video => crate::toolbar::insert_video(editor, requests),
        UiAction::Audio => crate::toolbar::insert_audio(editor, requests),
        UiAction::RecordNarration => editor.open_record_narration(),
        UiAction::WebObject => {
            editor.ui.web_error = None;
            editor.ui.show_web_window = true;
        }
        UiAction::Lottie => crate::toolbar::insert_lottie(editor, requests),
        UiAction::TextBox => crate::toolbar::insert_text_box(editor),
        UiAction::InsertSlideNumber => {
            let _ = crate::insert_text::insert_slide_number(editor, false);
        }
        UiAction::InsertSlideNumberOfTotal => {
            let _ = crate::insert_text::insert_slide_number(editor, true);
        }
        UiAction::InsertSymbol => crate::insert_text::open_symbol_picker(editor),
        UiAction::InsertReference => crate::insert_text::open_reference_picker(editor),
        UiAction::Table => crate::toolbar::insert_table(editor),
        UiAction::Button => crate::toolbar::insert_button(editor),
        UiAction::Slider => drop(crate::input_insert::insert_slider(editor)),
        UiAction::Dial => drop(crate::input_insert::insert_dial(editor)),
        UiAction::InputText => drop(crate::input_insert::insert_text_entry(editor)),
        UiAction::InputCheckbox => drop(crate::input_insert::insert_checkbox(editor)),
        UiAction::InputRadio => drop(crate::input_insert::insert_radio_group(editor)),
        UiAction::Marker => drop(crate::interaction_insert::insert_marker(editor)),
        UiAction::ScrollPanel => drop(crate::interaction_insert::insert_scroll_panel(editor)),
        UiAction::Trigger => crate::toolbar::insert_trigger(editor),
        UiAction::Interaction => editor.ui.show_interaction_gallery = true,
        // Its menu lists the kinds; a slide with a question opens its panel.
        UiAction::Question => editor.ui.bottom_tab = crate::BottomTab::Question,
        UiAction::ResultsSlide => crate::toolbar::insert_results_slide(editor),
        UiAction::Certificate => crate::toolbar::insert_certificate_slide(editor),
        UiAction::ImportPptx => requests.push(EditorRequest::PickPptx),
        UiAction::ImportQuestions => requests.push(EditorRequest::ImportQuestionsCsv),
        UiAction::ImportDocument => editor.open_document_dialog(),
        UiAction::ImportOutline => editor.open_outline_dialog(),
        UiAction::SlideMaster => editor.toggle_master_view(),
        UiAction::CourseMap => editor.ui.story_view = true,
        UiAction::SlideView => {
            editor.ui.story_view = false;
            if editor.ui.master_view {
                editor.toggle_master_view();
            }
        }
        UiAction::ZoomFit => editor.ui.zoom = crate::ZoomLevel::Fit,
        UiAction::ZoomIn => editor.zoom_in(),
        UiAction::ZoomOut => editor.zoom_out(),
        UiAction::Zoom100 => editor.ui.zoom = crate::ZoomLevel::Percent100,
        UiAction::Rulers => view_toggle(
            editor,
            "show_rulers",
            !editor.session.course.editor.show_rulers,
        ),
        UiAction::Grid => view_toggle(editor, "show_grid", !editor.session.course.editor.show_grid),
        UiAction::Guides => view_toggle(
            editor,
            "show_guides",
            !editor.session.course.editor.show_guides,
        ),
        UiAction::SnapToGrid => view_toggle(
            editor,
            "snap_to_grid",
            !editor.session.course.editor.snap_to_grid,
        ),
        UiAction::SnapToGuides => editor.ui.snap_to_guides = !editor.ui.snap_to_guides,
        UiAction::Panel(panel) => toggle_panel(editor, panel),
        UiAction::PanelAt(panel, section) => crate::panels::open_at(editor, panel, section),
        UiAction::ResetPanels => crate::dock::reset(editor),
        UiAction::ThemeLight => set_theme(editor, ThemePref::Light, requests),
        UiAction::ThemeDark => set_theme(editor, ThemePref::Dark, requests),
        UiAction::ThemeSystem => set_theme(editor, ThemePref::System, requests),
        UiAction::SlideSize => editor.open_slide_size_dialog(),
        UiAction::Themes => editor.ui.show_theme_window = true,
        UiAction::ThemeFonts => crate::theme_dialog::open_theme_fonts_dialog(editor),
        UiAction::SlideInspector => crate::dock::request_inspector_tab(editor, InspectorTab::Slide),
        UiAction::AnimateInspector => {
            crate::dock::request_inspector_tab(editor, InspectorTab::Animate)
        }
        UiAction::AddMotionPath => {
            if let Some(object) = editor.selection.object.clone() {
                let slide = editor.selection.slide.clone();
                editor.run(
                    "object.add_motion_path",
                    json!({ "slide": slide, "object": object }),
                );
                crate::dock::request_inspector_tab(editor, InspectorTab::Animate);
            }
        }
        UiAction::Guide => crate::report::open_link(&resp.ctx, GUIDE_URL),
        UiAction::Shortcuts => editor.ui.show_shortcuts_dialog = true,
        UiAction::Welcome => {
            editor.ui.file_page.section = crate::file_page::FileSection::Start;
            editor.ui.show_welcome = true;
        }
        UiAction::GitHub => crate::report::open_link(&resp.ctx, crate::project_links::REPO_URL),
        UiAction::ReportProblem => editor.open_report(),
        UiAction::SuggestFeature => crate::report::open_feature_request(&resp.ctx),
        // The desktop app checks and reports; the web build shows the releases page.
        UiAction::CheckUpdates if editor.updates_available => {
            requests.push(EditorRequest::CheckForUpdates);
        }
        UiAction::CheckUpdates => {
            crate::report::open_link(&resp.ctx, &crate::project_links::releases_url());
        }
        UiAction::ConnectAgent => editor.open_agent_settings(),
        UiAction::AiSettings => editor.open_ai_settings(),
        UiAction::Automation => {
            editor.automation_enabled = !editor.automation_enabled;
            requests.push(EditorRequest::SetAutomation(editor.automation_enabled));
        }
        UiAction::Context(op) => contextual::run(editor, op, requests),
        // A gallery acts from its tiles.
        UiAction::Gallery(_) => {}
        UiAction::InsertMaster => editor.create_master(),
        UiAction::DeleteMaster => {
            let id = editor.selection.slide.clone();
            crate::slides::request_master_delete(editor, &id);
        }
        UiAction::RenameMaster => {
            let id = editor.selection.slide.clone();
            crate::slides::start_master_rename(editor, &id);
        }
        UiAction::Effect(op) => gallery::run_effect(editor, op),
    }
}

fn view_toggle(editor: &mut Editor, key: &str, on: bool) {
    editor.run("view.set", json!({ key: on }));
}

fn toggle_panel(editor: &mut Editor, panel: crate::dock::Panel) {
    if crate::panels::is_shown(editor, panel) {
        crate::panels::close(editor, panel);
    } else {
        crate::panels::open_at(editor, panel, None);
    }
}

fn set_theme(editor: &mut Editor, pref: ThemePref, requests: &mut Vec<EditorRequest>) {
    editor.theme_pref = pref;
    requests.push(EditorRequest::SetTheme(pref));
}

/// Whether a toggle-like control shows as on.
fn ui_selected(editor: &Editor, action: UiAction) -> bool {
    let view = &editor.session.course.editor;
    match action {
        UiAction::FormatPainter => editor
            .ui
            .format_painter
            .as_ref()
            .is_some_and(|p| p.kind == crate::format_painter::PainterKind::Format),
        UiAction::AnimationPainter => editor.animation_painter_armed(),
        UiAction::TextBox => editor.ui.placement_tool == Some(crate::PlacementTool::TextBox),
        UiAction::Button => editor.ui.placement_tool == Some(crate::PlacementTool::Button),
        UiAction::Shape => matches!(
            editor.ui.placement_tool,
            Some(crate::PlacementTool::Shape(_) | crate::PlacementTool::Draw(_))
        ),
        UiAction::Caption => matches!(
            editor.ui.placement_tool,
            Some(crate::PlacementTool::Shape(kind))
                if coursecraft_course::SHAPE_CATALOG
                    .iter()
                    .any(|info| info.kind == kind && info.category == coursecraft_course::ShapeCategory::Callouts)
        ),
        UiAction::CourseMap => editor.ui.story_view,
        UiAction::SlideView => !editor.ui.story_view && !editor.ui.master_view,
        UiAction::SlideMaster => editor.ui.master_view,
        UiAction::ZoomFit => editor.ui.zoom == crate::ZoomLevel::Fit,
        UiAction::Zoom100 => editor.ui.zoom == crate::ZoomLevel::Percent100,
        UiAction::Rulers => view.show_rulers,
        UiAction::Grid => view.show_grid,
        UiAction::Guides => view.show_guides,
        UiAction::SnapToGrid => view.snap_to_grid,
        UiAction::SnapToGuides => editor.ui.snap_to_guides,
        UiAction::Panel(panel) => crate::panels::is_shown(editor, panel),
        UiAction::ThemeLight => editor.theme_pref == ThemePref::Light,
        UiAction::ThemeDark => editor.theme_pref == ThemePref::Dark,
        UiAction::ThemeSystem => editor.theme_pref == ThemePref::System,
        UiAction::Automation => editor.automation_available && editor.automation_enabled,
        UiAction::Context(op) => contextual::selected(editor, op),
        UiAction::Effect(op) => gallery::effect_selected(editor, op),
        _ => false,
    }
}

/// Draw the ribbon into `ui` (a top panel of [`height`]).
pub fn show(editor: &mut Editor, ui: &mut Ui, requests: &mut Vec<EditorRequest>) {
    if spec::tab(editor.ui.ribbon.tab).is_none() {
        editor.ui.ribbon.tab = "home";
    }
    follow_master_view(editor);
    if !collapse_shortcut_blocked(editor, ui.ctx())
        && ui.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::F1))
    {
        editor.ui.ribbon.collapsed = !editor.ui.ribbon.collapsed;
    }
    contextual::follow_selection(editor);
    editor.ui.ribbon.rect = ui.max_rect();
    editor.ui.ribbon.text = crate::text_format::targets(editor).and_then(|_| {
        crate::text_format::state(editor).ok_or("Select a text box, or a shape that holds text")
    });
    tab_strip(editor, ui, requests);
    if editor.ui.ribbon.collapsed {
        return;
    }
    let Some(tab) = spec::tab(editor.ui.ribbon.tab) else {
        return;
    };
    let area = Rect::from_min_max(
        pos2(ui.max_rect().min.x, ui.max_rect().min.y + STRIP_H + 2.0),
        ui.max_rect().max,
    );
    let available = area.width() - 4.0;
    let mut body = ui.new_child(egui::UiBuilder::new().max_rect(area));
    egui::ScrollArea::horizontal()
        .id_salt("ribbon_groups")
        .show(&mut body, |ui| {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing = vec2(2.0, 0.0);
                show_tab(editor, ui, tab, available, requests);
            });
        });
}

/// Ctrl+F1 leaves the ribbon alone while a text field has focus, a modal dialog
/// or the File page is open, like the other file shortcuts.
fn collapse_shortcut_blocked(editor: &Editor, ctx: &egui::Context) -> bool {
    ctx.egui_wants_keyboard_input()
        || editor.ui.inline_edit.is_some()
        || editor.ui.show_welcome
        || editor.ui.file_page.open
        || editor.ui.show_shortcuts_dialog
        || crate::dialog::any_open(ctx)
}

/// Master View opens on the Slide Master tab and shows only the tabs that
/// apply to masters; leaving it goes back to Home.
fn follow_master_view(editor: &mut Editor) {
    let tab = editor.ui.ribbon.tab;
    let was = editor.ui.ribbon.in_master;
    let now = editor.ui.master_view;
    editor.ui.ribbon.in_master = now;
    if now {
        let shown = tab == spec::MASTER_TAB.id
            || spec::MASTER_STRIP.contains(&tab)
            || spec::context_tab_of(tab).is_some();
        if !was || !shown {
            editor.ui.ribbon.tab = spec::MASTER_TAB.id;
        }
    } else if tab == spec::MASTER_TAB.id {
        editor.ui.ribbon.tab = "home";
    }
}

/// The main tabs of the strip now, in order.
fn main_tabs(editor: &Editor) -> Vec<&'static Tab> {
    if editor.ui.master_view {
        std::iter::once(&spec::MASTER_TAB)
            .chain(
                spec::TABS
                    .iter()
                    .filter(|t| spec::MASTER_STRIP.contains(&t.id)),
            )
            .collect()
    } else {
        spec::TABS.iter().collect()
    }
}

/// The quick access toolbar: Save, Undo, Redo, left of the tab strip.
/// Returns its right edge.
fn quick_access(editor: &mut Editor, ui: &mut Ui, requests: &mut Vec<EditorRequest>) -> f32 {
    let top = ui.max_rect().min;
    let rect = Rect::from_min_size(top, vec2(120.0, STRIP_H));
    let mut bar = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    bar.spacing_mut().item_spacing.x = 2.0;
    let save_tip = format!(
        "{} ({})",
        tl!("Save"),
        crate::menu::shortcut_label(ui.ctx(), "Ctrl+S")
    );
    let save_tip = if editor.file_ops_available {
        save_tip
    } else {
        tl!(crate::menu::FILE_OPS_DISABLED_MSG).to_owned()
    };
    if crate::widgets::icon_button_enabled(
        &mut bar,
        Icon::FloppyDisk,
        editor.file_ops_available,
        &save_tip,
    )
    .clicked()
    {
        requests.push(EditorRequest::Save);
    }
    for (id, icon, label, shortcut) in [
        ("edit.undo", Icon::ArrowCounterClockwise, "Undo", "Ctrl+Z"),
        ("edit.redo", Icon::ArrowClockwise, "Redo", "Ctrl+Y"),
    ] {
        let params = crate::menu::adapter(editor, id);
        let tip = format!(
            "{} ({})",
            tl!(label),
            crate::menu::shortcut_label(ui.ctx(), shortcut)
        );
        if crate::widgets::icon_button_enabled(&mut bar, icon, params.is_some(), &tip).clicked()
            && let Some(params) = params
        {
            editor.run(id, params);
        }
    }
    if crate::widgets::icon_button(&mut bar, Icon::ListBullets, tl!("Undo History…")).clicked() {
        editor.ui.show_history = true;
    }
    let right = bar.min_rect().max.x + 6.0;
    ui.painter().line_segment(
        [
            pos2(right - 3.0, top.y + 6.0),
            pos2(right - 3.0, top.y + STRIP_H - 6.0),
        ],
        egui::Stroke::new(1.0, coursecraft_render::palette(ui.ctx()).border),
    );
    right
}

fn tab_strip(editor: &mut Editor, ui: &mut Ui, requests: &mut Vec<EditorRequest>) {
    let p = coursecraft_render::palette(ui.ctx());
    let font = FontId::proportional(13.0);
    let top = ui.max_rect().min;
    let mut x = quick_access(editor, ui, requests) + 4.0;
    let tab_rect = |ui: &Ui, x: &mut f32, label: &str| {
        let w = ui
            .painter()
            .layout_no_wrap(label.to_owned(), font.clone(), p.text)
            .size()
            .x
            + 20.0;
        let r = Rect::from_min_size(pos2(*x, top.y + 1.0), vec2(w, STRIP_H - 2.0));
        *x += w + 2.0;
        r
    };

    // File: opens the full-window File page (ribbon.md › File).
    let file = tl!("File");
    let r = tab_rect(ui, &mut x, file);
    let resp = ui.interact(r, ui.scope_id().with("ribbon_tab_file"), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Tab, true, file));
    let fill = if resp.hovered() {
        p.accent.gamma_multiply(0.85)
    } else {
        p.accent
    };
    ui.painter().rect_filled(
        r.shrink(2.0),
        egui::CornerRadius::same(coursecraft_render::RADIUS_SM as u8),
        fill,
    );
    ui.painter().text(
        r.center(),
        Align2::CENTER_CENTER,
        file,
        font.clone(),
        crate::file_page::ON_ACCENT,
    );
    if resp
        .on_hover_text(tl!("New, open, save, import, export and publish"))
        .clicked()
    {
        crate::file_page::open(editor);
    }

    for tab in main_tabs(editor) {
        let r = tab_rect(ui, &mut x, tl!(tab.label));
        strip_tab(editor, ui, r, tab, None);
    }

    // Contextual tabs, right of Help, each context under its coloured caption.
    let visible = contextual::visible_tabs(editor);
    let mut i = 0;
    while let Some(first) = visible.get(i) {
        let context = first.context;
        let tabs: Vec<&'static spec::ContextTab> = visible[i..]
            .iter()
            .take_while(|t| t.context == context)
            .copied()
            .collect();
        i += tabs.len();
        let color = contextual::caption_color(context);
        let caption = tl!(context.caption());
        let caption_font = FontId::proportional(10.0);
        let caption_w = ui
            .painter()
            .layout_no_wrap(caption.to_owned(), caption_font.clone(), color)
            .size()
            .x
            + 12.0;
        let block_min = x + 2.0;
        x = block_min + caption_w;
        let mut rects = Vec::new();
        for t in &tabs {
            rects.push(tab_rect(ui, &mut x, tl!(t.tab.label)));
        }
        let block =
            Rect::from_min_max(pos2(block_min, top.y + 1.0), pos2(x, top.y + STRIP_H - 1.0));
        ui.painter().rect_filled(
            block,
            egui::CornerRadius::same(coursecraft_render::RADIUS_SM as u8),
            color.gamma_multiply(0.16),
        );
        ui.painter().rect_filled(
            Rect::from_min_size(block.min, vec2(block.width(), 2.0)),
            egui::CornerRadius::same(1),
            color,
        );
        ui.painter().text(
            pos2(block.min.x + 6.0, block.center().y),
            Align2::LEFT_CENTER,
            caption,
            caption_font,
            color,
        );
        for (t, r) in tabs.iter().zip(rects) {
            strip_tab(editor, ui, r, &t.tab, Some(caption));
        }
        x += 2.0;
    }
}

/// One tab of the strip; `caption` names a contextual tab's tools.
fn strip_tab(editor: &mut Editor, ui: &mut Ui, r: Rect, tab: &'static Tab, caption: Option<&str>) {
    let p = coursecraft_render::palette(ui.ctx());
    let font = FontId::proportional(13.0);
    let label = tl!(tab.label);
    let resp = ui.interact(
        r,
        ui.scope_id().with(("ribbon_tab", tab.id)),
        Sense::click(),
    );
    let active = editor.ui.ribbon.tab == tab.id;
    let name = caption.map_or_else(|| label.to_owned(), |c| format!("{c}: {label}"));
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Tab, true, active, &name));
    if resp.hovered() && !active {
        ui.painter().rect_filled(
            r.shrink(2.0),
            egui::CornerRadius::same(coursecraft_render::RADIUS_SM as u8),
            p.hover,
        );
    }
    let color = if active { p.text } else { p.text_secondary };
    ui.painter()
        .text(r.center(), Align2::CENTER_CENTER, label, font, color);
    if active {
        ui.painter().rect_filled(
            Rect::from_center_size(
                pos2(r.center().x, r.max.y - 1.0),
                vec2((r.width() - 16.0).max(12.0), 2.5),
            ),
            egui::CornerRadius::same(1),
            p.accent,
        );
    }
    let resp = if caption.is_some() {
        resp.on_hover_text(&name)
    } else {
        resp
    };
    tab_clicked(editor, ui.input(|i| i.time), tab.id, &resp);
}

/// A click opens the tab and expands the ribbon, never collapses it. Only a
/// double-click on a tab that was already open and expanded collapses it, once.
fn tab_clicked(editor: &mut Editor, now: f64, id: &'static str, resp: &egui::Response) {
    let state = &mut editor.ui.ribbon;
    let just_opened = state
        .tab_opened
        .is_some_and(|(tab, at)| tab == id && now - at < DOUBLE_CLICK_SECS);
    if resp.double_clicked() && !just_opened {
        state.collapsed = !state.collapsed;
        state.tab = id;
        state.tab_opened = None;
    } else if resp.clicked() && !just_opened && (state.tab != id || state.collapsed) {
        state.tab = id;
        state.collapsed = false;
        state.tab_opened = Some((id, now));
    }
}

fn show_tab(
    editor: &mut Editor,
    ui: &mut Ui,
    tab: &'static Tab,
    available: f32,
    requests: &mut Vec<EditorRequest>,
) {
    let map = editor.ui.story_view;
    let groups = match tab.map_groups {
        Some(map_groups) if map => map_groups,
        _ => tab.groups,
    };
    if groups.is_empty() {
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(tl!(
                "This tab is being built. Its commands are in the menu bar for now."
            ))
            .color(coursecraft_render::palette(ui.ctx()).text_secondary),
        );
        return;
    }
    let blocked = (tab.when == TabWhen::SlideOpen && map).then_some("Open a slide first");
    let visible_groups: Vec<&'static Group> =
        groups.iter().filter(|g| g.has_visible_items()).collect();
    let shrunk = shrunk_groups(&editor.ui.ribbon, &visible_groups, available);
    for (group, shrink) in visible_groups.into_iter().zip(shrunk) {
        if shrink {
            shrunk_group(editor, ui, group, blocked, requests);
        } else {
            let width = show_group(editor, ui, group, blocked, requests);
            remember_width(&mut editor.ui.ribbon, group.id, width);
        }
    }
}

/// Width of a group drawn as one button.
const SHRUNK_W: f32 = 64.0;

/// Which groups to draw as one button so the rest fit in `available`,
/// rightmost first. A group not measured yet counts as a shrunk one.
fn shrunk_groups(state: &RibbonState, groups: &[&'static Group], available: f32) -> Vec<bool> {
    let width = |g: &&'static Group| {
        state
            .group_widths
            .iter()
            .find(|(id, _)| *id == g.id)
            .map_or(SHRUNK_W, |(_, w)| *w)
    };
    let mut shrunk = vec![false; groups.len()];
    let mut total: f32 = groups.iter().map(width).sum();
    for (i, group) in groups.iter().enumerate().rev() {
        if total <= available {
            break;
        }
        let w = width(group);
        if w > SHRUNK_W {
            shrunk[i] = true;
            total -= w - SHRUNK_W;
        }
    }
    shrunk
}

fn remember_width(state: &mut RibbonState, id: &'static str, width: f32) {
    if !width.is_finite() {
        return;
    }
    match state.group_widths.iter_mut().find(|(g, _)| *g == id) {
        Some(entry) => entry.1 = width,
        None => state.group_widths.push((id, width)),
    }
}

/// A group that doesn't fit: one large button with its caption, opening the
/// whole group in a popup.
fn shrunk_group(
    editor: &mut Editor,
    ui: &mut Ui,
    group: &'static Group,
    blocked: Option<&'static str>,
    requests: &mut Vec<EditorRequest>,
) {
    let icon = group
        .items
        .iter()
        .find(|i| !i.is_hidden())
        .map_or(Icon::DotsThree, |i| i.icon);
    let caption = tl!(group.label);
    let inner = ui.vertical(|ui| {
        ui.set_width(SHRUNK_W - 8.0);
        let (main, arrow) = widgets::large(ui, icon, caption, true, true, false);
        let main = main.on_hover_text(caption);
        if let Some(arrow) = arrow {
            widgets::split_popup(&main, &arrow, true).show(|ui| {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing = vec2(2.0, 0.0);
                    show_group(editor, ui, group, blocked, requests);
                });
            });
        }
    });
    separator(ui, inner.response.rect);
}

fn separator(ui: &mut Ui, r: Rect) {
    let p = coursecraft_render::palette(ui.ctx());
    let x = r.max.x + 4.0;
    ui.painter().line_segment(
        [pos2(x, r.min.y + 4.0), pos2(x, r.max.y - 2.0)],
        egui::Stroke::new(1.0, p.border),
    );
    ui.add_space(8.0);
}

/// How a run of items is laid out.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Run {
    Large,
    /// Small buttons in columns of three.
    Columns,
    /// Toggles and combo fields in rows.
    Rows,
}

fn run_of(kind: Kind) -> Run {
    match kind {
        Kind::Large | Kind::LargeSplit => Run::Large,
        Kind::Small | Kind::SmallSplit => Run::Columns,
        Kind::Toggle | Kind::ToggleSplit | Kind::Combo { .. } | Kind::Number { .. } => Run::Rows,
        Kind::Gallery { .. } => Run::Large,
    }
}

/// Draw a group whole; returns the width it took, separator included.
fn show_group(
    editor: &mut Editor,
    ui: &mut Ui,
    group: &'static Group,
    blocked: Option<&'static str>,
    requests: &mut Vec<EditorRequest>,
) -> f32 {
    let visible_items: Vec<&'static Item> = group.items.iter().filter(|i| !i.is_hidden()).collect();
    let launcher = group.launcher.filter(|a| !a.is_hidden());
    let p = coursecraft_render::palette(ui.ctx());
    let caption = tl!(group.label);
    let caption_font = FontId::proportional(11.0);
    let caption_w = ui
        .painter()
        .layout_no_wrap(caption.to_owned(), caption_font.clone(), p.text)
        .size()
        .x
        + if launcher.is_some() { 22.0 } else { 8.0 };
    let inner = ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = vec2(2.0, 0.0);
        let controls = ui.horizontal_top(|ui| {
            ui.set_min_height(CONTROLS_H);
            ui.set_min_width(caption_w);
            let mut rest = visible_items.as_slice();
            while let Some(first) = rest.first() {
                let kind = run_of(first.kind);
                let len = rest.iter().take_while(|i| run_of(i.kind) == kind).count();
                let (run, tail) = rest.split_at(len);
                rest = tail;
                match kind {
                    Run::Large => {
                        for item in run {
                            control(editor, ui, item, blocked, requests);
                        }
                    }
                    Run::Columns => {
                        for column in run.chunks(3) {
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                                for item in column {
                                    control(editor, ui, item, blocked, requests);
                                }
                            });
                        }
                    }
                    Run::Rows => {
                        let rows = if run.len() <= 2 {
                            1
                        } else if run.len() <= 6 {
                            2
                        } else {
                            3
                        };
                        let per_row = run.len().div_ceil(rows);
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing = vec2(1.0, 0.0);
                            for row in run.chunks(per_row.max(1)) {
                                ui.horizontal(|ui| {
                                    ui.set_height(SMALL_H);
                                    for item in row {
                                        control(editor, ui, item, blocked, requests);
                                    }
                                });
                            }
                        });
                    }
                }
            }
        });
        let width = controls.response.rect.width().max(caption_w);
        let (rect, _) = ui.allocate_exact_size(vec2(width, CAPTION_H), Sense::hover());
        let text_rect = if launcher.is_some() {
            Rect::from_min_max(rect.min, pos2(rect.max.x - 16.0, rect.max.y))
        } else {
            rect
        };
        ui.painter().text(
            text_rect.center(),
            Align2::CENTER_CENTER,
            caption,
            caption_font,
            p.text_secondary,
        );
        if let Some(action) = launcher {
            let r =
                Rect::from_center_size(pos2(rect.max.x - 8.0, rect.center().y), vec2(14.0, 14.0));
            let name = craft_i18n::fmt(tl!("{group} settings"), &[("group", caption)]);
            let state = availability(editor, action, blocked);
            let resp = widgets::launcher(ui, r, &name, state.is_ok());
            let resp = match &state {
                Ok(()) => resp.on_hover_text(&name),
                Err(reason) => resp.on_hover_text(reason),
            };
            if state.is_ok() && resp.clicked() {
                run(editor, action, &resp, requests);
            }
        }
    });
    // Separator between groups.
    let r = inner.response.rect;
    separator(ui, r);
    r.width() + 8.0 + ui.spacing().item_spacing.x
}

/// The keyboard shortcut of a control, from the command registry or the shell
/// shortcuts, if it has one.
fn shortcut_of(action: Action) -> Option<&'static str> {
    use crate::shortcuts::{ShortcutAction as S, shortcut_for_action, shortcut_for_command};
    let shell = match action {
        Action::Command(id) => return shortcut_for_command(id),
        Action::Text(spec::TextOp::Bold) => S::Bold,
        Action::Text(spec::TextOp::Italic) => S::Italic,
        Action::Text(spec::TextOp::Underline) => S::Underline,
        Action::Ui(ui) => match ui {
            UiAction::Paste => S::Paste,
            UiAction::Cut => S::Cut,
            UiAction::Copy => S::Copy,
            UiAction::SelectAll => S::SelectAll,
            UiAction::FormatPainter => S::FormatPainter,
            UiAction::Spelling => S::CheckSpelling,
            UiAction::Publish => S::Publish,
            UiAction::Preview => S::PreviewThisSlide,
            UiAction::CourseMap => S::CourseMap,
            UiAction::ZoomFit => S::ZoomFit,
            UiAction::ZoomIn => S::ZoomIn,
            UiAction::ZoomOut => S::ZoomOut,
            UiAction::Shortcuts => S::KeyboardShortcuts,
            _ => return None,
        },
        _ => return None,
    };
    Some(shortcut_for_action(shell))
}

fn tooltip(
    ctx: &egui::Context,
    label: &str,
    shortcut: Option<&str>,
    help: &'static str,
    state: &Result<(), String>,
) -> String {
    let label = &crate::menu::tooltip_with_shortcut(ctx, label, shortcut);
    match state {
        Err(reason) => format!("{label}\n{reason}"),
        Ok(()) if help.is_empty() => label.to_owned(),
        Ok(()) => format!("{label}\n{}", tl!(help)),
    }
}

fn control(
    editor: &mut Editor,
    ui: &mut Ui,
    item: &'static Item,
    blocked: Option<&'static str>,
    requests: &mut Vec<EditorRequest>,
) {
    if item.is_hidden() {
        return;
    }
    let state = availability(editor, item.action, blocked);
    let enabled = state.is_ok();
    let group_interactive = item.action == Action::Command("group.set_interactive")
        && crate::menu::selected_group_interactive_info(editor)
            .is_some_and(|(_, _, interactive)| interactive);
    let label = if group_interactive {
        tl!("Remove button behaviour")
    } else {
        tl!(item.label)
    };
    let selected = match item.action {
        Action::Command("group.set_interactive") => group_interactive,
        Action::Ui(a) => ui_selected(editor, a),
        Action::Text(op) => text_selected(editor, op),
        _ => false,
    };
    let (main, arrow) = match item.kind {
        Kind::Large | Kind::LargeSplit => widgets::large(
            ui,
            item.icon,
            label,
            enabled,
            item.kind.is_split(),
            selected,
        ),
        Kind::Small | Kind::SmallSplit => widgets::small(
            ui,
            item.icon,
            label,
            enabled,
            selected,
            item.kind.is_split(),
        ),
        Kind::Toggle => (
            widgets::toggle(ui, item.icon, label, enabled, selected),
            None,
        ),
        Kind::ToggleSplit => {
            let bar = match item.action {
                Action::Text(spec::TextOp::Color) => Some(font_color_bar(editor)),
                Action::Text(spec::TextOp::Highlight) => Some(highlight_bar(editor)),
                _ => None,
            };
            let (main, arrow) =
                widgets::toggle_split_bar(ui, item.icon, label, enabled, selected, bar);
            (main, Some(arrow))
        }
        Kind::Combo { width } => (
            widgets::combo_field(ui, label, &combo_text(editor, item.action), width, enabled),
            None,
        ),
        Kind::Number { width } => match item.action {
            Action::Ui(UiAction::Effect(spec::EffectOp::Duration(motion))) => (
                gallery::duration_field(editor, ui, item, motion, width, enabled),
                None,
            ),
            Action::Ui(UiAction::Context(spec::ContextOp::Frame(field))) => (
                contextual::number_field(editor, ui, item, field, width, enabled),
                None,
            ),
            _ => (
                contextual::number_field(editor, ui, item, spec::FrameField::X, width, enabled),
                None,
            ),
        },
        Kind::Gallery { tiles } => {
            let Action::Ui(UiAction::Gallery(g)) = item.action else {
                return;
            };
            let resp = gallery::show(editor, ui, item, g, tiles, enabled);
            if let Err(reason) = &state {
                resp.on_hover_text(reason);
            }
            return;
        }
    };
    let tip = tooltip(ui.ctx(), label, shortcut_of(item.action), item.help, &state);
    let main = main.on_hover_text(&tip);
    // These open their ▾ menu from the whole control.
    let menu_only = matches!(
        item.action,
        Action::Ui(
            UiAction::Arrange
                | UiAction::Shape
                | UiAction::Caption
                | UiAction::Question
                | UiAction::Effect(spec::EffectOp::Options(_))
        )
    );
    if enabled && !menu_only && (main.clicked() || main.double_clicked()) {
        // A double click reaches the Format Painter (it keeps it on); other
        // actions run once, on the first click.
        if main.clicked()
            || item.action == Action::Ui(UiAction::FormatPainter)
            || item.action == Action::Ui(UiAction::AnimationPainter)
        {
            run(editor, item.action, &main, requests);
        }
    }
    if item.action == Action::Text(spec::TextOp::Color)
        && let Some(color) = crate::eyedropper::take_pending(
            ui.ctx(),
            &crate::eyedropper::field_key(&TEXT_COLOR_FIELD),
        )
    {
        let hex = crate::properties::color_to_hex(color);
        crate::properties::remember_color(ui.ctx(), &hex);
        apply_text_change(editor, &crate::text_format::Change::Color(hex));
    }
    if let (Some(arrow), Action::Text(op)) = (&arrow, item.action) {
        let arrow = arrow.clone().on_hover_text(&tip);
        widgets::split_popup(&main, &arrow, op.main_opens() && enabled)
            .show(|ui| text_menu(editor, ui, op));
    } else if let Some(arrow) = arrow {
        let arrow = arrow.on_hover_text(&tip);
        widgets::split_popup(&main, &arrow, menu_only && enabled).show(|ui| {
            menu(editor, ui, item, blocked, requests);
        });
    } else if let Action::Text(op) = item.action
        && op.has_menu()
    {
        // Font, size, colour, spacing: the control itself opens its list.
        widgets::split_popup(&main, &main, enabled).show(|ui| text_menu(editor, ui, op));
    }
}

/// Whether a text toggle is on for the text the controls act on.
fn text_selected(editor: &Editor, op: spec::TextOp) -> bool {
    use coursecraft_course::{Align, ListKind};
    use spec::TextOp;
    let Ok(st) = &editor.ui.ribbon.text else {
        return false;
    };
    match op {
        TextOp::Bold => st.bold == Some(true),
        TextOp::Italic => st.italic == Some(true),
        TextOp::Underline => st.underline == Some(true),
        TextOp::Strikethrough => st.strikethrough == Some(true),
        TextOp::Shadow => st.shadow == Some(true),
        TextOp::Bullets => st.list == Some(ListKind::Bullet),
        TextOp::Numbering => st.list == Some(ListKind::Number),
        TextOp::AlignLeft => st.align == Some(Align::Left),
        TextOp::AlignCenter => st.align == Some(Align::Center),
        TextOp::AlignRight => st.align == Some(Align::Right),
        TextOp::Justify => st.align == Some(Align::Justify),
        TextOp::WrapText => st.wrap == Some(true),
        _ => false,
    }
}

/// The value a combo shows: the font or size at the text, blank when mixed.
pub(crate) fn combo_text(editor: &Editor, action: Action) -> String {
    let Ok(st) = &editor.ui.ribbon.text else {
        return String::new();
    };
    match action {
        Action::Text(spec::TextOp::Font) => st
            .font
            .as_deref()
            .map(crate::font_picker::displayed_font)
            .unwrap_or_default(),
        Action::Text(spec::TextOp::Size) => st.size.map(fmt_size).unwrap_or_default(),
        _ => String::new(),
    }
}

fn fmt_size(size: f32) -> String {
    if (size - size.round()).abs() < 0.05 {
        format!("{}", size.round() as i32)
    } else {
        format!("{size:.1}")
    }
}

/// The change a text control makes when clicked (controls with a menu make
/// theirs from the menu).
fn text_change(editor: &Editor, op: spec::TextOp) -> Option<crate::text_format::Change> {
    use crate::text_format::Change;
    use coursecraft_course::{Align, ListKind};
    use spec::TextOp;
    Some(match op {
        TextOp::Color => Change::Color(last_font_color(editor)),
        TextOp::Highlight => Change::Highlight(Some(last_highlight(editor))),
        TextOp::Grow => Change::Grow,
        TextOp::Shrink => Change::Shrink,
        TextOp::Bold => Change::Bold,
        TextOp::Italic => Change::Italic,
        TextOp::Underline => Change::Underline,
        TextOp::Strikethrough => Change::Strikethrough,
        TextOp::ClearFormatting => Change::ClearFormatting,
        TextOp::Shadow => Change::Shadow,
        TextOp::Bullets => Change::List(ListKind::Bullet),
        TextOp::Numbering => Change::List(ListKind::Number),
        TextOp::IndentLess => Change::Indent(-1),
        TextOp::IndentMore => Change::Indent(1),
        TextOp::AlignLeft => Change::Align(Align::Left),
        TextOp::AlignCenter => Change::Align(Align::Center),
        TextOp::AlignRight => Change::Align(Align::Right),
        TextOp::Justify => Change::Align(Align::Justify),
        TextOp::WrapText => Change::Wrap,
        TextOp::TextStyles
        | TextOp::Font
        | TextOp::Size
        | TextOp::Spacing
        | TextOp::LineSpacing
        | TextOp::AlignText
        | TextOp::Case
        | TextOp::Direction => return None,
    })
}

/// The colour Font Color applies from its main part: the last one picked from the ribbon, else
/// the theme accent.
fn last_font_color(editor: &Editor) -> String {
    editor
        .ui
        .ribbon
        .last_font_color
        .clone()
        .unwrap_or_else(|| editor.session.course.theme.accent.0.clone())
}

/// The colour Highlight applies from its main part: the last one picked, else yellow.
fn last_highlight(editor: &Editor) -> String {
    editor
        .ui
        .ribbon
        .last_highlight
        .clone()
        .unwrap_or_else(|| HIGHLIGHT_COLORS[0].1.to_owned())
}

/// The bar under the Font Color icon.
fn font_color_bar(editor: &Editor) -> egui::Color32 {
    coursecraft_render::parse_color(&coursecraft_course::Color(last_font_color(editor)))
        .unwrap_or(egui::Color32::GRAY)
}

/// The bar under the Highlight icon.
fn highlight_bar(editor: &Editor) -> egui::Color32 {
    coursecraft_render::parse_color(&coursecraft_course::Color(last_highlight(editor)))
        .unwrap_or(egui::Color32::YELLOW)
}

/// Apply a text change from the ribbon, remembering a picked Font Color or Highlight colour so the
/// main part of that control applies it next time.
fn apply_text_change(editor: &mut Editor, change: &crate::text_format::Change) -> bool {
    use crate::text_format::Change;
    match change {
        Change::Color(hex) => editor.ui.ribbon.last_font_color = Some(hex.clone()),
        Change::Highlight(Some(hex)) => editor.ui.ribbon.last_highlight = Some(hex.clone()),
        _ => {}
    }
    crate::text_format::apply(editor, change)
}

/// The eyedropper's name for the ribbon's Font Color.
const TEXT_COLOR_FIELD: &str = "ribbon_text_color";

/// The ▾ list of a text control; a pick applies at once.
fn text_menu(editor: &mut Editor, ui: &mut Ui, op: spec::TextOp) {
    use crate::text_format::{Change, LINE_SPACINGS, SIZES, SPACINGS};
    use coursecraft_course::VAlign;
    use spec::TextOp;
    let st = editor.ui.ribbon.text.clone().unwrap_or_default();
    let mut picked: Option<Change> = None;
    let mut item = |ui: &mut Ui, label: &str, on: bool, change: Change| {
        if ui.add(egui::Button::selectable(on, label)).clicked() {
            picked = Some(change);
        }
    };
    match op {
        TextOp::TextStyles => {
            text_styles_menu(editor, ui);
            return;
        }
        TextOp::Font => {
            // The inspector's font picker list: search, the course's fonts,
            // recent and installed fonts, Add font….
            use crate::font_picker::FontPickerAction;
            let shown = st
                .font
                .as_deref()
                .map(crate::font_picker::displayed_font)
                .unwrap_or_default();
            let mut requests = Vec::new();
            let state = &mut editor.ui;
            let action = crate::font_picker::show_font_list(
                ui,
                "ribbon_font",
                &shown,
                &state.properties_lists.font_families,
                &mut state.properties.recent_fonts,
                state.system_fonts.as_deref(),
                &mut state.properties.system_fonts_list_requested,
                &mut state.properties.system_font_search,
                &mut requests,
            );
            match action {
                Some(FontPickerAction::PickCourseOrRecent(name)) => {
                    picked = Some(Change::Font(name));
                }
                Some(FontPickerAction::PickSystem(name)) => {
                    requests.push(EditorRequest::AddSystemFont {
                        family: name.clone(),
                        apply: None,
                    });
                    picked = Some(Change::Font(name));
                }
                Some(FontPickerAction::AddFont) => requests.push(EditorRequest::PickFonts),
                None => {}
            }
            editor.pending_requests.append(&mut requests);
        }
        TextOp::Size => {
            egui::ScrollArea::vertical()
                .max_height(320.0)
                .show(ui, |ui| {
                    for size in SIZES {
                        let on = st.size.is_some_and(|s| (s - size).abs() < 0.05);
                        item(ui, &fmt_size(size), on, Change::Size(size));
                    }
                });
        }
        TextOp::Spacing => {
            for (label, points) in SPACINGS {
                let on = st.letter_spacing.is_some_and(|v| (v - points).abs() < 0.01);
                item(ui, tl!(label), on, Change::Spacing(points));
            }
        }
        TextOp::LineSpacing => {
            for lines in LINE_SPACINGS {
                let on = st.line_spacing.is_some_and(|l| (l - lines).abs() < 0.01);
                item(
                    ui,
                    format!("{lines:.2}")
                        .trim_end_matches('0')
                        .trim_end_matches('.'),
                    on,
                    Change::LineSpacing(lines),
                );
            }
        }
        TextOp::AlignText => {
            for (label, valign) in [
                ("Top", VAlign::Top),
                ("Middle", VAlign::Middle),
                ("Bottom", VAlign::Bottom),
            ] {
                item(
                    ui,
                    tl!(label),
                    st.valign == Some(valign),
                    Change::VAlign(valign),
                );
            }
        }
        TextOp::Case => {
            use coursecraft_course::CaseChange;
            for (label, case) in [
                ("Sentence case", CaseChange::Sentence),
                ("lowercase", CaseChange::Lower),
                ("UPPERCASE", CaseChange::Upper),
                ("Capitalize Each Word", CaseChange::Title),
                ("tOGGLE cASE", CaseChange::Toggle),
            ] {
                item(ui, tl!(label), false, Change::Case(case));
            }
        }
        TextOp::Highlight => {
            highlight_menu(editor, ui, &mut picked);
        }
        TextOp::Direction => {
            use coursecraft_course::TextDirection;
            for (label, direction) in [
                ("Horizontal", TextDirection::Horizontal),
                ("Rotate all text 90°", TextDirection::Vertical),
                ("Rotate all text 270°", TextDirection::Vertical270),
            ] {
                item(
                    ui,
                    tl!(label),
                    st.direction == Some(direction),
                    Change::Direction(direction),
                );
            }
        }
        TextOp::Numbering => {
            numbering_menu(ui, &st, &mut picked);
        }
        TextOp::Color => {
            // The inspector's colour picker: eyedropper, palette, theme and
            // recent swatches, the colour wheel.
            let pending_id = egui::Id::unique("ribbon_text_color_pending");
            let (mut hex, mut pending) = ui
                .ctx()
                .data(|d| d.get_temp::<(String, bool)>(pending_id))
                .filter(|(_, pending)| *pending)
                .unwrap_or_else(|| (st.color.clone().unwrap_or_default(), false));
            let parsed =
                coursecraft_render::parse_color(&coursecraft_course::Color(hex.trim().to_owned()));
            let swatch = crate::properties::color_picker_body(
                ui,
                &mut hex,
                parsed,
                pending_id,
                &crate::eyedropper::field_key(&TEXT_COLOR_FIELD),
                &mut pending,
            );
            if swatch {
                picked = Some(Change::Color(hex.clone()));
                pending = false;
            } else if pending && !ui.input(|i| i.pointer.any_down()) {
                // A drag across the wheel is one change, applied on release;
                // the popup stays open for another try.
                crate::properties::remember_color(ui.ctx(), &hex);
                apply_text_change(editor, &Change::Color(hex.clone()));
                pending = false;
            }
            ui.ctx()
                .data_mut(|d| d.insert_temp(pending_id, (hex.clone(), pending)));
            if swatch {
                crate::properties::remember_color(ui.ctx(), &hex);
            }
        }
        _ => {}
    }
    if let Some(change) = picked {
        apply_text_change(editor, &change);
        ui.close();
    }
}

/// The standard highlight colours: name and hex.
pub(crate) const HIGHLIGHT_COLORS: [(&str, &str); 8] = [
    ("Yellow", "#FFFF00"),
    ("Bright Green", "#00FF00"),
    ("Turquoise", "#00FFFF"),
    ("Pink", "#FF00FF"),
    ("Blue", "#0070FF"),
    ("Red", "#FF0000"),
    ("Orange", "#FFA500"),
    ("Light Gray", "#D9D9D9"),
];

/// Highlight ▾: the theme colours, a few standard highlight colours and No Color.
fn highlight_menu(editor: &Editor, ui: &mut Ui, picked: &mut Option<crate::text_format::Change>) {
    let course = &editor.session.course;
    let mut theme: Vec<(String, String)> = vec![
        (tl!("Accent").to_owned(), course.theme.accent.0.clone()),
        (
            tl!("Background").to_owned(),
            course.theme.background.0.clone(),
        ),
        (tl!("Text").to_owned(), course.theme.text.0.clone()),
    ];
    theme.extend(
        course
            .theme
            .palette
            .iter()
            .map(|c| (c.name.clone(), c.color.0.clone())),
    );
    if let Some(hex) = crate::properties::swatch_row(ui, tl!("Theme colours"), &theme) {
        *picked = Some(crate::text_format::Change::Highlight(Some(hex)));
    }
    let standard: Vec<(String, String)> = HIGHLIGHT_COLORS
        .iter()
        .map(|(name, hex)| (tl!(*name).to_owned(), (*hex).to_owned()))
        .collect();
    if let Some(hex) = crate::properties::swatch_row(ui, tl!("Highlight colours"), &standard) {
        *picked = Some(crate::text_format::Change::Highlight(Some(hex)));
    }
    if ui.button(tl!("No Color")).clicked() {
        *picked = Some(crate::text_format::Change::Highlight(None));
    }
}

/// The numbering schemes with a sample of each.
pub(crate) const NUMBER_SCHEMES: [(coursecraft_course::NumberScheme, &str); 7] = [
    (coursecraft_course::NumberScheme::ArabicPeriod, "1. 2. 3."),
    (coursecraft_course::NumberScheme::ArabicParenR, "1) 2) 3)"),
    (coursecraft_course::NumberScheme::AlphaLcPeriod, "a. b. c."),
    (coursecraft_course::NumberScheme::AlphaLcParenR, "a) b) c)"),
    (coursecraft_course::NumberScheme::AlphaUcPeriod, "A. B. C."),
    (
        coursecraft_course::NumberScheme::RomanLcPeriod,
        "i. ii. iii.",
    ),
    (
        coursecraft_course::NumberScheme::RomanUcPeriod,
        "I. II. III.",
    ),
];

/// The Start at change, only when the number was edited (`N100`).
pub(crate) fn start_at_change(edited: bool, value: u32) -> Option<crate::text_format::Change> {
    edited.then_some(crate::text_format::Change::StartAt(value))
}

/// Numbering ▾: the seven schemes with samples, and Start at….
fn numbering_menu(
    ui: &mut Ui,
    st: &crate::text_format::TextState,
    picked: &mut Option<crate::text_format::Change>,
) {
    for (scheme, sample) in NUMBER_SCHEMES {
        let on = st.number_scheme == Some(scheme);
        if ui.add(egui::Button::selectable(on, sample)).clicked() {
            *picked = Some(crate::text_format::Change::NumberScheme(scheme));
        }
    }
    ui.separator();
    let id = egui::Id::unique("ribbon_numbering_start_at");
    // The number only counts once it was edited: clicking in and away must not turn the
    // text into a numbered list (`N100`).
    let edited_before = ui.ctx().data(|d| d.get_temp::<u32>(id));
    let mut value: u32 = edited_before.or(st.start_at).unwrap_or(1);
    ui.horizontal(|ui| {
        ui.label(tl!("Start at"));
        let resp = ui.add(
            egui::DragValue::new(&mut value)
                .range(1..=99_999)
                .speed(0.2),
        );
        if resp.changed() {
            ui.ctx().data_mut(|d| d.insert_temp(id, value));
        }
        if resp.drag_stopped() || resp.lost_focus() {
            ui.ctx().data_mut(|d| d.remove::<u32>(id));
            if let Some(change) = start_at_change(edited_before.is_some() || resp.changed(), value)
            {
                *picked = Some(change);
            }
        }
    });
}

/// Menu entries that mark themselves as the default choice of their menu.
pub(crate) fn is_default_entry(id: &str) -> bool {
    id == "paste.destination"
}

fn menu(
    editor: &mut Editor,
    ui: &mut Ui,
    item: &'static Item,
    blocked: Option<&'static str>,
    requests: &mut Vec<EditorRequest>,
) {
    match item.menu {
        Menu::None => {}
        Menu::Items(entries) => {
            for entry in entries.iter().filter(|e| !e.is_hidden()) {
                let state = availability(editor, entry.action, blocked);
                let mut button = egui::Button::new(tl!(entry.label));
                if is_default_entry(entry.id) {
                    // The entry the main button (and Ctrl+V) uses.
                    button = button.shortcut_text(tl!("Default"));
                }
                let resp = ui.add_enabled(state.is_ok(), button);
                let resp = match &state {
                    Ok(()) => resp,
                    Err(reason) => resp.on_disabled_hover_text(reason),
                };
                if resp.clicked() {
                    run(editor, entry.action, &resp, requests);
                    ui.close();
                }
            }
        }
        Menu::Ui(UiMenu::NewSlide) => crate::slides::show_new_slide_popup(ui, editor),
        Menu::Ui(UiMenu::Preview) => crate::menu::show_preview_menu_items(editor, ui, requests),
        Menu::Ui(UiMenu::Shapes) => shapes_menu_category(editor, ui, None),
        Menu::Ui(UiMenu::Callouts) => shapes_menu_category(
            editor,
            ui,
            Some(coursecraft_course::ShapeCategory::Callouts),
        ),
        Menu::Ui(UiMenu::Questions) => {
            for kind in crate::question::ALL_QUESTION_KINDS {
                if ui
                    .button(tl!(crate::question::question_kind_name(kind)))
                    .clicked()
                {
                    crate::toolbar::insert_question(editor, kind);
                    ui.close();
                }
            }
        }
        Menu::Ui(UiMenu::EffectOptions(motion)) => {
            gallery::effect_options_menu(editor, ui, motion);
        }
        Menu::Ui(UiMenu::ThemeFonts) => crate::theme_dialog::theme_fonts_menu(editor, ui),
        Menu::Ui(UiMenu::Arrange) => {
            let mut action = None;
            crate::menu::arrange_menu_items(editor, ui, &mut action, requests);
            if let Some((id, params)) = action {
                editor.run(id, params);
            }
        }
    }
}

/// Shape ▾ / Caption ▾: the shape gallery, optionally filtered to one category.
fn shapes_menu_category(
    editor: &mut Editor,
    ui: &mut Ui,
    category: Option<coursecraft_course::ShapeCategory>,
) {
    let mut picked = None;
    let output = match category {
        Some(cat) => crate::shape_gallery::show_shape_gallery_category(
            ui,
            &mut editor.ui.shape_gallery_query,
            cat,
            &mut |pick| picked = Some(pick),
        ),
        None => crate::shape_gallery::show_shape_gallery(
            ui,
            &mut editor.ui.shape_gallery_query,
            &mut |pick| picked = Some(pick),
        ),
    };
    editor.ui.shape_gallery_rects = output.icons;
    editor.ui.shape_gallery_draw_rects = output.draw_icons;
    editor.ui.shape_gallery_search_rect = output.search_rect;
    match picked {
        Some(crate::shape_gallery::GalleryPick::Shape(kind)) => {
            crate::toolbar::insert_shape(editor, kind);
            ui.close();
        }
        Some(crate::shape_gallery::GalleryPick::Draw(tool)) => {
            crate::toolbar::insert_draw(editor, tool);
            ui.close();
        }
        None => {}
    }
}

/// The first "Style N" that no text style uses yet.
pub(crate) fn free_style_name(styles: &[coursecraft_course::NamedTextStyle]) -> String {
    (1..)
        .map(|n| format!("Style {n}"))
        .find(|name| !styles.iter().any(|s| s.name == *name))
        .unwrap_or_else(|| "Style".to_owned())
}

/// Text Styles ▾: apply a course style to the selected text boxes, make one
/// from the first, or manage them.
fn text_styles_menu(editor: &mut Editor, ui: &mut Ui) {
    let Ok(targets) = crate::text_format::targets(editor) else {
        return;
    };
    let course = &editor.session.course;
    let slide = targets.first().map(|t| t.slide.clone()).unwrap_or_default();
    // Named styles link to text boxes only, not to shapes or table cells.
    let boxes: Vec<String> = targets
        .iter()
        .filter(|t| t.cell.is_none())
        .filter(|t| {
            course
                .slide(&t.slide)
                .and_then(|s| s.object(&t.object))
                .is_some_and(|(o, _)| matches!(o.kind, coursecraft_course::ObjectKind::Text { .. }))
        })
        .map(|t| t.object.clone())
        .collect();
    let mut action: Option<(&'static str, serde_json::Value)> = None;
    if course.text_styles.is_empty() {
        ui.label(tl!("No styles yet"));
    }
    for named in &course.text_styles {
        let resp = ui.add_enabled(!boxes.is_empty(), egui::Button::new(&named.name));
        let resp = resp.on_disabled_hover_text(tl!("Text styles apply to text boxes"));
        if resp.clicked() {
            action = Some((
                "text_style.apply",
                json!({ "id": named.id, "slide": slide, "objects": boxes }),
            ));
        }
    }
    ui.separator();
    let full = course.text_styles.len() >= coursecraft_course::MAX_TEXT_STYLES;
    // Only the reason that applies.
    let why_not = if boxes.is_empty() {
        tl!("Select a text box")
    } else {
        tl!("A course holds at most 50 styles")
    };
    let resp = ui
        .add_enabled(
            !full && !boxes.is_empty(),
            egui::Button::new(tl!("New Style from Selection")),
        )
        .on_disabled_hover_text(why_not);
    if resp.clicked()
        && let Some(object) = boxes.first()
    {
        let name = free_style_name(&course.text_styles);
        action = Some((
            "text_style.create",
            json!({ "name": name, "from_object": { "slide": slide, "object": object } }),
        ));
    }
    if ui.button(tl!("Manage Styles…")).clicked() {
        crate::text_styles_ui::set_manage_open(ui.ctx(), true);
        ui.close();
    }
    if let Some((id, params)) = action {
        editor.run(id, params);
        ui.close();
    }
}
