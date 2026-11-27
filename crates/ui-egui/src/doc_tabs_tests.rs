//! Document tabs under the ribbon: the Course Map, pinned slide tabs, and a
//! single italic preview tab.

use crate::doc_tabs;
use crate::test_harness::TestHarness;
use coursecraft_course::Course;
use egui::{Key, Modifiers, Pos2, Rect, pos2};

const HELLO_COURSE: &str = include_str!("../../../examples/hello-course/course.json");

fn with_ribbon() -> TestHarness {
    let course = Course::from_json(HELLO_COURSE).expect("hello course parses");
    let mut h = TestHarness::new_with_size(course, (1600.0, 900.0));
    h.editor.ui.ribbon.visible = true;
    for _ in 0..3 {
        h.idle_frame();
    }
    h
}

fn with_many_slides(count: usize, size: (f32, f32)) -> TestHarness {
    let mut course = Course::from_json(HELLO_COURSE).expect("hello course parses");
    let template = course.scenes[0].slides[0].clone();
    let current = course.scenes[0].slides.len();
    for i in current..count {
        let mut s = template.clone();
        s.id = format!("s-extra-{i:03}");
        s.title = format!("Slide {:02}", i + 1);
        course.scenes[0].slides.push(s);
    }
    let mut h = TestHarness::new_with_size(course, size);
    h.editor.ui.ribbon.visible = true;
    for _ in 0..3 {
        h.idle_frame();
    }
    h
}

fn raw_idle_input(h: &TestHarness) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(h.ctx.input(|i| i.content_rect())),
        time: Some(h.ctx.input(|i| i.time) + 1.0 / 60.0),
        ..Default::default()
    }
}

fn slide_ids(h: &TestHarness) -> Vec<String> {
    crate::selection::slide_layout(&h.editor.session.course)
        .into_iter()
        .flat_map(|(_, slides)| slides)
        .collect()
}

fn tab_is_italic(h: &mut TestHarness, label: &str) -> bool {
    fn walk(shape: &egui::epaint::Shape, label: &str) -> Option<bool> {
        match shape {
            egui::epaint::Shape::Text(t) if t.galley.text() == label => {
                Some(t.galley.job.sections.iter().any(|sec| sec.format.italics))
            }
            egui::epaint::Shape::Vec(v) => v.iter().find_map(|s| walk(s, label)),
            _ => None,
        }
    }
    let raw_input = raw_idle_input(h);
    let output = h.ctx.run_ui(raw_input, |ui| {
        let _ = h.editor.show(ui);
    });
    let italic = output
        .shapes
        .iter()
        .find_map(|cs| walk(&cs.shape, label))
        .unwrap_or(false);
    output.drop_without_applying_deltas();
    italic
}

fn tab_button_rect(h: &mut TestHarness, label: &str) -> Option<Rect> {
    fn walk_text(shape: &egui::epaint::Shape, label: &str) -> Option<Pos2> {
        match shape {
            egui::epaint::Shape::Text(t) if t.galley.text() == label => {
                Some(t.pos + t.galley.rect.center().to_vec2())
            }
            egui::epaint::Shape::Vec(v) => v.iter().find_map(|s| walk_text(s, label)),
            _ => None,
        }
    }
    fn walk_rect(shape: &egui::epaint::Shape, pt: Pos2) -> Option<Rect> {
        match shape {
            egui::epaint::Shape::Rect(r) if r.rect.contains(pt) && r.rect.height() <= 32.0 => {
                Some(r.rect)
            }
            egui::epaint::Shape::Vec(v) => v.iter().find_map(|s| walk_rect(s, pt)),
            _ => None,
        }
    }
    let raw_input = raw_idle_input(h);
    let output = h.ctx.run_ui(raw_input, |ui| {
        let _ = h.editor.show(ui);
    });
    let pt = output
        .shapes
        .iter()
        .find_map(|cs| walk_text(&cs.shape, label));
    let rect = pt.and_then(|p| output.shapes.iter().find_map(|cs| walk_rect(&cs.shape, p)));
    output.drop_without_applying_deltas();
    rect
}

#[test]
fn the_open_slide_gets_a_numbered_tab() {
    let mut h = with_ribbon();
    let first = slide_ids(&h)[0].clone();
    assert!(h.editor.ui.doc_tabs.open.is_empty());
    assert_eq!(
        h.editor.ui.doc_tabs.preview.as_deref(),
        Some(first.as_str())
    );
    let label = doc_tabs::slide_tab_label(&h.editor.session.course, &first);
    assert!(label.starts_with("1.1 "), "{label}");
    assert!(h.find_text(&label).is_some(), "preview tab drawn");
    assert!(tab_is_italic(&mut h, &label), "preview tab is italic");
    assert!(h.find_text("Course Map").is_some(), "map tab drawn");
}

#[test]
fn tabs_switch_between_the_map_and_slides() {
    let mut h = with_ribbon();
    let ids = slide_ids(&h);
    doc_tabs::pin(&mut h.editor, &ids[0]);
    doc_tabs::open_slide(&mut h.editor, &ids[1]);
    doc_tabs::pin(&mut h.editor, &ids[1]);
    h.idle_frame();

    let pos = h.find_text("Course Map").expect("map tab");
    h.click_at(pos);
    h.idle_frame();
    assert!(h.editor.ui.story_view, "map shown");

    assert_eq!(h.editor.ui.doc_tabs.open, [ids[0].clone(), ids[1].clone()]);
    let label = doc_tabs::slide_tab_label(&h.editor.session.course, &ids[0]);
    let pos = h.find_text(&label).expect("first slide tab");
    h.click_at(pos);
    h.idle_frame();
    assert!(!h.editor.ui.story_view);
    assert_eq!(h.editor.selection.slide, ids[0]);
}

#[test]
fn closing_the_shown_tab_shows_its_neighbour_then_the_map() {
    let mut h = with_ribbon();
    let ids = slide_ids(&h);
    doc_tabs::pin(&mut h.editor, &ids[0]);
    doc_tabs::open_slide(&mut h.editor, &ids[1]);
    doc_tabs::pin(&mut h.editor, &ids[1]);
    h.idle_frame();

    doc_tabs::close(&mut h.editor, &ids[1]);
    h.idle_frame();
    assert_eq!(h.editor.selection.slide, ids[0], "neighbour shown");
    doc_tabs::close(&mut h.editor, &ids[0]);
    h.idle_frame();
    assert!(h.editor.ui.story_view, "the map when no tab is left");
    assert!(h.editor.ui.doc_tabs.open.is_empty());
}

#[test]
fn a_deleted_slide_loses_its_tab() {
    let mut h = with_ribbon();
    let ids = slide_ids(&h);
    doc_tabs::open_slide(&mut h.editor, &ids[1]);
    doc_tabs::pin(&mut h.editor, &ids[1]);
    h.idle_frame();
    h.editor
        .run("slide.delete", serde_json::json!({ "slide": ids[1] }));
    h.idle_frame();
    assert!(!h.editor.ui.doc_tabs.open.contains(&ids[1]));
}

#[test]
fn no_tab_row_without_the_ribbon() {
    let course = Course::from_json(HELLO_COURSE).expect("hello course parses");
    let mut h = TestHarness::new_with_size(course, (1600.0, 900.0));
    h.idle_frame();
    assert!(h.editor.ui.doc_tabs.open.is_empty());
}

#[test]
fn m48_s2_01_clicking_ten_slides_in_list_leaves_one_preview_tab() {
    let mut h = with_many_slides(12, (1600.0, 1200.0));
    let ids = slide_ids(&h);
    for (idx, sid) in ids.iter().enumerate().take(10) {
        let title = h
            .editor
            .session
            .course
            .slide(sid)
            .expect("slide")
            .title
            .clone();
        if let Some(pos) = h.find_text(&title) {
            h.click_at(pos);
            h.idle_frame();
        } else {
            // Fallback if list scrolled past viewport in test window.
            doc_tabs::open_slide(&mut h.editor, sid);
            h.idle_frame();
        }
        assert!(
            h.editor.ui.doc_tabs.open.is_empty(),
            "clicking slide {idx} must not pin"
        );
    }
    let last_id = &ids[9];
    assert_eq!(
        h.editor.ui.doc_tabs.preview.as_deref(),
        Some(last_id.as_str())
    );
    let tab_label = doc_tabs::slide_tab_label(&h.editor.session.course, last_id);
    assert!(
        tab_is_italic(&mut h, &tab_label),
        "preview tab must be italic"
    );
}

#[test]
fn m48_s2_02_arrow_keys_through_ten_slides_leave_one_preview_tab() {
    let mut h = with_many_slides(12, (1600.0, 900.0));
    let ids = slide_ids(&h);
    let first_title = h
        .editor
        .session
        .course
        .slide(&ids[0])
        .expect("s0")
        .title
        .clone();
    let pos = h.find_text(&first_title).expect("first slide in list");
    h.click_at(pos);
    h.idle_frame();

    for _ in 0..9 {
        h.press(Key::ArrowDown, Modifiers::NONE);
        h.idle_frame();
    }
    assert_eq!(h.editor.selection.slide, ids[9]);
    assert!(h.editor.ui.doc_tabs.open.is_empty());
    assert_eq!(
        h.editor.ui.doc_tabs.preview.as_deref(),
        Some(ids[9].as_str())
    );

    h.press(Key::ArrowUp, Modifiers::NONE);
    h.idle_frame();
    assert_eq!(h.editor.selection.slide, ids[8]);
    assert!(h.editor.ui.doc_tabs.open.is_empty());
    assert_eq!(
        h.editor.ui.doc_tabs.preview.as_deref(),
        Some(ids[8].as_str())
    );
}

#[test]
fn m48_s2_03_double_click_pins_from_list_map_and_tab() {
    let mut h = with_many_slides(6, (1600.0, 900.0));
    let ids = slide_ids(&h);

    // 1. Double-click thumbnail in slide list pins slide 1.
    let s1_title = h
        .editor
        .session
        .course
        .slide(&ids[1])
        .expect("s1")
        .title
        .clone();
    let title_pos = h.find_text(&s1_title).expect("slide 1 title in list");
    h.double_click_at(pos2(title_pos.x, title_pos.y - 30.0));
    h.idle_frame();
    assert!(
        h.editor.ui.doc_tabs.open.contains(&ids[1]),
        "list double-click pins"
    );
    assert_ne!(
        h.editor.ui.doc_tabs.preview.as_deref(),
        Some(ids[1].as_str())
    );

    // 2. Double-click slide 2 in Course Map opens and pins it.
    h.advance_time(1.0);
    doc_tabs::open_map(&mut h.editor);
    h.idle_frame();
    let map_rect = h
        .editor
        .ui
        .course_map
        .thumb_rects
        .iter()
        .find(|(id, _)| id == &ids[2])
        .map(|(_, r)| *r)
        .expect("slide 2 in course map");
    h.double_click_at(map_rect.center());
    h.idle_frame();
    assert_eq!(
        h.editor.selection.slide, ids[2],
        "map double-click selects slide"
    );
    assert!(!h.editor.ui.story_view, "double-click in map opens slide");
    assert!(
        h.editor.ui.doc_tabs.open.contains(&ids[2]),
        "map double-click pins"
    );

    // 3. Single-click slide 3 -> preview tab; double-click the preview tab itself -> pins it.
    doc_tabs::open_slide(&mut h.editor, &ids[3]);
    h.idle_frame();
    h.advance_time(1.0);
    assert_eq!(
        h.editor.ui.doc_tabs.preview.as_deref(),
        Some(ids[3].as_str())
    );
    let tab_label = doc_tabs::slide_tab_label(&h.editor.session.course, &ids[3]);
    let tab_pos = h.find_text(&tab_label).expect("preview tab");
    h.double_click_at(tab_pos);
    h.idle_frame();
    assert!(
        h.editor.ui.doc_tabs.open.contains(&ids[3]),
        "tab double-click pins"
    );
    assert_eq!(h.editor.ui.doc_tabs.preview, None);
}

#[test]
fn m48_s2_04_edit_pins_but_undo_redo_and_new_slide_do_not() {
    let mut h = with_many_slides(5, (1600.0, 900.0));
    let ids = slide_ids(&h);

    // Editing previewed slide 1 pins it.
    doc_tabs::open_slide(&mut h.editor, &ids[1]);
    h.idle_frame();
    assert_eq!(
        h.editor.ui.doc_tabs.preview.as_deref(),
        Some(ids[1].as_str())
    );
    h.editor
        .run(
            "object.add",
            serde_json::json!({
                "slide": ids[1],
                "kind": { "type": "shape", "shape": "rectangle", "fill": "#3B82F6" },
                "frame": { "x": 100.0, "y": 100.0, "w": 200.0, "h": 100.0 }
            }),
        )
        .expect("object.add succeeds");
    h.idle_frame();
    assert!(h.editor.ui.doc_tabs.open.contains(&ids[1]), "edit pins");
    assert_eq!(h.editor.ui.doc_tabs.preview, None);

    // Close slide 1's tab, re-open it as preview, and run edit.undo / edit.redo: must NOT pin.
    doc_tabs::close(&mut h.editor, &ids[1]);
    doc_tabs::open_slide(&mut h.editor, &ids[1]);
    h.idle_frame();
    assert_eq!(
        h.editor.ui.doc_tabs.preview.as_deref(),
        Some(ids[1].as_str())
    );
    h.editor.run("edit.undo", serde_json::json!({}));
    h.idle_frame();
    assert!(
        !h.editor.ui.doc_tabs.open.contains(&ids[1]),
        "undo must not pin"
    );
    h.editor.run("edit.redo", serde_json::json!({}));
    h.idle_frame();
    assert!(
        !h.editor.ui.doc_tabs.open.contains(&ids[1]),
        "redo must not pin"
    );

    // slide.add and slide.duplicate show the new slide in preview without pinning.
    let res = h
        .editor
        .run("slide.add", serde_json::json!({}))
        .expect("slide.add");
    h.idle_frame();
    let added = res
        .get("id")
        .and_then(|v| v.as_str())
        .expect("new id")
        .to_owned();
    assert_eq!(h.editor.selection.slide, added);
    assert!(
        !h.editor.ui.doc_tabs.open.contains(&added),
        "slide.add must not pin"
    );
    assert_eq!(
        h.editor.ui.doc_tabs.preview.as_deref(),
        Some(added.as_str())
    );

    let dup = h
        .editor
        .run("slide.duplicate", serde_json::json!({ "slide": added }))
        .expect("slide.duplicate");
    h.idle_frame();
    let dup_id = dup
        .get("id")
        .and_then(|v| v.as_str())
        .expect("dup id")
        .to_owned();
    assert_eq!(h.editor.selection.slide, dup_id);
    assert!(
        !h.editor.ui.doc_tabs.open.contains(&dup_id),
        "slide.duplicate must not pin"
    );
    assert_eq!(
        h.editor.ui.doc_tabs.preview.as_deref(),
        Some(dup_id.as_str())
    );
}

#[test]
fn m48_s2_05_selecting_already_open_tab_does_not_create_preview() {
    let mut h = with_many_slides(4, (1600.0, 900.0));
    let ids = slide_ids(&h);
    doc_tabs::pin(&mut h.editor, &ids[0]);
    doc_tabs::pin(&mut h.editor, &ids[1]);
    h.editor.ui.doc_tabs.preview = None;
    h.idle_frame();

    doc_tabs::open_slide(&mut h.editor, &ids[0]);
    h.idle_frame();
    assert_eq!(h.editor.ui.doc_tabs.preview, None);

    // Preview slide 2, then click pinned slide 0: preview remains slide 2.
    doc_tabs::open_slide(&mut h.editor, &ids[2]);
    h.idle_frame();
    assert_eq!(
        h.editor.ui.doc_tabs.preview.as_deref(),
        Some(ids[2].as_str())
    );

    let s0_label = doc_tabs::slide_tab_label(&h.editor.session.course, &ids[0]);
    let pos = h.find_text(&s0_label).expect("pinned tab");
    h.click_at(pos);
    h.idle_frame();
    assert_eq!(h.editor.selection.slide, ids[0]);
    assert_eq!(
        h.editor.ui.doc_tabs.preview.as_deref(),
        Some(ids[2].as_str())
    );
}

#[test]
fn m48_s2_06_closing_and_deleting_preview_tab() {
    let mut h = with_many_slides(4, (1600.0, 900.0));
    let ids = slide_ids(&h);

    // With a pinned tab, closing the preview tab switches to the last pinned tab.
    doc_tabs::pin(&mut h.editor, &ids[0]);
    doc_tabs::open_slide(&mut h.editor, &ids[1]);
    h.idle_frame();
    doc_tabs::close(&mut h.editor, &ids[1]);
    h.idle_frame();
    assert_eq!(h.editor.ui.doc_tabs.preview, None);
    assert_eq!(h.editor.selection.slide, ids[0]);

    // With no pinned tabs, closing the preview tab switches to Course Map.
    doc_tabs::close(&mut h.editor, &ids[0]);
    doc_tabs::open_slide(&mut h.editor, &ids[2]);
    h.idle_frame();
    doc_tabs::close(&mut h.editor, &ids[2]);
    h.idle_frame();
    assert_eq!(h.editor.ui.doc_tabs.preview, None);
    assert!(h.editor.ui.story_view);

    // Deleting the previewed slide removes its preview tab.
    doc_tabs::open_slide(&mut h.editor, &ids[3]);
    h.idle_frame();
    assert_eq!(
        h.editor.ui.doc_tabs.preview.as_deref(),
        Some(ids[3].as_str())
    );
    h.editor
        .run("slide.delete", serde_json::json!({ "slide": ids[3] }));
    h.idle_frame();
    assert_ne!(
        h.editor.ui.doc_tabs.preview.as_deref(),
        Some(ids[3].as_str())
    );
}

#[test]
fn m48_s2_07_overflow_tabs_scroll_and_keep_readable_width() {
    let mut h = with_many_slides(16, (900.0, 700.0));
    let ids = slide_ids(&h);
    for sid in ids.iter().take(15) {
        doc_tabs::pin(&mut h.editor, sid);
    }
    for _ in 0..3 {
        h.idle_frame();
    }

    // Overflow scroll buttons appear and tabs stay at least 48 px wide.
    assert!(h.find_text("‹").is_some(), "left scroll button");
    let right_btn = h.find_text("›").expect("right scroll button");
    let first_label = doc_tabs::slide_tab_label(&h.editor.session.course, &ids[0]);
    let first_rect = tab_button_rect(&mut h, &first_label).expect("first tab rect");
    assert!(
        first_rect.width() >= 48.0,
        "tab width must stay readable, got {}",
        first_rect.width()
    );

    // Selecting the 15th (off-screen) pinned tab scrolls it into view.
    doc_tabs::open_slide(&mut h.editor, &ids[14]);
    for _ in 0..30 {
        h.idle_frame();
    }
    let last_label = doc_tabs::slide_tab_label(&h.editor.session.course, &ids[14]);
    let (last_pos, last_clip) = h.find_text_clipped(&last_label).expect("15th tab painted");
    assert!(
        last_clip.contains(last_pos),
        "active off-screen tab scrolled into view: pos={last_pos:?} clip={last_clip:?}"
    );

    // Clicking ‹ scrolls back left.
    let left_btn = h.find_text("‹").expect("left scroll button");
    for _ in 0..12 {
        h.click_at(left_btn);
        h.idle_frame();
    }
    for _ in 0..20 {
        h.idle_frame();
    }
    let _ = right_btn;
    let (first_pos, first_clip) = h
        .find_text_clipped(&first_label)
        .expect("first tab painted");
    assert!(
        first_clip.contains(first_pos),
        "clicking ‹ scrolls first tab back into view: pos={first_pos:?} clip={first_clip:?}"
    );
}

#[test]
fn m48_s2_08_300_slide_course_tab_sync_uses_cache() {
    let mut h = with_many_slides(300, (1600.0, 900.0));
    let ids = slide_ids(&h);
    assert_eq!(ids.len(), 300);
    let cached_rev = h.editor.ui.doc_tabs.cached_revision;
    assert!(cached_rev.is_some());

    for sid in ids.iter().step_by(15) {
        doc_tabs::open_slide(&mut h.editor, sid);
        h.idle_frame();
        assert_eq!(h.editor.ui.doc_tabs.preview.as_deref(), Some(sid.as_str()));
        assert!(h.editor.ui.doc_tabs.open.is_empty());
        assert_eq!(
            h.editor.ui.doc_tabs.cached_revision, cached_rev,
            "cache must not rebuild on slide selection"
        );
    }
}
