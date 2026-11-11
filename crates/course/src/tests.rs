use crate::{Course, Event, Severity, Trigger};

const HELLO: &str = include_str!("../../../examples/hello-course/course.json");

fn errors(c: &Course) -> Vec<String> {
    c.validate()
        .into_iter()
        .filter(|i| i.severity == Severity::Error)
        .map(|i| format!("{}: {}", i.path, i.message))
        .collect()
}

#[test]
fn example_course_is_valid() {
    let c = Course::from_json(HELLO).unwrap();
    assert_eq!(errors(&c), Vec::<String>::new());
    assert!(c.validate().is_empty(), "{:#?}", c.validate());
}

#[test]
fn round_trips_through_json() {
    let c = Course::from_json(HELLO).unwrap();
    let again = Course::from_json(&c.to_json().unwrap()).unwrap();
    assert_eq!(c, again);
}

#[test]
fn existing_example_save_is_stable_and_omits_the_new_group_field() {
    let c = Course::from_json(HELLO).unwrap();
    let saved = c.to_json().unwrap();
    let again = Course::from_json(&saved).unwrap();
    assert_eq!(c, again);
    assert_eq!(again.to_json().unwrap(), saved);
    assert!(!saved.contains("\"group\""));
    assert!(!saved.contains("\"crop\""));
}

#[test]
fn showcase_media_files_are_all_referenced() {
    let c = Course::from_json(include_str!("../../../examples/showcase/course.json")).unwrap();
    let references = crate::media_references(&c);
    for file in [
        "banner.png",
        "chime.wav",
        "chime.vtt",
        "floorplan.png",
        "intro.mp4",
        "intro.vtt",
    ] {
        assert!(
            references.iter().any(|reference| {
                reference.src == file
                    || (reference.src.ends_with('/') && file.starts_with(&reference.src))
            }),
            "{file} must be referenced; got {references:#?}"
        );
    }
}

#[test]
fn background_audio_defaults_are_omitted_from_json() {
    let mut c = Course::from_json(HELLO).unwrap();
    assert!(c.background_audio.is_none());
    let saved = c.to_json().unwrap();
    assert!(!saved.contains("background_audio"));

    c.background_audio = Some(crate::BackgroundAudio {
        src: "music.mp3".into(),
        volume: 0.5,
        looping: true,
    });
    let value = serde_json::to_value(&c).unwrap();
    let audio = &value["background_audio"];
    assert_eq!(audio["src"], "music.mp3");
    assert!(audio.get("volume").is_none(), "default volume is omitted");
    assert!(audio.get("loop").is_none(), "default loop is omitted");

    let audio: crate::BackgroundAudio =
        serde_json::from_value(serde_json::json!({ "src": "music.mp3" })).unwrap();
    assert_eq!(audio.volume, 0.5);
    assert!(audio.looping);

    let audio: crate::BackgroundAudio = serde_json::from_value(serde_json::json!({
        "src": "music.mp3", "volume": 0.35, "loop": false
    }))
    .unwrap();
    assert_eq!(audio.volume, 0.35);
    assert!(!audio.looping);

    assert!(
        serde_json::from_value::<crate::BackgroundAudio>(
            serde_json::json!({ "src": "music.mp3", "unknown": true })
        )
        .is_err(),
        "unknown background audio fields are rejected"
    );
}

#[test]
fn background_audio_validation_checks_path_and_volume() {
    let e = mutate(|c| {
        c.background_audio = Some(crate::BackgroundAudio {
            src: "../music.mp3".into(),
            volume: 0.5,
            looping: true,
        });
    });
    assert!(
        e.iter().any(|m| m.contains("/background_audio/src")),
        "{e:?}"
    );

    let e = mutate(|c| {
        c.background_audio = Some(crate::BackgroundAudio {
            src: "music.mp3".into(),
            volume: 1.5,
            looping: true,
        });
    });
    assert!(
        e.iter().any(|m| m.contains("/background_audio/volume")),
        "{e:?}"
    );

    let e = mutate(|c| {
        c.background_audio = Some(crate::BackgroundAudio {
            src: "music.mp3".into(),
            volume: f32::NAN,
            looping: true,
        });
    });
    assert!(
        e.iter().any(|m| m.contains("/background_audio/volume")),
        "{e:?}"
    );

    let e = mutate(|c| {
        c.background_audio = Some(crate::BackgroundAudio {
            src: "   ".into(),
            volume: 0.5,
            looping: true,
        });
    });
    assert!(
        e.iter().any(|m| m.contains("/background_audio/src")),
        "{e:?}"
    );
}

#[test]
fn background_audio_triggers_need_a_course_track() {
    let reserved = crate::BACKGROUND_AUDIO_ID.to_owned();
    let mut c = Course::from_json(HELLO).unwrap();
    c.background_audio = Some(crate::BackgroundAudio {
        src: "music.mp3".into(),
        volume: 0.5,
        looping: true,
    });
    c.slide_mut("s-intro").unwrap().triggers.push(Trigger {
        when: Event::MediaEnds {
            object: reserved.clone(),
        },
        action: crate::Action::PlayMedia {
            object: reserved.clone(),
        },
        condition: None,
        disabled: false,
    });
    assert!(errors(&c).is_empty(), "{:?}", errors(&c));

    let mut c = Course::from_json(HELLO).unwrap();
    c.slide_mut("s-intro").unwrap().triggers.push(Trigger {
        when: Event::MediaEnds {
            object: reserved.clone(),
        },
        action: crate::Action::PauseMedia {
            object: reserved.clone(),
        },
        condition: None,
        disabled: false,
    });
    let e = errors(&c);
    assert!(
        e.iter()
            .filter(|m| m.contains("the course has no background audio"))
            .count()
            >= 2,
        "{e:?}"
    );
}

#[test]
fn unknown_fields_are_rejected() {
    let bad = HELLO.replacen(
        "\"title\": \"Workplace",
        "\"titel\": \"x\", \"title\": \"Workplace",
        1,
    );
    assert!(Course::from_json(&bad).is_err());
}

#[test]
fn lookups() {
    let c = Course::from_json(HELLO).unwrap();
    assert_eq!(c.slide_index("s-quiz"), Some(2));
    assert_eq!(c.start_index(), Some(0));
    let s = c.slide("s-intro").unwrap();
    assert_eq!(s.object("btn-close").map(|(_, l)| l), Some(Some("about")));
    assert_eq!(s.object("btn-start").map(|(_, l)| l), Some(None));
    assert!(s.object("nope").is_none());
}

fn mutate(f: impl FnOnce(&mut Course)) -> Vec<String> {
    let mut c = Course::from_json(HELLO).unwrap();
    f(&mut c);
    errors(&c)
}

#[test]
fn catches_broken_references() {
    let e = mutate(|c| {
        let s = c.slide_mut("s-intro").unwrap();
        s.triggers[1].action = crate::Action::ShowLayer {
            layer: "ghost".into(),
        };
    });
    assert!(e.iter().any(|m| m.contains("layer \"ghost\"")), "{e:?}");

    let e = mutate(|c| c.start_slide = Some("nowhere".into()));
    assert!(e.iter().any(|m| m.contains("start_slide")), "{e:?}");

    let e = mutate(|c| {
        let s = c.slide_mut("s-intro").unwrap();
        s.triggers.push(crate::Trigger {
            action: crate::Action::HideLayer { layer: None },
            when: crate::Event::Click {
                object: "title".into(),
            },
            condition: None,
            disabled: false,
        });
    });
    assert!(e.iter().any(|m| m.contains("base layer")), "{e:?}");
}

#[test]
fn catches_type_errors() {
    let e = mutate(|c| {
        let s = c.slide_mut("s-tabs").unwrap();
        s.triggers[0].action = crate::Action::SetVariable {
            variable: "tabs_seen".into(),
            op: crate::VarOp::Toggle,
            value: None,
        };
    });
    assert!(e.iter().any(|m| m.contains("toggle")), "{e:?}");

    let e = mutate(|c| {
        let s = c.slide_mut("s-tabs").unwrap();
        s.triggers[6].condition = Some(crate::Condition::Compare {
            variable: "tabs_seen".into(),
            op: crate::CmpOp::Ge,
            value: crate::Value::Text("3".into()),
        });
    });
    assert!(e.iter().any(|m| m.contains("compared with")), "{e:?}");
}

#[test]
fn catches_bad_questions_and_ids() {
    let e = mutate(|c| {
        let q = c.slide_mut("s-quiz").unwrap().question.as_mut().unwrap();
        q.choices[0].correct = true;
    });
    assert!(e.iter().any(|m| m.contains("exactly 1 correct")), "{e:?}");

    let e = mutate(|c| {
        let s = c.slide_mut("s-tabs").unwrap();
        s.objects[1].id = "tab1".into();
    });
    assert!(
        e.iter().any(|m| m.contains("duplicate object/layer id")),
        "{e:?}"
    );
}

#[test]
fn deep_conditions_are_rejected_not_overflowed() {
    let e = mutate(|c| {
        let mut cond = crate::Condition::Compare {
            variable: "tabs_seen".into(),
            op: crate::CmpOp::Eq,
            value: crate::Value::Number(1.0),
        };
        // serde_json's recursion limit (128) already caps parsed input; this
        // checks courses built in code (engine, importers) too.
        for _ in 0..200 {
            cond = crate::Condition::Not(Box::new(cond));
        }
        c.slide_mut("s-tabs").unwrap().triggers[0].condition = Some(cond);
    });
    assert!(e.iter().any(|m| m.contains("nested deeper")), "{e:?}");
}

#[test]
fn builtin_variables_validation() {
    // compare on a built-in variable with a number is valid
    let e = mutate(|c| {
        let s = c.slide_mut("s-tabs").unwrap();
        s.triggers[6].condition = Some(crate::Condition::Compare {
            variable: "Course.Progress".into(),
            op: crate::CmpOp::Ge,
            value: crate::Value::Number(50.0),
        });
    });
    assert!(e.is_empty(), "expected valid compare on built-in: {e:?}");

    // compare on a built-in with a non-number is an error
    let e = mutate(|c| {
        let s = c.slide_mut("s-tabs").unwrap();
        s.triggers[6].condition = Some(crate::Condition::Compare {
            variable: "Course.Progress".into(),
            op: crate::CmpOp::Eq,
            value: crate::Value::Bool(true),
        });
    });
    assert!(e.iter().any(|m| m.contains("compared with")), "{e:?}");

    // set_variable on a built-in is an error
    let e = mutate(|c| {
        let s = c.slide_mut("s-tabs").unwrap();
        s.triggers[0].action = crate::Action::SetVariable {
            variable: "Course.SlideNumber".into(),
            op: crate::VarOp::Assign,
            value: Some(crate::Value::Number(2.0)),
        };
    });
    assert!(
        e.iter().any(|m| m.contains("read-only built-in variable")),
        "{e:?}"
    );

    // variable_changes trigger on a built-in is an error
    let e = mutate(|c| {
        let s = c.slide_mut("s-tabs").unwrap();
        s.triggers[0].when = crate::Event::VariableChanges {
            variable: "Slide.ElapsedMs".into(),
        };
    });
    assert!(
        e.iter()
            .any(|m| m.contains("built-in variables do not fire variable_changes")),
        "{e:?}"
    );

    // declaring a variable named like a built-in is rejected by the name rule
    let e = mutate(|c| {
        c.variables.push(crate::VariableDef {
            name: "Course.Progress".into(),
            value: crate::Value::Number(0.0),
        });
    });
    assert!(
        e.iter().any(|m| m.contains("[A-Za-z0-9_-]")),
        "expected name error for dot in user variable: {e:?}"
    );
}

#[test]
fn text_style_italic_round_trip() {
    let mut style = crate::TextStyle::default();
    assert!(!style.italic);
    let json = serde_json::to_string(&style).unwrap();
    assert!(
        !json.contains("italic"),
        "italic: false should be omitted in serialization"
    );

    style.italic = true;
    let json = serde_json::to_string(&style).unwrap();
    assert!(
        json.contains("\"italic\":true"),
        "italic: true should be serialized"
    );

    let parsed: crate::TextStyle = serde_json::from_str(&json).unwrap();
    assert!(parsed.italic);
}

#[test]
fn hello_course_serializes_unchanged() {
    let c = Course::from_json(HELLO).unwrap();
    let json = c.to_json().unwrap();
    assert!(!json.contains("underline"), "underline should not appear");
    assert!(!json.contains("align"), "align should not appear");
    assert!(!json.contains("valign"), "valign should not appear");
    assert!(!json.contains("runs"), "runs should not appear");
    assert!(!json.contains("theme"), "theme should not appear");
    assert!(!json.contains("background"), "background should not appear");
    assert!(!json.contains("\"accepted\""), "accepted should not appear");
    assert!(
        !json.contains("case_sensitive"),
        "case_sensitive should not appear"
    );
    assert!(!json.contains("\"pairs\""), "pairs should not appear");
    assert!(!json.contains("true_false"), "true_false should not appear");
    assert!(
        !json.contains("hotspot_image"),
        "hotspot_image should not appear"
    );
    assert!(!json.contains("\"regions\""), "regions should not appear");
    assert!(!json.contains("drag_items"), "drag_items should not appear");
    assert!(
        !json.contains("drop_targets"),
        "drop_targets should not appear"
    );
    assert!(
        !json.contains("snap_to_center"),
        "snap_to_center should not appear"
    );
    assert!(
        !json.contains("return_if_wrong"),
        "return_if_wrong should not appear"
    );
    assert!(
        !json.contains("one_per_target"),
        "one_per_target should not appear"
    );
    assert!(!json.contains("drag_drop"), "drag_drop should not appear");
    assert!(!json.contains("\"markers\""), "markers should not appear");
    assert_eq!(json, c.to_json().unwrap());
    let roundtripped = Course::from_json(&json).unwrap();
    assert_eq!(c, roundtripped);
}

#[test]
fn text_style_underline_align_valign_round_trip() {
    let mut style = crate::TextStyle::default();
    assert!(!style.underline);
    assert_eq!(style.align, crate::Align::Left);
    assert_eq!(style.valign, crate::VAlign::Top);

    let json = serde_json::to_string(&style).unwrap();
    assert!(!json.contains("underline"));
    assert!(!json.contains("align"));
    assert!(!json.contains("valign"));

    style.underline = true;
    style.align = crate::Align::Center;
    style.valign = crate::VAlign::Middle;

    let json = serde_json::to_string(&style).unwrap();
    assert!(json.contains("\"underline\":true"));
    assert!(json.contains("\"align\":\"center\""));
    assert!(json.contains("\"valign\":\"middle\""));

    let parsed: crate::TextStyle = serde_json::from_str(&json).unwrap();
    assert!(parsed.underline);
    assert_eq!(parsed.align, crate::Align::Center);
    assert_eq!(parsed.valign, crate::VAlign::Middle);
}

#[test]
fn text_run_serialization_and_round_trip() {
    let run = crate::TextRun {
        text: "hello".into(),
        bold: Some(true),
        italic: None,
        underline: Some(false),
        size: Some(24.0),
        color: Some(crate::Color("#123456".into())),
        link: None,
        font: None,
        strikethrough: None,
        baseline: None,
        letter_spacing: None,
    };
    let json = serde_json::to_string(&run).unwrap();
    assert!(json.contains("\"bold\":true"));
    assert!(!json.contains("italic"));
    assert!(json.contains("\"underline\":false"));
    assert!(json.contains("\"size\":24.0"));
    assert!(json.contains("\"#123456\""));

    let parsed: crate::TextRun = serde_json::from_str(&json).unwrap();
    assert_eq!(run, parsed);
}

#[test]
fn theme_serialization_and_default_skipped() {
    let course = Course::from_json(HELLO).unwrap();
    assert_eq!(course.theme, crate::Theme::default());
    let json = serde_json::to_string(&course).unwrap();
    assert!(
        !json.contains("\"theme\""),
        "default theme should be skipped in serialization"
    );

    let mut custom_course = course.clone();
    custom_course.theme.accent = crate::Color("#112233".into());
    let json_custom = serde_json::to_string(&custom_course).unwrap();
    assert!(json_custom.contains("\"theme\""));
    let parsed: Course = serde_json::from_str(&json_custom).unwrap();
    assert_eq!(parsed.theme.accent.0, "#112233");
}

#[test]
fn hostile_validation_theme_and_runs() {
    // Bad theme accent color
    let e = mutate(|c| {
        c.theme.accent = crate::Color("bad".into());
    });
    assert!(e.iter().any(|m| m.contains("/theme/accent")), "{e:?}");

    // Bad slide background color
    let e = mutate(|c| {
        let s = c.slide_mut("s-intro").unwrap();
        s.background = Some(crate::Color("not-a-color".into()));
    });
    assert!(e.iter().any(|m| m.contains("/background")), "{e:?}");

    // More than 1000 runs
    let e = mutate(|c| {
        let s = c.slide_mut("s-intro").unwrap();
        if let crate::ObjectKind::Text { runs, text, .. } = &mut s.objects[0].kind {
            *runs = (0..1001)
                .map(|i| crate::TextRun {
                    text: format!("{i}"),
                    bold: None,
                    italic: None,
                    underline: None,
                    size: None,
                    color: None,
                    link: None,
                    font: None,
                    strikethrough: None,
                    baseline: None,
                    letter_spacing: None,
                })
                .collect();
            *text = runs.iter().map(|r| r.text.as_str()).collect();
        }
    });
    assert!(e.iter().any(|m| m.contains("/kind/runs")), "{e:?}");

    // Text mismatch warning (not error)
    let c = {
        let mut course = Course::from_json(HELLO).unwrap();
        let s = course.slide_mut("s-intro").unwrap();
        if let crate::ObjectKind::Text { runs, text, .. } = &mut s.objects[0].kind {
            *runs = vec![crate::TextRun {
                text: "Part A".into(),
                bold: Some(true),
                italic: None,
                underline: None,
                size: None,
                color: None,
                link: None,
                font: None,
                strikethrough: None,
                baseline: None,
                letter_spacing: None,
            }];
            *text = "Something else".into();
        }
        course
    };
    let issues = c.validate();
    assert!(
        issues
            .iter()
            .any(|i| i.severity == crate::Severity::Warning && i.message.contains("concatenation"))
    );
    assert!(c.is_playable(), "warnings do not prevent playability");
}

#[test]
fn normalize_runs_table() {
    use crate::text::normalize_runs;

    let empty = vec![
        crate::TextRun {
            text: "".into(),
            bold: Some(true),
            italic: None,
            underline: None,
            size: None,
            color: None,
            link: None,
            font: None,
            strikethrough: None,
            baseline: None,
            letter_spacing: None,
        },
        crate::TextRun {
            text: "Hello ".into(),
            bold: Some(true),
            italic: None,
            underline: None,
            size: None,
            color: None,
            link: None,
            font: None,
            strikethrough: None,
            baseline: None,
            letter_spacing: None,
        },
        crate::TextRun {
            text: "World".into(),
            bold: Some(true),
            italic: None,
            underline: None,
            size: None,
            color: None,
            link: None,
            font: None,
            strikethrough: None,
            baseline: None,
            letter_spacing: None,
        },
    ];
    let norm = normalize_runs(&empty);
    assert_eq!(norm.len(), 1);
    assert_eq!(norm[0].text, "Hello World");
    assert_eq!(norm[0].bold, Some(true));
}

#[test]
fn apply_text_edit_table() {
    use crate::text::apply_text_edit;

    let runs = vec![
        crate::TextRun {
            text: "Hello ".into(),
            bold: Some(true),
            italic: None,
            underline: None,
            size: None,
            color: None,
            link: None,
            font: None,
            strikethrough: None,
            baseline: None,
            letter_spacing: None,
        },
        crate::TextRun {
            text: "World".into(),
            bold: None,
            italic: Some(true),
            underline: None,
            size: None,
            color: None,
            link: None,
            font: None,
            strikethrough: None,
            baseline: None,
            letter_spacing: None,
        },
    ];

    // Insert at start: position 0 -> takes first run overrides (bold: true)
    let edited = apply_text_edit(&runs, "Hello World", "Say: Hello World");
    assert_eq!(edited[0].text, "Say: Hello ");
    assert_eq!(edited[0].bold, Some(true));

    // Insert in middle of run 0
    let edited = apply_text_edit(&runs, "Hello World", "Hel--lo World");
    assert_eq!(edited[0].text, "Hel--lo ");
    assert_eq!(edited[0].bold, Some(true));
    assert_eq!(edited[1].text, "World");
    assert_eq!(edited[1].italic, Some(true));

    // Insert in middle of run 1
    let edited = apply_text_edit(&runs, "Hello World", "Hello Wo--rld");
    assert_eq!(edited.len(), 2);
    assert_eq!(edited[1].text, "Wo--rld");
    assert_eq!(edited[1].italic, Some(true));

    // Delete across runs
    let edited = apply_text_edit(&runs, "Hello World", "Held");
    assert_eq!(edited.len(), 2);
    assert_eq!(edited[0].text, "Hel");
    assert_eq!(edited[0].bold, Some(true));
    assert_eq!(edited[1].text, "d");
    assert_eq!(edited[1].italic, Some(true));

    // Multibyte characters safe
    let runs_mb = vec![
        crate::TextRun {
            text: "🦀".into(),
            bold: Some(true),
            italic: None,
            underline: None,
            size: None,
            color: None,
            link: None,
            font: None,
            strikethrough: None,
            baseline: None,
            letter_spacing: None,
        },
        crate::TextRun {
            text: "🚀".into(),
            bold: None,
            italic: Some(true),
            underline: None,
            size: None,
            color: None,
            link: None,
            font: None,
            strikethrough: None,
            baseline: None,
            letter_spacing: None,
        },
    ];
    let edited_mb = apply_text_edit(&runs_mb, "🦀🚀", "🦀✨🚀");
    assert_eq!(edited_mb.len(), 2);
    assert_eq!(edited_mb[0].text, "🦀✨");
    assert_eq!(edited_mb[0].bold, Some(true));
    assert_eq!(edited_mb[1].text, "🚀");
    assert_eq!(edited_mb[1].italic, Some(true));
}

#[test]
fn apply_style_to_range_table() {
    use crate::text::{TextRunPatch, apply_style_to_range};

    let plain = "Hello World";
    // Range (0, 5) gets bold
    let styled = apply_style_to_range(
        &[],
        plain,
        (0, 5),
        &TextRunPatch {
            bold: Some(true),
            ..Default::default()
        },
    );
    assert_eq!(styled.len(), 2);
    assert_eq!(styled[0].text, "Hello");
    assert_eq!(styled[0].bold, Some(true));
    assert_eq!(styled[1].text, " World");
    assert_eq!(styled[1].bold, None);

    // Empty range applies to whole text
    let styled_all = apply_style_to_range(
        &styled,
        plain,
        (0, 0),
        &TextRunPatch {
            italic: Some(true),
            ..Default::default()
        },
    );
    assert_eq!(styled_all.len(), 2);
    assert_eq!(styled_all[0].bold, Some(true));
    assert_eq!(styled_all[0].italic, Some(true));
    assert_eq!(styled_all[1].bold, None);
    assert_eq!(styled_all[1].italic, Some(true));

    // If every run has no overrides, returns empty vec
    let styled_none = apply_style_to_range(&[], plain, (0, 5), &TextRunPatch::default());
    assert!(styled_none.is_empty());
}

#[test]
fn animation_and_transition_serialization_and_validation() {
    use crate::{
        Action, AnimEffect, Animation, Direction, Easing, Event, MotionPath, Point, Transition,
        TransitionEffect, Trigger,
    };

    let mut c = Course::from_json(HELLO).unwrap();
    let s = c.slide_mut("s-intro").unwrap();
    s.transition = Some(Transition {
        effect: TransitionEffect::Push {
            from: Direction::Left,
        },
        duration_ms: 600,
    });

    let obj = s.objects.iter_mut().find(|o| o.id == "btn-start").unwrap();
    obj.entrance = Some(Animation {
        effect: AnimEffect::FlyIn {
            from: Direction::Bottom,
        },
        duration_ms: 400,
        easing: Easing::EaseOut,
    });
    obj.exit = Some(Animation {
        effect: AnimEffect::Fade,
        duration_ms: 300,
        easing: Easing::EaseIn,
    });
    obj.motion_paths = vec![MotionPath {
        id: "path1".into(),
        points: vec![Point { x: 0.0, y: 0.0 }, Point { x: 150.0, y: 50.0 }],
        duration_ms: 1200,
        easing: Easing::EaseInOut,
        relative: true,
    }];

    // Add triggers using move_on_path and animation_completes
    s.triggers.push(Trigger {
        action: Action::MoveOnPath {
            object: "btn-start".into(),
            path: "path1".into(),
        },
        when: Event::Click {
            object: "btn-start".into(),
        },
        condition: None,
        disabled: false,
    });
    s.triggers.push(Trigger {
        action: Action::PlayMedia {
            object: "audio-welcome".into(),
        },
        when: Event::AnimationCompletes {
            object: "btn-start".into(),
            path: Some("path1".into()),
        },
        condition: None,
        disabled: false,
    });
    // Add audio object so PlayMedia is valid
    s.objects.push(crate::Object {
        id: "audio-welcome".into(),
        name: "Audio Welcome".into(),
        kind: crate::ObjectKind::Audio {
            src: "media/welcome.mp3".into(),
            autoplay: false,
            captions: None,
            volume: 1.0,
            trim: None,
        },
        frame: crate::Rect {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        },
        timing: crate::Timing::default(),
        states: Vec::new(),
        initial_state: "normal".into(),
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
    });

    assert_eq!(errors(&c), Vec::<String>::new());

    // Round-trip through JSON
    let json_str = c.to_json().unwrap();
    let reloaded = Course::from_json(&json_str).unwrap();
    assert_eq!(c, reloaded);

    // Hostile validation tests
    let mut bad_c = c.clone();
    let bad_s = bad_c.slide_mut("s-intro").unwrap();
    bad_s.transition = Some(Transition {
        effect: TransitionEffect::Fade,
        duration_ms: 0,
    });
    assert!(
        errors(&bad_c)
            .iter()
            .any(|m| m.contains("transition duration_ms"))
    );

    let mut bad_c2 = c.clone();
    let bad_s2 = bad_c2.slide_mut("s-intro").unwrap();
    let bad_obj = bad_s2
        .objects
        .iter_mut()
        .find(|o| o.id == "btn-start")
        .unwrap();
    bad_obj.entrance = Some(Animation {
        effect: AnimEffect::Fade,
        duration_ms: 70000,
        easing: Easing::Linear,
    });
    assert!(
        errors(&bad_c2)
            .iter()
            .any(|m| m.contains("animation duration_ms"))
    );

    // Path with bad coordinates (NaN / out-of-range)
    let mut bad_c3 = c.clone();
    let bad_s3 = bad_c3.slide_mut("s-intro").unwrap();
    let bad_obj3 = bad_s3
        .objects
        .iter_mut()
        .find(|o| o.id == "btn-start")
        .unwrap();
    bad_obj3.motion_paths[0].points[1].x = 200_000.0;
    assert!(
        errors(&bad_c3)
            .iter()
            .any(|m| m.contains("path coordinates"))
    );

    // Bad action referring to non-existent path
    let mut bad_c4 = c.clone();
    let bad_s4 = bad_c4.slide_mut("s-intro").unwrap();
    bad_s4.triggers[2].action = Action::MoveOnPath {
        object: "btn-start".into(),
        path: "ghost_path".into(),
    };
    assert!(
        errors(&bad_c4)
            .iter()
            .any(|m| m.contains("motion path \"ghost_path\""))
    );

    // Bad event referring to non-existent path
    let mut bad_c5 = c.clone();
    let bad_s5 = bad_c5.slide_mut("s-intro").unwrap();
    bad_s5.triggers[3].when = Event::AnimationCompletes {
        object: "btn-start".into(),
        path: Some("ghost_path".into()),
    };
    assert!(
        errors(&bad_c5)
            .iter()
            .any(|m| m.contains("motion path \"ghost_path\""))
    );
}

fn set_quiz(mut q: serde_json::Value) -> Vec<String> {
    mutate(|c| {
        let parsed = serde_json::from_value(q.take()).unwrap();
        c.slide_mut("s-quiz").unwrap().question = Some(parsed);
    })
}

fn issues_of(mut q: serde_json::Value) -> Vec<crate::Issue> {
    let mut c = Course::from_json(HELLO).unwrap();
    let parsed = serde_json::from_value(q.take()).unwrap();
    c.slide_mut("s-quiz").unwrap().question = Some(parsed);
    c.validate()
}

#[test]
fn true_false_requires_two_choices_and_one_correct() {
    let e = set_quiz(serde_json::json!({
        "kind": "true_false",
        "prompt": "The sky is blue.",
        "choices": [{ "id": "true", "text": "True", "correct": true }]
    }));
    assert!(e.iter().any(|m| m.contains("exactly 2 choices")), "{e:?}");

    let e = set_quiz(serde_json::json!({
        "kind": "true_false",
        "prompt": "The sky is blue.",
        "choices": [
            { "id": "true", "text": "True", "correct": true },
            { "id": "false", "text": "False", "correct": true }
        ]
    }));
    assert!(
        e.iter().any(|m| m.contains("exactly 1 correct choice")),
        "{e:?}"
    );

    let e = set_quiz(serde_json::json!({
        "kind": "true_false",
        "prompt": "The sky is blue.",
        "choices": [
            { "id": "true", "text": "True", "correct": true },
            { "id": "false", "text": "False" }
        ]
    }));
    assert!(e.is_empty(), "{e:?}");
}

#[test]
fn fill_in_rules_reject_choices_and_bad_answers() {
    let e = set_quiz(serde_json::json!({
        "kind": "fill_in",
        "prompt": "Capital?",
        "choices": [{ "id": "a", "text": "A", "correct": true }],
        "accepted": ["Paris"]
    }));
    assert!(
        e.iter().any(|m| m.contains("must not have choices")),
        "{e:?}"
    );

    let e = set_quiz(serde_json::json!({
        "kind": "fill_in",
        "prompt": "Capital?",
        "choices": [],
        "accepted": []
    }));
    assert!(e.iter().any(|m| m.contains("1 to 50 accepted")), "{e:?}");

    let e = set_quiz(serde_json::json!({
        "kind": "fill_in",
        "prompt": "Capital?",
        "choices": [],
        "accepted": ["   "]
    }));
    assert!(e.iter().any(|m| m.contains("1 to 500 characters")), "{e:?}");

    let e = set_quiz(serde_json::json!({
        "kind": "fill_in",
        "prompt": "Capital?",
        "choices": [],
        "accepted": ["Paris"],
        "case_sensitive": true
    }));
    assert!(e.is_empty(), "{e:?}");
}

#[test]
fn sequence_warns_on_correct_flags_and_shuffle() {
    let issues = issues_of(serde_json::json!({
        "kind": "sequence",
        "prompt": "Order these.",
        "choices": [
            { "id": "a", "text": "First", "correct": true },
            { "id": "b", "text": "Second" }
        ],
        "shuffle": true
    }));
    assert!(
        issues.iter().all(|i| i.severity != Severity::Error),
        "{issues:?}"
    );
    assert!(
        issues
            .iter()
            .any(|i| i.message.contains("ignores correct flags")),
        "{issues:?}"
    );
    assert!(
        issues
            .iter()
            .any(|i| i.message.contains("shuffle is ignored for sequence")),
        "{issues:?}"
    );

    let e = set_quiz(serde_json::json!({
        "kind": "sequence",
        "prompt": "Order these.",
        "choices": [{ "id": "a", "text": "Only" }]
    }));
    assert!(e.iter().any(|m| m.contains("2 to 20 choices")), "{e:?}");
}

#[test]
fn matching_requires_pairs_and_warns_when_shuffle_is_set() {
    let e = set_quiz(serde_json::json!({
        "kind": "matching",
        "prompt": "Match them.",
        "choices": [{ "id": "a", "text": "A", "correct": true }],
        "pairs": [
            { "id": "p1", "left": "A", "right": "1" },
            { "id": "p2", "left": "B", "right": "2" }
        ]
    }));
    assert!(
        e.iter().any(|m| m.contains("must not have choices")),
        "{e:?}"
    );

    let e = set_quiz(serde_json::json!({
        "kind": "matching",
        "prompt": "Match them.",
        "choices": [],
        "pairs": [
            { "id": "p1", "left": "A", "right": "1" },
            { "id": "p1", "left": "B", "right": "2" }
        ]
    }));
    assert!(e.iter().any(|m| m.contains("duplicate pair id")), "{e:?}");

    let e = set_quiz(serde_json::json!({
        "kind": "matching",
        "prompt": "Match them.",
        "choices": [],
        "pairs": [
            { "id": "p1", "left": "", "right": "1" },
            { "id": "p2", "left": "B", "right": "2" }
        ]
    }));
    assert!(e.iter().any(|m| m.contains("non-empty")), "{e:?}");

    let issues = issues_of(serde_json::json!({
        "kind": "matching",
        "prompt": "Match them.",
        "choices": [],
        "shuffle": true,
        "pairs": [
            { "id": "p1", "left": "A", "right": "1" },
            { "id": "p2", "left": "B", "right": "2" }
        ]
    }));
    assert!(
        issues.iter().all(|i| i.severity != Severity::Error),
        "{issues:?}"
    );
    assert!(
        issues
            .iter()
            .any(|i| i.message.contains("shuffle is ignored for matching")),
        "{issues:?}"
    );
}

#[test]
fn new_question_kinds_round_trip() {
    let raw = serde_json::json!({
        "kind": "matching",
        "prompt": "Match %name%",
        "choices": [],
        "pairs": [
            { "id": "p1", "left": "Left", "right": "Right" },
            { "id": "p2", "left": "L2", "right": "R2" }
        ],
        "points": 4,
        "attempts": 0
    });
    let q: crate::Question = serde_json::from_value(raw).unwrap();
    let again: crate::Question = serde_json::from_str(&serde_json::to_string(&q).unwrap()).unwrap();
    assert_eq!(q, again);
    assert!(q.choices.is_empty());
    assert!(!q.case_sensitive);
    assert!(q.hotspot_image.is_none());
    assert!(q.regions.is_empty());
    assert_eq!(q.markers, 1);
    assert!(q.drag_items.is_empty());
    assert!(q.drop_targets.is_empty());
    assert!(q.snap_to_center);
    assert!(!q.return_if_wrong);
    assert!(q.one_per_target);
    let json = serde_json::to_string(&q).unwrap();
    assert!(!json.contains("hotspot_image"));
    assert!(!json.contains("snap_to_center"));
    assert!(!json.contains("one_per_target"));
    assert!(!json.contains("markers"));
}

#[test]
fn freeform_question_round_trips_and_omits_an_absent_condition() {
    let raw = serde_json::json!({
        "kind": "freeform",
        "prompt": "Show your working.",
        "choices": [],
        "points": 7,
        "correct_when": { "compare": { "variable": "tabs_seen", "op": "ge", "value": 0 } }
    });
    let q: crate::Question = serde_json::from_value(raw.clone()).expect("freeform");
    assert_eq!(q.kind, crate::QuestionKind::Freeform);
    assert!(!q.is_survey());
    assert_eq!(q.max_points(), 7);
    let json = serde_json::to_string(&q).unwrap();
    assert!(json.contains("correct_when"), "{json}");
    let again: crate::Question = serde_json::from_str(&json).unwrap();
    assert_eq!(q, again);

    let mc: crate::Question = serde_json::from_value(serde_json::json!({
        "kind": "multiple_choice",
        "prompt": "Q?",
        "choices": [
            { "id": "a", "text": "A", "correct": true },
            { "id": "b", "text": "B" }
        ]
    }))
    .unwrap();
    assert!(!serde_json::to_string(&mc).unwrap().contains("correct_when"));
}

#[test]
fn freeform_validation_requires_a_slide_condition_and_no_other_payload() {
    let good = serde_json::json!({
        "kind": "freeform",
        "prompt": "Show your working.",
        "choices": [],
        "correct_when": { "compare": { "variable": "tabs_seen", "op": "ge", "value": 0 } }
    });
    let issues = set_quiz(good);
    assert!(issues.is_empty(), "{issues:?}");

    let e = set_quiz(serde_json::json!({
        "kind": "freeform",
        "prompt": "No condition.",
        "choices": []
    }));
    assert!(e.iter().any(|m| m.contains("correct_when")), "{e:?}");

    let e = set_quiz(serde_json::json!({
        "kind": "freeform",
        "prompt": "Wrong slide.",
        "choices": [],
        "correct_when": {
            "object_state": { "object": "tab1", "state": "visited", "is": true }
        }
    }));
    assert!(e.iter().any(|m| m.contains("not on slide")), "{e:?}");

    let e = set_quiz(serde_json::json!({
        "kind": "freeform",
        "prompt": "Choices?",
        "choices": [{ "id": "a", "text": "A" }],
        "correct_when": { "compare": { "variable": "tabs_seen", "op": "ge", "value": 0 } }
    }));
    assert!(
        e.iter().any(|m| m.contains("must not have choices")),
        "{e:?}"
    );

    let e = set_quiz(serde_json::json!({
        "kind": "freeform",
        "prompt": "Per answer?",
        "choices": [],
        "scoring": "per_answer",
        "correct_when": { "compare": { "variable": "tabs_seen", "op": "ge", "value": 0 } }
    }));
    assert!(e.iter().any(|m| m.contains("all_or_nothing")), "{e:?}");

    let e = set_quiz(serde_json::json!({
        "kind": "multiple_choice",
        "prompt": "Q?",
        "choices": [
            { "id": "a", "text": "A", "correct": true },
            { "id": "b", "text": "B" }
        ],
        "correct_when": { "compare": { "variable": "tabs_seen", "op": "ge", "value": 0 } }
    }));
    assert!(
        e.iter().any(|m| m.contains("only allowed on freeform")),
        "{e:?}"
    );
}

fn quiz_with_objects(objects: serde_json::Value, question: serde_json::Value) -> Vec<String> {
    mutate(|c| {
        let slide = c.slide_mut("s-quiz").unwrap();
        let extra: Vec<crate::Object> = serde_json::from_value(objects).unwrap();
        slide.objects.extend(extra);
        slide.question = Some(serde_json::from_value(question).unwrap());
    })
}

#[test]
fn hotspot_rules_require_an_image_and_one_correct_region() {
    let e = set_quiz(serde_json::json!({
        "kind": "hotspot",
        "prompt": "Click it.",
        "choices": [],
        "hotspot_image": "btn-submit",
        "regions": [{ "id": "r1", "shape": "rect", "x": 0.0, "y": 0.0, "w": 10.0, "h": 10.0, "correct": true }]
    }));
    assert!(e.iter().any(|m| m.contains("not an image")), "{e:?}");

    let e = quiz_with_objects(
        serde_json::json!([{
            "id": "photo",
            "kind": { "type": "image", "src": "spill.png", "alt": "spill" },
            "frame": { "x": 40.0, "y": 40.0, "w": 200.0, "h": 120.0 }
        }]),
        serde_json::json!({
            "kind": "hotspot",
            "prompt": "Click it.",
            "choices": [{ "id": "a", "text": "A", "correct": true }],
            "hotspot_image": "photo",
            "regions": [{ "id": "r1", "shape": "rect", "x": 0.0, "y": 0.0, "w": 10.0, "h": 10.0, "correct": true }]
        }),
    );
    assert!(
        e.iter().any(|m| m.contains("must not have choices")),
        "{e:?}"
    );

    let e = quiz_with_objects(
        serde_json::json!([{
            "id": "photo",
            "kind": { "type": "image", "src": "spill.png", "alt": "spill" },
            "frame": { "x": 40.0, "y": 40.0, "w": 200.0, "h": 120.0 }
        }]),
        serde_json::json!({
            "kind": "hotspot",
            "prompt": "Click it.",
            "choices": [],
            "hotspot_image": "photo",
            "regions": [
                { "id": "r1", "shape": "rect", "x": 0.0, "y": 0.0, "w": 0.0, "h": 10.0, "correct": false },
                { "id": "r1", "shape": "ellipse", "x": 100000.0, "y": -100000.0, "w": 10.0, "h": 10.0, "correct": false }
            ],
            "markers": 0
        }),
    );
    assert!(
        e.iter().any(|m| m.contains("at least one correct")),
        "{e:?}"
    );
    assert!(e.iter().any(|m| m.contains("duplicate region")), "{e:?}");
    assert!(e.iter().any(|m| m.contains("w and h")), "{e:?}");
    assert!(
        e.iter().any(|m| m.contains("markers must be 1 to 10")),
        "{e:?}"
    );

    let e = quiz_with_objects(
        serde_json::json!([{
            "id": "photo",
            "kind": { "type": "image", "src": "spill.png", "alt": "spill" },
            "frame": { "x": 40.0, "y": 40.0, "w": 200.0, "h": 120.0 }
        }]),
        serde_json::json!({
            "kind": "hotspot",
            "prompt": "Click it.",
            "choices": [],
            "hotspot_image": "photo",
            "regions": [
                { "id": "r1", "shape": "ellipse", "x": 10.0, "y": 10.0, "w": 40.0, "h": 20.0, "correct": true }
            ]
        }),
    );
    assert!(e.is_empty(), "{e:?}");
}

#[test]
fn drag_drop_rules_reject_shared_ids_and_missing_targets() {
    let e = quiz_with_objects(
        serde_json::json!([
            { "id": "card", "kind": { "type": "shape", "shape": "rectangle", "fill": "#112233" }, "frame": { "x": 0.0, "y": 0.0, "w": 40.0, "h": 40.0 } },
            { "id": "box", "kind": { "type": "shape", "shape": "rectangle", "fill": "#334455" }, "frame": { "x": 200.0, "y": 0.0, "w": 80.0, "h": 80.0 } }
        ]),
        serde_json::json!({
            "kind": "drag_drop",
            "prompt": "Sort them.",
            "choices": [{ "id": "a", "text": "A" }],
            "drag_items": [{ "item": "card", "target": "card" }],
            "drop_targets": ["card"]
        }),
    );
    assert!(
        e.iter().any(|m| m.contains("must not have choices")),
        "{e:?}"
    );
    assert!(
        e.iter()
            .any(|m| m.contains("cannot target") || m.contains("both a drag item")),
        "{e:?}"
    );

    let e = quiz_with_objects(
        serde_json::json!([
            { "id": "card", "kind": { "type": "shape", "shape": "rectangle", "fill": "#112233" }, "frame": { "x": 0.0, "y": 0.0, "w": 40.0, "h": 40.0 } },
            { "id": "chip", "kind": { "type": "shape", "shape": "rectangle", "fill": "#223344" }, "frame": { "x": 0.0, "y": 80.0, "w": 40.0, "h": 40.0 } },
            { "id": "box", "kind": { "type": "shape", "shape": "rectangle", "fill": "#334455" }, "frame": { "x": 200.0, "y": 0.0, "w": 80.0, "h": 80.0 } }
        ]),
        serde_json::json!({
            "kind": "drag_drop",
            "prompt": "Sort them.",
            "choices": [],
            "drag_items": [
                { "item": "card", "target": "box" },
                { "item": "card", "target": "missing" }
            ],
            "drop_targets": ["box", "box"]
        }),
    );
    assert!(e.iter().any(|m| m.contains("duplicate drag item")), "{e:?}");
    assert!(
        e.iter().any(|m| m.contains("duplicate drop target")),
        "{e:?}"
    );
    assert!(
        e.iter().any(|m| m.contains("not a base-layer object")),
        "{e:?}"
    );

    let e = quiz_with_objects(
        serde_json::json!([
            { "id": "card", "kind": { "type": "shape", "shape": "rectangle", "fill": "#112233" }, "frame": { "x": 0.0, "y": 0.0, "w": 40.0, "h": 40.0 } },
            { "id": "box", "kind": { "type": "shape", "shape": "rectangle", "fill": "#334455" }, "frame": { "x": 200.0, "y": 0.0, "w": 80.0, "h": 80.0 } },
            { "id": "decoy", "kind": { "type": "shape", "shape": "ellipse", "fill": "#556677" }, "frame": { "x": 400.0, "y": 0.0, "w": 80.0, "h": 80.0 } }
        ]),
        serde_json::json!({
            "kind": "drag_drop",
            "prompt": "Sort them.",
            "choices": [],
            "drag_items": [{ "item": "card", "target": "box" }],
            "drop_targets": ["box", "decoy"],
            "snap_to_center": false,
            "return_if_wrong": true,
            "one_per_target": false
        }),
    );
    assert!(e.is_empty(), "{e:?}");
}

fn course_with_image_crop(crop: Option<crate::CropRect>) -> Course {
    course_with_media(crate::ObjectKind::Image {
        src: "media/pic.png".into(),
        alt: "A picture".into(),
        crop,
    })
}

#[test]
fn image_crop_round_trips_and_is_omitted_when_unset() {
    let mut c = course_with_image_crop(None);
    let saved = c.to_json().unwrap();
    assert!(!saved.contains("\"crop\""));
    assert_eq!(Course::from_json(&saved).unwrap(), c);

    let crop = crate::CropRect {
        x: 0.1,
        y: 0.2,
        w: 0.5,
        h: 0.6,
    };
    c = course_with_image_crop(Some(crop));
    let saved = c.to_json().unwrap();
    assert!(saved.contains("\"crop\""));
    let reloaded = Course::from_json(&saved).unwrap();
    assert_eq!(reloaded, c);
    assert_eq!(reloaded.to_json().unwrap(), saved);
}

#[test]
fn hostile_image_crops_are_validation_errors() {
    let bad = [
        crate::CropRect {
            x: f32::NAN,
            y: 0.0,
            w: 0.5,
            h: 0.5,
        },
        crate::CropRect {
            x: -0.1,
            y: 0.0,
            w: 0.5,
            h: 0.5,
        },
        crate::CropRect {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.5,
        },
        crate::CropRect {
            x: 0.0,
            y: 0.0,
            w: 0.5,
            h: -0.2,
        },
        crate::CropRect {
            x: 0.8,
            y: 0.0,
            w: 0.5,
            h: 0.5,
        },
        crate::CropRect {
            x: 0.0,
            y: 0.8,
            w: 0.5,
            h: 0.5,
        },
    ];
    for crop in bad {
        let e = errors(&course_with_image_crop(Some(crop)));
        assert!(
            e.iter().any(|m| m.contains("/kind/crop")),
            "crop {crop:?} not refused: {e:?}"
        );
    }
}

#[test]
fn slide_notes_round_trip_and_omitted_when_empty() {
    let mut c = Course::from_json(HELLO).unwrap();
    assert_eq!(c.slide("s-intro").unwrap().notes, "");
    let json = c.to_json().unwrap();
    assert!(!json.contains("\"notes\""));

    c.slide_mut("s-intro").unwrap().notes = "Welcome speaker notes".to_owned();
    let json_with_notes = c.to_json().unwrap();
    assert!(json_with_notes.contains("\"notes\": \"Welcome speaker notes\""));

    let restored = Course::from_json(&json_with_notes).unwrap();
    assert_eq!(
        restored.slide("s-intro").unwrap().notes,
        "Welcome speaker notes"
    );
}

fn course_with_media(kind: crate::ObjectKind) -> Course {
    let mut course = Course::from_json(HELLO).unwrap();
    let slide = &mut course.scenes[0].slides[0];
    slide.objects.push(crate::Object {
        id: "media1".into(),
        name: "Media".into(),
        kind,
        frame: crate::Rect {
            x: 0.0,
            y: 0.0,
            w: 48.0,
            h: 48.0,
        },
        timing: crate::Timing::default(),
        states: Vec::new(),
        initial_state: "normal".into(),
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
    });
    course
}

#[test]
fn valid_caption_paths_are_accepted_for_audio_and_video() {
    for kind in [
        crate::ObjectKind::Audio {
            src: "sound.wav".into(),
            autoplay: false,
            captions: Some("captions/sound.VTT".into()),
            volume: 1.0,
            trim: None,
        },
        crate::ObjectKind::Video {
            src: "clip.mp4".into(),
            autoplay: false,
            captions: Some("clip.vtt".into()),
            volume: 1.0,
            trim: None,
            cues: Vec::new(),
        },
    ] {
        let course = course_with_media(kind);
        assert_eq!(errors(&course), Vec::<String>::new());
    }
}

#[test]
fn web_urls_must_be_https_with_a_host() {
    for url in [
        "http://example.com",
        "https://",
        "https:///path",
        "https://user@example.com",
        "https://exa mple.com",
    ] {
        let course = course_with_media(crate::ObjectKind::Web {
            source: crate::WebSource::Url { url: url.into() },
        });
        assert!(
            errors(&course)
                .iter()
                .any(|error| error.contains("https://")),
            "{url:?} must be rejected: {:?}",
            errors(&course)
        );
    }

    let course = course_with_media(crate::ObjectKind::Web {
        source: crate::WebSource::Url {
            url: "https://example.com/lesson?x=1".into(),
        },
    });
    assert_eq!(errors(&course), Vec::<String>::new());
}

#[test]
fn web_folder_paths_must_stay_inside_media_web_and_entry_must_be_relative() {
    let bad_paths = [
        "/web/site",
        "../web/site",
        "web/../site",
        "web",
        "other/site",
    ];
    for path in bad_paths {
        let course = course_with_media(crate::ObjectKind::Web {
            source: crate::WebSource::Folder {
                path: path.into(),
                entry: "index.html".into(),
            },
        });
        assert!(
            errors(&course)
                .iter()
                .any(|error| error.contains("web folder path")),
            "{path:?} must be rejected: {:?}",
            errors(&course)
        );
    }

    for entry in ["", "../index.html", "/index.html", "nested/../index.html"] {
        let course = course_with_media(crate::ObjectKind::Web {
            source: crate::WebSource::Folder {
                path: "web/site".into(),
                entry: entry.into(),
            },
        });
        assert!(
            errors(&course)
                .iter()
                .any(|error| error.contains("web folder entry")),
            "{entry:?} must be rejected: {:?}",
            errors(&course)
        );
    }

    let course = course_with_media(crate::ObjectKind::Web {
        source: crate::WebSource::Folder {
            path: "web/site".into(),
            entry: "index.html".into(),
        },
    });
    assert_eq!(errors(&course), Vec::<String>::new());
}

#[test]
fn web_folder_entry_defaults_to_index_html_and_round_trips() {
    let kind: crate::ObjectKind = serde_json::from_value(serde_json::json!({
        "type": "web",
        "source": { "type": "folder", "path": "web/site" }
    }))
    .expect("folder without an entry");
    assert_eq!(
        kind,
        crate::ObjectKind::Web {
            source: crate::WebSource::Folder {
                path: "web/site".into(),
                entry: "index.html".into()
            }
        }
    );
    let value = serde_json::to_value(&kind).expect("serialize");
    assert!(
        value["source"].get("entry").is_none(),
        "the default entry is omitted"
    );
}

#[test]
fn web_folder_caps_are_inclusive() {
    assert!(crate::web_folder_within_caps(
        crate::WEB_MAX_FILES,
        crate::WEB_MAX_BYTES
    ));
    assert!(!crate::web_folder_within_caps(crate::WEB_MAX_FILES + 1, 0));
    assert!(!crate::web_folder_within_caps(0, crate::WEB_MAX_BYTES + 1));
}

#[test]
fn lottie_defaults_load_and_serialise_away() {
    let kind: crate::ObjectKind =
        serde_json::from_value(serde_json::json!({ "type": "lottie", "src": "anim.json" }))
            .expect("Lottie defaults");
    let crate::ObjectKind::Lottie {
        loop_playback,
        autoplay,
        speed,
        ..
    } = &kind
    else {
        panic!("expected a Lottie kind");
    };
    assert!(*loop_playback);
    assert!(*autoplay);
    assert!((*speed - 1.0).abs() < f32::EPSILON);

    let value = serde_json::to_value(&kind).expect("serialize");
    assert!(value.get("loop").is_none(), "default loop is omitted");
    assert!(
        value.get("autoplay").is_none(),
        "default autoplay is omitted"
    );
    assert!(value.get("speed").is_none(), "default speed is omitted");

    // The model field is named `loop` in JSON.
    let explicit: crate::ObjectKind = serde_json::from_value(serde_json::json!({
        "type": "lottie",
        "src": "anim.json",
        "loop": false,
        "autoplay": false,
        "speed": 2.0
    }))
    .expect("explicit Lottie settings");
    let value = serde_json::to_value(&explicit).expect("serialize");
    assert_eq!(value.get("loop"), Some(&serde_json::json!(false)));
    assert_eq!(value.get("autoplay"), Some(&serde_json::json!(false)));
    assert_eq!(value.get("speed"), Some(&serde_json::json!(2.0)));
}

#[test]
fn lottie_speed_outside_the_range_is_a_validation_error() {
    let course = course_with_media(crate::ObjectKind::Lottie {
        src: "anim.json".into(),
        loop_playback: true,
        autoplay: true,
        speed: 5.0,
    });
    assert!(
        errors(&course).iter().any(|e| e.contains("Lottie speed")),
        "{:?}",
        errors(&course)
    );
}

#[test]
fn unsafe_or_non_vtt_caption_paths_name_the_object_for_audio_and_video() {
    for kind in [
        crate::ObjectKind::Audio {
            src: "sound.wav".into(),
            autoplay: false,
            captions: Some("../sound.vtt".into()),
            volume: 1.0,
            trim: None,
        },
        crate::ObjectKind::Video {
            src: "clip.mp4".into(),
            autoplay: false,
            captions: Some("clip.srt".into()),
            volume: 1.0,
            trim: None,
            cues: Vec::new(),
        },
    ] {
        let course = course_with_media(kind);
        let found = errors(&course);
        assert!(
            found.iter().any(|e| e.contains("media1")),
            "object must be named: {found:?}"
        );
    }
}

#[test]
fn caption_field_is_omitted_when_unset_and_round_trips_when_set() {
    let mut course = course_with_media(crate::ObjectKind::Video {
        src: "clip.mp4".into(),
        autoplay: false,
        captions: None,
        volume: 1.0,
        trim: None,
        cues: Vec::new(),
    });
    let json = course.to_json().unwrap();
    assert!(!json.contains("captions"));

    if let crate::ObjectKind::Video { captions, .. } =
        &mut course.scenes[0].slides[0].objects.last_mut().unwrap().kind
    {
        *captions = Some("clip.vtt".into());
    }
    let json = course.to_json().unwrap();
    assert!(json.contains("\"captions\""));
    assert!(json.contains("clip.vtt"));
    let restored = Course::from_json(&json).unwrap();
    assert_eq!(restored, course);
}

#[test]
fn default_media_volume_and_trim_serialize_exactly_as_before() {
    let course = course_with_media(crate::ObjectKind::Audio {
        src: "sound.wav".into(),
        autoplay: false,
        captions: Some("sound.vtt".into()),
        volume: 1.0,
        trim: None,
    });
    let object = course.scenes[0].slides[0].objects.last().unwrap();
    assert_eq!(
        serde_json::to_value(object).unwrap(),
        serde_json::json!({
            "id": "media1",
            "name": "Media",
            "kind": {
                "type": "audio",
                "src": "sound.wav",
                "autoplay": false,
                "captions": "sound.vtt"
            },
            "frame": { "x": 0.0, "y": 0.0, "w": 48.0, "h": 48.0 },
            "timing": { "start_ms": 0 },
            "initial_state": "normal"
        })
    );
    let json = course.to_json().unwrap();
    assert!(!json.contains("\"volume\""));
    assert!(!json.contains("\"trim\""));
}

#[test]
fn media_volume_and_trim_round_trip() {
    let mut course = course_with_media(crate::ObjectKind::Video {
        src: "clip.mp4".into(),
        autoplay: false,
        captions: None,
        volume: 1.0,
        trim: None,
        cues: Vec::new(),
    });
    if let crate::ObjectKind::Video { volume, trim, .. } =
        &mut course.scenes[0].slides[0].objects.last_mut().unwrap().kind
    {
        *volume = 0.35;
        *trim = Some(crate::Trim {
            start_ms: 1000,
            end_ms: Some(4000),
        });
    }
    let json = course.to_json().unwrap();
    assert!(json.contains("\"volume\": 0.35"));
    assert!(json.contains("\"start_ms\": 1000"));
    assert!(json.contains("\"end_ms\": 4000"));
    let restored = Course::from_json(&json).unwrap();
    assert_eq!(restored, course);
}

#[test]
fn media_volume_and_trim_validation_rejects_hostile_values() {
    let with = |volume: f32, trim: Option<crate::Trim>| {
        course_with_media(crate::ObjectKind::Audio {
            src: "sound.wav".into(),
            autoplay: false,
            captions: None,
            volume,
            trim,
        })
    };

    for volume in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.1, 1.1] {
        let e = errors(&with(volume, None));
        assert!(
            e.iter().any(|m| m.contains("/kind/volume")),
            "volume {volume} not refused: {e:?}"
        );
    }
    assert_eq!(
        errors(&with(
            0.0,
            Some(crate::Trim {
                start_ms: 0,
                end_ms: Some(crate::MAX_TRIM_MS),
            }),
        )),
        Vec::<String>::new()
    );

    let e = errors(&with(
        1.0,
        Some(crate::Trim {
            start_ms: crate::MAX_TRIM_MS + 1,
            end_ms: None,
        }),
    ));
    assert!(e.iter().any(|m| m.contains("/kind/trim/start_ms")), "{e:?}");

    let e = errors(&with(
        1.0,
        Some(crate::Trim {
            start_ms: 1000,
            end_ms: Some(crate::MAX_TRIM_MS + 1),
        }),
    ));
    assert!(e.iter().any(|m| m.contains("/kind/trim/end_ms")), "{e:?}");

    for end_ms in [0, 1000] {
        let e = errors(&with(
            1.0,
            Some(crate::Trim {
                start_ms: 1000,
                end_ms: Some(end_ms),
            }),
        ));
        assert!(
            e.iter().any(|m| m.contains("must be after its start")),
            "end {end_ms} not refused: {e:?}"
        );
    }
}

#[test]
fn language_and_strings_are_omitted_when_unset_and_validated_when_set() {
    let mut course = Course::from_json(HELLO).unwrap();
    let json = course.to_json().unwrap();
    assert!(!json.contains("\"language\""));
    assert!(!json.contains("\"strings\""));

    course.language = Some("fr-CA".to_owned());
    course
        .strings
        .insert("courseComplete".to_owned(), "Cours terminé".to_owned());
    course.strings.insert("notAKey".to_owned(), "x".to_owned());
    let json = course.to_json().unwrap();
    assert!(json.contains("fr-CA"));
    let issues = course.validate();
    assert!(issues.iter().any(
        |issue| issue.path == "/strings/notAKey" && issue.severity == crate::Severity::Warning
    ));
    assert!(
        !issues
            .iter()
            .any(|issue| issue.severity == crate::Severity::Error)
    );

    course.language = Some("31".to_owned());
    assert!(
        course
            .validate()
            .iter()
            .any(|issue| issue.path == "/language" && issue.severity == crate::Severity::Error)
    );
}

#[test]
fn chrome_strings_fall_back_to_english_and_fill_placeholders() {
    let mut course = Course::from_json(HELLO).unwrap();
    assert_eq!(course.chrome_string("courseComplete"), "Course complete");
    assert_eq!(course.chrome_string("unknown"), "");
    course
        .strings
        .insert("markersPlaced".to_owned(), "{n} placé(s)".to_owned());
    assert_eq!(course.chrome_string("markersPlaced"), "{n} placé(s)");
    assert_eq!(
        crate::fill(course.chrome_string("markersPlaced"), &[("n", "3")]),
        "3 placé(s)"
    );
    assert_eq!(crate::fill("a {x} b", &[]), "a {x} b");
}

#[test]
fn unsupported_scripts_are_warned_once_per_slide() {
    let mut course = Course::from_json(HELLO).unwrap();
    let slide = &mut course.scenes[0].slides[0];
    slide.title = "こんにちは".to_owned();
    let warnings: Vec<_> = course
        .validate()
        .into_iter()
        .filter(|issue| {
            issue.path == "/scenes/0/slides/0" && issue.severity == crate::Severity::Warning
        })
        .collect();
    assert_eq!(warnings.len(), 1);
    assert!(crate::script_unsupported('漢'));
    assert!(!crate::script_unsupported('Ω'));
}

#[test]
fn a_question_only_submitted_by_the_submit_event_warns_about_no_control() {
    use crate::Event;

    let mut course = Course::from_json(HELLO).unwrap();
    let slide = course
        .scenes
        .iter_mut()
        .flat_map(|scene| scene.slides.iter_mut())
        .find(|slide| slide.question.is_some())
        .expect("a question slide");
    for trigger in &mut slide.triggers {
        if trigger.action == crate::Action::SubmitQuestion {
            trigger.when = Event::Submit;
        }
    }
    assert!(course.validate().iter().any(|issue| {
        issue.severity == Severity::Warning
            && issue
                .message
                .contains("no control on this slide submits the question")
    }));
    assert!(course.is_playable());
}

#[test]
fn a_course_with_questions_but_no_results_trigger_warns() {
    let mut course = Course::from_json(HELLO).unwrap();
    for scene in &mut course.scenes {
        for slide in &mut scene.slides {
            slide
                .triggers
                .retain(|trigger| trigger.action != crate::Action::SubmitResults);
            for layer in &mut slide.layers {
                layer
                    .triggers
                    .retain(|trigger| trigger.action != crate::Action::SubmitResults);
            }
        }
    }
    assert!(course.validate().iter().any(|issue| {
        issue.severity == Severity::Warning
            && issue
                .message
                .contains("this course never reports a result to the LMS")
    }));
}

#[test]
fn complete_course_counts_as_reporting_a_result() {
    let mut course = Course::from_json(HELLO).unwrap();
    for scene in &mut course.scenes {
        for slide in &mut scene.slides {
            slide
                .triggers
                .retain(|trigger| trigger.action != crate::Action::SubmitResults);
            for layer in &mut slide.layers {
                layer
                    .triggers
                    .retain(|trigger| trigger.action != crate::Action::SubmitResults);
            }
        }
    }
    let first = course
        .scenes
        .first_mut()
        .and_then(|scene| scene.slides.first_mut())
        .expect("first slide");
    first.triggers.push(Trigger {
        action: crate::Action::CompleteCourse {
            status: crate::CompletionStatus::Completed,
        },
        when: Event::TimelineStarts { layer: None },
        condition: None,
        disabled: false,
    });
    assert!(course.reports_result());
    assert!(!course.validate().iter().any(|issue| {
        issue
            .message
            .contains("this course never reports a result to the LMS")
    }));
}

#[test]
fn survey_question_kinds_round_trip_and_validate() {
    // Round-trip each new kind through JSON.
    for kind in ["likert", "short_answer", "essay"] {
        let choices = if kind == "likert" {
            serde_json::json!([{ "id": "1", "text": "A" }, { "id": "2", "text": "B" }])
        } else {
            serde_json::json!([])
        };
        let raw = serde_json::json!({
            "kind": kind,
            "prompt": "Survey?",
            "choices": choices,
            "points": 10
        });
        let q: crate::Question = serde_json::from_value(raw).unwrap();
        assert!(q.is_survey(), "{kind} should be survey");
        assert_eq!(q.max_points(), 0, "{kind} max_points");
        let again: crate::Question =
            serde_json::from_str(&serde_json::to_string(&q).unwrap()).unwrap();
        assert_eq!(q, again, "{kind} round trip");
        assert!(
            serde_json::to_string(&q).unwrap().contains(kind),
            "{kind} serialized"
        );
    }

    // is_survey is false for FillIn.
    let fi: crate::Question = serde_json::from_value(serde_json::json!({
        "kind": "fill_in",
        "prompt": "x",
        "choices": [],
        "accepted": ["a"]
    }))
    .unwrap();
    assert!(!fi.is_survey());

    // max_points is 0 even with points: 10.
    let q: crate::Question = serde_json::from_value(serde_json::json!({
        "kind": "likert",
        "prompt": "x",
        "choices": [{ "id": "1", "text": "A" }, { "id": "2", "text": "B" }],
        "points": 10
    }))
    .unwrap();
    assert_eq!(q.max_points(), 0);

    // Validation rejections for Likert.
    let e = set_quiz(serde_json::json!({
        "kind": "likert",
        "prompt": "x",
        "choices": [{ "id": "1", "text": "A" }]
    }));
    assert!(e.iter().any(|m| m.contains("2 to 11 choices")), "{e:?}");

    let e = set_quiz(serde_json::json!({
        "kind": "likert",
        "prompt": "x",
        "choices": [
            { "id": "1", "text": "A", "correct": true },
            { "id": "2", "text": "B" }
        ]
    }));
    assert!(
        e.iter().any(|m| m.contains("must not be marked correct")),
        "{e:?}"
    );

    // Validation rejections for ShortAnswer and Essay: choices not allowed.
    let e = set_quiz(serde_json::json!({
        "kind": "short_answer",
        "prompt": "x",
        "choices": [{ "id": "a", "text": "A" }]
    }));
    assert!(
        e.iter().any(|m| m.contains("must not have choices")),
        "{e:?}"
    );

    let e = set_quiz(serde_json::json!({
        "kind": "essay",
        "prompt": "x",
        "choices": [{ "id": "a", "text": "A" }]
    }));
    assert!(
        e.iter().any(|m| m.contains("must not have choices")),
        "{e:?}"
    );

    // Valid cases.
    let e = set_quiz(serde_json::json!({
        "kind": "likert",
        "prompt": "x",
        "choices": [{ "id": "1", "text": "A" }, { "id": "2", "text": "B" }]
    }));
    assert!(e.is_empty(), "{e:?}");

    let e = set_quiz(serde_json::json!({
        "kind": "short_answer",
        "prompt": "x",
        "choices": []
    }));
    assert!(e.is_empty(), "{e:?}");

    let e = set_quiz(serde_json::json!({
        "kind": "essay",
        "prompt": "x",
        "choices": []
    }));
    assert!(e.is_empty(), "{e:?}");
}

// ---------------------------------------------------------------- masters

fn with_master<'a>(course: &'a mut Course, id: &str) -> &'a mut crate::Slide {
    let mut master = course.scenes[0].slides[0].clone();
    master.id = id.to_owned();
    master.title = format!("Master {id}");
    master.triggers.clear();
    master.layers.clear();
    master.question = None;
    master.objects.clear();
    master.background = None;
    master.master = None;
    course.masters.push(master);
    course.masters.last_mut().expect("master just pushed")
}

#[test]
fn masters_are_not_in_play_order_but_slide_finds_them() {
    let mut c = Course::from_json(HELLO).unwrap();
    let play_slides = c.slides().count();
    with_master(&mut c, "m1");
    assert_eq!(c.slides().count(), play_slides);
    assert_eq!(c.slide_index("m1"), None);
    assert!(c.play_slide("m1").is_none());
    assert!(c.slide("m1").is_some());
    assert!(c.slide_mut("m1").is_some());
    assert!(c.template("m1").is_none());
}

#[test]
fn background_is_slide_then_master_then_theme() {
    let mut c = Course::from_json(HELLO).unwrap();
    with_master(&mut c, "m1").background = Some(crate::Color("#112233".into()));
    c.scenes[0].slides[0].master = Some("m1".into());
    let slide = c.scenes[0].slides[0].clone();
    assert_eq!(c.slide_background(&slide).0, "#112233");
    c.scenes[0].slides[0].background = Some(crate::Color("#445566".into()));
    let slide = c.scenes[0].slides[0].clone();
    assert_eq!(c.slide_background(&slide).0, "#445566");
    c.scenes[0].slides[0].background = None;
    c.scenes[0].slides[0].master = None;
    let slide = c.scenes[0].slides[0].clone();
    assert_eq!(c.slide_background(&slide).0, c.theme.background.0);
}

#[test]
fn a_jump_to_a_master_is_a_validation_error() {
    let mut c = Course::from_json(HELLO).unwrap();
    with_master(&mut c, "m1");
    let slide = &mut c.scenes[0].slides[0];
    slide.triggers.push(Trigger {
        action: crate::Action::JumpToSlide {
            target: crate::SlideTarget::Id("m1".into()),
        },
        when: Event::TimelineStarts { layer: None },
        condition: None,
        disabled: false,
    });
    let errors = errors(&c);
    assert!(
        errors
            .iter()
            .any(|e| e.contains("slide \"m1\" does not exist")),
        "{errors:?}"
    );
}

#[test]
fn a_master_must_not_have_play_time_behaviour_or_media() {
    let mut c = Course::from_json(HELLO).unwrap();
    with_master(&mut c, "m1").triggers.push(Trigger {
        action: crate::Action::PauseTimeline,
        when: Event::TimelineStarts { layer: None },
        condition: None,
        disabled: false,
    });
    assert!(
        errors(&c)
            .iter()
            .any(|e| e.contains("must not have triggers"))
    );

    let mut c = Course::from_json(HELLO).unwrap();
    with_master(&mut c, "m1").layers.push(crate::Layer {
        id: "l1".into(),
        name: "Layer".into(),
        duration_ms: 1000,
        objects: Vec::new(),
        triggers: Vec::new(),
        hide_other_layers: true,
        pause_base_timeline: false,
        prevent_base_clicks: false,
        hide_base_objects: false,
        hide_when_timeline_ends: false,
        allow_seek: crate::SeekMode::Auto,
        revisit: crate::RevisitMode::Auto,
    });
    assert!(
        errors(&c)
            .iter()
            .any(|e| e.contains("must not have layers"))
    );

    let mut c = Course::from_json(HELLO).unwrap();
    with_master(&mut c, "m1").objects.push(crate::Object {
        id: "a1".into(),
        name: String::new(),
        kind: crate::ObjectKind::Audio {
            src: "a.mp3".into(),
            autoplay: false,
            captions: None,
            volume: 1.0,
            trim: None,
        },
        frame: crate::Rect {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        },
        timing: crate::Timing::default(),
        states: Vec::new(),
        initial_state: "normal".into(),
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
    });
    assert!(
        errors(&c)
            .iter()
            .any(|e| e.contains("must not hold audio or video"))
    );
}

#[test]
fn a_slide_naming_a_missing_master_warns() {
    let mut c = Course::from_json(HELLO).unwrap();
    c.scenes[0].slides[0].master = Some("ghost".into());
    assert!(errors(&c).is_empty());
    assert!(c.validate().iter().any(|i| {
        i.severity == Severity::Warning
            && i.message.contains("ghost")
            && i.message.contains("no master")
    }));
}

#[test]
fn existing_example_round_trips_without_master_or_template_keys() {
    let c = Course::from_json(HELLO).unwrap();
    let saved = c.to_json().unwrap();
    assert!(!saved.contains("\"masters\""));
    assert!(!saved.contains("\"templates\""));
    assert!(!saved.contains("\"master\""));
    assert_eq!(Course::from_json(&saved).unwrap(), c);
}

fn shape_with_outline_and_text() -> crate::ObjectKind {
    crate::ObjectKind::Shape {
        shape: crate::ShapeKind::Rectangle,
        fill: crate::Color("#00000000".to_owned()),
        fill_style: None,
        outline: Some(crate::Outline {
            color: crate::Color("#2F6FEB".to_owned()),
            width: 3.0,
            dash: crate::DashStyle::Dash,
        }),
        text: Some(crate::ShapeText {
            text: "Hello".to_owned(),
            style: crate::TextStyle::default(),
            runs: vec![crate::TextRun {
                text: "Hello".to_owned(),
                bold: Some(true),
                italic: None,
                underline: None,
                size: None,
                color: None,
                link: None,
                font: None,
                strikethrough: None,
                baseline: None,
                letter_spacing: None,
            }],
            paragraphs: Vec::new(),
            autofit: crate::Autofit::None,
        }),
        adjust: Vec::new(),
    }
}

#[test]
fn shape_outline_and_text_round_trip() {
    let mut c = Course::from_json(HELLO).unwrap();
    c.scenes[0].slides[0].objects.push(crate::Object {
        id: "outlined".to_owned(),
        name: "Outlined".to_owned(),
        kind: shape_with_outline_and_text(),
        frame: crate::Rect {
            x: 10.0,
            y: 10.0,
            w: 200.0,
            h: 120.0,
        },
        timing: crate::Timing::default(),
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
    });
    assert!(errors(&c).is_empty(), "{:?}", errors(&c));
    let json = c.to_json().unwrap();
    let back = Course::from_json(&json).unwrap();
    assert_eq!(back, c);
    let kind = serde_json::to_value(shape_with_outline_and_text()).unwrap();
    assert_eq!(kind["outline"]["dash"], "dash");
    assert_eq!(kind["text"]["text"], "Hello");
}

#[test]
fn a_plain_shape_omits_the_new_fields_and_stays_byte_identical() {
    let plain = crate::ObjectKind::Shape {
        shape: crate::ShapeKind::Ellipse,
        fill: crate::Color("#D0D7DE".to_owned()),
        fill_style: None,
        outline: None,
        text: None,
        adjust: Vec::new(),
    };
    let value = serde_json::to_value(plain).unwrap();
    assert!(value.get("outline").is_none());
    assert!(value.get("text").is_none());
    assert!(value.get("adjust").is_none());
}

#[test]
fn shape_adjust_round_trips_and_clamps_per_kind() {
    let mut kind = crate::ObjectKind::Shape {
        shape: crate::ShapeKind::RoundedRectangle,
        fill: crate::Color("#D0D7DE".to_owned()),
        fill_style: None,
        outline: None,
        text: None,
        adjust: vec![0.2],
    };
    let value = serde_json::to_value(&kind).unwrap();
    assert_eq!(value["adjust"].as_array().map(Vec::len), Some(1));
    let back: crate::ObjectKind = serde_json::from_value(value).unwrap();
    assert_eq!(back, kind);

    if let crate::ObjectKind::Shape { adjust, .. } = &mut kind {
        *adjust = crate::clamp_adjust(crate::ShapeKind::RoundedRectangle, &[f32::NAN, 9.0]);
    }
    let crate::ObjectKind::Shape { adjust, .. } = &kind else {
        panic!("shape");
    };
    assert_eq!(adjust, &vec![0.1]);
}

#[test]
fn invalid_outline_and_shape_text_are_rejected() {
    let mut c = Course::from_json(HELLO).unwrap();
    let mut kind = shape_with_outline_and_text();
    if let crate::ObjectKind::Shape { outline, .. } = &mut kind
        && let Some(outline) = outline
    {
        outline.width = 51.0;
    }
    c.scenes[0].slides[0].objects.push(crate::Object {
        id: "bad-outline".to_owned(),
        name: String::new(),
        kind,
        frame: crate::Rect {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        },
        timing: crate::Timing::default(),
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
    });
    assert!(errors(&c).iter().any(|e| e.contains("outline width")));

    c.scenes[0].slides[0].objects.last_mut().unwrap().kind = crate::ObjectKind::Shape {
        shape: crate::ShapeKind::LineHorizontal,
        fill: crate::Color("#00000000".to_owned()),
        fill_style: None,
        outline: None,
        text: Some(crate::ShapeText {
            text: "nope".to_owned(),
            style: crate::TextStyle::default(),
            runs: Vec::new(),
            paragraphs: Vec::new(),
            autofit: crate::Autofit::None,
        }),
        adjust: Vec::new(),
    };
    assert!(
        errors(&c)
            .iter()
            .any(|e| e.contains("line shapes cannot carry text"))
    );
}

fn default_path() -> crate::Path {
    crate::Path {
        points: vec![
            crate::PathPoint { x: 0.0, y: 0.0 },
            crate::PathPoint { x: 1.0, y: 1.0 },
        ],
        style: crate::PathStyle::Straight,
        closed: false,
        fill: None,
        outline: crate::Outline {
            color: crate::Color("#1F1F1F".into()),
            width: 2.0,
            dash: crate::DashStyle::Solid,
        },
        start_end: crate::LineEnd::None,
        end_end: crate::LineEnd::None,
        connector: None,
        start_glue: None,
        end_glue: None,
    }
}

#[test]
fn path_frame_and_points_round_trip_including_flat_lines() {
    let points = vec![
        crate::Point { x: 10.0, y: 20.0 },
        crate::Point { x: 210.0, y: 120.0 },
    ];
    let (frame, fractions) = crate::path_frame_and_points(&points).unwrap();
    assert_eq!(
        frame,
        crate::Rect {
            x: 10.0,
            y: 20.0,
            w: 200.0,
            h: 100.0
        }
    );
    assert_eq!(fractions.first().map(|p| (p.x, p.y)), Some((0.0, 0.0)));
    assert_eq!(fractions.last().map(|p| (p.x, p.y)), Some((1.0, 1.0)));

    let horizontal = vec![
        crate::Point { x: 10.0, y: 20.0 },
        crate::Point { x: 210.0, y: 20.0 },
    ];
    let (frame, fractions) = crate::path_frame_and_points(&horizontal).unwrap();
    assert!(frame.h >= 1.0);
    for point in &fractions {
        let slide_y = frame.y + point.y * frame.h;
        assert!((slide_y - 20.0).abs() < 0.001);
    }

    let vertical = vec![
        crate::Point { x: 30.0, y: 40.0 },
        crate::Point { x: 30.0, y: 240.0 },
    ];
    let (frame, fractions) = crate::path_frame_and_points(&vertical).unwrap();
    assert!(frame.w >= 1.0);
    for point in &fractions {
        let slide_x = frame.x + point.x * frame.w;
        assert!((slide_x - 30.0).abs() < 0.001);
    }
}

#[test]
fn path_validation_refuses_hostile_points_and_shapes() {
    let mut path = default_path();
    path.points = vec![crate::PathPoint { x: 0.0, y: 0.0 }];
    let e = errors(&course_with_media(crate::ObjectKind::Path {
        path: path.clone(),
    }));
    assert!(e.iter().any(|m| m.contains("at least")), "{e:?}");

    path.points = (0..=crate::PATH_MAX_POINTS)
        .map(|i| crate::PathPoint {
            #[allow(clippy::cast_precision_loss)]
            x: i as f32 / crate::PATH_MAX_POINTS as f32,
            y: 0.5,
        })
        .collect();
    let e = errors(&course_with_media(crate::ObjectKind::Path {
        path: path.clone(),
    }));
    assert!(e.iter().any(|m| m.contains("at most")), "{e:?}");

    path = default_path();
    path.points[0].x = 1.5;
    let e = errors(&course_with_media(crate::ObjectKind::Path {
        path: path.clone(),
    }));
    assert!(e.iter().any(|m| m.contains("fractions")), "{e:?}");

    path = default_path();
    path.fill = Some(crate::Color("#2F6FEB".into()));
    let e = errors(&course_with_media(crate::ObjectKind::Path {
        path: path.clone(),
    }));
    assert!(e.iter().any(|m| m.contains("only have a fill")), "{e:?}");

    path = default_path();
    path.connector = Some(crate::ConnectorStyle::Elbow);
    path.points.push(crate::PathPoint { x: 0.5, y: 0.5 });
    let e = errors(&course_with_media(crate::ObjectKind::Path { path }));
    assert!(e.is_empty(), "a routed connector keeps its polyline: {e:?}");
}

#[test]
fn path_defaults_are_omitted_and_closed_fill_round_trips() {
    let mut path = default_path();
    path.closed = true;
    path.fill = Some(crate::Color("#2F6FEB".into()));
    path.end_end = crate::LineEnd::Arrow;
    let course = course_with_media(crate::ObjectKind::Path { path });
    assert_eq!(errors(&course), Vec::<String>::new());
    let saved = course.to_json().unwrap();
    assert!(saved.contains("\"closed\": true"));
    assert!(saved.contains("\"fill\""));
    assert!(saved.contains("\"end_end\": \"arrow\""));
    assert!(!saved.contains("\"start_end\""));
    assert!(!saved.contains("\"connector\""));
    assert_eq!(Course::from_json(&saved).unwrap(), course);
}

#[test]
fn rotation_defaults_are_omitted_and_normalised() {
    let course = course_with_media(crate::ObjectKind::Path {
        path: default_path(),
    });
    let saved = course.to_json().unwrap();
    assert!(!saved.contains("\"rotation\""));
    assert!(!saved.contains("\"flip_h\""));
    assert!(!saved.contains("\"flip_v\""));

    let mut value: serde_json::Value = serde_json::from_str(&saved).unwrap();
    let object = &mut value["scenes"][0]["slides"][0]["objects"][0];
    object["rotation"] = serde_json::json!(-30);
    object["flip_h"] = serde_json::json!(true);
    let loaded = Course::from_json(&serde_json::to_string(&value).unwrap()).unwrap();
    let object = &loaded.scenes[0].slides[0].objects[0];
    assert!(
        (object.rotation - 330.0).abs() < 0.001,
        "{:?}",
        object.rotation
    );
    assert!(object.flip_h);
    assert!(!object.flip_v);

    assert!((crate::normalize_rotation(370.0) - 10.0).abs() < 0.001);
    assert!((crate::normalize_rotation(-90.0) - 270.0).abs() < 0.001);
    assert_eq!(crate::normalize_rotation(f32::NAN), 0.0);
    assert_eq!(crate::normalize_rotation(f32::INFINITY), 0.0);
    assert_eq!(crate::normalize_rotation(360.0), 0.0);
}

#[test]
fn connector_glue_is_optional_and_sites_wrap() {
    let mut path = default_path();
    path.connector = Some(crate::ConnectorStyle::Straight);
    path.start_glue = Some(crate::Glue {
        object: "shape1".into(),
        site: 7,
    });
    path.end_glue = Some(crate::Glue {
        object: "shape2".into(),
        site: 2,
    });
    let course = course_with_media(crate::ObjectKind::Path { path });
    assert_eq!(errors(&course), Vec::<String>::new());
    let saved = course.to_json().unwrap();
    assert!(saved.contains("\"start_glue\""));
    assert!(saved.contains("\"end_glue\""));
    let loaded = Course::from_json(&saved).unwrap();
    assert_eq!(loaded, course);
    let path = loaded.scenes[0].slides[0]
        .objects
        .iter()
        .find_map(|object| match &object.kind {
            crate::ObjectKind::Path { path } => Some(path),
            _ => None,
        })
        .expect("expected a path");
    assert_eq!(path.start_glue.as_ref().map(crate::Glue::site), Some(3));

    let mut free = default_path();
    free.connector = Some(crate::ConnectorStyle::Straight);
    let saved = course_with_media(crate::ObjectKind::Path { path: free })
        .to_json()
        .unwrap();
    assert!(!saved.contains("start_glue"));
    assert!(!saved.contains("end_glue"));
}

#[test]
fn opacity_and_effects_round_trip_and_old_files_omit_them() {
    let old = Course::from_json(HELLO).unwrap();
    let saved = old.to_json().unwrap();
    assert!(!saved.contains("\"opacity\""));
    assert!(!saved.contains("\"shadow\""));
    assert!(!saved.contains("\"glow\""));
    assert!(!saved.contains("\"fill_style\""));
    assert_eq!(Course::from_json(&saved).unwrap(), old);

    let mut course = course_with_media(crate::ObjectKind::Shape {
        shape: crate::ShapeKind::Ellipse,
        fill: crate::Color("#112233".into()),
        fill_style: Some(crate::FillStyle::Linear {
            angle: 30.0,
            stops: vec![
                crate::GradientStop {
                    color: crate::Color("#FF0000".into()),
                    position: 0.0,
                },
                crate::GradientStop {
                    color: crate::Color("#0000FF".into()),
                    position: 1.0,
                },
            ],
        }),
        outline: None,
        text: None,
        adjust: Vec::new(),
    });
    if let Some(object) = course.scenes[0].slides[0].objects.last_mut() {
        object.opacity = 0.5;
        object.shadow = Some(crate::Shadow {
            color: crate::Color("#00000080".into()),
            offset_x: 8.0,
            offset_y: 6.0,
            blur: 12.0,
        });
        object.glow = Some(crate::Glow {
            color: crate::Color("#2F6FEBCC".into()),
            size: 10.0,
        });
    }
    let saved = course.to_json().unwrap();
    assert!(saved.contains("\"opacity\""));
    assert!(saved.contains("\"shadow\""));
    assert!(saved.contains("\"glow\""));
    assert!(saved.contains("\"fill_style\""));
    let loaded = Course::from_json(&saved).unwrap();
    assert_eq!(loaded, course);
}

#[test]
fn opacity_effects_and_gradient_stops_are_repaired_on_load() {
    let mut course = course_with_media(crate::ObjectKind::Shape {
        shape: crate::ShapeKind::Rectangle,
        fill: crate::Color("#000000".into()),
        fill_style: Some(crate::FillStyle::Linear {
            angle: -30.0,
            stops: vec![
                crate::GradientStop {
                    color: crate::Color("#FFFFFF".into()),
                    position: 2.0,
                },
                crate::GradientStop {
                    color: crate::Color("#000000".into()),
                    position: -1.0,
                },
                crate::GradientStop {
                    color: crate::Color("#FF0000".into()),
                    position: 0.5,
                },
            ],
        }),
        outline: None,
        text: None,
        adjust: Vec::new(),
    });
    if let Some(object) = course.scenes[0].slides[0].objects.last_mut() {
        object.opacity = 4.0;
        object.shadow = Some(crate::Shadow {
            color: crate::Color("#000000".into()),
            offset_x: 999.0,
            offset_y: -999.0,
            blur: 500.0,
        });
        object.glow = Some(crate::Glow {
            color: crate::Color("#FFFFFF".into()),
            size: 500.0,
        });
    }

    let reparsed = Course::from_json(&course.to_json().unwrap()).unwrap();
    let object = reparsed.scenes[0].slides[0]
        .objects
        .last()
        .expect("shape object");
    assert!((object.opacity - 1.0).abs() < f32::EPSILON);
    let shadow = object.shadow.as_ref().expect("shadow");
    assert!((shadow.offset_x - 200.0).abs() < f32::EPSILON);
    assert!((shadow.offset_y + 200.0).abs() < f32::EPSILON);
    assert!((shadow.blur - 100.0).abs() < f32::EPSILON);
    let glow = object.glow.as_ref().expect("glow");
    assert!((glow.size - 100.0).abs() < f32::EPSILON);
    let crate::ObjectKind::Shape {
        fill_style: Some(crate::FillStyle::Linear { angle, stops }),
        ..
    } = &object.kind
    else {
        panic!("expected a linear fill");
    };
    assert!((*angle - 330.0).abs() < f32::EPSILON);
    let positions: Vec<f32> = stops.iter().map(|stop| stop.position).collect();
    assert_eq!(positions, vec![0.0, 0.5, 1.0]);
}

#[test]
fn paragraph_and_link_serde_defaults_and_omission() {
    let paragraph: crate::Paragraph =
        serde_json::from_str(r#"{"list":"bullet","level":2}"#).expect("paragraph");
    assert_eq!(paragraph.list, crate::ListKind::Bullet);
    assert_eq!(paragraph.level, 2);
    assert_eq!(paragraph.line_spacing, 1.0);
    assert_eq!(paragraph.space_before, 0.0);
    assert_eq!(paragraph.space_after, 0.0);
    assert_eq!(
        serde_json::to_string(&crate::Paragraph::default()).expect("json"),
        "{}"
    );

    let plain = crate::TextRun {
        text: "plain".into(),
        bold: None,
        italic: None,
        underline: None,
        size: None,
        color: None,
        link: None,
        font: None,
        strikethrough: None,
        baseline: None,
        letter_spacing: None,
    };
    assert!(
        !serde_json::to_string(&plain)
            .expect("json")
            .contains("link")
    );

    let linked: crate::TextRun =
        serde_json::from_str(r#"{"text":"go","link":{"type":"url","url":"https://example.com"}}"#)
            .expect("linked run");
    assert_eq!(
        linked.link,
        Some(crate::Link::Url {
            url: "https://example.com".into()
        })
    );

    let kind = crate::ObjectKind::Text {
        text: "Line".into(),
        style: crate::TextStyle::default(),
        runs: Vec::new(),
        paragraphs: Vec::new(),
        autofit: crate::Autofit::None,
        text_style: None,
    };
    let json = serde_json::to_value(&kind).expect("kind json");
    assert!(json.get("paragraphs").is_none());
    assert!(json.get("autofit").is_none());

    let with_paragraphs = crate::ObjectKind::Text {
        text: "Line".into(),
        style: crate::TextStyle::default(),
        runs: Vec::new(),
        paragraphs: vec![crate::Paragraph {
            list: crate::ListKind::Number,
            ..crate::Paragraph::default()
        }],
        autofit: crate::Autofit::Shrink,
        text_style: None,
    };
    let json = serde_json::to_value(&with_paragraphs).expect("kind json");
    assert_eq!(json["paragraphs"][0]["list"], "number");
    assert_eq!(json["autofit"], "shrink");
}

#[test]
fn editing_newlines_keeps_paragraph_styles_aligned() {
    use crate::text::align_paragraphs;

    let styles = vec![
        crate::Paragraph {
            list: crate::ListKind::Bullet,
            ..crate::Paragraph::default()
        },
        crate::Paragraph {
            list: crate::ListKind::Number,
            level: 1,
            ..crate::Paragraph::default()
        },
    ];

    // Split the first paragraph: both halves keep its style.
    let split = align_paragraphs("onetwo\nthree", &styles, "one\ntwo\nthree");
    assert_eq!(split.len(), 3);
    assert_eq!(split[0].list, crate::ListKind::Bullet);
    assert_eq!(split[1].list, crate::ListKind::Bullet);
    assert_eq!(split[2].list, crate::ListKind::Number);

    // Split at a paragraph boundary: the new line copies the paragraph it
    // starts from.
    let boundary = align_paragraphs("one\ntwo", &styles, "one\nnew\ntwo");
    assert_eq!(boundary.len(), 3);
    assert_eq!(boundary[0].list, crate::ListKind::Bullet);
    assert_eq!(boundary[1].list, crate::ListKind::Number);
    assert_eq!(boundary[2].list, crate::ListKind::Number);

    // Joining keeps the first paragraph's style.
    let joined = align_paragraphs("one\ntwo", &styles, "onetwo");
    assert_eq!(joined.len(), 1);
    assert_eq!(joined[0].list, crate::ListKind::Bullet);
}

#[test]
fn fonts_are_omitted_when_empty_and_round_trip() {
    let course = Course::from_json(HELLO).unwrap();
    let json = course.to_json().unwrap();
    assert!(!json.contains("\"fonts\""), "empty fonts must be omitted");

    let mut with_font = course;
    with_font.fonts.push(crate::FontFace {
        family: "Lato".into(),
        style: crate::FontStyle::BoldItalic,
        src: "fonts/lato-bold-italic.ttf".into(),
    });
    let json = with_font.to_json().unwrap();
    assert!(json.contains("\"bold_italic\""));
    let parsed = Course::from_json(&json).unwrap();
    assert_eq!(parsed.fonts, with_font.fonts);
}

#[test]
fn font_validation_rejects_duplicates_too_many_and_bad_names() {
    let mut course = Course::from_json(HELLO).unwrap();
    course.fonts.push(crate::FontFace {
        family: "Lato".into(),
        style: crate::FontStyle::Regular,
        src: "fonts/lato.ttf".into(),
    });
    course.fonts.push(crate::FontFace {
        family: "lato".into(),
        style: crate::FontStyle::Regular,
        src: "fonts/lato-2.ttf".into(),
    });
    let issues = course.validate();
    assert!(
        issues
            .iter()
            .any(|i| i.severity == crate::Severity::Error && i.message.contains("duplicate font")),
        "{issues:#?}"
    );

    let mut course = Course::from_json(HELLO).unwrap();
    for i in 0..=crate::MAX_FONT_FACES {
        course.fonts.push(crate::FontFace {
            family: format!("Family{i}"),
            style: crate::FontStyle::Regular,
            src: format!("fonts/font{i}.ttf"),
        });
    }
    let issues = course.validate();
    assert!(
        issues
            .iter()
            .any(|i| i.severity == crate::Severity::Error && i.message.contains("at most")),
        "{issues:#?}"
    );

    let mut course = Course::from_json(HELLO).unwrap();
    course.fonts.push(crate::FontFace {
        family: "Bad\u{7}Name".into(),
        style: crate::FontStyle::Regular,
        src: "fonts/not-a-font.png".into(),
    });
    let issues = course.validate();
    assert!(issues.iter().any(|i| i.message.contains("control")));
    assert!(issues.iter().any(|i| i.message.contains(".ttf or .otf")));
}

#[test]
fn unknown_font_names_warn_and_known_course_families_do_not() {
    let mut course = Course::from_json(HELLO).unwrap();
    course.theme.font = "Lato".into();
    let issues = course.validate();
    assert!(
        issues.iter().any(|i| i.severity == crate::Severity::Warning
            && i.path == "/theme/font"
            && i.message.contains("Inter is used instead")),
        "{issues:#?}"
    );

    course.fonts.push(crate::FontFace {
        family: "Lato".into(),
        style: crate::FontStyle::Regular,
        src: "fonts/lato.ttf".into(),
    });
    let issues = course.validate();
    assert!(
        !issues
            .iter()
            .any(|i| i.path == "/theme/font" && i.message.contains("Inter is used instead")),
        "{issues:#?}"
    );
}

#[test]
fn links_are_validated_by_scheme_and_slide() {
    let mut course = Course::from_json(HELLO).unwrap();
    let make_text = |link: crate::Link| crate::ObjectKind::Text {
        text: "Go".into(),
        style: crate::TextStyle::default(),
        runs: vec![crate::TextRun {
            text: "Go".into(),
            bold: None,
            italic: None,
            underline: None,
            size: None,
            color: None,
            link: Some(link),
            font: None,
            strikethrough: None,
            baseline: None,
            letter_spacing: None,
        }],
        paragraphs: Vec::new(),
        autofit: crate::Autofit::None,
        text_style: None,
    };
    course.scenes[0].slides[0].objects[0].kind = make_text(crate::Link::Url {
        url: "javascript:alert(1)".into(),
    });
    let issues = course.validate();
    assert!(
        issues.iter().any(|issue| issue.message.contains("http://")),
        "{issues:#?}"
    );

    course.scenes[0].slides[0].objects[0].kind = make_text(crate::Link::Slide {
        slide: "missing".into(),
    });
    let issues = course.validate();
    assert!(
        issues
            .iter()
            .any(|issue| issue.message.contains("does not exist")),
        "{issues:#?}"
    );

    course.scenes[0].slides[0].objects[0].kind = make_text(crate::Link::Slide {
        slide: "s-tabs".into(),
    });
    assert!(course.is_playable());
}

fn table_cell(text: &str) -> crate::TableCell {
    crate::TableCell {
        text: text.into(),
        ..crate::TableCell::default()
    }
}

fn table_kind(columns: Vec<f32>, rows: Vec<crate::TableRow>) -> crate::ObjectKind {
    crate::ObjectKind::Table {
        columns,
        rows,
        borders: crate::TableBorders::default(),
        header_row: false,
    }
}

#[test]
fn table_fractions_are_repaired_on_load() {
    let object: crate::Object = serde_json::from_value(serde_json::json!({
        "id": "table1",
        "kind": {
            "type": "table",
            "columns": [2.0, 2.0],
            "rows": [
                {
                    "height": 3.0,
                    "cells": [
                        { "text": "A" },
                        { "text": "B" }
                    ]
                },
                {
                    "height": 1.0,
                    "cells": [
                        { "text": "C" },
                        { "text": "D" }
                    ]
                }
            ]
        },
        "frame": { "x": 0, "y": 0, "w": 400, "h": 200 }
    }))
    .expect("table object");
    let crate::ObjectKind::Table { columns, rows, .. } = &object.kind else {
        panic!("expected a table");
    };
    assert!((columns.iter().sum::<f32>() - 1.0).abs() < 1e-5);
    assert!((columns[0] - 0.5).abs() < 1e-5);
    assert!((rows.iter().map(|row| row.height).sum::<f32>() - 1.0).abs() < 1e-5);
    assert!((rows[0].height - 0.75).abs() < 1e-5);
    assert!((rows[1].height - 0.25).abs() < 1e-5);
}

#[test]
fn table_fraction_repair_handles_non_finite_values() {
    let mut values = vec![f32::NAN, f32::INFINITY, 2.0];
    crate::normalize_table_fractions(&mut values);
    assert!(values.iter().all(|value| value.is_finite()));
    assert!((values.iter().sum::<f32>() - 1.0).abs() < 1e-5);
}

#[test]
fn table_validation_rejects_spans_past_the_grid() {
    let mut course = Course::from_json(HELLO).unwrap();
    let mut cell = table_cell("wide");
    cell.col_span = 3;
    course.scenes[0].slides[0].objects[0].kind = table_kind(
        vec![0.5, 0.5],
        vec![crate::TableRow {
            height: 1.0,
            cells: vec![cell, table_cell("")],
        }],
    );
    let issues = course.validate();
    assert!(
        issues
            .iter()
            .any(|issue| issue.message.contains("column span runs past")),
        "{issues:#?}"
    );
}

#[test]
fn table_validation_accepts_a_merged_header_table() {
    let mut course = Course::from_json(HELLO).unwrap();
    let mut header = table_cell("Merged");
    header.col_span = 2;
    course.scenes[0].slides[0].objects[0].kind = crate::ObjectKind::Table {
        columns: vec![0.4, 0.6],
        rows: vec![
            crate::TableRow {
                height: 0.4,
                cells: vec![header, table_cell("")],
            },
            crate::TableRow {
                height: 0.6,
                cells: vec![table_cell("A"), table_cell("B")],
            },
        ],
        borders: crate::TableBorders::default(),
        header_row: true,
    };
    assert!(course.is_playable(), "{:#?}", course.validate());
}

#[test]
fn run_javascript_action_uses_its_spec_name_in_json() {
    let action: crate::Action =
        serde_json::from_str(r#"{"action":"run_javascript","code":"1"}"#).unwrap();
    assert!(matches!(action, crate::Action::RunJavaScript { .. }));
    let json = serde_json::to_string(&action).unwrap();
    assert!(json.contains("\"run_javascript\""), "{json}");
}

#[test]
fn print_slide_action_uses_its_spec_name_and_validates_anywhere() {
    let action: crate::Action = serde_json::from_str(r#"{"action":"print_slide"}"#).unwrap();
    assert_eq!(action, crate::Action::PrintSlide);
    assert_eq!(
        serde_json::to_string(&action).unwrap(),
        r#"{"action":"print_slide"}"#
    );

    let mut c = Course::from_json(HELLO).unwrap();
    c.scenes[0].slides[0].triggers.push(Trigger {
        action: crate::Action::PrintSlide,
        when: Event::TimelineStarts { layer: None },
        condition: None,
        disabled: false,
    });
    assert!(c.is_playable(), "{:#?}", c.validate());
}

#[test]
fn input_objects_validate_variable_kinds_and_ranges() {
    use crate::{ObjectKind, RadioOption, Value, VariableDef};

    let mut c = Course::from_json(HELLO).unwrap();
    c.variables.push(VariableDef {
        name: "input_text".into(),
        value: Value::Text(String::new()),
    });
    c.variables.push(VariableDef {
        name: "input_number".into(),
        value: Value::Number(0.0),
    });
    c.variables.push(VariableDef {
        name: "input_bool".into(),
        value: Value::Bool(false),
    });
    c.scenes[0].slides[0].objects[0].kind = ObjectKind::TextEntry {
        variable: "input_text".into(),
        placeholder: "Name".into(),
        multiline: false,
        max_chars: 10,
    };
    assert!(errors(&c).is_empty(), "{:#?}", errors(&c));

    // Wrong variable kind is an error.
    c.scenes[0].slides[0].objects[0].kind = ObjectKind::Checkbox {
        variable: "input_number".into(),
        label: "Agree".into(),
    };
    assert!(errors(&c).iter().any(|e| e.contains("needs Bool")));

    // A missing variable is an error.
    c.scenes[0].slides[0].objects[0].kind = ObjectKind::Slider {
        variable: "nope".into(),
        min: 0.0,
        max: 10.0,
        step: 1.0,
        orientation: crate::Orientation::Horizontal,
    };
    assert!(errors(&c).iter().any(|e| e.contains("does not exist")));

    c.scenes[0].slides[0].objects[0].kind = ObjectKind::RadioGroup {
        variable: "input_text".into(),
        options: vec![RadioOption {
            label: "Only".into(),
            value: "one".into(),
        }],
    };
    assert!(errors(&c).iter().any(|e| e.contains("2 to 20 options")));
}

#[test]
fn slider_dial_and_text_entry_ranges_are_checked() {
    use crate::{ObjectKind, Value, VariableDef};

    let mut c = Course::from_json(HELLO).unwrap();
    c.variables.push(VariableDef {
        name: "input_number".into(),
        value: Value::Number(0.0),
    });
    c.variables.push(VariableDef {
        name: "input_text".into(),
        value: Value::Text(String::new()),
    });
    c.scenes[0].slides[0].objects[0].kind = ObjectKind::Slider {
        variable: "input_number".into(),
        min: 10.0,
        max: 10.0,
        step: -1.0,
        orientation: crate::Orientation::Vertical,
    };
    let e = errors(&c);
    assert!(e.iter().any(|m| m.contains("min must be less")), "{e:?}");
    assert!(e.iter().any(|m| m.contains("at least 0")), "{e:?}");

    c.scenes[0].slides[0].objects[0].kind = ObjectKind::Dial {
        variable: "input_number".into(),
        min: 0.0,
        max: 10.0,
        step: 1.0,
        start_angle: 90.0,
        end_angle: -90.0,
    };
    assert!(
        errors(&c)
            .iter()
            .any(|m| m.contains("start_angle must be less")),
        "{:#?}",
        errors(&c)
    );

    c.scenes[0].slides[0].objects[0].kind = ObjectKind::TextEntry {
        variable: "input_text".into(),
        placeholder: String::new(),
        multiline: true,
        max_chars: crate::MAX_INPUT_CHARS + 1,
    };
    assert!(errors(&c).iter().any(|m| m.contains("max_chars")));
}

#[test]
fn input_string_locations_resolve_for_find_and_translate() {
    use crate::{ObjectKind, RadioOption};

    let mut c = Course::from_json(HELLO).unwrap();
    let object_id = {
        let object = &mut c.scenes[0].slides[0].objects[0];
        object.kind = ObjectKind::TextEntry {
            variable: "text".into(),
            placeholder: "Type here".into(),
            multiline: false,
            max_chars: 0,
        };
        object.description = None;
        object.id.clone()
    };
    let fields: Vec<String> = crate::course_strings(&c)
        .into_iter()
        .filter(|item| item.location.object.as_deref() == Some(object_id.as_str()))
        .map(|item| item.location.field)
        .collect();
    assert!(
        fields.iter().any(|field| field == "placeholder"),
        "{fields:?}"
    );
    for field in &fields {
        let location = crate::Location {
            slide: Some(c.scenes[0].slides[0].id.clone()),
            object: Some(c.scenes[0].slides[0].objects[0].id.clone()),
            layer: None,
            field: field.clone(),
        };
        assert!(
            crate::string_mut(&mut c, &location).is_some(),
            "string_mut must resolve {field}"
        );
    }

    c.scenes[0].slides[0].objects[0].kind = ObjectKind::RadioGroup {
        variable: "text".into(),
        options: vec![
            RadioOption {
                label: "First".into(),
                value: "first".into(),
            },
            RadioOption {
                label: "Second".into(),
                value: "second".into(),
            },
        ],
    };
    let fields: Vec<String> = crate::course_strings(&c)
        .into_iter()
        .filter(|item| item.location.object.as_deref() == Some(object_id.as_str()))
        .map(|item| item.location.field)
        .collect();
    assert!(
        fields.iter().any(|field| field == "options/1/label"),
        "{fields:?}"
    );
    for field in &fields {
        let location = crate::Location {
            slide: Some(c.scenes[0].slides[0].id.clone()),
            object: Some(c.scenes[0].slides[0].objects[0].id.clone()),
            layer: None,
            field: field.clone(),
        };
        assert!(
            crate::string_mut(&mut c, &location).is_some(),
            "string_mut must resolve {field}"
        );
    }
}

#[test]
fn marker_and_scroll_panel_defaults_round_trip() {
    let marker: crate::ObjectKind =
        serde_json::from_str(r#"{"type":"marker"}"#).expect("a marker with defaults parses");
    let crate::ObjectKind::Marker {
        title,
        body,
        icon,
        color,
        ..
    } = &marker
    else {
        panic!("marker");
    };
    assert!(title.is_empty() && body.is_empty());
    assert_eq!(*icon, crate::MarkerIcon::Info);
    assert!(color.is_none());
    let saved = serde_json::to_string(&marker).expect("serialise");
    assert!(!saved.contains("icon"), "{saved}");
    assert!(!saved.contains("color"), "{saved}");

    let panel: crate::ObjectKind =
        serde_json::from_str(r#"{"type":"scroll_panel","content_height":900}"#)
            .expect("a panel parses");
    let crate::ObjectKind::ScrollPanel {
        content_height,
        bar,
    } = &panel
    else {
        panic!("panel");
    };
    assert!((*content_height - 900.0).abs() < 1e-6);
    assert_eq!(*bar, crate::ScrollBar::Visible);
    let saved = serde_json::to_string(&panel).expect("serialise");
    assert!(!saved.contains("\"bar\""), "{saved}");
}

#[test]
fn scroll_panel_content_height_is_repaired_on_load() {
    let kind: crate::ObjectKind =
        serde_json::from_str(r#"{"type":"scroll_panel","content_height":1e39}"#)
            .expect("a hostile panel still parses");
    let crate::ObjectKind::ScrollPanel { content_height, .. } = kind else {
        panic!("panel");
    };
    assert!(content_height.is_finite());
    assert!(content_height <= crate::MAX_SCROLL_CONTENT_HEIGHT);

    let kind: crate::ObjectKind =
        serde_json::from_str(r#"{"type":"scroll_panel","content_height":-5}"#)
            .expect("a negative panel still parses");
    let crate::ObjectKind::ScrollPanel { content_height, .. } = kind else {
        panic!("panel");
    };
    assert!(content_height >= 0.0);
}

#[test]
fn marker_title_and_body_are_findable_and_replaceable() {
    let mut c = Course::from_json(
        &serde_json::json!({
            "schema_version": 1,
            "id": "marker-strings",
            "title": "Marker strings",
            "scenes": [{ "id": "sc", "title": "Scene", "slides": [{
                "id": "s1",
                "title": "Slide",
                "objects": [{
                    "id": "m1",
                    "kind": { "type": "marker", "title": "Title", "body": "Body" },
                    "frame": { "x": 0.0, "y": 0.0, "w": 32.0, "h": 32.0 }
                }]
            }] }]
        })
        .to_string(),
    )
    .expect("marker course");
    let items: Vec<crate::StringItem> = crate::course_strings(&c)
        .into_iter()
        .filter(|item| item.location.object.as_deref() == Some("m1"))
        .collect();
    let fields: Vec<&str> = items
        .iter()
        .map(|item| item.location.field.as_str())
        .collect();
    assert!(fields.contains(&"title"), "{fields:?}");
    assert!(fields.contains(&"body"), "{fields:?}");

    for item in &items {
        let location = item.location.clone();
        let Some(crate::StringTarget::Plain(text)) = crate::string_mut(&mut c, &location) else {
            continue;
        };
        if text == "Title" {
            *text = "New title".to_owned();
        } else if text == "Body" {
            *text = "New body".to_owned();
        }
    }
    let object = c
        .slide("s1")
        .and_then(|slide| slide.object("m1"))
        .map(|(object, _)| object.kind.clone())
        .expect("marker");
    assert!(matches!(
        object,
        crate::ObjectKind::Marker { title, body, .. }
            if title == "New title" && body == "New body"
    ));
}

#[test]
fn panel_membership_and_nesting_are_validated() {
    use crate::{ObjectKind, Rect, ScrollBar};

    let mut c = Course::from_json(HELLO).unwrap();
    {
        let slide = &mut c.scenes[0].slides[0];
        slide.objects.clear();
        slide.triggers.clear();
        slide.objects.push(crate::Object {
            id: "panel".into(),
            name: String::new(),
            kind: ObjectKind::ScrollPanel {
                content_height: 1200.0,
                bar: ScrollBar::Visible,
            },
            frame: Rect {
                x: 100.0,
                y: 100.0,
                w: 300.0,
                h: 400.0,
            },
            timing: Default::default(),
            states: Vec::new(),
            initial_state: "normal".into(),
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
        });
        let mut child = slide.objects[0].clone();
        child.id = "child".into();
        child.kind = ObjectKind::Text {
            text: "Child".into(),
            style: Default::default(),
            runs: Vec::new(),
            paragraphs: Vec::new(),
            autofit: Default::default(),
            text_style: None,
        };
        child.panel = Some("panel".into());
        slide.objects.push(child);
    }

    assert!(errors(&c).is_empty(), "{:#?}", errors(&c));

    c.scenes[0].slides[0].objects[1].panel = Some("missing".into());
    assert!(
        errors(&c)
            .iter()
            .any(|message| message.contains("not a scrolling panel")),
        "{:#?}",
        errors(&c)
    );

    c.scenes[0].slides[0].objects[1].panel = Some("panel".into());
    c.scenes[0].slides[0].objects[0].panel = Some("panel".into());
    assert!(
        errors(&c)
            .iter()
            .any(|message| message.contains("cannot be inside another")),
        "{:#?}",
        errors(&c)
    );

    c.scenes[0].slides[0].objects[0].panel = None;
    if let ObjectKind::ScrollPanel { content_height, .. } =
        &mut c.scenes[0].slides[0].objects[0].kind
    {
        *content_height = 20_001.0;
    }
    assert!(
        errors(&c)
            .iter()
            .any(|message| message.contains("content_height")),
        "{:#?}",
        errors(&c)
    );
}

#[test]
fn dial_angles_and_slider_numbers_are_repaired_on_load() {
    let kind: crate::ObjectKind = serde_json::from_str(
        r#"{"type":"dial","variable":"n","min":0,"max":10,"start_angle":1e39,"end_angle":-1e39}"#,
    )
    .expect("a hostile dial still parses");
    let crate::ObjectKind::Dial {
        start_angle,
        end_angle,
        ..
    } = kind
    else {
        panic!("dial");
    };
    assert!(start_angle.is_finite() && end_angle.is_finite());

    assert_eq!(crate::snap_to_step(0.0, 10.0, 2.0, 7.4), 8.0);
    assert_eq!(crate::snap_to_step(0.0, 10.0, 0.0, 7.4), 7.4);
    assert_eq!(crate::snap_to_step(0.0, 10.0, 2.0, 99.0), 10.0);
    assert_eq!(crate::snap_to_step(0.0, 10.0, 2.0, -99.0), 0.0);
}

#[test]
fn key_pressed_events_accept_only_the_listed_keys() {
    let mut c = Course::from_json(HELLO).unwrap();
    {
        let slide = c.slide_mut("s-intro").unwrap();
        slide.triggers.push(Trigger {
            action: crate::Action::JumpToSlide {
                target: crate::SlideTarget::Next,
            },
            when: Event::KeyPressed {
                key: "arrow_up".into(),
                object: None,
            },
            condition: None,
            disabled: false,
        });
    }
    assert!(errors(&c).is_empty(), "{:?}", errors(&c));

    c.slide_mut("s-intro")
        .unwrap()
        .triggers
        .last_mut()
        .unwrap()
        .when = Event::KeyPressed {
        key: "F1".into(),
        object: None,
    };
    assert!(
        errors(&c).iter().any(|e| e.contains("invalid key")),
        "{:?}",
        errors(&c)
    );

    c.slide_mut("s-intro")
        .unwrap()
        .triggers
        .last_mut()
        .unwrap()
        .when = Event::KeyPressed {
        key: "A".into(),
        object: None,
    };
    assert!(
        errors(&c).iter().any(|e| e.contains("invalid key")),
        "keys are lower-case names: {:?}",
        errors(&c)
    );
}

#[test]
fn drag_specs_reject_unknown_targets_and_question_widgets() {
    let mut c = Course::from_json(HELLO).unwrap();
    {
        let slide = c.slide_mut("s-intro").unwrap();
        let object = slide
            .objects
            .iter_mut()
            .find(|o| o.id == "btn-start")
            .expect("button");
        object.drag = Some(crate::DragSpec {
            targets: vec!["missing".into()],
            ..crate::DragSpec::default()
        });
    }
    assert!(
        errors(&c).iter().any(|e| e.contains("target \"missing\"")),
        "{:?}",
        errors(&c)
    );

    c.slide_mut("s-intro")
        .unwrap()
        .objects
        .iter_mut()
        .find(|o| o.id == "btn-start")
        .unwrap()
        .drag = Some(crate::DragSpec::default());
    assert!(errors(&c).is_empty(), "{:?}", errors(&c));

    let drag_question = serde_json::json!({
        "schema_version": 1,
        "id": "drag-validation",
        "title": "Drag validation",
        "scenes": [{
            "id": "scene",
            "title": "Scene",
            "slides": [{
                "id": "s1",
                "title": "Slide",
                "objects": [
                    { "id": "item", "kind": { "type": "shape", "shape": "rectangle", "fill": "#112233" },
                      "frame": { "x": 0, "y": 0, "w": 40, "h": 40 },
                      "drag": { "targets": ["box"] } },
                    { "id": "box", "kind": { "type": "shape", "shape": "rectangle", "fill": "#334455" },
                      "frame": { "x": 200, "y": 0, "w": 80, "h": 80 } }
                ],
                "question": {
                    "kind": "drag_drop",
                    "prompt": "Sort",
                    "choices": [],
                    "drag_items": [{ "item": "item", "target": "box" }],
                    "drop_targets": ["box"]
                }
            }]
        }]
    });
    let c = Course::from_json(&drag_question.to_string()).expect("parses");
    assert!(
        errors(&c)
            .iter()
            .any(|e| e.contains("not allowed on question widget")),
        "{:?}",
        errors(&c)
    );
}

#[test]
fn drag_specs_and_new_events_serialize_with_the_documented_names() {
    let mut c = Course::from_json(HELLO).unwrap();
    let slide = c.slide_mut("s-intro").unwrap();
    slide.objects[0].drag = Some(crate::DragSpec {
        targets: Vec::new(),
        snap: true,
        return_on_miss: true,
    });
    let json = c.to_json().unwrap();
    assert!(!json.contains("\"targets\""), "empty targets are omitted");
    assert!(!json.contains("\"snap\""), "default snap is omitted");
    assert!(
        !json.contains("\"return_on_miss\""),
        "default return is omitted"
    );

    let events = [
        (
            Event::KeyPressed {
                key: "enter".into(),
                object: Some("title".into()),
            },
            "key_pressed",
        ),
        (
            Event::DroppedOn {
                object: "a".into(),
                target: "b".into(),
            },
            "dropped_on",
        ),
        (
            Event::ObjectsIntersect {
                a: "a".into(),
                b: "b".into(),
            },
            "objects_intersect",
        ),
        (
            Event::ObjectsStopIntersecting {
                a: "a".into(),
                b: "b".into(),
            },
            "objects_stop_intersecting",
        ),
    ];
    for (event, name) in events {
        let value = serde_json::to_value(&event).unwrap();
        assert_eq!(value["event"], name, "{value}");
    }
}

fn timer_meter_course() -> Course {
    Course::from_json(
        &serde_json::json!({
            "schema_version": 1,
            "id": "timer-meter",
            "title": "Timer and meter",
            "variables": [
                { "name": "score", "value": 0 },
                { "name": "label", "value": "hi" }
            ],
            "scenes": [{ "id": "sc", "title": "Scene", "slides": [{
                "id": "s1",
                "title": "Slide",
                "objects": [
                    { "id": "t1", "kind": { "type": "timer", "duration_ms": 90000 }, "frame": { "x": 0.0, "y": 0.0, "w": 200.0, "h": 60.0 } },
                    { "id": "m1", "kind": { "type": "progress_meter", "variable": "score", "min": 0, "max": 10 }, "frame": { "x": 0.0, "y": 80.0, "w": 200.0, "h": 40.0 } }
                ]
            }] }]
        })
        .to_string(),
    )
    .expect("timer/meter course")
}

#[test]
fn timer_and_meter_defaults_round_trip() {
    let c = timer_meter_course();
    let slide = &c.scenes[0].slides[0];
    let crate::ObjectKind::Timer {
        duration_ms,
        format,
        autostart,
        ..
    } = slide.objects[0].kind
    else {
        panic!("timer");
    };
    assert_eq!(duration_ms, 90_000);
    assert_eq!(format, crate::TimerFormat::MmSs);
    assert!(autostart);
    let crate::ObjectKind::ProgressMeter {
        style, show_value, ..
    } = slide.objects[1].kind
    else {
        panic!("meter");
    };
    assert_eq!(style, crate::MeterStyle::Bar);
    assert!(!show_value);
    let saved = c.to_json().unwrap();
    assert!(!saved.contains("\"format\""), "{saved}");
    assert!(!saved.contains("\"autostart\""), "{saved}");
    assert!(!saved.contains("\"style\": \"bar\""), "{saved}");
    assert!(saved.contains("\"progress_meter\""), "{saved}");
    assert_eq!(Course::from_json(&saved).unwrap(), c);
}

#[test]
fn timer_duration_is_repaired_on_load() {
    let kind: crate::ObjectKind =
        serde_json::from_str(r#"{"type":"timer","duration_ms":0}"#).expect("zero duration parses");
    let crate::ObjectKind::Timer { duration_ms, .. } = kind else {
        panic!("timer");
    };
    assert_eq!(duration_ms, 1);

    let kind: crate::ObjectKind =
        serde_json::from_str(r#"{"type":"timer","duration_ms":99999999999}"#)
            .expect("huge duration parses");
    let crate::ObjectKind::Timer { duration_ms, .. } = kind else {
        panic!("timer");
    };
    assert_eq!(duration_ms, crate::MAX_TIMER_DURATION_MS);

    let kind: crate::ObjectKind =
        serde_json::from_str(r#"{"type":"timer"}"#).expect("missing duration parses");
    let crate::ObjectKind::Timer { duration_ms, .. } = kind else {
        panic!("timer");
    };
    assert_eq!(duration_ms, crate::DEFAULT_TIMER_DURATION_MS);
}

#[test]
fn timer_text_formats_roll_hours_up() {
    use crate::TimerFormat;
    assert_eq!(crate::format_timer_text(90_000, TimerFormat::MmSs), "01:30");
    assert_eq!(
        crate::format_timer_text(5_400_000, TimerFormat::MmSs),
        "90:00"
    );
    assert_eq!(crate::format_timer_text(5_000, TimerFormat::MmSs), "00:05");
    assert_eq!(crate::format_timer_text(999, TimerFormat::MmSs), "00:01");
    assert_eq!(crate::format_timer_text(0, TimerFormat::MmSs), "00:00");
    assert_eq!(crate::format_timer_text(61_000, TimerFormat::Seconds), "61");
    assert_eq!(crate::format_timer_text(61_000, TimerFormat::Hidden), "");
    assert_eq!(crate::timer_remaining_seconds(1), 1);
    assert_eq!(crate::timer_remaining_seconds(1000), 1);
    assert_eq!(crate::timer_remaining_seconds(1001), 2);
}

#[test]
fn timer_and_meter_are_validated() {
    let mut c = timer_meter_course();
    assert!(errors(&c).is_empty(), "{:#?}", errors(&c));

    // A meter needs a number variable and a finite min < max.
    c.scenes[0].slides[0].objects[1].kind = crate::ObjectKind::ProgressMeter {
        variable: "label".into(),
        min: 10.0,
        max: 0.0,
        style: crate::MeterStyle::Ring,
        show_value: true,
    };
    let e = errors(&c);
    assert!(e.iter().any(|m| m.contains("is Text")), "{e:?}");
    assert!(e.iter().any(|m| m.contains("min must be less")), "{e:?}");

    c.scenes[0].slides[0].objects[1].kind = crate::ObjectKind::ProgressMeter {
        variable: "missing".into(),
        min: 0.0,
        max: 10.0,
        style: crate::MeterStyle::Bar,
        show_value: false,
    };
    assert!(errors(&c).iter().any(|m| m.contains("does not exist")));

    // Timer actions and the timer_ends event need a real timer.
    c.scenes[0].slides[0].triggers = vec![
        Trigger {
            action: crate::Action::StartTimer { timer: "m1".into() },
            when: Event::Click {
                object: "t1".into(),
            },
            condition: None,
            disabled: false,
        },
        Trigger {
            action: crate::Action::PauseTimer { timer: "t1".into() },
            when: Event::TimerEnds { timer: "m1".into() },
            condition: None,
            disabled: false,
        },
    ];
    let e = errors(&c);
    assert!(e.iter().any(|m| m.contains("is not a timer")), "{e:?}");
}

#[test]
fn timer_remaining_variable_is_read_only_but_comparable() {
    let mut c = timer_meter_course();
    assert_eq!(crate::timer_remaining_variable("t1"), "Timer.t1.Remaining");
    assert_eq!(
        crate::timer_id_for_remaining_variable("Timer.t1.Remaining"),
        Some("t1")
    );
    assert!(crate::timer_id_for_remaining_variable("Timer..Remaining").is_none());
    assert!(crate::timer_id_for_remaining_variable("Course.Progress").is_none());

    // Writing it is rejected; comparing it and listening for its changes are fine.
    c.scenes[0].slides[0].triggers = vec![
        Trigger {
            action: crate::Action::SetVariable {
                variable: "Timer.t1.Remaining".into(),
                op: crate::VarOp::Assign,
                value: Some(crate::Value::Number(3.0)),
            },
            when: Event::Click {
                object: "t1".into(),
            },
            condition: None,
            disabled: false,
        },
        Trigger {
            action: crate::Action::PauseTimer { timer: "t1".into() },
            when: Event::VariableChanges {
                variable: "Timer.t1.Remaining".into(),
            },
            condition: Some(crate::Condition::Compare {
                variable: "Timer.t1.Remaining".into(),
                op: crate::CmpOp::Le,
                value: crate::Value::Number(5.0),
            }),
            disabled: false,
        },
    ];
    let e = errors(&c);
    assert!(e.iter().any(|m| m.contains("read-only")), "{e:?}");
    assert_eq!(e.len(), 1, "{e:?}");
}

#[test]
fn video_cues_sort_on_load_and_repair_bad_times() {
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "cue-load",
        "title": "Cue load",
        "scenes": [{
            "id": "scene1",
            "title": "Scene 1",
            "slides": [{
                "id": "s1",
                "title": "Slide 1",
                "duration_ms": 5000,
                "objects": [{
                    "id": "video1",
                    "kind": {
                        "type": "video",
                        "src": "clip.mp4",
                        "cues": [
                            { "id": "c3", "at_ms": 3000, "name": "Third" },
                            { "id": "c1", "at_ms": -50, "name": "First" },
                            { "id": "c2", "at_ms": 1000, "name": "Second", "pause": true }
                        ]
                    },
                    "frame": { "x": 0.0, "y": 0.0, "w": 320.0, "h": 180.0 }
                }]
            }]
        }]
    });
    let course = Course::from_json(&json.to_string()).unwrap();
    let crate::ObjectKind::Video { cues, .. } = &course.scenes[0].slides[0].objects[0].kind else {
        panic!("video");
    };
    let ids: Vec<&str> = cues.iter().map(|cue| cue.id.as_str()).collect();
    assert_eq!(ids, vec!["c1", "c2", "c3"]);
    assert_eq!(cues[0].at_ms, 0.0, "a negative time is repaired to 0");
    assert!(cues[1].pause);
    let saved = course.to_json().unwrap();
    assert!(saved.contains("\"cues\""));
}

#[test]
fn video_cue_validation_rejects_duplicates_empty_names_bad_times_and_too_many() {
    let mut course = course_with_media(crate::ObjectKind::Video {
        src: "clip.mp4".into(),
        autoplay: false,
        captions: None,
        volume: 1.0,
        trim: None,
        cues: vec![
            crate::Cue {
                id: "cue1".into(),
                at_ms: 1000.0,
                name: "One".into(),
                pause: false,
            },
            crate::Cue {
                id: "cue1".into(),
                at_ms: f64::NAN,
                name: "  ".into(),
                pause: false,
            },
            crate::Cue {
                id: "cue3".into(),
                at_ms: 99_999.0,
                name: "Past the end".into(),
                pause: false,
            },
        ],
    });
    let e = errors(&course);
    assert!(e.iter().any(|m| m.contains("duplicate cue id")), "{e:?}");
    assert!(e.iter().any(|m| m.contains("cue name is empty")), "{e:?}");
    assert!(e.iter().any(|m| m.contains("finite")), "{e:?}");
    assert!(e.iter().any(|m| m.contains("known duration")), "{e:?}");

    let crate::ObjectKind::Video { cues, .. } =
        &mut course.scenes[0].slides[0].objects.last_mut().unwrap().kind
    else {
        panic!("video");
    };
    *cues = (0..crate::MAX_CUES + 1)
        .map(|i| crate::Cue {
            id: format!("c{i}"),
            at_ms: i as f64,
            name: format!("Cue {i}"),
            pause: false,
        })
        .collect();
    let e = errors(&course);
    assert!(e.iter().any(|m| m.contains("at most")), "{e:?}");
}

#[test]
fn cue_reached_requires_a_video_with_that_cue() {
    let mut course = course_with_media(crate::ObjectKind::Video {
        src: "clip.mp4".into(),
        autoplay: false,
        captions: None,
        volume: 1.0,
        trim: None,
        cues: vec![crate::Cue {
            id: "cue1".into(),
            at_ms: 1000.0,
            name: "One".into(),
            pause: false,
        }],
    });
    course.scenes[0].slides[0].triggers.push(Trigger {
        action: crate::Action::PauseMedia {
            object: "media1".into(),
        },
        when: Event::CueReached {
            object: "media1".into(),
            cue: "cue1".into(),
        },
        condition: None,
        disabled: false,
    });
    assert_eq!(errors(&course), Vec::<String>::new());

    course.scenes[0].slides[0].triggers[0].when = Event::CueReached {
        object: "media1".into(),
        cue: "missing".into(),
    };
    let e = errors(&course);
    assert!(
        e.iter().any(|m| m.contains("not defined on video")),
        "{e:?}"
    );

    course.scenes[0].slides[0].triggers[0].when = Event::CueReached {
        object: "not-a-video".into(),
        cue: "cue1".into(),
    };
    let e = errors(&course);
    assert!(e.iter().any(|m| m.contains("not on slide")), "{e:?}");
}

#[test]
fn a_non_finite_or_negative_cue_time_is_repaired_and_sorted() {
    let mut cues = vec![
        crate::Cue {
            id: "c3".into(),
            at_ms: 3000.0,
            name: "Three".into(),
            pause: false,
        },
        crate::Cue {
            id: "c2".into(),
            at_ms: f64::NAN,
            name: "Bad".into(),
            pause: false,
        },
        crate::Cue {
            id: "c1".into(),
            at_ms: -10.0,
            name: "Negative".into(),
            pause: false,
        },
    ];
    crate::sort_and_repair_cues(&mut cues);
    let ids: Vec<&str> = cues.iter().map(|cue| cue.id.as_str()).collect();
    assert_eq!(ids, vec!["c2", "c1", "c3"]);
    assert_eq!(cues[0].at_ms, 0.0);
    assert_eq!(cues[1].at_ms, 0.0);
}

#[test]
fn answer_points_default_round_trip_is_stable() {
    let c = Course::from_json(HELLO).unwrap();
    let saved = c.to_json().unwrap();
    assert!(!saved.contains("\"scoring\""));
    let again = Course::from_json(&saved).unwrap();
    assert_eq!(c, again);
    assert_eq!(again.to_json().unwrap(), saved);
}

#[test]
fn answer_points_and_scoring_round_trip() {
    let raw = serde_json::json!({
        "kind": "multiple_response",
        "prompt": "Pick",
        "choices": [
            { "id": "a", "text": "A", "correct": true, "points": 3 },
            { "id": "b", "text": "B", "correct": false, "points": -1 }
        ],
        "points": 5,
        "scoring": "per_answer"
    });
    let q: crate::Question = serde_json::from_value(raw).unwrap();
    let again: crate::Question = serde_json::from_str(&serde_json::to_string(&q).unwrap()).unwrap();
    assert_eq!(q, again);
    assert_eq!(q.scoring, crate::Scoring::PerAnswer);
    assert_eq!(q.choices[0].points, 3);
    assert_eq!(q.choices[1].points, -1);
}

#[test]
fn max_points_per_answer_multiple_response_sums_positive() {
    let q: crate::Question = serde_json::from_value(serde_json::json!({
        "kind": "multiple_response",
        "prompt": "p",
        "choices": [
            { "id": "a", "text": "A", "correct": true, "points": 3 },
            { "id": "b", "text": "B", "correct": false, "points": -2 },
            { "id": "c", "text": "C", "correct": true, "points": 4 }
        ],
        "points": 10,
        "scoring": "per_answer"
    }))
    .unwrap();
    assert_eq!(q.max_points(), 7);
}

#[test]
fn max_points_per_answer_other_kind_uses_largest_or_zero() {
    let q: crate::Question = serde_json::from_value(serde_json::json!({
        "kind": "multiple_choice",
        "prompt": "p",
        "choices": [
            { "id": "a", "text": "A", "correct": true, "points": 5 },
            { "id": "b", "text": "B", "correct": false, "points": -3 }
        ],
        "points": 10,
        "scoring": "per_answer"
    }))
    .unwrap();
    assert_eq!(q.max_points(), 5);

    let q: crate::Question = serde_json::from_value(serde_json::json!({
        "kind": "multiple_choice",
        "prompt": "p",
        "choices": [
            { "id": "a", "text": "A", "correct": true, "points": -3 },
            { "id": "b", "text": "B", "correct": false, "points": -1 }
        ],
        "points": 10,
        "scoring": "per_answer"
    }))
    .unwrap();
    assert_eq!(q.max_points(), 0);

    let q: crate::Question = serde_json::from_value(serde_json::json!({
        "kind": "multiple_choice",
        "prompt": "p",
        "choices": [],
        "points": 10,
        "scoring": "per_answer"
    }))
    .unwrap();
    assert_eq!(q.max_points(), 0);
}

#[test]
fn max_points_otherwise_uses_points() {
    let q: crate::Question = serde_json::from_value(serde_json::json!({
        "kind": "multiple_choice",
        "prompt": "p",
        "choices": [
            { "id": "a", "text": "A", "correct": true, "points": 5 }
        ],
        "points": 7,
        "scoring": "all_or_nothing"
    }))
    .unwrap();
    assert_eq!(q.max_points(), 7);
}

#[test]
fn answer_points_validation_rejections() {
    // choice points out of range
    let e = mutate(|c| {
        let q = c.slide_mut("s-quiz").unwrap().question.as_mut().unwrap();
        q.choices[0].points = 2000;
    });
    assert!(e.iter().any(|m| m.contains("/points")), "{e:?}");

    // per_answer not allowed for fill_in
    let e = mutate(|c| {
        let q = c.slide_mut("s-quiz").unwrap().question.as_mut().unwrap();
        q.kind = crate::QuestionKind::FillIn;
        q.scoring = crate::Scoring::PerAnswer;
    });
    assert!(e.iter().any(|m| m.contains("per_answer scoring")), "{e:?}");

    // partial not allowed for multiple_choice
    let e = mutate(|c| {
        let q = c.slide_mut("s-quiz").unwrap().question.as_mut().unwrap();
        q.scoring = crate::Scoring::Partial;
    });
    assert!(e.iter().any(|m| m.contains("partial scoring")), "{e:?}");
}

// ---------------------------------------------------------------- question banks

fn bank_question_slide(id: &str) -> crate::Slide {
    let course = Course::from_json(HELLO).unwrap();
    let mut slide = course.slide("s-quiz").expect("quiz slide").clone();
    slide.id = id.to_owned();
    slide
}

fn course_with_bank() -> Course {
    let mut course = Course::from_json(HELLO).unwrap();
    course.banks.push(crate::QuestionBank {
        id: "bank1".into(),
        title: "Bank One".into(),
        slides: vec![bank_question_slide("bq1"), bank_question_slide("bq2")],
    });
    course
}

#[test]
fn banks_and_draw_round_trip_and_are_omitted_when_unused() {
    let plain = Course::from_json(HELLO).unwrap();
    let saved = plain.to_json().unwrap();
    assert!(!saved.contains("\"banks\""));
    assert!(!saved.contains("\"draw\""));
    assert_eq!(Course::from_json(&saved).unwrap(), plain);

    let mut course = course_with_bank();
    // A draw slide: no question, count within the bank size.
    let draw_slide = course
        .scenes
        .iter_mut()
        .flat_map(|scene| scene.slides.iter_mut())
        .find(|slide| slide.id == "s-intro")
        .expect("intro slide");
    draw_slide.question = None;
    draw_slide.draw = Some(crate::BankDraw {
        bank: "bank1".into(),
        count: 1,
    });
    assert_eq!(errors(&course), Vec::<String>::new());
    let saved = course.to_json().unwrap();
    assert!(saved.contains("\"banks\""));
    assert!(saved.contains("\"draw\""));
    assert!(saved.contains("\"bank\": \"bank1\""));
    let reloaded = Course::from_json(&saved).unwrap();
    assert_eq!(reloaded, course);
    assert_eq!(reloaded.to_json().unwrap(), saved);
}

#[test]
fn bank_lookups_find_bank_and_bank_slides() {
    let course = course_with_bank();
    let play_count = course.slides().count();
    assert_eq!(
        course.bank("bank1").map(|bank| bank.id.as_str()),
        Some("bank1")
    );
    assert!(course.bank("missing").is_none());
    assert_eq!(
        course.bank_slide("bq1").map(|slide| slide.id.as_str()),
        Some("bq1")
    );
    assert!(course.bank_slide("missing").is_none());
    // Item 4: play_slide finds bank slides after scene slides.
    assert_eq!(
        course.play_slide("bq2").map(|slide| slide.id.as_str()),
        Some("bq2")
    );
    assert!(course.play_slide("s-intro").is_some());
    // slides() and slide_index() stay scene slides only.
    assert_eq!(course.slides().count(), play_count);
    assert_eq!(course.slide_index("bq1"), None);
    assert_eq!(course.slide_index("s-intro"), Some(0));
    // Item 5: slide finds bank slides through play_slide, slide_mut too,
    // and all_slide_ids includes them.
    assert_eq!(
        course.slide("bq1").map(|slide| slide.id.as_str()),
        Some("bq1")
    );
    let mut course = course;
    assert!(course.slide_mut("bq1").is_some());
    assert!(course.slide_mut("bq2").is_some());
    assert!(course.all_slide_ids().any(|id| id == "bq1"));
    assert!(course.all_slide_ids().any(|id| id == "bq2"));
}

#[test]
fn bank_ids_and_titles_are_validated() {
    let mut course = course_with_bank();
    assert_eq!(errors(&course), Vec::<String>::new());

    course.banks[0].id = "bad id".into();
    assert!(
        errors(&course)
            .iter()
            .any(|e| e.contains("bank id") && e.contains("[A-Za-z0-9_-]")),
        "{:?}",
        errors(&course)
    );

    course = course_with_bank();
    course.banks.push(crate::QuestionBank {
        id: "bank1".into(),
        title: "Duplicate".into(),
        slides: Vec::new(),
    });
    assert!(
        errors(&course)
            .iter()
            .any(|e| e.contains("duplicate bank id")),
        "{:?}",
        errors(&course)
    );

    course = course_with_bank();
    course.banks[0].title = "   ".into();
    assert!(
        errors(&course)
            .iter()
            .any(|e| e.contains("bank title is empty")),
        "{:?}",
        errors(&course)
    );
}

#[test]
fn bank_slides_are_checked_like_scene_slides_and_ids_are_unique() {
    let course = course_with_bank();
    assert_eq!(errors(&course), Vec::<String>::new());

    // A bad question on a bank slide is reported at the bank path.
    let mut course = course_with_bank();
    let slide = &mut course.banks[0].slides[1];
    if let Some(question) = &mut slide.question {
        question.choices[0].correct = true;
    }
    let issues = course.validate();
    assert!(
        issues
            .iter()
            .any(|i| i.path == "/banks/0/slides/1/question"
                && i.message.contains("exactly 1 correct")),
        "{issues:?}"
    );

    // Duplicate with a scene slide.
    let mut course = course_with_bank();
    course.banks[0].slides[0].id = "s-intro".into();
    assert!(
        errors(&course)
            .iter()
            .any(|e| e.contains("/banks/0/slides/0") && e.contains("duplicate slide id")),
        "{:?}",
        errors(&course)
    );

    // Duplicate with another bank slide.
    let mut course = course_with_bank();
    course.banks[0].slides[1].id = "bq1".into();
    assert!(
        errors(&course)
            .iter()
            .any(|e| e.contains("duplicate slide id")),
        "{:?}",
        errors(&course)
    );

    // Duplicate with a master.
    let mut course = course_with_bank();
    with_master(&mut course, "bq1");
    assert!(!errors(&course).is_empty(), "{:?}", errors(&course));

    // Duplicate with a template.
    let mut course = course_with_bank();
    let mut template = bank_question_slide("bq1");
    template.question = None;
    course.templates.push(template);
    assert!(!errors(&course).is_empty(), "{:?}", errors(&course));
}

#[test]
fn bank_slides_must_have_a_question_and_no_draw() {
    let mut course = course_with_bank();
    course.banks[0].slides[0].question = None;
    assert!(
        errors(&course)
            .iter()
            .any(|e| e.contains("/banks/0/slides/0/question")
                && e.contains("must have a question")),
        "{:?}",
        errors(&course)
    );

    let mut course = course_with_bank();
    course.banks[0].slides[0].draw = Some(crate::BankDraw {
        bank: "bank1".into(),
        count: 1,
    });
    assert!(
        errors(&course)
            .iter()
            .any(|e| e.contains("/banks/0/slides/0/draw") && e.contains("must not have draw")),
        "{:?}",
        errors(&course)
    );
}

#[test]
fn draw_needs_an_existing_bank_a_sane_count_and_no_question() {
    let mut course = course_with_bank();
    let set_draw = |course: &mut Course, draw: crate::BankDraw| {
        let slide = course
            .scenes
            .iter_mut()
            .flat_map(|scene| scene.slides.iter_mut())
            .find(|slide| slide.id == "s-intro")
            .expect("intro slide");
        slide.question = None;
        slide.draw = Some(draw);
    };
    set_draw(
        &mut course,
        crate::BankDraw {
            bank: "bank1".into(),
            count: 2,
        },
    );
    assert_eq!(errors(&course), Vec::<String>::new());

    set_draw(
        &mut course,
        crate::BankDraw {
            bank: "ghost".into(),
            count: 1,
        },
    );
    assert!(
        errors(&course)
            .iter()
            .any(|e| e.contains("draw bank \"ghost\" does not exist")),
        "{:?}",
        errors(&course)
    );

    set_draw(
        &mut course,
        crate::BankDraw {
            bank: "bank1".into(),
            count: 0,
        },
    );
    assert!(
        errors(&course).iter().any(|e| e.contains("at least 1")),
        "{:?}",
        errors(&course)
    );

    set_draw(
        &mut course,
        crate::BankDraw {
            bank: "bank1".into(),
            count: 3,
        },
    );
    assert!(
        errors(&course).iter().any(|e| e.contains("more than bank")),
        "{:?}",
        errors(&course)
    );

    set_draw(
        &mut course,
        crate::BankDraw {
            bank: "bank1".into(),
            count: 1,
        },
    );
    course
        .scenes
        .iter_mut()
        .flat_map(|scene| scene.slides.iter_mut())
        .find(|slide| slide.id == "s-intro")
        .expect("intro slide")
        .question = Some(bank_question_slide("tmp").question.expect("question"));
    assert!(
        errors(&course)
            .iter()
            .any(|e| e.contains("must not have a question")),
        "{:?}",
        errors(&course)
    );
}

#[test]
fn draw_counts_from_one_bank_may_not_add_up_past_its_size() {
    let mut course = course_with_bank();
    // Two draws of 1 from a bank of 2: fine. A third would not be.
    for id in ["s-intro", "s-tabs"] {
        let slide = course
            .scenes
            .iter_mut()
            .flat_map(|scene| scene.slides.iter_mut())
            .find(|slide| slide.id == id)
            .expect("scene slide");
        slide.question = None;
        slide.draw = Some(crate::BankDraw {
            bank: "bank1".into(),
            count: 1,
        });
    }
    assert_eq!(errors(&course), Vec::<String>::new());

    let slide = course
        .scenes
        .iter_mut()
        .flat_map(|scene| scene.slides.iter_mut())
        .find(|slide| slide.id == "s-quiz")
        .expect("quiz slide");
    slide.question = None;
    slide.draw = Some(crate::BankDraw {
        bank: "bank1".into(),
        count: 1,
    });
    assert!(
        errors(&course).iter().any(|e| e.contains("add up to")),
        "{:?}",
        errors(&course)
    );
}

#[test]
fn masters_and_templates_must_not_carry_draw() {
    let mut course = course_with_bank();
    with_master(&mut course, "m1").draw = Some(crate::BankDraw {
        bank: "bank1".into(),
        count: 1,
    });
    assert!(
        errors(&course)
            .iter()
            .any(|e| e.contains("must not have draw")),
        "{:?}",
        errors(&course)
    );

    let mut course = course_with_bank();
    let mut template = bank_question_slide("t1");
    template.draw = Some(crate::BankDraw {
        bank: "bank1".into(),
        count: 1,
    });
    course.templates.push(template);
    assert!(
        errors(&course)
            .iter()
            .any(|e| e.contains("/templates/0/draw") && e.contains("must not have draw")),
        "{:?}",
        errors(&course)
    );
}

#[test]
fn text_effects_default_omitted_and_letter_spacing_repaired() {
    use crate::{TextRun, TextStyle};
    let style = TextStyle::default();
    let json = serde_json::to_value(&style).expect("json");
    assert!(json.get("strikethrough").is_none());
    assert!(json.get("baseline").is_none());
    assert!(json.get("letter_spacing").is_none());
    let style: TextStyle = serde_json::from_str(
        r#"{"strikethrough": true, "baseline": "superscript", "letter_spacing": 1000}"#,
    )
    .expect("style");
    assert!(style.strikethrough);
    assert_eq!(style.baseline, crate::Baseline::Superscript);
    assert_eq!(style.letter_spacing, 50.0);
    let low: TextStyle = serde_json::from_str(r#"{"letter_spacing": -99}"#).expect("style");
    assert_eq!(low.letter_spacing, -5.0);
    assert_eq!(crate::clamp_letter_spacing(f32::NAN), 0.0);
    assert_eq!(crate::clamp_letter_spacing(f32::INFINITY), 0.0);
    let run: TextRun = serde_json::from_str(r#"{"text": "a", "letter_spacing": 99}"#).expect("run");
    assert_eq!(run.letter_spacing, Some(50.0));
    let back = serde_json::to_value(&TextRun {
        text: "a".into(),
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
    })
    .expect("json");
    assert_eq!(back, serde_json::json!({ "text": "a" }));
}

#[test]
fn trigger_disabled_round_trips_and_is_omitted_when_false() {
    let json = r#"{"action": {"action": "submit_results"}, "when": {"event": "timeline_starts"}}"#;
    let trigger: Trigger = serde_json::from_str(json).expect("trigger");
    assert!(!trigger.disabled);
    let out = serde_json::to_value(&trigger).expect("json");
    assert!(out.get("disabled").is_none());
    let mut off = trigger.clone();
    off.disabled = true;
    let out = serde_json::to_value(&off).expect("json");
    assert_eq!(out["disabled"], true);
    let back: Trigger = serde_json::from_value(out).expect("trigger");
    assert_eq!(back, off);
}

#[test]
fn an_old_course_without_navigation_fields_loads_with_free_and_nothing() {
    let c = Course::from_json(HELLO).unwrap();
    assert_eq!(c.player.navigation, crate::NavMode::Free);
    assert_eq!(c.player.next_requires, crate::NextRequirement::Nothing);
    assert!(c.slides().all(|slide| slide.player.is_none()));
    let saved = c.to_json().unwrap();
    assert!(!saved.contains("navigation"));
    assert!(!saved.contains("next_requires"));
}

#[test]
fn navigation_fields_round_trip_and_unknown_variables_are_errors() {
    let mut c = Course::from_json(HELLO).unwrap();
    c.player.navigation = crate::NavMode::Locked;
    c.player.next_requires = crate::NextRequirement::TimelineEnds;
    let again = Course::from_json(&c.to_json().unwrap()).unwrap();
    assert_eq!(again.player, c.player);
    c.player.next_requires = crate::NextRequirement::Variable {
        name: "ghost".into(),
    };
    assert!(
        c.validate()
            .iter()
            .any(|i| i.path == "/player/next_requires")
    );
}

#[test]
fn resources_glossary_and_transcript_round_trip_and_old_json_loads_empty() {
    let mut c = Course::from_json(HELLO).unwrap();
    assert!(c.resources.is_empty() && c.glossary.is_empty());
    assert!(!c.player.show_transcript);
    let saved = c.to_json().unwrap();
    assert!(!saved.contains("resources") && !saved.contains("glossary"));
    assert!(!saved.contains("show_transcript"));

    c.resources = vec![
        crate::Resource {
            id: "r1".into(),
            title: "Guide".into(),
            target: crate::ResourceTarget::File {
                src: "guide.pdf".into(),
            },
        },
        crate::Resource {
            id: "r2".into(),
            title: "Site".into(),
            target: crate::ResourceTarget::Url {
                url: "mailto:help@example.com".into(),
            },
        },
    ];
    c.glossary = vec![crate::GlossaryEntry {
        id: "g1".into(),
        term: "SCORM".into(),
        definition: "A packaging standard.".into(),
    }];
    c.player.show_transcript = true;
    assert!(
        c.validate()
            .iter()
            .all(|i| i.severity != crate::Severity::Error)
    );
    let again = Course::from_json(&c.to_json().unwrap()).unwrap();
    assert_eq!(again.resources, c.resources);
    assert_eq!(again.glossary, c.glossary);
    assert!(again.player.show_transcript);
}

#[test]
fn bad_resources_and_glossary_entries_are_errors() {
    let mut c = Course::from_json(HELLO).unwrap();
    c.resources = vec![
        crate::Resource {
            id: "r1".into(),
            title: " ".into(),
            target: crate::ResourceTarget::Url {
                url: "ftp://example.com".into(),
            },
        },
        crate::Resource {
            id: "r2".into(),
            title: "File".into(),
            target: crate::ResourceTarget::File {
                src: "../secret.pdf".into(),
            },
        },
    ];
    c.glossary = vec![crate::GlossaryEntry {
        id: "g1".into(),
        term: "".into(),
        definition: String::new(),
    }];
    let paths: Vec<String> = c.validate().into_iter().map(|i| i.path).collect();
    assert!(paths.contains(&"/resources/0".to_owned()));
    assert!(paths.contains(&"/resources/0/target".to_owned()));
    assert!(paths.contains(&"/resources/1/target".to_owned()));
    assert!(paths.contains(&"/glossary/0".to_owned()));
}

#[test]
fn old_json_loads_with_the_default_player_look_and_new_fields_round_trip() {
    let mut c = Course::from_json(HELLO).unwrap();
    assert!(c.player.show_title && c.player.show_volume && c.player.show_captions);
    assert!(c.player.logo.is_none());
    let saved = c.to_json().unwrap();
    for key in ["logo", "show_title", "show_volume", "show_captions"] {
        assert!(!saved.contains(key), "{key} should be omitted by default");
    }
    c.player.logo = Some("logo.svg".into());
    c.player.show_title = false;
    c.slide_mut("s-intro").unwrap().player = Some(crate::SlidePlayerOverrides {
        show_seekbar: Some(false),
        ..Default::default()
    });
    let again = Course::from_json(&c.to_json().unwrap()).unwrap();
    assert_eq!(again.player, c.player);
    assert_eq!(
        again.slide("s-intro").unwrap().player,
        c.slide("s-intro").unwrap().player
    );
    c.player.logo = Some("narration.mp3".into());
    assert!(c.validate().iter().any(|i| i.path == "/player/logo"));
}

#[test]
fn newer_schema_version_gives_a_plain_message() {
    // A version above the one this build reads, with a field it does not know.
    let text =
        r#"{"schema_version": 99, "id": "x", "title": "T", "scenes": [], "from_the_future": 1}"#;
    let error = Course::from_json(text).unwrap_err().to_string();
    assert!(error.contains("newer CourseCraft"), "{error}");
    assert!(error.contains("version 99"), "{error}");
    assert!(error.contains("update CourseCraft"), "{error}");
    assert!(!error.contains("unknown field"), "{error}");
    // Known fields only: still refused, not left to validation.
    let text = r#"{"schema_version": 2, "id": "x", "title": "T", "scenes": []}"#;
    let error = Course::from_json(text).unwrap_err().to_string();
    assert!(
        error.contains("newer CourseCraft") && error.contains("version 2"),
        "{error}"
    );
    // Same version with a typo keeps the serde message naming the field.
    let text = r#"{"schema_version": 1, "id": "x", "title": "T", "scenes": [], "typo": 1}"#;
    let error = Course::from_json(text).unwrap_err().to_string();
    assert!(error.contains("unknown field `typo`"), "{error}");
}

#[test]
fn every_example_course_opens_and_validates() {
    let examples = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let mut found = 0;
    for entry in std::fs::read_dir(examples).unwrap() {
        let path = entry.unwrap().path().join("course.json");
        if !path.is_file() {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        let course = Course::from_json(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert_eq!(errors(&course), Vec::<String>::new(), "{}", path.display());
        found += 1;
    }
    assert!(found >= 2, "expected the shipped examples, found {found}");
}
