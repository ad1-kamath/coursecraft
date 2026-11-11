//! Question bottom panel: kind, prompt, per-kind answers, attempts/points, and layer targets.

use crate::widgets::{icon_button_danger, icon_button_enabled};
use coursecraft_course::{
    Action, Choice, Condition, DragItem, Event, HotspotRegion, HotspotShape, MatchPair, ObjectKind,
    Question, QuestionKind, Scoring, Slide, Trigger,
};
use coursecraft_render::Icon;
use egui::{Color32, DragValue, RichText, ScrollArea, TextEdit, Ui};
use serde_json::Value as Json;

/// Human-readable name for a QuestionKind.
#[must_use]
pub fn question_kind_name(kind: QuestionKind) -> &'static str {
    match kind {
        QuestionKind::MultipleChoice => "Multiple Choice",
        QuestionKind::MultipleResponse => "Multiple Response",
        QuestionKind::TrueFalse => "True / False",
        QuestionKind::FillIn => "Fill in",
        QuestionKind::Sequence => "Sequence",
        QuestionKind::Matching => "Matching",
        QuestionKind::Hotspot => "Hotspot",
        QuestionKind::DragDrop => "Drag and drop",
        QuestionKind::Likert => "Likert scale",
        QuestionKind::ShortAnswer => "Short answer",
        QuestionKind::Essay => "Essay",
        QuestionKind::Freeform => "Custom question (graded by a condition)",
    }
}

pub const ALL_QUESTION_KINDS: [QuestionKind; 12] = [
    QuestionKind::MultipleChoice,
    QuestionKind::MultipleResponse,
    QuestionKind::TrueFalse,
    QuestionKind::FillIn,
    QuestionKind::Freeform,
    QuestionKind::Sequence,
    QuestionKind::Matching,
    QuestionKind::Hotspot,
    QuestionKind::DragDrop,
    QuestionKind::Likert,
    QuestionKind::ShortAnswer,
    QuestionKind::Essay,
];

/// Scoring options available for a question kind.
#[must_use]
pub(crate) fn scoring_options(kind: QuestionKind) -> &'static [Scoring] {
    match kind {
        QuestionKind::MultipleChoice | QuestionKind::TrueFalse => {
            &[Scoring::AllOrNothing, Scoring::PerAnswer]
        }
        QuestionKind::MultipleResponse => {
            &[Scoring::AllOrNothing, Scoring::PerAnswer, Scoring::Partial]
        }
        QuestionKind::Sequence
        | QuestionKind::Matching
        | QuestionKind::Hotspot
        | QuestionKind::DragDrop => &[Scoring::AllOrNothing, Scoring::Partial],
        QuestionKind::FillIn
        | QuestionKind::Likert
        | QuestionKind::ShortAnswer
        | QuestionKind::Essay
        | QuestionKind::Freeform => &[],
    }
}

/// Edit state and local buffers for the Question panel.
#[derive(Debug, Clone, PartialEq)]
pub struct QuestionState {
    pub last_synced_revision: u64,
    pub last_synced_slide: String,

    pub kind: QuestionKind,
    pub prompt: String,
    pub choices: Vec<Choice>,
    pub points: u32,
    pub scoring: Scoring,
    pub attempts: u32,
    pub shuffle: bool,
    pub correct_layer: Option<String>,
    pub incorrect_layer: Option<String>,
    pub accepted: Vec<String>,
    pub case_sensitive: bool,
    pub pairs: Vec<MatchPair>,
    pub hotspot_image: Option<String>,
    pub regions: Vec<HotspotRegion>,
    pub markers: u32,
    pub drag_items: Vec<DragItem>,
    pub drop_targets: Vec<String>,
    pub snap_to_center: bool,
    pub return_if_wrong: bool,
    pub one_per_target: bool,
    pub correct_when: Option<Condition>,
    pub selected_region: Option<String>,

    /// Prompt field rect for headless tests.
    pub prompt_edit_rect: Option<egui::Rect>,
    /// Vertical scroll offset of the panel's scroll area (tracked for tests).
    pub scroll_offset_y: f32,
    /// Content height beyond the visible area of the scroll area (tracked for tests).
    pub scroll_overflow_y: f32,
    /// Rect of the scroll area's viewport in the last frame (tracked for tests).
    pub scroll_inner_rect: egui::Rect,
}

impl Default for QuestionState {
    fn default() -> Self {
        Self {
            last_synced_revision: u64::MAX,
            last_synced_slide: String::new(),
            kind: QuestionKind::MultipleChoice,
            prompt: String::new(),
            choices: Vec::new(),
            points: 10,
            scoring: Scoring::default(),
            attempts: 1,
            shuffle: false,
            correct_layer: None,
            incorrect_layer: None,
            accepted: Vec::new(),
            case_sensitive: false,
            pairs: Vec::new(),
            hotspot_image: None,
            regions: Vec::new(),
            markers: 1,
            drag_items: Vec::new(),
            drop_targets: Vec::new(),
            snap_to_center: true,
            return_if_wrong: false,
            one_per_target: true,
            correct_when: None,
            selected_region: None,
            prompt_edit_rect: None,
            scroll_offset_y: 0.0,
            scroll_overflow_y: 0.0,
            scroll_inner_rect: egui::Rect::NOTHING,
        }
    }
}

/// Generate the next available single-letter choice id (`a`, `b`, `c`, …).
#[must_use]
pub fn next_choice_id(choices: &[Choice]) -> String {
    for ch in 'a'..='z' {
        let s = ch.to_string();
        if !choices.iter().any(|c| c.id == s) {
            return s;
        }
    }
    format!("choice_{}", choices.len() + 1)
}

impl QuestionState {
    /// Resync buffers from the selected slide's question.
    pub fn sync(&mut self, slide: &Slide, revision: u64) {
        if self.last_synced_revision == revision && self.last_synced_slide == slide.id {
            return;
        }
        self.last_synced_revision = revision;
        self.last_synced_slide = slide.id.clone();

        if let Some(ref q) = slide.question {
            self.kind = q.kind;
            self.prompt = q.prompt.clone();
            self.choices = q.choices.clone();
            self.points = q.points;
            self.scoring = q.scoring;
            self.attempts = q.attempts;
            self.shuffle = q.shuffle;
            self.correct_layer = q.correct_layer.clone();
            self.incorrect_layer = q.incorrect_layer.clone();
            self.accepted = q.accepted.clone();
            self.case_sensitive = q.case_sensitive;
            self.pairs = q.pairs.clone();
            self.hotspot_image = q.hotspot_image.clone();
            self.regions = q.regions.clone();
            self.markers = q.markers;
            self.drag_items = q.drag_items.clone();
            self.drop_targets = q.drop_targets.clone();
            self.snap_to_center = q.snap_to_center;
            self.return_if_wrong = q.return_if_wrong;
            self.one_per_target = q.one_per_target;
            self.correct_when = q.correct_when.clone();
            if let Some(ref r_id) = self.selected_region {
                if !q.regions.iter().any(|r| &r.id == r_id) {
                    self.selected_region = q.regions.first().map(|r| r.id.clone());
                }
            } else {
                self.selected_region = q.regions.first().map(|r| r.id.clone());
            }
        } else {
            self.kind = QuestionKind::MultipleChoice;
            self.prompt.clear();
            self.choices.clear();
            self.points = 10;
            self.scoring = Scoring::default();
            self.attempts = 1;
            self.shuffle = false;
            self.correct_layer = None;
            self.incorrect_layer = None;
            self.accepted.clear();
            self.case_sensitive = false;
            self.pairs.clear();
            self.hotspot_image = None;
            self.regions.clear();
            self.markers = 1;
            self.drag_items.clear();
            self.drop_targets.clear();
            self.snap_to_center = true;
            self.return_if_wrong = false;
            self.one_per_target = true;
            self.correct_when = None;
            self.selected_region = None;
        }
    }

    /// Construct a Question from the current state.
    #[must_use]
    pub fn to_question(&self) -> Question {
        Question {
            kind: self.kind,
            prompt: self.prompt.clone(),
            choices: self.choices.clone(),
            points: self.points,
            scoring: self.scoring,
            attempts: self.attempts,
            shuffle: self.shuffle,
            correct_layer: self.correct_layer.clone(),
            incorrect_layer: self.incorrect_layer.clone(),
            accepted: self.accepted.clone(),
            case_sensitive: self.case_sensitive,
            pairs: self.pairs.clone(),
            hotspot_image: self.hotspot_image.clone(),
            regions: self.regions.clone(),
            markers: self.markers,
            drag_items: self.drag_items.clone(),
            drop_targets: self.drop_targets.clone(),
            snap_to_center: self.snap_to_center,
            return_if_wrong: self.return_if_wrong,
            one_per_target: self.one_per_target,
            correct_when: self.correct_when.clone(),
        }
    }
}

/// Closed-combo label for an optional layer reference: the layer's name (id as fallback).
fn layer_label(slide: &Slide, id: Option<&str>) -> String {
    match id {
        None => "(None)".to_owned(),
        Some(id) => slide
            .layer(id)
            .map(|l| l.name.as_str())
            .filter(|n| !n.is_empty())
            .unwrap_or(id)
            .to_owned(),
    }
}

/// Names of visible base-layer text objects that overlap the question prompt.
///
/// Uses the renderer's prompt rect so the warning and the painted prompt agree.
pub(crate) fn prompt_overlap_names(
    course: &coursecraft_course::Course,
    slide: &Slide,
) -> Vec<String> {
    let prompt = coursecraft_render::question_prompt_rect(
        course.slide_size.w as f32,
        course.slide_size.h as f32,
    );
    slide
        .objects
        .iter()
        .filter(|object| {
            matches!(
                object.kind,
                ObjectKind::Text { .. } | ObjectKind::Shape { text: Some(_), .. }
            )
        })
        .filter(|object| object.initial_state != "hidden")
        .filter_map(|object| {
            let frame = coursecraft_render::compose::validate_frame(&object.frame)?;
            frame.intersects(prompt).then(|| {
                if object.name.is_empty() {
                    object.id.clone()
                } else {
                    object.name.clone()
                }
            })
        })
        .collect()
}

/// Render the amber overlap warnings for the question prompt.
fn show_prompt_overlap_warning(ui: &mut Ui, names: &[String]) {
    let p = coursecraft_render::palette(ui.ctx());
    for name in names {
        ui.horizontal(|ui| {
            ui.label(
                coursecraft_render::icon(
                    coursecraft_render::Icon::Warning,
                    coursecraft_render::ICON_SM,
                )
                .color(p.warning),
            );
            ui.label(
                RichText::new(format!(
                    "The question prompt overlaps \"{name}\". Move it or remove it."
                ))
                .color(p.warning)
                .size(coursecraft_render::SIZE_BODY),
            );
        });
    }
}

/// Render the Question bottom panel.
pub fn show_question_tab(
    editor: &mut crate::Editor,
    ui: &mut Ui,
    actions: &mut Vec<(&'static str, Json)>,
) {
    let Some(slide) = editor.session.course.slide(&editor.selection.slide) else {
        ui.label("No slide selected");
        return;
    };
    // A draw slide has no question of its own; its questions come from a bank.
    if slide.draw.is_some() {
        ui.label("This slide draws questions from a bank. Edit the draw in the inspector.");
        return;
    }
    let slide_clone = slide.clone();

    editor
        .ui
        .question
        .sync(&slide_clone, editor.session.revision);

    let p = coursecraft_render::palette(ui.ctx());
    let has_question = slide_clone.question.is_some();
    if !has_question {
        ui.vertical_centered(|ui| {
            ui.add_space(20.0);
            ui.label(
                RichText::new("This slide has no question. Insert one from the toolbar.")
                    .color(p.text_secondary)
                    .size(coursecraft_render::SIZE_BODY),
            );
            ui.add_space(12.0);
            if crate::primary_button(ui, "Question").clicked() {
                crate::toolbar::insert_question(editor, QuestionKind::MultipleChoice);
            }
        });
        return;
    }

    let mut committed = false;
    let mut kind_change: Option<QuestionKind> = None;
    let mut structural: Vec<(&'static str, Json)> = Vec::new();
    let overlaps = prompt_overlap_names(&editor.session.course, &slide_clone);

    // A question can only be graded by a control the learner can click. A
    // submit-event trigger is dead because no player control produces `submit`.
    let submit_triggers: Vec<&Trigger> = slide_clone
        .triggers
        .iter()
        .chain(slide_clone.layers.iter().flat_map(|l| l.triggers.iter()))
        .filter(|t| t.action == Action::SubmitQuestion)
        .collect();
    let needs_submit_button = submit_triggers.is_empty()
        || submit_triggers
            .iter()
            .all(|trigger| trigger.when == Event::Submit);

    let scroll_out = ScrollArea::vertical()
        .id_salt("question_panel_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            // Header bar: Kind, tip line, Remove button
            ui.horizontal(|ui| {
                ui.label(RichText::new("Kind").strong());
                let prev_kind = editor.ui.question.kind;
                egui::ComboBox::from_id_salt("question_kind_combo")
                    .height(320.0)
                    .selected_text(question_kind_name(editor.ui.question.kind))
                    .show_ui(ui, |ui| {
                        for kind in ALL_QUESTION_KINDS {
                            ui.selectable_value(
                                &mut editor.ui.question.kind,
                                kind,
                                question_kind_name(kind),
                            );
                        }
                    });

                if editor.ui.question.kind != prev_kind {
                    // `question.set_kind` seeds a valid question for the new kind.
                    // Keep the buffers on the old kind so a same-frame `question.set`
                    // (a prompt edit that also lost focus) does not write the new
                    // kind with the old payload.
                    kind_change = Some(editor.ui.question.kind);
                    editor.ui.question.kind = prev_kind;
                }

                let p = coursecraft_render::palette(ui.ctx());
                if needs_submit_button {
                    ui.separator();
                    ui.colored_label(
                        p.warning,
                        "no control on this slide submits the question: add a Submit button",
                    );
                    if crate::quiet_primary_button(ui, "Add Submit button").clicked() {
                        actions.push((
                            "question.add_submit_button",
                            serde_json::json!({ "slide": slide_clone.id }),
                        ));
                    }
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if crate::destructive_action_button(ui, "Remove question").clicked() {
                        actions.push((
                            "question.delete",
                            serde_json::json!({
                                "slide": slide_clone.id,
                            }),
                        ));
                    }
                });
            });

            ui.add_space(4.0);
            if !overlaps.is_empty() {
                show_prompt_overlap_warning(ui, &overlaps);
                ui.add_space(4.0);
            }

            if ui.available_width() >= TWO_COLUMN_MIN_WIDTH {
                // Two columns: prompt and choices on the left, settings stacked on the right,
                // so the whole form fits a short panel without scrolling.
                let left_w = ui.available_width() - SETTINGS_COLUMN_WIDTH - 64.0;
                ui.horizontal_top(|ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(left_w, 0.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.set_width(left_w);
                            show_prompt_and_choices(
                                editor,
                                ui,
                                &slide_clone,
                                &mut committed,
                                &mut structural,
                            );
                        },
                    );
                    ui.separator();
                    ui.allocate_ui_with_layout(
                        egui::vec2(SETTINGS_COLUMN_WIDTH, 0.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.set_width(SETTINGS_COLUMN_WIDTH);
                            ui.label(RichText::new("Settings").strong());
                            show_settings(
                                editor,
                                ui,
                                &slide_clone,
                                &mut committed,
                                &mut structural,
                                true,
                            );
                        },
                    );
                });
            } else {
                show_prompt_and_choices(editor, ui, &slide_clone, &mut committed, &mut structural);
                ui.add_space(8.0);
                ui.separator();
                show_settings(
                    editor,
                    ui,
                    &slide_clone,
                    &mut committed,
                    &mut structural,
                    false,
                );
            }
        });
    editor.ui.question.scroll_offset_y = scroll_out.state.clamped_offset().y;
    editor.ui.question.scroll_inner_rect = scroll_out.inner_rect;
    editor.ui.question.scroll_overflow_y =
        (scroll_out.content_size.y - scroll_out.inner_rect.height()).max(0.0);

    // Text edits first, then structural edits, then a kind switch. The kind
    // switch reads the question `question.set` just wrote and replaces only
    // the fields that belong to the new kind.
    if committed {
        let updated = editor.ui.question.to_question();
        actions.push((
            "question.set",
            serde_json::json!({
                "slide": slide_clone.id,
                "question": updated,
            }),
        ));
    }
    actions.extend(structural);
    if let Some(kind) = kind_change {
        actions.push((
            "question.set_kind",
            serde_json::json!({
                "slide": slide_clone.id,
                "kind": kind,
            }),
        ));
    }
}

/// True when a single-line field should be written (blur or Enter).
fn singleline_done(ui: &Ui, response: &egui::Response) -> bool {
    let enter = response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    response.lost_focus() || enter
}

/// Prompt plus the editor for the current kind.
///
/// Text edits set `committed_out` (`question.set`). Add, remove, reorder, and the
/// case-sensitive flag go into `structural` and are not also applied locally.
fn show_prompt_and_choices(
    editor: &mut crate::Editor,
    ui: &mut Ui,
    slide: &Slide,
    committed_out: &mut bool,
    structural: &mut Vec<(&'static str, Json)>,
) {
    let mut committed = false;
    ui.label(RichText::new("Prompt").strong());
    let pr = ui.add(
        TextEdit::multiline(&mut editor.ui.question.prompt)
            .desired_rows(2)
            .desired_width(f32::INFINITY)
            .id_salt("question_prompt_edit"),
    );
    editor.ui.question.prompt_edit_rect = Some(pr.rect);
    let enter =
        pr.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.command);
    if (pr.lost_focus() || enter)
        && Some(&editor.ui.question.prompt) != slide.question.as_ref().map(|q| &q.prompt)
    {
        committed = true;
    }

    ui.add_space(8.0);
    match editor.ui.question.kind {
        QuestionKind::MultipleChoice => {
            show_choice_rows(editor, ui, slide, &mut committed, true, true, true);
        }
        QuestionKind::MultipleResponse => {
            show_choice_rows(editor, ui, slide, &mut committed, false, true, true);
        }
        QuestionKind::TrueFalse => {
            show_choice_rows(editor, ui, slide, &mut committed, true, false, true);
        }
        QuestionKind::FillIn => show_fill_in(editor, ui, slide, &mut committed, structural),
        QuestionKind::Sequence => show_sequence(editor, ui, slide, &mut committed, structural),
        QuestionKind::Matching => show_pairs(editor, ui, slide, &mut committed, structural),
        QuestionKind::Hotspot => show_hotspot(editor, ui, slide, &mut committed, structural),
        QuestionKind::DragDrop => show_drag_drop(editor, ui, slide, &mut committed, structural),
        QuestionKind::Likert => {
            show_choice_rows(editor, ui, slide, &mut committed, true, true, false)
        }
        QuestionKind::ShortAnswer | QuestionKind::Essay => {
            let p = coursecraft_render::palette(ui.ctx());
            ui.label(
                coursecraft_render::caption("Learners type their own answer. It is not graded.")
                    .color(p.text_secondary),
            );
        }
        QuestionKind::Freeform => show_freeform(editor, ui, slide, &mut committed),
    }
    *committed_out |= committed;
}

/// The freeform question's `correct_when` condition, edited with the trigger
/// form's condition builder.
fn show_freeform(editor: &mut crate::Editor, ui: &mut Ui, slide: &Slide, committed: &mut bool) {
    let p = coursecraft_render::palette(ui.ctx());
    ui.label(RichText::new("Correct when").strong());
    ui.label(
        coursecraft_render::caption("Design the slide yourself. Submit grades the condition.")
            .color(p.text_secondary),
    );
    let Some(condition) = editor.ui.question.correct_when.clone() else {
        ui.label(coursecraft_render::caption("No condition set.").color(p.text_secondary));
        return;
    };
    let mut node = crate::triggers::ConditionNode::from_condition(&condition);
    let before = node.clone();
    crate::triggers::render_condition_editor(&mut node, ui, slide, &editor.session.course, 1);
    if node != before
        && let Ok(condition) = node.to_condition()
    {
        editor.ui.question.correct_when = Some(condition);
        *committed = true;
    }
}

/// Choice rows. `single` is a radio (one correct); `list_edit` shows add, reorder, and remove.
/// `marks` draws the correct marker and lets the first choice start correct; survey
/// choices (`marks: false`) never mark a choice correct.
fn show_choice_rows(
    editor: &mut crate::Editor,
    ui: &mut Ui,
    slide: &Slide,
    committed: &mut bool,
    single: bool,
    list_edit: bool,
    marks: bool,
) {
    let p = coursecraft_render::palette(ui.ctx());
    ui.horizontal(|ui| {
        ui.label(coursecraft_render::caption("Choices").color(p.text_secondary));
        let show_add = list_edit && (marks || editor.ui.question.choices.len() < 11);
        if show_add && crate::quiet_primary_button(ui, "+ Add choice").clicked() {
            let next_id = next_choice_id(&editor.ui.question.choices);
            let is_first = editor.ui.question.choices.is_empty();
            editor.ui.question.choices.push(Choice {
                id: next_id,
                text: String::new(),
                correct: marks && is_first,
                points: 0,
            });
            *committed = true;
        }
    });

    let choices_count = editor.ui.question.choices.len();
    let mut choice_to_remove = None;
    let mut choice_to_swap = None;

    for idx in 0..choices_count {
        ui.horizontal(|ui| {
            let (cid, is_correct) = {
                let c = &editor.ui.question.choices[idx];
                (c.id.clone(), c.correct)
            };

            ui.label(RichText::new(format!("{cid}.")).strong());

            if marks && single {
                if ui
                    .radio(is_correct, "")
                    .on_hover_text("Mark as correct choice")
                    .clicked()
                    && !is_correct
                {
                    for (ci, c) in editor.ui.question.choices.iter_mut().enumerate() {
                        c.correct = ci == idx;
                    }
                    *committed = true;
                }
            } else if marks {
                let mut checked = is_correct;
                if ui
                    .checkbox(&mut checked, "")
                    .on_hover_text("Mark as correct choice")
                    .changed()
                {
                    if let Some(c) = editor.ui.question.choices.get_mut(idx) {
                        c.correct = checked;
                    }
                    *committed = true;
                }
            }

            // Leave room for the three reorder/remove buttons so narrow windows never overflow.
            let mut reserve = if list_edit { 130.0 } else { 8.0 };
            let per_answer = editor.ui.question.scoring == Scoring::PerAnswer;
            if per_answer {
                reserve += 64.0;
            }
            let text_w = (ui.available_width() - reserve).clamp(100.0, 320.0);
            let text_edit_res = {
                let c = &mut editor.ui.question.choices[idx];
                ui.add(
                    TextEdit::singleline(&mut c.text)
                        .desired_width(text_w)
                        .id_salt(format!("choice_text_{cid}")),
                )
            };
            let current = editor.ui.question.choices.get(idx).map(|c| c.text.as_str());
            let saved = slide
                .question
                .as_ref()
                .and_then(|q| q.choices.get(idx))
                .map(|c| c.text.as_str());
            if singleline_done(ui, &text_edit_res) && current != saved {
                *committed = true;
            }

            if per_answer {
                let saved = slide
                    .question
                    .as_ref()
                    .and_then(|q| q.choices.get(idx))
                    .map(|c| c.points);
                let pr = {
                    let c = &mut editor.ui.question.choices[idx];
                    ui.add(DragValue::new(&mut c.points).speed(1.0).range(-1000..=1000))
                }
                .on_hover_text("Points for this answer");
                let enter = pr.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let current = editor.ui.question.choices.get(idx).map(|c| c.points);
                if (pr.drag_stopped() || pr.lost_focus() || enter) && current != saved {
                    *committed = true;
                }
            }

            if list_edit {
                if icon_button_enabled(ui, Icon::ArrowUp, idx > 0, "Move up").clicked() {
                    choice_to_swap = Some((idx, idx - 1));
                }
                if icon_button_enabled(ui, Icon::ArrowDown, idx + 1 < choices_count, "Move down")
                    .clicked()
                {
                    choice_to_swap = Some((idx, idx + 1));
                }
                if icon_button_danger(ui, Icon::Trash, true, "Remove choice").clicked() {
                    choice_to_remove = Some(idx);
                }
            }
        });
    }

    if let Some((i1, i2)) = choice_to_swap {
        editor.ui.question.choices.swap(i1, i2);
        *committed = true;
    }
    if let Some(idx) = choice_to_remove
        && idx < editor.ui.question.choices.len()
    {
        editor.ui.question.choices.remove(idx);
        if marks
            && single
            && !editor.ui.question.choices.iter().any(|c| c.correct)
            && let Some(first) = editor.ui.question.choices.first_mut()
        {
            first.correct = true;
        }
        *committed = true;
    }
}

fn show_fill_in(
    editor: &mut crate::Editor,
    ui: &mut Ui,
    slide: &Slide,
    committed: &mut bool,
    structural: &mut Vec<(&'static str, Json)>,
) {
    let sid = slide.id.as_str();
    let n = editor.ui.question.accepted.len();
    let p = coursecraft_render::palette(ui.ctx());
    ui.horizontal(|ui| {
        ui.label(coursecraft_render::caption("Accepted answers").color(p.text_secondary));
        let btn = egui::Button::new(coursecraft_render::body("+ Add answer").color(p.accent_text))
            .fill(Color32::TRANSPARENT);
        if ui.add_enabled(n < 50, btn).clicked() {
            structural.push(("question.add_accepted", serde_json::json!({ "slide": sid })));
        }
    });

    let mut remove = None;
    for idx in 0..n {
        ui.horizontal(|ui| {
            let text_w = (ui.available_width() - 48.0).clamp(100.0, 360.0);
            let response = {
                let Some(slot) = editor.ui.question.accepted.get_mut(idx) else {
                    return;
                };
                ui.add(
                    TextEdit::singleline(slot)
                        .desired_width(text_w)
                        .id_salt(format!("accepted_{idx}")),
                )
            };
            let current = editor.ui.question.accepted.get(idx).map(String::as_str);
            let saved = slide
                .question
                .as_ref()
                .and_then(|q| q.accepted.get(idx))
                .map(String::as_str);
            if singleline_done(ui, &response) && current != saved {
                *committed = true;
            }
            if icon_button_danger(ui, Icon::Trash, n > 1, "Remove answer").clicked() {
                remove = Some(idx);
            }
        });
    }
    if let Some(index) = remove {
        structural.push((
            "question.remove_accepted",
            serde_json::json!({ "slide": sid, "index": index }),
        ));
    }

    if ui
        .checkbox(&mut editor.ui.question.case_sensitive, "Case sensitive")
        .changed()
    {
        structural.push((
            "question.set_case_sensitive",
            serde_json::json!({
                "slide": sid,
                "case_sensitive": editor.ui.question.case_sensitive,
            }),
        ));
    }
}

fn show_sequence(
    editor: &mut crate::Editor,
    ui: &mut Ui,
    slide: &Slide,
    committed: &mut bool,
    structural: &mut Vec<(&'static str, Json)>,
) {
    let sid = slide.id.as_str();
    let n = editor.ui.question.choices.len();
    let p = coursecraft_render::palette(ui.ctx());
    ui.horizontal(|ui| {
        ui.label(coursecraft_render::caption("Items (correct order)").color(p.text_secondary));
        let btn = egui::Button::new(coursecraft_render::body("+ Add item").color(p.accent_text))
            .fill(Color32::TRANSPARENT);
        if ui.add_enabled(n < 20, btn).clicked() {
            let next_id = next_choice_id(&editor.ui.question.choices);
            let label = format!("Item {}", n.saturating_add(1));
            editor.ui.question.choices.push(Choice {
                id: next_id,
                text: label,
                correct: false,
                points: 0,
            });
            *committed = true;
        }
    });

    let mut remove = None;
    let mut reorder: Option<(String, usize)> = None;
    for idx in 0..n {
        ui.horizontal(|ui| {
            let Some(cid) = editor.ui.question.choices.get(idx).map(|c| c.id.clone()) else {
                return;
            };
            ui.label(RichText::new(format!("{}.", idx + 1)).strong());
            let text_w = (ui.available_width() - 130.0).clamp(100.0, 320.0);
            let response = {
                let Some(choice) = editor.ui.question.choices.get_mut(idx) else {
                    return;
                };
                ui.add(
                    TextEdit::singleline(&mut choice.text)
                        .desired_width(text_w)
                        .id_salt(format!("seq_text_{cid}")),
                )
            };
            let current = editor.ui.question.choices.get(idx).map(|c| c.text.as_str());
            let saved = slide
                .question
                .as_ref()
                .and_then(|q| q.choices.get(idx))
                .map(|c| c.text.as_str());
            if singleline_done(ui, &response) && current != saved {
                *committed = true;
            }
            if icon_button_enabled(ui, Icon::ArrowUp, idx > 0, "Move up").clicked() {
                reorder = Some((cid.clone(), idx - 1));
            }
            if icon_button_enabled(ui, Icon::ArrowDown, idx + 1 < n, "Move down").clicked() {
                reorder = Some((cid, idx + 1));
            }
            if icon_button_danger(ui, Icon::Trash, n > 2, "Remove choice").clicked() {
                remove = Some(idx);
            }
        });
    }
    if let Some((id, to)) = reorder {
        structural.push((
            "question.reorder_sequence",
            serde_json::json!({ "slide": sid, "id": id, "to": to }),
        ));
    }
    if let Some(idx) = remove
        && idx < editor.ui.question.choices.len()
    {
        editor.ui.question.choices.remove(idx);
        *committed = true;
    }
}

fn show_pairs(
    editor: &mut crate::Editor,
    ui: &mut Ui,
    slide: &Slide,
    committed: &mut bool,
    structural: &mut Vec<(&'static str, Json)>,
) {
    let sid = slide.id.as_str();
    let n = editor.ui.question.pairs.len();
    let p = coursecraft_render::palette(ui.ctx());
    ui.horizontal(|ui| {
        ui.label(coursecraft_render::caption("Pairs").color(p.text_secondary));
        let btn = egui::Button::new(coursecraft_render::body("+ Add pair").color(p.accent_text))
            .fill(Color32::TRANSPARENT);
        if ui.add_enabled(n < 20, btn).clicked() {
            structural.push(("question.add_pair", serde_json::json!({ "slide": sid })));
        }
    });

    let mut remove: Option<String> = None;
    let mut reorder: Option<(String, usize)> = None;
    for idx in 0..n {
        ui.horizontal(|ui| {
            let Some(pair_id) = editor.ui.question.pairs.get(idx).map(|p| p.id.clone()) else {
                return;
            };
            ui.label(RichText::new(format!("{}.", idx + 1)).strong());
            let text_w = ((ui.available_width() - 150.0) * 0.5).clamp(60.0, 200.0);
            let left_res = {
                let Some(pair) = editor.ui.question.pairs.get_mut(idx) else {
                    return;
                };
                ui.add(
                    TextEdit::singleline(&mut pair.left)
                        .desired_width(text_w)
                        .id_salt(format!("pair_left_{pair_id}")),
                )
            };
            let current = editor.ui.question.pairs.get(idx).map(|p| p.left.as_str());
            let saved = slide
                .question
                .as_ref()
                .and_then(|q| q.pairs.get(idx))
                .map(|p| p.left.as_str());
            if singleline_done(ui, &left_res) && current != saved {
                *committed = true;
            }
            let right_res = {
                let Some(pair) = editor.ui.question.pairs.get_mut(idx) else {
                    return;
                };
                ui.add(
                    TextEdit::singleline(&mut pair.right)
                        .desired_width(text_w)
                        .id_salt(format!("pair_right_{pair_id}")),
                )
            };
            let current = editor.ui.question.pairs.get(idx).map(|p| p.right.as_str());
            let saved = slide
                .question
                .as_ref()
                .and_then(|q| q.pairs.get(idx))
                .map(|p| p.right.as_str());
            if singleline_done(ui, &right_res) && current != saved {
                *committed = true;
            }
            if icon_button_enabled(ui, Icon::ArrowUp, idx > 0, "Move up").clicked() {
                reorder = Some((pair_id.clone(), idx - 1));
            }
            if icon_button_enabled(ui, Icon::ArrowDown, idx + 1 < n, "Move down").clicked() {
                reorder = Some((pair_id.clone(), idx + 1));
            }
            if icon_button_danger(ui, Icon::Trash, n > 2, "Remove pair").clicked() {
                remove = Some(pair_id);
            }
        });
    }
    if let Some((id, to)) = reorder {
        structural.push((
            "question.reorder_pair",
            serde_json::json!({ "slide": sid, "id": id, "to": to }),
        ));
    }
    if let Some(id) = remove {
        structural.push((
            "question.remove_pair",
            serde_json::json!({ "slide": sid, "id": id }),
        ));
    }
}

fn show_hotspot(
    editor: &mut crate::Editor,
    ui: &mut Ui,
    slide: &Slide,
    _committed: &mut bool,
    structural: &mut Vec<(&'static str, Json)>,
) {
    let sid = slide.id.as_str();

    // The hotspot image is a normal base-layer image object. Its default
    // placeholder file is not in the project's media until the author chooses
    // one, so point that out and offer the same picker the image inspector uses.
    let hotspot_image = editor
        .ui
        .question
        .hotspot_image
        .as_deref()
        .and_then(|id| slide.objects.iter().find(|o| o.id == id))
        .filter(|o| matches!(o.kind, ObjectKind::Image { .. }))
        .or_else(|| {
            slide
                .objects
                .iter()
                .find(|o| matches!(o.kind, ObjectKind::Image { .. }))
        })
        .map(|o| {
            let src = match &o.kind {
                ObjectKind::Image { src, .. } => src.as_str(),
                _ => "",
            };
            (o.id.clone(), src.to_owned())
        });
    let hotspot_missing = hotspot_image.as_ref().is_some_and(|(_, src)| {
        src.trim().is_empty() || !editor.media().iter().any(|(name, _)| name == src)
    });

    // 1. Image picker row
    ui.horizontal(|ui| {
        ui.label(RichText::new("Hotspot image").strong());
        let base_images: Vec<(&str, &str)> = slide
            .objects
            .iter()
            .filter(|o| matches!(o.kind, ObjectKind::Image { .. }))
            .map(|o| {
                (
                    o.id.as_str(),
                    if o.name.is_empty() {
                        o.id.as_str()
                    } else {
                        o.name.as_str()
                    },
                )
            })
            .collect();

        let current_image_id = editor.ui.question.hotspot_image.as_deref().unwrap_or("");
        let current_image_name = base_images
            .iter()
            .find(|(id, _)| *id == current_image_id)
            .map(|(_, name)| *name)
            .unwrap_or(if current_image_id.is_empty() {
                "Select an image…"
            } else {
                current_image_id
            });

        egui::ComboBox::from_id_salt("hotspot_image_combo")
            .selected_text(current_image_name)
            .show_ui(ui, |ui| {
                for (id, name) in &base_images {
                    let is_selected = *id == current_image_id;
                    if ui.selectable_label(is_selected, *name).clicked() && !is_selected {
                        structural.push((
                            "question.set_hotspot_image",
                            serde_json::json!({
                                "slide": sid,
                                "image": *id,
                            }),
                        ));
                    }
                }
            });

        if hotspot_missing {
            let p = coursecraft_render::palette(ui.ctx());
            ui.add_space(12.0);
            ui.label(
                coursecraft_render::icon(
                    coursecraft_render::Icon::Warning,
                    coursecraft_render::ICON_SM,
                )
                .color(p.warning),
            );
            ui.label(
                RichText::new("Choose the image learners will click on.")
                    .color(p.warning)
                    .size(coursecraft_render::SIZE_BODY),
            );
            if let Some((image_id, _)) = hotspot_image.as_ref()
                && crate::quiet_primary_button(ui, "Choose image…").clicked()
            {
                editor.pending_replace_object = Some(image_id.clone());
                editor.request(crate::EditorRequest::PickImage);
            }
        }

        ui.add_space(16.0);
        ui.label(RichText::new("Markers").strong());
        let mut markers = editor.ui.question.markers;
        let mr = ui.add(DragValue::new(&mut markers).speed(1.0).range(1..=10));
        let enter = mr.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if (mr.drag_stopped() || mr.lost_focus() || enter) && markers != editor.ui.question.markers
        {
            editor.ui.question.markers = markers;
            structural.push((
                "question.set_markers",
                serde_json::json!({
                    "slide": sid,
                    "markers": markers,
                }),
            ));
        }
    });

    ui.add_space(8.0);

    // 2. Regions list
    let regions = editor.ui.question.regions.clone();
    let n = regions.len();
    let p = coursecraft_render::palette(ui.ctx());
    ui.horizontal(|ui| {
        ui.label(coursecraft_render::caption("Regions").color(p.text_secondary));
        let btn = egui::Button::new(coursecraft_render::body("+ Add region").color(p.accent_text))
            .fill(Color32::TRANSPARENT);
        if ui.add_enabled(n < 20, btn).clicked() {
            structural.push(("question.add_region", serde_json::json!({ "slide": sid })));
        }
    });

    let mut remove_region: Option<String> = None;
    for (idx, r) in regions.iter().enumerate() {
        ui.horizontal(|ui| {
            let p = coursecraft_render::palette(ui.ctx());
            let is_selected = editor.ui.question.selected_region.as_deref() == Some(&r.id);
            let select_label = if is_selected {
                RichText::new(format!("● #{}", idx + 1))
                    .strong()
                    .color(p.accent_text)
            } else {
                RichText::new(format!("#{}", idx + 1))
            };
            if ui.selectable_label(is_selected, select_label).clicked() {
                editor.ui.question.selected_region = Some(r.id.clone());
            }

            // Shape combo
            let current_shape = r.shape;
            let shape_text = match current_shape {
                HotspotShape::Rect => "Rectangle",
                HotspotShape::Ellipse => "Ellipse",
            };
            egui::ComboBox::from_id_salt(format!("region_shape_{}", r.id))
                .selected_text(shape_text)
                .width(90.0)
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_value(&mut r.shape.clone(), HotspotShape::Rect, "Rectangle")
                        .clicked()
                        && current_shape != HotspotShape::Rect
                    {
                        structural.push((
                            "question.update_region",
                            serde_json::json!({
                                "slide": sid,
                                "id": r.id,
                                "shape": "rect",
                            }),
                        ));
                    }
                    if ui
                        .selectable_value(&mut r.shape.clone(), HotspotShape::Ellipse, "Ellipse")
                        .clicked()
                        && current_shape != HotspotShape::Ellipse
                    {
                        structural.push((
                            "question.update_region",
                            serde_json::json!({
                                "slide": sid,
                                "id": r.id,
                                "shape": "ellipse",
                            }),
                        ));
                    }
                });

            // X, Y, W, H drag values
            ui.label("X");
            let mut x = r.x;
            let xr = ui.add(
                DragValue::new(&mut x)
                    .speed(1.0)
                    .range(-100_000.0..=100_000.0),
            );
            let enter_x = xr.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (xr.drag_stopped() || xr.lost_focus() || enter_x) && (x - r.x).abs() > 1e-3 {
                structural.push((
                    "question.update_region",
                    serde_json::json!({
                        "slide": sid,
                        "id": r.id,
                        "x": x,
                    }),
                ));
            }

            ui.label("Y");
            let mut y = r.y;
            let yr = ui.add(
                DragValue::new(&mut y)
                    .speed(1.0)
                    .range(-100_000.0..=100_000.0),
            );
            let enter_y = yr.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (yr.drag_stopped() || yr.lost_focus() || enter_y) && (y - r.y).abs() > 1e-3 {
                structural.push((
                    "question.update_region",
                    serde_json::json!({
                        "slide": sid,
                        "id": r.id,
                        "y": y,
                    }),
                ));
            }

            ui.label("W");
            let mut w = r.w;
            let wr = ui.add(DragValue::new(&mut w).speed(1.0).range(1.0..=100_000.0));
            let enter_w = wr.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (wr.drag_stopped() || wr.lost_focus() || enter_w) && (w - r.w).abs() > 1e-3 {
                structural.push((
                    "question.update_region",
                    serde_json::json!({
                        "slide": sid,
                        "id": r.id,
                        "w": w,
                    }),
                ));
            }

            ui.label("H");
            let mut h = r.h;
            let hr = ui.add(DragValue::new(&mut h).speed(1.0).range(1.0..=100_000.0));
            let enter_h = hr.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (hr.drag_stopped() || hr.lost_focus() || enter_h) && (h - r.h).abs() > 1e-3 {
                structural.push((
                    "question.update_region",
                    serde_json::json!({
                        "slide": sid,
                        "id": r.id,
                        "h": h,
                    }),
                ));
            }

            // Correct checkbox
            let mut correct = r.correct;
            let can_toggle_correct =
                !correct || regions.iter().filter(|reg| reg.correct).count() > 1;
            if ui
                .add_enabled(
                    can_toggle_correct,
                    egui::Checkbox::new(&mut correct, "Correct"),
                )
                .changed()
            {
                structural.push((
                    "question.update_region",
                    serde_json::json!({
                        "slide": sid,
                        "id": r.id,
                        "correct": correct,
                    }),
                ));
            }

            // Delete button
            let can_delete =
                n > 1 && (!r.correct || regions.iter().filter(|reg| reg.correct).count() > 1);
            if icon_button_danger(ui, Icon::Trash, can_delete, "Remove region").clicked() {
                remove_region = Some(r.id.clone());
            }
        });
    }

    if let Some(id) = remove_region {
        structural.push((
            "question.remove_region",
            serde_json::json!({
                "slide": sid,
                "id": id,
            }),
        ));
    }
}

fn show_drag_drop(
    editor: &mut crate::Editor,
    ui: &mut Ui,
    slide: &Slide,
    _committed: &mut bool,
    structural: &mut Vec<(&'static str, Json)>,
) {
    let sid = slide.id.as_str();

    // The three option checkboxes
    ui.horizontal(|ui| {
        let mut snap = editor.ui.question.snap_to_center;
        if ui.checkbox(&mut snap, "Snap to center").changed() {
            editor.ui.question.snap_to_center = snap;
            structural.push((
                "question.set_drag_options",
                serde_json::json!({
                    "slide": sid,
                    "snap_to_center": snap,
                }),
            ));
        }

        let mut ret = editor.ui.question.return_if_wrong;
        if ui.checkbox(&mut ret, "Return if wrong").changed() {
            editor.ui.question.return_if_wrong = ret;
            structural.push((
                "question.set_drag_options",
                serde_json::json!({
                    "slide": sid,
                    "return_if_wrong": ret,
                }),
            ));
        }

        let mut one = editor.ui.question.one_per_target;
        if ui.checkbox(&mut one, "One per target").changed() {
            editor.ui.question.one_per_target = one;
            structural.push((
                "question.set_drag_options",
                serde_json::json!({
                    "slide": sid,
                    "one_per_target": one,
                }),
            ));
        }
    });

    ui.add_space(8.0);

    // Rows of item -> target pickers
    let drag_items = editor.ui.question.drag_items.clone();
    let n = drag_items.len();

    let obj_name = |id: &str| -> String {
        slide
            .objects
            .iter()
            .find(|o| o.id == id)
            .map(|o| {
                if o.name.is_empty() {
                    o.id.clone()
                } else {
                    o.name.clone()
                }
            })
            .unwrap_or_else(|| id.to_owned())
    };

    let available_new_items: Vec<String> = slide
        .objects
        .iter()
        .filter(|o| {
            !drag_items.iter().any(|d| d.item == o.id)
                && !editor.ui.question.drop_targets.contains(&o.id)
        })
        .map(|o| o.id.clone())
        .collect();

    let target_candidates: Vec<String> = slide
        .objects
        .iter()
        .filter(|o| !drag_items.iter().any(|d| d.item == o.id))
        .map(|o| o.id.clone())
        .collect();

    let p = coursecraft_render::palette(ui.ctx());
    ui.horizontal(|ui| {
        ui.label(coursecraft_render::caption("Drag items & targets").color(p.text_secondary));
        let can_add = n < 20 && !available_new_items.is_empty() && !target_candidates.is_empty();
        let btn = egui::Button::new(coursecraft_render::body("+ Add item").color(p.accent_text))
            .fill(Color32::TRANSPARENT);
        if ui.add_enabled(can_add, btn).clicked() {
            let new_item = available_new_items[0].clone();
            let new_target = target_candidates
                .iter()
                .find(|t| *t != &new_item)
                .cloned()
                .unwrap_or_else(|| target_candidates[0].clone());
            structural.push((
                "question.add_drag_item",
                serde_json::json!({
                    "slide": sid,
                    "item": new_item,
                    "target": new_target,
                }),
            ));
        }
    });

    let mut remove_item: Option<String> = None;
    for (idx, d) in drag_items.iter().enumerate() {
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{}.", idx + 1)).strong());
            ui.label(format!("{} →", obj_name(&d.item)));

            let current_target_name = obj_name(&d.target);
            egui::ComboBox::from_id_salt(format!("target_combo_{}", d.item))
                .selected_text(current_target_name)
                .width(140.0)
                .show_ui(ui, |ui| {
                    for t_id in &target_candidates {
                        if t_id == &d.item {
                            continue;
                        }
                        let name = obj_name(t_id);
                        let is_selected = t_id == &d.target;
                        if ui.selectable_label(is_selected, name).clicked() && !is_selected {
                            structural.push((
                                "question.update_drag_item",
                                serde_json::json!({
                                    "slide": sid,
                                    "item": d.item,
                                    "target": t_id,
                                }),
                            ));
                        }
                    }
                });

            if icon_button_danger(ui, Icon::Trash, n > 1, "Remove item").clicked() {
                remove_item = Some(d.item.clone());
            }
        });
    }

    if let Some(item) = remove_item {
        structural.push((
            "question.remove_drag_item",
            serde_json::json!({
                "slide": sid,
                "item": item,
            }),
        ));
    }

    ui.add_space(8.0);

    // Drop-target checklist
    ui.label(RichText::new("Drop targets").strong());
    let mut updated_targets = editor.ui.question.drop_targets.clone();
    let mut targets_changed = false;

    for o in &slide.objects {
        if drag_items.iter().any(|d| d.item == o.id) {
            continue;
        }
        let is_required = drag_items.iter().any(|d| d.target == o.id);
        let mut is_checked = updated_targets.contains(&o.id);
        let name = if o.name.is_empty() { &o.id } else { &o.name };

        let mut cb = ui.add_enabled(!is_required, egui::Checkbox::new(&mut is_checked, name));
        if is_required {
            cb = cb.on_hover_text("This target is required because a drag item targets it");
        }
        if cb.changed() {
            if is_checked && !updated_targets.contains(&o.id) {
                if updated_targets.len() < 20 {
                    updated_targets.push(o.id.clone());
                    targets_changed = true;
                }
            } else if !is_checked && updated_targets.contains(&o.id) && !is_required {
                updated_targets.retain(|t| t != &o.id);
                targets_changed = true;
            }
        }
    }

    if targets_changed && !updated_targets.is_empty() {
        structural.push((
            "question.set_drop_targets",
            serde_json::json!({
                "slide": sid,
                "targets": updated_targets,
            }),
        ));
    }
}

/// Layer picker combo: `current` is the selected layer id (None = no layer).
fn layer_picker(
    ui: &mut Ui,
    label: &str,
    salt: &str,
    current: &mut Option<String>,
    slide: &Slide,
    committed: &mut bool,
) {
    ui.label(label);
    egui::ComboBox::from_id_salt(salt)
        .selected_text(layer_label(slide, current.as_deref()))
        .show_ui(ui, |ui| {
            let is_none = current.is_none();
            if ui.selectable_label(is_none, "(None)").clicked() && !is_none {
                *current = None;
                *committed = true;
            }
            for l in &slide.layers {
                let is_sel = current.as_deref() == Some(&l.id);
                if ui.selectable_label(is_sel, &l.name).clicked() && !is_sel {
                    *current = Some(l.id.clone());
                    *committed = true;
                }
            }
        });
}

fn points_ui(editor: &mut crate::Editor, ui: &mut Ui, slide: &Slide, committed: &mut bool) {
    if editor.ui.question.to_question().is_survey() {
        return;
    }
    if editor.ui.question.scoring == Scoring::PerAnswer {
        let n = editor.ui.question.to_question().max_points();
        ui.label(format!("Worth {n} points"));
        return;
    }
    ui.label("Points");
    let pr = ui.add(
        DragValue::new(&mut editor.ui.question.points)
            .speed(1.0)
            .range(0..=1000),
    );
    let enter = pr.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    if (pr.drag_stopped() || pr.lost_focus() || enter)
        && Some(editor.ui.question.points) != slide.question.as_ref().map(|q| q.points)
    {
        *committed = true;
    }
}

fn scoring_label(scoring: Scoring) -> &'static str {
    match scoring {
        Scoring::AllOrNothing => "All or nothing",
        Scoring::PerAnswer => "Points per answer",
        Scoring::Partial => "Partial credit",
    }
}

fn scoring_ui(editor: &mut crate::Editor, ui: &mut Ui, committed: &mut bool) {
    let options = scoring_options(editor.ui.question.kind);
    if options.is_empty() {
        return;
    }
    ui.label("Scoring");
    let current = editor.ui.question.scoring;
    let selected = scoring_label(current);
    egui::ComboBox::from_id_salt("question_scoring_combo")
        .selected_text(selected)
        .show_ui(ui, |ui| {
            for &scoring in options {
                let label = scoring_label(scoring);
                if ui.selectable_label(scoring == current, label).clicked() && scoring != current {
                    editor.ui.question.scoring = scoring;
                    *committed = true;
                }
            }
        });
}

fn attempts_ui(editor: &mut crate::Editor, ui: &mut Ui, slide: &Slide, committed: &mut bool) {
    if editor.ui.question.to_question().is_survey() {
        return;
    }
    ui.label("Attempts (0 = unlimited)");
    let ar = ui.add(
        DragValue::new(&mut editor.ui.question.attempts)
            .speed(1.0)
            .range(0..=100),
    );
    let enter = ar.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    if (ar.drag_stopped() || ar.lost_focus() || enter)
        && Some(editor.ui.question.attempts) != slide.question.as_ref().map(|q| q.attempts)
    {
        *committed = true;
    }
}

fn shuffle_ui(editor: &mut crate::Editor, ui: &mut Ui, committed: &mut bool) {
    match editor.ui.question.kind {
        QuestionKind::Sequence | QuestionKind::Matching => {
            ui.label("Shuffled each attempt");
        }
        QuestionKind::MultipleChoice
        | QuestionKind::MultipleResponse
        | QuestionKind::TrueFalse
        | QuestionKind::FillIn => {
            if ui
                .checkbox(&mut editor.ui.question.shuffle, "Shuffle choices")
                .changed()
            {
                *committed = true;
            }
        }
        QuestionKind::Hotspot
        | QuestionKind::DragDrop
        | QuestionKind::Likert
        | QuestionKind::ShortAnswer
        | QuestionKind::Essay
        | QuestionKind::Freeform => {}
    }
}

/// Points / attempts / shuffle and the correct / incorrect layer targets. `stacked` puts every
/// setting on its own row (narrow side column); otherwise they flow in two wide rows.
fn show_settings(
    editor: &mut crate::Editor,
    ui: &mut Ui,
    slide: &Slide,
    committed: &mut bool,
    structural: &mut Vec<(&'static str, Json)>,
    stacked: bool,
) {
    let is_survey = editor.ui.question.to_question().is_survey();
    if stacked {
        if is_survey {
            ui.horizontal(|ui| {
                ui.label("Not graded");
            });
        } else {
            ui.horizontal(|ui| points_ui(editor, ui, slide, committed));
        }
        ui.horizontal(|ui| scoring_ui(editor, ui, committed));
        ui.horizontal(|ui| attempts_ui(editor, ui, slide, committed));
        ui.horizontal(|ui| shuffle_ui(editor, ui, committed));
        ui.horizontal(|ui| {
            let q = &mut editor.ui.question;
            layer_picker(
                ui,
                "Correct layer",
                "correct_layer_combo",
                &mut q.correct_layer,
                slide,
                committed,
            );
        });
        ui.horizontal(|ui| {
            let q = &mut editor.ui.question;
            layer_picker(
                ui,
                "Incorrect layer",
                "incorrect_layer_combo",
                &mut q.incorrect_layer,
                slide,
                committed,
            );
        });
        add_feedback_button(editor, ui, slide, structural);
        return;
    }
    ui.horizontal(|ui| {
        if is_survey {
            ui.label("Not graded");
        } else {
            points_ui(editor, ui, slide, committed);
        }
        ui.add_space(16.0);
        scoring_ui(editor, ui, committed);
        ui.add_space(16.0);
        attempts_ui(editor, ui, slide, committed);
        ui.add_space(16.0);
        shuffle_ui(editor, ui, committed);
    });
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let q = &mut editor.ui.question;
        layer_picker(
            ui,
            "Correct layer",
            "correct_layer_combo",
            &mut q.correct_layer,
            slide,
            committed,
        );
        ui.add_space(16.0);
        layer_picker(
            ui,
            "Incorrect layer",
            "incorrect_layer_combo",
            &mut q.incorrect_layer,
            slide,
            committed,
        );
    });
    add_feedback_button(editor, ui, slide, structural);
}

/// Offer the two default feedback layers while the question has neither.
fn add_feedback_button(
    editor: &mut crate::Editor,
    ui: &mut Ui,
    slide: &Slide,
    structural: &mut Vec<(&'static str, Json)>,
) {
    if editor.ui.question.correct_layer.is_some() || editor.ui.question.incorrect_layer.is_some() {
        return;
    }
    if crate::quiet_primary_button(ui, "Add feedback layers").clicked() {
        structural.push((
            "question.add_feedback_layers",
            serde_json::json!({ "slide": slide.id }),
        ));
    }
}

/// Minimum width of the whole form for the two-column layout (choices left, settings right).
const TWO_COLUMN_MIN_WIDTH: f32 = 760.0;
/// Width reserved for the settings column in the two-column layout.
const SETTINGS_COLUMN_WIDTH: f32 = 250.0;
