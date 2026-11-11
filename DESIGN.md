---
name: CourseCraft
description: Native eLearning authoring with a quiet pro editor and a content-first learner player.
colors:
  app: "#1C1C1E"
  canvas: "#161618"
  panel: "#252528"
  raised: "#2F2F33"
  input: "#1E1E21"
  hover: "#323236"
  pressed: "#3A3A3F"
  border: "#36363A"
  border-strong: "#48484E"
  text: "#F2F2F5"
  text-secondary: "#A8A8B0"
  text-tertiary: "#8E8E96"
  text-disabled: "#55555B"
  accent: "#0A6CFF"
  accent-hover: "#2B80FF"
  accent-pressed: "#0058D6"
  accent-text: "#4D9DFF"
  accent-subtle: "#1C3556"
  success: "#30D158"
  warning: "#FFD60A"
  adjust-handle: "#FFD60A"
  danger: "#FF453A"
  light-app: "#ECECEE"
  light-canvas: "#E3E3E6"
  light-panel: "#F6F6F7"
  light-raised: "#FFFFFF"
  light-input: "#FFFFFF"
  light-hover: "#E9E9EC"
  light-pressed: "#DEDEE2"
  light-border: "#E1E1E4"
  light-border-strong: "#C9C9CE"
  light-text: "#1D1D1F"
  light-text-secondary: "#5E5E66"
  light-text-tertiary: "#6E6E75"
  light-text-disabled: "#B4B4BA"
  light-accent: "#0066E0"
  light-accent-hover: "#1A75E6"
  light-accent-pressed: "#0055BD"
  light-accent-text: "#0066E0"
  light-accent-subtle: "#DCEBFF"
  light-success: "#1F9D44"
  light-warning: "#B36B00"
  light-adjust-handle: "#B36B00"
  light-danger: "#D70015"
  player-ground: "#0E0E10"
  player-strip: "#1B1B1B"
  player-drawer: "#2F2F33"
  player-card: "#232326"
typography:
  display:
    fontFamily: "Inter"
    fontSize: "28pt"
    fontWeight: 600
  heading:
    fontFamily: "Inter"
    fontSize: "20pt"
    fontWeight: 600
  title:
    fontFamily: "Inter"
    fontSize: "15pt"
    fontWeight: 600
  body:
    fontFamily: "Inter"
    fontSize: "13pt"
    fontWeight: 400
  body-strong:
    fontFamily: "Inter"
    fontSize: "13pt"
    fontWeight: 600
  caption:
    fontFamily: "Inter"
    fontSize: "11pt"
    fontWeight: 600
  small:
    fontFamily: "Inter"
    fontSize: "11pt"
    fontWeight: 400
  mono:
    fontFamily: "egui monospace"
    fontSize: "12pt"
    fontWeight: 400
rounded:
  sm: "4px"
  md: "6px"
  lg: "8px"
  xl: "12px"
spacing:
  xs: "2px"
  sm: "4px"
  s6: "6px"
  md: "8px"
  lg: "12px"
  xl: "16px"
  s20: "20px"
  s24: "24px"
  s32: "32px"
  s48: "48px"
components:
  button-primary:
    backgroundColor: "{colors.accent}"
    textColor: "#FFFFFF"
    rounded: "{rounded.md}"
    height: "32px"
  button-primary-hover:
    backgroundColor: "{colors.accent-hover}"
  button-quiet:
    textColor: "{colors.accent-text}"
    rounded: "{rounded.md}"
    height: "26px"
  button-quiet-hover:
    backgroundColor: "{colors.hover}"
  icon-button:
    textColor: "{colors.text-secondary}"
    rounded: "{rounded.md}"
    size: "26px"
  icon-button-hover:
    backgroundColor: "{colors.hover}"
    textColor: "{colors.text}"
  toolbar-button:
    textColor: "{colors.text-secondary}"
    rounded: "{rounded.lg}"
    height: "44px"
  toolbar-button-active:
    backgroundColor: "{colors.accent-subtle}"
    textColor: "{colors.accent-text}"
  input:
    backgroundColor: "{colors.input}"
    textColor: "{colors.text}"
    rounded: "{rounded.md}"
    height: "26px"
  segmented-track:
    backgroundColor: "{colors.input}"
    rounded: "{rounded.md}"
  segmented-selected:
    backgroundColor: "{colors.pressed}"
    textColor: "{colors.text}"
  layer-pill:
    textColor: "{colors.text-secondary}"
    rounded: "12px"
    height: "24px"
  layer-pill-selected:
    backgroundColor: "{colors.accent-subtle}"
    textColor: "{colors.accent-text}"
  popover:
    backgroundColor: "{colors.raised}"
    rounded: "{rounded.lg}"
  dialog:
    backgroundColor: "{colors.raised}"
    rounded: "{rounded.xl}"
  player-strip:
    backgroundColor: "{colors.player-strip}"
    textColor: "#FFFFFF"
    rounded: "{rounded.xl}"
    height: "44px"
  player-card:
    backgroundColor: "{colors.player-card}"
    textColor: "{colors.text}"
    rounded: "{rounded.xl}"
    padding: "24px"
---

# Design System: CourseCraft

## Overview

**Creative North Star: "The Quiet Studio"**

CourseCraft is a working studio for course authors, and the learner player is the gallery wall the finished work hangs on. The user chose to play the category standard straight, at Keynote's level of craft. Quality comes from precision, hierarchy, spacing and finish, not from novelty. The editor is dense enough for professionals but labelled and forgiving for newcomers. The player steps back so the course is the hero.

Everything is immediate-mode egui 0.37, drawn from one set of tokens in `crates/render/src/design.rs` (the Rust source is normative; this file mirrors it). Dark is the default theme. Light is one click away (View ▸ Theme), and so is following the system. The course's own theme colours style course content only, never the editor chrome. The player chrome uses neutral tokens plus the course accent.

**Key characteristics:**
- Refined neutral greys with a single blue accent.
- Hairline separators (1 px) instead of boxes and gaps.
- Inter everywhere, sentence case, no trailing colons on labels.
- Labelled icons (Phosphor, MIT) in the toolbar; icon-only buttons always have a tooltip with the shortcut.
- The learner player is minimal: a floating strip of transport controls that hides when idle.

## Colors

One cool blue accent on layered charcoal (dark) or soft grey (light) surfaces; status colours are reserved for status.

### Primary
- **Signal Blue** (#0A6CFF dark / #0066E0 light): primary buttons, selection handles and rings, the active toolbar button. White text on it meets 4.5:1.
- **Adjust Yellow** (#FFD60A dark / #B36B00 light): the diamond handles that resize a shape's adjustment, drawn above the blue resize handles.
- **Link Blue** (#4D9DFF dark / #0066E0 light): the accent used as text or icons on panels, such as quiet buttons ("+ Add layer") and the selected slide number.
- **Selection Wash** (#1C3556 dark / #DCEBFF light): selected rows, layer pills and toolbar toggles, and text selection.

### Neutral
- **App ground** (#1C1C1E / #ECECEE): the window ground, the welcome screen.
- **Canvas** (#161618 / #E3E3E6): the backdrop behind the slide; the darkest surface, so the slide pops.
- **Panel** (#252528 / #F6F6F7): toolbar, navigator, inspector, timeline and status bar.
- **Raised** (#2F2F33 / #FFFFFF): menus, popovers, dialogs and cards.
- **Input** (#1E1E21 / #FFFFFF): text fields, drag values and combos.
- **Hover / Pressed** (#323236, #3A3A3F / #E9E9EC, #DEDEE2): interaction fills on panels.
- **Border / Border strong** (#36363A, #48484E / #E1E1E4, #C9C9CE): hairlines; input outlines and hover outlines.
- **Text, secondary, tertiary, disabled** (#F2F2F5, #A8A8B0, #8E8E96, #55555B / #1D1D1F, #5E5E66, #6E6E75, #B4B4BA): primary text; labels and section headers; hints and timecodes; disabled text.
- **Player ground** (#0E0E10): behind the letterboxed slide in the learner player.

### Status
- **Success / Warning / Danger** (#30D158, #FFD60A, #FF453A dark; #1F9D44, #B36B00, #D70015 light): the validation chip dot, overlap warnings, and destructive actions on hover.

### Named Rules
**The One Accent Rule.** Blue is the only chromatic colour in the chrome. Status colours appear only to report status, and danger appears only on hover over a destructive action.

**The Content Is Not Chrome Rule.** The course's theme (accent, background, text, font) never tints editor surfaces. In the player, only the strip's Next button, the seek bar and the drawer highlights take the course accent.

**The Contrast Rule.** Text and secondary text meet WCAG AA (4.5:1) on panel, raised and app in both themes, and tertiary text does on panel. This is unit-tested in `design.rs`.

## Typography

**UI font:** Inter (bundled, OFL; regular, medium, semibold, bold, italic and bold italic). Chrome uses regular, medium for buttons and semibold for emphasis; bold is kept for slide text.
**Mono font:** egui's default monospace, for ids and timecodes
**Icons:** Phosphor regular and fill (bundled subset, MIT)

**Character:** Neutral and highly legible at small sizes; weight does the work of hierarchy, not size jumps.

### Hierarchy
- **Display** (semibold, 28 pt): the welcome screen wordmark.
- **Heading** (semibold, 20 pt): dialog titles, empty-state titles, "Course complete".
- **Title** (semibold, 15 pt): panel titles such as the inspector header (Format, Animate, Slide, Course).
- **Body** (regular, 13 pt; medium on buttons): default UI text, buttons, fields.
- **Body strong** (semibold, 13 pt): emphasised labels, the player strip's slide title.
- **Caption** (semibold, 11 pt, secondary): section headers inside panels, in sentence case (never uppercase).
- **Small** (regular, 11 pt): hints, metadata, thumbnail numbers, toolbar labels.
- **Mono** (12 pt): object ids, timecodes.

### Named Rules
**The No Colon Rule.** Form labels never end in a colon ("Name", not "Name:"). The label column carries the relationship.

**The Sentence Case Rule.** Headers, buttons and menu items use sentence case: "Add layer", "Course complete".

## Layout

The editor is a Keynote-style frame:
- a 28 px **menu row**;
- a 52 px **toolbar** (labelled icons; icons only below about 1100 px wide):
  - left: Add Slide and Preview;
  - centre: Text, Shape, Image, Media, Button, Question and Trigger;
  - right: Undo, Redo, and the Format · Animate · Slide · Course inspector switches;
- a **slide navigator** on the left (220 px, resizable 180–320);
- the **canvas** in the centre, with layer pills above the slide and a zoom control at the bottom right;
- the **inspector** on the right (300 px, resizable 260–400);
- a resizable **bottom panel**: Timeline, Triggers, Variables and Question on the left, with a 260 px **Slide Layers** panel on the right (200 px below about 900 px of panel width), separated by a hairline; hidden in Master View;
- a 26 px **status bar**.

Spacing runs on a 4 pt base: 2, 4, 6, 8, 12, 16, 20, 24, 32, 48. Panels use a 12 px inner margin, applied horizontally to the menu row, bottom panel and status bar too, so nothing touches the window edge. Inspector rows put a fixed 92 px label column on the left, with the control filling the rest. Controls are 26 px compact. Paired numeric fields (X/Y, W/H) share one row as two halves. Sections are separated by a hairline with 16 px above and below.

The learner player puts the slide letterboxed at the course aspect ratio, using the full window, on the player ground. The strip floats 12 px above the bottom, centred and fitted to its content. On a phone in portrait the slide sits in the middle with bands above and below. The strip fits from 320 pt up, dropping `n / N`, then the volume slider, the time label and Replay as width falls. Play, the slide seek bar and Next stay the longest.

## Elevation & Depth

The system is mostly flat with tonal layering: app ground, then canvas, panel, raised, from darker to lighter in dark mode. Shadows mark things that float above the work.

### Shadow Vocabulary
- **Popover** (offset 0 6, blur 20, black 40% dark / 16% light): menus, tooltips, coach marks, the zoom control, the "⋯" thumbnail button, the player strip.
- **Dialog** (offset 0 12, blur 32, the same colours): dialogs, the welcome card, the player's "Course complete" card.
- **Slide** (offset 0 8, blur 24, black 50% dark / 20% light): the slide on the canvas, plus a 1 px border outline.

### Named Rules
**The Float Means Shadow Rule.** Only floating surfaces get shadows. Panels never do; they are separated by hairlines.

## Shapes

Radii form a four-step scale: 4 (checkboxes, swatches, keycap chips), 6 (buttons, inputs, thumbnails, timeline bars), 8 (popovers, menus, toolbar buttons, cards) and 12 (dialogs, the welcome card, the player strip, the player card). Pills, such as layer pills, use half their height. Selection is a 2 px accent ring drawn outside the shape. Focus is a 2 px accent ring.

Course shapes are one fill with no outline, drawn from their own contours. Detail is measured from the frame's smaller side, so arrowheads and corner cuts keep their proportions when a shape is stretched. The Shape gallery is 340 px wide and at most 420 px tall: a search field, then each category's small header and a wrapped grid of 32 px icon buttons. Icons are `text_secondary` at rest, use the hover fill or selection wash behind them, and show the shape's label as a tooltip. The toolbar and Inspector open the same gallery; the Insert menu opens it as a small floating window so its search field can take typing.

## Components

- **Buttons:**
  - *Primary* buttons are filled accent with white text, 32 px, one per view.
  - *Quiet* buttons have no fill at rest, a hover fill on hover, and accent-text or secondary text ("+ Add trigger", "Delete layer"). Destructive quiet buttons turn danger only on hover.
- **Icon buttons:** 26×26, no fill at rest, an icon in secondary turning primary on hover, always with a tooltip. Toggles show the fill weight in accent on the selection wash when active.
- **Toolbar buttons:** an 18 px icon over an 11 pt label, about 52×44, quiet until hovered; the active inspector switch uses the selection wash.
- **Inputs:** input fill with a 1 px strong-border outline, radius 6, 26 px. Colour fields are an 18×18 swatch plus a hex input on one row.
- **Navigator thumbnails:** 16:9, radius 6, 1 px border. The number sits in a 22 px gutter. Selected shows a 2 px accent ring, with the number in accent bold. Hover reveals a "⋯" menu, which is also on right-click.
- **Timeline:** 28 px rows with hairlines, 20 px bars at radius 6 (accent with trim grips when selected, a flat pressed fill otherwise), names in the left column only with timing in the tooltip, and a seconds ruler.
- **Slide Layers:** 30 px rows listed front to back, with the Base Layer pinned last below a hairline. Each layer row has an eye, its name and a gear; selecting a row uses the selection wash. The panel footer holds `+ New layer`, duplicate and delete icon buttons, and the Dim checkbox. Layer Settings opens from the gear in a 360 px centred window.
- **Motion:** quiet controls fade between rest, hover and active in 0.1 s, and pressing is instant. The bottom tabs' selected fill glides to the new tab in 0.14 s.
- **Empty states:** a one-line explanation in secondary text plus a single action, quiet when a primary is already in view (the toolbar's Publish); never a blank area, and never two copies of the same action.
- **Welcome screen:** a 720 px raised card holding the wordmark, a 2×2 grid of action tiles (New, Open, Import PowerPoint, Explore an example) and a recent-courses list.
- **Coach marks:** a raised popover with a caret toward its target, a "1 of 3" counter, a Next button and a Skip button; no dimming.
- **Player strip:** the course bar colour at about 90%, height 44, radius 12. Left to right it holds Play or Pause, Replay, the slide seek bar with its `0:03 / 0:10` time label, the speaker button and a 72 px volume slider, captions when the slide has any, the outline button with `n / N`, and Previous and Next (accent-filled when the slide is complete). It fades after 2.5 s idle, but stays visible while paused.
- **Slide seek bar:** a 4 px track on the strip, with the elapsed part in the course accent and a 12 px handle that grows to 16 px on hover. It belongs to the current slide, takes the free width, and is dimmed when seeking is not allowed or the slide has no duration. The time label uses tabular figures so it does not jitter.
- **Volume:** a speaker button that mutes and restores the last level, beside a small slider. Volume is a session preference, default 100%, and reaches media that starts later.
- **Outline drawer:** 320 px from the left, a scrim of black at 35%, the course title and a close button in its header, ticks on visited slides, the current slide highlighted.

## Do's and Don'ts

- **Do** take every colour, size and radius from `design.rs` (`palette(ctx)`, the `SIZE_*`, `RADIUS_*` and `CONTROL_*` constants).
- **Do** give every icon-only control a tooltip with its shortcut.
- **Do** check new screens with `coursecraft-cli snapshot-editor` / `snapshot-player` in both themes, and the player at phone, tablet and desktop sizes.
- **Don't** use emoji or symbol characters as button labels; the bundled fonts don't have them. Use Phosphor icons.
- **Don't** let the course theme tint editor chrome.
- **Don't** add uppercase headers, trailing colons, or a second primary action in one view.
- **Don't** add shadows to panels, or outlines to widgets at rest (inputs excepted).
