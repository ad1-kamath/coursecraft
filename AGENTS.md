# AGENTS.md: guide for AI agents and contributors

CourseCraft is an open-source, native, Storyline-comparable eLearning authoring tool written in **Rust only**. The product name is always **CourseCraft** in user-facing text. Machine names are lowercase (`coursecraft-*`).

It is built to the conventions of the ArtCraft Crafting Apps (storytold) so it can be offered to that family later. Read `docs/architecture.md` first.

## Orientation

| Read | Why |
|---|---|
| `docs/architecture.md` | Layers, principles, data flow |
| `docs/spec/course-model.md` | `course.json` schema |
| `docs/spec/triggers.md` | Exact runtime semantics (the contract) |
| `docs/roadmap.md` | Milestones and current focus |
| `docs/cli.md` | CLI and MCP setup for agents, and every command |
| `docs/contributing.md` | Gate, add-a-command checklist, clean-room rules |

## Golden rules

1. **Never crash.** No `unwrap`/`expect`/`panic!`/`unreachable!`/`todo!` in non-test code; `unsafe_code = "forbid"` everywhere except `coursecraft-video-native`, which calls the OS video frameworks. Every crate root carries the `#![deny(clippy::unwrap_used, …)]` block. Input-derived numbers are hostile.
2. **Everything is a command.** Editing behaviour lives in `coursecraft-engine` with an id; UIs only dispatch.
3. **One runtime.** Don't add play-time behaviour anywhere but `coursecraft-logic`.
4. **Layering is enforced:** `cargo xtask layers`. Register new crates in `xtask/src/main.rs`.
5. **Spec first.** A runtime behaviour change updates `docs/spec/triggers.md` in the same change.
6. **Clean-room.** Match capabilities, never copy code, assets or look; mind the patent watch list. See `docs/contributing.md`.
7. **Never break wasm** for L0–L5.

## Drive it headlessly

```sh
cargo run -p coursecraft-cli -- validate examples/hello-course/course.json
cargo run -p coursecraft-cli -- simulate examples/hello-course/course.json examples/hello-course/pass.inputs.json
cargo run -p coursecraft-cli -- run examples/hello-course/course.json examples/hello-course/build.script.json -o out.json
cargo run -p coursecraft-cli -- commands
```

## Recipes

Step-by-step recipes with exact commands: `docs/recipes/build-from-pdf.md`, `docs/recipes/accessibility-fix.md`, `docs/recipes/translate-and-check.md`, `docs/recipes/rebrand-many.md`. Many courses at once: `coursecraft-cli batch`. Questions from a spreadsheet: `coursecraft-cli import-questions`.
