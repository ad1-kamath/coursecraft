//! CourseCraft control channel (L6).
//!
//! A local JSON-RPC 2.0 protocol that lets scripts and agents drive a running
//! editor (or a headless engine session) through the same engine commands as
//! the UI. Loopback only, token-authenticated. Desktop only: not built for wasm.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

mod client;
mod dispatch;
mod host;
mod server;

#[cfg(test)]
mod tests;

pub use client::{ClientError, ControlClient};
pub use coursecraft_render::MediaSource;
pub use dispatch::{dispatch, encode_base64};
pub use host::{CommandInfo, ControlHost, HeadlessHost, HostError, Selection, SessionInfo};
pub use server::{ControlServer, ServerError, Waker, default_config_dir};
