//! CourseCraft engine (L5): an editing [`Session`] plus the command registry.
//!
//! Rule: every user-visible editing action is a [`Command`] with a stable id.
//! The egui shell, the CLI, the control channel and the MCP server all call
//! [`Session::execute`]; none of them edit the course directly. Commands are
//! atomic: if one fails, the course is restored exactly as it was.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

mod commands;
mod connectors;
mod slide_size;

pub use connectors::reroute_connectors;

use coursecraft_course::{Course, PlayerSettings, SCHEMA_VERSION, Scene, Size, Slide, Theme};
use serde_json::Value as Json;

pub use commands::registry;

/// Undo steps kept per session.
pub const MAX_HISTORY: usize = 200;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum EngineError {
    #[error("unknown command {0:?}")]
    UnknownCommand(String),
    #[error("bad params for {command}: {message}")]
    BadParams { command: String, message: String },
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Invalid(String),
    #[error("nothing to {0}")]
    NothingTo(&'static str),
    #[error("step {step} ({command}): {error}")]
    BatchFailed {
        step: usize,
        command: String,
        error: Box<EngineError>,
    },
}

pub type Result<T> = std::result::Result<T, EngineError>;

/// A registered command.
pub struct Command {
    /// Stable id, e.g. `slide.add`. Never rename a shipped id.
    pub id: &'static str,
    pub label: &'static str,
    /// Menu path in the editor, e.g. `Insert/New Slide`. Empty = not in a menu.
    pub menu: &'static str,
    pub shortcut: Option<&'static str>,
    /// Human/agent-readable params description.
    pub params: &'static str,
    /// Mutating commands snapshot the course for undo and roll back on error.
    pub mutates: bool,
    pub run: fn(&mut Session, &Json) -> Result<Json>,
}

/// An open course being edited.
pub struct Session {
    pub course: Course,
    undo: Vec<Course>,
    redo: Vec<Course>,
    /// Bumped on every successful mutating command (UI caches key off this).
    pub revision: u64,
    /// Set by a mutating command that changed nothing (for example a replace
    /// with no matches). The next commit then makes no undo step and no
    /// revision bump. Taken by [`Session::execute`] and
    /// [`Session::execute_batch`] after every command, so it never leaks.
    skip_undo: bool,
}

impl Session {
    #[must_use]
    pub fn new(course: Course) -> Self {
        Self {
            course,
            undo: Vec::new(),
            redo: Vec::new(),
            revision: 0,
            skip_undo: false,
        }
    }

    /// A one-scene, one-slide course.
    #[must_use]
    pub fn blank(title: &str) -> Self {
        Self::new(Course {
            schema_version: SCHEMA_VERSION,
            id: "course".into(),
            title: title.into(),
            slide_size: Size::default(),
            theme: Theme::default(),
            variables: Vec::new(),
            scenes: vec![Scene {
                id: "scene1".into(),
                title: "Scene 1".into(),
                slides: vec![Slide {
                    id: "slide1".into(),
                    title: "Slide 1".into(),
                    duration_ms: 5000,
                    background: None,
                    objects: Vec::new(),
                    layers: Vec::new(),
                    triggers: Vec::new(),
                    transition: None,
                    question: None,
                    draw: None,
                    notes: String::new(),
                    master: None,
                    revisit: coursecraft_course::RevisitMode::Auto,
                    player: None,
                }],
            }],
            masters: Vec::new(),
            templates: Vec::new(),
            banks: Vec::new(),
            start_slide: None,
            quiz: None,
            player: PlayerSettings::default(),
            language: None,
            strings: std::collections::BTreeMap::new(),
            fonts: Vec::new(),
            background_audio: None,
            glossary: Vec::new(),
            resources: Vec::new(),
            text_styles: Vec::new(),
        })
    }

    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Number of undo steps currently recorded (used by editor tests).
    #[must_use]
    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }

    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Run a command by id.
    ///
    /// # Errors
    /// Unknown id, bad params, or a command-specific failure. On error a
    /// mutating command leaves the course untouched.
    pub fn execute(&mut self, id: &str, params: &Json) -> Result<Json> {
        let cmd = registry()
            .iter()
            .find(|c| c.id == id)
            .ok_or_else(|| EngineError::UnknownCommand(id.to_owned()))?;
        if !cmd.mutates {
            return (cmd.run)(self, params);
        }
        let snapshot = self.course.clone();
        self.skip_undo = false;
        let outcome = (cmd.run)(self, params);
        let unchanged = std::mem::take(&mut self.skip_undo);
        match outcome {
            Ok(v) if unchanged => Ok(v),
            Ok(v) => {
                reroute_connectors(&mut self.course);
                self.undo.push(snapshot);
                if self.undo.len() > MAX_HISTORY {
                    self.undo.remove(0);
                }
                self.redo.clear();
                self.revision = self.revision.wrapping_add(1);
                Ok(v)
            }
            Err(e) => {
                self.course = snapshot;
                Err(e)
            }
        }
    }

    /// Run a batch of commands atomically.
    ///
    /// On success, pushes a single undo snapshot and increments revision once (if any command mutates).
    /// On failure of any step, restores the course to its state before the batch,
    /// and returns the failing error wrapped in [`EngineError::BatchFailed`].
    /// Non-mutating steps are allowed inside a batch. An empty batch is a no-op.
    ///
    /// # Errors
    /// Returns [`EngineError::BatchFailed`] on the first command error.
    pub fn execute_batch(&mut self, steps: &[(String, Json)]) -> Result<Vec<Json>> {
        if steps.is_empty() {
            return Ok(Vec::new());
        }

        let snapshot = self.course.clone();
        let mut results = Vec::with_capacity(steps.len());
        let mut any_changed = false;

        for (i, (id, params)) in steps.iter().enumerate() {
            let cmd = registry()
                .iter()
                .find(|c| c.id == id.as_str())
                .ok_or_else(|| EngineError::BatchFailed {
                    step: i,
                    command: id.clone(),
                    error: Box::new(EngineError::UnknownCommand(id.clone())),
                });
            let cmd = match cmd {
                Ok(c) => c,
                Err(e) => {
                    self.course = snapshot;
                    return Err(e);
                }
            };

            self.skip_undo = false;
            let outcome = (cmd.run)(self, params);
            // A step that changed nothing must not make an undo step on its own.
            // Other steps in the batch still do.
            if std::mem::take(&mut self.skip_undo) {
                // Nothing to record for this step.
            } else if cmd.mutates {
                any_changed = true;
            }
            match outcome {
                Ok(res) => results.push(res),
                Err(e) => {
                    self.course = snapshot;
                    return Err(EngineError::BatchFailed {
                        step: i,
                        command: id.clone(),
                        error: Box::new(e),
                    });
                }
            }
        }

        if any_changed {
            reroute_connectors(&mut self.course);
            self.undo.push(snapshot);
            if self.undo.len() > MAX_HISTORY {
                self.undo.remove(0);
            }
            self.redo.clear();
            self.revision = self.revision.wrapping_add(1);
        }

        Ok(results)
    }

    pub fn undo(&mut self) -> Result<()> {
        let prev = self.undo.pop().ok_or(EngineError::NothingTo("undo"))?;
        self.redo.push(std::mem::replace(&mut self.course, prev));
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }

    pub fn redo(&mut self) -> Result<()> {
        let next = self.redo.pop().ok_or(EngineError::NothingTo("redo"))?;
        self.undo.push(std::mem::replace(&mut self.course, next));
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }
}

#[cfg(test)]
mod connector_tests;
#[cfg(test)]
mod csv_tests;
#[cfg(test)]
mod find_tests;
#[cfg(test)]
mod outline_tests;
#[cfg(test)]
mod styles_tests;
#[cfg(test)]
mod tests;
