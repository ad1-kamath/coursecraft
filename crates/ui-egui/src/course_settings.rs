//! Course settings window dialog: title, id, player chrome toggles, quiz passing percent.

use crate::dialog::{Dialog, Form};
use coursecraft_course::{
    Course, NavMode, NextRequirement, PlayerOverrideField, SeekbarBehaviour, ValueKind,
};
use egui::{DragValue, TextEdit};

/// Names of the true/false variables a "Next unlocks" rule can watch.
#[must_use]
pub fn bool_variable_names(course: &Course) -> Vec<String> {
    let mut names: Vec<String> = coursecraft_course::BUILTIN_VARIABLES
        .iter()
        .filter(|(_, kind)| *kind == ValueKind::Bool)
        .map(|(name, _)| (*name).to_owned())
        .collect();
    names.extend(
        course
            .variables
            .iter()
            .filter(|v| v.value.kind() == ValueKind::Bool)
            .map(|v| v.name.clone()),
    );
    names
}

/// How many slide names an override note lists before it shortens the list.
const OVERRIDE_NOTE_NAMES: usize = 3;

/// The note under a course player setting when slides set their own value for it,
/// so the dialog never shows a setting the player does not apply. Names the
/// slides and offers to make the course setting apply on every slide. Returns
/// `field` when the author asks for that.
fn slide_override_note(
    ui: &mut egui::Ui,
    course: &Course,
    field: PlayerOverrideField,
) -> Option<PlayerOverrideField> {
    let slides = course.slides_overriding(field);
    if slides.is_empty() {
        return None;
    }
    let mut names: Vec<&str> = slides
        .iter()
        .take(OVERRIDE_NOTE_NAMES)
        .map(|slide| {
            if slide.title.trim().is_empty() {
                slide.id.as_str()
            } else {
                slide.title.as_str()
            }
        })
        .collect();
    if slides.len() > OVERRIDE_NOTE_NAMES {
        names.push("…");
    }
    let names = names.join(", ");
    let note = crate::i18n::translate_with(
        "These slides use their own setting: {slides}",
        &[("slides", names.as_str())],
    );
    let mut chosen = None;
    ui.horizontal_wrapped(|ui| {
        ui.label(
            egui::RichText::new(note)
                .color(coursecraft_render::palette(ui.ctx()).warning)
                .size(coursecraft_render::SIZE_SMALL),
        );
        if crate::quiet_button(
            ui,
            crate::tl!("Use on all slides"),
            crate::QuietTone::Secondary,
            true,
        )
        .on_hover_text(crate::tl!(
            "Remove the slides' own setting so this course setting applies everywhere"
        ))
        .clicked()
        {
            chosen = Some(field);
        }
    });
    chosen
}

/// The "Next unlocks" combo and variable picker. `current` is the rule shown
/// (`None` means "Course default", offered only when `allow_default`). Returns
/// the new choice when the author changes it.
pub fn next_rule_editor(
    ui: &mut egui::Ui,
    id_salt: &str,
    current: Option<&NextRequirement>,
    allow_default: bool,
    wide: bool,
    variables: &[String],
) -> Option<Option<NextRequirement>> {
    // A full-width combo inside an auto-sized window would feed back into its own width.
    let width = if wide {
        ui.available_width().max(90.0)
    } else {
        120.0
    };
    let label = match current {
        None => "Course default",
        Some(NextRequirement::Nothing) => "Always",
        Some(NextRequirement::TimelineEnds) => "When the timeline ends",
        Some(NextRequirement::Variable { .. }) => "When a variable is true",
    };
    let first_variable = variables.first().cloned().unwrap_or_default();
    let mut chosen: Option<Option<NextRequirement>> = None;
    crate::widgets::styled_combo(ui, id_salt, width)
        .selected_text(label)
        .show_ui(ui, |ui| {
            if allow_default
                && ui
                    .selectable_label(current.is_none(), "Course default")
                    .clicked()
            {
                chosen = Some(None);
            }
            if ui
                .selectable_label(current == Some(&NextRequirement::Nothing), "Always")
                .clicked()
            {
                chosen = Some(Some(NextRequirement::Nothing));
            }
            if ui
                .selectable_label(
                    current == Some(&NextRequirement::TimelineEnds),
                    "When the timeline ends",
                )
                .clicked()
            {
                chosen = Some(Some(NextRequirement::TimelineEnds));
            }
            let is_variable = matches!(current, Some(NextRequirement::Variable { .. }));
            if !first_variable.is_empty()
                && ui
                    .selectable_label(is_variable, "When a variable is true")
                    .clicked()
                && !is_variable
            {
                chosen = Some(Some(NextRequirement::Variable {
                    name: first_variable.clone(),
                }));
            }
        });
    if let Some(NextRequirement::Variable { name }) = current {
        let mut picked = name.clone();
        crate::widgets::styled_combo(ui, format!("{id_salt}_variable"), width)
            .selected_text(name.as_str())
            .show_ui(ui, |ui| {
                for var in variables {
                    ui.selectable_value(&mut picked, var.clone(), var.as_str());
                }
            });
        if picked != *name {
            chosen = Some(Some(NextRequirement::Variable { name: picked }));
        }
    }
    chosen
}

/// The player logo row: a combo of the project's image files and a Clear button.
/// Returns the new choice (`None` clears the logo).
fn logo_picker(
    ui: &mut egui::Ui,
    form: &Form,
    current: Option<&str>,
    media: &[(String, std::sync::Arc<[u8]>)],
) -> Option<Option<String>> {
    let mut chosen = None;
    form.row(ui, "Logo", |ui, _| {
        crate::widgets::styled_combo(ui, "course_player_logo", 160.0)
            .selected_text(current.unwrap_or("None"))
            .show_ui(ui, |ui| {
                for (src, _) in media {
                    if coursecraft_course::is_image_src(src)
                        && ui
                            .selectable_label(current == Some(src.as_str()), src.as_str())
                            .clicked()
                    {
                        chosen = Some(Some(src.clone()));
                    }
                }
            });
        if ui
            .add_enabled(current.is_some(), egui::Button::new("Clear"))
            .on_disabled_hover_text("No logo is selected")
            .clicked()
        {
            chosen = Some(None);
        }
    });
    chosen
}

fn nav_mode_label(mode: NavMode) -> &'static str {
    match mode {
        NavMode::Free => "Free",
        NavMode::Restricted => "Restricted",
        NavMode::Locked => "Locked",
    }
}

fn seekbar_label(mode: SeekbarBehaviour) -> &'static str {
    match mode {
        SeekbarBehaviour::Free => "Free",
        SeekbarBehaviour::Locked => "Locked",
        SeekbarBehaviour::UntilTimelineEnds => "Locked until timeline ends",
    }
}

fn seekbar_hint(mode: SeekbarBehaviour) -> &'static str {
    match mode {
        SeekbarBehaviour::Free => "Learners can drag the seekbar at any time.",
        SeekbarBehaviour::Locked => "The seekbar shows progress, but learners can't move it.",
        SeekbarBehaviour::UntilTimelineEnds => {
            "Learners can move the seekbar once the slide's timeline has played to the end."
        }
    }
}

/// Local buffers for editing Course Settings.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CourseSettingsState {
    pub last_synced_revision: crate::SyncRevision,
    pub title: String,
    pub id: String,
    pub language: String,
    pub show_bar: bool,
    pub show_nav: bool,
    pub show_menu: bool,
    pub show_seekbar: bool,
    pub show_speed: bool,
    pub show_fullscreen: bool,
    pub show_transcript: bool,
    pub show_title: bool,
    pub show_volume: bool,
    pub show_captions: bool,
    pub logo: Option<String>,
    pub resources: crate::resources_ui::ResourceBuffers,
    pub open_resources: bool,
    pub navigation: NavMode,
    pub next_requires: NextRequirement,
    pub seekbar: SeekbarBehaviour,
    pub quiz_enabled: bool,
    pub passing_percent: u8,
    pub background_audio_src: Option<String>,
    pub background_volume: f32,
    pub background_loop: bool,
    pub background_volume_rect: Option<egui::Rect>,
}

impl CourseSettingsState {
    /// Resync state buffers from course data on revision change.
    pub fn sync(&mut self, course: &Course, revision: u64) {
        if self.last_synced_revision.is(revision) {
            return;
        }
        self.last_synced_revision.set(revision);
        self.title = course.title.clone();
        self.id = course.id.clone();
        self.language = course.language.clone().unwrap_or_default();
        self.show_bar = course.player.show_bar;
        self.show_nav = course.player.show_nav;
        self.show_menu = course.player.show_menu;
        self.show_seekbar = course.player.show_seekbar;
        self.show_speed = course.player.show_speed;
        self.show_fullscreen = course.player.show_fullscreen;
        self.show_transcript = course.player.show_transcript;
        self.show_title = course.player.show_title;
        self.show_volume = course.player.show_volume;
        self.show_captions = course.player.show_captions;
        self.logo = course.player.logo.clone();
        self.resources.sync(course);
        self.navigation = course.player.navigation;
        self.next_requires = course.player.next_requires.clone();
        self.seekbar = course.player.seekbar;
        if let Some(ref q) = course.quiz {
            self.quiz_enabled = true;
            self.passing_percent = q.passing_percent;
        } else {
            self.quiz_enabled = false;
            self.passing_percent = 80;
        }
        if let Some(ref audio) = course.background_audio {
            self.background_audio_src = Some(audio.src.clone());
            self.background_volume = audio.volume * 100.0;
            self.background_loop = audio.looping;
        } else {
            self.background_audio_src = None;
            self.background_volume = 50.0;
            self.background_loop = true;
        }
    }
}

/// Render the Course Settings modal window dialog.
pub fn show_course_settings_window(editor: &mut crate::Editor, ctx: &egui::Context) {
    if !editor.ui.show_course_settings_window {
        return;
    }

    editor
        .ui
        .course_settings
        .sync(&editor.session.course, editor.session.revision);

    let mut open = true;
    let mut close = false;
    let mut update_course = false;
    let mut update_player = false;
    let mut update_quiz = false;
    let mut add_finish = false;
    let mut pick_background_audio = false;
    let mut remove_background_audio = false;
    let mut update_background_volume = false;
    let mut update_background_loop = false;
    let mut resource_actions = crate::resources_ui::ResourceActions::default();
    let mut logo_choice: Option<Option<String>> = None;
    let mut clear_override: Option<PlayerOverrideField> = None;

    let needs_result = !editor.session.course.reports_result();
    let bool_names = bool_variable_names(&editor.session.course);

    Dialog::new("Course settings")
        .open(&mut open)
        .resizable(true)
        .default_size(egui::vec2(460.0, 560.0))
        .show_with_footer(ctx, |dialog| {
            dialog.body(|ui| {
                // Course Info
                // One label column for the whole dialog, so every field starts at the same x.
                let form = Form::new(
                    ui,
                    &[
                        "Course title",
                        "Course ID",
                        "Language",
                        "Slide size",
                        "Logo",
                        "Menu access",
                        "Seekbar",
                        "Next unlocks",
                        "Passing score",
                        "Volume",
                    ],
                );
                ui.heading("General");
                form.row(ui, "Course title", |ui, width| {
                    let tr = ui.add(
                        TextEdit::singleline(&mut editor.ui.course_settings.title)
                            .desired_width(width),
                    );
                    let enter = tr.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if (tr.lost_focus() || enter)
                        && editor.ui.course_settings.title != editor.session.course.title
                    {
                        update_course = true;
                    }
                });

                form.row(ui, "Course ID", |ui, width| {
                    let ir = ui.add(
                        TextEdit::singleline(&mut editor.ui.course_settings.id)
                            .desired_width(width),
                    );
                    let enter = ir.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if (ir.lost_focus() || enter)
                        && editor.ui.course_settings.id != editor.session.course.id
                    {
                        update_course = true;
                    }
                });

                let mut language_valid = true;
                form.row(ui, "Language", |ui, width| {
                    let lr = ui.add(
                        TextEdit::singleline(&mut editor.ui.course_settings.language)
                            .desired_width(width),
                    );
                    let enter = lr.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    let tag = editor.ui.course_settings.language.trim();
                    language_valid = tag.is_empty() || coursecraft_course::is_language_tag_valid(tag);
                    if (lr.lost_focus() || enter)
                        && language_valid
                        && Some(tag) != editor.session.course.language.as_deref()
                    {
                        update_course = true;
                    }
                });
                // The note, or the error that replaces it, lines up with the fields.
                ui.horizontal(|ui| {
                    ui.add_space(form.label_width() + Form::LABEL_GAP);
                    let p = coursecraft_render::palette(ui.ctx());
                    if language_valid {
                        ui.label(
                            egui::RichText::new(
                                "Used by screen readers. For example en, fr or pt-BR.",
                            )
                            .color(p.text_secondary)
                            .size(coursecraft_render::SIZE_SMALL),
                        );
                    } else {
                        ui.colored_label(p.danger, "Use a tag such as en, fr or pt-BR.");
                    }
                });

                form.row(ui, "Slide size", |ui, _width| {
                    let size = editor.session.course.slide_size;
                    let ratio = crate::slide_size_dialog::format_aspect_ratio(size.w, size.h);
                    ui.horizontal(|ui| {
                        ui.label(format!("{} × {} px ({ratio})", size.w, size.h));
                        if ui.button("Change…").clicked() {
                            editor.open_slide_size_dialog();
                        }
                    });
                });

                if needs_result {
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.colored_label(
                            coursecraft_render::palette(ui.ctx()).warning,
                            "this course never reports a result to the LMS: add a Finish button on the last slide",
                        );
                        if crate::quiet_primary_button(ui, "Add Finish button").clicked() {
                            add_finish = true;
                        }
                    });
                }

                ui.add_space(8.0);
                ui.separator();

                // Player Chrome Toggles
                ui.heading("Player chrome");
                if ui
                    .checkbox(&mut editor.ui.course_settings.show_bar, "Player bar")
                    .changed()
                {
                    update_player = true;
                }
                clear_override = clear_override.or(slide_override_note(
                    ui,
                    &editor.session.course,
                    PlayerOverrideField::ShowBar,
                ));
                const BAR_OFF_TOOLTIP: &str = "Turn the player bar on to use this";
                ui.add_enabled_ui(editor.ui.course_settings.show_bar, |ui| {
                    if ui
                        .checkbox(
                            &mut editor.ui.course_settings.show_nav,
                            "Show navigation (Previous / Next buttons)",
                        )
                        .on_disabled_hover_text(BAR_OFF_TOOLTIP)
                        .changed()
                    {
                        update_player = true;
                    }
                    clear_override = clear_override.or(slide_override_note(
                        ui,
                        &editor.session.course,
                        PlayerOverrideField::ShowNav,
                    ));
                    if ui
                        .checkbox(
                            &mut editor.ui.course_settings.show_menu,
                            "Show slide menu (outline)",
                        )
                        .on_disabled_hover_text(BAR_OFF_TOOLTIP)
                        .changed()
                    {
                        update_player = true;
                    }
                    clear_override = clear_override.or(slide_override_note(
                        ui,
                        &editor.session.course,
                        PlayerOverrideField::ShowMenu,
                    ));
                    if ui
                        .checkbox(&mut editor.ui.course_settings.show_seekbar, "Show seekbar")
                        .on_disabled_hover_text(BAR_OFF_TOOLTIP)
                        .changed()
                    {
                        update_player = true;
                    }
                    clear_override = clear_override.or(slide_override_note(
                        ui,
                        &editor.session.course,
                        PlayerOverrideField::ShowSeekbar,
                    ));
                    if ui
                        .checkbox(
                            &mut editor.ui.course_settings.show_speed,
                            "Show playback speed",
                        )
                        .on_disabled_hover_text(BAR_OFF_TOOLTIP)
                        .changed()
                    {
                        update_player = true;
                    }
                    if ui
                        .checkbox(
                            &mut editor.ui.course_settings.show_fullscreen,
                            "Show full screen",
                        )
                        .on_disabled_hover_text(BAR_OFF_TOOLTIP)
                        .changed()
                    {
                        update_player = true;
                    }
                    if ui
                        .checkbox(
                            &mut editor.ui.course_settings.show_transcript,
                            "Show notes as transcript",
                        )
                        .on_disabled_hover_text(BAR_OFF_TOOLTIP)
                        .changed()
                    {
                        update_player = true;
                    }
                    ui.horizontal(|ui| {
                        for (flag, label) in [
                            (&mut editor.ui.course_settings.show_title, "Title"),
                            (&mut editor.ui.course_settings.show_volume, "Volume"),
                            (&mut editor.ui.course_settings.show_captions, "Captions"),
                        ] {
                            if ui
                                .checkbox(flag, label)
                                .on_disabled_hover_text(BAR_OFF_TOOLTIP)
                                .changed()
                            {
                                update_player = true;
                            }
                        }
                    });
                    if let Some(choice) = logo_picker(
                        ui,
                        &form,
                        editor.ui.course_settings.logo.as_deref(),
                        &editor.media,
                    ) {
                        logo_choice = Some(choice);
                    }
                });
                form.row(ui, "Menu access", |ui, _| {
                    let mut mode = editor.ui.course_settings.navigation;
                    crate::widgets::styled_combo(
                        ui,
                        "course_navigation_mode",
                        crate::widgets::COMBO_MIN_WIDTH,
                    )
                    .selected_text(nav_mode_label(mode))
                    .show_ui(ui, |ui| {
                        for choice in [NavMode::Free, NavMode::Restricted, NavMode::Locked] {
                            ui.selectable_value(&mut mode, choice, nav_mode_label(choice));
                        }
                    })
                    .response
                    .on_hover_text(crate::tl!(
                        "Which slides the learner can open from the slide menu. Next is set by \"Next unlocks\"."
                    ));
                    if mode != editor.ui.course_settings.navigation {
                        editor.ui.course_settings.navigation = mode;
                        update_player = true;
                    }
                });
                form.row(ui, "Seekbar", |ui, _| {
                    let mut mode = editor.ui.course_settings.seekbar;
                    let seekbar_enabled = editor.ui.course_settings.show_bar
                        && editor.ui.course_settings.show_seekbar;
                    let disabled_tip = if !editor.ui.course_settings.show_bar {
                        BAR_OFF_TOOLTIP
                    } else {
                        "Turn \"Show seekbar\" on to use this"
                    };
                    ui.add_enabled_ui(seekbar_enabled, |ui| {
                        crate::widgets::styled_combo(
                            ui,
                            "course_seekbar_behaviour",
                            crate::widgets::COMBO_MIN_WIDTH,
                        )
                        .selected_text(seekbar_label(mode))
                        .show_ui(ui, |ui| {
                            for choice in [
                                SeekbarBehaviour::Free,
                                SeekbarBehaviour::Locked,
                                SeekbarBehaviour::UntilTimelineEnds,
                            ] {
                                ui.selectable_value(&mut mode, choice, seekbar_label(choice))
                                    .on_hover_text(seekbar_hint(choice));
                            }
                        })
                        .response
                        .on_hover_text(seekbar_hint(mode))
                        .on_disabled_hover_text(disabled_tip);
                    });
                    if mode != editor.ui.course_settings.seekbar {
                        editor.ui.course_settings.seekbar = mode;
                        update_player = true;
                    }
                });
                form.row(ui, "Next unlocks", |ui, _| {
                    let current = editor.ui.course_settings.next_requires.clone();
                    if let Some(Some(rule)) = next_rule_editor(
                        ui,
                        "course_next_requires",
                        Some(&current),
                        false,
                        false,
                        &bool_names,
                    ) {
                        editor.ui.course_settings.next_requires = rule;
                        update_player = true;
                    }
                });
                clear_override = clear_override.or(slide_override_note(
                    ui,
                    &editor.session.course,
                    PlayerOverrideField::NextRequires,
                ));

                ui.add_space(8.0);
                ui.separator();

                // Quiz Settings
                ui.heading("Quiz settings");
                if ui
                    .checkbox(
                        &mut editor.ui.course_settings.quiz_enabled,
                        "Enable quiz grading and passing percentage",
                    )
                    .changed()
                {
                    update_quiz = true;
                }

                if editor.ui.course_settings.quiz_enabled {
                    form.row(ui, "Passing score", |ui, _| {
                        let pr = ui.add(
                            DragValue::new(&mut editor.ui.course_settings.passing_percent)
                                .speed(1.0)
                                .range(0..=100)
                                .suffix("%"),
                        );
                        let enter = pr.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if (pr.drag_stopped() || pr.lost_focus() || enter)
                            && Some(editor.ui.course_settings.passing_percent)
                                != editor.session.course.quiz.map(|q| q.passing_percent)
                        {
                            update_quiz = true;
                        }
                    });
                }

                ui.add_space(8.0);
                ui.separator();

                // Tracking
                ui.heading(crate::tl!("Tracking"));
                crate::tracking_ui::show_tracking_controls(
                    editor,
                    ui,
                    true,
                    "course_settings_tracking",
                );

                ui.add_space(8.0);
                ui.separator();

                // Background Audio
                ui.heading("Background audio");
                if let Some(src) = editor.ui.course_settings.background_audio_src.clone() {
                    ui.horizontal(|ui| {
                        ui.label(src);
                        if crate::quiet_button(
                            ui,
                            "Replace…",
                            crate::QuietTone::Secondary,
                            true,
                        )
                        .clicked()
                        {
                            pick_background_audio = true;
                        }
                        if crate::quiet_button(ui, "Remove", crate::QuietTone::Danger, true)
                            .clicked()
                        {
                            remove_background_audio = true;
                        }
                    });
                    form.row(ui, "Volume", |ui, rem_w| {
                        let response = ui.add_sized(
                            [rem_w, coursecraft_render::CONTROL_COMPACT],
                            egui::Slider::new(
                                &mut editor.ui.course_settings.background_volume,
                                0.0..=100.0,
                            )
                            .suffix("%")
                            .max_decimals(0),
                        );
                        editor.ui.course_settings.background_volume_rect = Some(response.rect);
                        let enter =
                            response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if response.drag_stopped() || response.lost_focus() || enter {
                            let new_volume =
                                (editor.ui.course_settings.background_volume / 100.0)
                                    .clamp(0.0, 1.0);
                            if editor
                                .session
                                .course
                                .background_audio
                                .as_ref()
                                .is_some_and(|audio| (audio.volume - new_volume).abs() > 0.001)
                            {
                                update_background_volume = true;
                            }
                        }
                    });
                    if ui
                        .checkbox(&mut editor.ui.course_settings.background_loop, "Loop")
                        .changed()
                    {
                        update_background_loop = true;
                    }
                } else {
                    if crate::quiet_primary_button(ui, "Choose…").clicked() {
                        pick_background_audio = true;
                    }
                    ui.label(
                        egui::RichText::new("Plays under every slide.")
                            .color(coursecraft_render::palette(ui.ctx()).text_secondary)
                            .size(coursecraft_render::SIZE_SMALL),
                    );
                }

                ui.add_space(8.0);
                ui.separator();
                let open_resources = std::mem::take(&mut editor.ui.course_settings.open_resources);
                resource_actions = editor.ui.course_settings.resources.show(ui, open_resources);
            });
            dialog.footer(|ui| {
                ui.add_space(coursecraft_render::SPACE_SM);
                ui.horizontal(|ui| {
                    if crate::primary_button(ui, "Close").clicked() {
                        close = true;
                    }
                });
            });
        });

    if update_course {
        let tag = editor.ui.course_settings.language.trim().to_owned();
        let language = if tag.is_empty() {
            serde_json::Value::Null
        } else if coursecraft_course::is_language_tag_valid(&tag) {
            serde_json::Value::String(tag)
        } else {
            // Leave an invalid draft alone; the field shows the error.
            editor
                .session
                .course
                .language
                .clone()
                .map_or(serde_json::Value::Null, serde_json::Value::String)
        };
        editor.run(
            "course.update",
            serde_json::json!({
                "title": editor.ui.course_settings.title.trim(),
                "id": editor.ui.course_settings.id.trim(),
                "language": language,
            }),
        );
    }
    if update_player {
        editor.run(
            "course.set_player",
            serde_json::json!({
                "show_bar": editor.ui.course_settings.show_bar,
                "show_nav": editor.ui.course_settings.show_nav,
                "show_menu": editor.ui.course_settings.show_menu,
                "show_seekbar": editor.ui.course_settings.show_seekbar,
                "show_speed": editor.ui.course_settings.show_speed,
                "show_fullscreen": editor.ui.course_settings.show_fullscreen,
                "show_transcript": editor.ui.course_settings.show_transcript,
                "show_title": editor.ui.course_settings.show_title,
                "show_volume": editor.ui.course_settings.show_volume,
                "show_captions": editor.ui.course_settings.show_captions,
                "navigation": editor.ui.course_settings.navigation,
                "next_requires": editor.ui.course_settings.next_requires,
                "seekbar": editor.ui.course_settings.seekbar,
            }),
        );
    }
    if update_quiz {
        if editor.ui.course_settings.quiz_enabled {
            editor.run(
                "course.set_quiz",
                serde_json::json!({
                    "passing_percent": editor.ui.course_settings.passing_percent,
                }),
            );
        } else {
            editor.run(
                "course.set_quiz",
                serde_json::json!({
                    "passing_percent": serde_json::Value::Null,
                }),
            );
        }
    }

    if pick_background_audio {
        editor.request(crate::EditorRequest::PickBackgroundAudio);
    }
    if remove_background_audio {
        editor.run(
            "course.set_background_audio",
            serde_json::json!({ "src": null }),
        );
    }
    if update_background_volume {
        let volume = (editor.ui.course_settings.background_volume / 100.0).clamp(0.0, 1.0);
        editor.run(
            "course.set_background_audio",
            serde_json::json!({ "volume": volume }),
        );
    }
    if update_background_loop {
        editor.run(
            "course.set_background_audio",
            serde_json::json!({ "loop": editor.ui.course_settings.background_loop }),
        );
    }

    if let Some(field) = clear_override {
        editor.run(
            "course.clear_slide_player_overrides",
            serde_json::json!({ "field": field.key() }),
        );
    }
    if let Some(logo) = logo_choice {
        editor.run("course.set_player", serde_json::json!({ "logo": logo }));
    }
    for (id, params) in resource_actions.runs {
        editor.run(id, params);
    }
    if let Some(resource) = resource_actions.pick_file {
        editor.request(crate::EditorRequest::PickResourceFile { resource });
    }

    if add_finish {
        editor.run("course.add_finish_button", serde_json::json!({}));
    }

    if close || !open {
        editor.ui.show_course_settings_window = false;
    }
}
