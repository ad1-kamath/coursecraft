# Authoring feature status

> Part of [the ribbon plan](README.md). **Written:** 2026-11-23. One row per authoring **feature**
> in CourseCraft, whatever tab or dialog it sits in. The control-by-control view is in
> [ribbon.md](ribbon.md) and [contextual-tabs.md](contextual-tabs.md); this file is the summary
> that drives the roadmap.
>
> Status marks as in README (✅ built, 🟡 partly, ⛔ not yet, 🚫 not planned, — left out).
> **Milestone** is where the work is planned in `docs/roadmap.md`. A row changes in the same commit
> as the code that changes it.

## Summary

| Area | ✅ | 🟡 | ⛔ | 🚫 / — |
|---|---|---|---|---|
| Project and file | 7 | 2 | 1 | 2 |
| Structure: scenes, slides, masters | 10 | 1 | 6 | 0 |
| Text | 21 | 3 | 0 | 0 |
| Shapes, pictures and buttons | 17 | 4 | 9 | 0 |
| Media | 10 | 3 | 5 | 1 |
| Interactivity | 12 | 3 | 2 | 0 |
| Animation, transitions, timeline | 19 | 1 | 5 | 0 |
| Questions and quizzes | 8 | 4 | 3 | 0 |
| Player, preview, publish | 11 | 4 | 7 | 2 |
| Editor shell and panels | 12 | 9 | 2 | 0 |
| AI | 3 | 1 | 6 | 0 |
| Assets and services | 0 | 0 | 1 | 7 |
| **Total** | **129** | **35** | **47** | **12** |

Counted from the tables below; recount when a row changes.

## Project and file

| Feature | CourseCraft | Status | Milestone |
|---|---|---|---|
| New, Open, Save, Save As | File menu | ✅ | — |
| Recent projects | File › Open Recent (names) | 🟡 thumbnails missing | M38 |
| Start screen / File page with start actions and Recent | File page from the File tab and on launch (`file_page.rs`, its Start section is the welcome screen), Recent with first-slide thumbnails; news to come | 🟡 news | M38 |
| Import PowerPoint | File › Import | ✅ | — |
| Import questions (CSV / spreadsheet) | File › Import | ✅ | — |
| Translation export / import (XLIFF) | File › Translation | ✅ | — |
| Version history, autosave and recovery | `version_history.rs` | ✅ | — |
| Close project (without quitting) | — | ⛔ | M38 |
| Slide size and default size | Slide Size dialog | ✅ | — |
| Project info (title, author, description) | Course settings | ✅ | — |
| Team Slides (shared cloud library) | — | — | — |
| Record Screen project | — | 🚫 patent watch list | — |

## Structure: scenes, slides, masters

| Feature | CourseCraft | Status | Milestone |
|---|---|---|---|
| Scenes; new scene; rename | `scene.*` | ✅ | — |
| Start section (start flag) | — | ⛔ needs model field + runtime rule | M41 |
| Course Map: cards, zoom, fit | `course_map.rs` | ✅ | — |
| Course Map: collapse scene cards, branch lines between scenes | — | ⛔ | M42 |
| New slide from layout, Basic Layouts gallery | `slide.add_from_layout` | ✅ | — |
| My Templates | `template.*` | ✅ | — |
| Duplicate, delete, move slides | `slide.*` | ✅ | — |
| Apply layout; reset slide to layout | `slide.set_master` | 🟡 reset ⛔ | M41 |
| Slide masters and layouts (insert, rename, delete) | `master.*` | ✅ | — |
| Master: Preserve, Title checkbox, Insert Placeholder | — | ⛔ | M41 |
| Hide background graphics | — | ⛔ | M41 |
| Feedback layouts | — | ⛔ | M41 |
| Slide layers (new, duplicate, delete, dim, properties) | `layer.*` | ✅ | — |
| Slide properties: advance, revisit, prev/next/submit | `slide.update`, `slide.set_player_overrides` | ✅ | — |
| Swipe gestures (prev/next) | — | ⛔ | M41 |
| Focus order | Home › Slide › Focus Order window (`focus_order.rs`): the slide's Tab order from `coursecraft_render::focus_order_candidates`, Move Up/Down or drag inside a layer section (base first), Reset to default order; OK writes 1..n as one undo step. Text and pictures are not listed: the player reads them in drawing order, not `focus_order` (gap, not counted) | ✅ | — |
| Themes: colours, fonts, gallery | `course.set_theme`, `theme.*`; Design › Themes gallery of built-in themes; Theme dialog uses the shared font picker; Design › Fonts ▾ offers font pairs and Create New Theme Fonts… | ✅ | — |

## Text

| Feature | CourseCraft | Status | Milestone |
|---|---|---|---|
| Text boxes, point text, placeholders, autofit | `object.add`, `object.set_autofit`, `text.set_paragraph` `{wrap}` (click places point text; Wrap Text in Home › Paragraph and Text panel › Text Box converts to area text) | ✅ | — |
| Font family and size, grow/shrink | `text.style_range`; Home › Font through `text_format.rs` | ✅ | — |
| Bold, italic, underline, strikethrough | `text.style_range` | ✅ | — |
| Superscript, subscript | `text.style_range` | ✅ | — |
| Character spacing | `letter_spacing` | ✅ | — |
| Font colour with theme colours and eyedropper | `eyedropper.rs` | ✅ | — |
| Text styles | `text_style.*` | ✅ | — |
| Bullets, numbering, indent, line spacing, alignment | `text.set_paragraph(s)` (`number_scheme`, `start_at`); Home › Paragraph through `text_format.rs`; Numbering ▾ with 7 schemes and Start at | ✅ | — |
| Find and replace | `course.find`, `course.replace` | ✅ | — |
| Spelling | `spell_check.rs` (English dictionary; skips non-English course languages with a note) | ✅ | — |
| Hyperlinks | `text.set_link` | ✅ | — |
| Format painter | `format_painter.rs` | ✅ | — |
| Theme fonts (heading + body) | `Theme.heading_font`, `course.set_theme {heading_font}`; layout titles use it; Create New Theme Fonts dialog; Theme Fonts section at the top of every font list | ✅ | — |
| Paste options (destination theme, keep source, text only) | Home › Paste ▾: Use Destination Theme (default), Keep Source Formatting, Keep Text Only, Paste Special… (`clipboard.rs`); the copy carries its source theme | ✅ | — |
| Vertical text alignment | `text.set_paragraph` `{valign}`: Home › Paragraph › Align Text, inspector | ✅ | — |
| Clear formatting | `text.clear_formatting` (keeps links); Home › Font › Clear Formatting | ✅ | — |
| Text shadow | `TextRun.shadow`, `text.style_range {shadow}`; Home › Font › Text Shadow; Text panel; PPTX `outerShdw` | ✅ | — |
| Change case | `text.change_case` (sentence, lower, upper, title, toggle); Home › Font › Change Case ▾ | ✅ | — |
| Highlight colour | `TextRun.highlight`, `text.style_range {highlight}`; Home › Font › Highlight ▾; Text panel; PPTX `a:highlight` | ✅ | — |
| Text direction (rotate, stacked) | Home › Paragraph › Text Direction ▾: Horizontal, Rotate 90°, Rotate 270° (`text.set_paragraph {direction}`) | 🟡 stacked text | Later |
| Font and Paragraph settings | Text settings panel (`panels.rs`, from Font ↘ and Paragraph ↘): every text control, space before and after, shadow and highlight; caps, underline styles, tab stops and columns drawn disabled | 🟡 | Later |
| Slide number field | Insert › Slide Number ▾ (number, or number of total): `%Course.SlideNumber%` at the text caret or in a new text box (`insert_text.rs`) | ✅ | — |
| Symbol picker | Insert › Symbol window with search and groups; inserts at the text caret or in a new text box | 🟡 symbols limited to the bundled Inter font's glyphs | Later |
| Insert variable reference | Insert › Reference window: built-in and course variables, or a typed name, inserted as `%Name%` | ✅ | — |

## Shapes, pictures and buttons

| Feature | CourseCraft | Status | Milestone |
|---|---|---|---|
| Shape gallery, edit points | `shape_gallery.rs` | ✅ | — |
| Shape fill (solid, gradient, picture) | `object.set_fill_style` | ✅ | — |
| Outline, joins | `object.update` | ✅ | — |
| Quick styles / shape styles | `object.apply_style` | ✅ | — |
| Shadow, glow | `object.set_shadow`, `object.set_glow` | ✅ | — |
| Arrange, group, interactive button groups, align, distribute, match size | `object.*`, `group.set_interactive`, `group.update` (Home › Arrange, contextual Arrange and Align, Format inspector, Timeline Button pill) | ✅ | — |
| Rotate, flip | `object.set_rotation`, `object.flip` | ✅ | — |
| X, Y, width, height | `object.update` | ✅ | — |
| Smart guides, grid, guides, rulers | `snap.rs`, `rulers.rs`; the Grid and Guides panel (`grid_panel.rs`) sets the grid spacing and lists the guides | ✅ | — |
| Lock and hide in editor | `object.lock`, `object.hide_in_editor` | ✅ | — |
| Pictures: insert, replace, crop | `object.set_crop`, `crop.rs` | ✅ | — |
| EMF and WMF clip art, kept as vectors | `coursecraft_media::metafile` converts to SVG on insert, media library import and PowerPoint import | ✅ | — |
| Picture border | `object.update` | ✅ | — |
| States (built-in and custom, items inside states) | M29 | ✅ | — |
| Recolor | `course.recolor` | 🟡 SVG only | M40 |
| Reset picture | | 🟡 | M40 |
| Picture effects | | 🟡 as shape effects | M40 |
| Button styles, fill, border, effects | Button Tools › Format: Button Fill opens the Format inspector, Effects open Shape Style; no button border yet | 🟡 | M40 |
| On Click on a button | trigger editor with the click event and selected button pre-filled | ✅ | — |
| Reflection, soft edges | — | ⛔ | M40 |
| Change shape (keep format) | — | ⛔ | M40 |
| Size and Position dialog | the Size and Position panel (`size_panel.rs`): width and height with an aspect lock, scale % (pictures from their original size), Reset to original size for pictures, rotation, flips, position from the top left or the centre, lock | ✅ | — |
| Brightness, contrast | — | ⛔ | M40 |
| Picture styles (frames, bevels) | — | ⛔ | M40 |
| Crop to shape | — | ⛔ | M40 |
| Zoom picture (learner enlarges) | — | ⛔ | M40 |
| Button icons (gallery, align, colour, remove) | — | ⛔ | M40 |
| Selection groups | — | ⛔ | M40 |
| Caption / callout shapes | Insert › Media › Caption (Callouts category in `shape_gallery.rs`) | ✅ | — |
| Export picture | — | ⛔ | M40 |

## Media

| Feature | CourseCraft | Status | Milestone |
|---|---|---|---|
| Audio from file | `object.add` | ✅ | — |
| Record microphone | `record_narration.rs` | ✅ | — |
| Text-to-speech narration | `audio_ai.rs` | ✅ | — |
| Background audio | `course.set_background_audio` | ✅ | — |
| Video from file, from website | `object.add`, `web_insert.rs` | ✅ | — |
| Video play options, controls, volume | `object.set_media` | ✅ | — |
| Video cue points and in-video questions | `video.*` | ✅ | — |
| Web object | `object.add` | ✅ | — |
| Media library (list, replace, unused) | `media_library.rs` | ✅ | — |
| Lottie animation | `object.set_lottie` | ✅ (CourseCraft extra) | — |
| Audio / video editor | trim only | 🟡 | M40 |
| Closed captions (add, import, export, delete) | `object.set_captions` | 🟡 | M40 |
| AI audio (sound effects, music) | — | ⛔ | M43 |
| Audio options dialog | — | 🟡 | M40 |
| Export audio / video | — | ⛔ | M40 |
| Compression setting | — | ⛔ | M40 |
| Show video in a new window | — | ⛔ | M40 |
| Record webcam | — | ⛔ | M40 |
| 360° images | — | 🚫 | — |

## Interactivity

| Feature | CourseCraft | Status | Milestone |
|---|---|---|---|
| Triggers: actions, events, conditions | `trigger.*` | ✅ | — |
| Object / slide / layer / player trigger sections | `triggers.rs` | ✅ | — |
| Variables | `variable.*` | ✅ | — |
| Buttons, hotspots, markers | `object.add`, `object.set_marker`; no hotspot object yet (hotspot questions and the Hotspot reveal interaction only) | 🟡 | M38 |
| Sliders, dials | `Slider`, `Dial` | ✅ | — |
| Text entry, checkbox, radio buttons | `object.set_input` | ✅ | — |
| Drag and drop | `object.set_drag` | ✅ | — |
| Scrolling panel | `object.set_scroll_panel` | ✅ | — |
| Tables | `table.*` | ✅ | — |
| Interactions gallery (tabs, process, flip cards…) | `interaction.insert` | ✅ (CourseCraft extra) | — |
| Timer, meter, motion path | `object.set_timer`, `object.set_meter`, `object.add_motion_path` | ✅ | — |
| Glossary, resources | `glossary.*`, `resource.*` | ✅ | — |
| Copy and paste triggers | `triggers.rs` | ✅ | — |
| Trigger dialog (Action / When / Conditions form) | inline editor | 🟡 | M38 |
| Built-in states for every input | M29 | 🟡 check the 5 button states | M40 |
| Custom mouse cursor | — | ⛔ | M41 |
| Convert slide to custom question | — | ⛔ | M41 |

## Animation, transitions, timeline

| Feature | CourseCraft | Status | Milestone |
|---|---|---|---|
| Entrance and exit animations | `object.set_animation` | ✅ | — |
| Animation duration and effect options (8 directions, shape, in/out, spokes, orientation) | `object.set_animation`; the Animate inspector's Entrance and Exit cards (`anim_panel.rs`) show one row of icon buttons per option the effect takes | ✅ | — |
| Preview an animation on the canvas | `anim_preview.rs`: plays the selected objects' entrance or exit in place through the player's sampler, once on ▶ Preview or a pick, in a loop while an effect tile is hovered; Auto-preview turns the automatic ones off | ✅ | M46 |
| Motion paths (add, duration, name, options) | `object.*motion_path` | ✅ | — |
| Slide transitions and duration | `slide.set_transition` | ✅ | — |
| Timeline: bars, drag timing, rename, restack | `timeline.rs` | ✅ | — |
| Show until end / show always | `timeline.rs` | ✅ | — |
| Cue points, snapping | `slide.add_cue` | ✅ | — |
| Slide duration | `slide.set_duration` | ✅ | — |
| Play / stop / zoom in timeline | `timeline_playback.rs` | ✅ | — |
| Timeline shows current layer; group rows expand | `timeline.rs` | ✅ | — |
| Full entrance gallery (16 effects) | Animations › Entrance / Exit galleries (`ribbon/gallery.rs`): 25 effects from the registry, each also as an exit | ✅ | M46 |
| Effect icons and Effect Options menu | Each gallery tile draws its effect with the effect's own sample function (entrance green, exit red, plays on hover); Effect Options ▾ has one section per option of the effect, with icons (`ribbon/effect_art.rs`) | ✅ | M46 |
| Transition effect options | Transitions › Effect Options ▾ and the Slide inspector: direction (eight), orientation, in/out, shape, spokes | ✅ | M46 |
| Transitions pack (15 transitions) | `TransitionEffect`: Fade, Fade through black, Push, Cover, Uncover, Reveal, Wipe, Split, Zoom, Shape, Clock, Dissolve, Random bars, Blinds, Checkerboard, with drawn gallery icons | ✅ | M46 |
| Path options | | 🟡 | M42 |
| Emphasis animations | 12 effects (`object.add_emphasis`, `set_emphasis`, `remove_emphasis`); Animations › Emphasis gallery, the Animate inspector's Emphasis card, amber strips on timeline bars | ✅ | M46 |
| Animation painter | Animations › Animation Painter (`object.copy_animation`) | ✅ | M46 |
| Sequence by paragraph (build steps) | `Animation.sequence`: as one, by paragraph, by letter (Effect Options ▾ and the Animate inspector's cards) | ✅ | M46 |
| Transition: apply to all | `slide.set_transition_all`, Transitions › Apply To All | ✅ | — |
| Timeline: fit to longest, reset, align to playhead/cue | — | ⛔ | M42 |
| Timing dialog | — | ⛔ | M42 |
| Camera zoom | — | ⛔ | M42 |
| C key adds a cue point while playing | | ⛔ verify | M42 |
| Timeline row context menu complete | | ⛔ | M42 |

## Questions and quizzes

| Feature | CourseCraft | Status | Milestone |
|---|---|---|---|
| Graded question kinds (MC, MR, T/F, fill-in, word bank, matching, sequence, numeric, hotspot, drag and drop) | `question.*` (MC, MR, T/F, fill-in, matching, sequence, hotspot, drag and drop built; word bank and numeric not yet) | 🟡 | M41 |
| Survey questions | `question.insert` | ✅ | — |
| Custom questions | `question.set_kind` | ✅ | — |
| Feedback layers (correct, incorrect, try again) | `question.add_feedback_layers` | ✅ | — |
| Shuffle, attempts | `question.set` | ✅ | — |
| Points | `question.set` | ✅ | — |
| Question banks, draw from bank | `bank.*` | ✅ | — |
| Results slide | `results.insert` | ✅ | — |
| Certificate slide | `certificate.insert` | ✅ (CourseCraft extra) | — |
| Feedback by choice | | 🟡 verify | M41 |
| Scoring by choice | | 🟡 | M41 |
| Insert Question dialog with filters and preview | new-slide popup | 🟡 | M38 |
| Question form view (grid editor) | inspector | ⛔ | M41 |
| Feedback layouts | — | ⛔ | M41 |
| Media in question form (per choice / feedback) | — | ⛔ | M41 |

## Player, preview, publish

| Feature | CourseCraft | Status | Milestone |
|---|---|---|---|
| Player: menu, resources, glossary, notes tabs | Course settings | ✅ | — |
| Player: title, logo, sidebar, seekbar modes, controls | Course settings | ✅ | — |
| Player colours light/dark, accent from theme | Course settings | ✅ | — |
| Navigation modes (free, restricted, locked) | `course.set_player` (Course settings › Menu access) | ✅ | — |
| Slides that override a course player setting, named under it, with Use on all slides | Course settings; `course.clear_slide_player_overrides` | ✅ | — |
| Preview slide, scene, selected slides, course | preview popup | ✅ | — |
| Publish web | `publish.rs` | ✅ | — |
| Publish LMS: SCORM 1.2, SCORM 2004, cmi5, xAPI, LTI | `publish.rs` | ✅ | — |
| Tracking by slides viewed / quiz | `course.set_tracking` (Course settings & Publish dialog); SCORM 1.2 Report status as offers Automatic (quiz result wins) and Automatic (completion stays) | ✅ | — |
| Publish video (MP4), Word, PDF | Export | ✅ | — |
| Notes as learner transcript | player | ✅ | — |
| Text labels (edit the player's own strings) | — | ⛔ verify | M43 |
| Player Properties dialog with its own ribbon and live preview | Course settings | 🟡 | M38 |
| Publish dialog: list on the left, property links | `publish.rs` | 🟡 | M38 |
| Preview: Replay slide / scene / course | | 🟡 | M43 |
| Save player / import player | | 🟡 verify | M43 |
| Preview device sizes (desktop, tablet, phone; both orientations) | — | ⛔ | M43 |
| Preview: Edit Slide | — | ⛔ | M43 |
| Preview: Inspect (variables, trigger log) | — | ⛔ | M43 |
| Swipe in player | — | ⛔ | M41 |
| Publish quality setting | — | ⛔ | M43 |
| Report to external LRS alongside LMS | | ⛔ verify | M43 |
| Share for review with comments | — | 🚫 | — |
| Hosted cloud delivery | — | — | — |

## Editor shell and panels

| Feature | CourseCraft | Status | Milestone |
|---|---|---|---|
| Undo / redo with history | `edit.history` | ✅ | — |
| Keyboard shortcuts sheet | `shortcuts.rs` | ✅ | — |
| Light / dark editor | View › Theme | ✅ | — |
| Scenes panel with scene combo | `slides.rs` | ✅ | — |
| Triggers panel | `triggers.rs` | ✅ | — |
| Slide layers panel | `layers.rs`; docked, a panel of its own to the right of the Timeline's group | ✅ | — |
| Timeline, States, Notes tabs | `timeline.rs`, `states.rs`; docked, Timeline, States, Triggers, Variables, Question and Notes are dock panels of their own, tabbed together by default (`dock.rs`); with the Notes panel open the Slide inspector leaves the notes to it | ✅ | — |
| Canvas zoom and fit | | ✅ | — |
| Tooltips with name, shortcut and help | | 🟡 | M38 |
| Status bar: slide n of m, size, theme, zoom | `status.rs` | 🟡 | M38 |
| Scene combo: mouse wheel switches scenes | | 🟡 | M38 |
| Right-click menus complete (canvas, timeline, scenes) | | 🟡 | M38 |
| Collapse panels | | 🟡 | M38 |
| Slide Properties in the Course Map | | 🟡 | M38 |
| **Tabbed ribbon** with contextual tabs | on by default (`ribbon/`): every main tab and the File page, replacing the menu bar, and the contextual tabs (Drawing, Button, Picture, Video, Audio, Table, Question Tools; `ribbon/contextual.rs`) and the theme, transition and animation galleries (`ribbon/gallery.rs`), and the Slide Master tab in Master View; the other modes' tab strips to come | 🟡 | M38 |
| Settings panels from the ↘ launchers, opening by themselves for the work in hand | `panels.rs` (`docs/ui/panels.md`): Text; the other categories to come | 🟡 | M38–M43 |
| Quick access toolbar | Save, Undo, Redo, Undo History left of the tab strip; Customize to come | 🟡 | M38 |
| Collapse ribbon | double-click the open tab, Ctrl+F1 (not while a dialog or text field has focus); a click opens a tab and never collapses | ✅ | — |
| View tabs for open slides | `doc_tabs.rs`: a Course Map tab, then a tab per opened slide (× closes it), under the ribbon | ✅ | — |
| Undock / float / redock panels | docked by default (`dock.rs`, craft-ui docking): move, tab, float, close and reopen (View › Panels), the four inspectors included; a floating panel can leave the app window as its own OS window (desktop, not Wayland); a panel dropped on the window's outer edge takes the whole width (bottom, top) or height (left, right); the arrangement is saved in the preferences; Preferences › Interface turns it off | ✅ | — |
| Reset panel layout | View › Panels › Reset Panel Layout | ✅ | — |
| Slide-flow arrows in the Scenes panel | — | ⛔ | M38 |
| Comments tab | — | ⛔ needs review workflow | — |

## AI

| Feature | CourseCraft | Status | Milestone |
|---|---|---|---|
| Narration from text | `audio_ai.rs` | ✅ | — |
| Alt text suggestions | `alt_text.rs` | ✅ | — |
| Translate course | `translate_ai.rs` | ✅ | — |
| Course from document | File › Import | 🟡 outline, not a quiz | M43 |
| Generate quiz from content | — | ⛔ | M43 |
| Generate one question | — | ⛔ | M43 |
| Summary slide | — | ⛔ | M43 |
| Insert / edit text with AI | — | ⛔ | M43 |
| Generate image | — | ⛔ | M43 |
| AI panel (chat with the connected agent) | — | ⛔ | M43 |

## Assets and services

| Feature | CourseCraft | Status | Milestone |
|---|---|---|---|
| Icon set | — | ⛔ an open-licence bundled set | M40 |
| Characters | — | 🚫 | — |
| Stock photos, illustrations, videos | — | 🚫 | — |
| Stock slide template library | — | 🚫 | — |
| 360° images and editor | — | 🚫 | — |
| Screen recording, software simulation | — | 🚫 patent watch list | — |
| Hosted review / comments | — | 🚫 | — |
| Shared cloud slides / hosted delivery | — | — | — |
