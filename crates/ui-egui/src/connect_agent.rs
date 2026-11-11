//! Help ▸ Connect an AI agent: ready-to-copy lines for Claude Code, Codex and the CLI.
//!
//! The lines live in the Agents tab's "Set up by hand" section. This module
//! keeps the text and the small copy row; it owns no window.

use egui::{RichText, Ui};

use crate::QuietTone;

/// Where `exe` isn't a good long-term home: a disk image, or a macOS quarantine copy.
pub(crate) fn exe_location_warning(exe: &str) -> Option<&'static str> {
    (exe.starts_with("/Volumes/") || exe.contains("/AppTranslocation/"))
        .then_some("Move CourseCraft to Applications and reopen it first, or these lines stop working when this copy goes away.")
}

/// The line that registers CourseCraft with Claude Code.
#[must_use]
pub fn claude_line(exe: &str) -> String {
    format!("claude mcp add --transport stdio --scope user coursecraft -- \"{exe}\" mcp --attach")
}

/// The line that registers CourseCraft with Codex.
#[must_use]
pub fn codex_line(exe: &str) -> String {
    format!("codex mcp add coursecraft -- \"{exe}\" mcp --attach")
}

/// A line that checks the CLI can reach this app.
#[must_use]
pub fn cli_line(exe: &str) -> String {
    format!("\"{exe}\" cli editor session.info")
}

/// A message to paste to any agent, naming both routes and when to switch.
#[must_use]
pub fn agent_message(exe: &str) -> String {
    format!(
        "CourseCraft is open on this computer. Use its CLI: \"{exe}\" cli <command>. \
         Run \"{exe}\" cli guide first for every command, and \"{exe}\" cli editor <method> to change the open course. \
         If the coursecraft MCP tools are connected you can use those too. If one route fails, try the other."
    )
}

/// A label, a copy button and the line itself.
pub(crate) fn copy_row(ui: &mut Ui, label: &str, text: &str) {
    let p = coursecraft_render::palette(ui.ctx());
    ui.add_space(coursecraft_render::SPACE_LG);
    ui.horizontal(|ui| {
        ui.label(coursecraft_render::body_strong(label));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if crate::quiet_button(ui, "Copy", QuietTone::Accent, true).clicked() {
                ui.ctx().copy_text(text.to_owned());
            }
        });
    });
    egui::Frame::NONE
        .fill(p.input)
        .corner_radius(coursecraft_render::RADIUS_MD as u8)
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.add(egui::Label::new(RichText::new(text).monospace()).wrap());
        });
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    const MAC: &str = "/Applications/Course Craft.app/Contents/MacOS/CourseCraft";

    #[test]
    fn lines_quote_a_path_with_spaces() {
        assert!(claude_line(MAC).ends_with(&format!("-- \"{MAC}\" mcp --attach")));
        assert!(codex_line(MAC).ends_with(&format!("-- \"{MAC}\" mcp --attach")));
        assert_eq!(cli_line(MAC), format!("\"{MAC}\" cli editor session.info"));
    }

    #[test]
    fn warns_only_when_the_app_is_not_installed() {
        assert!(exe_location_warning(MAC).is_none());
        assert!(exe_location_warning(r"C:\Users\a\Downloads\CourseCraft.exe").is_none());
        assert!(
            exe_location_warning("/Volumes/CourseCraft/CourseCraft.app/Contents/MacOS/CourseCraft")
                .is_some()
        );
        assert!(
            exe_location_warning("/private/var/folders/x/AppTranslocation/y/CourseCraft").is_some()
        );
    }

    #[test]
    fn the_message_names_both_routes() {
        let message = agent_message(MAC);
        assert!(message.contains("cli guide"));
        assert!(message.contains("MCP"));
        assert!(message.contains("try the other"));
    }
}
