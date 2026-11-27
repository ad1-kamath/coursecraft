//! Triggers bottom panel and Trigger Editor dialog.

use crate::dialog::Dialog;
use coursecraft_course::{
    Action, BACKGROUND_AUDIO_ID, BUILTIN_STATES, CmpOp, CompletionStatus, Condition, Course, Event,
    Slide, SlideTarget, Trigger, Value, ValueKind, ValueSource, VarOp,
};
use egui::{DragValue, RichText, ScrollArea, TextEdit, Ui};
use serde_json::Value as Json;

/// Helper to get the display name of an object or interactive group
/// (its name if non-empty, otherwise its id).
#[must_use]
pub fn object_name(slide: &Slide, obj_id: &str) -> String {
    for obj in &slide.objects {
        if obj.id == obj_id {
            if !obj.name.trim().is_empty() {
                return obj.name.clone();
            }
            return obj.id.clone();
        }
    }
    for layer in &slide.layers {
        for obj in &layer.objects {
            if obj.id == obj_id {
                if !obj.name.trim().is_empty() {
                    return obj.name.clone();
                }
                return obj.id.clone();
            }
        }
    }
    if let Some((group, _)) = slide.interactive_group(obj_id) {
        if !group.name.trim().is_empty() {
            return group.name.clone();
        }
        return group.id.clone();
    }
    obj_id.to_owned()
}

/// Every object on the slide, base layer first then layers in authored order.
fn all_objects(slide: &Slide) -> Vec<&coursecraft_course::Object> {
    slide
        .objects
        .iter()
        .chain(slide.layers.iter().flat_map(|layer| layer.objects.iter()))
        .collect()
}

/// Every interactive group on the slide, base layer first then layers in authored order.
fn all_interactive_groups(slide: &Slide) -> Vec<&coursecraft_course::Group> {
    slide
        .groups
        .iter()
        .chain(slide.layers.iter().flat_map(|layer| layer.groups.iter()))
        .filter(|g| g.interactive)
        .collect()
}

/// Default trigger target for the current editor selection: when a single
/// complete interactive group is selected, target that group id; otherwise
/// target the primary selected object.
#[must_use]
pub(crate) fn default_trigger_target(
    selection: &crate::Selection,
    course: &Course,
    slide: &Slide,
) -> Option<String> {
    if selection.is_one_group(course)
        && let Some(primary) = selection.object.as_deref()
        && let Some((obj, _)) = slide.object(primary)
        && let Some(group_id) = obj.group.as_deref()
        && slide.interactive_group(group_id).is_some()
    {
        return Some(group_id.to_owned());
    }
    selection.object.clone()
}

/// States available for a trigger target (object or interactive group).
fn available_states_for_target(slide: &Slide, target_id: &str) -> Vec<String> {
    if slide.interactive_group(target_id).is_some() {
        let mut states = vec![
            "normal".to_owned(),
            "hidden".to_owned(),
            "disabled".to_owned(),
        ];
        let mut extra: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for member in slide.group_members(target_id) {
            for s in &member.states {
                if !matches!(s.name.as_str(), "normal" | "hidden" | "disabled") {
                    extra.insert(s.name.clone());
                }
            }
        }
        states.extend(extra);
        return states;
    }
    let mut available_states: Vec<String> =
        BUILTIN_STATES.iter().map(|s| (*s).to_owned()).collect();
    if let Some((obj, _)) = slide.object(target_id) {
        for s in &obj.states {
            if !available_states.contains(&s.name) {
                available_states.push(s.name.clone());
            }
        }
    }
    available_states
}

/// Display name used inside readable trigger descriptions: uses `object_label`
/// for interactive groups so button groups stay distinct from plain objects.
fn target_display_name(slide: &Slide, target_id: &str) -> String {
    if slide.interactive_group(target_id).is_some() {
        object_label(slide, target_id)
    } else {
        object_name(slide, target_id)
    }
}

/// Maximum popup height for long combo lists: most of the window, so every entry is reachable
/// without relying on scrolling (the popup still scrolls when the window is smaller).
#[must_use]
pub fn combo_popup_height(ctx: &egui::Context) -> f32 {
    (ctx.content_rect().height() * 0.8).max(200.0)
}

/// A [`egui::ComboBox`] whose popup is tall enough to show long lists in full and has a minimum width.
fn tall_combo<'a>(ui: &Ui, id_salt: impl egui::AsIdSalt) -> egui::ComboBox<'a> {
    crate::widgets::styled_combo(ui, id_salt, 140.0)
}

/// Display name of a layer on `slide` (its name if non-empty, otherwise its id).
#[must_use]
pub fn layer_name(slide: &Slide, layer_id: &str) -> String {
    match slide.layer(layer_id) {
        Some(l) if !l.name.trim().is_empty() => l.name.clone(),
        _ => layer_id.to_owned(),
    }
}

/// Picker label for an object or interactive group: its name, plus ` (layer name)`
/// for objects on a layer, or ` (button group)` / ` (button group, layer name)` for
/// interactive groups. Used for both the open list and the closed combo so they
/// always agree.
#[must_use]
pub fn object_label(slide: &Slide, obj_id: &str) -> String {
    if let Some((group, owner)) = slide.interactive_group(obj_id) {
        let base = if !group.name.trim().is_empty() {
            group.name.as_str()
        } else {
            group.id.as_str()
        };
        return match owner {
            Some(layer_id) => format!("{base} (button group, {})", layer_name(slide, layer_id)),
            None => format!("{base} (button group)"),
        };
    }
    for l in &slide.layers {
        if l.objects.iter().any(|o| o.id == obj_id) {
            return format!(
                "{} ({})",
                object_name(slide, obj_id),
                layer_name(slide, &l.id)
            );
        }
    }
    object_name(slide, obj_id)
}

/// Picker label for a media object, including the reserved background track.
#[must_use]
pub fn media_object_label(slide: &Slide, obj_id: &str) -> String {
    if obj_id == BACKGROUND_AUDIO_ID {
        "Background audio".to_owned()
    } else {
        object_label(slide, obj_id)
    }
}

/// Display name for a media object, including the reserved background track.
fn media_object_name(slide: &Slide, obj_id: &str) -> String {
    if obj_id == BACKGROUND_AUDIO_ID {
        "background audio".to_owned()
    } else {
        object_name(slide, obj_id)
    }
}

/// Picker label for a slide: `title (id)`, or the id alone when the slide is unknown.
#[must_use]
pub fn slide_label(course: &Course, slide_id: &str) -> String {
    match course.slide(slide_id) {
        Some(sl) => format!("{} ({})", sl.title, sl.id),
        None => slide_id.to_owned(),
    }
}

/// Describe a Value for display.
#[must_use]
pub fn describe_value(value: &Value) -> String {
    match value {
        Value::Bool(b) => format!("{b}"),
        Value::Number(n) => {
            if n.fract() == 0.0 && n.abs() < 1e15 {
                format!("{}", *n as i64)
            } else {
                format!("{n}")
            }
        }
        Value::Text(t) => format!("\"{t}\""),
    }
}

#[must_use]
pub fn describe_value_source(source: &ValueSource) -> String {
    match source {
        ValueSource::Constant(v) => describe_value(v),
        ValueSource::Variable { variable } => variable.clone(),
    }
}

/// Describe an Action as a human-readable sentence fragment.
#[must_use]
pub fn describe_action(action: &Action, slide: &Slide) -> String {
    match action {
        Action::JumpToSlide { target } => match target {
            SlideTarget::Next => "Jump to next slide".to_owned(),
            SlideTarget::Previous => "Jump to previous slide".to_owned(),
            SlideTarget::LastVisited => "Jump to last visited slide".to_owned(),
            SlideTarget::Id(sid) => format!("Jump to slide {sid}"),
            SlideTarget::Scene(sid) => format!("Jump to scene {sid}"),
        },
        Action::ShowLayer { layer } => format!("Show layer {}", layer_name(slide, layer)),
        Action::HideLayer { layer } => match layer {
            Some(l) => format!("Hide layer {}", layer_name(slide, l)),
            None => "Hide this layer".to_owned(),
        },
        Action::ChangeState { object, state } => {
            format!(
                "Set {} state to {state}",
                target_display_name(slide, object)
            )
        }
        Action::SetVariable {
            variable,
            op,
            value,
        } => match op {
            VarOp::Assign => match value {
                Some(v) => format!("Set {variable} = {}", describe_value_source(v)),
                None => format!("Set {variable}"),
            },
            VarOp::Add => match value {
                Some(v) => format!("Set {variable} + {}", describe_value_source(v)),
                None => format!("Set {variable} + 1"),
            },
            VarOp::Subtract => match value {
                Some(v) => format!("Set {variable} - {}", describe_value_source(v)),
                None => format!("Set {variable} - 1"),
            },
            VarOp::Multiply => match value {
                Some(v) => format!("Set {variable} * {}", describe_value_source(v)),
                None => format!("Set {variable} * 1"),
            },
            VarOp::Divide => match value {
                Some(v) => format!("Set {variable} / {}", describe_value_source(v)),
                None => format!("Set {variable} / 1"),
            },
            VarOp::Toggle => format!("Toggle {variable}"),
        },
        Action::PlayMedia { object } => format!("Play media {}", media_object_name(slide, object)),
        Action::PauseMedia { object } => {
            format!("Pause media {}", media_object_name(slide, object))
        }
        Action::StopMedia { object } => format!("Stop media {}", media_object_name(slide, object)),
        Action::PauseTimeline => "Pause timeline".to_owned(),
        Action::ResumeTimeline => "Resume timeline".to_owned(),
        Action::SubmitQuestion => "Submit question".to_owned(),
        Action::SubmitResults => "Submit results".to_owned(),
        Action::RetryQuiz => "Retry quiz".to_owned(),
        Action::ReviewQuiz => "Review quiz".to_owned(),
        Action::ExitReview => "Exit review".to_owned(),
        Action::PrintSlide => "Print slide".to_owned(),
        Action::CompleteCourse { status } => {
            let s = match status {
                CompletionStatus::Passed => "passed",
                CompletionStatus::Failed => "failed",
                CompletionStatus::Completed => "completed",
                CompletionStatus::Incomplete => "incomplete",
            };
            format!("Complete course with status {s}")
        }
        Action::ExitCourse => "Exit course".to_owned(),
        Action::OpenUrl { url } => format!("Open URL {url}"),
        Action::MoveOnPath { object, path } => {
            format!("Move {} along {path}", object_name(slide, object))
        }
        Action::RunJavaScript { code } => {
            let first = code.lines().next().unwrap_or_default().trim();
            if first.is_empty() {
                "Run JavaScript".to_owned()
            } else {
                format!("Run JavaScript: {first}")
            }
        }
        Action::LightboxSlide { slide: target } => {
            format!("Show slide {target} in a lightbox")
        }
        Action::CloseLightbox => "Close lightbox".to_owned(),
        Action::StartTimer { timer } => format!("Start timer {}", object_name(slide, timer)),
        Action::PauseTimer { timer } => format!("Pause timer {}", object_name(slide, timer)),
        Action::ResetTimer { timer } => format!("Reset timer {}", object_name(slide, timer)),
    }
}

/// Describe an Event as a human-readable sentence fragment.
#[must_use]
pub fn describe_event(event: &Event, slide: &Slide) -> String {
    match event {
        Event::TimelineStarts { layer } => match layer {
            Some(l) => format!("timeline starts on layer {}", layer_name(slide, l)),
            None => "the timeline starts".to_owned(),
        },
        Event::TimelineEnds { layer } => match layer {
            Some(l) => format!("timeline ends on layer {}", layer_name(slide, l)),
            None => "the timeline ends".to_owned(),
        },
        Event::TimelineReaches { ms, layer, cue } => {
            let cue_label = cue.as_deref().map(|cid| {
                let cues = match layer.as_deref() {
                    Some(l) => slide.layer(l).map_or(&[][..], |l| l.cue_points.as_slice()),
                    None => slide.cue_points.as_slice(),
                };
                cues.iter()
                    .find(|c| c.id == cid)
                    .map_or_else(|| cid.to_owned(), |c| format!("cue \"{}\"", c.name))
            });
            match (layer, cue_label) {
                (Some(l), Some(c)) => {
                    format!("timeline reaches {c} on layer {}", layer_name(slide, l))
                }
                (None, Some(c)) => format!("timeline reaches {c}"),
                (Some(l), None) => {
                    format!("timeline reaches {ms}ms on layer {}", layer_name(slide, l))
                }
                (None, None) => format!("timeline reaches {ms}ms"),
            }
        }
        Event::Click { object } => {
            format!("the user clicks {}", target_display_name(slide, object))
        }
        Event::HoverStart { object } => {
            format!(
                "the user hovers over {}",
                target_display_name(slide, object)
            )
        }
        Event::HoverEnd { object } => {
            format!(
                "the user stops hovering over {}",
                target_display_name(slide, object)
            )
        }
        Event::VariableChanges { variable } => format!("{variable} changes"),
        Event::StateChanges { object, to } => match to {
            Some(st) => format!(
                "{} state changes to {st}",
                target_display_name(slide, object)
            ),
            None => format!("{} state changes", target_display_name(slide, object)),
        },
        Event::MediaEnds { object } => {
            format!("media ends on {}", media_object_name(slide, object))
        }
        Event::CueReached { object, cue } => {
            format!("cue {cue} is reached on {}", object_name(slide, object))
        }
        Event::KeyPressed { key, object } => match object {
            Some(obj) => format!(
                "the user presses key '{key}' on {}",
                target_display_name(slide, obj)
            ),
            None => format!("the user presses key '{key}'"),
        },
        Event::DroppedOn { object, target } => format!(
            "{} is dropped on {}",
            object_name(slide, object),
            object_name(slide, target)
        ),
        Event::ObjectsIntersect { a, b } => format!(
            "{} intersects {}",
            object_name(slide, a),
            object_name(slide, b)
        ),
        Event::ObjectsStopIntersecting { a, b } => format!(
            "{} stops intersecting {}",
            object_name(slide, a),
            object_name(slide, b)
        ),
        Event::Submit => "the user submits".to_owned(),
        Event::AnimationCompletes { object, path } => match path {
            Some(p) => format!("{} finishes moving along {p}", object_name(slide, object)),
            None => format!("{}'s entrance finishes", object_name(slide, object)),
        },
        Event::MarkerOpened { object } => {
            format!("{} opens", object_name(slide, object))
        }
        Event::MarkerClosed { object } => {
            format!("{} closes", object_name(slide, object))
        }
        Event::TimerEnds { timer } => format!("timer {} ends", object_name(slide, timer)),
    }
}

/// Describe a Condition as a human-readable sentence fragment.
#[must_use]
pub fn describe_condition(cond: &Condition, slide: &Slide) -> String {
    match cond {
        Condition::Compare {
            variable,
            op,
            value,
        } => {
            let op_str = match op {
                CmpOp::Eq => "equals",
                CmpOp::Ne => "does not equal",
                CmpOp::Lt => "<",
                CmpOp::Le => "<=",
                CmpOp::Gt => ">",
                CmpOp::Ge => ">=",
                CmpOp::Contains => "contains",
                CmpOp::EqIgnoreCase => "equals (ignore case)",
                CmpOp::ContainsIgnoreCase => "contains (ignore case)",
            };
            format!("{variable} {op_str} {}", describe_value_source(value))
        }
        Condition::ObjectState { object, state, is } => {
            let obj = target_display_name(slide, object);
            if *is {
                format!("{obj} is {state}")
            } else {
                format!("{obj} is not {state}")
            }
        }
        Condition::All(list) => {
            if list.is_empty() {
                "all()".to_owned()
            } else {
                list.iter()
                    .map(|c| describe_condition(c, slide))
                    .collect::<Vec<_>>()
                    .join(" and ")
            }
        }
        Condition::Any(list) => {
            if list.is_empty() {
                "any()".to_owned()
            } else {
                list.iter()
                    .map(|c| describe_condition(c, slide))
                    .collect::<Vec<_>>()
                    .join(" or ")
            }
        }
        Condition::Not(inner) => format!("not ({})", describe_condition(inner, slide)),
    }
}

/// Describe a Trigger as a readable sentence.
#[must_use]
pub fn describe(trigger: &Trigger, slide: &Slide) -> String {
    let act = describe_action(&trigger.action, slide);
    let when = describe_event(&trigger.when, slide);
    let mut s = format!("{act} when {when}");
    if let Some(ref cond) = trigger.condition {
        s.push_str(" if ");
        s.push_str(&describe_condition(cond, slide));
    }
    s
}

/// State for the Triggers panel.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TriggersState {
    pub last_synced_revision: crate::SyncRevision,
    /// Case-insensitive substring filter over trigger description text.
    pub filter: String,
    /// When true, group triggers under their event object's name.
    pub group_by_object: bool,
    /// Selection key (slide, layer, target) the list last scrolled to. Set only
    /// when the panel is drawn, so a hidden panel never scrolls.
    pub last_scroll_key: Option<String>,
    /// Highlight of each drawn row (by trigger index) in the last frame.
    pub row_highlights: std::collections::HashMap<usize, RowHighlight>,
    /// The group header drawn highlighted in the last frame (Group by object).
    pub highlighted_header: Option<String>,
}

/// How a trigger row relates to the selected object or interactive group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowHighlight {
    /// The row's event object is the selection: strong tint and accent bar.
    Own,
    /// The row's actions or conditions (or a second event object) name the selection.
    Related,
}

/// Whether `trigger` belongs to `target` or only names it.
fn row_highlight(trigger: &Trigger, target: &str) -> Option<RowHighlight> {
    if event_object_id(&trigger.when) == Some(target) {
        return Some(RowHighlight::Own);
    }
    let second_event = match &trigger.when {
        Event::DroppedOn { target: t, .. } => t == target,
        Event::ObjectsIntersect { b, .. } | Event::ObjectsStopIntersecting { b, .. } => b == target,
        _ => false,
    };
    let action = match &trigger.action {
        Action::ChangeState { object, .. }
        | Action::PlayMedia { object }
        | Action::PauseMedia { object }
        | Action::StopMedia { object }
        | Action::MoveOnPath { object, .. } => object == target,
        Action::StartTimer { timer }
        | Action::PauseTimer { timer }
        | Action::ResetTimer { timer } => timer == target,
        _ => false,
    };
    let condition = trigger
        .condition
        .as_ref()
        .is_some_and(|c| condition_names(c, target));
    (second_event || action || condition).then_some(RowHighlight::Related)
}

fn condition_names(condition: &Condition, target: &str) -> bool {
    match condition {
        Condition::All(list) | Condition::Any(list) => {
            list.iter().any(|inner| condition_names(inner, target))
        }
        Condition::Not(inner) => condition_names(inner, target),
        Condition::ObjectState { object, .. } => object == target,
        Condition::Compare { .. } => false,
    }
}

/// Tree editor node for editing conditions.
#[derive(Debug, Clone, PartialEq)]
pub enum ConditionNode {
    Compare {
        variable: String,
        op: CmpOp,
        value: ValueSource,
    },
    ObjectState {
        object: String,
        state: String,
        is: bool,
    },
    All(Vec<ConditionNode>),
    Any(Vec<ConditionNode>),
    Not(Box<ConditionNode>),
}

/// Kind selector for a condition node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConditionKind {
    Compare,
    ObjectState,
    All,
    Any,
    Not,
}

impl ConditionKind {
    /// Every kind, in menu order.
    pub const ALL: [Self; 5] = [
        Self::Compare,
        Self::ObjectState,
        Self::All,
        Self::Any,
        Self::Not,
    ];

    /// Menu label.
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::Compare => "Compare variable",
            Self::ObjectState => "Object state",
            Self::All => "All of",
            Self::Any => "Any of",
            Self::Not => "Not",
        }
    }
}

impl ConditionNode {
    /// The kind of this node.
    #[must_use]
    pub fn kind(&self) -> ConditionKind {
        match self {
            Self::Compare { .. } => ConditionKind::Compare,
            Self::ObjectState { .. } => ConditionKind::ObjectState,
            Self::All(_) => ConditionKind::All,
            Self::Any(_) => ConditionKind::Any,
            Self::Not(_) => ConditionKind::Not,
        }
    }

    /// Convert this node to another kind with sensible defaults: comparisons start on the first
    /// variable, object states on the first object, and groups/`Not` wrap the previous node.
    #[must_use]
    pub fn convert_to(&self, kind: ConditionKind, slide: &Slide, course: &Course) -> Self {
        if self.kind() == kind {
            return self.clone();
        }
        match kind {
            ConditionKind::Compare => {
                let (variable, op, value) = match course.variables.first() {
                    Some(v) => match v.value.kind() {
                        ValueKind::Number => (v.name.clone(), CmpOp::Ge, Value::Number(0.0)),
                        ValueKind::Bool => (v.name.clone(), CmpOp::Eq, Value::Bool(true)),
                        ValueKind::Text => (v.name.clone(), CmpOp::Eq, Value::Text(String::new())),
                    },
                    None => match coursecraft_course::BUILTIN_VARIABLES.first() {
                        Some(&(name, _)) => (name.to_string(), CmpOp::Ge, Value::Number(0.0)),
                        None => (String::new(), CmpOp::Eq, Value::Number(0.0)),
                    },
                };
                Self::Compare {
                    variable,
                    op,
                    value: ValueSource::Constant(value),
                }
            }
            ConditionKind::ObjectState => {
                let object = slide
                    .objects
                    .first()
                    .or_else(|| slide.layers.iter().find_map(|l| l.objects.first()))
                    .map(|o| o.id.clone())
                    .unwrap_or_default();
                Self::ObjectState {
                    object,
                    state: "visited".to_owned(),
                    is: true,
                }
            }
            ConditionKind::All => match self {
                Self::Any(list) => Self::All(list.clone()),
                other => Self::All(vec![other.clone()]),
            },
            ConditionKind::Any => match self {
                Self::All(list) => Self::Any(list.clone()),
                other => Self::Any(vec![other.clone()]),
            },
            ConditionKind::Not => Self::Not(Box::new(self.clone())),
        }
    }

    /// Convert to course model Condition.
    pub fn to_condition(&self) -> Result<Condition, String> {
        match self {
            Self::Compare {
                variable,
                op,
                value,
            } => {
                if variable.trim().is_empty() {
                    return Err("Variable name cannot be empty".to_owned());
                }
                if let ValueSource::Variable { variable: rhs } = value
                    && rhs.trim().is_empty()
                {
                    return Err("Comparison variable cannot be empty".to_owned());
                }
                Ok(Condition::Compare {
                    variable: variable.clone(),
                    op: *op,
                    value: value.clone(),
                })
            }
            Self::ObjectState { object, state, is } => {
                if object.trim().is_empty() {
                    return Err("Object cannot be empty".to_owned());
                }
                if state.trim().is_empty() {
                    return Err("State cannot be empty".to_owned());
                }
                Ok(Condition::ObjectState {
                    object: object.clone(),
                    state: state.clone(),
                    is: *is,
                })
            }
            Self::All(list) => {
                let mut conds = Vec::with_capacity(list.len());
                for n in list {
                    conds.push(n.to_condition()?);
                }
                Ok(Condition::All(conds))
            }
            Self::Any(list) => {
                let mut conds = Vec::with_capacity(list.len());
                for n in list {
                    conds.push(n.to_condition()?);
                }
                Ok(Condition::Any(conds))
            }
            Self::Not(inner) => Ok(Condition::Not(Box::new(inner.to_condition()?))),
        }
    }

    /// Build node tree from existing Condition.
    #[must_use]
    pub fn from_condition(c: &Condition) -> Self {
        match c {
            Condition::Compare {
                variable,
                op,
                value,
            } => Self::Compare {
                variable: variable.clone(),
                op: *op,
                value: value.clone(),
            },
            Condition::ObjectState { object, state, is } => Self::ObjectState {
                object: object.clone(),
                state: state.clone(),
                is: *is,
            },
            Condition::All(list) => Self::All(list.iter().map(Self::from_condition).collect()),
            Condition::Any(list) => Self::Any(list.iter().map(Self::from_condition).collect()),
            Condition::Not(inner) => Self::Not(Box::new(Self::from_condition(inner))),
        }
    }

    /// Compute tree depth.
    #[must_use]
    pub fn depth(&self) -> usize {
        match self {
            Self::Compare { .. } | Self::ObjectState { .. } => 1,
            Self::All(list) | Self::Any(list) => {
                1 + list.iter().map(|c| c.depth()).max().unwrap_or(0)
            }
            Self::Not(inner) => 1 + inner.depth(),
        }
    }
}

/// Action variant selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActionTag {
    #[default]
    JumpToSlide,
    ShowLayer,
    HideLayer,
    ChangeState,
    SetVariable,
    PlayMedia,
    PauseMedia,
    StopMedia,
    PauseTimeline,
    ResumeTimeline,
    SubmitQuestion,
    SubmitResults,
    RetryQuiz,
    ReviewQuiz,
    ExitReview,
    PrintSlide,
    CompleteCourse,
    ExitCourse,
    OpenUrl,
    MoveOnPath,
    RunJavaScript,
    LightboxSlide,
    CloseLightbox,
    StartTimer,
    PauseTimer,
    ResetTimer,
}

impl ActionTag {
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::JumpToSlide => "Jump to slide",
            Self::ShowLayer => "Show layer",
            Self::HideLayer => "Hide layer",
            Self::ChangeState => "Change state",
            Self::SetVariable => "Set variable",
            Self::PlayMedia => "Play media",
            Self::PauseMedia => "Pause media",
            Self::StopMedia => "Stop media",
            Self::PauseTimeline => "Pause timeline",
            Self::ResumeTimeline => "Resume timeline",
            Self::SubmitQuestion => "Submit question",
            Self::SubmitResults => "Submit results",
            Self::RetryQuiz => "Retry quiz",
            Self::ReviewQuiz => "Review quiz",
            Self::ExitReview => "Exit review",
            Self::PrintSlide => "Print slide",
            Self::CompleteCourse => "Complete course",
            Self::ExitCourse => "Exit course",
            Self::OpenUrl => "Open URL",
            Self::MoveOnPath => "Move on path",
            Self::RunJavaScript => "Run JavaScript",
            Self::LightboxSlide => "Lightbox slide",
            Self::CloseLightbox => "Close lightbox",
            Self::StartTimer => "Start timer",
            Self::PauseTimer => "Pause timer",
            Self::ResetTimer => "Reset timer",
        }
    }

    pub const ALL: [Self; 26] = [
        Self::JumpToSlide,
        Self::ShowLayer,
        Self::HideLayer,
        Self::ChangeState,
        Self::SetVariable,
        Self::PlayMedia,
        Self::PauseMedia,
        Self::StopMedia,
        Self::PauseTimeline,
        Self::ResumeTimeline,
        Self::SubmitQuestion,
        Self::SubmitResults,
        Self::RetryQuiz,
        Self::ReviewQuiz,
        Self::ExitReview,
        Self::PrintSlide,
        Self::CompleteCourse,
        Self::ExitCourse,
        Self::OpenUrl,
        Self::MoveOnPath,
        Self::RunJavaScript,
        Self::LightboxSlide,
        Self::CloseLightbox,
        Self::StartTimer,
        Self::PauseTimer,
        Self::ResetTimer,
    ];
}

/// Jump to slide target variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JumpTargetTag {
    #[default]
    Next,
    Previous,
    LastVisited,
    SlideId,
    Scene,
}

/// Event variant selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EventKindTag {
    #[default]
    Click,
    HoverStart,
    HoverEnd,
    TimelineStarts,
    TimelineEnds,
    TimelineReaches,
    VariableChanges,
    StateChanges,
    MediaEnds,
    CueReached,
    KeyPress,
    DroppedOn,
    ObjectsIntersect,
    ObjectsStopIntersecting,
    Submit,
    AnimationCompletes,
    MarkerOpened,
    MarkerClosed,
    TimerEnds,
}

impl EventKindTag {
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::Click => "User clicks",
            Self::HoverStart => "User hovers over",
            Self::HoverEnd => "User stops hovering",
            Self::TimelineStarts => "Timeline starts",
            Self::TimelineEnds => "Timeline ends",
            Self::TimelineReaches => "Timeline reaches",
            Self::VariableChanges => "Variable changes",
            Self::StateChanges => "State changes",
            Self::MediaEnds => "Media ends",
            Self::CueReached => "Video cue reached",
            Self::KeyPress => "Key press",
            Self::DroppedOn => "Dropped on",
            Self::ObjectsIntersect => "Objects intersect",
            Self::ObjectsStopIntersecting => "Objects stop intersecting",
            Self::Submit => "User submits",
            Self::AnimationCompletes => "Animation completes",
            Self::MarkerOpened => "Marker opens",
            Self::MarkerClosed => "Marker closes",
            Self::TimerEnds => "Timer ends",
        }
    }

    pub const ALL: [Self; 19] = [
        Self::Click,
        Self::HoverStart,
        Self::HoverEnd,
        Self::TimelineStarts,
        Self::TimelineEnds,
        Self::TimelineReaches,
        Self::VariableChanges,
        Self::StateChanges,
        Self::MediaEnds,
        Self::CueReached,
        Self::KeyPress,
        Self::DroppedOn,
        Self::ObjectsIntersect,
        Self::ObjectsStopIntersecting,
        Self::Submit,
        Self::AnimationCompletes,
        Self::MarkerOpened,
        Self::MarkerClosed,
        Self::TimerEnds,
    ];
}

/// State for editing a trigger in the Trigger Editor modal window.
#[derive(Debug, Clone, PartialEq)]
pub struct TriggerFormState {
    pub editing_index: Option<usize>,
    /// Kept when an existing trigger is edited.
    pub disabled: bool,
    pub owner_slide: String,
    pub owner_layer: Option<String>,

    // Action fields
    pub action_tag: ActionTag,
    pub jump_target_tag: JumpTargetTag,
    pub jump_slide_id: String,
    pub jump_scene_id: String,
    pub show_layer_id: String,
    pub hide_layer_id: Option<String>,
    pub change_state_object: String,
    pub change_state_state: String,
    pub set_var_name: String,
    pub set_var_op: VarOp,
    pub set_var_use_var: bool,
    pub set_var_src_var: String,
    pub set_var_val_bool: bool,
    pub set_var_val_num: f64,
    pub set_var_val_text: String,
    pub media_object: String,
    pub complete_status: CompletionStatus,
    pub open_url: String,
    pub move_path_object: String,
    pub move_path_id: String,
    pub javascript_code: String,
    pub lightbox_slide_id: String,
    pub timer_object: String,
    /// The timer of a Timer Ends event; apart from `timer_object` (the timer an action
    /// starts, pauses or resets), so a trigger can use two timers (`N90`).
    pub event_timer_object: String,

    // When fields
    pub event_tag: EventKindTag,
    pub event_layer: Option<String>,
    pub event_ms: u32,
    pub event_timeline_cue: Option<String>,
    pub event_object: String,
    pub event_cue: String,
    pub event_var: String,
    pub event_state_to: Option<String>,
    pub event_key: String,
    pub event_key_object: Option<String>,
    pub event_drop_target: String,
    pub event_intersect_other: String,
    pub anim_complete_object: String,
    pub anim_complete_which: String,

    // Condition fields
    pub condition_enabled: bool,
    pub condition_root: ConditionNode,

    pub error_message: Option<String>,
}

impl TriggerFormState {
    /// Initialize a new trigger form for the given slide and layer.
    #[must_use]
    pub fn new_for_owner(slide: &Slide, layer_id: Option<&str>, course: &Course) -> Self {
        let first_scene_id = course
            .scenes
            .first()
            .map(|s| s.id.clone())
            .unwrap_or_default();
        let first_slide_id = course
            .slides()
            .next()
            .map(|s| s.id.clone())
            .unwrap_or_default();
        let first_obj_id = slide
            .objects
            .first()
            .map(|o| o.id.clone())
            .unwrap_or_default();
        let first_var = course
            .variables
            .first()
            .map(|v| v.name.clone())
            .unwrap_or_default();
        let first_layer = slide
            .layers
            .first()
            .map(|l| l.id.clone())
            .unwrap_or_default();

        let first_path_id = slide
            .objects
            .first()
            .and_then(|o| o.motion_paths.first())
            .map(|p| p.id.clone())
            .unwrap_or_default();
        let first_timer_id = slide
            .objects
            .iter()
            .chain(slide.layers.iter().flat_map(|l| l.objects.iter()))
            .find(|o| matches!(o.kind, coursecraft_course::ObjectKind::Timer { .. }))
            .map(|o| o.id.clone())
            .unwrap_or_default();
        let first_cue = slide
            .objects
            .iter()
            .chain(slide.layers.iter().flat_map(|l| l.objects.iter()))
            .find_map(|object| match &object.kind {
                coursecraft_course::ObjectKind::Video { cues, .. } if !cues.is_empty() => {
                    cues.first().map(|cue| cue.id.clone())
                }
                _ => None,
            })
            .unwrap_or_default();

        Self {
            editing_index: None,
            disabled: false,
            owner_slide: slide.id.clone(),
            owner_layer: layer_id.map(str::to_owned),
            action_tag: ActionTag::JumpToSlide,
            jump_target_tag: JumpTargetTag::Next,
            jump_slide_id: first_slide_id,
            jump_scene_id: first_scene_id,
            show_layer_id: first_layer,
            hide_layer_id: None,
            change_state_object: first_obj_id.clone(),
            change_state_state: "normal".to_owned(),
            set_var_name: first_var.clone(),
            set_var_op: VarOp::Assign,
            set_var_use_var: false,
            set_var_src_var: first_var.clone(),
            set_var_val_bool: false,
            set_var_val_num: 0.0,
            set_var_val_text: String::new(),
            media_object: first_obj_id.clone(),
            complete_status: CompletionStatus::Completed,
            open_url: String::new(),
            move_path_object: first_obj_id.clone(),
            move_path_id: first_path_id,
            javascript_code: String::new(),
            timer_object: first_timer_id.clone(),
            event_timer_object: first_timer_id,
            lightbox_slide_id: course
                .slides()
                .find(|other| other.id != slide.id)
                .map(|other| other.id.clone())
                .unwrap_or_default(),

            event_tag: EventKindTag::Click,
            event_layer: None,
            event_ms: 0,
            event_timeline_cue: None,
            event_object: first_obj_id.clone(),
            event_cue: first_cue,
            event_var: first_var,
            event_state_to: None,
            event_key: "enter".to_owned(),
            event_key_object: None,
            event_drop_target: first_obj_id.clone(),
            event_intersect_other: first_obj_id.clone(),
            anim_complete_object: first_obj_id.clone(),
            anim_complete_which: "entrance".to_owned(),

            condition_enabled: false,
            condition_root: ConditionNode::ObjectState {
                object: first_obj_id,
                state: "visited".to_owned(),
                is: false,
            },
            error_message: None,
        }
    }

    /// Default the trigger's event object to `object_id` when it exists on `slide`
    /// as an object or an interactive group.
    #[must_use]
    pub fn with_selected_object(mut self, slide: &Slide, object_id: Option<&str>) -> Self {
        if let Some(id) = object_id
            && (slide.object(id).is_some() || slide.interactive_group(id).is_some())
        {
            self.event_object = id.to_owned();
        }
        self
    }

    /// Populate form state from an existing trigger.
    #[must_use]
    pub fn from_existing(
        index: usize,
        trigger: &Trigger,
        slide: &Slide,
        layer_id: Option<&str>,
        course: &Course,
    ) -> Self {
        let mut s = Self::new_for_owner(slide, layer_id, course);
        s.editing_index = Some(index);
        s.disabled = trigger.disabled;

        match &trigger.action {
            Action::JumpToSlide { target } => {
                s.action_tag = ActionTag::JumpToSlide;
                match target {
                    SlideTarget::Next => s.jump_target_tag = JumpTargetTag::Next,
                    SlideTarget::Previous => s.jump_target_tag = JumpTargetTag::Previous,
                    SlideTarget::LastVisited => s.jump_target_tag = JumpTargetTag::LastVisited,
                    SlideTarget::Id(id) => {
                        s.jump_target_tag = JumpTargetTag::SlideId;
                        s.jump_slide_id = id.clone();
                    }
                    SlideTarget::Scene(id) => {
                        s.jump_target_tag = JumpTargetTag::Scene;
                        s.jump_scene_id = id.clone();
                    }
                }
            }
            Action::ShowLayer { layer } => {
                s.action_tag = ActionTag::ShowLayer;
                s.show_layer_id = layer.clone();
            }
            Action::HideLayer { layer } => {
                s.action_tag = ActionTag::HideLayer;
                s.hide_layer_id = layer.clone();
            }
            Action::ChangeState { object, state } => {
                s.action_tag = ActionTag::ChangeState;
                s.change_state_object = object.clone();
                s.change_state_state = state.clone();
            }
            Action::SetVariable {
                variable,
                op,
                value,
            } => {
                s.action_tag = ActionTag::SetVariable;
                s.set_var_name = variable.clone();
                s.set_var_op = *op;
                if let Some(v) = value {
                    match v {
                        ValueSource::Constant(val) => {
                            s.set_var_use_var = false;
                            match val {
                                Value::Bool(b) => s.set_var_val_bool = *b,
                                Value::Number(n) => s.set_var_val_num = *n,
                                Value::Text(t) => s.set_var_val_text = t.clone(),
                            }
                        }
                        ValueSource::Variable { variable: src } => {
                            s.set_var_use_var = true;
                            s.set_var_src_var = src.clone();
                        }
                    }
                }
            }
            Action::PlayMedia { object } => {
                s.action_tag = ActionTag::PlayMedia;
                s.media_object = object.clone();
            }
            Action::PauseMedia { object } => {
                s.action_tag = ActionTag::PauseMedia;
                s.media_object = object.clone();
            }
            Action::StopMedia { object } => {
                s.action_tag = ActionTag::StopMedia;
                s.media_object = object.clone();
            }
            Action::PauseTimeline => s.action_tag = ActionTag::PauseTimeline,
            Action::ResumeTimeline => s.action_tag = ActionTag::ResumeTimeline,
            Action::SubmitQuestion => s.action_tag = ActionTag::SubmitQuestion,
            Action::SubmitResults => s.action_tag = ActionTag::SubmitResults,
            Action::RetryQuiz => s.action_tag = ActionTag::RetryQuiz,
            Action::ReviewQuiz => s.action_tag = ActionTag::ReviewQuiz,
            Action::ExitReview => s.action_tag = ActionTag::ExitReview,
            Action::PrintSlide => s.action_tag = ActionTag::PrintSlide,
            Action::CompleteCourse { status } => {
                s.action_tag = ActionTag::CompleteCourse;
                s.complete_status = *status;
            }
            Action::ExitCourse => s.action_tag = ActionTag::ExitCourse,
            Action::OpenUrl { url } => {
                s.action_tag = ActionTag::OpenUrl;
                s.open_url = url.clone();
            }
            Action::MoveOnPath { object, path } => {
                s.action_tag = ActionTag::MoveOnPath;
                s.move_path_object = object.clone();
                s.move_path_id = path.clone();
            }
            Action::RunJavaScript { code } => {
                s.action_tag = ActionTag::RunJavaScript;
                s.javascript_code = code.clone();
            }
            Action::LightboxSlide { slide: target } => {
                s.action_tag = ActionTag::LightboxSlide;
                s.lightbox_slide_id = target.clone();
            }
            Action::CloseLightbox => s.action_tag = ActionTag::CloseLightbox,
            Action::StartTimer { timer } => {
                s.action_tag = ActionTag::StartTimer;
                s.timer_object = timer.clone();
            }
            Action::PauseTimer { timer } => {
                s.action_tag = ActionTag::PauseTimer;
                s.timer_object = timer.clone();
            }
            Action::ResetTimer { timer } => {
                s.action_tag = ActionTag::ResetTimer;
                s.timer_object = timer.clone();
            }
        }

        match &trigger.when {
            Event::TimelineStarts { layer } => {
                s.event_tag = EventKindTag::TimelineStarts;
                s.event_layer = layer.clone();
            }
            Event::TimelineEnds { layer } => {
                s.event_tag = EventKindTag::TimelineEnds;
                s.event_layer = layer.clone();
            }
            Event::TimelineReaches { ms, layer, cue } => {
                s.event_tag = EventKindTag::TimelineReaches;
                s.event_ms = *ms;
                s.event_layer = layer.clone();
                s.event_timeline_cue = cue.clone();
            }
            Event::Click { object } => {
                s.event_tag = EventKindTag::Click;
                s.event_object = object.clone();
            }
            Event::HoverStart { object } => {
                s.event_tag = EventKindTag::HoverStart;
                s.event_object = object.clone();
            }
            Event::HoverEnd { object } => {
                s.event_tag = EventKindTag::HoverEnd;
                s.event_object = object.clone();
            }
            Event::VariableChanges { variable } => {
                s.event_tag = EventKindTag::VariableChanges;
                s.event_var = variable.clone();
            }
            Event::StateChanges { object, to } => {
                s.event_tag = EventKindTag::StateChanges;
                s.event_object = object.clone();
                s.event_state_to = to.clone();
            }
            Event::MediaEnds { object } => {
                s.event_tag = EventKindTag::MediaEnds;
                s.event_object = object.clone();
            }
            Event::CueReached { object, cue } => {
                s.event_tag = EventKindTag::CueReached;
                s.event_object = object.clone();
                s.event_cue = cue.clone();
            }
            Event::MarkerOpened { object } => {
                s.event_tag = EventKindTag::MarkerOpened;
                s.event_object = object.clone();
            }
            Event::MarkerClosed { object } => {
                s.event_tag = EventKindTag::MarkerClosed;
                s.event_object = object.clone();
            }
            Event::KeyPressed { key, object } => {
                s.event_tag = EventKindTag::KeyPress;
                s.event_key = key.clone();
                s.event_key_object = object.clone();
            }
            Event::DroppedOn { object, target } => {
                s.event_tag = EventKindTag::DroppedOn;
                s.event_object = object.clone();
                s.event_drop_target = target.clone();
            }
            Event::ObjectsIntersect { a, b } => {
                s.event_tag = EventKindTag::ObjectsIntersect;
                s.event_object = a.clone();
                s.event_intersect_other = b.clone();
            }
            Event::ObjectsStopIntersecting { a, b } => {
                s.event_tag = EventKindTag::ObjectsStopIntersecting;
                s.event_object = a.clone();
                s.event_intersect_other = b.clone();
            }
            Event::TimerEnds { timer } => {
                s.event_tag = EventKindTag::TimerEnds;
                s.event_timer_object = timer.clone();
            }
            Event::Submit => s.event_tag = EventKindTag::Submit,
            Event::AnimationCompletes { object, path } => {
                s.event_tag = EventKindTag::AnimationCompletes;
                s.anim_complete_object = object.clone();
                s.anim_complete_which = path.clone().unwrap_or_else(|| "entrance".to_owned());
            }
        }

        if let Some(ref cond) = trigger.condition {
            s.condition_enabled = true;
            s.condition_root = ConditionNode::from_condition(cond);
        } else {
            s.condition_enabled = false;
        }

        s
    }

    /// Construct a Trigger from the form fields.
    pub fn build_trigger(&self, course: &Course) -> Result<Trigger, String> {
        let action = match self.action_tag {
            ActionTag::JumpToSlide => {
                let target = match self.jump_target_tag {
                    JumpTargetTag::Next => SlideTarget::Next,
                    JumpTargetTag::Previous => SlideTarget::Previous,
                    JumpTargetTag::LastVisited => SlideTarget::LastVisited,
                    JumpTargetTag::SlideId => {
                        if self.jump_slide_id.trim().is_empty() {
                            return Err("Target slide cannot be empty".to_owned());
                        }
                        SlideTarget::Id(self.jump_slide_id.clone())
                    }
                    JumpTargetTag::Scene => {
                        if self.jump_scene_id.trim().is_empty() {
                            return Err("Target scene cannot be empty".to_owned());
                        }
                        SlideTarget::Scene(self.jump_scene_id.clone())
                    }
                };
                Action::JumpToSlide { target }
            }
            ActionTag::ShowLayer => {
                if self.show_layer_id.trim().is_empty() {
                    return Err("Layer to show cannot be empty".to_owned());
                }
                Action::ShowLayer {
                    layer: self.show_layer_id.clone(),
                }
            }
            ActionTag::HideLayer => Action::HideLayer {
                layer: self.hide_layer_id.clone(),
            },
            ActionTag::ChangeState => {
                if self.change_state_object.trim().is_empty() {
                    return Err("Target object cannot be empty".to_owned());
                }
                if self.change_state_state.trim().is_empty() {
                    return Err("Target state cannot be empty".to_owned());
                }
                Action::ChangeState {
                    object: self.change_state_object.clone(),
                    state: self.change_state_state.clone(),
                }
            }
            ActionTag::SetVariable => {
                if self.set_var_name.trim().is_empty() {
                    return Err("Variable name cannot be empty".to_owned());
                }
                let var_kind = course
                    .variable(&self.set_var_name)
                    .map(|v| v.value.kind())
                    .unwrap_or(ValueKind::Number);
                let value = if self.set_var_op == VarOp::Toggle {
                    None
                } else if self.set_var_use_var {
                    if self.set_var_src_var.trim().is_empty() {
                        return Err("Source variable cannot be empty".to_owned());
                    }
                    Some(ValueSource::Variable {
                        variable: self.set_var_src_var.clone(),
                    })
                } else {
                    match var_kind {
                        ValueKind::Bool => {
                            Some(ValueSource::Constant(Value::Bool(self.set_var_val_bool)))
                        }
                        ValueKind::Number => {
                            Some(ValueSource::Constant(Value::Number(self.set_var_val_num)))
                        }
                        ValueKind::Text => Some(ValueSource::Constant(Value::Text(
                            self.set_var_val_text.clone(),
                        ))),
                    }
                };
                Action::SetVariable {
                    variable: self.set_var_name.clone(),
                    op: self.set_var_op,
                    value,
                }
            }
            ActionTag::PlayMedia => Action::PlayMedia {
                object: self.media_object.clone(),
            },
            ActionTag::PauseMedia => Action::PauseMedia {
                object: self.media_object.clone(),
            },
            ActionTag::StopMedia => Action::StopMedia {
                object: self.media_object.clone(),
            },
            ActionTag::PauseTimeline => Action::PauseTimeline,
            ActionTag::ResumeTimeline => Action::ResumeTimeline,
            ActionTag::SubmitQuestion => Action::SubmitQuestion,
            ActionTag::SubmitResults => Action::SubmitResults,
            ActionTag::RetryQuiz => Action::RetryQuiz,
            ActionTag::ReviewQuiz => Action::ReviewQuiz,
            ActionTag::ExitReview => Action::ExitReview,
            ActionTag::PrintSlide => Action::PrintSlide,
            ActionTag::CompleteCourse => Action::CompleteCourse {
                status: self.complete_status,
            },
            ActionTag::ExitCourse => Action::ExitCourse,
            ActionTag::OpenUrl => Action::OpenUrl {
                url: self.open_url.clone(),
            },
            ActionTag::MoveOnPath => {
                if self.move_path_object.trim().is_empty() {
                    return Err("Target object cannot be empty".to_owned());
                }
                if self.move_path_id.trim().is_empty() {
                    return Err("Target motion path cannot be empty".to_owned());
                }
                Action::MoveOnPath {
                    object: self.move_path_object.clone(),
                    path: self.move_path_id.clone(),
                }
            }
            ActionTag::RunJavaScript => {
                if self.javascript_code.trim().is_empty() {
                    return Err("JavaScript cannot be empty".to_owned());
                }
                if self.javascript_code.len() > coursecraft_course::MAX_JAVASCRIPT_BYTES {
                    return Err(format!(
                        "JavaScript must be at most {} bytes",
                        coursecraft_course::MAX_JAVASCRIPT_BYTES
                    ));
                }
                Action::RunJavaScript {
                    code: self.javascript_code.clone(),
                }
            }
            ActionTag::LightboxSlide => {
                if self.lightbox_slide_id.trim().is_empty() {
                    return Err("Lightbox slide cannot be empty".to_owned());
                }
                if self.lightbox_slide_id == self.owner_slide {
                    return Err("A lightbox cannot show the slide it is on".to_owned());
                }
                Action::LightboxSlide {
                    slide: self.lightbox_slide_id.clone(),
                }
            }
            ActionTag::CloseLightbox => Action::CloseLightbox,
            ActionTag::StartTimer => Action::StartTimer {
                timer: self.timer_object.clone(),
            },
            ActionTag::PauseTimer => Action::PauseTimer {
                timer: self.timer_object.clone(),
            },
            ActionTag::ResetTimer => Action::ResetTimer {
                timer: self.timer_object.clone(),
            },
        };

        let when = match self.event_tag {
            EventKindTag::TimelineStarts => Event::TimelineStarts {
                layer: self.event_layer.clone(),
            },
            EventKindTag::TimelineEnds => Event::TimelineEnds {
                layer: self.event_layer.clone(),
            },
            EventKindTag::TimelineReaches => Event::TimelineReaches {
                ms: self.event_ms,
                layer: self.event_layer.clone(),
                cue: self.event_timeline_cue.clone(),
            },
            EventKindTag::Click => Event::Click {
                object: self.event_object.clone(),
            },
            EventKindTag::HoverStart => Event::HoverStart {
                object: self.event_object.clone(),
            },
            EventKindTag::HoverEnd => Event::HoverEnd {
                object: self.event_object.clone(),
            },
            EventKindTag::VariableChanges => Event::VariableChanges {
                variable: self.event_var.clone(),
            },
            EventKindTag::StateChanges => Event::StateChanges {
                object: self.event_object.clone(),
                to: self.event_state_to.clone(),
            },
            EventKindTag::MediaEnds => Event::MediaEnds {
                object: self.event_object.clone(),
            },
            EventKindTag::CueReached => {
                if self.event_object.trim().is_empty() {
                    return Err("Video cannot be empty".to_owned());
                }
                if self.event_cue.trim().is_empty() {
                    return Err("Cue cannot be empty".to_owned());
                }
                Event::CueReached {
                    object: self.event_object.clone(),
                    cue: self.event_cue.clone(),
                }
            }
            EventKindTag::MarkerOpened => Event::MarkerOpened {
                object: self.event_object.clone(),
            },
            EventKindTag::MarkerClosed => Event::MarkerClosed {
                object: self.event_object.clone(),
            },
            EventKindTag::KeyPress => {
                if self.event_key.trim().is_empty() {
                    return Err("Key cannot be empty".to_owned());
                }
                Event::KeyPressed {
                    key: self.event_key.clone(),
                    object: self.event_key_object.clone(),
                }
            }
            EventKindTag::DroppedOn => Event::DroppedOn {
                object: self.event_object.clone(),
                target: self.event_drop_target.clone(),
            },
            EventKindTag::ObjectsIntersect => Event::ObjectsIntersect {
                a: self.event_object.clone(),
                b: self.event_intersect_other.clone(),
            },
            EventKindTag::ObjectsStopIntersecting => Event::ObjectsStopIntersecting {
                a: self.event_object.clone(),
                b: self.event_intersect_other.clone(),
            },
            EventKindTag::Submit => Event::Submit,
            EventKindTag::AnimationCompletes => {
                if self.anim_complete_object.trim().is_empty() {
                    return Err("Target object cannot be empty".to_owned());
                }
                let path = if self.anim_complete_which.is_empty()
                    || self.anim_complete_which == "entrance"
                {
                    None
                } else {
                    Some(self.anim_complete_which.clone())
                };
                Event::AnimationCompletes {
                    object: self.anim_complete_object.clone(),
                    path,
                }
            }
            EventKindTag::TimerEnds => Event::TimerEnds {
                timer: self.event_timer_object.clone(),
            },
        };

        let condition = if self.condition_enabled {
            Some(self.condition_root.to_condition()?)
        } else {
            None
        };

        Ok(Trigger {
            action,
            when,
            condition,
            disabled: self.disabled,
        })
    }

    /// Commit the trigger via Editor::run, returning true on success.
    pub fn commit_ok(&mut self, editor: &mut crate::Editor) -> bool {
        let trigger = match self.build_trigger(&editor.session.course) {
            Ok(t) => t,
            Err(e) => {
                self.error_message = Some(e.clone());
                editor.ui.last_error = Some((e.clone(), editor.ui.current_time));
                editor.ui.error_history.push(e);
                return false;
            }
        };

        let result = match self.editing_index {
            None => {
                let params = match &self.owner_layer {
                    Some(lid) => serde_json::json!({
                        "slide": self.owner_slide,
                        "layer": lid,
                        "trigger": trigger,
                    }),
                    None => serde_json::json!({
                        "slide": self.owner_slide,
                        "trigger": trigger,
                    }),
                };
                editor.run("trigger.add", params)
            }
            Some(index) => {
                let params = match &self.owner_layer {
                    Some(lid) => serde_json::json!({
                        "slide": self.owner_slide,
                        "layer": lid,
                        "index": index,
                        "trigger": trigger,
                    }),
                    None => serde_json::json!({
                        "slide": self.owner_slide,
                        "index": index,
                        "trigger": trigger,
                    }),
                };
                editor.run("trigger.update", params)
            }
        };

        if result.is_some() {
            self.error_message = None;
            true
        } else {
            self.error_message = editor.ui.last_error.as_ref().map(|(msg, _)| msg.clone());
            false
        }
    }
}

/// Render the Triggers tab inside the bottom panel.
pub fn show_triggers_tab(
    editor: &mut crate::Editor,
    ui: &mut Ui,
    actions: &mut Vec<(&'static str, Json)>,
) {
    let slide_id = editor.selection.slide.clone();
    let selected_layer = editor.selection.layer.clone();
    let mut paste_clicked = false;
    {
        let crate::Editor {
            session,
            selection,
            ui: editor_ui,
            ..
        } = editor;
        let Some(slide) = session.course.slide(&selection.slide) else {
            ui.label("No slide selected");
            return;
        };

        let triggers: &[Trigger] = match &selected_layer {
            Some(lid) => match slide.layer(lid) {
                Some(l) => &l.triggers,
                None => &[],
            },
            None => &slide.triggers,
        };

        let trigger_prefix =
            trigger_list_path_prefix(&session.course, &slide.id, selected_layer.as_deref());
        let trigger_issues: Vec<Vec<crate::ValidationIssueSummary>> = (0..triggers.len())
            .map(|idx| {
                trigger_prefix
                    .as_ref()
                    .and_then(|prefix| {
                        editor_ui
                            .trigger_issues_by_path
                            .get(&format!("{prefix}/{idx}"))
                    })
                    .cloned()
                    .unwrap_or_default()
            })
            .collect();
        let problem_indices: Vec<usize> = trigger_issues
            .iter()
            .enumerate()
            .filter_map(|(idx, issues)| (!issues.is_empty()).then_some(idx))
            .collect();

        let p = coursecraft_render::palette(ui.ctx());
        // Header row: caption on the left, quiet "+ Add trigger" on the right.
        ui.horizontal(|ui| {
            let owner_name = match &selected_layer {
                Some(lid) => format!("on layer {}", layer_name(slide, lid)),
                None => "on this slide".to_owned(),
            };
            ui.label(
                coursecraft_render::caption(format!("Triggers {owner_name}"))
                    .color(p.text_secondary),
            );
            let can_paste = !editor_ui.copied_triggers.is_empty();
            if !triggers.is_empty() || can_paste {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if !triggers.is_empty()
                        && crate::quiet_button(ui, "+ Add trigger", crate::QuietTone::Accent, true)
                            .clicked()
                    {
                        let default_target =
                            default_trigger_target(selection, &session.course, slide);
                        editor_ui.trigger_form = Some(
                            TriggerFormState::new_for_owner(
                                slide,
                                selected_layer.as_deref(),
                                &session.course,
                            )
                            .with_selected_object(slide, default_target.as_deref()),
                        );
                    }
                    if can_paste
                        && crate::quiet_button(ui, "Paste triggers", crate::QuietTone::Accent, true)
                            .clicked()
                    {
                        paste_clicked = true;
                    }
                });
            }
        });

        ui.separator();

        // The row the user last clicked keeps its actions visible. It is remembered per owner
        // (slide and layer) so switching slides starts clean.
        let owner_key = format!("{}/{}", slide.id, selected_layer.as_deref().unwrap_or(""));
        let active_id = ui.make_persistent_id("triggers_active_row");
        let active_row: Option<usize> = ui.data(|d| {
            d.get_temp::<(String, usize)>(active_id)
                .and_then(|(k, i)| (k == owner_key).then_some(i))
        });
        let mut new_active_row: Option<usize> = None;

        if !problem_indices.is_empty() {
            let has_error = problem_indices.iter().any(|&idx| {
                trigger_issues
                    .get(idx)
                    .is_some_and(|issues| issues.iter().any(|i| i.severity == "error"))
            });
            let note_color = if has_error { p.danger } else { p.warning };
            let scope = if selected_layer.is_some() {
                "layer"
            } else {
                "slide"
            };
            let note = if problem_indices.len() == 1 {
                format!("1 trigger on this {scope} needs attention")
            } else {
                format!(
                    "{} triggers on this {scope} need attention",
                    problem_indices.len()
                )
            };
            ui.horizontal(|ui| {
                ui.label(
                    coursecraft_render::icon(
                        coursecraft_render::Icon::Warning,
                        coursecraft_render::ICON_SM,
                    )
                    .color(note_color),
                );
                ui.label(coursecraft_render::small(note).color(note_color));
                if let Some(&first_idx) = problem_indices.first()
                    && let Some(trig) = triggers.get(first_idx)
                    && crate::quiet_button(ui, "Fix the first one", crate::QuietTone::Accent, true)
                        .clicked()
                {
                    new_active_row = Some(first_idx);
                    editor_ui.trigger_form = Some(TriggerFormState::from_existing(
                        first_idx,
                        trig,
                        slide,
                        selected_layer.as_deref(),
                        &session.course,
                    ));
                }
            });
        }

        if !triggers.is_empty() {
            ui.horizontal(|ui| {
                let filter_w = (ui.available_width() - 180.0).clamp(140.0, 220.0);
                ui.add(
                    egui::TextEdit::singleline(&mut editor_ui.triggers.filter)
                        .hint_text("Filter triggers…")
                        .desired_width(filter_w),
                );
                if !editor_ui.triggers.filter.is_empty()
                    && crate::widgets::icon_button(ui, coursecraft_render::Icon::X, "Clear filter")
                        .clicked()
                {
                    editor_ui.triggers.filter.clear();
                }
                ui.checkbox(&mut editor_ui.triggers.group_by_object, "Group by object");
            });
        }

        ScrollArea::vertical()
            .id_salt("triggers_list_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if triggers.is_empty() {
                    ui.vertical_centered(|ui| {
                        ui.add_space(20.0);
                        ui.label(
                            RichText::new(
                                "No triggers on this slide yet. Triggers make objects respond to clicks, timing and variables.",
                            )
                            .color(p.text_secondary)
                            .size(coursecraft_render::SIZE_BODY),
                        );
                        ui.add_space(12.0);
                        if crate::primary_button(ui, "+ Add trigger").clicked() {
                            let default_target =
                                default_trigger_target(selection, &session.course, slide);
                            editor_ui.trigger_form = Some(
                                TriggerFormState::new_for_owner(
                                    slide,
                                    selected_layer.as_deref(),
                                    &session.course,
                                )
                                .with_selected_object(slide, default_target.as_deref()),
                            );
                        }
                    });
                    return;
                }

                type TriggerRow<'a> = (usize, &'a Trigger, String);
                let filter_lower = editor_ui.triggers.filter.trim().to_ascii_lowercase();
                let mut visible: Vec<TriggerRow<'_>> = triggers
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, trig)| {
                        let desc = describe(trig, slide);
                        if filter_lower.is_empty()
                            || desc.to_ascii_lowercase().contains(&filter_lower)
                        {
                            Some((idx, trig, desc))
                        } else {
                            None
                        }
                    })
                    .collect();

                if visible.is_empty() {
                    ui.vertical_centered(|ui| {
                        ui.add_space(16.0);
                        ui.label(
                            RichText::new("No triggers match the filter.")
                                .color(p.text_secondary)
                                .size(coursecraft_render::SIZE_BODY),
                        );
                    });
                    return;
                }

                let active_target = default_trigger_target(selection, &session.course, slide);
                type GroupHeader = Option<(String, bool)>;
                let groups: Vec<(GroupHeader, Vec<TriggerRow<'_>>)> =
                    if editor_ui.triggers.group_by_object {
                        let mut grouped: Vec<(Option<String>, String, Vec<TriggerRow<'_>>)> =
                            Vec::new();
                        for item in visible {
                            let key = event_object_id(&item.1.when).map(str::to_owned);
                            if let Some(existing) = grouped.iter_mut().find(|(k, _, _)| *k == key) {
                                existing.2.push(item);
                            } else {
                                let label = match key.as_deref() {
                                    Some(id) => target_display_name(slide, id),
                                    None => "Slide".to_owned(),
                                };
                                grouped.push((key, label, vec![item]));
                            }
                        }
                        grouped
                            .into_iter()
                            .map(|(key, label, rows)| {
                                let selected = active_target.is_some() && key == active_target;
                                (Some((label, selected)), rows)
                            })
                            .collect()
                    } else {
                        if let Some(sel_id) = active_target.as_deref() {
                            visible.sort_by_key(|(idx, trig, _)| {
                                let is_selected = event_object_id(&trig.when) == Some(sel_id);
                                (!is_selected, *idx)
                            });
                        }
                        vec![(None, visible)]
                    };

                // Highlights for the selected object or group, and the one-time scroll.
                let highlight_of = |trig: &Trigger| -> Option<RowHighlight> {
                    active_target
                        .as_deref()
                        .and_then(|target| row_highlight(trig, target))
                };
                editor_ui.triggers.row_highlights.clear();
                editor_ui.triggers.highlighted_header = None;
                let scroll_key = active_target
                    .as_deref()
                    .map(|target| format!("{}/{}/{target}", slide.id, owner_key));
                let scroll_now = match &scroll_key {
                    Some(key) => editor_ui.triggers.last_scroll_key.as_ref() != Some(key),
                    None => {
                        editor_ui.triggers.last_scroll_key = None;
                        false
                    }
                };
                // The first own row, else the first related row, in drawing order.
                let scroll_idx: Option<usize> = if scroll_now {
                    let all = || groups.iter().flat_map(|(_, rows)| rows.iter());
                    all()
                        .find(|(_, trig, _)| highlight_of(trig) == Some(RowHighlight::Own))
                        .or_else(|| all().find(|(_, trig, _)| highlight_of(trig).is_some()))
                        .map(|(idx, _, _)| *idx)
                } else {
                    None
                };
                if scroll_now {
                    editor_ui.triggers.last_scroll_key = scroll_key;
                }

                for (group_header, rows) in groups {
                    if let Some((header, selected)) = group_header {
                        ui.add_space(coursecraft_render::SPACE_SM);
                        let header_text = coursecraft_render::caption(header.clone())
                            .color(if selected { p.accent } else { p.text_secondary })
                            .strong();
                        if selected {
                            editor_ui.triggers.highlighted_header = Some(header);
                            let row = ui
                                .horizontal(|ui| {
                                    ui.add_space(coursecraft_render::SPACE_XS);
                                    ui.label(header_text);
                                })
                                .response
                                .rect;
                            let bar = egui::Rect::from_min_size(
                                row.min,
                                egui::vec2(3.0, row.height()),
                            );
                            ui.painter().rect_filled(bar, 0.0, p.accent);
                        } else {
                            ui.label(header_text);
                        }
                    }
                    let sel_id = if editor_ui.triggers.group_by_object {
                        None
                    } else {
                        active_target.as_deref()
                    };
                    let row_neighbors: Vec<(Option<usize>, Option<usize>)> = (0..rows.len())
                        .map(|pos| {
                            let cur_sel =
                                sel_id.map(|id| event_object_id(&rows[pos].1.when) == Some(id));
                            let prev = pos.checked_sub(1).and_then(|p_idx| {
                                let same_bucket = cur_sel.is_none_or(|s| {
                                    (event_object_id(&rows[p_idx].1.when) == sel_id) == s
                                });
                                same_bucket.then_some(rows[p_idx].0)
                            });
                            let next =
                                (pos + 1 < rows.len())
                                    .then_some(pos + 1)
                                    .and_then(|n_idx| {
                                        let same_bucket = cur_sel.is_none_or(|s| {
                                            (event_object_id(&rows[n_idx].1.when) == sel_id) == s
                                        });
                                        same_bucket.then_some(rows[n_idx].0)
                                    });
                            (prev, next)
                        })
                        .collect();
                    for (pos, (idx, trig, desc)) in rows.into_iter().enumerate() {
                        ui.push_id(idx, |ui| {
                        let (prev_idx, next_idx) = row_neighbors[pos];
                        let row_rect = egui::Rect::from_min_size(
                            ui.cursor().min,
                            egui::vec2(ui.available_width(), TRIGGER_ROW_HEIGHT),
                        );
                        let hovered = ui.rect_contains_pointer(row_rect);
                        let is_active = active_row == Some(idx);
                        let show_actions = hovered || is_active;
                        let interact_rect = if show_actions {
                            egui::Rect::from_min_max(
                                row_rect.min,
                                egui::pos2(
                                    (row_rect.max.x - 200.0).max(row_rect.min.x),
                                    row_rect.max.y,
                                ),
                            )
                        } else {
                            row_rect
                        };
                        // Registered before the row's widgets so the buttons win clicks over the row.
                        let row_resp = ui.interact(
                            interact_rect,
                            ui.scope_id().with(("trigger_row", idx)),
                            egui::Sense::click(),
                        );
                        if row_resp.clicked() {
                            new_active_row = Some(idx);
                        }
                        if row_resp.double_clicked() {
                            new_active_row = Some(idx);
                            editor_ui.trigger_form = Some(TriggerFormState::from_existing(
                                idx,
                                trig,
                                slide,
                                selected_layer.as_deref(),
                                &session.course,
                            ));
                        }
                        if is_active || hovered {
                            ui.painter().rect_filled(
                                row_rect,
                                egui::CornerRadius::same(coursecraft_render::RADIUS_SM as u8),
                                if is_active { p.accent_subtle } else { p.hover },
                            );
                        }

                        let highlight = highlight_of(trig);
                        if let Some(kind) = highlight {
                            editor_ui.triggers.row_highlights.insert(idx, kind);
                            let (tint, bar) = match kind {
                                RowHighlight::Own => (0.20, true),
                                RowHighlight::Related => (0.08, false),
                            };
                            ui.painter().rect_filled(
                                row_rect,
                                egui::CornerRadius::same(coursecraft_render::RADIUS_SM as u8),
                                p.accent.gamma_multiply(tint),
                            );
                            if bar {
                                ui.painter().rect_filled(
                                    egui::Rect::from_min_size(
                                        row_rect.min,
                                        egui::vec2(3.0, row_rect.height()),
                                    ),
                                    0.0,
                                    p.accent,
                                );
                            }
                        }
                        if scroll_idx == Some(idx) {
                            ui.scroll_to_rect(row_rect, Some(egui::Align::Center));
                        }

                        ui.scope_builder(egui::UiBuilder::new().max_rect(row_rect), |ui| {
                            ui.horizontal_centered(|ui| {
                                ui.add_space(coursecraft_render::SPACE_SM);
                                // Fixed-width index column keeps the descriptions aligned.
                                ui.allocate_ui_with_layout(
                                    egui::vec2(coursecraft_render::SPACE_20, TRIGGER_ROW_HEIGHT),
                                    egui::Layout::left_to_right(egui::Align::Center),
                                    |ui| {
                                        ui.set_min_width(coursecraft_render::SPACE_20);
                                        ui.add(
                                            egui::Label::new(format!("{}.", idx + 1))
                                                .selectable(false),
                                        )
                                    },
                                );
                                if let Some(issues) =
                                    trigger_issues.get(idx).filter(|v| !v.is_empty())
                                {
                                    let is_error = issues.iter().any(|i| i.severity == "error");
                                    let icon_color = if is_error { p.danger } else { p.warning };
                                    let tooltip = issues
                                        .iter()
                                        .map(|i| i.message.as_str())
                                        .collect::<Vec<_>>()
                                        .join("\n");
                                    ui.label(
                                        coursecraft_render::icon(
                                            coursecraft_render::Icon::Warning,
                                            coursecraft_render::ICON_SM,
                                        )
                                        .color(icon_color),
                                    )
                                    .on_hover_text(tooltip);
                                }
                                let text_max_w = if show_actions {
                                    (ui.available_width() - 180.0).max(80.0)
                                } else {
                                    ui.available_width().max(80.0)
                                };
                                ui.allocate_ui_with_layout(
                                    egui::vec2(text_max_w, TRIGGER_ROW_HEIGHT),
                                    egui::Layout::left_to_right(egui::Align::Center),
                                    |ui| {
                                        if trig.disabled {
                                            ui.add(
                                                egui::Label::new(
                                                    RichText::new(&desc).color(p.text_tertiary),
                                                )
                                                .truncate()
                                                .selectable(false),
                                            );
                                            ui.add(
                                                egui::Label::new(
                                                    coursecraft_render::small("Off")
                                                        .color(p.text_tertiary),
                                                )
                                                .selectable(false),
                                            );
                                        } else {
                                            ui.add(
                                                egui::Label::new(RichText::new(&desc).strong())
                                                    .truncate()
                                                    .selectable(false),
                                            );
                                        }
                                    },
                                );

                                if !show_actions {
                                    return;
                                }
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        let form_open = editor_ui.trigger_form.is_some();
                                        // More: duplicate, copy, disable or enable
                                        let more = crate::widgets::icon_button(
                                            ui,
                                            coursecraft_render::Icon::DotsThree,
                                            "More trigger actions",
                                        );
                                        // The row stays active so the menu below survives the pointer leaving it.
                                        if more.clicked() {
                                            new_active_row = Some(idx);
                                        }
                                        egui::Popup::menu(&more).show(|ui| {
                                            if ui
                                                .add_enabled(!form_open, egui::Button::new("Duplicate"))
                                                .on_disabled_hover_text(
                                                    "Close the trigger editor to duplicate triggers",
                                                )
                                                .clicked()
                                            {
                                                actions.push((
                                                    "trigger.add",
                                                    trigger_params(
                                                        &slide.id,
                                                        selected_layer.as_deref(),
                                                        trig,
                                                        Some(idx + 1),
                                                        None,
                                                    ),
                                                ));
                                                ui.close();
                                            }
                                            if ui.button("Copy").clicked() {
                                                editor_ui.copied_triggers = vec![trig.clone()];
                                                ui.close();
                                            }
                                            let label = if trig.disabled {
                                                "Enable"
                                            } else {
                                                "Disable"
                                            };
                                            if ui.button(label).clicked() {
                                                let mut toggled = trig.clone();
                                                toggled.disabled = !toggled.disabled;
                                                actions.push((
                                                    "trigger.update",
                                                    trigger_params(
                                                        &slide.id,
                                                        selected_layer.as_deref(),
                                                        &toggled,
                                                        None,
                                                        Some(idx),
                                                    ),
                                                ));
                                                ui.close();
                                            }
                                        });

                                        // Delete
                                        if crate::widgets::icon_button_danger_reason(
                                            ui,
                                            coursecraft_render::Icon::Trash,
                                            !form_open,
                                            "Delete trigger",
                                            "Close the trigger editor first",
                                        )
                                        .clicked()
                                        {
                                            let params = match &selected_layer {
                                                Some(lid) => serde_json::json!({
                                                    "slide": slide.id,
                                                    "layer": lid,
                                                    "index": idx,
                                                }),
                                                None => serde_json::json!({
                                                    "slide": slide.id,
                                                    "index": idx,
                                                }),
                                            };
                                            actions.push(("trigger.delete", params));
                                        }

                                        // Move Down
                                        let can_down = !form_open && next_idx.is_some();
                                        let down_tip = if form_open {
                                            "Close the trigger editor to reorder triggers"
                                        } else if next_idx.is_none() {
                                            "Already at the bottom"
                                        } else {
                                            "Move trigger down"
                                        };
                                        if crate::widgets::icon_button_enabled(
                                            ui,
                                            coursecraft_render::Icon::ArrowDown,
                                            can_down,
                                            down_tip,
                                        )
                                        .clicked()
                                            && let Some(to) = next_idx
                                        {
                                            let params = match &selected_layer {
                                                Some(lid) => serde_json::json!({
                                                    "slide": slide.id,
                                                    "layer": lid,
                                                    "from": idx,
                                                    "to": to,
                                                }),
                                                None => serde_json::json!({
                                                    "slide": slide.id,
                                                    "from": idx,
                                                    "to": to,
                                                }),
                                            };
                                            actions.push(("trigger.move", params));
                                            new_active_row = Some(to);
                                        }

                                        // Move Up
                                        let can_up = !form_open && prev_idx.is_some();
                                        let up_tip = if form_open {
                                            "Close the trigger editor to reorder triggers"
                                        } else if prev_idx.is_none() {
                                            "Already at the top"
                                        } else {
                                            "Move trigger up"
                                        };
                                        if crate::widgets::icon_button_enabled(
                                            ui,
                                            coursecraft_render::Icon::ArrowUp,
                                            can_up,
                                            up_tip,
                                        )
                                        .clicked()
                                            && let Some(to) = prev_idx
                                        {
                                            let params = match &selected_layer {
                                                Some(lid) => serde_json::json!({
                                                    "slide": slide.id,
                                                    "layer": lid,
                                                    "from": idx,
                                                    "to": to,
                                                }),
                                                None => serde_json::json!({
                                                    "slide": slide.id,
                                                    "from": idx,
                                                    "to": to,
                                                }),
                                            };
                                            actions.push(("trigger.move", params));
                                            new_active_row = Some(to);
                                        }

                                        // Edit
                                        if crate::widgets::icon_button(
                                            ui,
                                            coursecraft_render::Icon::PencilSimple,
                                            "Edit trigger",
                                        )
                                        .clicked()
                                        {
                                            editor_ui.trigger_form =
                                                Some(TriggerFormState::from_existing(
                                                    idx,
                                                    trig,
                                                    slide,
                                                    selected_layer.as_deref(),
                                                    &session.course,
                                                ));
                                        }
                                    },
                                );
                            });
                        });
                        ui.advance_cursor_after_rect(row_rect);
                        });
                    }
                }
            });

        if let Some(idx) = new_active_row {
            ui.data_mut(|d| d.insert_temp(active_id, (owner_key, idx)));
        }
    }
    if paste_clicked {
        paste_triggers(editor, &slide_id, selected_layer.as_deref());
    }
}

/// Canonical validation path prefix for the triggers on `slide_id` (and optional `layer_id`),
/// matching `coursecraft_course::Course::validate` paths (e.g. `"/scenes/0/slides/2/triggers"`).
fn trigger_list_path_prefix(
    course: &coursecraft_course::Course,
    slide_id: &str,
    layer_id: Option<&str>,
) -> Option<String> {
    let slide_path = course
        .scenes
        .iter()
        .enumerate()
        .find_map(|(si, scene)| {
            scene
                .slides
                .iter()
                .position(|s| s.id == slide_id)
                .map(|li| format!("/scenes/{si}/slides/{li}"))
        })
        .or_else(|| {
            course.banks.iter().enumerate().find_map(|(bi, bank)| {
                bank.slides
                    .iter()
                    .position(|s| s.id == slide_id)
                    .map(|si| format!("/banks/{bi}/slides/{si}"))
            })
        })?;
    match layer_id {
        Some(lid) => {
            let slide = course.slide(slide_id)?;
            let lyi = slide.layers.iter().position(|l| l.id == lid)?;
            Some(format!("{slide_path}/layers/{lyi}/triggers"))
        }
        None => Some(format!("{slide_path}/triggers")),
    }
}

/// Parameters for `trigger.add` (with `insert_at`) or `trigger.update` (with `index`).
fn trigger_params(
    slide: &str,
    layer: Option<&str>,
    trigger: &Trigger,
    insert_at: Option<usize>,
    index: Option<usize>,
) -> Json {
    let mut params = serde_json::json!({ "slide": slide, "trigger": trigger });
    if let Some(layer) = layer {
        params["layer"] = serde_json::json!(layer);
    }
    if let Some(at) = insert_at {
        params["index"] = serde_json::json!(at);
    }
    if let Some(index) = index {
        params["index"] = serde_json::json!(index);
    }
    params
}

/// Paste the copied triggers at the end of the current owner's list, as one undo step.
/// A trigger that would make the course invalid there is skipped and counted.
pub(crate) fn paste_triggers(editor: &mut crate::Editor, slide_id: &str, layer: Option<&str>) {
    let copied = editor.ui.copied_triggers.clone();
    let mut trial = editor.session.course.clone();
    let was_playable = trial.is_playable();
    let mut steps: Vec<(String, Json)> = Vec::new();
    let mut skipped = 0usize;
    for trigger in copied {
        let list = match trial.slide_mut(slide_id) {
            Some(slide) => match layer {
                Some(id) => slide.layer_mut(id).map(|l| &mut l.triggers),
                None => Some(&mut slide.triggers),
            },
            None => None,
        };
        let Some(list) = list else {
            skipped += 1;
            continue;
        };
        list.push(trigger.clone());
        if was_playable && !trial.is_playable() {
            if let Some(list) = trial.slide_mut(slide_id).and_then(|slide| match layer {
                Some(id) => slide.layer_mut(id).map(|l| &mut l.triggers),
                None => Some(&mut slide.triggers),
            }) {
                list.pop();
            }
            skipped += 1;
            continue;
        }
        steps.push((
            "trigger.add".to_owned(),
            trigger_params(slide_id, layer, &trigger, None, None),
        ));
    }
    let pasted = steps.len();
    if !steps.is_empty() && editor.run_batch(&steps).is_none() {
        return;
    }
    if skipped > 0 {
        editor.set_status(format!(
            "Pasted {pasted} trigger{}; skipped {skipped} that do not fit here",
            if pasted == 1 { "" } else { "s" }
        ));
    }
}

/// Event object id for grouping and prioritizing triggers in the list, if any.
fn event_object_id(event: &Event) -> Option<&str> {
    match event {
        Event::Click { object }
        | Event::HoverStart { object }
        | Event::HoverEnd { object }
        | Event::StateChanges { object, .. }
        | Event::MediaEnds { object }
        | Event::CueReached { object, .. }
        | Event::DroppedOn { object, .. }
        | Event::ObjectsIntersect { a: object, .. }
        | Event::ObjectsStopIntersecting { a: object, .. }
        | Event::AnimationCompletes { object, .. }
        | Event::MarkerOpened { object }
        | Event::MarkerClosed { object }
        | Event::TimerEnds { timer: object } => Some(object.as_str()),
        Event::KeyPressed { object, .. } => object.as_deref(),
        Event::TimelineStarts { .. }
        | Event::TimelineEnds { .. }
        | Event::TimelineReaches { .. }
        | Event::VariableChanges { .. }
        | Event::Submit => None,
    }
}

/// Height of one row in the Triggers tab list.
const TRIGGER_ROW_HEIGHT: f32 = coursecraft_render::CONTROL_COMPACT + coursecraft_render::SPACE_SM;

/// Render the Trigger Editor window dialog.
pub fn show_trigger_editor_window(editor: &mut crate::Editor, ctx: &egui::Context) {
    let Some(mut form) = editor.ui.trigger_form.take() else {
        return;
    };

    if editor.selection.slide != form.owner_slide || editor.selection.layer != form.owner_layer {
        return;
    }

    let title = if form.editing_index.is_some() {
        "Edit trigger"
    } else {
        "New trigger"
    };

    let course = &editor.session.course;
    let Some(slide) = course.slide(&form.owner_slide) else {
        return;
    };
    let owner_triggers = match &form.owner_layer {
        Some(lid) => {
            let Some(layer) = slide.layer(lid) else {
                return;
            };
            &layer.triggers
        }
        None => &slide.triggers,
    };
    if let Some(idx) = form.editing_index
        && idx >= owner_triggers.len()
    {
        return;
    }

    let mut close_dialog = false;
    let mut commit_requested = false;
    let popup_was_open = egui::Popup::is_any_open(ctx);

    Dialog::new(title)
        .default_size([550.0, 600.0])
        .resizable(true)
        .show_with_footer(ctx, |dialog| {
            let p = coursecraft_render::palette(ctx);
            if let Some(ref err) = form.error_message {
                let ui = dialog.ui();
                ui.colored_label(p.danger, err);
                ui.separator();
            }

            dialog.body(|ui| {
                // ---------------- 1. Action ----------------
                ui.heading("1. Action");
                tall_combo(ui, "trigger_action_combo")
                    .selected_text(form.action_tag.name())
                    .show_ui(ui, |ui| {
                        for tag in ActionTag::ALL {
                            ui.selectable_value(&mut form.action_tag, tag, tag.name());
                        }
                    });

                render_action_fields(&mut form, ui, slide, course);

                ui.add_space(8.0);
                ui.separator();

                // ---------------- 2. When ----------------
                ui.heading("2. When");
                tall_combo(ui, "trigger_event_combo")
                    .selected_text(form.event_tag.name())
                    .show_ui(ui, |ui| {
                        for tag in EventKindTag::ALL {
                            ui.selectable_value(&mut form.event_tag, tag, tag.name());
                        }
                    });

                render_event_fields(&mut form, ui, slide, course);

                ui.add_space(8.0);
                ui.separator();

                // ---------------- 3. Conditions ----------------
                ui.heading("3. Conditions (optional)");
                ui.checkbox(&mut form.condition_enabled, "Enable condition");
                if form.condition_enabled {
                    ui.indent("condition_tree_indent", |ui| {
                        render_condition_editor(&mut form.condition_root, ui, slide, course, 1);
                    });
                }
            });

            dialog.footer(|ui| {
                ui.add_space(coursecraft_render::SPACE_SM);
                ui.horizontal(|ui| {
                    if crate::primary_button(ui, "OK").clicked() {
                        commit_requested = true;
                    }
                    if ui.button("Cancel").clicked() {
                        close_dialog = true;
                    }
                });
            });
        });

    if !close_dialog && !commit_requested {
        if ctx.input_mut(|i| {
            i.consume_key(egui::Modifiers::COMMAND, egui::Key::Enter)
                || i.consume_key(egui::Modifiers::CTRL, egui::Key::Enter)
        }) {
            commit_requested = true;
        } else if !popup_was_open
            && !egui::Popup::is_any_open(ctx)
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            close_dialog = true;
        }
    }

    if commit_requested {
        let ok = form.commit_ok(editor);
        if ok {
            editor.ui.trigger_form = None;
        } else {
            editor.ui.trigger_form = Some(form);
        }
    } else if close_dialog {
        editor.ui.trigger_form = None;
    } else {
        editor.ui.trigger_form = Some(form);
    }
}

/// Every label of a trigger dialog's rows. The label column is as wide as the widest, so all
/// the fields of the dialog start at the same x.
const TRIGGER_ROW_LABELS: &[&str] = &[
    "Timer",
    "Target",
    "Slide",
    "Layer",
    "Object",
    "State",
    "Variable",
    "Operator",
    "Value",
    "Media object",
    "Status",
    "URL",
    "Path",
    "Timeline",
    "Time (ms)",
    "To state",
    "Video",
    "Cue",
    "Key",
    "Object (optional)",
    "Draggable object",
    "Object A",
    "Object B",
    "Animation",
];

/// One label-plus-field row of the trigger dialog.
fn trigger_row<R>(ui: &mut Ui, label: &str, add_field: impl FnOnce(&mut Ui) -> R) -> R {
    crate::dialog::Form::new(ui, TRIGGER_ROW_LABELS).row(ui, label, |ui, _| add_field(ui))
}

/// Pick one of the slide's timers (base layer and layers).
fn timer_picker(value: &mut String, ui: &mut Ui, slide: &Slide, salt: &str) {
    trigger_row(ui, "Timer", |ui| {
        tall_combo(ui, salt)
            .selected_text(object_label(slide, value))
            .show_ui(ui, |ui| {
                for obj in &slide.objects {
                    if matches!(obj.kind, coursecraft_course::ObjectKind::Timer { .. }) {
                        let label = object_name(slide, &obj.id);
                        ui.selectable_value(value, obj.id.clone(), label);
                    }
                }
                for l in &slide.layers {
                    for obj in &l.objects {
                        if matches!(obj.kind, coursecraft_course::ObjectKind::Timer { .. }) {
                            let label = object_label(slide, &obj.id);
                            ui.selectable_value(value, obj.id.clone(), label);
                        }
                    }
                }
            });
    });
}

pub(crate) fn render_action_fields(
    form: &mut TriggerFormState,
    ui: &mut Ui,
    slide: &Slide,
    course: &Course,
) {
    match form.action_tag {
        ActionTag::JumpToSlide => {
            trigger_row(ui, "Target", |ui| {
                ui.selectable_value(&mut form.jump_target_tag, JumpTargetTag::Next, "Next");
                ui.selectable_value(
                    &mut form.jump_target_tag,
                    JumpTargetTag::Previous,
                    "Previous",
                );
                ui.selectable_value(
                    &mut form.jump_target_tag,
                    JumpTargetTag::LastVisited,
                    "Last visited",
                );
                ui.selectable_value(
                    &mut form.jump_target_tag,
                    JumpTargetTag::SlideId,
                    "Specific slide",
                );
                ui.selectable_value(&mut form.jump_target_tag, JumpTargetTag::Scene, "Scene");
            });
            if form.jump_target_tag == JumpTargetTag::SlideId {
                trigger_row(ui, "Slide", |ui| {
                    tall_combo(ui, "action_jump_slide_combo")
                        .selected_text(slide_label(course, &form.jump_slide_id))
                        .show_ui(ui, |ui| {
                            for sc in &course.scenes {
                                for sl in &sc.slides {
                                    let label = slide_label(course, &sl.id);
                                    ui.selectable_value(
                                        &mut form.jump_slide_id,
                                        sl.id.clone(),
                                        label,
                                    );
                                }
                            }
                        });
                });
            }
            if form.jump_target_tag == JumpTargetTag::Scene {
                if form.jump_scene_id.is_empty()
                    && let Some(first) = course.scenes.first()
                {
                    form.jump_scene_id = first.id.clone();
                }
                let scene_label = course
                    .scenes
                    .iter()
                    .find(|s| s.id == form.jump_scene_id)
                    .map(|s| {
                        if s.title.is_empty() {
                            s.id.clone()
                        } else {
                            s.title.clone()
                        }
                    })
                    .unwrap_or_else(|| form.jump_scene_id.clone());
                trigger_row(ui, "Scene", |ui| {
                    tall_combo(ui, "action_jump_scene_combo")
                        .selected_text(scene_label)
                        .show_ui(ui, |ui| {
                            for sc in &course.scenes {
                                let label = if sc.title.is_empty() {
                                    sc.id.as_str()
                                } else {
                                    sc.title.as_str()
                                };
                                ui.selectable_value(&mut form.jump_scene_id, sc.id.clone(), label);
                            }
                        });
                });
            }
        }
        ActionTag::ShowLayer => {
            trigger_row(ui, "Layer", |ui| {
                tall_combo(ui, "action_show_layer_combo")
                    .selected_text(layer_name(slide, &form.show_layer_id))
                    .show_ui(ui, |ui| {
                        for l in &slide.layers {
                            let label = layer_name(slide, &l.id);
                            ui.selectable_value(&mut form.show_layer_id, l.id.clone(), label);
                        }
                    });
            });
        }
        ActionTag::HideLayer => {
            trigger_row(ui, "Layer", |ui| {
                let is_self = form.hide_layer_id.is_none();
                let display = match form.hide_layer_id.as_deref() {
                    Some(id) => layer_name(slide, id),
                    None => "This layer (owner)".to_owned(),
                };
                tall_combo(ui, "action_hide_layer_combo")
                    .selected_text(display)
                    .show_ui(ui, |ui| {
                        if ui.selectable_label(is_self, "This layer (owner)").clicked() {
                            form.hide_layer_id = None;
                        }
                        for l in &slide.layers {
                            let label = layer_name(slide, &l.id);
                            let sel = form.hide_layer_id.as_deref() == Some(&l.id);
                            if ui.selectable_label(sel, label).clicked() {
                                form.hide_layer_id = Some(l.id.clone());
                            }
                        }
                    });
            });
        }
        ActionTag::ChangeState => {
            trigger_row(ui, "Object", |ui| {
                tall_combo(ui, "action_change_state_obj_combo")
                    .selected_text(object_label(slide, &form.change_state_object))
                    .show_ui(ui, |ui| {
                        for obj in &slide.objects {
                            let label = object_name(slide, &obj.id);
                            ui.selectable_value(
                                &mut form.change_state_object,
                                obj.id.clone(),
                                label,
                            );
                        }
                        for l in &slide.layers {
                            for obj in &l.objects {
                                let label = object_label(slide, &obj.id);
                                ui.selectable_value(
                                    &mut form.change_state_object,
                                    obj.id.clone(),
                                    label,
                                );
                            }
                        }
                        for g in all_interactive_groups(slide) {
                            let label = object_label(slide, &g.id);
                            ui.selectable_value(&mut form.change_state_object, g.id.clone(), label);
                        }
                    });
            });

            // Find states available on this object or interactive group
            let available_states = available_states_for_target(slide, &form.change_state_object);
            // A state of the previous target may not exist on this one (`N101`).
            if !available_states.contains(&form.change_state_state) {
                form.change_state_state = available_states.first().cloned().unwrap_or_default();
            }

            trigger_row(ui, "State", |ui| {
                tall_combo(ui, "action_change_state_st_combo")
                    .selected_text(&form.change_state_state)
                    .show_ui(ui, |ui| {
                        for st in &available_states {
                            ui.selectable_value(&mut form.change_state_state, st.clone(), st);
                        }
                    });
            });
        }
        ActionTag::SetVariable => {
            trigger_row(ui, "Variable", |ui| {
                tall_combo(ui, "action_set_var_name_combo")
                    .selected_text(&form.set_var_name)
                    .show_ui(ui, |ui| {
                        for v in &course.variables {
                            ui.selectable_value(&mut form.set_var_name, v.name.clone(), &v.name);
                        }
                    });
            });

            let var_kind = course
                .variable(&form.set_var_name)
                .map(|v| v.value.kind())
                .unwrap_or(ValueKind::Number);

            let valid_ops = match var_kind {
                ValueKind::Bool => vec![VarOp::Assign, VarOp::Toggle],
                ValueKind::Number => vec![
                    VarOp::Assign,
                    VarOp::Add,
                    VarOp::Subtract,
                    VarOp::Multiply,
                    VarOp::Divide,
                ],
                ValueKind::Text => vec![VarOp::Assign],
            };

            // The operator of the previous variable's type may not fit this one (`N101`).
            if !valid_ops.contains(&form.set_var_op) {
                form.set_var_op = VarOp::Assign;
            }
            trigger_row(ui, "Operator", |ui| {
                let op_name = match form.set_var_op {
                    VarOp::Assign => "Assign (=)",
                    VarOp::Add => "Add (+)",
                    VarOp::Subtract => "Subtract (-)",
                    VarOp::Multiply => "Multiply (*)",
                    VarOp::Divide => "Divide (/)",
                    VarOp::Toggle => "Toggle",
                };
                tall_combo(ui, "action_set_var_op_combo")
                    .selected_text(op_name)
                    .show_ui(ui, |ui| {
                        for op in valid_ops {
                            let label = match op {
                                VarOp::Assign => "Assign (=)",
                                VarOp::Add => "Add (+)",
                                VarOp::Subtract => "Subtract (-)",
                                VarOp::Multiply => "Multiply (*)",
                                VarOp::Divide => "Divide (/)",
                                VarOp::Toggle => "Toggle",
                            };
                            ui.selectable_value(&mut form.set_var_op, op, label);
                        }
                    });
            });

            if form.set_var_op != VarOp::Toggle {
                trigger_row(ui, "Source", |ui| {
                    ui.selectable_value(&mut form.set_var_use_var, false, "A value");
                    ui.selectable_value(&mut form.set_var_use_var, true, "Another variable");
                });
                if form.set_var_use_var {
                    if form.set_var_src_var.is_empty() {
                        form.set_var_src_var = course
                            .variables
                            .first()
                            .map(|v| v.name.clone())
                            .or_else(|| {
                                coursecraft_course::BUILTIN_VARIABLES
                                    .first()
                                    .map(|&(n, _)| n.to_string())
                            })
                            .unwrap_or_default();
                    }
                    trigger_row(ui, "From variable", |ui| {
                        tall_combo(ui, "action_set_var_src_combo")
                            .selected_text(&form.set_var_src_var)
                            .show_ui(ui, |ui| {
                                for v in &course.variables {
                                    ui.selectable_value(
                                        &mut form.set_var_src_var,
                                        v.name.clone(),
                                        &v.name,
                                    );
                                }
                                if !course.variables.is_empty()
                                    && !coursecraft_course::BUILTIN_VARIABLES.is_empty()
                                {
                                    ui.separator();
                                }
                                for &(name, _) in coursecraft_course::BUILTIN_VARIABLES {
                                    ui.selectable_value(
                                        &mut form.set_var_src_var,
                                        name.to_string(),
                                        name,
                                    );
                                }
                            });
                    });
                } else {
                    trigger_row(ui, "Value", |ui| match var_kind {
                        ValueKind::Bool => {
                            ui.checkbox(&mut form.set_var_val_bool, "true / false");
                        }
                        ValueKind::Number => {
                            ui.add(DragValue::new(&mut form.set_var_val_num).speed(1.0));
                        }
                        ValueKind::Text => {
                            ui.add(TextEdit::singleline(&mut form.set_var_val_text));
                        }
                    });
                }
            }
        }
        ActionTag::PlayMedia | ActionTag::PauseMedia | ActionTag::StopMedia => {
            trigger_row(ui, "Media object", |ui| {
                tall_combo(ui, "action_media_obj_combo")
                    .selected_text(media_object_label(slide, &form.media_object))
                    .show_ui(ui, |ui| {
                        if course.background_audio.is_some() {
                            ui.selectable_value(
                                &mut form.media_object,
                                BACKGROUND_AUDIO_ID.to_owned(),
                                "Background audio",
                            );
                        }
                        for obj in &slide.objects {
                            let label = object_name(slide, &obj.id);
                            ui.selectable_value(&mut form.media_object, obj.id.clone(), label);
                        }
                        for l in &slide.layers {
                            for obj in &l.objects {
                                let label = object_label(slide, &obj.id);
                                ui.selectable_value(&mut form.media_object, obj.id.clone(), label);
                            }
                        }
                    });
            });
        }
        ActionTag::CompleteCourse => {
            trigger_row(ui, "Status", |ui| {
                let label = match form.complete_status {
                    CompletionStatus::Passed => "Passed",
                    CompletionStatus::Failed => "Failed",
                    CompletionStatus::Completed => "Completed",
                    CompletionStatus::Incomplete => "Incomplete",
                };
                tall_combo(ui, "action_complete_status_combo")
                    .selected_text(label)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut form.complete_status,
                            CompletionStatus::Passed,
                            "Passed",
                        );
                        ui.selectable_value(
                            &mut form.complete_status,
                            CompletionStatus::Failed,
                            "Failed",
                        );
                        ui.selectable_value(
                            &mut form.complete_status,
                            CompletionStatus::Completed,
                            "Completed",
                        );
                        ui.selectable_value(
                            &mut form.complete_status,
                            CompletionStatus::Incomplete,
                            "Incomplete",
                        );
                    });
            });
        }
        ActionTag::OpenUrl => {
            trigger_row(ui, "URL", |ui| {
                ui.add(TextEdit::singleline(&mut form.open_url).desired_width(300.0));
            });
        }
        ActionTag::MoveOnPath => {
            trigger_row(ui, "Object", |ui| {
                tall_combo(ui, "action_move_path_obj_combo")
                    .selected_text(object_label(slide, &form.move_path_object))
                    .show_ui(ui, |ui| {
                        for obj in &slide.objects {
                            let label = object_name(slide, &obj.id);
                            ui.selectable_value(&mut form.move_path_object, obj.id.clone(), label);
                        }
                        for l in &slide.layers {
                            for obj in &l.objects {
                                let label = object_label(slide, &obj.id);
                                ui.selectable_value(
                                    &mut form.move_path_object,
                                    obj.id.clone(),
                                    label,
                                );
                            }
                        }
                    });
            });
            trigger_row(ui, "Path", |ui| {
                let paths: Vec<String> = slide
                    .object(&form.move_path_object)
                    .map(|(o, _)| o.motion_paths.iter().map(|p| p.id.clone()).collect())
                    .unwrap_or_default();
                // A path of the previous object does not exist on this one (`N101`).
                if !paths.contains(&form.move_path_id) {
                    form.move_path_id = paths.first().cloned().unwrap_or_default();
                }
                let current_text = if form.move_path_id.is_empty() {
                    "Select path".to_owned()
                } else {
                    form.move_path_id.clone()
                };
                tall_combo(ui, "action_move_path_id_combo")
                    .selected_text(current_text)
                    .show_ui(ui, |ui| {
                        for p in paths {
                            ui.selectable_value(&mut form.move_path_id, p.clone(), p);
                        }
                    });
            });
        }
        ActionTag::RunJavaScript => {
            ui.label("Code");
            let width = ui.available_width().max(120.0);
            egui::Resize::default()
                .id_salt("trigger_javascript_resize")
                .default_width(width)
                .default_height(160.0)
                .min_height(80.0)
                .show(ui, |ui| {
                    let response = ui.add_sized(
                        [ui.available_width(), ui.available_height().max(72.0)],
                        TextEdit::multiline(&mut form.javascript_code)
                            .id_salt("trigger_javascript_code")
                            .code_editor()
                            .desired_rows(8),
                    );
                    if response.changed() && form.javascript_code.contains('\t') {
                        form.javascript_code = form.javascript_code.replace('\t', "  ");
                    }
                });
            if form.javascript_code.len() > coursecraft_course::MAX_JAVASCRIPT_BYTES {
                ui.colored_label(
                    coursecraft_render::palette(ui.ctx()).danger,
                    format!("At most {} bytes", coursecraft_course::MAX_JAVASCRIPT_BYTES),
                );
            }
            ui.label(
                RichText::new("Runs only in the published course")
                    .small()
                    .color(coursecraft_render::palette(ui.ctx()).text_secondary),
            );
        }
        ActionTag::LightboxSlide => {
            trigger_row(ui, "Slide", |ui| {
                tall_combo(ui, "action_lightbox_slide_combo")
                    .selected_text(slide_label(course, &form.lightbox_slide_id))
                    .show_ui(ui, |ui| {
                        for scene in &course.scenes {
                            for candidate in &scene.slides {
                                if candidate.id == form.owner_slide {
                                    continue;
                                }
                                let label = slide_label(course, &candidate.id);
                                ui.selectable_value(
                                    &mut form.lightbox_slide_id,
                                    candidate.id.clone(),
                                    label,
                                );
                            }
                        }
                    });
            });
        }
        ActionTag::StartTimer | ActionTag::PauseTimer | ActionTag::ResetTimer => {
            timer_picker(&mut form.timer_object, ui, slide, "action_timer_obj_combo");
        }
        ActionTag::PauseTimeline
        | ActionTag::ResumeTimeline
        | ActionTag::SubmitQuestion
        | ActionTag::SubmitResults
        | ActionTag::RetryQuiz
        | ActionTag::ReviewQuiz
        | ActionTag::ExitReview
        | ActionTag::PrintSlide
        | ActionTag::ExitCourse
        | ActionTag::CloseLightbox => {}
    }
}

fn render_event_fields(form: &mut TriggerFormState, ui: &mut Ui, slide: &Slide, course: &Course) {
    match form.event_tag {
        EventKindTag::Click
        | EventKindTag::HoverStart
        | EventKindTag::HoverEnd
        | EventKindTag::MarkerOpened
        | EventKindTag::MarkerClosed => {
            let include_groups = matches!(
                form.event_tag,
                EventKindTag::Click | EventKindTag::HoverStart | EventKindTag::HoverEnd
            );
            trigger_row(ui, "Object", |ui| {
                tall_combo(ui, "event_obj_combo")
                    .selected_text(object_label(slide, &form.event_object))
                    .show_ui(ui, |ui| {
                        for obj in &slide.objects {
                            let label = object_name(slide, &obj.id);
                            ui.selectable_value(&mut form.event_object, obj.id.clone(), label);
                        }
                        for l in &slide.layers {
                            for obj in &l.objects {
                                let label = object_label(slide, &obj.id);
                                ui.selectable_value(&mut form.event_object, obj.id.clone(), label);
                            }
                        }
                        if include_groups {
                            for g in all_interactive_groups(slide) {
                                let label = object_label(slide, &g.id);
                                ui.selectable_value(&mut form.event_object, g.id.clone(), label);
                            }
                        }
                    });
            });
        }
        EventKindTag::TimelineStarts | EventKindTag::TimelineEnds => {
            trigger_row(ui, "Timeline", |ui| {
                let is_base = form.event_layer.is_none();
                let display = match form.event_layer.as_deref() {
                    Some(id) => format!("Layer: {}", layer_name(slide, id)),
                    None => "Slide base timeline".to_owned(),
                };
                tall_combo(ui, "event_layer_combo")
                    .selected_text(display)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(is_base, "Slide base timeline")
                            .clicked()
                        {
                            form.event_layer = None;
                        }
                        for l in &slide.layers {
                            let label = format!("Layer: {}", layer_name(slide, &l.id));
                            let sel = form.event_layer.as_deref() == Some(&l.id);
                            if ui.selectable_label(sel, label).clicked() {
                                form.event_layer = Some(l.id.clone());
                            }
                        }
                    });
            });
        }
        EventKindTag::TimelineReaches => {
            trigger_row(ui, "Timeline", |ui| {
                let is_base = form.event_layer.is_none();
                let display = match form.event_layer.as_deref() {
                    Some(id) => format!("Layer: {}", layer_name(slide, id)),
                    None => "Slide base timeline".to_owned(),
                };
                tall_combo(ui, "event_reach_layer_combo")
                    .selected_text(display)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(is_base, "Slide base timeline")
                            .clicked()
                        {
                            form.event_layer = None;
                            form.event_timeline_cue = None;
                        }
                        for l in &slide.layers {
                            let label = format!("Layer: {}", layer_name(slide, &l.id));
                            let sel = form.event_layer.as_deref() == Some(&l.id);
                            if ui.selectable_label(sel, label).clicked() {
                                form.event_layer = Some(l.id.clone());
                                form.event_timeline_cue = None;
                            }
                        }
                    });
            });
            let cues: &[coursecraft_course::SlideCue] = match form.event_layer.as_deref() {
                Some(lid) => slide.layer(lid).map_or(&[], |l| l.cue_points.as_slice()),
                None => slide.cue_points.as_slice(),
            };
            if !cues.is_empty() {
                trigger_row(ui, "At", |ui| {
                    let cue_text = match form.event_timeline_cue.as_deref() {
                        Some(cid) => cues
                            .iter()
                            .find(|c| c.id == cid)
                            .map_or_else(|| cid.to_owned(), |c| format!("Cue: {}", c.name)),
                        None => "Specific time (ms)".to_owned(),
                    };
                    tall_combo(ui, "event_reach_cue_combo")
                        .selected_text(cue_text)
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_label(
                                    form.event_timeline_cue.is_none(),
                                    "Specific time (ms)",
                                )
                                .clicked()
                            {
                                form.event_timeline_cue = None;
                            }
                            for c in cues {
                                let sel = form.event_timeline_cue.as_deref() == Some(c.id.as_str());
                                if ui
                                    .selectable_label(
                                        sel,
                                        format!("Cue: {} ({} ms)", c.name, c.at_ms),
                                    )
                                    .clicked()
                                {
                                    form.event_timeline_cue = Some(c.id.clone());
                                    form.event_ms = c.at_ms;
                                }
                            }
                        });
                });
            }
            if form.event_timeline_cue.is_none() {
                trigger_row(ui, "Time (ms)", |ui| {
                    ui.add(
                        DragValue::new(&mut form.event_ms)
                            .speed(50.0)
                            .range(0..=86_400_000)
                            .suffix(" ms"),
                    );
                });
            }
        }
        EventKindTag::VariableChanges => {
            trigger_row(ui, "Variable", |ui| {
                tall_combo(ui, "event_var_combo")
                    .selected_text(&form.event_var)
                    .show_ui(ui, |ui| {
                        for v in &course.variables {
                            ui.selectable_value(&mut form.event_var, v.name.clone(), &v.name);
                        }
                    });
            });
        }
        EventKindTag::StateChanges => {
            trigger_row(ui, "Object", |ui| {
                tall_combo(ui, "event_state_obj_combo")
                    .selected_text(object_label(slide, &form.event_object))
                    .show_ui(ui, |ui| {
                        for obj in &slide.objects {
                            let label = object_name(slide, &obj.id);
                            ui.selectable_value(&mut form.event_object, obj.id.clone(), label);
                        }
                        for l in &slide.layers {
                            for obj in &l.objects {
                                let label = object_label(slide, &obj.id);
                                ui.selectable_value(&mut form.event_object, obj.id.clone(), label);
                            }
                        }
                        for g in all_interactive_groups(slide) {
                            let label = object_label(slide, &g.id);
                            ui.selectable_value(&mut form.event_object, g.id.clone(), label);
                        }
                    });
            });

            let available_states = available_states_for_target(slide, &form.event_object);

            trigger_row(ui, "To state", |ui| {
                let display = form.event_state_to.as_deref().unwrap_or("Any state change");
                let is_any = form.event_state_to.is_none();
                tall_combo(ui, "event_state_to_combo")
                    .selected_text(display)
                    .show_ui(ui, |ui| {
                        if ui.selectable_label(is_any, "Any state change").clicked() {
                            form.event_state_to = None;
                        }
                        for st in &available_states {
                            let sel = form.event_state_to.as_deref() == Some(st.as_str());
                            if ui.selectable_label(sel, st).clicked() {
                                form.event_state_to = Some(st.clone());
                            }
                        }
                    });
            });
        }
        EventKindTag::MediaEnds => {
            trigger_row(ui, "Media object", |ui| {
                tall_combo(ui, "event_media_ends_obj_combo")
                    .selected_text(media_object_label(slide, &form.event_object))
                    .show_ui(ui, |ui| {
                        if course.background_audio.is_some() {
                            ui.selectable_value(
                                &mut form.event_object,
                                BACKGROUND_AUDIO_ID.to_owned(),
                                "Background audio",
                            );
                        }
                        for obj in &slide.objects {
                            let label = object_name(slide, &obj.id);
                            ui.selectable_value(&mut form.event_object, obj.id.clone(), label);
                        }
                        for l in &slide.layers {
                            for obj in &l.objects {
                                let label = object_label(slide, &obj.id);
                                ui.selectable_value(&mut form.event_object, obj.id.clone(), label);
                            }
                        }
                    });
            });
        }
        EventKindTag::CueReached => {
            let videos: Vec<&coursecraft_course::Object> = all_objects(slide)
                .into_iter()
                .filter(|object| {
                    matches!(&object.kind, coursecraft_course::ObjectKind::Video { cues, .. } if !cues.is_empty())
                })
                .collect();
            if !videos.iter().any(|object| object.id == form.event_object) {
                form.event_object = videos
                    .first()
                    .map(|object| object.id.clone())
                    .unwrap_or_default();
            }
            let cues: Vec<coursecraft_course::Cue> = videos
                .iter()
                .find(|object| object.id == form.event_object)
                .and_then(|object| match &object.kind {
                    coursecraft_course::ObjectKind::Video { cues, .. } => Some(cues.clone()),
                    _ => None,
                })
                .unwrap_or_default();
            if !cues.iter().any(|cue| cue.id == form.event_cue) {
                form.event_cue = cues.first().map(|cue| cue.id.clone()).unwrap_or_default();
            }
            trigger_row(ui, "Video", |ui| {
                tall_combo(ui, "event_cue_video_combo")
                    .selected_text(object_label(slide, &form.event_object))
                    .show_ui(ui, |ui| {
                        for object in &videos {
                            let label = object_label(slide, &object.id);
                            ui.selectable_value(&mut form.event_object, object.id.clone(), label);
                        }
                    });
            });
            trigger_row(ui, "Cue", |ui| {
                let cue_label = cues
                    .iter()
                    .find(|cue| cue.id == form.event_cue)
                    .map(|cue| format!("{} ({} ms)", cue.name, cue.at_ms_u64()))
                    .unwrap_or_else(|| "No cues".to_owned());
                tall_combo(ui, "event_cue_combo")
                    .selected_text(cue_label)
                    .show_ui(ui, |ui| {
                        for cue in &cues {
                            ui.selectable_value(
                                &mut form.event_cue,
                                cue.id.clone(),
                                format!("{} ({} ms)", cue.name, cue.at_ms_u64()),
                            );
                        }
                    });
            });
        }
        EventKindTag::KeyPress => {
            trigger_row(ui, "Key", |ui| {
                tall_combo(ui, "event_key_pressed_combo")
                    .selected_text(&form.event_key)
                    .show_ui(ui, |ui| {
                        for k in coursecraft_course::VALID_TRIGGER_KEYS {
                            ui.selectable_value(&mut form.event_key, (*k).to_string(), *k);
                        }
                    });
            });
            trigger_row(ui, "Object (optional)", |ui| {
                let current_label = match &form.event_key_object {
                    Some(obj) => object_label(slide, obj),
                    None => "Anywhere on slide".to_string(),
                };
                tall_combo(ui, "event_key_pressed_obj_combo")
                    .selected_text(current_label)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut form.event_key_object, None, "Anywhere on slide");
                        for obj in all_objects(slide) {
                            let label = object_label(slide, &obj.id);
                            ui.selectable_value(
                                &mut form.event_key_object,
                                Some(obj.id.clone()),
                                label,
                            );
                        }
                        for g in all_interactive_groups(slide) {
                            let label = object_label(slide, &g.id);
                            ui.selectable_value(
                                &mut form.event_key_object,
                                Some(g.id.clone()),
                                label,
                            );
                        }
                    });
            });
        }
        EventKindTag::DroppedOn => {
            let draggables: Vec<&coursecraft_course::Object> = all_objects(slide)
                .into_iter()
                .filter(|object| object.drag.is_some())
                .collect();
            if !draggables
                .iter()
                .any(|object| object.id == form.event_object)
            {
                form.event_object = draggables
                    .first()
                    .map(|object| object.id.clone())
                    .unwrap_or_default();
            }
            trigger_row(ui, "Draggable object", |ui| {
                tall_combo(ui, "event_dropped_on_obj_combo")
                    .selected_text(object_label(slide, &form.event_object))
                    .show_ui(ui, |ui| {
                        for obj in &draggables {
                            let label = object_label(slide, &obj.id);
                            ui.selectable_value(&mut form.event_object, obj.id.clone(), label);
                        }
                    });
            });
            let targets: Vec<String> = slide
                .object(&form.event_object)
                .and_then(|(object, _)| object.drag.as_ref())
                .map(|drag| drag.targets.clone())
                .unwrap_or_default();
            if !targets.contains(&form.event_drop_target) {
                form.event_drop_target = targets.first().cloned().unwrap_or_default();
            }
            trigger_row(ui, "Target", |ui| {
                tall_combo(ui, "event_dropped_on_tgt_combo")
                    .selected_text(object_label(slide, &form.event_drop_target))
                    .show_ui(ui, |ui| {
                        for target in &targets {
                            let label = object_label(slide, target);
                            ui.selectable_value(&mut form.event_drop_target, target.clone(), label);
                        }
                    });
            });
        }
        EventKindTag::ObjectsIntersect | EventKindTag::ObjectsStopIntersecting => {
            let objects = all_objects(slide);
            if !objects.iter().any(|object| object.id == form.event_object) {
                form.event_object = objects
                    .first()
                    .map(|object| object.id.clone())
                    .unwrap_or_default();
            }
            if form.event_intersect_other == form.event_object
                || !objects
                    .iter()
                    .any(|object| object.id == form.event_intersect_other)
            {
                form.event_intersect_other = objects
                    .iter()
                    .find(|object| object.id != form.event_object)
                    .map(|object| object.id.clone())
                    .unwrap_or_default();
            }
            trigger_row(ui, "Object A", |ui| {
                tall_combo(ui, "event_intersect_obj_a_combo")
                    .selected_text(object_label(slide, &form.event_object))
                    .show_ui(ui, |ui| {
                        for obj in &objects {
                            let label = object_label(slide, &obj.id);
                            ui.selectable_value(&mut form.event_object, obj.id.clone(), label);
                        }
                    });
            });
            trigger_row(ui, "Object B", |ui| {
                tall_combo(ui, "event_intersect_obj_b_combo")
                    .selected_text(object_label(slide, &form.event_intersect_other))
                    .show_ui(ui, |ui| {
                        for obj in &objects {
                            if obj.id == form.event_object {
                                continue;
                            }
                            let label = object_label(slide, &obj.id);
                            ui.selectable_value(
                                &mut form.event_intersect_other,
                                obj.id.clone(),
                                label,
                            );
                        }
                    });
            });
        }
        EventKindTag::Submit => {}
        EventKindTag::AnimationCompletes => {
            trigger_row(ui, "Object", |ui| {
                tall_combo(ui, "event_anim_complete_obj_combo")
                    .selected_text(object_label(slide, &form.anim_complete_object))
                    .show_ui(ui, |ui| {
                        for obj in &slide.objects {
                            let label = object_name(slide, &obj.id);
                            ui.selectable_value(
                                &mut form.anim_complete_object,
                                obj.id.clone(),
                                label,
                            );
                        }
                        for l in &slide.layers {
                            for obj in &l.objects {
                                let label = object_label(slide, &obj.id);
                                ui.selectable_value(
                                    &mut form.anim_complete_object,
                                    obj.id.clone(),
                                    label,
                                );
                            }
                        }
                    });
            });
            trigger_row(ui, "Animation", |ui| {
                let mut options = vec![("entrance".to_owned(), "Entrance".to_owned())];
                if let Some((o, _)) = slide.object(&form.anim_complete_object) {
                    for p in &o.motion_paths {
                        options.push((p.id.clone(), format!("Path: {}", p.id)));
                    }
                }
                // A path of the previous object does not exist on this one (`N101`).
                if !options
                    .iter()
                    .any(|(val, _)| *val == form.anim_complete_which)
                {
                    form.anim_complete_which = "entrance".to_owned();
                }
                let current_text = if form.anim_complete_which.is_empty()
                    || form.anim_complete_which == "entrance"
                {
                    "Entrance".to_owned()
                } else {
                    format!("Path: {}", form.anim_complete_which)
                };
                tall_combo(ui, "event_anim_complete_which_combo")
                    .selected_text(current_text)
                    .show_ui(ui, |ui| {
                        for (val, label) in options {
                            ui.selectable_value(&mut form.anim_complete_which, val, label);
                        }
                    });
            });
        }
        EventKindTag::TimerEnds => {
            timer_picker(
                &mut form.event_timer_object,
                ui,
                slide,
                "event_timer_obj_combo",
            );
        }
    }
}

pub(crate) fn render_condition_editor(
    node: &mut ConditionNode,
    ui: &mut Ui,
    slide: &Slide,
    course: &Course,
    depth: usize,
) {
    let snapshot = node.clone();
    let mut replacement: Option<ConditionNode> = None;
    match node {
        ConditionNode::Compare {
            variable,
            op,
            value,
        } => {
            ui.horizontal_wrapped(|ui| {
                condition_kind_picker(&snapshot, ui, slide, course, depth, &mut replacement);
                tall_combo(ui, ("var_picker", depth))
                    .selected_text(variable.as_str())
                    .show_ui(ui, |ui| {
                        for v in &course.variables {
                            ui.selectable_value(variable, v.name.clone(), &v.name);
                        }
                        if !course.variables.is_empty()
                            && !coursecraft_course::BUILTIN_VARIABLES.is_empty()
                        {
                            ui.separator();
                        }
                        for &(name, _) in coursecraft_course::BUILTIN_VARIABLES {
                            ui.selectable_value(variable, name.to_string(), name);
                        }
                    });

                let var_kind = coursecraft_course::builtin_variable_kind(variable)
                    .or_else(|| course.variable(variable).map(|v| v.value.kind()))
                    .unwrap_or(ValueKind::Number);

                let ops = match var_kind {
                    ValueKind::Number => vec![
                        CmpOp::Eq,
                        CmpOp::Ne,
                        CmpOp::Lt,
                        CmpOp::Le,
                        CmpOp::Gt,
                        CmpOp::Ge,
                    ],
                    ValueKind::Text => vec![
                        CmpOp::Eq,
                        CmpOp::Ne,
                        CmpOp::Contains,
                        CmpOp::EqIgnoreCase,
                        CmpOp::ContainsIgnoreCase,
                    ],
                    ValueKind::Bool => vec![CmpOp::Eq, CmpOp::Ne],
                };

                // Keep the stored constant value and operator valid for the chosen variable's type.
                if let ValueSource::Constant(const_val) = value
                    && const_val.kind() != var_kind
                {
                    *const_val = match var_kind {
                        ValueKind::Number => Value::Number(0.0),
                        ValueKind::Bool => Value::Bool(true),
                        ValueKind::Text => Value::Text(String::new()),
                    };
                }
                if !ops.contains(op) {
                    *op = CmpOp::Eq;
                }

                let cmp_op_label = |o: CmpOp| match o {
                    CmpOp::Eq => "==",
                    CmpOp::Ne => "!=",
                    CmpOp::Lt => "<",
                    CmpOp::Le => "<=",
                    CmpOp::Gt => ">",
                    CmpOp::Ge => ">=",
                    CmpOp::Contains => "contains",
                    CmpOp::EqIgnoreCase => "equals (ignore case)",
                    CmpOp::ContainsIgnoreCase => "contains (ignore case)",
                };

                tall_combo(ui, ("op_picker", depth))
                    .selected_text(cmp_op_label(*op))
                    .show_ui(ui, |ui| {
                        for o in ops {
                            ui.selectable_value(op, o, cmp_op_label(o));
                        }
                    });

                let mut is_var = matches!(value, ValueSource::Variable { .. });
                let prev_is_var = is_var;
                tall_combo(ui, ("cmp_source_picker", depth))
                    .selected_text(if is_var {
                        "Another variable"
                    } else {
                        "A value"
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut is_var, false, "A value");
                        ui.selectable_value(&mut is_var, true, "Another variable");
                    });
                if is_var != prev_is_var {
                    if is_var {
                        let first_rhs = course
                            .variables
                            .first()
                            .map(|v| v.name.clone())
                            .or_else(|| {
                                coursecraft_course::BUILTIN_VARIABLES
                                    .first()
                                    .map(|&(n, _)| n.to_string())
                            })
                            .unwrap_or_default();
                        *value = ValueSource::Variable {
                            variable: first_rhs,
                        };
                    } else {
                        let default_val = match var_kind {
                            ValueKind::Number => Value::Number(0.0),
                            ValueKind::Bool => Value::Bool(true),
                            ValueKind::Text => Value::Text(String::new()),
                        };
                        *value = ValueSource::Constant(default_val);
                    }
                }

                match value {
                    ValueSource::Variable { variable: rhs_var } => {
                        tall_combo(ui, ("cmp_rhs_var_picker", depth))
                            .selected_text(rhs_var.as_str())
                            .show_ui(ui, |ui| {
                                for v in &course.variables {
                                    ui.selectable_value(rhs_var, v.name.clone(), &v.name);
                                }
                                if !course.variables.is_empty()
                                    && !coursecraft_course::BUILTIN_VARIABLES.is_empty()
                                {
                                    ui.separator();
                                }
                                for &(name, _) in coursecraft_course::BUILTIN_VARIABLES {
                                    ui.selectable_value(rhs_var, name.to_string(), name);
                                }
                            });
                    }
                    ValueSource::Constant(const_val) => match var_kind {
                        ValueKind::Bool => {
                            let mut b = match const_val {
                                Value::Bool(bv) => *bv,
                                _ => false,
                            };
                            if ui.checkbox(&mut b, "true / false").changed() {
                                *const_val = Value::Bool(b);
                            }
                        }
                        ValueKind::Number => {
                            let mut n = match const_val {
                                Value::Number(nv) => *nv,
                                _ => 0.0,
                            };
                            if ui.add(DragValue::new(&mut n).speed(1.0)).changed() {
                                *const_val = Value::Number(n);
                            }
                        }
                        ValueKind::Text => {
                            let mut t = match const_val {
                                Value::Text(tv) => tv.clone(),
                                _ => String::new(),
                            };
                            let w = ui.available_width().clamp(80.0, 180.0);
                            if ui
                                .add(TextEdit::singleline(&mut t).desired_width(w))
                                .changed()
                            {
                                *const_val = Value::Text(t);
                            }
                        }
                    },
                }
            });
        }
        ConditionNode::ObjectState { object, state, is } => {
            ui.horizontal_wrapped(|ui| {
                condition_kind_picker(&snapshot, ui, slide, course, depth, &mut replacement);
                tall_combo(ui, ("obj_state_picker", depth))
                    .selected_text(object_label(slide, object))
                    .show_ui(ui, |ui| {
                        for obj in &slide.objects {
                            let label = object_name(slide, &obj.id);
                            ui.selectable_value(object, obj.id.clone(), label);
                        }
                        for l in &slide.layers {
                            for obj in &l.objects {
                                let label = object_label(slide, &obj.id);
                                ui.selectable_value(object, obj.id.clone(), label);
                            }
                        }
                        for g in all_interactive_groups(slide) {
                            let label = object_label(slide, &g.id);
                            ui.selectable_value(object, g.id.clone(), label);
                        }
                    });

                let is_label = if *is { "is" } else { "is not" };
                tall_combo(ui, ("is_picker", depth))
                    .selected_text(is_label)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(is, true, "is");
                        ui.selectable_value(is, false, "is not");
                    });

                let available_states = available_states_for_target(slide, object);

                tall_combo(ui, ("state_name_picker", depth))
                    .selected_text(state.as_str())
                    .show_ui(ui, |ui| {
                        for st in &available_states {
                            ui.selectable_value(state, st.clone(), st);
                        }
                    });
            });
        }
        ConditionNode::All(list) => {
            ui.horizontal(|ui| {
                condition_kind_picker(&snapshot, ui, slide, course, depth, &mut replacement);
                ui.label(RichText::new("the following").strong());
            });
            render_condition_group(list, ui, slide, course, depth);
        }
        ConditionNode::Any(list) => {
            ui.horizontal(|ui| {
                condition_kind_picker(&snapshot, ui, slide, course, depth, &mut replacement);
                ui.label(RichText::new("the following").strong());
            });
            render_condition_group(list, ui, slide, course, depth);
        }
        ConditionNode::Not(inner) => {
            ui.horizontal(|ui| {
                condition_kind_picker(&snapshot, ui, slide, course, depth, &mut replacement);
                ui.label(RichText::new("the following").strong());
            });
            ui.indent("not_child_indent", |ui| {
                render_condition_editor(inner, ui, slide, course, depth + 1);
            });
        }
    }
    if let Some(new_node) = replacement {
        *node = new_node;
    }
}

/// Kind selector shown on every condition node (including the root). Sets `replacement` when
/// the user picks a different kind. Kinds whose conversion would nest deeper than
/// [`coursecraft_course::MAX_CONDITION_DEPTH`] are not offered.
fn condition_kind_picker(
    node: &ConditionNode,
    ui: &mut Ui,
    slide: &Slide,
    course: &Course,
    depth: usize,
    replacement: &mut Option<ConditionNode>,
) {
    let current = node.kind();
    tall_combo(ui, ("cond_kind_picker", depth))
        .selected_text(current.name())
        .width(130.0)
        .show_ui(ui, |ui| {
            for kind in ConditionKind::ALL {
                if kind != current {
                    let converted = node.convert_to(kind, slide, course);
                    // The deepest leaf of this subtree sits at `depth + converted.depth() - 1`.
                    if depth + converted.depth() - 1 > coursecraft_course::MAX_CONDITION_DEPTH {
                        continue;
                    }
                }
                if ui.selectable_label(kind == current, kind.name()).clicked() && kind != current {
                    *replacement = Some(node.convert_to(kind, slide, course));
                }
            }
        });
}

fn render_condition_group(
    list: &mut Vec<ConditionNode>,
    ui: &mut Ui,
    slide: &Slide,
    course: &Course,
    depth: usize,
) {
    let mut to_remove = None;
    for (idx, child) in list.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            if crate::widgets::icon_button_danger(
                ui,
                coursecraft_render::Icon::Trash,
                true,
                "Remove condition",
            )
            .clicked()
            {
                to_remove = Some(idx);
            }
            ui.vertical(|ui| {
                ui.indent(("child_indent", depth, idx), |ui| {
                    render_condition_editor(child, ui, slide, course, depth + 1);
                });
            });
        });
    }
    if let Some(idx) = to_remove
        && idx < list.len()
    {
        list.remove(idx);
    }

    if depth < coursecraft_course::MAX_CONDITION_DEPTH {
        ui.horizontal(|ui| {
            if ui.button("+ Add comparison").clicked() {
                let first_var = course
                    .variables
                    .first()
                    .map(|v| v.name.clone())
                    .or_else(|| {
                        coursecraft_course::BUILTIN_VARIABLES
                            .first()
                            .map(|&(n, _)| n.to_string())
                    })
                    .unwrap_or_default();
                list.push(ConditionNode::Compare {
                    variable: first_var,
                    op: CmpOp::Eq,
                    value: ValueSource::Constant(Value::Number(0.0)),
                });
            }
            if ui.button("+ Add object state").clicked() {
                let first_obj = slide
                    .objects
                    .first()
                    .map(|o| o.id.clone())
                    .unwrap_or_default();
                list.push(ConditionNode::ObjectState {
                    object: first_obj,
                    state: "visited".to_owned(),
                    is: true,
                });
            }
        });
    }
}
