//! M27 Step D-7: Slide Master, Layouts & Templates (D9.1–D9.10) and Scenes & Story View (D10.1–D10.10).

use crate::EditorRequest;
use crate::test_harness::TestHarness;
use coursecraft_course::{Action, Course, Event as CourseEvent, SlideTarget, Trigger};
use egui::{Key, Modifiers, pos2, vec2};

fn multi_scene_course() -> Course {
    serde_json::from_value(serde_json::json!({
        "schema_version": 1,
        "id": "m27-masters-scenes",
        "title": "M27 Masters & Scenes",
        "slide_size": { "w": 1280, "h": 720 },
        "scenes": [
            {
                "id": "scene-1",
                "title": "Scene 1",
                "slides": [
                    { "id": "s1", "title": "Slide 1", "duration_ms": 5000, "objects": [] },
                    { "id": "s2", "title": "Slide 2", "duration_ms": 5000, "objects": [] },
                    { "id": "s3", "title": "Slide 3", "duration_ms": 5000, "objects": [] }
                ]
            },
            {
                "id": "scene-2",
                "title": "Scene 2",
                "slides": [
                    { "id": "s4", "title": "Slide 4", "duration_ms": 5000, "objects": [] },
                    { "id": "s5", "title": "Slide 5", "duration_ms": 5000, "objects": [] },
                    { "id": "s6", "title": "Slide 6", "duration_ms": 5000, "objects": [] }
                ]
            },
            {
                "id": "scene-3",
                "title": "Scene 3",
                "slides": [
                    { "id": "s7", "title": "Slide 7", "duration_ms": 5000, "objects": [] },
                    { "id": "s8", "title": "Slide 8", "duration_ms": 5000, "objects": [] }
                ]
            }
        ]
    }))
    .expect("multi_scene_course")
}

fn setup_harness(course: Course) -> TestHarness {
    let mut h = TestHarness::new(course);
    h.editor.ui.show_welcome = false;
    h.editor.selection.slide = "s1".into();
    h.editor.selection.also_slides.clear();
    h.editor.selection.layer = None;
    h.editor.selection.clear_objects();
    h.idle_frame();
    h.idle_frame();
    h
}

// ---------------------------------------------------------------------------
// D9: Slide Master, Layouts & Templates (10 cases)
// ---------------------------------------------------------------------------

#[test]
fn d9_1_master_view_round_trip_restores_slide_and_blocks_canvas_select() {
    let mut h = setup_harness(multi_scene_course());
    h.editor.selection.slide = "s3".into();
    h.idle_frame();

    // Enter Master View.
    h.editor.enter_master_view();
    h.idle_frame();
    assert!(h.editor.ui.master_view);
    assert_eq!(h.editor.ui.master_view_return_slide.as_deref(), Some("s3"));

    // Add master "Corporate" + logo shape + background.
    let mid = h
        .editor
        .run("master.add", serde_json::json!({ "title": "Corporate" }))
        .expect("master.add")["id"]
        .as_str()
        .expect("master id")
        .to_owned();
    h.editor.selection.slide = mid.clone();
    h.editor
        .run(
            "slide.set_background",
            serde_json::json!({ "slide": mid, "background": "#1E293B" }),
        )
        .expect("master bg");
    let logo_id = h
        .editor
        .run(
            "object.add",
            serde_json::json!({
                "slide": mid,
                "name": "Corp Logo",
                "kind": { "type": "shape", "shape": "rectangle", "fill": "#38BDF8", "text": { "text": "LOGO" } },
                "frame": { "x": 100.0, "y": 100.0, "w": 160.0, "h": 80.0 }
            }),
        )
        .expect("master logo")["id"]
        .as_str()
        .expect("logo id")
        .to_owned();
    h.idle_frame();

    // Leave Master View -> restores Slide 3 (not Slide 1).
    h.editor.leave_master_view();
    h.idle_frame();
    assert!(!h.editor.ui.master_view);
    assert_eq!(h.editor.selection.slide, "s3");

    // Apply master to Slide 3.
    h.editor
        .run(
            "slide.set_master",
            serde_json::json!({ "slides": ["s3"], "master": mid }),
        )
        .expect("set_master");
    h.idle_frame();
    assert_eq!(
        h.editor
            .session
            .course
            .slide("s3")
            .unwrap()
            .master
            .as_deref(),
        Some(mid.as_str())
    );

    // Clicking on the master logo position on Slide 3 canvas must NOT select the master object.
    let slide_rect = h.editor.ui.last_slide_rect.expect("slide rect");
    let sx = slide_rect.width() / 1280.0;
    let sy = slide_rect.height() / 720.0;
    let logo_pos = pos2(slide_rect.min.x + 180.0 * sx, slide_rect.min.y + 140.0 * sy);
    h.click_at(logo_pos);
    h.idle_frame();
    assert!(
        h.editor.selection.object.is_none(),
        "master object {logo_id} must not be selectable on play slide canvas"
    );
}

#[test]
fn d9_2_apply_master_single_multi_clear_and_undo() {
    let mut h = setup_harness(multi_scene_course());
    let mid = h
        .editor
        .run("master.add", serde_json::json!({ "title": "Corporate" }))
        .expect("master.add")["id"]
        .as_str()
        .expect("id")
        .to_owned();

    // Multi-select Slides 1 & 2 and apply master via slide.set_master.
    h.editor.selection.slide = "s1".into();
    h.editor.selection.also_slides = vec!["s2".into()];
    h.editor
        .run(
            "slide.set_master",
            serde_json::json!({ "slides": ["s1", "s2"], "master": mid }),
        )
        .expect("set_master multi");
    assert_eq!(
        h.editor
            .session
            .course
            .slide("s1")
            .unwrap()
            .master
            .as_deref(),
        Some(mid.as_str())
    );
    assert_eq!(
        h.editor
            .session
            .course
            .slide("s2")
            .unwrap()
            .master
            .as_deref(),
        Some(mid.as_str())
    );

    // Clear master on Slide 1 (None -> falls back to theme background).
    h.editor
        .run(
            "slide.set_master",
            serde_json::json!({ "slides": ["s1"], "master": null }),
        )
        .expect("clear master");
    assert_eq!(h.editor.session.course.slide("s1").unwrap().master, None);
    assert_eq!(
        h.editor
            .session
            .course
            .slide("s2")
            .unwrap()
            .master
            .as_deref(),
        Some(mid.as_str())
    );

    // Undo x 2 restores s1's master, then clears both.
    h.editor
        .run("edit.undo", serde_json::json!({}))
        .expect("undo 1");
    assert_eq!(
        h.editor
            .session
            .course
            .slide("s1")
            .unwrap()
            .master
            .as_deref(),
        Some(mid.as_str())
    );
    h.editor
        .run("edit.undo", serde_json::json!({}))
        .expect("undo 2");
    assert_eq!(h.editor.session.course.slide("s1").unwrap().master, None);
    assert_eq!(h.editor.session.course.slide("s2").unwrap().master, None);
}

#[test]
fn d9_3_delete_master_in_use_counts_slides_and_templates_and_undoes() {
    let mut h = setup_harness(multi_scene_course());
    let mid = h
        .editor
        .run("master.add", serde_json::json!({ "title": "Corporate" }))
        .expect("master.add")["id"]
        .as_str()
        .expect("id")
        .to_owned();

    // Assign Corporate to 2 slides AND 1 template.
    h.editor
        .run(
            "slide.set_master",
            serde_json::json!({ "slides": ["s1", "s2"], "master": mid }),
        )
        .expect("set_master");
    let tpl_id = h
        .editor
        .run(
            "template.save",
            serde_json::json!({ "slide": "s1", "title": "Tpl With Master" }),
        )
        .expect("template.save")["id"]
        .as_str()
        .expect("tpl id")
        .to_owned();
    assert_eq!(
        h.editor
            .session
            .course
            .templates
            .iter()
            .find(|t| t.id == tpl_id)
            .unwrap()
            .master
            .as_deref(),
        Some(mid.as_str())
    );

    // Open delete confirmation dialog for the master.
    h.editor.ui.pending_delete_master = Some(mid.clone());
    h.idle_frame();
    h.idle_frame();

    // Verify confirmation dialog shows accurate usage count (2 slides + 1 template = 3).
    assert!(
        h.find_text_containing("3 slide(s)").is_some(),
        "confirmation dialog should report 3 usages (2 slides + 1 template)"
    );

    // Confirm delete (last match is the button, first is dialog title).
    let confirm_pos = h
        .find_text_last("Delete master")
        .expect("Delete master button");
    h.click_at(confirm_pos);
    h.idle_frame();

    assert!(h.editor.session.course.master(&mid).is_none());
    assert_eq!(h.editor.session.course.slide("s1").unwrap().master, None);
    assert_eq!(h.editor.session.course.slide("s2").unwrap().master, None);
    assert_eq!(
        h.editor
            .session
            .course
            .templates
            .iter()
            .find(|t| t.id == tpl_id)
            .unwrap()
            .master,
        None
    );

    // Single Ctrl+Z restores master AND re-attaches all 2 slides + 1 template.
    h.press(Key::Z, Modifiers::COMMAND);
    h.idle_frame();
    assert!(h.editor.session.course.master(&mid).is_some());
    assert_eq!(
        h.editor
            .session
            .course
            .slide("s1")
            .unwrap()
            .master
            .as_deref(),
        Some(mid.as_str())
    );
    assert_eq!(
        h.editor
            .session
            .course
            .slide("s2")
            .unwrap()
            .master
            .as_deref(),
        Some(mid.as_str())
    );
    assert_eq!(
        h.editor
            .session
            .course
            .templates
            .iter()
            .find(|t| t.id == tpl_id)
            .unwrap()
            .master
            .as_deref(),
        Some(mid.as_str())
    );
}

#[test]
fn d9_4_inline_master_rename_enter_esc_empty_and_huge() {
    let mut h = setup_harness(multi_scene_course());
    let mid = h
        .editor
        .run("master.add", serde_json::json!({ "title": "Corporate" }))
        .expect("master.add")["id"]
        .as_str()
        .expect("id")
        .to_owned();
    h.editor.enter_master_view();
    h.editor.selection.slide = mid.clone();
    h.idle_frame();
    h.idle_frame();

    // 1. Empty string ignored via rename UI.
    h.editor.ui.master_rename = Some(crate::slides::SceneRename {
        scene: mid.clone(),
        buffer: "   ".into(),
        original: "Corporate".into(),
        just_entered: true,
    });
    h.idle_frame();
    h.press(Key::Enter, Modifiers::NONE);
    h.idle_frame();
    assert_eq!(
        h.editor.session.course.master(&mid).unwrap().title,
        "Corporate"
    );

    // 2. Esc cancels.
    h.editor.ui.master_rename = Some(crate::slides::SceneRename {
        scene: mid.clone(),
        buffer: "Cancelled".into(),
        original: "Corporate".into(),
        just_entered: true,
    });
    h.idle_frame();
    h.press(Key::Escape, Modifiers::NONE);
    h.idle_frame();
    assert_eq!(
        h.editor.session.course.master(&mid).unwrap().title,
        "Corporate"
    );

    // 3. "Dark Hero" + Enter renames + undoable.
    h.editor.ui.master_rename = Some(crate::slides::SceneRename {
        scene: mid.clone(),
        buffer: "Dark Hero".into(),
        original: "Corporate".into(),
        just_entered: true,
    });
    h.idle_frame();
    h.press(Key::Enter, Modifiers::NONE);
    h.idle_frame();
    assert_eq!(
        h.editor.session.course.master(&mid).unwrap().title,
        "Dark Hero"
    );
    h.editor
        .run("edit.undo", serde_json::json!({}))
        .expect("undo");
    assert_eq!(
        h.editor.session.course.master(&mid).unwrap().title,
        "Corporate"
    );

    // 4. 300-char title updates cleanly without clipping the panel.
    let huge = "M".repeat(300);
    let _ = h.editor.run(
        "slide.update",
        serde_json::json!({ "slide": mid, "title": huge }),
    );
    let report = h.layout_report(2);
    assert!(
        report.findings.iter().all(|f| f.contains("MMMM")),
        "huge master title must not clip or push any surrounding panel UI: {report:?}"
    );
}

#[test]
fn d9_5_forbidden_content_on_master_slides() {
    let mut h = setup_harness(multi_scene_course());
    let mid = h
        .editor
        .run("master.add", serde_json::json!({ "title": "Master 1" }))
        .expect("master.add")["id"]
        .as_str()
        .expect("id")
        .to_owned();
    h.editor.enter_master_view();
    h.editor.selection.slide = mid.clone();
    h.idle_frame();

    // Engine refuses Audio, Video, Lottie, Layer, Trigger, Question, Draw on master ID.
    assert!(
        h.editor
            .session
            .execute(
                "object.add",
                &serde_json::json!({
                    "slide": mid,
                    "kind": { "type": "audio", "src": "a.wav" },
                    "frame": { "x": 10.0, "y": 10.0, "w": 40.0, "h": 40.0 }
                }),
            )
            .is_err()
    );
    assert!(
        h.editor
            .session
            .execute(
                "object.add",
                &serde_json::json!({
                    "slide": mid,
                    "kind": { "type": "video", "src": "v.mp4" },
                    "frame": { "x": 10.0, "y": 10.0, "w": 160.0, "h": 90.0 }
                }),
            )
            .is_err()
    );
    assert!(
        h.editor
            .session
            .execute(
                "object.add",
                &serde_json::json!({
                    "slide": mid,
                    "kind": { "type": "lottie", "src": "a.json" },
                    "frame": { "x": 10.0, "y": 10.0, "w": 100.0, "h": 100.0 }
                }),
            )
            .is_err()
    );
    assert!(
        h.editor
            .session
            .execute(
                "layer.add",
                &serde_json::json!({ "slide": mid, "name": "L" })
            )
            .is_err()
    );
    assert!(
        h.editor
            .session
            .execute(
                "trigger.add",
                &serde_json::json!({
                    "slide": mid,
                    "trigger": {
                        "id": "t1",
                        "event": { "type": "timeline_starts" },
                        "action": { "type": "pause_timeline" }
                    }
                }),
            )
            .is_err()
    );
    assert!(
        h.editor
            .session
            .execute(
                "question.insert",
                &serde_json::json!({ "slide": mid, "kind": "multiple_choice" }),
            )
            .is_err()
    );
    assert!(
        h.editor
            .session
            .execute(
                "draw.insert",
                &serde_json::json!({ "slide": mid, "bank": "b1" }),
            )
            .is_err()
    );

    assert!(
        h.editor
            .session
            .course
            .validate()
            .into_iter()
            .all(|i| i.severity != coursecraft_course::Severity::Error)
    );
}

#[test]
fn d9_6_slide_set_master_sets_and_clears_on_question_bank_slide() {
    let mut h = setup_harness(multi_scene_course());
    let mid = h
        .editor
        .run("master.add", serde_json::json!({ "title": "M" }))
        .expect("master.add")["id"]
        .as_str()
        .expect("id")
        .to_owned();
    let bid = h
        .editor
        .run("bank.create", serde_json::json!({ "title": "Bank 1" }))
        .expect("bank.create")["id"]
        .as_str()
        .expect("bank id")
        .to_owned();
    let bslide = h
        .editor
        .run(
            "bank.add_question",
            serde_json::json!({ "bank": bid, "title": "Bank Q" }),
        )
        .expect("bank.add_question")["id"]
        .as_str()
        .expect("bank slide id")
        .to_owned();

    h.editor
        .run(
            "slide.set_master",
            serde_json::json!({ "slides": [bslide.clone()], "master": mid }),
        )
        .expect("bank slide takes a master");
    let master_of = |h: &crate::test_harness::TestHarness| {
        h.editor
            .session
            .course
            .bank_slide(&bslide)
            .and_then(|slide| slide.master.clone())
    };
    assert_eq!(master_of(&h), Some(mid));
    h.editor
        .run(
            "slide.set_master",
            serde_json::json!({ "slides": [bslide.clone()], "master": null }),
        )
        .expect("clear the bank slide's master");
    assert_eq!(master_of(&h), None);
}

#[test]
fn d9_7_all_six_builtin_layouts_via_popup_and_undo() {
    let mut h = setup_harness(multi_scene_course());
    let layouts = [
        "Blank",
        "Title",
        "Title and content",
        "Two columns",
        "Picture and text",
        "Question",
    ];

    for label in layouts {
        let caret = h
            .find_icon(coursecraft_render::Icon::CaretDown)
            .expect("toolbar caret");
        h.click_at(caret);
        h.idle_frame();
        let btn = h.find_text_last(label).expect("layout button");
        h.click_at(btn);
        h.idle_frame();
    }

    assert_eq!(h.editor.session.course.slides().count(), 8 + 6);
    assert!(
        h.editor
            .session
            .course
            .validate()
            .into_iter()
            .all(|i| i.severity != coursecraft_course::Severity::Error)
    );

    // Undo x 6 removes all 6 layout slides cleanly.
    for _ in 0..6 {
        h.editor
            .run("edit.undo", serde_json::json!({}))
            .expect("undo");
    }
    assert_eq!(h.editor.session.course.slides().count(), 8);
}

#[test]
fn d9_8_in_course_templates_save_insert_delete_and_undo() {
    let mut h = setup_harness(multi_scene_course());
    let mid = h
        .editor
        .run("master.add", serde_json::json!({ "title": "M" }))
        .expect("master.add")["id"]
        .as_str()
        .expect("id")
        .to_owned();
    h.editor
        .run(
            "slide.set_master",
            serde_json::json!({ "slides": ["s2"], "master": mid }),
        )
        .expect("set_master");
    h.editor
        .run(
            "object.add",
            serde_json::json!({
                "slide": "s2",
                "name": "Box",
                "kind": { "type": "shape", "shape": "rectangle", "fill": "#2563EB", "text": { "text": "Hi" } },
                "frame": { "x": 40.0, "y": 40.0, "w": 120.0, "h": 60.0 }
            }),
        )
        .expect("object.add");
    h.editor
        .run(
            "layer.add",
            serde_json::json!({ "slide": "s2", "name": "L1" }),
        )
        .expect("layer.add");
    h.editor
        .run(
            "trigger.add",
            serde_json::json!({
                "slide": "s2",
                "trigger": {
                    "when": { "event": "timeline_starts" },
                    "action": { "action": "pause_timeline" }
                }
            }),
        )
        .expect("trigger.add");

    // Save Slide 2 as template.
    let tid = h
        .editor
        .run(
            "template.save",
            serde_json::json!({ "slide": "s2", "title": "Saved Tpl" }),
        )
        .expect("template.save")["id"]
        .as_str()
        .expect("tid")
        .to_owned();
    assert_eq!(h.editor.session.course.templates.len(), 1);

    // Insert slide from template -> fresh IDs + kept master.
    let new_sid = h
        .editor
        .run(
            "template.add_slide",
            serde_json::json!({ "template": tid, "scene": "scene-1", "after": "s2" }),
        )
        .expect("template.add_slide")["id"]
        .as_str()
        .expect("new_sid")
        .to_owned();
    assert_ne!(new_sid, "s2");
    let inserted = h.editor.session.course.slide(&new_sid).unwrap();
    assert_eq!(inserted.master.as_deref(), Some(mid.as_str()));
    assert_eq!(inserted.objects.len(), 1);
    assert_ne!(
        inserted.objects[0].id,
        h.editor.session.course.slide("s2").unwrap().objects[0].id
    );
    assert_eq!(inserted.layers.len(), 1);
    assert_eq!(inserted.triggers.len(), 1);

    // Delete template.
    h.editor
        .run("template.delete", serde_json::json!({ "template": tid }))
        .expect("template.delete");
    assert!(h.editor.session.course.templates.is_empty());

    // Undo x 3 restores template, removes inserted slide, removes saved template.
    h.editor
        .run("edit.undo", serde_json::json!({}))
        .expect("undo 1");
    assert_eq!(h.editor.session.course.templates.len(), 1);
    h.editor
        .run("edit.undo", serde_json::json!({}))
        .expect("undo 2");
    assert!(h.editor.session.course.slide(&new_sid).is_none());
    h.editor
        .run("edit.undo", serde_json::json!({}))
        .expect("undo 3");
    assert!(h.editor.session.course.templates.is_empty());
}

#[test]
fn d9_9_shared_library_templates_requests_and_delete_confirm() {
    let mut h = setup_harness(multi_scene_course());
    h.editor.set_library_templates_available(true);
    h.editor
        .set_library_templates(vec![crate::LibraryTemplateEntry {
            id: "lib-1".to_owned(),
            title: "Library Card".to_owned(),
        }]);
    h.editor
        .run(
            "template.save",
            serde_json::json!({ "slide": "s1", "title": "Local Tpl" }),
        )
        .expect("template.save");
    h.idle_frame();

    // Open + New slide caret popup -> Insert from Library.
    let caret = h
        .find_icon(coursecraft_render::Icon::CaretDown)
        .expect("toolbar caret");
    h.click_at(caret);
    h.idle_frame();
    let lib_btn = h.find_text("Library Card").expect("Library Card button");
    let reqs = h.click_at_requests(lib_btn);
    assert!(reqs.iter().any(|r| matches!(
        r,
        EditorRequest::InsertLibraryTemplate { id, .. } if id == "lib-1"
    )));

    // Reopen -> Delete -> Cancel does nothing.
    let caret = h
        .find_icon(coursecraft_render::Icon::CaretDown)
        .expect("toolbar caret");
    h.click_at(caret);
    h.idle_frame();
    let del_btn = h.find_text_last("Delete").expect("Delete button");
    h.click_at(del_btn);
    h.idle_frame();
    let cancel_btn = h.find_text("Cancel").expect("Cancel button");
    let reqs = h.click_at_requests(cancel_btn);
    assert!(
        !reqs
            .iter()
            .any(|r| matches!(r, EditorRequest::DeleteLibraryTemplate { .. }))
    );

    // Delete -> Confirm emits DeleteLibraryTemplate.
    let del_btn = h.find_text_last("Delete").expect("Delete button");
    h.click_at(del_btn);
    h.idle_frame();
    let confirm_btn = h.find_text("Confirm").expect("Confirm button");
    let reqs = h.click_at_requests(confirm_btn);
    assert!(reqs.iter().any(|r| matches!(
        r,
        EditorRequest::DeleteLibraryTemplate { id } if id == "lib-1"
    )));

    let report = h.layout_report(2);
    assert!(
        report.findings.is_empty(),
        "layout_report clean: {report:?}"
    );
}

#[test]
fn d9_10_master_view_shortcut_and_delete_isolation() {
    let mut h = setup_harness(multi_scene_course());
    h.editor
        .run("master.add", serde_json::json!({ "title": "M1" }))
        .expect("master.add");
    h.editor.enter_master_view();
    h.idle_frame();

    let play_count_before = h.editor.session.course.slides().count();

    // Delete key in Master View must NOT delete play slides.
    h.editor.ui.last_active_area = crate::ActiveArea::Navigator;
    h.press(Key::Delete, Modifiers::NONE);
    h.idle_frame();
    assert_eq!(h.editor.session.course.slides().count(), play_count_before);

    // Ctrl+Shift+M (Story View) is blocked in Master View.
    h.press(Key::M, Modifiers::COMMAND | Modifiers::SHIFT);
    h.idle_frame();
    assert!(!h.editor.ui.story_view);
    assert!(h.editor.ui.master_view);

    // Ctrl+M in Master View adds a master, never a hidden play slide.
    let masters_before = h.editor.session.course.masters.len();
    h.press(Key::M, Modifiers::COMMAND);
    h.idle_frame();
    assert_eq!(h.editor.session.course.slides().count(), play_count_before);
    assert!(h.editor.session.course.masters.len() >= masters_before);
}

// ---------------------------------------------------------------------------
// D10: Scenes & Story View / Course Map (10 cases)
// ---------------------------------------------------------------------------

#[test]
fn d10_1_story_view_toggle_arrows_enter_and_double_click_exit() {
    let mut h = setup_harness(multi_scene_course());
    h.press(Key::M, Modifiers::COMMAND | Modifiers::SHIFT);
    h.idle_frame();
    assert!(h.editor.ui.story_view);

    h.press(Key::ArrowRight, Modifiers::NONE);
    h.idle_frame();
    assert_eq!(h.editor.selection.slide, "s2");
    h.press(Key::ArrowDown, Modifiers::NONE);
    h.idle_frame();
    assert_eq!(h.editor.selection.slide, "s3");
    h.press(Key::ArrowLeft, Modifiers::NONE);
    h.idle_frame();
    assert_eq!(h.editor.selection.slide, "s2");
    h.press(Key::ArrowUp, Modifiers::NONE);
    h.idle_frame();
    assert_eq!(h.editor.selection.slide, "s1");

    // Enter exits Story View on selected slide.
    h.press(Key::ArrowRight, Modifiers::NONE);
    h.idle_frame();
    h.press(Key::Enter, Modifiers::NONE);
    h.idle_frame();
    assert!(!h.editor.ui.story_view);
    assert_eq!(h.editor.selection.slide, "s2");

    // Reopen and double-click Slide 5 thumbnail to exit onto s5.
    h.press(Key::M, Modifiers::COMMAND | Modifiers::SHIFT);
    h.idle_frame();
    let s5_rect = h
        .editor
        .ui
        .course_map
        .thumb_rects
        .iter()
        .find(|(id, _)| id == "s5")
        .map(|(_, r)| *r)
        .expect("s5 thumb rect");
    h.double_click_at(s5_rect.center());
    h.idle_frame();
    assert!(!h.editor.ui.story_view);
    assert_eq!(h.editor.selection.slide, "s5");
}

#[test]
fn d10_2_story_view_click_clears_stale_also_slides_bug_d7() {
    let mut h = setup_harness(multi_scene_course());
    // Simulate Shift+Click Slides 1-3 in Slide Navigator.
    h.editor.selection.slide = "s1".into();
    h.editor
        .selection
        .select_slide_range(&h.editor.session.course, "s3");
    assert_eq!(h.editor.selection.also_slides.len(), 2);

    // Enter Story View and click Slide 2.
    h.editor.ui.story_view = true;
    h.idle_frame();
    let s2_rect = h
        .editor
        .ui
        .course_map
        .thumb_rects
        .iter()
        .find(|(id, _)| id == "s2")
        .map(|(_, r)| *r)
        .expect("s2 thumb rect");
    h.click_at(s2_rect.center());
    h.idle_frame();
    assert_eq!(h.editor.selection.slide, "s2");
    assert!(
        h.editor.selection.also_slides.is_empty(),
        "BUG-D7: clicking a slide in Story View must clear also_slides"
    );

    // Exit Story View and delete from Navigator -> removes ONLY Slide 2, not Slides 1-3!
    h.editor.ui.story_view = false;
    h.editor.ui.last_active_area = crate::ActiveArea::Navigator;
    h.idle_frame();
    h.press(Key::Delete, Modifiers::NONE);
    h.idle_frame();
    assert!(h.editor.session.course.slide("s1").is_some());
    assert!(h.editor.session.course.slide("s2").is_none());
    assert!(h.editor.session.course.slide("s3").is_some());
}

#[test]
fn d10_3_story_view_keys_blocked_when_modal_or_preview_open_bug_d7() {
    let mut h = setup_harness(multi_scene_course());
    h.editor.ui.story_view = true;
    h.idle_frame();
    assert_eq!(h.editor.selection.slide, "s1");

    // 1. Find modal open (Ctrl+F).
    h.press(Key::F, Modifiers::COMMAND);
    h.idle_frame();
    assert!(h.editor.ui.find.open);
    h.press(Key::ArrowRight, Modifiers::NONE);
    h.press(Key::ArrowDown, Modifiers::NONE);
    h.press(Key::Enter, Modifiers::NONE);
    h.idle_frame();
    assert!(
        h.editor.ui.story_view,
        "Story View must stay open while Find is open"
    );
    assert_eq!(h.editor.selection.slide, "s1");
    h.editor.ui.find.open = false;

    // 2. Course Settings modal open.
    h.editor.ui.show_course_settings_window = true;
    h.idle_frame();
    h.press(Key::ArrowRight, Modifiers::NONE);
    h.press(Key::Enter, Modifiers::NONE);
    h.idle_frame();
    assert!(h.editor.ui.story_view);
    assert_eq!(h.editor.selection.slide, "s1");
    h.editor.ui.show_course_settings_window = false;

    // 3. Preview window open.
    h.editor.open_preview();
    h.idle_frame();
    assert!(h.editor.preview.is_some());
    h.press(Key::ArrowRight, Modifiers::NONE);
    h.press(Key::Enter, Modifiers::NONE);
    h.idle_frame();
    assert!(h.editor.ui.story_view);
    assert_eq!(h.editor.selection.slide, "s1");
}

#[test]
fn d10_4_story_view_drag_within_across_and_onto_empty_scene_header() {
    let mut h = setup_harness(multi_scene_course());
    // Empty Scene 4.
    let empty_sc = h
        .editor
        .run("scene.add", serde_json::json!({ "title": "Empty Scene" }))
        .expect("scene.add")["id"]
        .as_str()
        .expect("sc id")
        .to_owned();
    h.editor.ui.story_view = true;
    h.idle_frame();
    h.idle_frame();

    let rect_of = |h: &TestHarness, id: &str| {
        h.editor
            .ui
            .course_map
            .thumb_rects
            .iter()
            .find(|(sid, _)| sid == id)
            .map(|(_, r)| *r)
            .expect("thumb rect")
    };
    let header_of = |h: &TestHarness, id: &str| {
        h.editor
            .ui
            .course_map
            .scene_header_rects
            .iter()
            .find(|(sid, _)| sid == id)
            .map(|(_, r)| *r)
            .expect("header rect")
    };

    // 1. Drag s2 within Scene 1 (past s3).
    let s2_r = rect_of(&h, "s2");
    let s3_r = rect_of(&h, "s3");
    h.drag_by(
        s2_r.center(),
        vec2(0.0, (s3_r.max.y - s2_r.center().y) + 8.0),
        6,
        Modifiers::NONE,
    );
    h.idle_frame();
    assert_eq!(h.editor.session.course.scenes[0].slides[2].id, "s2");

    // 2. Drag s2 into Scene 2 (onto s4).
    let s2_r = rect_of(&h, "s2");
    let s4_r = rect_of(&h, "s4");
    h.drag_by(
        s2_r.center(),
        s4_r.center() - s2_r.center() - vec2(0.0, 20.0),
        6,
        Modifiers::NONE,
    );
    h.idle_frame();
    assert!(
        h.editor.session.course.scenes[1]
            .slides
            .iter()
            .any(|s| s.id == "s2")
    );

    // 3. Drag s2 onto Empty Scene 4's header.
    let s2_r = rect_of(&h, "s2");
    let empty_hdr = header_of(&h, &empty_sc);
    h.drag_by(
        s2_r.center(),
        empty_hdr.center() - s2_r.center(),
        6,
        Modifiers::NONE,
    );
    h.idle_frame();
    assert_eq!(
        h.editor
            .session
            .course
            .scenes
            .iter()
            .find(|sc| sc.id == empty_sc)
            .unwrap()
            .slides
            .len(),
        1
    );

    // Undo x 3 restores s2 to Scene 1 index 1.
    for _ in 0..3 {
        h.editor
            .run("edit.undo", serde_json::json!({}))
            .expect("undo");
    }
    assert_eq!(h.editor.session.course.scenes[0].slides[1].id, "s2");
}

#[test]
fn d10_5_inline_scene_rename_in_story_view_and_navigator() {
    let mut h = setup_harness(multi_scene_course());
    h.editor.ui.story_view = true;
    h.idle_frame();

    let header = h
        .editor
        .ui
        .course_map
        .scene_header_rects
        .first()
        .map(|(_, r)| *r)
        .expect("scene header");

    // Empty ignored.
    h.double_click_at(header.center());
    h.idle_frame();
    assert!(h.editor.ui.course_map.rename.is_some());
    let edit_rect = h
        .editor
        .ui
        .course_map
        .rename_edit_rect
        .expect("rename edit rect");
    h.set_field(edit_rect.center(), "   ");
    h.idle_frame();
    assert_eq!(h.editor.session.course.scenes[0].title, "Scene 1");

    // Esc cancels.
    h.advance_time(0.6);
    h.double_click_at(header.center());
    h.idle_frame();
    let edit_rect = h
        .editor
        .ui
        .course_map
        .rename_edit_rect
        .expect("rename edit rect");
    h.click_at(edit_rect.center());
    h.press(Key::A, Modifiers::COMMAND);
    h.type_text("Temp");
    h.press(Key::Escape, Modifiers::NONE);
    h.idle_frame();
    assert_eq!(h.editor.session.course.scenes[0].title, "Scene 1");

    // "Module 1" + Enter commits.
    h.advance_time(0.6);
    h.double_click_at(header.center());
    h.idle_frame();
    let edit_rect = h
        .editor
        .ui
        .course_map
        .rename_edit_rect
        .expect("rename edit rect");
    h.set_field(edit_rect.center(), "Module 1");
    h.idle_frame();
    assert_eq!(h.editor.session.course.scenes[0].title, "Module 1");

    // Also verify Navigator scene_rename commits.
    h.editor.ui.story_view = false;
    h.editor.ui.scene_rename = Some(crate::slides::SceneRename {
        scene: "scene-1".into(),
        buffer: "Unit 1".into(),
        original: "Module 1".into(),
        just_entered: true,
    });
    h.idle_frame();
    h.press(Key::Enter, Modifiers::NONE);
    h.idle_frame();
    assert_eq!(h.editor.session.course.scenes[0].title, "Unit 1");
}

#[test]
fn d10_6_scene_delete_last_scene_refused_and_cross_scene_trigger_cleanup_bug_d7() {
    // (a) Deleting only scene refused.
    let single: Course = serde_json::from_value(serde_json::json!({
        "schema_version": 1,
        "id": "single-scene",
        "title": "Single",
        "slide_size": { "w": 1280, "h": 720 },
        "scenes": [{
            "id": "sc1",
            "title": "Only",
            "slides": [{ "id": "s1", "title": "S1", "duration_ms": 5000 }]
        }]
    }))
    .expect("single");
    let mut h_single = setup_harness(single);
    assert!(
        h_single
            .editor
            .run("scene.delete", serde_json::json!({ "scene": "sc1" }))
            .is_none()
    );

    // (b) Scene 1 has JumpToSlide(s4_in_scene2) and LightboxSlide(s4_in_scene2) -> delete Scene 2 via confirm dialog.
    let mut h = setup_harness(multi_scene_course());
    for action in [
        Action::JumpToSlide {
            target: SlideTarget::Id("s4".into()),
        },
        Action::LightboxSlide { slide: "s4".into() },
    ] {
        h.editor
            .run(
                "trigger.add",
                serde_json::json!({
                    "slide": "s1",
                    "trigger": Trigger {
                        when: CourseEvent::TimelineStarts { layer: None },
                        condition: None,
                        action,
                        disabled: false,
                    }
                }),
            )
            .expect("trigger.add");
    }
    assert_eq!(
        h.editor.session.course.slide("s1").unwrap().triggers.len(),
        2
    );

    // Open scene delete confirm dialog and confirm (last match is the button).
    h.editor.ui.pending_delete_scene = Some("scene-2".into());
    h.idle_frame();
    let del_btn = h
        .find_text_last("Delete scene")
        .expect("Delete scene button");
    h.click_at(del_btn);
    h.idle_frame();

    assert!(
        h.editor
            .session
            .course
            .scenes
            .iter()
            .all(|sc| sc.id != "scene-2"),
        "BUG-D7: Scene 2 should be deleted even with cross-scene triggers"
    );
    assert!(
        h.editor
            .session
            .course
            .slide("s1")
            .unwrap()
            .triggers
            .is_empty(),
        "BUG-D7: cross-scene JumpToSlide/LightboxSlide triggers must be cleaned up"
    );
    assert!(
        h.editor
            .session
            .course
            .validate()
            .into_iter()
            .all(|i| i.severity != coursecraft_course::Severity::Error)
    );

    // Undo restores Scene 2 AND the triggers on s1.
    h.editor
        .run("edit.undo", serde_json::json!({}))
        .expect("undo");
    assert!(
        h.editor
            .session
            .course
            .scenes
            .iter()
            .any(|sc| sc.id == "scene-2")
    );
    assert_eq!(
        h.editor.session.course.slide("s1").unwrap().triggers.len(),
        2
    );
}

#[test]
fn d10_7_story_view_edge_arrows_more_chip_and_unreachable_badge() {
    let mut h = setup_harness(multi_scene_course());
    // Add extra slides so s1 can have 15 distinct outgoing edges (triggering +N more).
    let mut targets = vec![
        "s1".to_owned(), // self-loop
        "s2".to_owned(),
        "s3".to_owned(),
        "s4".to_owned(),
        "s5".to_owned(),
        "s6".to_owned(),
        "s7".to_owned(),
        "s8".to_owned(),
    ];
    for idx in 0..8 {
        let sid = h
            .editor
            .run(
                "slide.add",
                serde_json::json!({ "scene": "scene-1", "title": format!("Extra {idx}") }),
            )
            .expect("slide.add")["id"]
            .as_str()
            .expect("sid")
            .to_owned();
        targets.push(sid);
    }
    let _ = h.editor.run(
        "variable.add",
        serde_json::json!({ "name": "x", "value": 1.0 }),
    );
    // Add 15 distinct outgoing triggers from s1 (jump, lightbox, link, next, conditional, self-loop).
    for (i, tgt) in targets.iter().take(15).enumerate() {
        let action = match i {
            1 => Action::LightboxSlide { slide: tgt.clone() },
            2 => Action::OpenUrl {
                url: "https://example.com".into(),
            },
            3 => Action::JumpToSlide {
                target: SlideTarget::Next,
            },
            _ => Action::JumpToSlide {
                target: SlideTarget::Id(tgt.clone()),
            },
        };
        let condition = if i == 14 {
            Some(coursecraft_course::Condition::Compare {
                variable: "x".into(),
                op: coursecraft_course::CmpOp::Eq,
                value: coursecraft_course::ValueSource::Constant(
                    coursecraft_course::Value::Number(1.0),
                ),
            })
        } else {
            None
        };
        h.editor
            .run(
                "trigger.add",
                serde_json::json!({
                    "slide": "s1",
                    "trigger": Trigger {
                        when: CourseEvent::TimelineStarts { layer: None },
                        condition,
                        action,
                        disabled: false,
                    }
                }),
            )
            .expect("trigger.add");
    }

    h.editor.ui.story_view = true;
    h.idle_frame();
    h.idle_frame();

    assert!(
        h.find_text_containing("more").is_some(),
        "+N more chip should render when >12 outgoing arrows"
    );
    let report = h.layout_report(2);
    assert!(
        report.findings.is_empty(),
        "Story View layout_report clean: {report:?}"
    );
}

#[test]
fn d10_8_navigator_drag_reorder_and_multi_select() {
    let mut h = setup_harness(multi_scene_course());
    // Shift+Click range select and Cmd+Click toggle.
    h.editor.selection.slide = "s1".into();
    h.editor
        .selection
        .select_slide_range(&h.editor.session.course, "s3");
    assert_eq!(h.editor.selection.also_slides, vec!["s2", "s3"]);
    h.editor.selection.toggle_slide("s2");
    assert_eq!(h.editor.selection.also_slides, vec!["s3"]);

    // Slide move within and between scenes (matching Story View slide.move).
    h.editor
        .run(
            "slide.move",
            serde_json::json!({ "slide": "s3", "scene": "scene-1", "index": 0 }),
        )
        .expect("move within scene");
    assert_eq!(h.editor.session.course.scenes[0].slides[0].id, "s3");

    h.editor
        .run(
            "slide.move",
            serde_json::json!({ "slide": "s1", "scene": "scene-2", "index": 0 }),
        )
        .expect("move between scenes");
    assert_eq!(h.editor.session.course.scenes[1].slides[0].id, "s1");

    h.editor
        .run("edit.undo", serde_json::json!({}))
        .expect("undo 1");
    h.editor
        .run("edit.undo", serde_json::json!({}))
        .expect("undo 2");
    assert_eq!(h.editor.session.course.scenes[0].slides[0].id, "s1");
    assert_eq!(h.editor.session.course.scenes[0].slides[2].id, "s3");
}

#[test]
fn d10_9_empty_scene_and_deleting_only_non_empty_scene_refused() {
    let course: Course = serde_json::from_value(serde_json::json!({
        "schema_version": 1,
        "id": "empty-scene-test",
        "title": "Empty Scene Test",
        "slide_size": { "w": 1280, "h": 720 },
        "scenes": [
            {
                "id": "sc1",
                "title": "Scene 1",
                "slides": [{ "id": "s1", "title": "Slide 1", "duration_ms": 5000 }]
            },
            {
                "id": "sc2",
                "title": "Empty Scene 2",
                "slides": []
            }
        ]
    }))
    .expect("course");
    let mut h = setup_harness(course);

    // Deleting sc1 (the only non-empty scene) would leave 0 total slides -> refused!
    assert!(
        h.editor
            .run("scene.delete", serde_json::json!({ "scene": "sc1" }))
            .is_none()
    );
    // Deleting s1 (the only slide in the course) -> refused!
    assert!(
        h.editor
            .run("slide.delete", serde_json::json!({ "slide": "s1" }))
            .is_none()
    );
    assert_eq!(h.editor.session.course.slides().count(), 1);
}

#[test]
fn d10_10_large_story_view_50_slides_10_scenes_stress() {
    let mut scenes = Vec::new();
    for sc in 0..10 {
        let mut slides = Vec::new();
        for sl in 0..5 {
            slides.push(serde_json::json!({
                "id": format!("s-{sc}-{sl}"),
                "title": format!("Slide {sc}.{sl}"),
                "duration_ms": 5000,
                "objects": []
            }));
        }
        scenes.push(serde_json::json!({
            "id": format!("sc-{sc}"),
            "title": format!("Scene {sc}"),
            "slides": slides
        }));
    }
    let course: Course = serde_json::from_value(serde_json::json!({
        "schema_version": 1,
        "id": "stress-50",
        "title": "Stress 50",
        "slide_size": { "w": 1280, "h": 720 },
        "scenes": scenes
    }))
    .expect("stress course");

    let mut h = TestHarness::new(course);
    h.editor.ui.show_welcome = false;
    h.editor.selection.slide = "s-0-0".into();
    h.editor.ui.story_view = true;
    h.idle_frame();
    h.idle_frame();

    // Scroll, move a slide from Scene 9 to Scene 0, press arrows, check layout_report(2).
    let central = h.central_panel_rect();
    h.scroll_over(central.center(), -200.0);
    h.idle_frame();

    h.editor
        .run(
            "slide.move",
            serde_json::json!({ "slide": "s-9-4", "scene": "sc-0", "index": 0 }),
        )
        .expect("move s-9-4 to sc-0");
    h.idle_frame();
    assert_eq!(h.editor.session.course.scenes[0].slides[0].id, "s-9-4");

    h.press(Key::ArrowRight, Modifiers::NONE);
    h.press(Key::ArrowLeft, Modifiers::NONE);
    h.idle_frame();

    let report = h.layout_report(2);
    assert!(
        report.findings.iter().all(|f| !f.contains("Scene ")),
        "50-slide Story View must have zero clipped scene headers: {:?}",
        report.findings
    );
}
