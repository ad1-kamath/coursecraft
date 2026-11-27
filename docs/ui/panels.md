# Settings panels: what the ↘ launcher opens

> Part of [the ribbon plan](README.md). **Status:** step 1 built, steps 2 (Text) and 4 (bottom tabs) partly built.
> **Written:** 2026-11-23. Status marks as in README. Long-term plan: it starts in M38 and grows with every milestone after it.

The ↘ in a group's caption opens a **dock panel** rather than a modal dialog: the author can keep
it open, tab, split, float or close it. A panel is where a category's advanced settings live. The
ribbon keeps the common ones; the panel holds every setting of that category.

Why a panel and not a dialog:

- It stays open while the author works, so changing ten objects needs no reopening.
- It follows the selection, like the inspectors already do.
- It goes where the author wants it: tabbed with the inspectors, beside the canvas, on a second
  screen.
- It closes. Nothing is on screen that the author did not ask for.

## Before this plan (2026-11-23)

What there was when the plan was written. What is built since is under [Order of work](#order-of-work).

### Dock panels

`crates/ui-egui/src/dock.rs`, `Panel`:

| Panel | Holds | Closable |
|---|---|---|
| Slides | scene and slide thumbnails (`slides.rs`) | yes |
| Canvas | the slide being edited | no (protected) |
| Timeline | bottom tabs Timeline, States, Triggers, Variables, Question (`timeline.rs` `BottomTab`), and Slide Layers on its right (`layers.rs`) | yes |
| Format | inspector: Position & Size, Shape, Text, Image, Path, Effects, Frame, Rotate, Table (Rows, Columns, Cell, Borders), Draggable, Question, States, Accessibility (`properties.rs`) | yes |
| Animate | inspector: Auto-preview, Entrance, Emphasis and Exit cards (M46: effect picker with drawn icons, search, Recent and category headers; one row of icon buttons per option; Duration in seconds; Easing drawn as curves (seven, with Overshoot, Bounce and Elastic); Sequence for objects with text; ▶ Preview; Remove), Motion Paths, Transition | yes |
| Slide | inspector: Draw, Master, Question, Notes, background, advance and revisit | yes |
| Course | inspector: Title, Pass mark, Slides | yes |

Closing: craft-ui closes a docked panel from its tab's menu (Close Panel, Close Tab Group, Close
Other Panels in Group); a floating window draws a ×. View › Panels reopens any of them.

### Launchers

| Ribbon group | ↘ today |
|---|---|
| Home › Font | disabled: "the Font dialog" |
| Home › Paragraph | disabled: "the Paragraph dialog" |
| Home › Drawing | Shape Style panel at Fill ✅ |
| Design › Background | Slide inspector |
| Animations › Size and Position | Size and Position panel at Size ✅ (the button too) |
| Contextual: Shape Styles, Button Styles, Picture Styles, Size and Position (×3), Video Options, Audio Options, Background | listed in [contextual-tabs.md](contextual-tabs.md), not wired |

Every other group has no ↘.

## Categories

Every panel belongs to one of three families. View › Panels lists them in this order, one heading
per family. In the code: `crates/ui-egui/src/panels.rs`, `family()`.

| Family | What it is | Panels |
|---|---|---|
| **Workspace** | where the course is built | Slides, Canvas, Timeline, States, Triggers, Variables, Question, Layers, Notes |
| **Inspectors** | quick settings for the selection, as today | Format, Animate, Slide, Course |
| **Settings** | every setting of one category; opened by ↘ | the panels below |

The settings panels, by category. "Today" says where those settings live now. "Adds" is what the
panel holds that nothing holds today.

| Panel | Opened by ↘ in | Today | Adds | Milestone |
|---|---|---|---|---|
| **Text** (sections Font, Paragraph, Text Box) | Home › Font, Home › Paragraph, Drawing Tools text groups | Format › Text | character spacing, caps, super/subscript, highlight, underline style; indents, spacing before and after, exact line spacing, tab stops; autofit, margins, columns, direction, vertical alignment | M39 (caps, underline styles, tab stops, columns: later) |
| **Shape Style** (Fill, Outline, Effects) | Home › Drawing, Shape Styles, Button Styles, Picture Styles | Format › Shape, Effects | gradient stop editor, dash, caps and joins, every shadow, glow, reflection, soft edge and bevel parameter, save as style | M39, M40 |
| **Size and Position** | Size and Position (Drawing, Button, Picture tools), Animations › Size and Position | Format › Position & Size, Rotate, Frame | lock aspect ratio, scale %, reset to original size, numeric crop, anchor point, zoom picture | M39, M40 |
| **Media** (Picture, Video, Audio) | Video Options, Audio Options | Format › Image, Frame; Timeline › media timing | volume, fade in and out, trim, playback and controls, captions, compression | M40 |
| **Table** | Table Tools groups | Format › Rows, Columns, Cell, Borders | cell margins, merge and split, banded rows, border presets | M40 |
| **Animation** | Animations › Entrance, Exit, Emphasis, Motion, Advanced | Animate › Animations, Motion Paths | easing, sequence by paragraph, path orientation and relative start, timing in seconds | M42 |
| **Transition** | Transitions › Timing | Animate › Transition | per-layer transitions, direction options | M42 |
| **Slide Properties** | Design › Background, Design › Options, Slides › Slide | Slide inspector | background styles (gradient, picture), hide background graphics, swipe, player overrides | M41 |
| **Theme** | Design › Themes | `theme_dialog.rs` window | every colour slot, theme fonts, save and share a theme | M39, M41 |
| **Course and Player** | Design › Setup | Course inspector, `course_settings.rs` window | player features, menu, resources, glossary, text labels | M41 |
| **Triggers** | Insert › Interactive | Timeline › Triggers tab | the trigger form as a panel (Action, target, When, Object, Conditions) | M41 |
| **States** | Drawing Tools › States | Timeline › States tab | — (moves out) | M41 |
| **Variables** | Triggers › Variables | Timeline › Variables tab | — (moves out) | M41 |
| **Question** | Slides › Questions, Question Tools | Timeline › Question tab, Format › Question | feedback, scoring and attempts in one place | M41 |
| **Accessibility** | context menu Accessibility…, Home › Drawing | `alt_text.rs` window, Format › Accessibility | focus order, tab order, contrast check | M39 |
| **Grid and Guides** ✅ | View › Show | View toggles | grid spacing (`view.set` `{grid_spacing}`, saved with the course), show and snap for grid and guides, the guide list (move by number, remove, add at the slide's centre), show rulers; ruler units drawn disabled | M38 |
| **AI** | AI › Write, Media, Assess, Agents | `ai_settings.rs`, `connect_agent.rs` windows | chat with the connected agent, generation history, voices | M43 |

Layers and Notes move from the Timeline panel to Workspace panels of their own, as
[shell-panels-dialogs.md](shell-panels-dialogs.md) › step 6 already plans.

## Rules

1. **Every ↘ opens a panel.** In the ribbon spec a launcher is
   `Ui(UiAction::PanelAt(panel, section))` or `NotYet("the <name> panel")`. No launcher opens a
   modal dialog. A launcher whose panel is not built is drawn disabled with its reason.
2. **One panel, many launchers.** Font ↘ and Paragraph ↘ both open Text: one at Font, one at
   Paragraph. The panel scrolls to that section and flashes its header.
3. **↘ only reveals.** A closed panel reopens where it was last, or at its default place: tabbed
   with the inspectors. An open panel comes to the front. ↘ never closes.
4. **Every panel closes,** except Canvas. A closed panel comes back from its ↘, from View ›
   Panels, or from the context menu entry that names it. Docking off (Preferences › Interface):
   the panel opens as a floating window with a ×.
   Tab names stay whole: A tab is never narrower than the longest inspector name (Animate). When
   tabs do not fit, the rest go to the tab strip's … menu, which lists every name in full. A tab cut
   to "Ani…" is a bug (`inspector_tab_names_are_never_cut_to_three_letters`).
5. **One body of code per section.** A section (Font, Fill, Position…) is one function drawn in
   two modes: *quick* in an inspector, *full* in its panel. An inspector section header carries
   its own ↘ that opens the full panel at that section. No control is written twice.
6. **Panels follow the selection.** With nothing that the panel applies to, its controls are drawn
   disabled with the reason ("Select a shape to change its style"). Nothing is hidden.
7. **Missing features are drawn disabled,** as in the ribbon, and counted in
   [feature-parity.md](feature-parity.md).
8. **Every change is an engine command.** Panels dispatch; undo works as everywhere else.
9. **A saved layout survives new panels.** A panel the saved layout does not know opens at its
   default place; a name the build does not know is dropped. The layout never resets for that.
10. **The work opens its panel.** When the author starts work that needs a settings panel, that
    panel opens by itself and comes to the front, at the right section: editing text on the
    canvas opens Text at Font. Only the start of the work counts: a panel the author closes while
    working stays closed until the next start. Docked panels only: with docking off the fixed
    inspector already holds the controls, and a window opening by itself would cover the canvas
    being worked on (a launcher still floats it). Each new panel adds its trigger to
    `panels::wanted` (a table cell to Table, a motion path to Animation, a state to States).

## Customization (later)

The panels are the base for the customization testers ask for. In order:

1. **Sections:** collapse, expand and reorder sections in a panel; remembered per panel.
2. **Pin to inspector:** a full-panel section can be pinned into an inspector, so an author's
   inspector holds exactly what they use.
3. **Workspaces:** View › Workspace ▾ with presets (Build, Animate, Review, Compact) and Save
   Current Workspace. A workspace is a saved dock layout.
4. **Custom panels:** the author picks sections from any category into a panel of their own.
5. **Ribbon and QAT:** right-click any control › Add to Quick Access Toolbar, Show ↘ panel.

## Code

- `crates/ui-egui/src/panels/registry.rs`: one `PanelSpec` per panel: `Panel`, title, icon,
  family, default place, sections. `Panel::ALL`, View › Panels and the default layout come from
  it.
- `crates/ui-egui/src/panels/<category>.rs`: the sections of one category, each
  `fn show(editor, ui, mode: SectionMode)`. The inspectors in `properties.rs` call the same
  functions in `SectionMode::Quick`.
- `ribbon/spec.rs`: `UiAction::PanelAt(Panel, Section)`; launchers point there.
- Tests (`panels_tests.rs`):
  - every `Panel` has a spec, and every spec section exists;
  - every launcher in the spec opens a registered panel at one of its sections, or is `NotYet`;
  - ↘ on a closed panel opens it, on a shown panel keeps it shown;
  - every panel but Canvas closes and comes back from View › Panels;
  - a layout saved without a panel loads, and the panel opens at its default place;
  - a layout naming an unknown panel loads without it.

## Order of work

| Step | What | Closes |
|---|---|---|
| 1 ✅ | Registry (`panels.rs`: families, sections), `UiAction::PanelAt`, the tolerant layout loader (`dock::forget_unknown_panels`), View › Panels in family order, opening by itself (rule 10), floating windows with docking off, `panels_tests.rs`; every launcher moved to `PanelAt` | the infrastructure |
| 2 🟡 | **Text** panel (Font, Paragraph, Text Box sections); Font ↘ and Paragraph ↘ switched on; space before and after a paragraph (only in this panel); Text Box holds Autofit, vertical alignment, Wrap text (including point-text conversion), direction and insets. Shadow and highlight (M39). Still disabled: caps, underline styles, tab stops, columns. Still to do: the Format inspector's text rows in quick mode, a ↘ on its Text header | Font and Paragraph dialogs (M39) |
| 3 🟡 | **Size and Position** (✅ `size_panel.rs`: Size with aspect lock, Rotate with flips, Position from the top left or the centre, lock; one object for size and place, every selected object for rotation, flips and lock) and **Shape Style** (✅ Fill, Outline, Effects through the inspector's `shape_fill_rows`, `shape_outline_rows` and effect rows, state edits included); Drawing ↘ and Animations › Size and Position switched on. Scale % and Reset to original size for pictures (M39). Still to do: the contextual launchers (with the contextual tabs), button and picture fills | Size and Position dialog, Format Shape (M39) |
| 4 🟡 | **Triggers**, **States**, **Variables**, **Question** as dock panels of their own (✅: tabbed with Timeline by default; a saved layout gains them beside Timeline; `ui.bottom_tab` brings its panel forward; in Master View they say why they are empty). **Slide Layers** (✅, right of the Timeline's group) and **Notes** (✅, a tab of that group; the Slide inspector leaves the notes to it while it is open) out of the Timeline panel; a saved layout gains both. Still to do: Triggers, Variables and Question as settings panels with ↘ launchers | shell step 6 (M41) |
| 5 | **Animation** and **Transition** | M42 |
| 6 🟡 | **Grid and Guides** (✅, `grid_panel.rs`, View › Show ↘), **Slide Properties**, **Theme**, **Course and Player** | M41 |
| 7 | **Media**, **Table**, **Accessibility** | M40 |
| 8 | **AI** panel with agent chat | M43 |
| 9 | Customization 1–3 | after M43 |

Each step updates this file, [ribbon.md](ribbon.md) or [contextual-tabs.md](contextual-tabs.md),
and [feature-parity.md](feature-parity.md) in the same commit.
