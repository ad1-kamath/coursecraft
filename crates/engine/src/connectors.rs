//! Connector re-routing: one rule after every successful edit.
//!
//! A connector stores its routed points, so this runs only when a glued target
//! has moved (or a connector still only has its two original ends). It never
//! panics on hostile ids or geometry: unknown targets, targets in another owner
//! and targets that are themselves connectors simply lose their glue.

use std::collections::HashMap;

use coursecraft_course::{
    Course, Glue, Object, ObjectKind, Point, Rect, connector_site, path_frame_and_points,
    route_connector,
};

/// How close a routed end must already be to its site before it is left alone.
const SITE_EPSILON: f32 = 1.0;

struct Target {
    frame: Rect,
    connector: bool,
}

/// Re-route every glued connector whose target moved, in every slide and layer.
///
/// The course is untouched when nothing moved, so a command that changed
/// nothing keeps its revision and undo state.
pub fn reroute_connectors(course: &mut Course) {
    for slide in course
        .scenes
        .iter_mut()
        .flat_map(|scene| scene.slides.iter_mut())
        .chain(course.masters.iter_mut())
        .chain(course.templates.iter_mut())
        .chain(
            course
                .banks
                .iter_mut()
                .flat_map(|bank| bank.slides.iter_mut()),
        )
    {
        reroute_owner(&mut slide.objects);
        for layer in &mut slide.layers {
            reroute_owner(&mut layer.objects);
        }
    }
}

fn is_connector(object: &Object) -> bool {
    matches!(&object.kind, ObjectKind::Path { path } if path.connector.is_some())
}

fn is_glued_connector(object: &Object) -> bool {
    matches!(
        &object.kind,
        ObjectKind::Path { path }
            if path.connector.is_some()
                && (path.start_glue.is_some() || path.end_glue.is_some())
    )
}

fn finite_frame(frame: Rect) -> bool {
    [frame.x, frame.y, frame.w, frame.h]
        .iter()
        .all(|value| value.is_finite())
}

fn resolve(glue: &Glue, targets: &HashMap<String, Target>) -> Option<(Point, u8)> {
    let target = targets.get(&glue.object)?;
    if target.connector || !finite_frame(target.frame) {
        return None;
    }
    let site = glue.site();
    let point = connector_site(
        (
            target.frame.x,
            target.frame.y,
            target.frame.w,
            target.frame.h,
        ),
        site,
    );
    (point.0.is_finite() && point.1.is_finite()).then_some((
        Point {
            x: point.0,
            y: point.1,
        },
        site,
    ))
}

fn near(a: Point, b: Point) -> bool {
    (a.x - b.x).abs() <= SITE_EPSILON && (a.y - b.y).abs() <= SITE_EPSILON
}

fn reroute_owner(objects: &mut [Object]) {
    if !objects.iter().any(is_glued_connector) {
        return;
    }
    let targets: HashMap<String, Target> = objects
        .iter()
        .map(|object| {
            (
                object.id.clone(),
                Target {
                    frame: object.frame,
                    connector: is_connector(object),
                },
            )
        })
        .collect();

    for object in objects.iter_mut() {
        if !is_glued_connector(object) {
            continue;
        }
        let frame = object.frame;
        let ObjectKind::Path { path } = &mut object.kind else {
            continue;
        };
        let Some(connector) = path.connector else {
            continue;
        };
        let current = path.slide_points(&frame);
        let (Some(&current_start), Some(&current_end)) = (current.first(), current.last()) else {
            continue;
        };

        let start_glue = path.start_glue.clone();
        let end_glue = path.end_glue.clone();
        let start = start_glue.as_ref().and_then(|glue| resolve(glue, &targets));
        let end = end_glue.as_ref().and_then(|glue| resolve(glue, &targets));

        // A target that is gone, in another owner, or itself a connector loses
        // its glue and leaves the end where it is.
        if start.is_none() {
            path.start_glue = None;
        }
        if end.is_none() {
            path.end_glue = None;
        }
        if path.start_glue.is_none() && path.end_glue.is_none() {
            continue;
        }

        let desired_start = start.map_or(current_start, |(point, _)| point);
        let desired_end = end.map_or(current_end, |(point, _)| point);
        let needs_route = path.points.len() == 2
            || !near(current_start, desired_start)
            || !near(current_end, desired_end);
        if !needs_route {
            continue;
        }

        let routed = route_connector(
            (desired_start.x, desired_start.y),
            start.map(|(_, site)| site),
            (desired_end.x, desired_end.y),
            end.map(|(_, site)| site),
            connector,
        );
        let routed: Vec<Point> = routed.into_iter().map(|(x, y)| Point { x, y }).collect();
        let Some((new_frame, fractions)) = path_frame_and_points(&routed) else {
            continue;
        };
        path.points = fractions;
        object.frame = new_frame;
    }
}
