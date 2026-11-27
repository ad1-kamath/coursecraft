# Neon review 0.6.1: editor UI verification (N84 to N109, N126)

Verification and fixing were done together. Each bug was reproduced by reading the code and by a harness test, then fixed with a test.

| # | Verdict | Evidence | Fix commit |
|---|---|---|---|
| N84 | Confirmed | Format and Text inspector edits wrote to the base object in Edit-State Mode | 4d63176 |
| N85 | Confirmed | Ctrl+B/I/U in a state inline edit styled the base text; runs did not sync | 9577ea5 |
| N86 | Confirmed | `replace_course` kept revision-keyed caches and armed state | 2a0bfa7 |
| N87 | Confirmed | Outline import stayed behind the Start and File page | 06d0f0a |
| N88 | Confirmed | With docking off, bottom panels opened blank windows | d5e2ff9 |
| N89 | Partly | Edit keys reached the canvas behind windows and the Start page; the Agent dialog has no modal flag, not changed | a07b718 |
| N90 | Confirmed | Timer Ends shared the action timer field | c909629 |
| N91 | Confirmed | Format Painter skipped wrong objects and ran in masters and states | 9c675f8 |
| N92 | Confirmed | Size and Position wrote shared fields in a state | bf8d30b |
| N93 | Confirmed | Insert paths ignored the selected slide layer | b680387 |
| N94 | Confirmed | Button Fill, Border and launcher were mis-wired | 50ab61b |
| N95 | Confirmed | Table, narration and Arrange items were enabled where they cannot work | 57b993f |
| N96 | Confirmed | Emphasis draft values were lost during a drag | 51bb906 |
| N97 | Confirmed | Ribbon Duplicate copied only one object | 10f316d |
| N98 | Confirmed | Reveal requests did nothing on a closed or covered tab | d5e2ff9 |
| N99 | Confirmed | Font, size, colour and level did not show mixed values | aae34d9 |
| N100 | Confirmed | Numbering Start at changed text without an edit | 315c742 |
| N101 | Confirmed | Dependent trigger pickers kept stale values | 6e3d4a9 |
| N102 | Confirmed | Add item accepted one object; Fill-in showed shuffle | 969dae9 |
| N103 | Confirmed | Theme swatch and drafts out of sync | 0e59270 |
| N104 | Partly | Multi-selection rotation fixed; a mixed-rotation edge case in states is not covered | bf8d30b |
| N105 | Confirmed | F2 on a bank slide started a rename nothing can finish | 8b75416 |
| N106 | Confirmed | Recent file names went through `tl!` | d06cb3d |
| N107 | Confirmed | A plain ruler click or off-slide drop added a guide | 703116e |
| N108 | Confirmed | Layer eyes cleared only when the Layers panel was drawn; the test is a weaker guard because the Timeline also draws that panel | eccbfe4 |
| N109 | Partly | Inspector row labels, spacing preset, painter hint, Questions menu and symbol tooltips fixed; some `properties.rs` labels outside `inspector_row` stay untranslated | 0e59270, aae34d9 |
| N126 | Confirmed | `docs/ui` disagreed with `spec.rs` and `contextual.rs` | c25dda9 |
