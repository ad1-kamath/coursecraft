# M2 plan: publish to an LMS

Goal (from `docs/roadmap.md`): a course publishes to a SCORM 1.2 zip that an LMS can launch, and the LMS records completion, pass/fail, score and resume.

**Interim done criterion** (real LMS testing waits for accounts): the published `hello-course` runs inside a local mock LMS page (`xtask/mock-lms/`). That page provides a SCORM 1.2 `window.API` that records every call. Passing, failing and resuming after a reload must produce the expected `cmi` values there. SCORM Cloud and Moodle stay on the roadmap.

## Shape

```text
coursecraft-logic (L2)   suspend: PlayerState <-> compact text (resume rules in triggers.md)
coursecraft-lms (L4)     LmsBridge: effects -> cmi writes, start/resume, unload; over a CmiApi trait
coursecraft-publish (L4) package: course + player files + media -> SCORM 1.2 file set -> zip
coursecraft-cli          `publish` command (filesystem side)
coursecraft-player       wasm: finds window.API, implements CmiApi with js-sys, drives LmsBridge
xtask                    `package-example` and `mock-lms` (test tooling)
```

Everything that decides *what* to send to the LMS is pure and unit-tested (`coursecraft-lms` with a fake API). The player app only adapts JavaScript calls to the `CmiApi` trait.

## 1. Suspend data (`crates/logic/src/suspend.rs`)

- `pub fn encode_suspend(course: &Course, st: &PlayerState) -> String` and `pub fn decode_suspend(course: &Course, text: &str) -> Result<PlayerState, SuspendError>`.
- **Durable subset only:**
  - current slide (play-order index)
  - visited slides
  - object states that differ from the object's `initial_state`
  - variables
  - answers
- **Not stored:** clocks, visible layers, selection and epoch. Resume re-enters the slide, so these reset anyway.
- Ids are replaced by course-relative indices:
  - slide: play-order index
  - object: index in the slide's base objects, then each layer's objects, in model order
  - state: index in `["normal", "hover", "down", "visited", "selected", "disabled", "hidden"]` followed by the object's own non-builtin state names
  - variable: index in `course.variables`
- **Format:** `cc1:` followed by unpadded base64url (hand-written, no dependency) of a binary body. The body uses LEB128 varints for counts and indices, one byte for bools, 8 bytes little-endian for `f64`, and varint length + UTF-8 for text. `answers` record slide index, `correct`, `attempts` and `points`.
- **Decode is hostile-input safe:**
  - Every index is checked against the course.
  - Lengths are bounded by the remaining input.
  - A variable must keep the type of its initial value.
  - Trailing bytes are an error.
  - It never panics.
  - It returns `PlayerState { started: false, exited: false, .. }` with the decoded fields and defaults elsewhere.
- **Resume rule (spec):** the LMS layer calls `Player::resume(course, decoded)` and then sends `start`. The runtime enters the saved slide through the normal slide-entry steps, which keep object states, variables, visited slides and answers. No runtime code changes are expected beyond this module. Add a "## Suspend and resume" section to `docs/spec/triggers.md`.

## 2. LMS bridge (`crates/lms`, L4)

```rust
pub trait CmiApi {
    fn initialize(&mut self) -> bool;
    fn get_value(&mut self, key: &str) -> String;
    fn set_value(&mut self, key: &str, value: &str) -> bool;
    fn commit(&mut self) -> bool;
    fn finish(&mut self) -> bool;
    fn last_error(&mut self) -> String; // LMSGetLastError + LMSGetErrorString, for warnings
}
```

`LmsBridge` owns no player. It is driven by the app:

- **`start(api, course) -> StartPlan`.**
  - Calls `initialize`.
  - Reads `cmi.core.lesson_status` and `cmi.suspend_data`.
  - If the status is `not attempted`, sets it to `incomplete`.
  - If the suspend data decodes, returns `StartPlan::Resume(PlayerState)`. Otherwise it returns `StartPlan::Fresh`, with a warning string when decoding failed.
  - If `initialize` fails, the bridge goes inactive and every later call does nothing.
- **`after_input(api, course, state, effects)`**, after each `Player::handle`:
  - `score {percent, status}`: set `cmi.core.score.min` = `0`, `cmi.core.score.max` = `100`, `cmi.core.score.raw` = `percent`, and `cmi.core.lesson_status` = status, then commit.
  - `completion {status}`: set `cmi.core.lesson_status` = status, then commit.
  - `slide_entered {slide}`: set `cmi.core.lesson_location` = slide id, write suspend data, then commit.
  - Any other effect that changes durable state (`state_changed`, `variable_changed`, `question_answered`): write suspend data without committing.
  - `exit`: run `finish_session`.
  - Statuses map through `coursecraft_scorm::LessonStatus`.
- **Writing suspend data.** Encode it. If the result exceeds 4096 characters, don't write it; return a warning instead. Skip the write when it equals the last value written.
- **`finish_session(api, completed: bool, session_ms: u64)`.**
  - Sets `cmi.core.exit` to `suspend` while not completed, or `""` once passed, failed or completed.
  - Sets `cmi.core.session_time` as `HHHH:MM:SS.SS`.
  - Commits, then finishes.
  - Runs only once.
- Every LMS failure becomes a returned warning string. The app logs it. Nothing panics.
- Tests use a `FakeApi` that records calls and holds a key-value store. They cover: hello-course pass script (exact call sequence for score and status), the fail path, resume round trip, suspend over the cap, an inactive bridge, finishing only once, and session-time formatting.

## 3. Publish (`crates/publish`, L4)

- `pub fn build_package(input: &PackageInput) -> Result<Vec<PackageFile>, PublishError>`, in memory, with no filesystem access.
  - `PackageInput` holds: the course, `course_json`, the player files (path → bytes, from trunk `dist/`), and a media resolver (`&dyn Fn(&str) -> Option<Vec<u8>>`).
  - The result is the full file set: player files, `course.json`, `media/…` for every referenced `src`, and `imsmanifest.xml` from `coursecraft_scorm::manifest_scorm12` listing every file.
  - Mastery score comes from `quiz.passing_percent`.
- `pub fn zip_package(files: &[PackageFile]) -> Result<Vec<u8>, PublishError>` uses the `zip` crate with deflate.
- **Media paths are hostile input.**
  - Reject absolute paths, `..`, backslashes, empty segments, and URL schemes; `http(s)` sources are left as remote and not packaged.
  - A missing media file is an error naming the object.
  - The player rewrites nothing in M2: media is still drawn as placeholders. Packaging it now makes M4 media work only a renderer change.
- The player files must contain `index.html`; otherwise publishing fails.
- CLI: `coursecraft-cli publish COURSE_JSON --player DIST_DIR (-o OUT.zip | --out-dir DIR)`. Media resolves under the course file's `media/` folder (changed in M4.1 to match the spec and the player).
- `apps/coursecraft-player/Trunk.toml` sets `public_url = "./"`, so the build works from any LMS sub-path.

## 4. Player wiring (`apps/coursecraft-player`, wasm only)

- **API discovery** (SCORM 1.2 RTE): walk `window.parent` up to 7 levels looking for an `API` object, then check `window.opener` and its parents. Use `js_sys::Reflect` and `js_sys::Function`; no hand-written JS.
- **`JsCmiApi: CmiApi`** calls `LMSInitialize("")`, `LMSGetValue`, `LMSSetValue`, `LMSCommit("")`, `LMSFinish("")`, `LMSGetLastError` and `LMSGetErrorString`.
- **No API found:** play standalone and log `coursecraft: no SCORM API found; playing standalone` once.
- **Startup:**
  1. Load the course.
  2. `bridge.start`.
  3. On `Resume(state)`, create the player with `Player::resume`. If that fails, fall back to `Player::new` and log a warning.
  4. Send `Start`.
- After each input, call `bridge.after_input` and log its warnings to the console.
- On `pagehide` (and `beforeunload`), call `finish_session`. Completed means the last completion status was passed, failed or completed. Session time comes from `performance.now()`.
- Desktop: no LMS. The bridge isn't compiled into the native path, except for its tests in `coursecraft-lms`.

## 5. Mock LMS and tooling

- **`xtask/mock-lms/index.html`.** This is test tooling. It's the one hand-written JS file in the repo, and it is never shipped.
  - Defines `window.API` with the SCORM 1.2 functions.
  - Stores `cmi.*` in `localStorage`, keyed by course.
  - Logs every call into an on-page table and the console.
  - Shows the current `lesson_status`, `score.raw`, `lesson_location` and `suspend_data` length.
  - Has buttons for "Reload course" (re-mounts the iframe, as a relaunch would) and "Reset learner".
  - Loads `package/index.html` in an iframe.
- **`cargo xtask package-example`.** Runs `trunk build --release` for the player, then `coursecraft-cli publish examples/hello-course/course.json --player apps/coursecraft-player/dist --out-dir target/mock-lms/package -o target/hello-course.zip`, then copies the mock page to `target/mock-lms/index.html`.
- `.claude/launch.json` gets a `mock-lms` static server for `target/mock-lms`.

## Steps (one commit each)

1. Suspend encoding + resume spec (logic).
2. `coursecraft-lms` bridge with fake-API tests.
3. `coursecraft-publish` + CLI `publish` + Trunk `public_url`.
4. Player wasm wiring.
5. Mock LMS + xtask, then verification in the browser: pass (passed / 100), fail path (failed), resume after a mid-course reload (same slide, tabs still visited).
