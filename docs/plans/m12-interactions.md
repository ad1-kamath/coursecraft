# M12 Richer interactions

Goal: the interactive building blocks Storyline authors reach for, without writing code. Every runtime change lives in `coursecraft-logic` and is written into `docs/spec/triggers.md` in the same change. Every new object works on the canvas, in preview, in the published player and in offscreen renders, is reachable by keyboard and announced on the web, goes through engine commands with undo, and is driveable from the CLI and MCP.

## Step 1: Web object and Run JavaScript

- `ObjectKind::Web { source: Url { url } | Folder { path, entry } }`. A URL must be `https`. A folder is a directory of HTML, CSS, JS and media copied into the project as `web/<name>/` (size and file-count caps, no paths escaping it) with an entry file such as `index.html`.
- Published web player: an `<iframe>` over the object's frame, positioned like the web video element today (same known limit: it covers slide content above it), sandboxed (`allow-scripts allow-same-origin allow-forms allow-popups`), hidden when the object isn't on screen. Folder content is published under `media/web/<name>/`.
- Editor canvas, desktop player and offscreen: a placeholder card with a globe icon, the URL or folder name and "Plays in the published course". A first-frame preview is a stretch goal; if done, it's a picture the author can set (or capture) and is never fetched automatically.
- `Action::RunJavaScript { code }` (at most 64 KiB). Web player: runs the code with a small `CourseCraft` helper object (`getVar(name)`, `setVar(name, value)`, `jump(slide)`), wrapped so an exception is logged and ignored. Desktop player and preview: does nothing and logs "JavaScript runs in the published course". Variable writes from JavaScript go back through the logic as normal variable changes, so triggers fire. Documented as a power-user feature; the validator warns when a course uses it.
- Trigger form: the Run JavaScript action with a monospace code box.

## Step 2: Input objects

- `TextEntry { variable, placeholder, multiline, max_chars }`: writes its text to a text variable when the learner leaves the field or presses Enter (multiline: when leaving), firing `variable_changes`. Real text input on the web through the access layer's HTML element, egui text input on the desktop.
- `Slider { variable, min, max, step, initial, orientation }` and `Dial { variable, min, max, step, initial, start_angle, end_angle }`: drag or arrow keys change a number variable live.
- `Checkbox { variable, label }` (true/false variable) and `RadioGroup { variable, options }` (text variable holding the chosen option's value), drawn with the theme accent.
- All have states for disabled and focus, are styled by the design tokens on the canvas and theme colours in the player, and appear in the inspector with their variable picker (creating a variable from the picker).

## Step 3: Markers, scrolling panels, lightboxes, tabs and accordions

- `Marker { title, body (rich text), icon, color }`: a pulsing dot; clicking or focusing it opens a small label with the text; another marker closing it is optional.
- `ScrollPanel { children: Vec<Object id>, content_height }`: objects inside it scroll within its frame (mouse wheel, drag, keyboard), clipped to the frame. Children stay normal objects in the slide's list, marked with their panel.
- `Action::LightboxSlide { slide }` and `CloseLightbox`: shows a slide over the current one, dimmed backdrop, close button, the current slide paused. The learner returns to where they were.
- Tabs and accordions as **insertable interactions**, not new runtime: Insert ▸ Interaction ▸ Tabs or Accordion creates buttons, layers and triggers (with states for the selected tab) that the author can edit like anything else.

## Step 4: More triggers, timers and meters

- Events: `KeyPressed { key }` (letters, digits, arrows, Enter, Space, Escape; on the slide or while an object has focus), `DroppedOn { object, target }` (any object can be marked draggable, returns to its place unless dropped on a target that accepts it), `ObjectsIntersect { a, b }` and `ObjectsStopIntersecting`, `TimerEnds { timer }`.
- `Timer { duration_ms, format (mm:ss, seconds, hidden), autostart }` object with `StartTimer`, `PauseTimer`, `ResetTimer` actions and a `Timer.Remaining` built-in-style variable per timer.
- `ProgressMeter { variable, min, max, style: bar | ring }` drawing a number variable.

## Step 5: Interactive video

- Cue points on a video object: `cues: Vec<Cue { at_ms, name, pause: bool }>` and the event `CueReached { object, cue }`. A cue that pauses holds the video until a trigger resumes it (`PlayMedia`).
- The timeline shows cue markers on the video's row; the inspector lists cues with add, rename, move and delete; "Add question here" creates a cue that pauses, a layer with a question, and the triggers to show it and resume after it's answered.
- Works on the web video element and in preview (desktop preview shows the poster with the cue events simulated against the media clock).

## Step 6: Course map

- A Story view: every scene as a column of slide thumbnails, with arrows for every trigger, button and player navigation that jumps between slides (drawn from the course, never stored). Click a slide to open it, drag to reorder within a scene, double-click a scene title to rename. Read-only for branching (edit triggers on the slide).
- Also in the CLI: `course.map` returns the graph as JSON for agents.

## Done bar

Each step: model and validation tests, logic tests with `simulate` scripts for the new runtime, render tests, UI harness tests, `docs/spec/triggers.md` updated, `simulate` on the examples unchanged, `cargo xtask ci` green, played by hand in the web editor and the mock LMS by Claude after review.
