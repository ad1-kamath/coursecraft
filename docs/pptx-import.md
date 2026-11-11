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
- **Tables:** A `p:graphicFrame` with `a:tbl` becomes a `table` object. `a:tblGrid/a:gridCol@w` and `a:tr@h` become column-width and row-height fractions. Each `a:tc` carries `a:txBody` rich text, `a:tcPr` solid fills, and `gridSpan`/`rowSpan` merges (`hMerge` and `vMerge` continuations are kept as empty covered cells). `a:tcPr` side lines (`lnL`, `lnR`, `lnT`, `lnB`) become the table's outer and inner borders using the first table's most common value on each side. `a:tblPr@firstRow` sets `header_row`.

## What is Skipped

Elements not supported by the CourseCraft runtime are skipped and listed in the diagnostic import report:

- **Charts:** Office charts in graphic frames.
- **SmartArt Diagrams:** SmartArt graphics and DrawingML diagrams.
- **Audio and Video:** Media tracks and video shapes.
- **OLE Objects:** Embedded Excel spreadsheets or external OLE objects.
- **Animations and Timing:** PowerPoint animation effects and timing tracks (`p:timing`).
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
- Shape limit: maximum 2,000 shapes per slide.
- XML element depth: maximum 64 levels deep.
- Invalid EMU coordinates (negative, non-numeric, or absurdly large numbers) skip that shape safely without panicking.
- Targets escaping package boundaries (`..`, absolute paths, external URLs) are rejected and skipped.
