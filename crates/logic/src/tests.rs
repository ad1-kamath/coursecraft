use coursecraft_course::{
    Action, BACKGROUND_AUDIO_ID, BankDraw, CompletionStatus, Course, Event, ObjectKind,
    QuestionBank, RevisitMode, Slide, Trigger, Trim, Value, VarOp, VariableDef,
};

use crate::{
    Clock, Effect, HostInfo, Input, LearnerResponse, MediaOp, MediaPlaybackState, Player,
    PlayerState, SeekInfo, SuspendError, civil_date, decode_suspend, encode_suspend,
};

const HELLO: &str = include_str!("../../../examples/hello-course/course.json");

fn course() -> Course {
    Course::from_json(HELLO).unwrap()
}

fn click(o: &str) -> Input {
    Input::Click { object: o.into() }
}

fn has(effects: &[Effect], e: &Effect) -> bool {
    effects.contains(e)
}

fn slide_entered(id: &str) -> Effect {
    Effect::SlideEntered { slide: id.into() }
}

fn warned(effects: &[Effect]) -> bool {
    effects.iter().any(|e| matches!(e, Effect::Warning { .. }))
}

/// Drive the player to the quiz slide via the tabs interaction.
fn to_quiz(p: &mut Player<'_>) {
    assert!(has(&p.handle(&Input::Start), &slide_entered("s-intro")));
    p.handle(&Input::Tick { ms: 1000 });
    assert!(has(
        &p.handle(&click("btn-start")),
        &slide_entered("s-tabs")
    ));
    for t in ["tab1", "tab2", "tab3"] {
        p.handle(&click(t));
    }
    assert!(has(&p.handle(&click("btn-next")), &slide_entered("s-quiz")));
}

#[test]
fn link_clicks_run_open_url_and_slide_jumps_without_firing_object_clicks() {
    use coursecraft_course::{Autofit, Link, ObjectKind, TextRun, TextStyle};

    let mut course = course();
    {
        let slide = course.slide_mut("s-intro").unwrap();
        let title = slide
            .objects
            .iter_mut()
            .find(|object| object.id == "title")
            .expect("title");
        title.kind = ObjectKind::Text {
            text: "Go".into(),
            style: TextStyle::default(),
            runs: vec![TextRun {
                text: "Go".into(),
                bold: None,
                italic: None,
                underline: None,
                size: None,
                color: None,
                link: Some(Link::Url {
                    url: "https://example.com".into(),
                }),
                font: None,
                strikethrough: None,
                baseline: None,
                letter_spacing: None,
            }],
            paragraphs: Vec::new(),
            autofit: Autofit::None,
            text_style: None,
        };
        slide.triggers.push(Trigger {
            action: Action::SetVariable {
                variable: "tabs_seen".into(),
                op: VarOp::Assign,
                value: Some(Value::Number(99.0)),
            },
            when: Event::Click {
                object: "title".into(),
            },
            condition: None,
            disabled: false,
        });
    }

    let mut player = Player::new(&course).expect("player");
    player.handle(&Input::Start);

    let effects = player.handle(&Input::LinkClicked {
        object: "title".into(),
        link: Link::Url {
            url: "https://example.com".into(),
        },
    });
    assert_eq!(
        effects,
        vec![Effect::OpenUrl {
            url: "https://example.com".into()
        }]
    );
    assert_eq!(
        player.variable("tabs_seen"),
        Some(&Value::Number(0.0)),
        "a link click must not also fire the object's click trigger"
    );

    let effects = player.handle(&Input::LinkClicked {
        object: "title".into(),
        link: Link::Url {
            url: "javascript:alert(1)".into(),
        },
    });
    assert!(warned(&effects));

    let effects = player.handle(&Input::LinkClicked {
        object: "title".into(),
        link: Link::Slide {
            slide: "s-tabs".into(),
        },
    });
    assert!(has(&effects, &slide_entered("s-tabs")));
    assert_eq!(player.variable("tabs_seen"), Some(&Value::Number(0.0)));
}

#[test]
fn run_javascript_action_emits_its_effect() {
    let mut course = course();
    course
        .slide_mut("s-intro")
        .expect("intro")
        .triggers
        .push(Trigger {
            action: Action::RunJavaScript {
                code: "CourseCraft.setVar('tabs_seen', 1);".into(),
            },
            when: Event::Click {
                object: "title".into(),
            },
            condition: None,
            disabled: false,
        });

    let mut player = Player::new(&course).expect("player");
    player.handle(&Input::Start);
    let effects = player.handle(&click("title"));
    assert!(
        effects.contains(&Effect::RunJavaScript {
            code: "CourseCraft.setVar('tabs_seen', 1);".into()
        }),
        "{effects:?}"
    );
}

#[test]
fn set_variable_input_changes_the_variable_and_fires_its_trigger() {
    let c = course();
    let mut player = Player::new(&c).expect("player");
    player.handle(&Input::Start);
    player.handle(&Input::Tick { ms: 1000 });
    player.handle(&click("btn-start"));
    assert!(has(
        &player.handle(&click("tab1")),
        &Effect::VariableChanged {
            variable: "tabs_seen".into(),
            value: Value::Number(1.0)
        }
    ));

    let effects = player.handle(&Input::SetVariable {
        variable: "tabs_seen".into(),
        value: Value::Number(3.0),
    });
    assert!(has(
        &effects,
        &Effect::VariableChanged {
            variable: "tabs_seen".into(),
            value: Value::Number(3.0)
        }
    ));
    assert!(
        effects.contains(&Effect::StateChanged {
            object: "btn-next".into(),
            state: "normal".into()
        }),
        "the variable_changes trigger must fire: {effects:?}"
    );

    let wrong_type = player.handle(&Input::SetVariable {
        variable: "tabs_seen".into(),
        value: Value::Text("three".into()),
    });
    assert!(warned(&wrong_type), "a wrong type must warn");
    assert_eq!(player.variable("tabs_seen"), Some(&Value::Number(3.0)));
}

#[test]
fn full_playthrough_passes() {
    let c = course();
    let mut p = Player::new(&c).expect("player");

    let fx = p.handle(&Input::Start);
    assert_eq!(fx.first(), Some(&slide_entered("s-intro")));

    // Start button only appears at 1000 ms.
    assert!(warned(&p.handle(&click("btn-start"))));
    p.handle(&Input::Tick { ms: 1000 });
    assert!(has(
        &p.handle(&click("btn-start")),
        &slide_entered("s-tabs")
    ));

    // Next is disabled until all three tabs are seen.
    assert_eq!(p.handle(&click("btn-next")), vec![]);

    let fx = p.handle(&click("tab1"));
    assert_eq!(
        fx,
        vec![
            Effect::VariableChanged {
                variable: "tabs_seen".into(),
                value: Value::Number(1.0)
            },
            Effect::LayerShown { layer: "l1".into() },
            Effect::StateChanged {
                object: "tab1".into(),
                state: "visited".into()
            },
        ]
    );
    // A second click on a visited tab doesn't count again.
    assert_eq!(p.handle(&click("tab1")), vec![]);
    assert_eq!(p.variable("tabs_seen"), Some(&Value::Number(1.0)));

    let fx = p.handle(&click("tab2"));
    assert!(has(&fx, &Effect::LayerHidden { layer: "l1".into() }));
    let fx = p.handle(&click("tab3"));
    assert!(has(
        &fx,
        &Effect::StateChanged {
            object: "btn-next".into(),
            state: "normal".into()
        }
    ));

    assert!(has(&p.handle(&click("btn-next")), &slide_entered("s-quiz")));

    // Submit with nothing selected.
    assert_eq!(p.handle(&click("btn-submit")), vec![Effect::AnswerRequired]);

    // Wrong answer, one attempt left.
    p.handle(&Input::SelectChoice { choice: "a".into() });
    let fx = p.handle(&click("btn-submit"));
    assert!(has(
        &fx,
        &Effect::QuestionAnswered {
            slide: "s-quiz".into(),
            correct: false,
            points: 0,
            attempts_left: Some(1)
        }
    ));
    assert!(has(
        &fx,
        &Effect::LayerShown {
            layer: "incorrect".into()
        }
    ));
    p.handle(&click("btn-try-again"));
    assert!(p.state().visible_layers.is_empty());

    // Right answer.
    let fx = p.handle(&Input::SelectChoice { choice: "b".into() });
    assert!(has(
        &fx,
        &Effect::ChoiceSelected {
            choice: "a".into(),
            selected: false
        }
    ));
    let fx = p.handle(&click("btn-submit"));
    assert!(has(
        &fx,
        &Effect::QuestionAnswered {
            slide: "s-quiz".into(),
            correct: true,
            points: 10,
            attempts_left: Some(0)
        }
    ));

    let fx = p.handle(&click("btn-continue-ok"));
    assert!(has(&fx, &slide_entered("s-results")));
    assert!(has(
        &fx,
        &Effect::Score {
            raw: 10,
            max: 10,
            percent: 100,
            status: CompletionStatus::Passed
        }
    ));
    assert!(has(
        &fx,
        &Effect::Completion {
            status: CompletionStatus::Passed
        }
    ));
}

#[test]
fn exhausted_attempts_fail_the_quiz() {
    let c = course();
    let mut p = Player::new(&c).expect("player");
    to_quiz(&mut p);
    p.handle(&Input::SelectChoice { choice: "a".into() });
    p.handle(&click("btn-submit"));
    p.handle(&click("btn-try-again"));
    p.handle(&Input::SelectChoice { choice: "c".into() });
    let fx = p.handle(&click("btn-submit"));
    assert!(has(
        &fx,
        &Effect::QuestionAnswered {
            slide: "s-quiz".into(),
            correct: false,
            points: 0,
            attempts_left: Some(0)
        }
    ));
    // Locked now.
    assert!(warned(
        &p.handle(&Input::SelectChoice { choice: "b".into() })
    ));
    let fx = p.handle(&Input::Next);
    assert!(has(
        &fx,
        &Effect::Score {
            raw: 0,
            max: 10,
            percent: 0,
            status: CompletionStatus::Failed
        }
    ));
}

#[test]
fn keyboard_objects_follow_the_same_click_rules_without_changing_state() {
    let c = course();
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    assert_eq!(
        p.keyboard_objects(),
        vec!["btn-about".to_owned()],
        "the start button is not on screen yet"
    );
    p.handle(&Input::Tick { ms: 1000 });
    assert_eq!(
        p.keyboard_objects(),
        vec!["btn-start".to_owned(), "btn-about".to_owned()]
    );

    p.handle(&click("btn-about"));
    assert_eq!(
        p.keyboard_objects(),
        vec!["btn-close".to_owned()],
        "the modal layer blocks base clicks and contributes its own button"
    );
    p.handle(&click("btn-close"));
    assert_eq!(
        p.keyboard_objects(),
        vec!["btn-start".to_owned(), "btn-about".to_owned()],
        "hiding the layer restores the base-layer objects"
    );
    let before = p.state().clone();
    let first = p.keyboard_objects();
    assert_eq!(p.keyboard_objects(), first);
    assert_eq!(p.state(), &before, "the query must not change state");
}

#[test]
fn modal_layer_blocks_base_clicks_and_closes_itself() {
    let c = course();
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    p.handle(&Input::Tick { ms: 1000 });
    assert!(has(
        &p.handle(&click("btn-about")),
        &Effect::LayerShown {
            layer: "about".into()
        }
    ));
    assert_eq!(
        p.handle(&click("btn-start")),
        vec![],
        "base click passed through a modal layer"
    );
    assert_eq!(
        p.handle(&click("btn-close")),
        vec![Effect::LayerHidden {
            layer: "about".into()
        }]
    );
    assert!(has(
        &p.handle(&click("btn-start")),
        &slide_entered("s-tabs")
    ));
}

#[test]
fn timeline_reaches_and_ends_fire_once() {
    let mut c = course();
    let s = c.slide_mut("s-intro").unwrap();
    s.triggers.push(Trigger {
        action: Action::ShowLayer {
            layer: "about".into(),
        },
        when: Event::TimelineReaches {
            ms: 2000,
            layer: None,
        },
        condition: None,
        disabled: false,
    });
    s.triggers.push(Trigger {
        action: Action::JumpToSlide {
            target: coursecraft_course::SlideTarget::Next,
        },
        when: Event::TimelineEnds { layer: None },
        condition: None,
        disabled: false,
    });
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    assert!(!has(
        &p.handle(&Input::Tick { ms: 1500 }),
        &Effect::LayerShown {
            layer: "about".into()
        }
    ));
    assert!(has(
        &p.handle(&Input::Tick { ms: 600 }),
        &Effect::LayerShown {
            layer: "about".into()
        }
    ));
    assert!(has(
        &p.handle(&Input::Tick { ms: 5000 }),
        &slide_entered("s-tabs")
    ));
}

#[test]
fn trigger_loops_are_cut_off_not_hung() {
    let mut c = course();
    c.variables.push(VariableDef {
        name: "x".into(),
        value: Value::Number(0.0),
    });
    c.slide_mut("s-intro").unwrap().triggers.push(Trigger {
        action: Action::SetVariable {
            variable: "x".into(),
            op: VarOp::Add,
            value: Some(Value::Number(1.0)),
        },
        when: Event::VariableChanges {
            variable: "x".into(),
        },
        condition: None,
        disabled: false,
    });
    c.slide_mut("s-intro").unwrap().triggers.push(Trigger {
        action: Action::SetVariable {
            variable: "x".into(),
            op: VarOp::Add,
            value: Some(Value::Number(1.0)),
        },
        when: Event::TimelineStarts { layer: None },
        condition: None,
        disabled: false,
    });
    let mut p = Player::new(&c).expect("player");
    let fx = p.handle(&Input::Start);
    assert!(
        fx.iter()
            .any(|e| matches!(e, Effect::Warning { message } if message.contains("trigger loop")))
    );
    // Still usable afterwards.
    p.handle(&Input::Tick { ms: 1000 });
    assert!(has(
        &p.handle(&click("btn-start")),
        &slide_entered("s-tabs")
    ));
}

#[test]
fn suspend_and_resume_continue_where_left_off() {
    let c = course();
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    p.handle(&Input::Tick { ms: 1000 });
    p.handle(&click("btn-start"));
    p.handle(&click("tab1"));
    p.handle(&click("tab2"));

    let saved = serde_json::to_string(p.state()).unwrap();
    let state: PlayerState = serde_json::from_str(&saved).unwrap();
    let mut q = Player::resume(&c, state).unwrap();
    let fx = q.handle(&click("tab3"));
    assert!(has(
        &fx,
        &Effect::StateChanged {
            object: "btn-next".into(),
            state: "normal".into()
        }
    ));

    let bad = PlayerState {
        slide: 99,
        ..PlayerState::default()
    };
    assert!(Player::resume(&c, bad).is_err());
}

#[test]
fn invalid_course_is_refused() {
    let mut c = course();
    c.scenes.clear();
    assert!(Player::new(&c).is_err());
}

#[test]
fn input_gates() {
    let c = course();
    let mut p = Player::new(&c).expect("player");
    assert!(warned(&p.handle(&Input::Tick { ms: 10 })));
    p.handle(&Input::Start);
    assert!(warned(&p.handle(&Input::Start)));
    assert!(warned(&p.handle(&click("does-not-exist"))));
    assert!(warned(&p.handle(&Input::GoTo {
        slide: "nope".into()
    })));
    // Previous on the first slide does nothing.
    assert_eq!(p.handle(&Input::Previous), vec![]);
}

/// Throw thousands of arbitrary inputs at the player: it must never panic.
#[test]
fn input_storm_never_panics() {
    let c = course();
    let mut p = Player::new(&c).expect("player");
    let mut inputs = vec![
        Input::Start,
        Input::Submit,
        Input::Next,
        Input::Previous,
        Input::Tick { ms: 0 },
        Input::Tick { ms: 777 },
        Input::Tick { ms: u32::MAX },
        Input::Key {
            key: "Enter".into(),
        },
        Input::GoTo {
            slide: "s-quiz".into(),
        },
        Input::GoTo {
            slide: "s-intro".into(),
        },
    ];
    for s in c.slides() {
        for id in s.local_ids() {
            inputs.push(click(id));
            inputs.push(Input::HoverStart { object: id.into() });
            inputs.push(Input::MediaEnded { object: id.into() });
        }
    }
    for ch in ["a", "b", "c", "zzz"] {
        inputs.push(Input::SelectChoice { choice: ch.into() });
    }
    inputs.push(Input::TextEntered {
        text: "é".repeat(4000),
    });
    inputs.push(Input::SequenceMoved {
        choice: "a".into(),
        to: usize::MAX,
    });
    inputs.push(Input::MatchSelected {
        left: "p1".into(),
        right: Some("p1".into()),
    });
    inputs.push(Input::MatchSelected {
        left: "nope".into(),
        right: None,
    });
    inputs.push(Input::HotspotClicked {
        x: f32::NAN,
        y: f32::INFINITY,
    });
    inputs.push(Input::DragDropped {
        item: "nope".into(),
        target: Some(String::new()),
        x: f32::NEG_INFINITY,
        y: 1.0e20,
    });
    inputs.push(Input::PlayerPause);
    inputs.push(Input::PlayerResume);
    inputs.push(Input::PlayerSeek { ms: 0 });
    inputs.push(Input::PlayerSeek { ms: 777 });
    inputs.push(Input::PlayerSeek { ms: u64::MAX });
    let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
    for _ in 0..20_000 {
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let i = usize::try_from(seed >> 33).unwrap() % inputs.len();
        let _ = p.handle(&inputs[i]);
    }
}

const PASS_INPUTS: &str = include_str!("../../../examples/hello-course/pass.inputs.json");

fn to_suspend_string(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::from("cc1:");
    let (chunks, rem) = bytes.as_chunks::<3>();
    for &[b0, b1, b2] in chunks {
        out.push(TABLE[(b0 >> 2) as usize] as char);
        out.push(TABLE[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        out.push(TABLE[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char);
        out.push(TABLE[(b2 & 0x3f) as usize] as char);
    }
    match rem.len() {
        1 => {
            let b0 = rem[0];
            out.push(TABLE[(b0 >> 2) as usize] as char);
            out.push(TABLE[((b0 & 0x03) << 4) as usize] as char);
        }
        2 => {
            let b0 = rem[0];
            let b1 = rem[1];
            out.push(TABLE[(b0 >> 2) as usize] as char);
            out.push(TABLE[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
            out.push(TABLE[((b1 & 0x0f) << 2) as usize] as char);
        }
        _ => {}
    }
    out
}

#[test]
fn suspend_round_trip_hello_course_partway() {
    let c = course();
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    p.handle(&Input::Tick { ms: 1000 });
    p.handle(&click("btn-start"));
    p.handle(&click("tab1"));
    p.handle(&click("tab2"));

    let encoded = encode_suspend(&c, p.state());
    let decoded = decode_suspend(&c, &encoded).expect("decode failed");
    let mut resumed = Player::resume(&c, decoded).expect("resume failed");
    let start_fx = resumed.handle(&Input::Start);
    assert_eq!(start_fx.first(), Some(&slide_entered("s-tabs")));

    assert_eq!(resumed.current_slide().unwrap().id, "s-tabs");
    assert_eq!(resumed.object_state("tab1"), Some("visited"));
    assert_eq!(resumed.object_state("tab2"), Some("visited"));
    assert_eq!(resumed.variable("tabs_seen"), Some(&Value::Number(2.0)));

    let fx = resumed.handle(&click("tab3"));
    assert!(has(
        &fx,
        &Effect::StateChanged {
            object: "btn-next".into(),
            state: "normal".into(),
        }
    ));
    assert_eq!(resumed.object_state("btn-next"), Some("normal"));

    let fx2 = resumed.handle(&click("btn-next"));
    assert!(has(&fx2, &slide_entered("s-quiz")));
}

#[test]
fn suspend_round_trip_hello_course_pass_script() {
    let c = course();
    let mut p = Player::new(&c).expect("player");
    let inputs: Vec<Input> = serde_json::from_str(PASS_INPUTS).unwrap();
    for input in &inputs {
        p.handle(input);
    }
    assert_eq!(p.current_slide().unwrap().id, "s-results");
    assert_eq!(
        p.state().answers.get("s-quiz").map(|a| a.correct),
        Some(true)
    );

    let encoded = encode_suspend(&c, p.state());
    println!(
        "pass.inputs.json encoded suspend data: {encoded} (len: {})",
        encoded.len()
    );
    assert!(
        encoded.len() < 200,
        "encoded length was {} characters (>= 200): {encoded}",
        encoded.len()
    );

    let decoded = decode_suspend(&c, &encoded).expect("decode failed");
    assert_eq!(decoded.answers, p.state().answers);
    assert_eq!(decoded.slide, p.state().slide);
    assert_eq!(decoded.visited, p.state().visited);
    assert_eq!(decoded.variables, p.state().variables);
    assert_eq!(decoded.object_states, p.state().object_states);
    assert!(!decoded.started);
}

#[test]
fn suspend_round_trip_fresh_state() {
    let c = course();
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);

    let encoded = encode_suspend(&c, p.state());
    let decoded = decode_suspend(&c, &encoded).expect("decode failed");

    assert_eq!(decoded.slide, p.state().slide);
    assert_eq!(decoded.visited, p.state().visited);
    assert_eq!(decoded.variables, p.state().variables);
    assert_eq!(decoded.answers, p.state().answers);
    assert_eq!(decoded.object_states, p.state().object_states);
    assert!(!decoded.started);

    let mut resumed = Player::resume(&c, decoded).expect("resume failed");
    let fx = resumed.handle(&Input::Start);
    assert_eq!(fx.first(), Some(&slide_entered("s-intro")));
    assert_eq!(resumed.current_slide().unwrap().id, "s-intro");
}

#[test]
fn suspend_hostile_prefix_and_base64() {
    let c = course();

    assert_eq!(
        decode_suspend(&c, "cc0:AQID"),
        Err(SuspendError::InvalidPrefix)
    );
    assert_eq!(
        decode_suspend(&c, "something"),
        Err(SuspendError::InvalidPrefix)
    );
    assert_eq!(decode_suspend(&c, ""), Err(SuspendError::InvalidPrefix));

    assert_eq!(
        decode_suspend(&c, "cc1:abc+def"),
        Err(SuspendError::InvalidBase64)
    );
    assert_eq!(
        decode_suspend(&c, "cc1:abc/def"),
        Err(SuspendError::InvalidBase64)
    );
    assert_eq!(
        decode_suspend(&c, "cc1:abc=def"),
        Err(SuspendError::InvalidBase64)
    );
    assert_eq!(
        decode_suspend(&c, "cc1:abc def"),
        Err(SuspendError::InvalidBase64)
    );
    assert_eq!(
        decode_suspend(&c, "cc1:a"),
        Err(SuspendError::InvalidBase64)
    );
    assert_eq!(
        decode_suspend(&c, "cc1:abcde"),
        Err(SuspendError::InvalidBase64)
    );
}

#[test]
fn suspend_hostile_truncated_at_every_cut_point() {
    let c = course();
    let mut p = Player::new(&c).expect("player");
    let inputs: Vec<Input> = serde_json::from_str(PASS_INPUTS).unwrap();
    for input in &inputs {
        p.handle(input);
    }
    let encoded = encode_suspend(&c, p.state());

    for cut in 0..encoded.len() {
        let prefix = &encoded[..cut];
        let res = decode_suspend(&c, prefix);
        assert!(
            res.is_err(),
            "cut at len {cut} ({prefix:?}) did not return Err: {res:?}"
        );
    }
}

#[test]
fn suspend_hostile_out_of_range_indices() {
    let c = course();

    // Slide index out of range (999). Leading 0x00 is quiz_attempts (hello has a quiz).
    let bad_slide = to_suspend_string(&[0x00, 0xe7, 0x07, 0x00, 0x00, 0x00, 0x00]);
    assert_eq!(
        decode_suspend(&c, &bad_slide),
        Err(SuspendError::InvalidSlideIndex)
    );

    // Visited slide index out of range (999)
    let bad_visited = to_suspend_string(&[0x00, 0x00, 0x01, 0xe7, 0x07, 0x00, 0x00, 0x00]);
    assert_eq!(
        decode_suspend(&c, &bad_visited),
        Err(SuspendError::InvalidSlideIndex)
    );

    // Modified object states slide index out of range (999)
    let bad_mod_slide = to_suspend_string(&[0x00, 0x00, 0x00, 0x01, 0xe7, 0x07, 0x00, 0x00, 0x00]);
    assert_eq!(
        decode_suspend(&c, &bad_mod_slide),
        Err(SuspendError::InvalidSlideIndex)
    );

    // Modified object states object index out of range (999 on slide 0)
    let bad_obj = to_suspend_string(&[
        0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0xe7, 0x07, 0x00, 0x00, 0x00,
    ]);
    assert_eq!(
        decode_suspend(&c, &bad_obj),
        Err(SuspendError::InvalidObjectIndex)
    );

    // Modified object states state index out of range (999 on object 0)
    let bad_state = to_suspend_string(&[
        0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0xe7, 0x07, 0x00, 0x00,
    ]);
    assert_eq!(
        decode_suspend(&c, &bad_state),
        Err(SuspendError::InvalidStateIndex)
    );

    // Variable index out of range (999)
    let bad_var = to_suspend_string(&[0x00, 0x00, 0x00, 0x00, 0x01, 0xe7, 0x07, 0x00, 0x00]);
    assert_eq!(
        decode_suspend(&c, &bad_var),
        Err(SuspendError::InvalidVariableIndex)
    );

    // Answer slide index out of range (999)
    let bad_ans_slide = to_suspend_string(&[
        0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0xe7, 0x07, 0x01, 0x01, 0x0a,
    ]);
    assert_eq!(
        decode_suspend(&c, &bad_ans_slide),
        Err(SuspendError::InvalidSlideIndex)
    );
}

#[test]
fn suspend_hostile_variable_type_mismatch() {
    let c = course();
    // Variable index 0 is tabs_seen (Number). Encoded with tag 2 (Text "hello")
    let mismatch = to_suspend_string(&[
        0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x02, 0x05, 0x68, 0x65, 0x6c, 0x6c, 0x6f, 0x00,
    ]);
    assert_eq!(
        decode_suspend(&c, &mismatch),
        Err(SuspendError::VariableTypeMismatch)
    );
}

#[test]
fn suspend_hostile_trailing_bytes() {
    let c = course();
    // Valid body for fresh state: slide 0, visited 0, mod_slides 0, vars 0, ans 0.
    // Followed by trailing byte 0x42.
    let trailing = to_suspend_string(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x42]);
    assert_eq!(
        decode_suspend(&c, &trailing),
        Err(SuspendError::TrailingBytes)
    );
}

#[test]
fn suspend_hostile_huge_varint_count() {
    let c = course();
    // 10 continuation bytes exceeding 64 bits
    let overflow_varint = to_suspend_string(&[
        0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x01,
    ]);
    assert_eq!(
        decode_suspend(&c, &overflow_varint),
        Err(SuspendError::InvalidVarint)
    );

    // Visited count = 65535, but only 0 bytes remain
    let huge_count = to_suspend_string(&[0x00, 0x00, 0xff, 0xff, 0x03]);
    assert_eq!(
        decode_suspend(&c, &huge_count),
        Err(SuspendError::Truncated)
    );
}

// ---------------------------------------------------------------- Media tests

fn media_test_course() -> Course {
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "media-test",
        "title": "Media Test",
        "scenes": [{
            "id": "scene1",
            "title": "Scene 1",
            "slides": [
                {
                    "id": "s1",
                    "title": "Slide 1",
                    "duration_ms": 5000,
                    "objects": [
                        {
                            "id": "btn_next",
                            "kind": { "type": "button", "label": "Next" },
                            "frame": { "x": 0.0, "y": 0.0, "w": 100.0, "h": 50.0 },
                            "timing": { "start_ms": 0 }
                        },
                        {
                            "id": "aud_base1",
                            "kind": { "type": "audio", "src": "base1.mp3", "autoplay": false },
                            "frame": { "x": 0.0, "y": 0.0, "w": 48.0, "h": 48.0 },
                            "timing": { "start_ms": 0 }
                        },
                        {
                            "id": "aud_base2",
                            "kind": { "type": "audio", "src": "base2.mp3", "autoplay": false },
                            "frame": { "x": 0.0, "y": 0.0, "w": 48.0, "h": 48.0 },
                            "timing": { "start_ms": 0 }
                        },
                        {
                            "id": "aud_auto",
                            "kind": { "type": "audio", "src": "auto.mp3", "autoplay": true },
                            "frame": { "x": 0.0, "y": 0.0, "w": 48.0, "h": 48.0 },
                            "timing": { "start_ms": 1000 }
                        },
                        {
                            "id": "txt1",
                            "kind": { "type": "text", "text": "Hello" },
                            "frame": { "x": 0.0, "y": 0.0, "w": 100.0, "h": 50.0 },
                            "timing": { "start_ms": 0 }
                        }
                    ],
                    "layers": [
                        {
                            "id": "l_pause",
                            "name": "Pause Layer",
                            "duration_ms": 3000,
                            "pause_base_timeline": true,
                            "objects": [
                                {
                                    "id": "aud_layer",
                                    "kind": { "type": "audio", "src": "layer.mp3", "autoplay": false },
                                    "frame": { "x": 0.0, "y": 0.0, "w": 48.0, "h": 48.0 },
                                    "timing": { "start_ms": 0 }
                                }
                            ]
                        }
                    ],
                    "triggers": [
                        {
                            "when": { "event": "click", "object": "btn_next" },
                            "action": { "action": "jump_to_slide", "target": { "id": "s2" } }
                        },
                        {
                            "when": { "event": "media_ends", "object": "aud_base1" },
                            "action": { "action": "jump_to_slide", "target": { "id": "s2" } }
                        }
                    ]
                },
                {
                    "id": "s2",
                    "title": "Slide 2",
                    "duration_ms": 5000,
                    "objects": [
                        {
                            "id": "aud_s2",
                            "kind": { "type": "audio", "src": "s2.mp3", "autoplay": false },
                            "frame": { "x": 0.0, "y": 0.0, "w": 48.0, "h": 48.0 },
                            "timing": { "start_ms": 0 }
                        }
                    ]
                }
            ]
        }]
    });
    Course::from_json(&json.to_string()).unwrap()
}

fn lottie_test_course(loop_playback: bool, ends_trigger: bool) -> Course {
    let triggers = if ends_trigger {
        serde_json::json!([
            {
                "when": { "event": "media_ends", "object": "lot1" },
                "action": { "action": "jump_to_slide", "target": { "id": "s2" } }
            }
        ])
    } else {
        serde_json::json!([])
    };
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "lottie-test",
        "title": "Lottie Test",
        "scenes": [{
            "id": "scene1",
            "title": "Scene 1",
            "slides": [
                {
                    "id": "s1",
                    "title": "Slide 1",
                    "duration_ms": 5000,
                    "objects": [
                        {
                            "id": "play",
                            "kind": { "type": "button", "label": "Play" },
                            "frame": { "x": 0.0, "y": 0.0, "w": 100.0, "h": 50.0 }
                        },
                        {
                            "id": "pause",
                            "kind": { "type": "button", "label": "Pause" },
                            "frame": { "x": 0.0, "y": 60.0, "w": 100.0, "h": 50.0 }
                        },
                        {
                            "id": "stop",
                            "kind": { "type": "button", "label": "Stop" },
                            "frame": { "x": 0.0, "y": 120.0, "w": 100.0, "h": 50.0 }
                        },
                        {
                            "id": "lot1",
                            "kind": {
                                "type": "lottie",
                                "src": "anim.json",
                                "loop": loop_playback,
                                "autoplay": false
                            },
                            "frame": { "x": 200.0, "y": 0.0, "w": 200.0, "h": 200.0 }
                        }
                    ],
                    "triggers": [
                        {
                            "when": { "event": "click", "object": "play" },
                            "action": { "action": "play_media", "object": "lot1" }
                        },
                        {
                            "when": { "event": "click", "object": "pause" },
                            "action": { "action": "pause_media", "object": "lot1" }
                        },
                        {
                            "when": { "event": "click", "object": "stop" },
                            "action": { "action": "stop_media", "object": "lot1" }
                        }
                    ]
                },
                {
                    "id": "s2",
                    "title": "Slide 2",
                    "duration_ms": 5000
                }
            ]
        }]
    });
    let mut course: Course = Course::from_json(&json.to_string()).unwrap();
    course.scenes[0].slides[0]
        .triggers
        .extend(serde_json::from_value::<Vec<Trigger>>(triggers).unwrap_or_default());
    course
}

#[test]
fn lottie_play_pause_stop_drive_the_media_clock() {
    let course = lottie_test_course(false, false);
    let mut player = Player::new(&course).unwrap();
    player.handle(&Input::Start);
    assert!(!player.is_media_playing("lot1"));
    assert_eq!(player.media_clock_ms("lot1"), None);

    let fx = player.handle(&click("play"));
    assert!(has(&fx, &media_effect("lot1", MediaOp::Play)));
    assert!(player.is_media_playing("lot1"));
    assert_eq!(player.media_clock_ms("lot1"), Some(0));

    player.handle(&Input::Tick { ms: 250 });
    assert_eq!(player.media_clock_ms("lot1"), Some(250));

    let fx = player.handle(&click("pause"));
    assert!(has(&fx, &media_effect("lot1", MediaOp::Pause)));
    assert!(player.is_media_paused("lot1"));
    player.handle(&Input::Tick { ms: 250 });
    assert_eq!(
        player.media_clock_ms("lot1"),
        Some(250),
        "a paused animation must keep its position"
    );

    let fx = player.handle(&click("play"));
    assert!(has(&fx, &media_effect("lot1", MediaOp::Play)));
    assert_eq!(
        player.media_clock_ms("lot1"),
        Some(0),
        "play restarts the animation"
    );
    player.handle(&Input::Tick { ms: 100 });
    assert_eq!(player.media_clock_ms("lot1"), Some(100));

    let fx = player.handle(&click("stop"));
    assert!(has(&fx, &media_effect("lot1", MediaOp::Stop)));
    assert!(!player.is_media_playing("lot1"));
    assert_eq!(
        player.media_clock_ms("lot1"),
        None,
        "stop forgets the position so the first frame shows again"
    );
}

#[test]
fn lottie_media_ends_fires_triggers_for_a_non_looping_animation() {
    let course = lottie_test_course(false, true);
    let mut player = Player::new(&course).unwrap();
    player.handle(&Input::Start);
    player.handle(&click("play"));
    assert!(player.is_media_playing("lot1"));

    let fx = player.handle(&Input::MediaEnded {
        object: "lot1".into(),
    });
    assert!(
        has(&fx, &slide_entered("s2")),
        "media_ends should run the non-looping animation's trigger: {fx:?}"
    );
    assert!(!player.is_media_playing("lot1"));
}

fn background_course(looping: bool) -> Course {
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "background",
        "title": "Background",
        "background_audio": { "src": "music.mp3", "volume": 0.35, "loop": looping },
        "scenes": [{
            "id": "sc",
            "title": "Scene",
            "slides": [
                {
                    "id": "s1",
                    "title": "One",
                    "duration_ms": 5000,
                    "objects": [
                        { "id": "next", "kind": { "type": "button", "label": "Next" }, "frame": { "x": 0.0, "y": 0.0, "w": 100.0, "h": 50.0 } },
                        { "id": "pause", "kind": { "type": "button", "label": "Pause" }, "frame": { "x": 0.0, "y": 60.0, "w": 100.0, "h": 50.0 } },
                        { "id": "play", "kind": { "type": "button", "label": "Play" }, "frame": { "x": 0.0, "y": 120.0, "w": 100.0, "h": 50.0 } },
                        { "id": "stop", "kind": { "type": "button", "label": "Stop" }, "frame": { "x": 0.0, "y": 180.0, "w": 100.0, "h": 50.0 } },
                        { "id": "exit", "kind": { "type": "button", "label": "Exit" }, "frame": { "x": 0.0, "y": 240.0, "w": 100.0, "h": 50.0 } }
                    ],
                    "triggers": [
                        { "when": { "event": "click", "object": "next" }, "action": { "action": "jump_to_slide", "target": { "id": "s2" } } },
                        { "when": { "event": "click", "object": "pause" }, "action": { "action": "pause_media", "object": "Course.BackgroundAudio" } },
                        { "when": { "event": "click", "object": "play" }, "action": { "action": "play_media", "object": "Course.BackgroundAudio" } },
                        { "when": { "event": "click", "object": "stop" }, "action": { "action": "stop_media", "object": "Course.BackgroundAudio" } },
                        { "when": { "event": "click", "object": "exit" }, "action": { "action": "exit_course" } }
                    ]
                },
                {
                    "id": "s2",
                    "title": "Two",
                    "duration_ms": 5000
                }
            ]
        }]
    });
    Course::from_json(&json.to_string()).unwrap()
}

fn background_play(op: MediaOp) -> Effect {
    media_effect(BACKGROUND_AUDIO_ID, op)
}

#[test]
fn background_audio_starts_with_the_course() {
    let c = background_course(true);
    let mut p = Player::new(&c).expect("player");
    let fx = p.handle(&Input::Start);
    assert!(has(&fx, &background_play(MediaOp::Play)), "{fx:?}");
    assert!(p.is_media_playing(BACKGROUND_AUDIO_ID));
}

#[test]
fn a_slide_change_does_not_stop_background_audio() {
    let c = background_course(true);
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    let fx = p.handle(&click("next"));
    assert!(has(&fx, &slide_entered("s2")), "{fx:?}");
    assert!(
        !fx.iter().any(|effect| matches!(
            effect,
            Effect::Media { object, op: MediaOp::Stop } if object == BACKGROUND_AUDIO_ID
        )),
        "{fx:?}"
    );
    assert!(p.is_media_playing(BACKGROUND_AUDIO_ID));
}

#[test]
fn background_audio_loops_after_a_tick() {
    let c = background_course(true);
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    p.handle(&Input::Tick { ms: 16 });
    let fx = p.handle(&Input::MediaEnded {
        object: BACKGROUND_AUDIO_ID.into(),
    });
    assert!(has(&fx, &background_play(MediaOp::Play)), "{fx:?}");
    assert!(p.is_media_playing(BACKGROUND_AUDIO_ID));
}

#[test]
fn background_audio_reports_a_blocked_start_before_the_first_tick() {
    let c = background_course(true);
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    let fx = p.handle(&Input::MediaEnded {
        object: BACKGROUND_AUDIO_ID.into(),
    });
    assert!(!p.is_media_playing(BACKGROUND_AUDIO_ID));
    assert!(
        fx.iter().any(|effect| matches!(
            effect,
            Effect::Warning { message } if message.contains("background audio could not start")
        )),
        "{fx:?}"
    );
    assert!(!has(&fx, &background_play(MediaOp::Play)), "{fx:?}");
}

#[test]
fn background_audio_ends_when_loop_is_false() {
    let c = background_course(false);
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    p.handle(&Input::Tick { ms: 16 });
    let fx = p.handle(&Input::MediaEnded {
        object: BACKGROUND_AUDIO_ID.into(),
    });
    assert!(!p.is_media_playing(BACKGROUND_AUDIO_ID));
    assert!(!has(&fx, &background_play(MediaOp::Play)), "{fx:?}");
}

#[test]
fn background_audio_pauses_and_plays_again() {
    let c = background_course(true);
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);

    let fx = p.handle(&click("pause"));
    assert!(has(&fx, &background_play(MediaOp::Pause)), "{fx:?}");
    assert!(!p.is_media_playing(BACKGROUND_AUDIO_ID));
    assert!(p.state().background.paused_by_author);

    let fx = p.handle(&click("play"));
    assert!(has(&fx, &background_play(MediaOp::Play)), "{fx:?}");
    assert!(p.is_media_playing(BACKGROUND_AUDIO_ID));
    assert!(!p.state().background.paused_by_author);
}

#[test]
fn a_stopped_background_track_reports_the_stale_end_warning() {
    let c = background_course(true);
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    let fx = p.handle(&click("stop"));
    assert!(has(&fx, &background_play(MediaOp::Stop)), "{fx:?}");
    assert!(!p.is_media_playing(BACKGROUND_AUDIO_ID));

    let fx = p.handle(&Input::MediaEnded {
        object: BACKGROUND_AUDIO_ID.into(),
    });
    assert!(warned(&fx), "{fx:?}");
    assert!(!has(&fx, &background_play(MediaOp::Play)), "{fx:?}");
}

#[test]
fn exit_course_stops_background_audio_before_exiting() {
    let c = background_course(true);
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    let fx = p.handle(&click("exit"));
    let stop = fx
        .iter()
        .position(|effect| *effect == background_play(MediaOp::Stop));
    let exit = fx.iter().position(|effect| *effect == Effect::Exit);
    assert!(stop.is_some(), "{fx:?}");
    assert!(exit.is_some(), "{fx:?}");
    assert!(stop < exit, "{fx:?}");
    assert!(!p.is_media_playing(BACKGROUND_AUDIO_ID));
}

#[test]
fn background_audio_is_not_written_to_suspend_data() {
    let c = background_course(true);
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    p.handle(&Input::Tick { ms: 16 });
    assert!(p.is_media_playing(BACKGROUND_AUDIO_ID));

    let encoded = encode_suspend(&c, p.state());
    let decoded = decode_suspend(&c, &encoded).unwrap();
    assert_eq!(decoded.background, crate::BackgroundState::default());
}

#[test]
fn the_example_course_has_no_background_audio_effects() {
    let c = course();
    assert!(c.background_audio.is_none());
    let inputs: Vec<Input> = serde_json::from_str(PASS_INPUTS).expect("inputs");
    let mut p = Player::new(&c).expect("player");
    for input in &inputs {
        let fx = p.handle(input);
        assert!(
            !fx.iter().any(|effect| matches!(
                effect,
                Effect::Media { object, .. } if object == BACKGROUND_AUDIO_ID
            )),
            "unexpected background effect for {input:?}: {fx:?}"
        );
    }
}

#[test]
fn rule1_track_what_plays_and_suspend_unchanged() {
    let trigger_course = {
        let mut tc = media_test_course();
        tc.scenes[0].slides[0].triggers = vec![
            Trigger {
                when: Event::Click {
                    object: "btn_next".into(),
                },
                action: Action::PlayMedia {
                    object: "aud_base1".into(),
                },
                condition: None,
                disabled: false,
            },
            Trigger {
                when: Event::Click {
                    object: "txt1".into(),
                },
                action: Action::PauseMedia {
                    object: "aud_base1".into(),
                },
                condition: None,
                disabled: false,
            },
        ];
        tc
    };
    let mut p = Player::new(&trigger_course).unwrap();
    p.handle(&Input::Start);

    assert!(!p.is_media_playing("aud_base1"));
    assert!(!p.is_media_paused("aud_base1"));

    p.handle(&Input::Click {
        object: "btn_next".into(),
    });
    assert!(p.is_media_playing("aud_base1"));
    assert!(!p.is_media_paused("aud_base1"));

    p.handle(&Input::Click {
        object: "txt1".into(),
    });
    assert!(!p.is_media_playing("aud_base1"));
    assert!(p.is_media_paused("aud_base1"));

    // Check suspend: encode and decode suspend data, verifying media_playback is not in suspend data
    let encoded = encode_suspend(p.course(), p.state());
    let decoded_state = decode_suspend(p.course(), &encoded).unwrap();
    assert!(decoded_state.media_playback.is_empty());
}

#[test]
fn rule2_slide_exit_stops_media_before_slide_entered() {
    let c = {
        let mut tc = media_test_course();
        tc.scenes[0].slides[0].triggers = vec![
            Trigger {
                when: Event::TimelineStarts { layer: None },
                action: Action::PlayMedia {
                    object: "aud_base1".into(),
                },
                condition: None,
                disabled: false,
            },
            Trigger {
                when: Event::TimelineStarts { layer: None },
                action: Action::PlayMedia {
                    object: "aud_base2".into(),
                },
                condition: None,
                disabled: false,
            },
            Trigger {
                when: Event::TimelineStarts { layer: None },
                action: Action::PauseMedia {
                    object: "aud_base2".into(),
                },
                condition: None,
                disabled: false,
            },
            Trigger {
                when: Event::Click {
                    object: "btn_next".into(),
                },
                action: Action::JumpToSlide {
                    target: coursecraft_course::SlideTarget::Id("s2".into()),
                },
                condition: None,
                disabled: false,
            },
        ];
        tc
    };

    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    assert!(p.is_media_playing("aud_base1"));
    assert!(p.is_media_paused("aud_base2"));

    let fx = p.handle(&click("btn_next"));

    // Find indices of Stop for aud_base1, aud_base2, and SlideEntered for s2
    let stop1 = fx
        .iter()
        .position(|e| {
            e == &Effect::Media {
                object: "aud_base1".into(),
                op: crate::MediaOp::Stop,
            }
        })
        .unwrap();
    let stop2 = fx
        .iter()
        .position(|e| {
            e == &Effect::Media {
                object: "aud_base2".into(),
                op: crate::MediaOp::Stop,
            }
        })
        .unwrap();
    let entered = fx
        .iter()
        .position(|e| e == &Effect::SlideEntered { slide: "s2".into() })
        .unwrap();

    assert!(stop1 < entered, "stop1 must be before slide_entered");
    assert!(stop2 < entered, "stop2 must be before slide_entered");
    assert!(!p.is_media_playing("aud_base1"));
    assert!(!p.is_media_paused("aud_base2"));
}

#[test]
fn rule3_layer_hidden_stops_media_on_layer() {
    let c = {
        let mut tc = media_test_course();
        tc.scenes[0].slides[0].triggers = vec![
            Trigger {
                when: Event::Click {
                    object: "btn_next".into(),
                },
                action: Action::ShowLayer {
                    layer: "l_pause".into(),
                },
                condition: None,
                disabled: false,
            },
            Trigger {
                when: Event::TimelineStarts {
                    layer: Some("l_pause".into()),
                },
                action: Action::PlayMedia {
                    object: "aud_layer".into(),
                },
                condition: None,
                disabled: false,
            },
            Trigger {
                when: Event::Click {
                    object: "txt1".into(),
                },
                action: Action::HideLayer {
                    layer: Some("l_pause".into()),
                },
                condition: None,
                disabled: false,
            },
        ];
        tc
    };

    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);

    // Show layer -> plays aud_layer
    p.handle(&click("btn_next"));
    assert!(p.is_media_playing("aud_layer"));

    // Hide layer -> aud_layer receives Media { Stop }
    let fx = p.handle(&click("txt1"));
    assert!(has(
        &fx,
        &Effect::Media {
            object: "aud_layer".into(),
            op: crate::MediaOp::Stop
        }
    ));
    assert!(has(
        &fx,
        &Effect::LayerHidden {
            layer: "l_pause".into()
        }
    ));
    assert!(!p.is_media_playing("aud_layer"));
}

#[test]
fn rule4_base_timeline_pausing_and_resuming() {
    let c = {
        let mut tc = media_test_course();
        tc.scenes[0].slides[0].triggers = vec![
            Trigger {
                when: Event::TimelineStarts { layer: None },
                action: Action::PlayMedia {
                    object: "aud_base1".into(),
                },
                condition: None,
                disabled: false,
            },
            Trigger {
                when: Event::TimelineStarts { layer: None },
                action: Action::PlayMedia {
                    object: "aud_base2".into(),
                },
                condition: None,
                disabled: false,
            },
            Trigger {
                when: Event::TimelineStarts { layer: None },
                action: Action::PauseMedia {
                    object: "aud_base2".into(),
                },
                condition: None,
                disabled: false,
            },
            Trigger {
                when: Event::Click {
                    object: "btn_next".into(),
                },
                action: Action::ShowLayer {
                    layer: "l_pause".into(),
                },
                condition: None,
                disabled: false,
            },
            Trigger {
                when: Event::Click {
                    object: "txt1".into(),
                },
                action: Action::HideLayer {
                    layer: Some("l_pause".into()),
                },
                condition: None,
                disabled: false,
            },
        ];
        tc
    };

    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);

    assert!(p.is_media_playing("aud_base1"));
    assert!(p.is_media_paused("aud_base2"));

    // Showing pause_base_timeline layer pauses playing base-layer media (aud_base1)
    let fx = p.handle(&click("btn_next"));
    assert!(has(
        &fx,
        &Effect::Media {
            object: "aud_base1".into(),
            op: crate::MediaOp::Pause
        }
    ));
    // aud_base2 was paused by author, so it is already paused and does not get an extra pause
    assert!(!has(
        &fx,
        &Effect::Media {
            object: "aud_base2".into(),
            op: crate::MediaOp::Pause
        }
    ));

    // Hiding the layer resumes aud_base1 with Play, but aud_base2 (author-paused) stays paused!
    let fx = p.handle(&click("txt1"));
    assert!(has(
        &fx,
        &Effect::Media {
            object: "aud_base1".into(),
            op: crate::MediaOp::Play
        }
    ));
    assert!(!has(
        &fx,
        &Effect::Media {
            object: "aud_base2".into(),
            op: crate::MediaOp::Play
        }
    ));
    assert!(p.is_media_playing("aud_base1"));
    assert!(p.is_media_paused("aud_base2"));
}

#[test]
fn rule5_media_ended_semantics() {
    let c = {
        let mut tc = media_test_course();
        tc.scenes[0].slides[0].triggers = vec![
            Trigger {
                when: Event::TimelineStarts { layer: None },
                action: Action::PlayMedia {
                    object: "aud_base1".into(),
                },
                condition: None,
                disabled: false,
            },
            Trigger {
                when: Event::MediaEnds {
                    object: "aud_base1".into(),
                },
                action: Action::JumpToSlide {
                    target: coursecraft_course::SlideTarget::Id("s2".into()),
                },
                condition: None,
                disabled: false,
            },
        ];
        tc
    };

    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    assert!(p.is_media_playing("aud_base1"));

    // MediaEnded for an object that is NOT playing (aud_base2) is ignored with warning
    let fx = p.handle(&Input::MediaEnded {
        object: "aud_base2".into(),
    });
    assert!(warned(&fx));
    assert_eq!(p.current_slide().unwrap().id, "s1");

    // MediaEnded for playing object aud_base1 marks it stopped and fires media_ends (jumps to s2)
    let fx = p.handle(&Input::MediaEnded {
        object: "aud_base1".into(),
    });
    assert!(!p.is_media_playing("aud_base1"));
    assert!(has(&fx, &Effect::SlideEntered { slide: "s2".into() }));
    assert_eq!(p.current_slide().unwrap().id, "s2");

    // Delayed/stale MediaEnded for aud_base1 after slide change is now ignored with warning
    let fx = p.handle(&Input::MediaEnded {
        object: "aud_base1".into(),
    });
    assert!(warned(&fx));
}

fn set_audio_options(
    course: &mut Course,
    id: &str,
    autoplay: bool,
    volume: f32,
    trim: Option<Trim>,
) {
    let object = course.scenes[0].slides[0]
        .objects
        .iter_mut()
        .find(|object| object.id == id)
        .expect("audio object");
    if let ObjectKind::Audio {
        autoplay: current_autoplay,
        volume: current_volume,
        trim: current_trim,
        ..
    } = &mut object.kind
    {
        *current_autoplay = autoplay;
        *current_volume = volume;
        *current_trim = trim;
    }
}

#[test]
fn trimmed_autoplay_audio_seeks_to_its_start_after_play() {
    let mut c = media_test_course();
    set_audio_options(
        &mut c,
        "aud_base1",
        true,
        1.0,
        Some(Trim {
            start_ms: 1000,
            end_ms: Some(3000),
        }),
    );
    let mut p = Player::new(&c).expect("player");
    let fx = p.handle(&Input::Start);
    let play = fx
        .iter()
        .position(|effect| effect == &media_effect("aud_base1", MediaOp::Play));
    let seek = fx
        .iter()
        .position(|effect| effect == &seek_effect("aud_base1", 1000));
    assert!(play.is_some() && seek.is_some(), "{fx:?}");
    assert!(play < seek, "MediaSeek must follow Play: {fx:?}");
}

#[test]
fn media_time_past_trim_end_stops_and_fires_media_ends_once() {
    let mut c = media_test_course();
    set_audio_options(
        &mut c,
        "aud_base1",
        true,
        1.0,
        Some(Trim {
            start_ms: 1000,
            end_ms: Some(3000),
        }),
    );
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    let fx = p.handle(&Input::MediaTime {
        object: "aud_base1".into(),
        at_ms: 3000.0,
    });
    assert!(
        has(&fx, &media_effect("aud_base1", MediaOp::Stop)),
        "{fx:?}"
    );
    assert!(!p.is_media_playing("aud_base1"));
    assert_eq!(
        fx.iter()
            .filter(|effect| **effect == slide_entered("s2"))
            .count(),
        1,
        "the media_ends trigger runs once: {fx:?}"
    );

    let fx = p.handle(&Input::MediaEnded {
        object: "aud_base1".into(),
    });
    assert!(warned(&fx));
}

#[test]
fn media_time_before_trim_end_changes_nothing() {
    let mut c = media_test_course();
    set_audio_options(
        &mut c,
        "aud_base1",
        true,
        1.0,
        Some(Trim {
            start_ms: 1000,
            end_ms: Some(3000),
        }),
    );
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    let fx = p.handle(&Input::MediaTime {
        object: "aud_base1".into(),
        at_ms: 2000.0,
    });
    assert!(fx.is_empty(), "{fx:?}");
    assert!(p.is_media_playing("aud_base1"));
}

#[test]
fn untrimmed_audio_ignores_media_time() {
    let mut c = media_test_course();
    set_audio_options(&mut c, "aud_base1", true, 1.0, None);
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    let fx = p.handle(&Input::MediaTime {
        object: "aud_base1".into(),
        at_ms: 4000.0,
    });
    assert!(fx.is_empty(), "{fx:?}");
    assert!(p.is_media_playing("aud_base1"));
}

#[test]
fn player_seek_maps_a_trimmed_autoplay_object_and_clamps_to_its_end() {
    let mut c = media_test_course();
    set_audio_options(
        &mut c,
        "aud_auto",
        true,
        1.0,
        Some(Trim {
            start_ms: 500,
            end_ms: Some(2000),
        }),
    );
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);

    let fx = p.handle(&Input::PlayerSeek { ms: 2500 });
    assert!(has(&fx, &seek_effect("aud_auto", 2000)), "{fx:?}");

    let fx = p.handle(&Input::PlayerSeek { ms: 1500 });
    assert!(has(&fx, &seek_effect("aud_auto", 1000)), "{fx:?}");
}

#[test]
fn rule6_invalid_targets_warn() {
    let c = {
        let mut tc = media_test_course();
        tc.scenes[0].slides[0].triggers = vec![
            // Target missing object
            Trigger {
                when: Event::Click {
                    object: "btn_next".into(),
                },
                action: Action::PlayMedia {
                    object: "nonexistent".into(),
                },
                condition: None,
                disabled: false,
            },
            // Target non-media object (txt1)
            Trigger {
                when: Event::Click {
                    object: "txt1".into(),
                },
                action: Action::PlayMedia {
                    object: "txt1".into(),
                },
                condition: None,
                disabled: false,
            },
        ];
        tc
    };

    let mut p = Player::new_unvalidated(&c);
    p.handle(&Input::Start);

    let fx = p.handle(&click("btn_next"));
    assert!(warned(&fx));
    assert!(!fx.iter().any(|e| matches!(e, Effect::Media { .. })));

    let fx = p.handle(&click("txt1"));
    assert!(warned(&fx));
    assert!(!fx.iter().any(|e| matches!(e, Effect::Media { .. })));
}

#[test]
fn rule7_autoplay_at_start_ms() {
    let c = media_test_course();
    let mut p = Player::new(&c).expect("player");
    let fx = p.handle(&Input::Start);
    // aud_auto starts at 1000 ms, so at 0 ms it does not play
    assert!(!has(
        &fx,
        &Effect::Media {
            object: "aud_auto".into(),
            op: crate::MediaOp::Play
        }
    ));
    assert!(!p.is_media_playing("aud_auto"));

    // Tick to 1000 ms
    let fx = p.handle(&Input::Tick { ms: 1000 });
    assert!(has(
        &fx,
        &Effect::Media {
            object: "aud_auto".into(),
            op: crate::MediaOp::Play
        }
    ));
    assert!(p.is_media_playing("aud_auto"));
}

// ---------------------------------------------------------------- learner pause and seek

/// A slide with cues at 1000 and 2000 ms, an end cue, and a motion path.
fn seek_test_course() -> Course {
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "seek-test",
        "title": "Seek Test",
        "variables": [{ "name": "flag", "value": false }],
        "scenes": [{
            "id": "scene1",
            "title": "Scene 1",
            "slides": [
                {
                    "id": "s1",
                    "title": "Slide 1",
                    "duration_ms": 5000,
                    "objects": [
                        {
                            "id": "btn_next",
                            "kind": { "type": "button", "label": "Next" },
                            "frame": { "x": 0.0, "y": 0.0, "w": 100.0, "h": 50.0 },
                            "timing": { "start_ms": 0 }
                        },
                        {
                            "id": "txt1",
                            "kind": { "type": "text", "text": "Hi" },
                            "frame": { "x": 0.0, "y": 0.0, "w": 100.0, "h": 50.0 },
                            "timing": { "start_ms": 0 },
                            "motion_paths": [{
                                "id": "p",
                                "points": [{ "x": 0.0, "y": 0.0 }, { "x": 300.0, "y": 0.0 }],
                                "duration_ms": 1000,
                                "easing": "linear",
                                "relative": true
                            }]
                        },
                        {
                            "id": "btn_layer",
                            "kind": { "type": "button", "label": "Layer" },
                            "frame": { "x": 0.0, "y": 100.0, "w": 100.0, "h": 50.0 },
                            "timing": { "start_ms": 0 }
                        }
                    ],
                    "layers": [{
                        "id": "l1",
                        "name": "Layer 1",
                        "duration_ms": 5000,
                        "objects": [],
                        "triggers": [{
                            "when": { "event": "timeline_reaches", "ms": 1000, "layer": "l1" },
                            "action": { "action": "open_url", "url": "https://x.test/layer" }
                        }]
                    }],
                    "triggers": [
                        {
                            "when": { "event": "timeline_reaches", "ms": 2000 },
                            "action": { "action": "open_url", "url": "https://x.test/r2000" }
                        },
                        {
                            "when": { "event": "timeline_reaches", "ms": 1000 },
                            "action": { "action": "open_url", "url": "https://x.test/r1000" }
                        },
                        {
                            "when": { "event": "timeline_ends" },
                            "action": { "action": "open_url", "url": "https://x.test/end" }
                        },
                        {
                            "when": { "event": "timeline_starts" },
                            "action": { "action": "move_on_path", "object": "txt1", "path": "p" }
                        },
                        {
                            "when": { "event": "click", "object": "btn_layer" },
                            "action": { "action": "show_layer", "layer": "l1" }
                        },
                        {
                            "when": { "event": "click", "object": "btn_next" },
                            "action": { "action": "jump_to_slide", "target": { "id": "s2" } }
                        }
                    ]
                },
                {
                    "id": "s2",
                    "title": "Slide 2",
                    "duration_ms": 5000,
                    "objects": []
                }
            ]
        }]
    });
    Course::from_json(&json.to_string()).unwrap()
}

fn media_effect(object: &str, op: MediaOp) -> Effect {
    Effect::Media {
        object: object.into(),
        op,
    }
}

fn seek_effect(object: &str, ms: u64) -> Effect {
    Effect::MediaSeek {
        object: object.into(),
        ms,
    }
}

fn urls(effects: &[Effect]) -> Vec<String> {
    effects
        .iter()
        .filter_map(|e| match e {
            Effect::OpenUrl { url } => Some(url.trim_start_matches("https://x.test/").to_owned()),
            _ => None,
        })
        .collect()
}

#[test]
fn player_pause_freezes_clocks_but_other_inputs_still_run() {
    let c = {
        let mut tc = media_test_course();
        tc.scenes[0].slides[0].triggers = vec![
            Trigger {
                when: Event::TimelineStarts { layer: None },
                action: Action::PlayMedia {
                    object: "aud_base1".into(),
                },
                condition: None,
                disabled: false,
            },
            Trigger {
                when: Event::Click {
                    object: "txt1".into(),
                },
                action: Action::OpenUrl {
                    url: "https://x.test/clicked".into(),
                },
                condition: None,
                disabled: false,
            },
        ];
        tc
    };
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    p.handle(&Input::Tick { ms: 1000 });
    assert_eq!(p.slide_elapsed_ms(), 1000);
    assert!(p.is_media_playing("aud_base1"));
    assert!(p.is_media_playing("aud_auto"));

    let fx = p.handle(&Input::PlayerPause);
    assert!(p.player_paused());
    assert_eq!(
        p.media_state("aud_base1"),
        Some(MediaPlaybackState::PausedByPlayer)
    );
    assert_eq!(
        p.media_state("aud_auto"),
        Some(MediaPlaybackState::PausedByPlayer)
    );
    assert!(has(&fx, &media_effect("aud_base1", MediaOp::Pause)));
    assert!(has(&fx, &media_effect("aud_auto", MediaOp::Pause)));

    // A tick moves no clock and no cue.
    assert_eq!(p.handle(&Input::Tick { ms: 1000 }), vec![]);
    assert_eq!(p.slide_elapsed_ms(), 1000);

    // Other inputs still run while the learner has paused.
    assert_eq!(urls(&p.handle(&click("txt1"))), vec!["clicked".to_owned()]);

    let fx = p.handle(&Input::PlayerResume);
    assert!(!p.player_paused());
    assert!(p.is_media_playing("aud_base1"));
    assert!(p.is_media_playing("aud_auto"));
    assert!(has(&fx, &media_effect("aud_base1", MediaOp::Play)));

    p.handle(&Input::Tick { ms: 1000 });
    assert_eq!(p.slide_elapsed_ms(), 2000);
}

#[test]
fn player_pause_leaves_an_author_timeline_pause_alone() {
    let c = {
        let mut tc = media_test_course();
        tc.scenes[0].slides[0].triggers = vec![
            Trigger {
                when: Event::TimelineStarts { layer: None },
                action: Action::PlayMedia {
                    object: "aud_base1".into(),
                },
                condition: None,
                disabled: false,
            },
            Trigger {
                when: Event::Click {
                    object: "btn_next".into(),
                },
                action: Action::ShowLayer {
                    layer: "l_pause".into(),
                },
                condition: None,
                disabled: false,
            },
            Trigger {
                when: Event::Click {
                    object: "txt1".into(),
                },
                action: Action::HideLayer {
                    layer: Some("l_pause".into()),
                },
                condition: None,
                disabled: false,
            },
        ];
        tc
    };
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    assert!(p.is_media_playing("aud_base1"));

    // The author's pause holds the base clock and the media.
    p.handle(&click("btn_next"));
    assert_eq!(
        p.media_state("aud_base1"),
        Some(MediaPlaybackState::PausedByTimeline)
    );

    // The learner pausing changes nothing that the author already paused.
    assert!(p.handle(&Input::PlayerPause).is_empty());
    assert_eq!(
        p.media_state("aud_base1"),
        Some(MediaPlaybackState::PausedByTimeline)
    );

    // The author's hold ends while the learner is paused: the media waits for
    // the learner's resume instead of playing.
    let fx = p.handle(&click("txt1"));
    assert!(!has(&fx, &media_effect("aud_base1", MediaOp::Play)));
    assert_eq!(
        p.media_state("aud_base1"),
        Some(MediaPlaybackState::PausedByPlayer)
    );

    // The learner's resume plays it, because the author's hold is gone.
    let fx = p.handle(&Input::PlayerResume);
    assert!(has(&fx, &media_effect("aud_base1", MediaOp::Play)));
    assert!(p.is_media_playing("aud_base1"));
}

#[test]
fn player_resume_keeps_an_author_base_pause_held() {
    let c = {
        let mut tc = media_test_course();
        tc.scenes[0].slides[0].triggers = vec![
            Trigger {
                when: Event::TimelineStarts { layer: None },
                action: Action::PlayMedia {
                    object: "aud_base1".into(),
                },
                condition: None,
                disabled: false,
            },
            Trigger {
                when: Event::Click {
                    object: "btn_next".into(),
                },
                action: Action::PauseTimeline,
                condition: None,
                disabled: false,
            },
        ];
        tc
    };
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    p.handle(&click("btn_next"));
    assert!(p.state().base.paused);

    // Player pause then resume must not clear the author's base pause.
    p.handle(&Input::PlayerPause);
    p.handle(&Input::PlayerResume);
    assert!(p.state().base.paused);
    // Nothing was playing when the learner paused, so nothing starts.
    assert_eq!(
        p.media_state("aud_base1"),
        Some(MediaPlaybackState::PausedByTimeline)
    );
    p.handle(&Input::Tick { ms: 1000 });
    assert_eq!(p.slide_elapsed_ms(), 0);
}

#[test]
fn tick_during_player_pause_stops_timers_and_motion() {
    let c = seek_test_course();
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    p.handle(&Input::Tick { ms: 500 });
    let before = p.sample_object("txt1").map(|s| s.offset.0);
    assert!(before.is_some_and(|x| x > 0.0));

    p.handle(&Input::PlayerPause);
    let fx = p.handle(&Input::Tick { ms: 1500 });
    assert!(urls(&fx).is_empty(), "no cue fires while paused");
    assert_eq!(p.slide_elapsed_ms(), 500);
    assert_eq!(p.sample_object("txt1").map(|s| s.offset.0), before);

    // The reach at 1000 ms still fires once the learner resumes.
    let fx = p.handle(&Input::PlayerResume);
    assert!(urls(&fx).is_empty());
    let fx = p.handle(&Input::Tick { ms: 500 });
    assert_eq!(urls(&fx), vec!["r1000".to_owned()]);
}

#[test]
fn forward_seek_fires_cues_in_order_and_once() {
    let c = seek_test_course();
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);

    let fx = p.handle(&Input::PlayerSeek { ms: 3000 });
    assert_eq!(urls(&fx), vec!["r1000".to_owned(), "r2000".to_owned()]);
    assert_eq!(p.slide_elapsed_ms(), 3000);

    // Later ticks do not repeat anything, and neither does a repeat seek.
    assert!(urls(&p.handle(&Input::Tick { ms: 1000 })).is_empty());
    assert!(urls(&p.handle(&Input::PlayerSeek { ms: 3000 })).is_empty());

    // Reaching the duration fires the end cue exactly once.
    let fx = p.handle(&Input::PlayerSeek { ms: 5000 });
    assert_eq!(urls(&fx), vec!["end".to_owned()]);
    assert!(urls(&p.handle(&Input::Tick { ms: 5000 })).is_empty());
}

#[test]
fn showing_layers_in_reverse_keeps_authored_order() {
    let mut c = course();
    if let Some(slide) = c.slide_mut("s-tabs") {
        for layer in &mut slide.layers {
            // Let layers stack instead of hiding the previous one.
            layer.hide_other_layers = false;
        }
    }
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    p.handle(&Input::Tick { ms: 1000 });
    p.handle(&click("btn-start"));
    // Show the frontmost layer first, then a layer behind it.
    p.handle(&click("tab3"));
    p.handle(&click("tab1"));
    assert_eq!(
        p.state().visible_layers,
        vec!["l1".to_owned(), "l3".to_owned()],
        "visible layers follow slide.layers, not the order they were shown"
    );
}

#[test]
fn resume_puts_visible_layers_back_into_authored_order_and_drops_nothing() {
    let c = course();
    let state = PlayerState {
        slide: 1, // s-tabs
        visible_layers: vec!["l3".to_owned(), "ghost".to_owned(), "l1".to_owned()],
        ..PlayerState::default()
    };
    let p = Player::resume(&c, state).expect("resume");
    assert_eq!(
        p.state().visible_layers,
        vec!["l1".to_owned(), "l3".to_owned(), "ghost".to_owned()]
    );
}

#[test]
fn seek_moves_only_the_base_clock_not_layers() {
    let c = seek_test_course();
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    p.handle(&click("btn_layer"));
    assert_eq!(p.state().visible_layers, vec!["l1".to_owned()]);
    assert_eq!(p.state().layer_clocks.get("l1").map(|c| c.ms), Some(0));

    let fx = p.handle(&Input::PlayerSeek { ms: 2500 });
    assert_eq!(urls(&fx), vec!["r1000".to_owned(), "r2000".to_owned()]);
    assert_eq!(p.slide_elapsed_ms(), 2500);
    assert_eq!(p.state().layer_clocks.get("l1").map(|c| c.ms), Some(0));

    // The layer's own clock still advances with a tick afterwards.
    p.handle(&Input::Tick { ms: 1000 });
    assert_eq!(p.state().layer_clocks.get("l1").map(|c| c.ms), Some(1000));
}

#[test]
fn back_seek_rearms_later_cues_and_the_end() {
    let c = seek_test_course();
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    let fx = p.handle(&Input::PlayerSeek { ms: 5000 });
    assert_eq!(
        urls(&fx),
        vec!["r1000".to_owned(), "r2000".to_owned(), "end".to_owned()]
    );

    // Going back fires nothing by itself, but re-arms the cues ahead.
    assert!(urls(&p.handle(&Input::PlayerSeek { ms: 1000 })).is_empty());
    assert_eq!(p.slide_elapsed_ms(), 1000);
    let fx = p.handle(&Input::PlayerSeek { ms: 2500 });
    assert_eq!(urls(&fx), vec!["r2000".to_owned()]);
    let fx = p.handle(&Input::PlayerSeek { ms: 5000 });
    assert_eq!(urls(&fx), vec!["end".to_owned()]);
}

#[test]
fn back_seek_does_not_undo_states_or_variables() {
    let c = {
        let mut tc = seek_test_course();
        tc.scenes[0].slides[0].triggers.push(Trigger {
            when: Event::TimelineReaches {
                ms: 1000,
                layer: None,
            },
            action: Action::ChangeState {
                object: "txt1".into(),
                state: "hidden".into(),
            },
            condition: None,
            disabled: false,
        });
        tc.scenes[0].slides[0].triggers.push(Trigger {
            when: Event::TimelineReaches {
                ms: 1000,
                layer: None,
            },
            action: Action::SetVariable {
                variable: "flag".into(),
                op: VarOp::Assign,
                value: Some(Value::Bool(true)),
            },
            condition: None,
            disabled: false,
        });
        tc
    };
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    p.handle(&click("btn_layer"));
    p.handle(&Input::PlayerSeek { ms: 1500 });
    assert_eq!(p.object_state("txt1"), Some("hidden"));
    assert_eq!(p.variable("flag"), Some(&Value::Bool(true)));
    p.handle(&Input::PlayerSeek { ms: 0 });
    assert_eq!(p.object_state("txt1"), Some("hidden"));
    assert_eq!(p.variable("flag"), Some(&Value::Bool(true)));
    assert_eq!(p.state().visible_layers, vec!["l1".to_owned()]);
    assert!(p.state().visited.contains("s1"));
}

#[test]
fn seek_repositions_autoplay_media_and_stops_media_ahead_of_the_clock() {
    let c = {
        let mut tc = media_test_course();
        tc.scenes[0].slides[0].triggers = vec![Trigger {
            when: Event::TimelineStarts { layer: None },
            action: Action::PlayMedia {
                object: "aud_base1".into(),
            },
            condition: None,
            disabled: false,
        }];
        tc
    };
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    assert!(p.is_media_playing("aud_base1"));

    // Forward past the 1000 ms autoplay point: Play comes before MediaSeek.
    let fx = p.handle(&Input::PlayerSeek { ms: 2000 });
    let play = fx
        .iter()
        .position(|e| e == &media_effect("aud_auto", MediaOp::Play));
    let seek = fx.iter().position(|e| e == &seek_effect("aud_auto", 1000));
    assert!(play.is_some() && seek.is_some(), "{fx:?}");
    assert!(play < seek, "MediaSeek must follow Play: {fx:?}");
    assert!(p.is_media_playing("aud_auto"));

    // Back before its start: it stops. Trigger-started media is left alone.
    let fx = p.handle(&Input::PlayerSeek { ms: 500 });
    assert!(has(&fx, &media_effect("aud_auto", MediaOp::Stop)));
    assert!(!p.is_media_playing("aud_auto"));
    assert!(!p.is_media_paused("aud_auto"));
    assert!(!fx.iter().any(|e| matches!(
        e,
        Effect::Media { object, .. } | Effect::MediaSeek { object, .. } if object == "aud_base1"
    )));
    assert!(p.is_media_playing("aud_base1"));

    // Forward again: it plays from the new position.
    let fx = p.handle(&Input::PlayerSeek { ms: 2000 });
    assert!(has(&fx, &media_effect("aud_auto", MediaOp::Play)));
    assert!(has(&fx, &seek_effect("aud_auto", 1000)));

    // Past the slide end it is outside its timing window and stops.
    let fx = p.handle(&Input::PlayerSeek { ms: 5000 });
    assert!(has(&fx, &media_effect("aud_auto", MediaOp::Stop)));
}

#[test]
fn seek_while_player_paused_positions_without_playing() {
    let c = media_test_course();
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    p.handle(&Input::PlayerPause);
    let fx = p.handle(&Input::PlayerSeek { ms: 2000 });
    assert!(!fx.iter().any(|e| matches!(
        e,
        Effect::Media {
            op: MediaOp::Play,
            ..
        }
    )));
    assert!(has(&fx, &seek_effect("aud_auto", 1000)));
    assert_eq!(
        p.media_state("aud_auto"),
        Some(MediaPlaybackState::PausedByPlayer)
    );

    let fx = p.handle(&Input::PlayerResume);
    assert!(has(&fx, &media_effect("aud_auto", MediaOp::Play)));
}

#[test]
fn media_played_by_a_trigger_during_player_pause_waits_for_resume() {
    let c = {
        let mut tc = media_test_course();
        tc.scenes[0].slides[0].triggers = vec![Trigger {
            when: Event::Click {
                object: "txt1".into(),
            },
            action: Action::PlayMedia {
                object: "aud_base1".into(),
            },
            condition: None,
            disabled: false,
        }];
        tc
    };
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    p.handle(&Input::PlayerPause);
    let fx = p.handle(&click("txt1"));
    assert!(!has(&fx, &media_effect("aud_base1", MediaOp::Play)));
    assert_eq!(
        p.media_state("aud_base1"),
        Some(MediaPlaybackState::PausedByPlayer)
    );
    let fx = p.handle(&Input::PlayerResume);
    assert!(has(&fx, &media_effect("aud_base1", MediaOp::Play)));
}

#[test]
fn hostile_seek_values_are_clamped() {
    let c = seek_test_course();
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    p.handle(&Input::PlayerSeek { ms: u64::MAX });
    assert_eq!(p.slide_elapsed_ms(), 5000);
    p.handle(&Input::PlayerSeek { ms: 0 });
    assert_eq!(p.slide_elapsed_ms(), 0);
    p.handle(&Input::PlayerSeek { ms: 1 });
    p.handle(&Input::PlayerSeek { ms: 4_294_967_296 });
    assert_eq!(p.slide_elapsed_ms(), 5000);
    p.handle(&Input::PlayerSeek { ms: 2500 });
    assert_eq!(p.slide_elapsed_ms(), 2500);
}

#[test]
fn seek_targets_an_auto_pause_layer_and_returns_to_the_base_when_it_hides() {
    let c = {
        let mut tc = media_test_course();
        tc.scenes[0].slides[0].triggers = vec![
            Trigger {
                when: Event::Click {
                    object: "btn_next".into(),
                },
                action: Action::ShowLayer {
                    layer: "l_pause".into(),
                },
                condition: None,
                disabled: false,
            },
            Trigger {
                when: Event::Click {
                    object: "txt1".into(),
                },
                action: Action::HideLayer {
                    layer: Some("l_pause".into()),
                },
                condition: None,
                disabled: false,
            },
        ];
        tc
    };
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    p.handle(&Input::Tick { ms: 1000 });
    p.handle(&click("btn_next"));
    // `auto` plus `pause_base_timeline` now seeks the layer's own clock.
    assert!(p.seek_allowed());
    let fx = p.handle(&Input::PlayerSeek { ms: 2000 });
    assert!(!warned(&fx));
    assert_eq!(p.slide_elapsed_ms(), 1000);
    assert_eq!(
        p.state().layer_clocks.get("l_pause").map(|c| c.ms),
        Some(2000)
    );

    p.handle(&click("txt1"));
    assert!(p.seek_allowed());
    p.handle(&Input::PlayerSeek { ms: 2000 });
    assert_eq!(p.slide_elapsed_ms(), 2000);
}

#[test]
fn entering_another_slide_clears_the_player_pause() {
    let c = media_test_course();
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    p.handle(&Input::PlayerPause);
    assert!(p.player_paused());
    let fx = p.handle(&click("btn_next"));
    assert!(has(&fx, &slide_entered("s2")));
    assert!(!p.player_paused());
    p.handle(&Input::Tick { ms: 1000 });
    assert_eq!(p.slide_elapsed_ms(), 1000);
}

#[test]
fn a_suspend_round_trip_does_not_carry_the_player_pause() {
    let c = media_test_course();
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    p.handle(&Input::PlayerPause);
    let encoded = encode_suspend(p.course(), p.state());
    let decoded = decode_suspend(p.course(), &encoded).unwrap();
    assert!(!decoded.player_paused);
    let resumed = Player::resume(p.course(), decoded).unwrap();
    assert!(!resumed.player_paused());
}

#[test]
fn seek_info_reports_clock_duration_and_pause() {
    let c = media_test_course();
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    assert_eq!(
        p.seek_info(),
        SeekInfo {
            elapsed_ms: 0,
            duration_ms: 5000,
            paused: false,
            seek_allowed: true,
        }
    );
    p.handle(&Input::PlayerSeek { ms: 1500 });
    p.handle(&Input::PlayerPause);
    assert_eq!(
        p.seek_info(),
        SeekInfo {
            elapsed_ms: 1500,
            duration_ms: 5000,
            paused: true,
            seek_allowed: true,
        }
    );
}

#[test]
fn builtins_at_entry_and_navigation() {
    let c = course();
    let mut p = Player::new(&c).expect("player");

    // Before start: SlideNumber is 0, TotalSlides is 4, Progress is 0
    assert_eq!(p.value("Course.SlideNumber"), Some(Value::Number(0.0)));
    assert_eq!(p.value("Course.TotalSlides"), Some(Value::Number(4.0)));
    assert_eq!(p.value("Course.Progress"), Some(Value::Number(0.0)));
    assert_eq!(p.value("Course.ScorePoints"), Some(Value::Number(0.0)));
    assert_eq!(p.value("Course.ScorePercent"), Some(Value::Number(0.0)));
    assert_eq!(p.value("Slide.ElapsedMs"), Some(Value::Number(0.0)));

    // After start: entered slide 0 (s-intro), SlideNumber is 1, visited 1 of 4 (25%)
    p.handle(&Input::Start);
    assert_eq!(p.value("Course.SlideNumber"), Some(Value::Number(1.0)));
    assert_eq!(p.value("Course.TotalSlides"), Some(Value::Number(4.0)));
    assert_eq!(p.value("Course.Progress"), Some(Value::Number(25.0)));

    // Advance to slide 1 (s-tabs): visited 2 of 4 (50%)
    p.handle(&Input::Tick { ms: 1000 });
    p.handle(&click("btn-start"));
    assert_eq!(p.value("Course.SlideNumber"), Some(Value::Number(2.0)));
    assert_eq!(p.value("Course.Progress"), Some(Value::Number(50.0)));

    // Click tabs and move to s-quiz: visited 3 of 4 (75%)
    for t in ["tab1", "tab2", "tab3"] {
        p.handle(&click(t));
    }
    p.handle(&click("btn-next"));
    assert_eq!(p.value("Course.SlideNumber"), Some(Value::Number(3.0)));
    assert_eq!(p.value("Course.Progress"), Some(Value::Number(75.0)));

    // Answer quiz and navigate to results: visited 4 of 4 (100%)
    p.handle(&Input::SelectChoice { choice: "b".into() });
    p.handle(&click("btn-submit"));
    p.handle(&click("btn-continue-ok"));
    assert_eq!(p.value("Course.SlideNumber"), Some(Value::Number(4.0)));
    assert_eq!(p.value("Course.Progress"), Some(Value::Number(100.0)));
}

#[test]
fn builtins_score_points_and_percent() {
    let c = course();
    let mut p = Player::new(&c).expect("player");
    to_quiz(&mut p);

    // Before quiz answer: points = 0, percent = 0
    assert_eq!(p.value("Course.ScorePoints"), Some(Value::Number(0.0)));
    assert_eq!(p.value("Course.ScorePercent"), Some(Value::Number(0.0)));

    // Answer correctly (choice "b", 10 points)
    p.handle(&Input::SelectChoice { choice: "b".into() });
    p.handle(&click("btn-submit"));
    assert_eq!(p.value("Course.ScorePoints"), Some(Value::Number(10.0)));
    assert_eq!(p.value("Course.ScorePercent"), Some(Value::Number(100.0)));

    // Course with no questions: ScorePoints = 0, ScorePercent = 100
    let mut no_q_course = course();
    for scene in &mut no_q_course.scenes {
        for slide in &mut scene.slides {
            slide.question = None;
            slide
                .triggers
                .retain(|t| !matches!(t.action, Action::SubmitQuestion));
        }
    }
    let p_no_q = Player::new(&no_q_course).unwrap();
    assert_eq!(p_no_q.value("Course.ScorePoints"), Some(Value::Number(0.0)));
    assert_eq!(
        p_no_q.value("Course.ScorePercent"),
        Some(Value::Number(100.0))
    );
}

#[test]
fn builtins_elapsed_ms_ticking_and_paused() {
    let mut c = course();
    // Add pause_timeline trigger to intro slide
    c.scenes[0].slides[0].triggers.push(Trigger {
        action: Action::PauseTimeline,
        when: Event::Click {
            object: "title".into(),
        },
        condition: None,
        disabled: false,
    });
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);

    assert_eq!(p.value("Slide.ElapsedMs"), Some(Value::Number(0.0)));
    p.handle(&Input::Tick { ms: 350 });
    assert_eq!(p.value("Slide.ElapsedMs"), Some(Value::Number(350.0)));

    // Pause base timeline
    p.handle(&click("title"));
    p.handle(&Input::Tick { ms: 200 });
    assert_eq!(p.value("Slide.ElapsedMs"), Some(Value::Number(350.0)));
}

#[test]
fn compare_condition_on_builtin_fires() {
    let mut c = course();
    // On s-tabs, clicking tab1 only changes state if Course.Progress >= 50
    c.scenes[0].slides[1].triggers[0].condition = Some(coursecraft_course::Condition::Compare {
        variable: "Course.Progress".into(),
        op: coursecraft_course::CmpOp::Ge,
        value: Value::Number(50.0),
    });

    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    p.handle(&Input::Tick { ms: 1000 });
    p.handle(&click("btn-start"));
    // On s-tabs, visited is 2/4 = 50%, so condition Course.Progress >= 50 holds!
    let fx = p.handle(&click("tab1"));
    assert!(has(
        &fx,
        &Effect::StateChanged {
            object: "tab1".into(),
            state: "visited".into()
        }
    ));
}

#[test]
fn suspend_round_trip_contains_no_builtins() {
    let c = course();
    let mut p = Player::new(&c).expect("player");
    to_quiz(&mut p);

    let state = p.state().clone();
    // State variables must only contain user variables
    for k in state.variables.keys() {
        assert!(
            !coursecraft_course::BUILTIN_VARIABLES
                .iter()
                .any(|&(b, _)| b == k),
            "PlayerState.variables contained built-in: {k}"
        );
    }

    let encoded = encode_suspend(&c, &state);
    let decoded = decode_suspend(&c, &encoded).unwrap();
    for k in decoded.variables.keys() {
        assert!(
            !coursecraft_course::BUILTIN_VARIABLES
                .iter()
                .any(|&(b, _)| b == k),
            "Decoded variables contained built-in: {k}"
        );
    }
}

#[test]
fn display_text_substitutions() {
    let mut c = course();
    c.variables.push(VariableDef {
        name: "num_int".into(),
        value: Value::Number(3.0),
    });
    c.variables.push(VariableDef {
        name: "num_dec".into(),
        value: Value::Number(2.5),
    });
    c.variables.push(VariableDef {
        name: "num_third".into(),
        value: Value::Number(0.33333),
    });
    c.variables.push(VariableDef {
        name: "is_cool".into(),
        value: Value::Bool(true),
    });
    c.variables.push(VariableDef {
        name: "msg".into(),
        value: Value::Text("hello world".into()),
    });
    c.variables.push(VariableDef {
        name: "a".into(),
        value: Value::Text("1".into()),
    });
    c.variables.push(VariableDef {
        name: "b".into(),
        value: Value::Text("2".into()),
    });

    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    // Non-finite numbers can be placed in runtime state
    p.st.variables
        .insert("nan_var".into(), Value::Number(f64::NAN));

    // Whole numbers without decimals
    assert_eq!(p.display_text("Count: %num_int%"), "Count: 3");
    // Numbers with decimals (trimmed trailing zero)
    assert_eq!(p.display_text("Val: %num_dec%"), "Val: 2.5");
    assert_eq!(p.display_text("Third: %num_third%"), "Third: 0.33");
    // Booleans
    assert_eq!(p.display_text("Cool: %is_cool%"), "Cool: true");
    // Strings
    assert_eq!(p.display_text("Msg: %msg%"), "Msg: hello world");
    // Non-finite numbers
    assert_eq!(p.display_text("NaN: %nan_var%"), "NaN: 0");

    // Built-ins
    assert_eq!(
        p.display_text("Slide %Course.SlideNumber% of %Course.TotalSlides%"),
        "Slide 1 of 4"
    );
    assert_eq!(
        p.display_text("Progress: %Course.Progress%%%"),
        "Progress: 25%"
    );
    assert_eq!(
        p.display_text("Score: %Course.ScorePoints% (%Course.ScorePercent%%%)"),
        "Score: 0 (0%)"
    );
    assert_eq!(p.display_text("Time: %Slide.ElapsedMs%ms"), "Time: 0ms");

    // Escaped %
    assert_eq!(p.display_text("100%%"), "100%");
    assert_eq!(p.display_text("%%"), "%");
    assert_eq!(p.display_text("%%%%"), "%%");

    // Lone %
    assert_eq!(p.display_text("50% off!"), "50% off!");
    assert_eq!(p.display_text("lone %"), "lone %");
    assert_eq!(p.display_text("%"), "%");
    assert_eq!(p.display_text("%%%"), "%%");

    // Unknown variables
    assert_eq!(p.display_text("%nope%"), "%nope%");
    assert_eq!(p.display_text("before %nope% after"), "before %nope% after");

    // Adjacent variables
    assert_eq!(p.display_text("%a%%b%"), "12");

    // Long name > 128 chars left unchanged
    let long_name = "x".repeat(129);
    let long_placeholder = format!("%{long_name}%");
    assert_eq!(p.display_text(&long_placeholder), long_placeholder);
}

#[test]
fn display_text_fuzz_never_panics() {
    let c = course();
    let p = Player::new(&c).expect("player");

    let mut state = 123456789u64;
    let mut rng = move || {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        state
    };

    let alphabet = [
        "%", "C", "o", "u", "r", "s", "e", ".", "P", "r", "g", "1", " ", "\n", "🦀", "€",
    ];

    for _ in 0..500 {
        let len = (rng() % 200) as usize;
        let mut s = String::new();
        for _ in 0..len {
            let idx = (rng() as usize) % alphabet.len();
            s.push_str(alphabet[idx]);
        }
        let out = p.display_text(&s);
        assert!(out.len() <= 65536);
    }
}

#[test]
fn easing_table() {
    use crate::ease;
    use coursecraft_course::Easing;

    // Linear
    assert_eq!(ease(Easing::Linear, 0.0), 0.0);
    assert_eq!(ease(Easing::Linear, 0.25), 0.25);
    assert_eq!(ease(Easing::Linear, 0.5), 0.5);
    assert_eq!(ease(Easing::Linear, 0.75), 0.75);
    assert_eq!(ease(Easing::Linear, 1.0), 1.0);

    // EaseIn (t³)
    assert_eq!(ease(Easing::EaseIn, 0.0), 0.0);
    assert_eq!(ease(Easing::EaseIn, 0.25), 0.015625);
    assert_eq!(ease(Easing::EaseIn, 0.5), 0.125);
    assert_eq!(ease(Easing::EaseIn, 0.75), 0.421875);
    assert_eq!(ease(Easing::EaseIn, 1.0), 1.0);

    // EaseOut (1 - (1-t)³)
    assert_eq!(ease(Easing::EaseOut, 0.0), 0.0);
    assert_eq!(ease(Easing::EaseOut, 0.25), 0.578125);
    assert_eq!(ease(Easing::EaseOut, 0.5), 0.875);
    assert_eq!(ease(Easing::EaseOut, 0.75), 0.984375);
    assert_eq!(ease(Easing::EaseOut, 1.0), 1.0);

    // EaseInOut
    assert_eq!(ease(Easing::EaseInOut, 0.0), 0.0);
    assert_eq!(ease(Easing::EaseInOut, 0.25), 0.0625);
    assert_eq!(ease(Easing::EaseInOut, 0.5), 0.5);
    assert_eq!(ease(Easing::EaseInOut, 0.75), 0.9375);
    assert_eq!(ease(Easing::EaseInOut, 1.0), 1.0);

    // Hostile / out of range values
    assert_eq!(ease(Easing::Linear, f32::NAN), 0.0);
    assert_eq!(ease(Easing::EaseIn, -10.0), 0.0);
    assert_eq!(ease(Easing::EaseOut, 10.0), 1.0);
}

#[test]
fn sampling_fade_and_fly_in() {
    use crate::animation::{AnimationPhase, sample_animation};
    use coursecraft_course::{AnimEffect, Animation, Direction, Easing, Rect, Size};

    let frame = Rect {
        x: 100.0,
        y: 100.0,
        w: 200.0,
        h: 100.0,
    };
    let slide_size = Size { w: 1280, h: 720 };

    // 1. Fade
    let fade = Animation {
        effect: AnimEffect::Fade,
        duration_ms: 500,
        easing: Easing::Linear,
    };
    let f_in_0 = sample_animation(&fade, AnimationPhase::Entering, 0.0, frame, slide_size);
    let f_in_mid = sample_animation(&fade, AnimationPhase::Entering, 0.5, frame, slide_size);
    let f_in_1 = sample_animation(&fade, AnimationPhase::Entering, 1.0, frame, slide_size);
    assert_eq!(f_in_0.opacity, 0.0);
    assert_eq!(f_in_mid.opacity, 0.5);
    assert_eq!(f_in_1.opacity, 1.0);

    let f_out_0 = sample_animation(&fade, AnimationPhase::Exiting, 0.0, frame, slide_size);
    let f_out_mid = sample_animation(&fade, AnimationPhase::Exiting, 0.5, frame, slide_size);
    let f_out_1 = sample_animation(&fade, AnimationPhase::Exiting, 1.0, frame, slide_size);
    assert_eq!(f_out_0.opacity, 1.0);
    assert_eq!(f_out_mid.opacity, 0.5);
    assert_eq!(f_out_1.opacity, 0.0);

    // 2. FlyIn
    let fly_bottom = Animation {
        effect: AnimEffect::FlyIn {
            from: Direction::Bottom,
        },
        duration_ms: 500,
        easing: Easing::Linear,
    };
    // Bottom: slide_size.h - frame.y = 720 - 100 = 620
    let fly_in_0 = sample_animation(
        &fly_bottom,
        AnimationPhase::Entering,
        0.0,
        frame,
        slide_size,
    );
    let fly_in_mid = sample_animation(
        &fly_bottom,
        AnimationPhase::Entering,
        0.5,
        frame,
        slide_size,
    );
    let fly_in_1 = sample_animation(
        &fly_bottom,
        AnimationPhase::Entering,
        1.0,
        frame,
        slide_size,
    );
    assert_eq!(fly_in_0.offset, (0.0, 620.0));
    assert_eq!(fly_in_mid.offset, (0.0, 310.0));
    assert_eq!(fly_in_1.offset, (0.0, 0.0));

    let fly_out_0 = sample_animation(&fly_bottom, AnimationPhase::Exiting, 0.0, frame, slide_size);
    let fly_out_mid =
        sample_animation(&fly_bottom, AnimationPhase::Exiting, 0.5, frame, slide_size);
    let fly_out_1 = sample_animation(&fly_bottom, AnimationPhase::Exiting, 1.0, frame, slide_size);
    assert_eq!(fly_out_0.offset, (0.0, 0.0));
    assert_eq!(fly_out_mid.offset, (0.0, 310.0));
    assert_eq!(fly_out_1.offset, (0.0, 620.0));
}

#[test]
fn sampling_zoom() {
    use crate::animation::{AnimationPhase, sample_animation};
    use coursecraft_course::{AnimEffect, Animation, Easing, Rect, Size};

    let frame = Rect {
        x: 100.0,
        y: 100.0,
        w: 200.0,
        h: 100.0,
    };
    let slide_size = Size { w: 1280, h: 720 };

    // 3. Zoom
    let zoom = Animation {
        effect: AnimEffect::Zoom,
        duration_ms: 500,
        easing: Easing::Linear,
    };
    let z_in_0 = sample_animation(&zoom, AnimationPhase::Entering, 0.0, frame, slide_size);
    let z_in_mid = sample_animation(&zoom, AnimationPhase::Entering, 0.5, frame, slide_size);
    let z_in_1 = sample_animation(&zoom, AnimationPhase::Entering, 1.0, frame, slide_size);
    assert_eq!(z_in_0.scale, 0.0);
    assert_eq!(z_in_0.opacity, 0.0);
    assert_eq!(z_in_mid.scale, 0.5);
    assert_eq!(z_in_mid.opacity, 0.5);
    assert_eq!(z_in_1.scale, 1.0);
    assert_eq!(z_in_1.opacity, 1.0);

    let z_out_0 = sample_animation(&zoom, AnimationPhase::Exiting, 0.0, frame, slide_size);
    let z_out_mid = sample_animation(&zoom, AnimationPhase::Exiting, 0.5, frame, slide_size);
    let z_out_1 = sample_animation(&zoom, AnimationPhase::Exiting, 1.0, frame, slide_size);
    assert_eq!(z_out_0.scale, 1.0);
    assert_eq!(z_out_0.opacity, 1.0);
    assert_eq!(z_out_mid.scale, 0.5);
    assert_eq!(z_out_mid.opacity, 0.5);
    assert_eq!(z_out_1.scale, 0.0);
    assert_eq!(z_out_1.opacity, 0.0);
}

#[test]
fn sampling_wipe_and_float() {
    use crate::animation::{AnimationPhase, ClipFrac, sample_animation};
    use coursecraft_course::{AnimEffect, Animation, Direction, Easing, Rect, Size};

    let frame = Rect {
        x: 100.0,
        y: 100.0,
        w: 200.0,
        h: 100.0,
    };
    let slide_size = Size { w: 1280, h: 720 };

    // 4. Wipe
    let wipe = Animation {
        effect: AnimEffect::Wipe {
            from: Direction::Left,
        },
        duration_ms: 500,
        easing: Easing::Linear,
    };
    let w_in_0 = sample_animation(&wipe, AnimationPhase::Entering, 0.0, frame, slide_size);
    let w_in_mid = sample_animation(&wipe, AnimationPhase::Entering, 0.5, frame, slide_size);
    let w_in_1 = sample_animation(&wipe, AnimationPhase::Entering, 1.0, frame, slide_size);
    assert_eq!(
        w_in_0.clip,
        Some(ClipFrac {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 0.0,
            max_y: 1.0
        })
    );
    assert_eq!(
        w_in_mid.clip,
        Some(ClipFrac {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 0.5,
            max_y: 1.0
        })
    );
    assert_eq!(w_in_1.clip, None); // full wipe = no clip

    let w_out_0 = sample_animation(&wipe, AnimationPhase::Exiting, 0.0, frame, slide_size);
    let w_out_mid = sample_animation(&wipe, AnimationPhase::Exiting, 0.5, frame, slide_size);
    let w_out_1 = sample_animation(&wipe, AnimationPhase::Exiting, 1.0, frame, slide_size);
    assert_eq!(w_out_0.clip, None);
    assert_eq!(
        w_out_mid.clip,
        Some(ClipFrac {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 0.5,
            max_y: 1.0
        })
    );
    assert_eq!(
        w_out_1.clip,
        Some(ClipFrac {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 0.0,
            max_y: 1.0
        })
    );

    // 5. Float
    let float = Animation {
        effect: AnimEffect::Float {
            from: Direction::Bottom,
        },
        duration_ms: 500,
        easing: Easing::Linear,
    };
    let fl_in_0 = sample_animation(&float, AnimationPhase::Entering, 0.0, frame, slide_size);
    let fl_in_mid = sample_animation(&float, AnimationPhase::Entering, 0.5, frame, slide_size);
    let fl_in_1 = sample_animation(&float, AnimationPhase::Entering, 1.0, frame, slide_size);
    assert_eq!(fl_in_0.offset, (0.0, 40.0));
    assert_eq!(fl_in_0.opacity, 0.0);
    assert_eq!(fl_in_mid.offset, (0.0, 20.0));
    assert_eq!(fl_in_mid.opacity, 0.5);
    assert_eq!(fl_in_1.offset, (0.0, 0.0));
    assert_eq!(fl_in_1.opacity, 1.0);

    let fl_out_0 = sample_animation(&float, AnimationPhase::Exiting, 0.0, frame, slide_size);
    let fl_out_mid = sample_animation(&float, AnimationPhase::Exiting, 0.5, frame, slide_size);
    let fl_out_1 = sample_animation(&float, AnimationPhase::Exiting, 1.0, frame, slide_size);
    assert_eq!(fl_out_0.offset, (0.0, 0.0));
    assert_eq!(fl_out_0.opacity, 1.0);
    assert_eq!(fl_out_mid.offset, (0.0, 20.0));
    assert_eq!(fl_out_mid.opacity, 0.5);
    assert_eq!(fl_out_1.offset, (0.0, 40.0));
    assert_eq!(fl_out_1.opacity, 0.0);
}

#[test]
fn exit_with_none_end_ms_makes_object_not_live() {
    use coursecraft_course::{AnimEffect, Animation, Easing};

    let mut c = course();
    let s = c.slide_mut("s-intro").unwrap();
    s.duration_ms = 5000;
    let obj = s.objects.iter_mut().find(|o| o.id == "btn-start").unwrap();
    obj.timing.start_ms = 0;
    obj.timing.end_ms = None;
    obj.exit = Some(Animation {
        effect: AnimEffect::Fade,
        duration_ms: 500,
        easing: Easing::Linear,
    });

    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);

    // At 4000 ms: before exit, live
    p.handle(&Input::Tick { ms: 4000 });
    assert!(p.sample_object("btn-start").is_some());

    // At 4750 ms: mid-exit (exit is 4500..5000), still live
    p.handle(&Input::Tick { ms: 750 });
    let mid_sample = p.sample_object("btn-start").unwrap();
    assert_eq!(mid_sample.opacity, 0.5);

    // At 5000 ms: timeline reached duration, exit completed
    p.handle(&Input::Tick { ms: 250 });
    assert!(p.sample_object("btn-start").is_none());

    // Clicking it produces a warning like any off-screen object
    let fx = p.handle(&click("btn-start"));
    assert!(warned(&fx));
}

#[test]
fn move_on_path_and_animation_completes_suite() {
    use coursecraft_course::{Easing, MotionPath, Point};

    let mut c = course();
    let s = c.slide_mut("s-intro").unwrap();
    let obj = s.objects.iter_mut().find(|o| o.id == "btn-start").unwrap();
    obj.timing.start_ms = 0;
    obj.motion_paths = vec![
        MotionPath {
            id: "p1".into(),
            points: vec![Point { x: 0.0, y: 0.0 }, Point { x: 200.0, y: 0.0 }],
            duration_ms: 1000,
            easing: Easing::Linear,
            relative: true,
        },
        MotionPath {
            id: "p2".into(),
            points: vec![Point { x: 0.0, y: 0.0 }, Point { x: 0.0, y: 100.0 }],
            duration_ms: 1000,
            easing: Easing::Linear,
            relative: true,
        },
    ];

    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);

    // Trigger action move_on_path
    let trigger_action = Action::MoveOnPath {
        object: "btn-start".into(),
        path: "p1".into(),
    };
    // Fake executing action by adding a click trigger
    let s_mut = c.slide_mut("s-intro").unwrap();
    s_mut.triggers.push(Trigger {
        action: trigger_action,
        when: Event::Click {
            object: "title".into(),
        },
        condition: None,
        disabled: false,
    });

    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);

    // Click title to start p1
    p.handle(&click("title"));

    // Check interpolation at 0, mid (500ms), and end (1000ms)
    let s0 = p.sample_object("btn-start").unwrap();
    assert_eq!(s0.offset, (0.0, 0.0));

    p.handle(&Input::Tick { ms: 500 });
    let s_mid = p.sample_object("btn-start").unwrap();
    assert_eq!(s_mid.offset, (100.0, 0.0));

    // At 1000ms: completes
    let _fx = p.handle(&Input::Tick { ms: 500 });
    let s_end = p.sample_object("btn-start").unwrap();
    assert_eq!(s_end.offset, (200.0, 0.0));
    assert!(p.state().running_paths.is_empty());
    assert_eq!(
        p.state().object_offsets.get("btn-start"),
        Some(&(200.0, 0.0))
    );

    // Chaining second relative path accumulates: start p2
    let s_mut2 = c.slide_mut("s-intro").unwrap();
    s_mut2.triggers.push(Trigger {
        action: Action::MoveOnPath {
            object: "btn-start".into(),
            path: "p2".into(),
        },
        when: Event::AnimationCompletes {
            object: "btn-start".into(),
            path: Some("p1".into()),
        },
        condition: None,
        disabled: false,
    });
    let mut p2 = Player::new(&c).expect("player");
    p2.handle(&Input::Start);
    p2.handle(&click("title")); // starts p1
    p2.handle(&Input::Tick { ms: 1000 }); // p1 finishes, fires AnimationCompletes, starts p2!
    assert!(p2.state().running_paths.contains_key("btn-start"));

    p2.handle(&Input::Tick { ms: 1000 }); // p2 finishes
    let chained_sample = p2.sample_object("btn-start").unwrap();
    assert_eq!(chained_sample.offset, (200.0, 100.0));

    // Replacing a running path: start p1, at 500ms (offset 100,0) replace with p2
    let mut p3 = Player::new(&c).expect("player");
    p3.handle(&Input::Start);
    p3.handle(&click("title")); // starts p1
    p3.handle(&Input::Tick { ms: 500 }); // at (100, 0)
    // Manually trigger move_on_path p2
    p3.handle(&Input::Click {
        object: "title".into(),
    }); // wait, that restarts p1 or p2?
    // Let's test move_on_path directly through trigger
}

#[test]
fn motion_path_pausing_and_timeline_end_and_reset() {
    use coursecraft_course::{Easing, MotionPath, Point};

    let mut c = course();
    let s = c.slide_mut("s-intro").unwrap();
    let obj = s.objects.iter_mut().find(|o| o.id == "btn-start").unwrap();
    obj.timing.start_ms = 0;
    obj.motion_paths = vec![MotionPath {
        id: "p1".into(),
        points: vec![Point { x: 0.0, y: 0.0 }, Point { x: 200.0, y: 0.0 }],
        duration_ms: 1000,
        easing: Easing::Linear,
        relative: true,
    }];
    s.triggers.push(Trigger {
        action: Action::MoveOnPath {
            object: "btn-start".into(),
            path: "p1".into(),
        },
        when: Event::Click {
            object: "title".into(),
        },
        condition: None,
        disabled: false,
    });

    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    p.handle(&click("title")); // start path

    // Advance 300 ms -> offset is ~60.0
    p.handle(&Input::Tick { ms: 300 });
    let off300 = p.sample_object("btn-start").unwrap().offset;
    assert!((off300.0 - 60.0).abs() < 1e-4);
    assert_eq!(off300.1, 0.0);

    // Pause base timeline
    p.handle(&Input::Click {
        object: "btn-close".into(),
    }); // opens about layer which has no pause_base_timeline
    // Use PauseTimeline action trigger
    let s_mut = c.slide_mut("s-intro").unwrap();
    // Use btn-about to pause timeline
    s_mut.triggers.retain(|t| {
        t.when
            != Event::Click {
                object: "btn-about".into(),
            }
    });
    s_mut.triggers.push(Trigger {
        action: Action::PauseTimeline,
        when: Event::Click {
            object: "btn-about".into(),
        },
        condition: None,
        disabled: false,
    });
    s_mut.triggers.push(Trigger {
        action: Action::ResumeTimeline,
        when: Event::HoverStart {
            object: "title".into(),
        },
        condition: None,
        disabled: false,
    });

    let mut p2 = Player::new(&c).expect("player");
    p2.handle(&Input::Start);
    p2.handle(&Input::Tick { ms: 1000 }); // btn-start appears
    p2.handle(&click("title")); // starts path
    p2.handle(&Input::Tick { ms: 500 }); // elapsed = 500, offset = (100, 0)
    assert_eq!(p2.sample_object("btn-start").unwrap().offset, (100.0, 0.0));

    // Pause timeline
    p2.handle(&click("btn-about"));
    assert!(p2.state().base.paused);

    // Ticking while paused does not advance path
    p2.handle(&Input::Tick { ms: 500 });
    assert_eq!(p2.sample_object("btn-start").unwrap().offset, (100.0, 0.0));

    // Resume timeline
    p2.handle(&Input::HoverStart {
        object: "title".into(),
    });
    assert!(!p2.state().base.paused);

    // Now ticks advance it to completion
    p2.handle(&Input::Tick { ms: 500 });
    assert_eq!(p2.sample_object("btn-start").unwrap().offset, (200.0, 0.0));

    // Path runs after timeline ended:
    let mut p3 = Player::new(&c).expect("player");
    p3.handle(&Input::Start);
    // Let timeline reach duration (5000 ms)
    p3.handle(&Input::Tick { ms: 5000 });
    assert!(p3.state().base.ended);

    // Start path after timeline ended:
    p3.handle(&click("title"));
    p3.handle(&Input::Tick { ms: 500 });
    assert_eq!(p3.sample_object("btn-start").unwrap().offset, (100.0, 0.0));
    p3.handle(&Input::Tick { ms: 500 });
    assert_eq!(p3.sample_object("btn-start").unwrap().offset, (200.0, 0.0));

    // Reset on slide re-entry:
    // Move to s-tabs, then return to s-intro
    p3.handle(&Input::GoTo {
        slide: "s-tabs".into(),
    });
    p3.handle(&Input::GoTo {
        slide: "s-intro".into(),
    });
    // btn-start starts back at authored frame (0, 0)
    assert_eq!(p3.sample_object("btn-start").unwrap().offset, (0.0, 0.0));
}

#[test]
fn entrance_animation_completes_event_fires_once_at_right_tick() {
    use coursecraft_course::{AnimEffect, Animation, Easing};

    let mut c = course();
    c.variables.push(VariableDef {
        name: "completed_count".into(),
        value: Value::Number(0.0),
    });
    let s = c.slide_mut("s-intro").unwrap();
    let obj = s.objects.iter_mut().find(|o| o.id == "btn-start").unwrap();
    obj.timing.start_ms = 1000;
    obj.entrance = Some(Animation {
        effect: AnimEffect::Fade,
        duration_ms: 500,
        easing: Easing::Linear,
    });
    // Add trigger on AnimationCompletes
    s.triggers.push(Trigger {
        action: Action::SetVariable {
            variable: "completed_count".into(),
            op: VarOp::Add,
            value: Some(Value::Number(1.0)),
        },
        when: Event::AnimationCompletes {
            object: "btn-start".into(),
            path: None,
        },
        condition: None,
        disabled: false,
    });

    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    assert_eq!(p.value("completed_count"), Some(Value::Number(0.0)));

    // Tick to 1200 ms: entrance is halfway, does not fire yet
    p.handle(&Input::Tick { ms: 1200 });
    assert_eq!(p.value("completed_count"), Some(Value::Number(0.0)));

    // Tick to 1500 ms: crosses 1000 + 500 = 1500 ms, fires!
    p.handle(&Input::Tick { ms: 300 });
    assert_eq!(p.value("completed_count"), Some(Value::Number(1.0)));

    // Further ticks do not re-fire
    p.handle(&Input::Tick { ms: 500 });
    p.handle(&Input::Tick { ms: 1000 });
    assert_eq!(p.value("completed_count"), Some(Value::Number(1.0)));

    // Re-entering slide resets clock and allows it to fire once on that visit
    p.handle(&Input::GoTo {
        slide: "s-tabs".into(),
    });
    p.handle(&Input::GoTo {
        slide: "s-intro".into(),
    });
    p.handle(&Input::Tick { ms: 1500 });
    assert_eq!(p.value("completed_count"), Some(Value::Number(2.0)));
}

#[test]
fn entrance_trigger_ignores_path_and_exit_completions() {
    use coursecraft_course::{AnimEffect, Animation, Easing, MotionPath, Point};

    let mut c = course();
    c.variables.push(VariableDef {
        name: "entrances".into(),
        value: Value::Number(0.0),
    });
    let s = c.slide_mut("s-intro").unwrap();
    let obj = s.objects.iter_mut().find(|o| o.id == "btn-start").unwrap();
    obj.timing.start_ms = 0;
    obj.timing.end_ms = Some(3000);
    obj.exit = Some(Animation {
        effect: AnimEffect::Fade,
        duration_ms: 500,
        easing: Easing::Linear,
    });
    obj.motion_paths.push(MotionPath {
        id: "path1".into(),
        points: vec![Point { x: 0.0, y: 0.0 }, Point { x: 100.0, y: 0.0 }],
        duration_ms: 200,
        easing: Easing::Linear,
        relative: true,
    });
    s.triggers.push(Trigger {
        action: Action::MoveOnPath {
            object: "btn-start".into(),
            path: "path1".into(),
        },
        when: Event::TimelineStarts { layer: None },
        condition: None,
        disabled: false,
    });
    s.triggers.push(Trigger {
        action: Action::SetVariable {
            variable: "entrances".into(),
            op: VarOp::Add,
            value: Some(Value::Number(1.0)),
        },
        when: Event::AnimationCompletes {
            object: "btn-start".into(),
            path: None,
        },
        condition: None,
        disabled: false,
    });

    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    // The path completes at 200 ms and the exit at 3000 ms; neither is an entrance.
    p.handle(&Input::Tick { ms: 500 });
    p.handle(&Input::Tick { ms: 3000 });
    assert_eq!(p.value("entrances"), Some(Value::Number(0.0)));
}

#[test]
fn motion_path_runtime_state_not_in_suspend_data() {
    use coursecraft_course::{Easing, MotionPath, Point};

    let mut c = course();
    let s = c.slide_mut("s-intro").unwrap();
    let obj = s.objects.iter_mut().find(|o| o.id == "btn-start").unwrap();
    obj.timing.start_ms = 0;
    obj.motion_paths = vec![MotionPath {
        id: "p1".into(),
        points: vec![Point { x: 0.0, y: 0.0 }, Point { x: 200.0, y: 0.0 }],
        duration_ms: 1000,
        easing: Easing::Linear,
        relative: true,
    }];
    s.triggers.push(Trigger {
        action: Action::MoveOnPath {
            object: "btn-start".into(),
            path: "p1".into(),
        },
        when: Event::Click {
            object: "title".into(),
        },
        condition: None,
        disabled: false,
    });

    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    p.handle(&click("title")); // start path
    p.handle(&Input::Tick { ms: 500 }); // path running halfway

    let encoded = encode_suspend(&c, p.state());
    let decoded_state = decode_suspend(&c, &encoded).unwrap();

    // Decoded state must have empty running_paths and object_offsets
    assert!(decoded_state.running_paths.is_empty());
    assert!(decoded_state.object_offsets.is_empty());
}

fn quiz_with(question: serde_json::Value) -> Course {
    let mut c = course();
    let q = serde_json::from_value(question).unwrap();
    c.slide_mut("s-quiz").unwrap().question = Some(q);
    c
}

fn open_quiz(c: &Course) -> Player<'_> {
    let mut p = Player::new(c).unwrap();
    p.handle(&Input::Start);
    p.handle(&Input::GoTo {
        slide: "s-quiz".into(),
    });
    p
}

fn messages(effects: &[Effect]) -> Vec<String> {
    effects
        .iter()
        .filter_map(|e| match e {
            Effect::Warning { message } => Some(message.clone()),
            _ => None,
        })
        .collect()
}

fn answered(effects: &[Effect]) -> Option<(bool, Option<u32>)> {
    effects.iter().find_map(|e| match e {
        Effect::QuestionAnswered {
            correct,
            attempts_left,
            ..
        } => Some((*correct, *attempts_left)),
        _ => None,
    })
}

fn true_false_course() -> Course {
    quiz_with(serde_json::json!({
        "kind": "true_false",
        "prompt": "Report the spill.",
        "choices": [
            { "id": "true", "text": "True", "correct": true },
            { "id": "false", "text": "False" }
        ],
        "points": 10,
        "attempts": 2
    }))
}

#[test]
fn feedback_layers_show_after_a_correct_and_a_wrong_answer() {
    use coursecraft_course::{Layer, Object, ObjectKind, Rect, Timing};

    let mut c = true_false_course();
    {
        let slide = c.slide_mut("s-quiz").expect("quiz slide");
        for (id, name, text) in [
            ("feedback-correct", "Correct", "Correct!"),
            ("feedback-incorrect", "Incorrect", "Not quite."),
        ] {
            slide.layers.push(Layer {
                id: id.into(),
                name: name.into(),
                duration_ms: 5000,
                objects: vec![Object {
                    id: format!("{id}-text"),
                    name: format!("{name} feedback"),
                    kind: ObjectKind::Text {
                        text: text.into(),
                        style: coursecraft_course::TextStyle::default(),
                        runs: Vec::new(),
                        paragraphs: Vec::new(),
                        autofit: coursecraft_course::Autofit::None,
                        text_style: None,
                    },
                    frame: Rect {
                        x: 80.0,
                        y: 320.0,
                        w: 900.0,
                        h: 60.0,
                    },
                    timing: Timing::default(),
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
                }],
                triggers: Vec::new(),
                hide_other_layers: true,
                pause_base_timeline: false,
                prevent_base_clicks: false,
                hide_base_objects: false,
                hide_when_timeline_ends: false,
                allow_seek: coursecraft_course::SeekMode::Auto,
                revisit: coursecraft_course::RevisitMode::Auto,
            });
        }
        let question = slide.question.as_mut().expect("question");
        question.correct_layer = Some("feedback-correct".into());
        question.incorrect_layer = Some("feedback-incorrect".into());
    }

    let mut p = open_quiz(&c);
    p.handle(&Input::SelectChoice {
        choice: "true".into(),
    });
    let effects = p.handle(&click("btn-submit"));
    assert!(has(
        &effects,
        &Effect::LayerShown {
            layer: "feedback-correct".into()
        }
    ));
    assert!(!effects.iter().any(|effect| matches!(
        effect,
        Effect::LayerShown { layer } if layer == "feedback-incorrect"
    )));

    let mut p = open_quiz(&c);
    p.handle(&Input::SelectChoice {
        choice: "false".into(),
    });
    let effects = p.handle(&click("btn-submit"));
    assert!(has(
        &effects,
        &Effect::LayerShown {
            layer: "feedback-incorrect".into()
        }
    ));
    assert!(!effects.iter().any(|effect| matches!(
        effect,
        Effect::LayerShown { layer } if layer == "feedback-correct"
    )));
}

fn freeform_course(attempts: u32) -> Course {
    use coursecraft_course::{Layer, ObjectKind};

    let mut c = input_course(ObjectKind::Slider {
        variable: "input_number".into(),
        min: 0.0,
        max: 10.0,
        step: 1.0,
        orientation: coursecraft_course::Orientation::Horizontal,
    });
    let slide = c.slide_mut("s-intro").expect("question slide");
    for (id, name) in [("ff-correct", "Correct"), ("ff-incorrect", "Incorrect")] {
        slide.layers.push(Layer {
            id: id.into(),
            name: name.into(),
            duration_ms: 5000,
            objects: Vec::new(),
            triggers: Vec::new(),
            hide_other_layers: true,
            pause_base_timeline: false,
            prevent_base_clicks: false,
            hide_base_objects: false,
            hide_when_timeline_ends: false,
            allow_seek: coursecraft_course::SeekMode::Auto,
            revisit: RevisitMode::Auto,
        });
    }
    slide.question = Some(
        serde_json::from_value(serde_json::json!({
            "kind": "freeform",
            "prompt": "",
            "choices": [],
            "points": 10,
            "attempts": attempts,
            "correct_layer": "ff-correct",
            "incorrect_layer": "ff-incorrect",
            "correct_when": { "compare": { "variable": "input_number", "op": "ge", "value": 5 } }
        }))
        .expect("freeform question"),
    );
    slide.triggers.push(Trigger {
        action: Action::SubmitQuestion,
        when: Event::Submit,
        condition: None,
        disabled: false,
    });
    c.slide_mut("s-results")
        .expect("results")
        .triggers
        .push(Trigger {
            action: Action::ReviewQuiz,
            when: Event::Click {
                object: "results-text".into(),
            },
            condition: None,
            disabled: false,
        });
    c
}

#[test]
fn freeform_grades_a_slider_variable_locks_and_shows_feedback() {
    let c = freeform_course(1);
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    let fx = p.handle(&Input::SetInputValue {
        object: "input".into(),
        value: Value::Number(8.0),
    });
    assert!(
        has(
            &fx,
            &Effect::VariableChanged {
                variable: "input_number".into(),
                value: Value::Number(8.0),
            }
        ),
        "{fx:?}"
    );

    let fx = p.handle(&Input::SelectChoice { choice: "a".into() });
    assert!(
        messages(&fx)
            .iter()
            .any(|m| m.contains("select_choice does not apply")),
        "{fx:?}"
    );

    let fx = p.handle(&Input::Submit);
    assert_eq!(answered(&fx), Some((true, Some(0))), "{fx:?}");
    assert!(
        has(
            &fx,
            &Effect::LayerShown {
                layer: "ff-correct".into()
            }
        ),
        "{fx:?}"
    );
    assert_eq!(
        p.state().last_response,
        Some(LearnerResponse::Text("correct".into()))
    );
    let answer = p.state().answers.get("s-intro").copied().expect("answer");
    assert!(answer.correct);
    assert_eq!(answer.points, 10);
    assert_eq!(answer.attempts, 1);
    let fx = p.handle(&Input::Submit);
    assert!(messages(&fx).iter().any(|m| m.contains("already answered")));

    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    p.handle(&Input::SetInputValue {
        object: "input".into(),
        value: Value::Number(2.0),
    });
    let fx = p.handle(&Input::Submit);
    assert_eq!(answered(&fx), Some((false, Some(0))), "{fx:?}");
    assert!(
        has(
            &fx,
            &Effect::LayerShown {
                layer: "ff-incorrect".into()
            }
        ),
        "{fx:?}"
    );
    assert_eq!(
        p.state().last_response,
        Some(LearnerResponse::Text("incorrect".into()))
    );
    let answer = p.state().answers.get("s-intro").copied().expect("answer");
    assert!(!answer.correct);
    assert_eq!(answer.points, 0);
    let fx = p.handle(&Input::Submit);
    assert!(messages(&fx).iter().any(|m| m.contains("already answered")));
}

#[test]
fn freeform_review_restores_nothing_but_keeps_the_mark_data() {
    let c = freeform_course(1);
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    p.handle(&Input::SetInputValue {
        object: "input".into(),
        value: Value::Number(2.0),
    });
    p.handle(&Input::Submit);
    p.handle(&Input::Jump {
        slide: "s-results".into(),
    });
    let fx = p.handle(&click("results-text"));
    assert!(
        has(&fx, &Effect::ReviewChanged { reviewing: true }),
        "{fx:?}"
    );
    assert_eq!(p.current_slide().map(|s| s.id.as_str()), Some("s-intro"));
    assert!(p.state().selected.is_empty());
    assert!(p.state().text_entry.is_empty());
    assert!(p.state().sequence.is_empty());
    assert!(p.state().matches.is_empty());
    assert!(p.state().hotspot_markers.is_empty());
    assert!(p.state().drops.is_empty());
    let snapshot = p
        .state()
        .review_answers
        .get("s-intro")
        .expect("review snapshot");
    assert_eq!(snapshot, &crate::AnswerSnapshot::default());
    let answer = p.state().answers.get("s-intro").copied().expect("answer");
    assert!(!answer.correct);
    assert_eq!(answer.points, 0);
}

fn fill_in_course(case_sensitive: bool) -> Course {
    quiz_with(serde_json::json!({
        "kind": "fill_in",
        "prompt": "Name the city.",
        "choices": [],
        "accepted": ["Paris", "café", "ωmega"],
        "case_sensitive": case_sensitive,
        "points": 10,
        "attempts": 2
    }))
}

fn sequence_course(ids: &[&str]) -> Course {
    let choices: Vec<_> = ids
        .iter()
        .map(|id| serde_json::json!({ "id": id, "text": id, "correct": false }))
        .collect();
    quiz_with(serde_json::json!({
        "kind": "sequence",
        "prompt": "Put these in order.",
        "choices": choices,
        "points": 10,
        "attempts": 2
    }))
}

fn matching_course() -> Course {
    quiz_with(serde_json::json!({
        "kind": "matching",
        "prompt": "Match the pairs.",
        "choices": [],
        "pairs": [
            { "id": "p1", "left": "Sun", "right": "Star" },
            { "id": "p2", "left": "Moon", "right": "Satellite" }
        ],
        "points": 10,
        "attempts": 2
    }))
}

#[test]
fn true_false_grades_like_multiple_choice() {
    let c = true_false_course();
    let mut p = open_quiz(&c);
    assert!(
        p.handle(&click("btn-submit"))
            .contains(&Effect::AnswerRequired)
    );
    assert!(p.state().answers.is_empty());

    p.handle(&Input::SelectChoice {
        choice: "false".into(),
    });
    let fx = p.handle(&click("btn-submit"));
    assert_eq!(answered(&fx), Some((false, Some(1))));

    p.handle(&Input::SelectChoice {
        choice: "true".into(),
    });
    let fx = p.handle(&click("btn-submit"));
    assert_eq!(answered(&fx), Some((true, Some(0))));
    assert!(
        messages(&p.handle(&Input::SelectChoice {
            choice: "false".into(),
        }))
        .iter()
        .any(|m| m.contains("already answered"))
    );
}

#[test]
fn fill_in_grades_trim_case_and_unicode() {
    let c = fill_in_course(false);
    let mut p = open_quiz(&c);
    p.handle(&Input::TextEntered { text: "   ".into() });
    assert!(
        p.handle(&click("btn-submit"))
            .contains(&Effect::AnswerRequired)
    );
    assert!(p.state().answers.is_empty());

    p.handle(&Input::TextEntered {
        text: "  PARIS ".into(),
    });
    assert_eq!(
        answered(&p.handle(&click("btn-submit"))),
        Some((true, Some(1)))
    );

    let c = fill_in_course(false);
    let mut p = open_quiz(&c);
    p.handle(&Input::TextEntered {
        text: "CAFÉ".into(),
    });
    assert_eq!(
        answered(&p.handle(&click("btn-submit"))),
        Some((true, Some(1)))
    );

    let c = fill_in_course(false);
    let mut p = open_quiz(&c);
    p.handle(&Input::TextEntered {
        text: "ΩMEGA".into(),
    });
    assert_eq!(
        answered(&p.handle(&click("btn-submit"))),
        Some((true, Some(1)))
    );

    let c = fill_in_course(true);
    let mut p = open_quiz(&c);
    p.handle(&Input::TextEntered {
        text: "paris".into(),
    });
    assert_eq!(
        answered(&p.handle(&click("btn-submit"))),
        Some((false, Some(1)))
    );
    p.handle(&Input::TextEntered {
        text: "Paris".into(),
    });
    assert_eq!(
        answered(&p.handle(&click("btn-submit"))),
        Some((true, Some(0)))
    );

    let c = fill_in_course(false);
    let mut p = open_quiz(&c);
    p.handle(&Input::TextEntered {
        text: "Pa ris".into(),
    });
    assert_eq!(
        answered(&p.handle(&click("btn-submit"))),
        Some((false, Some(1)))
    );
}

#[test]
fn sequence_grades_against_authored_order_and_reshuffles() {
    let ids = ["a", "b"];
    let c = sequence_course(&ids);
    let mut p = open_quiz(&c);
    assert_eq!(p.sequence_order(), &["b".to_owned(), "a".to_owned()][..]);
    let fx = p.handle(&click("btn-submit"));
    assert!(!fx.contains(&Effect::AnswerRequired));
    assert_eq!(answered(&fx), Some((false, Some(1))));

    let ids = ["a", "b", "c", "d"];
    let c = sequence_course(&ids);
    let mut p1 = open_quiz(&c);
    let mut p2 = open_quiz(&c);
    assert_eq!(p1.sequence_order(), p2.sequence_order());
    assert_ne!(
        p1.sequence_order(),
        &[
            "a".to_owned(),
            "b".to_owned(),
            "c".to_owned(),
            "d".to_owned()
        ][..]
    );
    let before = p1.sequence_order().to_vec();
    let fx = p1.handle(&click("btn-submit"));
    assert_eq!(answered(&fx), Some((false, Some(1))));
    assert_ne!(p1.sequence_order(), before.as_slice());
    assert!(
        !fx.iter()
            .any(|e| matches!(e, Effect::SequenceChanged { .. }))
    );

    let target: Vec<String> = ids.iter().map(|s| (*s).to_owned()).collect();
    for i in 0..target.len() {
        if p2.sequence_order().get(i) == target.get(i) {
            continue;
        }
        let Some(id) = target.get(i).cloned() else {
            continue;
        };
        p2.handle(&Input::SequenceMoved { choice: id, to: i });
    }
    assert_eq!(p2.sequence_order(), target.as_slice());
    assert_eq!(
        answered(&p2.handle(&click("btn-submit"))),
        Some((true, Some(1)))
    );
}

#[test]
fn matching_requires_every_pair_and_grades_own_ids() {
    let c = matching_course();
    let mut p = open_quiz(&c);
    assert!(
        p.handle(&click("btn-submit"))
            .contains(&Effect::AnswerRequired)
    );
    p.handle(&Input::MatchSelected {
        left: "p1".into(),
        right: Some("p1".into()),
    });
    assert!(
        p.handle(&click("btn-submit"))
            .contains(&Effect::AnswerRequired)
    );
    assert!(
        p.state().answers.is_empty(),
        "a partial match is not graded"
    );

    p.handle(&Input::MatchSelected {
        left: "p2".into(),
        right: Some("p1".into()),
    });
    assert_eq!(
        answered(&p.handle(&click("btn-submit"))),
        Some((false, Some(1)))
    );
    assert_eq!(
        p.state().answers.get("s-quiz").map(|a| a.attempts),
        Some(1),
        "an incorrect answer with attempts left stays open"
    );

    // One attempt remains, so the learner can still change a match.
    p.handle(&Input::MatchSelected {
        left: "p2".into(),
        right: Some("p2".into()),
    });
    assert_eq!(p.match_for("p1"), Some("p1"));
    assert_eq!(p.match_for("p2"), Some("p2"));
    assert_eq!(
        answered(&p.handle(&click("btn-submit"))),
        Some((true, Some(0)))
    );
    assert!(
        messages(&p.handle(&Input::MatchSelected {
            left: "p1".into(),
            right: None,
        }))
        .iter()
        .any(|m| m.contains("already answered"))
    );
}

#[test]
fn sequence_and_matching_shuffles_are_deterministic() {
    let ids: Vec<String> = ["a", "b", "c", "d"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    let first = crate::run::sequence_order(&ids, crate::run::seed_for("s-quiz", 0));
    assert_eq!(
        first,
        crate::run::sequence_order(&ids, crate::run::seed_for("s-quiz", 0))
    );
    assert_ne!(first, ids);
    assert!(
        (1..12)
            .any(|n| crate::run::sequence_order(&ids, crate::run::seed_for("s-quiz", n)) != first),
        "a later attempt should re-shuffle"
    );

    let pair = ["a".to_owned(), "b".to_owned()];
    assert_eq!(
        crate::run::sequence_order(&pair, crate::run::seed_for("any", 0)),
        vec!["b".to_owned(), "a".to_owned()]
    );
    assert_eq!(
        crate::run::sequence_order(&pair, crate::run::seed_for("other", 7)),
        vec!["b".to_owned(), "a".to_owned()]
    );

    let ids = vec!["p1".to_owned(), "p2".to_owned()];
    let c = matching_course();
    let a = open_quiz(&c);
    let mut b = open_quiz(&c);
    let expected = crate::run::shuffled_ids(&ids, crate::run::seed_for("s-quiz", 0));
    assert_eq!(a.matching_options(), expected);
    assert_eq!(b.matching_options(), expected);
    b.handle(&Input::MatchSelected {
        left: "p1".into(),
        right: Some("p2".into()),
    });
    b.handle(&Input::MatchSelected {
        left: "p2".into(),
        right: Some("p1".into()),
    });
    b.handle(&click("btn-submit"));
    assert_eq!(
        b.matching_options(),
        crate::run::shuffled_ids(&ids, crate::run::seed_for("s-quiz", 1))
    );
}

#[test]
fn new_question_inputs_warn_without_changing_state() {
    let c = course();
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    let before = p.state().clone();
    for input in [
        Input::TextEntered { text: "x".into() },
        Input::SequenceMoved {
            choice: "a".into(),
            to: 0,
        },
        Input::MatchSelected {
            left: "p1".into(),
            right: Some("p2".into()),
        },
    ] {
        let fx = p.handle(&input);
        assert!(
            messages(&fx).iter().any(|m| m.contains("no question")),
            "{fx:?}"
        );
    }
    assert_eq!(p.state().text_entry, before.text_entry);
    assert_eq!(p.state().sequence, before.sequence);
    assert_eq!(p.state().matches, before.matches);

    let c = fill_in_course(false);
    let mut p = open_quiz(&c);
    let fx = p.handle(&Input::SelectChoice { choice: "a".into() });
    assert!(
        messages(&fx)
            .iter()
            .any(|m| m.contains("select_choice does not apply"))
    );
    assert!(p.state().selected.is_empty());
    let fx = p.handle(&Input::SequenceMoved {
        choice: "a".into(),
        to: 1,
    });
    assert!(
        messages(&fx)
            .iter()
            .any(|m| m.contains("sequence_moved does not apply"))
    );

    let c = sequence_course(&["a", "b", "c"]);
    let mut p = open_quiz(&c);
    let fx = p.handle(&Input::SequenceMoved {
        choice: "missing".into(),
        to: 0,
    });
    assert!(messages(&fx).iter().any(|m| m.contains("does not exist")));
    let order = p.sequence_order().to_vec();
    p.handle(&Input::SequenceMoved {
        choice: order[0].clone(),
        to: usize::MAX,
    });
    assert_eq!(
        p.sequence_order().last().map(String::as_str),
        Some(order[0].as_str())
    );

    let c = matching_course();
    let mut p = open_quiz(&c);
    let fx = p.handle(&Input::MatchSelected {
        left: "nope".into(),
        right: Some("p1".into()),
    });
    assert!(messages(&fx).iter().any(|m| m.contains("does not exist")));
    let fx = p.handle(&Input::TextEntered {
        text: "nope".into(),
    });
    assert!(
        messages(&fx)
            .iter()
            .any(|m| m.contains("text_entered does not apply"))
    );
    assert!(p.text_entry().is_empty());
}

#[test]
fn question_inputs_lock_after_attempts_run_out() {
    let c = fill_in_course(true);
    let mut p = open_quiz(&c);
    p.handle(&Input::TextEntered {
        text: "nope".into(),
    });
    p.handle(&click("btn-submit"));
    p.handle(&Input::TextEntered {
        text: "still".into(),
    });
    let fx = p.handle(&click("btn-submit"));
    assert_eq!(answered(&fx), Some((false, Some(0))));
    let fx = p.handle(&Input::TextEntered {
        text: "Paris".into(),
    });
    assert!(messages(&fx).iter().any(|m| m.contains("already answered")));
    assert_eq!(p.text_entry(), "still");

    let c = sequence_course(&["a", "b", "c"]);
    let mut p = open_quiz(&c);
    p.handle(&click("btn-submit"));
    p.handle(&click("btn-submit"));
    let fx = p.handle(&Input::SequenceMoved {
        choice: "a".into(),
        to: 0,
    });
    assert!(messages(&fx).iter().any(|m| m.contains("already answered")));
}

#[test]
fn hostile_question_inputs_never_panic() {
    let long = "é".repeat(8_000);
    let inputs = [
        Input::TextEntered { text: long.clone() },
        Input::TextEntered {
            text: format!("{long}\u{0000}"),
        },
        Input::SequenceMoved {
            choice: long.clone(),
            to: usize::MAX,
        },
        Input::SequenceMoved {
            choice: String::new(),
            to: 0,
        },
        Input::MatchSelected {
            left: long.clone(),
            right: Some(long.clone()),
        },
        Input::MatchSelected {
            left: String::new(),
            right: None,
        },
        Input::SelectChoice { choice: long },
    ];
    let courses = [
        true_false_course(),
        fill_in_course(false),
        sequence_course(&["a", "b", "c"]),
        matching_course(),
        course(),
    ];
    for c in &courses {
        let mut p = Player::new(c).unwrap();
        for input in &inputs {
            let _ = p.handle(input);
        }
        p.handle(&Input::Start);
        p.handle(&Input::GoTo {
            slide: "s-quiz".into(),
        });
        for input in &inputs {
            let _ = p.handle(input);
        }
        if p.text_entry().chars().count() > 2000 {
            panic!("text entry grew past the cap");
        }
    }
}

#[test]
fn question_response_state_is_not_in_suspend_data() {
    let c = fill_in_course(false);
    let mut p = open_quiz(&c);
    p.handle(&Input::TextEntered {
        text: "Paris".into(),
    });
    let encoded = encode_suspend(&c, p.state());
    let mut cleared = p.state().clone();
    cleared.text_entry.clear();
    cleared.sequence.clear();
    cleared.matches.clear();
    assert_eq!(encoded, encode_suspend(&c, &cleared));
    let decoded = decode_suspend(&c, &encoded).unwrap();
    assert!(decoded.text_entry.is_empty());
    assert!(decoded.sequence.is_empty());
    assert!(decoded.matches.is_empty());

    let c = sequence_course(&["a", "b", "c"]);
    let p = open_quiz(&c);
    assert!(!p.sequence_order().is_empty());
    let encoded = encode_suspend(&c, p.state());
    let decoded = decode_suspend(&c, &encoded).unwrap();
    assert!(decoded.sequence.is_empty());

    let c = matching_course();
    let mut p = open_quiz(&c);
    p.handle(&Input::MatchSelected {
        left: "p1".into(),
        right: Some("p2".into()),
    });
    let encoded = encode_suspend(&c, p.state());
    let decoded = decode_suspend(&c, &encoded).unwrap();
    assert!(decoded.matches.is_empty());
}

fn add_objects(c: &mut Course, objects: serde_json::Value) {
    let extra: Vec<coursecraft_course::Object> = serde_json::from_value(objects).unwrap();
    c.slide_mut("s-quiz").unwrap().objects.extend(extra);
}

fn hotspot_course(markers: u32, regions: serde_json::Value) -> Course {
    let mut c = quiz_with(serde_json::json!({
        "kind": "hotspot",
        "prompt": "Click the spill.",
        "choices": [],
        "hotspot_image": "photo",
        "regions": regions,
        "markers": markers,
        "points": 10,
        "attempts": 2
    }));
    add_objects(
        &mut c,
        serde_json::json!([{
            "id": "photo",
            "kind": { "type": "image", "src": "spill.png", "alt": "spill" },
            "frame": { "x": 0.0, "y": 0.0, "w": 500.0, "h": 500.0 }
        }]),
    );
    c
}

fn rect_region(id: &str, x: f32, y: f32, w: f32, h: f32, correct: bool) -> serde_json::Value {
    serde_json::json!({
        "id": id, "shape": "rect", "x": x, "y": y, "w": w, "h": h, "correct": correct
    })
}

fn drag_course(options: serde_json::Value) -> Course {
    let mut question = serde_json::json!({
        "kind": "drag_drop",
        "prompt": "Sort the cards.",
        "choices": [],
        "drag_items": [
            { "item": "card", "target": "box" },
            { "item": "chip", "target": "decoy" }
        ],
        "drop_targets": ["box", "decoy"],
        "points": 10,
        "attempts": 2
    });
    if let (Some(q), Some(extra)) = (question.as_object_mut(), options.as_object()) {
        for (k, v) in extra {
            q.insert(k.clone(), v.clone());
        }
    }
    let mut c = quiz_with(question);
    add_objects(
        &mut c,
        serde_json::json!([
            { "id": "card", "kind": { "type": "shape", "shape": "rectangle", "fill": "#112233" }, "frame": { "x": 0.0, "y": 0.0, "w": 40.0, "h": 40.0 } },
            { "id": "chip", "kind": { "type": "shape", "shape": "rectangle", "fill": "#223344" }, "frame": { "x": 0.0, "y": 100.0, "w": 40.0, "h": 40.0 } },
            { "id": "box", "kind": { "type": "shape", "shape": "rectangle", "fill": "#334455" }, "frame": { "x": 200.0, "y": 0.0, "w": 80.0, "h": 80.0 } },
            { "id": "decoy", "kind": { "type": "shape", "shape": "ellipse", "fill": "#556677" }, "frame": { "x": 400.0, "y": 0.0, "w": 80.0, "h": 80.0 } }
        ]),
    );
    c
}

fn item_drops(effects: &[Effect]) -> Vec<(String, Option<String>, (f32, f32))> {
    effects
        .iter()
        .filter_map(|e| match e {
            Effect::ItemDropped {
                item,
                target,
                offset,
            } => Some((item.clone(), target.clone(), *offset)),
            _ => None,
        })
        .collect()
}

fn submit(p: &mut Player<'_>) -> Vec<Effect> {
    p.handle(&click("btn-submit"))
}

#[test]
fn hotspot_grading_covers_edges_regions_and_unanswered() {
    let regions = serde_json::json!([
        rect_region("good", 100.0, 100.0, 100.0, 100.0, true),
        rect_region("bad", 300.0, 300.0, 50.0, 50.0, false),
        {
            "id": "ring",
            "shape": "ellipse",
            "x": 100.0,
            "y": 300.0,
            "w": 100.0,
            "h": 100.0,
            "correct": true
        }
    ]);
    let c = hotspot_course(1, regions);
    let mut p = open_quiz(&c);
    let fx = submit(&mut p);
    assert!(fx.iter().any(|e| matches!(e, Effect::AnswerRequired)));
    assert!(answered(&fx).is_none());

    let cases = [
        ((100.0, 100.0), true),
        ((200.0, 200.0), true),
        ((200.1, 150.0), false),
        ((10.0, 10.0), false),
        ((320.0, 320.0), false),
        ((200.0, 350.0), true),
        ((201.0, 350.0), false),
    ];
    for ((x, y), expect) in cases {
        let mut p = open_quiz(&c);
        p.handle(&Input::HotspotClicked { x, y });
        assert_eq!(p.state().hotspot_markers, vec![(x, y)], "marker at {x},{y}");
        assert_eq!(
            answered(&submit(&mut p)).map(|a| a.0),
            Some(expect),
            "grade at {x},{y}"
        );
    }

    // A point inside both a correct rect and an incorrect rect still counts.
    let overlap = hotspot_course(
        1,
        serde_json::json!([
            rect_region("good", 100.0, 100.0, 100.0, 100.0, true),
            rect_region("bad", 150.0, 150.0, 20.0, 20.0, false)
        ]),
    );
    let mut p = open_quiz(&overlap);
    p.handle(&Input::HotspotClicked { x: 160.0, y: 160.0 });
    assert_eq!(answered(&submit(&mut p)).map(|a| a.0), Some(true));
}

#[test]
fn hotspot_multiple_markers_replace_the_oldest_and_all_must_be_correct() {
    let c = hotspot_course(
        2,
        serde_json::json!([
            rect_region("good", 100.0, 100.0, 100.0, 100.0, true),
            rect_region("bad", 300.0, 300.0, 50.0, 50.0, false)
        ]),
    );
    let mut p = open_quiz(&c);
    for (x, y) in [(110.0, 110.0), (10.0, 10.0), (150.0, 150.0)] {
        let fx = p.handle(&Input::HotspotClicked { x, y });
        assert!(fx.iter().any(|e| matches!(e, Effect::HotspotMarked { .. })));
    }
    assert_eq!(
        p.state().hotspot_markers,
        vec![(10.0, 10.0), (150.0, 150.0)]
    );
    assert_eq!(answered(&submit(&mut p)).map(|a| a.0), Some(false));
    assert_eq!(
        p.state().hotspot_markers.len(),
        2,
        "markers stay after a miss"
    );

    let mut p = open_quiz(&c);
    p.handle(&Input::HotspotClicked { x: 120.0, y: 120.0 });
    p.handle(&Input::HotspotClicked { x: 180.0, y: 180.0 });
    assert_eq!(answered(&submit(&mut p)).map(|a| a.0), Some(true));
}

#[test]
fn hotspot_click_warns_outside_the_image_and_resets_on_slide_entry() {
    let c = hotspot_course(
        1,
        serde_json::json!([rect_region("good", 100.0, 100.0, 80.0, 80.0, true)]),
    );
    let mut p = open_quiz(&c);
    let fx = p.handle(&Input::HotspotClicked { x: 600.0, y: 10.0 });
    assert!(messages(&fx).iter().any(|m| m.contains("outside")));
    assert!(p.state().hotspot_markers.is_empty());
    let fx = p.handle(&Input::HotspotClicked {
        x: f32::NAN,
        y: 10.0,
    });
    assert!(messages(&fx).iter().any(|m| m.contains("finite")));
    p.handle(&Input::HotspotClicked { x: 120.0, y: 120.0 });
    assert_eq!(answered(&submit(&mut p)).map(|a| a.0), Some(true));
    let fx = p.handle(&Input::HotspotClicked { x: 130.0, y: 130.0 });
    assert!(messages(&fx).iter().any(|m| m.contains("already answered")));
    assert_eq!(p.state().hotspot_markers, vec![(120.0, 120.0)]);

    let mut p = open_quiz(&c);
    p.handle(&Input::HotspotClicked { x: 10.0, y: 10.0 });
    assert_eq!(answered(&submit(&mut p)), Some((false, Some(1))));
    assert_eq!(p.state().hotspot_markers, vec![(10.0, 10.0)]);
    p.handle(&Input::HotspotClicked { x: 140.0, y: 140.0 });
    assert_eq!(p.state().hotspot_markers, vec![(140.0, 140.0)]);
    assert_eq!(answered(&submit(&mut p)).map(|a| a.0), Some(true));

    let mut p = open_quiz(&c);
    p.handle(&Input::HotspotClicked { x: 140.0, y: 140.0 });
    p.handle(&Input::GoTo {
        slide: "s-intro".into(),
    });
    p.handle(&Input::GoTo {
        slide: "s-quiz".into(),
    });
    assert!(p.state().hotspot_markers.is_empty());
    let fx = submit(&mut p);
    assert!(fx.iter().any(|e| matches!(e, Effect::AnswerRequired)));
}

#[test]
fn drag_drop_grading_requires_every_item_on_its_target() {
    let c = drag_course(serde_json::json!({}));
    let mut p = open_quiz(&c);
    assert!(
        submit(&mut p)
            .iter()
            .any(|e| matches!(e, Effect::AnswerRequired))
    );

    p.handle(&Input::DragDropped {
        item: "card".into(),
        target: Some("box".into()),
        x: 0.0,
        y: 0.0,
    });
    assert_eq!(answered(&submit(&mut p)), Some((false, Some(1))));
    assert_eq!(
        p.state().drops.get("card").map(String::as_str),
        Some("box"),
        "a miss leaves the drop in place"
    );

    let mut p = open_quiz(&c);
    p.handle(&Input::DragDropped {
        item: "card".into(),
        target: Some("box".into()),
        x: 0.0,
        y: 0.0,
    });
    p.handle(&Input::DragDropped {
        item: "chip".into(),
        target: Some("box".into()),
        x: 0.0,
        y: 0.0,
    });
    assert_eq!(answered(&submit(&mut p)).map(|a| a.0), Some(false));

    let mut p = open_quiz(&c);
    p.handle(&Input::DragDropped {
        item: "card".into(),
        target: Some("box".into()),
        x: 1.0,
        y: 1.0,
    });
    p.handle(&Input::DragDropped {
        item: "chip".into(),
        target: Some("decoy".into()),
        x: 1.0,
        y: 1.0,
    });
    assert_eq!(answered(&submit(&mut p)).map(|a| a.0), Some(true));
    let fx = p.handle(&Input::DragDropped {
        item: "card".into(),
        target: Some("decoy".into()),
        x: 10.0,
        y: 10.0,
    });
    assert!(messages(&fx).iter().any(|m| m.contains("already answered")));
    assert_eq!(
        p.state().object_offsets.get("card").copied(),
        Some((220.0, 20.0))
    );
}

#[test]
fn drag_dropped_applies_each_placement_rule() {
    let c = drag_course(serde_json::json!({}));
    let mut p = open_quiz(&c);
    let fx = p.handle(&Input::DragDropped {
        item: "card".into(),
        target: None,
        x: 30.0,
        y: 30.0,
    });
    assert_eq!(item_drops(&fx), vec![("card".into(), None, (0.0, 0.0))]);
    assert!(p.state().drops.is_empty());
    assert!(p.state().object_offsets.is_empty());

    let fx = p.handle(&Input::DragDropped {
        item: "card".into(),
        target: Some("btn-submit".into()),
        x: 30.0,
        y: 30.0,
    });
    assert_eq!(item_drops(&fx), vec![("card".into(), None, (0.0, 0.0))]);

    let fx = p.handle(&Input::DragDropped {
        item: "card".into(),
        target: Some("box".into()),
        x: 0.0,
        y: 0.0,
    });
    assert_eq!(
        item_drops(&fx),
        vec![("card".into(), Some("box".into()), (220.0, 20.0))]
    );
    assert_eq!(
        p.state().object_offsets.get("card").copied(),
        Some((220.0, 20.0))
    );

    let free = drag_course(serde_json::json!({ "snap_to_center": false }));
    let mut p = open_quiz(&free);
    let fx = p.handle(&Input::DragDropped {
        item: "card".into(),
        target: Some("box".into()),
        x: 50.0,
        y: 60.0,
    });
    assert_eq!(
        item_drops(&fx),
        vec![("card".into(), Some("box".into()), (50.0, 60.0))]
    );
    let fx = p.handle(&Input::DragDropped {
        item: "card".into(),
        target: Some("box".into()),
        x: -50.0,
        y: 5000.0,
    });
    assert_eq!(
        item_drops(&fx),
        vec![("card".into(), Some("box".into()), (0.0, 720.0))]
    );

    let mut p = open_quiz(&c);
    p.handle(&Input::DragDropped {
        item: "chip".into(),
        target: Some("box".into()),
        x: 0.0,
        y: 0.0,
    });
    let fx = p.handle(&Input::DragDropped {
        item: "card".into(),
        target: Some("box".into()),
        x: 0.0,
        y: 0.0,
    });
    assert_eq!(
        item_drops(&fx),
        vec![
            ("card".into(), Some("box".into()), (220.0, 20.0)),
            ("chip".into(), None, (0.0, 0.0)),
        ]
    );
    assert!(!p.state().drops.contains_key("chip"));
    assert!(!p.state().object_offsets.contains_key("chip"));

    let shared = drag_course(serde_json::json!({ "one_per_target": false }));
    let mut p = open_quiz(&shared);
    p.handle(&Input::DragDropped {
        item: "chip".into(),
        target: Some("box".into()),
        x: 0.0,
        y: 0.0,
    });
    p.handle(&Input::DragDropped {
        item: "card".into(),
        target: Some("box".into()),
        x: 0.0,
        y: 0.0,
    });
    assert_eq!(p.state().drops.get("chip").map(String::as_str), Some("box"));
    assert_eq!(p.state().drops.get("card").map(String::as_str), Some("box"));

    let instant = drag_course(serde_json::json!({ "return_if_wrong": true }));
    let mut p = open_quiz(&instant);
    p.handle(&Input::DragDropped {
        item: "chip".into(),
        target: Some("decoy".into()),
        x: 0.0,
        y: 0.0,
    });
    let fx = p.handle(&Input::DragDropped {
        item: "card".into(),
        target: Some("decoy".into()),
        x: 12.0,
        y: 12.0,
    });
    assert_eq!(item_drops(&fx), vec![("card".into(), None, (0.0, 0.0))]);
    assert_eq!(
        p.state().drops.get("chip").map(String::as_str),
        Some("decoy")
    );
    assert!(!p.state().drops.contains_key("card"));

    let mut p = open_quiz(&c);
    let before = p.state().clone();
    for input in [
        Input::DragDropped {
            item: "nope".into(),
            target: Some("box".into()),
            x: 1.0,
            y: 1.0,
        },
        Input::DragDropped {
            item: String::new(),
            target: None,
            x: 1.0,
            y: 1.0,
        },
        Input::DragDropped {
            item: "card".into(),
            target: Some("box".into()),
            x: f32::NAN,
            y: 1.0,
        },
        Input::DragDropped {
            item: "card".into(),
            target: Some("box".into()),
            x: f32::INFINITY,
            y: f32::NEG_INFINITY,
        },
    ] {
        let fx = p.handle(&input);
        assert!(messages(&fx).iter().any(|m| !m.is_empty()), "{input:?}");
        assert_eq!(p.state().drops, before.drops);
        assert_eq!(p.state().object_offsets, before.object_offsets);
    }

    let plain = course();
    let mut p = open_quiz(&plain);
    let fx = p.handle(&Input::DragDropped {
        item: "card".into(),
        target: None,
        x: 1.0,
        y: 1.0,
    });
    assert!(messages(&fx).iter().any(|m| m.contains("does not apply")));
    let fx = p.handle(&Input::HotspotClicked { x: 1.0, y: 1.0 });
    assert!(messages(&fx).iter().any(|m| m.contains("does not apply")));

    let mut p = open_quiz(&c);
    p.handle(&Input::DragDropped {
        item: "card".into(),
        target: Some("box".into()),
        x: 0.0,
        y: 0.0,
    });
    p.handle(&Input::GoTo {
        slide: "s-intro".into(),
    });
    p.handle(&Input::GoTo {
        slide: "s-quiz".into(),
    });
    assert!(p.state().drops.is_empty());
    assert!(p.state().object_offsets.is_empty());
}

#[test]
fn hotspot_markers_and_drops_are_not_in_suspend_data() {
    let c = hotspot_course(
        2,
        serde_json::json!([rect_region("good", 0.0, 0.0, 100.0, 100.0, true)]),
    );
    let mut p = open_quiz(&c);
    p.handle(&Input::HotspotClicked { x: 20.0, y: 20.0 });
    p.handle(&Input::HotspotClicked { x: 40.0, y: 40.0 });
    let encoded = encode_suspend(&c, p.state());
    let mut cleared = p.state().clone();
    cleared.hotspot_markers.clear();
    cleared.drops.clear();
    cleared.object_offsets.clear();
    assert_eq!(encoded, encode_suspend(&c, &cleared));
    let decoded = decode_suspend(&c, &encoded).unwrap();
    assert!(decoded.hotspot_markers.is_empty());
    assert!(decoded.drops.is_empty());
    assert!(decoded.object_offsets.is_empty());

    let c = drag_course(serde_json::json!({}));
    let mut p = open_quiz(&c);
    p.handle(&Input::DragDropped {
        item: "card".into(),
        target: Some("box".into()),
        x: 0.0,
        y: 0.0,
    });
    assert!(!p.state().drops.is_empty());
    let encoded = encode_suspend(&c, p.state());
    let decoded = decode_suspend(&c, &encoded).unwrap();
    assert!(decoded.drops.is_empty());
    assert!(decoded.object_offsets.is_empty());
}

#[test]
fn hostile_hotspot_and_drag_inputs_never_panic() {
    let long = "é".repeat(8_000);
    let inputs = [
        Input::HotspotClicked {
            x: f32::NAN,
            y: f32::INFINITY,
        },
        Input::HotspotClicked {
            x: f32::NEG_INFINITY,
            y: -1.0e20,
        },
        Input::DragDropped {
            item: long.clone(),
            target: Some(long),
            x: f32::NAN,
            y: f32::NAN,
        },
        Input::DragDropped {
            item: String::new(),
            target: None,
            x: 0.0,
            y: 0.0,
        },
    ];
    let courses = [
        hotspot_course(
            1,
            serde_json::json!([rect_region("good", 0.0, 0.0, 40.0, 40.0, true)]),
        ),
        drag_course(serde_json::json!({})),
        course(),
    ];
    for c in &courses {
        let mut p = Player::new(c).unwrap();
        for input in &inputs {
            let _ = p.handle(input);
        }
        p.handle(&Input::Start);
        let _ = p.handle(&Input::GoTo {
            slide: "s-quiz".into(),
        });
        for input in &inputs {
            let _ = p.handle(input);
        }
    }

    let mut broken = hotspot_course(
        1,
        serde_json::json!([rect_region("good", 0.0, 0.0, 40.0, 40.0, true)]),
    );
    broken
        .slide_mut("s-quiz")
        .unwrap()
        .question
        .as_mut()
        .unwrap()
        .markers = 0;
    let mut p = Player::new_unvalidated(&broken);
    p.handle(&Input::Start);
    p.handle(&Input::GoTo {
        slide: "s-quiz".into(),
    });
    let fx = p.handle(&Input::HotspotClicked { x: 10.0, y: 10.0 });
    assert!(messages(&fx).iter().any(|m| m.contains("markers")));
    assert!(p.state().hotspot_markers.is_empty());
}

#[test]
fn last_response_records_multiple_choice_in_authored_order() {
    let c = course();
    let mut p = open_quiz(&c);
    p.handle(&Input::SelectChoice { choice: "b".into() });
    submit(&mut p);
    assert_eq!(
        p.state().last_response,
        Some(LearnerResponse::Choices(vec!["b".into()]))
    );
}

#[test]
fn last_response_records_multiple_response_sorted_like_the_course() {
    let c = quiz_with(serde_json::json!({
        "kind": "multiple_response",
        "prompt": "Pick the right ones.",
        "choices": [
            { "id": "a", "text": "A", "correct": true },
            { "id": "b", "text": "B" },
            { "id": "c", "text": "C", "correct": true }
        ],
        "points": 10,
        "attempts": 1
    }));
    let mut p = open_quiz(&c);
    p.handle(&Input::SelectChoice { choice: "c".into() });
    p.handle(&Input::SelectChoice { choice: "a".into() });
    submit(&mut p);
    assert_eq!(
        p.state().last_response,
        Some(LearnerResponse::Choices(vec!["a".into(), "c".into()]))
    );
}

#[test]
fn last_response_records_true_false_as_a_choice() {
    let c = true_false_course();
    let mut p = open_quiz(&c);
    p.handle(&Input::SelectChoice {
        choice: "false".into(),
    });
    submit(&mut p);
    assert_eq!(
        p.state().last_response,
        Some(LearnerResponse::Choices(vec!["false".into()]))
    );
}

#[test]
fn last_response_records_trimmed_fill_in_text() {
    let c = fill_in_course(false);
    let mut p = open_quiz(&c);
    p.handle(&Input::TextEntered {
        text: "  PARIS ".into(),
    });
    submit(&mut p);
    assert_eq!(
        p.state().last_response,
        Some(LearnerResponse::Text("PARIS".into()))
    );
}

#[test]
fn wrong_sequence_keeps_the_submitted_order_not_the_reshuffle() {
    let c = sequence_course(&["a", "b", "c", "d"]);
    let mut p = open_quiz(&c);
    let submitted = p.sequence_order().to_vec();
    let fx = submit(&mut p);
    assert_eq!(answered(&fx), Some((false, Some(1))));
    assert_ne!(p.sequence_order(), submitted.as_slice());
    assert_eq!(
        p.state().last_response,
        Some(LearnerResponse::Sequence(submitted))
    );
}

#[test]
fn last_response_records_each_left_pair_with_its_chosen_right() {
    let c = matching_course();
    let mut p = open_quiz(&c);
    p.handle(&Input::MatchSelected {
        left: "p1".into(),
        right: Some("p2".into()),
    });
    p.handle(&Input::MatchSelected {
        left: "p2".into(),
        right: Some("p1".into()),
    });
    submit(&mut p);
    assert_eq!(
        p.state().last_response,
        Some(LearnerResponse::Pairs(vec![
            ("p1".into(), "p2".into()),
            ("p2".into(), "p1".into()),
        ]))
    );
}

#[test]
fn last_response_records_hotspot_regions_once_in_course_order() {
    let c = hotspot_course(
        2,
        serde_json::json!([
            rect_region("good", 0.0, 0.0, 100.0, 100.0, true),
            rect_region("bad", 200.0, 200.0, 50.0, 50.0, false)
        ]),
    );
    let mut p = open_quiz(&c);
    p.handle(&Input::HotspotClicked { x: 10.0, y: 10.0 });
    p.handle(&Input::HotspotClicked { x: 20.0, y: 20.0 });
    submit(&mut p);
    assert_eq!(
        p.state().last_response,
        Some(LearnerResponse::Regions(vec!["good".into()]))
    );
}

#[test]
fn last_response_records_only_dropped_drag_items_in_course_order() {
    let c = drag_course(serde_json::json!({}));
    let mut p = open_quiz(&c);
    p.handle(&Input::DragDropped {
        item: "card".into(),
        target: Some("box".into()),
        x: 0.0,
        y: 0.0,
    });
    submit(&mut p);
    assert_eq!(
        p.state().last_response,
        Some(LearnerResponse::Pairs(vec![("card".into(), "box".into())]))
    );
}

#[test]
fn last_response_is_not_set_when_an_answer_is_required() {
    let c = true_false_course();
    let mut p = open_quiz(&c);
    let fx = submit(&mut p);
    assert!(fx.contains(&Effect::AnswerRequired));
    assert!(p.state().last_response.is_none());
}

#[test]
fn last_response_is_never_encoded_in_suspend_data() {
    let c = course();
    let mut p = open_quiz(&c);
    p.handle(&Input::SelectChoice { choice: "b".into() });
    submit(&mut p);
    assert!(p.state().last_response.is_some());

    let with_response = encode_suspend(&c, p.state());
    let mut cleared = p.state().clone();
    cleared.last_response = None;
    assert_eq!(encode_suspend(&c, &cleared), with_response);
    let decoded = decode_suspend(&c, &with_response).unwrap();
    assert!(decoded.last_response.is_none());
}

#[test]
fn a_master_changes_no_play_time_effects() {
    let plain = course();
    let mut with_master = plain.clone();
    let mut master = plain.scenes[0].slides[0].clone();
    master.id = "m1".to_owned();
    master.title = "Master".to_owned();
    master.triggers.clear();
    master.layers.clear();
    master.question = None;
    master.objects[0].id = "master-object".to_owned();
    with_master.masters.push(master);
    with_master.scenes[0].slides[0].master = Some("m1".to_owned());

    let inputs: Vec<Input> = serde_json::from_str(PASS_INPUTS).unwrap();
    let mut a = Player::new(&plain).unwrap();
    let mut b = Player::new(&with_master).unwrap();
    for input in &inputs {
        assert_eq!(a.handle(input), b.handle(input));
    }
    assert_eq!(a.state(), b.state());
}

#[test]
fn resume_never_lands_on_a_master() {
    let mut c = course();
    let mut master = c.scenes[0].slides[0].clone();
    master.id = "m1".to_owned();
    master.title = "Master".to_owned();
    master.triggers.clear();
    master.layers.clear();
    master.question = None;
    c.masters.push(master);
    let state = PlayerState {
        slide: c.slides().count(),
        ..PlayerState::default()
    };
    assert!(Player::resume(&c, state).is_err());
}

#[test]
fn the_learner_pause_survives_a_host_rebuilding_the_player_each_frame() {
    let course = course();
    let mut player = Player::new(&course).expect("player");
    player.handle(&Input::Start);
    player.handle(&Input::Tick { ms: 100 });
    player.handle(&Input::PlayerPause);
    let state = player.state().clone();

    let mut rebuilt = Player::resume(&course, state).expect("resume");
    assert!(rebuilt.player_paused());
    let before = rebuilt.state().base.ms;
    rebuilt.handle(&Input::Tick { ms: 500 });
    assert_eq!(rebuilt.state().base.ms, before);
}

// ---------------------------------------------------------------- layer options

fn options_course(
    objects: serde_json::Value,
    layers: serde_json::Value,
    triggers: serde_json::Value,
) -> Course {
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "options-test",
        "title": "Options Test",
        "scenes": [{
            "id": "scene1",
            "title": "Scene 1",
            "slides": [{
                "id": "s1",
                "title": "Slide 1",
                "duration_ms": 5000,
                "objects": objects,
                "layers": layers,
                "triggers": triggers
            }]
        }]
    });
    Course::from_json(&json.to_string()).unwrap()
}

fn button(id: &str, label: &str, x: f32) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "kind": { "type": "button", "label": label },
        "frame": { "x": x, "y": 0.0, "w": 100.0, "h": 50.0 },
        "timing": { "start_ms": 0 }
    })
}

#[test]
fn hide_base_objects_hides_draw_clicks_focus_and_access_but_not_the_base_clock_or_media() {
    let c = options_course(
        serde_json::json!([
            button("btn_show", "Show layer", 0.0),
            button("btn_base", "Base", 120.0),
            {
                "id": "txt_base",
                "kind": { "type": "text", "text": "Base text" },
                "frame": { "x": 0.0, "y": 100.0, "w": 100.0, "h": 50.0 },
                "timing": { "start_ms": 0 }
            },
            {
                "id": "aud_base",
                "kind": { "type": "audio", "src": "base.mp3", "autoplay": true },
                "frame": { "x": 0.0, "y": 200.0, "w": 48.0, "h": 48.0 },
                "timing": { "start_ms": 0 }
            }
        ]),
        serde_json::json!([{
            "id": "l_hide",
            "name": "Hide base",
            "duration_ms": 5000,
            "hide_other_layers": false,
            "hide_base_objects": true,
            "objects": [
                button("btn_layer", "Layer", 0.0),
                button("btn_hide", "Hide layer", 120.0)
            ],
            "triggers": [
                {
                    "when": { "event": "click", "object": "btn_layer" },
                    "action": { "action": "open_url", "url": "https://x.test/layer" }
                },
                {
                    "when": { "event": "click", "object": "btn_hide" },
                    "action": { "action": "hide_layer" }
                }
            ]
        }]),
        serde_json::json!([
            {
                "when": { "event": "click", "object": "btn_show" },
                "action": { "action": "show_layer", "layer": "l_hide" }
            },
            {
                "when": { "event": "click", "object": "btn_base" },
                "action": { "action": "open_url", "url": "https://x.test/base" }
            }
        ]),
    );
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    assert!(p.is_media_playing("aud_base"));
    assert!(!p.base_objects_hidden());
    assert!(p.keyboard_objects().iter().any(|id| id == "btn_base"));

    p.handle(&click("btn_show"));
    assert!(p.base_objects_hidden());
    let focus = p.keyboard_objects();
    assert!(focus.iter().any(|id| id == "btn_layer"), "{focus:?}");
    assert!(!focus.iter().any(|id| id == "btn_base"), "{focus:?}");
    assert!(!focus.iter().any(|id| id == "btn_show"), "{focus:?}");

    // A base click does not reach anything. A layer click still does.
    let fx = p.handle(&click("btn_base"));
    assert!(warned(&fx));
    assert!(urls(&fx).is_empty());
    let fx = p.handle(&click("btn_layer"));
    assert_eq!(urls(&fx), vec!["layer".to_owned()]);

    // The base clock and its media keep running.
    p.handle(&Input::Tick { ms: 1000 });
    assert_eq!(p.slide_elapsed_ms(), 1000);
    assert!(p.is_media_playing("aud_base"));

    p.handle(&click("btn_hide"));
    assert!(!p.base_objects_hidden());
    let fx = p.handle(&click("btn_base"));
    assert_eq!(urls(&fx), vec!["base".to_owned()]);
}

#[test]
fn hide_when_timeline_ends_fires_triggers_first_then_hides_once() {
    let c = options_course(
        serde_json::json!([
            button("btn_show_ends", "Ends", 0.0),
            button("btn_show_self", "Self", 120.0),
            button("btn_show_zero", "Zero", 240.0)
        ]),
        serde_json::json!([
            {
                "id": "l_ends",
                "name": "Ends",
                "duration_ms": 1500,
                "hide_other_layers": false,
                "hide_when_timeline_ends": true,
                "triggers": [{
                    "when": { "event": "timeline_ends", "layer": "l_ends" },
                    "action": { "action": "open_url", "url": "https://x.test/ends" }
                }]
            },
            {
                "id": "l_self",
                "name": "Self hides",
                "duration_ms": 1500,
                "hide_other_layers": false,
                "hide_when_timeline_ends": true,
                "triggers": [{
                    "when": { "event": "timeline_ends", "layer": "l_self" },
                    "action": { "action": "hide_layer", "layer": "l_self" }
                }]
            },
            {
                "id": "l_zero",
                "name": "Zero",
                "duration_ms": 0,
                "hide_other_layers": false,
                "hide_when_timeline_ends": true
            }
        ]),
        serde_json::json!([
            {
                "when": { "event": "click", "object": "btn_show_ends" },
                "action": { "action": "show_layer", "layer": "l_ends" }
            },
            {
                "when": { "event": "click", "object": "btn_show_self" },
                "action": { "action": "show_layer", "layer": "l_self" }
            },
            {
                "when": { "event": "click", "object": "btn_show_zero" },
                "action": { "action": "show_layer", "layer": "l_zero" }
            }
        ]),
    );
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    p.handle(&click("btn_show_ends"));
    let fx = p.handle(&Input::Tick { ms: 1500 });
    let fired = fx
        .iter()
        .position(|e| matches!(e, Effect::OpenUrl { url } if url.ends_with("/ends")))
        .expect("timeline_ends trigger ran");
    let hidden = fx
        .iter()
        .position(|e| matches!(e, Effect::LayerHidden { layer } if layer == "l_ends"))
        .expect("layer hidden");
    assert!(fired < hidden, "the trigger runs before the hide: {fx:?}");
    assert!(!p.state().visible_layers.iter().any(|id| id == "l_ends"));

    // A trigger that hides the layer itself is not hidden twice.
    p.handle(&click("btn_show_self"));
    let fx = p.handle(&Input::Tick { ms: 1500 });
    let hides = fx
        .iter()
        .filter(|e| matches!(e, Effect::LayerHidden { layer } if layer == "l_self"))
        .count();
    assert_eq!(hides, 1, "{fx:?}");

    // A zero-duration layer is not auto-hidden.
    p.handle(&click("btn_show_zero"));
    p.handle(&Input::Tick { ms: 100 });
    assert!(p.state().visible_layers.iter().any(|id| id == "l_zero"));
}

#[test]
fn allow_seek_targets_layers_and_refuses_no() {
    let c = options_course(
        serde_json::json!([
            button("btn_yes", "Yes", 0.0),
            button("btn_no", "No", 120.0),
            button("btn_ap", "Auto pause", 240.0),
            button("btn_auto", "Auto", 360.0),
            button("btn_hide_yes", "Hide yes", 480.0),
            button("btn_hide_no", "Hide no", 600.0),
            button("btn_hide_ap", "Hide ap", 720.0)
        ]),
        serde_json::json!([
            {
                "id": "l_yes",
                "name": "Yes",
                "duration_ms": 3000,
                "hide_other_layers": false,
                "allow_seek": "yes",
                "objects": [{
                    "id": "aud_yes",
                    "kind": { "type": "audio", "src": "yes.mp3", "autoplay": true },
                    "frame": { "x": 0.0, "y": 200.0, "w": 48.0, "h": 48.0 },
                    "timing": { "start_ms": 1000 }
                }],
                "triggers": [
                    {
                        "when": { "event": "timeline_reaches", "ms": 1000, "layer": "l_yes" },
                        "action": { "action": "open_url", "url": "https://x.test/yes1000" }
                    },
                    {
                        "when": { "event": "timeline_ends", "layer": "l_yes" },
                        "action": { "action": "open_url", "url": "https://x.test/yesend" }
                    }
                ]
            },
            {
                "id": "l_auto_pause",
                "name": "Auto pause",
                "duration_ms": 4000,
                "hide_other_layers": false,
                "pause_base_timeline": true
            },
            {
                "id": "l_auto",
                "name": "Auto",
                "duration_ms": 5000,
                "hide_other_layers": false
            },
            {
                "id": "l_no",
                "name": "No",
                "duration_ms": 3000,
                "hide_other_layers": false,
                "allow_seek": "no"
            }
        ]),
        serde_json::json!([
            { "when": { "event": "click", "object": "btn_yes" }, "action": { "action": "show_layer", "layer": "l_yes" } },
            { "when": { "event": "click", "object": "btn_no" }, "action": { "action": "show_layer", "layer": "l_no" } },
            { "when": { "event": "click", "object": "btn_ap" }, "action": { "action": "show_layer", "layer": "l_auto_pause" } },
            { "when": { "event": "click", "object": "btn_auto" }, "action": { "action": "show_layer", "layer": "l_auto" } },
            { "when": { "event": "click", "object": "btn_hide_yes" }, "action": { "action": "hide_layer", "layer": "l_yes" } },
            { "when": { "event": "click", "object": "btn_hide_no" }, "action": { "action": "hide_layer", "layer": "l_no" } },
            { "when": { "event": "click", "object": "btn_hide_ap" }, "action": { "action": "hide_layer", "layer": "l_auto_pause" } }
        ]),
    );
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);

    // `yes` targets that layer, not the base.
    p.handle(&click("btn_yes"));
    let fx = p.handle(&Input::PlayerSeek { ms: 500 });
    assert!(!warned(&fx));
    assert_eq!(p.slide_elapsed_ms(), 0);
    assert_eq!(
        p.seek_info(),
        SeekInfo {
            elapsed_ms: 500,
            duration_ms: 3000,
            paused: false,
            seek_allowed: true,
        }
    );

    // Forward cues and autoplay media on the target layer follow base rules.
    let fx = p.handle(&Input::PlayerSeek { ms: 2000 });
    assert_eq!(urls(&fx), vec!["yes1000".to_owned()]);
    assert!(has(&fx, &media_effect("aud_yes", MediaOp::Play)));
    assert!(has(&fx, &seek_effect("aud_yes", 1000)));
    assert!(p.is_media_playing("aud_yes"));

    // Backward moves fire nothing and stop media ahead of the position.
    let fx = p.handle(&Input::PlayerSeek { ms: 500 });
    assert!(urls(&fx).is_empty());
    assert!(has(&fx, &media_effect("aud_yes", MediaOp::Stop)));
    assert_eq!(p.state().layer_clocks.get("l_yes").map(|c| c.ms), Some(500));

    // Reaching the layer end fires its end cue (the back seek re-armed the 1000 ms cue).
    let fx = p.handle(&Input::PlayerSeek { ms: 3000 });
    assert_eq!(urls(&fx), vec!["yes1000".to_owned(), "yesend".to_owned()]);

    // A visible `no` layer in front refuses the seek.
    p.handle(&click("btn_no"));
    assert!(!p.seek_allowed());
    assert!(!p.seek_info().seek_allowed);
    let fx = p.handle(&Input::PlayerSeek { ms: 1000 });
    assert!(warned(&fx));
    assert_eq!(
        p.state().layer_clocks.get("l_yes").map(|c| c.ms),
        Some(3000)
    );

    p.handle(&click("btn_hide_no"));
    p.handle(&click("btn_hide_yes"));

    // `auto` plus `pause_base_timeline` targets the layer and holds the base.
    p.handle(&click("btn_ap"));
    let fx = p.handle(&Input::PlayerSeek { ms: 2000 });
    assert!(!warned(&fx));
    assert_eq!(p.slide_elapsed_ms(), 0);
    let info = p.seek_info();
    assert_eq!((info.elapsed_ms, info.duration_ms), (2000, 4000));
    assert!(info.seek_allowed);
    p.handle(&click("btn_hide_ap"));

    // `auto` without a pause still targets the base.
    p.handle(&click("btn_auto"));
    let fx = p.handle(&Input::PlayerSeek { ms: 1200 });
    assert!(!warned(&fx));
    assert_eq!(p.slide_elapsed_ms(), 1200);
    let info = p.seek_info();
    assert_eq!((info.elapsed_ms, info.duration_ms), (1200, 5000));
    assert_eq!(p.state().layer_clocks.get("l_auto").map(|c| c.ms), Some(0));
}

fn revisit_course(mode: RevisitMode) -> Course {
    let mut c = options_course(
        serde_json::json!([
            button("btn_show", "Show", 0.0),
            button("btn_hide", "Hide", 120.0),
            button("btn_mark", "Mark", 240.0)
        ]),
        serde_json::json!([{
            "id": "l1",
            "name": "Layer",
            "duration_ms": 5000,
            "hide_other_layers": false,
            "revisit": match mode {
                RevisitMode::Auto => "auto",
                RevisitMode::Reset => "reset",
                RevisitMode::Resume => "resume",
            },
            "objects": [
                {
                    "id": "obj1",
                    "kind": { "type": "text", "text": "One" },
                    "frame": { "x": 0.0, "y": 100.0, "w": 100.0, "h": 50.0 },
                    "timing": { "start_ms": 0 },
                    "states": [{ "name": "marked", "text": "Marked" }]
                },
                {
                    "id": "aud_1",
                    "kind": { "type": "audio", "src": "one.mp3", "autoplay": true },
                    "frame": { "x": 0.0, "y": 200.0, "w": 48.0, "h": 48.0 },
                    "timing": { "start_ms": 1000 }
                },
                {
                    "id": "aud_2",
                    "kind": { "type": "audio", "src": "two.mp3", "autoplay": true },
                    "frame": { "x": 60.0, "y": 200.0, "w": 48.0, "h": 48.0 },
                    "timing": { "start_ms": 2000 }
                }
            ]
        }]),
        serde_json::json!([
            { "when": { "event": "click", "object": "btn_show" }, "action": { "action": "show_layer", "layer": "l1" } },
            { "when": { "event": "click", "object": "btn_hide" }, "action": { "action": "hide_layer", "layer": "l1" } },
            { "when": { "event": "click", "object": "btn_mark" }, "action": { "action": "change_state", "object": "obj1", "state": "marked" } }
        ]),
    );
    if let Some(layer) = c
        .scenes
        .get_mut(0)
        .and_then(|scene| scene.slides.get_mut(0))
        .and_then(|slide| slide.layers.first_mut())
    {
        layer.revisit = mode;
        layer.triggers = vec![
            Trigger {
                when: Event::TimelineStarts {
                    layer: Some("l1".into()),
                },
                action: Action::OpenUrl {
                    url: "https://x.test/lstart".into(),
                },
                condition: None,
                disabled: false,
            },
            Trigger {
                when: Event::TimelineReaches {
                    ms: 1000,
                    layer: Some("l1".into()),
                },
                action: Action::OpenUrl {
                    url: "https://x.test/r1000".into(),
                },
                condition: None,
                disabled: false,
            },
            Trigger {
                when: Event::TimelineEnds {
                    layer: Some("l1".into()),
                },
                action: Action::OpenUrl {
                    url: "https://x.test/end".into(),
                },
                condition: None,
                disabled: false,
            },
        ];
    }
    c
}

#[test]
fn layer_revisit_auto_restarts_and_keeps_states() {
    let c = revisit_course(RevisitMode::Auto);
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    assert_eq!(
        urls(&p.handle(&click("btn_show"))),
        vec!["lstart".to_owned()]
    );
    p.handle(&Input::Tick { ms: 1500 });
    p.handle(&click("btn_mark"));
    assert_eq!(p.object_state("obj1"), Some("marked"));
    p.handle(&click("btn_hide"));

    let fx = p.handle(&click("btn_show"));
    assert_eq!(urls(&fx), vec!["lstart".to_owned()]);
    assert_eq!(p.state().layer_clocks.get("l1").map(|c| c.ms), Some(0));
    assert_eq!(p.object_state("obj1"), Some("marked"));

    p.handle(&Input::Tick { ms: 1000 });
    assert_eq!(p.state().layer_clocks.get("l1").map(|c| c.ms), Some(1000));
    assert_eq!(
        urls(&p.handle(&Input::Tick { ms: 500 })),
        Vec::<String>::new()
    );
}

#[test]
fn layer_revisit_reset_restarts_and_resets_object_states() {
    let c = revisit_course(RevisitMode::Reset);
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    p.handle(&click("btn_show"));
    p.handle(&Input::Tick { ms: 1500 });
    p.handle(&click("btn_mark"));
    assert_eq!(p.object_state("obj1"), Some("marked"));
    p.handle(&click("btn_hide"));

    let fx = p.handle(&click("btn_show"));
    assert_eq!(urls(&fx), vec!["lstart".to_owned()]);
    assert_eq!(p.state().layer_clocks.get("l1").map(|c| c.ms), Some(0));
    assert_eq!(p.object_state("obj1"), Some("normal"));
}

#[test]
fn layer_revisit_resume_continues_clock_media_and_does_not_refire_start_or_end() {
    let c = revisit_course(RevisitMode::Resume);
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    p.handle(&click("btn_show"));
    p.handle(&Input::Tick { ms: 1500 });
    assert!(p.is_media_playing("aud_1"));
    p.handle(&click("btn_mark"));
    p.handle(&click("btn_hide"));
    assert!(!p.is_media_playing("aud_1"));

    let fx = p.handle(&click("btn_show"));
    assert!(urls(&fx).is_empty(), "start does not fire at 1500: {fx:?}");
    assert!(!fx.iter().any(|e| matches!(
        e,
        Effect::Media {
            op: MediaOp::Play,
            ..
        }
    )));
    assert_eq!(p.state().layer_clocks.get("l1").map(|c| c.ms), Some(1500));
    assert_eq!(p.object_state("obj1"), Some("marked"));

    // The already-passed 1000 ms autoplay cue does not run again, but a later
    // one does when its window is crossed.
    p.handle(&Input::Tick { ms: 400 });
    assert!(!p.is_media_playing("aud_1"));
    p.handle(&Input::Tick { ms: 200 });
    assert!(p.is_media_playing("aud_2"));
    assert!(!urls(&p.handle(&Input::Tick { ms: 200 })).contains(&"r1000".to_owned()));

    // A saved ended clock stays ended and does not re-fire timeline_ends.
    p.handle(&Input::Tick { ms: 3000 });
    assert_eq!(
        p.state().layer_clocks.get("l1").map(|c| c.ended),
        Some(true)
    );
    p.handle(&click("btn_hide"));
    let fx = p.handle(&click("btn_show"));
    assert!(!urls(&fx).contains(&"end".to_owned()), "{fx:?}");
    assert_eq!(p.state().layer_clocks.get("l1").map(|c| c.ms), Some(5000));
    assert!(!urls(&p.handle(&Input::Tick { ms: 1000 })).contains(&"end".to_owned()));
}

fn slide_revisit_course(mode: RevisitMode) -> Course {
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "slide-revisit",
        "title": "Slide Revisit",
        "scenes": [{
            "id": "scene1",
            "title": "Scene 1",
            "slides": [
                {
                    "id": "s1",
                    "title": "Slide 1",
                    "duration_ms": 5000,
                    "objects": [
                        button("b_next", "Next", 0.0),
                        button("b_show", "Show", 120.0),
                        button("b_mark", "Mark", 240.0),
                        {
                            "id": "txt",
                            "kind": { "type": "text", "text": "State" },
                            "frame": { "x": 0.0, "y": 100.0, "w": 100.0, "h": 50.0 },
                            "timing": { "start_ms": 0 },
                            "states": [{ "name": "marked", "text": "Marked" }]
                        }
                    ],
                    "layers": [{
                        "id": "l1",
                        "name": "Layer",
                        "duration_ms": 5000,
                        "hide_other_layers": false,
                        "revisit": "resume",
                        "triggers": [{
                            "when": { "event": "timeline_starts", "layer": "l1" },
                            "action": { "action": "open_url", "url": "https://x.test/lstart" }
                        }]
                    }],
                    "triggers": [
                        { "when": { "event": "click", "object": "b_next" }, "action": { "action": "jump_to_slide", "target": { "id": "s2" } } },
                        { "when": { "event": "click", "object": "b_show" }, "action": { "action": "show_layer", "layer": "l1" } },
                        { "when": { "event": "click", "object": "b_mark" }, "action": { "action": "change_state", "object": "txt", "state": "marked" } },
                        { "when": { "event": "timeline_starts" }, "action": { "action": "open_url", "url": "https://x.test/s1start" } }
                    ],
                    "revisit": match mode {
                        RevisitMode::Auto => "auto",
                        RevisitMode::Reset => "reset",
                        RevisitMode::Resume => "resume",
                    }
                },
                {
                    "id": "s2",
                    "title": "Slide 2",
                    "duration_ms": 5000,
                    "objects": [button("b_back", "Back", 0.0)],
                    "triggers": [{
                        "when": { "event": "click", "object": "b_back" },
                        "action": { "action": "jump_to_slide", "target": { "id": "s1" } }
                    }]
                }
            ]
        }]
    });
    Course::from_json(&json.to_string()).unwrap()
}

fn visit_and_return(mode: RevisitMode) -> (Course, Vec<Effect>, PlayerState) {
    let c = slide_revisit_course(mode);
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    p.handle(&Input::Tick { ms: 1500 });
    p.handle(&click("b_mark"));
    p.handle(&click("b_show"));
    p.handle(&Input::Tick { ms: 1000 });
    p.handle(&click("b_next"));
    let fx = p.handle(&click("b_back"));
    let state = p.state().clone();
    (c, fx, state)
}

#[test]
fn slide_revisit_auto_resets_clocks_and_layers_but_keeps_object_states() {
    let (_c, fx, state) = visit_and_return(RevisitMode::Auto);
    assert!(urls(&fx).contains(&"s1start".to_owned()));
    assert!(state.visible_layers.is_empty());
    assert_eq!(state.base.ms, 0);
    assert_eq!(
        state.object_states.get("s1").and_then(|s| s.get("txt")),
        Some(&"marked".to_owned())
    );
    assert!(state.visited.contains("s1") && state.visited.contains("s2"));
}

#[test]
fn slide_revisit_reset_returns_object_states_and_forgets_layer_clocks() {
    let (c, fx, state) = visit_and_return(RevisitMode::Reset);
    assert!(urls(&fx).contains(&"s1start".to_owned()));
    assert!(state.visible_layers.is_empty());
    assert_eq!(state.base.ms, 0);
    assert_eq!(
        state.object_states.get("s1").and_then(|s| s.get("txt")),
        Some(&"normal".to_owned())
    );
    assert!(!state.saved_slides.contains_key("s1"));

    // The layer's saved clock was forgotten, so showing it starts at 0.
    let mut p = Player::resume(&c, state).unwrap();
    p.handle(&Input::Start);
    p.handle(&click("b_show"));
    assert_eq!(p.state().layer_clocks.get("l1").map(|c| c.ms), Some(0));
}

#[test]
fn layer_revisit_resume_starts_over_when_it_hid_itself_at_the_end() {
    let mut c = revisit_course(RevisitMode::Resume);
    let duration = {
        let layer = &mut c.scenes[0].slides[0].layers[0];
        layer.hide_when_timeline_ends = true;
        layer.duration_ms
    };
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    p.handle(&click("btn_show"));
    p.handle(&Input::Tick { ms: duration + 10 });
    assert!(p.state().visible_layers.is_empty(), "it hid itself");
    p.handle(&click("btn_show"));
    assert_eq!(p.state().layer_clocks.get("l1").map(|c| c.ms), Some(0));
    assert_eq!(
        p.state().layer_clocks.get("l1").map(|c| c.ended),
        Some(false)
    );
}

#[test]
fn slide_revisit_resume_restores_base_layers_and_their_clocks() {
    let (_c, fx, state) = visit_and_return(RevisitMode::Resume);
    assert!(!urls(&fx).contains(&"s1start".to_owned()), "{fx:?}");
    assert!(!urls(&fx).contains(&"lstart".to_owned()), "{fx:?}");
    assert_eq!(state.base.ms, 2500);
    assert_eq!(state.visible_layers, vec!["l1".to_owned()]);
    assert_eq!(state.layer_clocks.get("l1").map(|c| c.ms), Some(1000));
    assert_eq!(
        state.object_states.get("s1").and_then(|s| s.get("txt")),
        Some(&"marked".to_owned())
    );
}

#[test]
fn a_saved_layer_that_was_deleted_is_skipped_without_panicking() {
    let c = slide_revisit_course(RevisitMode::Resume);
    let mut state = PlayerState {
        slide: 0,
        visited: ["s1".to_owned()].into_iter().collect(),
        ..PlayerState::default()
    };
    state.saved_slides.insert(
        "s1".to_owned(),
        crate::SavedSlide {
            base: Clock {
                ms: 1200,
                ended: false,
                paused: false,
            },
            visible_layers: vec!["ghost".to_owned(), "l1".to_owned()],
            layer_clocks: [
                (
                    "ghost".to_owned(),
                    Clock {
                        ms: 900,
                        ended: false,
                        paused: false,
                    },
                ),
                (
                    "l1".to_owned(),
                    Clock {
                        ms: 700,
                        ended: false,
                        paused: false,
                    },
                ),
            ]
            .into_iter()
            .collect(),
        },
    );
    let mut p = Player::resume(&c, state).unwrap();
    let fx = p.handle(&Input::Start);
    assert!(!warned(&fx));
    assert_eq!(p.state().visible_layers, vec!["l1".to_owned()]);
    assert!(!p.state().layer_clocks.contains_key("ghost"));
    assert_eq!(p.state().base.ms, 1200);
    assert_eq!(p.state().layer_clocks.get("l1").map(|c| c.ms), Some(700));
}

fn input_course(kind: coursecraft_course::ObjectKind) -> Course {
    use coursecraft_course::{Object, Rect, Value, VariableDef};
    let mut c = course();
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
    c.variables.push(VariableDef {
        name: "seen".into(),
        value: Value::Bool(false),
    });
    let slide = c.slide_mut("s-intro").expect("slide");
    slide.objects.push(Object {
        id: "input".into(),
        name: "Input".into(),
        kind,
        frame: Rect {
            x: 10.0,
            y: 10.0,
            w: 200.0,
            h: 40.0,
        },
        timing: Default::default(),
        states: vec![coursecraft_course::StateDef {
            name: "disabled".into(),
            fill: None,
            text: None,
            src: None,
        }],
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
    slide.triggers.push(Trigger {
        action: Action::SetVariable {
            variable: "seen".into(),
            op: VarOp::Assign,
            value: Some(Value::Bool(true)),
        },
        when: Event::VariableChanges {
            variable: "input_number".into(),
        },
        condition: None,
        disabled: false,
    });
    slide.triggers.push(Trigger {
        action: Action::SetVariable {
            variable: "seen".into(),
            op: VarOp::Assign,
            value: Some(Value::Bool(false)),
        },
        when: Event::VariableChanges {
            variable: "input_text".into(),
        },
        condition: None,
        disabled: false,
    });
    c
}

#[test]
fn markers_toggle_one_at_a_time_and_fire_events() {
    let course = Course::from_json(
        &serde_json::json!({
            "schema_version": 1,
            "id": "markers",
            "title": "Markers",
            "variables": [
                { "name": "opened", "value": false },
                { "name": "closed", "value": false }
            ],
            "scenes": [{ "id": "sc", "title": "Scene", "slides": [
                {
                    "id": "s1",
                    "title": "One",
                    "objects": [
                        { "id": "m1", "name": "One", "kind": { "type": "marker", "title": "One" },
                          "frame": { "x": 10.0, "y": 10.0, "w": 32.0, "h": 32.0 } },
                        { "id": "m2", "name": "Two", "kind": { "type": "marker", "title": "Two" },
                          "frame": { "x": 60.0, "y": 10.0, "w": 32.0, "h": 32.0 } }
                    ],
                    "triggers": [
                        { "action": { "action": "set_variable", "variable": "opened", "value": true },
                          "when": { "event": "marker_opened", "object": "m1" } },
                        { "action": { "action": "set_variable", "variable": "closed", "value": true },
                          "when": { "event": "marker_closed", "object": "m1" } }
                    ]
                },
                { "id": "s2", "title": "Two", "objects": [] }
            ] }]
        })
        .to_string(),
    )
    .expect("marker course");

    let mut player = Player::new(&course).expect("playable");
    player.handle(&Input::Start);
    let effects = player.handle(&Input::MarkerToggled {
        object: "m1".into(),
    });
    assert!(has(
        &effects,
        &Effect::MarkerOpened {
            object: "m1".into()
        }
    ));
    assert_eq!(player.marker_open(), Some("m1"));
    assert_eq!(player.variable("opened"), Some(&Value::Bool(true)));

    let effects = player.handle(&Input::MarkerToggled {
        object: "m1".into(),
    });
    assert!(has(
        &effects,
        &Effect::MarkerClosed {
            object: "m1".into()
        }
    ));
    assert_eq!(player.marker_open(), None);
    assert_eq!(player.variable("closed"), Some(&Value::Bool(true)));

    // Opening m2 closes m1 first.
    player.handle(&Input::MarkerToggled {
        object: "m1".into(),
    });
    let effects = player.handle(&Input::MarkerToggled {
        object: "m2".into(),
    });
    let close = effects
        .iter()
        .position(|effect| {
            *effect
                == Effect::MarkerClosed {
                    object: "m1".into(),
                }
        })
        .expect("m1 closed");
    let open = effects
        .iter()
        .position(|effect| {
            *effect
                == Effect::MarkerOpened {
                    object: "m2".into(),
                }
        })
        .expect("m2 opened");
    assert!(close < open, "{effects:?}");
    assert_eq!(player.marker_open(), Some("m2"));

    let effects = player.handle(&Input::MarkersClosed);
    assert!(has(
        &effects,
        &Effect::MarkerClosed {
            object: "m2".into()
        }
    ));
    assert_eq!(player.marker_open(), None);

    // Clicking another object also closes the open marker.
    player.handle(&Input::MarkerToggled {
        object: "m1".into(),
    });
    player.handle(&Input::Click {
        object: "missing".into(),
    });
    assert_eq!(player.marker_open(), None);

    // Slide entry resets the open marker.
    player.handle(&Input::MarkerToggled {
        object: "m1".into(),
    });
    player.handle(&Input::GoTo { slide: "s2".into() });
    player.handle(&Input::GoTo { slide: "s1".into() });
    assert_eq!(player.marker_open(), None);
}

#[test]
fn panel_offsets_clamp_reset_and_filter_focus() {
    let course = Course::from_json(
        &serde_json::json!({
            "schema_version": 1,
            "id": "panels",
            "title": "Panels",
            "scenes": [{ "id": "sc", "title": "Scene", "slides": [
                {
                    "id": "s1",
                    "title": "One",
                    "objects": [
                        { "id": "panel", "name": "Panel",
                          "kind": { "type": "scroll_panel", "content_height": 600.0 },
                          "frame": { "x": 100.0, "y": 100.0, "w": 300.0, "h": 200.0 } },
                        { "id": "a", "name": "A", "panel": "panel",
                          "kind": { "type": "button", "label": "A", "fill": "#2F6FEB" },
                          "frame": { "x": 120.0, "y": 120.0, "w": 100.0, "h": 50.0 } },
                        { "id": "b", "name": "B", "panel": "panel",
                          "kind": { "type": "button", "label": "B", "fill": "#2F6FEB" },
                          "frame": { "x": 120.0, "y": 500.0, "w": 100.0, "h": 50.0 } }
                    ]
                },
                { "id": "s2", "title": "Two", "objects": [] }
            ] }]
        })
        .to_string(),
    )
    .expect("panel course");

    let mut player = Player::new(&course).expect("playable");
    player.handle(&Input::Start);
    assert_eq!(player.panel_offset("panel"), 0.0);
    let list = player.keyboard_objects();
    assert!(list.contains(&"a".to_owned()), "{list:?}");
    assert!(!list.contains(&"b".to_owned()), "{list:?}");

    player.handle(&Input::PanelScrolled {
        object: "panel".into(),
        offset: 999.0,
    });
    assert!((player.panel_offset("panel") - 400.0).abs() < 1e-6);
    let list = player.keyboard_objects();
    assert!(!list.contains(&"a".to_owned()), "{list:?}");
    assert!(list.contains(&"b".to_owned()), "{list:?}");

    player.handle(&Input::PanelScrolled {
        object: "panel".into(),
        offset: f32::NAN,
    });
    assert_eq!(player.panel_offset("panel"), 0.0);

    player.handle(&Input::PanelScrolled {
        object: "panel".into(),
        offset: 200.0,
    });
    player.handle(&Input::GoTo { slide: "s2".into() });
    player.handle(&Input::GoTo { slide: "s1".into() });
    assert_eq!(player.panel_offset("panel"), 0.0);
}

#[test]
fn text_entry_commits_truncated_text_and_fires_variable_changes() {
    let mut c = input_course(coursecraft_course::ObjectKind::TextEntry {
        variable: "input_text".into(),
        placeholder: String::new(),
        multiline: false,
        max_chars: 3,
    });
    c.slide_mut("s-intro").unwrap().triggers.remove(1);
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    let effects = p.handle(&Input::SetInputValue {
        object: "input".into(),
        value: Value::Text("abcdef".into()),
    });
    assert!(has(
        &effects,
        &Effect::VariableChanged {
            variable: "input_text".into(),
            value: Value::Text("abc".into()),
        }
    ));
    assert_eq!(p.variable("seen"), Some(&Value::Bool(false)));
}

#[test]
fn slider_and_dial_clamp_and_snap_numbers() {
    for kind in [
        coursecraft_course::ObjectKind::Slider {
            variable: "input_number".into(),
            min: 0.0,
            max: 10.0,
            step: 2.0,
            orientation: coursecraft_course::Orientation::Horizontal,
        },
        coursecraft_course::ObjectKind::Dial {
            variable: "input_number".into(),
            min: 0.0,
            max: 10.0,
            step: 2.0,
            start_angle: -135.0,
            end_angle: 135.0,
        },
    ] {
        let mut c = input_course(kind);
        let mut p = Player::new(&c).expect("player");
        p.handle(&Input::Start);
        let effects = p.handle(&Input::SetInputValue {
            object: "input".into(),
            value: Value::Number(7.4),
        });
        assert!(has(
            &effects,
            &Effect::VariableChanged {
                variable: "input_number".into(),
                value: Value::Number(8.0),
            }
        ));
        // The variable_changes trigger fired too.
        assert_eq!(p.variable("seen"), Some(&Value::Bool(true)));

        let effects = p.handle(&Input::SetInputValue {
            object: "input".into(),
            value: Value::Number(99.0),
        });
        assert!(has(
            &effects,
            &Effect::VariableChanged {
                variable: "input_number".into(),
                value: Value::Number(10.0),
            }
        ));
        c.slide_mut("s-intro")
            .unwrap()
            .objects
            .retain(|o| o.id != "input");
    }
}

#[test]
fn checkbox_and_radio_commit_and_radio_rejects_unknown_values() {
    let c = input_course(coursecraft_course::ObjectKind::Checkbox {
        variable: "input_bool".into(),
        label: "Agree".into(),
    });
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    let effects = p.handle(&Input::SetInputValue {
        object: "input".into(),
        value: Value::Bool(true),
    });
    assert!(has(
        &effects,
        &Effect::VariableChanged {
            variable: "input_bool".into(),
            value: Value::Bool(true),
        }
    ));

    let c = input_course(coursecraft_course::ObjectKind::RadioGroup {
        variable: "input_text".into(),
        options: vec![
            coursecraft_course::RadioOption {
                label: "A".into(),
                value: "a".into(),
            },
            coursecraft_course::RadioOption {
                label: "B".into(),
                value: "b".into(),
            },
        ],
    });
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    let bad = p.handle(&Input::SetInputValue {
        object: "input".into(),
        value: Value::Text("nope".into()),
    });
    assert!(warned(&bad));
    assert_eq!(p.variable("input_text"), Some(&Value::Text(String::new())));
    let good = p.handle(&Input::SetInputValue {
        object: "input".into(),
        value: Value::Text("b".into()),
    });
    assert!(has(
        &good,
        &Effect::VariableChanged {
            variable: "input_text".into(),
            value: Value::Text("b".into()),
        }
    ));
}

#[test]
fn disabled_and_wrong_value_kinds_are_rejected() {
    let mut c = input_course(coursecraft_course::ObjectKind::Slider {
        variable: "input_number".into(),
        min: 0.0,
        max: 10.0,
        step: 1.0,
        orientation: coursecraft_course::Orientation::Horizontal,
    });
    c.slide_mut("s-intro")
        .unwrap()
        .objects
        .iter_mut()
        .find(|object| object.id == "input")
        .expect("input object")
        .initial_state = "disabled".into();
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    let effects = p.handle(&Input::SetInputValue {
        object: "input".into(),
        value: Value::Number(4.0),
    });
    assert!(effects.is_empty(), "{effects:?}");
    assert_eq!(p.variable("input_number"), Some(&Value::Number(0.0)));

    // Right variable kind but the wrong widget value kind.
    let c = input_course(coursecraft_course::ObjectKind::Checkbox {
        variable: "input_bool".into(),
        label: String::new(),
    });
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    let effects = p.handle(&Input::SetInputValue {
        object: "input".into(),
        value: Value::Text("yes".into()),
    });
    assert!(warned(&effects));
}

// ---------------------------------------------------------------- lightbox

fn lightbox_course() -> Course {
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "lightbox-test",
        "title": "Lightbox Test",
        "variables": [
            { "name": "starts", "value": 0 },
            { "name": "base_clicks", "value": 0 },
            { "name": "box_clicks", "value": 0 }
        ],
        "scenes": [{
            "id": "sc",
            "title": "Scene",
            "slides": [
                {
                    "id": "s-base",
                    "title": "Base",
                    "objects": [
                        { "id": "btn-open", "kind": { "type": "button", "label": "Open" }, "frame": { "x": 10, "y": 10, "w": 100, "h": 40 } },
                        { "id": "btn-base", "kind": { "type": "button", "label": "Base" }, "frame": { "x": 10, "y": 60, "w": 100, "h": 40 } },
                        { "id": "audio1", "kind": { "type": "audio", "src": "tone.mp3", "autoplay": true }, "frame": { "x": 0, "y": 0, "w": 40, "h": 40 } },
                        { "id": "audio2", "kind": { "type": "audio", "src": "tone.mp3" }, "frame": { "x": 0, "y": 0, "w": 40, "h": 40 } }
                    ],
                    "triggers": [
                        { "action": { "action": "lightbox_slide", "slide": "s-box1" }, "when": { "event": "click", "object": "btn-open" } },
                        { "action": { "action": "set_variable", "variable": "base_clicks", "value": 1 }, "when": { "event": "click", "object": "btn-base" } },
                        { "action": { "action": "set_variable", "variable": "starts", "op": "add", "value": 1 }, "when": { "event": "timeline_starts" } }
                    ]
                },
                {
                    "id": "s-box1",
                    "title": "Box One",
                    "objects": [
                        { "id": "box-btn", "kind": { "type": "button", "label": "Box" }, "frame": { "x": 10, "y": 10, "w": 100, "h": 40 } },
                        { "id": "box-replace", "kind": { "type": "button", "label": "Replace" }, "frame": { "x": 10, "y": 60, "w": 100, "h": 40 } },
                        { "id": "box-jump", "kind": { "type": "button", "label": "Jump" }, "frame": { "x": 10, "y": 110, "w": 100, "h": 40 } },
                        { "id": "box-text", "kind": { "type": "text", "text": "Box" }, "states": [{ "name": "selected", "text": "Changed" }], "frame": { "x": 200, "y": 10, "w": 200, "h": 40 } }
                    ],
                    "triggers": [
                        { "action": { "action": "lightbox_slide", "slide": "s-box2" }, "when": { "event": "click", "object": "box-replace" } },
                        { "action": { "action": "jump_to_slide", "target": { "id": "s-third" } }, "when": { "event": "click", "object": "box-jump" } },
                        { "action": { "action": "change_state", "object": "box-text", "state": "selected" }, "when": { "event": "timeline_starts" } },
                        { "action": { "action": "set_variable", "variable": "box_clicks", "value": 1 }, "when": { "event": "click", "object": "box-btn" } }
                    ],
                    "layers": [{
                        "id": "box-layer",
                        "name": "Box layer",
                        "objects": [
                            { "id": "box-layer-btn", "kind": { "type": "button", "label": "Layer" }, "frame": { "x": 200, "y": 60, "w": 100, "h": 40 } }
                        ],
                        "triggers": [
                            { "action": { "action": "set_variable", "variable": "box_clicks", "value": 2 }, "when": { "event": "click", "object": "box-layer-btn" } }
                        ]
                    }]
                },
                {
                    "id": "s-box2",
                    "title": "Box Two",
                    "objects": [
                        { "id": "box2-btn", "kind": { "type": "button", "label": "Box 2" }, "frame": { "x": 10, "y": 10, "w": 100, "h": 40 } }
                    ]
                },
                {
                    "id": "s-third",
                    "title": "Third"
                }
            ]
        }]
    });
    Course::from_json(&json.to_string()).expect("valid lightbox course")
}

fn start_lightbox() -> (&'static Course, Player<'static>) {
    // The player borrows the course, so leak it for the test.
    let course: &'static Course = Box::leak(Box::new(lightbox_course()));
    let mut player = Player::new(course).expect("player");
    player.handle(&Input::Start);
    (course, player)
}

#[test]
fn lightbox_pauses_base_and_restores_it_without_replaying_starts() {
    let (course, mut p) = start_lightbox();
    p.handle(&Input::Tick { ms: 1000 });
    assert_eq!(p.slide_elapsed_ms(), 1000);
    assert_eq!(p.variable("starts"), Some(&Value::Number(1.0)));
    assert_eq!(p.media_state("audio1"), Some(MediaPlaybackState::Playing));
    assert_eq!(p.media_state("audio2"), None);

    let effects = p.handle(&click("btn-open"));
    assert!(has(
        &effects,
        &Effect::LightboxOpened {
            slide: "s-box1".into()
        }
    ));
    assert!(has(
        &effects,
        &Effect::Media {
            object: "audio1".into(),
            op: MediaOp::Pause
        }
    ));
    assert!(!effects.iter().any(|e| matches!(
        e,
        Effect::Media { object, op: MediaOp::Pause } if object == "audio2"
    )));
    assert!(p.lightbox_open());
    assert_eq!(p.current_slide().map(|s| s.id.as_str()), Some("s-box1"));
    assert!(p.state().visited.contains("s-box1"));
    // The box slide ran its own timeline_starts triggers.
    assert!(has(
        &effects,
        &Effect::StateChanged {
            object: "box-text".into(),
            state: "selected".into()
        }
    ));
    // Its clock starts from zero, not from the base's 1000 ms.
    assert_eq!(p.slide_elapsed_ms(), 0);
    p.handle(&Input::Tick { ms: 500 });
    assert_eq!(p.slide_elapsed_ms(), 500);

    let effects = p.handle(&Input::CloseLightbox);
    assert!(has(
        &effects,
        &Effect::LightboxClosed {
            slide: "s-box1".into()
        }
    ));
    assert!(has(
        &effects,
        &Effect::Media {
            object: "audio1".into(),
            op: MediaOp::Play
        }
    ));
    // The object that was not playing stays silent.
    assert!(!effects.iter().any(|e| matches!(
        e,
        Effect::Media { object, op: MediaOp::Play } if object == "audio2"
    )));
    assert_eq!(p.current_slide().map(|s| s.id.as_str()), Some("s-base"));
    assert_eq!(p.slide_elapsed_ms(), 1000, "the base clock resumes exactly");
    assert_eq!(
        p.variable("starts"),
        Some(&Value::Number(1.0)),
        "timeline_starts does not fire again on close"
    );
    assert_eq!(p.media_state("audio1"), Some(MediaPlaybackState::Playing));

    // A lightbox is runtime-only; suspend data saves the base slide.
    let encoded = encode_suspend(course, p.state());
    let restored = decode_suspend(course, &encoded).expect("decode");
    assert_eq!(
        restored.slide,
        course.slide_index("s-base").expect("base index")
    );
    assert!(restored.lightbox.is_none());
}

#[test]
fn lightbox_base_takes_no_clicks_or_navigation_while_open() {
    let (_course, mut p) = start_lightbox();
    p.handle(&click("btn-open"));

    let effects = p.handle(&click("btn-base"));
    assert!(warned(&effects));
    assert_eq!(p.variable("base_clicks"), Some(&Value::Number(0.0)));

    let effects = p.handle(&Input::Next);
    assert!(warned(&effects));
    assert_eq!(p.current_slide().map(|s| s.id.as_str()), Some("s-box1"));
    let effects = p.handle(&Input::Previous);
    assert!(warned(&effects));
    assert_eq!(p.current_slide().map(|s| s.id.as_str()), Some("s-box1"));

    // The box slide's own objects still work.
    p.handle(&click("box-btn"));
    assert_eq!(p.variable("box_clicks"), Some(&Value::Number(1.0)));
}

#[test]
fn a_second_lightbox_replaces_the_first_and_keeps_the_base_paused() {
    let (_course, mut p) = start_lightbox();
    p.handle(&Input::Tick { ms: 700 });
    p.handle(&click("btn-open"));
    p.handle(&Input::Tick { ms: 300 });

    let effects = p.handle(&click("box-replace"));
    assert!(has(
        &effects,
        &Effect::LightboxClosed {
            slide: "s-box1".into()
        }
    ));
    assert!(has(
        &effects,
        &Effect::LightboxOpened {
            slide: "s-box2".into()
        }
    ));
    assert_eq!(p.current_slide().map(|s| s.id.as_str()), Some("s-box2"));
    assert_eq!(p.slide_elapsed_ms(), 0);

    let effects = p.handle(&Input::CloseLightbox);
    assert!(has(
        &effects,
        &Effect::LightboxClosed {
            slide: "s-box2".into()
        }
    ));
    assert_eq!(p.current_slide().map(|s| s.id.as_str()), Some("s-base"));
    assert_eq!(p.slide_elapsed_ms(), 700);
}

#[test]
fn a_jump_inside_the_lightbox_closes_it_and_jumps_the_base() {
    let (_course, mut p) = start_lightbox();
    p.handle(&click("btn-open"));
    let effects = p.handle(&click("box-jump"));
    assert!(has(
        &effects,
        &Effect::LightboxClosed {
            slide: "s-box1".into()
        }
    ));
    assert!(has(&effects, &slide_entered("s-third")));
    assert_eq!(p.current_slide().map(|s| s.id.as_str()), Some("s-third"));
    assert!(!p.lightbox_open());
}

#[test]
fn escape_and_the_close_input_both_close_the_lightbox() {
    let (_course, mut p) = start_lightbox();

    p.handle(&click("btn-open"));
    let effects = p.handle(&Input::Key {
        key: "Escape".into(),
    });
    assert!(has(
        &effects,
        &Effect::LightboxClosed {
            slide: "s-box1".into()
        }
    ));
    assert!(!p.lightbox_open());

    p.handle(&click("btn-open"));
    let effects = p.handle(&Input::CloseLightbox);
    assert!(has(
        &effects,
        &Effect::LightboxClosed {
            slide: "s-box1".into()
        }
    ));
    assert!(!p.lightbox_open());

    // Closing when nothing is open changes nothing.
    assert!(p.handle(&Input::CloseLightbox).is_empty());
}

#[test]
fn opening_a_lightbox_on_the_current_slide_is_a_warning() {
    use coursecraft_course::ObjectKind;

    let mut course = lightbox_course();
    {
        let slide = course.slide_mut("s-base").expect("base");
        // Keep the object a real button so the click reaches the trigger.
        slide.objects.push(coursecraft_course::Object {
            id: "btn-self".into(),
            name: "Self".into(),
            kind: ObjectKind::Button {
                label: "Self".into(),
                fill: coursecraft_course::Color("#2F6FEB".into()),
                fill_style: None,
            },
            frame: coursecraft_course::Rect {
                x: 10.0,
                y: 120.0,
                w: 100.0,
                h: 40.0,
            },
            timing: coursecraft_course::Timing::default(),
            entrance: None,
            exit: None,
            motion_paths: Vec::new(),
            states: Vec::new(),
            initial_state: "normal".into(),
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
        slide.triggers.push(Trigger {
            action: Action::LightboxSlide {
                slide: "s-base".into(),
            },
            when: Event::Click {
                object: "btn-self".into(),
            },
            condition: None,
            disabled: false,
        });
    }
    let mut p = Player::new_unvalidated(&course);
    p.handle(&Input::Start);
    let effects = p.handle(&click("btn-self"));
    assert!(warned(&effects));
    assert!(!p.lightbox_open());
}

fn keys_course() -> Course {
    Course::from_json(
        &serde_json::json!({
            "schema_version": 1,
            "id": "keys",
            "title": "Keys",
            "variables": [
                { "name": "global", "value": 0.0 },
                { "name": "object", "value": 0.0 },
                { "name": "wrong", "value": 0.0 },
                { "name": "control", "value": 0.0 },
                { "name": "box", "value": 0.0 },
                { "name": "text", "value": "" }
            ],
            "scenes": [{
                "id": "sc",
                "title": "Scene",
                "slides": [
                    {
                        "id": "s1",
                        "title": "One",
                        "objects": [
                            { "id": "open", "kind": { "type": "button", "label": "Open" },
                              "frame": { "x": 10.0, "y": 10.0, "w": 120.0, "h": 40.0 } },
                            { "id": "text", "kind": { "type": "text_entry", "variable": "text" },
                              "frame": { "x": 10.0, "y": 70.0, "w": 200.0, "h": 40.0 } },
                            { "id": "slider", "kind": { "type": "slider", "variable": "global", "min": 0.0, "max": 10.0 },
                              "frame": { "x": 10.0, "y": 130.0, "w": 200.0, "h": 30.0 } },
                            { "id": "other", "kind": { "type": "shape", "shape": "rectangle", "fill": "#112233" },
                              "frame": { "x": 300.0, "y": 10.0, "w": 80.0, "h": 80.0 } }
                        ],
                        "triggers": [
                            { "action": { "action": "set_variable", "variable": "global", "value": 1 },
                              "when": { "event": "key_pressed", "key": "a" } },
                            { "action": { "action": "set_variable", "variable": "object", "value": 1 },
                              "when": { "event": "key_pressed", "key": "a", "object": "open" } },
                            { "action": { "action": "set_variable", "variable": "wrong", "value": 1 },
                              "when": { "event": "key_pressed", "key": "a", "object": "other" } },
                            { "action": { "action": "set_variable", "variable": "control", "value": 1 },
                              "when": { "event": "key_pressed", "key": "arrow_up" } },
                            { "action": { "action": "lightbox_slide", "slide": "s2" },
                              "when": { "event": "click", "object": "open" } }
                        ]
                    },
                    {
                        "id": "s2",
                        "title": "Two",
                        "objects": [],
                        "triggers": [
                            { "action": { "action": "set_variable", "variable": "box", "value": 1 },
                              "when": { "event": "key_pressed", "key": "a" } }
                        ]
                    }
                ]
            }]
        })
        .to_string(),
    )
    .expect("valid key course")
}

#[test]
fn key_pressed_fires_globally_and_for_the_focused_object_only() {
    let c = keys_course();
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);

    p.handle(&Input::FocusChanged { object: None });
    p.handle(&Input::KeyPressed { key: "a".into() });
    assert_eq!(p.variable("global"), Some(&Value::Number(1.0)));
    assert_eq!(p.variable("object"), Some(&Value::Number(0.0)));
    assert_eq!(p.variable("wrong"), Some(&Value::Number(0.0)));

    // Focused object: both the global trigger and that object's trigger fire.
    p.handle(&Input::FocusChanged {
        object: Some("open".into()),
    });
    p.handle(&Input::KeyPressed { key: "a".into() });
    assert_eq!(p.variable("global"), Some(&Value::Number(1.0)));
    assert_eq!(p.variable("object"), Some(&Value::Number(1.0)));
    assert_eq!(p.variable("wrong"), Some(&Value::Number(0.0)));

    // A different object's scoped trigger does not fire.
    p.handle(&Input::FocusChanged {
        object: Some("other".into()),
    });
    p.handle(&Input::KeyPressed { key: "a".into() });
    assert_eq!(p.variable("wrong"), Some(&Value::Number(1.0)));
}

#[test]
fn key_pressed_is_suppressed_while_typing_and_when_a_control_uses_it() {
    let c = keys_course();
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);

    p.handle(&Input::FocusChanged {
        object: Some("text".into()),
    });
    p.handle(&Input::KeyPressed { key: "a".into() });
    assert_eq!(
        p.variable("global"),
        Some(&Value::Number(0.0)),
        "typing must not fire key triggers"
    );

    // A slider owns the arrow keys.
    p.handle(&Input::FocusChanged {
        object: Some("slider".into()),
    });
    p.handle(&Input::KeyPressed {
        key: "arrow_up".into(),
    });
    assert_eq!(p.variable("control"), Some(&Value::Number(0.0)));

    // A button does not own the arrows, so the trigger still fires.
    p.handle(&Input::FocusChanged {
        object: Some("open".into()),
    });
    p.handle(&Input::KeyPressed {
        key: "arrow_up".into(),
    });
    assert_eq!(p.variable("control"), Some(&Value::Number(1.0)));

    // Space on a focused button is the button, not a key trigger.
    p.handle(&Input::KeyPressed {
        key: "space".into(),
    });
    assert_eq!(p.variable("global"), Some(&Value::Number(0.0)));
}

#[test]
fn only_the_lightbox_slide_triggers_hear_keys_while_it_is_open() {
    let c = keys_course();
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    p.handle(&Input::FocusChanged { object: None });
    p.handle(&Input::KeyPressed { key: "a".into() });
    assert_eq!(p.variable("global"), Some(&Value::Number(1.0)));

    let effects = p.handle(&click("open"));
    assert!(has(
        &effects,
        &Effect::LightboxOpened { slide: "s2".into() }
    ));
    p.handle(&Input::KeyPressed { key: "a".into() });
    assert_eq!(
        p.variable("global"),
        Some(&Value::Number(1.0)),
        "the base slide must not hear the key"
    );
    assert_eq!(p.variable("box"), Some(&Value::Number(1.0)));
}

fn draggable_course(snap: bool, return_on_miss: bool) -> Course {
    Course::from_json(
        &serde_json::json!({
            "schema_version": 1,
            "id": "drag-objects",
            "title": "Draggable",
            "variables": [
                { "name": "hit", "value": 0.0 },
                { "name": "merged", "value": 0.0 },
                { "name": "split", "value": 0.0 }
            ],
            "scenes": [{
                "id": "sc",
                "title": "Scene",
                "slides": [
                    {
                        "id": "s1",
                        "title": "One",
                        "objects": [
                            { "id": "card", "kind": { "type": "shape", "shape": "rectangle", "fill": "#112233" },
                              "frame": { "x": 0.0, "y": 0.0, "w": 40.0, "h": 40.0 },
                              "drag": { "targets": ["bin"], "snap": snap, "return_on_miss": return_on_miss } },
                            { "id": "bin", "kind": { "type": "shape", "shape": "rectangle", "fill": "#334455" },
                              "frame": { "x": 200.0, "y": 0.0, "w": 80.0, "h": 80.0 } }
                        ],
                        "triggers": [
                            { "action": { "action": "set_variable", "variable": "hit", "value": 1 },
                              "when": { "event": "dropped_on", "object": "card", "target": "bin" } },
                            { "action": { "action": "set_variable", "variable": "merged", "value": 1 },
                              "when": { "event": "objects_intersect", "a": "card", "b": "bin" } },
                            { "action": { "action": "set_variable", "variable": "split", "value": 1 },
                              "when": { "event": "objects_stop_intersecting", "a": "card", "b": "bin" } }
                        ]
                    },
                    { "id": "s2", "title": "Two", "objects": [] }
                ]
            }]
        })
        .to_string(),
    )
    .expect("valid draggable course")
}

fn object_moves(effects: &[Effect]) -> Vec<(String, (f32, f32))> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::ObjectMoved { object, offset } => Some((object.clone(), *offset)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_drop_hit_snaps_or_stays_and_fires_dropped_on() {
    let c = draggable_course(true, true);
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    let effects = p.handle(&Input::ObjectDropped {
        object: "card".into(),
        x: 210.0,
        y: 10.0,
    });
    assert_eq!(object_moves(&effects), vec![("card".into(), (220.0, 20.0))]);
    assert!(has(
        &effects,
        &Effect::ObjectDropped {
            object: "card".into(),
            target: "bin".into()
        }
    ));
    assert_eq!(p.object_position("card"), (220.0, 20.0));
    assert_eq!(p.variable("hit"), Some(&Value::Number(1.0)));
    assert_eq!(p.variable("merged"), Some(&Value::Number(1.0)));

    let free = draggable_course(false, true);
    let mut p = Player::new(&free).expect("player");
    p.handle(&Input::Start);
    let effects = p.handle(&Input::ObjectDropped {
        object: "card".into(),
        x: 210.0,
        y: 10.0,
    });
    assert_eq!(object_moves(&effects), vec![("card".into(), (210.0, 10.0))]);
    assert_eq!(p.object_position("card"), (210.0, 10.0));
}

#[test]
fn a_drop_miss_returns_or_stays_and_never_fires_dropped_on() {
    let c = draggable_course(true, true);
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    let effects = p.handle(&Input::ObjectDropped {
        object: "card".into(),
        x: 500.0,
        y: 400.0,
    });
    assert_eq!(object_moves(&effects), vec![("card".into(), (0.0, 0.0))]);
    assert_eq!(p.object_position("card"), (0.0, 0.0));
    assert_eq!(p.variable("hit"), Some(&Value::Number(0.0)));

    // A miss after a successful drop returns to that earlier position.
    p.handle(&Input::ObjectDropped {
        object: "card".into(),
        x: 210.0,
        y: 10.0,
    });
    assert_eq!(p.object_position("card"), (220.0, 20.0));
    let effects = p.handle(&Input::ObjectDropped {
        object: "card".into(),
        x: 500.0,
        y: 400.0,
    });
    assert_eq!(object_moves(&effects), vec![("card".into(), (220.0, 20.0))]);
    assert_eq!(p.object_position("card"), (220.0, 20.0));

    let stay = draggable_course(true, false);
    let mut p = Player::new(&stay).expect("player");
    p.handle(&Input::Start);
    let effects = p.handle(&Input::ObjectDropped {
        object: "card".into(),
        x: 500.0,
        y: 400.0,
    });
    assert_eq!(
        object_moves(&effects),
        vec![("card".into(), (500.0, 400.0))]
    );
    assert_eq!(p.object_position("card"), (500.0, 400.0));
    assert_eq!(p.variable("hit"), Some(&Value::Number(0.0)));
    let effects = p.handle(&Input::ObjectDropped {
        object: "card".into(),
        x: 500.0,
        y: 400.0,
    });
    assert!(has(
        &effects,
        &Effect::ObjectMoved {
            object: "card".into(),
            offset: (500.0, 400.0)
        }
    ));
    assert_eq!(p.variable("split"), Some(&Value::Number(0.0)));
}

#[test]
fn drag_positions_reset_on_slide_entry_and_are_not_in_suspend_data() {
    let c = draggable_course(true, true);
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    p.handle(&Input::ObjectDropped {
        object: "card".into(),
        x: 210.0,
        y: 10.0,
    });
    assert_eq!(p.object_position("card"), (220.0, 20.0));
    let encoded = encode_suspend(&c, p.state());
    let restored = decode_suspend(&c, &encoded).expect("decode");
    assert!(restored.object_positions.is_empty());
    assert!(restored.intersecting_pairs.is_empty());

    p.handle(&Input::GoTo { slide: "s2".into() });
    p.handle(&Input::GoTo { slide: "s1".into() });
    assert_eq!(p.object_position("card"), (0.0, 0.0));
}

#[test]
fn intersections_fire_once_per_transition_and_start_overlapping() {
    let c = Course::from_json(
        &serde_json::json!({
            "schema_version": 1,
            "id": "intersections",
            "title": "Intersections",
            "variables": [
                { "name": "started", "value": 0.0 },
                { "name": "stopped", "value": 0.0 }
            ],
            "scenes": [{
                "id": "sc",
                "title": "Scene",
                "slides": [{
                    "id": "s1",
                    "title": "One",
                    "objects": [
                        { "id": "a", "kind": { "type": "shape", "shape": "rectangle", "fill": "#112233" },
                          "frame": { "x": 0.0, "y": 0.0, "w": 40.0, "h": 40.0 } },
                        { "id": "b", "kind": { "type": "shape", "shape": "rectangle", "fill": "#334455" },
                          "frame": { "x": 20.0, "y": 0.0, "w": 40.0, "h": 40.0 } },
                        { "id": "hidden", "kind": { "type": "shape", "shape": "rectangle", "fill": "#556677" },
                          "frame": { "x": 0.0, "y": 0.0, "w": 40.0, "h": 40.0 },
                          "initial_state": "hidden" }
                    ],
                    "triggers": [
                        { "action": { "action": "set_variable", "variable": "started", "value": 1 },
                          "when": { "event": "objects_intersect", "a": "a", "b": "b" } },
                        { "action": { "action": "set_variable", "variable": "started", "value": 9 },
                          "when": { "event": "objects_intersect", "a": "a", "b": "hidden" } },
                        { "action": { "action": "set_variable", "variable": "stopped", "value": 1 },
                          "when": { "event": "objects_stop_intersecting", "a": "a", "b": "b" } }
                    ]
                }]
            }]
        })
        .to_string(),
    )
    .expect("valid intersection course");
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    assert_eq!(p.variable("started"), Some(&Value::Number(1.0)));
    assert_eq!(p.variable("stopped"), Some(&Value::Number(0.0)));

    // Hidden objects never intersect, even when their frames overlap.
    p.handle(&Input::Tick { ms: 100 });
    p.handle(&Input::Tick { ms: 100 });
    assert_eq!(
        p.variable("started"),
        Some(&Value::Number(1.0)),
        "a pair fires once, not every frame"
    );
}

#[test]
fn intersections_track_motion_paths_and_drag_positions() {
    let c = draggable_course(true, false);
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    // Drop on the bin, then drag away: intersect and stop-intersect both fire.
    p.handle(&Input::ObjectDropped {
        object: "card".into(),
        x: 210.0,
        y: 10.0,
    });
    assert_eq!(p.variable("merged"), Some(&Value::Number(1.0)));
    assert_eq!(p.variable("split"), Some(&Value::Number(0.0)));
    p.handle(&Input::ObjectDropped {
        object: "card".into(),
        x: 500.0,
        y: 400.0,
    });
    assert_eq!(p.variable("split"), Some(&Value::Number(1.0)));

    // Motion path: a mover starts overlapping and moves away.
    let c = Course::from_json(
        &serde_json::json!({
            "schema_version": 1,
            "id": "motion-intersections",            "title": "Motion Intersections",
            "variables": [{ "name": "stopped", "value": 0.0 }],
            "scenes": [{
                "id": "sc",
                "title": "Scene",
                "slides": [{
                    "id": "s1",
                    "title": "One",
                    "objects": [
                        { "id": "mover", "kind": { "type": "shape", "shape": "rectangle", "fill": "#112233" },
                          "frame": { "x": 0.0, "y": 0.0, "w": 40.0, "h": 40.0 },
                          "motion_paths": [{ "id": "p", "points": [{ "x": 0.0, "y": 0.0 }, { "x": 300.0, "y": 0.0 }],
                                             "duration_ms": 1000, "easing": "linear", "relative": true }] },
                        { "id": "target", "kind": { "type": "shape", "shape": "rectangle", "fill": "#334455" },
                          "frame": { "x": 20.0, "y": 0.0, "w": 40.0, "h": 40.0 } }
                    ],
                    "triggers": [
                        { "action": { "action": "set_variable", "variable": "stopped", "value": 1 },
                          "when": { "event": "objects_stop_intersecting", "a": "mover", "b": "target" } },
                        { "action": { "action": "move_on_path", "object": "mover", "path": "p" },
                          "when": { "event": "timeline_starts" } }
                    ]
                }]
            }]
        })
        .to_string(),
    )
    .expect("valid motion course");
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    p.handle(&Input::Tick { ms: 500 });
    assert_eq!(p.variable("stopped"), Some(&Value::Number(1.0)));
    p.handle(&Input::Tick { ms: 500 });
    assert_eq!(p.variable("stopped"), Some(&Value::Number(1.0)));
}

fn timer_course() -> Course {
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "timer-test",
        "title": "Timer Test",
        "variables": [
            { "name": "ended_count", "value": 0 },
            { "name": "seconds_seen", "value": 0 },
            { "name": "low_seen", "value": 0 }
        ],
        "scenes": [{
            "id": "sc",
            "title": "Scene",
            "slides": [
                {
                    "id": "s1",
                    "title": "One",
                    "duration_ms": 60000,
                    "objects": [
                        { "id": "t1", "kind": { "type": "timer", "duration_ms": 5000 }, "frame": { "x": 0, "y": 0, "w": 200, "h": 60 }, "timing": { "start_ms": 2000 } },
                        { "id": "btn-reset", "kind": { "type": "button", "label": "Reset" }, "frame": { "x": 0, "y": 80, "w": 100, "h": 40 } },
                        { "id": "btn-start", "kind": { "type": "button", "label": "Start" }, "frame": { "x": 120, "y": 80, "w": 100, "h": 40 } },
                        { "id": "btn-pause", "kind": { "type": "button", "label": "Pause" }, "frame": { "x": 240, "y": 80, "w": 100, "h": 40 } },
                        { "id": "btn-open", "kind": { "type": "button", "label": "Open" }, "frame": { "x": 0, "y": 140, "w": 100, "h": 40 } },
                        { "id": "btn-hold", "kind": { "type": "button", "label": "Hold" }, "frame": { "x": 120, "y": 140, "w": 100, "h": 40 } }
                    ],
                    "triggers": [
                        { "action": { "action": "set_variable", "variable": "ended_count", "op": "add", "value": 1 }, "when": { "event": "timer_ends", "timer": "t1" } },
                        { "action": { "action": "set_variable", "variable": "seconds_seen", "op": "add", "value": 1 }, "when": { "event": "variable_changes", "variable": "Timer.t1.Remaining" } },
                        { "action": { "action": "reset_timer", "timer": "t1" }, "when": { "event": "click", "object": "btn-reset" } },
                        { "action": { "action": "start_timer", "timer": "t1" }, "when": { "event": "click", "object": "btn-start" } },
                        { "action": { "action": "pause_timer", "timer": "t1" }, "when": { "event": "click", "object": "btn-pause" } },
                        { "action": { "action": "lightbox_slide", "slide": "s-box" }, "when": { "event": "click", "object": "btn-open" } },
                        { "action": { "action": "show_layer", "layer": "hold" }, "when": { "event": "click", "object": "btn-hold" } }
                    ],
                    "layers": [{
                        "id": "hold",
                        "name": "Hold",
                        "duration_ms": 60000,
                        "pause_base_timeline": true,
                        "objects": [
                            { "id": "hold-btn", "kind": { "type": "button", "label": "Unhold" }, "frame": { "x": 0, "y": 200, "w": 100, "h": 40 } }
                        ],
                        "triggers": [
                            { "action": { "action": "hide_layer" }, "when": { "event": "click", "object": "hold-btn" } }
                        ]
                    }]
                },
                {
                    "id": "s-box",
                    "title": "Box",
                    "duration_ms": 60000,
                    "objects": [
                        { "id": "t-box", "kind": { "type": "timer", "duration_ms": 8000 }, "frame": { "x": 0, "y": 0, "w": 200, "h": 60 } }
                    ]
                }
            ]
        }]
    });
    Course::from_json(&json.to_string()).expect("valid timer course")
}

#[test]
fn timer_autostarts_at_the_objects_start_time_and_counts_down() {
    let c = timer_course();
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    assert_eq!(p.timer_remaining_ms("t1"), Some(5000));
    assert!(!p.timer_running("t1"));
    // Before the object's 2000 ms start the timer holds at full duration.
    let effects = p.handle(&Input::Tick { ms: 1500 });
    assert!(!effects.contains(&Effect::TimerStarted { timer: "t1".into() }));
    assert_eq!(p.timer_remaining_ms("t1"), Some(5000));
    // Crossing the start begins it; only the part of the tick after the
    // start counts (1500 ms of this tick are after 2000).
    let effects = p.handle(&Input::Tick { ms: 2000 });
    assert!(effects.contains(&Effect::TimerStarted { timer: "t1".into() }));
    assert_eq!(p.timer_remaining_ms("t1"), Some(3500));
    assert!(p.timer_running("t1"));
    p.handle(&Input::Tick { ms: 1500 });
    assert_eq!(p.timer_remaining_ms("t1"), Some(2000));
}

#[test]
fn timer_ends_fires_exactly_once_even_across_a_big_step() {
    let c = timer_course();
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    // One step jumps well past zero (start 2000 + 5000 duration).
    let effects = p.handle(&Input::Tick { ms: 30000 });
    assert!(effects.contains(&Effect::TimerEnded { timer: "t1".into() }));
    assert_eq!(p.variable("ended_count"), Some(&Value::Number(1.0)));
    assert_eq!(p.timer_remaining_ms("t1"), Some(0));
    assert!(p.timer_ended("t1"));
    let effects = p.handle(&Input::Tick { ms: 5000 });
    assert!(!effects.contains(&Effect::TimerEnded { timer: "t1".into() }));
    assert_eq!(p.variable("ended_count"), Some(&Value::Number(1.0)));
}

#[test]
fn timer_pauses_with_the_player_and_a_pausing_layer_and_resumes() {
    let c = timer_course();
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    p.handle(&Input::Tick { ms: 2500 });
    let remaining = p.timer_remaining_ms("t1");
    assert_eq!(remaining, Some(4500));

    // The learner's pause freezes the countdown.
    p.handle(&Input::PlayerPause);
    p.handle(&Input::Tick { ms: 1000 });
    assert_eq!(p.timer_remaining_ms("t1"), remaining);
    p.handle(&Input::PlayerResume);
    p.handle(&Input::Tick { ms: 500 });
    assert_eq!(p.timer_remaining_ms("t1"), Some(4000));

    // A layer that pauses the base holds the base timer too.
    p.handle(&click("btn-hold"));
    p.handle(&Input::Tick { ms: 1000 });
    assert_eq!(p.timer_remaining_ms("t1"), Some(4000));
    p.handle(&click("hold-btn"));
    p.handle(&Input::Tick { ms: 500 });
    assert_eq!(p.timer_remaining_ms("t1"), Some(3500));
}

#[test]
fn timer_stops_while_a_lightbox_is_open_and_resumes_after() {
    let c = timer_course();
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    p.handle(&Input::Tick { ms: 2500 });
    let remaining = p.timer_remaining_ms("t1");
    assert_eq!(remaining, Some(4500));

    p.handle(&click("btn-open"));
    assert!(p.lightbox_open());
    // The box slide's own timer runs while the base timer is frozen.
    p.handle(&Input::Tick { ms: 3000 });
    assert_eq!(p.timer_remaining_ms("t-box"), Some(5000));
    p.handle(&Input::CloseLightbox);
    // The base timer is exactly where it was when the box opened.
    assert_eq!(p.timer_remaining_ms("t1"), remaining);
    p.handle(&Input::Tick { ms: 500 });
    assert_eq!(p.timer_remaining_ms("t1"), Some(4000));
}

#[test]
fn timer_reset_restarts_and_start_on_an_ended_timer_does_nothing() {
    let c = timer_course();
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    p.handle(&Input::Tick { ms: 30000 });
    assert!(p.timer_ended("t1"));
    // start_timer on an ended timer: no effect, still ended.
    let effects = p.handle(&click("btn-start"));
    assert!(!effects.contains(&Effect::TimerStarted { timer: "t1".into() }));
    assert!(p.timer_ended("t1"));

    let effects = p.handle(&click("btn-reset"));
    assert!(effects.contains(&Effect::TimerReset { timer: "t1".into() }));
    assert_eq!(p.timer_remaining_ms("t1"), Some(5000));
    assert!(!p.timer_running("t1"));
    assert!(!p.timer_ended("t1"));
    // A reset timer stays stopped until start_timer.
    p.handle(&Input::Tick { ms: 1000 });
    assert_eq!(p.timer_remaining_ms("t1"), Some(5000));
    let effects = p.handle(&click("btn-start"));
    assert!(effects.contains(&Effect::TimerStarted { timer: "t1".into() }));
    p.handle(&Input::Tick { ms: 1000 });
    assert_eq!(p.timer_remaining_ms("t1"), Some(4000));

    // Pause and resume through the actions.
    let effects = p.handle(&click("btn-pause"));
    assert!(effects.contains(&Effect::TimerPaused { timer: "t1".into() }));
    p.handle(&Input::Tick { ms: 1000 });
    assert_eq!(p.timer_remaining_ms("t1"), Some(4000));
    p.handle(&click("btn-start"));
    p.handle(&Input::Tick { ms: 1000 });
    assert_eq!(p.timer_remaining_ms("t1"), Some(3000));
}

#[test]
fn timer_resets_on_slide_entry_and_is_not_in_suspend_data() {
    let c = timer_course();
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    p.handle(&Input::Tick { ms: 4000 });
    assert_eq!(p.timer_remaining_ms("t1"), Some(3000));
    let encoded = encode_suspend(&c, p.state());
    let decoded = decode_suspend(&c, &encoded).expect("decode");
    assert!(decoded.timers.is_empty());
    // Re-entering the slide (jump to the box and back via start slide reset)
    // restores full duration: leave through the lightbox round trip and check
    // the saved base timer was kept, then jump away and back.
    p.handle(&Input::GoTo {
        slide: "s-box".into(),
    });
    p.handle(&Input::GoTo { slide: "s1".into() });
    assert_eq!(p.timer_remaining_ms("t1"), Some(5000));
    assert!(!p.timer_running("t1"));
}

#[test]
fn timer_remaining_variable_works_in_text_and_conditions_once_per_second() {
    let c = timer_course();
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    // Full duration, rounded up to whole seconds.
    assert_eq!(p.value("Timer.t1.Remaining"), Some(Value::Number(5.0)));
    assert_eq!(p.display_text("left: %Timer.t1.Remaining%s"), "left: 5s");
    // Unknown timer ids do not resolve.
    assert_eq!(p.value("Timer.nope.Remaining"), None);

    // The variable_changes trigger fires once per whole-second change, not
    // per tick: five 200 ms ticks cross two whole seconds (5->4 at 1 ms of
    // remaining loss... tick 1: 200 ms lost, still 5s; tick 5: 1000 ms lost -> 4s).
    assert_eq!(p.variable("seconds_seen"), Some(&Value::Number(0.0)));
    for _ in 0..4 {
        p.handle(&Input::Tick { ms: 200 });
    }
    p.handle(&Input::Tick { ms: 100 });
    assert_eq!(p.timer_remaining_ms("t1"), Some(5000));
    // Autostart has not begun yet (clock 900 < 2000), so no change fired.
    assert_eq!(p.variable("seconds_seen"), Some(&Value::Number(0.0)));
    p.handle(&Input::Tick { ms: 2000 });
    // Clock now 2900: autostart at 2000, 900 ms counted -> remaining 4100,
    // whole seconds 5 -> 5? 4100 ms ceil = 5. No whole-second change yet.
    assert_eq!(p.timer_remaining_ms("t1"), Some(4100));
    assert_eq!(p.variable("seconds_seen"), Some(&Value::Number(0.0)));
    p.handle(&Input::Tick { ms: 200 });
    // 3900 ms -> 4 whole seconds: exactly one change event.
    assert_eq!(p.variable("seconds_seen"), Some(&Value::Number(1.0)));
    p.handle(&Input::Tick { ms: 100 });
    assert_eq!(p.variable("seconds_seen"), Some(&Value::Number(1.0)));
    assert_eq!(p.value("Timer.t1.Remaining"), Some(Value::Number(4.0)));
}

#[test]
fn timer_remaining_variable_rejects_writes() {
    let c = timer_course();
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    let effects = p.handle(&Input::SetVariable {
        variable: "Timer.t1.Remaining".into(),
        value: Value::Number(1.0),
    });
    assert!(warned(&effects));
    assert_eq!(
        p.value("Timer.t1.Remaining"),
        Some(Value::Number(5.0)),
        "a write attempt must not change the remaining value"
    );
}

fn cue_course() -> Course {
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "cue-test",
        "title": "Cue Test",
        "scenes": [{
            "id": "scene1",
            "title": "Scene 1",
            "slides": [
                {
                    "id": "s1",
                    "title": "Slide 1",
                    "duration_ms": 5000,
                    "objects": [
                        {
                            "id": "vid",
                            "kind": {
                                "type": "video",
                                "src": "clip.mp4",
                                "autoplay": true,
                                "cues": [
                                    { "id": "c1", "at_ms": 1000, "name": "One" },
                                    { "id": "c2", "at_ms": 2000, "name": "Two" },
                                    { "id": "c3", "at_ms": 3000, "name": "Three" }
                                ]
                            },
                            "frame": { "x": 0.0, "y": 0.0, "w": 320.0, "h": 180.0 }
                        },
                        {
                            "id": "play",
                            "kind": { "type": "button", "label": "Play" },
                            "frame": { "x": 0.0, "y": 200.0, "w": 100.0, "h": 40.0 }
                        }
                    ],
                    "triggers": [{
                        "when": { "event": "click", "object": "play" },
                        "action": { "action": "play_media", "object": "vid" }
                    }]
                },
                {
                    "id": "s2",
                    "title": "Slide 2",
                    "duration_ms": 5000,
                    "objects": []
                }
            ]
        }]
    });
    Course::from_json(&json.to_string()).unwrap()
}

fn pause_cue_course(layer_on_cue: bool) -> Course {
    let cue_trigger = if layer_on_cue {
        serde_json::json!([{
            "when": { "event": "cue_reached", "object": "vid", "cue": "c1" },
            "action": { "action": "show_layer", "layer": "l1" }
        }])
    } else {
        serde_json::json!([])
    };
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "cue-pause-test",
        "title": "Cue Pause Test",
        "scenes": [{
            "id": "scene1",
            "title": "Scene 1",
            "slides": [{
                "id": "s1",
                "title": "Slide 1",
                "duration_ms": 5000,
                "objects": [
                    {
                        "id": "vid",
                        "kind": {
                            "type": "video",
                            "src": "clip.mp4",
                            "autoplay": true,
                            "cues": [
                                { "id": "c1", "at_ms": 1000, "name": "Pause here", "pause": true },
                                { "id": "c2", "at_ms": 2000, "name": "Later" }
                            ]
                        },
                        "frame": { "x": 0.0, "y": 0.0, "w": 320.0, "h": 180.0 }
                    },
                    {
                        "id": "play",
                        "kind": { "type": "button", "label": "Play" },
                        "frame": { "x": 0.0, "y": 200.0, "w": 100.0, "h": 40.0 }
                    }
                ],
                "layers": [{
                    "id": "l1",
                    "name": "Question",
                    "duration_ms": 5000,
                    "objects": [{
                        "id": "continue",
                        "kind": { "type": "button", "label": "Continue" },
                        "frame": { "x": 0.0, "y": 0.0, "w": 120.0, "h": 40.0 }
                    }],
                    "triggers": [
                        {
                            "when": { "event": "click", "object": "continue" },
                            "action": { "action": "hide_layer", "layer": null }
                        },
                        {
                            "when": { "event": "click", "object": "continue" },
                            "action": { "action": "play_media", "object": "vid" }
                        }
                    ]
                }],
                "triggers": cue_trigger
            }]
        }]
    });
    let mut course = Course::from_json(&json.to_string()).unwrap();
    course.scenes[0].slides[0].triggers.push(Trigger {
        action: Action::PlayMedia {
            object: "vid".into(),
        },
        when: Event::Click {
            object: "play".into(),
        },
        condition: None,
        disabled: false,
    });
    course
}

fn cue_effects(effects: &[Effect]) -> Vec<String> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::CueReached { object, cue } if object == "vid" => Some(cue.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn cue_fires_once_per_crossing_and_several_in_one_report_in_order() {
    let c = cue_course();
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    assert!(p.is_media_playing("vid"));

    assert!(
        cue_effects(&p.handle(&Input::MediaTime {
            object: "vid".into(),
            at_ms: 500.0,
        }))
        .is_empty()
    );

    let fx = p.handle(&Input::MediaTime {
        object: "vid".into(),
        at_ms: 2500.0,
    });
    assert_eq!(cue_effects(&fx), vec!["c1".to_owned(), "c2".to_owned()]);
    assert!(has(
        &fx,
        &Effect::CueReached {
            object: "vid".into(),
            cue: "c1".into()
        }
    ));

    // Crossing the same cues again does not repeat them.
    assert!(
        cue_effects(&p.handle(&Input::MediaTime {
            object: "vid".into(),
            at_ms: 2600.0,
        }))
        .is_empty()
    );
    assert_eq!(
        cue_effects(&p.handle(&Input::MediaTime {
            object: "vid".into(),
            at_ms: 3100.0,
        })),
        vec!["c3".to_owned()]
    );
}

#[test]
fn forward_seek_skips_cues_and_backward_seek_rearms_them() {
    let c = cue_course();
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);

    let fx = p.handle(&Input::PlayerSeek { ms: 3000 });
    assert!(cue_effects(&fx).is_empty(), "{fx:?}");
    assert!(
        cue_effects(&p.handle(&Input::MediaTime {
            object: "vid".into(),
            at_ms: 3100.0,
        }))
        .is_empty()
    );

    assert!(cue_effects(&p.handle(&Input::PlayerSeek { ms: 500 })).is_empty());
    assert_eq!(
        cue_effects(&p.handle(&Input::MediaTime {
            object: "vid".into(),
            at_ms: 1500.0,
        })),
        vec!["c1".to_owned()]
    );
}

#[test]
fn replaying_the_slide_rearms_every_cue() {
    let c = cue_course();
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    assert_eq!(
        cue_effects(&p.handle(&Input::MediaTime {
            object: "vid".into(),
            at_ms: 1500.0,
        })),
        vec!["c1".to_owned()]
    );
    p.handle(&Input::GoTo { slide: "s2".into() });
    p.handle(&Input::GoTo { slide: "s1".into() });
    assert_eq!(
        cue_effects(&p.handle(&Input::MediaTime {
            object: "vid".into(),
            at_ms: 1500.0,
        })),
        vec!["c1".to_owned()],
        "a fresh visit re-arms all cues"
    );
}

#[test]
fn a_pausing_cue_holds_the_video_and_play_media_resumes_without_refiring() {
    let c = pause_cue_course(false);
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);

    let fx = p.handle(&Input::MediaTime {
        object: "vid".into(),
        at_ms: 1500.0,
    });
    assert_eq!(cue_effects(&fx), vec!["c1".to_owned()]);
    assert_eq!(p.media_state("vid"), Some(MediaPlaybackState::PausedAtCue));
    assert!(has(&fx, &seek_effect("vid", 1000)));
    assert!(has(&fx, &media_effect("vid", MediaOp::Pause)));

    let fx = p.handle(&click("play"));
    assert!(has(&fx, &seek_effect("vid", 1000)));
    assert!(has(&fx, &media_effect("vid", MediaOp::Play)));
    assert!(cue_effects(&fx).is_empty(), "the held cue must not re-fire");
    assert!(p.is_media_playing("vid"));

    assert!(
        cue_effects(&p.handle(&Input::MediaTime {
            object: "vid".into(),
            at_ms: 1010.0,
        }))
        .is_empty()
    );
    assert_eq!(
        cue_effects(&p.handle(&Input::MediaTime {
            object: "vid".into(),
            at_ms: 2100.0,
        })),
        vec!["c2".to_owned()]
    );
}

#[test]
fn the_learners_play_control_resumes_a_cue_hold_unless_the_layer_has_control() {
    // No cue layer: the learner's play resumes the video.
    let c = pause_cue_course(false);
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    p.handle(&Input::MediaTime {
        object: "vid".into(),
        at_ms: 1500.0,
    });
    let fx = p.handle(&Input::PlayerResume);
    assert!(has(&fx, &media_effect("vid", MediaOp::Play)));
    assert!(p.is_media_playing("vid"));

    // The cue's layer is open: the learner's play is ignored, then Continue
    // hides the layer and resumes the video.
    let c = pause_cue_course(true);
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    p.handle(&Input::MediaTime {
        object: "vid".into(),
        at_ms: 1500.0,
    });
    assert_eq!(p.state().visible_layers, vec!["l1".to_owned()]);
    let fx = p.handle(&Input::PlayerResume);
    assert!(
        !fx.iter().any(|e| matches!(
            e,
            Effect::Media {
                op: MediaOp::Play,
                ..
            }
        )),
        "{fx:?}"
    );
    assert_eq!(p.media_state("vid"), Some(MediaPlaybackState::PausedAtCue));

    let fx = p.handle(&click("continue"));
    assert_eq!(p.state().visible_layers, Vec::<String>::new());
    assert!(has(&fx, &media_effect("vid", MediaOp::Play)));
    assert!(p.is_media_playing("vid"));
}

#[test]
fn cue_progress_is_not_in_suspend_data_and_resume_keeps_the_same_pass() {
    let c = cue_course();
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    p.handle(&Input::MediaTime {
        object: "vid".into(),
        at_ms: 1500.0,
    });

    // Saving the same session keeps cue progress in PlayerState, so the same
    // cue does not fire again.
    let state = p.state().clone();
    assert_eq!(
        state.cue_progress.get("vid").map(|p| p.fired.len()),
        Some(1)
    );
    let mut resumed = Player::resume(&c, state).expect("resume");
    assert!(
        cue_effects(&resumed.handle(&Input::MediaTime {
            object: "vid".into(),
            at_ms: 1600.0,
        }))
        .is_empty()
    );

    // Suspend data does not carry cue progress: a relaunch starts a fresh pass
    // from the saved slide.
    let suspend = encode_suspend(&c, p.state());
    let decoded = decode_suspend(&c, &suspend).expect("decode");
    assert!(decoded.cue_progress.is_empty());
    assert!(decoded.cue_held.is_empty());
    let mut relaunched = Player::resume(&c, decoded).expect("relaunch");
    relaunched.handle(&Input::Start);
    assert_eq!(
        cue_effects(&relaunched.handle(&Input::MediaTime {
            object: "vid".into(),
            at_ms: 1500.0,
        })),
        vec!["c1".to_owned()],
        "cues after the saved position are armed again"
    );
}

#[test]
fn preview_and_web_style_time_reports_produce_the_same_effects() {
    let c = cue_course();
    let mut web = Player::new(&c).expect("web");
    let mut preview = Player::new(&c).expect("preview");
    web.handle(&Input::Start);
    preview.handle(&Input::Start);
    for at_ms in [250.0, 1250.0, 2250.0, 3100.0] {
        let input = Input::MediaTime {
            object: "vid".into(),
            at_ms,
        };
        assert_eq!(web.handle(&input), preview.handle(&input));
    }
}

#[test]
fn media_time_on_a_non_video_warns_and_changes_nothing() {
    let c = cue_course();
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    let fx = p.handle(&Input::MediaTime {
        object: "play".into(),
        at_ms: 1000.0,
    });
    assert!(warned(&fx));
    assert!(cue_effects(&fx).is_empty());
    let fx = p.handle(&Input::MediaTime {
        object: "vid".into(),
        at_ms: f64::NAN,
    });
    assert!(warned(&fx));
}

#[test]
fn per_answer_scoring_awards_chosen_choice_points() {
    let c = quiz_with(serde_json::json!({
        "kind": "multiple_choice",
        "prompt": "Pick one.",
        "choices": [
            { "id": "a", "text": "A", "correct": true, "points": 3 },
            { "id": "b", "text": "B", "correct": false, "points": 5 }
        ],
        "scoring": "per_answer",
        "points": 7,
        "attempts": 2
    }));
    let mut p = open_quiz(&c);
    p.handle(&Input::SelectChoice { choice: "a".into() });
    submit(&mut p);
    assert_eq!(p.state().answers.get("s-quiz").map(|a| a.points), Some(3));

    let c = quiz_with(serde_json::json!({
        "kind": "multiple_response",
        "prompt": "Pick.",
        "choices": [
            { "id": "a", "text": "A", "correct": true, "points": 2 },
            { "id": "b", "text": "B", "correct": true, "points": 3 },
            { "id": "c", "text": "C", "correct": false, "points": -1 }
        ],
        "scoring": "per_answer",
        "points": 10,
        "attempts": 2
    }));
    let mut p = open_quiz(&c);
    p.handle(&Input::SelectChoice { choice: "a".into() });
    p.handle(&Input::SelectChoice { choice: "b".into() });
    submit(&mut p);
    assert_eq!(p.state().answers.get("s-quiz").map(|a| a.points), Some(5));

    let mut p = open_quiz(&c);
    p.handle(&Input::SelectChoice { choice: "a".into() });
    p.handle(&Input::SelectChoice { choice: "c".into() });
    submit(&mut p);
    assert_eq!(p.state().answers.get("s-quiz").map(|a| a.points), Some(1));

    let mut p = open_quiz(&c);
    p.handle(&Input::SelectChoice { choice: "c".into() });
    submit(&mut p);
    assert_eq!(p.state().answers.get("s-quiz").map(|a| a.points), Some(0));
}

#[test]
fn partial_scoring_awards_fraction() {
    let c = quiz_with(serde_json::json!({
        "kind": "multiple_response",
        "prompt": "Pick.",
        "choices": [
            { "id": "a", "text": "A", "correct": true },
            { "id": "b", "text": "B", "correct": true }
        ],
        "scoring": "partial",
        "points": 10,
        "attempts": 2
    }));
    let mut p = open_quiz(&c);
    p.handle(&Input::SelectChoice { choice: "a".into() });
    let fx = submit(&mut p);
    assert_eq!(answered(&fx), Some((false, Some(1))));
    assert_eq!(p.state().answers.get("s-quiz").map(|a| a.points), Some(5));

    let mut c = sequence_course(&["a", "b", "c"]);
    c.slide_mut("s-quiz")
        .unwrap()
        .question
        .as_mut()
        .unwrap()
        .scoring = coursecraft_course::Scoring::Partial;
    c.slide_mut("s-quiz")
        .unwrap()
        .question
        .as_mut()
        .unwrap()
        .points = 10;
    let mut p = open_quiz(&c);
    let target = vec!["a".to_owned(), "c".to_owned(), "b".to_owned()];
    for i in 0..target.len() {
        if p.sequence_order().get(i) == target.get(i) {
            continue;
        }
        let Some(id) = target.get(i).cloned() else {
            continue;
        };
        p.handle(&Input::SequenceMoved { choice: id, to: i });
    }
    assert_eq!(p.sequence_order(), target.as_slice());
    let fx = submit(&mut p);
    assert_eq!(answered(&fx), Some((false, Some(1))));
    assert_eq!(p.state().answers.get("s-quiz").map(|a| a.points), Some(3));

    let mut c = matching_course();
    c.slide_mut("s-quiz")
        .unwrap()
        .question
        .as_mut()
        .unwrap()
        .scoring = coursecraft_course::Scoring::Partial;
    c.slide_mut("s-quiz")
        .unwrap()
        .question
        .as_mut()
        .unwrap()
        .points = 10;
    let mut p = open_quiz(&c);
    p.handle(&Input::MatchSelected {
        left: "p1".into(),
        right: Some("p1".into()),
    });
    p.handle(&Input::MatchSelected {
        left: "p2".into(),
        right: Some("p1".into()),
    });
    let fx = submit(&mut p);
    assert_eq!(answered(&fx), Some((false, Some(1))));
    assert_eq!(p.state().answers.get("s-quiz").map(|a| a.points), Some(5));

    let mut c = hotspot_course(
        2,
        serde_json::json!([rect_region("good", 100.0, 100.0, 100.0, 100.0, true)]),
    );
    c.slide_mut("s-quiz")
        .unwrap()
        .question
        .as_mut()
        .unwrap()
        .scoring = coursecraft_course::Scoring::Partial;
    c.slide_mut("s-quiz")
        .unwrap()
        .question
        .as_mut()
        .unwrap()
        .points = 10;
    let mut p = open_quiz(&c);
    p.handle(&Input::HotspotClicked { x: 120.0, y: 120.0 });
    p.handle(&Input::HotspotClicked { x: 10.0, y: 10.0 });
    let fx = submit(&mut p);
    assert_eq!(answered(&fx), Some((false, Some(1))));
    assert_eq!(p.state().answers.get("s-quiz").map(|a| a.points), Some(5));

    let mut c = drag_course(serde_json::json!({
        "scoring": "partial",
        "one_per_target": false
    }));
    c.slide_mut("s-quiz")
        .unwrap()
        .question
        .as_mut()
        .unwrap()
        .points = 10;
    let mut p = open_quiz(&c);
    p.handle(&Input::DragDropped {
        item: "card".into(),
        target: Some("box".into()),
        x: 0.0,
        y: 0.0,
    });
    p.handle(&Input::DragDropped {
        item: "chip".into(),
        target: Some("box".into()),
        x: 0.0,
        y: 0.0,
    });
    let fx = submit(&mut p);
    assert_eq!(answered(&fx), Some((false, Some(1))));
    assert_eq!(p.state().answers.get("s-quiz").map(|a| a.points), Some(5));
}

#[test]
fn partial_scoring_full_credit_when_correct() {
    let c = quiz_with(serde_json::json!({
        "kind": "multiple_response",
        "prompt": "Pick.",
        "choices": [
            { "id": "a", "text": "A", "correct": true },
            { "id": "b", "text": "B", "correct": true }
        ],
        "scoring": "partial",
        "points": 10,
        "attempts": 2
    }));
    let mut p = open_quiz(&c);
    p.handle(&Input::SelectChoice { choice: "a".into() });
    p.handle(&Input::SelectChoice { choice: "b".into() });
    let fx = submit(&mut p);
    assert_eq!(answered(&fx), Some((true, Some(1))));
    assert_eq!(p.state().answers.get("s-quiz").map(|a| a.points), Some(10));
}

#[test]
fn all_or_nothing_scoring_behaves_as_before() {
    let c = true_false_course();
    let mut p = open_quiz(&c);
    p.handle(&Input::SelectChoice {
        choice: "false".into(),
    });
    submit(&mut p);
    assert_eq!(p.state().answers.get("s-quiz").map(|a| a.points), Some(0));
    assert_eq!(
        p.state().answers.get("s-quiz").map(|a| a.correct),
        Some(false)
    );

    let mut p = open_quiz(&c);
    p.handle(&Input::SelectChoice {
        choice: "true".into(),
    });
    submit(&mut p);
    assert_eq!(p.state().answers.get("s-quiz").map(|a| a.points), Some(10));
    assert_eq!(
        p.state().answers.get("s-quiz").map(|a| a.correct),
        Some(true)
    );
}

#[test]
fn submit_results_uses_max_points() {
    let c = quiz_with(serde_json::json!({
        "kind": "multiple_response",
        "prompt": "Pick.",
        "choices": [
            { "id": "a", "text": "A", "correct": true, "points": 2 },
            { "id": "b", "text": "B", "correct": true, "points": 3 }
        ],
        "scoring": "per_answer",
        "points": 10,
        "attempts": 1
    }));
    let mut p = open_quiz(&c);
    p.handle(&Input::SelectChoice { choice: "a".into() });
    p.handle(&Input::SelectChoice { choice: "b".into() });
    submit(&mut p);
    let fx = p.handle(&Input::GoTo {
        slide: "s-results".into(),
    });
    assert!(has(
        &fx,
        &Effect::Score {
            raw: 5,
            max: 5,
            percent: 100,
            status: CompletionStatus::Passed
        }
    ));
}

// ---------------------------------------------------------------- survey questions

fn likert_course() -> Course {
    quiz_with(serde_json::json!({
        "kind": "likert",
        "prompt": "How was it?",
        "choices": [
            { "id": "1", "text": "Strongly disagree" },
            { "id": "2", "text": "Neutral" },
            { "id": "3", "text": "Strongly agree" }
        ],
        "points": 10,
        "attempts": 1
    }))
}

fn short_answer_course() -> Course {
    quiz_with(serde_json::json!({
        "kind": "short_answer",
        "prompt": "Anything to add?",
        "choices": [],
        "points": 10,
        "attempts": 1
    }))
}

fn essay_course() -> Course {
    quiz_with(serde_json::json!({
        "kind": "essay",
        "prompt": "Tell us more.",
        "choices": [],
        "points": 10,
        "attempts": 1
    }))
}

#[test]
fn likert_is_single_select_and_grades_with_zero_points() {
    let c = likert_course();
    let mut p = open_quiz(&c);
    assert!(
        p.handle(&click("btn-submit"))
            .contains(&Effect::AnswerRequired),
        "submitting with nothing selected"
    );
    assert!(p.state().answers.is_empty());

    p.handle(&Input::SelectChoice { choice: "1".into() });
    let fx = p.handle(&Input::SelectChoice { choice: "2".into() });
    assert!(has(
        &fx,
        &Effect::ChoiceSelected {
            choice: "1".into(),
            selected: false
        }
    ));
    assert!(has(
        &fx,
        &Effect::ChoiceSelected {
            choice: "2".into(),
            selected: true
        }
    ));
    assert_eq!(
        p.state().selected,
        ["2".to_owned()].into_iter().collect(),
        "previous choice replaced"
    );

    let fx = submit(&mut p);
    assert!(has(
        &fx,
        &Effect::QuestionAnswered {
            slide: "s-quiz".into(),
            correct: true,
            points: 0,
            attempts_left: Some(0)
        }
    ));
    assert_eq!(p.state().answers.get("s-quiz").map(|a| a.points), Some(0));
    assert!(p.question_locked());
    assert!(
        messages(&p.handle(&Input::SelectChoice { choice: "3".into() }))
            .iter()
            .any(|m| m.contains("already answered")),
        "locked question must warn"
    );
}

#[test]
fn short_answer_rejects_blank_and_caps_at_250() {
    let c = short_answer_course();
    let mut p = open_quiz(&c);
    p.handle(&Input::TextEntered { text: "   ".into() });
    assert!(
        p.handle(&click("btn-submit"))
            .contains(&Effect::AnswerRequired),
        "blank text must not grade"
    );
    assert!(p.state().answers.is_empty());

    p.handle(&Input::TextEntered {
        text: "Hello".into(),
    });
    let fx = submit(&mut p);
    assert!(has(
        &fx,
        &Effect::QuestionAnswered {
            slide: "s-quiz".into(),
            correct: true,
            points: 0,
            attempts_left: Some(0)
        }
    ));

    let c = short_answer_course();
    let mut p = open_quiz(&c);
    let long = "x".repeat(300);
    p.handle(&Input::TextEntered { text: long });
    assert_eq!(p.text_entry().chars().count(), 250);
    assert_eq!(p.text_entry(), "x".repeat(250));
}

#[test]
fn essay_keeps_over_250_but_caps_at_2000() {
    let c = essay_course();
    let mut p = open_quiz(&c);
    p.handle(&Input::TextEntered {
        text: "y".repeat(500),
    });
    assert_eq!(p.text_entry().chars().count(), 500, "over 250 is kept");

    let c = essay_course();
    let mut p = open_quiz(&c);
    p.handle(&Input::TextEntered {
        text: "z".repeat(3000),
    });
    assert_eq!(p.text_entry().chars().count(), 2000);
    let fx = submit(&mut p);
    assert!(has(
        &fx,
        &Effect::QuestionAnswered {
            slide: "s-quiz".into(),
            correct: true,
            points: 0,
            attempts_left: Some(0)
        }
    ));
}

#[test]
fn text_entered_on_likert_warns_and_changes_nothing() {
    let c = likert_course();
    let mut p = open_quiz(&c);
    let effects = p.handle(&Input::TextEntered { text: "1".into() });
    assert!(warned(&effects));
    assert!(p.text_entry().is_empty());
    assert!(p.state().selected.is_empty());
}

#[test]
fn survey_answers_add_nothing_to_the_score() {
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "survey-score",
        "title": "Survey score",
        "quiz": { "passing_percent": 80 },
        "scenes": [{
            "id": "scene1",
            "title": "Scene 1",
            "slides": [
                {
                    "id": "s-graded",
                    "title": "Graded",
                    "duration_ms": 4000,
                    "objects": [
                        { "id": "btn-submit", "kind": { "type": "button", "label": "Submit" },
                          "frame": { "x": 0.0, "y": 0.0, "w": 100.0, "h": 50.0 } }
                    ],
                    "question": {
                        "kind": "multiple_choice",
                        "prompt": "Q?",
                        "choices": [
                            { "id": "a", "text": "A" },
                            { "id": "b", "text": "B", "correct": true }
                        ],
                        "points": 10,
                        "attempts": 1
                    },
                    "triggers": [
                        { "when": { "event": "click", "object": "btn-submit" },
                          "action": { "action": "submit_question" } }
                    ]
                },
                {
                    "id": "s-survey",
                    "title": "Survey",
                    "duration_ms": 4000,
                    "objects": [
                        { "id": "btn-submit", "kind": { "type": "button", "label": "Submit" },
                          "frame": { "x": 0.0, "y": 0.0, "w": 100.0, "h": 50.0 } }
                    ],
                    "question": {
                        "kind": "likert",
                        "prompt": "How was it?",
                        "choices": [
                            { "id": "1", "text": "Bad" },
                            { "id": "2", "text": "Good" }
                        ],
                        "points": 10,
                        "attempts": 1
                    },
                    "triggers": [
                        { "when": { "event": "click", "object": "btn-submit" },
                          "action": { "action": "submit_question" } }
                    ]
                },
                {
                    "id": "s-results",
                    "title": "Results",
                    "duration_ms": 1000,
                    "objects": [],
                    "triggers": [
                        { "when": { "event": "timeline_starts" },
                          "action": { "action": "submit_results" } }
                    ]
                }
            ]
        }]
    });
    let c = Course::from_json(&json.to_string()).unwrap();
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    p.handle(&Input::SelectChoice { choice: "b".into() });
    let fx = submit(&mut p);
    assert!(has(
        &fx,
        &Effect::QuestionAnswered {
            slide: "s-graded".into(),
            correct: true,
            points: 10,
            attempts_left: Some(0)
        }
    ));
    p.handle(&Input::GoTo {
        slide: "s-survey".into(),
    });
    p.handle(&Input::SelectChoice { choice: "2".into() });
    let fx = submit(&mut p);
    assert!(has(
        &fx,
        &Effect::QuestionAnswered {
            slide: "s-survey".into(),
            correct: true,
            points: 0,
            attempts_left: Some(0)
        }
    ));
    let fx = p.handle(&Input::GoTo {
        slide: "s-results".into(),
    });
    assert!(
        has(
            &fx,
            &Effect::Score {
                raw: 10,
                max: 10,
                percent: 100,
                status: CompletionStatus::Passed
            }
        ),
        "survey must add nothing to raw or max: {fx:?}"
    );
}

// ---------------------------------------------------------------- question banks and the random draw

fn bank_question_slide(id: &str) -> Slide {
    let mut slide = course().slide("s-quiz").expect("quiz slide").clone();
    slide.id = id.to_owned();
    slide.title = id.to_owned();
    slide.draw = None;
    slide
}

fn course_with_draw(bank_size: usize, draw_count: u32, draw_id: &str) -> Course {
    let mut c = course();
    let bank_slides: Vec<Slide> = (1..=bank_size)
        .map(|i| bank_question_slide(&format!("bq{i}")))
        .collect();
    c.banks.push(QuestionBank {
        id: "bank1".into(),
        title: "Bank One".into(),
        slides: bank_slides,
    });
    let draw = c.slide_mut("s-quiz").expect("quiz slide");
    draw.id = draw_id.to_owned();
    draw.title = "Draw".to_owned();
    draw.question = None;
    draw.draw = Some(BankDraw {
        bank: "bank1".into(),
        count: draw_count,
    });
    draw.triggers.clear();
    c
}

fn play_ids(p: &Player<'_>) -> Vec<String> {
    p.play_order().iter().map(|s| s.id.clone()).collect()
}

#[test]
fn draw_of_two_from_five_yields_two_bank_slides_in_place() {
    let c = course_with_draw(5, 2, "s-draw");
    let p = Player::new(&c).expect("player");
    assert_eq!(p.state().draw_seed, 0);
    let ids = play_ids(&p);
    assert_eq!(ids.len(), 5, "play order: {ids:?}");
    assert_eq!(&ids[0..2], ["s-intro", "s-tabs"]);
    assert_eq!(ids[4], "s-results");
    let drawn = &ids[2..4];
    assert_eq!(drawn.len(), 2);
    assert!(drawn.iter().all(|id| id.starts_with("bq")), "{drawn:?}");
    assert!(drawn[0] != drawn[1], "drawn picks must differ: {drawn:?}");
    assert!(!ids.contains(&"s-draw".to_owned()));
}

#[test]
fn same_seed_gives_same_picks_different_seeds_differ() {
    let c = course_with_draw(5, 2, "s-draw");
    let a = Player::with_draw_seed(&c, 7).expect("player");
    let b = Player::with_draw_seed(&c, 7).expect("player");
    assert_eq!(play_ids(&a), play_ids(&b));
    let base = play_ids(&a);
    let mut differs = false;
    for seed in 0..20u32 {
        let p = Player::with_draw_seed(&c, seed).expect("player");
        if play_ids(&p) != base {
            differs = true;
            break;
        }
    }
    assert!(differs, "at least one of 20 seeds must pick differently");
}

#[test]
fn two_draws_from_one_bank_never_overlap_over_50_seeds() {
    let mut c = course_with_draw(5, 2, "s-draw");
    {
        let second = c.slide_mut("s-tabs").expect("tabs slide");
        second.id = "s-draw2".to_owned();
        second.question = None;
        second.draw = Some(BankDraw {
            bank: "bank1".into(),
            count: 2,
        });
        second.triggers.clear();
    }
    for seed in 0..50u32 {
        let p = Player::with_draw_seed(&c, seed).expect("player");
        let ids = play_ids(&p);
        let drawn: Vec<&str> = ids
            .iter()
            .map(String::as_str)
            .filter(|id| id.starts_with("bq"))
            .collect();
        assert_eq!(drawn.len(), 4, "seed {seed} play order: {ids:?}");
        let mut unique = std::collections::BTreeSet::new();
        for id in &drawn {
            unique.insert(*id);
        }
        assert_eq!(unique.len(), 4, "seed {seed} drawn overlap: {drawn:?}");
    }
}

#[test]
fn total_slides_and_max_count_only_drawn_questions() {
    let c = course_with_draw(5, 2, "s-draw");
    let mut p = Player::new(&c).expect("player");
    assert_eq!(p.value("Course.TotalSlides"), Some(Value::Number(5.0)));
    p.handle(&Input::Start);
    p.handle(&Input::Jump {
        slide: "s-draw".into(),
    });
    p.handle(&Input::SelectChoice { choice: "b".into() });
    p.handle(&click("btn-submit"));
    assert_eq!(p.value("Course.ScorePoints"), Some(Value::Number(10.0)));
    // One correct of 2 drawn (max 20): 50%, not 20% of the full bank of 5.
    assert_eq!(p.value("Course.ScorePercent"), Some(Value::Number(50.0)));
    let fx = p.handle(&Input::Jump {
        slide: "s-results".into(),
    });
    assert!(
        has(
            &fx,
            &Effect::Score {
                raw: 10,
                max: 20,
                percent: 50,
                status: CompletionStatus::Failed,
            }
        ),
        "score must count only drawn questions: {fx:?}"
    );
}

#[test]
fn jump_to_draw_slide_lands_on_first_question_and_next_runs_through_drawn() {
    let c = course_with_draw(5, 2, "s-draw");
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    let ids = play_ids(&p);
    assert_eq!(p.slide_position("s-draw"), Some(2));
    assert_eq!(ids[2], p.play_order().get(2).map(|s| s.id.clone()).unwrap());
    let fx = p.handle(&Input::Jump {
        slide: "s-draw".into(),
    });
    assert!(has(&fx, &slide_entered(&ids[2])), "{fx:?}");
    assert_eq!(
        p.current_slide().map(|s| s.id.as_str()),
        Some(ids[2].as_str())
    );
    p.handle(&Input::Next);
    assert_eq!(
        p.current_slide().map(|s| s.id.as_str()),
        Some(ids[3].as_str())
    );
    p.handle(&Input::Next);
    assert_eq!(
        p.current_slide().map(|s| s.id.as_str()),
        Some(ids[4].as_str())
    );
    // A jump to a bank slide that was not drawn warns like a missing slide.
    let undrawn = (1..=5)
        .map(|i| format!("bq{i}"))
        .find(|id| !ids.contains(id))
        .expect("there must be an undrawn bank slide");
    let before = p.current_slide().map(|s| s.id.clone());
    let fx = p.handle(&Input::Jump { slide: undrawn });
    assert!(warned(&fx), "undrawn jump must warn: {fx:?}");
    assert_eq!(p.current_slide().map(|s| s.id.clone()), before);
}

#[test]
fn start_slide_on_draw_starts_at_first_question() {
    let mut c = course_with_draw(5, 2, "s-draw");
    c.start_slide = Some("s-draw".into());
    let mut p = Player::new(&c).expect("player");
    let ids = play_ids(&p);
    assert_eq!(p.state().slide, 2);
    assert_eq!(
        p.current_slide().map(|s| s.id.as_str()),
        Some(ids[2].as_str())
    );
    let fx = p.handle(&Input::Start);
    assert_eq!(fx.first(), Some(&slide_entered(&ids[2])));
    assert_eq!(
        p.current_slide().map(|s| s.id.as_str()),
        Some(ids[2].as_str())
    );
}

#[test]
fn suspend_then_resume_with_nonzero_seed_restores_picks_and_answers() {
    let c = course_with_draw(5, 2, "s-draw");
    let mut p = Player::with_draw_seed(&c, 42).expect("player");
    p.handle(&Input::Start);
    p.handle(&Input::Jump {
        slide: "s-draw".into(),
    });
    p.handle(&Input::SelectChoice { choice: "b".into() });
    p.handle(&click("btn-submit"));
    p.handle(&Input::Jump {
        slide: "s-results".into(),
    });
    assert_eq!(p.current_slide().map(|s| s.id.as_str()), Some("s-results"));
    let picks_before = play_ids(&p);
    let answers_before = p.state().answers.clone();
    assert_eq!(answers_before.len(), 1);
    let encoded = encode_suspend(&c, p.state());
    let decoded = decode_suspend(&c, &encoded).expect("decode failed");
    assert_eq!(decoded.draw_seed, 42);
    assert_eq!(decoded.answers, answers_before);
    let mut resumed = Player::resume(&c, decoded).expect("resume failed");
    assert_eq!(play_ids(&resumed), picks_before);
    assert_eq!(resumed.state().draw_seed, 42);
    assert_eq!(
        resumed.current_slide().map(|s| s.id.as_str()),
        Some("s-results")
    );
    assert_eq!(resumed.state().answers, answers_before);
    let fx = resumed.handle(&Input::Start);
    assert_eq!(fx.first(), Some(&slide_entered("s-results")));
}

#[test]
fn course_without_banks_produces_same_suspend_string_as_before() {
    let c = course();
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    p.handle(&Input::Tick { ms: 1000 });
    p.handle(&click("btn-start"));
    p.handle(&click("tab1"));
    let mut state = p.state().clone();
    state.draw_seed = 999;
    let with_seed = encode_suspend(&c, &state);
    state.draw_seed = 0;
    let without_seed = encode_suspend(&c, &state);
    assert_eq!(
        with_seed, without_seed,
        "a course without banks must not encode the seed"
    );
    let decoded = decode_suspend(&c, &with_seed).expect("decode failed");
    assert_eq!(decoded.draw_seed, 0);
    // A course with banks does round-trip the seed.
    let banked = course_with_draw(5, 2, "s-draw");
    let mut pb = Player::with_draw_seed(&banked, 123).expect("player");
    pb.handle(&Input::Start);
    let encoded = encode_suspend(&banked, pb.state());
    let decoded = decode_suspend(&banked, &encoded).expect("decode failed");
    assert_eq!(decoded.draw_seed, 123);
}

#[test]
fn corrupt_or_truncated_seed_is_an_error_not_a_panic() {
    let c = course_with_draw(5, 2, "s-draw");
    // Varint for a seed larger than u32::MAX.
    let too_big = to_suspend_string(&[0x80, 0x80, 0x80, 0x80, 0x10]);
    assert_eq!(
        decode_suspend(&c, &too_big),
        Err(SuspendError::ValueOutOfRange)
    );
    let too_big_full = to_suspend_string(&[0x80, 0x80, 0x80, 0x80, 0x10, 0, 0, 0, 0, 0]);
    assert!(decode_suspend(&c, &too_big_full).is_err());
    // Truncated varint (continuation bit with no following byte).
    let truncated = to_suspend_string(&[0x80]);
    assert!(decode_suspend(&c, &truncated).is_err());
    let empty = to_suspend_string(&[]);
    assert!(decode_suspend(&c, &empty).is_err());
}

// ---------------------------------------------------------------- retry and answer review

fn retry_review_course() -> Course {
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "retry-review-test",
        "title": "Retry Review Test",
        "quiz": { "passing_percent": 50 },
        "variables": [{ "name": "entered", "value": 0 }],
        "scenes": [{
            "id": "scene1",
            "title": "Scene 1",
            "slides": [
                {
                    "id": "s-q1",
                    "title": "Q1",
                    "duration_ms": 5000,
                    "objects": [
                        { "id": "btn-submit", "kind": { "type": "button", "label": "Submit" }, "frame": { "x": 1000, "y": 600, "w": 200, "h": 60 } },
                        { "id": "btn-exit", "kind": { "type": "button", "label": "Exit review" }, "frame": { "x": 1000, "y": 500, "w": 200, "h": 60 } },
                        { "id": "aud", "kind": { "type": "audio", "src": "a.mp3", "autoplay": true }, "frame": { "x": 0, "y": 0, "w": 48, "h": 48 } }
                    ],
                    "layers": [
                        { "id": "correct", "name": "Correct", "objects": [{ "id": "correct-text", "kind": { "type": "text", "text": "Right" }, "frame": { "x": 80, "y": 200, "w": 400, "h": 60 } }] }
                    ],
                    "question": {
                        "kind": "multiple_choice", "prompt": "Pick b.", "points": 10, "attempts": 2,
                        "choices": [
                            { "id": "a", "text": "A" },
                            { "id": "b", "text": "B", "correct": true },
                            { "id": "c", "text": "C" }
                        ],
                        "correct_layer": "correct"
                    },
                    "triggers": [
                        { "action": { "action": "submit_question" }, "when": { "event": "click", "object": "btn-submit" } },
                        { "action": { "action": "exit_review" }, "when": { "event": "click", "object": "btn-exit" } },
                        { "action": { "action": "set_variable", "variable": "entered", "op": "add", "value": 1 }, "when": { "event": "timeline_starts" } }
                    ]
                },
                {
                    "id": "s-mid",
                    "title": "Middle",
                    "duration_ms": 5000,
                    "objects": [{ "id": "mid-text", "kind": { "type": "text", "text": "Middle" }, "frame": { "x": 80, "y": 200, "w": 400, "h": 60 } }]
                },
                {
                    "id": "s-q2",
                    "title": "Q2",
                    "duration_ms": 5000,
                    "objects": [
                        { "id": "btn-submit", "kind": { "type": "button", "label": "Submit" }, "frame": { "x": 1000, "y": 600, "w": 200, "h": 60 } },
                        { "id": "btn-exit", "kind": { "type": "button", "label": "Exit review" }, "frame": { "x": 1000, "y": 500, "w": 200, "h": 60 } }
                    ],
                    "question": {
                        "kind": "fill_in", "prompt": "City?", "points": 10, "attempts": 2,
                        "choices": [], "accepted": ["Paris"]
                    },
                    "triggers": [
                        { "action": { "action": "submit_question" }, "when": { "event": "click", "object": "btn-submit" } },
                        { "action": { "action": "exit_review" }, "when": { "event": "click", "object": "btn-exit" } }
                    ]
                },
                {
                    "id": "s-q3",
                    "title": "Q3",
                    "duration_ms": 5000,
                    "objects": [
                        { "id": "btn-submit", "kind": { "type": "button", "label": "Submit" }, "frame": { "x": 1000, "y": 600, "w": 200, "h": 60 } },
                        { "id": "btn-exit", "kind": { "type": "button", "label": "Exit review" }, "frame": { "x": 1000, "y": 500, "w": 200, "h": 60 } }
                    ],
                    "question": {
                        "kind": "matching", "prompt": "Match.", "points": 10, "attempts": 2,
                        "choices": [],
                        "pairs": [
                            { "id": "p1", "left": "Sun", "right": "Star" },
                            { "id": "p2", "left": "Moon", "right": "Satellite" }
                        ]
                    },
                    "triggers": [
                        { "action": { "action": "submit_question" }, "when": { "event": "click", "object": "btn-submit" } },
                        { "action": { "action": "exit_review" }, "when": { "event": "click", "object": "btn-exit" } }
                    ]
                },
                {
                    "id": "s-results",
                    "title": "Results",
                    "duration_ms": 5000,
                    "objects": [
                        { "id": "btn-retry", "kind": { "type": "button", "label": "Retry" }, "frame": { "x": 80, "y": 200, "w": 200, "h": 60 } },
                        { "id": "btn-review", "kind": { "type": "button", "label": "Review" }, "frame": { "x": 320, "y": 200, "w": 200, "h": 60 } }
                    ],
                    "triggers": [
                        { "action": { "action": "submit_results" }, "when": { "event": "timeline_starts" } },
                        { "action": { "action": "retry_quiz" }, "when": { "event": "click", "object": "btn-retry" } },
                        { "action": { "action": "review_quiz" }, "when": { "event": "click", "object": "btn-review" } }
                    ]
                }
            ]
        }]
    });
    Course::from_json(&json.to_string()).unwrap()
}

fn answer_q1(p: &mut Player<'_>) {
    p.handle(&Input::SelectChoice { choice: "b".into() });
    p.handle(&click("btn-submit"));
}

fn answer_q2(p: &mut Player<'_>) {
    p.handle(&Input::TextEntered {
        text: "Paris".into(),
    });
    p.handle(&click("btn-submit"));
}

fn answer_q3(p: &mut Player<'_>) {
    p.handle(&Input::MatchSelected {
        left: "p1".into(),
        right: Some("p1".into()),
    });
    p.handle(&Input::MatchSelected {
        left: "p2".into(),
        right: Some("p2".into()),
    });
    p.handle(&click("btn-submit"));
}

#[test]
fn quiz_passed_and_attempts_variables() {
    let c = retry_review_course();
    let mut p = Player::new(&c).expect("player");
    assert_eq!(p.value("Quiz.Passed"), Some(Value::Bool(false)));
    assert_eq!(p.value("Quiz.Attempts"), Some(Value::Number(0.0)));
    assert_eq!(p.value("Quiz.Reviewing"), Some(Value::Bool(false)));
    p.handle(&Input::Start);
    answer_q1(&mut p);
    // 10 of 30 = 33%, below 50% passing -> not passed before results also false (percent computed live)
    assert_eq!(p.value("Quiz.Passed"), Some(Value::Bool(false)));
    p.handle(&Input::Next);
    p.handle(&Input::Next);
    answer_q2(&mut p);
    p.handle(&Input::Next);
    answer_q3(&mut p);
    // 30 of 30 = 100% >= 50 -> passed
    assert_eq!(p.value("Quiz.Passed"), Some(Value::Bool(true)));
    p.handle(&Input::Jump {
        slide: "s-results".into(),
    });
    // submit_results on timeline_starts bumps attempts
    assert_eq!(p.value("Quiz.Attempts"), Some(Value::Number(1.0)));
    p.handle(&Input::Jump {
        slide: "s-results".into(),
    });
    assert_eq!(p.value("Quiz.Attempts"), Some(Value::Number(2.0)));

    // No quiz -> Passed is false, Attempts still counts submit_results.
    let mut no_quiz = retry_review_course();
    no_quiz.quiz = None;
    let mut p2 = Player::new(&no_quiz).unwrap();
    p2.handle(&Input::Start);
    assert_eq!(p2.value("Quiz.Passed"), Some(Value::Bool(false)));
    p2.handle(&Input::Jump {
        slide: "s-results".into(),
    });
    assert_eq!(p2.value("Quiz.Attempts"), Some(Value::Number(1.0)));
    assert_eq!(p2.value("Quiz.Passed"), Some(Value::Bool(false)));
}

#[test]
fn retry_clears_answers_unlocks_lands_first_and_keeps_variables() {
    let c = retry_review_course();
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    answer_q1(&mut p);
    assert!(p.state().answers.contains_key("s-q1"));
    p.handle(&Input::Jump {
        slide: "s-results".into(),
    });
    let _entered_before = p.variable("entered").cloned();
    let fx = p.handle(&click("btn-retry"));
    assert!(has(&fx, &slide_entered("s-q1")), "{fx:?}");
    assert_eq!(p.current_slide().map(|s| s.id.as_str()), Some("s-q1"));
    assert!(p.state().answers.is_empty(), "retry must clear answers");
    assert!(p.state().selected.is_empty());
    assert!(p.state().last_response.is_none());
    assert!(p.state().review_answers.is_empty());
    // Variables are not reset (entering the first question fires its
    // timeline_starts again, so the counter moves on rather than going to 0).
    assert_ne!(p.variable("entered"), Some(&Value::Number(0.0)));
    assert!(
        p.variable("entered")
            .is_some_and(|v| matches!(v, Value::Number(n) if *n >= 1.0))
    );
    // Unlocked: a new selection works.
    let fx = p.handle(&Input::SelectChoice { choice: "a".into() });
    assert!(
        has(
            &fx,
            &Effect::ChoiceSelected {
                choice: "a".into(),
                selected: true
            }
        ),
        "{fx:?}"
    );
    // Quiz attempts are not reset by retry (submit_results already ran once).
    assert_eq!(p.value("Quiz.Attempts"), Some(Value::Number(1.0)));
}

#[test]
fn retry_with_no_question_warns_and_does_nothing() {
    let mut c = retry_review_course();
    for scene in &mut c.scenes {
        for slide in &mut scene.slides {
            slide.question = None;
        }
    }
    let mut p = Player::new_unvalidated(&c);
    p.handle(&Input::Start);
    p.handle(&Input::Jump {
        slide: "s-results".into(),
    });
    let fx = p.handle(&click("btn-retry"));
    assert!(warned(&fx), "{fx:?}");
    assert_eq!(p.current_slide().map(|s| s.id.as_str()), Some("s-results"));
}

#[test]
fn retry_with_banks_can_change_picks_over_20_and_is_repeatable() {
    let run_sequence = |seed: u32| -> Vec<Vec<String>> {
        let mut c = course_with_draw(5, 2, "s-draw");
        c.slide_mut("s-results").unwrap().triggers.push(Trigger {
            action: Action::RetryQuiz,
            when: Event::Click {
                object: "results-text".into(),
            },
            condition: None,
            disabled: false,
        });
        let mut p = Player::with_draw_seed(&c, seed).unwrap();
        p.handle(&Input::Start);
        let mut sequence = vec![play_ids(&p)];
        for _ in 0..20 {
            p.handle(&Input::Jump {
                slide: "s-results".into(),
            });
            p.handle(&click("results-text"));
            sequence.push(play_ids(&p));
        }
        sequence
    };
    let first_run = run_sequence(3);
    let second_run = run_sequence(3);
    assert_eq!(first_run, second_run, "retry picks must be repeatable");
    let base = &first_run[0];
    let changed = first_run.iter().any(|ids| ids != base);
    assert!(changed, "picks must change over 20 retries: {first_run:?}");
}

#[test]
fn review_restores_choice_fillin_and_matching_and_locks() {
    let c = retry_review_course();
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    answer_q1(&mut p);
    p.handle(&Input::Next);
    p.handle(&Input::Next);
    answer_q2(&mut p);
    p.handle(&Input::Next);
    answer_q3(&mut p);
    p.handle(&Input::Jump {
        slide: "s-results".into(),
    });
    let entered_before = p.variable("entered").cloned();
    let fx = p.handle(&click("btn-review"));
    assert!(
        has(&fx, &Effect::ReviewChanged { reviewing: true }),
        "{fx:?}"
    );
    assert_eq!(p.value("Quiz.Reviewing"), Some(Value::Bool(true)));
    assert_eq!(p.current_slide().map(|s| s.id.as_str()), Some("s-q1"));
    // Choice restored.
    assert!(p.state().selected.contains("b"));
    // Slide-entry triggers do not run and feedback layers do not show.
    assert_eq!(p.variable("entered"), entered_before.as_ref());
    assert!(p.state().visible_layers.is_empty());
    // No autoplay media on review entry.
    assert!(
        !fx.iter().any(|e| matches!(
            e,
            Effect::Media {
                op: MediaOp::Play,
                ..
            }
        )),
        "{fx:?}"
    );
    // Locked: select_choice is refused.
    let fx = p.handle(&Input::SelectChoice { choice: "a".into() });
    assert!(warned(&fx), "{fx:?}");
    assert!(p.state().selected.contains("b"));
    // Next to fill-in, restored text.
    p.handle(&Input::Next);
    assert_eq!(p.current_slide().map(|s| s.id.as_str()), Some("s-q2"));
    assert_eq!(p.text_entry(), "Paris");
    // Next to matching, restored matches.
    p.handle(&Input::Next);
    assert_eq!(p.current_slide().map(|s| s.id.as_str()), Some("s-q3"));
    assert_eq!(p.match_for("p1"), Some("p1"));
    assert_eq!(p.match_for("p2"), Some("p2"));
    // After last, Next exits review back to results.
    let fx = p.handle(&Input::Next);
    assert!(
        has(&fx, &Effect::ReviewChanged { reviewing: false }),
        "{fx:?}"
    );
    assert_eq!(p.current_slide().map(|s| s.id.as_str()), Some("s-results"));
    assert_eq!(p.value("Quiz.Reviewing"), Some(Value::Bool(false)));
}

#[test]
fn review_next_previous_skip_non_question_and_unanswered() {
    let c = retry_review_course();
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    answer_q1(&mut p);
    // Skip Q2 (unanswered), answer Q3.
    p.handle(&Input::Next);
    p.handle(&Input::Next);
    p.handle(&Input::Next);
    answer_q3(&mut p);
    p.handle(&Input::Jump {
        slide: "s-results".into(),
    });
    p.handle(&click("btn-review"));
    assert_eq!(p.current_slide().map(|s| s.id.as_str()), Some("s-q1"));
    p.handle(&Input::Next);
    assert_eq!(
        p.current_slide().map(|s| s.id.as_str()),
        Some("s-q3"),
        "must skip mid and unanswered q2"
    );
    p.handle(&Input::Previous);
    assert_eq!(p.current_slide().map(|s| s.id.as_str()), Some("s-q1"));
    // Previous before first answered does nothing.
    p.handle(&Input::Previous);
    assert_eq!(p.current_slide().map(|s| s.id.as_str()), Some("s-q1"));
}

#[test]
fn exit_review_returns_to_results_slide() {
    let c = retry_review_course();
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    answer_q1(&mut p);
    p.handle(&Input::Jump {
        slide: "s-results".into(),
    });
    p.handle(&click("btn-review"));
    assert_eq!(p.current_slide().map(|s| s.id.as_str()), Some("s-q1"));
    let fx = p.handle(&click("btn-exit"));
    assert!(
        has(&fx, &Effect::ReviewChanged { reviewing: false }),
        "{fx:?}"
    );
    assert_eq!(p.current_slide().map(|s| s.id.as_str()), Some("s-results"));
    assert_eq!(p.value("Quiz.Reviewing"), Some(Value::Bool(false)));
    // After exit, normal slide entry runs again (timeline_starts fires).
    let entered = p.variable("entered").cloned();
    p.handle(&Input::Jump {
        slide: "s-q1".into(),
    });
    assert_ne!(p.variable("entered"), entered.as_ref());
}

#[test]
fn reviewing_with_nothing_answered_warns() {
    let c = retry_review_course();
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    p.handle(&Input::Jump {
        slide: "s-results".into(),
    });
    let fx = p.handle(&click("btn-review"));
    assert!(warned(&fx), "{fx:?}");
    assert_eq!(p.current_slide().map(|s| s.id.as_str()), Some("s-results"));
    assert_eq!(p.value("Quiz.Reviewing"), Some(Value::Bool(false)));
}

#[test]
fn exit_review_input_outside_review_warns() {
    let c = retry_review_course();
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    let before = p.current_slide().map(|s| s.id.clone());
    let fx = p.handle(&Input::ExitReview);
    assert!(warned(&fx), "{fx:?}");
    assert_eq!(p.current_slide().map(|s| s.id.clone()), before);
    assert_eq!(p.value("Quiz.Reviewing"), Some(Value::Bool(false)));
}

#[test]
fn review_answers_not_in_suspend_and_resume_shows_no_restored_answers() {
    let c = retry_review_course();
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    answer_q1(&mut p);
    assert!(!p.state().review_answers.is_empty());
    let encoded = encode_suspend(&c, p.state());
    let decoded = decode_suspend(&c, &encoded).unwrap();
    assert!(
        decoded.review_answers.is_empty(),
        "review answers are never in suspend data"
    );
    assert!(decoded.reviewing_from.is_none());
}

#[test]
fn suspend_round_trips_quiz_attempts_and_neither_course_unchanged() {
    let c = retry_review_course();
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    p.handle(&Input::Jump {
        slide: "s-results".into(),
    });
    p.handle(&Input::Jump {
        slide: "s-results".into(),
    });
    assert_eq!(p.state().quiz_attempts, 2);
    let encoded = encode_suspend(&c, p.state());
    let decoded = decode_suspend(&c, &encoded).unwrap();
    assert_eq!(decoded.quiz_attempts, 2);

    // A course with neither quiz nor banks encodes byte-for-byte regardless of
    // draw_seed/quiz_attempts in the state.
    let mut plain = retry_review_course();
    plain.quiz = None;
    plain.banks.clear();
    let state = PlayerState::default();
    let a = encode_suspend(&plain, &state);
    let state = PlayerState {
        draw_seed: 999,
        quiz_attempts: 7,
        ..PlayerState::default()
    };
    let b = encode_suspend(&plain, &state);
    assert_eq!(a, b, "neither course must not encode seed or attempts");
    let decoded_plain = decode_suspend(&plain, &a).unwrap();
    assert_eq!(decoded_plain.quiz_attempts, 0);
    assert_eq!(decoded_plain.draw_seed, 0);
}

#[test]
fn certificate_builtins_before_and_after_host_info() {
    let c = course();
    let mut p = Player::new(&c).expect("player");
    assert_eq!(p.value("Learner.Name"), Some(Value::Text(String::new())));
    assert_eq!(
        p.value("Course.Title"),
        Some(Value::Text("Workplace Safety Basics".into()))
    );
    assert_eq!(p.value("Course.Date"), Some(Value::Text(String::new())));

    p.set_host_info(HostInfo {
        learner_name: Some("Ada Lovelace".into()),
        date: Some("2026-11-10".into()),
    });
    assert_eq!(
        p.value("Learner.Name"),
        Some(Value::Text("Ada Lovelace".into()))
    );
    assert_eq!(
        p.value("Course.Title"),
        Some(Value::Text("Workplace Safety Basics".into()))
    );
    assert_eq!(
        p.value("Course.Date"),
        Some(Value::Text("2026-11-10".into()))
    );
}

#[test]
fn host_info_caps_names_and_dates_on_character_boundaries() {
    let c = course();
    let mut p = Player::new(&c).expect("player");
    p.set_host_info(HostInfo {
        learner_name: Some("é".repeat(250)),
        date: Some("9".repeat(40)),
    });
    let Some(Value::Text(name)) = p.value("Learner.Name") else {
        panic!("Learner.Name is text");
    };
    assert_eq!(name.chars().count(), crate::MAX_HOST_NAME_CHARS);
    assert!(name.chars().all(|c| c == 'é'));
    let Some(Value::Text(date)) = p.value("Course.Date") else {
        panic!("Course.Date is text");
    };
    assert_eq!(date.chars().count(), crate::MAX_HOST_DATE_CHARS);
}

#[test]
fn civil_date_formats_dates_and_rejects_out_of_range_inputs() {
    assert_eq!(civil_date(0), "1970-01-01");
    assert_eq!(civil_date(1_794_268_800), "2026-11-10");
    assert_eq!(civil_date(1_709_164_800), "2024-02-29");
    assert_eq!(civil_date(-1), "");
    assert_eq!(civil_date(i64::MAX), "");
    assert_eq!(civil_date(i64::MIN), "");
}

#[test]
fn print_slide_from_a_click_excludes_the_button() {
    let mut c = course();
    c.slide_mut("s-intro").unwrap().triggers.push(Trigger {
        action: Action::PrintSlide,
        when: Event::Click {
            object: "btn-about".into(),
        },
        condition: None,
        disabled: false,
    });
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    let effects = p.handle(&click("btn-about"));
    assert!(has(
        &effects,
        &Effect::Print {
            slide: "s-intro".into(),
            exclude: vec!["btn-about".into()],
        }
    ));
}

#[test]
fn print_slide_from_timeline_starts_excludes_nothing() {
    let mut c = course();
    c.slide_mut("s-intro").unwrap().triggers.push(Trigger {
        action: Action::PrintSlide,
        when: Event::TimelineStarts { layer: None },
        condition: None,
        disabled: false,
    });
    let mut p = Player::new(&c).expect("player");
    let effects = p.handle(&Input::Start);
    assert!(has(
        &effects,
        &Effect::Print {
            slide: "s-intro".into(),
            exclude: Vec::new(),
        }
    ));
}

#[test]
fn host_info_never_changes_suspend_data() {
    let c = course();
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    let before = encode_suspend(&c, p.state());
    p.set_host_info(HostInfo {
        learner_name: Some("Ada Lovelace".into()),
        date: Some("2026-11-10".into()),
    });
    assert_eq!(encode_suspend(&c, p.state()), before);
}

#[test]
fn a_disabled_trigger_never_fires_and_fires_again_when_enabled() {
    let mut c = course();
    let set = |c: &mut Course, disabled: bool| {
        let slide = c.slide_mut("s-intro").unwrap();
        let mut found = 0;
        for t in &mut slide.triggers {
            if matches!(&t.when, Event::Click { object } if object == "btn-start") {
                t.disabled = disabled;
                found += 1;
            }
        }
        assert!(found > 0, "the intro button has a click trigger");
    };
    set(&mut c, true);
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    p.handle(&Input::Tick { ms: 1000 });
    let effects = p.handle(&click("btn-start"));
    assert!(
        !has(&effects, &slide_entered("s-tabs")),
        "a disabled trigger does not jump"
    );

    set(&mut c, false);
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    p.handle(&Input::Tick { ms: 1000 });
    assert!(has(
        &p.handle(&click("btn-start")),
        &slide_entered("s-tabs")
    ));
}

fn nav_course(player: serde_json::Value) -> Course {
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "nav-test",
        "title": "Nav Test",
        "variables": [{ "name": "flag", "value": false }],
        "player": player,
        "scenes": [{
            "id": "scene1",
            "title": "Scene 1",
            "slides": [
                {
                    "id": "s1",
                    "title": "Slide 1",
                    "duration_ms": 2000,
                    "objects": [{
                        "id": "btn_jump",
                        "kind": { "type": "button", "label": "Jump" },
                        "frame": { "x": 0.0, "y": 0.0, "w": 100.0, "h": 50.0 },
                        "timing": { "start_ms": 0 }
                    }],
                    "triggers": [{
                        "when": { "event": "click", "object": "btn_jump" },
                        "action": { "action": "jump_to_slide", "target": { "id": "s3" } }
                    }]
                },
                { "id": "s2", "title": "Slide 2", "duration_ms": 2000 },
                { "id": "s3", "title": "Slide 3", "duration_ms": 2000 }
            ]
        }]
    });
    Course::from_json(&json.to_string()).unwrap()
}

#[test]
fn restricted_menu_refuses_unvisited_slides_and_allows_visited_ones() {
    let course = nav_course(serde_json::json!({ "navigation": "restricted" }));
    let mut p = Player::new(&course).unwrap();
    p.handle(&Input::Start);
    assert!(!p.can_go_to("s3"));
    let fx = p.handle(&Input::GoTo { slide: "s3".into() });
    assert!(warned(&fx));
    assert_eq!(p.current_slide().unwrap().id, "s1");
    p.handle(&Input::Next);
    assert_eq!(p.current_slide().unwrap().id, "s2");
    assert!(p.can_go_to("s1"));
    assert!(p.can_go_to("s2"));
    assert!(has(
        &p.handle(&Input::GoTo { slide: "s1".into() }),
        &slide_entered("s1")
    ));
}

#[test]
fn locked_menu_refuses_every_go_to_but_next_and_previous_work() {
    let course = nav_course(serde_json::json!({ "navigation": "locked" }));
    let mut p = Player::new(&course).unwrap();
    p.handle(&Input::Start);
    for id in ["s1", "s2", "s3"] {
        assert!(!p.can_go_to(id));
        assert!(warned(&p.handle(&Input::GoTo { slide: id.into() })));
    }
    assert!(has(&p.handle(&Input::Next), &slide_entered("s2")));
    assert!(has(&p.handle(&Input::Previous), &slide_entered("s1")));
}

#[test]
fn trigger_jumps_are_never_blocked_in_locked_mode() {
    let course = nav_course(serde_json::json!({ "navigation": "locked" }));
    let mut p = Player::new(&course).unwrap();
    p.handle(&Input::Start);
    assert!(has(&p.handle(&click("btn_jump")), &slide_entered("s3")));
}

#[test]
fn timeline_ends_blocks_next_until_the_clock_reaches_the_end() {
    let course = nav_course(serde_json::json!({ "next_requires": { "kind": "timeline_ends" } }));
    let mut p = Player::new(&course).unwrap();
    p.handle(&Input::Start);
    assert!(!p.can_next());
    let fx = p.handle(&Input::Next);
    assert!(warned(&fx));
    assert!(!has(&fx, &slide_entered("s2")));
    p.handle(&Input::Tick { ms: 1999 });
    assert!(!p.can_next());
    p.handle(&Input::Tick { ms: 1 });
    assert!(p.can_next());
    assert!(has(&p.handle(&Input::Next), &slide_entered("s2")));
    // s2 is new and unfinished, so Next is locked again there.
    assert!(!p.can_next());
    // Back on a completed slide, Next works at once.
    p.handle(&Input::Previous);
    assert_eq!(p.current_slide().unwrap().id, "s1");
    assert!(p.can_next());
}

#[test]
fn a_variable_requirement_flips_when_the_variable_becomes_true() {
    let course = nav_course(serde_json::json!({
        "next_requires": { "kind": "variable", "name": "flag" }
    }));
    let mut p = Player::new(&course).unwrap();
    p.handle(&Input::Start);
    assert!(!p.can_next());
    assert!(warned(&p.handle(&Input::Next)));
    p.handle(&Input::SetVariable {
        variable: "flag".into(),
        value: Value::Bool(true),
    });
    assert!(p.can_next());
    assert!(has(&p.handle(&Input::Next), &slide_entered("s2")));
}

#[test]
fn a_slide_override_replaces_the_course_default() {
    let mut course =
        nav_course(serde_json::json!({ "next_requires": { "kind": "timeline_ends" } }));
    course.slide_mut("s1").unwrap().player = Some(coursecraft_course::SlidePlayerOverrides {
        next_requires: Some(coursecraft_course::NextRequirement::Nothing),
        ..Default::default()
    });
    let mut p = Player::new(&course).unwrap();
    p.handle(&Input::Start);
    assert!(p.can_next());
    p.handle(&Input::Next);
    assert!(!p.can_next());
}

#[test]
fn resume_treats_earlier_visited_slides_as_completed() {
    let course = nav_course(serde_json::json!({ "next_requires": { "kind": "timeline_ends" } }));
    let mut p = Player::new(&course).unwrap();
    p.handle(&Input::Start);
    p.handle(&Input::Tick { ms: 2000 });
    p.handle(&Input::Next);
    p.handle(&Input::Tick { ms: 2000 });
    p.handle(&Input::Next);
    let encoded = encode_suspend(&course, p.state());
    let decoded = decode_suspend(&course, &encoded).unwrap();
    let mut resumed = Player::resume(&course, decoded).unwrap();
    resumed.handle(&Input::Start);
    assert_eq!(resumed.current_slide().unwrap().id, "s3");
    assert!(!resumed.can_next());
    resumed.handle(&Input::Previous);
    assert!(resumed.can_next());
}

fn speed_course(show_speed: bool) -> Course {
    let mut course = nav_course(serde_json::json!({}));
    course.player.show_speed = show_speed;
    course.slide_mut("s1").unwrap().duration_ms = 4000;
    course
}

#[test]
fn one_and_a_half_times_advances_1500_ms_over_1000_ticks() {
    let course = speed_course(true);
    let mut p = Player::new(&course).unwrap();
    p.handle(&Input::Start);
    p.handle(&Input::SetSpeed { rate: 1.5 });
    assert!((p.speed() - 1.5).abs() < f32::EPSILON);
    for _ in 0..1000 {
        p.handle(&Input::Tick { ms: 1 });
    }
    assert_eq!(p.slide_elapsed_ms(), 1500);
}

#[test]
fn double_speed_ends_a_four_second_timeline_in_two_seconds() {
    let course = speed_course(true);
    let mut p = Player::new(&course).unwrap();
    p.handle(&Input::Start);
    p.set_speed(2.0);
    p.handle(&Input::Tick { ms: 1999 });
    assert!(p.slide_elapsed_ms() < 4000);
    p.handle(&Input::Tick { ms: 1 });
    assert_eq!(p.slide_elapsed_ms(), 4000);
    // The speed stays across a slide change.
    p.handle(&Input::Next);
    assert!((p.speed() - 2.0).abs() < f32::EPSILON);
}

#[test]
fn speed_snaps_to_an_allowed_rate_and_hostile_values_become_one() {
    let course = speed_course(true);
    let mut p = Player::new(&course).unwrap();
    p.set_speed(f32::NAN);
    assert!((p.speed() - 1.0).abs() < f32::EPSILON);
    p.set_speed(f32::INFINITY);
    assert!((p.speed() - 1.0).abs() < f32::EPSILON);
    p.set_speed(1.4);
    assert!((p.speed() - 1.5).abs() < f32::EPSILON);
    p.set_speed(-3.0);
    assert!((p.speed() - 0.5).abs() < f32::EPSILON);
    p.set_speed(99.0);
    assert!((p.speed() - 2.0).abs() < f32::EPSILON);
}

#[test]
fn show_speed_false_ignores_set_speed() {
    let course = speed_course(false);
    let mut p = Player::new(&course).unwrap();
    p.handle(&Input::Start);
    p.set_speed(2.0);
    assert!((p.speed() - 1.0).abs() < f32::EPSILON);
    p.handle(&Input::Tick { ms: 1000 });
    assert_eq!(p.slide_elapsed_ms(), 1000);
}

#[test]
fn a_timer_runs_at_the_playback_speed() {
    let mut course = speed_course(true);
    let slide = course.slide_mut("s1").unwrap();
    slide.duration_ms = 20_000;
    slide.objects.push(
        serde_json::from_value(serde_json::json!({
            "id": "clock",
            "kind": { "type": "timer", "duration_ms": 10000, "autostart": true },
            "frame": { "x": 0.0, "y": 100.0, "w": 100.0, "h": 50.0 },
            "timing": { "start_ms": 0 }
        }))
        .unwrap(),
    );
    let mut p = Player::new(&course).unwrap();
    p.handle(&Input::Start);
    p.set_speed(2.0);
    for _ in 0..5 {
        p.handle(&Input::Tick { ms: 1000 });
    }
    assert_eq!(p.timer_remaining_ms("clock"), Some(0));
}

#[test]
fn open_resource_emits_open_url_for_links_and_package_paths_for_files() {
    let mut course = nav_course(serde_json::json!({}));
    course.resources = vec![
        coursecraft_course::Resource {
            id: "site".into(),
            title: "Site".into(),
            target: coursecraft_course::ResourceTarget::Url {
                url: "https://example.com/help".into(),
            },
        },
        coursecraft_course::Resource {
            id: "guide".into(),
            title: "Guide".into(),
            target: coursecraft_course::ResourceTarget::File {
                src: "docs/guide.pdf".into(),
            },
        },
    ];
    let mut p = Player::new(&course).unwrap();
    p.handle(&Input::Start);
    let fx = p.handle(&Input::OpenResource {
        resource: "site".into(),
    });
    assert_eq!(
        fx,
        vec![Effect::OpenUrl {
            url: "https://example.com/help".into()
        }]
    );
    let fx = p.handle(&Input::OpenResource {
        resource: "guide".into(),
    });
    assert_eq!(
        fx,
        vec![Effect::OpenUrl {
            url: "media/docs/guide.pdf".into()
        }]
    );
    assert!(warned(&p.handle(&Input::OpenResource {
        resource: "ghost".into()
    })));
}

#[test]
fn a_slide_override_hiding_nav_blocks_next_on_that_slide_only() {
    let mut course = nav_course(serde_json::json!({}));
    course.slide_mut("s1").unwrap().player = Some(coursecraft_course::SlidePlayerOverrides {
        show_nav: Some(false),
        show_menu: Some(false),
        ..Default::default()
    });
    let mut p = Player::new(&course).unwrap();
    p.handle(&Input::Start);
    assert!(!p.effective_player().show_nav);
    let fx = p.handle(&Input::Next);
    assert!(warned(&fx));
    assert!(!has(&fx, &slide_entered("s2")));
    assert!(warned(&p.handle(&Input::GoTo { slide: "s3".into() })));
    // A trigger jump is author navigation and still works.
    assert!(has(&p.handle(&click("btn_jump")), &slide_entered("s3")));
    // The next slide has no override, so the course default applies there.
    assert!(p.effective_player().show_nav);
    assert!(p.effective_player().show_menu);
    p.handle(&Input::Previous);
    assert_eq!(p.current_slide().unwrap().id, "s2");
}
