# Neon verify: accessibility in the published player (N110-N119)

Branch `v4-a11y`, from `fixes-0.6.1` at bd4824f. Source: section 6 of `2026-11-27-neon-review-0.6.1.md`.

Method. Each test below failed on the unfixed code before the fix (the new-API helper tests could not compile on old code, so they are marked "new helper"). `apps/coursecraft-player/src/a11y.rs` is wasm only, so its decision logic moved into pure functions in `crates/render/src/access.rs` (`dom_key_action`, `map_dom_key`, `drag_is_held`, `AccessNode::is_disabled_input`) and those are tested natively. The remaining DOM attribute changes (tabindex, disabled) are verified by reading the code and by `cargo xtask wasm`.

| # | Verdict | Evidence (test name or reading) | Fix commit |
|---|---|---|---|
| N110 | Confirmed | `neon_bughunt_tests::focus_tab_during_keyboard_drag_does_nothing` failed before the fix | 83416fa |
| N111 | Confirmed | `neon_bughunt_tests::focus_space_on_radio_group_does_not_advance` failed before the fix | 83416fa |
| N112 | Confirmed | `access_tree_leaks_hidden_object_links_and_hidden_master_objects`, `access_tree_open_drawer_leaks_slide_inputs`, `access::tests::n112_off_timing_text_link_is_not_offered`, `n112_ended_card_is_read_and_hides_the_slide` failed before the fix; `n112_disabled_text_entry_is_marked_as_a_disabled_input` is a new helper test; the DOM `disabled` attribute is by reading | 83416fa, d0aa436 |
| N113 | Confirmed | `player_frame::tests::n113_pointer_click_lightbox_returns_focus_to_the_clicked_opener` failed before the fix. The focus manager had already marked the lightbox open when the frame checked, so the opener was never remembered | 83416fa |
| N114 | Confirmed (7 of 7 items; item 2 only partly fixed) | By reading `a11y.rs`; decision logic tested by `access::tests::n114_dom_keys_reach_links_lists_scroll_panels_and_key_triggers` and `n114_drag_held_ignores_toggle_buttons_that_use_pressed` (new helpers). Item 2: the fieldset is now focusable but still has no child radios | 83416fa, d0aa436 |
| N115 | Partly a bug | `player_frame::tests::n115_restart_keeps_the_dom_focus_owner_flag` (new helper). The flag is real, but it only gates the text-entry and fill-in `request_focus` fight, not Tab wrapping | 83416fa |
| N116 | Confirmed | `player_frame::tests::n116_leaving_keyboard_mode_clears_the_runtime_focus` failed before the fix (pointer press case). The slide-change and restart cases are by reading: the runtime resets its focus but the frame kept a stale copy | 83416fa |
| N117 | Partly a bug | (a) `focus::tests::n117_focus_ring_wraps_the_parent_not_its_state_badge`, (b, index) `n117_second_link_ring_skips_the_lines_of_a_wrapped_first_link`, (c) `access::tests::n117_drag_item_label_includes_state_badge_text` failed before the fix. (d) by reading (review mark replaced the value). (b, rotation and flips) and (e, link order) are not fixed | 83416fa |
| N118 | Confirmed | (a) `focus::tests::n118_space_reaches_key_triggers_when_nothing_is_focusable` and (b) `n118_slide_key_triggers_do_not_fire_behind_the_open_drawer` failed before the fix. (c, review guards) by reading. (c, `prevent_base_clicks` on question controls) is not fixed: the spec does not say what a modal layer does to question controls | 83416fa |
| N119 | Confirmed | `player_frame::tests::n119_exit_full_screen_tooltip_comes_from_the_chrome_strings` failed before the fix. New chrome string key `exitFullScreen` | 83416fa |

Fix commits: 83416fa is the render, course, spec and xliff-test commit (the xliff truncation test cut at half the export and broke when the chrome string table grew); d0aa436 is the web DOM layer commit.
