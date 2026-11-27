//! OOXML `<p:timing>` parser and timeline scheduler for PowerPoint animations.
//!
//! Translates PowerPoint entrance, exit, emphasis, motion-path (`p:animMotion`),
//! and interactive click-triggered (`interactiveSeq`) timing trees into
//! CourseCraft [`Animation`]s, [`MotionPath`]s, [`Trigger`]s,
//! [`ObjectTiming`] bounds, and `SkipKind::Animation` diagnostics.

use std::collections::{HashMap, HashSet};

use coursecraft_course::{
    Action, AnimEffect, Animation, Direction, Easing, Event, InOut, MotionPath, Object,
    Orientation, Point, RevealShape, Trigger,
};

use crate::{SlideLayout, SlideTracker, XmlNode};

/// Maximum number of timing nodes visited per slide to bound hostile inputs.
const MAX_ANIM_NODES: usize = 1000;

/// Maximum `<p:par>` nesting depth when walking `<p:timing>`.
const MAX_TIMING_DEPTH: usize = 64;

/// Stagger gap (ms) inserted between on-click animation steps.
const CLICK_STEP_GAP_MS: u32 = 500;

/// Default animation duration (ms) when omitted or unparseable.
const DEFAULT_DURATION_MS: u32 = 500;

/// Maximum valid animation duration (ms) in `coursecraft-course`.
const MAX_DURATION_MS: u32 = 60_000;

/// Maximum valid delay (ms) clamped for hostile inputs.
const MAX_DELAY_MS: u32 = 86_400_000;

/// Maximum number of points allowed on a CourseCraft [`MotionPath`].
const MAX_MOTION_PATH_POINTS: usize = 100;

/// Number of linear segments used to approximate each cubic Bézier (`C`) curve.
const BEZIER_SUBDIVISIONS: usize = 8;

/// Maximum coordinate magnitude allowed by `coursecraft-course` validation.
const MAX_MOTION_COORD: f64 = 100_000.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StepKind {
    OnClick,
    WithPrevious,
    AfterPrevious,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PresetClass {
    Entrance,
    Exit,
    Emphasis,
    Path,
}

#[derive(Debug, Clone)]
struct ParsedEffect<'a> {
    preset_class: PresetClass,
    preset_id: u32,
    preset_subtype: u32,
    step_kind: StepKind,
    delay_ms: u32,
    duration_ms: u32,
    easing: Easing,
    spid: Option<String>,
    paragraph_level: bool,
    ctn_node: &'a XmlNode,
}

#[derive(Debug, Default)]
struct TimingWalker {
    visited: usize,
    node_limit_reported: bool,
    depth_limit_reported: bool,
}

impl TimingWalker {
    fn visit_node(&mut self, tracker: &mut SlideTracker) -> bool {
        if self.visited >= MAX_ANIM_NODES {
            if !self.node_limit_reported {
                self.node_limit_reported = true;
                tracker.report_animation(format!(
                    "Animation timing node count exceeded {MAX_ANIM_NODES}; remaining animations skipped"
                ));
            }
            return false;
        }
        self.visited = self.visited.saturating_add(1);
        true
    }

    fn check_depth(&mut self, depth: usize, tracker: &mut SlideTracker) -> bool {
        if depth > MAX_TIMING_DEPTH {
            if !self.depth_limit_reported {
                self.depth_limit_reported = true;
                tracker.report_animation(format!(
                    "Animation timing nesting exceeded depth {MAX_TIMING_DEPTH}; branch skipped"
                ));
            }
            return false;
        }
        true
    }
}

/// Parse `<p:timing>` on a slide and apply entrance/exit/motion-path timings,
/// interactive click triggers, and emphasis/repeat diagnostics to `objects` and
/// `triggers`. Returns the maximum effect end timestamp (ms) observed across
/// the main sequence so the caller can extend slide duration.
pub(crate) fn apply_slide_timing(
    timing_node: &XmlNode,
    layout: &SlideLayout,
    objects: &mut [Object],
    triggers: &mut Vec<Trigger>,
    tracker: &mut SlideTracker,
) -> u32 {
    let Some(tn_lst) = timing_node.child("tnLst") else {
        return 0;
    };

    let mut walker = TimingWalker::default();
    let mut main_effects: Vec<ParsedEffect<'_>> = Vec::new();
    let mut interactive_seqs: Vec<(String, Vec<ParsedEffect<'_>>)> = Vec::new();

    for root_par in tn_lst.children_named("par") {
        if !walker.visit_node(tracker) {
            break;
        }
        let Some(root_ctn) = root_par.child("cTn") else {
            continue;
        };
        let Some(root_children) = root_ctn.child("childTnLst") else {
            continue;
        };
        for seq in root_children.children_named("seq") {
            if !walker.visit_node(tracker) {
                break;
            }
            let Some(seq_ctn) = seq.child("cTn") else {
                continue;
            };
            let node_type = seq_ctn.attr("nodeType").unwrap_or("");
            if node_type == "mainSeq"
                && let Some(seq_children) = seq_ctn.child("childTnLst")
            {
                for step_par in seq_children.children_named("par") {
                    collect_effects_from_par(
                        step_par,
                        0,
                        None,
                        &mut walker,
                        tracker,
                        &mut main_effects,
                    );
                }
            } else if node_type == "interactiveSeq"
                && let Some(seq_children) = seq_ctn.child("childTnLst")
            {
                let Some(trigger_spid) = find_interactive_trigger_spid(seq_ctn) else {
                    tracker.report_animation(
                        "Interactive animation sequence missing onClick trigger spid; skipped",
                    );
                    continue;
                };
                let mut seq_effects = Vec::new();
                for step_par in seq_children.children_named("par") {
                    collect_effects_from_par(
                        step_par,
                        0,
                        None,
                        &mut walker,
                        tracker,
                        &mut seq_effects,
                    );
                }
                interactive_seqs.push((trigger_spid, seq_effects));
            }
        }
    }

    let max_end = schedule_main_sequence(&main_effects, layout, objects, triggers, tracker);
    for (trigger_spid, seq_effects) in &interactive_seqs {
        schedule_interactive_sequence(
            trigger_spid,
            seq_effects,
            layout,
            objects,
            triggers,
            tracker,
        );
    }
    max_end
}

fn collect_effects_from_par<'a>(
    par: &'a XmlNode,
    depth: usize,
    inherited_step: Option<StepKind>,
    walker: &mut TimingWalker,
    tracker: &mut SlideTracker,
    out: &mut Vec<ParsedEffect<'a>>,
) {
    if !walker.check_depth(depth, tracker) || !walker.visit_node(tracker) {
        return;
    }

    let Some(ctn) = par.child("cTn") else {
        return;
    };

    // Only the effect's own delay counts. PowerPoint writes the delay of the enclosing group
    // `par` as the end time of what came before it (or the group's start in the click step),
    // and the schedule below already follows the earlier effects, so adding it would count
    // that time twice.
    let own_delay = parse_ctn_delay_ms(ctn);
    let own_step = parse_step_kind(ctn.attr("nodeType")).or(inherited_step);

    if let Some(preset_class) = ctn.attr("presetClass").and_then(parse_preset_class) {
        let preset_id = ctn
            .attr("presetID")
            .and_then(|s| s.trim().parse::<u32>().ok())
            .unwrap_or(0);
        let preset_subtype = ctn
            .attr("presetSubtype")
            .and_then(|s| s.trim().parse::<u32>().ok())
            .unwrap_or(0);
        let step_kind = own_step.unwrap_or(StepKind::WithPrevious);
        let duration_ms = if preset_id == 1
            && matches!(preset_class, PresetClass::Entrance | PresetClass::Exit)
        {
            0
        } else {
            parse_effect_duration_ms(ctn)
        };
        let easing = parse_effect_easing(ctn);
        let (spid, paragraph_level) = find_effect_target(ctn);

        out.push(ParsedEffect {
            preset_class,
            preset_id,
            preset_subtype,
            step_kind,
            delay_ms: own_delay,
            duration_ms,
            easing,
            spid,
            paragraph_level,
            ctn_node: ctn,
        });
        return;
    }

    if let Some(children) = ctn.child("childTnLst") {
        for child_par in children.children_named("par") {
            collect_effects_from_par(
                child_par,
                depth.saturating_add(1),
                own_step,
                walker,
                tracker,
                out,
            );
        }
    }
}

fn schedule_main_sequence(
    effects: &[ParsedEffect<'_>],
    layout: &SlideLayout,
    objects: &mut [Object],
    triggers: &mut Vec<Trigger>,
    tracker: &mut SlideTracker,
) -> u32 {
    let mut id_to_index: HashMap<String, usize> = HashMap::with_capacity(objects.len());
    for (idx, obj) in objects.iter().enumerate() {
        id_to_index.insert(obj.id.clone(), idx);
    }

    let mut had_entrance: HashSet<usize> = HashSet::new();
    let mut had_exit: HashSet<usize> = HashSet::new();

    let mut last_group_start_ms: u32 = 0;
    let mut last_end_ms: u32 = 0;
    let mut max_effect_end_ms: u32 = 0;
    let mut emitted_click_note = false;

    for eff in effects {
        let start_ms = match eff.step_kind {
            StepKind::WithPrevious => last_group_start_ms.saturating_add(eff.delay_ms),
            StepKind::AfterPrevious => {
                let s = last_end_ms.saturating_add(eff.delay_ms);
                last_group_start_ms = s;
                s
            }
            StepKind::OnClick => {
                if !emitted_click_note {
                    emitted_click_note = true;
                    tracker.report_animation(
                        "On-click animation steps converted to staggered timeline timings (500 ms step)",
                    );
                }
                let s = last_end_ms
                    .saturating_add(CLICK_STEP_GAP_MS)
                    .saturating_add(eff.delay_ms);
                last_group_start_ms = s;
                s
            }
        };

        let end_ms = start_ms.saturating_add(eff.duration_ms);
        last_end_ms = match eff.step_kind {
            StepKind::WithPrevious => last_end_ms.max(end_ms),
            StepKind::AfterPrevious | StepKind::OnClick => end_ms,
        };
        max_effect_end_ms = max_effect_end_ms.max(end_ms);

        let Some(spid) = eff.spid.as_deref() else {
            tracker.report_animation("Animation missing target spid; skipped");
            continue;
        };
        let Some(target_obj_ids) = tracker.pptx_ids.get(spid).cloned() else {
            tracker.report_animation(format!("Animation target spid {spid} not found on slide"));
            continue;
        };

        for obj_id in target_obj_ids {
            let Some(&obj_idx) = id_to_index.get(&obj_id) else {
                continue;
            };
            let obj_name = objects[obj_idx].name.clone();

            if eff.paragraph_level {
                tracker.report_animation(format!(
                    "Paragraph-level animation applied to whole text box on {obj_name}"
                ));
            }
            check_repeat_count(eff.ctn_node, &obj_name, tracker);

            match eff.preset_class {
                PresetClass::Emphasis => {
                    let preset_name = emphasis_preset_name(eff.preset_id);
                    tracker.report_animation(format!(
                        "Emphasis animation ({preset_name}, presetID={}) skipped on {obj_name}",
                        eff.preset_id
                    ));
                }
                PresetClass::Entrance => {
                    if !had_entrance.insert(obj_idx) {
                        tracker.report_animation(format!(
                            "Duplicate entrance animation skipped on {obj_name}"
                        ));
                        continue;
                    }
                    let mapped = map_entrance_exit_preset(
                        eff.preset_id,
                        eff.preset_subtype,
                        &obj_name,
                        tracker,
                    );
                    let obj = &mut objects[obj_idx];
                    obj.timing.start_ms = start_ms;
                    obj.entrance = mapped.map(|effect| Animation {
                        effect,
                        duration_ms: eff.duration_ms,
                        easing: eff.easing,
                        sequence: Default::default(),
                    });
                }
                PresetClass::Exit => {
                    if !had_exit.insert(obj_idx) {
                        tracker.report_animation(format!(
                            "Earlier exit animation replaced on {obj_name}"
                        ));
                    }
                    let mapped = map_entrance_exit_preset(
                        eff.preset_id,
                        eff.preset_subtype,
                        &obj_name,
                        tracker,
                    );
                    let obj = &mut objects[obj_idx];
                    let safe_end = if end_ms <= obj.timing.start_ms {
                        obj.timing.start_ms.saturating_add(eff.duration_ms.max(1))
                    } else {
                        end_ms
                    };
                    max_effect_end_ms = max_effect_end_ms.max(safe_end);
                    obj.timing.end_ms = Some(safe_end);
                    obj.exit = mapped.map(|effect| Animation {
                        effect,
                        duration_ms: eff.duration_ms,
                        easing: eff.easing,
                        sequence: Default::default(),
                    });
                }
                PresetClass::Path => {
                    if let Some((points, relative)) =
                        parse_anim_motion(eff.ctn_node, layout, &obj_name, tracker)
                    {
                        let obj = &mut objects[obj_idx];
                        let path_id = format!("pptx_path_{}", obj.motion_paths.len() + 1);
                        obj.motion_paths.push(MotionPath {
                            id: path_id.clone(),
                            points,
                            duration_ms: eff.duration_ms,
                            easing: eff.easing,
                            relative,
                        });
                        let when = if start_ms == 0 {
                            Event::TimelineStarts { layer: None }
                        } else {
                            Event::TimelineReaches {
                                ms: start_ms,
                                layer: None,
                                cue: None,
                            }
                        };
                        triggers.push(Trigger {
                            action: Action::MoveOnPath {
                                object: obj_id,
                                path: path_id,
                            },
                            when,
                            condition: None,
                            disabled: false,
                        });
                    }
                }
            }
        }
    }

    max_effect_end_ms
}

fn schedule_interactive_sequence(
    trigger_spid: &str,
    effects: &[ParsedEffect<'_>],
    layout: &SlideLayout,
    objects: &mut [Object],
    triggers: &mut Vec<Trigger>,
    tracker: &mut SlideTracker,
) {
    let Some(trigger_obj_ids) = tracker.pptx_ids.get(trigger_spid).cloned() else {
        tracker.report_animation(format!(
            "Interactive animation trigger spid {trigger_spid} not found on slide"
        ));
        return;
    };

    let mut id_to_index: HashMap<String, usize> = HashMap::with_capacity(objects.len());
    for (idx, obj) in objects.iter().enumerate() {
        id_to_index.insert(obj.id.clone(), idx);
    }

    for eff in effects {
        let Some(spid) = eff.spid.as_deref() else {
            tracker.report_animation("Interactive animation missing target spid; skipped");
            continue;
        };
        let Some(target_obj_ids) = tracker.pptx_ids.get(spid).cloned() else {
            tracker.report_animation(format!(
                "Interactive animation target spid {spid} not found on slide"
            ));
            continue;
        };

        for target_id in target_obj_ids {
            let Some(&obj_idx) = id_to_index.get(&target_id) else {
                continue;
            };
            let obj_name = objects[obj_idx].name.clone();
            check_repeat_count(eff.ctn_node, &obj_name, tracker);

            match eff.preset_class {
                PresetClass::Path => {
                    if let Some((points, relative)) =
                        parse_anim_motion(eff.ctn_node, layout, &obj_name, tracker)
                    {
                        let obj = &mut objects[obj_idx];
                        let path_id = format!("pptx_path_{}", obj.motion_paths.len() + 1);
                        obj.motion_paths.push(MotionPath {
                            id: path_id.clone(),
                            points,
                            duration_ms: eff.duration_ms,
                            easing: eff.easing,
                            relative,
                        });
                        for trigger_id in &trigger_obj_ids {
                            triggers.push(Trigger {
                                action: Action::MoveOnPath {
                                    object: target_id.clone(),
                                    path: path_id.clone(),
                                },
                                when: Event::Click {
                                    object: trigger_id.clone(),
                                },
                                condition: None,
                                disabled: false,
                            });
                        }
                    }
                }
                PresetClass::Entrance => {
                    objects[obj_idx].initial_state = "hidden".to_owned();
                    for trigger_id in &trigger_obj_ids {
                        triggers.push(Trigger {
                            action: Action::ChangeState {
                                object: target_id.clone(),
                                state: "normal".to_owned(),
                            },
                            when: Event::Click {
                                object: trigger_id.clone(),
                            },
                            condition: None,
                            disabled: false,
                        });
                    }
                    tracker.report_animation(format!(
                        "Interactive click-triggered entrance on {obj_name} mapped to ChangeState"
                    ));
                }
                PresetClass::Exit => {
                    for trigger_id in &trigger_obj_ids {
                        triggers.push(Trigger {
                            action: Action::ChangeState {
                                object: target_id.clone(),
                                state: "hidden".to_owned(),
                            },
                            when: Event::Click {
                                object: trigger_id.clone(),
                            },
                            condition: None,
                            disabled: false,
                        });
                    }
                    tracker.report_animation(format!(
                        "Interactive click-triggered exit on {obj_name} mapped to ChangeState"
                    ));
                }
                PresetClass::Emphasis => {
                    let preset_name = emphasis_preset_name(eff.preset_id);
                    tracker.report_animation(format!(
                        "Emphasis animation ({preset_name}, presetID={}) skipped on {obj_name}",
                        eff.preset_id
                    ));
                }
            }
        }
    }
}

fn find_interactive_trigger_spid(seq_ctn: &XmlNode) -> Option<String> {
    let st_cond = seq_ctn.child("stCondLst")?;
    for cond in st_cond.children_named("cond") {
        let is_click = cond.attr("evt").is_none_or(|e| e == "onClick");
        if is_click
            && let Some(tgt) = cond.child("tgtEl")
            && let Some(sp_tgt) = tgt.child("spTgt")
            && let Some(spid) = sp_tgt.attr("spid")
        {
            return Some(spid.to_owned());
        }
    }
    None
}

fn check_repeat_count(ctn: &XmlNode, obj_name: &str, tracker: &mut SlideTracker) {
    let mut raw_repeat = ctn.attr("repeatCount");
    if raw_repeat.is_none()
        && let Some(children) = ctn.child("childTnLst")
    {
        for behavior in &children.children {
            if let Some(cbhvr) = behavior.child("cBhvr")
                && let Some(b_ctn) = cbhvr.child("cTn")
                && let Some(rc) = b_ctn.attr("repeatCount")
            {
                raw_repeat = Some(rc);
                break;
            }
        }
    }
    let Some(raw) = raw_repeat.map(str::trim).filter(|s| !s.is_empty()) else {
        return;
    };
    let is_multi_repeat = if raw.eq_ignore_ascii_case("indefinite") {
        true
    } else if let Ok(val) = raw.parse::<u64>() {
        val > 1000
    } else {
        false
    };
    if is_multi_repeat {
        tracker.report_animation(format!(
            "Animation repeat (repeatCount={raw}) not supported on {obj_name}; plays once"
        ));
    }
}

fn parse_anim_motion(
    ctn: &XmlNode,
    layout: &SlideLayout,
    obj_name: &str,
    tracker: &mut SlideTracker,
) -> Option<(Vec<Point>, bool)> {
    let children = ctn.child("childTnLst")?;
    let anim_motion = children.child("animMotion")?;
    let path_str = anim_motion.attr("path").unwrap_or("").trim();
    if path_str.is_empty() {
        tracker.report_animation(format!("Empty motion path skipped on {obj_name}"));
        return None;
    }

    let relative = !anim_motion
        .attr("pathEditMode")
        .is_some_and(|m| m.eq_ignore_ascii_case("fixed"));

    let mut raw_points = match parse_motion_path_grammar(path_str) {
        Some(pts) if pts.len() >= 2 => pts,
        _ => {
            tracker.report_animation(format!(
                "Malformed or degenerate motion path skipped on {obj_name}"
            ));
            return None;
        }
    };

    if raw_points.len() > MAX_MOTION_PATH_POINTS {
        raw_points = downsample_points(&raw_points, MAX_MOTION_PATH_POINTS);
    }

    let mut points: Vec<Point> = raw_points
        .into_iter()
        .map(|(nx, ny)| {
            let sx =
                (nx * f64::from(layout.slide_size.w)).clamp(-MAX_MOTION_COORD, MAX_MOTION_COORD);
            let sy =
                (ny * f64::from(layout.slide_size.h)).clamp(-MAX_MOTION_COORD, MAX_MOTION_COORD);
            Point {
                x: sx as f32,
                y: sy as f32,
            }
        })
        .collect();

    if relative
        && let Some(&first) = points.first()
        && (first.x != 0.0 || first.y != 0.0)
    {
        for pt in &mut points {
            pt.x = (f64::from(pt.x) - f64::from(first.x)).clamp(-MAX_MOTION_COORD, MAX_MOTION_COORD)
                as f32;
            pt.y = (f64::from(pt.y) - f64::from(first.y)).clamp(-MAX_MOTION_COORD, MAX_MOTION_COORD)
                as f32;
        }
    }

    Some((points, relative))
}

fn parse_motion_path_grammar(path_str: &str) -> Option<Vec<(f64, f64)>> {
    let tokens = tokenize_motion_path(path_str);
    let mut idx = 0usize;
    let mut pts: Vec<(f64, f64)> = Vec::new();
    let mut start = (0.0, 0.0);
    let mut cur = (0.0, 0.0);

    let next_num = |tokens: &[String], i: &mut usize| -> Option<f64> {
        let tok = tokens.get(*i)?;
        let val = tok.parse::<f64>().ok()?;
        if !val.is_finite() {
            return None;
        }
        *i += 1;
        Some(val)
    };

    while let Some(cmd) = tokens.get(idx) {
        idx += 1;
        match cmd.as_str() {
            "M" | "m" => {
                let x = next_num(&tokens, &mut idx)?;
                let y = next_num(&tokens, &mut idx)?;
                let pt = if cmd == "m" && !pts.is_empty() {
                    (cur.0 + x, cur.1 + y)
                } else {
                    (x, y)
                };
                start = pt;
                cur = pt;
                pts.push(pt);
            }
            "L" | "l" => {
                let x = next_num(&tokens, &mut idx)?;
                let y = next_num(&tokens, &mut idx)?;
                let pt = if cmd == "l" {
                    (cur.0 + x, cur.1 + y)
                } else {
                    (x, y)
                };
                cur = pt;
                pts.push(pt);
            }
            "C" | "c" => {
                let x1 = next_num(&tokens, &mut idx)?;
                let y1 = next_num(&tokens, &mut idx)?;
                let x2 = next_num(&tokens, &mut idx)?;
                let y2 = next_num(&tokens, &mut idx)?;
                let x = next_num(&tokens, &mut idx)?;
                let y = next_num(&tokens, &mut idx)?;
                let (p1, p2, p3) = if cmd == "c" {
                    (
                        (cur.0 + x1, cur.1 + y1),
                        (cur.0 + x2, cur.1 + y2),
                        (cur.0 + x, cur.1 + y),
                    )
                } else {
                    ((x1, y1), (x2, y2), (x, y))
                };
                let p0 = cur;
                for step in 1..=BEZIER_SUBDIVISIONS {
                    let t = step as f64 / BEZIER_SUBDIVISIONS as f64;
                    let mt = 1.0 - t;
                    let bx = mt * mt * mt * p0.0
                        + 3.0 * mt * mt * t * p1.0
                        + 3.0 * mt * t * t * p2.0
                        + t * t * t * p3.0;
                    let by = mt * mt * mt * p0.1
                        + 3.0 * mt * mt * t * p1.1
                        + 3.0 * mt * t * t * p2.1
                        + t * t * t * p3.1;
                    pts.push((bx, by));
                }
                cur = p3;
            }
            "Z" | "z" => {
                if pts.last().copied() != Some(start) {
                    pts.push(start);
                }
                cur = start;
            }
            "E" | "e" => break,
            _ => return None,
        }
    }

    Some(pts)
}

fn tokenize_motion_path(path_str: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut chars = path_str.chars().peekable();
    while let Some(ch) = chars.next() {
        // `e` or `E` inside a number such as `1.5e-5` is an exponent, not the
        // end-of-path command (N51).
        let is_exponent = matches!(ch, 'e' | 'E')
            && current.ends_with(|c: char| c.is_ascii_digit() || c == '.')
            && chars
                .peek()
                .is_some_and(|next| next.is_ascii_digit() || matches!(next, '+' | '-'));
        if is_exponent {
            current.push(ch);
        } else if ch.is_ascii_whitespace() || ch == ',' {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
        } else if matches!(
            ch,
            'M' | 'm' | 'L' | 'l' | 'C' | 'c' | 'Z' | 'z' | 'E' | 'e'
        ) {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            tokens.push(ch.to_string());
        } else {
            current.push(ch);
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn downsample_points(pts: &[(f64, f64)], max_len: usize) -> Vec<(f64, f64)> {
    if pts.len() <= max_len || max_len < 2 {
        return pts.to_vec();
    }
    let mut out = Vec::with_capacity(max_len);
    let last_idx = pts.len() - 1;
    for i in 0..max_len {
        let src_idx = (i * last_idx) / (max_len - 1);
        out.push(pts[src_idx]);
    }
    out
}

fn parse_preset_class(raw: &str) -> Option<PresetClass> {
    match raw {
        "entr" => Some(PresetClass::Entrance),
        "exit" => Some(PresetClass::Exit),
        "emph" => Some(PresetClass::Emphasis),
        "path" => Some(PresetClass::Path),
        _ => None,
    }
}

fn parse_step_kind(raw: Option<&str>) -> Option<StepKind> {
    match raw? {
        "clickEffect" => Some(StepKind::OnClick),
        "withEffect" => Some(StepKind::WithPrevious),
        "afterEffect" => Some(StepKind::AfterPrevious),
        _ => None,
    }
}

fn parse_ctn_delay_ms(ctn: &XmlNode) -> u32 {
    let Some(st_cond) = ctn.child("stCondLst") else {
        return 0;
    };
    let mut sum: u32 = 0;
    for cond in st_cond.children_named("cond") {
        if let Some(delay_str) = cond.attr("delay") {
            let trimmed = delay_str.trim();
            if trimmed.eq_ignore_ascii_case("indefinite") {
                continue;
            }
            if let Ok(val) = trimmed.parse::<i64>()
                && val > 0
            {
                let clamped =
                    u32::try_from(val.min(i64::from(MAX_DELAY_MS))).unwrap_or(MAX_DELAY_MS);
                sum = sum.saturating_add(clamped).min(MAX_DELAY_MS);
            }
        }
    }
    sum
}

fn parse_ms_attr(raw: Option<&str>) -> Option<u32> {
    let trimmed = raw?.trim();
    if trimmed.eq_ignore_ascii_case("indefinite") {
        return None;
    }
    let val = trimmed.parse::<i64>().ok()?;
    if val <= 0 {
        return None;
    }
    Some(u32::try_from(val.min(i64::from(MAX_DURATION_MS))).unwrap_or(MAX_DURATION_MS))
}

fn parse_effect_duration_ms(ctn: &XmlNode) -> u32 {
    if let Some(dur) = parse_ms_attr(ctn.attr("dur"))
        && dur > 1
    {
        return dur.clamp(1, MAX_DURATION_MS);
    }

    let mut max_child_dur: Option<u32> = None;
    if let Some(children) = ctn.child("childTnLst") {
        for behavior in &children.children {
            let is_visibility_set = behavior.name == "set";
            if let Some(cbhvr) = behavior.child("cBhvr")
                && let Some(b_ctn) = cbhvr.child("cTn")
                && let Some(dur) = parse_ms_attr(b_ctn.attr("dur"))
            {
                if is_visibility_set && dur <= 1 {
                    continue;
                }
                max_child_dur = Some(max_child_dur.map_or(dur, |prev| prev.max(dur)));
            }
        }
    }

    max_child_dur
        .unwrap_or(DEFAULT_DURATION_MS)
        .clamp(1, MAX_DURATION_MS)
}

fn parse_effect_easing(ctn: &XmlNode) -> Easing {
    let mut saw_attr = false;
    let mut has_accel = false;
    let mut has_decel = false;

    if let Some(children) = ctn.child("childTnLst") {
        for behavior in &children.children {
            if let Some(cbhvr) = behavior.child("cBhvr")
                && let Some(b_ctn) = cbhvr.child("cTn")
            {
                if let Some(accel_str) = b_ctn.attr("accel")
                    && let Ok(v) = accel_str.trim().parse::<u32>()
                {
                    saw_attr = true;
                    if v > 0 {
                        has_accel = true;
                    }
                }
                if let Some(decel_str) = b_ctn.attr("decel")
                    && let Ok(v) = decel_str.trim().parse::<u32>()
                {
                    saw_attr = true;
                    if v > 0 {
                        has_decel = true;
                    }
                }
            }
        }
    }

    if !saw_attr {
        return Easing::EaseOut;
    }
    match (has_accel, has_decel) {
        (true, true) => Easing::EaseInOut,
        (true, false) => Easing::EaseIn,
        (false, true) => Easing::EaseOut,
        (false, false) => Easing::Linear,
    }
}

fn find_effect_target(ctn: &XmlNode) -> (Option<String>, bool) {
    let Some(children) = ctn.child("childTnLst") else {
        return (None, false);
    };
    for behavior in &children.children {
        if let Some(cbhvr) = behavior.child("cBhvr")
            && let Some(tgt_el) = cbhvr.child("tgtEl")
            && let Some(sp_tgt) = tgt_el.child("spTgt")
            && let Some(spid) = sp_tgt.attr("spid")
        {
            let paragraph_level = sp_tgt
                .child("txEl")
                .is_some_and(|tx| tx.child("pRg").is_some());
            return (Some(spid.to_owned()), paragraph_level);
        }
    }
    (None, false)
}

/// PowerPoint direction subtype bits: 1 top, 2 right, 4 bottom, 8 left; corners combine them.
fn map_direction(subtype: u32) -> Direction {
    match subtype {
        1 => Direction::Top,
        2 => Direction::Right,
        8 => Direction::Left,
        3 => Direction::TopRight,
        6 => Direction::BottomRight,
        12 => Direction::BottomLeft,
        9 => Direction::TopLeft,
        _ => Direction::Bottom,
    }
}

/// `effect` coming from the direction in `subtype`. A corner on an effect that only takes
/// sides becomes the nearer side, and the report says so.
fn directed(effect: &str, subtype: u32, obj_name: &str, tracker: &mut SlideTracker) -> AnimEffect {
    let from = map_direction(subtype);
    let allowed = coursecraft_course::effect_def(effect)
        .and_then(coursecraft_course::EffectDef::direction)
        .is_some_and(|(_, a)| a.contains(&from));
    if allowed {
        return AnimEffect::with_from(effect, from);
    }
    tracker.report_animation(format!(
        "Diagonal animation direction mapped to a side on {obj_name}"
    ));
    let side = match from {
        Direction::TopLeft | Direction::TopRight => Direction::Top,
        _ => Direction::Bottom,
    };
    AnimEffect::with_from(effect, side)
}

/// PowerPoint's `horz` (10) and `vert` (5) subtypes.
fn map_orientation(subtype: u32) -> Option<Orientation> {
    match subtype {
        10 => Some(Orientation::Horizontal),
        5 => Some(Orientation::Vertical),
        _ => None,
    }
}

/// A Shape reveal: subtype 16 closes in, 32 opens out.
fn shape_reveal(shape: RevealShape, subtype: u32) -> AnimEffect {
    AnimEffect {
        shape: Some(shape),
        inout: Some(if subtype == 16 { InOut::In } else { InOut::Out }),
        ..AnimEffect::new("shape")
    }
}

fn map_entrance_exit_preset(
    preset_id: u32,
    preset_subtype: u32,
    obj_name: &str,
    tracker: &mut SlideTracker,
) -> Option<AnimEffect> {
    let oriented = |id: &str| AnimEffect {
        orientation: map_orientation(preset_subtype).or(AnimEffect::new(id).orientation),
        ..AnimEffect::new(id)
    };
    match preset_id {
        1 => None,
        2 => Some(directed("fly_in", preset_subtype, obj_name, tracker)),
        3 => Some(oriented("blinds")),
        4 => Some(shape_reveal(RevealShape::Box, preset_subtype)),
        5 => Some(oriented("checkerboard")),
        6 => Some(shape_reveal(RevealShape::Circle, preset_subtype)),
        8 => Some(shape_reveal(RevealShape::Diamond, preset_subtype)),
        9 => Some(AnimEffect::new("dissolve")),
        10 => Some(AnimEffect::new("fade")),
        12 => Some(directed("peek", preset_subtype, obj_name, tracker)),
        13 => Some(shape_reveal(RevealShape::Plus, preset_subtype)),
        14 => Some(oriented("random_bars")),
        16 => {
            tracker.report_animation(format!(
                "Split direction approximated as vertical, opening out, on {obj_name}"
            ));
            Some(AnimEffect::new("split"))
        }
        18 => {
            let from = map_direction(preset_subtype);
            let from = if matches!(
                from,
                Direction::TopLeft
                    | Direction::TopRight
                    | Direction::BottomLeft
                    | Direction::BottomRight
            ) {
                from
            } else {
                Direction::BottomRight
            };
            Some(AnimEffect::with_from("strips", from))
        }
        21 => Some(AnimEffect {
            spokes: Some(match preset_subtype {
                2 | 3 | 4 | 8 => preset_subtype as u8,
                _ => 1,
            }),
            ..AnimEffect::new("wheel")
        }),
        22 => Some(directed("wipe", preset_subtype, obj_name, tracker)),
        23 | 53 => Some(AnimEffect::new("zoom")),
        26 => Some(AnimEffect::new("bounce")),
        31 => Some(AnimEffect::new("grow_turn")),
        37 => Some(AnimEffect::new("rise")),
        42 => Some(directed("float", preset_subtype, obj_name, tracker)),
        45 => Some(AnimEffect::new("swivel")),
        47 => Some(AnimEffect::with_from("float", Direction::Top)),
        49 => Some(AnimEffect::new("spin")),
        55 => Some(AnimEffect::new("expand")),
        other => {
            tracker.report_animation(format!(
                "Unsupported animation preset (id={other}) approximated as Fade on {obj_name}"
            ));
            Some(AnimEffect::new("fade"))
        }
    }
}

fn emphasis_preset_name(preset_id: u32) -> &'static str {
    match preset_id {
        1 => "Fill Color",
        2 => "Font",
        3 => "Font Color",
        4 => "Font Size",
        5 => "Font Style",
        6 => "Grow/Shrink",
        7 => "Line Color",
        8 => "Spin",
        9 => "Transparency",
        10 => "Bold Flash",
        14 => "Pulse",
        15 => "Color Pulse",
        26 => "Teeter",
        27 => "Flicker",
        32 => "Wave",
        33 => "Blink",
        _ => "Emphasis",
    }
}

#[cfg(test)]
mod tests {
    use coursecraft_course::{Color, Object, ObjectKind, Rect, ShapeKind, Timing};

    use super::*;
    use crate::{SkipKind, parse_xml_tree};

    fn dummy_object(id: &str, name: &str) -> Object {
        Object {
            id: id.to_owned(),
            name: name.to_owned(),
            kind: ObjectKind::Shape {
                shape: ShapeKind::Rectangle,
                fill: Color("#112233".to_owned()),
                fill_style: None,
                outline: None,
                text: None,
                adjust: Vec::new(),
            },
            frame: Rect {
                x: 100.0,
                y: 100.0,
                w: 200.0,
                h: 100.0,
            },
            timing: Timing::default(),
            states: Vec::new(),
            initial_state: "normal".to_owned(),
            entrance: None,
            exit: None,
            motion_paths: Vec::new(),
            emphasis: Vec::new(),
            focus_order: None,
            decorative: false,
            description: None,
            group: None,
            panel: None,
            rotation: 0.0,
            flip_h: false,
            flip_v: false,
            opacity: 1.0,
            shadow: None,
            glow: None,
            drag: None,
            locked: false,
            editor_hidden: false,
        }
    }

    #[test]
    fn maps_entrance_exit_emphasis_and_staggered_timeline() {
        let xml = r#"
        <p:timing xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
          <p:tnLst>
            <p:par><p:cTn id="1" nodeType="tmRoot"><p:childTnLst>
              <p:seq><p:cTn id="2" nodeType="mainSeq"><p:childTnLst>
                <p:par><p:cTn id="3"><p:childTnLst>
                  <p:par><p:cTn id="4" presetID="10" presetClass="entr" presetSubtype="0" nodeType="withEffect" dur="500">
                    <p:stCondLst><p:cond delay="0"/></p:stCondLst>
                    <p:childTnLst><p:set><p:cBhvr><p:cTn id="5" dur="1"/><p:tgtEl><p:spTgt spid="2"/></p:tgtEl></p:cBhvr></p:set></p:childTnLst>
                  </p:cTn></p:par>
                  <p:par><p:cTn id="6" presetID="2" presetClass="entr" presetSubtype="8" nodeType="withEffect" dur="400">
                    <p:stCondLst><p:cond delay="200"/></p:stCondLst>
                    <p:childTnLst><p:animEffect><p:cBhvr><p:cTn id="7" dur="400" accel="50000" decel="0"/><p:tgtEl><p:spTgt spid="3"/></p:tgtEl></p:cBhvr></p:animEffect></p:childTnLst>
                  </p:cTn></p:par>
                  <p:par><p:cTn id="8" presetID="8" presetClass="emph" presetSubtype="0" nodeType="afterEffect" dur="600">
                    <p:stCondLst><p:cond delay="100"/></p:stCondLst>
                    <p:childTnLst><p:animRot><p:cBhvr><p:cTn id="9" dur="600"/><p:tgtEl><p:spTgt spid="2"/></p:tgtEl></p:cBhvr></p:animRot></p:childTnLst>
                  </p:cTn></p:par>
                  <p:par><p:cTn id="10" presetID="22" presetClass="exit" presetSubtype="2" nodeType="clickEffect" dur="500">
                    <p:stCondLst><p:cond delay="0"/></p:stCondLst>
                    <p:childTnLst><p:animEffect><p:cBhvr><p:cTn id="11" dur="500" accel="50000" decel="50000"/><p:tgtEl><p:spTgt spid="2"/></p:tgtEl></p:cBhvr></p:animEffect></p:childTnLst>
                  </p:cTn></p:par>
                </p:childTnLst></p:cTn></p:par>
              </p:childTnLst></p:cTn></p:seq>
            </p:childTnLst></p:cTn></p:par>
          </p:tnLst>
        </p:timing>"#;

        let timing_tree = parse_xml_tree(xml, "slide1.xml", 64).expect("valid xml");
        let mut objects = vec![
            dummy_object("obj_a", "Box A"),
            dummy_object("obj_b", "Box B"),
        ];
        let mut tracker = SlideTracker::new(1);
        tracker
            .pptx_ids
            .insert("2".to_owned(), vec!["obj_a".to_owned()]);
        tracker
            .pptx_ids
            .insert("3".to_owned(), vec!["obj_b".to_owned()]);

        let layout = SlideLayout::from_emu(12_192_000.0, 6_858_000.0);
        let mut triggers = Vec::new();
        let max_end = apply_slide_timing(
            &timing_tree,
            &layout,
            &mut objects,
            &mut triggers,
            &mut tracker,
        );

        assert_eq!(objects[0].timing.start_ms, 0);
        assert_eq!(
            objects[0].entrance,
            Some(Animation {
                effect: AnimEffect::new("fade"),
                duration_ms: 500,
                easing: Easing::EaseOut,
                sequence: Default::default(),
            })
        );
        assert_eq!(objects[1].timing.start_ms, 200);
        assert_eq!(
            objects[1].entrance,
            Some(Animation {
                effect: AnimEffect::with_from("fly_in", Direction::Left),
                duration_ms: 400,
                easing: Easing::EaseIn,
                sequence: Default::default(),
            })
        );
        // Emphasis at 600 + 100 = 700..1300; clickEffect at 1300 + 500 = 1800..2300.
        assert_eq!(objects[0].timing.end_ms, Some(2300));
        assert_eq!(
            objects[0].exit,
            Some(Animation {
                effect: AnimEffect::with_from("wipe", Direction::Right),
                duration_ms: 500,
                easing: Easing::EaseInOut,
                sequence: Default::default(),
            })
        );
        assert_eq!(max_end, 2300);

        let anim_notes: Vec<&str> = tracker
            .items
            .iter()
            .filter(|i| i.kind == SkipKind::Animation)
            .map(|i| i.note.as_str())
            .collect();
        assert!(anim_notes.iter().any(|n| n.contains("Spin")));
        assert!(anim_notes.iter().any(|n| n.contains("On-click")));
    }

    #[test]
    fn group_spid_applies_animation_to_all_flattened_children() {
        let xml = r#"
        <p:timing xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
          <p:tnLst>
            <p:par><p:cTn id="1" nodeType="tmRoot"><p:childTnLst>
              <p:seq><p:cTn id="2" nodeType="mainSeq"><p:childTnLst>
                <p:par><p:cTn id="3" presetID="10" presetClass="entr" nodeType="withEffect" dur="500">
                  <p:childTnLst><p:set><p:cBhvr><p:tgtEl><p:spTgt spid="100"/></p:tgtEl></p:cBhvr></p:set></p:childTnLst>
                </p:cTn></p:par>
              </p:childTnLst></p:cTn></p:seq>
            </p:childTnLst></p:cTn></p:par>
          </p:tnLst>
        </p:timing>"#;

        let timing_tree = parse_xml_tree(xml, "slide1.xml", 64).expect("valid xml");
        let mut objects = vec![
            dummy_object("c1", "Child 1"),
            dummy_object("c2", "Child 2"),
            dummy_object("c3", "Child 3"),
        ];
        let mut tracker = SlideTracker::new(1);
        tracker.pptx_ids.insert(
            "100".to_owned(),
            vec!["c1".to_owned(), "c2".to_owned(), "c3".to_owned()],
        );

        let layout = SlideLayout::from_emu(12_192_000.0, 6_858_000.0);
        let mut triggers = Vec::new();
        apply_slide_timing(
            &timing_tree,
            &layout,
            &mut objects,
            &mut triggers,
            &mut tracker,
        );
        for obj in &objects {
            assert_eq!(
                obj.entrance,
                Some(Animation {
                    effect: AnimEffect::new("fade"),
                    duration_ms: 500,
                    easing: Easing::EaseOut,
                    sequence: Default::default(),
                })
            );
        }
    }

    #[test]
    fn appear_disappear_and_exit_before_start_clamping() {
        let xml = r#"
        <p:timing xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
          <p:tnLst>
            <p:par><p:cTn id="1" nodeType="tmRoot"><p:childTnLst>
              <p:seq><p:cTn id="2" nodeType="mainSeq"><p:childTnLst>
                <p:par><p:cTn id="3"><p:childTnLst>
                  <p:par><p:cTn id="4" presetID="1" presetClass="entr" nodeType="withEffect" dur="1">
                    <p:stCondLst><p:cond delay="1000"/></p:stCondLst>
                    <p:childTnLst><p:set><p:cBhvr><p:tgtEl><p:spTgt spid="2"/></p:tgtEl></p:cBhvr></p:set></p:childTnLst>
                  </p:cTn></p:par>
                  <p:par><p:cTn id="5" presetID="1" presetClass="exit" nodeType="withEffect" dur="200">
                    <p:stCondLst><p:cond delay="100"/></p:stCondLst>
                    <p:childTnLst><p:set><p:cBhvr><p:tgtEl><p:spTgt spid="2"/></p:tgtEl></p:cBhvr></p:set></p:childTnLst>
                  </p:cTn></p:par>
                  <p:par><p:cTn id="6" presetID="99" presetClass="entr" presetSubtype="6" nodeType="withEffect" dur="999999999">
                    <p:stCondLst><p:cond delay="-500"/></p:stCondLst>
                    <p:childTnLst><p:animEffect><p:cBhvr><p:tgtEl><p:spTgt spid="3"/></p:tgtEl></p:cBhvr></p:animEffect></p:childTnLst>
                  </p:cTn></p:par>
                </p:childTnLst></p:cTn></p:par>
              </p:childTnLst></p:cTn></p:seq>
            </p:childTnLst></p:cTn></p:par>
          </p:tnLst>
        </p:timing>"#;

        let timing_tree = parse_xml_tree(xml, "slide1.xml", 64).expect("valid xml");
        let mut objects = vec![dummy_object("o1", "One"), dummy_object("o2", "Two")];
        let mut tracker = SlideTracker::new(1);
        tracker
            .pptx_ids
            .insert("2".to_owned(), vec!["o1".to_owned()]);
        tracker
            .pptx_ids
            .insert("3".to_owned(), vec!["o2".to_owned()]);

        let layout = SlideLayout::from_emu(12_192_000.0, 6_858_000.0);
        let mut triggers = Vec::new();
        apply_slide_timing(
            &timing_tree,
            &layout,
            &mut objects,
            &mut triggers,
            &mut tracker,
        );

        assert_eq!(objects[0].timing.start_ms, 1000);
        assert_eq!(objects[0].entrance, None);
        assert_eq!(objects[0].exit, None);
        // Exit delay 100 + dur 500 = 600 <= start_ms 1000, so clamped > start_ms.
        assert!(objects[0].timing.end_ms.is_some_and(|e| e > 1000));

        assert_eq!(objects[1].timing.start_ms, 0);
        assert_eq!(
            objects[1].entrance,
            Some(Animation {
                effect: AnimEffect::new("fade"),
                duration_ms: 60_000,
                easing: Easing::EaseOut,
                sequence: Default::default(),
            })
        );
    }

    #[test]
    fn pptx_ids_records_group_children_and_connectors() {
        let slide_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
        <p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
               xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
          <p:cSld>
            <p:spTree>
              <p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>
              <p:grpSpPr/>
              <p:grpSp>
                <p:nvGrpSpPr><p:cNvPr id="10" name="Group 1"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>
                <p:grpSpPr/>
                <p:sp>
                  <p:nvSpPr><p:cNvPr id="11" name="S1"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr>
                  <p:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="914400" cy="914400"/></a:xfrm><a:prstGeom prst="rect"/></p:spPr>
                </p:sp>
                <p:sp>
                  <p:nvSpPr><p:cNvPr id="12" name="S2"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr>
                  <p:spPr><a:xfrm><a:off x="914400" y="0"/><a:ext cx="914400" cy="914400"/></a:xfrm><a:prstGeom prst="rect"/></p:spPr>
                </p:sp>
              </p:grpSp>
              <p:cxnSp>
                <p:nvCxnSpPr><p:cNvPr id="20" name="Conn 1"/><p:cNvCxnSpPr/><p:nvPr/></p:nvCxnSpPr>
                <p:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="914400" cy="0"/></a:xfrm><a:prstGeom prst="line"/></p:spPr>
              </p:cxnSp>
            </p:spTree>
          </p:cSld>
        </p:sld>"#;

        let tree = parse_xml_tree(slide_xml, "slide1.xml", 64).expect("valid xml");
        let sp_tree = tree.find_descendant("spTree").expect("spTree");
        let mut objects = Vec::new();
        let mut tracker = SlideTracker::new(1);
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let writer = zip::ZipWriter::new(&mut buf);
            writer.finish().expect("finish empty zip");
        }
        let zip_bytes = buf.into_inner();
        let archive = zip::ZipArchive::new(std::io::Cursor::new(zip_bytes.as_slice()))
            .expect("open empty zip");
        let limits = crate::Limits::default();
        let entries = crate::ZipEntries::new(archive, HashSet::new(), limits);
        let layout = crate::SlideLayout::from_emu(12_192_000.0, 6_858_000.0);
        let rels = HashMap::new();
        let mut slide_title = None;
        let mut imported_media = Vec::new();
        let mut existing_media_names = HashSet::new();
        let mut resolver = crate::PlaceholderResolver::new(limits.max_xml_depth);
        let slide_parts = HashMap::new();
        let theme = crate::ThemePalette::default();
        let table_styles = crate::TableStyleSet::default();
        let mut cx = crate::SlideContext {
            layout: &layout,
            slide_rels: &rels,
            entries: &entries,
            objects: &mut objects,
            slide_title: &mut slide_title,
            imported_media: &mut imported_media,
            existing_media_names: &mut existing_media_names,
            tracker: &mut tracker,
            limits: &limits,
            resolver: &mut resolver,
            layout_path: None,
            slide_parts: &slide_parts,
            theme: &theme,
            table_styles: &table_styles,
        };
        crate::process_group_children(sp_tree, crate::GroupTransform::default(), 0, &mut cx)
            .expect("group children processed");

        assert_eq!(objects.len(), 3);
        let grp_ids = tracker.pptx_ids.get("10").expect("group id mapped");
        assert_eq!(grp_ids.len(), 2);
        assert_eq!(tracker.pptx_ids.get("11").map(Vec::len), Some(1));
        assert_eq!(tracker.pptx_ids.get("12").map(Vec::len), Some(1));
        assert_eq!(tracker.pptx_ids.get("20").map(Vec::len), Some(1));
    }

    #[test]
    fn motion_path_interactive_seq_and_repeat_count() {
        let xml = r#"
        <p:timing xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
          <p:tnLst>
            <p:par><p:cTn id="1" nodeType="tmRoot"><p:childTnLst>
              <p:seq><p:cTn id="2" nodeType="mainSeq"><p:childTnLst>
                <p:par><p:cTn id="3"><p:childTnLst>
                  <p:par><p:cTn id="4" presetID="0" presetClass="path" nodeType="afterEffect" dur="1200" repeatCount="3000">
                    <p:stCondLst><p:cond delay="300"/></p:stCondLst>
                    <p:childTnLst>
                      <p:animMotion path="M 0 0 L 0.25 0 C 0.25 0.1 0.1 0.2 0 0.2 Z E" origin="layout" pathEditMode="relative">
                        <p:cBhvr><p:cTn id="5" dur="1200"/><p:tgtEl><p:spTgt spid="2"/></p:tgtEl></p:cBhvr>
                      </p:animMotion>
                    </p:childTnLst>
                  </p:cTn></p:par>
                </p:childTnLst></p:cTn></p:par>
              </p:childTnLst></p:cTn></p:seq>
              <p:seq><p:cTn id="10" nodeType="interactiveSeq">
                <p:stCondLst><p:cond evt="onClick"><p:tgtEl><p:spTgt spid="9"/></p:tgtEl></p:cond></p:stCondLst>
                <p:childTnLst>
                  <p:par><p:cTn id="11"><p:childTnLst>
                    <p:par><p:cTn id="12" presetID="0" presetClass="path" dur="800">
                      <p:childTnLst>
                        <p:animMotion path="M 0 0 L 0.5 0.5 E" origin="layout" pathEditMode="relative">
                          <p:cBhvr><p:cTn id="13" dur="800"/><p:tgtEl><p:spTgt spid="3"/></p:tgtEl></p:cBhvr>
                        </p:animMotion>
                      </p:childTnLst>
                    </p:cTn></p:par>
                    <p:par><p:cTn id="14" presetID="10" presetClass="entr" dur="500">
                      <p:childTnLst><p:set><p:cBhvr><p:tgtEl><p:spTgt spid="4"/></p:tgtEl></p:cBhvr></p:set></p:childTnLst>
                    </p:cTn></p:par>
                  </p:childTnLst></p:cTn></p:par>
                </p:childTnLst>
              </p:cTn></p:seq>
            </p:childTnLst></p:cTn></p:par>
          </p:tnLst>
        </p:timing>"#;

        let timing_tree = parse_xml_tree(xml, "slide1.xml", 64).expect("valid xml");
        let mut objects = vec![
            dummy_object("obj_a", "Mover A"),
            dummy_object("obj_b", "Mover B"),
            dummy_object("obj_c", "Reveal C"),
            dummy_object("btn", "Trigger Btn"),
        ];
        let mut tracker = SlideTracker::new(1);
        tracker
            .pptx_ids
            .insert("2".to_owned(), vec!["obj_a".to_owned()]);
        tracker
            .pptx_ids
            .insert("3".to_owned(), vec!["obj_b".to_owned()]);
        tracker
            .pptx_ids
            .insert("4".to_owned(), vec!["obj_c".to_owned()]);
        tracker
            .pptx_ids
            .insert("9".to_owned(), vec!["btn".to_owned()]);

        let layout = SlideLayout::from_emu(12_192_000.0, 6_858_000.0);
        let mut triggers = Vec::new();
        let max_end = apply_slide_timing(
            &timing_tree,
            &layout,
            &mut objects,
            &mut triggers,
            &mut tracker,
        );

        assert_eq!(max_end, 1500);
        assert_eq!(objects[0].motion_paths.len(), 1);
        let mp_a = &objects[0].motion_paths[0];
        assert!(mp_a.relative);
        assert_eq!(mp_a.points.first().copied(), Some(Point { x: 0.0, y: 0.0 }));
        assert_eq!(mp_a.points.last().copied(), Some(Point { x: 0.0, y: 0.0 }));
        assert!(mp_a.points.len() >= 10 && mp_a.points.len() <= MAX_MOTION_PATH_POINTS);

        assert_eq!(objects[1].motion_paths.len(), 1);
        assert_eq!(objects[2].initial_state, "hidden");

        assert!(triggers.iter().any(|t| {
            t.when
                == Event::TimelineReaches {
                    ms: 300,
                    layer: None,
                    cue: None,
                }
                && matches!(&t.action, Action::MoveOnPath { object, .. } if object == "obj_a")
        }));
        assert!(triggers.iter().any(|t| {
            t.when
                == Event::Click {
                    object: "btn".to_owned(),
                }
                && matches!(&t.action, Action::MoveOnPath { object, .. } if object == "obj_b")
        }));
        assert!(triggers.iter().any(|t| {
            t.when
                == Event::Click {
                    object: "btn".to_owned(),
                }
                && t.action
                    == Action::ChangeState {
                        object: "obj_c".to_owned(),
                        state: "normal".to_owned(),
                    }
        }));
        assert!(
            tracker
                .items
                .iter()
                .any(|i| i.kind == SkipKind::Animation && i.note.contains("repeatCount=3000"))
        );
    }

    /// One effect as PowerPoint writes it: the effect `par` with its own delay and the
    /// behaviour `cTn` that holds the duration.
    fn real_effect(id: u32, spid: u32, node_type: &str, own_delay: u32, dur: u32) -> String {
        format!(
            r#"<p:par><p:cTn id="{id}" presetID="10" presetClass="entr" presetSubtype="0" fill="hold" nodeType="{node_type}">
              <p:stCondLst><p:cond delay="{own_delay}"/></p:stCondLst>
              <p:childTnLst>
                <p:set><p:cBhvr><p:cTn id="{a}" dur="1" fill="hold"><p:stCondLst><p:cond delay="0"/></p:stCondLst></p:cTn><p:tgtEl><p:spTgt spid="{spid}"/></p:tgtEl></p:cBhvr></p:set>
                <p:animEffect transition="in" filter="fade"><p:cBhvr><p:cTn id="{b}" dur="{dur}"/><p:tgtEl><p:spTgt spid="{spid}"/></p:tgtEl></p:cBhvr></p:animEffect>
              </p:childTnLst></p:cTn></p:par>"#,
            a = id + 100,
            b = id + 200
        )
    }

    /// PowerPoint's middle `par`: its delay is where the group starts inside the click step
    /// (the end of what came before), not an extra delay of the effects inside it.
    fn real_group(id: u32, group_delay: u32, effects: &[String]) -> String {
        format!(
            r#"<p:par><p:cTn id="{id}" fill="hold"><p:stCondLst><p:cond delay="{group_delay}"/></p:stCondLst><p:childTnLst>{}</p:childTnLst></p:cTn></p:par>"#,
            effects.concat()
        )
    }

    /// Slide timing with one click step holding `groups`; returns each object's start.
    fn starts_for(groups: &[String], spids: &[u32]) -> Vec<u32> {
        let xml = format!(
            r#"<p:timing xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:tnLst>
            <p:par><p:cTn id="1" dur="indefinite" restart="never" nodeType="tmRoot"><p:childTnLst>
              <p:seq concurrent="1" nextAc="seek"><p:cTn id="2" dur="indefinite" nodeType="mainSeq"><p:childTnLst>
                <p:par><p:cTn id="3" fill="hold"><p:stCondLst><p:cond delay="indefinite"/></p:stCondLst><p:childTnLst>{}</p:childTnLst></p:cTn></p:par>
              </p:childTnLst></p:cTn></p:seq>
            </p:childTnLst></p:cTn></p:par></p:tnLst></p:timing>"#,
            groups.concat()
        );
        let tree = parse_xml_tree(&xml, "slide1.xml", 64).expect("valid xml");
        let mut objects: Vec<Object> = spids
            .iter()
            .map(|s| dummy_object(&format!("o{s}"), &format!("Obj {s}")))
            .collect();
        let mut tracker = SlideTracker::new(1);
        for s in spids {
            tracker
                .pptx_ids
                .insert(s.to_string(), vec![format!("o{s}")]);
        }
        let layout = SlideLayout::from_emu(12_192_000.0, 6_858_000.0);
        let mut triggers = Vec::new();
        apply_slide_timing(&tree, &layout, &mut objects, &mut triggers, &mut tracker);
        objects.iter().map(|o| o.timing.start_ms).collect()
    }

    #[test]
    fn after_previous_does_not_count_the_group_delay_twice() {
        // A on click (500 ms), B after previous. PowerPoint writes group delay 500 on B's par.
        let a = real_group(4, 0, &[real_effect(5, 2, "clickEffect", 0, 500)]);
        let b = real_group(8, 500, &[real_effect(9, 3, "afterEffect", 0, 500)]);
        let starts = starts_for(&[a, b], &[2, 3]);
        // A starts after the click gap; B starts exactly when A ends.
        assert_eq!(starts, vec![500, 1000]);
    }

    #[test]
    fn extra_delays_follow_the_effect_not_the_group() {
        // A (click, 500), B after previous with an extra 300 ms delay on the effect.
        let a = real_group(4, 0, &[real_effect(5, 2, "clickEffect", 0, 500)]);
        let b = real_group(8, 500, &[real_effect(9, 3, "afterEffect", 300, 500)]);
        assert_eq!(starts_for(&[a, b], &[2, 3]), vec![500, 1300]);
        // C follows B (500 ms, group delay 1000 = the end of B's group before it) and D is
        // with C with a 200 ms delay: D starts 200 ms after C, not 1000 ms later.
        let a = real_group(4, 0, &[real_effect(5, 2, "clickEffect", 0, 500)]);
        let c = real_group(
            8,
            500,
            &[
                real_effect(9, 3, "afterEffect", 0, 500),
                real_effect(12, 4, "withEffect", 200, 500),
            ],
        );
        assert_eq!(starts_for(&[a, c], &[2, 3, 4]), vec![500, 1000, 1200]);
    }

    #[test]
    fn with_previous_in_the_first_group_starts_with_the_click_step() {
        let a = real_group(
            4,
            0,
            &[
                real_effect(5, 2, "clickEffect", 0, 500),
                real_effect(8, 3, "withEffect", 0, 500),
            ],
        );
        assert_eq!(starts_for(&[a], &[2, 3]), vec![500, 500]);
    }
}
