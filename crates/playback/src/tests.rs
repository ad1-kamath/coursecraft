use std::sync::Arc;

use crate::{MediaBackend, MediaKind, NullBackend, tiny_wav_bytes};

#[test]
fn null_backend_state_machine() {
    let mut backend = NullBackend::new();
    let bytes: Arc<[u8]> = tiny_wav_bytes().into();

    // 1. Initial state
    assert!(!backend.is_playing("obj1"));
    assert!(!backend.is_paused("obj1"));

    // 2. Play starting from 0
    backend.play("obj1", "sound.wav", MediaKind::Audio, Arc::clone(&bytes));
    assert!(backend.is_playing("obj1"));
    assert!(!backend.is_paused("obj1"));
    assert_eq!(&backend.recorded_calls()[..], &["play:obj1:sound.wav"]);

    // 3. Pause
    backend.pause("obj1");
    assert!(!backend.is_playing("obj1"));
    assert!(backend.is_paused("obj1"));
    assert_eq!(
        &backend.recorded_calls()[..],
        &["play:obj1:sound.wav", "pause:obj1"]
    );

    // 4. Play resumes
    backend.play("obj1", "sound.wav", MediaKind::Audio, Arc::clone(&bytes));
    assert!(backend.is_playing("obj1"));
    assert!(!backend.is_paused("obj1"));
    assert_eq!(
        &backend.recorded_calls()[..],
        &["play:obj1:sound.wav", "pause:obj1", "resume:obj1:sound.wav"]
    );

    // 5. Stop resets
    backend.stop("obj1");
    assert!(!backend.is_playing("obj1"));
    assert!(!backend.is_paused("obj1"));
    assert_eq!(
        &backend.recorded_calls()[..],
        &[
            "play:obj1:sound.wav",
            "pause:obj1",
            "resume:obj1:sound.wav",
            "stop:obj1"
        ]
    );

    // 6. Unknown object pause/stop are no-ops on state
    backend.pause("unknown");
    backend.stop("unknown");
    assert!(!backend.is_playing("unknown"));
    assert!(!backend.is_paused("unknown"));

    // 7. Stop all
    backend.play("obj2", "music.wav", MediaKind::Audio, Arc::clone(&bytes));
    backend.play("obj3", "video.mp4", MediaKind::Video, Arc::clone(&bytes));
    assert!(backend.is_playing("obj2"));
    assert!(backend.is_playing("obj3"));
    backend.stop_all();
    assert!(!backend.is_playing("obj2"));
    assert!(!backend.is_playing("obj3"));

    // 8. Ended reporting
    backend.mark_ended("obj2");
    backend.mark_ended("obj3");
    let ended = backend.take_ended();
    assert_eq!(ended, vec!["obj2", "obj3"]);
    assert!(backend.take_ended().is_empty());

    // 9. Layout video
    backend.layout_video("v1", Some([10.0, 20.0, 640.0, 360.0]));
    backend.layout_video("v1", None);
}

#[test]
fn null_backend_seek_and_volume() {
    let mut backend = NullBackend::new();
    let bytes: Arc<[u8]> = tiny_wav_bytes().into();
    assert_eq!(backend.volume(), 1.0);

    backend.play("a1", "sound.wav", MediaKind::Audio, Arc::clone(&bytes));
    backend.seek("a1", 1250);
    assert_eq!(backend.position_ms("a1"), Some(1250));
    assert!(
        backend
            .recorded_calls()
            .contains(&"seek:a1:1250".to_owned())
    );

    // A seek is recorded even for media that has not started yet.
    backend.seek("later", 900);
    assert!(
        backend
            .recorded_calls()
            .contains(&"seek:later:900".to_owned())
    );
    assert_eq!(backend.position_ms("later"), None);

    backend.set_volume(0.5);
    assert_eq!(backend.volume(), 0.5);
    backend.set_volume(f32::NAN);
    assert_eq!(backend.volume(), 0.0);
    backend.set_volume(2.0);
    assert_eq!(backend.volume(), 1.0);
    backend.set_volume(-1.0);
    assert_eq!(backend.volume(), 0.0);
    backend.set_volume(f32::INFINITY);
    assert_eq!(backend.volume(), 1.0);
}

#[test]
fn null_backend_records_object_volume() {
    let mut backend = NullBackend::new();
    assert_eq!(backend.object_volume("a1"), None);
    backend.set_object_volume("a1", 0.5);
    assert_eq!(backend.object_volume("a1"), Some(0.5));
    assert!(
        backend
            .recorded_calls()
            .contains(&"set_object_volume:a1:0.5".to_owned())
    );
    backend.set_object_volume("a1", f32::NAN);
    assert_eq!(backend.object_volume("a1"), Some(0.0));
    backend.set_object_volume("a2", 3.0);
    assert_eq!(backend.object_volume("a2"), Some(1.0));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn rodio_object_volume_keeps_the_product_after_set_volume() {
    let mut backend = crate::RodioBackend::new_without_device();
    backend.set_object_volume("a1", 0.5);
    backend.set_volume(0.5);
    assert!((backend.effective_volume("a1") - 0.25).abs() < 0.001);
    // An object without its own volume uses the player volume.
    assert!((backend.effective_volume("a2") - 0.5).abs() < 0.001);
    // The object volume survives a later player change.
    backend.set_volume(1.0);
    assert!((backend.effective_volume("a1") - 0.5).abs() < 0.001);
    // NaN silences the object, as it does the player.
    backend.set_object_volume("a1", f32::NAN);
    assert_eq!(backend.effective_volume("a1"), 0.0);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn rodio_backend_no_device_reports_ended() {
    let mut backend = crate::RodioBackend::new_without_device();
    let bytes: Arc<[u8]> = tiny_wav_bytes().into();

    // Video reports ended on desktop. The file now opens on the worker
    // thread (never the UI thread), so poll until the failed open is
    // reported instead of expecting it synchronously.
    backend.play("v1", "movie.mp4", MediaKind::Video, Arc::clone(&bytes));
    let mut ended = Vec::new();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    while ended.is_empty() && std::time::Instant::now() < deadline {
        ended = backend.take_ended();
        if ended.is_empty() {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }
    assert_eq!(ended, vec!["v1"]);

    // Audio without device reports ended immediately so triggers don't hang
    backend.play("a1", "sound.wav", MediaKind::Audio, Arc::clone(&bytes));
    assert_eq!(backend.take_ended(), vec!["a1"]);

    // Pause, stop, seek, volume and stop_all are safe no-ops
    backend.pause("a1");
    backend.stop("a1");
    backend.seek("a1", 500);
    backend.seek("missing", u64::MAX);
    backend.set_volume(0.25);
    backend.set_volume(f32::NAN);
    backend.stop_all();
    assert!(backend.take_ended().is_empty());
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn tiny_wav_decodes_successfully() {
    let bytes = tiny_wav_bytes();
    let cursor = std::io::Cursor::new(bytes);
    let decoder = rodio::Decoder::try_from(cursor);
    assert!(decoder.is_ok(), "tiny_wav_bytes must be a valid WAV format");
}

#[cfg(not(target_arch = "wasm32"))]
mod record_tests {
    use crate::record::{Recording, ToI16, downmix_to_mono, encode_wav, input_devices, peak_level};

    fn le_u16(bytes: &[u8], at: usize) -> u16 {
        let mut out = [0u8; 2];
        if let Some(src) = bytes.get(at..at + 2) {
            out.copy_from_slice(src);
        }
        u16::from_le_bytes(out)
    }

    fn le_u32(bytes: &[u8], at: usize) -> u32 {
        let mut out = [0u8; 4];
        if let Some(src) = bytes.get(at..at + 4) {
            out.copy_from_slice(src);
        }
        u32::from_le_bytes(out)
    }

    #[test]
    fn encode_wav_writes_a_playable_mono_header() {
        // One second of 440 Hz at 44.1 kHz, mono.
        let samples: Vec<i16> = (0..44_100)
            .map(|i| {
                let phase = std::f64::consts::TAU * 440.0 * f64::from(i) / 44_100.0;
                (phase.sin() * 20_000.0) as i16
            })
            .collect();
        let recording = Recording {
            sample_rate: 44_100,
            channels: 1,
            samples,
            truncated: false,
        };
        let bytes = encode_wav(&recording);
        let data_len = 44_100u32 * 2;

        assert_eq!(bytes.get(0..4), Some(&b"RIFF"[..]));
        assert_eq!(le_u32(&bytes, 4), 36 + data_len);
        assert_eq!(bytes.get(8..12), Some(&b"WAVE"[..]));
        assert_eq!(bytes.get(12..16), Some(&b"fmt "[..]));
        assert_eq!(le_u32(&bytes, 16), 16, "fmt chunk size");
        assert_eq!(le_u16(&bytes, 20), 1, "PCM");
        assert_eq!(le_u16(&bytes, 22), 1, "mono");
        assert_eq!(le_u32(&bytes, 24), 44_100, "sample rate");
        assert_eq!(le_u32(&bytes, 28), 44_100 * 2, "byte rate");
        assert_eq!(le_u16(&bytes, 32), 2, "block align");
        assert_eq!(le_u16(&bytes, 34), 16, "bits per sample");
        assert_eq!(bytes.get(36..40), Some(&b"data"[..]));
        assert_eq!(le_u32(&bytes, 40), data_len, "data size");
        assert_eq!(bytes.len(), 44 + data_len as usize);

        // rodio decodes the same signal back to the same sample count.
        let cursor = std::io::Cursor::new(bytes);
        let decoder = rodio::Decoder::try_from(cursor).expect("valid WAV");
        let decoded = decoder.count();
        assert!(
            decoded.abs_diff(44_100) <= 1,
            "round trip kept {decoded} of 44100 samples"
        );
    }

    #[test]
    fn downmix_to_mono_averages_each_frame() {
        let recording = Recording {
            sample_rate: 48_000,
            channels: 2,
            samples: vec![100, 300, -100, -300, 1_000, 0],
            truncated: true,
        };
        let mono = downmix_to_mono(&recording);
        assert_eq!(mono.sample_rate, 48_000);
        assert_eq!(mono.channels, 1);
        assert_eq!(mono.samples, vec![200, -200, 500]);
        assert!(mono.truncated, "the cap flag survives the downmix");
    }

    #[test]
    fn peak_level_decays_and_clamps() {
        assert_eq!(peak_level(&[], 0.0), 0.0);
        assert_eq!(peak_level(&[], f32::NAN), 0.0);
        assert_eq!(peak_level(&[i16::MIN], 0.0), 1.0);
        let loud = peak_level(&[] as &[i16], 1.0);
        assert!(loud < 1.0 && loud > 0.0, "previous peak decays to {loud}");
        assert!(peak_level(&[0; 64], 5.0) <= 1.0);
        assert!(peak_level(&[i16::MAX], f32::INFINITY) <= 1.0);
        assert!(
            peak_level(&[i16::MAX], 0.0) > 0.99,
            "a full-scale sample reads as a full meter"
        );
    }

    #[test]
    fn sample_converters_handle_the_extremes() {
        assert_eq!(0.0_f32.to_i16(), 0);
        assert_eq!(0.5_f32.to_i16(), 16_384);
        assert_eq!(1.0_f32.to_i16(), i16::MAX);
        assert_eq!((-1.0_f32).to_i16(), i16::MIN);
        assert_eq!(2.0_f32.to_i16(), i16::MAX);
        assert_eq!((-2.0_f32).to_i16(), i16::MIN);
        assert_eq!(f32::NAN.to_i16(), 0);
        assert_eq!(f32::INFINITY.to_i16(), 0);

        assert_eq!(0u16.to_i16(), i16::MIN);
        assert_eq!(32_767u16.to_i16(), -1);
        assert_eq!(32_768u16.to_i16(), 0);
        assert_eq!(u16::MAX.to_i16(), i16::MAX);

        assert_eq!(i16::MIN.to_i16(), i16::MIN);
        assert_eq!(0i16.to_i16(), 0);
        assert_eq!(i16::MAX.to_i16(), i16::MAX);
    }

    #[test]
    fn input_devices_does_not_panic() {
        // CI may have no microphone: an empty list is fine.
        let devices = input_devices();
        let _ = devices.len();
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod worker_tests {
    use std::sync::Arc;

    use crate::{
        AudioPcm, VideoClock, VideoDecoder, VideoFrame, WorkerCmd, WorkerState, pcm_to_f32,
        run_video_worker,
    };

    /// Simulated clock: `sleep_ms` advances time instantly and every
    /// published frame is recorded as (clock, pts).
    struct FakeClock {
        now: Arc<std::sync::Mutex<u64>>,
        published: Arc<std::sync::Mutex<Vec<(u64, u64)>>>,
    }

    impl VideoClock for FakeClock {
        fn now_ms(&self) -> u64 {
            self.now.lock().map(|n| *n).unwrap_or(0)
        }
        fn sleep_ms(&self, ms: u64) {
            if let Ok(mut n) = self.now.lock() {
                *n = n.saturating_add(ms);
            }
        }
        fn on_publish(&self, pts_ms: u64) {
            if let Ok(mut p) = self.published.lock() {
                let now = self.now.lock().map(|n| *n).unwrap_or(0);
                p.push((now, pts_ms));
            }
        }
    }

    fn fake_clock() -> FakeClock {
        FakeClock {
            now: Arc::new(std::sync::Mutex::new(0)),
            published: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    struct FakeDecoder {
        frames: Vec<u64>,
        index: usize,
        duration_ms: u64,
        seeks: Vec<u64>,
        audio: Option<AudioPcm>,
        /// Simulated decode cost: each `next_frame` advances this clock.
        clock_now: Option<Arc<std::sync::Mutex<u64>>>,
        decode_cost_ms: u64,
        /// Index of the frame whose decode hiccups by `hiccup_ms`.
        hiccup_index: Option<usize>,
        hiccup_ms: u64,
    }

    impl VideoDecoder for FakeDecoder {
        fn duration_ms(&self) -> u64 {
            self.duration_ms
        }
        fn seek(&mut self, ms: u64) -> Result<(), String> {
            self.seeks.push(ms);
            self.index = self
                .frames
                .iter()
                .position(|pts| *pts >= ms)
                .unwrap_or(self.frames.len());
            Ok(())
        }
        fn next_frame(&mut self) -> Result<Option<VideoFrame>, String> {
            let Some(pts) = self.frames.get(self.index).copied() else {
                return Ok(None);
            };
            let this_index = self.index;
            self.index += 1;
            if let Some(clock_now) = &self.clock_now {
                let mut cost = self.decode_cost_ms;
                if Some(this_index) == self.hiccup_index {
                    cost = cost.saturating_add(self.hiccup_ms);
                }
                if let Ok(mut now) = clock_now.lock() {
                    *now = now.saturating_add(cost);
                }
            }
            Ok(Some(VideoFrame {
                pts_ms: pts,
                width: 16,
                height: 16,
                rgba: vec![0; 16 * 16 * 4],
            }))
        }
        fn has_audio(&self) -> bool {
            self.audio.is_some()
        }
        fn decode_audio(&mut self) -> Option<AudioPcm> {
            self.audio.clone()
        }
    }

    fn spawn(
        frames: &[u64],
        duration_ms: u64,
    ) -> (
        std::sync::mpsc::Sender<WorkerCmd>,
        Arc<std::sync::Mutex<WorkerState>>,
        std::thread::JoinHandle<()>,
    ) {
        spawn_with_audio(frames, duration_ms, None)
    }

    fn spawn_with_audio(
        frames: &[u64],
        duration_ms: u64,
        audio: Option<AudioPcm>,
    ) -> (
        std::sync::mpsc::Sender<WorkerCmd>,
        Arc<std::sync::Mutex<WorkerState>>,
        std::thread::JoinHandle<()>,
    ) {
        let decoder = FakeDecoder {
            frames: frames.to_vec(),
            index: 0,
            duration_ms,
            seeks: Vec::new(),
            audio,
            clock_now: None,
            decode_cost_ms: 0,
            hiccup_index: None,
            hiccup_ms: 0,
        };
        let (tx, rx) = std::sync::mpsc::channel::<WorkerCmd>();
        let shared = Arc::new(std::sync::Mutex::new(WorkerState::default()));
        let shared_thread = Arc::clone(&shared);
        let handle = std::thread::spawn(move || {
            run_video_worker(decoder, rx, shared_thread, crate::SystemClock::new());
        });
        (tx, shared, handle)
    }

    #[test]
    fn worker_play_publishes_and_ends() {
        let (tx, shared, handle) = spawn(&[0, 100, 200, 300], 300);
        tx.send(WorkerCmd::Play).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(250));
        {
            let state = shared.lock().unwrap();
            assert!(state.decoding, "worker is decoding while playing");
            // Frames are published at ~30fps pacing in real code, but the
            // fake decode publishes instantly; position advances past start.
            assert!(
                state.frame.as_ref().is_some_and(|f| f.pts_ms >= 100),
                "frame published, got {:?}",
                state.frame.as_ref().map(|f| f.pts_ms)
            );
        }
        tx.send(WorkerCmd::Stop).unwrap();
        handle.join().unwrap();
    }

    #[test]
    fn worker_pause_halts_position() {
        let (tx, shared, handle) = spawn(&[0, 500, 1000, 1500, 2000], 2000);
        tx.send(WorkerCmd::Play).unwrap();
        tx.send(WorkerCmd::Pause).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(120));
        let state = shared.lock().unwrap();
        assert!(!state.playing, "worker paused");
        tx.send(WorkerCmd::Stop).unwrap();
        drop(state);
        handle.join().unwrap();
    }

    #[test]
    fn worker_seek_while_paused_publishes_one_frame() {
        let frames: Vec<u64> = (0..5000).step_by(100).collect();
        let (tx, shared, handle) = spawn(&frames, 5000);
        // Paused scrub: seeking must publish the requested frame without
        // resuming playback, so the timeline canvas never goes blank.
        tx.send(WorkerCmd::Seek(1500)).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(200));
        {
            let state = shared.lock().unwrap();
            assert!(!state.playing, "worker stays paused");
            let pts = state.frame.as_ref().map(|frame| frame.pts_ms);
            assert_eq!(pts, Some(1500), "one frame published for the seek");
            assert_eq!(state.position_ms, Some(1500), "position follows the seek");
            assert!(!state.ended, "a paused seek never ends the stream");
        }
        tx.send(WorkerCmd::Stop).unwrap();
        handle.join().unwrap();
    }

    #[test]
    fn worker_paused_seek_to_the_end_shows_the_last_frame() {
        // Seeking to exactly the duration lands past the last frame; the
        // worker clamps to the last frame and stops retrying (end of
        // stream), keeping that frame instead of spinning forever.
        let frames: Vec<u64> = (0..5000).step_by(100).collect();
        let (tx, shared, handle) = spawn(&frames, 5000);
        tx.send(WorkerCmd::Seek(5000)).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(300));
        {
            let state = shared.lock().unwrap();
            assert!(!state.playing, "worker stays paused");
            assert!(!state.ended, "a paused seek never ends the stream");
            let pts = state.frame.as_ref().map(|frame| frame.pts_ms);
            assert_eq!(pts, Some(4900), "last frame shown for seek to end");
        }
        tx.send(WorkerCmd::Stop).unwrap();
        handle.join().unwrap();
    }

    #[test]
    fn worker_seek_while_playing_keeps_newest() {
        let frames: Vec<u64> = (0..3000).step_by(100).collect();
        let (tx, shared, handle) = spawn(&frames, 3000);
        tx.send(WorkerCmd::Play).unwrap();
        // Two seeks in quick succession: only the newest survives (worker
        // drains and coalesces), so the position ends near 2500, not 500.
        tx.send(WorkerCmd::Seek(500)).unwrap();
        tx.send(WorkerCmd::Seek(2500)).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(300));
        {
            let state = shared.lock().unwrap();
            let pos = state.position_ms.unwrap_or(0);
            assert!(pos >= 2400, "newest seek wins (pos={pos}, want near 2500+)");
        }
        tx.send(WorkerCmd::Stop).unwrap();
        handle.join().unwrap();
    }

    #[test]
    fn worker_reports_end_and_stop_clears() {
        let (tx, shared, handle) = spawn(&[0, 100], 100);
        tx.send(WorkerCmd::Play).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(250));
        {
            let state = shared.lock().unwrap();
            assert!(state.ended, "worker reports end of stream");
        }
        tx.send(WorkerCmd::Stop).unwrap();
        handle.join().unwrap();
        let state = shared.lock().unwrap();
        assert!(state.frame.is_none(), "stop clears the frame");
        assert_eq!(state.position_ms, None, "stop clears position");
        assert!(state.audio.is_none(), "stop clears audio");
    }

    #[test]
    fn worker_pcm_goes_to_audio_path() {
        // A fake decoder whose track decodes to PCM: the worker must publish
        // that PCM on the shared state so the backend can play it through a
        // rodio SamplesBuffer as the master clock.
        let pcm = AudioPcm {
            sample_rate: 48000,
            channels: 2,
            samples: vec![16384, -16384, 8192, -8192],
        };
        let (tx, shared, handle) = spawn_with_audio(&[0, 100, 200, 300], 300, Some(pcm.clone()));
        // Audio is decoded on the worker at startup, before any Play.
        std::thread::sleep(std::time::Duration::from_millis(120));
        {
            let state = shared.lock().unwrap();
            let published = state.audio.as_ref().expect("PCM published by worker");
            assert_eq!(published.sample_rate, 48000);
            assert_eq!(published.channels, 2);
            assert_eq!(published.samples, pcm.samples);
        }
        // The PCM also converts to a SamplesBuffer source whose duration
        // matches the sample count (48000 Hz stereo, 4 samples = 1 frame).
        let f32_samples = pcm_to_f32(&pcm.samples);
        assert_eq!(f32_samples.len(), 4);
        assert_eq!(f32_samples[0], 0.5);
        assert_eq!(f32_samples[1], -0.5);
        let buffer = rodio::buffer::SamplesBuffer::new(2, 48000, f32_samples);
        use rodio::Source;
        let duration = buffer.total_duration().unwrap();
        assert_eq!(duration.as_millis(), 0, "2 frames at 48 kHz is < 1 ms");
        // Frames still publish alongside audio.
        tx.send(WorkerCmd::Play).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(200));
        {
            let state = shared.lock().unwrap();
            assert!(
                state.frame.as_ref().is_some_and(|f| f.pts_ms >= 100),
                "frames publish with audio"
            );
            assert!(state.audio.is_some(), "audio stays published");
        }
        tx.send(WorkerCmd::Stop).unwrap();
        handle.join().unwrap();
    }

    #[test]
    fn worker_paces_24fps_frames_to_the_fake_clock() {
        // 24 fps over 30 simulated seconds: every frame published at
        // clock T has pts within one frame (42 ms) of T, and the stream
        // ends (the fake clock never waits in real time).
        let frames: Vec<u64> = (0..720).map(|i| (i as u64) * 1000 / 24).collect();
        let clock = fake_clock();
        let clock_now = Arc::clone(&clock.now);
        let published = Arc::clone(&clock.published);
        let decoder = FakeDecoder {
            frames,
            index: 0,
            duration_ms: 30_000,
            seeks: Vec::new(),
            audio: None,
            clock_now: None,
            decode_cost_ms: 0,
            hiccup_index: None,
            hiccup_ms: 0,
        };
        let (tx, rx) = std::sync::mpsc::channel::<WorkerCmd>();
        let shared = Arc::new(std::sync::Mutex::new(WorkerState::default()));
        let shared_thread = Arc::clone(&shared);
        let handle = std::thread::spawn(move || {
            run_video_worker(decoder, rx, shared_thread, clock);
        });
        tx.send(WorkerCmd::Play).unwrap();
        // Wait for the stream to end (simulated 30 s runs in milliseconds).
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            let ended = shared.lock().map(|s| s.ended).unwrap_or(false);
            if ended || std::time::Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        tx.send(WorkerCmd::Stop).unwrap();
        handle.join().unwrap();
        let _ = clock_now;
        let log = published.lock().unwrap();
        assert!(
            log.len() > 600,
            "most of 720 frames publish, got {}",
            log.len()
        );
        for (clock_ms, pts_ms) in log.iter() {
            assert!(
                pts_ms.abs_diff(*clock_ms) <= 42,
                "frame pts {pts_ms} published at clock {clock_ms} (more than one frame off)"
            );
        }
        assert!(
            log.last().is_some_and(|(c, _)| *c >= 29_900),
            "clock reached the end of 30 simulated seconds"
        );
    }

    #[test]
    fn worker_drops_frames_that_are_more_than_one_frame_late() {
        // A decode hiccup makes several frames late; they are dropped
        // (not published) so playback catches up to the clock.
        let frames: Vec<u64> = (0..240).map(|i| (i as u64) * 1000 / 24).collect();
        let clock = fake_clock();
        let clock_now = Arc::clone(&clock.now);
        let published = Arc::clone(&clock.published);
        let decoder = FakeDecoder {
            frames,
            index: 0,
            duration_ms: 10_000,
            seeks: Vec::new(),
            audio: None,
            clock_now: Some(Arc::clone(&clock_now)),
            decode_cost_ms: 10,
            hiccup_index: Some(20),
            hiccup_ms: 500,
        };
        let (tx, rx) = std::sync::mpsc::channel::<WorkerCmd>();
        let shared = Arc::new(std::sync::Mutex::new(WorkerState::default()));
        let shared_thread = Arc::clone(&shared);
        let handle = std::thread::spawn(move || {
            run_video_worker(decoder, rx, shared_thread, clock);
        });
        tx.send(WorkerCmd::Play).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            let ended = shared.lock().map(|s| s.ended).unwrap_or(false);
            if ended || std::time::Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        tx.send(WorkerCmd::Stop).unwrap();
        handle.join().unwrap();
        let log = published.lock().unwrap();
        // The 500 ms hiccup at frame 20 must drop frames (240 decoded,
        // clearly fewer published) while published frames stay on time.
        assert!(
            log.len() < 230,
            "late frames were dropped ({} of 240 published)",
            log.len()
        );
        for (clock_ms, pts_ms) in log.iter() {
            assert!(
                pts_ms.abs_diff(*clock_ms) <= 42,
                "after catch-up, frame pts {pts_ms} at clock {clock_ms}"
            );
        }
    }

    #[test]
    fn worker_recovers_when_short_sound_ends_before_video() {
        // 8 s of video, sound only 3 s: the master position freezes at
        // 3000 ms while the sink is finished. The backend (fixed with
        // `take_ended`/`position_ms`) then publishes `None`, and the worker
        // must fall back to its own clock (the anchor already tracks real
        // time) so frames reach the end and `ended` is set.
        let frames: Vec<u64> = (0..8000).step_by(100).collect();
        let (tx, shared, handle) = spawn(&frames, 8000);
        // Simulate the finished sound's frozen master before playback.
        if let Ok(mut state) = shared.lock() {
            state.audio_position_ms = Some(3000);
        }
        tx.send(WorkerCmd::Play).unwrap();
        // The master holds at 3000 ms: frames publish up to it, then stop.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            let pos = shared.lock().map(|s| s.position_ms).unwrap_or(None);
            if pos == Some(3000) || std::time::Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        {
            let state = shared.lock().unwrap();
            assert_eq!(state.position_ms, Some(3000), "master stops at 3000");
            assert_eq!(
                state.frame.as_ref().map(|f| f.pts_ms),
                Some(3000),
                "picture freezes at 3000 while the sound has ended"
            );
            assert!(!state.ended, "video has not ended");
        }
        // The backend now clears the finished sink (`None`): the worker's
        // own clock takes over from the anchor.
        if let Ok(mut state) = shared.lock() {
            state.audio_position_ms = None;
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        loop {
            let ended = shared.lock().map(|s| s.ended).unwrap_or(false);
            if ended || std::time::Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        {
            let state = shared.lock().unwrap();
            assert!(state.ended, "frames reach the end after the master clears");
            assert_eq!(state.position_ms, Some(8000), "position reaches the end");
            let pts = state.frame.as_ref().map(|f| f.pts_ms).unwrap_or(0);
            assert!(
                pts >= 7900,
                "last published frame is near the end (pts={pts})"
            );
        }
        tx.send(WorkerCmd::Stop).unwrap();
        handle.join().unwrap();
    }

    #[test]
    fn worker_without_audio_uses_own_clock() {
        // Without audio the worker's own clock drives frames: positions come
        // from frame pts, not from an audio sink.
        let (tx, shared, handle) = spawn(&[0, 100, 200, 300], 300);
        tx.send(WorkerCmd::Play).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(250));
        {
            let state = shared.lock().unwrap();
            assert!(state.audio.is_none(), "no audio track");
            let pos = state.position_ms.unwrap_or(0);
            assert!(pos >= 100, "worker clock advances position (pos={pos})");
            let frame_pts = state.frame.as_ref().map(|f| f.pts_ms).unwrap_or(0);
            assert_eq!(pos, frame_pts.min(300), "position follows frames");
        }
        tx.send(WorkerCmd::Stop).unwrap();
        handle.join().unwrap();
    }
}
