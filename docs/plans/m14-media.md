# M14 Media

Goal: the sound and video work an author does on every course without leaving CourseCraft: cut a clip to the part they need, balance its volume, record their own narration, lay music under the whole course, and see every file the project holds in one place. Video in the desktop editor and preview landed in M12. Every runtime change lives in `coursecraft-logic` and is written into `docs/spec/triggers.md` in the same change. Every new field goes through engine commands with undo and is driveable from the CLI and MCP. L0–L5 and the player keep building for wasm; anything that talks to a microphone or an OS media framework is `cfg(not(target_arch = "wasm32"))`.

## Step 1: Trim and volume

- `ObjectKind::Audio` and `ObjectKind::Video` gain `volume: f32` (0.0–1.0, default 1.0, omitted at 1.0) and `trim: Option<Trim { start_ms: u64, end_ms: Option<u64> }>` (omitted when unset). A course without them serializes byte-for-byte as before.
- Runtime: a trimmed object starts at `start_ms` (the runtime's `media_seek` after `Play` carries it) and ends when the host's `media_time` reaches `end_ms`: the runtime emits `Media { Stop }` and runs `media_ends`, the same as a natural end. Cue times stay in file time. Hosts report `media_time` for every playing audio or video with a trim end, not only videos with cues.
- Volume: a new `MediaBackend::set_object_volume(object, volume)` (default no-op) multiplies the player volume; the media driver sets it when playback starts. Desktop: the rodio sink; web: the element's `volume`.
- Inspector: Volume slider (0–100%) and a Trim row with start and end fields in `m:ss.t`, a Reset button and a "Use clip length" hint; `object.set_media { slide, object, volume?, trim? }` is the command behind both.
- Deferred: a waveform (needs decoding in the editor on both platforms), fade in and out, ducking other sound under narration, playback speed.

## Step 2: Record narration

- `coursecraft-playback::record` (desktop only): `input_devices()`, `Recorder::start(device)`, a live peak level for the meter, `stop()` returning 16-bit PCM capped at 30 minutes, and `encode_wav`. Built on `cpal`, which rodio already compiles into the app, so the microphone comes from CoreAudio on the Mac and WASAPI on Windows with no new download and no `unsafe` of our own.
- Editor: Insert ▸ Record Narration… opens a window with a device picker, Record and Stop, a level meter, the elapsed time, Play back (through the existing media backend), Re-record and **Use recording**, which adds `narration-N.wav` to the project and an autoplay audio object on the current slide, as Narration from Text does. The web editor shows the window with a hint that recording works in the desktop app.
- Deferred: recording in the browser editor (MediaRecorder), noise removal, trimming inside the recorder (Step 1's trim does it on the object), recording while the slide plays.

## Step 3: Background audio across slides

- `Course.background_audio: Option<BackgroundAudio { src, volume (default 0.5), loop (default true) }>`. The runtime plays it as the reserved media object `Course.BackgroundAudio` on `start` and after a resume, keeps it playing across slide changes, layer changes and timeline pauses, and starts it again when it ends while `loop` is set, unless the host reported the end before the first tick after `Play` (a browser's autoplay refusal), so a blocked start never loops. `play_media`, `pause_media` and `stop_media` accept that id, and `media_ends` can name it, so an author can add a mute button.
- The media driver resolves the reserved id to the course's file and volume; the player volume control still applies; publishing packages the file.
- Course settings: a Background audio row with Choose…, Remove, Volume and Loop; `course.set_background_audio { src?, volume?, loop? }`.
- Deferred: per-scene background tracks, crossfades between tracks, lowering the music while slide audio plays.

## Step 4: Media library

- View ▸ Media Library: a window listing every file in the project's `media/` with its kind, size and a "Used by" count from every reference in the course (objects on slides, layers, masters, templates and banks; picture fills; captions; fonts; background audio; web folders), a thumbnail for images and a filter box.
- Actions: Import… (adds a file without placing it), Insert on slide (image, audio, video, Lottie), Replace file… (same name, new bytes), Delete (unused files only) and Delete unused, each with a confirmation and a status line saying the file operations are not undoable, because the project's media list lives outside `course.json`. Selecting a used file lists the slides that use it; clicking one opens it.
- Deferred: renaming a file (every reference would need rewriting), folders and tags, drag from the library onto the canvas.

## Done bar

Each step: model and validation tests, logic tests with `simulate` scripts where the runtime changed, playback backend tests with the null backend, render and UI harness tests, `docs/spec/triggers.md` and `docs/spec/course-model.md` updated, `simulate` on the examples unchanged, `cargo xtask ci` green including the wasm check, and the step played by hand in the desktop app (recording, trim, volume, background audio) or the mock LMS (background audio looping and the mute button) after review.
