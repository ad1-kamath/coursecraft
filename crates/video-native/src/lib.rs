//! CourseCraft video decoding through the operating system (L1).
//!
//! The only crate in the workspace allowed to use `unsafe`, because it calls
//! AVFoundation (macOS) and Media Foundation (Windows). Everything it exports
//! is safe. See `docs/plans/m12-timeline-video.md`.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

use std::path::{Path, PathBuf};

/// Maximum decoded width in pixels unless the caller asks for less.
pub const DEFAULT_MAX_WIDTH: u32 = 1280;
/// Reject any side longer than this, from a hostile file.
pub const MAX_DIMENSION: u32 = 8192;
/// Audio is capped at 60 minutes of 16-bit interleaved PCM.
pub const MAX_AUDIO_MS: u64 = 60 * 60 * 1000;

/// Error from the OS video decoder. Hostile files become `Err`, never a panic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoError(pub String);

#[cfg(windows)]
impl From<windows::core::Error> for VideoError {
    fn from(error: windows::core::Error) -> Self {
        Self(format!("Media Foundation: {error}"))
    }
}

impl std::fmt::Display for VideoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "video decode failed: {}", self.0)
    }
}

impl std::error::Error for VideoError {}

/// One decoded video frame, RGBA with alpha 255.
#[derive(Debug, Clone)]
pub struct VideoFrame {
    /// Presentation time in milliseconds.
    pub pts_ms: u64,
    /// Width in pixels (at most `max_width`).
    pub width: u32,
    /// Height in pixels, aspect preserved.
    pub height: u32,
    /// `width * height * 4` bytes, RGBA, row-major, no row padding.
    pub rgba: Vec<u8>,
}

/// Whole audio track as 16-bit interleaved PCM.
#[derive(Debug, Clone)]
pub struct Pcm {
    /// Sample rate in Hz.
    pub sample_rate: u32,
    /// Channel count.
    pub channels: u16,
    /// Interleaved signed 16-bit samples.
    pub samples: Vec<i16>,
}

/// Scale `(width, height)` so the width is at most `max_width`, keeping the
/// aspect ratio, rejecting zero or over-`MAX_DIMENSION` dimensions.
#[cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]
fn scaled_size(width: u32, height: u32, max_width: u32) -> Result<(u32, u32), VideoError> {
    if width == 0 || height == 0 {
        return Err(VideoError(format!("empty frame {width}x{height}")));
    }
    if width > MAX_DIMENSION || height > MAX_DIMENSION {
        return Err(VideoError(format!("frame too large {width}x{height}")));
    }
    let max_width = max_width.clamp(1, MAX_DIMENSION);
    if width <= max_width {
        return Ok((width, height));
    }
    // Keep the aspect ratio; round to nearest, at least 1.
    let scale = f64::from(max_width) / f64::from(width);
    let h = (f64::from(height) * scale)
        .round()
        .clamp(1.0, f64::from(MAX_DIMENSION)) as u32;
    Ok((max_width, h))
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn ms_from_seconds(seconds: f64) -> u64 {
    if !seconds.is_finite() || seconds <= 0.0 {
        return 0;
    }
    let ms = (seconds * 1000.0).round();
    if ms >= u64::MAX as f64 {
        u64::MAX
    } else {
        ms as u64
    }
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn seconds_from_ms(ms: u64) -> f64 {
    ms as f64 / 1000.0
}

/// Byte offset of the chroma sample for luma pixel `x` in a 4:2:0 plane.
///
/// Chroma is subsampled by 2 in both axes, so two luma pixels share one
/// interleaved Cb/Cr pair: `x` and `x + 1` (for even `x`) both read the
/// pair at `(x / 2) * 2`. Reading `x * 2` walks off the plane past the
/// middle of the row, which is why the right half of a frame used to come
/// out grey (the fallback 128).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn chroma_offset(x: usize) -> usize {
    (x / 2) * 2
}

/// Convert one 8-bit YCbCr pixel to RGB.
///
/// `bt709` selects the BT.709 coefficients (HD); otherwise BT.601 (SD) is
/// used. `full_range` selects the full-range scaling; otherwise the
/// limited-range scaling is used.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn yuv_to_rgb(yv: u8, cb: u8, cr: u8, full_range: bool, bt709: bool) -> (u8, u8, u8) {
    let (yy, cb_c, cr_c) = if full_range {
        (
            f32::from(yv) / 255.0,
            f32::from(cb) / 255.0 - 0.5,
            f32::from(cr) / 255.0 - 0.5,
        )
    } else {
        (
            (f32::from(yv) - 16.0).clamp(0.0, 219.0) / 219.0,
            (f32::from(cb) - 128.0) / 224.0,
            (f32::from(cr) - 128.0) / 224.0,
        )
    };
    let (kr, kg_cb, kg_cr, kb) = if bt709 {
        (1.5748, 0.187324, 0.468124, 1.8556)
    } else {
        (1.402, 0.34414, 0.71414, 1.772)
    };
    let r = (yy + kr * cr_c).clamp(0.0, 1.0);
    let g = (yy - kg_cb * cb_c - kg_cr * cr_c).clamp(0.0, 1.0);
    let b = (yy + kb * cb_c).clamp(0.0, 1.0);
    (
        (r * 255.0).round() as u8,
        (g * 255.0).round() as u8,
        (b * 255.0).round() as u8,
    )
}

/// Safe wrapper around the OS decoder. `&mut self` methods, so a decoder is
/// only ever used from one thread at a time and can move between threads.
pub struct VideoFile {
    path: PathBuf,
    width: u32,
    height: u32,
    duration_ms: u64,
    has_audio: bool,
    max_width: u32,
    #[cfg(target_os = "macos")]
    mac: Option<macos::MacReader>,
    #[cfg(windows)]
    windows: Option<windows_impl::WindowsReader>,
}

impl std::fmt::Debug for VideoFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VideoFile")
            .field("path", &self.path)
            .field("width", &self.width)
            .field("height", &self.height)
            .field("duration_ms", &self.duration_ms)
            .field("has_audio", &self.has_audio)
            .field("max_width", &self.max_width)
            .finish()
    }
}

impl VideoFile {
    /// Open `path` and read its track info. Hostile files error, never panic.
    pub fn open(path: &Path) -> Result<Self, VideoError> {
        #[cfg(target_os = "macos")]
        {
            macos::open(path, DEFAULT_MAX_WIDTH)
        }
        #[cfg(windows)]
        {
            windows_impl::open(path, DEFAULT_MAX_WIDTH)
        }
        #[cfg(not(any(target_os = "macos", windows)))]
        {
            let _ = path;
            Err(VideoError("unsupported".to_string()))
        }
    }

    /// Width in pixels of the video track.
    #[must_use]
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Height in pixels of the video track.
    #[must_use]
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Duration in milliseconds.
    #[must_use]
    pub fn duration_ms(&self) -> u64 {
        self.duration_ms
    }

    /// Whether an audio track exists.
    #[must_use]
    pub fn has_audio(&self) -> bool {
        self.has_audio
    }

    /// Cap decoded frames at `max_width` (aspect preserved). Zero becomes 1.
    pub fn set_max_width(&mut self, max_width: u32) {
        self.max_width = max_width.clamp(1, MAX_DIMENSION);
        #[cfg(target_os = "macos")]
        if self.mac.is_some() {
            self.mac = None;
        }
        #[cfg(windows)]
        if self.windows.is_some() {
            self.windows = None;
        }
    }

    /// Sequential decode from the current position. A new reader is started by
    /// [`Self::seek`]; `None` means end of stream.
    pub fn next_frame(&mut self) -> Result<Option<VideoFrame>, VideoError> {
        #[cfg(target_os = "macos")]
        {
            macos::next_frame(self)
        }
        #[cfg(windows)]
        {
            windows_impl::next_frame(self)
        }
        #[cfg(not(any(target_os = "macos", windows)))]
        {
            Err(VideoError("unsupported".to_string()))
        }
    }

    /// Seek to `ms` and reset sequential decoding from there.
    pub fn seek(&mut self, ms: u64) -> Result<(), VideoError> {
        let ms = ms.min(self.duration_ms.saturating_add(1_000_000));
        #[cfg(target_os = "macos")]
        {
            macos::seek(self, ms)
        }
        #[cfg(windows)]
        {
            windows_impl::seek(self, ms)
        }
        #[cfg(not(any(target_os = "macos", windows)))]
        {
            let _ = ms;
            Err(VideoError("unsupported".to_string()))
        }
    }

    /// Seek and decode forward to the frame shown at `ms` (one frame tolerance).
    pub fn frame_at(&mut self, ms: u64) -> Result<Option<VideoFrame>, VideoError> {
        // Seeking to exactly the duration lands past the last frame; step
        // back 1 ms so the final frame is returned instead of `None`.
        let ms = if ms >= self.duration_ms && self.duration_ms > 0 {
            self.duration_ms.saturating_sub(1)
        } else {
            ms
        };
        self.seek(ms)?;
        let mut last: Option<VideoFrame> = None;
        loop {
            match self.next_frame()? {
                Some(frame) => {
                    let pts = frame.pts_ms;
                    last = Some(frame);
                    if pts >= ms {
                        return Ok(last);
                    }
                }
                None => return Ok(last),
            }
        }
    }

    /// Decode the whole audio track as 16-bit interleaved PCM, capped at 60 minutes.
    pub fn decode_audio(path: &Path) -> Result<Option<Pcm>, VideoError> {
        #[cfg(target_os = "macos")]
        {
            macos::decode_audio(path)
        }
        #[cfg(windows)]
        {
            windows_impl::decode_audio(path)
        }
        #[cfg(not(any(target_os = "macos", windows)))]
        {
            let _ = path;
            Err(VideoError("unsupported".to_string()))
        }
    }
}

// SAFETY: `VideoFile` methods all take `&mut self`, so no two threads can use
// one decoder concurrently; moving it to another thread is therefore safe.
unsafe impl Send for VideoFile {}

#[cfg(target_os = "macos")]
mod macos {
    use super::{
        MAX_AUDIO_MS, MAX_DIMENSION, Pcm, VideoError, VideoFile, VideoFrame, chroma_offset,
        ms_from_seconds, scaled_size, seconds_from_ms, yuv_to_rgb,
    };
    use std::path::Path;

    use objc2::rc::{Retained, autoreleasepool};
    use objc2::runtime::AnyObject;
    use objc2_av_foundation::{
        AVAsset, AVAssetReader, AVAssetReaderTrackOutput, AVAssetTrack, AVMediaTypeAudio,
        AVMediaTypeVideo,
    };
    use objc2_core_media::{CMSampleBuffer, CMTime, CMTimeRange};
    use objc2_core_video::{
        CVBuffer, CVPixelBuffer, CVPixelBufferGetBaseAddress, CVPixelBufferGetBaseAddressOfPlane,
        CVPixelBufferGetBytesPerRow, CVPixelBufferGetBytesPerRowOfPlane, CVPixelBufferGetDataSize,
        CVPixelBufferGetHeight, CVPixelBufferGetHeightOfPlane, CVPixelBufferGetPixelFormatType,
        CVPixelBufferGetPlaneCount, CVPixelBufferGetWidth, CVPixelBufferLockBaseAddress,
        CVPixelBufferUnlockBaseAddress, kCVImageBufferYCbCrMatrix_ITU_R_601_4,
        kCVImageBufferYCbCrMatrix_ITU_R_709_2, kCVImageBufferYCbCrMatrixKey,
        kCVPixelBufferPixelFormatTypeKey, kCVPixelFormatType_32BGRA, kCVPixelFormatType_32RGBA,
        kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,
        kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange, kCVPixelFormatType_420YpCbCr8Planar,
        kCVPixelFormatType_420YpCbCr8PlanarFullRange,
    };
    use objc2_foundation::{NSDictionary, NSNumber, NSString, NSURL};

    pub struct MacReader {
        reader: Retained<AVAssetReader>,
        output: Retained<AVAssetReaderTrackOutput>,
        out_width: u32,
        out_height: u32,
    }

    fn file_url(path: &Path) -> Retained<NSURL> {
        let s = path.to_string_lossy();
        // SAFETY: path is only read; NSURL is a value type retained by the caller.
        let path_str = NSString::from_str(&s);
        NSURL::fileURLWithPath(&path_str)
    }

    fn video_media_type() -> Option<&'static objc2_av_foundation::AVMediaType> {
        // SAFETY: the static is a valid media type or null if unavailable.
        unsafe { AVMediaTypeVideo }
    }

    fn audio_media_type() -> Option<&'static objc2_av_foundation::AVMediaType> {
        // SAFETY: the static is a valid media type or null if unavailable.
        unsafe { AVMediaTypeAudio }
    }

    fn first_track(
        asset: &AVAsset,
        media_type: Option<&objc2_av_foundation::AVMediaType>,
    ) -> Option<Retained<AVAssetTrack>> {
        let media_type = media_type?;
        // tracksWithMediaType blocks until loaded synchronously (we run on a
        // worker/test thread, never the UI thread).
        // SAFETY: `asset` is a live AVAsset; the call blocks until loaded.
        #[allow(deprecated)]
        let tracks = unsafe { asset.tracksWithMediaType(media_type) };
        // SAFETY: NSDate-like search: firstObject is safe.
        tracks.firstObject()
    }

    fn nsobj(value: Retained<NSNumber>) -> Retained<objc2::runtime::AnyObject> {
        value.into_super().into_super().into_super()
    }

    /// Run `f` (kept as a seam for an exception guard; without the
    /// `exception` feature ObjC exceptions would abort, so `f` must avoid
    /// throwing calls by validation instead).
    fn run<T>(f: impl FnOnce() -> Result<T, VideoError>) -> Result<T, VideoError> {
        f()
    }

    pub fn open(path: &Path, max_width: u32) -> Result<VideoFile, VideoError> {
        run(|| {
            autoreleasepool(|_pool| {
                if !path.is_file() {
                    return Err(VideoError(format!("not a file: {}", path.display())));
                }
                let url = file_url(path);
                // SAFETY: `url` is a live valid file URL.
                let asset = unsafe { AVAsset::assetWithURL(&url) };
                let Some(video) = first_track(&asset, video_media_type()) else {
                    return Err(VideoError("no video track".to_string()));
                };
                // SAFETY: track is live, naturalSize is a pure getter.
                let natural = unsafe { video.naturalSize() };
                let (width_f, height_f) = (natural.width, natural.height);
                if !(width_f.is_finite() && height_f.is_finite() && width_f > 0.0 && height_f > 0.0)
                {
                    return Err(VideoError(format!("bad natural size {width_f}x{height_f}")));
                }
                let width = width_f.round() as u32;
                let height = height_f.round() as u32;
                scaled_size(width, height, max_width)?;
                // SAFETY: duration is a pure getter; we validate the CMTime flags.
                let duration = unsafe { asset.duration() };
                let duration_ms = cm_duration_ms(duration);
                let has_audio = first_track(&asset, audio_media_type()).is_some();
                Ok(VideoFile {
                    path: path.to_path_buf(),
                    width,
                    height,
                    duration_ms,
                    has_audio,
                    max_width,
                    mac: None,
                })
            })
        })
    }

    fn cm_duration_ms(duration: CMTime) -> u64 {
        // `seconds` is a safe pure conversion; NaN/negative become 0 via ms_from_seconds.
        // SAFETY: CMTime is a value struct; converting to seconds only reads it.
        let seconds = unsafe { duration.seconds() };
        ms_from_seconds(seconds)
    }

    pub fn seek(file: &mut VideoFile, ms: u64) -> Result<(), VideoError> {
        run(|| {
            autoreleasepool(|_pool| {
                let path = file.path.clone();
                let (width, height) = (file.width, file.height);
                let max_width = file.max_width;
                let (out_width, out_height) = scaled_size(width, height, max_width)?;
                let url = file_url(&path);
                // SAFETY: `url` is live.
                let asset = unsafe { AVAsset::assetWithURL(&url) };
                let Some(track) = first_track(&asset, video_media_type()) else {
                    return Err(VideoError("no video track on seek".to_string()));
                };
                // Wait briefly for the track's format to load (async file parsing):
                // reader creation throws if no format is available yet. This runs
                // on the worker thread / test thread, never the UI thread.
                for _ in 0..200 {
                    // SAFETY: track is live; formatDescriptions blocks until loaded.
                    let formats = unsafe { track.formatDescriptions() };
                    let count = formats.count();
                    if count > 0 {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                // SAFETY: creating a reader from a valid asset/track cannot crash
                // on a hostile file; it returns an error which we propagate.
                let reader = unsafe { AVAssetReader::assetReaderWithAsset_error(&asset) }
                    .map_err(|e| VideoError(format!("reader: {e:?}")))?;
                // Decompressed BGRA at natural size via nested decompression
                // properties (top-level pixel-format/width keys throw on macOS);
                // max_width downscale happens in Rust after decode.
                let settings: Retained<NSDictionary<NSString, AnyObject>> = {
                    // The real `kCVPixelBufferPixelFormatTypeKey` constant (a
                    // CFString, toll-free bridged to NSString). Using the
                    // constant's *name* as the key ("kCVPixelBufferPixelFormatTypeKey")
                    // made AVFoundation ignore the BGRA request and hand back
                    // the native YUV format instead.
                    // SAFETY: the static CFString is a valid NSString
                    // (toll-free bridge) for the lifetime of the static.
                    let pf_key: &NSString = unsafe {
                        &*(kCVPixelBufferPixelFormatTypeKey as *const _ as *const NSString)
                    };
                    let pf = nsobj(NSNumber::new_u32(kCVPixelFormatType_32BGRA));
                    let inner: Retained<NSDictionary<NSString, AnyObject>> =
                        NSDictionary::from_slices(&[pf_key], &[&*pf]);
                    let decomp_key = NSString::from_str("AVVideoDecompressionPropertiesKey");
                    let decomp_any: Retained<AnyObject> = inner.into_super().into_super();
                    NSDictionary::from_slices(&[&*decomp_key], &[&*decomp_any])
                };
                // SAFETY: track is from the same asset.
                let output = unsafe {
                    AVAssetReaderTrackOutput::assetReaderTrackOutputWithTrack_outputSettings(
                        &track,
                        Some(&settings),
                    )
                };
                // SAFETY: reader/output are live and compatible.
                unsafe { reader.addOutput(&output) };
                // SAFETY: CMTimeMakeWithSeconds only reads its value arguments.
                let start = unsafe { CMTime::with_seconds(seconds_from_ms(ms), 600) };
                let range_dur = if file.duration_ms > ms {
                    // SAFETY: CMTimeMakeWithSeconds only reads its value arguments.
                    unsafe { CMTime::with_seconds(seconds_from_ms(file.duration_ms - ms), 600) }
                } else {
                    // SAFETY: CMTimeMakeWithSeconds only reads its value arguments.
                    unsafe { CMTime::with_seconds(1.0, 600) }
                };
                // SAFETY: CMTimeRangeMake only reads its value arguments.
                let range = unsafe { CMTimeRange::new(start, range_dur) };
                // SAFETY: setting the range before reading has started.
                unsafe { reader.setTimeRange(range) };
                // SAFETY: startReading validates settings and returns false instead of crashing.
                let _ = unsafe { reader.startReading() };
                file.mac = Some(MacReader {
                    reader,
                    output,
                    out_width,
                    out_height,
                });
                Ok(())
            })
        })
    }

    pub fn next_frame(file: &mut VideoFile) -> Result<Option<VideoFrame>, VideoError> {
        run(|| {
            autoreleasepool(|_pool| {
                let Some(mac) = file.mac.as_ref() else {
                    // No reader yet: start at 0.
                    file.seek(0)?;
                    return next_frame(file);
                };
                // Skip marker-only samples (no image buffer) up to a bounded count.
                for _ in 0..1000 {
                    // SAFETY: copyNextSampleBuffer returns a retained or null sample;
                    // the output is attached to the reader and reading has started.
                    let sample = unsafe { mac.output.copyNextSampleBuffer() };
                    let Some(sample) = sample else {
                        // SAFETY: status is a pure getter.
                        let status = unsafe { mac.reader.status() };
                        let status = status.0;
                        if status == 2 {
                            return Ok(None); // Completed
                        }
                        if status == 4 {
                            return Ok(None); // Cancelled
                        }
                        return Err(VideoError(format!("reader status {status}")));
                    };

                    // SAFETY: presentationTimeStamp only reads the sample.
                    let pts_time = unsafe { sample.presentation_time_stamp() };
                    // SAFETY: `seconds` only reads the CMTime value.
                    let pts_seconds = unsafe { pts_time.seconds() };
                    let pts_ms = ms_from_seconds(pts_seconds);

                    // SAFETY: imageBuffer returns a retained buffer or null for marker samples.
                    let image = unsafe { sample.image_buffer() };
                    let Some(image) = image else {
                        continue;
                    };
                    return frame_from_pixel_buffer(&image, pts_ms, mac.out_width, mac.out_height)
                        .map(Some);
                }
                Err(VideoError("too many marker samples".to_string()))
            })
        })
    }

    fn frame_from_pixel_buffer(
        image: &CVPixelBuffer,
        pts_ms: u64,
        _want_width: u32,
        _want_height: u32,
    ) -> Result<VideoFrame, VideoError> {
        let format = CVPixelBufferGetPixelFormatType(image);
        let width = CVPixelBufferGetWidth(image);
        let height = CVPixelBufferGetHeight(image);
        if width == 0
            || height == 0
            || width > MAX_DIMENSION as usize
            || height > MAX_DIMENSION as usize
        {
            return Err(VideoError(format!("bad buffer size {width}x{height}")));
        }
        let total = (width as u64)
            .saturating_mul(height as u64)
            .saturating_mul(4);
        if total > 256 * 1024 * 1024 {
            return Err(VideoError("frame allocation too large".to_string()));
        }
        // SAFETY: lock the buffer read-only; we unlock on every path below.
        let locked = unsafe {
            CVPixelBufferLockBaseAddress(image, objc2_core_video::CVPixelBufferLockFlags::ReadOnly)
        };
        if locked != objc2_core_video::kCVReturnSuccess as _ && locked != 0 {
            // CVReturn 0 == success; compare loosely. A failed lock yields a
            // null base address, handled next.
        }
        let result = (|| -> Result<Vec<u8>, VideoError> {
            if format == kCVPixelFormatType_32BGRA || format == kCVPixelFormatType_32RGBA {
                let bytes_per_row = CVPixelBufferGetBytesPerRow(image);
                if bytes_per_row < width.saturating_mul(4) {
                    return Err(VideoError(format!(
                        "bytes-per-row {bytes_per_row} too small"
                    )));
                }
                let data_size = CVPixelBufferGetDataSize(image);
                let need = bytes_per_row.saturating_mul(height);
                if data_size != 0 && data_size < need {
                    return Err(VideoError(format!("buffer too small {data_size} < {need}")));
                }
                let base = CVPixelBufferGetBaseAddress(image);
                if base.is_null() {
                    return Err(VideoError("null base address".to_string()));
                }
                // SAFETY: base points to `need` readable bytes while locked.
                let src: &[u8] = unsafe { std::slice::from_raw_parts(base as *const u8, need) };
                if src.len() < need {
                    return Err(VideoError("short pixel buffer".to_string()));
                }
                let mut rgba = Vec::with_capacity(total as usize);
                for row in 0..height {
                    let row_start = row.saturating_mul(bytes_per_row);
                    let row_end = row_start.saturating_add(width.saturating_mul(4));
                    let Some(row_bytes) = src.get(row_start..row_end) else {
                        return Err(VideoError("short row".to_string()));
                    };
                    #[allow(clippy::explicit_iter_loop)]
                    for chunk in row_bytes.chunks(4) {
                        if chunk.len() < 4 {
                            return Err(VideoError("short pixel chunk".to_string()));
                        }
                        if format == kCVPixelFormatType_32BGRA {
                            rgba.push(chunk[2]);
                            rgba.push(chunk[1]);
                            rgba.push(chunk[0]);
                        } else {
                            rgba.push(chunk[0]);
                            rgba.push(chunk[1]);
                            rgba.push(chunk[2]);
                        }
                        rgba.push(255);
                    }
                }
                if rgba.len() as u64 != total {
                    return Err(VideoError("rgba length mismatch".to_string()));
                }
                Ok(rgba)
            } else if format == kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange
                || format == kCVPixelFormatType_420YpCbCr8BiPlanarFullRange
                || format == kCVPixelFormatType_420YpCbCr8Planar
                || format == kCVPixelFormatType_420YpCbCr8PlanarFullRange
            {
                let full_range = format == kCVPixelFormatType_420YpCbCr8BiPlanarFullRange
                    || format == kCVPixelFormatType_420YpCbCr8PlanarFullRange;
                let bt709 = ycbcr_is_bt709(image, width, height);
                convert_yuv_biplanar(image, width, height, full_range, bt709)
            } else {
                Err(VideoError(format!("pixel format {format:#x} unsupported")))
            }
        })();
        // SAFETY: the buffer was locked above and is still live; always unlock.
        unsafe {
            CVPixelBufferUnlockBaseAddress(
                image,
                objc2_core_video::CVPixelBufferLockFlags::ReadOnly,
            )
        };
        let rgba = result?;
        let (width, height) = (width as u32, height as u32);
        // Downscale to max_width in Rust (native decode is unscaled).
        let (target_w, target_h) = if width > _want_width || height > _want_height {
            scaled_size(width, height, _want_width.max(1)).unwrap_or((width, height))
        } else {
            (width, height)
        };
        let rgba = if target_w == width && target_h == height {
            rgba
        } else {
            downscale_nearest(&rgba, width, height, target_w, target_h)?
        };
        Ok(VideoFrame {
            pts_ms,
            width: target_w,
            height: target_h,
            rgba,
        })
    }

    fn downscale_nearest(
        src: &[u8],
        src_w: u32,
        src_h: u32,
        dst_w: u32,
        dst_h: u32,
    ) -> Result<Vec<u8>, VideoError> {
        if dst_w == 0 || dst_h == 0 {
            return Err(VideoError("empty downscale".to_string()));
        }
        let src_len = (src_w as u64) * (src_h as u64) * 4;
        if src.len() as u64 != src_len {
            return Err(VideoError("downscale length mismatch".to_string()));
        }
        let mut out = Vec::with_capacity((dst_w as usize) * (dst_h as usize) * 4);
        for y in 0..dst_h {
            let src_y = (y as u64 * src_h as u64 / dst_h as u64) as usize;
            for x in 0..dst_w {
                let src_x = (x as u64 * src_w as u64 / dst_w as u64) as usize;
                let idx = (src_y * src_w as usize * 4) + src_x * 4;
                let chunk = src
                    .get(idx..idx + 4)
                    .ok_or_else(|| VideoError("downscale oob".to_string()))?;
                out.extend_from_slice(chunk);
            }
        }
        Ok(out)
    }

    /// Whether a YUV buffer uses the BT.709 matrix.
    ///
    /// The buffer's `kCVImageBufferYCbCrMatrixKey` attachment wins when
    /// present; otherwise HD frames default to BT.709 and SD frames to
    /// BT.601.
    fn ycbcr_is_bt709(image: &CVPixelBuffer, width: usize, height: usize) -> bool {
        // SAFETY: a CVPixelBuffer is a CVBuffer; the pointer is unchanged by
        // the cast, and the buffer outlives this call.
        let buffer: &CVBuffer = unsafe { &*(image as *const CVPixelBuffer) };
        // SAFETY: `buffer` is live; a null mode pointer is allowed.
        let attachment =
            unsafe { buffer.attachment(kCVImageBufferYCbCrMatrixKey, std::ptr::null_mut()) };
        if let Some(attachment) = attachment {
            let got: &objc2_core_foundation::CFType = &attachment;
            // SAFETY: the statics are valid CFStrings (CFTypes) for 'static.
            let bt709: &objc2_core_foundation::CFType =
                unsafe { &*(kCVImageBufferYCbCrMatrix_ITU_R_709_2 as *const _ as *const _) };
            // SAFETY: the statics are valid CFStrings (CFTypes) for 'static.
            let bt601: &objc2_core_foundation::CFType =
                unsafe { &*(kCVImageBufferYCbCrMatrix_ITU_R_601_4 as *const _ as *const _) };
            if *got == *bt709 {
                return true;
            }
            if *got == *bt601 {
                return false;
            }
        }
        height >= 720 || width >= 1280
    }

    fn convert_yuv_biplanar(
        image: &CVPixelBuffer,
        width: usize,
        height: usize,
        full_range: bool,
        bt709: bool,
    ) -> Result<Vec<u8>, VideoError> {
        let planes = CVPixelBufferGetPlaneCount(image);
        if planes < 2 {
            return Err(VideoError("yuv planes < 2".to_string()));
        }
        let y_base = CVPixelBufferGetBaseAddressOfPlane(image, 0);
        let y_row = CVPixelBufferGetBytesPerRowOfPlane(image, 0);
        let y_plane_h = CVPixelBufferGetHeightOfPlane(image, 0);
        let cb_base = CVPixelBufferGetBaseAddressOfPlane(image, 1);
        let cb_row = CVPixelBufferGetBytesPerRowOfPlane(image, 1);
        let cb_plane_h = CVPixelBufferGetHeightOfPlane(image, 1);
        let chroma_row_bytes = width.div_ceil(2) * 2;
        if y_base.is_null() || cb_base.is_null() || y_row < width || cb_row < chroma_row_bytes {
            return Err(VideoError("yuv plane pointer/stride invalid".to_string()));
        }
        if y_plane_h < height || cb_plane_h < height / 2 {
            return Err(VideoError("yuv plane height invalid".to_string()));
        }
        let total = width.saturating_mul(height).saturating_mul(4);
        let mut rgba = Vec::with_capacity(total);
        // SAFETY: plane bases point to y_row*y_plane_h / cb_row*cb_plane_h
        // readable bytes while locked; we read row by row with bounds checks.
        // SAFETY: raw pointer is non-null and points to readable plane data.
        let y_data: &[u8] = unsafe {
            std::slice::from_raw_parts(y_base as *const u8, y_row.saturating_mul(y_plane_h))
        };
        // SAFETY: raw pointer is non-null and points to readable plane data.
        let cb_data: &[u8] = unsafe {
            std::slice::from_raw_parts(cb_base as *const u8, cb_row.saturating_mul(cb_plane_h))
        };
        for y in 0..height {
            let y_start = y.saturating_mul(y_row);
            let y_end = y_start.saturating_add(width);
            let Some(y_line) = y_data.get(y_start..y_end) else {
                return Err(VideoError("short y row".to_string()));
            };
            let cb_y = y / 2;
            let cb_start = cb_y.saturating_mul(cb_row);
            let cb_end = cb_start.saturating_add(chroma_row_bytes);
            let Some(cb_line) = cb_data.get(cb_start..cb_end) else {
                return Err(VideoError("short cb row".to_string()));
            };
            for x in 0..width {
                let yv = *y_line.get(x).unwrap_or(&0);
                let off = chroma_offset(x);
                let cb = *cb_line.get(off).unwrap_or(&128);
                let cr = *cb_line.get(off + 1).unwrap_or(&128);
                let (r, g, b) = yuv_to_rgb(yv, cb, cr, full_range, bt709);
                rgba.push(r);
                rgba.push(g);
                rgba.push(b);
                rgba.push(255);
            }
        }
        Ok(rgba)
    }

    pub fn decode_audio(path: &Path) -> Result<Option<Pcm>, VideoError> {
        run(|| {
            autoreleasepool(|_pool| {
                let url = file_url(path);
                // SAFETY: `url` is live.
                let asset = unsafe { AVAsset::assetWithURL(&url) };
                let Some(track) = first_track(&asset, audio_media_type()) else {
                    return Ok(None);
                };
                // Force format loading before reader creation (blocks until loaded).
                // SAFETY: track is live; formatDescriptions blocks until loaded.
                let _ = unsafe { track.formatDescriptions() };
                // Wait briefly for the track's format to load (async file parsing):
                // output-settings creation throws if no format is available yet. This runs
                // on the worker thread / test thread, never the UI thread.
                for _ in 0..200 {
                    // SAFETY: track is live; formatDescriptions blocks until loaded.
                    let formats = unsafe { track.formatDescriptions() };
                    if formats.count() > 0 {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                // SAFETY: asset/track are live.
                let reader = unsafe { AVAssetReader::assetReaderWithAsset_error(&asset) }
                    .map_err(|e| VideoError(format!("audio reader: {e:?}")))?;

                // Decode compressed audio (e.g. AAC) to PCM through AVFoundation.
                // `AVAssetReaderTrackOutput` with Linear PCM output settings
                // returns 16-bit interleaved PCM directly, so no float
                // conversion is needed. The sample rate and channel count are
                // read from the output's sample buffers.
                //
                // The previous `AVAssetReaderAudioMixOutput` with nil settings
                // stalled in `startReading` (measured ~12s on this machine, and
                // ~30s total with asset loading on a cold process, versus
                // microseconds for `copyNextSampleBuffer` once reading starts).
                // A track output with explicit PCM settings starts reading
                // without that audio-mix timeout once AVFoundation's first-use
                // codec init (paid on the first asset/conversion in a process)
                // has happened.
                let settings: Retained<NSDictionary<NSString, AnyObject>> = {
                    // kAudioFormatLinearPCM = 'lpcm' = 0x6c70_636d.
                    // Keys are the AVAudioSettings constants: AVFormatIDKey,
                    // AVLinearPCMBitDepthKey, AVLinearPCMIsFloatKey,
                    // AVLinearPCMIsBigEndianKey and
                    // AVLinearPCMIsNonInterleaved (whose string value has no
                    // "Key" suffix).
                    let keys = [
                        NSString::from_str("AVFormatIDKey"),
                        NSString::from_str("AVLinearPCMBitDepthKey"),
                        NSString::from_str("AVLinearPCMIsFloatKey"),
                        NSString::from_str("AVLinearPCMIsBigEndianKey"),
                        NSString::from_str("AVLinearPCMIsNonInterleaved"),
                    ];
                    let values = [
                        nsobj(NSNumber::new_u32(0x6c70_636d)),
                        nsobj(NSNumber::new_u32(16)),
                        nsobj(NSNumber::new_bool(false)),
                        nsobj(NSNumber::new_bool(false)),
                        nsobj(NSNumber::new_bool(false)),
                    ];
                    NSDictionary::from_slices(
                        &keys.iter().map(|k| &**k).collect::<Vec<_>>(),
                        &values.iter().map(|v| &**v).collect::<Vec<_>>(),
                    )
                };
                // SAFETY: track is from the same asset; settings request
                // Linear PCM so output is uncompressed 16-bit interleaved.
                let output = unsafe {
                    AVAssetReaderTrackOutput::assetReaderTrackOutputWithTrack_outputSettings(
                        &track,
                        Some(&settings),
                    )
                };
                // SAFETY: output is compatible.
                unsafe { reader.addOutput(&output) };
                // SAFETY: starts reading or returns false with an error status.
                let started = unsafe { reader.startReading() };
                if !started {
                    return Err(VideoError("audio startReading failed".to_string()));
                }

                let mut collected: Vec<i16> = Vec::new();
                let mut sample_rate_out: u32 = 0;
                let mut channels_out: u16 = 0;
                loop {
                    // SAFETY: output/reader are live and reading.
                    let Some(sample) = (unsafe { output.copyNextSampleBuffer() }) else {
                        break;
                    };
                    let (rate, ch) = audio_format_details(&sample);
                    if rate != 0 {
                        sample_rate_out = rate;
                    }
                    if ch != 0 {
                        channels_out = ch;
                    }
                    // Raw audio-buffer extraction without naming
                    // AudioBufferList (objc2-core-audio-types is not a direct dep).
                    let (mut block_buffer, mut buffer_list) = raw_audio_bytes(&sample)?;
                    if buffer_list.is_empty() {
                        // Release retained block buffer before continuing.
                        if !block_buffer.is_null() {
                            // SAFETY: block_buffer is a valid reference.
                            unsafe { cf_release(block_buffer) };
                        }
                        continue;
                    }
                    if append_audio_bytes(&buffer_list, &mut collected).is_err() {
                        // Unexpected format; skip this sample.
                        if !block_buffer.is_null() {
                            // SAFETY: block_buffer is a valid reference.
                            unsafe { cf_release(block_buffer) };
                        }
                        continue;
                    }
                    if !block_buffer.is_null() {
                        // SAFETY: block_buffer is a valid reference.
                        unsafe { cf_release(block_buffer) };
                    }
                    // Cap at 60 minutes of 16-bit PCM. Use the rate/channels
                    // read from the output when known, else a worst-case bound.
                    let worst_case = (60 * 60 * 48000 * 8) as usize;
                    let cap = if sample_rate_out != 0 && channels_out != 0 {
                        ((MAX_AUDIO_MS / 1000)
                            .saturating_mul(u64::from(sample_rate_out))
                            .saturating_mul(u64::from(channels_out)))
                        .min(u64::MAX as usize as u64) as usize
                    } else {
                        worst_case
                    };
                    if collected.len() >= cap {
                        break;
                    }
                    let _ = &mut block_buffer;
                    let _ = &mut buffer_list;
                }
                if collected.is_empty() {
                    return Ok(None);
                }
                let sample_rate_out = if sample_rate_out == 0 {
                    48000
                } else {
                    sample_rate_out
                };
                let channels_out = if channels_out == 0 { 2 } else { channels_out };
                Ok(Some(Pcm {
                    sample_rate: sample_rate_out,
                    channels: channels_out,
                    samples: collected,
                }))
            })
        })
    }

    // Copy the sample's audio bytes, returning the retained block buffer (or
    // null) and the raw AudioBufferList bytes.
    fn raw_audio_bytes(
        sample: &CMSampleBuffer,
    ) -> Result<(*mut core::ffi::c_void, Vec<u8>), VideoError> {
        unsafe extern "C-unwind" {
            fn CMSampleBufferGetAudioBufferListWithRetainedBlockBuffer(
                sbuf: *const core::ffi::c_void,
                buffer_list_size_needed_out: *mut usize,
                buffer_list_out: *mut core::ffi::c_void,
                buffer_list_size: usize,
                block_buffer_structure_allocator: *const core::ffi::c_void,
                block_buffer_block_allocator: *const core::ffi::c_void,
                flags: u32,
                block_buffer_out: *mut *mut core::ffi::c_void,
            ) -> i32;
        }
        let sbuf = sample as *const CMSampleBuffer as *const core::ffi::c_void;
        let mut needed: usize = 0;
        let mut block: *mut core::ffi::c_void = std::ptr::null_mut();
        // SAFETY: `sbuf` points to a live sample; out params are valid slots.
        let status = unsafe {
            CMSampleBufferGetAudioBufferListWithRetainedBlockBuffer(
                sbuf,
                &mut needed,
                std::ptr::null_mut(),
                0,
                std::ptr::null(),
                std::ptr::null(),
                0,
                &mut block,
            )
        };
        if status != 0 || needed == 0 || needed > 16 * 1024 * 1024 {
            if !block.is_null() {
                // SAFETY: block is a valid retained reference.
                unsafe { cf_release(block) };
            }
            return Ok((std::ptr::null_mut(), Vec::new()));
        }
        let mut bytes = vec![0u8; needed];
        let mut block2: *mut core::ffi::c_void = std::ptr::null_mut();
        // SAFETY: bytes has `needed` writable bytes; block2 is a valid slot.
        let status = unsafe {
            CMSampleBufferGetAudioBufferListWithRetainedBlockBuffer(
                sbuf,
                std::ptr::null_mut(),
                bytes.as_mut_ptr() as *mut core::ffi::c_void,
                needed,
                std::ptr::null(),
                std::ptr::null(),
                0,
                &mut block2,
            )
        };
        if status != 0 {
            if !block2.is_null() {
                // SAFETY: valid reference.
                unsafe { cf_release(block2) };
            }
            if !block.is_null() {
                // SAFETY: valid reference.
                unsafe { cf_release(block) };
            }
            return Ok((std::ptr::null_mut(), Vec::new()));
        }
        // Return the second retained block (first one was a size query with a
        // null buffer, which does not retain), prefer block2.
        if !block2.is_null() {
            Ok((block2, bytes))
        } else {
            Ok((block, bytes))
        }
    }

    // SAFETY: `ptr` is null or a valid CF reference to release.
    unsafe fn cf_release(ptr: *mut core::ffi::c_void) {
        unsafe extern "C-unwind" {
            fn CFRelease(ref_: *const core::ffi::c_void);
        }
        // SAFETY: caller guarantees validity.
        unsafe { CFRelease(ptr) }
    }

    fn append_audio_bytes(bytes: &[u8], out: &mut Vec<i16>) -> Result<(), VideoError> {
        // AudioBufferList: u32 mNumberBuffers at 0, buffers start at offset 8
        // (4-byte count + 4-byte padding for 8-byte alignment on 64-bit).
        // Each AudioBuffer: {u32 channels, u32 byte_size, *data} = 16 bytes.
        if bytes.len() < 8 {
            return Err(VideoError("audio list too small".to_string()));
        }
        let number_buffers = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
        if number_buffers == 0 || number_buffers > 1024 {
            return Err(VideoError(format!("bad buffer count {number_buffers}")));
        }
        let mut offset = 8usize;
        for _ in 0..number_buffers {
            if offset.saturating_add(16) > bytes.len() {
                return Err(VideoError("audio list truncated".to_string()));
            }
            let byte_size = u32::from_le_bytes([
                bytes[offset + 4],
                bytes[offset + 5],
                bytes[offset + 6],
                bytes[offset + 7],
            ]) as usize;
            let data_ptr = usize::from_le_bytes([
                bytes[offset + 8],
                bytes[offset + 9],
                bytes[offset + 10],
                bytes[offset + 11],
                bytes[offset + 12],
                bytes[offset + 13],
                bytes[offset + 14],
                bytes[offset + 15],
            ]) as *const u8;
            offset = offset.saturating_add(16);
            if byte_size == 0 || data_ptr.is_null() {
                continue;
            }
            if !byte_size.is_multiple_of(2) {
                return Err(VideoError("odd audio byte size".to_string()));
            }
            if byte_size > 16 * 1024 * 1024 {
                return Err(VideoError("audio buffer too large".to_string()));
            }
            // SAFETY: data_ptr points to byte_size readable bytes owned by the sample.
            let raw: &[u8] = unsafe { std::slice::from_raw_parts(data_ptr, byte_size) };
            for pair in raw.chunks(2) {
                if pair.len() == 2 {
                    out.push(i16::from_le_bytes([pair[0], pair[1]]));
                }
            }
        }
        Ok(())
    }

    fn audio_format_details(sample: &CMSampleBuffer) -> (u32, u16) {
        // SAFETY: format description is retained or null; we treat null as unknown.
        let Some(desc) = (unsafe { sample.format_description() }) else {
            return (0, 0);
        };
        // SAFETY: the description is a valid CMAudioFormatDescription.
        let asbdesc =
            unsafe { objc2_core_media::CMAudioFormatDescriptionGetStreamBasicDescription(&desc) };
        if asbdesc.is_null() {
            return (0, 0);
        }
        // SAFETY: asbdesc is a valid AudioStreamBasicDescription; sample rate at
        // offset 0, channels-per-frame at 28.
        let bytes: &[u8] = unsafe { std::slice::from_raw_parts(asbdesc as *const u8, 44) };
        let mut rate_bytes = [0u8; 8];
        rate_bytes.copy_from_slice(&bytes[0..8]);
        let rate_f = f64::from_le_bytes(rate_bytes);
        let mut ch_bytes = [0u8; 4];
        ch_bytes.copy_from_slice(&bytes[28..32]);
        let channels = u32::from_le_bytes(ch_bytes);
        let rate = if rate_f.is_finite() && rate_f > 0.0 && rate_f < 1_000_000.0 {
            rate_f as u32
        } else {
            0
        };
        (rate, channels.min(8) as u16)
    }
}

#[cfg(windows)]
mod windows_impl {
    use super::{MAX_AUDIO_MS, Pcm, VideoError, VideoFile, VideoFrame, scaled_size};
    use std::path::Path;

    use windows::Win32::Media::MediaFoundation::{
        IMFAttributes, IMFMediaBuffer, IMFMediaType, IMFSample, IMFSourceReader,
        MF_MT_AUDIO_BITS_PER_SAMPLE, MF_MT_AUDIO_NUM_CHANNELS, MF_MT_AUDIO_SAMPLES_PER_SECOND,
        MF_MT_DEFAULT_STRIDE, MF_MT_FRAME_SIZE, MF_MT_MAJOR_TYPE, MF_MT_SUBTYPE, MF_PD_DURATION,
        MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING, MF_SOURCE_READER_MEDIASOURCE,
        MF_SOURCE_READERF_ENDOFSTREAM, MF_SOURCE_READERF_ERROR, MF_SOURCE_READERF_STREAMTICK,
        MFAudioFormat_PCM, MFCreateAttributes, MFCreateMediaType, MFCreateSourceReaderFromURL,
        MFMediaType_Audio, MFMediaType_Video, MFShutdown, MFStartup, MFVideoFormat_RGB32,
    };
    use windows::Win32::System::Com::StructuredStorage::{PROPVARIANT, PropVariantToInt64};
    use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};
    use windows::core::HSTRING;

    // MF_SA_D3D11_AWARE etc. are not needed. We keep COM/MF init per reader.

    pub struct WindowsReader {
        reader: IMFSourceReader,
        out_width: u32,
        out_height: u32,
        stride: i32,
    }

    // SAFETY: MFStartup/CoInitializeEx are called on this thread in open().
    // Drop order: the reader is dropped before MFShutdown/CoUninitialize in
    // the Drop impl below, matching the startup order.
    pub(crate) struct MfGuard;
    impl Drop for MfGuard {
        fn drop(&mut self) {
            // SAFETY: paired with the successful MFStartup in `start_media_foundation`.
            let _ = unsafe { MFShutdown() };
            // SAFETY: paired with CoInitializeEx; called once per reader.
            unsafe { CoUninitialize() };
        }
    }

    pub(crate) fn start_media_foundation() -> Result<MfGuard, VideoError> {
        // SAFETY: MFStartup initializes Media Foundation for this thread/process;
        // the caller pairs it with MFShutdown in MfGuard.
        unsafe { MFStartup(0x0002_0000, 0) }.map_err(|e| VideoError(format!("MFStartup: {e}")))?;
        // SAFETY: MTA COM on the calling thread as the plan requires.
        let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if hr.is_err() {
            // SAFETY: MFStartup succeeded, so shut it back down.
            let _ = unsafe { MFShutdown() };
            return Err(VideoError(format!("CoInitializeEx: {hr:?}")));
        }
        Ok(MfGuard)
    }

    fn make_rgb_media_type(width: u32, height: u32) -> Result<IMFMediaType, VideoError> {
        // SAFETY: creates a media type object owned by the caller.
        let media_type = unsafe { MFCreateMediaType() }
            .map_err(|e| VideoError(format!("MFCreateMediaType: {e}")))?;
        // SAFETY: media_type is a live IMFAttributes; SetGUID only fails on bad args.
        unsafe { media_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video) }
            .map_err(|e| VideoError(format!("SetGUID major: {e}")))?;
        // SAFETY: same as above.
        unsafe { media_type.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32) }
            .map_err(|e| VideoError(format!("SetGUID subtype: {e}")))?;
        let packed = (u64::from(width) << 32) | u64::from(height);
        // SAFETY: frame size is width<<32|height.
        unsafe { media_type.SetUINT64(&MF_MT_FRAME_SIZE, packed) }
            .map_err(|e| VideoError(format!("SetUINT64 frame size: {e}")))?;
        Ok(media_type)
    }

    fn make_attributes() -> Result<IMFAttributes, VideoError> {
        let mut attributes: Option<IMFAttributes> = None;
        // SAFETY: creates an owned attribute bag; returns via out param.
        unsafe { MFCreateAttributes(&mut attributes as *mut Option<IMFAttributes>, 0) }
            .map_err(|e| VideoError(format!("MFCreateAttributes: {e}")))?;
        attributes.ok_or_else(|| VideoError("no attributes".to_string()))
    }

    fn open_reader(
        path: &Path,
        out_width: u32,
        out_height: u32,
    ) -> Result<WindowsReader, VideoError> {
        let guard = start_media_foundation()?;
        std::mem::forget(guard); // shutdown happens in Drop of WindowsReader; see below.

        let attributes = make_attributes()?;
        // SAFETY: attributes is live; bool value is nonzero for enable.
        let _ =
            unsafe { attributes.SetUINT32(&MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING, 1) };

        let path_str = path.to_string_lossy().to_string();
        let wide = HSTRING::from(path_str);
        // SAFETY: reader is created from the URL; null attributes would be valid,
        // we pass the bag we just created.
        let reader = unsafe { MFCreateSourceReaderFromURL(&wide, Some(&attributes)) }
            .map_err(|e| VideoError(format!("source reader: {e}")))?;

        // Select video stream 0, deselect others.
        // SAFETY: stream 0 is the video stream in the common layout.
        let _ = unsafe { reader.SetStreamSelection(0, true) };

        let media_type = make_rgb_media_type(out_width, out_height)?;
        // SAFETY: sets the current media type for stream 0.
        unsafe { reader.SetCurrentMediaType(0, None, &media_type) }
            .map_err(|e| VideoError(format!("SetCurrentMediaType: {e}")))?;

        // SAFETY: current media type is now RGB32 at the requested size.
        let current = unsafe { reader.GetCurrentMediaType(0) }
            .map_err(|e| VideoError(format!("GetCurrentMediaType: {e}")))?;
        // SAFETY: `current` is a live IMFMediaType; attribute missing yields default stride.
        let stride: i64 = unsafe { current.GetUINT64(&MF_MT_DEFAULT_STRIDE) }.unwrap_or(0) as i64;
        let stride = if stride != 0 {
            stride as i32
        } else {
            (out_width as i32) * 4
        };

        Ok(WindowsReader {
            reader,
            out_width,
            out_height,
            stride,
        })
    }

    impl Drop for WindowsReader {
        fn drop(&mut self) {
            // Drop order: reader first (field), then MFShutdown/CoUninitialize.
            // Fields drop after this body; we release the reader explicitly by
            // dropping a clone is unnecessary since MFShutdown is process-wide
            // and safe after the reader is gone. To respect order, we shut down
            // here after `reader` is already no longer used.
            // SAFETY: paired shutdown for the MFStartup in open_reader.
            let _ = unsafe { MFShutdown() };
            // SAFETY: paired with CoInitializeEx.
            unsafe { CoUninitialize() };
        }
    }

    fn source_reader_from_path(
        path: &Path,
        out_width: u32,
        out_height: u32,
    ) -> Result<WindowsReader, VideoError> {
        open_reader(path, out_width, out_height)
    }

    pub fn open(path: &Path, max_width: u32) -> Result<VideoFile, VideoError> {
        if !path.is_file() {
            return Err(VideoError(format!("not a file: {}", path.display())));
        }
        // Probe with a reader at 1x1? Instead probe native size via the reader:
        // create a reader without forcing size to learn the natural size.
        // For simplicity we create the reader at the default scaled size after
        // learning the size from the native media type.
        let probe = probe_size(path)?;
        let (width, height) = probe;
        scaled_size(width, height, max_width)?;
        let duration_ms = probe_duration(path).unwrap_or(0);
        let has_audio = probe_has_audio(path);
        Ok(VideoFile {
            path: path.to_path_buf(),
            width,
            height,
            duration_ms,
            has_audio,
            max_width,
            windows: None,
        })
    }

    fn probe_size(path: &Path) -> Result<(u32, u32), VideoError> {
        let guard = start_media_foundation()?;
        std::mem::forget(guard);
        // SAFETY: attributes optional.
        let wide = HSTRING::from(path.to_string_lossy().to_string());
        // SAFETY: creates reader from URL.
        let reader = unsafe { MFCreateSourceReaderFromURL(&wide, None::<&IMFAttributes>) }
            .map_err(|e| VideoError(format!("probe reader: {e}")))?;
        // SAFETY: native media type of stream 0.
        let native = unsafe { reader.GetNativeMediaType(0, 0) }
            .map_err(|e| VideoError(format!("native type: {e}")))?;
        // SAFETY: `native` is a live IMFMediaType; frame size attribute read.
        let packed = unsafe { native.GetUINT64(&MF_MT_FRAME_SIZE) }
            .map_err(|e| VideoError(format!("frame size: {e}")))?;
        let width = ((packed >> 32) & 0xFFFF_FFFF) as u32;
        let height = (packed & 0xFFFF_FFFF) as u32;
        // SAFETY: paired shutdown.
        let _ = unsafe { MFShutdown() };
        // SAFETY: paired with CoInitializeEx in start_media_foundation.
        unsafe { CoUninitialize() };
        Ok((width, height))
    }

    fn probe_duration(path: &Path) -> Option<u64> {
        if !path.is_file() {
            return None;
        }
        let guard = start_media_foundation().ok()?;
        std::mem::forget(guard);
        let wide = HSTRING::from(path.to_string_lossy().to_string());
        // SAFETY: reader is live for this block; we shut MF down afterwards.
        let duration_ms = unsafe {
            let reader = MFCreateSourceReaderFromURL(&wide, None::<&IMFAttributes>).ok()?;
            let stream = MF_SOURCE_READER_MEDIASOURCE.0 as u32;
            let variant = reader
                .GetPresentationAttribute(stream, &MF_PD_DURATION)
                .ok()?;
            let ticks = PropVariantToInt64(&variant as *const PROPVARIANT).ok()?;
            // MF_PD_DURATION is in 100 ns units; convert to milliseconds.
            if ticks <= 0 {
                None
            } else {
                Some((ticks / 10_000) as u64)
            }
        };
        // SAFETY: paired shutdown.
        let _ = unsafe { MFShutdown() };
        // SAFETY: paired with CoInitializeEx in start_media_foundation.
        unsafe { CoUninitialize() };
        duration_ms
    }

    fn probe_has_audio(path: &Path) -> bool {
        let guard = match start_media_foundation() {
            Ok(g) => g,
            Err(_) => return false,
        };
        std::mem::forget(guard);
        let wide = HSTRING::from(path.to_string_lossy().to_string());
        // SAFETY: reader creation.
        let has = match unsafe { MFCreateSourceReaderFromURL(&wide, None::<&IMFAttributes>) } {
            Ok(reader) => {
                // Stream 1 is audio when present; GetNativeMediaType fails if absent.
                // SAFETY: probing stream 1.
                unsafe { reader.GetNativeMediaType(1, 0) }.is_ok()
            }
            Err(_) => false,
        };
        // SAFETY: paired shutdown.
        let _ = unsafe { MFShutdown() };
        // SAFETY: paired with CoInitializeEx in start_media_foundation.
        unsafe { CoUninitialize() };
        has
    }

    pub fn seek(file: &mut VideoFile, ms: u64) -> Result<(), VideoError> {
        let (out_width, out_height) = scaled_size(file.width, file.height, file.max_width)?;
        let path = file.path.clone();
        let reader = source_reader_from_path(&path, out_width, out_height)?;
        // Seek via SetCurrentPosition with GUID_NULL (no time format) and a
        // VT_I8 PROPVARIANT holding 100ns units.
        let mut variant = PROPVARIANT::default();
        // SAFETY: PROPVARIANT layout is {u16 vt, ... i64 value at offset 8}
        // for VT_I8; we write both fields into our owned value.
        unsafe {
            let vt_ptr = &mut variant as *mut PROPVARIANT as *mut u16;
            std::ptr::write(vt_ptr, windows::Win32::System::Variant::VT_I8.0);
            let val_ptr = (&mut variant as *mut PROPVARIANT).add(1) as *mut u8;
            let _ = val_ptr;
            let ticks = (ms.saturating_mul(10_000)) as i64;
            // VT_I8 value lives at offset 8 on 64-bit (vt u16 + 6 bytes pad).
            let value_ptr = (&mut variant as *mut PROPVARIANT as *mut u8).add(8) as *mut i64;
            std::ptr::write(value_ptr, ticks);
        }
        // SAFETY: reader is live; null GUID means "use the format for the type".
        let _ = unsafe {
            reader
                .reader
                .SetCurrentPosition(std::ptr::null(), &variant as *const PROPVARIANT)
        };
        file.windows = Some(reader);
        Ok(())
    }

    pub fn next_frame(file: &mut VideoFile) -> Result<Option<VideoFrame>, VideoError> {
        if file.windows.is_none() {
            file.seek(0)?;
        }
        let Some(reader) = file.windows.as_ref() else {
            return Err(VideoError("no reader".to_string()));
        };
        let out_width = reader.out_width;
        let out_height = reader.out_height;
        let stride = reader.stride;
        let mut actual_stream: u32 = 0;
        let mut stream_flags: u32 = 0;
        let mut timestamp: i64 = 0;
        let mut sample: Option<IMFSample> = None;
        // SAFETY: ReadSample fills the out params; we pass valid stack slots.
        let hr = unsafe {
            reader.reader.ReadSample(
                0,
                0,
                Some(&mut actual_stream),
                Some(&mut stream_flags),
                Some(&mut timestamp),
                Some(&mut sample),
            )
        };
        if hr.is_err() {
            return Err(VideoError(format!("ReadSample: {hr:?}")));
        }
        let eof = MF_SOURCE_READERF_ENDOFSTREAM.0 as u32;
        let err_flag = MF_SOURCE_READERF_ERROR.0 as u32;
        let tick = MF_SOURCE_READERF_STREAMTICK.0 as u32;
        if stream_flags & eof != 0 || stream_flags & err_flag != 0 || sample.is_none() {
            if stream_flags & tick != 0 {
                // Tick with no sample: try again once.
                return next_frame(file);
            }
            return Ok(None);
        }
        let sample = match sample {
            Some(s) => s,
            None => return Ok(None),
        };
        let pts_ms = if timestamp > 0 {
            (timestamp / 10_000) as u64
        } else {
            0
        };
        // Get the media buffer.
        // SAFETY: buffer count for a video sample is 1.
        let buffer_count = unsafe { sample.GetBufferCount() }.unwrap_or(0);
        if buffer_count == 0 {
            return Err(VideoError("no buffer".to_string()));
        }
        // SAFETY: first buffer.
        let buffer: IMFMediaBuffer = unsafe { sample.GetBufferByIndex(0) }
            .map_err(|e| VideoError(format!("GetBufferByIndex: {e}")))?;
        let mut raw_ptr: *mut u8 = std::ptr::null_mut();
        let mut max_len: u32 = 0;
        let mut cur_len: u32 = 0;
        // SAFETY: Lock returns a pointer we must Unlock.
        unsafe { buffer.Lock(&mut raw_ptr, Some(&mut max_len), Some(&mut cur_len)) }
            .map_err(|e| VideoError(format!("Lock: {e}")))?;
        let result = (|| -> Result<VideoFrame, VideoError> {
            if raw_ptr.is_null() {
                return Err(VideoError("null buffer pointer".to_string()));
            }
            let width = out_width as usize;
            let height = out_height as usize;
            let row_bytes = if stride < 0 {
                // Top-down negative stride means bottom-up; handle by absolute value
                // and flip rows when copying.
                (stride.unsigned_abs()) as usize
            } else if stride != 0 {
                stride as usize
            } else {
                width * 4
            };
            if row_bytes < width * 4 {
                return Err(VideoError(format!("stride {row_bytes} too small")));
            }
            let effective_len = if cur_len != 0 {
                cur_len as usize
            } else {
                max_len as usize
            };
            let need = row_bytes.saturating_mul(height);
            if effective_len != 0 && effective_len < need {
                return Err(VideoError(format!(
                    "buffer too small {effective_len} < {need}"
                )));
            }
            // SAFETY: raw_ptr points to `need` readable bytes while locked.
            let src: &[u8] = unsafe { std::slice::from_raw_parts(raw_ptr as *const u8, need) };
            if src.len() < need {
                return Err(VideoError("short buffer".to_string()));
            }
            let total = width.saturating_mul(height).saturating_mul(4);
            if total > 256 * 1024 * 1024 {
                return Err(VideoError("frame too large".to_string()));
            }
            let bottom_up = stride < 0;
            let mut rgba = Vec::with_capacity(total);
            for row in 0..height {
                let src_row = if bottom_up { height - 1 - row } else { row };
                let start = src_row.saturating_mul(row_bytes);
                let end = start.saturating_add(width * 4);
                let Some(row_bytes_slice) = src.get(start..end) else {
                    return Err(VideoError("short row".to_string()));
                };
                for chunk in row_bytes_slice.chunks(4) {
                    if chunk.len() < 4 {
                        return Err(VideoError("short pixel chunk".to_string()));
                    }
                    // RGB32 is B,G,R,X in memory -> R,G,B,255.
                    rgba.push(chunk[2]);
                    rgba.push(chunk[1]);
                    rgba.push(chunk[0]);
                    rgba.push(255);
                }
            }
            Ok(VideoFrame {
                pts_ms,
                width: out_width,
                height: out_height,
                rgba,
            })
        })();
        // SAFETY: buffer was locked above; always unlock.
        let _ = unsafe { buffer.Unlock() };
        result.map(Some)
    }

    pub fn decode_audio(path: &Path) -> Result<Option<Pcm>, VideoError> {
        // Decode the audio track to 16-bit PCM through Media Foundation's
        // IMFSourceReader, capped at 60 minutes. The first audio stream is
        // selected, the output is forced to MFAudioFormat_PCM 16-bit, and the
        // actual format is read back with GetCurrentMediaType.
        if !path.is_file() {
            return Err(VideoError(format!("not a file: {}", path.display())));
        }
        let guard = start_media_foundation()?;
        std::mem::forget(guard);
        let result = (|| -> Result<Option<Pcm>, VideoError> {
            let wide = HSTRING::from(path.to_string_lossy().to_string());
            // SAFETY: creates a reader from the URL; null attributes are valid.
            let reader = unsafe { MFCreateSourceReaderFromURL(&wide, None::<&IMFAttributes>) }
                .map_err(|e| VideoError(format!("audio source reader: {e}")))?;
            // Find the first audio stream by native major type.
            let mut audio_stream: Option<u32> = None;
            for idx in 0..4 {
                // SAFETY: probing stream `idx` for its native media type.
                let native = unsafe { reader.GetNativeMediaType(idx, 0) };
                let Ok(media) = native else {
                    // Indices past the last stream fail; keep probing a
                    // couple of slots then give up.
                    if idx > 1 {
                        break;
                    }
                    continue;
                };
                // SAFETY: `media` is a live IMFMediaType.
                let major = unsafe { media.GetGUID(&MF_MT_MAJOR_TYPE) };
                if let Ok(guid) = major
                    && guid == MFMediaType_Audio
                {
                    audio_stream = Some(idx);
                    break;
                }
            }
            let Some(stream) = audio_stream else {
                return Ok(None);
            };
            // Build a PCM media type: audio / PCM / 16-bit.
            // SAFETY: creates a media type object owned by the caller.
            let media_type = unsafe { MFCreateMediaType() }
                .map_err(|e| VideoError(format!("MFCreateMediaType: {e}")))?;
            // SAFETY: media_type is a live IMFAttributes.
            unsafe { media_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio) }
                .map_err(|e| VideoError(format!("SetGUID major: {e}")))?;
            // SAFETY: same as above.
            unsafe { media_type.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM) }
                .map_err(|e| VideoError(format!("SetGUID subtype: {e}")))?;
            // SAFETY: same as above.
            unsafe { media_type.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16) }
                .map_err(|e| VideoError(format!("SetUINT32 bits: {e}")))?;
            // SAFETY: selects PCM output for the audio stream.
            unsafe { reader.SetCurrentMediaType(stream, None, &media_type) }
                .map_err(|e| VideoError(format!("SetCurrentMediaType: {e}")))?;
            // SAFETY: current media type is now PCM at the requested depth.
            let current = unsafe { reader.GetCurrentMediaType(stream) }
                .map_err(|e| VideoError(format!("GetCurrentMediaType: {e}")))?;
            // SAFETY: `current` is a live IMFMediaType.
            let rate = unsafe { current.GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND) }.unwrap_or(0);
            // SAFETY: same as above.
            let channels = unsafe { current.GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS) }.unwrap_or(0);
            if rate == 0 || rate > 768_000 {
                return Err(VideoError(format!("bad sample rate {rate}")));
            }
            if !(1..=8).contains(&channels) {
                return Err(VideoError(format!("bad channel count {channels}")));
            }
            let channels_u16 = channels as u16;
            let max_samples = (MAX_AUDIO_MS / 1000)
                .saturating_mul(u64::from(rate))
                .saturating_mul(u64::from(channels_u16))
                .min(usize::MAX as u64) as usize;
            let mut collected: Vec<i16> = Vec::new();
            loop {
                let mut actual_stream: u32 = 0;
                let mut stream_flags: u32 = 0;
                let mut timestamp: i64 = 0;
                let mut sample: Option<IMFSample> = None;
                // SAFETY: ReadSample fills the out params.
                let hr = unsafe {
                    reader.ReadSample(
                        stream,
                        0,
                        Some(&mut actual_stream),
                        Some(&mut stream_flags),
                        Some(&mut timestamp),
                        Some(&mut sample),
                    )
                };
                if hr.is_err() {
                    break;
                }
                let eof = MF_SOURCE_READERF_ENDOFSTREAM.0 as u32;
                let err_flag = MF_SOURCE_READERF_ERROR.0 as u32;
                let tick = MF_SOURCE_READERF_STREAMTICK.0 as u32;
                if stream_flags & tick != 0 && sample.is_none() {
                    continue;
                }
                if stream_flags & eof != 0 || stream_flags & err_flag != 0 {
                    if sample.is_none() {
                        break;
                    }
                    // A final sample with EOF/error still carries data; read it
                    // below then stop.
                }
                let Some(sample) = sample else {
                    continue;
                };
                // SAFETY: buffer count for an audio sample.
                let buffer_count = unsafe { sample.GetBufferCount() }.unwrap_or(0);
                if buffer_count == 0 {
                    continue;
                }
                for buf_idx in 0..buffer_count {
                    // SAFETY: buffer index is in range.
                    let buffer: IMFMediaBuffer = match unsafe { sample.GetBufferByIndex(buf_idx) } {
                        Ok(buf) => buf,
                        Err(_) => continue,
                    };
                    let mut raw_ptr: *mut u8 = std::ptr::null_mut();
                    let mut max_len: u32 = 0;
                    let mut cur_len: u32 = 0;
                    // SAFETY: Lock returns a pointer we must Unlock.
                    let lock_result = unsafe {
                        buffer.Lock(&mut raw_ptr, Some(&mut max_len), Some(&mut cur_len))
                    };
                    if lock_result.is_err() {
                        continue;
                    }
                    let copy_result = (|| -> Result<(), VideoError> {
                        if raw_ptr.is_null() {
                            return Err(VideoError("null audio buffer".to_string()));
                        }
                        let effective_len = if cur_len != 0 {
                            cur_len as usize
                        } else {
                            max_len as usize
                        };
                        if effective_len == 0 {
                            return Ok(());
                        }
                        if !effective_len.is_multiple_of(2) {
                            return Err(VideoError("odd audio length".to_string()));
                        }
                        if effective_len > 16 * 1024 * 1024 {
                            return Err(VideoError("audio buffer too large".to_string()));
                        }
                        // SAFETY: raw_ptr points to `effective_len` bytes while locked.
                        let src: &[u8] = unsafe {
                            std::slice::from_raw_parts(raw_ptr as *const u8, effective_len)
                        };
                        let room = max_samples.saturating_sub(collected.len());
                        let mut count = 0usize;
                        for pair in src.chunks(2) {
                            if count >= room {
                                break;
                            }
                            if pair.len() == 2 {
                                collected.push(i16::from_le_bytes([pair[0], pair[1]]));
                                count += 1;
                            }
                        }
                        Ok(())
                    })();
                    // SAFETY: buffer was locked above; always unlock.
                    let _ = unsafe { buffer.Unlock() };
                    let _ = copy_result;
                    if collected.len() >= max_samples {
                        break;
                    }
                }
                if collected.len() >= max_samples {
                    break;
                }
                if stream_flags & eof != 0 || stream_flags & err_flag != 0 {
                    break;
                }
            }
            if collected.is_empty() {
                return Ok(None);
            }
            Ok(Some(Pcm {
                sample_rate: rate,
                channels: channels_u16,
                samples: collected,
            }))
        })();
        // SAFETY: paired shutdown.
        let _ = unsafe { MFShutdown() };
        // SAFETY: paired with CoInitializeEx.
        unsafe { CoUninitialize() };
        result
    }
}

/// MP4 writing through the operating system encoder.
///
/// AVFoundation's `AVAssetWriter` on macOS, Media Foundation's sink writer on
/// Windows, and `VideoError("unsupported")` elsewhere. See
/// `docs/plans/m15-publishing.md`.
pub mod mp4 {
    use super::VideoError;
    use std::path::Path;

    /// Audio track settings for an MP4 export.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct AudioSettings {
        /// Sample rate in Hz (48000 from the exporter).
        pub sample_rate: u32,
        /// Channel count (2 from the exporter).
        pub channels: u16,
    }

    /// Settings for writing an MP4.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Mp4Settings {
        /// Video width in pixels; even and 16..=4096.
        pub width: u32,
        /// Video height in pixels; even and 16..=4096.
        pub height: u32,
        /// Frames per second; 1..=60.
        pub fps: u32,
        /// Optional AAC audio track.
        pub audio: Option<AudioSettings>,
        /// Average video bitrate in bits per second; 0 picks a default.
        pub video_bitrate_bps: u32,
    }

    impl Mp4Settings {
        fn normalised(self) -> Result<Self, VideoError> {
            if !self.width.is_multiple_of(2) || !self.height.is_multiple_of(2) {
                return Err(VideoError(format!(
                    "video size {}x{} must be even",
                    self.width, self.height
                )));
            }
            if !(16..=4096).contains(&self.width) || !(16..=4096).contains(&self.height) {
                return Err(VideoError(format!(
                    "video size {}x{} outside 16..=4096",
                    self.width, self.height
                )));
            }
            if !(1..=60).contains(&self.fps) {
                return Err(VideoError(format!("fps {} outside 1..=60", self.fps)));
            }
            if let Some(audio) = self.audio {
                if !(8000..=192_000).contains(&audio.sample_rate) {
                    return Err(VideoError(format!(
                        "audio rate {} outside 8000..=192000",
                        audio.sample_rate
                    )));
                }
                if !(1..=8).contains(&audio.channels) {
                    return Err(VideoError(format!(
                        "audio channels {} outside 1..=8",
                        audio.channels
                    )));
                }
            }
            let default_bitrate = u64::from(self.width)
                .saturating_mul(u64::from(self.height))
                .saturating_mul(4)
                .min(20_000_000) as u32;
            let bitrate = if self.video_bitrate_bps == 0 {
                default_bitrate.max(1_000_000)
            } else {
                self.video_bitrate_bps.clamp(1_000_000, 20_000_000)
            };
            Ok(Self {
                video_bitrate_bps: bitrate,
                ..self
            })
        }
    }

    /// Writes H.264 video and AAC audio to an MP4 through the OS encoder.
    ///
    /// `Drop` without [`Self::finish`] cancels the output and deletes the
    /// partial file.
    pub struct Mp4Writer {
        frame_len: usize,
        audio: Option<AudioSettings>,
        last_video_pts: Option<u64>,
        last_audio_pts: Option<u64>,
        finished: bool,
        #[cfg(target_os = "macos")]
        mac: Option<macos::MacWriter>,
        #[cfg(windows)]
        windows: Option<windows_impl::WindowsWriter>,
    }

    impl std::fmt::Debug for Mp4Writer {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("Mp4Writer")
                .field("frame_len", &self.frame_len)
                .field("audio", &self.audio)
                .field("finished", &self.finished)
                .finish()
        }
    }

    impl Mp4Writer {
        /// Create an MP4 at `path`. The file must not already exist.
        pub fn create(path: &Path, settings: Mp4Settings) -> Result<Self, VideoError> {
            let settings = settings.normalised()?;
            let frame_len = (settings.width as usize)
                .saturating_mul(settings.height as usize)
                .saturating_mul(4);

            #[cfg(target_os = "macos")]
            {
                let mac = macos::MacWriter::create(path, settings)?;
                Ok(Self {
                    frame_len,
                    audio: settings.audio,
                    last_video_pts: None,
                    last_audio_pts: None,
                    finished: false,
                    mac: Some(mac),
                })
            }
            #[cfg(windows)]
            {
                let windows = windows_impl::WindowsWriter::create(path, settings)?;
                Ok(Self {
                    frame_len,
                    audio: settings.audio,
                    last_video_pts: None,
                    last_audio_pts: None,
                    finished: false,
                    windows: Some(windows),
                })
            }
            #[cfg(not(any(target_os = "macos", windows)))]
            {
                let _ = (path, frame_len, settings);
                Err(VideoError("unsupported".to_string()))
            }
        }

        /// Append one RGBA frame. `rgba` is `width * height * 4` bytes and
        /// `pts_ms` must be strictly increasing.
        pub fn write_frame(&mut self, rgba: &[u8], pts_ms: u64) -> Result<(), VideoError> {
            if self.finished {
                return Err(VideoError("writer is already finished".to_string()));
            }
            if rgba.len() != self.frame_len {
                return Err(VideoError(format!(
                    "frame is {} bytes, expected {}",
                    rgba.len(),
                    self.frame_len
                )));
            }
            if self.last_video_pts.is_some_and(|last| pts_ms <= last) {
                return Err(VideoError(format!(
                    "frame pts {pts_ms} is not after the previous frame"
                )));
            }

            #[cfg(target_os = "macos")]
            let result = self
                .mac
                .as_mut()
                .ok_or_else(|| VideoError("no writer".to_string()))
                .and_then(|writer| writer.write_frame(rgba, pts_ms));
            #[cfg(windows)]
            let result = self
                .windows
                .as_mut()
                .ok_or_else(|| VideoError("no writer".to_string()))
                .and_then(|writer| writer.write_frame(rgba, pts_ms));
            #[cfg(not(any(target_os = "macos", windows)))]
            let result = {
                let _ = (rgba, pts_ms);
                Err(VideoError("unsupported".to_string()))
            };

            if result.is_ok() {
                self.last_video_pts = Some(pts_ms);
            }
            result
        }

        /// Append interleaved 16-bit PCM at the settings' rate and channels.
        /// `pts_ms` must be strictly increasing.
        pub fn write_audio(&mut self, samples: &[i16], pts_ms: u64) -> Result<(), VideoError> {
            let Some(audio) = self.audio else {
                return Err(VideoError("no audio track".to_string()));
            };
            if self.finished {
                return Err(VideoError("writer is already finished".to_string()));
            }
            let channels = usize::from(audio.channels);
            if samples.is_empty() || !samples.len().is_multiple_of(channels) {
                return Err(VideoError(format!(
                    "audio has {} samples, not a whole number of {channels}-channel frames",
                    samples.len()
                )));
            }
            if self.last_audio_pts.is_some_and(|last| pts_ms <= last) {
                return Err(VideoError(format!(
                    "audio pts {pts_ms} is not after the previous block"
                )));
            }

            #[cfg(target_os = "macos")]
            let result = self
                .mac
                .as_mut()
                .ok_or_else(|| VideoError("no writer".to_string()))
                .and_then(|writer| writer.write_audio(samples, pts_ms));
            #[cfg(windows)]
            let result = self
                .windows
                .as_mut()
                .ok_or_else(|| VideoError("no writer".to_string()))
                .and_then(|writer| writer.write_audio(samples, pts_ms));
            #[cfg(not(any(target_os = "macos", windows)))]
            let result = {
                let _ = (samples, pts_ms);
                Err(VideoError("unsupported".to_string()))
            };

            if result.is_ok() {
                self.last_audio_pts = Some(pts_ms);
            }
            result
        }

        /// Finish the file. Without this, `Drop` cancels and deletes it.
        pub fn finish(mut self) -> Result<(), VideoError> {
            self.finished = true;
            #[cfg(target_os = "macos")]
            {
                match self.mac.take() {
                    Some(mac) => mac.finish(),
                    None => Err(VideoError("no writer".to_string())),
                }
            }
            #[cfg(windows)]
            {
                match self.windows.take() {
                    Some(windows) => windows.finish(),
                    None => Err(VideoError("no writer".to_string())),
                }
            }
            #[cfg(not(any(target_os = "macos", windows)))]
            {
                Err(VideoError("unsupported".to_string()))
            }
        }
    }

    impl Drop for Mp4Writer {
        // On other systems nothing follows the early return.
        #[cfg_attr(not(any(target_os = "macos", windows)), allow(clippy::needless_return))]
        fn drop(&mut self) {
            if self.finished {
                return;
            }
            #[cfg(target_os = "macos")]
            if let Some(mac) = self.mac.take() {
                mac.cancel();
            }
            #[cfg(windows)]
            if let Some(windows) = self.windows.take() {
                windows.cancel();
            }
        }
    }

    #[cfg(target_os = "macos")]
    mod macos {
        use super::{AudioSettings, Mp4Settings, VideoError};
        use objc2::rc::{Retained, autoreleasepool};
        use objc2::runtime::AnyObject;
        use objc2_av_foundation::{
            AVAssetWriter, AVAssetWriterInput, AVAssetWriterInputPixelBufferAdaptor,
            AVAssetWriterStatus, AVFileTypeMPEG4, AVMediaTypeAudio, AVMediaTypeVideo,
            AVVideoAverageBitRateKey, AVVideoCodecKey, AVVideoCodecTypeH264,
            AVVideoCompressionPropertiesKey, AVVideoHeightKey, AVVideoWidthKey,
        };
        use objc2_core_audio_types::{
            AudioStreamBasicDescription, kAudioFormatFlagIsPacked, kAudioFormatFlagIsSignedInteger,
            kAudioFormatLinearPCM,
        };
        use objc2_core_foundation::{CFRetained, CFString};
        use objc2_core_media::{
            CMBlockBuffer, CMFormatDescription, CMSampleBuffer, CMTime,
            kCMBlockBufferAssureMemoryNowFlag,
        };
        use objc2_core_video::{
            CVPixelBuffer, CVPixelBufferCreate, CVPixelBufferGetBaseAddress,
            CVPixelBufferGetBytesPerRow, CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags,
            CVPixelBufferUnlockBaseAddress, kCVPixelBufferHeightKey,
            kCVPixelBufferPixelFormatTypeKey, kCVPixelBufferWidthKey, kCVPixelFormatType_32BGRA,
        };
        use objc2_foundation::{NSDictionary, NSNumber, NSString, NSURL};
        use std::path::Path;
        use std::ptr::NonNull;
        use std::sync::mpsc;
        use std::time::{Duration, Instant};

        pub struct MacWriter {
            writer: Retained<AVAssetWriter>,
            video_input: Retained<AVAssetWriterInput>,
            adaptor: Retained<AVAssetWriterInputPixelBufferAdaptor>,
            audio_input: Option<Retained<AVAssetWriterInput>>,
            audio_format: Option<CFRetained<CMFormatDescription>>,
            width: u32,
            height: u32,
            sample_rate: u32,
            channels: u16,
        }

        fn file_url(path: &Path) -> Retained<NSURL> {
            let s = path.to_string_lossy();
            let path_str = NSString::from_str(&s);
            NSURL::fileURLWithPath(&path_str)
        }

        fn nsobj(value: Retained<NSNumber>) -> Retained<AnyObject> {
            value.into_super().into_super().into_super()
        }

        fn static_string(
            value: Option<&'static NSString>,
            name: &str,
        ) -> Result<&'static NSString, VideoError> {
            value.ok_or_else(|| VideoError(format!("{name} unavailable")))
        }

        fn video_settings(
            settings: Mp4Settings,
        ) -> Result<Retained<NSDictionary<NSString, AnyObject>>, VideoError> {
            // SAFETY: AVFoundation exposes these constants for the process lifetime.
            let codec_key = static_string(unsafe { AVVideoCodecKey }, "AVVideoCodecKey")?;
            // SAFETY: same as above.
            let width_key = static_string(unsafe { AVVideoWidthKey }, "AVVideoWidthKey")?;
            // SAFETY: same as above.
            let height_key = static_string(unsafe { AVVideoHeightKey }, "AVVideoHeightKey")?;
            // SAFETY: same as above.
            let compression_key = static_string(
                unsafe { AVVideoCompressionPropertiesKey },
                "AVVideoCompressionPropertiesKey",
            )?;
            // SAFETY: same as above.
            let bitrate_key = static_string(
                unsafe { AVVideoAverageBitRateKey },
                "AVVideoAverageBitRateKey",
            )?;

            // SAFETY: AVFoundation exposes this codec constant for the process lifetime.
            let codec = unsafe { AVVideoCodecTypeH264 }
                .ok_or_else(|| VideoError("AVVideoCodecTypeH264 unavailable".to_string()))?;
            let codec_value = NSString::from_str(&codec.to_string());
            let codec_any: Retained<AnyObject> = codec_value.into_super().into_super();
            let width = nsobj(NSNumber::new_u32(settings.width));
            let height = nsobj(NSNumber::new_u32(settings.height));
            let bitrate = nsobj(NSNumber::new_u32(settings.video_bitrate_bps));
            let compression: Retained<NSDictionary<NSString, AnyObject>> =
                NSDictionary::from_slices(&[bitrate_key], &[&*bitrate]);
            let compression_any: Retained<AnyObject> = compression.into_super().into_super();
            Ok(NSDictionary::from_slices(
                &[codec_key, width_key, height_key, compression_key],
                &[&*codec_any, &*width, &*height, &*compression_any],
            ))
        }

        fn audio_settings(
            audio: AudioSettings,
        ) -> Result<Retained<NSDictionary<NSString, AnyObject>>, VideoError> {
            let keys = [
                NSString::from_str("AVFormatIDKey"),
                NSString::from_str("AVSampleRateKey"),
                NSString::from_str("AVNumberOfChannelsKey"),
                NSString::from_str("AVEncoderBitRateKey"),
            ];
            let values = [
                // kAudioFormatMPEG4AAC = 'aac ' = 0x6161_6320.
                nsobj(NSNumber::new_u32(0x6161_6320)),
                nsobj(NSNumber::new_f64(f64::from(audio.sample_rate))),
                nsobj(NSNumber::new_u32(u32::from(audio.channels))),
                nsobj(NSNumber::new_u32(128_000)),
            ];
            Ok(NSDictionary::from_slices(
                &keys.iter().map(|k| &**k).collect::<Vec<_>>(),
                &values.iter().map(|v| &**v).collect::<Vec<_>>(),
            ))
        }

        fn pixel_buffer_attributes(
            width: u32,
            height: u32,
        ) -> Retained<NSDictionary<NSString, AnyObject>> {
            // SAFETY: the CoreVideo keys are CFStrings toll-free bridged to
            // NSString and valid for the process lifetime.
            let format_key: &NSString = unsafe {
                &*(kCVPixelBufferPixelFormatTypeKey as *const CFString as *const NSString)
            };
            // SAFETY: same as above.
            let width_key: &NSString =
                unsafe { &*(kCVPixelBufferWidthKey as *const CFString as *const NSString) };
            // SAFETY: same as above.
            let height_key: &NSString =
                unsafe { &*(kCVPixelBufferHeightKey as *const CFString as *const NSString) };
            let format = nsobj(NSNumber::new_u32(kCVPixelFormatType_32BGRA));
            let w = nsobj(NSNumber::new_u32(width));
            let h = nsobj(NSNumber::new_u32(height));
            NSDictionary::from_slices(&[format_key, width_key, height_key], &[&*format, &*w, &*h])
        }

        fn audio_format_description(
            audio: AudioSettings,
        ) -> Result<CFRetained<CMFormatDescription>, VideoError> {
            let bytes_per_frame = u32::from(audio.channels).saturating_mul(2);
            let asbd = AudioStreamBasicDescription {
                mSampleRate: f64::from(audio.sample_rate),
                mFormatID: kAudioFormatLinearPCM,
                mFormatFlags: kAudioFormatFlagIsSignedInteger | kAudioFormatFlagIsPacked,
                mBytesPerPacket: bytes_per_frame,
                mFramesPerPacket: 1,
                mBytesPerFrame: bytes_per_frame,
                mChannelsPerFrame: u32::from(audio.channels),
                mBitsPerChannel: 16,
                mReserved: 0,
            };
            let mut description: *const CMFormatDescription = std::ptr::null();
            // SAFETY: `asbd` is a valid linear-PCM description and the out
            // pointer is a valid slot; the call copies the description.
            let status = unsafe {
                objc2_core_media::CMAudioFormatDescriptionCreate(
                    None,
                    NonNull::from(&asbd),
                    0,
                    std::ptr::null(),
                    0,
                    std::ptr::null(),
                    None,
                    NonNull::from(&mut description),
                )
            };
            if status != 0 {
                return Err(VideoError(format!(
                    "CMAudioFormatDescriptionCreate failed: {status}"
                )));
            }
            let Some(description) = NonNull::new(description as *mut CMFormatDescription) else {
                return Err(VideoError("null audio format description".to_string()));
            };
            // SAFETY: the call returned a retained format description.
            Ok(unsafe { CFRetained::from_raw(description) })
        }

        fn writer_error(writer: &AVAssetWriter, context: &str) -> VideoError {
            // SAFETY: `error` is a pure getter and returns nil unless the writer failed.
            match unsafe { writer.error() } {
                Some(error) => VideoError(format!("{context}: {}", error.localizedDescription())),
                None => VideoError(context.to_string()),
            }
        }

        fn wait_ready(input: &AVAssetWriterInput, what: &str) -> Result<(), VideoError> {
            let started = Instant::now();
            loop {
                // SAFETY: readiness is a pure getter on a live input.
                if unsafe { input.isReadyForMoreMediaData() } {
                    return Ok(());
                }
                if started.elapsed() >= Duration::from_secs(10) {
                    return Err(VideoError(format!("{what} input stayed busy for 10 s")));
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        }

        fn make_pixel_buffer(
            rgba: &[u8],
            width: u32,
            height: u32,
        ) -> Result<CFRetained<CVPixelBuffer>, VideoError> {
            let mut raw: *mut CVPixelBuffer = std::ptr::null_mut();
            // SAFETY: the out pointer is valid; the buffer is created BGRA at this size.
            let status = unsafe {
                CVPixelBufferCreate(
                    None,
                    width as usize,
                    height as usize,
                    kCVPixelFormatType_32BGRA,
                    None,
                    NonNull::from(&mut raw),
                )
            };
            if status != 0 {
                return Err(VideoError(format!("CVPixelBufferCreate failed: {status}")));
            }
            let Some(raw) = NonNull::new(raw) else {
                return Err(VideoError("null pixel buffer".to_string()));
            };
            // SAFETY: CVPixelBufferCreate returned a retained pixel buffer.
            let buffer = unsafe { CFRetained::from_raw(raw) };

            // SAFETY: the buffer is live and newly created; lock for writing.
            let locked =
                unsafe { CVPixelBufferLockBaseAddress(&buffer, CVPixelBufferLockFlags::empty()) };
            if locked != 0 {
                return Err(VideoError(format!("lock pixel buffer failed: {locked}")));
            }
            let result = (|| -> Result<(), VideoError> {
                let base = CVPixelBufferGetBaseAddress(&buffer);
                if base.is_null() {
                    return Err(VideoError("null pixel buffer base".to_string()));
                }
                let row_bytes = CVPixelBufferGetBytesPerRow(&buffer);
                let src_row_bytes = (width as usize).saturating_mul(4);
                let need = row_bytes.saturating_mul(height as usize);
                if row_bytes < src_row_bytes {
                    return Err(VideoError(format!(
                        "pixel buffer stride {row_bytes} too small"
                    )));
                }
                // SAFETY: base points to `need` writable bytes while locked.
                let dst = unsafe { std::slice::from_raw_parts_mut(base as *mut u8, need) };
                for y in 0..height as usize {
                    let src_start = y.saturating_mul(src_row_bytes);
                    let src = rgba
                        .get(src_start..src_start.saturating_add(src_row_bytes))
                        .ok_or_else(|| VideoError("short rgba row".to_string()))?;
                    let dst_start = y.saturating_mul(row_bytes);
                    let dst_row = dst
                        .get_mut(dst_start..dst_start.saturating_add(src_row_bytes))
                        .ok_or_else(|| VideoError("short pixel row".to_string()))?;
                    for (out, pixel) in dst_row.chunks_mut(4).zip(src.chunks(4)) {
                        out[0] = pixel[2];
                        out[1] = pixel[1];
                        out[2] = pixel[0];
                        out[3] = pixel[3];
                    }
                }
                Ok(())
            })();
            // SAFETY: the buffer was locked above and is still live; always unlock.
            unsafe { CVPixelBufferUnlockBaseAddress(&buffer, CVPixelBufferLockFlags::empty()) };
            result?;
            Ok(buffer)
        }

        fn make_audio_sample(
            bytes: &[u8],
            frames: usize,
            pts_ms: u64,
            rate: u32,
            format: &CMFormatDescription,
        ) -> Result<CFRetained<CMSampleBuffer>, VideoError> {
            if bytes.is_empty() || frames == 0 {
                return Err(VideoError("empty audio block".to_string()));
            }
            let mut block_ptr: *mut CMBlockBuffer = std::ptr::null_mut();
            // SAFETY: the out pointer is valid and the block buffer allocates
            // its own memory; the data is copied in below.
            let status = unsafe {
                CMBlockBuffer::create_with_memory_block(
                    None,
                    std::ptr::null_mut(),
                    bytes.len(),
                    None,
                    std::ptr::null(),
                    0,
                    bytes.len(),
                    kCMBlockBufferAssureMemoryNowFlag,
                    NonNull::from(&mut block_ptr),
                )
            };
            if status != 0 {
                return Err(VideoError(format!("CMBlockBufferCreate failed: {status}")));
            }
            let Some(block_ptr) = NonNull::new(block_ptr) else {
                return Err(VideoError("null block buffer".to_string()));
            };
            // SAFETY: the create call returned a retained block buffer.
            let block = unsafe { CFRetained::from_raw(block_ptr) };
            let Some(source) = NonNull::new(bytes.as_ptr().cast_mut().cast::<core::ffi::c_void>())
            else {
                return Err(VideoError("null audio source".to_string()));
            };
            // SAFETY: the block buffer is at least `bytes.len()` bytes and the
            // source is valid for that many bytes.
            let status =
                unsafe { CMBlockBuffer::replace_data_bytes(source, &block, 0, bytes.len()) };
            if status != 0 {
                return Err(VideoError(format!(
                    "CMBlockBufferReplaceDataBytes failed: {status}"
                )));
            }

            let rate_i32 = i32::try_from(rate)
                .map_err(|_| VideoError(format!("sample rate {rate} too large")))?;
            let frame_pos = (u64::from(rate))
                .saturating_mul(pts_ms)
                .saturating_div(1000)
                .min(i64::MAX as u64) as i64;
            // SAFETY: CMTime::new only reads its arguments.
            let presentation = unsafe { CMTime::new(frame_pos, rate_i32) };
            let mut sample_ptr: *mut CMSampleBuffer = std::ptr::null_mut();
            // SAFETY: block and format are live, the packet description is null
            // for linear PCM, and the out pointer is valid.
            let status = unsafe {
                objc2_core_media::CMAudioSampleBufferCreateReadyWithPacketDescriptions(
                    None,
                    &block,
                    format,
                    frames as objc2_core_media::CMItemCount,
                    presentation,
                    std::ptr::null(),
                    NonNull::from(&mut sample_ptr),
                )
            };
            if status != 0 {
                return Err(VideoError(format!(
                    "CMAudioSampleBufferCreateReady failed: {status}"
                )));
            }
            let Some(sample_ptr) = NonNull::new(sample_ptr) else {
                return Err(VideoError("null sample buffer".to_string()));
            };
            // SAFETY: the create call returned a retained sample buffer.
            Ok(unsafe { CFRetained::from_raw(sample_ptr) })
        }

        impl MacWriter {
            pub fn create(path: &Path, settings: Mp4Settings) -> Result<Self, VideoError> {
                autoreleasepool(|_| {
                    if path.exists() {
                        return Err(VideoError(format!(
                            "output already exists: {}",
                            path.display()
                        )));
                    }
                    // AVAssetWriter throws instead of returning an error when the
                    // parent directory is missing, so reject that case first.
                    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty())
                        && !parent.is_dir()
                    {
                        return Err(VideoError(format!(
                            "output directory does not exist: {}",
                            parent.display()
                        )));
                    }
                    let url = file_url(path);
                    // SAFETY: the file-type constant is valid for the process lifetime.
                    let file_type = unsafe { AVFileTypeMPEG4 }
                        .ok_or_else(|| VideoError("AVFileTypeMPEG4 unavailable".to_string()))?;
                    // SAFETY: `url` and `file_type` are live; a bad path becomes an error.
                    let writer = unsafe {
                        AVAssetWriter::assetWriterWithURL_fileType_error(&url, file_type)
                    }
                    .map_err(|error| {
                        VideoError(format!("asset writer: {}", error.localizedDescription()))
                    })?;

                    let video_output_settings = video_settings(settings)?;
                    // SAFETY: the media type constant is valid for the process.
                    let video_type = unsafe { AVMediaTypeVideo }
                        .ok_or_else(|| VideoError("AVMediaTypeVideo unavailable".to_string()))?;
                    // SAFETY: the media type and settings are live and well-formed.
                    let video_input = unsafe {
                        AVAssetWriterInput::assetWriterInputWithMediaType_outputSettings(
                            video_type,
                            Some(&video_output_settings),
                        )
                    };
                    // SAFETY: a pure setter on a live input; the plan requires non-realtime.
                    unsafe { video_input.setExpectsMediaDataInRealTime(false) };

                    let attributes = pixel_buffer_attributes(settings.width, settings.height);
                    // SAFETY: the input is live and the attributes describe the buffers appended.
                    let adaptor = unsafe {
                        AVAssetWriterInputPixelBufferAdaptor::assetWriterInputPixelBufferAdaptorWithAssetWriterInput_sourcePixelBufferAttributes(
                            &video_input,
                            Some(&attributes),
                        )
                    };
                    // SAFETY: the writer is live and accepts an H.264 video input.
                    unsafe { writer.addInput(&video_input) };

                    let (audio_input, audio_format) = match settings.audio {
                        Some(audio) => {
                            let audio_output_settings = audio_settings(audio)?;
                            // SAFETY: the media type constant is valid for the process.
                            let audio_type = unsafe { AVMediaTypeAudio }.ok_or_else(|| {
                                VideoError("AVMediaTypeAudio unavailable".to_string())
                            })?;
                            // SAFETY: the media type and settings are live and well-formed.
                            let input = unsafe {
                                AVAssetWriterInput::assetWriterInputWithMediaType_outputSettings(
                                    audio_type,
                                    Some(&audio_output_settings),
                                )
                            };
                            // The audio input is fed in whole blocks. Marking it
                            // real-time uses AVAssetWriter's permissive readiness
                            // policy, so a full video input queue is not held on
                            // the interleaving lookahead; the video input stays
                            // non-realtime as the plan requires.
                            // SAFETY: a pure setter on a live input.
                            unsafe { input.setExpectsMediaDataInRealTime(true) };
                            // SAFETY: the writer is live and accepts an AAC audio input.
                            unsafe { writer.addInput(&input) };
                            let format = audio_format_description(audio)?;
                            (Some(input), Some(format))
                        }
                        None => (None, None),
                    };

                    // SAFETY: all inputs are added; startWriting validates the setup.
                    if !unsafe { writer.startWriting() } {
                        return Err(writer_error(&writer, "startWriting"));
                    }
                    // SAFETY: writing has started; start the sample session at zero.
                    let zero = unsafe { CMTime::new(0, 1000) };
                    // SAFETY: a valid numeric start time for a started writer.
                    unsafe { writer.startSessionAtSourceTime(zero) };

                    Ok(Self {
                        writer,
                        video_input,
                        adaptor,
                        audio_input,
                        audio_format,
                        width: settings.width,
                        height: settings.height,
                        sample_rate: settings.audio.map_or(0, |audio| audio.sample_rate),
                        channels: settings.audio.map_or(0, |audio| audio.channels),
                    })
                })
            }

            pub fn write_frame(&mut self, rgba: &[u8], pts_ms: u64) -> Result<(), VideoError> {
                autoreleasepool(|_| {
                    wait_ready(&self.video_input, "video")?;
                    let buffer = make_pixel_buffer(rgba, self.width, self.height)?;
                    let value = i64::try_from(pts_ms)
                        .map_err(|_| VideoError("frame pts too large".to_string()))?;
                    // SAFETY: CMTime::new only reads its arguments.
                    let time = unsafe { CMTime::new(value, 1000) };
                    // SAFETY: the input is ready and the BGRA buffer matches its size.
                    let appended = unsafe {
                        self.adaptor
                            .appendPixelBuffer_withPresentationTime(&buffer, time)
                    };
                    if !appended {
                        return Err(writer_error(&self.writer, "append frame"));
                    }
                    Ok(())
                })
            }

            pub fn write_audio(&mut self, samples: &[i16], pts_ms: u64) -> Result<(), VideoError> {
                autoreleasepool(|_| {
                    let Some(input) = self.audio_input.as_ref() else {
                        return Err(VideoError("no audio track".to_string()));
                    };
                    let Some(format) = self.audio_format.as_ref() else {
                        return Err(VideoError("no audio format".to_string()));
                    };
                    let channels = usize::from(self.channels.max(1));
                    let frames = samples.len() / channels;
                    let mut bytes = Vec::with_capacity(samples.len().saturating_mul(2));
                    for sample in samples {
                        bytes.extend_from_slice(&sample.to_le_bytes());
                    }
                    wait_ready(input, "audio")?;
                    let sample =
                        make_audio_sample(&bytes, frames, pts_ms, self.sample_rate, format)?;
                    // SAFETY: the input is ready and the sample carries the input's PCM format.
                    let appended = unsafe { input.appendSampleBuffer(&sample) };
                    if !appended {
                        return Err(writer_error(&self.writer, "append audio"));
                    }
                    Ok(())
                })
            }

            pub fn finish(self) -> Result<(), VideoError> {
                if let Some(audio) = &self.audio_input {
                    // SAFETY: no appends are pending; marking an input finished is valid.
                    unsafe { audio.markAsFinished() };
                }
                // SAFETY: no appends are pending; marking an input finished is valid.
                unsafe { self.video_input.markAsFinished() };

                let (tx, rx) = mpsc::channel();
                let block = block2::RcBlock::new(move || {
                    let _ = tx.send(());
                });
                let block_ref: &block2::DynBlock<dyn Fn()> = &block;
                // SAFETY: the writer is live and the handler only sends on a channel.
                unsafe {
                    self.writer.finishWritingWithCompletionHandler(block_ref);
                }
                if rx.recv_timeout(Duration::from_secs(60)).is_err() {
                    // SAFETY: a live writer may be cancelled after a timeout.
                    unsafe { self.writer.cancelWriting() };
                    return Err(VideoError("finish timed out".to_string()));
                }
                // SAFETY: status is a pure getter on a live writer.
                let status = unsafe { self.writer.status() };
                if status == AVAssetWriterStatus::Completed {
                    Ok(())
                } else if status == AVAssetWriterStatus::Failed {
                    Err(writer_error(&self.writer, "finish"))
                } else {
                    Err(VideoError(format!("writer status {}", status.0)))
                }
            }

            pub fn cancel(self) {
                // SAFETY: cancelling a live writer deletes the partial output file.
                unsafe { self.writer.cancelWriting() };
            }
        }
    }

    #[cfg(windows)]
    mod windows_impl {
        use super::{AudioSettings, Mp4Settings, VideoError};
        use crate::windows_impl::start_media_foundation;
        use std::path::{Path, PathBuf};
        use windows::Win32::Media::MediaFoundation::{
            IMFAttributes, IMFByteStream, IMFMediaType, IMFSinkWriter,
            MF_MT_AUDIO_AVG_BYTES_PER_SECOND, MF_MT_AUDIO_BITS_PER_SAMPLE,
            MF_MT_AUDIO_BLOCK_ALIGNMENT, MF_MT_AUDIO_NUM_CHANNELS, MF_MT_AUDIO_SAMPLES_PER_SECOND,
            MF_MT_AVG_BITRATE, MF_MT_DEFAULT_STRIDE, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE,
            MF_MT_INTERLACE_MODE, MF_MT_MAJOR_TYPE, MF_MT_PIXEL_ASPECT_RATIO, MF_MT_SUBTYPE,
            MFAudioFormat_AAC, MFAudioFormat_PCM, MFCreateMediaType, MFCreateMemoryBuffer,
            MFCreateSample, MFCreateSinkWriterFromURL, MFMediaType_Audio, MFMediaType_Video,
            MFShutdown, MFVideoFormat_H264, MFVideoFormat_RGB32, MFVideoInterlace_Progressive,
        };
        use windows::Win32::System::Com::CoUninitialize;
        use windows::core::HSTRING;

        pub struct WindowsWriter {
            writer: IMFSinkWriter,
            video_stream: u32,
            audio_stream: Option<u32>,
            channels: u16,
            sample_rate: u32,
            path: PathBuf,
        }

        fn media_type() -> Result<IMFMediaType, VideoError> {
            // SAFETY: creates an owned media type object.
            unsafe { MFCreateMediaType() }
                .map_err(|error| VideoError(format!("MFCreateMediaType: {error}")))
        }

        fn set_guid(
            media_type: &IMFMediaType,
            key: &windows::core::GUID,
            value: &windows::core::GUID,
        ) -> Result<(), VideoError> {
            // SAFETY: both arguments are live attribute values.
            unsafe { media_type.SetGUID(key, value) }
                .map_err(|error| VideoError(format!("SetGUID: {error}")))
        }

        fn video_output_type(settings: Mp4Settings) -> Result<IMFMediaType, VideoError> {
            let media_type = media_type()?;
            set_guid(&media_type, &MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            set_guid(&media_type, &MF_MT_SUBTYPE, &MFVideoFormat_H264)?;
            let size = (u64::from(settings.width) << 32) | u64::from(settings.height);
            // SAFETY: the media type is live and each value matches its attribute type.
            unsafe {
                media_type.SetUINT64(&MF_MT_FRAME_SIZE, size)?;
                media_type.SetUINT64(&MF_MT_FRAME_RATE, (u64::from(settings.fps) << 32) | 1)?;
                media_type.SetUINT32(&MF_MT_AVG_BITRATE, settings.video_bitrate_bps)?;
                media_type
                    .SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
                media_type.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, (1u64 << 32) | 1)?;
            }
            Ok(media_type)
        }

        fn video_input_type(settings: Mp4Settings) -> Result<IMFMediaType, VideoError> {
            let media_type = media_type()?;
            set_guid(&media_type, &MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            set_guid(&media_type, &MF_MT_SUBTYPE, &MFVideoFormat_RGB32)?;
            let size = (u64::from(settings.width) << 32) | u64::from(settings.height);
            // Negative stride means top-down rows, matching our RGBA input.
            let stride = -((settings.width as i32).saturating_mul(4));
            // SAFETY: the media type is live and each value matches its attribute type.
            unsafe {
                media_type.SetUINT64(&MF_MT_FRAME_SIZE, size)?;
                media_type.SetUINT64(&MF_MT_FRAME_RATE, (u64::from(settings.fps) << 32) | 1)?;
                media_type
                    .SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
                media_type.SetUINT32(&MF_MT_DEFAULT_STRIDE, stride as u32)?;
            }
            Ok(media_type)
        }

        fn audio_output_type(audio: AudioSettings) -> Result<IMFMediaType, VideoError> {
            let media_type = media_type()?;
            set_guid(&media_type, &MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
            set_guid(&media_type, &MF_MT_SUBTYPE, &MFAudioFormat_AAC)?;
            // SAFETY: the media type is live and each value matches its attribute type.
            unsafe {
                media_type.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
                media_type.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, audio.sample_rate)?;
                media_type.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, u32::from(audio.channels))?;
                // 128 kbps.
                media_type.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, 16_000)?;
            }
            Ok(media_type)
        }

        fn audio_input_type(audio: AudioSettings) -> Result<IMFMediaType, VideoError> {
            let media_type = media_type()?;
            set_guid(&media_type, &MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
            set_guid(&media_type, &MF_MT_SUBTYPE, &MFAudioFormat_PCM)?;
            let block_align = u32::from(audio.channels).saturating_mul(2);
            let bytes_per_second = audio.sample_rate.saturating_mul(block_align);
            // SAFETY: the media type is live and each value matches its attribute type.
            unsafe {
                media_type.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
                media_type.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, audio.sample_rate)?;
                media_type.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, u32::from(audio.channels))?;
                media_type.SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, block_align)?;
                media_type.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, bytes_per_second)?;
            }
            Ok(media_type)
        }

        fn write_sample(
            writer: &IMFSinkWriter,
            stream: u32,
            bytes: &[u8],
            ticks: i64,
            duration: Option<i64>,
        ) -> Result<(), VideoError> {
            let len = u32::try_from(bytes.len())
                .map_err(|_| VideoError("sample too large".to_string()))?;
            if len == 0 {
                return Err(VideoError("empty sample".to_string()));
            }
            // SAFETY: allocates an owned media buffer.
            let buffer = unsafe { MFCreateMemoryBuffer(len) }
                .map_err(|error| VideoError(format!("MFCreateMemoryBuffer: {error}")))?;
            let mut ptr: *mut u8 = std::ptr::null_mut();
            let mut max_len: u32 = 0;
            // SAFETY: `Lock` fills the out pointers; `Unlock` is called below.
            unsafe { buffer.Lock(&mut ptr, Some(&mut max_len), None) }
                .map_err(|error| VideoError(format!("Lock: {error}")))?;
            let copied = (|| -> Result<(), VideoError> {
                if ptr.is_null() {
                    return Err(VideoError("null media buffer".to_string()));
                }
                if max_len < len {
                    return Err(VideoError(format!(
                        "media buffer {max_len} bytes, need {len}"
                    )));
                }
                // SAFETY: `ptr` points to `max_len` writable bytes while locked.
                unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len()) };
                // SAFETY: the buffer is locked and `len` fits.
                unsafe { buffer.SetCurrentLength(len) }
                    .map_err(|error| VideoError(format!("SetCurrentLength: {error}")))?;
                Ok(())
            })();
            // SAFETY: the buffer was locked above; always unlock.
            let _ = unsafe { buffer.Unlock() };
            copied?;

            // SAFETY: creates an owned sample.
            let sample = unsafe { MFCreateSample() }
                .map_err(|error| VideoError(format!("MFCreateSample: {error}")))?;
            // SAFETY: sample and buffer are live and compatible.
            unsafe { sample.AddBuffer(&buffer) }
                .map_err(|error| VideoError(format!("AddBuffer: {error}")))?;
            // SAFETY: the sample is live; the timestamp is in 100 ns units.
            unsafe { sample.SetSampleTime(ticks) }
                .map_err(|error| VideoError(format!("SetSampleTime: {error}")))?;
            if let Some(duration) = duration {
                // SAFETY: the sample is live.
                unsafe { sample.SetSampleDuration(duration) }
                    .map_err(|error| VideoError(format!("SetSampleDuration: {error}")))?;
            }
            // SAFETY: the writer is live and the stream index exists.
            unsafe { writer.WriteSample(stream, &sample) }
                .map_err(|error| VideoError(format!("WriteSample: {error}")))
        }

        impl WindowsWriter {
            pub fn create(path: &Path, settings: Mp4Settings) -> Result<Self, VideoError> {
                if path.exists() {
                    return Err(VideoError(format!(
                        "output already exists: {}",
                        path.display()
                    )));
                }
                let guard = start_media_foundation()?;
                std::mem::forget(guard);
                let result = (|| -> Result<Self, VideoError> {
                    let wide = HSTRING::from(path.to_string_lossy().to_string());
                    // SAFETY: creates a sink writer for the URL; no byte stream or attributes.
                    let writer = unsafe {
                        MFCreateSinkWriterFromURL(
                            &wide,
                            None::<&IMFByteStream>,
                            None::<&IMFAttributes>,
                        )
                    }
                    .map_err(|error| VideoError(format!("sink writer: {error}")))?;

                    let video_output = video_output_type(settings)?;
                    // SAFETY: the writer is live and the media type is fully specified.
                    let video_stream = unsafe { writer.AddStream(&video_output) }
                        .map_err(|error| VideoError(format!("AddStream video: {error}")))?;
                    let video_input = video_input_type(settings)?;
                    // SAFETY: the stream index exists and the input type matches.
                    unsafe {
                        writer.SetInputMediaType(video_stream, &video_input, None::<&IMFAttributes>)
                    }
                    .map_err(|error| VideoError(format!("SetInputMediaType video: {error}")))?;

                    let audio_stream = match settings.audio {
                        Some(audio) => {
                            let output = audio_output_type(audio)?;
                            // SAFETY: the writer is live.
                            let index = unsafe { writer.AddStream(&output) }
                                .map_err(|error| VideoError(format!("AddStream audio: {error}")))?;
                            let input = audio_input_type(audio)?;
                            // SAFETY: the stream index exists and the input type matches.
                            unsafe {
                                writer.SetInputMediaType(index, &input, None::<&IMFAttributes>)
                            }
                            .map_err(|error| {
                                VideoError(format!("SetInputMediaType audio: {error}"))
                            })?;
                            Some(index)
                        }
                        None => None,
                    };

                    // SAFETY: all streams are configured; start accepting samples.
                    unsafe { writer.BeginWriting() }
                        .map_err(|error| VideoError(format!("BeginWriting: {error}")))?;
                    Ok(Self {
                        writer,
                        video_stream,
                        audio_stream,
                        channels: settings.audio.map_or(0, |audio| audio.channels),
                        sample_rate: settings.audio.map_or(0, |audio| audio.sample_rate),
                        path: path.to_path_buf(),
                    })
                })();
                if result.is_err() {
                    let _ = std::fs::remove_file(path);
                    // SAFETY: paired with the MFStartup in start_media_foundation.
                    let _ = unsafe { MFShutdown() };
                    // SAFETY: paired with CoInitializeEx in start_media_foundation.
                    unsafe { CoUninitialize() };
                }
                result
            }

            pub fn write_frame(&mut self, rgba: &[u8], pts_ms: u64) -> Result<(), VideoError> {
                let mut bgra = Vec::with_capacity(rgba.len());
                for pixel in rgba.chunks(4) {
                    bgra.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
                }
                let ticks = i64::try_from(pts_ms)
                    .ok()
                    .and_then(|value| value.checked_mul(10_000))
                    .ok_or_else(|| VideoError("frame pts too large".to_string()))?;
                write_sample(&self.writer, self.video_stream, &bgra, ticks, None)
            }

            pub fn write_audio(&mut self, samples: &[i16], pts_ms: u64) -> Result<(), VideoError> {
                let Some(stream) = self.audio_stream else {
                    return Err(VideoError("no audio track".to_string()));
                };
                let channels = usize::from(self.channels.max(1));
                let frames = samples.len() / channels;
                let ticks = i64::try_from(pts_ms)
                    .ok()
                    .and_then(|value| value.checked_mul(10_000))
                    .ok_or_else(|| VideoError("audio pts too large".to_string()))?;
                let duration = (frames as u64)
                    .checked_mul(10_000_000)
                    .and_then(|value| value.checked_div(u64::from(self.sample_rate.max(1))))
                    .and_then(|value| i64::try_from(value).ok())
                    .ok_or_else(|| VideoError("audio duration too large".to_string()))?;
                let mut bytes = Vec::with_capacity(samples.len().saturating_mul(2));
                for sample in samples {
                    bytes.extend_from_slice(&sample.to_le_bytes());
                }
                write_sample(&self.writer, stream, &bytes, ticks, Some(duration))
            }

            pub fn finish(self) -> Result<(), VideoError> {
                // SAFETY: all samples have been written.
                let result = unsafe { self.writer.Finalize() }
                    .map_err(|error| VideoError(format!("Finalize: {error}")));
                drop(self.writer);
                // SAFETY: paired with the MFStartup in start_media_foundation.
                let _ = unsafe { MFShutdown() };
                // SAFETY: paired with CoInitializeEx in start_media_foundation.
                unsafe { CoUninitialize() };
                result
            }

            pub fn cancel(self) {
                drop(self.writer);
                let _ = std::fs::remove_file(&self.path);
                // SAFETY: paired with the MFStartup in start_media_foundation.
                let _ = unsafe { MFShutdown() };
                // SAFETY: paired with CoInitializeEx in start_media_foundation.
                unsafe { CoUninitialize() };
            }
        }
    }
}

#[cfg(all(not(target_os = "macos"), not(windows), test))]
mod tests {
    use super::*;

    #[test]
    fn scaled_size_keeps_aspect() {
        assert_eq!(scaled_size(1920, 1080, 1280), Ok((1280, 720)));
        assert_eq!(scaled_size(800, 600, 1280), Ok((800, 600)));
        assert!(scaled_size(0, 100, 1280).is_err());
        assert!(scaled_size(9000, 100, 1280).is_err());
        assert_eq!(scaled_size(1920, 1080, 0), Ok((1, 1)));
    }

    #[test]
    fn unsupported_open_errors() {
        assert!(VideoFile::open(Path::new("/nonexistent/video.mp4")).is_err());
    }

    #[test]
    fn chroma_row_right_half_keeps_colour() {
        // A hand-built 8-pixel chroma row: the first two pairs are neutral
        // (grey), the last two pairs are saturated red (Cr high, Cb
        // neutral). With the old `x * 2` indexing the right half read past
        // the row and fell back to 128 (grey); with `(x / 2) * 2` it keeps
        // its colour.
        let cb_row: [u8; 8] = [128, 128, 128, 128, 128, 240, 128, 240];
        for x in 0..8 {
            let off = chroma_offset(x);
            let cb = cb_row[off];
            let cr = cb_row[off + 1];
            let (r, g, b) = yuv_to_rgb(128, cb, cr, false, true);
            if x < 4 {
                assert!(
                    r == g && g == b,
                    "neutral pair is grey at x={x}: ({r},{g},{b})"
                );
            } else {
                assert!(
                    r > 200 && g < 110,
                    "right half stays saturated red at x={x}: ({r},{g},{b})"
                );
            }
        }
        // Pairs share: x=4 and x=5 read the same chroma pair.
        assert_eq!(chroma_offset(4), chroma_offset(5));
        assert_eq!(chroma_offset(6), 6);
        // BT.601 and BT.709 differ for a saturated pixel.
        let p601 = yuv_to_rgb(128, 128, 240, false, false);
        let p709 = yuv_to_rgb(128, 128, 240, false, true);
        assert_ne!(p601, p709, "matrix selection changes the colour");
    }
}

#[cfg(all(any(target_os = "macos", windows), test))]
mod tests {
    use super::*;

    #[test]
    fn scaled_size_keeps_aspect() {
        assert_eq!(scaled_size(1920, 1080, 1280), Ok((1280, 720)));
        assert_eq!(scaled_size(800, 600, 1280), Ok((800, 600)));
        assert!(scaled_size(0, 100, 1280).is_err());
        assert!(scaled_size(9000, 100, 1280).is_err());
        assert_eq!(scaled_size(1920, 1080, 0), Ok((1, 1)));
    }
}

#[cfg(all(target_os = "macos", test))]
mod macos_tests {
    use super::mp4::{AudioSettings, Mp4Settings, Mp4Writer};
    use super::*;
    use std::path::PathBuf;

    fn intro_path() -> PathBuf {
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push("../../examples/showcase/media/intro.mp4");
        p
    }

    #[test]
    fn open_info() {
        let file = VideoFile::open(&intro_path()).expect("intro.mp4 opens");
        assert!(file.width() > 0);
        assert!(file.height() > 0);
        assert!(file.width() <= MAX_DIMENSION);
        assert!(
            file.duration_ms() >= 2500 && file.duration_ms() <= 4000,
            "duration {}",
            file.duration_ms()
        );
    }

    #[test]
    fn frame_at_known_time_is_not_blank() {
        let mut file = VideoFile::open(&intro_path()).expect("opens");
        let frame = file.frame_at(1000).expect("frame").expect("some frame");
        assert!(frame.width > 0 && frame.height > 0);
        assert_eq!(
            frame.rgba.len(),
            (frame.width as usize) * (frame.height as usize) * 4
        );
        // Not blank: not all pixels identical.
        let first = &frame.rgba[0..4];
        let mut varied = false;
        for i in (0..frame.rgba.len()).step_by(4096) {
            if frame.rgba[i..i + 4] != *first {
                varied = true;
                break;
            }
        }
        assert!(varied, "frame is blank");
        // Alpha is 255 everywhere we sample.
        for i in (3..frame.rgba.len()).step_by(4016) {
            assert_eq!(frame.rgba[i], 255, "alpha at {i}");
        }
    }

    #[test]
    fn frame_at_is_within_one_frame() {
        let mut file = VideoFile::open(&intro_path()).expect("opens");
        let frame = file.frame_at(1500).expect("frame").expect("some frame");
        let diff = frame.pts_ms.abs_diff(1500);
        assert!(diff <= 200, "pts {} too far from 1500", frame.pts_ms);
    }

    #[test]
    fn garbage_and_truncated_error() {
        let dir = std::env::temp_dir();
        let garbage = dir.join(format!("cc-garbage-{}.mp4", std::process::id()));
        std::fs::write(&garbage, b"this is not a video file at all............").unwrap();
        assert!(VideoFile::open(&garbage).is_err());
        let _ = std::fs::remove_file(&garbage);

        let real = std::fs::read(intro_path()).unwrap();
        let truncated = dir.join(format!("cc-trunc-{}.mp4", std::process::id()));
        std::fs::write(&truncated, &real[..64]).unwrap();
        // A 64-byte prefix is not a playable file; open must error.
        assert!(VideoFile::open(&truncated).is_err());
        let _ = std::fs::remove_file(&truncated);
    }

    #[test]
    fn right_hand_bars_are_saturated_not_grey() {
        // intro.mp4 is a colour-bar pattern: the right-hand bars must be
        // saturated colours, not grey (the old chroma indexing read past
        // the middle of the row and fell back to neutral).
        let mut file = VideoFile::open(&intro_path()).expect("opens");
        let frame = file.frame_at(1000).expect("frame").expect("some frame");
        let w = frame.width as usize;
        let h = frame.height as usize;
        assert!(w > 8 && h > 8);
        let mut saturated = 0usize;
        let mut sampled = 0usize;
        for y in (0..h).step_by(8) {
            for x in (w / 2..w).step_by(4) {
                let idx = (y * w + x) * 4;
                let (r, g, b) = (frame.rgba[idx], frame.rgba[idx + 1], frame.rgba[idx + 2]);
                let spread = r.max(g).max(b) - r.min(g).min(b);
                sampled += 1;
                if spread > 80 {
                    saturated += 1;
                }
            }
        }
        assert!(
            sampled > 0 && saturated * 5 > sampled,
            "right half should be mostly saturated bars, got {saturated}/{sampled} saturated"
        );
    }

    #[test]
    fn chroma_row_right_half_keeps_colour_macos() {
        let cb_row: [u8; 8] = [128, 128, 128, 128, 128, 240, 128, 240];
        for x in 4..8 {
            let off = chroma_offset(x);
            let (r, g, b) = yuv_to_rgb(128, cb_row[off], cb_row[off + 1], false, true);
            assert!(r > 200 && g < 110, "x={x}: ({r},{g},{b})");
        }
    }

    #[test]
    fn decode_audio_returns_pcm() {
        // intro.mp4 is 3 s of H.264 with an AAC track, so decode_audio must
        // return real PCM (not None) with the right rate/channels, a length
        // within 10% of 3 s, and non-silent samples.
        //
        // Warm up AVFoundation first: the first asset/conversion in a fresh
        // process pays codec/plugin init (measured ~5.8s for the first asset
        // and ~12s for the first audio conversion on this machine, with a
        // second decode starting reading in ~5ms). The app pays that init
        // when it opens the video, before decode_audio runs, so the timed
        // decode below is the steady-state decode that must be quick.
        let _ = VideoFile::decode_audio(&intro_path());
        let started = std::time::Instant::now();
        let pcm = VideoFile::decode_audio(&intro_path())
            .expect("decode audio")
            .expect("intro.mp4 has an AAC track, so audio must decode to PCM");
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "decode_audio took {:?} (must finish quickly, not wait on a timeout)",
            started.elapsed()
        );
        assert!(pcm.sample_rate > 0, "rate {}", pcm.sample_rate);
        assert!(pcm.channels > 0);
        assert!(pcm.channels <= 8);
        assert!(
            pcm.samples.len() <= (MAX_AUDIO_MS / 1000 * 48000 * 8) as usize,
            "60-minute cap"
        );
        // Length within 10% of 3 seconds.
        let expected = (u64::from(pcm.sample_rate) * 3 * u64::from(pcm.channels)) as usize;
        let low = (expected as f64 * 0.9) as usize;
        let high = (expected as f64 * 1.1) as usize;
        assert!(
            pcm.samples.len() >= low && pcm.samples.len() <= high,
            "samples {} not within 10% of {} (rate={}, channels={})",
            pcm.samples.len(),
            expected,
            pcm.sample_rate,
            pcm.channels
        );
        assert!(
            pcm.samples.iter().any(|s| *s != 0),
            "audio samples are all zero"
        );
    }

    fn solid_rgba(width: u32, height: u32, rgb: [u8; 3]) -> Vec<u8> {
        let pixels = (width as usize) * (height as usize);
        let mut out = Vec::with_capacity(pixels * 4);
        for _ in 0..pixels {
            out.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
        }
        out
    }

    fn centre_pixel(frame: &VideoFrame) -> [u8; 3] {
        let x = (frame.width as usize) / 2;
        let y = (frame.height as usize) / 2;
        let index = (y * (frame.width as usize) + x) * 4;
        [
            frame.rgba[index],
            frame.rgba[index + 1],
            frame.rgba[index + 2],
        ]
    }

    fn sine_samples(frames: usize, rate: u32) -> Vec<i16> {
        let mut samples = Vec::with_capacity(frames * 2);
        for i in 0..frames {
            let t = i as f64 / f64::from(rate);
            let value = (std::f64::consts::TAU * 440.0 * t).sin() * 0.25;
            let sample = (value * 32767.0) as i16;
            samples.push(sample);
            samples.push(sample);
        }
        samples
    }

    #[test]
    fn mp4_writes_a_readable_clip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("clip.mp4");
        let width = 640;
        let height = 360;
        let fps = 30;
        let mut writer = Mp4Writer::create(
            &path,
            Mp4Settings {
                width,
                height,
                fps,
                audio: Some(AudioSettings {
                    sample_rate: 48_000,
                    channels: 2,
                }),
                video_bitrate_bps: 0,
            },
        )
        .expect("create writer");
        let rate = 48_000;
        // Twenty 0.1 s audio blocks, interleaved with the video frames so the
        // writer never has to buffer the whole soundtrack at once.
        let audio = sine_samples(rate as usize * 2, rate);
        let audio_block = rate as usize / 10 * 2;

        let red = solid_rgba(width, height, [255, 0, 0]);
        let blue = solid_rgba(width, height, [0, 0, 255]);
        for i in 0..(fps * 2) {
            if i % 3 == 0 {
                let block = usize::try_from(i / 3).expect("block index");
                let start = block * audio_block;
                writer
                    .write_audio(&audio[start..start + audio_block], block as u64 * 100)
                    .expect("write audio");
            }
            let pts_ms = u64::from(i) * 1000 / u64::from(fps);
            let frame = if i < fps { &red } else { &blue };
            writer.write_frame(frame, pts_ms).expect("write frame");
        }
        writer.finish().expect("finish");

        let mut file = VideoFile::open(&path).expect("open written clip");
        assert!(
            (1900..=2100).contains(&file.duration_ms()),
            "duration {}",
            file.duration_ms()
        );
        assert!(file.has_audio(), "the clip must have an audio track");

        let frame = file.frame_at(500).expect("seek red").expect("red frame");
        let [r, g, b] = centre_pixel(&frame);
        assert!(
            r >= 215 && g <= 40 && b <= 40,
            "red at 500 ms: ({r},{g},{b})"
        );

        let frame = file.frame_at(1500).expect("seek blue").expect("blue frame");
        let [r, g, b] = centre_pixel(&frame);
        assert!(
            b >= 215 && r <= 40 && g <= 40,
            "blue at 1500 ms: ({r},{g},{b})"
        );

        let pcm = VideoFile::decode_audio(&path)
            .expect("decode audio")
            .expect("the clip has an audio track");
        assert_eq!(pcm.channels, 2, "channels {}", pcm.channels);
        let per_channel = pcm.samples.len() / usize::from(pcm.channels);
        assert!(
            per_channel.abs_diff(96_000) <= 9_600,
            "per-channel samples {per_channel}, expected about 96000"
        );
    }

    #[test]
    fn mp4_rejects_bad_frames_and_pts() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("errors.mp4");
        let mut writer = Mp4Writer::create(
            &path,
            Mp4Settings {
                width: 64,
                height: 64,
                fps: 30,
                audio: None,
                video_bitrate_bps: 0,
            },
        )
        .expect("create writer");
        let frame = solid_rgba(64, 64, [0, 255, 0]);
        assert!(
            writer.write_frame(&frame[..frame.len() - 4], 0).is_err(),
            "a short frame must error"
        );
        writer.write_frame(&frame, 0).expect("first frame");
        assert!(
            writer.write_frame(&frame, 0).is_err(),
            "a non-increasing pts must error"
        );
    }

    #[test]
    fn mp4_rejects_missing_directory() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("missing").join("clip.mp4");
        let result = Mp4Writer::create(
            &path,
            Mp4Settings {
                width: 64,
                height: 64,
                fps: 30,
                audio: None,
                video_bitrate_bps: 0,
            },
        );
        assert!(result.is_err(), "create in a missing directory must error");
    }

    #[test]
    fn mp4_drop_without_finish_removes_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("cancel.mp4");
        {
            let mut writer = Mp4Writer::create(
                &path,
                Mp4Settings {
                    width: 64,
                    height: 64,
                    fps: 30,
                    audio: None,
                    video_bitrate_bps: 0,
                },
            )
            .expect("create writer");
            writer
                .write_frame(&solid_rgba(64, 64, [10, 20, 30]), 0)
                .expect("write frame");
        }
        assert!(!path.exists(), "cancelled output must not leave a file");
    }
}

#[cfg(all(windows, test))]
mod windows_tests {
    use super::*;
    use std::path::PathBuf;

    fn intro_path() -> PathBuf {
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push("../../examples/showcase/media/intro.mp4");
        p
    }

    #[test]
    fn open_info() {
        let file = VideoFile::open(&intro_path()).expect("intro.mp4 opens");
        assert!(file.width() > 0);
        assert!(file.height() > 0);
        assert!(file.duration_ms() >= 2500 && file.duration_ms() <= 4000);
    }

    #[test]
    fn frame_at_known_time_is_not_blank() {
        let mut file = VideoFile::open(&intro_path()).expect("opens");
        let frame = file.frame_at(1000).expect("frame").expect("some frame");
        assert_eq!(
            frame.rgba.len(),
            (frame.width as usize) * (frame.height as usize) * 4
        );
        let first = &frame.rgba[0..4];
        let mut varied = false;
        for i in (0..frame.rgba.len()).step_by(4096) {
            if frame.rgba[i..i + 4] != *first {
                varied = true;
                break;
            }
        }
        assert!(varied);
    }

    #[test]
    fn frame_at_is_within_one_frame() {
        let mut file = VideoFile::open(&intro_path()).expect("opens");
        let frame = file.frame_at(1500).expect("frame").expect("some frame");
        assert!(frame.pts_ms.abs_diff(1500) <= 200);
    }

    #[test]
    fn garbage_and_truncated_error() {
        let dir = std::env::temp_dir();
        let garbage = dir.join(format!("cc-garbage-{}.mp4", std::process::id()));
        std::fs::write(&garbage, b"not a video").unwrap();
        assert!(VideoFile::open(&garbage).is_err());
        let _ = std::fs::remove_file(&garbage);
        let real = std::fs::read(intro_path()).unwrap();
        let truncated = dir.join(format!("cc-trunc-{}.mp4", std::process::id()));
        std::fs::write(&truncated, &real[..64]).unwrap();
        assert!(VideoFile::open(&truncated).is_err());
        let _ = std::fs::remove_file(&truncated);
    }

    #[test]
    fn decode_audio_returns_pcm() {
        // Warm up Media Foundation first-use init, as in the macOS test.
        let _ = VideoFile::decode_audio(&intro_path());
        let started = std::time::Instant::now();
        let pcm = VideoFile::decode_audio(&intro_path())
            .expect("decode audio")
            .expect("intro.mp4 has an AAC track, so audio must decode to PCM");
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "decode_audio took {:?}",
            started.elapsed()
        );
        assert!(pcm.sample_rate > 0);
        assert!(pcm.channels > 0);
        assert!(pcm.channels <= 8);
        let expected = (u64::from(pcm.sample_rate) * 3 * u64::from(pcm.channels)) as usize;
        let low = (expected as f64 * 0.9) as usize;
        let high = (expected as f64 * 1.1) as usize;
        assert!(
            pcm.samples.len() >= low && pcm.samples.len() <= high,
            "samples {} not within 10% of {}",
            pcm.samples.len(),
            expected
        );
        assert!(
            pcm.samples.iter().any(|s| *s != 0),
            "audio samples are all zero"
        );
    }
}
