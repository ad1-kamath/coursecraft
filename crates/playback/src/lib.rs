//! CourseCraft media playback (L2).
//!
//! Plays the audio and video the runtime asks for through `media` effects:
//! rodio on the desktop, HTML media elements in the browser. The runtime
//! decides; this crate only plays and reports when media ends.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

use std::sync::Arc;

#[cfg(not(target_arch = "wasm32"))]
pub mod record;

/// One decoded video frame, RGBA with alpha 255.
#[derive(Debug, Clone)]
pub struct VideoFrame {
    /// Presentation time in milliseconds.
    pub pts_ms: u64,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width * height * 4` bytes, RGBA, row-major.
    pub rgba: Vec<u8>,
}

/// Whole audio track as 16-bit interleaved PCM, decoded on the worker.
#[derive(Debug, Clone)]
pub struct AudioPcm {
    /// Sample rate in Hz.
    pub sample_rate: u32,
    /// Channel count.
    pub channels: u16,
    /// Interleaved signed 16-bit samples.
    pub samples: Vec<i16>,
}

/// Abstract video frame decoder owned by a worker thread.
pub trait VideoDecoder: Send {
    /// Duration in milliseconds.
    fn duration_ms(&self) -> u64;
    /// Seek to `ms`.
    fn seek(&mut self, ms: u64) -> Result<(), String>;
    /// Next frame from the current position, or `None` at end of stream.
    fn next_frame(&mut self) -> Result<Option<VideoFrame>, String>;
    /// Whether the file has an audio track.
    fn has_audio(&self) -> bool {
        false
    }
    /// Decode the audio track as PCM. Called on the worker thread, never the
    /// UI thread. Default is no audio (`None`).
    fn decode_audio(&mut self) -> Option<AudioPcm> {
        let _ = self;
        None
    }
}

#[cfg(not(target_arch = "wasm32"))]
struct RealDecoder {
    file: coursecraft_video_native::VideoFile,
}

#[cfg(not(target_arch = "wasm32"))]
impl VideoDecoder for RealDecoder {
    fn duration_ms(&self) -> u64 {
        self.file.duration_ms()
    }
    fn seek(&mut self, ms: u64) -> Result<(), String> {
        self.file.seek(ms).map_err(|e| e.to_string())
    }
    fn next_frame(&mut self) -> Result<Option<VideoFrame>, String> {
        self.file
            .next_frame()
            .map_err(|e| e.to_string())
            .map(|opt| {
                opt.map(|f| VideoFrame {
                    pts_ms: f.pts_ms,
                    width: f.width,
                    height: f.height,
                    rgba: f.rgba,
                })
            })
    }
    fn has_audio(&self) -> bool {
        self.file.has_audio()
    }
    // No `decode_audio` override: the real audio track is decoded on its
    // own dedicated thread in `RodioBackend::start_video` (path-based, so
    // it needs no `VideoFile`), and published through `WorkerState::audio`.
    // Returning the trait default (`None`) here keeps the video worker from
    // ever blocking on audio decoding, so frames keep playing while audio
    // is still decoding. Decoders that can publish PCM synchronously
    // (in-memory fakes in tests) override `decode_audio` instead.
}

/// Clock a video worker paces frames against.
///
/// The real worker uses [`SystemClock`]; tests use a fake clock whose
/// `sleep_ms` advances simulated time instantly, so 30 simulated seconds
/// run without waiting.
pub trait VideoClock: Send + Sync {
    /// Current clock time in milliseconds.
    fn now_ms(&self) -> u64;
    /// Wait `ms` milliseconds (real or simulated).
    fn sleep_ms(&self, ms: u64);
    /// Called when the worker publishes a frame (test seam; default no-op).
    fn on_publish(&self, _pts_ms: u64) {}
}

/// Monotonic system clock used by the real worker.
#[derive(Debug)]
pub struct SystemClock {
    start: std::time::Instant,
}

impl SystemClock {
    /// Create a clock anchored now.
    #[must_use]
    pub fn new() -> Self {
        Self {
            start: std::time::Instant::now(),
        }
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl VideoClock for SystemClock {
    fn now_ms(&self) -> u64 {
        u64::try_from(self.start.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    fn sleep_ms(&self, ms: u64) {
        std::thread::sleep(std::time::Duration::from_millis(ms));
    }
}

/// Commands to a video worker thread.
#[derive(Debug, Clone)]
enum WorkerCmd {
    Play,
    Pause,
    Seek(u64),
    Stop,
}

/// Shared video worker state, published through a mutex (never blocks UI).
#[derive(Debug, Default)]
pub struct WorkerState {
    /// Newest published frame.
    pub frame: Option<Arc<VideoFrame>>,
    /// Real playback position in milliseconds, when playing.
    pub position_ms: Option<u64>,
    /// Whether the worker is playing.
    pub playing: bool,
    /// Whether the stream ended (once).
    pub ended: bool,
    /// Whether decoding is active (real video, not fallback).
    pub decoding: bool,
    /// Whether the file failed to open (fallback / ended immediately).
    pub failed: bool,
    /// Decoded audio track as PCM, published by the worker once ready.
    pub audio: Option<Arc<AudioPcm>>,
    /// Master-clock position in milliseconds, published by the backend
    /// from the audio sink while it is playing. `None` until the sink
    /// exists and is playing (or when there is no audio), in which case
    /// the worker paces itself with its own monotonic clock.
    pub audio_position_ms: Option<u64>,
}

struct WorkerHandle {
    cmd_tx: std::sync::mpsc::Sender<WorkerCmd>,
    shared: Arc<std::sync::Mutex<WorkerState>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for WorkerHandle {
    fn drop(&mut self) {
        let _ = self.cmd_tx.send(WorkerCmd::Stop);
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

/// Outcome of trying to publish one frame after a paused seek.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FrameOutcome {
    /// A frame was published.
    Published,
    /// No frame yet, but one may still arrive (the reader is warming up):
    /// the caller retries.
    Retry,
    /// End of stream: no frame will ever arrive for this seek. The caller
    /// stops retrying and keeps the previous frame on screen.
    Done,
}

/// Decode one frame after a seek while paused for the timeline scrub.
///
/// A seek while playing is followed by the regular decode loop, so no
/// extra work is needed there. While paused the worker would otherwise
/// only reposition the decoder and the editor would keep showing the
/// previous frame forever. Decoding a single frame lets the canvas
/// show the requested position while still holding the last frame if
/// the stream has nothing to give. Never marks the stream ended: a
/// paused scrub to the end must not read as a finished player track.
fn publish_one_frame<D: VideoDecoder>(
    decoder: &mut D,
    shared: &Arc<std::sync::Mutex<WorkerState>>,
    duration: u64,
    position_ms: &mut u64,
) -> FrameOutcome {
    match decoder.next_frame() {
        Ok(Some(frame)) => {
            *position_ms = frame.pts_ms.min(duration);
            if let Ok(mut state) = shared.lock() {
                state.frame = Some(Arc::new(frame));
                state.position_ms = Some(*position_ms);
            }
            FrameOutcome::Published
        }
        // `None` is end of stream (see `VideoDecoder::next_frame`): at or
        // past the end the decoder never produces a frame, so retrying
        // would spin forever. Stop and keep the previous frame.
        Ok(None) => FrameOutcome::Done,
        // An error (e.g. the reader has not produced its first sample
        // yet) is not end of stream: retry later.
        Err(_) => FrameOutcome::Retry,
    }
}

/// Publish the last frame of the stream (scan from about 2 s before the
/// end, keeping the final frame; fall back to 0 only if that finds
/// nothing). Used when a paused seek lands past the last frame: the seek
/// itself produces nothing, but the canvas must show the last frame.
/// Seeking near the end avoids decoding the whole file on long media.
/// Returns whether a frame was published.
fn publish_last_frame<D: VideoDecoder>(
    decoder: &mut D,
    shared: &Arc<std::sync::Mutex<WorkerState>>,
    duration: u64,
    position_ms: &mut u64,
) -> bool {
    // Scan from about 2 s before the end first (or 0 for short media).
    let near_end = duration.saturating_sub(2000);
    let mut last = scan_last_frame(decoder, near_end);
    if last.is_none() && near_end > 0 {
        last = scan_last_frame(decoder, 0);
    }
    if let Some(frame) = last {
        *position_ms = frame.pts_ms.min(duration);
        if let Ok(mut state) = shared.lock() {
            state.frame = Some(Arc::new(frame));
            state.position_ms = Some(*position_ms);
        }
        true
    } else {
        false
    }
}

/// Decode from `seek_ms` to the end, keeping the final frame.
fn scan_last_frame<D: VideoDecoder>(decoder: &mut D, seek_ms: u64) -> Option<VideoFrame> {
    let _ = decoder.seek(seek_ms);
    let mut last: Option<VideoFrame> = None;
    loop {
        match decoder.next_frame() {
            Ok(Some(frame)) => last = Some(frame),
            Ok(None) => break,
            Err(_) => break,
        }
    }
    last
}

/// Clamp a seek target so it lands on a real frame: seeking to exactly
/// the duration lands past the last frame, so step back 1 ms.
fn seek_target(ms: u64, duration: u64) -> u64 {
    if ms >= duration && duration > 0 {
        duration.saturating_sub(1)
    } else {
        ms
    }
}

/// How to pace one decoded frame against the master clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pace {
    /// The frame is due: publish it now.
    Publish,
    /// The frame is due in `ms`: wait (in short sleeps) until it is due.
    Wait(u64),
    /// The frame is more than one frame late: drop it and decode the next
    /// without publishing, so video catches up to the master clock.
    Drop,
}

/// Decide what to do with a frame due at `due_ms` when the master clock
/// reads `now_ms` and frames are `interval_ms` apart.
fn pace_frame(due_ms: u64, now_ms: u64, interval_ms: u64) -> Pace {
    let interval = interval_ms.max(1);
    if due_ms > now_ms {
        Pace::Wait(due_ms - now_ms)
    } else if now_ms - due_ms > interval {
        Pace::Drop
    } else {
        Pace::Publish
    }
}

/// Decode and pace one frame while playing. Returns `false` when the
/// stream ended (the caller stops playing).
#[allow(clippy::too_many_arguments)]
fn play_one_frame<D: VideoDecoder, C: VideoClock>(
    decoder: &mut D,
    shared: &Arc<std::sync::Mutex<WorkerState>>,
    clock: &C,
    cmd_rx: &std::sync::mpsc::Receiver<WorkerCmd>,
    pending: &mut std::collections::VecDeque<WorkerCmd>,
    duration: u64,
    position_ms: &mut u64,
    last_pts: &mut Option<u64>,
    anchor_master_ms: u64,
    anchor_clock_ms: u64,
) -> bool {
    match decoder.next_frame() {
        Ok(Some(frame)) => {
            let pts = frame.pts_ms;
            let interval = last_pts
                .map(|prev| pts.saturating_sub(prev))
                .filter(|gap| *gap > 0)
                .unwrap_or(33);
            *last_pts = Some(pts);
            let now = master_now(shared, clock, anchor_master_ms, anchor_clock_ms);
            match pace_frame(pts, now, interval) {
                Pace::Publish => publish_frame(shared, clock, frame, duration, position_ms),
                Pace::Drop => {
                    // More than one frame late: drop without publishing
                    // so video catches up to the master clock.
                    *position_ms = pts.min(duration);
                }
                Pace::Wait(wait_ms) => {
                    // Wait until the frame is due, in short sleeps so
                    // commands stay responsive.
                    let mut remaining = wait_ms;
                    while remaining > 0 {
                        let chunk = remaining.min(10);
                        clock.sleep_ms(chunk);
                        if let Ok(cmd) = cmd_rx.try_recv() {
                            pending.push_back(cmd);
                            break;
                        }
                        // The master may have jumped (audio sink):
                        // recompute what is left of the wait.
                        let now = master_now(shared, clock, anchor_master_ms, anchor_clock_ms);
                        remaining = pts.saturating_sub(now);
                    }
                    // Re-check lateness after waiting: a slow decode may
                    // have made this frame late.
                    let now = master_now(shared, clock, anchor_master_ms, anchor_clock_ms);
                    if pace_frame(pts, now, interval) == Pace::Publish {
                        publish_frame(shared, clock, frame, duration, position_ms);
                    } else {
                        *position_ms = pts.min(duration);
                    }
                }
            }
            true
        }
        Ok(None) => {
            if let Ok(mut state) = shared.lock() {
                state.ended = true;
                state.playing = false;
                state.position_ms = Some(duration);
            }
            false
        }
        Err(_) => {
            if let Ok(mut state) = shared.lock() {
                state.ended = true;
                state.playing = false;
            }
            false
        }
    }
}

/// Publish a decoded frame to the shared state.
fn publish_frame<C: VideoClock>(
    shared: &Arc<std::sync::Mutex<WorkerState>>,
    clock: &C,
    frame: VideoFrame,
    duration: u64,
    position_ms: &mut u64,
) {
    let pts = frame.pts_ms;
    *position_ms = pts.min(duration);
    if let Ok(mut state) = shared.lock() {
        state.frame = Some(Arc::new(frame));
        state.position_ms = Some(*position_ms);
    }
    clock.on_publish(pts);
}

/// Master-clock position right now: the audio sink when it is running,
/// else the anchored monotonic clock.
fn master_now<C: VideoClock>(
    shared: &Arc<std::sync::Mutex<WorkerState>>,
    clock: &C,
    anchor_master_ms: u64,
    anchor_clock_ms: u64,
) -> u64 {
    if let Ok(state) = shared.lock()
        && let Some(audio_pos) = state.audio_position_ms
    {
        return audio_pos;
    }
    anchor_master_ms.saturating_add(clock.now_ms().saturating_sub(anchor_clock_ms))
}

pub(crate) fn run_video_worker<D: VideoDecoder, C: VideoClock>(
    mut decoder: D,
    cmd_rx: std::sync::mpsc::Receiver<WorkerCmd>,
    shared: Arc<std::sync::Mutex<WorkerState>>,
    clock: C,
) {
    let duration = decoder.duration_ms();
    let mut position_ms: u64 = 0;
    let mut playing = false;
    // A paused seek whose frame has not arrived yet. The reader may need
    // a moment to produce its first sample, so the frame is retried each
    // pass until it lands, the stream ends, or a newer command replaces it.
    let mut paused_seek: Option<u64> = None;
    // Commands received while pacing a frame, handled on the next pass.
    let mut pending: std::collections::VecDeque<WorkerCmd> = std::collections::VecDeque::new();
    // Monotonic-clock anchor: the master position when the clock read
    // `anchor_clock_ms`. Re-anchored on Play, resume and seek so the worker
    // never drifts from real time. When an audio sink is running its
    // published position is the master instead (see `audio_position_ms`).
    let mut anchor_master_ms: u64 = 0;
    let mut anchor_clock_ms: u64 = clock.now_ms();
    // Previous frame's pts, for estimating the frame interval.
    let mut last_pts: Option<u64> = None;
    // Publish PCM for decoders that can decode synchronously (in-memory
    // fakes in tests). The real decoder returns `None` here because its
    // audio is decoded on a dedicated thread in
    // `RodioBackend::start_video`, never on this video worker, so frames
    // must not wait for `decode_audio`. Without audio, the worker's own
    // frame clock drives playback.
    if decoder.has_audio()
        && let Some(pcm) = decoder.decode_audio()
        && let Ok(mut state) = shared.lock()
    {
        state.audio = Some(Arc::new(pcm));
    }
    loop {
        // Drain commands, keeping only the newest seek/state.
        let mut pending_seek: Option<u64> = None;
        let mut pending_play: Option<bool> = None;
        let mut stop = false;
        while let Some(cmd) = pending.pop_front().or_else(|| cmd_rx.try_recv().ok()) {
            match cmd {
                WorkerCmd::Play => pending_play = Some(true),
                WorkerCmd::Pause => pending_play = Some(false),
                WorkerCmd::Seek(ms) => pending_seek = Some(ms),
                WorkerCmd::Stop => stop = true,
            }
        }
        if stop {
            if let Ok(mut state) = shared.lock() {
                state.frame.take();
                state.audio.take();
                state.audio_position_ms = None;
                state.position_ms = None;
                state.playing = false;
            }
            return;
        }
        if let Some(want) = pending_play {
            if want && !playing {
                playing = true;
                paused_seek = None;
                anchor_master_ms = position_ms;
                anchor_clock_ms = clock.now_ms();
                if let Ok(mut state) = shared.lock() {
                    state.playing = true;
                    state.ended = false;
                    state.decoding = true;
                }
            } else if !want && playing {
                playing = false;
                if let Ok(mut state) = shared.lock() {
                    state.playing = false;
                    state.position_ms = Some(position_ms);
                }
            }
        }
        let drained_seek = pending_seek;
        if let Some(ms) = pending_seek {
            let ms = seek_target(ms, duration);
            let _ = decoder.seek(ms);
            position_ms = ms;
            last_pts = None;
            if playing {
                anchor_master_ms = ms;
                anchor_clock_ms = clock.now_ms();
            }
            if let Ok(mut state) = shared.lock() {
                state.position_ms = Some(position_ms);
            }
        }
        if !playing && drained_seek.is_some() {
            match publish_one_frame(&mut decoder, &shared, duration, &mut position_ms) {
                FrameOutcome::Published => paused_seek = None,
                FrameOutcome::Done => {
                    // Past the last frame: show the last frame instead of
                    // nothing, and stop retrying.
                    publish_last_frame(&mut decoder, &shared, duration, &mut position_ms);
                    paused_seek = None;
                }
                FrameOutcome::Retry => paused_seek = drained_seek,
            }
        }
        if playing {
            playing = play_one_frame(
                &mut decoder,
                &shared,
                &clock,
                &cmd_rx,
                &mut pending,
                duration,
                &mut position_ms,
                &mut last_pts,
                anchor_master_ms,
                anchor_clock_ms,
            );
        } else {
            // Block briefly waiting for commands (seek-while-playing coalescing
            // happens on the next drain).
            if let Ok(cmd) = cmd_rx.recv_timeout(std::time::Duration::from_millis(10))
                && paused_command(
                    &mut decoder,
                    &shared,
                    duration,
                    &mut position_ms,
                    &mut playing,
                    &mut paused_seek,
                    &mut anchor_master_ms,
                    &mut anchor_clock_ms,
                    &clock,
                    &mut pending,
                    cmd,
                )
            {
                return;
            }
            // A paused seek whose frame has not landed yet: retry until
            // the reader produces it or the stream ends. The 10 ms command
            // wait above paces this, so an idle worker does not spin.
            if !playing && paused_seek.is_some() {
                match publish_one_frame(&mut decoder, &shared, duration, &mut position_ms) {
                    FrameOutcome::Published => paused_seek = None,
                    FrameOutcome::Done => {
                        publish_last_frame(&mut decoder, &shared, duration, &mut position_ms);
                        paused_seek = None;
                    }
                    FrameOutcome::Retry => {}
                }
            }
        }
    }
}

/// Handle one command received while the worker is paused. Returns
/// `true` when the worker must stop.
#[allow(clippy::too_many_arguments)]
fn paused_command<D: VideoDecoder, C: VideoClock>(
    decoder: &mut D,
    shared: &Arc<std::sync::Mutex<WorkerState>>,
    duration: u64,
    position_ms: &mut u64,
    playing: &mut bool,
    paused_seek: &mut Option<u64>,
    anchor_master_ms: &mut u64,
    anchor_clock_ms: &mut u64,
    clock: &C,
    _pending: &mut std::collections::VecDeque<WorkerCmd>,
    cmd: WorkerCmd,
) -> bool {
    match cmd {
        WorkerCmd::Play => {
            *paused_seek = None;
            *anchor_master_ms = *position_ms;
            *anchor_clock_ms = clock.now_ms();
            if let Ok(mut state) = shared.lock() {
                state.playing = true;
                state.decoding = true;
            }
            *playing = true;
            false
        }
        WorkerCmd::Pause => {
            *playing = false;
            if let Ok(mut state) = shared.lock() {
                state.playing = false;
                state.position_ms = Some(*position_ms);
            }
            false
        }
        WorkerCmd::Seek(ms) => {
            let ms = seek_target(ms, duration);
            let _ = decoder.seek(ms);
            *position_ms = ms;
            if let Ok(mut state) = shared.lock() {
                state.position_ms = Some(*position_ms);
            }
            if !*playing {
                match publish_one_frame(decoder, shared, duration, position_ms) {
                    FrameOutcome::Published => *paused_seek = None,
                    FrameOutcome::Done => {
                        publish_last_frame(decoder, shared, duration, position_ms);
                        *paused_seek = None;
                    }
                    FrameOutcome::Retry => *paused_seek = Some(ms),
                }
            }
            false
        }
        WorkerCmd::Stop => {
            if let Ok(mut state) = shared.lock() {
                state.frame.take();
                state.audio.take();
                state.audio_position_ms = None;
                state.position_ms = None;
                state.playing = false;
            }
            true
        }
    }
}

/// FNV-1a 64-bit content hash for temp-file naming.
fn content_hash(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &byte in bytes.iter().take(1_000_000) {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash ^ (bytes.len() as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
}

/// Write video bytes once to a temp file named by content hash.
fn write_video_bytes(bytes: &[u8]) -> Option<std::path::PathBuf> {
    if bytes.is_empty() || bytes.len() > 512 * 1024 * 1024 {
        return None;
    }
    // Hash the whole file length plus a sample for speed on huge files, but
    // intro-sized files hash fully above via the len mix (take is capped).
    let hash = content_hash(bytes);
    let path = std::env::temp_dir().join(format!("coursecraft-video-{hash:016x}.mp4"));
    if path.is_file()
        && let Ok(metadata) = std::fs::metadata(&path)
        && metadata.len() == bytes.len() as u64
    {
        return Some(path);
    }
    std::fs::write(&path, bytes).ok()?;
    Some(path)
}

/// Kind of media asset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaKind {
    Audio,
    Video,
}

/// Abstract media playback backend.
pub trait MediaBackend {
    /// Play or resume media playback.
    /// Bytes for src come from the app's MediaSource; `kind` is Audio or Video.
    fn play(&mut self, object: &str, src: &str, kind: MediaKind, bytes: Arc<[u8]>);
    /// Pause playback of the given media object.
    fn pause(&mut self, object: &str);
    /// Stop playback of the given media object and reset its position.
    fn stop(&mut self, object: &str);
    /// Stop all active media playback and reset.
    fn stop_all(&mut self);
    /// Move playback of the given media object to `ms`. Seeking past the end
    /// ends the media normally.
    fn seek(&mut self, object: &str, ms: u64);
    /// Set player volume (`0.0`–`1.0`) for current and later media. NaN becomes
    /// `0.0`; infinities and out-of-range values are clamped.
    fn set_volume(&mut self, volume: f32);
    /// Set one object's own volume (`0.0`–`1.0`). The effective level of the
    /// object is the player volume times this object volume. NaN becomes `0.0`;
    /// infinities and out-of-range values are clamped. Default is a no-op.
    fn set_object_volume(&mut self, _object: &str, _volume: f32) {}
    /// Set the playback rate (`0.5`–`2.0`) for current and later media. The
    /// default does nothing; only backends that return `true` from
    /// [`MediaBackend::supports_rate`] need to act on it.
    fn set_rate(&mut self, _rate: f32) {}
    /// Whether this backend can really change the playback rate. The player
    /// hides its speed control when this is `false`, so the slide clock never
    /// runs faster than the narration.
    fn supports_rate(&self) -> bool {
        false
    }
    /// Objects whose media finished since the last call (the app turns them into Input::MediaEnded).
    fn take_ended(&mut self) -> Vec<String>;
    /// Current playback position of the given media object in milliseconds, or
    /// `None` when it is not playing.
    fn position_ms(&self, object: &str) -> Option<u64>;
    /// Wasm video only: position/size/visibility of each video's element in CSS px; desktop no-op.
    fn layout_video(&mut self, object: &str, rect: Option<[f32; 4]>);
    /// Wasm video only: show or clear the caption text drawn over one video.
    /// The default does nothing, which is right for audio and for desktop.
    fn show_caption(&mut self, _object: &str, _text: Option<&str>) {}
    /// Desktop video: newest decoded frame for `object`, if any. Never blocks.
    /// Default is `None` (poster / wasm).
    fn video_frame(&self, _object: &str) -> Option<Arc<VideoFrame>> {
        let _ = _object;
        None
    }
    /// Desktop video: whether `object` is decoding real video (as opposed to
    /// a simulated-clock fallback when decoding fails). Default is `false`.
    fn is_video_decoding(&self, _object: &str) -> bool {
        let _ = _object;
        false
    }
    /// Whether this backend has reported that no audio output device is
    /// available. Default is `false`. The desktop backend sets it once
    /// opening the default audio stream has failed.
    fn no_device(&self) -> bool {
        false
    }
}

/// Clamp a player volume into `0.0..=1.0`, turning NaN into silence.
#[must_use]
pub fn sanitize_volume(volume: f32) -> f32 {
    if volume.is_nan() {
        0.0
    } else {
        volume.clamp(0.0, 1.0)
    }
}

// ---------------------------------------------------------------- NullBackend

#[derive(Debug, Default)]
struct NullBackendInner {
    calls: Vec<String>,
    ended: Vec<String>,
    playing: std::collections::HashSet<String>,
    paused: std::collections::HashSet<String>,
    positions: std::collections::HashMap<String, u64>,
    volume: f32,
    object_volumes: std::collections::HashMap<String, f32>,
}

/// Null media playback backend for headless operation and unit tests.
#[derive(Debug, Clone)]
pub struct NullBackend {
    inner: Arc<std::sync::Mutex<NullBackendInner>>,
}

impl Default for NullBackend {
    fn default() -> Self {
        Self {
            inner: Arc::new(std::sync::Mutex::new(NullBackendInner {
                volume: 1.0,
                ..NullBackendInner::default()
            })),
        }
    }
}

impl NullBackend {
    /// Create a new empty [`NullBackend`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Mark an object's media as ended, to be returned by [`take_ended`].
    pub fn mark_ended(&self, object: &str) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.playing.remove(object);
            inner.paused.remove(object);
            inner.positions.remove(object);
            inner.ended.push(object.to_string());
        }
    }

    /// Set the reported playback position for an object, for tests.
    pub fn set_position_ms(&self, object: &str, position_ms: u64) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.positions.insert(object.to_string(), position_ms);
        }
    }

    /// Return all recorded call signatures.
    #[must_use]
    pub fn recorded_calls(&self) -> Vec<String> {
        self.inner
            .lock()
            .map(|i| i.calls.clone())
            .unwrap_or_default()
    }

    /// Check if the given object is currently in the playing state.
    #[must_use]
    pub fn is_playing(&self, object: &str) -> bool {
        self.inner
            .lock()
            .map(|i| i.playing.contains(object))
            .unwrap_or(false)
    }

    /// Check if the given object is currently in the paused state.
    #[must_use]
    pub fn is_paused(&self, object: &str) -> bool {
        self.inner
            .lock()
            .map(|i| i.paused.contains(object))
            .unwrap_or(false)
    }

    /// Last volume set on this backend.
    #[must_use]
    pub fn volume(&self) -> f32 {
        self.inner.lock().map(|i| i.volume).unwrap_or(1.0)
    }

    /// Last volume set for `object`, if any.
    #[must_use]
    pub fn object_volume(&self, object: &str) -> Option<f32> {
        self.inner.lock().ok()?.object_volumes.get(object).copied()
    }
}

impl MediaBackend for NullBackend {
    fn play(&mut self, object: &str, src: &str, _kind: MediaKind, _bytes: Arc<[u8]>) {
        if let Ok(mut inner) = self.inner.lock() {
            if inner.paused.remove(object) {
                inner.calls.push(format!("resume:{object}:{src}"));
            } else {
                inner.calls.push(format!("play:{object}:{src}"));
            }
            inner.playing.insert(object.to_string());
            inner.positions.entry(object.to_string()).or_insert(0);
        }
    }

    fn pause(&mut self, object: &str) {
        if let Ok(mut inner) = self.inner.lock()
            && inner.playing.remove(object)
        {
            inner.paused.insert(object.to_string());
            inner.calls.push(format!("pause:{object}"));
        }
    }

    fn stop(&mut self, object: &str) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.playing.remove(object);
            inner.paused.remove(object);
            inner.positions.remove(object);
            inner.calls.push(format!("stop:{object}"));
        }
    }

    fn stop_all(&mut self) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.playing.clear();
            inner.paused.clear();
            inner.positions.clear();
            inner.calls.push("stop_all".to_string());
        }
    }

    fn seek(&mut self, object: &str, ms: u64) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.calls.push(format!("seek:{object}:{ms}"));
            inner.positions.insert(object.to_string(), ms);
        }
    }

    fn set_volume(&mut self, volume: f32) {
        if let Ok(mut inner) = self.inner.lock() {
            let volume = sanitize_volume(volume);
            inner.volume = volume;
            inner.calls.push(format!("volume:{volume}"));
        }
    }

    fn set_object_volume(&mut self, object: &str, volume: f32) {
        if let Ok(mut inner) = self.inner.lock() {
            let volume = sanitize_volume(volume);
            inner.object_volumes.insert(object.to_owned(), volume);
            inner
                .calls
                .push(format!("set_object_volume:{object}:{volume}"));
        }
    }

    fn take_ended(&mut self) -> Vec<String> {
        if let Ok(mut inner) = self.inner.lock() {
            std::mem::take(&mut inner.ended)
        } else {
            Vec::new()
        }
    }

    fn position_ms(&self, object: &str) -> Option<u64> {
        let inner = self.inner.lock().ok()?;
        if inner.playing.contains(object) {
            Some(inner.positions.get(object).copied().unwrap_or(0))
        } else {
            None
        }
    }

    fn layout_video(&mut self, object: &str, rect: Option<[f32; 4]>) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.calls.push(format!("layout_video:{object}:{rect:?}"));
        }
    }
}

// ---------------------------------------------------------------- RodioBackend

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn pcm_to_f32(samples: &[i16]) -> Vec<f32> {
    samples
        .iter()
        .map(|s| {
            let f = f32::from(*s) / 32768.0;
            if f.is_finite() { f } else { 0.0 }
        })
        .collect()
}

#[cfg(not(target_arch = "wasm32"))]
struct RodioTrack {
    sink: rodio::Sink,
    _src: String,
    _bytes: Arc<[u8]>,
    is_paused: bool,
}

/// Desktop media playback backend using `rodio` and OS video decoding.
///
/// Desktop video decodes on a worker thread (see [`WorkerHandle`]); the audio
/// track plays through the rodio sink and is the master clock when present.
#[cfg(not(target_arch = "wasm32"))]
pub struct RodioBackend {
    stream: Option<rodio::OutputStream>,
    no_device_mode: bool,
    warned_no_device: bool,
    warned_video: bool,
    tracks: std::collections::HashMap<String, RodioTrack>,
    videos: std::collections::HashMap<String, WorkerHandle>,
    ended: Vec<String>,
    volume: f32,
    /// Per-object volume, applied on top of the player volume. Missing is 1.0.
    object_volumes: std::collections::HashMap<String, f32>,
}

#[cfg(not(target_arch = "wasm32"))]
impl Default for RodioBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl RodioBackend {
    /// Create a new [`RodioBackend`]. The audio output stream is opened lazily.
    #[must_use]
    pub fn new() -> Self {
        Self {
            stream: None,
            no_device_mode: false,
            warned_no_device: false,
            warned_video: false,
            tracks: std::collections::HashMap::new(),
            videos: std::collections::HashMap::new(),
            ended: Vec::new(),
            volume: 1.0,
            object_volumes: std::collections::HashMap::new(),
        }
    }

    /// Test constructor simulating an environment with no audio device.
    #[must_use]
    pub fn new_without_device() -> Self {
        Self {
            stream: None,
            no_device_mode: true,
            warned_no_device: false,
            warned_video: false,
            tracks: std::collections::HashMap::new(),
            videos: std::collections::HashMap::new(),
            ended: Vec::new(),
            volume: 1.0,
            object_volumes: std::collections::HashMap::new(),
        }
    }

    /// The level applied to `object`'s sink: the player volume times the
    /// object's own volume, both clamped, so a NaN anywhere means silence.
    fn effective_volume(&self, object: &str) -> f32 {
        let object_volume = self.object_volumes.get(object).copied().unwrap_or(1.0);
        sanitize_volume(self.volume) * sanitize_volume(object_volume)
    }

    fn start_video(&mut self, object: &str, src: &str, bytes: Arc<[u8]>) -> bool {
        if self.videos.contains_key(object) {
            return self.resume_video(object);
        }
        let Some(path) = write_video_bytes(&bytes) else {
            log::warn!("video '{object}' ({src}) has no decodable bytes");
            return false;
        };
        // `VideoFile::open` runs on the worker thread, never here: the
        // first open in a process pays AVFoundation/Media Foundation
        // first-use init (seconds), and the UI must not wait for it.
        // Commands sent while the worker is still opening queue in the
        // channel, so a seek that arrives during the open is not lost and
        // the frame appears as soon as the decoder is ready. A failed
        // open publishes `failed` and the backend reports ended from it
        // (see `take_ended`).
        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<WorkerCmd>();
        let shared = Arc::new(std::sync::Mutex::new(WorkerState {
            decoding: true,
            ..WorkerState::default()
        }));
        // The audio track is decoded to PCM on its own dedicated thread
        // (never the UI thread, and never the video worker, so frames keep
        // playing while audio is still decoding); the backend turns that
        // PCM into a rodio SamplesBuffer in poll_video_audio() when it is
        // ready, seeking the sink to the worker's current position, and
        // that sink is the master clock, with seek/pause/stop/volume
        // applied like any audio track. `decode_audio` returns `None`
        // when there is no audio track, so this runs unconditionally.
        let audio_shared = Arc::clone(&shared);
        let audio_path = path.clone();
        // Detached thread: it publishes PCM once ready and then exits.
        // If the video is stopped first, the backend has already
        // removed its worker entry, so poll_video_audio() will not
        // start a sink for the late PCM.
        std::thread::spawn(move || {
            let Ok(Some(pcm)) = coursecraft_video_native::VideoFile::decode_audio(&audio_path)
            else {
                return;
            };
            if let Ok(mut state) = audio_shared.lock() {
                state.audio = Some(Arc::new(AudioPcm {
                    sample_rate: pcm.sample_rate,
                    channels: pcm.channels,
                    samples: pcm.samples,
                }));
            }
        });
        let shared_thread = Arc::clone(&shared);
        let handle = std::thread::spawn(move || {
            let Ok(file) = coursecraft_video_native::VideoFile::open(&path) else {
                if let Ok(mut state) = shared_thread.lock() {
                    state.failed = true;
                    state.decoding = false;
                }
                return;
            };
            run_video_worker(
                RealDecoder { file },
                cmd_rx,
                shared_thread,
                SystemClock::new(),
            );
        });
        self.videos.insert(
            object.to_string(),
            WorkerHandle {
                cmd_tx,
                shared,
                thread: Some(handle),
            },
        );
        self.resume_video(object)
    }

    fn resume_video(&mut self, object: &str) -> bool {
        if !self.videos.contains_key(object) {
            return false;
        }
        if let Some(worker) = self.videos.get(object) {
            let _ = worker.cmd_tx.send(WorkerCmd::Play);
        }
        // Create the audio sink if the worker has published PCM, then
        // resume it if paused.
        self.poll_video_audio(object);
        // Resume audio too if paused.
        if let Some(track) = self.tracks.get_mut(object)
            && track.is_paused
        {
            track.sink.play();
            track.is_paused = false;
        }
        true
    }

    /// Create the rodio sink for a video's decoded PCM once the worker has
    /// published it. The PCM was decoded on the worker thread; here we only
    /// wrap it in a `SamplesBuffer` whose position is the master clock.
    /// Seek, pause, stop and volume then apply to that sink like any track.
    fn poll_video_audio(&mut self, object: &str) {
        if self.tracks.contains_key(object) || self.no_device_mode {
            return;
        }
        if !self.videos.contains_key(object) {
            return;
        }
        let (pcm, playing, position) = {
            let Some(worker) = self.videos.get(object) else {
                return;
            };
            let Ok(state) = worker.shared.lock() else {
                return;
            };
            let Some(pcm) = state.audio.clone() else {
                return;
            };
            (pcm, state.playing, state.position_ms)
        };
        if pcm.sample_rate == 0 || pcm.channels == 0 || pcm.samples.is_empty() {
            return;
        }
        let Some(stream) = self.ensure_stream() else {
            return;
        };
        let sink = rodio::Sink::connect_new(stream.mixer());
        let f32_samples = pcm_to_f32(&pcm.samples);
        let source = rodio::buffer::SamplesBuffer::new(pcm.channels, pcm.sample_rate, f32_samples);
        sink.set_volume(self.effective_volume(object));
        sink.append(source);
        if let Some(pos) = position {
            let _ = sink.try_seek(std::time::Duration::from_millis(pos));
        }
        let is_paused = !playing;
        if is_paused {
            sink.pause();
        }
        self.tracks.insert(
            object.to_string(),
            RodioTrack {
                sink,
                _src: object.to_string(),
                _bytes: Arc::new([]),
                is_paused,
            },
        );
    }

    fn ensure_stream(&mut self) -> Option<&rodio::OutputStream> {
        if self.no_device_mode {
            if !self.warned_no_device {
                log::warn!("no audio output device available; audio playback disabled");
                self.warned_no_device = true;
            }
            return None;
        }

        if self.stream.is_none() {
            match rodio::OutputStreamBuilder::open_default_stream() {
                Ok(s) => self.stream = Some(s),
                Err(e) => {
                    if !self.warned_no_device {
                        log::warn!(
                            "failed to open default audio output stream ({e}); audio disabled"
                        );
                        self.warned_no_device = true;
                    }
                    self.no_device_mode = true;
                    return None;
                }
            }
        }

        self.stream.as_ref()
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl MediaBackend for RodioBackend {
    fn play(&mut self, object: &str, src: &str, kind: MediaKind, bytes: Arc<[u8]>) {
        if kind == MediaKind::Video {
            // Returns true immediately (worker owns decoding); UI never blocks.
            if self.start_video(object, src, bytes) {
                // Audio PCM is decoded on the worker; poll_video_audio()
                // turns it into a rodio sink on later &mut ticks.
            } else if !self.videos.contains_key(object) {
                // Decode failure: warn once (inside start_video) and report
                // ended immediately so triggers don't hang. Cue-bearing
                // videos are handled by the driver fallback, which suppresses
                // this ended while simulating.
                self.ended.push(object.to_string());
            }
            return;
        }

        // Resume if already paused
        if let Some(track) = self.tracks.get_mut(object)
            && track.is_paused
        {
            track.sink.play();
            track.is_paused = false;
            return;
        }

        let Some(stream) = self.ensure_stream() else {
            self.ended.push(object.to_string());
            return;
        };

        let sink = rodio::Sink::connect_new(stream.mixer());
        let cursor = std::io::Cursor::new(Arc::clone(&bytes));
        match rodio::Decoder::try_from(cursor) {
            Ok(decoder) => {
                sink.set_volume(self.effective_volume(object));
                sink.append(decoder);
                self.tracks.insert(
                    object.to_string(),
                    RodioTrack {
                        sink,
                        _src: src.to_string(),
                        _bytes: bytes,
                        is_paused: false,
                    },
                );
            }
            Err(e) => {
                log::warn!("failed to decode audio for object '{object}' ({src}): {e}");
                self.ended.push(object.to_string());
            }
        }
    }

    fn pause(&mut self, object: &str) {
        // Create the audio sink first if PCM is ready so pause applies to it
        // like any audio track.
        self.poll_video_audio(object);
        if let Some(track) = self.tracks.get_mut(object)
            && !track.is_paused
        {
            track.sink.pause();
            track.is_paused = true;
        }
        if let Some(worker) = self.videos.get(object) {
            let _ = worker.cmd_tx.send(WorkerCmd::Pause);
        }
    }

    fn stop(&mut self, object: &str) {
        if let Some(track) = self.tracks.remove(object) {
            track.sink.stop();
        }
        self.videos.remove(object);
    }

    fn stop_all(&mut self) {
        for (_, track) in self.tracks.drain() {
            track.sink.stop();
        }
        self.videos.clear();
        self.ended.clear();
    }

    fn seek(&mut self, object: &str, ms: u64) {
        // Create the audio sink first so seek applies to it as well.
        self.poll_video_audio(object);
        if let Some(worker) = self.videos.get(object) {
            let _ = worker.cmd_tx.send(WorkerCmd::Seek(ms));
        }
        if let Some(track) = self.tracks.get(object) {
            let position = std::time::Duration::from_millis(ms);
            if let Err(error) = track.sink.try_seek(position) {
                log::warn!("seek to {ms} ms failed for object '{object}' ({error})");
                // A format that cannot seek plays from the start instead.
                if track.sink.try_seek(std::time::Duration::ZERO).is_err() {
                    log::warn!("object '{object}' cannot seek; keeping its position");
                }
            }
        }
    }

    fn set_volume(&mut self, volume: f32) {
        self.volume = sanitize_volume(volume);
        for (object, track) in &self.tracks {
            track.sink.set_volume(self.effective_volume(object));
        }
    }

    fn set_object_volume(&mut self, object: &str, volume: f32) {
        self.object_volumes
            .insert(object.to_owned(), sanitize_volume(volume));
        if let Some(track) = self.tracks.get(object) {
            track.sink.set_volume(self.effective_volume(object));
        }
    }

    fn take_ended(&mut self) -> Vec<String> {
        // Turn any newly published PCM into a rodio sink so the audio clock
        // takes over as master clock without needing a separate &mut tick.
        let video_objects: Vec<String> = self.videos.keys().cloned().collect();
        for obj in video_objects {
            self.poll_video_audio(&obj);
        }
        let mut finished = Vec::new();
        for (obj, track) in &self.tracks {
            if !track.is_paused && track.sink.empty() && !self.videos.contains_key(obj) {
                finished.push(obj.clone());
            }
        }
        for obj in &finished {
            self.tracks.remove(obj);
        }
        // Publish each video's audio-sink position to its worker: while a
        // sink is playing it is the master clock the worker paces frames
        // against. When the sink stops playing (paused, done, or not yet
        // created because the PCM arrived late), publish `None` so the
        // worker falls back to its own monotonic clock. A sink that has
        // finished (`empty()`) has a frozen position: publishing it would
        // freeze the picture mid-video when the sound is shorter than the
        // video, so it counts as no master.
        for obj in self.videos.keys() {
            let audio_pos = match self.tracks.get(obj) {
                Some(track) if !track.is_paused && !track.sink.empty() => {
                    u64::try_from(track.sink.get_pos().as_millis()).ok()
                }
                _ => None,
            };
            if let Some(worker) = self.videos.get(obj)
                && let Ok(mut state) = worker.shared.lock()
            {
                state.audio_position_ms = audio_pos;
            }
        }
        // Video workers that reported ended (once), plus workers whose
        // file failed to open on the worker thread: clear and remove
        // their worker so take_ended fires exactly once.
        let video_ended: Vec<(String, bool)> = self
            .videos
            .iter()
            .filter_map(|(obj, worker)| {
                let mut state = worker.shared.lock().ok()?;
                if state.ended {
                    state.ended = false;
                    Some((obj.clone(), false))
                } else if state.failed {
                    state.failed = false;
                    Some((obj.clone(), true))
                } else {
                    None
                }
            })
            .collect();
        for (obj, failed) in &video_ended {
            if *failed && !self.warned_video {
                log::warn!("video '{obj}' could not be decoded; showing poster (first failure)");
                self.warned_video = true;
            } else if *failed {
                log::warn!("video '{obj}' could not be decoded");
            }
        }
        for (obj, _) in &video_ended {
            self.videos.remove(obj);
            if let Some(track) = self.tracks.remove(obj) {
                track.sink.stop();
            }
        }
        let mut res = std::mem::take(&mut self.ended);
        res.extend(finished);
        for (obj, _) in video_ended {
            if !res.contains(&obj) {
                res.push(obj);
            }
        }
        res
    }

    fn position_ms(&self, object: &str) -> Option<u64> {
        // Audio is the master clock when present. A finished sink
        // (`empty()`) has a frozen position, so fall through to the
        // worker's own position instead of reporting the stuck value.
        if let Some(worker) = self.videos.get(object) {
            if let Some(track) = self.tracks.get(object)
                && !track.is_paused
                && !track.sink.empty()
            {
                let pos = u64::try_from(track.sink.get_pos().as_millis()).ok();
                // Keep the worker's master clock fresh even between
                // `take_ended` calls.
                if let Ok(mut state) = worker.shared.lock() {
                    state.audio_position_ms = pos;
                }
                return pos;
            }
            let state = worker.shared.lock().ok()?;
            return state.position_ms;
        }
        let track = self.tracks.get(object)?;
        if track.is_paused {
            return None;
        }
        u64::try_from(track.sink.get_pos().as_millis()).ok()
    }

    fn layout_video(&mut self, _object: &str, _rect: Option<[f32; 4]>) {}

    fn video_frame(&self, object: &str) -> Option<Arc<VideoFrame>> {
        let worker = self.videos.get(object)?;
        let state = worker.shared.lock().ok()?;
        state.frame.as_ref().map(Arc::clone)
    }

    fn is_video_decoding(&self, object: &str) -> bool {
        let Some(worker) = self.videos.get(object) else {
            return false;
        };
        worker.shared.lock().is_ok_and(|state| state.decoding)
    }

    fn no_device(&self) -> bool {
        self.no_device_mode
    }
}

// ---------------------------------------------------------------- WebBackend

/// Ended-queue entry: the object and the generation of the track that produced it.
/// Generation 0 means "unconditional" (the play never got as far as a live track).
#[cfg(target_arch = "wasm32")]
type EndedQueue = Arc<std::sync::Mutex<Vec<(String, u64)>>>;

/// Object id to the generation of its current track.
#[cfg(target_arch = "wasm32")]
type LiveTracks = Arc<std::sync::Mutex<std::collections::HashMap<String, u64>>>;

#[cfg(target_arch = "wasm32")]
type PlayClosure = wasm_bindgen::closure::Closure<dyn FnMut(wasm_bindgen::JsValue)>;

#[cfg(target_arch = "wasm32")]
struct WebTrack {
    element: web_sys::HtmlMediaElement,
    blob_url: String,
    is_video: bool,
    generation: u64,
    /// Wasm video only: the caption box drawn over the bottom of the video.
    caption: Option<web_sys::HtmlElement>,
    caption_visible: bool,
    ended_closure: wasm_bindgen::closure::Closure<dyn FnMut(web_sys::Event)>,
    /// Rejection handlers of this track's `play()` promises. A promise that is
    /// still pending may call one later, so they live as long as the track.
    play_closures: Vec<PlayClosure>,
}

#[cfg(target_arch = "wasm32")]
impl Drop for WebTrack {
    fn drop(&mut self) {
        use wasm_bindgen::JsCast as _;
        let _ = self.element.pause();
        let _ = self.element.remove_event_listener_with_callback(
            "ended",
            self.ended_closure.as_ref().unchecked_ref(),
        );
        if self.is_video
            && let Some(parent) = self.element.parent_node()
        {
            let _ = parent.remove_child(&self.element);
        }
        if let Some(caption) = &self.caption
            && let Some(parent) = caption.parent_node()
        {
            let _ = parent.remove_child(caption);
        }
        let _ = web_sys::Url::revoke_object_url(&self.blob_url);
        // Pausing rejects a pending play() with AbortError, which calls the
        // handler after this drop. Dropping a closure that JS can still call
        // throws, so leak the few bytes instead; the handler is generation
        // guarded and ignores the stale rejection.
        for closure in self.play_closures.drain(..) {
            closure.forget();
        }
    }
}

/// Web media playback backend using `web-sys` HTML media elements.
#[cfg(target_arch = "wasm32")]
pub struct WebBackend {
    tracks: std::collections::HashMap<String, WebTrack>,
    ended_queue: EndedQueue,
    live: LiveTracks,
    next_generation: u64,
    volume: f32,
    /// Per-object volume, applied on top of the player volume. Missing is 1.0.
    object_volumes: std::collections::HashMap<String, f32>,
    /// Playback rate applied to every element.
    rate: f64,
}

#[cfg(target_arch = "wasm32")]
impl Default for WebBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(target_arch = "wasm32")]
impl WebBackend {
    /// Create a new [`WebBackend`].
    #[must_use]
    pub fn new() -> Self {
        Self {
            tracks: std::collections::HashMap::new(),
            ended_queue: Arc::new(std::sync::Mutex::new(Vec::new())),
            live: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            next_generation: 1,
            volume: 1.0,
            object_volumes: std::collections::HashMap::new(),
            rate: 1.0,
        }
    }

    /// The level applied to `object`'s element: the player volume times the
    /// object's own volume, both clamped, so a NaN anywhere means silence.
    fn effective_volume(&self, object: &str) -> f32 {
        let object_volume = self.object_volumes.get(object).copied().unwrap_or(1.0);
        sanitize_volume(self.volume) * sanitize_volume(object_volume)
    }

    fn make_blob_url(bytes: &[u8], src: &str) -> Result<String, wasm_bindgen::JsValue> {
        let array = js_sys::Array::new();
        let uint8 = js_sys::Uint8Array::from(bytes);
        array.push(&uint8);

        let mime = if src.ends_with(".mp3") {
            "audio/mpeg"
        } else if src.ends_with(".wav") {
            "audio/wav"
        } else if src.ends_with(".ogg") {
            "audio/ogg"
        } else if src.ends_with(".m4a") {
            "audio/mp4"
        } else if src.ends_with(".mp4") {
            "video/mp4"
        } else if src.ends_with(".webm") {
            "video/webm"
        } else {
            "application/octet-stream"
        };

        let bag = web_sys::BlobPropertyBag::new();
        bag.set_type(mime);
        let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&array, &bag)?;
        web_sys::Url::create_object_url_with_blob(&blob)
    }

    fn closure_as_function<T: ?Sized>(
        closure: &wasm_bindgen::closure::Closure<T>,
    ) -> &js_sys::Function {
        use wasm_bindgen::JsCast as _;
        let val: &wasm_bindgen::JsValue = closure.as_ref();
        val.unchecked_ref()
    }

    fn push_ended(&self, object: &str, generation: u64) {
        if let Ok(mut q) = self.ended_queue.lock() {
            q.push((object.to_string(), generation));
        }
    }

    /// Handler for a rejected `play()` promise. Only a play the browser refuses
    /// (autoplay policy, unsupported media) counts as ended; `AbortError` just
    /// means a pause or removal interrupted the play.
    fn reject_handler(&self, object: &str, kind: &str, generation: u64) -> PlayClosure {
        let queue = Arc::clone(&self.ended_queue);
        let obj = object.to_string();
        let kind = kind.to_string();
        wasm_bindgen::closure::Closure::<dyn FnMut(wasm_bindgen::JsValue)>::new(
            move |err: wasm_bindgen::JsValue| {
                let name = js_sys::Reflect::get(&err, &wasm_bindgen::JsValue::from_str("name"))
                    .ok()
                    .and_then(|v| v.as_string())
                    .unwrap_or_default();
                if name == "NotAllowedError" || name == "NotSupportedError" {
                    log::warn!("{kind} play() rejected for '{obj}' ({name}): {err:?}");
                    if let Ok(mut q) = queue.lock() {
                        q.push((obj.clone(), generation));
                    }
                } else {
                    log::info!("{kind} play() for '{obj}' interrupted or failed ({name}); ignored");
                }
            },
        )
    }

    /// Start `element` and return the rejection handler, or `Err` when `play()`
    /// threw synchronously.
    fn start_element(
        &self,
        element: &web_sys::HtmlMediaElement,
        object: &str,
        kind: &str,
        generation: u64,
    ) -> Result<PlayClosure, ()> {
        match element.play() {
            Ok(promise) => {
                let on_err = self.reject_handler(object, kind, generation);
                let _ = promise.catch(&on_err);
                Ok(on_err)
            }
            Err(e) => {
                log::warn!("{kind} play() error for object '{object}': {e:?}");
                Err(())
            }
        }
    }

    /// Create the element, event listener and track for a first play.
    fn new_track(
        &self,
        object: &str,
        src: &str,
        kind: MediaKind,
        bytes: &[u8],
        generation: u64,
    ) -> Option<WebTrack> {
        use wasm_bindgen::JsCast as _;

        let is_video = kind == MediaKind::Video;
        let label = if is_video { "video" } else { "audio" };
        let blob_url = match Self::make_blob_url(bytes, src) {
            Ok(url) => url,
            Err(e) => {
                log::warn!("failed to create blob URL for {label} '{object}': {e:?}");
                self.push_ended(object, 0);
                return None;
            }
        };

        let element = if is_video {
            let created = web_sys::window()
                .and_then(|w| w.document())
                .and_then(|d| d.create_element("video").ok())
                .and_then(|e| e.dyn_into::<web_sys::HtmlMediaElement>().ok());
            let Some(video) = created else {
                log::warn!("failed to create video element for '{object}'");
                let _ = web_sys::Url::revoke_object_url(&blob_url);
                self.push_ended(object, 0);
                return None;
            };
            video.set_src(&blob_url);
            let _ = video.set_attribute("playsinline", "");
            let style = video.style();
            let _ = style.set_property("position", "absolute");
            let _ = style.set_property("z-index", "10");
            let _ = style.set_property("display", "none");
            let _ = style.set_property("pointer-events", "none");
            if let Some(body) = web_sys::window()
                .and_then(|w| w.document())
                .and_then(|d| d.body())
            {
                let _ = body.append_child(&video);
            }
            video
        } else {
            match web_sys::HtmlAudioElement::new_with_src(&blob_url) {
                Ok(a) => a.unchecked_into::<web_sys::HtmlMediaElement>(),
                Err(e) => {
                    log::warn!("failed to create HtmlAudioElement for '{object}': {e:?}");
                    let _ = web_sys::Url::revoke_object_url(&blob_url);
                    self.push_ended(object, 0);
                    return None;
                }
            }
        };

        element.set_volume(f64::from(self.effective_volume(object)));
        element.set_playback_rate(self.rate);

        let queue = Arc::clone(&self.ended_queue);
        let obj = object.to_string();
        let ended_closure = wasm_bindgen::closure::Closure::<dyn FnMut(web_sys::Event)>::new(
            move |_e: web_sys::Event| {
                if let Ok(mut q) = queue.lock() {
                    q.push((obj.clone(), generation));
                }
            },
        );
        let _ = element
            .add_event_listener_with_callback("ended", Self::closure_as_function(&ended_closure));

        let caption = if is_video {
            Self::create_caption_element(object)
        } else {
            None
        };

        // From here the track owns the element and blob URL: dropping it on an
        // error path removes the listener and element and revokes the URL.
        Some(WebTrack {
            element,
            blob_url,
            is_video,
            generation,
            caption,
            caption_visible: false,
            ended_closure,
            play_closures: Vec::new(),
        })
    }

    /// Create the caption box for one video in the same stacking context.
    fn create_caption_element(_object: &str) -> Option<web_sys::HtmlElement> {
        use wasm_bindgen::JsCast as _;

        let document = web_sys::window()?.document()?;
        let element = document
            .create_element("div")
            .ok()?
            .dyn_into::<web_sys::HtmlElement>()
            .ok()?;
        let style = element.style();
        let _ = style.set_property("position", "absolute");
        let _ = style.set_property("z-index", "11");
        let _ = style.set_property("display", "none");
        let _ = style.set_property("pointer-events", "none");
        let _ = style.set_property("box-sizing", "border-box");
        let _ = style.set_property("background", "rgba(0, 0, 0, 0.78)");
        let _ = style.set_property("color", "#FFFFFF");
        let _ = style.set_property(
            "font-family",
            "system-ui, -apple-system, 'Segoe UI', sans-serif",
        );
        let _ = style.set_property("font-size", "16px");
        let _ = style.set_property("line-height", "1.35");
        let _ = style.set_property("padding", "4px 10px");
        let _ = style.set_property("border-radius", "4px");
        let _ = style.set_property("text-align", "center");
        let _ = style.set_property("white-space", "pre-wrap");
        let _ = style.set_property("transform", "translateY(-100%)");
        let _ = element.set_attribute("aria-hidden", "true");
        let _ = element.set_text_content(Some(""));
        document.body()?.append_child(&element).ok()?;
        Some(element)
    }
}

#[cfg(target_arch = "wasm32")]
impl MediaBackend for WebBackend {
    fn play(&mut self, object: &str, src: &str, kind: MediaKind, bytes: Arc<[u8]>) {
        let label = if kind == MediaKind::Video {
            "video"
        } else {
            "audio"
        };

        if let Some(track) = self.tracks.get(object) {
            let generation = track.generation;
            let element = track.element.clone();
            element.set_volume(f64::from(self.effective_volume(object)));
            match self.start_element(&element, object, label, generation) {
                Ok(closure) => {
                    if let Some(track) = self.tracks.get_mut(object) {
                        track.play_closures.push(closure);
                    }
                }
                Err(()) => self.push_ended(object, generation),
            }
            return;
        }

        let generation = self.next_generation;
        self.next_generation = self.next_generation.saturating_add(1);
        let Some(mut track) = self.new_track(object, src, kind, &bytes, generation) else {
            return;
        };
        match self.start_element(&track.element, object, label, generation) {
            Ok(closure) => {
                track.play_closures.push(closure);
                if let Ok(mut live) = self.live.lock() {
                    live.insert(object.to_string(), generation);
                }
                self.tracks.insert(object.to_string(), track);
            }
            Err(()) => {
                // play() threw: dropping the track removes the element and
                // revokes the blob URL.
                self.push_ended(object, 0);
            }
        }
    }

    fn pause(&mut self, object: &str) {
        if let Some(track) = self.tracks.get_mut(object) {
            let _ = track.element.pause();
        }
    }

    fn stop(&mut self, object: &str) {
        if let Some(track) = self.tracks.remove(object) {
            track.element.set_current_time(0.0);
        }
        if let Ok(mut live) = self.live.lock() {
            live.remove(object);
        }
    }

    fn stop_all(&mut self) {
        self.tracks.clear();
        if let Ok(mut live) = self.live.lock() {
            live.clear();
        }
        if let Ok(mut q) = self.ended_queue.lock() {
            q.clear();
        }
    }

    fn seek(&mut self, object: &str, ms: u64) {
        let ended = {
            let Some(track) = self.tracks.get_mut(object) else {
                return;
            };
            let seconds = ms as f64 / 1000.0;
            if !seconds.is_finite() || seconds < 0.0 {
                return;
            }
            let duration = track.element.duration();
            let has_duration = duration.is_finite() && duration >= 0.0;
            let target = if has_duration {
                seconds.min(duration)
            } else {
                seconds
            };
            track.element.set_current_time(target);
            // Browsers differ on whether seeking to the end of a paused element
            // fires `ended`; finish it here so the runtime always hears about it.
            if has_duration && target >= duration {
                let _ = track.element.pause();
                Some((object.to_owned(), track.generation))
            } else {
                None
            }
        };
        if let Some((object, generation)) = ended {
            self.push_ended(&object, generation);
        }
    }

    fn set_volume(&mut self, volume: f32) {
        self.volume = sanitize_volume(volume);
        for (object, track) in &self.tracks {
            track
                .element
                .set_volume(f64::from(self.effective_volume(object)));
        }
    }

    fn set_rate(&mut self, rate: f32) {
        self.rate = if rate.is_finite() {
            f64::from(rate.clamp(0.5, 2.0))
        } else {
            1.0
        };
        for track in self.tracks.values() {
            track.element.set_playback_rate(self.rate);
        }
    }

    fn supports_rate(&self) -> bool {
        true
    }

    fn set_object_volume(&mut self, object: &str, volume: f32) {
        self.object_volumes
            .insert(object.to_owned(), sanitize_volume(volume));
        if let Some(track) = self.tracks.get(object) {
            track
                .element
                .set_volume(f64::from(self.effective_volume(object)));
        }
    }

    fn take_ended(&mut self) -> Vec<String> {
        let queued = match self.ended_queue.lock() {
            Ok(mut q) => std::mem::take(&mut *q),
            Err(_) => return Vec::new(),
        };
        let Ok(live) = self.live.lock() else {
            return Vec::new();
        };
        let mut seen = std::collections::HashSet::new();
        queued
            .into_iter()
            .filter(|(object, generation)| *generation == 0 || live.get(object) == Some(generation))
            .map(|(object, _)| object)
            .filter(|object| seen.insert(object.clone()))
            .collect()
    }

    fn position_ms(&self, object: &str) -> Option<u64> {
        let track = self.tracks.get(object)?;
        if track.element.paused() {
            return None;
        }
        let seconds = track.element.current_time();
        if !seconds.is_finite() || seconds < 0.0 {
            return None;
        }
        let millis = (seconds * 1000.0).round();
        if millis > u64::MAX as f64 {
            None
        } else {
            Some(millis as u64)
        }
    }

    fn layout_video(&mut self, object: &str, rect: Option<[f32; 4]>) {
        if let Some(track) = self.tracks.get(object)
            && track.is_video
        {
            let style = track.element.style();
            match rect {
                Some([x, y, w, h]) => {
                    let _ = style.set_property("display", "block");
                    let _ = style.set_property("left", &format!("{x}px"));
                    let _ = style.set_property("top", &format!("{y}px"));
                    let _ = style.set_property("width", &format!("{w}px"));
                    let _ = style.set_property("height", &format!("{h}px"));
                    if let Some(caption) = &track.caption {
                        let caption_style = caption.style();
                        let _ = caption_style.set_property("left", &format!("{}px", x + w * 0.05));
                        let _ = caption_style.set_property("width", &format!("{}px", w * 0.9));
                        // Scale with the video, or a small video is mostly caption box.
                        let _ = caption_style.set_property(
                            "font-size",
                            &format!("{}px", (h * 0.075).clamp(11.0, 28.0)),
                        );
                        let _ = caption_style.set_property("top", &format!("{}px", y + h - 8.0));
                        let _ = caption_style.set_property(
                            "display",
                            if track.caption_visible {
                                "block"
                            } else {
                                "none"
                            },
                        );
                    }
                }
                None => {
                    let _ = style.set_property("display", "none");
                    if let Some(caption) = &track.caption {
                        let _ = caption.style().set_property("display", "none");
                    }
                }
            }
        }
    }

    fn show_caption(&mut self, object: &str, text: Option<&str>) {
        let Some(track) = self.tracks.get_mut(object) else {
            return;
        };
        if !track.is_video {
            return;
        }
        let Some(caption) = track.caption.clone() else {
            return;
        };
        match text.filter(|text| !text.is_empty()) {
            Some(text) => {
                let _ = caption.set_text_content(Some(text));
                track.caption_visible = true;
                let _ = caption.style().set_property("display", "block");
            }
            None => {
                let _ = caption.set_text_content(Some(""));
                track.caption_visible = false;
                let _ = caption.style().set_property("display", "none");
            }
        }
    }
}

// ---------------------------------------------------------------- Default Backend Factory

/// Default backend type for the target platform.
#[cfg(not(target_arch = "wasm32"))]
pub type DefaultMediaBackend = RodioBackend;

/// Default backend type for the target platform.
#[cfg(target_arch = "wasm32")]
pub type DefaultMediaBackend = WebBackend;

/// Create a new default media backend for the current platform.
#[must_use]
pub fn create_default_backend() -> Box<dyn MediaBackend> {
    Box::new(DefaultMediaBackend::new())
}

/// Generate a valid 44-byte WAV header plus PCM samples for testing and fallback.
#[must_use]
pub fn tiny_wav_bytes() -> Vec<u8> {
    let num_samples: u32 = 100;
    let data_len: u32 = num_samples.saturating_mul(2); // 16-bit mono = 2 bytes per sample
    let file_size: u32 = 36u32.saturating_add(data_len);

    let mut bytes = Vec::with_capacity(44usize.saturating_add(data_len as usize));
    // RIFF header
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&file_size.to_le_bytes());
    bytes.extend_from_slice(b"WAVE");

    // fmt chunk
    bytes.extend_from_slice(b"fmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes()); // subchunk size (16 for PCM)
    bytes.extend_from_slice(&1u16.to_le_bytes()); // audio format (1 = PCM)
    bytes.extend_from_slice(&1u16.to_le_bytes()); // channels (1 = mono)
    bytes.extend_from_slice(&44100u32.to_le_bytes()); // sample rate
    bytes.extend_from_slice(&(44100u32.saturating_mul(2)).to_le_bytes()); // byte rate
    bytes.extend_from_slice(&2u16.to_le_bytes()); // block align
    bytes.extend_from_slice(&16u16.to_le_bytes()); // bits per sample

    // data chunk
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());

    // 100 samples of a small sine/triangle wave
    for i in 0..num_samples {
        let sample = if (i % 2) == 0 { 500i16 } else { -500i16 };
        bytes.extend_from_slice(&sample.to_le_bytes());
    }

    bytes
}

#[cfg(test)]
mod tests;
