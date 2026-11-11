# Tester feedback, round 3

The limits round 2 left open: Storyline's layer options, and shapes that can carry an outline and text, be adjusted, and be drawn freely. Each step ends with `cargo xtask ci` passing.

| Step | What |
|---|---|
| 1 | Layer options: hide base objects, hide when the timeline ends, seeking, revisit. Slide revisit |
| 2 | Shape outline, no fill, and text inside shapes |
| 3 | Adjust handles on the common shapes |
| 4 | Draw tools: line, arrow, connectors with free ends, curve, freeform, scribble, point editing, hit testing by path |
| 5 | Connectors glue to objects and follow them |

## Decisions

- **Layer options are play-time behaviour,** so they live in `coursecraft-logic` and `docs/spec/triggers.md` changes with them. Storyline specifics we can't check are marked `[verify]`.
- **"Automatically decide" keeps today's behaviour** for revisit and seeking, so existing courses play the same.
- **Hiding base objects doesn't stop the base.** Its clock, triggers and media carry on, unless `pause_base_timeline` is also on.
- **New fields have serde defaults** and are left out when default, so `schema_version` stays 1. Older builds can't open a course that uses them, which is fine: no older builds were shared.
- **`ShapeKind` stays `Copy`.** Point data lives on a separate `ObjectKind::Path`, not in the enum.
- **Text in a shape lives in the shape,** so it moves, translates, audits and imports with it. A PowerPoint autoshape's text goes into the shape, not a separate text box.
- **Paths hit-test by distance to the line,** or by area when filled, so a diagonal line doesn't steal clicks across its bounding box. Other shapes still hit-test by frame.
- **Handle drags are one undo step,** committed on release.
- **Connectors re-route in one place in the engine,** after each successful command, so every kind of move is covered by one rule.

## Not in this round

- **Rotation and flip.** They touch handles, resize, text, images, guides, focus and import, so they get their own step when wanted. The draw tools already give lines at any angle.
- **Gradient fills, shadows, multiple text columns.**
