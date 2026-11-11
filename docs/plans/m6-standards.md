# M6 Standards & reach

Goal: a CourseCraft course works in any modern LMS or LRS, can be used by keyboard and screen reader, and can be translated. Eight working steps and a final check. Step 1 is specified here; later steps are outlined and get their detail in their own brief when they start.

| Step | What | Depends on |
|---|---|---|
| 1 | SCORM 2004 4th edition: runtime and packaging | |
| 2 | Question-level reporting (`cmi.interactions`) in both SCORM versions | 1 |
| 3 | xAPI and cmi5: LRS client, statements, packaging, mock LRS | 2 |
| 4 | Player keyboard access and author-set focus order | |
| 5 | Web screen-reader layer (hidden ARIA DOM over the canvas) | 4 |
| 6 | Captions (WebVTT) | |
| 7 | Accessibility checker (WCAG 2.1 AA checks an author can act on) | 4, 6 |
| 8 | Localisation: XLIFF export and import | |
| 9 | M6 check: mock LMS and LRS by hand, roadmap, docs | all |

## Decisions that hold across the milestone

- **One player binary.** The same wasm player runs every standard. A package states its standard in `coursecraft-package.json` at its root, for example `{"standard":"scorm2004"}`. A missing file means SCORM 1.2, so 1.2 packages stay byte-identical to today's. The package states it, instead of the player sniffing for APIs, because some LMSs expose both `API` and `API_1484_11` and the one matching the manifest must win.
- **The rules live in `coursecraft-lms`** (L4), testable with fakes and no browser. `apps/coursecraft-player` stays a thin shim over JavaScript. No play-time behaviour outside `coursecraft-logic` (golden rule 3): the bridges only translate effects.
- **SCORM 1.2 stays the default.** Existing `build_package`, `LmsBridge::new()` and their tests keep working unchanged. New standards use new entry points.
- **Spec first.** Each standard's reporting mapping goes in `docs/spec/triggers.md` in the same step.
- **No sequencing.** SCORM 2004 packages are single-SCO with no sequencing rules. Multi-SCO packages are out of scope.
- **Real-LMS runs remain the user's check.** Every step is tested against the mock LMS. `target/showcase-2004.zip` joins the two SCORM 1.2 zips for the user's SCORM Cloud run.

## Step 1: SCORM 2004 4th edition

### Data model mapping

| Meaning | SCORM 1.2 (today) | SCORM 2004 |
|---|---|---|
| API object | `API` | `API_1484_11` |
| Calls | `LMSInitialize` `LMSGetValue` `LMSSetValue` `LMSCommit` `LMSFinish` `LMSGetLastError` `LMSGetErrorString` | `Initialize` `GetValue` `SetValue` `Commit` `Terminate` `GetLastError` `GetErrorString` |
| Status read at start | `cmi.core.lesson_status` | `cmi.completion_status`, `cmi.success_status` |
| First launch | `not attempted` → write `incomplete` | `completion_status` of `unknown` or `not attempted` → write `incomplete` |
| Already finished | status is `passed`, `failed` or `completed` | `completion_status` is `completed`, or `success_status` is `passed` or `failed` |
| Score | `cmi.core.score.min`, `.max`, `.raw` | `cmi.score.min`, `.max`, `.raw`, plus `cmi.score.scaled` = percent / 100 (`0.85`, `1`, `0`) |
| Passed | `lesson_status` = `passed` | `success_status` = `passed`, `completion_status` = `completed` |
| Failed | `lesson_status` = `failed` | `success_status` = `failed`, `completion_status` = `completed` |
| Completed | `lesson_status` = `completed` | `completion_status` = `completed`; `success_status` untouched |
| Incomplete | `lesson_status` = `incomplete` | `completion_status` = `incomplete`; `success_status` untouched |
| Location | `cmi.core.lesson_location` (255) | `cmi.location` (1000) |
| Suspend data | `cmi.suspend_data` (4096) | `cmi.suspend_data` (64000) |
| Exit, not finished | `cmi.core.exit` = `suspend` | `cmi.exit` = `suspend` |
| Exit, finished | `cmi.core.exit` = `` | `cmi.exit` = `normal` |
| Session time | `cmi.core.session_time` = `HHHH:MM:SS.SS` | `cmi.session_time` = ISO 8601, `PT1H2M3.45S` |

Effects are processed exactly as for SCORM 1.2, in the same order, with the same "suspend once, commit at most once" rule. Score is written before status. Writes the LMS refuses become warnings, as today.

### Manifest (`coursecraft-scorm`)

`manifest_scorm2004(&ManifestSpec)`: same validation as `manifest_scorm12`, with:

- `<schema>ADL SCORM</schema>`, `<schemaversion>2004 4th Edition</schemaversion>`;
- default namespace `http://www.imsglobal.org/xsd/imscp_v1p1`, `xmlns:adlcp="http://www.adlnet.org/xsd/adlcp_v1p3"`, `xmlns:adlseq="http://www.adlnet.org/xsd/adlseq_v1p3"`, `xmlns:adlnav="http://www.adlnet.org/xsd/adlnav_v1p3"`, `xmlns:imsss="http://www.imsglobal.org/xsd/imsss"`;
- `adlcp:scormType="sco"` on the resource (capital T in 2004);
- no `masteryscore` and no `<imsss:sequencing>`. CourseCraft decides pass or fail and reports `success_status` itself. An LMS-side passing score could override it.

A `Standard` enum (`Scorm12`, `Scorm2004`) lives in `coursecraft-scorm`, with `as_id()` returning `scorm12` or `scorm2004`.

### Publishing (`coursecraft-publish`, CLI)

- New `build_package_for(input, standard)`. `build_package` keeps its signature and calls it with `Scorm12`.
- A SCORM 2004 package adds `coursecraft-package.json`, listed in the manifest. SCORM 1.2 adds nothing, and a test proves the 1.2 output is unchanged.
- `coursecraft-cli publish --standard scorm12|scorm2004`, default `scorm12`.

### Bridge (`coursecraft-lms`)

- `LmsBridge::for_standard(Standard)`. `new()` means SCORM 1.2 and every existing test passes unmodified.
- All element names, vocabularies and limits come from one per-standard mapping, so `bridge.rs` has no scattered `if standard` branches.
- `format_session_time_iso(ms)`, with hostile values (`u64::MAX`) handled without overflow or panic.
- `CmiApi` stays as it is. Its methods already mean the same thing under both standards.

### Player (`apps/coursecraft-player`)

- The loader reads `coursecraft-package.json`. A missing or unreadable file means SCORM 1.2; an unknown standard logs a warning and falls back to SCORM 1.2.
- SCORM 2004 searches for `API_1484_11` on `window`, its parents and `window.opener`. SCORM 1.2 keeps searching for `API`. `JsCmiApi` takes the call names from the standard. The cross-realm rules at the top of `lms.rs` still apply.

### Mock LMS and package-example (`xtask`)

- `xtask/mock-lms/index.html` gains a SCORM 2004 runtime, chosen by `?standard=2004`, that defines only `API_1484_11` and rejects values outside the 2004 vocabulary with error 406, so a wrong write shows up red in the log.
- `cargo xtask package-example` also publishes the showcase as SCORM 2004 to `target/mock-lms/scorm2004/` and `target/showcase-2004.zip`. Its page opens with the 2004 runtime selected.

### Spec

`docs/spec/triggers.md`: a "SCORM 2004" section holding the mapping table above. `docs/automation.md`: the `--standard` flag.

### Done means

- A fake 2004 API in `crates/lms/src/tests.rs` that enforces the 2004 vocabulary, covering: first launch, resume, pass, fail, completion without a quiz, finish (exit values), ISO session time, and a suspend payload between 4096 and 64000 characters that 2004 accepts and 1.2 refuses.
- Manifest tests: required parts present, capital-T `scormType`, no `masteryscore`, hostile input refused.
- Publish tests: the 2004 package has `coursecraft-package.json` listed in its manifest; the 1.2 package doesn't have it.
- `cargo xtask ci` passes.

## Step 2: question-level reporting

`Effect::QuestionAnswered` carries only slide, correctness and points. LMS quiz reports need the learner's answer. Add a pure function in `coursecraft-lms` from question plus player state to an `InteractionRecord` (id, SCORM interaction type, learner response, correct response, result, description), covering all eight question kinds. Write it as `cmi.interactions.n.*` in both dialects, with the vocabulary differences handled in the mapping (1.2 `student_response` and `choice` tokens against 2004 `learner_response` and `short_identifier` patterns). The record is standard-neutral so step 3 reuses it for xAPI `answered` statements.

## Step 3: xAPI and cmi5

- New L0 crate `coursecraft-xapi`: statement types and builders, standalone like `coursecraft-scorm`. Register it in `xtask` layers.
- `coursecraft-lms`: an `LrsApi` trait, mirroring `CmiApi`, and an `XapiBridge`. It turns effects into statements (`initialized`, `answered`, `passed`, `failed`, `completed`, `terminated`). In cmi5 it follows the required order and adds the cmi5 context, and it reads `LMS.LaunchData` for `masteryScore`, `moveOn` and `returnURL`. Suspend data goes in the xAPI State API. Statements queue and retry; nothing blocks play.
- Player: launch parameters from the URL (cmi5: `endpoint`, `fetch`, `actor`, `registration`, `activityId`; xAPI: `endpoint`, `auth`, `actor`, `registration`, `activity_id`), a `fetch` shim, and `keepalive` for the final statement on page close. Needs new `web-sys` features, named in the brief.
- Packaging: `publish --standard cmi5` (`cmi5.xml`) and `--standard xapi` (`tincan.xml`).
- Mock: the mock LMS runs on Python's `http.server`, which can't accept POST. `cargo xtask mock-lrs` is a small `std::net` server in Rust that serves the mock pages and implements the statements, state and fetch endpoints, with a log page.
- Tests: exact statement sequences for pass, fail and resume, against a fake `LrsApi`.

## Step 4: player keyboard access and focus order

- Model: `Object.focus_order: Option<u32>` and `Object.decorative: bool`, with `alt` extended to any object through `Object.description`. Default reading order is base-layer order, bottom to top, then visible layers. Engine commands and inspector fields for each.
- Player: `Tab` and `Shift+Tab` move through interactive objects and question controls in order, with a visible 2 px focus ring. `Enter` and `Space` activate through the same `Input`s a pointer would send. Keyboard routes through every question kind, including sequence, matching, hotspot and drag-and-drop (pick up, move, drop). Chrome joins the same order. Focus resets to the slide's first item on a slide change. Runtime semantics change, so the spec updates in the same change.
- Rule: every action available by pointer is available by keyboard. A test drives the whole showcase pass path with keys only and matches `simulate`.
- Desktop: `widget_info` on the same nodes so AccessKit gets labels.

## Step 5: web screen-reader layer

`egui` on the web paints to a canvas, and eframe's `web_screen_reader` feature is speech synthesis, not an accessibility tree. A screen reader sees nothing in the published player today, and this step is what makes the WCAG claim true.

- `coursecraft-render`: a pure `AccessTree` built from the same composition as the slide, a list of nodes with role, label, value, state, rect, action and order. It's unit-tested with no browser.
- Player: a DOM layer synced from the tree each change, positioned over the canvas the way `layout_videos` positions videos. Real elements (`button`, `img` with alt, `input`, `select`, headings), visually transparent and focusable. A live region announces slide changes and question feedback. DOM focus and egui focus stay in step in both directions.
- Check: `read_page` accessibility tree in the browser pane, plus a by-hand VoiceOver pass by the user.
- Limit to state: the DOM layer is per-slide, not per-glyph, so rich text reads as one block.

## Step 6: captions

- Model: `captions: Option<String>` on audio and video, a `.vtt` file under `media/`. Publish carries it, and validation checks it.
- `coursecraft-media`: one hostile-input-safe WebVTT parser (cues and timing, no styling), used everywhere.
- Display: a web video is an HTML element above the canvas, so anything egui draws over it is hidden (see M4's known limitation). On the web, captions are a DOM element above the video. On the desktop and for audio, `PlayerFrame` draws them. Both show the same cue.
- A CC toggle in the player strip, shown only when the slide has captions. It's a chrome preference, not runtime state, and it persists across slides.

## Step 7: accessibility checker

`coursecraft-course` gains `audit` findings with slide, object, WCAG criterion, severity and a fix message. Shown in the editor's validation list and through `coursecraft-cli a11y-check`, with a non-zero exit on errors for CI. Checks: missing alt text unless decorative, text contrast below 4.5:1 against the actual background (3:1 for large text), text under 12 pt, targets under 24 px, audio or video without captions, autoplay with no pause control, and a slide with no title. Reports stay honest: the checker covers what a tool can measure and says the rest needs a manual pass. CourseCraft doesn't call a course "WCAG compliant".

## Step 8: localisation

- XLIFF 1.2, the version most translation tools import. `quick-xml` is already in the workspace.
- Export: every translatable string (slide and scene titles, text and runs, button labels, alt text, descriptions, questions, choices, feedback, notes, and the player's chrome strings) with stable ids such as `slide/object/field`. Text runs become inline `<g>` tags so formatting survives translation.
- Import: an engine command (golden rule 2), so one undo reverts it. It writes a translated copy, reports units with changed source or missing translations, and refuses hostile XML (entity expansion, size limits).
- Model: `course.language` (BCP 47), which sets `<html lang>`, and `course.strings` for the player's chrome text ("Course complete", tooltips), so a translated course has no English left in it.
- CLI `export-xliff` and `import-xliff`, and File menu items. One package per language.
- **Limit to state:** bundled Inter covers Latin, Greek and Cyrillic only. CJK, Arabic, Hebrew and Indic scripts have no glyphs, and right-to-left and complex shaping aren't supported by the current text stack. Validation warns when a translation has characters the bundled fonts lack. Fonts and shaping for those scripts are a separate milestone.

## Step 9: M6 check

Pass, fail and resume by hand in the mock LMS under SCORM 2004 and the mock LRS under cmi5 and xAPI; the showcase by keyboard only; a screen-reader tree inspection; captions on audio and video; an XLIFF round trip. Roadmap updated. The user's SCORM Cloud run covers all three real zips.

## Not in M6

- **Publish from the editor.** Publishing is CLI-only today and needs the built player folder. It belongs with the next milestone's packaging work.
- Multi-SCO packages and SCORM sequencing rules.
- Non-Latin fonts and right-to-left text.
