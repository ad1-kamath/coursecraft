//! CourseCraft export (L4).
//!
//! Writes a course out in the shapes people ask for besides an LMS package.
//! The MP4 writer follows the linear play path through the shared runtime and
//! draws every frame with the same compositor as the player. The OS encoder
//! does the H.264 and AAC work, so no encoder is bundled.
//!
//! Step 2 adds the PDF and Word review documents.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

#[cfg(not(target_arch = "wasm32"))]
pub mod audio_mix;
pub mod docx;
#[cfg(not(target_arch = "wasm32"))]
pub mod mp4;
pub mod pages;
pub mod pdf;

pub use docx::{export_docx, write_docx};
pub use pages::{ExportError, ReviewOptions, ReviewPage, review_pages};
pub use pdf::{export_pdf, write_pdf};

#[cfg(not(target_arch = "wasm32"))]
pub use mp4::{ExportMedia, ExportProgress, ExportReport, Mp4Options, branch_slides, export_mp4};

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;
