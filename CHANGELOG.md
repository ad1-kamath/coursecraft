# Changelog

What changed in each release, newest first. Versions follow [semantic versioning](https://semver.org): until 1.0, a minor version (0.x) may change things, but a project saved by an older CourseCraft always opens in a newer one.

## 0.6.7

Luca's two animation ideas: groups that animate as one picture and animations on slide masters. Also a dark app icon on macOS, the Windows portable download is back, and the matching question lists open again.

**Animations**
- A group now animates as one picture. An entrance, exit or emphasis on a group moves, zooms, turns, wipes and fades the whole group around its shared box, and overlapping shapes no longer show through each other halfway through a fade. Items inside the group can still have their own animations. Courses made before keep playing as they did. Thanks to Luca.
- Objects on a slide master can now have entrance, exit and emphasis animations, timed on the timeline in Master View. They play from the start of every slide that uses the master, and stay unclickable. Thanks to Luca.

**Questions**
- In the editor's Preview, the "Choose…" lists in matching questions now open. Escape closes only the open list, and the arrow keys move through the list you opened.

**Downloads**
- Windows: a portable download is back (`-portable.zip`). Unzip it and run CourseCraft without installing, handy for trying it out. The installer is still there too.
- macOS: CourseCraft has its own dark app icon when Dark icons are turned on in System Settings (macOS 26 and later).

## 0.6.6

Fixes from using 0.6.5: easier split buttons, no more pile of slide tabs, a layer bar you can always see, and shadows that follow what a shape draws.

**Ribbon**
- Split buttons such as Preview are easier to use: on large buttons the icon runs the action and the label with its arrow opens the menu, and each part lights up on its own when you point at it. Small split buttons have a wider arrow (#17).

**Slide tabs**
- Clicking slides or moving through them with the arrow keys shows them in one temporary tab, in italics, instead of opening a new tab each time. Double-click a slide or its tab, or edit the slide, to keep the tab. When the tabs don't fit, the row scrolls.

**Layers**
- The layer bar above the slide (Base, the layers, + and Base slide: Show, Dim, Hide) is always fully visible and is no longer hidden behind the slide.

**Shapes**
- With Fill turned off, a shape's shadow and glow follow its outline; with no outline either, they are hidden. The settings are kept, so turning Fill back on brings them back. The format panel says why the effect can't be seen.

## 0.6.5

Two new sample courses, redesigned templates, and fixes for triggers, question banks and variable text.

**Sample courses and templates**
- Two new sample courses on the welcome screen: Workplace Safety Basics (tabs, a spot-the-hazard question, a quiz with results) and Feature tour (video with captions, states, motion paths, a map with markers and every question type).
- The Quick Knowledge Check, Lesson, Branching scenario and Blank templates are redesigned with real layouts instead of plain text and buttons.
- The Quick Knowledge Check results slide no longer shows raw variable names to learners.

**Validation**
- Check Course and `validate` now warn when text uses an unknown `%variable%` reference.
- Validation warns when a question bank slide uses a slide master that no longer exists.

**Authoring**
- Button labels on light fills now use dark text so they stay readable and pass contrast checks, on the slide, in Preview and while you type a label.
- The trigger editor no longer saves over a different trigger after Undo or Redo; it tells you the trigger changed instead.
- A question bank slide's master can be changed or cleared, and deleting a master clears it from bank slides.

**Privacy**
- AI error messages now hide API keys that sit next to punctuation, such as in quotes or after a colon.

## 0.6.4

A fix for videos crashing the app on Windows, EMF and WMF clip art, and clearer player settings.

Thanks to LucaFontanot for EMF and WMF clip art and the player settings work (#14 and #15), and to cocoon, who reported the Windows video crash (#16).

**Fixes**
- Playing a video no longer closes the app on Windows (#16).

**Pictures**
- EMF and WMF clip art, from PowerPoint import or Insert › Picture, is kept as vectors.

**Player settings**
- A slide's own player settings no longer silently beat the course settings. Course settings names the slides that change each setting and offers "Use on all slides".
- With the player bar hidden on a slide, Next, Previous and the slide menu are refused everywhere, not only in the player window.

## 0.6.3

More than 150 bug fixes from a review of 0.6.1 and from rebuilding a sample course, and a redesigned Healthy Habits at Work sample course.

**Sample course**
- Healthy Habits at Work, the course that opens on first launch, is rebuilt: a new look with illustrations, voiceover with captions, music, check-your-understanding questions, drag-and-drop sorting, a breathing exercise, a mood check-in, a personal plan and a certificate.

**Animation and timing**
- Moving an object on the timeline also moves its emphasis. Copy Animation keeps an emphasis offset inside the target's time.
- An entrance or exit cut short by the end of the slide now finishes and sends "animation completes". Validation warns when an animation falls outside the object's time.
- PowerPoint import no longer counts the delay of an "After previous" effect twice.
- Overshoot and Elastic easing now play fully on emphasis and motion paths.
- The Fade transition no longer dips to dark at its midpoint.
- Deleting a member of a group no longer leaves a group of one object.

**Reduced motion**
- With reduced motion on, transitions, entrances and exits become a short fade, and emphasis plays one time with no movement.

**Text, paste and fonts**
- Text that plays in by letter now fills in `%variables%`.
- Symbols such as ✕ ✔ ★ ⚠ now draw, from a small bundled symbol font. `validate` warns about characters that no bundled font can draw.
- Change Case handles curly apostrophes and abbreviations such as "e.g.". Change Case and Clear Formatting with an empty selection change nothing.
- A named style with "clear overrides" also clears highlight and shadow.
- A paste mode chosen from Paste ▾ no longer applies to a later Ctrl+V. Use Destination Theme maps a heading font when the destination has only a body font. Paste Special lists Keep Text Only one time, and Paste ▾ marks the default.

**Pictures and questions**
- A picture cut into a shape keeps its crop. Picture fills have a new Fill mode that keeps the picture's proportions.
- Image shapes accept `circle` as a name for `ellipse`.
- A question can have a frame. Its prompt and answers are then laid out inside the frame.

**Editor**
- The ribbon Font box shows Inter for the default font, and font lists mark only one row.
- Escape on a font list closes only that list. The Theme Fonts window closes with its Theme window. Design › Fonts opens the font settings directly.
- The Symbol window keeps its place when its size changes.
- Focus Order shows the base layer and each layer in its own section, and closes when the slide changes under it.
- Inspector tab names are no longer cut to three letters.
- Font Color and Highlight show the current colour under the icon.
- Scale % uses the correct size after a resize elsewhere.
- Picking a gallery effect keeps the Sequence setting. An effect applied to several objects is one undo step. Entrance › Appear from the gallery now works.

**Files and CLI**
- Each untitled window has its own autosave file, so a second window cannot restore or discard the work of the first.
- `render --layer` shows only the named layer. `snapshot-player` "ended" shows the slide at its real end.

**Files, import, export and publish**
- `coursecraft-cli run`, `new` and `pack` write a real `.ccraft` archive, with all media, when the output path ends in `.ccraft` in any letter case. `publish` of a `.ccraft` packs the archive's own media.
- `import-xliff` and `import-questions` refuse an output that is the input course. All three writes keep media, check a `.ccraft` opens again, and replace the old file only when the new one is whole.
- A project folder whose `media` is a link to a folder outside the project is refused. A planted `course.json.ccraft-tmp` link is no longer written through. A case-only rename of a media file no longer leaves two files on a case-sensitive disk.
- A bare `course.json` keeps its web folders when it is opened in the app or in `coursecraft-mcp` and saved as a `.ccraft`. A project folder opened by `coursecraft-mcp` saves only what changed.
- MP4 export no longer fails on a size such as 1200 x 800, and no longer deletes an existing file when an export fails or is cancelled.
- PDF and Word review exports shrink a tall slide picture to fit the page.
- XLIFF import keeps a space typed between two formatted runs and accepts CDATA sections.
- `batch --replace-media` treats `.m4a` as audio and accepts fonts and other files. Packages from courses with the same file name no longer overwrite each other.
- `coursecraft-cli` media folders on Windows use `/` in media names, and a link loop no longer hangs.
- `course.get` with the pointer `/tracking` works. xAPI launch links with an empty `registration` or `activity_id` use the default. SCORM manifests refuse a launch path with a colon.

**Runtime (second review)**
- Nothing runs after an `exit` action, even another trigger on the same event.
- Safe Resume with a lightbox open no longer marks the base slide completed. A lightbox slide keeps its question order.
- A slide cannot open as a lightbox over itself.
- Intersections and the "hidden base" rule are rechecked after a trigger changes a state or a layer.
- Emphasis, trimmed media start, cue points at the trim end, layer media stop and seeks during a slide jump follow the spec.
- Answer review shows dragged objects where the learner dropped them.
- LTI: a failed retry no longer drops a newer result, and a 401 or 403 mid-session turns reporting off.

**Player (second review)**
- A new playback speed applies to media at once.
- Text entry commits on `Tab` and on a screen-reader focus change.
- Inputs and draggable objects on the base layer ignore the learner while a layer with "prevent base clicks" shows. A disabled object cannot be dragged.
- `M` and `C` reach the slide's key triggers when the menu or captions are off. The end card and an open speed list take no slide clicks.
- Scrollbar thumbs move the right way, and scroll panels clip their text boxes.
- Captions from web videos are not drawn two times. Flipped text stays readable, and its shadow and links follow the text.
- Scroll panel thumbs, reveal masks and base markers draw in the right place during transitions, drags and rotation.
- `run_javascript` can read built-in and timer variables. Path hit tests, table border dashes and text link areas match what is drawn. Media file types are found in any letter case.

**Editor (second review)**
- In Edit State, the Format inspector, text panel, Ctrl+B/I/U and the Size and Position panel change the edited state, not the normal state.
- Insert puts tables, inputs, markers, scroll panels, timers, meters and web objects on the selected layer. Controls that cannot work in Master View or Edit State are disabled with a reason.
- Opening another course clears old caches. Import from the Start page leaves the Start page.
- Trigger Editor: timer actions and the Timer Ends event keep separate timers, and pickers reset when the variable or target changes.
- Format and Animation Painter work between duplicated slides. Copy › Duplicate copies all selected objects.
- The ribbon shows "mixed" for mixed font, size, colour and indent. Numbering › Start at changes nothing unless the value changes.
- Emphasis sliders drag smoothly. Multi-selection rotation shows and resets correctly. Theme palette swatches update.
- Button Fill and Border open the right panel. Question Tools › Display and Scoring open the docked panel.
- A single click on a ruler adds no guide. F2 on a question bank slide does nothing. Layer eye toggles reset when the slide changes.
- More dialogs block shortcuts, and more strings are translated. Recent file names are no longer sent through translation.

**Accessibility (second review)**
- `Tab` moves between drop targets while a keyboard drag is held. `Space` on a radio group picks the next option.
- Screen readers no longer reach hidden objects, hidden master objects or the slide behind an open drawer or the end card. The end card reads "Course complete". Disabled inputs are disabled in the web page.
- Focus returns to the object that opened a lightbox, also after a mouse click.
- In the web player, links, glossary terms, draggable objects and scroll panels work by keyboard, and letter and digit keys reach key triggers.
- The player tells the course when focus leaves an object. Slide key triggers do not fire behind the drawer. Review mode is read-only for matching, hotspot and drag questions.
- The focus ring no longer wraps a state badge and covers every line of a wrapped link. The full-screen exit tooltip is translated.

**Hostile input (second review)**
- PPTX colours with non-ASCII text, very large Roman numerals, zip bombs, Lottie files with too much drawing work, very large PPTX tables, long LTI token lifetimes and very low audio rates no longer crash or hang.

**Engine (second review)**
- Text styles, connector frames, merges, state links and slide paste or duplicate now keep the course valid. Delete commands clean up the links they break.
- The theme accent follows the palette. Appear has a 500 ms default. Replace with no match makes no change. Styles apply to lines. A disabled "complete course" trigger does not count as reporting.
- `course.add_font` accepts `.ttc` and `.otc` files.

## 0.6.1

The Download and What's new buttons in the update window now work, plus text and formatting gaps (M39) and two fixes from GitHub issues.

Thanks to apomultimedia70, who reported the Triggers and Layers issues fixed in this release (#11 and #12).

**Text formatting**
- Highlight colour and text shadow for any part of the text, from Home › Font and the Text panel. PowerPoint import brings in highlights and text shadows.
- Clear Formatting removes character formatting from the selection and keeps links.
- Change Case: Sentence case, lowercase, UPPERCASE, Capitalize Each Word and tOGGLE cASE.
- Numbering is a split button: the main part turns numbering on or off, and the ▾ offers seven number styles and Start at.
- Text Direction: Horizontal, Rotate all text 90° and Rotate all text 270°.
- The Format Painter also copies highlight and shadow.

**Insert**
- Slide Number (or Slide Number of Total) at the text cursor, or in a new text box.
- A Symbol window and a Reference window that inserts a variable as `%Name%`.

**Paste and focus order**
- Paste ▾: Use Destination Theme (the default), Keep Source Formatting, Keep Text Only, and Paste Special….
- A Focus Order window sets the Tab order of a slide: move rows up and down or drag them, then OK.

**Theme fonts and size**
- A theme can have a heading font and a body font. Slide titles from layouts use the heading font.
- Design › Fonts ▾ lists font pairs and opens a new Create New Theme Fonts dialog. Font lists show the theme fonts first.
- The Size and Position panel has Scale % and, for pictures, Reset to original size.

**Fixes**
- Triggers panel (issue #11): rows for the selected object are highlighted, and the list scrolls to the first one only while the Triggers tab is open.
- Layers (issue #12): a focused layer now shows only itself and the base slide. Turn on another layer's eye to show it as well.
- Links open in the browser on the desktop: Download and What's new in the update window, Open URL in Preview, and help links.

## 0.6.0

A new animation system: many more entrance, exit and emphasis effects, slide transitions, and live previews while you pick them.

Thanks to LucaFontanot, who designed and built the whole animation system in this release (PR #13).

**Entrance and exit effects**
- 25 entrance and exit effects in four groups (Basic, Subtle, Moderate, Exciting), including Fly, Float, Wipe, Zoom, Bounce, Flip, Swivel, Wheel and Dissolve, with diagonal directions and shape, in/out, spokes and orientation options.
- Effects can scale, turn, tint and reveal objects through shaped masks, and text shrinks smoothly instead of rewrapping.
- Play an entrance or exit as one, by paragraph or letter by letter.
- New easings: Overshoot, Bounce and Elastic.

**Emphasis effects**
- 12 emphasis effects such as Pulse, Grow and shrink, Spin, Shake, Heartbeat, Jump, Blink and Colour pulse, each with its own start, duration and repeat, layered over an entrance or exit.
- The Animation Painter copies an object's animations to other objects, like the Format Painter.
- Timeline bars show strips for each animation.

**Slide transitions**
- 15 slide transitions: Fade, Fade through black, Push, Cover, Uncover, Reveal, Wipe, Split, Zoom, Shape, Clock, Dissolve, Random bars, Blinds and Checkerboard, each with its options.

**Picking and previewing**
- Every gallery tile draws its own effect, looping while you hover it. Effect Options shows directions as a compass and the other options with icons.
- The Animate panel has Entrance, Exit and Emphasis cards with an effect picker (search, Recent, groups), duration in seconds and easing drawn as curves.
- The canvas plays the selected objects' animation in place when you pick an effect or press Preview. Auto-preview turns this off.
- PowerPoint import brings in the matching entrance and exit presets.
- Agents can list every effect with the new `animation.effects` command, and add emphasis with `object.add_emphasis`, `object.set_emphasis`, `object.remove_emphasis` and `object.copy_animation`.

Courses saved by older versions open unchanged, and an unknown effect in a file plays as Fade.

## 0.5.3

Everyday-use fixes for the keyboard, the slide list, the stage, number fields and the ribbon.

**Keyboard and slide list**
- After clicking a slide in the slide list, Up/Down select the previous or next slide (across scenes) and Home/End the first or last. Shift extends the range, and the list scrolls to the slide.
- Delete, the arrow keys, Undo/Redo, Ctrl+A and Ctrl+D no longer act on the course behind the start page, the File page or a dialog.
- Ctrl+A skips locked and hidden objects, and the arrow keys and Delete leave locked objects alone.
- Holding an arrow key to nudge an object is now one undo step.
- Zooming in or out keeps the centre of the view.
- Ctrl+S in a text field now keeps what you typed and saves.
- Cancelling Open… on the start page or File page keeps the page open instead of showing an empty course.
- Help › Welcome with a course open: Course Info, Version History and Manage Fonts work, and Esc goes back.
- File › Check Course opens the problems window with the result.
- Cut, Copy, Duplicate, Move and Paste in the slide list use all selected slides, as one undo step.

**Stage**
- Ctrl+click adds an object to the selection or removes it, and Shift/Ctrl+drag on a selected object moves the selection.
- Clicking one object of a multi-selection selects just that object; dragging still moves them all.
- Dragging the middle of a straight line moves it, and thin lines are easy to click at any zoom.
- Alt+drag shows an outline of the copy, resize handles are easier to hit, and the resize cursor follows the object's rotation.
- Crop works on rotated and flipped pictures.
- Less typing lag in point text and auto-size text boxes, and less lag after edits on busy slides.

**Number fields**
- Typed numbers are kept and drags no longer jump back in Focus order, Draw-from-bank Count, motion path Duration, hotspot markers and region position and size.
- One drag or one typed number is one undo step in text insets, border width, Input max characters, Dial and Slider settings, Scroll panel height, Lottie speed and "Slides viewed – at least N".

**Ribbon, timeline and player**
- Clicking the open ribbon tab no longer collapses the ribbon; double-click it (or Ctrl+F1) instead. Inserting an object no longer expands a collapsed ribbon.
- Ribbon tooltips show the keyboard shortcut, and New Style from Selection always makes a unique name.
- Timeline Show all and Unlock all cover every layer of the slide, as one undo step.
- Smoother slide list, waveforms and state cards, and the web player saves its progress only when it changed.

**Files and translation**
- A failed open of a `.json` course no longer drops the pictures and sounds of the course that was open.
- Saving a `.json` course from the CLI or MCP writes its media to the `media/` folder beside it, and reports an error instead of marking it saved if a file can't be written.
- The translation check counts repeated `%var%` and `{var}` placeholders and finds a placeholder after a lone `%`.
- Changing the slide size with content scaling also scales tiled picture fills.

## 0.5.2

Quick fixes for everyday authoring, an Intel Mac download, and two fixes from user feedback.

- Choosing pictures, video, captions or media files that can't be read now says which file failed and why, instead of doing nothing.
- The Theme window picks fonts with the same font picker as the text panel.
- The Button Tools › On Click tooltip now says what it does.
- Insert › Media › Caption now opens the callout shapes so you can pick and draw one on the slide.
- Opening a `course.json` now loads its pictures, audio and video from the `media/` folder beside it.
- Published web courses now show a loading indicator until the player starts, or a short message if it fails to start.
- Spell check now skips non-English courses and explains this in the Check Spelling window.
- The Windows installer and Linux packages now register `.ccraft` files to open in CourseCraft.
- macOS releases now include an Intel Mac download alongside Apple Silicon.
- Unticking and re-ticking Fill on a shape or button now restores its previous colour instead of resetting to the default.
- Shortening an audio bar on the timeline now crops playback at the bar's end and keeps the waveform at its real time scale.

## 0.5.1

Choose how a course reports completion to the LMS, plus two editor fixes.

**LMS tracking options** (issue #9, thanks to apomultimedia70):
- A new Tracking section in Course Settings, the Publish window and the Course panel. Report completion when the learner has viewed all slides or at least a number of slides, from a quiz result, or both. Completion triggers keep working either way, and existing courses stay triggers-only.
- cmi5 and SCORM packages describe the chosen tracking to the LMS, so the LMS judges completion the same way the course does.
- A reported completion or pass is never lowered by a later attempt, a lower score or a relaunch, in SCORM 2004, cmi5, xAPI and LTI 1.3.
- SCORM 1.2 **Report status as** has two automatic choices. **Automatic (quiz result wins)**, the default, shows a failed quiz as failed even after the course was completed. **Automatic (completion stays)** keeps a completed course completed and still records the quiz score.
- Agents can set tracking with the new `course.set_tracking` command from the CLI and MCP.

**Fixes:**
- Question choices no longer squeeze into a narrow column when the question editor is in a narrow panel.
- Text boxes now resize while you type (one-line boxes grow sideways, wrapping boxes grow downward).

## 0.5.0

A new editor: a tabbed ribbon and panels you can arrange the way you like. Thanks to LucaFontanot, who built it.

**Ribbon and panels:**
- Commands are grouped in ribbon tabs: File, Home, Insert, Slides, Design, Transitions, Animations, View, AI and Help, plus tabs that appear for what you select (drawing, button, picture, video, audio and more). The Slide Master has its own tab.
- A button whose feature isn't built yet is greyed out, and its tooltip says why.
- Slides, Canvas, Timeline, the inspectors, States, Triggers, Variables, Question, Slide Layers and Notes are now panels. Move, tab, split, float or close them; a floating panel can leave the window as its own window. View › Panels reopens a closed panel where it was, and your layout is saved.
- The small ↘ buttons on ribbon groups open the matching settings panel (Text, Size and Position, Shape Style, Grid and Guides) instead of a dialog.
- The welcome screen is now the File tab's Start page, with a thumbnail of each recent course's first slide.
- Theme, transition and animation galleries are in the ribbon.
- Every interface label is ready for translation, with a Language setting in Preferences.

**Updates:**
- On Mac and Linux, **Update and Restart** now installs an update for you. It only happens when you click it, never while you have unsaved work, and closing the prompt during the download cancels it. Downloads are still checked against the published checksums first.
- On Windows, the update prompt gives you the installer to run.

**Downloads:**
- Windows: the installer (`-setup.exe`) only. Portable builds are no longer published; a portable copy is offered the installer when it checks for updates.
- Linux: a new `.AppImage` you can run straight away, alongside the `.tar.gz`.

Built on open-source libraries from the ArtCraft Crafting Apps (DeckCraft): `craft-ui`, `craft-i18n`, `craft-update` and the Craft egui/winit fork (MIT OR Apache-2.0).

## 0.4.6

Text boxes fit their text, and a group can work as one button.

**Point text** (issue #7):
- Click with the Text box tool to place a one-line text box that is only as wide as its text and grows as you type, so labels line up and centre without resizing. Drag to draw a fixed-width box that wraps, as before.
- Turning on Wrap text, dragging the side width handle or choosing Shrink turns it into a wrapping box, in one undo step. Works in the State Editor too, and from the CLI and MCP.

**Grouped buttons** (issue #8, part B):
- Turn on **Act as one button** for a group (or Arrange ▸ Make button from group). Hover, Down, Selected, Visited and Disabled then change on every member together, without flicker as the pointer moves between them.
- A click trigger on the group fires once, Tab stops on the button once with a focus ring around the whole group, Enter and Space activate it, and screen readers announce it as one button. Preview and published courses behave the same.
- Existing triggers on the members are never moved or deleted. Duplicating a slide keeps triggers that use the group.

Courses saved by older versions open, save and publish exactly as before.

## 0.4.5

Groups are easier to work with on the timeline, and the code underneath is tidier.

**Timeline** (part of issue #8):
- A group now shows as a **folder row** above its members. Click the arrow, or double-click the name, to collapse or expand it; this only changes your view, not the course.
- Drag the folder row's bar to move all the members' timing together (snaps to 100 ms, 10 ms with Shift, Escape cancels). It's one undo step.
- The eye and lock on the folder row show, hide, lock or unlock every member at once.

**Under the hood:**
- Media is tracked in one place, so new kinds of media can't be missed when saving, publishing or checking a course.
- Publishing shares media instead of copying it, and the command code is split into smaller files by area. Nothing changes in how CourseCraft works or what it saves.

## 0.4.4

CourseCraft is now much lighter on low-end computers, and it can tell you when an update is out.

**Faster everyday editing** (measured on a 300-slide test course):
- **Undo and redo: 3× faster** (8.8 ms → 2.9 ms), and inserting an interaction and undoing it went from 3.3 ms to 1.2 ms. Each edit now re-checks only what it changed instead of the whole course.
- **Moving between slides with pictures: about 7× faster** (3.3 ms → 0.44 ms). Pictures are decoded at the size they're shown, and the same picture used on many slides is kept in memory only once.
- **Check Spelling's Ignore: about 16× faster** (9.3 ms → 0.56 ms).
- **Published courses and Preview no longer redraw when nothing moves**, so a learner reading a slide uses almost no CPU or battery. They still wake for timers, media, animations and clicks.
- Saving in the background (autosave) no longer pauses the editor; opening and publishing big courses use much less memory; text, shapes and slides are cached instead of redrawn every frame; the published player is smaller.
- New **Performance mode** in Preferences (Auto, Standard, Lite). Lite uses less memory on weak computers and never changes how a published course looks.

**Check for updates** (thanks to aplannersguide for the idea):
- **Help → Check for updates…** shows the new version and what's new in it. CourseCraft can also check quietly once a day; turn that off in Preferences.
- **Download update** picks the right file for your computer, checks it's complete and genuine, and shows it in your Downloads folder. CourseCraft only asks GitHub for the version list and sends nothing about you.
- Each release now includes `SHA256SUMS.txt`.

## 0.4.3

Big courses are now much lighter and faster, and the menus are tidier.

**Faster, lighter editor** (measured on a 300-slide test course):
- **Undo memory: about 250 times smaller.** Each edit used to keep a full copy of the course for undo; now it keeps only what changed. 200 edits used +392 MB before and +1.6 MB now; on a course full of pictures and objects, +1.9 GB before and well under 1 MB now.
- **Editing is faster:** 200 edits took 397 ms before and 278 ms now (2.1 s → 1.1 s on the picture-heavy course), and undo or redo itself is instant.
- **The slide list only draws what you can see.** It used to redraw all 300 thumbnails every frame; now it redraws none while you're just looking, and only the one slide you change. An idle frame takes about a third of the time it did.

**Easier to find things:**
- New **Preview** menu with Preview this slide, **Preview from here** (Cmd+F12 on macOS, Ctrl+F12 elsewhere), Preview selected slides and Preview whole course.
- **File → Import** gathers PowerPoint, course from document, slides from outline, questions from a spreadsheet, translation (XLIFF), media files and fonts; **File → Export** sits next to it.
- Tooltips show the keyboard shortcut for every button and menu item that has one.
- Hovering a greyed-out button tells you why it's unavailable.
- Labels use sentence case everywhere, and each action has one name in every place it appears.

## 0.4.2

An urgent update that fixes ways to lose work, crashes and a wrong LMS result. Please update.

**Your work is safer:**
- **Ctrl+S saves** (Cmd+S on macOS). Ctrl+Shift+S is Save As, Ctrl+O opens and Ctrl+N starts a new course; they're shown in the File menu and in Keyboard shortcuts.
- In the web editor, New, Open, examples, templates and PowerPoint import ask before replacing a course with unsaved changes.
- Opening a `.ccraft` file through the MCP server (for AI agents) keeps its pictures and media, so saving no longer writes the file without them.

**Fixes:**
- **LMS results:** with SCORM 1.2 and a "Report status as" choice, a pass or completion that was already reported is never replaced by a later fail or lower score, in the same session or after resuming.
- Publishing works when a state's picture is empty; Check Course warns about it.
- The editor no longer crashes when inserting an input or interaction on a very small slide, or using Fit to content on a very tall scroll panel, and many similar edge cases are now safe.
- **Last visited slide** no longer bounces between the same two slides.
- Publishing from the web editor no longer freezes the browser tab and shows its progress.

## 0.4.1

**New: change a course's slide size** (thanks to PhilOB). A redesigned **Slide size** dialog has presets for Full HD, HD, 16:10, 4:3, square and portrait, or your own size, with an aspect-ratio lock. Choose **Scale to fit**, **Fill slide** or **Keep as is**, and a live preview shows where the content lands and how many objects would end up outside the slide before you apply. **Use as default for new courses** makes any size the starting size. Resizing now scales everything, including states and the items inside them. Open it from the File or Slide menu, the Course tab, Course settings, or the size shown in the status bar. Check Course warns about any object left completely outside the slide.

**New, small things:**
- The glossary is included in Find and Replace, Spell Check and translation (XLIFF), and can be sorted A to Z.
- **Show all** and **Unlock all** in the timeline and the View menu, each undone in one step.

**Fixes:**
- Reset overrides on a state no longer deletes the items inside it.
- In a published web course, web objects and videos no longer cover the Play button, the menu or the end screen, and keyboard shortcuts wait until the learner starts.
- A pressed button keeps its gradient or picture fill.
- Renaming the state you are editing keeps you in that state.
- The Timer uses a state's text style.
- Items inside a rotated or flipped object are placed, edited and rotated correctly.
- Dragging the seekbar to the end counts as watching the slide, so Next stays unlocked.
- The Seekbar setting is greyed out, with a reason, when the player bar or seekbar is off.

## 0.4.0

**New: rich object states.** States grow from a colour swap into a full second look for an object. Thanks to Noe for the feedback that shaped this.
- **States tab** at the bottom, next to Timeline and Triggers: every state of the selected object as a live thumbnail, side by side. Duplicate, rename and reorder states; renaming a state updates every trigger and question that uses it. Right-click an object → **Edit states…** opens the tab, and the Format panel keeps one compact Initial row.
- **Edit a state right on the canvas.** Pick a state and the normal toolbar and Format panel edit that state instead of the object. A dot marks each property the state changes, with a reset next to it, and Esc steps back out.
- **States can change much more:** fill style and gradients, outline, text style and formatted text, opacity, shadow, glow, crop, picture shape, rotation and flips, as well as the picture and text.
- **Items inside a state:** a state can add its own shapes, pictures, text and paths, such as a speech bubble on hover or a tick mark once visited. Insert, move, resize and edit them on the canvas, or from the **Items in this state** list.
- Find, Check Course and the Media Library now see everything inside states, and states come along when you translate (XLIFF), recolour or publish.
- New example course: `examples/rich-states`.

**New, from contributors** (thanks to Luca):
- **Seekbar behaviour** in Player Settings: Free (as before), Locked, or Locked until the slide's timeline has played once.
- **Play button before a web course starts.** Browsers keep sound off until the learner clicks, so a published web course now opens on its first slide behind a Play button and the first narration is never silent. It works with the mouse, the keyboard and screen readers.

## 0.3.6

**New: a calmer, more consistent editor.** Every dialog, menu, panel and player control was checked against the same 22 rules (readable contrast in light and dark, comfortable click targets, tooltips on every icon and on every greyed-out button, one scroll area per dialog, Esc closes one thing at a time) and fixed until an automated sweep of 61 editor screens and the player found nothing left to fix.

**New: states that do what you expect.** Hover and pressed states now change an object's picture and text, not only a button's colour. A state's picture is chosen with **Choose image…** (or **From project…**) instead of typing a file path, and a picture that isn't in the course is flagged in Check Course. Thanks to Noe for the feedback and the screen recordings.

**New, from tester requests** (thanks to PhilOB):
- **SCORM 1.2 status:** choose what the LMS receives: Passed/Incomplete, Passed/Failed, Completed/Incomplete or Completed/Failed (Automatic keeps today's behaviour).
- **Player bar off:** hide the player bar for the whole course or one slide and use your own buttons; the slide fills the window and the arrow keys don't skip ahead. Check Course warns about a slide with no way forward.
- **Hand pointer** over everything a learner can click in the preview and the published course.
- **Shadow and glow opacity** sliders.

**Preview and player:**
- The preview has a darker backdrop, can be resized from any edge or corner, and has a full-screen mode.
- **Preview this slide** previews only that slide, and **Preview selected slides** is new; at the end a short message offers to replay or close.
- The player's controls have their own space and never cover the slide or a video's subtitles.
- The outline drawer opens inside the player, not at the edge of the screen.
- Clicking a video plays and pauses it.

**Fixes:**
- Thick shape outlines are even all the way round, and pointed corners are no longer cut off; PowerPoint's corner style comes across, and a new **Corners** choice (Sharp, Round, Bevel) sits next to Dash.
- Bold, Italic and Underline show the style of the selected text, not of the letters next to it, and Underline toggles reliably.
- Check Course warns when a slide's states change from its timeline but revisiting keeps the old states (the example courses now reset on revisit).

## 0.3.5

**New: PowerPoint animations come across.** Importing a `.pptx` now brings its entrance and exit animations (Appear, Fly, Fade, Wipe, Zoom, Float), motion paths, effects that start on a click of a shape, and repeats, with their timing and order. Other entrance and exit effects become the closest match and are noted in the import report. Emphasis effects (Spin, Pulse and the like) are not imported yet.

Everything else in this release is bug fixes: 48 of them, found by a new strict test LMS that checks every message a course sends, a reference course that uses every feature, and tests across every part of the editor.

- **LMS reporting:** a SCORM 1.2 course with a certificate slide no longer turns "passed" into "completed"; decimal scores, empty first-launch status, long slide and question ids, and very long session times are reported correctly; the package no longer lists its own manifest; video posters and picture-filled paths are included in the package; xAPI and cmi5 save progress when a lightbox opens or closes and reject broken endpoints and empty tokens.
- **LTI:** scores post to line items with a trailing slash, a launch without score permission is refused cleanly, and a score still waiting when the learner closes the page is sent.
- **Player:** a lightbox over a slide with a video no longer resets the video's cue points, and jumping away from a lightbox no longer flashes the base audio.
- **PowerPoint import:** tables without a style get PowerPoint's thin black grid (tables with an unknown style no longer get one); "Disappear" no longer happens half a second late; grouped and connector shapes keep their animations.
- **Triggers and states:** Move Up and Move Down follow the filtered list; deleting a state that is in use explains why it can't be deleted; state names are checked; empty key-press and JavaScript triggers are refused.
- **Timeline and media:** cue markers can be dragged again; media on question-bank slides counts as used; "Go to" from the media library finds the object; renaming a font family is one Undo step; only `.vtt` caption files are accepted.
- **Editing:** reordering several selected layers keeps their order; pasting triggers between two objects keeps both ends; clicking a History step past a deleted slide selects it; deleting a layer or scene removes triggers that pointed to it; focusing a hidden layer shows it; deleting a master counts the templates using it.
- **Story View, Find and Check Course:** clicking a slide clears an old multi-slide selection (Story View could delete extra slides); arrow keys no longer move behind an open dialog; Find results and Check Course issues leave Story View and Master View and reach question-bank slides; a replacement that is too long is refused before it runs.
- **Files:** a failed Save As keeps the current file; autosave restarts for every newly opened course; deleted versions disappear from Version History at once, and its Delete button is no longer cut off.
- **Checked:** Undo and Redo were tested on all 166 editing commands.

## 0.3.0

A big round of fixes from real testing, much better PowerPoint import, real fonts, and a Windows installer. Thanks to Aurelie for testing so thoroughly and reporting the issues behind most of these fixes.

- **Text formatting:** font size, letter spacing, line spacing and paragraph spacing accept typed and dragged values, for one or many objects, with inline editing open or closed. Text keeps its fonts, colours, sizes, underline, strikethrough and super/subscript while you edit it. Justify really justifies.
- **Fonts:** fonts installed on your computer show in the editor and in a bigger, searchable font picker. Fonts packed in a PowerPoint file are imported. Publishing embeds the fonts a course uses (fonts whose licence forbids embedding are listed instead).
- **Gradients:** any number of colours, reorderable, with hard edges where two colours meet, everywhere: canvas, preview, thumbnails and the published course.
- **Animations and timeline:** exit animations play with a slide transition; exit effects are named as exits (Fly out, Fade out…); dragging the playhead shows the slide at that moment; the timeline plays to the end of the slide after a video ends (the video holds its last frame); the timeline scroll bar no longer covers the end of a bar.
- **Shapes and lines:** the drag preview shows the real shape; "Fill" and "Outline" are ticked switches; dashes and dots have adjustable length and gap; pattern fills.
- **PowerPoint import:** grouped shapes, pictures and cropped pictures, picture fills, theme colours, slide backgrounds, master and layout content, bullets and numbering, text margins, no-wrap text, exact line spacing, 4:3 and portrait slides, custom shapes, tables and video. A built-in comparison test checks every slide of a reference deck against PowerPoint's own pictures.
- **Editing:** Insert ▸ Interaction and other dialogs open full size every time; Paint copies a format (Esc or a click on empty canvas cancels); the preview closes with a click outside or Esc, and editor shortcuts no longer change the course behind it; the question editor is also in the right panel; a layer can show the base slide normally, dimmed or hidden; deleting a slide keeps your place in the list; the Spacebar no longer starts the timeline while you type.
- **Player:** at the end of a slide the Pause button shows Play and replays the slide, instead of a second Replay button.
- **Video:** selecting a video no longer freezes or crashes the app on Windows; a video that cannot be decoded shows a placeholder.
- **Windows:** a real installer (Start menu entry and uninstall), alongside the portable `.exe`.
- **Also fixed:** Find and Replace with built-in variables, Version History for `course.json` projects, inserting library templates with links or similar media names, headless MCP save keeping media, "Start over" playback speed on the web, and the web editor saving unused media.

## 0.2.5

Fixes from a full code review, faster everywhere, and new features: resume on the web, desktop playback speed, a shared template library, and open, save and publish in the web editor.

- **No more lost work:** saving through an AI agent (MCP or the control server) keeps a project's media; Ctrl+S on a plain `course.json` writes JSON instead of breaking the file; folder projects keep media whose names differ only by upper and lower case.
- **No more crashes:** fixed crashes from a ring progress meter with a zoom animation, pasting a copy bigger than the slide, dragging the end of a timeline bar near the end, very tall scroll panels, a freeze on a huge slide duration, and a damaged `.ccraft` that asks for huge memory. Slide durations and object sizes now have sensible limits.
- **Windows:** videos whose video track is not the first stream now open.
- **Import and editing:** PowerPoint and Word import keep `&`, `<` and `>`; Find and Replace keeps subscript, strikethrough, spacing and font; replacing text near a `%` no longer breaks a `%variable%`; shortcuts like Ctrl+Z act on your typing while you type; objects on question-bank slides can be deleted; duplicating slides with scroll panels or drag targets works; clicking a path point no longer moves it.
- **Player:** Retry Quiz, Review Quiz and Exit Review close an open lightbox; a resumed course runs the leave steps of the slide it resumed on; a warning when a timer is longer than its slide.
- **LMS and LTI:** a pass is no longer overwritten by a later fail; the LTI server refuses cross-site score posts, keeps working through platform key changes, and ties logins to the browser.
- **Faster:** fewer repeated checks per edit, a lighter runtime, less copying when saving, opening and autosaving, and the LTI server caches files and supports video seeking in Safari.
- **New:** courses published to Web remember where the learner stopped (with a "Start over" option); the desktop preview has playback speed; long documents for AI are read in parts instead of being cut; save templates to "My library" and use them in any course; the web editor can open, save and publish.

## 0.2.4

Everyday speed and fewer clicks.

- **Faster editor:** big courses stay smooth; the accessibility check runs once you pause; saving and publishing use much less memory.
- **Rename in place:** double-click a slide title or a timeline object name (or press F2). Timeline durations show in seconds.
- **Publish loop:** Publish remembers your last settings, finishes instantly, and has "Open in browser" to preview the result.
- **Triggers:** new triggers start on the selected object; double-click a trigger to edit it; Esc cancels and Cmd+Enter saves; filter and group the list.
- **Canvas:** Alt-drag duplicates, Shift-drag keeps one axis, Space-drag pans, and Cmd+B/I/U work while typing. Set animation, outline, opacity and font on many objects at once. Fixed: changing the fill of several buttons no longer removes their gradient.
- **Smaller web player:** about 30% smaller (3.3 MB gzipped), starts faster, and loads the next slide's pictures early. The browser console stays quiet unless the address has `?debug=1`.
- **Small fixes:** drop a `.ccraft` file to open it; one-click Hover / Selected / Visited / Disabled states; Enter adds a variable; double-click a media row to insert it; a note when an SVG has text; a note when old undo steps are dropped; `coursecraft-cli simulate --strict`, plainer `validate` errors, and `publish` finds the built player on its own.
- **Checked by two stress-test passes:** fixed Undo after Publish (remembering the publish standard no longer adds an Undo step), Cmd+B with only a caret, the state preset buttons shifting under the mouse, a rename box left open after deleting its slide, the preview server waiting on a stuck browser, and the command line picking a half-built player folder.

## 0.2.3

Share courses on any website, start from a template, give learners a second try, branch on variables, and time slides to narration. Also a fix so published courses load in SCORM Cloud.

- **Publish to Web:** File ▸ Publish has a "Web (any website, no LMS)" option (and `--standard web` in the command line). Upload the folder to any web server; results are not reported to an LMS.
- **Starter templates:** the welcome screen offers Blank, Quick Knowledge Check, Linear Lesson and Branching Scenario.
- **Lock and hide are saved:** locked and hidden objects, guides, rulers and the grid stay as you left them when you reopen a course. Hidden objects are hidden only in the editor; learners still see them.
- **Try Again layer:** a question with more than one attempt can show a Try Again layer after a wrong answer; its button clears the answer so the learner can try again. Add it with one click in the question panel.
- **Safe resume:** coming back to a question you already answered shows its feedback layer again, so its Continue button is always there.
- **Variables compared to variables:** set a variable from another variable, or compare two variables in a condition. Text checks can ignore upper and lower case.
- **More jumps:** jump to the last slide the learner visited, or to the first slide of a scene.
- **Waveforms and cue points:** audio on the timeline shows its waveform. Press C to drop a named cue point at the playhead; objects snap to cues, and a trigger can fire at a cue, so moving the cue retimes the trigger.

**Fixes**
- Published courses load in LMSs that protect course files with sign-in cookies, such as SCORM Cloud. Before, the player showed "failed to fetch course.json: HTTP 403".

## 0.2.2

Clearer everyday editing: named undo steps with a History panel, equal-spacing guides, problems marked on triggers, and live variables in Preview.

- **Undo history:** Undo and Redo in the Edit menu name the step ("Undo Set Fill Style"), and Edit ▸ History… lists every step; click one to go straight back to it.
- **Equal-spacing guides:** while you drag an object, guides show when its gap to a neighbour matches the other gaps in the row or column, with the gap in pixels, and it snaps into place.
- **Problems on triggers:** the Triggers tab marks a trigger that can't work (for example, it uses an object you deleted) with a warning and the reason, and says how many need attention.
- **Live variables in Preview:** a Variables panel (F3) in the preview shows every variable's value as it changes, so you can see why a branch went one way.

**Fixes**
- Opening "Correct when" on a Freeform question no longer crashes the editor.
- A click that closes a menu over the slide list or the course map no longer also switches the slide.
- Cmd+Shift+Z (Ctrl+Shift+Z on Windows and Linux) redoes; it used to undo.
- Insert ▸ Question ▸ Freeform works when the course's first variable is true/false or text.

## 0.2.1

A new sample course that shows off every feature, and the fixes we found by building it.

- **Healthy Habits at Work:** a new sample course opens behind the welcome screen on a fresh launch and is first under Explore an example. Its 36 slides use every feature: narration with objects timed to the spoken words (also inside layers), a video with cue points, a Lottie animation, all 12 question types, a question bank with a random draw, every ready-made interaction, layers, states, a slide template, a custom font, background music and a certificate.
- **Animations work again:** what you pick in the Animations panel is now saved, so entrances and exits play, and the timeline's Play button shows them on the slide.
- **Audio and video play by themselves:** media you place starts with the slide, like narration, and a slide shorter than its sound or video grows to fit. A long sound starts from the beginning on the next Play, and scrubbing back works.
- **Faster typing:** typing in long text with spell check on is fast again (it took most of a second per key in 0.2.0). The editor also stops redrawing while idle, and the slide list draws only the thumbnails you can see.
- **Quick preview:** F12 previews the current slide and Shift+F12 previews from the start.
- **Scenes:** each scene in the slide list has a + button for a new slide in that scene, and New Scene starts with a slide of its own.

**Fixes**
- Deleting an object that has triggers works: its triggers go with it, the status bar says how many, and one Undo brings everything back. Deleting a layer also removes the triggers that show it.
- Speaker notes typed in the Slide tab are saved.
- Saving keeps the course's background music and any media used only by question-bank slides.
- The player's outline drawer no longer draws scrolled rows over its header and tabs, and its rows ignore clicks while it slides in. On the web, the screen-reader layer no longer drifts away from the rows it describes.
- Choosing a value that is already set no longer adds an Undo step that does nothing.
- Flip cards have readable text on both sides with any theme colour, and process-step labels are centred under their circles.
- A click on a menu or popup no longer also clicks what lies underneath. Picking "Radial gradient" from the Fill type menu in a smaller window used to select the slide layer under the menu too: the layer's content appeared over the slide and the panel jumped to the Slide tab. A click that closes a menu now only closes it, and the same fix covers the canvas, rulers and timeline.
- Picture fill: choosing Picture when the course has no pictures opens the file picker, and the Image menu always offers "Choose a file…". The chosen picture fills the shape or button; it is no longer placed as a separate picture. The menu lists only pictures, not audio or video, and cancelling the file picker leaves the fill as it was.
- Gradient colours are labelled Colour 1, Colour 2 and so on, with "Add colour", and each row fits inside the panel (its position and remove button used to run off the edge).
- Radio group options fit inside the Format panel. One option row was wider than the panel and pushed every field below it past the edge.
- The timeline ruler labels every 1, 2, 5 or 10 seconds as space allows, so labels on a long slide no longer overlap.
- The colour field for an object state is wide enough to show the whole colour code.

## 0.2.0

Everyday authoring: fonts, pictures, colours, spelling and ready-made interactions.

- **Your own fonts:** use any font installed on your computer. It is copied into the course so it publishes with it, with a note when the font's maker restricts embedding.
- **Paste pictures:** Cmd/Ctrl+V pastes a screenshot straight onto the slide. Copy and paste now also works with other apps.
- **Eyedropper:** pick any colour from the screen in every colour picker.
- **Spell check:** misspelled words are underlined as you type, with suggestions on right-click. Edit ▸ Check Spelling (F7) checks the whole course, and words you add are remembered.
- **Interactions gallery:** Insert ▸ Interaction shows ready-made, editable interactions, including six new ones: flip cards, hotspot reveal, process steps, timeline, reveal grid and scenario choice.

## 0.1.2

- CourseCraft has an app icon: a "C" around a play button, in the ArtCraft family style. It shows in the Mac Dock and Finder, on the Windows .exe and taskbar, and on the Linux window.

## 0.1.1

The first release with downloads. The v0.1.0 tag never finished building, so its fixes are included here too.

**Cleaner layout**
- Dialogs open centred below the toolbar, stay on screen, and scroll when the window is small, with their action buttons always visible.
- Form fields in dialogs line up, and labels in the properties panel show in full.
- Fixed controls running past the properties panel's edge, timeline names running into their icons, and other overlaps and margins.
- An automatic check now catches overlapping or cut-off text, off-screen dialogs and clipped buttons at three window sizes, in dark and light.

**Builds**
- The Mac download is built on a current macOS build machine.

## 0.1.0 (first public release)

The first public release. CourseCraft is early: it is tested with automated tests and local mock servers, not yet certified against commercial LMSs. Try a package in your LMS before relying on it.

**Build courses**
- A visual editor for slides, scenes, layers, states, triggers, variables, timelines and animation, on macOS (Apple Silicon), Windows and Linux (less tested), and in the browser.
- Text, shapes, images, audio, video, web objects, timers, sliders, dials, scrolling panels, markers, progress meters, Lottie animations and motion paths.
- Slide masters, layouts, templates, format painter, slide size, named text styles and a theme palette.
- PowerPoint import, Word and PDF review exports, MP4 export, and XLIFF translation.
- Quizzes: twelve question types, question banks, results slides, retry and review. Questions can be imported from a CSV file.

**Learner player**
- Navigation modes, playback speed, full screen, captions, transcripts, glossary and resources, and keyboard and screen-reader use.

**Publish**
- SCORM 1.2, SCORM 2004 4th edition, cmi5, xAPI and LTI 1.3, with validation and accessibility checks first.

**Your files**
- Safe saving, version history, and project folders that work with git.

**Automation and AI**
- Every editing action is a command you can run from the command line, scripts, batch jobs or an AI agent (MCP). Optional AI that you bring yourself: off by default, never required.

**Feedback**
- Help → Report a Problem opens a pre-filled GitHub issue in your browser. Nothing is sent automatically.
