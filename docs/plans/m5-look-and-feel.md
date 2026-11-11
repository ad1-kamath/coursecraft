# M5 plan: look and feel (UI/UX)

Goal: CourseCraft's editor and learner player look and feel like a current, premium product. The bar is **Keynote's craft level** (user's choice): the category standard played straight, with no novelty theme. Quality comes from precision, hierarchy, spacing and finish. Product context: `PRODUCT.md`.

- **Editor:** slide-first and pro-dense, but obvious for newcomers. The default theme is dark (refined neutral greys, not egui's stock look), light is one click away, and a "follow the system" option is available.
- **Player:** minimal and content-first. The course is the hero; the chrome is a slim floating control strip that fades when idle, plus a slide-out outline drawer and a hairline progress rule.

Everything stays egui 0.37. Tokens live in one place and are shared by the editor and the player. Every control still dispatches engine commands (golden rule 2).

## Design system (the spec)

### Colour roles

Both themes meet WCAG AA for text roles on their surfaces; this is unit-tested. Hex values are sRGB.

| Role | Dark (default) | Light | Use |
|---|---|---|---|
| `app` | `#1C1C1E` | `#ECECEE` | window ground |
| `canvas` | `#161618` | `#E3E3E6` | backdrop behind the slide |
| `panel` | `#252528` | `#F6F6F7` | toolbar, navigator, inspector, timeline, status bar |
| `raised` | `#2F2F33` | `#FFFFFF` | popovers, menus, dialogs, cards |
| `input` | `#1E1E21` | `#FFFFFF` | text fields, drag values, combo buttons |
| `hover` | `#323236` | `#E9E9EC` | hover fill on panel |
| `pressed` | `#3A3A3F` | `#DEDEE2` | pressed or neutral-selected fill |
| `border` | `#36363A` | `#E1E1E4` | hairlines and separators (1 px) |
| `border_strong` | `#48484E` | `#C9C9CE` | input outlines and focused-panel edges |
| `text` | `#F2F2F5` | `#1D1D1F` | primary text |
| `text_secondary` | `#A8A8B0` | `#5E5E66` | labels, section headers |
| `text_tertiary` | `#8E8E96` | `#6E6E75` | placeholders, hints, timecodes |
| `text_disabled` | `#55555B` | `#B4B4BA` | disabled |
| `accent` | `#0A6CFF` | `#0066E0` | primary buttons, selection handles, active tab (white text on it ≥ 4.5:1) |
| `accent_hover` | `#2B80FF` | `#1A75E6` | |
| `accent_pressed` | `#0058D6` | `#0055BD` | |
| `accent_text` | `#4D9DFF` | `#0066E0` | accent used as text or icons on panel |
| `accent_subtle` | `#1C3556` | `#DCEBFF` | selected row or thumbnail fill, text selection |
| `success` | `#30D158` | `#1F9D44` | |
| `warning` | `#FFD60A` | `#B36B00` | |
| `danger` | `#FF453A` | `#D70015` | |
| `focus` | `accent` | `accent` | 2 px focus ring, 2 px outside the control |

The course's own `theme` (accent, background, text, font) styles course content only, never the editor chrome. The player chrome uses its own neutral tokens plus the course accent (see Player).

### Typography

Inter is bundled (regular, bold, italic and bold-italic); monospace stays egui's default. Sizes are in points.

| Style | Size / weight | Use |
|---|---|---|
| `display` | 28 bold | welcome screen title |
| `heading` | 20 bold | dialog titles, empty-state titles |
| `title` | 15 bold | panel titles, inspector tab content headers |
| `body` | 13 regular | default UI text, buttons, fields |
| `body_strong` | 13 bold | emphasised labels |
| `caption` | 11 bold, `text_secondary` | section headers inside panels (sentence case, not uppercase) |
| `small` | 11 regular | hints, metadata, thumbnail numbers |
| `mono` | 12 | ids, timecodes |

### Spacing, size, shape

- Spacing scale (4 pt base): 2, 4, 6, 8, 12, 16, 20, 24, 32, 48.
- egui spacing: `item_spacing (8, 6)`, `button_padding (10, 5)`, `interact_size.y 26` (compact controls), `indent 16`. Panel inner margin 12; gap between inspector sections 16, with 20 above a section header and 8 below it.
- Control heights: 26 compact (inspector, timeline), 30 toolbar buttons, 32 primary dialog buttons.
- Radii: 4 (checkbox, small chips), 6 (buttons, inputs, thumbnails), 8 (popovers, menus, cards), 12 (dialogs, the welcome card).
- Strokes: 1 px hairline `border`. Widgets have no outline at rest except inputs (`border_strong` 1 px). Focus is a 2 px `focus` ring.
- Shadows:
  - popovers and menus: offset (0, 6), blur 20, `#00000066` dark / `#0000002A` light;
  - dialogs: offset (0, 12), blur 32;
  - the slide on the canvas: offset (0, 8), blur 24, `#00000080` dark / `#00000033` light, plus a 1 px `border` outline.
- Motion: 120 ms for hover and press, 180 ms for panels and popovers, 260 ms for drawers; ease-out. egui `animation_time = 0.12`. Respect reduced motion where the platform reports it (web: `prefers-reduced-motion`).
- Scrollbars: thin floating bars (egui `ScrollStyle::floating()`), `text_tertiary` handle.

### Iconography

A single outline icon family drawn at 16 px (toolbar 18 px), 1.5 px stroke, `text_secondary` at rest, `text` on hover, `accent_text` when active. Source: Phosphor Icons (MIT, user's choice), bundled in `crates/render/fonts` as subset regular and fill fonts (fill marks active or selected states); the icon list and codepoints are in `phosphor-icons.txt` and `phosphor-codepoints.txt`. Every icon-only button has a tooltip with its name and shortcut.

### Editor layout (Keynote-like)

- **Menu row** (28 px, `panel`): File, Edit, Insert, Arrange, View, Help, compact, unchanged commands.
- **Toolbar** (52 px, `panel`, hairline below): labelled icon buttons (icon over an 11 pt label, like Keynote):
  - left: Add Slide (with layout menu) and Preview;
  - centre group: Text, Shape, Image, Media, Button, Question, Hotspot;
  - right: Theme and the inspector toggles Format, Animate, Slide, Course.
- **Slide navigator** (left, 220 px, resizable 180–320):
  - numbered 16:9 thumbnails with 6 px radius, grouped under scene headers;
  - selected: 2 px `accent` ring plus `accent_subtle` behind the number; hover reveals a "⋯" menu (duplicate, delete, move);
  - drag to reorder, where it exists;
  - the footer has an "Add slide" button.
- **Canvas** (`canvas` ground): the slide is centred with its shadow and outline. The layers strip is a row of segmented pills above the slide. A zoom control at the bottom right offers Fit, 50%, 100% and 200%.
- **Inspector** (right, 300 px, resizable 260–400): a segmented control at the top (Format, Animate, Slide, Course). It replaces the stacked collapsing headers. Sections use `caption` headers, and rows are a label on the left with the control on the right.
- **Bottom panel** (timeline, triggers, variables, question): a segmented tab bar; rows alternate a subtle zebra; timeline bars use 4 px radius in `accent` (selected) or `pressed` (others); a time ruler in `text_tertiary`.
- **Status bar** (26 px): the course title on the left; the validation chip shows a green dot for 0 errors, amber for warnings, red for errors, and opens the list; undo state and the automation chip are on the right.
- **Dialogs:** `raised`, 12 px radius, dialog shadow, a 20 pt heading, primary action at the bottom right in `accent`, Cancel as a quiet button.

### Empty states and onboarding

- **Welcome screen** (no project open; also File ▸ New Window on desktop):
  - CourseCraft wordmark and a short line;
  - actions: New blank course, Open…, Import PowerPoint…, Open an example (hello-course, showcase);
  - recent files with path and date;
  - a "What's new" link to the docs.
- **Empty slide:** a quiet centred hint with the Text, Shape and Image actions.
- **Empty panels** (no triggers, variables or layers): a one-line explanation plus the primary action, never a blank area.
- **First run:** three dismissible coach marks (navigator, toolbar, inspector), remembered in prefs.
- **Shortcuts:** `?` opens a cheat sheet listing every command with a shortcut from the registry.

### Player (learner-facing)

- The course is full-bleed and letterboxed on a neutral ground (`#0E0E10`, or a dark tint of the course background).
- **Control strip:** floating, 44 px high, 12 px radius, `#1C1C1EE6`, 12 px from the bottom edge and centred, width fitted to its content.
  - Contents: outline (drawer), previous, current slide title and "3 / 12", next. A replay-slide control is included only if the runtime supports it; no new behaviour is invented.
  - Icons are 18 px; hover is `#FFFFFF1A`; the next button uses the course accent fill when the slide is complete.
- **Auto-hide:** the strip fades out after 2.5 s without pointer or keyboard activity, but only while nothing is focused in it and the menu is closed. It fades back in on movement, touch or focus. With reduced motion it is always shown.
- **Progress rule:** a 3 px hairline along the very bottom across the full width. It shows course progress segmented per slide (visited segments in the course accent, the current slide filling with its timeline) and stays visible when the strip hides.
- **Outline drawer:** slides in from the left (320 px, `raised` dark), with scenes and slides, a check mark on visited slides and the current slide highlighted. Esc or a click outside closes it.
- **Keyboard:** ← / → for previous and next (when allowed), M for the outline, Esc closes it, visible focus rings.
- **Loading:** a centred course title with a thin progress line, not a blank screen.
- Course `theme.player.bar` and `bar_text` map onto the strip surface and text, with defaults equal to the tokens above. The existing `show_menu`, `show_seekbar` and `show_nav` flags keep working: the seekbar flag maps to the progress rule.

## Steps

### 1. Foundation: tokens, themes, editor snapshots
- `coursecraft-render`: a `design` module with the palettes, type, spacing, radii and shadow tokens above, `apply_style(ctx, mode)` and `palette(ctx)`, plus WCAG contrast tests.
- An offscreen `render_ui(...)` that rasterises any egui UI (reusing the 8b rasteriser), and an editor snapshot tool that writes PNGs of the editor with a course loaded in both themes. Agents and reviewers use it; no browser is needed.
- Editor theme preference (dark default, light, system) with a View menu entry.
- Global restyle through tokens: panel fills and hairlines, canvas backdrop and slide shadow, section headers, inputs, buttons, tabs, timeline, status bar, dialogs. Hard-coded chrome colours are replaced with roles.

### 2. Icons and toolbar
Register the Phosphor fonts (already bundled), add an `icon` helper, the Keynote-style toolbar, and icon buttons across panels with tooltips.

### 3. Panels
The slide navigator, the canvas zoom and layer pills, the segmented inspector (Format, Animate, Slide, Course), and the bottom-panel tabs and rows.

### 4. Empty states and onboarding
The welcome screen, empty slide and panel states, first-run coach marks, and the shortcut cheat sheet.

### 5. Player chrome
The floating auto-hiding strip, progress rule, outline drawer, keyboard support, loading screen, and theme mapping. Shared by the published player and the editor preview.

### 6. Polish and review
- A consistency pass over every dialog and form, plus the M4 polish list in `docs/plans/m4-mvp.md`: prompt placement, fill-in padding, matching truncation, drag-drop labels and ellipse segments.
- Snapshot review in both themes and the player at phone, tablet and desktop sizes.
- `DESIGN.md` recorded from the built result. Roadmap: M5 ✅.

## Done bar

- Snapshots of the editor in dark and light, and of the player at three sizes, read as one coherent modern product at Keynote's craft level.
- A newcomer can start a course from the welcome screen without docs.
- All existing tests pass, the walkthrough test still builds hello-course through the UI, and wasm builds.
