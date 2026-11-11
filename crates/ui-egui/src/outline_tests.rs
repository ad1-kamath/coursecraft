//! Engine → player round-trip for `course.import_outline`.
//!
//! This lives in the UI crate's tests because `coursecraft-engine` does not
//! depend on `coursecraft-logic` and the workspace rule is layering downward
//! only. The UI crate already depends on both, so the grading check runs here.

use crate::{Editor, test_harness::TestHarness};
use coursecraft_course::{Course, CourseOutline, outline_from_markdown};
use coursecraft_engine::Session;
use coursecraft_logic::{Effect, Input, Player};
use egui::{Key, Modifiers};
use serde_json::json;

const OUTLINE: &str = "# Fire Safety\n\
## Welcome\n\
- Keep exits clear\n\
- Know the alarm sound\n\
> Point at the nearest exit.\n\
## Quiz\n\
? What do you do when the alarm sounds?\n\
[ ] Finish your coffee\n\
[x] Leave by the nearest exit\n\
[ ] Wait for an email\n\
# Wrap Up\n\
## Thanks\n\
## Questions\n";

fn session_with_outline() -> (Session, String) {
    let outline = match outline_from_markdown(OUTLINE) {
        Ok(o) => o,
        Err(p) => panic!("outline should parse: {p:?}"),
    };
    let mut s = Session::blank("Blank");
    let result = match s.execute(
        "course.import_outline",
        &json!({ "outline": outline, "after": "slide1" }),
    ) {
        Ok(v) => v,
        Err(e) => panic!("import failed: {e}"),
    };
    let first = result["first_slide"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    (s, first)
}

#[test]
fn imported_question_grades_through_the_player() {
    let (s, _first) = session_with_outline();
    let course: Course = s.course.clone();
    let quiz = course
        .slides()
        .find(|sl| sl.question.is_some())
        .cloned()
        .unwrap_or_else(|| panic!("no question slide in the import"));

    // Play just the quiz slide by making it the start slide.
    let mut alone = course.clone();
    alone.start_slide = Some(quiz.id.clone());
    let mut player = match Player::new(&alone) {
        Ok(p) => p,
        Err(issues) => panic!("player refused the imported course: {issues:#?}"),
    };
    player.handle(&Input::Start);

    let choices: Vec<(String, bool)> = quiz
        .question
        .as_ref()
        .map(|q| {
            q.choices
                .iter()
                .map(|c| (c.id.clone(), c.correct))
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(choices.len(), 3, "outline had three choices");
    assert_eq!(choices.iter().filter(|(_, ok)| *ok).count(), 1);
    let (wrong, _) = choices
        .iter()
        .find(|(_, ok)| !*ok)
        .cloned()
        .unwrap_or_default();
    let (right, _) = choices
        .iter()
        .find(|(_, ok)| *ok)
        .cloned()
        .unwrap_or_default();

    let submit = quiz
        .objects
        .iter()
        .find(|o| o.name == "Submit")
        .map(|o| o.id.clone())
        .unwrap_or_else(|| panic!("imported question keeps its Submit button"));

    // A wrong answer is graded wrong.
    player.handle(&Input::SelectChoice {
        choice: wrong.clone(),
    });
    let effects = player.handle(&Input::Click {
        object: submit.clone(),
    });
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::QuestionAnswered { correct: false, .. })),
        "wrong choice should grade wrong: {effects:#?}"
    );

    // The imported correct choice grades correct.
    let mut alone = course;
    alone.start_slide = Some(quiz.id.clone());
    let mut player = match Player::new(&alone) {
        Ok(p) => p,
        Err(issues) => panic!("player refused the imported course: {issues:#?}"),
    };
    player.handle(&Input::Start);
    player.handle(&Input::SelectChoice { choice: right });
    let effects = player.handle(&Input::Click { object: submit });
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::QuestionAnswered { correct: true, .. })),
        "correct choice should grade correct: {effects:#?}"
    );
}

fn example() -> &'static str {
    include_str!("../../../examples/hello-course/course.json")
}

fn example_harness() -> TestHarness {
    TestHarness::new(Course::from_json(example()).expect("example course"))
}

#[test]
fn insert_menu_opens_the_outline_dialog() {
    let mut h = example_harness();
    h.idle_frame();
    assert!(!h.editor.outline_dialog_open_for_test());

    let insert = h.find_text("Insert").expect("Insert menu");
    h.click_at(insert);
    h.idle_frame();
    let item = h
        .find_text_last("Slides from Outline…")
        .expect("Slides from Outline menu item");
    h.click_at(item);
    h.idle_frame();

    assert!(h.editor.outline_dialog_open_for_test());
}

#[test]
fn typing_an_outline_shows_the_live_summary() {
    let mut h = example_harness();
    h.idle_frame();
    h.editor.open_outline_dialog();
    h.idle_frame();
    h.type_text(OUTLINE);
    h.idle_frame();

    assert_eq!(h.editor.ui.outline_text, OUTLINE);
    assert!(
        h.find_text_containing("2 scenes, 4 slides, 1 question")
            .is_some(),
        "the live summary is not shown"
    );
}

#[test]
fn a_problem_shows_its_line_and_disables_add_slides() {
    let mut h = example_harness();
    h.idle_frame();
    h.editor.open_outline_dialog();
    h.editor
        .set_outline_text_for_test("## Quiz\n? Pick one\n[x] Only choice\n");
    h.idle_frame();

    assert!(!h.editor.outline_dialog_can_add(), "a problem disables Add");
    assert!(
        h.find_text_containing("at least 2").is_some(),
        "the problem is not shown"
    );
    let before = h.editor.undo_len_for_test();
    let add = h.find_text("Add slides").expect("Add slides button");
    h.click_at(add);
    h.idle_frame();
    assert_eq!(h.editor.undo_len_for_test(), before, "nothing was imported");
    assert!(h.editor.outline_dialog_open_for_test());
}

#[test]
fn add_slides_button_imports_in_one_undo_step_and_closes() {
    let mut h = example_harness();
    h.idle_frame();
    let before = h.editor.undo_len_for_test();
    h.editor.open_outline_dialog();
    h.editor.set_outline_text_for_test(OUTLINE);
    h.idle_frame();
    assert!(
        h.editor.outline_dialog_can_add(),
        "valid outline enables Add"
    );

    let add = h.find_text("Add slides").expect("Add slides button");
    h.click_at(add);
    h.idle_frame();

    assert!(
        !h.editor.outline_dialog_open_for_test(),
        "the window closes"
    );
    assert_eq!(h.editor.undo_len_for_test(), before + 1, "one undo step");
    let selected = h.editor.selected_slide_for_test();
    assert_eq!(
        Some(selected.as_str()),
        h.editor.outline_last_first_slide_for_test().as_deref(),
        "selection moves to the first imported slide"
    );
    assert!(
        h.find_text_containing("Added 4 slides in 2 scenes")
            .is_some(),
        "the status line reports the import"
    );
}

#[test]
fn escape_closes_without_importing() {
    let mut h = example_harness();
    h.idle_frame();
    h.editor.open_outline_dialog();
    h.editor.set_outline_text_for_test(OUTLINE);
    h.idle_frame();
    let before = h.editor.undo_len_for_test();
    let course = h.editor.session.course.clone();

    h.press(Key::Escape, Modifiers::NONE);

    assert!(!h.editor.outline_dialog_open_for_test());
    assert_eq!(h.editor.undo_len_for_test(), before);
    assert_eq!(h.editor.session.course, course);
}

#[test]
fn typing_in_the_outline_does_not_nudge_the_selection() {
    let mut h = example_harness();
    h.editor.selection.slide = "s-intro".into();
    h.editor.selection.object = Some("btn-start".into());
    h.idle_frame();
    let before = h
        .editor
        .session
        .course
        .play_slide("s-intro")
        .and_then(|slide| slide.object("btn-start"))
        .map(|(object, _)| object.frame.x)
        .expect("btn-start frame");

    h.editor.open_outline_dialog();
    h.idle_frame();
    h.type_text("a");
    h.press(Key::ArrowRight, Modifiers::NONE);
    h.idle_frame();

    let after = h
        .editor
        .session
        .course
        .play_slide("s-intro")
        .and_then(|slide| slide.object("btn-start"))
        .map(|(object, _)| object.frame.x)
        .expect("btn-start frame");
    assert_eq!(before, after, "arrow keys in the field must not nudge");
    assert_eq!(h.editor.ui.outline_text, "a");
}

#[test]
fn outline_from_json_checks_before_the_command_runs() {
    // A hostile JSON outline is refused without touching the course.
    let mut s = Session::blank("T");
    let before = s.course.clone();
    let bad: CourseOutline = serde_json::from_value(json!({
        "scenes": [{ "title": "S", "slides": [{ "title": "Q", "question": {
            "prompt": "Pick", "choices": [{ "text": "a", "correct": true }]
        }}]}]
    }))
    .unwrap_or_default();
    let err = s.execute("course.import_outline", &json!({ "outline": bad }));
    assert!(err.is_err());
    assert_eq!(s.course, before);
}

#[test]
fn editor_helper_types_compile() {
    // Keep the imports above honest when the helpers change shape.
    let mut ed = Editor::new(Session::blank("T").course);
    ed.open_outline_dialog();
    assert!(ed.outline_dialog_open_for_test());
}
