<img src="apps/coursecraft/icons/coursecraft-1024.png" alt="" width="96" align="right">

# CourseCraft

An open-source, clean-room alternative to Articulate Storyline 360, written in pure Rust. You build interactive eLearning with slides, layers, states, triggers, variables and quizzes, and publish it to any LMS.

> **Status: early.** You can build a course in the visual editor, from text, shapes, images, audio, video, triggers, variables, layers and quizzes, and publish it for SCORM 1.2, SCORM 2004, cmi5, xAPI or LTI 1.3. It's tested against local mock servers. It has not yet been certified against commercial LMSs, so try a package in yours before relying on it.

## Screenshots

<p align="center"><img src="docs/screenshots/tour.gif" alt="A tour of CourseCraft: the editor, themes, interactions, triggers, quizzes, the learner player and publishing" width="900"></p>

The sample course "Healthy Habits at Work" was built entirely with `coursecraft-cli` commands; its pictures were made with AI. Tap or click any picture below to see it full size.

<details>
<summary><b>All screenshots (18)</b></summary>
<br>
<table>
<tr><td width="50%" align="center"><a href="docs/screenshots/editor.jpg"><img src="docs/screenshots/editor.jpg" alt="The editor"></a><br><sub>The editor</sub></td><td width="50%" align="center"><a href="docs/screenshots/editor-dark-light.jpg"><img src="docs/screenshots/editor-dark-light.jpg" alt="Dark and light themes"></a><br><sub>Dark and light themes</sub></td></tr>
<tr><td width="50%" align="center"><a href="docs/screenshots/design-light.jpg"><img src="docs/screenshots/design-light.jpg" alt="Designing a slide"></a><br><sub>Designing a slide</sub></td><td width="50%" align="center"><a href="docs/screenshots/interactions-gallery.jpg"><img src="docs/screenshots/interactions-gallery.jpg" alt="Ready-made interactions"></a><br><sub>Ready-made interactions</sub></td></tr>
<tr><td width="50%" align="center"><a href="docs/screenshots/markers-dark-light.jpg"><img src="docs/screenshots/markers-dark-light.jpg" alt="Markers on a picture"></a><br><sub>Markers on a picture</sub></td><td width="50%" align="center"><a href="docs/screenshots/triggers.jpg"><img src="docs/screenshots/triggers.jpg" alt="Triggers, without code"></a><br><sub>Triggers, without code</sub></td></tr>
<tr><td width="50%" align="center"><a href="docs/screenshots/quiz-editor.jpg"><img src="docs/screenshots/quiz-editor.jpg" alt="Question editor"></a><br><sub>Question editor</sub></td><td width="50%" align="center"><a href="docs/screenshots/animate.jpg"><img src="docs/screenshots/animate.jpg" alt="Animation"></a><br><sub>Animation</sub></td></tr>
<tr><td width="50%" align="center"><a href="docs/screenshots/publish.jpg"><img src="docs/screenshots/publish.jpg" alt="Publish to any LMS"></a><br><sub>Publish to any LMS</sub></td><td width="50%" align="center"><a href="docs/screenshots/cards-dark-light.jpg"><img src="docs/screenshots/cards-dark-light.jpg" alt="Cards, dark and light"></a><br><sub>Cards, dark and light</sub></td></tr>
<tr><td width="50%" align="center"><a href="docs/screenshots/triggers-dark-light.jpg"><img src="docs/screenshots/triggers-dark-light.jpg" alt="Triggers, dark and light"></a><br><sub>Triggers, dark and light</sub></td><td width="50%" align="center"><a href="docs/screenshots/steps-dark-light.jpg"><img src="docs/screenshots/steps-dark-light.jpg" alt="Process steps, dark and light"></a><br><sub>Process steps, dark and light</sub></td></tr>
<tr><td width="50%" align="center"><a href="docs/screenshots/player-title.jpg"><img src="docs/screenshots/player-title.jpg" alt="Learner player"></a><br><sub>Learner player</sub></td><td width="50%" align="center"><a href="docs/screenshots/player-flip-cards.jpg"><img src="docs/screenshots/player-flip-cards.jpg" alt="Flip cards"></a><br><sub>Flip cards</sub></td></tr>
<tr><td width="50%" align="center"><a href="docs/screenshots/player-quiz.jpg"><img src="docs/screenshots/player-quiz.jpg" alt="A quiz question"></a><br><sub>A quiz question</sub></td><td width="50%" align="center"><a href="docs/screenshots/player-menu.jpg"><img src="docs/screenshots/player-menu.jpg" alt="Slide menu"></a><br><sub>Slide menu</sub></td></tr>
<tr><td width="50%" align="center"><a href="docs/screenshots/player-finish.jpg"><img src="docs/screenshots/player-finish.jpg" alt="Finish slide"></a><br><sub>Finish slide</sub></td><td width="50%" align="center"><a href="docs/screenshots/slide-size-dark-light.jpg"><img src="docs/screenshots/slide-size-dark-light.jpg" alt="Slide size, dark and light"></a><br><sub>Slide size, dark and light</sub></td></tr>
</table>
</details>

## Get CourseCraft

Download the latest version from the **Releases** page of this repository: a `.dmg` for macOS (Apple Silicon and Intel), a Windows installer (`-setup.exe`) or portable `-portable.zip` (no install, good for trying it out), or an `.AppImage` or `.tar.gz` for Linux (x86-64; built automatically but less tested, so reports are welcome). It's free, with no account and no subscription.

The first time you open it:
- **macOS** says it can't check the app, because it isn't signed with a paid Apple developer account. Right-click the app and choose Open, or go to System Settings → Privacy & Security → Open Anyway.
- **Windows** SmartScreen may warn about an unknown publisher. Choose More info → Run anyway.
- **Linux:** make the `.AppImage` executable (`chmod +x coursecraft-*.AppImage`) and run it, or unpack the `.tar.gz` archive and run `bin/coursecraft` in its folder. Both need GTK 3, which most desktops already have.

CourseCraft checks for a newer version when it starts (turn that off in Preferences), and Help → Check for Updates checks on demand. On macOS (`.app`) and Linux (`.AppImage` or unpacked `.tar.gz`), where your copy can update itself, it offers Update and Restart after verifying the release's `SHA256SUMS.txt`; on Windows it offers the `-setup.exe` installer for you to download and run, or the new `-portable.zip` if you use the portable copy. Nothing installs or restarts without your confirmation. What changed in each version is in [CHANGELOG.md](CHANGELOG.md).

### Install on Windows

- **Installer (`coursecraft-<version>-windows-x64-setup.exe`):** Installs per user into `%LOCALAPPDATA%\Programs\CourseCraft` without administrator rights, adds a Start menu shortcut (and an optional desktop shortcut), and registers an uninstaller in Windows **Settings → Apps**.
- **Portable (`coursecraft-<version>-windows-x64-portable.zip`):** Unzip anywhere (a USB stick works) and run `coursecraft.exe`; nothing is installed (settings still live in `%APPDATA%\CourseCraft`, below). To update, unpack the new zip in place of the folder. To remove it, delete the folder.
- **Settings and logs:** Preferences, saved project version history, and crash notes (`last-crash.txt`) are stored in `%APPDATA%\CourseCraft`.

Built to the conventions of the [ArtCraft](https://github.com/storytold) Crafting Apps, and on their open-source `craft-ui`, `craft-i18n` and `craft-update` libraries (the same ones DeckCraft uses): native egui/wgpu, the same code on the web via WebAssembly, every action a command, and no crashes.

## What it does

- **A ribbon editor with panels you arrange:** commands grouped in ribbon tabs, with extra tabs for what you select, and Slides, Timeline, Triggers, States, the inspectors and more as panels you can move, tab, split, float or close. Your layout is saved, and every label is ready for translation.
- **A visual editor** on the desktop and in the browser: multi-select, align and distribute, smart guides, grouping, copy and paste, slide masters, layouts and templates, find and replace, image crop, a timeline, and PowerPoint import.
- **Interactive courses:** layers, triggers, variables, animation, and twelve question types with feedback, question banks and results slides.
- **Animation and transitions:** 25 entrance and exit effects, played as one, by paragraph or letter by letter, 12 emphasis effects layered on top, and 15 slide transitions, each previewed live while you pick it. The Animation Painter copies animations from one object to others.
- **Text formatting:** fonts, highlight colour, text shadow, Change Case, bullets and numbering styles, text direction, theme heading and body fonts, symbols, and variables inserted right into the text.
- **Text and buttons that behave:** click to place a label that fits its text and grows as you type, or drag a box that wraps. Group a shape, icon and text and make the group act as one button, with one hover look, one click, one Tab stop and one screen-reader name.
- **Rich object states:** hover, pressed, selected, visited and your own states, each a full second look for an object (picture, text, fill, outline, shadow, glow, rotation and items of its own), all side by side in the States tab and edited right on the canvas.
- **Any slide size:** Full HD, HD, 4:3, square or portrait, or your own; resize a whole course with a live preview of how content fits.
- **A calm, consistent editor** in dark and light, checked screen by screen against the same UI rules, and a learner player with a seekbar you can lock and a Play button before a web course starts.
- **Publish from the editor** (desktop) to SCORM 1.2, SCORM 2004 4th edition, cmi5, xAPI or LTI 1.3, with validation and accessibility checks first.
- **Teams and automation:** keep a project as a folder in git, see changes in plain words with `coursecraft-cli diff`, run one script over many courses, and let an AI agent drive the editor through the same commands you use. See [docs/cli.md](docs/cli.md) and [docs/git.md](docs/git.md).
- **Accessible output:** keyboard use, screen reader support, captions, and translation through XLIFF.
- **Optional AI, and you bring your own.** It's off by default and CourseCraft works fully without it. Point it at a free model on your own computer, such as Ollama or LM Studio, or at a hosted provider you pay for yourself. It can turn a document into a course, suggest alt text, translate a course, make captions from audio and make narration from text. Narration from text can also use your own ElevenLabs API key and account credits, chosen with **Voice source** in the Narration from Text window; that has its own settings, separate from the AI provider. Browsers may block ElevenLabs, so the desktop app is the sure way to use it. You review everything before it goes in, and it tells you where your content is going before it's sent.

## Build from source

You need [Rust](https://rustup.rs) 1.99 or newer.


```sh
cargo xtask build-player         # puts the player inside the desktop app, so it can publish
cargo run --release -p coursecraft
```

Without `build-player`, the editor still runs, but its Publish button explains that this build has no player. You need [trunk](https://trunkrs.dev) and the `wasm32-unknown-unknown` target for that step and for the web editor:

```sh
cd apps/coursecraft && trunk serve   # the editor in a browser
```

## Drive it without the editor

```sh
cargo run -p coursecraft-cli -- validate examples/hello-course/course.json
cargo run -p coursecraft-cli -- simulate examples/hello-course/course.json examples/hello-course/pass.inputs.json
cargo run -p coursecraft-cli -- publish examples/hello-course/course.json --player apps/coursecraft-player/dist -o out.zip
```

`simulate` plays the course headlessly and prints what the player would do: slides entered, layers shown, states changed, score and pass/fail. The editor preview and published packages run that same runtime.

Play it for real, on the desktop or in the browser:

```sh
cargo run -p coursecraft-player -- examples/hello-course/course.json --log-effects
cd apps/coursecraft-player && trunk serve   # then open http://127.0.0.1:8080
```

`--log-effects` prints the same JSON lines as `simulate`; in the browser they go to the console. `F2` shows them in a side panel. The web build needs `rustup target add wasm32-unknown-unknown` and `cargo install trunk`.

To publish and try a SCORM package in a local mock LMS (records every SCORM call, survives relaunches):

```sh
cargo xtask package-example                       # writes target/hello-course.zip and target/mock-lms/
python3 -m http.server 8090 --directory target/mock-lms   # then open http://127.0.0.1:8090
```

## Your files

- **Saving is safe.** A save writes a new file next to your project, checks that it opens and every part is intact, and only then replaces the old one. If the check fails, your existing file stays as it was. If another app (a sync service such as Dropbox or OneDrive) is using the file, CourseCraft tries again for a couple of seconds before telling you.
- **Changed elsewhere?** If the file changed on disk since you opened or last saved it, CourseCraft asks before overwriting: Overwrite, Save a Copy or Cancel. Autosave never asks and never writes to your project file; it keeps a separate `.autosave.ccraft` beside it.
- **Version history.** Each save first keeps a copy of the file it replaces, in CourseCraft's own settings folder (not next to your project, so sync services do not carry the copies around). Choose File > Version History to see them, open one as a copy, or delete it. Preferences > Versions to keep sets how many are kept per project (default 10; 0 turns it off; at most 100, and at most 5 GB per project).
- **Damaged files.** A project that will not open gives a plain message and is left untouched. If it has kept versions, the message offers Open Latest Version.

## Reporting a problem

Choose Help > Report a Problem to tell us what went wrong, or Help > Suggest a Feature for an idea. There is no server: the report opens as a pre-filled issue on GitHub in your browser, and you read it and submit it yourself. Nothing is sent automatically. You need a free GitHub account; without one, use Copy to Clipboard and send the text another way.

- **What is shared.** Your two answers, plus (if you leave "Include technical details" on) the CourseCraft version, your OS and CPU, whether this is the desktop or web app, the last 20 status messages from this session, and a note about the last crash if there was one. All of it is shown in the window before you send anything.
- **What is never shared.** Course content, file names, file paths, user names, AI keys and settings. Paths and file names in messages are replaced with `<path>` and `<file>`.
- **After a crash.** The desktop app writes a short `last-crash.txt` (time, version, OS, panic message and location, paths removed) to its settings folder. On the next start it asks whether to report it, and deletes the file once you report or dismiss it.
- **Updates.** The desktop app checks GitHub Releases for a newer version at start (which you can turn off in Preferences) or when you choose Help > Check for Updates. Downloads are verified against the release's `SHA256SUMS.txt`. On macOS and Linux it can install the update with Update and Restart, only when you click it and never with unsaved work; on Windows it gives you the installer to run.

## Workspace

| Crate | |
|---|---|
| `coursecraft-scorm`, `coursecraft-xapi` | SCORM and xAPI vocabulary (standalone) |
| `coursecraft-course` | `course.json` model, validator, accessibility audit, outlines |
| `coursecraft-logic` | Player runtime: the one place play-time behaviour lives |
| `coursecraft-media`, `coursecraft-playback` | Decoding and media playback state |
| `coursecraft-render` | Slide compositor and player chrome (egui) |
| `coursecraft-format` | `.ccraft` project files and XLIFF |
| `coursecraft-pptx` | PowerPoint and Word import |
| `coursecraft-ai` | Optional AI: request building and answer checking, with no network of its own |
| `coursecraft-lms` | LMS bridges: effects to `cmi` and xAPI statements, resume, finish |
| `coursecraft-publish` | SCORM, cmi5 and xAPI packaging |
| `coursecraft-engine` | Commands and undo |
| `coursecraft-ui-egui`, `coursecraft-control` | Editor UI and its control channel |
| `coursecraft` | The editor as a desktop app and a web page |
| `coursecraft-player` | Desktop and web player |
| `coursecraft-cli`, `coursecraft-mcp` | Headless tool and MCP server; agents start at [docs/cli.md](docs/cli.md) |

See [docs/architecture.md](docs/architecture.md) and [docs/roadmap.md](docs/roadmap.md). The editor's
layout (a tabbed ribbon grouping commands by task, with planned buttons shown greyed out until
they work) is specified in [docs/ui/](docs/ui/README.md); what CourseCraft can and cannot do yet,
feature by feature, is in [docs/ui/feature-parity.md](docs/ui/feature-parity.md).

## Contributors

Thanks to [LucaFontanot](https://github.com/LucaFontanot), who designed and built the animation system (PR #13), the ribbon editor interface (PR #6), and the player's seekbar settings and web Play gate (PRs #3 and #4).

## Help and contributing

- **Questions and ideas:** the Discussions tab of this repository.
- **Bugs:** Help → Report a Problem in the app, or open an issue.
- **Security problems:** report privately, see [SECURITY.md](SECURITY.md).
- **Code:** read [docs/contributing.md](docs/contributing.md) (and [AGENTS.md](AGENTS.md) if you use an AI agent), then open a pull request. Editor UI work starts at [docs/ui/README.md](docs/ui/README.md). Every pull request runs `cargo xtask ci`, the same checks you can run yourself.
- Be kind: [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).

## License

MIT OR Apache-2.0. CourseCraft is not affiliated with Articulate or Adobe. "Storyline" and "Articulate" are trademarks of Articulate Global; "Captivate" and "Adobe" are trademarks of Adobe. They are used only to describe comparable features.
