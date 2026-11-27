# M47 Report — Two Bugs Found While Rebuilding the Examples

**Milestone:** M47 (Two Bugs Found While Rebuilding the Examples, pending release)  
**Plan:** [`docs/testing/m47-plan.md`](m47-plan.md)  
**Scope:** Two bug fixes from the 2026-11-27 examples rebuild across steps `S1`–`S2` (bugs 1 and 4 from that report were already fixed on `examples-revamp` and are out of scope): warning on unknown `%variable%` references in `Course::validate` using one shared `%Name%` parser with the runtime (`S1`), and automatically choosing a contrasting button label colour (`#FFFFFF` or dark ink `#1F1F1F`) in both the renderer and accessibility audit (`S2`).

## What Shipped Per Step (`S1`–`S2`)

| Step | Summary | Review Verdict |
|---|---|---|
| **S1** | Added and re-exported `parse_variable_reference_at(text: &str, start: usize)` in `crates/course/src/strings.rs` / `crates/course/src/lib.rs`, rewrote `variable_reference_spans` on top of it, and updated `Player::display_text` in `crates/logic/src/lib.rs` to use the same parser so `%%` escapes and lone `%` signs (e.g., `"80%"`) behave identically in validation and playback. Added a single-line `check_unknown_variable_refs(self, cx)` hook in `Course::run_validate` (`crates/course/src/validate.rs`, keeping `check_bank_slide` untouched for merge safety) that validates `%Name%` references against declared `course.variables`, `builtin_variable_kind(name)`, and `is_timer_remaining_variable_on(slide, name)` across scenes, banks, masters, and templates (base and layer objects; text and rich-text runs, shape and path text, button labels, marker titles/bodies, table cells, state text and extras, and question prompts, choices, and matching pairs), emitting `Severity::Warning` with `Text uses %Name%, but the course has no variable called Name.`. Fixed two real course bugs found while validating all 10 bundled courses: replaced `%Results.ScorePercent%%` with `%Course.ScorePercent%%%` in `examples/rich-states/course.json` and replaced `%Results.ScorePercent%%` / `%Results.PassPercent%%` with `%Course.ScorePercent%%%` / `80%` in `examples/templates/quick-knowledge-check.json` (both now validate with `0 error(s), 0 warning(s)`). Added a `Validation` bullet under `## Unreleased` in `CHANGELOG.md`. Review added the missing unknown-`%Name%` entry in `docs/spec/course-model.md` (`## Validation`, also fixing 3 stray cp1252 `0x85` bytes on line 697 with UTF-8 `…`), added `StateExtraKind::Path { path }` text scanning in `validate.rs` to match `compose.rs`, and added a regression test. | `FIXED` |
| **S2** | Added and re-exported `BUTTON_DARK_INK` (`#1F1F1F`) and `button_label_color(fill, backdrop)` in `crates/course/src/audit.rs` / `crates/course/src/lib.rs` to pick `#FFFFFF` or `#1F1F1F` by higher WCAG contrast ratio (compositing translucent fills over `backdrop` when known, falling back to white when unknown, and keeping white whenever white already has higher contrast so existing dark buttons stay pixel-identical). Updated `Auditor::check_button` to use `button_label_color(background, None)` instead of hardcoded `WHITE`. Updated `paint_items` and `paint` in `crates/render/src/paint.rs` (and all 5 `paint_items` call sites in `crates/render/src/view.rs`) to accept `slide_bg: Option<&Color>` and compute `Visual::Button` label colour via `coursecraft_course::button_label_color` from `effective_fill` (covering base, hover, down, and custom state fills) before `apply_dim`, with no new model fields and zero golden/snapshot render changes. Documented the contrast rule in `docs/spec/course-model.md` and added an `Authoring` bullet under `## Unreleased` in `CHANGELOG.md`. | `OK` |

## Test Coverage (`M47-01`–`M47-11`, 11 New Passing Tests)

| ID | Step | Crate / File | Coverage |
|---|---|---|---|
| `M47-01` | S1 | `coursecraft-course` (`tests.rs`) | `m47_s1_unknown_variable_reference_warns`: unknown `%Results.ScorePercent%` emits `Severity::Warning` at `/scenes/0/slides/0/objects/0` with exact message `Text uses %Results.ScorePercent%, but the course has no variable called Results.ScorePercent.`, and `is_playable()` remains `true` |
| `M47-02` | S1 | `coursecraft-course` (`tests.rs`) | `m47_s1_declared_builtin_timer_and_escaped_percent_do_not_warn`: declared course variable (`%score%`), built-in (`%Course.ScorePercent%%%`), slide timer (`%Timer.t1.Remaining%`), and escaped `%%Nope%%` / `100%%` emit no unknown-variable warning |
| `M47-03` | S1 | `coursecraft-course` (`tests.rs`) | `m47_s1_lone_percent_does_not_warn`: `"Pass mark 80%"` and `"80% to 90%"` do not warn while `"Pass mark 80% (%Missing%)"` warns once at `/scenes/0/slides/0/objects/2` for `Missing` |
| `M47-04` | S1 | `coursecraft-course` (`tests.rs`) | `m47_s1_layer_button_state_marker_question_master_and_template_refs_are_found`: unknown references are detected with expected paths in button labels, shape text, rich-text runs, state text, marker titles and bodies, layer objects, question prompts and choices, masters, and templates |
| `M47-05` | S1 | `coursecraft-course` (`tests.rs`) | `m47_s1_examples_and_templates_have_no_unknown_variable_warnings`: validates all 10 bundled courses under `examples/` and `examples/templates/` and asserts zero unknown-variable warnings |
| `M47-06` | S1 | `coursecraft-course` (`tests.rs`) | `m47_s1_state_extra_path_table_bank_and_pairs_refs_are_found_without_run_duplicates` *(added in review)*: detects unknown references in `StateExtraKind::Path` text, table cells, bank slides, and matching pairs, and confirms rich-text `runs` suppress duplicate scanning of fallback `text` |
| `M47-07` | S1 | `coursecraft-logic` (`tests.rs`) | `m47_s1_display_text_and_validator_parser_agree`: confirms `Player::display_text` and `coursecraft_course::variable_reference_spans` agree on `%Course.ScorePercent%%%`, lone `%`, mixed text + variable, `%%score%%`, and unknown `%Missing%` |
| `M47-08` | S2 | `coursecraft-course` (`audit/tests.rs`) | `m47_s2_button_label_color_picks_white_on_dark_and_dark_ink_on_light`: `#1F4E8C` and `#2F6FEB` return `WHITE`; `#F2B705` and `#FFFFFF` return `BUTTON_DARK_INK` (`#1F1F1F`) |
| `M47-09` | S2 | `coursecraft-course` (`audit/tests.rs`) | `m47_s2_button_label_color_handles_transparent_fills`: transparent `#00000000` and semi-transparent `#F2B70580` composite over known backdrops (`#FFFFFF` → `BUTTON_DARK_INK`, `#1F4E8C` → `WHITE`) and fall back to `WHITE` when backdrop is `None` |
| `M47-10` | S2 | `coursecraft-course` (`audit/tests.rs`) | `m47_s2_audit_yellow_button_passes_and_mid_grey_button_fails_contrast`: yellow `#F2B705` button passes contrast with no finding, while mid-grey `#7D7D7D` button still reports a `WCAG 1.4.3` `AuditSeverity::Error` |
| `M47-11` | S2 | `coursecraft-render` (`tests.rs`) | `m47_s2_paint_button_label_contrasts_with_fill_and_states`: `paint()` draws `WHITE` on `#1F4E8C`, `#1F1F1F` on `#F2B705` and `#FFFFFF`, switches to `#1F1F1F` on hover when a dark button has a yellow `hover_fill`, and resolves transparent fills against `slide_bg` (or `WHITE` when unknown) |

## Driver Gates

- **`cargo xtask ci` (`g1` — `g1-1.log`, after `S2` + review): passed on attempt 1**
  - `cargo fmt --all --check`: passed.
  - `cargo clippy --workspace --all-targets -- -D warnings`: passed (0 warnings, `52.93s`).
  - `cargo test --workspace`: **4,708 passed; 0 failed; 17 ignored** (test build finished in `1m 30s`).
  - `cargo xtask layers`: `layers: ok`.
  - `cargo check --target wasm32-unknown-unknown` across all 17 WASM crates: passed (`27.15s`).
- **`cargo xtask build-player` (`p1` — `p1-player.log`): passed** (`wasm-release` finished in `1m 52s`, written to `target/wt-m47/target/player-dist`).
- **Release editor perf suite (`perf.log` — `coursecraft-ui-egui::perf_tests`, `--release --ignored`): passed** (`9 passed; 0 failed; 0 ignored; 1673 filtered out` in `2.45s`; release build finished in `2m 57s`).

## Measured Performance (`perf.log`) vs. Baseline

`docs/testing/m47-plan.md` does not define a separate performance baseline table. Below are the exact measured numbers from `/Users/aditya/Documents/Claude Projects/ScormCraft/target/m47/perf.log` compared against the budgets and recorded baseline in [`docs/performance.md`](../performance.md) (with M45's measured figure from [`docs/testing/m45-report.md`](m45-report.md) shown for `low-end lite 300 slides`, which post-dates the M21 baseline table):

| Stage (300-slide course, 1280×900) | Budget (`docs/performance.md`) | Baseline (`docs/performance.md`, median / p95) | Measured (`perf.log`, median / p95) | Notes |
|---|---:|---:|---:|---|
| `idle` | 16 ms | 2.1 / 2.2 ms | **0.40 ms / 0.41 ms** | Well under 16 ms budget |
| `select slide 150` | 33 ms | 2.1 / 2.1 ms | **0.64 ms / 0.80 ms** | Well under 33 ms budget |
| `scroll slide list` | 33 ms | 1.9 / 2.1 ms | **0.36 ms / 0.38 ms** | Well under 33 ms budget |
| `typing in a text field` | 33 ms | 2.0 / 2.0 ms | **0.54 ms / 0.55 ms** | Well under 33 ms budget |
| `type on canvas with spell check` | 33 ms | 0.42 / 0.44 ms | **0.78 ms / 0.81 ms** | Well under 33 ms budget |
| `check spelling, open` | 250 ms | 31 ms | **36.96 ms** | Well under 250 ms budget |
| `check spelling, ignore step` (20 issues) | 33 ms | 9.5 / 10.0 ms | **0.44 ms / 0.58 ms** | Well under 33 ms budget |
| `insert interaction + undo` (8 kinds) | 100 ms | 2.0 / 2.2 ms | **1.17 ms / 1.61 ms** | Well under 100 ms budget |
| `undo / redo move` (30 cycles) | 33 ms | 8.2 / 8.7 ms (M33 target ≤ 6.5 / 7.5 ms) | **2.98 ms / 3.31 ms** | Well under budget and M33 target |
| `first frame` | 500 ms | 13 ms | **8.58 ms** | Well under 500 ms budget |
| `visit every slide` | 33 ms | 1.9 / 1.9 ms | **0.43 ms / 0.72 ms** | `rss 268 -> 269 MB` |
| `visit every slide, real pictures` | 33 ms | 4.2 / 7.0 ms | **0.44 ms / 0.72 ms** | `rss 274 -> 276 MB, media 0.2 MiB, 1 decoded pictures` |
| `low-end lite 300 slides` | 33 ms | 14.84 / 35.72 ms (`m45-report.md`) | **15.15 ms / 34.95 ms** | `media 4.4 MiB, undo 0.0 KiB` (test passed; median under 33 ms, p95 down slightly vs. M45) |

## Skipped Steps & Open Concerns

- **Skipped steps:** None — `/Users/aditya/Documents/Claude Projects/ScormCraft/target/m47/SKIPPED` does not exist. Both steps (`s1` and `s2`) completed, committed, and passed review.
- **Open concerns from `/Users/aditya/Documents/Claude Projects/ScormCraft/target/m47/REVIEW.md`:**
  1. **Canvas inline editing on light buttons (out of `s2` crate scope):** `crates/ui-egui/src/canvas.rs` (`inline_edit_text_props` for `ObjectKind::Button`, line 6740) still hardcodes `#ffffff` while inline-editing a button label on the editor canvas; consider switching it to `button_label_color` in a follow-up so typing directly on light buttons stays visible.
  2. **Step `s1` review findings (resolved during review):** `s1` received `Verdict: FIXED` after Neon's review added the missing `## Validation` entry in `docs/spec/course-model.md` (replacing 3 stray cp1252 `0x85` bytes on line 697 with UTF-8 `…`), added `StateExtraKind::Path { path }` text scanning in `crates/course/src/validate.rs`, and added `m47_s1_state_extra_path_table_bank_and_pairs_refs_are_found_without_run_duplicates`. No open issues remain on `s1`.
  3. **Uncommitted `CHANGELOG.md` entries:** Both step reviews noted that the `## Unreleased` entries in `CHANGELOG.md` remained unstaged in the working tree (the driver's `commit_step` stages `crates docs apps xtask examples Cargo.toml Cargo.lock`, not `CHANGELOG.md`); include `CHANGELOG.md` when landing M47.
  4. **Pre-existing WASM / binary compiler warnings (unrelated to M47):** `cargo xtask build-player` (`p1`) and the `wasm32-unknown-unknown` check in `g1` passed with 16 pre-existing `dead_code` warnings in `crates/playback/src/lib.rs` and 3 pre-existing warnings (`unused_mut`, `dead_code`) in `apps/coursecraft/src/main.rs`.

## Owner Manual Verification Checklist

*(From `docs/testing/m47-plan.md` § Owner's manual checks.)*

- [ ] Open a template, add a button with a yellow fill, and check that the label is dark and readable in the editor and in Preview.
- [ ] Type `%Nope%` into a text box and check that Check Course or `validate` warns about it.
