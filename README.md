# CourseCraft

An open-source, clean-room alternative to Articulate Storyline 360, written in pure Rust. You build interactive eLearning with slides, layers, states, triggers, variables and quizzes, and publish it to any LMS.

> **Status: early.** You can build a course in the visual editor, from text, shapes, images, audio, video, triggers, variables, layers and quizzes, and publish it for SCORM 1.2, SCORM 2004, cmi5, xAPI or LTI 1.3. It's tested against local mock servers. It has not yet been certified against commercial LMSs, so try a package in yours before relying on it.

## Get CourseCraft

Download the latest version from the **Releases** page of this repository: a `.dmg` for macOS on Apple Silicon, an `.exe` for Windows, or a `.tar.gz` for Linux (x86-64; built automatically but less tested, so reports are welcome). It's free, with no account and no subscription.

The first time you open it:
- **macOS** says it can't check the app, because it isn't signed with a paid Apple developer account. Right-click the app and choose Open, or go to System Settings → Privacy & Security → Open Anyway.
- **Windows** SmartScreen may warn about an unknown publisher. Choose More info → Run anyway.
- **Linux:** unpack the archive and run `./coursecraft`. It needs GTK 3, which most desktops already have.

There is no automatic update. Help → Check for Updates opens the Releases page. What changed in each version is in [CHANGELOG.md](CHANGELOG.md).

Built to the conventions of the [ArtCraft](https://github.com/storytold) Crafting Apps: native egui/wgpu, the same code on the web via WebAssembly, every action a command, and no crashes.

## What it does

- **A visual editor** on the desktop and in the browser: multi-select, align and distribute, smart guides, grouping, copy and paste, slide masters, layouts and templates, find and replace, image crop, a timeline, and PowerPoint import.
- **Interactive courses:** layers, states, triggers, variables, animation, and twelve question types with feedback, question banks and results slides.
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
- Help > Check for Updates only opens the releases page. CourseCraft makes no network calls to look for updates.

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

See [docs/architecture.md](docs/architecture.md) and [docs/roadmap.md](docs/roadmap.md).

## Help and contributing

- **Questions and ideas:** the Discussions tab of this repository.
- **Bugs:** Help → Report a Problem in the app, or open an issue.
- **Security problems:** report privately, see [SECURITY.md](SECURITY.md).
- **Code:** read [docs/contributing.md](docs/contributing.md) (and [AGENTS.md](AGENTS.md) if you use an AI agent), then open a pull request. Every pull request runs `cargo xtask ci`, the same checks you can run yourself.
- Be kind: [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).

## License

MIT OR Apache-2.0. CourseCraft is not affiliated with Articulate or Adobe. "Storyline" and "Articulate" are trademarks of Articulate Global; "Captivate" and "Adobe" are trademarks of Adobe. They are used only to describe comparable features.
