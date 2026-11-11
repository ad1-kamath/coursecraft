# M15 Publishing

Goal: get a course out of CourseCraft in the shapes people ask for besides an LMS package: a video for sharing, a document a reviewer can mark up, and a launch from an LTI 1.3 platform. A new L4 crate, `coursecraft-export`, holds the MP4, PDF and Word writers; it builds for wasm with the MP4 part compiled out. The CLI and the editor both call it. Encoding goes through the operating system, as decoding already does: AVFoundation's `AVAssetWriter` on the Mac and Media Foundation's sink writer on Windows, in `coursecraft-video-native`, so no ffmpeg is bundled and the app stays one binary.

## Step 1: Export as MP4

- `coursecraft-video-native::Mp4Writer`: H.264 video plus AAC audio from RGBA frames and 16-bit PCM, at a fixed frame rate; `create`, `write_frame`, `write_audio`, `finish`. Tested on the Mac by writing a clip and reading it back with `VideoFile`; compiled on Windows.
- `coursecraft-export::mp4`: runs one `Player` through the play order on an export clock, ticking at the frame rate and drawing each frame with `render_player_rgba` (from M13 step 5), so animations, transitions, timeline entrances, states and variables look as they do in the player. Slides end at their timeline's end and the exporter presses Next. Audio and video objects are decoded to PCM through the OS decoder and mixed at the time their `Play` effect fires, honouring trim, volume and stop; video frames come from the decoder at the media clock. Background audio runs under everything. A question slide is drawn at its timeline end without an answer and listed in the export report, along with slides a trigger would have branched from: the export follows the linear path.
- Editor: File ▸ Export ▸ Video (MP4)… with size (720p, 1080p), frame rate (30) and a save dialog; progress in the status bar with Cancel; "Show in folder" when done. CLI `export-mp4 <course> -o <file> [--width] [--fps]`.
- Deferred: the browser editor (no encoder there), an interactive-path recorder, subtitles burned in from captions (VTT sidecar can come later), bitrate controls (a sensible default per size).

## Step 2: Export as PDF or Word for review

- `coursecraft-export::pdf`: one page per slide in play order (bank slides in an appendix), each with the slide drawn at 1600 px wide as a JPEG, a heading "n. Title", the slide notes and the slide's text as plain paragraphs, with an option for each layer as its own page. Hand-written PDF 1.4 with Helvetica, so no new dependency; characters outside WinAnsi print as `?` until embedded fonts arrive.
- `coursecraft-export::docx`: the same content as a Word document (heading, picture, notes, text) written as a zip of OOXML parts with the `zip` crate and hand-written XML, so a reviewer can comment and track changes.
- Editor: File ▸ Export ▸ PDF for review… and Word for review… with checkboxes for layers, notes and text; CLI `export-pdf` and `export-docx`.
- Deferred: embedded fonts and non-Latin text in the PDF, editable shapes in Word, a change-tracked round trip back into the course, a review-comment workflow.

## Step 3: LTI 1.3

LTI needs a tool server; a static package cannot do an OIDC launch or sign a grade. CourseCraft ships one, small and self-hosted, behind the user's own HTTPS reverse proxy.

- `coursecraft-lti` (L4, standalone): the protocol as pure functions. OIDC login redirect, `id_token` validation against the platform's JWKS (RS256 through `ring`), the tool's own JWKS, the `client_credentials` assertion, and the Assignment and Grade Services score payload. Tested with a fixture RSA key; no network.
- `apps/coursecraft-lti`: `coursecraft-lti serve <lti.json> <published-folder>`: a plain HTTP/1.1 server on `std::net` with `httparse`, serving `/.well-known/jwks.json`, `/lti/login`, `/lti/launch`, the published course files and `/lti/score`, which fetches an access token with `ureq` and posts the score to the line item. Configuration is one JSON file; the private key is a PKCS#8 file the user makes with `openssl`.
- Player: a `lti` package standard (marker only) that makes the web player post `score` and `completion` effects to `/lti/score` on the same origin with the launch cookie, and shows the lost-reporting banner when that fails. `publish --standard lti` and the editor's Publish dialog gain the option.
- Dependencies: `ring`, `base64`, `httparse` and `ureq` are already in `Cargo.lock` through other crates, so no download is needed, but they become direct dependencies and the reviewer confirms `cargo metadata --offline` resolves before the pieces run.
- Deferred: Deep Linking, Names and Role Provisioning, multiple platforms per server, TLS inside the server (the proxy does it), a hosted service.

## Done bar

Each step: unit tests for every writer (files read back by our own decoder or parser), CLI tests, UI harness tests for the dialogs, `docs/cli.md` updated, `cargo xtask ci` green including the wasm check, and the output opened by hand after review: the MP4 in QuickTime and a browser, the PDF in Preview, the Word file in Word or Pages, and an LTI launch from the IMS reference platform or a local Moodle.
