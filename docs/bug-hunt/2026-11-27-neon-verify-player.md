# Neon review 0.6.1: verification of the player input and chrome bugs

Scope: N4, N5, N6, N7, N20, N27, N29 to N34, N40, and the N18 follow-up (crates `coursecraft-render`,
`coursecraft-logic`, apps `coursecraft-player`).
Method: the `#[ignore]` proof tests (N5, N20) were run with `--ignored` on `fixes-0.6.1`. For every other bug a
new test was written first and run against the unfixed code, so each "Confirmed" below failed before its fix.
N40 and the N7 widget layering are checked by reading code only (the player print path is wasm only, and the
widget test needs the fix's helper); both are plain from the code.

| # | Verdict | Evidence | Fix commit |
|---|---|---|---|
| N4 | Confirmed | `Playing::handle_input` called `media.set_rate(player.speed())` before the runtime applied `SetSpeed`. New test `set_speed_applies_the_new_speed_to_media_at_once` fails (`media.rate()` is 1.0 after `SetSpeed 1.5`). | cd3f0c4 |
| N5 | Confirmed | `PanelDrag.on_thumb` was stored and never read; the move used the content-drag sign. Proof test `scroll_panel_thumb_drag_down_scrolls_wrong_way` fails. | 9b7e240 |
| N6 | Confirmed | Test `n6_tab_out_of_a_text_entry_commits_the_typed_text` fails: after typing "Alice" and Tab, only `FocusChanged` inputs come out. Native: `has_focus` is already false when the widget runs, so the buffer resets. Web: `focus_external` changed focus without committing the screen-reader buffer. | 9b7e240 |
| N7 | Confirmed | (a) Live widgets are drawn after all slide painting, so they sit over layers and ignore `prevent_base_clicks`; they ignored the question lock. (b) The widget host clipped to the slide, not to the scroll panel. (c) `set_input_value` had no `prevent_base_clicks` check: logic test `n7_a_base_input_ignores_set_input_value_under_a_modal_layer` fails (`VariableChanged` for "Bob"). | cc37236, 9b7e240 |
| N20 | Confirmed | Proof test `at_slide_end_true_while_seekable_layer_plays` fails. `at_slide_end` tested `base.ended` even when `seek_info` reports a layer clock. | 9b7e240 |
| N27 | Confirmed | Test `n27_slider_values_follow_rotation_and_flip` fails: the left edge of a slider rotated 90 degrees maps to 50, not 0. `input_value_at` never used `item_local_point`. | 9b7e240 |
| N29 | Confirmed | Test `n29_next_stays_enabled_on_the_last_answered_question_in_review` fails: `ArrowRight` sends nothing and no Next button is laid out on the last slide in play order. Spec: `Next` after the last answered question runs `ExitReview`. | 9b7e240 |
| N30 | Confirmed | Test `n30_the_ended_card_takes_no_keys_and_no_slide_input` fails: `M` opens the drawer over the end card. The end card did not cover the slide and the slide's inputs were still forwarded. | 9b7e240 |
| N31 | Confirmed | Test `n31_unused_m_and_c_keys_still_reach_key_triggers` fails: a `key_pressed "m"` trigger on a slide with `show_menu: false` never fires. `M` and `C` were consumed before any check. | 9b7e240 |
| N32 | Confirmed | Test `n32_closing_a_lightbox_runs_no_transition_and_a_plain_slide_ends_one` fails on both parts: closing the lightbox starts the base slide's transition, and a jump to a slide without a transition leaves the old one running. | 9b7e240 |
| N33 | Confirmed | Logic test `n33_a_disabled_or_modal_blocked_object_ignores_a_drop` fails (a disabled object drops and fires `dropped_on`). Render tests fail: `open_draggable_specs` lists a disabled object, `is_object_clickable` is true for a disabled trigger and false for a video. The text link cursor ignoring `prevent_base_clicks` is by reading. | cc37236, 9b7e240 |
| N34 | Confirmed | Test `n34_a_click_on_the_speed_popup_does_not_reach_the_slide` fails: one press gives `SetSpeed` and `Click { big }`. The popup rect was never added to `covered`. | 9b7e240 |
| N40 | Confirmed | `print::show_blocked` set `frame.notice` and left `notice_dismissed`, so `showing_notice` returns `None` after any earlier dismissal. Resources and Glossary rows (`drawer_item`) took clicks while the drawer slid in and reported unclipped rects. Both by reading code. | 9b7e240 |
| N18 follow-up | Already fixed | The frame reads the per-slide `show_bar` through `effective_show_bar`, so the strip, the arrow keys, `M` and the Next control are all refused. New test `n18_slide_show_bar_false_refuses_navigation_keys_and_menu` passes on the unfixed code. It stays as a regression test. The runtime stays ungated on purpose. | test only |
