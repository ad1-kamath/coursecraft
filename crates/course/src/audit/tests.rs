use super::*;
use crate::{
    Action, Choice, Question, QuestionKind, Scoring, ShapeKind, TextRun, TextStyle, Timing, Trigger,
};

const BASE: &str = r#"{
  "schema_version": 1,
  "id": "course",
  "title": "Course",
  "scenes": [{
    "id": "scene",
    "title": "Scene",
    "slides": [{ "id": "slide", "title": "Slide" }]
  }]
}"#;

fn empty_course() -> Course {
    Course::from_json(BASE).expect("base course parses")
}

fn slide(course: &mut Course) -> &mut Slide {
    &mut course.scenes[0].slides[0]
}

fn findings(course: &Course) -> Vec<AuditFinding> {
    course.audit().findings
}

fn has(course: &Course, check: &str) -> bool {
    findings(course).iter().any(|f| f.check == check)
}

fn object(id: &str, kind: ObjectKind, (x, y, w, h): (f32, f32, f32, f32)) -> Object {
    Object {
        id: id.to_owned(),
        name: String::new(),
        kind,
        frame: Rect { x, y, w, h },
        timing: Timing::default(),
        states: Vec::new(),
        initial_state: "normal".to_owned(),
        entrance: None,
        exit: None,
        motion_paths: Vec::new(),
        focus_order: None,
        decorative: false,
        description: None,
        group: None,
        panel: None,
        rotation: 0.0,
        flip_h: false,
        flip_v: false,
        opacity: 1.0,
        shadow: None,
        glow: None,
        drag: None,
    }
}

fn text(id: &str, content: &str, size: f32, color: &str) -> Object {
    object(
        id,
        ObjectKind::Text {
            text: content.to_owned(),
            style: TextStyle {
                size,
                color: Color(color.to_owned()),
                ..TextStyle::default()
            },
            runs: Vec::new(),
            paragraphs: Vec::new(),
            autofit: crate::Autofit::None,
            text_style: None,
        },
        (0.0, 0.0, 400.0, 100.0),
    )
}

fn button(id: &str, label: &str, fill: &str) -> Object {
    object(
        id,
        ObjectKind::Button {
            label: label.to_owned(),
            fill: Color(fill.to_owned()),
            fill_style: None,
        },
        (0.0, 0.0, 200.0, 56.0),
    )
}

fn image(id: &str, alt: &str) -> Object {
    object(
        id,
        ObjectKind::Image {
            src: "image.png".to_owned(),
            alt: alt.to_owned(),
            crop: None,
        },
        (600.0, 0.0, 200.0, 200.0),
    )
}

fn shape(id: &str, fill: &str, frame: (f32, f32, f32, f32)) -> Object {
    object(
        id,
        ObjectKind::Shape {
            shape: ShapeKind::Rectangle,
            fill: Color(fill.to_owned()),
            fill_style: None,
            outline: None,
            text: None,
            adjust: Vec::new(),
        },
        frame,
    )
}

fn shape_with_text(id: &str, fill: &str, color: &str, content: &str) -> Object {
    object(
        id,
        ObjectKind::Shape {
            shape: ShapeKind::Rectangle,
            fill: Color(fill.to_owned()),
            fill_style: None,
            outline: None,
            adjust: Vec::new(),
            text: Some(crate::ShapeText {
                text: content.to_owned(),
                style: TextStyle {
                    size: 20.0,
                    color: Color(color.to_owned()),
                    ..TextStyle::default()
                },
                runs: Vec::new(),
                paragraphs: Vec::new(),
                autofit: crate::Autofit::None,
            }),
        },
        (0.0, 0.0, 400.0, 100.0),
    )
}

fn video(id: &str, autoplay: bool, captions: Option<&str>) -> Object {
    object(
        id,
        ObjectKind::Video {
            src: "video.mp4".to_owned(),
            autoplay,
            captions: captions.map(str::to_owned),
            volume: 1.0,
            trim: None,
            cues: Vec::new(),
        },
        (0.0, 0.0, 400.0, 300.0),
    )
}

fn audio(id: &str, autoplay: bool, captions: Option<&str>) -> Object {
    object(
        id,
        ObjectKind::Audio {
            src: "audio.wav".to_owned(),
            autoplay,
            captions: captions.map(str::to_owned),
            volume: 1.0,
            trim: None,
        },
        (0.0, 0.0, 400.0, 40.0),
    )
}

fn question(kind: QuestionKind, prompt: &str, choices: Vec<Choice>) -> Question {
    Question {
        kind,
        prompt: prompt.to_owned(),
        choices,
        points: 10,
        scoring: Scoring::default(),
        attempts: 1,
        shuffle: false,
        correct_layer: None,
        incorrect_layer: None,
        accepted: Vec::new(),
        case_sensitive: false,
        pairs: Vec::new(),
        hotspot_image: None,
        regions: Vec::new(),
        markers: 1,
        drag_items: Vec::new(),
        drop_targets: Vec::new(),
        snap_to_center: true,
        return_if_wrong: false,
        one_per_target: true,
        correct_when: None,
    }
}

fn click_trigger(id: &str) -> Trigger {
    Trigger {
        action: Action::PauseTimeline,
        when: Event::Click {
            object: id.to_owned(),
        },
        condition: None,
        disabled: false,
    }
}

// ------------------------------------------------------------ course title

#[test]
fn empty_course_title_is_an_error() {
    let mut course = empty_course();
    course.title = "   ".to_owned();
    let findings = findings(&course);
    let finding = findings
        .iter()
        .find(|f| f.check == "course-title")
        .expect("course-title finding");
    assert_eq!(finding.severity, AuditSeverity::Error);
    assert_eq!(finding.criterion, "WCAG 2.4.2");
    assert!(finding.slide.is_none());
    assert!(finding.object.is_none());
}

#[test]
fn a_named_course_has_no_course_title_finding() {
    assert!(!has(&empty_course(), "course-title"));
}

// ------------------------------------------------------------- slide title

#[test]
fn empty_slide_title_is_an_error() {
    let mut course = empty_course();
    slide(&mut course).title = String::new();
    let findings = findings(&course);
    let finding = findings
        .iter()
        .find(|f| f.check == "slide-title")
        .expect("slide-title finding");
    assert_eq!(finding.severity, AuditSeverity::Error);
    assert_eq!(finding.slide.as_deref(), Some("slide"));
}

#[test]
fn a_titled_slide_has_no_slide_title_finding() {
    assert!(!has(&empty_course(), "slide-title"));
}

// ---------------------------------------------------------------- alt text

#[test]
fn an_image_without_alt_text_or_description_is_an_error() {
    let mut course = empty_course();
    slide(&mut course).objects.push(image("img", "  "));
    let findings = findings(&course);
    let finding = findings
        .iter()
        .find(|f| f.check == "alt-text")
        .expect("alt-text finding");
    assert_eq!(finding.severity, AuditSeverity::Error);
    assert_eq!(finding.criterion, "WCAG 1.1.1");
    assert_eq!(finding.object.as_deref(), Some("img"));
}

#[test]
fn alt_text_or_description_satisfies_alt_text() {
    let mut course = empty_course();
    slide(&mut course).objects.push(image("img", "A cat"));
    assert!(!has(&course, "alt-text"));

    let mut course = empty_course();
    let mut picture = image("img", "");
    picture.description = Some("A cat asleep".to_owned());
    slide(&mut course).objects.push(picture);
    assert!(!has(&course, "alt-text"));
}

#[test]
fn decorative_images_skip_alt_text() {
    let mut course = empty_course();
    let mut picture = image("img", "");
    picture.decorative = true;
    slide(&mut course).objects.push(picture);
    assert!(!has(&course, "alt-text"));
}

// ----------------------------------------------------------- control names

#[test]
fn a_button_without_a_label_is_an_error() {
    let mut course = empty_course();
    slide(&mut course)
        .objects
        .push(button("btn", "", "#2563EB"));
    let findings = findings(&course);
    let finding = findings
        .iter()
        .find(|f| f.check == "control-name")
        .expect("control-name finding");
    assert_eq!(finding.severity, AuditSeverity::Error);
    assert_eq!(finding.criterion, "WCAG 4.1.2");
}

#[test]
fn a_labelled_button_is_named() {
    let mut course = empty_course();
    slide(&mut course)
        .objects
        .push(button("btn", "Continue", "#2563EB"));
    assert!(!has(&course, "control-name"));
}

#[test]
fn a_pointer_target_without_a_description_is_an_error() {
    let mut course = empty_course();
    slide(&mut course)
        .objects
        .push(shape("hot", "#2563EB", (0.0, 0.0, 200.0, 56.0)));
    slide(&mut course).triggers.push(click_trigger("hot"));
    assert!(has(&course, "control-name"));
}

#[test]
fn a_pointer_target_with_a_description_is_named() {
    let mut course = empty_course();
    let mut target = shape("hot", "#2563EB", (0.0, 0.0, 200.0, 56.0));
    target.description = Some("Open the menu".to_owned());
    slide(&mut course).objects.push(target);
    slide(&mut course).triggers.push(click_trigger("hot"));
    assert!(!has(&course, "control-name"));
}

#[test]
fn question_targets_count_as_controls() {
    let mut course = empty_course();
    slide(&mut course)
        .objects
        .push(shape("drop", "#E0E7FF", (600.0, 0.0, 320.0, 96.0)));
    let mut q = question(QuestionKind::DragDrop, "Drop it", Vec::new());
    q.drop_targets.push("drop".to_owned());
    slide(&mut course).question = Some(q);
    assert!(has(&course, "control-name"));
}

// ---------------------------------------------------------------- contrast

#[test]
fn contrast_threshold_treats_the_exact_minimum_as_passing() {
    assert!(contrast_passes(4.5, false));
    assert!(!contrast_passes(4.4998, false));
    assert!(contrast_passes(3.0, true));
    assert!(!contrast_passes(2.9998, true));
}

#[test]
fn body_text_just_below_4_5_fails_and_just_above_passes() {
    let mut course = empty_course();
    slide(&mut course)
        .objects
        .push(text("t", "Readable", 20.0, "#767676"));
    assert!(!has(&course, "contrast"));

    let mut course = empty_course();
    slide(&mut course)
        .objects
        .push(text("t", "Faint", 20.0, "#777777"));
    let findings = findings(&course);
    let finding = findings
        .iter()
        .find(|f| f.check == "contrast")
        .expect("contrast finding");
    assert_eq!(finding.severity, AuditSeverity::Error);
    assert_eq!(finding.criterion, "WCAG 1.4.3");
}

#[test]
fn large_text_uses_the_3_to_1_threshold() {
    let mut passes = empty_course();
    slide(&mut passes)
        .objects
        .push(text("t", "Large", 24.0, "#949494"));
    assert!(!has(&passes, "contrast"));

    let mut fails = empty_course();
    slide(&mut fails)
        .objects
        .push(text("t", "Small", 23.0, "#949494"));
    assert!(has(&fails, "contrast"));
}

#[test]
fn bold_text_becomes_large_at_18_66_px() {
    let mut passes = empty_course();
    let mut object = text("t", "Bold large", 19.0, "#949494");
    if let ObjectKind::Text { style, .. } = &mut object.kind {
        style.bold = true;
    }
    slide(&mut passes).objects.push(object);
    assert!(!has(&passes, "contrast"));

    let mut fails = empty_course();
    let mut object = text("t", "Bold small", 18.0, "#949494");
    if let ObjectKind::Text { style, .. } = &mut object.kind {
        style.bold = true;
    }
    slide(&mut fails).objects.push(object);
    assert!(has(&fails, "contrast"));
}

#[test]
fn translucent_shapes_are_composited_over_the_slide_background() {
    let mut course = empty_course();
    slide(&mut course)
        .objects
        .push(shape("back", "#00000080", (0.0, 0.0, 400.0, 100.0)));
    slide(&mut course)
        .objects
        .push(text("t", "White", 20.0, "#FFFFFF"));
    assert!(has(&course, "contrast"));

    let mut opaque = empty_course();
    slide(&mut opaque)
        .objects
        .push(shape("back", "#000000FF", (0.0, 0.0, 400.0, 100.0)));
    slide(&mut opaque)
        .objects
        .push(text("t", "White", 20.0, "#FFFFFF"));
    assert!(!has(&opaque, "contrast"));
}

#[test]
fn a_transparent_shape_changes_nothing() {
    let mut course = empty_course();
    slide(&mut course)
        .objects
        .push(shape("back", "#00000000", (0.0, 0.0, 400.0, 100.0)));
    slide(&mut course)
        .objects
        .push(text("t", "White", 20.0, "#FFFFFF"));
    assert!(has(&course, "contrast"));
}

#[test]
fn a_fully_covered_text_over_an_image_is_unknown_not_a_guess() {
    let mut course = empty_course();
    slide(&mut course).objects.push(object(
        "img",
        ObjectKind::Image {
            src: "photo.png".to_owned(),
            alt: "A photo".to_owned(),
            crop: None,
        },
        (0.0, 0.0, 400.0, 100.0),
    ));
    slide(&mut course)
        .objects
        .push(shape("back", "#000000", (0.0, 0.0, 400.0, 100.0)));
    slide(&mut course)
        .objects
        .push(text("t", "White", 20.0, "#FFFFFF"));
    let findings = findings(&course);
    assert!(findings.iter().any(|f| f.check == "contrast-unknown"));
    assert!(!findings.iter().any(|f| f.check == "contrast"));
}

#[test]
fn a_partly_covering_shape_makes_contrast_unknown() {
    let mut course = empty_course();
    slide(&mut course)
        .objects
        .push(shape("back", "#000000", (0.0, 0.0, 200.0, 100.0)));
    slide(&mut course)
        .objects
        .push(text("t", "White", 20.0, "#FFFFFF"));
    let findings = findings(&course);
    let finding = findings
        .iter()
        .find(|f| f.check == "contrast-unknown")
        .expect("contrast-unknown finding");
    assert_eq!(finding.severity, AuditSeverity::Note);
    assert!(!findings.iter().any(|f| f.check == "contrast"));
}

#[test]
fn runs_are_checked_separately_with_their_own_colours() {
    let mut course = empty_course();
    let mut object = text("t", "ReadableFaint", 20.0, "#1F1F1F");
    object.kind = ObjectKind::Text {
        text: "ReadableFaint".to_owned(),
        style: TextStyle {
            size: 20.0,
            color: Color("#1F1F1F".to_owned()),
            ..TextStyle::default()
        },
        runs: vec![
            TextRun {
                text: "Readable".to_owned(),
                bold: None,
                italic: None,
                underline: None,
                size: None,
                color: Some(Color("#1F1F1F".to_owned())),
                link: None,
                font: None,
                strikethrough: None,
                baseline: None,
                letter_spacing: None,
            },
            TextRun {
                text: "Faint".to_owned(),
                bold: None,
                italic: None,
                underline: None,
                size: None,
                color: Some(Color("#DDDDDD".to_owned())),
                link: None,
                font: None,
                strikethrough: None,
                baseline: None,
                letter_spacing: None,
            },
        ],
        paragraphs: Vec::new(),
        autofit: crate::Autofit::None,
        text_style: None,
    };
    slide(&mut course).objects.push(object);
    let findings = findings(&course);
    let contrast: Vec<_> = findings.iter().filter(|f| f.check == "contrast").collect();
    assert_eq!(contrast.len(), 1, "{findings:?}");
    assert!(contrast[0].message.contains("Faint"), "{contrast:?}");
}

#[test]
fn variable_references_count_as_text() {
    let mut course = empty_course();
    slide(&mut course)
        .objects
        .push(text("t", "%learner_name%", 20.0, "#DDDDDD"));
    assert!(has(&course, "contrast"));
}

#[test]
fn question_prompts_and_choices_use_theme_colours() {
    let mut course = empty_course();
    course.theme.text = Color("#FFFFFF".to_owned());
    slide(&mut course).question = Some(question(
        QuestionKind::MultipleChoice,
        "Pick one",
        vec![
            Choice {
                id: "a".to_owned(),
                text: "First".to_owned(),
                correct: true,
                points: 0,
            },
            Choice {
                id: "b".to_owned(),
                text: "Second".to_owned(),
                correct: false,
                points: 0,
            },
        ],
    ));
    let findings = findings(&course);
    let contrast: Vec<_> = findings.iter().filter(|f| f.check == "contrast").collect();
    assert_eq!(contrast.len(), 3, "{findings:?}");
    assert!(contrast.iter().all(|f| f.object.is_none()));
}

// --------------------------------------------------------------- text size

#[test]
fn text_under_16_px_warns() {
    let mut course = empty_course();
    slide(&mut course)
        .objects
        .push(text("t", "Tiny", 12.0, "#1F1F1F"));
    let findings = findings(&course);
    let finding = findings
        .iter()
        .find(|f| f.check == "text-size")
        .expect("text-size finding");
    assert_eq!(finding.severity, AuditSeverity::Warning);
    assert_eq!(finding.criterion, "Best practice");
}

#[test]
fn text_at_16_px_or_larger_is_fine() {
    let mut course = empty_course();
    slide(&mut course)
        .objects
        .push(text("t", "Fine", 16.0, "#1F1F1F"));
    assert!(!has(&course, "text-size"));
}

// ------------------------------------------------------------- target size

#[test]
fn interactive_objects_under_24_px_warn() {
    let mut course = empty_course();
    let mut small = button("btn", "Go", "#2563EB");
    small.frame = Rect {
        x: 0.0,
        y: 0.0,
        w: 23.0,
        h: 24.0,
    };
    slide(&mut course).objects.push(small);
    let findings = findings(&course);
    let finding = findings
        .iter()
        .find(|f| f.check == "target-size")
        .expect("target-size finding");
    assert_eq!(finding.severity, AuditSeverity::Warning);
    assert_eq!(finding.criterion, "WCAG 2.5.8 (WCAG 2.2)");
}

#[test]
fn interactive_objects_at_24_px_are_fine() {
    let mut course = empty_course();
    let mut size = button("btn", "Go", "#2563EB");
    size.frame = Rect {
        x: 0.0,
        y: 0.0,
        w: 24.0,
        h: 24.0,
    };
    slide(&mut course).objects.push(size);
    assert!(!has(&course, "target-size"));
}

#[test]
fn untouched_media_is_not_measured_for_target_size() {
    let mut course = empty_course();
    slide(&mut course)
        .objects
        .push(video("v", false, Some("v.vtt")));
    assert!(!has(&course, "target-size"));
}

// ---------------------------------------------------------------- captions

#[test]
fn a_video_without_captions_is_an_error() {
    let mut course = empty_course();
    slide(&mut course).objects.push(video("v", false, None));
    let findings = findings(&course);
    let finding = findings
        .iter()
        .find(|f| f.check == "video-captions")
        .expect("video-captions finding");
    assert_eq!(finding.severity, AuditSeverity::Error);
    assert_eq!(finding.criterion, "WCAG 1.2.2");
}

#[test]
fn a_video_with_captions_is_fine() {
    let mut course = empty_course();
    slide(&mut course)
        .objects
        .push(video("v", false, Some("v.vtt")));
    assert!(!has(&course, "video-captions"));
}

#[test]
fn audio_without_captions_warns() {
    let mut course = empty_course();
    slide(&mut course).objects.push(audio("a", false, None));
    let findings = findings(&course);
    let finding = findings
        .iter()
        .find(|f| f.check == "audio-captions")
        .expect("audio-captions finding");
    assert_eq!(finding.severity, AuditSeverity::Warning);
    assert_eq!(finding.criterion, "WCAG 1.2.1");
}

#[test]
fn audio_with_captions_is_fine() {
    let mut course = empty_course();
    slide(&mut course)
        .objects
        .push(audio("a", false, Some("a.vtt")));
    assert!(!has(&course, "audio-captions"));
}

#[test]
fn autoplay_without_a_pause_trigger_warns() {
    let mut course = empty_course();
    slide(&mut course)
        .objects
        .push(video("v", true, Some("v.vtt")));
    let findings = findings(&course);
    let finding = findings
        .iter()
        .find(|f| f.check == "autoplay-control")
        .expect("autoplay-control finding");
    assert_eq!(finding.severity, AuditSeverity::Warning);
    assert_eq!(finding.criterion, "WCAG 1.4.2");
}

#[test]
fn a_pause_trigger_satisfies_autoplay_control() {
    let mut course = empty_course();
    slide(&mut course)
        .objects
        .push(video("v", true, Some("v.vtt")));
    slide(&mut course).triggers.push(Trigger {
        action: Action::PauseMedia {
            object: "v".to_owned(),
        },
        when: Event::TimelineEnds { layer: None },
        condition: None,
        disabled: false,
    });
    assert!(!has(&course, "autoplay-control"));
}

#[test]
fn a_pause_trigger_on_a_layer_also_counts() {
    let mut course = empty_course();
    slide(&mut course)
        .objects
        .push(audio("a", true, Some("a.vtt")));
    let mut layer = crate::Layer {
        id: "layer".to_owned(),
        name: "Layer".to_owned(),
        duration_ms: 5000,
        objects: Vec::new(),
        triggers: vec![Trigger {
            action: Action::StopMedia {
                object: "a".to_owned(),
            },
            when: Event::TimelineEnds { layer: None },
            condition: None,
            disabled: false,
        }],
        hide_other_layers: true,
        pause_base_timeline: false,
        prevent_base_clicks: false,
        hide_base_objects: false,
        hide_when_timeline_ends: false,
        allow_seek: crate::SeekMode::Auto,
        revisit: crate::RevisitMode::Auto,
    };
    layer.duration_ms = 5000;
    slide(&mut course).layers.push(layer);
    assert!(!has(&course, "autoplay-control"));
}

// ------------------------------------------------------------- focus order

#[test]
fn duplicate_focus_order_warns() {
    let mut course = empty_course();
    let mut first = text("a", "One", 20.0, "#1F1F1F");
    first.focus_order = Some(2);
    let mut second = text("b", "Two", 20.0, "#1F1F1F");
    second.focus_order = Some(2);
    let objects = slide(&mut course);
    objects.objects.push(first);
    objects.objects.push(second);
    let findings = findings(&course);
    let finding = findings
        .iter()
        .find(|f| f.check == "focus-order")
        .expect("focus-order finding");
    assert_eq!(finding.severity, AuditSeverity::Warning);
    assert_eq!(finding.criterion, "WCAG 2.4.3");
    assert!(finding.message.contains("\"a\"") && finding.message.contains("\"b\""));
}

#[test]
fn distinct_focus_orders_are_fine() {
    let mut course = empty_course();
    let mut first = text("a", "One", 20.0, "#1F1F1F");
    first.focus_order = Some(1);
    let mut second = text("b", "Two", 20.0, "#1F1F1F");
    second.focus_order = Some(2);
    let objects = slide(&mut course);
    objects.objects.push(first);
    objects.objects.push(second);
    assert!(!has(&course, "focus-order"));
}

// -------------------------------------------------- decorative interactive

#[test]
fn a_decorative_button_is_an_error() {
    let mut course = empty_course();
    let mut object = button("btn", "Continue", "#2563EB");
    object.decorative = true;
    slide(&mut course).objects.push(object);
    let findings = findings(&course);
    let finding = findings
        .iter()
        .find(|f| f.check == "decorative-interactive")
        .expect("decorative-interactive finding");
    assert_eq!(finding.severity, AuditSeverity::Error);
    assert_eq!(finding.criterion, "WCAG 4.1.2");
    assert!(!findings.iter().any(|f| f.check == "control-name"));
}

#[test]
fn a_decorative_object_skips_the_other_checks() {
    let mut course = empty_course();
    let mut object = text("t", "Tiny faint", 10.0, "#EEEEEE");
    object.decorative = true;
    slide(&mut course).objects.push(object);
    let findings = findings(&course);
    assert!(!findings.iter().any(|f| f.check == "contrast"));
    assert!(!findings.iter().any(|f| f.check == "text-size"));
    assert!(!findings.iter().any(|f| f.check == "target-size"));
}

// ------------------------------------------------------------ hostile data

#[test]
fn hostile_frames_never_panic() {
    let frames = [
        (f32::NAN, 0.0, 100.0, 100.0),
        (0.0, f32::INFINITY, 100.0, 100.0),
        (0.0, 0.0, -100.0, 100.0),
        (f32::MAX, f32::MAX, f32::MAX, f32::MAX),
        (0.0, 0.0, f32::INFINITY, f32::INFINITY),
    ];
    for frame in frames {
        let mut course = empty_course();
        slide(&mut course)
            .objects
            .push(shape("back", "#000000", frame));
        let mut object = text("t", "Text", 20.0, "#1F1F1F");
        object.frame = Rect {
            x: 0.0,
            y: 0.0,
            w: 400.0,
            h: 100.0,
        };
        slide(&mut course).objects.push(object);
        let report = course.audit();
        assert_eq!(report.manual_checks.len(), 7);
    }
}

// ------------------------------------------------------------ honest words

fn crowded_course() -> Course {
    let mut course = empty_course();
    course.title = String::new();
    let objects = slide(&mut course);
    objects.title = String::new();
    objects.objects.push(image("img", ""));
    objects.objects.push(button("empty-button", "", "#2563EB"));
    objects
        .objects
        .push(text("small", "Tiny faint", 10.0, "#DDDDDD"));
    let mut small_button = button("small-button", "Go", "#2563EB");
    small_button.frame = Rect {
        x: 0.0,
        y: 0.0,
        w: 10.0,
        h: 10.0,
    };
    objects.objects.push(small_button);
    objects.objects.push(video("video", true, None));
    objects.objects.push(audio("audio", true, None));
    let mut first = text("first", "One", 20.0, "#1F1F1F");
    first.focus_order = Some(1);
    let mut second = text("second", "Two", 20.0, "#1F1F1F");
    second.focus_order = Some(1);
    objects.objects.push(first);
    objects.objects.push(second);
    let mut decorative = button("decorative", "Decorative", "#2563EB");
    decorative.decorative = true;
    objects.objects.push(decorative);
    objects.objects.push(object(
        "photo",
        ObjectKind::Image {
            src: "photo.png".to_owned(),
            alt: "A photo".to_owned(),
            crop: None,
        },
        (0.0, 150.0, 400.0, 100.0),
    ));
    let mut over_photo = text("over", "Over a photo", 20.0, "#FFFFFF");
    over_photo.frame = Rect {
        x: 0.0,
        y: 150.0,
        w: 400.0,
        h: 100.0,
    };
    objects.objects.push(over_photo);
    course
}

#[test]
fn every_row_of_the_table_can_fire() {
    let findings = findings(&crowded_course());
    let checks: std::collections::BTreeSet<&str> = findings.iter().map(|f| f.check).collect();
    for expected in [
        "course-title",
        "slide-title",
        "alt-text",
        "control-name",
        "contrast",
        "contrast-unknown",
        "text-size",
        "target-size",
        "video-captions",
        "audio-captions",
        "autoplay-control",
        "focus-order",
        "decorative-interactive",
    ] {
        assert!(checks.contains(expected), "missing {expected}: {checks:?}");
    }
}

#[test]
fn no_output_claims_a_course_is_accessible() {
    let report = crowded_course().audit();
    let mut texts: Vec<String> = report
        .findings
        .iter()
        .flat_map(|f| [f.message.clone(), f.fix.clone()])
        .collect();
    texts.extend(MANUAL_CHECKS.iter().map(|s| (*s).to_owned()));
    for text in texts {
        let lower = text.to_ascii_lowercase();
        assert!(!lower.contains("compliant"), "{text}");
        assert!(!lower.contains("passes wcag"), "{text}");
        assert!(!lower.contains("accessible course"), "{text}");
    }
}

#[test]
fn a_well_formed_slide_has_no_findings() {
    let mut course = empty_course();
    let objects = slide(&mut course);
    objects.objects.push(text("t", "Readable", 20.0, "#1F1F1F"));
    objects.objects.push(button("btn", "Continue", "#2563EB"));
    objects.objects.push(image("img", "A cat"));
    objects.objects.push(video("v", false, Some("v.vtt")));
    objects.objects.push(audio("a", false, Some("a.vtt")));
    assert_eq!(findings(&course), Vec::new());
}

#[test]
fn report_always_includes_the_manual_checks() {
    let report = empty_course().audit();
    assert_eq!(report.manual_checks, MANUAL_CHECKS);
    assert_eq!(report.manual_checks.len(), 7);
}

#[test]
fn audit_covers_master_objects_and_names_the_master() {
    let mut course = empty_course();
    let mut master = course.scenes[0].slides[0].clone();
    master.id = "m1".to_owned();
    master.title = "Brand master".to_owned();
    master.triggers.clear();
    master.layers.clear();
    master.question = None;
    master.objects = vec![image("master-img", "")];
    course.masters.push(master);

    let findings = findings(&course);
    let finding = findings
        .iter()
        .find(|finding| finding.object.as_deref() == Some("master-img"))
        .expect("master image is audited");
    assert_eq!(finding.slide.as_deref(), Some("m1"));
    assert_eq!(finding.check, "alt-text");
    assert!(
        finding.message.contains("Brand master"),
        "{:?}",
        finding.message
    );
}

#[test]
fn audit_contrast_uses_the_master_background() {
    let mut course = empty_course();
    let mut master = course.scenes[0].slides[0].clone();
    master.id = "m1".to_owned();
    master.title = "Dark master".to_owned();
    master.triggers.clear();
    master.layers.clear();
    master.question = None;
    master.background = Some(Color("#FFFFFF".to_owned()));
    let mut text = text("master-text", "White on white", 20.0, "#FFFFFF");
    text.frame = Rect {
        x: 0.0,
        y: 0.0,
        w: 200.0,
        h: 40.0,
    };
    master.objects = vec![text];
    course.masters.push(master);
    course.scenes[0].slides[0].master = Some("m1".to_owned());
    course.scenes[0].slides[0].objects.clear();

    let findings = findings(&course);
    assert!(
        findings.iter().any(|finding| finding.check == "contrast"
            && finding.object.as_deref() == Some("master-text")),
        "{findings:#?}"
    );
}

#[test]
fn shape_text_contrast_uses_the_shape_fill_or_the_slide_when_unfilled() {
    let mut course = empty_course();
    slide(&mut course).objects.push(shape_with_text(
        "on-fill",
        "#FFFFFF",
        "#FFFFFF",
        "White on white",
    ));
    let on_fill = findings(&course);
    assert!(
        on_fill
            .iter()
            .any(|finding| finding.check == "contrast"
                && finding.object.as_deref() == Some("on-fill")),
        "{on_fill:#?}"
    );

    let mut course = empty_course();
    slide(&mut course).objects.push(shape_with_text(
        "no-fill-dark",
        "#00000000",
        "#1F1F1F",
        "Dark on the slide",
    ));
    assert!(
        !findings(&course)
            .iter()
            .any(|finding| finding.check == "contrast"
                && finding.object.as_deref() == Some("no-fill-dark")),
        "{:#?}",
        findings(&course)
    );

    let mut course = empty_course();
    slide(&mut course).objects.push(shape_with_text(
        "no-fill-light",
        "#00000000",
        "#FFFFFF",
        "White on the slide",
    ));
    assert!(
        findings(&course)
            .iter()
            .any(|finding| finding.check == "contrast"
                && finding.object.as_deref() == Some("no-fill-light")),
        "{:#?}",
        findings(&course)
    );
}
