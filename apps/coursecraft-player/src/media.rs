//! Media sources for desktop and web player.

#[cfg(not(target_arch = "wasm32"))]
use std::collections::HashMap;
#[cfg(not(target_arch = "wasm32"))]
use std::path::{Path, PathBuf};
use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Mutex;

use coursecraft_render::{MediaFetch, MediaSource};

/// Desktop media source that loads files from `<course dir>/media/<src>` lazily on request.
#[cfg(not(target_arch = "wasm32"))]
pub struct DesktopMediaSource {
    media_dir: PathBuf,
    cache: Mutex<HashMap<String, Option<Arc<[u8]>>>>,
}

#[cfg(not(target_arch = "wasm32"))]
impl DesktopMediaSource {
    /// Create a new desktop media source rooted at the parent directory of `course_path`.
    pub fn new(course_path: &Path) -> Self {
        let parent = course_path.parent().unwrap_or_else(|| Path::new("."));
        let media_dir = parent.join("media");
        Self {
            media_dir,
            cache: Mutex::new(HashMap::new()),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl MediaSource for DesktopMediaSource {
    fn get(&self, src: &str) -> MediaFetch {
        if !coursecraft_format::is_safe_relative_path(src) {
            return MediaFetch::Missing;
        }

        let mut lock = match self.cache.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };

        if let Some(entry) = lock.get(src) {
            return match entry {
                Some(bytes) => MediaFetch::Ready(Arc::clone(bytes)),
                None => MediaFetch::Missing,
            };
        }

        let full_path = self.media_dir.join(src);
        match std::fs::read(&full_path) {
            Ok(bytes) => {
                let arc: Arc<[u8]> = bytes.into();
                lock.insert(src.to_string(), Some(Arc::clone(&arc)));
                MediaFetch::Ready(arc)
            }
            Err(_) => {
                lock.insert(src.to_string(), None);
                MediaFetch::Missing
            }
        }
    }
}

/// Web media source that fetches `media/<src>` relative to the page with at most 6 in-flight requests.
#[cfg(target_arch = "wasm32")]
pub struct WebMediaSource {
    state: Arc<std::sync::Mutex<WebMediaState>>,
    ctx: egui::Context,
}

#[cfg(target_arch = "wasm32")]
struct WebMediaState {
    entries: std::collections::HashMap<String, WebEntryStatus>,
    in_flight: usize,
    queue: std::collections::VecDeque<String>,
}

#[cfg(target_arch = "wasm32")]
enum WebEntryStatus {
    Ready(Arc<[u8]>),
    Pending,
    Missing,
}

#[cfg(target_arch = "wasm32")]
impl WebMediaSource {
    /// Maximum concurrent in-flight HTTP requests for media assets.
    pub const MAX_IN_FLIGHT: usize = 6;

    /// Create a new web media source that repaints `ctx` when assets arrive.
    pub fn new(ctx: egui::Context) -> Self {
        Self {
            state: Arc::new(std::sync::Mutex::new(WebMediaState {
                entries: std::collections::HashMap::new(),
                in_flight: 0,
                queue: std::collections::VecDeque::new(),
            })),
            ctx,
        }
    }

    fn maybe_start_next(state: &Arc<std::sync::Mutex<WebMediaState>>, ctx: &egui::Context) {
        let next_src = {
            let mut s = match state.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            if s.in_flight >= Self::MAX_IN_FLIGHT {
                return;
            }
            let Some(src) = s.queue.pop_front() else {
                return;
            };
            s.in_flight += 1;
            src
        };

        let state_cb = Arc::clone(state);
        let ctx_cb = ctx.clone();
        let src_cb = next_src.clone();
        let url = format!("media/{next_src}");

        ehttp::fetch(ehttp::Request::get(url), move |response| {
            {
                let mut s = match state_cb.lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => poisoned.into_inner(),
                };
                s.in_flight = s.in_flight.saturating_sub(1);
                match response {
                    Ok(resp) if resp.status == 200 => {
                        s.entries
                            .insert(src_cb, WebEntryStatus::Ready(resp.bytes.into()));
                    }
                    _ => {
                        s.entries.insert(src_cb, WebEntryStatus::Missing);
                    }
                }
            }
            ctx_cb.request_repaint();
            Self::maybe_start_next(&state_cb, &ctx_cb);
        });
    }
}

#[cfg(target_arch = "wasm32")]
impl MediaSource for WebMediaSource {
    fn get(&self, src: &str) -> MediaFetch {
        if !coursecraft_format::is_safe_relative_path(src) {
            return MediaFetch::Missing;
        }

        let mut s = match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };

        if let Some(status) = s.entries.get(src) {
            return match status {
                WebEntryStatus::Ready(bytes) => MediaFetch::Ready(Arc::clone(bytes)),
                WebEntryStatus::Pending => MediaFetch::Pending,
                WebEntryStatus::Missing => MediaFetch::Missing,
            };
        }

        s.entries.insert(src.to_string(), WebEntryStatus::Pending);
        s.queue.push_back(src.to_string());
        drop(s);

        Self::maybe_start_next(&self.state, &self.ctx);
        MediaFetch::Pending
    }
}
