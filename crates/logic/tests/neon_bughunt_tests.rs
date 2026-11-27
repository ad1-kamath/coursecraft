//! Bug-hunt proof tests for `coursecraft-logic` (0.6.1 Neon review).
//!
//! Each test failed before its fix and now guards against a regression.

use coursecraft_course::{Course, Value};
use coursecraft_logic::{Effect, Input, MediaOp, Player};

/// Spec (`docs/spec/triggers.md`): `exit_course` "stops everything else in the
/// queue and ends the run". Because `ExitCourse` does not bump `self.st.epoch`,
/// subsequent triggers on the same event still execute and mutate state after
/// `Effect::Exit`.
#[test]
fn exit_course_does_not_halt_subsequent_triggers_on_same_event() {
    let course = Course::from_json(
        r#"{
          "schema_version": 1,
          "id": "exit-order",
          "title": "Exit Order",
          "variables": [
            { "name": "after_exit", "value": false }
          ],
          "scenes": [{
            "id": "sc1",
            "title": "Scene 1",
            "slides": [{
              "id": "s1",
              "title": "Slide 1",
              "duration_ms": 5000,
              "objects": [{
                "id": "btn",
                "kind": { "type": "button", "label": "Quit" },
                "frame": { "x": 10, "y": 10, "w": 120, "h": 40 }
              }],
              "triggers": [
                {
                  "when": { "event": "click", "object": "btn" },
                  "action": { "action": "exit_course" }
                },
                {
                  "when": { "event": "click", "object": "btn" },
                  "action": {
                    "action": "set_variable",
                    "variable": "after_exit",
                    "op": "assign",
                    "value": true
                  }
                }
              ]
            }]
          }]
        }"#,
    )
    .expect("valid course");

    let mut player = Player::new(&course).expect("player");
    let _ = player.handle(&Input::Start);
    let fx = player.handle(&Input::Click {
        object: "btn".to_owned(),
    });

    assert!(fx.contains(&Effect::Exit), "Exit effect should be emitted");
    assert_eq!(
        player.variable("after_exit"),
        Some(&Value::Bool(false)),
        "trigger after exit_course must not run; emitted effects: {fx:?}"
    );
}

/// `Player::finish_resume` populates `timeline_completed` for every visited
/// non-current slide whenever `timeline_completed` is empty, without checking
/// whether this is an old suspend restore vs an active session. Hosts such as
/// the editor preview (`crates/ui-egui/src/preview.rs`) reconstruct `Player`
/// every frame via `Player::resume_trusted`. Opening a lightbox (or jumping
/// away early) while `timeline_completed` is empty falsely unlocks `next_requires:
/// timeline_ends` and `seekbar: until_timeline_ends` on the uncompleted slide.
#[test]
fn resume_trusted_falsely_unlocks_timeline_ends_after_opening_lightbox() {
    let course = Course::from_json(
        r#"{
          "schema_version": 1,
          "id": "resume-unlock",
          "title": "Resume Unlock",
          "player": {
            "next_requires": { "kind": "timeline_ends" },
            "seekbar": "until_timeline_ends"
          },
          "scenes": [{
            "id": "sc1",
            "title": "Scene 1",
            "slides": [
              {
                "id": "s1",
                "title": "Slide 1",
                "duration_ms": 10000,
                "objects": [{
                  "id": "open_lb",
                  "kind": { "type": "button", "label": "Help" },
                  "frame": { "x": 10, "y": 10, "w": 120, "h": 40 }
                }],
                "triggers": [{
                  "when": { "event": "click", "object": "open_lb" },
                  "action": { "action": "lightbox_slide", "slide": "s2" }
                }]
              },
              {
                "id": "s2",
                "title": "Help Slide",
                "duration_ms": 5000
              }
            ]
          }]
        }"#,
    )
    .expect("valid course");

    let mut player = Player::new(&course).expect("player");
    let _ = player.handle(&Input::Start);
    assert!(!player.can_next(), "s1 timeline has not finished yet");
    assert!(player.seekbar_locked(), "s1 seekbar should be locked");

    // Open lightbox s2 at t = 0 ms on s1.
    let _ = player.handle(&Input::Click {
        object: "open_lb".to_owned(),
    });
    assert!(player.lightbox_open());

    // Simulate the next editor preview frame reconstructing Player via resume_trusted.
    let plan = player.play_plan();
    let saved = player.state().clone();
    let mut resumed = Player::resume_trusted(&course, saved, &plan).expect("resume_trusted");

    // Close the lightbox and return to s1 at t = 0 ms.
    let _ = resumed.handle(&Input::CloseLightbox);
    assert_eq!(resumed.current_slide().map(|s| s.id.as_str()), Some("s1"));
    assert_eq!(resumed.slide_elapsed_ms(), 0);

    assert!(
        !resumed.can_next(),
        "s1 must still lock Next because its 10s timeline never finished"
    );
    assert!(
        resumed.seekbar_locked(),
        "s1 seekbar must remain locked until timeline ends"
    );
}

/// `enter_lightbox_slide` clears `self.st.sequence` without calling
/// `seed_sequence`, making Sequence questions inside a lightbox empty and
/// unanswerable.
#[test]
fn sequence_question_in_lightbox_never_seeds_sequence() {
    let course = Course::from_json(
        r#"{
          "schema_version": 1,
          "id": "lb-seq",
          "title": "Lightbox Sequence",
          "scenes": [{
            "id": "sc1",
            "title": "Scene 1",
            "slides": [
              {
                "id": "s1",
                "title": "Base",
                "duration_ms": 5000,
                "objects": [{
                  "id": "open_q",
                  "kind": { "type": "button", "label": "Quiz" },
                  "frame": { "x": 10, "y": 10, "w": 120, "h": 40 }
                }],
                "triggers": [{
                  "when": { "event": "click", "object": "open_q" },
                  "action": { "action": "lightbox_slide", "slide": "q1" }
                }]
              },
              {
                "id": "q1",
                "title": "Order Steps",
                "duration_ms": 5000,
                "question": {
                  "kind": "sequence",
                  "prompt": "Order these",
                  "choices": [
                    { "id": "a", "text": "First" },
                    { "id": "b", "text": "Second" },
                    { "id": "c", "text": "Third" }
                  ]
                },
                "objects": [{
                  "id": "sub",
                  "kind": { "type": "button", "label": "Submit" },
                  "frame": { "x": 10, "y": 300, "w": 120, "h": 40 }
                }],
                "triggers": [{
                  "when": { "event": "click", "object": "sub" },
                  "action": { "action": "submit_question" }
                }]
              }
            ]
          }]
        }"#,
    )
    .expect("valid course");

    let mut player = Player::new(&course).expect("player");
    let _ = player.handle(&Input::Start);
    let _ = player.handle(&Input::Click {
        object: "open_q".to_owned(),
    });
    assert_eq!(player.lightbox_slide_id(), Some("q1"));
    assert_eq!(
        player.sequence_order().len(),
        3,
        "sequence question inside lightbox should seed all 3 choices, got {:?}",
        player.sequence_order()
    );
}

/// Replacing an open lightbox skips the `idx == self.st.slide` / base-slide
/// check, allowing a lightbox slide to open the underlying base slide as a
/// lightbox over itself (`lightbox_slide_id == lightbox_base_slide`).
#[test]
fn replacing_lightbox_allows_opening_base_slide_over_itself() {
    let course = Course::from_json(
        r#"{
          "schema_version": 1,
          "id": "lb-self-base",
          "title": "Lightbox Base",
          "scenes": [{
            "id": "sc1",
            "title": "Scene 1",
            "slides": [
              {
                "id": "s1",
                "title": "Base",
                "duration_ms": 5000,
                "objects": [{
                  "id": "b1",
                  "kind": { "type": "button", "label": "Open s2" },
                  "frame": { "x": 10, "y": 10, "w": 120, "h": 40 }
                }],
                "triggers": [{
                  "when": { "event": "click", "object": "b1" },
                  "action": { "action": "lightbox_slide", "slide": "s2" }
                }]
              },
              {
                "id": "s2",
                "title": "Overlay",
                "duration_ms": 5000,
                "objects": [{
                  "id": "b2",
                  "kind": { "type": "button", "label": "Open s1" },
                  "frame": { "x": 10, "y": 10, "w": 120, "h": 40 }
                }],
                "triggers": [{
                  "when": { "event": "click", "object": "b2" },
                  "action": { "action": "lightbox_slide", "slide": "s1" }
                }]
              }
            ]
          }]
        }"#,
    )
    .expect("valid course");

    assert!(course.is_playable(), "course passes validation");
    let mut player = Player::new(&course).expect("player");
    let _ = player.handle(&Input::Start);
    let _ = player.handle(&Input::Click {
        object: "b1".to_owned(),
    });
    assert_eq!(player.lightbox_slide_id(), Some("s2"));

    // Clicking b2 inside lightbox s2 attempts to lightbox base slide s1 over s1.
    let _ = player.handle(&Input::Click {
        object: "b2".to_owned(),
    });
    assert_ne!(
        player.lightbox_slide_id(),
        player.lightbox_base_slide().map(|s| s.id.as_str()),
        "a slide must not be opened as a lightbox over itself"
    );
}

/// `clock_next_wake` ignores `obj.emphasis`, so `next_wake_ms()` skips over
/// emphasis start times and fails to request animation frames (`Some(0)`) while
/// an emphasis animation is playing.
#[test]
fn next_wake_ms_ignores_emphasis_animations() {
    let course = Course::from_json(
        r##"{
          "schema_version": 1,
          "id": "emphasis-wake",
          "title": "Emphasis Wake",
          "player": {
            "show_seekbar": false
          },
          "scenes": [{
            "id": "sc1",
            "title": "Scene 1",
            "slides": [{
              "id": "s1",
              "title": "Slide 1",
              "duration_ms": 5000,
              "objects": [{
                "id": "box1",
                "kind": { "type": "shape", "shape": "rectangle", "fill": "#2266CC" },
                "frame": { "x": 40, "y": 40, "w": 120, "h": 80 },
                "emphasis": [{
                  "effect": { "type": "pulse" },
                  "start_ms": 1000,
                  "duration_ms": 1000,
                  "repeat": 1
                }]
              }]
            }]
          }]
        }"##,
    )
    .expect("valid course");

    let mut player = Player::new(&course).expect("player");
    let _ = player.handle(&Input::Start);

    // At t = 0, should wake at 1000 ms (emphasis start), not 5000 ms (slide end).
    assert_eq!(
        player.next_wake_ms(),
        Some(1000),
        "should schedule wakeup at emphasis start_ms (1000 ms)"
    );

    // Mid-emphasis (t = 1200 ms), should request immediate animation frames Some(0).
    let _ = player.handle(&Input::Tick { ms: 1200 });
    assert_eq!(
        player.next_wake_ms(),
        Some(0),
        "must return Some(0) while emphasis animation is actively playing"
    );
}

/// Spec (`docs/spec/triggers.md`, `docs/spec/course-model.md`): a trimmed audio
/// or video clip starts at `trim.start_ms` and emits `Effect::MediaSeek` after
/// `Play`. `play_media` (used by `play_media` triggers and video clicks) omits
/// `MediaSeek` and plays from file time 0 instead.
#[test]
fn play_media_ignores_trim_start_ms() {
    let course = Course::from_json(
        r#"{
          "schema_version": 1,
          "id": "trim-play",
          "title": "Trim Play",
          "scenes": [{
            "id": "sc1",
            "title": "Scene 1",
            "slides": [{
              "id": "s1",
              "title": "Slide 1",
              "duration_ms": 10000,
              "objects": [
                {
                  "id": "vid",
                  "kind": {
                    "type": "video",
                    "src": "clip.mp4",
                    "autoplay": false,
                    "trim": { "start_ms": 3000, "end_ms": 8000 }
                  },
                  "frame": { "x": 40, "y": 40, "w": 320, "h": 180 }
                },
                {
                  "id": "play_btn",
                  "kind": { "type": "button", "label": "Play" },
                  "frame": { "x": 40, "y": 240, "w": 100, "h": 40 }
                }
              ],
              "triggers": [{
                "when": { "event": "click", "object": "play_btn" },
                "action": { "action": "play_media", "object": "vid" }
              }]
            }]
          }]
        }"#,
    )
    .expect("valid course");

    let mut player = Player::new(&course).expect("player");
    let _ = player.handle(&Input::Start);
    let fx = player.handle(&Input::Click {
        object: "play_btn".to_owned(),
    });

    assert!(
        fx.contains(&Effect::Media {
            object: "vid".to_owned(),
            op: MediaOp::Play,
        }),
        "Play should be emitted"
    );
    assert!(
        fx.contains(&Effect::MediaSeek {
            object: "vid".to_owned(),
            ms: 3000,
        }),
        "play_media on trimmed video must seek to trim.start_ms (3000), got: {fx:?}"
    );
}

/// Spec (`docs/spec/triggers.md`): `objects_intersect` / `objects_stop_intersecting`
/// frames are computed after every clock tick, motion-path move, state change,
/// and drag drop. `set_state` and `drag_dropped` never call `check_intersections`.
#[test]
fn objects_intersect_not_recomputed_on_change_state() {
    let course = Course::from_json(
        r##"{
          "schema_version": 1,
          "id": "intersect-state",
          "title": "Intersect State",
          "variables": [{ "name": "hit", "value": false }],
          "scenes": [{
            "id": "sc1",
            "title": "Scene 1",
            "slides": [{
              "id": "s1",
              "title": "Slide 1",
              "duration_ms": 5000,
              "objects": [
                {
                  "id": "a",
                  "initial_state": "hidden",
                  "kind": { "type": "shape", "shape": "rectangle", "fill": "#FF0000" },
                  "frame": { "x": 100, "y": 100, "w": 100, "h": 100 }
                },
                {
                  "id": "b",
                  "kind": { "type": "shape", "shape": "rectangle", "fill": "#00FF00" },
                  "frame": { "x": 120, "y": 120, "w": 100, "h": 100 }
                },
                {
                  "id": "reveal",
                  "kind": { "type": "button", "label": "Show A" },
                  "frame": { "x": 10, "y": 10, "w": 80, "h": 30 }
                }
              ],
              "triggers": [
                {
                  "when": { "event": "key_pressed", "key": "a" },
                  "action": { "action": "change_state", "object": "a", "state": "normal" }
                },
                {
                  "when": { "event": "objects_intersect", "a": "a", "b": "b" },
                  "action": {
                    "action": "set_variable",
                    "variable": "hit",
                    "op": "assign",
                    "value": true
                  }
                }
              ]
            }]
          }]
        }"##,
    )
    .expect("valid course");

    let mut player = Player::new(&course).expect("player");
    let _ = player.handle(&Input::Start);
    assert_eq!(player.variable("hit"), Some(&Value::Bool(false)));

    let _ = player.handle(&Input::KeyPressed {
        key: "a".to_owned(),
    });
    assert_eq!(
        player.object_state("a"),
        Some("normal"),
        "a should be unhidden by key_pressed trigger"
    );
    assert_eq!(
        player.variable("hit"),
        Some(&Value::Bool(true)),
        "unhiding overlapping object `a` via change_state must fire objects_intersect immediately"
    );
}

/// `sample_object` and `object_current_bounds` ignore `base_objects_hidden()`,
/// so hotspot clicks still land on a base-layer hotspot image covered by a
/// visible layer with `hide_base_objects: true`.
#[test]
fn hotspot_click_hits_base_image_when_hide_base_objects_layer_is_open() {
    let course = Course::from_json(
        r#"{
          "schema_version": 1,
          "id": "hotspot-hidden-base",
          "title": "Hotspot Hidden Base",
          "scenes": [{
            "id": "sc1",
            "title": "Scene 1",
            "slides": [{
              "id": "s1",
              "title": "Slide 1",
              "duration_ms": 5000,
              "objects": [{
                "id": "img",
                "kind": { "type": "image", "src": "map.png" },
                "frame": { "x": 0, "y": 0, "w": 400, "h": 300 }
              }],
              "layers": [{
                "id": "cover",
                "name": "Cover",
                "hide_base_objects": true,
                "objects": []
              }],
              "triggers": [{
                "when": { "event": "timeline_starts" },
                "action": { "action": "show_layer", "layer": "cover" }
              }],
              "question": {
                "kind": "hotspot",
                "prompt": "Click the area",
                "choices": [],
                "hotspot_image": "img",
                "regions": [{
                  "id": "r1",
                  "shape": "rect",
                  "x": 50,
                  "y": 50,
                  "w": 100,
                  "h": 100,
                  "correct": true
                }]
              }
            }]
          }]
        }"#,
    )
    .expect("valid course");

    let mut player = Player::new(&course).expect("player");
    let _ = player.handle(&Input::Start);
    assert!(player.base_objects_hidden());

    let fx = player.handle(&Input::HotspotClicked { x: 100.0, y: 100.0 });
    assert!(
        !fx.iter().any(|e| matches!(e, Effect::HotspotMarked { .. })),
        "hotspot click must be ignored when base objects are hidden by a layer, got: {fx:?}"
    );
}

/// `seek_base` calls `seek_media` before `process()` runs queued timeline
/// events. Seeking past a `timeline_reaches` trigger that jumps to another
/// slide emits spurious `Media { Play }` / `MediaSeek` for media on the old
/// slide before leaving it.
#[test]
fn seek_base_plays_old_slide_media_before_processing_jump_trigger() {
    let course = Course::from_json(
        r#"{
          "schema_version": 1,
          "id": "seek-jump-media",
          "title": "Seek Jump Media",
          "scenes": [{
            "id": "sc1",
            "title": "Scene 1",
            "slides": [
              {
                "id": "s1",
                "title": "Slide 1",
                "duration_ms": 10000,
                "objects": [{
                  "id": "aud",
                  "kind": { "type": "audio", "src": "clip.mp3", "autoplay": true },
                  "timing": { "start_ms": 3000 },
                  "frame": { "x": 0, "y": 0, "w": 40, "h": 40 }
                }],
                "triggers": [{
                  "when": { "event": "timeline_reaches", "ms": 2000 },
                  "action": { "action": "jump_to_slide", "target": { "id": "s2" } }
                }]
              },
              {
                "id": "s2",
                "title": "Slide 2",
                "duration_ms": 5000
              }
            ]
          }]
        }"#,
    )
    .expect("valid course");

    let mut player = Player::new(&course).expect("player");
    let _ = player.handle(&Input::Start);

    // Seeking to 5000 ms crosses the jump at 2000 ms; `aud` (starting at 3000 ms)
    // should never be started/seeked after the jump.
    let fx = player.handle(&Input::PlayerSeek { ms: 5000 });
    assert_eq!(player.current_slide().map(|s| s.id.as_str()), Some("s2"));
    assert!(
        !fx.iter().any(|e| matches!(
            e,
            Effect::MediaSeek { object, .. } if object == "aud"
        )),
        "old slide's audio starting past the jump point should not be seeked during seek: {fx:?}"
    );
}

/// `media_time` checks `trim.end_ms` and returns via `finish_media` before
/// scanning cues, silently dropping any cue between the previous position and
/// `trim.end_ms`.
#[test]
fn media_time_skips_cue_when_update_reaches_trim_end() {
    let course = Course::from_json(
        r#"{
          "schema_version": 1,
          "id": "trim-cue",
          "title": "Trim Cue",
          "variables": [{ "name": "cue_hit", "value": false }],
          "scenes": [{
            "id": "sc1",
            "title": "Scene 1",
            "slides": [{
              "id": "s1",
              "title": "Slide 1",
              "duration_ms": 10000,
              "objects": [{
                "id": "vid",
                "kind": {
                  "type": "video",
                  "src": "clip.mp4",
                  "autoplay": true,
                  "trim": { "start_ms": 0, "end_ms": 5000 },
                  "cues": [{ "id": "c1", "at_ms": 4900, "name": "Near End" }]
                },
                "frame": { "x": 40, "y": 40, "w": 320, "h": 180 }
              }],
              "triggers": [{
                "when": { "event": "cue_reached", "object": "vid", "cue": "c1" },
                "action": {
                  "action": "set_variable",
                  "variable": "cue_hit",
                  "op": "assign",
                  "value": true
                }
              }]
            }]
          }]
        }"#,
    )
    .expect("valid course");

    let mut player = Player::new(&course).expect("player");
    let _ = player.handle(&Input::Start);
    let _ = player.handle(&Input::MediaTime {
        object: "vid".to_owned(),
        at_ms: 4800.0,
    });
    let _ = player.handle(&Input::MediaTime {
        object: "vid".to_owned(),
        at_ms: 5050.0,
    });

    assert_eq!(
        player.variable("cue_hit"),
        Some(&Value::Bool(true)),
        "cue at 4900 ms inside trim [0, 5000] must fire even when MediaTime steps from 4800 to 5050"
    );
}

/// Answer review only restores DragDrop `object_offsets` when `snap_to_center`
/// is `true`. When `snap_to_center` is `false`, reviewed drag items jump back
/// to `(0, 0)` instead of showing where the learner dropped them.
#[test]
fn drag_drop_review_loses_positions_when_snap_to_center_false() {
    let course = Course::from_json(
        r##"{
          "schema_version": 1,
          "id": "dd-review",
          "title": "DD Review",
          "quiz": { "passing_percent": 80 },
          "scenes": [{
            "id": "sc1",
            "title": "Scene 1",
            "slides": [
              {
                "id": "q1",
                "title": "Drag Question",
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
                    "frame": { "x": 300, "y": 100, "w": 160, "h": 120 }
                  },
                  {
                    "id": "sub",
                    "kind": { "type": "button", "label": "Submit" },
                    "frame": { "x": 20, "y": 260, "w": 100, "h": 40 }
                  }
                ],
                "question": {
                  "kind": "drag_drop",
                  "prompt": "Drag d1 to t1",
                  "choices": [],
                  "snap_to_center": false,
                  "drag_items": [{ "item": "d1", "target": "t1" }],
                  "drop_targets": ["t1"]
                },
                "triggers": [{
                  "when": { "event": "click", "object": "sub" },
                  "action": { "action": "submit_question" }
                }]
              },
              {
                "id": "res",
                "title": "Results",
                "duration_ms": 5000,
                "objects": [{
                  "id": "rev",
                  "kind": { "type": "button", "label": "Review" },
                  "frame": { "x": 20, "y": 20, "w": 100, "h": 40 }
                }],
                "triggers": [{
                  "when": { "event": "click", "object": "rev" },
                  "action": { "action": "review_quiz" }
                }]
              }
            ]
          }]
        }"##,
    )
    .expect("valid course");

    let mut player = Player::new(&course).expect("player");
    let _ = player.handle(&Input::Start);
    let _ = player.handle(&Input::DragDropped {
        item: "d1".to_owned(),
        target: Some("t1".to_owned()),
        x: 320.0,
        y: 120.0,
    });
    let dropped_offset = player.sample_object("d1").expect("d1 visible").offset;
    assert_eq!(dropped_offset, (300.0, 100.0));

    let _ = player.handle(&Input::Click {
        object: "sub".to_owned(),
    });
    let _ = player.handle(&Input::Next);
    let _ = player.handle(&Input::Click {
        object: "rev".to_owned(),
    });
    assert!(player.is_reviewing());
    assert_eq!(player.current_slide().map(|s| s.id.as_str()), Some("q1"));

    let review_offset = player
        .sample_object("d1")
        .expect("d1 visible in review")
        .offset;
    assert_eq!(
        review_offset, dropped_offset,
        "reviewed drag item should remain where the learner dropped it when snap_to_center is false"
    );
}

/// `stop_layer_media` emits `Media { Stop }` when hiding a layer, but does not
/// clear `media_clocks` (unlike `stop_media`). A Lottie object on that layer
/// retains its old clock position instead of resetting to 0 (`None`).
#[test]
fn hide_layer_does_not_clear_lottie_media_clock() {
    let course = Course::from_json(
        r#"{
          "schema_version": 1,
          "id": "layer-lottie-clock",
          "title": "Layer Lottie Clock",
          "scenes": [{
            "id": "sc1",
            "title": "Scene 1",
            "slides": [{
              "id": "s1",
              "title": "Slide 1",
              "duration_ms": 5000,
              "objects": [
                {
                  "id": "show_btn",
                  "kind": { "type": "button", "label": "Show" },
                  "frame": { "x": 10, "y": 10, "w": 80, "h": 30 }
                },
                {
                  "id": "hide_btn",
                  "kind": { "type": "button", "label": "Hide" },
                  "frame": { "x": 100, "y": 10, "w": 80, "h": 30 }
                }
              ],
              "layers": [{
                "id": "l1",
                "name": "Layer 1",
                "duration_ms": 5000,
                "objects": [{
                  "id": "anim",
                  "kind": { "type": "lottie", "src": "spin.json", "autoplay": true },
                  "frame": { "x": 50, "y": 60, "w": 100, "h": 100 }
                }]
              }],
              "triggers": [
                {
                  "when": { "event": "click", "object": "show_btn" },
                  "action": { "action": "show_layer", "layer": "l1" }
                },
                {
                  "when": { "event": "click", "object": "hide_btn" },
                  "action": { "action": "hide_layer", "layer": "l1" }
                }
              ]
            }]
          }]
        }"#,
    )
    .expect("valid course");

    let mut player = Player::new(&course).expect("player");
    let _ = player.handle(&Input::Start);
    let _ = player.handle(&Input::Click {
        object: "show_btn".to_owned(),
    });
    let _ = player.handle(&Input::Tick { ms: 1500 });
    assert_eq!(player.media_clock_ms("anim"), Some(1500));

    let fx = player.handle(&Input::Click {
        object: "hide_btn".to_owned(),
    });
    assert!(fx.contains(&Effect::Media {
        object: "anim".to_owned(),
        op: MediaOp::Stop,
    }));
    assert_eq!(
        player.media_clock_ms("anim"),
        None,
        "stopping layer media must reset media_clock_ms to None, matching stop_media"
    );
}

/// `effective_player()` does not apply `Slide.player.show_bar`, and runtime
/// `Input::Next`/`Previous`/`GoTo` only check `show_nav`/`show_menu`, ignoring
/// `show_bar: false`.
#[test]
fn effective_player_applies_slide_show_bar_false() {
    let course = Course::from_json(
        r#"{
          "schema_version": 1,
          "id": "show-bar-off",
          "title": "Show Bar Off",
          "player": {
            "show_bar": true,
            "show_nav": true,
            "show_menu": true
          },
          "scenes": [{
            "id": "sc1",
            "title": "Scene 1",
            "slides": [
              {
                "id": "s1",
                "title": "Slide 1",
                "duration_ms": 5000,
                "player": { "show_bar": false },
                "triggers": [{
                  "when": { "event": "key_pressed", "key": "j" },
                  "action": { "action": "jump_to_slide", "target": "next" }
                }]
              },
              {
                "id": "s2",
                "title": "Slide 2",
                "duration_ms": 5000
              }
            ]
          }]
        }"#,
    )
    .expect("valid course");

    let mut player = Player::new(&course).expect("player");
    let _ = player.handle(&Input::Start);
    assert!(
        !player.effective_player().show_bar,
        "effective_player() must reflect per-slide player.show_bar = false"
    );

    // With the bar hidden the runtime itself refuses the learner's `Next`.
    let _ = player.handle(&Input::Next);
    assert_eq!(player.current_slide().map(|s| s.id.as_str()), Some("s1"));

    // An author jump still works, and the override ends with the slide.
    let _ = player.handle(&Input::KeyPressed {
        key: "j".to_owned(),
    });
    assert_eq!(player.current_slide().map(|s| s.id.as_str()), Some("s2"));
    assert!(player.effective_player().show_bar);
}
