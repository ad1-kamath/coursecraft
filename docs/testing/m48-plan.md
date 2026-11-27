# M48: Editor feedback from 0.6.5 (split buttons, slide tabs, layer bar, no-fill effects)

Goal: fix four problems the owner found while using 0.6.5. Ship as 0.6.6.

Rules for every step: AGENTS.md golden rules (never crash, everything is a command, one runtime, layering, wasm stays working, clean-room wording, `tl!` for every user-facing string). Editor UI changes follow `docs/ui/` and update its status tables (`ribbon.md` or `contextual-tabs.md`, and `feature-parity.md` with its summary counts) in the same step. A rendering behaviour change updates `docs/spec/course-model.md` in the same step. Add a `CHANGELOG.md` line for each user-visible change under a `## Unreleased` heading above `## 0.6.5` (create it if missing). Use plain words, no internal ids. Every step adds headless UI tests (egui_kittest, like the existing `*_tests.rs` files in `crates/ui-egui/src`).

## Steps

1. **Split buttons: the menu part is easy to hit (GitHub issue #17).**
   Today a large split button (Preview ▾ and the other 32 `Kind::LargeSplit` items in `crates/ui-egui/src/ribbon/spec.rs`) opens its menu only from a 14 px wide strip at the bottom-right corner (`SPLIT_ARROW_W` in `crates/ui-egui/src/ribbon/widgets.rs`, `fn large`, the `arrow_rect` built at about line 138). Clicking anywhere else, including most of the label and right next to the ▾, runs the main action. Users keep starting Preview when they wanted the menu.
   - Use the standard two-part layout for a large split button: the **top part** (the icon area, from the top of the button down to the line above the label) runs the main action; the **bottom part** (the full width of the label rows plus the ▾, down to the bottom of the button) opens the menu. Put the ▾ centred under or after the label, inside the bottom part.
   - On hover, highlight the two parts separately so the user sees which part a click hits: hovering the top fills only the top; hovering the bottom fills only the bottom. Draw a thin divider line between the two parts while the button is hovered.
   - Small split buttons (`fn small`): widen the ▾ hit area from 12 px to at least 18 px, and highlight it separately on hover in the same way. Toggle split buttons (`toggle_split_bar`) get the same wider ▾ area.
   - Keyboard and accessibility: keep the two `WidgetInfo` entries ("Preview" and "Preview options"). Alt+Down (or the platform's usual key) on a focused split button opens its menu, if focus is supported for ribbon buttons today; if not, leave a note in the report.
   - Keep all labels, icons, ids and actions the same. Do not change any button that is not a split button.
   - Tests: a click in the bottom half of Preview opens the Preview menu; a click on the icon runs Preview; the same for one other large split button and one small split button; the hover fill covers only the hovered part.
   - Update `docs/ui/README.md` (or the ribbon section that describes split buttons) with the two-part rule.

2. **Selecting slides does not pile up tabs.**
   Today `doc_tabs::sync` (`crates/ui-egui/src/doc_tabs.rs`) adds a permanent tab for every slide that becomes the current one. Clicking slides in the slide list, or stepping with the Up and Down arrow keys, opens one new tab per slide. On a 300-slide course this floods the tab row.
   - Use a **preview tab**: there is at most one preview tab. Selecting a slide that has no tab (slide list click, arrow keys, Find, Course Map single click, the Next and Previous commands, a new or duplicated slide shown after the command) shows it in the preview tab, replacing the slide that was in it. The preview tab's label is drawn in italics so it reads as temporary.
   - The preview tab becomes a normal (pinned) tab when the user: double-clicks the slide in the slide list or the Course Map; double-clicks the preview tab itself; or makes any edit to that slide (any command that changes the course while that slide is shown, including selecting and moving an object, typing text, adding a layer). Edits made through undo or redo do not pin.
   - A slide that already has a normal tab is just switched to; no preview tab is used.
   - The preview tab sits after the normal tabs. Closing it with × works like closing any tab.
   - Also cap the tab row: when the normal tabs no longer fit, the row scrolls sideways (mouse wheel and a small ‹ › pair at the ends) and the active tab is always scrolled into view. Do not shrink tabs below a readable width. Check how the row behaves today first; if it already scrolls, keep that.
   - Keep `DocTabs` serialisable state backwards compatible if it is saved anywhere (check); a new field defaults to "no preview tab".
   - Tests: clicking ten slides in the slide list leaves one preview tab; arrow keys through ten slides leave one preview tab; double-click pins; an edit pins; undo does not pin; a slide that already has a tab does not create a preview tab; closing the preview tab shows the neighbour as today; deleting the previewed slide removes the tab.
   - Update `docs/ui/` where document tabs are described.

3. **The layer bar above the slide is never hidden behind the slide.**
   Today the layer pill bar (Base, the layer names, +, and "Base slide: Show / Dim / Hide") is drawn by the function around `crates/ui-egui/src/canvas.rs` line 1980–2100. Its top is placed 32 px above the slide (`pills_y = slide_rect.min.y - 24 - 8`). When a layer is selected (or the pills don't fit in one row) the bar wraps to 64 px (`pills_h`), so the second row is drawn over the slide area and the slide then paints over it. The owner's screenshot shows "Base slide: Show / Dim / Hide" and the layer names cut off behind the slide; a normal user can't tell what they do.
   - The whole bar must be fully visible and clickable at every zoom level, window size and number of layers. Options, pick the best and say why in the review: (a) reserve the bar's real height above the slide when the canvas lays out the slide, so the slide moves down when the bar wraps; (b) keep the bar to one row, putting "Base slide: Show / Dim / Hide" as a compact segmented control at the right end and letting the layer pills scroll sideways when they don't fit; (c) draw the bar in a foreground layer with a solid background strip. Prefer (a) or (b); (c) alone is not enough because it would cover the top of the slide.
   - The bar must not cover the rulers when rulers are shown, and must not jump while the user zooms or scrolls.
   - Check the same thing in the master view and the feedback-layer case shown in the screenshots (a question slide with Correct and Incorrect layers, "Dim" selected).
   - Tests: with three layers and a layer selected, every pill's rect (Base, each layer, +, Show, Dim, Hide) lies outside the slide rect and inside the canvas clip rect, at zoom 50%, 100% and 200%, and at a narrow window width where the pills wrap today; clicking "Hide" works.

4. **Turning Fill off: shadow and glow follow what is still visible.**
   The owner asks: should unticking Fill also turn off the shape's shadow and glow? Today the shadow stays (glow untested). Decision for this milestone, matching how common presentation tools behave:
   - The shadow and glow are cast by what the shape actually draws. With Fill off and the outline on, the shadow and glow follow the **outline only** (and the shape's text, if the text already casts its own shadow today, keep that as is). With Fill off **and** the outline off, the shape draws no shadow and no glow.
   - Do **not** clear or change the shadow and glow settings in the model when Fill is turned off: turning Fill back on brings the effect back exactly as it was. No new model field.
   - In the inspector, when the shape has no fill and no outline, show the Shadow and Glow sections greyed (still editable is fine) with a one-line note: "Add a fill or outline to see the shadow." (and the glow equivalent), via `tl!`.
   - The editor canvas, the player (`crates/render`, `compose.rs` resolves shadow and glow around lines 470–890; the painter in `paint.rs`) and the web export must agree. Find where a shape's shadow is drawn as a filled silhouette and make it use the outline path (stroked with the outline width) when the fill is off. Do the same for glow. Check images and other kinds are not affected: this is about shapes (and buttons/text boxes with a fill toggle, if they have one; say which kinds you changed).
   - Tests: render tests (pixel probes) for a rectangle with shadow: fill on → shadow under the whole shape; fill off, outline on → shadow pixels only near the outline, the inside of the shape shows the slide background; fill off, outline off → no shadow pixels; same three cases for glow; turning Fill back on restores the shadow without other changes.
   - Update `docs/spec/course-model.md` (shape shadow and glow: they follow the visible fill and outline).

5. **Gate, player build, perf and report** (driver steps).

## Owner's manual checks

- Hover Preview in the ribbon: the top and the bottom light up separately; clicking the label opens the menu.
- Click through 20 slides in the slide list and with the arrow keys: one italic tab follows you; double-click a slide to keep its tab.
- Select a layer on a question slide: Base, the layer names, + and Show / Dim / Hide are all fully visible.
- Untick Fill on a shape with a shadow: the shadow follows the outline; remove the outline too and the shadow disappears; tick Fill again and the shadow returns.

## Reviewer notes

- Step 1 must not change what any non-split button does.
- Step 2: performance matters. Nothing may scan all slides per frame; the 300-slide case must stay smooth.
- Step 3: a fix that only moves the bar higher without accounting for its real height is not a fix.
- Step 4: settings are never lost. A course saved by 0.6.5 must look the same where Fill is on.
