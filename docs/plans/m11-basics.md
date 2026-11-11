# M11 The basics

Goal: what a Storyline or PowerPoint author expects in the first hour. Every step keeps old `course.json` files loading unchanged (new fields default and are omitted when default), goes through engine commands with undo, has an inspector control, and is drawn the same way on the canvas, in preview, in the published player and in the offscreen renderer (one code path in `coursecraft-render`). PowerPoint import picks up the same property where the file has it. Spell check is left for later.

## Step 1: Rotate and flip

- `Object.rotation` (degrees, clockwise, normalised to 0..360, omitted when 0), `flip_h`, `flip_v` (omitted when false). Audio objects ignore it (they have no visible body in the player); everything visible rotates, question widgets included.
- Render: paint the object as now, then rotate everything it drew about the frame's centre and mirror for flips. In epaint, tessellate the item's shapes to a `Mesh` and use `Mesh::rotate` and a mirror of the vertices (and fix triangle winding for one flip), or an equivalent that also turns text and images. Clip rects of a rotated item cover its rotated bounding box.
- Hit testing, hover, the focus ring, snapping, marquee and smart guides use the rotated outline (bounding box for marquee and guides, exact rotated rect for hits).
- Canvas: a rotate handle above the top edge; Shift snaps to 15°. Resize handles stay on the rotated frame and resize in the object's own axes. Inspector: a rotation field and two flip buttons. Commands: `object.set_rotation`, `object.flip` (`axis`).
- Entrance and exit animations and motion paths still work on rotated objects (fly-in moves the rotated object).
- PowerPoint: `xfrm@rot` (60000ths of a degree), `flipH`, `flipV` on shapes, pictures and group children (a group's rotation adds to its children's, rotating their centres about the group centre).

## Step 2: Opacity, fills and effects

- `Object.opacity` 0..=1 (omitted when 1). Multiplies with animation fades.
- `Fill` for shapes and buttons: solid (today's `Color`, which still loads), `linear` (angle, stops 2..=10 of colour and position), `radial` (centre, stops), `picture` (media `src`, stretch or tile). Old files with a plain colour load as solid.
- `Object.shadow` (colour with alpha, offset x/y, blur) and `glow` (colour, size), omitted when unset; drawn behind the object, rotated with it. Blur is approximated with a few layered, increasingly transparent copies, capped in cost.
- Inspector: fill type picker with stop editing, picture choice, opacity slider, shadow and glow toggles with their values. States can override fill (as they override colour today).
- PowerPoint: `gradFill` (linear and path), `blipFill` on shapes, `outerShdw`, `glow`, and alpha already kept.

## Step 3: Text

- Paragraph properties on text runs' paragraphs: list (`none`, `bullet`, `number`), indent level 0..=4, line spacing (multiple, 0.8..=3), space before and after. Model paragraphs explicitly (a `paragraphs` list on text and shape text, each with runs) or as paragraph attributes on the run that ends a line; pick the one that keeps old files loading and the XLIFF and find-and-replace code simple, and record the choice in `docs/spec/course-model.md`.
- Links: a run may carry a link, either a URL or a slide. Clicking it in the player runs the same effect as the `OpenUrl` and `JumpToSlide` actions. Links are underlined in the theme accent, reachable by keyboard, and announced as links on the web.
- Shrink text to fit: `autofit` on text boxes and shape text (`none`, `shrink`); shrink lowers the font scale until the text fits, never below 40%.
- Formatting toolbar: bullets, numbering, indent, outdent, line spacing, link (URL or slide picker).
- PowerPoint: `buChar`, `buAutoNum`, `lvl`, `lnSpc`, `spcBef`/`spcAft`, `hlinkClick` (URL and slide jumps), `normAutofit`.

## Step 4: Custom fonts

- An author adds a `.ttf` or `.otf` (File ▸ Add font…, or from the font picker). It's copied into the project's media under `fonts/` and listed in `course.fonts` (family name read from the file, style: regular, bold, italic, bold italic). Validation: size cap, the file must parse, family names are bounded and cleaned.
- The font picker lists Inter plus the course's fonts. Missing bold or italic faces fall back to the regular face (faux bold is fine).
- Every renderer registers the course's fonts before drawing; the published package includes them; the player loads them from `media/`.
- PowerPoint: fonts named in the deck that aren't in the course are listed in the import report ("Add these fonts to keep the look").

## Step 5: SVG and animated images

- SVG images: rasterised with `resvg` at the size they're drawn (re-rasterised when the drawn size changes by more than 25%, cached), on every target including wasm. Hostile-input limits: file size, element count, no external references.
- Animated GIF and WebP: frames decoded with `image`'s animation decoders, played in a loop at their frame timing in the player and preview, first frame on the canvas, frame and memory caps. An `autoplay`-style choice is not needed: they always loop, like on the web.

## Step 6: Lottie

- A `Lottie` object kind: `src` (a `.json` or `.lottie` in media), `loop`, `autoplay`, `speed`. Triggers can play, pause and stop it like media, and `media_ends` fires at the end of a non-looping run.
- Render with a pure-Rust Lottie renderer that draws through the same painter (so canvas, player, offscreen and wasm match). If no suitable crate exists, write `coursecraft-lottie` covering what LottieFiles animations commonly use: shape layers, groups, rectangle, ellipse, path, fill, stroke, linear and radial gradients, transforms with keyframes and bezier easing, trim paths, opacity, precomps, parenting and masks (add mode). Unsupported features are listed in a warning on the object, never a crash.

## Step 7: Tables

- A `Table` object kind: rows and columns (each 1..=50), column widths and row heights as fractions, cells with rich text (the same paragraph model as step 3), cell fill, borders (outer and inner: colour, width), a header-row style, and merged cells (row and column span).
- Editing on the canvas: click a cell to type, Tab to the next cell, add or remove rows and columns, drag column and row borders. Commands for each.
- Reading order and screen readers: a real table on the web (header cells as headers). XLIFF and find-and-replace include cell text.
- PowerPoint: `a:tbl` in a `graphicFrame` (grid, cells, spans, fills, borders, text).

## Done bar

Each step: model and validation tests, engine command tests with undo, render tests (offscreen pixels where it matters), a UI harness test of the canvas or inspector control, a PowerPoint fixture test, `simulate` output on the examples unchanged, `cargo xtask ci` green. Claude plays each step by hand in the web editor and preview after review.
