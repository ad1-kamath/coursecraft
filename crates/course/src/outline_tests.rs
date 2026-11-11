use crate::{
    CourseOutline, MAX_BODY_LINES, MAX_CHOICES, MAX_LINE_BYTES, MAX_OUTLINE_SLIDES, OutlineChoice,
    OutlineQuestion, OutlineScene, OutlineSlide, outline_from_markdown,
};

fn ok(text: &str) -> CourseOutline {
    match outline_from_markdown(text) {
        Ok(o) => o,
        Err(p) => panic!("expected ok, got {p:#?}"),
    }
}

fn problems(text: &str) -> Vec<(usize, String)> {
    match outline_from_markdown(text) {
        Ok(o) => panic!("expected problems, got {o:#?}"),
        Err(p) => p.into_iter().map(|p| (p.line, p.message)).collect(),
    }
}

#[test]
fn every_line_type_lands_in_its_place() {
    let o = ok(
        "# Safety\n## Welcome\n- First point\n* Second point\nPlain line\n> Say hello\n> and smile\n## Quiz\n? Which is safest?\n[x] Lock out\n[ ] Skip it\n",
    );
    assert_eq!(o.title.as_deref(), Some("Safety"));
    assert_eq!(o.scenes.len(), 1);
    let scene = &o.scenes[0];
    assert_eq!(scene.title, "Safety");
    assert_eq!(scene.slides.len(), 2);
    let first = &scene.slides[0];
    assert_eq!(first.title, "Welcome");
    assert_eq!(
        first.body,
        vec!["First point", "Second point", "Plain line"]
    );
    assert_eq!(first.notes.as_deref(), Some("Say hello\nand smile"));
    assert!(first.question.is_none());
    let quiz = &scene.slides[1];
    let q = quiz.question.as_ref().expect("question");
    assert_eq!(q.prompt, "Which is safest?");
    assert_eq!(q.choices.len(), 2);
    assert!(q.choices[0].correct);
    assert!(!q.choices[1].correct);
    assert_eq!(q.choices[0].text, "Lock out");
}

#[test]
fn slides_before_a_scene_get_a_default_scene() {
    let o = ok("## One\n## Two\n");
    assert_eq!(o.title, None);
    assert_eq!(o.scenes.len(), 1);
    assert_eq!(o.scenes[0].title, "Scene 1");
    assert_eq!(o.slides().count(), 2);
}

#[test]
fn several_scenes_keep_order() {
    let o = ok("# A\n## a1\n# B\n## b1\n## b2\n");
    let titles: Vec<&str> = o.scenes.iter().map(|s| s.title.as_str()).collect();
    assert_eq!(titles, vec!["A", "B"]);
    assert_eq!(o.scenes[1].slides.len(), 2);
}

#[test]
fn first_scene_names_the_document() {
    let o = ok("# One\n## a\n# Two\n## b\n");
    assert_eq!(o.title.as_deref(), Some("One"));
    assert_eq!(o.scenes.len(), 2);
}

#[test]
fn slides_before_a_scene_take_the_course_title() {
    let o = ok("# Intro\n## One\n");
    assert_eq!(o.scenes.len(), 1);
    assert_eq!(o.scenes[0].title, "Intro");
    assert_eq!(o.scenes[0].slides.len(), 1);
}

#[test]
fn crlf_and_crlf_blank_lines_parse_like_lf() {
    let o = ok("# S\r\n## Slide\r\n- body\r\n\r\n> note\r\n? Q\r\n[x] yes\r\n[ ] no\r\n");
    let slide = &o.scenes[0].slides[0];
    assert_eq!(slide.title, "Slide");
    assert_eq!(slide.body, vec!["body"]);
    assert_eq!(slide.notes.as_deref(), Some("note"));
    assert!(slide.question.is_some());
}

#[test]
fn non_ascii_text_is_preserved() {
    let o = ok("# Sécurité 安全\n## 介绍 🎉\n- café naïve\n? ¿Cuál es correcto?\n[x] Sí\n[ ] No\n");
    assert_eq!(o.scenes[0].title, "Sécurité 安全");
    assert_eq!(o.scenes[0].slides[0].title, "介绍 🎉");
    assert_eq!(o.scenes[0].slides[0].body, vec!["café naïve"]);
}

#[test]
fn blank_lines_are_ignored() {
    let o = ok("\n\n# S\n\n## Slide\n\n\n- body\n\n");
    assert_eq!(o.scenes[0].slides[0].body, vec!["body"]);
}

#[test]
fn body_before_any_slide_is_a_problem() {
    let p = problems("- stray\n");
    assert!(
        p.iter().any(|(l, m)| *l == 1 && m.contains("slide")),
        "{p:#?}"
    );
}

#[test]
fn notes_before_any_slide_is_a_problem() {
    let p = problems("# S\n> stray\n");
    assert_eq!(p.len(), 1);
    assert_eq!(p[0].0, 2);
}

#[test]
fn choice_without_a_question_is_a_problem() {
    let p = problems("# S\n## Slide\n[x] yes\n");
    assert_eq!(p[0].0, 3);
    assert!(p[0].1.contains("question"), "{}", p[0].1);
}

#[test]
fn second_question_on_a_slide_is_a_problem() {
    let p = problems("## Slide\n? one\n[x] a\n[ ] b\n? two\n[x] c\n[ ] d\n");
    assert!(
        p.iter().any(|(l, m)| *l == 5 && m.contains("only one")),
        "{p:#?}"
    );
}

#[test]
fn body_inside_a_question_is_a_problem() {
    let p = problems("## Slide\n? Q\n- oops\n[x] a\n[ ] b\n");
    assert_eq!(p[0].0, 3);
}

#[test]
fn question_needs_two_choices() {
    let p = problems("## Slide\n? Q\n[x] a\n");
    assert!(p.iter().any(|(_, m)| m.contains("at least 2")), "{p:#?}");
}

#[test]
fn question_needs_exactly_one_correct_choice() {
    let none = problems("## Slide\n? Q\n[ ] a\n[ ] b\n");
    assert!(
        none.iter().any(|(_, m)| m.contains("one correct")),
        "{none:#?}"
    );
    let two = problems("## Slide\n? Q\n[x] a\n[x] b\n");
    assert!(
        two.iter().any(|(_, m)| m.contains("one correct")),
        "{two:#?}"
    );
}

#[test]
fn empty_choice_text_is_a_problem() {
    let p = problems("## Slide\n? Q\n[x] \n[ ] b\n");
    assert!(p.iter().any(|(l, _)| *l == 3), "{p:#?}");
}

#[test]
fn empty_titles_are_problems() {
    assert!(!problems("#  \n## S\n").is_empty());
    assert!(!problems("# S\n##   \n").is_empty());
}

#[test]
fn empty_document_is_a_problem() {
    let p = problems("");
    assert!(!p.is_empty());
    assert_eq!(p[0].0, 0);
    assert!(!problems("   \n\n").is_empty());
}

#[test]
fn too_many_slides_is_a_problem() {
    let mut text = String::from("# S\n");
    for i in 0..=MAX_OUTLINE_SLIDES {
        text.push_str(&format!("## Slide {i}\n"));
    }
    let p = problems(&text);
    assert!(p.iter().any(|(_, m)| m.contains("limit is 500")), "{p:#?}");
    // Exactly the limit still passes.
    let mut text = String::from("# S\n");
    for i in 0..MAX_OUTLINE_SLIDES {
        text.push_str(&format!("## Slide {i}\n"));
    }
    assert_eq!(ok(&text).slides().count(), MAX_OUTLINE_SLIDES);
}

#[test]
fn too_many_body_lines_is_a_problem() {
    let mut text = String::from("# S\n## Slide\n");
    for i in 0..=MAX_BODY_LINES {
        text.push_str(&format!("- line {i}\n"));
    }
    let p = problems(&text);
    assert!(p.iter().any(|(_, m)| m.contains("body lines")), "{p:#?}");
    let over_line = p
        .iter()
        .find(|(_, m)| m.contains("body lines"))
        .map(|(l, _)| *l)
        .unwrap_or(0);
    assert_eq!(over_line, 3 + MAX_BODY_LINES);

    let mut text = String::from("# S\n## Slide\n");
    for i in 0..MAX_BODY_LINES {
        text.push_str(&format!("- line {i}\n"));
    }
    assert_eq!(ok(&text).slides().next().map(|s| s.body.len()), Some(50));
}

#[test]
fn too_many_choices_is_a_problem() {
    let mut text = String::from("# S\n## Slide\n? Q\n");
    for i in 0..=MAX_CHOICES {
        let mark = if i == 0 { "[x] " } else { "[ ] " };
        text.push_str(&format!("{mark}choice {i}\n"));
    }
    let p = problems(&text);
    assert!(p.iter().any(|(_, m)| m.contains("choices")), "{p:#?}");
}

#[test]
fn overlong_line_is_a_problem() {
    let long = "a".repeat(MAX_LINE_BYTES + 1);
    let text = format!("# S\n## Slide\n- {long}\n");
    let p = problems(&text);
    assert_eq!(p[0].0, 3);
    assert!(p[0].1.contains("4000"), "{}", p[0].1);

    let exact = "b".repeat(MAX_LINE_BYTES - 2);
    assert!(outline_from_markdown(&format!("# S\n## Slide\n- {exact}\n")).is_ok());
    let just_over = "b".repeat(MAX_LINE_BYTES);
    let p = problems(&format!("# S\n## Slide\n- {just_over}\n"));
    assert_eq!(p[0].0, 3);
}

#[test]
fn hostile_input_never_panics() {
    let long_line = "a".repeat(20_000);
    let deep_hashes = "#".repeat(20_000);
    let deep_headings = "## ".repeat(5_000);
    let samples = [
        "",
        "\0",
        "##",
        "#",
        "?",
        "[x]",
        "[ ] ",
        "### deep",
        "#### deeper",
        ">",
        "- ",
        "\u{feff}# bom\n## s\n",
        "\r",
        "\r\n\r\n",
        "## a\n[x] b\n? c\nd\n> e\n",
        "🎉🎉🎉",
        "## s\n- a\n? q\n[x] a\n[ ] b\n- after\n",
        long_line.as_str(),
        deep_hashes.as_str(),
        deep_headings.as_str(),
    ];
    for s in samples {
        let _ = outline_from_markdown(s);
    }
    // Deterministic pseudo-random fuzz over the interesting characters.
    let alphabet = [
        "#", "## ", "- ", "* ", "> ", "? ", "[x] ", "[ ] ", "a", " ", "\n", "\r\n", "é", "🎉",
    ];
    let mut seed = 0x1234_5678_u32;
    for _ in 0..300 {
        let mut text = String::new();
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let len = (seed >> 16) % 40;
        for _ in 0..len {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let i = (seed as usize) % alphabet.len();
            if let Some(part) = alphabet.get(i) {
                text.push_str(part);
            }
        }
        let _ = outline_from_markdown(&text);
    }
}

#[test]
fn check_catches_hostile_json_outlines() {
    let slide = |title: &str| OutlineSlide {
        title: title.to_owned(),
        ..OutlineSlide::default()
    };
    let base = CourseOutline {
        title: None,
        scenes: vec![OutlineScene {
            title: "S".into(),
            slides: vec![slide("ok")],
        }],
    };
    assert!(base.check().is_empty(), "{:#?}", base.check());

    let mut no_scenes = base.clone();
    no_scenes.scenes.clear();
    assert!(!no_scenes.check().is_empty());

    let mut no_scene_title = base.clone();
    no_scene_title.scenes[0].title.clear();
    assert!(!no_scene_title.check().is_empty());

    let mut no_slide_title = base.clone();
    no_slide_title.scenes[0].slides[0].title.clear();
    assert!(!no_slide_title.check().is_empty());

    let mut long_outline_title = base.clone();
    long_outline_title.title = Some("x".repeat(MAX_LINE_BYTES + 1));
    assert!(!long_outline_title.check().is_empty());

    let mut too_long = base.clone();
    too_long.scenes[0].slides[0].body = vec!["x".repeat(MAX_LINE_BYTES + 1)];
    assert!(!too_long.check().is_empty());

    let mut too_many_body = base.clone();
    too_many_body.scenes[0].slides[0].body = vec!["x".into(); MAX_BODY_LINES + 1];
    assert!(!too_many_body.check().is_empty());

    let mut long_notes = base.clone();
    long_notes.scenes[0].slides[0].notes = Some("x".repeat(MAX_LINE_BYTES + 1));
    assert!(!long_notes.check().is_empty());

    let mut bad_question = base.clone();
    bad_question.scenes[0].slides[0].question = Some(OutlineQuestion {
        prompt: String::new(),
        choices: vec![
            OutlineChoice {
                text: "a".into(),
                correct: false,
            },
            OutlineChoice {
                text: "b".into(),
                correct: false,
            },
        ],
    });
    let p = bad_question.check();
    assert!(p.iter().any(|p| p.message.contains("no prompt")), "{p:#?}");
    assert!(
        p.iter().any(|p| p.message.contains("one correct")),
        "{p:#?}"
    );

    let mut many_choices = base.clone();
    many_choices.scenes[0].slides[0].question = Some(OutlineQuestion {
        prompt: "Q".into(),
        choices: (0..=MAX_CHOICES)
            .map(|i| OutlineChoice {
                text: format!("c{i}"),
                correct: i == 0,
            })
            .collect(),
    });
    assert!(!many_choices.check().is_empty());

    let mut many_slides = base.clone();
    many_slides.scenes[0].slides = (0..=MAX_OUTLINE_SLIDES).map(|_| slide("s")).collect();
    assert!(!many_slides.check().is_empty());

    // A JSON round trip of a hostile document is checked the same way.
    let json = serde_json::to_string(&many_slides).unwrap_or_default();
    let parsed: CourseOutline = serde_json::from_str(&json).unwrap_or_default();
    assert!(!parsed.check().is_empty());
}

#[test]
fn outline_round_trips_through_json() {
    let o = ok("# S\n## Slide\n- body\n> note\n? Q\n[x] a\n[ ] b\n");
    let json = serde_json::to_string(&o).unwrap_or_default();
    let back: CourseOutline = serde_json::from_str(&json).unwrap_or_default();
    assert_eq!(back, o);
    assert_eq!(back.question_count(), 1);
}

#[test]
fn problems_are_sorted_by_line() {
    let p = problems("- stray\n## Slide\n? Q\n[x] a\n[ ] b\n- oops\n");
    let lines: Vec<usize> = p.iter().map(|(l, _)| *l).collect();
    let mut sorted = lines.clone();
    sorted.sort_unstable();
    assert_eq!(lines, sorted);
    assert!(!lines.is_empty());
}
