//! Bug-hunt proof tests for `coursecraft-ui-egui` (0.6.1 Neon review).
//!
//! Each test failed before its fix and now guards against a regression.

use coursecraft_course::{Action, Course, Event, ObjectKind, Trigger};
use coursecraft_ui_egui::dock::Panel;
use coursecraft_ui_egui::triggers::TriggerFormState;
use coursecraft_ui_egui::{BottomTab, Editor, panels, toolbar};

/// `Editor::replace_course` creates a new `Session` starting at revision 0
/// without clearing revision-keyed caches (`PropertiesListCache`, `MediaRefsCache`).
/// Opening Course B (rev 0) after Course A (rev 0) leaves Course A's slide list
/// and media references in inspector dropdowns.
#[test]
fn replace_course_leaves_stale_properties_cache() {
    let course_a = Course::from_json(
        r##"{
          "schema_version": 1,
          "id": "course-a",
          "title": "Course A",
          "scenes": [{
            "id": "sc1",
            "title": "Scene A",
            "slides": [{
              "id": "slide_alpha",
              "title": "Alpha",
              "duration_ms": 5000,
              "objects": [{
                "id": "o1",
                "kind": { "type": "shape", "shape": "rectangle", "fill": "#FF0000" },
                "frame": { "x": 10, "y": 10, "w": 80, "h": 40 }
              }]
            }]
          }]
        }"##,
    )
    .expect("course A");
    let course_b = Course::from_json(
        r#"{
          "schema_version": 1,
          "id": "course-b",
          "title": "Course B",
          "scenes": [{
            "id": "sc1",
            "title": "Scene B",
            "slides": [{ "id": "slide_beta", "title": "Beta", "duration_ms": 5000 }]
          }]
        }"#,
    )
    .expect("course B");

    let mut editor = Editor::new(course_a);
    editor.selection.slide = "slide_alpha".to_owned();
    editor.selection.object = Some("o1".to_owned());
    // Open a course-scoped dialog and arm format painter on Course A.
    editor.ui.show_fonts_window = true;
    editor.arm_format_painter(false);
    assert!(editor.ui.format_painter.is_some());

    // Replace with Course B (starts at revision 0, same as Course A).
    editor.replace_course(course_b);

    assert!(
        editor.ui.format_painter.is_none(),
        "replace_course must clear format_painter armed on Course A, still has {:?}",
        editor.ui.format_painter
    );
    assert!(
        !editor.ui.show_fonts_window,
        "replace_course must close course-scoped Fonts window from Course A"
    );
}

/// `apply_format_painter` filters targets with `**id != painter.source` without
/// checking `self.selection.slide == painter.source_slide`. Painting across
/// slides onto an object sharing the same ID (common after `slide.duplicate`)
/// silently does nothing.
#[test]
fn format_painter_cross_slide_skips_same_object_id() {
    let course = Course::from_json(
        r##"{
          "schema_version": 1,
          "id": "fp-same-id",
          "title": "FP Same Id",
          "scenes": [{
            "id": "sc1",
            "title": "Scene 1",
            "slides": [
              {
                "id": "s1",
                "title": "Slide 1",
                "duration_ms": 5000,
                "objects": [{
                  "id": "box1",
                  "kind": { "type": "shape", "shape": "rectangle", "fill": "#FF0000" },
                  "frame": { "x": 20, "y": 20, "w": 100, "h": 60 }
                }]
              },
              {
                "id": "s2",
                "title": "Slide 2",
                "duration_ms": 5000,
                "objects": [{
                  "id": "box1",
                  "kind": { "type": "shape", "shape": "rectangle", "fill": "#0000FF" },
                  "frame": { "x": 20, "y": 20, "w": 100, "h": 60 }
                }]
              }
            ]
          }]
        }"##,
    )
    .expect("valid course");

    let mut editor = Editor::new(course);
    editor.selection.slide = "s1".to_owned();
    editor.selection.object = Some("box1".to_owned());
    editor.arm_format_painter(false);

    editor.selection.slide = "s2".to_owned();
    editor.apply_format_painter(&["box1".to_owned()]);

    let ObjectKind::Shape { fill, .. } = &editor.session.course.scenes[0].slides[1].objects[0].kind
    else {
        panic!("expected shape");
    };
    assert_eq!(
        fill.0, "#FF0000",
        "Format Painter from s1/box1 onto s2/box1 must copy fill #FF0000, got {}",
        fill.0
    );
}

/// `TriggerFormState` shares a single `timer_object` field for both Timer
/// actions (`StartTimer`/`PauseTimer`/`ResetTimer`) and `Event::TimerEnds`.
/// Opening an existing trigger "Start t2 when t1 ends" overwrites the action
/// timer with `t1`.
#[test]
fn trigger_form_corrupts_two_timer_trigger_roundtrip() {
    let course = Course::from_json(
        r#"{
          "schema_version": 1,
          "id": "two-timers",
          "title": "Two Timers",
          "scenes": [{
            "id": "sc1",
            "title": "Scene 1",
            "slides": [{
              "id": "s1",
              "title": "Slide 1",
              "duration_ms": 10000,
              "objects": [
                {
                  "id": "t1",
                  "kind": { "type": "timer", "duration_ms": 2000 },
                  "frame": { "x": 10, "y": 10, "w": 100, "h": 40 }
                },
                {
                  "id": "t2",
                  "kind": { "type": "timer", "duration_ms": 3000, "autostart": false },
                  "frame": { "x": 10, "y": 60, "w": 100, "h": 40 }
                }
              ]
            }]
          }]
        }"#,
    )
    .expect("valid course");

    let slide = &course.scenes[0].slides[0];
    let original = Trigger {
        when: Event::TimerEnds {
            timer: "t1".to_owned(),
        },
        action: Action::StartTimer {
            timer: "t2".to_owned(),
        },
        condition: None,
        disabled: false,
    };

    let form = TriggerFormState::from_existing(0, &original, slide, None, &course);
    let rebuilt = form.build_trigger(&course).expect("build_trigger");
    assert_eq!(
        rebuilt.action,
        Action::StartTimer {
            timer: "t2".to_owned()
        },
        "loading TriggerFormState for 'Start t2 when t1 ends' corrupted action timer to {:?}",
        rebuilt.action
    );
}

/// With docking off, `panels::open_at(editor, Panel::Question, None)` (called by
/// Question Tools Display/Scoring launchers) pushes `Panel::Question` into
/// `editor.ui.panels.floating` (which renders a blank window in `panels::body`)
/// instead of activating `BottomTab::Question`.
#[test]
fn docking_off_open_question_panel_opens_empty_floating_window() {
    let course = Course::from_json(
        r#"{
          "schema_version": 1,
          "id": "dock-q",
          "title": "Dock Question",
          "scenes": [{
            "id": "sc1",
            "title": "Scene 1",
            "slides": [{ "id": "s1", "title": "Slide 1", "duration_ms": 5000 }]
          }]
        }"#,
    )
    .expect("valid course");

    let mut editor = Editor::new(course);
    editor.ui.dock.enabled = false;
    editor.ui.bottom_tab = BottomTab::Timeline;

    panels::open_at(&mut editor, Panel::Question, None);

    assert!(
        !editor.ui.panels.floating.contains(&Panel::Question),
        "Panel::Question has no body in panels::body and must not open as a blank floating window"
    );
    assert_eq!(
        editor.ui.bottom_tab,
        BottomTab::Question,
        "should switch bottom_tab to BottomTab::Question when docking is off"
    );
}

/// `toolbar::insert_table` omits `"layer": editor.selection.layer`, so inserting
/// a table while a slide layer is active places it on the base slide and drops
/// the selection.
#[test]
fn insert_table_on_slide_layer_inserts_onto_base_slide() {
    let course = Course::from_json(
        r#"{
          "schema_version": 1,
          "id": "layer-table",
          "title": "Layer Table",
          "scenes": [{
            "id": "sc1",
            "title": "Scene 1",
            "slides": [{
              "id": "s1",
              "title": "Slide 1",
              "duration_ms": 5000,
              "layers": [{
                "id": "l1",
                "name": "Layer 1",
                "objects": []
              }]
            }]
          }]
        }"#,
    )
    .expect("valid course");

    let mut editor = Editor::new(course);
    editor.selection.slide = "s1".to_owned();
    editor.selection.layer = Some("l1".to_owned());

    toolbar::insert_table(&mut editor);

    let slide = &editor.session.course.scenes[0].slides[0];
    assert_eq!(
        slide.layers[0].objects.len(),
        1,
        "table inserted while layer l1 is selected should land on l1, not base slide (base has {} objects)",
        slide.objects.len()
    );
}
