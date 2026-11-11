use serde_json::json;

use coursecraft_course::{
    Color, ConnectorStyle, DashStyle, Glue, LineEnd, Object, ObjectKind, Outline, Path, PathPoint,
    Point, Rect, ShapeKind,
};

use crate::{Session, reroute_connectors};

fn add_shape(s: &mut Session, x: f32, y: f32, w: f32, h: f32) -> String {
    s.execute(
        "object.add",
        &json!({
            "slide": "slide1",
            "kind": {
                "type": "shape",
                "shape": "rectangle",
                "fill": "#D9D9D9"
            },
            "frame": { "x": x, "y": y, "w": w, "h": h }
        }),
    )
    .unwrap_or_else(|error| panic!("add shape: {error}"))["id"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

fn add_connector(s: &mut Session, target: &str, site: u8) -> String {
    let id = s
        .execute(
            "object.add",
            &json!({
                "slide": "slide1",
                "kind": {
                    "type": "path",
                    "points": [{"x": 0.0, "y": 0.0}, {"x": 1.0, "y": 1.0}],
                    "connector": "straight",
                    "outline": {"color": "#1F1F1F", "width": 2}
                },
                "frame": { "x": 0, "y": 0, "w": 100, "h": 100 }
            }),
        )
        .unwrap_or_else(|error| panic!("add connector: {error}"))["id"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    s.execute(
        "object.set_path",
        &json!({
            "slide": "slide1",
            "object": id,
            "points": [{"x": 20, "y": 20}, {"x": 60, "y": 60}],
            "start_glue": null,
            "end_glue": {"object": target, "site": site}
        }),
    )
    .unwrap_or_else(|error| panic!("glue connector: {error}"));
    id
}

fn path_of<'a>(s: &'a Session, id: &str) -> (&'a Object, &'a Path) {
    let object = s
        .course
        .slide("slide1")
        .and_then(|slide| slide.object(id))
        .map(|(object, _)| object)
        .unwrap_or_else(|| panic!("object {id}"));
    let ObjectKind::Path { path } = &object.kind else {
        panic!("{id} is not a path");
    };
    (object, path)
}

fn endpoint(s: &Session, id: &str) -> Point {
    let (object, path) = path_of(s, id);
    *path
        .slide_points(&object.frame)
        .last()
        .unwrap_or(&Point::default())
}

fn site(s: &Session, id: &str, site: u8) -> Point {
    let frame = s
        .course
        .slide("slide1")
        .and_then(|slide| slide.object(id))
        .map(|(object, _)| object.frame)
        .unwrap_or_default();
    match site % 4 {
        0 => Point {
            x: frame.x + frame.w * 0.5,
            y: frame.y,
        },
        1 => Point {
            x: frame.x + frame.w,
            y: frame.y + frame.h * 0.5,
        },
        2 => Point {
            x: frame.x + frame.w * 0.5,
            y: frame.y + frame.h,
        },
        _ => Point {
            x: frame.x,
            y: frame.y + frame.h * 0.5,
        },
    }
}

fn assert_near(a: Point, b: Point) {
    assert!(
        (a.x - b.x).abs() < 0.01 && (a.y - b.y).abs() < 0.01,
        "{a:?} != {b:?}"
    );
}

#[test]
fn moving_a_target_reroutes_in_the_same_undo_step() {
    let mut s = Session::blank("Connectors");
    let target = add_shape(&mut s, 200.0, 100.0, 100.0, 80.0);
    let connector = add_connector(&mut s, &target, 3);
    let before = s.course.clone();
    let revision = s.revision;
    let undo = s.undo_len();

    s.execute(
        "object.set_frame",
        &json!({
            "slide": "slide1",
            "object": target,
            "frame": { "x": 320, "y": 180, "w": 100, "h": 80 }
        }),
    )
    .expect("move target");

    assert_near(endpoint(&s, &connector), site(&s, &target, 3));
    assert_eq!(s.revision, revision + 1);
    assert_eq!(s.undo_len(), undo + 1);

    s.execute("edit.undo", &json!({})).expect("undo");
    assert_eq!(s.course, before);
}

#[test]
fn align_distribute_and_match_size_all_reroute() {
    let mut s = Session::blank("Connectors");
    let target = add_shape(&mut s, 200.0, 100.0, 100.0, 80.0);
    let connector = add_connector(&mut s, &target, 0);
    let before = s.course.clone();
    s.execute(
        "object.align",
        &json!({
            "slide": "slide1",
            "objects": [target],
            "edge": "left",
            "to": "slide"
        }),
    )
    .expect("align");
    assert_near(endpoint(&s, &connector), site(&s, &target, 0));
    s.execute("edit.undo", &json!({})).expect("undo align");
    assert_eq!(s.course, before);

    let first = add_shape(&mut s, 0.0, 0.0, 50.0, 50.0);
    let middle = add_shape(&mut s, 150.0, 0.0, 100.0, 50.0);
    let last = add_shape(&mut s, 400.0, 0.0, 50.0, 50.0);
    let connector = add_connector(&mut s, &middle, 1);
    s.execute(
        "object.distribute",
        &json!({
            "slide": "slide1",
            "objects": [first, middle, last],
            "axis": "horizontal"
        }),
    )
    .expect("distribute");
    assert_near(endpoint(&s, &connector), site(&s, &middle, 1));

    let connector = add_connector(&mut s, &middle, 2);
    s.execute(
        "object.match_size",
        &json!({
            "slide": "slide1",
            "objects": [last, middle],
            "dimension": "both"
        }),
    )
    .expect("match size");
    assert_near(endpoint(&s, &connector), site(&s, &middle, 2));
}

#[test]
fn a_batch_move_is_one_undo_step_for_objects_and_connectors() {
    let mut s = Session::blank("Connectors");
    let target = add_shape(&mut s, 200.0, 100.0, 100.0, 80.0);
    let connector = add_connector(&mut s, &target, 1);
    let before = s.course.clone();
    let revision = s.revision;
    let undos = s.undo_len();

    s.execute_batch(&[
        (
            "object.set_frame".to_owned(),
            json!({
                "slide": "slide1",
                "object": target,
                "frame": { "x": 360, "y": 220, "w": 100, "h": 80 }
            }),
        ),
        (
            "object.set_frame".to_owned(),
            json!({
                "slide": "slide1",
                "object": target,
                "frame": { "x": 400, "y": 260, "w": 120, "h": 90 }
            }),
        ),
    ])
    .expect("batch move");

    assert_near(endpoint(&s, &connector), site(&s, &target, 1));
    assert_eq!(s.revision, revision + 1);
    assert_eq!(s.undo_len(), undos + 1);
    s.execute("edit.undo", &json!({})).expect("undo");
    assert_eq!(s.course, before);
}

#[test]
fn a_no_op_keeps_revision_and_undo_and_rerouting_is_equal() {
    let mut s = Session::blank("Connectors");
    let target = add_shape(&mut s, 200.0, 100.0, 100.0, 80.0);
    let _connector = add_connector(&mut s, &target, 2);
    let before = s.course.clone();
    let revision = s.revision;
    let undo = s.undo_len();

    reroute_connectors(&mut s.course);
    assert_eq!(s.course, before);
    assert_eq!(s.revision, revision);
    assert_eq!(s.undo_len(), undo);

    s.execute(
        "course.replace",
        &json!({ "query": "no such text anywhere", "replacement": "x" }),
    )
    .expect("no-op replace");
    assert_eq!(s.course, before);
    assert_eq!(s.revision, revision);
    assert_eq!(s.undo_len(), undo);
}

#[test]
fn set_path_glue_params_are_optional_and_null_clears() {
    let mut s = Session::blank("Connectors");
    let target = add_shape(&mut s, 200.0, 100.0, 100.0, 80.0);
    let connector = add_connector(&mut s, &target, 0);

    // Omitting the params leaves the existing glue alone.
    s.execute(
        "object.set_path",
        &json!({
            "slide": "slide1",
            "object": connector,
            "points": [{"x": 30, "y": 30}, {"x": 240, "y": 140}]
        }),
    )
    .expect("move without changing glue");
    let (_, path) = path_of(&s, &connector);
    assert_eq!(
        path.end_glue.as_ref().map(|glue| glue.object.as_str()),
        Some(target.as_str())
    );

    // Null clears that end only.
    s.execute(
        "object.set_path",
        &json!({
            "slide": "slide1",
            "object": connector,
            "points": [{"x": 30, "y": 30}, {"x": 260, "y": 150}],
            "end_glue": null
        }),
    )
    .expect("clear glue");
    let (_, path) = path_of(&s, &connector);
    assert!(path.end_glue.is_none());
}

#[test]
fn deleting_a_target_clears_glue_and_undo_restores_both() {
    let mut s = Session::blank("Connectors");
    let target = add_shape(&mut s, 200.0, 100.0, 100.0, 80.0);
    let connector = add_connector(&mut s, &target, 3);
    let before = s.course.clone();
    let old_end = endpoint(&s, &connector);

    s.execute(
        "object.delete",
        &json!({ "slide": "slide1", "object": target }),
    )
    .expect("delete target");

    let (_, path) = path_of(&s, &connector);
    assert!(path.end_glue.is_none());
    assert_near(endpoint(&s, &connector), old_end);

    s.execute("edit.undo", &json!({})).expect("undo");
    assert_eq!(s.course, before);
    let (_, path) = path_of(&s, &connector);
    assert_eq!(
        path.end_glue.as_ref().map(|glue| glue.object.as_str()),
        Some(target.as_str())
    );
}

#[test]
fn a_target_in_another_owner_loses_its_glue() {
    let mut s = Session::blank("Connectors");
    let layer = s
        .execute("layer.add", &json!({ "slide": "slide1", "id": "layer1" }))
        .expect("layer")["id"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let target = s
        .execute(
            "object.add",
            &json!({
                "slide": "slide1",
                "layer": layer,
                "kind": {
                    "type": "shape",
                    "shape": "rectangle",
                    "fill": "#D9D9D9"
                },
                "frame": { "x": 200, "y": 100, "w": 100, "h": 80 }
            }),
        )
        .expect("layer shape")["id"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let connector = add_connector(&mut s, &target, 1);
    let (_, path) = path_of(&s, &connector);
    assert!(
        path.end_glue.is_none(),
        "a base connector cannot glue to a layer object"
    );
}

#[test]
fn hostile_ids_and_sites_clear_without_panicking() {
    let mut s = Session::blank("Connectors");
    let _target = add_shape(&mut s, 200.0, 100.0, 100.0, 80.0);
    let connector = s
        .execute(
            "object.add",
            &json!({
                "slide": "slide1",
                "kind": {
                    "type": "path",
                    "points": [{"x": 0.0, "y": 0.0}, {"x": 1.0, "y": 1.0}],
                    "connector": "elbow",
                    "outline": {"color": "#1F1F1F", "width": 2}
                },
                "frame": { "x": 0, "y": 0, "w": 100, "h": 100 }
            }),
        )
        .expect("connector")["id"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    s.execute(
        "object.set_path",
        &json!({
            "slide": "slide1",
            "object": connector,
            "points": [{"x": 10, "y": 10}, {"x": 90, "y": 90}],
            "start_glue": {"object": "../../\u{0}evil", "site": 255},
            "end_glue": {"object": "missing", "site": 99}
        }),
    )
    .expect("hostile glue params");
    let (_, path) = path_of(&s, &connector);
    assert!(path.start_glue.is_none());
    assert!(path.end_glue.is_none());
    assert!(path.points.iter().all(PathPoint::is_valid));
}

#[test]
fn duplicate_and_paste_remap_glue_only_when_the_target_is_copied() {
    let mut s = Session::blank("Connectors");
    let target = add_shape(&mut s, 200.0, 100.0, 100.0, 80.0);
    let connector = add_connector(&mut s, &target, 0);

    let copy = s
        .execute(
            "object.duplicate",
            &json!({ "slide": "slide1", "objects": [target, connector] }),
        )
        .expect("duplicate pair");
    let ids = copy["ids"].as_array().cloned().unwrap_or_default();
    assert_eq!(ids.len(), 2);
    let new_target = ids[0].as_str().unwrap_or_default();
    let new_connector = ids[1].as_str().unwrap_or_default();
    let (_, path) = path_of(&s, new_connector);
    assert_eq!(
        path.end_glue.as_ref().map(|glue| glue.object.as_str()),
        Some(new_target)
    );

    let copy = s
        .execute(
            "object.duplicate",
            &json!({ "slide": "slide1", "object": connector }),
        )
        .expect("duplicate connector alone");
    let alone = copy["id"].as_str().unwrap_or_default();
    let (_, path) = path_of(&s, alone);
    assert!(path.end_glue.is_none());

    let (target_object, _) = s
        .course
        .slide("slide1")
        .and_then(|slide| slide.object(&target))
        .map(|(object, _)| (object.clone(), ()))
        .expect("target object");
    let (connector_object, _) = s
        .course
        .slide("slide1")
        .and_then(|slide| slide.object(&connector))
        .map(|(object, _)| (object.clone(), ()))
        .expect("connector object");
    let pasted = s
        .execute(
            "object.paste",
            &json!({
                "slide": "slide1",
                "objects": [target_object, connector_object],
                "offset": 30
            }),
        )
        .expect("paste pair");
    let ids = pasted["ids"].as_array().cloned().unwrap_or_default();
    assert_eq!(ids.len(), 2);
    let pasted_target = ids[0].as_str().unwrap_or_default();
    let pasted_connector = ids[1].as_str().unwrap_or_default();
    let (_, path) = path_of(&s, pasted_connector);
    assert_eq!(
        path.end_glue.as_ref().map(|glue| glue.object.as_str()),
        Some(pasted_target)
    );

    let (connector_object, _) = s
        .course
        .slide("slide1")
        .and_then(|slide| slide.object(&connector))
        .map(|(object, _)| (object.clone(), ()))
        .expect("connector object");
    let pasted = s
        .execute(
            "object.paste",
            &json!({
                "slide": "slide1",
                "objects": [connector_object],
                "offset": 30
            }),
        )
        .expect("paste connector alone");
    let alone = pasted["id"].as_str().unwrap_or_default();
    let (_, path) = path_of(&s, alone);
    assert!(path.end_glue.is_none());
}

#[test]
fn duplicating_a_slide_or_layer_remaps_glue_inside_the_copy() {
    let mut s = Session::blank("Connectors");
    let target = add_shape(&mut s, 200.0, 100.0, 100.0, 80.0);
    let _connector = add_connector(&mut s, &target, 0);

    let copy = s
        .execute("slide.duplicate", &json!({ "slide": "slide1" }))
        .expect("slide duplicate");
    let new_slide = copy["id"].as_str().unwrap_or_default();
    let slide = s.course.slide(new_slide).expect("copied slide");
    let new_target = slide.objects[0].id.clone();
    let new_connector = slide.objects[1].id.clone();
    let ObjectKind::Path { path } = &slide.objects[1].kind else {
        panic!("copied connector");
    };
    assert_eq!(
        path.end_glue.as_ref().map(|glue| glue.object.as_str()),
        Some(new_target.as_str())
    );
    let _ = new_connector;

    let layer = s
        .execute("layer.add", &json!({ "slide": "slide1", "id": "layer1" }))
        .expect("layer")["id"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let layer_target = s
        .execute(
            "object.add",
            &json!({
                "slide": "slide1",
                "layer": layer,
                "kind": {
                    "type": "shape",
                    "shape": "rectangle",
                    "fill": "#D9D9D9"
                },
                "frame": { "x": 500, "y": 300, "w": 100, "h": 80 }
            }),
        )
        .expect("layer target")["id"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let layer_connector = {
        let id = s
            .execute(
                "object.add",
                &json!({
                    "slide": "slide1",
                    "layer": layer,
                    "kind": {
                        "type": "path",
                        "points": [{"x": 0.0, "y": 0.0}, {"x": 1.0, "y": 1.0}],
                        "connector": "straight",
                        "outline": {"color": "#1F1F1F", "width": 2}
                    },
                    "frame": { "x": 0, "y": 0, "w": 100, "h": 100 }
                }),
            )
            .expect("layer connector")["id"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        s.execute(
            "object.set_path",
            &json!({
                "slide": "slide1",
                "object": id,
                "points": [{"x": 450, "y": 300}, {"x": 500, "y": 340}],
                "end_glue": {"object": layer_target, "site": 3}
            }),
        )
        .expect("layer glue");
        id
    };

    let copy = s
        .execute(
            "layer.duplicate",
            &json!({ "slide": "slide1", "layer": layer }),
        )
        .expect("layer duplicate");
    let new_layer = copy["id"].as_str().unwrap_or_default();
    let layer = s
        .course
        .slide("slide1")
        .and_then(|slide| slide.layer(new_layer))
        .expect("copied layer");
    let copied_connector = layer
        .objects
        .iter()
        .find_map(|object| match &object.kind {
            ObjectKind::Path { path } if path.connector.is_some() => Some(path),
            _ => None,
        })
        .expect("copied connector path");
    assert!(
        copied_connector.end_glue.is_some(),
        "the target inside the duplicated layer is copied too"
    );
    assert_ne!(
        copied_connector
            .end_glue
            .as_ref()
            .map(|glue| glue.object.as_str()),
        Some(layer_target.as_str())
    );
    let _ = layer_connector;
}

#[test]
fn big_slide_reroutes_quickly() {
    let mut s = Session::blank("Big");
    let slide = &mut s.course.scenes[0].slides[0];
    let outline = Outline {
        color: Color("#1F1F1F".into()),
        width: 2.0,
        dash: DashStyle::Solid,
    };
    for index in 0..2000usize {
        #[allow(clippy::cast_precision_loss)]
        let x = (index % 50) as f32 * 20.0;
        #[allow(clippy::cast_precision_loss)]
        let y = (index / 50) as f32 * 18.0;
        let target = index < 200;
        slide.objects.push(Object {
            id: format!("object{index}"),
            name: String::new(),
            kind: if target {
                ObjectKind::Shape {
                    shape: ShapeKind::Rectangle,
                    fill: Color("#D9D9D9".into()),
                    fill_style: None,
                    outline: None,
                    text: None,
                    adjust: Vec::new(),
                }
            } else {
                ObjectKind::Path {
                    path: Path {
                        points: vec![PathPoint { x: 0.0, y: 0.0 }, PathPoint { x: 1.0, y: 1.0 }],
                        style: coursecraft_course::PathStyle::Straight,
                        closed: false,
                        fill: None,
                        outline: outline.clone(),
                        start_end: LineEnd::None,
                        end_end: LineEnd::None,
                        connector: target.then_some(ConnectorStyle::Elbow),
                        start_glue: target.then(|| Glue {
                            object: format!("object{index}"),
                            site: 0,
                        }),
                        end_glue: None,
                    },
                }
            },
            frame: Rect {
                x,
                y,
                w: 100.0,
                h: 60.0,
            },
            timing: Default::default(),
            states: Vec::new(),
            initial_state: "normal".into(),
            entrance: None,
            exit: None,
            motion_paths: Vec::new(),
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
        });
    }
    for index in 0..200usize {
        let target = format!("object{index}");
        slide.objects[index].frame.x += 3.0;
        let connector = Object {
            id: format!("connector{index}"),
            name: String::new(),
            kind: ObjectKind::Path {
                path: Path {
                    points: vec![PathPoint { x: 0.0, y: 0.0 }, PathPoint { x: 1.0, y: 1.0 }],
                    style: coursecraft_course::PathStyle::Straight,
                    closed: false,
                    fill: None,
                    outline: outline.clone(),
                    start_end: LineEnd::None,
                    end_end: LineEnd::None,
                    connector: Some(ConnectorStyle::Elbow),
                    start_glue: Some(Glue {
                        object: target,
                        site: (index % 4) as u8,
                    }),
                    end_glue: None,
                },
            },
            frame: Rect {
                x: 0.0,
                y: 0.0,
                w: 10.0,
                h: 10.0,
            },
            timing: Default::default(),
            states: Vec::new(),
            initial_state: "normal".into(),
            entrance: None,
            exit: None,
            motion_paths: Vec::new(),
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
        };
        slide.objects.push(connector);
    }
    let start = std::time::Instant::now();
    reroute_connectors(&mut s.course);
    let elapsed = start.elapsed();
    assert!(
        elapsed < std::time::Duration::from_secs(2),
        "rerouting a 2200-object slide took {elapsed:?}"
    );
}
