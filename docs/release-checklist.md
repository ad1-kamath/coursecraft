# Release checklist: what is left to try by hand

Everything here was built and tested by machine but never played by a person on the real thing. Tick each box. If something fails, note what you clicked and what you saw.

The zips live in `target/` (rebuild any time with `cargo xtask package-example`; the LTI one with `cargo run -p coursecraft-cli -- publish examples/showcase/course.json --player apps/coursecraft-player/dist --standard lti --out-dir target/showcase-lti -o target/showcase-lti.zip`):

| Zip | Standard |
|---|---|
| `target/hello-course.zip` | SCORM 1.2, 4 slides, pass mark 80% |
| `target/showcase.zip` | SCORM 1.2, all 8 question kinds, 80 points |
| `target/showcase-2004.zip` | SCORM 2004 |
| `target/showcase-cmi5.zip` | cmi5 |
| `target/showcase-xapi.zip` | xAPI |
| `target/showcase-lti.zip` (and the folder `target/showcase-lti/`) | LTI |

**What a pass looks like for any LMS item.** Answer every question right: status Passed (or Completed), score 100. Answer every question wrong: Failed, score 0. Leave halfway, close the window, launch again: you land on the same slide with your answers, states and variables kept.

---

## 1. SCORM Cloud

Time: about 45 minutes. Needs: a SCORM Cloud account (the free trial is enough), a browser.

- [ ] Upload `target/hello-course.zip`. Launch it, click through, answer the quiz right. Pass: Passed, score 100.
- [ ] Launch it again as a new registration and answer wrong. Pass: Failed, score 0.
- [ ] Launch, get to the quiz slide, close the tab, launch the same registration again. Pass: back on the quiz slide.
- [ ] Upload `target/showcase.zip`. Play every one of the 8 question kinds (multiple choice, multiple response, true/false, fill-in, sequence, matching, hotspot, drag and drop). Pass: Passed, 100. Open the registration's Interactions view: 8 questions listed.
- [ ] Showcase, all wrong: Failed, score 0.
- [ ] Showcase, leave after question 4 and relaunch: same slide, the 3 tab buttons you visited still show as visited.
- [ ] Upload `target/showcase-2004.zip`. Repeat pass, fail and resume. Pass: Completed plus Passed or Failed, score right.
- [ ] Upload `target/showcase-cmi5.zip` (as cmi5). Pass, resume, and a fail run. Pass: the fail run reports Failed. Also look at what the "return" link at the end does (it should go back to SCORM Cloud, not a blank page).
- [ ] Upload `target/showcase-xapi.zip`. One pass run. Pass: statements show in the registration's xAPI activity.
- [ ] On the showcase, play the audio clip with captions on (CC button). Pass: captions show in step with the sound.
- [ ] Make a zip with the desktop app: File > Publish, SCORM 1.2, of any course you build (see section 4). Upload and pass it. Pass: same as above.

## 2. Moodle (or another real LMS)

Time: about 1 hour. Needs: a Moodle site you can add activities to (a local Docker one or a hosted trial), a teacher account and a student account.

- [ ] Add a SCORM activity with `target/showcase.zip`. As the student, pass it. Pass: the gradebook shows 100 and the activity shows complete.
- [ ] As the student, attempt again and fail. Pass: the grade follows the setting you chose (highest, last, and so on) and the attempt shows failed.
- [ ] Leave halfway and re-enter. Pass: the LMS offers to resume and you land on the same slide.
- [ ] Add a SCORM activity with `target/showcase-2004.zip`. Pass and fail runs report the same way.

## 3. LTI from a real LMS

Time: about 1.5 hours. Needs: a Moodle (or Canvas, Blackboard) where you can register an external tool; a machine with a public HTTPS address in front of `coursecraft-lti` (Caddy or nginx; see `docs/cli.md`, "LTI 1.3"); `openssl`.

- [ ] Follow `docs/cli.md`: make a key, write `lti.json`, run `coursecraft-lti check lti.json`, paste the three URLs and the key id into the LMS tool registration. Pass: `check` prints them without error and the LMS accepts the registration.
- [ ] Serve `target/showcase-lti/` with `coursecraft-lti serve lti.json`. Add the tool to a course and launch as a student. Pass: the showcase opens inside the LMS and shows the learner's name.
- [ ] Finish with all answers right. Pass: the LMS gradebook shows 100. Finish with all wrong: it shows 0.
- [ ] Open `http://<your host>/lti/health`. Pass: it answers.
- [ ] Launch with a wrong or expired key. Pass: a plain error, no crash, no course shown.

## 4. macOS desktop app by hand

Time: about 2 hours. Needs: this Mac, the built app (`cargo run -p coursecraft --release`), a microphone, a real `.pptx` deck with pictures, rotated shapes, lists, links, a table and a cropped picture, a `.docx` file.

Files and saving (new in M18)

- [ ] File > New, add a slide, Save As to the Desktop, quit, reopen from File > Open Recent. Pass: the same course opens.
- [ ] Save twice, then File > Version History. Pass: one version listed with a date, size and slide count. Click Open as Copy. Pass: a new unsaved course titled "... (restored ...)"; the original file on the Desktop is unchanged.
- [ ] Preferences > Versions to keep: set 2, save four more times. Pass: only 2 versions remain. Set 0 and save: no new version. Set 100 and drag past it: it stops at 100.
- [ ] Open the project file in a text editor, delete half of it, save, and open it in CourseCraft. Pass: a plain message ("can't be opened ... damaged"), the file is not changed, and "Open Latest Version" opens the newest version as a copy.
- [ ] Open a project, then (in Finder) replace its file with another copy of the same project that has a different edit, then press Save. Pass: "This file changed since you opened it" with Overwrite, Save a Copy and Cancel; each button does what it says.
- [ ] Autosave: edit, wait for the autosave interval, quit with the app killed (Force Quit). Reopen the file. Pass: the restore prompt appears and restores your edit.

Native dialogs and menus (M3, M7)

- [ ] File > Open, Save As, Import PowerPoint, and picking an image, audio, video, caption and font file all open native dialogs and work.
- [ ] File > Publish in the desktop app: pick SCORM 1.2, publish, then Show in folder. Pass: the zip is made and Finder shows it.
- [ ] Translation: export XLIFF for a language, import the filled file back. Pass: the strings change and undo works.

Look and feel (M5)

- [ ] Switch Preferences > Appearance between System, Light and Dark. Pass: the whole app, including the player preview, changes with no unreadable text.
- [ ] Press `?` for the shortcut sheet and walk the first-run coach marks on a fresh prefs folder. Pass: they read right and can be dismissed.

Authoring (M8, M11, M12, M13, M16)

- [ ] Drag an object near another: the guide line shows while dragging. Use real Cmd+C and Cmd+V on objects and on slides. Pass: copies appear.
- [ ] Crop a picture, then drag the picture under the crop frame, and hold Shift while resizing the crop. Pass: the picture moves under the frame, and Shift keeps the proportions.
- [ ] Use the rotate handle (Shift snaps to 15 degrees). Pass: the object rotates and flips.
- [ ] Inspector: set a gradient fill, a picture fill, a shadow and a glow. Pass: the slide shows them and they survive Save and reopen.
- [ ] Formatting toolbar: make a bulleted and a numbered list, add a link to a web address and to a slide. Pass: they show in preview and the links work.
- [ ] Add font: pick a `.ttf`. Pass: it appears in the font picker and shows in preview and in a published course.
- [ ] Table: add a table, type in cells, merge two cells, give them a fill and a border, tab between cells. Pass: all work on the canvas.
- [ ] Add a Lottie file to a slide and play it in preview. Pass: it animates.
- [ ] Import your real `.pptx` deck. Pass: pictures, rotation, lists, links, table and crops land close to the original.
- [ ] Open the Course Map. Pass: it reads clearly (its look has had no review in the app).
- [ ] Add a question bank, a results slide and a survey question; open the certificate print dialog. Pass: the editor controls make sense and the certificate prints or saves to PDF.
- [ ] Lock and hide objects, rulers, grid and guides; colour picker on a fill; format painter; Slide Size with content scaling; copy and paste triggers; strikethrough, superscript, subscript and letter spacing. Pass: each does what its label says. (Lock, hide, rulers, grid and guides last for the session only.)

Media (M14, M15)

- [ ] Record Narration with the real microphone. Pass: you hear your voice on playback and it can be undone.
- [ ] Set course background audio and listen across slide changes. Pass: it plays across slides and stops when asked.
- [ ] Open the media library panel. Pass: it lists, replaces and shows unused media.
- [ ] File > Export as MP4, PDF and Word. Pass: files open and look right.
- [ ] Video in the editor and in preview: play, trim, volume, a cue point. Pass: picture and sound stay in step.

Player (M17)

- [ ] Preview the showcase in the app and the published web version in Safari: full screen (the button and Esc), playback speed menu, Resources, Glossary and Transcript tabs, the player logo, and navigation modes (free, restricted, locked) with "Next unlocks" rules. Pass: each does what the editor setting says.
- [ ] Inside an LMS frame (SCORM Cloud), full screen hides itself if the frame does not allow it. Pass: no broken button.

## 5. Windows

Time: about 1.5 hours. Needs: a Windows 10 or 11 machine, the Windows build of the app (compiled but never run).

- [ ] The app starts, shows the welcome screen, and a course can be built, saved, closed and reopened.
- [ ] Open a course with video. Pass: the video plays in the editor and in preview with sound and without a green or wrong-coloured picture.
- [ ] Record Narration from the microphone. Pass: it records and plays back.
- [ ] File > Export as MP4. Pass: the file plays.
- [ ] Publish a SCORM zip and upload it to SCORM Cloud. Pass: same as section 1.
- [ ] AI Settings > Agents: it finds Claude Code, Codex or Cursor if installed there. Pass: detection works and Connect and Disconnect change only that tool's own settings.
- [ ] A project with spaces and accented letters in its path saves, reopens and publishes.
- [ ] Save while OneDrive is syncing the file (section 8). Pass: a save either works or says "Another app, such as a sync service, is using this file. Try again in a moment."

## 6. Screen readers: VoiceOver and NVDA

Time: about 45 minutes each. Needs: a Mac with VoiceOver and Safari (and Chrome); a Windows machine with NVDA and Firefox or Chrome. Use the published `target/showcase.zip` unpacked and opened through SCORM Cloud, and the app's editor.

- [ ] Tab through the first slides with the screen reader on. Pass: every button is spoken by name and focus order matches the author's order.
- [ ] Answer a question with the keyboard only (all 8 kinds, or at least multiple choice, sequence, hotspot and drag and drop). Pass: you can do it and Submit and the feedback are announced.
- [ ] Tab into the course inside the LMS frame and out of it again. Pass: focus is never trapped.
- [ ] Open a slide with a table. Pass: it reads as a table with headers.
- [ ] In the editor: open the Accessibility section, the Captions row, and the translation dialogs with the screen reader. Pass: controls have names and can be used. Note anything that reads as one block (rich text is known to).
- [ ] Open the Version History and Preferences windows. Pass: Open as Copy and Delete buttons announce which version they belong to; every field has a label.

## 7. A real AI provider

Time: about 45 minutes. Needs: an API key for an OpenAI-compatible service, or Claude Code or Codex signed in on this Mac; a real `.docx`; a recorded audio clip; the showcase course.

- [ ] AI Settings > AI features: pick your provider and press the connection test. Pass: it says connected. Give a wrong key. Pass: a plain error sentence.
- [ ] Course from Document with your real `.docx`. Pass: an outline appears, and accepting it makes slides.
- [ ] Suggest alt text on a picture, apply it, undo it.
- [ ] Translate the showcase into another language and apply. Pass: it reads naturally and the layout still fits; judge the quality.
- [ ] Narration from text and captions from audio with the real service. Pass: you can hear the speech and the captions match. Try a server with no audio support. Pass: the "no audio" message, not a crash.
- [ ] Press Cancel during each kind of request. Pass: it stops and a late answer is ignored.
- [ ] AI Settings > Agents: Connect and Disconnect for Claude Code, Codex and Claude Desktop, and test Cursor and Gemini CLI if you have them. These change your own tool settings. Pass: each switches on and off cleanly. With Claude Code signed in, an AI request answers.
- [ ] Check the key file in the app's prefs folder is readable only by you (`ls -l` shows `-rw-------`).

## 8. Saving inside Dropbox, OneDrive and iCloud

Time: about 30 minutes each. Needs: an account and the desktop client for each service (on two machines, or one machine plus the service's website, for the "changed" test).

Do each of these once per service:

- [ ] Put a project in the synced folder, open it, edit, and save five times in a row. Pass: no "conflicted copy" files appear and no error shows.
- [ ] Edit and save the same project from the other machine (or upload a changed copy on the website), wait for it to sync, then Save here without reopening. Pass: "This file changed since you opened it"; Overwrite, Save a Copy and Cancel all work.
- [ ] Look in the synced folder after a few saves. Pass: only the project and its `.autosave.ccraft` are there; no `.tmp-` files and no version copies (versions are in `~/Library/Application Support/CourseCraft/versions/` on a Mac, `%APPDATA%\CourseCraft\versions\` on Windows).
- [ ] While the service is uploading the file, press Save. Pass: it succeeds or, after about 2 seconds, shows the "sync service" message; the old file is never damaged.
- [ ] Open the project from an iCloud or OneDrive file that is only online (not downloaded yet). Pass: it downloads and opens, or says why it cannot.
