# M8 plan: faster authoring

Goal: the everyday speed of Storyline 360. An author can select many things, copy, paste, line them up, reuse layouts and fix text across a course without touching slides one at a time.

| Step | What | Depends on |
|---|---|---|
| 1 | Multi-select, marquee, group move and resize; copy, cut and paste of objects and slides | |
| 2 | Align and distribute, smart guides and snapping; grouping | 1 |
| 3 | Slide masters and templates | |
| 4 | Find and replace across a course | |
| 5 | Image crop | |
| 6 | M8 check: played by hand, roadmap, docs | all |

## Decisions that hold across the milestone

- **One user action is one undo step.** Actions on many objects go through `Session::execute_batch`, or a single command when the operation is one idea.
- **Play doesn't change for an existing course.** New model fields are optional and omitted when empty, so existing files stay byte-identical and `simulate` output doesn't move.
- **Selection is editor state.** `Selection.object` stays the primary object, and everything that handles one object keeps working.
- **Groups and masters are about authoring and drawing.** Neither adds play-time behaviour to `coursecraft-logic`.
- **A master is stored as a slide.** `course.masters` holds `Slide` values with no triggers, question or layers, so every object command and canvas tool works on them unchanged.
- **Clipboard goes through egui's copy, cut and paste events,** so it works in the browser as well as on the desktop, and between windows.
