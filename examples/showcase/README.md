# CourseCraft Showcase Course

A feature-complete, polished demonstration course illustrating CourseCraft's native authoring capabilities.

## What It Shows

- **Custom Theme**: Coherent palette (accent `#2563EB`, background `#F8FAFC`, text `#0F172A`) with the bundled Inter font family and slide background overrides (`#EFF6FF`).
- **Media Support**:
  - Image decode and rendering (`banner.png`).
  - Video playback with manual play trigger and `media_ends` event handling (`intro.mp4`), with WebVTT captions (`intro.vtt`).
  - Audio playback on button trigger (`chime.wav`), with WebVTT captions (`chime.vtt`).
- **Interactive Layers & States**:
  - 3-tab navigation with persistent `visited` button states.
  - Variable tracking (`tabs_visited`) displayed with `%tabs_visited%` dynamic text substitution.
  - Motion path animation (`slide_right` polyline motion).
- **Slide Transitions**:
  - Fade transition.
  - Push transition (`from: right`).
- **All 8 Question Kinds**:
  1. Multiple Choice (single selection with feedback layer).
  2. Multiple Response (multi-select grading).
  3. True / False (`true`/`false` choices).
  4. Fill-in (case-insensitive string matching against accepted answers).
  5. Sequence (seeded deterministic shuffle and reorder grading).
  6. Matching (pair associations).
  7. Hotspot (clicking the red hazard zone on `floorplan.png`).
  8. Drag and Drop (placing three crates onto three layer target boxes).
- **Assessment & SCORM Reporting**:
  - 8 graded questions (10 points each, 80 points total).
  - Built-in variable substitution for final score: `%Course.ScorePoints%` and `%Course.ScorePercent%%%`.
  - Conditional results layers (`passed` / `failed`) based on the 80% passing standard.
  - SCORM 1.2 completion and score reporting (`submit_results`).

## Running Headlessly

### Validate

Verify schema, triggers, and asset references with zero errors and zero warnings:

```sh
cargo run -p coursecraft-cli -- validate examples/showcase/course.json
```

### Simulate

Run headless simulation through the pure state machine:

```sh
# Passing scenario (100% score, Passed status):
cargo run -p coursecraft-cli -- simulate examples/showcase/course.json examples/showcase/pass.inputs.json

# Failing scenario (0% score, Failed status):
cargo run -p coursecraft-cli -- simulate examples/showcase/course.json examples/showcase/fail.inputs.json
```

### Publish

Publish to a SCORM 1.2 zip archive and package folder:

```sh
cargo run -p coursecraft-cli -- publish examples/showcase/course.json --player apps/coursecraft-player/dist --out-dir target/mock-lms/showcase/package -o target/showcase.zip
```

### Render

Render any slide offscreen to a PNG image:

```sh
cargo run -p coursecraft-cli -- render examples/showcase/course.json --slide s-welcome -o target/welcome.png
```
