//! The compiled web player embedded in the desktop editor at build time.
//!
//! `build.rs` writes the list from `target/player-dist`. An empty list means
//! this build has no player, so the Publish dialog can explain what to run.

use coursecraft_publish::PackageFile;

include!(concat!(env!("OUT_DIR"), "/player_bundle.rs"));

/// Whether this build embeds a player.
#[must_use]
pub fn has_player() -> bool {
    !PLAYER_FILES.is_empty()
}

/// Player files embedded at compile time, or an empty list when this build has none.
#[must_use]
pub fn player_files() -> Vec<PackageFile> {
    PLAYER_FILES
        .iter()
        .map(|entry| PackageFile {
            path: entry.0.to_owned(),
            bytes: entry.1.to_vec(),
        })
        .collect()
}
