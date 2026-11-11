# Roadmap

Stage: **pre-alpha.** The model, runtime, engine, CLI, the visual player (desktop and browser), SCORM 1.2 and 2004, cmi5 and xAPI publishing with LMS and LRS bridges (tested against mocks) and the egui editor (desktop and browser) exist and are tested.

## M0 Foundation ✅

- Course model + validator, trigger runtime, command engine with undo, CLI (`validate`, `simulate`, `run`, `manifest`, `commands`), layering check.

## M1 Player ✅

- `coursecraft-render` (L3): slide compositor on egui, covering text, shapes, buttons, states, hover/down, question choices, and timeline-driven entrance (no animations yet). Player chrome: menu, seekbar, prev/next.
- `apps/coursecraft-player`: desktop (eframe/wgpu) and browser (wasm, trunk) player with an effect log.
- `crates/render/tests/parity.rs` clicks through `hello-course` with real pointer events at four screen sizes and matches `simulate` line for line. Checked by hand in a browser too.
- Carried forward: images draw as placeholders and audio/video don't play (media arrives with `media/` in M2); `font` and `shuffle` are ignored; the seekbar is read-only.

## M2 Publish to an LMS ✅

- `coursecraft-publish` (L4) + `coursecraft-cli publish`: player + `course.json` + `media/` + `imsmanifest.xml` → folder and/or deterministic zip. ✅
- `coursecraft-lms` (L4): effects → `cmi` writes (score as percent of 100, `lesson_status`, `lesson_location`), compact `cc1:` suspend data from `coursecraft-logic`, start/resume, finish. The web player finds `window.API` across frames and drives it. ✅
- Tested green against the mock LMS (`cargo xtask package-example`, launch config `mock-lms`): pass (passed, 100), fail (failed, 0), resume after a mid-course relaunch (same slide, states kept), exit = suspend until complete. ✅
- Done bar accepted: the mock LMS. Real-LMS certification moved to "Later".
- Known LMS-dependent behaviour: resume happens whenever suspend data decodes, ignoring `cmi.core.entry` (see `triggers.md`, "Suspend and resume"); LMSs that clear suspend data per attempt start fresh.
- Follow-up: after a relaunch on an answered question slide, the feedback layer is gone, so a course without player navigation can trap the learner there (**[verify]** in `triggers.md`).

## M3 Editor ✅

- `coursecraft-ui-egui` (L6): menus and shortcuts from the command registry, slides panel with thumbnails, canvas (select, move, resize, insert tools), properties, layers, states, timeline, triggers (form with condition builder: compare, object state, all/any/not), variables, question form, course settings, validation list, in-editor preview with effect log. Every edit goes through engine commands (undo/redo, refusals shown in the status bar).
- `coursecraft-format`: `.ccraft` projects (zip of `course.json` + `media/`, hostile-input limits). `apps/coursecraft`: desktop open/save/save-as, recent files, atomic writes, 60 s autosave with restore, unsaved-changes prompt; browser build with file operations disabled.
- Done bar met: `walkthrough_tests::walkthrough_builds_hello_course_through_ui` builds all four slides of `hello-course` from a blank editor with UI input only and matches `simulate` line for line; `rebuild_script_recreates_hello_course` proves the command set. In the browser editor, slides 1–2 were built by hand and previewed (effects match `simulate`), slide 3's layers and triggers were built by hand, and the question form, course settings and preview were checked on the sample course. The by-hand browser course was lost when the pane closed (the web build can't save), so the quiz question and results slide were only built through the UI in the headless test.
- Carried forward: native file dialogs are covered by tests only (not driven by hand); bold text doesn't render bold (font has no bold face; fixed by the bundled font in M4); the trigger/combo popups need a pointer move before their list appears in very slow frames.

## M4 MVP: authoring essentials + automation ✅

Plan: `docs/plans/m4-mvp.md`.

**M3 + M4 is the MVP.** Priority is what course authors use most, built solidly; rarely used features wait (see "Not planned for now").

- Media: images render in the player and on the canvas from the packaged `media/`; audio and video play (autoplay, play/pause/stop triggers, `media_ends`). Desktop video: poster only in M4; plays in published (web) courses.
- Built-in variables (`Course.SlideNumber`, `Course.TotalSlides`, `Course.Progress`, `Course.ScorePoints`, `Course.ScorePercent`, `Slide.ElapsedMs`), `%var%` text references in player/preview, and bundled Inter font (regular, bold, italic, bold-italic) with real bold/italic rendering and editor property support.
- Rich text runs (`TextRun` with bold, italic, underline, size, color), paragraph alignment (`align`, `valign`), course theme (`accent`, `background`, `text`, `font`, `player`), slide background overrides, and editor formatting toolbar + Theme dialog.
- Known M4 limitation: on the web a video is an HTML element drawn above the canvas, so slide layers and overlays shown on top of a playing video's area are covered by it; videos are hidden when their object is not on screen.
- PPTX import: `coursecraft-pptx` (L3) reads `.pptx` slides, text runs, basic shapes with solid fills, pictures with media extraction, group flattening, slide notes and titles, bounded safely against hostile inputs; CLI `import-pptx` command; editor **File ▸ Import PowerPoint…** opens as new untitled dirty project with diagnostic report. ✅
- Animations (entrance/exit: fade, fly-in, zoom, wipe, float; motion paths with polyline editing and easing), slide transitions (fade, push, cover).
- Question types are done: true/false, fill-in, sequence, matching, hotspot, and drag-and-drop (model, grading and attempts, player widgets, question form).
- Built-in variables (progress, slide number), player themes, text formatting runs.
- Automation (step 8 complete ✅: 8a control channel + MCP + editor CLI; 8b CPU offscreen rasteriser `render_slide_rgba` + safe PNG encoding in `coursecraft-render` and `coursecraft-media`, JSON-RPC `render.slide`, `coursecraft-cli render`, MCP `render_preview` tool).
- MVP check (step 9): CourseCraft Showcase demo course (`examples/showcase/`) covering media, layers, states, variables, motion paths, transitions, all 8 question kinds, and SCORM reporting; mock LMS upgraded to host both hello-course and showcase; passes validation, pass/fail simulation, packaging, and offscreen render smoke tests. Played by hand in the mock LMS (web player): showcase pass (passed, 100) with every question kind answered through the UI, fail (failed, 0), resume after a mid-course relaunch (same slide, states and variables kept); hello-course pass (passed, 100); the two courses keep separate LMS records. SCORM Cloud run pending (needs the user's account).
- Marked done on 2026-11-01 at the user's call; the SCORM Cloud run of `target/showcase.zip` and `target/hello-course.zip` is still open (the user will test it).

## M5 Look and feel (UI/UX) ✅

Plan: `docs/plans/m5-look-and-feel.md`. Design system record: `DESIGN.md`. Direction (the user's choice): the category standard at Keynote's level of craft, dark by default with light one click away, and a minimal, content-first player.

- Step 1: design tokens and dark/light themes (`coursecraft-render::design`, WCAG AA tested), offscreen UI rasterising and `coursecraft-cli snapshot-editor`, the View ▸ Theme preference, and a global restyle.
- Step 2: Phosphor icon fonts with an `Icon` helper, icon buttons with tooltips and shortcuts, and a Keynote-style toolbar (icons only at narrow widths).
- Step 3: panels. Numbered navigator thumbnails with a "⋯" or right-click menu, an inspector with Format, Animate, Slide and Course views, canvas zoom (Fit to 200%, ⌘0, ⌘= and ⌘−) with layer pills, and a timeline with a ruler and zebra rows.
- Step 4: a welcome screen (new, open, PowerPoint import, bundled examples, recent files), empty-state hints, three first-run coach marks, and a `?` shortcut sheet.
- Step 5: the learner player's frame, shared by the published player and the editor preview: a letterboxed slide, a floating strip that hides when idle, a segmented progress rule, an outline drawer, keyboard control, and loading and "Course complete" screens. Adds `coursecraft-cli snapshot-player`.
- Step 6: a polish pass (labels, shortcut sheet, inspector header, player end card), the M4 polish list (question prompt overlap warning, fill-in padding, one-line matching dropdowns, readable drag-drop labels, smooth ellipses), Phosphor icons in place of emoji buttons, and `DESIGN.md`.
- Implementers: agy (Gemini 3.8 Flash) for steps 1–4 until its weekly quota ran out, then opencode (DeepSeek V4.1 Flash, free, falling back to paid) for steps 5–6, with review fixes by Sonnet and opencode.
- Not yet checked by hand in the desktop app or a browser; snapshot review only.

## M6 Standards & reach ✅

Plan: `docs/plans/m6-standards.md`. A course now works beyond SCORM 1.2, can be used by keyboard and screen reader, and can be translated.

- SCORM 2004 4th edition runtime and packaging, with one player binary reading a package marker. Question-level reporting (`cmi.interactions`) in SCORM 1.2 and 2004.
- xAPI and cmi5: `coursecraft-xapi`, a non-blocking bridge with retry and resume, `cmi5.xml` and `tincan.xml`, and `cargo xtask mock-lrs`, which validates every statement against the cmi5 rules.
- Keyboard access: everything a pointer can do, in author-set focus order, with a visible ring. On the web, a hidden HTML layer gives screen readers a real tree and live announcements, and holds real focus, so Tab can leave the course.
- Captions (WebVTT) with a CC toggle, on audio and on web video.
- Accessibility checker: `coursecraft-cli a11y-check`, `course.audit` and an editor section. It reports what a tool can measure, lists what a person must check, and never calls a course compliant.
- XLIFF 1.2 export and import, `course.language`, and `course.strings` for the player's own text.
- Played by hand in the mock LMS and mock LRS: 2004 pass, fail and resume with all eight question kinds; cmi5 pass, resume, retry after server errors, Browse, a reused launch; xAPI pass; keyboard entry and exit inside an iframe; video captions; a pseudo-translated course's `lang` and labels.
- Implementer: opencode (DeepSeek V4.1 Flash, paid), about $3 for the milestone, with review fixes by Claude.
- Still open: a real screen-reader pass (VoiceOver, the user); SCORM Cloud or another real LMS and LRS with `target/showcase.zip`, `target/hello-course.zip`, `target/showcase-2004.zip`, `target/showcase-cmi5.zip` and `target/showcase-xapi.zip` (the user). Covered by unit tests only, not played: cmi5 fail, `returnURL`, audio captions. The editor's Accessibility section, Captions row and translation dialogs have tests but no hand check, so they join M7's hand test.
- Known limits: rich text reads as one block to a screen reader; fonts cover Latin, Greek and Cyrillic only, with no right-to-left.

## Road to the first release

Priority (the user's): people can use CourseCraft themselves, in the visual editor, without spending any money. AI is optional help on top and is never required, because requiring it would mean paying for an AI subscription. The first release comes after M9, and later milestones aim to bring it as close to Storyline 360 as possible.

## M7 Usable without the command line ✅

Plan: `docs/plans/m7-usable.md`. A course can now be built, checked and published from the visual editor.

- Publish from the editor: File ▸ Publish… and a toolbar button, a standard picker, validation errors and missing media that block, accessibility findings that don't, and Show in folder. The player ships inside the desktop app through `cargo xtask build-player` and `apps/coursecraft/build.rs`.
- Found by walking the editor by hand and fixed: a question made in the editor had no way to be submitted; nothing told an author how to report a result to the LMS, so there's now a warning and a one-click Finish button; learners got no response to Submit, so questions start with Correct and Incorrect feedback layers; text couldn't be typed on the slide; scenes couldn't be renamed or deleted; menu items that could never work; dialogs at the window's top left; Esc not closing the preview; a cramped bottom panel; an inspector that could grow off screen.
- Hotspot and drag-and-drop questions start in a state an author can understand: a "No image yet" placeholder with Choose image…, and a labelled item and target.
- Preview from the start, an editable course title, and a language field.
- From M6: a dropped drag item and its target's label are both readable, and a launched course that loses reporting tells the learner.
- Played by hand in the web editor: a new course with typed text, a scene rename, a graded question, feedback, Finish and the end card in preview, all from the UI; the Captions row and the Accessibility section. Played in the mock LRS: the lost-reporting banner after a refreshed cmi5 launch, and its dismiss button; the dropped drag label.
- Implementers: opencode with DeepSeek V4.1 Flash for steps 1 and 2 and the fixes, GLM 5.3 Flash for step 3 as a trial, about $2.53 in all. Review fixes by Claude.
- Still open, for the user: the desktop app by hand. It starts with the player embedded, but its native dialogs couldn't be driven here, so **Publish in the desktop app, Show in folder and the translation dialogs have tests only**. A zip made that way should go through SCORM Cloud with the M6 zips.
- Known limits: the web editor doesn't publish. A Finish button is usable before the question is answered, so an author may want to move it to a later slide. A brand-new blank course starts with one warning until it has a Finish button. Opening a bare `course.json` on the desktop doesn't load a `media/` folder beside it, and Publish then lists the files as missing; `.ccraft` projects are the normal path.

## M8 Faster authoring ✅

Plan: `docs/plans/m8-authoring.md`. An author can select many things, line them up, reuse layouts and fix text across a course without touching slides one at a time.

- Multi-select, marquee, group move and resize, and copy, cut and paste of objects and slides, through the system clipboard.
- Align, distribute and match size, with a group moving as one unit. Smart guides while dragging, and grouping with double-click to reach a member.
- Slide masters: a logo and background carried by every slide that uses the master, with no play-time logic. Six slide layouts, and templates saved from any slide.
- Find and replace across a course: formatting kept, `%variable%` references protected, every result clickable, including one on a master or in a question.
- Image crop on the canvas, kept by the object, with PowerPoint crops imported. An uncropped image draws exactly as it did before.
- Played by hand in the web editor: marquee, group move, align, distribute, snapping to a neighbour, grouping, undo; master view, a logo and background reaching slides and preview, a layout, a template; find, Replace, Replace All, skipped references, Esc; crop with a handle, Enter, Reset, undo, Esc. `simulate` output on the examples didn't change.
- Implementers: opencode with DeepSeek V4.1 Flash for steps 1 to 3, Haiku 5.5 at low effort for step 4 as a trial, DeepSeek direct for step 5. Costs are in the run report kept outside the repository.
- **Tests only, not played:** the guide line drawn during a drag (the result of snapping was played); moving the picture under a crop frame, and Shift to keep a crop's proportions; a PowerPoint crop, from a fixture rather than a real file; real Cmd+C and Cmd+V, which were simulated.
- Known limits: a template lives in its course, not across courses. A master can't hold audio, video, triggers, a question or layers. Replace on one row replaces every match in that text. After undo, the Find window still says `Replaced 3.` until the search changes.

## M9 AI-assisted authoring ✅

Plan: `docs/plans/m9-ai.md`. Optional help that's off by default, with any OpenAI-compatible endpoint, including free local servers. Everything still works without it.

- Slides from a pasted outline, with no AI at all.
- AI Settings with presets and a connection test. A key stays in the desktop app's owner-only prefs, or for the session on the web, and a test holds it out of courses, packages, logs and errors.
- A document to a course, from pasted text or `.docx`, `.txt` or `.md`. The answer opens in the outline review window.
- Suggested alt text, translation through the existing XLIFF importer, captions from audio, and narration from text.
- Each feature says what's being sent and where before the first request, and shows the result for review. Accepting is one undo step, and an answer never gets past the checks a loaded file gets.
- Played by hand in the web editor against `cargo xtask mock-ai`: settings and connection test, not-set-up state, document to outline to slides, suggest and apply alt text, 162 units translated and applied on the showcase, narration added to a slide, captions used and undone.
- Implementers: opencode with DeepSeek direct, about $0.70 for steps 2 to 6. Review fixes by Claude.
- **Not tested at all: a real AI provider.** Every run used the mock, so real answers, real speed, the quality of a translation or description, real speech and captions, and a provider's real errors are unknown. **Tests only:** a real `.docx` (fixtures only), the desktop app's key file, the 404 message for a server with no audio, and Cancel by hand.
- Known limits: the web editor can reach only servers that allow browser requests. A document over 60,000 characters is cut, not summarised in parts. Narration has no Play button in its window, so listen in preview. Undoing a generated narration or captions leaves the file in the project's media until it's closed. Translating isn't useful for scripts the bundled font lacks yet. At a small window the translation table shows about five rows at once and scrolls.

## First release

Once M6 to M9 are tested and verified: upload to GitHub and publish `.dmg`, `.exe` and the Linux packages, built after `cargo xtask build-player` so the player is inside them. **Ask the user when the M9 steps are done, before starting this.**

## Toward Storyline 360 and Captivate

The user's order (2026-11-06), started straight away without waiting for review between milestones. Spell check, a free content library, more AI presets, and the big features (screen recording, characters, 360° images, review workflow, phone layouts) wait until later.

## M10 AI through agents and subscriptions ✅

Plan: `docs/plans/m10-ai-agents.md`.

- AI Settings has two tabs. **Agents** finds Claude Code, Codex, Claude Desktop, Cursor and Gemini CLI (also with the bare `PATH` a Mac app gets) and connects, tests or disconnects each with a click; the old copy lines sit under "Set up by hand". **AI features** picks an installed Claude Code or Codex on the author's own plan, or an API endpoint as before.
- Played on this Mac: detection found Claude Code, Codex and Claude Desktop; Codex answered a text request and described an image through the real CLI; Claude Code, not signed in on this Mac, gave the "isn't signed in" sentence.
- Implementer: opencode with DeepSeek direct, about $0.38. Review fix by Claude: run the CLI that was found, not whatever `PATH` finds first.
- **Tests only:** Connect and Disconnect against the real Claude Code, Codex and Claude Desktop settings (they change the user's own settings, so they're left for the user), Claude Code answering while signed in, Cursor and Gemini CLI, Windows.

## M11 The basics ✅

Plan: `docs/plans/m11-basics.md`.

- Rotate and flip, with a canvas rotate handle (Shift snaps to 15°) and resizing in the object's own axes.
- Opacity, linear, radial and picture fills, shadow and glow.
- Bulleted and numbered lists with levels, line spacing, space before and after, links in text (a web address or a slide), and text that shrinks to fit.
- Custom fonts: TrueType and OpenType files added to the course, embedded, published and listed in the font picker.
- SVG images, rasterised at the size they're drawn; animated GIF and WebP that loop in the player.
- Lottie animations (`.json` and `.lottie`) drawn by CourseCraft's own renderer through velato, with play, pause, stop and `media_ends`.
- Tables with rich-text cells, merged cells, fills, borders and a header row, edited on the canvas, read as real tables by screen readers.
- PowerPoint import picks up each of these where the deck has them.
- Checked by Claude in offscreen renders of a test slide for every step. Review fixes by Claude: run the found agent CLI (M10), soft shadow edges, link underlines on the baseline, list markers on the first baseline, and `.lottie` read with the zip crate instead of a hand-written inflate.
- Implementer: opencode with DeepSeek direct, about $2.40 for the seven steps.
- **Not played by hand yet** in the web editor or the desktop app: the rotate handle, inspector controls for fills and effects, the formatting toolbar's list and link buttons, Add font…, table editing on the canvas, and Lottie in preview. No real PowerPoint deck with these features has been imported yet.
- Known limits: spell check is left for later. Lottie masks clip to a bounding box, embedded Lottie images are skipped, and outlines show small spikes at some joins. Glows show faint rings. SVG text isn't drawn. Find and replace that adds or removes line breaks doesn't realign paragraph styles. The autofit control sits at the bottom of the inspector.

## M12 Richer interactions ✅

- Web object (a URL or a folder of HTML), with a first-frame preview in the editor if that's not too much work, otherwise a placeholder.
- Run JavaScript as a trigger action (web player).
- Text entry fields, sliders, dials, checkboxes and radio buttons that write to variables.
- Tabs, accordions, markers, scrolling panels, lightbox slides.
- Triggers: key pressed, dropped on an object, objects overlap, timer ends. Timer and progress-meter objects.
- Interactive video: questions and hotspots at times in a video.
- A map of the whole course.
- Checked: inputs, panels, lightboxes, triggers, timers and video cues played in a published course; the course map has automated tests and `course.map` output, and its look still needs a review in the app. Desktop video colour and sync were measured on the real decoder on macOS; Windows has been compiled but not run.

## M13 Deeper quizzes ✅

- Question banks, random draw, results slides, retry, answer review, certificates.
- Survey questions (Likert, short answer, essay), points per answer and partial credit, any slide as a graded question.
- Checked: scoring by simulation; surveys, results, retry and marked review played in a published course. Question banks by automated tests and simulation. The editor controls and the certificate print dialog still need a look in the app.

## M14 Media ✅

- Video in the desktop editor and preview; trim and volume for audio and video; recording narration from the microphone; background audio across slides; a media library panel. **Update (M12, 2026-11-09): "Video in the desktop editor and preview" moved earlier and is done in M12** (decoder step 1 plus timeline playback step 2). The rest stays here.
- Checked: automated tests and the gate. Recording on a real microphone, background audio by ear and the library panel still need a look in the app. Windows has been compiled but not run.

## M15 Publishing ✅

- Export as MP4 video, as a PDF or Word document for review, and LTI 1.3 through a self-hosted tool server.
- Checked: automated tests and the gate; a real MP4, PDF and Word file of the sample course opened and looked right. The Export dialog still needs a look in the app, and LTI has not been launched from a real LMS. Windows has been compiled but not run.

## M16 Everyday polish ✅

- Record Narration on the toolbar, a colour picker on every colour field with theme and recent colours, a format painter, lock and hide for objects, rulers, a grid and guides, slide size with content scaling, copy, paste, duplicate and disable for triggers, strikethrough, superscript, subscript and letter spacing, and a Preferences window (autosave interval, default slide size).
- Checked: automated tests and the gate. None of it has been tried in the app by hand yet. Lock, hide, rulers, grid and guides last for the session only.
- Left for later: spell check, image paste from the clipboard, an eyedropper, and strike and baseline in the Word export.

## M17 Learner player parity ✅

- Navigation modes (free, restricted, locked) and "Next unlocks" rules (when the timeline ends or a variable is true), per course with per-slide overrides. Playback speed and full screen. Resources, glossary and transcript tabs in the player drawer. Player logo, title and control toggles, and per-slide show or hide for Next/Previous, seekbar and menu.
- Checked: automated tests and the gate. Not tried by hand: full screen in a real window or browser, playback speed in a browser, and the new editor settings.
- Limits: playback speed is offered only in the web player, because the desktop audio cannot change speed yet. File resources open only in the web player. Full screen hides itself inside an LMS frame that does not allow it.
- Left for later: learner text zoom, speed in the desktop player, and choosing a resource file in the browser editor.

## M18 Proof before release (partly done)

Code done; the checks that need accounts, a Windows machine or a person are in `docs/release-checklist.md`.

- Big-course test: 300 slides and 1.3 GB of media, numbers in `docs/performance.md`. Save went from 34 s to under a second, publish from 35 s to half a second. A project over 512 MB used to save and then refuse to open; that is fixed, and a save is now refused if the file could not be opened again.
- Safe saving: the file is synced and checked on disk before it replaces the last save, retries when a sync service holds it, warns if the file changed since it was opened, and damaged files give a plain message. Version history keeps the last 10 saves outside the project folder and restores them as a copy.
- Limits: a project opens up to 4 GB, with no single file over 1 GB.
- Checked: automated tests, including a kill-during-save test, and the gate. Not tried by hand: the new dialogs in the real app. The retry for locked files compiles for Windows but has not run there.
- Still open, for the user (the checklist): SCORM Cloud and a real LMS, LTI from a real LMS, the Windows and macOS hand tests, VoiceOver and NVDA, a real AI provider, and saving inside Dropbox, OneDrive and iCloud.

## M19 Agents and the open format ✅

Goal: things Storyline and Captivate can't do at all, built on the command engine and the open `course.json`.

- A written promise for the `course.json` format (`docs/spec/course-model.md`); a file from a newer CourseCraft gives a plain message. `coursecraft-cli diff` says in plain words what changed between two versions.
- A project can be a folder instead of a zip, for git (`docs/git.md`). Saves write only changed files; files changed outside the app go to the changed-on-disk dialog. `pack` and `unpack` convert between the two. On the 300-slide course: first save 2.8 s, unchanged save 0.27 s, open 1.4 s.
- Named text styles and a theme palette. Changing a palette colour or a style updates the whole course in one undo step.
- `coursecraft-cli batch` runs one script over many courses (rebrand, replace a logo, update a date, republish), with a dry run that shows the diff. Questions import from a CSV file, all or nothing, with row-numbered problems. Four agent recipes in `docs/recipes/`, checked by a test against the command list.
- Checked: automated tests and the gate. Not tried by hand: the folder open and save dialogs, the text style and palette controls, and the CSV import menu item.

## After the first release

- More languages: fonts and text layout for Hindi, Arabic, Chinese, Japanese and Korean, so more testers can join. Until then, testing uses English and the other scripts the bundled font supports.
- Phone-friendly, scrolling lessons, once CourseCraft can compete with Storyline 360. This is where it starts to stand apart from the competition.

## Later

- Wider real-LMS certification: a local Moodle and other LMSs (completion, pass/fail, score, resume).

## Not planned for now

Rarely used in practice; revisit only once the core is complete and stable.

- `.story` import (interop only; would also need a legal check, see `contributing.md`).
- Screen recording, characters, 360° images, review/comment workflow.
- A free content library of templates, interactions and illustrations.
