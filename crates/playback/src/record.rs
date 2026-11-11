//! Desktop microphone recording (L2).
//!
//! Captures 16-bit PCM from the OS input device through `cpal` (CoreAudio on
//! the Mac, WASAPI on Windows) so the editor can record narration without
//! borrowing an `unsafe` call. Everything here is pure Rust above cpal; wasm
//! compiles the module out.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

/// Longest take the recorder keeps, in milliseconds (30 minutes).
pub const MAX_RECORDING_MS: u64 = 30 * 60 * 1000;

/// How much of the previous peak survives one block without new sound.
const LEVEL_DECAY: f32 = 0.85;

/// A finished take as 16-bit interleaved PCM.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Recording {
    /// Sample rate in Hz.
    pub sample_rate: u32,
    /// Channel count.
    pub channels: u16,
    /// Interleaved signed 16-bit samples.
    pub samples: Vec<i16>,
    /// Whether the take hit [`MAX_RECORDING_MS`] and lost the rest.
    pub truncated: bool,
}

/// Why a take could not start or finish.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordError {
    /// The host has no default input device.
    NoDevice,
    /// The requested input device is not available.
    Device(String),
    /// The input stream could not be opened or run.
    Stream(String),
    /// Stopped before any audio arrived.
    Empty,
}

impl std::fmt::Display for RecordError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoDevice => write!(f, "no microphone available"),
            Self::Device(name) => write!(f, "microphone '{name}' not found"),
            Self::Stream(message) => write!(f, "microphone error: {message}"),
            Self::Empty => write!(f, "nothing was recorded"),
        }
    }
}

impl std::error::Error for RecordError {}

/// A live recorder the editor dialog drives: meter while recording, then
/// `stop` for the take. A fake can implement it in tests.
pub trait RecorderBackend: Send {
    /// Current input peak, `0.0`–`1.0`, decaying between blocks.
    fn level(&self) -> f32;
    /// Recorded length in milliseconds.
    fn elapsed_ms(&self) -> u64;
    /// Stop capturing and return the take.
    fn stop(self: Box<Self>) -> Result<Recording, RecordError>;
}

/// How long a device list may take before it is treated as unavailable.
///
/// CoreAudio can stall for minutes on a sleeping Continuity device (an
/// iPhone microphone shows up as an input), so the query runs off-thread and
/// the dialog never freezes waiting for a device to answer.
const DEVICE_QUERY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

/// Microphone names on the default host, empty on error.
#[must_use]
pub fn input_devices() -> Vec<String> {
    let (tx, rx) = std::sync::mpsc::channel::<Vec<String>>();
    std::thread::spawn(move || {
        let host = cpal::default_host();
        let names = match host.input_devices() {
            Ok(devices) => devices.filter_map(|device| device.name().ok()).collect(),
            Err(_) => Vec::new(),
        };
        let _ = tx.send(names);
    });
    rx.recv_timeout(DEVICE_QUERY_TIMEOUT).unwrap_or_default()
}

/// Convert one device sample of any supported format into 16-bit PCM.
pub(crate) trait ToI16 {
    /// Convert this sample.
    fn to_i16(self) -> i16;
}

impl ToI16 for f32 {
    fn to_i16(self) -> i16 {
        if !self.is_finite() {
            return 0;
        }
        let scaled = (self.clamp(-1.0, 1.0) * 32768.0).round();
        if scaled >= f32::from(i16::MAX) {
            i16::MAX
        } else if scaled <= f32::from(i16::MIN) {
            i16::MIN
        } else {
            // `scaled` is inside `i16`'s range and was rounded to a whole number.
            scaled as i16
        }
    }
}

impl ToI16 for i16 {
    fn to_i16(self) -> i16 {
        self
    }
}

impl ToI16 for u16 {
    fn to_i16(self) -> i16 {
        let centered = i32::from(self) - 32768;
        i16::try_from(centered).unwrap_or(0)
    }
}

/// Decaying peak meter for one block of samples, clamped to `0.0`–`1.0`.
#[must_use]
pub(crate) fn peak_level(samples: &[i16], previous: f32) -> f32 {
    let previous = if previous.is_finite() {
        previous.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let peak = samples.iter().fold(0.0_f32, |peak, sample| {
        let amplitude = (f32::from(*sample) / 32768.0).abs();
        peak.max(amplitude)
    });
    (previous * LEVEL_DECAY).max(peak).clamp(0.0, 1.0)
}

/// Shared capture state, written by the audio callback and read by the UI.
#[derive(Default)]
struct Shared {
    samples: Mutex<Vec<i16>>,
    level: Mutex<f32>,
    truncated: AtomicBool,
}

/// Append one callback block, dropping it past the cap.
fn record_block(shared: &Shared, block: &[i16], cap: usize) {
    let previous = shared.level.lock().map(|level| *level).unwrap_or(0.0);
    let level = peak_level(block, previous);
    if let Ok(mut current) = shared.level.lock() {
        *current = level;
    }
    if let Ok(mut samples) = shared.samples.lock() {
        let room = cap.saturating_sub(samples.len());
        if block.len() > room {
            shared.truncated.store(true, Ordering::Relaxed);
        }
        samples.extend(block.iter().take(room).copied());
    }
}

/// Sample count that fills [`MAX_RECORDING_MS`] at this rate and width.
fn cap_samples(sample_rate: u32, channels: u16) -> usize {
    let rate = u64::from(sample_rate).saturating_mul(u64::from(channels));
    let cap = rate.saturating_mul(MAX_RECORDING_MS) / 1000;
    usize::try_from(cap).unwrap_or(usize::MAX)
}

/// Build the cpal input stream for one sample format.
fn build_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    shared: Arc<Shared>,
    cap: usize,
) -> Result<cpal::Stream, RecordError>
where
    T: cpal::SizedSample + ToI16,
{
    let callback_shared = Arc::clone(&shared);
    device
        .build_input_stream(
            config,
            move |data: &[T], _info: &cpal::InputCallbackInfo| {
                let block: Vec<i16> = data.iter().map(|sample| (*sample).to_i16()).collect();
                record_block(&callback_shared, &block, cap);
            },
            |error| log::warn!("microphone stream error: {error}"),
            None,
        )
        .map_err(|error| RecordError::Stream(error.to_string()))
}

/// An open microphone stream collecting PCM until stopped.
///
/// cpal's stream is not `Send` on every platform, so the stream lives on a
/// dedicated thread that stays parked until [`RecorderBackend::stop`]; this
/// type only holds the channel and handle, which are `Send`.
pub struct Recorder {
    stop_tx: Option<std::sync::mpsc::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
    shared: Arc<Shared>,
    sample_rate: u32,
    channels: u16,
}

impl Recorder {
    /// Open the named input device (or the default) and start recording.
    pub fn start(device: Option<&str>) -> Result<Box<dyn RecorderBackend>, RecordError> {
        let shared = Arc::new(Shared::default());
        // The stream is built, played and parked on a thread of its own:
        // `cpal::Stream` is not `Send` on every platform. `ready` carries the
        // outcome back so `start` can report why a microphone failed.
        let (ready_tx, ready_rx) =
            std::sync::mpsc::sync_channel::<Result<(u32, u16), RecordError>>(0);
        let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
        let thread_shared = Arc::clone(&shared);
        let wanted = device.map(str::to_owned);
        let thread = std::thread::spawn(move || {
            open_and_record(wanted, thread_shared, ready_tx, stop_rx);
        });

        match ready_rx.recv() {
            Ok(Ok((sample_rate, channels))) => Ok(Box::new(Self {
                stop_tx: Some(stop_tx),
                thread: Some(thread),
                shared,
                sample_rate,
                channels,
            })),
            Ok(Err(error)) => {
                let _ = thread.join();
                Err(error)
            }
            Err(_) => {
                let _ = thread.join();
                Err(RecordError::Stream(
                    "microphone thread stopped before recording".to_string(),
                ))
            }
        }
    }
}

/// Runs on the capture thread: open the device, start the stream and park.
fn open_and_record(
    name: Option<String>,
    shared: Arc<Shared>,
    ready: std::sync::mpsc::SyncSender<Result<(u32, u16), RecordError>>,
    stop_rx: std::sync::mpsc::Receiver<()>,
) {
    let host = cpal::default_host();
    let device = match resolve_device(&host, name.as_deref()) {
        Ok(device) => device,
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };
    let supported = match device.default_input_config() {
        Ok(supported) => supported,
        Err(error) => {
            let _ = ready.send(Err(RecordError::Stream(error.to_string())));
            return;
        }
    };
    let sample_format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    if config.sample_rate.0 == 0 || config.channels == 0 {
        let _ = ready.send(Err(RecordError::Stream(
            "invalid microphone config".to_string(),
        )));
        return;
    }
    let cap = cap_samples(config.sample_rate.0, config.channels);
    let built = match sample_format {
        cpal::SampleFormat::F32 => build_stream::<f32>(&device, &config, shared, cap),
        cpal::SampleFormat::I16 => build_stream::<i16>(&device, &config, shared, cap),
        cpal::SampleFormat::U16 => build_stream::<u16>(&device, &config, shared, cap),
        other => Err(RecordError::Stream(format!(
            "unsupported microphone sample format {other:?}"
        ))),
    };
    let stream = match built {
        Ok(stream) => stream,
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };
    if let Err(error) = stream.play() {
        let _ = ready.send(Err(RecordError::Stream(error.to_string())));
        return;
    }
    if ready
        .send(Ok((config.sample_rate.0, config.channels)))
        .is_err()
    {
        return;
    }
    // Park until `stop` (or until the backend is dropped). Either way
    // dropping the stream here ends capture.
    let _ = stop_rx.recv();
    drop(stream);
}

/// Find the named device, or the host default when no name was given.
fn resolve_device(host: &cpal::Host, name: Option<&str>) -> Result<cpal::Device, RecordError> {
    match name {
        Some(name) => host
            .devices()
            .map_err(|error| RecordError::Stream(error.to_string()))?
            .find(|device| device.name().is_ok_and(|found| found == name))
            .ok_or_else(|| RecordError::Device(name.to_string())),
        None => host.default_input_device().ok_or(RecordError::NoDevice),
    }
}

impl RecorderBackend for Recorder {
    fn level(&self) -> f32 {
        self.shared.level.lock().map(|level| *level).unwrap_or(0.0)
    }

    fn elapsed_ms(&self) -> u64 {
        let frames = self
            .shared
            .samples
            .lock()
            .map(|samples| u64::try_from(samples.len()).unwrap_or(u64::MAX))
            .unwrap_or(0);
        let per_second = u64::from(self.sample_rate).saturating_mul(u64::from(self.channels));
        frames
            .saturating_mul(1000)
            .checked_div(per_second)
            .unwrap_or(0)
    }

    fn stop(self: Box<Self>) -> Result<Recording, RecordError> {
        let mut recorder = *self;
        if let Some(stop_tx) = recorder.stop_tx.take() {
            let _ = stop_tx.send(());
        }
        let joined = match recorder.thread.take() {
            Some(thread) => thread.join().is_ok(),
            None => true,
        };
        let samples = recorder
            .shared
            .samples
            .lock()
            .map(|mut samples| std::mem::take(&mut *samples))
            .map_err(|_| RecordError::Stream("recording buffer poisoned".to_string()))?;
        if !joined {
            return Err(RecordError::Stream(
                "microphone thread failed while stopping".to_string(),
            ));
        }
        if samples.is_empty() {
            return Err(RecordError::Empty);
        }
        Ok(Recording {
            sample_rate: recorder.sample_rate,
            channels: recorder.channels,
            samples,
            truncated: recorder.shared.truncated.load(Ordering::Relaxed),
        })
    }
}

/// Encode a take as a RIFF/WAVE file with 16-bit little-endian PCM.
///
/// Returns an empty vector when a header size would overflow `u32`.
#[must_use]
pub fn encode_wav(recording: &Recording) -> Vec<u8> {
    let Some(data_len) = u32::try_from(recording.samples.len().saturating_mul(2)).ok() else {
        return Vec::new();
    };
    let Some(riff_len) = 36_u32.checked_add(data_len) else {
        return Vec::new();
    };
    let Some(byte_rate) = recording
        .sample_rate
        .checked_mul(u32::from(recording.channels))
        .and_then(|rate| rate.checked_mul(2))
    else {
        return Vec::new();
    };
    let block_align = u32::from(recording.channels).saturating_mul(2);
    let Ok(block_align) = u16::try_from(block_align) else {
        return Vec::new();
    };

    let mut bytes = Vec::with_capacity(44usize.saturating_add(data_len as usize));
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&riff_len.to_le_bytes());
    bytes.extend_from_slice(b"WAVE");

    bytes.extend_from_slice(b"fmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
    bytes.extend_from_slice(&recording.channels.to_le_bytes());
    bytes.extend_from_slice(&recording.sample_rate.to_le_bytes());
    bytes.extend_from_slice(&byte_rate.to_le_bytes());
    bytes.extend_from_slice(&block_align.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());

    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for sample in &recording.samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}

/// Average each frame's channels into one mono track.
#[must_use]
pub fn downmix_to_mono(recording: &Recording) -> Recording {
    let channels = usize::from(recording.channels);
    if channels <= 1 {
        return recording.clone();
    }
    let mut samples = Vec::with_capacity(recording.samples.len() / channels);
    for frame in recording.samples.chunks_exact(channels) {
        let sum: i32 = frame.iter().map(|sample| i32::from(*sample)).sum();
        let average = sum / i32::try_from(channels).unwrap_or(i32::MAX);
        samples.push(i16::try_from(average).unwrap_or(0));
    }
    Recording {
        sample_rate: recording.sample_rate,
        channels: 1,
        samples,
        truncated: recording.truncated,
    }
}
