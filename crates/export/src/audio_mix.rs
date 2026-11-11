//! Audio mixing for the MP4 exporter (desktop only).
//!
//! Decodes each source once through the OS decoder, resamples it to 48 kHz
//! stereo, and mixes the voices the runtime's `media` effects ask for into the
//! frame currently being written. Nothing here decides play-time behaviour:
//! the effects say when a voice starts, seeks, pauses or stops.

use std::collections::HashMap;
use std::io::Write;
use std::sync::Arc;

use coursecraft_playback::sanitize_volume;
use coursecraft_video_native::VideoFile;

use crate::mp4::ExportMedia;

/// Output sample rate of the mixed track.
pub(crate) const SAMPLE_RATE: u32 = 48_000;
/// Stereo samples per millisecond at [`SAMPLE_RATE`].
const SAMPLES_PER_MS: u64 = 48;

/// One playing audio source as the runtime described it.
struct Voice {
    object: String,
    start_t: u64,
    offset_ms: u64,
    volume: f32,
    samples: Arc<[i16]>,
    done: bool,
}

/// Decoded source, cached so every play of the same file decodes once.
enum CachedAudio {
    Ready(Arc<[i16]>),
    /// The file decoded but carries no audio (a silent video, for example).
    Silent,
    /// Decoding failed; the first warning is already in the export report.
    Failed,
}

/// Mixes runtime media effects into interleaved 48 kHz stereo samples.
pub(crate) struct AudioMixer {
    voices: Vec<Voice>,
    cache: HashMap<String, CachedAudio>,
    /// Mix buffer for the current frame, interleaved stereo.
    master: Vec<i16>,
}

impl AudioMixer {
    /// Create a mixer with no voices and an empty cache.
    pub(crate) fn new() -> Self {
        Self {
            voices: Vec::new(),
            cache: HashMap::new(),
            master: Vec::new(),
        }
    }

    /// Start (or restart) `object` from the beginning of `src`.
    pub(crate) fn play(
        &mut self,
        object: &str,
        src: &str,
        volume: f32,
        t: u64,
        media: &dyn ExportMedia,
        warnings: &mut Vec<String>,
    ) {
        self.voices.retain(|voice| voice.object != object);
        let Some(samples) = self.decode(src, media, warnings) else {
            return;
        };
        self.voices.push(Voice {
            object: object.to_owned(),
            start_t: t,
            offset_ms: 0,
            volume: sanitize_volume(volume),
            samples,
            done: false,
        });
    }

    /// End `object`'s voice at `t` (pause and stop both silence it).
    pub(crate) fn end(&mut self, object: &str) {
        self.voices.retain(|voice| voice.object != object);
    }

    /// Move `object` to `ms` in file time, continuing from `t`.
    pub(crate) fn seek(&mut self, object: &str, ms: u64, t: u64) {
        if let Some(voice) = self.voices.iter_mut().find(|voice| voice.object == object) {
            voice.offset_ms = ms;
            voice.start_t = t;
            voice.done = false;
        }
    }

    /// Playback position of `object` in file milliseconds at time `t`.
    pub(crate) fn position_ms(&self, object: &str, t: u64) -> Option<u64> {
        self.voices
            .iter()
            .find(|voice| voice.object == object)
            .map(|voice| {
                voice
                    .offset_ms
                    .saturating_add(t.saturating_sub(voice.start_t))
            })
    }

    /// Mix the voices for `[t, next_t)` and return the objects that ran out.
    ///
    /// The mixed samples stay in [`Self::samples`] until the next call, which
    /// is exactly the one frame the encoder writes.
    pub(crate) fn mix_step(&mut self, t: u64, next_t: u64) -> Vec<String> {
        let frames =
            usize::try_from(next_t.saturating_sub(t).saturating_mul(SAMPLES_PER_MS)).unwrap_or(0);
        let Some(sample_len) = frames.checked_mul(2) else {
            self.master.clear();
            return Vec::new();
        };
        self.master.clear();
        self.master.resize(sample_len, 0);

        let mut ended = Vec::new();
        for voice in &mut self.voices {
            if voice.done {
                continue;
            }
            let total = voice.samples.len() / 2;
            if total == 0 {
                voice.done = true;
                ended.push(voice.object.clone());
                continue;
            }
            let base = usize::try_from(
                t.saturating_sub(voice.start_t)
                    .saturating_add(voice.offset_ms)
                    .saturating_mul(SAMPLES_PER_MS),
            )
            .unwrap_or(usize::MAX);
            if base >= total {
                voice.done = true;
                ended.push(voice.object.clone());
                continue;
            }
            let take = (total - base).min(frames);
            for i in 0..take {
                let Some(left) = voice.samples.get((base + i) * 2).copied() else {
                    break;
                };
                let Some(right) = voice.samples.get((base + i) * 2 + 1).copied() else {
                    break;
                };
                let left = (f32::from(left) * voice.volume).round() as i32;
                let right = (f32::from(right) * voice.volume).round() as i32;
                if let Some(dst) = self.master.get_mut(i * 2) {
                    *dst = sat_add(*dst, left);
                }
                if let Some(dst) = self.master.get_mut(i * 2 + 1) {
                    *dst = sat_add(*dst, right);
                }
            }
            if base.saturating_add(take) >= total {
                voice.done = true;
                ended.push(voice.object.clone());
            }
        }
        self.voices.retain(|voice| !voice.done);
        ended
    }

    /// The samples mixed for the current frame, interleaved stereo.
    pub(crate) fn samples(&self) -> &[i16] {
        &self.master
    }

    fn decode(
        &mut self,
        src: &str,
        media: &dyn ExportMedia,
        warnings: &mut Vec<String>,
    ) -> Option<Arc<[i16]>> {
        if !self.cache.contains_key(src) {
            let entry = decode_source(src, media);
            if let CachedAudio::Failed = entry {
                warnings.push(format!(
                    "audio {src} could not be decoded; the object is silent"
                ));
            }
            self.cache.insert(src.to_owned(), entry);
        }
        match self.cache.get(src) {
            Some(CachedAudio::Ready(samples)) => Some(Arc::clone(samples)),
            Some(CachedAudio::Silent | CachedAudio::Failed) | None => None,
        }
    }
}

/// Decode one source to 48 kHz stereo, or remember why it stayed silent.
fn decode_source(src: &str, media: &dyn ExportMedia) -> CachedAudio {
    let Some(bytes) = media.bytes(src) else {
        return CachedAudio::Failed;
    };
    if bytes.is_empty() {
        return CachedAudio::Failed;
    }
    let suffix = temp_suffix(src);
    let Ok(mut file) = tempfile::Builder::new().suffix(&suffix).tempfile() else {
        return CachedAudio::Failed;
    };
    if file.write_all(&bytes).is_err() {
        return CachedAudio::Failed;
    }
    match VideoFile::decode_audio(file.path()) {
        Ok(Some(pcm)) => {
            if pcm.sample_rate == 0 || pcm.channels == 0 || pcm.samples.is_empty() {
                return CachedAudio::Silent;
            }
            let stereo = resample_to_stereo(&pcm.samples, pcm.channels, pcm.sample_rate);
            if stereo.is_empty() {
                CachedAudio::Silent
            } else {
                CachedAudio::Ready(Arc::from(stereo))
            }
        }
        Ok(None) => CachedAudio::Silent,
        Err(_) => CachedAudio::Failed,
    }
}

/// A safe temp-file suffix taken from the source extension.
///
/// Media Foundation picks its decoder partly from the extension, so the bytes
/// keep the same suffix they arrived with. Anything odd falls back to `.media`.
pub(crate) fn temp_suffix(src: &str) -> String {
    let extension = std::path::Path::new(src)
        .extension()
        .and_then(|extension| extension.to_str())
        .filter(|extension| {
            !extension.is_empty()
                && extension.len() <= 8
                && extension.chars().all(|c| c.is_ascii_alphanumeric())
        });
    match extension {
        Some(extension) => format!(".{}", extension.to_ascii_lowercase()),
        None => ".media".to_owned(),
    }
}

/// Resample interleaved PCM to 48 kHz stereo with linear interpolation.
///
/// Mono is duplicated to both channels and anything above stereo keeps its
/// first two channels. A constant signal stays constant.
pub(crate) fn resample_to_stereo(samples: &[i16], channels: u16, from_rate: u32) -> Vec<i16> {
    let channels = usize::from(channels);
    if channels == 0 || from_rate == 0 {
        return Vec::new();
    }
    let frames = samples.len() / channels;
    if frames == 0 {
        return Vec::new();
    }
    let to_rate = f64::from(SAMPLE_RATE);
    let from_rate_f = f64::from(from_rate);
    let out_frames = if from_rate == SAMPLE_RATE {
        frames
    } else {
        let scaled = (frames as f64) * to_rate / from_rate_f;
        if !scaled.is_finite() || scaled <= 0.0 {
            return Vec::new();
        }
        scaled.round() as usize
    };
    let mut out = Vec::with_capacity(out_frames.saturating_mul(2));
    for i in 0..out_frames {
        let pos = (i as f64) * from_rate_f / to_rate;
        let floor = pos.floor();
        let i0 = floor as usize;
        let frac = (pos - floor).clamp(0.0, 1.0);
        let i1 = (i0 + 1).min(frames.saturating_sub(1));
        let left = lerp(
            sample_at(samples, channels, i0, 0),
            sample_at(samples, channels, i1, 0),
            frac,
        );
        let right = if channels >= 2 {
            lerp(
                sample_at(samples, channels, i0, 1),
                sample_at(samples, channels, i1, 1),
                frac,
            )
        } else {
            left
        };
        out.push(to_i16(left));
        out.push(to_i16(right));
    }
    out
}

fn sample_at(samples: &[i16], channels: usize, frame: usize, channel: usize) -> f64 {
    samples
        .get(frame.saturating_mul(channels).saturating_add(channel))
        .map_or(0.0, |sample| f64::from(*sample))
}

fn lerp(a: f64, b: f64, frac: f64) -> f64 {
    a + (b - a) * frac
}

fn to_i16(value: f64) -> i16 {
    if value.is_nan() {
        0
    } else {
        value
            .round()
            .clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i16
    }
}

fn sat_add(base: i16, add: i32) -> i16 {
    let sum = i32::from(base).saturating_add(add);
    sum.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16
}

/// Saturating add of one buffer into another, for the pure mixing test.
#[cfg(test)]
pub(crate) fn mix_add(dst: &mut [i16], src: &[i16]) {
    for (dst, add) in dst.iter_mut().zip(src.iter()) {
        *dst = sat_add(*dst, i32::from(*add));
    }
}
