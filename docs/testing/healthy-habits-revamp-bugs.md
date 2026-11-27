# Bugs found while rebuilding Healthy Habits at Work (2026-11-27)

Found while building and testing the redesigned sample course with `render`, `snapshot-player`,
`simulate`, `a11y-check` and the full gate. None of them blocks the course; each has a
workaround in `examples/healthy-habits/build.py`. Ordered by how much an author would notice.

## 1. Text that plays in by letter never substitutes `%variables%`

- **Seen:** the certificate name, set to `%learner_name%` with a by-letter entrance, shows the raw
  `%learner_name%` in the player. The same object with no sequence, or by paragraph, shows the
  name (`friend` when none is set).
- **Cause:** `crates/render/src/compose.rs` (around line 3086) substitutes variables run by run
  *after* `sequence::apply_object_samples` has split the text into one run per letter, so no run
  holds a whole `%name%` token. By-word splitting would hit the same thing for a token next to
  another word.
- **Fix idea:** substitute before splitting (in `compose_object_items_opts`, or before
  `apply_object_samples`), and add a test with a by-letter `%var%` text.
- **Workaround used:** the certificate name plays in as one piece.

## 2. A shape-masked image ignores its crop and squashes the picture

- **Seen:** an image with `"shape": "ellipse"` and a crop draws the whole 16:9 source squeezed
  into the circle; the same crop with no mask draws correctly.
- **Cause:** `crates/render/src/paint.rs` (around line 3034): with a mask, the image goes
  through `paint_picture_fill`, which takes no crop window; only the unmasked branch passes
  `crop` to `image_dest_rect`. Picture fills on shapes (`fill_style.picture`, mode `stretch`)
  have the same limit: they always stretch the whole picture to the shape.
- **Fix idea:** pass the crop window (UV rect) into `paint_picture_fill` for masked images, and
  give picture fills a `fill` (cover) mode that keeps the picture's ratio. Add a render test
  comparing a masked crop with the unmasked one.
- **Workaround used:** the masked pictures use square copies cut ahead of time
  (`*-round.jpg`) with no crop.

## 3. Question prompt and answers can't be moved or narrowed

- **Seen:** a picture placed beside a question is covered by the prompt and the answer rows.
- **Cause:** the player lays questions out on a fixed full-width grid
  (`question_prompt_rect` in `compose.rs`: 80 px margins, answer rows up to 900 px wide). The
  question object's frame is not used for this layout.
- **Fix idea:** lay the prompt and rows out inside the question object's frame, so authors can
  make room for a picture.
- **Workaround used:** question pictures are small corner cards at the bottom right.

## 4. No glyph and no fallback for U+2715 (✕)

- **Seen:** a `✕` in a button label draws as an empty box.
- **Fix idea:** add a symbol fallback font, or map common symbols (✕ ✓ ★ →) to a bundled face;
  `a11y-check` or `validate` could warn about characters no font can draw.
- **Workaround used:** `×` (U+00D7).

## 5. Image masks and reveal effects use different names for a circle

- **Seen:** an image `shape` mask of `"circle"` is rejected; it needs the shape name
  `"ellipse"`. The shape reveal effect calls the same thing `"circle"`.
- **Fix idea:** accept `circle` as an alias for `ellipse` in masks (and say so in the error).

## 6. `render --layer X` also shows layers the slide opens by itself

- **Seen:** `render --slide results1 --layer passed` also draws the failed layer, because the
  slide's own timeline-start trigger shows it at the same time.
- **Fix idea:** with `--layer`, show only the named layer (or add `--only-layer`). This is a
  CLI checking tool quirk, not something learners see.

## 7. `snapshot-player` "ended" view hides objects whose entrance starts late

- **Seen:** on long slides, a snapshot taken at the slide's end can leave out objects that
  start after a pause trigger, which makes the frame look broken when the player is fine.
- **Status:** not confirmed as a player bug; it may be how the snapshot advances time. Worth a
  look before trusting snapshots for layout checks.

## Outside the app

- Lyria refused the first music prompt as "recitation"; a reworded prompt worked.
- Gemini TTS on Tier 1 allows 10 requests a minute; the clip script retries after 20 s.
- `agy` print mode stopped after the fourth image in one session; one image per run works.
