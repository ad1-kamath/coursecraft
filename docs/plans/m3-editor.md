# M3 plan: editor MVP

Goal (from `docs/roadmap.md`): someone can build `examples/hello-course` from scratch without touching JSON. The editor is egui. Every action goes through `coursecraft-engine` commands, and projects save as `.ccraft`.

## Shape

```text
coursecraft-engine (L5)    + the missing editing commands; optional caller ids
coursecraft-format (L3)    .ccraft = zip(course.json + media/); safe paths; size limits
coursecraft-ui-egui (L6)   Editor: menus from the registry, slides, canvas, panels, dialogs, preview
apps/coursecraft           eframe shell: file open/save/recent/autosave; desktop + wasm build
```

**Rule:** the UI never edits `session.course` directly. It reads the course to draw, and changes it only with `session.execute(id, params)`. Undo/redo, atomicity and validity come from the engine.

## 1. Engine command completeness (`crates/engine`)

New commands, following `docs/contributing.md`. Each one validates its params, calls `keep_valid`, gets a `REGISTRY` row and a menu path where it has one, and has a test. `panic_hunt` covers hostile params.

| id | params |
|---|---|
| `course.update` | `{"title"?: string, "id"?: string, "start_slide"?: id\|null}` (replaces `course.set_title`; keep `course.set_title` as an alias) |
| `course.set_player` | `{"show_nav"?: bool, "show_menu"?: bool, "show_seekbar"?: bool}` |
| `course.set_quiz` | `{"passing_percent": 0..=100 \| null}` |
| `scene.update` / `scene.delete` | `{"scene": id, "title"?: string}` / `{"scene": id}` (refuse to delete the last scene) |
| `slide.update` | `{"slide": id, "title"?: string, "duration_ms"?: u32}` |
| `slide.move` | `{"slide": id, "scene"?: id, "index": usize}` |
| `layer.update` / `layer.delete` | `{"slide", "layer", "name"?, "duration_ms"?, "hide_other_layers"?, "pause_base_timeline"?, "prevent_base_clicks"?}` / `{"slide", "layer"}` |
| `object.update` | `{"slide", "object", "name"?, "kind"?: ObjectKind, "timing"?: Timing, "initial_state"?: string}` |
| `object.reorder` | `{"slide", "object", "index": usize}` (z-order within its own list) |
| `object.duplicate` | `{"slide", "object"}` → `{"id"}` (offset +20,+20, fresh id) |
| `object.update_state` / `object.delete_state` | `{"slide", "object", "state": StateDef}` (by name) / `{"slide", "object", "name"}` |
| `trigger.update` / `trigger.move` | `{"slide", "layer"?, "index", "trigger": Trigger}` / `{"slide", "layer"?, "from", "to"}` |
| `variable.set_value` / `variable.delete` | `{"name", "value"}` (same type only) / `{"name"}` (refused while referenced) |
| `question.delete` | `{"slide"}` |

- `scene.add`, `slide.add`, `layer.add` and `object.add` accept an optional `"id"`. It must be non-empty, at most 64 characters of `[A-Za-z0-9_-]`, and unique in its scope. Without it, `fresh_id` is used as today. Agents and the rebuild script need stable ids.
- **Acceptance:** `examples/hello-course/rebuild.script.json` rebuilds hello-course from `Session::blank` using commands only. An engine test applies it and checks the result equals `examples/hello-course/course.json` (parsed, compared as `Course`). This proves the command set can build the course. Keep `build.script.json` as it is.
- **The rebuild comparison is strict:** `Course == Course` on the parsed files, with no normalising, ignored fields or custom equality, and `examples/hello-course/course.json` is not edited. Where an engine default would break equality (for example `object.add` filling in `name`, or `Session::blank`'s `scene1`/`slide1`/5000 ms), the script sets it with a param instead. The script deletes the blank `scene1`/`slide1` after adding the real ones; the last-scene and last-slide guards stay as they are.

## 2. Project format (`crates/format`, L3)

- `pub fn save_project(course: &Course, media: &[(String, Vec<u8>)]) -> Result<Vec<u8>, FormatError>` writes a zip with `course.json` (pretty) and `media/<path>`, deterministically.
- `pub fn open_project(bytes: &[u8]) -> Result<Project, FormatError>` returns `Project { course, media }`. It is hostile-input safe:
  - Cap entries at 10,000, total uncompressed size at 512 MiB, and any single entry at 256 MiB. Check declared and actual sizes while reading.
  - Every path passes `is_safe_relative_path` (move it here from `coursecraft-publish`, which then depends on `format`).
  - Ignore unknown entries outside `media/`; reject duplicates.
  - Require `course.json`.
  - Parse it with `Course::from_json`.
  - Validation errors are allowed on open, since the editor shows them; only parse errors fail.
- Register L3 in `LAYERS` and the wasm list.

## 3–6. Editor (`crates/ui-egui`, L6)

`pub struct Editor { session: Session, selection: Selection, ui: UiState }`, with `pub fn show(&mut self, ui: &mut egui::Ui) -> Vec<EditorRequest>`. Requests are what the shell must do outside egui: open, save, save-as, new, preview, quit. The editor never touches the filesystem.

Layout, the conventional slide-editor arrangement, designed for CourseCraft:

```text
┌ menu bar (from registry menu paths + File items) ─────────────────────────┐
│ slides  │            canvas (selected slide)            │ properties      │
│ panel   │                                               │ (selected obj)  │
│ (scenes │                                               ├─────────────────┤
│ +thumbs)│                                               │ layers / states │
├─────────┴───────────────────────────────────────────────┴─────────────────┤
│ bottom tabs: Timeline │ Triggers │ Variables │ Question                   │
└───────────────────────────────────────────────────────────────────────────┘
```

- **Step 3a: authoring compose (in `coursecraft-render`).** `compose` reads clocks and visible layers from a live `Player`, and `Player::new` refuses invalid courses, which the editor must still be able to show. Add `compose_authoring(course: &Course, slide: &Slide, focus_layer: Option<&str>) -> Vec<DrawItem>`. It draws all base objects regardless of timing at their `initial_state`, plus the question prompt and choices, then the focus layer's objects. Base items get a `dimmed` flag when a layer is focused, which `paint` honours. It shares the per-object resolver with `compose`, so the canvas, the thumbnails and the player draw identically. It's used for canvas drawing, thumbnails and selection hit-testing, and it has tests.
- **Errors are never dropped.** `keep_valid` refuses ordinary edits, such as deleting a layer a trigger points at or deleting a referenced variable. Every `execute` error is shown in the status bar or a toast. Pickers (trigger targets, layers, variables, states) offer only things that exist. A test checks that a refused command shows its error and leaves `revision` unchanged.
- **Step 3: shell, slides, canvas display, selection.**
  - The menu bar is generated from `registry()` menu paths, plus File (New/Open/Save/Save As/Recent/Quit as `EditorRequest`s).
  - Shortcuts come from registry `shortcut` strings.
  - The slides panel lists scenes and slides, with thumbnails drawn by `coursecraft-render` `compose` + `paint` at small scale. It supports select, add, delete and drag-to-move (`slide.move`).
  - The canvas draws the selected slide with the base layer, or the selected layer over a dimmed base, using `compose_authoring` + `paint`.
  - Click selects (topmost), and selection draws a frame with 8 resize handles.
  - Delete removes the selection.
- **Step 4: canvas editing and properties.**
  - Move and resize use drag with a preview. The course is untouched while dragging, and `object.set_frame` runs once on release, giving one undo step. Optional snap to 10 px with Shift.
  - Insert tools (Text, Shape, Button, Image) via menu or toolbar: click on the canvas places a default-sized object, or drag to draw its frame (`object.add`).
  - The properties panel edits name, kind-specific fields (text, style size/colour/bold, label, fill, shape, src/alt), frame numbers, timing start/end and initial state. Each committed field edit is one `object.update`; text fields commit on Enter or focus loss, not per keystroke.
- **Step 5: layers, states, timeline.**
  - The layers panel lists the base layer and layers, with add, delete, rename and properties (`layer.update`).
  - The states panel lists the selected object's states, with add/edit/delete (fill/text/src) and the initial state.
  - The timeline shows the slide or layer duration and one bar per object (start→end). Bars drag to change timing (one `object.update` on release), and the duration is editable.
- **Step 6: triggers, variables, question.**
  - The triggers panel lists triggers for the current owner (base or selected layer) as sentences ("Jump to next slide when the user clicks btn-start"). It supports add/edit/delete/move.
  - The trigger editor is a form: action picker plus its fields, event picker plus its fields, and an optional condition builder (compare / object_state, combined with all/any/not, nesting allowed).
  - The variables dialog supports add, set the initial value, and delete.
  - The question form edits kind, prompt, choices (add/remove/correct), points, attempts, and correct/incorrect layer pickers, through `question.set` / `question.delete`.
  - Course settings: title, player chrome toggles, quiz passing percent.
- **Validation surface:** after every revision change, run `course.validate`. Show the error/warning count in a status bar, and a clickable list that selects the offending slide or object where the path allows.

## 7. Editor app (`apps/coursecraft`)

- eframe, desktop and wasm, with the title "CourseCraft".
- **Desktop:**
  - New / Open / Save / Save As through `rfd` native dialogs (`.ccraft` filter).
  - Recent files (last 8) and preferences in a small JSON file under the OS config directory: macOS `~/Library/Application Support/CourseCraft/`, Linux `$XDG_CONFIG_HOME` or `~/.config/coursecraft/`, Windows `%APPDATA%\CourseCraft\`. No new crate is needed for paths.
  - Autosave: every 60 s while the revision changed since the last save, to `<name>.autosave.ccraft` next to the file. Untitled projects autosave in the config directory. On open, if an autosave is newer, offer to restore it.
  - The config and autosave directory is a parameter of the code that uses it, so tests write under `target/`, never into the real user config directory.
  - Unsaved changes prompt on quit, new and open.
- **Web** (wasm, trunk): starts with `examples/hello-course` or blank (`?blank`). Open/save are disabled in M3, with a note in File. This build exists so the editor can be driven and verified in a browser.
- **Preview** (desktop and web): an in-app window running `Player` + `SlideView` + `Chrome` on a clone of the current course, starting at the selected slide (`start_slide` override on the clone). It shows an effect log, with a close button.

## Verification

1. The engine rebuild test (step 1) proves the commands can express hello-course.
2. Headless egui tests per editor step drive the UI with synthetic input and assert the resulting command effects on the course, the same way the M1 parity test drives the player. For example: insert a button and drag it, then check the frame changed and one undo restores it; add a trigger through the form; set a question.
3. Manual walkthrough in the browser (wasm editor, built-in browser):
   1. Start blank.
   2. Build the intro slide (title, Start button with entrance at 1 s, About button, About layer with Close trigger) and the tabs slide (three tabs with visited states, counter variable, unlock trigger).
   3. Preview, and check it behaves like hello-course.
   4. Build the quiz slide through the UI too: question form, correct and incorrect layers with their Continue and Try again triggers, and the Submit trigger. Then build the results slide with its `submit_results` trigger.
   5. Preview the whole course and compare it with `simulate`. The final report says plainly which slides were built through the UI and which, if any, were only proven by the rebuild test.

## Steps (one commit each)

1. Engine commands + optional ids + rebuild script/test.
2. `coursecraft-format` + move safe-path.
3. `compose_authoring` in render; `coursecraft-ui-egui` shell: menus, slides panel, canvas display, selection, error surfacing.
4. Canvas move/resize, insert tools, properties panel.
5. Layers, states, timeline.
6. Triggers, variables, question form, course settings, validation list.
7. `apps/coursecraft`: desktop file handling, recent, autosave, preview; wasm build.
8. Browser walkthrough, docs, roadmap.

## Outcome

- Steps 1–8 landed (M3.1–M3.8c). The headless walkthrough (`crates/ui-egui/src/walkthrough_tests.rs`) is the repeatable form of the step-8 walkthrough: it builds all of `hello-course` from blank with UI input only and matches `simulate` line for line.
- Browser walkthrough: slides 1–2 built by hand and previewed (match `simulate`); slide 3's layers and triggers built by hand; question form, course settings and preview checked on the sample course. The by-hand course was lost when the browser pane closed (no save on the web build), so the quiz question and results slide were built through the UI only in the headless test.
- Bugs found by the walkthrough and fixed: no way to pick a condition kind (compare / all / any / not); bottom panel too small with a hard-to-grab resize edge; long combo popups cut off; ids shown instead of names; Preview window growing each frame; Course Settings and the triggers list not syncing on an unedited course; slides panel not scrolling.
