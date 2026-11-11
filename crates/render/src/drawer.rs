//! The outline drawer's tabs: Menu, Resources, Glossary and Transcript.
//!
//! Only state and lookups live here, shared by the painter, the focus order and
//! the access tree so they can never disagree about what the drawer shows.

use coursecraft_course::{Course, GlossaryEntry};

/// One tab of the drawer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum DrawerTab {
    /// The slide outline.
    #[default]
    Menu,
    /// Links and files.
    Resources,
    /// Terms and definitions.
    Glossary,
    /// The current slide's notes.
    Transcript,
}

impl DrawerTab {
    /// The key of the chrome string that labels this tab.
    #[must_use]
    pub const fn label_key(self) -> &'static str {
        match self {
            Self::Menu => "tabMenu",
            Self::Resources => "tabResources",
            Self::Glossary => "tabGlossary",
            Self::Transcript => "tabTranscript",
        }
    }
}

/// What the learner has chosen inside the drawer.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DrawerView {
    /// The tab the learner picked. Use [`DrawerView::effective_tab`] to draw.
    pub tab: DrawerTab,
    /// The glossary search text.
    pub query: String,
    /// The glossary term whose definition is open.
    pub selected: Option<String>,
}

/// The tabs this course offers, Menu first. A tab appears only when it has
/// content and its setting allows it.
#[must_use]
pub fn available_tabs(course: &Course) -> Vec<DrawerTab> {
    let mut tabs = vec![DrawerTab::Menu];
    if !course.resources.is_empty() {
        tabs.push(DrawerTab::Resources);
    }
    if !course.glossary.is_empty() {
        tabs.push(DrawerTab::Glossary);
    }
    if course.player.show_transcript {
        tabs.push(DrawerTab::Transcript);
    }
    tabs
}

impl DrawerView {
    /// The tab to show: the chosen one, or Menu when it is no longer offered.
    #[must_use]
    pub fn effective_tab(&self, course: &Course) -> DrawerTab {
        if available_tabs(course).contains(&self.tab) {
            self.tab
        } else {
            DrawerTab::Menu
        }
    }
}

/// Glossary entries whose term contains `query` (ignoring case), A to Z.
#[must_use]
pub fn visible_glossary<'a>(course: &'a Course, query: &str) -> Vec<&'a GlossaryEntry> {
    let needle = query.trim().to_lowercase();
    let mut entries: Vec<&GlossaryEntry> = course
        .glossary
        .iter()
        .filter(|entry| needle.is_empty() || entry.term.to_lowercase().contains(&needle))
        .collect();
    entries.sort_by_key(|entry| entry.term.to_lowercase());
    entries
}
