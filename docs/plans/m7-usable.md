# M7 plan: usable without the command line

Goal: an author who has never opened a terminal can build a course in the visual editor, publish it and hand the zip to an LMS. Everything in the plan is checked by hand in the editor and player, which M5 and M6 skipped.

| Step | What | Depends on |
|---|---|---|
| 1 | Publish from the editor: player bundled in the app, standard picker, zip output | |
| 2 | Close the editor gaps a hand walk finds: anything only reachable from the CLI or MCP | |
| 3 | The two M6 finds: dropped drag label, notice for a refreshed cmi5 launch | |
| 4 | Hand test of editor and player, fixing what it finds | 1, 2, 3 |
| 5 | M7 check: roadmap, docs | all |

## Decisions that hold across the milestone

- **The player ships inside the desktop editor.** `cargo xtask build-player` builds the release web player into `target/player-dist/`. `apps/coursecraft/build.rs` embeds that folder at compile time. A build made without it still compiles and runs, and its Publish dialog says what's missing, so CI and ordinary `cargo run` don't need trunk.
- **Publishing isn't an edit.** It changes no course state and has no undo, so like Save it's an `EditorRequest` served by `apps/coursecraft`, using the same `coursecraft-publish` functions as the CLI.
- **The web editor doesn't publish yet,** like its other file operations.
- **Validation errors block publishing,** with a way to see them. Accessibility findings never block.
- **Authors see plain words.** "SCORM 1.2" is a name they know from their LMS; "marker", "manifest" and "cmi.interactions" aren't.
