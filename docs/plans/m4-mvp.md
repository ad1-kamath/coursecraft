# M4 plan: authoring essentials and automation (the MVP)

Goal (from `docs/roadmap.md`): M3 + M4 is the MVP. Course authors get the features they use most, built solidly, and agents and scripts can drive the editor through the same engine commands as the UI. Rare features stay under "Not planned for now".

Ground rules carry over from M1–M3:
- The runtime decides; renderers and apps only send inputs and act on effects.
- Every editing feature is an engine command, with a test.
- Runtime changes update `docs/spec/triggers.md` in the same commit.
- Hostile input never panics.

## Shape

```text
coursecraft-course (L1)   model: media, animations, transitions, question kinds, text runs, theme
coursecraft-logic (L2)    built-in variables, question grading, motion-path and media state, effects
coursecraft-render (L3)   media textures, animation/transition sampling, rich text, question widgets, theme
coursecraft-media (L2)    NEW: image decode (png/jpeg/webp/gif first frame) into RGBA, size-capped
coursecraft-pptx (L3)     NEW: .pptx -> Course + media (text boxes, shapes, pictures, slide order)
coursecraft-engine (L5)   commands for everything above
coursecraft-ui-egui (L6)  forms for everything above
coursecraft-mcp (app)     NEW: MCP server (stdio) + control channel client
apps/coursecraft          control channel server inside the running editor
coursecraft-cli           import-pptx; `editor` subcommands that talk to a running editor
```

## Steps (one commit each, CI green after each)

### 1. Images everywhere
- **`coursecraft-media` (L2, standalone, new):** `decode_image(bytes, limits) -> Result<Rgba8Image, MediaError>`, using the `image` crate with png/jpeg/webp/gif features (dependency added by Claude before dispatch).
  - Limits: max 8192×8192 px and 64 MiB decoded.
  - Never panics on corrupt files.
- **Renderer:**
  - A `MediaCache` keyed by `src` that owns `egui::TextureHandle`s.
  - `paint` draws images with aspect-fit inside the frame. While an image loads or after it fails, it falls back to the current placeholder, plus the alt text for failures.
  - The canvas, thumbnails and preview share the cache.
- **Media sources:**
  - player web: fetch `media/<src>` relative to `index.html`
  - player desktop: read next to `course.json`
  - editor: `Project.media` (in memory)
  - All three go through one `MediaSource` trait so the renderer stays I/O-free.
- **Editor:**
  - Insert ▸ Image opens a file picker on desktop and copies the bytes into `Project.media` under `media/<unique-name>`. On the web, use `<input type=file>` through `rfd`'s wasm support, so images work in the browser editor too.
  - Properties: replace image, alt text, "reset to natural size".
- **Tests:** decoder limits and corrupt input; the renderer draws a texture for a known 2×2 PNG; the editor import adds the media entry and the object in one undo step.

### 2. Audio and video
- The logic already emits `media` effects and accepts `Input::MediaEnded`. Spec the exact semantics in triggers.md:
  - autoplay on timeline start
  - play/pause/stop actions
  - `media_ends` event
  - media pauses with the base timeline when a `pause_base_timeline` layer shows
  - media stops on slide exit
- **Web player and editor preview (wasm):**
  - Audio goes through `HtmlAudioElement` (`web-sys`).
  - Video goes through a positioned `<video>` element overlaid on the canvas at the object's frame, kept in sync each frame (position, size, visibility, clipped by layers).
  - The `ended` event is sent back as `Input::MediaEnded`.
- **Desktop:**
  - Audio uses `rodio` (dependency added by Claude).
  - Video on desktop is out of scope for M4. Draw a poster or placeholder with a "Video plays in the published course" note and log one warning. Say so in the roadmap.
- **Tests:** logic tests for every media rule; a fake media backend in the player tests that records play/pause/stop and feeds `MediaEnded`.

### 3. Built-in variables, text references, rich text, theme
Done in two commits:
- **3a:** built-in variables, `%var%` text references, and the bundled Inter font (real bold and italic).
- **3b:** rich text runs, paragraph alignment, and the theme.

- **Built-in read-only variables, computed by logic and never stored in suspend data:**
  - `Course.SlideNumber`, `Course.TotalSlides`, `Course.Progress` (visited / total, 0–100)
  - `Course.ScorePercent`, `Course.ScorePoints`
  - `Slide.ElapsedMs`
  - Usable in conditions and in text.
- **Text references:**
  - `%var%` inside text is replaced at render time by logic's `display_text(object)`, including user variables.
  - The renderer redraws when a referenced variable changes.
- **Rich text:**
  - `Text { runs: Vec<TextRun> }`, where each run carries text, bold, italic, underline, size and color.
  - Paragraph align (left, center, right) and vertical align.
  - Backwards compatible: a plain `text` + `style` still parses and becomes one run.
  - The renderer uses `egui::text::LayoutJob`.
  - Bundle a font family with real bold and italic faces (Inter or similar OFL font, in `crates/render/fonts/` with its licence), so bold renders bold. This also fixes the M3 "bold doesn't look bold" issue.
- **Theme:**
  - `course.theme { accent, background, font, player: { bar_color, text_color } }`.
  - Chrome and default object colours follow the theme; the editor gets a Theme dialog.
- **Tests:** built-ins update on slide entry and scoring; `%var%` substitution, including unknown names and `%` escapes; run layout; theme round-trip.

### 4. Animations and transitions
- **Model:**
  - `object.entrance` and `object.exit`: `{ effect: fade|fly_in(dir)|zoom|wipe(dir)|float, duration_ms, easing }`.
  - `object.motion_paths: [{ id, points: [{x,y}], duration_ms, easing, relative: bool }]`.
  - `slide.transition: { effect: fade|push(dir)|cover(dir)|none, duration_ms }`.
- **Logic:**
  - Entrance and exit are pure functions of the object's timeline time, so the renderer samples them and no runtime state is needed.
  - A new action `move_on_path { object, path }` starts a path. Logic keeps the path's start time and end offset in the player state, so a moved object stays moved and later triggers see its new position.
  - A new event `animation_completes { object, path? }`.
  - Spec everything first in triggers.md.
- **Renderer:** samples transform and opacity per object; slide transitions blend the previous slide's last frame with the new one.
- **Editor:**
  - An Animations section in Properties.
  - Motion path points can be edited on the canvas (drag handles).
  - A transition picker in slide properties.
  - Preview plays them.
- **Tests:** easing curve values; sampling at 0, mid and end; move_on_path persists across layer show and hide; transition timing in parity frames.

### 5. Question types I: true/false, fill-in, sequence, matching
- **Model:** `QuestionKind` gains these kinds:
  - `true_false`: two fixed choices.
  - `fill_in`: accepted answers, case-sensitive flag, trimmed.
  - `sequence`: items in the correct order; shown shuffled, with a per-attempt seeded shuffle that is deterministic for tests.
  - `matching`: pairs, answered with a dropdown per left item.
- **Logic:** grading, attempts and the feedback layers reuse the M1 question flow. New inputs: `TextEntered`, `SequenceMoved`, `MatchSelected`.
- **Renderer:** native widgets drawn inside the slide transform (text box, reorder list with up/down and drag, dropdowns), all sending inputs only.
- **Editor:** a question form per kind.
- **Tests:** per-kind grading tables; renderer → input tests; a hostile-input pass for each new input.

### 6. Question types II: hotspot, drag-and-drop
- **Hotspot:** an image plus one or more correct regions (rect or ellipse); a click places a marker; submit grades.
- **Drag-and-drop:** drag items (objects) onto drop targets (objects), with a correct target per item. Options: snap to center, return if wrong, one item per target.
- Logic owns item positions and target assignment (`DragDropped { item, target? }`); the renderer handles the drag gesture.
- Editor forms; tests as in step 5.

### 7. PPTX import
- **`coursecraft-pptx` (L3, new):** reads the zip and parses `ppt/presentation.xml`, the slide XML and its rels with `quick-xml` (dependency added by Claude).
  - **Imported:**
    - slide order and size (scaled to 1280×720 with letterboxing when the aspect ratio differs)
    - text boxes with runs (bold, italic, size, color, align)
    - basic shapes (rect, roundRect, ellipse) with solid fills
    - pictures, with media copied in
    - slide notes kept in a `notes` field
    - slide titles
  - Everything else is skipped and listed in an import report: charts, SmartArt, groups (flattened when simple), animations, gradients.
  - Hostile-input limits as in `format`.
- CLI `coursecraft-cli import-pptx deck.pptx -o deck.ccraft`; editor File ▸ Import PowerPoint… opens the result as a new untitled project.
- **Tests:** a small fixture `.pptx` generated in the test from XML strings (no binary blobs checked in); malformed zips and XML.

### 8. Automation: MCP server, control channel, editor CLI
- **Control channel:**
  - The running editor (desktop) listens on `127.0.0.1` on a random port, written to `<config>/control.json` with a random token. It speaks JSON-RPC 2.0 over a local TCP connection (std only).
  - Methods:
    - `commands.list`, `command.run`, `command.batch` (atomic, one undo step)
    - `course.get` (JSON, optional JSON pointer), `course.validate`
    - `selection.get` / `selection.set`
    - `render.slide` (PNG of a slide via the same compose/paint offscreen; uses the egui-wgpu offscreen render or a CPU rasteriser fallback in render)
    - `preview.run_inputs` (runs inputs on a player clone and returns effects)
    - `file.save` / `file.open`
  - Every mutation goes through `Editor::run`, so it shows up in the UI and in undo.
  - No remote binding, the token is required, and the server is off when the setting is off.
- **`coursecraft-mcp`:**
  - An MCP server over stdio that exposes these as tools.
  - It works headless on a `.ccraft` or `course.json` file (an in-process engine session), or attached to a running editor through the control channel.
  - Tool names follow the Crafting Apps pattern (`command_list`, `command_run`, `command_batch`, `course_inspect`, `render_preview`, `validate`, `simulate`).
  - It has its own small JSON-RPC implementation, unless an MCP crate is clearly better (Claude decides before dispatch).
- **CLI:** `coursecraft-cli editor <method> [json]` talks to a running editor; existing headless subcommands stay.
- **Tests:**
  - control channel round trip on an ephemeral port in a test
  - token required
  - batch atomicity
  - MCP initialize, `tools/list` and `tools/call` over in-memory pipes
  - building hello-course through MCP `command_batch` using `rebuild.script.json` equals the course

### 9. MVP check
- Publish hello-course plus a media-and-question showcase course. Run both through the mock LMS.
- One real-LMS run on SCORM Cloud. This needs the user's account: ask when we get here, and don't block the other steps on it.
- Update the roadmap and docs, then mark M4 ✅.

## After M4

M5 is a UI/UX milestone (requested by the user): modern, polished, pleasant editor and player.
- design tokens and theme
- typography
- spacing
- iconography
- empty states
- onboarding
- consistent controls
- dark and light themes

It is planned separately once M4 lands.

Polish found during the M4 MVP check (feed into M5):
- The question prompt always draws at a fixed spot at the top of the slide, so an author's own title text there overlaps it. Give the prompt a frame the author can move, or warn on overlap.
- A fill-in field's typed text has no left padding.
- Matching dropdowns wrap long selected text onto two lines instead of truncating it.
- Dropped drag-drop items cover their target's label; consider label placement or snapping that keeps it visible.
- Ellipses look faceted at large sizes (too few segments in the shape path).
