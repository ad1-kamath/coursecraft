# M47: Two bugs found while rebuilding the examples

Goal: fix the two open bugs from the examples rebuild (2026-11-27). The full report is copied at the end of this file. Bugs 1 and 4 in that report are already fixed on the `examples-revamp` branch and are NOT part of this milestone.

Rules for every step: AGENTS.md golden rules (never crash, everything is a command, one runtime, layering, wasm stays working, clean-room wording, `tl!` for every user-facing string). A runtime or rendering behaviour change updates `docs/spec/course-model.md` and/or `docs/spec/triggers.md` in the same step. Add a `CHANGELOG.md` line for each user-visible change under a `## Unreleased` heading placed above `## 0.6.4` (create the heading if it is missing). Use plain words and no internal ids.

Another branch, still unmerged, also edits `crates/course/src/validate.rs`: it adds a check that bank slides have no master. To keep merges simple, put new validation code in its own function, called from one line in the main validate path, and avoid reformatting code you don't change.

## Steps

1. **`validate` warns about unknown `%variable%` references.**
   Today a text object that uses `%Results.ScorePercent%` (no such variable) passes `validate` with 0 warnings, and the player draws the raw text.
   - Add a warning for every `%name%` reference whose name is neither a course variable (`course.variables`) nor a built-in variable (`Course.*`, `Quiz.*`, `Learner.*`, `Slide.*` and the others the runtime defines; find the real list in `coursecraft-logic` or `coursecraft-course` and reuse it, do not copy it).
   - Look in: object text and rich-text runs, shape text, button labels, state text, marker titles and bodies, question prompts and choices, and layer objects, master objects and template objects.
   - `%%` is an escaped percent sign and is not a reference. A lone `%` (as in "80%") is not a reference either. Reuse the parser the runtime already uses for substitution (the XLIFF import also checks `%var%` references), so the validator and the player agree on what a reference is.
   - Message, in sentence case: `Text uses %Name%, but the course has no variable called Name.` The path points to the object, the same way other object warnings do.
   - Tests:
     - an unknown name warns;
     - a declared variable, a built-in (`%Course.ScorePercent%`) and an escaped `%%` don't warn;
     - "Pass mark 80%" doesn't warn;
     - references in a layer object and in a button label are found.
   - Run `validate` on every course in `examples/` and `examples/templates/`: none of them may gain a warning. If one does, the reference is a real bug; fix the course and say so in the report.
   - Update `docs/spec/course-model.md` where validation warnings are listed.

2. **Button labels stay readable on light fills.**
   Today the player always paints a button label white at 20 px (`crates/render/src/paint.rs`, around line 2847, `Color32::WHITE`). On light fills (yellow, peach, white, tints) the label is unreadable, and `a11y-check` reports WCAG 1.4.3 errors that the author cannot fix.
   - Pick the label colour for contrast with the button's effective fill: white, or the dark ink `#1F1F1F`, whichever has the higher WCAG contrast ratio against the fill. For a transparent fill, use the fill composited over the slide background when that is known, and white otherwise. Keep today's white wherever white already has the higher contrast, so every existing course with a dark button looks exactly the same.
   - Put the choice in one shared function (for example `button_label_color(fill) -> Color` in `coursecraft-course` or `coursecraft-render`, whichever the layering allows) and use it in:
     - the renderer (`paint.rs`, base and hover or other states, since a state can change the fill);
     - the accessibility audit (`crates/course/src/audit.rs`, around line 409, which assumes `WHITE` today).

     The audit and the painter must agree.
   - Do not add a new model field in this step.
   - Tests:
     - a dark fill (`#1F4E8C`) gives white;
     - a light fill (`#F2B705` and `#FFFFFF`) gives the dark ink;
     - the audit reports no 1.4.3 error for a yellow button and still reports one where neither colour reaches the threshold, if such a fill exists (otherwise test the chosen ratio).
   - Update `docs/spec/course-model.md` (Button kind: the label colour is chosen for contrast). Check the snapshot or golden render tests and update any that change, explaining each one in the review.

3. **Gate, player build, perf and report** (driver steps).

## Owner's manual checks

- Open a template, add a button with a yellow fill, and check that the label is dark and readable in the editor and in Preview.
- Type `%Nope%` into a text box and check that Check Course or `validate` warns about it.

## Reviewer notes

- Step 1 must reuse the runtime's reference parser. A second regex that disagrees with the player is a bug.
- Step 2 must not change how any existing dark button looks.

---

## Copy of `docs/testing/examples-revamp-bugs.md` (from the examples-revamp branch)

1. **The 0.6.4 Quick Knowledge Check template shows raw variable names to learners.** Its results slide says "Score: %Results.ScorePercent% (passing score: %Results.PassPercent%)", and neither variable exists. *Fixed on examples-revamp.*
2. **`validate` does not flag unknown `%variable%` references.** Bug 1 shipped because of this. *Step 1 above.*
3. **Button labels are always white, 20 px.** On light fills the label fails contrast, and nothing in the model can change it. The examples work around it by drawing light buttons as labelled shapes. *Step 2 above.*
4. **The old in-app Feature showcase left out its caption files** (`intro.vtt`, `chime.vtt`). *Fixed on examples-revamp.*

Feature gaps (not in scope): no by-word text sequence; the slider question ignores the question frame; the symbol fallback font has only 39 glyphs.
