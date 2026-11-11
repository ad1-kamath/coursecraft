# Automation: Control Channel, MCP Server, and Editor CLI

CourseCraft includes a local automation interface that lets agents, scripts, and command-line tools drive the editor or headless courses through the exact same engine commands used by the graphical user interface.

## Quick Start

### 1. Claude Code / MCP Clients

Add CourseCraft to your MCP client configuration (e.g. `claude_desktop_config.json` or `.claude.json`):

**Attached mode** (drives a running CourseCraft editor live):
```json
{
  "mcpServers": {
    "coursecraft": {
      "command": "coursecraft-mcp",
      "args": ["--attach"]
    }
  }
}
```

**Headless mode** (operates directly on a `.ccraft` archive or `course.json` folder):
```json
{
  "mcpServers": {
    "coursecraft": {
      "command": "coursecraft-mcp",
      "args": ["--course", "path/to/course.ccraft"]
    }
  }
}
```

**Blank in-memory course**:
```json
{
  "mcpServers": {
    "coursecraft": {
      "command": "coursecraft-mcp",
      "args": ["--create", "Onboarding Course"]
    }
  }
}
```

### 2. Editor CLI

You can send commands directly from your terminal using `coursecraft-cli editor`:

```sh
# Get editor session info
coursecraft-cli editor session.info

# List available commands
coursecraft-cli editor commands.list

# Inspect course title via JSON pointer
coursecraft-cli editor course.get '{"pointer": "/title"}'

# Render slide preview through running editor
coursecraft-cli editor render.slide '{"slide": "s-intro", "path": "preview.png"}'

# Execute a command
coursecraft-cli editor command.run '{"id": "slide.add", "params": {"title": "Module 2"}}'

# Execute an atomic batch of commands
coursecraft-cli editor command.batch '{"steps": [
  {"id": "slide.add", "params": {"title": "Intro"}},
  {"id": "slide.add", "params": {"title": "Summary"}}
]}'
```

### Slide masters and templates

Masters and templates are ordinary engine commands, so they work over the control channel, MCP and the CLI alike.

```sh
# Create a master, then apply it to a slide
coursecraft-cli editor command.run '{"id": "master.add", "params": {"title": "Brand"}}'
coursecraft-cli editor command.run '{"id": "slide.set_master", "params": {"slides": ["s-intro"], "master": "master1"}}'

# Add a slide from one of the six layouts
coursecraft-cli editor command.run '{"id": "slide.add_from_layout", "params": {"layout": "title_content", "after": "s-intro"}}'

# Save the current slide as a reusable template, then add it back
coursecraft-cli editor command.run '{"id": "template.save", "params": {"slide": "s-intro"}}'
coursecraft-cli editor command.run '{"id": "template.add_slide", "params": {"template": "template1", "after": "s-intro"}}'
```

| Command | Params | Notes |
|---|---|---|
| `master.add` | `{title?}` | Returns `{"id"}`. Default title `Master N`. |
| `master.delete` | `{master}` | Also clears `slide.master` on every slide using it. One undo step. |
| `slide.set_master` | `{slides: id[], master: id\|null}` | Rejects a master that does not exist. |
| `slide.add_from_layout` | `{layout, after?, scene?, master?}` | `layout` is `blank`, `title`, `title_content`, `two_columns`, `picture_text` or `question`. |
| `template.save` | `{slide, title?}` | Deep-clones a play slide into `course.templates`. A master is refused. |
| `template.add_slide` | `{template, after?, scene?}` | Fresh ids and id remapping, keeping `slide.master`. |
| `template.delete` | `{template}` | Removes the template. |

Master titles and backgrounds reuse `slide.update` and `slide.set_background`; duration, notes and transition on a master are stored but ignored. `object.paste` onto a master drops triggers and refuses audio and video.

### 3. Headless Slide Rendering

Render any slide directly to PNG without starting the editor or windowing system:

```sh
# Render intro slide in authoring mode
coursecraft-cli render examples/hello-course/course.json --slide s-intro --width 1280 -o intro.png

# Render with focused layers or player simulation state
coursecraft-cli render course.ccraft --slide s-tabs --mode player --at-ms 1500 --layer modal_hint -o slide.png
```

### 4. Headless Editor Snapshots

Capture high-fidelity pixel snapshots of the full authoring editor UI in Dark or Light mode without opening a window:

```sh
# Capture dark theme snapshot at 2x HiDPI scale
coursecraft-cli snapshot-editor examples/showcase/course.json --theme dark --scale 2 --slide s-media -o target/snapshots/m5s1-dark.png

# Capture light theme snapshot with selected object
coursecraft-cli snapshot-editor examples/showcase/course.json --theme light --scale 2 --slide s-media --object title -o target/snapshots/m5s1-light.png

# Capture the actual player preview panel instead of the canvas
coursecraft-cli snapshot-editor examples/showcase/course.json --preview --slide s-media -o target/snapshots/m5s2-preview.png
```

### 5. Headless Player Snapshots

Capture the learner-facing player chrome (letterboxed slide, control strip with its slide seek bar, outline drawer) at a fixed state and size, without a window or an LMS:

```sh
# Desktop frame with the strip visible
coursecraft-cli snapshot-player examples/showcase/course.json --slide s-media --size 1280x720 --state playing -o target/snapshots/m5s2-desktop.png

# Tablet and phone crops of the same slide
coursecraft-cli snapshot-player examples/showcase/course.json --slide s-media --size 820x1180 --state playing -o target/snapshots/m5s2-tablet.png
coursecraft-cli snapshot-player examples/showcase/course.json --slide s-media --size 390x844 --state playing -o target/snapshots/m5s2-phone.png

# Paused keeps the strip visible, with Play in place of Pause
coursecraft-cli snapshot-player examples/showcase/course.json --slide s-media --size 1280x720 --state paused -o target/snapshots/player-paused.png

# Idle (strip auto-hidden), outline drawer, ended and loading states
coursecraft-cli snapshot-player examples/showcase/course.json --slide s-media --size 1280x720 --state hidden -o target/snapshots/m5s2-hidden.png
coursecraft-cli snapshot-player examples/showcase/course.json --slide s-media --size 1280x720 --state drawer -o target/snapshots/m5s2-drawer.png
coursecraft-cli snapshot-player examples/showcase/course.json --slide s-media --size 1280x720 --state ended -o target/snapshots/m5s2-ended.png
coursecraft-cli snapshot-player examples/showcase/course.json --slide s-media --size 1280x720 --state loading -o target/snapshots/m5s2-loading.png
```

`--state` accepts `playing` (default), `paused`, `hidden`, `drawer`, `ended` and `loading`. Snapshots mark the slides before the target as visited and set the base clock to half the slide duration, so the seek bar shows a realistic mid-slide frame. `--state paused` sets the learner pause flag; `--state ended` paints the end-of-course overlay over the slide.

### 6. Publishing a SCORM, cmi5 or xAPI package

The desktop editor publishes from **File ▸ Publish…** and embeds the release web player built into `target/player-dist/`. Build that folder before compiling a desktop build that should publish:

```sh
cargo xtask build-player
```

A build made without it still runs; its Publish dialog says the player is missing and disables Publish. CI and ordinary `cargo run` do not need trunk.

`publish` builds a package from a course, a built player folder and the course's media. SCORM 1.2 is the default and stays byte-identical to earlier releases.

```sh
# SCORM 1.2 (default)
coursecraft-cli publish examples/showcase/course.json --player apps/coursecraft-player/dist -o target/showcase.zip --out-dir target/showcase-package

# SCORM 2004 4th Edition
coursecraft-cli publish --standard scorm2004 examples/showcase/course.json --player apps/coursecraft-player/dist -o target/showcase-2004.zip --out-dir target/showcase-2004-package

# cmi5 and plain xAPI
coursecraft-cli publish --standard cmi5 examples/showcase/course.json --player apps/coursecraft-player/dist -o target/showcase-cmi5.zip --out-dir target/showcase-cmi5-package
coursecraft-cli publish --standard xapi examples/showcase/course.json --player apps/coursecraft-player/dist -o target/showcase-xapi.zip --out-dir target/showcase-xapi-package
```

`--standard` accepts `scorm12`, `scorm2004`, `cmi5` or `xapi`.

- A SCORM 2004 package adds `coursecraft-package.json` (`{"standard":"scorm2004"}`) at its root and lists it in `imsmanifest.xml`. SCORM 1.2 packages omit the file; the player treats a missing marker as SCORM 1.2, and a bad or unknown marker as SCORM 1.2 with a console warning.
- A cmi5 package adds `coursecraft-package.json` (`{"standard":"cmi5"}`) and `cmi5.xml`; it has no `imsmanifest.xml`.
- An xAPI package adds `coursecraft-package.json` (`{"standard":"xapi"}`) and `tincan.xml`; it has no `imsmanifest.xml`.

`cargo xtask package-example` publishes the showcase as SCORM 1.2 and SCORM 2004, into `target/mock-lms/showcase/` and `target/mock-lms/scorm2004/`, with zips at `target/showcase.zip` and `target/showcase-2004.zip`. It also publishes showcase and hello-course as cmi5 and xAPI into `target/mock-lrs/{cmi5,xapi}/{showcase,hello}/`, with zips at `target/showcase-cmi5.zip` and `target/showcase-xapi.zip`.

### 7. Mock LRS

`cargo xtask mock-lrs [--port 8091]` serves the published xAPI and cmi5 packages, a mock LRS under `/xapi/`, and a test page. It binds to `127.0.0.1` only.

```sh
# Publish the packages first
cargo xtask package-example

# Serve the mock LRS (default port 8091)
cargo xtask mock-lrs
cargo xtask mock-lrs --port 8095
```

Open `http://127.0.0.1:8091/`. The page launches a course in an iframe with a course picker (hello or showcase), a flavor picker (cmi5 or xAPI), a launch mode (Normal, Browse or Review), mastery and return-URL checkboxes, and buttons to relaunch, close the course, reset the learner, or make the next three statement POSTs fail. The side panel polls `/mock/log` once a second and shows each statement as verb and object, with validator problems in red.

The mock LMS keeps one registration and actor per course and flavor until reset, issues a fresh cmi5 fetch path per launch, and records statement-rule failures as `problems` instead of refusing them. Test helpers:

| Endpoint | Purpose |
|---|---|
| `POST /mock/launch?flavor=cmi5\|xapi&course=showcase\|hello&mode=Normal\|Browse\|Review&mastery=0.8&return=1` | Returns `{"url": ...}` for the package launch |
| `GET /mock/log` | Statements, problems and state keys |
| `POST /mock/reset` | Clears registrations, tokens, statements and state |
| `POST /mock/fail?count=3&status=500` | Makes the next `count` statement POSTs fail |

Inspect the latest log from devtools with `window.mockLrs.log()`.

### 8. Accessibility check

`a11y-check` runs the automated accessibility checks in `coursecraft-course` on a `course.json`, a folder containing it, or a `.ccraft` archive:

```sh
# Human-readable report, grouped by slide
coursecraft-cli a11y-check examples/showcase/course.json

# JSON for CI or another tool
coursecraft-cli a11y-check examples/showcase/course.json --json
```

The report lists each finding with its severity (`error`, `warning` or `note`), WCAG criterion, object, message and suggested fix, then a count. Every report ends with **"Automated checks only. These still need a person:"** and the manual checks a tool cannot do. The checks cover names, alt text, contrast against a measurable background, text and target sizes, captions, autoplay controls and duplicate focus order; anything over an image or a partly covering shape is reported as unknown rather than guessed.

Exit codes match `validate`: `1` when there is any `error`, `0` otherwise, and `2` when the course cannot be loaded. Passing the automated checks does not make a course accessible, and CourseCraft never calls a course WCAG compliant.

### 9. Translation (XLIFF 1.2)

```sh
# Export every learner-visible string for a French translation
coursecraft-cli export-xliff examples/showcase/course.json --target-language fr -o target/showcase-fr.xlf

# Apply a translated file to a copy; the input is never modified
coursecraft-cli import-xliff examples/showcase/course.json target/showcase-fr.xlf -o target/showcase-fr.ccraft
```

Both commands accept `course.json` or `.ccraft`. A `.ccraft` input writes `.ccraft` output with its media. `import-xliff` prints the report (applied units, missing targets, changed sources, unknown ids, placeholder mismatches, unsupported scripts) and exits `1` when anything was skipped for a changed source or a placeholder mismatch, `0` otherwise, and `2` on failure. The same work is available as the engine commands `course.export_xliff {target_language}` and `course.import_xliff {xliff}`; the import is one command, so one undo reverts it.

### 10. Commands added for the M7 editor gaps

These commands back the editor controls added in M7 step 2. Each is one undo step.

| Command | Params | What it does |
|---|---|---|
| `question.insert` | `{"slide": id, "kind"?: QuestionKind}` | Inserts a default question for the kind (default `multiple_choice`), two hidden feedback layers with `Correct!` / `Not quite.`, a Submit button at the bottom right inside a 40 px margin, and a `click` trigger on that button that runs `submit_question`. Hotspot adds a placeholder image; drag-and-drop adds a labelled `Drag me` button, a `Drop target` rectangle and a `Drop here` label. Refuses a slide that already has a question. |
| `question.add_submit_button` | `{"slide": id}` | Adds the same Submit button and click trigger to a slide that already has a question, and removes any `submit_question` trigger on the dead `submit` event. |
| `question.add_feedback_layers` | `{"slide": id}` | Adds hidden `Correct` and `Incorrect` layers with `Correct!` / `Not quite.` text below the choices, and points the question's `correct_layer` / `incorrect_layer` at them. |
| `question.delete` | `{"slide": id}` | Removes the slide's question in one undo step. It also removes every `submit_question` trigger and dead `submit` event on the slide and its layers, and any button that one of those triggers fired when no other trigger still names it. Layers, feedback layers and all other objects stay. Refuses a slide with no question. |
| `course.add_finish_button` | `{}` | Adds a Finish button on the last slide in play order, inside the bottom-right 40 px margin and to the left of any button already there. Clicking it runs `submit_results` then `exit_course`, so the score is reported and the end card shows. Works with or without questions. |
| `course.update` | `{"title"?, "id"?, "start_slide"?: id\|null, "language"?: string\|null}` | `language` sets or clears the BCP 47 course language; an invalid tag is refused. |

`question.set` is unchanged and stays available to scripts and tests, including its automatic `submit` trigger. No player control produces `Input::Submit`, so prefer `question.insert` or `question.add_submit_button` in authoring flows.

### 11. Commands added for M8 authoring step 1

These commands back multi-select copy/paste and slide reuse. Each is one undo step.

| Command | Params | What it does |
|---|---|---|
| `object.paste` | `{"slide": id, "layer"?: id, "objects": Object[], "triggers"?: Trigger[], "offset"?: number, "source_slide"?: id}` | Adds the objects with fresh ids, shifts frames by `offset` (default 20 px), and keeps at least one pixel of each on the slide. Triggers have every object reference rewritten to the new ids; layer references are kept only when `source_slide` is the target slide, otherwise those triggers are skipped. Returns `{"ids": [id], "skipped": number}`. |
| `slide.duplicate` | `{"slide": id, "after"?: slide id}` | Copies a slide after itself (or after `after`), with fresh slide, layer and object ids and every internal reference rewritten. The title gets ` copy`. Returns `{"id": id, "skipped": number}`. |
| `slide.paste` | `{"scene"?: id, "after"?: slide id, "slide": Slide}` | Pastes a slide value at the end of a scene or after a slide, with the same fresh-id and reference rewriting as `slide.duplicate`. Jump targets to other slides are kept when the target exists and dropped (and counted) otherwise. Returns `{"id": id, "skipped": number}`. |

The editor's `Edit ▸ Cut`, `Copy`, `Paste` and `Select All`, and the navigator's slide menu, use these commands. The clipboard payload is JSON text `{"coursecraft_clipboard": 1, "kind": "objects" | "slides", ...}`; plain text pasted on the canvas becomes a new text box.

### 12. Commands added for M8 authoring step 2

These commands back align, distribute, match size and grouping. Each is one undo step. Objects with non-finite frames are skipped rather than rejected.

| Command | Params | What it does |
|---|---|---|
| `object.align` | `{"slide": id, "objects": id[], "edge": "left"\|"center"\|"right"\|"top"\|"middle"\|"bottom", "to"?: "selection"\|"slide"}` | Aligns each object's named edge to the bounding box of the given objects (`to` default `selection`), or to the slide when `to` is `slide` or only one object is given. Frames may end up partly off the slide; nothing is clamped. Returns `{"moved": number}`. |
| `object.distribute` | `{"slide": id, "objects": id[], "axis": "horizontal"\|"vertical"}` | Needs at least three objects. Sorts by position (ties keep the order given), leaves the outermost objects in place, and makes the gaps between the rest equal. Returns `{"moved": number}`. |
| `object.match_size` | `{"slide": id, "objects": id[], "dimension": "width"\|"height"\|"both"}` | Gives every object the width and/or height of the first (primary) object. Returns `{"moved": number}`. |
| `object.group` | `{"slide": id, "objects": id[]}` | Needs at least two objects on the same layer. Gives them a fresh group id and makes them contiguous in z-order at the topmost member's position, keeping relative order. An object already in a group brings its whole group in, flattening it. Returns `{"group": id, "objects": number}`. |
| `object.ungroup` | `{"slide": id, "objects": id[]}` | Clears `group` on the given objects and everything sharing their group ids. Returns `{"objects": number}`. |

`object.duplicate` now also accepts `{"objects": id[]}` so a multi-selection duplicates in one command. Copies get fresh object ids and a fresh group id per source group, shared by the copies. `object.paste` does the same for pasted groups. `slide.paste` and `slide.duplicate` keep group ids, because they are scoped per slide. The `group` field is authoring-only and omitted from JSON when unset.

### 13. Commands added for M8 authoring step 4

Find and replace across a course. The strings searched are the ones the XLIFF export covers, in play order and then masters: titles, notes, object text (with its runs), button labels, alt text, descriptions, state text, question prompts, choices, accepted answers and pairs, plus the layers and questions of masters. Templates and the player's own chrome strings are not searched. Matching is case-insensitive unless `case` is true. A match is any non-overlapping occurrence, left to right. `whole_word` skips matches next to a letter, digit or `_`.

| Command | Params | What it does |
|---|---|---|
| `course.find` | `{"query": string, "case"?: bool, "whole_word"?: bool}` | Read-only. Returns `{"hits": Hit[], "truncated": bool}`. A hit is `{slide, object, layer, field, label, snippet, snippet_match, start, end}`, where `slide`, `object` and `layer` are `null` when they do not apply, `label` reads like `Slide 3 ▸ Title`, `snippet` is up to 30 characters either side of the match with `…` where cut, and `snippet_match` is the match's byte range inside `snippet`. Returns at most 1000 hits, with `truncated: true` when there were more. An empty query finds nothing. Makes no undo step. |
| `course.replace` | `{"query": string, "replacement": string, "case"?: bool, "whole_word"?: bool, "only"?: Location[]}` | Replaces every match in one undo step. `only` takes locations (`{slide?, object?, layer?, field}`, as returned in a hit) and limits the replace to those strings. Returns `{"replaced": number, "skipped_variable_refs": number}`. Text with runs keeps its formatting: a match inside one run changes only that run, and a match across runs puts the replacement in the first run it touches and removes the matched text from the others, dropping runs left empty. A match that overlaps a `%name%` reference is left alone and counted in `skipped_variable_refs`; a replacement that adds a `%…%` is not refused, and the validator's warning covers it. Choice and other ids are never changed. A replace that changes nothing makes no undo step and does not bump the revision. A replacement that would make one string longer than 4096 bytes is refused, and the whole replace is rolled back. |

Passing a hit's `slide`, `object`, `layer` and `field` as one `only` entry replaces every match in that string, not only the match the hit points at. The editor's Find and Replace window does the same when you press Replace on a row.

The editor opens the window from `Edit ▸ Find and Replace…` or `Ctrl+F`. It is not a play-time feature, so `simulate` output is unchanged.

### 14. Commands added for M8 authoring step 5

| Command | Params | What it does |
|---|---|---|
| `object.set_crop` | `{"slide": id, "object": id, "crop": {x,y,w,h}\|null, "frame"?: Rect}` | Sets the image's crop to a kept window of its source (`x`, `y`, `w`, `h` are fractions in `0..=1`) and, when `frame` is given, its frame in the same undo step. `null` clears the crop. A full-source window is stored as no crop. Refuses an object that is not an image, a non-finite or out-of-range window, an empty window, and a window that runs past the source. Hotspot regions are in slide coordinates and are not moved. |

The canvas's double-click crop mode and the inspector's **Crop** / **Reset** use this command. Dragging a crop handle or moving the source inside the frame is local editor state until **Enter**, **Apply**, or a click outside writes it once; an unchanged crop writes nothing. Crop mode starts an uncropped image from its aspect-fit rectangle, so opening it moves nothing. The image inspector's **Reset** clears the crop and grows the frame so the whole source shows at the same scale with the kept pixels unmoved, in one undo step.

### 15. Commands added for M9 authoring step 1

Slides from an outline. The Markdown dialect is read by `coursecraft_course::outline`; the command takes the resulting `Outline` as JSON, so a later AI step can produce the same shape without going through Markdown. There is no network or AI in this step.

| Line | Becomes |
|---|---|
| `# Text` | a scene. The first one also names the document. |
| `## Text` | a slide. |
| `- Text`, `* Text`, or a plain line | a body line. |
| `> Text` | speaker notes, joined with a newline. |
| `? Text` | the slide's question prompt. |
| `[x] Text` | a correct choice. |
| `[ ] Text` | a wrong choice. |

Slides before the first `#` go into a scene named `Scene 1`. A question needs at least two choices and exactly one correct. Limits are 500 slides, 50 body lines a slide, 20 choices and 4000 bytes a line; breaking one is a problem naming its line, never a truncation. CRLF and non-ASCII text are fine.

```sh
# Build an outline first, or write the Outline JSON directly
coursecraft-cli editor command.run '{"id": "course.import_outline", "params": {
  "outline": {"scenes": [{"title": "Safety", "slides": [
    {"title": "Welcome", "body": ["Keep exits clear", "Know the alarm"]},
    {"title": "Quiz", "question": {"prompt": "What do you do?",
      "choices": [{"text": "Leave by the exit", "correct": true},
                  {"text": "Wait", "correct": false}]}}
  ]}]},
  "after": "s-intro"
}}'
```

| Command | Params | What it does |
|---|---|---|
| `course.import_outline` | `{"outline": Outline, "after"?: slide id}` | Appends the outline's scenes and slides as one undo step and returns `{"scenes", "slides", "first_slide"}`. With `after`, the new scenes go after that slide's scene; without it they go at the end. A slide with body lines uses the `title_content` layout and joins them with newlines, prefixing each with `• ` when there are two or more; a slide with none uses `title`. A slide with a question uses the `question` layout, replaces the default prompt and choices, and keeps the Submit button and Correct/Incorrect feedback layers that `question.insert` adds; the one correct choice stays correct, and **the slide's body is ignored**. New slides take the master of the slide they follow. It refuses an outline that fails `Outline::check` with the problems in the error message and changes nothing. |

The editor opens the window from `Insert ▸ Slides from Outline…`. It shows a live summary and the problems with their lines; `Add slides` is disabled while there are problems or nothing to add. Escape or Cancel closes it without changing the course. The window only takes pasted text, so it works the same in the browser and on the desktop.

### 16. AI settings and a mock endpoint

AI is optional and off by default. Open **Project ▸ AI Settings…**, then use the **Use:** row on the **AI features** tab. **An API endpoint** shows the original fields: pick a preset (Ollama, LM Studio, DeepSeek or OpenAI) or type any OpenAI-compatible base URL, a model and an optional key, then press **Test connection**. A plain `http://` URL is only accepted for `localhost`, `127.0.0.1` or `[::1]`, so a key never travels in clear text to another machine. The preset fills in the base URL only; CourseCraft does not guess whether to add `/v1`.

On the desktop the other choices are **Claude Code (your Claude plan)** and **Codex (your ChatGPT plan)**. Each one needs that agent found by the **Agents** tab; the tab shows **Installed, not connected**, **Connected**, or **Not installed** with an install link. **Test connection** still sends `Reply with OK`, but through the agent instead of HTTP. The web editor offers only **An API endpoint**.

When an agent is chosen, the request's messages become one prompt: the system text, the user text, and `Reply with only the answer, no commentary.` Images from alt text are written to a fresh temporary folder beside the prompt and named in it. Claude Code runs `claude -p --output-format json --tools Read` with the prompt on standard input; Codex runs `codex exec --skip-git-repo-check --sandbox read-only --output-last-message <tmp>/answer.txt -`, adding `--image <file>` for each image. Both use the temporary folder as the working directory. No course file, project folder or API key is given to the agent, and the folder is removed afterwards. An agent request gets ten minutes; **Cancel** kills the process. A non-zero exit says either “Claude Code isn't signed in. Run `claude` once in a terminal to sign in.” (or the Codex equivalent) when the output mentions login or authentication, otherwise that the agent stopped with an error and at most 200 characters of its output. The answer comes back as an ordinary chat reply, so document to course, alt text and translation work unchanged.

On the desktop the settings live in `prefs.json` as an optional `ai` object (`source`, `base_url`, `model`, `api_key`, and the optional `speech_to_text_model`, `text_to_speech_model` and `voice`), omitted when unset, and the file is made owner-only (`0o600` on Unix) after a key is written. `source` is `endpoint` when absent, so old files load as before; the agent choices are `claude-code` and `codex`. In the web editor the key is kept for the session only and never stored. The key never enters `course.json`, a `.ccraft`, a published package, an XLIFF, a log or an error message.

`cargo xtask mock-ai [--port 8099]` is a local OpenAI-compatible endpoint for trying the settings without an account. It binds to `127.0.0.1` only and answers `POST /v1/chat/completions` with an OpenAI-shaped reply and permissive CORS (including `OPTIONS`), so the web editor can reach it. It logs each request's first line and body size, never headers.

```sh
cargo xtask mock-ai
# Base URL: http://localhost:8099/v1
# Model:    anything
# API key:  leave empty
```

The reply is `OK` unless the last user message contains `MOCK_FILE=<name>`; then it returns the contents of `xtask/mock-ai/<name>`. Only simple file names inside that folder are accepted, so `MOCK_FILE=hello.txt` returns the sample reply and a path is ignored.

### 17. Course from a document

**Insert ▸ Course from Document…** turns pasted text, or a chosen `.docx`, `.txt` or `.md` file, into an outline with the configured AI endpoint. If AI is not set up, the window says so and offers a button to **AI Settings**; nothing is sent. On the desktop the file picker is native; in the browser it uses the same file-request path as the rest of the web editor.

The window shows what will be sent: a large editable text field, the character count, and a consent line naming the endpoint's host. For a local server it adds “on this computer”. With an agent source the line names it instead, for example “This sends the text above to Claude Code on this computer, which sends it to Anthropic under your plan.” A document over 60,000 characters is cut on a character boundary before sending, and the window says `Only the first 60,000 characters will be sent.` The optional slide target is 1 to 100; quiz questions are included by default.

`crates/pptx` reads a `.docx`'s `word/document.xml` with `docx_to_text`: headings become `#`/`##`, list paragraphs become `- `, table cells become lines, and images are ignored. The archive entry count, decompressed size (32 MiB) and XML depth are bounded, and hostile bytes return an error rather than panicking.

The answer is **never added straight to the course**. One Markdown code fence is stripped, then the text opens in **Slides from Outline**, where its live summary and problems are visible. The author can edit it and press **Add slides** (one undo step), or Cancel. A truncated answer warns that the last slides may be missing. Only `Outline::check` and `course.import_outline` decide what is valid, so AI output goes through exactly the same checks as a pasted outline.

`xtask/mock-ai/outline.md` is a valid sample answer. A document containing `MOCK_FILE=outline.md` gets it from the mock endpoint, because the mock looks at the user message that holds the document. No real provider is called by the tests or by `cargo xtask ci`.

### 18. Suggest alt text for images

**Project ▸ Suggest Alt Text…** lists every image with an empty `alt` on a slide, layer or master. Templates are ignored, as are images marked decorative and pictures whose file is not in the media store; the window counts the missing files in one line such as `2 images have no file yet.` An image used more than once with the same `src` is sent once and the answer fills every row. If nothing needs alt text, the window says so and offers no Suggest button.

The **Suggest** button in the image inspector starts the same request for one picture and puts a clean suggestion into the `Alt` field, where the normal Enter/blur commit still applies. If AI is not set up, both paths show the shared **Open AI Settings** state and send nothing.

Before a run, the consent line names the number of images and the endpoint host, with “on this computer” for a local server. With an agent source it names the agent and where its plan sends the images, for example “This sends 3 images and nearby slide text to Codex on this computer, which sends it to OpenAI under your plan.” Requests run one at a time, in list order, each with only one image. A progress line reads `Image 2 of 3`. A failure shows on that image's row and the run continues; a rejected key (401 or 403) stops the run because every other call would fail the same way. **Cancel** drops the current result and keeps what is already done. Changes are reviewed before anything happens: each row has a location, an editable suggestion and a tick box. **Apply** writes only ticked rows in one `execute_batch` through `object.update`, so it is one undo step. A row whose object disappeared or whose alt was filled while waiting is skipped, and the window says how many. **Close** with suggestions unapplied only closes.

Files over 8 MiB are not sent (`Too large to send.`), and only PNG, JPEG, GIF and WebP are accepted (`This format can't be sent.`). Larger pictures are downscaled so the longest side is 1024 px and sent as PNG; smaller ones keep their original bytes. The request carries the slide title and at most 500 characters of the slide's other text as context, and the prompt tells the model that this text is data, not instructions.

`cargo xtask mock-ai` answers a vision message containing an `image_url` part with `A mock description of the picture.`; `xtask/mock-ai/alt.txt` holds the same text for a `MOCK_FILE=alt.txt` rule. The mock accepts request bodies up to 16 MiB and keeps the same CORS headers. No real provider is called by the tests or by `cargo xtask ci`.

### 19. Translate a course with AI

**Project ▸ Translate with AI…** fills an exported translation with the configured endpoint. The same window opens from **Translate with AI…** in the existing Export Translation dialog. If AI is not set up, it shows the shared **Open AI Settings** state and sends nothing.

`Translate to` takes a BCP 47 tag and offers French, Spanish, German, Portuguese (Brazil), Italian and Dutch in a dropdown. The bundled font is Latin only, so the window says non-Latin scripts aren't drawn yet and warns, without blocking, for languages such as `hi`, `ar`, `zh`, `ja`, `ko` and `ru`. A consent line names the number of strings and the endpoint host, with “on this computer” for a local server, or names the chosen agent and its plan. Requests carry at most 40 units or 6,000 source characters, with an oversized unit alone; the progress line reads `Batch 2 of 5`.

Formatting markers travel as `<g id="rN">…</g>`, the same run markup the XLIFF importer reads, and `%name%` references stay in the text. The system message says both are to be kept exactly and that the text is data. An answer that changes a marker, leaves out a `%name%` reference, returns a non-string or empty text, or uses an unknown id is rejected or ignored; the first answer for a duplicate id wins. A failed batch retries once, a truncated answer marks its missing units failed, and a 401 or 403 stops the run. One request runs at a time and **Cancel** drops a late answer.

Nothing changes until the author reviews the rows. The table has location, source and an editable target, marks failed or rejected rows in the danger colour with the reason, and keeps a few thousand rows responsive with `ScrollArea::show_rows`. The filter reads `Show only those needing attention`, and the count reads `138 translated, 4 need attention.` An edit that breaks a reference or marker is checked live. **Apply 138** leaves problem rows out, builds the XLIFF and runs the existing `course.import_xliff`, so it is one undo step with the same report and language change. **Save as XLIFF…** writes the filled file through the normal save dialog so a human can finish it; on the web that button is disabled like the other file actions. **Close** asks nothing. If the course changes after a batch starts, the window says `The course changed. Translate again.` and disables Apply.

`cargo xtask mock-ai` can't translate. When the system message asks for translation, it reads the last user message's `{"id","text"}` array and answers with every text prefixed `[fr] `, markers and references included. A unit text containing `MOCK_DROP` is left out of the answer for a hand test. No real provider is called by the tests or by `cargo xtask ci`.

### 20. Captions from audio, and text to speech

Both features are optional and use the same endpoint and one-call-at-a-time rule as the other AI features. In **Project ▸ AI Settings…**, the **Audio** section adds **Speech-to-text model** (hint `whisper-1`), **Text-to-speech model** (hint `tts-1`) and **Voice** (hint `alloy`). They are stored with the other AI preferences and omitted when empty. Leaving a model empty turns only that feature off: its window says `Speech-to-text model isn't set.` or `Text-to-speech model isn't set.` and offers **Open AI Settings**. Nothing is sent.

Both features need audio models, so an agent source can't make them. With **Claude Code** or **Codex** chosen, the captions window says `Captions need an API endpoint or ElevenLabs`, the narration window says `Narration needs an API endpoint or ElevenLabs`, and both offer **Open AI Settings** on the **AI features** tab. **ElevenLabs** narration still works whatever source is chosen.

**Captions from audio.** An audio or video object's **Captions** row has **Generate…** when its file is in the media store and no larger than 25 MB. The file is sent as it is, so a server may accept a video or reject its format, and that failure reads as the error it is. The window's consent line names the file, its size and the endpoint host, with “on this computer” for a local server, for example `This sends sound.mp3 (3.2 MB) to api.example.com.` The answer must parse as WebVTT with at least one cue; a JSON `{"text": …}` answer or plain text reads `The server didn't return timed captions.` and changes nothing.

The window counts the cues and shows the first three in a read-only preview. **Use these captions** saves the VTT in the media library and sets the object's `captions` in one `object.set_captions` command, so one undo restores the previous value. The file follows the existing captions-import naming rule (`sound.vtt`, then `sound-2.vtt`, never overwriting another file). When captions are already set the button says **Replace captions**. **Close** throws the answer away. **Cancel** drops a late answer.

**Narration from text.** **Insert ▸ Narration from Text…** opens the window, and **Narrate…** beside a slide's Notes field opens it filled with those notes. A **Voice source** control at the top chooses **AI provider** (the endpoint from AI Settings, exactly as before) or **ElevenLabs**. The choice is remembered. With **ElevenLabs** selected none of the AI provider checks apply: the window never says AI is not set up. The field counts characters against the 4,000 limit for the AI provider, or the selected ElevenLabs model's limit (10,000 when the model does not say). A consent line names the endpoint host, or says `This sends N characters to ElevenLabs, which uses your account's credits.` Text containing a `%name%` reference warns that the reference will be read aloud literally, because speech is made once. The window also says `Tell your learners this voice is AI-generated, if your provider's terms require it.`

**ElevenLabs narration** needs the author's own ElevenLabs API key and account credits; CourseCraft has no key of its own. Open **Get a key** in the window to reach ElevenLabs' key page. The key is stored in `prefs.json` under a separate optional `elevenlabs` object (never in the course), with the voice id, voice name and model id, and the file is made owner-only on Unix. Saving the key loads models and then voices, one request at a time. Models come from `GET /v1/models` and only text-to-speech models are listed; the model's `maximum_text_length_per_request` sets the character limit, and the default `eleven_multilingual_v2` still works if the list fails. Voices come from `GET /v2/voices?page_size=100`, following at most ten pages; they are shown as `name · category`, built-in voices first then alphabetically, with a filter once there are more than twelve. **Refresh** reloads them. **Or paste a voice ID** accepts an id from the author's library and looks it up; a failed lookup only warns and the id is still used, because a restricted key can speak with a voice it cannot read. A voice id may only contain ASCII letters, digits, `_` and `-`.

Speech is `POST /v1/text-to-speech/{voice_id}?output_format=mp3_44100_128` with the key in `xi-api-key`. Characters are counted, not bytes, and text over the limit or an empty key, voice or text is refused before a call. Errors read as plain sentences: `Your ElevenLabs account is out of credits.`, `ElevenLabs rejected this API key.`, `This API key isn't allowed to do that. In ElevenLabs, give the key access to text to speech and voices, or paste a voice ID.`, `ElevenLabs can't find that voice.`, `That's too much text for this model.` or `ElevenLabs is busy or you've hit a limit. Try again shortly.` The key is never printed, logged or put in a course. Speech may take longer than other calls, so it gets a 300-second timeout while everything else keeps 120 seconds.

Browsers may block ElevenLabs' cross-origin requests, so the desktop app is the sure way to use it. No test or `cargo xtask ci` run calls the real service.

The success state offers **Add to slide**, which stores the MP3 in the media library and adds an autoplaying audio object in the bottom-right corner through the ordinary `object.add` command: one undo step. The course validates and publish's missing-media preflight finds the new file, so publishing is not blocked. There is deliberately no separate captions request: using captions is a second, author-triggered action. **Close** and **Cancel** change nothing.

A 404 is normal for these endpoints: `This server doesn't offer speech-to-text. Local servers often don't: try a hosted one.` or the equivalent for text-to-speech.

`cargo xtask mock-ai` now also answers `POST /v1/audio/transcriptions` with a small valid WebVTT file (two cues) and `POST /v1/audio/speech` with a tiny valid MP3 whose ID3 tag and frame headers are generated in code. Both keep permissive CORS and `OPTIONS`. A request whose `model` is `no-audio` answers 404 on both routes, for the error path. The mock logs only the request line and body size. The audio files in tests are not real speech, and no real provider is called by the tests or by `cargo xtask ci`.

---

## Editor Automation Toggle

In the desktop editor:
- Automation is enabled by default.
- You can toggle automation via **Help ▸ Allow automation** (checkbox).
- When active, the status bar displays a small green `"Automation on"` chip with the bound listening port on hover.
- Disabling the toggle immediately shuts down the TCP server and removes the discovery file.
- The preference is persisted in `prefs.json` as `"automation_enabled": true|false`.

---

## Discovery File

When the control server starts, it writes connection details atomically to `<config_dir>/control.json`:

| Platform | Default Path |
|---|---|
| macOS | `~/Library/Application Support/CourseCraft/control.json` |
| Windows | `%APPDATA%\CourseCraft\control.json` |
| Linux | `${XDG_CONFIG_HOME:-~/.config}/coursecraft/control.json` |

### File Format

```json
{
  "port": 54321,
  "token": "b78a9c3d4e5f60718293a4b5c6d7e8f90123456789abcdef0123456789abcdef",
  "pid": 48210,
  "version": "0.1.0"
}
```

- **`port`**: Ephemeral port assigned by the OS on `127.0.0.1`.
- **`token`**: 32-byte cryptographic random hex token required on all incoming requests.
- **`pid`**: Process ID of the editor process.
- **`version`**: Application version.

The file is written with strict file permissions (`0o600` on Unix) and removed on shutdown when the PID matches.

---

## Control Channel Protocol

The control channel uses **JSON-RPC 2.0** over a TCP connection to `127.0.0.1`.
- Encoding: UTF-8, newline-delimited (`\n`), one JSON object per line.
- Maximum line size: 16 MiB (lines exceeding 16 MiB result in immediate connection closure).
- Authentication: Every request must carry `"token": "<token>"` inside `"params"`.

### Methods

#### `commands.list`
Returns a list of all registered engine commands:
```json
{ "jsonrpc": "2.0", "id": 1, "method": "commands.list", "params": { "token": "..." } }
```
**Result**: `[{ "id": "slide.add", "label": "New Slide", "menu": "Insert/New Slide", "params": "{...}", "mutates": true }]`

#### `command.run`
Executes an engine command:
```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "command.run",
  "params": {
    "token": "...",
    "id": "slide.add",
    "params": { "title": "Safety Guidelines" }
  }
}
```
**Result**: Command result JSON (e.g. `{"id": "slide2"}`).

#### `command.batch`
Executes an array of commands atomically in a single undo step:
```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "method": "command.batch",
  "params": {
    "token": "...",
    "steps": [
      { "id": "slide.add", "params": { "title": "Step 1" } },
      { "id": "slide.add", "params": { "title": "Step 2" } }
    ]
  }
}
```
**Result**: `[{"id": "slide2"}, {"id": "slide3"}]`

#### `course.get`
Returns the course document model or a value at an RFC 6901 JSON pointer:
```json
{
  "jsonrpc": "2.0",
  "id": 4,
  "method": "course.get",
  "params": { "token": "...", "pointer": "/scenes/0/slides/0/title" }
}
```
**Result**: `"Safety Guidelines"`

#### `course.validate`
Validates the course and returns any errors or warnings:
```json
{ "jsonrpc": "2.0", "id": 5, "method": "course.validate", "params": { "token": "..." } }
```
**Result**: `[{ "severity": "error"|"warning", "path": "/...", "message": "..." }]`

#### `selection.get` & `selection.set`
Inspects or updates the current slide, layer, and object selection:
```json
{
  "jsonrpc": "2.0",
  "id": 6,
  "method": "selection.set",
  "params": { "token": "...", "slide": "slide1", "layer": null, "object": "btn_next" }
}
```
**Result**: `{ "slide": "slide1", "layer": null, "object": "btn_next" }`

#### `preview.run_inputs`
Simulates player inputs on a fresh player instance and returns emitted effects:
```json
{
  "jsonrpc": "2.0",
  "id": 7,
  "method": "preview.run_inputs",
  "params": {
    "token": "...",
    "inputs": [{ "input": "start" }, { "input": "click", "object": "btn_next" }]
  }
}
```
**Result**: `{ "effects": [...] }`

Player transport inputs use the same names as the runtime: `{"input": "player_pause"}`, `{"input": "player_resume"}` and `{"input": "player_seek", "ms": 2500}`. They move only the current slide's learner clock, exactly as the strip's controls do.

#### `file.save` & `file.open`
Saves or loads course documents:
```json
{
  "jsonrpc": "2.0",
  "id": 8,
  "method": "file.save",
  "params": { "token": "...", "path": "path/to/project.ccraft" }
}
```
```json
{
  "jsonrpc": "2.0",
  "id": 9,
  "method": "file.open",
  "params": { "token": "...", "path": "path/to/project.ccraft", "force": false }
}
```

#### `session.info`
Returns current session status:
```json
{ "jsonrpc": "2.0", "id": 10, "method": "session.info", "params": { "token": "..." } }
```
**Result**: `{ "title": "Safety", "path": "...", "dirty": false, "revision": 3, "can_undo": true, "can_redo": false }`

#### `edit.undo` & `edit.redo`
Reverts or re-applies changes:
```json
{ "jsonrpc": "2.0", "id": 11, "method": "edit.undo", "params": { "token": "..." } }
```
**Result**: `{ "revision": 2 }`

#### `render.slide`
Renders a slide to PNG without opening a window:
```json
{
  "jsonrpc": "2.0",
  "id": 12,
  "method": "render.slide",
  "params": {
    "token": "...",
    "slide": "s-intro",
    "width": 1280,
    "mode": "authoring",
    "layers": [],
    "at_ms": null
  }
}
```
**Result**: `{ "width": 1280, "height": 720, "png_base64": "iVBORw0KGgo..." }`

When `path` is specified in `params`, the PNG bytes are written to disk directly:
```json
{
  "jsonrpc": "2.0",
  "id": 13,
  "method": "render.slide",
  "params": {
    "token": "...",
    "slide": "s-intro",
    "width": 1280,
    "path": "target/preview.png"
  }
}
```
**Result**: `{ "width": 1280, "height": 720, "path": "target/preview.png" }`

- **`slide`**: Slide identifier (defaults to current selection if omitted).
- **`width`**: Target width in pixels (clamped to 16..=4096; requests > 4096 are refused). Height is computed automatically from the slide aspect ratio.
- **`mode`**: `"authoring"` (default) for static editor composition, or `"player"` for runtime simulation state.
- **`layers`**: Array of layer IDs to compose on top.
- **`at_ms`**: In player mode, timestamp in milliseconds to tick to (default: slide duration).

### Error Codes

| Code | Meaning |
|---|---|
| `-32700` | Parse error (malformed JSON or line exceeding 16 MiB) |
| `-32600` | Invalid request |
| `-32601` | Method not found |
| `-32602` | Invalid params |
| `-32000` | Engine execution error |
| `-32001` | Unauthorized (missing or invalid token) |
| `-32002` | Unsaved changes (attempted `file.open` while dirty without `force: true`) |
| `-32004` | Not found (referenced JSON pointer, slide, layer, or object does not exist) |

---

## MCP Server Tools Reference

The `coursecraft-mcp` binary provides standard Model Context Protocol (MCP) tools:

| Tool | Parameters | Description |
|---|---|---|
| `command_list` | `{}` | List all available engine commands |
| `command_run` | `{id: string, params?: object}` | Run a command by id |
| `command_batch` | `{steps: [{id: string, params?: object}]}` | Execute atomic command batch with single undo step |
| `course_inspect` | `{pointer?: string}` | Inspect course model or JSON pointer value |
| `validate` | `{}` | Check course for structural/trigger errors |
| `simulate` | `{inputs: array, start_slide?: string}` | Simulate player inputs and return effects |
| `selection_get` | `{}` | Get current selection |
| `selection_set` | `{slide?: string, layer?: string, object?: string}` | Update active selection |
| `undo` | `{}` | Undo last change |
| `redo` | `{}` | Redo last undone change |
| `session_info` | `{}` | Get session metadata and dirty status |
| `file_save` | `{path?: string}` | Save course to `.ccraft` or folder |
| `file_open` | `{path: string, force?: boolean}` | Open course from disk |
| `render_preview` | `{slide?: string, width?: integer, mode?: string, layers?: string[], at_ms?: integer}` | Render slide to PNG and return image preview block and metadata |

### MCP `render_preview` Example

When an MCP client invokes `render_preview`:
```json
{
  "name": "render_preview",
  "arguments": {
    "slide": "s-intro",
    "width": 960,
    "mode": "authoring"
  }
}
```

The response includes an image block and a summary text block:
```json
{
  "content": [
    {
      "type": "image",
      "data": "iVBORw0KGgoAAAANSUhEUgAAA8AAA...",
      "mimeType": "image/png"
    },
    {
      "type": "text",
      "text": "Welcome — 960×540, authoring"
    }
  ]
}
```

---

## Security Model

1. **Loopback only**: The server binds exclusively to `127.0.0.1:0`. It will never accept connections from external networks.
2. **Ephemeral ports**: Random port selection prevents port conflict issues.
3. **Token authentication**: A 32-byte cryptographically random hex token generated from `/dev/urandom` is required on every request.
4. **File permissions**: `control.json` is created with mode `0o600` on Unix systems, accessible only by the current OS user.
5. **Connection limits**: At most 8 concurrent TCP connections are accepted.
6. **Idle timeouts**: Connections idle for more than 10 minutes are closed automatically.
7. **User control**: The server can be enabled or disabled at any time from the editor UI (**Help ▸ Allow automation**).
