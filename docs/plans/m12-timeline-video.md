# Video on the timeline

Goal: an author sees real video in the editor. Dragging the timeline playhead shows that frame on the canvas, and Play on the timeline plays the video with sound, so placing cue points (M12 step 5) doesn't need a trip to Preview. The desktop preview and desktop player use the same decoder, so video stops being a poster there. This brings forward the "video in the desktop editor and preview" item of M14.

Decision (the user, 2026-11-09): use the decoders built into the operating system, AVFoundation on macOS and Media Foundation on Windows, instead of bundling one. That gives hardware speed and every format the OS plays. It needs FFI, so `coursecraft-video-native` is the one crate exempt from `unsafe_code = "forbid"`. Its public API is safe, each `unsafe` block has a SAFETY comment, and it's the only place OS video calls live.

## Step 1: Decoder crate and desktop playback

`coursecraft-video-native` (L1, desktop only; on other targets `VideoFile::open` returns `Unsupported`):

- `VideoFile::open(path) -> Result<VideoFile, VideoError>` reads the track info: `width`, `height`, `duration_ms`, `has_audio`. Hostile files return an error and never panic.
- `seek(ms)` and `next_frame() -> Result<Option<VideoFrame>>`: sequential decode from a position. `VideoFrame { pts_ms, width, height, rgba: Vec<u8> }`, scaled so the width is at most `max_width` (default 1280), RGBA with alpha 255. macOS: `AVAssetReader` with `AVAssetReaderTrackOutput` (BGRA), a new reader for each seek. Windows: `IMFSourceReader` with `MFVideoFormat_RGB32`, `SetCurrentPosition` to seek, COM initialised (MTA) on the calling thread.
- `frame_at(ms)`: seek and decode forward to the frame shown at `ms`.
- `decode_audio(path) -> Result<Option<Pcm>>`: whole audio track as 16-bit interleaved PCM with its rate and channel count, capped at 60 minutes. Same two APIs.
- Unit tests decode `examples/showcase/media/intro.mp4` on macOS: info, a frame at a known time has the right size and isn't blank, `frame_at` is within one frame of the request, truncated and garbage files return errors. Windows code must pass `cargo check --target x86_64-pc-windows-gnu -p coursecraft-video-native`, and the same tests are `cfg` enabled for it.

`coursecraft-playback` desktop backend:

- Video bytes are written once to a temp file named by content hash. A worker thread per video owns its `VideoFile` and takes Play, Pause, Seek and Stop commands, decoding at real time against a clock and publishing the newest frame through `Arc<Mutex<..>>`. It never blocks the UI thread. The audio track plays through the existing rodio path, and the audio clock is the master clock when there's audio.
- `MediaBackend` gains `video_frame(object) -> Option<Arc<VideoFrame>>` (default `None`). `position_ms` returns the real position and `take_ended` reports the real end. A file that can't be decoded logs a warning once and behaves like today: poster, simulated clock for cues.

Render:

- `Visual::Video` draws the current frame as a texture when one exists, with the poster as the fallback. Frames reach the painter through the same kind of per-context store images use, keyed by object id, with the texture updated in place rather than reallocated each frame.
- The player and preview use it, so desktop preview and player show real video. Real position replaces the simulated cue clock whenever decoding works.

## Step 2: Timeline playback in the editor

- A Play/Pause button and time readout in the timeline header, with Space as the shortcut when no text field has focus.
- Canvas: for each video on the current slide whose timing includes the playhead, draw the frame for `playhead - start_ms`. Scrubbing asks for a frame on the worker and shows the latest result: no queue of stale seeks, never blocking a frame.
- Play advances the playhead in real time, with video and sound, and stops at the end of the timeline. Clicking or dragging the playhead while playing seeks. Editing or leaving the slide stops playback.
- Authoring only: triggers and cues don't fire, and the document is not changed, so nothing goes on the undo stack. Add Cue at playhead uses the frame on screen. The playhead snaps to cue markers within 6 px (Shift disables it).
- Audio objects also sound during timeline Play, so a video and narration can be lined up.
- The web editor keeps its placeholder for now.

## Done bar

Each step: unit tests above; a harness test with a fake frame source for the canvas and timeline (scrub shows the requested frame, Play advances, stops at the end, no undo entries); `cargo xtask ci` green; `cargo check` for the Windows target; played by hand by Claude in the Mac app with a real MP4.
