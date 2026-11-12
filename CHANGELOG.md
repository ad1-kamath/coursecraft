# Changelog

What changed in each release, newest first. Versions follow [semantic versioning](https://semver.org): until 1.0, a minor version (0.x) may change things, but a project saved by an older CourseCraft always opens in a newer one.

## Unreleased

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
