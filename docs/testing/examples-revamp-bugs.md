# Bugs found while rebuilding the examples and templates (2026-11-27)

Found while building Workplace Safety Basics, the Feature Tour and the four starter templates on `examples-revamp`. Each one was checked by hand with the CLI.

## 1. The 0.6.4 Quick Knowledge Check template shows raw variable names to learners

**Severity:** high. Anyone who starts from this template sees it.

The results slide of `examples/templates/quick-knowledge-check.json` (as released in 0.6.4) says "Score: %Results.ScorePercent% (passing score: %Results.PassPercent%)". Neither variable exists (the built-ins are `Course.ScorePercent`, `Course.ScorePoints`, `Quiz.Passed` and so on), so the player draws the text as it is.

**Repro:**
```sh
git show bffb2a8:examples/templates/quick-knowledge-check.json > old.json
coursecraft-cli render old.json --slide s-results --mode player --at-ms 3000 -o old.png
```

**Status:** fixed on this branch. The new template uses `%Course.ScorePercent%` and shows the pass mark as plain text.

## 2. `validate` does not flag unknown `%variable%` references

**Severity:** medium. Bug 1 shipped because of this.

A text object that refers to a variable that is neither declared nor built in passes `validate` with 0 errors and 0 warnings. Changing `results-score` in the new Quick Knowledge Check template to `%Results.ScorePercent%%` still gives "0 error(s), 0 warning(s)".

**Suggested fix:** warn on every `%name%` in text, button labels, state text and question text whose name is not a course variable or a built-in. The XLIFF import already parses these references.

**Status:** open.

## 3. Button labels are always white, 20 px

**Severity:** medium (design).

The Button kind has only `label`, `fill` and `fill_style`. The player always paints the label white at 20 px with a 6 px radius (`crates/render/src/paint.rs`, around line 2846). On a light fill (hi-vis yellow, peach, white or a tint) the label fails contrast, and `a11y-check` reports WCAG 1.4.3 errors. Nothing in the model can change it, so the new examples draw light buttons as labelled shapes with a hover state (`soft_button` in `examples/tools/ccdesign.py`).

**Suggested fix:** pick the label colour for contrast with the fill, or add `label_style` (colour, size, weight) to the button kind and to `StateDef`.

**Status:** open.

## 4. The old in-app Feature showcase left out its caption files

**Severity:** low.

`welcome.rs` embedded only `banner.png`, `chime.wav`, `floorplan.png` and `intro.mp4` for the Feature showcase, but the course points at `intro.vtt` and `chime.vtt`. Opened from the welcome screen, the caption files were not in the course's media, so its captions could not load. (This was found by reading the code, not by playing the course.)

**Status:** fixed on this branch. Each example now embeds its media from one list, and `every_example_embeds_exactly_the_media_it_uses` (in `apps/coursecraft`) fails if a used file is not embedded or an embedded file is never used.

## Feature gaps (not bugs)

- **No by-word text sequence.** Text entrances offer `as_one`, `by_paragraph` and `by_letter` only.
- **The slider question ignores the question `frame`.** This was already known; it did not affect these courses.
- **The symbol fallback font is small (39 glyphs).** It covers ✓ ✕ ★ ⚠ → ● but not common UI glyphs such as ＋ ▤ ◐ ➜ ◎ ✺ ▮ ↻. `validate` warns about each one, which is good. The examples use shape icons instead.
