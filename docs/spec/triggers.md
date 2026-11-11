# Trigger semantics (schema v1)

This is the contract `coursecraft-logic` implements. The editor preview and published packages run the same code, so whatever this file says is what learners get. Where we have not yet checked a rule against what authors expect from established tools, it is marked **[verify]**. Those are parity tasks, not bugs.

## Model

A trigger is: **do** `action` **when** `when` happens, **if** `condition` holds.

Triggers belong to an **owner**: the slide's base layer (`slide.triggers`) or a slide layer (`layer.triggers`). Order inside a list is significant.

## Slide masters

A slide may name a **master** (`slide.master`). At play time the master is drawn under the slide's own objects, before the base layer. Master objects:

- never take a press: they are not clickable and never become a keyboard focus target;
- ignore timing and animation: they are always shown, except an object whose `initial_state` is `hidden`;
- add no events and no states. A master has no triggers, no layers and no question, and validation rejects one that does.

A master's background applies when the slide has none of its own. Masters are authoring and drawing only: `coursecraft-logic` never reaches one, jump targets naming a master are invalid, and a saved suspend state never resumes on one.

## Inputs → events

The renderer sends `Input`s. The runtime turns them into events and queues them:

| Input | Event(s) |
|---|---|
| `start` | enters the start slide → `timeline_starts` (base) |
| `tick {ms}` | advances clocks → `timeline_reaches`, `timeline_ends`, `timer_ends` |
| `click {object}` | `click` (if the object is clickable, see below) |
| `link_clicked {object, link}` | no event; opens a URL or jumps to a slide (see Text links) |
| `hover_start/hover_end {object}` | `hover_start` / `hover_end` |
| `media_ended {object}` | `media_ends` |
| `media_time {object, at_ms}` | `cue_reached` when playback crosses a cue forward (see Interactive video cues); `media_ends` when a trimmed object reaches its trim end (see Media) |
| `key {key}` | `key_pressed` (legacy; the player sends `key_pressed`) |
| `key_pressed {key}` | `key_pressed` (see Key presses) |
| `object_dropped {object, x, y}` | no event by itself; fires `dropped_on` on a hit (see Draggable objects) |
| `focus_changed {object?}` | no event; tells the runtime which object has keyboard focus |
| `submit` | `submit` |
| `select_choice {choice}` | no event; updates the selection directly |
| `text_entered {text}` | no event; replaces fill-in text (see Questions) |
| `sequence_moved {choice, to}` | no event; reorders a sequence item (see Questions) |
| `match_selected {left, right}` | no event; sets or clears a match (see Questions) |
| `hotspot_clicked {x, y}` | no event; places a hotspot marker (see Questions) |
| `drag_dropped {item, target?, x, y}` | no event; moves a drag item (see Questions) |
| `set_variable {variable, value}` | no event; writes the variable through the normal assignment path, so `variable_changes` fires (see Run JavaScript) |
| `set_input_value {object, value}` | no event; commits an input object's change through the normal assignment path, so `variable_changes` fires (see Input objects) |
| `marker_toggled {object}` | `marker_opened` or `marker_closed` (see Markers) |
| `markers_closed` | `marker_closed` when a marker was open (see Markers) |
| `panel_scrolled {object, offset}` | no event; clamps and stores the panel offset (see Scrolling panels) |
| `close_lightbox` | closes the open lightbox (see Lightboxes) |
| `jump {slide}` | no event; jumps to the slide id (see Run JavaScript) |
| `next` / `previous` / `go_to` | player chrome navigation (when enabled in `player`) |
| `player_pause` / `player_resume` | no event; the learner's pause flag (see Learner pause, resume and seek) |
| `player_seek {ms}` | no event; moves the clock of the timeline the seekbar targets (see Learner pause, resume and seek) |

**`submit` is for scripts.** No player control produces `Input::Submit`: the published player, the editor preview and the renderer never send it. A `submit_question` trigger on the `submit` event therefore never fires for a learner. Add a button with a `click` trigger instead; the editor warns when a question's only `submit_question` triggers use `submit`. Scripts and automation may still send `Input::Submit` deliberately.

## Processing order

1. Events are processed **first in, first out**. Events caused by actions (`variable_changes`, `state_changes`, `timeline_starts` of a layer that was shown) go to the back of the queue. They run after the current event's triggers finish (breadth-first).
2. For one event, the runtime **snapshots** the list of owners before any action runs: the base layer first, then visible layers bottom to top. A layer shown by this event does not respond to the same event. A layer hidden by this event still finishes its triggers for it. **[verify]**
3. Within an owner, triggers run in list order. Conditions are evaluated at the moment the trigger is reached, so earlier actions in the same event are visible to later conditions.
4. **Disabled triggers.** A trigger with `disabled: true` never fires and never evaluates its condition. It is not a listener for pointer cursors, key presses or `timeline_reaches` points. Clearing the flag restores it exactly.
5. **Jumping to a slide ends the event.** Remaining triggers are skipped and every queued event from the old slide is dropped. The runtime tracks this with an epoch counter on every queued event.
6. **Loop guard.** At most 1024 events are processed per input. Beyond that the queue is cleared and a `warning` effect is emitted. The course keeps running.

## Clicks

An object is clickable when all of these hold:

- It is on the base layer or on a **visible** layer.
- Its layer's clock is inside its `timing` window: `start_ms <= t < end_ms`.
- Its state is not `hidden`.
- Its state is not `disabled`. Disabled objects swallow the click silently: no warning, no triggers.
- It is not a base-layer object while a visible layer has `prevent_base_clicks`.
- It is not a base-layer object while a visible layer has `hide_base_objects`. (Same for keyboard focus and the accessibility tree; base media and triggers carry on.)

### Built-in states after a click

These run **after** the click's triggers, and only if those triggers left the object's state unchanged and stayed on the slide. **[verify]**

- Object defines `selected` → toggles `normal` ⇄ `selected`.
- Else object defines `visited` → becomes `visited`.

Built-in changes fire `state_changes` like any other state change.

`hover` and `down` are presentational. The renderer applies them, and they are not stored in runtime state.

## Text links

A run may carry a URL or a slide link. The renderer draws it underlined in the theme accent (unless the run sets its own colour) and reports a press on it as `Input::LinkClicked { object, link }`. The runtime applies the same reachability rules as a click (visible layer, timing window, `hidden`/`disabled`, `prevent_base_clicks`, `hide_base_objects`). A link on a disabled object or under a preventing layer is ignored silently.

`link_clicked` does **not** fire the object's `click` event, built-in selected/visited states or any other trigger. A URL link emits `Effect::OpenUrl`; a slide link performs the same jump as `jump_to_slide`. Links join the keyboard focus order immediately after their object and are announced as links in the access tree. An invalid URL or a slide that no longer exists emits a `warning` and changes nothing.

## Timelines

- Each slide has a base clock. Each visible layer has its own clock, which starts at 0 when the layer is shown.
- `tick` advances every running clock by `ms`, clamped to that timeline's duration.
- `timeline_reaches {ms}` fires when a tick crosses `ms` (`old < ms <= new`). Use `timeline_starts` for 0.
- `timeline_ends` fires once, when the clock reaches the duration.
- `pause_timeline` / `resume_timeline` act on the **owner's** timeline. **[verify]**
- A visible layer with `pause_base_timeline` holds the base clock.
- Audio and video with `autoplay` emit `media {play}` when their `start_ms` is reached (or on entry, for 0).

## Media

- **Playback control and effects:** The runtime communicates media operations via `Effect::Media { object, op: Play | Pause | Stop }`. Triggers control media using actions `play_media { object }`, `pause_media { object }`, and `stop_media { object }`. Audio, video and Lottie objects configured with `autoplay: true` emit `Media { Play }` when their timeline reaches `timing.start_ms` (or on slide/layer entry for 0 ms). A Lottie object behaves exactly like audio and video for these actions.
- **Tracking playback state:** The runtime tracks which media objects are playing or paused in `PlayerState`. This state is purely runtime-only and is not stored in durable SCORM `suspend_data` (nor encoded in `crates/logic/src/suspend.rs`). Lottie playback position is also runtime-only, in `PlayerState.media_clocks`; `tick` advances it while the object is `Playing`, by `ms` (the renderer applies `speed`). Pause keeps the position; stop forgets it.
- **Slide exit:** When exiting a slide, every playing or paused media object receives `Media { Stop }`, emitted strictly before the `slide_entered` effect of the newly entered slide.
- **Layer hidden:** When a layer is hidden (either explicitly via `hide_layer` or implicitly by showing another layer with `hide_other_layers`), every media object on that layer receives `Media { Stop }` and is marked stopped.
- **Timeline pausing and resuming:** When the base timeline pauses (either because a visible layer with `pause_base_timeline` is shown, or because an author trigger executes `pause_timeline` on the base slide), all currently playing base-layer media objects receive `Media { Pause }` and are tracked as paused by timeline. When the base timeline resumes (when all `pause_base_timeline` layers are hidden and base timeline is not paused), media objects that were paused by that timeline pause receive `Media { Play }` and resume. Media objects that the author explicitly paused with `pause_media` remain paused. Layer media objects are tied to their own layer's timeline and are not paused by base timeline pauses.
- **Layer timelines:** A layer's own `pause_timeline` action pauses only that layer's clock; it does not pause the layer's media (current behaviour, kept as is).
- **Object ids and lookup:** Object ids are unique per slide only, so a `media` effect is resolved against the current slide (base objects and its layers' objects), never against the rest of the course. The one exception is the reserved background-audio id `Course.BackgroundAudio`, which resolves to the course's `background_audio`.
- **Pause while fetching:** If a `Media { Pause }` or `Media { Stop }` arrives for an object whose bytes are still being fetched, the host drops the pending play; it does not start when the bytes arrive (the runtime re-sends `Media { Play }` on resume).
- **Web video layering (known M4 limitation):** On the web a video is an HTML element drawn above the canvas, so slide layers and overlays shown on top of a playing video's area are covered by it. Videos are hidden when their object is not on screen and while a slide transition runs.
- **Web object layering:** The published web player follows the same rule for a `web` object's `<iframe>`; it is hidden when the object is not on screen and while a slide transition runs. The iframe ignores the object's rotation and flips, while the frame itself follows scroll, zoom and letterboxing. Other hosts draw a placeholder instead.
- **Media ends:** When audio or video finishes, the host sends `Input::MediaEnded { object }`. For a non-looping Lottie animation the renderer does the same when the media clock reaches the composition's last frame; a looping animation never sends it. If received for an object that is not currently playing (e.g. stale event after slide transition, or an already stopped/paused object), the input is ignored with an `Effect::Warning`. Otherwise, the runtime marks the object stopped and fires triggers matching the `media_ends { object }` event. A finished Lottie keeps its last clock position so the renderer can hold the final frame.
- **Invalid targets:** `play_media`, `pause_media`, and `stop_media` actions targeting a missing object or an object that is not audio, video or Lottie emit an `Effect::Warning` and emit no `Media` effect.
- **Failure resilience:** If playback fails to start (e.g. corrupt or unsupported audio format, no audio device available on desktop, missing media bytes, or browser autoplay policy rejection without prior user gesture), the backend logs a warning and reports the media as "ended" right away so subsequent `media_ends` triggers execute and courses do not get stuck.
- **Player volume:** `MediaBackend::set_volume` takes `0.0`–`1.0`. NaN becomes `0.0` and an infinity is clamped to the nearest end, so a hostile value can never reach the audio graph or the DOM. The value applies to all current and later media. Volume is a player setting: not course content and not part of `suspend_data`.
- **Seeking media:** `MediaBackend::seek(object, ms)` moves current media to `ms`. A seek that arrives while the bytes are still being fetched is remembered and applied once that play starts. Seeking past the end ends the media normally, so the host reports `media_ends`; `position_ms` always reports the real position. For an autoplay Lottie object, `player_seek` sets `media_clocks[object]` to `position − start_ms` instead, and a backward seek rewinds it.
- **Trim:** A trimmed audio or video object starts at `trim.start_ms`, so the `MediaSeek` the runtime emits after `Media { Play }` carries `start_ms +` the resume position (the slide clock minus `timing.start_ms`), clamped to `trim.end_ms`. An untrimmed object keeps its existing seek position. Hosts report `media_time` for every playing audio or video object that has a trim end, as they do for videos with cues. When `at_ms` reaches `end_ms` the runtime marks the object stopped, emits `Media { Stop }`, and fires `media_ends { object }` exactly as a natural `media_ended` would; a later `media_ended` for it is ignored with the usual warning. A report before the end changes nothing, and an audio object without a trim end is ignored. Cue and trim times stay in file time.
- **Trim on `player_seek`:** For an autoplay trimmed object, `player_seek` maps the slide clock to `start_ms + (position − timing.start_ms)` and clamps the seek to `end_ms`, for the same tracked and untracked cases as an untrimmed object.

### Background audio

A course may set `background_audio` (see [course-model.md](course-model.md#background-audio)). The runtime plays it as the reserved media object `Course.BackgroundAudio`, whose id cannot collide with a user object id.

- **Starting.** On `start`, including after a resume, the runtime emits `Media { Play }` for `Course.BackgroundAudio`, marks it playing and records that its start is pending. Any `tick` clears the pending flag. The host resolves the id to the course's `src` and `volume`.
- **Across slides and timelines.** It keeps playing across slide entry and exit, layer show and hide, and timeline pause and resume; none of those emit `Media` effects for it. `exit_course` stops it: the runtime emits `Media { Stop }` before `Effect::Exit`.
- **Author control.** `play_media`, `pause_media` and `stop_media` accept the reserved id when the course has background audio. They emit the matching `Media` effect and track `playing` / `paused_by_author`: play starts it again, pause marks it paused by the author, and stop clears both. A later play starts it again.
- **Ending.** On `Input::MediaEnded { object: "Course.BackgroundAudio" }` while it is playing, the runtime fires the `media_ends { object: "Course.BackgroundAudio" }` triggers, then: if `loop` is set and the start was not still pending, it emits `Media { Play }` again; if the start was still pending (the host reported the end before the first tick, as a browser autoplay refusal does), it marks the track stopped, emits no play and one `warning` "background audio could not start"; if `loop` is false, it marks it stopped. When it is not playing, a `media_ended` is ignored with the usual stale warning.
- **State.** Playback state lives in `PlayerState.background`, is runtime-only and is never written to `suspend_data`. `is_media_playing("Course.BackgroundAudio")` reports whether it is playing.

## Interactive video cues

A `video` object may declare cue points (`at_ms`, `name`, `pause`; see [course-model.md](course-model.md#interactive-video-cues)). Hosts report the video's playback position, in milliseconds from the start of the clip, as `Input::MediaTime { object, at_ms }`. The runtime is the only place that decides when a cue fires. Cue times stay in file time and do not shift when the video is trimmed.

1. **Crossing.** A cue fires when the reported position moves forward across it (`old < at_ms <= new`). Several cues crossed by one report fire in time order. A cue fires at most once per pass: the runtime records it in runtime-only `cue_progress`.
2. **Backward seeking.** A report that moves backward re-arms every cue at or after the new position and clears any held cue. No event fires for the backward movement itself.
3. **Forward seeking.** `player_seek` moves the cue progress with the timeline clock, marks the cues behind the new position as already fired, and fires none of them. A video started by a trigger, not autoplay, keeps its own backend position but its cue markers still follow the seek.
4. **Pausing cues.** When a cue with `pause: true` fires, the runtime emits `Effect::CueReached { object, cue }`, then `Effect::MediaSeek { object, ms: cue }` and `Effect::Media { Pause }`, and marks the video `PausedAtCue`. Cues later in the same report are not fired until playback resumes. The host seeks the media element back to the cue's exact time.
5. **Resuming.** `play_media { object }` on a held video seeks to the cue and plays again without re-firing that cue. The learner's `player_resume` also resumes it, unless the cue's triggers opened a layer that is still visible: that layer keeps control. A `pause_base_timeline` layer keeps the usual author hold.
6. **Replay.** Entering or replaying a slide re-arms all cues on it. A stop or `media_ends` clears a video's cue progress. Cue progress and the held-cue state are runtime-only and are never written to `suspend_data`; `Player::resume` keeps them for the same session, so a course resumed after a relaunch starts a fresh pass.
7. **Hosts.** The published web player reports the HTML video element's `currentTime` every frame while it plays and seeks it back on a pausing cue. Desktop preview and the desktop player play real video via the OS decoder and report the decoder position (the audio track's position when there is audio); when decoding fails they log once and fall back to a simulated media clock for cue-bearing videos, using the same input, so cues can still be tested. Offscreen renders never report media time and ignore cues.
8. **Effects are the audit trail.** Every crossing emits `Effect::CueReached`, so `simulate` prints it like any other effect. An object that is not an audio or video object with a trim end or cues, a cue that is not declared on it, or a non-finite position emits `Effect::Warning` and changes nothing.

## Run JavaScript

`run_javascript { code }` is a power-user action. The code is at most 64 KiB; validation warns "Uses JavaScript, which runs only in the published course". The action always emits `Effect::RunJavaScript { code }`; what a host does with it differs:

- **Published web player:** runs the code in the player page (not inside any web object's iframe) with a helper object named `CourseCraft`:
  - `CourseCraft.getVar(name)` returns the current value (`true`/`false`, a number or text), or `undefined` when the variable does not exist.
  - `CourseCraft.setVar(name, value)` writes a variable. The value must match the variable's kind; a wrong type or unknown name is ignored with a console warning. A write goes back into the runtime as `Input::SetVariable`, so `variable_changes` triggers fire exactly as they would for a `set_variable` action.
  - `CourseCraft.jump(slideId)` goes back into the runtime as `Input::Jump`, which performs the same jump as `jump_to_slide { id }`.
  - The code is wrapped so an exception is caught and logged to the console; the learner keeps playing.
  - A syntax error is caught the same way. The helper works only while the code runs: calls made later (from `setTimeout`, promises or event handlers) do nothing.
- **Desktop player:** ignores the action and logs "JavaScript runs in the published course".
- **Editor preview:** ignores the action and adds "JavaScript runs in the published course" to its effect log.
- **`simulate`:** prints the effect as JSON like any other effect.

JavaScript is not part of `PlayerState` or `suspend_data`. It runs only in the published web player, never in preview, offscreen renders or the desktop player.

## Input objects

`text_entry`, `slider`, `dial`, `checkbox` and `radio_group` objects (see [course-model.md](course-model.md#input-objects)) show a value read from their variable on every frame. A change the learner makes goes to the runtime as `Input::SetInputValue { object, value }`. The runtime does not trust the host:

1. The object must be on the current slide, on screen (visible layer, timing window, not `hidden`), and be an input object. A missing object or a non-input emits a `warning`.
2. A `disabled` input ignores the input silently and changes nothing.
3. The value's kind must match the widget: text for text entry and radio, number for slider and dial, true/false for checkbox. A mismatch emits a `warning`.
4. Text is truncated to `max_chars` characters (on a character boundary) when `max_chars` is non-zero. Slider and dial numbers are clamped to `min..=max` and snapped to `step` measured from `min` (step 0 is continuous). A radio value that is not one of the option values emits a `warning` and changes nothing.
5. The accepted value is assigned to the input's variable through the same path as `set_variable`, so `variable_changes` fires only when the value actually changes. Nothing else fires: there is no `click` and no new effect.

The host commits at these moments:

- **Text entry:** Enter on a single-line field, or when focus leaves the field (including multiline). Typing alone does not commit.
- **Slider and dial:** continuously while dragging, each time the snapped value changes, and on pointer press. Arrow keys move one step (Shift ten); Home and End jump to `min` and `max`. A vertical slider has `min` at the bottom. A dial's angle is measured clockwise from 12 o'clock between `start_angle` and `end_angle`.
- **Checkbox:** a click, or Space/Enter while focused.
- **Radio:** a click on an option, or Left/Up and Right/Down while the group is focused.

Because the displayed value is read from the variable, a `set_variable` or `set_input_value` on the same variable moves the widget on the next frame, on the canvas (with the variable's default), in preview, in the player and in offscreen renders.

## Key presses

`Input::KeyPressed { key }` fires the event `key_pressed {key, object?}`. `key` is one of `a`–`z`, `0`–`9`, `arrow_up`, `arrow_down`, `arrow_left`, `arrow_right`, `enter`, `space` or `escape`. Validation rejects anything else, including uppercase names; matching is case-insensitive at run time.

- Without `object`, the trigger fires anywhere on the current slide. It never fires while a text entry or a fill-in field is taking the learner's typing.
- With `object`, it fires only while that object has keyboard focus. The renderer reports focus with `Input::FocusChanged { object }` and still lets focus-free triggers fire on the same key.
- A focused control owns the keys it already uses, and those keys do not also fire a trigger: Space/Enter on a button, checkbox, marker or draggable object; arrows on a slider, dial, radio group, scroll panel, hotspot or player-chrome slider; Tab for focus movement. Previous/Next navigation owns an arrow only when it actually changes slides. Other keys still reach key triggers.
- The editor preview closes on Escape unless the current slide has an `escape` key trigger; a trigger that uses Escape claims it first.
- While a lightbox is open the runtime is on the box slide, so only the box slide's triggers fire. Escape closes the box first.
- Key presses are not stored in `PlayerState` or `suspend_data`.

## Draggable objects

Any non-question object may set `drag: {targets, snap, return_on_miss}` (see [course-model.md](course-model.md#draggable-objects)). `targets` lists object ids on the same slide, `snap` defaults true and `return_on_miss` defaults true. An empty target list means every drop is a miss. Validation rejects unknown targets and a drag spec on a question widget.

The renderer sends `Input::ObjectDropped { object, x, y }` on release, with `x` and `y` the object's new top-left in slide coordinates. A press that moves less than 3 px is an ordinary click and sends no drop. While moving, the object follows the pointer above everything else, clamped inside the slide.

The runtime does not trust the host:

1. The object must be on the current slide, live (visible layer, timing window, not `hidden`), and have a `drag` spec. Otherwise `warning` and nothing changes.
2. Non-finite coordinates emit `warning` and nothing changes. Finite coordinates are clamped so the object stays inside the slide.
3. The first id in `targets` whose current frame contains the dropped object's centre is the hit. A hidden or absent target is skipped.
4. On a hit the object's offset is stored in runtime-only `object_positions`; with `snap` it is centred on the target, otherwise it stays where released. It emits `Effect::ObjectMoved { object, offset }`, then `Effect::ObjectDropped { object, target }`, then `dropped_on {object, target}` for triggers.
5. On a miss, `return_on_miss` leaves the object at the position it had before the drag (which may itself be an earlier drop); otherwise it stays where released. Either way it emits `ObjectMoved` and no `dropped_on`.

`object_positions` resets on slide entry, is restored when a lightbox closes and is never written to `suspend_data`. Composition and hit testing add that offset, on top of motion paths and entrance/exit animation. The inspector's **Draggable** switch edits the spec through `object.set_drag`; the trigger form's `dropped_on` pickers list draggable objects and their targets.

Keyboard follows drag-and-drop questions: on a focused draggable object, Space or Enter picks it up, arrows (or Tab/Shift+Tab) cycle through its targets, Space or Enter drops it, and Escape cancels and sends nothing. The web access layer announces "draggable" and uses the existing live region for picked up, over target and dropped.

## Object intersections

`objects_intersect {a, b}` fires when two objects' frames start to overlap; `objects_stop_intersecting {a, b}` fires when they stop. Trigger matching treats `(a, b)` and `(b, a)` as the same pair.

- Frames are computed by the runtime after every clock tick, motion-path move, state change and drag drop, using each object's current position including motion-path and drag offsets.
- Objects start "not intersecting" on slide entry, so a pair that is already overlapping fires `objects_intersect` once at slide start. Events fire once per transition, not every frame.
- Hidden objects never intersect. An object is hidden when its state is `hidden`, when it is outside its timing window or exit animation, when it is on a hidden layer, or when a visible layer has `hide_base_objects`.
- Intersection pairs are runtime-only, reset on slide entry and never written to `suspend_data`.

## Timers

A `timer` counts down from `duration_ms` in `coursecraft-logic` while its owning timeline runs (see [course-model.md](course-model.md#timers-and-progress-meters)). Actions `start_timer { timer }`, `pause_timer { timer }` and `reset_timer { timer }` control it; validation requires the named object to be a timer on the same slide (base layer or any layer).

1. **Slide entry resets every timer on the slide.** Each timer returns to its full duration, stopped, with autostart armed. Leaving the slide and coming back resets them again.
2. **Autostart begins when the owning clock reaches the object's start time.** A base-layer timer follows the base clock; a layer timer follows its layer's clock. Only the part of the tick after the start time counts. A timer with `autostart: false` waits for `start_timer`.
3. **A running timer loses the distance its owning clock moved this tick.** It stops while the player's pause is on, while its base timeline is held (an author `pause_timeline` or a visible `pause_base_timeline` layer), and while a lightbox freezes the base slide. A timer on a hidden layer does not run. Resuming continues from the remaining time.
4. **`timer_ends { timer }` fires exactly once when the countdown reaches zero**, including when one large tick jumps past zero. The final tick clamps the remaining time at zero.
5. **`start_timer`** starts or resumes a timer and emits `Effect::TimerStarted`. On an ended timer it does nothing until the timer is reset.
6. **`pause_timer`** stops a running timer and emits `Effect::TimerPaused`. Pausing an already stopped timer emits nothing.
7. **`reset_timer`** returns the timer to full duration, stopped and un-ended, and emits `Effect::TimerReset`. A reset timer does not autostart again; only `start_timer` restarts it.
8. **Effects are the audit trail:** `timer_started`, `timer_paused`, `timer_ended` and `timer_reset` are effects, so `simulate` prints them like any other effect. Clock ticks in `simulate` drive timers the same way the player does.
9. **Timers are runtime-only.** They are never written to `suspend_data`; resuming always starts them full.

### The `Timer.<id>.Remaining` variable

Each timer exposes a built-in-style read-only variable named `Timer.<object id>.Remaining` holding the whole seconds remaining, rounded up (a fresh 5000 ms timer reads 5 until a whole second elapses).

- It resolves everywhere a built-in variable does: `%Timer.t1.Remaining%` in text references and `compare` conditions. Validation rejects writes to it (`set_variable`, and the editor's set-variable action), exactly like the built-ins.
- It updates each tick without firing `variable_changes` on every tick: the `variable_changes` event for that name fires once per whole-second change, so a trigger listening for it runs once per second, not once per tick. No `VariableChanged` effect is emitted for it.
- `reset_timer` jumps the value back to the full duration; that jump also fires one `variable_changes` when the whole-second value changes.

## Progress meters

A `progress_meter` draws a number variable's current value between `min` and `max` in every target through one code path (see [course-model.md](course-model.md#timers-and-progress-meters)). It is not an input: the learner cannot change it, `set_input_value` on it warns like any non-input, and it never enters the keyboard focus order on its own. Values outside the range clamp; the displayed value is read from the variable every frame, so a `set_variable` action moves it on the next frame. In the access tree it is `role="progressbar"` with `aria-valuemin`, `aria-valuemax` and `aria-valuenow` from the meter's range and the variable's current value.

## Markers

A `marker` opens a small label card. The renderer sends `marker_toggled {object}` when the dot is clicked or activated by keyboard.

1. The object must be on the current slide, on screen (visible layer, timing window, not `hidden`), and be a marker. A missing object or a non-marker emits a `warning`.
2. A `disabled` marker ignores the toggle silently.
3. If this marker is open, it closes. Otherwise any other open marker closes first, then this one opens.
4. Opening emits `Effect::MarkerOpened {object}` and the `marker_opened {object}` event. Closing emits `Effect::MarkerClosed {object}` and `marker_closed {object}`. Triggers for both events run normally.
5. Only one marker on a slide is open at a time. The open marker is player state, is reset on slide entry and is never written to `suspend_data`.
6. Clicking any other object closes an open marker before that object's own click triggers run. Background clicks and Escape send `markers_closed`, which closes it if open and does nothing otherwise.

The renderer draws the label card above other objects. The card is not a hit target; the dot remains the marker's target.

## Scrolling panels

A `scroll_panel` clips and moves the objects whose `panel` names it (see [course-model.md](course-model.md#scrolling-panels)).

- `panel_scrolled {object, offset}` is clamped to `0..=content_height - frame.height`. Non-finite offsets become 0. A non-finite or negative content height is treated as 0. The offset is player state, resets on slide entry and is never written to `suspend_data`.
- `Player::panel_offset(object)` is the current offset. Children are drawn and hit-tested moved up by it, and clipped to the panel's frame. A child wholly outside the viewport is not hit-testable, not in the keyboard focus order and not announced.
- The renderer sends the input for a mouse wheel over the panel, dragging the scrollbar thumb, dragging the content, and PageUp/PageDown/Home/End/arrows while the panel has keyboard focus. Arrow keys scroll 40 slide pixels, PageUp/PageDown about one viewport, and Home/End jump to the ends.
- When keyboard focus lands on a partly hidden child, the renderer scrolls the panel just enough to show it.
- Offscreen renders always draw at offset 0.

## Lightboxes

`lightbox_slide { slide }` shows another slide over the current one. `close_lightbox` closes it. The renderer sends `Input::CloseLightbox` for the × button and Escape; a `key_pressed` input of `escape` closes an open lightbox too.

1. The target slide must exist in play order and must not be the slide the trigger belongs to. Validation rejects both statically; at run time a missing slide or the current slide produces `Effect::Warning`.
2. Opening saves the current slide's runtime (clock, visible layers, layer clocks, media state, motion paths and offsets, selections and panel offsets), pauses its playing media with `Media { Pause }`, and emits `Effect::LightboxOpened { slide }`. The base clock does not advance and its `timeline_starts` does not fire again on close.
3. The lightboxed slide enters from its start: its base clock is 0, its layers are hidden, its own triggers run, its `timeline_starts` fires, and autoplay media start. Variables, answers and visited slides are shared. The slide is marked visited.
4. Only one lightbox is open at a time. `lightbox_slide` while one is open stops the old box's media, emits `LightboxClosed` for it, then opens the new slide over the same paused base.
5. While a lightbox is open, [`Player::current_slide`] is the slide inside the box. The base slide takes no clicks, hover or keyboard focus, and the player's Next/Previous controls are disabled (`Effect::Warning`).
6. `close_lightbox`, `Input::CloseLightbox` or Escape stops the box slide's media, emits `Effect::LightboxClosed { slide }`, and restores the base runtime exactly. A base media object resumes with `Media { Play }` only when it was playing when the box opened; one the author had paused stays paused. No `slide_entered` or `timeline_starts` effect is emitted for the base.
7. `jump_to_slide`, a slide text link or `Input::Jump` inside a lightbox closes it and performs the jump on the base slide.
8. The box is runtime-only: it is never written to `suspend_data`. Resuming always enters the saved base slide.

The renderer dims the base with black at 55% (fading in unless reduced motion), scales the box slide to 85% of the slide rect, centres it with a soft border, and draws a × close button at its top-right. Inside the box every slide coordinate goes through the box transform, so hit testing, drag, inputs, scrolling panels and markers map correctly. Web video and web object elements hide while a lightbox is open. The backdrop takes no press: only the × button or a `close_lightbox` trigger closes. Focus moves into the box and is trapped there, including the close button, and returns to the object that opened it when the box closes. On the web the box is a `role="dialog"` container with `aria-modal="true"`, labelled by the slide's title.

## Learner pause, resume and seek

These are the learner's controls. They never change the author's `pause_timeline`, `pause_media` or `pause_base_timeline`.

**`player_pause` / `player_resume`** set a runtime-only player-paused flag in `PlayerState`. It is runtime-only and kept out of `suspend_data`, like `media_playback`.

- While the flag is set, `tick` advances no clock, no motion path and no timer: `timeline_reaches`, `timeline_ends`, animation completions and autoplay crossings all stop. Other inputs — clicks, keys, questions, navigation and their triggers — still run.
- `player_pause` gives every media object tracked as `Playing` (base layer or any visible layer) `Media { Pause }` and marks it `PausedByPlayer`. `player_resume` gives exactly those `Media { Play }` and marks them `Playing`. Objects in `Paused` (author `pause_media`) or `PausedByTimeline` are not touched.
- If the author's base timeline is still held when the player resumes (`pause_timeline` on the base, or a visible `pause_base_timeline` layer), a **base-layer** object resumed by the player becomes `PausedByTimeline` again at once and gets no `Play`. The author's hold wins.
- Media started by a trigger while the player is paused (`play_media`, or autoplay on a layer shown during the pause) is tracked as `PausedByPlayer` and emits no `Play` effect; it starts from 0 when the player resumes.
- Entering another slide clears the flag before that slide's autoplay, so the next slide plays.
- Both inputs are idempotent: repeating the current state changes and emits nothing.

**`player_seek { ms }`** moves the clock of one timeline, chosen by the visible layers. It is safe for any `ms`: no panic and no overflow.

The target is chosen by taking visible layers **front to back** (the topmost first):

1. A layer with `allow_seek: no` refuses the seek: `Effect::Warning`, nothing moves, and `seek_info` reports seeking not allowed.
2. A layer with `allow_seek: yes`, or with `auto` and `pause_base_timeline` on, is the target. Its clock moves, clamped to `0..=layer.duration_ms`, with the forward and back rules below. Its autoplay media follows the same rules as base media.
3. Otherwise the target is the base clock, clamped to `0..=slide.duration_ms`, as before. A visible `pause_base_timeline` layer with `auto` now takes rule 2, so the old refusal only remains for a held base clock that still ends up as the target: a seek of a held base clock is refused with `Effect::Warning` and nothing moves.

`auto` on a layer without `pause_base_timeline` leaves the target as the base, as today. An author `pause_timeline` on the base does not refuse a seek: the clock moves and stays paused.

- **Forward:** every cue crossed fires in time order with the same rules as one `tick` of that length: `timeline_reaches`, entrance/animation completions, autoplay at `timing.start_ms` (with `Media { Play }` and state `Playing`), and `timeline_ends` once on reaching the duration. For a layer target, a layer with `hide_when_timeline_ends` is then hidden after those triggers. The event queue rules apply, including a slide jump during the seek dropping the rest of the old slide's events.
- **Back:** the clock moves and is no longer "ended", so cues after it fire again when later crossed and `timeline_ends` can fire again. No event fires for the backward movement itself. Variables, states, visible layers, visited slides, motion paths and offsets are not undone.
- **Media:** after any `Play`, the runtime handles each audio or video object with `autoplay: true` on the target timeline:
  - If its `timing.start_ms` is after the new position, or it is outside its timing window there, it receives `Media { Stop }` if it was tracked and is forgotten.
  - If it starts at or before the position and is on the timeline, it receives `Effect::MediaSeek { object, ms: position - start_ms }`. A tracked paused object (`Paused`, `PausedByTimeline`, `PausedByPlayer`) gets only `MediaSeek`; a stopped object gets `Media { Play }` then `MediaSeek`; while the player is paused an untracked object becomes `PausedByPlayer` and gets only `MediaSeek`.
  - Media started by a trigger (not by `autoplay`) is never moved or stopped by a seek.
- Object visibility follows the new clock on the next frame, including exit animations, because composition samples the clock.
- `Player::seek_info()` is the read-only accessor for the UI. When a layer is the target it reports that layer's elapsed ms and duration; otherwise it reports the base clock and slide duration. It also reports whether the player is paused and whether seeking is allowed (false for a `no` layer or a held base target).

## Layers

- `show_layer` on a visible layer does nothing. Otherwise, if the layer has `hide_other_layers` (default true), every other visible layer is hidden first. The layer's clock is chosen by `revisit` (see below) and `timeline_starts {layer}` is queued only when that clock is 0.
- `slide.layers` runs back to front and `visible_layers` stays in that authored order, not the order layers were shown. A new visible id is inserted at its `slide.layers` position. Resuming a saved state reorders `visible_layers` the same way and drops nothing.
- `hide_layer` with no `layer` hides the owner. On the base layer that is a validation error.
- **`hide_base_objects`.** While any visible layer has it, base-layer objects are not drawn, take no clicks, and leave keyboard focus and the accessibility tree. Master items stay. The base clock, base triggers and base media carry on (`pause_base_timeline` still governs pausing). `Player::base_objects_hidden()` is the read-only accessor; composition, focus, access and hit testing all read it. Questions on the base are treated as base objects for drawing. **[verify]**
- **`hide_when_timeline_ends`.** When the layer's clock reaches its duration, `timeline_ends` for that layer fires first and its triggers run. Then the layer is hidden by the same path as a `hide_layer` action, including base media resume. If a trigger already hid it, nothing more happens. A layer whose `duration_ms` is 0 is not auto-hidden. **[verify]**
- **Layer revisit.** Applies when a layer is shown again within the same slide visit, or on a later visit:
  - `auto`: today's behaviour. The clock restarts at 0 and object states are kept.
  - `reset`: the clock restarts at 0 and the layer's object states return to `initial_state`.
  - `resume`: the clock continues from where it was when the layer was last hidden. Object states are kept. Media that was playing when hidden does not auto-restart unless its timing window is crossed again: autoplay runs only for cues after the resumed position. `timeline_starts` fires only if the clock is at 0. A layer whose saved clock had ended stays ended and never re-fires `timeline_ends`, unless it has `hide_when_timeline_ends`: that one starts over from 0, or it would stay on screen for good. **[verify]**
- Saved layer clocks are runtime-only, kept per layer id, and never encoded in `suspend_data`.

## Slides

On entry the runtime does the following:

1. Bumps the epoch.
2. Resets the base clock (or restores it for `revisit: resume`, below).
3. Hides all layers (or restores them for `revisit: resume`).
4. Clears the choice selection.
5. Marks the slide visited.
6. Initialises object states, on the **first visit only**: later visits resume the saved state, which is what authors expect. **[verify]**
7. Emits `slide_entered`.
8. Queues `timeline_starts` when the base clock is 0.

**Slide revisit** applies on entering a slide already visited:

- `auto`: today's behaviour (the steps above).
- `reset`: as `auto`, plus every object state on the slide and its layers returns to `initial_state`, and saved layer clocks for that slide are forgotten.
- `resume`: object states are kept, the base clock restarts from where the learner left it, and layers that were visible when they left are shown again with their saved clocks. `timeline_starts` is queued only if the clock is 0. On leaving a slide, the runtime saves its clock and visible layers (runtime-only, never in `suspend_data`). `Player::resume` after a relaunch still follows the existing rule above. **[verify]**

Variables, answers and visited slides are never reset by any of these.

`jump_to_slide next` on the last slide and `previous` on the first slide do nothing.

## Variables

- Types are fixed by the initial value: `bool`, `number` or `text`.
- `assign` needs the same type. `toggle` is bool only. `add`, `subtract`, `multiply` and `divide` are number only. Divide by zero and non-finite results are refused with a `warning`.
- `variable_changes` fires only when the value actually changes.
- Number equality is exact `f64` equality. Keep counters as whole numbers.
- Built-in variables never fire `variable_changes`. The one exception is `Timer.<id>.Remaining`, which fires once per whole-second change (see [Timers](#timers)).

## Built-in variables

The runtime provides read-only built-in variables computed on read from `PlayerState`. They are never stored in `PlayerState.variables` and never serialized into SCORM `suspend_data`.

| Name | Type | Value |
|---|---|---|
| `Course.SlideNumber` | number | 1-based play-order index of the current slide (0 before `start`) |
| `Course.TotalSlides` | number | number of slides in play order |
| `Course.Progress` | number | `floor(visited slides * 100 / total slides)`, 0–100 (0 when there are no slides) |
| `Course.ScorePoints` | number | sum of points of graded answers so far (same source as `results`) |
| `Course.ScorePercent` | number | `floor(ScorePoints * 100 / max points)` clamped to 100; 100 when the course has no questions (same rule as `results`) |
| `Quiz.Passed` | true/false | true when the course has `quiz.passing_percent` and the current percent (`floor(raw * 100 / max)`, 100 when there are no questions) is at or above it; false when the course has no `quiz` |
| `Quiz.Attempts` | number | how many times `submit_results` has run this session |
| `Quiz.Reviewing` | true/false | true while answer review is showing (see Retry and answer review) |
| `Slide.ElapsedMs` | number | the base timeline clock of the current slide in ms |
| `Learner.Name` | text | the learner's name set by the host; empty string when unknown |
| `Course.Title` | text | the course's `title` |
| `Course.Date` | text | the host's date string (`YYYY-MM-DD`); empty string until the host sets it |
| `Timer.<id>.Remaining` | number | whole seconds remaining on that timer, rounded up (see [Timers](#timers)) |

- **Naming and collision:** Built-in names contain a dot (`.`), whereas user variable names are restricted to `[A-Za-z0-9_-]`. They can never collide.
- **Read-only:** Triggers cannot modify built-in variables. Any `set_variable` targeting a built-in variable is rejected by validation.
- **No change events:** Built-in variables do not fire `variable_changes`. A trigger specifying a built-in variable in `variable_changes` is rejected by validation.
- **Conditions:** Conditions can inspect built-in variables via `compare { variable, op, value }`, where they are type-checked. Number built-ins accept numeric comparisons; `Quiz.Passed` and `Quiz.Reviewing` are true/false and accept `eq`/`ne`; text built-ins (`Learner.Name`, `Course.Title`, `Course.Date`) accept `eq`, `ne` and `contains`.

**Hosts.** The host sets `Learner.Name` and `Course.Date` with `Player::set_host_info(HostInfo { learner_name, date })`, which replaces both. Names are capped at 200 characters and dates at 32, cut on a character boundary. The SCORM bridge reads `cmi.core.student_name` (1.2) or `cmi.learner_name` (2004), and the xAPI bridge reads the launch actor's `name`; standalone players leave the name empty. Hosts format today's date with `civil_date(unix_secs) -> String`, which returns `YYYY-MM-DD` for years 1970–9999 and an empty string outside that range. Host info is runtime-only (`PlayerState.host`) and never written to suspend data.

## Text references

The runtime provides pure text reference replacement via `Player::display_text(&self, text: &str) -> String`.

- **Substitution:** Replaces `%Name%` with the current value of the referenced user variable or built-in variable `Name`.
- **Formatting:**
  - Numbers: whole numbers print without decimals (`3`, not `3.0`); non-whole numbers format with up to 2 decimal places and trailing zeros trimmed (`2.5`, `0.33`). Non-finite numbers print as `0`.
  - Booleans: `true` or `false`.
  - Strings: inserted as-is.
- **Escapes and unmatched names:**
  - `%%` represents a literal `%` character (for example, `100%%` renders as `100%`).
  - Unknown variable names remain unchanged, including their `%` delimiters (for example, `%nope%` stays `%nope%`).
  - A lone `%` without a closing `%` is left as-is.
  - Names inside `%…%` are matched case-sensitively and capped at 128 characters; longer sequences are left unchanged.
- **Resource protection:** Output string length is strictly capped at 64 KiB (65536 bytes) to prevent hostile input from exhausting memory. Parsing never panics on any input.
- **Render context:** Text substitution is applied at render time in the player and preview only: text object runs, button labels, state `text` overrides, question prompts, and question choices. The editor canvas and slide thumbnails preserve authored text with `%Name%` placeholders untouched.
- **Rich text runs:** In text objects with runs, `%var%` substitution is applied per run independently. A reference split across two runs (e.g. `%Cour` in run 1 and `se.Progress%` in run 2) is not substituted.
- **State text overrides:** When an object state defines a `text` override, it replaces the entire content with a single run using the paragraph style defaults.

## Conditions

`all`, `any`, `not`, `compare {variable, op, value}` and `object_state {object, state, is}`. Conditions can be nested at most 32 levels deep. `lt`, `le`, `gt` and `ge` compare numbers only. `contains` applies to text only.

## Questions

Learner choices live in runtime state and are cleared on slide entry, together with `selected`:

- `text_entry` (string) — fill-in text.
- `sequence` (ordered choice ids) — the current sequence order.
- `matches` (pair id of the left → pair id of the chosen right).
- `hotspot_markers` (slide coordinates) — markers the learner has placed, at most `markers`.
- `drops` (drag item id → drop target id) — which target each item sits on.

Drag item positions are the same `object_offsets` map motion paths use (see Animations). Slide entry clears that map, so drag positions reset with the markers and drops.

None of these are written to suspend data. Resume re-enters the slide, so they start empty and a sequence question is shuffled again.

- `select_choice`: for multiple choice, multiple response, and true/false it selects a choice (multiple choice and true/false replace the selection; multiple response toggles). For fill-in, sequence, matching, hotspot, and drag-and-drop it emits `warning` and changes nothing.
- `text_entered {text}`: fill-in, short answer, and essay. Replaces `text_entry` and emits `text_entry_changed {text}`. The text is capped at 2000 characters, cut on a character boundary, and never panics on long or odd input.
- `sequence_moved {choice, to}`: sequence only. Moves that choice id to index `to` (clamped into the list) and emits `sequence_changed {order}`.
- `match_selected {left, right}`: matching only. Sets or clears (`right` null) the match for a left pair id and emits `match_changed {left, right}`.
- `hotspot_clicked {x, y}`: hotspot only, and only while the question is unlocked. `x` and `y` are slide coordinates. Non-finite coordinates emit `warning` and change nothing. The point must fall inside the hotspot image's current frame (authored frame, plus its `object_offsets` translation, then scaled about the centre, inclusive bounds); otherwise `warning` and no change. If `sample_object` has no frame for that image, `warning` and no change. With `markers == 1` the marker list becomes that single point. With more, the point is appended until the list is full, then the oldest point is dropped. `markers` outside 1–10 emits `warning` and changes nothing. Emits `hotspot_marked {markers}`.
- `drag_dropped {item, target?, x, y}`: drag-and-drop only, and only while the question is unlocked. The host reports where the learner released the item: `x`, `y` are the item frame's new top-left in slide coordinates, and `target` is the drop target under the item's centre (the renderer decides; the runtime re-checks). Unknown item, a question that is not drag-and-drop, or a locked question emits `warning` and changes nothing. Non-finite `x` or `y` emits `warning` and changes nothing. Finite coordinates are then clamped into the slide (`x` in `[0, slide width]`, `y` in `[0, slide height]`). A `target` that is not in `drop_targets` is treated as none. Dropped on none (outside every target): the item returns to its authored position (its offset is cleared) and its drop is removed. Dropped on a target: if `return_if_wrong` and that target is not this item's correct target, the item returns to its start immediately and the drop is not recorded (instant feedback; an occupant of that target stays). Otherwise, if `one_per_target` and another item is already on that target, that other item returns to its start. Then the drop is recorded. The item's offset is centred on the target object's authored frame when `snap_to_center` (target origin plus half the target size, minus half the item size, minus the item's authored origin), and otherwise `(x, y)` minus the item's authored frame origin. Emits `item_dropped {item, target?, offset}` for the moved item first, then one for each item sent back, in item-id order. Offset `(0, 0)` and `target` null means the item is back at its authored position.
- Each of those inputs emits `warning` and changes nothing when the slide has no question, the question is a different kind, an id does not exist, or the question is locked (the same lock rule as `select_choice`). Hostile values warn and never panic. While a drag-and-drop question is unlocked, a press on a drag item starts a drag and does not send `click` (the renderer owns that; the runtime still accepts a `click` if one is sent).
- **Shuffle.** Sequence (the initial order) and matching (the order of the right-hand options) are shuffled with a seeded PRNG, not `shuffle` on the question. The seed is FNV-1a 64 (offset basis `14695981039346656037`, prime `1099511628211`) over the UTF-8 bytes of the slide id followed by the attempt number as a little-endian `u32`. The attempt number is how many graded attempts are already recorded for that slide (`0` before the first submit). The PRNG is SplitMix64 (`state += 0x9E3779B97F4A7C15`, then the usual xorshift-multiply steps). Fisher–Yates walks from the last index down to `1`, swapping with `next_u64 % (i + 1)`. For a sequence, if that order equals the authored order and there are at least two items, the first item is moved to the end so the learner never starts already correct. A new attempt (an incorrect submit that is not yet locked) re-shuffles the sequence; matching options change with the attempt number because they are computed from the same seed.
- `submit_question` grades the current kind, then records attempts, emits `question_answered {correct, points, attempts_left}`, and shows `correct_layer` or `incorrect_layer`. A question is locked once it is answered correctly or its attempts run out (`attempts: 0` = unlimited). Grading also stores a copy of the submitted answer state (`selected`, `text_entry`, `sequence`, `matches`, `hotspot_markers`, `drops`) in the runtime-only `PlayerState.review_answers` map (slide id → snapshot), before any sequence reshuffle; re-grading replaces it. `review_answers` is never written to suspend data.
  - **multiple choice, multiple response, true/false:** an empty selection emits `answer_required`. Otherwise correct means the selection equals the set of correct choices exactly.
  - **fill-in:** the entry is trimmed; an empty entry emits `answer_required`. Correct when the trimmed entry equals any trimmed accepted answer. Comparison is Unicode `to_lowercase` unless `case_sensitive`. `%var%` is not expanded when grading.
  - **sequence:** correct when `sequence` equals the choices' authored order. Never `answer_required` — the list always has an order. An incorrect submit that is not locked replaces `sequence` with a new shuffle (no extra effect; the renderer reads the order).
  - **matching:** `answer_required` unless every left has a match. Correct when every left maps to its own pair id.
  - **hotspot:** `answer_required` when there are no markers. Correct when every marker lies inside at least one correct region. A rectangle contains a point when `x <= px <= x + w` and `y <= py <= y + h` (inclusive). An ellipse is centred at `(x + w / 2, y + h / 2)` with radii `w / 2` and `h / 2`, and contains a point when `((px - cx) / rx)² + ((py - cy) / ry)² <= 1`. A non-positive radius does not contain the point. A marker outside every region, or inside only incorrect regions, makes the answer incorrect. A marker that is inside a correct region and also an incorrect one still counts as correct. After an incorrect submit that is not locked, the markers stay so the learner can move them.
  - **drag-and-drop:** `answer_required` when no item has been dropped. Correct when every drag item is on its own authored target (a distractor then holds no item). After an incorrect submit that is not locked, items stay where they are.
  - **likert, short answer, essay:** an empty answer emits `answer_required`; otherwise the question is recorded as answered with `correct: true` and 0 points, shows `correct_layer`, and locks. They add nothing to `raw` or `max`. Short answers are capped at 250 characters and essays at 2000.
  - **freeform:** never emits `answer_required`. Correct when `correct_when` evaluates true against the current variables and object states, using the same condition rules and depth limit as a trigger condition. `all_or_nothing` awards `points` when correct and 0 otherwise. It records the attempt, locks by the normal rule, shows `correct_layer` or `incorrect_layer`, and snapshots the (empty) answer state for review exactly like the other kinds. `last_response` is the text `correct` or `incorrect`.
- **points:** `correct` is decided exactly as above. `Question.scoring` changes only the points awarded: `all_or_nothing` awards `points` when correct and 0 otherwise; `per_answer` sums the selected choices' points and clamps to `0..=max_points()`; `partial` awards `points` when correct, otherwise `floor(points * right / total)` with `right` floored at 0 and 0 when `total` is 0 (multiple response: selected correct choices minus selected wrong ones over the number of correct choices; sequence: positions matching the authored order over the number of choices; matching: pairs matched to their own id over the number of pairs; hotspot: markers inside a correct region over markers placed; drag-and-drop: items on their own target over the number of drag items).
- `submit_results`: `percent = floor(raw * 100 / max)`. If the course has `quiz.passing_percent`, the status is `passed` or `failed`; otherwise it is `completed`. It emits `score` and `completion`, and adds 1 to `PlayerState.quiz_attempts` (read as `Quiz.Attempts`). It does not end the course: follow it with `exit_course` to emit `exit` and show the end card. **[verify rounding]**

## Retry and answer review

Actions `retry_quiz`, `review_quiz` and `exit_review` (no fields) are available on any trigger, typically from buttons on a results slide. They work in `coursecraft-logic` only; no UI for them exists yet beyond dispatching the actions.

- **`retry_quiz`.** Clears `answers`, the answer state of every question (`selected`, `text_entry`, `sequence`, `matches`, `hotspot_markers`, `drops`), `last_response` and `review_answers`, and the saved slide state (`saved_slides`) for question slides. If the course has banks, `draw_seed` is set to a new value derived from the old one with `seed_for(old_seed_string, quiz_attempts)` truncated to `u32`, and the play order `slides` is rebuilt from that seed, so a retry redraws new questions from the banks. This is repeatable: the same sequence of retries from the same starting state produces the same picks. Then it jumps to the first slide in play order that has a question. With no question anywhere it warns and does nothing (nothing is cleared). Variables are not reset.
- **`review_quiz`.** Only when at least one question has been answered (a graded `answers` entry on a slide in play order), otherwise it warns and does nothing; it also warns when already reviewing. It sets `PlayerState.reviewing_from` to the current slide id and jumps to the first answered question in play order. It emits `Effect::ReviewChanged { reviewing: true }` on entering.
- **While reviewing** (`Quiz.Reviewing` is true):
  - Entering a slide restores its answer state from `review_answers` (the snapshot taken at grading), and every question is locked: `select_choice` and the other question inputs warn and change nothing, exactly like an answered-and-locked question.
  - Player `Next` moves to the next slide in play order that has a question with an answer; `Previous` likewise backward. Non-question slides and unanswered questions are skipped. After the last answered question, `Next` runs `ExitReview`. `Previous` before the first answered question does nothing.
  - Entering a slide runs none of its `timeline_starts` or other slide-entry triggers, no autoplay media starts, and no feedback layers are shown (saved visible layers are not restored). The clocks do not advance while reviewing, so `tick` is ignored.
  - `review_answers` is runtime-only (`#[serde(skip)]`-style, empty for a fresh state) and is never written to suspend data. After a resume it is empty, so review slides show no restored answers (though the questions are still locked by their graded `answers`).
- **`exit_review`.** Clears the reviewing state, emits `Effect::ReviewChanged { reviewing: false }`, and returns to the slide in `reviewing_from` through the normal slide-entry steps (so its `timeline_starts` and autoplay run again). If not reviewing it warns. `retry_quiz` while reviewing also clears the reviewing state and emits `ReviewChanged { reviewing: false }` before jumping to the first question. The player's **Exit review** button sends `Input::ExitReview`, which runs exactly the same code as this action and warns when not reviewing.
- **Effect.** `Effect::ReviewChanged { reviewing: bool }` is emitted on entering review (`true`) and on leaving it (`false`, via `exit_review`, via `Next` after the last answered question, or via `retry_quiz`). It is not a durable change: the LMS bridge ignores it, and it is not stored in `PlayerState` beyond `reviewing_from`.

## Printing

`print_slide` (no fields) is available on any trigger, typically on a Print button on a certificate slide. Running it emits `Effect::Print { slide, exclude }`, where `slide` is the current slide's id. `exclude` holds the object of the firing `click` event, so the Print button itself is never printed; for any other event it is empty. The action changes no player state.

Hosts render the current slide as it is now, without the excluded objects, and offer it for printing; the bare desktop player logs that printing works in the published course or the editor preview.

## Animations

Entrance and exit animations and motion paths describe object visual motion and visibility transitions:

- **Easing:** Easing curves transform normalized time `t` in `[0, 1]`:
  - `linear`: `t`
  - `ease_in`: `t³`
  - `ease_out`: `1 - (1 - t)³`
  - `ease_in_out`: cubic in/out (`4t³` for `t < 0.5`, `1 - 4(1 - t)³` for `t >= 0.5`)
- **Entrance:** Runs over `[start_ms, start_ms + duration)`.
  - When the clock crosses `start_ms + duration`, event `animation_completes { object, path: None }` fires.
- **Exit:** Runs over `[end - duration, end)`, where `end` is `timing.end_ms` or the owning timeline's duration (`slide.duration_ms` / `layer.duration_ms`).
  - If entrance and exit overlap, exit wins during the overlap.
  - After exit completes (`t >= end`), the object is not drawn and not live (`object_is_live` is false, and clicks/hovers warn like any off-screen object).
- **Motion paths:**
  - Started by action `move_on_path { object, path }`. Starting a path on an object already moving replaces the running path, starting from the current interpolated offset.
  - Stored in runtime `PlayerState` (`running_paths` and `object_offsets`); runtime-only, never persisted in `suspend_data`.
  - Advanced during `tick(dt)` unless paused by timeline pause (`pause_timeline` or `pause_base_timeline`).
  - When a path completes (`elapsed_ms >= duration_ms`), resting offset is recorded, and event `animation_completes { object, path: Some(id) }` fires.
  - Position along a path travels at constant speed along the polyline by length, eased by the path's easing curve.
  - On slide entry (`enter_slide`), running paths and offsets are reset. Layer hiding does not reset offsets. Drag-and-drop positions are the same `object_offsets` map, so they reset on slide entry too.
  - Motion paths and animated offsets do not move connectors at play time. A glued connector's routed points are fixed authoring data; the runtime draws them unchanged even when its target animates.

## Transitions

Slide transitions are render-only visual effects occurring when a slide with `transition` is entered:
- `fade`: Previous slide items blend out at alpha `1 - p`, new slide items blend in at alpha `p`.
- `push { from }`: Both slides translate together; new enters from `from`, old exits opposite.
- `cover { from }`: New slide translates in from `from` over static previous slide.
- Transitions run for `duration_ms` of real time. Inputs and logic begin running on the new slide immediately.

## Question banks and the random draw

A course may carry named question banks (`Course.banks`), kept outside play
order like masters. Every bank slide carries a question. A scene slide may
carry `draw: { bank, count }` and has no question of its own.

At play time the player expands the scene order into a play order: a slide
without `draw` is kept; a slide with `draw` is replaced by `count` slides
picked at random from its bank. The picks come from a shuffled list per bank
(one list per bank, seeded from the draw seed and the bank id with the same
FNV-1a / SplitMix64 / Fisher–Yates machinery used for sequence shuffles, via
`shuffled_ids`). Draws take from the front of that list in course order, so two
draws from one bank never pick the same question. A missing or short bank
draws what exists and never panics, and the runtime never crashes on it.

- **Seed.** The draw seed is chosen when the learner starts (`Player::new`
  uses 0; `Player::with_draw_seed` takes an explicit seed) and is stored in
  `PlayerState.draw_seed`. `Player::resume` rebuilds the play order from the
  state's seed before checking the saved slide index. `simulate` uses seed 0.
- **Start slide.** The start index is computed from the expanded order, not
  from the scene order. When `start_slide` names a draw slide, the learner
  starts on its first drawn question.
- **Jumps and lightboxes.** A jump, the slide menu and `CourseCraft.jump` to a
  draw slide land on its first drawn question; the same resolution applies to
  lightboxes opening on a draw slide. A jump to a bank slide that was not drawn
  warns as a missing slide does, and changes nothing.
- **Score.** Only drawn questions count: `Course.TotalSlides` counts expanded
  slides and the score `max` (and `ScorePercent`) sums only the questions in
  the expanded play order. Undrawn bank slides and the draw slide itself are
  never scored.
- **Suspend data.** When `course.banks` is not empty the first field is `draw_seed` as a varint, immediately followed by `quiz_attempts` as a varint (see Suspend and resume); every slide index is an expanded play-order
  index. A course without banks but with `quiz` writes only `quiz_attempts`. A course with neither encodes byte-for-byte as before. A seed larger
  than `u32::MAX` is an error on decode, not a panic.

## Suspend and resume

The runtime encodes durable learner progress into compact suspend data and restores it across sessions.

- **Durable subset:** current slide index, visited slides, object states differing from initial states, variables, answers, and `quiz_attempts` when the course has `quiz` set or has banks (one varint directly after the draw-seed position; a course with neither encodes byte-for-byte as before).
- **Not stored:** timeline clocks, visible layers, saved slide/layer clocks, selection, fill-in text, sequence order, matches, hotspot markers, drag drops, object offsets, `review_answers` snapshots, the reviewing slide, media playback, timer countdowns, the player-paused flag, open lightboxes, epoch, started and exited flags. Resuming re-enters the saved slide, resetting these. Because `review_answers` is empty after a resume, review slides show no restored answers.
- **Indices:** ids are replaced by course-relative indices (play-order slide index, model-order object index, state index in builtins then object states, variable index in course variables). If the course structure changes, old data with out-of-range indices or mismatched types is refused, and the LMS layer starts fresh.
- **Format:** `cc1:` prefix followed by unpadded base64url of a binary payload with LEB128 varints. See `crates/logic/src/suspend.rs` for the exact byte layout.
- **Resume rule:** the LMS layer restores state via `Player::resume(course, decoded)` and sends `start`. The runtime enters the saved slide through the normal slide-entry steps, preserving restored object states, variables, visited slides and answers.
- **Resume whenever suspend data decodes**, whatever `cmi.core.entry` says. A finished learner (`exit` was empty) lands on the last slide again rather than starting a new attempt, so a passed status is never overwritten by a fresh, failed attempt. LMSs that clear suspend data for a new attempt get a fresh start naturally.
- Because resume re-enters the slide, layers are hidden again. A learner who answered a question and relaunched sees the question locked with its feedback layer (and that layer's Continue button) gone; with player navigation turned off they cannot leave the slide. **[verify]** against Storyline, then decide whether answered questions re-show their feedback layer on resume.

## Player navigation modes

Two course-level settings in `player` and one per-slide override restrict how a learner moves. They gate only the learner's own controls (`next` and `go_to` inputs); author navigation (`jump_to_slide` actions, review flow, results slide buttons, `Input::Jump`) is never blocked. Lightbox and review rules are checked first and stay as described elsewhere.

- **`player.navigation`** is `free` (default), `restricted` or `locked`. `free`: any slide opens from the menu. `restricted`: `go_to` works only for a slide already in `visited`, or for the current slide. `locked`: `go_to` is refused for every slide. `next` and `previous` are not affected by this setting.
- **`player.next_requires`** is `{"kind":"nothing"}` (default), `{"kind":"timeline_ends"}` or `{"kind":"variable","name":…}` (a true/false variable). It is the course default for when Next works. `slide.player.next_requires`, when set, replaces it on that slide.
- **Met when.** `nothing` is always met. `timeline_ends` is met once the slide's base timeline has reached `duration_ms` on this visit or any earlier one. `variable` is met while the variable is true.
- **Completed slides.** The runtime keeps a set `timeline_completed` (runtime-only, never in `suspend_data`). A slide joins it when its base clock reaches the end, noted on each tick and when the learner leaves. A slide in the set keeps Next unlocked on a return visit. `Player::resume` treats every visited slide other than the current one as completed when the set is empty.
- **Queries.** `Player::can_next()` is false while the current slide's requirement is unmet (always true before start and while reviewing). `Player::can_go_to(slide)` applies the navigation mode (always true while reviewing). Every host uses these.
- **Blocked input.** A blocked `next` or `go_to` changes nothing and returns one `warning` effect ("Next is locked until this slide is complete", "that slide is locked in the slide menu"). `previous` is never blocked by these rules.
- **Chrome.** The Next button is drawn disabled with the tooltip "Complete this slide to continue" while `can_next()` is false. Drawer rows the learner cannot reach are dimmed with a lock glyph and send nothing. Both stay in the focus order but do nothing, and the screen reader reads their value as "locked". The `→` key sends nothing while Next is locked.
- **Validation.** A `variable` rule must name an existing true/false variable (user or built-in).

## Playback speed

The learner can run the course faster or slower. This is a player-chrome feature with one runtime rule: time.

- **Rates.** `Player::set_speed(rate)` (or `Input::SetSpeed { rate }`) accepts 0.5, 0.75, 1, 1.25, 1.5 and 2. Any other finite value snaps to the nearest one; NaN and infinity become 1. `Player::speed()` returns the rate in effect.
- **Time.** `tick(ms)` advances every clock (base and layer timelines, timers, Lottie clocks, cue progress, motion paths) by `ms × speed`. The runtime counts quarter milliseconds exactly and carries the remainder between ticks, so 1000 ticks of 1 ms at 1.5× advance exactly 1500 ms. Timers run at speed too; there is no quiz time limit.
- **Scope.** Speed is runtime-only: it is never in `suspend_data` and it stays across slide changes. With `player.show_speed` false it is ignored and always 1.
- **Media.** `MediaBackend::set_rate` and `supports_rate` (default: do nothing, `false`) change the rate of audio and video. `MediaDriver` applies the player's rate whenever it changes and to newly started media. **The speed control is hidden whenever the active backend's `supports_rate()` is false**, so the slide clock never runs at 1.5× while narration plays at 1×. Hosts that offer speed: the web player (HTML media `playbackRate`, which keeps pitch). The native desktop backend and the editor preview do not, so they show no speed control.
- **Chrome.** A speed button after the captions button shows the rate (for example "1×") and opens a list of the six rates; picking one sends `set_speed`. With keyboard focus, Enter cycles to the next rate. It is announced as "Playback speed, 1.5 times". `player.show_speed` and the host flag `PlayerFrame::can_speed` must both be true.
- **Full screen.** A toggle at the right end of the strip (`player.show_fullscreen` and the host flag `PlayerFrame::can_fullscreen`). The render crate calls no platform API: it reports `FrameOutput::fullscreen_toggle` and the host acts. The native player sends `ViewportCommand::Fullscreen`; the web player uses the browser Fullscreen API on the document element and shows the button only when `document.fullscreenEnabled` is true (false inside an LMS iframe without `allowfullscreen`). A refused request is logged, never a crash. The editor preview never offers it. `Esc` leaves full screen (platform default). The button is announced as "Full screen", pressed or not.

## Renderer

The renderer draws what the runtime says and reports what the learner does. It never decides play-time behaviour.

- **It sends; the runtime decides.** A press and release on the same item sends `click` for an object or `select_choice` for a question choice. Clickability (layer visibility, timing window, `hidden`/`disabled`, `prevent_base_clicks`, `hide_base_objects`) is the runtime's call, so the renderer never filters clicks and never changes states.
- **One hit path.** Draw order is base objects, then the question prompt and choices, then visible layers in authored order (back to front, `slide.layers` order, whatever order they were shown in). Base objects and the question are left out entirely while a visible layer has `hide_base_objects`; masters still draw. The topmost item under the pointer wins. Releasing on a different item, or off the slide, sends nothing. Pointer events over other UI drawn above the slide (menus, popups) don't reach it.
- **Image crops.** An image with no `crop` is drawn aspect-fit and centred in its frame, exactly as before crops existed, so a frame whose proportions differ from the picture's letterboxes it. With a `crop`, the kept window of the source texture fills the object's frame. A full-source crop draws exactly like no crop. Crop is presentational: the image still hit-tests by its frame, and hotspot regions stay in slide coordinates, so cropping a hotspot image never moves a region.
- **Shapes hit-test by their frame.** Every shape kind draws its own contours, which may be concave or have holes, but pointer hits and keyboard focus use the object's authored frame. A click in a cut-out hole, or beside a concave edge inside the frame, still reaches the shape. A `path` is the exception: it hits within `max(6 px, stroke width / 2 + 2 px)` of its drawn line, or anywhere inside a closed path that has a fill. A diagonal line therefore does not steal a click across its bounding box. The editor's object picking uses the same rule.
- **Hover.** When the topmost object under the pointer changes, the renderer sends `hover_end` for the old one, then `hover_start` for the new one. Choices don't hover. If the slide changes under a hovered object, the hover is dropped without `hover_end`.
- **Question layout.** The model has no frames for questions, so the renderer uses fixed slide-coordinate rects: prompt at (80, 60, w − 160, 80). Rows start at y = 160, x = 80, width min(900, w − 160). The default row is 56 tall with a 16 gap. If that would pass the bottom margin (24 px above the slide bottom), the row shrinks down to 28 tall with a 4 gap; anything still past the slide is clipped. Multiple choice, multiple response, and true/false are choice rows in model order (true/false is a radio). Fill-in is one text row. Sequence rows follow the current shuffled order and carry up/down buttons. Matching is one row per pair in model order; the dropdown lists the shuffled right-hand texts.
- **Question widgets.** They are drawn inside the slide transform and send inputs only. Fill-in places a single-line text field over its row (placeholder "Type your answer") and sends `text_entered` when the text changes. Sequence up/down buttons send `sequence_moved`. Matching places a dropdown per pair (a "Choose…" entry plus the shuffled rights) and sends `match_selected`. Hotspot and drag-and-drop draw the prompt and no choice rows. The editor canvas shows the same layout with no interaction. `%var%` substitution applies to the prompt, choice texts, pair texts, and any accepted-answer text that is drawn, the same way it does for other player text. The learner's own fill-in entry is not substituted, and grading does not expand `%var%`.
- **Hotspot.** Clicking inside the hotspot image while the question is unlocked sends `hotspot_clicked` with slide coordinates (the slide transform's inverse) and does not also send `click`. When the question is locked, that image is an ordinary object again. Each marker is a filled circle in the theme accent with a white 2 px ring and about a 12 px radius, both scaled by the slide transform, drawn above the image and clipped to the slide. Markers do not take hits. The editor canvas draws every region as a dashed outline (green when correct, grey when incorrect) so authors can see them. The player never draws regions. When a region is selected in the question form, the canvas shows the same move and resize handles objects use; one drag commits one `question.update_region`.
- **Drag-and-drop.** Pressing a drag item while the question is unlocked starts a drag and does not send `click`. While dragging, the item is drawn following the pointer (a render-only offset on top of its logic offset, above everything else, clipped to the slide) and the drop target under the item's centre is outlined in the accent colour. Releasing sends `drag_dropped` with the item's new top-left in slide coordinates and the target under its centre. A release outside the slide sends `target` null. Escape during a drag cancels it and sends nothing. A press and release on the same ordinary object (including a button that is not a drag item) still sends `click`. Items mid-entrance stay draggable. Drop targets stay ordinary objects for hit testing. A dropped item paints above everything. When a target's text label would be covered by a dropped item, the label and its soft backing pill move just outside the target instead — directly above the target's frame, centred on it and clamped inside the slide, or directly below when there is no room above — and take no presses while moved, so both labels stay fully readable and the item keeps every press. The label moves only while a drag item overlaps it: an empty target keeps its label where the author put it.
- **Hover and down visuals** are presentational and are not runtime states. The object's `hover`/`down` state fill is used if it has one; otherwise buttons lighten 12% on hover and darken 15% when pressed. Objects in `disabled` get neither.
- **Time.** The renderer sends `tick` in whole milliseconds, carries the remainder to the next frame, and sends at most 1000 ms per tick.

### Captions

Captions are presentational. They are a chrome preference owned by the player frame, never part of `PlayerState`, and they never send inputs or change any runtime decision.

- **Toggle.** A CC button appears in the control strip only when the current slide has an audio or video object with a `captions` path. It uses the closed-captioning icon, the tooltip `Captions (C)`, and a pressed look while on. The `C` key toggles it unless a text field has focus, or, on the web screen-reader layer, unless the focused DOM control is a text box or select. The button joins the keyboard focus order before the outline button. Captions default to off and the choice persists across slides for the session.
- **What shows.** The media objects on the current slide that declare captions, are playing, and have active cue text are candidates. When more than one qualifies, video wins, then the first in slide order. Text comes from the WebVTT parser in `coursecraft-media`: cues are active while `start <= position < end` and simultaneous cues join with a newline. Cue lookup uses the backend's real position, so a seek moves the captions with the media. The display changes only when the cue text changes and clears when the object stops, when the cue ends, or when captions are turned off.
- **Desktop video and audio.** The player frame draws the text on the slide, centred horizontally, 24 slide px above the slide bottom and above the control strip. A dark translucent backing holds white text, sized from the design tokens and scaled with the slide transform, wrapped to at most 80% of the slide width. Desktop video plays real video (poster when decoding fails), and its caption position comes from the real decoder or audio clock, so it shows like any other video.
- **Web video.** An HTML video sits above the canvas, so egui text would be hidden behind it. The web backend keeps one `<div>` per video in the same stacking context, positioned over the bottom of the video's rect, with `pointer-events: none` and `aria-hidden="true"`. Its look matches the canvas backing. Text is written with `textContent`, never `innerHTML`, and only when the cue changes. The box follows the video's layout rect and hides with the video or when captions are off.
- **Assets.** A `.vtt` file lives under `media/` like any other asset. Validation checks its path and extension, publishing fails when a referenced file is missing, and `.ccraft` save, reopen and publish carry it unchanged.

### Keyboard access

Every action available to the pointer is available by keyboard, and the keyboard sends the same `Input`s. The renderer only reports the learner's keys; the runtime still decides. `Player::keyboard_objects()` answers which objects a click would reach right now using the same check as `click` (visible layers, timing window, `hidden`/`disabled`, `prevent_base_clicks`, unlocked questions). It is read-only and changes no state or effects.

**List order.** The renderer rebuilds the list every frame:

1. The notice banner's dismiss button, while a notice is showing (see Lost reporting).
2. Objects with `focus_order`, ascending; equal values keep the default order below.
3. Question controls, top to bottom: choice rows; the fill-in field; sequence up/down controls that can move (up then down per row); matching rows; the hotspot; drag items.
4. Base-layer objects from `keyboard_objects`, in order.
5. Objects on visible layers from `keyboard_objects`, layers bottom to top.
6. Player chrome in strip order: play/pause, replay, the seek slider, mute, the volume slider, captions when the slide has captions, outline, previous, next. Previous and next appear only where navigation is possible. A control dropped for width is not in the list.

`decorative` objects are never in the list. A locked question contributes no question controls. When the outline drawer is open, only its slide rows and close button are listed — with the notice's dismiss button first while a notice is showing — so focus is trapped there.

**Keys.**

| Target | Enter or Space | Arrows / other keys |
|---|---|---|
| `Object` | sends `click` | — |
| `Input` | text entry takes typing and commits on Enter (single-line); checkbox toggles on Space; radio selects the next option on Space | sliders/dials: arrows step (Shift = ten), Home/End go to min/max; radio: arrows move and select within the group; text entry: arrows edit text |
| `Marker` | sends `marker_toggled` | —; `Escape` while any marker is open sends `markers_closed` |
| Lightbox | `Escape` or its × button sends `close_lightbox` | — |
| `ScrollPanel` | — | arrows scroll 40 slide px, PageUp/PageDown about one viewport, Home/End jump to the ends |
| `Choice` | sends `select_choice` | — |
| `SequenceMove` | sends `sequence_moved` with the same `to` a pointer hit uses | — |
| `FillIn` | nothing | focus moves into the text field; typing sends `text_entered` as today. `Tab` leaves the field and keeps its text |
| `MatchRow` | nothing | `ArrowUp` / `ArrowDown` cycle "Choose…" and the rights in displayed order, sending `match_selected`; "Choose…" clears the match |
| `Hotspot` | sends `hotspot_clicked` at the keyboard cursor | arrows move the cursor 16 slide px, or 4 with Shift, clamped inside the image |
| `DragItem` | picks the item up; while held, drops it | while held, arrows or `Tab` cycle the open drop targets and then "back to start"; `Enter`/`Space` sends `drag_dropped` through the pointer path, centred on the target or home. `Escape` cancels and sends nothing. Focus stays on the item |
| Object with `drag` | picks the object up; while held, drops it | while held, arrows or `Tab` cycle its listed targets; `Enter`/`Space` sends `object_dropped` through the pointer path, centred on the target or back to its old position. `Escape` cancels and sends nothing. Focus stays on the object |
| `Chrome` | play/pause sends `player_pause` or `player_resume`; replay sends `player_resume` when paused then `player_seek {ms: 0}`; mute and volume change the session volume; previous/next send navigation; outline toggles the drawer; a drawer row sends `go_to`, close closes, the dismiss button hides the notice | on the seek slider, `←`/`→` move one second and `Home`/`End` go to the ends; on the volume slider, arrows nudge and `Home`/`End` go to silence/full |

`Tab` and `Shift+Tab` move through the list and wrap on desktop. On the web the hidden DOM layer owns native focus (see Screen readers), so `Tab` is not wrapped: past the last element it leaves the course, as WCAG 2.1.2 requires. The keys are consumed before egui's own focus handling, so there is one focus order. Moving focus to a chrome button shows the control strip.

**Arrows.** While the hotspot is focused, an item is held, or a matching row is focused for up and down, those arrows belong to the control and never send `previous` or `next`. Otherwise `←` and `→` keep their chrome meaning.

**Focus changes.**

- A slide change resets focus to the first item of the new list.
- A layer becoming visible moves focus to that layer's first item.
- A layer hiding moves focus back to the first item of the list.
- If the focused target otherwise leaves the list, focus moves to the target at the same index, or the last one. An empty list has no focus.
- After a sequence move focus stays on the same choice and direction, or switches to its other direction when that one can no longer move.
- Opening the drawer traps focus inside it. `Escape` closes it and returns focus to the outline button.

**Ring.** Keyboard mode is on after `Tab`, `Enter` or `Space`, and off after a pointer press; focus identity survives. While it is off the learner can't see what has focus, so the first of those keys only turns the ring on: it neither moves focus nor activates anything. Plain `←` and `→` navigation doesn't turn it on. Once the course has ended, nothing takes focus. While it is on, a 2 px accent ring with a 1 px contrasting outer ring is drawn from design tokens around the focused target's composed rect, following the slide transform above everything on the slide. Chrome buttons get the ring on their own rects. Hotspot focus draws a circular cursor in the same colours. Pointer hover looks unchanged.

**Hover parity.** When focus lands on an `Object` the renderer sends `hover_start`, and when it leaves it sends `hover_end`. Pointer hover tracking stays separate; neither path sends a duplicate for an object the other already holds. A slide change drops the old hover without `hover_end`.

### Screen readers

A canvas is invisible to assistive technology, so the web player places a hidden HTML layer over it. The layer is a plain `<main>` labelled with the course title, with `opacity: 0`, `overflow: hidden` and `pointer-events: none`; it is not `role="application"`. It exists for every standard, including standalone playback, and is absent on desktop and in the editor preview. The canvas and eframe's text-agent `<input>` are `aria-hidden="true"` with `tabindex="-1"` so neither is announced or tabbed to. `<html lang>` is `course.language` when set, else `en`, and the document title is the course title.

**Tree order.** `coursecraft-render` builds a pure `AccessTree` from the same composition the canvas draws. Reading order is:

1. The notice banner's text, then its dismiss button, while a notice is showing (see Lost reporting).
2. The slide title as a heading.
3. Interactive objects with `focus_order`, ascending.
4. Non-interactive base objects, bottom to top: text with its displayed text, images with `description`, else `alt`. A shape with neither text nor description, and a text object with empty text, are left out. An image with neither `alt` nor `description` stays with an empty label so a checker can flag it.
5. The question prompt as text, then its controls.
6. Interactive base objects.
7. Visible layers, bottom to top, each layer's objects in order, interactive or not, so feedback is read before its Continue button.
8. Player chrome.

`decorative` objects and anything not currently composed (hidden, outside its timing window, on a hidden layer) are left out. `%var%` substitution applies exactly as it does on the canvas. Course text stays plain text in the DOM: it is set with `textContent`, never `innerHTML`. Interactive nodes appear in exactly the same order as the keyboard focus list.

**Roles.**

| Focus target | DOM role | Label | State and description |
|---|---|---|---|
| `Object` | button | label text or `description` | `checked` when a state named `selected` applies |
| `Input` text entry | text input, or a textarea when `multiline` | the object's name or `description`, else its placeholder | `value` |
| `Input` slider/dial | range input with `role="slider"` | the object's name or `description` | `aria-valuemin`, `aria-valuemax`, `aria-valuenow` |
| `Input` checkbox | checkbox | the object's name or `description`, else its label | `checked` |
| `Input` radio group | fieldset with `role="radiogroup"` | the object's name or `description` | its options, with the selected value |
| `Marker` | button | the object's name, `description`, else its title | `aria-expanded` is true while its label is open; the card's title and body are read as text right after it while open |
| `ScrollPanel` | button | the object's name or `description` | children wholly outside the viewport are not read; a child scrolled into view appears in list order |
| `Choice`, multiple choice or true/false | radio | choice text | `checked` |
| `Choice`, multiple response | checkbox | choice text | `checked` |
| `SequenceMove` | button | `Move "{choice}" up` or `down`, then `, position {n} of {m}` | |
| `FillIn` | text input | the prompt | `value` |
| `MatchRow` | select | left text | options are "Choose…" plus the shuffled rights; `value` is the selected right |
| `Hotspot` | button with `role="slider"` | `Place a marker on the image` | `aria-valuetext` is `{markers} placed`; description `Use the arrow keys, then press Enter.` |
| `DragItem` | button | item label | `aria-pressed` while held; description `Press Space to pick up, arrows to choose a target, Space to drop.`; when dropped the label ends with `, on {target label}` |
| Object with `drag` | button | object label | `aria-pressed` while held; description `Press Space to pick up, arrows to choose a target, Space to drop.`; while held or after a successful drop the label ends with `, on {target label}` |
| `Chrome` | button, or slider for seek and volume | `Dismiss`, `Play`, `Pause`, `Replay slide`, `Slide position`, `Mute`, `Unmute`, `Volume`, `Previous slide`, `Next slide`, `Course outline`, `Captions`, `Close outline`, `Close lightbox`, or the slide title | `Captions` and `Mute` set `aria-pressed`; the sliders report their `value` |
| `Timer` | timer (a `div`) | the object's name or `description`, else `Timer` | its text is the current value; the live region announces it only at each whole minute and in the last 10 seconds |
| `ProgressBar` | progressbar (a `div`) | the object's name or `description`, else `Progress meter` | `aria-valuemin`, `aria-valuemax`, `aria-valuenow` and `aria-valuetext` as a percentage |
| Lightbox container | dialog | the lightboxed slide's title | `aria-modal="true"`; its controls and close button follow in reading order |

These are the English defaults. A course can replace every label and description through `course.strings` (see Localisation); the placeholders above must stay.

Each element is positioned from its composed slide rect through the slide transform (chrome uses screen coordinates), so a screen reader's highlight and touch exploration land in the right place.

**Focus ownership.** The DOM layer owns real focus on the web. `Tab` and `Shift+Tab` walk its elements in DOM order, which is the focus list's order, and native `Tab` is not wrapped: past the last element it leaves the course. `focusin` tells the focus manager which target to focus and turns keyboard mode on, so the canvas ring follows. When a key moves focus in the manager while the canvas has focus, the matching element receives `.focus({preventScroll:true})` and native handling takes over. A pointer press on the canvas keeps today's behaviour: the layer ignores pointer events, the canvas takes focus and keyboard mode goes off. The player never takes DOM focus while keyboard mode is off, so a mouse user and the surrounding LMS page are left alone. A slide change or a newly shown layer pushes focus to the matching element only if focus was already in the layer or on the canvas.

**Live region.** A polite, atomic live region inside the layer announces:

- a slide change: `Slide {n} of {m}: {title}`;
- a newly visible layer: its text nodes joined;
- drag-and-drop, for question items and draggable objects: `Picked up {item}`, then `{item} over {target}` as the target changes, then `{item} dropped on {target}`;
- submitting an unanswered question: `Please answer the question`;
- the notice banner, once, when it appears: its text.

The region clears and re-sets its text so a repeated message is read again.

**Limit to state.** The layer is per-slide, not per-glyph: a rich-text object reads as one block, without its run boundaries, bold, or emphasis.

### Player chrome

The published player and the editor preview share one chrome widget (`crates/render/src/player_frame.rs`). It draws the letterboxed slide on a neutral ground, a floating control strip and an outline drawer. It emits only the player inputs already in the runtime: `next`, `previous`, `go_to`, `player_pause`, `player_resume` and `player_seek`. Everything else about the course is drawn from runtime state, never stored in the chrome.

- **Strip, left to right.** Play or Pause, Replay, the current slide's seek bar and its `0:03 / 0:10` time label, the speaker button and a small volume slider, captions when the slide has any, the outline button with `n / N`, then Previous and Next. The title sits before `n / N` and drops first when space is short, because the outline carries it. It fits from 320 pt up; as width falls it drops `n / N`, then the volume slider (keeping the mute button), the time label and Replay. Play, the seek bar and Next stay the longest.
- **Play and pause.** Play sends `player_pause` when running and `player_resume` when paused. The strip stays visible while paused. `Space` does this when the Play/Pause control has focus; it is not taken from a focused slide object or trigger. A separate Replay control sends `player_seek {ms: 0}`, preceded by `player_resume` when paused. At the end of a slide the main button shows the Replay glyph and acts the same way.
- **Seek bar.** It belongs to the current slide. Its 4 px track shows the elapsed part in the course accent with a 12 px handle that grows on hover. A click or drag moves the handle while held and sends one `player_seek` on release. When the control has focus, `←`/`→` move one second and `Home`/`End` go to the ends. It is dimmed and sends nothing when `seek_info().seek_allowed` is false (a visible `pause_base_timeline` layer holds the clock) or the slide duration is zero.
- **Volume.** The speaker button toggles `Mute` / `Unmute` and the slider sets the level. The frame reports the change in `FrameOutput::volume`; the host applies it through `MediaDriver::set_volume`, which covers current media and media that starts later. Volume is a session preference, default 100%, and never enters course content or `suspend_data`.
- **Per-slide settings.** `Player::effective_player()` is the course's `player` with the current slide's `Slide.player` overrides (`show_nav`, `show_seekbar`, `show_menu`) applied. The runtime (the `next` and `go_to` checks) and every host read it instead of `course.player` for those three flags, so a slide can hide navigation, the seekbar or the menu on its own. `show_title`, `show_volume`, `show_captions` and `logo` are course-wide. `show_title` hides the title in the strip, `show_volume` hides the volume button and slider, `show_captions` hides the captions button. The `logo` picture sits at the left of the strip, at most 28 pt high with its aspect kept (and at most 120 pt wide); it is decorative, so it has no label, no focus stop and no access node. Until its picture loads, or if it fails, the strip shows no logo.
- **Flags.** `player.show_nav` gates the previous/next buttons, the `←`/`→` slide keys and drawer navigation. `player.show_menu` gates the outline button, the `M` key and the drawer. `player.show_seekbar` gates the slide seek bar and its time label. Play, pause and volume are shown once the course has started, whatever the flags. While a lightbox is open the Previous and Next controls and the arrow keys send nothing (the runtime warns), and the focus order contains only the box's controls and its close button.
- **Ignored before start.** Every chrome input is dropped until the runtime reports `started`. Before that the strip may be painted, but pressing a button, a key or an outline row sends nothing, because the runtime has not entered the course yet. Once the course has ended the strip is hidden (see Ended).
- **Keyboard.** `Tab` and `Shift+Tab` move through the player's focus order (see Keyboard access) and reach the strip buttons and the open drawer. They wrap at both ends on desktop; on the web native `Tab` is not wrapped and leaves the course past the last element. `Enter` or `Space` activates the focused control. `←` and `→` send `previous` and `next` unless a hotspot, a held drag item, a matching row's up/down arrows, or the focused seek or volume slider own them. `M` toggles the outline drawer and `C` toggles captions. `Escape` closes it and returns focus to the outline button, and during a drag-and-drop question it cancels the drag instead (see above). Keys are ignored while a text field has focus, except `Tab`, which leaves the fill-in field. At the first and last slide the matching arrow sends nothing. There is no jump-to-slide shortcut.
- **Auto-hide.** The strip fades out after 2.5 s without pointer or keyboard activity and fades back in on any pointer movement, press, key press or touch. It stays visible while the pointer is over it, while one of its buttons has focus, while paused, while the drawer is open, and permanently when the platform reports reduced motion. Reduced motion also removes the fade and the drawer slide: both switch instantly.
- **Drawer tabs.** The drawer has up to four tabs: Menu (the outline below), Resources, Glossary and Transcript. A tab appears only when it has content and its setting allows it: Resources when `course.resources` is non-empty, Glossary when `course.glossary` is non-empty, Transcript when `player.show_transcript` is on. With only Menu the drawer looks as before, with no tab row. Resources lists each resource; activating one sends `open_resource`, and the runtime answers with the existing `open_url` effect (a link opens its URL; a file opens `media/<src>` from the package). Glossary shows a search field and the terms A to Z (stored in author order); the search keeps terms containing the text, ignoring case; choosing a term shows its definition below the list. Transcript shows the current slide's notes, wrapping and scrollable, and follows the slide. The focus order inside the open drawer is close, the tabs, then the chosen tab's items (rows, the search field and terms, or the transcript). The access tree exposes tabs as `tab` (pressed when chosen), resources as `link` and glossary terms as `list item`; slide notes stay in published packages because the transcript reads them.
- **Drawer.** The outline lists scenes and slides in play order, marks visited slides, highlights the current one, and sends `go_to` for the clicked slide. The header shows the course title (truncated) and a 32 px close button (tooltip "Close outline (Esc)"). The rest of the frame is covered by a black scrim at 35%. Clicking the scrim or the close button, or pressing `Escape`, closes the drawer; a scrim click never reaches the slide and sends no input. The current slide is not a navigation target.
- **Ended.** When the course has ended the chrome dims the slide with black at 55% and centres "Course complete" (heading size) over "You can close this window." (small, 8 px below), both white at 70%. The control strip is hidden. Because the strip is gone, no strip input is sent; the ended card's Restart control behaves as before.
- **Notice.** When the app reports lost reporting (see Lost reporting) the frame draws a slim banner across the top of the player, above the slide: the strip's bar colour with a 3 px warning accent from the design tokens, a warning icon, the notice text wrapped at the width left over from the icon and the close button, in the strip's text colour. It does not push the layout and takes a pointer only within its own rect. Pressing the close button, or activating it first in the keyboard focus order, hides the notice for the session.

## Localisation

The player's own learner-facing text comes from `CHROME_STRINGS` in `coursecraft-course`, with per-course overrides in `course.strings`. That covers the navigation and drawer tooltips, the completion card, the loading label, the fill-in and matching placeholders, the lost-reporting notice and its dismiss button, every screen-reader label and hint, and every live-region announcement. `{name}` placeholders are filled by `coursecraft_course::fill`; a translation must keep the same placeholder set.

`course.language` (BCP 47) sets the web player's `<html lang>`; a missing tag means `en`. The editor writes values in `course.json`, and XLIFF import sets the language from `target-language`.

XLIFF 1.2 export uses the stable ids documented in `docs/spec/course-model.md`; text runs become `<g id="rN">` groups so each run keeps its formatting through a reorder. Import applies only units whose `<source>` still matches the course, refuses `%var%` or chrome `{name}` placeholder changes, and reports missing targets, changed sources, unknown ids and unsupported scripts. The whole import is one engine command, so one undo reverts it.

**Limits.** The bundled font covers Latin, Greek and Cyrillic only. CJK, Arabic, Hebrew, Indic and similar scripts are reported by validation and still applied; right-to-left layout and complex shaping are not supported. String variables are not exported because triggers may compare them.

## Effects

The renderer and LMS bridge consume these: `slide_entered`, `layer_shown`, `layer_hidden`, `state_changed`, `variable_changed`, `media`, `media_seek`, `timeline_paused/resumed`, `choice_selected`, `text_entry_changed`, `sequence_changed`, `match_changed`, `hotspot_marked`, `item_dropped`, `object_moved`, `object_dropped`, `answer_required`, `question_answered`, `score`, `completion`, `open_url`, `run_javascript`, `lightbox_opened`, `lightbox_closed`, `review_changed`, `print`, `exit` and `warning`.

`Effect::Print { slide, exclude }` asks the host to render the named slide without the excluded objects and offer it for printing; the LMS bridge ignores it because it changes no durable state. The bare desktop player logs that printing works in the published course or the editor preview.

The LMS bridge maps `score` to `cmi.core.score.raw` (set to `percent`), `cmi.core.score.min` (`0`), `cmi.core.score.max` (`100`), and `cmi.core.lesson_status` (`status`). It maps `completion` to `cmi.core.lesson_status` and `slide_entered` to `cmi.core.lesson_location`. Durable progress is stored in `cmi.suspend_data` using the compact `cc1:` encoding; writes exceeding the SCORM 1.2 limit of 4096 characters are skipped with a warning. Commits are batched at most once per input. On `start` the bridge calls `LMSInitialize`, turns `not attempted` into `incomplete`, and resumes from `cmi.suspend_data` when it decodes (otherwise it starts fresh with a warning). Finishing (on `exit`, or when the page closes) sets `cmi.core.exit` to `suspend` until a passed/failed/completed status has been written and to an empty string after, sets `cmi.core.session_time` (`HHHH:MM:SS.SS`), commits and calls `LMSFinish`, once.

## SCORM 2004

A package states its standard in `coursecraft-package.json` at its root, for example `{"standard":"scorm2004"}`. A missing file means SCORM 1.2, so existing packages behave exactly as before. A bad or unknown marker logs a console warning and falls back to SCORM 1.2. The runtime is the same player; only the bridge's element names, vocabulary and limits change.

| Meaning | SCORM 1.2 | SCORM 2004 |
|---|---|---|
| API object | `API` | `API_1484_11` |
| Calls | `LMSInitialize` `LMSGetValue` `LMSSetValue` `LMSCommit` `LMSFinish` `LMSGetLastError` `LMSGetErrorString` | `Initialize` `GetValue` `SetValue` `Commit` `Terminate` `GetLastError` `GetErrorString` |
| Status read at start | `cmi.core.lesson_status` | `cmi.completion_status`, `cmi.success_status` |
| First launch | `not attempted` → write `incomplete` | completion of `unknown` or `not attempted` → write `incomplete` |
| Already finished | status is `passed`, `failed` or `completed` | completion is `completed`, or success is `passed` or `failed` |
| Score | `cmi.core.score.min`, `.max`, `.raw` | `cmi.score.min`, `.max`, `.raw`, plus `cmi.score.scaled` = percent / 100 (`0.85`, `1`, `0`) |
| Passed | `lesson_status` = `passed` | `success_status` = `passed`, `completion_status` = `completed` |
| Failed | `lesson_status` = `failed` | `success_status` = `failed`, `completion_status` = `completed` |
| Completed | `lesson_status` = `completed` | `completion_status` = `completed`; `success_status` untouched |
| Incomplete | `lesson_status` = `incomplete` | `completion_status` = `incomplete`; `success_status` untouched |
| Location | `cmi.core.lesson_location` (255) | `cmi.location` (1000) |
| Suspend data | `cmi.suspend_data` (4096) | `cmi.suspend_data` (64000) |
| Exit, not finished | `cmi.core.exit` = `suspend` | `cmi.exit` = `suspend` |
| Exit, finished | `cmi.core.exit` = empty string | `cmi.exit` = `normal` |
| Session time | `cmi.core.session_time` = `HHHH:MM:SS.SS` | `cmi.session_time` = ISO 8601, e.g. `PT1H2M3.45S` |

- **Unchanged effect rules.** Effects are processed in the same order as SCORM 1.2, with the same "suspend once, commit at most once" rule. Score is written before status. A write the LMS refuses becomes a warning, not a crash.
- **Scaled score.** `cmi.score.scaled` is a plain decimal from the same `percent`: `85` becomes `0.85`, `100` becomes `1`, `0` becomes `0`. It is never written in exponent form.
- **Hostile durations.** The ISO formatter uses wide arithmetic; `u64::MAX` milliseconds produces a valid `PT…H…M…S` string instead of overflowing. A zero duration is `PT0S`.
- **Location limit.** A slide id longer than 1000 characters is not written to `cmi.location`; the bridge logs a length warning and keeps playing. SCORM 1.2 keeps its historical unchecked behaviour for `cmi.core.lesson_location`.
- **Suspend limit.** Data between 4096 and 64000 characters is written under SCORM 2004 and still refused with a warning under SCORM 1.2.
- **No sequencing, no mastery score.** CourseCraft packages are single-SCO without `imsss:sequencing`. The manifest carries no `adlcp:masteryscore`, because CourseCraft decides pass or fail and reports `success_status` itself; an LMS-side passing score could override it.

## Interactions

Every graded question answer is also reported as one `cmi.interactions.n` record, so LMS quiz reports show what the learner chose. The runtime keeps the last submitted answer in `PlayerState.last_response`; it is captured by `grade` only when an answer is actually graded, before any sequence reshuffle or reset. It is runtime-only and never encoded in suspend data. The effect stream is unchanged: `question_answered` carries the same fields as before.

| Question | Interaction type | Correct answer |
|---|---|---|
| True/false | `true-false` | the correct choice as a boolean: the choice with id `true` maps to true, otherwise the first choice is true |
| Multiple choice | `choice` | one entry for each correct choice |
| Multiple response | `choice` | one entry holding all correct choice ids |
| Fill-in | `fill-in` | one entry per `accepted` value |
| Sequence | `sequencing` | one entry, the choice ids in authored order |
| Matching | `matching` | one entry, each pair id matched to itself |
| Hotspot | `choice` | with one marker, one entry per correct region; with more markers, one entry holding all correct region ids |
| Drag-and-drop | `matching` | one entry, each drag item to its authored target |
| Likert | `likert` | no correct answer |
| Short answer | `fill-in` | no correct answer |
| Essay | `fill-in` | no correct answer |
| Freeform | `other` | one entry `correct` |

The record's id is the sanitized slide id plus `_a` and the attempt number (`s-q-mc_a1`, then `s-q-mc_a2`). The description is the question prompt. Weighting is the question's `points`.

| Element | SCORM 1.2 | SCORM 2004 |
|---|---|---|
| Count | `cmi.interactions._count` | `cmi.interactions._count` |
| Id, written first | `.id` | `.id` |
| Type, written second | `.type` | `.type` |
| Description | not written | `.description`, prompt cut to 250 characters |
| Weighting | `.weighting` (question's maximum points) | `.weighting` (question's maximum points) |
| Correct | `.correct_responses.m.pattern` | `.correct_responses.m.pattern` |
| Learner | `.student_response` | `.learner_response` |
| Result | `.result` = `correct` or `wrong` | `.result` = `correct` or `incorrect` |

Interaction types are `true-false`, `choice`, `fill-in`, `matching`, `sequencing`, `likert` and `other` in both standards. A survey question (Likert, short answer or essay) has no correct answer and no correct responses are written for it; its result is `neutral` in both standards. Free-text answers are reported up to 250 characters. In xAPI, an `answered` statement for a survey omits `success` from its result; graded questions keep it.

| Response | SCORM 1.2 | SCORM 2004 |
|---|---|---|
| Boolean | `t`, `f` | `true`, `false` |
| Ids | `a,b` | `a[,]b` |
| Pairs | `a.b,c.d` | `a[.]b[,]c[.]d` |
| Text | the text | the text; a correct pattern adds `{case_matters=true}` only when the question is case-sensitive |

- **Indexing.** The bridge reads `cmi.interactions._count` once at start. A non-negative integer is the next index, and each record increments it, so a resumed session appends. If the value does not parse, the bridge emits one warning and writes no interactions for the session; scoring continues normally.
- **Writing.** A record is written on `question_answered` inside the existing effect loop and persisted by the session's usual commit or finish. A refused write becomes a warning, never a crash.
- **Sanitizing.** Ids in ids and pairs keep only `[A-Za-z0-9_:-]`; anything else becomes `_`. Text drops control characters and, under SCORM 2004, replaces literal `[,]` and `[.]` with `,` and `.`. The cut always lands on a character boundary.
- **Limits.** SCORM 1.2 values are cut to 255 characters; SCORM 2004 text is cut to 250. Correct patterns are capped at 5 for fill-in, matching and sequencing, and at 10 for choice and true-false. An empty id list writes an empty string.

## xAPI and cmi5

A package states its flavor in `coursecraft-package.json`: `{"standard":"cmi5"}` or `{"standard":"xapi"}`. cmi5 packages carry `cmi5.xml`; xAPI packages carry `tincan.xml`. Neither has an `imsmanifest.xml`. The same player runs every standard; `coursecraft-lms` translates effects into statements and never blocks play for the network.

### Launch parameters

The player reads its URL query string (with or without `?`) and percent-decodes it, treating `+` as a space. A query over 16 KiB is refused. Bad escapes stay literal, invalid UTF-8 is replaced, and the parser never panics.

| Field | cmi5 | xAPI | Rule |
|---|---|---|---|
| `endpoint` | `endpoint` | `endpoint` | Required, `http://` or `https://`, stored with exactly one trailing `/` |
| `actor` | `actor` | `actor` | Required JSON object; an old Tin Can array-valued property keeps its first element |
| `registration` | `registration` | `registration` | Required for cmi5, optional for xAPI |
| `activity` | `activityId` | `activity_id` | Required for cmi5; xAPI falls back to the package activity id |
| `fetch` | `fetch` | — | Required for cmi5, `http://` or `https://` |
| `auth` | — | `auth` | Required for xAPI; the full `Authorization` header value |

Debug output and warnings never print `auth`, the token or the fetch URL. A launch error names the parameter, never its value.

### Statements

Every statement has an `id`, `timestamp`, the launch actor, a verb with an `en-US` display, and the course activity as `object` with type `http://adlnet.gov/expapi/activities/course`.

| Verb | Sent when | Result |
|---|---|---|
| `initialized` | Handshake reaches the start | none |
| `answered` | `question_answered` | `success`, `response`, `score` when points > 0, attempt extension |
| `completed` | `completion` of completed/passed/failed, or after a pass/fail | `{completion:true,duration}` |
| `passed` | `score` resolves to a pass | `{success:true,score:{scaled},duration}` |
| `failed` | `score` resolves to a fail | `{success:false,score:{scaled},duration}` |
| `terminated` | Finish, once | `{duration}` |

Verb ids are `http://adlnet.gov/expapi/verbs/{initialized,answered,completed,passed,failed,terminated}`. Durations are the ISO 8601 session time (`PT1H2M3.45S`). `scaled` is `percent / 100`.

**cmi5 context.** Each context starts as a copy of the `LMS.LaunchData` `contextTemplate`; template values are never overwritten. `registration` is set. Template `contextActivities.grouping` and `extensions` — including the session id extension `https://w3id.org/xapi/cmi5/context/extensions/sessionid` — are kept exactly.

- `initialized`, `completed`, `passed`, `failed` and `terminated` add `https://w3id.org/xapi/cmi5/context/categories/cmi5` to `contextActivities.category`, keeping existing entries.
- `completed`, `passed` and `failed` also add `https://w3id.org/xapi/cmi5/context/categories/moveon`. `initialized` and `terminated` never have it.
- `answered` keeps the template and session id but has no cmi5 or moveon category.
- `passed` and `failed` add `https://w3id.org/xapi/cmi5/context/extensions/masteryscore` when a mastery score is set. No other verb has that extension.

**Plain xAPI context.** No categories and no extensions. `registration` is written only when the launch supplied one.

**`answered`.** The object id is `{activity id}/questions/{question_id}`, stable across attempts, with type `http://adlnet.gov/expapi/activities/cmi.interaction`. Its definition holds `interactionType`, `description`, and `correctResponsesPattern`. The result is `success` and `response`, with `score.raw` (points earned), `score.min` 0 and `score.max` (question's maximum points) when points > 0, plus `extensions` `urn:coursecraft:xapi:attempt`. Responses use the SCORM 2004 / xAPI vocabulary: `[,]` between ids, `[.]` between pair members, `true`/`false`, and `{case_matters=true}` on a case-sensitive correct text pattern.

### Handshake

1. cmi5 only: POST the `fetch` URL once, with no auth header. A 200 whose JSON has a string `auth-token` gives `Authorization: Basic {token}`. Anything else — a 200 with `error-code`, another status, bad JSON or a network error — produces one warning, turns reporting off for the session, and plans `Fresh`.
2. cmi5 only: GET state `LMS.LaunchData`. Read `contextTemplate`, `launchMode` (default `Normal`), `masteryScore` (a number in 0..=1, otherwise ignored with a warning) and `returnURL` (only `http://` or `https://`, otherwise ignored with a warning). A failure produces one warning and the defaults. This document is never written.
3. Enqueue `initialized`. It is always first.
4. GET state `coursecraft.progress`. A 404 means fresh. A decodable `suspend` gives `Resume`; bad data gives `Fresh` with a warning. The saved `completed` and `passed` flags are restored either way.
5. Return the plan exactly once.

The plan is returned within `HANDSHAKE_TIMEOUT_MS` (8000) of the first tick. Past that, play starts `Fresh` with one warning, reporting stops, and late responses are ignored. When reporting is off, nothing more is sent.

### Lost reporting

When a launch attempted reporting and it is now off — a failed cmi5 fetch, a handshake timeout, or 401/403 mid-session — the bridge answers `reporting_lost()` true, and the web player shows the notice banner (see Player chrome) for the rest of the session: the translated `reportingLost` text with a dismiss button (`dismissNotice`). It is first in the keyboard focus order and in the access tree, and its text is announced once through the live region when it appears. Browse and Review keep reporting on, so they never show it; a launch with no parameters has no bridge at all, and SCORM never does this.

### Repeat rules

Enforced by the bridge in every flavor:

- `completed`: at most once per registration, using the saved flag.
- `passed`: at most once per registration, using the saved flag.
- `failed`: never after a pass, saved or in this session; at most one of `passed` or `failed` per session.
- Nothing after `terminated`, and `terminated` is sent at most once and is always last.

A score with status `Passed` or `Failed` gives one statement. With a cmi5 mastery score it passes when `scaled >= mastery`; without one, the status decides. A score with status `Completed` (a course with no quiz) gives neither, even when the LMS supplies a mastery score. A pass or fail is always followed by the completed rule, so the LRS sees the same "completed plus passed or failed" shape as the SCORM 2004 bridge.

### Browse and Review

A cmi5 `launchMode` other than `Normal` is Browse/Review: only `initialized` and `terminated` are sent. There are no `answered`, `completed`, `passed` or `failed` statements and no progress writes.

### Sending and retry

- Statements keep their order. At most one POST is in flight and one POST carries up to 50 statements as a JSON array. Statement ids come from `new_uuid` once, so a retry reuses them.
- 2xx and 409 count as sent. Any other 4xx except 401 and 403 drops that batch with a warning. 401 or 403 gives one warning and turns reporting off.
- 5xx or a network error (status 0) keeps the batch and retries after 1, 2, 4, 8, 16 then 30 seconds.
- The queue holds at most 1000 statements. Past that, the oldest statement except `initialized` is dropped with one warning.
- Every request carries `X-Experience-API-Version: 1.0.3` and `Authorization`; a request with a body also carries `Content-Type: application/json`.

### Progress document

Saved progress lives in state `coursecraft.progress` for the launch activity and registration. Its URL is `{endpoint}activities/state?stateId=...&activityId=...&agent=...&registration=...`, percent-encoded, with `registration` omitted when absent.

```json
{"suspend":"cc1:...","completed":false,"passed":false}
```

The document is PUT when dirty, at most once every 5000 ms, with at most one PUT in flight. A failed PUT stays dirty and is retried in a later window. `question_answered`, `slide_entered`, state changes, variable changes and the completed/passed flags mark it dirty.

### Finish

`terminated` is enqueued last if the handshake reached it. Everything still unacknowledged — including anything in flight — is sent at once as POST requests with `keepalive: true`, split so each body stays under 60 000 bytes. A dirty progress document is PUT with `keepalive: true` when the player state is available. Responses after this are ignored, and a second finish does nothing. If the handshake never finished, finish sends nothing beyond warnings already made.

### Host behaviour

The web player reads `coursecraft-package.json`. SCORM packages use the SCORM bridge unchanged. For cmi5 or xAPI, the player reads `window.location.search` and parses the launch parameters. A launch that fails logs one error (naming the parameter but never its value), logs the standalone line, and plays with no reporting.

While the handshake runs, the player shows the same "Loading course" screen as the asset loader and ticks the bridge every frame. The plan must arrive within `HANDSHAKE_TIMEOUT_MS` (8000) of the first tick. A `Resume` plan calls `Player::resume`, falling back to a fresh player with one warning; a `Fresh` plan calls `Player::new`. The session start timestamp is taken when the plan arrives, so time spent connecting is never counted as session time. Only then is `Input::Start` sent.

During play the bridge is ticked once per frame and after every input, and its warnings go to the console. The page registers `pagehide` and `beforeunload` once when play begins; either event finishes the session with the last player state and the elapsed session time, so a closed or navigated-away page still sends `terminated` and its final progress.

On `Effect::Exit`, after the effects are forwarded, a `returnURL` from `LMS.LaunchData` navigates the window to that URL. Without one, the player keeps its normal end-of-course behaviour.

## LTI

An LTI package states `{"standard":"lti"}` in `coursecraft-package.json`. It has the SCORM 1.2 file set without an `imsmanifest.xml` and no xAPI or SCORM manifest. The player still runs the same runtime and only reports results differently.

The web player posts each report to `../lti/score`, resolved against `index.html` so a tool mounted under a URL prefix still reaches its own server. The launch cookie is sent with the request. A report is JSON `{"percent": 0..100 | null, "status": "passed" | "failed" | "completed" | "incomplete"}`. A `score` effect gives `Some(percent)`; a `completion` effect with no score in the same input gives `percent: null`, and the server then omits `scoreGiven` and sends only the progress fields. Only the latest unsent report is kept, and an identical report already sent is ignored.

A `204` means sent. Any other status, or a network error, is retried once; a second failure stops reporting for the session and shows the lost-reporting notice banner (the same `reportingLost` chrome as an xAPI launch). LTI 1.3 has no suspend data: nothing is written to `cmi` or an LRS, and a relaunch starts the course fresh.

The launch redirect carries the learner's name as `?name=…` when the platform supplied one. The player reads it from `window.location.search` and sets `HostInfo.learner_name`, so `Learner.Name` works without a SCORM or xAPI launch. The desktop player ignores the marker, as it does for the other standards.
