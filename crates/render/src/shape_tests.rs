//! Shape geometry, fill and catalog tests, plus the gallery course writer.

use std::path::Path;

use coursecraft_course::{
    Color, Course, Object, ObjectKind, Rect as ModelRect, SCHEMA_VERSION, SHAPE_CATALOG, Scene,
    Severity, ShapeCategory, ShapeKind, Slide, TextStyle, Timing, adjust_defaults, adjust_spec,
    shape_is_adjustable, shape_label,
};
use egui::Color32;

use crate::shape_fill::even_odd_area;
use crate::shape_fill::stroke_contours;
use crate::shape_geometry::{shape_contours, shape_text_rect};

const SIZES: [(f32, f32); 4] = [(200.0, 120.0), (120.0, 200.0), (40.0, 40.0), (1.0, 1000.0)];

fn mesh_area(mesh: &egui::Mesh) -> f32 {
    mesh.triangles()
        .map(|[a, b, c]| {
            let (Some(&pa), Some(&pb), Some(&pc)) = (
                mesh.vertices.get(a as usize),
                mesh.vertices.get(b as usize),
                mesh.vertices.get(c as usize),
            ) else {
                return 0.0;
            };
            let a = (pa.pos.x, pa.pos.y);
            let b = (pb.pos.x, pb.pos.y);
            let c = (pc.pos.x, pc.pos.y);
            ((b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0)).abs() * 0.5
        })
        .sum()
}

#[test]
fn every_catalog_kind_is_finite_inside_its_frame_and_fills_its_area() {
    for info in SHAPE_CATALOG {
        for (w, h) in SIZES {
            let contours = shape_contours(info.kind, w, h);
            assert!(
                !contours.is_empty(),
                "{} at {w}x{h} produced no contours",
                info.label
            );
            for contour in &contours {
                assert!(
                    contour.len() >= 3,
                    "{} at {w}x{h} has a tiny contour",
                    info.label
                );
                for &(x, y) in contour {
                    assert!(x.is_finite() && y.is_finite(), "{}", info.label);
                    assert!(
                        (-0.5..=w + 0.5).contains(&x) && (-0.5..=h + 0.5).contains(&y),
                        "{} at {w}x{h} has point ({x}, {y}) outside the frame",
                        info.label
                    );
                }
            }
            let expected = even_odd_area(&contours);
            assert!(
                expected > 0.0,
                "{} at {w}x{h} has no even-odd area",
                info.label
            );
            let mesh = crate::shape_fill::fill_contours(&contours, Color32::WHITE, 0.0);
            assert!(
                !mesh.indices.is_empty(),
                "{} at {w}x{h} produced no triangles",
                info.label
            );
            let got = mesh_area(&mesh);
            assert!(
                (got - expected).abs() <= expected * 0.01 + 1e-3,
                "{} at {w}x{h}: filled area {got} differs from even-odd area {expected}",
                info.label
            );
        }
    }
}

#[test]
fn classic_shapes_keep_their_old_outline() {
    let rectangle = shape_contours(ShapeKind::Rectangle, 200.0, 120.0);
    assert_eq!(rectangle.len(), 1);
    assert_eq!(rectangle.first().map(Vec::len), Some(4));
    let bounds = rectangle.first().map(|points| {
        let min_x = points.iter().map(|p| p.0).fold(f32::INFINITY, f32::min);
        let max_x = points.iter().map(|p| p.0).fold(f32::NEG_INFINITY, f32::max);
        let min_y = points.iter().map(|p| p.1).fold(f32::INFINITY, f32::min);
        let max_y = points.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max);
        (min_x, min_y, max_x, max_y)
    });
    assert_eq!(bounds, Some((0.0, 0.0, 200.0, 120.0)));

    let rounded = shape_contours(ShapeKind::RoundedRectangle, 200.0, 120.0);
    let rounded_bounds = rounded.first().map(|points| {
        let min_x = points.iter().map(|p| p.0).fold(f32::INFINITY, f32::min);
        let max_x = points.iter().map(|p| p.0).fold(f32::NEG_INFINITY, f32::max);
        let min_y = points.iter().map(|p| p.1).fold(f32::INFINITY, f32::min);
        let max_y = points.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max);
        (min_x, min_y, max_x, max_y)
    });
    assert_eq!(rounded_bounds, Some((0.0, 0.0, 200.0, 120.0)));

    let ellipse = shape_contours(ShapeKind::Ellipse, 200.0, 120.0);
    assert!(ellipse.first().is_some_and(|points| points.len() >= 32));
    let area = even_odd_area(&ellipse);
    let expected = std::f32::consts::PI * 100.0 * 60.0;
    assert!(
        (area - expected).abs() <= expected * 0.02,
        "ellipse area {area} vs {expected}"
    );
}

#[test]
fn shape_kind_json_round_trips_for_every_variant() {
    for &kind in ShapeKind::ALL {
        let text = serde_json::to_string(&kind).expect("serialise shape kind");
        let back: ShapeKind = serde_json::from_str(&text).expect("parse shape kind");
        assert_eq!(back, kind);
        assert!(text.starts_with('"') && text.ends_with('"'));
    }
    assert_eq!(
        serde_json::to_string(&ShapeKind::ArrowRight).expect("json"),
        "\"arrow_right\""
    );
    assert_eq!(
        serde_json::to_string(&ShapeKind::ArrowUTurn).expect("json"),
        "\"arrow_u_turn\""
    );
    assert_eq!(
        serde_json::to_string(&ShapeKind::RoundedRectangle).expect("json"),
        "\"rounded_rectangle\""
    );
}

#[test]
fn geometry_helpers_produce_a_bracket_band() {
    let band = crate::shape_geometry::stroke_polyline(
        &[(2.0, 0.0), (2.0, 10.0), (8.0, 10.0), (8.0, 0.0)],
        2.0,
    );
    assert!(band.len() >= 6);
    let surface = crate::shape_geometry::rounded_polygon(
        &[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)],
        2.0,
    );
    assert!(surface.len() > 4);
    let star = crate::shape_geometry::star(5, 0.4);
    assert_eq!(star.len(), 10);
    assert!(crate::shape_geometry::regular_polygon(3).len() == 3);
}

#[test]
fn arrow_callouts_are_a_big_box_with_thin_arrows() {
    for kind in [
        ShapeKind::CalloutRightArrow,
        ShapeKind::CalloutDownArrow,
        ShapeKind::CalloutLeftArrow,
        ShapeKind::CalloutUpArrow,
        ShapeKind::CalloutLeftRightArrow,
        ShapeKind::CalloutQuadArrow,
    ] {
        let contours = shape_contours(kind, 200.0, 120.0);
        assert!(contours.len() >= 2, "{kind:?} has no separate box");
        let box_area = even_odd_area(std::slice::from_ref(&contours[0]));
        let arrow_area: f32 = contours
            .iter()
            .skip(1)
            .map(|contour| even_odd_area(std::slice::from_ref(contour)))
            .sum();
        assert!(
            box_area > arrow_area,
            "{kind:?}: box {box_area} is not bigger than its arrows {arrow_area}"
        );
    }
}

#[test]
fn striped_right_arrow_has_two_bars_then_a_gap_then_the_arrow() {
    let contours = shape_contours(ShapeKind::ArrowStripedRight, 200.0, 120.0);
    assert_eq!(contours.len(), 3, "two bars and the arrow body");
    let bounds = |points: &[(f32, f32)]| {
        points.iter().fold(
            (
                f32::INFINITY,
                f32::NEG_INFINITY,
                f32::INFINITY,
                f32::NEG_INFINITY,
            ),
            |(min_x, max_x, min_y, max_y), &(x, y)| {
                (min_x.min(x), max_x.max(x), min_y.min(y), max_y.max(y))
            },
        )
    };
    let first = bounds(&contours[0]);
    let second = bounds(&contours[1]);
    let third = bounds(&contours[2]);
    assert!(first.1 < second.0, "the two bars overlap");
    assert!(second.1 < third.0, "the arrow starts too close to the bars");
    assert!(first.1 - first.0 < third.1 - third.0, "the bars are shafts");
}

#[test]
fn notched_right_arrow_cuts_its_tail_inward() {
    let contour = shape_contours(ShapeKind::ArrowNotchedRight, 200.0, 120.0)
        .into_iter()
        .next()
        .expect("contour");
    let mid = 60.0;
    let inward = contour
        .iter()
        .any(|&(x, y)| (y - mid).abs() < 1.0 && x > 1.0);
    assert!(inward, "the tail should have an inward notch at the middle");
}

fn shape_object(id: &str, kind: ShapeKind, frame: ModelRect) -> Object {
    Object {
        id: id.to_owned(),
        name: shape_label(kind).to_owned(),
        kind: ObjectKind::Shape {
            shape: kind,
            fill: Color("#D0D7DE".to_owned()),
            fill_style: None,
            outline: None,
            text: None,
            adjust: Vec::new(),
        },
        frame,
        timing: Timing::default(),
        states: Vec::new(),
        initial_state: "normal".to_owned(),
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
    }
}

fn shape_object_adjusted(id: &str, kind: ShapeKind, frame: ModelRect, adjust: Vec<f32>) -> Object {
    let mut object = shape_object(id, kind, frame);
    if let ObjectKind::Shape { adjust: slot, .. } = &mut object.kind {
        *slot = adjust;
    }
    object
}

fn shape_with_text(
    id: &str,
    kind: ShapeKind,
    fill: Color,
    outline: Option<coursecraft_course::Outline>,
    text: &str,
    frame: ModelRect,
) -> Object {
    Object {
        id: id.to_owned(),
        name: text.to_owned(),
        kind: ObjectKind::Shape {
            shape: kind,
            fill,
            fill_style: None,
            outline,
            text: Some(coursecraft_course::ShapeText {
                text: text.to_owned(),
                style: TextStyle {
                    size: 14.0,
                    color: Color("#1F1F1F".to_owned()),
                    align: coursecraft_course::Align::Center,
                    valign: coursecraft_course::VAlign::Middle,
                    ..TextStyle::default()
                },
                runs: Vec::new(),
                paragraphs: Vec::new(),
                autofit: coursecraft_course::Autofit::None,
            }),
            adjust: Vec::new(),
        },
        frame,
        timing: Timing::default(),
        states: Vec::new(),
        initial_state: "normal".to_owned(),
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
    }
}

fn outline_text_slide() -> Slide {
    let specs: [(ShapeKind, &str, &str, bool); 14] = [
        (ShapeKind::Rectangle, "Rectangle", "#D0D7DE", false),
        (ShapeKind::RoundedRectangle, "Rounded", "#D0D7DE", false),
        (ShapeKind::Ellipse, "Ellipse", "#D0D7DE", false),
        (ShapeKind::Diamond, "Diamond", "#D0D7DE", false),
        (ShapeKind::Triangle, "Triangle", "#D0D7DE", false),
        (ShapeKind::ArrowRight, "Arrow", "#D0D7DE", false),
        (ShapeKind::ArrowChevron, "Chevron", "#D0D7DE", false),
        (ShapeKind::CalloutRectangle, "Callout", "#D0D7DE", false),
        (ShapeKind::Star5, "Star", "#D0D7DE", false),
        (ShapeKind::Cloud, "Cloud", "#D0D7DE", false),
        (ShapeKind::Heart, "Heart", "#D0D7DE", false),
        (ShapeKind::FlowDecision, "Decision", "#D0D7DE", false),
        (ShapeKind::Ellipse, "No fill", "#00000000", true),
        (ShapeKind::Rectangle, "Dashed", "#00000000", true),
    ];
    let mut objects = Vec::new();
    for (i, (kind, label, fill, dashed)) in specs.iter().enumerate() {
        let col = i % 7;
        let row = i / 7;
        let x = 40.0 + col as f32 * 172.0;
        let y = 60.0 + row as f32 * 190.0;
        let dash = if *dashed {
            if label == &"Dashed" {
                coursecraft_course::DashStyle::Dash
            } else {
                coursecraft_course::DashStyle::Dot
            }
        } else if i % 2 == 0 {
            coursecraft_course::DashStyle::Dash
        } else {
            coursecraft_course::DashStyle::Solid
        };
        let width = if *dashed { 3.0 } else { 2.5 };
        objects.push(shape_with_text(
            &format!("outline{i}"),
            *kind,
            Color((*fill).to_owned()),
            Some(coursecraft_course::Outline {
                color: Color("#2F6FEB".to_owned()),
                width,
                dash,
            }),
            label,
            ModelRect {
                x,
                y,
                w: 150.0,
                h: 110.0,
            },
        ));
    }
    Slide {
        id: "shapes-outline-text".to_owned(),
        title: "Shapes: outline, dash, no fill and text".to_owned(),
        duration_ms: 5000,
        background: None,
        objects,
        layers: Vec::new(),
        triggers: Vec::new(),
        question: None,
        draw: None,
        transition: None,
        notes: String::new(),
        master: None,
        revisit: coursecraft_course::RevisitMode::Auto,
        player: None,
    }
}

fn adjust_slide(text_color: Color) -> Slide {
    let rows: Vec<_> = SHAPE_CATALOG
        .iter()
        .filter(|info| shape_is_adjustable(info.kind))
        .collect();
    let columns = 8usize;
    let cell_w = 150.0f32;
    let cell_h = 92.0f32;
    let margin = 20.0f32;
    let shape_w = 100.0f32;
    let shape_h = 58.0f32;
    let mut objects = Vec::new();
    for (i, info) in rows.iter().enumerate() {
        let col = i % columns;
        let row = i / columns;
        let x = margin + col as f32 * cell_w + (cell_w - shape_w) * 0.5;
        let y = 16.0 + row as f32 * cell_h;
        objects.push(shape_object_adjusted(
            &format!("adjust{i}"),
            info.kind,
            ModelRect {
                x,
                y,
                w: shape_w,
                h: shape_h,
            },
            non_default_adjust(info.kind),
        ));
        objects.push(label_object(
            &format!("adjust-label{i}"),
            info.label,
            ModelRect {
                x: x - 14.0,
                y: y + shape_h + 4.0,
                w: shape_w + 28.0,
                h: 22.0,
            },
            text_color.clone(),
        ));
    }
    Slide {
        id: "shapes-adjust".to_owned(),
        title: "Shapes: adjustment handles at non-default values".to_owned(),
        duration_ms: 5000,
        background: None,
        objects,
        layers: Vec::new(),
        triggers: Vec::new(),
        question: None,
        draw: None,
        transition: None,
        notes: String::new(),
        master: None,
        revisit: coursecraft_course::RevisitMode::Auto,
        player: None,
    }
}

fn label_object(id: &str, text: &str, frame: ModelRect, color: Color) -> Object {
    Object {
        id: id.to_owned(),
        name: text.to_owned(),
        kind: ObjectKind::Text {
            text: text.to_owned(),
            style: TextStyle {
                size: 10.0,
                color,
                align: coursecraft_course::Align::Center,
                ..TextStyle::default()
            },
            runs: Vec::new(),
            paragraphs: Vec::new(),
            autofit: coursecraft_course::Autofit::None,
            text_style: None,
        },
        frame,
        timing: Timing::default(),
        states: Vec::new(),
        initial_state: "normal".to_owned(),
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
    }
}

fn category_slide(category: ShapeCategory, text_color: Color) -> Slide {
    let rows: Vec<_> = SHAPE_CATALOG
        .iter()
        .filter(|info| info.category == category)
        .collect();
    // The full catalog has categories with forty kinds, so the review sheet
    // uses a tighter grid than step 3's first three categories needed.
    let columns = 10usize;
    let cell_w = 120.0f32;
    let cell_h = 124.0f32;
    let margin = 25.0f32;
    let mut objects = Vec::new();
    for (i, info) in rows.iter().enumerate() {
        // Show each kind at the proportions placement gives it, so vertical
        // arrows and square polygons are judged as authors will see them.
        let (shape_w, shape_h) = if info.tall {
            (54.0, 90.0)
        } else if info.square {
            (70.0, 70.0)
        } else {
            (100.0, 70.0)
        };
        let col = i % columns;
        let row = i / columns;
        let x = margin + col as f32 * cell_w + (cell_w - shape_w) * 0.5;
        let y = 18.0 + row as f32 * cell_h;
        objects.push(shape_object(
            &format!("shape{i}"),
            info.kind,
            ModelRect {
                x,
                y,
                w: shape_w,
                h: shape_h,
            },
        ));
        objects.push(label_object(
            &format!("label{i}"),
            info.label,
            ModelRect {
                x: x - 8.0,
                y: y + shape_h + 6.0,
                w: shape_w + 16.0,
                h: 24.0,
            },
            text_color.clone(),
        ));
    }
    Slide {
        id: format!("shapes-{}", category_slug(category)),
        title: format!("Shapes: {}", category.label()),
        duration_ms: 5000,
        background: None,
        objects,
        layers: Vec::new(),
        triggers: Vec::new(),
        question: None,
        draw: None,
        transition: None,
        notes: String::new(),
        master: None,
        revisit: coursecraft_course::RevisitMode::Auto,
        player: None,
    }
}

fn category_slug(category: ShapeCategory) -> &'static str {
    match category {
        ShapeCategory::Lines => "lines",
        ShapeCategory::Rectangles => "rectangles",
        ShapeCategory::BasicShapes => "basic-shapes",
        ShapeCategory::BlockArrows => "block-arrows",
        ShapeCategory::EquationShapes => "equation-shapes",
        ShapeCategory::Flowchart => "flowchart",
        ShapeCategory::StarsAndBanners => "stars-and-banners",
        ShapeCategory::Callouts => "callouts",
        ShapeCategory::ActionButtons => "action-buttons",
        ShapeCategory::Learning => "learning",
    }
}

fn draw_slide(text_color: Color) -> Slide {
    let outline = coursecraft_course::Outline {
        color: text_color.clone(),
        width: 2.0,
        dash: coursecraft_course::DashStyle::Solid,
    };
    let mut objects = Vec::new();
    let specs: Vec<(&str, coursecraft_course::Path)> = vec![
        (
            "Line",
            coursecraft_course::Path {
                points: vec![
                    coursecraft_course::PathPoint { x: 0.0, y: 1.0 },
                    coursecraft_course::PathPoint { x: 1.0, y: 0.0 },
                ],
                style: coursecraft_course::PathStyle::Straight,
                closed: false,
                fill: None,
                outline: outline.clone(),
                start_end: coursecraft_course::LineEnd::None,
                end_end: coursecraft_course::LineEnd::None,
                connector: None,
                start_glue: None,
                end_glue: None,
            },
        ),
        (
            "Arrow",
            coursecraft_course::Path {
                points: vec![
                    coursecraft_course::PathPoint { x: 0.0, y: 0.5 },
                    coursecraft_course::PathPoint { x: 1.0, y: 0.5 },
                ],
                style: coursecraft_course::PathStyle::Straight,
                closed: false,
                fill: None,
                outline: outline.clone(),
                start_end: coursecraft_course::LineEnd::None,
                end_end: coursecraft_course::LineEnd::Arrow,
                connector: None,
                start_glue: None,
                end_glue: None,
            },
        ),
        (
            "Elbow connector",
            coursecraft_course::Path {
                points: vec![
                    coursecraft_course::PathPoint { x: 0.0, y: 0.0 },
                    coursecraft_course::PathPoint { x: 1.0, y: 1.0 },
                ],
                style: coursecraft_course::PathStyle::Straight,
                closed: false,
                fill: None,
                outline: outline.clone(),
                start_end: coursecraft_course::LineEnd::None,
                end_end: coursecraft_course::LineEnd::None,
                connector: Some(coursecraft_course::ConnectorStyle::Elbow),
                start_glue: None,
                end_glue: None,
            },
        ),
        (
            "Curved connector",
            coursecraft_course::Path {
                points: vec![
                    coursecraft_course::PathPoint { x: 0.0, y: 0.0 },
                    coursecraft_course::PathPoint { x: 1.0, y: 1.0 },
                ],
                style: coursecraft_course::PathStyle::Straight,
                closed: false,
                fill: None,
                outline: outline.clone(),
                start_end: coursecraft_course::LineEnd::None,
                end_end: coursecraft_course::LineEnd::None,
                connector: Some(coursecraft_course::ConnectorStyle::Curved),
                start_glue: None,
                end_glue: None,
            },
        ),
        (
            "Curve",
            coursecraft_course::Path {
                points: vec![
                    coursecraft_course::PathPoint { x: 0.0, y: 1.0 },
                    coursecraft_course::PathPoint { x: 0.5, y: 0.0 },
                    coursecraft_course::PathPoint { x: 1.0, y: 1.0 },
                ],
                style: coursecraft_course::PathStyle::Curved,
                closed: false,
                fill: None,
                outline: outline.clone(),
                start_end: coursecraft_course::LineEnd::None,
                end_end: coursecraft_course::LineEnd::None,
                connector: None,
                start_glue: None,
                end_glue: None,
            },
        ),
        (
            "Freeform",
            coursecraft_course::Path {
                points: vec![
                    coursecraft_course::PathPoint { x: 0.0, y: 0.6 },
                    coursecraft_course::PathPoint { x: 0.2, y: 0.0 },
                    coursecraft_course::PathPoint { x: 0.6, y: 1.0 },
                    coursecraft_course::PathPoint { x: 1.0, y: 0.4 },
                ],
                style: coursecraft_course::PathStyle::Straight,
                closed: false,
                fill: None,
                outline: outline.clone(),
                start_end: coursecraft_course::LineEnd::None,
                end_end: coursecraft_course::LineEnd::None,
                connector: None,
                start_glue: None,
                end_glue: None,
            },
        ),
        (
            "Scribble",
            coursecraft_course::Path {
                points: vec![
                    coursecraft_course::PathPoint { x: 0.0, y: 0.5 },
                    coursecraft_course::PathPoint { x: 0.2, y: 0.1 },
                    coursecraft_course::PathPoint { x: 0.4, y: 0.9 },
                    coursecraft_course::PathPoint { x: 0.6, y: 0.2 },
                    coursecraft_course::PathPoint { x: 0.8, y: 0.8 },
                    coursecraft_course::PathPoint { x: 1.0, y: 0.5 },
                ],
                style: coursecraft_course::PathStyle::Straight,
                closed: false,
                fill: None,
                outline: outline.clone(),
                start_end: coursecraft_course::LineEnd::None,
                end_end: coursecraft_course::LineEnd::None,
                connector: None,
                start_glue: None,
                end_glue: None,
            },
        ),
        (
            "Closed fill",
            coursecraft_course::Path {
                points: vec![
                    coursecraft_course::PathPoint { x: 0.0, y: 0.0 },
                    coursecraft_course::PathPoint { x: 1.0, y: 0.2 },
                    coursecraft_course::PathPoint { x: 0.7, y: 1.0 },
                    coursecraft_course::PathPoint { x: 0.2, y: 0.8 },
                ],
                style: coursecraft_course::PathStyle::Straight,
                closed: true,
                fill: Some(Color("#2F6FEB".to_owned())),
                outline: outline.clone(),
                start_end: coursecraft_course::LineEnd::None,
                end_end: coursecraft_course::LineEnd::None,
                connector: None,
                start_glue: None,
                end_glue: None,
            },
        ),
    ];
    for (index, (label, path)) in specs.into_iter().enumerate() {
        let col = index % 4;
        let row = index / 4;
        let x = 40.0 + col as f32 * 300.0;
        let y = 40.0 + row as f32 * 200.0;
        let frame = ModelRect {
            x,
            y,
            w: 180.0,
            h: 110.0,
        };
        objects.push(Object {
            id: format!("draw{index}"),
            name: label.to_owned(),
            kind: ObjectKind::Path { path },
            frame,
            timing: Timing::default(),
            states: Vec::new(),
            initial_state: "normal".to_owned(),
            entrance: None,
            exit: None,
            motion_paths: Vec::new(),
            focus_order: None,
            decorative: false,
            description: Some(label.to_owned()),
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
        objects.push(label_object(
            &format!("draw-label{index}"),
            label,
            ModelRect {
                x: x - 20.0,
                y: y + 116.0,
                w: 220.0,
                h: 22.0,
            },
            text_color.clone(),
        ));
    }

    // Two shapes joined by a glued connector, so the gallery shows that moving
    // a target carries its connector end along.
    let left_frame = ModelRect {
        x: 40.0,
        y: 470.0,
        w: 160.0,
        h: 90.0,
    };
    let right_frame = ModelRect {
        x: 400.0,
        y: 470.0,
        w: 160.0,
        h: 90.0,
    };
    for (id, frame, label) in [
        ("connected-left", left_frame, "Connected A"),
        ("connected-right", right_frame, "Connected B"),
    ] {
        objects.push(Object {
            id: id.to_owned(),
            name: label.to_owned(),
            kind: ObjectKind::Shape {
                shape: ShapeKind::Rectangle,
                fill: Color("#D9D9D9".to_owned()),
                fill_style: None,
                outline: None,
                text: None,
                adjust: Vec::new(),
            },
            frame,
            timing: Timing::default(),
            states: Vec::new(),
            initial_state: "normal".to_owned(),
            entrance: None,
            exit: None,
            motion_paths: Vec::new(),
            focus_order: None,
            decorative: false,
            description: Some(label.to_owned()),
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
    let connector_start = ModelRect {
        x: left_frame.x + left_frame.w,
        y: left_frame.y + left_frame.h * 0.5,
        w: 1.0,
        h: 1.0,
    };
    let right_point = (right_frame.x, right_frame.y + right_frame.h * 0.5);
    let slide_points = [
        coursecraft_course::Point {
            x: connector_start.x,
            y: connector_start.y,
        },
        coursecraft_course::Point {
            x: right_point.0,
            y: right_point.1,
        },
    ];
    let (frame, points) =
        coursecraft_course::path_frame_and_points(&slide_points).expect("connector geometry");
    objects.push(Object {
        id: "connected-connector".to_owned(),
        name: "Connected connector".to_owned(),
        kind: ObjectKind::Path {
            path: coursecraft_course::Path {
                points,
                style: coursecraft_course::PathStyle::Straight,
                closed: false,
                fill: None,
                outline: coursecraft_course::Outline {
                    color: text_color.clone(),
                    width: 2.0,
                    dash: coursecraft_course::DashStyle::Solid,
                },
                start_end: coursecraft_course::LineEnd::None,
                end_end: coursecraft_course::LineEnd::None,
                connector: Some(coursecraft_course::ConnectorStyle::Straight),
                start_glue: Some(coursecraft_course::Glue {
                    object: "connected-left".to_owned(),
                    site: 1,
                }),
                end_glue: Some(coursecraft_course::Glue {
                    object: "connected-right".to_owned(),
                    site: 3,
                }),
            },
        },
        frame,
        timing: Timing::default(),
        states: Vec::new(),
        initial_state: "normal".to_owned(),
        entrance: None,
        exit: None,
        motion_paths: Vec::new(),
        focus_order: None,
        decorative: false,
        description: Some("Connector between two shapes".to_owned()),
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
    objects.push(label_object(
        "connected-label",
        "Connected shapes",
        ModelRect {
            x: 40.0,
            y: 566.0,
            w: 400.0,
            h: 22.0,
        },
        text_color.clone(),
    ));
    Slide {
        id: "shapes-draw".to_owned(),
        title: "Shapes: draw tools".to_owned(),
        duration_ms: 5000,
        background: None,
        objects,
        layers: Vec::new(),
        triggers: Vec::new(),
        question: None,
        draw: None,
        transition: None,
        notes: String::new(),
        master: None,
        revisit: coursecraft_course::RevisitMode::Auto,
        player: None,
    }
}

#[test]
fn outlines_draw_bands_on_convex_concave_and_holed_shapes() {
    for kind in [
        ShapeKind::Rectangle,
        ShapeKind::LightningBolt,
        ShapeKind::Donut,
    ] {
        let contours = shape_contours(kind, 120.0, 80.0);
        let mesh = stroke_contours(
            &contours,
            Color32::from_rgb(20, 40, 60),
            4.0,
            coursecraft_course::DashStyle::Solid,
            1.0,
        );
        assert!(
            !mesh.indices.is_empty(),
            "{kind:?} should produce an outline mesh"
        );
        let area = mesh_area(&mesh);
        assert!(area > 0.0, "{kind:?} outline has no area");
    }

    // A rectangle's band area is close to its perimeter times its width.
    let contours = shape_contours(ShapeKind::Rectangle, 100.0, 60.0);
    let mesh = stroke_contours(
        &contours,
        Color32::WHITE,
        4.0,
        coursecraft_course::DashStyle::Solid,
        0.0,
    );
    let expected = 2.0 * (100.0 + 60.0) * 4.0;
    let got = mesh_area(&mesh);
    assert!(
        (got - expected).abs() <= expected * 0.2,
        "rectangle band area {got} should be near {expected}"
    );
}

#[test]
fn dashes_and_dots_break_the_outline_into_pieces() {
    let contours = shape_contours(ShapeKind::Rectangle, 200.0, 120.0);
    let solid = stroke_contours(
        &contours,
        Color32::WHITE,
        4.0,
        coursecraft_course::DashStyle::Solid,
        0.0,
    );
    let dashed = stroke_contours(
        &contours,
        Color32::WHITE,
        4.0,
        coursecraft_course::DashStyle::Dash,
        0.0,
    );
    let dotted = stroke_contours(
        &contours,
        Color32::WHITE,
        4.0,
        coursecraft_course::DashStyle::Dot,
        0.0,
    );
    assert!(!solid.indices.is_empty());
    assert!(!dashed.indices.is_empty());
    assert!(!dotted.indices.is_empty());
    let solid_area = mesh_area(&solid);
    assert!(mesh_area(&dashed) < solid_area);
    assert!(mesh_area(&dotted) < solid_area);
}

#[test]
fn hostile_outline_widths_draw_nothing() {
    let contours = shape_contours(ShapeKind::Rectangle, 100.0, 60.0);
    for width in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        let mesh = stroke_contours(
            &contours,
            Color32::WHITE,
            width,
            coursecraft_course::DashStyle::Solid,
            1.0,
        );
        assert!(mesh.is_empty(), "width {width} should draw nothing");
    }
}

#[test]
fn no_fill_is_a_transparent_colour() {
    let object = Object {
        id: "shape".to_owned(),
        name: String::new(),
        kind: ObjectKind::Shape {
            shape: ShapeKind::Ellipse,
            fill: Color("#00000000".to_owned()),
            fill_style: None,
            outline: Some(coursecraft_course::Outline {
                color: Color("#2F6FEB".to_owned()),
                width: 3.0,
                dash: coursecraft_course::DashStyle::Dash,
            }),
            text: Some(coursecraft_course::ShapeText {
                text: "No fill".to_owned(),
                style: TextStyle::default(),
                runs: Vec::new(),
                paragraphs: Vec::new(),
                autofit: coursecraft_course::Autofit::None,
            }),
            adjust: Vec::new(),
        },
        frame: ModelRect {
            x: 10.0,
            y: 10.0,
            w: 200.0,
            h: 120.0,
        },
        timing: Timing::default(),
        states: Vec::new(),
        initial_state: "normal".to_owned(),
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
    let item = crate::compose_object(&object, "normal").expect("compose shape");
    let crate::Visual::Shape {
        fill,
        outline,
        text,
        ..
    } = item.visual
    else {
        panic!("expected a shape");
    };
    assert_eq!(fill.a(), 0);
    assert!(outline.is_some());
    assert!(text.is_some_and(|text| !text.runs.is_empty()));
}

#[test]
fn shape_text_rect_stays_inside_every_frame() {
    let sizes = [
        (10.0, 10.0),
        (40.0, 40.0),
        (200.0, 120.0),
        (1.0, 1000.0),
        (1000.0, 1.0),
    ];
    for info in SHAPE_CATALOG {
        for (w, h) in sizes {
            let (x, y, tw, th) = shape_text_rect(info.kind, w, h, &[]);
            assert!(x.is_finite() && y.is_finite() && tw.is_finite() && th.is_finite());
            assert!(x >= 0.0 && y >= 0.0, "{} at {w}x{h}", info.label);
            assert!(x + tw <= w + 0.001, "{} at {w}x{h}", info.label);
            assert!(y + th <= h + 0.001, "{} at {w}x{h}", info.label);
            if info.category == ShapeCategory::Lines {
                assert_eq!((tw, th), (0.0, 0.0), "{} takes no text", info.label);
            } else {
                assert!(tw > 0.0 && th > 0.0, "{} at {w}x{h}", info.label);
            }
        }
        for (w, h) in [(f32::NAN, 10.0), (10.0, f32::INFINITY), (0.0, 10.0)] {
            assert_eq!(shape_text_rect(info.kind, w, h, &[]), (0.0, 0.0, 0.0, 0.0));
        }
    }
}

#[test]
fn shape_text_rect_overrides_sit_on_the_body() {
    let (w, h) = (200.0, 120.0);

    let (x, y, tw, th) = shape_text_rect(ShapeKind::Ellipse, w, h, &[]);
    let (cx, cy) = (w * 0.5, h * 0.5);
    for (px, py) in [(x, y), (x + tw, y), (x, y + th), (x + tw, y + th)] {
        let ellipse = ((px - cx) / cx).powi(2) + ((py - cy) / cy).powi(2);
        assert!(ellipse <= 1.001, "ellipse corner outside: {ellipse}");
    }

    let (x, y, tw, th) = shape_text_rect(ShapeKind::Diamond, w, h, &[]);
    for (px, py) in [(x, y), (x + tw, y), (x, y + th), (x + tw, y + th)] {
        let diamond = (px - cx).abs() / cx + (py - cy).abs() / cy;
        assert!(diamond <= 1.001, "diamond corner outside: {diamond}");
    }

    let (x, _, tw, _) = shape_text_rect(ShapeKind::ArrowRight, w, h, &[]);
    assert!(
        x + tw < w * 0.8,
        "arrow text should sit on the shaft, not the head"
    );

    let (_, y, _, th) = shape_text_rect(ShapeKind::CalloutRectangle, w, h, &[]);
    assert!(
        y + th <= h * 0.72 + 0.001,
        "callout text sits in the bubble"
    );

    let (x, y, tw, th) = shape_text_rect(ShapeKind::Star5, w, h, &[]);
    assert!(
        x >= w * 0.2 && x + tw <= w * 0.8 && y >= h * 0.2 && y + th <= h * 0.8,
        "star text sits in the middle"
    );

    let (_, y, _, th) = shape_text_rect(ShapeKind::Cloud, w, h, &[]);
    assert!(y >= h * 0.2 && y + th <= h * 0.8);
    let (_, y, _, th) = shape_text_rect(ShapeKind::Heart, w, h, &[]);
    assert!(y >= h * 0.15 && y + th <= h * 0.75);
}

/// laying out every catalog kind with its label underneath.
#[test]
fn shape_gallery_course_is_written_for_review() {
    let theme = coursecraft_course::Theme::default();
    let text_color = theme.text.clone();
    let mut slides: Vec<Slide> = ShapeCategory::ALL
        .iter()
        .filter(|category| SHAPE_CATALOG.iter().any(|info| info.category == **category))
        .map(|category| category_slide(*category, text_color.clone()))
        .collect();
    slides.push(outline_text_slide());
    slides.push(adjust_slide(text_color.clone()));
    slides.push(draw_slide(text_color));
    assert!(slides.iter().any(|slide| slide.id == "shapes-outline-text"));
    assert!(slides.iter().any(|slide| slide.id == "shapes-adjust"));
    assert!(slides.iter().any(|slide| slide.id == "shapes-draw"));
    assert!(!slides.is_empty());
    let course = Course {
        schema_version: SCHEMA_VERSION,
        id: "shape-gallery".to_owned(),
        title: "Shape gallery".to_owned(),
        slide_size: coursecraft_course::Size::default(),
        variables: Vec::new(),
        scenes: vec![Scene {
            id: "shapes".to_owned(),
            title: "Shapes".to_owned(),
            slides,
        }],
        masters: Vec::new(),
        templates: Vec::new(),
        banks: Vec::new(),
        start_slide: None,
        quiz: None,
        player: coursecraft_course::PlayerSettings::default(),
        theme,
        language: None,
        strings: std::collections::BTreeMap::new(),
        fonts: Vec::new(),
        background_audio: None,
        glossary: Vec::new(),
        resources: Vec::new(),
        text_styles: Vec::new(),
    };
    let issues = course.validate();
    assert!(
        issues.iter().all(|issue| issue.severity != Severity::Error),
        "shape gallery course must validate: {issues:?}"
    );
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    let target = root.join("target");
    std::fs::create_dir_all(&target).expect("create target directory");
    let path = target.join("shape-gallery.json");
    std::fs::write(&path, course.to_json().expect("serialise gallery"))
        .expect("write shape gallery course");
    let slide_ids: Vec<String> = course.slides().map(|slide| slide.id.clone()).collect();
    println!(
        "wrote {} with slides: {}",
        path.display(),
        slide_ids.join(", ")
    );
}

/// Regression: a solid outline used to lose whole edges on diamonds, arrows and
/// callouts. Every edge of every shape must be covered by opaque band triangles.
fn non_default_adjust(kind: ShapeKind) -> Vec<f32> {
    let spec = adjust_spec(kind);
    spec.defaults
        .iter()
        .enumerate()
        .map(|(i, &default)| {
            let min = spec.min.get(i).copied().unwrap_or(default);
            let max = spec.max.get(i).copied().unwrap_or(default);
            if (max - default).abs() >= (default - min).abs() {
                default + (max - default) * 0.3
            } else {
                default - (default - min) * 0.3
            }
        })
        .collect()
}

#[test]
fn empty_adjust_draws_the_defaults_and_shape_contours() {
    for info in SHAPE_CATALOG {
        let defaults = adjust_defaults(info.kind).to_vec();
        let plain = shape_contours(info.kind, 200.0, 120.0);
        let empty = crate::shape_contours_adjusted(info.kind, 200.0, 120.0, &[]);
        let explicit = crate::shape_contours_adjusted(info.kind, 200.0, 120.0, &defaults);
        assert_eq!(plain, empty, "{} changed with an empty adjust", info.label);
        assert_eq!(
            plain, explicit,
            "{} changed with its explicit defaults",
            info.label
        );
    }
}

#[test]
fn every_adjustment_changes_the_contours() {
    for info in SHAPE_CATALOG {
        if !shape_is_adjustable(info.kind) {
            continue;
        }
        let plain = shape_contours(info.kind, 200.0, 120.0);
        let adjusted =
            crate::shape_contours_adjusted(info.kind, 200.0, 120.0, &non_default_adjust(info.kind));
        assert_ne!(plain, adjusted, "{} ignored its adjustment", info.label);
    }
}

#[test]
fn adjust_from_point_puts_the_handle_back_where_it_was_dragged() {
    let (w, h) = (200.0f32, 120.0f32);
    for info in SHAPE_CATALOG {
        if !shape_is_adjustable(info.kind) {
            continue;
        }
        let defaults = adjust_defaults(info.kind).to_vec();
        let handles = crate::adjust_handles(info.kind, w, h, &defaults);
        for (index, &handle) in handles.iter().enumerate() {
            let center = (w * 0.5, h * 0.5);
            let target = match info.kind {
                ShapeKind::Can | ShapeKind::Cube => (handle.0, handle.1 + 4.0),
                ShapeKind::Star4
                | ShapeKind::Star5
                | ShapeKind::Star6
                | ShapeKind::Star7
                | ShapeKind::Star8
                | ShapeKind::Star10
                | ShapeKind::Star12
                | ShapeKind::Star16 => (
                    center.0 + (handle.0 - center.0) * 1.04,
                    center.1 + (handle.1 - center.1) * 1.04,
                ),
                ShapeKind::BlockArc if index == 0 => {
                    let c = (w * 0.5, h * 0.55);
                    (c.0 + (handle.0 - c.0) * 1.04, c.1 + (handle.1 - c.1) * 1.04)
                }
                ShapeKind::BlockArc | ShapeKind::Pie | ShapeKind::Chord | ShapeKind::Arc => {
                    let (cx, cy, rx, ry) = if info.kind == ShapeKind::BlockArc {
                        (w * 0.5, h * 0.55, w * 0.5, h * 0.55)
                    } else {
                        (center.0, center.1, w * 0.5, h * 0.5)
                    };
                    let current = ((handle.1 - cy) / ry).atan2((handle.0 - cx) / rx);
                    let step = if info.kind == ShapeKind::BlockArc && index == 2 {
                        -0.05
                    } else {
                        0.05
                    };
                    let angle = current + step;
                    (cx + angle.cos() * rx, cy + angle.sin() * ry)
                }
                ShapeKind::ArrowUp | ShapeKind::ArrowDown | ShapeKind::ArrowUpDown
                    if index == 0 =>
                {
                    (handle.0, handle.1 + 4.0)
                }
                ShapeKind::ArrowRight | ShapeKind::ArrowLeft | ShapeKind::ArrowLeftRight
                    if index == 1 =>
                {
                    (handle.0, handle.1 + 4.0)
                }
                ShapeKind::ArrowUp | ShapeKind::ArrowDown | ShapeKind::ArrowUpDown => {
                    (handle.0 + 4.0, handle.1)
                }
                _ => (handle.0 + 4.0, handle.1),
            };
            let values = crate::adjust_from_point(info.kind, w, h, &defaults, index, target);
            let moved = crate::adjust_handles(info.kind, w, h, &values);
            let Some(&back) = moved.get(index) else {
                panic!("{} lost handle {index}", info.label);
            };
            let error = ((back.0 - target.0).powi(2) + (back.1 - target.1).powi(2)).sqrt();
            assert!(
                error <= 0.05,
                "{} handle {index} came back at {back:?}, not {target:?}",
                info.label
            );
        }
    }
}

#[test]
fn adjust_functions_are_total_for_random_points() {
    let mut state: u32 = 0x1234_5678;
    let mut next = || {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let unit = (state >> 8) as f32 / 16_777_215.0;
        unit * 800.0 - 300.0
    };
    for info in SHAPE_CATALOG {
        for _ in 0..40 {
            let x = next();
            let y = next();
            let values = [x, y, -x * 10.0, y * 0.25];
            let contours = crate::shape_contours_adjusted(info.kind, 200.0, 120.0, &values);
            for contour in &contours {
                for &(px, py) in contour {
                    assert!(px.is_finite() && py.is_finite(), "{}", info.label);
                }
            }
            for index in 0..5 {
                let handles = crate::adjust_handles(info.kind, 200.0, 120.0, &values);
                assert!(
                    handles
                        .iter()
                        .all(|&(px, py)| px.is_finite() && py.is_finite()),
                    "{}",
                    info.label
                );
                let next_values =
                    crate::adjust_from_point(info.kind, 200.0, 120.0, &values, index, (x, y));
                assert!(
                    next_values.iter().all(|value| value.is_finite()),
                    "{}",
                    info.label
                );
            }
        }
    }
}

#[test]
fn body_callouts_are_one_contour_with_the_tail_spliced_in() {
    for kind in [
        ShapeKind::CalloutRectangle,
        ShapeKind::CalloutRounded,
        ShapeKind::CalloutOval,
    ] {
        let plain = shape_contours(kind, 200.0, 120.0);
        assert_eq!(plain.len(), 1, "{kind:?} should be one contour");
        let dragged = crate::shape_contours_adjusted(kind, 200.0, 120.0, &[0.55, 1.6]);
        assert_eq!(dragged.len(), 1, "{kind:?} should stay one contour");
        assert!(
            dragged.iter().flatten().any(|&(_, y)| y > 120.0 + 0.5),
            "{kind:?} should let its tail leave the frame"
        );
        let text = shape_text_rect(kind, 200.0, 120.0, &[0.55, 1.6]);
        assert!(
            text.1 + text.3 <= 120.0 * 0.72 + 0.001,
            "{kind:?} text left the bubble"
        );
    }
}

#[test]
fn hostile_adjustments_stay_finite_and_never_panic() {
    let hostile = [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 1e30, -1e30, 0.0];
    for info in SHAPE_CATALOG {
        for window in hostile.windows(2) {
            let contours = crate::shape_contours_adjusted(info.kind, 200.0, 120.0, window);
            for contour in &contours {
                for &(x, y) in contour {
                    assert!(x.is_finite() && y.is_finite(), "{}", info.label);
                }
            }
            let handles = crate::adjust_handles(info.kind, 200.0, 120.0, window);
            for &(x, y) in &handles {
                assert!(x.is_finite() && y.is_finite(), "{}", info.label);
            }
            let values = crate::adjust_from_point(
                info.kind,
                200.0,
                120.0,
                window,
                0,
                (f32::NAN, f32::INFINITY),
            );
            for value in values {
                assert!(value.is_finite(), "{}", info.label);
            }
        }
        let extra = crate::adjust_from_point(
            info.kind,
            200.0,
            120.0,
            &hostile,
            usize::MAX,
            (f32::MAX, f32::MIN),
        );
        assert!(
            extra.iter().all(|value| value.is_finite()),
            "{}",
            info.label
        );
    }
}

/// Two contours that cross look like a seam when outlined, or punch a hole
/// with the even-odd fill. Disjoint pieces and nested holes are fine, so only
/// proper edge crossings are flagged.
#[test]
fn no_kind_has_crossing_contours() {
    fn cross(a: (f32, f32), b: (f32, f32)) -> f32 {
        a.0 * b.1 - a.1 * b.0
    }
    fn orientation(a: (f32, f32), b: (f32, f32), c: (f32, f32)) -> f32 {
        cross((b.0 - a.0, b.1 - a.1), (c.0 - a.0, c.1 - a.1))
    }
    fn edges_cross(a: (f32, f32), b: (f32, f32), c: (f32, f32), d: (f32, f32)) -> bool {
        let o1 = orientation(a, b, c);
        let o2 = orientation(a, b, d);
        let o3 = orientation(c, d, a);
        let o4 = orientation(c, d, b);
        o1 * o2 < -1e-6 && o3 * o4 < -1e-6
    }
    // Merging these would need a general polygon union (or would lose their
    // deliberate tapers and cut-outs), so they stay as overlapping pieces.
    const ALLOWED: &[&str] = &[
        "Left brace",          // two tapered strokes meeting at the middle
        "Right brace",         // mirror of the left brace
        "Right arrow callout", // box plus a separate arrow, like the other arrow callouts
        "Down arrow callout",
        "Left arrow callout",
        "Up arrow callout",
        "Left-right arrow callout",
        "Bent line callout", // a box plus the stroked bend
        "Not equal",         // the slash crosses the two bars by design
        "Trophy",            // cup, stem, base and ring handles overlap
        "Lock",              // shackle band overlaps the body
        "Key",               // ring and shaft overlap
        "Graduation cap",    // top and base overlap
        "Thumbs up",         // cuff, fist and finger slots overlap
        "Magnifier",         // lens and handle overlap
    ];
    let mut flagged: Vec<&str> = Vec::new();
    for info in SHAPE_CATALOG {
        if ALLOWED.contains(&info.label) {
            continue;
        }
        for (w, h) in [(200.0, 120.0), (120.0, 200.0), (90.0, 90.0)] {
            let contours = shape_contours(info.kind, w, h);
            let mut crosses = false;
            for i in 0..contours.len() {
                for j in (i + 1)..contours.len() {
                    let (Some(left), Some(right)) = (contours.get(i), contours.get(j)) else {
                        continue;
                    };
                    for (k, &a) in left.iter().enumerate() {
                        let Some(&b) = left.get((k + 1) % left.len()) else {
                            continue;
                        };
                        for (m, &c) in right.iter().enumerate() {
                            let Some(&d) = right.get((m + 1) % right.len()) else {
                                continue;
                            };
                            if edges_cross(a, b, c, d) {
                                crosses = true;
                            }
                        }
                    }
                }
            }
            if crosses {
                flagged.push(info.label);
                break;
            }
        }
    }
    assert!(flagged.is_empty(), "contours cross in: {flagged:?}");
}

#[test]
fn solid_outline_covers_every_edge_of_every_kind() {
    use coursecraft_course::{DashStyle, SHAPE_CATALOG};
    fn inside(p: (f32, f32), a: egui::Pos2, b: egui::Pos2, c: egui::Pos2) -> bool {
        let area = |u: (f32, f32), v: (f32, f32), w: (f32, f32)| {
            (v.0 - u.0) * (w.1 - u.1) - (v.1 - u.1) * (w.0 - u.0)
        };
        let (a, b, c) = ((a.x, a.y), (b.x, b.y), (c.x, c.y));
        let (d1, d2, d3) = (area(p, a, b), area(p, b, c), area(p, c, a));
        let eps = 0.01;
        !((d1 < -eps || d2 < -eps || d3 < -eps) && (d1 > eps || d2 > eps || d3 > eps))
    }
    let color = egui::Color32::from_rgb(40, 110, 240);
    for info in SHAPE_CATALOG {
        let contours = crate::shape_contours(info.kind, 150.0, 110.0);
        let mesh = crate::shape_fill::stroke_contours(&contours, color, 3.0, DashStyle::Solid, 1.0);
        let opaque: Vec<[egui::Pos2; 3]> = mesh
            .triangles()
            .filter_map(|[a, b, c]| {
                let v = |i: u32| mesh.vertices.get(i as usize);
                let (a, b, c) = (v(a)?, v(b)?, v(c)?);
                (a.color.a() > 0 || b.color.a() > 0 || c.color.a() > 0)
                    .then_some([a.pos, b.pos, c.pos])
            })
            .collect();
        for contour in &contours {
            for (i, &a) in contour.iter().enumerate() {
                let b = contour[(i + 1) % contour.len()];
                if (a.0 - b.0).abs() + (a.1 - b.1).abs() < 0.5 {
                    continue;
                }
                let mid = ((a.0 + b.0) * 0.5, (a.1 + b.1) * 0.5);
                assert!(
                    opaque.iter().any(|t| inside(mid, t[0], t[1], t[2])),
                    "{} lost the edge at {mid:?}",
                    info.label
                );
            }
        }
    }
}
