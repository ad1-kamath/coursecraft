use coursecraft_course::{
    Action, BACKGROUND_AUDIO_ID, BankDraw, CompletionStatus, Course, Event, NavMode, Object,
    ObjectKind, QuestionBank, RevisitMode, Slide, Trigger, Trim, Value, VarOp, VariableDef,
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
                highlight: None,
                shadow: None,
            }],
            paragraphs: Vec::new(),
            autofit: Autofit::None,
            text_style: None,
            outline: None,
            point_text: false,
        };
        slide.triggers.push(Trigger {
            action: Action::SetVariable {
                variable: "tabs_seen".into(),
                op: VarOp::Assign,
                value: Some(Value::Number(99.0).into()),
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
            cue: None,
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
fn timeline_reaches_cue_fires_at_cue_time_and_moves_with_cue() {
    let mut c = Course::from_json(HELLO).expect("hello");
    let s = c.slide_mut("s-intro").unwrap();
    s.cue_points.push(coursecraft_course::SlideCue {
        id: "beat1".into(),
        name: "Beat 1".into(),
        at_ms: 1500,
    });
    s.triggers.push(Trigger {
        action: Action::ShowLayer {
            layer: "about".into(),
        },
        when: Event::TimelineReaches {
            ms: 0,
            layer: None,
            cue: Some("beat1".into()),
        },
        condition: None,
        disabled: false,
    });

    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    assert!(!has(
        &p.handle(&Input::Tick { ms: 1400 }),
        &Effect::LayerShown {
            layer: "about".into()
        }
    ));
    assert!(has(
        &p.handle(&Input::Tick { ms: 200 }),
        &Effect::LayerShown {
            layer: "about".into()
        }
    ));

    // Moving the cue moves when the trigger fires.
    c.slide_mut("s-intro").unwrap().cue_points[0].at_ms = 3000;
    let mut p2 = Player::new(&c).expect("player");
    p2.handle(&Input::Start);
    assert!(!has(
        &p2.handle(&Input::Tick { ms: 1600 }),
        &Effect::LayerShown {
            layer: "about".into()
        }
    ));
    assert!(!has(
        &p2.handle(&Input::Tick { ms: 1300 }),
        &Effect::LayerShown {
            layer: "about".into()
        }
    ));
    assert!(has(
        &p2.handle(&Input::Tick { ms: 200 }),
        &Effect::LayerShown {
            layer: "about".into()
        }
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
            value: Some(Value::Number(1.0).into()),
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
            value: Some(Value::Number(1.0).into()),
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
                cue: None,
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
                cue: None,
            },
            action: Action::SetVariable {
                variable: "flag".into(),
                op: VarOp::Assign,
                value: Some(Value::Bool(true).into()),
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
        value: Value::Number(50.0).into(),
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
        effect: AnimEffect::new("fade"),
        duration_ms: 500,
        easing: Easing::Linear,
        sequence: Default::default(),
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
        effect: AnimEffect::with_from("fly_in", Direction::Bottom),
        duration_ms: 500,
        easing: Easing::Linear,
        sequence: Default::default(),
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
        effect: AnimEffect::new("zoom"),
        duration_ms: 500,
        easing: Easing::Linear,
        sequence: Default::default(),
    };
    let z_in_0 = sample_animation(&zoom, AnimationPhase::Entering, 0.0, frame, slide_size);
    let z_in_mid = sample_animation(&zoom, AnimationPhase::Entering, 0.5, frame, slide_size);
    let z_in_1 = sample_animation(&zoom, AnimationPhase::Entering, 1.0, frame, slide_size);
    assert_eq!(z_in_0.scale, (0.0, 0.0));
    assert_eq!(z_in_0.opacity, 0.0);
    assert_eq!(z_in_mid.scale, (0.5, 0.5));
    assert_eq!(z_in_mid.opacity, 0.5);
    assert_eq!(z_in_1.scale, (1.0, 1.0));
    assert_eq!(z_in_1.opacity, 1.0);

    let z_out_0 = sample_animation(&zoom, AnimationPhase::Exiting, 0.0, frame, slide_size);
    let z_out_mid = sample_animation(&zoom, AnimationPhase::Exiting, 0.5, frame, slide_size);
    let z_out_1 = sample_animation(&zoom, AnimationPhase::Exiting, 1.0, frame, slide_size);
    assert_eq!(z_out_0.scale, (1.0, 1.0));
    assert_eq!(z_out_0.opacity, 1.0);
    assert_eq!(z_out_mid.scale, (0.5, 0.5));
    assert_eq!(z_out_mid.opacity, 0.5);
    assert_eq!(z_out_1.scale, (0.0, 0.0));
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
        effect: AnimEffect::with_from("wipe", Direction::Left),
        duration_ms: 500,
        easing: Easing::Linear,
        sequence: Default::default(),
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
        effect: AnimEffect::with_from("float", Direction::Bottom),
        duration_ms: 500,
        easing: Easing::Linear,
        sequence: Default::default(),
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
        effect: AnimEffect::new("fade"),
        duration_ms: 500,
        easing: Easing::Linear,
        sequence: Default::default(),
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
        effect: AnimEffect::new("fade"),
        duration_ms: 500,
        easing: Easing::Linear,
        sequence: Default::default(),
    });
    // Add trigger on AnimationCompletes
    s.triggers.push(Trigger {
        action: Action::SetVariable {
            variable: "completed_count".into(),
            op: VarOp::Add,
            value: Some(Value::Number(1.0).into()),
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
        effect: AnimEffect::new("fade"),
        duration_ms: 500,
        easing: Easing::Linear,
        sequence: Default::default(),
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
            value: Some(Value::Number(1.0).into()),
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
                        outline: None,
                        point_text: false,
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
                    emphasis: Vec::new(),
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
                    locked: false,
                    editor_hidden: false,
                }],
                triggers: Vec::new(),
                hide_other_layers: true,
                pause_base_timeline: false,
                prevent_base_clicks: false,
                hide_base_objects: false,
                hide_when_timeline_ends: false,
                allow_seek: coursecraft_course::SeekMode::Auto,
                revisit: coursecraft_course::RevisitMode::Auto,
                cue_points: Vec::new(),
                groups: Vec::new(),
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
            cue_points: Vec::new(),
            groups: Vec::new(),
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
                    cue: None,
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
            ..Default::default()
        }],
        initial_state: "normal".into(),
        entrance: None,
        exit: None,
        motion_paths: Vec::new(),
        emphasis: Vec::new(),
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
        locked: false,
        editor_hidden: false,
    });
    slide.triggers.push(Trigger {
        action: Action::SetVariable {
            variable: "seen".into(),
            op: VarOp::Assign,
            value: Some(Value::Bool(true).into()),
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
            value: Some(Value::Bool(false).into()),
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
            emphasis: Vec::new(),
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
            locked: false,
            editor_hidden: false,
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

/// Every per-slide override must reach `effective_player()`, the one view of
/// the settings that the runtime and every host read.
#[test]
fn effective_player_applies_every_slide_override() {
    use coursecraft_course::{NextRequirement, PlayerOverrideField, SlidePlayerOverrides};
    let mut course = nav_course(serde_json::json!({}));
    course.slide_mut("s1").unwrap().player = Some(SlidePlayerOverrides {
        show_bar: Some(false),
        next_requires: Some(NextRequirement::TimelineEnds),
        show_nav: Some(false),
        show_seekbar: Some(false),
        show_menu: Some(false),
    });
    let mut p = Player::new(&course).unwrap();
    p.handle(&Input::Start);
    let settings = p.effective_player();
    assert!(!settings.show_bar);
    assert!(!settings.show_nav);
    assert!(!settings.show_seekbar);
    assert!(!settings.show_menu);
    assert_eq!(settings.next_requires, NextRequirement::TimelineEnds);
    let differing = course.slides_overriding(PlayerOverrideField::NextRequires);
    assert_eq!(differing.len(), 1);
    for field in PlayerOverrideField::ALL {
        assert_eq!(course.slides_overriding(field).len(), 1, "{field:?}");
    }
}

#[test]
fn bar_off_refuses_learner_next_previous_and_menu_but_not_author_jumps() {
    let mut course = nav_course(serde_json::json!({}));
    for id in ["s1", "s2", "s3"] {
        course.slide_mut(id).unwrap().player = Some(coursecraft_course::SlidePlayerOverrides {
            show_bar: Some(false),
            ..Default::default()
        });
    }
    let mut p = Player::new(&course).unwrap();
    p.handle(&Input::Start);
    assert!(!has(&p.handle(&Input::Next), &slide_entered("s2")));
    assert!(!has(
        &p.handle(&Input::GoTo { slide: "s3".into() }),
        &slide_entered("s3")
    ));
    assert!(has(
        &p.handle(&Input::Jump { slide: "s2".into() }),
        &slide_entered("s2")
    ));
    assert!(!has(&p.handle(&Input::Previous), &slide_entered("s1")));
}

/// `player.navigation` governs the slide menu only. With the course on Free
/// and no Next rule, Next works at once; a slide's own Next rule still wins
/// and the course reports that slide as overriding it.
#[test]
fn free_navigation_leaves_next_to_the_next_rule() {
    let mut course = nav_course(serde_json::json!({ "navigation": "free" }));
    let mut p = Player::new(&course).unwrap();
    p.handle(&Input::Start);
    assert!(p.can_next());
    assert!(has(&p.handle(&Input::Next), &slide_entered("s2")));

    course.slide_mut("s1").unwrap().player = Some(coursecraft_course::SlidePlayerOverrides {
        next_requires: Some(coursecraft_course::NextRequirement::TimelineEnds),
        ..Default::default()
    });
    let mut p = Player::new(&course).unwrap();
    p.handle(&Input::Start);
    assert!(!p.can_next());
    let overriding =
        course.slides_overriding(coursecraft_course::PlayerOverrideField::NextRequires);
    assert_eq!(
        overriding.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
        ["s1"]
    );
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

#[test]
fn free_seekbar_moves_the_clock() {
    let course = nav_course(serde_json::json!({}));
    let mut p = Player::new(&course).unwrap();
    p.handle(&Input::Start);
    assert!(p.seek_allowed());
    assert!(p.seek_info().seek_allowed);
    let fx = p.handle(&Input::PlayerSeek { ms: 1500 });
    assert!(!warned(&fx));
    assert_eq!(p.slide_elapsed_ms(), 1500);
}

#[test]
fn locked_seekbar_refuses_every_seek() {
    let course = nav_course(serde_json::json!({ "seekbar": "locked" }));
    let mut p = Player::new(&course).unwrap();
    p.handle(&Input::Start);
    assert!(p.seekbar_locked());
    assert!(!p.seek_allowed());
    assert!(!p.seek_info().seek_allowed);
    assert!(warned(&p.handle(&Input::PlayerSeek { ms: 1500 })));
    assert_eq!(p.slide_elapsed_ms(), 0);
    // Still locked after the timeline has played through.
    p.handle(&Input::Tick { ms: 2000 });
    assert!(!p.seek_allowed());
    assert!(warned(&p.handle(&Input::PlayerSeek { ms: 0 })));
    assert_eq!(p.slide_elapsed_ms(), 2000);
}

#[test]
fn seekbar_unlocks_when_the_timeline_ends_and_stays_unlocked_on_return() {
    let course = nav_course(serde_json::json!({ "seekbar": "until_timeline_ends" }));
    let mut p = Player::new(&course).unwrap();
    p.handle(&Input::Start);
    assert!(!p.seek_allowed());
    assert!(warned(&p.handle(&Input::PlayerSeek { ms: 1999 })));
    assert_eq!(p.slide_elapsed_ms(), 0);
    p.handle(&Input::Tick { ms: 1999 });
    assert!(!p.seek_info().seek_allowed);
    p.handle(&Input::Tick { ms: 1 });
    assert!(p.seek_info().seek_allowed);
    // Seeking back keeps it unlocked: the slide is completed.
    assert!(!warned(&p.handle(&Input::PlayerSeek { ms: 500 })));
    assert_eq!(p.slide_elapsed_ms(), 500);
    assert!(p.seek_allowed());
    // A new slide starts locked; the completed one is unlocked on return.
    p.handle(&Input::Next);
    assert!(!p.seek_allowed());
    p.handle(&Input::Previous);
    assert_eq!(p.current_slide().unwrap().id, "s1");
    assert!(p.seek_allowed());
}

#[test]
fn b11_free_seekbar_forward_seek_to_end_records_timeline_completion() {
    let course = nav_course(serde_json::json!({
        "seekbar": "free",
        "next_requires": { "kind": "timeline_ends" }
    }));
    let mut p = Player::new(&course).unwrap();
    p.handle(&Input::Start);
    assert!(!p.can_next());

    let fx_end = p.handle(&Input::PlayerSeek { ms: 2000 });
    assert!(!warned(&fx_end));
    assert!(p.can_next());

    let fx_back = p.handle(&Input::PlayerSeek { ms: 500 });
    assert!(!warned(&fx_back));
    assert!(
        p.can_next(),
        "scrubbing back after seeking to the end must keep Next unlocked"
    );

    let fx_next = p.handle(&Input::Next);
    assert!(!warned(&fx_next));
    assert_eq!(p.current_slide().unwrap().id, "s2");
}

#[test]
fn b11_locked_and_until_timeline_ends_refuse_forward_seek_to_end() {
    for seekbar in ["locked", "until_timeline_ends"] {
        let course = nav_course(serde_json::json!({
            "seekbar": seekbar,
            "next_requires": { "kind": "timeline_ends" }
        }));
        let mut p = Player::new(&course).unwrap();
        p.handle(&Input::Start);
        assert!(!p.can_next());

        let fx = p.handle(&Input::PlayerSeek { ms: 2000 });
        assert!(warned(&fx), "{seekbar} seekbar must refuse seek to end");
        assert_eq!(p.slide_elapsed_ms(), 0);
        assert!(!p.can_next(), "{seekbar} seekbar must keep Next locked");
    }
}

#[test]
fn seekbar_setting_round_trips_and_defaults_to_free() {
    let course = nav_course(serde_json::json!({}));
    assert_eq!(
        course.player.seekbar,
        coursecraft_course::SeekbarBehaviour::Free
    );
    assert!(!course.to_json().unwrap().contains("\"seekbar\""));
    let locked = nav_course(serde_json::json!({ "seekbar": "until_timeline_ends" }));
    let again = Course::from_json(&locked.to_json().unwrap()).unwrap();
    assert_eq!(
        again.player.seekbar,
        coursecraft_course::SeekbarBehaviour::UntilTimelineEnds
    );
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

// ---------------------------------------------------------------- inserted interactions

/// Build a one-slide course from the same object, state, layer, variable and
/// trigger shapes `interaction.insert` writes. `coursecraft-logic` cannot depend
/// on the engine, so the play-through tests describe the runtime contract here.
fn interaction_course(slide: serde_json::Value, variables: serde_json::Value) -> Course {
    let course = serde_json::json!({
        "schema_version": 1,
        "id": "interaction-test",
        "title": "Interaction",
        "slide_size": { "w": 1280, "h": 720 },
        "variables": variables,
        "scenes": [{
            "id": "main",
            "title": "Main",
            "slides": [slide]
        }]
    });
    let text = serde_json::to_string(&course).expect("course json");
    Course::from_json(&text).expect("valid course")
}

#[test]
fn flip_cards_toggle_between_front_and_back_on_click() {
    let mut triggers = Vec::new();
    for i in 1..=3 {
        let card = format!("card{i}");
        let variable = format!("flip{i}");
        triggers.push(serde_json::json!({
            "action": { "action": "set_variable", "variable": variable, "op": "toggle" },
            "when": { "event": "click", "object": card }
        }));
        for (value, state) in [(true, "Back"), (false, "normal")] {
            triggers.push(serde_json::json!({
                "action": { "action": "change_state", "object": card, "state": state },
                "when": { "event": "variable_changes", "variable": variable },
                "condition": { "compare": { "variable": variable, "op": "eq", "value": value } }
            }));
        }
    }
    let objects: Vec<_> = (1..=3)
        .map(|i| {
            serde_json::json!({
                "id": format!("card{i}"),
                "name": format!("Card {i}"),
                "kind": { "type": "shape", "shape": "rounded_rectangle", "fill": "#3366CC" },
                "frame": { "x": 60 + (i - 1) * 400, "y": 200, "w": 360, "h": 300 },
                "states": [{ "name": "Back", "text": "Add the detail here." }]
            })
        })
        .collect();
    let slide = serde_json::json!({
        "id": "s1",
        "title": "Cards",
        "objects": objects,
        "triggers": triggers
    });
    let course = interaction_course(
        slide,
        serde_json::json!([
            { "name": "flip1", "value": false },
            { "name": "flip2", "value": false },
            { "name": "flip3", "value": false }
        ]),
    );
    let mut p = Player::new(&course).expect("player");
    p.handle(&Input::Start);

    assert_eq!(p.object_state("card2"), Some("normal"));
    p.handle(&click("card2"));
    assert_eq!(p.object_state("card2"), Some("Back"), "the card turns over");
    p.handle(&click("card2"));
    assert_eq!(
        p.object_state("card2"),
        Some("normal"),
        "it flips back again"
    );
    assert_eq!(
        p.object_state("card1"),
        Some("normal"),
        "other cards are untouched"
    );
}

#[test]
fn hotspot_reveal_shows_one_callout_and_its_close_button_hides_it() {
    let mut objects = vec![serde_json::json!({
        "id": "pic",
        "kind": { "type": "shape", "shape": "rounded_rectangle", "fill": "#EEEEEE" },
        "frame": { "x": 80, "y": 100, "w": 1120, "h": 520 }
    })];
    for i in 1..=3 {
        objects.push(serde_json::json!({
            "id": format!("m{i}"),
            "name": format!("Hotspot {i} marker"),
            "kind": { "type": "shape", "shape": "ellipse", "fill": "#3366CC" },
            "frame": { "x": 200 + i * 260, "y": 260, "w": 48, "h": 48 },
            "states": [{ "name": "Visited", "fill": "#224488" }]
        }));
    }
    let layers = (1..=3)
        .map(|i| {
            serde_json::json!({
                "id": format!("h{i}"),
                "name": format!("Hotspot {i}"),
                "hide_other_layers": true,
                "objects": [{
                    "id": format!("c{i}"),
                    "name": format!("Close hotspot {i}"),
                    "kind": { "type": "button", "label": "×" },
                    "frame": { "x": 900, "y": 160, "w": 40, "h": 40 }
                }],
                "triggers": [{
                    "action": { "action": "hide_layer" },
                    "when": { "event": "click", "object": format!("c{i}") }
                }]
            })
        })
        .collect::<Vec<_>>();
    let mut triggers = Vec::new();
    for i in 1..=3 {
        let marker = format!("m{i}");
        triggers.push(serde_json::json!({
            "action": { "action": "show_layer", "layer": format!("h{i}") },
            "when": { "event": "click", "object": marker }
        }));
        triggers.push(serde_json::json!({
            "action": { "action": "change_state", "object": marker, "state": "Visited" },
            "when": { "event": "click", "object": marker }
        }));
    }
    let course = interaction_course(
        serde_json::json!({
            "id": "s1",
            "title": "Hotspots",
            "objects": objects,
            "layers": layers,
            "triggers": triggers
        }),
        serde_json::json!([]),
    );
    let mut p = Player::new(&course).expect("player");
    p.handle(&Input::Start);
    assert!(p.state().visible_layers.is_empty());

    p.handle(&click("m2"));
    assert_eq!(
        p.state().visible_layers,
        vec!["h2"],
        "only callout 2 is visible"
    );
    assert_eq!(
        p.object_state("m2"),
        Some("Visited"),
        "the marker remembers the visit"
    );

    p.handle(&click("c2"));
    assert!(
        p.state().visible_layers.is_empty(),
        "the close button hides the callout"
    );
}

#[test]
fn process_steps_share_one_variable_for_clicks_and_buttons() {
    let variable = "process_step1";
    let mut objects: Vec<_> = (1..=4)
        .map(|i| {
            serde_json::json!({
                "id": format!("c{i}"),
                "name": format!("Step {i} circle"),
                "kind": { "type": "button", "label": format!("{i}") },
                "frame": { "x": 100 + (i - 1) * 260, "y": 140, "w": 60, "h": 60 },
                "states": [{ "name": "Selected", "fill": "#3366CC" }]
            })
        })
        .collect();
    objects.push(serde_json::json!({
        "id": "back", "name": "Back",
        "kind": { "type": "button", "label": "Back" },
        "frame": { "x": 100, "y": 600, "w": 120, "h": 48 }
    }));
    objects.push(serde_json::json!({
        "id": "next", "name": "Next",
        "kind": { "type": "button", "label": "Next" },
        "frame": { "x": 1060, "y": 600, "w": 120, "h": 48 }
    }));
    let layers: Vec<_> = (1..=4)
        .map(|i| {
            serde_json::json!({
                "id": format!("p{i}"),
                "name": format!("Step {i}"),
                "hide_other_layers": true,
                "objects": [{
                    "id": format!("panel{i}"),
                    "kind": { "type": "shape", "shape": "rounded_rectangle", "fill": "#EEEEEE" },
                    "frame": { "x": 100, "y": 240, "w": 1080, "h": 300 }
                }]
            })
        })
        .collect();
    let mut triggers = Vec::new();
    for i in 1..=4 {
        triggers.push(serde_json::json!({
            "action": { "action": "set_variable", "variable": variable, "op": "assign", "value": i },
            "when": { "event": "click", "object": format!("c{i}") }
        }));
    }
    triggers.push(serde_json::json!({
        "action": { "action": "set_variable", "variable": variable, "op": "add", "value": 1 },
        "when": { "event": "click", "object": "next" },
        "condition": { "compare": { "variable": variable, "op": "lt", "value": 4 } }
    }));
    triggers.push(serde_json::json!({
        "action": { "action": "set_variable", "variable": variable, "op": "subtract", "value": 1 },
        "when": { "event": "click", "object": "back" },
        "condition": { "compare": { "variable": variable, "op": "gt", "value": 1 } }
    }));
    for i in 1..=4 {
        let condition = serde_json::json!({
            "compare": { "variable": variable, "op": "eq", "value": i }
        });
        triggers.push(serde_json::json!({
            "action": { "action": "show_layer", "layer": format!("p{i}") },
            "when": { "event": "variable_changes", "variable": variable },
            "condition": condition
        }));
        for j in 1..=4 {
            triggers.push(serde_json::json!({
                "action": {
                    "action": "change_state",
                    "object": format!("c{j}"),
                    "state": if i == j { "Selected" } else { "normal" }
                },
                "when": { "event": "variable_changes", "variable": variable },
                "condition": { "compare": { "variable": variable, "op": "eq", "value": i } }
            }));
        }
    }
    triggers.push(serde_json::json!({
        "action": { "action": "show_layer", "layer": "p1" },
        "when": { "event": "timeline_starts" }
    }));
    triggers.push(serde_json::json!({
        "action": { "action": "change_state", "object": "c1", "state": "Selected" },
        "when": { "event": "timeline_starts" }
    }));
    let course = interaction_course(
        serde_json::json!({
            "id": "s1",
            "title": "Steps",
            "objects": objects,
            "layers": layers,
            "triggers": triggers
        }),
        serde_json::json!([{ "name": variable, "value": 1 }]),
    );
    let mut p = Player::new(&course).expect("player");
    p.handle(&Input::Start);
    assert_eq!(
        p.state().visible_layers,
        vec!["p1"],
        "step 1 opens on start"
    );
    assert_eq!(p.object_state("c1"), Some("Selected"));

    p.handle(&click("next"));
    assert_eq!(p.variable(variable), Some(&Value::Number(2.0)));
    assert_eq!(p.state().visible_layers, vec!["p2"]);
    assert_eq!(
        p.object_state("c2"),
        Some("Selected"),
        "circle 2 is highlighted"
    );
    assert_eq!(p.object_state("c1"), Some("normal"));

    p.handle(&click("c4"));
    assert_eq!(
        p.state().visible_layers,
        vec!["p4"],
        "clicking circle 4 jumps there"
    );
    assert_eq!(p.object_state("c4"), Some("Selected"));

    p.handle(&click("next"));
    assert_eq!(
        p.variable(variable),
        Some(&Value::Number(4.0)),
        "Next stops at 4"
    );
    assert_eq!(p.state().visible_layers, vec!["p4"]);

    for _ in 0..3 {
        p.handle(&click("back"));
    }
    assert_eq!(p.variable(variable), Some(&Value::Number(1.0)));
    p.handle(&click("back"));
    assert_eq!(
        p.variable(variable),
        Some(&Value::Number(1.0)),
        "Back stops at 1"
    );
    assert_eq!(p.state().visible_layers, vec!["p1"]);
}

#[test]
fn timeline_points_show_one_detail_and_stay_visited() {
    let objects: Vec<_> = (1..=5)
        .map(|i| {
            serde_json::json!({
                "id": format!("p{i}"),
                "name": format!("Point {i}"),
                "kind": { "type": "shape", "shape": "ellipse", "fill": "#3366CC" },
                "frame": { "x": 100 + (i - 1) * 220, "y": 200, "w": 48, "h": 48 },
                "states": [{ "name": "Visited", "fill": "#224488" }]
            })
        })
        .collect();
    let layers: Vec<_> = (1..=5)
        .map(|i| {
            serde_json::json!({
                "id": format!("t{i}"),
                "name": format!("Point {i}"),
                "hide_other_layers": true,
                "objects": [{
                    "id": format!("d{i}"),
                    "kind": { "type": "shape", "shape": "rounded_rectangle", "fill": "#EEEEEE" },
                    "frame": { "x": 100, "y": 300, "w": 1000, "h": 300 }
                }]
            })
        })
        .collect();
    let mut triggers = Vec::new();
    for i in 1..=5 {
        triggers.push(serde_json::json!({
            "action": { "action": "show_layer", "layer": format!("t{i}") },
            "when": { "event": "click", "object": format!("p{i}") }
        }));
        triggers.push(serde_json::json!({
            "action": { "action": "change_state", "object": format!("p{i}"), "state": "Visited" },
            "when": { "event": "click", "object": format!("p{i}") }
        }));
    }
    let course = interaction_course(
        serde_json::json!({
            "id": "s1",
            "title": "Timeline",
            "objects": objects,
            "layers": layers,
            "triggers": triggers
        }),
        serde_json::json!([]),
    );
    let mut p = Player::new(&course).expect("player");
    p.handle(&Input::Start);
    assert!(
        p.state().visible_layers.is_empty(),
        "no detail layer shows on start"
    );

    p.handle(&click("p3"));
    assert_eq!(p.state().visible_layers, vec!["t3"], "only point 3 opens");
    assert_eq!(p.object_state("p3"), Some("Visited"));
    assert_eq!(p.object_state("p1"), Some("normal"));

    p.handle(&click("p1"));
    assert_eq!(p.state().visible_layers, vec!["t1"], "point 1 opens alone");
    assert_eq!(p.object_state("p1"), Some("Visited"));
    assert_eq!(
        p.object_state("p3"),
        Some("Visited"),
        "point 3 stays visited"
    );
}

#[test]
fn reveal_grid_counts_each_tile_once_and_resolves_the_counter_text() {
    let variable = "grid_explored1";
    let mut variables = serde_json::json!([{ "name": variable, "value": 0 }]);
    let variables_array = variables.as_array_mut().expect("variables array");
    for i in 1..=6 {
        variables_array.push(serde_json::json!({ "name": format!("tile{i}"), "value": false }));
    }

    let mut objects: Vec<_> = (1..=6)
        .map(|i| {
            serde_json::json!({
                "id": format!("tile{i}"),
                "name": format!("Tile {i}"),
                "kind": { "type": "shape", "shape": "rounded_rectangle", "fill": "#EEEEEE" },
                "frame": { "x": 80 + (i - 1) % 3 * 360, "y": 120 + (i - 1) / 3 * 240, "w": 340, "h": 220 },
                "states": [{ "name": "Revealed", "text": "Detail" }]
            })
        })
        .collect();
    objects.push(serde_json::json!({
        "id": "counter",
        "name": "Explored counter",
        "kind": { "type": "text", "text": format!("%{variable}% of 6 explored") },
        "frame": { "x": 80, "y": 620, "w": 1100, "h": 40 }
    }));

    let mut triggers = Vec::new();
    for i in 1..=6 {
        let flag = format!("tile{i}");
        triggers.push(serde_json::json!({
            "action": { "action": "set_variable", "variable": flag, "op": "assign", "value": true },
            "when": { "event": "click", "object": format!("tile{i}") },
            "condition": { "compare": { "variable": flag, "op": "eq", "value": false } }
        }));
        triggers.push(serde_json::json!({
            "action": { "action": "change_state", "object": format!("tile{i}"), "state": "Revealed" },
            "when": { "event": "variable_changes", "variable": flag },
            "condition": { "compare": { "variable": flag, "op": "eq", "value": true } }
        }));
        triggers.push(serde_json::json!({
            "action": { "action": "set_variable", "variable": variable, "op": "add", "value": 1 },
            "when": { "event": "variable_changes", "variable": flag },
            "condition": { "compare": { "variable": flag, "op": "eq", "value": true } }
        }));
        triggers.push(serde_json::json!({
            "action": { "action": "set_variable", "variable": flag, "op": "assign", "value": false },
            "when": { "event": "timeline_starts" }
        }));
        triggers.push(serde_json::json!({
            "action": { "action": "change_state", "object": format!("tile{i}"), "state": "normal" },
            "when": { "event": "timeline_starts" }
        }));
    }
    triggers.push(serde_json::json!({
        "action": { "action": "set_variable", "variable": variable, "op": "assign", "value": 0 },
        "when": { "event": "timeline_starts" }
    }));

    let course = interaction_course(
        serde_json::json!({
            "id": "s1",
            "title": "Reveal grid",
            "objects": objects,
            "triggers": triggers
        }),
        variables,
    );
    let mut p = Player::new(&course).expect("player");
    p.handle(&Input::Start);
    assert_eq!(p.variable(variable), Some(&Value::Number(0.0)));
    assert_eq!(p.object_state("tile2"), Some("normal"));

    p.handle(&click("tile2"));
    assert_eq!(p.object_state("tile2"), Some("Revealed"), "tile 2 opens");
    assert_eq!(p.variable(variable), Some(&Value::Number(1.0)));
    p.handle(&click("tile2"));
    assert_eq!(
        p.variable(variable),
        Some(&Value::Number(1.0)),
        "clicking a revealed tile does not count twice"
    );
    p.handle(&click("tile4"));
    p.handle(&click("tile5"));
    assert_eq!(p.variable(variable), Some(&Value::Number(3.0)));
    assert_eq!(
        p.display_text(&format!("%{variable}% of 6 explored")),
        "3 of 6 explored"
    );
}

#[test]
fn scenario_choices_show_feedback_and_try_again_hides_it() {
    let objects: Vec<_> = (1..=3)
        .map(|i| {
            serde_json::json!({
                "id": format!("b{i}"),
                "name": format!("Choice {i}"),
                "kind": { "type": "button", "label": format!("Choice {i}") },
                "frame": { "x": 100, "y": 200 + (i - 1) * 80, "w": 1000, "h": 60 }
            })
        })
        .collect();
    let layers: Vec<_> = (1..=3)
        .map(|i| {
            let mut layer_objects = vec![serde_json::json!({
                "id": format!("heading{i}"),
                "name": format!("Choice {i} heading"),
                "kind": { "type": "text", "text": if i == 2 { "Good choice" } else { "Think again" } },
                "frame": { "x": 200, "y": 260, "w": 800, "h": 300 }
            })];
            let mut layer_triggers = Vec::new();
            if i != 2 {
                layer_objects.push(serde_json::json!({
                    "id": format!("retry{i}"),
                    "name": format!("Try again {i}"),
                    "kind": { "type": "button", "label": "Try again" },
                    "frame": { "x": 200, "y": 560, "w": 120, "h": 40 }
                }));
                layer_triggers.push(serde_json::json!({
                    "action": { "action": "hide_layer" },
                    "when": { "event": "click", "object": format!("retry{i}") }
                }));
            }
            serde_json::json!({
                "id": format!("ch{i}"),
                "name": format!("Choice {i}"),
                "hide_other_layers": true,
                "objects": layer_objects,
                "triggers": layer_triggers
            })
        })
        .collect();
    let triggers: Vec<_> = (1..=3)
        .map(|i| {
            serde_json::json!({
                "action": { "action": "show_layer", "layer": format!("ch{i}") },
                "when": { "event": "click", "object": format!("b{i}") }
            })
        })
        .collect();
    let course = interaction_course(
        serde_json::json!({
            "id": "s1",
            "title": "Scenario",
            "objects": objects,
            "layers": layers,
            "triggers": triggers
        }),
        serde_json::json!([]),
    );
    let mut p = Player::new(&course).expect("player");
    p.handle(&Input::Start);
    assert!(p.state().visible_layers.is_empty());

    p.handle(&click("b1"));
    assert_eq!(p.state().visible_layers, vec!["ch1"]);
    p.handle(&click("retry1"));
    assert!(
        p.state().visible_layers.is_empty(),
        "Try again closes the feedback"
    );

    p.handle(&click("b2"));
    assert_eq!(p.state().visible_layers, vec!["ch2"]);
    p.handle(&click("b1"));
    assert_eq!(
        p.state().visible_layers,
        vec!["ch1"],
        "choosing again hides the previous feedback"
    );
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

#[test]
fn timed_animation_follows_entrance_timing_and_exit() {
    use coursecraft_course::{AnimEffect, Animation, Direction, Easing, Object, Size};
    let json = serde_json::json!({
        "id": "o", "kind": { "type": "text", "text": "Hi" },
        "frame": { "x": 100.0, "y": 100.0, "w": 200.0, "h": 50.0 },
        "timing": { "start_ms": 1000, "end_ms": 4000 }
    });
    let Ok(mut obj) = serde_json::from_value::<Object>(json) else {
        panic!("valid object");
    };
    obj.entrance = Some(Animation {
        effect: AnimEffect::with_from("fly_in", Direction::Left),
        duration_ms: 1000,
        easing: Easing::Linear,
        sequence: Default::default(),
    });
    obj.exit = Some(Animation {
        effect: AnimEffect::new("fade"),
        duration_ms: 500,
        easing: Easing::Linear,
        sequence: Default::default(),
    });
    let size = Size { w: 1280, h: 720 };
    let sample = |ms| crate::sample_timed_animation(&obj, ms, 10_000, size);
    assert!(sample(999).is_none(), "hidden before it starts");
    let start = sample(1000).map(|s| s.offset.0);
    assert_eq!(start, Some(-300.0), "starts fully off the left edge");
    let mid = sample(1500).map(|s| s.offset.0);
    assert_eq!(mid, Some(-150.0), "halfway in at half the duration");
    assert_eq!(sample(2500), Some(crate::ObjectSample::IDENTITY), "at rest");
    let fading = sample(3750).map(|s| s.opacity);
    assert_eq!(fading, Some(0.5), "half faded halfway through the exit");
    assert!(sample(4000).is_none(), "gone at its end");
}

fn hello_with_try_again() -> Course {
    use coursecraft_course::{
        Action, Color, Event, Layer, Object, ObjectKind, Rect, RevisitMode, SeekMode, Timing,
        Trigger,
    };
    let mut c = Course::from_json(HELLO).expect("hello");
    let slide = c.slide_mut("s-quiz").expect("quiz slide");
    slide.layers.push(Layer {
        id: "l-try-again".into(),
        name: "Try Again".into(),
        duration_ms: 5000,
        objects: vec![Object {
            id: "btn-try-again-layer".into(),
            name: "Try Again".into(),
            kind: ObjectKind::Button {
                label: "Try Again".into(),
                fill: Color("#3B82F6".into()),
                fill_style: None,
            },
            frame: Rect {
                x: 80.0,
                y: 420.0,
                w: 160.0,
                h: 44.0,
            },
            timing: Timing::default(),
            states: Vec::new(),
            initial_state: "normal".into(),
            entrance: None,
            exit: None,
            motion_paths: Vec::new(),
            emphasis: Vec::new(),
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
            locked: false,
            editor_hidden: false,
        }],
        triggers: vec![Trigger {
            when: Event::Click {
                object: "btn-try-again-layer".into(),
            },
            action: Action::HideLayer { layer: None },
            condition: None,
            disabled: false,
        }],
        hide_other_layers: true,
        pause_base_timeline: false,
        prevent_base_clicks: true,
        hide_base_objects: false,
        hide_when_timeline_ends: false,
        allow_seek: SeekMode::Auto,
        revisit: RevisitMode::Auto,
        cue_points: Vec::new(),
        groups: Vec::new(),
    });
    let q = slide.question.as_mut().expect("question");
    q.attempts = 2;
    q.try_again_layer = Some("l-try-again".into());
    c
}

#[test]
fn try_again_layer_wrong_then_right_clears_choice_and_shows_correct() {
    let c = hello_with_try_again();
    let mut p = open_quiz(&c);

    // Attempt 1: wrong answer -> shows try-again layer, does not lock.
    p.handle(&Input::SelectChoice { choice: "a".into() });
    let eff = p.handle(&click("btn-submit"));
    assert!(has(
        &eff,
        &Effect::LayerShown {
            layer: "l-try-again".into()
        }
    ));
    assert!(!eff.iter().any(|e| matches!(
        e,
        Effect::LayerShown { layer } if layer == "incorrect" || layer == "correct"
    )));
    assert!(!p.question_locked());

    // Clicking Try Again closes the layer and clears the learner's choice.
    let eff = p.handle(&click("btn-try-again-layer"));
    assert!(has(
        &eff,
        &Effect::LayerHidden {
            layer: "l-try-again".into()
        }
    ));
    assert!(p.state().selected.is_empty());

    // Attempt 2: right answer -> shows correct layer and locks.
    p.handle(&Input::SelectChoice { choice: "b".into() });
    let eff = p.handle(&click("btn-submit"));
    assert!(has(
        &eff,
        &Effect::LayerShown {
            layer: "correct".into()
        }
    ));
    assert!(p.question_locked());
}

#[test]
fn try_again_layer_wrong_twice_shows_try_again_then_incorrect() {
    let c = hello_with_try_again();
    let mut p = open_quiz(&c);

    // Attempt 1: wrong -> try-again.
    p.handle(&Input::SelectChoice { choice: "a".into() });
    let eff = p.handle(&click("btn-submit"));
    assert!(has(
        &eff,
        &Effect::LayerShown {
            layer: "l-try-again".into()
        }
    ));
    assert!(!p.question_locked());

    p.handle(&click("btn-try-again-layer"));
    assert!(p.state().selected.is_empty());

    // Attempt 2 (last): wrong -> incorrect layer and locked.
    p.handle(&Input::SelectChoice { choice: "c".into() });
    let eff = p.handle(&click("btn-submit"));
    assert!(has(
        &eff,
        &Effect::LayerShown {
            layer: "incorrect".into()
        }
    ));
    assert!(!eff.iter().any(|e| matches!(
        e,
        Effect::LayerShown { layer } if layer == "l-try-again"
    )));
    assert!(p.question_locked());
}

#[test]
fn safe_resume_re_shows_outcome_layer_on_answered_locked_question() {
    // 1. Answered correctly -> suspend & resume shows correct_layer again.
    let c = hello_with_try_again();
    let mut p = open_quiz(&c);
    p.handle(&Input::SelectChoice { choice: "b".into() });
    p.handle(&click("btn-submit"));
    assert!(p.question_locked());
    let data = crate::encode_suspend(&c, p.state());

    let restored = crate::decode_suspend(&c, &data).expect("decode suspend");
    let mut resumed = Player::resume(&c, restored).expect("resume");
    let eff = resumed.handle(&Input::Start);
    assert!(has(
        &eff,
        &Effect::LayerShown {
            layer: "correct".into()
        }
    ));
    assert!(
        resumed
            .state()
            .visible_layers
            .contains(&"correct".to_owned())
    );

    // Navigating away and back also re-shows the outcome layer.
    resumed.handle(&Input::Previous);
    let eff = resumed.handle(&Input::Next);
    assert!(has(
        &eff,
        &Effect::LayerShown {
            layer: "correct".into()
        }
    ));

    // 2. Exhausted wrong attempts -> suspend & resume shows incorrect_layer.
    let mut p2 = open_quiz(&c);
    p2.handle(&Input::SelectChoice { choice: "a".into() });
    p2.handle(&click("btn-submit"));
    // After 1 wrong attempt of 2, question is NOT locked: resuming must NOT show outcome layer.
    let mid_data = crate::encode_suspend(&c, p2.state());
    let mid_restored = crate::decode_suspend(&c, &mid_data).expect("decode mid");
    let mut mid_resumed = Player::resume(&c, mid_restored).expect("resume mid");
    let eff = mid_resumed.handle(&Input::Start);
    assert!(
        !eff.iter().any(|e| matches!(e, Effect::LayerShown { .. })),
        "unlocked question should not show outcome layer on resume: {eff:?}"
    );

    // Finish second wrong attempt -> now locked.
    p2.handle(&click("btn-try-again-layer"));
    p2.handle(&Input::SelectChoice { choice: "c".into() });
    p2.handle(&click("btn-submit"));
    assert!(p2.question_locked());
    let fail_data = crate::encode_suspend(&c, p2.state());
    let fail_restored = crate::decode_suspend(&c, &fail_data).expect("decode fail");
    let mut fail_resumed = Player::resume(&c, fail_restored).expect("resume fail");
    let eff = fail_resumed.handle(&Input::Start);
    assert!(has(
        &eff,
        &Effect::LayerShown {
            layer: "incorrect".into()
        }
    ));
}

#[test]
fn variable_to_variable_set_and_compare_and_ignore_case_text_ops() {
    use coursecraft_course::{CmpOp, Condition, ValueSource};

    let mut c = course();
    c.variables.push(coursecraft_course::VariableDef {
        name: "Score".into(),
        value: Value::Number(10.0),
    });
    c.variables.push(coursecraft_course::VariableDef {
        name: "BestScore".into(),
        value: Value::Number(25.0),
    });
    c.variables.push(coursecraft_course::VariableDef {
        name: "Greeting".into(),
        value: Value::Text("Hello World".into()),
    });
    c.variables.push(coursecraft_course::VariableDef {
        name: "Target".into(),
        value: Value::Text("hello world".into()),
    });
    c.variables.push(coursecraft_course::VariableDef {
        name: "Matched".into(),
        value: Value::Bool(false),
    });

    // Click btn-start: Score = BestScore (25), then if Score == BestScore && Greeting eq_ignore_case Target && Greeting contains_ignore_case "WORLD", set Matched = true.
    let s = c.slide_mut("s-intro").unwrap();
    s.triggers.clear();
    s.triggers.push(Trigger {
        action: Action::SetVariable {
            variable: "Score".into(),
            op: VarOp::Assign,
            value: Some(ValueSource::Variable {
                variable: "BestScore".into(),
            }),
        },
        when: Event::Click {
            object: "btn-start".into(),
        },
        condition: None,
        disabled: false,
    });
    s.triggers.push(Trigger {
        action: Action::SetVariable {
            variable: "Matched".into(),
            op: VarOp::Assign,
            value: Some(Value::Bool(true).into()),
        },
        when: Event::Click {
            object: "btn-start".into(),
        },
        condition: Some(Condition::All(vec![
            Condition::Compare {
                variable: "Score".into(),
                op: CmpOp::Eq,
                value: ValueSource::Variable {
                    variable: "BestScore".into(),
                },
            },
            Condition::Compare {
                variable: "Greeting".into(),
                op: CmpOp::EqIgnoreCase,
                value: ValueSource::Variable {
                    variable: "Target".into(),
                },
            },
            Condition::Compare {
                variable: "Greeting".into(),
                op: CmpOp::ContainsIgnoreCase,
                value: Value::Text("WORLD".into()).into(),
            },
        ])),
        disabled: false,
    });

    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    p.handle(&Input::Tick { ms: 1000 });
    p.handle(&click("btn-start"));
    assert_eq!(p.variable("Score"), Some(&Value::Number(25.0)));
    assert_eq!(p.variable("Matched"), Some(&Value::Bool(true)));
}

#[test]
fn jump_to_scene_and_last_visited_history_and_resume() {
    use coursecraft_course::SlideTarget;

    let mut c = course();
    // Add a second scene "sc2" with two slides "sc2-a" and "sc2-b".
    let mut sc2_a = c.slide("s-intro").unwrap().clone();
    sc2_a.id = "sc2-a".into();
    sc2_a.triggers.clear();
    sc2_a.triggers.push(Trigger {
        action: Action::JumpToSlide {
            target: SlideTarget::LastVisited,
        },
        when: Event::Click {
            object: "btn-start".into(),
        },
        condition: None,
        disabled: false,
    });
    let mut sc2_b = sc2_a.clone();
    sc2_b.id = "sc2-b".into();
    c.scenes.push(coursecraft_course::Scene {
        id: "sc2".into(),
        title: "Scene 2".into(),
        slides: vec![sc2_a, sc2_b],
    });

    // On s-intro, clicking btn-start jumps to Scene("sc2").
    let s0 = c.slide_mut("s-intro").unwrap();
    s0.triggers.clear();
    s0.triggers.push(Trigger {
        action: Action::JumpToSlide {
            target: SlideTarget::Scene("sc2".into()),
        },
        when: Event::Click {
            object: "btn-start".into(),
        },
        condition: None,
        disabled: false,
    });

    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    p.handle(&Input::Tick { ms: 1000 });
    assert_eq!(p.current_slide().map(|s| s.id.as_str()), Some("s-intro"));

    // Jump from s-intro to sc2 -> lands on sc2-a, history has ["s-intro"].
    let fx = p.handle(&click("btn-start"));
    assert!(has(&fx, &slide_entered("sc2-a")));
    assert_eq!(p.state().visit_history, vec!["s-intro".to_owned()]);

    // Jump to LastVisited on sc2-a -> returns to s-intro, history is now empty.
    p.handle(&Input::Tick { ms: 1000 });
    let fx = p.handle(&click("btn-start"));
    assert!(has(&fx, &slide_entered("s-intro")));
    assert!(p.state().visit_history.is_empty());

    // Cap at 50 entries.
    for i in 0..60 {
        let target = if i % 2 == 0 { "s-tabs" } else { "s-quiz" };
        p.handle(&Input::Jump {
            slide: target.into(),
        });
    }
    assert_eq!(p.state().visit_history.len(), 50);

    // Suspend data does NOT carry visit_history; after resume, LastVisited acts like Previous.
    p.handle(&Input::Jump {
        slide: "sc2-b".into(),
    });
    p.handle(&Input::Tick { ms: 1000 });
    let suspend = crate::encode_suspend(&c, p.state());
    let decoded = crate::decode_suspend(&c, &suspend).expect("decode");
    assert!(decoded.visit_history.is_empty());

    let mut resumed = Player::resume(&c, decoded).expect("resume");
    resumed.handle(&Input::Start);
    resumed.handle(&Input::Tick { ms: 1000 });
    let fx = resumed.handle(&click("btn-start"));
    // Previous slide before sc2-b in play order is sc2-a.
    assert!(has(&fx, &slide_entered("sc2-a")));
}

fn lightbox_quiz_course() -> Course {
    let mut c = retry_review_course();
    // Add a button on s1 to open s-results as a lightbox, and make s1 revisit: resume.
    c.scenes[0].slides[0].revisit = RevisitMode::Resume;
    c.scenes[0].slides[0].objects.push(
        serde_json::from_value::<Object>(serde_json::json!({
            "id": "btn-lb",
            "kind": { "type": "button", "label": "Open Results" },
            "frame": { "x": 0.0, "y": 300.0, "w": 120.0, "h": 40.0 }
        }))
        .expect("btn-lb"),
    );
    c.scenes[0].slides[0].triggers.push(Trigger {
        action: Action::LightboxSlide {
            slide: "s-results".into(),
        },
        when: Event::Click {
            object: "btn-lb".into(),
        },
        condition: None,
        disabled: false,
    });
    c
}

#[test]
fn retry_quiz_closes_open_lightbox() {
    let c = lightbox_quiz_course();
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    answer_q1(&mut p);
    p.handle(&click("btn-lb"));
    assert!(p.state().lightbox.is_some());

    let fx = p.handle(&click("btn-retry"));
    assert!(p.state().lightbox.is_none(), "lightbox should be closed");
    assert!(
        fx.iter()
            .any(|e| matches!(e, Effect::LightboxClosed { slide } if slide == "s-results")),
        "expected LightboxClosed, got {fx:?}"
    );
    assert_eq!(p.current_slide().map(|s| s.id.as_str()), Some("s-q1"));
}

#[test]
fn review_quiz_and_exit_review_close_open_lightbox() {
    let c = lightbox_quiz_course();
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    answer_q1(&mut p);
    p.handle(&click("btn-lb"));
    assert!(p.state().lightbox.is_some());

    let fx = p.handle(&click("btn-review"));
    assert!(
        p.state().lightbox.is_none(),
        "lightbox should be closed on review"
    );
    assert!(
        fx.iter()
            .any(|e| matches!(e, Effect::LightboxClosed { slide } if slide == "s-results")),
        "expected LightboxClosed on review, got {fx:?}"
    );
    assert_eq!(p.current_slide().map(|s| s.id.as_str()), Some("s-q1"));

    // Open a lightbox while reviewing, then exit_review must close it too.
    let mut c2 = c.clone();
    let s_q1 = c2.slide_mut("s-q1").expect("s-q1");
    s_q1.objects.push(
        serde_json::from_value::<Object>(serde_json::json!({
            "id": "btn-lb-rev",
            "kind": { "type": "button", "label": "Open Results" },
            "frame": { "x": 0.0, "y": 400.0, "w": 120.0, "h": 40.0 }
        }))
        .expect("btn-lb-rev"),
    );
    s_q1.triggers.push(Trigger {
        action: Action::LightboxSlide {
            slide: "s-results".into(),
        },
        when: Event::Click {
            object: "btn-lb-rev".into(),
        },
        condition: None,
        disabled: false,
    });
    let mut p2 = Player::new(&c2).expect("player");
    p2.handle(&Input::Start);
    answer_q1(&mut p2);
    p2.handle(&Input::Jump {
        slide: "s-results".into(),
    });
    p2.handle(&click("btn-review"));
    p2.handle(&click("btn-lb-rev"));
    assert!(p2.state().lightbox.is_some());
    let fx2 = p2.handle(&Input::ExitReview);
    assert!(
        p2.state().lightbox.is_none(),
        "lightbox should be closed on exit_review"
    );
    assert!(
        fx2.iter()
            .any(|e| matches!(e, Effect::LightboxClosed { slide } if slide == "s-results")),
        "expected LightboxClosed on exit_review, got {fx2:?}"
    );
}

#[test]
fn resume_saves_the_resumed_slide_when_leaving_it() {
    let mut c = Course::from_json(HELLO).expect("hello");
    c.scenes[0].slides[0].revisit = RevisitMode::Resume;
    c.scenes[0].slides[0].duration_ms = 5000;
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    p.handle(&Input::Tick { ms: 1000 });
    let saved = p.state().clone();

    let mut resumed = Player::resume(&c, saved).expect("resume");
    resumed.handle(&Input::Start);
    resumed.handle(&Input::Tick { ms: 500 });
    resumed.handle(&Input::Jump {
        slide: c.scenes[0].slides[1].id.clone(),
    });
    resumed.handle(&Input::Jump {
        slide: c.scenes[0].slides[0].id.clone(),
    });
    assert_eq!(
        resumed.state().base.ms,
        1500,
        "resumed slide should save its clock when left"
    );
}

#[test]
fn go_to_respects_navigation_lock_while_lightbox_is_open() {
    let mut c = timer_course();
    c.player.navigation = NavMode::Locked;
    c.player.show_menu = true;
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    p.handle(&click("btn-open"));
    assert!(p.state().lightbox.is_some());

    let fx = p.handle(&Input::GoTo {
        slide: "s-box".into(),
    });
    assert!(warned(&fx), "expected warning when navigation is locked");
    assert!(p.state().lightbox.is_some(), "lightbox should stay open");
}

#[test]
fn retry_quiz_with_banks_runs_leave_steps_before_replacing_slides() {
    let mut c = course_with_draw(10, 1, "draw1");
    for s in &mut c.banks[0].slides {
        s.revisit = RevisitMode::Resume;
        s.triggers.push(Trigger {
            action: Action::RetryQuiz,
            when: Event::Click {
                object: "btn".into(),
            },
            condition: None,
            disabled: false,
        });
        s.objects.push(
            serde_json::from_value::<Object>(serde_json::json!({
                "id": "btn",
                "kind": { "type": "button", "label": "Retry" },
                "frame": { "x": 0.0, "y": 0.0, "w": 80.0, "h": 40.0 }
            }))
            .expect("btn"),
        );
    }
    let mut p = Player::with_draw_seed(&c, 1).expect("player");
    p.handle(&Input::Start);
    while p.current_slide().is_some_and(|s| s.question.is_none()) {
        p.handle(&Input::Next);
    }
    p.handle(&Input::Tick { ms: 1234 });
    let old_first = p.current_slide().expect("slide").id.clone();
    p.handle(&click("btn"));
    let new_first = p.current_slide().expect("slide").id.clone();
    assert_ne!(old_first, new_first);
    assert!(
        !p.state().saved_slides.contains_key(&new_first),
        "new slide at draw index must not inherit old slide's saved state"
    );
}

#[test]
fn check_intersections_skips_when_no_intersection_triggers_and_fires_when_present() {
    let mut c: Course = serde_json::from_value(serde_json::json!({
        "schema_version": 1,
        "id": "intersections-skip",
        "title": "Intersections Skip",
        "slide_size": { "w": 800, "h": 600 },
        "variables": [{ "name": "hit", "value": 0 }],
        "scenes": [{
            "id": "sc1",
            "title": "Scene 1",
            "slides": [{
                "id": "s1",
                "title": "S1",
                "objects": [
                    {
                        "id": "a",
                        "kind": { "type": "shape", "shape": "rectangle", "fill": "#ffffff" },
                        "frame": { "x": 10.0, "y": 10.0, "w": 100.0, "h": 100.0 }
                    },
                    {
                        "id": "b",
                        "kind": { "type": "shape", "shape": "rectangle", "fill": "#ffffff" },
                        "frame": { "x": 50.0, "y": 50.0, "w": 100.0, "h": 100.0 }
                    }
                ],
                "triggers": []
            }]
        }]
    }))
    .expect("course");

    // Without any intersection trigger, overlapping objects do not populate intersecting_pairs.
    let mut p_no_trig = Player::new(&c).expect("player");
    p_no_trig.handle(&Input::Start);
    assert!(p_no_trig.state().intersecting_pairs.is_empty());

    // With a disabled intersection trigger, still skipped.
    c.scenes[0].slides[0].triggers.push(
        serde_json::from_value(serde_json::json!({
            "when": { "event": "objects_intersect", "a": "a", "b": "b" },
            "action": { "action": "set_variable", "variable": "hit", "op": "assign", "value": 1 },
            "disabled": true
        }))
        .expect("trigger"),
    );
    let mut p_disabled = Player::new(&c).expect("player");
    p_disabled.handle(&Input::Start);
    assert!(p_disabled.state().intersecting_pairs.is_empty());
    assert_eq!(p_disabled.variable("hit"), Some(&Value::Number(0.0)));

    // Enable the trigger: intersection is detected on slide start.
    c.scenes[0].slides[0].triggers[0].disabled = false;
    let mut p_enabled = Player::new(&c).expect("player");
    p_enabled.handle(&Input::Start);
    assert!(!p_enabled.state().intersecting_pairs.is_empty());
    assert_eq!(p_enabled.variable("hit"), Some(&Value::Number(1.0)));
}

#[test]
fn resume_trusted_matches_full_resume_including_draw_seed_fallback() {
    let c = course_with_draw(4, 2, "draw1");
    let mut p = Player::with_draw_seed(&c, 7).expect("player");
    let plan = p.play_plan();
    p.handle(&Input::Start);
    p.handle(&Input::Tick { ms: 250 });
    p.handle(&Input::Next);
    let saved = p.state().clone();

    let mut full = Player::resume(&c, saved.clone()).expect("full resume");
    let mut trusted = Player::resume_trusted(&c, saved, &plan).expect("trusted resume");
    assert_eq!(
        full.current_slide().map(|s| s.id.as_str()),
        trusted.current_slide().map(|s| s.id.as_str())
    );
    let full_ids: Vec<&str> = full.play_order().iter().map(|s| s.id.as_str()).collect();
    let trusted_ids: Vec<&str> = trusted.play_order().iter().map(|s| s.id.as_str()).collect();
    assert_eq!(full_ids, trusted_ids);
    assert_eq!(
        full.handle(&Input::Tick { ms: 200 }),
        trusted.handle(&Input::Tick { ms: 200 })
    );

    // If draw_seed changed (e.g. after RetryQuiz), resume_trusted falls back to full resume.
    let mut other_seed_state = p.state().clone();
    other_seed_state.draw_seed = 99;
    let full_other = Player::resume(&c, other_seed_state.clone()).expect("full other");
    let trusted_other =
        Player::resume_trusted(&c, other_seed_state, &plan).expect("trusted fallback");
    let full_other_ids: Vec<&str> = full_other
        .play_order()
        .iter()
        .map(|s| s.id.as_str())
        .collect();
    let trusted_other_ids: Vec<&str> = trusted_other
        .play_order()
        .iter()
        .map(|s| s.id.as_str())
        .collect();
    assert_eq!(full_other_ids, trusted_other_ids);
}

#[test]
fn reach_points_buffer_reuse_fires_base_and_layer_reaches_in_order() {
    let mut c: Course = serde_json::from_value(serde_json::json!({
        "schema_version": 1,
        "id": "reach-buf",
        "title": "Reach Buf",
        "slide_size": { "w": 800, "h": 600 },
        "variables": [{ "name": "seq", "value": "" }],
        "scenes": [{
            "id": "sc1",
            "title": "Scene 1",
            "slides": [{
                "id": "s1",
                "title": "S1",
                "duration_ms": 2000,
                "cue_points": [{ "id": "cue1", "name": "C1", "at_ms": 300 }],
                "layers": [{
                    "id": "l1",
                    "name": "L1",
                    "duration_ms": 1000,
                    "triggers": [{
                        "when": { "event": "timeline_reaches", "ms": 400, "layer": "l1" },
                        "action": { "action": "set_variable", "variable": "seq", "op": "assign", "value": "layer400" }
                    }]
                }],
                "triggers": [
                    {
                        "when": { "event": "timeline_reaches", "ms": 500 },
                        "action": { "action": "set_variable", "variable": "seq", "op": "assign", "value": "base500" }
                    },
                    {
                        "when": { "event": "timeline_reaches", "ms": 0, "cue": "cue1" },
                        "action": { "action": "show_layer", "layer": "l1" }
                    },
                    {
                        "when": { "event": "timeline_reaches", "ms": 200 },
                        "action": { "action": "set_variable", "variable": "seq", "op": "assign", "value": "disabled" },
                        "disabled": true
                    }
                ]
            }]
        }]
    }))
    .expect("course");
    c.scenes[0].slides[0].duration_ms = 2000;

    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    p.handle(&Input::Tick { ms: 250 });
    assert_eq!(p.variable("seq"), Some(&Value::Text(String::new())));
    // Crosses cue1 at 300ms -> shows layer l1.
    p.handle(&Input::Tick { ms: 100 });
    assert_eq!(p.state().visible_layers, vec!["l1".to_owned()]);
    // Base advances to 500 (fires base500), layer advances to 150.
    p.handle(&Input::Tick { ms: 150 });
    assert_eq!(p.variable("seq"), Some(&Value::Text("base500".into())));
    // Layer advances to 450 (fires layer400).
    p.handle(&Input::Tick { ms: 300 });
    assert_eq!(p.variable("seq"), Some(&Value::Text("layer400".into())));
}

#[test]
fn video_cues_borrow_fires_pauses_and_seeks_backwards_correctly() {
    let c: Course = serde_json::from_value(serde_json::json!({
        "schema_version": 1,
        "id": "video-cues",
        "title": "Video Cues",
        "slide_size": { "w": 800, "h": 600 },
        "scenes": [{
            "id": "sc1",
            "title": "Scene 1",
            "slides": [{
                "id": "s1",
                "title": "S1",
                "objects": [{
                    "id": "vid",
                    "kind": {
                        "type": "video",
                        "src": "v.mp4",
                        "autoplay": true,
                        "cues": [
                            { "id": "c1", "name": "C1", "at_ms": 500.0, "pause": false },
                            { "id": "c2", "name": "C2", "at_ms": 1000.0, "pause": true }
                        ]
                    },
                    "frame": { "x": 0.0, "y": 0.0, "w": 400.0, "h": 300.0 }
                }]
            }]
        }]
    }))
    .expect("course");

    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    let fx1 = p.handle(&Input::MediaTime {
        object: "vid".into(),
        at_ms: 600.0,
    });
    assert!(has(
        &fx1,
        &Effect::CueReached {
            object: "vid".into(),
            cue: "c1".into()
        }
    ));
    // Backward seek re-arms c1.
    p.handle(&Input::MediaTime {
        object: "vid".into(),
        at_ms: 100.0,
    });
    let fx2 = p.handle(&Input::MediaTime {
        object: "vid".into(),
        at_ms: 600.0,
    });
    assert!(has(
        &fx2,
        &Effect::CueReached {
            object: "vid".into(),
            cue: "c1".into()
        }
    ));
    // Advance to pausing cue c2.
    let fx3 = p.handle(&Input::MediaTime {
        object: "vid".into(),
        at_ms: 1100.0,
    });
    assert!(has(
        &fx3,
        &Effect::CueReached {
            object: "vid".into(),
            cue: "c2".into()
        }
    ));
    assert!(has(
        &fx3,
        &Effect::Media {
            object: "vid".into(),
            op: MediaOp::Pause
        }
    ));
}

#[test]
fn keyboard_objects_uses_precomputed_pointer_trigger_set() {
    let c: Course = serde_json::from_value(serde_json::json!({
        "schema_version": 1,
        "id": "ptr-cache",
        "title": "Pointer Cache",
        "slide_size": { "w": 800, "h": 600 },
        "scenes": [{
            "id": "sc1",
            "title": "Scene 1",
            "slides": [{
                "id": "s1",
                "title": "S1",
                "objects": [
                    { "id": "clicked", "kind": { "type": "shape", "shape": "rectangle", "fill": "#ffffff" }, "frame": { "x": 0.0, "y": 0.0, "w": 50.0, "h": 50.0 } },
                    { "id": "hovered", "kind": { "type": "shape", "shape": "rectangle", "fill": "#ffffff" }, "frame": { "x": 60.0, "y": 0.0, "w": 50.0, "h": 50.0 } },
                    { "id": "disabled_ptr", "kind": { "type": "shape", "shape": "rectangle", "fill": "#ffffff" }, "frame": { "x": 120.0, "y": 0.0, "w": 50.0, "h": 50.0 } },
                    { "id": "plain", "kind": { "type": "shape", "shape": "rectangle", "fill": "#ffffff" }, "frame": { "x": 180.0, "y": 0.0, "w": 50.0, "h": 50.0 } }
                ],
                "layers": [{
                    "id": "l1",
                    "name": "L1",
                    "objects": [
                        { "id": "layer_obj", "kind": { "type": "shape", "shape": "rectangle", "fill": "#ffffff" }, "frame": { "x": 240.0, "y": 0.0, "w": 50.0, "h": 50.0 } }
                    ],
                    "triggers": [
                        { "when": { "event": "hover_end", "object": "layer_obj" }, "action": { "action": "hide_layer", "layer": "l1" } }
                    ]
                }],
                "triggers": [
                    { "when": { "event": "click", "object": "clicked" }, "action": { "action": "show_layer", "layer": "l1" } },
                    { "when": { "event": "hover_start", "object": "hovered" }, "action": { "action": "show_layer", "layer": "l1" } },
                    { "when": { "event": "click", "object": "disabled_ptr" }, "action": { "action": "show_layer", "layer": "l1" }, "disabled": true }
                ]
            }]
        }]
    }))
    .expect("course");

    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    assert_eq!(p.keyboard_objects(), vec!["clicked", "hovered"]);
    p.handle(&click("clicked"));
    assert_eq!(
        p.keyboard_objects(),
        vec!["clicked", "hovered", "layer_obj"]
    );
}

#[test]
fn exit_animations_with_slide_transition_fly_out_and_disappear() {
    use crate::ObjectSample;
    use coursecraft_course::{
        AnimEffect, Animation, Direction, Easing, Transition, TransitionEffect,
    };

    let make_course = |transition: Option<Transition>| {
        let mut c = Course::from_json(HELLO).expect("hello");
        let s = c.slide_mut("s-intro").expect("s-intro");
        s.duration_ms = 5000;
        s.transition = transition;
        s.objects = vec![
            serde_json::from_value(serde_json::json!({
                "id": "obj-fly",
                "kind": { "type": "shape", "shape": "rectangle", "fill": "#2F6FEB" },
                "frame": { "x": 100.0, "y": 100.0, "w": 200.0, "h": 100.0 },
                "timing": { "start_ms": 0, "end_ms": null },
                "exit": {
                    "effect": { "type": "fly_in", "from": "left" },
                    "duration_ms": 1000,
                    "easing": "linear"
                }
            }))
            .expect("obj-fly"),
            serde_json::from_value(serde_json::json!({
                "id": "obj-disappear",
                "kind": { "type": "shape", "shape": "rectangle", "fill": "#10B981" },
                "frame": { "x": 400.0, "y": 100.0, "w": 200.0, "h": 100.0 },
                "timing": { "start_ms": 0, "end_ms": null },
                "exit": {
                    "effect": { "type": "fade" },
                    "duration_ms": 1000,
                    "easing": "linear"
                }
            }))
            .expect("obj-disappear"),
            serde_json::from_value(serde_json::json!({
                "id": "obj-hard-end",
                "kind": { "type": "shape", "shape": "rectangle", "fill": "#F59E0B" },
                "frame": { "x": 700.0, "y": 100.0, "w": 200.0, "h": 100.0 },
                "timing": { "start_ms": 0, "end_ms": 5000 }
            }))
            .expect("obj-hard-end"),
        ];
        s.triggers.clear();
        c
    };

    for trans in [
        None,
        Some(Transition {
            effect: TransitionEffect::Fade,
            duration_ms: 500,
        }),
        Some(Transition {
            effect: TransitionEffect::Push {
                from: Direction::Right,
            },
            duration_ms: 500,
        }),
        Some(Transition {
            effect: TransitionEffect::Cover {
                from: Direction::Bottom,
            },
            duration_ms: 500,
        }),
    ] {
        let c = make_course(trans.clone());
        let mut p = Player::new(&c).expect("player");
        p.handle(&Input::Start);

        // Before exit window (t = 3500ms): both at rest.
        p.handle(&Input::Tick { ms: 3500 });
        assert_eq!(p.sample_object("obj-fly"), Some(ObjectSample::IDENTITY));
        assert_eq!(
            p.sample_object("obj-disappear"),
            Some(ObjectSample::IDENTITY)
        );
        assert_eq!(
            p.sample_object("obj-hard-end"),
            Some(ObjectSample::IDENTITY)
        );

        // Mid-exit (t = 4500ms, exit is 4000..5000): Fly out moved left by 50% of (100+200)=150px,
        // Disappear faded to 0.5 opacity.
        p.handle(&Input::Tick { ms: 1000 });
        let fly_mid = p.sample_object("obj-fly").expect("fly mid-exit");
        assert!(
            (fly_mid.offset.0 - (-150.0)).abs() < 1e-3,
            "fly out should move at mid-exit with transition {trans:?}, got {:?}",
            fly_mid.offset
        );
        let fade_mid = p.sample_object("obj-disappear").expect("fade mid-exit");
        assert!(
            (fade_mid.opacity - 0.5).abs() < 1e-3,
            "disappear should fade at mid-exit with transition {trans:?}, got {}",
            fade_mid.opacity
        );
        assert_eq!(
            p.sample_object("obj-hard-end"),
            Some(ObjectSample::IDENTITY)
        );

        // End of timeline (t = 5000ms): all exited / ended.
        p.handle(&Input::Tick { ms: 500 });
        assert!(p.sample_object("obj-fly").is_none());
        assert!(p.sample_object("obj-disappear").is_none());
        assert!(p.sample_object("obj-hard-end").is_none());
        assert!(warned(&p.handle(&click("obj-fly"))));
        assert!(warned(&p.handle(&click("obj-disappear"))));
    }

    // Silence unused import warning if needed by using Animation/AnimEffect directly.
    let _ = Animation {
        effect: AnimEffect::new("zoom"),
        duration_ms: 500,
        easing: Easing::Linear,
        sequence: Default::default(),
    };
}

#[test]
fn every_exit_effect_with_and_without_transition_and_entrance_exit() {
    use crate::ObjectSample;
    use coursecraft_course::{
        AnimEffect, Animation, Direction, Easing, Transition, TransitionEffect,
    };

    let effects = [
        AnimEffect::new("fade"),
        AnimEffect::with_from("fly_in", Direction::Left),
        AnimEffect::with_from("fly_in", Direction::Right),
        AnimEffect::with_from("fly_in", Direction::Top),
        AnimEffect::with_from("fly_in", Direction::Bottom),
        AnimEffect::new("zoom"),
        AnimEffect::with_from("wipe", Direction::Left),
        AnimEffect::with_from("float", Direction::Bottom),
    ];

    for with_transition in [false, true] {
        for effect in &effects {
            let mut c = Course::from_json(HELLO).expect("hello");
            let s = c.slide_mut("s-intro").expect("s-intro");
            s.duration_ms = 4000;
            s.transition = with_transition.then_some(Transition {
                effect: TransitionEffect::Fade,
                duration_ms: 500,
            });
            s.objects = vec![
                serde_json::from_value(serde_json::json!({
                    "id": "obj",
                    "kind": { "type": "shape", "shape": "rectangle", "fill": "#2F6FEB" },
                    "frame": { "x": 100.0, "y": 100.0, "w": 200.0, "h": 100.0 },
                    "timing": { "start_ms": 0, "end_ms": null }
                }))
                .expect("obj"),
            ];
            s.objects[0].entrance = Some(Animation {
                effect: AnimEffect::new("fade"),
                duration_ms: 1000,
                easing: Easing::Linear,
                sequence: Default::default(),
            });
            s.objects[0].exit = Some(Animation {
                effect: effect.clone(),
                duration_ms: 1000,
                easing: Easing::Linear,
                sequence: Default::default(),
            });
            s.triggers.clear();

            let mut p = Player::new(&c).expect("player");
            p.handle(&Input::Start);

            // Entrance mid (t = 500ms)
            p.handle(&Input::Tick { ms: 500 });
            let ent = p.sample_object("obj").expect("entering");
            assert!((ent.opacity - 0.5).abs() < 1e-3);

            // At rest (t = 2000ms)
            p.handle(&Input::Tick { ms: 1500 });
            assert_eq!(p.sample_object("obj"), Some(ObjectSample::IDENTITY));

            // Exit mid (t = 3500ms)
            p.handle(&Input::Tick { ms: 1500 });
            let ex = p.sample_object("obj").expect("exiting");
            assert_ne!(
                ex,
                ObjectSample::IDENTITY,
                "exit {effect:?} (transition={with_transition}) must animate at mid-exit"
            );

            // End (t = 4000ms)
            p.handle(&Input::Tick { ms: 500 });
            assert!(p.sample_object("obj").is_none());
        }
    }
}

#[test]
fn m31_17_last_visited_after_bank_redraw_drains_stale_entries_and_never_loops() {
    use coursecraft_course::SlideTarget;

    let mut c = course_with_draw(10, 2, "draw1");
    // Add btn-back (LastVisited) to s-intro, every bank slide, and a trailing s-results slide.
    let back_btn: Object = serde_json::from_value(serde_json::json!({
        "id": "btn-back",
        "kind": { "type": "button", "label": "Back" },
        "frame": { "x": 0.0, "y": 100.0, "w": 80.0, "h": 40.0 }
    }))
    .expect("btn-back");
    let back_trigger = Trigger {
        action: Action::JumpToSlide {
            target: SlideTarget::LastVisited,
        },
        when: Event::Click {
            object: "btn-back".into(),
        },
        condition: None,
        disabled: false,
    };

    for slide in &mut c.scenes[0].slides {
        slide.objects.push(back_btn.clone());
        slide.triggers.push(back_trigger.clone());
        if slide.id == "s-results" {
            slide.objects.push(
                serde_json::from_value::<Object>(serde_json::json!({
                    "id": "btn-retry",
                    "kind": { "type": "button", "label": "Retry" },
                    "frame": { "x": 0.0, "y": 0.0, "w": 80.0, "h": 40.0 }
                }))
                .expect("btn-retry"),
            );
            slide.triggers.push(Trigger {
                action: Action::RetryQuiz,
                when: Event::Click {
                    object: "btn-retry".into(),
                },
                condition: None,
                disabled: false,
            });
        }
    }
    for slide in &mut c.banks[0].slides {
        slide.objects.push(back_btn.clone());
        slide.triggers.push(back_trigger.clone());
    }

    let mut p = Player::with_draw_seed(&c, 1).expect("player");
    p.handle(&Input::Start);
    assert_eq!(p.current_slide().map(|s| s.id.as_str()), Some("s-intro"));

    // Visit s-intro -> s-tabs -> bank1 -> bank2 -> s-results.
    p.handle(&Input::Next);
    assert_eq!(p.current_slide().map(|s| s.id.as_str()), Some("s-tabs"));
    p.handle(&Input::Next);
    let q1 = p.current_slide().expect("q1").id.clone();
    p.handle(&Input::Next);
    let q2 = p.current_slide().expect("q2").id.clone();
    p.handle(&Input::Next);
    assert_eq!(p.current_slide().map(|s| s.id.as_str()), Some("s-results"));
    assert_eq!(
        p.state().visit_history,
        vec![
            "s-intro".to_owned(),
            "s-tabs".to_owned(),
            q1.clone(),
            q2.clone()
        ]
    );

    // Retry quiz redraws the bank (replacing q1/q2) and enters the first redrawn slide.
    p.handle(&click("btn-retry"));
    let q3 = p.current_slide().expect("q3").id.clone();
    assert_ne!(q3, q1);
    assert_ne!(q3, q2);
    assert!(p.slide_position(&q1).is_none(), "old q1 must be gone");
    assert!(p.slide_position(&q2).is_none(), "old q2 must be gone");

    // LastVisited #1 on q3 -> pops "s-results", enters s-results.
    let fx1 = p.handle(&click("btn-back"));
    assert!(has(&fx1, &slide_entered("s-results")));
    assert_eq!(p.current_slide().map(|s| s.id.as_str()), Some("s-results"));

    // LastVisited #2 on s-results -> drains stale q2, q1, pops "s-tabs", enters s-tabs.
    let fx2 = p.handle(&click("btn-back"));
    assert!(has(&fx2, &slide_entered("s-tabs")));
    assert_eq!(p.current_slide().map(|s| s.id.as_str()), Some("s-tabs"));
    assert_eq!(p.state().visit_history, vec!["s-intro".to_owned()]);

    // LastVisited #3 on s-tabs -> pops "s-intro", enters s-intro, drains history to [].
    let fx3 = p.handle(&click("btn-back"));
    assert!(has(&fx3, &slide_entered("s-intro")));
    assert_eq!(p.current_slide().map(|s| s.id.as_str()), Some("s-intro"));
    assert!(
        p.state().visit_history.is_empty(),
        "stale entries must be drained, got {:?}",
        p.state().visit_history
    );

    // LastVisited #4 on s-intro (index 0) -> does NOT re-enter s-intro.
    let fx4 = p.handle(&click("btn-back"));
    assert!(
        !fx4.iter().any(|e| matches!(e, Effect::SlideEntered { .. })),
        "LastVisited on slide 0 with empty history must not loop/re-enter, got {fx4:?}"
    );
    assert_eq!(p.current_slide().map(|s| s.id.as_str()), Some("s-intro"));
}

#[test]
fn m31_17_last_visited_skips_self_duplicates_and_handles_preview_subset() {
    use coursecraft_course::SlideTarget;

    let mut c = course_with_draw(10, 1, "draw1");
    let back_btn: Object = serde_json::from_value(serde_json::json!({
        "id": "btn-back",
        "kind": { "type": "button", "label": "Back" },
        "frame": { "x": 0.0, "y": 100.0, "w": 80.0, "h": 40.0 }
    }))
    .expect("btn-back");
    let back_trigger = Trigger {
        action: Action::JumpToSlide {
            target: SlideTarget::LastVisited,
        },
        when: Event::Click {
            object: "btn-back".into(),
        },
        condition: None,
        disabled: false,
    };
    for slide in &mut c.scenes[0].slides {
        slide.objects.push(back_btn.clone());
        slide.triggers.clear();
        slide.triggers.push(back_trigger.clone());
        if slide.id == "s-results" {
            slide.objects.push(
                serde_json::from_value::<Object>(serde_json::json!({
                    "id": "btn-retry",
                    "kind": { "type": "button", "label": "Retry" },
                    "frame": { "x": 0.0, "y": 0.0, "w": 80.0, "h": 40.0 }
                }))
                .expect("btn-retry"),
            );
            slide.triggers.push(Trigger {
                action: Action::RetryQuiz,
                when: Event::Click {
                    object: "btn-retry".into(),
                },
                condition: None,
                disabled: false,
            });
        }
    }
    for slide in &mut c.banks[0].slides {
        slide.objects.push(back_btn.clone());
        slide.triggers.clear();
        slide.triggers.push(back_trigger.clone());
    }

    // Case 1: consecutive/self-matching entries in visit_history are skipped until a different slide.
    let mut p2 = Player::with_draw_seed(&c, 1).expect("player2");
    p2.handle(&Input::Start);
    p2.handle(&Input::Jump {
        slide: "s-results".into(),
    });
    p2.handle(&Input::Jump {
        slide: "s-intro".into(),
    });
    p2.handle(&Input::Jump {
        slide: "s-tabs".into(),
    });
    p2.handle(&Input::Next); // q1
    let q1_id = p2.current_slide().expect("q1").id.clone();
    p2.handle(&Input::Jump {
        slide: "s-results".into(),
    });
    p2.handle(&click("btn-retry")); // removes q1_id, pushes "s-results", enters q2.
    let q2_id = p2.current_slide().expect("q2").id.clone();
    assert_ne!(q1_id, q2_id);

    // LastVisited on q2 -> pops "s-results", enters s-results.
    let fx_q2 = p2.handle(&click("btn-back"));
    assert!(has(&fx_q2, &slide_entered("s-results")));
    // LastVisited on s-results -> drains missing q1_id, pops "s-tabs", enters s-tabs.
    let fx_back = p2.handle(&click("btn-back"));
    assert!(has(&fx_back, &slide_entered("s-tabs")));
    assert_eq!(p2.current_slide().map(|s| s.id.as_str()), Some("s-tabs"));
    assert_eq!(
        p2.state().visit_history,
        vec![
            "s-intro".to_owned(),
            "s-results".to_owned(),
            "s-intro".to_owned()
        ]
    );
    // Next LastVisited from s-tabs pops "s-intro" and enters s-intro.
    let fx_intro1 = p2.handle(&click("btn-back"));
    assert!(has(&fx_intro1, &slide_entered("s-intro")));
    // Next LastVisited from s-intro pops "s-results" and enters s-results.
    let fx_res = p2.handle(&click("btn-back"));
    assert!(has(&fx_res, &slide_entered("s-results")));
    // Next LastVisited from s-results pops "s-intro" and enters s-intro, draining history.
    let fx_intro2 = p2.handle(&click("btn-back"));
    assert!(has(&fx_intro2, &slide_entered("s-intro")));
    assert!(p2.state().visit_history.is_empty());

    // Case 2: preview subset with single slide hits preview boundary on LastVisited at index 0.
    let mut prev =
        Player::with_preview_subset(&c, 0, &["s-intro".to_owned()]).expect("preview subset");
    prev.handle(&Input::Start);
    let fx_prev = prev.handle(&click("btn-back"));
    assert!(
        !fx_prev
            .iter()
            .any(|e| matches!(e, Effect::SlideEntered { .. }))
    );
    assert!(prev.preview_boundary_hit);
}

/// W-01: `Player::next_wake_ms` returns `None` before start, while paused,
/// after the slide timeline finishes, and after the course exits.
#[test]
fn w01_next_wake_ms_none_when_not_started_paused_ended_or_finished() {
    let mut c = Course::from_json(
        r#"{
          "schema_version": 1,
          "id": "w01-idle",
          "title": "W01 Idle",
          "slide_size": { "w": 960, "h": 540 },
          "scenes": [{
            "id": "sc1",
            "title": "Scene 1",
            "slides": [{
              "id": "s1",
              "title": "Slide 1",
              "duration_ms": 2000,
              "objects": [{
                "id": "btn-exit",
                "kind": { "type": "button", "label": "Exit" },
                "frame": { "x": 10.0, "y": 10.0, "w": 100.0, "h": 40.0 }
              }],
              "triggers": [{
                "when": { "event": "click", "object": "btn-exit" },
                "action": { "action": "exit_course" }
              }]
            }]
          }]
        }"#,
    )
    .expect("valid course");
    c.player.show_seekbar = false;

    let mut p = Player::new(&c).expect("player");
    assert_eq!(p.next_wake_ms(), None, "not started yet");

    p.handle(&Input::Start);
    assert_eq!(p.next_wake_ms(), Some(2000), "running towards timeline end");

    p.handle(&Input::PlayerPause);
    assert_eq!(p.next_wake_ms(), None, "paused by learner");

    p.handle(&Input::PlayerResume);
    assert_eq!(p.next_wake_ms(), Some(2000), "resumed");

    p.handle(&Input::Tick { ms: 2000 });
    assert_eq!(p.next_wake_ms(), None, "timeline finished");

    // Start a fresh player and exit while timeline is still active.
    let mut p2 = Player::new(&c).expect("player2");
    p2.handle(&Input::Start);
    assert!(p2.next_wake_ms().is_some());
    p2.handle(&click("btn-exit"));
    assert!(p2.state().exited);
    assert_eq!(p2.next_wake_ms(), None, "course exited");
}

/// W-01: `Player::next_wake_ms` returns `Some(0)` during active entrance,
/// exit, and motion path animations, and sleeps between them.
#[test]
fn w01_next_wake_ms_immediate_during_entrance_exit_and_motion_path() {
    use coursecraft_course::{AnimEffect, Animation, Easing, MotionPath, Point};

    let mut c = Course::from_json(
        r##"{
          "schema_version": 1,
          "id": "w01-anim",
          "title": "W01 Anim",
          "slide_size": { "w": 960, "h": 540 },
          "scenes": [{
            "id": "sc1",
            "title": "Scene 1",
            "slides": [{
              "id": "s1",
              "title": "Slide 1",
              "duration_ms": 5000,
              "objects": [
                {
                  "id": "box1",
                  "kind": { "type": "shape", "shape": "rectangle", "fill": "#336699" },
                  "frame": { "x": 10.0, "y": 10.0, "w": 100.0, "h": 100.0 },
                  "timing": { "start_ms": 1000, "end_ms": 3000 }
                },
                {
                  "id": "btn-move",
                  "kind": { "type": "button", "label": "Move" },
                  "frame": { "x": 200.0, "y": 10.0, "w": 100.0, "h": 40.0 }
                }
              ],
              "triggers": [{
                "when": { "event": "click", "object": "btn-move" },
                "action": { "action": "move_on_path", "object": "btn-move", "path": "p1" }
              }]
            }]
          }]
        }"##,
    )
    .expect("valid course");
    c.player.show_seekbar = false;

    let slide = c.slide_mut("s1").expect("s1");
    let box1 = slide
        .objects
        .iter_mut()
        .find(|o| o.id == "box1")
        .expect("box1");
    box1.entrance = Some(Animation {
        effect: AnimEffect::new("fade"),
        duration_ms: 400,
        easing: Easing::Linear,
        sequence: Default::default(),
    });
    box1.exit = Some(Animation {
        effect: AnimEffect::new("fade"),
        duration_ms: 300,
        easing: Easing::Linear,
        sequence: Default::default(),
    });
    let btn = slide
        .objects
        .iter_mut()
        .find(|o| o.id == "btn-move")
        .expect("btn-move");
    btn.motion_paths = vec![MotionPath {
        id: "p1".into(),
        points: vec![Point { x: 0.0, y: 0.0 }, Point { x: 120.0, y: 0.0 }],
        duration_ms: 600,
        easing: Easing::Linear,
        relative: true,
    }];

    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);

    // Waiting for box1 entrance at 1000 ms.
    assert_eq!(p.next_wake_ms(), Some(1000));
    p.handle(&Input::Tick { ms: 1000 });
    // Entrance runs [1000, 1400).
    assert_eq!(p.next_wake_ms(), Some(0));
    p.handle(&Input::Tick { ms: 200 });
    assert_eq!(p.next_wake_ms(), Some(0));
    p.handle(&Input::Tick { ms: 200 });
    // Now at 1400; exit starts at 3000 - 300 = 2700 -> delta 1300.
    assert_eq!(p.next_wake_ms(), Some(1300));
    p.handle(&Input::Tick { ms: 1300 });
    // Exit runs [2700, 3000).
    assert_eq!(p.next_wake_ms(), Some(0));
    p.handle(&Input::Tick { ms: 300 });
    // Now at 3000; slide ends at 5000 -> delta 2000.
    assert_eq!(p.next_wake_ms(), Some(2000));

    // Start a motion path on btn-move -> immediate wake while running.
    p.handle(&click("btn-move"));
    assert_eq!(p.next_wake_ms(), Some(0));
    p.handle(&Input::Tick { ms: 300 });
    assert_eq!(p.next_wake_ms(), Some(0));
    p.handle(&Input::Tick { ms: 300 });
    // Path finished at t = 3600; 1400 ms remain until slide ends at 5000.
    assert_eq!(p.next_wake_ms(), Some(1400));
}

/// W-01: `Player::next_wake_ms` returns exact delays to object start, cue point,
/// `timeline_reaches`, and timeline end, and caps at 100 ms when the seekbar is visible.
#[test]
fn w01_next_wake_ms_exact_delay_to_object_start_cue_point_and_timeline_reaches() {
    let mut c = Course::from_json(
        r##"{
          "schema_version": 1,
          "id": "w01-delays",
          "title": "W01 Delays",
          "slide_size": { "w": 960, "h": 540 },
          "scenes": [{
            "id": "sc1",
            "title": "Scene 1",
            "slides": [{
              "id": "s1",
              "title": "Slide 1",
              "duration_ms": 8000,
              "objects": [{
                "id": "late-box",
                "kind": { "type": "shape", "shape": "rectangle", "fill": "#336699" },
                "frame": { "x": 10.0, "y": 10.0, "w": 100.0, "h": 100.0 },
                "timing": { "start_ms": 1500 }
              }],
              "cue_points": [
                { "id": "cue1", "name": "Cue 1", "at_ms": 2500 }
              ],
              "triggers": [{
                "when": { "event": "timeline_reaches", "ms": 4000 },
                "action": { "action": "change_state", "object": "late-box", "state": "normal" }
              }]
            }]
          }]
        }"##,
    )
    .expect("valid course");
    c.player.show_seekbar = false;

    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);

    assert_eq!(p.next_wake_ms(), Some(1500), "delay to object start");
    p.handle(&Input::Tick { ms: 1500 });
    assert_eq!(p.next_wake_ms(), Some(1000), "delay to cue point at 2500");
    p.handle(&Input::Tick { ms: 1000 });
    assert_eq!(
        p.next_wake_ms(),
        Some(1500),
        "delay to timeline_reaches at 4000"
    );
    p.handle(&Input::Tick { ms: 1500 });
    assert_eq!(
        p.next_wake_ms(),
        Some(4000),
        "delay to timeline end at 8000"
    );

    // With seekbar visible, the wake delay caps at 100 ms (10 fps seekbar).
    c.player.show_seekbar = true;
    let mut p_seek = Player::new(&c).expect("player with seekbar");
    p_seek.handle(&Input::Start);
    assert_eq!(p_seek.next_wake_ms(), Some(100));
}

/// W-01: `Player::next_wake_ms` covers running timers, visible layer clocks,
/// and playing video, Lottie, and audio (captioned and uncaptioned).
#[test]
fn w01_next_wake_ms_covers_timers_visible_layer_clocks_and_playing_media() {
    let mut c = Course::from_json(
        r#"{
          "schema_version": 1,
          "id": "w01-sources",
          "title": "W01 Sources",
          "slide_size": { "w": 960, "h": 540 },
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
                  "kind": { "type": "timer", "duration_ms": 5000, "autostart": true },
                  "frame": { "x": 10.0, "y": 10.0, "w": 120.0, "h": 40.0 }
                },
                {
                  "id": "btn-layer",
                  "kind": { "type": "button", "label": "Layer" },
                  "frame": { "x": 10.0, "y": 60.0, "w": 100.0, "h": 40.0 }
                },
                {
                  "id": "aud-plain",
                  "kind": { "type": "audio", "src": "voice.mp3", "autoplay": false },
                  "frame": { "x": 10.0, "y": 110.0, "w": 48.0, "h": 48.0 }
                },
                {
                  "id": "aud-cc",
                  "kind": { "type": "audio", "src": "voice.mp3", "autoplay": false, "captions": "voice.vtt" },
                  "frame": { "x": 70.0, "y": 110.0, "w": 48.0, "h": 48.0 }
                },
                {
                  "id": "vid1",
                  "kind": { "type": "video", "src": "clip.mp4", "autoplay": false },
                  "frame": { "x": 130.0, "y": 110.0, "w": 200.0, "h": 120.0 }
                },
                {
                  "id": "lot1",
                  "kind": { "type": "lottie", "src": "anim.json", "autoplay": false },
                  "frame": { "x": 350.0, "y": 110.0, "w": 120.0, "h": 120.0 }
                }
              ],
              "layers": [{
                "id": "l1",
                "name": "Layer 1",
                "duration_ms": 2000,
                "cue_points": [{ "id": "lcue", "name": "Layer Cue", "at_ms": 700 }]
              }],
              "triggers": [
                {
                  "when": { "event": "click", "object": "btn-layer" },
                  "action": { "action": "show_layer", "layer": "l1" }
                },
                {
                  "when": { "event": "click", "object": "aud-plain" },
                  "action": { "action": "play_media", "object": "aud-plain" }
                },
                {
                  "when": { "event": "click", "object": "aud-cc" },
                  "action": { "action": "play_media", "object": "aud-cc" }
                },
                {
                  "when": { "event": "click", "object": "vid1" },
                  "action": { "action": "play_media", "object": "vid1" }
                },
                {
                  "when": { "event": "click", "object": "lot1" },
                  "action": { "action": "play_media", "object": "lot1" }
                }
              ]
            }]
          }]
        }"#,
    )
    .expect("valid course");
    c.player.show_seekbar = false;

    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);

    // Running countdown timer caps wake at 100 ms for crisp second updates.
    assert_eq!(p.next_wake_ms(), Some(100));
    p.handle(&Input::Tick { ms: 4960 });
    assert_eq!(p.next_wake_ms(), Some(40), "remaining timer < 100 ms");
    p.handle(&Input::Tick { ms: 40 });
    // Timer finished at t = 5000; 5000 ms left on base slide.
    assert_eq!(p.next_wake_ms(), Some(5000));
    // Finish base slide.
    p.handle(&Input::Tick { ms: 5000 });
    assert_eq!(p.next_wake_ms(), None);

    // Showing a visible layer wakes for the layer's cue point and end.
    p.handle(&click("btn-layer"));
    assert_eq!(p.next_wake_ms(), Some(700));
    p.handle(&Input::Tick { ms: 700 });
    assert_eq!(p.next_wake_ms(), Some(1300));
    p.handle(&Input::Tick { ms: 1300 });
    assert_eq!(p.next_wake_ms(), None);

    // Playing plain audio wakes at 50 ms polling cadence.
    p.handle(&click("aud-plain"));
    assert_eq!(p.next_wake_ms(), Some(50));
    p.handle(&Input::MediaEnded {
        object: "aud-plain".into(),
    });
    assert_eq!(p.next_wake_ms(), None);

    // Playing captioned audio wakes immediately.
    p.handle(&click("aud-cc"));
    assert_eq!(p.next_wake_ms(), Some(0));
    p.handle(&Input::MediaEnded {
        object: "aud-cc".into(),
    });
    assert_eq!(p.next_wake_ms(), None);

    // Playing video wakes immediately.
    p.handle(&click("vid1"));
    assert_eq!(p.next_wake_ms(), Some(0));
    p.handle(&Input::MediaEnded {
        object: "vid1".into(),
    });
    assert_eq!(p.next_wake_ms(), None);

    // Playing Lottie wakes immediately.
    p.handle(&click("lot1"));
    assert_eq!(p.next_wake_ms(), Some(0));
    p.handle(&Input::MediaEnded {
        object: "lot1".into(),
    });
    assert_eq!(p.next_wake_ms(), None);
}

/// W-01: `Player::next_wake_ms` scales non-zero wake delays by playback speed.
#[test]
fn w01_next_wake_ms_scales_with_playback_speed() {
    let mut c = Course::from_json(
        r##"{
          "schema_version": 1,
          "id": "w01-speed",
          "title": "W01 Speed",
          "slide_size": { "w": 960, "h": 540 },
          "scenes": [{
            "id": "sc1",
            "title": "Scene 1",
            "slides": [{
              "id": "s1",
              "title": "Slide 1",
              "duration_ms": 4000,
              "objects": [{
                "id": "late",
                "kind": { "type": "shape", "shape": "rectangle", "fill": "#336699" },
                "frame": { "x": 10.0, "y": 10.0, "w": 100.0, "h": 100.0 },
                "timing": { "start_ms": 1000 }
              }]
            }]
          }]
        }"##,
    )
    .expect("valid course");
    c.player.show_seekbar = false;
    c.player.show_speed = true;

    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    assert_eq!(p.next_wake_ms(), Some(1000));

    p.handle(&Input::SetSpeed { rate: 2.0 });
    assert_eq!(p.next_wake_ms(), Some(500));

    p.handle(&Input::SetSpeed { rate: 0.5 });
    assert_eq!(p.next_wake_ms(), Some(2000));

    p.handle(&Input::SetSpeed { rate: 1.5 });
    assert_eq!(p.next_wake_ms(), Some(667));
}

#[test]
fn m37_golden_suspend_data_resume() {
    let c = course();
    let mut p = Player::new(&c).expect("player");
    to_quiz(&mut p);

    // Record a graded quiz answer on s-quiz so slide, visited slides, modified
    // object states (tab1/tab2/tab3 visited, btn-next normal), modified variable
    // (tabs_seen = 3), and quiz answers are all exercised in the golden payload.
    p.handle(&Input::SelectChoice { choice: "b".into() });
    let fx = p.handle(&click("btn-submit"));
    assert!(has(
        &fx,
        &Effect::QuestionAnswered {
            slide: "s-quiz".into(),
            correct: true,
            points: 10,
            attempts_left: Some(1),
        }
    ));

    const GOLDEN_SUSPEND: &str = "cc1:AAIDAAECAQEEAAMBAwIDAwABAAEAAAAAAAAIQAECAQEK";
    let encoded = encode_suspend(&c, p.state());
    assert_eq!(
        encoded, GOLDEN_SUSPEND,
        "pre-M37 suspend_data changed: got {encoded:?}"
    );

    let decoded = decode_suspend(&c, GOLDEN_SUSPEND).expect("decode golden suspend_data");
    assert_eq!(decoded.slide, p.state().slide);
    assert_eq!(decoded.visited, p.state().visited);
    assert_eq!(decoded.object_states, p.state().object_states);
    assert_eq!(decoded.variables, p.state().variables);
    assert_eq!(decoded.answers, p.state().answers);

    let mut resumed = Player::resume(&c, decoded).expect("resume from golden suspend_data");
    let start_fx = resumed.handle(&Input::Start);
    assert_eq!(start_fx.first(), Some(&slide_entered("s-quiz")));
    assert_eq!(
        resumed.current_slide().map(|s| s.id.as_str()),
        Some("s-quiz")
    );
    assert_eq!(resumed.state().visited, p.state().visited);
    assert_eq!(resumed.state().object_states, p.state().object_states);
    assert_eq!(resumed.variable("tabs_seen"), Some(&Value::Number(3.0)));
    assert_eq!(resumed.state().answers, p.state().answers);
    assert_eq!(encode_suspend(&c, resumed.state()), GOLDEN_SUSPEND);
}

fn gb_03_course() -> Course {
    use coursecraft_course::{
        Color, Condition, Group, Layer, Rect, Scene, ShapeKind, StateDef, TextStyle, Timing,
    };

    let make_obj = |id: &str, group: Option<&str>, states: Vec<StateDef>| Object {
        id: id.into(),
        name: id.into(),
        kind: ObjectKind::Shape {
            shape: ShapeKind::Rectangle,
            fill: Color("#2F6FEB".into()),
            fill_style: None,
            outline: None,
            text: None,
            adjust: Vec::new(),
        },
        frame: Rect {
            x: 100.0,
            y: 100.0,
            w: 160.0,
            h: 48.0,
        },
        timing: Timing::default(),
        states,
        initial_state: "normal".into(),
        entrance: None,
        exit: None,
        motion_paths: Vec::new(),
        emphasis: Vec::new(),
        focus_order: None,
        decorative: false,
        description: None,
        group: group.map(str::to_owned),
        panel: None,
        rotation: 0.0,
        flip_h: false,
        flip_v: false,
        opacity: 1.0,
        shadow: None,
        glow: None,
        drag: None,
        locked: false,
        editor_hidden: false,
    };

    let mut label = make_obj("m-label", Some("g-btn"), Vec::new());
    label.kind = ObjectKind::Text {
        text: "Back".into(),
        style: TextStyle::default(),
        runs: Vec::new(),
        paragraphs: Vec::new(),
        autofit: coursecraft_course::Autofit::None,
        text_style: None,
        outline: None,
        point_text: false,
    };

    let state = |name: &str| StateDef {
        name: name.into(),
        ..StateDef::default()
    };

    let slide = Slide {
        id: "s1".into(),
        title: "Interactive groups".into(),
        duration_ms: 5000,
        background: None,
        objects: vec![
            make_obj(
                "m-bg",
                Some("g-btn"),
                vec![
                    state("hover"),
                    state("down"),
                    state("selected"),
                    state("custom_lit"),
                ],
            ),
            make_obj("m-icon", Some("g-btn"), vec![state("hover")]),
            label,
            make_obj("v-bg", Some("g-visit"), vec![state("visited")]),
            make_obj("v-txt", Some("g-visit"), Vec::new()),
            make_obj("btn-disable", None, Vec::new()),
            make_obj("btn-hide", None, Vec::new()),
            make_obj("btn-check-cond", None, Vec::new()),
        ],
        layers: Vec::<Layer>::new(),
        groups: vec![
            Group {
                id: "g-btn".into(),
                name: "Back button".into(),
                interactive: true,
                initial_state: "normal".into(),
                focus_order: None,
                description: None,
                entrance: None,
                exit: None,
                emphasis: Vec::new(),
            },
            Group {
                id: "g-visit".into(),
                name: "Visit button".into(),
                interactive: true,
                initial_state: "normal".into(),
                focus_order: None,
                description: None,
                entrance: None,
                exit: None,
                emphasis: Vec::new(),
            },
        ],
        triggers: vec![
            Trigger {
                action: Action::SetVariable {
                    variable: "group_clicks".into(),
                    op: VarOp::Add,
                    value: Some(Value::Number(1.0).into()),
                },
                when: Event::Click {
                    object: "g-btn".into(),
                },
                condition: None,
                disabled: false,
            },
            Trigger {
                action: Action::SetVariable {
                    variable: "member_clicks".into(),
                    op: VarOp::Add,
                    value: Some(Value::Number(1.0).into()),
                },
                when: Event::Click {
                    object: "m-icon".into(),
                },
                condition: None,
                disabled: false,
            },
            Trigger {
                action: Action::SetVariable {
                    variable: "hover_starts".into(),
                    op: VarOp::Add,
                    value: Some(Value::Number(1.0).into()),
                },
                when: Event::HoverStart {
                    object: "g-btn".into(),
                },
                condition: None,
                disabled: false,
            },
            Trigger {
                action: Action::SetVariable {
                    variable: "hover_ends".into(),
                    op: VarOp::Add,
                    value: Some(Value::Number(1.0).into()),
                },
                when: Event::HoverEnd {
                    object: "g-btn".into(),
                },
                condition: None,
                disabled: false,
            },
            Trigger {
                action: Action::SetVariable {
                    variable: "member_hovers".into(),
                    op: VarOp::Add,
                    value: Some(Value::Number(1.0).into()),
                },
                when: Event::HoverStart {
                    object: "m-bg".into(),
                },
                condition: None,
                disabled: false,
            },
            Trigger {
                action: Action::SetVariable {
                    variable: "state_events".into(),
                    op: VarOp::Add,
                    value: Some(Value::Number(1.0).into()),
                },
                when: Event::StateChanges {
                    object: "g-btn".into(),
                    to: None,
                },
                condition: None,
                disabled: false,
            },
            Trigger {
                action: Action::ChangeState {
                    object: "g-btn".into(),
                    state: "disabled".into(),
                },
                when: Event::Click {
                    object: "btn-disable".into(),
                },
                condition: None,
                disabled: false,
            },
            Trigger {
                action: Action::ChangeState {
                    object: "m-label".into(),
                    state: "hidden".into(),
                },
                when: Event::Click {
                    object: "btn-hide".into(),
                },
                condition: None,
                disabled: false,
            },
            Trigger {
                action: Action::SetVariable {
                    variable: "cond_hit".into(),
                    op: VarOp::Assign,
                    value: Some(Value::Bool(true).into()),
                },
                when: Event::Click {
                    object: "btn-check-cond".into(),
                },
                condition: Some(Condition::All(vec![
                    Condition::ObjectState {
                        object: "g-btn".into(),
                        state: "selected".into(),
                        is: true,
                    },
                    Condition::ObjectState {
                        object: "m-bg".into(),
                        state: "selected".into(),
                        is: true,
                    },
                ])),
                disabled: false,
            },
        ],
        cue_points: Vec::new(),
        question: None,
        draw: None,
        transition: None,
        notes: String::new(),
        master: None,
        revisit: RevisitMode::Auto,
        player: None,
    };

    Course {
        schema_version: 1,
        id: "gb03".into(),
        title: "GB-03".into(),
        slide_size: coursecraft_course::Size::default(),
        variables: vec![
            VariableDef {
                name: "group_clicks".into(),
                value: Value::Number(0.0),
            },
            VariableDef {
                name: "member_clicks".into(),
                value: Value::Number(0.0),
            },
            VariableDef {
                name: "hover_starts".into(),
                value: Value::Number(0.0),
            },
            VariableDef {
                name: "hover_ends".into(),
                value: Value::Number(0.0),
            },
            VariableDef {
                name: "member_hovers".into(),
                value: Value::Number(0.0),
            },
            VariableDef {
                name: "state_events".into(),
                value: Value::Number(0.0),
            },
            VariableDef {
                name: "cond_hit".into(),
                value: Value::Bool(false),
            },
        ],
        scenes: vec![Scene {
            id: "sc1".into(),
            title: "Scene 1".into(),
            slides: vec![slide],
        }],
        masters: Vec::new(),
        templates: Vec::new(),
        banks: Vec::new(),
        start_slide: None,
        quiz: None,
        tracking: Default::default(),
        player: coursecraft_course::PlayerSettings::default(),
        theme: coursecraft_course::Theme::default(),
        language: None,
        strings: std::collections::BTreeMap::new(),
        fonts: Vec::new(),
        background_audio: None,
        text_styles: Vec::new(),
        resources: Vec::new(),
        glossary: Vec::new(),
        editor: coursecraft_course::EditorSettings::default(),
    }
}

fn assert_shared_state(p: &Player<'_>, ids: &[&str], expected: &str) {
    for id in ids {
        assert_eq!(p.object_state(id), Some(expected), "state for {id}");
    }
}

fn gb_03_verify_hover_disabled_and_hidden(p: &mut Player<'_>, all_btn: &[&str]) {
    // HoverStart / HoverEnd fire once for the group and keep existing member hover trigger working
    let fx = p.handle(&Input::HoverStart {
        object: "g-btn".into(),
    });
    assert!(!warned(&fx));
    assert_eq!(p.variable("hover_starts"), Some(&Value::Number(1.0)));
    assert_eq!(p.variable("member_hovers"), Some(&Value::Number(1.0)));
    let fx = p.handle(&Input::HoverEnd {
        object: "m-label".into(),
    });
    assert!(!warned(&fx));
    assert_eq!(p.variable("hover_ends"), Some(&Value::Number(1.0)));

    // ChangeState("disabled") disables all members and swallows clicks silently
    let fx = p.handle(&click("btn-disable"));
    assert!(has(
        &fx,
        &Effect::StateChanged {
            object: "g-btn".into(),
            state: "disabled".into(),
        }
    ));
    assert_shared_state(p, all_btn, "disabled");
    let clicks_before = p.variable("group_clicks").cloned();
    assert!(p.handle(&click("g-btn")).is_empty());
    assert!(p.handle(&click("m-icon")).is_empty());
    assert_eq!(p.variable("group_clicks"), clicks_before.as_ref());

    // ChangeState via member id to "hidden" hides all members; subsequent click/hover warns
    let fx = p.handle(&click("btn-hide"));
    assert!(has(
        &fx,
        &Effect::StateChanged {
            object: "g-btn".into(),
            state: "hidden".into(),
        }
    ));
    assert_shared_state(p, all_btn, "hidden");
    assert!(warned(&p.handle(&click("g-btn"))));
    assert!(warned(&p.handle(&Input::HoverStart {
        object: "m-bg".into(),
    })));
}

#[test]
fn gb_03_interactive_group_states_triggers_and_member_triggers() {
    let c = gb_03_course();
    let mut p = Player::new(&c).expect("valid interactive group course");
    p.handle(&Input::Start);

    let all_btn = ["g-btn", "m-bg", "m-icon", "m-label"];
    assert!(p.is_interactive_group("g-btn"));
    assert!(!p.is_interactive_group("m-bg"));
    assert_eq!(p.interactive_group_of("m-bg"), Some("g-btn"));
    assert_eq!(p.interactive_group_of("m-icon"), Some("g-btn"));
    assert_eq!(p.interactive_group_of("m-label"), Some("g-btn"));
    assert_eq!(p.interactive_group_of("btn-disable"), None);
    assert_eq!(p.group_members("g-btn").len(), 3);
    assert_shared_state(&p, &all_btn, "normal");

    // Clicking group ID fires group click once, keeps existing member click working,
    // and toggles built-in "selected" once across all members.
    let fx = p.handle(&click("g-btn"));
    assert_eq!(p.variable("group_clicks"), Some(&Value::Number(1.0)));
    assert_eq!(p.variable("member_clicks"), Some(&Value::Number(1.0)));
    assert_eq!(p.variable("state_events"), Some(&Value::Number(1.0)));
    let state_fx: Vec<_> = fx
        .iter()
        .filter(|e| matches!(e, Effect::StateChanged { .. }))
        .collect();
    assert_eq!(
        state_fx,
        vec![&Effect::StateChanged {
            object: "g-btn".into(),
            state: "selected".into(),
        }]
    );
    assert_shared_state(&p, &all_btn, "selected");

    // Condition::ObjectState matches both group id and member id
    p.handle(&click("btn-check-cond"));
    assert_eq!(p.variable("cond_hit"), Some(&Value::Bool(true)));

    // Clicking a member without its own trigger fires group click once and toggles back to normal
    let fx = p.handle(&click("m-bg"));
    assert_eq!(p.variable("group_clicks"), Some(&Value::Number(2.0)));
    assert_eq!(p.variable("member_clicks"), Some(&Value::Number(1.0)));
    assert!(has(
        &fx,
        &Effect::StateChanged {
            object: "g-btn".into(),
            state: "normal".into(),
        }
    ));
    assert_shared_state(&p, &all_btn, "normal");

    // Clicking member "m-icon" directly fires group click once AND m-icon's click trigger once
    p.handle(&click("m-icon"));
    assert_eq!(p.variable("group_clicks"), Some(&Value::Number(3.0)));
    assert_eq!(p.variable("member_clicks"), Some(&Value::Number(2.0)));
    assert_eq!(p.object_state("g-btn"), Some("selected"));

    // Built-in "visited" applies once across all members
    let fx = p.handle(&click("v-txt"));
    assert!(has(
        &fx,
        &Effect::StateChanged {
            object: "g-visit".into(),
            state: "visited".into(),
        }
    ));
    assert_shared_state(&p, &["g-visit", "v-bg", "v-txt"], "visited");
    let fx2 = p.handle(&click("g-visit"));
    assert!(!fx2.iter().any(|e| matches!(e, Effect::StateChanged { .. })));

    gb_03_verify_hover_disabled_and_hidden(&mut p, &all_btn);
}

#[test]
fn gb_04_interactive_group_keyboard_objects_and_focus() {
    use coursecraft_course::{
        Color, Group, Layer, Rect, Scene, ScrollBar, ShapeKind, StateDef, Timing,
    };

    let shape = |id: &str, group: Option<&str>, y: f32, panel: Option<&str>| Object {
        id: id.into(),
        name: id.into(),
        kind: ObjectKind::Shape {
            shape: ShapeKind::Rectangle,
            fill: Color("#2F6FEB".into()),
            fill_style: None,
            outline: None,
            text: None,
            adjust: Vec::new(),
        },
        frame: Rect {
            x: 80.0,
            y,
            w: 140.0,
            h: 44.0,
        },
        timing: Timing::default(),
        states: vec![StateDef {
            name: "selected".into(),
            ..StateDef::default()
        }],
        initial_state: "normal".into(),
        entrance: None,
        exit: None,
        motion_paths: Vec::new(),
        emphasis: Vec::new(),
        focus_order: None,
        decorative: false,
        description: None,
        group: group.map(str::to_owned),
        panel: panel.map(str::to_owned),
        rotation: 0.0,
        flip_h: false,
        flip_v: false,
        opacity: 1.0,
        shadow: None,
        glow: None,
        drag: None,
        locked: false,
        editor_hidden: false,
    };

    let mut btn_before = shape("btn-before", None, 20.0, None);
    btn_before.kind = ObjectKind::Button {
        label: "Before".into(),
        fill: Color("#2F6FEB".into()),
        fill_style: None,
    };

    // Even a Button member inside an interactive group must be omitted individually.
    let mut member_btn = shape("m-btn", Some("g-btn"), 80.0, None);
    member_btn.kind = ObjectKind::Button {
        label: "Inside".into(),
        fill: Color("#2F6FEB".into()),
        fill_style: None,
    };

    let mut scroll_panel = shape("panel", None, 200.0, None);
    scroll_panel.frame = Rect {
        x: 40.0,
        y: 200.0,
        w: 300.0,
        h: 150.0,
    };
    scroll_panel.kind = ObjectKind::ScrollPanel {
        content_height: 800.0,
        bar: ScrollBar::Visible,
    };

    let slide = Slide {
        id: "s1".into(),
        title: "Keyboard order".into(),
        duration_ms: 5000,
        background: None,
        objects: vec![
            btn_before,
            member_btn,
            shape("m-icon", Some("g-btn"), 80.0, None),
            shape("m-lbl", Some("g-btn"), 80.0, None),
            scroll_panel,
            // Inside scroll panel, initially below viewport (y = 650 > 200 + 150)
            shape("sp-a", Some("g-scrolled"), 650.0, Some("panel")),
            shape("sp-b", Some("g-scrolled"), 660.0, Some("panel")),
            shape("ctl-show-prevent", None, 10.0, None),
            shape("ctl-show-hide", None, 10.0, None),
            shape("ctl-hide-layers", None, 10.0, None),
            shape("ctl-disable-group", None, 10.0, None),
            shape("ctl-hide-group", None, 10.0, None),
        ],
        layers: vec![
            Layer {
                id: "l-prevent".into(),
                name: "Prevent".into(),
                duration_ms: 5000,
                objects: Vec::new(),
                triggers: Vec::new(),
                hide_other_layers: true,
                pause_base_timeline: false,
                prevent_base_clicks: true,
                hide_base_objects: false,
                hide_when_timeline_ends: false,
                allow_seek: coursecraft_course::SeekMode::Auto,
                revisit: RevisitMode::Auto,
                cue_points: Vec::new(),
                groups: Vec::new(),
            },
            Layer {
                id: "l-hide".into(),
                name: "Hide base".into(),
                duration_ms: 5000,
                objects: vec![
                    shape("lg-a", Some("g-layer"), 120.0, None),
                    shape("lg-b", Some("g-layer"), 120.0, None),
                ],
                triggers: Vec::new(),
                hide_other_layers: true,
                pause_base_timeline: false,
                prevent_base_clicks: false,
                hide_base_objects: true,
                hide_when_timeline_ends: false,
                allow_seek: coursecraft_course::SeekMode::Auto,
                revisit: RevisitMode::Auto,
                cue_points: Vec::new(),
                groups: vec![Group {
                    id: "g-layer".into(),
                    name: "Layer button".into(),
                    interactive: true,
                    initial_state: "normal".into(),
                    focus_order: None,
                    description: None,
                    entrance: None,
                    exit: None,
                    emphasis: Vec::new(),
                }],
            },
        ],
        groups: vec![
            Group {
                id: "g-btn".into(),
                name: "Main button".into(),
                interactive: true,
                initial_state: "normal".into(),
                focus_order: None,
                description: None,
                entrance: None,
                exit: None,
                emphasis: Vec::new(),
            },
            Group {
                id: "g-scrolled".into(),
                name: "Scrolled button".into(),
                interactive: true,
                initial_state: "normal".into(),
                focus_order: None,
                description: None,
                entrance: None,
                exit: None,
                emphasis: Vec::new(),
            },
        ],
        triggers: vec![
            Trigger {
                action: Action::SetVariable {
                    variable: "group_key".into(),
                    op: VarOp::Add,
                    value: Some(Value::Number(1.0).into()),
                },
                when: Event::KeyPressed {
                    key: "k".into(),
                    object: Some("g-btn".into()),
                },
                condition: None,
                disabled: false,
            },
            Trigger {
                action: Action::SetVariable {
                    variable: "space_key".into(),
                    op: VarOp::Add,
                    value: Some(Value::Number(1.0).into()),
                },
                when: Event::KeyPressed {
                    key: "space".into(),
                    object: None,
                },
                condition: None,
                disabled: false,
            },
            Trigger {
                action: Action::ShowLayer {
                    layer: "l-prevent".into(),
                },
                when: Event::Click {
                    object: "ctl-show-prevent".into(),
                },
                condition: None,
                disabled: false,
            },
            Trigger {
                action: Action::ShowLayer {
                    layer: "l-hide".into(),
                },
                when: Event::Click {
                    object: "ctl-show-hide".into(),
                },
                condition: None,
                disabled: false,
            },
            Trigger {
                action: Action::HideLayer {
                    layer: Some("l-prevent".into()),
                },
                when: Event::KeyPressed {
                    key: "h".into(),
                    object: None,
                },
                condition: None,
                disabled: false,
            },
            Trigger {
                action: Action::HideLayer {
                    layer: Some("l-hide".into()),
                },
                when: Event::KeyPressed {
                    key: "h".into(),
                    object: None,
                },
                condition: None,
                disabled: false,
            },
            Trigger {
                action: Action::ChangeState {
                    object: "g-btn".into(),
                    state: "disabled".into(),
                },
                when: Event::Click {
                    object: "ctl-disable-group".into(),
                },
                condition: None,
                disabled: false,
            },
            Trigger {
                action: Action::ChangeState {
                    object: "g-btn".into(),
                    state: "hidden".into(),
                },
                when: Event::Click {
                    object: "ctl-hide-group".into(),
                },
                condition: None,
                disabled: false,
            },
        ],
        cue_points: Vec::new(),
        question: None,
        draw: None,
        transition: None,
        notes: String::new(),
        master: None,
        revisit: RevisitMode::Auto,
        player: None,
    };

    let c = Course {
        schema_version: 1,
        id: "gb04".into(),
        title: "GB-04".into(),
        slide_size: coursecraft_course::Size::default(),
        variables: vec![
            VariableDef {
                name: "group_key".into(),
                value: Value::Number(0.0),
            },
            VariableDef {
                name: "space_key".into(),
                value: Value::Number(0.0),
            },
        ],
        scenes: vec![Scene {
            id: "sc1".into(),
            title: "Scene 1".into(),
            slides: vec![slide],
        }],
        masters: Vec::new(),
        templates: Vec::new(),
        banks: Vec::new(),
        start_slide: None,
        quiz: None,
        tracking: Default::default(),
        player: coursecraft_course::PlayerSettings::default(),
        theme: coursecraft_course::Theme::default(),
        language: None,
        strings: std::collections::BTreeMap::new(),
        fonts: Vec::new(),
        background_audio: None,
        text_styles: Vec::new(),
        resources: Vec::new(),
        glossary: Vec::new(),
        editor: coursecraft_course::EditorSettings::default(),
    };

    let mut p = Player::new(&c).expect("valid course");
    p.handle(&Input::Start);

    // Base keyboard order: g-btn appears once at its first member's position;
    // members m-btn/m-icon/m-lbl are omitted; g-scrolled is outside viewport.
    let kb = p.keyboard_objects();
    assert_eq!(kb.iter().filter(|id| id.as_str() == "g-btn").count(), 1);
    assert!(
        kb.iter()
            .position(|id| id == "btn-before")
            .zip(kb.iter().position(|id| id == "g-btn"))
            .is_some_and(|(a, b)| a < b),
        "g-btn must appear at its first member's position after btn-before"
    );
    for omitted in ["m-btn", "m-icon", "m-lbl", "sp-a", "sp-b", "g-scrolled"] {
        assert!(
            !kb.iter().any(|id| id == omitted),
            "{omitted} must not be in keyboard_objects"
        );
    }

    // Scrolling panel into view adds g-scrolled once (still omitting sp-a/sp-b)
    p.handle(&Input::PanelScrolled {
        object: "panel".into(),
        offset: 400.0,
    });
    let kb_scrolled = p.keyboard_objects();
    assert_eq!(
        kb_scrolled
            .iter()
            .filter(|id| id.as_str() == "g-scrolled")
            .count(),
        1
    );
    assert!(!kb_scrolled.iter().any(|id| id == "sp-a" || id == "sp-b"));

    // FocusChanged accepts interactive group ID and enables KeyPressed { object: Some(group_id) }
    p.handle(&Input::FocusChanged { object: None });
    p.handle(&Input::KeyPressed { key: "k".into() });
    assert_eq!(p.variable("group_key"), Some(&Value::Number(0.0)));

    p.handle(&Input::FocusChanged {
        object: Some("g-btn".into()),
    });
    p.handle(&Input::KeyPressed { key: "k".into() });
    assert_eq!(p.variable("group_key"), Some(&Value::Number(1.0)));

    // Space is owned by focused interactive group (like a Button)
    p.handle(&Input::KeyPressed {
        key: "space".into(),
    });
    assert_eq!(p.variable("space_key"), Some(&Value::Number(0.0)));

    // prevent_base_clicks excludes base interactive groups
    p.handle(&click("ctl-show-prevent"));
    assert!(!p.keyboard_objects().iter().any(|id| id == "g-btn"));

    // hide_base_objects excludes base groups and includes visible layer's interactive group once
    p.handle(&Input::KeyPressed { key: "h".into() });
    p.handle(&click("ctl-show-hide"));
    let kb_layer = p.keyboard_objects();
    assert_eq!(kb_layer, vec!["g-layer".to_owned()]);

    // Hide l-hide, then test disabled and hidden states on g-btn
    p.handle(&Input::KeyPressed { key: "h".into() });
    assert!(p.keyboard_objects().iter().any(|id| id == "g-btn"));
    p.handle(&click("ctl-disable-group"));
    assert!(!p.keyboard_objects().iter().any(|id| id == "g-btn"));
    p.handle(&click("ctl-hide-group"));
    assert!(!p.keyboard_objects().iter().any(|id| id == "g-btn"));
}

#[test]
fn gb_05_suspend_data_interactive_groups_and_golden_compat() {
    use coursecraft_course::{Color, Group, Layer, Rect, Scene, ShapeKind, StateDef, Timing};

    // 1. Pre-M37 golden compatibility: course with zero interactive groups is byte-identical.
    let hello = course();
    let mut hp = Player::new(&hello).expect("hello player");
    to_quiz(&mut hp);
    hp.handle(&Input::SelectChoice { choice: "b".into() });
    hp.handle(&click("btn-submit"));
    const GOLDEN_SUSPEND: &str = "cc1:AAIDAAECAQEEAAMBAwIDAwABAAEAAAAAAAAIQAECAQEK";
    assert_eq!(encode_suspend(&hello, hp.state()), GOLDEN_SUSPEND);

    // 2. Interactive group states (selected, visited, disabled, custom) round-trip cleanly.
    let make_obj = |id: &str, group: Option<&str>, states: Vec<StateDef>| Object {
        id: id.into(),
        name: id.into(),
        kind: ObjectKind::Shape {
            shape: ShapeKind::Rectangle,
            fill: Color("#2F6FEB".into()),
            fill_style: None,
            outline: None,
            text: None,
            adjust: Vec::new(),
        },
        frame: Rect {
            x: 50.0,
            y: 50.0,
            w: 120.0,
            h: 40.0,
        },
        timing: Timing::default(),
        states,
        initial_state: "normal".into(),
        entrance: None,
        exit: None,
        motion_paths: Vec::new(),
        emphasis: Vec::new(),
        focus_order: None,
        decorative: false,
        description: None,
        group: group.map(str::to_owned),
        panel: None,
        rotation: 0.0,
        flip_h: false,
        flip_v: false,
        opacity: 1.0,
        shadow: None,
        glow: None,
        drag: None,
        locked: false,
        editor_hidden: false,
    };
    let st = |name: &str| StateDef {
        name: name.into(),
        ..StateDef::default()
    };

    let slide = Slide {
        id: "s1".into(),
        title: "Suspend groups".into(),
        duration_ms: 5000,
        background: None,
        objects: vec![
            make_obj("a1", Some("g-sel"), vec![st("selected"), st("custom_gold")]),
            make_obj("a2", Some("g-sel"), vec![st("hover")]),
            make_obj("b1", Some("g-vis"), vec![st("visited")]),
            make_obj("b2", Some("g-vis"), Vec::new()),
            make_obj("c1", Some("g-dis"), Vec::new()),
            make_obj("c2", Some("g-dis"), Vec::new()),
            make_obj("btn-custom", None, Vec::new()),
            make_obj("btn-disable", None, Vec::new()),
            make_obj("btn-layer", None, Vec::new()),
        ],
        layers: vec![Layer {
            id: "l1".into(),
            name: "Layer 1".into(),
            duration_ms: 5000,
            objects: vec![
                make_obj("la", Some("g-layer"), vec![st("custom_done")]),
                make_obj("lb", Some("g-layer"), Vec::new()),
            ],
            triggers: vec![Trigger {
                action: Action::ChangeState {
                    object: "g-layer".into(),
                    state: "custom_done".into(),
                },
                when: Event::Click {
                    object: "g-layer".into(),
                },
                condition: None,
                disabled: false,
            }],
            hide_other_layers: true,
            pause_base_timeline: false,
            prevent_base_clicks: false,
            hide_base_objects: false,
            hide_when_timeline_ends: false,
            allow_seek: coursecraft_course::SeekMode::Auto,
            revisit: RevisitMode::Auto,
            cue_points: Vec::new(),
            groups: vec![Group {
                id: "g-layer".into(),
                name: "Layer button".into(),
                interactive: true,
                initial_state: "normal".into(),
                focus_order: None,
                description: None,
                entrance: None,
                exit: None,
                emphasis: Vec::new(),
            }],
        }],
        groups: vec![
            Group {
                id: "g-sel".into(),
                name: "Selectable".into(),
                interactive: true,
                initial_state: "normal".into(),
                focus_order: None,
                description: None,
                entrance: None,
                exit: None,
                emphasis: Vec::new(),
            },
            Group {
                id: "g-vis".into(),
                name: "Visitable".into(),
                interactive: true,
                initial_state: "normal".into(),
                focus_order: None,
                description: None,
                entrance: None,
                exit: None,
                emphasis: Vec::new(),
            },
            Group {
                id: "g-dis".into(),
                name: "Disableable".into(),
                interactive: true,
                initial_state: "normal".into(),
                focus_order: None,
                description: None,
                entrance: None,
                exit: None,
                emphasis: Vec::new(),
            },
        ],
        triggers: vec![
            Trigger {
                action: Action::ChangeState {
                    object: "g-sel".into(),
                    state: "custom_gold".into(),
                },
                when: Event::Click {
                    object: "btn-custom".into(),
                },
                condition: None,
                disabled: false,
            },
            Trigger {
                action: Action::ChangeState {
                    object: "g-dis".into(),
                    state: "disabled".into(),
                },
                when: Event::Click {
                    object: "btn-disable".into(),
                },
                condition: None,
                disabled: false,
            },
            Trigger {
                action: Action::ShowLayer { layer: "l1".into() },
                when: Event::Click {
                    object: "btn-layer".into(),
                },
                condition: None,
                disabled: false,
            },
        ],
        cue_points: Vec::new(),
        question: None,
        draw: None,
        transition: None,
        notes: String::new(),
        master: None,
        revisit: RevisitMode::Auto,
        player: None,
    };

    let c = Course {
        schema_version: 1,
        id: "gb05".into(),
        title: "GB-05".into(),
        slide_size: coursecraft_course::Size::default(),
        variables: Vec::new(),
        scenes: vec![Scene {
            id: "sc1".into(),
            title: "Scene 1".into(),
            slides: vec![slide],
        }],
        masters: Vec::new(),
        templates: Vec::new(),
        banks: Vec::new(),
        start_slide: None,
        quiz: None,
        tracking: Default::default(),
        player: coursecraft_course::PlayerSettings::default(),
        theme: coursecraft_course::Theme::default(),
        language: None,
        strings: std::collections::BTreeMap::new(),
        fonts: Vec::new(),
        background_audio: None,
        text_styles: Vec::new(),
        resources: Vec::new(),
        glossary: Vec::new(),
        editor: coursecraft_course::EditorSettings::default(),
    };

    // Case A: selected + visited + disabled + layer custom state
    let mut p = Player::new(&c).expect("valid course");
    p.handle(&Input::Start);
    p.handle(&click("g-sel")); // -> selected
    p.handle(&click("g-vis")); // -> visited
    p.handle(&click("btn-disable")); // -> disabled
    p.handle(&click("btn-layer"));
    p.handle(&click("g-layer")); // -> custom_done

    let encoded = encode_suspend(&c, p.state());
    let decoded = decode_suspend(&c, &encoded).expect("decode interactive group suspend");
    assert_eq!(decoded.object_states, p.state().object_states);

    let mut resumed = Player::resume(&c, decoded).expect("resume player");
    resumed.handle(&Input::Start);
    assert_eq!(resumed.object_state("g-sel"), Some("selected"));
    assert_eq!(resumed.object_state("a1"), Some("selected"));
    assert_eq!(resumed.object_state("a2"), Some("selected"));
    assert_eq!(resumed.object_state("g-vis"), Some("visited"));
    assert_eq!(resumed.object_state("b1"), Some("visited"));
    assert_eq!(resumed.object_state("b2"), Some("visited"));
    assert_eq!(resumed.object_state("g-dis"), Some("disabled"));
    assert_eq!(resumed.object_state("c1"), Some("disabled"));
    assert_eq!(resumed.object_state("c2"), Some("disabled"));
    assert_eq!(resumed.object_state("g-layer"), Some("custom_done"));
    assert_eq!(resumed.object_state("la"), Some("custom_done"));
    assert_eq!(resumed.object_state("lb"), Some("custom_done"));
    assert_eq!(encode_suspend(&c, resumed.state()), encoded);

    // Case B: custom state on base group also round-trips cleanly
    resumed.handle(&click("btn-custom"));
    assert_eq!(resumed.object_state("g-sel"), Some("custom_gold"));
    let encoded_custom = encode_suspend(&c, resumed.state());
    let decoded_custom = decode_suspend(&c, &encoded_custom).expect("decode custom group state");
    let mut resumed2 = Player::resume(&c, decoded_custom).expect("resume player 2");
    resumed2.handle(&Input::Start);
    assert_eq!(resumed2.object_state("g-sel"), Some("custom_gold"));
    assert_eq!(resumed2.object_state("a1"), Some("custom_gold"));
    assert_eq!(resumed2.object_state("a2"), Some("custom_gold"));
}

#[test]
fn tr_03_suspend_data_goldens_decode_and_reencode_byte_identically() {
    use coursecraft_course::{QuizTracking, SlidesViewedTracking, TrackingSettings};

    // 1. Pre-M44 golden for examples/hello-course/course.json (at s-quiz after answering).
    const HELLO_GOLDEN: &str = "cc1:AAIDAAECAQEEAAMBAwIDAwABAAEAAAAAAAAIQAECAQEK";
    let hello_default = course();
    let mut hello_tracked = course();
    hello_tracked.tracking = TrackingSettings {
        slides_viewed: Some(SlidesViewedTracking::AtLeast { count: 3 }),
        quiz: Some(QuizTracking {
            slide: Some("s-results".into()),
        }),
    };

    for c in [&hello_default, &hello_tracked] {
        let decoded = decode_suspend(c, HELLO_GOLDEN).expect("decode hello golden");
        assert_eq!(encode_suspend(c, &decoded), HELLO_GOLDEN);
    }

    // 2. Pre-M44 golden for a plain multi-slide course (3 slides, at slide 2, visited s1 & s2).
    const PLAIN_GOLDEN: &str = "cc1:AQIAAQAAAA";
    let plain_json = r#"{
        "schema_version": 1,
        "id": "plain-multi",
        "title": "Plain Multi",
        "slide_size": { "w": 1280, "h": 720 },
        "scenes": [{
            "id": "sc1",
            "title": "Scene 1",
            "slides": [
                { "id": "s1", "title": "S1", "duration_ms": 5000 },
                { "id": "s2", "title": "S2", "duration_ms": 5000 },
                { "id": "s3", "title": "S3", "duration_ms": 5000 }
            ]
        }]
    }"#;
    let plain_default = Course::from_json(plain_json).expect("plain course");
    let mut plain_tracked = plain_default.clone();
    plain_tracked.tracking = TrackingSettings {
        slides_viewed: Some(SlidesViewedTracking::All),
        quiz: Some(QuizTracking { slide: None }),
    };

    for c in [&plain_default, &plain_tracked] {
        let decoded = decode_suspend(c, PLAIN_GOLDEN).expect("decode plain golden");
        assert_eq!(decoded.slide, 1);
        assert_eq!(decoded.visited.len(), 2);
        assert_eq!(encode_suspend(c, &decoded), PLAIN_GOLDEN);
    }
}

fn tr_plain_course(slide_count: usize) -> Course {
    let slides_json: Vec<String> = (1..=slide_count)
        .map(|i| format!(r#"{{ "id": "s{i}", "title": "Slide {i}", "duration_ms": 5000 }}"#))
        .collect();
    let json = format!(
        r#"{{
            "schema_version": 1,
            "id": "tr-plain",
            "title": "TR Plain",
            "slide_size": {{ "w": 1280, "h": 720 }},
            "scenes": [{{
                "id": "sc1",
                "title": "Scene 1",
                "slides": [{}]
            }}]
        }}"#,
        slides_json.join(",")
    );
    Course::from_json(&json).expect("tr_plain_course")
}

fn tr_quiz_course(passing_percent: u8, attempts: u32, with_results_trigger: bool) -> Course {
    let results_triggers = if with_results_trigger {
        r#"[{ "when": { "event": "timeline_starts" }, "action": { "action": "submit_results" } }]"#
    } else {
        "[]"
    };
    let make_q = |id: &str| {
        format!(
            r#"{{
                "id": "{id}",
                "title": "{id}",
                "duration_ms": 5000,
                "question": {{
                    "kind": "multiple_choice",
                    "prompt": "Pick a",
                    "choices": [
                        {{ "id": "a", "text": "A", "correct": true }},
                        {{ "id": "b", "text": "B" }}
                    ],
                    "points": 10,
                    "attempts": {attempts}
                }},
                "objects": [{{
                    "id": "submit-{id}",
                    "kind": {{ "type": "button", "label": "Submit" }},
                    "frame": {{ "x": 100, "y": 100, "w": 120, "h": 40 }}
                }}],
                "triggers": [{{
                    "when": {{ "event": "click", "object": "submit-{id}" }},
                    "action": {{ "action": "submit_question" }}
                }}]
            }}"#
        )
    };
    let json = format!(
        r#"{{
            "schema_version": 1,
            "id": "tr-quiz",
            "title": "TR Quiz",
            "slide_size": {{ "w": 1280, "h": 720 }},
            "quiz": {{ "passing_percent": {passing_percent} }},
            "scenes": [{{
                "id": "sc1",
                "title": "Scene 1",
                "slides": [
                    {},
                    {},
                    {{
                        "id": "results",
                        "title": "Results",
                        "duration_ms": 5000,
                        "triggers": {results_triggers}
                    }}
                ]
            }}]
        }}"#,
        make_q("q1"),
        make_q("q2"),
    );
    Course::from_json(&json).expect("tr_quiz_course")
}

fn tr_button(id: &str, label: &str) -> Object {
    let json = format!(
        r#"{{
            "id": "{id}",
            "kind": {{ "type": "button", "label": "{label}" }},
            "frame": {{ "x": 10, "y": 10, "w": 100, "h": 40 }}
        }}"#
    );
    serde_json::from_str(&json).expect("tr_button")
}

#[test]
fn tr_04_default_tracking_preserves_triggers_and_example_traces() {
    use coursecraft_course::{SlidesViewedTracking, TrackingSettings};

    const SHOWCASE_JSON: &str = include_str!("../../../examples/showcase/course.json");
    const SHOWCASE_PASS: &str = include_str!("../../../examples/showcase/pass.inputs.json");

    // 1. Default tracking on hello-course and showcase produces expected trigger-only completions.
    let hello = course();
    assert!(hello.tracking.is_default());
    let hello_inputs: Vec<Input> = serde_json::from_str(PASS_INPUTS).expect("hello pass inputs");
    let mut p_hello = Player::new(&hello).expect("hello player");
    let mut hello_fx = Vec::new();
    for inp in &hello_inputs {
        hello_fx.extend(p_hello.handle(inp));
    }
    let hello_completions: Vec<_> = hello_fx
        .iter()
        .filter(|e| matches!(e, Effect::Completion { .. }))
        .collect();
    assert_eq!(
        hello_completions,
        vec![&Effect::Completion {
            status: CompletionStatus::Passed
        }]
    );

    let showcase = Course::from_json(SHOWCASE_JSON).expect("showcase course");
    assert!(showcase.tracking.is_default());
    let showcase_inputs: Vec<Input> =
        serde_json::from_str(SHOWCASE_PASS).expect("showcase pass inputs");
    let mut p_show = Player::new(&showcase).expect("showcase player");
    let mut show_fx = Vec::new();
    for inp in &showcase_inputs {
        show_fx.extend(p_show.handle(inp));
    }
    assert!(show_fx.iter().any(|e| matches!(
        e,
        Effect::Completion {
            status: CompletionStatus::Passed
        }
    )));

    // 2. Explicit complete_course trigger works alongside slides_viewed.
    let mut c = tr_plain_course(3);
    c.tracking = TrackingSettings {
        slides_viewed: Some(SlidesViewedTracking::All),
        quiz: None,
    };
    c.scenes[0].slides[0]
        .objects
        .push(tr_button("btn-done", "Done"));
    c.scenes[0].slides[0].triggers.push(Trigger {
        when: Event::Click {
            object: "btn-done".into(),
        },
        action: Action::CompleteCourse {
            status: CompletionStatus::Completed,
        },
        condition: None,
        disabled: false,
    });

    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    let fx = p.handle(&click("btn-done"));
    assert_eq!(
        fx,
        vec![Effect::Completion {
            status: CompletionStatus::Completed
        }]
    );
}

#[test]
fn tr_05_slides_viewed_runtime_rules() {
    use coursecraft_course::{SlidesViewedTracking, TrackingSettings};

    // (a) All on 3-slide course completes once on entering slide 3 (no duplicate on revisit).
    let mut c = tr_plain_course(3);
    c.tracking.slides_viewed = Some(SlidesViewedTracking::All);
    let mut p = Player::new(&c).expect("player");
    assert!(
        !p.handle(&Input::Start)
            .iter()
            .any(|e| matches!(e, Effect::Completion { .. }))
    );
    assert!(
        !p.handle(&Input::Next)
            .iter()
            .any(|e| matches!(e, Effect::Completion { .. }))
    );
    let fx3 = p.handle(&Input::Next);
    assert!(has(
        &fx3,
        &Effect::Completion {
            status: CompletionStatus::Completed
        }
    ));
    assert!(!has(&fx3, &Effect::Exit));
    assert!(p.state().slides_tracking_reported);
    // Revisit s2 and s3: no duplicate Completion.
    assert!(
        !p.handle(&Input::Previous)
            .iter()
            .any(|e| matches!(e, Effect::Completion { .. }))
    );
    assert!(
        !p.handle(&Input::Next)
            .iter()
            .any(|e| matches!(e, Effect::Completion { .. }))
    );

    // (b) AtLeast { count: 2 } on 4-slide course with unreachable scene completes on slide 2.
    let mut c4 = Course::from_json(
        r#"{
            "schema_version": 1,
            "id": "tr-c4",
            "title": "TR C4",
            "slide_size": { "w": 1280, "h": 720 },
            "scenes": [
                {
                    "id": "sc1",
                    "title": "Scene 1",
                    "slides": [
                        { "id": "s1", "title": "S1", "duration_ms": 5000 },
                        { "id": "s2", "title": "S2", "duration_ms": 5000 }
                    ]
                },
                {
                    "id": "sc-unreachable",
                    "title": "Unreachable",
                    "slides": [
                        { "id": "u1", "title": "U1", "duration_ms": 5000 },
                        { "id": "u2", "title": "U2", "duration_ms": 5000 }
                    ]
                }
            ]
        }"#,
    )
    .expect("c4");
    c4.tracking.slides_viewed = Some(SlidesViewedTracking::AtLeast { count: 2 });
    let mut p4 = Player::new(&c4).expect("player");
    assert!(
        !p4.handle(&Input::Start)
            .iter()
            .any(|e| matches!(e, Effect::Completion { .. }))
    );
    let fx_u = p4.handle(&Input::Next);
    assert!(has(
        &fx_u,
        &Effect::Completion {
            status: CompletionStatus::Completed
        }
    ));

    // (c) AtLeast { count: 10 } on 3-slide course clamps to 3 and completes when all 3 viewed.
    let mut c_clamp = tr_plain_course(3);
    c_clamp.tracking.slides_viewed = Some(SlidesViewedTracking::AtLeast { count: 10 });
    let mut p_clamp = Player::new_unvalidated(&c_clamp);
    p_clamp.handle(&Input::Start);
    p_clamp.handle(&Input::Next);
    let fx_clamp = p_clamp.handle(&Input::Next);
    assert!(has(
        &fx_clamp,
        &Effect::Completion {
            status: CompletionStatus::Completed
        }
    ));

    // (d) 1-slide course with All completes on Input::Start.
    let mut c1 = tr_plain_course(1);
    c1.tracking.slides_viewed = Some(SlidesViewedTracking::All);
    let mut p1 = Player::new(&c1).expect("player");
    let fx1 = p1.handle(&Input::Start);
    assert!(has(
        &fx1,
        &Effect::Completion {
            status: CompletionStatus::Completed
        }
    ));

    // (e) Lightboxed scene slide counts as visited.
    let mut c_lb = tr_plain_course(2);
    c_lb.tracking.slides_viewed = Some(SlidesViewedTracking::All);
    c_lb.scenes[0].slides[0]
        .objects
        .push(tr_button("btn-lb", "LB"));
    c_lb.scenes[0].slides[0].triggers.push(Trigger {
        when: Event::Click {
            object: "btn-lb".into(),
        },
        action: Action::LightboxSlide { slide: "s2".into() },
        condition: None,
        disabled: false,
    });
    let mut p_lb = Player::new(&c_lb).expect("player");
    p_lb.handle(&Input::Start);
    let fx_lb = p_lb.handle(&click("btn-lb"));
    assert!(has(
        &fx_lb,
        &Effect::Completion {
            status: CompletionStatus::Completed
        }
    ));

    // (f) Bank-draw course counts expanded self.slides.len(), never masters/templates/undrawn bank slides.
    let mut c_bank = Course::from_json(
        r#"{
            "schema_version": 1,
            "id": "bank-tr",
            "title": "Bank TR",
            "slide_size": { "w": 1280, "h": 720 },
            "masters": [{ "id": "m1", "title": "Master", "duration_ms": 5000 }],
            "templates": [{ "id": "t1", "title": "Template", "duration_ms": 5000 }],
            "banks": [{
                "id": "b1",
                "title": "Bank",
                "slides": [
                    { "id": "bs1", "title": "BS1", "duration_ms": 5000, "question": { "kind": "multiple_choice", "prompt": "Q1", "choices": [{ "id": "a", "text": "A", "correct": true }, { "id": "b", "text": "B" }] } },
                    { "id": "bs2", "title": "BS2", "duration_ms": 5000, "question": { "kind": "multiple_choice", "prompt": "Q2", "choices": [{ "id": "a", "text": "A", "correct": true }, { "id": "b", "text": "B" }] } },
                    { "id": "bs3", "title": "BS3", "duration_ms": 5000, "question": { "kind": "multiple_choice", "prompt": "Q3", "choices": [{ "id": "a", "text": "A", "correct": true }, { "id": "b", "text": "B" }] } }
                ]
            }],
            "scenes": [{
                "id": "sc1",
                "title": "Scene 1",
                "slides": [
                    { "id": "intro", "title": "Intro", "duration_ms": 5000 },
                    { "id": "draw", "title": "Draw", "duration_ms": 5000, "draw": { "bank": "b1", "count": 1 } }
                ]
            }]
        }"#,
    )
    .expect("bank course");
    c_bank.tracking = TrackingSettings {
        slides_viewed: Some(SlidesViewedTracking::All),
        quiz: None,
    };
    let mut p_bank = Player::new(&c_bank).expect("bank player");
    assert_eq!(p_bank.play_order().len(), 2);
    assert!(
        !p_bank
            .handle(&Input::Start)
            .iter()
            .any(|e| matches!(e, Effect::Completion { .. }))
    );
    let fx_bank = p_bank.handle(&Input::Next);
    assert!(has(
        &fx_bank,
        &Effect::Completion {
            status: CompletionStatus::Completed
        }
    ));

    // (g) Partial preview_subset does not falsely complete on slide 1.
    let mut p_sub = Player::with_preview_subset(&c, 0, &["s1".to_owned()]).expect("subset");
    let fx_sub = p_sub.handle(&Input::Start);
    assert!(
        !fx_sub
            .iter()
            .any(|e| matches!(e, Effect::Completion { .. }))
    );
    assert!(!p_sub.state().slides_tracking_reported);
}

#[test]
fn tr_06_quiz_result_runtime_rules() {
    use coursecraft_course::{QuizTracking, TrackingSettings};

    // (a) QuizTracking { slide: Some("results") } with SubmitResults trigger emits Score + Completion once.
    let mut c_trig = tr_quiz_course(80, 1, true);
    c_trig.tracking.quiz = Some(QuizTracking {
        slide: Some("results".into()),
    });
    let mut p_a = Player::new(&c_trig).expect("player");
    p_a.handle(&Input::Start);
    p_a.handle(&Input::SelectChoice { choice: "a".into() });
    p_a.handle(&click("submit-q1"));
    p_a.handle(&Input::Next);
    p_a.handle(&Input::SelectChoice { choice: "a".into() });
    p_a.handle(&click("submit-q2"));
    let fx_a = p_a.handle(&Input::Next);
    let score_count = fx_a
        .iter()
        .filter(|e| matches!(e, Effect::Score { .. }))
        .count();
    let comp_count = fx_a
        .iter()
        .filter(|e| matches!(e, Effect::Completion { .. }))
        .count();
    assert_eq!(score_count, 1);
    assert_eq!(comp_count, 1);
    assert_eq!(p_a.state().quiz_attempts, 1);

    // (b) Custom results slide without submit_results trigger reports on entry.
    let mut c_custom = tr_quiz_course(80, 1, false);
    c_custom.tracking.quiz = Some(QuizTracking {
        slide: Some("results".into()),
    });
    let mut p_b = Player::new(&c_custom).expect("player");
    p_b.handle(&Input::Start);
    p_b.handle(&Input::SelectChoice { choice: "a".into() });
    p_b.handle(&click("submit-q1"));
    p_b.handle(&Input::Next);
    p_b.handle(&Input::SelectChoice { choice: "b".into() }); // wrong -> 50% < 80%
    p_b.handle(&click("submit-q2"));
    let fx_b = p_b.handle(&Input::Next);
    assert!(has(
        &fx_b,
        &Effect::Score {
            raw: 10,
            max: 20,
            percent: 50,
            status: CompletionStatus::Failed,
        }
    ));
    assert!(has(
        &fx_b,
        &Effect::Completion {
            status: CompletionStatus::Failed
        }
    ));
    assert_eq!(p_b.state().quiz_attempts, 1);

    // (c) QuizTracking { slide: None } waits until all graded questions lock (including multi-attempt).
    let mut c_auto = tr_quiz_course(80, 2, false);
    c_auto.tracking.quiz = Some(QuizTracking { slide: None });
    let mut p_c = Player::new(&c_auto).expect("player");
    p_c.handle(&Input::Start);
    // q1: wrong on attempt 1 (still unlocked, attempts_left = 1) -> no Score yet.
    p_c.handle(&Input::SelectChoice { choice: "b".into() });
    let fx_q1_try1 = p_c.handle(&click("submit-q1"));
    assert!(!fx_q1_try1.iter().any(|e| matches!(e, Effect::Score { .. })));
    // q1: right on attempt 2 (locked), q2 still unlocked -> no Score yet.
    p_c.handle(&Input::SelectChoice { choice: "a".into() });
    let fx_q1_try2 = p_c.handle(&click("submit-q1"));
    assert!(!fx_q1_try2.iter().any(|e| matches!(e, Effect::Score { .. })));
    // q2: wrong on attempt 1 (unlocked) -> no Score yet.
    p_c.handle(&Input::Next);
    p_c.handle(&Input::SelectChoice { choice: "b".into() });
    let fx_q2_try1 = p_c.handle(&click("submit-q2"));
    assert!(!fx_q2_try1.iter().any(|e| matches!(e, Effect::Score { .. })));
    // q2: right on attempt 2 (all graded questions now locked!) -> Score(100, Passed).
    p_c.handle(&Input::SelectChoice { choice: "a".into() });
    let fx_q2_try2 = p_c.handle(&click("submit-q2"));
    assert!(has(
        &fx_q2_try2,
        &Effect::Score {
            raw: 20,
            max: 20,
            percent: 100,
            status: CompletionStatus::Passed,
        }
    ));
    assert!(has(
        &fx_q2_try2,
        &Effect::Completion {
            status: CompletionStatus::Passed
        }
    ));

    // (d) retry_quiz unlocks questions and re-evaluates on next submit.
    c_auto.scenes[0].slides[2]
        .objects
        .push(tr_button("btn-retry", "Retry"));
    c_auto.scenes[0].slides[2].triggers.push(Trigger {
        when: Event::Click {
            object: "btn-retry".into(),
        },
        action: Action::RetryQuiz,
        condition: None,
        disabled: false,
    });
    let mut p_d = Player::new(&c_auto).expect("player");
    p_d.handle(&Input::Start);
    p_d.handle(&Input::SelectChoice { choice: "b".into() });
    p_d.handle(&click("submit-q1"));
    p_d.handle(&Input::SelectChoice { choice: "b".into() });
    p_d.handle(&click("submit-q1")); // q1 locked wrong
    p_d.handle(&Input::Next);
    p_d.handle(&Input::SelectChoice { choice: "b".into() });
    p_d.handle(&click("submit-q2"));
    p_d.handle(&Input::SelectChoice { choice: "b".into() });
    let fx_fail = p_d.handle(&click("submit-q2"));
    assert!(has(
        &fx_fail,
        &Effect::Completion {
            status: CompletionStatus::Failed
        }
    ));
    p_d.handle(&Input::Next); // to results
    p_d.handle(&click("btn-retry")); // jumps back to q1 with answers cleared
    p_d.handle(&Input::SelectChoice { choice: "a".into() });
    p_d.handle(&click("submit-q1"));
    p_d.handle(&Input::Next);
    p_d.handle(&Input::SelectChoice { choice: "a".into() });
    let fx_pass = p_d.handle(&click("submit-q2"));
    assert!(has(
        &fx_pass,
        &Effect::Completion {
            status: CompletionStatus::Passed
        }
    ));
    assert_eq!(p_d.state().quiz_attempts, 2);

    // (e) Survey-only / course.quiz: None degrades safely without panic.
    let mut c_survey = tr_plain_course(2);
    c_survey.tracking = TrackingSettings {
        slides_viewed: None,
        quiz: Some(QuizTracking {
            slide: Some("s2".into()),
        }),
    };
    let mut p_e = Player::new_unvalidated(&c_survey);
    p_e.handle(&Input::Start);
    let fx_e = p_e.handle(&Input::Next);
    assert!(has(
        &fx_e,
        &Effect::Score {
            raw: 0,
            max: 0,
            percent: 100,
            status: CompletionStatus::Completed,
        }
    ));
    assert!(has(
        &fx_e,
        &Effect::Completion {
            status: CompletionStatus::Completed
        }
    ));

    // (f) Ungraded survey question submitted after all graded questions lock does NOT re-emit Score.
    let mut c_mixed = Course::from_json(
        r#"{
            "schema_version": 1,
            "id": "tr-mixed",
            "title": "TR Mixed",
            "slide_size": { "w": 1280, "h": 720 },
            "quiz": { "passing_percent": 80 },
            "scenes": [{
                "id": "sc1",
                "title": "Scene 1",
                "slides": [
                    {
                        "id": "q1",
                        "title": "Graded",
                        "duration_ms": 5000,
                        "question": {
                            "kind": "multiple_choice",
                            "prompt": "Pick a",
                            "choices": [
                                { "id": "a", "text": "A", "correct": true },
                                { "id": "b", "text": "B" }
                            ],
                            "points": 10,
                            "attempts": 1
                        },
                        "objects": [{
                            "id": "submit-q1",
                            "kind": { "type": "button", "label": "Submit" },
                            "frame": { "x": 100, "y": 100, "w": 120, "h": 40 }
                        }],
                        "triggers": [{
                            "when": { "event": "click", "object": "submit-q1" },
                            "action": { "action": "submit_question" }
                        }]
                    },
                    {
                        "id": "survey",
                        "title": "Survey",
                        "duration_ms": 5000,
                        "question": {
                            "kind": "likert",
                            "prompt": "Rate",
                            "choices": [
                                { "id": "s1", "text": "Good" },
                                { "id": "s2", "text": "Great" }
                            ],
                            "points": 0,
                            "attempts": 1
                        },
                        "objects": [{
                            "id": "submit-survey",
                            "kind": { "type": "button", "label": "Submit" },
                            "frame": { "x": 100, "y": 100, "w": 120, "h": 40 }
                        }],
                        "triggers": [{
                            "when": { "event": "click", "object": "submit-survey" },
                            "action": { "action": "submit_question" }
                        }]
                    }
                ]
            }]
        }"#,
    )
    .expect("mixed course");
    c_mixed.tracking.quiz = Some(QuizTracking { slide: None });
    let mut p_f = Player::new(&c_mixed).expect("player");
    p_f.handle(&Input::Start);
    p_f.handle(&Input::SelectChoice { choice: "a".into() });
    let fx_f_graded = p_f.handle(&click("submit-q1"));
    assert!(
        fx_f_graded
            .iter()
            .any(|e| matches!(e, Effect::Score { .. }))
    );
    assert_eq!(p_f.state().quiz_attempts, 1);
    p_f.handle(&Input::Next);
    p_f.handle(&Input::SelectChoice {
        choice: "s1".into(),
    });
    let fx_f_survey = p_f.handle(&click("submit-survey"));
    assert!(
        !fx_f_survey
            .iter()
            .any(|e| matches!(e, Effect::Score { .. })),
        "answering an ungraded survey question must not re-report quiz results"
    );
    assert_eq!(p_f.state().quiz_attempts, 1);

    // (g) QuizTracking targeting a question slide without SubmitResults reports when locked;
    //     saturated quiz_attempts == u32::MAX still prevents duplicate SubmitResults in same input.
    let mut c_qslide = tr_quiz_course(80, 1, false);
    c_qslide.tracking.quiz = Some(QuizTracking {
        slide: Some("q2".into()),
    });
    let mut p_g = Player::new(&c_qslide).expect("player");
    p_g.handle(&Input::Start);
    p_g.handle(&Input::SelectChoice { choice: "a".into() });
    assert!(
        !p_g.handle(&click("submit-q1"))
            .iter()
            .any(|e| matches!(e, Effect::Score { .. }))
    );
    p_g.handle(&Input::Next);
    p_g.handle(&Input::SelectChoice { choice: "a".into() });
    let fx_g = p_g.handle(&click("submit-q2"));
    assert!(has(
        &fx_g,
        &Effect::Score {
            raw: 20,
            max: 20,
            percent: 100,
            status: CompletionStatus::Passed,
        }
    ));

    let mut c_sat = tr_quiz_course(80, 1, false);
    c_sat.tracking.quiz = Some(QuizTracking { slide: None });
    c_sat.scenes[0].slides[1].triggers.push(Trigger {
        when: Event::Click {
            object: "submit-q2".into(),
        },
        action: Action::SubmitResults,
        condition: None,
        disabled: false,
    });
    let mut st_sat = Player::new(&c_sat).expect("player").into_state();
    st_sat.quiz_attempts = u32::MAX;
    let mut p_sat = Player::resume(&c_sat, st_sat).expect("resume saturated");
    p_sat.handle(&Input::Start);
    p_sat.handle(&Input::SelectChoice { choice: "a".into() });
    p_sat.handle(&click("submit-q1"));
    p_sat.handle(&Input::Next);
    p_sat.handle(&Input::SelectChoice { choice: "a".into() });
    let fx_sat = p_sat.handle(&click("submit-q2"));
    assert_eq!(
        fx_sat
            .iter()
            .filter(|e| matches!(e, Effect::Score { .. }))
            .count(),
        1,
        "SubmitResults with saturated quiz_attempts must not duplicate Score"
    );
}

#[test]
fn tr_07_combined_options_and_resume() {
    use coursecraft_course::{QuizTracking, SlidesViewedTracking, TrackingSettings};

    // (a) Pass quiz early -> Score(Passed) + Completion(Passed), then view remaining slides -> Completion(Completed) once.
    let mut c = tr_quiz_course(80, 1, false);
    c.tracking = TrackingSettings {
        slides_viewed: Some(SlidesViewedTracking::All),
        quiz: Some(QuizTracking { slide: None }),
    };
    let mut p_a = Player::new(&c).expect("player");
    p_a.handle(&Input::Start);
    p_a.handle(&Input::SelectChoice { choice: "a".into() });
    p_a.handle(&click("submit-q1"));
    p_a.handle(&Input::Next);
    p_a.handle(&Input::SelectChoice { choice: "a".into() });
    let fx_q2 = p_a.handle(&click("submit-q2"));
    assert!(has(
        &fx_q2,
        &Effect::Score {
            raw: 20,
            max: 20,
            percent: 100,
            status: CompletionStatus::Passed,
        }
    ));
    assert!(has(
        &fx_q2,
        &Effect::Completion {
            status: CompletionStatus::Passed
        }
    ));
    let fx_last = p_a.handle(&Input::Next);
    assert_eq!(
        fx_last
            .iter()
            .filter(|e| matches!(e, Effect::Completion { .. }))
            .collect::<Vec<_>>(),
        vec![&Effect::Completion {
            status: CompletionStatus::Completed
        }]
    );
    // Revisiting does not re-emit Completion(Completed).
    assert!(
        !p_a.handle(&Input::Previous)
            .iter()
            .any(|e| matches!(e, Effect::Completion { .. }))
    );

    // (b) Fail quiz first, then view last slide -> Score(Failed) + Completion(Failed), then Completion(Completed).
    let mut p_b = Player::new(&c).expect("player");
    p_b.handle(&Input::Start);
    p_b.handle(&Input::SelectChoice { choice: "b".into() });
    p_b.handle(&click("submit-q1"));
    p_b.handle(&Input::Next);
    p_b.handle(&Input::SelectChoice { choice: "b".into() });
    let fx_b_fail = p_b.handle(&click("submit-q2"));
    assert!(has(
        &fx_b_fail,
        &Effect::Completion {
            status: CompletionStatus::Failed
        }
    ));
    let fx_b_last = p_b.handle(&Input::Next);
    assert!(has(
        &fx_b_last,
        &Effect::Completion {
            status: CompletionStatus::Completed
        }
    ));

    // (c) View all slides first, then fail quiz.
    let mut p_c = Player::new(&c).expect("player");
    p_c.handle(&Input::Start); // q1
    p_c.handle(&Input::Next); // q2
    let fx_c_last = p_c.handle(&Input::Next); // results -> all 3 viewed!
    assert!(has(
        &fx_c_last,
        &Effect::Completion {
            status: CompletionStatus::Completed
        }
    ));
    p_c.handle(&Input::Previous); // q2
    p_c.handle(&Input::SelectChoice { choice: "b".into() });
    p_c.handle(&click("submit-q2"));
    p_c.handle(&Input::Previous); // q1
    p_c.handle(&Input::SelectChoice { choice: "b".into() });
    let fx_c_fail = p_c.handle(&click("submit-q1"));
    assert!(has(
        &fx_c_fail,
        &Effect::Score {
            raw: 0,
            max: 20,
            percent: 0,
            status: CompletionStatus::Failed,
        }
    ));
    assert!(has(
        &fx_c_fail,
        &Effect::Completion {
            status: CompletionStatus::Failed
        }
    ));

    // (d) Enter last slide which is also a failed results slide -> Score(Failed), Completion(Failed), Completion(Completed).
    let mut c_d = tr_quiz_course(80, 1, true);
    c_d.tracking = TrackingSettings {
        slides_viewed: Some(SlidesViewedTracking::All),
        quiz: Some(QuizTracking {
            slide: Some("results".into()),
        }),
    };
    let mut p_d = Player::new(&c_d).expect("player");
    p_d.handle(&Input::Start);
    p_d.handle(&Input::SelectChoice { choice: "b".into() });
    p_d.handle(&click("submit-q1"));
    p_d.handle(&Input::Next);
    p_d.handle(&Input::SelectChoice { choice: "b".into() });
    p_d.handle(&click("submit-q2"));
    let fx_d = p_d.handle(&Input::Next);
    let verdicts: Vec<_> = fx_d
        .iter()
        .filter(|e| matches!(e, Effect::Score { .. } | Effect::Completion { .. }))
        .collect();
    assert_eq!(
        verdicts,
        vec![
            &Effect::Score {
                raw: 0,
                max: 20,
                percent: 0,
                status: CompletionStatus::Failed,
            },
            &Effect::Completion {
                status: CompletionStatus::Failed
            },
            &Effect::Completion {
                status: CompletionStatus::Completed
            },
        ]
    );

    // (e) Suspend at 2/3 slides -> resume -> no premature completion on Start, completes on slide 3.
    let mut c_res = tr_plain_course(3);
    c_res.tracking.slides_viewed = Some(SlidesViewedTracking::All);
    let mut p_e = Player::new(&c_res).expect("player");
    p_e.handle(&Input::Start);
    p_e.handle(&Input::Next);
    let sus_e = encode_suspend(&c_res, p_e.state());
    let dec_e = decode_suspend(&c_res, &sus_e).expect("decode e");
    let mut res_e = Player::resume(&c_res, dec_e).expect("resume e");
    assert!(!res_e.state().slides_tracking_reported);
    let fx_e_start = res_e.handle(&Input::Start);
    assert!(
        !fx_e_start
            .iter()
            .any(|e| matches!(e, Effect::Completion { .. }))
    );
    let fx_e_s3 = res_e.handle(&Input::Next);
    assert!(has(
        &fx_e_s3,
        &Effect::Completion {
            status: CompletionStatus::Completed
        }
    ));

    // (f) Suspend at 3/3 slides -> resume -> slides_tracking_reported == true, no duplicate completion on Start.
    let sus_f = encode_suspend(&c_res, res_e.state());
    let dec_f = decode_suspend(&c_res, &sus_f).expect("decode f");
    let mut res_f = Player::resume(&c_res, dec_f).expect("resume f");
    assert!(res_f.state().slides_tracking_reported);
    let fx_f_start = res_f.handle(&Input::Start);
    assert!(
        !fx_f_start
            .iter()
            .any(|e| matches!(e, Effect::Completion { .. }))
    );
}

#[test]
fn audio_stops_when_visible_window_ends() {
    let json = serde_json::json!({
        "schema_version": 1,
        "id": "window-end",
        "title": "Window End",
        "scenes": [{
            "id": "sc1",
            "title": "Scene 1",
            "slides": [{
                "id": "s1",
                "title": "Slide 1",
                "duration_ms": 10000,
                "objects": [
                    {
                        "id": "aud_crop",
                        "kind": { "type": "audio", "src": "crop.mp3", "autoplay": true },
                        "frame": { "x": 0.0, "y": 0.0, "w": 48.0, "h": 48.0 },
                        "timing": { "start_ms": 0, "end_ms": 5000 }
                    },
                    {
                        "id": "aud_exit",
                        "kind": { "type": "audio", "src": "exit.mp3", "autoplay": true },
                        "frame": { "x": 0.0, "y": 0.0, "w": 48.0, "h": 48.0 },
                        "timing": { "start_ms": 0 },
                        "exit": { "effect": { "type": "fade" }, "duration_ms": 500 }
                    },
                    {
                        "id": "aud_open",
                        "kind": { "type": "audio", "src": "open.mp3", "autoplay": true },
                        "frame": { "x": 0.0, "y": 0.0, "w": 48.0, "h": 48.0 },
                        "timing": { "start_ms": 0 }
                    }
                ],
                "triggers": [
                    {
                        "when": { "event": "media_ends", "object": "aud_crop" },
                        "action": { "action": "play_media", "object": "aud_crop" }
                    }
                ]
            }]
        }]
    })
    .to_string();
    let c = Course::from_json(&json).expect("valid course");
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    assert!(p.is_media_playing("aud_crop"));
    assert!(p.is_media_playing("aud_exit"));
    assert!(p.is_media_playing("aud_open"));

    let fx_mid = p.handle(&Input::Tick { ms: 4000 });
    assert!(!has(&fx_mid, &media_effect("aud_crop", MediaOp::Stop)));
    assert!(p.is_media_playing("aud_crop"));

    // Crossing 5000 ms stops aud_crop without firing media_ends (so its loop trigger does not restart it).
    let fx_end = p.handle(&Input::Tick { ms: 1000 });
    assert!(has(&fx_end, &media_effect("aud_crop", MediaOp::Stop)));
    assert!(!has(&fx_end, &media_effect("aud_crop", MediaOp::Play)));
    assert!(!p.is_media_playing("aud_crop"));
    assert!(p.is_media_playing("aud_exit"));
    assert!(p.is_media_playing("aud_open"));

    // Reaching 10000 ms stops aud_exit (has exit) while aud_open (no end_ms, no exit) stays playing.
    let fx_tl = p.handle(&Input::Tick { ms: 5000 });
    assert!(has(&fx_tl, &media_effect("aud_exit", MediaOp::Stop)));
    assert!(!p.is_media_playing("aud_exit"));
    assert!(!has(&fx_tl, &media_effect("aud_open", MediaOp::Stop)));
    assert!(p.is_media_playing("aud_open"));
}

#[test]
fn audio_mid_bar_seek_uses_trim_start_offset_and_after_bar_plays_nothing() {
    let mut c = media_test_course();
    c.scenes[0].slides[0].duration_ms = 10000;
    if let Some(obj) = c.scenes[0].slides[0]
        .objects
        .iter_mut()
        .find(|o| o.id == "aud_auto")
    {
        obj.timing.start_ms = 1000;
        obj.timing.end_ms = Some(5000);
        if let ObjectKind::Audio { trim, .. } = &mut obj.kind {
            *trim = Some(Trim {
                start_ms: 2000,
                end_ms: None,
            });
        }
    }
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);

    let fx = p.handle(&Input::PlayerSeek { ms: 3500 });
    assert!(has(&fx, &media_effect("aud_auto", MediaOp::Play)), "{fx:?}");
    assert!(has(&fx, &seek_effect("aud_auto", 4500)), "{fx:?}");
    assert!(p.is_media_playing("aud_auto"));

    // Seeking to bar end stops it.
    let fx = p.handle(&Input::PlayerSeek { ms: 5000 });
    assert!(has(&fx, &media_effect("aud_auto", MediaOp::Stop)), "{fx:?}");
    assert!(!p.is_media_playing("aud_auto"));

    // Seeking past bar end while untracked starts nothing.
    let fx = p.handle(&Input::PlayerSeek { ms: 6000 });
    assert!(!fx.iter().any(|e| matches!(
        e,
        Effect::Media { object, .. } | Effect::MediaSeek { object, .. } if object == "aud_auto"
    )));
    assert!(!p.is_media_playing("aud_auto"));
}

fn neon_modal_course() -> Course {
    Course::from_json(
        &serde_json::json!({
            "schema_version": 1,
            "id": "neon-modal",
            "title": "Neon Modal",
            "variables": [
                { "name": "t", "value": "" },
                { "name": "hit", "value": 0.0 }
            ],
            "scenes": [{ "id": "sc", "title": "Scene", "slides": [{
                "id": "s1", "title": "One",
                "objects": [
                    { "id": "open", "kind": { "type": "button", "label": "Open" },
                      "frame": { "x": 0.0, "y": 0.0, "w": 40.0, "h": 40.0 } },
                    { "id": "entry", "kind": { "type": "text_entry", "variable": "t" },
                      "frame": { "x": 0.0, "y": 100.0, "w": 100.0, "h": 40.0 } },
                    { "id": "card", "kind": { "type": "shape", "shape": "rectangle", "fill": "#112233" },
                      "frame": { "x": 0.0, "y": 200.0, "w": 40.0, "h": 40.0 },
                      "drag": { "targets": ["bin"] } },
                    { "id": "off", "initial_state": "disabled",
                      "states": [{ "name": "disabled" }],
                      "kind": { "type": "shape", "shape": "rectangle", "fill": "#112233" },
                      "frame": { "x": 0.0, "y": 300.0, "w": 40.0, "h": 40.0 },
                      "drag": { "targets": ["bin"] } },
                    { "id": "bin", "kind": { "type": "shape", "shape": "rectangle", "fill": "#334455" },
                      "frame": { "x": 200.0, "y": 0.0, "w": 80.0, "h": 400.0 } }
                ],
                "layers": [{ "id": "modal", "name": "Modal", "prevent_base_clicks": true, "objects": [] }],
                "triggers": [
                    { "action": { "action": "show_layer", "layer": "modal" },
                      "when": { "event": "click", "object": "open" } },
                    { "action": { "action": "set_variable", "variable": "hit", "value": 1 },
                      "when": { "event": "dropped_on", "object": "card", "target": "bin" } },
                    { "action": { "action": "set_variable", "variable": "hit", "value": 1 },
                      "when": { "event": "dropped_on", "object": "off", "target": "bin" } }
                ]
            }] }]
        })
        .to_string(),
    )
    .expect("valid course")
}

fn neon_set_text(p: &mut Player<'_>) -> Vec<Effect> {
    p.handle(&Input::SetInputValue {
        object: "entry".into(),
        value: Value::Text("Alice".into()),
    })
}

fn neon_drop(p: &mut Player<'_>, object: &str, y: f32) -> Vec<Effect> {
    p.handle(&Input::ObjectDropped {
        object: object.into(),
        x: 210.0,
        y,
    })
}

#[test]
fn n7_a_base_input_ignores_set_input_value_under_a_modal_layer() {
    let c = neon_modal_course();
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    neon_set_text(&mut p);
    assert_eq!(p.variable("t"), Some(&Value::Text("Alice".into())));

    p.handle(&click("open"));
    let effects = p.handle(&Input::SetInputValue {
        object: "entry".into(),
        value: Value::Text("Bob".into()),
    });
    assert!(
        effects.is_empty(),
        "silent, like a blocked click: {effects:?}"
    );
    assert_eq!(p.variable("t"), Some(&Value::Text("Alice".into())));
}

#[test]
fn n33_a_disabled_or_modal_blocked_object_ignores_a_drop() {
    let c = neon_modal_course();
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    // A disabled object ignores the drop silently.
    let effects = neon_drop(&mut p, "off", 300.0);
    assert!(effects.is_empty(), "{effects:?}");
    assert_eq!(p.variable("hit"), Some(&Value::Number(0.0)));

    // A live one under no modal still works.
    let effects = neon_drop(&mut p, "card", 200.0);
    assert!(has(
        &effects,
        &Effect::ObjectDropped {
            object: "card".into(),
            target: "bin".into()
        }
    ));

    // Under a `prevent_base_clicks` layer the base object ignores the drop.
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);
    p.handle(&click("open"));
    let effects = neon_drop(&mut p, "card", 200.0);
    assert!(effects.is_empty(), "{effects:?}");
    assert_eq!(p.variable("hit"), Some(&Value::Number(0.0)));
}

#[test]
fn m47_s1_display_text_and_validator_parser_agree() {
    let json = r##"{
        "schema_version":1,"id":"c","title":"T","slide_size":{"w":1280,"h":720},
        "variables":[{"name":"score","value":42}],
        "scenes":[{"id":"sc0","title":"S","slides":[{"id":"s0","title":"Slide","objects":[]}]}]
    }"##;
    let mut c = Course::from_json(json).expect("valid json");
    c.player.show_seekbar = false;
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);

    assert_eq!(
        p.display_text("Score: %Course.ScorePercent%%%"),
        "Score: 100%"
    );
    assert_eq!(p.display_text("Pass mark 80%"), "Pass mark 80%");
    assert_eq!(p.display_text("100% of %score%"), "100% of 42");
    assert_eq!(p.display_text("%%score%%"), "%score%");
    assert_eq!(p.display_text("%Missing%"), "%Missing%");

    assert_eq!(
        coursecraft_course::variable_reference_spans("Score: %Course.ScorePercent%%%"),
        vec![7..28]
    );
    assert!(coursecraft_course::variable_reference_spans("Pass mark 80%").is_empty());
    assert_eq!(
        coursecraft_course::variable_reference_spans("100% of %score%"),
        vec![8..15]
    );
    assert!(coursecraft_course::variable_reference_spans("%%score%%").is_empty());
    assert_eq!(
        coursecraft_course::variable_reference_spans("%Missing%"),
        vec![0..9]
    );
}

#[test]
fn m49_s2_group_sample_uses_group_timing_and_bounding_box() {
    let json = r##"{
        "schema_version": 1,
        "id": "m49-group-sample",
        "title": "Group Sample",
        "slide_size": { "w": 1280, "h": 720 },
        "scenes": [{
            "id": "sc0",
            "title": "S",
            "slides": [{
                "id": "s0",
                "title": "Slide",
                "duration_ms": 5000,
                "groups": [{
                    "id": "g1",
                    "entrance": { "effect": { "type": "fly_in", "from": "left" }, "duration_ms": 1000, "easing": "linear" },
                    "exit": { "effect": { "type": "fade" }, "duration_ms": 1000, "easing": "linear" },
                    "emphasis": [{ "effect": { "type": "pulse" }, "start_ms": 1500, "duration_ms": 1000, "repeat": 1, "easing": "linear" }]
                }],
                "objects": [
                    { "id": "a", "group": "g1", "kind": { "type": "shape", "shape": "rectangle", "fill": "#FF0000" }, "frame": { "x": 200, "y": 100, "w": 100, "h": 100 }, "timing": { "start_ms": 500, "end_ms": 4000 } },
                    { "id": "b", "group": "g1", "kind": { "type": "shape", "shape": "rectangle", "fill": "#FF0000" }, "frame": { "x": 400, "y": 100, "w": 200, "h": 100 }, "timing": { "start_ms": 1000, "end_ms": 4500 } }
                ]
            }]
        }]
    }"##;
    let mut c = Course::from_json(json).expect("valid json");
    c.player.show_seekbar = false;
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);

    // Before group union start_ms (500): hidden.
    assert!(p.sample_group("g1").is_none());

    // At t = 500 (start of entrance): fly_in from left uses union bounding box (x=200..600, w=400).
    p.handle(&Input::Tick { ms: 500 });
    let s_ent = p.sample_group("g1").expect("entering");
    assert!(
        s_ent.offset.0 < -500.0,
        "group fly_in from left should start off-slide: {:?}",
        s_ent
    );

    // At t = 2000 (midpoint of pulse emphasis): scaled > 1.0.
    p.handle(&Input::Tick { ms: 1500 });
    let s_emph = p.sample_group("g1").expect("emphasis");
    assert!(s_emph.scale.0 > 1.01, "pulse should scale up: {:?}", s_emph);

    // Reduced motion: fly_in becomes fade, pulse (motion) stays at rest.
    p.set_reduced_motion(true);
    let s_reduced = p.sample_group("g1").expect("reduced");
    assert_eq!(s_reduced, crate::ObjectSample::IDENTITY);
    p.set_reduced_motion(false);

    // At t = 4000 (midpoint of exit [3500, 4500)): opacity ~ 0.5.
    p.handle(&Input::Tick { ms: 2000 });
    let s_exit = p.sample_group("g1").expect("exiting");
    assert!(
        (s_exit.opacity - 0.5).abs() < 0.05,
        "exit opacity: {:?}",
        s_exit
    );

    // At t = 4500 (exit finished): None.
    p.handle(&Input::Tick { ms: 500 });
    assert!(p.sample_group("g1").is_none());
}

#[test]
fn m49_s2_group_exit_ends_interactive_group_liveness() {
    let json = r##"{
        "schema_version": 1,
        "id": "m49-group-live",
        "title": "Group Liveness",
        "slide_size": { "w": 1280, "h": 720 },
        "variables": [{ "name": "clicks", "value": 0 }],
        "scenes": [{
            "id": "sc0",
            "title": "S",
            "slides": [{
                "id": "s0",
                "title": "Slide",
                "duration_ms": 5000,
                "groups": [{
                    "id": "g1",
                    "interactive": true,
                    "exit": { "effect": { "type": "fade" }, "duration_ms": 500, "easing": "linear" }
                }],
                "objects": [
                    { "id": "a", "group": "g1", "kind": { "type": "shape", "shape": "rectangle", "fill": "#FF0000" }, "frame": { "x": 100, "y": 100, "w": 100, "h": 100 }, "timing": { "start_ms": 0, "end_ms": 2000 } },
                    { "id": "b", "group": "g1", "kind": { "type": "shape", "shape": "rectangle", "fill": "#FF0000" }, "frame": { "x": 200, "y": 100, "w": 100, "h": 100 }, "timing": { "start_ms": 0, "end_ms": 2000 } }
                ],
                "triggers": [{
                    "when": { "event": "click", "object": "g1" },
                    "action": { "action": "set_variable", "variable": "clicks", "value": 1 }
                }]
            }]
        }]
    }"##;
    let mut c = Course::from_json(json).expect("valid json");
    c.player.show_seekbar = false;
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);

    // Before exit finishes (t = 1750): live and clickable.
    p.handle(&Input::Tick { ms: 1750 });
    p.handle(&Input::Click {
        object: "g1".into(),
    });
    assert_eq!(p.variable("clicks"), Some(&Value::Number(1.0)));

    // After exit finishes (t = 2000): no longer live, click ignored.
    p.handle(&Input::Tick { ms: 250 });
    let out = p.handle(&Input::Click {
        object: "g1".into(),
    });
    assert_eq!(p.variable("clicks"), Some(&Value::Number(1.0)));
    assert!(
        out.iter().any(|e| matches!(e, Effect::Warning { .. })),
        "clicking exited interactive group should warn: {out:?}"
    );
}

#[test]
fn m49_s2_next_wake_ms_wakes_for_group_animation() {
    let json = r##"{
        "schema_version": 1,
        "id": "m49-group-wake",
        "title": "Group Wake",
        "slide_size": { "w": 1280, "h": 720 },
        "scenes": [{
            "id": "sc0",
            "title": "S",
            "slides": [{
                "id": "s0",
                "title": "Slide",
                "duration_ms": 5000,
                "groups": [{
                    "id": "g1",
                    "entrance": { "effect": { "type": "fade" }, "duration_ms": 500, "easing": "linear" },
                    "exit": { "effect": { "type": "fade" }, "duration_ms": 500, "easing": "linear" }
                }],
                "objects": [
                    { "id": "a", "group": "g1", "kind": { "type": "shape", "shape": "rectangle", "fill": "#FF0000" }, "frame": { "x": 100, "y": 100, "w": 100, "h": 100 } },
                    { "id": "b", "group": "g1", "kind": { "type": "shape", "shape": "rectangle", "fill": "#FF0000" }, "frame": { "x": 200, "y": 100, "w": 100, "h": 100 } }
                ]
            }]
        }]
    }"##;
    let mut c = Course::from_json(json).expect("valid json");
    c.player.show_seekbar = false;
    let mut p = Player::new(&c).expect("player");
    p.handle(&Input::Start);

    // At t=0: group entrance is running -> next_wake_ms == Some(0).
    assert_eq!(p.next_wake_ms(), Some(0));

    // At t=1000: group entrance finished, exit starts at 4500 -> wake delta == 3500.
    p.handle(&Input::Tick { ms: 1000 });
    assert_eq!(p.next_wake_ms(), Some(3500));

    // At t=4600: group exit is running -> next_wake_ms == Some(0).
    p.handle(&Input::Tick { ms: 3600 });
    assert_eq!(p.next_wake_ms(), Some(0));
}
