//! Automatic layout check: no clipped, overlapping or off-window text in the editor chrome.
//!
//! `layout_checker_*` tests the checker itself on a fixture. `layout_sweep_*` opens every
//! editor state and window at three window sizes in both themes and asserts the checker finds
//! nothing. Run the sweep alone with
//! `cargo test -p coursecraft-ui-egui layout_sweep -- --nocapture`.

use crate::snapshot::{SnapshotSelect, WINDOWS};
use crate::test_harness::{LayoutReport, TestHarness, capture_layers, check_layout, check_windows};
use crate::{BottomTab, InspectorTab};
use coursecraft_course::Course;
use coursecraft_render::ThemeMode;
use egui::epaint::{ClippedShape, Shape};
use egui::{Align2, Color32, FontId, Id, LayerId, Order, Rect, pos2, vec2};

const SHOWCASE: &str = include_str!("../../../examples/showcase/course.json");

const SIZES: [(f32, f32); 3] = [(1440.0, 900.0), (1024.0, 700.0), (1024.0, 600.0)];

fn showcase() -> Course {
    Course::from_json(SHOWCASE).unwrap_or_else(|e| panic!("showcase course: {e}"))
}

/// One editor state to check.
struct Case {
    label: String,
    select: SnapshotSelect,
    /// Text that proves the state is really on screen.
    shows: Option<&'static str>,
}

fn base_select() -> SnapshotSelect {
    SnapshotSelect {
        slide: Some("s-layers".to_owned()),
        object: Some("obj-layers-title".to_owned()),
        ..Default::default()
    }
}

fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for (name, tab) in [
        ("format", InspectorTab::Format),
        ("animate", InspectorTab::Animate),
        ("slide", InspectorTab::Slide),
        ("course", InspectorTab::Course),
    ] {
        cases.push(Case {
            label: format!("inspector-{name}"),
            select: SnapshotSelect {
                inspector_tab: Some(tab),
                ..base_select()
            },
            shows: None,
        });
    }
    for (name, tab) in [
        ("timeline", BottomTab::Timeline),
        ("triggers", BottomTab::Triggers),
        ("variables", BottomTab::Variables),
        ("question", BottomTab::Question),
    ] {
        let select = if tab == BottomTab::Question {
            SnapshotSelect {
                slide: Some("s-q-dd".to_owned()),
                object: None,
                bottom_tab: Some(tab),
                ..Default::default()
            }
        } else {
            SnapshotSelect {
                bottom_tab: Some(tab),
                ..base_select()
            }
        };
        cases.push(Case {
            label: format!("bottom-{name}"),
            select,
            shows: None,
        });
    }
    for slide in [
        "s-q-mc",
        "s-q-mr",
        "s-q-tf",
        "s-q-fi",
        "s-q-seq",
        "s-q-mat",
        "s-q-hs",
        "s-q-dd",
        "s-results",
    ] {
        cases.push(Case {
            label: format!("question-{slide}"),
            select: SnapshotSelect {
                slide: Some(slide.to_owned()),
                bottom_tab: Some(BottomTab::Question),
                ..Default::default()
            },
            shows: None,
        });
    }
    for spec in WINDOWS {
        cases.push(Case {
            label: format!("window-{}", spec.name),
            select: SnapshotSelect {
                slide: Some("s-layers".to_owned()),
                window: Some(spec.name.to_owned()),
                ..Default::default()
            },
            shows: Some(spec.shows),
        });
    }
    cases
}

fn run_case(case: &Case, size: (f32, f32), mode: ThemeMode) -> LayoutReport {
    let mut harness = TestHarness::new_with_theme(showcase(), size, mode);
    case.select.apply(&mut harness.editor);
    harness.layout_report(4)
}

/// Run the whole sweep and return `(findings, unopened)`.
fn sweep() -> (Vec<String>, Vec<String>) {
    let mut findings = Vec::new();
    let mut unopened = Vec::new();
    for case in cases() {
        for size in SIZES {
            for (mode, theme) in [(ThemeMode::Dark, "dark"), (ThemeMode::Light, "light")] {
                let report = run_case(&case, size, mode);
                let tag = format!("{} {}x{} {theme}", case.label, size.0, size.1);
                if let Some(shows) = case.shows
                    && !report.painted(shows)
                {
                    unopened.push(format!("{tag}: \"{shows}\" was not painted"));
                }
                for f in report.findings {
                    findings.push(format!("{tag}: {f}"));
                }
            }
        }
    }
    (findings, unopened)
}

/// The checker reports clipped, overlapping and off-window text, and leaves tidy text alone.
#[test]
fn layout_checker_reports_deliberate_problems() {
    let ctx = egui::Context::default();
    coursecraft_render::apply_style(&ctx, ThemeMode::Dark);
    let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 300.0));
    let raw = egui::RawInput {
        screen_rect: Some(screen),
        ..Default::default()
    };
    let mut layers = Vec::new();
    let output = ctx.run_ui(raw, |ui| {
        let font = FontId::proportional(14.0);
        let white = Color32::WHITE;
        let painter = ui.painter();
        // Two labels on top of each other.
        painter.text(
            pos2(20.0, 20.0),
            Align2::LEFT_TOP,
            "Navigation buttons",
            font.clone(),
            white,
        );
        painter.text(
            pos2(40.0, 22.0),
            Align2::LEFT_TOP,
            "Control value",
            font.clone(),
            white,
        );
        // A label cut off by a narrow clip.
        let narrow = ui.painter_at(Rect::from_min_size(pos2(20.0, 80.0), vec2(40.0, 20.0)));
        narrow.text(
            pos2(20.0, 80.0),
            Align2::LEFT_TOP,
            "Clipped label here",
            font.clone(),
            white,
        );
        // A label hanging past the window edge.
        painter.text(
            pos2(380.0, 150.0),
            Align2::LEFT_TOP,
            "Off the edge",
            font.clone(),
            white,
        );
        // A tidy label that must not be reported.
        painter.text(pos2(20.0, 220.0), Align2::LEFT_TOP, "All fine", font, white);
        layers = capture_layers(ui.ctx());
    });
    output.drop_without_applying_deltas();

    let findings = check_layout(&layers, screen, &[], &[]).findings;
    let joined = findings.join("\n");
    assert!(
        findings.iter().any(|f| f.starts_with("overlap:")
            && f.contains("Navigation")
            && f.contains("Control")),
        "overlap missing in:\n{joined}"
    );
    assert!(
        findings
            .iter()
            .any(|f| f.starts_with("clipped:") && f.contains("Clipped label")),
        "clipped missing in:\n{joined}"
    );
    assert!(
        findings
            .iter()
            .any(|f| f.starts_with("off-window:") && f.contains("Off the edge")),
        "off-window missing in:\n{joined}"
    );
    assert!(
        !findings.iter().any(|f| f.contains("All fine")),
        "tidy text reported in:\n{joined}"
    );

    // A registered content region silences text inside it.
    let region = Rect::from_min_size(pos2(0.0, 0.0), vec2(200.0, 60.0));
    let layer = layers
        .iter()
        .map(|(id, _)| *id)
        .find(|id| id.order == egui::Order::Background)
        .unwrap_or_else(egui::LayerId::background);
    let quiet = check_layout(&layers, screen, &[(layer, region)], &[]).findings;
    assert!(
        !quiet.iter().any(|f| f.starts_with("overlap:")),
        "overlap not excluded by region:\n{}",
        quiet.join("\n")
    );
}

/// Every editor state and window, three sizes, both themes: nothing clipped, overlapping or
/// off-window, and every window really opened.
#[test]
fn layout_sweep_editor_states_and_windows() {
    let (findings, unopened) = sweep();
    println!("layout sweep: {} findings", findings.len());
    for f in &findings {
        println!("FINDING {f}");
    }
    println!("layout sweep: {} windows not painted", unopened.len());
    for u in &unopened {
        println!("UNOPENED {u}");
    }
    assert!(
        findings.is_empty() && unopened.is_empty(),
        "{} layout findings, {} unopened windows (see output)",
        findings.len(),
        unopened.len()
    );
}

/// A window must stay inside the screen and below the menu bar, and a button cut off at the
/// bottom edge of its window is a finding.
#[test]
fn layout_checker_reports_bad_windows_and_bottom_clips() {
    let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 300.0));
    let menu_bar = Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 26.0));
    let layer = LayerId::new(Order::Middle, Id::unique("test_window"));
    let windows = [
        (
            layer,
            Rect::from_min_size(pos2(20.0, 4.0), vec2(200.0, 100.0)),
        ),
        (
            layer,
            Rect::from_min_size(pos2(300.0, 100.0), vec2(200.0, 100.0)),
        ),
        (
            layer,
            Rect::from_min_size(pos2(20.0, 200.0), vec2(100.0, 150.0)),
        ),
        (
            layer,
            Rect::from_min_size(pos2(100.0, 60.0), vec2(200.0, 120.0)),
        ),
    ];
    let findings = check_windows(&windows, screen, menu_bar);
    assert_eq!(
        findings
            .iter()
            .filter(|f| f.starts_with("window-over-menu-bar:"))
            .count(),
        1,
        "menu bar overlap in:\n{}",
        findings.join("\n")
    );
    assert_eq!(
        findings
            .iter()
            .filter(|f| f.starts_with("window-off-screen:"))
            .count(),
        2,
        "off-screen in:\n{}",
        findings.join("\n")
    );

    // A button cut off at the bottom of its window.
    let win = Rect::from_min_size(pos2(50.0, 50.0), vec2(200.0, 100.0));
    let button = |rect: Rect, clip: Rect| ClippedShape {
        clip_rect: clip,
        shape: Shape::rect_filled(rect, 4.0, Color32::from_gray(80)),
        transform_after_tessellation: egui::emath::TSTransform::IDENTITY,
    };
    let layers = vec![(
        layer,
        vec![
            // A button reaching past the window's bottom edge.
            button(
                Rect::from_min_size(pos2(60.0, 130.0), vec2(80.0, 30.0)),
                win,
            ),
            // A button cut by a scroll viewport that ends well above the window's edge.
            button(
                Rect::from_min_size(pos2(60.0, 90.0), vec2(80.0, 30.0)),
                Rect::from_min_size(pos2(50.0, 50.0), vec2(200.0, 60.0)),
            ),
        ],
    )];
    let findings = check_layout(&layers, screen, &[], &[(layer, win)]).findings;
    let bottom: Vec<_> = findings
        .iter()
        .filter(|f| f.contains("bottom of its window"))
        .collect();
    assert_eq!(bottom.len(), 1, "bottom clips in:\n{}", findings.join("\n"));
}

/// The window checks only mean something if the checker really sees the dialogs: every
/// dialog-style window opens as a window below the menu bar, inside the screen.
#[test]
fn layout_sweep_sees_dialogs_as_windows() {
    for spec in WINDOWS {
        if matches!(
            spec.name,
            "welcome" | "shortcuts" | "preview" | "coach-1" | "coach-2" | "coach-3"
        ) {
            continue;
        }
        let mut harness = TestHarness::new_with_theme(showcase(), (1024.0, 700.0), ThemeMode::Dark);
        SnapshotSelect {
            slide: Some("s-layers".to_owned()),
            window: Some(spec.name.to_owned()),
            ..Default::default()
        }
        .apply(&mut harness.editor);
        let report = harness.layout_report(4);
        let dialog = report
            .windows
            .iter()
            .find(|(_, r)| r.width() > 200.0 && r.height() > 60.0);
        assert!(
            dialog.is_some(),
            "{}: no dialog window seen among {:?}",
            spec.name,
            report.windows
        );
        let menu_bottom = crate::TOP_BAR_HEIGHT;
        if let Some((_, r)) = dialog {
            assert!(
                r.min.y >= menu_bottom,
                "{}: dialog {r:?} starts above the menu bar bottom {menu_bottom}",
                spec.name
            );
        }
    }
}
