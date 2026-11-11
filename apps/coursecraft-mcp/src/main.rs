//! `coursecraft-mcp`: the standalone binary. The desktop app runs the same code with `CourseCraft mcp`.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

fn main() -> std::process::ExitCode {
    coursecraft_mcp::main_with_args(std::env::args_os())
}
