//! Event processing. See `docs/spec/triggers.md` for the rules implemented here.

use std::collections::{BTreeMap, VecDeque};

use coursecraft_course::{
    Action, BACKGROUND_AUDIO_ID, CmpOp, CompletionStatus, Condition, Cue, Event, Layer, Link,
    MAX_CONDITION_DEPTH, ObjectKind, QuestionKind, RevisitMode, Scoring, SeekMode, Slide,
    SlideTarget, Trigger, Value, VarOp, clamp_panel_offset, is_valid_link_url,
};

use crate::{
    Answer, Clock, CueProgress, Effect, Input, LearnerResponse, Lightbox, MAX_EVENTS_PER_INPUT,
    MediaOp, MediaPlaybackState, Player, SeekTarget, sample_motion_path,
};

/// An event waiting to be matched against triggers, tagged with the slide
/// visit (epoch) it belongs to.
struct Fired {
    epoch: u64,
    event: Event,
    /// Hide this layer after the event's triggers finish, for
    /// `hide_when_timeline_ends`.
    hide_after: Option<String>,
    /// When this is a pausing cue's event, the video it paused. The layers open
    /// after its triggers are recorded so they keep control of playback.
    cue_pause: Option<String>,
}

type Queue = VecDeque<Fired>;

/// One timeline being advanced by a tick: the base slide or a visible layer.
#[derive(Clone, Copy)]
struct Timeline<'a> {
    duration: u32,
    layer: Option<&'a str>,
    objects: &'a [coursecraft_course::Object],
}

fn warn(out: &mut Vec<Effect>, message: impl Into<String>) {
    out.push(Effect::Warning {
        message: message.into(),
    });
}

/// Result of the shared check a click and the keyboard focus list both use.
enum ClickCheck<'a> {
    Reaches {
        obj: &'a coursecraft_course::Object,
        layer: Option<&'a str>,
    },
    Missing,
    NotLive,
    Disabled,
    Prevented,
}

/// Links carried by an object's text, in run order. Adjacent runs with the same
/// link are one target, so a link split by formatting is still one tab stop.
fn object_links(object: &coursecraft_course::Object) -> Vec<Link> {
    let runs: &[coursecraft_course::TextRun] = match &object.kind {
        ObjectKind::Text { runs, .. } => runs,
        ObjectKind::Shape {
            text: Some(text), ..
        } => &text.runs,
        _ => return Vec::new(),
    };
    let mut out: Vec<Link> = Vec::new();
    for run in runs {
        let Some(link) = &run.link else {
            continue;
        };
        if run.text.is_empty() || out.last() == Some(link) {
            continue;
        }
        out.push(link.clone());
    }
    out
}

/// Does any trigger on the slide listen for a pointer event from `object`?
fn has_pointer_trigger(slide: &Slide, object: &str) -> bool {
    let matches = |event: &Event| {
        matches!(
            event,
            Event::Click { object: o }
                | Event::HoverStart { object: o }
                | Event::HoverEnd { object: o }
                if o == object
        )
    };
    slide
        .triggers
        .iter()
        .any(|t| !t.disabled && matches(&t.when))
        || slide
            .layers
            .iter()
            .flat_map(|l| l.triggers.iter())
            .any(|t| !t.disabled && matches(&t.when))
}

/// Does a trigger's `when` match a fired event?
fn matches(pattern: &Event, fired: &Event) -> bool {
    match (pattern, fired) {
        (
            Event::StateChanges {
                object: a,
                to: None,
            },
            Event::StateChanges { object: b, .. },
        ) => a == b,
        (
            Event::KeyPressed {
                key: pat_key,
                object: pat_obj,
            },
            Event::KeyPressed {
                key: fired_key,
                object: fired_obj,
            },
        ) => {
            if !pat_key.eq_ignore_ascii_case(fired_key) {
                return false;
            }
            match pat_obj {
                Some(po) => fired_obj.as_ref() == Some(po),
                None => true,
            }
        }
        (
            Event::DroppedOn {
                object: pat_obj,
                target: pat_tgt,
            },
            Event::DroppedOn {
                object: fired_obj,
                target: fired_tgt,
            },
        ) => pat_obj == fired_obj && pat_tgt == fired_tgt,
        (
            Event::ObjectsIntersect { a: p_a, b: p_b },
            Event::ObjectsIntersect { a: f_a, b: f_b },
        ) => (p_a == f_a && p_b == f_b) || (p_a == f_b && p_b == f_a),
        (
            Event::ObjectsStopIntersecting { a: p_a, b: p_b },
            Event::ObjectsStopIntersecting { a: f_a, b: f_b },
        ) => (p_a == f_a && p_b == f_b) || (p_a == f_b && p_b == f_a),
        _ => pattern == fired,
    }
}

/// The trim window of an audio or video object, if it has one.
fn media_trim(obj: &coursecraft_course::Object) -> Option<coursecraft_course::Trim> {
    match &obj.kind {
        ObjectKind::Audio { trim, .. } | ObjectKind::Video { trim, .. } => *trim,
        _ => None,
    }
}

impl<'c> Player<'c> {
    pub(crate) fn handle_into(&mut self, input: &Input, out: &mut Vec<Effect>) {
        if self.st.exited {
            warn(out, "the course has exited; input ignored");
            return;
        }
        if !self.st.started && *input != Input::Start {
            warn(out, "send `start` first");
            return;
        }
        let mut q = Queue::new();
        match input {
            Input::Start => {
                if self.st.started {
                    warn(out, "already started");
                    return;
                }
                self.st.started = true;
                self.enter_slide(self.st.slide, &mut q, out);
                self.start_background(out);
            }
            Input::Tick { ms } => {
                let scaled = self.scale_tick(*ms);
                self.tick(scaled, &mut q, out);
            }
            Input::OpenResource { resource } => {
                self.open_resource(resource, out);
                return;
            }
            Input::SetSpeed { rate } => {
                self.set_speed(*rate);
                return;
            }
            Input::Click { object } => {
                self.click(object, out);
                return;
            }
            Input::LinkClicked { object, link } => {
                match self.click_check(object) {
                    ClickCheck::Reaches { .. } => {}
                    ClickCheck::Missing => {
                        warn(out, format!("object {object:?} is not on this slide"));
                        return;
                    }
                    ClickCheck::NotLive => {
                        warn(out, format!("object {object:?} is not on screen"));
                        return;
                    }
                    ClickCheck::Disabled | ClickCheck::Prevented => return,
                }
                match link {
                    Link::Url { url } => {
                        if is_valid_link_url(url) {
                            out.push(Effect::OpenUrl { url: url.clone() });
                        } else {
                            warn(out, format!("link {url:?} is not a safe external URL"));
                        }
                        return;
                    }
                    Link::Slide { slide } => {
                        if self.course.play_slide(slide).is_none() {
                            warn(out, format!("link target slide {slide:?} does not exist"));
                            return;
                        }
                        self.jump(&SlideTarget::Id(slide.clone()), &mut q, out);
                    }
                }
            }
            Input::HoverStart { object } | Input::HoverEnd { object } => {
                if !self.object_is_live(object) {
                    warn(out, format!("object {object:?} is not on screen"));
                    return;
                }
                let event = match input {
                    Input::HoverStart { .. } => Event::HoverStart {
                        object: object.clone(),
                    },
                    _ => Event::HoverEnd {
                        object: object.clone(),
                    },
                };
                self.push(&mut q, event);
            }
            Input::MediaEnded { object } if object == BACKGROUND_AUDIO_ID => {
                self.background_media_ended(&mut q, out);
            }
            Input::MediaEnded { object } => {
                if self.st.media_playback.get(object) != Some(&MediaPlaybackState::Playing) {
                    warn(out, format!("media {object:?} is not playing"));
                    return;
                }
                self.finish_media(object, &mut q);
            }
            Input::MediaTime { object, at_ms } => {
                if !self.media_time(object, *at_ms, &mut q, out) {
                    return;
                }
            }
            Input::Key { key } => {
                if self.st.lightbox.is_some()
                    && (key.eq_ignore_ascii_case("escape") || key.eq_ignore_ascii_case("esc"))
                {
                    self.close_lightbox(true, out);
                    return;
                }
                if !self.key_is_for_control(key) {
                    self.push_key(&mut q, key);
                }
            }
            Input::KeyPressed { key } => {
                if self.st.lightbox.is_some()
                    && (key.eq_ignore_ascii_case("escape") || key.eq_ignore_ascii_case("esc"))
                {
                    self.close_lightbox(true, out);
                    return;
                }
                if !self.key_is_for_control(key) {
                    self.push_key(&mut q, key);
                }
            }
            Input::FocusChanged { object } => {
                self.st.focused_object = object.clone().filter(|id| {
                    self.current_slide()
                        .is_some_and(|slide| slide.object(id).is_some())
                });
                return;
            }
            Input::SelectChoice { choice } => self.select_choice(choice, out),
            Input::Submit => self.push(&mut q, Event::Submit),
            Input::Next | Input::Previous => {
                if self.st.lightbox.is_some() {
                    warn(
                        out,
                        "player navigation is disabled while a lightbox is open",
                    );
                    return;
                }
                if self.st.reviewing_from.is_some() {
                    let forward = *input == Input::Next;
                    self.review_nav(forward, &mut q, out);
                    self.process(&mut q, out);
                    return;
                }
                if !self.effective_player().show_nav {
                    warn(out, "player navigation is turned off for this course");
                    return;
                }
                if *input == Input::Next && !self.can_next() {
                    warn(out, "Next is locked until this slide is complete");
                    return;
                }
                let target = if *input == Input::Next {
                    SlideTarget::Next
                } else {
                    SlideTarget::Previous
                };
                self.jump(&target, &mut q, out);
            }
            Input::GoTo { slide } => {
                if !self.effective_player().show_menu {
                    warn(out, "the slide menu is turned off for this course");
                    return;
                }
                if self.st.lightbox.is_none() && !self.can_go_to(slide) {
                    warn(out, "that slide is locked in the slide menu");
                    return;
                }
                self.jump(&SlideTarget::Id(slide.clone()), &mut q, out);
            }
            Input::TextEntered { text } => {
                self.text_entered(text, out);
                return;
            }
            Input::SequenceMoved { choice, to } => {
                self.sequence_moved(choice, *to, out);
                return;
            }
            Input::MatchSelected { left, right } => {
                self.match_selected(left, right.as_deref(), out);
                return;
            }
            Input::HotspotClicked { x, y } => {
                self.hotspot_clicked(*x, *y, out);
                return;
            }
            Input::SetVariable { variable, value } => {
                self.set_variable(variable, VarOp::Assign, Some(value), &mut q, out);
            }
            Input::SetInputValue { object, value } => {
                self.set_input_value(object, value, &mut q, out);
            }
            Input::MarkerToggled { object } => {
                self.marker_toggled(object, out);
                return;
            }
            Input::MarkersClosed => {
                self.markers_closed(out);
                return;
            }
            Input::CloseLightbox => {
                self.close_lightbox(true, out);
                return;
            }
            Input::PanelScrolled { object, offset } => {
                self.scroll_panel(object, *offset, out);
                return;
            }
            Input::Jump { slide } => {
                self.jump(&SlideTarget::Id(slide.clone()), &mut q, out);
            }
            Input::DragDropped { item, target, x, y } => {
                self.drag_dropped(item, target.as_deref(), *x, *y, out);
                return;
            }
            Input::ObjectDropped { object, x, y } => {
                self.object_dropped(object, *x, *y, &mut q, out);
            }
            Input::PlayerPause => {
                self.set_player_paused(true, out);
                return;
            }
            Input::PlayerResume => {
                self.set_player_paused(false, out);
                return;
            }
            Input::PlayerSeek { ms } => self.seek(*ms, &mut q, out),
            Input::ExitReview => self.exit_review(&mut q, out),
        }
        self.process(&mut q, out);
    }

    /// Open a resource: an external link, or a packaged file under `media/`.
    fn open_resource(&mut self, id: &str, out: &mut Vec<Effect>) {
        let Some(resource) = self.course.resources.iter().find(|r| r.id == id) else {
            warn(out, format!("resource {id:?} does not exist"));
            return;
        };
        match &resource.target {
            coursecraft_course::ResourceTarget::Url { url } => {
                if is_valid_link_url(url) {
                    out.push(Effect::OpenUrl { url: url.clone() });
                } else {
                    warn(
                        out,
                        format!("resource link {url:?} is not a safe external URL"),
                    );
                }
            }
            coursecraft_course::ResourceTarget::File { src } => {
                if coursecraft_course::is_safe_media_path(src) {
                    out.push(Effect::OpenUrl {
                        url: format!("media/{src}"),
                    });
                } else {
                    warn(out, format!("resource file {src:?} is not a safe path"));
                }
            }
        }
    }

    fn push(&self, q: &mut Queue, event: Event) {
        q.push_back(Fired {
            epoch: self.st.epoch,
            event,
            hide_after: None,
            cue_pause: None,
        });
    }

    /// Queue a pausing cue's event, remembering the video it paused.
    fn push_cue_pause(&self, q: &mut Queue, event: Event, object: &str) {
        q.push_back(Fired {
            epoch: self.st.epoch,
            event,
            hide_after: None,
            cue_pause: Some(object.to_owned()),
        });
    }

    /// Queue a key press. An object that currently has focus is attached so
    /// object-scoped triggers can match it; global triggers still fire.
    fn push_key(&self, q: &mut Queue, key: &str) {
        let object = self.st.focused_object.as_ref().and_then(|id| {
            self.current_slide()
                .and_then(|slide| slide.object(id))
                .map(|_| id.clone())
        });
        self.push(
            q,
            Event::KeyPressed {
                key: key.to_owned(),
                object,
            },
        );
    }

    /// Whether the focused control already owns `key`, in which case the
    /// key press must not also fire a trigger.
    fn key_is_for_control(&self, key: &str) -> bool {
        let Some(id) = self.st.focused_object.as_deref() else {
            return false;
        };
        let Some((object, _)) = self.current_slide().and_then(|slide| slide.object(id)) else {
            return false;
        };
        let lower = key.to_ascii_lowercase();
        let arrow = matches!(
            lower.as_str(),
            "arrow_up" | "arrow_down" | "arrow_left" | "arrow_right"
        );
        let activate = matches!(lower.as_str(), "enter" | "space");
        match &object.kind {
            ObjectKind::TextEntry { .. } => true,
            ObjectKind::Slider { .. } | ObjectKind::Dial { .. } => arrow,
            ObjectKind::Checkbox { .. } => activate,
            ObjectKind::RadioGroup { .. } => arrow,
            ObjectKind::Marker { .. } => activate,
            ObjectKind::ScrollPanel { .. } => arrow || lower == "space",
            ObjectKind::Button { .. } => activate,
            _ => object.drag.is_some() && activate,
        }
    }

    fn push_with_hide(&self, q: &mut Queue, event: Event, hide_after: Option<String>) {
        q.push_back(Fired {
            epoch: self.st.epoch,
            event,
            hide_after,
            cue_pause: None,
        });
    }

    fn slide(&self) -> Option<&'c Slide> {
        self.slides.get(self.st.slide).copied()
    }

    fn layer(&self, id: &str) -> Option<&'c Layer> {
        self.slide().and_then(|s| s.layer(id))
    }

    // ------------------------------------------------------------ processing

    fn process(&mut self, q: &mut Queue, out: &mut Vec<Effect>) {
        let mut budget = MAX_EVENTS_PER_INPUT;
        while let Some(fired) = q.pop_front() {
            if fired.epoch != self.st.epoch {
                continue; // belongs to a slide we already left
            }
            if budget == 0 {
                warn(
                    out,
                    format!(
                        "more than {MAX_EVENTS_PER_INPUT} events from one input; stopping (trigger loop?)"
                    ),
                );
                q.clear();
                return;
            }
            budget -= 1;
            let layers_before = fired
                .cue_pause
                .as_ref()
                .map(|_| self.st.visible_layers.clone());
            self.fire(&fired, q, out);
            if let Some(object) = &fired.cue_pause
                && self.st.epoch == fired.epoch
                && !self.st.exited
            {
                let before = layers_before.unwrap_or_default();
                let layers: Vec<String> = self
                    .st
                    .visible_layers
                    .iter()
                    .filter(|layer| !before.contains(layer))
                    .cloned()
                    .collect();
                self.st.cue_layers.insert(object.clone(), layers);
            }
            if let Some(layer) = fired.hide_after
                && self.st.epoch == fired.epoch
                && !self.st.exited
            {
                self.hide_layer(&layer, out);
            }
        }
    }

    /// Run every matching trigger for one event. The trigger list is the base
    /// layer then visible layers bottom to top, snapshotted before any action runs.
    fn fire(&mut self, fired: &Fired, q: &mut Queue, out: &mut Vec<Effect>) {
        let Some(slide) = self.slide() else { return };
        let mut owners: Vec<(Option<&'c str>, &'c [Trigger])> =
            vec![(None, slide.triggers.as_slice())];
        for id in &self.st.visible_layers {
            if let Some(l) = slide.layer(id) {
                owners.push((Some(l.id.as_str()), l.triggers.as_slice()));
            }
        }
        for (owner, triggers) in owners {
            for t in triggers {
                if self.st.epoch != fired.epoch {
                    return; // a jump left the slide: the rest of the list is dropped
                }
                if t.disabled || !matches(&t.when, &fired.event) {
                    continue;
                }
                if let Some(c) = &t.condition
                    && !self.eval(c, 0)
                {
                    continue;
                }
                self.act(&t.action, &fired.event, owner, q, out);
            }
        }
    }

    fn eval(&self, c: &Condition, depth: usize) -> bool {
        if depth > MAX_CONDITION_DEPTH {
            return false;
        }
        match c {
            Condition::All(list) => list.iter().all(|c| self.eval(c, depth + 1)),
            Condition::Any(list) => list.iter().any(|c| self.eval(c, depth + 1)),
            Condition::Not(inner) => !self.eval(inner, depth + 1),
            Condition::Compare {
                variable,
                op,
                value,
            } => self
                .value(variable)
                .as_ref()
                .is_some_and(|cur| compare(cur, *op, value)),
            Condition::ObjectState { object, state, is } => {
                (self.object_state(object) == Some(state.as_str())) == *is
            }
        }
    }

    // ------------------------------------------------------------ actions

    fn act(
        &mut self,
        a: &Action,
        event: &Event,
        owner: Option<&'c str>,
        q: &mut Queue,
        out: &mut Vec<Effect>,
    ) {
        match a {
            Action::JumpToSlide { target } => self.jump(target, q, out),
            Action::ShowLayer { layer } => self.show_layer(layer, q, out),
            Action::HideLayer { layer } => match layer.as_deref().or(owner) {
                Some(l) => self.hide_layer(l, out),
                None => warn(out, "hide_layer on the base layer has nothing to hide"),
            },
            Action::ChangeState { object, state } => self.set_state(object, state, q, out),
            Action::SetVariable {
                variable,
                op,
                value,
            } => self.set_variable(variable, *op, value.as_ref(), q, out),
            Action::PlayMedia { object } => self.play_media(object, out),
            Action::PauseMedia { object } => self.pause_media(object, out),
            Action::StopMedia { object } => self.stop_media(object, out),
            Action::MoveOnPath { object, path } => self.move_on_path(object, path, out),
            Action::PauseTimeline | Action::ResumeTimeline => {
                let paused = matches!(a, Action::PauseTimeline);
                self.pause(owner, paused, out);
            }
            Action::SubmitQuestion => self.grade(q, out),
            Action::SubmitResults => self.results(out),
            Action::RetryQuiz => self.retry_quiz(q, out),
            Action::ReviewQuiz => self.review_quiz(q, out),
            Action::ExitReview => self.exit_review(q, out),
            Action::PrintSlide => {
                let Some(slide) = self.slide() else {
                    return;
                };
                let exclude = match event {
                    Event::Click { object } => vec![object.clone()],
                    _ => Vec::new(),
                };
                out.push(Effect::Print {
                    slide: slide.id.clone(),
                    exclude,
                });
            }
            Action::CompleteCourse { status } => out.push(Effect::Completion { status: *status }),
            Action::ExitCourse => {
                if self.st.background.playing || self.st.background.paused_by_author {
                    self.st.background.playing = false;
                    self.st.background.paused_by_author = false;
                    self.st.background.started_pending = false;
                    out.push(Effect::Media {
                        object: BACKGROUND_AUDIO_ID.to_owned(),
                        op: MediaOp::Stop,
                    });
                }
                self.st.exited = true;
                q.clear();
                out.push(Effect::Exit);
            }
            Action::OpenUrl { url } => out.push(Effect::OpenUrl { url: url.clone() }),
            Action::RunJavaScript { code } => {
                out.push(Effect::RunJavaScript { code: code.clone() })
            }
            Action::LightboxSlide { slide } => self.open_lightbox(slide, q, out),
            Action::CloseLightbox => self.close_lightbox(true, out),
            Action::StartTimer { timer } => self.start_timer(timer, out),
            Action::PauseTimer { timer } => self.pause_timer(timer, out),
            Action::ResetTimer { timer } => self.reset_timer(timer, q, out),
        }
    }

    /// The object a timer action names, warning when it is not a timer.
    fn timer_target(&self, timer: &str, out: &mut Vec<Effect>) -> Option<u32> {
        let slide = self.slide()?;
        let Some((obj, _)) = slide.object(timer) else {
            warn(out, format!("object {timer:?} is not on this slide"));
            return None;
        };
        let ObjectKind::Timer { duration_ms, .. } = obj.kind else {
            warn(out, format!("object {timer:?} is not a timer"));
            return None;
        };
        Some(duration_ms)
    }

    /// Start (or resume) a timer. An ended timer does nothing until reset.
    fn start_timer(&mut self, timer: &str, out: &mut Vec<Effect>) {
        if self.timer_target(timer, out).is_none() {
            return;
        }
        let state = self.st.timers.entry(timer.to_owned()).or_default();
        if state.ended || state.running {
            return;
        }
        state.running = true;
        state.autostart_armed = false;
        out.push(Effect::TimerStarted {
            timer: timer.to_owned(),
        });
    }

    /// Pause a timer, keeping its remaining time.
    fn pause_timer(&mut self, timer: &str, out: &mut Vec<Effect>) {
        if self.timer_target(timer, out).is_none() {
            return;
        }
        let state = self.st.timers.entry(timer.to_owned()).or_default();
        if !state.running {
            return;
        }
        state.running = false;
        out.push(Effect::TimerPaused {
            timer: timer.to_owned(),
        });
    }

    /// Return a timer to its full duration, stopped. Autostart does not
    /// restart it; only `start_timer` does.
    fn reset_timer(&mut self, timer: &str, q: &mut Queue, out: &mut Vec<Effect>) {
        let Some(duration_ms) = self.timer_target(timer, out) else {
            return;
        };
        let before_seconds = self.timer_remaining_seconds(timer).unwrap_or(0);
        self.st.timers.insert(
            timer.to_owned(),
            crate::TimerState {
                remaining_ms: duration_ms,
                running: false,
                ended: false,
                autostart_armed: false,
            },
        );
        out.push(Effect::TimerReset {
            timer: timer.to_owned(),
        });
        if self.timer_remaining_seconds(timer).unwrap_or(0) != before_seconds {
            self.push(
                q,
                Event::VariableChanges {
                    variable: coursecraft_course::timer_remaining_variable(timer),
                },
            );
        }
    }

    fn jump(&mut self, target: &SlideTarget, q: &mut Queue, out: &mut Vec<Effect>) {
        if self.st.lightbox.is_some() {
            // A jump from inside the box closes it and moves the base slide.
            self.close_lightbox(true, out);
        }
        let cur = self.st.slide;
        let idx = match target {
            SlideTarget::Next => cur.checked_add(1).filter(|i| *i < self.slides.len()),
            SlideTarget::Previous => cur.checked_sub(1),
            SlideTarget::Id(id) => {
                let found = self.slide_position(id);
                if found.is_none() {
                    warn(out, format!("slide {id:?} does not exist"));
                }
                found
            }
        };
        // Next on the last slide / Previous on the first: nothing happens.
        if let Some(idx) = idx {
            self.enter_slide(idx, q, out);
        }
    }

    fn check_media_target<'a>(
        &'a self,
        object: &str,
        out: &mut Vec<Effect>,
    ) -> Option<&'a coursecraft_course::Object> {
        let slide = self.slide()?;
        let (obj, _) = match slide.object(object) {
            Some(pair) => pair,
            None => {
                warn(out, format!("media object {object:?} does not exist"));
                return None;
            }
        };
        if !matches!(
            obj.kind,
            ObjectKind::Audio { .. } | ObjectKind::Video { .. } | ObjectKind::Lottie { .. }
        ) {
            warn(out, format!("object {object:?} is not a media object"));
            return None;
        }
        Some(obj)
    }

    fn video_cues(&self, object: &str) -> Option<Vec<Cue>> {
        let slide = self.slide()?;
        let (obj, _) = slide.object(object)?;
        match &obj.kind {
            ObjectKind::Video { cues, .. } => Some(cues.clone()),
            _ => None,
        }
    }

    /// Mark `object` stopped and queue `MediaEnds`, the way a natural end does.
    fn finish_media(&mut self, object: &str, q: &mut Queue) {
        self.st.media_playback.remove(object);
        self.st.cue_progress.remove(object);
        self.st.cue_held.remove(object);
        self.st.cue_layers.remove(object);
        self.push(
            q,
            Event::MediaEnds {
                object: object.to_owned(),
            },
        );
    }

    /// Process a host-reported media position. Returns whether events were
    /// queued (and so the caller should run the event queue).
    ///
    /// Audio and video positions are in file time, so cue times and `trim`
    /// bounds compare directly.
    fn media_time(
        &mut self,
        object: &str,
        at_ms: f64,
        q: &mut Queue,
        out: &mut Vec<Effect>,
    ) -> bool {
        let (cues, trim_end) = {
            let Some(slide) = self.slide() else {
                return false;
            };
            let Some((obj, _)) = slide.object(object) else {
                warn(out, format!("video object {object:?} is not on this slide"));
                return false;
            };
            match &obj.kind {
                ObjectKind::Video { cues, trim, .. } => {
                    (cues.clone(), trim.as_ref().and_then(|trim| trim.end_ms))
                }
                ObjectKind::Audio { trim, .. } => {
                    (Vec::new(), trim.as_ref().and_then(|trim| trim.end_ms))
                }
                _ => {
                    warn(out, format!("object {object:?} is not audio or video"));
                    return false;
                }
            }
        };
        if !at_ms.is_finite() || at_ms < 0.0 {
            warn(
                out,
                format!("media time for {object:?} must be finite and at least 0"),
            );
            return false;
        }
        if self.st.media_playback.get(object) != Some(&MediaPlaybackState::Playing) {
            return false;
        }
        // A trim end ends the clip exactly like a natural end.
        if trim_end.is_some_and(|end_ms| at_ms >= end_ms as f64) {
            out.push(Effect::Media {
                object: object.to_owned(),
                op: MediaOp::Stop,
            });
            self.finish_media(object, q);
            return true;
        }
        if cues.is_empty() {
            return false;
        }

        let mut progress = self.st.cue_progress.remove(object).unwrap_or_default();
        let old = if progress.last_ms.is_finite() {
            progress.last_ms
        } else {
            0.0
        };
        if at_ms < old {
            if at_ms + 1.0 >= old {
                // Host position rounding around a just-seeked cue must not
                // re-arm the cue that is being resumed.
                self.st.cue_progress.insert(object.to_owned(), progress);
                return false;
            }
            // A backward seek re-arms the cues at or after the new position.
            progress
                .fired
                .retain(|id| cues.iter().any(|cue| cue.id == *id && cue.at_ms < at_ms));
            progress.last_ms = at_ms;
            self.st.cue_progress.insert(object.to_owned(), progress);
            self.st.cue_held.remove(object);
            self.st.cue_layers.remove(object);
            return false;
        }
        if at_ms == old {
            self.st.cue_progress.insert(object.to_owned(), progress);
            return false;
        }

        let mut queued = false;
        for cue in &cues {
            if progress.fired.contains(&cue.id) || cue.at_ms < old || cue.at_ms > at_ms {
                continue;
            }
            progress.fired.insert(cue.id.clone());
            progress.last_ms = cue.at_ms;
            out.push(Effect::CueReached {
                object: object.to_owned(),
                cue: cue.id.clone(),
            });
            let event = Event::CueReached {
                object: object.to_owned(),
                cue: cue.id.clone(),
            };
            if cue.pause {
                // Stop on the cue and let its triggers run from the held state.
                self.st
                    .media_playback
                    .insert(object.to_owned(), MediaPlaybackState::PausedAtCue);
                self.st.cue_held.insert(object.to_owned(), cue.id.clone());
                out.push(Effect::MediaSeek {
                    object: object.to_owned(),
                    ms: cue.at_ms_u64(),
                });
                out.push(Effect::Media {
                    object: object.to_owned(),
                    op: MediaOp::Pause,
                });
                self.push_cue_pause(q, event, object);
                self.st.cue_progress.insert(object.to_owned(), progress);
                return true;
            }
            self.push(q, event);
            queued = true;
        }
        progress.last_ms = at_ms;
        self.st.cue_progress.insert(object.to_owned(), progress);
        queued
    }

    /// Give every video on `objects` fresh cue progress, re-arming all cues.
    fn seed_cues<'a>(&mut self, objects: impl Iterator<Item = &'a coursecraft_course::Object>) {
        for object in objects {
            if let ObjectKind::Video { cues, .. } = &object.kind
                && !cues.is_empty()
            {
                self.st
                    .cue_progress
                    .insert(object.id.clone(), CueProgress::default());
            }
        }
    }

    fn clear_cue_state(&mut self, object: &str) {
        self.st.cue_progress.remove(object);
        self.st.cue_held.remove(object);
        self.st.cue_layers.remove(object);
    }

    /// Move a video's cue progress after the timeline clock moved. A forward
    /// seek marks the cues behind it as fired without firing them; a backward
    /// seek re-arms the cues at or after the new position.
    fn seek_cue_position(&mut self, object: &str, at_ms: f64) {
        let Some(cues) = self.video_cues(object) else {
            return;
        };
        let mut progress = self.st.cue_progress.remove(object).unwrap_or_default();
        let old = if progress.last_ms.is_finite() {
            progress.last_ms
        } else {
            0.0
        };
        if at_ms < old {
            progress
                .fired
                .retain(|id| cues.iter().any(|cue| cue.id == *id && cue.at_ms < at_ms));
        } else {
            for cue in &cues {
                if cue.at_ms <= at_ms {
                    progress.fired.insert(cue.id.clone());
                }
            }
        }
        progress.last_ms = at_ms;
        self.st.cue_progress.insert(object.to_owned(), progress);
        self.st.cue_held.remove(object);
        self.st.cue_layers.remove(object);
    }

    /// Start the course's background audio, if it has one.
    fn start_background(&mut self, out: &mut Vec<Effect>) {
        if self.course.background_audio.is_none() {
            return;
        }
        self.st.background.playing = true;
        self.st.background.paused_by_author = false;
        self.st.background.started_pending = true;
        out.push(Effect::Media {
            object: BACKGROUND_AUDIO_ID.to_owned(),
            op: MediaOp::Play,
        });
    }

    /// A natural end reported for the course's background audio.
    fn background_media_ended(&mut self, q: &mut Queue, out: &mut Vec<Effect>) {
        if !self.st.background.playing {
            warn(out, format!("media {BACKGROUND_AUDIO_ID:?} is not playing"));
            return;
        }
        // Run the author's `media_ends` triggers before deciding whether to loop.
        self.push(
            q,
            Event::MediaEnds {
                object: BACKGROUND_AUDIO_ID.to_owned(),
            },
        );
        self.process(q, out);
        if !self.st.background.playing {
            return;
        }
        let looping = self
            .course
            .background_audio
            .as_ref()
            .is_some_and(|audio| audio.looping);
        if !looping {
            self.st.background.playing = false;
        } else if self.st.background.started_pending {
            // The host reported the end before its first tick: the browser
            // refused the initial play, so do not loop a track that never began.
            self.st.background.playing = false;
            warn(out, "background audio could not start");
        } else {
            out.push(Effect::Media {
                object: BACKGROUND_AUDIO_ID.to_owned(),
                op: MediaOp::Play,
            });
        }
    }

    fn play_media(&mut self, object: &str, out: &mut Vec<Effect>) {
        if object == BACKGROUND_AUDIO_ID {
            if self.course.background_audio.is_none() {
                warn(out, "the course has no background audio");
                return;
            }
            self.st.background.playing = true;
            self.st.background.paused_by_author = false;
            out.push(Effect::Media {
                object: BACKGROUND_AUDIO_ID.to_owned(),
                op: MediaOp::Play,
            });
            return;
        }
        if self.check_media_target(object, out).is_none() {
            return;
        }
        // A video held at a pausing cue resumes from that cue, and the cue does
        // not fire again.
        let held_cue = self.st.cue_held.remove(object);
        if let Some(cue_id) = &held_cue
            && let Some(cue) = self
                .video_cues(object)
                .and_then(|cues| cues.into_iter().find(|cue| cue.id == *cue_id))
        {
            let mut progress = self.st.cue_progress.remove(object).unwrap_or_default();
            progress.last_ms = cue.at_ms;
            progress.fired.insert(cue.id.clone());
            self.st.cue_progress.insert(object.to_owned(), progress);
            out.push(Effect::MediaSeek {
                object: object.to_owned(),
                ms: cue.at_ms_u64(),
            });
        }
        self.st.cue_layers.remove(object);
        // A fresh play after a stop or end re-arms the cues from the start.
        if held_cue.is_none()
            && !self.st.media_playback.contains_key(object)
            && self.video_cues(object).is_some()
        {
            self.st.cue_progress.entry(object.to_owned()).or_default();
        }
        // A play always starts from the beginning. Only Lottie keeps a clock
        // here; audio and video positions live in their backend.
        if held_cue.is_none() && self.object_is_lottie(object) {
            self.st.media_clocks.insert(object.to_owned(), 0);
        }
        // Media started while the learner has paused waits for the resume.
        if self.st.player_paused {
            self.st
                .media_playback
                .insert(object.to_owned(), MediaPlaybackState::PausedByPlayer);
            return;
        }
        self.st
            .media_playback
            .insert(object.to_owned(), MediaPlaybackState::Playing);
        out.push(Effect::Media {
            object: object.to_owned(),
            op: MediaOp::Play,
        });
    }

    fn object_is_lottie(&self, object: &str) -> bool {
        self.slide()
            .and_then(|slide| slide.object(object))
            .is_some_and(|(obj, _)| matches!(obj.kind, ObjectKind::Lottie { .. }))
    }

    fn set_player_paused(&mut self, paused: bool, out: &mut Vec<Effect>) {
        let changed = self.st.player_paused != paused;
        if paused && !changed {
            return;
        }
        self.st.player_paused = paused;
        let ids: Vec<String> = self.st.media_playback.keys().cloned().collect();
        for id in ids {
            let state = self.st.media_playback.get(&id).copied();
            if paused {
                if state == Some(MediaPlaybackState::Playing) {
                    self.st
                        .media_playback
                        .insert(id.clone(), MediaPlaybackState::PausedByPlayer);
                    out.push(Effect::Media {
                        object: id,
                        op: MediaOp::Pause,
                    });
                }
            } else if state == Some(MediaPlaybackState::PausedByPlayer) {
                let on_base = self
                    .slide()
                    .and_then(|slide| slide.object(&id))
                    .is_some_and(|(_, layer)| layer.is_none());
                if on_base && self.base_timeline_is_paused() {
                    // The author's hold wins over the learner's resume.
                    self.st
                        .media_playback
                        .insert(id, MediaPlaybackState::PausedByTimeline);
                } else {
                    self.st
                        .media_playback
                        .insert(id.clone(), MediaPlaybackState::Playing);
                    out.push(Effect::Media {
                        object: id,
                        op: MediaOp::Play,
                    });
                }
            } else if state == Some(MediaPlaybackState::PausedAtCue) {
                // A layer the cue opened keeps control until it closes.
                let held_by_layer = self.st.cue_layers.get(&id).is_some_and(|layers| {
                    layers
                        .iter()
                        .any(|layer| self.st.visible_layers.iter().any(|v| v == layer))
                });
                let on_base = self
                    .slide()
                    .and_then(|slide| slide.object(&id))
                    .is_some_and(|(_, layer)| layer.is_none());
                if held_by_layer {
                    continue;
                }
                self.st.cue_held.remove(&id);
                self.st.cue_layers.remove(&id);
                if on_base && self.base_timeline_is_paused() {
                    self.st
                        .media_playback
                        .insert(id, MediaPlaybackState::PausedByTimeline);
                } else {
                    self.st
                        .media_playback
                        .insert(id.clone(), MediaPlaybackState::Playing);
                    out.push(Effect::Media {
                        object: id,
                        op: MediaOp::Play,
                    });
                }
            }
        }
    }

    fn pause_media(&mut self, object: &str, out: &mut Vec<Effect>) {
        if object == BACKGROUND_AUDIO_ID {
            if self.course.background_audio.is_none() {
                warn(out, "the course has no background audio");
                return;
            }
            self.st.background.playing = false;
            self.st.background.paused_by_author = true;
            out.push(Effect::Media {
                object: BACKGROUND_AUDIO_ID.to_owned(),
                op: MediaOp::Pause,
            });
            return;
        }
        if self.check_media_target(object, out).is_none() {
            return;
        }
        self.st.cue_held.remove(object);
        self.st.cue_layers.remove(object);
        self.st
            .media_playback
            .insert(object.to_owned(), MediaPlaybackState::Paused);
        out.push(Effect::Media {
            object: object.to_owned(),
            op: MediaOp::Pause,
        });
    }

    fn stop_media(&mut self, object: &str, out: &mut Vec<Effect>) {
        if object == BACKGROUND_AUDIO_ID {
            if self.course.background_audio.is_none() {
                warn(out, "the course has no background audio");
                return;
            }
            self.st.background.playing = false;
            self.st.background.paused_by_author = false;
            out.push(Effect::Media {
                object: BACKGROUND_AUDIO_ID.to_owned(),
                op: MediaOp::Stop,
            });
            return;
        }
        if self.check_media_target(object, out).is_none() {
            return;
        }
        self.st.media_playback.remove(object);
        self.st.media_clocks.remove(object);
        self.clear_cue_state(object);
        out.push(Effect::Media {
            object: object.to_owned(),
            op: MediaOp::Stop,
        });
    }

    fn stop_layer_media(&mut self, layer_id: &str, out: &mut Vec<Effect>) {
        let Some(slide) = self.slide() else { return };
        let Some(layer) = slide.layer(layer_id) else {
            return;
        };
        for obj in &layer.objects {
            if self.st.media_playback.remove(&obj.id).is_some() {
                out.push(Effect::Media {
                    object: obj.id.clone(),
                    op: MediaOp::Stop,
                });
            }
            self.clear_cue_state(&obj.id);
        }
    }

    fn move_on_path(&mut self, object_id: &str, path_id: &str, out: &mut Vec<Effect>) {
        let Some(slide) = self.slide() else { return };
        let Some((obj, _)) = slide.object(object_id) else {
            warn(out, format!("object {object_id:?} is not on this slide"));
            return;
        };
        let Some(path) = obj.motion_paths.iter().find(|mp| mp.id == path_id).cloned() else {
            warn(
                out,
                format!("motion path {path_id:?} not found on object {object_id:?}"),
            );
            return;
        };
        let current_offset = if let Some(running) = self.st.running_paths.get(object_id) {
            sample_motion_path(&running.path, running.elapsed_ms, running.from, obj.frame)
        } else {
            self.st
                .object_offsets
                .get(object_id)
                .copied()
                .unwrap_or((0.0, 0.0))
        };
        self.st.running_paths.insert(
            object_id.to_owned(),
            crate::RunningPath {
                path,
                elapsed_ms: 0,
                from: current_offset,
            },
        );
    }

    fn base_timeline_is_paused(&self) -> bool {
        self.st.base.paused || self.base_held_by_layer()
    }

    /// Does a visible layer hold the base clock with `pause_base_timeline`?
    pub(crate) fn base_held_by_layer(&self) -> bool {
        let Some(slide) = self.slide() else {
            return false;
        };
        self.st
            .visible_layers
            .iter()
            .any(|id| slide.layer(id).is_some_and(|l| l.pause_base_timeline))
    }

    fn update_base_media_pause_state(&mut self, was_paused: bool, out: &mut Vec<Effect>) {
        let is_paused = self.base_timeline_is_paused();
        if !was_paused && is_paused {
            let Some(slide) = self.slide() else { return };
            for obj in &slide.objects {
                if self.st.media_playback.get(&obj.id) == Some(&MediaPlaybackState::Playing) {
                    self.st
                        .media_playback
                        .insert(obj.id.clone(), MediaPlaybackState::PausedByTimeline);
                    out.push(Effect::Media {
                        object: obj.id.clone(),
                        op: MediaOp::Pause,
                    });
                }
            }
        } else if was_paused && !is_paused {
            let Some(slide) = self.slide() else { return };
            let player_paused = self.st.player_paused;
            for obj in &slide.objects {
                if self.st.media_playback.get(&obj.id)
                    == Some(&MediaPlaybackState::PausedByTimeline)
                {
                    if player_paused {
                        // The learner still has the pause: hand these over to it.
                        self.st
                            .media_playback
                            .insert(obj.id.clone(), MediaPlaybackState::PausedByPlayer);
                    } else {
                        self.st
                            .media_playback
                            .insert(obj.id.clone(), MediaPlaybackState::Playing);
                        out.push(Effect::Media {
                            object: obj.id.clone(),
                            op: MediaOp::Play,
                        });
                    }
                }
            }
        }
    }

    fn enter_slide(&mut self, idx: usize, q: &mut Queue, out: &mut Vec<Effect>) {
        let Some(slide) = self.slides.get(idx).copied() else {
            warn(out, "slide index out of range");
            return;
        };
        let already_visited = self.st.visited.contains(&slide.id);
        if self.entered {
            self.note_timeline_completed();
            self.save_current_slide();
        }
        self.entered = true;
        // Slide exit: every playing or paused media object gets Media { Stop },
        // emitted before slide_entered of the next slide.
        let to_stop: Vec<String> = self.st.media_playback.keys().cloned().collect();
        for obj in to_stop {
            out.push(Effect::Media {
                object: obj,
                op: MediaOp::Stop,
            });
        }
        self.st.media_playback.clear();
        self.st.media_clocks.clear();
        self.st.cue_progress.clear();
        self.st.cue_held.clear();
        self.st.cue_layers.clear();

        self.st.epoch = self.st.epoch.wrapping_add(1);
        self.st.slide = idx;
        // The learner's pause belongs to the slide they paused on.
        self.st.player_paused = false;
        self.st.base = Clock::default();
        self.st.visible_layers.clear();
        self.st.layer_clocks.clear();
        self.st.selected.clear();
        self.st.text_entry.clear();
        self.st.focused_object = None;
        self.st.sequence.clear();
        self.st.matches.clear();
        self.st.hotspot_markers.clear();
        self.st.drops.clear();
        self.st.marker_open = None;
        self.st.panel_offsets.clear();
        self.st.timers.clear();
        self.seed_timers(slide);
        self.seed_cues(
            slide
                .objects
                .iter()
                .chain(slide.layers.iter().flat_map(|layer| layer.objects.iter())),
        );
        self.st.visited.insert(slide.id.clone());
        self.st.running_paths.clear();
        self.st.object_offsets.clear();
        self.st.object_positions.clear();
        self.st.intersecting_pairs.clear();
        if slide.revisit == RevisitMode::Reset {
            // Reset also forgets any saved layer clocks for this slide.
            self.st.saved_slides.remove(&slide.id);
        }
        let states = self
            .st
            .object_states
            .entry(slide.id.clone())
            .or_insert_with(|| {
                slide
                    .objects
                    .iter()
                    .chain(slide.layers.iter().flat_map(|l| l.objects.iter()))
                    .map(|o| (o.id.clone(), o.initial_state.clone()))
                    .collect()
            });
        if slide.revisit == RevisitMode::Reset {
            for object in slide
                .objects
                .iter()
                .chain(slide.layers.iter().flat_map(|l| l.objects.iter()))
            {
                states.insert(object.id.clone(), object.initial_state.clone());
            }
        }

        // `resume` restores the base clock, visible layers and layer clocks
        // saved when the learner last left this slide. While reviewing, no
        // layers are restored, so no feedback layers show.
        let reviewing = self.st.reviewing_from.is_some();
        let mut restored_layers = Vec::new();
        if !reviewing
            && slide.revisit == RevisitMode::Resume
            && already_visited
            && let Some(saved) = self.st.saved_slides.get(&slide.id).cloned()
        {
            let mut visible: Vec<String> = saved
                .visible_layers
                .into_iter()
                .filter(|id| slide.layer(id).is_some())
                .collect();
            let authored: Vec<&str> = slide.layers.iter().map(|layer| layer.id.as_str()).collect();
            visible.sort_by_key(|id| {
                authored
                    .iter()
                    .position(|other| other == id)
                    .unwrap_or(usize::MAX)
            });
            for id in &visible {
                let clock = saved.layer_clocks.get(id).copied().unwrap_or_default();
                self.st.layer_clocks.insert(id.clone(), clock);
            }
            self.st.visible_layers = visible.clone();
            self.st.base = saved.base;
            restored_layers = visible;
        }

        out.push(Effect::SlideEntered {
            slide: slide.id.clone(),
        });
        if reviewing {
            // Restore the graded answer state and run no slide-entry
            // triggers or autoplay media.
            if let Some(snapshot) = self.st.review_answers.get(&slide.id).cloned() {
                let question = slide.question.clone();
                self.st.selected = snapshot.selected;
                self.st.text_entry = snapshot.text_entry;
                self.st.sequence = snapshot.sequence;
                self.st.matches = snapshot.matches;
                self.st.hotspot_markers = snapshot.hotspot_markers;
                self.st.drops = snapshot.drops;
                if let Some(q) = &question
                    && q.snap_to_center
                {
                    for (item, target) in self.st.drops.clone() {
                        if let (Some((obj, _)), Some((tobj, _))) =
                            (slide.object(&item), slide.object(&target))
                        {
                            let offset = (
                                tobj.frame.x + tobj.frame.w * 0.5 - obj.frame.w * 0.5 - obj.frame.x,
                                tobj.frame.y + tobj.frame.h * 0.5 - obj.frame.h * 0.5 - obj.frame.y,
                            );
                            if offset.0.is_finite() && offset.1.is_finite() {
                                self.st.object_offsets.insert(item, offset);
                            }
                        }
                    }
                }
            }
            self.check_intersections(q);
            return;
        }
        self.seed_sequence(slide);
        // Starting at a restored clock already past 0 must not replay the
        // start cue or autoplay cues that are behind the learner.
        if self.st.base.ms == 0 {
            self.autoplay(&slide.objects, 0, out);
            self.push(q, Event::TimelineStarts { layer: None });
        }
        for id in restored_layers {
            out.push(Effect::LayerShown { layer: id.clone() });
            let clock = self.st.layer_clocks.get(&id).copied().unwrap_or_default();
            if clock.ms == 0
                && let Some(layer) = slide.layer(&id)
            {
                self.autoplay(&layer.objects, 0, out);
                self.push(
                    q,
                    Event::TimelineStarts {
                        layer: Some(id.clone()),
                    },
                );
            }
        }
        self.check_intersections(q);
    }

    fn check_intersections(&mut self, q: &mut Queue) {
        let Some(slide) = self.slide() else { return };
        // Collect all object IDs on the current slide (base objects + visible layers)
        let mut object_ids = Vec::new();
        if !self.base_objects_hidden() {
            for obj in &slide.objects {
                object_ids.push(obj.id.clone());
            }
        }
        for lid in &self.st.visible_layers {
            if let Some(layer) = slide.layer(lid) {
                for obj in &layer.objects {
                    object_ids.push(obj.id.clone());
                }
            }
        }

        let mut current_intersections = std::collections::BTreeSet::new();
        let len = object_ids.len();
        for i in 0..len {
            for j in (i + 1)..len {
                let id_a = match object_ids.get(i) {
                    Some(id) => id,
                    None => continue,
                };
                let id_b = match object_ids.get(j) {
                    Some(id) => id,
                    None => continue,
                };
                let (pair_a, pair_b) = if id_a < id_b {
                    (id_a.clone(), id_b.clone())
                } else {
                    (id_b.clone(), id_a.clone())
                };

                let bounds_a = self.object_current_bounds(&pair_a);
                let bounds_b = self.object_current_bounds(&pair_b);
                if let (Some(a), Some(b)) = (bounds_a, bounds_b) {
                    let overlaps =
                        a.x < b.x + b.w && a.x + a.w > b.x && a.y < b.y + b.h && a.y + a.h > b.y;
                    if overlaps {
                        current_intersections.insert((pair_a, pair_b));
                    }
                }
            }
        }

        // Newly intersecting
        for (a, b) in current_intersections.difference(&self.st.intersecting_pairs) {
            self.push(
                q,
                Event::ObjectsIntersect {
                    a: a.clone(),
                    b: b.clone(),
                },
            );
        }

        // Stopped intersecting
        for (a, b) in self
            .st
            .intersecting_pairs
            .difference(&current_intersections)
        {
            self.push(
                q,
                Event::ObjectsStopIntersecting {
                    a: a.clone(),
                    b: b.clone(),
                },
            );
        }

        self.st.intersecting_pairs = current_intersections;
    }

    /// Reset every timer on `slide` to its full duration, stopped, with
    /// autostart armed so it begins when the owning clock reaches the
    /// object's start time.
    fn seed_timers(&mut self, slide: &Slide) {
        for object in slide
            .objects
            .iter()
            .chain(slide.layers.iter().flat_map(|l| l.objects.iter()))
        {
            if let ObjectKind::Timer { duration_ms, .. } = object.kind {
                self.st.timers.insert(
                    object.id.clone(),
                    crate::TimerState {
                        remaining_ms: duration_ms,
                        running: false,
                        ended: false,
                        autostart_armed: true,
                    },
                );
            }
        }
    }

    /// Save the current slide's clock and visible layers before leaving it.
    fn save_current_slide(&mut self) {
        if !self.st.started {
            return;
        }
        let Some(slide) = self.slide() else { return };
        if !self.st.visited.contains(&slide.id) {
            return;
        }
        let slide_id = slide.id.clone();
        let base = self.st.base;
        let visible_layers = self.st.visible_layers.clone();
        let clocks: Vec<(String, Clock)> = visible_layers
            .iter()
            .filter_map(|id| self.st.layer_clocks.get(id).map(|c| (id.clone(), *c)))
            .collect();
        let entry = self.st.saved_slides.entry(slide_id).or_default();
        entry.base = base;
        entry.visible_layers = visible_layers;
        for (id, clock) in clocks {
            entry.layer_clocks.insert(id, clock);
        }
    }

    /// Save one layer's clock so `revisit: resume` can continue from it.
    fn save_layer_clock(&mut self, id: &str) {
        let Some(slide) = self.slide() else { return };
        let slide_id = slide.id.clone();
        let Some(clock) = self.st.layer_clocks.get(id).copied() else {
            return;
        };
        self.st
            .saved_slides
            .entry(slide_id)
            .or_default()
            .layer_clocks
            .insert(id.to_owned(), clock);
    }

    fn reset_layer_states(&mut self, layer_id: &str) {
        let Some(slide) = self.slide() else { return };
        let Some(layer) = slide.layer(layer_id) else {
            return;
        };
        let initial: Vec<(String, String)> = layer
            .objects
            .iter()
            .map(|obj| (obj.id.clone(), obj.initial_state.clone()))
            .collect();
        let states = self.st.object_states.entry(slide.id.clone()).or_default();
        for (id, state) in initial {
            states.insert(id, state);
        }
    }

    fn show_layer(&mut self, id: &str, q: &mut Queue, out: &mut Vec<Effect>) {
        let Some(layer) = self.layer(id) else {
            warn(out, format!("layer {id:?} is not on this slide"));
            return;
        };
        if self.st.visible_layers.iter().any(|l| l == id) {
            return;
        }
        let saved_clock = self
            .slide()
            .and_then(|slide| self.st.saved_slides.get(&slide.id))
            .and_then(|saved| saved.layer_clocks.get(id))
            .copied();
        let was_base_paused = self.base_timeline_is_paused();
        if layer.hide_other_layers {
            for other in std::mem::take(&mut self.st.visible_layers) {
                self.save_layer_clock(&other);
                self.st.layer_clocks.remove(&other);
                self.stop_layer_media(&other, out);
                out.push(Effect::LayerHidden { layer: other });
            }
        }
        // `slide.layers` runs back to front, and visible layers stay in that
        // authored order however they were shown. Insert this id in its place
        // instead of appending it in show order.
        let authored: Vec<String> = self
            .slide()
            .map(|slide| slide.layers.iter().map(|layer| layer.id.clone()).collect())
            .unwrap_or_default();
        let position = authored.iter().position(|layer| layer == id);
        let at = position
            .map(|position| {
                self.st
                    .visible_layers
                    .iter()
                    .filter(|other| {
                        authored
                            .iter()
                            .position(|layer| layer == *other)
                            .is_none_or(|index| index < position)
                    })
                    .count()
            })
            .unwrap_or(self.st.visible_layers.len());
        let mut clock = Clock::default();
        match layer.revisit {
            RevisitMode::Auto => {}
            RevisitMode::Reset => self.reset_layer_states(id),
            RevisitMode::Resume => {
                // A layer that hides itself at the end would otherwise come back
                // already ended and never hide again, so it starts over instead.
                if let Some(saved) = saved_clock
                    && !(saved.ended && layer.hide_when_timeline_ends)
                {
                    clock = saved;
                }
            }
        }
        self.st.visible_layers.insert(at, id.to_owned());
        self.st.layer_clocks.insert(id.to_owned(), clock);
        self.seed_cues(layer.objects.iter());
        out.push(Effect::LayerShown {
            layer: id.to_owned(),
        });
        self.update_base_media_pause_state(was_base_paused, out);
        // A resumed clock past 0 must not replay start cues or autoplay that
        // are behind the learner. Ticking on crosses later cues normally.
        if clock.ms == 0 {
            self.autoplay(&layer.objects, 0, out);
            self.push(
                q,
                Event::TimelineStarts {
                    layer: Some(id.to_owned()),
                },
            );
        }
    }

    fn hide_layer(&mut self, id: &str, out: &mut Vec<Effect>) {
        let before = self.st.visible_layers.len();
        let was_base_paused = self.base_timeline_is_paused();
        self.st.visible_layers.retain(|l| l != id);
        if self.st.visible_layers.len() != before {
            self.save_layer_clock(id);
            self.st.layer_clocks.remove(id);
            self.stop_layer_media(id, out);
            out.push(Effect::LayerHidden {
                layer: id.to_owned(),
            });
            self.update_base_media_pause_state(was_base_paused, out);
        }
    }

    fn set_state(&mut self, object: &str, state: &str, q: &mut Queue, out: &mut Vec<Effect>) {
        let Some(slide) = self.slide() else { return };
        let Some((obj, _)) = slide.object(object) else {
            warn(out, format!("object {object:?} is not on this slide"));
            return;
        };
        if !obj.has_state(state) {
            warn(out, format!("object {object:?} has no state {state:?}"));
            return;
        }
        let states = self.st.object_states.entry(slide.id.clone()).or_default();
        if states.get(object).map(String::as_str) == Some(state) {
            return;
        }
        states.insert(object.to_owned(), state.to_owned());
        out.push(Effect::StateChanged {
            object: object.to_owned(),
            state: state.to_owned(),
        });
        self.push(
            q,
            Event::StateChanges {
                object: object.to_owned(),
                to: Some(state.to_owned()),
            },
        );
    }

    fn set_variable(
        &mut self,
        name: &str,
        op: VarOp,
        value: Option<&Value>,
        q: &mut Queue,
        out: &mut Vec<Effect>,
    ) {
        if coursecraft_course::timer_id_for_remaining_variable(name).is_some() {
            warn(out, format!("variable {name:?} is read-only"));
            return;
        }
        let Some(cur) = self.st.variables.get(name) else {
            warn(out, format!("variable {name:?} does not exist"));
            return;
        };
        let new = match (op, cur, value) {
            (VarOp::Assign, cur, Some(v)) if v.kind() == cur.kind() => v.clone(),
            (VarOp::Toggle, Value::Bool(b), _) => Value::Bool(!b),
            (VarOp::Add, Value::Number(a), Some(Value::Number(b))) => Value::Number(a + b),
            (VarOp::Subtract, Value::Number(a), Some(Value::Number(b))) => Value::Number(a - b),
            (VarOp::Multiply, Value::Number(a), Some(Value::Number(b))) => Value::Number(a * b),
            (VarOp::Divide, Value::Number(a), Some(Value::Number(b))) if *b != 0.0 => {
                Value::Number(a / b)
            }
            _ => {
                warn(out, format!("cannot apply {op:?} to variable {name:?}"));
                return;
            }
        };
        if let Value::Number(n) = new
            && !n.is_finite()
        {
            warn(
                out,
                format!("variable {name:?} would become non-finite; unchanged"),
            );
            return;
        }
        if *cur == new {
            return;
        }
        self.st.variables.insert(name.to_owned(), new.clone());
        out.push(Effect::VariableChanged {
            variable: name.to_owned(),
            value: new,
        });
        self.push(
            q,
            Event::VariableChanges {
                variable: name.to_owned(),
            },
        );
    }

    /// Commit a value from an input object. The host has already produced a
    /// value of the right kind; the runtime re-checks everything because a
    /// command or a script can send anything.
    fn set_input_value(
        &mut self,
        object: &str,
        value: &Value,
        q: &mut Queue,
        out: &mut Vec<Effect>,
    ) {
        let Some(slide) = self.slide() else { return };
        let Some((obj, _)) = slide.object(object) else {
            warn(out, format!("object {object:?} is not on this slide"));
            return;
        };
        if obj.kind.input_kind().is_none() {
            warn(out, format!("object {object:?} is not an input"));
            return;
        }
        if !self.object_is_live(object) {
            warn(out, format!("object {object:?} is not on screen"));
            return;
        }
        // A disabled input ignores every commitment silently, like a disabled click.
        if self.object_state(object) == Some("disabled") {
            return;
        }
        let (variable, commit) = match &obj.kind {
            ObjectKind::TextEntry {
                variable,
                max_chars,
                ..
            } => {
                let Value::Text(text) = value else {
                    warn(out, format!("text entry {object:?} needs a text value"));
                    return;
                };
                let limit = usize::try_from(*max_chars).unwrap_or(usize::MAX);
                let text = if *max_chars > 0 {
                    text.chars().take(limit).collect()
                } else {
                    text.clone()
                };
                (variable, Value::Text(text))
            }
            ObjectKind::Checkbox { variable, .. } => {
                let Value::Bool(checked) = value else {
                    warn(out, format!("checkbox {object:?} needs a true/false value"));
                    return;
                };
                (variable, Value::Bool(*checked))
            }
            ObjectKind::RadioGroup { variable, options } => {
                let Value::Text(text) = value else {
                    warn(out, format!("radio group {object:?} needs a text value"));
                    return;
                };
                if !options.iter().any(|option| &option.value == text) {
                    warn(
                        out,
                        format!("radio group {object:?} has no option {text:?}"),
                    );
                    return;
                }
                (variable, Value::Text(text.clone()))
            }
            ObjectKind::Slider {
                variable,
                min,
                max,
                step,
                ..
            }
            | ObjectKind::Dial {
                variable,
                min,
                max,
                step,
                ..
            } => {
                let Value::Number(number) = value else {
                    warn(out, format!("slider/dial {object:?} needs a number value"));
                    return;
                };
                if !min.is_finite() || !max.is_finite() || min >= max {
                    warn(
                        out,
                        format!("slider/dial {object:?} has an invalid range; unchanged"),
                    );
                    return;
                }
                (
                    variable,
                    Value::Number(coursecraft_course::snap_to_step(*min, *max, *step, *number)),
                )
            }
            _ => return,
        };
        self.set_variable(variable, VarOp::Assign, Some(&commit), q, out);
    }

    /// Toggle one marker's label. Opening any marker closes the other.
    fn marker_toggled(&mut self, object: &str, out: &mut Vec<Effect>) {
        let Some(slide) = self.slide() else {
            warn(out, "no slide is showing");
            return;
        };
        let Some((obj, _)) = slide.object(object) else {
            warn(out, format!("object {object:?} is not on this slide"));
            return;
        };
        if !matches!(obj.kind, ObjectKind::Marker { .. }) {
            warn(out, format!("object {object:?} is not a marker"));
            return;
        }
        if !self.object_is_live(object) {
            warn(out, format!("marker {object:?} is not on screen"));
            return;
        }
        if self.object_state(object) == Some("disabled") {
            return;
        }
        if self.st.marker_open.as_deref() == Some(object) {
            self.close_marker(object, out);
            return;
        }
        if let Some(open) = self.st.marker_open.clone() {
            self.close_marker(&open, out);
        }
        self.st.marker_open = Some(object.to_owned());
        out.push(Effect::MarkerOpened {
            object: object.to_owned(),
        });
        let mut q = Queue::new();
        self.push(
            &mut q,
            Event::MarkerOpened {
                object: object.to_owned(),
            },
        );
        self.process(&mut q, out);
    }

    /// Close whichever marker label is open, if any.
    fn markers_closed(&mut self, out: &mut Vec<Effect>) {
        if let Some(open) = self.st.marker_open.clone() {
            self.close_marker(&open, out);
        }
    }

    fn close_marker(&mut self, object: &str, out: &mut Vec<Effect>) {
        if self.st.marker_open.as_deref() != Some(object) {
            return;
        }
        self.st.marker_open = None;
        out.push(Effect::MarkerClosed {
            object: object.to_owned(),
        });
        let mut q = Queue::new();
        self.push(
            &mut q,
            Event::MarkerClosed {
                object: object.to_owned(),
            },
        );
        self.process(&mut q, out);
    }

    /// Apply a scrolling panel's new offset, clamped to its content.
    fn scroll_panel(&mut self, object: &str, offset: f32, out: &mut Vec<Effect>) {
        let Some(slide) = self.slide() else {
            warn(out, "no slide is showing");
            return;
        };
        let Some((obj, _)) = slide.object(object) else {
            warn(out, format!("object {object:?} is not on this slide"));
            return;
        };
        let ObjectKind::ScrollPanel { content_height, .. } = obj.kind else {
            warn(out, format!("object {object:?} is not a scrolling panel"));
            return;
        };
        if !self.object_is_live(object) {
            warn(out, format!("scrolling panel {object:?} is not on screen"));
            return;
        }
        let next = clamp_panel_offset(content_height, obj.frame.h, offset);
        if next.abs() <= f32::EPSILON {
            self.st.panel_offsets.remove(object);
        } else {
            self.st.panel_offsets.insert(object.to_owned(), next);
        }
    }

    fn pause(&mut self, owner: Option<&str>, paused: bool, out: &mut Vec<Effect>) {
        let was_base_paused = if owner.is_none() {
            Some(self.base_timeline_is_paused())
        } else {
            None
        };
        let clock = match owner {
            None => Some(&mut self.st.base),
            Some(l) => self.st.layer_clocks.get_mut(l),
        };
        let Some(clock) = clock else { return };
        if clock.paused == paused {
            return;
        }
        clock.paused = paused;
        let layer = owner.map(str::to_owned);
        out.push(if paused {
            Effect::TimelinePaused { layer }
        } else {
            Effect::TimelineResumed { layer }
        });
        if let Some(was_paused) = was_base_paused {
            self.update_base_media_pause_state(was_paused, out);
        }
    }

    // ------------------------------------------------------------ lightbox

    /// Show `slide` over the current slide. A second call replaces the open
    /// lightbox while keeping the same base slide paused underneath.
    fn open_lightbox(&mut self, slide_id: &str, q: &mut Queue, out: &mut Vec<Effect>) {
        let Some(idx) = self.slide_position(slide_id) else {
            warn(out, format!("slide {slide_id:?} does not exist"));
            return;
        };
        if self.st.lightbox.is_none() {
            if idx == self.st.slide {
                warn(out, "cannot open a lightbox on the current slide");
                return;
            }
            // Pause the base media but remember that it was playing, so closing
            // can resume exactly those objects.
            for (object, state) in &self.st.media_playback {
                if *state == MediaPlaybackState::Playing {
                    out.push(Effect::Media {
                        object: object.clone(),
                        op: MediaOp::Pause,
                    });
                }
            }
            let lb = Lightbox {
                slide: idx,
                base_slide: self.st.slide,
                base: self.st.base,
                base_visible_layers: self.st.visible_layers.clone(),
                base_layer_clocks: self.st.layer_clocks.clone(),
                base_media_playback: self.st.media_playback.clone(),
                base_media_clocks: self.st.media_clocks.clone(),
                base_running_paths: self.st.running_paths.clone(),
                base_object_offsets: self.st.object_offsets.clone(),
                base_selected: self.st.selected.clone(),
                base_text_entry: self.st.text_entry.clone(),
                base_sequence: self.st.sequence.clone(),
                base_matches: self.st.matches.clone(),
                base_hotspot_markers: self.st.hotspot_markers.clone(),
                base_drops: self.st.drops.clone(),
                base_marker_open: self.st.marker_open.clone(),
                base_panel_offsets: self.st.panel_offsets.clone(),
                base_object_positions: self.st.object_positions.clone(),
                base_intersecting_pairs: self.st.intersecting_pairs.clone(),
                base_timers: self.st.timers.clone(),
            };
            self.st.lightbox = Some(lb);
        } else {
            // Replacing: close the old box's media but keep the saved base.
            let old = self
                .st
                .lightbox
                .as_ref()
                .and_then(|lb| self.slides.get(lb.slide))
                .map(|s| s.id.clone());
            for object in self.st.media_playback.keys().cloned().collect::<Vec<_>>() {
                out.push(Effect::Media {
                    object,
                    op: MediaOp::Stop,
                });
            }
            if let Some(old) = old {
                out.push(Effect::LightboxClosed { slide: old });
            }
            if let Some(lb) = self.st.lightbox.as_mut() {
                lb.slide = idx;
            }
        }
        self.enter_lightbox_slide(idx, q, out);
    }

    fn enter_lightbox_slide(&mut self, idx: usize, q: &mut Queue, out: &mut Vec<Effect>) {
        let Some(slide) = self.slides.get(idx).copied() else {
            warn(out, "slide index out of range");
            return;
        };
        self.st.epoch = self.st.epoch.wrapping_add(1);
        self.st.slide = idx;
        self.st.base = Clock::default();
        self.st.visible_layers.clear();
        self.st.layer_clocks.clear();
        self.st.selected.clear();
        self.st.text_entry.clear();
        self.st.focused_object = None;
        self.st.sequence.clear();
        self.st.matches.clear();
        self.st.hotspot_markers.clear();
        self.st.drops.clear();
        self.st.marker_open = None;
        self.st.panel_offsets.clear();
        self.st.timers.clear();
        self.seed_timers(slide);
        self.st.running_paths.clear();
        self.st.object_offsets.clear();
        self.st.object_positions.clear();
        self.st.intersecting_pairs.clear();
        self.st.media_playback.clear();
        self.st.media_clocks.clear();
        self.st.cue_progress.clear();
        self.st.cue_held.clear();
        self.st.cue_layers.clear();
        self.seed_cues(
            slide
                .objects
                .iter()
                .chain(slide.layers.iter().flat_map(|layer| layer.objects.iter())),
        );
        self.st.visited.insert(slide.id.clone());
        let states = self
            .st
            .object_states
            .entry(slide.id.clone())
            .or_insert_with(|| {
                slide
                    .objects
                    .iter()
                    .chain(slide.layers.iter().flat_map(|l| l.objects.iter()))
                    .map(|o| (o.id.clone(), o.initial_state.clone()))
                    .collect()
            });
        if slide.revisit == RevisitMode::Reset {
            for object in slide
                .objects
                .iter()
                .chain(slide.layers.iter().flat_map(|l| l.objects.iter()))
            {
                states.insert(object.id.clone(), object.initial_state.clone());
            }
        }
        out.push(Effect::LightboxOpened {
            slide: slide.id.clone(),
        });
        self.autoplay(&slide.objects, 0, out);
        self.push(q, Event::TimelineStarts { layer: None });
        self.check_intersections(q);
    }

    /// Close the open lightbox and restore the base slide's saved runtime.
    ///
    /// `resume_media` sends `Media { Play }` for the base objects that were
    /// playing when the box opened. A jump closes without those plays because
    /// entering the new slide stops them immediately anyway.
    fn close_lightbox(&mut self, resume_media: bool, out: &mut Vec<Effect>) {
        let Some(lb) = self.st.lightbox.take() else {
            return;
        };
        // The slide inside the box exits: stop everything it was playing.
        for object in self.st.media_playback.keys().cloned().collect::<Vec<_>>() {
            out.push(Effect::Media {
                object,
                op: MediaOp::Stop,
            });
        }
        if let Some(slide) = self.slides.get(lb.slide) {
            out.push(Effect::LightboxClosed {
                slide: slide.id.clone(),
            });
        }
        self.st.epoch = self.st.epoch.wrapping_add(1);
        self.st.slide = lb.base_slide;
        self.st.base = lb.base;
        self.st.visible_layers = lb.base_visible_layers;
        self.st.layer_clocks = lb.base_layer_clocks;
        self.st.media_playback = lb.base_media_playback;
        self.st.media_clocks = lb.base_media_clocks;
        self.st.running_paths = lb.base_running_paths;
        self.st.object_offsets = lb.base_object_offsets;
        self.st.selected = lb.base_selected;
        self.st.text_entry = lb.base_text_entry;
        self.st.focused_object = None;
        self.st.sequence = lb.base_sequence;
        self.st.matches = lb.base_matches;
        self.st.hotspot_markers = lb.base_hotspot_markers;
        self.st.drops = lb.base_drops;
        self.st.marker_open = lb.base_marker_open;
        self.st.panel_offsets = lb.base_panel_offsets;
        self.st.object_positions = lb.base_object_positions;
        self.st.intersecting_pairs = lb.base_intersecting_pairs;
        self.st.timers = lb.base_timers;
        self.check_intersections(&mut Queue::new());
        if resume_media {
            for (object, state) in &self.st.media_playback {
                if *state == MediaPlaybackState::Playing {
                    out.push(Effect::Media {
                        object: object.clone(),
                        op: MediaOp::Play,
                    });
                }
            }
        }
    }

    // ------------------------------------------------------------ input helpers

    /// Is the object on a visible layer, inside its timing window, not hidden,
    /// and has not completed an exit animation?
    fn object_is_live(&self, id: &str) -> bool {
        let Some(slide) = self.slide() else {
            return false;
        };
        let Some((obj, layer)) = slide.object(id) else {
            return false;
        };
        if layer.is_none() && self.base_objects_hidden() {
            return false;
        }
        let (t, duration_ms) = match layer {
            None => (self.st.base.ms, slide.duration_ms),
            Some(l) => match self.st.layer_clocks.get(l) {
                Some(c) => (c.ms, slide.layer(l).map_or(5000, |l| l.duration_ms)),
                None => return false, // layer not visible
            },
        };
        let end_ms = obj.timing.end_ms.unwrap_or(duration_ms);
        let exited = obj.exit.is_some() && t >= end_ms;
        let in_window =
            obj.timing.start_ms <= t && obj.timing.end_ms.is_none_or(|end| t < end) && !exited;
        in_window && self.object_state(id) != Some("hidden")
    }

    /// Whether any trigger on the current slide listens for `key` (globally or
    /// on an object). Read-only; used by the renderer to avoid sending key
    /// inputs that no trigger can use.
    #[must_use]
    pub fn has_key_trigger(&self, key: &str) -> bool {
        let Some(slide) = self.slide() else {
            return false;
        };
        let listens = |triggers: &[Trigger]| {
            triggers.iter().filter(|t| !t.disabled).any(|trigger| {
                matches!(
                    &trigger.when,
                    Event::KeyPressed { key: fired, .. }
                        if fired.eq_ignore_ascii_case(key)
                )
            })
        };
        listens(&slide.triggers)
            || self.st.visible_layers.iter().any(|id| {
                slide
                    .layer(id)
                    .is_some_and(|layer| listens(&layer.triggers))
            })
    }

    /// Why a click on `id` would or would not reach the object.
    fn click_check(&self, id: &str) -> ClickCheck<'c> {
        let Some(slide) = self.slide() else {
            return ClickCheck::Missing;
        };
        let Some((obj, layer)) = slide.object(id) else {
            return ClickCheck::Missing;
        };
        if !self.object_is_live(id) {
            return ClickCheck::NotLive;
        }
        if self.object_state(id) == Some("disabled") {
            return ClickCheck::Disabled;
        }
        if layer.is_none()
            && self
                .st
                .visible_layers
                .iter()
                .any(|l| slide.layer(l).is_some_and(|l| l.prevent_base_clicks))
        {
            return ClickCheck::Prevented;
        }
        ClickCheck::Reaches { obj, layer }
    }

    /// Text links on the current slide, in reading order, with the object that
    /// carries each one. Links on hidden, disabled or off-screen objects are
    /// left out, exactly as their object's click would be.
    #[must_use]
    pub fn link_objects(&self) -> Vec<(String, Vec<Link>)> {
        let Some(slide) = self.slide() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let mut collect = |obj: &coursecraft_course::Object| {
            if !matches!(self.click_check(&obj.id), ClickCheck::Reaches { .. }) {
                return;
            }
            let links = object_links(obj);
            if !links.is_empty() {
                out.push((obj.id.clone(), links));
            }
        };
        for obj in &slide.objects {
            collect(obj);
        }
        for layer_id in &self.st.visible_layers {
            if let Some(layer) = slide.layer(layer_id) {
                for obj in &layer.objects {
                    collect(obj);
                }
            }
        }
        out
    }

    /// The valid links carried by one object, in run order.
    #[must_use]
    pub fn object_links(&self, object: &str) -> Vec<Link> {
        let Some(slide) = self.slide() else {
            return Vec::new();
        };
        let Some((obj, _)) = slide.object(object) else {
            return Vec::new();
        };
        object_links(obj)
    }

    /// Objects on the current slide that a click reaches right now and that
    /// should appear in the keyboard focus order.
    ///
    /// Buttons, inputs, markers, scroll panels and draggable objects are always
    /// candidates; other objects only when a trigger on the slide listens for
    /// their click or hover. The shared [`Player::click_check`] keeps this list
    /// in step with [`Player::click`].
    #[must_use]
    pub fn keyboard_objects(&self) -> Vec<String> {
        let Some(slide) = self.slide() else {
            return Vec::new();
        };
        let is_candidate = |obj: &coursecraft_course::Object| {
            obj.kind.input_kind().is_some()
                || obj.drag.is_some()
                || matches!(
                    obj.kind,
                    ObjectKind::Button { .. }
                        | ObjectKind::Marker { .. }
                        | ObjectKind::ScrollPanel { .. }
                )
                || has_pointer_trigger(slide, &obj.id)
        };
        let mut out = Vec::new();
        for obj in &slide.objects {
            if is_candidate(obj)
                && matches!(self.click_check(&obj.id), ClickCheck::Reaches { .. })
                && self.object_in_panel_viewport(&obj.id)
            {
                out.push(obj.id.clone());
            }
        }
        for layer_id in &self.st.visible_layers {
            let Some(layer) = slide.layer(layer_id) else {
                continue;
            };
            for obj in &layer.objects {
                if is_candidate(obj)
                    && matches!(self.click_check(&obj.id), ClickCheck::Reaches { .. })
                    && self.object_in_panel_viewport(&obj.id)
                {
                    out.push(obj.id.clone());
                }
            }
        }
        out
    }

    fn click(&mut self, id: &str, out: &mut Vec<Effect>) {
        if let Some(open) = self.st.marker_open.clone()
            && open != id
        {
            self.close_marker(&open, out);
        }
        let (obj, _layer) = match self.click_check(id) {
            ClickCheck::Reaches { obj, layer } => (obj, layer),
            ClickCheck::Missing => {
                warn(out, format!("object {id:?} is not on this slide"));
                return;
            }
            ClickCheck::NotLive => {
                warn(out, format!("object {id:?} is not on screen"));
                return;
            }
            ClickCheck::Disabled | ClickCheck::Prevented => {
                return; // disabled objects swallow clicks silently
            }
        };
        let before = self.object_state(id).map(str::to_owned);
        let epoch = self.st.epoch;
        let mut q = Queue::new();
        self.push(
            &mut q,
            Event::Click {
                object: id.to_owned(),
            },
        );
        self.process(&mut q, out);

        // Built-in states apply after the click's triggers, and only if those
        // triggers left the object's state alone and stayed on this slide.
        if self.st.epoch != epoch
            || self.st.exited
            || self.object_state(id).map(str::to_owned) != before
        {
            return;
        }
        let has = |n: &str| obj.states.iter().any(|s| s.name == n);
        let next = if has("selected") {
            Some(if before.as_deref() == Some("selected") {
                "normal"
            } else {
                "selected"
            })
        } else if has("visited") && before.as_deref() != Some("visited") {
            Some("visited")
        } else {
            None
        };
        if let Some(state) = next {
            self.set_state(id, state, &mut q, out);
            self.process(&mut q, out);
        }
        self.check_intersections(&mut q);
        self.process(&mut q, out);
    }

    // ------------------------------------------------------------ timelines

    fn tick(&mut self, dt: u32, q: &mut Queue, out: &mut Vec<Effect>) {
        self.st.background.started_pending = false;
        if self.st.player_paused {
            return;
        }
        if self.st.reviewing_from.is_some() {
            return;
        }
        let Some(slide) = self.slides.get(self.st.slide).copied() else {
            return;
        };
        self.advance_media_clocks(dt);
        let base_before = self.st.base.ms;
        let base_held = self
            .st
            .visible_layers
            .iter()
            .any(|l| slide.layer(l).is_some_and(|l| l.pause_base_timeline));
        if !base_held {
            let clock = self.st.base;
            let tl = Timeline {
                duration: slide.duration_ms,
                layer: None,
                objects: &slide.objects,
            };
            self.st.base = self.advance(clock, dt, &tl, q, out);
        }
        let base_after = self.st.base.ms;
        // How far each visible layer's clock moved this tick, for its timers.
        let mut layer_deltas: BTreeMap<String, (u32, u32)> = BTreeMap::new();
        for id in self.st.visible_layers.clone() {
            let (Some(layer), Some(clock)) =
                (slide.layer(&id), self.st.layer_clocks.get(&id).copied())
            else {
                continue;
            };
            let before = clock.ms;
            let tl = Timeline {
                duration: layer.duration_ms,
                layer: Some(&id),
                objects: &layer.objects,
            };
            let next = self.advance(clock, dt, &tl, q, out);
            layer_deltas.insert(id.clone(), (before, next.ms));
            self.st.layer_clocks.insert(id, next);
        }

        self.note_timeline_completed();
        self.advance_timers(slide, base_before, base_after, &layer_deltas, q, out);
        self.advance_running_paths(dt, q);
        self.check_intersections(q);
    }

    /// Advance every timer on the slide by the distance its owning clock
    /// moved this tick. A held, paused or hidden timeline moves no clock, so
    /// its timers hold too; the player's pause and an open lightbox stop the
    /// tick before this point (or freeze the base slide), so those stop too.
    fn advance_timers(
        &mut self,
        slide: &Slide,
        base_before: u32,
        base_after: u32,
        layer_deltas: &BTreeMap<String, (u32, u32)>,
        q: &mut Queue,
        out: &mut Vec<Effect>,
    ) {
        // (object id, autostart, start_ms, owner clock before, owner clock after)
        let mut timers: Vec<(String, bool, u32, u32, u32)> = Vec::new();
        for object in &slide.objects {
            if let ObjectKind::Timer { autostart, .. } = object.kind {
                timers.push((
                    object.id.clone(),
                    autostart,
                    object.timing.start_ms,
                    base_before,
                    base_after,
                ));
            }
        }
        for layer in &slide.layers {
            let Some((before, after)) = layer_deltas.get(&layer.id) else {
                continue;
            };
            for object in &layer.objects {
                if let ObjectKind::Timer { autostart, .. } = object.kind {
                    timers.push((
                        object.id.clone(),
                        autostart,
                        object.timing.start_ms,
                        *before,
                        *after,
                    ));
                }
            }
        }
        for (id, autostart, start_ms, before, after) in timers {
            let delta = after.saturating_sub(before);
            let state = self.st.timers.entry(id.clone()).or_default();
            let mut effective = delta;
            if state.autostart_armed && autostart {
                if after < start_ms {
                    continue;
                }
                state.autostart_armed = false;
                state.running = true;
                // Only the part of this tick after the start time counts.
                effective = delta.min(after.saturating_sub(start_ms));
                out.push(Effect::TimerStarted { timer: id.clone() });
            }
            if !state.running || state.ended || effective == 0 {
                continue;
            }
            let before_seconds = coursecraft_course::timer_remaining_seconds(state.remaining_ms);
            state.remaining_ms = state.remaining_ms.saturating_sub(effective);
            let after_seconds = coursecraft_course::timer_remaining_seconds(state.remaining_ms);
            let seconds_changed = after_seconds != before_seconds;
            let reached_zero = state.remaining_ms == 0;
            if reached_zero {
                state.running = false;
                state.ended = true;
            }
            // Drop the state borrow before queueing events.
            let _ = state;
            if seconds_changed {
                self.push(
                    q,
                    Event::VariableChanges {
                        variable: coursecraft_course::timer_remaining_variable(&id),
                    },
                );
            }
            if reached_zero {
                out.push(Effect::TimerEnded { timer: id.clone() });
                self.push(q, Event::TimerEnds { timer: id });
            }
        }
    }

    /// Move every Lottie object that is playing forward by `dt`.
    ///
    /// Audio and video positions belong to their playback backend, so only
    /// animations keep a clock here. Paused, timeline-held and player-held
    /// animations do not move.
    fn advance_media_clocks(&mut self, dt: u32) {
        let playing: Vec<String> = self
            .st
            .media_playback
            .iter()
            .filter(|(_, state)| **state == MediaPlaybackState::Playing)
            .map(|(id, _)| id.clone())
            .collect();
        for id in playing {
            if self.object_is_lottie(&id) {
                let clock = self.st.media_clocks.entry(id).or_insert(0);
                *clock = clock.saturating_add(u64::from(dt));
            }
        }
    }

    fn advance_running_paths(&mut self, dt: u32, q: &mut Queue) {
        if self.st.running_paths.is_empty() {
            return;
        }
        let Some(slide) = self.slide() else { return };
        let base_paused = self.base_timeline_is_paused();

        let mut completed = Vec::new();
        for (obj_id, running) in &mut self.st.running_paths {
            let Some((obj, layer_id)) = slide.object(obj_id) else {
                continue;
            };
            let is_paused = match layer_id {
                None => base_paused,
                Some(lid) => {
                    !self.st.visible_layers.iter().any(|vl| vl == lid)
                        || self.st.layer_clocks.get(lid).is_none_or(|c| c.paused)
                }
            };
            if is_paused {
                continue;
            }
            running.elapsed_ms = running.elapsed_ms.saturating_add(dt);
            if running.elapsed_ms >= running.path.duration_ms {
                let end_point = running.path.points.last().copied().unwrap_or_default();
                let resting = if running.path.relative {
                    (running.from.0 + end_point.x, running.from.1 + end_point.y)
                } else {
                    (end_point.x - obj.frame.x, end_point.y - obj.frame.y)
                };
                completed.push((obj_id.clone(), running.path.id.clone(), resting));
            }
        }

        for (obj_id, path_id, resting) in completed {
            self.st.running_paths.remove(&obj_id);
            self.st.object_offsets.insert(obj_id.clone(), resting);
            self.push(
                q,
                Event::AnimationCompletes {
                    object: obj_id,
                    path: Some(path_id),
                },
            );
        }
        self.check_intersections(q);
    }

    fn advance(
        &mut self,
        clock: Clock,
        dt: u32,
        tl: &Timeline<'_>,
        q: &mut Queue,
        out: &mut Vec<Effect>,
    ) -> Clock {
        let Timeline {
            duration,
            layer,
            objects,
        } = *tl;
        if clock.paused || clock.ended {
            return clock;
        }
        let old = clock.ms;
        let new = old.saturating_add(dt).min(duration);
        let layer_owned = layer.map(str::to_owned);
        for ms in self.reach_points(layer) {
            if old < ms && ms <= new {
                self.push(
                    q,
                    Event::TimelineReaches {
                        ms,
                        layer: layer_owned.clone(),
                    },
                );
            }
        }
        for o in objects {
            if let Some(ref anim) = o.entrance {
                let finish_ms = o.timing.start_ms.saturating_add(anim.duration_ms);
                if old < finish_ms && finish_ms <= new {
                    self.push(
                        q,
                        Event::AnimationCompletes {
                            object: o.id.clone(),
                            path: None,
                        },
                    );
                }
            }
            if old < o.timing.start_ms && o.timing.start_ms <= new {
                let auto = matches!(
                    o.kind,
                    ObjectKind::Audio { autoplay: true, .. }
                        | ObjectKind::Video { autoplay: true, .. }
                        | ObjectKind::Lottie { autoplay: true, .. }
                );
                if auto {
                    let elapsed = u64::from(new.saturating_sub(o.timing.start_ms));
                    self.start_autoplay(&o.id, elapsed, out);
                }
            }
        }
        let ended = new >= duration;
        if ended {
            let hide_after = layer.and_then(|id| {
                self.layer(id)
                    .filter(|layer| layer.hide_when_timeline_ends && duration > 0)
                    .map(|_| id.to_owned())
            });
            self.push_with_hide(q, Event::TimelineEnds { layer: layer_owned }, hide_after);
        }
        Clock {
            ms: new,
            ended,
            paused: false,
        }
    }

    /// Autoplay reached `object`. While the learner is paused it waits as
    /// `PausedByPlayer` and emits no `Play`. `elapsed_ms` is how much of the
    /// animation is already behind the crossing, so a single long tick lands on
    /// the right frame.
    fn start_autoplay(&mut self, object: &str, elapsed_ms: u64, out: &mut Vec<Effect>) {
        if self.object_is_lottie(object) {
            self.st.media_clocks.insert(object.to_owned(), elapsed_ms);
        }
        if self.st.player_paused {
            self.st
                .media_playback
                .insert(object.to_owned(), MediaPlaybackState::PausedByPlayer);
            return;
        }
        self.st
            .media_playback
            .insert(object.to_owned(), MediaPlaybackState::Playing);
        out.push(Effect::Media {
            object: object.to_owned(),
            op: MediaOp::Play,
        });
        // A trimmed clip starts at its trim start, not at file time 0.
        if let Some(trim) = self
            .slide()
            .and_then(|slide| slide.object(object))
            .and_then(|(obj, _)| media_trim(obj))
        {
            let mut ms = trim.start_ms.saturating_add(elapsed_ms);
            if let Some(end_ms) = trim.end_ms {
                ms = ms.min(end_ms);
            }
            out.push(Effect::MediaSeek {
                object: object.to_owned(),
                ms,
            });
        }
    }

    fn autoplay(
        &mut self,
        objects: &[coursecraft_course::Object],
        at_ms: u32,
        out: &mut Vec<Effect>,
    ) {
        for o in objects {
            let auto = matches!(
                o.kind,
                ObjectKind::Audio { autoplay: true, .. }
                    | ObjectKind::Video { autoplay: true, .. }
                    | ObjectKind::Lottie { autoplay: true, .. }
            );
            if auto && o.timing.start_ms == at_ms {
                self.start_autoplay(&o.id, 0, out);
            }
        }
    }

    /// What `player_seek` and `seek_info` act on. See "Learner pause, resume and seek".
    pub(crate) fn seek_target(&self) -> SeekTarget {
        let Some(slide) = self.slide() else {
            return SeekTarget::Refused;
        };
        for id in self.st.visible_layers.iter().rev() {
            let Some(layer) = slide.layer(id) else {
                continue;
            };
            match layer.allow_seek {
                SeekMode::No => return SeekTarget::Refused,
                SeekMode::Yes => return SeekTarget::Layer(id.clone()),
                SeekMode::Auto if layer.pause_base_timeline => {
                    return SeekTarget::Layer(id.clone());
                }
                SeekMode::Auto => {}
            }
        }
        SeekTarget::Base
    }

    /// Move the clock the seekbar targets, clamped to that timeline's duration.
    fn seek(&mut self, ms: u64, q: &mut Queue, out: &mut Vec<Effect>) {
        let Some(slide) = self.slides.get(self.st.slide).copied() else {
            return;
        };
        match self.seek_target() {
            SeekTarget::Refused => {
                warn(out, "the slide timeline is held by a layer; seek refused");
            }
            SeekTarget::Layer(id) => self.seek_layer(&id, ms, q, out),
            SeekTarget::Base => {
                if self.base_held_by_layer() {
                    warn(out, "the slide timeline is held by a layer; seek refused");
                    return;
                }
                self.seek_base(ms, slide, q, out);
            }
        }
    }

    fn seek_base(&mut self, ms: u64, slide: &Slide, q: &mut Queue, out: &mut Vec<Effect>) {
        let duration = slide.duration_ms;
        let target = u32::try_from(ms).unwrap_or(u32::MAX).min(duration);
        let old = self.st.base.ms;
        let paused = self.st.base.paused;
        let epoch = self.st.epoch;
        if target > old {
            let tl = Timeline {
                duration,
                layer: None,
                objects: &slide.objects,
            };
            // A seek moves the clock even when an author pause holds it, but it
            // must not emit a resume or unfreeze the clock.
            let clock = Clock {
                ms: old,
                ended: false,
                paused: false,
            };
            let next = self.advance(clock, target - old, &tl, q, out);
            self.st.base = Clock {
                ms: next.ms,
                ended: next.ended,
                paused,
            };
        } else if target < old {
            self.st.base = Clock {
                ms: target,
                ended: target >= duration,
                paused,
            };
        }
        if self.st.epoch != epoch || self.st.exited {
            return; // a cue jumped to another slide; this seek belongs to the old one
        }
        self.seek_media(target, &slide.objects, duration, out);
    }

    fn seek_layer(&mut self, id: &str, ms: u64, q: &mut Queue, out: &mut Vec<Effect>) {
        let Some(slide) = self.slides.get(self.st.slide).copied() else {
            return;
        };
        let Some(layer) = slide.layer(id) else {
            return;
        };
        if !self.st.visible_layers.iter().any(|visible| visible == id) {
            return;
        }
        let duration = layer.duration_ms;
        let target = u32::try_from(ms).unwrap_or(u32::MAX).min(duration);
        let Some(clock) = self.st.layer_clocks.get(id).copied() else {
            return;
        };
        let old = clock.ms;
        let paused = clock.paused;
        let epoch = self.st.epoch;
        let next = if target > old {
            let tl = Timeline {
                duration,
                layer: Some(id),
                objects: &layer.objects,
            };
            let moved = self.advance(
                Clock {
                    ms: old,
                    ended: false,
                    paused: false,
                },
                target - old,
                &tl,
                q,
                out,
            );
            Clock {
                ms: moved.ms,
                ended: moved.ended,
                paused,
            }
        } else if target < old {
            Clock {
                ms: target,
                ended: target >= duration,
                paused,
            }
        } else {
            clock
        };
        if self.st.epoch != epoch || self.st.exited {
            return; // a cue jumped to another slide; this seek belongs to the old one
        }
        // A cue may have hidden the layer (including `hide_when_timeline_ends`).
        // Leaving it hidden is the same outcome as a hide during a base seek.
        if !self.st.visible_layers.iter().any(|visible| visible == id) {
            return;
        }
        self.st.layer_clocks.insert(id.to_owned(), next);
        self.seek_media(target, &layer.objects, duration, out);
    }

    /// Reposition the timeline-driven media after a clock seek.
    fn seek_media(
        &mut self,
        position: u32,
        objects: &[coursecraft_course::Object],
        duration: u32,
        out: &mut Vec<Effect>,
    ) {
        let player_paused = self.st.player_paused;
        for obj in objects {
            if let ObjectKind::Video { cues, .. } = &obj.kind
                && !cues.is_empty()
            {
                let trim = media_trim(obj);
                let mut media_at = f64::from(position.saturating_sub(obj.timing.start_ms));
                if let Some(trim) = trim {
                    media_at += trim.start_ms as f64;
                    if let Some(end_ms) = trim.end_ms {
                        media_at = media_at.min(end_ms as f64);
                    }
                }
                self.seek_cue_position(&obj.id, media_at);
                if self.st.media_playback.get(&obj.id) == Some(&MediaPlaybackState::PausedAtCue) {
                    self.st
                        .media_playback
                        .insert(obj.id.clone(), MediaPlaybackState::Paused);
                }
            }
        }
        for obj in objects {
            let autoplay = matches!(
                obj.kind,
                ObjectKind::Audio { autoplay: true, .. }
                    | ObjectKind::Video { autoplay: true, .. }
                    | ObjectKind::Lottie { autoplay: true, .. }
            );
            if !autoplay {
                continue;
            }
            let is_lottie = matches!(obj.kind, ObjectKind::Lottie { .. });
            let start = obj.timing.start_ms;
            let end = obj.timing.end_ms.unwrap_or(duration);
            let state = self.st.media_playback.get(&obj.id).copied();
            if state == Some(MediaPlaybackState::PausedAtCue) {
                self.st
                    .media_playback
                    .insert(obj.id.clone(), MediaPlaybackState::Paused);
                self.st.cue_held.remove(&obj.id);
            }
            if start > position || position >= end {
                if state.is_some() {
                    self.st.media_playback.remove(&obj.id);
                    self.st.media_clocks.remove(&obj.id);
                    out.push(Effect::Media {
                        object: obj.id.clone(),
                        op: MediaOp::Stop,
                    });
                }
                continue;
            }
            let offset = u64::from(position - start);
            let trim = media_trim(obj);
            let mut media_ms = trim.map_or(0, |trim| trim.start_ms).saturating_add(offset);
            if let Some(end_ms) = trim.and_then(|trim| trim.end_ms) {
                media_ms = media_ms.min(end_ms);
            }
            match state {
                Some(_) => out.push(Effect::MediaSeek {
                    object: obj.id.clone(),
                    ms: media_ms,
                }),
                None if player_paused => {
                    self.st
                        .media_playback
                        .insert(obj.id.clone(), MediaPlaybackState::PausedByPlayer);
                    out.push(Effect::MediaSeek {
                        object: obj.id.clone(),
                        ms: media_ms,
                    });
                }
                None => {
                    // `start_autoplay` emits the mapped seek for a trimmed
                    // object; untrimmed objects keep their explicit seek.
                    self.start_autoplay(&obj.id, offset, out);
                    if trim.is_none() {
                        out.push(Effect::MediaSeek {
                            object: obj.id.clone(),
                            ms: media_ms,
                        });
                    }
                }
            }
            if is_lottie {
                self.st.media_clocks.insert(obj.id.clone(), offset);
            }
        }
    }

    /// Distinct `timeline_reaches` points for one timeline, ascending.
    fn reach_points(&self, layer: Option<&str>) -> Vec<u32> {
        let Some(slide) = self.slide() else {
            return Vec::new();
        };
        let mut v: Vec<u32> = slide
            .triggers
            .iter()
            .chain(slide.layers.iter().flat_map(|l| l.triggers.iter()))
            .filter(|t| !t.disabled)
            .filter_map(|t| match &t.when {
                Event::TimelineReaches { ms, layer: l } if l.as_deref() == layer => Some(*ms),
                _ => None,
            })
            .collect();
        v.sort_unstable();
        v.dedup();
        v
    }

    // ------------------------------------------------------------ quiz

    fn question_locked_on_slide(&self, slide: &Slide) -> bool {
        self.question_locked_on(slide)
    }

    fn select_choice(&mut self, choice: &str, out: &mut Vec<Effect>) {
        let Some(slide) = self.slide() else { return };
        let Some(question) = &slide.question else {
            warn(out, "this slide has no question");
            return;
        };
        if !is_choice_kind(question.kind) {
            warn(out, "select_choice does not apply to this question");
            return;
        }
        if !question.choices.iter().any(|c| c.id == choice) {
            warn(out, format!("choice {choice:?} does not exist"));
            return;
        }
        if self.question_locked_on_slide(slide) {
            warn(out, "this question is already answered");
            return;
        }
        if question.kind == QuestionKind::MultipleResponse {
            let selected = if self.st.selected.remove(choice) {
                false
            } else {
                self.st.selected.insert(choice.to_owned());
                true
            };
            out.push(Effect::ChoiceSelected {
                choice: choice.to_owned(),
                selected,
            });
            return;
        }
        for prev in std::mem::take(&mut self.st.selected) {
            if prev != choice {
                out.push(Effect::ChoiceSelected {
                    choice: prev,
                    selected: false,
                });
            }
        }
        self.st.selected.insert(choice.to_owned());
        out.push(Effect::ChoiceSelected {
            choice: choice.to_owned(),
            selected: true,
        });
    }

    fn text_entered(&mut self, text: &str, out: &mut Vec<Effect>) {
        let Some(slide) = self.slide() else { return };
        let Some(question) = &slide.question else {
            warn(out, "this slide has no question");
            return;
        };
        if question.kind != QuestionKind::FillIn
            && question.kind != QuestionKind::ShortAnswer
            && question.kind != QuestionKind::Essay
        {
            warn(out, "text_entered does not apply to this question");
            return;
        }
        if self.question_locked_on_slide(slide) {
            warn(out, "this question is already answered");
            return;
        }
        let cap = if question.kind == QuestionKind::ShortAnswer {
            SHORT_ANSWER_CAP
        } else {
            TEXT_CAP
        };
        let text = cap_chars(text, cap);
        self.st.text_entry = text.clone();
        out.push(Effect::TextEntryChanged { text });
    }

    fn sequence_moved(&mut self, choice: &str, to: usize, out: &mut Vec<Effect>) {
        let Some(slide) = self.slide() else { return };
        let Some(question) = &slide.question else {
            warn(out, "this slide has no question");
            return;
        };
        if question.kind != QuestionKind::Sequence {
            warn(out, "sequence_moved does not apply to this question");
            return;
        }
        if !self.st.sequence.iter().any(|id| id == choice) {
            warn(out, format!("choice {choice:?} does not exist"));
            return;
        }
        if self.question_locked_on_slide(slide) {
            warn(out, "this question is already answered");
            return;
        }
        move_item(&mut self.st.sequence, choice, to);
        out.push(Effect::SequenceChanged {
            order: self.st.sequence.clone(),
        });
    }

    fn match_selected(&mut self, left: &str, right: Option<&str>, out: &mut Vec<Effect>) {
        let Some(slide) = self.slide() else { return };
        let Some(question) = &slide.question else {
            warn(out, "this slide has no question");
            return;
        };
        if question.kind != QuestionKind::Matching {
            warn(out, "match_selected does not apply to this question");
            return;
        }
        if !question.pairs.iter().any(|p| p.id == left) {
            warn(out, format!("pair {left:?} does not exist"));
            return;
        }
        if let Some(right_id) = right
            && !question.pairs.iter().any(|p| p.id == right_id)
        {
            warn(out, format!("pair {right_id:?} does not exist"));
            return;
        }
        if self.question_locked_on_slide(slide) {
            warn(out, "this question is already answered");
            return;
        }
        match right {
            Some(right_id) => {
                self.st.matches.insert(left.to_owned(), right_id.to_owned());
            }
            None => {
                self.st.matches.remove(left);
            }
        }
        out.push(Effect::MatchChanged {
            left: left.to_owned(),
            right: right.map(str::to_owned),
        });
    }

    fn hotspot_clicked(&mut self, x: f32, y: f32, out: &mut Vec<Effect>) {
        let Some(slide) = self.slide() else { return };
        let Some(question) = &slide.question else {
            warn(out, "this slide has no question");
            return;
        };
        if question.kind != QuestionKind::Hotspot {
            warn(out, "hotspot_clicked does not apply to this question");
            return;
        }
        if self.question_locked_on_slide(slide) {
            warn(out, "this question is already answered");
            return;
        }
        if !(1..=10).contains(&question.markers) {
            warn(out, "markers must be 1 to 10");
            return;
        }
        if !x.is_finite() || !y.is_finite() {
            warn(out, "hotspot coordinates must be finite");
            return;
        }
        let Some(image_id) = question.hotspot_image.clone() else {
            warn(out, "hotspot image is not set");
            return;
        };
        let Some((object, layer)) = slide.object(&image_id) else {
            warn(out, "hotspot image is not on this slide");
            return;
        };
        if layer.is_some() {
            warn(out, "hotspot image is not a base-layer object");
            return;
        }
        let Some(sample) = self.sample_object(&image_id) else {
            warn(out, "hotspot image is not on screen");
            return;
        };
        if !point_in_sampled_frame(object.frame, sample.offset, sample.scale, x, y) {
            warn(out, "click is outside the hotspot image");
            return;
        }
        let limit = question.markers as usize;
        if limit == 1 {
            self.st.hotspot_markers.clear();
        } else if self.st.hotspot_markers.len() >= limit {
            self.st.hotspot_markers.remove(0);
        }
        self.st.hotspot_markers.push((x, y));
        out.push(Effect::HotspotMarked {
            markers: self.st.hotspot_markers.clone(),
        });
    }

    fn drag_dropped(
        &mut self,
        item: &str,
        target: Option<&str>,
        x: f32,
        y: f32,
        out: &mut Vec<Effect>,
    ) {
        let Some(slide) = self.slide() else { return };
        let Some(question) = &slide.question else {
            warn(out, "this slide has no question");
            return;
        };
        if question.kind != QuestionKind::DragDrop {
            warn(out, "drag_dropped does not apply to this question");
            return;
        }
        if self.question_locked_on_slide(slide) {
            warn(out, "this question is already answered");
            return;
        }
        if item.is_empty() || !question.drag_items.iter().any(|d| d.item == item) {
            warn(out, format!("drag item {item:?} does not exist"));
            return;
        }
        if !x.is_finite() || !y.is_finite() {
            warn(out, "drag coordinates must be finite");
            return;
        }
        let Some((object, layer)) = slide.object(item) else {
            warn(out, format!("drag item {item:?} is not on this slide"));
            return;
        };
        if layer.is_some() {
            warn(
                out,
                format!("drag item {item:?} is not a base-layer object"),
            );
            return;
        }
        let frame = object.frame;
        if !frame.x.is_finite()
            || !frame.y.is_finite()
            || !frame.w.is_finite()
            || !frame.h.is_finite()
        {
            warn(out, format!("drag item {item:?} has a non-finite frame"));
            return;
        }
        let slide_w = self.course.slide_size.w as f32;
        let slide_h = self.course.slide_size.h as f32;
        let x = x.clamp(0.0, slide_w.max(0.0));
        let y = y.clamp(0.0, slide_h.max(0.0));
        let accepted = target
            .filter(|id| question.drop_targets.iter().any(|t| t == id))
            .map(str::to_owned);
        let correct_target = question
            .drag_items
            .iter()
            .find(|d| d.item == item)
            .map(|d| d.target.clone());
        let snap = question.snap_to_center;
        let return_if_wrong = question.return_if_wrong;
        let one_per_target = question.one_per_target;
        let Some(correct_target) = correct_target else {
            warn(out, format!("drag item {item:?} does not exist"));
            return;
        };

        let Some(target_id) = accepted else {
            self.send_home(item, out);
            return;
        };
        if return_if_wrong && target_id != correct_target {
            self.send_home(item, out);
            return;
        }
        let Some((target_object, target_layer)) = slide.object(&target_id) else {
            warn(
                out,
                format!("drop target {target_id:?} is not on this slide"),
            );
            return;
        };
        if target_layer.is_some() {
            warn(
                out,
                format!("drop target {target_id:?} is not a base-layer object"),
            );
            return;
        }
        let target_frame = target_object.frame;
        if !target_frame.x.is_finite()
            || !target_frame.y.is_finite()
            || !target_frame.w.is_finite()
            || !target_frame.h.is_finite()
        {
            warn(
                out,
                format!("drop target {target_id:?} has a non-finite frame"),
            );
            return;
        }
        let offset = if snap {
            (
                target_frame.x + target_frame.w * 0.5 - frame.w * 0.5 - frame.x,
                target_frame.y + target_frame.h * 0.5 - frame.h * 0.5 - frame.y,
            )
        } else {
            (x - frame.x, y - frame.y)
        };
        if !offset.0.is_finite() || !offset.1.is_finite() {
            warn(out, "drag offset is not finite");
            return;
        }
        let bumped: Vec<String> = if one_per_target {
            self.st
                .drops
                .iter()
                .filter(|(other, sitting)| other.as_str() != item && sitting.as_str() == target_id)
                .map(|(other, _)| other.clone())
                .collect()
        } else {
            Vec::new()
        };
        for other in &bumped {
            self.st.drops.remove(other);
            self.st.object_offsets.remove(other);
        }
        self.st.drops.insert(item.to_owned(), target_id.clone());
        self.st.object_offsets.insert(item.to_owned(), offset);
        out.push(Effect::ItemDropped {
            item: item.to_owned(),
            target: Some(target_id),
            offset,
        });
        for other in bumped {
            out.push(Effect::ItemDropped {
                item: other,
                target: None,
                offset: (0.0, 0.0),
            });
        }
    }

    fn send_home(&mut self, item: &str, out: &mut Vec<Effect>) {
        self.st.drops.remove(item);
        self.st.object_offsets.remove(item);
        out.push(Effect::ItemDropped {
            item: item.to_owned(),
            target: None,
            offset: (0.0, 0.0),
        });
    }

    fn object_dropped(
        &mut self,
        object_id: &str,
        x: f32,
        y: f32,
        q: &mut Queue,
        out: &mut Vec<Effect>,
    ) {
        let Some(slide) = self.slide() else { return };
        let Some((obj, _layer)) = slide.object(object_id) else {
            warn(
                out,
                format!("draggable object {object_id:?} is not on this slide"),
            );
            return;
        };
        let Some(drag_spec) = &obj.drag else {
            warn(out, format!("object {object_id:?} is not draggable"));
            return;
        };
        if !x.is_finite() || !y.is_finite() {
            warn(out, "drop coordinates must be finite");
            return;
        }
        let frame = obj.frame;
        if !frame.x.is_finite()
            || !frame.y.is_finite()
            || !frame.w.is_finite()
            || !frame.h.is_finite()
        {
            warn(
                out,
                format!("draggable object {object_id:?} has a non-finite frame"),
            );
            return;
        }
        if !self.object_is_live(object_id) {
            warn(
                out,
                format!("draggable object {object_id:?} is not on screen"),
            );
            return;
        }
        let slide_w = self.course.slide_size.w as f32;
        let slide_h = self.course.slide_size.h as f32;
        let obj_w = frame.w.max(0.0);
        let obj_h = frame.h.max(0.0);
        let clamped_x = x.clamp(0.0, (slide_w - obj_w).max(0.0));
        let clamped_y = y.clamp(0.0, (slide_h - obj_h).max(0.0));

        let center_x = clamped_x + obj_w * 0.5;
        let center_y = clamped_y + obj_h * 0.5;

        // Find the first listed target whose current frame contains the object's centre.
        let mut hit_target = None;
        for target_id in &drag_spec.targets {
            if let Some(target_bounds) = self.object_current_bounds(target_id)
                && center_x >= target_bounds.x
                && center_x <= target_bounds.x + target_bounds.w
                && center_y >= target_bounds.y
                && center_y <= target_bounds.y + target_bounds.h
            {
                hit_target = Some((target_id.clone(), target_bounds));
                break;
            }
        }

        let previous = self
            .st
            .object_positions
            .get(object_id)
            .copied()
            .unwrap_or((0.0, 0.0));
        if let Some((target_id, target_bounds)) = hit_target {
            let offset = if drag_spec.snap {
                let target_cx = target_bounds.x + target_bounds.w * 0.5;
                let target_cy = target_bounds.y + target_bounds.h * 0.5;
                let orig_cx = frame.x + obj_w * 0.5;
                let orig_cy = frame.y + obj_h * 0.5;
                (target_cx - orig_cx, target_cy - orig_cy)
            } else {
                (clamped_x - frame.x, clamped_y - frame.y)
            };
            self.st
                .object_positions
                .insert(object_id.to_owned(), offset);
            out.push(Effect::ObjectMoved {
                object: object_id.to_owned(),
                offset,
            });
            out.push(Effect::ObjectDropped {
                object: object_id.to_owned(),
                target: target_id.clone(),
            });
            self.push(
                q,
                Event::DroppedOn {
                    object: object_id.to_owned(),
                    target: target_id,
                },
            );
        } else if drag_spec.return_on_miss {
            // Back to where it was before this drag, which may itself be a
            // position from an earlier drop.
            out.push(Effect::ObjectMoved {
                object: object_id.to_owned(),
                offset: previous,
            });
        } else {
            let offset = (clamped_x - frame.x, clamped_y - frame.y);
            self.st
                .object_positions
                .insert(object_id.to_owned(), offset);
            out.push(Effect::ObjectMoved {
                object: object_id.to_owned(),
                offset,
            });
        }
        self.check_intersections(q);
    }

    fn seed_sequence(&mut self, slide: &Slide) {
        let Some(question) = &slide.question else {
            return;
        };
        if question.kind != QuestionKind::Sequence {
            return;
        }
        let ids: Vec<String> = question.choices.iter().map(|c| c.id.clone()).collect();
        let attempt = self
            .st
            .answers
            .get(&slide.id)
            .map(|a| a.attempts)
            .unwrap_or(0);
        self.st.sequence = sequence_order(&ids, seed_for(&slide.id, attempt));
    }

    fn grade(&mut self, q: &mut Queue, out: &mut Vec<Effect>) {
        let Some(slide) = self.slide() else { return };
        let Some(question) = &slide.question else {
            warn(out, "submit_question on a slide without a question");
            return;
        };
        if self.question_locked_on_slide(slide) {
            warn(out, "this question is already answered");
            return;
        }
        let verdict = if question.kind == QuestionKind::Freeform {
            GradeCheck::Graded(
                question
                    .correct_when
                    .as_ref()
                    .is_some_and(|condition| self.eval(condition, 0)),
            )
        } else {
            grade_answer(
                question,
                &self.st.selected,
                &self.st.text_entry,
                &self.st.sequence,
                &self.st.matches,
                &self.st.hotspot_markers,
                &self.st.drops,
            )
        };
        let correct = match verdict {
            GradeCheck::Required => {
                out.push(Effect::AnswerRequired);
                return;
            }
            GradeCheck::Graded(correct) => correct,
        };
        // Capture the submitted answer before any shuffle or reset. The effect
        // stream is unchanged; hosts read it from the player state.
        self.st.last_response = Some(learner_response(
            question,
            correct,
            &self.st.selected,
            &self.st.text_entry,
            &self.st.sequence,
            &self.st.matches,
            &self.st.hotspot_markers,
            &self.st.drops,
        ));
        let kind = question.kind;
        let limit = question.attempts;
        let choice_ids: Vec<String> = question.choices.iter().map(|c| c.id.clone()).collect();
        let slide_id = slide.id.clone();
        let feedback = if correct {
            question.correct_layer.clone()
        } else {
            question.incorrect_layer.clone()
        };
        let prev = self.st.answers.get(&slide_id).map_or(0, |a| a.attempts);
        let attempts = prev.saturating_add(1);
        let points = awarded_points(
            question,
            correct,
            &self.st.selected,
            &self.st.sequence,
            &self.st.matches,
            &self.st.hotspot_markers,
            &self.st.drops,
        );
        self.st.answers.insert(
            slide_id.clone(),
            Answer {
                correct,
                attempts,
                points,
            },
        );
        // Snapshot the submitted answer state for answer review, before any
        // sequence reshuffle below. Re-grading replaces the snapshot.
        self.st.review_answers.insert(
            slide_id.clone(),
            crate::AnswerSnapshot {
                selected: self.st.selected.clone(),
                text_entry: self.st.text_entry.clone(),
                sequence: self.st.sequence.clone(),
                matches: self.st.matches.clone(),
                hotspot_markers: self.st.hotspot_markers.clone(),
                drops: self.st.drops.clone(),
            },
        );
        let attempts_left = (limit != 0).then(|| limit.saturating_sub(attempts));
        out.push(Effect::QuestionAnswered {
            slide: slide_id.clone(),
            correct,
            points,
            attempts_left,
        });
        let still_open = !correct && (limit == 0 || attempts < limit);
        if kind == QuestionKind::Sequence && still_open {
            self.st.sequence = sequence_order(&choice_ids, seed_for(&slide_id, attempts));
        }
        if let Some(layer) = feedback {
            self.show_layer(&layer, q, out);
        }
    }

    fn results(&mut self, out: &mut Vec<Effect>) {
        self.st.quiz_attempts = self.st.quiz_attempts.saturating_add(1);
        let (raw, max) = self.score_points_and_max();
        // No questions at all counts as 100%.
        let percent = raw
            .saturating_mul(100)
            .checked_div(max)
            .map_or(100, |p| p.min(100));
        let percent = u8::try_from(percent).unwrap_or(100);
        let status = match self.course.quiz {
            Some(qz) if percent >= qz.passing_percent => CompletionStatus::Passed,
            Some(_) => CompletionStatus::Failed,
            None => CompletionStatus::Completed,
        };
        let clamp = |v: u64| u32::try_from(v).unwrap_or(u32::MAX);
        out.push(Effect::Score {
            raw: clamp(raw),
            max: clamp(max),
            percent,
            status,
        });
        out.push(Effect::Completion { status });
    }

    /// First play-order slide with a question that has been answered.
    fn first_answered_question(&self) -> Option<usize> {
        self.slides
            .iter()
            .position(|s| s.question.is_some() && self.st.answers.contains_key(&s.id))
    }

    fn retry_quiz(&mut self, q: &mut Queue, out: &mut Vec<Effect>) {
        // Check first: with no question anywhere, warn and do nothing.
        if !self.slides.iter().any(|s| s.question.is_some()) {
            warn(out, "retry_quiz on a course without any question");
            return;
        }
        let was_reviewing = self.st.reviewing_from.is_some();
        self.st.reviewing_from = None;
        self.st.answers.clear();
        self.st.review_answers.clear();
        self.st.selected.clear();
        self.st.text_entry.clear();
        self.st.sequence.clear();
        self.st.matches.clear();
        self.st.hotspot_markers.clear();
        self.st.drops.clear();
        self.st.last_response = None;
        // Saved slide state for question slides is forgotten.
        let question_ids: Vec<String> = self
            .slides
            .iter()
            .filter(|s| s.question.is_some())
            .map(|s| s.id.clone())
            .collect();
        for id in &question_ids {
            self.st.saved_slides.remove(id);
        }
        if !self.course.banks.is_empty() {
            let new_seed = seed_for(&self.st.draw_seed.to_string(), self.st.quiz_attempts) as u32;
            self.st.draw_seed = new_seed;
            self.slides = crate::draw::play_order(self.course, new_seed);
        }
        if was_reviewing {
            out.push(Effect::ReviewChanged { reviewing: false });
        }
        if let Some(idx) = self.slides.iter().position(|s| s.question.is_some()) {
            self.enter_slide(idx, q, out);
        } else {
            warn(out, "retry_quiz on a course without any question");
        }
    }

    fn review_quiz(&mut self, q: &mut Queue, out: &mut Vec<Effect>) {
        if self.st.reviewing_from.is_some() {
            warn(out, "already reviewing answers");
            return;
        }
        let Some(idx) = self.first_answered_question() else {
            warn(out, "review_quiz with nothing answered");
            return;
        };
        let Some(current) = self.slide() else {
            warn(out, "review_quiz with no slide showing");
            return;
        };
        self.st.reviewing_from = Some(current.id.clone());
        self.enter_slide(idx, q, out);
        out.push(Effect::ReviewChanged { reviewing: true });
    }

    fn exit_review(&mut self, q: &mut Queue, out: &mut Vec<Effect>) {
        let Some(from) = self.st.reviewing_from.take() else {
            warn(out, "exit_review while not reviewing");
            return;
        };
        let idx = self.slide_position(&from);
        out.push(Effect::ReviewChanged { reviewing: false });
        match idx {
            Some(idx) => self.enter_slide(idx, q, out),
            None => warn(out, format!("review return slide {from:?} does not exist")),
        }
    }

    /// Player Next/Previous while reviewing. Returns true when handled.
    fn review_nav(&mut self, forward: bool, q: &mut Queue, out: &mut Vec<Effect>) -> bool {
        if self.st.reviewing_from.is_none() {
            return false;
        }
        let cur = self.st.slide;
        let target = if forward {
            (cur.saturating_add(1)..self.slides.len()).find(|&i| {
                self.slides
                    .get(i)
                    .is_some_and(|s| s.question.is_some() && self.st.answers.contains_key(&s.id))
            })
        } else {
            (0..cur).rev().find(|&i| {
                self.slides
                    .get(i)
                    .is_some_and(|s| s.question.is_some() && self.st.answers.contains_key(&s.id))
            })
        };
        match target {
            Some(idx) => {
                self.enter_slide(idx, q, out);
            }
            None if forward => {
                self.exit_review(q, out);
            }
            None => {}
        }
        true
    }
}

const TEXT_CAP: usize = 2000;
const SHORT_ANSWER_CAP: usize = 250;

fn is_choice_kind(kind: QuestionKind) -> bool {
    matches!(
        kind,
        QuestionKind::MultipleChoice
            | QuestionKind::MultipleResponse
            | QuestionKind::TrueFalse
            | QuestionKind::Likert
    )
}

fn cap_chars(text: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for (i, ch) in text.chars().enumerate() {
        if i >= max_chars {
            break;
        }
        out.push(ch);
    }
    out
}

/// FNV-1a 64 over the slide id's UTF-8 bytes followed by `attempt` as little-endian `u32`.
pub(crate) fn seed_for(slide_id: &str, attempt: u32) -> u64 {
    const OFFSET: u64 = 14_695_981_039_346_656_037;
    const PRIME: u64 = 1_099_511_628_211;
    let mut hash = OFFSET;
    for b in slide_id.as_bytes() {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(PRIME);
    }
    for b in attempt.to_le_bytes() {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

struct SplitMix64(u64);

impl SplitMix64 {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

/// Fisher–Yates shuffle. `ids` stays in its original order; the result is a new vec.
pub(crate) fn shuffled_ids(ids: &[String], seed: u64) -> Vec<String> {
    let mut order = ids.to_vec();
    let n = order.len();
    if n < 2 {
        return order;
    }
    let mut rng = SplitMix64(seed);
    for i in (1..n).rev() {
        let span = (i as u64).saturating_add(1);
        let j = usize::try_from(rng.next_u64() % span.max(1)).unwrap_or(i);
        if j < order.len() && i < order.len() {
            order.swap(i, j);
        }
    }
    order
}

/// Shuffled sequence order that is never the authored order when there are at least two items.
pub(crate) fn sequence_order(ids: &[String], seed: u64) -> Vec<String> {
    let mut order = shuffled_ids(ids, seed);
    if order.len() >= 2
        && order == ids
        && let Some(first) = order.first().cloned()
    {
        order.remove(0);
        order.push(first);
    }
    order
}

fn move_item(order: &mut Vec<String>, choice: &str, to: usize) {
    let Some(from) = order.iter().position(|id| id == choice) else {
        return;
    };
    let Some(id) = order.get(from).cloned() else {
        return;
    };
    order.remove(from);
    let dest = to.min(order.len());
    order.insert(dest, id);
}

enum GradeCheck {
    Required,
    Graded(bool),
}

fn answers_equal(entry: &str, accepted: &str, case_sensitive: bool) -> bool {
    if case_sensitive {
        entry == accepted
    } else {
        entry.to_lowercase() == accepted.to_lowercase()
    }
}

fn point_in_sampled_frame(
    frame: coursecraft_course::Rect,
    offset: (f32, f32),
    scale: f32,
    x: f32,
    y: f32,
) -> bool {
    if !x.is_finite()
        || !y.is_finite()
        || !frame.x.is_finite()
        || !frame.y.is_finite()
        || !frame.w.is_finite()
        || !frame.h.is_finite()
        || !offset.0.is_finite()
        || !offset.1.is_finite()
        || !scale.is_finite()
        || frame.w < 0.0
        || frame.h < 0.0
        || scale < 0.0
    {
        return false;
    }
    let cx = frame.x + offset.0 + frame.w * 0.5;
    let cy = frame.y + offset.1 + frame.h * 0.5;
    let hw = (frame.w * scale * 0.5).abs();
    let hh = (frame.h * scale * 0.5).abs();
    x >= cx - hw && x <= cx + hw && y >= cy - hh && y <= cy + hh
}

fn point_in_region(region: &coursecraft_course::HotspotRegion, px: f32, py: f32) -> bool {
    if !px.is_finite()
        || !py.is_finite()
        || !region.x.is_finite()
        || !region.y.is_finite()
        || !region.w.is_finite()
        || !region.h.is_finite()
    {
        return false;
    }
    match region.shape {
        coursecraft_course::HotspotShape::Rect => {
            px >= region.x
                && px <= region.x + region.w
                && py >= region.y
                && py <= region.y + region.h
        }
        coursecraft_course::HotspotShape::Ellipse => {
            let rx = region.w * 0.5;
            let ry = region.h * 0.5;
            if rx <= 0.0 || ry <= 0.0 {
                return false;
            }
            let nx = (px - (region.x + rx)) / rx;
            let ny = (py - (region.y + ry)) / ry;
            nx * nx + ny * ny <= 1.0
        }
    }
}

/// Snapshot what the learner submitted, in the course's authored order where
/// one exists. Called only for a graded answer (never for `AnswerRequired`).
#[allow(clippy::too_many_arguments)]
fn learner_response(
    question: &coursecraft_course::Question,
    correct: bool,
    selected: &std::collections::BTreeSet<String>,
    text_entry: &str,
    sequence: &[String],
    matches: &std::collections::BTreeMap<String, String>,
    markers: &[(f32, f32)],
    drops: &std::collections::BTreeMap<String, String>,
) -> LearnerResponse {
    match question.kind {
        QuestionKind::MultipleChoice
        | QuestionKind::MultipleResponse
        | QuestionKind::TrueFalse
        | QuestionKind::Likert => LearnerResponse::Choices(
            question
                .choices
                .iter()
                .filter(|c| selected.contains(&c.id))
                .map(|c| c.id.clone())
                .collect(),
        ),
        QuestionKind::FillIn | QuestionKind::ShortAnswer | QuestionKind::Essay => {
            LearnerResponse::Text(text_entry.trim().to_owned())
        }
        QuestionKind::Sequence => LearnerResponse::Sequence(sequence.to_vec()),
        QuestionKind::Matching => LearnerResponse::Pairs(
            question
                .pairs
                .iter()
                .filter_map(|p| {
                    matches
                        .get(&p.id)
                        .map(|right| (p.id.clone(), right.clone()))
                })
                .collect(),
        ),
        QuestionKind::Hotspot => {
            let mut regions = Vec::new();
            for region in &question.regions {
                let hit = markers.iter().any(|(x, y)| point_in_region(region, *x, *y));
                if hit && !regions.contains(&region.id) {
                    regions.push(region.id.clone());
                }
            }
            LearnerResponse::Regions(regions)
        }
        QuestionKind::DragDrop => LearnerResponse::Pairs(
            question
                .drag_items
                .iter()
                .filter_map(|d| {
                    drops
                        .get(&d.item)
                        .map(|target| (d.item.clone(), target.clone()))
                })
                .collect(),
        ),
        QuestionKind::Freeform => {
            LearnerResponse::Text(if correct { "correct" } else { "incorrect" }.to_owned())
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn awarded_points(
    question: &coursecraft_course::Question,
    correct: bool,
    selected: &std::collections::BTreeSet<String>,
    sequence: &[String],
    matches: &std::collections::BTreeMap<String, String>,
    markers: &[(f32, f32)],
    drops: &std::collections::BTreeMap<String, String>,
) -> u32 {
    if question.is_survey() {
        return 0;
    }
    match question.scoring {
        Scoring::AllOrNothing => {
            if correct {
                question.points
            } else {
                0
            }
        }
        Scoring::PerAnswer => {
            let mut total: i64 = 0;
            for id in selected {
                if let Some(choice) = question.choices.iter().find(|c| c.id == *id) {
                    total = total.saturating_add(i64::from(choice.points));
                }
            }
            let max = i64::from(question.max_points());
            let clamped = total.clamp(0, max);
            u32::try_from(clamped).unwrap_or(0)
        }
        Scoring::Partial => {
            if correct {
                return question.points;
            }
            let (right_raw, total_raw) = match question.kind {
                QuestionKind::MultipleResponse => {
                    let selected_correct = question
                        .choices
                        .iter()
                        .filter(|c| c.correct && selected.contains(&c.id))
                        .count() as i64;
                    let selected_wrong = question
                        .choices
                        .iter()
                        .filter(|c| !c.correct && selected.contains(&c.id))
                        .count() as i64;
                    let right = selected_correct.saturating_sub(selected_wrong);
                    let total = question.choices.iter().filter(|c| c.correct).count() as i64;
                    (right, total)
                }
                QuestionKind::Sequence => {
                    let authored: Vec<&str> =
                        question.choices.iter().map(|c| c.id.as_str()).collect();
                    let mut right: i64 = 0;
                    for (i, id) in sequence.iter().enumerate() {
                        if authored.get(i).copied() == Some(id.as_str()) {
                            right = right.saturating_add(1);
                        }
                    }
                    let total = question.choices.len() as i64;
                    (right, total)
                }
                QuestionKind::Matching => {
                    let mut right: i64 = 0;
                    for p in &question.pairs {
                        if matches.get(p.id.as_str()).map(String::as_str) == Some(p.id.as_str()) {
                            right = right.saturating_add(1);
                        }
                    }
                    let total = question.pairs.len() as i64;
                    (right, total)
                }
                QuestionKind::Hotspot => {
                    let mut right: i64 = 0;
                    for (x, y) in markers {
                        let inside = question
                            .regions
                            .iter()
                            .any(|r| r.correct && point_in_region(r, *x, *y));
                        if inside {
                            right = right.saturating_add(1);
                        }
                    }
                    let total = markers.len() as i64;
                    (right, total)
                }
                QuestionKind::DragDrop => {
                    let mut right: i64 = 0;
                    for drag in &question.drag_items {
                        if drops.get(&drag.item).map(String::as_str) == Some(drag.target.as_str()) {
                            right = right.saturating_add(1);
                        }
                    }
                    let total = question.drag_items.len() as i64;
                    (right, total)
                }
                _ => (0, 0),
            };
            let right = right_raw.max(0);
            if total_raw <= 0 {
                return 0;
            }
            let points = i64::from(question.points);
            let result = points
                .saturating_mul(right)
                .checked_div(total_raw)
                .unwrap_or(0);
            let result = result.max(0);
            u32::try_from(result).unwrap_or(u32::MAX)
        }
    }
}

fn grade_answer(
    question: &coursecraft_course::Question,
    selected: &std::collections::BTreeSet<String>,
    text_entry: &str,
    sequence: &[String],
    matches: &std::collections::BTreeMap<String, String>,
    markers: &[(f32, f32)],
    drops: &std::collections::BTreeMap<String, String>,
) -> GradeCheck {
    match question.kind {
        QuestionKind::MultipleChoice | QuestionKind::MultipleResponse | QuestionKind::TrueFalse => {
            if selected.is_empty() {
                GradeCheck::Required
            } else {
                let correct = question
                    .choices
                    .iter()
                    .all(|c| c.correct == selected.contains(&c.id));
                GradeCheck::Graded(correct)
            }
        }
        QuestionKind::FillIn => {
            let entry = text_entry.trim();
            if entry.is_empty() {
                GradeCheck::Required
            } else {
                let correct = question
                    .accepted
                    .iter()
                    .any(|answer| answers_equal(entry, answer.trim(), question.case_sensitive));
                GradeCheck::Graded(correct)
            }
        }
        QuestionKind::Sequence => {
            let authored: Vec<&str> = question.choices.iter().map(|c| c.id.as_str()).collect();
            let current: Vec<&str> = sequence.iter().map(String::as_str).collect();
            GradeCheck::Graded(authored == current)
        }
        QuestionKind::Matching => {
            let complete = question
                .pairs
                .iter()
                .all(|p| matches.contains_key(p.id.as_str()));
            if !complete {
                GradeCheck::Required
            } else {
                let correct = question
                    .pairs
                    .iter()
                    .all(|p| matches.get(p.id.as_str()).map(String::as_str) == Some(p.id.as_str()));
                GradeCheck::Graded(correct)
            }
        }
        QuestionKind::Hotspot => {
            if markers.is_empty() {
                GradeCheck::Required
            } else {
                let correct = markers.iter().all(|(x, y)| {
                    question
                        .regions
                        .iter()
                        .any(|region| region.correct && point_in_region(region, *x, *y))
                });
                GradeCheck::Graded(correct)
            }
        }
        QuestionKind::DragDrop => {
            if drops.is_empty() {
                GradeCheck::Required
            } else {
                let correct = question.drag_items.iter().all(|drag| {
                    drops.get(&drag.item).map(String::as_str) == Some(drag.target.as_str())
                });
                GradeCheck::Graded(correct)
            }
        }
        QuestionKind::Likert => {
            if selected.is_empty() {
                GradeCheck::Required
            } else {
                GradeCheck::Graded(true)
            }
        }
        QuestionKind::ShortAnswer | QuestionKind::Essay => {
            if text_entry.trim().is_empty() {
                GradeCheck::Required
            } else {
                GradeCheck::Graded(true)
            }
        }
        // Graded in `grade`, which has the player state the condition needs.
        QuestionKind::Freeform => GradeCheck::Graded(false),
    }
}

fn compare(cur: &Value, op: CmpOp, rhs: &Value) -> bool {
    match (cur, rhs) {
        (Value::Number(a), Value::Number(b)) => match op {
            CmpOp::Eq => a == b,
            CmpOp::Ne => a != b,
            CmpOp::Lt => a < b,
            CmpOp::Le => a <= b,
            CmpOp::Gt => a > b,
            CmpOp::Ge => a >= b,
            CmpOp::Contains => false,
        },
        (Value::Text(a), Value::Text(b)) => match op {
            CmpOp::Eq => a == b,
            CmpOp::Ne => a != b,
            CmpOp::Contains => a.contains(b.as_str()),
            _ => false,
        },
        (Value::Bool(a), Value::Bool(b)) => match op {
            CmpOp::Eq => a == b,
            CmpOp::Ne => a != b,
            _ => false,
        },
        _ => false,
    }
}
