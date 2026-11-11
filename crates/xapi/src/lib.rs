//! xAPI and cmi5 vocabulary for CourseCraft.
//!
//! Implemented from the public Experience API 1.0.3 specification and the
//! cmi5 profile. Standalone L0 crate: no workspace dependencies, so it builds
//! for wasm and can be reused by other tools.
//!
//! Scope: launch parameters, statement builders, and the cmi5 / Tin Can
//! package manifests. The bridge that sends these statements lives in
//! `coursecraft-lms`.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

mod flavor;
mod launch;
mod package;
mod statements;

#[cfg(test)]
mod tests;

pub use flavor::Flavor;
pub use launch::{LaunchError, LaunchParams};
pub use package::{PackageSpec, activity_id_for, cmi5_xml, tincan_xml};
pub use statements::{AnsweredInput, StatementContext};

/// Maximum launch-query length accepted by [`LaunchParams::parse`].
pub const MAX_LAUNCH_QUERY: usize = 16 * 1024;
