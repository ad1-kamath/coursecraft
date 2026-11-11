# Course model (`course.json`, schema v1)

## Compatibility

`course.json` is a promise to the people and agents who write it by hand or by script.

- **`schema_version`** (now `1`, `SCHEMA_VERSION` in `crates/course/src/model.rs`) names the format. Files saved by any older CourseCraft always open in a newer one.
- **A change that old apps cannot read raises the version.** Adding an optional field that old apps would refuse counts: structs use `deny_unknown_fields`, so unknown fields are refused rather than ignored, and typos fail loudly.
- **A file from a newer CourseCraft may not open in an older one.** The app says so in plain words (made with a newer CourseCraft, format version N, update CourseCraft to open it) and does not show a parser error. A same-version file with an unknown field is a typo, and the message names the field.
- Fields that are not required have defaults and are left out when empty or default, so saved files stay short and diff cleanly. The shipped examples under `examples/` are tested to open and validate.
- To see what changed between two versions of a course, run `coursecraft-cli diff <a> <b>` (see `docs/cli.md`).

Source of truth: `crates/course/src/model.rs`. Example: `examples/hello-course/course.json`. Unknown fields are rejected, so typos fail loudly.

```text
Course
├─ schema_version (1), id, title, slide_size {w,h}=1280×720
├─ theme?           {accent="#2F6FEB", background="#FFFFFF", text="#1F1F1F", font="Inter",
│                    player {bar="#1B1B1B", bar_text="#FFFFFF"}}
├─ variables[]      {name, value: bool|number|text}
│                   (User variables use [A-Za-z0-9_-]; read-only built-in variables
│                    like Course.Progress and Slide.ElapsedMs are computed by logic)
├─ quiz?            {passing_percent}
├─ player           {show_nav, show_menu, show_seekbar, show_speed, show_fullscreen (default true),
│                    show_transcript=false (notes shown as a transcript tab),
│                    show_title, show_volume, show_captions (default true),
│                    logo? (project-relative image src, shown in the strip, max 28 pt high),
│                    navigation=free (free|restricted|locked),
│                    next_requires={"kind":"nothing"} (nothing|timeline_ends|variable{name})}
├─ resources[]?     {id, title, target: {kind:"url", url (http|https|mailto)} | {kind:"file", src}}
│                   (omitted when empty; a file is a project-relative path packaged under media/)
├─ glossary[]?      {id, term, definition} (omitted when empty; shown A to Z, stored in author order)
├─ text_styles[]?   {id, name, style: TextStyle, paragraphs?: Paragraph} (omitted when empty; at most 50)
├─ language?        BCP 47 tag for learner-facing text, e.g. "fr-CA" (default: en)
├─ strings          player chrome overrides keyed by CHROME_STRINGS (omitted when empty)
├─ fonts[]?         {family, style: regular|bold|italic|bold_italic, src} (omitted when empty)
├─ background_audio? {src, volume=0.5, loop=true} (omitted when unset)
├─ start_slide?     slide id (default: first slide)
├─ scenes[] {id, title}
   └─ slides[] {id, title, duration_ms=5000, background?, transition?, notes?, master?, revisit=auto, draw?, player?}
      ├─ objects[]      base layer, bottom → top (timing, states, entrance?, exit?, motion_paths[])
      ├─ layers[] {id, name, duration_ms, hide_other_layers=true,   (back → front)
      │            pause_base_timeline=false, prevent_base_clicks=false,
      │            hide_base_objects=false, hide_when_timeline_ends=false,
      │            allow_seek=auto, revisit=auto}
      │   ├─ objects[]
      │   └─ triggers[]
      ├─ triggers[]     base-layer triggers
      └─ question?  {kind, prompt, choices[{id,text,correct,points=0}], points=10,
                     scoring=all_or_nothing (all_or_nothing|per_answer|partial),
                     attempts=1 (0 = unlimited), shuffle, correct_layer?, incorrect_layer?,
                     accepted[]?, case_sensitive?, pairs[{id,left,right}]?}
├─ masters[]?    Slide (omitted when empty)
├─ templates[]?  Slide (omitted when empty)
└─ banks[]?      {id, title} (omitted when empty)
   └─ slides[]   Slide (every slide has a question)
```

## Objects

`{id, name, kind, frame {x,y,w,h}, timing {start_ms, end_ms?}, states[], initial_state="normal", group?, panel?, rotation?, flip_h?, flip_v?, opacity?, shadow?, glow?}`

| `kind.type` | fields |
|---|---|
| `text` | `text`, `style {font, size, color, bold, italic?, underline?, strikethrough?, baseline?, letter_spacing?, align?, valign?}`, `runs? [{text, bold?, italic?, underline?, strikethrough?, baseline?, letter_spacing?, size?, color?, font?, link?}]`, `paragraphs? [Paragraph]`, `autofit?` |
| `shape` | `shape` (see [Shapes](#shapes)), `fill`, `fill_style?`, `outline? {color, width, dash?}`, `text? {text, style, runs?, paragraphs?, autofit?}`, `adjust? [number]` |
| `image` | `src`, `alt`, `crop?` |
| `audio` / `video` | `src`, `autoplay`, `captions?`, `volume?`, `trim?`, `cues[]?` (video only, see [Interactive video cues](#interactive-video-cues)) |
| `lottie` | `src`, `loop?`, `autoplay?`, `speed?` |
| `web` | `source` (see [Web objects](#web-objects)) |
| `text_entry` | `variable`, `placeholder?`, `multiline?`, `max_chars?` (default 0 = no limit, at most 10000) |
| `slider` | `variable`, `min`, `max`, `step` (0 = continuous), `orientation?` (`horizontal` default, `vertical`) |
| `dial` | `variable`, `min`, `max`, `step`, `start_angle?` (default -135), `end_angle?` (default 135) |
| `checkbox` | `variable`, `label?` |
| `radio_group` | `variable`, `options [{label, value}]` (2..=20 unique non-empty values) |
| `marker` | `title`, `body`, `runs?`, `paragraphs?`, `icon?` (`info` default, `plus`, `question`, `star`, `pin`), `color?` (theme accent default) |
| `scroll_panel` | `content_height` (slide px), `bar?` (`visible` default, `hidden`) |
| `timer` | `duration_ms` (1..=86400000, default 60000), `format?` (`mm_ss` default, `seconds`, `hidden`), `autostart?` (default true), `style` (text box style) |
| `progress_meter` | `variable`, `min`, `max`, `style?` (`bar` default, `ring`), `show_value?` (default false) |
| `button` | `label`, `fill`, `fill_style?` |
| `path` | `points [{x,y}]`, `style?`, `closed?`, `fill?`, `outline {color, width, dash?}`, `start_end?`, `end_end?`, `connector?` |
| `table` | `columns [number]`, `rows [{height, cells [{text, style?, runs?, paragraphs?, fill?, row_span?, col_span?, valign?}]}]`, `borders?`, `header_row?` |

Colours are `#RRGGBB` or `#RRGGBBAA`. `style.font`, a run's `font` and `theme.font` name `Inter` (the legacy `Open Sans` also means Inter) or any family in `course.fonts`; an unknown name falls back to Inter and validation reports a warning. `style.italic`, `style.underline` and `style.strikethrough` default to false and are omitted when false. `style.baseline` is `normal` (default, omitted), `superscript` or `subscript`; those draw at 65% size, raised or lowered. `style.letter_spacing` is extra space after each letter in points, default 0 (omitted), repaired to 0 when not finite and clamped to -5..=50. A run may override each of the three; omitted overrides inherit the style. `style.align` defaults to `left` (`left`, `center`, `right`) and `style.valign` defaults to `top` (`top`, `middle`, `bottom`), omitted when default. Media `src` (such as `Image.src`) is a relative path under the project's `media/` folder, and the editor stores imported files there.

Audio and video may set `captions` to a WebVTT file path relative to `media/`, for example `"captions": "intro.vtt"`. The path follows the same safe-relative-path rule as `src` and must end in `.vtt` (case-insensitive); validation rejects anything else and names the object. The field is omitted when unset, so existing courses stay byte-identical. Captions are presentational: they are never part of `coursecraft-logic` state and never send inputs.

### Trim and volume

Audio and video objects may carry `volume` and `trim`; both are omitted from JSON when default, so existing courses stay byte-identical:

```json
{ "type": "audio", "src": "narration.wav", "volume": 0.8, "trim": { "start_ms": 2000, "end_ms": 9500 } }
```

- `volume` is a finite number in `0.0..=1.0`, default `1.0`. An absent value means full volume. A non-finite or out-of-range value is a validation error, not repaired.
- `trim` has `start_ms` and an optional `end_ms`, both at most 86400000 (24 hours). `end_ms` is omitted for "to the end of the file"; when present it must be greater than `start_ms`.
- Trim times are in file time. A trimmed object starts at `start_ms`; the host sees `media_seek { object, ms: start_ms }` after the runtime's `media { Play }`, and playback ends when the host reports `media_time` at or past `end_ms`. Cue points also stay in file time. See [triggers.md](triggers.md#media).
- `object.set_media` takes `{slide, object, volume?, trim?}` and is one undo step. Absent fields keep their value and `trim: null` clears it. It refuses objects that are not audio or video.

### Background audio

A course may carry one track that plays across every slide:

```json
{ "background_audio": { "src": "music.mp3", "volume": 0.35, "loop": false } }
```

- `src` is a non-empty relative path under `media/` following the same safe-relative-path rule as every other media source.
- `volume` is a finite number in `0.0..=1.0`, default `0.5`. `loop` defaults to `true`. Both are omitted from JSON when default, so a course without background audio serializes unchanged.
- The runtime addresses it with the reserved media object id `Course.BackgroundAudio`. The dot keeps it apart from every valid user object id; `play_media`, `pause_media`, `stop_media` and `media_ends` accept it only when the course has background audio. See [triggers.md](triggers.md#background-audio).
- The player resolves the reserved id to this file and its volume; publishing packages the file like any other media reference.

### Lottie animations

A `lottie` object plays a `.json` or `.lottie` file under `media/` through the shared media runtime. `loop` defaults to `true`, `autoplay` to `true`, and `speed` to `1`; all three are omitted from JSON when default, so existing courses stay byte-identical.

- `speed` is finite and clamped into `0.1..=4` when a file loads and when `object.set_lottie` stores it. The player advances the animation at `elapsed × speed`.
- Triggers `play_media`, `pause_media` and `stop_media` act on a Lottie object exactly as they do on audio and video. An `autoplay` animation starts when its timeline reaches `timing.start_ms`. `media_ends` fires at the end of a non-looping run; a looping animation never ends.
- Playback position belongs to `coursecraft-logic` (`PlayerState.media_clocks`), is runtime-only, and is never encoded in `suspend_data`. A paused animation keeps its position; a stopped one forgets it and shows frame 0 again.
- The animation is fitted into the object frame preserving its aspect ratio, then rotated, flipped and faded like any object. The canvas, thumbnails and static renders show frame 0; preview and the player follow the media clock, and offscreen player mode advances it with `--at-ms`.
- A `.lottie` file is a zip containing `animations/*.json` and `manifest.json`; the first animation entry is used. Images embedded in the animation are not drawn in this step.
- Hostile-input limits: the file is at most 10 MiB, JSON depth and element count are bounded before parsing, the composition is at most 4096×4096, at most 240 fps and 100,000 frames, and at most 5,000 layers. A file that fails shows a placeholder with the reason; it never crashes.
- Parsed compositions are cached per `src`. Rendering stops with a warning when one frame would exceed 120,000 triangles. Fills use the shape code's even-odd triangulator (non-zero is emulated by dropping same-winding nested contours); Lottie masks clip to the mask path's bounding box.
- The editor's Insert ▸ Media menu accepts `.json` and `.lottie`, sizes the new object from the composition and shows Loop, Autoplay and Speed in the inspector. `object.set_lottie` takes `{slide, object|objects, loop?, autoplay?, speed?}` and is one undo step.

### Interactive video cues

A `video` object may carry cue points, omitted when empty so existing courses save unchanged:

```json
{ "type": "video", "src": "intro.mp4", "cues": [
  { "id": "cue1", "at_ms": 4000, "name": "Pause for a question", "pause": true }
] }
```

- `at_ms` is milliseconds from the start of the video. It is finite, at least 0, and within the video's known duration when that duration is known (the owning slide or layer duration, the only duration in the model). Cues are stably sorted by `at_ms` on load; a non-finite or negative time is repaired to 0.
- `id` is unique within the object, and `name` is non-empty. At most 200 cues are allowed. `pause` is omitted when false. A pausing cue stops playback and holds the video until a trigger resumes it.
- The event `cue_reached { object, cue }` fires when playback crosses the cue going forward. See [triggers.md](triggers.md#interactive-video-cues) for the runtime rules.
- `video.add_cue`, `video.set_cue`, `video.remove_cue` and `video.add_question_cue` edit cues in one undo step. Deleting a cue also deletes every `cue_reached` trigger naming it in the same undo step. `video.add_question_cue` creates a pausing cue, a layer containing a question layout and the triggers that show the layer and resume the video. The layer hides the base objects, because the video element draws above the slide and would otherwise cover the question.

### Web objects

A `web` object shows a page in the published course. It has one of two sources, tagged like other enums:

```json
{ "type": "web", "source": { "type": "url", "url": "https://example.com/lesson" } }
{ "type": "web", "source": { "type": "folder", "path": "web/site", "entry": "index.html" } }
```

- A URL source must start with `https://` and name a non-empty host. `http://`, missing hosts, user info and whitespace are validation errors.
- A folder source is a directory copied into the project's `media/` folder as `web/<name>/`. `path` is relative to `media/` and must be a safe path under `web/`; `entry` is a safe relative file inside it and defaults to `index.html` (omitted from JSON when default). Absolute paths, `..`, backslashes and empty segments are rejected. A folder is limited to 500 files and 100 MiB; the copy step enforces both caps and skips hidden files and symlinks, and validation rejects a missing or unsafe entry.
- The editor copies a chosen folder into media with those caps and inserts the object through `object.add`. `object.set_web_source` takes `{slide, object|objects, source}` and is one undo step.
- On the authoring canvas, thumbnails, desktop player, preview and offscreen renders the object is a placeholder card showing a globe, the URL host or folder name and "Plays in the published course"; pages are never fetched for that preview. Only the published web player draws a real `<iframe>`.
- The iframe is sandboxed (`allow-scripts allow-same-origin allow-forms allow-popups`), uses `referrerpolicy="no-referrer"` and `loading="lazy"`, and takes its accessible title from the object's name. Folder content loads from `media/web/<name>/<entry>` relative to the player. Like the web video element it is positioned over the object's frame and hidden off-slide, while hidden, or during a transition. Object rotation and flips are ignored for the iframe; the frame still follows scroll, zoom and letterboxing.
- Publishing copies every file in the folder, keeps its relative path (`media/web/<name>/...`), includes those files in the manifest, and blocks with a missing-media error when the entry or folder is absent. `.ccraft` save keeps the whole folder, not just the entry, and the whole folder survives undo.

### Input objects

An input object writes a learner's change into a course variable. Every input names one variable, and the variable's type fixes what the input may write:

| Kind | Variable type | Writes |
|---|---|---|
| `text_entry { variable, placeholder?, multiline?, max_chars? }` | text | the typed text, truncated to `max_chars` characters when non-zero |
| `slider { variable, min, max, step?, orientation? }` | number | the chosen number, clamped to `min..=max` and snapped to `step` from `min` |
| `dial { variable, min, max, step?, start_angle?, end_angle? }` | number | the same, chosen by the pointer angle |
| `checkbox { variable, label? }` | true/false | the checked state |
| `radio_group { variable, options }` | text | the selected option's `value` |

- `placeholder`, `multiline`, `max_chars`, and all numeric fields are omitted when default. `step` of zero means continuous. `orientation` is `horizontal` (default) or `vertical`; vertical sliders put `min` at the bottom.
- Dial angles are clockwise degrees from 12 o'clock. They must be finite, within `-360..=360`, and `start_angle < end_angle`. Defaults are -135 and 135. Non-finite loaded angles become their defaults; non-finite `min`/`max`/`step` become 0, 100 and 0.
- `min` and `max` are finite and `min < max`; `step` is finite and at least zero. A text entry's `max_chars` is at most 10000. A radio group has 2 to 20 options whose values are non-empty and unique.
- A missing variable, or one of the wrong kind, is a validation error. `disabled` is the built-in state: a disabled input ignores changes and draws dimmed.
- The input's displayed value is read from its variable every frame, so a `set_variable` action or a `%var%` reference moves a slider, checks a box or changes the text. The authoring canvas shows the variable's default value and is not interactive.
- Text entry commits on Enter (single-line) or when focus leaves. Slider and dial commit continuously while dragging and on arrow keys; checkbox on click or Space; radio on click or arrows within the group.
- `object.set_input {"slide", "object"|"objects", "patch"}` edits the kind's own fields in one undo step. The inspector also offers a variable picker filtered to the right kind, with "New variable…" creating one in the same batch. See [triggers.md](triggers.md#input-objects).

### Draggable objects

Any non-question object may carry `drag`, omitted when unset, so existing courses stay byte-identical:

```json
{ "targets": ["bin-paper", "bin-glass"], "snap": true, "return_on_miss": true }
```

| field | meaning |
|---|---|
| `targets` | Object ids on the same slide the object may be dropped on. Empty means every drop is a miss. Omitted when empty. |
| `snap` | Centre the object on the target it hits. Default true, omitted when true. |
| `return_on_miss` | Put the object back where it was before the drag when no target is hit. Default true, omitted when true. |

Validation rejects a target id that is not on the slide, and a `drag` spec on a question widget (the hotspot image, a drag item or a drop target). `object.set_drag {"slide", "object"|"objects", "drag": DragSpec|null}` sets or clears it in one undo step. Runtime rules are in [triggers.md](triggers.md#draggable-objects).

### Markers

A `marker` is a round dot in `color` (the theme accent when omitted) carrying a small icon. Its `title` and `body` use the same rich-text model as a text box: `body`, optional `runs` and optional `paragraphs`. `icon` is `info` (default), `plus`, `question`, `star` or `pin`, and is omitted from JSON when `info`. `color` is `#RRGGBB`/`#RRGGBBAA` and omitted when the theme accent is used.

- Clicking or focusing the dot opens a label card beside it; clicking it again, pressing Escape, or clicking elsewhere closes it. Only one marker on a slide is open at a time. See [triggers.md](triggers.md#markers).
- The card draws above other objects, grows from its text and is at most 360 slide pixels wide. It is placed on whichever side of the dot fits inside the slide.
- On the authoring canvas the selected marker shows its card. The card is authoring-only there; the player shows it while open.
- `object.set_marker {"slide", "object"|"objects", "patch": {title?, body?, runs?, paragraphs?, icon?, color?}}` edits it in one undo step. `color: null` returns to the theme accent. Supplying `body` without `runs` clears the runs.
- `marker_opened` and `marker_closed` events can drive triggers. XLIFF and find/replace cover the title and body.

### Scrolling panels

A `scroll_panel` clips a group of child objects to its frame and scrolls them inside it.

```json
{ "type": "scroll_panel", "content_height": 1200, "bar": "visible" }
```

- `content_height` is the total scrollable height in slide pixels, at least the panel's frame height and at most 20000. Non-finite values are repaired to 0 on load.
- `bar` is `visible` (default, omitted) or `hidden`.
- Children stay in the slide's normal object list, in the same layer. `Object.panel` names the panel they scroll inside; it is omitted when the object is free. The panel itself cannot be inside another panel.
- A child's `frame` is authored at scroll 0, relative to the slide. Its y may run past the panel's bottom, up to `content_height` below the panel's top.
- A child whose frame is wholly outside the panel viewport at the current offset is clipped, is not hit-testable, is not in the keyboard focus order and is not announced.
- `object.set_panel {"slide", "object"|"objects", "panel": id|null}` sets or clears membership; `object.set_scroll_panel {"slide", "object"|"objects", "patch": {content_height?, bar?}}` edits the panel. Both are one undo step. Deleting a panel clears `panel` on its children in the same undo step.
- In the editor the panel draws its frame with children shown unclipped, plus a faint outline at the content height, so every child can be selected. Dragging an object over a panel does not parent it.
- Offscreen renders use offset 0. See [triggers.md](triggers.md#scrolling-panels).

### Timers and progress meters

A `timer` counts down from `duration_ms` in `coursecraft-logic` while its owning timeline runs (see [triggers.md](triggers.md#timers)).
```json
{ "type": "timer", "duration_ms": 90000, "format": "mm_ss", "autostart": true,
  "style": { "font": "Inter", "size": 24, "color": "#1F1F1F", "bold": false } }
```
- `duration_ms` is 1..=86400000 (24 hours), default 60000. Out-of-range values are clamped on load.
- `format` is `mm_ss` (default, `minutes:seconds` with minutes rolling up past 60, e.g. `90:00`), `seconds` (a plain whole-second count), or `hidden` (drawn as nothing in the player and as a small labelled placeholder on the canvas).
- `autostart` (default true) begins the countdown when the owning timeline's clock reaches the object's `timing.start_ms`. A timer that does not autostart waits for `start_timer`.
- `style` is the same text style a text box uses; the digits are padded (`05:00`) so they do not jitter as the value changes. The canvas and offscreen renders show the full duration and do not run.
- The read-only variable `Timer.<object id>.Remaining` holds the whole seconds remaining, rounded up. See [triggers.md](triggers.md#timers).

A `progress_meter` draws a number variable between `min` and `max` (finite, `min < max`, like a slider):
```json
{ "type": "progress_meter", "variable": "score", "min": 0, "max": 100, "style": "bar", "show_value": false }
```
- `style` is `bar` (default, fills left to right) or `ring` (fills clockwise from 12 o'clock). `show_value` (default false) centres the percentage in the meter.
- The fill is the theme accent over a design-token track. Values outside the range clamp; the displayed value is read from the variable every frame, so a `set_variable` moves the meter on the canvas, in preview, in the player and in offscreen renders.
- `object.set_timer {"slide", "object"|"objects", "patch": {duration_ms?, format?, autostart?, style?}}` and `object.set_meter {"slide", "object"|"objects", "patch": {variable?, min?, max?, style?, show_value?}}` edit them in one undo step. The editor's Insert ▸ Interaction menu has Timer and Progress meter; the meter creates a new number variable, like the inputs do.

### Interactions

Tabs and accordions are **insertable interactions**, not object kinds. `interaction.insert {"slide": id, "kind": "tabs"|"accordion"}` builds them from ordinary buttons, layers and triggers in one undo step and returns `{"objects": [base button ids], "layers": [layer ids], "all_objects": [every inserted id]}`. The editor selects the base buttons afterwards, so the author can edit everything like any other content.

- **Tabs.** Three buttons across the top, each with a `selected` state in the theme accent, and one layer per tab whose text starts `Content for Tab 1`, and so on. Clicking a tab shows its layer and sets that button's `selected` state while resetting the others to `normal`; the first layer is shown by a `timeline_starts` trigger.
- **Accordion.** Three header buttons stacked on the base layer and one layer per open section. States cannot override an object's frame, so each open-section layer carries its own copy of the headers: the opened section's content sits directly under its header and the headers below it are moved down by the content height. Every layer sets `hide_base_objects` and `prevent_base_clicks`, shows the same headers, opens another section when one is clicked, and hides itself when the open section's own header is clicked.

### Rotation and flips

An object may set `rotation` (clockwise degrees), `flip_h` and `flip_v` (mirror about the frame centre). All three are optional and omitted when default, so old files load and save unchanged.

- `rotation` is normalised into `0..360` when loaded and when a command stores it. A non-finite value in a loaded file is repaired to 0. A value of 0 is omitted.
- `flip_h` and `flip_v` default to false and are omitted when false.
- Rotation and flips are presentational: they are drawn identically on the canvas, in preview, in the player and in the offscreen renderer. Audio objects ignore them because they have no visible body in the player.
- The item is painted as usual, then its meshes are rotated and mirrored about the frame centre. Text mirrors with the object, as it does in PowerPoint. Animations and motion paths apply to the already-rotated item.
- Hit testing, hover, the focus ring and drag-and-drop targets use the exact rotated frame. Marquee selection, smart guides and snapping use the rotated bounding box.
- `object.set_rotation` takes `{slide, object|objects, degrees}`; `object.flip` takes `{slide, object|objects, axis}` where `axis` is `horizontal` or `vertical` and flips. Both are undoable and act on every object in `objects`.
- PowerPoint import keeps `a:xfrm@rot` (60000ths of a degree), `flipH` and `flipV` on shapes, pictures and connectors. A group's rotation adds to its children's and rotates their centres about the group centre; a group's flips also compose into the children.

### Opacity, fills and effects

`opacity` is a number in `0..=1`, omitted when 1. It multiplies every colour the object draws (fill, gradient stops, picture, outline, text and effects) and multiplies with entrance and exit fades. A non-finite value is repaired to 1 and an out-of-range value is clamped when a file loads.

Shapes and buttons keep `fill` as their solid colour. That colour is the fallback, what a state override replaces, and what is drawn when a picture is missing. `fill_style` is optional and omitted when the fill is solid:

```json
{ "type": "linear", "angle": 45, "stops": [
  { "color": "#FF0000", "position": 0 }, { "color": "#0000FF80", "position": 1 } ] }
{ "type": "radial", "center": [0.5, 0.5], "stops": [/* the same stops */] }
{ "type": "picture", "src": "texture.png", "mode": "stretch" }
```

- `linear.angle` is clockwise degrees from left-to-right, normalised into `0..360`.
- `radial.center` is `[x, y]` as fractions of the frame, each clamped into `0..=1`.
- `stops` has 2 to 10 entries. Each `position` is a fraction in `0..=1`; a loaded file is clamped and stably sorted by position, and extra stops are dropped. A stop with an unreadable colour is treated as the solid fill.
- `picture.src` is a media path under `media/`, like an image's `src`. `mode` is `stretch` (default) or `tile`. A picture fill counts as media: it is kept on `.ccraft` save, checked for missing files, included when publishing, and reported by find-unused. An empty or missing picture draws the solid `fill`; an empty `src` is a validation warning.
- A state whose `fill` is set draws that solid colour and overrides `fill_style` for as long as the state is current. Other states do not change the fill.

`shadow` draws the object's silhouette behind it, offset by `offset_x`/`offset_y` slide pixels, in `color` (its alpha is used), with `blur` slide pixels. `glow` draws the same silhouette without an offset, expanded by `size`. Both are omitted when unset and rotate and flip with the object. Offsets are bounded to `-200..=200`; blur and size to `0..=100`. Non-finite values are repaired and out-of-range values clamped on load. Blur is approximated with up to six layered, increasingly transparent copies, so cost stays bounded. Text boxes get their shadow and glow on the glyphs.

The engine commands are `object.set_opacity {"opacity": number}`, `object.set_fill_style {"fill_style": FillStyle|null}`, `object.set_shadow {"shadow": Shadow|null}` and `object.set_glow {"glow": Glow|null}`. Each takes `object` or `objects`, works on a multi-selection, and is one undo step. Clearing `fill_style`, `shadow` or `glow` is `null`.

### Custom fonts

An author can add TrueType or OpenType files with File ▸ Add font…, or through the font picker's "Add font…" entry. Each file is copied into the project's media as `fonts/<safe-name>.ttf`/`.otf` and registered in `course.fonts`; the family and style come from the file's `name` table (nameID 16/17 when present, else 1/2). The list is omitted when empty, so existing courses stay byte-identical.

```json
"fonts": [
  { "family": "Lato", "style": "regular", "src": "fonts/lato-regular.ttf" },
  { "family": "Lato", "style": "bold", "src": "fonts/lato-bold.ttf" }
]
```

- `style` is `regular`, `bold`, `italic` or `bold_italic`. A family may register any subset of faces.
- At most 40 faces per course. A family name is trimmed, stripped of control characters, and 1–64 characters. `(family, style)` is unique, compared case-insensitively.
- `src` is a safe relative media path under `fonts/` ending in `.ttf` or `.otf`. The file must parse as TrueType/OpenType and be at most 15 MiB. The editor validates the bytes before copying them and the `course.add_font` command enforces the model rules.
- `TextStyle.font`, `TextRun.font` and `Theme.font` may name `Inter` or a registered family. A run's `font` overrides its style. Rendering resolves the exact `(family, bold, italic)` face, then the family's `regular` face, then Inter. An unknown or missing family renders in Inter.
- Fonts are media: they are kept by `.ccraft` save/load, checked for missing files, and shipped by every publish standard. The player loads them from `media/`.
- `course.add_font` takes `{src, family, style}` and is undoable. `course.remove_font` takes `{family, style?}` or `{src}` and is undoable. Removing a font used by text is confirmed by the editor first; that text then falls back to Inter.
- PowerPoint import keeps each run's `a:latin@typeface` in `TextRun.font` and reports fonts used by the deck or its theme (`a:majorFont`/`a:minorFont`) that are not Inter: "Add these fonts to keep the look: …". Adding them later makes the imported text render in them.

### Shapes

A shape is a closed fill with an optional outline and optional text. Detail is cut out as gaps. `shape` is one of the kinds in `crates/course/src/shapes.rs` (`SHAPE_CATALOG`), which is also the source of the gallery, labels, categories and PowerPoint mapping:

| Category | JSON `shape` names |
|---|---|
| Rectangles | `rectangle`, `rounded_rectangle`, `snip_single_corner`, `snip_same_side`, `snip_diagonal`, `snip_round`, `round_single_corner`, `round_same_side`, `round_diagonal` |
| Basic Shapes | `ellipse`, `triangle` |
| Block Arrows | `arrow_right`, `arrow_left`, `arrow_up`, `arrow_down`, `arrow_left_right`, `arrow_up_down`, `arrow_quad`, `arrow_left_right_up`, `arrow_bent`, `arrow_u_turn`, `arrow_left_up`, `arrow_bent_up`, `arrow_curved_right`, `arrow_curved_left`, `arrow_curved_up`, `arrow_curved_down`, `arrow_striped_right`, `arrow_notched_right`, `arrow_pentagon`, `arrow_chevron`, `callout_right_arrow`, `callout_down_arrow`, `callout_left_arrow`, `callout_up_arrow`, `callout_left_right_arrow`, `callout_quad_arrow`, `arrow_circular` |

The later catalog groups — Lines, Equation Shapes, Flowchart, Stars and Banners, Action Buttons and Learning — arrive additively. New kinds are one enum variant, one catalog row and one geometry arm. The first batch's arrow-callout kinds sit in Block Arrows until the full Callouts group lands.

Geometry is computed for the object's frame. Details such as corner radii and arrowheads measure from `min(w, h)`, so an arrowhead keeps its proportions when the frame is stretched. Contours are filled with the even-odd rule, so a contour inside another is a hole. The JSON names stay snake_case; `schema_version` stays 1 because the change is additive, but an older build cannot open a course that uses a new kind.

Hit testing stays the object's frame (rotated and flipped when the object is), not the visible contour, so a click in a hole or beside a concave edge still selects the shape. PowerPoint import maps `prstGeom/@prst` names through each catalog row's `ooxml` list; unknown preset geometry keeps the old custom-geometry fallback. Rotation and flips import as described above.

`adjust` carries the kind's adjustment values, in the order its handles appear. Values are unitless and clamped per kind when a command stores them and again when they are drawn; non-finite values fall back to the kind's default and extra values are ignored. Callout tail points are fractions of the width and height and may sit outside the frame; angle values are radians. An empty list means the kind's defaults, and the field is omitted when empty, so a plain shape saves exactly as before. PowerPoint `a:avLst/a:gd` values import where the preset has a known mapping (most are 1/100000 of `min(w, h)`, star inner radii are 1/50000 of the outer radius, and angles are 60000ths of a degree); unknown presets keep the defaults. A rectangular, rounded or oval callout is a single contour with its wedge spliced into the nearest side, so an outline never draws through the bubble.

### Shape outlines, no fill and text

A shape may set `outline` to `{color, width, dash}`. `color` is a normal `#RRGGBB`/`#RRGGBBAA` colour, `width` is in slide pixels and must be finite and within `0..=50`, and `dash` is `solid` (default), `dash` or `dot`. The outline is a band centred on the shape's edge, including a hole's edge; it is a mesh, not an `egui::Stroke`, so the wasm player and the offscreen rasteriser draw it the same way. Miter joins fall back to a bevel past a limit. Non-finite or zero widths draw nothing. `Lines` kinds keep their filled bands and take neither an outline nor text.

"No fill" is `fill: "#00000000"`; the shape then draws only its outline and text. `outline` and `text` are omitted when unset, so a plain shape saves exactly as it did before these fields existed.

A shape may set `text` to `{text, style, runs?, paragraphs?, autofit?}` using the same style, run, paragraph and autofit rules as a text object. It moves, animates, translates and imports with the shape. `%name%` substitution, `StateDef.text` overrides, alignment, runs and the accessibility name all treat it as the shape's own text. A `Lines` shape cannot carry text. Text may overflow the shape, as it does in PowerPoint, and sits in a per-kind text rectangle: the frame inset by 6 px by default, with overrides for ellipses, diamonds, triangles, callouts, arrows, stars, flowchart shapes, cloud and heart. A PowerPoint autoshape's `txBody` imports into this field; a plain text box stays a `text` object.

### Paths

A `path` is a drawn line, curve, freeform, scribble or connector. Points are fractions of the object's frame (`0..=1`), so resizing the frame scales the drawing without changing `points`. A frame's width or height may be as small as one pixel: that is how a horizontal or vertical line is stored. A path may extend outside the slide exactly as other objects may.

| field | meaning |
|---|---|
| `points` | 2 to 2000 `{x, y}` fractions in `0..=1` |
| `style` | `straight` (default) joins points with straight segments; `curved` is a Catmull-Rom curve through them |
| `closed` | join the last point to the first; default false, omitted |
| `fill` | `#RRGGBB`/`#RRGGBBAA`, only on a closed path; omitted otherwise |
| `outline` | `{color, width, dash?}` with the shape outline rules: width `0..=50` slide px |
| `start_end` | `none` (default), `arrow`, `open_arrow`, `dot` or `diamond` |
| `end_end` | the same values at the far end |
| `connector` | `straight`, `elbow` or `curved`; only a connector uses the next two fields |
| `start_glue` | `{object, site}` attaching the start end to an object's frame; omitted when free |
| `end_glue` | `{object, site}` attaching the end end; omitted when free |

A glue `site` is 0 top centre, 1 right centre, 2 bottom centre or 3 left centre; other values are read as the nearest valid one (`site % 4`). A connector's routed geometry is stored in `points`, so the player draws the stored polyline and never routes at play time. Re-routing happens once after every successful engine command; moving a target slides or resizes its glued ends and leaves a free end where it is. A target that is removed, moves to another owner (base or another layer), or is itself a connector loses its glue and the end stays put.

A path has no text. Its `description` is its accessible name; a path with no description is ignored by assistive technology, like an unnamed shape. `StateDef.fill` overrides a closed path's fill; other state overrides do not apply. Text localisation ignores paths except for their descriptions.

Straight joins points directly. Curved is a Catmull-Rom spline converted to cubic Béziers and sampled adaptively with a hard cap. A connector's `points` hold its routed result: straight is a direct line, elbow leaves each glued site along its side for at least 20 px and joins with right-angle segments, and curved uses the site directions for its cubic controls. Near-duplicate points are dropped before geometry, so hostile files cannot divide by zero. Line ends are sized from the stroke width with a minimum size; a filled head stops the band short so a translucent colour does not double up.

A closed path fills with the even-odd rule. A self-intersecting or otherwise degenerate path may fill nothing; its outline still draws and nothing panics or hangs.

`object.set_path` takes `points` in slide coordinates and rewrites both `points` and the object's frame, so tools and agents never duplicate the conversion. It refuses fewer than two, more than 2000, or non-finite points. Its optional `start_glue` and `end_glue` params set or (with `null`) clear one end; omitting a param leaves that end's glue alone. Setting either on a path that is not a connector clears both. `object.add`, `object.update`, `object.set_frame`, duplicate, paste, groups, align, distribute and motion paths all work on the frame as they do for other objects.

PPTX import maps `p:cxnSp` and `prstGeom prst="line"` to two-point paths, and `a:custGeom/a:pathLst` to a flattened path. For a connector, `flipH` and `flipV` choose which corners its ends sit on; its `rot` is kept as the object's rotation. Group rotation and flips also compose into connector ends and centres. An `a:stCxn` or `a:endCxn` whose target imported as an object on the same slide becomes glue; the end keeps its imported position until a later edit re-routes it. Rectangle-like targets map `idx` 0 top, 1 left, 2 bottom, 3 right to our sites; other targets use the nearest of our four sites. With several custom subpaths only the first imports and the report says so.

### Tables

A `table` is a grid of rich-text cells with fills and borders.

```json
{
  "type": "table",
  "columns": [0.25, 0.5, 0.25],
  "rows": [
    { "height": 0.4, "cells": [
      { "text": "Name", "fill": "#2F6FEB" },
      { "text": "Merged", "col_span": 2 }
    ] },
    { "height": 0.6, "cells": [
      { "text": "Ada" }, { "text": "" }, { "text": "10" }
    ] }
  ],
  "borders": {
    "outer": { "color": "#1F1F1F", "width": 2 },
    "inner": { "color": "#808080", "width": 1 }
  },
  "header_row": true
}
```

| field | meaning |
|---|---|
| `columns` | 1 to 50 positive fractions of the width, normalised to sum to 1 on load |
| `rows` | 1 to 50 rows; at most 2500 cells in total |
| `rows[].height` | positive fraction of the height, normalised with the other rows |
| `rows[].cells` | exactly one entry per column; a cell covered by a merge is kept but empty |
| `cell.text` / `style` / `runs` / `paragraphs` | the same rich-text model as a text box; `valign` is top, middle or bottom |
| `cell.fill` | optional `#RRGGBB`/`#RRGGBBAA`; omitted means no fill |
| `cell.row_span` / `cell.col_span` | merge size, 1 to the table bounds; omitted when 1 |
| `borders.outer` / `borders.inner` | optional `{color, width, dash?}` with the shape outline rules |
| `header_row` | bold the first row and give unfilled header cells a 15 % theme-accent wash; omitted when false |

Non-finite or negative fractions are repaired to 0 and then normalised; an all-zero or empty list becomes equal fractions. Spans must stay inside the grid. Covered cells are kept in the JSON so every row still has one entry per column, but they are empty and are not drawn. Text wraps inside the cell with 6 slide pixels of padding; a row keeps its fraction and overflowing text is clipped in the player and marked on the editor canvas. Rotation, flips, opacity, shadow and glow apply to the whole table. On the web the access layer emits a real table, with header-row cells as column headers, and keyboard focus treats the table as one stop. XLIFF and find/replace cover each cell's text.

### Image crops

An `image` may set `crop` to `{x, y, w, h}`, the kept window of its source picture in fractions of the source (`0..=1`). `x` and `y` are where the window starts and `w` and `h` its size. The field is omitted when unset, and a full-source window is stored as no crop, so existing courses stay byte-identical.

Drawing only changes for a real crop: an image with no crop is still drawn aspect-fit and centred in its frame, so a frame whose proportions differ from the picture's letterboxes it. A kept window fills its frame.

Validation rejects a crop that is non-finite, outside `0..=1`, has `w` or `h` of zero or below, or runs past the source (`x + w > 1` or `y + h > 1`). Draw paths treat a hostile crop in a loaded file as no crop instead of panicking. `object.set_crop` refuses the same windows.

The crop is only about drawing: an image still hit-tests by its frame, and a hotspot region is in slide coordinates, so cropping the hotspot image never moves its regions. `object.paste`, `object.duplicate`, `slide.duplicate`, templates and masters keep the crop. `simulate` output is unchanged.

### Accessibility fields

Every object may also carry three optional fields. They are omitted from JSON when unset, so existing courses and golden files stay byte-identical:

| field | meaning |
|---|---|
| `focus_order: Option<u32>` | Author-set keyboard focus position. Lower numbers run first; equal numbers keep the default reading order. Omitted when unset. |
| `decorative: bool` | The object is ignored by assistive technology and never enters the keyboard focus order. Default false, omitted when false. |
| `description: Option<String>` | The object's text alternative, for any kind. For images the existing `alt` stays and wins while `description` is empty. Omitted from JSON when unset. |

`slide.layers` runs back to front: the last layer draws on top, and the Slide Layers panel lists it first with the base pinned at the bottom. The default reading order is the base layer bottom to top, then visible layers in that same authored order, whatever order they were shown in. Author-ordered objects come first, then question controls, then the default order, then the player chrome. The `keyboard_objects()` query in `coursecraft-logic` lists the objects a click would actually reach right now; the renderer builds the full focus list from it. Exact keyboard semantics are in `triggers.md`.

Layer options beyond the basic flags are omitted from JSON when default, so existing courses stay byte-identical:

- `hide_base_objects` (default false) stops base-layer objects being drawn, clicked, focused or read while the layer is visible. Master items stay.
- `hide_when_timeline_ends` (default false) hides the layer after its `timeline_ends` triggers run. A zero duration does not auto-hide.
- `allow_seek` is `auto` (default), `yes` or `no`, and decides whether the learner's seekbar moves this layer's clock.
- `revisit` is `auto` (default), `reset` or `resume`, and decides what showing the layer again does to its clock and object states.

`Slide.player` is an optional struct of per-slide player overrides, omitted when unset. Its `next_requires` (same shape as `player.next_requires`) replaces the course rule on that slide. Its `show_nav`, `show_seekbar` and `show_menu` (each true, false or unset) replace the course flags of the same name on that slide. A `variable` rule must name a true/false variable. See "Player navigation modes" in `triggers.md`.

`Slide.revisit` uses the same `auto | reset | resume` values and applies when entering an already-visited slide. See `triggers.md` for the runtime rules.

### Groups

`group: Option<String>` is an authoring-only grouping id. It is omitted from JSON when the object is not grouped, so existing courses and golden files stay byte-identical. Logic, render and publish ignore it, so `simulate` output does not change.

- All members of a group sit on the same slide layer, and their ids are contiguous in z-order.
- Grouping an object that already belongs to a group pulls the whole group in and flattens it; nested groups are not supported.
- A group id with only one member on a slide layer is a validation **warning** (it never blocks play or publish).
- `object.paste` and `object.duplicate` give the copies fresh group ids shared by the copies. `slide.paste` and `slide.duplicate` keep the source slide's group ids, because group ids are scoped per slide.
- Clicking any member selects the whole group. Double-clicking a member of a selected group focuses just that member for editing; clicking away clears the focus.

### Slide masters and templates

`masters: Vec<Slide>` and `templates: Vec<Slide>` are omitted from JSON when empty, so existing courses and `simulate` output stay byte-identical. `Slide.master: Option<String>` names the master a slide draws under and is omitted when unset.

- A master is a slide with objects and a background only. It must not have triggers, layers or a question, and it must not hold audio, video or Lottie objects. Validation reports these as errors.
- A master is drawn under the slide's own objects, before the base layer. Master objects never take a press, are never in the keyboard focus list, ignore timing and animation, and are always shown except when their `initial_state` is `hidden`. They add no events and no states.
- A slide's background is its own, else its master's, else `theme.background`. `Course::slide_background` is the one helper every drawing path uses.
- A master is never reachable in play. `Course::play_slide` looks at scenes then bank slides, and `Course::slide_index` looks only at scenes; `Course::slide` and `Course::slide_mut` also find masters so object commands and canvas tools work on them unchanged. They never find templates.
- Slide ids are unique across scenes, masters, templates and banks.
- A warning is reported when `slide.master` names a master that does not exist; the slide then draws with no master.
- Templates are authoring-only deep clones of a slide, including its objects, layers, triggers and question. They are never played or exported. Adding one to the course clones it with fresh ids using the same id remapping as `slide.duplicate`, keeping `slide.master`.

**Publishing.** Publish collects media from slides and masters. Templates are stripped from the published `course.json`, and their media is not collected. A course with no templates publishes byte-identically.

**XLIFF.** Translation exports master titles, notes, object text, alt text, descriptions and state text under stable `master/{id}/...` unit ids. Templates are not exported.

### Question banks and random draw

`banks: Vec<QuestionBank>` is omitted from JSON when empty, so courses that use neither banks nor draw serialize exactly as before. A bank is `{id, title, slides}`, kept outside play order like masters. A scene slide may carry `draw: {bank, count}`, omitted when unset; at play time it is replaced by `count` questions picked at random from that bank (play-time behaviour is not part of this step).

| field | meaning |
|---|---|
| `draw.bank` | The id of the question bank to draw from. It must exist. |
| `draw.count` | How many questions to draw. At least 1 and at most the size of the bank. |

- Bank ids are valid names (1-128 chars of `[A-Za-z0-9_-]`) and unique among banks. Bank titles follow the same rule scenes use: they must not be empty.
- Every bank slide goes through the same checks as a scene slide, at path `/banks/{i}/slides/{j}`. Slide ids are unique across scenes, masters, templates and banks.
- Every bank slide must have a question, and must not itself carry `draw`.
- A slide with `draw` must not have a question. Its `bank` must exist, and its `count` must be at least 1 and at most the size of that bank.
- Across the whole course, the `count`s drawn from one bank add up to at most that bank's size, so two draws from one bank never need the same question.
- A masters or template slide must not carry `draw`.
- `Course::bank` finds a bank by id and `Course::bank_slide` finds a bank slide by id. `Course::play_slide` also finds bank slides after scene slides; `slides()` and `slide_index()` stay scene slides only. `slide` finds bank slides through `play_slide`, `slide_mut` also finds bank slides, and `all_slide_ids` includes bank slides.

### Accessibility checks

`Course::audit()` returns automated findings plus a fixed list of manual checks. It measures only what a tool can measure. Passing the automated checks does not make a course accessible, and CourseCraft never calls a course WCAG compliant. Master objects are audited too; a master finding names the master and uses the slide background fallback.

| Check id | Severity | Criterion | Rule |
|---|---|---|---|
| `course-title` | error | WCAG 2.4.2 | the course title is empty |
| `slide-title` | error | WCAG 2.4.2 | a slide title is empty |
| `alt-text` | error | WCAG 1.1.1 | an image that isn't decorative has no `alt` and no `description` |
| `control-name` | error | WCAG 4.1.2 | a pointer target or button has no label text and no `description` |
| `contrast` | error | WCAG 1.4.3 | text is below 4.5:1, or below 3:1 when large, against its measured background |
| `contrast-unknown` | note | WCAG 1.4.3 | the text sits over an image or video, or over a partly covering shape, so a tool cannot measure it |
| `text-size` | warning | Best practice | text is under 16 slide px (12 pt) |
| `target-size` | warning | WCAG 2.5.8 (WCAG 2.2) | an interactive object is smaller than 24 by 24 slide px |
| `video-captions` | error | WCAG 1.2.2 | a video has no `captions` |
| `audio-captions` | warning | WCAG 1.2.1 | an audio object has no `captions` |
| `autoplay-control` | warning | WCAG 1.4.2 | autoplay audio or video has no trigger that pauses or stops it |
| `focus-order` | warning | WCAG 2.4.3 | two objects on a slide share a `focus_order` |
| `decorative-interactive` | error | WCAG 4.1.2 | a `decorative` object is a pointer target or button |

Large text is at least 24 slide px, or at least 18.66 slide px and bold (WCAG's 18 pt and 14 pt bold). Slide pixels are treated as CSS pixels; the player may scale text, so the measured size is a floor. Backgrounds are measured from the slide background and from shapes that fully cover the text frame, with sRGB alpha compositing. Images, videos and partly covering shapes make contrast unknown rather than guessed.

The manual list always accompanies the findings: whether the reading order makes sense aloud, whether alt text and descriptions say what matters, whether captions are accurate and include important sounds, whether anything relies on colour or on shape and position alone, whether timed content can be paused, and whether a real screen reader and keyboard pass has been done.

### Rich text runs

A `text` object can define formatted segments via `runs: [TextRun]`. When `runs` is non-empty, the displayed text is the concatenation of the runs, and `text` must equal that concatenation (the engine keeps them in sync; validator warns if they differ; renderer always uses `runs`). Each run may override `bold`, `italic`, `underline`, `size`, and `color`; omitted overrides inherit from the paragraph `style`. An object without runs renders as a single run using `text` and `style`. Limits: at most 1000 runs per text object, and run sizes are clamped to 1–400 at render time.

### Paragraphs, links and autofit

`paragraphs: [Paragraph]` styles the `\n`-separated paragraphs of `text`. Entry `i` styles paragraph `i`; missing entries use the defaults and extra entries are ignored. The list is omitted when empty, so old files load and save unchanged.

```json
{ "list": "bullet" | "number" | "none", "level": 0..4, "line_spacing": 0.8..=3,
  "space_before": 0..=200, "space_after": 0..=200 }
```

`list` defaults to `none`, `level` to 0, `line_spacing` to 1, and both spaces to 0; every default is omitted. `line_spacing` multiplies the row height, and the spaces are slide pixels above and below the paragraph. Bullets use `•`, `◦`, `▪` by level; numbers use `1.`, `a.`, `i.` by level and restart after a non-numbered paragraph or a level change. Editing text keeps the list aligned: a paragraph created by splitting copies the style it split from, and joining paragraphs keeps the first one's style. `text.set` and the editor do this automatically.

A run may carry `link`, either `{ "type": "url", "url": "https://…" }` or `{ "type": "slide", "slide": "slide-id" }`. A URL must start with `http://`, `https://` or `mailto:`; a slide must exist. An invalid link is a validation error and is dropped by the renderer. Clicking a link in preview or the player runs the same effect as the `open_url` / `jump_to_slide` actions (see `triggers.md`), and does not fire the object's own click triggers. Links are underlined in the theme accent unless the run sets its own colour, are in the keyboard focus order after their object, and are announced as links on the web.

`autofit` is `none` (the default, omitted) or `shrink`. `shrink` lays the text out at full size and reduces the font scale until it fits, never below 40 %. It applies to runs, list markers and paragraph spacing, and works on text objects and shape text alike. A shape's `text` uses the same `runs`, `paragraphs` and `autofit` fields.

## Animations and Motion Paths

Objects may define entrance and exit animations, and one or more motion paths:
- `entrance`: `Option<Animation>`
- `exit`: `Option<Animation>`
- `motion_paths`: `Vec<MotionPath>`

```json
{
  "entrance": { "type": "fade", "duration_ms": 500, "easing": "ease_out" },
  "exit": { "type": "fly_in", "from": "right", "duration_ms": 500, "easing": "ease_out" },
  "motion_paths": [
    { "id": "path1", "points": [{"x": 0.0, "y": 0.0}, {"x": 200.0, "y": 0.0}], "duration_ms": 1000, "easing": "ease_out", "relative": true }
  ]
}
```

- **Effects (`AnimEffect`):** `fade`, `fly_in { from }`, `zoom`, `wipe { from }`, `float { from }`.
- **Direction (`Direction`):** `left`, `right`, `top`, `bottom` (default `bottom`). For exits, `from` indicates the direction the object leaves toward.
- **Easing (`Easing`):** `linear`, `ease_in`, `ease_out` (default), `ease_in_out`.
- **Duration (`duration_ms`):** default 500 ms for animations, 1000 ms for motion paths (allowed 1–60000 ms).
- **Motion paths:** polyline points (`Point {x, y}`). `relative: true` (default) means points are offsets from the object's current position; the first point should be `{0, 0}`. `relative: false` means absolute slide coordinates. Limit: 2–100 points per path, at most 20 paths per object.

## Slide Transitions

Slides optionally specify a transition when navigated to:
- `transition`: `Option<Transition>`

```json
{
  "transition": { "type": "fade", "duration_ms": 500 }
}
```

- **Effects (`TransitionEffect`):** `fade`, `push { from }`, `cover { from }`.
- **Duration (`duration_ms`):** default 500 ms (allowed 1–60000 ms).

## Slide Notes

Slides optionally contain speaker / slide notes:
- `notes`: `String` (defaults to empty, omitted when empty).

When importing presentations from PowerPoint (.pptx), speaker notes defined on the slide's notes page body placeholder are imported into this field. The editor displays notes read-only in slide properties.

## Theme

`Course.theme` defines global visual styling (`accent`, `background`, `text`, `font`, and `player { bar, bar_text }`), skipped when equal to default values. `Slide.background` optionally overrides `theme.background` for an individual slide.

### Palette

`theme.palette: [{ "id", "name", "color" }]` is a list of named colours, at most 12, omitted when empty. It is an authoring aid: the renderer and player never read it; objects keep concrete colours. `theme.set_palette_color` changes a palette colour and, by default, every use of its old colour in the course. A use is matched on its six RGB digits after normalising (`#abc` becomes `#AABBCC`, case ignored); each use keeps its own alpha, so an 8-digit `#RRGGBBAA` stays 8-digit with the same `AA`. Colours that do not parse are left alone. `course.recolor {from, to}` does the same replacement without a palette entry, and also recolours palette entries. Duplicate palette ids, bad ids and colours that are not `#RRGGBB` or `#RRGGBBAA` are validation errors.

### Named text styles

`Course.text_styles: [{ "id", "name", "style", "paragraphs"? }]` holds up to 50 named styles. `style` is a full `TextStyle` (font, size, colour, bold, italic, underline, strikethrough, baseline, letter spacing, align, valign). `paragraphs` is optional paragraph formatting (line spacing, space before and after, and a list kind and level that are applied only when not the default). A text object may carry `text_style: <id>`, a link to a style; the field is omitted when unset. Like the palette, a style is an authoring aid: the object's own `style` and `paragraphs` are what render, so the renderer and player ignore the link.

Applying a style copies it into the object's base style and sets the link; per-range overrides in `runs` are kept unless `clear_overrides` is set. Changing a linked object by hand keeps the link, and the editor marks it "modified". `text_style.update` rewrites every linked object in the course, hand-changed ones included, and keeps per-range overrides. `text_style.delete` unlinks objects and leaves their look unchanged. A link to a style that does not exist is a validation warning, not an error, and the object keeps its own look.

## States

`normal` is the object itself; never redefine it. Every other state is a `StateDef {name, fill?, text?, src?}` of property overrides. For an `image` object, a non-empty state `src` replaces the picture while that state is current (the base `alt` is kept); an empty or absent `src` keeps the base picture. Built-in names have player meaning:

| state | meaning |
|---|---|
| `hidden` | not drawn, not clickable (needs no definition) |
| `disabled` | drawn, swallows clicks |
| `visited` / `selected` | set automatically after a click (see `triggers.md`) |
| `hover` / `down` | applied by the renderer while hovered or pressed |

Any other name is a custom state you change with `change_state`.

## Questions

`kind` is one of `multiple_choice`, `multiple_response`, `true_false`, `fill_in`, `sequence`, `matching`, `hotspot`, `drag_drop`, `likert`, `short_answer`, `essay`, `freeform`.

`accepted`, `pairs`, `regions`, `drag_items`, and `drop_targets` are omitted when empty. `case_sensitive` and `return_if_wrong` are omitted when false. `hotspot_image` is omitted when absent. `markers` is omitted when `1`. `snap_to_center` and `one_per_target` are omitted when true. `correct_when` is omitted when absent. A course that does not use these fields serializes unchanged.

Each choice may carry `points` (integer, -1000..=1000, default 0). `scoring` is `all_or_nothing` (default), `per_answer` or `partial`; validation restricts which kinds each applies to. `Question::max_points()` reports the question's value.

| `kind` | rules |
|---|---|
| `multiple_choice` | `choices`: at least 2, exactly one `correct` |
| `multiple_response` | `choices`: at least 2, at least one `correct` |
| `true_false` | `choices`: exactly 2, exactly one `correct`. The editor creates ids `true` and `false` with texts "True" and "False"; the texts can be edited |
| `fill_in` | `choices` must be empty. `accepted`: 1–50 strings, each 1–500 characters after trimming. `case_sensitive` defaults to false |
| `sequence` | `choices` are the items **in the correct order** (2–20). `correct` must be false; the validator **warns** if one is set. `shuffle` is ignored (the player always shuffles) and **warns** if set |
| `matching` | `choices` must be empty. `pairs`: 2–20 `{id, left, right}` with unique ids and non-empty texts. The correct answer pairs each left with its own pair's right. `shuffle` is ignored and **warns** if set |
| `hotspot` | `choices` must be empty. `hotspot_image` names a base-layer `image` object on the same slide. `regions`: 1–20 `{id, shape, x, y, w, h, correct}` in slide coordinates. `shape` is `rect` or `ellipse`. At least one region is `correct`. Each of `x`, `y`, `w`, `h` is finite; `x` and `y` are within ±100000; `w` and `h` are greater than 0 and at most 100000. `markers` is how many markers the learner may place (default 1, range 1–10) |
| `drag_drop` | `choices` must be empty. `drag_items`: 1–20 `{item, target}`. `item` and `target` are ids of base-layer objects on the same slide. Item ids are unique. An item is not its own target and is not another item's target. `drop_targets`: 1–20 unique base-layer object ids that accept drops; every `drag_items` target is in the list (extra ids are distractors). No object is both an item and a drop target. `snap_to_center` defaults to true, `return_if_wrong` to false, `one_per_target` to true |
| `likert` | `choices`: 2–11. No choice may be `correct`. A scale of labelled choices, pick one. Not graded and worth no points |
| `short_answer` | `choices` must be empty. One line of free text. Not graded and worth no points |
| `essay` | `choices` must be empty. Several lines of free text. Not graded and worth no points |
| `freeform` | Requires `correct_when`, a trigger-style condition checked against this slide's variables and object states (same rules and depth limit as a trigger condition). `choices`, `accepted`, `pairs`, `regions`, `drag_items`, and `drop_targets` must be empty. `scoring` must be `all_or_nothing`. Graded by the condition, worth `points` |

Pair ids and hotspot region ids follow the id rule below (1–128 characters of `[A-Za-z0-9_-]`). Grading rules are in `triggers.md`.

## IDs

- Slide ids are unique across scenes, masters and templates. Scene ids are unique per course.
- Object and layer ids share one namespace per slide; triggers can only reference things on their own slide.
- ids are 1–128 characters of `[A-Za-z0-9_-]`, so they are safe in URLs, file names and SCORM identifiers.

## Triggers

`{action, when, condition?, disabled?}`. `disabled` defaults to false and is omitted when false; a disabled trigger never fires. All vocabularies and the processing rules are in `triggers.md`. JSON shape:

```json
{ "action": { "action": "show_layer", "layer": "l1" },
  "when":   { "event": "click", "object": "tab1" },
  "condition": { "all": [
      { "compare": { "variable": "score", "op": "ge", "value": 3 } },
      { "object_state": { "object": "tab2", "state": "visited", "is": false } } ] } }
```

Actions: `jump_to_slide {target: "next"|"previous"|{"id": …}}`, `show_layer`, `hide_layer`, `change_state`, `set_variable {variable, op, value}`, `play/pause/stop_media`, `pause/resume_timeline`, `submit_question`, `submit_results`, `complete_course {status}`, `exit_course`, `open_url` (http(s) and mailto only), `move_on_path {object, path}`, `run_javascript {code}`, `lightbox_slide {slide}` and `close_lightbox`.

Events: `timeline_starts`, `timeline_ends`, `timeline_reaches {ms}` (each with optional `layer`), `click`, `hover_start`, `hover_end`, `media_ends`, `state_changes {object, to?}`, `variable_changes`, `key_pressed {key, object?}`, `dropped_on {object, target}`, `objects_intersect {a, b}`, `objects_stop_intersecting {a, b}`, `submit`, `animation_completes {object, path?}`.

## Validation

`Course::validate()` returns issues with a path like `/scenes/0/slides/2/triggers/1/action`. Errors block playing and publishing; warnings don't. Engine commands refuse any edit that would turn a valid course invalid. For example, deleting an object a trigger still uses is refused, and the error names that trigger.

## Localisation

`language` is an optional BCP 47 tag with a deliberately simple syntax check: hyphen-separated ASCII alphanumeric subtags, the first 2–8 letters long. The web player copies it to `<html lang>`; a missing tag means `en`. Course Settings edits it, and `course.update {"language": string|null}` writes or clears it; an invalid tag is refused.

`strings` overrides the player's own text, keyed by `CHROME_STRINGS` in `coursecraft-course`. Every learner-facing string the player paints itself lives in that table (navigation and drawer tooltips, the completion card, loading, fill-in and matching placeholders, screen-reader labels and hints, and live-region announcements). Unknown keys are a validation warning. `Course::chrome_string(key)` returns the override or the English default, and `fill(template, args)` substitutes `{name}` placeholders.

XLIFF 1.2 export walks every learner-visible field: course, scene and slide titles, slide notes, object text (runs as `<g>` groups), table cell text, button labels, image alt text, descriptions, state text, question prompts, choices, accepted answers and matching pairs, plus every chrome string. **String variables are not exported**, because triggers can compare them; translating one would change play-time behaviour. Import refuses units whose `%var%` references or chrome `{name}` placeholders changed, and refuses a result that fails validation. Translated text outside Latin, Greek and Cyrillic is reported because the bundled font cannot draw it; right-to-left and complex shaping are not supported.

## Versioning

Any breaking change bumps `SCHEMA_VERSION` and ships a migration in `coursecraft-course`. Never silently reinterpret old files.
