//! Bug-hunt proof tests for `coursecraft-render` (0.6.1 Neon review).
//!
//! Each test failed before its fix and now guards against a regression.

use coursecraft_course::{Course, ObjectKind, TextRun};
use coursecraft_logic::{Input, ObjectSample, Player};
use coursecraft_render::chrome::{at_slide_end, play_pause_inputs};
use coursecraft_render::compose::{Visual, apply_sample_to_items, compose, compose_object_items};
use coursecraft_render::player_frame::PlayerFrame;
use coursecraft_render::shape_fill::fill_contours;
use coursecraft_render::{AccessRole, FocusTarget, build_access_tree};
use egui::{Color32, Rect, pos2};

/// `at_slide_end` short-circuits on `player.state().base.ended`, even while a
/// seekable layer (`allow_seek: "yes"`) is actively playing. Clicking Pause
/// then rewinds the layer to 0 (`PlayerSeek { ms: 0 }`) instead of pausing it.
#[test]
fn at_slide_end_true_while_seekable_layer_plays() {
    let course = Course::from_json(
        r#"{
          "schema_version": 1,
          "id": "layer-seek-end",
          "title": "Layer Seek End",
          "scenes": [{
            "id": "sc1",
            "title": "Scene 1",
            "slides": [{
              "id": "s1",
              "title": "Slide 1",
              "duration_ms": 1000,
              "objects": [{
                "id": "open_l",
                "kind": { "type": "button", "label": "Open" },
                "frame": { "x": 10, "y": 10, "w": 80, "h": 30 }
              }],
              "layers": [{
                "id": "l1",
                "name": "Layer 1",
                "duration_ms": 10000,
                "allow_seek": "yes",
                "objects": []
              }],
              "triggers": [{
                "when": { "event": "click", "object": "open_l" },
                "action": { "action": "show_layer", "layer": "l1" }
              }]
            }]
          }]
        }"#,
    )
    .expect("valid course");

    let mut player = Player::new(&course).expect("player");
    let _ = player.handle(&Input::Start);
    let _ = player.handle(&Input::Tick { ms: 1500 }); // base ends
    let _ = player.handle(&Input::Click {
        object: "open_l".to_owned(),
    });
    let _ = player.handle(&Input::Tick { ms: 2000 }); // layer at 2000 / 10000 ms

    assert!(
        !at_slide_end(&player),
        "seekable layer is at 2000/10000 ms, so at_slide_end must be false"
    );
    assert_eq!(
        play_pause_inputs(&player),
        vec![Input::PlayerPause],
        "Play/Pause should pause the active layer instead of rewinding to 0"
    );
}

/// `fill_contours` performs a single pass over contours, so a hole contour
/// listed before its enclosing outer contour is silently dropped (filling the
/// hole solid instead of cutting it out).
#[test]
fn fill_contours_drops_hole_listed_before_outer() {
    let outer = vec![(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)];
    let hole = vec![(30.0, 30.0), (70.0, 30.0), (70.0, 70.0), (30.0, 70.0)];

    let mesh_outer_first = fill_contours(&[outer.clone(), hole.clone()], Color32::RED, 1.0);
    let mesh_hole_first = fill_contours(&[hole, outer], Color32::RED, 1.0);

    let mesh_area = |mesh: &egui::Mesh| -> f32 {
        let mut total = 0.0_f32;
        for tri in mesh.indices.as_chunks::<3>().0 {
            let a = mesh.vertices[tri[0] as usize].pos;
            let b = mesh.vertices[tri[1] as usize].pos;
            let c = mesh.vertices[tri[2] as usize].pos;
            // Only count opaque core triangles (skip transparent fringe).
            if mesh.vertices[tri[0] as usize].color.a() > 0
                && mesh.vertices[tri[1] as usize].color.a() > 0
                && mesh.vertices[tri[2] as usize].color.a() > 0
            {
                total += 0.5 * ((b.x - a.x) * (c.y - a.y) - (c.x - a.x) * (b.y - a.y)).abs();
            }
        }
        total
    };

    let area_outer_first = mesh_area(&mesh_outer_first); // expected ~8400 (10000 - 1600)
    let area_hole_first = mesh_area(&mesh_hole_first);
    assert!(
        (area_outer_first - area_hole_first).abs() < 50.0,
        "hole-first area ({area_hole_first}) should match outer-first area ({area_outer_first}), not fill solid (10000)"
    );
}

/// `compose` performs `%var%` substitution for Text, Button, Shape, Table,
/// Marker, etc., but skips `Visual::Path` text.
#[test]
fn compose_skips_variable_expansion_in_path_text() {
    let course = Course::from_json(
        r##"{
          "schema_version": 1,
          "id": "path-var",
          "title": "My Course Title",
          "scenes": [{
            "id": "sc1",
            "title": "Scene 1",
            "slides": [{
              "id": "s1",
              "title": "Slide 1",
              "duration_ms": 5000,
              "objects": [{
                "id": "p1",
                "kind": {
                  "type": "path",
                  "points": [
                    { "x": 0.0, "y": 0.0 },
                    { "x": 1.0, "y": 0.0 },
                    { "x": 1.0, "y": 1.0 },
                    { "x": 0.0, "y": 1.0 }
                  ],
                  "closed": true,
                  "fill": "#2266CC",
                  "outline": { "color": "#000000", "width": 1.0 },
                  "text": { "text": "%Course.Title%" }
                },
                "frame": { "x": 20, "y": 20, "w": 200, "h": 100 }
              }]
            }]
          }]
        }"##,
    )
    .expect("valid course");

    let mut player = Player::new(&course).expect("player");
    let _ = player.handle(&Input::Start);
    let items = compose(&player);
    let path_text: String = items
        .iter()
        .find_map(|it| match &it.visual {
            Visual::Path(p) => p
                .text
                .as_ref()
                .map(|t| t.runs.iter().map(|r| r.text.as_str()).collect::<String>()),
            _ => None,
        })
        .expect("resolved path text");

    assert_eq!(
        path_text, "My Course Title",
        "Path text must expand %Course.Title%, got {path_text:?}"
    );
}

/// `push_master_items` reads `course_input_value` instead of `player_input_value`,
/// leaving a master-slide `ProgressMeter` bound to `Course.Progress` frozen at 0%.
#[test]
fn master_progress_meter_ignores_live_course_progress() {
    let course = Course::from_json(
        r#"{
          "schema_version": 1,
          "id": "master-meter",
          "title": "Master Meter",
          "variables": [{ "name": "pct", "value": 0 }],
          "masters": [{
            "id": "m1",
            "title": "Master",
            "objects": [{
              "id": "prog",
              "kind": {
                "type": "progress_meter",
                "variable": "pct",
                "min": 0.0,
                "max": 100.0
              },
              "frame": { "x": 10, "y": 10, "w": 200, "h": 20 }
            }]
          }],
          "scenes": [{
            "id": "sc1",
            "title": "Scene 1",
            "slides": [{
              "id": "s1",
              "title": "S1",
              "master": "m1",
              "duration_ms": 5000,
              "triggers": [{
                "when": { "event": "timeline_starts" },
                "action": {
                  "action": "set_variable",
                  "variable": "pct",
                  "op": "assign",
                  "value": 100
                }
              }]
            }]
          }]
        }"#,
    )
    .expect("valid course");

    let mut player = Player::new(&course).expect("player");
    let _ = player.handle(&Input::Start); // trigger sets pct = 100

    let items = compose(&player);
    let fraction = items
        .iter()
        .find_map(|it| match &it.visual {
            Visual::ProgressMeter { fraction, .. } => Some(*fraction),
            _ => None,
        })
        .expect("master progress meter");

    assert!(
        (fraction - 1.0).abs() < 0.01,
        "master ProgressMeter should reflect live pct = 100 (fraction 1.0), got {fraction}"
    );
}

/// `build_access_tree` leaks interactive `FocusTarget::Link` nodes on objects
/// whose state is `"hidden"`, and leaks hidden master objects.
#[test]
fn access_tree_leaks_hidden_object_links_and_hidden_master_objects() {
    let course = Course::from_json(
        r#"{
          "schema_version": 1,
          "id": "a11y-hidden",
          "title": "A11y Hidden",
          "masters": [{
            "id": "m1",
            "title": "Master",
            "objects": [{
              "id": "m_hidden",
              "initial_state": "hidden",
              "kind": { "type": "text", "text": "Secret Master Text" },
              "frame": { "x": 10, "y": 10, "w": 200, "h": 40 }
            }]
          }],
          "scenes": [{
            "id": "sc1",
            "title": "Scene 1",
            "slides": [{
              "id": "s1",
              "title": "Slide 1",
              "master": "m1",
              "duration_ms": 5000,
              "objects": [{
                "id": "hidden_link_box",
                "initial_state": "hidden",
                "kind": {
                  "type": "text",
                  "text": "Hidden Link",
                  "runs": [{
                    "text": "Hidden Link",
                    "link": { "type": "url", "url": "https://example.invalid" }
                  }]
                },
                "frame": { "x": 10, "y": 80, "w": 200, "h": 40 }
              }]
            }]
          }]
        }"#,
    )
    .expect("valid course");

    let mut player = Player::new(&course).expect("player");
    let _ = player.handle(&Input::Start);
    let frame = PlayerFrame::new();
    let items = compose(&player);
    let tree = build_access_tree(&player, &frame, &items, true, true);

    let has_hidden_link = tree.nodes.iter().any(|n| {
        n.role == AccessRole::Link
            || matches!(&n.target, Some(FocusTarget::Link { object, .. }) if object == "hidden_link_box")
    });
    assert!(
        !has_hidden_link,
        "hidden text object must not emit an interactive Link node in AccessTree: {:?}",
        tree.nodes
    );

    let has_hidden_master = tree
        .nodes
        .iter()
        .any(|n| n.label.contains("Secret Master Text"));
    assert!(
        !has_hidden_master,
        "hidden master object must not be emitted in AccessTree: {:?}",
        tree.nodes
    );
}

/// `apply_sample_to_items` adds `+turn` to `first.rotation` (which rotates
/// counter-clockwise when `flip_h != flip_v`), while orbiting each `StateExtra`
/// clockwise via `rotate_point(..., turn)` — tearing extras away from mirrored parents.
#[test]
fn apply_sample_to_items_mirrored_parent_tears_state_extra() {
    let course = Course::from_json(
        r##"{
          "schema_version": 1,
          "id": "mirror-extra",
          "title": "Mirror Extra",
          "scenes": [{
            "id": "sc1",
            "title": "Scene 1",
            "slides": [{
              "id": "s1",
              "title": "Slide 1",
              "duration_ms": 5000,
              "objects": [{
                "id": "card",
                "flip_h": true,
                "initial_state": "active",
                "kind": { "type": "shape", "shape": "rectangle", "fill": "#2266CC" },
                "frame": { "x": 100, "y": 100, "w": 100, "h": 100 },
                "states": [{
                  "name": "active",
                  "extras": [{
                    "id": "badge",
                    "frame": { "x": 180, "y": 140, "w": 20, "h": 20 },
                    "kind": { "type": "shape", "shape": "ellipse", "fill": "#FF0000" }
                  }]
                }]
              }]
            }]
          }]
        }"##,
    )
    .expect("valid course");

    let obj = &course.scenes[0].slides[0].objects[0];
    let mut items = compose_object_items(obj, "active", Some(&course.theme), None);
    assert_eq!(items.len(), 2);

    // Apply a +90 degree clockwise animation turn.
    let sample = ObjectSample {
        rotation_deg: 90.0,
        ..ObjectSample::IDENTITY
    };
    apply_sample_to_items(&mut items, &sample);

    // Because `MeshPost::apply` flips AFTER rotating by `item.rotation`, a
    // mirrored item (`flip_h != flip_v`) needs `rotation -= turn` to rotate
    // clockwise on screen, matching the clockwise orbit of its `StateExtra`.
    assert_eq!(
        items[0].rotation, -90.0,
        "mirrored parent received +90° rotation (counter-clockwise on screen) while extra orbited +90° clockwise"
    );
    let _ = (ObjectKind::Text {
        text: String::new(),
        style: Default::default(),
        runs: Vec::<TextRun>::new(),
        paragraphs: Vec::new(),
        autofit: Default::default(),
        text_style: None,
        outline: None,
        point_text: false,
    },);
}

/// `text_layout::roman` has no upper bound on `Paragraph.start_at`, and
/// `validate()` accepts `start_at = u32::MAX`. With `start_at = 50_000`,
/// `roman` emits 50 consecutive `'m'` characters instead of capping or falling back.
#[test]
fn roman_numbering_unbounded_for_large_start_at() {
    let course = Course::from_json(
        r#"{
          "schema_version": 1,
          "id": "roman-dos",
          "title": "Roman DoS",
          "scenes": [{
            "id": "sc1",
            "title": "Scene 1",
            "slides": [{
              "id": "s1",
              "title": "Slide 1",
              "duration_ms": 5000,
              "objects": [{
                "id": "t1",
                "kind": {
                  "type": "text",
                  "text": "Item",
                  "paragraphs": [{
                    "list": "number",
                    "number_scheme": "roman_uc_period",
                    "start_at": 4294967295
                  }]
                },
                "frame": { "x": 10, "y": 10, "w": 300, "h": 100 }
              }]
            }]
          }]
        }"#,
    )
    .expect("valid json");

    // First verify that `start_at: u32::MAX` passes `validate()` with zero errors!
    assert!(
        course.is_playable(),
        "course with start_at = u32::MAX currently passes validation"
    );
    // Use a smaller value (50_000) to avoid OOM in the test while proving the
    // prefix length grows linearly without bound (50 'M' chars > 20).
    let ctx = egui::Context::default();
    run_frame(&ctx, egui::RawInput::default(), |ui| {
        let p = coursecraft_course::Paragraph {
            list: coursecraft_course::ListKind::Number,
            number_scheme: Some(coursecraft_course::NumberScheme::RomanUcPeriod),
            start_at: Some(50_000),
            ..Default::default()
        };
        let style = coursecraft_course::TextStyle::default();
        let runs = vec![TextRun {
            text: "Item".to_owned(),
            bold: None,
            italic: None,
            underline: None,
            strikethrough: None,
            baseline: None,
            letter_spacing: None,
            size: None,
            color: None,
            link: None,
            font: None,
            highlight: None,
            shadow: None,
        }];
        let resolved = coursecraft_render::resolve_text_runs(
            "Item",
            &style,
            &runs,
            None,
            Color32::BLACK,
            Color32::BLUE,
            None,
        );
        let layout = coursecraft_render::text_layout::layout_text(
            ui.ctx(),
            Rect::from_min_max(pos2(0.0, 0.0), pos2(300.0, 100.0)),
            1.0,
            &resolved,
            &[p],
            coursecraft_course::Align::Left,
            coursecraft_course::VAlign::Top,
            coursecraft_course::Autofit::None,
        );
        let marker_text = layout.paragraphs[0]
            .marker
            .as_ref()
            .expect("roman marker")
            .galley
            .text()
            .to_owned();
        assert!(
            marker_text.len() < 30,
            "Roman marker grew unbounded to {} chars (for u32::MAX it allocates 4.29 MB of 'M's): {:?}",
            marker_text.len(),
            &marker_text[..marker_text.len().min(60)]
        );
    });
}

fn run_frame(ctx: &egui::Context, raw: egui::RawInput, f: impl FnMut(&mut egui::Ui)) {
    let mut out = ctx.run_ui(raw, f);
    out.textures_delta.clear();
}

fn key_event(key: egui::Key) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

/// Spec (`docs/spec/triggers.md`): while holding a drag item via keyboard,
/// "arrows or Tab cycle the open drop targets". In `handle_drag_keys`, plain
/// `Tab` is consumed with `backwards = false` while `forwards` does not include
/// `Tab`, so plain `Tab` does nothing.
#[test]
fn focus_tab_during_keyboard_drag_does_nothing() {
    let course = Course::from_json(
        r##"{
          "schema_version": 1,
          "id": "drag-tab",
          "title": "Drag Tab",
          "scenes": [{
            "id": "sc1",
            "title": "Scene 1",
            "slides": [{
              "id": "s1",
              "title": "Drag Slide",
              "duration_ms": 5000,
              "objects": [
                {
                  "id": "d1",
                  "kind": { "type": "shape", "shape": "rectangle", "fill": "#2266CC" },
                  "frame": { "x": 20, "y": 20, "w": 80, "h": 40 }
                },
                {
                  "id": "t1",
                  "kind": { "type": "shape", "shape": "rectangle", "fill": "#EEEEEE" },
                  "frame": { "x": 300, "y": 20, "w": 120, "h": 80 }
                },
                {
                  "id": "t2",
                  "kind": { "type": "shape", "shape": "rectangle", "fill": "#EEEEEE" },
                  "frame": { "x": 300, "y": 140, "w": 120, "h": 80 }
                }
              ],
              "question": {
                "kind": "drag_drop",
                "prompt": "Drag",
                "choices": [],
                "drag_items": [{ "item": "d1", "target": "t1" }],
                "drop_targets": ["t1", "t2"]
              }
            }]
          }]
        }"##,
    )
    .expect("valid course");

    let mut player = Player::new(&course).expect("player");
    let _ = player.handle(&Input::Start);

    let ctx = egui::Context::default();
    let mut focus = coursecraft_render::FocusManager::new();
    let mut drawer_open = false;

    // Frame 1: Tab reveals focus on d1.
    let mut raw = egui::RawInput::default();
    raw.events.push(key_event(egui::Key::Tab));
    run_frame(&ctx, raw, |ui| {
        let _ = focus.update(ui, &player, false, false, &mut drawer_open, None);
    });
    assert_eq!(
        focus.focused(),
        Some(&FocusTarget::DragItem("d1".to_owned()))
    );

    // Frame 2: Space picks up d1.
    let mut raw = egui::RawInput::default();
    raw.events.push(key_event(egui::Key::Space));
    run_frame(&ctx, raw, |ui| {
        let _ = focus.update(ui, &player, false, false, &mut drawer_open, None);
    });
    let before_target = focus
        .drag_visual()
        .expect("drag should be active after Space")
        .target
        .clone();

    // Frame 3: Plain Tab should cycle to the next drop target.
    let mut raw = egui::RawInput::default();
    raw.events.push(key_event(egui::Key::Tab));
    run_frame(&ctx, raw, |ui| {
        let _ = focus.update(ui, &player, false, false, &mut drawer_open, None);
    });
    let after_target = focus
        .drag_visual()
        .expect("drag should remain active after Tab")
        .target
        .clone();

    assert_ne!(
        after_target, before_target,
        "plain Tab while holding a drag item must advance the drop target, stayed at {after_target:?}"
    );
}

/// Spec (`docs/spec/triggers.md`): "radio selects the next option on Space".
/// `FocusManager::handle_keys` returns immediately after `handle_radio_keys`,
/// so Space never activates the radio group (and leaks `Input::KeyPressed`).
#[test]
fn focus_space_on_radio_group_does_not_advance() {
    let course = Course::from_json(
        r#"{
          "schema_version": 1,
          "id": "radio-space",
          "title": "Radio Space",
          "variables": [{ "name": "choice", "value": "a" }],
          "scenes": [{
            "id": "sc1",
            "title": "Scene 1",
            "slides": [{
              "id": "s1",
              "title": "Radio Slide",
              "duration_ms": 5000,
              "objects": [{
                "id": "rg",
                "kind": {
                  "type": "radio_group",
                  "variable": "choice",
                  "options": [
                    { "label": "Alpha", "value": "a" },
                    { "label": "Beta", "value": "b" }
                  ]
                },
                "frame": { "x": 20, "y": 20, "w": 200, "h": 80 }
              }]
            }]
          }]
        }"#,
    )
    .expect("valid course");

    let mut player = Player::new(&course).expect("player");
    let _ = player.handle(&Input::Start);

    let ctx = egui::Context::default();
    let mut focus = coursecraft_render::FocusManager::new();
    let mut drawer_open = false;

    // Frame 1: Tab reveals focus on rg.
    let mut raw = egui::RawInput::default();
    raw.events.push(key_event(egui::Key::Tab));
    run_frame(&ctx, raw, |ui| {
        let _ = focus.update(ui, &player, false, false, &mut drawer_open, None);
    });
    assert_eq!(focus.focused(), Some(&FocusTarget::Object("rg".to_owned())));

    // Frame 2: Space should select next option ("b").
    let mut raw = egui::RawInput::default();
    raw.events.push(key_event(egui::Key::Space));
    let mut emitted = Vec::new();
    run_frame(&ctx, raw, |ui| {
        emitted = focus.update(ui, &player, false, false, &mut drawer_open, None);
    });

    assert!(
        emitted.iter().any(|i| matches!(
            i,
            Input::SetInputValue { object, value: coursecraft_course::Value::Text(v) }
                if object == "rg" && v == "b"
        )),
        "pressing Space on focused RadioGroup should emit SetInputValue(\"b\"), got: {emitted:?}"
    );
}

/// When the outline drawer is open (`frame.drawer_open() == true`),
/// `build_access_tree` still emits all slide content and native `<input>`
/// controls behind the modal drawer, breaking the web drawer focus trap.
#[test]
fn access_tree_open_drawer_leaks_slide_inputs() {
    let course = Course::from_json(
        r#"{
          "schema_version": 1,
          "id": "drawer-leak",
          "title": "Drawer Leak",
          "variables": [{ "name": "name", "value": "" }],
          "scenes": [{
            "id": "sc1",
            "title": "Scene 1",
            "slides": [{
              "id": "s1",
              "title": "Slide 1",
              "duration_ms": 5000,
              "objects": [{
                "id": "entry1",
                "kind": { "type": "text_entry", "variable": "name", "placeholder": "Your name" },
                "frame": { "x": 20, "y": 20, "w": 240, "h": 40 }
              }]
            }]
          }]
        }"#,
    )
    .expect("valid course");

    let mut player = Player::new(&course).expect("player");
    let _ = player.handle(&Input::Start);
    let mut frame = PlayerFrame::new();
    frame.open_drawer();
    assert!(frame.drawer_open());

    let items = compose(&player);
    let tree = build_access_tree(&player, &frame, &items, true, true);

    let leaked_input = tree
        .nodes
        .iter()
        .any(|n| n.role == AccessRole::TextBox && n.key.contains("entry1"));
    assert!(
        !leaked_input,
        "slide TextBox behind an open drawer must not be emitted into AccessTree: {:?}",
        tree.nodes
    );
}

/// `PanelDrag` computes and stores `on_thumb` when pressing on a `ScrollPanel`
/// scrollbar thumb, but never reads it: pointer move always computes
/// `start_offset - delta * ratio`, making dragging the scrollbar thumb down
/// scroll UP (and clamping to 0 when starting at the top).
#[test]
fn scroll_panel_thumb_drag_down_scrolls_wrong_way() {
    let course = Course::from_json(
        r#"{
          "schema_version": 1,
          "id": "scroll-thumb",
          "title": "Scroll Thumb",
          "slide_size": { "w": 800, "h": 600 },
          "scenes": [{
            "id": "sc1",
            "title": "Scene 1",
            "slides": [{
              "id": "s1",
              "title": "Slide 1",
              "duration_ms": 5000,
              "objects": [{
                "id": "sp1",
                "kind": { "type": "scroll_panel", "content_height": 1000.0 },
                "frame": { "x": 100, "y": 100, "w": 400, "h": 300 }
              }]
            }]
          }]
        }"#,
    )
    .expect("valid course");

    let mut player = Player::new(&course).expect("player");
    let _ = player.handle(&Input::Start);
    // Start scrolled halfway down (offset = 200) so we can clearly see reverse direction.
    let _ = player.handle(&Input::PanelScrolled {
        object: "sp1".to_owned(),
        offset: 200.0,
    });
    assert_eq!(player.panel_offset("sp1"), 200.0);

    let ctx = egui::Context::default();
    let mut view = coursecraft_render::SlideView::new();
    let screen = Rect::from_min_max(pos2(0.0, 0.0), pos2(800.0, 600.0));

    // Frame 1: Initial render to populate items and locate thumb rect.
    let mut raw = egui::RawInput {
        screen_rect: Some(screen),
        ..egui::RawInput::default()
    };
    run_frame(&ctx, raw.clone(), |ui| {
        let _ = view.show(ui, &player, None);
    });
    let thumb_center = view
        .last_items()
        .iter()
        .find_map(|it| match &it.visual {
            Visual::ScrollPanel { thumb: Some(t), .. } => Some(t.center()),
            _ => None,
        })
        .expect("scroll panel thumb rect");
    let transform = view.last_transform().expect("transform");
    let start_screen = transform.to_screen_pos(thumb_center);
    let end_screen = pos2(start_screen.x, start_screen.y + 40.0); // drag DOWN by 40 px

    // Frame 2: Press on the scrollbar thumb.
    raw.events.push(egui::Event::PointerButton {
        pos: start_screen,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    run_frame(&ctx, raw, |ui| {
        let _ = view.show(ui, &player, None);
    });

    // Frame 3: Drag thumb downward (+40 px in Y).
    let mut raw_move = egui::RawInput {
        screen_rect: Some(screen),
        ..egui::RawInput::default()
    };
    raw_move.events.push(egui::Event::PointerMoved(end_screen));
    let mut emitted = Vec::new();
    run_frame(&ctx, raw_move, |ui| {
        emitted = view.show(ui, &player, None);
    });

    let new_offset = emitted
        .iter()
        .find_map(|i| match i {
            Input::PanelScrolled { object, offset } if object == "sp1" => Some(*offset),
            _ => None,
        })
        .expect("dragging thumb should emit PanelScrolled");
    assert!(
        new_offset > 200.0,
        "dragging scrollbar thumb DOWN from offset 200 must increase offset (> 200), got {new_offset}"
    );
}
