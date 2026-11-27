//! The File page (docs/ui/ribbon.md › File): a full-window page opened from the
//! ribbon's File tab, in place of the menu bar's File menu.
//!
//! A column of entries on the left: some act at once (Save, Publish…), others
//! (Recent, Import, Export, Translation, Fonts) show their choices on the right.
//! Every action closes the page; Esc and Back return to the editor.
//!
//! It is also the page CourseCraft opens on (the welcome screen): its Start
//! section is the start page (`welcome::start_content`), and until a course is
//! open the entries that need one are drawn disabled.

use coursecraft_render::Icon;
use egui::{Align, Button, Layout, RichText, Ui, vec2};

use crate::{Editor, EditorRequest, ReviewFormat};

/// The entry whose choices fill the right side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FileSection {
    /// New, open, templates, examples and recent courses (the start page).
    #[default]
    Start,
    Recent,
    Import,
    Export,
    Translation,
    Fonts,
}

impl FileSection {
    fn label(self) -> &'static str {
        match self {
            Self::Start => "Start",
            Self::Recent => "Recent",
            Self::Import => "Import",
            Self::Export => "Export",
            Self::Translation => "Translation",
            Self::Fonts => "Fonts",
        }
    }

    fn icon(self) -> Icon {
        match self {
            Self::Start => Icon::House,
            Self::Recent => Icon::Clock,
            Self::Import => Icon::ArrowSquareOut,
            Self::Export => Icon::Export,
            Self::Translation => Icon::BookOpen,
            Self::Fonts => Icon::TextAa,
        }
    }
}

/// Whether the page is open, and which section it shows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FilePage {
    pub open: bool,
    pub section: FileSection,
}

/// Width of the left column.
const COLUMN_W: f32 = 240.0;

/// What a click on the page asks for, run once the page is drawn.
enum Pick {
    Request(EditorRequest),
    Run(fn(&mut Editor)),
    Back,
}

/// Open the page (the File tab).
pub fn open(editor: &mut Editor) {
    editor.ui.file_page.open = true;
}

/// The welcome screen is up and no course has been opened or made yet.
fn no_course(editor: &Editor) -> bool {
    editor.ui.show_welcome && !editor.ui.has_loaded_course
}

/// Why an entry that needs a course is off on the welcome screen.
const NEEDS_COURSE: &str = "Open or create a course first";

/// The requests that load a course: on the welcome screen they close it.
fn loads_course(request: &EditorRequest) -> bool {
    matches!(
        request,
        EditorRequest::New
            | EditorRequest::Open
            | EditorRequest::OpenFolder
            | EditorRequest::OpenRecent(_)
            | EditorRequest::PickPptx
    )
}

/// Esc acts as Back: it closes the File page, and the start page when a course is
/// open behind it. A menu, a popup or a dialog that is open takes the Esc first.
/// Returns whether the page was left.
pub fn esc_back(editor: &mut Editor, ctx: &egui::Context) -> bool {
    let can_go_back = if editor.ui.show_welcome {
        editor.ui.has_loaded_course
    } else {
        editor.ui.file_page.open
    };
    if !can_go_back || egui::Popup::is_any_open(ctx) || editor.modal_dialog_open() {
        return false;
    }
    if !ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
        return false;
    }
    editor.ui.file_page.open = false;
    editor.ui.show_welcome = false;
    true
}

/// Check Course: run the check and show its problems.
fn check_course(editor: &mut Editor) {
    editor.refresh_validation();
    editor.ui.show_validation_window = true;
}

/// Draw the page over the editor's work area.
pub fn show(editor: &mut Editor, ui: &mut Ui, requests: &mut Vec<EditorRequest>) {
    let welcome = editor.ui.show_welcome;
    // Esc goes back to the editor; on the welcome screen with no course there is none.
    if esc_back(editor, ui.ctx()) {
        return;
    }
    let p = coursecraft_render::palette(ui.ctx());
    let rect = ui.available_rect_before_wrap();
    let column = egui::Rect::from_min_size(rect.min, vec2(COLUMN_W, rect.height()));
    ui.painter().rect_filled(column, 0.0, p.accent);
    ui.painter().rect_filled(
        egui::Rect::from_min_max(egui::pos2(column.max.x, rect.min.y), rect.max),
        0.0,
        p.app,
    );

    let mut pick: Option<Pick> = None;
    let mut column_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(column.shrink2(vec2(10.0, 12.0)))
            .layout(Layout::top_down_justified(Align::Min)),
    );
    egui::ScrollArea::vertical()
        .id_salt("file_page_column")
        .show(&mut column_ui, |ui| {
            column_entries(editor, ui, ON_ACCENT, &mut pick);
        });

    let detail = egui::Rect::from_min_max(egui::pos2(column.max.x, rect.min.y), rect.max)
        .shrink2(vec2(32.0, 24.0));
    let mut detail_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(detail)
            .layout(Layout::top_down(Align::Min)),
    );
    let asked = requests.len();
    egui::ScrollArea::vertical()
        .id_salt("file_page_detail")
        .show(&mut detail_ui, |ui| {
            section_detail(editor, ui, &mut pick, requests);
        });
    ui.allocate_rect(rect, egui::Sense::hover());
    // The start page acts by itself (a new course, an example, a template). A request
    // that loads a course keeps the page open: the page closes when the course has
    // really loaded, so a cancelled file dialog leaves it where it was.
    if requests.len() > asked && !requests[asked..].iter().any(loads_course) {
        editor.ui.file_page.open = false;
        // Anything but a load leaves the start page for the editor (a dialog it opens
        // is drawn there), as long as there is a course to go back to.
        if welcome && editor.ui.has_loaded_course {
            editor.ui.show_welcome = false;
        }
    }

    if let Some(pick) = pick {
        let loads = matches!(&pick, Pick::Request(r) if loads_course(r));
        if !loads {
            editor.ui.file_page.open = false;
            // Course Info, Version History and the like need the editor: leave the
            // start page first, then open the dialog.
            if welcome && (matches!(pick, Pick::Back) || editor.ui.has_loaded_course) {
                editor.ui.show_welcome = false;
            }
        }
        match pick {
            Pick::Request(r) => requests.push(r),
            Pick::Run(f) => f(editor),
            Pick::Back => {}
        }
    }
}

fn desktop_file_ops(editor: &Editor) -> bool {
    editor.file_ops_available && !cfg!(target_arch = "wasm32")
}

/// Text on the accent column and the File tab.
pub const ON_ACCENT: egui::Color32 = egui::Color32::WHITE;

/// One entry of the left column: icon and name, left-aligned, white on the accent.
fn entry(
    ui: &mut Ui,
    ink: egui::Color32,
    icon: Icon,
    label: &str,
    selected: bool,
) -> egui::Response {
    let label = tl!(label);
    let (rect, resp) =
        ui.allocate_exact_size(vec2(ui.available_width(), 32.0), egui::Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), label));
    let ink = if ui.is_enabled() {
        ink
    } else {
        ink.gamma_multiply(0.45)
    };
    if selected || (resp.hovered() && ui.is_enabled()) {
        let alpha = if selected { 0.22 } else { 0.12 };
        ui.painter()
            .rect_filled(rect, 4.0, egui::Color32::WHITE.gamma_multiply(alpha));
    }
    let painter = ui.painter();
    painter.text(
        rect.left_center() + vec2(12.0, 0.0),
        egui::Align2::LEFT_CENTER,
        icon.codepoint().to_string(),
        egui::FontId::new(16.0, egui::FontFamily::Name("phosphor".into())),
        ink,
    );
    painter.text(
        rect.left_center() + vec2(40.0, 0.0),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(14.0),
        ink,
    );
    resp
}

fn column_entries(editor: &mut Editor, ui: &mut Ui, ink: egui::Color32, pick: &mut Option<Pick>) {
    let no_course = no_course(editor);
    let file_ops = editor.file_ops_available;
    let desktop = desktop_file_ops(editor);
    let disabled = tl!(crate::menu::FILE_OPS_DISABLED_MSG);
    let needs = tl!(NEEDS_COURSE);
    // What an entry that needs a course says on the welcome screen.
    let course_why = |why: &'static str| if no_course { needs } else { why };

    let back = if editor.ui.show_welcome {
        "Back to course"
    } else {
        "Back"
    };
    let resp = ui
        .add_enabled_ui(!no_course, |ui| {
            entry(ui, ink, Icon::ArrowLeft, back, false)
        })
        .inner
        .on_disabled_hover_text(needs);
    if resp.clicked() {
        *pick = Some(Pick::Back);
    }
    ui.add_space(8.0);
    let mut act = |ui: &mut Ui, icon: Icon, label: &str, enabled: bool, why: &str, what: Pick| {
        let resp = ui
            .add_enabled_ui(enabled, |ui| entry(ui, ink, icon, label, false))
            .inner
            .on_disabled_hover_text(why);
        if resp.clicked() {
            *pick = Some(what);
        }
    };
    act(
        ui,
        Icon::FilePlus,
        "New",
        true,
        "",
        Pick::Request(EditorRequest::New),
    );
    act(
        ui,
        Icon::FolderOpen,
        "Open…",
        file_ops,
        disabled,
        Pick::Request(EditorRequest::Open),
    );
    act(
        ui,
        Icon::FolderOpen,
        "Open Folder…",
        desktop,
        disabled,
        Pick::Request(EditorRequest::OpenFolder),
    );
    ui.add_space(6.0);
    act(
        ui,
        Icon::FloppyDisk,
        "Save",
        file_ops && !no_course,
        course_why(disabled),
        Pick::Request(EditorRequest::Save),
    );
    act(
        ui,
        Icon::FloppyDisk,
        "Save As…",
        file_ops && !no_course,
        course_why(disabled),
        Pick::Request(EditorRequest::SaveAs),
    );
    act(
        ui,
        Icon::Stack,
        "Save As Project Folder…",
        desktop && !no_course,
        course_why(disabled),
        Pick::Request(EditorRequest::SaveAsFolder),
    );
    let template_ok = !no_course
        && editor
            .session
            .course
            .play_slide(&editor.selection.slide)
            .is_some();
    act(
        ui,
        Icon::Layout,
        "Save Slide as Template",
        template_ok,
        course_why(tl!("Select a slide first")),
        Pick::Run(save_template),
    );
    let history_ok = desktop && editor.ui.versions.has_file && !no_course;
    let history_why = if desktop {
        tl!("Save the course first to start its history.")
    } else {
        disabled
    };
    act(
        ui,
        Icon::ArrowCounterClockwise,
        "Version History…",
        history_ok,
        course_why(history_why),
        Pick::Request(EditorRequest::ShowVersionHistory),
    );
    ui.add_space(6.0);
    for section in [
        FileSection::Start,
        FileSection::Recent,
        FileSection::Import,
        FileSection::Export,
        FileSection::Translation,
        FileSection::Fonts,
    ] {
        let selected = editor.ui.file_page.section == section;
        let enabled = !(no_course
            && matches!(
                section,
                FileSection::Export | FileSection::Translation | FileSection::Fonts
            ));
        let resp = ui
            .add_enabled_ui(enabled, |ui| {
                entry(ui, ink, section.icon(), section.label(), selected)
            })
            .inner
            .on_disabled_hover_text(needs);
        if resp.clicked() {
            editor.ui.file_page.section = section;
        }
    }
    ui.add_space(6.0);
    act(
        ui,
        Icon::UploadSimple,
        "Publish…",
        file_ops && !no_course,
        course_why(disabled),
        Pick::Run(Editor::open_publish_dialog),
    );
    act(
        ui,
        Icon::Info,
        "Course Info…",
        !no_course,
        needs,
        Pick::Run(|e| e.ui.show_course_settings_window = true),
    );
    act(
        ui,
        Icon::ListChecks,
        "Check Course",
        !no_course,
        needs,
        Pick::Run(check_course),
    );
    act(
        ui,
        Icon::Gear,
        "Preferences…",
        true,
        "",
        Pick::Run(Editor::open_preferences),
    );
    ui.add_space(6.0);
    act(
        ui,
        Icon::X,
        "Close",
        false,
        &craft_i18n::fmt(
            tl!("Not available yet: {reason}"),
            &[("reason", tl!("closing a course (New replaces it)"))],
        ),
        Pick::Back,
    );
    act(
        ui,
        Icon::XCircle,
        "Quit",
        true,
        "",
        Pick::Request(EditorRequest::Quit),
    );
}

fn save_template(editor: &mut Editor) {
    let slide = editor.selection.slide.clone();
    editor.run("template.save", serde_json::json!({ "slide": slide }));
}

/// A choice on the right side: a wide button with a line of help.
fn choice(ui: &mut Ui, label: &str, help: &str, enabled: bool, why: &str) -> bool {
    let (label, help) = (tl!(label), tl!(help));
    choice_text(ui, label, help, enabled, why)
}

/// [`choice`] for text that is not interface text: a file name and its path. They never go
/// through the lookup (a file called `Save` stays `Save`), and read left to right (`N106`).
fn choice_text(ui: &mut Ui, label: &str, help: &str, enabled: bool, why: &str) -> bool {
    let resp = ui
        .add_enabled(
            enabled,
            Button::new(RichText::new(label).size(15.0)).min_size(vec2(320.0, 34.0)),
        )
        .on_disabled_hover_text(why);
    if !help.is_empty() {
        ui.label(coursecraft_render::small(help));
    }
    ui.add_space(8.0);
    resp.clicked()
}

fn section_detail(
    editor: &mut Editor,
    ui: &mut Ui,
    pick: &mut Option<Pick>,
    requests: &mut Vec<EditorRequest>,
) {
    let section = editor.ui.file_page.section;
    if section == FileSection::Start {
        ui.set_max_width(720.0);
        crate::welcome::start_content(editor, ui, requests, false);
        return;
    }
    ui.label(coursecraft_render::heading(tl!(section.label())));
    ui.add_space(12.0);
    let desktop = desktop_file_ops(editor);
    let disabled = tl!(crate::menu::FILE_OPS_DISABLED_MSG);
    match section {
        FileSection::Start => {}
        FileSection::Recent => {
            if !desktop {
                ui.label(disabled);
            } else if editor.recent_files.is_empty() {
                ui.label(tl!("No recent courses yet."));
            }
            if desktop {
                for (idx, full_path) in editor.recent_files.iter().enumerate() {
                    let name = std::path::Path::new(full_path)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or(full_path);
                    let preview = editor
                        .recent_previews
                        .iter()
                        .find(|(path, _)| path == full_path)
                        .map(|(_, preview)| preview);
                    let clicked = ui
                        .horizontal_top(|ui| {
                            let (thumb, _) =
                                ui.allocate_exact_size(vec2(96.0, 54.0), egui::Sense::hover());
                            crate::welcome::paint_recent_thumbnail(ui, preview, thumb);
                            ui.add_space(8.0);
                            ui.vertical(|ui| {
                                choice_text(
                                    ui,
                                    &craft_i18n::ltr(name),
                                    &craft_i18n::ltr(full_path),
                                    true,
                                    "",
                                )
                            })
                            .inner
                        })
                        .inner;
                    if clicked {
                        *pick = Some(Pick::Request(EditorRequest::OpenRecent(idx)));
                    }
                }
            }
            ui.add_space(8.0);
            if choice(
                ui,
                "Browse for more…",
                "",
                editor.file_ops_available,
                disabled,
            ) {
                *pick = Some(Pick::Request(EditorRequest::Open));
            }
        }
        FileSection::Import => {
            if choice(ui, "PowerPoint…", "Slides from a .pptx file.", true, "") {
                *pick = Some(Pick::Request(EditorRequest::PickPptx));
            }
            if choice(
                ui,
                "Questions from CSV…",
                "Question slides from a spreadsheet.",
                true,
                "",
            ) {
                *pick = Some(Pick::Request(EditorRequest::ImportQuestionsCsv));
            }
            if choice(
                ui,
                "Course from Document…",
                "Slides from a text or Markdown document.",
                true,
                "",
            ) {
                *pick = Some(Pick::Run(Editor::open_document_dialog));
            }
            if choice(
                ui,
                "Outline…",
                "Scenes and slides from an outline.",
                true,
                "",
            ) {
                *pick = Some(Pick::Run(Editor::open_outline_dialog));
            }
        }
        FileSection::Export => {
            if choice(ui, "Video (MP4)…", "The course as a video.", true, "") {
                *pick = Some(Pick::Run(Editor::open_export_dialog));
            }
            if choice(
                ui,
                "PDF for review…",
                "Every slide with its notes.",
                true,
                "",
            ) {
                *pick = Some(Pick::Run(|e| e.open_export_review(ReviewFormat::Pdf)));
            }
            if choice(
                ui,
                "Word for review…",
                "Every slide with its notes.",
                true,
                "",
            ) {
                *pick = Some(Pick::Run(|e| e.open_export_review(ReviewFormat::Word)));
            }
        }
        FileSection::Translation => {
            if choice(
                ui,
                "Export Translation…",
                "The course text as XLIFF for a translator.",
                desktop,
                disabled,
            ) {
                *pick = Some(Pick::Run(|e| e.ui.show_export_translation = true));
            }
            if choice(
                ui,
                "Import Translation…",
                "Put a translated XLIFF file back.",
                desktop,
                disabled,
            ) {
                *pick = Some(Pick::Request(EditorRequest::ImportTranslation));
            }
            if choice(
                ui,
                "Translate with AI…",
                "Through your AI settings.",
                true,
                "",
            ) {
                *pick = Some(Pick::Run(Editor::open_translate_dialog));
            }
        }
        FileSection::Fonts => {
            if choice(
                ui,
                "Add Font…",
                "Embed a font file in the course.",
                true,
                "",
            ) {
                *pick = Some(Pick::Request(EditorRequest::PickFonts));
            }
            if choice(
                ui,
                "Manage Fonts…",
                "The course's embedded fonts.",
                true,
                "",
            ) {
                *pick = Some(Pick::Run(|e| e.ui.show_fonts_window = true));
            }
        }
    }
}
