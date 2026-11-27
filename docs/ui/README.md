# The ribbon editor

> **Status:** in progress (M38). **Written:** 2026-11-23. Layout and look follow `DESIGN.md`
> (see [Clean-room](#clean-room)).

CourseCraft's editor organizes authoring commands in a **tabbed ribbon** grouped by task (File,
Home, Insert, Slides, Design, Transitions, Animations, View, AI, Help, plus contextual tabs for
the active selection) paired with a dockable panel workspace.

**The rule: build every planned menu now, wire what exists, disable the rest.** A planned button
whose feature is not built yet is still drawn, in its place, greyed out, with a tooltip that says
why ("Not available yet: camera zoom"). It never disappears and it never fakes a result. When the
feature lands, the button is switched on; nothing moves. Controls marked **Not planned** (`🚫`)
are kept in the spec for possible future use and hidden from the UI.

## Files

| File | What it holds |
|---|---|
| [feature-parity.md](feature-parity.md) | One row per feature: CourseCraft status and the milestone (M38–M43) that closes it. The summary the roadmap follows. |
| [ribbon.md](ribbon.md) | CourseCraft's main tabs: every group and control, its CourseCraft label, kind, command id and status. |
| [contextual-tabs.md](contextual-tabs.md) | Tabs that appear for a selection or a mode: Drawing Tools, Picture Tools, Audio/Video Tools, Question Tools, Table Tools, Slide Master, Feedback Layouts, Preview, question form view. |
| [shell-panels-dialogs.md](shell-panels-dialogs.md) | Window layout around the ribbon: title bar, quick access toolbar, view tabs, panels (Scenes, Timeline/States/Notes, Triggers, Slide Layers, Slide Properties), status bar, context menus and dialogs. |
| [panels.md](panels.md) | Dock panels by category and the settings panel each ↘ launcher opens: the long-term plan for advanced settings and customization. |
| [implementation.md](implementation.md) | How to build it in `coursecraft-ui-egui` on `craft-ui` (craft-libs): a declarative ribbon spec, the ribbon widgets to port from DeckCraft, the egui fork, enable rules, tests and the order of work. |
| [localization.md](localization.md) | Localizing the interface with `craft-i18n`: wiring, rules for strings, offered languages, language setting, tests, getting translations. |

## Status legend

Used in every table in this folder.

| Mark | Meaning | Drawn as |
|---|---|---|
| ✅ | Built: dispatches an existing engine command or UI action | enabled (subject to the selection) |
| 🟡 | Partly built: the button works, some options of its menu are disabled | enabled; missing menu items disabled |
| ⛔ | Not built yet, but planned or plausible | **disabled**, tooltip `Not available yet: <feature>` |
| 🚫 | Not planned (see `docs/roadmap.md` › Not planned for now) | hidden (kept in spec for possible future use, not drawn) |
| — | Out of scope (hosted/cloud services) | not drawn |

A status is a claim about the code on the date above. Whoever wires a control checks it against the
code first and updates the table in the same commit.

## Clean-room

Read `docs/contributing.md` › Clean-room rules first. For the editor UI it means:

- **CourseCraft's own layout and look.** The ribbon groups commands by task; icons, colours,
  spacing and fonts come from `DESIGN.md`. No icon, screenshot or asset from any closed-source or
  commercial tool goes into the repository.
- **Generic and CourseCraft labels.** Standard authoring terms (Home, Insert, Design, Animations,
  Trigger, Layer, State, Question Bank, Slide Master, Text Styles, Format Painter, Focus Order,
  Scrolling Panel, Preserve) are used alongside CourseCraft's own labels: **Course Map**, **Custom
  Question**, **On Click**, **Feedback Layouts**, **Camera Zoom**, **Selection Group**, **Start
  Section**, **Library**, **Share for Review**, **Trigger**, **AI**, and **CourseCraft**.

## How to add or wire a button

1. Find the control in [ribbon.md](ribbon.md) or [contextual-tabs.md](contextual-tabs.md).
2. If the behaviour is new, add the engine command first (`docs/contributing.md` › add-a-command
   checklist). UIs only dispatch (AGENTS.md rule 2).
3. In the ribbon spec (see [implementation.md](implementation.md)), change the control's action from
   `NotYet(..)` to `Command("<id>")` or `Ui(<action>)`.
4. Write its label and tooltip in English through `tl!` / `tid` (localization.md).
5. Change its status in the table to ✅ or 🟡, and the feature's row and summary counts in
   [feature-parity.md](feature-parity.md).
6. If the control changes play-time behaviour, update `docs/spec/triggers.md` in the same commit.
7. Run `cargo xtask ci`.

## Split buttons

- **Large split buttons** use a two-part top/bottom layout: the top half (icon) runs the main action, and the bottom half (label and ▾) opens the menu. Hovering either half highlights only that half and draws a thin divider line between them.
- **Small and toggle split buttons** use a ▾ menu strip of at least 18 px on the right with separate hover highlight and a thin vertical divider line between the main part and the ▾ strip.
- Pressing `Alt+Down` while a split button is focused opens its menu.
