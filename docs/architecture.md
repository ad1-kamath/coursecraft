# Architecture

CourseCraft follows the conventions of the ArtCraft Crafting Apps (PhotoCraft, VectorCraft, EffectCraft …). The goal is that it can join that family later without a rewrite.

## Principles

1. **Rust only.** The desktop editor will be egui/eframe on wgpu. The player and web editor are the same Rust compiled to WebAssembly. No Tauri, Electron or webview shells. The single exception will be the few lines of JavaScript glue wasm-bindgen generates for the LMS bridge. SCORM's API object lives in JavaScript, so this can't be avoided.
2. **One runtime.** `coursecraft-logic` is the player. The editor preview, the CLI `simulate` command and published LMS packages all run it, so preview equals published.
3. **Everything is a command.** Edits go through `coursecraft-engine` by command id, and the UI, CLI, control channel and MCP all dispatch the same way. Commands are atomic, undoable, and refuse to break a valid course.
4. **Never crash.** Non-test code has no `unwrap`, `expect`, `panic!` or `unsafe` (one exception: `coursecraft-video-native` needs `unsafe` to call the OS video frameworks; it exposes only safe functions). Hostile files and params produce errors. The runtime turns run-time surprises into `warning` effects and keeps the learner moving.
5. **Clean-room.** We match what established authoring tools can do. The look, code and assets are our own; we never use another product's code, assets or player files.

## Layers

A crate may depend only on lower layers. `cargo xtask layers` enforces this.

| Layer | Crate | Role |
|---|---|---|
| L0 | `coursecraft-scorm` | SCORM manifest + vocabulary. **Standalone**: depends on no workspace crate, so it can be published |
| L1 | `coursecraft-course` | Document model + validation (pure data) |
| L1 | `coursecraft-video-native` | Video decoding through AVFoundation and Media Foundation. Desktop only, and the one crate allowed `unsafe` |
| L2 | `coursecraft-logic`, `coursecraft-media`, `coursecraft-playback` | Player runtime; image decoding (capped, safe); audio and video playback backends. `coursecraft-media` is **standalone**; `coursecraft-playback` depends only on `coursecraft-video-native` |
| L3 | `coursecraft-render`, `coursecraft-ai`, `coursecraft-format`, `coursecraft-pptx` | Slide compositor & design system (tokens, themes, `render_ui`); OpenAI-compatible AI request/response types (pure, no network); `.ccraft` project zip; PPTX import |
| L4 | `coursecraft-lms`, `coursecraft-publish` | LMS bridge logic (effects → `cmi`, resume); SCORM packaging |
| L4 | `coursecraft-lti` | LTI 1.3 protocol: OIDC login, `id_token` validation, tool JWKS, client assertion and AGS scores (pure, no network) |
| L4 | `coursecraft-export` | MP4, PDF and Word export |
| L5 | `coursecraft-engine` | Session, undo, command registry |
| L6 | `coursecraft-ui-egui`, `coursecraft-control` | Editor UI; control channel (JSON-RPC protocol, TCP server, client) |
| apps | `coursecraft-cli`, `coursecraft-mcp`, `coursecraft-player`, `coursecraft` | Binaries (exempt) |

Below L6, only `coursecraft-render` may use egui (painting and input only). eframe and winit belong to apps. L0–L5 and the player must build for `wasm32-unknown-unknown` (`cargo xtask wasm`, part of `ci`).

## Media decoding

`coursecraft-media` decodes every picture behind fixed caps, and the same code runs on desktop and wasm.

- **Raster images** (PNG, JPEG, WebP, GIF) are size- and byte-capped by [`Limits`] before allocation. Only a GIF's first frame is decoded by `decode_image`.
- **SVG** is parsed and rasterised with `resvg` at the size it is drawn, cached per source and size bucket, and re-rasterised once the drawn size changes by more than 25% (debounced while a resize drag runs). The file is capped at 5 MiB, the tree at 20,000 elements, and output pixels at [`Limits`]. `<image>` references may only be `data:` URLs; paths and URLs are never opened, so the parser cannot read a local file or the network. The build has no SVG text feature enabled, so **text inside an SVG is not drawn** and no system font is ever loaded (SVG text is left unrendered rather than substituted).
- **Animated GIF and WebP** are decoded by `decode_animation`: at most 300 frames and 256 MiB of decoded pixels. A zero or sub-10 ms delay becomes 100 ms (as browsers do), and every delay is clamped to 20..=10000 ms. Animations always loop. The player and preview follow their frame timing; the canvas, thumbnails and offscreen renders show frame 0. An animation over the cap shows its first frame and the editor reports a warning.
- **Lottie** files (`.json`, and `.lottie` zips with `animations/*.json`) are parsed by `velato` under fixed caps: 10 MiB file, bounded JSON depth and element count, 4096×4096 composition, 240 fps, 100,000 frames and 5,000 layers. Parsed compositions are cached per `src`; a rejected file draws a placeholder with the reason. Rendering flattens `kurbo` paths into epaint meshes through the same shape triangulator and stroke bands as native shapes, so canvas, preview, player, offscreen and wasm match. Lottie playback position lives in `coursecraft-logic`; `media_ends` reports a non-looping run, and one frame is capped at 120,000 triangles.
- The renderer caches rasterised SVG textures and uploads animation frames lazily; the published web player uses the same renderer, so it behaves identically.

[`Limits`]: ../crates/media/src/lib.rs

## Data flow at play time

```text
renderer ──Input──▶ Player (logic) ──Effect──▶ renderer (draw, media)
                         │                  └─▶ LMS bridge (score, status, suspend_data)
                         └─ PlayerState (serde) ⇄ cmi.suspend_data
```
