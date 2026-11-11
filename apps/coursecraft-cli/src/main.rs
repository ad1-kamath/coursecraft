//! `coursecraft-cli`: the standalone binary. The desktop app runs the same code with `CourseCraft cli`.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

fn main() -> std::process::ExitCode {
    coursecraft_cli::main_with_args(std::env::args_os())
}
