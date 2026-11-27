# Neon review 0.6.1: verification of PPTX and hostile-input bugs

Scope: N51, N52, N76 to N83. Base: `fixes-0.6.1`. Verdicts come from reading the code and from running or writing a test. "Failed before" means the test was run against the unfixed code and failed. "By code" means the defect is plain in the code and the test was only run after the fix.

| # | Verdict | Evidence | Fix commit |
|---|---|---|---|
| N51 | Confirmed | Neon test `pptx_motion_path_scientific_notation_truncated` failed before (0 motion paths, expected 1). `tokenize_motion_path` split on `e`/`E` inside numbers. Now `e`/`E` after a digit and before a digit or sign is an exponent. | f271f3a (test enabled in 5b12279) |
| N52 | Confirmed | New test `pptx_animation_target_id_does_not_leak_to_master_shapes` failed before (animated: MasterBox and SlideBox). Master and layout ids shared `pptx_ids` with the slide. Now connectors are settled and ids cleared after master and after layout. | 063c755 |
| N76 | Confirmed | By code: `parse_hex_rgb` checked byte length and sliced; `"中中"` is 6 bytes. Neon test now passes. A second slicing site, `blend_hex_colors` (`h.len() >= 6`), had the same defect and is fixed too. | 39494ab |
| N77 | Confirmed | By code: `roman` loops once per 1000. Fix is at render time: Roman schemes write digits above 3999. Validation gives a warning (not an error) for `start_at` above 99999, so old files still load. The engine command already caps at 99999. Neon test enabled and passes. | beb0080 |
| N78 | Confirmed | By code: no `take`, no path check, no duplicate check in `verify_project_with_limits`. Now it mirrors `open_project`: path safety, duplicates, declared size check, and reads capped at limit + 1. Neon test enabled; new test `verify_project_refuses_deflate_bomb_entry` added. | 3e31283 |
| N79 | Confirmed (all four parts) | By code and new tests. Diamond DAG (25 levels) and four stacked 256-copy repeaters passed `parse_composition` (memoized visit; per-repeater check). Now a work budget (500,000 shapes per frame, repeaters in a group multiply, precomposition instances multiply) refuses them at load. `EpaintSink::draw` now returns at once after the triangle budget is spent. `paint_composition` returned `sink.warned`, the inverse of its doc; fixed to `!sink.warned` (the only caller ignores it). | a82f983 |
| N80 | Confirmed | By code: `cells.resize(column_count, ..)` per row with no axis or cell cap. Now a table over 50 rows or columns, or 2,500 cells, is skipped with a report note before any allocation. New test `pptx_huge_table_is_skipped_not_allocated` (5000 x 5000) passes in milliseconds. | 23948dc |
| N81 | Confirmed (shape cap); Partly (point cap) | Neon test failed before (20 objects with cap 5). Fixed: paths stop at the remaining shape room, with a report note. The point cap part is real but minor: temporary points are bounded by XML size. `cubicBezTo`, `quadBezTo` and `arcTo` now check `PATH_MAX_POINTS` like `lnTo`. | 5b12279 |
| N82 | Confirmed | By code: `Instant + Duration::from_secs(u64)` panics on overflow. The code is in `apps/coursecraft-lti` (package `coursecraft-lti-server`), not `crates/lti`. New `token_expiry` clamps the life to 24 h and uses `checked_add`. Test with `u64::MAX` added. | 421c930 |
| N83 | Partly | Resampler part confirmed: a 1 Hz source multiplies the output 48,000 times. Sources below 1,000 Hz now stay silent (telephone audio is 8 kHz). Condition walkers: `validate.rs` already has `MAX_CONDITION_DEPTH` (32), so not a bug there. `describe_condition` in `course_map.rs` had no cap and is now cut at that depth. The cited engine site (`commands/course.rs:923`) is `variable_set_value`, not a condition walker: not a bug. | ab77586 |

## New limits

- Lottie: 500,000 shapes per frame after expanding precompositions and repeaters (`MAX_DRAW_WORK`). Documented in `docs/spec/course-model.md`.
- Roman list numbers: digits above 3999. Validation warning for `start_at` above 99999. Documented in `docs/spec/course-model.md`.
- PPTX import: table over 50 rows or columns or 2,500 cells is skipped; custom-geometry paths count against `max_shapes_per_slide`. Documented in `docs/pptx-import.md`.
- Audio resampler: source rate below 1,000 Hz is silent.
- LTI token cache: platform `expires_in` clamped to 24 h.
- `verify_project`: same entry limits as `open_project`, now enforced while reading.
