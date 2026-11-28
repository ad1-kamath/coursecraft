//! The docked panel workspace (M37 step 1): default arrangement, drawing, and
//! layout changes through the workspace.

use craft_ui::docking::{Action, Placement};

use crate::dock::{self, Panel};
use crate::test_harness::TestHarness;
use coursecraft_course::Course;

const HELLO_COURSE: &str = include_str!("../../../examples/hello-course/course.json");

/// The panels of the default layout: all but the settings panels, which open
/// on demand.
fn laid_out() -> impl Iterator<Item = Panel> {
    Panel::ALL
        .into_iter()
        .filter(|p| crate::panels::family(*p) != crate::panels::Family::Settings)
}

fn docked() -> TestHarness {
    let course = Course::from_json(HELLO_COURSE).expect("hello course parses");
    let mut h = TestHarness::new_with_size(course, (1600.0, 900.0));
    h.editor.ui.dock.enabled = true;
    for _ in 0..3 {
        h.idle_frame();
    }
    h
}

#[test]
fn default_layout_is_valid_and_has_every_panel_once() {
    let state = dock::DockState::default();
    assert!(state.enabled, "docking is the default");
    assert_eq!(state.workspace.validate(), Ok(()));
    let panels: Vec<Panel> = state
        .workspace
        .layout
        .panels()
        .into_iter()
        .copied()
        .collect();
    for panel in laid_out() {
        assert_eq!(
            panels.iter().filter(|p| **p == panel).count(),
            1,
            "{panel:?} must be in the layout once"
        );
    }
}

#[test]
fn every_panel_is_drawn_where_today_s_panels_are() {
    let h = docked();
    let rect = |p| {
        h.editor
            .ui
            .dock
            .rect(p)
            .unwrap_or_else(|| panic!("{p:?} not drawn"))
    };
    let (slides, canvas, timeline, inspector) = (
        rect(Panel::Slides),
        rect(Panel::Canvas),
        rect(Panel::Timeline),
        h.editor
            .ui
            .dock
            .inspector_rect()
            .expect("an inspector is drawn"),
    );
    assert!(
        slides.max.x <= canvas.min.x + 1.0,
        "slides left of the canvas"
    );
    assert!(
        canvas.max.x <= inspector.min.x + 1.0,
        "Inspector right of the canvas"
    );
    assert!(
        canvas.max.y <= timeline.min.y + 1.0,
        "timeline under the canvas"
    );
    assert!(
        canvas.width() > 400.0 && canvas.height() > 200.0,
        "canvas has room: {canvas:?}"
    );
    assert_eq!(h.editor.ui.timeline.panel_rect, Some(timeline));
}

#[test]
fn panel_contents_still_draw_inside_the_dock() {
    let mut h = docked();
    // The inspectors' dock tabs and the timeline's header.
    assert!(h.find_text("Format").is_some(), "Format panel");
    assert!(h.find_text("Timeline").is_some(), "timeline tab");
}

#[test]
fn panels_move_through_the_workspace() {
    let mut h = docked();
    dock::apply(
        &mut h.editor,
        vec![Action::Move {
            panel: Panel::Format,
            anchor: Panel::Slides,
            placement: Placement::Tab { before: None },
        }],
    );
    h.idle_frame();
    let location: Vec<Panel> = h
        .editor
        .ui
        .dock
        .workspace
        .layout
        .panels()
        .into_iter()
        .copied()
        .collect();
    assert!(location.contains(&Panel::Format));
    // Tabbed with Slides, only the active one of the two is drawn.
    let drawn: Vec<Panel> = h.editor.ui.dock.rects.iter().map(|(p, _)| *p).collect();
    assert!(drawn.contains(&Panel::Canvas));
    assert!(!(drawn.contains(&Panel::Slides) && drawn.contains(&Panel::Format)));
}

#[test]
fn the_canvas_stays_docked_and_open() {
    let mut h = docked();
    let before = h.editor.ui.dock.workspace.clone();
    dock::apply(
        &mut h.editor,
        vec![Action::Close {
            panel: Panel::Canvas,
        }],
    );
    dock::apply(
        &mut h.editor,
        vec![Action::Move {
            panel: Panel::Canvas,
            anchor: Panel::Slides,
            placement: Placement::Tab { before: None },
        }],
    );
    assert_eq!(h.editor.ui.dock.workspace, before);
}

#[test]
fn panels_close_and_reopen_where_they_were() {
    let mut h = docked();
    dock::apply(
        &mut h.editor,
        vec![Action::Move {
            panel: Panel::Timeline,
            anchor: Panel::Format,
            placement: Placement::Tab { before: None },
        }],
    );
    let moved = h.editor.ui.dock.workspace.layout.clone();
    dock::set_open(&mut h.editor, Panel::Timeline, false);
    assert!(!dock::is_open(&h.editor, Panel::Timeline));
    h.idle_frame();
    assert_eq!(
        h.editor.ui.timeline.panel_rect, None,
        "a closed timeline has no rect"
    );
    dock::set_open(&mut h.editor, Panel::Timeline, true);
    assert!(dock::is_open(&h.editor, Panel::Timeline));
    assert_eq!(
        h.editor.ui.dock.workspace.layout.panels().len(),
        moved.panels().len()
    );
    let tabbed_with_inspector = h
        .editor
        .ui
        .dock
        .workspace
        .layout
        .location(&Panel::Timeline)
        .ok()
        .and_then(|l| l.anchor)
        == Some(Panel::Format);
    assert!(tabbed_with_inspector, "reopened beside the Inspector");
}

#[test]
fn the_canvas_cannot_be_hidden() {
    let mut h = docked();
    dock::set_open(&mut h.editor, Panel::Canvas, false);
    assert!(dock::is_open(&h.editor, Panel::Canvas));
}

#[test]
fn reset_reopens_every_panel() {
    let mut h = docked();
    for panel in [Panel::Slides, Panel::Timeline, Panel::Format] {
        dock::set_open(&mut h.editor, panel, false);
    }
    dock::reset(&mut h.editor);
    for panel in laid_out() {
        assert!(dock::is_open(&h.editor, panel), "{panel:?}");
    }
}

#[test]
fn layout_round_trips_through_the_preferences() {
    let mut h = docked();
    dock::set_open(&mut h.editor, Panel::Slides, false);
    let prefs = h.editor.layout_prefs();
    assert!(prefs.ribbon == h.editor.ui.ribbon.visible && prefs.docked);
    let mut other = TestHarness::new(Course::from_json(HELLO_COURSE).expect("parses"));
    other.editor.set_layout_prefs(&prefs);
    assert_eq!(other.editor.ui.dock.workspace, h.editor.ui.dock.workspace);
    assert!(other.editor.ui.dock.enabled);
}

#[test]
fn a_bad_saved_layout_falls_back_to_the_default() {
    for saved in [
        serde_json::json!("not a layout"),
        serde_json::json!({ "version": 99, "layout": {} }),
        serde_json::json!({ "layout": { "root": { "Tabs": { "panels": ["slides", "slides"], "active": 0 } } } }),
    ] {
        let workspace = dock::load_workspace(Some(&saved));
        assert_eq!(workspace.validate(), Ok(()));
        for panel in laid_out() {
            assert!(workspace.layout.contains(&panel), "{panel:?} after {saved}");
        }
    }
}

#[test]
fn a_saved_layout_without_the_canvas_gets_it_back() {
    let mut workspace = craft_ui::docking::Workspace::new(dock::default_layout());
    workspace.hidden.clear();
    let mut value = serde_json::to_value(&workspace).expect("serializes");
    // An edited file: every panel but the canvas.
    let text = value.to_string().replace("\"canvas\"", "\"slides\"");
    value = serde_json::from_str(&text).expect("still JSON");
    let loaded = dock::load_workspace(Some(&value));
    assert!(loaded.layout.contains(&Panel::Canvas));
}

#[test]
fn a_layout_change_is_sent_once_the_gesture_ends() {
    let mut h = docked();
    let _ = h.editor.layout_prefs();
    dock::set_open(&mut h.editor, Panel::Format, false);
    let first = h.idle_frame();
    let saves = |r: &[crate::EditorRequest]| {
        r.iter()
            .filter(|r| matches!(r, crate::EditorRequest::SaveLayout(_)))
            .count()
    };
    assert_eq!(saves(&first), 1);
    let second = h.idle_frame();
    assert_eq!(saves(&second), 0, "sent once");
}

#[test]
fn master_view_shows_the_timeline_panel_while_keeping_layers_states_triggers_unavailable() {
    let mut h = docked();
    h.editor.toggle_master_view();
    h.idle_frame();
    assert!(h.find_text("Slide masters have no timeline.").is_none());
    assert!(h.editor.ui.timeline.panel_rect.is_some());

    dock::reveal(&mut h.editor, Panel::States);
    h.idle_frame();
    assert!(h.find_text("Slide masters have no states.").is_some());

    dock::request_bottom_tab(&mut h.editor, crate::BottomTab::Triggers);
    h.idle_frame();
    assert!(h.find_text("Slide masters have no triggers.").is_some());

    dock::set_open(&mut h.editor, Panel::Layers, true);
    h.idle_frame();
    assert!(h.find_text("Slide masters have no layers.").is_some());
}

#[test]
fn the_inspectors_start_as_tabs_of_one_group() {
    let h = docked();
    let location = h
        .editor
        .ui
        .dock
        .workspace
        .layout
        .location(&Panel::Animate)
        .expect("animate is docked");
    assert_eq!(location.anchor, Some(Panel::Format), "tabbed with Format");
    let drawn: Vec<Panel> = h.editor.ui.dock.rects.iter().map(|(p, _)| *p).collect();
    let inspectors: Vec<&Panel> = drawn.iter().filter(|p| p.inspector().is_some()).collect();
    let current = Panel::for_inspector(h.editor.ui.inspector_tab);
    assert_eq!(inspectors, [&current], "only the active tab draws");
}

#[test]
fn choosing_an_inspector_brings_its_panel_forward() {
    let mut h = docked();
    dock::set_open(&mut h.editor, Panel::Animate, false);
    h.editor.ui.inspector_tab = crate::InspectorTab::Animate;
    h.idle_frame();
    assert!(dock::is_open(&h.editor, Panel::Animate), "reopened");
    assert!(h.editor.ui.dock.rect(Panel::Animate).is_some(), "active");
    assert!(h.editor.ui.dock.rect(Panel::SlideInspector).is_none());
}

#[test]
fn inspectors_split_apart_draw_side_by_side() {
    let mut h = docked();
    dock::apply(
        &mut h.editor,
        vec![Action::Move {
            panel: Panel::CourseInspector,
            anchor: Panel::Slides,
            placement: Placement::Tab { before: None },
        }],
    );
    dock::apply(
        &mut h.editor,
        vec![Action::Activate {
            panel: Panel::CourseInspector,
        }],
    );
    h.idle_frame();
    h.idle_frame();
    assert!(h.editor.ui.dock.rect(Panel::CourseInspector).is_some());
    let inspectors = h
        .editor
        .ui
        .dock
        .rects
        .iter()
        .filter(|(p, _)| p.inspector().is_some())
        .count();
    assert_eq!(inspectors, 2, "Course and the active one of the others");
}
