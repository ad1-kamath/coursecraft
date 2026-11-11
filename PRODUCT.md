# Product

<!-- impeccable:product-schema 1 -->

## Platform

adaptive

One Rust/egui codebase ships as a native desktop app (macOS, Windows, Linux) and as a browser build (wasm). It draws its own UI on every platform rather than adopting per-OS native widgets, so the design language is CourseCraft's own and consistent across targets; it should still respect platform conventions for shortcuts (Cmd vs Ctrl), menus and file dialogs.

## Users

- **Course authors**, equally:
  - professional instructional designers who know Storyline/Captivate and build courses daily: they want density, speed, keyboard shortcuts and precise control;
  - occasional creators (subject experts, teachers) who make a course now and then: they need guidance, sensible defaults, clear empty states and onboarding.
- **Learners**, who take the published course inside an LMS (SCORM 1.2) or a browser; they never see the editor and should not need instructions.
- **Agents and scripts**, which drive the editor through the command registry, CLI and MCP server (no UI, but every UI action has a command).

## Product Purpose

CourseCraft is an open-source, native, Storyline-comparable eLearning authoring tool written in Rust only. Authors build interactive slide-based courses (layers, states, triggers, variables, media, animations, eight question types) and publish them as SCORM packages that run in any LMS. Success: an author can build and publish a good-looking, working course without fighting the tool, and the learner experience looks modern by default.

## Positioning

Open-source, native and fast (no Electron, no subscription), with one deterministic runtime shared by the editor preview, the published player and headless simulation, and full automation (every edit is a named command; CLI and MCP for agents).

## Operating Context

- Authors work on laptops and desktops, often for hours, side by side with source material (PowerPoint decks, scripts, images). They import PPTX, arrange objects on a 1280×720 slide canvas, wire triggers, preview, then publish a zip to an LMS.
- Learners open the course in an LMS iframe or a browser tab, on desktop and sometimes tablet/phone, possibly with sound off.
- The published player is the same egui renderer compiled to wasm, so player chrome is drawn by CourseCraft, not by HTML/CSS.

## Capabilities and Constraints

- egui/eframe 0.37 (immediate mode); all UI is painted, styled through egui `Style`/`Visuals` and custom painting. No new crates without a deliberate decision; fonts and icons must be bundled assets with OFL/MIT-compatible licences.
- Inter (OFL) is already bundled: regular, bold, italic, bold-italic.
- Golden rules: never crash, everything is a command, one runtime, wasm must keep building.
- Course content colours/fonts come from the course `theme` (accent, background, text, font, player); the editor's own chrome theme is separate from a course's theme.
- Product name in UI is always "CourseCraft".

## Brand Commitments

- Name: CourseCraft. Part of the ArtCraft "Crafting Apps" family conventions (storytold); no logo or palette is fixed yet.
- Clean-room: match Storyline's capabilities, not its look or assets.
- Visual direction (user's choice, 2026-11-01): the category standard played straight, at Keynote's craft level: a slide-first pro editor with a refined dark default (light available) and a clean, minimal, content-first learner player. No novelty theme; quality comes from precision, hierarchy and finish.

## Evidence on Hand

- Example courses: `examples/hello-course`, `examples/showcase` (with media).
- No screenshots, testimonials, users or metrics exist; none may be invented.

## Product Principles

1. The slide is the hero: chrome recedes around the canvas in the editor and around the course in the player.
2. Fast for experts, obvious for newcomers: dense defaults with discoverable help, empty states that teach, shortcuts everywhere.
3. One source of truth: every control maps to an engine command; the UI never invents behaviour.
4. Modern by default: a course published with zero styling effort should look current.

## Accessibility & Inclusion

- Learner player: keyboard operable, visible focus, sufficient contrast (WCAG 2.1 AA target, full audit in M6).
- Editor: readable at default size, both dark and light themes meet AA contrast for text.
