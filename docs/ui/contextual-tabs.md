# Ribbon: contextual tabs and modes

> Part of [the ribbon plan](README.md). Kinds, actions and status marks as in [ribbon.md](ribbon.md).

> **In the code (M38 step 3, 2026-11-24):** every contextual tab below is drawn from
> `CONTEXT_TABS` in `crates/ui-egui/src/ribbon/spec.rs`; `ribbon/contextual.rs` decides which ones
> the selection shows and runs the controls that act on the selected object. The coloured caption
> sits in the tab strip, left of its tabs. Built features that the ribbon doesn't reach yet are
> drawn disabled with "Not available yet: ribbon wiring (use the Format inspector…)"; the ↘
> launchers open the matching settings panel or the Format inspector. **Slide Master mode is
> built** (2026-11-24): Master View opens on the Slide Master tab and the strip shows only Home,
> Insert, Transitions, Animations, View and Help beside it (`MASTER_TAB`, `MASTER_STRIP`);
> closing Master View goes back to Home. Feedback Layouts, Question form view and the Preview tab
> don't replace the strip yet.

## When each one shows

Contextual tabs sit right of Help under a coloured caption. Selecting the object does **not**
switch to its tab, except when the object is just inserted.

| Caption | Tab | Shown when the selection contains… |
|---|---|---|
| Drawing Tools | **Format** | a shape, text box, hotspot, marker, slider, dial, input, or text being edited |
| Button Tools | **Format** | a button (instead of Drawing Tools) |
| Picture Tools | **Format** | an image |
| Video Tools | **Options** | a video |
| Audio Tools | **Options** | an audio object |
| Table Tools | **Design**, **Layout** | a table or a table cell |
| Question Tools | **Design** | the slide is a question slide (any selection) |

Several captions can show at once (Question Tools + Drawing Tools). If the current tab disappears
because the selection changed, the ribbon returns to Home. **Built:** all three rules, and the
switch to the object's tab right after it is inserted (an object id the slide didn't have the
frame before).

Modes replace the tab strip:

| Mode | Tab strip |
|---|---|
| Slide master | File, **Slide Master**, Home, Insert, Transitions, Animations, View, Help |
| Feedback layouts | File, **Feedback Layouts**, Home, Insert, View, Help |
| Question form view | File, **Question**, Slides, Help |
| Preview | File, **Preview** |
| Edit state (canvas state editing, M29) | the normal strip; no **Editing State** tab yet, Done and the state picker are in the States panel |

## Drawing Tools › Format

| Group | Control | Kind | Action | Status | Notes |
|---|---|---|---|---|---|
| Insert Shapes | shape gallery | G | `object.add` `{shape}` | ✅ | `shape_gallery.rs` |
| | Change Shape | SS | `object.update` `{shape}` | ⛔ | keep size and format, change geometry |
| | Edit Points | S | `ContextOp::EditPoints` | ✅ | a freeform (path) selected |
| | Text Box | S | `object.add` `{text}` | ✅ | |
| Shape Styles ↘ | style gallery | G | `object.apply_style` | ✅ | drawn disabled until the galleries; ↘ opens the Shape Style panel |
| | Shape Fill | SS | `object.set_fill_style` | ✅ | opens the Shape Style panel at Fill |
| | Shape Outline | SS | `object.update` `{outline}`, `object.set_outline_join` | ✅ | opens the Shape Style panel at Outline |
| | Shape Effect | SS | `object.set_shadow`, `object.set_glow` | 🟡 | opens the Shape Style panel; ▸ Shadow ✅, Glow ✅, Reflection ⛔, Soft Edges ⛔ |
| Arrange and Align | Arrange | SS | `object.reorder`, `group.set_interactive` | ✅ | Bring to Front/Forward, Send Backward/to Back, Group/Ungroup, Make button from group / Remove button behaviour |
| | Group | SS | `object.group`, `object.ungroup` | ✅ | |
| | Make button from group / Remove button behaviour | S | `group.set_interactive` | ✅ | CourseCraft extra; shares button states and triggers across the selected group |
| | Rotate | SS | `object.set_rotation`, `object.flip` | ✅ | opens the Size and Position panel at Rotate |
| | align grid (9) | S | `object.align`, `object.distribute`, `object.match_size` | ✅ | in the Arrange ▾ menu; Align to Slide toggle |
| Size and Position ↘ | X, Y, Width, Height | N | `object.set_frame` | ✅ | number fields; one edit is one undo step; ↘ opens the Size and Position panel |
| Publish | Preview | LS | | ✅ | |

## Button Tools › Format

| Group | Control | Kind | Action | Status | Notes |
|---|---|---|---|---|---|
| Action | On Click | L | `trigger.add` (on click) | ✅ | opens the trigger editor with the click event and this button pre-filled |
| Button Icons | icon gallery | G | `object.update` `{icon}` | ⛔ | built-in arrow / check / play icons inside the button |
| | Align Icon | SS | `object.update` `{icon_align}` | ⛔ | left, right, top, bottom, centre |
| | Icon Color | SS | `object.update` `{icon_color}` | ⛔ | |
| | Remove Icon | S | `object.update` `{icon: null}` | ⛔ | |
| Button Styles ↘ | Format inspector (label, fill) | G | `object.apply_style` | ✅ | Shape Style only styles shapes (N94) |
| | Button Fill | SS | Format inspector Fill row | ✅ | |
| | Button Border | SS | button borders | ⛔ | a button has no outline yet; drawn disabled (N94) |
| | Button Effects | SS | as Shape Effect | ✅ | |
| Arrange and Align | as Drawing Tools | | | ✅ | |
| Size and Position ↘ | X, Y, Width, Height | N | `object.update` | ✅ | |
| Publish | Preview | LS | | ✅ | |

A new button gets five built-in states (Normal, Hover, Down, Visited, Disabled); CourseCraft
already creates them (M29), check the set matches.

## Picture Tools › Format

| Group | Control | Kind | Action | Status | Notes |
|---|---|---|---|---|---|
| Adjust | Brightness | SS | — | ⛔ | |
| | Contrast | SS | — | ⛔ | (`contrast` in code is the accessibility checker) |
| | Recolor | SS | `course.recolor` | 🟡 | recolours SVG/icons; photos ⛔ |
| | Replace Picture | SS | `object.update` `{src}` | ✅ | from a file (▾ From Media Library ⛔) |
| | Reset Picture | SS | `object.set_crop` (clear) + reset size | ✅ | clears the crop, back to the picture's own size |
| Picture Styles ↘ | style gallery | G | `object.apply_style` | ⛔ | frames, bevels: needs picture style presets |
| | Picture Shape | SS | `object.set_crop` `{mask}` | ⛔ | crop to shape |
| | Picture Border | SS | `object.update` `{outline}` | ✅ | drawn disabled: ribbon wiring (Format inspector) |
| | Picture Effects | SS | `object.set_shadow`, `object.set_glow` | ⛔ | drawn disabled (ribbon wiring, Format inspector) | |
| Arrange and Align | as Drawing Tools | | | ✅ | |
| Size and Position ↘ | Zoom Picture | L | `object.update` `{zoom_on_click}` | ⛔ | learner clicks to enlarge |
| | Crop | L | `object.set_crop` | ✅ | starts crop mode on the canvas (`crop.rs`) |
| | X, Y, Width, Height | N | `object.update` | ✅ | |
| Publish | Preview | LS | | ✅ | |

## Video Tools › Options

Mirrors Audio Tools plus video specifics.

| Group | Control | Kind | Action | Status |
|---|---|---|---|---|
| Play | Preview | L | `ui:media.preview` | ✅ |
| Video Options ↘ | Volume | LS | `object.set_media` `{volume}` | ✅ |
| | Video Editor (trim) | L | `object.set_media` `{trim}` | 🟡 |
| | Replace Video | SS | `object.update` `{src}` | ✅ wired |
| | Export Video | S | `ui:media.export` | ⛔ |
| | Show Video: On Slide / In New Window | C | `object.set_media` | ⛔ |
| | Play Video: Automatically / When Clicked / From Trigger | C | `object.set_media` `{autoplay}` | ✅ |
| | Video Controls: Show / Hide | C | `object.set_media` `{controls}` | ✅ |
| | Compression | C | — | ⛔ |
| Closed Captions | Add / Import / Export / Delete | | `object.set_captions` | 🟡 Import Captions ✅ wired |
| Cue points | Add Cue / Question at time | | `video.add_cue`, `video.add_question_cue` | ⛔ (no ribbon control; cues are added in the timeline) |
| Publish | Preview | LS | | ✅ |

## Audio Tools › Options

| Group | Control | Kind | Action | Status |
|---|---|---|---|---|
| Play | Preview | L | `ui:media.preview` | ✅ |
| Audio Options ↘ | Audio Volume | LS | `object.set_media` `{volume}` | ✅ |
| | Audio Editor | L | `ui:audioEditor` | 🟡 trim ✅, edit waveform ⛔ |
| | Replace Audio | SS | `object.update` `{src}` | ✅ wired, from a file (▾ Record Mic, Text-to-Speech ⛔ here) |
| | Export Audio | S | `ui:media.export` | ⛔ |
| | Compression | C | — | ⛔ |
| Closed Captions | Add Captions | L | `object.set_captions` | 🟡 |
| | Import / Export / Delete | S | `object.set_captions` | 🟡 Import Captions ✅ wired |
| Publish | Preview | LS | | ✅ |

## Table Tools › Design and Layout

CourseCraft already has the commands.

| Tab | Group | Controls | Action | Status |
|---|---|---|---|---|
| Design | Table Style Options | Header Row (K) | `table.set_header_row` | ✅ wired |
| | Table Styles | gallery, Shading, Borders | `table.set_cell`, `table.set_borders` | 🟡 |
| Layout | Rows & Columns | Insert Above/Below/Left/Right, Delete | `table.insert_row`, `table.insert_column`, `table.delete_row`, `table.delete_column` | ✅ wired, at the selected cell |
| | Merge | Merge Cells, Split Cells | `table.merge`, `table.split` | 🟡 Split ✅ wired; Merge needs a range of cells selected ⛔ |
| | Cell Size | row height, column width | `table.resize` | ✅ drawn disabled: ribbon wiring |
| | Alignment | cell text alignment | `table.set_cell` | 🟡 |

## Question Tools › Design

| Group | Control | Kind | Action | Status | Notes |
|---|---|---|---|---|---|
| Feedback | Feedback Layouts | LS | — | ⛔ | |
| Display | Feedback: By Question / By Choice / None | C | `question.set` `{feedback}` | 🟡 | per-choice feedback to verify |
| | Shuffle: Answers / None | C | `question.set` `{shuffle}` | ✅ | Shuffle Answers toggle, wired |
| | Results Slide | C | `question.set` `{results}` | ✅ | |
| Scoring | Score: By Question / By Choice | C | `question.set` `{scoring}` | 🟡 | |
| | Attempts | C | `question.set` `{attempts}` | ✅ | |
| AI | Edit Question / Insert Question | L | — | ⛔ | an Edit Question button opens the Question panel |
| Publish | Player / Preview / Publish | | | ✅ | |

The right panel gains a **Question** pane with a **Form View | Slide View** toggle (today
`question.rs` shows the question editor in the inspector).

## Question form view (Question tab)

| Group | Controls | Status |
|---|---|---|
| Clipboard | Cut, Copy, Paste | ✅ |
| Text | Bold, Italic, Underline, Spelling | ✅ |
| Feedback | Feedback Layouts | ⛔ |
| Display, Scoring | as Question Tools › Design | as there |
| AI | Edit Question, Insert Question | ⛔ |
| Insert | Media ▾, Audio ▾ (per question/feedback) | ⛔ |
| Publish | Player, Preview, Publish | ✅ |

Body layout: question box, choices grid with a Correct column (rows A–J),
feedback-and-branching grid (Correct, Incorrect, Post-quiz review; Feedback, More…, Points). Today
the same data is edited in `question.rs`; form view is a full-canvas layout of it. ⛔ as a view, ✅
as data.

## Slide Master tab

| Group | Control | Kind | Action | Status |
|---|---|---|---|---|
| Edit Master | Insert Slide Master | L | `master.add` | ✅ wired |
| | Insert Layout | L | `master.add` `{layout}` | ⛔ masters have no layouts yet; drawn disabled |
| | Delete | S | `master.delete` | ✅ wired, asks first when slides use the master |
| | Rename | S | `ui:rename` | ✅ wired, renames in the masters panel |
| | Preserve | T | — | ⛔ |
| Master Layout | Insert Placeholder | LS | — | ⛔ ▾ Content, Text, Picture, Media… |
| | Title | K | — | ⛔ |
| Edit Theme | Themes | G | `course.set_theme` | ✅ the theme gallery |
| | Colors / Fonts | SS | as Design | as Design |
| Background ↘ | Background Styles, Hide Background Graphics | | as Design | as Design |
| Close | Close Master View | L | `ui:view.slide` | ✅ wired |

## Feedback Layouts tab

⛔ as a whole. Same groups as Slide Master; edits the layouts of Correct / Incorrect / Try Again
feedback layers (today generated by `question.add_feedback_layers`).

## Preview tab (preview window)

| Group | Control | Kind | Action | Status |
|---|---|---|---|---|
| Close | Close Preview | L | `ui:preview.close` | ✅ |
| Preview | Select | LS | `ui:preview.scope` | ✅ This Slide / This Scene / Entire Course |
| | Replay | LS | `ui:preview.replay` | 🟡 ▾ Replay Slide, Replay Scene, Replay Course |
| | Edit Slide | L | `ui:preview.edit` | ⛔ close preview on the current slide |
| | Inspect | L | `ui:preview.inspect` | ⛔ variables and trigger log (the simulator has the data) |
| (right) | Desktop / Tablet landscape / Tablet portrait / Phone landscape / Phone portrait | T×5 | `ui:preview.device` | ⛔ |
| | Settings ⚙ | S | `ui:preview.settings` | ⛔ |

## 360° image editor

🚫 Not planned. Not drawn: there is no 360° object to select, so its tabs never appear.
