# Tester feedback, round 2

Each step ends with `cargo xtask ci` passing.

| Step | What |
|---|---|
| 1 | Fuller right-click menus: cut, copy, paste, arrange and align, on the canvas, timeline, slides and scenes |
| 2 | Slide Layers panel: base at the bottom, Settings button, reorder, duplicate, eye and dim |
| 3 | Shape system: every kind is geometry, concave fills, a gallery, PowerPoint import mapping |
| 4 | The full shape catalog: PowerPoint's gallery plus a Learning set |
| 5 | ElevenLabs narration |

## Decisions

- **Right-click acts on what's under the pointer.** An unselected object becomes the selection first. A selected one keeps the multi-selection.
- **Menu Paste uses the private clipboard,** as the Edit menu does, so it works on the web. It's disabled when that clipboard is empty.
- **Layers stack in authored order.** `slide.layers` runs back to front and the runtime keeps visible layers in that order, not the order they were shown. The panel lists front to back, with the base pinned at the bottom, like the timeline's rows.
- **Layer settings live in a modal window** opened by a gear button. OK sends one `layer.update`, one undo step. Cancel discards.
- **The eye and Dim are editor view state,** never course data, and don't affect play.
- **Not in this round:** new layer runtime options (hide base objects, revisit behaviour, seeking). Round 3 adds them: see `feedback-3.md`.
- **Shape geometry is computed for the frame's size,** so an arrowhead keeps its proportions when stretched. Shapes still hit-test by their frame.
- **New shape kinds are additive,** so `schema_version` stays 1, but an older build can't open a course that uses one.
- **Shapes have one fill, no outline, no text and no adjust handles.** Detail is cut out as gaps. Connectors, curves and freeform drawing are not included. Round 3 adds these: see `feedback-3.md`.
- **ElevenLabs has its own settings,** separate from the AI provider, so people can keep another provider for chat. Only the key, voice and model are stored, in prefs, never in a course.
