# Neon 0.6.1 review: verification of the engine and course-model bugs

Scope: N62-N75, N120, N122-N125 (crates `engine` and `course`, plus `docs/spec`).
Method: each bug was reproduced with the Neon proof test (`--ignored`), with a new test in
`crates/engine/tests/neon_verify_tests.rs`, or by reading the code.

| # | Verdict | Evidence | Fix commit |
|---|---|---|---|
| N62 | Confirmed | Neon test fails: `text_style.create` returns Ok and leaves 5 validate errors (level, line_spacing, space_before, bullet_color, start_at). | d408145 |
| N63 | Confirmed | Neon test fails: `object.set_frame` leaves a connector frame wider than 100000 px after `keep_valid` ran. | d408145 |
| N64 | Confirmed | Neon test fails: `theme.accent` stays `#2F6FEB` when the palette is non-empty and the theme is default. | d408145 |
| N65 | Confirmed | Root cause still present: `EFFECTS[0]` has `default_duration_ms: 0` (`effects.rs`). The ribbon gallery now falls back to 500, but `properties.rs` and any API user still get 0. Neon test fails. | d408145 |
| N66 | Confirmed | Neon test fails: `course.replace` with 0 matches materialises `state.text` with no undo step. | 32af8ae |
| N67 | Confirmed | Neon test fails: overlapping merges accepted. Validator does not detect overlapping spans either. | ddbf24e, e0f583c |
| N68 | Confirmed | Neon test fails: `layer.duplicate` fails on a duplicated group id. | ddbf24e |
| N69 | Confirmed | Neon test fails (`Timer.t1.Remaining` refused). The background-audio trigger drop is the same root cause (`remap_*` does not know the reserved ids). | ddbf24e |
| N70 | Confirmed | Neon course test fails: `validate()` gives no error for a bad outline on Text and Image. | e0f583c |
| N71 | Confirmed | New tests fail: `slide.paste` with a missing slide link in a state run is refused by `keep_valid`. `slide.duplicate` leaves self links in state runs and state extras on the old slide. | ddbf24e |
| N72 | Partly | New tests fail for: `object.delete` on a timer used in `Timer.<id>.Remaining` and on a drag target; `layer.delete` with an interactive group trigger and with a question feedback layer; `slide.delete`/`scene.delete` with a `Link::Slide` run; `course.set_background_audio {src:null}` with a `Course.BackgroundAudio` trigger. `question.delete` gaps confirmed by reading (event/action variants and timeline triggers missing). `start_slide` and `tracking.quiz.slide` after `slide.delete` are covered by the same test. | 2979ccc, 819566b (test) |
| N73 | Confirmed | Neon test fails: `object.apply_style` onto a line shape is refused. | ab278a7 |
| N74 | Confirmed | Neon course test fails: a disabled `complete_course` trigger counts as reporting. | e0f583c |
| N75 | Partly | Real for very small slides: `interaction.insert` fails at slide side 1 (the margin has a floor of 20 px, so widths go negative). The report's example (40x40) passes. The NaN claim for `slide_size.rs` is wrong: `set_slide_size` already guards `old.w > 0`, `old.h > 0` and a finite factor. | ab278a7 |
| N120 | Confirmed | Neon test fails. Code is wrong: `course.add_font` rejects `.ttc`/`.otc` that the validator and the spec accept. | 32af8ae |
| N122 | Confirmed | Validator accepts `1..=8192` on both sides; `course.set_slide_size` and the spec say `100..=7680` x `100..=4320`. The validator is the load gate and must stay loose for old files, so the spec is what changes. | 0a60235 (spec) |
| N123 | Confirmed | `validate.rs` warns that a state `src` applies only to image objects, but `compose.rs` applies it to Image, Video, Audio and Lottie. Scene ids are not checked with `valid_name` and scene titles may be empty although the spec says they must not. Validator is outdated. | e0f583c |
| N124 | Confirmed | Spec line is wrong. `bridge.rs` blocks only `Incomplete` after `completed` (SCORM 2004); `Failed` is allowed because completion and success are separate. After `passed`, both are blocked. | 0a60235 (spec) |
| N125 | Confirmed | Spec drift verified for (a) path text, (b) slide group ids (code regenerates `grp-N`), (c) `interaction.insert` kinds, (d) duplicate `object.set_drag` row in `cli.md`. | 0a60235, 21b5dcd (spec and CLI doc) |

## Notes on the fixes

- N65: the root cause was fixed: `appear` now has `default_duration_ms: 1` (1 ms is the shortest legal duration and looks instant). The registry test now requires every default to be in `1..=60000`.
- N67: `table.merge` refuses a range that overlaps a merge owned from outside the range (it may still swallow merges wholly inside it). Validation gives a warning, not an error, for overlapping spans, so older files that already hold one still open.
- N70: errors for outlines, path `fill_style`, `subpaths`, path `text` and non-finite insets. Unsafe media paths, odd insets, scene ids and empty scene titles are warnings only, because an older CourseCraft could have saved them and old projects must still open.
- N72: `question.delete` now also treats key, marker, cue, drop, intersect and timer events, timer actions and timer variables as uses of a button, and keeps a feedback layer that a timeline event names. A reference that only a question holds (for example `correct_when` on a deleted timer) still makes the command fail with a message that names it; this is deliberate.
- N75: margins, top and gap in `interaction.insert` are capped by the slide size. At slide side 1 some kinds can still draw 1 px wide pieces, but the course stays valid.
- N122: the validator keeps `1..=8192`, because tightening it would stop older projects from opening. `course.set_slide_size` keeps `100..=7680` x `100..=4320` as an authoring limit. The spec now says both.
- N123: scene id and title problems are warnings (not errors) for the same compatibility reason.
- N124: the spec was wrong, so the spec changed. The code is correct and unchanged.
- N125: updated the checkable items (a), (b), (c) and (d) except the `view.set last_publish` claim, which is wrong: `last_publish` is set by its own command. Item (e) (`FillStyle::Pattern`, `tile_scale` and some shipped `Action`/`Event`/shape entries in `course-model.md`) is not done.
