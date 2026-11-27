# PowerPoint (.pptx) Import

CourseCraft can import existing PowerPoint presentations (`.pptx`) headlessly via the CLI or interactively through the desktop and web editors.

## How to Import

### Editor
Choose **File ▸ Import PowerPoint…** from the menu bar. If the current project has unsaved modifications, CourseCraft prompts you to save or discard changes first. The presentation is imported as a new untitled project (marked dirty so closing prompts to save), and an Import Report summary dialog displays any skipped elements or layout adjustments.

### CLI
```sh
# Import into a self-contained .ccraft project archive:
cargo run -p coursecraft-cli -- import-pptx presentation.pptx -o output.ccraft

# Or import into a folder containing course.json and media/:
cargo run -p coursecraft-cli -- import-pptx presentation.pptx -o output_dir/
```

## What is Imported

- **Slide Order and Dimensions:** Slides are imported in presentation sequence into an "Imported" scene. The target slide canvas is 1280×720 (16:9). Presentations with different aspect ratios (such as 4:3) are scaled uniformly and letterboxed to fit, with an import report note recorded.
- **Slide Titles:** The first line of text from a title placeholder (`title` or `ctrTitle`) becomes the slide's title (up to 200 characters). The title is taken even when the placeholder's position cannot be resolved. If no title placeholder exists, slides are named "Slide N".
- **Speaker Notes:** Slide notes from notes pages (`p:ph type="body"`) are imported into the slide's `notes` property and displayed read-only in slide properties.
- **Placeholders:** Placeholders with text are imported as text objects. A placeholder without its own position inherits it from the slide's layout (matched by `idx`, else by type; `ctrTitle` matches `title`, no type means body) and then from the layout's master (matched by type). If no position can be found, the placeholder is skipped with the note "Placeholder without position skipped". Inherited font sizes and colours are not read: placeholder text without an explicit size uses the usual Office defaults (titles 44 pt, subtitles 24 pt, other placeholders 28 pt), and other text uses the course default.
- **Text Boxes and Text Runs:** Rich formatting on individual text runs (`a:r`) is preserved, including bold, italic, underline, font size (points converted to slide pixels at 96 dpi, then scaled with the slide), solid RGB colors, and `a:hlinkClick`. Paragraph alignment (left, center, right) and vertical alignment (top, middle, bottom) are preserved. Paragraph lists come from `a:pPr`: `lvl`, `a:buChar` → bullet, `a:buAutoNum` → number, `a:buNone` → none. `a:lnSpc/a:spcPct` becomes `line_spacing` (1000ths of a percent); `a:spcBef` and `a:spcAft` with `a:spcPts` become `space_before`/`space_after` (100ths of a point converted to pixels at 96/72). `a:normAutofit` becomes `autofit: "shrink"`. A link with an external relationship target becomes a URL link (http/https/mailto only); `action="ppaction://hlinksldjump"` with a slide relationship becomes a slide link. Without an explicit alignment, text in shapes and in centred-title or subtitle placeholders is centred, as in PowerPoint; text boxes stay left-aligned.
- **Basic Shapes:** Preset shapes with solid RGB fills are imported. `a:gradFill` becomes a linear or radial `fill_style` (`a:gsLst` stops with `pos` in 1000ths of a percent, `a:lin@ang` in 60000ths of a degree, and `a:path`/`a:fillToRect` for the radial centre). `a:blipFill` inside `spPr` becomes a picture fill with `stretch` or `tile`. Shapes that include both geometry and text are imported as two objects occupying the same frame: the shape beneath, followed by the text box.
- **Pictures:** Embedded pictures (`png`, `jpeg`, `webp`, `gif`) are decoded, validated, copied into the course's `media/` folder with safe sanitized filenames, and placed on the slide with their alternative text (`descr` or `name`). The same media path is used by a picture fill.
- **Shadows and Glows:** `a:effectLst/a:outerShdw` becomes `shadow` (`blurRad`, `dist`, `dir` and RGB colour with alpha), and `a:glow` becomes `glow` (`rad` and colour). Distances and radii are converted from EMU to slide pixels.
- **Groups:** Grouped shapes (`p:grpSp`) are flattened into the slide's document order, with coordinates scaled and offset according to group bounds (`off`, `ext`, `chOff`, `chExt`) up to a nesting depth of 8.
- **Tables:** A `p:graphicFrame` with `a:tbl` becomes a `table` object. `a:tblGrid/a:gridCol@w` and `a:tr@h` become column-width and row-height fractions. Each `a:tc` carries `a:txBody` rich text (`a:bodyPr@anchor`, falling back to `a:tcPr@anchor` for vertical alignment), `a:tcPr` solid fills, and `gridSpan`/`rowSpan` merges (`hMerge` and `vMerge` continuations are kept as empty covered cells). `a:tcPr` side lines (`lnL`, `lnR`, `lnT`, `lnB`) become the table's outer and inner borders using the table's most common value on each side. Tables without a `<a:tableStyleId>` receive PowerPoint's default 1 pt `#000000` outer and inner grid unless explicitly disabled via `<a:noFill/>` on all sides of that group; `{5940675A-B579-460E-94D1-54222C63F5DA}` ("No Style, Table Grid") also receives the 1 pt `#000000` grid, while `{2D5ABB26-0587-4C30-8999-92F81FD0307C}` ("No Style, No Grid") stays borderless. Unrecognized `<a:tableStyleId>` GUIDs do not receive the default black grid, honour explicit cell borders, and record a diagnostic note. `a:tblPr@firstRow` sets `header_row`.
- **Animations and Motion Paths:** `<p:timing>` entrance and exit animations, motion paths (`p:animMotion`), and interactive click sequences (`interactiveSeq`) are imported:
  - **Entrance & Exit Presets:** `Appear` / `Disappear` (`presetID=1`, instantaneous visibility via `timing.start_ms` / `timing.end_ms`), `Fly In` / `Fly Out` (`presetID=2`), `Fade` (`presetID=10`), `Wipe` (`presetID=22`), `Zoom` (`presetID=23`), and `Float In` / `Float Out` (`presetID=42`, `47`). Cardinal directions (`presetSubtype` `1`=Top, `2`=Right, `4`=Bottom, `8`=Left) and `accel`/`decel` easing (`Linear`, `EaseIn`, `EaseOut`, `EaseInOut`) are preserved. Unsupported presets fall back to `Fade` (or `Wipe` for `Peek` `id=12`, `Zoom` for `Grow & Turn` `id=53`) and diagonal directions map to cardinal with a diagnostic note.
  - **Timeline Scheduling:** `withEffect` and `afterEffect` timings are preserved with delays; `clickEffect` steps in the main sequence are staggered onto the timeline at 500 ms intervals with a diagnostic note; `slide.duration_ms` is extended to fit all main-sequence animations.
  - **Motion Paths (`p:animMotion`):** `M`, `L`, `C` (cubic Bézier subdivided into polyline segments), `Z`, and `E` commands are scaled from normalized slide coordinates to slide pixels (downsampled to at most 100 points) and wired with `TimelineStarts` / `TimelineReaches` -> `MoveOnPath` triggers (or `Click` -> `MoveOnPath` inside `interactiveSeq`).
  - **Interactive Click Sequences (`interactiveSeq`):** Click-triggered motion paths become `Click` -> `MoveOnPath` triggers; click-triggered entrances/exits map to `ChangeState` (`hidden` <-> `normal`) with a diagnostic note.
  - **Group & Connector Targeting:** Animations targeting a grouped shape (`p:grpSp`) apply to all flattened child objects; animations targeting connectors (`p:cxnSp`) are also resolved.
- **Fonts:** Embedded fonts (`p:embeddedFontLst`, including ECMA-376 `.fntdata` obfuscated fonts) are extracted into `media/fonts/` and registered in `course.fonts`. On desktop, text also resolves against fonts installed on your computer. Any deck font that is neither embedded nor installed is listed in the import report (`"Add these fonts to keep the look: … (shown as Inter)."`). When publishing on desktop, used installed fonts whose licence permits embedding are auto-embedded into the package (and offered to be saved into the project).

## What is Skipped

Elements not supported by the CourseCraft runtime are skipped and listed in the diagnostic import report:

- **Charts:** Office charts in graphic frames.
- **SmartArt Diagrams:** SmartArt graphics and DrawingML diagrams.
- **Audio and Video:** Media tracks and video shapes.
- **OLE Objects:** Embedded Excel spreadsheets or external OLE objects.
- **Unsupported Animations:** Emphasis animations (`presetClass="emph"` such as Spin, Pulse, or Grow/Shrink), animation repeats (`repeatCount > 1000` or `"indefinite"`), and duplicate entrances on the same object are skipped with a diagnostic note (without hiding the emphasized object). On-click main-sequence steps and unsupported entrance/exit presets are approximated as described above.
- **Slide Transitions:** PowerPoint slide transitions.
- **Unsupported Fills:** Pattern fills on shapes (imported using the default fill `#D9D9D9`). Gradient and picture fills are imported (see above).
- **Connectors and Lines:** Line connectors (`p:cxnSp`).
- **Custom Geometry:** Non-standard and freeform shapes (`a:custGeom`).
- **Empty Placeholders:** Placeholders containing no text.
- **Hidden Slides:** Slides marked as hidden (`show="0"`).
- **Undecodable Images:** Vector graphics or formats not supported by the safe decoder (e.g. WMF, EMF, TIFF).
- **Rotated or Flipped Objects:** `a:xfrm@rot` (60000ths of a degree), `flipH` and `flipV` are imported on shapes, pictures, connectors and group children. A group's rotation adds to its children's and rotates their centres about the group centre; a group's flips compose into the children.
- **Scheme Colors:** PowerPoint theme scheme colors are mapped to default light grey (`#D9D9D9`) with a report note.

## Safety and Resource Limits

The PowerPoint importer treats all input as hostile and enforces strict bounds:
- Archive entries: maximum 10,000 entries.
- Uncompressed archive size: maximum 512 MiB total, 256 MiB per entry.
- Slide limit: maximum 500 slides.
- Shape limit: maximum 2,000 shapes per slide. Each path of a custom-geometry shape counts as a shape, and extra paths are skipped with a report note.
- Table limit: a table with more than 50 rows or columns, or more than 2,500 cells, is skipped with a report note before any cell is built.
- Shape ids (cNvPr/@id) are numbered per XML part. Master and layout ids are forgotten before the slide is read, so an animation or trigger on slide shape 2 never reaches master shape 2.
- XML element depth: maximum 64 levels deep.
- Invalid EMU coordinates (negative, non-numeric, or absurdly large numbers) skip that shape safely without panicking.
- Targets escaping package boundaries (`..`, absolute paths, external URLs) are rejected and skipped.
