# Neon review 0.6.1: verification of the logic and lms bugs

Scope: N1, N2, N3, N9 to N19, N38 (crates `coursecraft-logic` and `coursecraft-lms`).
All 15 `#[ignore]` proof tests were run with `--ignored` on branch `fixes-0.6.1` and all 15 failed
before any fix. A failing proof test was then checked against the spec, not taken as proof.

| # | Verdict | Evidence | Fix commit |
|---|---|---|---|
| N1 | Confirmed | `Action::ExitCourse` sets `exited` and clears the queue but `fire()` only stops on an epoch change, so a later trigger on the same event still runs (proof test: effects `[Exit, VariableChanged]`). Spec: nothing runs after `exit`. | 2c752f0 |
| N2 | Partly | Real bug, but the cause differs from the report. With a lightbox open, `st.slide` is the box slide, so `finish_resume` treats the base slide as "already left" and marks it completed (proof test fails at `can_next`). Checking `!state.started` is not the fix: `triggers.md` says a slide the learner leaves counts as completed. The fix excludes the lightbox base slide. No change to `preview.rs` is needed. | ac83783 |
| N3 | Confirmed | `enter_lightbox_slide` clears `sequence` and never calls `seed_sequence` (proof test: order is `[]`). The Safe Resume outcome-layer part is not fixed: the spec says nothing about it for lightbox slides. | f2c2b72 |
| N9 | Confirmed | `pending` holds at most one report, so a non-empty `pending` during `mark_failed` is always newer. `push_front` puts the stale report back in front, and a second failure clears the newer `Passed`. Spec (`triggers.md` LTI section): retry once, then lose reporting. | e2802e1 |
| N10 | Confirmed | `clock_next_wake` never reads `obj.emphasis` (proof test: `Some(5000)` instead of `Some(1000)`). Spec: emphasis plays over `[start_ms, end_ms())` on the object timeline. | d63ade7 |
| N11 | Confirmed | `play_media` emits `Media { Play }` only. Spec ("Trim"): a trimmed object starts at `trim.start_ms`, with `MediaSeek` after `Play`, as `start_autoplay` does. | 6f27a1e |
| N12 | Confirmed | The replace branch of `open_lightbox` skips the current-slide guard. Proof test: `s1` opens as a lightbox over itself. | 781d81f |
| N13 | Confirmed | Spec: intersections are recomputed after every state change. `set_state` called from a trigger action never recomputes (proof test). Layer show/hide from actions has the same gap. | 9cdbe04 |
| N14 | Confirmed | `sample_object` ignores `base_objects_hidden()`; spec: hidden objects never intersect and a layer with `hide_base_objects` hides the base. Proof test: `HotspotMarked` emitted. | 53eb4c4 |
| N15 | Partly | Confirmed: `seek_media` runs before the queue is processed, so it emits `MediaSeek` for the old slide before the jump (proof test). Not a bug: stopping untrimmed media with no end at `position == duration`, because the window `[start, end)` ends with the slide. | 4a1ca02 |
| N16 | Confirmed | `media_time` returns through `finish_media` at the trim end before the cue scan (proof test: `cue_hit` false). Cues in `(old, trim_end]` must fire first. | 29e22f3 |
| N17 | Confirmed | `AnswerSnapshot` has no drag positions, and review only rebuilds offsets when `snap_to_center` is true (proof test: `(0,0)`). | 7b85068 |
| N18 | Partly | Confirmed: `effective_player` had no `show_bar` override. Not fixed in the runtime: gating `Input::Next`/`GoTo` on `show_bar`. The spec makes the renderer disable the keys and strip, and the existing render test `bar_off_uses_full_frame_without_strip_and_ignores_arrow_keys_and_menu` sends `Input::Next` with the bar off and expects it to advance. Gating it is a render-wave decision (that test would change). | 0fa4424, 55583ea |
| N19 | Confirmed | `handle_progress_put` ignores the status class. Spec: 401/403 mid-session turns reporting off and sets `reporting_lost()`. | aa17c1b |
| N38 | Confirmed | `stop_layer_media` does not remove `media_clocks`. Spec: "stop forgets it". Proof test: `Some(1500)`. | 01bf0cf |
