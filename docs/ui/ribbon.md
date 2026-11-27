# Ribbon: main tabs

> Part of [the ribbon plan](README.md). Status marks are explained there.
>
> **Kinds:** `L` large button · `LS` large split button (▾ menu) · `S` small button · `SS` small
> split button · `C` combo box · `G` gallery · `N` number spinner · `K` checkbox · `T` toggle ·
> `↘` dialog launcher in the group caption.
>
> **Action:** an engine command id (`slide.add`), a UI action (`ui:preview.slide`, defined in
> `coursecraft-ui-egui`), or `—` when the control is disabled. Ids marked `(new)` do not exist yet.
> "Existing UI" names where the feature lives today, so the wiring reuses it instead of writing it
> again.

## Tab strip

| Order | Tab | Shown when |
|---|---|---|
| 0 | **File** | always; opens the File page (below), drawn in the accent colour |
| 1 | **Home** | always; Course Map variant or Slide variant |
| 2 | **Insert** | slide open |
| 3 | **Slides** | always (hidden in master mode) |
| 4 | **Design** | always (hidden in master mode) |
| 5 | **Transitions** | slide open |
| 6 | **Animations** | slide open |
| 7 | **View** | always |
| 8 | **AI** | always; CourseCraft's own tab |
| 9 | **Help** | always |
| 10+ | contextual | see [contextual-tabs.md](contextual-tabs.md) |

In the Course Map, Insert, Transitions and Animations stay in the strip but every control in them is
disabled with the tooltip `Open a slide first`. (This keeps the strip from jumping.)

Every main tab but AI and Help ends with a **Publish** group (Preview at least). Its controls are the same
everywhere and are specified once, under Home.

## File

**Built** (`file_page.rs`): the File tab opens it over the window; an accent column of entries
(Back, New, Open…, Open Folder…, Save, Save As…, Save As Project Folder…, Save Slide as Template,
Version History…, Start, Recent, Import, Export, Translation, Fonts, Publish…, Course Info…, Check
Course, Preferences…, Close ⛔, Quit), and the chosen section's choices on the right. Every action
closes the page; Esc and Back return. **It is also the launch page** (2026-11-24): with the ribbon
on, CourseCraft opens on the File page at **Start** (new blank course, open, PowerPoint, the
examples, the templates and recent courses, `welcome::start_content`); until a course is open,
Back, Esc and the entries that need a course are off ("Open or create a course first"), and
Help › Welcome opens it again. With the ribbon off the old welcome card is drawn. Recent courses
show their first slide as a thumbnail (the desktop app reads each `course.json` once in the
background, `apps/coursecraft/src/recent_previews.rs`; media isn't loaded, so pictures show as
placeholders). Still to do: news.

Not a ribbon tab: a full-window **File page**, the same page CourseCraft shows on launch
(today's welcome screen, `crates/ui-egui/src/welcome.rs`). It has an accent-coloured left column
with the start actions, Recent and Browse for more…, and a right area with CourseCraft news
(release notes from the app, no network call) and links. Esc or the Back entry returns to the
editor. It replaces today's File menu (`menu.rs` `show_menu_bar`) and keeps all of it:

| Start action (left column, top) | Action | Status | Existing UI |
|---|---|---|---|
| New Project | `ui:file.new` | ✅ | welcome screen, File › New |
| Record Screen | — | 🚫 | Not planned (patent watch list) |
| Import ▾ (PowerPoint…, Questions from CSV…, Course from Document…) | `ui:file.import.*` | ✅ | File › Import |
| Team Slides | — | — | shared cloud library; not drawn |
| Recent (thumbnail + name) | `ui:file.openRecent` | ✅ | first-slide thumbnail and name, in Start and Recent (pictures as placeholders) |
| Browse for more… | `ui:file.open` | ✅ | File › Open… |

Project commands (below the start actions once a course is open):

| Item | Action | Status | Existing UI |
|---|---|---|---|
| New | `ui:file.new` | ✅ | File › New |
| Open… | `ui:file.open` | ✅ | File › Open… |
| Open Folder… | `ui:file.openFolder` | ✅ | File › Open Folder… (CourseCraft extra, keep) |
| Open Recent ▸ | `ui:file.openRecent` | ✅ | File › Open Recent |
| Save | `ui:file.save` | ✅ | File › Save |
| Save As… | `ui:file.saveAs` | ✅ | File › Save As… |
| Import ▸ PowerPoint…, Questions from CSV…, Course from Document…, Outline | `ui:file.import.*` | ✅ | File › Import |
| Translation ▸ Export…, Import…, Translate with AI… | `course.export_xliff`, `course.import_xliff`, `ui:translate.ai` | ✅ | File › Export/Import Translation |
| Publish… | `ui:publish` | ✅ | toolbar Publish |
| Export ▸ Video (MP4)…, Word for review…, PDF for review… | `ui:export.*` | ✅ | File › Export |
| Course Info… | `course.info` | 🟡 | Course settings |
| Version History… | `ui:versionHistory` | ✅ | `version_history.rs` |
| Preferences… | `ui:preferences` | ✅ | File › Preferences… |
| Close | `ui:file.close` | ⛔ | today New replaces the course; add a real Close (drawn disabled) |
| Save Slide as Template | `template.save` | ✅ | Project › Save as Template |
| Fonts ▸ Add Font…, Manage Fonts… | `ui:fonts.*` | ✅ | File › Add font…, Manage fonts… |
| Check Course | `course.validate` | ✅ | File › Check Course |
| Quit | `ui:quit` | ✅ | File › Quit |

## Home (Course Map variant)

Shown when the Course Map view tab is active.

> **In the code (M38 steps 1–2):** every main tab (Home in both variants, Insert, Slides, Design,
> Transitions, Animations, View, Help) is drawn from
> `crates/ui-egui/src/ribbon/spec.rs`; the ribbon is on by default (Preferences › Interface) and
> replaces the flat toolbar, with Save, Undo and Redo in a quick access toolbar left of the tab
> strip. The Status column is the feature's status. Built features that the ribbon doesn't reach
> yet (Hyperlink, Apply Layout, Question Banks, Quick Styles) are drawn disabled with "Not
> available yet: ribbon wiring". Shape Fill, Shape Outline and Shape Effect open the Shape Style
> settings panel at their section. Groups that
> don't fit the window shrink to one button, rightmost first, which opens the group in a popup.
>
> **Font and Paragraph** go through `crates/ui-egui/src/text_format.rs`, the one place that
> resolves what a text control acts on (the characters selected in the text being edited, its
> caret, or every selected text box and shape whole), the state the toggles and combos show
> (`None` where mixed), and the engine commands a change runs, as one undo step with any typed
> text committed first. Ctrl+B, Ctrl+I and Ctrl+U take the same path. A click on the ribbon
> leaves the in-place editor open. Text inside a state and button labels stay with the Text panel
> and the Format inspector; the controls say so.

| Group | Control | Kind | Action | Status | Notes |
|---|---|---|---|---|---|
| Clipboard | Paste | LS | `slide.paste` | ✅ | pastes slides in the map |
| | Cut | S | `ui:edit.cut` | ✅ | |
| | Copy | S | `ui:edit.copy` | ✅ | |
| Scenes | New Scene | L | `scene.add` | ✅ | `toolbar::add_new_scene` |
| | Start Section | T | `course.update` `{start_scene}` (new field) | ⛔ | the course model has no start-section field yet; the course starts at the first slide. Needs the field, a flag in the map and a runtime rule in `triggers.md` |
| | Question Banks | LS | `bank.create`, `ui:banks.manage` | ✅ | ▾ New Question Bank, Manage Banks…, Import Questions from CSV… |
| Slide | New Slide | LS | `slide.add`, `slide.add_from_layout`, `template.add_slide` | 🟡 | ▾ menu below |
| | Apply Layout | SS | `slide.set_master` | ✅ | |
| | Reset | S | `slide.set_master` (reapply) | ⛔ | reset placeholders to the layout |
| | Duplicate | S | `slide.duplicate` | ✅ | |
| | Record Screen | LS | — | 🚫 | Not planned (patent watch list: Assima) |
| Publish | Player | L | `ui:player.properties` | ✅ | Course settings › Player; slides that set their own player value are named under the course setting, with Use on all slides (`course.clear_slide_player_overrides`) |
| | Preview | LS | `ui:preview.*` | ✅ | ▾ This Slide, This Scene, Selected Slides, Entire Course, Publish… (`toolbar` preview popup) |
| | Publish | L | `ui:publish` | ✅ | |

**New Slide ▾** (shared by every tab that has New Slide):

| Item | Action | Status |
|---|---|---|
| Library… | — | 🚫 Not planned: stock asset library |
| My Templates… | `template.add_slide` | ✅ (`slides::show_new_slide_popup`) |
| Team Slides… | — | — not drawn |
| Basic Layouts ▸ (gallery of the course's slide layouts, headed by the theme name) | `slide.add_from_layout` | ✅ |
| Import ▸ PowerPoint…, Questions from CSV… | `ui:file.import.*` | ✅ |
| Graded Question… / Survey Question… / Custom Question… | `question.insert` | ✅ |
| Interaction… | `interaction.insert` | ✅ (CourseCraft extra) |
| Results… | `results.insert` | ✅ |
| Record Screen… | — | 🚫 |

## Home (Slide variant)

| Group | Control | Kind | Action | Status | Notes |
|---|---|---|---|---|---|
| Clipboard | Paste | LS | `object.paste` | ✅ | ▾ Use Destination Theme (default), Keep Source Formatting, Keep Text Only, Paste Special… (`clipboard.rs`) |
| | Cut | S | `ui:edit.cut` | ✅ | |
| | Copy | SS | `ui:edit.copy` | ✅ | ▾ Copy, Duplicate (`object.duplicate`) |
| | Format Painter | S | `ui:formatPainter` | ✅ | double-click keeps it on |
| | Select All | S | `ui:edit.selectAll` | ✅ | from Edit › Select All |
| Slide | New Slide | LS | as Course Map | ✅ | |
| | Apply Layout | SS | `slide.set_master` | ✅ | |
| | Focus Order | S | `ui:focusOrder` | ✅ | Focus Order window (`focus_order.rs`): grouped by layer (base first, each layer a section; reorder inside a section only), drag, reset; OK sets `focus_order` 1..n in one undo step; closes when the slide or course changes under it. Lists only keyboard-focusable objects: the player reads text and pictures to a screen reader in drawing order, so a reading-order list for them is a feature gap |
| | Duplicate | S | `slide.duplicate` | ✅ | |
| Font ↘ | Text Styles | LS | `text_style.apply`, `text_style.create` | ✅ | ▾ course styles, New Style from Selection, Manage Styles… |
| | Font family | C | `text.style_range` `{font}` | ✅ | ▾ the inspector's font picker (`font_picker::show_font_list`): search, the course's fonts, recent and installed fonts, Add font… |
| | Font size | C | `text.style_range` `{size}` | ✅ | ▾ 8–96 |
| | Grow Font / Shrink Font | S | `text.style_range` `{size}` | ✅ | next size in the list |
| | Clear Formatting | S | `text.clear_formatting` | ✅ | keeps links; nothing at a bare caret |
| | Bold / Italic / Underline | T | `text.style_range` | ✅ | |
| | Shadow (text) | T | `text.style_range` `{shadow}` | ✅ | on, mixed or off for the selection |
| | Strikethrough | T | `text.style_range` | ✅ | |
| | Character Spacing | SS | `text.style_range` `{letter_spacing}` | ✅ | ▾ Very Tight … Very Loose; exact value in the Text panel |
| | Change Case | SS | `text.change_case` | ✅ | ▾ Sentence case, lowercase, UPPERCASE, Capitalize Each Word, tOGGLE cASE |
| | Highlight | SS | `text.style_range` `{highlight}` | ✅ | a bar under the icon shows the last colour picked (yellow at first); the main part applies it, ▾ lists theme colours, 8 standard colours, No Color |
| | Font Color | SS | `text.style_range` `{color}` | ✅ | a bar under the icon shows the last colour picked (the theme accent at first); the main part applies it, ▾ opens the inspector's colour picker (`properties::color_picker_body`): eyedropper, palette, theme and recent swatches, colour wheel (one change per drag) |
| | Spelling | S | `ui:spellCheck` | ✅ | `spell_check.rs` |
| | ↘ Font | — | `UiAction::PanelAt(Text, Font)` | ✅ | opens the Text settings panel at Font ([panels.md](panels.md)) |
| Paragraph ↘ | Bullets | SS | `text.set_paragraphs` `{list}` | ✅ | toggles on the paragraphs at the selection; bullet symbols in the Text panel |
| | Numbering | SS | `text.set_paragraphs` `{list, number_scheme, start_at}` | ✅ | split: the main part toggles; ▾ the 7 schemes with samples and Start at |
| | Decrease / Increase Indent | S | `text.set_paragraphs` `{level}` | ✅ | levels 0–4 |
| | Line Spacing | SS | `text.set_paragraphs` `{line_spacing}` | ✅ | ▾ 1.0–3.0 |
| | Align Left / Center / Right / Justify | T | `text.set_paragraph` `{align}` | ✅ | |
| | Text Direction | SS | `text.set_paragraph` `{direction}` | 🟡 | ▾ Horizontal, Rotate all text 90°, Rotate all text 270°; stacked is not built |
| | Align Text | SS | `text.set_paragraph` `{valign}` | ✅ | ▾ top / middle / bottom |
| | Wrap Text | S | `text.set_paragraph` `{wrap}` | ✅ | CourseCraft extra; turning Wrap on converts point text to fixed-width area text |
| | Find / Replace | SS | `course.find`, `course.replace` | ✅ | `find.rs` |
| | ↘ Paragraph | — | `UiAction::PanelAt(Text, Paragraph)` | ✅ | opens the Text settings panel at Paragraph |
| Drawing ↘ | Arrange | LS | `object.reorder`, `object.align`, `object.distribute`, `object.group`, `group.set_interactive` | ✅ | ▾ Order, Group, Ungroup, Make button from group / Remove button behaviour, Align, Rotate (as Edit › Order/Align) |
| | Quick Styles | LS | `object.apply_style` | ✅ | |
| | Shape Fill | SS | `object.set_fill_style` | ✅ | opens the Shape Style panel at Fill |
| | Shape Outline | SS | `object.update` `{outline}` | ✅ | opens the Shape Style panel at Outline |
| | Shape Effect | SS | `object.set_shadow`, `object.set_glow` | 🟡 | opens the Shape Style panel; Shadow ✅, Glow ✅, Reflection ⛔, Soft Edges ⛔ |
| | ↘ Format Shape | — | `UiAction::PanelAt(ShapeStyle, Fill)` | ✅ | opens the Shape Style settings panel: Fill, Outline and Effects, with the Format inspector's own rows ([panels.md](panels.md)) |
| Publish | Player / Preview / Publish | | as Course Map | ✅ | |

## Insert

| Group | Control | Kind | Action | Status | Notes |
|---|---|---|---|---|---|
| Slide | Slide Layer | L | `layer.add` | ✅ | `toolbar::add_new_layer`. **Insert is built in `spec.rs`**; the menu bar's Insert menu is hidden while the ribbon is on |
| | Convert to Custom Question | L | `question.set_kind` | ⛔ | turn the slide's objects into a custom question |
| | Camera Zoom | L | — | ⛔ | camera zoom on part of the slide |
| AI | Insert Text, Edit Text, Insert Image, Insert Audio, Quiz, Question, Summary | | on the AI tab | | |
| Library | Characters | L | — | 🚫 | Not planned: characters |
| | Photos | L | — | 🚫 | Not planned: stock asset library |
| | Illustrations | L | — | 🚫 | |
| | Icons | L | `ui:insert.icon` | ⛔ | CourseCraft ships no icon set yet; a bundled open icon set would make this ✅ |
| | Videos | L | — | 🚫 | |
| Media | Picture | LS | `object.add` `{image}` | ✅ | ▾ Picture from File… ✅ (PNG, JPEG, WebP, GIF, SVG; EMF and WMF converted to SVG, `coursecraft_media::metafile`), Media Library… ✅ (`media_library.rs`), Screenshot ⛔ |
| | 360° Image | LS | — | 🚫 | Not planned: 360° images |
| | Shape | LS | `object.add` `{shape}` | ✅ | `shape_gallery.rs` |
| | Caption | LS | `object.add` `{shape: callout}` | ✅ | opens the shape gallery filtered to Callouts (`shape_gallery.rs`) |
| | Video | LS | `object.add` `{video}` | 🟡 | ▾ Video from File… ✅, From Website (embed) ✅ (`web_insert.rs`), Record Webcam ⛔, Media Library… ✅ |
| | Audio | LS | `object.add` `{audio}` | 🟡 | ▾ Audio from File… ✅, Media Library… ✅, Record Mic… ✅ (`record_narration.rs`), Text-to-Speech… ✅ (`audio_ai.rs`), AI Audio ▸ ⛔, Background Audio ▸ ⛔ (not in the menu; set it in Course Settings, `course.set_background_audio`), Options… ⛔ |
| | Web Object | L | `object.add` `{web}` | ✅ | `web_insert.rs` |
| | Animation (Lottie) | L | `object.set_lottie` | ✅ | CourseCraft extra; place after Web Object |
| Text | Text Box | LS | `object.add` `{text}` | ✅ | |
| | Table | LS | `object.add` `{table}` | ✅ | grid picker on ▾ |
| | Slide Number | LS | `ui:insertSlideNumber` | ✅ | ▾ Slide Number, Slide Number of Total; `%Course.SlideNumber%` at the caret or in a new text box |
| | Symbol | S | `ui:insertSymbol` | ✅ | Symbol window (`insert_text.rs`); the set is limited to the bundled font's glyphs |
| | Reference | S | `ui:insertReference` | ✅ | Reference window: built-in and course variables, inserted as `%Name%` |
| | Hyperlink | S | `text.set_link` | ✅ | |
| Interactive Objects | Button | LS | `object.add` `{button}` | ✅ | ▾ button styles |
| | Slider | LS | `object.add` `{slider}` | ✅ | `input_insert.rs` |
| | Dial | LS | `object.add` `{dial}` | ✅ | |
| | Hotspot | LS | `object.add` `{hotspot}` | ⛔ | no hotspot object yet: hotspot questions have their own regions and the Hotspot reveal interaction covers clickable areas; ▾ rectangle, oval, freeform to come |
| | Input | LS | `object.add` `{text_entry}` / `object.set_input` | ✅ | ▾ Text Entry, Checkbox, Radio Buttons |
| | Marker | LS | `object.set_marker` | ✅ | |
| | Trigger | S | `trigger.add` | ✅ | |
| | Scrolling Panel | S | `object.set_scroll_panel` | ✅ | |
| | Mouse | SS | `slide.update` `{cursor}` | ⛔ | custom cursor per slide |
| | Interaction | L | `interaction.insert` | ✅ | CourseCraft extra (`interaction_gallery.rs`) |
| Publish | Preview | LS | | ✅ | |

## Slides

| Group | Control | Kind | Action | Status |
|---|---|---|---|---|
| Slide | New Slide | LS | `slide.add` | ✅ (built in `spec.rs`, as is the rest of this tab) |
| | Import | LS | `ui:file.import.*` | ✅ ▾ PowerPoint…, Questions from CSV…, Course from Document…, Slides from Outline… |
| | Duplicate | S | `slide.duplicate` | ✅ |
| Scene | New Scene | L | `scene.add` | ✅ |
| Questions | Graded / Survey / Custom Question | LS | `question.insert` | ✅ (one Question ▾ listing the kinds) |
| | Question Banks | LS | `bank.*` | ✅ (drawn disabled: ribbon wiring) |
| | Results Slide | L | `results.insert` | ✅ |
| | Certificate | L | `certificate.insert` | ✅ (CourseCraft extra) |
| Masters | Slide Master | L | `ui:view.master` | ✅ |
| | Feedback Layouts | L | — | ⛔ |
| Publish | Preview | LS | | ✅ |

## Design

| Group | Control | Kind | Action | Status | Notes |
|---|---|---|---|---|---|
| Setup | Slide Size | L | `course.set_slide_size` | ✅ | `slide_size_dialog.rs`. In the code: Colors and Fonts open the theme window; Background Styles waits for ribbon wiring (↘ opens the Slide inspector) |
| Themes | theme gallery | G | `course.set_theme` | ✅ | `ribbon/gallery.rs`: eight built-in themes of our own (accent, background, text), ▾ shows them all, the current one is marked; Colors and Fonts open the theme window (`theme_dialog.rs`) |
| Theme options | Colors | SS | `theme.set_palette` | ✅ | ▾ palettes, Create New Theme Colors… (`theme.set_palette_color`) |
| | Fonts | SS | `UiAction::ThemeFonts` → `course.set_theme` `{font, heading_font}` | ✅ | the main button opens the Create New Theme Fonts window (heading and body fonts) directly; ▾ lists font pairs (heading / body) and Create New Theme Fonts… |
| Background ↘ | Background Styles | SS | `slide.set_background` | ✅ | |
| | Hide Background Graphics | K | `slide.update` `{hide_master_graphics}` | ⛔ | |
| Publish | Preview | LS | | ✅ | |

## Transitions

| Group | Control | Kind | Action | Status |
|---|---|---|---|---|
| Transition to This Slide ↘ | transition gallery | G | `slide.set_transition` | ✅ (`ribbon/gallery.rs`: None plus the 15 transitions of the registry: Fade, Fade through black, Push, Cover, Uncover, Reveal, Wipe, Split, Zoom, Shape, Clock, Dissolve, Random bars, Blinds, Checkerboard; each tile draws its own sample (old slide grey, new slide accent) and plays on hover; a pick keeps the options the new transition also takes; ↘ opens the Slide inspector) |
| | Effect Options | LS | `slide.set_transition` `{from, orientation, inout, shape, spokes}` | ✅ ▾ built from the registry like the animation menus: a compass of eight arrows for the moving and wiping transitions, rows with icons for orientation, in/out, shape and spokes; off for a transition without options |
| Timing | Duration | N | `slide.set_transition` `{duration_ms}` | ✅ seconds, one undo step |
| | Apply To All | S | `slide.set_transition_all` | ✅ every slide of every scene, one undo step |
| Publish | Preview | LS | | ✅ |

## Animations

| Group | Control | Kind | Action | Status | Notes |
|---|---|---|---|---|---|
| Advanced | Animation Painter | L | `ui:animationPainter` → `object.copy_animation` | ✅ | like the Format Painter: click arms it from the selected object, double-click keeps it on, a click on an object copies its entrance, exit and emphasis (emphasis keeps its offset from the object's start), Esc disarms |
| Entrance Animations ↘ | effect gallery | G | `object.set_animation` `{entrance}` | ✅ | `ribbon/gallery.rs`: None plus every effect in the registry (25: Appear, Fade, Grow, Fly, Float, Split, Wipe, Shape, Wheel, Random bars, Zoom, Dissolve, Blinds, Peek, Expand, Rise up, Spin, Spin & grow, Grow & turn, Checkerboard, Strips, Flip, Swivel, Bounce, Pop), on every selected object; each tile's icon is the effect drawn by its own sample function on a small card (entrance green, exit red; it plays in a loop on hover; `ribbon/effect_art.rs`); hovering a tile also plays it on the selected objects on the canvas, and a pick plays once (`anim_preview.rs`, Auto-preview); ↘ opens the Animate inspector; Add Motion Path runs `object.add_motion_path` |
| | Duration | N | `object.set_animation` `{duration_ms}` | ✅ | seconds |
| | Effect Options | SS | `object.set_animation` `{from, shape, inout, spokes, orientation, sequence}` | ✅ | ▾ built from the registry, one section per option the current effect takes: Direction as a 3×3 compass of arrows (only the sides and corners the effect takes), Shape, In or out, Spokes and Orientation as rows with icons; for an object with text, a Sequence section (As one object, By paragraph, By letter); off when there is nothing to choose |
| Exit Animations | (same three) | | `object.set_animation` `{exit}` | ✅ | |
| Emphasis Animations | Add Emphasis Animation | G | `object.add_emphasis` | ✅ | `ribbon/gallery.rs`: the 12 emphasis effects as amber tiles drawn by their own sample functions; a pick adds one to every selected object at the playhead (or at the object's start when the playhead is outside its time on screen) and plays it once; hovering plays it in a loop on the canvas |
| | emphasis list | G | `object.set_emphasis` | ✅ | in the Animate inspector's Emphasis card (one row each), not in the ribbon |
| | Duration / Effect Options / Delete | | `object.set_emphasis`, `object.remove_emphasis` | ✅ | in the Emphasis card: Start, Duration, Repeat, ▶ and Remove per emphasis |
| Motion Paths | Add Motion Path | LS | `object.add_motion_path` | ✅ | |
| | Duration, Name | N, C | `object.update_motion_path` | ⛔ | edited in the Animate panel; no ribbon control yet |
| | Path Options | LS | `object.update_motion_path` | ⛔ | no ribbon control yet |
| Size and Position ↘ | Size and Position | L | `UiAction::PanelAt(SizePosition, Size)` | ✅ | the button and ↘ open the Size and Position settings panel (`object.set_frame`, `object.set_rotation`, `object.flip`, `object.lock`) |
| Publish | Preview | LS | | ✅ | |

## View

| Group | Control | Kind | Action | Status |
|---|---|---|---|---|
| Views | Course Map | L | `ui:view.map` | ✅ (built in `spec.rs`, as is the rest of this tab) |
| | Slide | L | `ui:view.slide` | ✅ |
| | Slide Master | L | `ui:view.master` | ✅ |
| | Feedback Layouts | L | — | ⛔ |
| Zoom | Zoom | L | `ui:zoom.dialog` | 🟡 (Zoom In, Zoom Out, 100% for now) |
| | Fit in Window | L | `ui:zoom.fit` | ✅ |
| Show | Rulers | K | `view.set` `{rulers}` | ✅ |
| | Grid | K | `view.set` `{grid}` | ✅ |
| | Guides | K | `view.set` `{guides}` | ✅ |
| | Snap to Grid / Snap to Guides | K | `view.set` | ✅ |
| | ↘ Show | — | `UiAction::PanelAt(GridGuides, Grid)` | ✅ (opens the Grid and Guides settings panel: grid spacing, snapping, the guide list, rulers; [panels.md](panels.md)) |
| Panels | Timeline, States, Notes, Triggers, Layers, Scenes | K | `ui:panel.toggle` | 🟡 (Slides, Timeline, States, Triggers, Variables, Question, Notes, Slide Layers, the four inspectors and the settings panels, each its own panel; View › Panels lists them all, the ribbon has buttons for some) |
| | Reset Panels | S | `ui:panel.reset` | ✅ |
| Theme | Light / Dark | T | `ui:theme` | ✅ (Light, Dark, Match System) |
| Publish | Preview | LS | | ✅ |

## AI

CourseCraft's own tab: every AI control in one place, so Home keeps only editing.

| Group | Control | Kind | Action | Status | Notes |
|---|---|---|---|---|---|
| Write | Insert Text | L | — | ⛔ | draft text on the slide |
| | Edit Text | L | — | ⛔ | rewrite selected text |
| | Summary | L | `ui:ai.summary` (new) | ⛔ | |
| Media | Insert Image | L | — | ⛔ | |
| | Insert Audio | LS | `ui:narration.fromText` | 🟡 | ▾ Narration from Text… ✅ (`audio_ai.rs`), Sound Effect ⛔, Music ⛔ |
| Assess | Quiz | L | `ui:ai.quiz` (new) | ⛔ | draft a quiz from the course text through the user's agent |
| | Question | L | `ui:ai.question` (new) | ⛔ | |
| | Suggest Alt Text | L | `ui:alt.suggest` | ✅ | drawn in the Write group in the ribbon (`ai.alt_text`); from Project › Suggest Alt Text… (`alt_text.rs`) |
| Agents | Connect an AI Agent…, AI Settings…, Allow automation | S×3 | `ui:ai.*` | ✅ | moved from Help |

## Help

| Group | Control | Action | Status |
|---|---|---|---|
| Help | Help / Guide | `ui:help.guide` | ✅ (opens the docs on GitHub) |
| | Keyboard Shortcuts | `ui:help.shortcuts` | ✅ |
| | Welcome Screen | `ui:welcome` | ✅ |
| Community | CourseCraft on GitHub, Report a Problem…, Suggest a Feature… | `ui:help.*` | ✅ |
| Updates | Check for Updates | `ui:updates` | ✅ |
| About | About CourseCraft | `ui:about` | ⛔ (no About window yet; drawn disabled) |

### Where the menu bar went

With the ribbon on, the menu bar is not drawn. File is the File page; the rest:

| Menu item | Ribbon home |
|---|---|
| Edit › Undo, Redo | quick access toolbar, Ctrl+Z / Ctrl+Y |
| Edit › History… | quick access toolbar › Undo History |
| Edit › Cut, Copy, Paste, Duplicate | Home › Clipboard |
| Edit › Select All | Home › Clipboard › Select All, Ctrl+A |
| Edit › Check Spelling…, Find and Replace… | Home › Font › Spelling, Home › Paragraph › Find / Replace |
| Edit › Delete (object, slide, scene, layer, question) | Delete key and the right-click menus |
| Insert | Insert and Slides tabs |
| Arrange | Home › Drawing › Arrange |
| Project › Course settings, Player Settings, Quiz Settings | Home › Publish › Player; File › Course Info… |
| Project › Variables | the Variables tab of the timeline panel |
| Project › Theme… | Design › Themes |
| Project › AI Settings…, Suggest Alt Text…, Translate with AI… | AI tab; Translate with AI also File › Translation |
| Project › Save as Template | File › Save Slide as Template |
| View | View tab; Ribbon and Docked Panels in Preferences › Interface |
| Help | Help tab; agent and automation controls on the AI tab |

CourseCraft's old **Project** menu items (Course settings, Variables, Glossary, Resources, Audit,
Accessibility check, Suggest Alt Text…) are grouped by task: Course settings and Player go under
Home › Publish › Player; Variables, Glossary and Resources sit in Player Properties and the
Triggers panel; Audit and Accessibility go under Review controls in View until a Review tab is
decided.
