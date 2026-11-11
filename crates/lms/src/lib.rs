//! CourseCraft LMS bridge logic (L4).
//!
//! Decides what a running course tells the LMS: `cmi` writes for score,
//! status, location and suspend data, plus start/resume and finish. It talks
//! to the LMS only through the [`CmiApi`]-style trait the app implements, so
//! every rule here is testable without a browser. Rules are in
//! `docs/spec/triggers.md` and `docs/plans/m2-publish.md`.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod api;
pub mod bridge;
pub mod interaction;
pub mod lrs;
pub mod lti;
pub mod marker;
pub mod time;

#[cfg(test)]
mod lrs_tests;
#[cfg(test)]
mod lti_tests;
#[cfg(test)]
mod marker_tests;
#[cfg(test)]
mod tests;

pub use api::CmiApi;
pub use bridge::{LOCATION_LIMIT_2004, LmsBridge, SUSPEND_LIMIT, SUSPEND_LIMIT_2004, StartPlan};
pub use coursecraft_scorm::Standard;
pub use coursecraft_xapi::{Flavor, LaunchError, LaunchParams};
pub use interaction::{
    InteractionKind, InteractionRecord, Response, interaction_record, sanitize_identifier,
};
pub use lrs::{
    HANDSHAKE_TIMEOUT_MS, LrsApi, LrsMethod, LrsRequest, LrsResponse, MAX_BATCH,
    MAX_KEEPALIVE_BODY, MAX_QUEUE, PROGRESS_INTERVAL_MS, Tick, XapiBridge,
};
pub use lti::{LtiReporter, ScoreReport};
pub use marker::{ReportingStandard, reporting_from_marker};
pub use time::{format_scaled_score, format_session_time, format_session_time_iso};
