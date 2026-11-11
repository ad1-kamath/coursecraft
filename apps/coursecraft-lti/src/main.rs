//! `coursecraft-lti`: serve a published course to an LTI 1.3 platform, or
//! print the URLs to register in that platform.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use coursecraft_lti_server::server::Server;
use coursecraft_lti_server::{Config, check_text};

#[derive(Parser)]
#[command(
    name = "coursecraft-lti",
    about = "Serve a CourseCraft LTI 1.3 tool behind your own HTTPS proxy."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Serve launches, course files and scores.
    Serve {
        /// The lti.json configuration file.
        config: PathBuf,
    },
    /// Print the tool URLs and key id to paste into the platform.
    Check {
        /// The lti.json configuration file.
        config: PathBuf,
    },
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("coursecraft-lti: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let cli = Cli::try_parse().map_err(|error| error.to_string())?;
    match cli.command {
        Command::Check { config } => {
            let config = Config::load(&config).map_err(|error| error.to_string())?;
            let tool = config.tool_config().map_err(|error| error.to_string())?;
            print!("{}", check_text(&tool));
            Ok(())
        }
        Command::Serve { config } => {
            let config = Config::load(&config).map_err(|error| error.to_string())?;
            let tool = config.tool_config().map_err(|error| error.to_string())?;
            let course_dir = config.course_dir();
            let server = Server::bind(tool, course_dir, &config.listen)
                .map_err(|error| error.to_string())?;
            match server.local_addr() {
                Ok(address) => println!("coursecraft-lti listening on http://{address}"),
                Err(error) => return Err(error.to_string()),
            }
            server.run().map_err(|error| error.to_string())
        }
    }
}
