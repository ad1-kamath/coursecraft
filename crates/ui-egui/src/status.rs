//! Status bar: course title, validation summary, and error notifications.

use crate::Editor;
use crate::dialog::Dialog;
use coursecraft_course::Course;
use egui::{RichText, Ui};

pub const STATUS_BAR_HEIGHT: f32 = 26.0;

/// Parse an issue path and navigate the selection to the referenced slide, layer, and object if possible.
/// Returns true if a slide was selected.
pub fn navigate_to_issue_path(
    course: &Course,
    selection: &mut crate::Selection,
    path: &str,
) -> bool {
    let segments: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    if segments.is_empty() {
        return false;
    }

    // Pattern 1: scenes/{si}/slides/{li}/...
    if segments.first() == Some(&"scenes") {
        let Some(si_str) = segments.get(1) else {
            return false;
        };
        let Ok(si) = si_str.parse::<usize>() else {
            return false;
        };
        let Some(scene) = course.scenes.get(si) else {
            return false;
        };

        if segments.get(2) == Some(&"slides") {
            let Some(li_str) = segments.get(3) else {
                return false;
            };
            let Ok(li) = li_str.parse::<usize>() else {
                return false;
            };
            let Some(slide) = scene.slides.get(li) else {
                return false;
            };

            select_slide_subpath(slide, selection, &segments[4..]);
            return true;
        } else if let Some(first_slide) = scene.slides.first() {
            selection.slide = first_slide.id.clone();
            selection.layer = None;
            selection.clear_objects();
            return true;
        }
    }

    // Pattern 2: banks/{bi}/slides/{si}/...
    if segments.first() == Some(&"banks") {
        let Some(bi_str) = segments.get(1) else {
            return false;
        };
        let Ok(bi) = bi_str.parse::<usize>() else {
            return false;
        };
        let Some(bank) = course.banks.get(bi) else {
            return false;
        };
        if segments.get(2) == Some(&"slides") {
            let Some(si_str) = segments.get(3) else {
                return false;
            };
            let Ok(si) = si_str.parse::<usize>() else {
                return false;
            };
            let Some(slide) = bank.slides.get(si) else {
                return false;
            };
            select_slide_subpath(slide, selection, &segments[4..]);
            return true;
        } else if let Some(first_slide) = bank.slides.first() {
            selection.slide = first_slide.id.clone();
            selection.layer = None;
            selection.clear_objects();
            return true;
        }
    }

    // Pattern 2: masters/{mi}/objects/{oi}
    if segments.first() == Some(&"masters") {
        let Some(mi_str) = segments.get(1) else {
            return false;
        };
        let Ok(mi) = mi_str.parse::<usize>() else {
            return false;
        };
        let Some(master) = course.masters.get(mi) else {
            return false;
        };
        selection.slide = master.id.clone();
        selection.layer = None;
        selection.clear_objects();
        if segments.get(2) == Some(&"objects")
            && let Some(oi_str) = segments.get(3)
            && let Ok(oi) = oi_str.parse::<usize>()
            && let Some(object) = master.objects.get(oi)
        {
            selection.object = Some(object.id.clone());
        }
        return true;
    }

    // Pattern 3: start_slide
    if segments.first() == Some(&"start_slide")
        && let Some(ref start) = course.start_slide
        && course.play_slide(start).is_some()
    {
        selection.slide = start.clone();
        selection.layer = None;
        selection.clear_objects();
        return true;
    }

    false
}

fn select_slide_subpath(
    slide: &coursecraft_course::Slide,
    selection: &mut crate::Selection,
    rest: &[&str],
) {
    selection.slide = slide.id.clone();
    selection.layer = None;
    selection.clear_objects();

    if rest.first() == Some(&"layers") {
        if let Some(lyi_str) = rest.get(1)
            && let Ok(lyi) = lyi_str.parse::<usize>()
            && let Some(layer) = slide.layers.get(lyi)
        {
            selection.layer = Some(layer.id.clone());
            if rest.get(2) == Some(&"objects")
                && let Some(oi_str) = rest.get(3)
                && let Ok(oi) = oi_str.parse::<usize>()
                && let Some(obj) = layer.objects.get(oi)
            {
                selection.object = Some(obj.id.clone());
            }
        }
    } else if rest.first() == Some(&"objects")
        && let Some(oi_str) = rest.get(1)
        && let Ok(oi) = oi_str.parse::<usize>()
        && let Some(obj) = slide.objects.get(oi)
    {
        selection.object = Some(obj.id.clone());
    }
}

/// Extract the state name from a `media_references` place string (`… ▸ State "<name>"`).
pub(crate) fn state_name_from_place(place: &str) -> Option<String> {
    let (_, rest) = place.rsplit_once("▸ State \"")?;
    let name = rest.split('"').next()?;
    (!name.is_empty()).then(|| name.to_owned())
}

fn state_name_from_issue_path(
    course: &Course,
    slide_id: &str,
    object_id: Option<&str>,
    path: &str,
) -> Option<String> {
    if let Some(name) = state_name_from_place(path) {
        return Some(name);
    }
    let segments: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    let idx = segments.windows(2).find_map(|pair| {
        (pair[0] == "states")
            .then(|| pair[1].parse::<usize>().ok())
            .flatten()
    })?;
    let slide = course.slide(slide_id)?;
    let (obj, _) = slide.object(object_id?)?;
    obj.states.get(idx).map(|state| state.name.clone())
}

fn finish_slide_navigation(editor: &mut Editor, previous: String) {
    editor.ui.story_view = false;
    if editor
        .session
        .course
        .master(&editor.selection.slide)
        .is_some()
    {
        if !editor.ui.master_view {
            editor.ui.master_view_return_slide = Some(previous);
            editor.ui.master_view = true;
            crate::dock::request_bottom_tab(editor, crate::BottomTab::Timeline);
        }
    } else {
        editor.ui.master_view = false;
        editor.ui.master_view_return_slide = None;
    }
}

/// Select the slide and object named by an accessibility finding.
///
/// Returns true if a slide was selected.
pub fn navigate_to_audit_finding(
    course: &Course,
    selection: &mut crate::Selection,
    slide: Option<&str>,
    object: Option<&str>,
) -> bool {
    let Some(slide_id) = slide else {
        return false;
    };
    let Some(slide) = course.slide(slide_id) else {
        return false;
    };
    selection.slide = slide_id.to_owned();
    selection.layer = None;
    selection.clear_objects();
    if let Some(object_id) = object
        && let Some((found, layer)) = slide.object(object_id)
    {
        selection.object = Some(found.id.clone());
        selection.layer = layer.map(str::to_owned);
    }
    true
}

/// Render the status bar at the bottom of the editor.
/// What the status bar says while a painter is armed: it names what is copied (`N109`).
pub(crate) fn painter_hint(kind: crate::format_painter::PainterKind, sticky: bool) -> &'static str {
    use crate::format_painter::PainterKind;
    match (kind, sticky) {
        (PainterKind::Format, true) => "Click objects to copy the format onto them (Esc to cancel)",
        (PainterKind::Format, false) => {
            "Click an object to copy the format onto it (Esc to cancel)"
        }
        (PainterKind::Animation, true) => {
            "Click objects to copy the animations onto them (Esc to cancel)"
        }
        (PainterKind::Animation, false) => {
            "Click an object to copy the animations onto it (Esc to cancel)"
        }
    }
}

pub fn show_status_bar(editor: &mut Editor, ui: &mut Ui) {
    let p = coursecraft_render::palette(ui.ctx());
    ui.horizontal(|ui| {
        // Placement mode prompt
        if let Some(ref tool) = editor.ui.placement_tool {
            ui.label(
                RichText::new(format!(
                    "Click for the default size, or drag to draw {}",
                    tool.name()
                ))
                .strong()
                .color(p.accent_text),
            );
            ui.separator();
        }

        // Course / file title info (truncate long filenames so validation & error chips stay visible)
        let title_text = if let Some((ref name, dirty)) = editor.title_info {
            let dirty_marker = if dirty { " •" } else { "" };
            format!("{name}{dirty_marker}")
        } else {
            editor.session.course.title.clone()
        };
        let max_title_w = (ui.available_width() * 0.35).clamp(60.0, 260.0);
        ui.scope(|ui| {
            ui.set_max_width(max_title_w);
            ui.add(
                egui::Label::new(coursecraft_render::small(&title_text).color(p.text_secondary))
                    .truncate(),
            )
            .on_hover_text(&title_text);
        });
        ui.separator();

        // Validation summary chip
        let errs = editor.ui.validation_errors;
        let warns = editor.ui.validation_warnings;
        let err_label = if errs == 1 { "error" } else { "errors" };
        let warn_label = if warns == 1 { "warning" } else { "warnings" };
        let summary_text = format!("{errs} {err_label}, {warns} {warn_label}");

        let dot_color = if errs > 0 {
            p.danger
        } else if warns > 0 {
            p.warning
        } else {
            p.success
        };

        let font_id = egui::TextStyle::Small.resolve(ui.style());
        let galley = ui
            .painter()
            .layout_no_wrap(summary_text.clone(), font_id, p.text);
        let desired_width = 8.0 + 6.0 + 6.0 + galley.size().x + 8.0;
        let desired_height = 24.0;
        let (rect, resp) = ui.allocate_exact_size(
            egui::vec2(desired_width, desired_height),
            egui::Sense::click(),
        );
        let bg = if resp.is_pointer_button_down_on() {
            p.pressed
        } else if resp.hovered() {
            p.hover
        } else {
            p.input
        };
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(12), bg);
        let dot_center = egui::pos2(rect.min.x + 8.0 + 3.0, rect.center().y);
        ui.painter().circle_filled(dot_center, 3.0, dot_color);
        let text_pos = egui::pos2(
            rect.min.x + 8.0 + 6.0 + 6.0,
            rect.center().y - galley.size().y / 2.0,
        );
        ui.painter().galley(text_pos, galley, p.text);

        if resp
            .on_hover_text("Click to view validation issues")
            .clicked()
        {
            editor.ui.show_validation_window = !editor.ui.show_validation_window;
        }

        // Last error (shown in red for ~5 seconds)
        if let Some((ref err_msg, err_time)) = editor.ui.last_error
            && editor.ui.current_time - err_time <= 5.0
        {
            ui.separator();
            if ui
                .button(coursecraft_render::small(err_msg).color(p.danger))
                .on_hover_text("Click to view error history")
                .clicked()
            {
                editor.ui.show_error_history_window = !editor.ui.show_error_history_window;
            }
        }
        // Neutral status message (paste results, skipped triggers)
        if let Some((ref message, shown_at)) = editor.ui.status_message
            && editor.ui.current_time - shown_at <= 5.0
        {
            ui.separator();
            ui.label(coursecraft_render::small(message).color(p.text_secondary));
        }
        if let Some(painter) = editor.ui.format_painter.as_ref() {
            ui.separator();
            let hint = tl!(painter_hint(painter.kind, painter.sticky));
            ui.label(coursecraft_render::small(hint).color(p.accent));
        }
        // MP4 export progress or the finished report
        crate::export_dialog::show_export_status(editor, ui);
        // Automation chip when running
        if let Some(port) = editor.automation_port {
            ui.separator();
            ui.label(coursecraft_render::small("Automation on").color(p.success))
                .on_hover_text(format!("Port {port}"));
        }

        // Slide size pill (right-aligned, hidden on ultra-narrow bars per RISK-05)
        let slide_size = editor.session.course.slide_size;
        let size_text = format!("{} × {} px", slide_size.w, slide_size.h);
        let ratio = crate::slide_size_dialog::format_aspect_ratio(slide_size.w, slide_size.h);
        let size_font = egui::TextStyle::Small.resolve(ui.style());
        let size_galley = ui
            .painter()
            .layout_no_wrap(size_text, size_font, p.text_secondary);
        let pill_w = 10.0 + size_galley.size().x + 10.0;
        let pill_h = 24.0;
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.available_width() >= pill_w.max(96.0) {
                let (rect, resp) =
                    ui.allocate_exact_size(egui::vec2(pill_w, pill_h), egui::Sense::click());
                let bg = if resp.is_pointer_button_down_on() {
                    p.pressed
                } else if resp.hovered() {
                    p.hover
                } else {
                    p.input
                };
                ui.painter()
                    .rect_filled(rect, egui::CornerRadius::same(12), bg);
                let text_pos = egui::pos2(
                    rect.min.x + 10.0,
                    rect.center().y - size_galley.size().y / 2.0,
                );
                ui.painter().galley(text_pos, size_galley, p.text_secondary);
                if resp
                    .on_hover_text(format!("Slide size ({ratio}) — click to change"))
                    .clicked()
                {
                    editor.open_slide_size_dialog();
                }
            }
        });
    });

    // Validation issues popup window
    if editor.ui.show_validation_window {
        if editor.ui.audit_dirty {
            editor.refresh_audit_now();
        }
        let mut path_to_navigate = None;
        let mut audit_to_navigate = None;
        let mut suggest_alt = false;
        let has_missing_alt = editor
            .ui
            .audit_findings
            .iter()
            .any(|finding| finding.check == "alt-text");
        Dialog::new("Check course")
            .open(&mut editor.ui.show_validation_window)
            .default_size([500.0, 300.0])
            .resizable(true)
            .show(ui.ctx(), |ui| {
                if editor.ui.validation_issues.is_empty() {
                    ui.label("No validation issues found.");
                } else {
                    for issue in &editor.ui.validation_issues {
                        let color = if issue.severity == "error" {
                            p.danger
                        } else {
                            p.warning
                        };
                        let label = format!("{} — {}", issue.path, issue.message);
                        if ui
                            .add(egui::Button::new(RichText::new(label).color(color)).wrap())
                            .on_hover_text("Click to select referenced slide/object")
                            .clicked()
                        {
                            path_to_navigate = Some(issue.path.clone());
                        }
                    }
                }

                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Accessibility").strong());
                    if has_missing_alt && ui.button("Suggest alt text…").clicked() {
                        suggest_alt = true;
                    }
                });
                if editor.ui.audit_findings.is_empty() {
                    ui.label(
                        coursecraft_render::small("No automated accessibility findings.")
                            .color(p.text_secondary),
                    );
                } else {
                    for finding in &editor.ui.audit_findings {
                        let color = match finding.severity.as_str() {
                            "error" => p.danger,
                            "warning" => p.warning,
                            _ => p.text_secondary,
                        };
                        let object = finding
                            .object
                            .as_deref()
                            .map_or_else(String::new, |id| format!(" {id}"));
                        let label = format!(
                            "{} [{}]{} — {}",
                            finding.severity, finding.criterion, object, finding.message
                        );
                        if ui
                            .add(egui::Button::new(RichText::new(label).color(color)).wrap())
                            .on_hover_text("Click to select the referenced slide and object")
                            .clicked()
                        {
                            audit_to_navigate =
                                Some((finding.slide.clone(), finding.object.clone()));
                        }
                        ui.label(
                            coursecraft_render::small(format!("Fix: {}", finding.fix))
                                .color(p.text_secondary),
                        );
                    }
                }

                ui.separator();
                ui.label(
                    coursecraft_render::small("Automated checks only. These still need a person:")
                        .color(p.text_secondary),
                );
                for check in &editor.ui.audit_manual_checks {
                    ui.label(
                        coursecraft_render::small(format!("• {check}")).color(p.text_secondary),
                    );
                }
            });

        if let Some(path) = path_to_navigate {
            let previous = editor.selection.slide.clone();
            let navigated =
                navigate_to_issue_path(&editor.session.course, &mut editor.selection, &path)
                    || coursecraft_course::media_references(&editor.session.course)
                        .into_iter()
                        .find(|r| r.place == path)
                        .is_some_and(|r| {
                            navigate_to_audit_finding(
                                &editor.session.course,
                                &mut editor.selection,
                                r.slide.as_deref(),
                                r.object.as_deref(),
                            )
                        });
            if navigated {
                finish_slide_navigation(editor, previous);
                if path.ends_with("/revisit") {
                    crate::dock::request_inspector_tab(
                        editor,
                        crate::properties::InspectorTab::Slide,
                    );
                } else if !editor.ui.master_view
                    && let Some(state_name) = state_name_from_issue_path(
                        &editor.session.course,
                        &editor.selection.slide,
                        editor.selection.object.as_deref(),
                        &path,
                    )
                {
                    editor.preview_navigated_state(state_name);
                }
            }
        }
        if let Some((slide, object)) = audit_to_navigate {
            if slide.is_none() {
                editor.ui.story_view = false;
                editor.ui.master_view = false;
                editor.ui.master_view_return_slide = None;
                crate::dock::request_inspector_tab(editor, crate::properties::InspectorTab::Course);
            } else {
                let previous = editor.selection.slide.clone();
                if navigate_to_audit_finding(
                    &editor.session.course,
                    &mut editor.selection,
                    slide.as_deref(),
                    object.as_deref(),
                ) {
                    finish_slide_navigation(editor, previous);
                }
            }
        }
        if suggest_alt {
            editor.open_alt_text_dialog();
        }
    }

    // Error history popup window
    if editor.ui.show_error_history_window {
        Dialog::new("Error history")
            .open(&mut editor.ui.show_error_history_window)
            .default_size([500.0, 300.0])
            .resizable(true)
            .show(ui.ctx(), |ui| {
                if editor.ui.error_history.is_empty() {
                    ui.label("No errors recorded.");
                } else {
                    for (idx, err) in editor.ui.error_history.iter().enumerate() {
                        ui.label(format!("{}. {}", idx + 1, err));
                    }
                }
            });
    }

    // PowerPoint Import Report popup window
    if let Some(ref report) = editor.ui.pptx_import_report.clone() {
        let mut open = true;
        let mut dismissed = false;
        Dialog::new("PowerPoint import report")
            .open(&mut open)
            .default_size([450.0, 220.0])
            .resizable(true)
            .show_with_footer(ui.ctx(), |dialog| {
                dialog.body(|ui| {
                    ui.label(report);
                });
                dialog.footer(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Dismiss").clicked() {
                            dismissed = true;
                        }
                    });
                });
            });
        if !open || dismissed {
            editor.ui.pptx_import_report = None;
        }
    }
}
