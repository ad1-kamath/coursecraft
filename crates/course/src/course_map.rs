//! Course map graph (L1): every way a slide can lead to another slide.
//!
//! Pure data derived from the course on each call, never stored: triggers
//! that jump or lightbox, text links, question pass/fail layer triggers
//! and the player's Next. The editor's Course map and the `course.map`
//! engine command both read this, so they can never disagree.
//!
//! Masters and templates are never in play order and never appear here.

use serde::{Deserialize, Serialize};

use crate::{Action, CmpOp, Condition, Course, Event, Link, ObjectKind, SlideTarget, Value};

/// What an edge means, for colouring and for the CLI's `kind` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MapEdgeKind {
    /// A `jump_to_slide` trigger.
    Jump,
    /// A `lightbox_slide` trigger.
    Lightbox,
    /// A slide link in a text run.
    Link,
    /// The player's Next button, drawn only when no other edge from the
    /// same slide already reaches the next slide.
    Next,
}

impl MapEdgeKind {
    /// Stable lowercase name used in JSON and the legend.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Jump => "jump",
            Self::Lightbox => "lightbox",
            Self::Link => "link",
            Self::Next => "next",
        }
    }
}

/// One way to travel from one slide to another.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapEdge {
    /// Slide the learner is on.
    pub from: String,
    /// Slide the learner arrives on.
    pub to: String,
    pub kind: MapEdgeKind,
    /// Where the edge comes from: `slide`, `layer:<layer id>`,
    /// `button:<object id>`, `question_pass`, `question_fail`,
    /// `link:<object id>` or `player_next`.
    pub via: String,
    /// The trigger condition in words, when the edge is conditional.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
}

/// One slide in the map's scene column.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapSlide {
    pub id: String,
    pub title: String,
    /// Nothing leads here and this is not an entry slide (the first slide
    /// or `start_slide`).
    pub unreachable: bool,
}

/// One scene column in the map.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapScene {
    pub id: String,
    pub title: String,
    pub slides: Vec<MapSlide>,
}

/// The whole course graph as JSON for `course.map` and the Course map.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CourseMap {
    pub scenes: Vec<MapScene>,
    pub edges: Vec<MapEdge>,
}

/// Every way a slide can lead to another, in course order.
#[must_use]
pub fn course_map(course: &Course) -> CourseMap {
    let play_ids: Vec<String> = course.slides().map(|slide| slide.id.clone()).collect();
    let mut edges: Vec<MapEdge> = Vec::new();

    for (index, slide) in course.slides().enumerate() {
        let from = slide.id.clone();
        let correct_layer = slide
            .question
            .as_ref()
            .and_then(|question| question.correct_layer.clone());
        let incorrect_layer = slide
            .question
            .as_ref()
            .and_then(|question| question.incorrect_layer.clone());

        // Triggers on the base layer, then on each layer in order.
        let owners: Vec<(&str, &Vec<crate::Trigger>)> = std::iter::once(("", &slide.triggers))
            .chain(
                slide
                    .layers
                    .iter()
                    .map(|layer| (layer.id.as_str(), &layer.triggers)),
            )
            .collect();
        for (layer_id, triggers) in owners {
            for trigger in triggers {
                let via = edge_via(
                    layer_id,
                    correct_layer.as_deref(),
                    incorrect_layer.as_deref(),
                    slide,
                    &trigger.when,
                );
                let condition = trigger.condition.as_ref().map(describe_condition);
                match &trigger.action {
                    Action::JumpToSlide { target } => {
                        if let Some(to) = resolve_target(course, index, target) {
                            edges.push(MapEdge {
                                from: from.clone(),
                                to,
                                kind: MapEdgeKind::Jump,
                                via,
                                condition,
                            });
                        }
                    }
                    Action::LightboxSlide { slide: target }
                        if course.play_slide(target).is_some() =>
                    {
                        edges.push(MapEdge {
                            from: from.clone(),
                            to: target.clone(),
                            kind: MapEdgeKind::Lightbox,
                            via,
                            condition,
                        });
                    }
                    _ => {}
                }
            }
        }

        // Slide links in text runs, on the base layer and every layer.
        let mut link_objects: Vec<(&crate::Object, Vec<String>)> = Vec::new();
        for object in &slide.objects {
            link_objects.push((object, slide_links(object, course)));
        }
        for layer in &slide.layers {
            for object in &layer.objects {
                link_objects.push((object, slide_links(object, course)));
            }
        }
        for (object, targets) in link_objects {
            for to in targets {
                edges.push(MapEdge {
                    from: from.clone(),
                    to,
                    kind: MapEdgeKind::Link,
                    via: format!("link:{}", object.id),
                    condition: None,
                });
            }
        }
    }

    // The player's Next, only where no edge already reaches the next slide
    // and the slide shows navigation (its own override beats the course).
    {
        // Set lookup, not a per-slide scan of every edge: the graph stays
        // close to linear in slides plus edges even with many triggers.
        let covered: std::collections::HashSet<(String, String)> = edges
            .iter()
            .map(|edge| (edge.from.clone(), edge.to.clone()))
            .collect();
        for (index, slide) in course.slides().enumerate() {
            let Some(next) = play_ids.get(index.saturating_add(1)) else {
                continue;
            };
            let covered = covered.contains(&(slide.id.clone(), next.clone()));
            let nav = slide
                .player
                .as_ref()
                .and_then(|overrides| overrides.show_nav)
                .unwrap_or(course.player.show_nav);
            if !covered && nav {
                edges.push(MapEdge {
                    from: slide.id.clone(),
                    to: next.clone(),
                    kind: MapEdgeKind::Next,
                    via: "player_next".to_owned(),
                    condition: None,
                });
            }
        }
    }

    // Reachability: an entry slide (first, or `start_slide`) is reachable
    // even with no incoming edge.
    let incoming: std::collections::HashSet<&str> =
        edges.iter().map(|edge| edge.to.as_str()).collect();
    let mut entry: std::collections::HashSet<&str> = std::collections::HashSet::new();
    if let Some(first) = play_ids.first() {
        entry.insert(first.as_str());
    }
    if let Some(start) = course.start_slide.as_deref()
        && course.play_slide(start).is_some()
    {
        entry.insert(start);
    }
    let unreachable = |id: &str| !incoming.contains(id) && !entry.contains(id);

    let scenes = course
        .scenes
        .iter()
        .map(|scene| MapScene {
            id: scene.id.clone(),
            title: scene.title.clone(),
            slides: scene
                .slides
                .iter()
                .map(|slide| MapSlide {
                    id: slide.id.clone(),
                    title: slide.title.clone(),
                    unreachable: unreachable(&slide.id),
                })
                .collect(),
        })
        .collect();

    CourseMap { scenes, edges }
}

/// Resolve a `jump_to_slide` target to a play slide id, in course order.
fn resolve_target(course: &Course, index: usize, target: &SlideTarget) -> Option<String> {
    match target {
        SlideTarget::Next => course
            .slides()
            .nth(index.saturating_add(1))
            .map(|slide| slide.id.clone()),
        SlideTarget::Previous => {
            if index == 0 {
                None
            } else {
                course
                    .slides()
                    .nth(index.saturating_sub(1))
                    .map(|slide| slide.id.clone())
            }
        }
        SlideTarget::Id(id) => {
            if course.play_slide(id).is_some() {
                Some(id.clone())
            } else {
                None
            }
        }
    }
}

/// The `via` label for a trigger edge: a question branch layer wins, then
/// a button click, then the owning layer, then the slide itself.
fn edge_via(
    layer_id: &str,
    correct_layer: Option<&str>,
    incorrect_layer: Option<&str>,
    slide: &crate::Slide,
    when: &Event,
) -> String {
    if !layer_id.is_empty() {
        if Some(layer_id) == correct_layer {
            return "question_pass".to_owned();
        }
        if Some(layer_id) == incorrect_layer {
            return "question_fail".to_owned();
        }
    }
    if let Event::Click { object } = when
        && slide
            .object(object)
            .is_some_and(|(found, _)| matches!(found.kind, ObjectKind::Button { .. }))
    {
        return format!("button:{object}");
    }
    if layer_id.is_empty() {
        "slide".to_owned()
    } else {
        format!("layer:{layer_id}")
    }
}

/// Every slide id this object's text links to, in run order.
fn slide_links(object: &crate::Object, course: &Course) -> Vec<String> {
    let mut targets = Vec::new();
    let mut from_runs = |runs: &[crate::TextRun]| {
        for run in runs {
            if let Some(Link::Slide { slide }) = &run.link
                && course.play_slide(slide).is_some()
            {
                targets.push(slide.clone());
            }
        }
    };
    match &object.kind {
        ObjectKind::Text { runs, .. } => from_runs(runs),
        ObjectKind::Shape {
            text: Some(shape_text),
            ..
        } => from_runs(&shape_text.runs),
        ObjectKind::Table { rows, .. } => {
            for row in rows {
                for cell in &row.cells {
                    from_runs(&cell.runs);
                }
            }
        }
        _ => {}
    }
    targets
}

/// A trigger condition in words, for tooltips and `course.map` output.
#[must_use]
pub fn describe_condition(condition: &Condition) -> String {
    match condition {
        Condition::All(parts) => join_conditions(parts, "AND"),
        Condition::Any(parts) => join_conditions(parts, "OR"),
        Condition::Not(inner) => format!("NOT ({})", describe_condition(inner)),
        Condition::Compare {
            variable,
            op,
            value,
        } => format!(
            "{variable} {} {}",
            match op {
                CmpOp::Eq => "=",
                CmpOp::Ne => "!=",
                CmpOp::Lt => "<",
                CmpOp::Le => "<=",
                CmpOp::Gt => ">",
                CmpOp::Ge => ">=",
                CmpOp::Contains => "contains",
            },
            describe_value(value)
        ),
        Condition::ObjectState { object, state, is } => {
            if *is {
                format!("{object} is {state}")
            } else {
                format!("{object} is not {state}")
            }
        }
    }
}

fn join_conditions(parts: &[Condition], glue: &str) -> String {
    let described: Vec<String> = parts.iter().map(describe_condition).collect();
    if described.len() <= 1 {
        return described.into_iter().next().unwrap_or_default();
    }
    format!("({})", described.join(&format!(" {glue} ")))
}

fn describe_value(value: &Value) -> String {
    match value {
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => {
            if n.is_finite() {
                n.to_string()
            } else {
                "0".to_owned()
            }
        }
        Value::Text(text) => format!("\"{text}\""),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Action, Event, SlideTarget, Trigger};

    fn course(json: &str) -> Course {
        Course::from_json(json).expect("valid map course")
    }

    fn edges(map: &CourseMap) -> Vec<(String, String, MapEdgeKind, String)> {
        map.edges
            .iter()
            .map(|e| (e.from.clone(), e.to.clone(), e.kind, e.via.clone()))
            .collect()
    }

    #[test]
    fn jump_lightbox_link_and_layer_edges() {
        let c = course(
            r##"{
            "schema_version": 1, "id": "map1", "title": "Map",
            "slide_size": { "w": 1280, "h": 720 },
            "scenes": [{ "id": "sc1", "title": "Scene 1", "slides": [
              { "id": "s1", "title": "One", "duration_ms": 5000,
                "objects": [
                  { "id": "btn", "kind": { "type": "button", "label": "Go", "fill": "#2F6FEB" },
                    "frame": { "x": 0, "y": 0, "w": 100, "h": 40 } },
                  { "id": "txt", "kind": { "type": "text", "text": "Next slide",
                      "style": { "font": "Inter", "size": 24, "color": "#1F1F1F", "bold": false },
                      "runs": [{ "text": "Next slide", "link": { "type": "slide", "slide": "s3" } }] },
                    "frame": { "x": 0, "y": 60, "w": 300, "h": 40 } }
                ],
                "layers": [{ "id": "lyr", "name": "Layer", "duration_ms": 5000,
                  "triggers": [{ "when": { "event": "timeline_starts" },
                    "action": { "action": "jump_to_slide", "target": { "id": "s2" } } }] }],
                "triggers": [
                  { "when": { "event": "click", "object": "btn" },
                    "action": { "action": "jump_to_slide", "target": { "id": "s2" } } },
                  { "when": { "event": "click", "object": "btn" },
                    "action": { "action": "lightbox_slide", "slide": "s3" } }
                ] },
              { "id": "s2", "title": "Two", "duration_ms": 5000 },
              { "id": "s3", "title": "Three", "duration_ms": 5000 }
            ] }]
        }"##,
        );
        let map = course_map(&c);
        let got = edges(&map);
        assert!(
            got.contains(&(
                "s1".into(),
                "s2".into(),
                MapEdgeKind::Jump,
                "button:btn".into()
            )),
            "{got:?}"
        );
        assert!(
            got.contains(&(
                "s1".into(),
                "s3".into(),
                MapEdgeKind::Lightbox,
                "button:btn".into()
            )),
            "{got:?}"
        );
        assert!(
            got.contains(&(
                "s1".into(),
                "s3".into(),
                MapEdgeKind::Link,
                "link:txt".into()
            )),
            "{got:?}"
        );
        assert!(
            got.contains(&(
                "s1".into(),
                "s2".into(),
                MapEdgeKind::Jump,
                "layer:lyr".into()
            )),
            "{got:?}"
        );
        // No Next edge s1->s2 (a trigger already reaches s2); Next covers s2->s3.
        assert!(
            !got.contains(&(
                "s1".into(),
                "s2".into(),
                MapEdgeKind::Next,
                "player_next".into()
            )),
            "{got:?}"
        );
        assert!(
            got.contains(&(
                "s2".into(),
                "s3".into(),
                MapEdgeKind::Next,
                "player_next".into()
            )),
            "{got:?}"
        );
    }

    #[test]
    fn question_pass_and_fail_branches() {
        let c = course(
            r##"{
            "schema_version": 1, "id": "map2", "title": "Map",
            "slide_size": { "w": 1280, "h": 720 },
            "scenes": [{ "id": "sc1", "title": "Scene 1", "slides": [
              { "id": "q", "title": "Quiz", "duration_ms": 5000,
                "question": { "kind": "multiple_choice", "prompt": "Pick",
                  "choices": [{ "id": "a", "text": "A", "correct": true }],
                  "correct_layer": "pass", "incorrect_layer": "fail" },
                "layers": [
                  { "id": "pass", "name": "Pass", "duration_ms": 5000, "triggers": [
                    { "when": { "event": "timeline_starts" },
                      "action": { "action": "jump_to_slide", "target": { "id": "good" } } } ] },
                  { "id": "fail", "name": "Fail", "duration_ms": 5000, "triggers": [
                    { "when": { "event": "timeline_starts" },
                      "action": { "action": "jump_to_slide", "target": { "id": "bad" } } } ] }
                ] },
              { "id": "good", "title": "Good", "duration_ms": 5000 },
              { "id": "bad", "title": "Bad", "duration_ms": 5000 }
            ] }]
        }"##,
        );
        let map = course_map(&c);
        let got = edges(&map);
        assert!(
            got.contains(&(
                "q".into(),
                "good".into(),
                MapEdgeKind::Jump,
                "question_pass".into()
            )),
            "{got:?}"
        );
        assert!(
            got.contains(&(
                "q".into(),
                "bad".into(),
                MapEdgeKind::Jump,
                "question_fail".into()
            )),
            "{got:?}"
        );
    }

    #[test]
    fn conditional_edges_carry_the_condition() {
        let c = course(
            r##"{
            "schema_version": 1, "id": "map3", "title": "Map",
            "slide_size": { "w": 1280, "h": 720 },
            "variables": [{ "name": "score", "value": 0 }],
            "scenes": [{ "id": "sc1", "title": "Scene 1", "slides": [
              { "id": "s1", "title": "One", "duration_ms": 5000,
                "triggers": [{ "when": { "event": "timeline_starts" },
                  "condition": { "compare": { "variable": "score", "op": "ge", "value": 10 } },
                  "action": { "action": "jump_to_slide", "target": { "id": "s2" } } }] },
              { "id": "s2", "title": "Two", "duration_ms": 5000 }
            ] }]
        }"##,
        );
        let map = course_map(&c);
        let edge = map
            .edges
            .iter()
            .find(|e| e.from == "s1" && e.to == "s2" && e.kind == MapEdgeKind::Jump)
            .expect("conditional jump edge");
        assert_eq!(edge.condition.as_deref(), Some("score >= 10"));
    }

    #[test]
    fn relative_targets_and_missing_and_self() {
        let c = course(
            r##"{
            "schema_version": 1, "id": "map4", "title": "Map",
            "slide_size": { "w": 1280, "h": 720 },
            "scenes": [{ "id": "sc1", "title": "Scene 1", "slides": [
              { "id": "s1", "title": "One", "duration_ms": 5000,
                "triggers": [
                  { "when": { "event": "timeline_starts" },
                    "action": { "action": "jump_to_slide", "target": "previous" } },
                  { "when": { "event": "timeline_starts" },
                    "action": { "action": "jump_to_slide", "target": { "id": "gone" } } },
                  { "when": { "event": "timeline_starts" },
                    "action": { "action": "jump_to_slide", "target": { "id": "s1" } } }
                ] },
              { "id": "s2", "title": "Two", "duration_ms": 5000,
                "triggers": [
                  { "when": { "event": "timeline_starts" },
                    "action": { "action": "jump_to_slide", "target": "previous" } },
                  { "when": { "event": "timeline_starts" },
                    "action": { "action": "jump_to_slide", "target": "next" } }
                ] }
            ] }]
        }"##,
        );
        let map = course_map(&c);
        let got = edges(&map);
        // Previous on the first slide and a deleted target make no edge.
        assert!(
            got.contains(&("s1".into(), "s1".into(), MapEdgeKind::Jump, "slide".into())),
            "{got:?}"
        );
        assert!(
            got.contains(&("s2".into(), "s1".into(), MapEdgeKind::Jump, "slide".into())),
            "{got:?}"
        );
        // Next on the last slide makes no edge; s2's explicit next is a jump.
        assert!(
            got.contains(&("s2".into(), "s1".into(), MapEdgeKind::Jump, "slide".into())),
            "{got:?}"
        );
        assert!(
            !map.edges
                .iter()
                .any(|e| e.from == "s2" && e.kind == MapEdgeKind::Next),
            "{:?}",
            map.edges
        );
    }

    #[test]
    fn unreachable_detection() {
        let mut c = course(
            r##"{
            "schema_version": 1, "id": "map5", "title": "Map",
            "slide_size": { "w": 1280, "h": 720 },
            "player": { "show_nav": false, "show_menu": true, "show_seekbar": true },
            "scenes": [{ "id": "sc1", "title": "Scene 1", "slides": [
              { "id": "s1", "title": "One", "duration_ms": 5000 },
              { "id": "s2", "title": "Two", "duration_ms": 5000 },
              { "id": "s3", "title": "Three", "duration_ms": 5000 }
            ] }]
        }"##,
        );
        let map = course_map(&c);
        let flags: Vec<bool> = map.scenes[0].slides.iter().map(|s| s.unreachable).collect();
        assert_eq!(
            flags,
            vec![false, true, true],
            "only the first slide is reachable"
        );
        // A start_slide is an entry point even with no incoming edge.
        c.start_slide = Some("s2".to_owned());
        let map = course_map(&c);
        let flags: Vec<bool> = map.scenes[0].slides.iter().map(|s| s.unreachable).collect();
        assert_eq!(flags, vec![false, false, true]);
        // A self jump reaches its slide.
        if let Some(slide) = c.scenes[0].slides.get_mut(2) {
            slide.triggers.push(Trigger {
                when: Event::TimelineStarts { layer: None },
                condition: None,
                action: Action::JumpToSlide {
                    target: SlideTarget::Id("s3".to_owned()),
                },
                disabled: false,
            });
        }
        let map = course_map(&c);
        let flags: Vec<bool> = map.scenes[0].slides.iter().map(|s| s.unreachable).collect();
        assert_eq!(flags, vec![false, false, false]);
    }

    #[test]
    fn five_hundred_slides_stay_linear() {
        let mut slides = String::new();
        for i in 0..500 {
            if i > 0 {
                slides.push(',');
            }
            slides.push_str(&format!(
                r##"{{ "id": "s{i}", "title": "Slide {i}", "duration_ms": 5000 }}"##
            ));
        }
        let json = format!(
            r##"{{ "schema_version": 1, "id": "big", "title": "Big",
            "slide_size": {{ "w": 1280, "h": 720 }},
            "scenes": [{{ "id": "sc1", "title": "Scene 1", "slides": [{slides}] }}] }}"##
        );
        let c = course(&json);
        let map = course_map(&c);
        assert_eq!(map.scenes[0].slides.len(), 500);
        assert_eq!(map.edges.len(), 499, "one Next edge per slide but the last");
        assert!(map.scenes[0].slides.iter().all(|s| !s.unreachable));
    }

    #[test]
    fn five_hundred_slides_with_many_triggers_stay_linear() {
        // 500 slides, 5 jump triggers each (2500 edges) plus the player's
        // Next. The Next step is a set lookup, so this stays quick and the
        // edge counts are exact.
        const PER_SLIDE: usize = 5;
        let mut slides = String::new();
        for i in 0..500 {
            if i > 0 {
                slides.push(',');
            }
            let mut triggers = String::new();
            for t in 0..PER_SLIDE {
                if t > 0 {
                    triggers.push(',');
                }
                triggers.push_str(
                    r#"{ "when": { "event": "timeline_starts" },
                        "action": { "action": "jump_to_slide", "target": { "id": "s0" } } }"#,
                );
            }
            slides.push_str(&format!(
                r##"{{ "id": "s{i}", "title": "Slide {i}", "duration_ms": 5000,
                    "triggers": [{triggers}] }}"##
            ));
        }
        let json = format!(
            r##"{{ "schema_version": 1, "id": "big", "title": "Big",
            "slide_size": {{ "w": 1280, "h": 720 }},
            "scenes": [{{ "id": "sc1", "title": "Scene 1", "slides": [{slides}] }}] }}"##
        );
        let c = course(&json);
        let map = course_map(&c);
        assert_eq!(map.scenes[0].slides.len(), 500);
        // 500 * 5 jump edges plus 499 Next edges (no jump goes to the next
        // slide, so Next is never covered).
        assert_eq!(map.edges.len(), 500 * PER_SLIDE + 499);
        assert_eq!(
            map.edges
                .iter()
                .filter(|e| e.kind == MapEdgeKind::Next)
                .count(),
            499,
            "one Next edge per slide but the last"
        );
        assert!(map.scenes[0].slides.iter().all(|s| !s.unreachable));
    }

    #[test]
    fn describe_condition_words() {
        let all = Condition::All(vec![
            Condition::Compare {
                variable: "score".to_owned(),
                op: CmpOp::Ge,
                value: Value::Number(10.0),
            },
            Condition::ObjectState {
                object: "btn".to_owned(),
                state: "selected".to_owned(),
                is: false,
            },
        ]);
        assert_eq!(
            describe_condition(&all),
            "(score >= 10 AND btn is not selected)"
        );
        let not = Condition::Not(Box::new(Condition::Compare {
            variable: "done".to_owned(),
            op: CmpOp::Eq,
            value: Value::Bool(true),
        }));
        assert_eq!(describe_condition(&not), "NOT (done = true)");
    }

    #[test]
    fn map_serialises_to_the_cli_shape() {
        let c = course(
            r##"{
            "schema_version": 1, "id": "map6", "title": "Map",
            "slide_size": { "w": 1280, "h": 720 },
            "scenes": [{ "id": "sc1", "title": "Scene 1", "slides": [
              { "id": "s1", "title": "One", "duration_ms": 5000 }
            ] }]
        }"##,
        );
        let map = course_map(&c);
        let value = serde_json::to_value(&map).expect("map serialises");
        assert_eq!(value["scenes"][0]["slides"][0]["id"], "s1");
        assert_eq!(value["scenes"][0]["slides"][0]["unreachable"], false);
        assert!(value["edges"].is_array());
    }
}
