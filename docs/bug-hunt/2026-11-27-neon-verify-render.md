# Neon 0.6.1 review: verification of the render, playback and web-object bugs

Scope: N8, N21, N22, N23, N24, N25, N26, N28, N35, N36, N37, N39 (crates `render` and `playback`, and the web player's `web_object.rs`).
Method: each bug was checked by running the Neon proof test with `--ignored` where one exists, or by reading the code against the current tree (compose.rs now fills in variables before sequencing, and has shaped picture crop, cover fill and question frames). Where a fix was made, a test was added or un-ignored. Items marked "reading" could not be proven by a test in this environment (see the notes).

| # | Verdict | Evidence | Fix commit |
|---|---|---|---|
| N8 | Confirmed | `PlayerFrame::show_captions` skipped every non-audio caption, and the desktop backend's `show_caption` is a no-op, so desktop video captions never showed. Reading plus the new test `desktop_video_caption_is_left_to_the_player_frame`. The part about suppressed audio captions is real only when a video and an audio object both have cue text at the same moment: the one-line caption box shows the video's text (video still wins). That is kept, and the spec says so. | daa73b2 |
| N21 | Confirmed | Neon test failed: area 10000 (solid) instead of 8400. The single pass looked for an outer before later outers were known. | 9d1685a |
| N22 | Confirmed | Neon test failed: path text stayed `%Course.Title%`. `fill_item_variables` had no `Visual::Path` arm. | d8e42b5 |
| N23 | Confirmed | Neon test failed: master progress meter fraction 0 instead of 1. `push_master_items` always read the authored default. | d8e42b5 |
| N24 | Confirmed | Neon test failed: mirrored parent got +90 while its extra orbited clockwise. | d8e42b5 |
| N25 | Confirmed | Reading, and new test `lottie_ends_when_the_sped_animation_ends_not_the_timeline_clock`: `lottie_ended` compared the timeline clock to the raw duration and ignored `speed`. | ae822c9 |
| N26 | Confirmed | Reading: `last_ptr` was a raw `*const VideoFrame` with no hold on the allocation, so an address reused after the old frame was freed looks like the same frame. A `Weak` keeps the address reserved. New test `every_new_video_frame_is_uploaded_even_when_addresses_repeat` passes; it cannot force an address reuse, so it guards the behaviour, it does not prove the old failure. | ae822c9 |
| N28 | Confirmed | New tests failed before the fix. Text was turned the opposite way to its box (`rotated_flipped_text_stays_with_its_box`: text at the top instead of the bottom for 90 degrees), the text shadow was mirrored (`flipped_text_shadow_follows_the_unmirrored_text`: 274 px off), and link hit tests used the mirrored transform. The spec line "Text mirrors with the object" was wrong against the code and PowerPoint; the code (glyphs are never mirrored by `flip_h`) is kept and the spec corrected. Not changed: `flip_v` still mirrors glyphs, and a mirrored object turns counter-clockwise for a positive `rotation` (the flip is applied after the rotation); both are recorded in the spec. | 297738e, af9e5b7 |
| N35 (a) | Confirmed | Reading and new tests `scrollbar_copy_sits_below_a_layer_and_draws_once`. The copy was appended after all items (above layers), and the panel item painted the thumb too (double alpha). | baad394 |
| N35 (b) | Confirmed | Reading: `paint_items` translated the item, mask and effects by the transition `offset` but not the panel `clip_rect` or the thumb. No test (needs a transition frame). | baad394 |
| N35 (c) | Confirmed | Reading: `PlacedMask.frame` was not moved by `apply_panels` (scroll) or `lifted_items` (drag, one line in `view.rs`). No test (needs a masked entrance sample on a scrolled child). | baad394 |
| N35 (d) | Confirmed | Reading and new test `open_base_marker_stays_below_a_visible_layer`. The spec ("above other objects") is now "above the objects of its own layer". | baad394 |
| N35 (e) | Confirmed | Reading and new test `rectangular_clip_turns_with_the_object`: the painter clip is axis-aligned and applies to the turned meshes. The clip is now a turned quad in the mesh mask. | baad394 |
| N36 (a) | Confirmed | Reading: `getVar` only knew `course.variables`. Spec says it returns the current value. It now snapshots user, built-in and slide timer variables. wasm-only code, compiled by `cargo xtask wasm`, not run in a browser. | 035eaf9 |
| N36 (b) | Confirmed | Reading: closures were dropped after the sync run, so a later call throws. The helper functions are now guarded and switched off after the run (spec: later calls do nothing). Not run in a browser. | 035eaf9 |
| N36 (c) | Partly | The web video and iframe overlays did ignore the panel `clip_rect`. A DOM element cannot be cut to the viewport without a trait change, so an overlay that is not wholly inside the clip now hides and the clipped canvas poster shows (test `web_overlays_hide_when_a_scroll_panel_clips_them`). A partly scrolled video is therefore hidden, not cut. | 035eaf9 |
| N37 (a) | Partly | Confirmed: subpaths were ignored, and a gradient or picture `fill_style` with `fill: None` gave no inside hit (new tests). Not a bug: a transparent colour fill is still "a fill" (spec: "a closed path that has a fill"; PowerPoint also hits it). | dfc9739 |
| N37 (b) | Confirmed | Test `n37b_curved_fill_with_a_subpath_follows_the_curve`: the fill list used raw control points; it now uses the flattened curve. The test is a regression guard (not run against the old code). | dfc9739 |
| N37 (c) | Confirmed | Reading and test `n37c_table_border_dash_shows_gaps`: borders were always solid. | dfc9739 |
| N37 (d) | Confirmed | Reading and test `parts_keep_the_link_only_on_their_own_text`: hidden runs kept `link`. | dfc9739 |
| N37 (e) | Confirmed | Reading and test `n37e_links_follow_insets_and_wrap`: link regions and text effects ignored `wrap`, `insets` and vertical `direction`. Path text also had no link regions. Vertical direction has no test (the code shares one function for the box with the painter). | dfc9739 |
| N39 | Confirmed | Reading: six case-sensitive `ends_with` checks. The MIME choice is now `mime_for_src`, case-insensitive, with a unit test (the wasm function that calls it is not run). | 9db75bc |

## Notes

- N8, N35 and N28 touch other agents' files in small places: `player_frame.rs` (one condition in `show_captions`) and its test file (`dom: false` in two struct literals); `view.rs` (three lines in `lifted_items`).
- The N8 fix adds `MediaBackend::shows_video_captions` (default false, true for `WebBackend`) and a `dom` flag on `CaptionFrame`.
- Not verified in a browser: N36 (a), (b), N39 and the web caption path. They compile for `wasm32-unknown-unknown`.
