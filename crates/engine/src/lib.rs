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
#[allow(dead_code)]
mod delta;
mod slide_size;
#[cfg(debug_assertions)]
pub mod test_counts;

pub use connectors::reroute_connectors;

use std::collections::VecDeque;

use coursecraft_course::{Course, PlayerSettings, SCHEMA_VERSION, Scene, Size, Slide, Theme};
use serde_json::Value as Json;

use crate::delta::{
    CourseDelta, CourseHeaderPatch, CourseHeaderSnapshot, SlideDelta, SlideLoc, UndoScope,
    slide_at, slide_at_mut,
};
pub use commands::registry;

/// Undo steps kept per session.
pub const MAX_HISTORY: usize = 200;

fn clone_course_counted(course: &Course) -> Course {
    #[cfg(debug_assertions)]
    test_counts::note_full_course_clone();
    course.clone()
}

#[cfg(test)]
thread_local! {
    static VERIFY_SCOPE: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
    static FORCE_FULL_VALIDATION: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
pub(crate) fn with_full_validation_forced<T>(f: impl FnOnce() -> T) -> T {
    struct Reset(bool);
    impl Drop for Reset {
        fn drop(&mut self) {
            FORCE_FULL_VALIDATION.set(self.0);
        }
    }
    let prev = FORCE_FULL_VALIDATION.replace(true);
    let _guard = Reset(prev);
    f()
}

#[cfg(test)]
pub(crate) fn with_scope_verifier_disabled<T>(f: impl FnOnce() -> T) -> T {
    struct Reset(bool);
    impl Drop for Reset {
        fn drop(&mut self) {
            VERIFY_SCOPE.set(self.0);
        }
    }
    let prev = VERIFY_SCOPE.replace(false);
    let _guard = Reset(prev);
    f()
}

#[cfg(test)]
fn verify_single_slide_isolation(before: &Course, after: &Course, loc: SlideLoc, label: &str) {
    if !VERIFY_SCOPE.get() {
        return;
    }
    let after_slide = slide_at(after, loc);
    assert!(
        after_slide.is_some(),
        "scope isolation violation ({label}): slide at {loc:?} disappeared"
    );
    let mut expected = before.clone();
    let slot = slide_at_mut(&mut expected, loc);
    assert!(
        slot.is_some(),
        "scope isolation violation ({label}): slide at {loc:?} missing in before snapshot"
    );
    if let (Some(slot), Some(after_slide)) = (slot, after_slide) {
        *slot = after_slide.clone();
    }
    assert_eq!(
        *after, expected,
        "scope isolation violation ({label}): SingleSlide command mutated state outside {loc:?}"
    );
}

#[cfg(test)]
fn verify_course_header_isolation(before: &Course, after: &Course, label: &str) {
    if !VERIFY_SCOPE.get() {
        return;
    }
    assert_eq!(
        after.scenes, before.scenes,
        "scope isolation violation ({label}): CourseHeader command mutated scenes"
    );
    assert_eq!(
        after.masters, before.masters,
        "scope isolation violation ({label}): CourseHeader command mutated masters"
    );
    assert_eq!(
        after.templates, before.templates,
        "scope isolation violation ({label}): CourseHeader command mutated templates"
    );
    assert_eq!(
        after.banks, before.banks,
        "scope isolation violation ({label}): CourseHeader command mutated banks"
    );
}

fn same_slide_batch_loc(course: &Course, steps: &[(String, Json)]) -> Option<SlideLoc> {
    let mut target_slide: Option<&str> = None;
    for (id, params) in steps {
        let cmd = registry().iter().find(|c| c.id == id.as_str())?;
        if !cmd.mutates {
            continue;
        }
        if cmd.scope() != UndoScope::SingleSlide {
            return None;
        }
        let sid = params.get("slide").and_then(Json::as_str)?;
        match target_slide {
            None => target_slide = Some(sid),
            Some(prev) if prev == sid => {}
            Some(_) => return None,
        }
    }
    let sid = target_slide?;
    SlideLoc::find_editable(course, sid)
}

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
    pub(crate) scope: UndoScope,
    pub(crate) slide_validation: bool,
    pub run: fn(&mut Session, &Json) -> Result<Json>,
}

impl Command {
    #[must_use]
    pub(crate) fn scope(&self) -> UndoScope {
        self.scope
    }

    #[must_use]
    pub(crate) fn can_use_slide_validation(&self) -> bool {
        self.slide_validation
    }
}

/// An open course being edited.
pub struct Session {
    pub course: Course,
    undo: VecDeque<(CourseDelta, String)>,
    redo: VecDeque<(CourseDelta, String)>,
    /// Bumped on every successful mutating command (UI caches key off this).
    pub revision: u64,
    /// Set by a mutating command that changed nothing (for example a replace
    /// with no matches). The next commit then makes no undo step and no
    /// revision bump. Taken by [`Session::execute`] and
    /// [`Session::execute_batch`] after every command, so it never leaks.
    skip_undo: bool,
    /// Set once older undo steps are dropped to stay within [`MAX_HISTORY`].
    history_truncated: bool,
    /// Last known `course.is_playable()` result for a given `revision`.
    valid_at_revision: Option<(u64, bool)>,
    /// Set by `keep_valid` when the mutated course passed validation.
    validated_in_cmd: bool,
    /// Active single-slide target while running a command eligible for scoped
    /// slide validation; always cleared immediately after `cmd.run`.
    active_single_slide: Option<SlideLoc>,
    /// Cached full `course.validate()` output keyed by [`Session::revision`].
    last_validation: Option<(u64, Vec<coursecraft_course::Issue>)>,
    /// If the current [`Session::revision`] came from a single-slide edit, its
    /// `(revision, slide_id)` pair.
    last_single_slide_edit: Option<(u64, String)>,
    #[cfg(test)]
    pub(crate) is_playable_checks: usize,
}

impl Session {
    #[must_use]
    pub fn new(course: Course) -> Self {
        Self {
            course,
            undo: VecDeque::new(),
            redo: VecDeque::new(),
            revision: 0,
            skip_undo: false,
            history_truncated: false,
            valid_at_revision: None,
            validated_in_cmd: false,
            active_single_slide: None,
            last_validation: None,
            last_single_slide_edit: None,
            #[cfg(test)]
            is_playable_checks: 0,
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
                    groups: Vec::new(),
                    triggers: Vec::new(),
                    cue_points: Vec::new(),
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
            tracking: Default::default(),
            player: PlayerSettings::default(),
            language: None,
            strings: std::collections::BTreeMap::new(),
            fonts: Vec::new(),
            background_audio: None,
            glossary: Vec::new(),
            editor: Default::default(),
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

    /// Names of recorded undo steps, newest first.
    #[must_use]
    pub fn undo_names(&self) -> Vec<&str> {
        self.undo
            .iter()
            .rev()
            .map(|(_, name)| name.as_str())
            .collect()
    }

    /// Names of recorded redo steps, next redo first.
    #[must_use]
    pub fn redo_names(&self) -> Vec<&str> {
        self.redo
            .iter()
            .rev()
            .map(|(_, name)| name.as_str())
            .collect()
    }

    /// Estimated heap + stack bytes retained across the undo and redo stacks.
    #[must_use]
    pub fn undo_estimated_bytes(&self) -> usize {
        self.undo
            .iter()
            .chain(self.redo.iter())
            .fold(0usize, |acc, (delta, _)| {
                acc.saturating_add(delta.estimated_bytes())
            })
    }

    /// Merge the two most recent undo steps into one reversible step.
    ///
    /// Returns `false` if fewer than two undo steps are recorded. When `label`
    /// is `None`, the older step's label is kept. Does not bump `revision` and
    /// does not clear the redo stack.
    pub fn merge_last_two_undos(&mut self, label: Option<&str>) -> bool {
        if self.undo.len() < 2 {
            return false;
        }
        let (Some((newer, _)), Some((older, older_label))) =
            (self.undo.pop_back(), self.undo.pop_back())
        else {
            return false;
        };
        let merged = CourseDelta::merge(older, newer);
        let merged_label = label.map_or(older_label, str::to_owned);
        self.undo.push_back((merged, merged_label));
        true
    }

    fn commit_delta(&mut self, delta: CourseDelta, label: String) {
        self.undo.push_back((delta, label));
        if self.undo.len() > MAX_HISTORY {
            self.undo.pop_front();
            self.history_truncated = true;
        }
        self.redo.clear();
        self.last_validation = None;
        self.last_single_slide_edit = None;
        self.revision = self.revision.wrapping_add(1);
    }

    /// If the current [`Session::revision`] came from a single-slide edit,
    /// return that slide's id so UI caches can skip untouched slides.
    #[must_use]
    pub fn last_changed_slide(&self) -> Option<&str> {
        self.last_single_slide_edit
            .as_ref()
            .filter(|(rev, _)| *rev == self.revision)
            .map(|(_, id)| id.as_str())
    }

    /// Full validation issues for the current course, cached by [`Session::revision`].
    ///
    /// Also populates the cached playability state for this revision so subsequent
    /// commands do not need to re-run full-course playability checks.
    pub fn validation_issues(&mut self) -> &[coursecraft_course::Issue] {
        let stale = !matches!(self.last_validation, Some((rev, _)) if rev == self.revision);
        if stale {
            let issues = self.course.validate();
            let no_errors = !issues
                .iter()
                .any(|i| i.severity == coursecraft_course::Severity::Error);
            self.valid_at_revision = Some((self.revision, no_errors));
            self.last_validation = Some((self.revision, issues));
        }
        match self.last_validation.as_ref() {
            Some((_, issues)) => issues.as_slice(),
            None => &[],
        }
    }

    fn scoped_slide_for(cmd: &Command, loc: SlideLoc) -> Option<SlideLoc> {
        #[cfg(test)]
        if FORCE_FULL_VALIDATION.get() {
            return None;
        }
        cmd.can_use_slide_validation().then_some(loc)
    }

    /// Whether the course currently passes validation without errors.
    ///
    /// Caches the result per [`Session::revision`] so consecutive commands on an
    /// unchanged course do not re-validate it.
    pub(crate) fn is_playable(&mut self) -> bool {
        if let Some((rev, valid)) = self.valid_at_revision
            && rev == self.revision
        {
            return valid;
        }
        #[cfg(test)]
        {
            self.is_playable_checks += 1;
        }
        #[cfg(debug_assertions)]
        crate::test_counts::note_full_course_validation();
        let valid = self.course.is_playable();
        self.valid_at_revision = Some((self.revision, valid));
        valid
    }

    pub(crate) fn mark_validated_valid(&mut self) {
        self.validated_in_cmd = true;
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
        match cmd.scope() {
            UndoScope::SingleSlide => {
                if let Some(loc) = params
                    .get("slide")
                    .and_then(Json::as_str)
                    .and_then(|sid| SlideLoc::find_editable(&self.course, sid))
                {
                    return self.execute_single_slide(cmd, params, loc);
                }
                self.execute_full_course(cmd, params)
            }
            UndoScope::CourseHeader => self.execute_course_header(cmd, params),
            UndoScope::FullCourse => self.execute_full_course(cmd, params),
        }
    }

    fn execute_single_slide(
        &mut self,
        cmd: &Command,
        params: &Json,
        loc: SlideLoc,
    ) -> Result<Json> {
        let Some(before_slide) = slide_at(&self.course, loc).cloned() else {
            return self.execute_full_course(cmd, params);
        };
        #[cfg(test)]
        let verify_before = VERIFY_SCOPE.get().then(|| self.course.clone());
        self.skip_undo = false;
        self.validated_in_cmd = false;
        self.active_single_slide = Self::scoped_slide_for(cmd, loc);
        let outcome = (cmd.run)(self, params);
        self.active_single_slide = None;
        let skip_undo = std::mem::take(&mut self.skip_undo);
        match outcome {
            Ok(v) => {
                #[cfg(test)]
                if let Some(ref before) = verify_before {
                    verify_single_slide_isolation(before, &self.course, loc, cmd.id);
                }
                let unchanged = skip_undo || slide_at(&self.course, loc) == Some(&before_slide);
                if unchanged {
                    return Ok(v);
                }
                if let Some(slide) = slide_at_mut(&mut self.course, loc) {
                    connectors::reroute_connectors_in_slide(slide);
                }
                let changed_id = slide_at(&self.course, loc).map(|s| s.id.clone());
                let delta = slide_at(&self.course, loc)
                    .and_then(|after_slide| SlideDelta::diff(before_slide, after_slide))
                    .map(|change| CourseDelta::SlideChange { loc, change });
                if let Some(delta) = delta {
                    self.commit_delta(delta, cmd.label.to_owned());
                    self.last_single_slide_edit = changed_id.map(|id| (self.revision, id));
                    if self.validated_in_cmd {
                        self.valid_at_revision = Some((self.revision, true));
                    }
                }
                Ok(v)
            }
            Err(e) => {
                if let Some(slot) = slide_at_mut(&mut self.course, loc) {
                    *slot = before_slide;
                }
                #[cfg(test)]
                if let Some(ref before) = verify_before {
                    assert_eq!(
                        self.course, *before,
                        "scope isolation violation ({}): SingleSlide error rollback left course modified",
                        cmd.id
                    );
                }
                Err(e)
            }
        }
    }

    fn execute_course_header(&mut self, cmd: &Command, params: &Json) -> Result<Json> {
        let before_header = CourseHeaderSnapshot::capture(&self.course);
        #[cfg(test)]
        let verify_before = VERIFY_SCOPE.get().then(|| self.course.clone());
        self.skip_undo = false;
        self.validated_in_cmd = false;
        let outcome = (cmd.run)(self, params);
        let skip_undo = std::mem::take(&mut self.skip_undo);
        match outcome {
            Ok(v) => {
                #[cfg(test)]
                if let Some(ref before) = verify_before {
                    verify_course_header_isolation(before, &self.course, cmd.id);
                }
                if skip_undo {
                    return Ok(v);
                }
                let Some(patch) = CourseHeaderPatch::diff(before_header, &self.course) else {
                    return Ok(v);
                };
                self.commit_delta(CourseDelta::Header(Box::new(patch)), cmd.label.to_owned());
                if self.validated_in_cmd {
                    self.valid_at_revision = Some((self.revision, true));
                }
                Ok(v)
            }
            Err(e) => {
                before_header.restore(&mut self.course);
                #[cfg(test)]
                if let Some(ref before) = verify_before {
                    assert_eq!(
                        self.course, *before,
                        "scope isolation violation ({}): CourseHeader error rollback left course modified",
                        cmd.id
                    );
                }
                Err(e)
            }
        }
    }

    fn execute_full_course(&mut self, cmd: &Command, params: &Json) -> Result<Json> {
        let snapshot = clone_course_counted(&self.course);
        self.skip_undo = false;
        self.validated_in_cmd = false;
        let outcome = (cmd.run)(self, params);
        // A command that leaves the course exactly as it was (an inspector
        // re-sending the value it already has) records no undo step and no
        // new revision, so Undo never "does nothing".
        let unchanged = std::mem::take(&mut self.skip_undo) || self.course == snapshot;
        match outcome {
            Ok(v) if unchanged => Ok(v),
            Ok(v) => {
                reroute_connectors(&mut self.course);
                if let Some(delta) = CourseDelta::diff(snapshot, &self.course) {
                    self.commit_delta(delta, cmd.label.to_owned());
                    if self.validated_in_cmd {
                        self.valid_at_revision = Some((self.revision, true));
                    }
                }
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
        self.execute_batch_with_name(None, steps)
    }

    /// Run a batch of commands atomically with an explicit undo-step name.
    ///
    /// # Errors
    /// Returns [`EngineError::BatchFailed`] on the first command error.
    pub fn execute_batch_named(
        &mut self,
        name: &str,
        steps: &[(String, Json)],
    ) -> Result<Vec<Json>> {
        self.execute_batch_with_name(Some(name), steps)
    }

    fn execute_batch_with_name(
        &mut self,
        name: Option<&str>,
        steps: &[(String, Json)],
    ) -> Result<Vec<Json>> {
        if steps.is_empty() {
            return Ok(Vec::new());
        }
        for (i, (id, _)) in steps.iter().enumerate() {
            if matches!(id.as_str(), "edit.undo" | "edit.redo") {
                return Err(EngineError::BatchFailed {
                    step: i,
                    command: id.clone(),
                    error: Box::new(EngineError::Invalid(format!(
                        "{id} is not allowed inside a batch"
                    ))),
                });
            }
        }

        if let Some(loc) = same_slide_batch_loc(&self.course, steps) {
            return self.execute_batch_single_slide(name, steps, loc);
        }

        let snapshot = clone_course_counted(&self.course);
        let saved_valid = self.valid_at_revision;
        let mut results = Vec::with_capacity(steps.len());
        let mut any_changed = false;
        let mut first_label: Option<&'static str> = None;

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
                    self.valid_at_revision = saved_valid;
                    self.last_validation = None;
                    return Err(e);
                }
            };

            self.skip_undo = false;
            self.validated_in_cmd = false;
            let outcome = (cmd.run)(self, params);
            let skip_undo = std::mem::take(&mut self.skip_undo);
            if cmd.mutates && !skip_undo {
                self.last_validation = None;
                self.valid_at_revision = None;
            }
            match outcome {
                Ok(res) => {
                    // A step that changed nothing must not make an undo step on its own.
                    // Other steps in the batch still do.
                    if !skip_undo && cmd.mutates {
                        any_changed = true;
                        if name.is_none() && first_label.is_none() && self.course != snapshot {
                            first_label = Some(cmd.label);
                        }
                    }
                    results.push(res);
                }
                Err(e) => {
                    self.course = snapshot;
                    self.valid_at_revision = saved_valid;
                    self.last_validation = None;
                    return Err(EngineError::BatchFailed {
                        step: i,
                        command: id.clone(),
                        error: Box::new(e),
                    });
                }
            }
        }

        if any_changed && self.course != snapshot {
            reroute_connectors(&mut self.course);
            let label = name
                .map(str::to_owned)
                .or_else(|| first_label.map(str::to_owned))
                .unwrap_or_else(|| "Batch".to_owned());
            if let Some(delta) = CourseDelta::diff(snapshot, &self.course) {
                self.commit_delta(delta, label);
            }
        }

        Ok(results)
    }

    fn execute_batch_single_slide(
        &mut self,
        name: Option<&str>,
        steps: &[(String, Json)],
        loc: SlideLoc,
    ) -> Result<Vec<Json>> {
        let Some(before_slide) = slide_at(&self.course, loc).cloned() else {
            return Err(EngineError::NotFound(format!("slide at {loc:?} not found")));
        };
        #[cfg(test)]
        let verify_before = VERIFY_SCOPE.get().then(|| self.course.clone());
        let saved_valid = self.valid_at_revision;
        let mut results = Vec::with_capacity(steps.len());
        let mut any_changed = false;
        let mut batch_validated_valid = false;
        let mut first_label: Option<&'static str> = None;

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
                    if let Some(slot) = slide_at_mut(&mut self.course, loc) {
                        *slot = before_slide;
                    }
                    self.valid_at_revision = saved_valid;
                    self.last_validation = None;
                    return Err(e);
                }
            };

            self.skip_undo = false;
            self.validated_in_cmd = false;
            self.active_single_slide = Self::scoped_slide_for(cmd, loc);
            let outcome = (cmd.run)(self, params);
            self.active_single_slide = None;
            let skip_undo = std::mem::take(&mut self.skip_undo);
            if cmd.mutates && !skip_undo {
                self.last_validation = None;
                if self.validated_in_cmd {
                    self.valid_at_revision = Some((self.revision, true));
                    batch_validated_valid = true;
                } else {
                    self.valid_at_revision = None;
                    batch_validated_valid = false;
                }
            }
            match outcome {
                Ok(res) => {
                    if !skip_undo && cmd.mutates {
                        any_changed = true;
                        if name.is_none()
                            && first_label.is_none()
                            && slide_at(&self.course, loc) != Some(&before_slide)
                        {
                            first_label = Some(cmd.label);
                        }
                    }
                    results.push(res);
                }
                Err(e) => {
                    if let Some(slot) = slide_at_mut(&mut self.course, loc) {
                        *slot = before_slide;
                    }
                    self.valid_at_revision = saved_valid;
                    self.last_validation = None;
                    #[cfg(test)]
                    if let Some(ref before) = verify_before {
                        assert_eq!(
                            self.course, *before,
                            "scope isolation violation (batch step {i} {id}): SingleSlide rollback left course modified"
                        );
                    }
                    return Err(EngineError::BatchFailed {
                        step: i,
                        command: id.clone(),
                        error: Box::new(e),
                    });
                }
            }
        }

        #[cfg(test)]
        if let Some(ref before) = verify_before {
            verify_single_slide_isolation(before, &self.course, loc, "batch");
        }

        if any_changed && slide_at(&self.course, loc) != Some(&before_slide) {
            if let Some(slide) = slide_at_mut(&mut self.course, loc) {
                connectors::reroute_connectors_in_slide(slide);
            }
            let label = name
                .map(str::to_owned)
                .or_else(|| first_label.map(str::to_owned))
                .unwrap_or_else(|| "Batch".to_owned());
            let changed_id = slide_at(&self.course, loc).map(|s| s.id.clone());
            let delta = slide_at(&self.course, loc)
                .and_then(|after_slide| SlideDelta::diff(before_slide, after_slide))
                .map(|change| CourseDelta::SlideChange { loc, change });
            if let Some(delta) = delta {
                self.commit_delta(delta, label);
                self.last_single_slide_edit = changed_id.map(|id| (self.revision, id));
                if batch_validated_valid {
                    self.valid_at_revision = Some((self.revision, true));
                }
            }
        }

        Ok(results)
    }

    pub fn undo(&mut self) -> Result<()> {
        let (mut delta, name) = self.undo.pop_back().ok_or(EngineError::NothingTo("undo"))?;
        if let Err(err) = delta.apply(&mut self.course) {
            self.undo.push_back((delta, name));
            return Err(err);
        }
        self.redo.push_back((delta, name));
        self.valid_at_revision = None;
        self.last_validation = None;
        self.last_single_slide_edit = None;
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }

    pub fn redo(&mut self) -> Result<()> {
        let (mut delta, name) = self.redo.pop_back().ok_or(EngineError::NothingTo("redo"))?;
        if let Err(err) = delta.apply(&mut self.course) {
            self.redo.push_back((delta, name));
            return Err(err);
        }
        self.undo.push_back((delta, name));
        self.valid_at_revision = None;
        self.last_validation = None;
        self.last_single_slide_edit = None;
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }

    /// Undo `n` steps atomically: if fewer than `n` steps are available,
    /// returns [`EngineError::NothingTo`] and leaves the course untouched.
    pub fn undo_steps(&mut self, n: usize) -> Result<()> {
        if n > self.undo.len() {
            return Err(EngineError::NothingTo("undo"));
        }
        let saved_revision = self.revision;
        let saved_valid = self.valid_at_revision;
        for done in 0..n {
            if let Err(err) = self.undo() {
                for _ in 0..done {
                    let _ = self.redo();
                }
                self.revision = saved_revision;
                self.valid_at_revision = saved_valid;
                self.last_validation = None;
                return Err(err);
            }
        }
        Ok(())
    }

    /// Redo `n` steps atomically: if fewer than `n` steps are available,
    /// returns [`EngineError::NothingTo`] and leaves the course untouched.
    pub fn redo_steps(&mut self, n: usize) -> Result<()> {
        if n > self.redo.len() {
            return Err(EngineError::NothingTo("redo"));
        }
        let saved_revision = self.revision;
        let saved_valid = self.valid_at_revision;
        for done in 0..n {
            if let Err(err) = self.redo() {
                for _ in 0..done {
                    let _ = self.undo();
                }
                self.revision = saved_revision;
                self.valid_at_revision = saved_valid;
                self.last_validation = None;
                return Err(err);
            }
        }
        Ok(())
    }

    /// Whether older undo steps were dropped because history exceeded [`MAX_HISTORY`].
    #[must_use]
    pub fn history_truncated(&self) -> bool {
        self.history_truncated
    }
}

#[cfg(test)]
mod anim_timing_tests;
#[cfg(test)]
mod bank_master_tests;
#[cfg(test)]
mod bughunt_m25_engine;
#[cfg(test)]
mod bughunt_undo_tests;
#[cfg(test)]
mod connector_tests;
#[cfg(test)]
mod csv_tests;
#[cfg(test)]
mod find_tests;
#[cfg(test)]
mod m27_all_commands_undo_tests;
#[cfg(test)]
mod m29_state_commands_tests;
#[cfg(test)]
mod m39_t_tests;
#[cfg(test)]
mod outline_tests;
#[cfg(test)]
mod styles_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod u21_publish_settings_tests;
#[cfg(test)]
mod undo_perf_tests;
