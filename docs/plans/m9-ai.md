# M9 plan: AI-assisted authoring

Goal: optional help that saves an author time. Everything still works without it, nobody needs a subscription, and an author can point it at a free model running on their own machine.

| Step | What | Depends on |
|---|---|---|
| 1 | Build slides from a pasted outline, with no AI | |
| 2 | AI settings and a fake-tested client: an OpenAI-compatible endpoint | |
| 3 | Turn a document into a course | 1, 2 |
| 4 | Suggest alt text for images | 2 |
| 5 | Translate a course, filling the XLIFF | 2 |
| 6 | Captions from audio, and text to speech | 2 |
| 7 | M9 check: played by hand with a mock endpoint, roadmap, docs | all |

## Decisions that hold across the milestone

- **It's bring-your-own and off by default.** No built-in key, no account, no call to anything until an author sets it up. With no setup, the AI menu items explain how to set it up and everything else stays the same.
- **One OpenAI-compatible endpoint:** a base URL, a model and an optional key. That covers hosted providers and local servers such as Ollama and LM Studio, which cost nothing and send nothing off the machine.
- **Both apps use the same path, through `ehttp`.** Browsers block most hosted providers from a page, so in the web editor a blocked call says to use the desktop app or a local server that allows it. There, a key is kept for the session only and never stored.
- **AI output is hostile input.** It goes through the same checks as a loaded file: an outline through `Outline::check` and `course.import_outline`, translations through the XLIFF importer. Nothing the model returns is run, or trusted as a path or an id.
- **An author reviews before anything changes.** Each feature shows what it will add, and accepting it is one undo step.
- **An author sees what's sent, and where,** before the first request of each kind: the endpoint's host and what's going to it. A course's content never leaves without that click.
- **On the desktop the key lives only in the app's prefs,** written with owner-only permissions on Unix. It never appears in `course.json`, a `.ccraft`, a published zip, an XLIFF, a log or an error message. A test holds this.
- **No new crates.** `ehttp` is already in the workspace. The request and response code is pure and has no network in it, so tests use a fake transport and never call a real API.
- **One request at a time, cancellable,** on a background path that never freezes the window, with a timeout and plain errors. A failed call changes nothing.
- **Play doesn't change.** AI only ever produces ordinary course content, through ordinary commands.
